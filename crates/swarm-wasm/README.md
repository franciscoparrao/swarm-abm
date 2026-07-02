# swarm-wasm — swarm-abm WASM viewer

Runs the engine's models (Schelling, SIR, Sugarscape) in the browser, on a
`<canvas>`. The simulation loop compiles to WebAssembly; JavaScript only
animates and draws the RGBA buffer produced each step.

## Build and serve

```bash
# from crates/swarm-wasm/
wasm-pack build --target web --out-dir www/pkg --release
cd www && python3 -m http.server 8000
# open http://localhost:8000
```

`www/pkg/` is generated code (not version-controlled): build it before
serving. The resulting `.wasm` weighs ~68 KB.

## Why it's outside the workspace

Like `swarm-py`, this crate is excluded from `cargo --workspace`: it
compiles to `wasm32-unknown-unknown` with `wasm-pack`, not with the
workspace's `cargo`. It uses `swarm-abm`/`swarm-models` with
`default-features = false` (no rayon: the wasm target has no threads), so
ensembles would run sequentially — but the viewer only needs to advance
one model step by step.

## Controls

Model selector, one parameter per model (tolerance / β / growth), grid
size, speed (steps per frame), play/pause/step/reset, and seed. Same seed
⇒ same run (the engine is deterministic in wasm too).
