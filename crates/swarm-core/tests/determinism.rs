//! Test de integración: la simulación completa es determinista dada la
//! semilla (RNG + scheduler aleatorio + movimiento de agentes).

use swarm_core::prelude::*;

struct Walker {
    pos: Pos,
}

struct World {
    agents: AgentSet<Walker>,
    grid: Grid2D<u32>,
}

impl Agent for Walker {
    type Model = World;

    fn step(&mut self, _id: AgentId, world: &mut World, rng: &mut SimRng) {
        if let Some(destino) = world
            .grid
            .random_neighbor(self.pos, Neighborhood::Moore, rng)
        {
            self.pos = destino;
            world.grid[self.pos] += 1;
        }
    }
}

impl Model for World {
    type Agent = Walker;

    fn agents(&self) -> &AgentSet<Walker> {
        &self.agents
    }

    fn agents_mut(&mut self) -> &mut AgentSet<Walker> {
        &mut self.agents
    }
}

/// Corre 100 caminantes 50 pasos y devuelve la serie de "masa" de posiciones.
fn correr(seed: u64) -> (Vec<f64>, Vec<(usize, usize)>) {
    let mut setup_rng = rng_from_seed(seed ^ 0xABCD);
    let mut agents = AgentSet::new();
    for _ in 0..100 {
        let pos = Pos::new(setup_rng.random_range(0..30), setup_rng.random_range(0..30));
        agents.insert(Walker { pos });
    }
    let grid = Grid2D::new(30, 30).with_torus(true);

    let mut sim = Simulation::new(World { agents, grid }, seed);
    sim.add_reporter("suma_x", |w: &World| {
        w.agents.iter().map(|(_, a)| a.pos.x as f64).sum()
    });
    sim.run(50);

    let serie = sim
        .data()
        .series("suma_x")
        .expect("reporter registrado")
        .to_vec();
    let posiciones = sim
        .model
        .agents
        .iter()
        .map(|(_, a)| (a.pos.x, a.pos.y))
        .collect();
    (serie, posiciones)
}

#[test]
fn misma_semilla_reproduce_exactamente() {
    let (serie_a, pos_a) = correr(2024);
    let (serie_b, pos_b) = correr(2024);
    assert_eq!(serie_a, serie_b);
    assert_eq!(pos_a, pos_b);
}

#[test]
fn semillas_distintas_divergen() {
    let (serie_a, _) = correr(1);
    let (serie_b, _) = correr(2);
    assert_ne!(serie_a, serie_b);
}
