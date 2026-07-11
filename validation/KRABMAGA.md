# Benchmark vs krABMaga — el competidor de rendimiento en Rust

krABMaga es el único motor que comparte el stack de swarm-abm (Rust, paralelo
intra-modelo, WASM, exploración de modelos) y, por lo tanto, el único comparador
de rendimiento honesto que no está limitado por un runtime interpretado (Mesa)
ni por JIT (Agents.jl). Este documento reporta ese benchmark **incluyendo el
resultado donde swarm-abm pierde** — es lo que fija correctamente el eje en el
que compite el motor.

> **Resultado en una línea: krABMaga es más rápido.** ~1.15–2.1× en SIR y
> ~2.0–2.6× en Schelling. swarm-abm **no** compite por ser el ABM más rápido en
> Rust; compite por ser **determinista bit a bit por construcción** — una
> garantía que krABMaga no reclama (ver "Lectura").

## Motores

| Motor | Versión | Runtime |
|---|---|---|
| **swarm-abm** | 0.3 (este repo, misma corrida que `CROSS_ENGINE.md`) | Rust 1.94, release (LTO) |
| **krABMaga** | 0.6 | Rust 1.94, release (LTO, opt-level 3) |

## Modelo y metodología

Idénticos a `CROSS_ENGINE.md` (mismo SIR y mismo Schelling, misma máquina
i7-1270P, un solo hilo, mediana de 3 semillas, solo se cronometra el stepping;
build fuera del cronómetro). Las columnas `swarm_mspp` de los CSV
(`validation/data/krabmaga_*.csv`) coinciden byte a byte con los números de
swarm-abm de `CROSS_ENGINE.md`: es la misma corrida, directamente comparable.

**La implementación de krABMaga es su mejor versión, a propósito** (para que el
resultado no se pueda descartar con "usaste el field equivocado"): SIR sobre
`DenseNumberGrid2D<i32>` (estado escalar por celda, `get_value` sin alocación) y
un **único agente `Updater`** que recorre la grilla con actualización sincrónica
vía `lazy_update` — el patrón idiomático eficiente de krABMaga, no el object-grid
que aloca un `Vec` por consulta. Ver `validation/krabmaga-bench/`.

## Resultados — SIR (ms/paso, mediana de 3 semillas)

| Grilla | Agentes | swarm-abm | krABMaga | krABMaga más rápido |
|---|---|---|---|---|
| 25×25 | 625 | 0.0162 | 0.0078 | 2.08× |
| 50×50 | 2 500 | 0.0673 | 0.0485 | 1.39× |
| 100×100 | 10 000 | 0.2459 | 0.2137 | 1.15× |
| 200×200 | 40 000 | 1.3387 | 1.0443 | 1.28× |

krABMaga **1.15–2.08× más rápido**. La ventaja se estrecha al crecer la grilla
(2.08× → 1.15×): a escala chica domina el overhead por-agente de swarm-abm; a
escala grande ambos quedan acotados por el mismo recorrido de la grilla.

## Resultados — Schelling (ms/paso, mediana de 3 semillas, 100 pasos fijos)

| Grilla | swarm-abm | krABMaga | krABMaga más rápido |
|---|---|---|---|
| 25×25 | 0.0516 | 0.0201 | 2.57× |
| 50×50 | 0.1759 | 0.0827 | 2.13× |
| 100×100 | 0.8385 | 0.3896 | 2.15× |
| 200×200 | 3.3883 | 1.6941 | 2.00× |

krABMaga **2.0–2.6× más rápido**, ventaja estable a través de las escalas.

## Lectura — por qué esto no mueve la tesis del motor

1. **swarm-abm no compite en velocidad bruta, y este benchmark lo confirma.**
   Frente a los intérpretes swarm-abm gana con holgura (45–184× sobre Mesa,
   2.3–5.2× sobre Agents.jl; ver `CROSS_ENGINE.md`), pero frente a otro motor
   nativo en Rust la velocidad deja de ser el diferenciador — como debe ser: dos
   implementaciones idiomáticas en Rust release convergen al mismo orden de
   magnitud, y las diferencias que quedan son de abstracción (el `DenseNumberGrid`
   con un único sweeper de krABMaga es esencialmente un autómata celular; el
   modelo por-agente de swarm-abm paga el costo de la generalidad que le permite
   demografía, heterogeneidad y tres espacios bajo la misma API).

2. **El eje real es el determinismo, y ahí krABMaga no compite** (no porque sea
   peor, sino porque no lo reclama). Verificado sobre fuentes primarias
   (`docs/SOTA.md`, 2026-07-03): el paper de krABMaga (Antelmi et al., *JASSS*
   2024) vende **memory-safety** como su garantía de fiabilidad ("memory flaws
   which could invalidate the experiment results"), **no** reproducibilidad bit a
   bit; una búsqueda full-text de "deterministic/reproducible" da 0 resultados, y
   su propio feature `parallel` está en estado **Experimental**. swarm-abm hace lo
   contrario: ejecución paralela bit-idéntica a la secuencial e identidad
   cross-platform x86-64 ↔ wasm32, verificadas y con gate de CI
   (`RNG_AND_DETERMINISM.md`, `REPRODUCIBILITY.md`).

3. **Para el argumento del paper, esto es evidencia a favor, no en contra.** La
   tesis es que la *reproducibilidad por construcción* es lo que hace auditables
   los experimentos (calibración robusta, análisis de sensibilidad global) —
   demostrado en los casos de detritos y SIGRID. Un motor 1.5× más rápido pero
   cuyo GSA no es reproducible no resuelve el problema que swarm-abm resuelve. La
   velocidad de krABMaga es real y se reporta; no toca el wedge.

## Caveats (honestos)

- Un solo hilo. El paralelismo intra-paso de swarm-abm (fase `decide`, ~5× a 16
  hilos, `SCALABILITY.md`) **preserva el determinismo**; el de krABMaga es
  Experimental y no reclama reproducibilidad — una comparación paralela mediría
  ejes distintos y queda pendiente como trabajo separado.
- El sweeper único sobre `DenseNumberGrid` de krABMaga es la versión rápida para
  estos dos modelos concretos; no todos los modelos se expresan así (un modelo
  con agentes móviles heterogéneos y demografía no se reduce a un barrido escalar
  de la grilla), pero para SIR/Schelling es su mejor caso y se le concede.
- Una sola máquina, mediana de 3 semillas — mismas limitaciones que
  `CROSS_ENGINE.md`.

## Reproducir

```
# swarm-abm: mismos binarios que CROSS_ENGINE.md (bench-sir / bench-schelling)
# krABMaga:
cd validation/krabmaga-bench
cargo run --release --bin main -- <grid>       # SIR
cargo run --release --bin schelling -- <grid>  # Schelling
# ratios en validation/data/krabmaga_{sir,schelling}.csv
```
