# SIGRID en BSPonMPI — Hito 6: multi-nodo por descomposición de dominio (BSPlib)

Versión distribuida del modelo de ovejas SIGRID sobre **BSPonMPI** (la
implementación de BSPlib de Wijnand Suijlen sobre MPI). Reusa EXACTAMENTE el
núcleo del modelo (`../cpp-sigrid/sigrid_core.hpp`, la misma fuente que la versión
OpenMP), así que el comportamiento es idéntico por construcción; esta capa solo
agrega la distribución BSP.

> **Decisión (confirmada con el usuario):** priorizar **bit-identidad** a cualquier
> número de procesadores por sobre el escalamiento de los depredadores. El
> resultado es que la versión BSPonMPI da el **mismo `loss_rate`/matanzas/intentos
> bit a bit** que el oráculo serial/OpenMP, a P=1,2,4,…, en todo el barrido de
> screening. Es el problema genuinamente difícil del ABM paralelo (determinismo
> bajo distribución), resuelto y verificado.

## Estado: bit-idéntico y validado (6/6 configs, todas las especies)

`validate.sh` compila el oráculo serial y el BSP con `-ffp-contract=off` y compara.
Resultado: **serial == BSP P=2 == BSP P=4**, bit a bit, en:

| config | loss_rate · matadas · intentos |
|---|---|
| sin perros (guarda de radio activa) | 28.45% · 437 · 4219 |
| perros ×3 | 0.59% · 9 · 86 |
| liebres 4/ha | 0.00% · 0 · 4697 |
| chillas 3/km² | 35.48% · 545 · 6088 |
| perros ×2 + liebres + chillas (5×5 km) | 0.04% · 1 · 7160 |
| todo junto, 4 perros, 6×6 km | 0.00% · 0 · 8683 |

## Arquitectura BSP

El modelo ya es **BSP conservador / YAWNS con ε = 1 tick** (§9.3 del plan): la
instantánea de inicio de paso congela el estado que leen todos, así que no hay
causalidad violable dentro del tick. Cada tick son dos supersteps:

- **Fase A (ovejas + liebres) — DISTRIBUIDA.** El dominio se parte en franjas en x
  entre los P procesadores, con **halo de 500 m** (≥ el mayor radio de consulta de
  una oveja: la atracción al perro). Cada rank arma su snapshot local **ordenado
  por gid** (índice global), de modo que el orden intra-celda es gid-descendente,
  igual que el serial ⇒ las consultas dan el mismo resultado bit a bit. El RNG
  por-agente se siembra con `(semilla, tick, gid)`, independiente del rank.
- **Fase B (zorros + perros) — CENTRALIZADA en proc 0.** Es secuencial-global
  (orden barajado por tick, mutaciones cruzadas, y la señal del perro es **global**:
  el territorio del zorro llega a 6135 m > diagonal del mapa). El proc 0 corre el
  código de fase B serial EXACTO sobre el estado global. Los perros se **difunden**
  a todos (patrón uno-a-muchos de Marín, barato porque son pocos).

Primitivas BSPlib usadas: `bsp_send`/`bsp_move`/`bsp_qsize` (BSMP, para los
intercambios irregulares: dispersión franja+halo, broadcast de perros, gather del
estado vivo, señal de término) y `bsp_sync` (la barrera de superstep).

## Hallazgo clave: la contracción FP rompe la bit-identidad cross-compilador

Al validar, los casos **con perros** divergían del oráculo por **1 ULP**, que tras
~120 ticks se amplificaba a una matanza de diferencia. El bug **no estaba en la
distribución** (P=2 y P=4 daban idéntico entre sí): era que `bspcxx` (que envuelve
`mpicxx`→g++) y el `g++` del oráculo **fusionaban `a*b+c` en un FMA de forma
distinta**. En un sistema de umbral de alta varianza como la predación con perros,
ese 1 ULP en el movimiento de un zorro cascadea a un resultado visible.

**La cura: compilar todo con `-ffp-contract=off`.** Con la contracción desactivada
el modelo computa idéntico bit a bit en cualquier compilador. Es una lección de
reproducibilidad numérica general (el FMA es un riesgo de determinismo silencioso),
no específica de BSP. Está documentada en la cabecera de `sigrid_core.hpp`. El
`#pragma STDC FP_CONTRACT OFF` no basta con GCC (lo ignora en C++); hay que usar el
flag de compilación.

## Escalamiento (hallazgo honesto — confirma la predicción de §9.2)

Medido en 8×8 km, 10 días, la versión BSP **no acelera al agregar procesadores**;
al contrario:

| P | wall |
|---:|---:|
| 2 | 9.2 s |
| 4 | 11.2 s |
| 8 | 18.8 s |

Esto es **exactamente lo que §9.2 del plan anticipó**: a estas escalas el modelo es
**comunicación-bound**. Dos razones, ambas consecuencia de la elección
bit-idéntica:
1. **Fase B centralizada** ⇒ el proc 0 es un cuello secuencial (la señal global del
   perro y el orden secuencial de predación no se distribuyen sin perder la
   bit-identidad).
2. **Gather O(N) por tick** ⇒ reunir el estado vivo de las ovejas en proc 0 domina;
   más ranks = más mensajes al proc 0.

El plan ya condicionaba BSPonMPI a "confirmar el techo de agentes/área con Marín
antes de invertir" y advertía que **la descomposición solo paga cuando el área
crece hasta que los radios de interacción (halo 500–1200 m) son chicos frente al
bloque** — a 8 km con P=8 los bloques son de 1 km, del orden del halo. El camino a
un speedup real es (a) áreas mucho mayores (decenas de km, estancias completas) con
densidad constante, y/o (b) distribuir también la fase B. La bit-identidad —el
problema difícil— ya está resuelta; el rendimiento multi-nodo es el trabajo
siguiente, con el diseño correcto validado como base.

Sobre el **encargo de Marín (variante event-driven conservadora)**: ver
[`EVENT_DRIVEN_STUDY.md`](EVENT_DRIVEN_STUDY.md). Conclusión: el event-driven
*temporal* (YAWNS estricto) **no conviene** para SIGRID (lookahead = 1 tick por la
instantánea ⇒ degenera al BSP de Hito 6; y la ociosidad ya está explotada
intra-tick). La variante conservadora que **sí** paga es la **distribución espacial
de la fase B** (elimina el gather O(N)), a costa de cambiar bit-identidad-vs-oráculo
por paridad distribucional.

## Reproducir

```bash
# 1. Instalar BSPonMPI (una vez)
git clone https://github.com/wijnand-suijlen/bsponmpi.git
cd bsponmpi && ./configure --prefix=$HOME/.local/bsponmpi && make install
export BSP_PREFIX=$HOME/.local/bsponmpi

# 2. Compilar y validar la bit-identidad vs el oráculo serial
cd validation/mpi-sigrid
./validate.sh              # 6/6 configs: serial == BSP P=2 == BSP P=4

# 3. Correr directo
./build.sh
$BSP_PREFIX/bin/bsprun -n 4 ./sheep_fox_bsp --width 8000 --height 8000 --days 30 --seed 1000
```

Nota: BSPonMPI degenera con `-n 1` (error `MPI_ERR_WIN` de ventana inválida con un
solo proceso); usar P≥2. La bit-identidad se verifica comparando P=2/4/… contra el
oráculo serial, y entre sí.
