// Perfil de cómputo del modelo SIGRID — ¿dónde se va el tiempo por fase/especie,
// y qué fracción es paralelizable vs secuencial (techo de Amdahl)?
//
// Reusa el núcleo (sigrid_core.hpp) y replica el bucle de tick de run_loss_rate,
// pero cronometrando cada fase con std::chrono. Las FRACCIONES (%) son robustas a
// la carga de la máquina (todas las fases sufren la misma contención).
//
// Clasificación para el techo de Amdahl:
//   PARALELIZABLE = before_step (grid build, ya paralelo en Hito 5b) + fase A
//                   (ovejas/liebres, Hito 4) + fase B1 (fox_detect, Hito 4b)
//   SECUENCIAL    = fase B2 (zorros/perros, orden global + mutación) + término
//
// Compilar: g++ -std=c++17 -O3 -march=native -ffp-contract=off profile.cpp -o profile
// (sin -fopenmp -> perfil serial, el reparto de cómputo puro)
// Uso: ./profile [--days N] [--seed N] [--width W] [--height H] [--sheep-density D]
//                [--fox-density D] [--dogs N] [--hare-density D] [--chilla-density D]
#include "sigrid_core.hpp"
#include <chrono>
using clk = std::chrono::steady_clock;
static inline double secs(clk::time_point a, clk::time_point b){ return std::chrono::duration<double>(b - a).count(); }

int main(int argc, char** argv) {
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
    p.farms_x = (int)darg("--farms-x", p.farms_x); p.farms_y = (int)darg("--farms-y", p.farms_y);
    p.width = darg("--width", p.width); p.height = darg("--height", p.height);

    Model m; std::vector<int> phaseA, phaseB, foxidx;
    build_model(p, seed, m, phaseA, phaseB, foxidx);
    // composición de agentes
    long n_sheep=0,n_hare=0,n_fox=0,n_dog=0;
    for (auto& a : m.agents){ if(a.species==SHEEP)n_sheep++; else if(a.species==HARE)n_hare++; else if(a.species==FOX)n_fox++; else if(a.species==DOG)n_dog++; }

    std::vector<std::vector<int>> fdprey(m.agents.size());
    std::vector<int> fdhares(m.agents.size(), 0);
    double t_before=0, t_A=0, t_B1=0, t_B2fox=0, t_B2dog=0, t_C=0, t_term=0;
    long ticks=0;

    for (uint64_t step = 0; step < days * 24; ++step) {
        auto t0 = clk::now();
        m.before_step();
        auto t1 = clk::now(); t_before += secs(t0, t1);

        // Fase A (ovejas + liebres)
        for (size_t a = 0; a < phaseA.size(); ++a) {
            int idx = phaseA[a]; if (!m.agents[idx].alive) continue;
            PRng r(agent_seed(seed, step, (uint64_t)idx));
            if (m.agents[idx].species == SHEEP) step_sheep(m, idx, r); else step_hare(m, idx, r);
        }
        auto t2 = clk::now(); t_A += secs(t1, t2);

        // Fase B1 (detección de presas del zorro)
        for (size_t k = 0; k < foxidx.size(); ++k) {
            int idx = foxidx[k];
            if (!m.agents[idx].alive) { fdprey[idx].clear(); continue; }
            fox_detect(m, m.agents[idx].x, m.agents[idx].y, fdprey[idx], fdhares[idx]);
        }
        auto t3 = clk::now(); t_B1 += secs(t2, t3);

        // Fase B2 (secuencial): shuffle + step_fox/step_dog, cronometrando fox vs dog
        { PRng shuf(agent_seed(seed, step, 0xFFFFFFFFFFFFFFFFULL));
          for (size_t k = phaseB.size(); k > 1; --k) { size_t j = (size_t)(shuf.unit() * k); std::swap(phaseB[k - 1], phaseB[j]); } }
        for (int idx : phaseB) {
            if (!m.agents[idx].alive) continue;
            if (m.agents[idx].species == FOX) {
                auto a = clk::now();
                PRng r(agent_seed(seed, step, (uint64_t)idx));
                step_fox(m, idx, r, fdprey[idx], fdhares[idx]);
                t_B2fox += secs(a, clk::now());
            } else {
                auto a = clk::now();
                step_dog(m, idx);
                t_B2dog += secs(a, clk::now());
            }
        }
        auto t4 = clk::now();

        // Fase C (secuencial): nacimientos de liebres + mortalidad denso-dependiente
        if (hare_births(m, phaseA) > 0) { fdprey.resize(m.agents.size()); fdhares.resize(m.agents.size(), 0); }
        if (step % 24 == 0) hare_cull(m);
        auto t4c = clk::now(); t_C += secs(t4, t4c);

        bool any_sheep = false; for (auto& a : m.agents) if (a.alive && a.species == SHEEP) { any_sheep = true; break; }
        t_term += secs(t4c, clk::now());
        ticks++;
        if (!any_sheep) break;
    }

    double t_B2 = t_B2fox + t_B2dog;
    double total = t_before + t_A + t_B1 + t_B2 + t_C + t_term;
    double par = t_before + t_A + t_B1;        // paralelizable
    double seq = t_B2 + t_C + t_term;           // secuencial (Amdahl)
    double f_seq = seq / total;
    double amdahl_inf = 1.0 / f_seq;            // techo de speedup con ∞ hilos

    printf("=== Perfil de cómputo · SIGRID (serial) ===\n");
    printf("config: %.0fx%.0f m · dens ov=%.2f zorro=%.2f · perros=%d · liebres=%.1f · chillas=%.1f · %llu días\n",
           p.width, p.height, p.sheep_density, p.fox_density, p.n_dogs, p.hare_density, p.chilla_density, (unsigned long long)days);
    printf("agentes: %ld ovejas · %ld liebres · %ld zorros · %ld perros | %ld ticks | %.3f s\n\n",
           n_sheep, n_hare, n_fox, n_dog, ticks, total);
    auto row = [&](const char* name, double t){ printf("  %-26s %8.3f s   %5.1f%%\n", name, t, 100.0*t/total); };
    row("before_step (grid+snap)", t_before);
    row("fase A (ovejas+liebres)", t_A);
    row("fase B1 (fox_detect)",   t_B1);
    row("fase B2 zorros",          t_B2fox);
    row("fase B2 perros",          t_B2dog);
    row("fase C (liebres nac/cull)", t_C);
    row("término (conteo ovejas)", t_term);
    printf("\n  PARALELIZABLE (before+A+B1)  %8.3f s   %5.1f%%\n", par, 100.0*par/total);
    printf("  SECUENCIAL   (B2+C+término)  %8.3f s   %5.1f%%\n", seq, 100.0*seq/total);
    printf("\n  Techo de Amdahl (∞ hilos): %.1fx   |  a 16 hilos: %.1fx\n",
           amdahl_inf, 1.0/(f_seq + (1.0-f_seq)/16.0));
    // Control de equivalencia: debe coincidir con sheep_fox.cpp a igual config/semilla.
    printf("\n  [control] loss_rate %.2f%% | matadas %ld | intentos %ld\n",
           (double)m.sheep_killed / std::max(1, m.n_sheep_initial) * 100.0,
           m.sheep_killed, m.predation_attempts);
    return 0;
}
