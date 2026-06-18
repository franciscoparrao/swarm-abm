# Changelog

Formato basado en [Keep a Changelog](https://keepachangelog.com/).
El proyecto sigue [SemVer](https://semver.org/). Mientras `0.x`, la API
puede cambiar entre minors.

## [Sin publicar]

### Añadido

- **Bindings Python (PyO3)** en `crates/swarm-py` (módulo `swarm_abm`),
  estrategia *modelos nativos + barridos*: clases `Sir`, `Schelling` y
  `Sugarscape` (misma API `run`/`series`/getters) y un barrido paralelo por
  modelo (`sir_sweep`, `schelling_sweep`, `sugarscape_sweep`, con el GIL
  liberado). El bucle corre íntegro en Rust; paridad bit a bit con los
  binarios nativos verificada. Se construye con maturin (fuera del workspace).
- **Crate `swarm-models`**: los modelos de referencia (SIR, Schelling,
  Sugarscape) se extraen a una librería reutilizable por ejemplos, bindings y
  benches, para no duplicar la física entre el ejecutable y el binding.
- **Ejemplo `sugarscape`** (Epstein & Axtell, 1996): movimiento + muerte de
  agentes + paisaje con estado; desigualdad emergente (Gini 0.24 → 0.42) y
  población autorregulada.

### Cambiado

- `examples/sir`, `examples/schelling` y `examples/sugarscape` pasan a ser
  binarios delgados sobre `swarm-models` (misma salida, paridad bit a bit).
- CI: cubre el camino `--no-default-features` (WASM/secuencial) y falla ante
  warnings de rustdoc.

## [0.3.0] — 2026-06-18

El motor pasa de un solo espacio (grilla) a **tres paradigmas espaciales
bajo el mismo `Agent`/`Model`**, más ejecución en lote.

### Añadido

- **Espacio de grafo** (`graph::Graph<T>`, `NodeId`): generadores
  deterministas Erdős–Rényi, Watts–Strogatz y Barabási–Albert; `neighbors`,
  `degree`, `random_neighbor`, indexado por nodo. Ejemplo `network-sir`:
  contagio SIR sobre topologías aleatoria, mundo-pequeño y libre de escala.
- **Espacio continuo** (`continuous::ContinuousSpace<T>`, `Vec2`, `PointId`):
  vecindad por radio con *spatial hashing*, `for_each_within` sin asignación,
  distancia/delta toroidales, torus opcional. Ejemplo `boids`: flocking de
  Reynolds (orden de Vicsek 0.02 → 0.96).
- **Batch runner** (`batch::run_ensemble`, `batch::run_sweep`, `SweepCell`):
  réplicas y barridos de parámetros en paralelo vía Rayon, tras la feature
  `parallel` (activa por defecto; sin ella el camino es secuencial, apto WASM).
- **Modelo cliente real** `models/debris-flow`: port del evento Atacama 2015,
  paridad distribucional vs el original Mesa/Python, ~100× más rápido.
  Calibración por Differential Evolution + benchmark de 5 metaheurísticas
  (DE/GA/PSO/SA/GWO con tests de Friedman + Wilcoxon).

### Cambiado

- Roadmap y README reflejan los tres espacios como el argumento central de
  generalidad del motor.

## [0.2.0]

### Añadido

- **Activación simultánea** (`Activation::Simultaneous`) en dos fases
  (`decide`/`apply`) con el modelo inmutable en `decide` — garantía del
  compilador, no disciplina del usuario. Validado con el Juego de la Vida.
- Validación de **paridad numérica contra Mesa**: espejos de Schelling y SIR
  en Python, protocolo de paridad distribucional (7/7 métricas), benchmark
  cross-engine con criterion.
- Optimización del camino caliente: el runner reutiliza el buffer de orden
  y `Grid2D::random_neighbor` no asigna memoria.

## [0.1.0]

### Añadido

- Núcleo del motor: traits `Agent`/`Model`, `Simulation`, patrón **take-out**
  para acceso mutable al modelo sin conflicto de préstamos.
- Scheduler con activación ordenada y aleatoria (`Schedule`, `Activation`).
- `Grid2D` con vecindades Moore/Von Neumann, torus opcional y `diffuse`.
- `DataCollector` de series por paso (`to_csv`) y RNG sembrable portable
  (`SimRng`, ChaCha8). Ejemplos: Schelling, SIR espacial, difusión.
