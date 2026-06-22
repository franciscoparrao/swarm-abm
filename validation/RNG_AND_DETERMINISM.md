# V&V: calidad del RNG y determinismo cross-platform

Dos verificaciones que sustentan la afirmación de **reproducibilidad** del
motor — la base metodológica de un ABM creíble.

## A. Calidad estadística del RNG (PractRand 0.95)

El motor usa `SimRng` (ChaCha8) y, para el `decide` paralelo, un RNG **por
agente** derivado de `(semilla, paso, id)` con una mezcla splitmix64
(`rng::child_rng`). ChaCha8 es de grado criptográfico y su calidad está
establecida; lo que hay que verificar es que la **derivación por-agente no
introduzca correlación** entre los streams de agentes vecinos (lo que
arruinaría un ABM paralelo).

Se vuelcan dos streams del motor (`examples/rng-dump`) y se pasan por PractRand:

| Stream | Qué prueba | Resultado |
|---|---|---|
| `single` (ChaCha8 directo) | control de cordura | **no anomalies** hasta 256 MB |
| `interagent` (primera extracción de `child_rng` por agente, ciclando agentes y pasos) | decorrelación entre streams de agentes | **no anomalies** hasta **1 GB** (246 test results) |

El stream inter-agente —el relevante para el paralelismo— pasa la batería
completa de PractRand sin una sola anomalía hasta 1 GB: la derivación de
semillas por agente produce streams independientes y bien distribuidos.

```bash
cargo build --release -p rng-dump
target/release/rng-dump interagent 42 | ./RNG_test stdin64 -tlmax 1GB
target/release/rng-dump single     42 | ./RNG_test stdin64 -tlmax 256MB
```

## B. Determinismo cross-platform (x86-64 nativo vs wasm32)

La misma implementación de los modelos (`swarm-models`) se compila a **dos
arquitecturas** —nativo x86-64 (vía los bindings PyO3) y wasm32 (vía node)— y
se comparan los **bits IEEE-754 exactos** (hex little-endian) de métricas
sensibles a la configuración, con la misma semilla.

| Modelo | Métrica | x86-64 (PyO3) | wasm32 (node) | |
|---|---|---|---|---|
| SIR | recovered | `d9cef753e3a5ef3f` | `d9cef753e3a5ef3f` | ✓ |
| SIR | infected | `0000000000000000` | `0000000000000000` | ✓ |
| Schelling | happy | `000000000000f03f` | `000000000000f03f` | ✓ |
| Schelling | mean_similarity | `ef5c842f15a3e83f` | `ef5c842f15a3e83f` | ✓ |
| Sugarscape | population | `203` | `203` | ✓ |
| Sugarscape | gini | `d9b1231640ceda3f` | `d9b1231640ceda3f` | ✓ |

**Todas las métricas son bit-idénticas**, incluido `mean_similarity` (el índice
de segregación de Schelling, que depende de la configuración espacial exacta) y
`gini` (la distribución completa de riqueza de Sugarscape). La identidad se
mantiene **a pesar del cambio de tamaño de palabra** (usize de 64 bits en
nativo vs 32 bits en wasm32): el motor no cae en las trampas de portabilidad de
RNG dependientes del ancho. El determinismo es **portable**, no solo
intra-plataforma — un prerrequisito para ciencia reproducible.

```bash
# nativo (PyO3) — requiere `maturin develop` en crates/swarm-py
.venv/bin/python validation/crossplatform/native.py
# wasm32 (node) — requiere `wasm-pack build --target nodejs`
node validation/crossplatform/wasm.js /ruta/al/pkg/swarm_wasm.js
```
