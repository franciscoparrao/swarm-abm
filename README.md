# swarm-abm

Motor de modelado basado en agentes (ABM) espacial en Rust — un
"Mesa/NetLogo moderno": millones de agentes, determinismo reproducible y
(a futuro) targets Python y WASM.

## Estructura

- `crates/swarm-core` — el motor: traits `Agent`/`Model`, scheduler
  (orden fijo, aleatorio o **simultáneo en dos fases**), `Grid2D` con
  vecindades Moore/Von Neumann y torus opcional, `DataCollector` de
  series por paso, RNG sembrable (ChaCha8, portable entre plataformas),
  **batch runner** (`batch::run_ensemble` / `run_sweep`) para réplicas y
  barridos de parámetros en paralelo (rayon, feature `parallel`),
  **espacio de red** (`graph::Graph<T>`) con generadores Erdős–Rényi,
  Watts–Strogatz y Barabási–Albert, y **espacio continuo**
  (`continuous::ContinuousSpace<T>`, vecindad por radio + spatial hashing).
  Tres paradigmas espaciales —grilla, grafo, continuo— bajo el mismo
  `Agent`/`Model`.
- `examples/schelling` — segregación de Schelling (1971).
- `examples/sir` — SIR espacial (contagio en grilla).
- `examples/difusion` — feromona depositada por caminantes que difunde
  (`Grid2D::diffuse`, semántica NetLogo) y se evapora; converge al punto
  fijo analítico.
- `examples/network-sir` — contagio SIR **sobre una red** (no una grilla):
  compara la dinámica epidémica sobre topologías aleatoria, mundo-pequeño y
  libre de escala. Demuestra que el mismo `Agent`/`Model` corre sobre grafo.
- `examples/boids` — flocking de Reynolds en el **espacio continuo**: de
  reglas locales (separación/alineación/cohesión) emerge una bandada
  (orden de Vicsek 0.02 → 0.96).
- `examples/sugarscape` — **Sugarscape** (Epstein & Axtell, 1996), el modelo
  canónico de la economía basada en agentes: agentes que se mueven, cosechan
  y **mueren** sobre un paisaje de dos picos de azúcar. De una población casi
  homogénea emerge una distribución de riqueza desigual (Gini 0.24 → 0.42) y
  la población se autorregula a la capacidad de carga. Ejercita movimiento +
  muerte de agentes (baja diferida en `after_step`) + paisaje con estado.
- `models/debris-flow` — **modelo cliente real**: flujos de detritos del
  evento Atacama 2015 (Copiapó, DEM 5871×5422 @ 30 m), port fiel de
  [debris-flow-abm](https://github.com/franciscoparrao/debris-flow-abm)
  (Mesa/Python). Paridad distribucional verificada contra el código
  original sobre insumos idénticos y **~100× más rápido** (130–240 s →
  1.2–4 s por corrida). En el mejor caso documentado (Chañaral, Config B)
  reproduce el IoU de referencia con diferencia **< 1 %** (0.468 vs 0.465);
  detalles en `models/debris-flow/PARITY.md`. Y va más allá: vía un ciclo
  iterativo de diagnóstico del error → mecanismo faltante → recalibración
  (expansión en abanico, inicio ponderado por susceptibilidad, y un raster
  de sedimento derivado con **SurtGIS**), **supera el mejor caso histórico** —
  IoU 0.468 → **0.555** (+19 %; precision 0.69 → 0.83), un ciclo inviable en
  Python. Ver `models/debris-flow/PHYSICS_EXPERIMENT.md`.
  Incluye **calibración por Differential Evolution** (`bin/calibrate`,
  rayon + stack compartido): lo que en Python era una calibración de
  ~11–34 h cabe en 1–5 min, y duplica el IoU medio del modelo
  (0.074 → 0.158) con validación de robustez — ver
  `models/debris-flow/CALIBRATION.md`. Y un **benchmark de 5
  metaheurísticas** (`bin/benchmark`: DE/GA/PSO/SA/GWO, N corridas, tests
  de Friedman + Wilcoxon) — el estudio comparativo que el costo de Python
  impedía: GWO gana con respaldo estadístico
  (`models/debris-flow/BENCHMARK_OPTIM.md`).

## Uso rápido

```bash
cargo test --workspace          # tests + doc-tests
cargo run --release -p schelling [semilla]
cargo run --release -p sir [semilla]
cargo run --release -p difusion [semilla]
```

Misma semilla → resultados bit a bit idénticos (scheduler y RNG son
deterministas). Ver el ejemplo de API completo en `crates/swarm-core/src/lib.rs`.

## Bindings Python (PyO3)

`crates/swarm-py` expone el motor a Python con la estrategia **modelos nativos
+ barridos**: los modelos viven compilados en Rust (`swarm-models`) y Python
solo los configura, los dispara y recibe las series para analizarlas con
numpy/pandas/matplotlib. El bucle de simulación corre **íntegro en Rust** —se
conserva el speedup ~45–67× sobre Mesa— y los barridos de parámetros corren en
paralelo (rayon) liberando el GIL.

```bash
python -m venv .venv && . .venv/bin/activate
pip install maturin
cd crates/swarm-py && maturin develop --release
python demo.py
```

```python
import swarm_abm as sw

m = sw.Sir(size=200, beta=0.15, seed=42)
m.run(500)
infected = m.series("i")          # curva de infectados, lista por paso
print(m.recovered)                # tamaño final de la epidemia

# barrido paralelo de beta, a velocidad Rust → filas para un DataFrame
rows = sw.sir_sweep(betas=[0.05, 0.1, 0.2], seeds=range(30))
```

Misma `(parámetros, semilla)` ⇒ resultado idéntico al binario nativo (paridad
bit a bit verificada). El crate se construye con maturin y queda fuera del
`cargo --workspace` (la feature `extension-module` de PyO3 no enlaza libpython).
El modelo SIR es el primero expuesto; Schelling y Sugarscape siguen el mismo
patrón.

## Validación: paridad numérica contra Mesa

`validation/` contiene espejos exactos de Schelling y SIR escritos en
[Mesa](https://mesa.readthedocs.io/) (Python) y un protocolo de paridad
distribucional: 50 réplicas por motor, test z de dos muestras por métrica
(α = 0.05). Resultado: **las 7 métricas en paridad** (|z| ≤ 1.22); las
curvas medias de ensamble difieren < 0.021 en todo el horizonte. Detalle
en `validation/REPORT.md`. En la misma configuración, swarm-core corre
~67× más rápido que Mesa.

```bash
python3 -m venv validation/.venv
validation/.venv/bin/pip install -r validation/mesa/requirements.txt
./validation/run_validation.sh 50
```

## Benchmarks

Cross-engine (mismo SIR, medición en proceso, mediana sobre réplicas;
detalle y entorno en `validation/BENCHMARKS.md`):

| Grilla | Agentes | Rust (ms/paso) | Mesa (ms/paso) | Speedup |
|---|---|---|---|---|
| 25×25 | 625 | 0.023 | 1.35 | 58× |
| 50×50 | 2.500 | 0.142 | 8.23 | 58× |
| 100×100 | 10.000 | 0.312 | 20.79 | 67× |
| 200×200 | 40.000 | 1.401 | 63.19 | 45× |

swarm-core sostiene **~25–38 millones de agente-pasos por segundo** en un
hilo (i7-1270P); con 1 millón de agentes móviles, ~12 M/s (~12 pasos/s
en vivo). El runner reutiliza el buffer de orden entre pasos y
`Grid2D::random_neighbor` elige vecino sin asignar memoria — el camino
caliente del motor no aloca. Microbenchmarks con criterion:
`cargo bench -p swarm-core` (escalamiento de caminantes 10k→1M, SIR
end-to-end, Life simultáneo a 37 M celdas/s, `diffuse`). Reproducir el
cross-engine: `./validation/run_benchmark.sh`.

## Diseño clave

Los agentes viven en un `AgentSet` dentro del modelo. Para ejecutar
`Agent::step(&mut self, id, &mut Model, &mut SimRng)` sin conflicto de
préstamos, el runner usa el patrón **take-out**: saca al agente del set,
corre su step con acceso mutable al modelo completo, y lo devuelve.

La **activación simultánea** (`Activation::Simultaneous`) usa dos fases
con garantía del compilador: en `decide(&mut self, id, &Model, rng)` el
modelo llega *inmutable* — nadie puede escribir estado compartido antes
del commit en `apply` (a diferencia de Mesa, donde es disciplina del
usuario). Un modelo escrito como `decide`/`apply` corre bajo cualquier
política: el `step` por defecto los encadena. Validado con el Juego de
la Vida (`tests/simultaneous.rs`): el blinker oscila bajo simultánea y
se rompe bajo secuencial.

## Roadmap

**v0.3 (actual) — completado:**

- [x] Tres espacios: grilla, grafo (Erdős–Rényi/Watts–Strogatz/Barabási–Albert)
  y continuo (radio + spatial hashing), bajo el mismo `Agent`/`Model`.
- [x] Batch runs y barrido de parámetros (Rayon, feature `parallel`).
- [x] Activación simultánea en dos fases con garantía del compilador.
- [x] Benchmark formal vs Mesa (criterion + protocolo de paridad).
- [x] Reescritura de `debris-flow-abm` sobre el motor (modelo cliente real).

**v0.4 (en curso):**

- [x] Bindings PyO3 (API Python sobre el motor nativo) — modelo SIR + barrido
  paralelo; falta exponer Schelling y Sugarscape.
- [ ] Visor WASM (correr modelos en el navegador).

Ver el historial completo en [`CHANGELOG.md`](CHANGELOG.md).

## Licencia

MIT OR Apache-2.0
