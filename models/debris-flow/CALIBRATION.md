# Calibración masiva del modelo de flujos de detritos

El port a Rust no solo reproduce el modelo original: lo vuelve **calibrable
en serio**. Con el código Python (~60 s por corrida en esta configuración,
sin reproducibilidad) una calibración de cientos de evaluaciones era
impráctica. Sobre el port —stack cargado una vez y compartido por `Arc`,
población evaluada en paralelo (rayon, 16 hilos), superficie objetivo
estable (RNG sembrado)— es cuestión de minutos.

## Método

`src/bin/calibrate.rs`: **Differential Evolution** (DE/rand/1/bin, F=0.6,
CR=0.9) sobre los 15 parámetros continuos del modelo, con los mismos rangos
que la calibración Optuna original. Objetivo: maximizar el IoU contra el
ground truth de Copiapó (44.8 km²), con 50 agentes × 300 pasos.

```bash
# Calibración robusta (objetivo = IoU medio sobre 3 semillas):
cargo run --release -p debris-flow --bin calibrate -- \
    --pop 32 --gens 20 --eval-seeds 3
# Validar el preset resultante en semillas dentro y fuera del ajuste:
cargo run --release -p debris-flow -- --preset de --steps 300 --seeds 8
```

## Throughput

| Calibración | Simulaciones | Tiempo (Rust, 16 cores) | Equivalente Python secuencial |
|---|---|---|---|
| single-seed (672 evals) | 672 | ~70 s | ~11 h |
| robusta (672 evals × 3 seeds) | 2016 | ~5 min | ~34 h |

Lo que en el flujo Python era una calibración de un día (o inviable) aquí
cabe en la pausa de un café.

## Resultado y un hallazgo metodológico

Una primera calibración con **una sola semilla** de evaluación alcanzó IoU
0.174… pero al validar en otras semillas cayó a 0.113 ± 0.034: **sobreajuste
al ruido** de esa realización (colocación de agentes + softmax). El síntoma
delator: eligió temperatura estocástica T≈1.81 (mucha dispersión aleatoria
que "ayudaba" solo en esa semilla).

La corrección —objetivo robusto promediando 3 semillas— colapsó la
temperatura a T≈0.02 (casi determinista) y generaliza:

| Configuración | IoU medio (8 semillas, incl. fuera de ajuste) |
|---|---|
| Default Optuna-T (del original) | 0.074 ± 0.026 |
| **DE robusto (preset `de`)** | **0.158 ± 0.026** |

**~2.1× de mejora en IoU medio**, con desviación estrecha y sin colapso
fuera de muestra (la media en validación, 0.158, queda junto al óptimo de
ajuste 0.162). Que el motor rápido permita *detectar y corregir* el
sobreajuste —no solo encontrar un número alto— es la verdadera ganancia.

Parámetros en `data/best_params_de.json`; embebidos como `Params::preset_de()`.

## Honestidad sobre el IoU absoluto

El IoU sigue siendo modesto (~0.16): el modelo sobre-predice área (recall
alto, precision baja). Eso es propiedad del **modelo** de detritos, no del
motor ni del optimizador; la contribución aquí es metodológica —calibración
robusta, reproducible y ~400× más rápida— no un cambio en la física del
modelo.
