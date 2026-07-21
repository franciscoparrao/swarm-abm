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
#include "sigrid_core.hpp"

static double run_loss_rate(const Params& params, uint64_t seed, uint64_t n_days, long& killed, long& attempts) {
    // Construcción de la población (fuente única en sigrid_core.hpp): agentes,
    // rasters, y las listas de fase A (ovejas/liebres) / fase B (zorros/perros).
    Model m; std::vector<int> phaseA, phaseB, foxidx;
    build_model(params, seed, m, phaseA, phaseB, foxidx);
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
