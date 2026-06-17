# ¿Más física mejora el ajuste? Un experimento honesto en Chañaral

El motor vuelve barato algo que en Python era inviable: **probar física más
realista y medir si vale la pena**, en vez de quedarse con el modelo simple
que toleraba el tiempo de cómputo. Aquí el experimento completo.

## Las tres mejoras físicas

Sobre el modelo base (que ya reproduce Config B, IoU 0.468) se añadieron tres
términos calibrables (`EnhancedPhysics`), cada uno justificado en la
literatura de debris flows:

1. **Entrainment / bulking** — el volumen *crece* arrastrando sedimento en
   pendiente erosiva (`×(1 + k·sedimento)`, con tope), y *deposita* al perder
   pendiente. El modelo base solo decaía.
2. **Reología de Voellmy** — velocidad por fricción de Coulomb (μ) + término
   turbulento (`v²/ξ`), estándar en avalanchas/debris (RAMMS), en vez de
   `gravedad − drag`.
3. **Inercia direccional** — premia seguir la dirección acumulada del flujo
   (producto punto), evitando giros bruscos a la máxima pendiente local.

14 parámetros (8 base relevantes + 6 físicos), calibrados con Differential
Evolution sobre el port (`bin/calibrate_chanaral`).

## Lo que pasó — y una lección metodológica

**Primer intento (objetivo = media de 3 semillas):** IoU pico 0.475, pero
fuera de muestra **0.314 ± 0.183**. La física extra, con más grados de
libertad, **sobreajustó** a las 3 semillas de calibración. Sin el motor
rápido este fracaso habría costado días; aquí, minutos.

**Corrección (objetivo = media − desviación estándar, 5 semillas):**
penalizar la varianza obliga a soluciones robustas. Resultado fuera de
muestra (8 semillas frescas):

| Modelo | IoU medio | sd | máx | precision | recall | F1 |
|---|---|---|---|---|---|---|
| Base (Config B) | **0.4684** | **0.0004** | 0.4692 | 0.690 | 0.593 | **0.638** |
| Enriquecido (calibrado robusto) | 0.4597 | 0.0275 | **0.4757** | **0.715** | 0.563 | 0.629 |

## Veredicto (honesto)

La física adicional **no mejora robustamente** el ajuste espacial:

- Alcanza un **pico** algo superior (0.476 vs 0.469) y **mejor precision**
  (0.715 vs 0.690) — predice un área más certera.
- Pero su **media es menor** (0.460 vs 0.468) y su **estabilidad mucho peor**
  (±0.027 vs ±0.0004): el modelo base es extraordinariamente consistente.

Es un **empate con trade-offs**, no una mejora. Los tres términos quedan
activos en el óptimo (entrainment 0.12, inercia en el tope del rango,
Voellmy μ=0.30 / ξ=3341), lo que indica que *contribuyen* — pero el techo del
ajuste parece estar limitado por los datos (resolución del DEM, calidad del
raster de sedimento disponible), no por la simplicidad del modelo.

**Esto es un resultado publicable por sí mismo:** en debris flows, "más
física" no compró mejor ajuste para este caso, y el motor permitió
establecerlo con rigor (calibración robusta + validación fuera de muestra)
en una tarde — el tipo de experimento que el costo de Python desalentaba.

Reproducir:

```bash
cargo run --release -p debris-flow --bin calibrate_chanaral -- \
    --pop 28 --gens 25 --steps 500 --eval-seeds 5
cargo run --release -p debris-flow -- --preset chanaral --steps 500 --seed 100 --seeds 8
```
