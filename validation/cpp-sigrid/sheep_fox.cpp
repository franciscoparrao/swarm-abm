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
#ifdef _OPENMP
#include <omp.h>
#endif

// ---- constantes (del SIGRID committeado) ----
static const double CELL_SIZE = 30.0;   // resolución del raster
static const double SPACE_CELL = 60.0;  // celda del índice espacial
static const double TAU = 6.283185307179586;
static const double SHEEP_SPEED = 50.0, SHEEP_FLEE_SPEED = 100.0, SHEEP_PERCEPTION_RADIUS = 100.0, SHEEP_ADULT_VULN = 0.4;
static const double LAMB_SPEED = 35.0, LAMB_FLEE_SPEED = 60.0, LAMB_PERCEPTION_RADIUS = 50.0, LAMB_FEAR_DECAY = 0.05;
static const double LAMB_MATURATION_DAYS = 120.0, LAMB_VULN = 0.85;
static const double FOX_SPEED_WALK = 500.0, FOX_DETECTION_RADIUS = 300.0, FOX_TERRITORY_RADIUS = 6135.0;
static const double HUNGER_THRESHOLD = 0.3, BASE_RISK_AVERSION = 0.6, FOX_ATTACK_RADIUS = 50.0;
static const double FOX_ACT_PEAK_ND = 0.0, FOX_ACT_AMP_ND = 0.90, FOX_ACT_SIGMA_ND = 4.5, FOX_ACT_BASE_ND = 0.05; // sin perro
static const double FOX_ACT_PEAK_WD = 1.5, FOX_ACT_AMP_WD = 0.45, FOX_ACT_SIGMA_WD = 3.0, FOX_ACT_BASE_WD = 0.30; // con perro
static const double FOX_P_MIN = 0.02, FOX_P_MAX = 0.35;
// Perros y disuasión
static const double DOG_PROXIMITY_VIGILANCE = 100.0, DOG_SPEED_PATROL = 300.0, DOG_SPEED_CHASE = 3000.0;
static const double DOG_DETECTION_RADIUS = 1200.0, DOG_CHASE_RADIUS = 200.0, DOG_PATROL_RADIUS = 250.0;
static const double DOG_DETER_RADIUS = 50.0, DOG_PROTECTION_STRENGTH = 0.20, DOG_AVOID_RADIUS = 500.0;
static const double DANGER_RADIUS = 400.0;
static const uint64_t DANGER_TTL = 168;
// Liebre (presa alternativa) y chilla (segundo depredador)
static const double HARE_SPEED_NORMAL = 100.0, HARE_SPEED_FLEE = 800.0, HARE_PERCEPTION_RADIUS = 80.0;
static const double HARE_MATURITY_AGE_H = 60.0 * 24.0, HARE_VULN_JUV = 0.9, HARE_VULN_MATURE = 0.6;
static const double CHILLA_TERRITORY_RADIUS = 4295.0;

enum Species { SHEEP = 0, FOX = 1, DOG = 2, HARE = 3 };

struct DangerZone { double x, y; uint64_t step; };

struct Animal {
    int species; double x, y; bool alive; double energy, age_days, fear;
    bool is_lamb; double vulnerability; int mother; // índice o -1
    double hunger, risk_aversion, predation_eff;
    bool is_chilla = false, mature = true; double territory_radius = 0.0;
    // zorro: memoria de zonas peligrosas + presa en acecho
    std::vector<DangerZone> danger_zones; int stalk_target = -1;
    // perro: patrullaje circular
    double patrol_angle = 0.0, patrol_cx = 0.0, patrol_cy = 0.0;
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

struct Rng { // envoltorio sobre mt19937_64 con helpers estilo swarm-abm (solo build)
    std::mt19937_64 g;
    explicit Rng(uint64_t s) : g(s) {}
    double unit() { return std::uniform_real_distribution<double>(0.0, 1.0)(g); }        // [0,1)
    double range(double a, double b) { return std::uniform_real_distribution<double>(a, b)(g); }
    double normal(double std) { double u1 = range(1e-12, 1.0), u2 = unit(); return std::sqrt(-2.0 * std::log(u1)) * std::cos(TAU * u2) * std; }
};

// RNG por-agente (splitmix64): barato de sembrar por (semilla, tick, id), lo que
// hace el resultado INDEPENDIENTE del orden de ejecución -> determinista bajo
// paralelismo (la idea de `child_rng` de swarm-abm). Mismos helpers.
struct PRng {
    uint64_t s;
    explicit PRng(uint64_t seed) : s(seed) {}
    uint64_t next() {
        uint64_t z = (s += 0x9E3779B97F4A7C15ULL);
        z = (z ^ (z >> 30)) * 0xBF58476D1CE4E5B9ULL;
        z = (z ^ (z >> 27)) * 0x94D049BB133111EBULL;
        return z ^ (z >> 31);
    }
    double unit() { return (double)(next() >> 11) * (1.0 / 9007199254740992.0); } // [0,1)
    double range(double a, double b) { return a + unit() * (b - a); }
    double normal(double std) { double u1 = range(1e-12, 1.0), u2 = unit(); return std::sqrt(-2.0 * std::log(u1)) * std::cos(TAU * u2) * std; }
};
static inline uint64_t agent_seed(uint64_t base, uint64_t tick, uint64_t id) {
    uint64_t z = base ^ (tick * 0x9E3779B97F4A7C15ULL) ^ (id * 0xD1B54A32D192ED03ULL);
    z = (z ^ (z >> 30)) * 0xBF58476D1CE4E5B9ULL;
    z = (z ^ (z >> 27)) * 0x94D049BB133111EBULL;
    return z ^ (z >> 31);
}

struct Params { double width = 2000, height = 2000, sheep_density = 0.96, fox_density = 8.4,
                fox_predation_effectiveness = 0.14, lamb_proportion = 0.2,
                chilla_density = 0.0, hare_density = 0.0; int n_dogs = 0; };

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
    std::vector<std::pair<double,double>> dog_positions;
    int current_hour = 0; uint64_t step_count = 0;
    long sheep_killed = 0, predation_attempts = 0; int n_sheep_initial = 0;

    void before_step() {
        current_hour = (int)(step_count % 24); step_count++;
        snap.resize(agents.size());
        dog_positions.clear();
        for (size_t i = 0; i < agents.size(); ++i) {
            const Animal& a = agents[i];
            snap[i] = {a.x, a.y, a.species, a.alive, a.is_lamb, a.vulnerability, a.energy, a.mother};
            if (a.alive && a.species == DOG) dog_positions.push_back({a.x, a.y});
        }
        grid.build(snap, params.width, params.height);
    }

    // nearest_dog_dist: menor distancia a un perro dentro de `max`, o -1 si ninguno.
    double nearest_dog_dist(double px, double py, double max) const {
        double best = -1.0;
        for (auto& d : dog_positions) {
            double dd = std::sqrt((px - d.first) * (px - d.first) + (py - d.second) * (py - d.second));
            if (dd <= max && (best < 0 || dd < best)) best = dd;
        }
        return best;
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
static void step_sheep(Model& m, int i, PRng& rng) {
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
        bool dog_near = m.nearest_dog_dist(s.x, s.y, DOG_PROXIMITY_VIGILANCE) >= 0;
        double p_move = dog_near ? (0.60 + 0.02) : (0.76 + 0.04);
        if (rng.unit() < p_move) {
            // graze_and_move
            double vfx, vfy; food_direction(m.veg_quality, s.x, s.y, vfx, vfy);
            double vrx = 0, vry = 0, worst = m.veg_cover.get(s.x, s.y);
            for (int k = 0; k < 8; ++k) { double a = TAU * k / 8.0, ox = std::cos(a), oy = std::sin(a);
                double cov = m.veg_cover.get(s.x + ox * 50.0, s.y + oy * 50.0); if (cov > worst) { worst = cov; vrx = -ox; vry = -oy; } }
            double cx = 0, cy = 0, n = 0, dog_best = 1e300, dgx = 0, dgy = 0;
            m.grid.within(m.snap, s.x, s.y, std::max(SHEEP_PERCEPTION_RADIUS, 500.0), [&](const Snap& sn, double d) {
                if (!sn.alive) return;
                if (sn.species == SHEEP && d <= SHEEP_PERCEPTION_RADIUS) { cx += sn.x; cy += sn.y; n += 1; }
                if (sn.species == DOG && d < dog_best) { dog_best = d; dgx = sn.x; dgy = sn.y; } });
            double vcx = 0, vcy = 0;
            if (n > 0) { vcx = cx / n - s.x; vcy = cy / n - s.y; norm(vcx, vcy); }
            double vdx = 0, vdy = 0;
            if (dog_best < 500.0) { vdx = dgx - s.x; vdy = dgy - s.y; norm(vdx, vdy); }
            double dx = vfx * 0.3 + vrx * 0.2 + vcx * 0.2 + vdx * 0.3 + rng.normal(0.1);
            double dy = vfy * 0.3 + vry * 0.2 + vcy * 0.2 + vdy * 0.3 + rng.normal(0.1);
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
    double dnd = m.nearest_dog_dist(fox.x, fox.y, 500.0);
    double m_dog = (dnd >= 0) ? -DOG_PROTECTION_STRENGTH * (1.0 - dnd / 500.0) : 0.0;
    double nearby = 0;
    m.grid.within(m.snap, prey.x, prey.y, 50.0, [&](const Snap& sn, double) { if (sn.alive && sn.species == SHEEP) nearby += 1; });
    double m_group = -0.03 * std::min(nearby / 10.0, 1.0);
    double m_condition = 0.03 * (1.0 - prey.energy / 100.0);
    double m_mother = 0.0;
    if (prey.is_lamb && prey.mother >= 0) { Animal& mo = m.agents[prey.mother];
        if (mo.alive) { double d = std::sqrt((prey.x - mo.x) * (prey.x - mo.x) + (prey.y - mo.y) * (prey.y - mo.y));
            if (d < 20.0) m_mother = -0.12 * (1.0 - d / 20.0); } }
    return clampd(p_base + m_cover + m_dog + m_group + m_condition + m_mother, FOX_P_MIN, FOX_P_MAX);
}

static void attempt_predation(Model& m, int fox_i, int prey_i, PRng& rng) {
    double p = predation_probability(m, fox_i, prey_i);
    m.predation_attempts++;
    bool success = rng.unit() < p;
    if (success) { m.agents[prey_i].alive = false; m.agents[fox_i].hunger = 0.0;
        if (m.agents[prey_i].species == SHEEP) m.sheep_killed++; }
    else { m.agents[prey_i].fear = 1.0; m.agents[fox_i].hunger = std::min(m.agents[fox_i].hunger + 0.05, 1.0); }
}

// ---- zorro ----
// Detección de presas del zorro (query espacial, READ-ONLY sobre el snapshot) —
// se puede precomputar en paralelo (Hito 4b) sin cambiar la semántica: el centro
// es la posición de inicio de tick (los zorros no se mueven en la fase A), y la
// selección/ataque (que leen estado vivo) siguen secuenciales.
static void fox_detect(const Model& m, double fx, double fy, std::vector<int>& prey, int& hares_nearby) {
    prey.clear(); hares_nearby = 0;
    int gx = (int)(fx / m.grid.csz); if (gx < 0) gx = 0; if (gx > m.grid.nx - 1) gx = m.grid.nx - 1;
    int gy = (int)(fy / m.grid.csz); if (gy < 0) gy = 0; if (gy > m.grid.ny - 1) gy = m.grid.ny - 1;
    int cr = (int)(FOX_DETECTION_RADIUS / m.grid.csz) + 1;
    for (int dy = -cr; dy <= cr; ++dy) for (int dx = -cr; dx <= cr; ++dx) {
        int ax = gx + dx, ay = gy + dy; if (ax < 0 || ay < 0 || ax >= m.grid.nx || ay >= m.grid.ny) continue;
        for (int j = m.grid.head[(size_t)ay * m.grid.nx + ax]; j != -1; j = m.grid.nxt[j]) {
            const Snap& sn = m.snap[j];
            double ddx = sn.x - fx, ddy = sn.y - fy; if (std::sqrt(ddx * ddx + ddy * ddy) > FOX_DETECTION_RADIUS) continue;
            if (!sn.alive) continue;
            if (sn.species == SHEEP) prey.push_back(j);
            else if (sn.species == HARE) { hares_nearby++; prey.push_back(j); }
        }
    }
}

static void step_fox(Model& m, int i, PRng& rng, const std::vector<int>& prey, int hares_nearby) {
    Animal& f = m.agents[i];
    f.hunger = std::min(f.hunger + 0.01, 1.0);
    // olvido de zonas peligrosas vencidas (memoria de 168 h)
    uint64_t now = m.step_count;
    { auto& dz = f.danger_zones; dz.erase(std::remove_if(dz.begin(), dz.end(),
        [&](const DangerZone& z) { return now - z.step >= DANGER_TTL; }), dz.end()); }
    // fox_active: curva gaussiana, desplazada/aplanada si percibe un perro en su territorio
    bool perceives_dog = m.nearest_dog_dist(f.x, f.y, f.territory_radius) >= 0;
    double peak = perceives_dog ? FOX_ACT_PEAK_WD : FOX_ACT_PEAK_ND;
    double amp = perceives_dog ? FOX_ACT_AMP_WD : FOX_ACT_AMP_ND;
    double sigma = perceives_dog ? FOX_ACT_SIGMA_WD : FOX_ACT_SIGMA_ND;
    double base = perceives_dog ? FOX_ACT_BASE_WD : FOX_ACT_BASE_ND;
    double h = (double)m.current_hour; double raw = std::fabs(h - peak); double dd0 = std::min(raw, 24.0 - raw);
    double level = std::min(base + amp * std::exp(-(dd0 * dd0) / (2.0 * sigma * sigma)), 1.0);
    if (rng.unit() >= level) return; // descanso
    if (f.hunger < HUNGER_THRESHOLD) { double a = rng.range(0.0, TAU), dd = rng.range(50.0, 200.0);
        f.x = clampd(f.x + std::cos(a) * dd, 0, m.params.width); f.y = clampd(f.y + std::sin(a) * dd, 0, m.params.height); return; }
    // --- hunt ---
    // (a) evitación de área: riesgo sumado por perros dentro de DOG_AVOID_RADIUS
    // + memoria de zonas peligrosas. Si supera la aversión, aborta y se aleja.
    {
        double risk = 0, wx = 0, wy = 0, wsum = 0;
        for (auto& dgp : m.dog_positions) {
            double ddx = f.x - dgp.first, ddy = f.y - dgp.second, dist = std::sqrt(ddx * ddx + ddy * ddy);
            if (dist < DOG_AVOID_RADIUS) { double r = 1.0 - dist / DOG_AVOID_RADIUS; risk += r; wx += dgp.first * r; wy += dgp.second * r; wsum += r; }
        }
        for (auto& z : f.danger_zones) {
            double ddx = f.x - z.x, ddy = f.y - z.y, dist = std::sqrt(ddx * ddx + ddy * ddy);
            if (dist < DANGER_RADIUS) { double age = (double)(now - z.step) / (double)DANGER_TTL;
                double r = (1.0 - age) * (1.0 - dist / DANGER_RADIUS); risk += r; wx += z.x * r; wy += z.y * r; wsum += r; }
        }
        double mult = f.is_chilla ? 1.8 : 1.0;
        if (risk * mult > f.risk_aversion) {
            f.danger_zones.push_back({f.x, f.y, now}); if (f.danger_zones.size() > 64) f.danger_zones.erase(f.danger_zones.begin());
            double fx = wsum > 0 ? wx / wsum : f.x, fy = wsum > 0 ? wy / wsum : f.y;
            double ax = f.x - fx, ay = f.y - fy; norm(ax, ay);
            f.x = clampd(f.x + ax * FOX_SPEED_WALK, 0, m.params.width); f.y = clampd(f.y + ay * FOX_SPEED_WALK, 0, m.params.height);
            f.stalk_target = -1; return;
        }
    }
    // (b) ataque comprometido: si venía acechando y la presa está a tiro, ataca
    { int tid = f.stalk_target; f.stalk_target = -1;
        if (tid >= 0 && m.agents[tid].alive) {
            double ddx = f.x - m.agents[tid].x, ddy = f.y - m.agents[tid].y;
            if (std::sqrt(ddx * ddx + ddy * ddy) < FOX_ATTACK_RADIUS) { attempt_predation(m, i, tid, rng); return; }
        }
    }
    // (c) detección de presas: precomputada en paralelo (fox_detect), pasada como
    // argumento. Es la parte cara del zorro; el resto (selección/ataque/mutación)
    // sigue secuencial leyendo estado vivo.
    if (prey.empty()) { double a = rng.range(0.0, TAU);
        f.x = clampd(f.x + std::cos(a) * FOX_SPEED_WALK, 0, m.params.width); f.y = clampd(f.y + std::sin(a) * FOX_SPEED_WALK, 0, m.params.height); return; }
    // seleccionar por score (vuln; -0.3 oveja si hay prey switching; +0.2 cordero)
    bool sw = hares_nearby >= 2;
    int best = prey[0]; double best_score = -1e300;
    for (int pid : prey) { Animal& pa = m.agents[pid]; if (!pa.alive) continue;
        double score = pa.vulnerability;
        if (pa.species == SHEEP && sw) score -= 0.3;
        if (pa.is_lamb) score += 0.2;
        if (score > best_score) { best_score = score; best = pid; } }
    if (!m.agents[best].alive) return;
    double tx = m.agents[best].x, ty = m.agents[best].y; // posición VIVA de la presa
    double to_x = tx - f.x, to_y = ty - f.y; double tolen = std::sqrt(to_x * to_x + to_y * to_y);
    double step = std::min(FOX_SPEED_WALK, tolen); double ux = to_x, uy = to_y; norm(ux, uy);
    f.x = clampd(f.x + ux * step, 0, m.params.width); f.y = clampd(f.y + uy * step, 0, m.params.height);
    double adx = f.x - tx, ady = f.y - ty;
    if (std::sqrt(adx * adx + ady * ady) < FOX_ATTACK_RADIUS) {
        // Si hay un perro en rango de detección, NO mata este tick: queda en
        // acecho expuesto (el perro tendrá un turno para interceptarlo). Sin
        // perro cerca, mata de inmediato.
        bool dog_near = m.nearest_dog_dist(f.x, f.y, DOG_DETECTION_RADIUS) >= 0;
        if (dog_near) f.stalk_target = best; else attempt_predation(m, i, best, rng);
    }
}

// ---- liebre (presa alternativa) ----
static void step_hare(Model& m, int i, PRng& rng) {
    Animal& h = m.agents[i];
    h.age_days += 1.0 / 24.0;
    if (!h.mature && h.age_days * 24.0 >= HARE_MATURITY_AGE_H) { h.mature = true; h.vulnerability = HARE_VULN_MATURE; }
    h.fear = std::max(h.fear - 0.15, 0.0);
    double pbest = 1e300, ppx = 0, ppy = 0; bool found = false;
    m.grid.within(m.snap, h.x, h.y, HARE_PERCEPTION_RADIUS, [&](const Snap& sn, double d) {
        if (sn.alive && sn.species == FOX && d < pbest) { pbest = d; ppx = sn.x; ppy = sn.y; found = true; } });
    if (found || h.fear > 0.5) {
        double dx, dy;
        if (found) { dx = h.x - ppx; dy = h.y - ppy; norm(dx, dy); }
        else { double a = rng.range(0.0, TAU); dx = std::cos(a); dy = std::sin(a); }
        h.x = clampd(h.x + dx * HARE_SPEED_FLEE, 0, m.params.width); h.y = clampd(h.y + dy * HARE_SPEED_FLEE, 0, m.params.height);
    } else {
        double dx, dy; food_direction(m.veg_quality, h.x, h.y, dx, dy);
        h.x = clampd(h.x + dx * HARE_SPEED_NORMAL, 0, m.params.width); h.y = clampd(h.y + dy * HARE_SPEED_NORMAL, 0, m.params.height);
    }
}

// ---- perro guardián (determinista) ----
static void step_dog(Model& m, int i) {
    Animal& dog = m.agents[i];
    // detectar zorro más cercano en DOG_DETECTION_RADIUS (snapshot)
    double best = 1e300, bx = 0, by = 0; int best_id = -1;
    {
        int gx = (int)(dog.x / m.grid.csz); if (gx < 0) gx = 0; if (gx > m.grid.nx - 1) gx = m.grid.nx - 1;
        int gy = (int)(dog.y / m.grid.csz); if (gy < 0) gy = 0; if (gy > m.grid.ny - 1) gy = m.grid.ny - 1;
        int cr = (int)(DOG_DETECTION_RADIUS / m.grid.csz) + 1;
        for (int ddy = -cr; ddy <= cr; ++ddy) for (int ddx = -cr; ddx <= cr; ++ddx) {
            int ax = gx + ddx, ay = gy + ddy; if (ax < 0 || ay < 0 || ax >= m.grid.nx || ay >= m.grid.ny) continue;
            for (int j = m.grid.head[(size_t)ay * m.grid.nx + ax]; j != -1; j = m.grid.nxt[j]) {
                const Snap& sn = m.snap[j]; if (!sn.alive || sn.species != FOX) continue;
                double dx = sn.x - dog.x, dy = sn.y - dog.y, dist = std::sqrt(dx * dx + dy * dy);
                if (dist <= DOG_DETECTION_RADIUS && dist < best) { best = dist; bx = sn.x; by = sn.y; best_id = j; }
            }
        }
    }
    if (best_id >= 0) {
        double to_x = bx - dog.x, to_y = by - dog.y, tolen = std::sqrt(to_x * to_x + to_y * to_y);
        double step = std::min(DOG_SPEED_CHASE, tolen), ux = to_x, uy = to_y; norm(ux, uy);
        dog.x = clampd(dog.x + ux * step, 0, m.params.width); dog.y = clampd(dog.y + uy * step, 0, m.params.height);
        double cdx = dog.x - bx, cdy = dog.y - by;
        if (best < DOG_CHASE_RADIUS && std::sqrt(cdx * cdx + cdy * cdy) < DOG_DETER_RADIUS) {
            // disuasión multi-objetivo: el objetivo + otros zorros dentro de
            // DOG_CHASE_RADIUS de la posición nueva del perro (snapshot)
            std::vector<std::pair<int, std::pair<double, double>>> deterred;
            deterred.push_back({best_id, {bx, by}});
            int gx = (int)(dog.x / m.grid.csz); if (gx < 0) gx = 0; if (gx > m.grid.nx - 1) gx = m.grid.nx - 1;
            int gy = (int)(dog.y / m.grid.csz); if (gy < 0) gy = 0; if (gy > m.grid.ny - 1) gy = m.grid.ny - 1;
            int cr = (int)(DOG_CHASE_RADIUS / m.grid.csz) + 1;
            for (int ddy = -cr; ddy <= cr; ++ddy) for (int ddx = -cr; ddx <= cr; ++ddx) {
                int ax = gx + ddx, ay = gy + ddy; if (ax < 0 || ay < 0 || ax >= m.grid.nx || ay >= m.grid.ny) continue;
                for (int j = m.grid.head[(size_t)ay * m.grid.nx + ax]; j != -1; j = m.grid.nxt[j]) {
                    const Snap& sn = m.snap[j]; if (!sn.alive || sn.species != FOX || j == best_id) continue;
                    double dx = sn.x - dog.x, dy = sn.y - dog.y; if (std::sqrt(dx * dx + dy * dy) <= DOG_CHASE_RADIUS) deterred.push_back({j, {sn.x, sn.y}});
                }
            }
            uint64_t now = m.step_count;
            for (auto& pr : deterred) { Animal& fox = m.agents[pr.first]; if (!fox.alive) continue;
                fox.fear = 1.0; fox.hunger = 0.0; fox.stalk_target = -1;
                fox.danger_zones.push_back({pr.second.first, pr.second.second, now});
                if (fox.danger_zones.size() > 64) fox.danger_zones.erase(fox.danger_zones.begin()); }
        }
        return;
    }
    // patrullaje circular alrededor del centroide del rebaño (posiciones vivas)
    double cx = 0, cy = 0, n = 0;
    for (auto& a : m.agents) if (a.alive && a.species == SHEEP) { cx += a.x; cy += a.y; n += 1; }
    double flx = n > 0 ? cx / n : dog.patrol_cx, fly = n > 0 ? cy / n : dog.patrol_cy;
    dog.patrol_angle += 0.1;
    double tx = flx + std::cos(dog.patrol_angle) * DOG_PATROL_RADIUS, ty = fly + std::sin(dog.patrol_angle) * DOG_PATROL_RADIUS;
    double dx = tx - dog.x, dy = ty - dog.y; norm(dx, dy);
    dog.x = clampd(dog.x + dx * DOG_SPEED_PATROL, 0, m.params.width); dog.y = clampd(dog.y + dy * DOG_SPEED_PATROL, 0, m.params.height);
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
        a.predation_eff = params.fox_predation_effectiveness; a.territory_radius = FOX_TERRITORY_RADIUS;
        m.agents.push_back(a);
    }
    // Chillas (mismo trait Fox, territorio menor, 1.8x más averso al perro).
    int n_chillas = (int)std::lround(params.chilla_density * area_km2);
    for (int k = 0; k < n_chillas; ++k) {
        Animal a{}; a.species = FOX; a.is_chilla = true; a.alive = true; a.energy = 100; a.mother = -1;
        a.x = rng_build.range(0, params.width); a.y = rng_build.range(0, params.height);
        a.hunger = rng_build.range(0.3, 0.7); a.risk_aversion = (BASE_RISK_AVERSION + rng_build.range(-0.1, 0.1)) * 0.7;
        a.predation_eff = params.fox_predation_effectiveness; a.territory_radius = CHILLA_TERRITORY_RADIUS;
        m.agents.push_back(a);
    }
    // Liebres (presa alternativa).
    int n_hares = (int)std::lround(params.hare_density * area_ha);
    for (int k = 0; k < n_hares; ++k) {
        Animal a{}; a.species = HARE; a.alive = true; a.mother = -1;
        a.x = rng_build.range(0, params.width); a.y = rng_build.range(0, params.height);
        a.energy = rng_build.range(60, 100); a.age_days = rng_build.range(0, 365);
        a.mature = a.age_days * 24.0 >= HARE_MATURITY_AGE_H;
        a.vulnerability = a.mature ? HARE_VULN_MATURE : HARE_VULN_JUV;
        m.agents.push_back(a);
    }
    // Perros guardianes alrededor del rebaño (sin chillas/liebres en screening,
    // así que el consumo de RNG de build coincide con el orden del oráculo).
    for (int k = 0; k < params.n_dogs; ++k) {
        double a0 = rng_build.range(0.0, TAU);
        Animal a{}; a.species = DOG; a.alive = true; a.energy = 100; a.mother = -1;
        a.x = clampd(fcx + std::cos(a0) * 300.0, 0, params.width); a.y = clampd(fcy + std::sin(a0) * 300.0, 0, params.height);
        a.patrol_cx = fcx; a.patrol_cy = fcy;
        m.agents.push_back(a);
    }

    // Fase A = ovejas/liebres (independientes: leen snapshot+raster, escriben a
    // si mismas -> paralelizables). Fase B = zorros/perros (mutan estado
    // compartido: matanzas, contadores, zonas de peligro -> secuencial).
    std::vector<int> phaseA, phaseB, foxidx;
    for (size_t k = 0; k < m.agents.size(); ++k) {
        int sp = m.agents[k].species;
        if (sp == SHEEP || sp == HARE) phaseA.push_back((int)k); else phaseB.push_back((int)k);
        if (sp == FOX) foxidx.push_back((int)k);
    }
    // Buffers reusados para la detección de zorros precomputada (Hito 4b).
    std::vector<std::vector<int>> fdprey(m.agents.size());
    std::vector<int> fdhares(m.agents.size(), 0);
    for (uint64_t step = 0; step < n_days * 24; ++step) {
        m.before_step();
        // Fase A (paralela): RNG por-agente (seed,step,id) -> independiente del orden de hilos.
        #pragma omp parallel for schedule(static)
        for (size_t a = 0; a < phaseA.size(); ++a) {
            int idx = phaseA[a]; if (!m.agents[idx].alive) continue;
            PRng r(agent_seed(seed, step, (uint64_t)idx));
            if (m.agents[idx].species == SHEEP) step_sheep(m, idx, r); else step_hare(m, idx, r);
        }
        // Fase B1 (paralela): detección de presas de cada zorro (read-only sobre el
        // snapshot; los zorros están en su posición de inicio de tick).
        #pragma omp parallel for schedule(dynamic, 16)
        for (size_t k = 0; k < foxidx.size(); ++k) {
            int idx = foxidx[k];
            if (!m.agents[idx].alive) { fdprey[idx].clear(); continue; }
            fox_detect(m, m.agents[idx].x, m.agents[idx].y, fdprey[idx], fdhares[idx]);
        }
        // Fase B2 (secuencial): orden barajado por tick (determinista); zorros y
        // perros mutan estado compartido (selección/ataque/disuasión).
        { PRng shuf(agent_seed(seed, step, 0xFFFFFFFFFFFFFFFFULL));
          for (size_t k = phaseB.size(); k > 1; --k) { size_t j = (size_t)(shuf.unit() * k); std::swap(phaseB[k - 1], phaseB[j]); } }
        for (int idx : phaseB) { if (!m.agents[idx].alive) continue;
            PRng r(agent_seed(seed, step, (uint64_t)idx));
            if (m.agents[idx].species == FOX) step_fox(m, idx, r, fdprey[idx], fdhares[idx]); else step_dog(m, idx); }
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
    p.chilla_density = darg("--chilla-density", p.chilla_density);
    p.hare_density = darg("--hare-density", p.hare_density);
    p.n_dogs = (int)darg("--dogs", p.n_dogs);
    p.width = darg("--width", p.width);   // escalar el AREA a densidad constante
    p.height = darg("--height", p.height); // -> muchos agentes, pocos vecinos/query

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
