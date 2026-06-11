# Benchmarks: swarm-core (Rust) vs Mesa (Python)

Modelo: SIR espacial idéntico en ambos motores (paridad numérica
verificada en `REPORT.md`). Se mide solo la fase de simulación, en
proceso, sin reporters; mediana de ms/paso sobre réplicas con
semillas distintas. β=0.08, γ=0.1, 5 infectados iniciales, máx. 300
pasos, vecindad Moore, torus.

| Grilla | Agentes | Rust (ms/paso) | Mesa (ms/paso) | Speedup | Rust (agente-pasos/s) |
|---|---|---|---|---|---|
| 25×25 | 625 | 0.023 | 1.35 | **58×** | 26,834,239 |
| 50×50 | 2,500 | 0.142 | 8.23 | **58×** | 17,568,903 |
| 100×100 | 10,000 | 0.312 | 20.79 | **67×** | 32,098,987 |
| 200×200 | 40,000 | 1.401 | 63.19 | **45×** | 28,546,552 |

Réplicas por celda: 3. NetLogo queda pendiente (requiere instalación JVM); los microbenchmarks del motor (escalamiento de caminantes a 1M de agentes, Life simultáneo, `diffuse`) corren con criterion: `cargo bench -p swarm-core`.

## Entorno

- CPU: 12th Gen Intel(R) Core(TM) i7-1270P
- OS: Linux 6.17.0-23-generic
- Rust: rustc 1.94.1 (e408947bf 2026-03-25) (perfil release: lto, codegen-units=1)
- Python: 3.12.3, Mesa: 3.5.1

Generado por `validation/bench_report.py` desde `validation/data/bench.csv`.
