# Changelog

Formato basado en [Keep a Changelog](https://keepachangelog.com/).
El proyecto sigue [SemVer](https://semver.org/). Mientras `0.x`, la API
puede cambiar entre minors.

## [Sin publicar]

### Rompe determinismo

Ver `docs/REPRODUCIBILITY.md` para la política completa. Estos cuatro
cambios alteran los bits exactos que produce el motor para una semilla
dada — no solo la API — así que cualquier resultado numérico publicado con
una versión anterior necesita re-validarse (ver
`models/sigrid/PARITY.md`, sección "Re-validación 2026-07-02", como caso
de estudio real de qué implica eso: los números cambiaron, las
conclusiones científicas no).

- `Graph::barabasi_albert` dejó de depender del orden de iteración (no
  garantizado) de un `HashSet` interno.
- `rng::{uniform_below, uniform_usize, uniform_f64, bernoulli, shuffle}`
  reemplazan el uso interno y recomendado de `rand::Rng::random_range`/
  `random_bool`/`SliceRandom::shuffle` (algoritmo no especificado por
  `rand`, sujeto a cambiar entre versiones sin previo aviso).
- `rng::child_rng` combina `(semilla, paso, agente)` en cadena (estilo
  hash-combine) en vez de con XOR de tres hashes independientes.
- `AgentSet`/`ContinuousSpace` pasan a una arena generacional con
  reutilización de slots (`AgentId`/`PointId` = `{index, generation}`):
  cualquier modelo con demografía (inserciones/remociones en runtime)
  puede ver una asignación distinta de índices frente a versiones
  anteriores.

### Añadido

- **Auditoría de ingeniería completa del motor** (`docs/AUDIT.md`): 5
  hallazgos P0 (correctitud del determinismo), 9 P1 (arquitectura) y el
  diferenciador P3-4, todos resueltos con test coverage real (no solo
  "compila"). Resumen de lo nuevo:
  - `swarm_abm::experiment` (feature opcional `experiment`): diseño de
    experimentos **determinista por construcción** — `sobol`/
    `latin_hypercube`/`morris`, con análisis de sensibilidad global
    (S1 de Saltelli 2010, ST de Jansen 1999, bootstrap 95% CI), validado
    contra la función de Ishigami (índices analíticos conocidos).
    Internaliza el arnés híbrido SALib+Rust que usaba SIGRID.
  - `#[derive(MultiAgent)]` (crate `swarm-abm-derive`, macro procedural):
    heterogeneidad de agentes vía `enum` sin campos muertos ni trait
    objects — el despacho sigue siendo un `match` estático.
  - `Agent::decide_with_peers`/`Simulation::step_with_peers` (+ variante
    paralela): la fase `decide` puede observar un snapshot congelado de
    **todos** los agentes, aditivo (no cambia la firma de `decide`, no
    fuerza `Clone` a modelos que no lo usan).
  - `Activation::Staged(n)` + `Agent::stage`: N barridos completos por
    paso con modelo mutable, garantizando que todos los agentes completen
    la etapa `s` antes de que cualquiera entre a `s+1` (patrón
    `StagedActivation` de Mesa).
  - `Simulation::from_checkpoint`/`rng_state`/`seed` (feature `serde`):
    checkpoint/restore bit-exacto a partir de 4 piezas mínimas (modelo,
    semilla, estado del RNG, pasos corridos).
  - `AgentDataCollector`: recolección de series **por agente** (no solo a
    nivel modelo), más `Simulation::with_collect_every` para muestrear 1
    de cada N pasos.
  - `Graph<N, E = ()>` generalizado con pesos por arista
    (`add_weighted_edge`) y grafos dirigidos (`directed(bool)`).
  - `Grid2D::neighbor_positions_r`/`neighbors_r`/`random_neighbor_r`:
    vecindades de radio arbitrario (antes fijo en 1), con muestreo por
    reservorio.
  - `ContinuousSpace` v2: arena generacional con `remove()` real, índice
    espacial en buckets planos (sin `Vec<Vec<PointId>>`), sin `HashSet`
    en `for_each_within`.
  - Tests de **valores dorados** (`tests/golden_values.rs`): pinnean los
    bytes exactos que produce cada primitiva de RNG del motor, para
    detectar deriva silenciosa de una dependencia (`rand`/`rand_chacha`)
    entre actualizaciones — no solo deriva propia del motor.
- **CI en `wasm32-wasip1`** (`golden-values-wasm32`): corre los mismos
  valores dorados bajo `wasmtime`, verificando en cada push que la
  identidad cross-platform del motor no es una afirmación sino un gate de
  CI. Job `msrv` adicional que fija el MSRV real (1.87.0, verificado
  empíricamente, no solo declarado) en un toolchain pineado.
- **`docs/REPRODUCIBILITY.md`**: política de estabilidad de primera clase
  para el contrato de determinismo del motor — qué está garantizado, qué
  no, y qué cuenta como cambio que rompe determinismo.
- **Rustdoc, README y mensajes públicos en inglés**: los 12 archivos
  fuente de `swarm-abm` (~4800 líneas), `swarm-abm-derive`, y los dos
  `README.md` (raíz + `swarm-wasm`) traducidos para adopción
  internacional — ver P3-1 en `docs/AUDIT.md` para el alcance exacto.
- **`decide` paralelo intra-paso** para activación simultánea
  (`Simulation::run_parallel` / `step_parallel`, feature `parallel`): la fase
  `decide` se reparte entre hilos con rayon y da un resultado **bit-idéntico**
  al secuencial, gracias a un RNG por-agente (`rng::child_rng`, derivado de
  `(semilla, paso, id)`) y a que `decide` recibe el modelo inmutable (el
  compilador prueba que no hay escritura compartida). `step`/`run` siguen
  genéricos y secuenciales (no se exige `Send`/`Sync` ni se afecta el camino
  WASM). Escala ~5× a 16 hilos en decisiones compute-bound (Amdahl: `apply`
  sigue secuencial). Ejemplo `life` (Juego de la Vida) como banco; V&V en
  `tests/parallel_decide.rs`; medición en `validation/SCALABILITY.md`.
- Benchmark **cross-engine** vs Agents.jl (Julia) y Mesa: swarm-abm ~2–5× más
  rápido que Agents.jl y ~45–184× que Mesa en SIR y Schelling
  (`validation/CROSS_ENGINE.md`).
- **V&V del RNG y determinismo cross-platform** (`validation/RNG_AND_DETERMINISM.md`):
  el stream inter-agente de `child_rng` pasa PractRand 0.95 sin anomalías hasta
  1 GB (decorrelación entre agentes), y los modelos dan resultados **bit-idénticos
  entre x86-64 y wasm32** (métricas sensibles a la configuración: segregación de
  Schelling, Gini de Sugarscape). Herramienta `examples/rng-dump`.
- **Visor WASM** en `crates/swarm-wasm`: compila el motor a WebAssembly y corre
  Schelling, SIR y Sugarscape sobre un `<canvas>` (bucle en wasm, JS solo dibuja
  el buffer RGBA por paso). Binario ~68 KB, determinista (paridad con native
  verificada). Se construye con `wasm-pack` (fuera del workspace).
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

- **Los crates del motor se renombraron y se publicaron en crates.io**:
  `swarm-core` → [`swarm-abm`](https://crates.io/crates/swarm-abm),
  `swarm-derive` → [`swarm-abm-derive`](https://crates.io/crates/swarm-abm-derive)
  (ambos v0.3.0). Motivo del rename: al preparar la publicación (P3-3) se
  descubrió que ambos nombres originales ya existen en crates.io,
  registrados por terceros sin relación con este proyecto — `swarm-derive`
  son macros de `tetsy-libp2p`, `swarm-core` es un orquestador de agentes
  de IA no relacionado con ABM. `swarm-abm` coincide con el nombre del
  repositorio. Quien tenga una dependencia local por `path` a
  `crates/swarm-core`/`crates/swarm-derive` necesita actualizarla.
- `examples/sir`, `examples/schelling` y `examples/sugarscape` pasan a ser
  binarios delgados sobre `swarm-models` (misma salida, paridad bit a bit).
- CI: cubre el camino `--no-default-features` (WASM/secuencial) y falla ante
  warnings de rustdoc.
- `rand` se usa sin default-features (solo `alloc`): el motor solo necesita RNG
  sembrado (ChaCha8), no `std_rng`/`os_rng`/`thread_rng` — así no arrastra
  `getrandom` y compila a `wasm32-unknown-unknown`.

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
