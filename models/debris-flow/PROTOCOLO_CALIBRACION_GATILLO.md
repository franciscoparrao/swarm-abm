# Protocolo de calibración del inicio de flujos (punto 3) — propuesta

**Estado:** aprobado por nowcast el 2026-10-23 con tres cambios (unidad zona-día →
zona-evento para el umbral; regla de densidad declarada por ellos; `2·a` por modelo)
y **ejecutado** el 23-oct: barrido de densidad en dos vueltas
(`calibracion/barrido_densidad.py`, reglas v1 y v2) y umbral con la tabla zona-día
(`calibracion/umbral_zona_dia.py`). Los reportes con cifras viven junto a los datos
en `nowcast/data/calibracion_runout/` (derivados del catálogo de SERNAGEOMIN, no
redistribuibles) y no en este repo. Resultado en una línea: densidad 0,1 gotas/km²
por la regla v2 con la serie de perfil (0,2 con la cota uniforme; banda 0,05–0,2),
en sombra en operación; el umbral de inicio **no se identifica** con estaciones
(cuatro mediciones independientes detrás de 57 eventos) y queda a la espera de la
forzante IMERG por evento. Texto original de la propuesta (22-oct) a continuación.
**Datos:** `nowcast/data/calibracion_runout/` (no se redistribuye; términos de
SERNAGEOMIN pendientes). **Motor:** `swarm-abm` ≥ `6f07499`.

## 0. Resumen

El paquete no alcanza para calibrar los tres parámetros libres del modo gatillo
como si fueran físicos. Propongo tratarlos de forma distinta:

| Parámetro | Qué mueve (medido) | Propuesta |
|---|---|---|
| `trigger_volume_scale` | la **vida útil** de cada flujo en zona plana (logarítmica; satura sobre ~20) | **fijarlo por construcción**: `2 · I₁`, con `I₁` el umbral I-D a 1 h (mm/h). No calibrarlo. |
| umbral de inicio (I-D de nowcast; `rain_threshold` solo en modo lluvia) | **cuántas horas** disparan en la ventana | calibrar al nivel día-estación con `dias_etiquetados.csv`, como problema PU; entregar **banda**, no punto; validar dejando un temporal fuera. |
| `trigger_drops_per_km2` | **cuántos flujos** nacen por hora excedida ⇒ la escala de la huella (×5,7 entre 0,02 y 0,5 en El Arrayán) | **punto de operación**, no parámetro físico: curva aciertos-vs-área sobre los 56 eventos con lluvia, regla de elección declarada; se vuelve físico cuando haya huellas (PlanetScope). |

La degeneración entre `trigger_volume_scale` y `trigger_drops_per_km2` que
plantea nowcast es real con puntos; la propuesta la cierra **por construcción**
(fila 1), no con datos. Cuando haya huellas, se reabre y se calibra la pareja.

## 1. Qué hay y qué no (del README del paquete)

- 214 eventos fechados al día (desde nov-2016; RM, O'Higgins, Valparaíso), 204
  con detonante «Lluvias».
- **56** con lluvia medida a ≤ 15 km, y salen de **tres temporales**: 29–31 ene
  2021 (34), 22–24 jun 2023 (21), 26 abr 2022 (1). En `dias_etiquetados.csv`
  son **12 días-estación `evento`** contra **11 785 `no_evento`**.
- 118 sin estación a 15 km; 35 en días sin dato (incluye los 34 flujos de feb
  2017); 5 con < 1 mm en la estación más cercana (convectivos que la red no ve).
- `no_evento` = «sin flujo reportado», no «sin flujo»: catálogo incompleto.
- Día de agromet en UTC−4 fijo; catálogo en día civil; la ventana
  `[ini−1, fin+1]` absorbe el desfase.
- Serie horaria solo en ±3 días alrededor de cada evento, estaciones a ≤ 30 km.

Consecuencia que manda sobre todo lo demás: **cualquier número que salga de
aquí está sostenido por tres temporales, y uno de ellos (ene-2021) pone 34 de
los 56 eventos.** Por eso toda validación es «dejar un temporal fuera» y todo
resultado se entrega como banda entre pliegues.

## 2. Qué mueve cada parámetro (medido en Chañaral, s42, 50 mm/día, E en gradiente, `follow`)

Barrido de `trigger_volume_scale` con el resto fijo (`trigger_sensitivity
--mm 50 --gradiente --volume-scale V`):

| V | flujos | huella (celdas) |
|---|---|---|
| 0,02 | 666 | 22 369 |
| 0,05 | 994 | 89 094 |
| 0,5 | 2 597 | 114 492 |
| 2 (default) | 2 597 | 122 022 |
| 20 | 2 597 | 123 252 |
| 200 | 2 597 | 123 252 |

**No es inerte**, y la causa es la muerte por volumen: en zona plana el volumen
decae `× volume_decay_flat` por paso (0,98666 en `chanaral_enhanced`) y el flujo
muere bajo `MIN_FLOW_VOLUME = 0,05`. Vida útil ≈ `ln(V₀ / 0,05) / 0,01343` pasos:
275 con `V₀ = 2`, 401 con `V₀ = 11`, 446 con `V₀ = 20`. Sobre el presupuesto de
300 pasos el parámetro deja de importar — ahí está la saturación de la tabla.
Bajo 0,05 los flujos mueren al nacer (`flujos` cae porque los hijos del spread
ya no alcanzan a nacer).

Es decir: `trigger_volume_scale` es una **longitud** (vida útil), degenerada con
`--steps` y con `volume_decay_flat`; `trigger_drops_per_km2` es una **cuenta**.
Con puntos, una huella más grande puede venir de cualquiera de los dos.

## 3. Decisión 1 — fijar `trigger_volume_scale = 2 · I₁`

En modo lluvia el volumen inicial es `2 · I` con `I` la intensidad horaria
(mm/h). Con gatillo, `E = I / I_umbral`; para que el flujo nazca con **el mismo
volumen que le daría el modo lluvia** basta `trigger_volume_scale = 2 · I₁`, con
`I₁` el umbral I-D a 1 h. Con el `a* = 5,5 mm/h` actual de nowcast: **11**.

Lo que se gana:
- el parámetro desaparece de la calibración y la degeneración con la densidad
  se cierra sin pedirle nada a los 56 puntos;
- se hereda el régimen en que se calibró Chañaral: con lluvias de 10–30 mm/h el
  modo lluvia nacía con `V₀ = 20–60` y la muerte por volumen no cortaba (el 71 %
  de los flujos seguía vivo al paso 300). Con 11·E y E ≥ 1 la vida útil es ≥ 401
  pasos > 300: mismo régimen.

Lo que se pierde: el acoplamiento volumen → largo queda inoperante dentro del
presupuesto de pasos. Es el punto 4 y va después, como se acordó. Cuando entre
el punto 4, esta decisión se revisa.

## 4. Decisión 2 — umbral de inicio con `dias_etiquetados.csv` (PU)

**Unidad:** día-estación. Positivos: 12 (3 temporales). No etiquetados: 11 785.
**Forzante:** `pp_mm`, `pp_3d_mm`; `lluvia_horaria.csv` para I-D donde hay dato.

**Método.** Para un umbral θ (sobre lámina, o sobre la curva I-D):
- recall sobre positivos, `R(θ)`;
- tasa de disparo sobre no etiquetados, `A(θ)` (tasa de alarma; no es «falsos
  positivos» porque el catálogo es incompleto).
Se entrega la curva `R` vs `A`, no un θ. La precisión no es estimable sin la
prevalencia π (PU); si hace falta un número, se da como **cota inferior** con π
tomado del catálogo (y se dice que es cota).

**Validación:** dejar un temporal fuera (3 pliegues). Resultado: banda de θ.

**Representatividad de la estación, antes que θ.** Para el mismo temporal, El
Colorado da 29 mm y La Platina 55 (E máx 0,68 vs 1,66): la elección de estación
mueve el resultado más que el umbral. Propuesta: forzante por evento = **máximo
entre estaciones a ≤ 15 km**, con análisis de sensibilidad «más cercana vs
máximo». IMERG Final como extensión para los 118 sin estación, declarando su
sesgo (pierde los convectivos de piedemonte).

**Exclusiones declaradas:** los 5 eventos con < 1 mm no entran; se reportan como
pérdidas conocidas del sistema (mecanismo convectivo que la red no ve). No son
evidencia de que «lluvia baja basta».

**Qué calibra esto en el motor.** Si el modo operativo es el gatillo,
`rain_threshold` del motor no se usa: lo que se calibra es el umbral I-D de
nowcast y el motor lo hereda vía E. `rain_threshold` solo importa para el modo
lluvia; ampliar su rango a mm/h físicos queda hecho de paso.

## 5. Decisión 3 — `trigger_drops_per_km2` como punto de operación

**Datos:** los 56 eventos con lluvia medida (puntos del catálogo).
**Montaje por evento:** ventana de nowcast, serie horaria real (según §4), E
por hora, `follow`, `trigger_volume_scale = 2·I₁`, `--steps 300` fijo.
**Barrido:** densidad ∈ {0,02; 0,05; 0,1; 0,2; 0,5; 1} gotas/km², 3 semillas.
**Métricas:** aciertos a 150 m y lift contra puntos al azar junto a cauces (como
ya hace nowcast), área de huella, y **aciertos por km² de huella** (proxy de
precisión). Entregable: curva aciertos-vs-área por temporal.
**Validación:** dejar un temporal fuera; reportar por temporal porque 2021
domina (34 de 56).
**Elección:** regla declarada antes de ver los resultados — por ejemplo «mínima
densidad que alcanza el 80 % de los aciertos del máximo» o «máximo lift». La
regla es una decisión de producto (recall vs área en alerta), no un dato.
**Cómputo:** 56 × 6 × 3 ≈ 1 000 corridas × ~7 s ≈ 2 h en una máquina.
**Reparto:** nowcast tiene los stacks por evento (`runout_ondemand`); swarm-abm
aporta el barrido y el análisis. Dos opciones: (a) nowcast exporta por evento
el stack + las grillas de E y yo corro el barrido; (b) nowcast corre
`runout_ondemand` en lote y manda los `stats.json` + huellas. (b) evita mover
datos; (a) me deja repetir con otras semillas sin pedirles nada.

**Por qué no es físico todavía.** Con E uniforme en ventanas de 12–14 km (lo que
dan GFS/ECMWF/ICON), el área excedida es todo o nada: la densidad fija la escala
de la huella y nada en los puntos la ancla. Lo que la ancla son huellas reales:
PlanetScope. Hasta entonces, es un dial con una curva de operación.

## 6. Entregables

1. Informe con curvas `R` vs `A` (umbral) y aciertos-vs-área (densidad), con
   bandas entre pliegues, por temporal. **Sin número único.**
2. Script de análisis reproducible (lee `stats.json`/huellas, no cifras a mano).
3. Recomendación explícita de si conviene esperar PlanetScope antes de fijar la
   densidad en operación.

## 7. Lo que este protocolo no va a poder afirmar

- Un umbral con incertidumbre menor que la que dan tres temporales.
- Una densidad de gotas física (solo un punto de operación).
- Nada sobre eventos convectivos.
- Nada sobre magnitud a área constante (punto 4).
