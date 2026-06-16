# Benchmark de metaheurísticas para calibrar el modelo de detritos

Comparación de 5 metaheurísticas (las mismas familias que el
paper original) sobre el port Rust, con **el rigor que el costo de Python
impedía**: cada método se corrió de forma independiente con el mismo
presupuesto de evaluaciones, y se comparan las distribuciones de IoU.

- Corridas independientes por método: **10**
- Métrica: IoU máximo alcanzado por corrida (objetivo de optimización)

## Ranking (IoU medio sobre corridas, mayor es mejor)

| Método | IoU medio | sd | mejor | rango medio |
|---|---|---|---|---|
| GWO | 0.1465 | 0.0123 | 0.1698 | 1.80 |
| PSO | 0.1398 | 0.0153 | 0.1717 | 2.50 |
| SA | 0.1290 | 0.0276 | 0.1587 | 2.80 |
| DE | 0.1228 | 0.0113 | 0.1440 | 3.80 |
| GA | 0.1213 | 0.0087 | 0.1406 | 4.10 |

## Test de Friedman (ómnibus)

χ² = 14.320, p = 0.0063 → diferencias entre métodos: **SÍ** (α = 0.05).

## Wilcoxon signed-rank por pares (p ajustado por Holm)

| Par | p (Holm) | significativo |
|---|---|---|
| DE vs GWO | 0.0391 | sí |
| GA vs GWO | 0.0391 | sí |
| GA vs PSO | 0.0469 | sí |
| DE vs PSO | 0.0684 | no |
| SA vs GWO | 0.7852 | no |
| PSO vs GWO | 0.9668 | no |
| DE vs GA | 1.0000 | no |
| DE vs SA | 1.0000 | no |
| GA vs SA | 1.0000 | no |
| PSO vs SA | 1.0000 | no |

## Lectura

Mejor método por IoU medio y rango: **GWO**. Friedman detecta diferencias globales; ver pares significativos arriba.

El valor del motor: este estudio son miles de simulaciones; en el flujo Python original (una corrida por método, presupuestos recortados por memoria) era inviable afirmar nada con respaldo estadístico.

Generado por `validation/calibration_benchmark.py` desde `benchmark.csv`.
