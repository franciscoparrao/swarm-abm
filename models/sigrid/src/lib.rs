//! **SIGRID** — modelo ABM de depredación de ovejas y perros guardianes
//! (Isla Riesco, Patagonia) portado de Python/Mesa a swarm-abm.
//!
//! Port enfocado en reproducir la métrica de salida del estudio —el *loss rate*
//! anual de ovejas (patrón P_C1, target empírico 3.73%)— en la configuración con
//! la que se corrió el análisis de sensibilidad: sin estacionalidad, sin puma,
//! fear landscape opcional (off por defecto). El objetivo es **paridad
//! distribucional** con el modelo Mesa, no bit a bit (el RNG difiere), igual que
//! el port de debris-flow.
//!
//! Espacio: [`ContinuousSpace`] (índice de vecindad por radio) + un raster de
//! calidad/cobertura de vegetación (overlay tipo grilla). Activación: aleatoria
//! secuencial con mutación inmediata (un depredador mata a la presa en el acto),
//! reproducible bit a bit vía el RNG por-agente del motor.
//!
//! Constantes y fórmulas tomadas del modelo Mesa original
//! (`simulacion_agentes/`, Yusti et al., Isla Riesco 2017-2018).

use swarm_abm::prelude::*;

// ---------------------------------------------------------------------------
// Constantes del modelo (literales del código Mesa)
// ---------------------------------------------------------------------------

const SHEEP_SPEED: f64 = 50.0;
const SHEEP_FLEE_SPEED: f64 = 100.0;
const SHEEP_PERCEPTION_RADIUS: f64 = 100.0;
const SHEEP_ADULT_VULN: f64 = 0.4;
const LAMB_SPEED: f64 = 35.0;
const LAMB_FLEE_SPEED: f64 = 60.0;
const LAMB_PERCEPTION_RADIUS: f64 = 50.0;
const LAMB_FEAR_DECAY: f64 = 0.05;
const LAMB_MATURATION_DAYS: f64 = 120.0;
const LAMB_VULN: f64 = 0.85;
const DOG_PROXIMITY_VIGILANCE: f64 = 100.0;

const FOX_SPEED_WALK: f64 = 500.0;
const FOX_DETECTION_RADIUS: f64 = 300.0;
const FOX_TERRITORY_RADIUS: f64 = 6135.0;
const CHILLA_TERRITORY_RADIUS: f64 = 4295.0;
const HUNGER_THRESHOLD: f64 = 0.3;
const BASE_RISK_AVERSION: f64 = 0.6;
const FOX_ATTACK_RADIUS: f64 = 50.0;

// Patrón circadiano del zorro (Arenas-Rodriguez 2024 + Yusti C18).
const FOX_ACT_PEAK_NO_DOG: f64 = 0.0;
const FOX_ACT_AMP_NO_DOG: f64 = 0.90;
const FOX_ACT_SIGMA_NO_DOG: f64 = 4.5;
const FOX_ACT_BASE_NO_DOG: f64 = 0.05;
const FOX_ACT_PEAK_WITH_DOG: f64 = 1.5;
const FOX_ACT_AMP_WITH_DOG: f64 = 0.45;
const FOX_ACT_SIGMA_WITH_DOG: f64 = 3.0;
const FOX_ACT_BASE_WITH_DOG: f64 = 0.30;

const DOG_SPEED_PATROL: f64 = 300.0;
const DOG_SPEED_CHASE: f64 = 3000.0;
const DOG_DETECTION_RADIUS: f64 = 400.0;
const DOG_CHASE_RADIUS: f64 = 200.0;
const DOG_PATROL_RADIUS: f64 = 250.0;
const DOG_DETER_RADIUS: f64 = 50.0;
/// Radio de evitación de área: un depredador que percibe un perro dentro de
/// este radio aborta la cacería si el riesgo supera su aversión. Es el canal
/// escalable de disuasión (Mesa lo hace vía memoria de zonas peligrosas).
const DOG_AVOID_RADIUS: f64 = 500.0;
/// Vigencia (horas) y radio de influencia de una zona peligrosa recordada.
const DANGER_TTL: u64 = 168;
const DANGER_RADIUS: f64 = 400.0;

const HARE_SPEED_NORMAL: f64 = 100.0;
const HARE_SPEED_FLEE: f64 = 800.0;
const HARE_PERCEPTION_RADIUS: f64 = 80.0;
const HARE_MATURITY_AGE_H: f64 = 60.0 * 24.0;
const HARE_VULN_JUV: f64 = 0.9;
const HARE_VULN_MATURE: f64 = 0.6;

// Predación (fórmula acotada, fox/chilla).
const FOX_P_MIN: f64 = 0.02;
const FOX_P_MAX: f64 = 0.35;

const CELL_SIZE: f64 = 30.0;

// ---------------------------------------------------------------------------
// Parámetros (los 6 del muestreo Sobol + fijos)
// ---------------------------------------------------------------------------

/// Parámetros del modelo. Los seis primeros son los ejes del análisis de
/// sensibilidad; el resto se mantiene fijo (mirror de `run_paper_experiments.py`).
#[derive(Debug, Clone, Copy)]
pub struct Params {
    pub width: f64,
    pub height: f64,
    /// Ovejas por hectárea (Yusti C22: 0.96-1.5).
    pub sheep_density: f64,
    /// Zorros por km² (Yusti C23). Fijo en el screening.
    pub fox_density: f64,
    /// Efectividad de cacería del zorro (Yusti C11: 0.08/0.14/0.26).
    pub fox_predation_effectiveness: f64,
    /// Número de perros guardianes (intervención).
    pub n_dogs: usize,
    /// Liebres por hectárea (Yusti C26).
    pub hare_density: f64,
    /// Chillas por km² (Yusti C23).
    pub chilla_density: f64,
    /// Fracción de corderos en el rebaño.
    pub lamb_proportion: f64,
    /// Paisaje del miedo perro→zorro (off por defecto, como el screening Mesa).
    pub use_fear: bool,
}

impl Default for Params {
    fn default() -> Self {
        // Mirror del área base de las estancias modeladas (4 km² = 400 ha).
        Self {
            width: 2000.0,
            height: 2000.0,
            sheep_density: 0.96,
            fox_density: 8.4,
            fox_predation_effectiveness: 0.14,
            n_dogs: 0,
            hare_density: 0.0,
            chilla_density: 0.0,
            lamb_proportion: 0.2,
            use_fear: false,
        }
    }
}

// ---------------------------------------------------------------------------
// Agentes
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Species {
    Sheep,
    Fox,
    Dog,
    Hare,
}

/// Un animal del modelo. Estructura única con `species` (mirror del patrón de
/// herencia de Mesa: base compartida + atributos por especie).
#[derive(Debug, Clone)]
pub struct Animal {
    pub species: Species,
    pub pos: Vec2,
    pub alive: bool,
    pub energy: f64,
    pub age_days: f64,
    pub fear: f64,
    // Oveja / cordero
    pub is_lamb: bool,
    pub vulnerability: f64,
    pub mother: Option<AgentId>,
    // Zorro / chilla
    pub is_chilla: bool,
    pub hunger: f64,
    pub risk_aversion: f64,
    pub predation_eff: f64,
    pub territory: Vec2,
    pub territory_radius: f64,
    // Perro
    pub patrol_angle: f64,
    pub patrol_center: Vec2,
    // Liebre
    pub mature: bool,
    /// Memoria de zonas peligrosas (zorro/chilla): lugares donde fue disuadido
    /// por un perro, como `(posición, paso)`. Decae a 168 h. Es el mecanismo que
    /// hace que la disuasión del perro se ACUMULE sobre el área del rebaño.
    pub danger_zones: Vec<(Vec2, u64)>,
    /// Presa que el zorro tiene en la mira tras aproximarse. Si hay un perro
    /// cerca, el ataque se DIFIERE un tick (acecho): el depredador queda
    /// expuesto y el perro tiene un turno para interceptarlo antes de la muerte.
    pub stalk_target: Option<AgentId>,
}

impl Animal {
    fn blank(species: Species, pos: Vec2) -> Self {
        Self {
            species,
            pos,
            alive: true,
            energy: 100.0,
            age_days: 0.0,
            fear: 0.0,
            is_lamb: false,
            vulnerability: 0.0,
            mother: None,
            is_chilla: false,
            hunger: 0.0,
            risk_aversion: 0.0,
            predation_eff: 0.0,
            territory: pos,
            territory_radius: 0.0,
            patrol_angle: 0.0,
            patrol_center: pos,
            mature: true,
            danger_zones: Vec::new(),
            stalk_target: None,
        }
    }
}

/// Instantánea ligera por punto para las consultas de vecindad (el espacio se
/// reconstruye al inicio de cada paso, estilo boids).
#[derive(Debug, Clone, Copy)]
pub struct Snap {
    pub id: AgentId,
    pub species: Species,
    pub alive: bool,
    pub is_lamb: bool,
    pub vulnerability: f64,
    pub energy: f64,
    pub mother: Option<AgentId>,
}

// ---------------------------------------------------------------------------
// Raster de vegetación (overlay tipo grilla sobre el espacio continuo)
// ---------------------------------------------------------------------------

/// Raster sintético de una capa escalar en `[0,1]`, resolución `CELL_SIZE`.
#[derive(Debug, Clone)]
pub struct Raster {
    cols: usize,
    rows: usize,
    height: f64,
    data: Vec<f64>,
}

impl Raster {
    fn get(&self, p: Vec2) -> f64 {
        let col = (p.x / CELL_SIZE).floor().clamp(0.0, (self.cols - 1) as f64) as usize;
        let row = ((self.height - p.y) / CELL_SIZE)
            .floor()
            .clamp(0.0, (self.rows - 1) as f64) as usize;
        self.data[row * self.cols + col]
    }
}

/// Genera los rasters sintéticos de calidad y cobertura de vegetación
/// (versión simplificada y determinista de `landscape.generate_synthetic`).
fn build_rasters(width: f64, height: f64, rng: &mut SimRng) -> (Raster, Raster) {
    let cols = (width / CELL_SIZE).ceil() as usize;
    let rows = (height / CELL_SIZE).ceil() as usize;
    let mut quality = vec![0.0; cols * rows];
    let mut cover = vec![0.0; cols * rows];
    for r in 0..rows {
        for c in 0..cols {
            let x = c as f64 / cols as f64;
            let y = r as f64 / rows as f64;
            // Elevación sintética (campo suave multi-frecuencia, normalizado).
            let elev = 0.5
                + 0.5
                    * ((x * 6.0).sin() * (y * 6.0).cos()
                        + 0.5 * (x * 12.0).sin() * (y * 12.0).cos())
                    / 1.5;
            // Cobertura: más en zonas bajas, con ruido.
            let cov = (0.7 - elev * 0.4 + rng.random_range(-0.1..0.1)).clamp(0.0, 1.0);
            // Calidad: mejor en cobertura intermedia/baja (pasto accesible).
            let ndvi = ((1.0 - cov) * 0.5 + (1.0 - elev) * 0.5).clamp(0.0, 1.0);
            let qual = (ndvi * (1.0 + rng.random_range(-0.1..0.1))).clamp(0.0, 1.0);
            cover[r * cols + c] = cov;
            quality[r * cols + c] = qual;
        }
    }
    (
        Raster {
            cols,
            rows,
            height,
            data: quality,
        },
        Raster {
            cols,
            rows,
            height,
            data: cover,
        },
    )
}

// ---------------------------------------------------------------------------
// Modelo
// ---------------------------------------------------------------------------

pub struct SigridModel {
    agents: AgentSet<Animal>,
    space: ContinuousSpace<Snap>,
    veg_quality: Raster,
    veg_cover: Raster,
    /// Posiciones de perros vivos (refrescadas en `before_step`). Como son
    /// pocos, la consulta de "perro más cercano" es un loop directo, no espacial.
    dog_positions: Vec<Vec2>,
    params: Params,
    current_hour: u32,
    step_count: u64,
    pub sheep_killed: usize,
    pub sheep_killed_by_fox: usize,
    pub sheep_killed_by_chilla: usize,
    pub hares_killed: usize,
    pub predation_attempts: usize,
    n_sheep_initial: usize,
}

impl SigridModel {
    /// Loss rate acumulado (% de ovejas iniciales muertas) — el patrón P_C1.
    pub fn loss_rate_pct(&self) -> f64 {
        self.sheep_killed as f64 / self.n_sheep_initial.max(1) as f64 * 100.0
    }

    fn n_alive(&self, species: Species) -> usize {
        self.agents
            .iter()
            .filter(|(_, a)| a.alive && a.species == species)
            .count()
    }

    /// Distancia al perro vivo más cercano dentro de `max`; `None` si ninguno.
    fn nearest_dog_dist(&self, pos: Vec2, max: f64) -> Option<f64> {
        let mut best: Option<f64> = None;
        for &d in &self.dog_positions {
            let dist = (pos - d).length();
            if dist <= max {
                best = Some(best.map_or(dist, |b: f64| b.min(dist)));
            }
        }
        best
    }
}

impl Model for SigridModel {
    type Agent = Animal;

    fn agents(&self) -> &AgentSet<Animal> {
        &self.agents
    }
    fn agents_mut(&mut self) -> &mut AgentSet<Animal> {
        &mut self.agents
    }

    /// Pre-paso: hora del día, y reconstrucción del índice espacial con el
    /// estado actual (instantánea consistente para las consultas del tick).
    fn before_step(&mut self, _rng: &mut SimRng) {
        self.current_hour = (self.step_count % 24) as u32;
        self.step_count += 1;
        // Reconstruye el índice espacial con el estado actual. A esta escala
        // (~10³ agentes) reconstruir es más barato que mantener PointId por
        // agente, y evita estado espacial obsoleto entre pasos.
        let Self {
            agents,
            space,
            params,
            dog_positions,
            ..
        } = self;
        *space = ContinuousSpace::new(params.width, params.height, 60.0);
        dog_positions.clear();
        for (id, a) in agents.iter() {
            let snap = Snap {
                id,
                species: a.species,
                alive: a.alive,
                is_lamb: a.is_lamb,
                vulnerability: a.vulnerability,
                energy: a.energy,
                mother: a.mother,
            };
            space.add(a.pos, snap);
            if a.alive && a.species == Species::Dog {
                dog_positions.push(a.pos);
            }
        }
        space.reindex();
    }

    fn finished(&self) -> bool {
        self.n_alive(Species::Sheep) == 0
    }
}

// ---------------------------------------------------------------------------
// Utilidades
// ---------------------------------------------------------------------------

/// Ruido gaussiano N(0, std) por Box-Muller (rand sin features extra).
fn normal(rng: &mut SimRng, std: f64) -> f64 {
    let u1: f64 = rng.random_range(1e-12..1.0);
    let u2: f64 = rng.random::<f64>();
    (-2.0 * u1.ln()).sqrt() * (std::f64::consts::TAU * u2).cos() * std
}

fn clamp_pos(p: Vec2, w: f64, h: f64) -> Vec2 {
    Vec2::new(p.x.clamp(0.0, w), p.y.clamp(0.0, h))
}

/// Dirección unitaria hacia el vecino de mayor `quality` muestreando 8 offsets.
fn food_direction(p: Vec2, raster: &Raster) -> Vec2 {
    let mut best = raster.get(p);
    let mut dir = Vec2::ZERO;
    for k in 0..8 {
        let a = std::f64::consts::TAU * k as f64 / 8.0;
        let off = Vec2::new(a.cos(), a.sin()) * 50.0;
        let q = raster.get(p + off);
        if q > best {
            best = q;
            dir = Vec2::new(a.cos(), a.sin());
        }
    }
    dir
}

// ---------------------------------------------------------------------------
// Comportamiento por agente
// ---------------------------------------------------------------------------

impl Agent for Animal {
    type Model = SigridModel;

    fn step(&mut self, id: AgentId, model: &mut SigridModel, rng: &mut SimRng) {
        if !self.alive {
            return;
        }
        match self.species {
            Species::Sheep => self.step_sheep(model, rng),
            Species::Fox => self.step_fox(id, model, rng),
            Species::Dog => self.step_dog(model, rng),
            Species::Hare => self.step_hare(model, rng),
        }
    }
}

impl Animal {
    // ---- Oveja / cordero ------------------------------------------------
    fn step_sheep(&mut self, model: &mut SigridModel, rng: &mut SimRng) {
        let decay = if self.is_lamb { LAMB_FEAR_DECAY } else { 0.1 };
        self.fear = (self.fear - decay).max(0.0);
        self.age_days += 1.0 / 24.0;

        if self.fear > 0.7 {
            self.sheep_flee(model, rng);
        } else if self.choose_forage(model, rng) {
            self.graze_and_move(model, rng);
        }

        // Maduración del cordero.
        if self.is_lamb && self.age_days > LAMB_MATURATION_DAYS {
            self.is_lamb = false;
            self.vulnerability = SHEEP_ADULT_VULN;
        }

        // Energía a partir de la calidad de pasto, menos estrés por miedo.
        let q = model.veg_quality.get(self.pos);
        let gain = if self.is_lamb { q } else { q * 2.0 } - 1.0;
        self.energy = (self.energy + gain - self.fear * 2.0).min(100.0);
    }

    /// Elige si la actividad del tick implica moverse (forage/movement) según
    /// el time budget, que cambia si hay un perro a <100 m (Yusti C9).
    fn choose_forage(&self, model: &SigridModel, rng: &mut SimRng) -> bool {
        let dog_near = model
            .nearest_dog_dist(self.pos, DOG_PROXIMITY_VIGILANCE)
            .is_some();
        // P(moverse) = forage + movement.
        let p_move = if dog_near { 0.60 + 0.02 } else { 0.76 + 0.04 };
        rng.random::<f64>() < p_move
    }

    fn graze_and_move(&mut self, model: &SigridModel, rng: &mut SimRng) {
        let v_food = food_direction(self.pos, &model.veg_quality);
        // Evitar cobertura densa (riesgo): alejarse del offset de mayor cover.
        let mut v_risk = Vec2::ZERO;
        let mut worst = model.veg_cover.get(self.pos);
        for k in 0..8 {
            let a = std::f64::consts::TAU * k as f64 / 8.0;
            let off = Vec2::new(a.cos(), a.sin()) * 50.0;
            let cov = model.veg_cover.get(self.pos + off);
            if cov > worst {
                worst = cov;
                v_risk = Vec2::new(-a.cos(), -a.sin());
            }
        }
        // Cohesión hacia el centroide de ovejas vecinas y atracción al perro.
        let (mut center, mut n) = (Vec2::ZERO, 0.0);
        let mut v_dog = Vec2::ZERO;
        let (mut dog_best, mut dog_pos) = (f64::MAX, Vec2::ZERO);
        model.space.for_each_within(
            self.pos,
            SHEEP_PERCEPTION_RADIUS.max(500.0),
            |_, npos, snap, dist| {
                if !snap.alive {
                    return;
                }
                if snap.species == Species::Sheep && dist <= SHEEP_PERCEPTION_RADIUS {
                    center = center + npos;
                    n += 1.0;
                }
                if snap.species == Species::Dog && dist < dog_best {
                    dog_best = dist;
                    dog_pos = npos;
                }
            },
        );
        let mut v_cohesion = Vec2::ZERO;
        if n > 0.0 {
            v_cohesion = (center * (1.0 / n) - self.pos).normalize_or_zero();
        }
        if dog_best < 500.0 {
            v_dog = (dog_pos - self.pos).normalize_or_zero();
        }

        let dir = v_food * 0.3
            + v_risk * 0.2
            + v_cohesion * 0.2
            + v_dog * 0.3
            + Vec2::new(normal(rng, 0.1), normal(rng, 0.1));
        let dir = dir.normalize_or_zero();
        let speed = if self.is_lamb {
            LAMB_SPEED
        } else {
            SHEEP_SPEED
        };
        self.pos = clamp_pos(
            self.pos + dir * speed,
            model.params.width,
            model.params.height,
        );
    }

    fn sheep_flee(&mut self, model: &SigridModel, rng: &mut SimRng) {
        let radius = if self.is_lamb {
            LAMB_PERCEPTION_RADIUS
        } else {
            SHEEP_PERCEPTION_RADIUS
        };
        let (mut pred_best, mut pred_pos, mut found) = (f64::MAX, Vec2::ZERO, false);
        model
            .space
            .for_each_within(self.pos, radius, |_, npos, snap, dist| {
                if snap.alive && snap.species == Species::Fox && dist < pred_best {
                    pred_best = dist;
                    pred_pos = npos;
                    found = true;
                }
            });
        let speed = if self.is_lamb {
            LAMB_FLEE_SPEED
        } else {
            SHEEP_FLEE_SPEED
        };
        let dir = if found {
            (self.pos - pred_pos).normalize_or_zero()
        } else {
            let a: f64 = rng.random_range(0.0..std::f64::consts::TAU);
            Vec2::new(a.cos(), a.sin())
        };
        self.pos = clamp_pos(
            self.pos + dir * speed,
            model.params.width,
            model.params.height,
        );
    }

    // ---- Zorro / chilla -------------------------------------------------
    fn step_fox(&mut self, _id: AgentId, model: &mut SigridModel, rng: &mut SimRng) {
        self.hunger = (self.hunger + 0.01).min(1.0);
        // Olvido de zonas peligrosas vencidas (memoria de 168 h).
        let now = model.step_count;
        self.danger_zones
            .retain(|&(_, s)| now.saturating_sub(s) < DANGER_TTL);
        if !self.fox_active(model, rng) {
            return; // descanso fuera del periodo activo
        }
        if self.hunger < HUNGER_THRESHOLD {
            // Patrullar el territorio (movimiento aleatorio corto).
            let a: f64 = rng.random_range(0.0..std::f64::consts::TAU);
            let d: f64 = rng.random_range(50.0..200.0);
            self.pos = clamp_pos(
                self.pos + Vec2::new(a.cos(), a.sin()) * d,
                model.params.width,
                model.params.height,
            );
            return;
        }
        self.hunt(model, rng);
    }

    /// Periodo activo: curva gaussiana circular sobre la hora, desplazada y
    /// aplanada si el zorro percibe un perro en su territorio.
    fn fox_active(&self, model: &SigridModel, rng: &mut SimRng) -> bool {
        let perceives_dog = model
            .nearest_dog_dist(self.pos, self.territory_radius)
            .is_some();
        let (peak, amp, sigma, base) = if perceives_dog {
            (
                FOX_ACT_PEAK_WITH_DOG,
                FOX_ACT_AMP_WITH_DOG,
                FOX_ACT_SIGMA_WITH_DOG,
                FOX_ACT_BASE_WITH_DOG,
            )
        } else {
            (
                FOX_ACT_PEAK_NO_DOG,
                FOX_ACT_AMP_NO_DOG,
                FOX_ACT_SIGMA_NO_DOG,
                FOX_ACT_BASE_NO_DOG,
            )
        };
        let h = model.current_hour as f64;
        let raw = (h - peak).abs();
        let d = raw.min(24.0 - raw);
        let level = (base + amp * (-(d * d) / (2.0 * sigma * sigma)).exp()).min(1.0);
        rng.random::<f64>() < level
    }

    fn hunt(&mut self, model: &mut SigridModel, rng: &mut SimRng) {
        // Evitación de área: el canal dominante de disuasión del perro. El
        // riesgo percibido suma (a) la proximidad de un perro actual y (b) la
        // memoria de zonas donde el depredador fue disuadido antes (decae a
        // 168 h). Con varios perros la memoria SATURA el área del rebaño y la
        // mortalidad cae a casi cero, como en Mesa. La chilla percibe 1.8x.
        // Si el riesgo supera la aversión, aborta y se aleja de la fuente.
        {
            let now = model.step_count;
            let mult = if self.is_chilla { 1.8 } else { 1.0 };
            let mut risk = 0.0;
            let mut wpos = Vec2::ZERO;
            let mut wsum = 0.0;
            // (a) riesgo SUMADO sobre los perros dentro del radio de evitación
            for &d in &model.dog_positions {
                let dd = (self.pos - d).length();
                if dd < DOG_AVOID_RADIUS {
                    let r = 1.0 - dd / DOG_AVOID_RADIUS;
                    risk += r;
                    wpos = wpos + d * r;
                    wsum += r;
                }
            }
            // (b) memoria de zonas peligrosas cercanas
            for &(zpos, zstep) in &self.danger_zones {
                let dist = (self.pos - zpos).length();
                if dist < DANGER_RADIUS {
                    let age = now.saturating_sub(zstep) as f64 / DANGER_TTL as f64;
                    let r = (1.0 - age) * (1.0 - dist / DANGER_RADIUS);
                    risk += r;
                    wpos = wpos + zpos * r;
                    wsum += r;
                }
            }
            if risk * mult > self.risk_aversion {
                // Evaluación de riesgo proactiva (como Mesa): marca el lugar como
                // peligroso ANTES de atacar y se aleja. Así la disuasión acumula
                // sobre el área aunque el zorro mate en un tick.
                self.danger_zones.push((self.pos, now));
                if self.danger_zones.len() > 64 {
                    self.danger_zones.remove(0);
                }
                let from = if wsum > 0.0 {
                    wpos * (1.0 / wsum)
                } else {
                    self.pos
                };
                let away = (self.pos - from).normalize_or_zero();
                self.pos = clamp_pos(
                    self.pos + away * FOX_SPEED_WALK,
                    model.params.width,
                    model.params.height,
                );
                self.stalk_target = None; // abandona el acecho al huir
                return;
            }
        }
        // Ataque comprometido: si venía acechando una presa y la tiene a tiro,
        // ataca ahora (sobrevivió al turno de exposición sin ser interceptado).
        if let Some(tid) = self.stalk_target.take()
            && let Some(t) = model.agents.get(tid)
            && t.alive
            && (self.pos - t.pos).length() < FOX_ATTACK_RADIUS
        {
            self.attempt_predation(tid, model, rng);
            return;
        }
        // Detectar presas vivas (oveja/liebre) en el radio de detección.
        let mut prey: Vec<(AgentId, Species, bool, f64)> = Vec::new(); // id, especie, is_lamb, vuln
        let mut hares_nearby = 0;
        model
            .space
            .for_each_within(self.pos, FOX_DETECTION_RADIUS, |_, _, snap, _| {
                if !snap.alive {
                    return;
                }
                match snap.species {
                    Species::Sheep => {
                        prey.push((snap.id, Species::Sheep, snap.is_lamb, snap.vulnerability))
                    }
                    Species::Hare => {
                        hares_nearby += 1;
                        prey.push((snap.id, Species::Hare, false, snap.vulnerability));
                    }
                    _ => {}
                }
            });
        if prey.is_empty() {
            // Búsqueda: desplazamiento en dirección aleatoria.
            let a: f64 = rng.random_range(0.0..std::f64::consts::TAU);
            self.pos = clamp_pos(
                self.pos + Vec2::new(a.cos(), a.sin()) * FOX_SPEED_WALK,
                model.params.width,
                model.params.height,
            );
            return;
        }

        // Seleccionar objetivo por score de vulnerabilidad (prey switching:
        // con >=2 liebres cerca, baja el score de las ovejas).
        let switch = hares_nearby >= 2;
        let mut best_id = prey[0].0;
        let mut best_score = f64::MIN;
        for &(pid, sp, is_lamb, vuln) in &prey {
            let Some(pa) = model.agents.get(pid) else {
                continue;
            };
            if !pa.alive {
                continue;
            }
            let mut score = vuln;
            if sp == Species::Sheep && switch {
                score -= 0.3;
            }
            if is_lamb {
                score += 0.2;
            }
            if score > best_score {
                best_score = score;
                best_id = pid;
            }
        }

        // Aproximarse; si queda a <50 m, intentar la predación.
        let tpos = match model.agents.get(best_id) {
            Some(t) if t.alive => t.pos,
            _ => return,
        };
        // Avanzar hacia la presa sin sobrepasarla (paso = min(velocidad, dist)).
        let to = tpos - self.pos;
        let step = FOX_SPEED_WALK.min(to.length());
        self.pos = clamp_pos(
            self.pos + to.normalize_or_zero() * step,
            model.params.width,
            model.params.height,
        );
        if (self.pos - tpos).length() < FOX_ATTACK_RADIUS {
            // Si hay un perro en rango de detección, NO mata este tick: queda en
            // acecho expuesto y el perro tendrá un turno para interceptarlo. Sin
            // perro cerca, mata de inmediato (baseline sin perros intacta).
            let dog_near = model
                .dog_positions
                .iter()
                .any(|&d| (self.pos - d).length() < DOG_DETECTION_RADIUS);
            if dog_near {
                self.stalk_target = Some(best_id);
            } else {
                self.attempt_predation(best_id, model, rng);
            }
        }
    }

    fn attempt_predation(&mut self, prey_id: AgentId, model: &mut SigridModel, rng: &mut SimRng) {
        let p = self.predation_probability(prey_id, model);
        model.predation_attempts += 1;
        let success = rng.random::<f64>() < p;
        // Datos de la presa para clasificar la muerte.
        let (is_sheep, is_hare) = match model.agents.get(prey_id) {
            Some(a) => (a.species == Species::Sheep, a.species == Species::Hare),
            None => return,
        };
        if success {
            if let Some(prey) = model.agents.get_mut(prey_id) {
                prey.alive = false;
            }
            self.hunger = 0.0;
            if is_sheep {
                model.sheep_killed += 1;
                if self.is_chilla {
                    model.sheep_killed_by_chilla += 1;
                } else {
                    model.sheep_killed_by_fox += 1;
                }
            } else if is_hare {
                model.hares_killed += 1;
            }
        } else {
            if let Some(prey) = model.agents.get_mut(prey_id) {
                prey.fear = 1.0;
            }
            self.hunger = (self.hunger + 0.05).min(1.0);
        }
    }

    /// Probabilidad de éxito de la predación (fox.py:421-491): base
    /// efectividad×vulnerabilidad más modificadores ambientales, acotada.
    fn predation_probability(&self, prey_id: AgentId, model: &SigridModel) -> f64 {
        let Some(prey) = model.agents.get(prey_id) else {
            return 0.0;
        };
        let p_base = self.predation_eff * prey.vulnerability;
        let m_cover = 0.05 * model.veg_cover.get(self.pos);
        let m_dog = match model.nearest_dog_dist(self.pos, 500.0) {
            Some(dist) => -0.10 * (1.0 - dist / 500.0),
            None => 0.0,
        };
        // Tamaño de grupo alrededor de la presa.
        let mut nearby: f64 = 0.0;
        model
            .space
            .for_each_within(prey.pos, 50.0, |_, _, snap, _| {
                if snap.alive && snap.species == Species::Sheep {
                    nearby += 1.0;
                }
            });
        let m_group = -0.03 * (nearby / 10.0).min(1.0);
        let m_condition = 0.03 * (1.0 - prey.energy / 100.0);
        // Defensa materna si el cordero está pegado a la madre.
        let mut m_mother = 0.0;
        if prey.is_lamb
            && let Some(mid) = prey.mother
            && let Some(mother) = model.agents.get(mid)
            && mother.alive
        {
            let d = (prey.pos - mother.pos).length();
            if d < 20.0 {
                m_mother = -0.12 * (1.0 - d / 20.0);
            }
        }
        (p_base + m_cover + m_dog + m_group + m_condition + m_mother).clamp(FOX_P_MIN, FOX_P_MAX)
    }

    // ---- Perro guardián -------------------------------------------------
    fn step_dog(&mut self, model: &mut SigridModel, rng: &mut SimRng) {
        let _ = rng; // el perro es determinista (patrulla/persecución)
        // Detectar depredadores en el radio de detección.
        let (mut best, mut best_id, mut best_pos) = (f64::MAX, None, Vec2::ZERO);
        model
            .space
            .for_each_within(self.pos, DOG_DETECTION_RADIUS, |_, npos, snap, dist| {
                if snap.alive && snap.species == Species::Fox && dist < best {
                    best = dist;
                    best_id = Some(snap.id);
                    best_pos = npos;
                }
            });
        if let Some(fox_id) = best_id {
            // Perseguir al depredador sin sobrepasarlo.
            let to = best_pos - self.pos;
            let step = DOG_SPEED_CHASE.min(to.length());
            self.pos = clamp_pos(
                self.pos + to.normalize_or_zero() * step,
                model.params.width,
                model.params.height,
            );
            if best < DOG_CHASE_RADIUS && (self.pos - best_pos).length() < DOG_DETER_RADIUS {
                // Disuasión: el zorro queda con miedo, pierde apetito y RECUERDA
                // el lugar como zona peligrosa (memoria que decae a 168 h).
                let now = model.step_count;
                if let Some(fox) = model.agents.get_mut(fox_id) {
                    fox.fear = 1.0;
                    fox.hunger = 0.0; // ahuyentado: deja de cazar hasta volver a tener hambre
                    fox.stalk_target = None; // pierde la presa que acechaba
                    fox.danger_zones.push((best_pos, now));
                    if fox.danger_zones.len() > 64 {
                        fox.danger_zones.remove(0);
                    }
                }
            }
            return;
        }
        // Patrullaje circular alrededor del centroide del rebaño.
        let (mut center, mut n) = (Vec2::ZERO, 0.0);
        for (_, a) in model.agents.iter() {
            if a.alive && a.species == Species::Sheep {
                center = center + a.pos;
                n += 1.0;
            }
        }
        let flock = if n > 0.0 {
            center * (1.0 / n)
        } else {
            self.patrol_center
        };
        self.patrol_angle += 0.1;
        let target =
            flock + Vec2::new(self.patrol_angle.cos(), self.patrol_angle.sin()) * DOG_PATROL_RADIUS;
        let dir = (target - self.pos).normalize_or_zero();
        self.pos = clamp_pos(
            self.pos + dir * DOG_SPEED_PATROL,
            model.params.width,
            model.params.height,
        );
    }

    // ---- Liebre ---------------------------------------------------------
    fn step_hare(&mut self, model: &mut SigridModel, rng: &mut SimRng) {
        self.age_days += 1.0 / 24.0;
        if !self.mature && self.age_days * 24.0 >= HARE_MATURITY_AGE_H {
            self.mature = true;
            self.vulnerability = HARE_VULN_MATURE;
        }
        self.fear = (self.fear - 0.15).max(0.0);

        let (mut pred_best, mut pred_pos, mut found) = (f64::MAX, Vec2::ZERO, false);
        model
            .space
            .for_each_within(self.pos, HARE_PERCEPTION_RADIUS, |_, npos, snap, dist| {
                if snap.alive && snap.species == Species::Fox && dist < pred_best {
                    pred_best = dist;
                    pred_pos = npos;
                    found = true;
                }
            });
        if found || self.fear > 0.5 {
            let dir = if found {
                (self.pos - pred_pos).normalize_or_zero()
            } else {
                let a: f64 = rng.random_range(0.0..std::f64::consts::TAU);
                Vec2::new(a.cos(), a.sin())
            };
            self.pos = clamp_pos(
                self.pos + dir * HARE_SPEED_FLEE,
                model.params.width,
                model.params.height,
            );
        } else {
            // Forrajeo: paso hacia mejor cobertura intermedia.
            let dir = food_direction(self.pos, &model.veg_quality);
            self.pos = clamp_pos(
                self.pos + dir * HARE_SPEED_NORMAL,
                model.params.width,
                model.params.height,
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Construcción
// ---------------------------------------------------------------------------

/// Construye el modelo SIGRID a partir de los parámetros y una semilla.
/// Las densidades se convierten a conteos sobre el área dada (mirror de
/// `model_v2.__init__`).
pub fn build(params: Params, seed: u64) -> SigridModel {
    let mut rng = rng_from_seed(seed);
    let area_ha = params.width * params.height / 10_000.0;
    let area_km2 = params.width * params.height / 1_000_000.0;

    let n_sheep = (params.sheep_density * area_ha).round().max(1.0) as usize;
    let n_lambs = (n_sheep as f64 * params.lamb_proportion) as usize;
    let n_adults = n_sheep - n_lambs;
    let n_foxes = (params.fox_density * area_km2).round() as usize;
    let n_chillas = (params.chilla_density * area_km2).round() as usize;
    let n_hares = (params.hare_density * area_ha).round() as usize;
    let n_dogs = params.n_dogs;

    let (veg_quality, veg_cover) = build_rasters(params.width, params.height, &mut rng);

    let cap = n_sheep + n_foxes + n_chillas + n_hares + n_dogs + 1;
    let mut agents = AgentSet::with_capacity(cap);
    // El índice espacial se llena en cada `before_step`; aquí queda vacío.
    let space = ContinuousSpace::new(params.width, params.height, 60.0);

    let rand_pos = |rng: &mut SimRng| {
        Vec2::new(
            rng.random_range(0.0..params.width),
            rng.random_range(0.0..params.height),
        )
    };

    // Centro del rebaño (para inicializar perros alrededor).
    let flock_center = Vec2::new(params.width / 2.0, params.height / 2.0);

    // Ovejas adultas.
    let mut adult_ids: Vec<AgentId> = Vec::with_capacity(n_adults);
    for _ in 0..n_adults {
        let pos = clamp_pos(
            flock_center
                + Vec2::new(
                    rng.random_range(-300.0..300.0),
                    rng.random_range(-300.0..300.0),
                ),
            params.width,
            params.height,
        );
        let mut a = Animal::blank(Species::Sheep, pos);
        a.age_days = rng.random_range(365.0..2000.0);
        a.vulnerability = SHEEP_ADULT_VULN;
        let id = agents.insert(a);
        adult_ids.push(id);
    }
    // Corderos vinculados a una madre adulta al azar.
    for _ in 0..n_lambs {
        let mother = if adult_ids.is_empty() {
            None
        } else {
            Some(adult_ids[rng.random_range(0..adult_ids.len())])
        };
        let base = mother
            .and_then(|m| agents.get(m).map(|a| a.pos))
            .unwrap_or(flock_center);
        let pos = clamp_pos(
            base + Vec2::new(rng.random_range(-20.0..20.0), rng.random_range(-20.0..20.0)),
            params.width,
            params.height,
        );
        let mut a = Animal::blank(Species::Sheep, pos);
        a.is_lamb = true;
        a.energy = 70.0;
        a.age_days = rng.random_range(0.0..30.0);
        a.vulnerability = LAMB_VULN;
        a.mother = mother;
        agents.insert(a);
    }
    // Zorros culpeo.
    for _ in 0..n_foxes {
        let pos = rand_pos(&mut rng);
        let mut a = Animal::blank(Species::Fox, pos);
        a.hunger = rng.random_range(0.3..0.7);
        a.risk_aversion = BASE_RISK_AVERSION + rng.random_range(-0.1..0.1);
        a.predation_eff = params.fox_predation_effectiveness;
        a.territory = pos;
        a.territory_radius = FOX_TERRITORY_RADIUS;
        agents.insert(a);
    }
    // Chillas (mismo trait Fox, territorio menor, más averso al perro).
    for _ in 0..n_chillas {
        let pos = rand_pos(&mut rng);
        let mut a = Animal::blank(Species::Fox, pos);
        a.is_chilla = true;
        a.hunger = rng.random_range(0.3..0.7);
        a.risk_aversion = (BASE_RISK_AVERSION + rng.random_range(-0.1..0.1)) * 0.7;
        a.predation_eff = params.fox_predation_effectiveness;
        a.territory = pos;
        a.territory_radius = CHILLA_TERRITORY_RADIUS;
        agents.insert(a);
    }
    // Liebres.
    for _ in 0..n_hares {
        let pos = rand_pos(&mut rng);
        let mut a = Animal::blank(Species::Hare, pos);
        a.energy = rng.random_range(60.0..100.0);
        a.age_days = rng.random_range(0.0..365.0);
        a.mature = a.age_days * 24.0 >= HARE_MATURITY_AGE_H;
        a.vulnerability = if a.mature {
            HARE_VULN_MATURE
        } else {
            HARE_VULN_JUV
        };
        agents.insert(a);
    }
    // Perros guardianes alrededor del rebaño.
    for _ in 0..n_dogs {
        let a0: f64 = rng.random_range(0.0..std::f64::consts::TAU);
        let pos = clamp_pos(
            flock_center + Vec2::new(a0.cos(), a0.sin()) * 300.0,
            params.width,
            params.height,
        );
        let mut a = Animal::blank(Species::Dog, pos);
        a.patrol_center = flock_center;
        agents.insert(a);
    }

    SigridModel {
        agents,
        space,
        veg_quality,
        veg_cover,
        dog_positions: Vec::new(),
        params,
        current_hour: 0,
        step_count: 0,
        sheep_killed: 0,
        sheep_killed_by_fox: 0,
        sheep_killed_by_chilla: 0,
        hares_killed: 0,
        predation_attempts: 0,
        n_sheep_initial: n_sheep,
    }
}

/// Corre una simulación de `n_days` días (24 pasos/día) y devuelve el loss rate.
pub fn run_loss_rate(params: Params, seed: u64, n_days: u64) -> f64 {
    let mut sim =
        Simulation::new(build(params, seed), seed).with_schedule(Schedule::new(Activation::Random));
    sim.run(n_days * 24);
    sim.model.loss_rate_pct()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pressured() -> Params {
        Params {
            fox_predation_effectiveness: 0.26,
            chilla_density: 10.0,
            hare_density: 0.1,
            ..Params::default()
        }
    }

    /// El sello del motor sobre un modelo real: misma semilla => loss rate
    /// bit-idéntico (RNG por-agente determinista), corrida a corrida.
    #[test]
    fn determinismo_misma_semilla() {
        let p = pressured();
        let a = run_loss_rate(p, 12345, 10);
        let b = run_loss_rate(p, 12345, 10);
        assert_eq!(
            a.to_bits(),
            b.to_bits(),
            "loss rate no determinista: {a} vs {b}"
        );
    }

    /// Semillas distintas deberían (casi siempre) dar trayectorias distintas.
    #[test]
    fn semillas_distintas_difieren() {
        let p = pressured();
        let a = run_loss_rate(p, 1, 10);
        let b = run_loss_rate(p, 2, 10);
        assert!(
            (a - b).abs() > f64::EPSILON,
            "dos semillas dieron el mismo loss rate exacto"
        );
    }

    /// Los perros guardianes reducen la mortalidad (promedio sobre semillas).
    #[test]
    fn perros_reducen_mortalidad() {
        let mean = |n_dogs: usize| -> f64 {
            let p = Params {
                n_dogs,
                ..pressured()
            };
            (0..4).map(|s| run_loss_rate(p, 100 + s, 14)).sum::<f64>() / 4.0
        };
        let sin = mean(0);
        let con = mean(2);
        assert!(
            con < sin,
            "2 perros ({con:.1}%) no redujeron vs 0 perros ({sin:.1}%)"
        );
    }

    /// El loss rate siempre está en [0, 100].
    #[test]
    fn loss_rate_acotado() {
        for seed in 0..5 {
            let lr = run_loss_rate(pressured(), seed, 10);
            assert!(
                (0.0..=100.0).contains(&lr),
                "loss rate fuera de rango: {lr}"
            );
        }
    }
}
