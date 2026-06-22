# Plan de refuerzo — review en frío SIMPAT (ronda 2)

> Origen: `/paper-review-simpat blind` del 2026-06-22 sobre el manuscrito ya
> escrito (paper/tex/main.tex, 17 pp). Veredicto simulado: **Major Revision**
> (riesgo de Reject con handling editor de PADS). Complementa
> `REVIEW_FIXES.md` (ronda 1, tex-review interno). **No enviar hasta ESPL.**

## Tabla maestra

| # | Sev | Issue | Acción | Tipo | Esfuerzo | Estado |
|---|-----|-------|--------|------|----------|--------|
| C1 | 🔴 | Sin contexto de la literatura PADS sobre determinismo/reproducibilidad en simulación paralela | Buscar refs PADS + párrafo de related work + re-acotar el claim de novedad | escritura+lit | M | ☐ |
| C2 | 🔴 | Historia HPC débil; krABMaga más rápido + distribuido | (a) defender el trade-off determinismo-vs-velocidad en §8 + crisis de reproducibilidad; (b) opcional: paralelizar fase `apply` | escritura / ingeniería | S / L | ☐ |
| C3 | 🟠 | "Compiler-verified" se lee como feature de Rust, no aporte de M&S general | Párrafo: el principio (inmutabilidad de la fase decide por tipos) generaliza más allá de Rust | escritura | S | ☐ |
| S1 | 🟡 | La garantía es más angosta de lo enunciado (el snapshot lo construye el autor) | Enunciar el límite explícito en §3 | escritura | S | ☐ |
| S2 | 🟡 | Benchmarks con solo 3 réplicas, sin IC (vs 50 en la paridad) | Re-correr con N réplicas + reportar IC/IQR | experimento | M | ☐ |
| S3 | 🟡 | Solo 2 modelos de grilla; grafo/continuo sin medir | Medir throughput de swarm-abm en grafo (network-sir) y continuo (boids) | experimento | M–L | ☐ |
| S4 | 🟡 | Frontera con el manuscrito ESPL poco clara | Frase: qué es nuevo aquí vs el manuscrito debris-flow | escritura | S | ☐ |
| S5 | 🟡 | Sin enunciado formal de la propiedad | Proposición etiquetada con precondición (contrato snapshot) y alcance | escritura | S | ☐ |
| Z | 🟢 | Depósito archivado (Zenodo DOI) — también en R1 #12 | Acuñar DOI (post-ESPL) | admin | S | ☐ |

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
