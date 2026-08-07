# Estudio: ¿conviene una variante event-driven para SIGRID? (conservadora, optimista o aproximada)

Encargo del prof. Marín a Manuel (plan §9.3): *estudiar YAWNS y si conviene la
variante event-driven conservadora* frente al time-stepped BSP ya implementado
(Hito 6). Este documento responde con el análisis fundamentado en el código y
mediciones, y recomienda el camino. La §6 amplía el alcance más allá del
conservador (optimista/aproximada) a partir de un aporte de Manuel (ago-2026).

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

Ampliando a las otras familias PDES (§6): la equivalencia tiempo↔evento está
**formalmente respaldada** (los modelos por pasos son un caso particular de DEVS;
Zeigler §1.5–1.6), así que la pregunta no es *si se puede* sino *si acelera*. La vía
**optimista** (Time Warp) es poco prometedora para este modelo —la señal global del
perro dispararía rollbacks frecuentes y en cascada, y el *state-saving* con miles de
agentes es caro—; la vía **aproximada** coincide con la distribución de la fase B
(soltar bit-identidad → paridad distribucional). El juez decisivo sigue siendo
**medir el perfil de cómputo**.

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

## 6. Más allá del conservador: optimista y aproximado

*Ampliación a partir del aporte de Manuel (ago-2026): la referencia DEVS y la
observación de que YAWNS es solo la familia **conservadora** — falta evaluar si una
variante **optimista** o **aproximada** rinde mejor.*

**Base formal (DEVS).** La equivalencia tiempo↔evento está establecida hace décadas:
los modelos por pasos (DTSS, *Discrete Time System Specification*) son un **subclase
representable en DEVS**, formalismo planteado como universal para eventos discretos
(Zeigler, *Theory of Modeling and Simulation*, §1.5–1.6 y Cap. 7). El
**homomorfismo** de §1.5 es el puente concreto: un **tick es la macrotransición**
que agrupa las micro-decisiones internas del agente (el "budget"). Conclusión: **la
posibilidad está zanjada; la pregunta abierta es solo el speedup** — justo lo que
el prof. Marín anticipaba ("se puede, pero no dice cómo").

Las §§1–5 cubrieron la familia **conservadora** (YAWNS/BSP). PDES tiene otras dos:

**Optimista (Time Warp).** Los procesos avanzan **especulativamente** sin esperar;
ante un evento que llega "en el pasado" (violación de causalidad) hacen **rollback**.
No está atado al lookahead → un rank podría correr muchos ticks adelantado y
retroceder solo en los choques de frontera. Atractivo con lookahead = 1 (el
conservador está obligado a sincronizar cada tick). Pero para *este* modelo hay tres
frenos:

1. **Acoplamiento global (la señal del perro).** El territorio del zorro
   (6135 m > diagonal del mapa) hace que **todo zorro dependa de cualquier perro**, y
   el perro se mueve cada tick (hasta 3000 m). Un rank adelantado recibiría un
   "el perro se movió" que afecta a sus zorros **casi cada tick** → rollback casi cada
   tick → la ventana especulativa colapsa a ~1 tick, sin ganancia sobre el conservador
   y con el overhead extra. Time Warp gana cuando los eventos cruzados son **raros**;
   aquí son frecuentes y globales.
2. **Cascada de rollbacks.** Con acoplamiento global, deshacer una matanza obliga a
   deshacer lo que otros ranks ya vieron → **rollbacks en cascada**, el modo de falla
   patológico de Time Warp. El acoplamiento global es exactamente la condición que lo
   degrada.
3. **Costo de *state-saving*.** El rollback exige *checkpointing* del estado; con las
   ovejas dominando (miles), guardar/restaurar es caro en memoria y tiempo — y agrava
   justo el régimen **memory-bound** que Hito 5 combatió.

*Veredicto optimista:* poco prometedor **as-is**. El mismo acoplamiento global que
vuelve al conservador comm-bound vuelve al optimista rollback-bound. Solo pagaría si
la señal del perro se **localizara** o el acoplamiento se raleara — un cambio de
*modelo*, no de sincronización. Amerita a lo más una estimación de sobre, no un build.

**Aproximado / relajado.** Tolerar errores de causalidad acotados a cambio de
velocidad (p. ej., usar posiciones de perro con un retardo de ε ticks, o
sincronización relajada). No es un mecanismo nuevo sino un **objetivo de correctitud
nuevo**: **coincide con la distribución de la fase B de §5** (soltar la bit-identidad
→ paridad distribucional). Dicho de otro modo, "PDES aproximado" y "distribuir la
fase B validando por Pearson ≥ 0.95" son **la misma palanca vista desde dos ángulos**.

**Bottom line:** ninguna de las tres familias cambia el diagnóstico sin datos. El
paso decisivo sigue siendo **medir el perfil de cómputo/comunicación** (acordado en
la reunión con Manuel) antes de comprometer un build.

> **Actualización (ago-2026): el perfil ya se midió** — ver
> [`docs/PERFIL_COMPUTO.md`](../../docs/PERFIL_COMPUTO.md). Confirma este estudio y
> zanja la decisión: el modelo es **97–99.7 % paralelizable** (la fase B secuencial
> pesa ≤ 3.2 % y *baja* al escalar), así que **no hay cuello de sincronización que
> ningún esquema PDES pueda atacar**. Además, a escala real el screening completo
> corre en **0.12 s** en serie. Conclusión: sin caso para event-driven en ninguna de
> sus tres familias; si se escala, **OpenMP en un nodo**.

## 7. Recomendación

1. **No** invertir en un scheduler event-driven **temporal conservador** (YAWNS
   estricto): para SIGRID degenera al BSP de Hito 6 (lookahead = 1 tick, sin tiempo
   vacío) y la ociosidad ya está explotada. Confirmado analítica y empíricamente.
2. La variante conservadora que **sí** paga es la **distribución espacial de la
   fase B** (§5): elimina el gather O(N), escala, y se mantiene conservadora (ε = 1
   tick). Trade-off: bit-identidad-vs-oráculo → paridad distribucional (con
   determinismo P-independiente preservable).
3. **Optimista (Time Warp): descartar as-is** — el acoplamiento global (señal del
   perro) hace los rollbacks frecuentes y en cascada (§6). Solo reconsiderar si el
   modelo localiza esa señal.
4. **Aproximada: es la misma ruta que (2)** — soltar exactitud coincide con
   distribuir la fase B por paridad distribucional. No es una vía separada.
5. **Antes de cualquier build: medir el perfil de cómputo/comunicación** (acuerdo
   con Manuel). El event-driven temporal solo se re-evaluaría si el modelo migrara a
   un régimen genuinamente ralo en el tiempo (no es el caso del screening).

## Referencias

- D. Nicol, *The cost of conservative synchronization in parallel discrete event
  simulations* (YAWNS). J. ACM, 1993.
- D. Jefferson, *Virtual Time* (Time Warp / PDES optimista). ACM TOPLAS, 1985.
- R. Fujimoto, *Parallel and Distributed Simulation Systems*, 2000 (panorama
  conservador vs optimista).
- B. Zeigler, H. Praehofer, T.G. Kim, *Theory of Modeling and Simulation* — Cap. 1,
  §1.5 (morfismos, homomorfismo modelo↔simulador) y §1.6 (evolución de DEVS; DTSS
  como subclase). Referencia aportada por Manuel.
- ROSS (Rensselaer's Optimistic Simulation System): implementa YAWNS conservador y
  Time Warp optimista — referencia de implementación.
- Plan del port, §9 (`docs/PLAN_PORT_CPP_SIGRID.md`): comunicación uno-a-muchos,
  BSP/YAWNS conservador, y la validación por tendencia.
