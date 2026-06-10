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
- [ ] (v0.2) Activación simultánea; grafos/redes; batch runs + barrido de parámetros; viz WASM.

## Arquitectura tentativa
- `swarm-core`: motor; modelos como crates de ejemplo.
- Targets: native (Rayon para ensembles) + Python (PyO3) + WASM (visor).

## Validación / paridad numérica
Reproducir resultados canónicos (Schelling, SIR) y comparar contra Mesa.

## Venue objetivo
**Environmental Modelling & Software** o **JASSS** (social simulation).

## Conexiones con tu ecosistema
- **debris-flow-abm**: primer modelo cliente; valida el diseño del motor.
- **firespread**: variante ABM de propagación.
- **SurtGIS**: rásters como entorno espacial de los agentes.

## Próximos pasos al retomar
1. Paridad numérica: reproducir Schelling/SIR en Mesa y comparar curvas.
2. Activación simultánea (dos fases) en el scheduler.
3. CI en GitHub Actions (test + clippy + fmt).
4. Reescribir debris-flow-abm sobre el motor para validar generalidad.
