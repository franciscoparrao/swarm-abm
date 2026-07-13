# Comparación con el simulador C++ mandatado (sim2-agricultores)

> **Contexto.** Para *escalar* el problema de las ovejas depredadas (el modelo
> SIGRID: depredación de ganado en Isla Riesco) se propuso reimplementarlo en
> C++, reutilizando el codebase [`ManachoM/sim2-agricultores`](https://github.com/ManachoM/sim2-agricultores).
> Este documento compara ese codebase contra swarm-abm en los ejes que importan
> para *escalar* un ABM (calibración, análisis de sensibilidad, reproducibilidad),
> con una demostración **en vivo** de la diferencia decisiva. No es un ataque al
> código C++ —es una tesis sólida en su dominio— sino un análisis de idoneidad
> para esta tarea concreta.

## 1. El codebase C++ no es el modelo de ovejas — es otro dominio y otro paradigma

`sim2-agricultores` (~5.160 líneas de C++) modela **mercados agrícolas**:
agricultores, feriantes, consumidores, mercado mayorista, terrenos y productos,
con eventos de sequía/helada/ola de calor. Es la tesis de otro estudiante, y su
arquitectura es de **simulación por eventos discretos (DES)**: una *future event
list* (`heap_fel`), event handlers y colas de mensajes.

El modelo de ovejas es lo contrario: **espacial y por pasos** (grilla,
depredador–presa evaluado por tick). Dos consecuencias:

- **No hay primitivas espaciales** en el codebase C++ (grilla, vecindades,
  difusión). Reimplementar SIGRID ahí implica construir toda esa infraestructura
  desde cero — justo lo que swarm-abm ya provee como núcleo.
- **El paradigma no calza**: un DES orientado a transacciones de mercado no
  expresa naturalmente la dinámica espacial sincrónica de depredación. Se puede
  forzar, pero se estaría peleando contra el diseño del framework.

## 2. Demostración en vivo: el C++ mandatado NO es reproducible

Lo más importante para *escalar* (calibrar, hacer análisis de sensibilidad) es la
**reproducibilidad**: mismo input → mismo output. Sin ella, no se puede calibrar
de forma auditable, no se pueden usar *common random numbers* en un GSA, y un bug
no se puede reproducir para depurarlo.

Se compiló y corrió el simulador C++ **dos veces con configuración idéntica**
(`sim_config.json` sin cambios), en un contenedor Docker autocontenido
(PostgreSQL 16 + g++ 13 + libpqxx; ver `validation/cpp-repro/`). Se comparó el
vector completo de resultados agregados (`aggregated_product_results`) vía hash
MD5:

```
CORRIDA 1 — exit 0 — 1260 filas — hash 089cdd94c7695f779ee5ef932029c620
CORRIDA 2 — exit 0 — 1260 filas — hash 9f344b0f1e960c3dd78487eb87d1a3a1
                                        └─ distinto ─┘
65 de 1260 filas (mismas claves) tienen VALOR DISTINTO entre las dos corridas.

Ejemplo (proceso | tiempo | producto → valor corrida A vs B):
  COMPRA DE FERIANTE A AGRICULTOR | 16 | 18 →  36 360  vs  28 620   (−21%)
  COMPRA DE FERIANTE A AGRICULTOR | 10 | 19 → 176 400  vs 182 700
  COMPRA DE CONSUMIDOR            | 15 | 19 →  27 200  vs  25 560

VEREDICTO: NO REPRODUCIBLE.
```

**Por qué.** Cada fuente de aleatoriedad del código hace
`std::random_device rd; std::mt19937 gen(rd());` — se siembra desde la entropía
del sistema operativo, **sin semilla fija en ninguna parte del API** (aparece en
al menos 8 archivos: `consumidor_factory`, `agricultor_riesgo`,
`product_partitioner`, `terreno`, `feriante_estatico`, …). Un caso incluso hace
`rd() + id*10`. No hay forma, con el código actual, de forzar dos corridas
idénticas.

> *En rigor*: para estadísticas agregadas de mercado por Monte Carlo (correr N
> veces y promediar), la no-reproducibilidad per-corrida es una decisión de
> diseño legítima. Pero es una **limitación** para calibración y análisis de
> sensibilidad, que es exactamente lo que "escalar el problema" requiere. Es
> arreglable en principio (cablear una semilla), pero hoy no está, y hacerlo bien
> —determinismo bajo paralelismo, por-agente— es justamente el trabajo de
> ingeniería que swarm-abm ya resolvió y verifica en CI.

## 3. Demostración en vivo: swarm-abm SÍ es reproducible

El mismo experimento sobre el modelo de ovejas real en swarm-abm (binario
`sigrid`, dos corridas con `--seed 1000 --seeds 5 --days 30`, desde un árbol
limpio en HEAD):

```
CORRIDA 1 — hash de la salida (loss_rate + conteos de matanza): faa5b35e733da2cb6835ac2cf6559f0f
CORRIDA 2 — hash de la salida:                                  faa5b35e733da2cb6835ac2cf6559f0f
                                                                └──────── idéntico ────────┘
VEREDICTO: REPRODUCIBLE BIT A BIT.
```

Y no es un accidente de una corrida: el determinismo de swarm-abm está
**garantizado por construcción y verificado en CI** (RNG ChaCha8 sembrable,
semilla por-agente `child_rng(seed, step, agent)`, ejecución paralela
bit-idéntica a la secuencial, identidad x86-64 ↔ wasm32, stream validado con
PractRand). Ver `RNG_AND_DETERMINISM.md` y `../docs/REPRODUCIBILITY.md`.

## 4. Matriz de idoneidad para escalar el problema de las ovejas

| Eje (lo que "escalar" exige) | sim2-agricultores (C++) | swarm-abm (Rust) |
|---|---|---|
| Reproducibilidad bit a bit | ❌ no (demostrado en vivo) | ✅ sí (demostrado en vivo + CI) |
| Ya implementa el modelo de ovejas | ❌ no (es mercados agrícolas) | ✅ SIGRID, paridad vs Mesa (Pearson 0.966) |
| Primitivas espaciales (grilla, vecindades) | ❌ no (DES no espacial) | ✅ tres espacios (grilla/grafo/continuo) |
| Análisis de sensibilidad global nativo | ❌ hay que construirlo | ✅ `experiment` (Sobol/Morris/LHS + bootstrap) |
| Calibración | ❌ manual | ✅ demostrada (DE, ~400× vs Python en debris-flow) |
| Determinismo bajo paralelismo | ❌ n/a | ✅ `decide` paralelo == secuencial |
| Dependencias para correr | PostgreSQL + libpqxx | ninguna (self-contained; + target WASM) |
| Estado del código | tesis; compila con `-fpermissive -w` | publicado en crates.io, 3 auditorías, tests + CI |
| Paradigma | eventos discretos (mercados) | pasos, espacial (depredador–presa) |

## 5. Sobre el rendimiento (honesto)

**No se puede comparar la velocidad de los dos directamente**: son modelos y
paradigmas distintos (un DES de mercados vs un ABM espacial de depredación).
Cronometrar uno contra el otro no mediría nada. Para referencia, el C++ corre su
config chica (23 agricultores) en ~2,6 s escribiendo a Postgres.

Lo que sí está medido, con rigor, es que **swarm-abm no sacrifica rendimiento por
tener el determinismo**: es 45–184× más rápido que Mesa (Python) y está a solo
~1,5–2,6× del motor nativo más rápido que existe (krABMaga, Rust) — ver
`CROSS_ENGINE.md` y `KRABMAGA.md`. Es decir, el argumento "hay que ir a C++ por
velocidad" no se sostiene: un motor en Rust ya está en la banda de rendimiento
nativo **y además** es reproducible, tiene análisis de sensibilidad nativo, y ya
implementa el modelo.

## 6. Conclusión para la decisión

**No se trata de que sea "imposible" usar el código C++** — sería un exceso
afirmarlo, y conviene ser preciso porque hay dos cosas distintas en juego:

1. **La no-reproducibilidad es un defecto arreglable, no una barrera.** El
   patrón `std::mt19937(std::random_device{}())` se puede reemplazar cableando
   una semilla fija a través del código. Es factible *en principio*. Pero es
   **trabajo de ingeniería real**: hilar la semilla por 8+ archivos con RNG
   por-objeto y, si se paraleliza, garantizar el determinismo bajo paralelismo —
   que es precisamente el problema difícil que swarm-abm ya resolvió y verifica
   en CI.

2. **El obstáculo grande no es el seed — es que el código no es el modelo ni el
   paradigma.** `sim2-agricultores` es un DES de mercados agrícolas sin
   primitivas espaciales. Usarlo para las ovejas no es "agregar un seed": es
   **reimplementar desde cero el modelo espacial depredador–presa** dentro de un
   framework que no fue diseñado para eso, y *además* arreglarle la
   reproducibilidad.

Por eso el encuadre correcto no es "se puede o no se puede", sino **esfuerzo y
riesgo frente a reutilizar lo ya hecho**:

> Usar este C++ para escalar el problema significa reconstruir en C++ —con menos
> garantías y semanas de trabajo— el motor espacial, el diseño de experimentos
> (análisis de sensibilidad, calibración) y la garantía de reproducibilidad que
> swarm-abm **ya** provee, tiene auditado (tres pasadas) y publicado en
> crates.io. El modelo de ovejas ya corre ahí (SIGRID), validado contra Mesa
> (Pearson 0,966) y reproducible bit a bit (verificado en vivo y en CI), y fue
> con su análisis de sensibilidad nativo que se identificó a `n_dogs` como la
> palanca de manejo.

La demostración en vivo de la sección 2 no depende de ninguna palabra fuerte:
muestra un hecho verificable —el código *tal como está hoy* no reproduce sus
propias corridas— y deja que la decisión de esfuerzo/riesgo se tome sobre esa
base.

## Reproducir esta comparación

```
# Lado C++ (contenedor autocontenido con Postgres):
git clone https://github.com/ManachoM/sim2-agricultores
cp validation/cpp-repro/{Dockerfile.repro,run_demo.sh} sim2-agricultores/
cd sim2-agricultores
docker build -f Dockerfile.repro -t agro-sim-repro .
docker run --rm -v "$PWD/run_demo.sh:/run_demo.sh:ro" agro-sim-repro bash /run_demo.sh

# Lado swarm-abm:
cargo build --release -p sigrid --bin sigrid
for i in 1 2; do ./target/release/sigrid --days 30 --seed 1000 --seeds 5 | grep -v ms | md5sum; done
# los dos hashes son idénticos
```
