# Plan de correcciones del manuscrito (SIMPAT)

> Origen: `/tex-review` del 2026-06-22 sobre `paper/tex/main.tex` (7 dimensiones)
> + hallazgo de related-work (krABMaga). Documento vivo: marcar estado conforme
> se resuelve. **No enviar hasta decisión ESPL.**

## Tabla maestra

| # | Dim | Sev | Issue | Acción | Esfuerzo | Estado |
|---|-----|-----|-------|--------|----------|--------|
| 1 | D7 | 🔴 | Falta krABMaga (competidor Rust directo) | Citar + posicionar (y ver benchmark) | M–L | ☐ |
| 2 | D3 | 🔴 | "Parity" desde test no significativo (falacia de equivalencia) | Reencuadrado: lidera el bound de efecto; el z-test es "no detectable difference" | S | ☑ |
| 3 | D4 | 🔴 | "By construction" mezcla garantizado (hilos) vs empírico (plataformas) | §4.3: cross-platform marcado como empírico, distinto del caso hilos | S | ☑ |
| 4 | D1 | 🟡 | "the fastest compiled ABM framework" (superlativo) | → "one of the fastest compiled ABM frameworks" (3×) | S | ☑ |
| 5 | D2/D4 | 🟡 | Speedup generaliza desde n=2 modelos de grilla | Limitación añadida (§8): solo grilla benchmarkeada | S | ☑ |
| 6 | D2 | 🟡 | "for any model and thread count" universal, 1 modelo testeado | §3.3: "any model under the parallel API" + verificado empírico | S | ☑ |
| 7 | D4 | 🟡 | CPU híbrida (4P+8E) no revelada | §5.1 + §5.3: declarada + nota sobre la curva | S | ☑ |
| 8 | D4 | 🟡 | Escalabilidad usa carga sintética (`--work`) | §5.3: marcada como "synthetic proxy" | S | ☑ |
| 9 | D3 | 🟢 | "~400×" de un rango ancho (~130–2000×) | §6: → "two to three orders of magnitude" | S | ☑ |
| 10 | D4 | 🟢 | "GWO leads out-of-sample" sin mostrarlo | §6: → "ranks first and performs competitively out of sample" | S | ☑ |
| 11 | D5 | 🟢 | No preempta "double-buffering ya resuelve esto" | §3.2: frase directa añadida | S | ☑ |
| 12 | D6 | 🟢 | Placeholders: autor/afiliación, DOI Zenodo, decl. IA | Llenar al cierre | S | ☐ |

> **Estado (2026-06-22):** bloque de edición #2–#11 APLICADO (compila, 16 pp).
> Pendiente: #1 (krABMaga) y #12 (placeholders).

Esfuerzo: S = minutos (edición), M = horas, L = día+ (experimento/código).

---

## 🔴 Detalle de los ALTO

### #1 — krABMaga: el competidor que falta

**Qué es:** framework de ABM en Rust (ISISLab, U. Salerno; Spagnuolo, Cordasco,
Antelmi et al.). Re-ingeniería de MASON. Paralelización del scheduling de
agentes intra-step, MPI distribuido, visualización Bevy + WASM, model
exploration en clusters. Papers: AsiaSim 2019 y **JASSS 2024**.

**Por qué es crítico:** mismo lenguaje, features solapadas (Rust rápido,
paralelo intra-step, WASM). Es un comparador **más relevante que Agents.jl**.
Omitirlo lee como blind spot y debilita el claim de novedad.

**Posicionamiento (honesto):** el diferenciador NO es "Rust+paralelo+WASM"
(krABMaga ya lo tiene), sino **determinismo-por-construcción**:
- update síncrono **verificado por el compilador** (krABMaga no lo afirma);
- paralelismo **bit-idéntico** parallel==sequential (krABMaga paraleliza el
  scheduling pero **no garantiza** reproducibilidad del resultado paralelo);
- bit-identidad **cross-platform** (x86/wasm).

**Acciones:**
- [ ] **(mínimo)** Citar krABMaga (AsiaSim 2019 + JASSS 2024) en Introducción
  (related work) y en §8 (Relation to other engines). Posicionar por la tabla
  de arriba: solapamiento real + el wedge determinista.
- [ ] **(recomendado, stretch)** Benchmark head-to-head krABMaga vs swarm-abm
  en SIR/Schelling (mismo protocolo del §5). Es el comparador Rust directo; si
  ganamos o empatamos, refuerza; si perdemos en velocidad, el wedge determinista
  sigue de pie. Requiere instalar krABMaga, aprender su API, escribir espejos.
- [ ] Verificar las 2 refs con `/verify-refs` antes de insertar (autores/venue
  exactos del JASSS 2024).

### #2 — Falacia de equivalencia en la paridad vs Mesa

**Dónde:** §4.1. "All seven metrics are in parity ($|z|\le1.22$)".
**Problema:** no rechazar H₀ ≠ probar equivalencia (ausencia de evidencia ≠
evidencia de ausencia). Además 7 tests sin corrección.
**Fix:** reencuadrar el lenguaje a "no statistically detectable difference" y
**liderar con el bound de tamaño de efecto** (curvas Δ < 0.021), que ya es un
argumento de equivalencia. Opcional fuerte: correr un **TOST** con margen
pre-especificado (p. ej. ±0.05 en la métrica) y reportarlo. Recalcular es barato
(datos en `validation/`).

### #3 — "By construction" garantizado vs empírico

**Dónde:** abstract, §4.3, §8 (paraguas "determinism by construction").
**Problema:** la identidad **entre hilos** es por-construcción (RNG por-agente,
demostrable). La bit-identidad **cross-platform** (x86/wasm) es **empírica**
(probada en 3 modelos; podría romperse para usos de RNG sensibles al ancho de
palabra, p. ej. `random_range(0..usize)` con valores > 2³²).
**Fix:** distinguir explícito —
- "**guaranteed by construction**" para el caso paralelo/hilos;
- "**verified empirically across x86-64 and WebAssembly**" para plataformas.
No re-experimentar; es precisión de lenguaje (más honesto y más fuerte ante un
reviewer cuidadoso).

---

## 🟡 Detalle de los MODERADO (todos edición)

- **#4** Reemplazar "the fastest compiled ABM framework" → "a state-of-the-art
  compiled ABM framework" / "one of the fastest" (abstract, intro, §5.1).
- **#5** Acotar el speedup a los benchmarks medidos; añadir a *Limitations*:
  "performance evaluated on two grid-based models; graph and continuous-space
  paradigms are not benchmarked here". (Honestidad de alcance.)
- **#6** §3.3: "for any model **expressible under the parallel API** (which
  enforces the snapshot contract) and any thread count … verified empirically
  on a stochastic model."
- **#7** §5.1: declarar "the i7-1270P is a hybrid 4P+8E design (12 cores, 16
  threads); scaling beyond 8 threads engages the slower efficiency cores, so the
  curve mixes Amdahl and core heterogeneity."
- **#8** §5.3: aclarar que el parámetro de trabajo (`--work`) es un **proxy
  sintético** del régimen compute-bound, no un modelo real con decisión cara.

## 🟢 Detalle de los MENOR

- **#9** §6: dar el rango del speedup de calibración o marcar "approximate".
- **#10** §6: mostrar el dato out-of-sample de GWO (de `BENCHMARK_OPTIM.md`) o
  suavizar "leads out-of-sample" → "performed competitively out-of-sample".
- **#11** §3: una frase preempt: "double-buffering is the known remedy; our
  contribution is that the type system **enforces** it and thereby licenses safe
  parallelism, rather than relying on the modeller to remember it."
- **#12** Front matter: autor/afiliación; §7 + Data availability: DOI Zenodo
  (post-ESPL); Declaration of generative AI use (obligatorio Elsevier).

---

## Secuencia recomendada

1. **Bloque de edición** (todo S, una pasada al .tex): #2, #3, #4, #5, #6, #7,
   #8, #9, #10, #11. Recompilar + re-`/paper-style` rápido. ~1 sesión.
2. **krABMaga cita+posicionamiento** (#1 mínimo): WebFetch/verify-refs de las 2
   refs, escribir el párrafo de related work + actualizar §8. ~1 sesión.
3. **krABMaga benchmark** (#1 stretch): decisión del usuario — alto valor para
   SIMPAT, esfuerzo M–L. Si sí, replica el pipeline `validation/` con un crate
   espejo en krABMaga.
4. **#12 placeholders**: al cierre, antes de armar el paquete de submission.
5. Bloqueado por ESPL: envío + DOI Zenodo.

## Notas

- Ninguna corrección requiere re-correr los experimentos del motor salvo el
  TOST opcional (#2, barato) y el benchmark de krABMaga (#1 stretch).
- Los tres ALTO (#1, #2, #3) son los que la persona-metodólogo de SIMPAT caza;
  priorizarlos.
