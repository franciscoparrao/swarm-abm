# swarm-abm

Spatial agent-based modeling (ABM) engine in Rust — a "modern
Mesa/NetLogo": millions of agents, reproducible determinism, and
(eventually) Python and WASM targets.

## Structure

- `crates/swarm-abm` — the engine: `Agent`/`Model` traits, scheduler
  (fixed order, random, or **two-phase simultaneous**), `Grid2D` with
  Moore/Von Neumann neighborhoods and optional torus, per-step
  `DataCollector` time series, seedable RNG (ChaCha8, portable across
  platforms), **batch runner** (`batch::run_ensemble` / `run_sweep`) for
  replicates and parameter sweeps in parallel (rayon, `parallel`
  feature), **network space** (`graph::Graph<T>`) with Erdős–Rényi,
  Watts–Strogatz, and Barabási–Albert generators, and **continuous space**
  (`continuous::ContinuousSpace<T>`, radius-based neighborhood + spatial
  hashing). Three spatial paradigms — grid, graph, continuous — under the
  same `Agent`/`Model`.
- `examples/schelling` — Schelling's segregation model (1971).
- `examples/sir` — spatial SIR (grid-based contagion).
- `examples/difusion` — pheromone deposited by walkers that diffuses
  (`Grid2D::diffuse`, NetLogo semantics) and evaporates; converges to the
  analytical fixed point.
- `examples/network-sir` — SIR contagion **on a network** (not a grid):
  compares epidemic dynamics across random, small-world, and scale-free
  topologies. Demonstrates that the same `Agent`/`Model` runs on a graph.
- `examples/boids` — Reynolds flocking in **continuous space**: from local
  rules (separation/alignment/cohesion) a flock emerges (Vicsek order
  0.02 → 0.96).
- `examples/sugarscape` — **Sugarscape** (Epstein & Axtell, 1996), the
  canonical model of agent-based economics: agents that move, harvest, and
  **die** on a two-peak sugar landscape. An almost homogeneous population
  gives rise to unequal wealth distribution (Gini 0.24 → 0.42), and
  population self-regulates to carrying capacity. Exercises movement +
  agent death (deferred removal in `after_step`) + a stateful landscape.
- `examples/life` — Conway's Game of Life, the canonical **simultaneous
  activation** model; serves as the testbed for **parallel `decide`**
  (`--bench --parallel --work N`): scales ~5× at 16 threads when per-agent
  decision is compute-bound. See `validation/SCALABILITY.md`.
- `models/debris-flow` — **real client model**: debris flows from the
  Atacama 2015 event (Copiapó, 5871×5422 DEM @ 30 m), a faithful port of
  [debris-flow-abm](https://github.com/franciscoparrao/debris-flow-abm)
  (Mesa/Python). Distributional parity verified against the original code
  on identical inputs, **~100× faster** (130–240 s → 1.2–4 s per run). In
  the best documented case (Chañaral, Config B) it reproduces the
  reference IoU within **< 1 %** (0.468 vs 0.465); details in
  `models/debris-flow/PARITY.md`. And it goes further: through an
  iterative diagnose-error → find-missing-mechanism → recalibrate cycle
  (fan expansion, susceptibility-weighted initiation, and a sediment
  raster derived with **SurtGIS**), it **beats the best historical
  case** — IoU 0.468 → **0.555** (+19 %; precision 0.69 → 0.83), a cycle
  that wasn't feasible in Python. See
  `models/debris-flow/PHYSICS_EXPERIMENT.md`.
  Includes **Differential Evolution calibration** (`bin/calibrate`, rayon
  + a shared stack): what was an ~11–34 h calibration in Python fits in
  1–5 min, and it doubles the model's mean IoU (0.074 → 0.158) with
  robustness validation — see `models/debris-flow/CALIBRATION.md`. And a
  **5-metaheuristic benchmark** (`bin/benchmark`: DE/GA/PSO/SA/GWO, N
  runs, Friedman + Wilcoxon tests) — the comparative study that Python's
  cost made impractical: GWO wins with statistical backing
  (`models/debris-flow/BENCHMARK_OPTIM.md`).

## Quick start

```bash
cargo test --workspace          # tests + doc-tests
cargo run --release -p schelling [seed]
cargo run --release -p sir [seed]
cargo run --release -p difusion [seed]
```

Same seed → bit-for-bit identical results (scheduler and RNG are
deterministic). See the full API example in `crates/swarm-abm/src/lib.rs`.

## Python bindings (PyO3)

`crates/swarm-py` exposes the engine to Python with a **native models +
sweeps** strategy: models live compiled in Rust (`swarm-models`), and
Python only configures them, runs them, and receives the series to
analyze with numpy/pandas/matplotlib. The simulation loop runs **entirely
in Rust** — preserving the ~45–67× speedup over Mesa — and parameter
sweeps run in parallel (rayon), releasing the GIL.

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
infected = m.series("i")          # infection curve, one value per step
print(m.recovered)                # final epidemic size

# parallel sweep over beta, at Rust speed → rows for a DataFrame
rows = sw.sir_sweep(betas=[0.05, 0.1, 0.2], seeds=range(30))
```

Same `(parameters, seed)` ⇒ identical result to the native binary (bit-for-bit
parity verified). The crate is built with maturin and sits outside
`cargo --workspace` (PyO3's `extension-module` feature doesn't link
libpython). Exposed models: `Sir`, `Schelling`, and `Sugarscape` (same
`run`/`series`/getters API), with a parallel sweep per model (`sir_sweep`,
`schelling_sweep`, `sugarscape_sweep`).

## WASM viewer (browser)

`crates/swarm-wasm` compiles the engine to WebAssembly and runs the models
(Schelling, SIR, Sugarscape) on a `<canvas>`, with no compute server: the
loop lives in wasm and JavaScript only draws each step's RGBA buffer. The
binary weighs ~68 KB and is deterministic (same seed ⇒ same run, parity
with native verified).

```bash
cd crates/swarm-wasm
wasm-pack build --target web --out-dir www/pkg --release
cd www && python3 -m http.server 8000   # open http://localhost:8000
```

## Validation: numerical parity against Mesa

`validation/` contains exact mirrors of Schelling and SIR written in
[Mesa](https://mesa.readthedocs.io/) (Python), plus a distributional
parity protocol: 50 replicates per engine, a two-sample z-test per metric
(α = 0.05). Result: **all 7 metrics at parity** (|z| ≤ 1.22); mean
ensemble curves differ by < 0.021 across the whole horizon. Details in
`validation/REPORT.md`. Under the same configuration, swarm-abm runs
~67× faster than Mesa.

```bash
python3 -m venv validation/.venv
validation/.venv/bin/pip install -r validation/mesa/requirements.txt
./validation/run_validation.sh 50
```

## Benchmarks

Cross-engine (same SIR, in-process measurement, median over replicates;
details and environment in `validation/BENCHMARKS.md`):

| Grid | Agents | Rust (ms/step) | Mesa (ms/step) | Speedup |
|---|---|---|---|---|
| 25×25 | 625 | 0.023 | 1.35 | 58× |
| 50×50 | 2,500 | 0.142 | 8.23 | 58× |
| 100×100 | 10,000 | 0.312 | 20.79 | 67× |
| 200×200 | 40,000 | 1.401 | 63.19 | 45× |

swarm-abm sustains **~25–38 million agent-steps per second** on a single
thread (i7-1270P); with 1 million mobile agents, ~12 M/s (~12 steps/s
live). The runner reuses the order buffer between steps and
`Grid2D::random_neighbor` picks a neighbor without allocating — the
engine's hot path allocates nothing. Microbenchmarks with criterion:
`cargo bench -p swarm-abm` (walker scaling 10k→1M, end-to-end SIR,
simultaneous Life at 37 M cells/s, `diffuse`). Reproduce the cross-engine
numbers: `./validation/run_benchmark.sh`.

## Key design

Agents live in an `AgentSet` inside the model. To run
`Agent::step(&mut self, id, &mut Model, &mut SimRng)` without a borrow
conflict, the runner uses the **take-out** pattern: it takes the agent out
of the set, runs its step with mutable access to the whole model, and
returns it.

**Simultaneous activation** (`Activation::Simultaneous`) uses two phases
with a compiler-enforced guarantee: in `decide(&mut self, id, &Model,
rng)` the model arrives *immutable* — no one can write shared state before
the commit in `apply` (unlike Mesa, where that's left to user discipline).
A model written in `decide`/`apply` style runs under any policy: the
default `step` chains them. Validated with the Game of Life
(`tests/simultaneous.rs`): the blinker oscillates under simultaneous
activation and breaks under sequential.

That compiler-proven immutability is what makes it safe to **parallelize
the `decide` phase** (`Simulation::run_parallel`, `parallel` feature): each
agent uses a per-agent RNG derived from `(seed, step, id)` — not from the
thread — so the result is **bit-identical** to sequential regardless of
thread count (verified in `tests/parallel_decide.rs`). Scales ~5× at 16
threads on compute-bound decisions; details in
`validation/SCALABILITY.md`.

## Roadmap

**v0.3 (current) — complete:**

- [x] Three spaces: grid, graph (Erdős–Rényi/Watts–Strogatz/Barabási–Albert)
  and continuous (radius + spatial hashing), under the same `Agent`/`Model`.
- [x] Batch runs and parameter sweeps (Rayon, `parallel` feature).
- [x] Two-phase simultaneous activation with a compiler-enforced guarantee.
- [x] Formal benchmark vs. Mesa (criterion + parity protocol).
- [x] Rewrite of `debris-flow-abm` on top of the engine (real client model).

**v0.4 (in progress):**

- [x] PyO3 bindings (Python API over the native engine) — `Sir`,
  `Schelling`, and `Sugarscape` with parallel sweeps.
- [x] WASM viewer (run models in the browser) — Schelling, SIR, and
  Sugarscape on canvas, ~68 KB binary.

See the full history in [`CHANGELOG.md`](CHANGELOG.md).

## License

MIT OR Apache-2.0
