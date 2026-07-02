# Auditoría de ingeniería del motor (swarm-core) — 2026-07-01

> **Alcance.** Lectura completa del código de `crates/swarm-core` (~2.400 líneas:
> `agent`, `sim`, `model`, `schedule`, `rng`, `grid`, `graph`, `continuous`,
> `data`, `batch`), más los bindings (`swarm-py`, `swarm-wasm`) y contraste con
> `docs/SOTA.md`. **Complementa** al SOTA: aquél es la brecha *de features*
> frente al campo; esto es la auditoría *de código* — corrección, arquitectura,
> rendimiento y ecosistema. Objetivo declarado: qué cambiar para que sea el
> mejor motor de ABM espacial existente.
>
> Un hallazgo (P0-1) fue **verificado empíricamente** con un test que se
> escribió, se corrió (falla) y se eliminó; la receta de reproducción está en
> el hallazgo.

## Estado de resolución

| Ítem | Estado | Fecha | Commit/nota |
|------|--------|-------|-------------|
| P0-1 | ✅ **Resuelto** | 2026-07-02 | `HashSet`→`Vec` en `barabasi_albert` + test de regresión (20 semillas) en `graph.rs` |
| P0-2 | ✅ **Resuelto** | 2026-07-02 | Primitivas propias (`uniform_below`/`uniform_usize`/`uniform_f64`/`bernoulli`/`shuffle`) en `rng.rs`, migradas en `swarm-core` + `swarm-models`; 7 tests de valores dorados en `tests/golden_values.rs`. De paso, expuso y corrigió un segundo bug real en `watts_strogatz` (pérdida de arista por colisión) — ver nota abajo. **Cambia los resultados numéricos exactos de todo lo ya corrido con el motor** (Sobol de SIGRID incluido); ver advertencia al final de esta entrada. |
| P0-3 | ✅ **Resuelto** | 2026-07-02 | `child_rng` de XOR de tres hashes → cadena secuencial (hash-combine) en `rng.rs`. Golden value de `child_rng` re-pinneado (los otros 6 no cambiaron, como se esperaba). Misma advertencia de impacto que P0-2: cambia otra vez los resultados numéricos exactos — se hizo en la misma ventana para no pagar el re-pinneo dos veces. |
| P0-4 | ✅ **Resuelto** | 2026-07-02 | `ContinuousSpace::wrap` clampea a `next_down(width/height)` en vez de `width/height` — dominio `[0,width)` ya no deja pasar el borde. Test de regresión en `continuous.rs`. No cambia resultados de corridas existentes salvo que un punto tocara exactamente el borde (caso raro). |
| P0-5 | ✅ **Resuelto** | 2026-07-02 | `Simulation` recolecta el paso 0 la primera vez que se invoca `step`/`run`/`step_parallel`/`run_parallel` (flag `initial_collected`), no solo si `run()` es la primera llamada. Dos tests de regresión en `sim.rs`. |
| P1-3 | ✅ **Resuelto (Fase 1)** | 2026-07-02 | Nuevo crate `swarm-derive` con `#[proc_macro_derive(MultiAgent)]`; re-exportado en el `prelude`. 5 tests `trybuild` (entradas inválidas) + ejemplo `examples/multi-agent` (depredador-presa, 2 tests incl. verificación directa del despacho). Fase 2 (SoA) no implementada — condicionada a evidencia de profiling, no urgente. |
| P1-1 | ✅ **Resuelto** | 2026-07-02 | Arena generacional en `AgentSet` (`AgentId = {index: u32, generation: u32}` + free-list LIFO). Incluye el bonus (self-remove durante el propio `step`). 6 tests en `agent.rs` (3 nuevos). API pública casi no se movió — ningún modelo/ejemplo necesitó cambios. **Afecta los resultados numéricos de modelos con demografía** (SIGRID inserta agentes en runtime); se suma a la lista de re-validación de P0-2/P0-3, no abre ventana nueva. |
| P1-2 | ✅ **Resuelto — diseño aditivo, no el "cambio de trait" del borrador** | 2026-07-02 | `Agent::decide_with_peers` (no-op por defecto) + `Simulation::step_with_peers`/`run_with_peers` en impl block separado (bound `M::Agent: Clone`, mismo patrón que `step_parallel`). Variante paralela incluida. 4 tests en `tests/decide_with_peers.rs`. **Cero cambios** en modelos existentes — ver la nota de por qué el diseño del borrador (cambiar la firma de `decide`) habría roto 6+ crates sin necesidad por monomorphización. |
| P1-4 | ✅ **Resuelto** | 2026-07-02 | `ContinuousSpace` reescrito sobre el mismo diseño de arena que `AgentSet` (P1-1): `PointId={index,generation}` + free-list ⇒ `remove` real; buckets planos (`Vec<u32>` de offsets + `Vec<u32>` contiguo, *counting sort*) reemplazan `Vec<Vec<PointId>>`; `for_each_within` sin `HashSet` (offsets de fila/columna acotados a `rows`/`cols` en el caso toroidal, sin duplicados por construcción). 11 tests (5 nuevos). `boids` (único consumidor real fuera de SIGRID) verificado end-to-end: Vicsek 0.019→0.957. **Sin impacto en resultados existentes** (a diferencia de P0-2/P0-3/P1-1). |
| P1-5 | ✅ **Resuelto** | 2026-07-02 | `NeighborsR` (iterador perezoso sin buffer fijo) + `neighbor_positions_r`/`neighbors_r`/`random_neighbor_r` (radio arbitrario en `Grid2D`, muestreo por reservorio). 6 tests nuevos — uno detectó un bug real de exclusión de la celda propia con radio grande en torus chico, corregido en el momento. API aditiva, sin impacto en resultados existentes. |
| P1-9 | ✅ **Resuelto** | 2026-07-02 | `Graph<T>` → `Graph<N, E=()>` (pesos por arista) + `directed(bool)` (builder, no un `DiGraph` separado — los 5 generadores canónicos son inherentemente no dirigidos por definición). Único cambio de firma pública: `neighbors()` de `&[NodeId]` a `impl Iterator` — afectó 1 call site (`network-sir`) + `golden_values.rs`, ambos actualizados. 2 tests nuevos. `network-sir` verificado end-to-end. |
| P1-6 | ✅ **Resuelto (2 de 3 puntos)** | 2026-07-02 | `AgentDataCollector<M: Model>` (reporters `Fn(AgentId,&A)->f64`) + `Simulation::with_collect_every(k)` (frecuencia compartida entre ambos colectores). Export Parquet/Arrow diferido — decisión de dependencia que le corresponde al usuario, no a la auditoría. 5 tests nuevos. |
| P1-8 | ✅ **Resuelto** | 2026-07-02 | `Activation::Staged(n)` + `Agent::stage(stage, id, model:&mut M, rng)` — modelo mutable en cada etapa (a diferencia de `decide`), `n` barridos completos sin re-mezclar entre etapas. Cableado en las 4 variantes de `step`. 4 tests, el principal verifica la garantía de barrido completo (no solo que las etapas corran en orden). |
| P1-7 | ✅ **Resuelto (con acotación de alcance)** | 2026-07-02 | Feature `serde` opcional en `swarm-core`: deriva Serialize/Deserialize en `Pos/Grid2D/NodeId/Graph/Vec2/PointId/ContinuousSpace/AgentId/AgentSet` + `rand_chacha/serde` para el RNG. `DataCollector` deliberadamente NO serializable (reporters son closures) — reformulado como `Simulation::from_checkpoint(model,seed,rng,steps_done)`. 2 tests: el central serializa/deserializa a mitad de una corrida y confirma huella bit-idéntica a la corrida sin interrumpir. Verificado con y sin la feature. **Con esto, los 9 ítems de P1 quedan resueltos** (P1-6 con una pieza diferida deliberadamente). |
| P3-4 | ✅ **Resuelto — el ítem más importante del audit** | 2026-07-02 | Nuevo módulo `swarm_abm::experiment` (feature opcional): `sobol()`/`SobolDesign::run` (Saltelli 2010 S1 + Jansen 1999 ST, bootstrap 95% CI, depende de la crate `sobol` — decisión documentada de por qué esto no repite P0-2), `latin_hypercube()` (propio, sin dependencia), `morris()`/`MorrisDesign::run` (efectos elementales). **Validado contra la función de Ishigami** (índices S1/ST de forma cerrada conocida): los 4 valores coinciden dentro de tolerancia 0.05, incluida la firma cualitativa (S1[x3]≈0, ST[x3] sustancial por interacción) — la única forma real de descartar un error de signo/índice silencioso en la fórmula. 4 tests. Internaliza el arnés híbrido de SIGRID (SALib+Rust) sin Python. Cierra el diferenciador central del wedge para el paper. |
| P3-1 | ✅ **Resuelto** | 2026-07-02 | Los 12 archivos de `swarm-core` + `swarm-derive` + 2 `README.md` traducidos al inglés (doc comments públicos, comentarios internos, mensajes de `panic!`/error de macro). Nombres de test en español dejados deliberadamente. `cargo doc` sin warnings, workspace en verde con y sin features. |
| P3-2 | ✅ **Resuelto** | 2026-07-02 | Job `golden-values-wasm32` en CI: valores dorados corridos en `wasm32-wasip1` vía `wasmtime`, verificado localmente antes de comprometerlo (bit a bit idéntico a x86-64). Hallazgo colateral corregido: `criterion` acotado a `cfg(not(target_family = "wasm"))` en dev-dependencies (bloqueaba la compilación para wasm). |
| P3-3 | ✅ **Resuelto — publicado en crates.io** | 2026-07-02 | `docs/REPRODUCIBILITY.md`, MSRV real determinado empíricamente (1.87.0), job de CI que lo fija, `CHANGELOG.md` con sección "Rompe determinismo", metadatos de `Cargo.toml` listos. Los crates se renombraron `swarm-core`→`swarm-abm` y `swarm-derive`→`swarm-abm-derive` (nombre elegido por el usuario, coincide con el repo) tras el choque real con terceros en crates.io. Todo el workspace verificado en verde tras el rename. **`swarm-abm-derive` v0.3.0 y `swarm-abm` v0.3.0 publicados y confirmados en crates.io** (con autorización explícita del usuario para cada uno, en ese orden — `swarm-abm` depende de que `swarm-abm-derive` ya estuviera indexado). |

**Cierre (2026-07-02): los 18 ítems de esta auditoría (5 P0 + 9 P1 + P3-1/2/3/4)
quedan resueltos.** El motor está publicado en crates.io como `swarm-abm` /
`swarm-abm-derive` — cualquiera fuera de este repositorio puede
`cargo add swarm-abm` hoy. Ver `docs/REPRODUCIBILITY.md` para la política de
estabilidad hacia adelante (qué versión bumpea qué tipo de cambio) y la
sección "Veredicto general" original más abajo para el diagnóstico *inicial*
que motivó todo este trabajo (se conserva sin editar como registro histórico
de dónde se partió).

## Veredicto general

El motor es pequeño, limpio y honesto: cero `unsafe`, `#![warn(missing_docs)]`
al día, tests con validación analítica (difusión al punto fijo, hash espacial
contra fuerza bruta), y el wedge (par == seq bit a bit vía `child_rng`) está
bien construido *en el hot path*. Pero el claim central — "misma semilla →
mismos resultados, siempre" — hoy **no es verdad en todo el API** (P0-1) y
**descansa en primitivas de una dependencia externa que no controla** (P0-2).
Para ser "el mejor", el determinismo tiene que dejar de ser una propiedad del
camino feliz y pasar a ser una **invariante verificada por CI en todo el API
público**. El resto de la distancia al título es arquitectura (agentes
heterogéneos, arena generacional, decide que vea agentes) y ecosistema (inglés,
CI, crates.io).

---

## P0 — Corrección: agujeros en el determinismo (arreglar antes del paper)

### P0-1. `Graph::barabasi_albert` NO es determinista ⚠️ VERIFICADO — ✅ RESUELTO (2026-07-02)

`graph.rs:286-296`: el muestreo de destinos usa
`std::collections::HashSet` y luego **itera sobre él**:

```rust
let mut targets = std::collections::HashSet::new();
while targets.len() < m { ... targets.insert(t); }
for &t in &targets {           // ← orden de iteración NO determinista
    g.add_edge(NodeId(new), NodeId(t));
    repeated.push(new);
    repeated.push(t);
}
```

El `HashSet` de std usa `RandomState` (semilla aleatoria por instancia), así
que el orden de iteración cambia **entre corridas del mismo programa con la
misma semilla del RNG**. El orden altera `repeated`, que es la distribución de
muestreo de los nodos siguientes → **el conjunto de aristas mismo diverge**,
no solo su orden.

**Reproducción** (test que se corrió y falla en la semilla 0):

```rust
let a = Graph::<()>::barabasi_albert(500, 3, &mut rng_from_seed(0));
let b = Graph::<()>::barabasi_albert(500, 3, &mut rng_from_seed(0));
assert_eq!(a, b);   // FALLA
```

Es exactamente la clase de bug que un revisor de SIMPAT buscaría dado el claim
del paper, y explica por qué el test existente (`barabasi_albert_es_scale_free`)
no lo pesca: es el único generador **sin** test de `g(seed) == g(seed)`
(Erdős–Rényi y Watts–Strogatz sí lo tienen, `graph.rs:337`, `graph.rs:348`).

**Fix.** Reemplazar el `HashSet` por un `Vec` con chequeo `contains` (m es
chico, O(m) es gratis) o muestreo con rechazo sobre un buffer ordenado.
Añadir el test de determinismo **para todos los generadores presentes y
futuros** (idealmente un helper genérico `assert_deterministic(|rng| ...)`).

**Regla derivada:** prohibir `std::collections::{HashSet, HashMap}` en
cualquier camino que toque resultados. Un test de lint barato: `grep` en CI, o
clippy `disallowed_types` en `clippy.toml` — convierte la regla en compilación.
(`continuous.rs:254` usa `HashSet` solo como guarda de visita de celdas — hoy
no afecta el resultado, pero es una mina a un refactor de distancia; cae con
P1-4 de todos modos.)

**✅ Resuelto (2026-07-02).** `targets` pasó de `HashSet<usize>` a
`Vec<usize>` con chequeo `contains` (m es chico, el `O(m)` lineal es gratis);
el orden de descubrimiento —ligado al draw del RNG— ahora es la única fuente
de orden. Se agregó `barabasi_albert_es_determinista` (`graph.rs`, 20
semillas, `g(seed) == g(seed)`) que replica exactamente la reproducción de
arriba y falla en el código viejo. Suite completa + clippy en verde tras el
fix. La regla de lint (`disallowed_types` para `HashSet`/`HashMap` en rutas de
resultado) queda pendiente — se abordará junto con P1-4, que también toca
`continuous.rs:254`.

### P0-2. La reproducibilidad depende de primitivas de `rand` que el motor no controla

El claim es "misma semilla → mismos resultados en cualquier plataforma y
versión" (`rng.rs:3-5`). Pero dos primitivas centrales son de `rand`, no del
motor:

- `SliceRandom::shuffle` — el corazón de `Activation::Random`
  (`schedule.rs:50`);
- `Rng::random_range` — usado en todo el motor y en todos los modelos.

`Cargo.toml` pide `rand = "0.9"` (caret): un `cargo update` dentro de 0.9.x es
seguro por la política de value-stability de rand, pero el salto 0.9 → 0.10
(inevitable en la vida del motor) **puede cambiar los algoritmos de shuffle y
de rango uniforme y silenciosamente invalidar toda reproducción de resultados
publicados**. Para un motor cuyo producto ES la reproducibilidad, delegar esas
primitivas es deuda estratégica.

**Fix (en orden de robustez):**
1. **Implementar en el crate** Fisher–Yates y el rango uniforme (Lemire) sobre
   el stream crudo de ChaCha8 (~40 líneas totales). El stream ChaCha8 en sí es
   una especificación criptográfica estable — ahí no hay riesgo.
2. **Tests de valores dorados**: fijar en el repo vectores esperados
   (primeros N draws de `rng_from_seed(k)` y de `child_rng`, una permutación
   de shuffle, un grafo generado) y compararlos en CI. Detecta cualquier
   deriva — de rand, del compilador o propia — en el momento en que ocurre.
   Hoy **ningún test protege contra esto**: todos comparan una corrida contra
   otra corrida del mismo binario.
3. Documentar la política de estabilidad de valores del motor (qué cambios la
   rompen, cómo se versiona: "cambio de valores ⇒ major bump").

Esto convierte "reproducible" de convención en **garantía verificada** — y es
una frase fuerte para el paper: nadie en la matriz SOTA la tiene.

**✅ Resuelto (2026-07-02).** Las tres primeras acciones del fix, hechas:

1. **Primitivas propias en `rng.rs`**: `uniform_below`/`uniform_usize`
   (Lemire, sobre `RngCore::next_u64`), `uniform_f64` (53 bits altos
   escalados), `bernoulli`, `shuffle` (Fisher–Yates). Documentadas con la
   razón (independencia del algoritmo interno de `rand`). Migrados todos los
   call sites determinismo-críticos: `schedule.rs` (activación aleatoria),
   `grid.rs` (`random_neighbor`), `graph.rs` (los tres generadores) y los tres
   modelos canónicos de `swarm-models` (`sir`, `schelling`, `sugarscape`).
   `rand::Rng`/`SliceRandom` se mantienen en el `prelude` (rangos flotantes
   arbitrarios y otros usos no cubiertos), pero documentados como *no
   preferidos* para lo que las nuevas primitivas sí cubren.
2. **7 tests de valores dorados** en `crates/swarm-core/tests/golden_values.rs`:
   stream crudo de ChaCha8, `child_rng`, `uniform_below`, `uniform_f64`,
   `bernoulli`, `shuffle`, y un grafo Erdős–Rényi completo. Si una futura
   actualización de `rand`/`rand_chacha` cambia cualquiera de estos valores,
   CI lo detecta en el commit exacto que lo causó.
3. Política de estabilidad: pendiente como documento separado (`docs/
   REPRODUCIBILITY.md`, P3-3) — el mecanismo (golden tests) ya existe, falta
   la prosa que explique cómo versionar un cambio deliberado de valores.

**Hallazgo colateral: un segundo bug real.** Al migrar `watts_strogatz`, el
cambio de algoritmo de muestreo hizo aparecer un test que fallaba
(`watts_strogatz_conserva_numero_de_aristas`, semilla 1: 599 aristas en vez de
600). Investigado: **no era un efecto cosmético del cambio de primitiva**, sino
un bug preexistente. El código daba por hecho que `g.add_edge(a, b)` en la
rama "no recablear" y en el respaldo "me rindo, uso la arista original"
siempre tenía éxito — pero si un recableo *anterior* del mismo nodo `a` (otro
`d` procesado antes) ya había conectado `a` con ese destino por coincidencia,
`add_edge` fallaba en silencio y la iteración perdía su arista sin que nada lo
notara. Con el muestreo de `rand` viejo esto nunca se manifestó en los seeds
probados; con Lemire sí, en la primera semilla que se probó. **Fix**: función
compartida `destino_recableo_valido` que, ante cualquier colisión (recablear o
no), reintenta hasta encontrar un destino realmente libre, con un respaldo
determinista (recorrido lineal) que garantiza éxito salvo que el nodo ya esté
conectado a todos los demás — caso que ahora entra en pánico explícito en vez
de corromper el grafo en silencio (documentado en el `# Panics` de
`watts_strogatz`, y solo alcanzable con `2k` patológicamente cerca de `n`, no
en uso normal de un modelo *small-world*). Test de regresión ampliado:
`watts_strogatz_conserva_aristas_en_muchos_seeds_y_parametros` (5
combinaciones de `(n,k)` × 5 `beta` × 10 semillas = 250 grafos verificados).

**⚠️ Impacto en trabajo ya corrido.** Como estaba previsto en este mismo
documento ("la ventana para hacerlo es ahora, antes del paper y de v1.0"),
este cambio **altera los resultados numéricos exactos** de cualquier corrida
previa del motor con la misma semilla — incluida la validación de paridad de
SIGRID (`models/sigrid/PARITY.md`: Pearson 0.97 / RMSE 10.1pp) y el Sobol
N=512 ya corrido, que usan `Schedule`/`Grid2D::random_neighbor` del motor
internamente aunque `models/sigrid` no fue tocado directamente. La suite de
tests de SIGRID (`determinismo_misma_semilla`, `semillas_distintas_difieren`)
sigue en verde — la propiedad "misma semilla ⇒ mismo resultado" se conserva,
solo cambia CUÁL es ese resultado. **Antes de citar esos números en el paper
hay que re-correr la validación de paridad y el Sobol** con el motor
post-P0-2.

**✅ Re-validación hecha (2026-07-02)** — ver `models/sigrid/PARITY.md`,
sección "Re-validación 2026-07-02": paridad (Pearson 0.966→0.963, Spearman
0.902→**0.916**, mejoró) y Sobol N=512/30d (`n_dogs` ST 0.93→**1.04**, el
hallazgo central se refuerza) robustos al cambio de RNG del motor. Ninguna
conclusión científica cambió; solo los números exactos, como estaba
previsto.

`models/sigrid` y `models/debris-flow` se dejaron deliberadamente
sin migrar a las primitivas propias (siguen usando `rand::Rng` directo vía
prelude) porque son artefactos de investigación con resultados ya publicados
en documentos propios (`PARITY.md`, `CALIBRATION.md`,
`BENCHMARK_OPTIM.md`); no se tocó su código para no ensanchar el radio de
impacto más allá de lo que el cambio del motor ya fuerza.

### P0-3. `child_rng` combina con XOR: estructura algebraica evitable

`rng.rs:43-48` deriva la semilla hija como
`mix64(seed+C1) ^ mix64(step+C2) ^ mix64(agent+C3)` y re-mezcla. El XOR de
tres hashes independientes admite cancelaciones estructurales (existen familias
`(seed,step,agent) ≠ (seed',step',agent')` con la misma salida; con mix64 de
por medio no son *construibles* fácilmente, pero tampoco hay argumento de
inyectividad). La construcción canónica es la **cadena secuencial** estilo
SplitMix/hash-combine:

```rust
let mut s = mix64(seed ^ 0x9E37_79B9_7F4A_7C15);
s = mix64(s ^ step);
s = mix64(s ^ agent);
```

que compone permutaciones (biyectiva en cada eslabón dado el anterior). Costo
idéntico. **Cambiarlo rompe la compatibilidad de resultados con todo lo ya
corrido** (Sobol de SIGRID incluido), así que la ventana para hacerlo es
**ahora, antes del paper y de v1.0** — junto con P0-2, que igual re-fija los
valores dorados. Ya hay validación PractRand del esquema actual
(`examples/rng-dump`); repetirla tras el cambio.

**✅ Resuelto (2026-07-02).** `child_rng` implementado exactamente como el
snippet de arriba (cadena secuencial en vez de XOR de tres hashes
independientes). Impacto verificado en `tests/golden_values.rs`: el golden de
`child_rng_primeros_draws` cambió (re-pinneado con los valores nuevos); los
otros 6 golden tests (stream crudo, `uniform_below`, `uniform_f64`,
`bernoulli`, `shuffle`, `erdos_renyi`) **no** cambiaron, confirmando que el
cambio quedó contenido a `child_rng` y no tuvo efectos colaterales en el resto
del motor. `decide_paralelo_es_bit_identico_al_secuencial`
(`tests/parallel_decide.rs`) sigue pasando: la cadena nueva conserva la
propiedad central del wedge (par == seq bit a bit), ya que el argumento no
depende de la construcción interna de `child_rng` sino de que sea función pura
de `(seed, step, agent)`. Workspace completo (17 crates) verde: build, clippy,
tests. Validación PractRand de `examples/rng-dump` **pendiente** — no había un
binario `RNG_test`/PractRand disponible en este entorno para repetirla; queda
para cuando el usuario la corra con acceso a esa herramienta (el generador
`interagent` de `rng-dump` ya apunta a `child_rng`, así que exercita
automáticamente la construcción nueva sin cambios).

**Mismo aviso que P0-2**: este cambio también altera los resultados numéricos
exactos de todo lo corrido con el motor hasta ahora (incluida SIGRID). Se hizo
en la misma ventana de ruptura que P0-2 para no pagar el costo de re-pinnear
los valores dorados dos veces — ver la advertencia de impacto detallada en la
entrada de P0-2 arriba; aplica igual aquí y no se repite dos veces.

### P0-4. `ContinuousSpace::wrap` sin torus produce posiciones fuera de dominio

`continuous.rs:239`: el modo no-toroidal hace `p.x.clamp(0.0, self.width)` —
**incluye** `width`, pero el dominio declarado es `[0, width)`
(`continuous.rs:106`). Un punto clampeado al borde queda fuera del invariante;
`cell_coords` lo salva con `min(cols-1)`, pero `distance`/`delta` y cualquier
código de usuario que asuma el semiabierto opera con un valor ilegal.
**Fix:** clamp a `f64::next_down(width)` (Rust ≥ 1.86 lo tiene estable) o
documentar el dominio como cerrado. Trivial, pero es exactamente el tipo de
esquina que un test de propiedad (P3-6) encontraría solo.

**✅ Resuelto (2026-07-02).** `wrap` clampea a `self.width.next_down()` /
`self.height.next_down()` (rustc 1.94, `f64::next_down` estable desde 1.86).
Test de regresión `wrap_sin_torus_respeta_dominio_semiabierto` en
`continuous.rs`: puntos exactamente en el borde o muy fuera de rango, en
ambos ejes, quedan siempre estrictamente dentro de `[0,width) × [0,height)`.
Impacto en resultados existentes: nulo salvo el caso borde de un punto que
tocara *exactamente* `width` o `height` (con `f64`, solo ocurre si el usuario
lo puso ahí a propósito o tras una operación que da justo ese valor — muy
raro en la práctica), así que no se agrega a la lista de "hay que re-correr
la validación" de P0-2/P0-3.

### P0-5. Paso 0 perdido si se mezcla `step()` con `run()`

`sim.rs:135-137`: `run()` recolecta el estado inicial solo si el colector está
vacío. Si el usuario llama `sim.step()` manualmente y después `run()`, la
primera fila recolectada corresponde al paso actual, no al 0, y el eje de
`steps()` arranca desfasado sin aviso. Menor, pero sorprende. **Fix:** mover la
recolección del estado inicial a la construcción (o al primer `step()`), de
modo que la semántica no dependa de por dónde se entró.

**✅ Resuelto (2026-07-02).** No se pudo mover la recolección a la
construcción (`Simulation::new`) porque los reporters se agregan *después*
con `add_reporter` (patrón usado en `swarm-py`, `swarm-wasm` y todos los
ejemplos) — recolectar en `new()` correría con cero reporters registrados. En
su lugar: un flag `initial_collected: bool`, revisado por un método
compartido `collect_initial_if_needed()` invocado al principio de `step`,
`run`, `step_parallel` y `run_parallel` por igual. Dos tests de regresión en
`sim.rs`: `step_manual_antes_de_run_no_pierde_el_paso_0` (reproduce
exactamente el escenario del bug) y `run_dos_veces_no_duplica_el_paso_0`
(la otra esquina: que el flag no permita duplicar la fila del paso 0 si
`run()` se llama más de una vez). No cambia ningún resultado de una corrida
que ya empezaba por `run()` (el patrón usado en todo el motor y en SIGRID),
así que tampoco se suma a la lista de "hay que re-correr" de P0-2/P0-3.

---

## P1 — Arquitectura: lo que separa el MVP del "mejor motor"

### P1-1. `AgentSet` nunca reutiliza slots: degradación en modelos con demografía

`agent.rs:69-70`: los `remove` dejan `None` para siempre. En modelos con
natalidad/mortalidad sostenida (Sugarscape con reproducción, SIGRID a horizonte
largo, cualquier modelo ecológico), `slots` crece **con el total histórico de
agentes**, no con los vivos: memoria O(históricos) e iteración que recorre
cadáveres (`iter`, `collect_ids_into`, `decide_all` visitan cada slot muerto).
Una simulación de 10⁶ pasos con recambio poblacional se arrastra.

**Fix:** arena **generacional** (estilo `slotmap`): `AgentId = (índice: u32,
generación: u32)` — mismo tamaño que el `usize` actual. Reúso de slots sin ABA
(un id viejo nunca resuelve a un agente nuevo), iteración O(vivos) con
free-list, y **el orden de iteración sigue siendo determinista** (definirlo
como orden de índice, documentando que un slot reusado hereda la posición).
Bonus: elimina la limitación documentada "un agente no puede eliminarse a sí
mismo" (`agent.rs:72-74`) si `remove` durante el take-out marca el slot en la
free-list al `put_back`. Es un cambio de tipo interno; el API público apenas se
mueve.

**✅ Resuelto (2026-07-02).** Implementado exactamente como se diseñó,
incluido el bonus:

- `AgentId` pasa de `usize` a `{ index: u32, generation: u32 }` — mismo
  tamaño en 64-bit, el doble en wasm32 (32→64 bits), aceptado como
  compensación razonable. `as_usize()` se conserva pero su rustdoc ahora
  advierte que el índice puede reutilizarse tras un `remove` (ya no es clave
  estable a largo plazo; el `AgentId` completo sí lo es, vía `Eq`/`Hash`).
- `AgentSet<A>` interno pasa de `Vec<Option<A>>` a `Vec<Slot<A>>` con
  `SlotState::{Occupied, TakenOut, Free}` y una free-list LIFO
  (`free_head: Option<u32>`). `insert` reutiliza el slot libre más
  reciente si hay uno; si no, crece el `Vec`. `remove`/`get`/`get_mut`
  verifican la generación antes de resolver — un `AgentId` emitido antes de
  un `remove` nunca vuelve a resolver a nada, ni al agente original (ya no
  existe) ni al que reutilice su slot (generación distinta): problema ABA
  evitado por construcción.
- **Self-remove** (el bonus): `SlotState::TakenOut { pending_removal }`
  permite que un agente llame a `model.agents_mut().remove(su_propio_id)`
  **durante** su propio `step`/`apply`. Como el valor está prestado en ese
  momento (patrón take-out), `remove` no puede devolverlo (devuelve `None`)
  pero sí decrementa `len()` y marca el slot; `put_back` —invocado por
  `sim.rs` al terminar la fase— libera el slot de verdad y descarta el valor
  que intentaba restaurar. La limitación documentada en el trait `Agent`
  queda obsoleta (se actualizó el rustdoc).
- **6 tests** en `agent.rs` (3 nuevos + 3 existentes adaptados): reutilización
  de slot invalida el id viejo, memoria acotada por el pico (1000 altas/bajas
  alternadas nunca crecen más de un slot), y el ciclo completo de self-remove
  (`remove` durante `take-out` → `put_back` libera, no restaura → el slot
  queda disponible para el siguiente `insert`).
- API pública casi no se mueve, como estaba previsto: `AgentId` sigue siendo
  `Copy`/`Eq`/`Hash`/`Ord`, `as_usize()` se mantiene. Ningún modelo de
  `swarm-models` ni ejemplo necesitó cambios — compilan sin tocar una línea.
- Workspace completo en verde: 51 tests en `swarm-core` (+3), clippy limpio,
  `cargo test --workspace` sin fallos (incluidos SIGRID y `multi-agent`).

**⚠️ Impacto en modelos con demografía.** Igual que P0-2/P0-3: esto cambia
los resultados numéricos exactos de cualquier modelo que **inserte agentes
después de la construcción inicial** (nacimientos/reproducción), porque el
índice de slot de un agente nuevo ahora puede reutilizar el de uno recién
eliminado, cambiando el `child_rng(seed, step, id)` que le toca. **SIGRID
inserta agentes durante la simulación** (6 sitios en `models/sigrid/src/
lib.rs`, lógica de nacimiento/reproducción) y por lo tanto queda afectado
— se suma a la lista de "hay que re-correr la validación" ya abierta por
P0-2/P0-3, no es una ventana de ruptura nueva. Los tres modelos canónicos de
`swarm-models` (SIR, Schelling, Sugarscape) **no** insertan agentes tras el
`build()` inicial (solo remueven, o ni eso), así que no les afecta.

### P1-2. La fase `decide` no puede observar a los otros agentes

`sim.rs:104`: `decide_phase` saca el `AgentSet` **completo** del modelo, así
que durante `decide` el modelo llega con el set vacío. Consecuencia: en
activación simultánea los agentes solo pueden observar el *entorno* (grilla,
espacio), y todo estado de agente que otros necesiten ver debe duplicarse a
mano en el entorno (Life lo hace vía la grilla; SIGRID duplicó posiciones).
Es una restricción semántica seria: el caso de uso arquetípico de la
activación simultánea — "todos deciden mirando el estado *anterior* de todos" —
requiere plomería manual del usuario.

**Fix:** doble buffer real de agentes: `decide` recibe además un
`snapshot: &AgentSet<A>` (el estado del paso anterior, clonado o intercambiado
al inicio de la fase). Firma: `fn decide(&mut self, id, model: &M, agents:
&AgentSet<A>, rng)`. Mantiene intacta la garantía del wedge (todo inmutable
durante la fase, paralelizable igual) y elimina la duplicación manual. Costo:
un clone del set por paso **solo** en activación simultánea — o cero si se
mantienen dos sets y se alternan (swap de punteros). Cambio de trait ⇒ hacerlo
antes de v1.0.

**✅ Resuelto (2026-07-02) — con una desviación deliberada del diseño
original.** El fix tal como está escrito arriba ("cambio de trait") tiene un
problema práctico que el borrador no consideró: **cambiar la firma de
`decide` en el trait `Agent` fuerza `M::Agent: Clone` en TODO modelo que use
`Simulation<M>`, no solo en los que usan activación simultánea con
snapshot.** La razón es monomorphización: `Simulation::step()` llama a la
fase `decide` simultánea dentro de una rama de un `match` en tiempo de
ejecución, pero el compilador tipa el CUERPO COMPLETO de `step()` en tiempo
de compilación para cada `M` concreto — si esa rama exige `A: Clone`, **todo**
`M::Agent` necesita `Clone`, incluidos SIR, Schelling, Sugarscape, boids,
network-sir y `multi-agent`, que nunca tocan esa rama. Habría roto 6+ crates
sin necesidad real.

**Diseño implementado en su lugar**: puramente **aditivo**, mismo patrón que
`step_parallel`/`run_parallel` (impl block separado con bounds extra, opt-in):

- `Agent::decide_with_peers(&mut self, id, model, peers: &AgentSet<Self>,
  rng)` — método nuevo del trait, no-op por defecto, **no reemplaza**
  `decide`. Implementarlo no afecta a quien no lo implementa.
- `AgentSet<A>` ahora deriva `Clone` (`A: Clone` ⇒ `AgentSet<A>: Clone`,
  trivial dado que `Slot`/`SlotState` son structs/enums simples).
- `Simulation<M>::step_with_peers`/`run_with_peers` — **impl block separado**
  con bound `M::Agent: Clone`, mismo patrón que la variante paralela. Toma
  una foto (`agents.clone()`) al empezar la fase, la pasa como `peers` a
  `decide_with_peers`; `step`/`run` normales **no cambian** — cero bound
  nuevo, cero recompilación de modelos existentes.
- `step_with_peers_parallel`/`run_with_peers_parallel` (feature `parallel`) —
  combina las dos garantías: `peers` compartido entre hilos (bound extra
  `A: Sync`) y paralelismo bit-idéntico al secuencial.
- **4 tests** en `tests/decide_with_peers.rs`: un modelo de agregación
  (`Node`, cada agente necesita la suma de `value` de *todos* los demás —
  un dato que ningún entorno espacial puede representar, la prueba de que
  `decide` a secas genuinamente no alcanza) confirma que (a) `peers` sí ve el
  estado de otros, (b) `decide` normal NO hereda esa visibilidad
  (independencia de los dos caminos), (c) determinismo, (d) paralelo
  bit-idéntico a secuencial.
- Workspace completo en verde: build, clippy, `cargo test --workspace` sin
  fallos. **Cero cambios** en SIR, Schelling, Sugarscape, boids, life,
  network-sir, `multi-agent`, SIGRID o debris-flow — el punto entero del
  diseño aditivo.

**Por qué esto sigue resolviendo P1-2 de verdad**: el problema real
("SIGRID duplicó posiciones a mano porque `decide` no veía a los demás
agentes") queda resuelto igual — cualquier modelo nuevo (o refactor futuro
de SIGRID) puede usar `decide_with_peers` sin la plomería manual. La única
diferencia con el diseño del borrador es que la capacidad es **opt-in a
nivel de tipo** en vez de universal — un tradeoff estrictamente mejor dado
el costo real (evita romper 6+ crates para una feature que la mayoría de
los modelos no necesita).

### P1-3. Heterogeneidad de agentes (la fricción nº 1, según SIGRID)

Ya priorizada como Tier-1 #2 en el SOTA; aquí la propuesta concreta de diseño,
porque hay una solución que conserva las dos propiedades que importan (layout
cache-friendly y determinismo):

- **No** trait objects (`Box<dyn Agent>`): mata el layout, el paralelismo
  tipado y el inlining.
- **Sí** enum + derive macro: el usuario escribe
  `#[derive(MultiAgent)] enum Critter { Fox(Fox), Dog(Dog), ... }` y el macro
  genera el dispatch estático de `decide/apply/step` por variante. Los campos
  muertos desaparecen (cada variante lleva solo lo suyo); el `AgentSet<Critter>`
  actual funciona sin tocar el motor.
- **Fase 2 (opcional, SoA)**: un `MultiAgentSet` generado con un set denso por
  variante y un orden de activación global determinista (índice global =
  (variante, índice local) con merge estable). Solo si el profiling lo pide.

El macro es un crate `swarm-derive` chico (~200 líneas) y es la mejora con
mayor ROI de adopción: es lo primero que golpea a cualquier modelo no-trivial.

**✅ Resuelto — Fase 1 (2026-07-02).** Implementado tal como se diseñó arriba,
sin desviaciones:

- Nuevo crate `crates/swarm-derive` (proc-macro, ~150 líneas):
  `#[proc_macro_derive(MultiAgent)]` valida que la entrada sea un `enum` con
  al menos una variante y que cada variante envuelva exactamente un tipo
  (`Variante(Tipo)`), y genera `impl Agent for Enum` con `decide`/`apply`/
  `step` despachando por `match` estático al tipo interno — cero
  `Box<dyn Agent>`, cero costo de vtable, el `AgentSet<Enum>` de siempre.
  `type Model` se toma de la primera variante (`<PrimerTipo as Agent>::Model`);
  si otra variante tiene un `Model` distinto, el `match` generado no
  tipa y el error del compilador señala el brazo en conflicto.
- Re-exportado desde `swarm_abm::prelude` (`swarm-abm` depende de
  `swarm-derive`), así que un modelo solo necesita `use
  swarm_abm::prelude::*;` para tener `#[derive(MultiAgent)]` disponible —
  no hace falta que el usuario agregue una dependencia nueva a su
  `Cargo.toml`.
- **5 tests `trybuild`** (`crates/swarm-derive/tests/`) verifican que las
  entradas inválidas (struct en vez de enum, enum vacío, variante unitaria,
  variante con campos nombrados, variante con dos campos) fallan en
  compilación con un mensaje que señala el problema — no un panic del macro
  ni un error de tipos incomprensible. No se fijan los `.stderr` exactos
  (frágil entre versiones de rustc): `trybuild` solo confirma que la
  compilación falla.
- **Ejemplo nuevo `examples/multi-agent`**: ecosistema depredador-presa
  mínimo (`Grazer` pasta y se reproduce, `Wolf` caza) con
  `#[derive(MultiAgent)] enum Critter { Grazer(Grazer), Wolf(Wolf) }` —
  exactamente el caso que SIGRID tuvo que resolver a mano con un `enum
  Species` y campos muertos. Dos tests: determinismo (misma semilla → mismo
  resultado) y una prueba directa de que el despacho generado por la macro
  invoca el método del tipo correcto (un `Grazer` con pasto de sobra debe
  *comer*, no moverse; si el `match` generado llamara al brazo equivocado,
  el test lo detecta por valores de energía/posición incorrectos).
- Workspace completo (19 crates ahora) en verde: build, clippy, `cargo test
  --workspace` sin fallos.

**Fase 2 (SoA / `MultiAgentSet`) no implementada** — quedó correctamente
marcada como "solo si el profiling lo pide" en el diseño original; no hay
señal de que el `AgentSet<Enum>` actual (con el enum como unidad de
almacenamiento) sea un cuello de botella real todavía. Queda en el backlog,
condicionada a evidencia de profiling futura, no a un cronograma fijo.

### P1-4. `ContinuousSpace`: el índice espacial necesita su v2

Cuatro problemas concretos, todos ya sufridos por SIGRID (SOTA los lista;
aquí el diseño):

1. **No hay `remove`** (`continuous.rs`): los puntos no pueden morir. SIGRID
   reconstruyó el espacio entero cada paso.
2. **`for_each_within` aloca un `HashSet` por consulta** (`continuous.rs:254`)
   — en el hot path de vecindad, el peor lugar posible. Con torus, las celdas
   duplicadas solo ocurren cuando el radio de búsqueda abarca la rejilla
   completa en algún eje: se elimina el set acotando `drow/dcol` a
   `min(2*cr+1, rows|cols)` celdas — sin duplicados por construcción, cero
   allocs.
3. **`reindex` reconstruye `Vec<Vec<PointId>>`** (`continuous.rs:196-203`):
   churn de allocs por bucket y saltos de cache. Layout plano estándar
   (counting sort): un `Vec<u32>` de offsets por celda + un `Vec<PointId>`
   contiguo. Reindexar es dos pasadas O(n) sin alloc (buffers reutilizados);
   consultar es un slice contiguo.
4. **`PointId` sin constructor estable** obliga a paralelos frágiles
   usuario-lado. Con la arena generacional (P1-1) el mismo tipo de handle
   sirve aquí.

Además: `within` (`continuous.rs:285`) aloca un `Vec` por llamada — con (2)
arreglado, deja de ser necesario en los hot paths y puede quedarse como API de
conveniencia.

**✅ Resuelto — los 4 puntos (2026-07-02).** Reescritura completa de
`continuous.rs` sobre el mismo diseño de arena que `AgentSet` (P1-1):

1. **`remove`**: `PointId` pasó a `{index: u32, generation: u32}` (mismo
   diseño que `AgentId`) sobre una arena `Vec<Slot<T>>` con free-list LIFO.
   `remove(id) -> Option<T>` libera el slot para un `add` futuro; un
   `PointId` emitido antes del `remove` nunca vuelve a resolver a nada
   (ABA evitado igual que en P1-1).
2. **`for_each_within` sin `HashSet`**: en el caso toroidal, el rango de
   offsets de fila/columna se acota a `min(2·cr+1, rows|cols)` — más allá de
   ese número el wrap necesariamente repetiría una celda ya visitada, así
   que iterar exactamente esa cantidad de offsets consecutivos (mod
   rows/cols) cubre todas las celdas relevantes **sin duplicados por
   construcción**, sin alocar nada para deduplicar. El caso no-toroidal ya
   era libre de duplicados (nunca los necesitó).
3. **Buckets planos**: `Vec<Vec<PointId>>` → dos `Vec<u32>` contiguos
   (`bucket_start` + `bucket_points`, layout de *counting sort*) más un
   buffer `cursor` de escritura. `reindex()` es ahora dos pasadas O(n) que
   reutilizan los tres buffers entre llamadas (crecen solo si crece la
   población o la grilla) — cero *heap churn* por celda, que es lo que el
   diseño anterior pagaba en cada `reindex` de cada paso.
4. **`PointId` con la arena generacional** de (1): mismo diseño que
   `AgentId`, así que un modelo puede guardar `PointId`s como claves
   estables (mientras el punto viva) sin los "paralelos frágiles" que
   forzaban a SIGRID a reconstruir el espacio entero cada paso
   (`*space = ContinuousSpace::new(...)` en `models/sigrid/src/lib.rs` —
   confirmado al revisar el código antes de diseñar el fix, exactamente el
   síntoma que motivó este ítem).
- **11 tests** en `continuous.rs` (5 nuevos: `remove` libera el slot e
  invalida el id viejo, un punto removido deja de aparecer en consultas
  tras `reindex`, `position` sobre un punto eliminado panica, y un test que
  fuerza el caso límite toroidal — radio de búsqueda mayor que todo el
  torus — verificando que cada punto se visita **exactamente una vez** sin
  el `HashSet` viejo).
- `within`/`Index`/`IndexMut`/`position`/`set_pos` mantienen su firma
  pública (siguen panicando ante un id inválido, mismo estilo que
  `Grid2D::Index`); solo `remove` y `point_ids` son API nueva.
- **`boids` (el único consumidor real de `ContinuousSpace` en el repo,
  fuera de SIGRID) verificado end-to-end**: `cargo run --release -p boids`
  reproduce el flocking emergente esperado — orden de Vicsek de 0.019
  (caótico) a 0.957 (bandada alineada) — confirmando que el reemplazo del
  índice espacial no rompió la física del modelo.
- Workspace completo en verde: build, clippy, `cargo test --workspace` sin
  fallos (incluidos `boids` y `sigrid`, los dos consumidores reales del
  tipo).

**Sin impacto en resultados existentes**: a diferencia de P0-2/P0-3/P1-1,
este cambio **no** altera ningún resultado numérico de corridas ya hechas —
`boids` no usa `remove` (población fija) y el índice espacial reordenado
internamente no cambia qué puntos caen dentro de un radio, solo cómo se
almacenan. No se suma a la lista de "hay que re-correr" de SIGRID.

### P1-5. Vecindades de grilla con radio fijo 1

`grid.rs:33` lo declara ("radio 1 en v0.1"). NetLogo vive de `in-radius`; Mesa
tiene `get_neighborhood(radius=r)`. Falta `neighbor_positions_r(pos, nh, r)` y
un iterador sin alloc equivalente a `Neighbors` (el buffer fijo de 8 ya no
sirve; usar un iterador perezoso sobre el rectángulo con filtro). También
`random_neighbor` con radio. Es API aditiva, sin riesgo.

**✅ Resuelto (2026-07-02).** `NeighborsR` (nuevo iterador perezoso, sin
buffer fijo — el número de vecinas escala con `r²`) + `neighbor_positions_r`/
`neighbors_r`/`random_neighbor_r`. En torus, mismo fix que P1-4/`for_each_within`:
el rango de offsets se acota a `min(2r+1, height|width)` para no visitar la
misma celda dos veces. `random_neighbor_r` usa **muestreo por reservorio**
(Algoritmo R) en vez de "contar y elegir índice" (no hay buffer fijo donde
indexar): un draw de RNG por vecina visitada, distinto del único draw de
`random_neighbor` (r=1, que sí puede permitirse el buffer). 6 tests nuevos.

**Bug real encontrado y corregido durante la implementación**: el primer
intento excluía la celda propia comparando el offset conceptual `(dx,dy) ==
(0,0)` *antes* de envolver — con `r` grande en un torus chico, el offset
conceptual nunca es literalmente `(0,0)` (queda fuera del rango acotado),
pero tras el wrap puede resolver a la posición de partida igual. El test
`neighbor_positions_r_torus_radio_grande_no_duplica` lo detectó de inmediato
(25 resultados en vez de 24 en un torus 5×5). Fix: comparar la posición **ya
envuelta** contra `pos`, no el offset pre-wrap.

### P1-6. Recolección de datos a nivel agente + control de frecuencia

`data.rs` solo recolecta escalares f64 por paso. Faltan (Tier-2 #5 del SOTA):

- **Reporters por agente** (`Fn(AgentId, &A) -> f64`): distribuciones, no solo
  medias — el Gini de Sugarscape hoy se calcula modelo-lado precisamente por
  este hueco.
- **Frecuencia de muestreo** (`every(k)`): un run de 10⁶ pasos con 10
  reporters son 80 MB de f64 que casi nunca se quieren completos.
- **Export columnar**: `to_csv` está bien para el MVP; para el pitch HPC,
  Parquet/Arrow vía `arrow-rs` (feature-gated) es lo que un usuario de
  pandas/polars espera de "millones de agentes".

**✅ Resuelto — 2 de 3 puntos (2026-07-02).**

- **Reporters por agente**: nuevo `AgentDataCollector<M: Model>` (en
  `data.rs`, junto a `DataCollector`), con `add_reporter(name, Fn(AgentId,
  &M::Agent) -> f64)`. Cada fila recolectada es `Vec<(AgentId, valor)>` —uno
  por agente vivo en ese momento, en orden de iteración del `AgentSet`— no un
  escalar. `Simulation::add_agent_reporter`/`agent_data()` lo conectan
  automáticamente en los mismos puntos que el `DataCollector` de siempre
  (paso 0 + cada `end_step`); no-op barato si no se registró ningún reporter
  (`collect()` retorna temprano sin tocar `steps`). 3 tests (recolecta un
  valor por agente vivo, excluye agentes eliminados, no-op sin reporters).
- **Frecuencia de muestreo**: `Simulation::with_collect_every(k)` — la
  decisión de si recolectar vive en `Simulation` (no en cada colector por
  separado), aplicada por igual a `DataCollector` y `AgentDataCollector` para
  que sus ejes de `steps()` queden siempre alineados. El paso 0 siempre se
  recolecta sin importar `k` (`0 % k == 0` para cualquier `k>0`). 2 tests
  (reduce filas conservando el paso 0, panic con `every=0`). Diseño más
  simple que "stride por-reporter" (que habría roto el eje compartido de
  `steps()`) — documentado como decisión de alcance deliberada.
- **Export Parquet/Arrow: diferido, no implementado.** Agregar `arrow-rs`
  como dependencia feature-gated es una decisión de build/tamaño de
  compilación que le corresponde al usuario, no algo para colar en un ítem
  de prioridad baja de una auditoría. `to_csv` sigue siendo la única vía de
  export; queda en el backlog si se necesita el pitch HPC completo.
- Workspace completo en verde: 68 tests en `swarm-core` (+6). API aditiva,
  cero cambios en modelos/ejemplos existentes.

### P1-7. Falta checkpoint/restore (serde)

Ninguna estructura del motor deriva `Serialize`. Para la historia HPC
(corridas largas, clusters con colas, crash-recovery) y para la
reproducibilidad forense ("adjunto el estado exacto en el paso 10⁶"),
`#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]` en
`Grid2D/Graph/ContinuousSpace/AgentSet/DataCollector` + serializar el estado
del `SimRng` (ChaCha8Rng lo soporta) da checkpoint bit-exacto casi gratis.
Encaja directo en el wedge: **checkpoint determinista** = poder reanudar y
obtener bit-identidad con la corrida ininterrumpida. Nadie en la matriz SOTA
reclama eso.

**✅ Resuelto — con una acotación de alcance (2026-07-02).**

- Nueva feature `serde` en `swarm-core` (`["dep:serde",
  "rand_chacha/serde"]` — confirmado que `rand_chacha` la expone). Con ella,
  `#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]` en `Pos`,
  `Grid2D<T>`, `NodeId`, `Graph<N,E>`, `Vec2`, `PointId`,
  `ContinuousSpace<T>` (+ sus tipos internos `Slot`/`PointState`),
  `AgentId`, `AgentSet<A>` (+ `Slot`/`SlotState`) — exactamente la lista del
  borrador, salvo `DataCollector` (ver abajo). `SimRng` (`ChaCha8Rng`) ya es
  serializable vía la propia feature de `rand_chacha`, sin que el motor
  necesite envolverlo.
- **`DataCollector`/`AgentDataCollector` deliberadamente NO serializables**:
  sus reporters son `Box<dyn Fn(...)>` — closures, no serializables en
  Rust por construcción (no hay forma genérica de serializar código). El
  borrador los incluía en la lista, pero esto no es alcanzable tal cual.
  Reformulado como diseño: `Simulation::from_checkpoint(model, seed, rng,
  steps_done)` reconstruye la simulación desde las **cuatro piezas que sí
  determinan el estado resumible** (el modelo, que ya es público vía
  `sim.model`; la semilla; el RNG del paso, expuesto por el nuevo
  `rng_state()`; y los pasos corridos, `step_count()`) — los reporters se
  vuelven a registrar tras restaurar, exactamente como se recomendaba para
  el caso general de checkpoint/resume (los datos ya recolectados no son
  parte del estado que hace falta para *continuar* la simulación).
- **2 tests** en `tests/checkpoint.rs` (gateados `#[cfg(feature =
  "serde")]`), el central es la prueba real del claim: una corrida de 20
  pasos sin interrupción vs. una de 10+10 con checkpoint/restore de por
  medio (serializando a JSON con `serde_json` y reconstruyendo) — **misma
  huella exacta** en la grilla al final. El segundo confirma que
  `from_checkpoint` no vuelve a etiquetar el estado restaurado como "paso
  0".
- Verificado en ambas configuraciones: `cargo test --workspace` (sin
  `serde`, default) y `cargo test -p swarm-core --features serde` — ambas
  en verde, clippy limpio en ambas. La feature es genuinamente opcional: no
  afecta a ningún modelo/ejemplo que no la active.

Esto convierte "checkpoint determinista" (nadie en la matriz SOTA lo
reclama) en un test verificado, no solo una afirmación — mismo patrón que
P0-2 con "reproducibilidad bit a bit bajo paralelismo".

### P1-8. Scheduling: falta activación por etapas (staged)

Mesa tiene `StagedActivation` (todas las fases N antes de la fase N+1) y es el
patrón que los modelos epidemiológicos y económicos usan constantemente. El
motor ya tiene el caso 2-fases (decide/apply) cableado; generalizarlo a N
etapas nombradas es una extensión natural del `Schedule` actual, no un
rediseño. Eventos discretos (DES) en cambio es otro paradigma — no perseguirlo
salvo que SIMPAT lo pida (allá es core).

**✅ Resuelto (2026-07-02).** `Activation::Staged(usize)` + `Agent::stage(&mut
self, stage: usize, id, model: &mut Self::Model, rng)` (no-op por defecto).
A diferencia de `decide` (modelo *inmutable*, pensado para paralelizar), cada
etapa recibe el modelo **mutable** — las etapas son simétricas entre sí, sin
la asimetría lectura/escritura de `decide`/`apply`; encaja con el patrón real
de Mesa (`"moverse"`, `"comer"`, `"reproducirse"`, cada una con acceso
completo). `n` barridos completos por paso (`staged_phases`/`stage_phase`),
mismo orden en las `n` (sin re-mezclar entre etapas — limitación documentada,
Mesa sí lo permite vía `shuffle_between_stages`; no se replicó por alcance).
Cableado en las 4 variantes de `step` existentes (`step`, `step_parallel`,
`step_with_peers`, `step_with_peers_parallel`) — sin esto, un modelo `Staged`
llamado vía `step_parallel()` habría caído silenciosamente en el `_ =>
activate_sequential()` incorrecto (que llama a `Agent::step`, no
`Agent::stage`).

**4 tests** en `tests/staged.rs`, el más importante verifica la garantía
central (no solo que las 3 etapas corran, sino que se respete el barrido
completo): en la etapa 1, cada agente comprueba que *todos los demás* ya
completaron la etapa 0, sin importar el orden de recorrido — la propiedad
que distingue "staged" de simplemente llamar 3 métodos seguidos por agente.
Workspace completo en verde. API aditiva: cero cambios en modelos
existentes (ninguno usaba `Staged` antes, por definición).

### P1-9. `Graph` sin pesos ni aristas dirigidas

`graph.rs:29-33`: no dirigido, sin datos por arista, `add_edge` O(grado) por el
`contains`. Suficiente para SIR-en-red; corto para epidemiología con
intensidades de contacto, redes de flujo o movilidad. API aditiva:
`Graph<N, E = ()>` con dato por arista, y un `DiGraph` o flag. No urgente,
pero cierra la celda "grafo" de la matriz a nivel ✅✅.

**✅ Resuelto (2026-07-02) — flag, no `DiGraph` separado.** `Graph<T>` pasó a
`Graph<N, E = ()>`: `E` con valor por defecto `()` para que **todo** el
código existente (`Graph<T>` en swarm-models, examples, SIGRID) siga
compilando sin cambios. `add_weighted_edge(a, b, peso)` (requiere `E: Clone`
— en grafo no dirigido hace falta una copia del peso en cada extremo) +
`add_edge(a, b)` como conveniencia (`E::default()`) para el caso no
ponderado de siempre. `directed(bool)` como builder en vez de un tipo
`DiGraph` separado — evita duplicar los 5 generadores canónicos (que además
son inherentemente no dirigidos por definición de la literatura: Erdős–Rényi,
Watts–Strogatz, Barabási–Albert no tienen variante dirigida estándar, así que
`directed()` solo aplica a grafos armados a mano por el usuario). Único
cambio de firma pública: `neighbors()` pasó de `&[NodeId]` a
`impl Iterator<Item = NodeId>` (necesario porque la adyacencia interna ahora
guarda `(NodeId, E)`, no solo `NodeId`) — afectó **un** call site en todo el
repo (`examples/network-sir`, `.iter()` de más) y el test de
`golden_values.rs`; ambos actualizados. 2 tests nuevos (peso ida y vuelta,
dirigido conecta un solo sentido). `network-sir` verificado end-to-end (las
tres topologías siguen propagando sensatamente). Workspace completo en
verde.

---

## P2 — Rendimiento fino (medir antes, con criterion ya instalado)

1. **`Grid2D::diffuse` aloca un `Vec` nuevo por paso** (`grid.rs:311`): con un
   scratch buffer interno (`Option<Vec<f64>>` + swap) es cero-alloc por paso.
   En modelos tipo difusión con grillas grandes es el costo dominante después
   del recorrido mismo.
2. **`child_rng` instancia un ChaCha8 por agente por paso**
   (`agent.rs:210,232`): `seed_from_u64` expande con SplitMix y el primer draw
   paga la inicialización del bloque. Alternativas a *medir*: RNG counter-based
   (Philox 4×32, el estándar en simulación paralela exactamente por esto) o
   `ChaCha8Rng::set_word_pos`/streams. Si el bench dice <5% del paso, dejarlo.
3. **`Vec<Option<A>>`**: para `A` grande sin niche, `Option` cuesta 8 bytes +
   branch por slot. La arena de P1-1 con free-list explícita usa el mismo
   espacio para el enlace y desambigua con la generación — dos pájaros.
4. **`to_csv` materializa un `String` completo** (`data.rs:102`): para series
   largas, un `write_csv(&mut impl Write)` streaming evita el pico de memoria.

## P3 — Ecosistema: lo que hace "el mejor" en la práctica

1. **Inglés.** Rustdoc, README y mensajes públicos están en español. Para
   adopción internacional (y coherencia con el paper en SIMPAT) el API público
   necesita rustdoc en inglés — sin eso no hay comunidad posible fuera del
   mundo hispano, y "mejor motor existente" es un título que otorga la
   comunidad. (Los docs internos/comentarios pueden quedarse en español si
   prefieres; la frontera pública no.)

   **✅ Resuelto (2026-07-02).** Los 12 archivos fuente de `swarm-core`
   (~4800 líneas), `swarm-derive/src/lib.rs` y los dos `README.md`
   (raíz + `crates/swarm-wasm`) traducidos al inglés: doc comments
   públicos (`//!`/`///`), comentarios inline de implementación, y
   strings de `panic!`/`assert!`/errores de macro (genuinamente
   user-facing en tiempo de compilación/ejecución). Los nombres de
   función de test (`#[cfg(test)] mod tests`) se dejaron en español
   deliberadamente — no son API pública, renombrarlos no aporta nada a
   la adopción y solo agrega riesgo. Los 3 doctests compilados
   (`lib.rs`, `rng.rs`, `batch.rs`) y los 5 tests `trybuild` de
   `swarm-derive` (snapshots `.stderr` regenerados con los mensajes de
   error ahora en inglés) verificados en verde. `cargo doc` sin
   warnings (se corrigieron 2 links rotos de rustdoc detectados en el
   proceso, sin relación con la traducción). Cero español fuera de
   bloques `#[cfg(test)]` (verificado por grep sobre palabras
   funcionales comunes). Workspace completo (`cargo build`/`clippy -D
   warnings`/`test`, con y sin features `serde`+`experiment`) en verde.

   **Alcance deliberadamente excluido**: `crates/swarm-py` y
   `crates/swarm-wasm/src` (crates de binding, no el motor central;
   quedan fuera de `cargo --workspace` por su propio diseño — ver sus
   READMEs) y `CHANGELOG.md` se dejan en español por ahora. `models/
   sigrid`, `models/debris-flow` y los `examples/*` tampoco se tocaron:
   son artefactos de investigación/demostración con documentación
   propia ya publicada, no la superficie pública del motor que un
   adoptante externo necesita entender primero.
2. **CI con el determinismo como invariante** (ya en pendientes, pero con un
   matiz que nadie del SOTA tiene): además de test+clippy+fmt, un job que
   corre los **valores dorados** (P0-2) en Linux x86-64 **y** en wasm32
   (wasmtime) y compara — la identidad cross-platform del wedge, verificada en
   cada push. Es barato y es un párrafo demoledor en el paper: "la
   reproducibilidad bit a bit es un test de CI, no una afirmación".

   **✅ Resuelto (2026-07-02).** Nuevo job `golden-values-wasm32` en
   `.github/workflows/ci.yml`: instala el target `wasm32-wasip1` +
   `wasmtime` (vía su script oficial de instalación) y corre
   `cargo test --target wasm32-wasip1 -p swarm-core --no-default-features
   --test golden_values` + `--test determinism` con
   `CARGO_TARGET_WASM32_WASIP1_RUNNER=wasmtime`. No hace falta un paso de
   "comparar" por separado: `golden_values.rs` ya fija los resultados
   esperados como literales (`assert_eq!` contra valores `u64` exactos);
   que el mismo archivo de test pase en dos targets distintos **es** la
   prueba de identidad cross-platform — si algún día un target produjera
   bits distintos, el assert pinneado fallaría justo ahí.

   Verificado localmente antes de comprometerlo al CI (se instaló
   `wasmtime` 27.0.0 y el target `wasm32-wasip1` en este entorno para
   probar el pipeline end-to-end, no solo redactar el YAML a ciegas):
   los 7 tests de `golden_values.rs` y los 2 de `determinism.rs` pasan
   **bit a bit idénticos** en `wasm32-wasip1` vía wasmtime frente a
   x86-64 nativo.

   **Hallazgo colateral**: `cargo test --target wasm32-wasip1` fallaba
   al compilar porque `criterion` (dev-dependency, solo para `cargo
   bench`) trae `rayon`, que rechaza compilar para `wasi32` con un
   `compile_error!` propio — aunque el test invocado no usa criterion
   para nada, Cargo igual intenta resolver todas las dev-dependencies
   del paquete. Se resolvió acotando `criterion` a
   `[target.'cfg(not(target_family = "wasm"))'.dev-dependencies]` en
   `crates/swarm-core/Cargo.toml` (las dev-dependencies no soportan
   `optional = true`, pero sí exclusión por target) — no afecta
   `cargo bench` nativo ni el build de `swarm-wasm` (que nunca dependía
   de las dev-dependencies de `swarm-core`; son internas al crate).

   De paso, `cargo fmt --all --check` (ya en el job `check` del CI)
   estaba fallando en deriva de formato preexistente en varios archivos
   (algunos no tocados por P3-1) — se corrió `cargo fmt --all` para que
   el gate de CI existente pase limpio también, no solo el nuevo job.
   Workspace completo (`build`/`clippy -D warnings`/`test`/`fmt --check`)
   verificado en verde tras el cambio.
3. **Release engineering**: publicar en crates.io, CHANGELOG, política MSRV, y
   la política de estabilidad de valores de P0-2 como documento de primera
   clase (`docs/REPRODUCIBILITY.md`).

   **✅ Resuelto (2026-07-02), incluido el rename.**

   Hecho:
   - **`docs/REPRODUCIBILITY.md`**: documento de primera clase con la
     política de estabilidad completa (qué está garantizado, qué no, qué
     cuenta como cambio que rompe determinismo, cómo se aplica en CI) —
     ver también la nueva sección "Rompe determinismo" al inicio de
     `CHANGELOG.md`.
   - **MSRV real determinado empíricamente**, no solo declarado: se probó
     con toolchains 1.85.0/1.86.0/1.87.0 instalados localmente
     (`rustup toolchain install`). `edition = "2024"` por sí sola pide
     1.85, pero `f64::next_down` (`continuous.rs`) pide 1.86 y
     `u64::is_multiple_of` (`sim.rs`) pide 1.87 — el MSRV real es
     **1.87.0**, no la fecha de `edition 2024`. `rust-version = "1.87"`
     en `[workspace.package]`, aplicado explícitamente solo a
     `swarm-abm`/`swarm-abm-derive` (los crates publicables; los modelos
     de investigación usan let-chains de 1.88+ y no están sujetos a
     esto). Nuevo job `msrv` en CI: instala el toolchain 1.87.0
     **pineado** (no `stable`) y corre `cargo test --all-features`
     contra él — el punto de este job es cazar el día en que el código
     empiece a pedir más, no confirmar que compila en stable de hoy.
   - **CHANGELOG.md actualizado** con todo el trabajo de esta auditoría
     (P0/P1/P3-1/P3-2/P3-4) bajo "Sin publicar", incluida una sección
     nueva "Rompe determinismo" que hace explícita la política del
     documento anterior con los 4 cambios reales de esta sesión que
     aplican.
   - Metadatos de `Cargo.toml` listos para publicar: `description` en
     inglés, `keywords`/`categories` válidos de crates.io, `readme`
     (se creó `crates/swarm-abm/README.md` propio — `cargo publish`
     solo empaqueta el directorio del crate, no puede apuntar al README
     de la raíz del repo), `rust-version`.

   **Choque de nombre real encontrado y resuelto mediante rename**:
   `cargo publish -p swarm-core --dry-run` falló — no por un error de
   packaging, sino porque **tanto `swarm-core` como `swarm-derive` ya
   existen en crates.io**, publicados por terceros sin relación con
   este proyecto (`swarm-derive` v0.22.0, macros de `tetsy-libp2p`;
   `swarm-core` v0.1.0, creado 2026-03-12, orquestación de agentes de
   IA — temáticamente cercano y por eso más confuso todavía). Se
   consultó al usuario (no es una decisión de ingeniería unilateral:
   afecta identidad/marca) con alternativas verificadas libres en
   crates.io; eligió **`swarm-abm`** (coincide con el nombre del repo).
   Rename ejecutado: directorios (`crates/swarm-core`→`crates/swarm-abm`,
   `crates/swarm-derive`→`crates/swarm-abm-derive`), nombres de paquete,
   todas las referencias `swarm_core::`/`swarm_derive::` en código
   (~40 archivos vía `sed` + correcciones puntuales), `Cargo.toml`
   (paths, claves de dependencia, `version` junto a `path` — requisito
   de `cargo publish` que no existía antes), CI, y la prosa de código
   vivo en README/CHANGELOG/REPRODUCIBILITY.md (los documentos
   históricos de investigación — `PARITY.md`, `CALIBRATION.md`, el
   draft del paper — se dejaron intactos: describen resultados ya
   obtenidos bajo el nombre de ese momento, no una API vigente).
   Verificado tras el rename: build/clippy -D warnings/test/fmt --check
   en todo el workspace, más `swarm-py` y `swarm-wasm` (excluidos del
   workspace principal, compilados por separado) — sin conflicto entre
   el crate `swarm_abm` y la función `#[pymodule] fn swarm_abm` de
   `swarm-py`. `cargo publish --dry-run` de ambos crates confirma que el
   nombre está libre (`swarm-abm-derive` pasa limpio; `swarm-abm` solo
   falla en dry-run porque su dependencia aún no existe *de verdad* en
   el índice — es el orden esperado, no un problema).

   **Publicado (2026-07-02), con autorización explícita del usuario para
   cada paso**: `cargo publish -p swarm-abm-derive` primero (confirmado
   en `https://crates.io/crates/swarm-abm-derive`, v0.3.0), luego
   `cargo publish -p swarm-abm` una vez el índice tuvo la dependencia
   disponible (confirmado en `https://crates.io/crates/swarm-abm`,
   v0.3.0). Con esto el motor tiene, por primera vez, una instalación de
   un comando (`cargo add swarm-abm`) para cualquiera fuera de este
   repositorio.
4. **Experimentos nativos deterministas** (Tier-1 #1 del SOTA — el
   diferenciador). Diseño concreto: módulo `swarm_abm::experiment` con
   secuencias Sobol'/LHS/Morris generadas en Rust (determinista por
   construcción, sin SALib), componiendo con el `run_sweep` existente:
   `experiment::sobol(&param_specs, n).run(build, outcome)` → índices S1/ST
   con bootstrap. El arnés híbrido de SIGRID (SALib muestrea, Rust evalúa) es
   el prototipo validado; esto lo internaliza.

   **✅ Resuelto (2026-07-02).** Nuevo módulo `swarm_abm::experiment`
   (feature `experiment`, opcional — no en el camino WASM por defecto):

   - **`sobol(&specs, n) -> SobolDesign`** — esquema de Saltelli (2010): dos
     matrices `A`/`B` de `n` puntos generadas de UNA secuencia Sobol' de
     `2d` dimensiones (primeras `d` columnas → `A`, siguientes `d` → `B`,
     decorrelacionadas por construcción). `SobolDesign::run(seed, max_steps,
     n_boot, build, outcome)` evalúa las `n·(d+2)` combinaciones (`A`, `B`,
     `d` matrices `AB_i`), en paralelo si `parallel` está activa, y calcula
     S1 (estimador de Saltelli 2010) + ST (estimador de Jansen 1999) con
     intervalos de confianza al 95% por bootstrap (remuestreo con
     reemplazo de los índices de evaluación, recalculando la fórmula
     cerrada — no re-corre el modelo).
   - **`latin_hypercube(&specs, n, seed) -> Vec<Vec<f64>>`** — hipercubo
     latino propio (estratificación + jitter, sobre las primitivas RNG del
     motor: `shuffle`/`uniform_f64`), sin dependencia externa.
   - **`morris(&specs, n_trayectorias, niveles, seed) -> MorrisDesign`** —
     diseño clásico de Morris (1991): trayectorias de `d+1` puntos, una
     dimensión perturbada a la vez en orden aleatorio, paso fijo `+delta`
     (simplificación documentada frente al `±delta` del diseño original —
     no afecta la interpretación de `mu`/`mu_star`/`sigma`).
     `MorrisDesign::run(...)` devuelve esas tres estadísticas por parámetro.

   **Decisión de dependencia (documentada, no implícita): sí depender de la
   crate `sobol`** (BSD-3-Clause, números de dirección de Joe & Kuo) para la
   secuencia de baja discrepancia — a diferencia de P0-2, donde se
   reimplementó el muestreo de `rand` precisamente porque su algoritmo
   interno no tiene garantía de estabilidad entre versiones. Una secuencia
   de Sobol' es distinta en naturaleza: dados los mismos números de
   dirección y la recurrencia canónica de Antonov-Saleev, la secuencia está
   matemáticamente determinada — no hay "elección de la librería" que un
   *bump* de versión pueda cambiar. Es una dependencia de una
   especificación publicada, no de un detalle de implementación no
   especificado. La distinción está documentada en el rustdoc del módulo
   para que la próxima persona no la repita como pregunta abierta.

   **Validación real, no solo "compila y corre"**: el test central
   (`sobol_indices_coinciden_con_ishigami_analitico`) evalúa el diseño
   contra la **función de Ishigami**, el benchmark estándar de GSA con
   índices S1/ST de forma cerrada conocida (Saltelli et al. 2008): S1 ≈
   [0.314, 0.442, 0.0], ST ≈ [0.557, 0.442, 0.244] con N=4096. Los cuatro
   valores coinciden dentro de tolerancia 0.05 — incluida la firma
   cualitativa que hace de Ishigami un caso no trivial (`x3` tiene S1≈0 pero
   ST sustancial, por interacción pura con `x1`). Sin este test, un error de
   signo o de índice en la fórmula de Saltelli/Jansen habría producido
   resultados plausibles pero silenciosamente incorrectos — exactamente el
   tipo de bug que ningún test de "corre sin panic" puede cazar. 4 tests en
   total (Sobol' vs. Ishigami, determinismo de Sobol', estratificación +
   rango de LHS, Morris produce estadísticas finitas y deterministas).
   Workspace completo en verde con y sin la feature `experiment`.

   **Alcance v1, documentado en el rustdoc del módulo**: una evaluación del
   modelo por punto del diseño (sin promediar réplicas — un modelo ABM muy
   ruidoso se beneficiaría de varias réplicas por punto; el usuario puede
   envolver `outcome` con su propio promedio si lo necesita). `sobol`
   soporta hasta 50 parámetros (la tabla `minimal` de Joe & Kuo cubre 100
   dimensiones; Saltelli usa `2d`). Compatibilidad wasm32 de la dependencia
   `sobol`/`libflate` no verificada — irrelevante mientras la feature no se
   active en el camino WASM, pero queda como pregunta abierta si se
   necesitara en el futuro.
5. **Batch: progreso y aborto**: `run_sweep` de 7.500 sims corre mudo y no se
   puede cortar. Un callback opcional de progreso (`Fn(done, total)` atómico) y
   un `AtomicBool` de cancelación cuestan 20 líneas y transforman la
   experiencia de calibración.
6. **Property-based testing** (`proptest`): invariantes que ya se testean
   puntualmente, generalizadas — conservación de masa de `diffuse` bajo
   cualquier grilla/rate, hash espacial == fuerza bruta bajo cualquier
   configuración (el test actual usa una sola), determinismo de cualquier
   generador de grafos. P0-1 y P0-4 son exactamente la clase de bug que esto
   caza solo.

## Qué NO cambiar (fortalezas a proteger)

- **Cero `unsafe`** en el núcleo, `#![warn(missing_docs)]` y
  `warn(clippy::unwrap_used)` — la higiene es real.
- **El patrón take-out** para el doble préstamo: simple, sin `RefCell`, sin
  overhead medible. Es mejor solución que la de Mesa (que no tiene el
  problema porque Python no tiene préstamos… y por eso es 67× más lento).
- **`decide` con `&Model` inmutable**: que el *compilador* pruebe la ausencia
  de escrituras durante la fase paralela es el argumento técnico más elegante
  del motor — consérvalo en cualquier evolución del trait (P1-2 lo respeta).
- **API mínima y ortogonal** (3 espacios × 1 trait de agente × 1 runner):
  resistir la tentación GAMA de crecer hacia un DSL.
- **Hot path sin allocs** (buffer de orden reutilizado, `Neighbors` en stack,
  `random_neighbor` sin Vec): la disciplina ya existe; P1-4 y P2-1 son
  aplicarla a los rincones que faltan.
- **Tests que validan contra teoría** (punto fijo analítico, Vicsek, paridad
  Mesa): es la cultura de validación que el paper vende — mantenerla como
  requisito para cada feature nueva.

## Priorización sugerida

| # | Ítem | Esfuerzo | Impacto | Cuándo |
|---|------|----------|---------|--------|
| P0-1 | ✅ Determinismo `barabasi_albert` + tests todos los generadores | horas | Protege el claim del paper | **Hecho** 2026-07-02 |
| P0-2 | ✅ Shuffle/range propios + valores dorados en CI | 1-2 días | Convierte el wedge en garantía | **Hecho** 2026-07-02 |
| P0-3 | ✅ `child_rng` en cadena (con P0-2, misma ventana de ruptura) | horas | Robustez del wedge | **Hecho** 2026-07-02 |
| P0-4/5 | ✅ wrap fuera de dominio; paso 0 | horas | Corrección menor | **Hecho** 2026-07-02 |
| P1-3 | ✅ `#[derive(MultiAgent)]` (heterogeneidad) | 1 semana | Fricción de adopción nº 1 | **Hecho (Fase 1)** 2026-07-02 |
| P1-1 | ✅ Arena generacional en `AgentSet` | 2-3 días | Escala demográfica + self-remove | **Hecho** 2026-07-02 |
| P1-2 | ✅ Snapshot de agentes en `decide` | 2-3 días | Semántica simultánea completa | **Hecho** 2026-07-02 (aditivo, no rompió el trait) |
| P1-4 | ✅ `ContinuousSpace` v2 (remove, flat buckets, sin HashSet) | 2-3 días | Hot path de SIGRID/boids | **Hecho** 2026-07-02 |
| P3-4 | ✅ Experimentos nativos deterministas (Sobol/Morris/LHS) | 1-2 semanas | **El** diferenciador (Tier-1 #1) | **Hecho** 2026-07-02 |
| P1-7 | ✅ Checkpoint serde bit-exacto | 2 días | Historia HPC única | **Hecho** 2026-07-02 |
| P1-6 | ✅ Datos nivel agente + frecuencia | 2 días | Paridad Mesa en colección | **Hecho (2/3)** 2026-07-02 |
| P3-1/2 | ✅ Inglés + CI valores dorados en wasm32 | 1 semana | Adopción | **Hecho** 2026-07-02 |
| P3-3 | ✅ CHANGELOG + MSRV + REPRODUCIBILITY.md + rename + **publicado en crates.io** | 1 semana | Adopción | **Hecho** 2026-07-02 |
| P1-5 | ✅ Radio r en `Grid2D` | según demanda | Paridad de features | **Hecho** 2026-07-02 |
| P1-8 | ✅ Staged activation | según demanda | Paridad de features | **Hecho** 2026-07-02 |
| P1-9 | ✅ Grafos con peso/dirigido | según demanda | Paridad de features | **Hecho** 2026-07-02 |
| P2-* | Rendimiento fino | según bench | Marginal hoy | backlog |

**La secuencia estratégica**: P0 completo (el wedge queda blindado y
verificado por CI) → P1-1..4 (la arquitectura deja de tener las fricciones que
SIGRID documentó) → P3-4 (experimentos nativos: el feature que ningún
competidor puede reclamar) → P3-1..3 (abrir al mundo). Con eso el motor no
solo *afirma* ser el único ABM con experimentos deterministas bit a bit —
lo **demuestra en cada push de CI**, y esa es la definición operativa de
"mejor motor de ABM espacial existente" que este proyecto puede defender.
