//! Modelo V4 HYBRID v2 de flujos de detritos, portado fielmente desde
//! `debris-flow-abm/src/simulate_copiapo.py` (Mesa/Python) a swarm-core.
//!
//! Dos tipos de agente sobre un mismo [`AgentSet`] (enum):
//! - [`Raindrop`]: posición fija; según el patrón horario de lluvia y las
//!   condiciones del terreno (sedimento, isoterma, susceptibilidad) genera
//!   flujos.
//! - [`Flow`]: desciende por el DEM eligiendo la celda con mejor score
//!   (pendiente + atracción a cauces) dentro de un radio que crece con la
//!   velocidad; con temperatura estocástica > 0 la elección es softmax.
//!   El volumen decae por paso; bajo el umbral, el flujo muere (baja
//!   diferida vía `after_step`). En zonas planas con volumen alto se
//!   divide en hijos (spread costero).
//!
//! Notas de fidelidad con el original Python:
//! - Activación **Ordered** (Mesa iteraba `list(self.agents)` sin barajar).
//! - La hora avanza ANTES de activar agentes (hook `before_step`).
//! - Los valores nodata se comparan tal cual (el original no enmascaraba).
//! - El "flat fallback" del original era código muerto (inalcanzable tras
//!   `if len(candidates) == 0: return None`); no se porta.
//! - El softmax del original usaba el RNG global de numpy (¡sin semilla!);
//!   aquí usa el [`SimRng`] de la simulación: el port es reproducible.

use std::sync::Arc;

use swarm_core::prelude::*;

use crate::raster::Window;

/// Variante física del modelo. El repositorio original tiene dos:
/// `simulate_copiapo.py` (transferibilidad a Copiapó) y
/// `simulate_fabdem_coastal.py` (Chañaral, el mejor caso). Difieren en tres
/// puntos: regla de selección de celda, radio del footprint y ley de
/// velocidad.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Physics {
    /// Selección por mejor *score* (pendiente + atracción a cauces, softmax
    /// con temperatura), radio de footprint fijo, velocidad con
    /// `critical_slope`/`slope_acceleration_factor`. Sin flat-fallback.
    Copiapo,
    /// Selección por menor *elevación* absoluta (determinista) con
    /// flat-fallback alcanzable, radio de footprint dinámico
    /// `√(volumen/π)·8`, velocidad con drag cuadrático. (Chañaral.)
    Coastal,
}

/// Parámetros del modelo. `Default` = calibración Optuna-TPE con temperatura
/// (Copiapó, `best_params_optuna_withT.json`, IoU de referencia 0.1344).
#[derive(Debug, Clone)]
pub struct Params {
    /// Variante física (regla de selección, radio, velocidad).
    pub physics: Physics,
    /// Velocidad mínima para activar el flat-fallback (solo `Coastal`).
    pub min_velocity_for_flat: f64,
    pub n_rain_agents: usize,
    pub rain_threshold: f64,
    pub sediment_threshold: f64,
    pub susceptibility_threshold: f64,
    pub friction_coefficient: f64,
    pub coastal_slope_threshold: f64,
    pub coastal_spread_factor: f64,
    pub coastal_volume_threshold: f64,
    pub volume_decay_flat: f64,
    pub volume_decay_slope: f64,
    pub stream_attraction_weight: f64,
    pub max_velocity: f64,
    pub min_velocity: f64,
    pub critical_slope: f64,
    pub slope_acceleration_factor: f64,
    pub stochastic_temperature: f64,
    pub footprint_radius: f64,
    /// Exponente del muestreo de puntos de inicio por susceptibilidad: 0 ⇒
    /// uniforme (como el original); >0 ⇒ los agentes de lluvia nacen con
    /// probabilidad ∝ `susceptibilidad^seeding_power`, concentrándolos en las
    /// zonas inestables y reduciendo flujos espurios en laderas.
    pub seeding_power: f64,
    /// Física enriquecida opcional (entrainment, Voellmy, inercia).
    /// `None` ⇒ modelo base (paridad con el original).
    pub enhanced: Option<EnhancedPhysics>,
}

/// Términos físicos adicionales, calibrables. Cada uno se reduce al modelo
/// base cuando su coeficiente es 0, salvo Voellmy que se activa con su flag.
#[derive(Debug, Clone, Copy)]
pub struct EnhancedPhysics {
    /// Tasa de arrastre de sedimento (bulking): en pendiente erosiva el
    /// volumen crece `×(1 + entrainment_coef · sedimento_local)` por paso.
    pub entrainment_coef: f64,
    /// Pendiente sobre la cual el flujo erosiona (arrastra) en vez de depositar.
    pub erosion_slope_threshold: f64,
    /// Tope de crecimiento del volumen por bulking (múltiplo del inicial).
    pub max_bulking: f64,
    /// Peso de la inercia: premia alinear el paso con la dirección acumulada.
    pub inertia_weight: f64,
    /// Reología de Voellmy: si `true`, la velocidad usa fricción de Coulomb
    /// (`mu`) + término turbulento (`v²/xi`) en vez de `gravedad − drag`.
    pub use_voellmy: bool,
    /// Coeficiente de fricción de Coulomb (Voellmy).
    pub voellmy_mu: f64,
    /// Coeficiente turbulento (Voellmy), m/s².
    pub voellmy_xi: f64,
    /// Expansión en abanico: pendiente bajo la cual el flujo desconfina y
    /// deposita esparcido (planicie / abanico aluvial). 0 ⇒ desactivado.
    pub fan_slope_threshold: f64,
    /// Multiplicador del radio de deposición en la zona de abanico
    /// (1 ⇒ sin expansión).
    pub fan_factor: f64,
}

impl Default for Params {
    fn default() -> Self {
        Self {
            physics: Physics::Copiapo,
            min_velocity_for_flat: 0.3,
            n_rain_agents: 50,
            rain_threshold: 0.144_190_846_138_895_52,
            sediment_threshold: 0.245_290_867_453_274_14,
            susceptibility_threshold: 0.315_782_720_173_570_3,
            friction_coefficient: 0.033_402_563_283_542_08,
            coastal_slope_threshold: 0.050_581_521_089_951_41,
            coastal_spread_factor: 2.922_478_772_007_097_7,
            coastal_volume_threshold: 0.599_910_119_171_883_6,
            volume_decay_flat: 0.968_981_826_375_527_2,
            volume_decay_slope: 0.980_345_773_041_599_6,
            stream_attraction_weight: 1.915_163_555_345_845,
            max_velocity: 23.585_956_684_109_266,
            min_velocity: 0.453_801_319_526_27,
            critical_slope: 0.010_085_078_220_822_6,
            slope_acceleration_factor: 1.977_372_842_263_519_3,
            stochastic_temperature: 0.284_740_340_530_182_05,
            footprint_radius: 4.0,
            seeding_power: 0.0,
            enhanced: None,
        }
    }
}

impl Params {
    /// Calibración GP "18iters" (`best_params_copiapo_18iters.json`):
    /// referencia con métricas completas — IoU 0.1455, 895 flujos,
    /// área predicha 28.24 km², con 100 agentes × 500 pasos y T=0.
    #[must_use]
    pub fn preset_18iters() -> Self {
        Self {
            physics: Physics::Copiapo,
            min_velocity_for_flat: 0.3,
            n_rain_agents: 100,
            rain_threshold: 0.137_407_585_541_073_35,
            sediment_threshold: 0.068_498_568_677_264_9,
            susceptibility_threshold: 0.363_517_258_485_731_85,
            friction_coefficient: 0.052_783_320_086_390_08,
            coastal_slope_threshold: 0.088_858_580_076_693_71,
            coastal_spread_factor: 4.086_548_259_278_382,
            coastal_volume_threshold: 0.295_064_036_168_225_96,
            volume_decay_flat: 0.977_198_782_067_501_8,
            volume_decay_slope: 0.989_717_139_643_430_1,
            stream_attraction_weight: 2.827_551_022_612_925,
            max_velocity: 28.857_071_411_159_623,
            min_velocity: 0.638_978_919_839_682_5,
            critical_slope: 0.072_530_643_973_573_42,
            slope_acceleration_factor: 1.880_467_839_015_258,
            stochastic_temperature: 0.0,
            footprint_radius: 4.0,
            seeding_power: 0.0,
            enhanced: None,
        }
    }
}

impl Params {
    /// Calibración propia por Differential Evolution sobre el port Rust,
    /// objetivo ROBUSTO = IoU medio sobre 3 semillas (`bin/calibrate
    /// --eval-seeds 3`, 672×3 simulaciones en ~5 min; `data/best_params_de.json`).
    /// Notablemente, el objetivo multi-semilla colapsa la temperatura
    /// estocástica a ~0.02 (casi determinista): calibrar contra una sola
    /// semilla la inflaba a ~1.8 sobreajustando al ruido.
    #[must_use]
    pub fn preset_de() -> Self {
        Self {
            physics: Physics::Copiapo,
            min_velocity_for_flat: 0.3,
            n_rain_agents: 50,
            rain_threshold: 0.3,
            sediment_threshold: 0.092_558_033_987_087_35,
            susceptibility_threshold: 0.197_823_133_809_339_7,
            friction_coefficient: 0.089_147_511_065_123_7,
            coastal_slope_threshold: 0.01,
            coastal_spread_factor: 3.393_513_238_230_253_7,
            coastal_volume_threshold: 1.171_472_310_161_277_8,
            volume_decay_flat: 0.979_308_381_555_189_5,
            volume_decay_slope: 0.993_423_792_099_001_9,
            stream_attraction_weight: 10.0,
            max_velocity: 22.201_013_730_875_697,
            min_velocity: 0.653_928_601_682_270_1,
            critical_slope: 0.01,
            slope_acceleration_factor: 1.967_888_613_326_076,
            stochastic_temperature: 0.020_193_507_448_305_392,
            footprint_radius: 4.0,
            seeding_power: 0.0,
            enhanced: None,
        }
    }

    /// Config B (`config_b_final_params.json`, iteración #39): el **mejor caso
    /// documentado**, calibrado en Chañaral con la física `Coastal`.
    /// IoU de referencia 0.4653 (precision 0.673, recall 0.602) sobre el bbox
    /// urbano. 100 agentes × 500 pasos.
    #[must_use]
    pub fn preset_chanaral() -> Self {
        Self {
            // Config B trae `radius: 4.0` y sin `stochastic_temperature`: es el
            // modelo de radio fijo determinista (estilo Copiapó), no el coastal
            // dinámico.
            physics: Physics::Copiapo,
            min_velocity_for_flat: 0.3,
            n_rain_agents: 100,
            rain_threshold: 0.101_606_108_481_210_84,
            sediment_threshold: 0.094_635_181_408_432_01,
            susceptibility_threshold: 0.198_861_568_955_504_58,
            friction_coefficient: 0.037_457_245_035_043_26,
            coastal_slope_threshold: 0.049_142_772_130_048_896,
            coastal_spread_factor: 3.0,
            coastal_volume_threshold: 0.507_932_647_213_464_8,
            volume_decay_flat: 0.982_647_199_400_866_5,
            volume_decay_slope: 0.992_117_700_717_812_9,
            stream_attraction_weight: 4.885_735_349_727_721,
            max_velocity: 21.769_307_004_602_613,
            min_velocity: 0.455_355_360_773_441_74,
            critical_slope: 0.044_696_974_323_989_53,
            slope_acceleration_factor: 1.272_307_202_025_451_8,
            stochastic_temperature: 0.0,
            footprint_radius: 4.0,
            seeding_power: 0.0,
            enhanced: None,
        }
    }

    /// Chañaral con física enriquecida, **calibrado** por DE
    /// (`bin/calibrate_chanaral`, objetivo media−sd, 17 parámetros;
    /// `data/best_params_chanaral_enhanced.json`). IoU **0.543 ± 0.083** fuera
    /// de muestra (precision 0.745, recall 0.669, F1 0.700) vs 0.468 del
    /// baseline — **+16 %** sobre el mejor caso histórico. Combina dos mejoras
    /// dirigidas por el diagnóstico del error: la **expansión en abanico**
    /// (captura los falsos negativos de la planicie urbana) y el **inicio
    /// ponderado por susceptibilidad** (`seeding_power = 3.5`), que recupera la
    /// precision concentrando los flujos en las zonas inestables.
    #[must_use]
    pub fn preset_chanaral_enhanced() -> Self {
        Self {
            rain_threshold: 0.196_970_325_955_673_3,
            sediment_threshold: 0.147_801_495_370_392_03,
            susceptibility_threshold: 0.175_755_024_654_978_41,
            stream_attraction_weight: 4.536_514_280_928_059,
            volume_decay_flat: 0.949_366_166_127_164_8,
            footprint_radius: 3.310_278_644_161_179,
            coastal_volume_threshold: 0.227_267_600_791_588_03,
            coastal_slope_threshold: 0.052_761_538_154_907_18,
            seeding_power: 3.534_082_979_697_019_2,
            enhanced: Some(EnhancedPhysics {
                entrainment_coef: 0.123_318_919_555_470_06,
                erosion_slope_threshold: 0.129_092_426_712_826_45,
                max_bulking: 3.980_133_155_065_993_3,
                inertia_weight: 1.755_878_640_972_358,
                use_voellmy: true,
                voellmy_mu: 0.02,
                voellmy_xi: 2899.5234547305,
                fan_slope_threshold: 0.002_748_885_337_396_284,
                fan_factor: 5.486_580_964_957_672,
            }),
            ..Self::preset_chanaral()
        }
    }
}

/// Una dimensión calibrable: nombre, rango y setter sobre [`Params`].
pub struct ParamDim {
    pub name: &'static str,
    pub lo: f64,
    pub hi: f64,
    pub set: fn(&mut Params, f64),
}

/// Espacio de búsqueda de la calibración: los 15 parámetros continuos del
/// modelo con los mismos rangos que la calibración Optuna original.
/// `n_rain_agents` y `footprint_radius` quedan fijos. Compartido por el
/// calibrador (`bin/calibrate`) y el benchmark (`bin/benchmark`).
pub const PARAM_DIMS: &[ParamDim] = &[
    ParamDim {
        name: "rain_threshold",
        lo: 0.01,
        hi: 0.3,
        set: |p, v| p.rain_threshold = v,
    },
    ParamDim {
        name: "sediment_threshold",
        lo: 0.01,
        hi: 0.3,
        set: |p, v| p.sediment_threshold = v,
    },
    ParamDim {
        name: "susceptibility_threshold",
        lo: 0.05,
        hi: 0.4,
        set: |p, v| p.susceptibility_threshold = v,
    },
    ParamDim {
        name: "friction_coefficient",
        lo: 0.01,
        hi: 0.1,
        set: |p, v| p.friction_coefficient = v,
    },
    ParamDim {
        name: "coastal_slope_threshold",
        lo: 0.01,
        hi: 0.15,
        set: |p, v| p.coastal_slope_threshold = v,
    },
    ParamDim {
        name: "coastal_spread_factor",
        lo: 2.0,
        hi: 5.0,
        set: |p, v| p.coastal_spread_factor = v,
    },
    ParamDim {
        name: "coastal_volume_threshold",
        lo: 0.1,
        hi: 1.5,
        set: |p, v| p.coastal_volume_threshold = v,
    },
    ParamDim {
        name: "volume_decay_flat",
        lo: 0.95,
        hi: 0.995,
        set: |p, v| p.volume_decay_flat = v,
    },
    ParamDim {
        name: "volume_decay_slope",
        lo: 0.98,
        hi: 0.998,
        set: |p, v| p.volume_decay_slope = v,
    },
    ParamDim {
        name: "stream_attraction_weight",
        lo: 1.0,
        hi: 10.0,
        set: |p, v| p.stream_attraction_weight = v,
    },
    ParamDim {
        name: "max_velocity",
        lo: 10.0,
        hi: 30.0,
        set: |p, v| p.max_velocity = v,
    },
    ParamDim {
        name: "min_velocity",
        lo: 0.1,
        hi: 1.0,
        set: |p, v| p.min_velocity = v,
    },
    ParamDim {
        name: "critical_slope",
        lo: 0.01,
        hi: 0.1,
        set: |p, v| p.critical_slope = v,
    },
    ParamDim {
        name: "slope_acceleration_factor",
        lo: 1.0,
        hi: 2.0,
        set: |p, v| p.slope_acceleration_factor = v,
    },
    ParamDim {
        name: "stochastic_temperature",
        lo: 0.0,
        hi: 2.0,
        set: |p, v| p.stochastic_temperature = v,
    },
];

/// Construye `Params` desde un vector de genes (resto = defaults del modelo).
#[must_use]
pub fn params_from_genes(x: &[f64], n_agents: usize) -> Params {
    let mut p = Params {
        n_rain_agents: n_agents,
        ..Params::default()
    };
    for (dim, &v) in PARAM_DIMS.iter().zip(x) {
        (dim.set)(&mut p, v);
    }
    p
}

/// Espacio de calibración del modelo **enriquecido** de Chañaral: parámetros
/// base relevantes + los 6 términos físicos (entrainment, Voellmy, inercia).
/// Los setters de la sección enriquecida asumen `enhanced = Some(..)`
/// (garantizado por [`Params::preset_chanaral_enhanced`]).
pub const PARAM_DIMS_CHANARAL: &[ParamDim] = &[
    ParamDim {
        name: "rain_threshold",
        lo: 0.01,
        hi: 0.3,
        set: |p, v| p.rain_threshold = v,
    },
    ParamDim {
        name: "sediment_threshold",
        lo: 0.01,
        hi: 0.3,
        set: |p, v| p.sediment_threshold = v,
    },
    ParamDim {
        name: "susceptibility_threshold",
        lo: 0.05,
        hi: 0.4,
        set: |p, v| p.susceptibility_threshold = v,
    },
    ParamDim {
        name: "stream_attraction_weight",
        lo: 1.0,
        hi: 10.0,
        set: |p, v| p.stream_attraction_weight = v,
    },
    ParamDim {
        name: "volume_decay_flat",
        lo: 0.90,
        hi: 0.999,
        set: |p, v| p.volume_decay_flat = v,
    },
    ParamDim {
        name: "footprint_radius",
        lo: 2.0,
        hi: 6.0,
        set: |p, v| p.footprint_radius = v,
    },
    ParamDim {
        name: "coastal_volume_threshold",
        lo: 0.1,
        hi: 1.5,
        set: |p, v| p.coastal_volume_threshold = v,
    },
    ParamDim {
        name: "coastal_slope_threshold",
        lo: 0.01,
        hi: 0.15,
        set: |p, v| p.coastal_slope_threshold = v,
    },
    // --- física enriquecida ---
    ParamDim {
        name: "entrainment_coef",
        lo: 0.0,
        hi: 0.2,
        set: |p, v| p.enhanced.as_mut().unwrap().entrainment_coef = v,
    },
    ParamDim {
        name: "erosion_slope_threshold",
        lo: 0.02,
        hi: 0.3,
        set: |p, v| p.enhanced.as_mut().unwrap().erosion_slope_threshold = v,
    },
    ParamDim {
        name: "max_bulking",
        lo: 1.0,
        hi: 10.0,
        set: |p, v| p.enhanced.as_mut().unwrap().max_bulking = v,
    },
    ParamDim {
        name: "inertia_weight",
        lo: 0.0,
        hi: 3.0,
        set: |p, v| p.enhanced.as_mut().unwrap().inertia_weight = v,
    },
    ParamDim {
        name: "voellmy_mu",
        lo: 0.02,
        hi: 0.4,
        set: |p, v| p.enhanced.as_mut().unwrap().voellmy_mu = v,
    },
    ParamDim {
        name: "voellmy_xi",
        lo: 200.0,
        hi: 4000.0,
        set: |p, v| p.enhanced.as_mut().unwrap().voellmy_xi = v,
    },
    ParamDim {
        name: "fan_slope_threshold",
        lo: 0.0,
        hi: 0.08,
        set: |p, v| p.enhanced.as_mut().unwrap().fan_slope_threshold = v,
    },
    ParamDim {
        name: "fan_factor",
        lo: 1.0,
        hi: 6.0,
        set: |p, v| p.enhanced.as_mut().unwrap().fan_factor = v,
    },
    ParamDim {
        name: "seeding_power",
        lo: 0.0,
        hi: 5.0,
        set: |p, v| p.seeding_power = v,
    },
];

/// Construye `Params` del modelo enriquecido de Chañaral desde genes.
#[must_use]
pub fn params_chanaral_from_genes(x: &[f64]) -> Params {
    let mut p = Params::preset_chanaral_enhanced();
    for (dim, &v) in PARAM_DIMS_CHANARAL.iter().zip(x) {
        (dim.set)(&mut p, v);
    }
    p
}

/// Capas raster de entrada (todas alineadas a la misma grilla).
#[derive(Clone)]
pub struct Layers {
    pub dem: Grid2D<f32>,
    pub slope: Grid2D<f32>,
    /// Precipitación diaria (un mapa por día del evento).
    pub rain: Vec<Grid2D<f32>>,
    pub isotherm: Grid2D<f32>,
    pub sediment: Grid2D<f32>,
    pub susceptibility: Grid2D<f32>,
    pub streams: Grid2D<f32>,
}

/// Patrón horario de desagregación de la lluvia diaria (día → (hora, fracción)).
const HOURLY_PATTERNS: [&[(u64, f64)]; 3] = [
    &[(6, 0.70), (7, 0.30)],
    &[(4, 0.40), (5, 0.30), (12, 0.20), (18, 0.10)],
    &[(2, 0.50), (8, 0.30), (14, 0.20)],
];

const GRAVITY: f64 = 9.81;
const MIN_FLOW_VOLUME: f64 = 0.05;

#[derive(Debug)]
pub struct Raindrop {
    pub pos: Pos,
}

#[derive(Debug)]
pub struct Flow {
    pub pos: Pos,
    pub volume: f64,
    pub velocity: f64,
    pub has_spread: bool,
    /// Radio del footprint de este flujo. Fijo (`footprint_radius`) en
    /// `Copiapo`; dinámico `√(volumen_inicial/π)·8` en `Coastal`.
    pub radius: f64,
    /// Volumen inicial (referencia para el tope de bulking).
    pub initial_volume: f64,
    /// Dirección normalizada del último desplazamiento (para la inercia).
    pub dir: (f64, f64),
}

#[derive(Debug)]
pub enum DebrisAgent {
    Raindrop(Raindrop),
    Flow(Flow),
}

pub struct DebrisFlowModel {
    pub agents: AgentSet<DebrisAgent>,
    /// Capas de entrada compartidas: `Arc` para que muchas evaluaciones
    /// (calibración) reusen el stack sin copiar ~1.2 GB por corrida.
    pub layers: Arc<Layers>,
    pub params: Params,
    pub pixel_size: f64,
    /// Hora de simulación (1-based: avanza antes de activar agentes).
    pub hour: u64,
    /// Celdas alcanzadas por algún flujo (disco de radio `footprint_radius`).
    pub footprint: Grid2D<bool>,
    pub flows_created: usize,
    /// Movimientos totales de flujos (diagnóstico).
    pub total_moves: u64,
    /// Muertes por volumen agotado vs por quedar atascado (diagnóstico).
    pub deaths_volume: usize,
    pub deaths_stuck: usize,
    /// Bajas diferidas: los flujos muertos se eliminan en `after_step`.
    dead: Vec<AgentId>,
    /// Offsets del disco de footprint, precomputados.
    disc: Vec<(i64, i64)>,
}

impl DebrisFlowModel {
    pub fn new(layers: Arc<Layers>, params: Params, pixel_size: f64, seed: u64) -> Self {
        let width = layers.dem.width();
        let height = layers.dem.height();

        // Disco: el original suma peso (1 - d/r) si d <= r; el peso es > 0
        // (y por tanto marca la celda) solo si d < r estrictamente.
        let r = params.footprint_radius;
        let ri = r.ceil() as i64;
        let mut disc = Vec::new();
        for dy in -ri..=ri {
            for dx in -ri..=ri {
                if ((dx * dx + dy * dy) as f64).sqrt() < r {
                    disc.push((dx, dy));
                }
            }
        }

        let mut model = Self {
            agents: AgentSet::new(),
            footprint: Grid2D::fill(width, height, false),
            layers,
            params,
            pixel_size,
            hour: 0,
            flows_created: 0,
            total_moves: 0,
            deaths_volume: 0,
            deaths_stuck: 0,
            dead: Vec::new(),
            disc,
        };

        // Agentes de lluvia en celdas válidas distintas.
        let mut rng = rng_from_seed(seed ^ 0xDEB1_5F10);
        let mut usadas = std::collections::HashSet::new();
        let power = model.params.seeding_power;
        if power == 0.0 {
            // Camino original: colocación uniforme (paridad bit a bit).
            while usadas.len() < model.params.n_rain_agents {
                let pos = Pos::new(rng.random_range(0..width), rng.random_range(0..height));
                if !model.layers.dem[pos].is_nan() && usadas.insert(pos) {
                    model.agents.insert(DebrisAgent::Raindrop(Raindrop { pos }));
                }
            }
        } else {
            // Muestreo por rechazo ponderado por susceptibilidad^power: los
            // agentes se concentran en zonas inestables. Tope de intentos por
            // si hay pocas celdas susceptibles.
            let max_intentos = model.params.n_rain_agents * 10_000;
            let mut intentos = 0;
            while usadas.len() < model.params.n_rain_agents && intentos < max_intentos {
                intentos += 1;
                let pos = Pos::new(rng.random_range(0..width), rng.random_range(0..height));
                if model.layers.dem[pos].is_nan() {
                    continue;
                }
                let sus = f64::from(model.layers.susceptibility[pos]).clamp(0.0, 1.0);
                if rng.random_range(0.0..1.0) < sus.powf(power) && usadas.insert(pos) {
                    model.agents.insert(DebrisAgent::Raindrop(Raindrop { pos }));
                }
            }
        }
        model
    }

    /// Radio del footprint de un flujo según su volumen y la física.
    fn flow_radius(&self, volume: f64) -> f64 {
        match self.params.physics {
            Physics::Copiapo => self.params.footprint_radius,
            // Disco proporcional a la raíz del volumen (área ∝ volumen).
            Physics::Coastal => (volume / std::f64::consts::PI).sqrt() * 8.0,
        }
    }

    /// Marca el disco de footprint de radio `radius` centrado en `pos`.
    /// `Copiapo` usa el disco fijo precomputado (`d < r`); `Coastal` un disco
    /// dinámico por flujo (`d² ≤ ⌊r⌋²`, como el original).
    fn mark_footprint(&mut self, pos: Pos, radius: f64) {
        let (w, h) = (
            self.footprint.width() as i64,
            self.footprint.height() as i64,
        );
        let mut mark = |dx: i64, dy: i64| {
            let (x, y) = (pos.x as i64 + dx, pos.y as i64 + dy);
            if x >= 0 && y >= 0 && x < w && y < h {
                self.footprint[Pos::new(x as usize, y as usize)] = true;
            }
        };
        // Disco fijo precomputado solo en el modelo base `Copiapo`; con física
        // enriquecida (o `Coastal`) el radio es dinámico —necesario para la
        // expansión en abanico, que agranda el radio en la planicie.
        if self.params.physics == Physics::Copiapo && self.params.enhanced.is_none() {
            for &(dx, dy) in &self.disc {
                mark(dx, dy);
            }
        } else {
            let rp = (radius as i64).max(1);
            for dy in -rp..=rp {
                for dx in -rp..=rp {
                    if dx * dx + dy * dy <= rp * rp {
                        mark(dx, dy);
                    }
                }
            }
        }
    }

    fn spawn_flow(&mut self, pos: Pos, volume: f64, velocity: f64, has_spread: bool) {
        let radius = self.flow_radius(volume);
        self.agents.insert(DebrisAgent::Flow(Flow {
            pos,
            volume,
            velocity,
            has_spread,
            radius,
            initial_volume: volume,
            dir: (0.0, 0.0),
        }));
        self.flows_created += 1;
        self.mark_footprint(pos, radius);
    }

    /// Flujos vivos en este momento.
    pub fn active_flows(&self) -> usize {
        self.agents
            .iter()
            .filter(|(_, a)| matches!(a, DebrisAgent::Flow(_)))
            .count()
    }
}

impl Model for DebrisFlowModel {
    type Agent = DebrisAgent;

    fn agents(&self) -> &AgentSet<DebrisAgent> {
        &self.agents
    }

    fn agents_mut(&mut self) -> &mut AgentSet<DebrisAgent> {
        &mut self.agents
    }

    fn before_step(&mut self, _rng: &mut SimRng) {
        self.hour += 1;
    }

    fn after_step(&mut self, _rng: &mut SimRng) {
        for id in self.dead.drain(..) {
            self.agents.remove(id);
        }
    }

    /// Tras el evento de lluvia (72 h) y sin flujos vivos, no pasa nada más.
    fn finished(&self) -> bool {
        self.hour >= 72 && self.active_flows() == 0
    }
}

impl Agent for DebrisAgent {
    type Model = DebrisFlowModel;

    fn step(&mut self, id: AgentId, model: &mut DebrisFlowModel, rng: &mut SimRng) {
        match self {
            DebrisAgent::Raindrop(r) => r.step(model),
            DebrisAgent::Flow(f) => f.step(id, model, rng),
        }
    }
}

impl Raindrop {
    fn step(&mut self, model: &mut DebrisFlowModel) {
        let rain = self.hourly_rain(model);
        if rain > model.params.rain_threshold && self.can_generate_flow(model) {
            model.spawn_flow(self.pos, rain * 2.0, 0.0, false);
        }
    }

    fn hourly_rain(&self, model: &DebrisFlowModel) -> f64 {
        let day = (model.hour / 24) as usize;
        let hour_of_day = model.hour % 24;
        if day >= model.layers.rain.len() {
            return 0.0;
        }
        let daily = f64::from(model.layers.rain[day][self.pos]);
        if daily.is_nan() {
            return 0.0;
        }
        let factor = HOURLY_PATTERNS
            .get(day)
            .and_then(|p| p.iter().find(|(h, _)| *h == hour_of_day))
            .map_or(0.0, |(_, f)| *f);
        daily * factor
    }

    fn can_generate_flow(&self, model: &DebrisFlowModel) -> bool {
        let p = &model.params;
        let sediment = f64::from(model.layers.sediment[self.pos]);
        let isotherm = f64::from(model.layers.isotherm[self.pos]);
        let susceptibility = f64::from(model.layers.susceptibility[self.pos]);
        // NaN > x es false: equivale a los checks isnan del original.
        sediment > p.sediment_threshold
            && isotherm > 0.5
            && susceptibility > p.susceptibility_threshold
    }
}

impl Flow {
    fn step(&mut self, id: AgentId, model: &mut DebrisFlowModel, rng: &mut SimRng) {
        if self.volume < MIN_FLOW_VOLUME {
            model.dead.push(id);
            model.deaths_volume += 1;
            return;
        }

        let slope_here = f64::from(model.layers.slope[self.pos]);
        if !self.has_spread
            && slope_here < model.params.coastal_slope_threshold
            && self.volume > model.params.coastal_volume_threshold
        {
            self.coastal_spread(model);
        }

        match self.find_next_position(model, rng) {
            Some((next, slope)) => {
                // Dirección del desplazamiento (para la inercia del próximo paso).
                let (dx, dy) = (
                    next.x as f64 - self.pos.x as f64,
                    next.y as f64 - self.pos.y as f64,
                );
                let norm = (dx * dx + dy * dy).sqrt();
                if norm > 0.0 {
                    self.dir = (dx / norm, dy / norm);
                }
                self.pos = next;
                self.velocity = self.update_velocity(slope, &model.params);
                self.update_volume(slope, model);
                // Expansión en abanico: al desconfinar en la planicie de baja
                // pendiente, el radio de deposición crece (esparcido lateral).
                let next_slope = f64::from(model.layers.slope[next]);
                let mark_radius = match &model.params.enhanced {
                    Some(e) if e.fan_factor > 1.0 && next_slope < e.fan_slope_threshold => {
                        self.radius * e.fan_factor
                    }
                    _ => self.radius,
                };
                model.mark_footprint(next, mark_radius);
                model.total_moves += 1;
            }
            None => {
                self.velocity *= 0.9;
                if self.velocity < 0.01 {
                    model.dead.push(id);
                    model.deaths_stuck += 1;
                }
            }
        }
    }

    /// Evolución del volumen. Base: decae según pendiente. Con física
    /// enriquecida: **entrainment** (en pendiente erosiva el volumen crece
    /// arrastrando sedimento, con tope) o **deposición** (decae en zona plana).
    fn update_volume(&mut self, slope: f64, model: &DebrisFlowModel) {
        let p = &model.params;
        match &p.enhanced {
            Some(e) if slope > e.erosion_slope_threshold => {
                let sed = f64::from(model.layers.sediment[self.pos]).max(0.0);
                let grown = self.volume * (1.0 + e.entrainment_coef * sed);
                self.volume = grown.min(self.initial_volume * e.max_bulking);
            }
            _ => {
                self.volume *= if slope > 0.01 {
                    p.volume_decay_slope
                } else {
                    p.volume_decay_flat
                };
            }
        }
    }

    /// División en zona plana: hijos en círculo a distancia 2 (igual que el
    /// original: hijos con volumen/(n+1) calculado ANTES de dividir el padre,
    /// y padre dividido por (factor_real + 1)).
    fn coastal_spread(&mut self, model: &mut DebrisFlowModel) {
        let n_children = model.params.coastal_spread_factor as i64;
        if n_children > 0 {
            let volume_per_child = self.volume / (n_children as f64 + 1.0);
            let (w, h) = (
                model.layers.dem.width() as i64,
                model.layers.dem.height() as i64,
            );
            for i in 0..n_children {
                let angle = std::f64::consts::TAU * i as f64 / n_children as f64;
                let x = self.pos.x as i64 + (angle.cos() * 2.0) as i64;
                let y = self.pos.y as i64 + (angle.sin() * 2.0) as i64;
                if x >= 0 && y >= 0 && x < w && y < h {
                    let child = Pos::new(x as usize, y as usize);
                    if !model.layers.dem[child].is_nan() {
                        model.spawn_flow(child, volume_per_child, self.velocity * 0.8, true);
                    }
                }
            }
        }
        self.has_spread = true;
        self.volume /= model.params.coastal_spread_factor + 1.0;
    }

    fn find_next_position(&self, model: &DebrisFlowModel, rng: &mut SimRng) -> Option<(Pos, f64)> {
        let current_elev = f64::from(model.layers.dem[self.pos]);
        if current_elev.is_nan() {
            return None;
        }
        let sr_base = (self.velocity as i64 * 3).clamp(2, 10);
        self.search_in_radius(model, sr_base, current_elev, rng)
            .or_else(|| self.search_in_radius(model, (sr_base + 5).min(20), current_elev, rng))
    }

    fn search_in_radius(
        &self,
        model: &DebrisFlowModel,
        radius: i64,
        current_elev: f64,
        rng: &mut SimRng,
    ) -> Option<(Pos, f64)> {
        match model.params.physics {
            Physics::Copiapo => self.search_score(model, radius, current_elev, rng),
            Physics::Coastal => self.search_min_elev(model, radius, current_elev),
        }
    }

    /// Selección `Copiapo`: mejor score (pendiente + atracción a cauces),
    /// softmax con temperatura si `stochastic_temperature > 0`.
    fn search_score(
        &self,
        model: &DebrisFlowModel,
        radius: i64,
        current_elev: f64,
        rng: &mut SimRng,
    ) -> Option<(Pos, f64)> {
        let p = &model.params;
        let (w, h) = (
            model.layers.dem.width() as i64,
            model.layers.dem.height() as i64,
        );
        // (pos, pendiente, score) — mismo orden de recorrido que el original
        // (dx externo, dy interno, ascendentes) para empates del argmax.
        let mut candidates: Vec<(Pos, f64, f64)> = Vec::new();

        for dx in -radius..=radius {
            for dy in -radius..=radius {
                if dx == 0 && dy == 0 {
                    continue;
                }
                let (nx, ny) = (self.pos.x as i64 + dx, self.pos.y as i64 + dy);
                if nx < 0 || ny < 0 || nx >= w || ny >= h {
                    continue;
                }
                let npos = Pos::new(nx as usize, ny as usize);
                let neighbor_elev = f64::from(model.layers.dem[npos]);
                if neighbor_elev.is_nan() {
                    continue;
                }
                let dist_m = ((dx * dx + dy * dy) as f64).sqrt() * model.pixel_size;
                let slope = (current_elev - neighbor_elev) / dist_m;
                if slope > 0.0 {
                    let mut score = slope;
                    if model.layers.streams[npos] > 0.0 {
                        score += p.stream_attraction_weight * slope;
                    }
                    // Inercia (física enriquecida): premia seguir la dirección
                    // acumulada (producto punto con el versor del candidato).
                    if let Some(e) = &p.enhanced
                        && e.inertia_weight > 0.0
                    {
                        let dist = ((dx * dx + dy * dy) as f64).sqrt();
                        if dist > 0.0 {
                            let align = (dx as f64 * self.dir.0 + dy as f64 * self.dir.1) / dist;
                            score += e.inertia_weight * align.max(0.0) * slope;
                        }
                    }
                    candidates.push((npos, slope, score));
                }
            }
        }

        if candidates.is_empty() {
            return None;
        }

        if p.stochastic_temperature > 0.0 && candidates.len() > 1 {
            // Softmax con temperatura (restar el máximo no altera la
            // distribución y evita overflow del exp).
            let t = p.stochastic_temperature;
            let max_score = candidates.iter().fold(f64::MIN, |m, c| m.max(c.2));
            let weights: Vec<f64> = candidates
                .iter()
                .map(|c| ((c.2 - max_score) / t).exp())
                .collect();
            let total: f64 = weights.iter().sum();
            let mut u = rng.random_range(0.0..1.0) * total;
            for (i, w) in weights.iter().enumerate() {
                u -= w;
                if u <= 0.0 {
                    return Some((candidates[i].0, candidates[i].1));
                }
            }
            let last = candidates.last().expect("candidates no vacío");
            Some((last.0, last.1))
        } else {
            // Determinista: primer máximo en orden de recorrido (como max()
            // de Python).
            let best = candidates
                .iter()
                .fold(None::<&(Pos, f64, f64)>, |acc, c| match acc {
                    Some(b) if c.2 <= b.2 => acc,
                    _ => Some(c),
                })
                .expect("candidates no vacío");
            Some((best.0, best.1))
        }
    }

    /// Selección `Coastal`: vecino de menor elevación absoluta (determinista);
    /// si ninguno desciende y la velocidad supera `min_velocity_for_flat`,
    /// flat-fallback al vecino casi-plano de menor diferencia de cota.
    fn search_min_elev(
        &self,
        model: &DebrisFlowModel,
        radius: i64,
        current_elev: f64,
    ) -> Option<(Pos, f64)> {
        let (w, h) = (
            model.layers.dem.width() as i64,
            model.layers.dem.height() as i64,
        );
        let elev_at = |dx: i64, dy: i64| -> Option<(Pos, f64, f64)> {
            let (nx, ny) = (self.pos.x as i64 + dx, self.pos.y as i64 + dy);
            if nx < 0 || ny < 0 || nx >= w || ny >= h {
                return None;
            }
            let npos = Pos::new(nx as usize, ny as usize);
            let elev = f64::from(model.layers.dem[npos]);
            if elev.is_nan() {
                return None;
            }
            let dist_m = ((dx * dx + dy * dy) as f64).sqrt() * model.pixel_size;
            Some((npos, elev, (current_elev - elev) / dist_m))
        };

        // Vecino de menor elevación con pendiente descendente.
        let mut best_elev = current_elev;
        let mut best: Option<(Pos, f64)> = None;
        for dx in -radius..=radius {
            for dy in -radius..=radius {
                if dx == 0 && dy == 0 {
                    continue;
                }
                if let Some((npos, elev, slope)) = elev_at(dx, dy)
                    && slope > 0.0
                    && elev < best_elev
                {
                    best_elev = elev;
                    best = Some((npos, slope));
                }
            }
        }
        if best.is_some() {
            return best;
        }

        // Flat-fallback: solo con suficiente inercia.
        if self.velocity > model.params.min_velocity_for_flat {
            let mut best_diff = f64::INFINITY;
            let mut bestf: Option<(Pos, f64)> = None;
            for dx in -radius..=radius {
                for dy in -radius..=radius {
                    if dx == 0 && dy == 0 {
                        continue;
                    }
                    if let Some((npos, elev, slope)) = elev_at(dx, dy)
                        && slope >= -0.05
                    {
                        let diff = (elev - current_elev).abs();
                        if diff < best_diff {
                            best_diff = diff;
                            bestf = Some((npos, slope));
                        }
                    }
                }
            }
            return bestf;
        }
        None
    }

    fn update_velocity(&self, slope: f64, p: &Params) -> f64 {
        // Reología de Voellmy (física enriquecida): fricción de Coulomb +
        // término turbulento. a = g·(slope − μ) − g·v²/ξ.
        if let Some(e) = &p.enhanced
            && e.use_voellmy
        {
            if slope <= 0.0 {
                return self.velocity * 0.95;
            }
            let accel =
                GRAVITY * (slope - e.voellmy_mu) - GRAVITY * self.velocity.powi(2) / e.voellmy_xi;
            return (self.velocity + accel).clamp(0.01, 600.0);
        }
        match p.physics {
            Physics::Copiapo => {
                let v = if slope > p.critical_slope {
                    self.velocity
                        + GRAVITY
                            * slope
                            * (1.0 - p.friction_coefficient)
                            * p.slope_acceleration_factor
                } else {
                    self.velocity * 0.95
                };
                v.clamp(p.min_velocity, p.max_velocity)
            }
            Physics::Coastal => {
                if slope <= 0.0 {
                    return self.velocity * 0.95;
                }
                // Aceleración gravitatoria menos drag cuadrático (dt = 1).
                let accel = GRAVITY * slope - p.friction_coefficient * self.velocity.powi(2);
                (self.velocity + accel).clamp(0.01, 600.0)
            }
        }
    }
}

/// Corre una simulación completa con `params`/`seed` y evalúa el footprint
/// contra el ground truth. Encapsula el ciclo crear→simular→evaluar para
/// que la calibración (que comparte `layers` vía `Arc`) lo invoque en bucle.
pub fn run_and_score(
    layers: &Arc<Layers>,
    ground_truth: &Grid2D<f32>,
    window: Window,
    pixel_size: f64,
    params: Params,
    seed: u64,
    steps: u64,
) -> Metrics {
    let model = DebrisFlowModel::new(Arc::clone(layers), params, pixel_size, seed);
    let mut sim = Simulation::new(model, seed).with_schedule(Schedule::new(Activation::Ordered));
    sim.run(steps);
    evaluate(&sim.model.footprint, ground_truth, window, pixel_size)
}

/// Métricas de validación espacial contra el ground truth, sobre la ventana
/// del bbox (idéntico al script de calibración Python).
#[derive(Debug, Clone, Copy)]
pub struct Metrics {
    pub tp: u64,
    pub fp: u64,
    pub r#fn: u64,
    pub iou: f64,
    pub precision: f64,
    pub recall: f64,
    pub f1: f64,
    pub area_pred_km2: f64,
    pub area_gt_km2: f64,
}

/// Como [`evaluate`] pero contando solo las celdas dentro de `bbox` (> 0) —
/// el dominio de evaluación urbano de Chañaral, donde el ground truth y el
/// bounding box son rásters separados (`AFFECTED = area & bbox` en el
/// original).
pub fn evaluate_masked(
    footprint: &Grid2D<bool>,
    ground_truth: &Grid2D<f32>,
    bbox: &Grid2D<f32>,
    window: Window,
    pixel_size: f64,
) -> Metrics {
    let (mut tp, mut fp, mut fneg, mut gt_total) = (0u64, 0u64, 0u64, 0u64);
    for y in window.row_start..window.row_end {
        for x in window.col_start..window.col_end {
            let pos = Pos::new(x, y);
            if bbox[pos] <= 0.0 {
                continue;
            }
            let pred = footprint[pos];
            let truth = ground_truth[pos] > 0.0;
            match (pred, truth) {
                (true, true) => tp += 1,
                (true, false) => fp += 1,
                (false, true) => fneg += 1,
                (false, false) => {}
            }
            if truth {
                gt_total += 1;
            }
        }
    }
    metrics_from_counts(tp, fp, fneg, gt_total, pixel_size)
}

pub fn evaluate(
    footprint: &Grid2D<bool>,
    ground_truth: &Grid2D<f32>,
    window: Window,
    pixel_size: f64,
) -> Metrics {
    let (mut tp, mut fp, mut fneg, mut gt_total) = (0u64, 0u64, 0u64, 0u64);
    for y in window.row_start..window.row_end {
        for x in window.col_start..window.col_end {
            let pos = Pos::new(x, y);
            let pred = footprint[pos];
            let truth = ground_truth[pos] > 0.0;
            match (pred, truth) {
                (true, true) => tp += 1,
                (true, false) => fp += 1,
                (false, true) => fneg += 1,
                (false, false) => {}
            }
            if truth {
                gt_total += 1;
            }
        }
    }
    metrics_from_counts(tp, fp, fneg, gt_total, pixel_size)
}

/// Construye [`Metrics`] (IoU, precision, recall, F1, áreas) desde los conteos.
fn metrics_from_counts(tp: u64, fp: u64, fneg: u64, gt_total: u64, pixel_size: f64) -> Metrics {
    let union = tp + fp + fneg;
    let iou = if union > 0 {
        tp as f64 / union as f64
    } else {
        0.0
    };
    let precision = if tp + fp > 0 {
        tp as f64 / (tp + fp) as f64
    } else {
        0.0
    };
    let recall = if tp + fneg > 0 {
        tp as f64 / (tp + fneg) as f64
    } else {
        0.0
    };
    let f1 = if precision + recall > 0.0 {
        2.0 * precision * recall / (precision + recall)
    } else {
        0.0
    };
    let km2 = pixel_size * pixel_size / 1.0e6;
    Metrics {
        tp,
        fp,
        r#fn: fneg,
        iou,
        precision,
        recall,
        f1,
        area_pred_km2: (tp + fp) as f64 * km2,
        area_gt_km2: gt_total as f64 * km2,
    }
}
