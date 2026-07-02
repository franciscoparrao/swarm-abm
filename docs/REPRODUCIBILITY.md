# Reproducibility policy

swarm-abm's central claim is: **same seed → bit-identical results**, on any
platform, with any number of threads, forever — not "usually," not "unless
a dependency changes under you." This document says precisely what that
guarantee covers, how it is enforced, and what breaks it.

## What's guaranteed

Given the same `(seed, model code, engine version)`:

- `Simulation::run`/`step` produce identical `DataCollector`/
  `AgentDataCollector` series regardless of the machine, OS, or CPU
  architecture (verified on x86-64 and wasm32 — see below).
- `Simulation::run_parallel`/`step_parallel`/`run_with_peers_parallel`
  produce results **identical to the sequential run**, regardless of thread
  count. This holds because each agent's randomness during `decide` is
  derived from `(seed, step, agent_id)` via [`rng::child_rng`], never from
  which thread or in what order the scheduler happens to reach it.
- Topology generators (`Graph::erdos_renyi`/`watts_strogatz`/
  `barabasi_albert`) are deterministic given their seed — including under
  Rust's unspecified `HashMap`/`HashSet` iteration order, which the engine
  never relies on internally (see `docs/AUDIT.md`, P0-1).
- `batch::run_ensemble`/`run_sweep` and `experiment::sobol`/`morris`/
  `latin_hypercube` are deterministic given their seed, in parallel or not.

## What produces the guarantee

The guarantee rests on a small set of engine-owned primitives in
[`rng`](../crates/swarm-abm/src/rng.rs), built directly on ChaCha8's raw
stream (`RngCore::next_u64`) instead of on `rand::Rng`'s higher-level API:

- `uniform_below`/`uniform_usize`/`uniform_f64` — unbiased range sampling
  (Lemire's method / top-bit scaling).
- `bernoulli` — probability-weighted boolean.
- `shuffle` — Fisher–Yates permutation.
- `child_rng` — per-agent RNG derived from `(seed, step, agent)` via a
  SplitMix64 hash chain.

**Why not just use `rand::Rng::random_range`/`random_bool`/
`SliceRandom::shuffle`?** Those are explicitly **unspecified-algorithm**
APIs: `rand` documents that it may change how it maps raw bits to a bounded
range, a boolean, or a permutation between versions, without that counting
as a breaking change on their end. A model built on those APIs could see
its published results silently stop being reproducible after a routine
`cargo update`. The engine reimplements those four operations once, on top
of the one thing that *is* a stable, published specification — ChaCha8's
raw keystream, which `rand_chacha` cannot change without breaking its own
published test vectors.

**What about the `sobol` crate** (used by `experiment::sobol`, feature
`experiment`)? That's a deliberately different case: a Sobol' sequence,
given fixed, published direction numbers (Joe & Kuo's) and the canonical
Antonov–Saleev recurrence, is a **mathematical specification** with no
implementation freedom — there's no "algorithm choice" a version bump could
silently change. It's a dependency on a specification, not on an
unspecified implementation detail. See the module doc in
[`experiment.rs`](../crates/swarm-abm/src/experiment.rs) for the full
reasoning.

## What is NOT guaranteed

- **Your own model code drawing directly from `rand::Rng`/
  `SliceRandom`** instead of the primitives above. This is legal (the
  prelude re-exports `rand::Rng`/`SliceRandom` for cases the engine's
  primitives don't cover, e.g. arbitrary float ranges) but opts out of the
  stability guarantee for that draw. Prefer `uniform_usize`/`bernoulli`/
  `shuffle` wherever they cover your need.
- **Changing the number or order of RNG draws** in your own model between
  versions of *your* code. The engine guarantees the *stream* is stable;
  it can't guarantee your model still asks for the same sequence of draws
  after you edit it.
- **Compatibility across a `rand_chacha` major version bump** that changes
  the ChaCha8 stream itself. This has never happened in `rand_chacha`'s
  history (the whole point of ChaCha8 as a choice is that it's a published
  cipher spec, not swarm-abm's own invention) but if it ever did, that
  would be a determinism-breaking change on the engine's side too — see
  the policy below.

## Stability policy: what counts as a breaking change

Any change that alters the exact sequence of bits `SimRng`/`child_rng`/
`uniform_below`/`uniform_usize`/`uniform_f64`/`bernoulli`/`shuffle` produce
for a **fixed seed** is treated as **determinism-breaking**, regardless of
whether it "looks like" a bug fix:

- It is never shipped silently in a patch release.
- It bumps at least a minor version (while `0.x`; a major version post
  `1.0`), and is called out explicitly under a **"Determinism-breaking"**
  heading in `CHANGELOG.md` — not buried in a generic "Fixed" bullet.
- Past examples of this exact situation: `docs/AUDIT.md` P0-1 (a
  `HashSet`-driven non-determinism in `Graph::barabasi_albert`), P0-2
  (migrating off `rand::Rng`'s unspecified sampling algorithms), P0-3
  (`child_rng`'s combination changed from XOR to a hash chain), and P1-1
  (the generational-arena rewrite of `AgentSet`, which changes agent-index
  assignment for any model with demography). Each was deliberate, each
  changed previously-published numeric results, and each is documented
  with that explicit warning in `docs/AUDIT.md`. The SIGRID case study
  (`models/sigrid/PARITY.md`, "Re-validación 2026-07-02") is what
  re-validating a real downstream model after such a bump looks like: the
  exact numbers moved, the scientific conclusions didn't.
- Bumping the MSRV (see `Cargo.toml`, `workspace.package.rust-version`) is
  also treated as a minor-version event, for the same "don't silently
  break downstream consumers" reason, though it doesn't affect numeric
  results.

## How it's enforced, not just claimed

`crates/swarm-abm/tests/golden_values.rs` pins the exact output of every
primitive above for fixed seeds as literal expected values (not "runs
without panicking" — literal `u64`/`f64` constants). If a `cargo update`
ever silently changed a transitive dependency's behavior in a way that
altered these values, this test — not a published paper's numbers — is
what would catch it first.

As of the CI job `golden-values-wasm32` (`.github/workflows/ci.yml`), that
same test file is run on **both** x86-64 (the default `check` job) and
`wasm32-wasip1` under `wasmtime`, on every push and pull request. The
cross-platform identity claim is a CI gate that fails the build, not a
sentence in a README.

## Practical guidance for model authors

- Prefer `swarm_abm::rng::{uniform_usize, uniform_f64, bernoulli,
  shuffle}` over the `rand::Rng`/`SliceRandom` equivalents wherever they
  cover your need.
- Never seed from wall-clock time, thread ID, or any other non-reproducible
  source inside a model meant to be reproducible — only from the
  `Simulation`'s seed (directly, or via `child_rng`).
- Don't rely on `HashMap`/`HashSet` iteration order for anything that
  affects simulation results (insertion-ordered collections, or explicit
  sorting, if order matters).
- If you need a checkpoint/restore (`Simulation::from_checkpoint`, feature
  `serde`), the four pieces that fully determine future behavior are the
  model state, the original seed, the current `SimRng` state, and the step
  count — see `sim.rs` and `tests/checkpoint.rs`.
