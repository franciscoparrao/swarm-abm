//! Sugarscape (Epstein & Axtell, 1996): el modelo canónico de la economía
//! basada en agentes.
//!
//! Paisaje de dos picos de azúcar; agentes idénticos salvo tres atributos
//! sorteados (visión, metabolismo, dote inicial). Regla de movimiento M (van a
//! la celda libre con más azúcar dentro de su visión, la cosechan, pagan su
//! metabolismo; mueren si su azúcar llega a cero) + crecimiento G del paisaje.
//! De una población casi homogénea emerge una distribución de riqueza desigual
//! (Gini alto) y la población se autorregula a la capacidad de carga.

use swarm_core::prelude::*;

/// Una celda del paisaje: capacidad máxima de azúcar, azúcar presente y el
/// agente que la ocupa (si hay).
#[derive(Debug, Clone, Default)]
pub struct Cell {
    /// Azúcar máximo que la celda puede contener.
    pub capacity: u32,
    /// Azúcar disponible ahora.
    pub sugar: u32,
    /// Agente que ocupa la celda, si hay.
    pub occupant: Option<AgentId>,
}

/// Un agente. Visión y metabolismo son fijos de por vida; `sugar` es su
/// riqueza acumulada.
pub struct Ant {
    /// Posición en la grilla.
    pub pos: Pos,
    /// Alcance de visión a lo largo de los ejes.
    pub vision: usize,
    /// Azúcar consumido por paso.
    pub metabolism: u32,
    /// Riqueza acumulada.
    pub sugar: u32,
}

/// Parámetros del modelo Sugarscape.
#[derive(Debug, Clone, Copy)]
pub struct SugarscapeConfig {
    /// Ancho de la grilla.
    pub width: usize,
    /// Alto de la grilla.
    pub height: usize,
    /// Número de agentes iniciales.
    pub n_agents: usize,
    /// Azúcar que cada celda recupera por paso (regla de crecimiento G).
    pub growback: u32,
}

impl Default for SugarscapeConfig {
    fn default() -> Self {
        Self {
            width: 50,
            height: 50,
            n_agents: 400,
            growback: 1,
        }
    }
}

/// Estado global del modelo Sugarscape.
pub struct Sugarscape {
    agents: AgentSet<Ant>,
    grid: Grid2D<Cell>,
    growback: u32,
    dead: Vec<AgentId>,
}

impl Sugarscape {
    /// Número de agentes vivos.
    #[must_use]
    pub fn population(&self) -> usize {
        self.agents.len()
    }

    /// Coeficiente de Gini de la riqueza (0 = igualdad total, →1 = un solo
    /// agente concentra todo). Es la firma del modelo.
    #[must_use]
    pub fn gini(&self) -> f64 {
        let mut w: Vec<u64> = self
            .agents
            .iter()
            .map(|(_, a)| u64::from(a.sugar))
            .collect();
        let n = w.len();
        if n == 0 {
            return 0.0;
        }
        w.sort_unstable();
        let sum: u64 = w.iter().sum();
        if sum == 0 {
            return 0.0;
        }
        let mut acc: i128 = 0;
        for (i, &x) in w.iter().enumerate() {
            let coef = 2 * (i as i128 + 1) - n as i128 - 1;
            acc += coef * x as i128;
        }
        acc as f64 / (n as f64 * sum as f64)
    }

    /// Riqueza media de la población viva.
    #[must_use]
    pub fn mean_wealth(&self) -> f64 {
        let n = self.agents.len();
        if n == 0 {
            return 0.0;
        }
        let sum: u64 = self.agents.iter().map(|(_, a)| u64::from(a.sugar)).sum();
        sum as f64 / n as f64
    }

    /// Riqueza de cada agente vivo (para histogramas de la distribución).
    #[must_use]
    pub fn wealths(&self) -> Vec<u32> {
        self.agents.iter().map(|(_, a)| a.sugar).collect()
    }
}

impl Agent for Ant {
    type Model = Sugarscape;

    fn step(&mut self, id: AgentId, model: &mut Sugarscape, rng: &mut SimRng) {
        let (w, h) = (model.grid.width() as i64, model.grid.height() as i64);

        let mut best_pos = self.pos;
        let mut best_sugar = model.grid[self.pos].sugar;
        let mut best_dist = 0usize;

        // Orden de ejes barajado por paso: rompe el sesgo direccional en los
        // empates exactos sin perder determinismo.
        let mut axes: [(i64, i64); 4] = [(0, -1), (0, 1), (-1, 0), (1, 0)];
        axes.shuffle(rng);

        for (dx, dy) in axes {
            for d in 1..=self.vision as i64 {
                let (nx, ny) = (self.pos.x as i64 + dx * d, self.pos.y as i64 + dy * d);
                if nx < 0 || ny < 0 || nx >= w || ny >= h {
                    break;
                }
                let cand = Pos::new(nx as usize, ny as usize);
                let cell = &model.grid[cand];
                if cell.occupant.is_some() {
                    continue;
                }
                let dist = d as usize;
                if cell.sugar > best_sugar || (cell.sugar == best_sugar && dist < best_dist) {
                    best_pos = cand;
                    best_sugar = cell.sugar;
                    best_dist = dist;
                }
            }
        }

        if best_pos != self.pos {
            model.grid[self.pos].occupant = None;
            model.grid[best_pos].occupant = Some(id);
            self.pos = best_pos;
        }
        self.sugar += model.grid[self.pos].sugar;
        model.grid[self.pos].sugar = 0;

        if self.sugar <= self.metabolism {
            model.grid[self.pos].occupant = None;
            model.dead.push(id);
        } else {
            self.sugar -= self.metabolism;
        }
    }
}

impl Model for Sugarscape {
    type Agent = Ant;

    fn agents(&self) -> &AgentSet<Ant> {
        &self.agents
    }

    fn agents_mut(&mut self) -> &mut AgentSet<Ant> {
        &mut self.agents
    }

    fn after_step(&mut self, _rng: &mut SimRng) {
        for id in self.dead.drain(..) {
            self.agents.remove(id);
        }
        let growback = self.growback;
        for (_, cell) in self.grid.iter_mut() {
            cell.sugar = (cell.sugar + growback).min(cell.capacity);
        }
    }

    fn finished(&self) -> bool {
        self.agents.is_empty()
    }
}

/// Capacidad de azúcar de una celda: dos picos en diagonal, con mesetas
/// concéntricas de nivel 4 → 0 (el paisaje canónico de dos montañas).
fn capacity_at(p: Pos, w: usize, h: usize) -> u32 {
    let peaks = [
        (w as f64 * 0.75, h as f64 * 0.25),
        (w as f64 * 0.25, h as f64 * 0.75),
    ];
    let maxr = w.min(h) as f64 * 0.55;
    let bands = [0.18, 0.37, 0.62, 1.0];
    let mut cap = 0u32;
    for (px, py) in peaks {
        let d = ((p.x as f64 - px).powi(2) + (p.y as f64 - py).powi(2)).sqrt();
        let level = bands
            .iter()
            .position(|&b| d < maxr * b)
            .map_or(0, |i| 4 - i as u32);
        cap = cap.max(level);
    }
    cap
}

/// Construye un modelo Sugarscape a partir de su configuración y una semilla.
#[must_use]
pub fn build(config: SugarscapeConfig, seed: u64) -> Sugarscape {
    let SugarscapeConfig {
        width,
        height,
        n_agents,
        growback,
    } = config;
    let mut rng = rng_from_seed(seed ^ 0x5_06A8_5CA9_E000);
    let mut grid = Grid2D::from_fn(width, height, |p| {
        let cap = capacity_at(p, width, height);
        Cell {
            capacity: cap,
            sugar: cap,
            occupant: None,
        }
    });

    let mut coords: Vec<Pos> = grid.iter().map(|(p, _)| p).collect();
    coords.shuffle(&mut rng);

    let mut agents = AgentSet::with_capacity(n_agents);
    for &pos in coords.iter().take(n_agents) {
        let vision = rng.random_range(1..=6);
        let metabolism = rng.random_range(1..=4);
        let sugar = rng.random_range(5..=25);
        let id = agents.insert(Ant {
            pos,
            vision,
            metabolism,
            sugar,
        });
        grid[pos].occupant = Some(id);
    }

    Sugarscape {
        agents,
        grid,
        growback,
        dead: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn estado_inicial() {
        let m = build(SugarscapeConfig::default(), 1);
        assert_eq!(m.population(), 400);
        assert!(m.gini() >= 0.0 && m.gini() < 1.0);
    }

    #[test]
    fn emerge_desigualdad_y_se_autorregula() {
        let mut sim = Simulation::new(build(SugarscapeConfig::default(), 42), 42)
            .with_schedule(Schedule::new(Activation::Random));
        let g0 = sim.model.gini();
        sim.run(200);
        // La población cae a la capacidad de carga y la desigualdad sube.
        assert!(sim.model.population() < 400);
        assert!(sim.model.gini() > g0);
    }
}
