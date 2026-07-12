# Changelog

Formato basado en [Keep a Changelog](https://keepachangelog.com/).
El proyecto sigue [SemVer](https://semver.org/). Mientras `0.x`, la API
puede cambiar entre minors.

## [0.4.0] — 2026-07-10

Publicada en crates.io el 2026-07-10 (`swarm-abm` y `swarm-abm-derive`).
Acumula dos tandas de correcciones de auditoría posteriores a la
publicación de 0.3.0: la **auditoría de seguimiento** del 2026-07-05
(F1–F6) y la **tercera pasada** del 2026-07-10 (5 altos, 9 medios y ~10
bajos; ver `docs/AUDIT.md`). Ninguna de las dos está en la 0.3.0 de
crates.io — 0.4.0 es la primera versión publicada con los fixes de
determinismo del GSA (CRN de Sobol/Morris) y del checkpoint.

### Rompe determinismo

Cambian los bits exactos para una semilla dada — re-validar resultados
numéricos publicados (los índices de Sobol de SIGRID incluidos):

- `experiment::sobol` usa *common random numbers*: `A[j]` y cada
  `AB_i[j]` comparten semilla por fila (`B[j]` independiente), como exige
  el esquema de Saltelli. Antes cada punto corría con RNG propio, inflando
  `ST` de parámetros inertes con ruido puro en modelos estocásticos (F3).
- La secuencia Sobol′ ahora descarta los primeros `n.next_power_of_two()`
  puntos (bloque alineado diádicamente, Owen 2020) en vez del origen a
  secas: `.skip(1)` degradaba la convergencia QMC de ~O(1/n) hacia
  O(n^(−1/2)) — con skip alineado, el error del estimador a n=4096 cae
  ~3 órdenes de magnitud (3ª pasada, corrige el alcance corto de F4).
- `experiment::morris` usa *common random numbers* por trayectoria: los
  `d+1` puntos de una trayectoria comparten semilla, así los efectos
  elementales de un parámetro inerte son 0 y no ruido (3ª pasada, el
  espejo de F3 que la 2ª pasada no revisó).
- `Grid2D::neighbor_positions_r` con VonNeumann + torus + radio grande ya
  no sub-incluye vecinos (F5); `neighbor_positions`/`random_neighbor` ya
  no incluyen la celda propia en grillas toroidales degeneradas (algún
  eje de dimensión 1) — grillas con ejes ≥ 3 no cambian ni un bit.
- `ContinuousSpace::wrap` toroidal ya no puede devolver exactamente
  `width`/`height` (redondeo de `rem_euclid` con residuo negativo
  diminuto); el punto se normaliza a `0.0`.
- Los 4 ejemplos (`network-sir`, `boids`, `difusion`, `life`) migran a las
  primitivas propias de `rng` (cambia su stream, no el del motor).

### Cambios de API (breaking respecto de 0.3.0)

- `Simulation::from_checkpoint` exige el `Schedule` como quinto parámetro:
  antes fijaba `Random` en silencio y el resume no era bit-exacto para
  `Ordered`/`Simultaneous`/`Staged` (F2).
- `swarm-wasm`: los constructores toman `seed: u64` (antes `u32`); en JS
  la semilla se pasa como `BigInt`. Corridas nativas con semilla ≥ 2³²
  ahora son reproducibles en el navegador.

### Corregido

- `#[derive(MultiAgent)]` despacha `Agent::stage`: un enum multi-especie
  bajo `Activation::Staged` ya no cae al no-op en silencio (F1).
- `swarm-py` compila de nuevo: el `#[pymodule]` colisionaba con el nombre
  del crate renombrado (E0659, roto desde 0.3.0); el módulo Python sigue
  siendo `swarm_abm`.
- Un NaN en las evaluaciones de Sobol propaga NaN a los índices en vez de
  colapsarlos todos a 0.0 exacto en silencio (el peor modo de falla:
  "nada es sensible" como resultado aparentemente válido).
- `sobol_indices_with_bootstrap` con `n_boot = 0` degrada a `(NaN, NaN)`
  en vez de panic por underflow (F6).
- `ContinuousSpace::for_each_within` con radio no finito o astronómico
  devuelve todos los puntos en vez de panic (debug) / vacío (release).
- Morris: nivel base máximo en aritmética entera (la fórmula flotante
  perdía un nivel válido para `levels` ∈ {30, 88, 150, …}); `sigma` con
  varianza muestral (n−1, como SALib), no poblacional.
- `from_checkpoint` documenta que `collect_every` vuelve a 1 y debe
  re-aplicarse (los reporters ya estaban documentados).
- Rustdoc honesto: el orden de iteración es por slot (igual al de
  inserción solo hasta que un remove es seguido de un insert) — 4 sitios
  prometían "insertion order" pre-arena; `# Panics` completos en `graph`.

### Añadido

- **Golden test de trayectoria completa** (`tests/golden_values.rs`):
  pinnea posiciones finales y hash FNV-1a de una simulación entera
  (10×10, 10 agentes, 20 pasos, semilla 42) — corre también en wasm32 en
  CI. Su falla significa ruptura del contrato de reproducibilidad: exige
  bump minor + entrada aquí, no re-pinnear.
- Cobertura nueva: `Staged` bit-idéntico por los 4 entry points de step;
  checkpoint bajo `Simultaneous` y `Staged`; CRN de Sobol y Morris con
  modelos de ruido puro.
- CI: job `bindings` (`cargo check` de swarm-py y swarm-wasm — estaban
  fuera del workspace y nada los compilaba: así se rompió swarm-py sin
  que nadie lo viera), clippy `--all-features` en stable, y el camino
  secuencial de `experiment` (`--no-default-features --features
  experiment,serde`, el camino WASM declarado).

### Cambiado

- Versión del workspace a 0.4.0; los bindings (que nacieron en 0.4.0)
  quedan por fin alineados con el motor que envuelven.
- `docs/REPRODUCIBILITY.md`: el checkpoint son **cinco** piezas (falta el
  `Schedule` era exactamente la omisión detrás de F2); portada de docs.rs
  actualizada (describía el motor v0.1, sin Graph/Continuous/experiment).

## [0.3.0] — 2026-07-02 — primera publicación en crates.io

> Publicada en crates.io el 2026-07-02. **Advertencia**: esta versión
> contiene los 6 defectos F1–F6 corregidos después de publicar (ver la
> sección "Sin publicar" de arriba y `docs/AUDIT.md`, "Auditoría de
> seguimiento — 2026-07-05"); en particular el Sobol sin *common random
> numbers* (F3) y el `from_checkpoint` que ignora el `Schedule` (F2).
> Quien dependa de `experiment::sobol` o de checkpoints debería esperar
> a 0.4.0.

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
    Internaliza el arnés híbrido SALib+Rust que usaba SIGRID: el modelo
    migró a él (`models/sigrid/src/bin/sobol_native.rs`, binario
    `sobol-native`), reemplazando `Isla_Riesco/experiments/sobol_rust.py` —
    muestreo Saltelli, evaluación y S1/ST con bootstrap, todo en Rust, sin
    Python en el camino del análisis de sensibilidad (ver
    `models/sigrid/PARITY.md`).
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

## [0.3.0-dev] — 2026-06-18 — interna, previa a la publicación

> Numerada 0.3.0 en el árbol antes de que existiera la publicación en
> crates.io; nunca se publicó como tal. Se re-etiqueta aquí para
> distinguirla de la 0.3.0 real de crates.io (arriba).

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
