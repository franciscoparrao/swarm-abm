# SIGRID en C++ — port de producción, validado contra swarm-abm (oráculo)

Implementación en C++ del modelo de ovejas SIGRID, para la vía de escala
(OpenMP → BSPonMPI) que pide el objetivo del proyecto. **swarm-abm es el oráculo
de referencia**: el C++ se valida contra él por paridad distribucional. Ver el
plan completo en `../../docs/PLAN_PORT_CPP_SIGRID.md`.

## Estado: Hitos 1–6 completos — screening + OpenMP + layout memory-bound + BSPonMPI multi-nodo

> El núcleo del modelo (constantes, structs, RNG, Grid, `Model`, funciones de step,
> `build_model`) vive ahora en **`sigrid_core.hpp`**, fuente única compartida por la
> versión OpenMP (`sheep_fox.cpp`) y la versión BSPonMPI multi-nodo
> (`../mpi-sigrid/`, Hito 6). Para bit-identidad **entre binarios** (serial/OMP/BSP)
> compilar con `-ffp-contract=off` (ver la cabecera de `sigrid_core.hpp`).

Las cuatro especies (oveja, zorro, perro guardián, liebre, chilla) están
portadas y validadas contra el oráculo swarm-abm, y la versión paralela (OpenMP)
es **determinista** y sigue validando. `sheep_fox.cpp` reproduce el
comportamiento del modelo de screening en todo el espacio de parámetros.

### Hito 4 — OpenMP determinista (un nodo)

Paralelización en dos fases: **fase A (paralela)** = ovejas y liebres, que son
independientes (leen la instantánea congelada + rasters y se escriben a sí
mismas); **fase B (secuencial)** = zorros y perros, que mutan estado compartido
(matanzas, contadores, zonas de peligro). Cada agente usa un **RNG por-agente**
sembrado por `(semilla, paso, id)` (la idea de `child_rng`), lo que hace el
resultado **independiente del orden de ejecución**.

- **Determinismo**: 1, 4 y 16 hilos dan el **mismo hash bit a bit** (serial ==
  paralelo, sin importar el número de hilos). Es el problema difícil del ABM
  paralelo, resuelto.
- **Speedup** (8×8 km, densidad default, pocos zorros, compute-bound):

  | hilos | 1 | 2 | 4 | 8 | 16 |
  |---|---|---|---|---|---|
  | speedup | 1,00× | 1,34× | 1,88× | 2,50× | 3,07× |

- **Correctitud preservada**: la reestructuración (RNG por-agente + fase split)
  **sigue validando** contra el oráculo — Pearson **0,9976**, RMSE 2,1 pp sobre
  el barrido (densidades, perros, liebres, chillas). El oráculo confirmó que la
  paralelización no rompe el comportamiento.

### Hito 4b — detección de zorros en paralelo

La detección de presas del zorro es una query espacial **read-only sobre el
snapshot**, así que se precomputa en una fase paralela (`fox_detect`) antes de la
fase secuencial de selección/ataque/mutación. Es **bit-idéntico** a Hito 4 (mismo
hash) — solo cambia el orden de cómputo, no la semántica, así que la paridad con
el oráculo (0,9976) se mantiene automáticamente.

**Ganancia modesta y por qué** (hallazgo honesto): en un caso fox-heavy (8×8 km,
~538 zorros, ~6144 ovejas), a 16 hilos el speedup sube de **2,71× a 3,08×**
(~13%). Poco, porque **las ovejas dominan el conteo de agentes y ya estaban
paralelas** en Hito 4; los zorros nunca fueron el cuello principal. El límite real
no era la serialidad de los depredadores sino el ancho de banda de memoria (abajo)
y la fase secuencial de mutación. Para subir sustancialmente habría que atacar el
layout de datos (SoA, `Snap` más chico) antes que paralelizar más agentes.

**Límites honestos del speedup** (hallazgos, no fallas — exactamente lo que el
prof. Marín anticipó):
1. **Memory-bandwidth**: a densidad alta en área fija, las consultas de vecindad
   recorren miles de vecinos y saturan el ancho de banda → agregar hilos no ayuda
   (mismo techo memory-bound que swarm-abm, ~1,6×). Por eso el speedup se mide
   escalando el **área** a densidad constante (queries chicas, compute-bound).
2. **Fase secuencial de depredadores**: los zorros escalan con el área y están en
   la fase B secuencial → cuello de Amdahl. Para ir más allá de ~3× hay que dar a
   los zorros tratamiento de dos fases (decide paralelo / apply secuencial con
   resolución de conflictos de matanza) — Hito 4b.

Compilar: `g++ -std=c++17 -O3 -march=native -fopenmp sheep_fox.cpp -o
sheep_fox_omp`. Sin `-fopenmp` compila la versión serial idéntica.

### Hito 5 — layout memory-bound (el techo real, atacado sin cambiar resultados)

El índice espacial dejó de ser una **lista ligada** (`head`/`nxt`) sobre structs
de 48 B y pasó a un **counting-sort por celda (CSR + SoA)**: los agentes quedan
contiguos por celda (`cellStart[c]..cellStart[c+1]`), con las coordenadas
calientes `(x,y)` en un arreglo aparte de los campos fríos. El barrido de
vecindad toca solo 16 B por candidato de forma **secuencial** (amigable al
prefetcher) en vez de saltar 48 B por punteros, y los campos fríos se reúnen solo
en los aciertos. Además, la consulta de forrajeo de la oveja —la más caliente—
usaba radio 500 m (que solo existe para ver perros); **sin perros en el modelo se
reduce a 100 m** (`cr` 9→2, ~14× menos celdas), lo cual es lícito porque los
agentes extra que el barrido a 500 tocaba se descartaban igual.

**Clave metodológica — bit-idéntico**: el orden de visita se preservó exacto
(índice descendente dentro de celda, mismo recorrido de bloque), así que la
optimización **no cambia ni un bit** del resultado. Verificado: `serial` y `omp`
dan el **mismo hash** que Hito 4b en todo el barrido de screening (ovejas, zorros,
perros, liebres, chillas), y el determinismo por hilos (1/4/8) se mantiene. Como
no cambia la semántica, la paridad con el oráculo (Pearson 0,9976) se hereda sin
re-validar — es un cambio de layout, no de modelo.

**Por qué importa el régimen**: en un solo hilo a tamaños factibles la
instantánea cabe en L2 (i7-1270P, L2 1,25 MB), así que ahí el layout no cambia el
tiempo (0 % de regresión, 0 % de ganancia — no es memory-bound). La ganancia
aparece en el régimen que el prof. Marín señaló: **densidad alta en área fija**,
donde cada consulta recorre miles de vecinos. Medido por `user`-CPU-time (robusto
a la carga de la máquina compartida), 800×800 m a 40 ovejas/ha:

| caso | base | Hito 5 | speedup |
|---|---:|---:|---:|
| con perros (solo SoA, guarda inactiva) | 3,73 s | 2,97 s | **1,26×** |
| sin perros (SoA + guarda de radio) | 4,74 s | 1,06 s | **4,5×** |

Es decir: el layout SoA/CSR da ~1,25× en el régimen memory-bound, y la reducción
de radio de la consulta caliente lleva el caso de screening típico (sin perros) a
~4,5×. Todo bit-idéntico.

**Escalamiento paralelo — el techo de ancho de banda se relajó** (lo que
anticipaba Hito 4b). Con la máquina descargada, régimen memory-bound (1200×1200 m,
40 ovejas/ha, 6 días), min-of-5, wall-clock, i7-1270P (4 P-cores/8 hilos +
8 E-cores):

| hilos | BASE 4b + perros | HITO 5 + perros (aísla SoA) | HITO 5 sin perros |
|---:|---:|---:|---:|
| 1  | 16,86 s (1,00×) | 11,66 s (1,00×) | 3,16 s (1,00×) |
| 4  | 7,81 s (2,15×)  | 5,77 s (2,02×)  | 1,93 s (1,63×) |
| 8  | **8,24 s (2,04×)** | 5,09 s (2,28×) | 1,34 s (2,36×) |
| 16 | 5,85 s (2,88×)  | 3,93 s (2,96×)  | 1,08 s (2,92×) |

Dos lecturas:

1. **BASE 4b regresa a 8 hilos** (8,24 s > 7,81 s de 4 hilos): es el techo de ancho
   de banda del que hablaba Hito 4b — con structs de 48 B recorridos por punteros,
   8 hilos saturan el bus antes de aprovechar los núcleos. HITO 5 escala **monótono**
   (5,77 → 5,09 → 3,93): al streamear 16 B contiguos por candidato, el bus ya no es
   el cuello y sumar hilos vuelve a ayudar. El SoA **relajó el techo**.
2. **Throughput absoluto: HITO 5 gana a cada conteo de hilos.** Solo-SoA (mismo
   trabajo, con perros) es **1,45× a 1 hilo y 1,49× a 16**. En el caso de screening
   real (sin perros), HITO 5 a 16 hilos es **5,4× más rápido** que el mejor del
   BASE 4b — y HITO 5 a *un* hilo (3,16 s) ya **le gana al BASE a 16 hilos** (5,85 s).

Nota sobre el ratio de speedup: HITO 5 tiene una base serial ya ~1,45× más rápida,
así que su fracción paralelizable es menor (Amdahl) y el *ratio* no infla — pero el
*tiempo absoluto* es mejor en todos los puntos, que es lo que importa.

### Hito 5b — counting-sort del índice paralelo (bit-idéntico)

El `build` del índice (el counting-sort que se rehace cada tick) era la parte
serial que quedaba en el paso (el término de Amdahl citado arriba). Ahora es
**paralelo** manteniendo la bit-identidad, que es el punto no trivial: un
counting-sort paralelo ingenuo cambia el orden intra-celda y rompería la
igualdad. La técnica:

1. **Histogramas locales por hilo** sobre chunks contiguos de índice (fase
   paralela).
2. **Offsets por (hilo, celda)** calculados de modo que los chunks de índice
   *mayor* ocupen los primeros slots de cada celda, y cada hilo scatterea su
   chunk en orden **descendente** (fase paralela). La concatenación
   `[chunk T-1 desc][T-2 desc]…[0 desc]` da índice global descendente **para
   cualquier número de hilos** — exactamente el orden serial (== lista ligada de
   Hito 4b).

Verificado: `serial` y `omp` dan el **mismo hash** que Hito 5 en todo el barrido
de screening, y el determinismo por hilos (1/4/16) se mantiene. Un **camino
rápido para T==1** usa el counting-sort simple (sin el overhead de los
histogramas por-hilo), así que no hay regresión a un hilo.

Dónde ayuda: cuando el `build` es una fracción no trivial del paso —régimenes
dispersos / de pocas queries por agente—. En el régimen de **densidad alta** el
`build` es una fracción pequeña (domina la fase A de las ovejas), así que ahí el
efecto es ~neutro; la medición limpia de wall-clock paralelo en régimen
build-dominado quedó pendiente porque la estación compartida volvió a carga alta
(≈22/16) durante esta sesión. La ganancia estructural —quitar la sección serial
del paso— es la que importa de cara a BSPonMPI (Hito 6).

### Validación por tendencia al escalar (encargo del prof. Marín, §9.1 del plan)

Marín señaló que hay errores que solo aparecen al aumentar el tamaño del
sistema, y que **ambos motores deben tener la misma tendencia al crecer**. Se
corrió el modelo completo (versión OpenMP) y el oráculo swarm-abm a tamaños
crecientes (escalando `sheep_density`), verificando que el desvío entre motores
**no crece** con el tamaño. Sin perros (métrica en rango sensible en todos los
tamaños), 5 semillas:

| sheep_dens | ~agentes | C++ (OpenMP) | swarm-abm | \|Δ\| |
|---:|---:|---:|---:|---:|
| 0.96 | 384 | 49.4% | 51.3% | 1.9 |
| 2.00 | 800 | 27.4% | 27.2% | 0.1 |
| 4.00 | 1600 | 14.8% | 14.7% | 0.1 |
| 8.00 | 3200 | 7.2% | 7.6% | 0.4 |

**Pearson r = 0.9995.** Ambos motores trazan la **misma curva** (el loss cae al
diluirse con más ovejas) y el desvío se mantiene ≤ 1.9 pp **sin crecer con el
tamaño** — no hay bug de escala. El mismo barrido con 2 perros da un desvío que
incluso **decrece** al crecer (4.7 → 2.4 → 0.2 → 0.0 → 0.1; el 4.7 del extremo
chico es la alta varianza de perros con pocas ovejas, no un defecto de escala).

## Hito 3 — liebres (presa alternativa) + chillas (segundo depredador)

### Hito 3 — liebres (presa alternativa) + chillas (segundo depredador)

Liebre: percepción 80 m, huida a 800 m/h, maduración a 60 días (vulnerabilidad
0,9 juvenil → 0,6 adulta). Chilla: mismo comportamiento que el zorro pero con
territorio menor (4295 m) y 1,8× más aversa al perro. El zorro/chilla ahora
detecta liebres como presa y aplica **prey switching** (con ≥2 liebres cerca,
baja el atractivo de las ovejas).

Validación vs oráculo, 8 semillas/config:

| config | C++ | swarm-abm | \|Δ\| |
|---|---:|---:|---:|
| baseline | 51.2% | 50.6% | 0.6 |
| hare 3/ha | 13.5% | 14.2% | 0.7 |
| hare 8/ha | 6.6% | 6.6% | 0.0 |
| chilla 4/km² | 71.5% | 71.7% | 0.2 |
| chilla 8/km² | 90.3% | 91.0% | 0.8 |
| hare 3 + chilla 4 | 22.1% | 22.2% | 0.1 |
| dogs 2 + hare 3 | 2.6% | 0.6% | 2.0 |

**Pearson r = 0.9997 · RMSE = 0.90 pp · sesgo +0.13 pp.** Las liebres reducen la
pérdida (presa alternativa); las chillas la aumentan (segundo depredador) —
ambos efectos reproducidos.

## Hito 2 — perros guardianes (la intervención)

### Hito 2 — perros guardianes (la intervención)

`sheep_fox.cpp` incorpora los perros (`--dogs N`): patrulla circular del rebaño,
persecución a 3000 m/h, y **disuasión multi-objetivo** dentro de 200 m que deja a
los zorros con miedo, sin apetito y con memoria de la zona peligrosa (decae a
168 h). Con perros presentes los zorros pasan a la curva de actividad "con perro"
y evitan las áreas de riesgo acumulado.

Validación contra el oráculo, barrido `dogs ∈ {0..4} × fox_eff`, **15
semillas/punto**:

| dogs | fox_eff | C++ | swarm-abm | \|Δ\| |
|---:|---:|---:|---:|---:|
| 0 | 0.14 | 51.5% | 51.3% | 0.1 |
| 1 | 0.14 | 10.0% | 13.6% | 3.6 |
| 2 | 0.14 |  5.8% |  3.8% | 2.0 |
| 3 | 0.14 |  4.1% |  5.0% | 0.9 |
| 4 | 0.14 |  4.8% |  2.7% | 2.1 |

**Pearson r = 0.9949 · RMSE = 2.00 pp · sesgo +0.17 pp.** Los perros reducen la
pérdida de ~50% a dígitos únicos en ambos motores.

**Lección metodológica**: la disuasión es un sistema de **umbral de alta
varianza** (un zorro es disuadido o no, y eso cae en cascada). Con 3 semillas el
1-perro parecía divergir (C++ 3,4% vs oráculo 17,1%); con 10–15 semillas
converge (Δ<1–3 pp). El subsistema de perros **necesita más réplicas** que el
suave oveja+zorro del Hito 1 para una paridad estable.

**Nota (blanco móvil)**: el caso de **2 perros** es justo el "residual" que se
está tuneando en el modelo swarm-abm (WIP no committeado). Este port replica el
HEAD committeado; la validación fina de 2 perros conviene rehacerla cuando ese
tuning se commitee.

## Hito 1 — subconjunto de *screening* (oveja + zorro)

`sheep_fox.cpp` porta el subconjunto de screening del SIGRID committeado
(`models/sigrid/src/lib.rs @ HEAD`): ovejas (adultas/corderos, con miedo, huida,
forrajeo con cohesión y evitación de riesgo, maduración, energía) y zorros culpeo
(curva de actividad horaria, hambre, detección, selección de presa por
vulnerabilidad, predación con sus modificadores de cobertura/grupo/condición/
defensa materna). Con los parámetros de screening no hay perros, liebres ni
chillas, así que esas ramas se omiten (Hitos 2+).

- **Determinista**: sembrado (`mt19937_64`); dos corridas idénticas → mismo hash.
  A diferencia de `sim2-agricultores` (que no es reproducible), este port sí lo
  es desde el inicio.
- **Semántica del motor replicada**: índice espacial = instantánea de inicio de
  paso; activación aleatoria (orden barajado por paso); mutaciones sobre el
  arreglo vivo.

### Validación contra el oráculo (paridad distribucional)

Como el RNG difiere (ChaCha8 en swarm-abm vs mt19937 en C++), la paridad es
distribucional, no bit-exacta — **misma metodología que la paridad vs Mesa**
(`models/sigrid/PARITY.md`). Barrido `sheep_density × fox_eff`, 14 días, 3
semillas/punto, 9 puntos, con `fox_density` en su default en ambos:

| sheep_dens | fox_eff | C++ | swarm-abm | \|Δ\| |
|---:|---:|---:|---:|---:|
| 0.96 | 0.08 | 42.5% | 43.5% | 1.0 |
| 0.96 | 0.14 | 52.3% | 50.1% | 2.2 |
| 0.96 | 0.26 | 60.8% | 58.2% | 2.6 |
| 2.00 | 0.08 | 24.7% | 23.8% | 0.9 |
| 2.00 | 0.14 | 28.2% | 27.8% | 0.4 |
| 2.00 | 0.26 | 31.2% | 30.3% | 0.9 |
| 4.00 | 0.08 | 12.9% | 12.9% | 0.0 |
| 4.00 | 0.14 | 14.9% | 14.3% | 0.7 |
| 4.00 | 0.26 | 17.0% | 16.5% | 0.5 |

**Pearson r = 0.9986 · Spearman ρ = 1.0000 · RMSE = 1.30 pp · sesgo +0.80 pp.**
La paridad C++↔swarm-abm **supera** la de swarm-abm↔Mesa (Pearson 0.966): el
port reproduce el comportamiento del oráculo en todo el rango de loss (13–61%).

### Hallazgo colateral de la validación

El barrido destapó que el CLI del oráculo (`models/sigrid/src/main.rs @ HEAD`)
**documenta `--fox-density` en su ayuda pero no lo parsea** — el argumento se
ignora y `fox_density` queda fijo en 8.4. (Este port C++ sí lo aplica.) Es
justamente el tipo de discrepancia que el esquema de dos motores está pensado
para cazar. Pendiente de arreglar en el `main.rs` de swarm-abm.

## Reproducir

```bash
cd validation/cpp-sigrid
g++ -std=c++17 -O3 -march=native -flto sheep_fox.cpp -o sheep_fox
./sheep_fox --days 30 --seed 1000 --seeds 5          # corrida directa
python3 parity.py <ruta-al-binario-sigrid-de-swarm-abm>   # barrido de paridad
```

El oráculo se construye desde un árbol limpio en HEAD:
`cargo build --release -p sigrid --bin sigrid`.

## Próximos hitos

6. **BSPonMPI** (multi-nodo): ✅ HECHO — ver `../mpi-sigrid/`. Descomposición de
   dominio + halo + broadcast de perros; superpasos = ticks. Bit-idéntico al
   oráculo serial/OMP a cualquier P (6/6 configs). Reveló el hallazgo de
   contracción FP (`-ffp-contract=off`). Escalamiento comm-bound como predijo §9.2.
7. **(Según decisión de alcance)** subsistemas del Mesa completo (infraestructura,
   estacionalidad, rasters GIS) — ver §7 del plan. Se agregan primero al oráculo.

> Pendiente de medición (no de implementación): wall-clock paralelo del
> counting-sort (Hito 5b) en un régimen build-dominado con la máquina descargada.
