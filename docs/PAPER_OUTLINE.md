# Software paper — swarm-abm — esqueleto de trabajo

> Borrador de estructura. Venue objetivo: **Environmental Modelling & Software (EMS)**.
> Estado: esqueleto con claims por sección; los números se anclan a la evidencia del repo (ver inventario).

## Venue: EMS primero, JASSS de respaldo

**EMS (recomendado).** JIF 2025 = 5.2 (Q1 Water Resources, Q2 CS Interdisciplinary / Env Eng).
Journal oficial de la iEMSs. Encaje directo con el scope:
- Lista **multi-agent systems** como técnica de AI privilegiada.
- Lista **sensitivity/uncertainty assessment** entre las áreas metodológicas → Sobol/Morris nativo es diana.
- Exige **V&V con resultados cuantitativos**, **development/maintenance**, **licensing/open source claro** → lo tenemos.
- Privilegia **frameworks genéricos** que revelen **insights generalizables de interés para quienes estudian OTROS sistemas**.
- Los dos casos demostrados son ambientales (aluviones/hazard; depredación de ganado/ecología).

**El filtro clave de EMS**: no basta "motor más rápido". El reviewer va a exigir un *insight generalizable*.
→ Ese es el eje del paper (ver Tesis). NetLogo/Mesa también son rápidos-de-prototipar; hay que argumentar
qué **cambia en la ciencia**, no solo en el reloj.

**⚠️ Refuerzo inesperado del inventario (integridad + tesis a la vez)**: existe un benchmark vs **krABMaga**
(otro motor ABM en Rust) en `validation/data/krabmaga_*.csv` donde **swarm-abm PIERDE en velocidad bruta**
(krABMaga ~1.1–2× más rápido en SIR, ~2× en Schelling). NO está escrito en ningún .md. Esconderlo sería
selección de datos que un reviewer detectaría. Pero mostrarlo **fortalece la tesis**: si el wedge fuera
"el más rápido", krABMaga lo mata; como el wedge es **determinismo bit-a-bit + experimentación integrada
(SA/calibración)** —que krABMaga no reclama— el benchmark donde perdemos en reloj es justo la evidencia de
que competimos en otro eje. Hay que escribirlo y posicionarlo (ya está el argumento en `docs/SOTA.md`).

**JASSS (respaldo).** Social simulation — encaje más débil porque los casos son ambientales, no sociales.
Solo si se quiere empujar el ángulo "motor genérico de sociedades artificiales" con un caso social nuevo.

Otros posibles (no primarios): SoftwareX / JOSS (software puro, menos "insight"), GMD (geociencia),
Journal of Open Source Software (nota corta, no da la narrativa de insight).

## Tesis (el insight generalizable que EMS exige)

**La reproducibilidad bit-exacta como propiedad de ingeniería — garantizada por el compilador y por CI,
no por disciplina — no es un detalle técnico: baja el costo de cómputo lo suficiente como para que
experimentos antes inviables (calibración robusta multi-semilla, análisis de sensibilidad global) se
vuelvan rutina, y esos experimentos CAMBIAN las conclusiones científicas.**

Dos demostraciones de que el presupuesto de cómputo cambia la ciencia (no solo el reloj):
1. **debris-flow**: la calibración a semilla única sobreajusta (un parámetro colapsa 1.81→0.02); solo se
   detecta porque el objetivo multi-semilla se volvió costeable (~400× vs Python). IoU 0.074→0.158.
2. **SIGRID**: el análisis de sensibilidad de Sobol —inviable sobre el modelo Mesa original— corre nativo;
   y el fix de *common random numbers* del estimador (que solo importa cuando corres el GSA de verdad)
   elimina un artefacto (ST>1 por ruido) y afina el ranking. El A/B controlado lo aísla.

Corolario metodológico transversal (interés para OTROS sistemas): **el determinismo del motor es lo que
hace auditable el experimento** — un GSA cuyo resultado depende de la secuencia pseudoaleatoria no es
reproducible; con CRN + semilla derivada, el índice de sensibilidad es una función determinista del diseño.

## Estructura (research article EMS)

### 1. Introduction
- ABM espacial: por qué (emergencia desde reglas individuales); dominios ambientales.
- El campo: NetLogo (JVM), Mesa (Python) — prototipado excelente, 2 órdenes de magnitud lentos para
  calibración/SA/ensembles; reproducibilidad exacta débil (RNG global, semillas no fijadas).
- El gap: falta el motor genérico que combine (a) rendimiento nativo, (b) reproducibilidad como contrato,
  (c) experimentación (SA/calibración) integrada. [anclar en SOTA.md]
- Contribución declarada (4-5 bullets) + la tesis del insight.

### 2. Software description (el corazón "software" que EMS pide)
- 2.1 Arquitectura: traits Agent/Model, patrón take-out, arena generacional. [sin unsafe]
- 2.2 Tres espacios bajo la misma abstracción: grilla, grafo, continuo (spatial hashing).
- 2.3 Cuatro esquemas de activación (ordered/random/simultaneous/staged).
- 2.4 Determinismo: ChaCha8 + child_rng(semilla,paso,id); paralelo == secuencial bit a bit;
      cross-platform x86/wasm. [REPRODUCIBILITY.md]
- 2.5 Experimentación integrada: batch (ensemble/sweep), experiment (Sobol/Morris/LHS + bootstrap).
- 2.6 Ecosistema: crates.io, PyO3, WASM; CI (golden, wasm, MSRV, bindings); licencia MIT/Apache-2.
- 2.7 Ingeniería de calidad: 3 auditorías, golden tests, política de estabilidad. [AUDIT.md, honesto]

### 3. Verification & Validation (EMS lo exige cuantitativo)
- 3.1 Determinismo verificado: golden values, PractRand, identidad x86/wasm. [RNG_AND_DETERMINISM.md]
- 3.2 Paridad numérica vs Mesa: 50 réplicas, test-z α=0.05, 7/7 métricas |z|≤1.22 (3 Schelling + 4 SIR),
      curvas de ensamble max|Δ|≤0.021. [REPORT.md] (OJO: son 2 modelos, no 3; difusión = analítica).
- 3.3 Validación analítica: difusión al punto fijo; Ishigami para el GSA (índices de forma cerrada).
- 3.4 Rendimiento — HONESTO en dos direcciones:
      * vs intérpretes: SIR 48–59× / Schelling 45–184× sobre Mesa (el 184× admitido como artefacto del
        move_agent O(n) de Mesa); 2.3–5.2× sobre Agents.jl. [CROSS_ENGINE.md, i7-1270P, 1 hilo, mediana 3 semillas]
      * vs otro Rust: krABMaga es ~1.1–2× MÁS rápido (SIR ratio 0.48–0.87; Schelling 0.39–0.50).
        [validation/data/krabmaga_*.csv — SIN escribir aún] → escribir y posicionar: no competimos en reloj.
      * criterion motor puro: 38M ag-pasos/s (10k), 12M/s (1M), Life 37M celdas/s, SIR 50² en 7 ms
        [solo en CLAUDE.md:63 — GAP: falta doc citable].
      * escalabilidad paralela (Life 500², 16 hilos): compute-bound 5.27×, memory-bound tope 1.63×
        (ancho de banda), Amdahl (apply secuencial). [SCALABILITY.md]

### 4. Illustrative applications (los dos casos = la demostración de la tesis)
- 4.1 Caso 1 — flujos de detritos (Atacama 2015): port fiel (IoU Python 0.4653 vs Rust 0.4684, Δ<1%;
      grilla 5871×5422 @30m ≈31.8M celdas), speedup ~100× (128–239 s → 1.2–4.1 s); calibración DE
      (15 params) robusta 2016 sims ~5 min vs ~34 h Python (~400×); sobreajuste a semilla detectado
      (T 1.81→0.02), IoU 8 semillas 0.074→0.158 (~2.1×); benchmark 5 metaheurísticas 7500 sims ~12 min
      (vs ~80 h), Friedman χ²=14.32 p=0.0063, GWO gana (Wilcoxon-Holm); física dirigida por diagnóstico
      IoU 0.468→0.555 (+19%, recall 0.59→0.79, precision 0.825). [debris-flow/PARITY,CALIBRATION,BENCHMARK_OPTIM,PHYSICS_EXPERIMENT.md]
      → Insight: HPC habilita la calibración robusta multi-semilla que revela el sobreajuste — inaccesible a Python.
- 4.2 Caso 2 — depredación de ganado, Isla Riesco (SIGRID): port, paridad distribucional (Pearson 0.966,
      Spearman 0.902, RMSE 10.1pp; re-validada v0.4 estable), speedup ~53× (14d) a ~100–116× (30d);
      GSA Sobol N=512/30d = 7168 evals, INVIABLE en Mesa (~2728 core-horas); nativo señala n_dogs dominante
      (ST≈0.90, único con S1 real ~0.79); el A/B del fix de CRN (ST 1.021 artefacto>1 → 0.902 sano;
      Δsum(ST) del fix = −0.23, aislado del fix del modelo). [sigrid/PARITY.md]
      → Insight: el SA riguroso (inviable en Mesa) señala la palanca de manejo (n_dogs); el determinismo +
        CRN lo hace auditable — un GSA no determinista no sería reproducible. La MEJOR demostración de la tesis.
      → CAVEAT honesto: el ranking ecológico secundario es provisional (residual de 2 perros abierto);
        la *habilitación HPC* es firme, la conclusión ecológica fina no.

### 5. Impact / Discussion
- Qué desbloquea para el usuario ambiental: SA/calibración de rutina, reproducibilidad citable.
- Comparación honesta con el campo (tabla de features vs NetLogo/Mesa/Agents.jl/Repast) — [GAP: construir].
- Limitaciones de alcance (single-node, GIL en run(), CSV sin Parquet, AoS, visor demo) — honestas.

### 6. Conclusions

### Metadata EMS (secciones requeridas de software)
- Code metadata table (versión, licencia MIT/Apache, repo, lenguaje, DOI de Zenodo — [GAP: /zenodo]).
- Required Metadata / Current code version.
- CRediT, data availability, AI use statement.

## GAPS a cerrar antes de submit — priorizados por el inventario

### Bloqueantes de integridad (resolver SÍ o SÍ antes de escribir prosa)
1. **[CERRADO 2026-07-11] krABMaga**: escrito `validation/KRABMAGA.md` (krABMaga 1.15–2.1× más rápido en SIR,
   2.0–2.6× en Schelling; reframe en determinismo+experimentación). Corregida la afirmación falsa
   "consistentemente el más rápido" en `CROSS_ENGINE.md` + referencia cruzada. Falta: llevarlo a la prosa
   de la sección 3.4/5 del paper y una figura de la comparación.
2. **[CRÍTICO] Publicar 0.4.0 + DOI de Zenodo**: los fixes de determinismo (CRN de Sobol/Morris, F2) están
   en 0.4.0 SIN publicar; 0.3.0 (con F1–F6) es lo único en crates.io. El paper debe citar la versión exacta
   usada, idealmente con DOI (skill `/zenodo`; falta `.zenodo.json`/`CITATION.cff`). Encadena con los yanks.

### Debilidades de related-work / benchmarking (fortalecen, no bloquean)
3. **NetLogo ausente**: es el motor de referencia del campo; su ausencia debilita el related-work. Requiere JVM.
4. **Tabla de features SOTA sin verificar**: `docs/SOTA.md` tiene la matriz pero solo 5 celdas verificadas en
   vivo; filas Mesa/NetLogo/Repast marcadas "borrador cutoff enero 2026". Verificar antes de publicarla.
5. **Cross-engine limitado**: 1 máquina, 1 hilo, mediana de 3 semillas; el paralelismo intra-paso NO se mide
   vs competidores (solo vs sí mismo en SCALABILITY.md).
6. **Criterion sin doc citable**: los 38M/12M/37M ag-pasos/s viven solo en CLAUDE.md, sin entorno/metodología.
7. **GAMA head-to-head ausente**: SOTA posiciona a GAMA como el único competidor con SA/calibración nativas,
   pero no hay experimento contra él — el wedge "experimentos deterministas" se argumenta, no se mide vs el rival real.

### Reproducibilidad del artefacto
8. **Ejemplo end-to-end congelado**: falta un artefacto reproducible (DEM + ground truth + semillas archivados)
   que regenere una figura del paper; debris-flow depende de datos externos no versionados (PARITY nota que ni
   el Python original reproduce métricas históricas). Candidato a Zenodo.
9. **Visor WASM sin documentar**: existe (~68 KB, canvas) pero sin screenshots/demo desplegada/uso — para un
   software paper un demo interactivo pesa.

### Menores / calidad de evidencia
10. **Paridad vs Mesa solo 2 modelos** (Schelling, SIR); un tercero formal reforzaría "paridad del motor".
11. **Residual SIGRID de 2 perros**: cerrar la re-validación Rust-vs-Mesa-actual (~40 min) para firmar el
    ranking ecológico, o declararlo explícitamente provisional en el paper.

### Producción
12. **Figuras** (candidatas): arquitectura del motor; los 3 espacios; curvas de paridad de ensamble;
    escalabilidad par (compute vs memory-bound); los 2 casos (footprint debris-flow, Sobol de SIGRID con el A/B).
    → Usar R/ggplot2 por default (preferencia declarada del usuario para figuras de publicación).
13. **Zenodo/CITATION.cff/`.zenodo.json`** (encadena con el gap 2).
