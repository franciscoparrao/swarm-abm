# swarm-abm frente al estado del arte (análisis SOTA)

> Doble propósito: (a) **roadmap** del motor, priorizando mejoras por ROI; y
> (b) material de **related work / positioning** para el paper (SIMPAT exige
> situar una herramienta frente al campo). La evidencia más fuerte de las
> brechas viene de **dogfoodear el motor con un modelo real** (el port de SIGRID,
> ver `models/sigrid/PARITY.md`): varias limitaciones emergieron en la práctica,
> no en el papel.

## Ejes de comparación

1. **Heterogeneidad de agentes** — ¿múltiples tipos idiomáticos?
2. **Espacios** — grilla / continuo / grafo / GIS (raster+vector) / OSM.
3. **Scheduling** — ordenado / aleatorio / simultáneo / por eventos / escalonado.
4. **Determinismo & reproducibilidad** — ¿reproducible? ¿bit a bit? ¿el
   paralelismo preserva el resultado? ¿RNG sembrable por-agente?
5. **Paralelismo CPU** — multihilo; qué se paraleliza.
6. **GPU** — ejecución en GPU y escala.
7. **Distribuido / multi-nodo (MPI)**.
8. **Diseño de experimentos** — barridos / sensibilidad (Sobol/Morris) /
   calibración (ABC, optimización, surrogate) integrados.
9. **Visualización** — viewer interactivo; web.
10. **WebAssembly** — corre en navegador.
11. **GIS** — consume rásters/vectores reales.
12. **V&V / ODD / provenance** — descripción de modelo, tests, reproducibilidad.

## Matriz de capacidades

> ⚠️ **ESTADO DE VERIFICACIÓN.** Las columnas de competidores partieron del
> **conocimiento de base (cutoff enero 2026)**. Las 5 celdas `‡` originales
> (las de mayor riesgo para el paper) ya se verificaron contra fuentes en
> vivo — ver "Verificación parcial vía OpenAlex", "Verificación con fuentes
> primarias" y "Verificación en vivo (2026-07-03)" abajo — así que la matriz
> **ya no depende solo de conocimiento de base** para esas 5. El resto de las
> celdas (sin `‡`) siguen sin verificación en vivo; tratarlas como borrador si
> se citan en el paper. swarm-abm se autoevaluó contra el repo.

| Eje | swarm-abm | Mesa 3.x | Agents.jl | NetLogo | krABMaga | FLAME GPU 2 | GAMA |
|---|---|---|---|---|---|---|---|
| Lenguaje | Rust 2024 | Python | Julia | JVM | Rust | C++/CUDA | Java/GAML |
| Heterogeneidad | ⚠️ monomórfica | ✅ clases | ✅ `@multiagent` | ✅ breeds | ⚠️ trait | ✅ tipos/estados | ✅ species |
| Grilla | ✅ Moore/VN+diffuse | ✅ | ✅ | ✅ patches | ✅ | ✅ | ✅ |
| Continuo | ✅ hash, radio | ✅ | ✅ | ⚠️ | ✅ | ✅ msg espacial | ✅ |
| Grafo/red | ✅ ER/WS/BA | ✅ networkx | ✅ | ✅ links | ✅ | ✅ msg grafo | ✅ |
| GIS raster/vector | ❌ (ad-hoc) | ⚠️ mesa-geo | ⚠️ OSM | ⚠️ ext GIS | ❌ | ❌ | ✅✅ nativo |
| Scheduling | Ord/Rand/Simul | rand/staged | flexible | ask(rand) | ord/rand | por capas | configurable |
| **Determinismo bit a bit** | ✅✅ **par==seq** | ⚠️ 1-hilo | ⚠️ 1-hilo (sin threading intra-modelo) | ✅ 1-hilo | ⚠️ no documentado | ❌ GPU | ⚠️ 1-hilo |
| RNG por-agente sembrable | ✅ child_rng | ⚠️ global | ⚠️ global | ⚠️ global | ⚠️ | ⚠️ por-agente | ⚠️ global |
| Paralelismo CPU | ⚠️ `decide` ~5× | ⚠️ entre-runs | ⚠️ entre-runs (Dict no thread-safe) | ⚠️ entre-runs | ✅ intra (experimental) | ✅✅ GPU | ✅ intra+runs |
| GPU | ❌ | ❌ | ❌ | ❌ | ❌ | ✅✅ | ❌ |
| Distribuido (MPI) | ❌ | ❌ | ⚠️ Distributed | ⚠️ cluster | ✅ solo exploración | ✅ multi-GPU | ⚠️ headless |
| Experimentos integrados | ⚠️ sweep | ⚠️ batch_run | ⚠️ paramscan | ⚠️ BehaviorSpace | ✅ exploración | ⚠️ ensembles | ✅✅ SA+calib |
| — sensibilidad (Sobol/Morris) | ✅ (nativo, `swarm_abm::experiment`) | ❌ | ❌ | ❌ | ❌ | ❌ | ✅ (≥1.8.2) |
| — calibración | ❌ | ❌ | ❌ | ❌ | ✅ Bayes/GA (README) | ❌ | ✅ (1.8.1) |
| Visualización | ⚠️ WASM básico | ✅ Solara | ✅ Makie | ✅✅ | ✅ bevy | ✅ OpenGL | ✅✅ IDE |
| WebAssembly | ✅ bit-idéntico | ❌ | ❌ | ⚠️ JS reimpl | ✅ | ❌ | ❌ |
| V&V/ODD nativo | ⚠️ paridad+det | ❌ | ❌ | ⚠️ norma comunidad | ❌ | ❌ | ❌ |
| Bindings | PyO3+wasm | — | — | — | — | Python (pyflamegpu) | — |

Leyenda: ✅✅ líder · ✅ fuerte · ⚠️ parcial/limitado · ❌ ausente. (La marca
`‡` de versiones anteriores de este documento señalaba celdas de alto riesgo
sin verificar; las 5 originales se verificaron — ver abajo — así que ya no
aparece en la matriz.)

### Celdas de alto riesgo — estado tras verificación (originalmente `‡`)
1. **krABMaga — determinismo bajo paralelismo.** *La más importante.*
   ✅ **Verificado 2026-07-03** (README en vivo + reconfirmado hoy): 0
   menciones de determinismo/reproducibilidad/seed/RNG. Su propio feature
   `parallel` está en estado **Experimental** (no Release Candidate, a
   diferencia de `visualization`/`distributed-mpi`/`bayesian`) — ni ellos lo
   tratan como listo para producción. Sigue siendo inferencia por ausencia,
   no negación explícita del equipo, pero es el máximo nivel de verificación
   posible sin contactar a los autores.
2. **krABMaga — MPI/distribuido, exploración (Bayes/GA), WASM/bevy.**
   ✅ Confirmado (README primario, ver abajo): los tiene todos, con el matiz
   de que MPI distribuye la *exploración* de configuraciones, no descompone
   una sola simulación.
3. **FLAME GPU 2 — determinismo en GPU.** ✅ Confirmado vía Richmond et al.
   2023: resuelve *race conditions* con sub-modelos pero sin reclamo de
   bit-determinismo.
4. **GAMA — sensibilidad (Morris/Sobol) y calibración integradas.**
   ✅ **Verificado 2026-07-03** (docs en vivo, `gama-platform.org`): Sobol,
   Morris y un tercer índice Beta están integrados como "Analysis Methods";
   el changelog los ubica ya presentes en 1.8.2 (ausentes en 1.8.1, la
   versión que se había revisado antes) y siguen en 1.9.1 y la versión
   actual. Calibración: además de `hill_climbing`/`simulated_annealing`/
   `tabu`/`genetic` ya confirmados, la doc actual suma `reactive tabu` y
   `particle swarm optimization`. GAMA sigue siendo el benchmark a batir en
   GSA+calibración nativas (Tier-1 #1), y ahora con fecha de referencia
   concreta (≥1.8.2) en vez de "1.9+" especulativo.
5. **Agents.jl — determinismo bajo `ensemblerun!`/threads.**
   ✅ **Verificado 2026-07-03**: Agents.jl **no tiene** paralelismo
   intra-modelo soportado — reportes de la comunidad (Julia Discourse,
   issues de GitHub) confirman que `Threads.@threads` sobre `allagents(model)`
   falla porque los `Dict` internos no son thread-safe. Su único paralelismo
   es `ensemblerun!(parallel=true)`, que distribuye corridas **independientes**
   (cada una con su propia semilla) vía `Distributed`, no agentes de una
   misma corrida entre hilos. La pregunta original ("¿determinismo bajo
   threads?") no aplica del todo: no hay threading intra-simulación que
   pudiera romper el determinismo. Coherente con la celda ya existente
   "⚠️ entre-runs" de la fila Paralelismo CPU — no fue necesario corregirla,
   solo confirmarla.

### Verificación parcial vía OpenAlex (abstracts de papers canónicos)

Hecha con la API de OpenAlex (sin docs en vivo, pero son fuentes citables donde
los autores declaran capacidades). Resultados:

- **krABMaga** — Antelmi et al., *JASSS* 2024, `10.18564/jasss.5300`. El abstract
  **confirma**: Rust; "model exploration and optimization capabilities over
  **parallel, distributed, and cloud architectures**"; "**visualization**
  features"; "model **calibration** experiment over an AWS EC2 cluster". Su
  reclamo de fiabilidad es **memory-safety** ("reliability, and safeness",
  "memory flaws which could invalidate the experiment results"), **NO
  determinismo/reproducibilidad bajo paralelismo** (búsqueda full-text de
  "krABMaga deterministic/reproducible" → 0 resultados). ⇒ El wedge de swarm-abm
  se sostiene, pero **el eje es distinto**: krABMaga = seguro de memoria;
  swarm-abm = reproducible por construcción.
- **FLAME GPU 2** — Richmond et al., *Softw. Pract. Exper.* 2023,
  `10.1002/spe.3207`. Confirma GPU, ensembles (barridos), millones de agentes y
  que resuelve *race conditions* con sub-modelos. **Sin** reclamo de
  bit-determinismo.
- **Agents.jl** — Datseris et al., *SIMULATION* 2022,
  `10.1177/00375497211068820`. "Most performant + feature-full + integra con el
  ecosistema Julia". Determinismo bajo paralelismo: no abordado en el abstract.
- **GAMA** — Taillandier et al., *LNCS* 2011, `10.1007/978-3-642-25920-3_17`.
  Confirma GIS como núcleo ("Integrates Geographical Information Data"). Su
  sensibilidad (Morris/Sobol) y calibración integradas **no** se verificaron
  (son de versiones posteriores) — confirmado más abajo, "Verificación en
  vivo (2026-07-03)": GAMA ≥1.8.2.
- **Mesa** — Kazil et al., *LNCS* 2020, `10.1007/978-3-030-61255-9_30`.
- **Repast Simphony** — North et al., *CASM* 2013, `10.1186/2194-3206-1-3`.

> **Corrección al borrador.** krABMaga **ya tiene** exploración de modelos +
> optimización/calibración + distribuido; GAMA tiene experimentos integrados.
> Por lo tanto el "espacio en blanco" de swarm-abm **no** es *tener* diseño de
> experimentos (varios lo tienen), sino que sea **determinista/reproducible bit
> a bit** — un eje que ninguno reclama. Eso afina la tesis (ver abajo).

### Verificación con fuentes primarias (curl a repos/docs)

- **krABMaga README** (`raw.githubusercontent.com/krABMaga/krABMaga`, rama main).
  Tabla de features confirma, textual:
  - `parallel` = *"Speed-up a single simulation parallelizing agent scheduling
    during a step"* → paralelismo **intra-paso** ✅.
  - `distributed-mpi` = *"distributed model **exploration** using MPI… the amount
    of configurations are balanced among your nodes"* → MPI distribuye el
    **barrido de parámetros** entre nodos, **no** descompone una sola simulación.
    Matiz clave: su "distribuido" es para *exploración*, no para una corrida.
  - `visualization` (Bevy) ✅ y `visualization-wasm` (WebAssembly) ✅ → **cierra
    la celda WASM** de krABMaga (es para viz en navegador).
  - `bayesian` = Bayesian Optimization; exploración = Parameter Sweeping +
    Genetic + Random ✅.
  - **Determinismo/reproducibilidad: 0 menciones** (`grep determinist|reproducib|
    seed|rng` en el README → vacío). Confirma por ausencia que NO es su pitch.
- **GAMA `ExplorationMethods.html`** (wiki 1.8.1, repo `gama-platform.github.io`).
  **Calibración/optimización confirmada**: `exhaustive`, `hill_climbing`,
  `simulated annealing`, `tabu`, `genetic` (decenas de menciones). **Sobol/Morris
  NO aparecen en 1.8.1** → la sensibilidad global integrada es de versiones
  posteriores (confirmado abajo: GAMA ≥1.8.2).

Estado de celdas tras esta verificación: krABMaga (paralelo intra-paso, MPI-para-
exploración, WASM-viz, Bayes/GA/sweep) y GAMA (GIS, calibración) quedan con
**fuente primaria**; el *no-determinismo* de ambos es inferencia por ausencia, no
negación explícita. Resto de celdas: conocimiento sin verificar.

### Verificación en vivo (2026-07-03)

A diferencia de las secciones anteriores (hechas sin acceso a red), esta
verificación usó WebSearch/WebFetch en vivo. Cierra las 2 celdas `‡` que
seguían abiertas (GAMA versión de Sobol/Morris, determinismo de Agents.jl) y
reconfirma krABMaga.

- **GAMA — Sobol/Morris/Beta confirmados como nativos.**
  [ExplorationMethods](https://gama-platform.org/wiki/ExplorationMethods) (doc
  actual, 2025-06) lista Sobol, Morris y Beta bajo "Analysis Methods", más
  Factorial/Uniform/Latin Hypercube/Orthogonal como exploration methods y
  Hill Climbing/Simulated Annealing/Tabu/Reactive Tabu/Genetic/PSO como
  calibración. El [Changelog 1.8.2](https://gama-platform.org/wiki/1.8.2-RC2/Changelog)
  ubica la limpieza de la documentación de `sobol` ya en esa versión — coherente
  con la verificación anterior de que 1.8.1 no lo tenía. GAMA es, con esto, el
  único motor de la matriz (además de swarm-abm) con GSA nativo — pero sin la
  garantía de determinismo bit a bit que sí tiene swarm-abm (GAMA es JVM,
  sin ese reclamo documentado).
- **Agents.jl — sin paralelismo intra-modelo soportado.** Búsquedas en Julia
  Discourse y GitHub (issues
  [#500](https://github.com/JuliaDynamics/Agents.jl/issues/500) y
  [#670](https://github.com/JuliaDynamics/Agents.jl/issues/670)) muestran que
  `Threads.@threads` sobre agentes falla por `Dict`s no thread-safe en el core;
  el único paralelismo documentado es `ensemblerun!(parallel=true)` vía
  `Distributed` (corridas independientes, cada una con su propia semilla, no
  hilos sobre una corrida). El issue #500 (RNG que se desincroniza según el
  método de recolección de datos) es un bug de inicialización, no de
  paralelismo — tampoco compromete la comparación.
- **krABMaga — reconfirmado.** El
  [README](https://github.com/krABMaga/krABMaga) actual sigue sin mencionar
  determinismo/reproducibilidad/seed/RNG, y su tabla de features marca
  `parallel` como **Experimental** (el resto de features listadas —
  `visualization`, `visualization-wasm`, `distributed-mpi`, `bayesian` — están
  en Release Candidate). Dato nuevo respecto a la verificación previa: ni el
  propio proyecto trata su paralelismo intra-simulación como maduro.

## Lectura estratégica (qué implica la matriz)

- **krABMaga es el único solapamiento-amenaza real.** Comparte stack (Rust,
  paralelo intra-modelo, WASM, viz, exploración, posiblemente MPI). **Todo el
  wedge de swarm-abm descansa en una sola celda: determinismo bajo paralelismo.**
  Prioridad absoluta de verificación; si cae, hay que repensar el posicionamiento.
- **GAMA domina GIS + diseño de experimentos integrado** (sensibilidad +
  calibración nativas). Es el benchmark a batir para las mejoras Tier-1 #1
  (experimentos nativos) y Tier-2 #4 (GIS). El ángulo de swarm-abm frente a GAMA:
  **determinismo + performance de Rust + experimentos HPC reproducibles** (GAMA
  es JVM y no garantiza reproducibilidad bajo paralelismo).
- **FLAME GPU 2 es dueño de la escala** (GPU, millones+ de agentes). No competir
  en conteo bruto de agentes; competir en **reproducibilidad** (lo que GPU
  sacrifica).
- **Mesa y NetLogo son dueños del ecosistema/adopción/viz**, no de la
  performance. No son competidores de rendimiento.
- **El espacio en blanco (afinado tras verificar):** krABMaga (JASSS 2024) y
  GAMA **ya tienen** exploración/calibración integradas, así que *tener* diseño
  de experimentos no es el diferenciador. Lo que **nadie reclama** es que esos
  experimentos sean **deterministas/reproducibles bit a bit** — krABMaga vende
  *memory-safety*, FLAME GPU sacrifica reproducibilidad por escala GPU, GAMA es
  JVM sin garantía bajo paralelismo. ⇒ La tesis de producto de swarm-abm es
  estrecha y defendible: **diseño de experimentos (GSA/calibración) determinista
  por construcción** — reproducibilidad como propiedad, no como convención.

## swarm-abm: dónde está parado (autoevaluación)

- **El wedge defendible es el determinismo por construcción**: ejecución
  paralela bit-idéntica a la secuencial, RNG por-agente `child_rng(seed,step,
  agent)`, validado con PractRand y con identidad x86-64 ↔ wasm32. Ningún
  competidor combina Rust + paralelo + WASM **con** esta garantía (krABMaga es
  el más cercano en stack; verificado 2026-07-03 que no documenta determinismo
  bajo paralelismo y que su propio `parallel` es Experimental, no
  Release Candidate).
- **Tres espacios** (grilla, grafo, continuo) bajo un solo trait `Agent`/`Model`.
- **Limitaciones reales surgidas en SIGRID** (no especulativas):
  - Heterogeneidad: `AgentSet<A>` es monomórfico → el modelo de 7 especies se
    forzó a un único `struct Animal` con `enum Species` y campos muertos.
  - Sin espacio GIS: hubo que hacer un `Raster` a mano para la vegetación.
  - Índice espacial: se reconstruyó `ContinuousSpace` cada paso (handles
    `PointId` sin constructor estable) y `for_each_within` aloca un `HashSet`
    por llamada.
  - Sin diseño de experimentos nativo (histórico): el Sobol se hizo con SALib
    por fuera (`sobol_eval` en Rust como evaluador, SALib en Python
    muestrea/analiza). Cerrado por P3-4 (`swarm_abm::experiment`) y
    adoptado en SIGRID (`sobol-native`, ver `models/sigrid/PARITY.md`):
    muestreo Saltelli + evaluación + S1/ST con bootstrap, todo en Rust, sin
    Python en el camino.

## Brechas y oportunidades (priorizadas)

### Tier 1 — refuerzan el wedge y SIGRID las expuso
1. **Diseño de experimentos nativo, DETERMINISTA y paralelo.** GSA
   (Sobol/Morris/LHS) **cerrado**: `swarm_abm::experiment` (P3-4 de
   `docs/AUDIT.md`), validado contra Ishigami, adoptado en SIGRID
   (`sobol-native`, ver `models/sigrid/PARITY.md`). Pendiente: **calibración
   determinista nativa** (ABC/DE/surrogate) — hoy `debris-flow` calibra con
   DE a nivel de script del modelo (`calibrate.rs`), no como módulo del
   motor. Verificado (2026-07-03): krABMaga y GAMA ya tienen
   exploración/calibración — el diferenciador **no** es tenerla, sino que sea
   **reproducible bit a bit**. Esa es la feature que nadie ofrece (GAMA es
   JVM sin esa garantía; el `parallel` de krABMaga es Experimental y no la
   reclama) y que convierte tu wedge en producto; extiende la historia
   debris-flow/SIGRID ("inviable→rutinario") con el sello de
   reproducibilidad.
2. **Ergonomía de agentes heterogéneos** (múltiples tipos sin `enum` con campos
   muertos, conservando layout cache-friendly y determinismo). Es la fricción de
   adopción #1 que mostró SIGRID.
3. **Índice espacial**: actualización incremental sin alloc en el hot-path,
   handles estables. Mejora medible, bajo riesgo.

### Tier 2 — cerrar brechas vs SOTA
4. **Espacio GIS-nativo (raster + vector)** — diferenciador por el dominio
   geoespacial del autor (conecta con SurtGIS); SIGRID lo necesitó a mano.
5. **Recolección de datos a nivel agente** (no solo series por paso).
6. **Visualización viva** (más allá del WASM básico).

### Tier 3 — frontera de escala (bets, con cuidado)
7. **Backend GPU** (territorio FLAME GPU 2): determinismo bit a bit en GPU sería
   novel y difícil — encaja con la tesis si se logra.
8. **Distribuido/MPI**: tensiona el posicionamiento single-node determinista; no
   perseguir sin razón fuerte.
9. **Techo de paralelismo más allá de `decide`**: paralelizar `apply` rompe la
   semántica síncrona (límite ya documentado). No forzar.

## Metodología y estado de verificación

- **swarm-abm**: autoevaluado contra el código del repo (a 2026-07, tras el
  cierre de `docs/AUDIT.md` y la migración de SIGRID a Sobol nativo).
  Confiable.
- **Competidores — las 5 celdas de mayor riesgo (antes `‡`): verificadas en
  vivo** (WebSearch/WebFetch, 2026-07-03 y verificaciones previas vía OpenAlex
  + curl a repos/docs) — ver "Verificación parcial vía OpenAlex",
  "Verificación con fuentes primarias" y "Verificación en vivo (2026-07-03)"
  arriba. **El resto de la matriz** (celdas sin marcar) sigue proveniendo de
  conocimiento de base, cutoff enero 2026, sin verificación en vivo. Tratar
  esas como **borrador**: no citarlas en el paper sin confirmar contra la
  fuente actual.
- **Celdas sin verificar que quedan** (menor riesgo para el positioning, pero
  pendientes si se quiere blindar el related-work completo):
  - Mesa: `mesa.readthedocs.io` (Mesa 3.x, `batch_run`, AgentSet, Solara) —
    nunca verificado en vivo en este documento.
  - NetLogo: BehaviorSpace + extensión GIS en el manual — nunca verificado.
  - Repast Simphony, GIS raster/vector de cada motor (fila completa) — solo
    conocimiento de base.
- **Si se quiere cerrar el resto antes del paper**: repetir el patrón de esta
  sección (WebSearch → WebFetch a la doc primaria → citar) para Mesa y
  NetLogo; son las dos filas con más menciones en el related-work de un
  paper SIMPAT/EMS y las que menos verificación tienen hoy.
