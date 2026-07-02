//! Modelo de segregación de Schelling (1971).
//!
//! Agentes de dos grupos sobre una grilla toroidal. Un agente está conforme si
//! la fracción de vecinos de su mismo grupo es ≥ `tolerance`; si no, se muda a
//! una celda vacía al azar. El resultado clásico: segregación emergente incluso
//! con tolerancias bajas.

use swarm_abm::prelude::*;

/// Grupo al que pertenece un agente.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Group {
    /// Primer grupo.
    Red,
    /// Segundo grupo.
    Blue,
}

/// Un agente con posición y pertenencia de grupo.
pub struct Person {
    /// Posición en la grilla.
    pub pos: Pos,
    /// Grupo del agente.
    pub group: Group,
}

/// Parámetros del modelo de Schelling.
#[derive(Debug, Clone, Copy)]
pub struct SchellingConfig {
    /// Ancho de la grilla.
    pub width: usize,
    /// Alto de la grilla.
    pub height: usize,
    /// Fracción de celdas ocupadas (el resto quedan vacías).
    pub density: f64,
    /// Umbral de conformidad: fracción mínima de vecinos del mismo grupo.
    pub tolerance: f64,
}

impl Default for SchellingConfig {
    fn default() -> Self {
        Self {
            width: 50,
            height: 50,
            density: 0.85,
            tolerance: 0.375,
        }
    }
}

/// Estado global del modelo de Schelling.
pub struct Schelling {
    agents: AgentSet<Person>,
    grid: Grid2D<Option<AgentId>>,
    empties: Vec<Pos>,
    tolerance: f64,
}

impl Schelling {
    /// Fracción de vecinos ocupados del mismo grupo (1.0 si está aislado).
    fn similarity_at(&self, pos: Pos, group: Group) -> f64 {
        let mut same = 0u32;
        let mut occupied = 0u32;
        for (_, cell) in self.grid.neighbors(pos, Neighborhood::Moore) {
            if let Some(id) = cell {
                occupied += 1;
                if self.agents.get(*id).is_some_and(|n| n.group == group) {
                    same += 1;
                }
            }
        }
        if occupied == 0 {
            1.0
        } else {
            f64::from(same) / f64::from(occupied)
        }
    }

    fn is_happy(&self, person: &Person) -> bool {
        self.similarity_at(person.pos, person.group) >= self.tolerance
    }

    /// Número de agentes.
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
    /// `0` vacía, `1` grupo Red, `2` grupo Blue.
    #[must_use]
    pub fn cells(&self) -> Vec<u8> {
        self.grid
            .iter()
            .map(|(_, cell)| match cell {
                None => 0,
                Some(id) => match self.agents.get(*id).map(|p| p.group) {
                    Some(Group::Red) => 1,
                    Some(Group::Blue) => 2,
                    None => 0,
                },
            })
            .collect()
    }

    /// Fracción de agentes conformes (felices) con su vecindario.
    #[must_use]
    pub fn fraction_happy(&self) -> f64 {
        if self.agents.is_empty() {
            return 1.0;
        }
        let happy = self.agents.iter().filter(|(_, p)| self.is_happy(p)).count();
        happy as f64 / self.agents.len() as f64
    }

    /// Similitud media: fracción media de vecinos del mismo grupo (índice de
    /// segregación; sube a medida que el sistema se segrega).
    #[must_use]
    pub fn mean_similarity(&self) -> f64 {
        if self.agents.is_empty() {
            return 1.0;
        }
        let total: f64 = self
            .agents
            .iter()
            .map(|(_, p)| self.similarity_at(p.pos, p.group))
            .sum();
        total / self.agents.len() as f64
    }
}

impl Agent for Person {
    type Model = Schelling;

    fn step(&mut self, id: AgentId, model: &mut Schelling, rng: &mut SimRng) {
        if model.similarity_at(self.pos, self.group) >= model.tolerance {
            return;
        }
        if model.empties.is_empty() {
            return;
        }
        let i = uniform_usize(rng, model.empties.len());
        let destino = model.empties.swap_remove(i);
        model.grid[self.pos] = None;
        model.grid[destino] = Some(id);
        model.empties.push(self.pos);
        self.pos = destino;
    }
}

impl Model for Schelling {
    type Agent = Person;

    fn agents(&self) -> &AgentSet<Person> {
        &self.agents
    }

    fn agents_mut(&mut self) -> &mut AgentSet<Person> {
        &mut self.agents
    }

    fn finished(&self) -> bool {
        self.agents.iter().all(|(_, p)| self.is_happy(p))
    }
}

/// Construye un modelo de Schelling a partir de su configuración y una semilla.
#[must_use]
pub fn build(config: SchellingConfig, seed: u64) -> Schelling {
    let SchellingConfig {
        width,
        height,
        density,
        tolerance,
    } = config;
    let mut rng = rng_from_seed(seed ^ 0x05E7_0F5E_ED00);
    let mut grid: Grid2D<Option<AgentId>> = Grid2D::new(width, height).with_torus(true);
    let mut agents = AgentSet::new();

    let mut coords: Vec<Pos> = grid.iter().map(|(p, _)| p).collect();
    shuffle(&mut rng, &mut coords);

    let n_agents = ((width * height) as f64 * density).round() as usize;
    for (i, &pos) in coords.iter().take(n_agents).enumerate() {
        let group = if i % 2 == 0 { Group::Red } else { Group::Blue };
        let id = agents.insert(Person { pos, group });
        grid[pos] = Some(id);
    }
    let empties = coords[n_agents..].to_vec();

    Schelling {
        agents,
        grid,
        empties,
        tolerance,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn estado_inicial() {
        let cfg = SchellingConfig {
            width: 20,
            height: 20,
            density: 0.9,
            tolerance: 0.5,
        };
        let m = build(cfg, 1);
        assert_eq!(m.population(), (400.0_f64 * 0.9).round() as usize);
    }

    #[test]
    fn cells_tiene_un_valor_por_celda() {
        let cfg = SchellingConfig {
            width: 10,
            height: 8,
            density: 0.5,
            tolerance: 0.5,
        };
        let m = build(cfg, 1);
        let cells = m.cells();
        assert_eq!(cells.len(), 80);
        assert!(cells.iter().all(|&c| c <= 2));
        // Hay tantas celdas no vacías como agentes.
        assert_eq!(cells.iter().filter(|&&c| c != 0).count(), m.population());
    }

    #[test]
    fn la_segregacion_aumenta_la_similitud() {
        let cfg = SchellingConfig::default();
        let mut sim = Simulation::new(build(cfg, 3), 3);
        let sim0 = build(cfg, 3);
        let inicial = sim0.mean_similarity();
        sim.run(200);
        // Tras correr, el vecindario es más homogéneo que al inicio.
        assert!(sim.model.mean_similarity() > inicial);
    }
}
