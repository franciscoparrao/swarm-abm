# Benchmark cross-engine — SIR espacial

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
- **Métrica: ms por paso.** Cada motor termina la epidemia en un paso distinto
  (RNG distinto), así que el tiempo total no es comparable; se normaliza por
  paso. Mediana sobre 3 semillas, hasta 300 pasos.
- **Solo se cronometra el stepping** — la construcción del modelo queda fuera
  del cronómetro en los tres motores.
- **Julia: warmup explícito** antes de medir, para no cronometrar la
  compilación JIT (error clásico que invalida comparaciones con Julia).
- Mismo equipo (i7-1270P, 16 hilos), un solo hilo, carga baja.

## Resultados (ms/paso, mediana de 3 semillas)

| Grilla | Agentes | swarm-abm | Agents.jl | Mesa | swarm-abm vs Mesa | swarm-abm vs Agents.jl |
|---|---|---|---|---|---|---|
| 25×25 | 625 | 0.0162 | 0.0577 | 0.870 | 54× | 3.6× |
| 50×50 | 2 500 | 0.0673 | 0.2748 | 3.546 | 53× | 4.1× |
| 100×100 | 10 000 | 0.2459 | 1.2870 | 14.619 | 59× | 5.2× |
| 200×200 | 40 000 | 1.3387 | 6.6194 | 64.632 | 48× | 4.9× |

**Resumen:**
- swarm-abm vs **Mesa**: **48–59×**
- swarm-abm vs **Agents.jl**: **3.6–5.2×**
- Agents.jl vs Mesa: 10–15× *(control de cordura: Agents.jl rinde en su rango
  conocido sobre Mesa, lo que confirma que el espejo en Agents.jl es una
  implementación idiomática y justa, no una versión deliberadamente lenta).*

## Lectura

swarm-abm no solo supera al baseline lento (Mesa) sino también al **competidor
compilado de referencia (Agents.jl) por ~4–5×**, manteniendo además
determinismo bit a bit y un target WASM que Agents.jl no ofrece. El factor de
cordura Agents.jl/Mesa (10–15×, en su rango esperado) respalda que la
comparación es legítima.

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
