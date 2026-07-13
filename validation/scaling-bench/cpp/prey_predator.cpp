// Kernel de escalamiento depredador-presa (espacial) — implementación C++
// idiomática, con índice espacial de celdas (cell list). Espejo exacto, en
// reglas, del kernel swarm-abm (../rust). Mide throughput del stepping puro;
// la construcción queda fuera del cronómetro. Sembrado (reproducible).
//
// Reglas (idénticas a la versión Rust):
//   - N agentes en un mundo [0,L)^2; los primeros PRED_FRAC*N son depredadores.
//   - L = sqrt(N / LAMBDA) con LAMBDA constante -> densidad fija al escalar.
//   - Por paso, se reconstruye la grilla (celda = R) y:
//       * presa: camina en dirección aleatoria un paso `MOVE`.
//       * depredador: busca la presa más cercana dentro de radio R (escaneo de
//         las 9 celdas vecinas); si la encuentra, avanza `MOVE` hacia ella; si
//         no, camina aleatorio.
//   - Sin nacimientos ni muertes: población constante -> ms/paso limpio.
//
// Uso: prey_predator <N> <steps> [seed]
#include <cstdio>
#include <cstdlib>
#include <cmath>
#include <cstdint>
#include <vector>
#include <random>
#include <chrono>

static const double LAMBDA = 0.06366; // agentes por unidad^2 (~5 vecinos en R)
static const double R      = 5.0;      // radio de sensado
static const double MOVE   = 1.0;      // paso de movimiento
static const double PRED_FRAC = 0.2;   // fracción de depredadores
static const double TWO_PI = 6.283185307179586;

int main(int argc, char** argv) {
    if (argc < 3) { std::fprintf(stderr, "uso: %s <N> <steps> [seed]\n", argv[0]); return 1; }
    const size_t N     = std::strtoull(argv[1], nullptr, 10);
    const int    STEPS = std::atoi(argv[2]);
    const uint64_t SEED = (argc > 3) ? std::strtoull(argv[3], nullptr, 10) : 1;

    const double L = std::sqrt((double)N / LAMBDA);
    const int    ncell = std::max(1, (int)(L / R));       // celdas por eje
    const double csz = L / ncell;                          // tamaño de celda

    std::vector<double> x(N), y(N), x2(N), y2(N);
    std::vector<uint8_t> pred(N, 0);
    const size_t n_pred = (size_t)(PRED_FRAC * N);

    std::mt19937_64 gen(SEED);
    std::uniform_real_distribution<double> U(0.0, 1.0);
    for (size_t i = 0; i < N; ++i) {
        x[i] = U(gen) * L; y[i] = U(gen) * L;
        pred[i] = (i < n_pred) ? 1 : 0;
    }

    // Cell list: head/next (encadenado), reconstruido por paso sin realocar.
    std::vector<int32_t> head((size_t)ncell * ncell, -1);
    std::vector<int32_t> nxt(N, -1);
    auto cell_of = [&](double px, double py) {
        int cx = (int)(px / csz); if (cx >= ncell) cx = ncell - 1; if (cx < 0) cx = 0;
        int cy = (int)(py / csz); if (cy >= ncell) cy = ncell - 1; if (cy < 0) cy = 0;
        return cy * ncell + cx;
    };

    auto t0 = std::chrono::steady_clock::now();
    const double R2 = R * R;
    uint64_t contactos = 0; // trabajo observable (evita que el optimizador elimine el loop)

    for (int step = 0; step < STEPS; ++step) {
        // reconstruir cell list
        std::fill(head.begin(), head.end(), -1);
        for (size_t i = 0; i < N; ++i) {
            int c = cell_of(x[i], y[i]);
            nxt[i] = head[c]; head[c] = (int32_t)i;
        }
        // Actualización sincrónica: se lee de (x,y) del inicio del paso y se
        // escribe en (x2,y2); las consultas ven posiciones de inicio de paso,
        // igual que `for_each_within` sobre el índice reindexado en Rust.
        for (size_t i = 0; i < N; ++i) {
            double nxi, nyi;
            bool moved_to_prey = false;
            if (pred[i]) {
                int cx = (int)(x[i] / csz); if (cx >= ncell) cx = ncell - 1; if (cx < 0) cx = 0;
                int cy = (int)(y[i] / csz); if (cy >= ncell) cy = ncell - 1; if (cy < 0) cy = 0;
                double best = R2; int bj = -1;
                for (int dy = -1; dy <= 1; ++dy) for (int dx = -1; dx <= 1; ++dx) {
                    int nx = cx + dx, ny = cy + dy;
                    if (nx < 0 || ny < 0 || nx >= ncell || ny >= ncell) continue;
                    for (int32_t j = head[ny * ncell + nx]; j != -1; j = nxt[j]) {
                        if (pred[j]) continue;
                        double ddx = x[j] - x[i], ddy = y[j] - y[i];
                        double d2 = ddx*ddx + ddy*ddy;
                        if (d2 < best) { best = d2; bj = j; }
                    }
                }
                if (bj != -1) {
                    contactos++;
                    double ddx = x[bj] - x[i], ddy = y[bj] - y[i];
                    double d = std::sqrt(ddx*ddx + ddy*ddy);
                    nxi = x[i] + (d > 1e-9 ? MOVE * ddx / d : 0.0);
                    nyi = y[i] + (d > 1e-9 ? MOVE * ddy / d : 0.0);
                    moved_to_prey = true;
                }
            }
            if (!moved_to_prey) {
                double ang = U(gen) * TWO_PI;
                nxi = x[i] + MOVE * std::cos(ang);
                nyi = y[i] + MOVE * std::sin(ang);
            }
            if (nxi < 0) nxi = 0; else if (nxi >= L) nxi = std::nextafter(L, 0.0);
            if (nyi < 0) nyi = 0; else if (nyi >= L) nyi = std::nextafter(L, 0.0);
            x2[i] = nxi; y2[i] = nyi;
        }
        x.swap(x2); y.swap(y2);
    }

    auto t1 = std::chrono::steady_clock::now();
    double ms = std::chrono::duration<double, std::milli>(t1 - t0).count();
    // N | steps | ms_total | ms_por_paso | contactos (sink)
    std::printf("%zu %d %.3f %.5f %llu\n", N, STEPS, ms, ms / STEPS,
                (unsigned long long)contactos);
    return 0;
}
