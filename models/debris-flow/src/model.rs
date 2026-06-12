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

use swarm_core::prelude::*;

use crate::raster::Window;

/// Parámetros del modelo. `Default` = calibración Optuna-TPE con temperatura
/// (Copiapó, `best_params_optuna_withT.json`, IoU de referencia 0.1344).
#[derive(Debug, Clone)]
pub struct Params {
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
}

impl Default for Params {
    fn default() -> Self {
        Self {
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
        }
    }
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
}

#[derive(Debug)]
pub enum DebrisAgent {
    Raindrop(Raindrop),
    Flow(Flow),
}

pub struct DebrisFlowModel {
    pub agents: AgentSet<DebrisAgent>,
    pub layers: Layers,
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
    pub fn new(layers: Layers, params: Params, pixel_size: f64, seed: u64) -> Self {
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

        // Agentes de lluvia en celdas válidas distintas, al azar.
        let mut rng = rng_from_seed(seed ^ 0xDEB1_5F10);
        let mut usadas = std::collections::HashSet::new();
        while usadas.len() < model.params.n_rain_agents {
            let pos = Pos::new(rng.random_range(0..width), rng.random_range(0..height));
            if !model.layers.dem[pos].is_nan() && usadas.insert(pos) {
                model.agents.insert(DebrisAgent::Raindrop(Raindrop { pos }));
            }
        }
        model
    }

    fn mark_footprint(&mut self, pos: Pos) {
        let (w, h) = (
            self.footprint.width() as i64,
            self.footprint.height() as i64,
        );
        for &(dx, dy) in &self.disc {
            let (x, y) = (pos.x as i64 + dx, pos.y as i64 + dy);
            if x >= 0 && y >= 0 && x < w && y < h {
                self.footprint[Pos::new(x as usize, y as usize)] = true;
            }
        }
    }

    fn spawn_flow(&mut self, pos: Pos, volume: f64, velocity: f64, has_spread: bool) {
        self.agents.insert(DebrisAgent::Flow(Flow {
            pos,
            volume,
            velocity,
            has_spread,
        }));
        self.flows_created += 1;
        self.mark_footprint(pos);
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
                self.pos = next;
                self.velocity = self.update_velocity(slope, &model.params);
                self.volume *= if slope > 0.01 {
                    model.params.volume_decay_slope
                } else {
                    model.params.volume_decay_flat
                };
                model.mark_footprint(next);
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

    fn update_velocity(&self, slope: f64, p: &Params) -> f64 {
        let v = if slope > p.critical_slope {
            self.velocity
                + GRAVITY * slope * (1.0 - p.friction_coefficient) * p.slope_acceleration_factor
        } else {
            self.velocity * 0.95
        };
        v.clamp(p.min_velocity, p.max_velocity)
    }
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
