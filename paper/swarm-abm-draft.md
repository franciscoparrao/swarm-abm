# swarm-abm — borrador estructurado del paper del motor

> Documento de estructura (no el manuscrito final). Define título, abstract,
> claims por sección, evidencia disponible y figuras. Sirve para (a) escribir el
> borrador y (b) alimentar `/paper-match`.
>
> **Estado del proyecto:** v0.4 (núcleo + 3 espacios + Python + WASM), CI verde.
> **Venue objetivo declarado (CLAUDE.md):** Environmental Modelling & Software (EMS) o JASSS.
> **Restricción estratégica:** el manuscrito Python de debris-flow está en revisión en ESPL;
> este paper del motor **no se envía** hasta que ESPL decida (para citarlo y evitar auto-scoop),
> y el caso debris-flow se usa aquí solo como **aplicación ilustrativa de reproducción/paridad**,
> sin las mejoras congeladas (abanico/seeding/sedimento).

---

## Título (candidatos)

1. **swarm-abm: a deterministic, high-performance engine for spatial agent-based
   modelling that makes previously intractable analyses tractable**
2. swarm-abm: reproducible spatial agent-based modelling at scale, with a real-world
   environmental application
3. When compute is not neutral: a deterministic Rust engine for spatial ABM and the
   environmental analyses it unlocks

> Preferencia para EMS: el título debe nombrar el software y señalar el *insight*
> generalizable (no solo "es más rápido", sino "habilita ciencia antes inviable").

---

## Abstract (estructura, ~250 palabras)

- **Problema.** El ABM espacial domina en ecología, epidemiología, ciencias sociales y
  geociencias, pero las plataformas de referencia —NetLogo (JVM) y Mesa (Python)— imponen
  un techo de rendimiento que **sesga la práctica científica**: estudios con una sola corrida
  por método, calibraciones a semilla única (sobreajuste), análisis de sensibilidad recortados.
- **Contribución.** Presentamos `swarm-abm`, un motor de ABM espacial en Rust con tres
  paradigmas espaciales (grilla, grafo, espacio continuo) bajo una sola API de agentes,
  **determinismo bit a bit** (RNG sembrable portable), y ejecución paralela de ensembles/barridos.
- **Validación.** Paridad distribucional con Mesa (7/7 métricas, |z| ≤ 1.22), con un speedup
  de 45–67×; el núcleo sostiene decenas de millones de agente-pasos por segundo en un hilo.
- **Aplicación.** Un modelo de flujos de detritos (evento Atacama 2015, Chile) reescrito sobre
  el motor reproduce el original con paridad distribucional ~100× más rápido, y —ya tratable—
  habilita una **calibración robusta multi-semilla** (~400× sobre Python) y un **benchmark de
  cinco metaheurísticas con potencia estadística** (Friedman + Wilcoxon-Holm).
- **Accesibilidad.** Bindings Python (modelos nativos + barridos paralelos) y un visor WebAssembly.
  Open source (MIT/Apache-2.0), determinista y reproducible.
- **Insight.** El costo computacional no es neutral: bajar dos órdenes de magnitud el costo por
  corrida cambia *qué preguntas científicas son respondibles*.

---

## 1. Introducción

**Claims:**
- El ABM espacial es transversal; las herramientas de facto (NetLogo, Mesa) priorizan
  accesibilidad sobre rendimiento y reproducibilidad estricta.
- Tres carencias concretas que limitan la *ciencia*, no solo la velocidad:
  1. **Reproducibilidad**: RNG global sin semilla → resultados no replicables bit a bit.
  2. **Rendimiento**: el costo por corrida fuerza diseños experimentales pobres (N=1 por método).
  3. **Generalidad espacial**: cada herramienta tiende a un paradigma; cambiar de grilla a red
     a espacio continuo suele implicar reescribir.
- **Contribución del paper**: un motor que ataca las tres a la vez, validado contra Mesa y
  demostrado en un caso ambiental real.

**Evidencia/Referencias a reunir:** NetLogo (Wilensky), Mesa (Kazil et al.), Repast, Agents.jl
(Julia, comparador de rendimiento honesto), revisión breve de reproducibilidad en ABM.

## 2. Diseño del motor (Software description)

**Claims + evidencia (todo implementado):**
- **Modelo de agentes**: traits `Agent`/`Model`; patrón *take-out* para acceso mutable al modelo
  sin conflicto de préstamos (sin `RefCell`/`unsafe`). → garantía del compilador, no disciplina del usuario.
- **Scheduler**: activación ordenada / aleatoria / **simultánea en dos fases** (`decide`/`apply`),
  con el modelo inmutable en `decide` (el compilador impide escritura de estado compartido pre-commit;
  a diferencia de Mesa). Validado con el Juego de la Vida.
- **Tres espacios bajo la misma API**:
  - `Grid2D` (Moore/Von Neumann, torus, `diffuse` estilo NetLogo).
  - `Graph<T>` (Erdős–Rényi, Watts–Strogatz, Barabási–Albert, deterministas).
  - `ContinuousSpace` (radio + spatial hashing, sin asignación en el hot path).
- **Determinismo**: `SimRng` = ChaCha8 sembrable, portable entre plataformas (native/WASM).
- **Batch**: `run_ensemble`/`run_sweep`, paralelos con rayon (feature `parallel`), secuenciales para WASM.
- **Recolección de datos**: series por paso, exportables.

**Figura 1**: diagrama de arquitectura (núcleo + 3 espacios + targets native/Python/WASM).

## 3. Verificación y validación (paridad con Mesa)

**Claims + evidencia (hecho):**
- Espejos exactos de Schelling y SIR en Mesa (Python).
- Protocolo de paridad **distribucional**: 50 réplicas por motor, test z de dos muestras por métrica (α=0.05).
- Resultado: **7/7 métricas en paridad** (|z| ≤ 1.22); curvas medias de ensamble difieren < 0.021.
- Difusión converge al punto fijo analítico (validación independiente del RNG).

**Figura 2**: curvas de ensamble swarm-abm vs Mesa (S/I/R y conformidad Schelling) con bandas.

## 4. Rendimiento (benchmarks)

**Claims + evidencia (hecho):**
- Cross-engine SIR vs Mesa: **45–67×** (25²→200², mediana sobre réplicas).
- Criterion: ~25–38 M agente-pasos/s (1 hilo, i7-1270P); 1 M agentes móviles ~12 M/s;
  Life simultáneo 37 M celdas/s; SIR 50×50 completo en 7 ms.
- Hot path sin asignaciones (buffer de orden reutilizado; `random_neighbor` sin alloc).

**Figura 3**: speedup vs Mesa por tamaño de grilla (barras). **Figura 4**: escalamiento agente-pasos/s.

## 5. Aplicación ilustrativa: flujos de detritos (Atacama 2015)

**Claims + evidencia (hecho), enmarcado como reproducción/paridad:**
- Port fiel de un modelo Mesa/Python existente (debris-flow) sobre el motor.
- **Paridad distribucional** con el original sobre insumos idénticos; **~100×** más rápido
  (130–240 s → 1.2–4 s por corrida; 31.8 M celdas Copiapó).
- Reproducibilidad recuperada (el original usaba RNG global sin semilla).
- **Habilitado por el rendimiento** (el argumento central para EMS):
  - **Calibración robusta** por Differential Evolution multi-semilla; detecta y corrige
    sobreajuste a semilla única; ~400× sobre el equivalente Python secuencial.
  - **Benchmark de 5 metaheurísticas** (DE/GA/PSO/SA/GWO) con potencia estadística
    (Friedman χ²=14.3, p=0.006; Wilcoxon-Holm) — el estudio comparativo que el costo de Python impedía.

> **Cuidado ESPL**: presentar como "el motor reproduce y acelera un modelo publicado/ en revisión",
> citando el manuscrito Python; NO incluir las mejoras de métrica (abanico/seeding/SurtGIS).

**Figura 5**: footprint reproducido (mapa) + curva de convergencia de calibración + ranking de optimizadores.

## 6. Accesibilidad y reproducibilidad

**Claims + evidencia (hecho):**
- Bindings **Python** (PyO3): `Sir`/`Schelling`/`Sugarscape` + barridos paralelos; bucle íntegro en Rust,
  análisis en numpy/pandas; paridad bit a bit con native.
- Visor **WebAssembly**: los modelos corren en el navegador (~68 KB), determinista.
- **Open source** MIT/Apache-2.0; CI (test/clippy/fmt/doc); 6 modelos canónicos de ejemplo
  (incluido Sugarscape, Epstein & Axtell 1996).

## 7. Discusión — el insight generalizable

- **El compute no es neutral.** Mostramos dos casos donde bajar el costo por corrida cambió la
  *conclusión científica*, no solo el tiempo: (i) la calibración robusta corrige un sobreajuste
  invisible a semilla única; (ii) el benchmark de optimizadores con N réplicas permite inferencia
  estadística donde antes había una sola corrida por método.
- Determinismo como **requisito metodológico** del ABM reproducible, no un lujo de ingeniería.
- Generalidad espacial: el mismo `Agent`/`Model` sobre grilla, red y continuo reduce el costo de
  cambiar de pregunta.
- Limitaciones: definir modelos requiere Rust (mitigado por bindings); sin paralelismo intra-step aún.

## 8. Conclusiones

- swarm-abm: motor de ABM espacial determinista, de alto rendimiento y multi-paradigma, validado
  contra Mesa y demostrado en un caso ambiental real, con Python/WASM y open source.
- El aporte no es solo velocidad: es **ampliar el espacio de análisis tratables** en modelado ambiental.

---

## Evidencia ya disponible (mapeo a archivos del repo)

| Sección | Respaldo en el repo |
|---|---|
| Paridad Mesa | `validation/REPORT.md`, `validation/run_validation.sh` |
| Benchmarks | `validation/BENCHMARKS.md`, `crates/swarm-core/benches/engine.rs` |
| debris-flow paridad | `models/debris-flow/PARITY.md` |
| Calibración robusta | `models/debris-flow/CALIBRATION.md` |
| Benchmark metaheurísticas | `models/debris-flow/BENCHMARK_OPTIM.md`, `validation/calibration_benchmark.py` |
| Python | `crates/swarm-py/` (3 modelos + barridos) |
| WASM | `crates/swarm-wasm/` (3 modelos en canvas) |

## Features para paper-match

- **Topic**: framework/motor de modelado basado en agentes espacial; software científico.
- **Methodology**: ingeniería de software (Rust), HPC, validación estadística distribucional,
  metaheurísticas, aplicación geocientífica (flujos de detritos).
- **Article type**: research article / software paper (framework genérico).
- **Claims**: generales (motor de propósito general) + un caso ambiental real.
- **Evidence**: benchmarks, paridad contra plataforma de referencia (Mesa), case study.
- **Open data/code**: sí, MIT/Apache-2.0, GitHub, CI, determinista.
- **Cross-disciplinary span**: bridge (ciencias de la computación ↔ modelado ambiental/geociencias).
