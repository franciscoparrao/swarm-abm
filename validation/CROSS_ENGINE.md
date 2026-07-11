# Benchmark cross-engine — SIR + Schelling

swarm-abm (Rust) frente a las plataformas de ABM de referencia, en el **mismo
modelo, la misma máquina y la misma metodología**. Motivado por el requisito de
SIMPAT de comparar contra los motores de alto rendimiento reales, no solo contra
un baseline lento (Mesa).

## Motores

| Motor | Versión | Runtime |
|---|---|---|
| **swarm-abm** | 0.3 (este repo) | Rust 1.94, release (LTO) |
| **Agents.jl** | 7.0.2 | Julia 1.10.11 LTS |
| **Mesa** | 3.5.1 | CPython 3.12 |

> Agents.jl es la plataforma de ABM moderna y rápida (Julia compilado); es el
> comparador honesto de rendimiento. Mesa (Python) es el baseline de
> accesibilidad. NetLogo (JVM) y FLAME GPU / Repast HPC quedan como trabajo
> relacionado (NetLogo pendiente; GPU/MPI fuera del alcance de un solo nodo CPU).

## Modelo y metodología

- **SIR espacial idéntico**: grilla torus totalmente ocupada, vecindad Moore,
  susceptible con `k` vecinos infectados se contagia con prob `1−(1−β)^k`
  (β=0.08), infectado se recupera con prob `γ=0.1`, activación aleatoria por
  paso, término cuando no quedan infectados. 5 infectados iniciales.
- **Schelling idéntico**: grilla torus, densidad 0.85, dos grupos 50/50,
  vecindad Moore, conforme si la fracción de vecinos del mismo grupo ≥ 0.375
  (1.0 si aislado), el inconforme se muda a una celda vacía uniforme al azar,
  activación aleatoria. Bench: **100 pasos fijos sin corte por convergencia**
  (el escaneo de similitud sobre todos los agentes domina el costo por paso).
- **Métrica: ms por paso.** En SIR cada motor termina la epidemia en un paso
  distinto (RNG distinto), así que el tiempo total no es comparable y se
  normaliza por paso (mediana de 3 semillas, hasta 300 pasos). En Schelling se
  fijan 100 pasos.
- **Solo se cronometra el stepping** — la construcción del modelo queda fuera
  del cronómetro en los tres motores.
- **Julia: warmup explícito** antes de medir, para no cronometrar la
  compilación JIT (error clásico que invalida comparaciones con Julia).
- Mismo equipo (i7-1270P, 16 hilos), un solo hilo, carga baja.

## Resultados — modelo 1: SIR (ms/paso, mediana de 3 semillas, hasta 300 pasos)

| Grilla | Agentes | swarm-abm | Agents.jl | Mesa | vs Mesa | vs Agents.jl |
|---|---|---|---|---|---|---|
| 25×25 | 625 | 0.0162 | 0.0577 | 0.870 | 54× | 3.6× |
| 50×50 | 2 500 | 0.0673 | 0.2748 | 3.546 | 53× | 4.1× |
| 100×100 | 10 000 | 0.2459 | 1.2870 | 14.619 | 59× | 5.2× |
| 200×200 | 40 000 | 1.3387 | 6.6194 | 64.632 | 48× | 4.9× |

- swarm-abm vs **Mesa**: **48–59×** · vs **Agents.jl**: **3.6–5.2×** · Agents.jl vs Mesa: 10–15×

## Resultados — modelo 2: Schelling (ms/paso, mediana de 3 semillas, 100 pasos fijos)

Segundo modelo canónico, con un patrón de acceso distinto (movilidad + celdas
vacías en vez de contagio in situ). Pasos fijos sin corte por convergencia.

| Grilla | Agentes | swarm-abm | Agents.jl | Mesa | vs Mesa | vs Agents.jl |
|---|---|---|---|---|---|---|
| 25×25 | 625 | 0.0516 | 0.1212 | 2.324 | 45× | 2.3× |
| 50×50 | 2 500 | 0.1759 | 0.5623 | 8.947 | 51× | 3.2× |
| 100×100 | 10 000 | 0.8385 | 2.6390 | 54.811 | 65× | 3.1× |
| 200×200 | 40 000 | 3.3883 | 12.9040 | 621.851 | 184× | 3.8× |

- swarm-abm vs **Mesa**: **45–184×** · vs **Agents.jl**: **2.3–3.8×** · Agents.jl vs Mesa: 16–48×
- *El 184× sobre Mesa a 200² refleja en parte el manejo O(n) de celdas vacías /
  `move_agent` de Mesa en Schelling, no solo el costo de cómputo puro — se
  reporta honestamente.*

## Lectura (dos modelos)

A través de **dos modelos canónicos con patrones de acceso distintos**,
swarm-abm supera con holgura a los motores de runtime interpretado o JIT:
**~2–5× sobre Agents.jl** (el competidor compilado de referencia, Julia) y
**~45–184× sobre Mesa**, manteniendo además determinismo bit a bit y un target
WASM que Agents.jl no ofrece. El control de cordura Agents.jl/Mesa (10–48×, su
rango conocido) confirma que los espejos en Agents.jl son implementaciones
idiomáticas y justas.

> **swarm-abm NO es el ABM más rápido en Rust.** Frente a **krABMaga** (otro
> motor nativo en Rust) swarm-abm **pierde** en velocidad bruta: ~1.15–2.1× en
> SIR, ~2.0–2.6× en Schelling. Eso es esperable y no toca la tesis del motor —
> el wedge de swarm-abm es el **determinismo bit a bit por construcción**, que
> krABMaga no reclama (vende memory-safety; su `parallel` es Experimental). El
> benchmark completo, con su lectura honesta, está en **`KRABMAGA.md`**. Se
> reporta explícitamente para no seleccionar solo los comparadores donde se
> gana.

## Caveats (honestos)

- Un solo modelo (SIR), un solo hilo, una sola máquina. Schelling como segundo
  modelo está pendiente para generalizar.
- Ambas son implementaciones *idiomáticas* en su framework (no micro-optimizadas
  a mano). swarm-abm se beneficia de una grilla especializada; ese es,
  precisamente, el punto de diseño del motor.
- El paralelismo intra-paso de swarm-abm (fase `decide` paralela) aún no está
  medido aquí; este benchmark es single-thread.

## Reproducir

```bash
# Julia 1.10 LTS + Agents.jl (pin de Distributions por compat de macro)
juliaup add lts
julia +lts --project=validation/agents-jl -e 'using Pkg; Pkg.instantiate()'
# Mesa
validation/.venv/bin/pip install mesa==3.5.1
# Benchmark (genera validation/data/cross_engine.csv + summary)
#   ver el bucle en este commit; tres motores × 4 tamaños × 3 semillas
```

> **Nota de reproducibilidad**: Agents.jl 7.0.2 es incompatible con
> Distributions ≥ 0.25.110 (cambio estricto del macro `@check_args`). El
> `Manifest.toml` fija **Distributions 0.25.100**. Julia 1.12 no precompila esta
> combinación; usar **1.10 LTS**.
