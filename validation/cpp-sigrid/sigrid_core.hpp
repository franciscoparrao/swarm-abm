// sigrid_core.hpp — núcleo del modelo SIGRID (constantes, structs, RNG, Grid,
// rasters, Model, funciones de step). FUENTE ÚNICA compartida por la versión
// OpenMP (sheep_fox.cpp) y la versión BSPonMPI multi-nodo (sheep_fox_bsp.cpp),
// para garantizar que ambas ejecutan EXACTAMENTE la misma lógica de modelo.
#ifndef SIGRID_CORE_HPP
#define SIGRID_CORE_HPP
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <cmath>
#include <cstdint>
#include <vector>
#include <random>
#include <algorithm>
#include <unordered_map>
#ifdef _OPENMP
#include <omp.h>
#endif

// Determinismo cross-compilador (IMPORTANTE): compilar SIEMPRE con
// `-ffp-contract=off`. Sin él, g++ y bspcxx (o OpenMP vs LTO) fusionan `a*b+c` en
// un FMA de forma distinta y el resultado difiere ~1 ULP; en el sistema de umbral
// de alta varianza que es la predación con perros, ese 1 ULP se amplifica a
// diferencias visibles (una matanza de más/menos). Con la contracción desactivada
// el MODELO computa idéntico bit a bit en cualquier compilador, que es lo que hace
// verificable la bit-identidad serial == OpenMP == BSPonMPI.
// (El `#pragma STDC FP_CONTRACT OFF` no basta: GCC lo ignora en C++; hay que usar
// el flag de compilación. Se deja el pragma por si se compila con clang.)
#pragma STDC FP_CONTRACT OFF

// ---- constantes (del SIGRID committeado) ----
static const double CELL_SIZE = 30.0;   // resolución del raster
static const double SPACE_CELL = 60.0;  // celda del índice espacial
static const double TAU = 6.283185307179586;
static const double SHEEP_SPEED = 50.0, SHEEP_FLEE_SPEED = 100.0, SHEEP_PERCEPTION_RADIUS = 100.0, SHEEP_ADULT_VULN = 0.4;
static const double LAMB_SPEED = 35.0, LAMB_FLEE_SPEED = 60.0, LAMB_PERCEPTION_RADIUS = 50.0, LAMB_FEAR_DECAY = 0.05;
static const double LAMB_MATURATION_DAYS = 120.0, LAMB_VULN = 0.8;
static const double FOX_SPEED_WALK = 500.0, FOX_DETECTION_RADIUS = 300.0, FOX_TERRITORY_RADIUS = 6135.0;
static const double HUNGER_THRESHOLD = 0.3, BASE_RISK_AVERSION = 0.6, FOX_ATTACK_RADIUS = 50.0;
static const double FOX_ACT_PEAK_ND = 0.0, FOX_ACT_AMP_ND = 0.90, FOX_ACT_SIGMA_ND = 4.5, FOX_ACT_BASE_ND = 0.05; // sin perro
static const double FOX_ACT_PEAK_WD = 1.5, FOX_ACT_AMP_WD = 0.45, FOX_ACT_SIGMA_WD = 3.0, FOX_ACT_BASE_WD = 0.30; // con perro
static const double FOX_P_MIN = 0.02, FOX_P_MAX = 0.35;
// Perros y disuasión
static const double DOG_PROXIMITY_VIGILANCE = 100.0, DOG_SPEED_PATROL = 300.0, DOG_SPEED_CHASE = 3000.0;
static const double DOG_DETECTION_RADIUS = 1200.0, DOG_CHASE_RADIUS = 200.0, DOG_PATROL_RADIUS = 250.0;
static const double DOG_DETER_RADIUS = 50.0, DOG_PROTECTION_STRENGTH = 0.20, DOG_AVOID_RADIUS = 500.0;
static const double DANGER_RADIUS = 400.0;
static const uint64_t DANGER_TTL = 168;
// Liebre (presa alternativa) y chilla (segundo depredador)
static const double HARE_SPEED_NORMAL = 100.0, HARE_SPEED_FLEE = 800.0, HARE_PERCEPTION_RADIUS = 80.0;
static const double HARE_MATURITY_AGE_H = 60.0 * 24.0, HARE_VULN_JUV = 0.9, HARE_VULN_MATURE = 0.6;
static const double CHILLA_TERRITORY_RADIUS = 4295.0;

// ---------------------------------------------------------------------------
// Dinámica poblacional de la liebre (port del Python, agents/hare.py).
// El C++ colocaba las liebres al inicio y sólo las quitaba por depredación, de
// modo que la población se agotaba durante la corrida, el switching perdía
// fuerza y los zorros pasaban a las ovejas. El Python las reproduce y las poda
// hacia una capacidad de carga, así que la densidad INICIAL es una condición
// inicial y no un tratamiento sostenido.
static const double HARE_REPRODUCTION_RATE   = 0.02;   // prob. por paso (hora)
static const double HARE_CARRYING_CAPACITY   = 0.35;   // liebres/ha
static const double HARE_REPRO_ENERGY_MIN    = 70.0;
static const double HARE_REPRO_DAYS_MIN      = 30.0;
static const double HARE_REPRO_FEAR_MAX      = 0.3;
static const double HARE_REPRO_COST          = 20.0;
static const double HARE_LOCAL_RADIUS        = 500.0;  // radio de densidad local
static const double HARE_FORAGE_GAIN         = 1.5;    // × calidad de pastizal
static const double HARE_UPKEEP              = 0.5;    // + fear × 1.5
static const int    HARE_MAX_OFFSPRING       = 4;      // randint(2,5) -> 2..4

// ---------------------------------------------------------------------------
// A1 — presupuesto energético (port del Python, agents/fox.py, 2026-08-12).
// La saciedad escala con la biomasa APROVECHABLE de la presa. Antes de esta
// corrección el modelo hacía `hunger = 0` tras cualquier captura, de modo que
// una liebre de 4 kg y una oveja de 40 kg saciaban igual y el consumo per
// cápita subía a varias veces el requerimiento de mantención.
// φ (fracción aprovechable) cae con el tamaño porque lo que limita el uso de
// una carcasa grande no es su masa sino la ventana de 2-5 días antes de que
// la descomposición y los carroñeros la retiren.
static const double PREY_MASS_HARE = 4.0,  PREY_USABLE_HARE  = 0.85;
static const double PREY_MASS_LAMB = 8.0,  PREY_USABLE_LAMB  = 0.50;
static const double PREY_MASS_EWE  = 40.0, PREY_USABLE_EWE   = 0.10;
static const int    STEPS_PER_DAY  = 24;               // pasos de 1 hora
static const double HUNGER_RATE_PER_STEP = 0.01;
static const double HUNGER_RATE_PER_DAY  = HUNGER_RATE_PER_STEP * STEPS_PER_DAY; // 0.24
static const double FOX_DAILY_INTAKE_KG    = 0.45;     // culpeo
static const double CHILLA_DAILY_INTAKE_KG = 0.22;     // chilla (~3 kg)
static const double SATIATION_FLOOR = -15.0;           // crédito energético acotado

// A2 — preferencia saturante por presa silvestre (port del Python).
// Reemplaza los escalones codificados a mano (+0.4 liebre / +0.2 si >=3 /
// -0.3 oveja si >=2), cuya magnitud nunca apareció en el manuscrito ni se
// barrió. Ahora es un par de parámetros explícitos y una función saturante.
// No son const: el manuscrito calibra la FUERZA del switching como un escalar
// lambda sobre ambos betas (lambda = 1.0 reproduce los defaults del Python).
// La celda calibrada del paper es lambda ~ 0.20-0.27.
static double PREY_SWITCH_BETA_HARE  = 1.2;
static double PREY_SWITCH_BETA_SHEEP = 0.6;
static double PREY_SWITCH_N_HALF     = 3.0;
// Términos del score de selección que faltaban en el port. El de aislamiento
// es el decisivo: la oveja va en majada (nearby alto -> bonus ~0) y la liebre
// anda dispersa (nearby ~0 -> bonus ~0.3), lo que invierte el ranking entre
// cordero y liebre en el punto de operación calibrado.
static const double FOX_ISOLATED_BONUS      = 0.3;
// Índice de aislamiento: la celda es de 50 m (= el radio de conteo), y por debajo
// de ISO_DIRECT_MAX presas el escaneo directo O(p²) es más barato que armar el hash.
static const double ISO_CELL        = 50.0;
#ifdef ISO_DIRECT_MAX_OVERRIDE
static const size_t ISO_DIRECT_MAX  = ISO_DIRECT_MAX_OVERRIDE;  // para el A/B de bit-identidad
#else
static const size_t ISO_DIRECT_MAX  = 64;
#endif
static const double FOX_FAR_FROM_DOG_BONUS  = 0.3;
static const double FOX_SUCCESS_ZONE_BONUS  = 0.2;
// Gate logístico de aborto (port del Python). El corte duro
// `if (risk > risk_aversion)` hacía que un incremento de riesgo que no cruza
// el umbral no cambiara nada; con el gate, la probabilidad de abortar sube
// suavemente alrededor de la aversión.
static const double RISK_ABORT_SHARPNESS = 10.0;
// Perro: comportamientos que faltaban en el port. El orden de prioridad del
// Python es reactivo: amenaza > rastro olfativo > arreo de dispersas > patrulla.
// El arreo importa: la bitácora del proyecto documenta que el fallo de "un
// perro empeora" no venía de la tasa de intentos sino de la letalidad sobre
// stragglers que un solo perro no mantiene agrupados.
static const double DOG_TRAIL_RADIUS      = 1500.0;   // radio olfativo
static const uint64_t DOG_TRAIL_TTL_HOURS = 2;        // 1 paso = 1 hora
static const double DOG_SCATTERED_THRESHOLD = 150.0;  // distancia al centro
// Toggle de fidelidad: si SIGRID_NO_STALK está definido, el zorro mata al
// contacto (como el Python) en vez de acechar cuando hay un perro cerca.
static const bool FOX_NO_STALK = (std::getenv("SIGRID_NO_STALK") != nullptr);
// Si SIGRID_FAITHFUL_DETER está definido, la disuasión del perro sólo agrega una
// zona peligrosa (como dog.chase del Python), sin resetear hunger/fear del zorro.
static const bool DOG_FAITHFUL_DETER = (std::getenv("SIGRID_FAITHFUL_DETER") != nullptr);

// ---------------------------------------------------------------------------
// Paisaje del miedo perro->zorro (port del Python, agents/fox.py::evaluate_risk
// + behavior/landscape_of_fear.py::FoxFearLandscape). Era el bloque que faltaba
// para que el efecto del LGD coincidiera con el Python (medido 30.9 % C++ vs
// 42.1 % Python): sin el paisaje del miedo y con el zorro percibiendo al perro
// solo a DOG_AVOID_RADIUS=500 m, la disuasión del perro quedaba subrepresentada.
//
// La `evaluate_risk` del Python suma en la pathway del perro tres términos que
// aquí faltaban, todos escalados por dog_risk_multiplier (1.0 culpeo / 1.8
// chilla):
//   (1) proximidad al perro más cercano dentro de FOX_DOG_DETECTION_RADIUS
//       (1500 m, no 500): (1 - d/1500) * DOG_PROXIMITY_RISK_WEIGHT
//   (2) piso ambiental DOG_AMBIENT_RISK si hay perros pero ninguno en rango
//       (ningún punto del mapa es un santuario absoluto — estabiliza colas)
//   (3) paisaje del miedo (capa espacial acumulada): fear(x,y) * DOG_FEAR_LANDSCAPE_WEIGHT
// Con params.n_dogs == 0 los tres términos son EXACTAMENTE cero, así que este
// bloque no altera el brazo "sin perro" (nivel base ya en paridad).
static const double FOX_DOG_DETECTION_RADIUS = 1500.0;  // radio de percepción del perro (era 500)
static const double DOG_PROXIMITY_RISK_WEIGHT = 0.7;
static const double DOG_FEAR_LANDSCAPE_WEIGHT = 0.7;
static const double DOG_AMBIENT_RISK          = 0.08;
static const double CHILLA_DOG_RISK_MULTIPLIER = 1.8;   // dog_risk_multiplier de la chilla
// FoxFearLandscape: el perro intimida al zorro en un radio de 600 m con caída
// gaussiana (sigma = radio*0.4). intensidad ≡ 1.0 (is_patrolling/is_alerted no
// existen en el código Python) e historical_fear ≡ 0 (los eventos
// dog_deterrence/dog_attack_fox nunca se generan), así que el paisaje se reduce
// a: fear_map = max_perros exp(-0.5 (d/(600*0.4))^2), combined = 0.6*fear_map,
// luego un suavizado gaussiano (scipy.ndimage.gaussian_filter, sigma=1).
static const double FOX_FEAR_INTENSITY   = 1.0;     // is_patrolling/is_alerted no existen -> ≡ 1.0
static const double FOX_FEAR_RADIUS      = 600.0;   // radio de miedo del perro sobre el zorro
static const double FOX_FEAR_SIGMA_FRAC  = 0.4;     // sigma espacial = radio * frac
static const double FOX_FEAR_COMBINE_W   = 0.6;     // peso de fear_map en combined_risk
static const double FOX_FEAR_GF_SIGMA    = 1.0;     // sigma (en celdas) del gaussian_filter
static const int    FOX_FEAR_GF_RADIUS   = 4;       // int(truncate*sigma+0.5), truncate=4

static inline void set_switch_lambda(double lam) {
    PREY_SWITCH_BETA_HARE  = 1.2 * lam;
    PREY_SWITCH_BETA_SHEEP = 0.6 * lam;
}

enum Species { SHEEP = 0, FOX = 1, DOG = 2, HARE = 3 };

// intensity: riesgo con que se registró la zona (Python: add_dangerous_zone
// guarda intensity=risk y evaluate_risk/select_target la usan como intensity*0.3).
struct DangerZone { double x, y; uint64_t step; double intensity = 1.0; };

struct Animal {
    int species; double x, y; bool alive; double energy, age_days, fear;
    bool is_lamb; double vulnerability; int mother; // índice o -1
    double hunger, risk_aversion, predation_eff;
    bool is_chilla = false, mature = true; double territory_radius = 0.0;
    // zorro: memoria de zonas peligrosas + presa en acecho
    std::vector<DangerZone> danger_zones; int stalk_target = -1;
    // perro: patrullaje circular
    double patrol_angle = 0.0, patrol_cx = 0.0, patrol_cy = 0.0;
    // granja de origen (ovejas/corderos/perros, especies cercadas). Los zorros y
    // liebres cruzan predios libres y no usan este campo.
    int farm = 0;
    // unidad de cerco (potrero) de origen de las especies cercadas. Sin raster de
    // potreros coincide con la granja (unit_of == farm_of).
    int unit = 0;
    // liebre: reproducción. La decisión (cuántas crías y dónde) se toma en la
    // fase A paralela con el RNG por-agente y se MATERIALIZA en una fase
    // secuencial posterior: así el resultado no depende del orden de hilos y
    // ningún hilo escribe en el vector de agentes.
    double days_since_repro = 0.0;
    int pending_offspring = 0;
    double off_x[HARE_MAX_OFFSPRING] = {0,0,0,0}, off_y[HARE_MAX_OFFSPRING] = {0,0,0,0};
};

// Biomasa aprovechable (kg) de una presa, según su tipo.
static inline double prey_usable_kg(const Animal& prey) {
    if (prey.species == HARE) return PREY_MASS_HARE * PREY_USABLE_HARE;
    if (prey.is_lamb)         return PREY_MASS_LAMB * PREY_USABLE_LAMB;
    return PREY_MASS_EWE * PREY_USABLE_EWE;   // adulta: la categoría conservadora
}

// Consume la presa y reduce el déficit en proporción a la biomasa asimilable.
// Una unidad de hambre equivale a `daily_intake` kg de presa.
static inline void feed_on(Animal& fox, const Animal& prey) {
    const double usable_kg = prey_usable_kg(prey);
    const double daily_intake = fox.is_chilla ? CHILLA_DAILY_INTAKE_KG : FOX_DAILY_INTAKE_KG;
    const double hunger_units = usable_kg * (HUNGER_RATE_PER_DAY / daily_intake);
    fox.hunger = std::max(SATIATION_FLOOR, fox.hunger - hunger_units);
}

struct Snap { double x, y; int species; bool alive, is_lamb; double vulnerability, energy; int mother; };
// Layout SoA para la instantánea (Hito 5): coords calientes separadas de los
// campos fríos. `Snap` sigue siendo la interfaz del callback (se reconstruye en
// el acierto), así que los cuerpos de las consultas no cambian.
struct XY { double x, y; };
struct SnapCold { double vulnerability, energy; int species, mother; uint8_t alive, is_lamb; };

struct Raster {
    int cols, rows; double height; std::vector<double> data;
    double get(double px, double py) const {
        int col = (int)std::floor(px / CELL_SIZE); if (col < 0) col = 0; if (col > cols - 1) col = cols - 1;
        int row = (int)std::floor((height - py) / CELL_SIZE); if (row < 0) row = 0; if (row > rows - 1) row = rows - 1;
        return data[(size_t)row * cols + col];
    }
};

struct Rng { // envoltorio sobre mt19937_64 con helpers estilo swarm-abm (solo build)
    std::mt19937_64 g;
    explicit Rng(uint64_t s) : g(s) {}
    double unit() { return std::uniform_real_distribution<double>(0.0, 1.0)(g); }        // [0,1)
    double range(double a, double b) { return std::uniform_real_distribution<double>(a, b)(g); }
    double normal(double std) { double u1 = range(1e-12, 1.0), u2 = unit(); return std::sqrt(-2.0 * std::log(u1)) * std::cos(TAU * u2) * std; }
};

// RNG por-agente (splitmix64): barato de sembrar por (semilla, tick, id), lo que
// hace el resultado INDEPENDIENTE del orden de ejecución -> determinista bajo
// paralelismo (la idea de `child_rng` de swarm-abm). Mismos helpers.
struct PRng {
    uint64_t s;
    explicit PRng(uint64_t seed) : s(seed) {}
    uint64_t next() {
        uint64_t z = (s += 0x9E3779B97F4A7C15ULL);
        z = (z ^ (z >> 30)) * 0xBF58476D1CE4E5B9ULL;
        z = (z ^ (z >> 27)) * 0x94D049BB133111EBULL;
        return z ^ (z >> 31);
    }
    double unit() { return (double)(next() >> 11) * (1.0 / 9007199254740992.0); } // [0,1)
    double range(double a, double b) { return a + unit() * (b - a); }
    double normal(double std) { double u1 = range(1e-12, 1.0), u2 = unit(); return std::sqrt(-2.0 * std::log(u1)) * std::cos(TAU * u2) * std; }
};
static inline uint64_t agent_seed(uint64_t base, uint64_t tick, uint64_t id) {
    uint64_t z = base ^ (tick * 0x9E3779B97F4A7C15ULL) ^ (id * 0xD1B54A32D192ED03ULL);
    z = (z ^ (z >> 30)) * 0xBF58476D1CE4E5B9ULL;
    z = (z ^ (z >> 27)) * 0x94D049BB133111EBULL;
    return z ^ (z >> 31);
}

struct Params { double width = 2000, height = 2000, sheep_density = 0.96, fox_density = 8.4,
                fox_predation_effectiveness = 0.14, lamb_proportion = 0.2,
                chilla_density = 0.0, hare_density = 0.0; int n_dogs = 0;
                int farms_x = 1, farms_y = 1;   // particion sintetica en grilla (1x1 = una granja)
                double hunt_prob = 0.0;          // caceria: prob. por granja/dia (0 = inerte)
                const char* farm_raster_path = nullptr;    // raster real de granjas (si != null, fija width/height)
                const char* cascos_path = nullptr;          // casas reales por granja (ancla de cacería)
                const char* paddock_raster_path = nullptr;  // raster de potreros (cercos internos; misma grilla que el de granjas)
                const char* units_path = nullptr;           // tabla de potreros: area real + punto interior (reparto del rebaño)
                const char* season_path = nullptr;          // estación por unidad (0 invernada, 1 veranada); requiere potreros
                int season_sim = -1;                        // estación simulada: el rebaño parte SOLO en sus unidades (-1 = inerte)
                bool dog_roam = false;                      // perro confinado solo por la estancia; sigue al potrero con más ovejas
                const char* dogs_farm = nullptr;            // perros por estancia "n0,n1,..." (orden farm_id); reemplaza el round-robin de n_dogs
                double dog_leash = 0.0;                     // distancia máxima (m) del perro al centro de su rebaño (0 = sin límite)
                // Cacería régimen B (Ana 2026-11-03/05): el ovejero sale lunes-sábado, de
                // día, a un potrero con ovejas; mata con prob. por encuentro y por especie.
                int hunt_mode = 0;                          // 0 = régimen anterior (casa, hunt_prob); 1 = régimen B
                double hunt_p_culpeo = 0.0;                 // prob. de abatir un culpeo dentro del radio, por salida
                double hunt_chilla_ratio = 0.05;            // prob. chilla = hunt_p_culpeo * ratio (cacería selectiva)
                double hunt_radius = 500.0;                 // radio de detección/tiro del ovejero (m)
                double hunt_outing_prob = 1.0;              // prob. de salir cada día hábil (1 = régimen B; baja + p alta = régimen A)
                const char* hunt_farms = nullptr;           // estancias que cazan "1,0,0,0,1" (orden farm_id); nullptr = todas
                const char* farm_out_path = nullptr;        // CSV de resultados por estancia (se agrega una fila por semilla y estancia)
                // Vegetación real: sobrescribe los rásters sintéticos DESPUÉS de generarlos
                // (build_rasters consume el RNG de build; así el resto de la población es idéntico).
                const char* veg_quality_path = nullptr;     // calidad de forraje 0-1 (formato .farms, fila 0 = sur)
                const char* veg_cover_path = nullptr;       // cobertura vegetal 0-1 (mismo formato)
                double cover_effect = 0.05;                 // coef. de la cobertura en la prob. de captura (Paper 1: +0,05)
                const char* land_path = nullptr;            // máscara de tierra (1) / mar (0), misma grilla que --farm-raster
                bool fence_slide = false; };                // regla de cerco de la oveja: false = se detiene (0), true = lo rodea (1)

// ---- espacio: índice de celdas sobre la instantánea (CSR + SoA) ----
// Layout memory-bound (Hito 5): en vez de una lista ligada (head/nxt) sobre
// structs de 48 B, se hace un counting-sort de los agentes por celda —CSR:
// cellStart[c]..cellStart[c+1]— con las coordenadas calientes (x,y) en un arreglo
// contiguo aparte de los campos fríos. El barrido de vecindad toca solo 16 B por
// candidato de forma SECUENCIAL (amigable al prefetcher) y reúne los campos fríos
// solo en los aciertos. El ORDEN de visita se mantiene idéntico a la lista ligada
// (índice descendente dentro de la celda, mismo recorrido de bloque) => resultados
// bit-idénticos: cambia el layout, no la semántica.
struct Grid {
    int nx, ny; double csz, w, h;
    std::vector<int> cellStart;    // offsets CSR, tamaño nx*ny+1
    std::vector<XY> spos;          // coords calientes, ordenadas por celda
    std::vector<SnapCold> scold;   // campos fríos, mismo orden
    std::vector<int> sid;          // índice de agente original (para mutar el vivo)
    std::vector<int> hist;         // histogramas por-hilo [T*ncell], reusado entre pasos
    int cell(double px, double py) const {
        int cx = (int)(px / csz); if (cx < 0) cx = 0; if (cx > nx - 1) cx = nx - 1;
        int cy = (int)(py / csz); if (cy < 0) cy = 0; if (cy > ny - 1) cy = ny - 1;
        return cy * nx + cx;
    }
    // Counting-sort del índice, PARALELO y bit-idéntico al serial (Hito 5+):
    // histogramas locales por hilo + offsets por (hilo,celda) tales que dentro de
    // cada celda el orden final sea índice global DESCENDENTE, INDEPENDIENTE del
    // número de hilos. Clave: los chunks de índice más alto ocupan los primeros
    // slots de la celda y cada hilo scatterea su chunk en orden descendente, así
    // la concatenación [chunk T-1 desc][T-2 desc]...[0 desc] = global descendente,
    // igual que el recorrido serial (== lista ligada de Hito 4b).
    void build(const std::vector<Animal>& agents, double W, double H) {
        w = W; h = H; csz = SPACE_CELL;
        nx = std::max(1, (int)(W / csz)); ny = std::max(1, (int)(H / csz));
        size_t ncell = (size_t)nx * ny, n = agents.size();
        cellStart.assign(ncell + 1, 0);
        spos.resize(n); scold.resize(n); sid.resize(n);
        if (n == 0) return;
#ifdef _OPENMP
        int T = omp_get_max_threads();
        if ((size_t)T > n) T = (int)n; if (T < 1) T = 1;
#else
        int T = 1;
#endif
        // Camino serial (T==1): el counting-sort simple, sin el overhead de los
        // histogramas por-hilo. Idéntico al de Hito 5 -> sin regresión a 1 hilo.
        if (T == 1) {
            for (size_t i = 0; i < n; ++i) cellStart[cell(agents[i].x, agents[i].y) + 1]++;
            for (size_t c = 0; c < ncell; ++c) cellStart[c + 1] += cellStart[c];
            hist.assign(ncell, 0);
            for (size_t c = 0; c < ncell; ++c) hist[c] = cellStart[c];  // cursor = inicio de celda
            for (size_t ii = n; ii-- > 0; ) {
                const Animal& a = agents[ii];
                int p = hist[cell(a.x, a.y)]++;
                spos[p] = {a.x, a.y};
                scold[p] = {a.vulnerability, a.energy, a.species, a.mother, (uint8_t)a.alive, (uint8_t)a.is_lamb};
                sid[p] = (int)ii;
            }
            return;
        }
        // Camino paralelo (T>1): un solo team. Fase 1 histograma local; luego un
        // `single` calcula cellStart y los cursores por (hilo,celda); Fase 3 scatter.
        hist.assign((size_t)T * ncell, 0);
        #pragma omp parallel num_threads(T)
        {
#ifdef _OPENMP
            int t = omp_get_thread_num();
#else
            int t = 0;
#endif
            size_t lo = (size_t)t * n / T, hi = (size_t)(t + 1) * n / T;
            // Fase 1: histograma local del chunk de este hilo.
            { int* h = &hist[(size_t)t * ncell];
              for (size_t i = lo; i < hi; ++i) h[cell(agents[i].x, agents[i].y)]++; }
            #pragma omp barrier
            // Fase 2 (un solo hilo): totales -> cellStart (prefijo) y offset inicial
            // de cada (hilo,celda) tal que los hilos de índice mayor van primero.
            #pragma omp single
            {
                for (size_t c = 0; c < ncell; ++c) {
                    int total = 0;
                    for (int tt = 0; tt < T; ++tt) total += hist[(size_t)tt * ncell + c];
                    cellStart[c + 1] = total;
                }
                for (size_t c = 0; c < ncell; ++c) cellStart[c + 1] += cellStart[c];
                for (size_t c = 0; c < ncell; ++c) {
                    int acc = cellStart[c];
                    for (int tt = T - 1; tt >= 0; --tt) {
                        size_t idx = (size_t)tt * ncell + c; int cnt = hist[idx];
                        hist[idx] = acc; acc += cnt;
                    }
                }
            } // barrera implícita al cerrar el `single`
            // Fase 3: scatter del chunk en índice DESCENDENTE con cursor local
            // (rangos de slots disjuntos entre hilos -> sin conflictos de escritura).
            int* cur = &hist[(size_t)t * ncell];
            for (size_t ii = hi; ii-- > lo; ) {
                const Animal& a = agents[ii];
                int p = cur[cell(a.x, a.y)]++;
                spos[p] = {a.x, a.y};
                scold[p] = {a.vulnerability, a.energy, a.species, a.mother, (uint8_t)a.alive, (uint8_t)a.is_lamb};
                sid[p] = (int)ii;
            }
        }
    }
    // aplica f(snap, dist) a los vecinos a distancia <= radius (mismo orden que la lista ligada)
    template <class F> void within(double px, double py, double radius, F f) const {
        int cx = (int)(px / csz); if (cx < 0) cx = 0; if (cx > nx - 1) cx = nx - 1;
        int cy = (int)(py / csz); if (cy < 0) cy = 0; if (cy > ny - 1) cy = ny - 1;
        int cr = (int)(radius / csz) + 1;
        for (int dy = -cr; dy <= cr; ++dy) for (int dx = -cr; dx <= cr; ++dx) {
            int ax = cx + dx, ay = cy + dy; if (ax < 0 || ay < 0 || ax >= nx || ay >= ny) continue;
            int c = ay * nx + ax, e = cellStart[c + 1];
            for (int p = cellStart[c]; p < e; ++p) {
                double ddx = spos[p].x - px, ddy = spos[p].y - py; double d = std::sqrt(ddx * ddx + ddy * ddy);
                if (d <= radius) {
                    const SnapCold& cd = scold[p];
                    Snap sn{spos[p].x, spos[p].y, cd.species, (bool)cd.alive, (bool)cd.is_lamb, cd.vulnerability, cd.energy, cd.mother};
                    f(sn, d);
                }
            }
        }
    }
};

static double clampd(double v, double lo, double hi) { return v < lo ? lo : (v > hi ? hi : v); }

// ---- rasters sintéticos (réplica de build_rasters) ----
static void build_rasters(double W, double H, Rng& rng, Raster& quality, Raster& cover) {
    int cols = (int)std::ceil(W / CELL_SIZE), rows = (int)std::ceil(H / CELL_SIZE);
    quality = {cols, rows, H, std::vector<double>((size_t)cols * rows)};
    cover = {cols, rows, H, std::vector<double>((size_t)cols * rows)};
    for (int r = 0; r < rows; ++r) for (int c = 0; c < cols; ++c) {
        double x = (double)c / cols, y = (double)r / rows;
        double elev = 0.5 + 0.5 * (std::sin(x * 6.0) * std::cos(y * 6.0) + 0.5 * std::sin(x * 12.0) * std::cos(y * 12.0)) / 1.5;
        double cov = clampd(0.7 - elev * 0.4 + rng.range(-0.1, 0.1), 0.0, 1.0);
        double ndvi = clampd((1.0 - cov) * 0.5 + (1.0 - elev) * 0.5, 0.0, 1.0);
        double qual = clampd(ndvi * (1.0 + rng.range(-0.1, 0.1)), 0.0, 1.0);
        cover.data[(size_t)r * cols + c] = cov; quality.data[(size_t)r * cols + c] = qual;
    }
}

// ---------------------------------------------------------------------------
// Suavizado gaussiano separable equivalente a scipy.ndimage.gaussian_filter con
// sigma=1, truncate=4 (radio=4) y mode='reflect' (borde half-sample: el índice
// -1 mapea a 0, -2 a 1, ...; n mapea a n-1, etc.). Se aplica primero sobre el
// eje 0 (filas) y luego sobre el eje 1 (columnas), como hace scipy. El kernel es
// el de scipy _gaussian_kernel1d: phi(k)=exp(-0.5 k^2/sigma^2) normalizado en
// [-radio, radio]. No hace falta bit-identidad con Python (la validación C++ vs
// Python es distribucional), pero reproducir el kernel y el borde exactos evita
// introducir un desalineamiento numérico gratuito en el campo de miedo.
static void gaussian_blur_reflect(std::vector<double>& img, int cols, int rows) {
    const int R = FOX_FEAR_GF_RADIUS;
    const double sig = FOX_FEAR_GF_SIGMA, sig2 = sig * sig;
    double w[2 * FOX_FEAR_GF_RADIUS + 1]; double wsum = 0.0;
    for (int k = -R; k <= R; ++k) { double v = std::exp(-0.5 * (double)(k * k) / sig2); w[k + R] = v; wsum += v; }
    for (int k = 0; k < 2 * R + 1; ++k) w[k] /= wsum;
    auto reflect = [](int i, int n) { // borde 'reflect' de scipy (half-sample)
        if (n == 1) return 0;
        while (i < 0 || i >= n) { if (i < 0) i = -i - 1; if (i >= n) i = 2 * n - i - 1; }
        return i;
    };
    std::vector<double> tmp(img.size());
    // eje 0: filas (columna fija, se recorre la fila r+k)
    for (int c = 0; c < cols; ++c) for (int r = 0; r < rows; ++r) {
        double acc = 0.0;
        for (int k = -R; k <= R; ++k) { int rr = reflect(r + k, rows); acc += w[k + R] * img[(size_t)rr * cols + c]; }
        tmp[(size_t)r * cols + c] = acc;
    }
    // eje 1: columnas (fila fija, se recorre la columna c+k)
    for (int r = 0; r < rows; ++r) for (int c = 0; c < cols; ++c) {
        double acc = 0.0;
        for (int k = -R; k <= R; ++k) { int cc = reflect(c + k, cols); acc += w[k + R] * tmp[(size_t)r * cols + cc]; }
        img[(size_t)r * cols + c] = acc;
    }
}

// Recalcula el paisaje del miedo perro->zorro (FoxFearLandscape) desde las
// posiciones de perros al inicio del tick. `fear` ya viene dimensionado como los
// rasters de vegetación (mismos cols/rows/height/CELL_SIZE). Convención de
// celda idéntica a Raster::get y a _pos_to_grid del Python: col=x/CELL_SIZE,
// row=(height-y)/CELL_SIZE.
static void compute_fox_fear(Raster& fear, const std::vector<std::pair<double,double>>& dogs) {
    const int cols = fear.cols, rows = fear.rows; const double H = fear.height;
    std::fill(fear.data.begin(), fear.data.end(), 0.0);
    if (dogs.empty()) return;   // sin perros el campo es cero (no toca el brazo sin perro)
    const double denom = FOX_FEAR_RADIUS * FOX_FEAR_SIGMA_FRAC;   // 240
    const int rc = (int)(FOX_FEAR_RADIUS / CELL_SIZE);            // int(600/30)=20
    // fear_map[r,c] = max sobre perros de intensidad * exp(-0.5 (dist/denom)^2), dist<radio
    for (const auto& d : dogs) {
        int col = (int)std::floor(d.first / CELL_SIZE);
        int row = (int)std::floor((H - d.second) / CELL_SIZE);
        for (int i = -rc; i <= rc; ++i) for (int j = -rc; j <= rc; ++j) {
            int r = row + i, c = col + j;
            if (r < 0 || r >= rows || c < 0 || c >= cols) continue;
            double dist = std::sqrt((double)(i * i + j * j)) * CELL_SIZE;
            if (dist >= FOX_FEAR_RADIUS) continue;
            double fv = FOX_FEAR_INTENSITY * std::exp(-0.5 * (dist / denom) * (dist / denom));
            size_t idx = (size_t)r * cols + c;
            if (fv > fear.data[idx]) fear.data[idx] = fv;
        }
    }
    // combined_risk = clip(0.6*fear_map, 0, 1); como 0.6*fear ∈ [0,0.6], el clip
    // es identidad. Luego el suavizado gaussiano (historical_fear ≡ 0 -> se omite).
    for (double& v : fear.data) v = FOX_FEAR_COMBINE_W * v;
    gaussian_blur_reflect(fear.data, cols, rows);
}

struct Model {
    std::vector<Animal> agents; Grid grid;
    Raster veg_quality, veg_cover; Params params;
    std::vector<std::pair<double,double>> dog_positions;
    Raster fox_fear;   // paisaje del miedo perro->zorro (combined_risk suavizado)
    int current_hour = 0; uint64_t step_count = 0;
    long sheep_killed = 0, hares_killed = 0, predation_attempts = 0, att_sheep = 0, att_hare = 0;
    long foxes_hunted = 0;   // zorros abatidos por cacería humana
    long culpeos_hunted = 0, chillas_hunted = 0, hunt_outings = 0;   // desglose (régimen B)
    double dog_max_flock_dist = 0.0;   // diagnóstico: máxima distancia perro-centro de su rebaño tras moverse
    long fence_slides = 0, fence_stuck = 0;   // diagnóstico regla de cerco 1: pasos desviados / sin salida (fase A: atómicos)
    // Resultados por estancia (H1-H3: redistribución de la depredación entre vecinos).
    // Solo se acumulan; no alteran la simulación. fox_hours_* se llena si hay --farm-out.
    std::vector<long> sheep_init_farm, sheep_killed_farm, culpeos_hunted_farm, chillas_hunted_farm, outings_farm;
    std::vector<double> culpeo_hours_farm, chilla_hours_farm;
    std::vector<char> hunt_farm_mask;  // vacío = todas las estancias cazan
    // Rastro olfativo: posición y paso de los eventos de depredación recientes.
    struct TrailEvent { double x, y; uint64_t step; };
    long n_trail = 0, n_herd = 0, n_chase = 0;
    std::vector<TrailEvent> trail; int n_sheep_initial = 0;

    // Granjas (modelo multi-granja secuencial). Con n_farms<=1 el dominio es una
    // sola granja y toda la logica de cercos queda inerte (bit-identico al modelo
    // de una granja). farm_of usa el raster real si esta cargado; si no, una
    // particion analitica en grilla params.farms_x x params.farms_y (para pruebas).
    int n_farms = 1, farm_cols = 0, farm_rows = 0; double farm_csz = 30.0;
    std::vector<int> farm_raster;
    std::vector<std::pair<double,double>> farm_casco;  // centroide de cada granja (centro del rebaño, fallback de cacería)
    std::vector<std::vector<std::pair<double,double>>> farm_homesteads;  // casas reales por granja (ancla de cacería)
    int farm_of(double x, double y) const {
        if (n_farms <= 1) return 0;
        if (!farm_raster.empty()) {
            int cx = (int)(x / farm_csz); cx = cx < 0 ? 0 : (cx >= farm_cols ? farm_cols - 1 : cx);
            int cy = (int)(y / farm_csz); cy = cy < 0 ? 0 : (cy >= farm_rows ? farm_rows - 1 : cy);
            return farm_raster[(size_t)cy * farm_cols + cx];
        }
        int gx = (int)(x / (params.width  / params.farms_x)); gx = gx < 0 ? 0 : (gx >= params.farms_x ? params.farms_x - 1 : gx);
        int gy = (int)(y / (params.height / params.farms_y)); gy = gy < 0 ? 0 : (gy >= params.farms_y ? params.farms_y - 1 : gy);
        return gy * params.farms_x + gx;
    }

    // Potreros (cercos internos): segundo nivel de particion, SOLO para la
    // contencion (move_fenced) y la cohesion del rebano. La estancia (farm) sigue
    // siendo la unidad de manejo: rebano, perro, caceria, cascos. Cada unidad
    // pertenece a una sola granja (unit_farm, verificado al cargar). Sin raster de
    // potreros, unit_of == farm_of y todo lo de abajo queda inerte -> bit-identico.
    int n_units = 0;
    std::vector<int> paddock_raster;      // misma grilla (cols/rows/csz) que farm_raster
    std::vector<int> unit_farm;           // unidad -> granja
    std::vector<double> unit_weight;      // peso del reparto inicial (area real si hay .units; si no, celdas)
    std::vector<std::pair<double,double>> unit_center;  // centro del rebano inicial (punto interior si hay .units; si no, centroide)
    std::vector<int> unit_season;         // estación de cada unidad (0 invernada, 1 veranada); vacío si no se cargó
    // Máscara de tierra (misma grilla que farm_raster): liebres y zorros no nacen ni
    // entran al mar, y sus densidades se refieren al área de tierra. Vacía = todo es
    // tierra (comportamiento anterior, bit-idéntico).
    std::vector<uint8_t> land_mask; double land_area_ha = 0.0;
    bool is_land(double x, double y) const {
        if (land_mask.empty()) return true;
        int cx = (int)(x / farm_csz); cx = cx < 0 ? 0 : (cx >= farm_cols ? farm_cols - 1 : cx);
        int cy = (int)(y / farm_csz); cy = cy < 0 ? 0 : (cy >= farm_rows ? farm_rows - 1 : cy);
        return land_mask[(size_t)cy * farm_cols + cx] != 0;
    }
    bool has_paddocks() const { return !paddock_raster.empty(); }
    int unit_of(double x, double y) const {
        if (paddock_raster.empty()) return farm_of(x, y);
        int cx = (int)(x / farm_csz); cx = cx < 0 ? 0 : (cx >= farm_cols ? farm_cols - 1 : cx);
        int cy = (int)(y / farm_csz); cy = cy < 0 ? 0 : (cy >= farm_rows ? farm_rows - 1 : cy);
        return paddock_raster[(size_t)cy * farm_cols + cx];
    }

    void before_step() {
        current_hour = (int)(step_count % 24); step_count++;
        dog_positions.clear();
        for (size_t i = 0; i < agents.size(); ++i) {
            const Animal& a = agents[i];
            if (a.alive && a.species == DOG) dog_positions.push_back({a.x, a.y});
        }
        // Paisaje del miedo del zorro: recomputado aquí (serial, antes de las
        // fases de agentes) desde las posiciones de perros de inicio de tick.
        // Read-only durante las fases A/B -> no afecta la bit-identidad
        // serial == OpenMP. Con dog_positions vacío queda en cero.
        compute_fox_fear(fox_fear, dog_positions);
        grid.build(agents, params.width, params.height);
    }

    // Riesgo percibido por el zorro (paisaje del miedo perro->zorro) en (px,py).
    double fox_perceived_risk(double px, double py) const {
        if (dog_positions.empty()) return 0.0;   // get_fox_perceived_risk -> 0.0 sin perros
        return fox_fear.get(px, py);
    }

    // nearest_dog_dist: menor distancia a un perro dentro de `max`, o -1 si ninguno.
    double nearest_dog_dist(double px, double py, double max) const {
        double best = -1.0;
        for (auto& d : dog_positions) {
            double dd = std::sqrt((px - d.first) * (px - d.first) + (py - d.second) * (py - d.second));
            if (dd <= max && (best < 0 || dd < best)) best = dd;
        }
        return best;
    }

    // nearest_dog_pos: posición del perro más cercano dentro de `max`; devuelve
    // false si no hay ninguno. Usado por avoid_and_search (huir del perro).
    bool nearest_dog_pos(double px, double py, double max, double& ox, double& oy) const {
        double best = -1.0; bool found = false;
        for (auto& d : dog_positions) {
            double dd = std::sqrt((px - d.first) * (px - d.first) + (py - d.second) * (py - d.second));
            if (dd <= max && (best < 0 || dd < best)) { best = dd; ox = d.first; oy = d.second; found = true; }
        }
        return found;
    }
};

// dirección unitaria hacia el vecino de mayor quality (8 offsets * 50)
static void food_direction(const Raster& q, double px, double py, double& dx, double& dy) {
    double best = q.get(px, py); dx = 0; dy = 0;
    for (int k = 0; k < 8; ++k) { double a = TAU * k / 8.0, ox = std::cos(a), oy = std::sin(a);
        double v = q.get(px + ox * 50.0, py + oy * 50.0); if (v > best) { best = v; dx = ox; dy = oy; } }
}
static void norm(double& x, double& y) { double l = std::sqrt(x * x + y * y); if (l > 1e-12) { x /= l; y /= l; } else { x = 0; y = 0; } }

// Movimiento con cercos: las especies cercadas (oveja/cordero/perro) no cruzan a
// otra granja; si el destino cae en otra granja, el agente no se mueve (queda
// contra el cerco). Con n_farms<=1 nunca bloquea -> bit-identico. Zorros y
// liebres NO pasan por aqui (cruzan predios libres, se mueven con clampd directo).
static inline void move_fenced(Model& m, Animal& a, double nx, double ny) {
    nx = clampd(nx, 0, m.params.width); ny = clampd(ny, 0, m.params.height);
    // potreros: el cerco es la unidad (implica la granja). El perro con dog_roam cruza
    // los cercos internos y solo lo detiene el límite de su estancia.
    const bool by_unit = m.has_paddocks() && !(a.species == DOG && m.params.dog_roam);
    if (by_unit) {
        if (m.unit_of(nx, ny) != a.unit) return;
    } else if (m.n_farms > 1 && m.farm_of(nx, ny) != a.farm) return;
    a.x = nx; a.y = ny;
}
// ¿El cerco permite que `a` quede en (x,y)? Misma regla que move_fenced (x,y ya acotados).
static inline bool fence_allows(const Model& m, const Animal& a, double x, double y) {
    const bool by_unit = m.has_paddocks() && !(a.species == DOG && m.params.dog_roam);
    if (by_unit) return m.unit_of(x, y) == a.unit;
    return !(m.n_farms > 1 && m.farm_of(x, y) != a.farm);
}

// Movimiento de la oveja con el cerco. Regla 0 (default, Paper 1 y motor anterior):
// si el paso cruzaría el cerco, la oveja no se mueve ese paso (queda contra el cerco).
// Regla 1 (--fence-rule 1; A. Yusti-Muñoz 2026-11-10, respaldada en la literatura
// según ella): el cerco es una frontera de acceso; la oveja no lo salta, lo RODEA, y
// al alarmarse huye pero no necesariamente contra el cerco. Si el paso directo cruza,
// prueba el mismo largo girado ±30°, ±60°, ±90° (y ±120°, ±150° al pastorear) y toma
// el primero que no cruce; el lado que prueba primero se sortea con el RNG de la oveja
// (sin sesgo de giro). Al huir nunca gira más de 90°: no se acerca al zorro. En una
// esquina sin salida se queda. Inerte con regla 0 -> bit-idéntico.
static void move_sheep(Model& m, Animal& s, double dx, double dy, double sp, PRng& rng, bool fleeing) {
    const double nx = s.x + dx * sp, ny = s.y + dy * sp;
    if (!m.params.fence_slide) { move_fenced(m, s, nx, ny); return; }
    const double cx = clampd(nx, 0, m.params.width), cy = clampd(ny, 0, m.params.height);
    if (fence_allows(m, s, cx, cy)) { s.x = cx; s.y = cy; return; }
    const double max_deg = fleeing ? 90.0 : 150.0;
    const double sgn = (rng.unit() < 0.5) ? 1.0 : -1.0;
    for (double deg = 30.0; deg <= max_deg + 1e-9; deg += 30.0)
        for (int k = 0; k < 2; ++k) {
            const double th = (k == 0 ? sgn : -sgn) * deg * (TAU / 360.0);
            const double rx = dx * std::cos(th) - dy * std::sin(th), ry = dx * std::sin(th) + dy * std::cos(th);
            const double tx = clampd(s.x + rx * sp, 0, m.params.width), ty = clampd(s.y + ry * sp, 0, m.params.height);
            if (fence_allows(m, s, tx, ty)) {
                s.x = tx; s.y = ty;
                #pragma omp atomic
                m.fence_slides++;
                return;
            }
        }
    #pragma omp atomic
    m.fence_stuck++;
}

// Misma unidad de cerco que el agente `a` para un vecino en (x,y): potrero si hay
// raster de potreros, granja si no. Con n_farms<=1 siempre true -> bit-identico.
static inline bool same_fence(const Model& m, const Animal& a, double x, double y) {
    if (m.has_paddocks()) return m.unit_of(x, y) == a.unit;
    return (m.n_farms <= 1) || (m.farm_of(x, y) == a.farm);
}

// ---- oveja ----
static void step_sheep(Model& m, int i, PRng& rng) {
    Animal& s = m.agents[i];
    double decay = s.is_lamb ? LAMB_FEAR_DECAY : 0.1;
    s.fear = std::max(s.fear - decay, 0.0); s.age_days += 1.0 / 24.0;
    if (s.fear > 0.7) {
        double radius = s.is_lamb ? LAMB_PERCEPTION_RADIUS : SHEEP_PERCEPTION_RADIUS;
        double pbest = 1e300, ppx = 0, ppy = 0; bool found = false;
        m.grid.within(s.x, s.y, radius, [&](const Snap& sn, double d) {
            if (sn.alive && sn.species == FOX && d < pbest) { pbest = d; ppx = sn.x; ppy = sn.y; found = true; } });
        double sp = s.is_lamb ? LAMB_FLEE_SPEED : SHEEP_FLEE_SPEED;
        double dx, dy;
        if (found) { dx = s.x - ppx; dy = s.y - ppy; norm(dx, dy); }
        else { double a = rng.range(0.0, TAU); dx = std::cos(a); dy = std::sin(a); }
        move_sheep(m, s, dx, dy, sp, rng, true);
    } else {
        bool dog_near = m.nearest_dog_dist(s.x, s.y, DOG_PROXIMITY_VIGILANCE) >= 0;
        double p_move = dog_near ? (0.60 + 0.02) : (0.76 + 0.04);
        if (rng.unit() < p_move) {
            // graze_and_move
            double vfx, vfy; food_direction(m.veg_quality, s.x, s.y, vfx, vfy);
            double vrx = 0, vry = 0, worst = m.veg_cover.get(s.x, s.y);
            for (int k = 0; k < 8; ++k) { double a = TAU * k / 8.0, ox = std::cos(a), oy = std::sin(a);
                double cov = m.veg_cover.get(s.x + ox * 50.0, s.y + oy * 50.0); if (cov > worst) { worst = cov; vrx = -ox; vry = -oy; } }
            double cx = 0, cy = 0, n = 0, dog_best = 1e300, dgx = 0, dgy = 0;
            // El radio 500 solo existe para ver perros; la cohesión de ovejas usa
            // <=100. Sin perros en el modelo, reducir a 100 es BIT-IDÉNTICO (los
            // agentes extra que el barrido a 500 tocaba se descartan igual) y
            // recorta ~14x las celdas de esta consulta —la más caliente— (cr 9->2).
            double graze_r = m.dog_positions.empty() ? SHEEP_PERCEPTION_RADIUS : std::max(SHEEP_PERCEPTION_RADIUS, 500.0);
            m.grid.within(s.x, s.y, graze_r, [&](const Snap& sn, double d) {
                if (!sn.alive) return;
                // Cohesión y atracción al perro, solo dentro del propio cerco (granja, o
                // potrero si hay raster de potreros): una oveja no se agrupa a través de
                // un cerco. El zorro del que huye NO se filtra (los zorros cruzan
                // predios). Con n_farms<=1 -> bit-idéntico.
                bool same_farm = same_fence(m, s, sn.x, sn.y);
                if (sn.species == SHEEP && d <= SHEEP_PERCEPTION_RADIUS && same_farm) { cx += sn.x; cy += sn.y; n += 1; }
                if (sn.species == DOG && d < dog_best && same_farm) { dog_best = d; dgx = sn.x; dgy = sn.y; } });
            double vcx = 0, vcy = 0;
            // NO normalizar: en el Python get_flocking_vectors() devuelve el vector
            // COMPLETO hacia el centro del grupo, cuya magnitud es la distancia al
            // centro. Con peso 0.2 eso domina a los demas terminos (unitarios), y es
            // lo que mantiene la majada compacta. Normalizarlo la dispersaba, y una
            // majada dispersa deja de proteger: m_group cae, el cordero se separa de
            // la madre (m_mother deja de aplicar) y sube el bonus de aislamiento.
            if (n > 0) { vcx = cx / n - s.x; vcy = cy / n - s.y; }
            double vdx = 0, vdy = 0;
            if (dog_best < 500.0) { vdx = dgx - s.x; vdy = dgy - s.y; norm(vdx, vdy); }
            double dx = vfx * 0.3 + vrx * 0.2 + vcx * 0.2 + vdx * 0.3 + rng.normal(0.1);
            double dy = vfy * 0.3 + vry * 0.2 + vcy * 0.2 + vdy * 0.3 + rng.normal(0.1);
            norm(dx, dy);
            double sp = s.is_lamb ? LAMB_SPEED : SHEEP_SPEED;
            move_sheep(m, s, dx, dy, sp, rng, false);
        }
    }
    if (s.is_lamb && s.age_days > LAMB_MATURATION_DAYS) { s.is_lamb = false; s.vulnerability = SHEEP_ADULT_VULN; }
    double q = m.veg_quality.get(s.x, s.y);
    double gain = (s.is_lamb ? q : q * 2.0) - 1.0;
    s.energy = std::min(s.energy + gain - s.fear * 2.0, 100.0);
}

static double predation_probability(Model& m, int fox_i, int prey_i) {
    Animal& fox = m.agents[fox_i]; Animal& prey = m.agents[prey_i];
    double p_base = fox.predation_eff * prey.vulnerability;
    // Cobertura en la captura: +0,05 por defecto (Paper 1: emboscada). Yusti-Muñoz (2026)
    // encuentra MÁS depredación en suelo desnudo en estancias sin perro, es decir el signo
    // contrario; --cover-effect permite barrerlo (incluido negativo) sin tocar el default.
    double m_cover = m.params.cover_effect * m.veg_cover.get(fox.x, fox.y);
    double dnd = m.nearest_dog_dist(fox.x, fox.y, 500.0);
    double m_dog = (dnd >= 0) ? -DOG_PROTECTION_STRENGTH * (1.0 - dnd / 500.0) : 0.0;
    double nearby = 0;
    m.grid.within(prey.x, prey.y, 50.0, [&](const Snap& sn, double) { if (sn.alive && sn.species == SHEEP) nearby += 1; });
    double m_group = -0.03 * std::min(nearby / 10.0, 1.0);
    double m_condition = 0.03 * (1.0 - prey.energy / 100.0);
    double m_mother = 0.0;
    if (prey.is_lamb && prey.mother >= 0) { Animal& mo = m.agents[prey.mother];
        if (mo.alive) { double d = std::sqrt((prey.x - mo.x) * (prey.x - mo.x) + (prey.y - mo.y) * (prey.y - mo.y));
            if (d < 20.0) m_mother = -0.12 * (1.0 - d / 20.0); } }
    return clampd(p_base + m_cover + m_dog + m_group + m_condition + m_mother, FOX_P_MIN, FOX_P_MAX);
}

static void attempt_predation(Model& m, int fox_i, int prey_i, PRng& rng) {
    double p = predation_probability(m, fox_i, prey_i);
    m.predation_attempts++;
    if (m.agents[prey_i].species == SHEEP) m.att_sheep++; else if (m.agents[prey_i].species == HARE) m.att_hare++;
    m.trail.push_back({m.agents[prey_i].x, m.agents[prey_i].y, m.step_count});
    bool success = rng.unit() < p;
    if (success) { m.agents[prey_i].alive = false;
        feed_on(m.agents[fox_i], m.agents[prey_i]);   // A1: saciedad por biomasa
        if (m.agents[prey_i].species == SHEEP) { m.sheep_killed++;
            const int pf = m.agents[prey_i].farm; if (pf >= 0 && pf < (int)m.sheep_killed_farm.size()) m.sheep_killed_farm[pf]++; }
        else if (m.agents[prey_i].species == HARE) m.hares_killed++; }
    else { m.agents[prey_i].fear = 1.0; m.agents[fox_i].hunger = std::min(m.agents[fox_i].hunger + 0.05, 1.0); }
}

// ---- zorro ----
// Detección de presas del zorro (query espacial, READ-ONLY sobre el snapshot) —
// se puede precomputar en paralelo (Hito 4b) sin cambiar la semántica: el centro
// es la posición de inicio de tick (los zorros no se mueven en la fase A), y la
// selección/ataque (que leen estado vivo) siguen secuenciales.
static void fox_detect(const Model& m, double fx, double fy, std::vector<int>& prey, int& hares_nearby) {
    prey.clear(); hares_nearby = 0;
    int gx = (int)(fx / m.grid.csz); if (gx < 0) gx = 0; if (gx > m.grid.nx - 1) gx = m.grid.nx - 1;
    int gy = (int)(fy / m.grid.csz); if (gy < 0) gy = 0; if (gy > m.grid.ny - 1) gy = m.grid.ny - 1;
    int cr = (int)(FOX_DETECTION_RADIUS / m.grid.csz) + 1;
    const Grid& g = m.grid;
    for (int dy = -cr; dy <= cr; ++dy) for (int dx = -cr; dx <= cr; ++dx) {
        int ax = gx + dx, ay = gy + dy; if (ax < 0 || ay < 0 || ax >= g.nx || ay >= g.ny) continue;
        int c = ay * g.nx + ax, e = g.cellStart[c + 1];
        for (int p = g.cellStart[c]; p < e; ++p) {
            double ddx = g.spos[p].x - fx, ddy = g.spos[p].y - fy; if (std::sqrt(ddx * ddx + ddy * ddy) > FOX_DETECTION_RADIUS) continue;
            const SnapCold& cd = g.scold[p]; if (!cd.alive) continue;
            if (cd.species == SHEEP) prey.push_back(g.sid[p]);
            else if (cd.species == HARE) { hares_nearby++; prey.push_back(g.sid[p]); }
        }
    }
}

// Riesgo percibido por el zorro al cazar una presa en (tx,ty) — PORT FIEL de
// Fox.evaluate_risk (agents/fox.py). Nota de estructura: el término de
// PROXIMIDAD al perro usa la distancia ZORRO->perro (self en el Python), mientras
// que el PAISAJE DEL MIEDO y las ZONAS PELIGROSAS se evalúan en la PRESA. Se
// omiten cobertura vegetal (≈0 frente a la aversión 0.6, no dispara el aborto en
// el brazo sin perro donde ya hay paridad) e infraestructura (no existe en el C++).
static double fox_evaluate_risk(const Model& m, const Animal& f, double tx, double ty) {
    const double mult = f.is_chilla ? CHILLA_DOG_RISK_MULTIPLIER : 1.0; // dog_risk_multiplier
    double risk = 0.0;
    // (1) proximidad al perro más cercano dentro de 1500 m (distancia zorro->perro);
    //     (2) si no hay perro en rango pero existen perros, piso ambiental.
    double dnd = m.nearest_dog_dist(f.x, f.y, FOX_DOG_DETECTION_RADIUS);
    if (dnd >= 0.0)
        risk += (1.0 - dnd / FOX_DOG_DETECTION_RADIUS) * DOG_PROXIMITY_RISK_WEIGHT * mult;
    else if (!m.dog_positions.empty())
        risk += DOG_AMBIENT_RISK * mult;
    // (3) paisaje del miedo perro->zorro en la posición de la PRESA.
    risk += m.fox_perceived_risk(tx, ty) * DOG_FEAR_LANDSCAPE_WEIGHT * mult;
    // memoria propia de zonas peligrosas: distancia PRESA->zona (<500 -> +intensity*0.3).
    for (auto& z : f.danger_zones) {
        double ddx = tx - z.x, ddy = ty - z.y;
        if (std::sqrt(ddx * ddx + ddy * ddy) < 500.0) risk += z.intensity * 0.3;
    }
    // (Python evaluate_risk añade dos términos dog-independientes que aquí se
    //  omiten: cobertura vegetal (1-cover)*0.2 e infraestructura +0.3. La
    //  infraestructura no existe en el C++. La cobertura, al probarla, EMPEORÓ la
    //  paridad del efecto del perro: como el brazo con-perro ya opera cerca del
    //  umbral de aborto, un término dog-independiente lo cruza de forma
    //  desproporcionada y sobre-suprime. Se deja fuera; el gap residual apunta a
    //  divergencias del modelo de perro (p.ej. la disuasión del C++ resetea
    //  hunger=0/fear=1, que el Python no hace), no a este término.)
    return std::min(1.0, risk);
}

// Dirección (8 rumbos) de menor riesgo del paisaje del miedo alrededor de (px,py)
// a distancia search_r — PORT de FearLandscapeBase.get_safest_direction. Devuelve
// false si ninguna dirección mejora el riesgo actual (entonces no hay sesgo). Fuera
// del dominio el riesgo percibido es 1.0 (como get_perceived_risk del Python).
static bool fox_safest_direction(const Model& m, double px, double py, double search_r,
                                 double& bdx, double& bdy) {
    auto risk_at = [&](double x, double y) -> double {
        if (x < 0 || x >= m.params.width || y < 0 || y >= m.params.height) return 1.0;
        if (!m.is_land(x, y)) return 1.0;   // el mar, como fuera del dominio (sin máscara: nunca)
        return m.fox_perceived_risk(x, y);
    };
    double best_risk = risk_at(px, py); bdx = 0.0; bdy = 0.0; bool found = false;
    for (int k = 0; k < 8; ++k) {           // linspace(0, 2pi, 8, endpoint=False)
        double ang = TAU * (double)k / 8.0, dx = std::cos(ang), dy = std::sin(ang);
        double r = risk_at(px + dx * search_r, py + dy * search_r);
        if (r < best_risk) { best_risk = r; bdx = dx; bdy = dy; found = true; }
    }
    return found;
}

// Búsqueda activa de presas — PORT de Fox.search_for_prey. Rumbo al azar, salvo
// que haya una zona peligrosa a <300 m (huye de ella) y/o sesgo 60/40 hacia la
// dirección más segura del paisaje del miedo. Sólo se mueve si el destino es
// válido (dominio cerrado, como is_valid_position del Python — no clampea).
static void fox_search_for_prey(Model& m, Animal& f, PRng& rng) {
    double angle = rng.range(0.0, TAU);
    for (auto& z : f.danger_zones) {
        double dx = f.x - z.x, dy = f.y - z.y;
        if (std::sqrt(dx * dx + dy * dy) < 300.0) { angle = std::atan2(dy, dx); break; }
    }
    double sdx, sdy;
    if (fox_safest_direction(m, f.x, f.y, FOX_SPEED_WALK, sdx, sdy)) {
        double cdx = std::cos(angle), cdy = std::sin(angle);
        double mdx = 0.6 * sdx + 0.4 * cdx, mdy = 0.6 * sdy + 0.4 * cdy;
        angle = std::atan2(mdy, mdx);
    }
    double nx = f.x + std::cos(angle) * FOX_SPEED_WALK, ny = f.y + std::sin(angle) * FOX_SPEED_WALK;
    if (nx >= 0 && nx < m.params.width && ny >= 0 && ny < m.params.height && m.is_land(nx, ny)) { f.x = nx; f.y = ny; }
}

static void step_fox(Model& m, int i, PRng& rng, const std::vector<int>& prey, int hares_nearby) {
    Animal& f = m.agents[i];
    f.hunger = std::min(f.hunger + 0.01, 1.0);
    // olvido de zonas peligrosas vencidas (memoria de 168 h)
    uint64_t now = m.step_count;
    { auto& dz = f.danger_zones; dz.erase(std::remove_if(dz.begin(), dz.end(),
        [&](const DangerZone& z) { return now - z.step >= DANGER_TTL; }), dz.end()); }
    // fox_active: curva gaussiana, desplazada/aplanada si percibe un perro en su territorio
    bool perceives_dog = m.nearest_dog_dist(f.x, f.y, f.territory_radius) >= 0;
    double peak = perceives_dog ? FOX_ACT_PEAK_WD : FOX_ACT_PEAK_ND;
    double amp = perceives_dog ? FOX_ACT_AMP_WD : FOX_ACT_AMP_ND;
    double sigma = perceives_dog ? FOX_ACT_SIGMA_WD : FOX_ACT_SIGMA_ND;
    double base = perceives_dog ? FOX_ACT_BASE_WD : FOX_ACT_BASE_ND;
    double h = (double)m.current_hour; double raw = std::fabs(h - peak); double dd0 = std::min(raw, 24.0 - raw);
    double level = std::min(base + amp * std::exp(-(dd0 * dd0) / (2.0 * sigma * sigma)), 1.0);
    if (rng.unit() >= level) return; // descanso
    if (f.hunger < HUNGER_THRESHOLD) { double a = rng.range(0.0, TAU), dd = rng.range(50.0, 200.0);
        const double nx = clampd(f.x + std::cos(a) * dd, 0, m.params.width), ny = clampd(f.y + std::sin(a) * dd, 0, m.params.height);
        if (m.is_land(nx, ny)) { f.x = nx; f.y = ny; } return; }
    // --- hunt --- (reordenado 2026-09-11 para reproducir Fox.hunt del Python:
    // detect -> select_target -> evaluate_risk(target) -> aborto -> approach ->
    // attempt. El C++ evaluaba el riesgo ANTES de detectar presa y en la posición
    // del zorro, corriendo el aborto-y-huida en CADA paso activo aunque no hubiera
    // presa; eso expulsaba al zorro del rebaño mucho más agresivamente que el
    // Python y sobredisparaba el efecto del LGD. Ahora el aborto sólo ocurre con un
    // objetivo seleccionado, y sin presa se hace la búsqueda sesgada por el
    // paisaje del miedo (search_for_prey), no una huida dura.)
    // (b) ataque comprometido: si venía acechando y la presa está a tiro, ataca.
    // (El acecho es el mecanismo C++ de ventana de intercepción del perro; el
    //  Python no lo tiene. Se conserva.)
    { int tid = f.stalk_target; f.stalk_target = -1;
        if (tid >= 0 && m.agents[tid].alive) {
            double ddx = f.x - m.agents[tid].x, ddy = f.y - m.agents[tid].y;
            if (std::sqrt(ddx * ddx + ddy * ddy) < FOX_ATTACK_RADIUS) { attempt_predation(m, i, tid, rng); return; }
        }
    }
    // (c) detección de presas: precomputada en paralelo (fox_detect), pasada como
    // argumento. Sin presa -> búsqueda activa sesgada por el paisaje del miedo.
    if (prey.empty()) { fox_search_for_prey(m, f, rng); return; }
    // Seleccionar por score: vulnerabilidad + preferencia saturante (A2) +
    // bonus de cordero. s(n_h) satura en la disponibilidad local de liebres.
    const double s_switch = (hares_nearby > 0 && PREY_SWITCH_N_HALF > 0.0)
        ? (double)hares_nearby / ((double)hares_nearby + PREY_SWITCH_N_HALF)
        : 0.0;
    // Bonus por lejanía del perro: es el mismo para todas las presas de este
    // zorro, así que no altera el ranking, pero se incluye por fidelidad.
    const double dog_d = m.nearest_dog_dist(f.x, f.y, 1000.0);
    const double m_far_dog = (dog_d >= 0) ? std::min(dog_d / 500.0, 1.0) * FOX_FAR_FROM_DOG_BONUS
                                          : FOX_FAR_FROM_DOG_BONUS;
    // Índice de aislamiento O(p·k) en vez de O(p²): para listas grandes se agrupan
    // las presas de ESTE zorro en celdas de 50 m (hash local reutilizado por
    // thread) y cada candidata consulta su vecindad 3×3; para listas chicas el
    // escaneo directo es más barato que armar el índice. Los dos caminos cuentan
    // EXACTAMENTE el mismo conjunto (presas de la lista, vivas, a <50 m), así que
    // es bit-idéntico — sólo cambia cómo se encuentran. El conteo lee el estado
    // VIVO de las presas (cambia dentro de B2 al matar), por eso no se precomputa
    // en B1; el índice se arma aquí y el `alive` se consulta en vivo.
    const bool iso_indexed = prey.size() > ISO_DIRECT_MAX;
    static thread_local std::unordered_map<uint64_t, std::vector<int>> iso_cells;
    if (iso_indexed) {
        iso_cells.clear();
        for (int qid : prey) { const Animal& qa = m.agents[qid];
            uint64_t cx = (uint64_t)(int64_t)std::floor(qa.x / ISO_CELL);
            uint64_t cy = (uint64_t)(int64_t)std::floor(qa.y / ISO_CELL);
            iso_cells[(cx << 32) | cy].push_back(qid); }
    }
    int best = prey[0]; double best_score = -1e300;
    for (int pid : prey) { Animal& pa = m.agents[pid]; if (!pa.alive) continue;
        double score = pa.vulnerability;
        if (pa.species == HARE)       score += PREY_SWITCH_BETA_HARE  * s_switch;
        else if (pa.species == SHEEP) score -= PREY_SWITCH_BETA_SHEEP * s_switch;
        if (pa.is_lamb) score += 0.2;
        // Aislamiento: cuenta los CONGÉNERES detectados a menos de 50 m de la
        // presa candidata, entre los que este zorro tiene en su lista.
        int nearby = 0;
        if (iso_indexed) {
            int64_t pcx = (int64_t)std::floor(pa.x / ISO_CELL);
            int64_t pcy = (int64_t)std::floor(pa.y / ISO_CELL);
            for (int64_t ox = -1; ox <= 1; ++ox) for (int64_t oy = -1; oy <= 1; ++oy) {
                int64_t kx = pcx + ox, ky = pcy + oy; if (kx < 0 || ky < 0) continue;
                auto it = iso_cells.find(((uint64_t)kx << 32) | (uint64_t)ky);
                if (it == iso_cells.end()) continue;
                for (int qid : it->second) { if (qid == pid) continue; const Animal& qa = m.agents[qid];
                    if (!qa.alive) continue;
                    const double ddx = pa.x - qa.x, ddy = pa.y - qa.y;
                    if (ddx * ddx + ddy * ddy < 50.0 * 50.0) nearby++; }
            }
        } else {
            for (int qid : prey) { if (qid == pid) continue; const Animal& qa = m.agents[qid];
                if (!qa.alive) continue;
                const double ddx = pa.x - qa.x, ddy = pa.y - qa.y;
                if (ddx * ddx + ddy * ddy < 50.0 * 50.0) nearby++; }
        }
        score += (1.0 - std::min((double)nearby / 10.0, 1.0)) * FOX_ISOLATED_BONUS;
        score += m_far_dog;
        // Penalización por zona peligrosa recordada por este zorro (Python:
        // dist<300 -> score -= intensity*0.3; break tras la primera).
        for (const auto& z : f.danger_zones) {
            const double zdx = pa.x - z.x, zdy = pa.y - z.y;
            if (zdx * zdx + zdy * zdy < 300.0 * 300.0) { score -= z.intensity * 0.3; break; } }
        if (score > best_score) { best_score = score; best = pid; } }
    if (!m.agents[best].alive) return;
    double tx = m.agents[best].x, ty = m.agents[best].y; // posición VIVA de la presa
    // (d) evaluación de riesgo y aborto — PORT FIEL de Fox.hunt: se evalúa sobre
    // la PRESA seleccionada. Si el gate logístico dispara, recuerda la zona
    // (intensity=risk) y ejecuta avoid_and_search (huir del perro <1000 m, si no,
    // caminar al azar). Con params.n_dogs == 0 el riesgo del perro es cero.
    {
        const double risk = fox_evaluate_risk(m, f, tx, ty);
        const double abort_prob = 1.0 / (1.0 + std::exp(-RISK_ABORT_SHARPNESS * (risk - f.risk_aversion)));
        if (rng.unit() < abort_prob) {
            f.danger_zones.push_back({tx, ty, now, risk});   // add_dangerous_zone(target.pos, intensity=risk)
            if (f.danger_zones.size() > 64) f.danger_zones.erase(f.danger_zones.begin());
            double dgx, dgy;                                  // avoid_and_search()
            if (m.nearest_dog_pos(f.x, f.y, 1000.0, dgx, dgy)) {
                double ax = f.x - dgx, ay = f.y - dgy; norm(ax, ay);
                const double nx = clampd(f.x + ax * FOX_SPEED_WALK, 0, m.params.width), ny = clampd(f.y + ay * FOX_SPEED_WALK, 0, m.params.height);
                if (m.is_land(nx, ny)) { f.x = nx; f.y = ny; }
            } else {
                double a = rng.range(0.0, TAU);
                const double nx = clampd(f.x + std::cos(a) * FOX_SPEED_WALK, 0, m.params.width), ny = clampd(f.y + std::sin(a) * FOX_SPEED_WALK, 0, m.params.height);
                if (m.is_land(nx, ny)) { f.x = nx; f.y = ny; }
            }
            return;
        }
    }
    // (e) approach_prey + intento si queda a tiro.
    double to_x = tx - f.x, to_y = ty - f.y; double tolen = std::sqrt(to_x * to_x + to_y * to_y);
    double step = std::min(FOX_SPEED_WALK, tolen); double ux = to_x, uy = to_y; norm(ux, uy);
    { const double nx = clampd(f.x + ux * step, 0, m.params.width), ny = clampd(f.y + uy * step, 0, m.params.height);
      if (m.is_land(nx, ny)) { f.x = nx; f.y = ny; } }
    double adx = f.x - tx, ady = f.y - ty;
    if (std::sqrt(adx * adx + ady * ady) < FOX_ATTACK_RADIUS) {
        // Acecho/intercepción (mecanismo C++, NO presente en el Python del paper 1,
        // cuyo zorro mata de inmediato al llegar a <50 m). Si hay un perro en rango
        // de detección, el zorro NO mata este tick: queda en acecho expuesto y el
        // perro tendrá un turno para interceptarlo. Este canal extra hace que el
        // perro del C++ sobre-disuada respecto del Python (~35-40% más bajo el
        // con-perro). Conmutable con SIGRID_NO_STALK para el A/B de fidelidad.
        if (!FOX_NO_STALK && m.nearest_dog_dist(f.x, f.y, DOG_DETECTION_RADIUS) >= 0)
            f.stalk_target = best;
        else
            attempt_predation(m, i, best, rng);
    }
}

// ---- liebre (presa alternativa) ----
static void step_hare(Model& m, int i, PRng& rng) {
    Animal& h = m.agents[i];
    h.age_days += 1.0 / 24.0;
    h.days_since_repro += 1.0 / 24.0;
    h.pending_offspring = 0;
    if (!h.mature && h.age_days * 24.0 >= HARE_MATURITY_AGE_H) { h.mature = true; h.vulnerability = HARE_VULN_MATURE; }
    h.fear = std::max(h.fear - 0.15, 0.0);
    double pbest = 1e300, ppx = 0, ppy = 0; bool found = false;
    m.grid.within(h.x, h.y, HARE_PERCEPTION_RADIUS, [&](const Snap& sn, double d) {
        if (sn.alive && sn.species == FOX && d < pbest) { pbest = d; ppx = sn.x; ppy = sn.y; found = true; } });
    if (found || h.fear > 0.5) {
        double dx, dy;
        if (found) { dx = h.x - ppx; dy = h.y - ppy; norm(dx, dy); }
        else { double a = rng.range(0.0, TAU); dx = std::cos(a); dy = std::sin(a); }
        const double nx = clampd(h.x + dx * HARE_SPEED_FLEE, 0, m.params.width), ny = clampd(h.y + dy * HARE_SPEED_FLEE, 0, m.params.height);
        if (m.is_land(nx, ny)) { h.x = nx; h.y = ny; }   // no entra al mar (sin máscara: siempre)
    } else {
        double dx, dy; food_direction(m.veg_quality, h.x, h.y, dx, dy);
        const double nx = clampd(h.x + dx * HARE_SPEED_NORMAL, 0, m.params.width), ny = clampd(h.y + dy * HARE_SPEED_NORMAL, 0, m.params.height);
        if (m.is_land(nx, ny)) { h.x = nx; h.y = ny; }
        // forrajea: gana energía según la calidad del pastizal donde queda
        h.energy = std::min(100.0, h.energy + m.veg_quality.get(h.x, h.y) * HARE_FORAGE_GAIN);
    }
    // desgaste y muerte por inanición
    h.energy -= HARE_UPKEEP + h.fear * 1.5;
    if (h.energy <= 0.0) { h.alive = false; return; }

    // ¿intenta reproducirse? Densidad local sobre la INSTANTÁNEA, que es la
    // convención de consulta espacial de este port (el Python recorre posiciones
    // vivas; la diferencia es la misma que ya existe en el resto del modelo).
    if (h.energy > HARE_REPRO_ENERGY_MIN && h.days_since_repro > HARE_REPRO_DAYS_MIN
        && h.fear < HARE_REPRO_FEAR_MAX) {
        int local = 0;
        m.grid.within(h.x, h.y, HARE_LOCAL_RADIUS, [&](const Snap& sn, double) {
            if (sn.alive && sn.species == HARE) local++; });
        if (local > 0) local--;   // no contarse a sí misma
        const double local_area_ha = 3.14159265358979323846 * HARE_LOCAL_RADIUS * HARE_LOCAL_RADIUS / 10000.0;
        const double local_density = (double)local / std::max(local_area_ha, 0.1);
        if (local_density < HARE_CARRYING_CAPACITY && rng.unit() < HARE_REPRODUCTION_RATE) {
            const int n_off = 2 + (int)(rng.unit() * 3.0);        // 2..4
            h.pending_offspring = n_off;
            for (int k = 0; k < n_off; ++k) {
                const double a = rng.range(0.0, TAU), d = rng.range(10.0, 50.0);
                h.off_x[k] = clampd(h.x + std::cos(a) * d, 0, m.params.width);
                h.off_y[k] = clampd(h.y + std::sin(a) * d, 0, m.params.height);
                if (!m.is_land(h.off_x[k], h.off_y[k])) { h.off_x[k] = h.x; h.off_y[k] = h.y; }   // cría en el mar -> junto a la madre
            }
            h.days_since_repro = 0.0;
            h.energy -= HARE_REPRO_COST;
        }
    }
}

// ---- dinámica poblacional de la liebre: fase C (secuencial, determinista) ----
// Materializa los nacimientos decididos en la fase A y aplica la mortalidad
// denso-dependiente. Va DESPUÉS de la fase B, recorriendo índices en orden, de
// modo que el resultado es independiente del número de hilos.

// Nacimientos. Devuelve cuántas crías se crearon. Los índices nuevos se
// agregan a `phaseA` para que caminen desde el tick siguiente.
static int hare_births(Model& m, std::vector<int>& phaseA) {
    int born = 0;
    const size_t n0 = m.agents.size();          // no iterar sobre las recién nacidas
    for (size_t i = 0; i < n0; ++i) {
        Animal& h = m.agents[i];
        if (!h.alive || h.species != HARE || h.pending_offspring <= 0) continue;
        for (int k = 0; k < h.pending_offspring; ++k) {
            Animal a{};
            a.species = HARE; a.alive = true; a.mother = -1;
            a.x = h.off_x[k]; a.y = h.off_y[k];
            a.energy = 80.0; a.age_days = 0.0; a.fear = 0.0;
            a.mature = false; a.vulnerability = HARE_VULN_JUV;
            a.days_since_repro = 0.0;
            m.agents.push_back(a);
            phaseA.push_back((int)m.agents.size() - 1);
            born++;
        }
        m.agents[i].pending_offspring = 0;
    }
    return born;
}

// Mortalidad denso-dependiente: si la densidad supera la capacidad de carga,
// mueren las de MENOR energía. El desempate es por índice, así que el conjunto
// podado es único para un estado dado.
static int hare_cull(Model& m) {
    // capacidad de carga sobre el área de TIERRA si hay máscara (si no, el dominio)
    const double area_ha = m.land_mask.empty() ? (m.params.width * m.params.height) / 10000.0 : m.land_area_ha;
    std::vector<int> live;
    for (size_t i = 0; i < m.agents.size(); ++i)
        if (m.agents[i].alive && m.agents[i].species == HARE) live.push_back((int)i);
    const double density = (double)live.size() / area_ha;
    if (density <= HARE_CARRYING_CAPACITY) return 0;
    const int n_remove = (int)((density - HARE_CARRYING_CAPACITY) * area_ha);
    if (n_remove <= 0) return 0;
    std::stable_sort(live.begin(), live.end(), [&](int a, int b) {
        if (m.agents[a].energy != m.agents[b].energy) return m.agents[a].energy < m.agents[b].energy;
        return a < b;                     // desempate determinista
    });
    const int k = std::min(n_remove, (int)live.size());
    for (int j = 0; j < k; ++j) m.agents[live[j]].alive = false;
    return k;
}

// Cacería humana: parámetros (placeholder, a calibrar con datos de Diego y literatura).
static const uint64_t HUNT_SALT = 0xF000000000000000ULL;  // distinto de ids (pequeños) y del 0xFFFF... del barajado
static const double HUNT_RADIUS         = 1000.0;  // radio de disparo + miedo (m)
static const double HUNT_KILL_PROB      = 0.15;    // prob. de abatir un zorro dentro del radio
static const double HUNT_FEAR_INTENSITY = 1.0;     // intensidad del hotspot de miedo que deja

// Evento estocástico de cacería, anclado al casco de cada granja. Una oportunidad
// al inicio de cada día: con prob. hunt_prob la granja sale a cazar; cada zorro en
// HUNT_RADIUS muere con prob. HUNT_KILL_PROB (disparo) y, si sobrevive, recuerda el
// sitio como zona peligrosa (hotspot de miedo que luego evita). Reusa la memoria de
// danger_zones de los zorros. Con hunt_prob<=0 es inerte -> bit-idéntico. Secuencial,
// agentes en orden de índice; RNG determinista (seed, step, salt+granja).
// Régimen B (Ana 2026-11-03/05; Fotos_Personas.xlsx: 2/59 detecciones en domingo,
// todas entre 7 y 17 h). Cada estancia, cada día salvo el domingo (día 0 = lunes,
// SUPUESTO), a una hora al azar entre 8 y 16 h: el ovejero va a un potrero con
// ovejas (al azar entre los ocupados de su estancia) y se ubica junto a una oveja al
// azar de ese potrero. Cada zorro dentro de hunt_radius muere con prob. hunt_p_culpeo
// (culpeo) o hunt_p_culpeo*hunt_chilla_ratio (chilla); si sobrevive, memoriza el
// sitio como zona peligrosa. RNG propio por (seed, día, granja) -> no altera el de
// los agentes. Inerte si hunt_mode != 1.
static const uint64_t HUNT_SALT_B = 0xE000000000000000ULL;
static long hunt_events_b(Model& m, uint64_t step, uint64_t seed) {
    const Params& P = m.params;
    if (P.hunt_mode != 1 || P.hunt_p_culpeo <= 0.0) return 0;
    const uint64_t day = step / 24; const int hour = (int)(step % 24);
    if (day % 7 == 6) return 0;                                   // domingo
    long shot = 0; const uint64_t now = m.step_count;
    for (int f = 0; f < m.n_farms; ++f) {
        PRng r(agent_seed(seed, day * 24, HUNT_SALT_B + (uint64_t)f));
        const int h = 8 + (int)(r.unit() * 9.0);                  // 8..16 h
        if (hour != h) continue;
        if (!m.hunt_farm_mask.empty() && !m.hunt_farm_mask[f]) continue;   // esta estancia no caza
        // régimen A: sale solo con prob. hunt_outing_prob (el sorteo existe solo si < 1,
        // así el régimen B por defecto conserva su secuencia de números aleatorios)
        if (P.hunt_outing_prob < 1.0 && r.unit() >= P.hunt_outing_prob) continue;
        // ovejas vivas de la estancia, agrupadas por unidad (potrero si hay; si no, la estancia)
        std::vector<std::vector<int>> by_unit(m.has_paddocks() ? m.n_units : 1);
        for (size_t k = 0; k < m.agents.size(); ++k) { const Animal& a = m.agents[k];
            if (a.alive && a.species == SHEEP && (m.n_farms <= 1 || a.farm == f)) by_unit[m.has_paddocks() ? a.unit : 0].push_back((int)k); }
        std::vector<int> occ; for (int u = 0; u < (int)by_unit.size(); ++u) if (!by_unit[u].empty()) occ.push_back(u);
        if (occ.empty()) continue;
        const std::vector<int>& flock = by_unit[occ[(size_t)(r.unit() * occ.size())]];
        const Animal& ref = m.agents[flock[(size_t)(r.unit() * flock.size())]];
        const double hx = ref.x, hy = ref.y; m.hunt_outings++;
        if (f < (int)m.outings_farm.size()) m.outings_farm[f]++;
        for (auto& a : m.agents) {
            if (!a.alive || a.species != FOX) continue;
            const double dx = a.x - hx, dy = a.y - hy;
            if (dx * dx + dy * dy > P.hunt_radius * P.hunt_radius) continue;
            const double p = a.is_chilla ? P.hunt_p_culpeo * P.hunt_chilla_ratio : P.hunt_p_culpeo;
            if (r.unit() < p) { a.alive = false; shot++; (a.is_chilla ? m.chillas_hunted : m.culpeos_hunted)++;
                if (f < (int)m.culpeos_hunted_farm.size()) (a.is_chilla ? m.chillas_hunted_farm : m.culpeos_hunted_farm)[f]++; }
            else {
                a.danger_zones.push_back({hx, hy, now, HUNT_FEAR_INTENSITY});
                if (a.danger_zones.size() > 64) a.danger_zones.erase(a.danger_zones.begin());
            }
        }
    }
    m.foxes_hunted += shot;
    return shot;
}

static long hunt_events(Model& m, uint64_t step, uint64_t seed) {
    if (m.params.hunt_mode == 1) return hunt_events_b(m, step, seed);
    if (m.params.hunt_prob <= 0.0 || step % 24 != 0) return 0;
    long shot = 0; const uint64_t now = m.step_count;
    for (int f = 0; f < m.n_farms; ++f) {
        PRng r(agent_seed(seed, step, HUNT_SALT + (uint64_t)f));
        if (r.unit() >= m.params.hunt_prob) continue;
        // ancla: una casa real de la granja (elegida al azar) si hay; si no, el centroide.
        double hx, hy;
        if (f < (int)m.farm_homesteads.size() && !m.farm_homesteads[f].empty()) {
            int c = (int)(r.unit() * (double)m.farm_homesteads[f].size());
            hx = m.farm_homesteads[f][c].first; hy = m.farm_homesteads[f][c].second;
        } else { hx = m.farm_casco[f].first; hy = m.farm_casco[f].second; }
        for (auto& a : m.agents) {
            if (!a.alive || a.species != FOX) continue;
            const double dx = a.x - hx, dy = a.y - hy;
            if (dx * dx + dy * dy > HUNT_RADIUS * HUNT_RADIUS) continue;
            if (r.unit() < HUNT_KILL_PROB) { a.alive = false; shot++; }
            else {
                a.danger_zones.push_back({hx, hy, now, HUNT_FEAR_INTENSITY});
                if (a.danger_zones.size() > 64) a.danger_zones.erase(a.danger_zones.begin());
            }
        }
    }
    m.foxes_hunted += shot;
    return shot;
}

// ---- perro guardián (determinista) ----
static void step_dog(Model& m, int i) {
    Animal& dog = m.agents[i];
    // centroide del rebaño (posiciones vivas). Se calcula al inicio (solo lee) para
    // que la correa (dog_leash) pueda acotar TODOS los movimientos del perro.
    double cx = 0, cy = 0, n = 0;
    // El rebaño del perro: las ovejas de su cerco (granja; potrero si hay potreros,
    // porque el perro tampoco cruza el cerco interno y no puede arrear lo que no alcanza).
    // Con dog_roam el perro sigue al grupo más grande de su estancia: el potrero con más
    // ovejas vivas (Ana 2026-11-03: "tiende a estar con el grupo más grande, pero se
    // mueve todo el tiempo"). Se recalcula cada paso; empate -> menor id de unidad.
    int flock_unit = dog.unit;
    if (m.has_paddocks() && m.params.dog_roam) {
        std::vector<int> cnt(m.n_units, 0);
        for (auto& a : m.agents) if (a.alive && a.species == SHEEP && a.farm == dog.farm) cnt[a.unit]++;
        int best = -1; for (int u = 0; u < m.n_units; ++u) if (cnt[u] > 0 && (best < 0 || cnt[u] > cnt[best])) best = u;
        if (best >= 0) flock_unit = best;
    }
    auto my_flock = [&](const Animal& a) { return m.has_paddocks() ? (a.unit == flock_unit) : (m.n_farms <= 1 || a.farm == dog.farm); };
    for (auto& a : m.agents) if (a.alive && a.species == SHEEP && my_flock(a)) { cx += a.x; cy += a.y; n += 1; }
    const double flx = n > 0 ? cx / n : dog.patrol_cx, fly = n > 0 ? cy / n : dog.patrol_cy;
    // Correa: el perro no se aleja más de dog_leash del centro de su rebaño (radio del
    // ámbito de hogar de collares GPS: 475 ha -> 1230 m; IC 271-680 ha -> 929-1471 m).
    // Si el destino sale del círculo: estando dentro, el perro avanza solo hasta el
    // borde por su misma trayectoria; estando fuera (el rebaño cambió de potrero),
    // vuelve hacia el centro con el largo de paso que traía. Nunca avanza más que su
    // paso (sin saltos). Con dog_leash<=0 es move_fenced.
    auto mv = [&](double nx, double ny) {
        const double L = m.params.dog_leash;
        if (L > 0.0 && n > 0) {
            const double ex = nx - flx, ey = ny - fly;
            if (ex * ex + ey * ey > L * L) {
                const double ox = dog.x - flx, oy = dog.y - fly, od = std::sqrt(ox * ox + oy * oy);
                const double sx = nx - dog.x, sy = ny - dog.y, s = std::sqrt(sx * sx + sy * sy);
                if (od <= L) {   // cruce del segmento perro->destino con el círculo: |o + t*s| = L, t en [0,1]
                    const double a = sx * sx + sy * sy, b = 2.0 * (ox * sx + oy * sy), c = ox * ox + oy * oy - L * L;
                    const double t = a > 0 ? (-b + std::sqrt(std::max(0.0, b * b - 4.0 * a * c))) / (2.0 * a) : 0.0;
                    nx = dog.x + sx * t; ny = dog.y + sy * t;
                } else {         // fuera: volver hacia el centro del rebaño sin pasarse
                    const double st = std::min(s, od);
                    nx = dog.x - ox / od * st; ny = dog.y - oy / od * st;
                }
            }
        }
        move_fenced(m, dog, nx, ny);
        if (n > 0) { const double ex = dog.x - flx, ey = dog.y - fly; m.dog_max_flock_dist = std::max(m.dog_max_flock_dist, std::sqrt(ex * ex + ey * ey)); }
    };
    // detectar zorro más cercano en DOG_DETECTION_RADIUS (snapshot)
    double best = 1e300, bx = 0, by = 0; int best_id = -1;
    {
        int gx = (int)(dog.x / m.grid.csz); if (gx < 0) gx = 0; if (gx > m.grid.nx - 1) gx = m.grid.nx - 1;
        int gy = (int)(dog.y / m.grid.csz); if (gy < 0) gy = 0; if (gy > m.grid.ny - 1) gy = m.grid.ny - 1;
        int cr = (int)(DOG_DETECTION_RADIUS / m.grid.csz) + 1;
        const Grid& g = m.grid;
        for (int ddy = -cr; ddy <= cr; ++ddy) for (int ddx = -cr; ddx <= cr; ++ddx) {
            int ax = gx + ddx, ay = gy + ddy; if (ax < 0 || ay < 0 || ax >= g.nx || ay >= g.ny) continue;
            int c = ay * g.nx + ax, e = g.cellStart[c + 1];
            for (int p = g.cellStart[c]; p < e; ++p) {
                const SnapCold& cd = g.scold[p]; if (!cd.alive || cd.species != FOX) continue;
                double dx = g.spos[p].x - dog.x, dy = g.spos[p].y - dog.y, dist = std::sqrt(dx * dx + dy * dy);
                if (dist <= DOG_DETECTION_RADIUS && dist < best) { best = dist; bx = g.spos[p].x; by = g.spos[p].y; best_id = g.sid[p]; }
            }
        }
    }
    if (best_id >= 0) { m.n_chase++;
        double to_x = bx - dog.x, to_y = by - dog.y, tolen = std::sqrt(to_x * to_x + to_y * to_y);
        double step = std::min(DOG_SPEED_CHASE, tolen), ux = to_x, uy = to_y; norm(ux, uy);
        mv(dog.x + ux * step, dog.y + uy * step);
        double cdx = dog.x - bx, cdy = dog.y - by;
        if (best < DOG_CHASE_RADIUS && std::sqrt(cdx * cdx + cdy * cdy) < DOG_DETER_RADIUS) {
            // disuasión multi-objetivo: el objetivo + otros zorros dentro de
            // DOG_CHASE_RADIUS de la posición nueva del perro (snapshot)
            std::vector<std::pair<int, std::pair<double, double>>> deterred;
            deterred.push_back({best_id, {bx, by}});
            int gx = (int)(dog.x / m.grid.csz); if (gx < 0) gx = 0; if (gx > m.grid.nx - 1) gx = m.grid.nx - 1;
            int gy = (int)(dog.y / m.grid.csz); if (gy < 0) gy = 0; if (gy > m.grid.ny - 1) gy = m.grid.ny - 1;
            int cr = (int)(DOG_CHASE_RADIUS / m.grid.csz) + 1;
            const Grid& g = m.grid;
            for (int ddy = -cr; ddy <= cr; ++ddy) for (int ddx = -cr; ddx <= cr; ++ddx) {
                int ax = gx + ddx, ay = gy + ddy; if (ax < 0 || ay < 0 || ax >= g.nx || ay >= g.ny) continue;
                int c = ay * g.nx + ax, e = g.cellStart[c + 1];
                for (int p = g.cellStart[c]; p < e; ++p) {
                    const SnapCold& cd = g.scold[p]; int j = g.sid[p]; if (!cd.alive || cd.species != FOX || j == best_id) continue;
                    double dx = g.spos[p].x - dog.x, dy = g.spos[p].y - dog.y; if (std::sqrt(dx * dx + dy * dy) <= DOG_CHASE_RADIUS) deterred.push_back({j, {g.spos[p].x, g.spos[p].y}});
                }
            }
            uint64_t now = m.step_count;
            for (auto& pr : deterred) { Animal& fox = m.agents[pr.first]; if (!fox.alive) continue;
                // El Python (dog.chase) al disuadir SÓLO agrega una zona peligrosa
                // al zorro; NO le resetea hunger/fear. El C++ además hacía
                // fear=1/hunger=0, que sidelina al zorro ~30 h (hunger<0.3 -> no
                // caza) — un canal de sobre-supresión ausente en el Python.
                // Conmutable con SIGRID_FAITHFUL_DETER para el A/B de fidelidad.
                if (!DOG_FAITHFUL_DETER) { fox.fear = 1.0; fox.hunger = 0.0; }
                fox.stalk_target = -1;
                fox.danger_zones.push_back({pr.second.first, pr.second.second, now});
                if (fox.danger_zones.size() > 64) fox.danger_zones.erase(fox.danger_zones.begin()); }
        }
        return;
    }
    // (2) rastro olfativo: evento de depredación reciente dentro del radio.
    {
        const uint64_t now = m.step_count;
        double tx = 0, ty = 0; long best_step = -1;
        for (size_t k = m.trail.size(); k-- > 0; ) {
            const auto& ev = m.trail[k];
            if (now - ev.step > DOG_TRAIL_TTL_HOURS) break;   // lista ordenada por tiempo
            const double dx = ev.x - dog.x, dy = ev.y - dog.y;
            if (std::sqrt(dx * dx + dy * dy) <= DOG_TRAIL_RADIUS && (long)ev.step > best_step) {
                tx = ev.x; ty = ev.y; best_step = (long)ev.step; }
        }
        if (best_step >= 0) { m.n_trail++;   // investigar: se acerca a velocidad de patrulla
            double dx = tx - dog.x, dy = ty - dog.y, len = std::sqrt(dx * dx + dy * dy);
            const double stp = std::min(DOG_SPEED_PATROL, len); norm(dx, dy);
            mv(dog.x + dx * stp, dog.y + dy * stp);
            return;
        }
    }
    // (3) arreo: si hay ovejas lejos del centro, el perro se pone POR DETRÁS de
    // la más alejada (a 50 m en la dirección opuesta al rebaño) para empujarla.
    if (n > 1) {
        int worst = -1; double worst_d = DOG_SCATTERED_THRESHOLD;
        for (size_t k = 0; k < m.agents.size(); ++k) { const Animal& a = m.agents[k];
            if (!a.alive || a.species != SHEEP || !my_flock(a)) continue;
            const double dx = a.x - flx, dy = a.y - fly, d = std::sqrt(dx * dx + dy * dy);
            if (d > worst_d) { worst_d = d; worst = (int)k; } }
        if (worst >= 0) { m.n_herd++;
            const Animal& t = m.agents[worst];
            double dx = t.x - flx, dy = t.y - fly; const double dist = std::sqrt(dx * dx + dy * dy);
            if (dist > 0) {
                const double bx = t.x + (dx / dist) * 50.0, by = t.y + (dy / dist) * 50.0;
                double mx = bx - dog.x, my = by - dog.y; const double len = std::sqrt(mx * mx + my * my);
                const double stp = std::min(DOG_SPEED_PATROL, len); norm(mx, my);
                mv(dog.x + mx * stp, dog.y + my * stp);
                return;
            }
        }
    }
    dog.patrol_angle += 0.1;
    double tx = flx + std::cos(dog.patrol_angle) * DOG_PATROL_RADIUS, ty = fly + std::sin(dog.patrol_angle) * DOG_PATROL_RADIUS;
    double dx = tx - dog.x, dy = ty - dog.y; norm(dx, dy);
    mv(dog.x + dx * DOG_SPEED_PATROL, dog.y + dy * DOG_SPEED_PATROL);
}

// ---- construcción de la población (idéntica en serial/OMP y BSPonMPI) ----
// El orden de creación (adultos, corderos, zorros, chillas, liebres, perros) y el
// consumo del RNG de build (mt19937) definen los agentes y su índice global (gid)
// = orden de inserción. Ese gid siembra el RNG por-agente (seed,tick,gid) y ordena
// el snapshot, así que replicarlo exacto es condición de la bit-identidad.
static void build_model(const Params& params, uint64_t seed, Model& m,
                        std::vector<int>& phaseA, std::vector<int>& phaseB, std::vector<int>& foxidx) {
    Rng rng_build(seed);
    double area_ha = params.width * params.height / 10000.0;
    double area_km2 = params.width * params.height / 1000000.0;
    int n_sheep = std::max(1L, std::lround(params.sheep_density * area_ha));
    int n_lambs = (int)(n_sheep * params.lamb_proportion);
    int n_adults = n_sheep - n_lambs;
    // Con máscara de tierra, las densidades de zorros y liebres se refieren al área de
    // TIERRA (no al rectángulo con mar). Sin máscara, el área del dominio (como antes).
    const double wild_km2 = m.land_mask.empty() ? area_km2 : m.land_area_ha / 100.0;
    const double wild_ha  = m.land_mask.empty() ? area_ha  : m.land_area_ha;
    // Posición inicial de zorros y liebres: uniforme en el dominio; con máscara se
    // vuelve a sortear hasta caer en tierra (consume más RNG solo si hay máscara).
    auto place_wild = [&](Animal& a) {
        int tries = 0;
        do { a.x = rng_build.range(0, params.width); a.y = rng_build.range(0, params.height); }
        while (!m.is_land(a.x, a.y) && ++tries < 100000);
    };
    int n_foxes = (int)std::lround(params.fox_density * wild_km2);

    m.params = params; m.n_sheep_initial = n_sheep;
    build_rasters(params.width, params.height, rng_build, m.veg_quality, m.veg_cover);
    // Paisaje del miedo del zorro: mismas dimensiones/height que los rasters de
    // vegetación (misma grilla CELL_SIZE), inicializado en cero.
    m.fox_fear = {m.veg_cover.cols, m.veg_cover.rows, m.veg_cover.height,
                  std::vector<double>((size_t)m.veg_cover.cols * m.veg_cover.rows, 0.0)};

    // --- granjas: fijar n_farms, áreas y cascos ANTES de crear la población, para
    // repartir los rebaños por estancia. Con n_farms<=1 no se usa en la creación
    // (se mantiene el PATH ORIGINAL agrupado en el centro -> bit-idéntico; el cálculo
    // de abajo no consume el RNG de build). ---
    if (m.farm_raster.empty())
        m.n_farms = std::max(1, params.farms_x * params.farms_y);
    std::vector<long> farm_cells(m.n_farms, 0);
    m.farm_casco.assign(m.n_farms, {params.width / 2.0, params.height / 2.0});
    {
        std::vector<double> sx(m.n_farms, 0), sy(m.n_farms, 0);
        int cols = std::max(1, (int)(params.width / 30.0)), rows = std::max(1, (int)(params.height / 30.0));
        for (int r = 0; r < rows; ++r) for (int c = 0; c < cols; ++c) {
            double x = (c + 0.5) * 30.0, y = (r + 0.5) * 30.0; int f = m.farm_of(x, y);
            if (f >= 0 && f < m.n_farms) { sx[f] += x; sy[f] += y; farm_cells[f]++; }
        }
        for (int f = 0; f < m.n_farms; ++f) if (farm_cells[f] > 0) m.farm_casco[f] = {sx[f] / farm_cells[f], sy[f] / farm_cells[f]};
    }
    double fcx = params.width / 2.0, fcy = params.height / 2.0;
    // --- grupos del reparto inicial del rebaño: POTREROS si hay raster de potreros,
    // granjas si no. Pesos y centros: con potreros, el área real y el punto interior
    // de .units (o celdas y centroide si no se cargó .units); sin potreros, celdas y
    // casco de la granja (camino original, misma aritmética -> bit-idéntico). Es
    // geométrico, no consume RNG. Con n_farms<=1 group_of_k devuelve 0. ---
    const bool grouped = m.n_farms > 1;
    const int n_groups = m.has_paddocks() ? m.n_units : m.n_farms;
    std::vector<double> gw(n_groups, 0.0);
    std::vector<std::pair<double,double>> gc(n_groups, {fcx, fcy});
    if (m.has_paddocks()) { gw = m.unit_weight; gc = m.unit_center; }
    else for (int f = 0; f < m.n_farms; ++f) { gw[f] = (double)farm_cells[f]; gc[f] = m.farm_casco[f]; }
    // Estación simulada: el rebaño parte SOLO en las unidades de esa estación. Cada
    // estancia conserva su total (la carga es de la estancia, no de la mitad usada):
    // los pesos activos se reescalan para sumar el área de la estancia. Inerte si
    // season_sim < 0 o no se cargó .season.
    const bool season_on = m.has_paddocks() && params.season_sim >= 0 && !m.unit_season.empty();
    if (season_on) {
        for (int f = 0; f < m.n_farms; ++f) {
            double tot = 0, act = 0;
            for (int u = 0; u < m.n_units; ++u) if (m.unit_farm[u] == f) { tot += gw[u]; if (m.unit_season[u] == params.season_sim) act += gw[u]; }
            for (int u = 0; u < m.n_units; ++u) if (m.unit_farm[u] == f)
                gw[u] = (m.unit_season[u] == params.season_sim && act > 0) ? gw[u] * tot / act : 0.0;
        }
    }
    auto group_of_k = [&](int k, int n) -> int {
        if (!grouped) return 0;
        double T = 0; for (double w : gw) T += w; if (T <= 0) return 0;
        double target = ((double)k + 0.5) / (double)n * T, acc = 0;
        for (int g = 0; g < n_groups; ++g) { acc += gw[g]; if (target < acc) return g; }
        return n_groups - 1;
    };
    // Grupo del perro de la granja f: su potrero de mayor peso (si hay potreros) o la granja.
    auto dog_group = [&](int f) -> int {
        if (!m.has_paddocks()) return f;
        int best = -1; for (int u = 0; u < m.n_units; ++u) if (m.unit_farm[u] == f && (best < 0 || gw[u] > gw[best])) best = u;
        return best < 0 ? 0 : best;
    };

    std::vector<int> adult_ids;                           // (n_farms==1)
    std::vector<std::vector<int>> adult_ids_f;            // (n_farms>1) por grupo
    if (grouped) adult_ids_f.resize(n_groups);
    for (int k = 0; k < n_adults; ++k) {
        int f = group_of_k(k, n_adults);
        double ax = grouped ? gc[f].first  : fcx;
        double ay = grouped ? gc[f].second : fcy;
        Animal a{}; a.species = SHEEP; a.alive = true; a.energy = 100; a.mother = -1;
        a.x = clampd(ax + rng_build.range(-300, 300), 0, params.width); a.y = clampd(ay + rng_build.range(-300, 300), 0, params.height);
        // con estación: si la dispersión de ±300 m la saca a una unidad de la otra
        // estación, parte en el centro de su grupo (no consume RNG; inerte sin estación).
        if (season_on && m.unit_season[m.unit_of(a.x, a.y)] != params.season_sim) { a.x = ax; a.y = ay; }
        a.age_days = rng_build.range(365, 2000); a.vulnerability = SHEEP_ADULT_VULN;
        int gid = (int)m.agents.size();
        if (grouped) adult_ids_f[f].push_back(gid); else adult_ids.push_back(gid);
        m.agents.push_back(a);
    }
    for (int k = 0; k < n_lambs; ++k) {
        int f = group_of_k(k, n_lambs);
        std::vector<int>& ids = grouped ? adult_ids_f[f] : adult_ids;
        int mother = ids.empty() ? -1 : ids[(int)rng_build.range(0, (double)ids.size())];
        double bx = grouped ? gc[f].first : fcx;
        double by = grouped ? gc[f].second : fcy;
        if (mother >= 0) { bx = m.agents[mother].x; by = m.agents[mother].y; }
        Animal a{}; a.species = SHEEP; a.alive = true; a.is_lamb = true; a.energy = 70; a.mother = mother;
        a.x = clampd(bx + rng_build.range(-20, 20), 0, params.width); a.y = clampd(by + rng_build.range(-20, 20), 0, params.height);
        if (season_on && m.unit_season[m.unit_of(a.x, a.y)] != params.season_sim) { a.x = bx; a.y = by; }
        a.age_days = rng_build.range(0, 30); a.vulnerability = LAMB_VULN;
        m.agents.push_back(a);
    }
    for (int k = 0; k < n_foxes; ++k) {
        Animal a{}; a.species = FOX; a.alive = true; a.energy = 100; a.mother = -1;
        place_wild(a);
        a.hunger = rng_build.range(0.3, 0.7); a.risk_aversion = BASE_RISK_AVERSION + rng_build.range(-0.1, 0.1);
        a.predation_eff = params.fox_predation_effectiveness; a.territory_radius = FOX_TERRITORY_RADIUS;
        m.agents.push_back(a);
    }
    int n_chillas = (int)std::lround(params.chilla_density * wild_km2);
    for (int k = 0; k < n_chillas; ++k) {
        Animal a{}; a.species = FOX; a.is_chilla = true; a.alive = true; a.energy = 100; a.mother = -1;
        place_wild(a);
        a.hunger = rng_build.range(0.3, 0.7); a.risk_aversion = (BASE_RISK_AVERSION + rng_build.range(-0.1, 0.1)) * 0.7;
        a.predation_eff = params.fox_predation_effectiveness; a.territory_radius = CHILLA_TERRITORY_RADIUS;
        m.agents.push_back(a);
    }
    int n_hares = (int)std::lround(params.hare_density * wild_ha);
    for (int k = 0; k < n_hares; ++k) {
        Animal a{}; a.species = HARE; a.alive = true; a.mother = -1;
        place_wild(a);
        a.energy = rng_build.range(60, 100); a.age_days = rng_build.range(0, 365);
        a.mature = a.age_days * 24.0 >= HARE_MATURITY_AGE_H;
        a.vulnerability = a.mature ? HARE_VULN_MATURE : HARE_VULN_JUV;
        m.agents.push_back(a);
    }
    // Granja de cada perro: round-robin sobre n_dogs (camino original) o, con
    // dogs_farm "n0,n1,...", tantos perros por estancia como se pidan (en orden farm_id).
    std::vector<int> dog_farm;
    if (params.dogs_farm) {
        const char* s = params.dogs_farm; int f = 0;
        while (*s) { char* e; long n = std::strtol(s, &e, 10);
            if (e == s || n < 0) { std::fprintf(stderr, "dogs_farm invalido: \"%s\"\n", params.dogs_farm); std::exit(2); }
            for (long j = 0; j < n; ++j) dog_farm.push_back(f);
            ++f; s = (*e == ',') ? e + 1 : e; if (*e && *e != ',') { std::fprintf(stderr, "dogs_farm invalido: \"%s\"\n", params.dogs_farm); std::exit(2); } }
        if (f != m.n_farms) { std::fprintf(stderr, "dogs_farm da %d estancias; el modelo tiene %d\n", f, m.n_farms); std::exit(2); }
    } else for (int k = 0; k < params.n_dogs; ++k) dog_farm.push_back(grouped ? k % m.n_farms : 0);
    for (int k = 0; k < (int)dog_farm.size(); ++k) {
        double a0 = rng_build.range(0.0, TAU);
        double dcx = fcx, dcy = fcy;                       // n_farms>1: un perro por granja (round-robin) en su casco
        if (grouped) { int g = dog_group(dog_farm[k]); dcx = gc[g].first; dcy = gc[g].second; }  // con potreros: en el potrero mayor de su granja
        Animal a{}; a.species = DOG; a.alive = true; a.energy = 100; a.mother = -1;
        a.x = clampd(dcx + std::cos(a0) * 300.0, 0, params.width); a.y = clampd(dcy + std::sin(a0) * 300.0, 0, params.height);
        // con dogs_farm el perro debe quedar en SU estancia: si el desplazamiento de 300 m
        // lo saca, parte en el centro de su grupo (no consume RNG).
        if (params.dogs_farm && grouped && m.farm_of(a.x, a.y) != dog_farm[k]) { a.x = dcx; a.y = dcy; }
        a.patrol_cx = dcx; a.patrol_cy = dcy;
        m.agents.push_back(a);
    }
    // Granja de origen de las especies cercadas, según su posición de creación
    // (n_farms y cascos ya se fijaron arriba). Con n_farms<=1 queda farm=0.
    for (auto& a : m.agents)
        if (a.species == SHEEP || a.species == DOG) { a.farm = m.farm_of(a.x, a.y); a.unit = m.unit_of(a.x, a.y); }

    phaseA.clear(); phaseB.clear(); foxidx.clear();
    for (size_t k = 0; k < m.agents.size(); ++k) {
        int sp = m.agents[k].species;
        if (sp == SHEEP || sp == HARE) phaseA.push_back((int)k); else phaseB.push_back((int)k);
        if (sp == FOX) foxidx.push_back((int)k);
    }
}

// Carga un raster de granjas (formato simple: "cols rows cellsize" seguido de
// cols*rows ids de granja, row-major, fila 0 = sur). Fija farm_raster, dimensiones
// y n_farms del modelo. Llamar ANTES de build_model; el driver ademas fija
// params.width/height = cols*csz x rows*csz. Devuelve false si no pudo leerlo
// (path nulo o error) -> el motor sigue con la particion por defecto.
static bool load_farm_raster(Model& m, const char* path) {
    if (!path) return false;
    FILE* fp = std::fopen(path, "r");
    if (!fp) { std::fprintf(stderr, "load_farm_raster: no pude abrir %s\n", path); return false; }
    int cols = 0, rows = 0; double csz = 0.0;
    if (std::fscanf(fp, "%d %d %lf", &cols, &rows, &csz) != 3 || cols <= 0 || rows <= 0 || csz <= 0.0) {
        std::fprintf(stderr, "load_farm_raster: cabecera invalida en %s\n", path); std::fclose(fp); return false;
    }
    std::vector<int> data((size_t)cols * rows); int maxid = -1;
    for (size_t i = 0; i < data.size(); ++i) {
        if (std::fscanf(fp, "%d", &data[i]) != 1) {
            std::fprintf(stderr, "load_farm_raster: faltan datos en %s (celda %zu de %zu)\n", path, i, data.size());
            std::fclose(fp); return false;
        }
        if (data[i] > maxid) maxid = data[i];
    }
    std::fclose(fp);
    m.farm_cols = cols; m.farm_rows = rows; m.farm_csz = csz;
    m.farm_raster = std::move(data); m.n_farms = maxid + 1;
    return true;
}

// Carga los cascos (casas) reales: cabecera "n" + n lineas "farm_id x y" en coords
// locales. Llena m.farm_homesteads[farm]. Llamar DESPUES de build_model (n_farms ya
// fijado). La cacería dispara desde una casa de la granja si hay; si no, del
// centroide. Devuelve false si no pudo leerlo (path nulo o error).
static bool load_cascos(Model& m, const char* path) {
    if (!path) return false;
    FILE* fp = std::fopen(path, "r");
    if (!fp) { std::fprintf(stderr, "load_cascos: no pude abrir %s\n", path); return false; }
    int n = 0;
    if (std::fscanf(fp, "%d", &n) != 1 || n < 0) { std::fprintf(stderr, "load_cascos: cabecera invalida\n"); std::fclose(fp); return false; }
    m.farm_homesteads.assign(m.n_farms, {});
    for (int i = 0; i < n; ++i) {
        int f = -1; double x = 0, y = 0;
        if (std::fscanf(fp, "%d %lf %lf", &f, &x, &y) != 3) { std::fprintf(stderr, "load_cascos: dato invalido (casa %d)\n", i); std::fclose(fp); return false; }
        if (f >= 0 && f < m.n_farms) m.farm_homesteads[f].push_back({x, y});
    }
    std::fclose(fp);
    return true;
}

// Carga el raster de potreros (mismo formato y MISMA grilla que el de granjas) y,
// opcionalmente, la tabla .units ("n" + lineas "unit_id farm_id nombre area_ha x y",
// coords locales). Deriva unit_farm del raster de granjas y exige que cada unidad
// caiga en UNA sola granja (el potrero refina la estancia, no la cruza). Sin .units,
// peso = celdas y centro = centroide de celdas. Llamar DESPUES de load_farm_raster
// y ANTES de build_model. Devuelve false (con mensaje) ante cualquier error: el
// driver debe ABORTAR, no seguir sin cercos en silencio.
static bool load_paddocks(Model& m, const char* raster_path, const char* units_path) {
    if (!raster_path) return false;
    if (m.farm_raster.empty()) { std::fprintf(stderr, "load_paddocks: requiere --farm-raster\n"); return false; }
    FILE* fp = std::fopen(raster_path, "r");
    if (!fp) { std::fprintf(stderr, "load_paddocks: no pude abrir %s\n", raster_path); return false; }
    int cols = 0, rows = 0; double csz = 0.0;
    if (std::fscanf(fp, "%d %d %lf", &cols, &rows, &csz) != 3 || cols != m.farm_cols || rows != m.farm_rows || csz != m.farm_csz) {
        std::fprintf(stderr, "load_paddocks: la grilla de %s (%dx%d @%.1f) no coincide con la de granjas (%dx%d @%.1f)\n",
                     raster_path, cols, rows, csz, m.farm_cols, m.farm_rows, m.farm_csz); std::fclose(fp); return false;
    }
    std::vector<int> data((size_t)cols * rows); int maxid = -1;
    for (size_t i = 0; i < data.size(); ++i) {
        if (std::fscanf(fp, "%d", &data[i]) != 1 || data[i] < 0) {
            std::fprintf(stderr, "load_paddocks: dato invalido en %s (celda %zu de %zu)\n", raster_path, i, data.size());
            std::fclose(fp); return false;
        }
        if (data[i] > maxid) maxid = data[i];
    }
    std::fclose(fp);
    const int n = maxid + 1;
    std::vector<int> uf(n, -1); std::vector<double> cells(n, 0.0), sx(n, 0.0), sy(n, 0.0);
    for (int r = 0; r < rows; ++r) for (int c = 0; c < cols; ++c) {
        const size_t i = (size_t)r * cols + c; const int u = data[i], f = m.farm_raster[i];
        if (uf[u] < 0) uf[u] = f;
        else if (uf[u] != f) { std::fprintf(stderr, "load_paddocks: la unidad %d cae en las granjas %d y %d\n", u, uf[u], f); return false; }
        cells[u] += 1.0; sx[u] += (c + 0.5) * csz; sy[u] += (r + 0.5) * csz;   // fila 0 = sur, como farm_of
    }
    for (int u = 0; u < n; ++u) if (uf[u] < 0) { std::fprintf(stderr, "load_paddocks: la unidad %d no tiene celdas\n", u); return false; }
    m.paddock_raster = std::move(data); m.n_units = n; m.unit_farm = uf;
    m.unit_weight = cells; m.unit_center.resize(n);
    for (int u = 0; u < n; ++u) m.unit_center[u] = {sx[u] / cells[u], sy[u] / cells[u]};
    if (!units_path) return true;
    FILE* fu = std::fopen(units_path, "r");
    if (!fu) { std::fprintf(stderr, "load_paddocks: no pude abrir %s\n", units_path); return false; }
    int nu = 0;
    if (std::fscanf(fu, "%d", &nu) != 1 || nu != n) { std::fprintf(stderr, "load_paddocks: %s declara %d unidades, el raster tiene %d\n", units_path, nu, n); std::fclose(fu); return false; }
    char name[256];
    for (int i = 0; i < nu; ++i) {
        int u = -1, f = -1; double ha = 0, x = 0, y = 0;
        if (std::fscanf(fu, "%d %d %255s %lf %lf %lf", &u, &f, name, &ha, &x, &y) != 6 || u < 0 || u >= n) {
            std::fprintf(stderr, "load_paddocks: linea invalida en %s (unidad %d)\n", units_path, i); std::fclose(fu); return false; }
        if (f != m.unit_farm[u]) { std::fprintf(stderr, "load_paddocks: %s dice que la unidad %d es de la granja %d; el raster dice %d\n", units_path, u, f, m.unit_farm[u]); std::fclose(fu); return false; }
        if (ha <= 0 || x < 0 || y < 0 || x > cols * csz || y > rows * csz) { std::fprintf(stderr, "load_paddocks: area o centro invalidos (unidad %d)\n", u); std::fclose(fu); return false; }
        if (m.unit_of(x, y) != u) { std::fprintf(stderr, "load_paddocks: el centro (%.0f,%.0f) de la unidad %d cae en la unidad %d\n", x, y, u, m.unit_of(x, y)); std::fclose(fu); return false; }
        m.unit_weight[u] = ha; m.unit_center[u] = {x, y};
    }
    std::fclose(fu);
    return true;
}

// Carga la estación de cada unidad ("n" + lineas "unit_id estacion", 0 invernada /
// 1 veranada). Requiere potreros ya cargados y el mismo número de unidades. Exige que
// cada estancia tenga al menos una unidad de cada estación. Devuelve false (con
// mensaje) ante cualquier error: el driver debe ABORTAR.
static bool load_season(Model& m, const char* path) {
    if (!path) return false;
    if (!m.has_paddocks()) { std::fprintf(stderr, "load_season: requiere --paddocks\n"); return false; }
    FILE* fp = std::fopen(path, "r");
    if (!fp) { std::fprintf(stderr, "load_season: no pude abrir %s\n", path); return false; }
    int n = 0;
    if (std::fscanf(fp, "%d", &n) != 1 || n != m.n_units) { std::fprintf(stderr, "load_season: %s declara %d unidades, hay %d\n", path, n, m.n_units); std::fclose(fp); return false; }
    std::vector<int> s(n, -1);
    for (int i = 0; i < n; ++i) {
        int u = -1, v = -1;
        if (std::fscanf(fp, "%d %d", &u, &v) != 2 || u < 0 || u >= n || (v != 0 && v != 1)) {
            std::fprintf(stderr, "load_season: linea invalida en %s (%d)\n", path, i); std::fclose(fp); return false; }
        s[u] = v;
    }
    std::fclose(fp);
    for (int f = 0; f < m.n_farms; ++f) {
        bool has0 = false, has1 = false;
        for (int u = 0; u < n; ++u) if (m.unit_farm[u] == f) { if (s[u] < 0) { std::fprintf(stderr, "load_season: falta la unidad %d\n", u); return false; } has0 |= s[u] == 0; has1 |= s[u] == 1; }
        if (!has0 || !has1) { std::fprintf(stderr, "load_season: la estancia %d no tiene unidades de ambas estaciones\n", f); return false; }
    }
    m.unit_season = s;
    return true;
}

// Carga la máscara de tierra: formato .farms, valores 0 (mar) / 1 (tierra), MISMA
// grilla que el ráster de granjas. Fija land_area_ha. Llamar DESPUÉS de
// load_farm_raster y ANTES de build_model. false (con mensaje) ante error.
static bool load_land(Model& m, const char* path) {
    if (!path) return false;
    if (m.farm_raster.empty()) { std::fprintf(stderr, "load_land: requiere --farm-raster\n"); return false; }
    FILE* fp = std::fopen(path, "r");
    if (!fp) { std::fprintf(stderr, "load_land: no pude abrir %s\n", path); return false; }
    int cols = 0, rows = 0; double csz = 0.0;
    if (std::fscanf(fp, "%d %d %lf", &cols, &rows, &csz) != 3 || cols != m.farm_cols || rows != m.farm_rows || csz != m.farm_csz) {
        std::fprintf(stderr, "load_land: la grilla de %s no coincide con la de granjas\n", path); std::fclose(fp); return false; }
    std::vector<uint8_t> d((size_t)cols * rows); long n_land = 0;
    for (size_t i = 0; i < d.size(); ++i) {
        int v;
        if (std::fscanf(fp, "%d", &v) != 1 || (v != 0 && v != 1)) { std::fprintf(stderr, "load_land: valor inválido (celda %zu)\n", i); std::fclose(fp); return false; }
        d[i] = (uint8_t)v; n_land += v;
    }
    std::fclose(fp);
    if (n_land == 0) { std::fprintf(stderr, "load_land: la máscara no tiene tierra\n"); return false; }
    m.land_mask = std::move(d); m.land_area_ha = n_land * csz * csz / 10000.0;
    return true;
}

// Carga un ráster de vegetación (calidad o cobertura) en el formato de .farms
// ("cols rows cellsize" + cols*rows valores, row-major, fila 0 = SUR) y SOBRESCRIBE
// `r`, que ya viene dimensionado por build_rasters (fila 0 = NORTE, como Raster::get).
// Exige la misma grilla (cols, rows, celda = CELL_SIZE) y valores en [0, 1].
// Devuelve false (con mensaje) ante cualquier error: el driver debe ABORTAR.
static bool load_veg_raster(Raster& r, const char* path, const char* what) {
    if (!path) return false;
    FILE* fp = std::fopen(path, "r");
    if (!fp) { std::fprintf(stderr, "load_veg_raster(%s): no pude abrir %s\n", what, path); return false; }
    int cols = 0, rows = 0; double csz = 0.0;
    if (std::fscanf(fp, "%d %d %lf", &cols, &rows, &csz) != 3 || cols != r.cols || rows != r.rows || std::fabs(csz - CELL_SIZE) > 1e-9) {
        std::fprintf(stderr, "load_veg_raster(%s): la grilla de %s (%dx%d @%.1f) no coincide con la del motor (%dx%d @%.1f)\n",
                     what, path, cols, rows, csz, r.cols, r.rows, CELL_SIZE); std::fclose(fp); return false;
    }
    std::vector<double> d((size_t)cols * rows);
    for (int rs = 0; rs < rows; ++rs) for (int c = 0; c < cols; ++c) {   // rs = fila desde el sur
        double v;
        if (std::fscanf(fp, "%lf", &v) != 1 || !(v >= 0.0 && v <= 1.0)) {
            std::fprintf(stderr, "load_veg_raster(%s): valor inválido en fila %d col %d de %s\n", what, rs, c, path); std::fclose(fp); return false; }
        d[(size_t)(rows - 1 - rs) * cols + c] = v;                     // a fila desde el norte
    }
    std::fclose(fp);
    r.data = std::move(d);
    return true;
}

#endif // SIGRID_CORE_HPP
