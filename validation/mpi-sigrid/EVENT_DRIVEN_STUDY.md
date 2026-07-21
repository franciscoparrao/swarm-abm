# Estudio: ¿conviene la variante event-driven conservadora para SIGRID?

Encargo del prof. Marín a Manuel (plan §9.3): *estudiar YAWNS y si conviene la
variante event-driven conservadora* frente al time-stepped BSP ya implementado
(Hito 6). Este documento responde con el análisis fundamentado en el código y
mediciones, y recomienda el camino.

**TL;DR.** Para SIGRID, el event-driven conservador **temporal** (YAWNS estricto)
**no aporta** speedup: el *lookahead* del modelo es exactamente **1 tick** (por la
semántica de instantánea), así que YAWNS con ε = lookahead = 1 tick *es* el BSP
time-stepped que ya existe; y como **todos los agentes actúan en todos los ticks**,
no hay "tiempo vacío" que saltar (que es de donde el event-driven saca su
ganancia). La ociosidad que sí existe (26% ovejas, 56% zorros) es **intra-tick** y
**ya está explotada** por el código. El cuello real (revelado por Hito 6) es
**espacial**, no temporal: la fase B centralizada + el gather O(N) por tick. La
variante conservadora que **sí** conviene es la **distribución espacial de la fase
B**, que cambia bit-identidad-vs-oráculo por paridad distribucional pero escala.

## 1. YAWNS en una línea, y el lookahead de SIGRID

YAWNS (Nicol) es PDES **conservador**: en vez de sincronizar evento por evento,
avanza todos los procesos hasta una frontera temporal común `T + ε` y sincroniza
con una barrera global; es **exacto** si `ε ≤ lookahead`, donde el *lookahead* es
el mínimo tiempo antes de que un evento en un procesador pueda afectar a otro. Con
ε = lookahead es, en esencia, **BSP** (superpasos = ventanas de tamaño ε).

El lookahead de SIGRID se lee directo de la semántica del modelo (`sigrid_core.hpp`,
`Model::before_step`): la **instantánea se congela al inicio del tick** y todas las
consultas la leen; las mutaciones (mover, matar, miedo) se ven recién en la
instantánea del **tick siguiente**. Por lo tanto una acción en el tick *t* afecta
la percepción de cualquier otro agente recién en *t+1*:

> **lookahead = 1 tick.**

Luego YAWNS estricto con `ε = 1 tick` **es** el BSP de Hito 6 (superpaso = tick).
No hay ε mayor posible: la instantánea acopla todo el sistema cada tick. Esto no es
una limitación de la implementación sino del modelo — y es la misma propiedad que
hace segura y bit-idéntica la paralelización (Hito 6).

## 2. El event-driven gana saltando "tiempo vacío" — que aquí no existe

La ventaja del event-driven sobre el time-stepped es **saltar los ticks en que no
pasa nada**: si ningún LP tiene un evento en `[T, T+k)`, se avanza el reloj a `T+k`
de una. Eso paga cuando los eventos son **ralos en el tiempo** (baja densidad, como
apuntó Marín).

En SIGRID **no hay tiempo vacío**: en cada tick los bucles de fase A y fase B
recorren a *todos* los agentes vivos, y cada oveja/liebre corre su `step` (decaen
miedo, envejecen, actualizan energía, maduran) *aunque no se muevan*. No existe un
tick que algún agente pueda saltar por completo. Con lookahead = 1 y sin tiempo
vacío, **las ventanas ε > 1 nunca se forman** → el event-driven degenera al
time-stepped BSP, sin ganancia.

## 3. La ociosidad que existe es intra-tick, y ya está explotada

Medido (8×8 km, 3 perros, 10 días, `run_loss_rate` instrumentado):

| agente | activo | ocioso | % ocioso |
|---|---:|---:|---:|
| ovejas | 1 086 807 | 386 701 | **26.2%** |
| zorros | 56 694 | 72 426 | **56.1%** |

Dos observaciones que anulan el atractivo del event-driven:

1. **Las ovejas dominan el cómputo 11×** (1.47M ticks-oveja vs 129k ticks-zorro).
   El 56% de zorros ociosos es casi irrelevante para el costo total.
2. **La ociosidad ya está explotada en el hot path.** `step_sheep` solo corre la
   consulta de forrajeo (la parte cara) si la oveja decide moverse
   (`if (rng.unit() < p_move) { graze }`); una oveja que no se mueve ya salta esa
   query. `step_fox` hace `if (rng.unit() >= level) return;` al descansar. Es
   decir, el modelo **ya es "event-driven a nivel de sub-paso"**: no ejecuta el
   trabajo caro de un agente ocioso. Un scheduler event-driven no tiene nada nuevo
   que saltar.

Skipping de agentes ociosos = optimización de **cómputo local**, ortogonal a la
sincronización/comunicación, y ya hecha. No es lo que arregla el multi-nodo.

## 4. El cuello real es espacial (Hito 6), no temporal

Hito 6 mostró que el BSP no escala (P2 9.2s → P8 18.8s a 8×8 km): es
**comunicación-bound**, por dos costos que son consecuencia de la elección
bit-idéntica, no de la sincronización temporal:

- **Fase B centralizada en proc 0** (orden secuencial global + señal de perro
  global) → cuello serial.
- **Gather O(N)/tick** del estado de ovejas al proc 0.

Ninguno de los dos lo toca el event-driven temporal. El event-driven **no ataca el
cuello de SIGRID**.

## 5. Lo que sí conviene: distribución espacial conservadora de la fase B

La variante conservadora productiva para SIGRID es **espacial**: distribuir también
la fase B para eliminar el gather global, manteniendo el marco conservador (ε = 1
tick, barrera por tick) pero comunicando **solo en las fronteras**:

- Cada rank procesa **sus** depredadores sobre su franja + **halo de depredador**
  (≥ 1200 m, el radio de detección del perro; el zorro necesita ~800 m).
- **Perros**: se difunden a todos (uno-a-muchos barato, pocos emisores) — su señal
  a 6135 m es global e inevitable, pero el volumen es mínimo.
- **Centroide del rebaño** (patrulla del perro): una **reducción global** barata
  (suma de posiciones + conteo), no un gather.
- **Predación cruzada de frontera** (un zorro de un rank mata una oveja *ghost* de
  otro): se resuelve **conservadoramente** con **eventos de matanza** reconciliados
  por una regla determinista (p. ej. menor gid gana si dos zorros reclaman la misma
  presa), en vez de serializar globalmente.

**El costo:** el orden de activación deja de ser el *shuffle* global del oráculo
serial, así que el resultado **ya no es bit-idéntico al oráculo** — es otra
realización válida, a validar por **paridad distribucional** (Pearson ≥ 0.95 vs
swarm-abm, el criterio explícito del plan §4 para la versión paralela). Se puede
mantener **determinismo P-independiente** (mismo resultado a cualquier P) usando un
orden por gid y desempates por gid. Es el trade-off que el plan ya contemplaba para
la versión distribuida, y el único camino a un speedup multi-nodo real en este
modelo globalmente-acoplado.

## 6. Recomendación

1. **No** invertir en un scheduler event-driven **temporal** (YAWNS estricto): para
   SIGRID degenera al BSP de Hito 6 (lookahead = 1 tick, sin tiempo vacío) y la
   ociosidad ya está explotada. Confirmado analítica y empíricamente.
2. La variante conservadora que **sí** paga es la **distribución espacial de la
   fase B** (§5): elimina el gather O(N), escala, y se mantiene conservadora (ε = 1
   tick). Trade-off: bit-identidad-vs-oráculo → paridad distribucional (con
   determinismo P-independiente preservable).
3. El event-driven **temporal** solo valdría la pena si el modelo migrara a un
   régimen genuinamente ralo en el tiempo (agentes que no actúan por muchos ticks)
   — no es el caso de SIGRID de screening. Para densidades muy bajas y áreas
   enormes podría re-evaluarse.

## Referencias

- D. Nicol, *The cost of conservative synchronization in parallel discrete event
  simulations* (YAWNS). J. ACM, 1993.
- ROSS (Rensselaer's Optimistic Simulation System) implementa YAWNS conservador y
  Time Warp optimista — referencia de implementación.
- Plan del port, §9 (`docs/PLAN_PORT_CPP_SIGRID.md`): comunicación uno-a-muchos,
  BSP/YAWNS conservador, y la validación por tendencia.
