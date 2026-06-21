# Escalabilidad — `decide` paralelo intra-paso

La activación **simultánea** de swarm-abm corre la fase `decide` en paralelo
(rayon), con un resultado **bit-idéntico** al secuencial. Este documento mide
cuánto escala y bajo qué condiciones.

## Por qué es seguro (y por qué importa para el paper)

En activación simultánea, `decide` recibe el modelo **inmutable** (`&Model`): el
compilador *prueba* que ningún agente puede escribir estado compartido durante
la fase. Sumado a un **RNG por-agente** derivado de `(semilla, paso, id)` —no del
hilo—, el motor reparte `decide` entre hilos sin carreras y sin perder
reproducibilidad. La inmutabilidad garantizada por el tipo es, literalmente, lo
que habilita el paralelismo. Verificado en `tests/parallel_decide.rs`
(paralelo == secuencial, bit a bit).

## Banco: Juego de la Vida (`examples/life`)

Modelo simultáneo canónico (cada celda es un agente; `decide` lee las 8 vecinas,
`apply` materializa). Grilla 500×500 (250 000 agentes), 30 pasos, mínimo de 3
réplicas. Máquina de 16 hilos. Un parámetro `--work` añade costo de decisión por
agente, para separar dos regímenes:

- **`--work 0`** — Vida pura: `decide` solo lee la grilla → **memory-bound**.
- **`--work 50`** — decisión cara por agente (utilidad/percepción/optimización
  local, lo común en ABM económico/social) → **compute-bound**.

## Resultados (speedup vs secuencial)

| Hilos | compute-bound (work=50) | memory-bound (Vida pura) |
|---|---|---|
| 1 (paralelo) | 1.01× | 0.71× *(overhead de rayon)* |
| 2 | 1.68× | 1.08× |
| 4 | 2.57× | 1.27× |
| 8 | 3.89× | 1.48× |
| 16 | **5.27×** | **1.63×** |

## Lectura (honesta)

- **Cuando la decisión por agente es compute-bound, el `decide` paralelo escala
  fuerte: ~5.3× a 16 hilos.** El techo lo pone la **ley de Amdahl**: la fase
  `apply` sigue siendo secuencial (materializa y resuelve colisiones en orden).
- **Cuando es memory-bound (stencil tipo Vida pura), el beneficio es modesto
  (~1.6×)**: el cuello de botella es el ancho de banda de memoria, no los
  núcleos — un resultado esperado y bien conocido para cómputos de tipo stencil.
  El paralelismo no inventa ancho de banda.
- Combinado con la ventaja single-thread (~2–5× sobre Agents.jl, ver
  `CROSS_ENGINE.md`), el `decide` paralelo **amplía la brecha** en modelos con
  decisiones caras: el régimen donde el paralelismo intra-paso paga.

## Trabajo futuro

La fase `apply` también es, en muchos modelos, paralelizable (escrituras a
celdas disjuntas). Paralelizarla subiría el techo de Amdahl. Queda fuera del
alcance actual (requiere una API de `apply` con acceso disjunto garantizado).

## Reproducir

```bash
cargo build --release -p life
# compute-bound, 16 hilos
RAYON_NUM_THREADS=16 target/release/life --bench --parallel \
  --width 500 --height 500 --steps 30 --work 50
# secuencial de referencia
target/release/life --bench --width 500 --height 500 --steps 30 --work 50
```

Datos en `validation/data/scaling.csv` y `scaling_summary.csv`.
