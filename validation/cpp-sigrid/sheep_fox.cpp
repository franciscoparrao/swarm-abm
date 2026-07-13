// SIGRID en C++ — Hito 1: subconjunto de screening (ovejas + zorros culpeo,
// sin perros/liebres/chillas). Port fiel del modelo swarm-abm committeado
// (models/sigrid/src/lib.rs @ HEAD), que es el oráculo de validación.
//
// Determinismo: sembrado (mt19937_64). El RNG difiere del ChaCha8 de swarm-abm,
// así que la validación es DISTRIBUCIONAL (Pearson/Spearman sobre puntos de
// parámetros × semillas), no bit-exacta — misma metodología que la paridad vs
// Mesa (models/sigrid/PARITY.md).
//
// Semántica replicada del motor: el índice espacial es una INSTANTÁNEA tomada al
// inicio del paso (before_step); las consultas ven posiciones y estado de inicio
// de tick, mientras las mutaciones (mover, matar, miedo) ocurren sobre el arreglo
// vivo. Activación aleatoria (orden barajado por paso con el RNG maestro).
//
// Uso: sheep_fox [--days N] [--seed N] [--seeds N] [--sheep-density D]
//                [--fox-density D] [--fox-eff E] [--lamb-prop P]
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <cmath>
#include <cstdint>
#include <vector>
#include <random>
#include <algorithm>

// ---- constantes (del SIGRID committeado) ----
static const double CELL_SIZE = 30.0;   // resolución del raster
static const double SPACE_CELL = 60.0;  // celda del índice espacial
static const double TAU = 6.283185307179586;
static const double SHEEP_SPEED = 50.0, SHEEP_FLEE_SPEED = 100.0, SHEEP_PERCEPTION_RADIUS = 100.0, SHEEP_ADULT_VULN = 0.4;
static const double LAMB_SPEED = 35.0, LAMB_FLEE_SPEED = 60.0, LAMB_PERCEPTION_RADIUS = 50.0, LAMB_FEAR_DECAY = 0.05;
static const double LAMB_MATURATION_DAYS = 120.0, LAMB_VULN = 0.85;
static const double FOX_SPEED_WALK = 500.0, FOX_DETECTION_RADIUS = 300.0, FOX_TERRITORY_RADIUS = 6135.0;
static const double HUNGER_THRESHOLD = 0.3, BASE_RISK_AVERSION = 0.6, FOX_ATTACK_RADIUS = 50.0;
static const double FOX_ACT_PEAK = 0.0, FOX_ACT_AMP = 0.90, FOX_ACT_SIGMA = 4.5, FOX_ACT_BASE = 0.05; // sin perro
static const double FOX_P_MIN = 0.02, FOX_P_MAX = 0.35;

enum Species { SHEEP = 0, FOX = 1 };

struct Animal {
    int species; double x, y; bool alive; double energy, age_days, fear;
    bool is_lamb; double vulnerability; int mother; // índice o -1
    double hunger, risk_aversion, predation_eff;
};

struct Snap { double x, y; int species; bool alive, is_lamb; double vulnerability, energy; int mother; };

struct Raster {
    int cols, rows; double height; std::vector<double> data;
    double get(double px, double py) const {
        int col = (int)std::floor(px / CELL_SIZE); if (col < 0) col = 0; if (col > cols - 1) col = cols - 1;
        int row = (int)std::floor((height - py) / CELL_SIZE); if (row < 0) row = 0; if (row > rows - 1) row = rows - 1;
        return data[(size_t)row * cols + col];
    }
};

struct Rng { // envoltorio sobre mt19937_64 con helpers estilo swarm-abm
    std::mt19937_64 g;
    explicit Rng(uint64_t s) : g(s) {}
    double unit() { return std::uniform_real_distribution<double>(0.0, 1.0)(g); }        // [0,1)
    double range(double a, double b) { return std::uniform_real_distribution<double>(a, b)(g); }
    double normal(double std) { double u1 = range(1e-12, 1.0), u2 = unit(); return std::sqrt(-2.0 * std::log(u1)) * std::cos(TAU * u2) * std; }
};

struct Params { double width = 2000, height = 2000, sheep_density = 0.96, fox_density = 8.4,
                fox_predation_effectiveness = 0.14, lamb_proportion = 0.2; };

// ---- espacio: índice de celdas sobre la instantánea ----
struct Grid {
    int nx, ny; double csz, w, h; std::vector<int> head; std::vector<int> nxt;
    void build(const std::vector<Snap>& s, double W, double H) {
        w = W; h = H; csz = SPACE_CELL;
        nx = std::max(1, (int)(W / csz)); ny = std::max(1, (int)(H / csz));
        head.assign((size_t)nx * ny, -1); nxt.assign(s.size(), -1);
        for (size_t i = 0; i < s.size(); ++i) { int c = cell(s[i].x, s[i].y); nxt[i] = head[c]; head[c] = (int)i; }
    }
    int cell(double px, double py) const {
        int cx = (int)(px / csz); if (cx < 0) cx = 0; if (cx > nx - 1) cx = nx - 1;
        int cy = (int)(py / csz); if (cy < 0) cy = 0; if (cy > ny - 1) cy = ny - 1;
        return cy * nx + cx;
    }
    // aplica f(snap, dist) a los vecinos a distancia <= radius
    template <class F> void within(const std::vector<Snap>& s, double px, double py, double radius, F f) const {
        int cx = (int)(px / csz); if (cx < 0) cx = 0; if (cx > nx - 1) cx = nx - 1;
        int cy = (int)(py / csz); if (cy < 0) cy = 0; if (cy > ny - 1) cy = ny - 1;
        int cr = (int)(radius / csz) + 1;
        for (int dy = -cr; dy <= cr; ++dy) for (int dx = -cr; dx <= cr; ++dx) {
            int ax = cx + dx, ay = cy + dy; if (ax < 0 || ay < 0 || ax >= nx || ay >= ny) continue;
            for (int j = head[(size_t)ay * nx + ax]; j != -1; j = nxt[j]) {
                double ddx = s[j].x - px, ddy = s[j].y - py; double d = std::sqrt(ddx * ddx + ddy * ddy);
                if (d <= radius) f(s[j], d);
            }
        }
    }
};

static double clampd(double v, double lo, double hi) { return v < lo ? lo : (v > hi ? hi : v); }

// ---- rasters sintéticos (réplica de build_rasters) ----
static void build_rasters(double W, double H, Rng& rng, Raster& quality, Raster& cover) {
    int cols = (int)std::ceil(W / CELL_SIZE), rows = (int)std::ceil(H / CELL_SIZE);
    quality = {cols, rows, H, std::vector<double>((size_t)cols * rows)};
    cover = {cols, rows, H, std::vector<double>((size_t)cols * rows)};
    for (int r = 0; r < rows; ++r) for (int c = 0; c < cols; ++c) {
        double x = (double)c / cols, y = (double)r / rows;
        double elev = 0.5 + 0.5 * (std::sin(x * 6.0) * std::cos(y * 6.0) + 0.5 * std::sin(x * 12.0) * std::cos(y * 12.0)) / 1.5;
        double cov = clampd(0.7 - elev * 0.4 + rng.range(-0.1, 0.1), 0.0, 1.0);
        double ndvi = clampd((1.0 - cov) * 0.5 + (1.0 - elev) * 0.5, 0.0, 1.0);
        double qual = clampd(ndvi * (1.0 + rng.range(-0.1, 0.1)), 0.0, 1.0);
        cover.data[(size_t)r * cols + c] = cov; quality.data[(size_t)r * cols + c] = qual;
    }
}

struct Model {
    std::vector<Animal> agents; std::vector<Snap> snap; Grid grid;
    Raster veg_quality, veg_cover; Params params;
    int current_hour = 0; uint64_t step_count = 0;
    long sheep_killed = 0, predation_attempts = 0; int n_sheep_initial = 0;

    void before_step() {
        current_hour = (int)(step_count % 24); step_count++;
        snap.resize(agents.size());
        for (size_t i = 0; i < agents.size(); ++i) {
            const Animal& a = agents[i];
            snap[i] = {a.x, a.y, a.species, a.alive, a.is_lamb, a.vulnerability, a.energy, a.mother};
        }
        grid.build(snap, params.width, params.height);
    }
};

// dirección unitaria hacia el vecino de mayor quality (8 offsets * 50)
static void food_direction(const Raster& q, double px, double py, double& dx, double& dy) {
    double best = q.get(px, py); dx = 0; dy = 0;
    for (int k = 0; k < 8; ++k) { double a = TAU * k / 8.0, ox = std::cos(a), oy = std::sin(a);
        double v = q.get(px + ox * 50.0, py + oy * 50.0); if (v > best) { best = v; dx = ox; dy = oy; } }
}
static void norm(double& x, double& y) { double l = std::sqrt(x * x + y * y); if (l > 1e-12) { x /= l; y /= l; } else { x = 0; y = 0; } }

// ---- oveja ----
static void step_sheep(Model& m, int i, Rng& rng) {
    Animal& s = m.agents[i];
    double decay = s.is_lamb ? LAMB_FEAR_DECAY : 0.1;
    s.fear = std::max(s.fear - decay, 0.0); s.age_days += 1.0 / 24.0;
    if (s.fear > 0.7) {
        double radius = s.is_lamb ? LAMB_PERCEPTION_RADIUS : SHEEP_PERCEPTION_RADIUS;
        double pbest = 1e300, ppx = 0, ppy = 0; bool found = false;
        m.grid.within(m.snap, s.x, s.y, radius, [&](const Snap& sn, double d) {
            if (sn.alive && sn.species == FOX && d < pbest) { pbest = d; ppx = sn.x; ppy = sn.y; found = true; } });
        double sp = s.is_lamb ? LAMB_FLEE_SPEED : SHEEP_FLEE_SPEED;
        double dx, dy;
        if (found) { dx = s.x - ppx; dy = s.y - ppy; norm(dx, dy); }
        else { double a = rng.range(0.0, TAU); dx = std::cos(a); dy = std::sin(a); }
        s.x = clampd(s.x + dx * sp, 0, m.params.width); s.y = clampd(s.y + dy * sp, 0, m.params.height);
    } else {
        double p_move = 0.76 + 0.04; // sin perro
        if (rng.unit() < p_move) {
            // graze_and_move
            double vfx, vfy; food_direction(m.veg_quality, s.x, s.y, vfx, vfy);
            double vrx = 0, vry = 0, worst = m.veg_cover.get(s.x, s.y);
            for (int k = 0; k < 8; ++k) { double a = TAU * k / 8.0, ox = std::cos(a), oy = std::sin(a);
                double cov = m.veg_cover.get(s.x + ox * 50.0, s.y + oy * 50.0); if (cov > worst) { worst = cov; vrx = -ox; vry = -oy; } }
            double cx = 0, cy = 0, n = 0;
            m.grid.within(m.snap, s.x, s.y, std::max(SHEEP_PERCEPTION_RADIUS, 500.0), [&](const Snap& sn, double d) {
                if (!sn.alive) return;
                if (sn.species == SHEEP && d <= SHEEP_PERCEPTION_RADIUS) { cx += sn.x; cy += sn.y; n += 1; } });
            double vcx = 0, vcy = 0;
            if (n > 0) { vcx = cx / n - s.x; vcy = cy / n - s.y; norm(vcx, vcy); }
            double dx = vfx * 0.3 + vrx * 0.2 + vcx * 0.2 + rng.normal(0.1);
            double dy = vfy * 0.3 + vry * 0.2 + vcy * 0.2 + rng.normal(0.1);
            norm(dx, dy);
            double sp = s.is_lamb ? LAMB_SPEED : SHEEP_SPEED;
            s.x = clampd(s.x + dx * sp, 0, m.params.width); s.y = clampd(s.y + dy * sp, 0, m.params.height);
        }
    }
    if (s.is_lamb && s.age_days > LAMB_MATURATION_DAYS) { s.is_lamb = false; s.vulnerability = SHEEP_ADULT_VULN; }
    double q = m.veg_quality.get(s.x, s.y);
    double gain = (s.is_lamb ? q : q * 2.0) - 1.0;
    s.energy = std::min(s.energy + gain - s.fear * 2.0, 100.0);
}

static double predation_probability(Model& m, int fox_i, int prey_i) {
    Animal& fox = m.agents[fox_i]; Animal& prey = m.agents[prey_i];
    double p_base = fox.predation_eff * prey.vulnerability;
    double m_cover = 0.05 * m.veg_cover.get(fox.x, fox.y);
    double nearby = 0;
    m.grid.within(m.snap, prey.x, prey.y, 50.0, [&](const Snap& sn, double) { if (sn.alive && sn.species == SHEEP) nearby += 1; });
    double m_group = -0.03 * std::min(nearby / 10.0, 1.0);
    double m_condition = 0.03 * (1.0 - prey.energy / 100.0);
    double m_mother = 0.0;
    if (prey.is_lamb && prey.mother >= 0) { Animal& mo = m.agents[prey.mother];
        if (mo.alive) { double d = std::sqrt((prey.x - mo.x) * (prey.x - mo.x) + (prey.y - mo.y) * (prey.y - mo.y));
            if (d < 20.0) m_mother = -0.12 * (1.0 - d / 20.0); } }
    return clampd(p_base + m_cover + m_group + m_condition + m_mother, FOX_P_MIN, FOX_P_MAX);
}

static void attempt_predation(Model& m, int fox_i, int prey_i, Rng& rng) {
    double p = predation_probability(m, fox_i, prey_i);
    m.predation_attempts++;
    bool success = rng.unit() < p;
    if (success) { m.agents[prey_i].alive = false; m.agents[fox_i].hunger = 0.0;
        if (m.agents[prey_i].species == SHEEP) m.sheep_killed++; }
    else { m.agents[prey_i].fear = 1.0; m.agents[fox_i].hunger = std::min(m.agents[fox_i].hunger + 0.05, 1.0); }
}

// ---- zorro ----
static void step_fox(Model& m, int i, Rng& rng) {
    Animal& f = m.agents[i];
    f.hunger = std::min(f.hunger + 0.01, 1.0);
    // fox_active (sin perro)
    double h = (double)m.current_hour; double raw = std::fabs(h - FOX_ACT_PEAK); double d = std::min(raw, 24.0 - raw);
    double level = std::min(FOX_ACT_BASE + FOX_ACT_AMP * std::exp(-(d * d) / (2.0 * FOX_ACT_SIGMA * FOX_ACT_SIGMA)), 1.0);
    if (rng.unit() >= level) return; // descanso
    if (f.hunger < HUNGER_THRESHOLD) { double a = rng.range(0.0, TAU), dd = rng.range(50.0, 200.0);
        f.x = clampd(f.x + std::cos(a) * dd, 0, m.params.width); f.y = clampd(f.y + std::sin(a) * dd, 0, m.params.height); return; }
    // hunt (sin perros, sin liebres): sin evitación de riesgo ni acecho.
    // Detección de presas: el snapshot está alineado 1:1 con `agents` (índice j),
    // así que recorremos las celdas vecinas y usamos j como id de agente.
    std::vector<int> prey;
    int gx = (int)(f.x / m.grid.csz); if (gx < 0) gx = 0; if (gx > m.grid.nx - 1) gx = m.grid.nx - 1;
    int gy = (int)(f.y / m.grid.csz); if (gy < 0) gy = 0; if (gy > m.grid.ny - 1) gy = m.grid.ny - 1;
    int cr = (int)(FOX_DETECTION_RADIUS / m.grid.csz) + 1;
    for (int dy = -cr; dy <= cr; ++dy) for (int dx = -cr; dx <= cr; ++dx) {
        int ax = gx + dx, ay = gy + dy; if (ax < 0 || ay < 0 || ax >= m.grid.nx || ay >= m.grid.ny) continue;
        for (int j = m.grid.head[(size_t)ay * m.grid.nx + ax]; j != -1; j = m.grid.nxt[j]) {
            const Snap& sn = m.snap[j];
            double ddx = sn.x - f.x, ddy = sn.y - f.y; if (std::sqrt(ddx * ddx + ddy * ddy) > FOX_DETECTION_RADIUS) continue;
            if (sn.alive && sn.species == SHEEP) prey.push_back(j);
        }
    }
    if (prey.empty()) { double a = rng.range(0.0, TAU);
        f.x = clampd(f.x + std::cos(a) * FOX_SPEED_WALK, 0, m.params.width); f.y = clampd(f.y + std::sin(a) * FOX_SPEED_WALK, 0, m.params.height); return; }
    // seleccionar por score (vuln, +0.2 cordero); sobre agentes VIVOS
    int best = prey[0]; double best_score = -1e300;
    for (int pid : prey) { Animal& pa = m.agents[pid]; if (!pa.alive) continue;
        double score = pa.vulnerability; if (pa.is_lamb) score += 0.2;
        if (score > best_score) { best_score = score; best = pid; } }
    if (!m.agents[best].alive) return;
    double tx = m.agents[best].x, ty = m.agents[best].y; // posición VIVA de la presa
    double to_x = tx - f.x, to_y = ty - f.y; double tolen = std::sqrt(to_x * to_x + to_y * to_y);
    double step = std::min(FOX_SPEED_WALK, tolen); double ux = to_x, uy = to_y; norm(ux, uy);
    f.x = clampd(f.x + ux * step, 0, m.params.width); f.y = clampd(f.y + uy * step, 0, m.params.height);
    double adx = f.x - tx, ady = f.y - ty;
    if (std::sqrt(adx * adx + ady * ady) < FOX_ATTACK_RADIUS) attempt_predation(m, i, best, rng); // sin perro: mata de inmediato
}

static double run_loss_rate(const Params& params, uint64_t seed, uint64_t n_days, long& killed, long& attempts) {
    Rng rng_build(seed);
    double area_ha = params.width * params.height / 10000.0;
    double area_km2 = params.width * params.height / 1000000.0;
    int n_sheep = std::max(1L, std::lround(params.sheep_density * area_ha));
    int n_lambs = (int)(n_sheep * params.lamb_proportion);
    int n_adults = n_sheep - n_lambs;
    int n_foxes = (int)std::lround(params.fox_density * area_km2);

    Model m; m.params = params; m.n_sheep_initial = n_sheep;
    build_rasters(params.width, params.height, rng_build, m.veg_quality, m.veg_cover);

    double fcx = params.width / 2.0, fcy = params.height / 2.0;
    std::vector<int> adult_ids;
    for (int k = 0; k < n_adults; ++k) {
        Animal a{}; a.species = SHEEP; a.alive = true; a.energy = 100; a.mother = -1;
        a.x = clampd(fcx + rng_build.range(-300, 300), 0, params.width); a.y = clampd(fcy + rng_build.range(-300, 300), 0, params.height);
        a.age_days = rng_build.range(365, 2000); a.vulnerability = SHEEP_ADULT_VULN;
        adult_ids.push_back((int)m.agents.size()); m.agents.push_back(a);
    }
    for (int k = 0; k < n_lambs; ++k) {
        int mother = adult_ids.empty() ? -1 : adult_ids[(int)rng_build.range(0, (double)adult_ids.size())];
        double bx = fcx, by = fcy; if (mother >= 0) { bx = m.agents[mother].x; by = m.agents[mother].y; }
        Animal a{}; a.species = SHEEP; a.alive = true; a.is_lamb = true; a.energy = 70; a.mother = mother;
        a.x = clampd(bx + rng_build.range(-20, 20), 0, params.width); a.y = clampd(by + rng_build.range(-20, 20), 0, params.height);
        a.age_days = rng_build.range(0, 30); a.vulnerability = LAMB_VULN;
        m.agents.push_back(a);
    }
    for (int k = 0; k < n_foxes; ++k) {
        Animal a{}; a.species = FOX; a.alive = true; a.energy = 100; a.mother = -1;
        a.x = rng_build.range(0, params.width); a.y = rng_build.range(0, params.height);
        a.hunger = rng_build.range(0.3, 0.7); a.risk_aversion = BASE_RISK_AVERSION + rng_build.range(-0.1, 0.1);
        a.predation_eff = params.fox_predation_effectiveness;
        m.agents.push_back(a);
    }

    // stepping: rng fresco sembrado con la misma semilla (como Simulation::new)
    Rng rng(seed);
    std::vector<int> order(m.agents.size()); for (size_t k = 0; k < order.size(); ++k) order[k] = (int)k;
    for (uint64_t step = 0; step < n_days * 24; ++step) {
        m.before_step();
        // activación aleatoria: barajar con el RNG maestro (Fisher-Yates, como shuffle de swarm-abm)
        for (size_t k = order.size(); k > 1; --k) { size_t j = (size_t)(rng.unit() * k); std::swap(order[k - 1], order[j]); }
        for (int idx : order) { if (!m.agents[idx].alive) continue;
            if (m.agents[idx].species == SHEEP) step_sheep(m, idx, rng); else step_fox(m, idx, rng); }
        // término anticipado si no quedan ovejas
        bool any_sheep = false; for (auto& a : m.agents) if (a.alive && a.species == SHEEP) { any_sheep = true; break; }
        if (!any_sheep) break;
    }
    killed = m.sheep_killed; attempts = m.predation_attempts;
    return (double)m.sheep_killed / std::max(1, m.n_sheep_initial) * 100.0;
}

int main(int argc, char** argv) {
    Params p; uint64_t days = 30, seed0 = 1000, n_seeds = 1;
    auto darg = [&](const char* n, double dv) -> double { for (int i = 1; i < argc - 1; ++i) if (!std::strcmp(argv[i], n)) return std::atof(argv[i + 1]); return dv; };
    days = (uint64_t)darg("--days", 30); seed0 = (uint64_t)darg("--seed", 1000); n_seeds = (uint64_t)darg("--seeds", 1);
    p.sheep_density = darg("--sheep-density", p.sheep_density);
    p.fox_density = darg("--fox-density", p.fox_density);
    p.fox_predation_effectiveness = darg("--fox-eff", p.fox_predation_effectiveness);
    p.lamb_proportion = darg("--lamb-prop", p.lamb_proportion);

    double sum = 0; int cnt = 0;
    for (uint64_t s = 0; s < n_seeds; ++s) {
        long killed = 0, attempts = 0;
        double lr = run_loss_rate(p, seed0 + s, days, killed, attempts);
        printf("  semilla %llu: loss_rate %.2f%% | matadas %ld | intentos %ld\n",
               (unsigned long long)(seed0 + s), lr, killed, attempts);
        sum += lr; cnt++;
    }
    printf("\nloss_rate medio %.2f%% sobre %llu semillas\n", sum / std::max(1, cnt), (unsigned long long)n_seeds);
    return 0;
}
