# Perfil de cómputo de SIGRID — ¿vale la pena paralelizar?

Medición acordada en la reunión con Manuel (2026-07-29): *antes de invertir más en
paralelismo, medir dónde se va el tiempo*. Este documento responde con datos.

**TL;DR.** El modelo es **97–99.7 % paralelizable**: la fase secuencial (zorros y
perros, el único término de Amdahl) **nunca supera ~3 %**. Pero a la **escala real
del estudio** una corrida completa de 30 días toma **0.1–0.7 s en serie** — no hay
nada que paralelizar. Si se escala área o densidad (corridas de ~15 s), la vía es
**OpenMP en un nodo**, no multi-nodo: no existe un cuello de cómputo secuencial que
el multi-nodo pueda atacar, y su costo es comunicación (Hito 6). Esto **confirma con
datos** la intuición de Manuel: *OpenMP antes que multinodo*.

## 1. Método

`validation/cpp-sigrid/profile.cpp` replica el bucle de tick de `run_loss_rate`
reusando el núcleo (`sigrid_core.hpp`), cronometrando cada fase con `std::chrono`:

| Fase | Qué hace | Clasificación |
|---|---|---|
| `before_step` | instantánea + counting-sort del índice espacial | paralelizable (Hito 5b) |
| **Fase A** | ovejas + liebres (leen snapshot, se escriben a sí mismas) | paralelizable (Hito 4) |
| **Fase B1** | `fox_detect` (consulta read-only sobre el snapshot) | paralelizable (Hito 4b) |
| **Fase B2** | zorros + perros: orden barajado global + mutación compartida | **secuencial** |
| término | conteo de ovejas vivas (corte anticipado) | secuencial |

Compilado **serial** (`-O3 -march=native -ffp-contract=off`, sin `-fopenmp`) para
medir el reparto de **cómputo puro**. Las **fracciones (%) son robustas a la carga**
de la máquina: todas las fases sufren la misma contención. Medido con load ≈ 2.4.

```bash
g++ -std=c++17 -O3 -march=native -ffp-contract=off profile.cpp -o profile
./profile --width 2000 --height 2000 --days 30 --seed 1000 [--dogs N] [...]
```

## 2. Resultados

| Régimen | Agentes | Serial | `before` | **Fase A** | **B1** | **B2 (secuencial)** | **Paralelizable** |
|---|---|---:|---:|---:|---:|---:|---:|
| **Screening default** (2×2 km, 30 d) | 384 ov · 34 zorros | **0.119 s** | 2.4 % | 70.1 % | 24.3 % | **3.2 %** | **96.8 %** |
| **Modelo completo** (2×2 km, 30 d, 2 perros + liebres + chillas) | 384 ov · 1200 lieb · 42 zorros · 2 perros | **0.741 s** | 2.2 % | 91.1 % | 4.7 % | **2.0 %** | **98.0 %** |
| **Área grande** (8×8 km, 30 d) | 6144 ov · 538 zorros | 14.83 s | 0.5 % | 87.9 % | 9.8 % | **1.8 %** | **98.2 %** |
| **Densidad alta** (2×2 km, 15 d, 20 ov/ha) | 8000 ov · 34 zorros | 13.85 s | 0.3 % | 98.3 % | 1.1 % | **0.3 %** | **99.7 %** |

Techo de Amdahl derivado de la fracción secuencial:

| Régimen | fracción secuencial | techo ∞ hilos | techo 16 hilos |
|---|---:|---:|---:|
| Screening default | 3.2 % | 31× | 10.8× |
| Modelo completo | 2.0 % | 51× | 12.4× |
| Área grande | 1.8 % | 57× | 12.6× |
| Densidad alta | 0.3 % | 292× | 15.2× |

## 3. Lectura

**(a) No hay cuello secuencial.** La fase B2 —los depredadores, que es justamente lo
que obliga a coordinación global y lo que el multi-nodo centraliza (Hito 6)— pesa
**≤ 3.2 %** en todos los regímenes, y **baja** al escalar (0.3 % a densidad alta).
El término de Amdahl es despreciable: el modelo es casi perfectamente paralelo.

**(b) La fase A domina siempre** (70–98 %). Las ovejas (y liebres, cuando están) son
el presupuesto de cómputo; los depredadores son ruido. Confirma lo conversado con
Manuel y la §3 del estudio event-driven: optimizar/paralelizar depredadores no mueve
la aguja.

**(c) A la escala real del estudio, no hay nada que paralelizar.** El screening de
30 días corre en **0.12 s** (0.74 s con el modelo completo). El overhead de lanzar
hilos —y con más razón de MPI— excede lo que se podría ahorrar. El paralelismo solo
pasa a ser relevante al escalar área o densidad, donde una corrida llega a ~15 s.

**(d) Matiz honesto — Amdahl no es el limitante real.** Los techos de la tabla
(11–15× a 16 hilos) salen de la fracción secuencial, pero **no se alcanzan**: al
escalar densidad la fase A es *memory-bandwidth-bound* (Hito 5), y el speedup real
medido con OpenMP es **~3× a 16 hilos**. El limitante práctico es **el bus de
memoria, no la fracción serial**. Ambos análisis apuntan al mismo lugar: memoria
compartida y layout, no distribución.

## 4. Decisión

1. **A escala real (screening): no paralelizar.** Corridas de fracciones de segundo;
   el esfuerzo rinde cero. Para campañas grandes (barridos, Sobol, calibración) el
   paralelismo natural es **por réplica** (corridas independientes en paralelo), no
   intra-modelo.
2. **Si se escala área/densidad: OpenMP en un nodo.** ~98 % paralelizable ⇒ memoria
   compartida basta. El techo práctico lo pone el ancho de banda (Hito 5 lo relajó).
3. **Multi-nodo: no se justifica por cómputo.** No hay fracción secuencial que
   distribuir; el multi-nodo solo agrega comunicación (Hito 6: P2 9.2 s → P8 18.8 s).
   Se mantiene como resultado de correctitud (bit-idéntico a cualquier P) y como base
   si el dominio creciera órdenes de magnitud.
4. **Event-driven (conservador, optimista o aproximado): sin caso.** El perfil
   refuerza `validation/mpi-sigrid/EVENT_DRIVEN_STUDY.md` — no hay tiempo muerto ni
   cuello de sincronización que justifique el cambio de paradigma.

## 5. Reproducir

```bash
cd validation/cpp-sigrid
g++ -std=c++17 -O3 -march=native -ffp-contract=off profile.cpp -o profile
./profile --width 2000 --height 2000 --days 30 --seed 1000                          # screening
./profile --width 2000 --height 2000 --days 30 --seed 1000 --dogs 2 --hare-density 3 --chilla-density 2
./profile --width 8000 --height 8000 --days 30 --seed 1000                          # área grande
./profile --width 2000 --height 2000 --days 15 --seed 1000 --sheep-density 20       # densidad alta
```
