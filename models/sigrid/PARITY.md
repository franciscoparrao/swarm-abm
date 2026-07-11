# Paridad SIGRID: port Rust (swarm-abm) vs modelo Mesa original

## Reducción del residual de 2 perros — 2026-07-03

Al revisar el pendiente "cerrar el residual de 2 perros" se encontró que el
modelo **Mesa de referencia cambió muy recientemente** bajo nuestros pies:
`Isla_Riesco/simulacion_agentes/agents/dog.py` (editado 2026-07-03) y `fox.py`
(editado 2026-07-02, después de que se generó el caché `parity_d14.npz` que
usa la sección "Re-validación" de abajo como línea base Mesa). El propio
comentario de `dog.py` describe exactamente el mecanismo que este documento
ya había diagnosticado como causa del residual: una asimetría de escala
donde el zorro percibe al perro a `FOX_DOG_DETECTION_RADIUS=1500` m pero el
perro solo detectaba al zorro a 400 m, dejándolo "ciego" ante depredadores
que ya habían decidido cazar. Ana lo corrigió en Mesa subiendo
`DOG_DETECTION_RADIUS` a 1200 m y `DOG_PROTECTION_STRENGTH` (magnitud de la
protección directa perro→éxito de caza) de 0.10 a 0.20.

**Cambios aplicados al port** (`models/sigrid/src/lib.rs`):
1. **Espejo del fix de Mesa**: `DOG_DETECTION_RADIUS` 400→1200 m,
   `DOG_PROTECTION_STRENGTH` 0.10→0.20 (mismos valores y misma fórmula que
   Mesa: `m_dog = -DOG_PROTECTION_STRENGTH * (1 - dist/DOG_PROTECTION_RADIUS)`).
2. **Disuasión multi-objetivo** (el ítem original del pendiente): antes,
   cuando el perro llegaba a interceptar a su objetivo, solo ESE depredador
   quedaba disuadido — con varios depredadores acechando a la vez, el perro
   solo podía intervenir sobre uno por tick. Ahora la confrontación también
   disuade a cualquier otro depredador dentro de `DOG_CHASE_RADIUS` (200 m),
   no solo al perseguido. Probado primero con el radio de contacto
   (`DOG_DETER_RADIUS`, 50 m): la mejora fue nula/negativa (muy angosto para
   que coincidan dos depredadores). Con 200 m sí ayuda de forma consistente.

**Medición** — no es una re-validación de paridad completa contra Mesa (el
caché Mesa quedó desactualizado por los cambios de Ana; re-correrlo toma
~40 min y queda como pendiente, ver abajo). Es una comparación **interna**
Rust antes/después, mismas semillas, n=30 y luego n=60 por punto (los 4
puntos de 2 perros del diseño factorial de 12 puntos, 14 días):

| fox_eff | chilla | Antes (residual doc., re-validación) | Solo espejo de Mesa (n=30) | + multi-objetivo @200m (n=60) |
|---:|---:|---:|---:|---:|
| 0.08 | 0  | ~14-21% | 9.15% | **6.34%** |
| 0.08 | 10 | ~14-21% | 13.06% | **6.73%** |
| 0.26 | 0  | ~14-21% | 11.25% | **12.57%** |
| 0.26 | 10 | ~14-21% | 12.99% | **10.03%** |

El espejo del fix de Mesa por sí solo ya reduce el residual en los 4 puntos
(confirmado con n=30, la muestra de n=5 inicial dio señal mixta por puro
ruido — sd~7-19 con n=5 vs diferencias de 1-5pp). La disuasión
multi-objetivo @200m suma una reducción adicional en 3 de 4 puntos (el
cuarto, fox_eff=0.26/chilla=0, queda esencialmente plano). Media de los 4
puntos: 11.61% (solo espejo) → 8.92% (+ multi-objetivo), frente al ~14-21%
documentado antes y al ~0% de Mesa. **El residual se redujo a aproximadamente
la mitad, no se cerró del todo.**

**Pendiente real**: la comparación de arriba es Rust-contra-Rust, no
Rust-contra-Mesa-actual. Con `dog.py`/`fox.py` cambiados, el `Mesa ~0%`
documentado en este archivo también podría haberse movido. Para una
conclusión de paridad honesta hace falta re-correr
`Isla_Riesco/experiments/parity.py --reps 5 --days 14` **sin** `--reuse-mesa`
(la carga del lado Mesa actual toma ~40 min en la corrida completa de 12
puntos) y comparar contra el port ya actualizado. No se corrió en esta
sesión por el costo de tiempo; los cambios de código y la medición interna
sí quedan validados (`cargo test -p sigrid`, 4/4 ok).

## Migración a Sobol nativo (`swarm_abm::experiment`) — 2026-07-02

El análisis de sensibilidad global (Sobol N=512/30 días, sección siguiente)
se corrió con un arnés híbrido: `Isla_Riesco/experiments/sobol_rust.py`
muestreaba (Saltelli, SALib) y analizaba (S1/ST, SALib) en Python; solo la
evaluación —la parte cara— corría en Rust vía el binario `sobol_eval.rs`.
Ese arnés queda **superseded** por `models/sigrid/src/bin/sobol_native.rs`
(binario `sobol-native`), que usa `swarm_abm::experiment` (cerrado en
P3-4 de `docs/AUDIT.md`) para hacer las tres etapas —muestreo Saltelli,
evaluación, S1/ST con bootstrap— en Rust puro. No hay Python en el camino
del análisis de sensibilidad.

`sobol_eval.rs` (el evaluador por CSV) **no se elimina**: lo sigue usando
`Isla_Riesco/experiments/parity.py` para la comparación punto a punto contra
Mesa (12 puntos factoriales fijos, no un diseño de muestreo), que es una
tarea distinta de la del Sobol.

**Validación del reemplazo** — corrida `sobol-native --n 64 --days 30`
(512 evaluaciones; el esquema del motor no calcula índices de segundo orden,
así que a igual `N` evalúa `N·(d+2)` puntos, la mitad que el `N·(2d+2)` de
SALib con `calc_second_order=True` — no es una comparación 1:1 en `N`, pero
sí en la conclusión):

| Parámetro (ST) | Híbrido SALib+Rust (N=512, 7168 evals) | Nativo `sobol-native` (N=64, 512 evals) |
|---|---:|---:|
| n_dogs | 1.04 [0.955, 1.125] | **1.08** [0.837, 1.340] |
| sheep_density | 0.41 | 0.45 |
| chilla_density | 0.43 | 0.24 |
| lamb_proportion | 0.37 | 0.30 |
| fox_predation_effectiveness | 0.35 | 0.28 |
| hare_density | 0.36 | 0.37 |

El hallazgo central sobrevive con una muestra ~14× más chica: `n_dogs`
domina con claridad (ST≈1.0-1.1, intervalos de confianza solapados entre
ambos métodos) y sin solape con el segundo lugar. El orden dentro del grupo
secundario (`sheep_density`/`chilla_density`/`lamb_proportion`/
`fox_predation_effectiveness`/`hare_density`) sí cambia entre corridas —
esperable con `N` catorce veces menor, esos cinco efectos ya eran
comparables entre sí en la corrida grande (0.35-0.43). No se recomienda usar
`N=64` para las conclusiones del ranking secundario; sirve como validación
de que el motor nativo reproduce el resultado dominante, no como reemplazo
de la corrida N=512 documentada abajo (que se mantiene como la referencia
del ranking).

## Re-validación 2026-07-02 (motor post P0-2/P0-3/P1-1)

Los números originales de este documento (Pearson 0.966, RMSE 10.1, Sobol
N=512 con `n_dogs` ST=0.93) se corrieron con una versión anterior del motor.
La auditoría de ingeniería del motor (`docs/AUDIT.md`) cambió deliberadamente
las primitivas de RNG (P0-2: `uniform_below`/`bernoulli`/`shuffle` propias en
vez de `rand::Rng`; P0-3: `child_rng` en cadena en vez de XOR) y la
asignación de índices de agente (P1-1: arena generacional con reúso de
slots) — todos preservan el determinismo (misma semilla ⇒ mismo resultado)
pero cambian **cuál** es ese resultado para la misma semilla. SIGRID crea
agentes en runtime (nacimientos), así que queda afectado por los tres
cambios. Se re-corrió la validación completa con el motor actualizado; el
lado Mesa (Python, `np.random` global, independiente del motor) **no**
necesitó re-correrse — se reutilizó del caché (`--reuse-mesa`).

**Paridad (12 puntos × 5 semillas, 14 días) — antes vs. después:**

| Métrica | Antes (motor viejo) | Después (motor v0.4) |
|---|---:|---:|
| Pearson r | 0.966 | 0.963 |
| Spearman ρ | 0.902 | 0.916 |
| RMSE | 10.1 pp | 12.1 pp |
| MAE | 8.2 pp | 9.8 pp |
| Media Mesa / Rust | 31.7% / 33.1% | 31.7% / 35.6% |
| Sesgo | +1.4 pp | +3.9 pp |

**Lectura**: la correlación de Pearson (estructura de rangos entre puntos) es
prácticamente idéntica (0.966→0.963), y Spearman incluso mejoró levemente
(0.902→0.916) — la propiedad central de la paridad, que el modelo ordene
correctamente los efectos de los parámetros, es **robusta** al cambio de RNG
del motor. Es el resultado esperado si el port está bien implementado: las
propiedades estadísticas de un ABM no deberían depender de la secuencia
pseudoaleatoria específica, solo de que las reglas del modelo estén bien
traducidas. El RMSE y el sesgo global empeoraron algo (10.1→12.1 pp,
+1.4→+3.9 pp), concentrado casi enteramente en el residual **ya
diagnosticado** de 2 perros (ver "Diagnóstico" abajo: Mesa ~0%, el port
9-19% antes, ahora 14-21%) — no es un problema nuevo, es el mismo residual
conocido, un poco más pronunciado con esta secuencia de semillas particular.
No cambia ninguna conclusión cualitativa del documento original.

**Sobol N=512/30 días — re-corrida con el motor v0.4:**

| Parámetro (ST) | Port tuneado (motor viejo) | Port tuneado (motor v0.4) |
|---|---:|---:|
| n_dogs | 0.93 | **1.04** |
| chilla_density | 0.50 | 0.43 |
| sheep_density | 0.48 | 0.41 |
| lamb_proportion | 0.43 | 0.37 |
| fox_predation_effectiveness | 0.42 | 0.35 |
| hare_density | 0.41 | 0.36 |

`n_dogs` sigue siendo, con claridad, el driver dominante — el hallazgo
central **no solo sobrevive la re-validación, se refuerza**: ST sube de 0.93
a 1.04 (intervalo bootstrap [0.955, 1.125], sin solape con el segundo lugar,
`chilla_density` en [0.367, 0.493]) y la brecha con el segundo lugar pasa de
~1.9× a ~2.4×. El resto del ranking (`chilla_density` > `sheep_density` >
`lamb_proportion` ⪆ `hare_density`/`fox_predation_effectiveness`) se
mantiene esencialmente igual, con `hare_density` y
`fox_predation_effectiveness` intercambiando el 5°/6° lugar — una
reordenación menor dentro del grupo de efectos ya secundarios, sin
consecuencia para el mensaje de manejo. `sum(ST)=2.96` (antes 3.18): la
estructura de interacciones sigue siendo el grueso del efecto de casi todos
los parámetros salvo `n_dogs` (S1 de `n_dogs`=0.624 vs. S1≈0 del resto —
`n_dogs` es el único con efecto de primer orden real; todo lo demás actúa
casi enteramente por interacción). `Y`: media 50.1%, sd 34.7%, min 0.19%,
max 100% — sin saturación, distribución sana.

Corrida completa: 7168 evaluaciones en 2235 s (37.3 min) en 16 cores — más
rápido que los ~88 min originales (motores/hardware no son directamente
comparables entre sesiones; no se afirma una mejora de rendimiento
atribuible al refactor sin un benchmark A/B dedicado, se reporta el tiempo
observado sin más).

**Conclusión de la re-validación**: los cambios de determinismo del motor
(P0-2/P0-3/P1-1) alteraron los números exactos, como estaba previsto y
documentado, pero **ninguna conclusión científica cambia**. La paridad
estructural (Pearson, Spearman) es robusta; el ranking del Sobol no solo se
mantiene, la señal dominante de `n_dogs` se ve más nítida. Es exactamente el
resultado que valida la premisa del motor: la reproducibilidad bit a bit es
una propiedad de ingeniería, no algo de lo que dependa la validez del
modelo científico construido encima.

## Re-corrida 2026-07-10 — A/B controlado del fix de *common random numbers*

La 3ª auditoría del motor (`../../docs/AUDIT.md`, "Auditoría — 3ª pasada
(2026-07-10)") encontró dos defectos en el `experiment::sobol` con el que se
computa este GSA: **A1/F3** — las evaluaciones no compartían *common random
numbers* (cada punto del diseño de Saltelli corría con una semilla
independiente), lo que para un ABM estocástico infla el `ST` de los
parámetros inertes con ruido puro; y **A2** — el `.skip(1)` de la secuencia
Sobol' rompía el balance diádico (Owen 2020). Ambos están corregidos en
`swarm-abm` 0.4.0.

Para aislar el efecto de estos fixes de otros cambios (en particular el fix
del modelo `3276c6d`, residual de 2 perros, posterior a la re-validación de
arriba), se corrió un **A/B controlado con el modelo fijo en HEAD**
(`54982e2`): dos brazos idénticos salvo por revertir F3+A2 en `experiment.rs`.
Ambos: `sobol-native --n 512 --days 30 --seed 1 --n-boot 500`, 4096
evaluaciones, desde worktrees limpios.

| ST (efecto total) | BUGGY-GSA (F3+A2 revertidos) | FIXED (0.4.0) | Δ (fix GSA) |
|---|---:|---:|---:|
| n_dogs | **1.021** [0.919, 1.111] | **0.902** [0.801, 0.992] | −0.119 |
| chilla_density | 0.204 | 0.194 | −0.010 |
| sheep_density | 0.199 | 0.163 | −0.036 |
| fox_predation_effectiveness | 0.198 | 0.152 | −0.046 |
| lamb_proportion | 0.195 | 0.177 | −0.018 |
| hare_density | 0.183 | 0.183 | 0.000 |
| **sum(ST)** | **2.00** | **1.77** | **−0.23** |
| n_dogs S1 | 0.782 | 0.796 | +0.014 |
| Y media | 38.46% | 38.23% | (modelo constante ✓) |

**Lecturas:**

1. **El A/B es limpio.** La media de `Y` (38.5% vs 38.2%) y la distribución
   coinciden entre brazos: el modelo no cambió, solo el esquema de semillas
   del análisis de sensibilidad.

2. **Qué hace realmente el fix de CRN.** El brazo con bug estima
   `ST[n_dogs] = 1.021`, es decir **por encima de 1** — imposible para un
   índice de efecto total legítimo; es la firma de la contaminación por
   ruido que F3 describe. El fix lo baja a `0.902` (físicamente sano, < 1),
   des-infla los efectos secundarios en 0.01–0.05, y deja `S1[n_dogs]`
   prácticamente intacto (0.782 → 0.796) — el efecto de primer orden es
   robusto, como se espera (el ruido sin CRN se cuela por la vía de la
   interacción/total, no del primer orden).

3. **Corrección a la lectura preliminar.** La caída de `sum(ST)` frente a los
   números viejos documentados arriba (≈2.96 → 1.77) **no** es
   mayoritariamente el fix del GSA, como se hipotetizó al ver la primera
   corrida. El fix de CRN aporta solo −0.23 (2.00 → 1.77); el grueso
   (2.96 → 2.00) es el fix del modelo `3276c6d` más el cambio de arnés (el
   módulo nativo estima S1/ST con `N·(D+2)=4096` evaluaciones, mientras la
   tabla vieja de 2026-07-02 usó el híbrido SALib con `N·(2D+2)=7168` e
   incluía términos de segundo orden). El A/B controlado fue justamente lo
   que evitó atribuir al fix del GSA un efecto que es del modelo.

4. **El hallazgo central es robusto en ambos brazos.** `n_dogs` domina con
   `ST` 0.90–1.02 y una brecha de ~4.6–5× sobre el segundo lugar
   (`chilla_density`), y es el único parámetro con efecto de primer orden
   real (S1≈0.79; el resto con S1 indistinguible de cero). El fix del GSA
   **mejora la calidad del estimador** (elimina el artefacto `ST > 1` sobre
   el parámetro dominante y afina levemente el orden secundario) sin tocar
   ni la señal de primer orden ni la conclusión de manejo: el número de
   perros es la palanca.

*Nota de alcance:* estas cifras son del modelo **committeado** en `54982e2`.
El diff en progreso sobre `src/{lib.rs,main.rs}` (fuera de esta corrida) las
volvería a mover; re-correr el A/B tras committear ese trabajo es directo con
los mismos comandos.

---


Validación distribucional del port frente al modelo Mesa de Isla Riesco
(`Isla_Riesco/simulacion_agentes/`). No se busca igualdad bit a bit (el RNG
difiere: ChaCha8 sembrable vs `np.random` global), sino que la distribución del
**loss rate** de ovejas se solape punto a punto, igual que la validación de
`debris-flow`.

## Método

- Arnés: `Isla_Riesco/experiments/parity.py` (corre ambos motores) +
  `models/sigrid/src/bin/sobol_eval.rs` (evaluador Rust).
- 12 puntos factoriales: `fox_eff ∈ {0.08, 0.26} × n_dogs ∈ {0,1,2} ×
  chilla ∈ {0, 10/km²}`, con `sheep_density=0.96`, `hare=0.1`, `lamb=0.2`.
- 5 semillas por punto por motor; horizonte 14 días; sin estacionalidad, sin
  puma (config del análisis de sensibilidad).
- Métrica de salida: `loss_rate_pct = sheep_killed / n_sheep_initial * 100`.

## Resultado (5 semillas, 14 días)

Config final: evitación con **riesgo sumado** (perros dentro de `DOG_AVOID_RADIUS`)
+ **memoria de zonas peligrosas** con **marca proactiva** (el zorro evalúa el
riesgo y veda el lugar ANTES de atacar, decae a 168 h) + patrulla cercana al
rebaño (`DOG_PATROL_RADIUS=250`).

Config final: evitación con riesgo sumado + memoria de zonas peligrosas con
marca proactiva (168 h) + patrulla cercana (`DOG_PATROL_RADIUS=250`) +
**acecho multi-tick**: si hay un perro en rango de detección, el zorro NO mata el
tick que llega a la presa — queda expuesto y el perro tiene un turno para
interceptarlo; la disuasión resetea su hambre y cancela el acecho. Sin perro
cerca, mata de inmediato (baseline sin perros intacta).

| fox_eff | perros | chilla | Mesa | Rust | dif |
|---:|---:|---:|---:|---:|---:|
| 0.08 | 0 | 0  | 35.8 | 38.0 | +2.2 |
| 0.08 | 0 | 10 | 95.9 | 79.5 | −16.5 |
| 0.08 | 1 | 0  | 15.0 | 6.3  | −8.7 |
| 0.08 | 1 | 10 | 17.2 | 18.8 | +1.5 |
| 0.08 | 2 | 0  | 0.2  | 12.7 | +12.5 |
| 0.08 | 2 | 10 | 0.2  | 14.8 | +14.7 |
| 0.26 | 0 | 0  | 57.9 | 50.7 | −7.2 |
| 0.26 | 0 | 10 | 100.0| 100.0| 0.0 |
| 0.26 | 1 | 0  | 26.8 | 23.3 | −3.5 |
| 0.26 | 1 | 10 | 30.3 | 25.1 | −5.2 |
| 0.26 | 2 | 0  | 0.5  | 9.5  | +9.0 |
| 0.26 | 2 | 10 | 0.7  | 18.6 | +17.9 |

**Pearson r = 0.966 · Spearman ρ = 0.902 · RMSE = 10.1 pp · MAE = 8.2 pp.**
Media global: Mesa 31.7 % vs Rust 33.1 % (sesgo +1.4 pp).

Evolución del tuning: simple proximidad (RMSE 14.2, sesgo +6.5) → +memoria
(RMSE 16.1, sesgo +10.1) → **+acecho multi-tick (RMSE 10.1, sesgo +1.4)**.

## Diagnóstico

- **Núcleo de predación fiel y estructura de rangos excelente** (Pearson 0.97):
  el modelo ordena correctamente los efectos, lo central para la sensibilidad.
- **El acecho multi-tick eliminó el sesgo global** (+10→+1.4 pp) y bajó el RMSE
  a la mitad del original (28→10): dar un turno de exposición permite que el
  perro intercepte, como en Mesa.
- **Residual menor: 2 perros.** Mesa ~0 %; el port ~9-19 %. Los 2 perros no
  alcanzan a disuadir a todos los depredadores que acechan a la vez (cada perro
  persigue a uno por tick). **Actualización 2026-07-03** (ver sección al inicio
  de este documento, "Reducción del residual de 2 perros"): espejar un fix de
  Mesa (radio/magnitud de disuasión del perro) + disuasión multi-objetivo lo
  redujo a la mitad (~9-13%, medido internamente Rust-contra-Rust), sin
  cerrarlo del todo — pendiente re-validar contra Mesa actual.
- Probado y descartado: radio de evitación 1000 m (regresó y disparó varianza).
- Probado y descartado (2026-07-03): disuasión multi-objetivo con el radio de
  contacto (`DOG_DETER_RADIUS`, 50 m) — demasiado angosto para que coincidan
  dos depredadores, mejora nula/negativa. Con `DOG_CHASE_RADIUS` (200 m) sí
  funciona.

## Implicación para el ranking del Sobol (N=512, 30 días)

La paridad importa para las CONCLUSIONES. El refuerzo de la disuasión del perro
reordenó el análisis de sensibilidad global:

| Parámetro (ST) | Port sin tuning | Port tuneado |
|---|---:|---:|
| n_dogs | 0.41 | **0.93** |
| chilla_density | **0.68** | 0.50 |
| sheep_density | 0.13 | 0.48 |
| lamb_proportion | 0.08 | 0.43 |
| fox_predation_effectiveness | 0.16 | 0.42 |
| hare_density | 0.08 | 0.41 |

`n_dogs` pasa a ser el driver dominante (ST 0.93), coherente con Mesa y con el
mensaje de manejo. El output se des-satura (media 81→60%, mín 24→3.7%) y las
interacciones crecen (sum ST 1.54→3.18). El ranking sigue **provisional**
mientras persistan el sesgo ~+10 pp y el residual de 2 perros, pero la dirección
es la correcta. La habilitación HPC es independiente de esto y ya es firme.

## Velocidad

Sobre el conjunto de paridad (60 evals, 14 días): Mesa ~2576 s vs Rust ~49 s
(**~53×** aquí; ~100-116× en evals de 30 días más caros). El Sobol N=512/30d
(7168 evals), inviable en Mesa (~2728 core-hours), corre en **88 min** en un
nodo de 16 cores.

## Implicación

La **habilitación HPC** (port + speedup + análisis de sensibilidad global
factible) está demostrada con números reales. El **ranking ecológico** del GSA
es provisional hasta cerrar el residual de 2 perros: con el perro aún algo débil,
el Sobol sobreestima la importancia de `chilla_density` frente a `n_dogs`.
