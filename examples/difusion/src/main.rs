//! Difusión: caminantes aleatorios depositan feromona en un campo escalar
//! que difunde (estilo `diffuse` de NetLogo) y se evapora en cada paso.
//!
//! El sistema tiene un punto fijo analítico: con `n` agentes depositando
//! `d` por paso y evaporación `e` (la difusión conserva masa), la masa
//! total converge a `m* = n·d·(1-e)/e`. El ejemplo compara el valor
//! simulado contra el teórico.
//!
//! Uso: `cargo run --release -p difusion [semilla]`

use swarm_abm::prelude::*;

struct Walker {
    pos: Pos,
}

struct World {
    agents: AgentSet<Walker>,
    field: Grid2D<f64>,
    deposit: f64,
    diffusion_rate: f64,
    evaporation: f64,
}

impl Agent for Walker {
    type Model = World;

    fn step(&mut self, _id: AgentId, world: &mut World, rng: &mut SimRng) {
        if let Some(destino) = world
            .field
            .random_neighbor(self.pos, Neighborhood::Moore, rng)
        {
            self.pos = destino;
            world.field[self.pos] += world.deposit;
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

    /// El campo evoluciona como entorno, después de que actúan los agentes.
    fn after_step(&mut self, _rng: &mut SimRng) {
        self.field.diffuse(self.diffusion_rate, Neighborhood::Moore);
        let factor = 1.0 - self.evaporation;
        for (_, v) in self.field.iter_mut() {
            *v *= factor;
        }
    }
}

fn build(width: usize, height: usize, n_walkers: usize, seed: u64) -> World {
    let mut rng = rng_from_seed(seed ^ 0xD1F0_55ED);
    let mut agents = AgentSet::with_capacity(n_walkers);
    for _ in 0..n_walkers {
        let pos = Pos::new(rng.random_range(0..width), rng.random_range(0..height));
        agents.insert(Walker { pos });
    }
    World {
        agents,
        field: Grid2D::new(width, height).with_torus(true),
        deposit: 1.0,
        diffusion_rate: 0.5,
        evaporation: 0.05,
    }
}

fn main() {
    let seed: u64 = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(42);

    let model = build(50, 50, 20, seed);
    let n = model.agents.len();
    let teorico = n as f64 * model.deposit * (1.0 - model.evaporation) / model.evaporation;

    let mut sim = Simulation::new(model, seed);
    sim.add_reporter("masa_total", |w: &World| w.field.total());
    sim.add_reporter("max_celda", |w: &World| {
        w.field.iter().map(|(_, &v)| v).fold(0.0, f64::max)
    });
    sim.add_reporter("celdas_activas", |w: &World| {
        w.field.iter().filter(|&(_, &v)| v > 0.01).count() as f64
    });

    sim.run(300);

    let masa = sim.data().series("masa_total").unwrap_or(&[]);
    let final_masa = masa.last().copied().unwrap_or(0.0);

    println!(
        "Difusión 50x50 (torus) | {n} caminantes | difusión 0.5, evaporación 0.05 | semilla {seed}"
    );
    println!("Masa en estado estacionario: teórica {teorico:.1}, simulada {final_masa:.1}");

    let max_c = sim.data().series("max_celda").unwrap_or(&[]);
    let activas = sim.data().series("celdas_activas").unwrap_or(&[]);
    println!("\n  paso  masa_total  max_celda  celdas_activas");
    for (i, &step) in sim.data().steps().iter().enumerate() {
        if step % 50 == 0 || i + 1 == masa.len() {
            println!(
                "  {step:>4}  {:>10.1}  {:>9.2}  {:>14.0}",
                masa[i], max_c[i], activas[i]
            );
        }
    }
}
