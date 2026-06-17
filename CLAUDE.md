# swarm-abm — Motor de modelado basado en agentes espacial (Rust, "Mesa/NetLogo moderno")

> **Estado:** MVP v0.1 completo (núcleo + 3 ejemplos validados). Creado 2026-06-10.
> Repo: https://github.com/franciscoparrao/swarm-abm
> Familia de motores Rust del autor: SurtGIS, Hydroflux, Smelt, Anvil, Cantus, Criterium.
> Doc madre: `~/proyectos/ideas-motores-rust.md` (idea G2). Semilla: `debris-flow-abm`.

## Qué es
Framework genérico de ABM espacial (agentes sobre grilla/red): scheduling,
vecindades, recolección de datos y reproducibilidad determinista.

## El gap que llena
Tienes `debris-flow-abm` como caso puntual; falta el **motor genérico**. El
campo es **NetLogo** (JVM, lento), **Mesa** (Python, lento). Rust = millones de
agentes en tiempo real + WASM para correr en navegador.

## Alcance MVP (v0.1)
- [x] Núcleo: agentes, scheduler (ordered/random), grilla 2D + vecindades (+ `diffuse` estilo NetLogo).
- [x] Recolectores de datos (series por step) y RNG sembrable (ChaCha8, determinismo bit a bit).
- [x] API para definir modelos (trait `Agent`, `Model`; patrón take-out para el doble préstamo).
- [x] Ejemplos: difusión, contagio (SIR espacial), Schelling — los 3 validados (difusión converge al punto fijo analítico).
- [x] (v0.2) Activación simultánea en dos fases (`decide` con `&Model` inmutable + `apply`; validada con Game of Life).
- [ ] (v0.3) Grafos/redes; batch runs + barrido de parámetros; viz WASM.

## Arquitectura tentativa
- `swarm-core`: motor; modelos como crates de ejemplo.
- Targets: native (Rayon para ensembles) + Python (PyO3) + WASM (visor).

## Validación / paridad numérica
**HECHA (2026-06-10)**: paridad distribucional vs Mesa confirmada — 50 réplicas
por motor, 7/7 métricas con |z| ≤ 1.22, curvas de ensamble Δ < 0.021.
Speedup ~67× sobre Mesa. Ver `validation/REPORT.md` y `validation/run_validation.sh`.

## Venue objetivo
**Environmental Modelling & Software** o **JASSS** (social simulation).

## Conexiones con tu ecosistema
- **debris-flow-abm**: ✅ REESCRITO sobre el motor (2026-06-11, `models/debris-flow/`):
  port fiel del V4 HYBRID v2, paridad distribucional verificada corriendo el
  Python original sobre insumos idénticos (IoU/área/flujos/trayectorias
  solapados), speedup ~100× (130-240s → 1.2-4s por corrida Copiapó 31.8M
  celdas), y ahora reproducible (el original usaba np.random global sin
  semilla). Ver `models/debris-flow/PARITY.md`.
- **firespread**: variante ABM de propagación.
- **SurtGIS**: rásters como entorno espacial de los agentes.

## Benchmarks
**HECHOS (2026-06-10/11)**: cross-engine SIR vs Mesa **45–67×** (25²→200²,
mediana, medición en proceso); criterion post-optimización: 38M
agente-pasos/s (10k walkers), 12M/s (1M walkers), Life simultáneo 37M
celdas/s, SIR 50×50 completo en 7 ms. Hot path sin allocaciones: buffer de
orden reutilizado en Simulation + `Grid2D::random_neighbor` (mejora +30% a
+74% según escala). Ver `validation/BENCHMARKS.md` y
`crates/swarm-core/benches/engine.rs`. NetLogo pendiente (requiere JVM).

## Calibración (debris-flow)
**HECHA (2026-06-14)**: Differential Evolution sobre el port Rust
(`models/debris-flow/src/bin/calibrate.rs`, rayon + Arc<Layers> compartido).
672 evals en ~70s (single-seed) / 2016 sims en ~5min (robusto 3 semillas);
equivalente Python secuencial ~11-34h → ~400x. Detectó y corrigió
sobreajuste a semilla única (T colapsa 1.81→0.02 con objetivo multi-semilla);
IoU medio 0.074→0.158 (8 semillas). Ver models/debris-flow/CALIBRATION.md.

## Benchmark de metaheurísticas (debris-flow)
**HECHO (2026-06-15)**: 5 optimizadores (DE/GA/PSO/SA/GWO en
`models/debris-flow/src/optim.rs`) comparados con potencia estadística —
el estudio que el paper original no pudo (1 corrida/método, recortado por
memoria). 5×10 corridas = 7500 sims en ~12min (Python ~80h). GWO gana:
Friedman χ²=14.3 p=0.006, Wilcoxon-Holm GWO>DE,GA; lidera fuera de muestra.
Tests en validation/calibration_benchmark.py (scipy). Ver BENCHMARK_OPTIM.md.

## Experimento de física enriquecida (2026-06-16)
Chañaral con entrainment + Voellmy + inercia (EnhancedPhysics, calibrado por
DE en bin/calibrate_chanaral). Veredicto HONESTO: no mejora robustamente vs
base. Base 0.4684±0.0004 (precision 0.690) vs enriquecido 0.4597±0.0275
(precision 0.715, pico 0.4757). Empate con trade-offs. Lección: 3-seed
sobreajustó (oos 0.31); objetivo media−sd con 5 semillas lo arregló.
Resultado publicable: "more physics ≠ better" + el motor lo estableció con
rigor en una tarde. Ver models/debris-flow/PHYSICS_EXPERIMENT.md.

## Próximos pasos al retomar
1. CI en GitHub Actions (test + clippy + fmt).
2. Borrador del paper (EMS/JASSS): patas — determinismo, paridad Mesa,
   benchmarks 45-67x, debris-flow ~100x, calibración robusta ~400x,
   benchmark de metaheurísticas con stats. Posible 2o paper (geociencia):
   "comparación de calibradores de ABM espacial habilitada por HPC".
