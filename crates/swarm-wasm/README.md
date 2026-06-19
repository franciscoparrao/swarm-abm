# swarm-wasm — visor WASM de swarm-abm

Corre los modelos del motor (Schelling, SIR, Sugarscape) en el navegador, sobre
un `<canvas>`. El bucle de simulación se compila a WebAssembly; JavaScript solo
anima y dibuja el buffer RGBA que entrega cada paso.

## Construir y servir

```bash
# desde crates/swarm-wasm/
wasm-pack build --target web --out-dir www/pkg --release
cd www && python3 -m http.server 8000
# abrir http://localhost:8000
```

`www/pkg/` es código generado (no versionado): hay que construirlo antes de
servir. El `.wasm` resultante pesa ~68 KB.

## Por qué está fuera del workspace

Igual que `swarm-py`, este crate se excluye del `cargo --workspace`: se compila
a `wasm32-unknown-unknown` con `wasm-pack`, no con el `cargo` del workspace. Usa
`swarm-core`/`swarm-models` con `default-features = false` (sin rayon: el target
wasm no tiene hilos), por lo que los ensembles correrían en secuencia — pero el
visor solo necesita avanzar un modelo paso a paso.

## Controles

Selector de modelo, un parámetro por modelo (tolerancia / β / crecimiento),
tamaño de grilla, velocidad (pasos por frame), play/pausa/paso/reset y semilla.
Misma semilla ⇒ misma corrida (el motor es determinista también en wasm).
