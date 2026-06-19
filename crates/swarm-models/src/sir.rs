//! SIR espacial: epidemia sobre una grilla toroidal totalmente ocupada.
//!
//! Cada celda tiene un agente fijo. Un susceptible con `k` vecinos (Moore)
//! infectados se contagia con probabilidad `1 − (1−beta)^k`; un infectado se
//! recupera con probabilidad `gamma` por paso. Termina cuando no quedan
//! infectados.

use swarm_core::prelude::*;

/// Estado epidemiológico de un agente.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// Sano y contagiable.
    Susceptible,
    /// Cursando la infección (puede contagiar y recuperarse).
    Infected,
    /// Recuperado e inmune.
    Recovered,
}

/// Un habitante de la grilla.
pub struct Person {
    /// Posición fija en la grilla.
    pub pos: Pos,
    /// Estado epidemiológico actual.
    pub status: Status,
}

/// Parámetros del modelo SIR.
#[derive(Debug, Clone, Copy)]
pub struct SirConfig {
    /// Ancho de la grilla.
    pub width: usize,
    /// Alto de la grilla.
    pub height: usize,
    /// Cuántos agentes arrancan infectados (sembrados al azar).
    pub initial_infected: usize,
    /// Probabilidad de contagio por vecino infectado, por paso.
    pub beta: f64,
    /// Probabilidad de recuperación de un infectado, por paso.
    pub gamma: f64,
}

impl Default for SirConfig {
    fn default() -> Self {
        Self {
            width: 100,
            height: 100,
            initial_infected: 10,
            beta: 0.08,
            gamma: 0.1,
        }
    }
}

/// Estado global del modelo SIR.
pub struct Sir {
    agents: AgentSet<Person>,
    grid: Grid2D<Option<AgentId>>,
    beta: f64,
    gamma: f64,
}

impl Sir {
    /// Número de agentes en un estado dado.
    #[must_use]
    pub fn count(&self, status: Status) -> usize {
        self.agents
            .iter()
            .filter(|(_, p)| p.status == status)
            .count()
    }

    /// Población total (constante: la grilla está siempre llena).
    #[must_use]
    pub fn population(&self) -> usize {
        self.agents.len()
    }

    /// Ancho de la grilla.
    #[must_use]
    pub fn width(&self) -> usize {
        self.grid.width()
    }

    /// Alto de la grilla.
    #[must_use]
    pub fn height(&self) -> usize {
        self.grid.height()
    }

    /// Categoría por celda en orden fila-mayor, para visualización:
    /// `0` susceptible, `1` infectado, `2` recuperado.
    #[must_use]
    pub fn cells(&self) -> Vec<u8> {
        self.grid
            .iter()
            .map(|(_, cell)| {
                cell.and_then(|id| self.agents.get(id))
                    .map_or(0, |p| match p.status {
                        Status::Susceptible => 0,
                        Status::Infected => 1,
                        Status::Recovered => 2,
                    })
            })
            .collect()
    }

    /// Fracción de la población en un estado dado.
    #[must_use]
    pub fn fraction(&self, status: Status) -> f64 {
        let n = self.agents.len();
        if n == 0 {
            0.0
        } else {
            self.count(status) as f64 / n as f64
        }
    }

    fn infected_neighbors(&self, pos: Pos) -> u32 {
        self.grid
            .neighbors(pos, Neighborhood::Moore)
            .filter(|(_, cell)| {
                cell.is_some_and(|id| {
                    self.agents
                        .get(id)
                        .is_some_and(|n| n.status == Status::Infected)
                })
            })
            .count() as u32
    }
}

impl Agent for Person {
    type Model = Sir;

    fn step(&mut self, _id: AgentId, model: &mut Sir, rng: &mut SimRng) {
        match self.status {
            Status::Susceptible => {
                let k = model.infected_neighbors(self.pos);
                if k > 0 {
                    let p = 1.0 - (1.0 - model.beta).powi(k as i32);
                    if rng.random_bool(p) {
                        self.status = Status::Infected;
                    }
                }
            }
            Status::Infected => {
                if rng.random_bool(model.gamma) {
                    self.status = Status::Recovered;
                }
            }
            Status::Recovered => {}
        }
    }
}

impl Model for Sir {
    type Agent = Person;

    fn agents(&self) -> &AgentSet<Person> {
        &self.agents
    }

    fn agents_mut(&mut self) -> &mut AgentSet<Person> {
        &mut self.agents
    }

    fn finished(&self) -> bool {
        self.agents
            .iter()
            .all(|(_, p)| p.status != Status::Infected)
    }
}

/// Construye un modelo SIR a partir de su configuración y una semilla.
///
/// El sembrado de infectados consume el RNG de forma determinista: misma
/// `(config, seed)` ⇒ mismo estado inicial.
#[must_use]
pub fn build(config: SirConfig, seed: u64) -> Sir {
    let SirConfig {
        width,
        height,
        initial_infected,
        beta,
        gamma,
    } = config;
    let mut rng = rng_from_seed(seed ^ 0x0510_5EED);
    let mut agents = AgentSet::with_capacity(width * height);
    let mut grid: Grid2D<Option<AgentId>> = Grid2D::new(width, height).with_torus(true);

    let mut ids = Vec::with_capacity(width * height);
    for y in 0..height {
        for x in 0..width {
            let pos = Pos::new(x, y);
            let id = agents.insert(Person {
                pos,
                status: Status::Susceptible,
            });
            grid[pos] = Some(id);
            ids.push(id);
        }
    }

    ids.shuffle(&mut rng);
    for &id in ids.iter().take(initial_infected) {
        if let Some(p) = agents.get_mut(id) {
            p.status = Status::Infected;
        }
    }

    Sir {
        agents,
        grid,
        beta,
        gamma,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn estado_inicial_consistente() {
        let cfg = SirConfig {
            width: 20,
            height: 20,
            initial_infected: 5,
            ..Default::default()
        };
        let m = build(cfg, 42);
        assert_eq!(m.population(), 400);
        assert_eq!(m.count(Status::Infected), 5);
        assert_eq!(m.count(Status::Susceptible), 395);
        assert_eq!(m.count(Status::Recovered), 0);
    }

    #[test]
    fn la_epidemia_avanza_y_termina() {
        let cfg = SirConfig {
            width: 50,
            height: 50,
            initial_infected: 10,
            beta: 0.2,
            gamma: 0.1,
        };
        let mut sim = Simulation::new(build(cfg, 7), 7);
        sim.run(1000);
        // Terminó (sin infectados) y dejó recuperados: hubo epidemia.
        assert_eq!(sim.model.count(Status::Infected), 0);
        assert!(sim.model.count(Status::Recovered) > 10);
    }

    #[test]
    fn determinismo_bit_a_bit() {
        let cfg = SirConfig::default();
        let corrida = |seed| {
            let mut sim = Simulation::new(build(cfg, seed), seed);
            sim.run(50);
            sim.model.count(Status::Recovered)
        };
        assert_eq!(corrida(123), corrida(123));
    }
}
