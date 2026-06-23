# Plan de refuerzo — review en frío SIMPAT (ronda 2)

> Origen: `/paper-review-simpat blind` del 2026-06-22 sobre el manuscrito ya
> escrito (paper/tex/main.tex, 17 pp). Veredicto simulado: **Major Revision**
> (riesgo de Reject con handling editor de PADS). Complementa
> `REVIEW_FIXES.md` (ronda 1, tex-review interno). **No enviar hasta ESPL.**

## Tabla maestra

| # | Sev | Issue | Acción | Tipo | Esfuerzo | Estado |
|---|-----|-------|--------|------|----------|--------|
| C1 | 🔴 | Sin contexto de la literatura PADS sobre determinismo/reproducibilidad en simulación paralela | §3.3: situado como *repeatability* de PADS (Fujimoto 1990/2000), novedad re-acotada al MECANISMO (por tipos, no por protocolo) | escritura+lit | M | ☑ |
| C2 | 🔴 | Historia HPC débil; krABMaga más rápido + distribuido | (a) trade-off defendido; (b) `apply` paralelo EXPLORADO y descartado: conflicto de semántica (apply paralelizable = solo-self → pierde las escrituras inmediatas que dan sentido a la activación síncrona; y apply es fracción chica en compute-bound). Paper §5.3/§8 corregidos al hallazgo | escritura / ingeniería | S / L | ☑ (a hecho; b explorado→no) |
| C3 | 🟠 | "Compiler-verified" se lee como feature de Rust, no aporte de M&S general | Discusión: principio agnóstico al lenguaje (const-correctness/ownership/capabilities); "the contribution is the principle, not the language" | escritura | S | ☑ |
| S1 | 🟡 | La garantía es más angosta de lo enunciado (el snapshot lo construye el autor) | §3.2: scope explícito — el compilador impide escribir el modelo, no certifica el snapshot | escritura | S | ☑ |
| S2 | 🟡 | Benchmarks con solo 3 réplicas, sin IC (vs 50 en la paridad) | n=10 seeds; speedup por-seed (load-independent) mediana 4.8x IQR 4.5-5.3x; reportado en §5.1 | experimento | M | ☑ |
| S3 | 🟡 | Solo 2 modelos de grilla; grafo/continuo sin medir | --bench en network-sir/boids; throughput grafo ~2e7, continuo ~4e5 agent-steps/s; §5 nueva subsección + §8 limitación corregida | experimento | M–L | ☑ |
| S4 | 🟡 | Frontera con el manuscrito ESPL poco clara | §6: delimitado — el motor es de este paper; el modelo/calibración debris-flow son del manuscrito ESPL, no re-reclamados | escritura | S | ☑ |
| S5 | 🟡 | Sin enunciado formal de la propiedad | §3.3: Property (deterministic parallelism) etiquetada con precondición del snapshot y alcance | escritura | S | ☑ |
| Z | 🟢 | Depósito archivado (Zenodo DOI) — también en R1 #12 | Acuñar DOI (post-ESPL) | admin | S | ☐ |

> **Estado (2026-06-22):** TODO el review en frío abordado salvo Z (Zenodo,
> post-ESPL). C1/C2a/C3/S1/S4/S5 (escritura) + C2b (explorado→descartado con
> fundamento) + S2 (n=10, IQR del speedup) + S3 (throughput 3 paradigmas).
> Paper 19 pp, compila, workspace CI verde. Pendiente solo Z + placeholders.

## 🔴 Detalle de los deciders accept/reject

### C1 — Situar en la literatura PADS (el más importante)
**Por qué:** el determinismo/repeatability en simulación paralela y distribuida
(PADS) tiene décadas de literatura (deterministic replay, repeatability,
synchronisation conservadora/optimista). El EIC de SIMPAT (Furfaro) trabaja en
PADS. Sin este contexto, el claim "a combination the reference tools do not
provide" se sostiene contra una baseline demasiado estrecha.
**Acción:**
- [ ] Identificar refs núcleo PADS: Fujimoto (PDES, CACM 1990; libro PADS 2000),
  literatura de repeatability/deterministic replay en parallel simulation,
  reproducibilidad en ABM (más allá de ODD).
- [ ] Añadir subsección/párrafo "Related work" o ampliar §8: situar la
  bit-identidad-entre-hilos como una forma de *repeatability* de PADS, y
  distinguir el aporte (garantía por tipos a nivel de lenguaje, no por protocolo
  de sincronización).
- [ ] Re-acotar el claim de novedad en intro/§3 contra esa baseline.
- [ ] Verificar refs con `/verify-refs`.

### C2 — Defender el trade-off determinismo vs HPC
**Por qué:** el benchmark (honesto) muestra que krABMaga es más rápido y tiene
MPI distribuido; el techo de Amdahl limita el paralelismo a ~5×. El caso de
ingeniería queda apoyado solo en el determinismo → hay que defenderlo más fuerte.
**Acción mínima (a):**
- [ ] §8/Discusión: argumentar explícito por qué la reproducibilidad bit a bit
  justifica ser single-node y algo más lento — citar el costo científico de
  resultados de ABM irreproducibles (la "crisis de reproducibilidad"
  computacional). El determinismo no es gratis pero es un *requisito*, no un lujo.
**Acción fuerte (b), opcional pero contundente:**
- [ ] Paralelizar la fase `apply` bajo una garantía de disjointness (ya marcada
  como future work). Subiría el techo de Amdahl y respondería C2 **con datos**.
  Es cambio de motor real (esfuerzo L). Decisión del usuario.

## 🟠/🟡 Detalle del resto (mayormente escritura)

- **C3** §3 o §8: una frase/párrafo — el principio es "una región de solo-lectura
  garantizada para la fase de decisión"; en Rust se realiza con el borrow
  checker, pero la idea aplica a cualquier sistema de tipos/ownership/capabilities
  que pueda imponer inmutabilidad. Evita que se lea como "usamos Rust".
- **S1** §3: enunciar que el compilador impide *mutar el modelo* en `decide`,
  pero la completitud/corrección semántica del *snapshot* (lo que el autor expone
  para lecturas cross-agente) sigue siendo responsabilidad del autor. La garantía
  es "no escritura accidental del modelo", no "update síncrono correcto" en
  abstracto.
- **S2** Re-correr cross-engine + escalabilidad con N≥10 réplicas; reportar
  mediana + IC/IQR. Para krABMaga (no sembrable) reportar el spread de N corridas.
  Actualizar figuras.
- **S3** Añadir throughput de swarm-abm en **grafo** (network-sir) y **continuo**
  (boids) — aunque sea sin comparador, como evidencia de que la generalidad
  espacial no es solo nominal. Si hay comparador idiomático fácil (Agents.jl tiene
  GraphSpace/ContinuousSpace), mejor.
- **S4** §6: una frase delimitando — el motor + la habilitación de
  performance/estudios son aporte de *este* paper; el modelo de debris-flow y sus
  resultados de calibración son del manuscrito en revisión (citado); el benchmark
  de metaheurísticas no se reclama como contribución nueva de este paper.
- **S5** §3: enunciar la propiedad como proposición etiquetada —
  *Proposition: for any model whose `decide` reads only the environment and a
  published snapshot (enforced: the model is immutable in `decide`), and any
  thread count, the parallel run equals the sequential run bitwise.* Con su
  precondición y alcance.
- **Z** Acuñar DOI Zenodo (post-ESPL) — ver `/zenodo`.

## Secuencia recomendada

1. **Bloque de escritura** (S, una pasada): C2(a), C3, S1, S4, S5 + re-acotar
   claims. Recompilar. ~1 sesión.
2. **C1 (PADS)**: lit search + párrafo + verify-refs. ~1 sesión. *El decider.*
3. **Experimentos** (si se decide reforzar más): S2 (réplicas+IC), S3 (grafo/
   continuo), y la opción fuerte C2(b) paralelizar `apply`. Decisión del usuario.
4. **Z + R1 #12** placeholders: al cierre, antes del paquete de submission.
5. Bloqueado por ESPL: envío + DOI.

## Notas
- C1 y C2 deciden accept-vs-reject con un handling editor de PADS; priorizar.
- La mayoría son escritura/posicionamiento; solo S2/S3/C2(b) son experimentos.
- El paper ya es honesto y fuerte; esta ronda es para subirlo del Major-Revision
  al borde del accept en un venue del 7%.
