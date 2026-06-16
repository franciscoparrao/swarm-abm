# Paridad: port Rust (swarm-core) vs debris-flow-abm original (Mesa/Python)

## El mejor caso: Chañaral (Config B) — paridad <1%

El caso de referencia más fuerte del repositorio original es Chañaral, donde
el modelo se **calibró** (Config B, `config_b_final_params.json`): IoU 0.4653.
El port lo reproduce sobre el mismo stack (DEM FABDEM con océano enmascarado,
ground truth + bbox urbano rasterizados, evaluación sobre el bbox):

| Métrica | Python (Config B) | Rust port (moda, 8 semillas) |
|---|---|---|
| IoU | 0.4653 | **0.4684** |
| precision | 0.673 | 0.690 |
| recall | 0.602 | 0.593 |
| F1 | 0.635 | 0.638 |
| n flujos | 384 | 384 |
| afectado∩bbox (px) | 1973 | 1973 |

Diferencia en IoU **< 1 %**, dentro del ruido de la colocación aleatoria de
agentes (numpy global sin semilla en el original vs `SimRng` sembrado en el
port). El conteo de flujos y el dominio de evaluación coinciden exactamente.
Reproducir: `cargo run --release -p debris-flow -- --preset chanaral --steps 500 --seeds 8`
(tras `python3 models/debris-flow/prepare_chanaral.py`).

> Detalle de modelado: Config B trae `radius: 4.0` y no calibra
> `stochastic_temperature` → es la física de **radio fijo determinista**
> (`Physics::Copiapo`), no la variante coastal de radio dinámico (también
> portada, `Physics::Coastal`). Confundir las dos sobre-predice el área ~4×.
> Y en una cuenca costera, usar el DEM **sin** enmascarar el océano derrama
> los flujos por la zona urbana: ambos detalles fueron necesarios para la
> paridad.

## Protocolo

El modelo original (`simulate_copiapo.py`, V4 HYBRID v2) se ejecutó **sin
modificar** sobre exactamente los mismos insumos que el port
(`run_reference.py` extrae las clases del script original y solo reemplaza
la lectura de rasters por el stack `.f32` compartido). Misma grilla
(5871×5422 @ 30 m, evento Atacama marzo 2015, Copiapó), misma ventana de
evaluación y mismo ground truth (44.8 km²). El original usa el RNG global de
numpy sin semilla — no es reproducible — así que la paridad es
**distribucional**: varias corridas por motor.

## Resultados — preset `18iters` (100 agentes, 500 pasos, T=0)

| Motor | IoU (rango) | Área pred (km²) | Flujos | Movs/flujo | Tiempo/corrida |
|---|---|---|---|---|---|
| Python original (n=3) | 0.035 – 0.085 | 68 – 82 | 675 – 980 | 218 – 235* | 128 – 239 s |
| Rust port (n=5) | 0.056 – 0.088 | 67 – 86 | 596 – 965 | 203 – 237 | 1.2 – 4.1 s |

\* historia media (incluye el punto inicial).

Las distribuciones se superponen en todas las métricas: IoU, área, número
de flujos y longitud de trayectoria. **Speedup ~100×** en la fase de
simulación.

## Resultados — preset `optuna-t` (50 agentes, 300 pasos, T=0.285)

Ejercita la rama de selección softmax con temperatura estocástica:

| Motor | IoU (rango) | Área pred (km²) | Flujos | Tiempo/corrida |
|---|---|---|---|---|
| Python original (n=3) | 0.054 – 0.124 | 122 – 155 | 217 – 225 | 52 – 74 s |
| Rust port (n=5) | 0.026 – 0.096 | 95 – 205 | 162 – 311 | 1.4 – 2.0 s |

Distribuciones solapadas también en la rama estocástica. El IoU histórico
archivado (0.1344) cae dentro del rango que produce el propio código Python
con esta configuración (mejor corrida observada aquí: 0.1236).

## Nota sobre las métricas históricas

Los JSON de calibración archivados (IoU 0.1344 / 0.1455, área 28 km²) **no
se reproducen ni con el código Python original** sobre el stack actual:
provienen del entorno de calibración de 2024-2025 (posiblemente otra versión
del raster de susceptibilidad o del recorte). La vara de paridad correcta es
el comportamiento del código original sobre insumos idénticos, que es lo que
esta comparación mide.

## Diferencias intencionales del port

1. **Reproducibilidad**: el port usa el `SimRng` sembrado del motor para el
   softmax y la colocación de agentes; el original usa `np.random` global
   sin semilla. Misma semilla → mismo footprint, bit a bit.
2. **Bajas explícitas**: los flujos muertos se eliminan del `AgentSet`
   (baja diferida en `after_step`); el original los dejaba inactivos en el
   scheduler para siempre.
3. **Código muerto no portado**: el "flat fallback" de `search_in_radius`
   era inalcanzable en el original (tras `if len(candidates) == 0: return
   None`).

## Reproducir

```bash
python3 models/debris-flow/prepare_data.py          # GeoTIFF -> .f32 (GDAL)
cargo run --release -p debris-flow -- --preset 18iters --steps 500 --seeds 5
validation/.venv/bin/python models/debris-flow/run_reference.py \
    --preset 18iters --agents 100 --steps 500 --runs 3
```
