# ¿Más física mejora el ajuste? El ciclo modelo–datos en Chañaral

El motor vuelve barato algo que en Python era inviable: **un ciclo completo de
mejora del modelo** —diagnosticar el error, hipotetizar el mecanismo faltante,
implementarlo, recalibrar, validar— en una tarde en vez de semanas. Esta es la
historia, incluidos los callejones sin salida.

## Punto de partida

El modelo base reproduce Config B (IoU 0.468 ± 0.0004 fuera de muestra), pero
con recall 0.59: ~40 % del área real no se captura.

## Intento 1 — física genérica (no funcionó)

Se añadieron tres mejoras estándar de la literatura de debris flows como
términos calibrables (`EnhancedPhysics`): **entrainment/bulking**, **reología
de Voellmy** y **inercia direccional**. Calibradas con DE (objetivo robusto
media − sd, validación fuera de muestra):

| Modelo | IoU medio | sd | máx | precision |
|---|---|---|---|---|
| Base | 0.4684 | 0.0004 | 0.469 | 0.690 |
| +física genérica | 0.4597 | 0.0275 | 0.476 | 0.715 |

**Empate con trade-offs, no mejora.** Añadir física a ciegas no ayudó. (Y un
primer intento con objetivo media simple sobre 3 semillas *sobreajustó*
—IoU fuera de muestra 0.31—; penalizar la varianza lo corrigió. Lección de
calibración: más grados de libertad ⇒ exigir robustez en el objetivo.)

## El diagnóstico — dónde falla el modelo

En vez de seguir añadiendo física, se **mapeó el error** dentro del bbox
urbano (`--dump` del footprint + análisis espacial):

| | n | elevación (mediana) | en cauce |
|---|---|---|---|
| Aciertos (TP) | 1171 | 36 m | 34 % |
| Falsos + (FP) | 529 | 62 m (laderas) | 1 % |
| Falsos − (FN) | 802 | **5 m (planicie baja)** | 5 % |

**El 77 % de los falsos negativos está en las cotas más bajas (mediana 5 m):**
la planicie urbana costera donde el debris flow real desconfinó del cauce y se
**esparció en abanico**, depositando sobre la ciudad. El modelo no cubría esa
deposición lateral — un mecanismo faltante, no ruido. (Además: el raster de
sedimento vale 0 en todo el bbox, lo que explica por qué el entrainment no
podía ayudar — límite de datos.)

## Intento 2 — física dirigida por el diagnóstico (funcionó)

Se añadió **expansión en abanico**: al entrar en la planicie de baja pendiente,
el radio de deposición del flujo crece (`fan_factor`), esparciendo
lateralmente justo donde estaban los FN. Recalibrado (16 parámetros):

| Modelo | IoU medio | sd | máx | precision | recall | F1 |
|---|---|---|---|---|---|---|
| Base | 0.4684 | 0.0004 | 0.469 | 0.690 | 0.593 | 0.638 |
| **+abanico (dirigido)** | **0.5081** | 0.065 | **0.5310** | 0.595 | **0.789** | **0.672** |

**+8.5 % en IoU medio, recall de 0.59 → 0.79.** Y el mapa de error lo confirma:
los FN de zona baja cayeron **74 %** (620 → 161), TP subió de 1171 a 1654.

Más aún, la calibración **apagó las mejoras genéricas** que no servían
(`max_bulking = 1` anula el entrainment; `inertia_weight = 0`) y subió el
abanico al tope (`fan_factor = 6`). De toda la física disponible, el optimizador
seleccionó exactamente la que el diagnóstico predijo.

## Conclusión

- **Física a ciegas: no.** Física **dirigida por el diagnóstico del error: sí**
  (+8.5 % IoU, mejora robusta y validada fuera de muestra).
- El cuello de botella nunca fue la idea, sino el cómputo: este ciclo
  (diagnóstico → hipótesis → implementación → recalibración → validación) son
  miles de simulaciones. El motor lo hizo en una tarde; en el flujo Python
  original habría sido inviable.

Reproducir:

```bash
cargo run --release -p debris-flow -- --preset chanaral          --steps 500 --seed 100 --seeds 8   # base 0.468
cargo run --release -p debris-flow -- --preset chanaral-enhanced --steps 500 --seed 100 --seeds 8   # abanico 0.508
cargo run --release -p debris-flow --bin calibrate_chanaral -- --pop 32 --gens 28 --eval-seeds 5    # recalibrar
```
