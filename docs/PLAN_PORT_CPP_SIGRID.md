# Plan técnico — port C++ del modelo de ovejas (SIGRID), con swarm-abm de oráculo

> **Estrategia (dos motores, roles complementarios).** swarm-abm (Rust) queda
> como **implementación de referencia**: reproducible bit a bit, ya validada
> contra Mesa, con análisis de sensibilidad y calibración nativos. El nuevo
> simulador **C++ (OpenMP → BSPonMPI)** es el motor de **producción/escala**. El
> C++ se **valida contra swarm-abm** a cada paso — un ABM paralelo es muy difícil
> de verificar sin un oráculo reproducible.
>
> **Importante:** el C++ es una **implementación nueva**, no una adaptación de
> `sim2-agricultores`. Ese código es un DES de mercados agrícolas, no espacial;
> el modelo de ovejas es espacial y por pasos, y necesita primitivas (grilla,
> vecindades, movimiento) que aquel no tiene. El kernel de
> `validation/scaling-bench/cpp` ya es la base del motor espacial y se extiende
> con el modelo.

## 1. Qué hay que portar (el modelo, desde el SIGRID committeado)

Modelo tick-horario (`age_days += 1/24`), ~30 días = 720 pasos, sobre espacio
continuo 2D (4 km² = 2000×2000 m por defecto) + dos rasters (calidad de
vegetación y otro de paisaje). Activación **aleatoria por paso** (el orden afecta
el stream de RNG — hay que replicarlo para paridad).

**9 parámetros** (`Params`): `sheep_density`, `fox_density`,
`fox_predation_effectiveness`, `n_dogs`, `hare_density`, `chilla_density`,
`lamb_proportion`, `use_fear`, dimensiones.

**4 especies**, cada una con lógica de `step` propia:

- **Oveja** (adulta / cordero, con maduración a los 120 días): decaimiento de
  miedo, huida si `fear>0.7`, forrajeo según *time budget* (que cambia si hay
  perro a <100 m), energía a partir de calidad de pasto menos estrés. Radios de
  percepción y velocidades distintos para cordero vs adulto; vulnerabilidad
  0,4 (adulto) / 0,85 (cordero).
- **Zorro**: territorio (6135 m), detección (300 m), ataque (50 m), curva de
  actividad gaussiana (distinta con/sin perro), umbral de hambre, aversión al
  riesgo, memoria de peligro (TTL 168 h, radio 400 m).
- **Perro** (intervención, `n_dogs`): patrulla (300 m/h) vs persecución
  (3000 m/h), detección (1200 m), disuasión (radio 50 m, fuerza 0,20),
  radios de patrulla/persecución/evasión.
- **Liebre** (presa alternativa): velocidad normal/huida, percepción 80 m,
  madurez a 60 días.

Todas las constantes están en el SIGRID committeado (`models/sigrid/src/lib.rs`,
cabecera) — son la especificación exacta a replicar.

## 2. Arquitectura del simulador C++

- **Motor espacial**: reusar el cell list del kernel de `scaling-bench` (índice
  O(N), reconstruido por paso). Consulta de vecindad por radio = el análogo de
  `for_each_within`.
- **Agentes heterogéneos**: `struct Animal` con un *tag* de especie (`enum
  Species`) + `switch` en el `step` — **no** clases con métodos virtuales en el
  hot path (el despacho virtual mata el rendimiento y complica el port fiel).
  Espeja el `enum Species` + `match` de swarm-abm. Considerar SoA si el profiling
  lo pide (Fase B).
- **Scheduler**: activación aleatoria por paso con `shuffle` sembrado — mismo
  orden lógico que `Activation::Random` de swarm-abm.
- **Rasters**: cargar/generar idénticos a swarm-abm (misma grilla, mismos
  valores) — condición para la paridad.
- **Rebaño/demografía**: matanzas remueven agentes; corderos maduran. Manejar la
  remoción como en swarm-abm (marca `alive` + compactación, o arena).

## 3. Determinismo en C++ (el punto que da sentido al oráculo)

- **Fase serial**: `mt19937_64` sembrado desde config; orden de activación
  determinista (`shuffle` sembrado). Esto ya deja el C++ reproducible corrida a
  corrida (lo que `sim2-agricultores` no tiene).
- **Fase paralela (OpenMP)** — el problema difícil: para que el resultado **no
  dependa del scheduling de hilos**, cada agente deriva su RNG de
  `(semilla, tick, agent_id)` (la idea de `child_rng` de swarm-abm), y las
  reducciones (conteos de matanza, etc.) se acumulan en **orden estable**, no en
  orden de finalización de hilo. Sin esto, el paralelo deja de ser reproducible —
  y es exactamente el tipo de bug que el oráculo caza.
- **Fase distribuida (BSPonMPI)**: descomposición del dominio espacial en bloques
  + intercambio de halo en las fronteras; superpasos BSP. Determinismo bajo
  distribución = orden de mensajes estable + RNG por-agente. Es lo genuinamente
  difícil y donde el aporte de Marín es central.

## 4. Protocolo de validación contra swarm-abm (el oráculo)

El RNG difiere entre lenguajes (ChaCha8 vs mt19937), así que **no** habrá paridad
bit-exacta cross-lenguaje. Se usa la **misma metodología que la paridad vs Mesa**
(`models/sigrid/PARITY.md`): **paridad distribucional** sobre una grilla de
puntos de parámetros × semillas.

- Diseño: ~12 puntos factoriales de parámetros × 5 semillas, 14–30 días.
- Métrica: `loss_rate` (y conteos de matanza por depredador) por punto.
- Aceptación: **Pearson ≥ 0,95 y Spearman ≥ 0,90** entre C++ y swarm-abm, RMSE
  comparable al que swarm-abm logró vs Mesa (r=0,966). Distribuciones solapadas.
- **Validación incremental** (clave para no debuggear todo junto): (a) ovejas +
  zorros; (b) + perros; (c) + chillas; (d) + liebres. Cada capa se valida contra
  el swarm-abm equivalente antes de agregar la siguiente.
- En la Fase paralela, el oráculo se vuelve indispensable: se corre el C++ en
  serie y en paralelo y **ambos** deben validar contra swarm-abm; si el paralelo
  se desvía, hay un bug de concurrencia.

## 5. Escalamiento (responde el objetivo de Marín)

- **Fase A — serial correcto**: valida contra el oráculo. Entregable: C++ que
  reproduce el comportamiento de swarm-abm (Pearson ≥ 0,95).
- **Fase B — OpenMP (un nodo)**: decenas de miles de agentes. Medir speedup y
  re-validar contra el oráculo. Aquí ya se cubre la escala discutida (el
  benchmark de `scaling-bench` muestra que 40k agentes son ms/paso en un nodo).
- **Fase C — BSPonMPI (multi-nodo)**: solo si el techo real de agentes supera lo
  que un nodo aguanta. Descomposición de dominio + halo. Confirmar el techo con
  Marín antes de invertir aquí.

## 6. Hitos

1. **Motor espacial C++ + 2 especies (oveja/zorro)** sembrado y determinista →
   valida contra swarm-abm (subconjunto). Base: kernel de `scaling-bench`.
2. **Modelo completo (4 especies)** serial → paridad distribucional Pearson ≥ 0,95.
3. **OpenMP** → mismo resultado validado, con speedup; benchmark de escala.
4. **(Opcional) BSPonMPI** → multi-nodo, si el techo de agentes lo justifica.

En paralelo a todo esto, la **ciencia sigue en swarm-abm**: la calibración y el
análisis de sensibilidad (Sobol) corren ahí desde ya, sin esperar la infra C++.

## 7. Alcance frente al Mesa original — decisión de scope (¡leer antes de portar!)

El SIGRID de swarm-abm (~1.200 líneas) es un port del **subconjunto de
screening** del Mesa original de Isla Riesco (`~/proyectos/Isla_Riesco/
simulacion_agentes/`, ~5.235 líneas). La paridad documentada (Pearson 0,966) se
midió **sin estacionalidad y sin infraestructura, a 14 días** — esa es la validez
actual del oráculo. El Mesa tiene subsistemas que **ni el port de Rust ni el
kernel C++ tienen todavía**:

| Elemento del Mesa | En swarm-abm (oráculo) | Implicancia para el C++ |
|---|---|---|
| **Infraestructura**: cercos (`Fence.can_cross` por tipo de agente), corrales, aguadas, construcciones (`infrastructure.py`, 464 líneas) | ❌ ausente | Si el modelo de producción los necesita, hay que agregarlos a C++ **y** al oráculo |
| **Estacionalidad** (`seasonality.py`, 809 líneas) | ❌ ausente (screening la apaga) | Idem — subsistema grande |
| **Rasters reales GIS** (DEM/land cover/NDVI vía `from_rasters`/`from_geopackage`) | ❌ usa rasters sintéticos | Decidir si producción corre sobre geodata real |
| **Chilla** como agente (`model_v2.py`, 36 refs) | ⚠️ parámetro `chilla_density`, revisar si es agente propio o variante de zorro | Confirmar el tratamiento antes de portar |
| Mapa de riesgo (`calculate_risk_map`), accesibilidad, agua | ❌ | Parte del paisaje del miedo completo |

**Decisión necesaria (con Marín y la ciencia):** ¿el simulador C++ de producción
debe ser el **screening** (lo que el oráculo valida hoy) o el **Mesa completo**
(con infraestructura + estacionalidad + geodata)? Recomendación:

1. **Portar primero el scope de screening** — es lo único que el oráculo valida
   hoy, así que es lo único verificable de inmediato. Cierra los Hitos 1–3.
2. **Luego, si producción exige el modelo completo**, agregar los subsistemas
   faltantes **primero al oráculo swarm-abm** (donde es barato y reproducible),
   re-validar contra Mesa, y recién después replicarlos en C++. Es decir: el
   oráculo siempre va un paso adelante del C++.

Esto evita el peor escenario: escribir infraestructura/estacionalidad directo en
C++ paralelo sin una referencia reproducible contra la cual validarla.

## 8. Qué ya está hecho y sirve de base

- `validation/scaling-bench/cpp/prey_predator.cpp` — motor espacial (cell list),
  sembrado, reproducible: la base del Hito 1.
- `validation/scaling-bench/rust/` — el mismo kernel en swarm-abm: referencia de
  las reglas y del rendimiento objetivo.
- `models/sigrid/` (swarm-abm) — el modelo completo, el oráculo, y la
  especificación exacta (constantes y reglas).
- `models/sigrid/PARITY.md` — la metodología de paridad distribucional ya
  aplicada vs Mesa, a reutilizar tal cual para validar el C++.

## 9. Paralelización: sincronización conservadora (BSP/YAWNS) y patrones de comunicación

Dirección del prof. Marín (2026-07-14). Tres puntos, integrados:

### 9.1 Validación por TENDENCIA al crecer el sistema
No basta paridad a un tamaño fijo: hay bugs (sobre todo en la versión paralela)
que solo aparecen al aumentar gradualmente el tamaño. **Ambos motores deben
mostrar la misma tendencia** a medida que crece el modelo. Acción: extender la
validación a un barrido de tamaños crecientes (área × agentes), corriendo el
**modelo completo** (no solo el kernel) en C++ y en swarm-abm, y verificar que
la curva de la métrica agregada (p. ej. `loss_rate`, o conteos por especie)
coincide punto a punto al escalar — no solo un valor aislado.

### 9.2 Patrones de comunicación (uno-a-muchos procesadores)
En paralelo con descomposición de dominio, la comunicación entre procesadores es
el cuello de botella. Caso citado por Marín: un perro emite una señal a todas las
ovejas de su piño, repartidas en varios procesadores → patrón **uno-a-muchos**.
Análisis de los rangos de interacción del modelo vs el mapa (2000 m):

| Interacción | Radio | Alcance |
|---|---:|---|
| Percepción oveja / cordero | 100 / 50 m | local |
| Detección de presa (zorro) | 300 m | local |
| Cohesión / atracción al perro (oveja) | 500 m | local-medio |
| Detección de zorros (perro) | 1200 m | **medio-grande** (>½ mapa) |
| **`fox_active` percibe perro (territorio)** | **6135 / 4295 m** | **GLOBAL (> diagonal del mapa)** |

Dos patrones no-locales concretos:
- **Global (all-to-any)**: cada zorro/chilla consulta si hay un perro dentro de
  6135/4295 m — es decir, en todo el mapa. Todo zorro es afectado por *cualquier*
  perro. Como los perros son pocos (`n_dogs` ≤ ~4), la solución natural es
  **replicar las posiciones de los perros en todos los procesadores cada
  superpaso** (broadcast barato). Ese es el patrón uno-a-muchos de Marín, y su
  costo es bajo por el número chico de emisores.
- **Medio (halo)**: dog↔zorro (1200 m), oveja↔perro (500 m), zorro↔presa (300 m)
  se cubren con **halo/ghost** de ancho ≥ 1200 m en las fronteras de bloque.

**Implicancia de escala (clave):** a 2000 m el modelo **no se descompone bien** —
el halo (1200 m) es del orden del bloque para P≥4 (bloques de 1000 m). La
descomposición de dominio solo paga cuando el **área** crece hasta que los radios
de interacción son chicos frente al bloque (p. ej. una estancia de decenas de km
→ bloques de varios km con halos de 1,2 km). Esto conecta con 9.1 y con definir
el techo de agentes/área con Marín: **es escalar el área lo que habilita la
descomposición**, no solo agregar agentes en la misma área.

### 9.3 Sincronización conservadora — el modelo tick-based YA es YAWNS/BSP
YAWNS (*Yet Another Windowing Network Simulator*, Nicol) es un algoritmo
**conservador** de PDES: en vez de sincronizar evento por evento (Chandy–Misra–
Bryant) o optimista con rollback (Time Warp), avanza todos los procesos hasta una
frontera temporal común `T+ε` y sincroniza con una **barrera global**; es exacto
si `ε ≤ lookahead` (el mínimo retardo antes de que un evento afecte a otro LP).
Es, en esencia, **BSP** — encaja con el BSPonMPI que pide Marín.

Mapeo a nuestro caso: el modelo es **time-stepped con instantánea de inicio de
paso** (`before_step`: todos leen el estado congelado del tick). Eso ya es una
simulación conservadora con **ventana ε = 1 tick**: (1) cómputo local leyendo
halo/ghost del vecino, (2) barrera, (3) intercambio de agentes de frontera. No
hay causalidad violable dentro del tick porque nadie ve las mutaciones del tick
en curso. Es decir: **el port, tal como está, es directamente paralelizable en
modo conservador BSP/YAWNS** — la instantánea que replicamos para la paridad es
la misma que hace segura la ejecución paralela.

- **OpenMP (un nodo, memoria compartida)**: la barrera es implícita entre las dos
  fases (todos leen `snap`, todos escriben `new_pos`); es el Hito 4 directo.
- **BSPonMPI (multi-nodo)**: superpasos = ticks; halo exchange de agentes de
  frontera + broadcast de perros; ε=1 tick, lookahead=1 tick. YAWNS en su forma
  estricta (ε ≤ min retardo de evento) recién sería necesario **si** se pasa a
  modo *event-driven* (que Marín mencionó para baja densidad de agentes); ahí ε
  se define por el mínimo tiempo entre eventos que cruzan procesadores.

**Pendiente para Manuel** (encargo de Marín): estudiar YAWNS y si conviene la
variante event-driven conservadora. Referencia: buscar "parallel discrete event
simulation YAWNS" (Nicol; ROSS implementa YAWNS conservador y Time Warp
optimista).
