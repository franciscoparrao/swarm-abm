# SIGRID — Modelo e implementación (nota técnica para Manuel)

Resumen del modelo actual y su implementación paralela, con foco en las
propiedades que importan para evaluar la vía DEVS / PDES.

## 1. El modelo

**Qué es.** ABM espacial de depredación de ovejas (SIGRID). Estudia la efectividad
de perros guardianes frente a zorros, calibrado contra mediciones de terreno (Ana
Justi). Métrica de interés: **tasa de pérdida** del rebaño.

**Espacio y tiempo.**
- Espacio **continuo 2D** (una estancia simulada; **no georreferenciado aún** — eso
  es para la 2ª publicación con los datos GPS de collares).
- Índice espacial = **grilla de celdas** (cell list) reconstruida cada paso.
- Dos rásters de entorno: calidad de vegetación y cobertura.
- Tiempo por **ticks horarios**; ~1 mes = **720 pasos**.

**Agentes** (heterogéneos, `enum Species` + reglas por especie):
- **Oveja**: adulta y cordero (maduración a 120 días); percepción 100 m (cordero 50),
  miedo/huida, forrajeo con cohesión y evitación de riesgo, energía.
- **Zorro culpeo** (principal) y **chilla** (menor, época de parición): territorio
  6135 / 4295 m, detección 300 m, ataque 50 m, curva de actividad horaria, hambre,
  aversión al riesgo, memoria de peligro (TTL 168 h).
- **Perro** (la intervención): patrulla 300 m/h vs persecución 3000 m/h, detección
  1200 m, disuasión 50 m.
- **Liebre** (presa alternativa — el 4º agente que destrabó los resultados): baja la
  depredación sobre la oveja; el perro también la espanta.

**Mecánica de activación (crítica para lo de DEVS/PDES):**
- **Instantánea de inicio de paso** (`snapshot`): al comenzar el tick se congela el
  estado; **todas las consultas leen ese estado congelado**, las mutaciones (mover,
  matar, miedo) van sobre el arreglo vivo y **se ven recién en el tick siguiente**.
- **Activación aleatoria** (orden barajado por paso).
- **"Budget" por probabilidad**: cada agente, cada tick, ejecuta actividades
  (comer/moverse/…) con cierta probabilidad. No hay ticks "vacíos": todos actúan
  siempre (aunque la oveja que no se mueve ya salta la parte cara).

## 2. Propiedades que definen la paralelización

- **Lookahead = 1 tick.** Por la instantánea, una acción en `t` afecta la percepción
  de otros recién en `t+1`. → cualquier sincronización conservadora está atada a
  ventanas de 1 tick.
- **Dos fases por tick:**
  - **Fase A** (ovejas + liebres): independientes — leen snapshot + rásters, se
    escriben a sí mismas → **paralelizable**.
  - **Fase B** (zorros + perros): mutan estado compartido (matanzas, contadores,
    zonas de peligro) + orden secuencial barajado → **secuencial**.
- **Acoplamiento global:** el territorio del zorro (6135 m) supera la diagonal del
  mapa → **todo zorro es afectado por cualquier perro** (señal global, no local).
- **RNG por-agente** `(semilla, tick, id)`: el azar de cada agente depende de su
  identidad, no del orden de ejecución → resultado **independiente del scheduling**.
- **Perfil de cómputo:** las ovejas dominan ~11× el conteo de ticks-agente; a
  densidad alta el cuello es **memory-bound** (ancho de banda), no cómputo puro.

## 3. Implementación (estado actual)

| Motor | Rol | Estado |
|---|---|---|
| **swarm-abm** (Rust) | **Oráculo** de referencia, determinista (ChaCha8) | Validado vs Mesa/Python |
| **C++ serial/OpenMP** | Producción, un nodo | Bit-idéntico 1/4/16 hilos; ~3× |
| **C++ memory-bound** (CSR+SoA) | Optimización de layout | Bit-idéntico; relaja el techo de ancho de banda |
| **C++ BSPonMPI** | Multi-nodo (BSPlib real) | Bit-idéntico al oráculo (6/6 configs) |

**Validación:** paridad **distribucional** vs oráculo (Pearson **0.9986–0.9997**;
el RNG difiere entre lenguajes, por eso no es bit-exacta cross-lenguaje); validación
por **tendencia** al escalar (r 0.9995, sin bug de escala); y **bit-identidad**
serial == OpenMP == BSPonMPI (con `-ffp-contract=off`, tras cazar un desvío de 1 ULP
por contracción FP/FMA).

**Multi-nodo — arquitectura y hallazgo:**
- Fase A distribuida por **franjas** en x + **halo** de 500 m (snapshot local
  ordenado por gid ⇒ mismo orden intra-celda que el serial).
- Fase B **centralizada** en proc 0 (código serial exacto) + **broadcast de perros**
  (el patrón uno-a-muchos, barato porque son pocos).
- **No acelera** al agregar nodos (P2 9.2 s → P8 18.8 s): **comm-bound** (fase B
  centralizada = cuello serial + gather O(N)/tick). Previsto por §9.2 del plan: la
  descomposición solo paga a áreas mucho mayores.

## 4. Lo que esto implica para DEVS / PDES

- El tick **es** la macrotransición (§1.5 de Zeigler): agrupa las micro-decisiones
  del budget. Existe el homomorfismo; la equivalencia DTSS ⊂ DEVS está.
- Vía **conservadora** (YAWNS/BSP): con lookahead = 1 tick degenera al BSP ya hecho;
  el cuello no es la sincronización sino la **comunicación** (señal global del perro
  + gather).
- Vía **optimista/aproximada** (aún sin evaluar): la señal global del perro y el
  costo de *state-saving* con muchos agentes son los riesgos a medir; el "aproximado"
  coincide con la ruta de **paridad distribucional** (soltar bit-identidad para
  distribuir también la fase B).

**Escala real:** pendiente (nº de ovejas/zorros/liebres) — lo aporta Ana Justi.

## 5. Punteros para revisar el código

- Núcleo del modelo: `validation/cpp-sigrid/sigrid_core.hpp` (constantes, structs,
  RNG, grid, fases, `build_model`).
- Multi-nodo + estudio: `validation/mpi-sigrid/` (`sheep_fox_bsp.cpp`,
  `EVENT_DRIVEN_STUDY.md`).
- Plan técnico (incl. §9, comunicación uno-a-muchos y BSP/YAWNS):
  `docs/PLAN_PORT_CPP_SIGRID.md`.
