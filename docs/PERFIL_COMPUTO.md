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

> ## ⚠️ AVISO — estas mediciones fueron superadas (2026-09-30)
>
> Las cifras de §2 y las conclusiones de §3–§4 corresponden al modelo **de agosto
> 2026**. Al re-medir el 30-sep con el modelo actual (paisaje del miedo, nacimientos
> y mortalidad de liebres, nueva lógica de selección de presa) **el resultado se
> invirtió**: la fase secuencial pasó de ≤ 3,2 % a **hasta 89,7 %**. Ver **§7**, que
> reemplaza a §2–§4. Se conserva lo anterior como registro de la medición original.

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

---

## 6. Actualización (2026-09-30) — el escenario multi-granja

En la reunión del 2026-09-30 el proyecto se reorientó a simular **muchas granjas**
(hoy: un predio de 400 ha, ~100 ovejas y ~100 zorros). El paralelismo indicado pasa a
ser **por granja** — una granja por core o por nodo (propuesta de Verónica Gil) —, que
es **paralelismo por dominio independiente**, no intra-modelo.

**El problema nuevo es el acoplamiento entre granjas.** Si fueran independientes esto
sería *embarrassingly parallel*. No lo son: hay **zorros que cruzan** entre granjas y
**ovejas que traspasan cercos** (con probabilidad calibrada según el ángulo de
aproximación; más el caso de cerco roto). A diferencia del Hito 6 —donde el
acoplamiento era *global* (la señal del perro alcanza todo el mapa) y por eso la
descomposición no pagaba— aquí el acoplamiento es **de frontera y esporádico**, que es
la condición bajo la cual la descomposición de dominio **sí** rinde.

**Falta medir:** (1) cuántas granjas hay; (2) la **tasa de cruce** de zorros por tick,
que determina el volumen de comunicación; (3) el **tamaño real por granja** (censo vía
Ana Justi).

## 7. Re-medición (2026-09-30) — la conclusión se invirtió

Al re-medir con el modelo actual **el diagnóstico cambia por completo**. Dos cosas
hubo que corregir antes de que los números fueran válidos:

- El modelo incorporó desde agosto el **paisaje del miedo** (`compute_fox_fear`),
  los **nacimientos y mortalidad de liebres** (fase C) y una **nueva lógica de
  selección de presa**.
- **`profile.cpp` estaba desactualizado**: no ejecutaba la fase C, así que las liebres
  no se reproducían y la dinámica perfilada no era la del modelo. Se corrigió y se le
  agregó un **control de equivalencia** que imprime `loss_rate`/matadas/intentos para
  contrastar contra `sheep_fox.cpp` (deben coincidir).

### Resultados (modelo actual, serial, `-ffp-contract=off`)

| Régimen | Serial | `before` | Fase A | B1 | **B2 (sec.)** | Fase C | **Secuencial** | **Paraleliz.** | Amdahl 16h |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| Screening 2×2 km | 0,275 s | 1,7 % | 56,9 % | 11,0 % | **30,1 %** | 0,3 % | **30,4 %** | 69,6 % | **2,9×** |
| Modelo completo 2×2 km | 1,425 s | 9,8 % | 79,9 % | 6,0 % | 2,5 % | 0,3 % | **4,4 %** | 95,6 % | 9,7× |
| Densidad alta (20 ov/ha) | 61,9 s | 0,1 % | 28,4 % | 0,2 % | **71,2 %** | 0,0 % | **71,2 %** | 28,8 % | **1,4×** |
| Área grande 8×8 km | 189,3 s | 0,1 % | 9,6 % | 0,7 % | **89,6 %** | 0,0 % | **89,7 %** | 10,3 % | **1,1×** |

Contraste con agosto: la fracción secuencial pasó de **3,2 % → 30,4 %** (screening),
**1,8 % → 89,7 %** (área grande) y **0,3 % → 71,2 %** (densidad alta). El costo
absoluto del área grande subió de **14,8 s a 189,3 s** (12,8×).

### La causa: un bucle O(presas²) en la fase secuencial

En `step_fox`, la selección de presa calcula el **aislamiento** de cada candidata
contando congéneres a menos de 50 m **recorriendo toda la lista de presas**:

```cpp
for (int pid : prey) {                 // presas detectadas (radio 300 m)
    int nearby = 0;
    for (int qid : prey) { ... if (dist2 < 50*50) nearby++; }   // O(p) dentro de O(p)
    score += (1.0 - min(nearby/10.0, 1.0)) * FOX_ISOLATED_BONUS;
}
```

Es **O(p²) por zorro**, con *p* = presas dentro de 300 m. A densidad alta o área
grande, *p* crece y el término cuadrático domina — y está en la **fase B2, que es
secuencial**, así que OpenMP no lo alivia.

### Qué implica

1. **Ahora sí existe un cuello secuencial, y domina al escalar** (hasta 89,7 %). El
   techo de Amdahl cae a **1,1–1,4×**: agregar hilos casi no sirve en los regímenes
   grandes.
2. **Antes de cualquier decisión de paralelismo, conviene atacar el O(p²).** Es
   probablemente la mejor relación esfuerzo/beneficio del proyecto ahora mismo:
   - El cálculo de aislamiento es **read-only sobre las presas** ⇒ puede **precomputarse
     en la fase B1**, que **ya es paralela** (igual que se hizo con `fox_detect` en el
     Hito 4b).
   - O reemplazar el bucle anidado por una consulta al **índice espacial** ya existente
     (el mismo `grid` que usa `fox_detect`), pasando de O(p²) a ~O(p).
   - Cualquiera de las dos debería devolver la fracción secuencial a valores bajos y
     **restaurar el techo de Amdahl**.
3. **Las conclusiones de §4 quedan en suspenso** hasta resolver (2). En particular, el
   argumento que descartaba distribuir la fase B —"sería un build grande para repartir
   un 2 % del tiempo"— **ya no aplica**: hoy esa fase pesa hasta 89,7 %. Pero antes de
   rediseñar arquitectura conviene comprobar cuánto de ese 89,7 % es simplemente el
   bucle cuadrático.

> **Recomendación:** optimizar el aislamiento (mover a B1 o usar el grid), **re-medir**,
> y recién entonces revisar si la decisión de arquitectura cambia. No propagar las
> cifras de §2 a documentos nuevos.

---

## 8. Re-medición tras el fix del O(p²) (2026-10-19) — cuello cuadrático resuelto

Se implementó la recomendación de §7. **Hallazgo que corrige la hipótesis de §7**:
el conteo de aislamiento **no puede precomputarse en B1**, porque lee el estado
`alive` **vivo** de las presas, que cambia dentro de B2 a medida que los zorros
matan. Precomputarlo sobre la instantánea (todas vivas) cambiaría la selección.

El fix queda **dentro de B2** y es bit-idéntico: para listas de >64 presas se
agrupan las presas del zorro en un hash espacial de 50 m (`thread_local`,
reutilizado; listo para cuando B2 se paralelice por granja) y cada candidata
consulta su vecindad 3×3, pasando de O(p²) a O(p·k); para listas chicas se
conserva el escaneo directo (más barato que armar el índice). Código:
`sigrid_core.hpp::step_fox`. Hook `-DISO_DIRECT_MAX_OVERRIDE` para rehacer el A/B.

### Verificación de bit-identidad

- A/B directo (= original) vs hash, mismo binario: **6/6 configuraciones
  byte-idénticas**, incluyendo 20 ov/ha y 8×8 km (donde el hash se ejercita).
- Golden `serial == OMP(1/4/16)`: **5/5** (`verify_golden_fear.sh`).
- Métricas de control (`loss_rate`/matadas/intentos) idénticas en todos los brazos.

### A/B limpio de rendimiento (mismo `profile.cpp`, solo cambia `ISO_DIRECT_MAX`)

Densidad alta, 2×2 km, 15 d, 20 ov/ha (8000 ovejas · 34 zorros):

| | O(p²) original | Hash (fix) | |
|---|---:|---:|---|
| Total | 44.9 s | **23.5 s** | 1.9× |
| Fase B2 (zorros) | 31.86 s (71.0%) | **9.73 s (41.4%)** | 3.3× |
| Fracción secuencial | 71.0% | **41.5%** | |
| Techo de Amdahl (∞) | 1.4× | **2.4×** | |
| loss_rate / matadas / intentos | 0.89% / 71 / 542 | 0.89% / 71 / 542 | idéntico ✓ |

### Respuesta a la pregunta abierta de §7

*"¿Cuánto del 89.7% es el bucle cuadrático?"* → depende del régimen:

- **Densidad alta** (p grande, el hash actúa): el O(p²) valía ~30 pp de la
  fracción secuencial; al quitarlo, el techo de Amdahl sube de 1.4× a 2.4×.
- **Área grande, baja densidad** (8×8 km, p≈27 < 64, camino directo): el O(p²)
  **no** era el driver; B2 sigue alto (~69%) por otro costo por-zorro. El fix no
  lo toca (correctamente); queda como follow-up si ese régimen pasa a importar.

**Implicación:** en los regímenes de alta densidad la fracción secuencial deja de
estar dominada por el aislamiento, así que este cuello ya no sesga la decisión de
arquitectura (BSP multi-granja). El siguiente candidato de optimización es el
costo por-zorro residual de B2 en baja densidad.
