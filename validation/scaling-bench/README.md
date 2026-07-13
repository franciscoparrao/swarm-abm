# Benchmark de escalamiento — swarm-abm vs. C++ escrito a mano

Compara el **motor swarm-abm** (Rust) contra una implementación **C++ idiomática
escrita a mano** en un kernel depredador-presa espacial, escalando el número de
agentes en un solo nodo. Responde la pregunta concreta que motivó la comparación:
para escalar el problema de las ovejas a decenas de miles de agentes, ¿hace falta
irse a C++/MPI, o el motor en Rust ya alcanza?

> **Resultado.** Ambos escalan **linealmente** (mismo orden algorítmico). El C++
> a mano es **~2× más rápido** que swarm-abm a decenas de miles de agentes —un
> factor constante, no creciente—, en la misma banda que el resultado vs.
> krABMaga. A 40.000 agentes ambos corren un paso en milisegundos: la escala que
> se discute cabe holgadamente en **un solo nodo**, sin distribución.

## Qué se compara (y por qué es justo)

Un kernel deliberadamente simple pero representativo de lo que domina el costo en
el modelo de ovejas: N agentes en espacio continuo, consultas de vecindad por
radio, movimiento por paso. No es el SIGRID completo (eso mediría quién programó
mejor la ecología, no el motor) — es el **núcleo espacial** que determina cómo
escala cada motor.

Reglas idénticas en ambos lados (`cpp/prey_predator.cpp`, `rust/src/main.rs`):

- N agentes en `[0,L)²`, con `L = √(N/λ)` y `λ` constante ⇒ **densidad fija al
  escalar** (cada consulta por radio devuelve ~5 vecinos, sin importar N).
- 20% depredadores. Por paso: se reconstruye el índice espacial; cada depredador
  busca la presa más cercana dentro de radio R y avanza hacia ella (o camina
  aleatorio si no hay); las presas caminan aleatorio. Actualización **sincrónica**
  (las consultas ven las posiciones del inicio del paso).
- Sin nacimientos ni muertes ⇒ población constante ⇒ `ms/paso` limpio.

Condiciones de justicia:

| | C++ | swarm-abm (Rust) |
|---|---|---|
| Índice espacial | cell list a mano | `ContinuousSpace` (hash espacial del motor) |
| Optimización | `-O3 -march=native -flto` | `opt-level=3`, LTO, `target-cpu=native` |
| RNG | `mt19937_64` sembrado | ChaCha8 sembrado |
| Hilos | 1 | 1 |
| Reproducible | sí (sembrado) | sí (sembrado + garantía del motor) |

El C++ es escrito **a mano en su mejor forma** (sin overhead de "motor"); el Rust
usa la **API pública real** de swarm-abm (`ContinuousSpace`, `PointId`, `reindex`,
`for_each_within`), con toda su indirección de arena generacional — es decir, se
mide el costo real de *usar el motor*, no una versión desnuda.

## Resultados

Intel i7-1270P, 1 hilo, mediana de 7 réplicas, `ms` por paso:

| N agentes | C++ (a mano) | swarm-abm | C++ más rápido |
|---:|---:|---:|---:|
| 400    | 0,029 | 0,040 | 1,36× |
| 1.600  | 0,118 | 0,195 | 1,65× |
| 6.400  | 0,541 | 1,062 | 1,96× |
| 25.600 | 2,462 | 4,778 | 1,94× |
| 40.000 | 3,694 | 7,151 | 1,94× |

swarm-abm (v0.4, desde crates.io) vs. g++ 13.3, rustc 1.96.

## Lectura

1. **Los dos escalan igual de bien.** El factor C++/swarm-abm no crece con N: se
   estabiliza en ~1,94×. Ambos son O(N) con el índice espacial; la diferencia es
   un **factor constante**, el precio de la generalidad del motor (arena
   generacional, indirección de `PointId`, API de trait), no un problema de
   escalamiento.

2. **~2× es el mismo orden que vs. krABMaga** (~1,5–2,6×). Un motor genérico y
   reproducible en Rust queda a un factor ~2 de C++ escrito a mano para este
   problema — y a cambio entrega reproducibilidad verificada, el modelo de ovejas
   ya implementado y validado contra Mesa, y análisis de sensibilidad nativo.

3. **A esta escala no hace falta distribución.** 40.000 agentes = 3,7 ms/paso
   (C++) o 7,2 ms/paso (swarm-abm): cientos de pasos por segundo en **un solo
   nodo**. OpenMP/BSPonMPI apuntan a cuando un nodo no alcanza; decenas de miles
   de agentes no llegan a ese punto en ninguno de los dos motores.

4. **El C++ es reproducible aquí solo porque es serial y lo sembramos.** Al
   paralelizar con OpenMP/BSP —que es la razón para ir a C++— la reproducibilidad
   deja de ser un `seed` y se vuelve el problema difícil (RNG por-agente
   decorrelacionado, orden de reducción estable, determinismo bajo scheduling)
   que swarm-abm ya resuelve y verifica en CI. Este benchmark, al ser
   single-thread, le concede a C++ el caso fácil de la reproducibilidad.

## Reproducir

```bash
cd validation/scaling-bench
./run.sh                      # compila ambos y corre la tabla
REPS=11 SIZES="400 4000 40000" ./run.sh   # parámetros ajustables
```

> Nota: los tiempos absolutos dependen del hardware. Conviene re-correrlo en la
> máquina objetivo; lo estable entre máquinas es el **ratio** y la **linealidad**.
