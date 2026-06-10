# Paridad numérica: swarm-core (Rust) vs Mesa (Python)

Réplicas por motor: **50** (semillas 0..49). Test z de dos muestras por métrica, α = 0.05.

**Veredicto: ✅ PARIDAD NUMÉRICA CONFIRMADA**

## Schelling (50×50, densidad 0.85, tolerancia 0.375)

| Métrica | swarm-core | Mesa | z | Paridad |
|---|---|---|---|---|
| Pasos hasta converger | 11.4200 | 11.4600 | -0.11 | ✅ PASS |
| Similitud media final | 0.7714 | 0.7716 | -0.13 | ✅ PASS |
| Fracción conforme inicial | 0.7703 | 0.7698 | +0.29 | ✅ PASS |

Curvas (medias de ensamble, horizonte 30 pasos, relleno con último valor):

| Serie | max \|Δ media de ensamble\| |
|---|---|
| fraccion_conforme | 0.0011 |
| similitud_media | 0.0011 |

## SIR espacial (50×50 lleno, β=0.08, γ=0.1, 5 infectados iniciales)

| Métrica | swarm-core | Mesa | z | Paridad |
|---|---|---|---|---|
| Pico de infectados (fracción) | 0.2329 | 0.2346 | -0.29 | ✅ PASS |
| Paso del pico | 42.9600 | 44.3400 | -0.91 | ✅ PASS |
| Tamaño final de la epidemia (R) | 0.9906 | 0.9912 | -1.22 | ✅ PASS |
| Duración (pasos) | 126.1200 | 127.5800 | -0.47 | ✅ PASS |

Curvas (medias de ensamble, horizonte 300 pasos, relleno con último valor):

| Serie | max \|Δ media de ensamble\| |
|---|---|
| s | 0.0208 |
| i | 0.0085 |
| r | 0.0184 |

Generado por `validation/compare.py` (datos en `validation/data/`, espejos Mesa en `validation/mesa/`).
