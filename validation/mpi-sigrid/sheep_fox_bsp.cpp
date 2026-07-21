// SIGRID en BSPonMPI — Hito 6: multi-nodo por descomposición de dominio (BSPlib).
//
// Reusa EXACTAMENTE el modelo de sigrid_core.hpp (fuente única compartida con la
// versión OpenMP), así que el comportamiento es idéntico por construcción; lo que
// esta capa agrega es la distribución BSP.
//
// Estrategia (decisión: BIT-IDÉNTICO a la referencia serial/OMP a cualquier P):
//  - La fase A (ovejas + liebres) se DESCOMPONE por franjas en x entre los P
//    procesadores, con halo de 500 m (cubre el mayor radio de consulta de una
//    oveja: atracción al perro). Cada procesador arma su snapshot local ORDENADO
//    POR gid (índice global) — así el orden intra-celda es gid-descendente, igual
//    que el serial => las consultas dan el mismo resultado bit a bit.
//  - La fase B (zorros + perros) es secuencial-global (orden barajado, mutaciones
//    cruzadas, señal de perro GLOBAL a 6135 m). Se CENTRALIZA en el proc 0, que
//    corre el código de fase B serial EXACTO sobre el estado global. Los
//    depredadores son minoría; el costo es reunir el estado de ovejas en el proc 0
//    cada tick (O(N) de comunicación) — el precio honesto de la garantía
//    bit-idéntica, y justo lo que motiva la variante event-driven (encargo de
//    Marín a Manuel).
//
// Superpasos BSP (ε = 1 tick = YAWNS/BSP conservador, §9.3 del plan):
//   S1: proc 0 dispersa a cada rank su franja+halo (frozen) + posiciones de perros;
//       cada rank corre fase A de sus ovejas/liebres y devuelve el estado vivo.
//   S2: proc 0 aplica el estado vivo, corre la fase B serial-exacta sobre gm, y
//       dispersa el frozen del SIGUIENTE tick (o una señal de término).
//
// Compilar: bspcxx -O3 -march=native sheep_fox_bsp.cpp -o sheep_fox_bsp
// Correr:   bsprun -n <P> ./sheep_fox_bsp [--days N] [--seed N] [--width W] ...
#include "../cpp-sigrid/sigrid_core.hpp"
#include <bsp.h>

static const double HALO = 500.0; // ≥ mayor radio de consulta de una oveja (atracción al perro)

// Registro de agente que viaja proc0<->rank para la fase A (subconjunto de Animal
// suficiente para el snapshot y para step_sheep/step_hare).
struct AgentRec {
    int gid, mother;
    double x, y, energy, age_days, fear, vulnerability;
    uint8_t species, alive, is_lamb, mature;
};
enum MsgTag { TAG_WORK = 1, TAG_DOG = 2, TAG_RESULT = 3, TAG_STOP = 4 };

static Params g_params;
static uint64_t g_seed, g_days;
static int P, PID;

static inline int owner_of(double x) {
    int r = (int)(x / (g_params.width / P));
    if (r < 0) r = 0; if (r > P - 1) r = P - 1; return r;
}

static AgentRec to_rec(const Animal& a, int gid) {
    AgentRec r; r.gid = gid; r.mother = a.mother; r.x = a.x; r.y = a.y;
    r.energy = a.energy; r.age_days = a.age_days; r.fear = a.fear; r.vulnerability = a.vulnerability;
    r.species = (uint8_t)a.species; r.alive = (uint8_t)a.alive; r.is_lamb = (uint8_t)a.is_lamb; r.mature = (uint8_t)a.mature;
    return r;
}
static Animal from_rec(const AgentRec& r) {
    Animal a{}; a.species = r.species; a.mother = r.mother; a.x = r.x; a.y = r.y;
    a.energy = r.energy; a.age_days = r.age_days; a.fear = r.fear; a.vulnerability = r.vulnerability;
    a.alive = (bool)r.alive; a.is_lamb = (bool)r.is_lamb; a.mature = (bool)r.mature;
    return a;
}

// proc 0 dispersa el estado FROZEN del tick: a cada rank r, todos los agentes con
// x en [lo_r - HALO, hi_r + HALO) (su franja + halo), más las posiciones de perros.
static void scatter_frozen(const Model& gm) {
    double strip = g_params.width / P;
    for (size_t gid = 0; gid < gm.agents.size(); ++gid) {
        const Animal& a = gm.agents[gid];
        AgentRec rec = to_rec(a, (int)gid);
        int r_lo = owner_of(a.x - HALO), r_hi = owner_of(a.x + HALO);
        for (int r = r_lo; r <= r_hi; ++r) { int t = TAG_WORK; bsp_send(r, &t, &rec, sizeof(rec)); }
    }
    // broadcast de perros (señal global; pocos emisores — el patrón uno-a-muchos)
    for (auto& d : gm.dog_positions) {
        double xy[2] = {d.first, d.second};
        for (int r = 0; r < P; ++r) { int t = TAG_DOG; bsp_send(r, &t, xy, sizeof(xy)); }
    }
}

static void send_stop() { int t = TAG_STOP; for (int r = 0; r < P; ++r) bsp_send(r, &t, &t, sizeof(int)); }

int main(int argc, char** argv) {
    bsp_begin(bsp_nprocs());
    P = bsp_nprocs(); PID = bsp_pid();
    int tagsz = sizeof(int); bsp_set_tagsize(&tagsz); bsp_sync();

    // Todas las ranks parsean argv idéntico (mismos params/seed) y construyen los
    // rasters localmente (deterministas) — sin comunicación de setup.
    Params p; uint64_t days = 30, seed = 1000;
    auto darg = [&](const char* n, double dv) -> double { for (int i = 1; i < argc - 1; ++i) if (!std::strcmp(argv[i], n)) return std::atof(argv[i + 1]); return dv; };
    days = (uint64_t)darg("--days", 30); seed = (uint64_t)darg("--seed", 1000);
    p.sheep_density = darg("--sheep-density", p.sheep_density);
    p.fox_density = darg("--fox-density", p.fox_density);
    p.fox_predation_effectiveness = darg("--fox-eff", p.fox_predation_effectiveness);
    p.lamb_proportion = darg("--lamb-prop", p.lamb_proportion);
    p.chilla_density = darg("--chilla-density", p.chilla_density);
    p.hare_density = darg("--hare-density", p.hare_density);
    p.n_dogs = (int)darg("--dogs", p.n_dogs);
    p.width = darg("--width", p.width); p.height = darg("--height", p.height);
    g_params = p; g_seed = seed; g_days = days;

    // Modelo local de cada rank (para la fase A): solo rasters + params.
    Model lm; lm.params = p;
    { Rng rng_r(seed); build_rasters(p.width, p.height, rng_r, lm.veg_quality, lm.veg_cover); }

    // proc 0: modelo global autoritativo (build EXACTO del serial vía build_model).
    Model gm; std::vector<int> phaseA0, phaseB0, foxidx0;
    if (PID == 0) build_model(p, seed, gm, phaseA0, phaseB0, foxidx0);

    // Buffers de fase B en proc 0 (como run_loss_rate).
    std::vector<std::vector<int>> fdprey; std::vector<int> fdhares;
    if (PID == 0) { fdprey.assign(gm.agents.size(), {}); fdhares.assign(gm.agents.size(), 0); }

    // Scatter inicial (frozen del tick 0).
    if (PID == 0) { gm.before_step(); scatter_frozen(gm); }
    bsp_sync();

    bool stop = false;
    for (uint64_t step = 0; step < g_days * 24 && !stop; ++step) {
        // ---- S1: cada rank recibe su franja+halo, corre fase A, devuelve vivo ----
        std::vector<AgentRec> recs; std::vector<std::pair<double,double>> dogs;
        int nmsg; bsp_size_t nbytes; bsp_qsize(&nmsg, &nbytes);
        for (int mi = 0; mi < nmsg; ++mi) {
            int tag; bsp_size_t sz; bsp_get_tag(&sz, &tag);
            if (tag == TAG_WORK) { AgentRec r; bsp_move(&r, sizeof(r)); recs.push_back(r); }
            else if (tag == TAG_DOG) { double xy[2]; bsp_move(xy, sizeof(xy)); dogs.push_back({xy[0], xy[1]}); }
            else if (tag == TAG_STOP) { char c; bsp_move(&c, sz); stop = true; }
        }
        if (stop) break;
        // arma el modelo local: agentes ordenados por gid ascendente (=> orden
        // intra-celda gid-descendente en el grid, igual que el serial).
        std::sort(recs.begin(), recs.end(), [](const AgentRec& a, const AgentRec& b){ return a.gid < b.gid; });
        lm.agents.clear(); lm.agents.reserve(recs.size());
        for (auto& r : recs) lm.agents.push_back(from_rec(r));
        lm.dog_positions = dogs;
        lm.step_count = step + 1; lm.current_hour = (int)(step % 24); // fox_active no aplica en fase A; step_sheep no usa hour
        lm.grid.build(lm.agents, p.width, p.height);
        // corre la fase A SOLO de los agentes que este rank posee (owner(x)==PID).
        for (size_t li = 0; li < lm.agents.size(); ++li) {
            Animal& a = lm.agents[li];
            if (!a.alive) continue;
            if (!(a.species == SHEEP || a.species == HARE)) continue;
            if (owner_of(a.x) != PID) continue; // solo los propios (los demás son ghost)
            PRng r(agent_seed(seed, step, (uint64_t)recs[li].gid));
            if (a.species == SHEEP) step_sheep(lm, (int)li, r); else step_hare(lm, (int)li, r);
            AgentRec out = to_rec(a, recs[li].gid);
            int t = TAG_RESULT; bsp_send(0, &t, &out, sizeof(out));
        }
        bsp_sync();

        // ---- S2: proc 0 aplica vivo, corre fase B serial-exacta, dispersa sig. ----
        if (PID == 0) {
            int nm; bsp_size_t nb; bsp_qsize(&nm, &nb);
            for (int mi = 0; mi < nm; ++mi) {
                int tag; bsp_size_t sz; bsp_get_tag(&sz, &tag);
                AgentRec r; bsp_move(&r, sizeof(r));
                if (tag == TAG_RESULT) { // aplica el estado vivo (post-fase-A) al gm
                    Animal& a = gm.agents[r.gid];
                    a.x = r.x; a.y = r.y; a.energy = r.energy; a.age_days = r.age_days;
                    a.fear = r.fear; a.vulnerability = r.vulnerability;
                    a.is_lamb = (bool)r.is_lamb; a.mature = (bool)r.mature;
                }
            }
            // Fase B EXACTA (idéntica a run_loss_rate): grid frozen ya construido por
            // el before_step del scatter previo; gm.agents ahora tiene ovejas vivas.
            for (size_t k = 0; k < foxidx0.size(); ++k) {
                int idx = foxidx0[k];
                if (!gm.agents[idx].alive) { fdprey[idx].clear(); continue; }
                fox_detect(gm, gm.agents[idx].x, gm.agents[idx].y, fdprey[idx], fdhares[idx]);
            }
            { PRng shuf(agent_seed(seed, step, 0xFFFFFFFFFFFFFFFFULL));
              for (size_t k = phaseB0.size(); k > 1; --k) { size_t j = (size_t)(shuf.unit() * k); std::swap(phaseB0[k - 1], phaseB0[j]); } }
            for (int idx : phaseB0) { if (!gm.agents[idx].alive) continue;
                PRng r(agent_seed(seed, step, (uint64_t)idx));
                if (gm.agents[idx].species == FOX) step_fox(gm, idx, r, fdprey[idx], fdhares[idx]); else step_dog(gm, idx); }
            // término anticipado si no quedan ovejas, o último tick
            bool any_sheep = false; for (auto& a : gm.agents) if (a.alive && a.species == SHEEP) { any_sheep = true; break; }
            bool last = (step + 1 >= g_days * 24);
            if (!any_sheep || last) { send_stop(); }
            else { gm.before_step(); scatter_frozen(gm); } // frozen del siguiente tick
        }
        bsp_sync();
    }

    if (PID == 0) {
        double lr = (double)gm.sheep_killed / std::max(1, gm.n_sheep_initial) * 100.0;
        printf("  semilla %llu: loss_rate %.2f%% | matadas %ld | intentos %ld  [BSP P=%d]\n",
               (unsigned long long)seed, lr, gm.sheep_killed, gm.predation_attempts, P);
    }
    bsp_end();
    return 0;
}
