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
- **debris-flow-abm**: primer modelo cliente; valida el diseño del motor.
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

## Próximos pasos al retomar
1. CI en GitHub Actions (test + clippy + fmt).
2. Reescribir debris-flow-abm sobre el motor para validar generalidad.
