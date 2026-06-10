# swarm-abm

Motor de modelado basado en agentes (ABM) espacial en Rust — un
"Mesa/NetLogo moderno": millones de agentes, determinismo reproducible y
(a futuro) targets Python y WASM.

## Estructura

- `crates/swarm-core` — el motor: traits `Agent`/`Model`, scheduler
  (orden fijo o aleatorio), `Grid2D` con vecindades Moore/Von Neumann y
  torus opcional, `DataCollector` de series por paso y RNG sembrable
  (ChaCha8, portable entre plataformas).
- `examples/schelling` — segregación de Schelling (1971).
- `examples/sir` — SIR espacial (contagio en grilla).
- `examples/difusion` — feromona depositada por caminantes que difunde
  (`Grid2D::diffuse`, semántica NetLogo) y se evapora; converge al punto
  fijo analítico.

## Uso rápido

```bash
cargo test --workspace          # tests + doc-tests
cargo run --release -p schelling [semilla]
cargo run --release -p sir [semilla]
cargo run --release -p difusion [semilla]
```

Misma semilla → resultados bit a bit idénticos (scheduler y RNG son
deterministas). Ver el ejemplo de API completo en `crates/swarm-core/src/lib.rs`.

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

## Diseño clave

Los agentes viven en un `AgentSet` dentro del modelo. Para ejecutar
`Agent::step(&mut self, id, &mut Model, &mut SimRng)` sin conflicto de
préstamos, el runner usa el patrón **take-out**: saca al agente del set,
corre su step con acceso mutable al modelo completo, y lo devuelve.

## Roadmap (v0.2)

- Activación simultánea (dos fases) en el scheduler.
- Grafos/redes como espacio; batch runs y barrido de parámetros (Rayon).
- Bindings PyO3 y visor WASM.
- Reescritura de `debris-flow-abm` sobre el motor.

## Licencia

MIT OR Apache-2.0
