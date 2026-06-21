//! V&V de la fase `decide` simultánea paralela.
//!
//! Propiedad clave: correr `decide` en paralelo produce el **mismo resultado
//! bit a bit** que en secuencial, sin importar cuántos hilos. Es lo que
//! convierte el determinismo en una *garantía verificable* (no un accidente):
//! el RNG de cada agente depende solo de `(semilla, paso, id)` y `decide`
//! recibe el modelo inmutable, así que el orden de ejecución es irrelevante.

#![cfg(feature = "parallel")]

use swarm_core::prelude::*;

struct Walker {
    pending: u64,
    acc: u64,
}

struct World {
    agents: AgentSet<Walker>,
}

impl Agent for Walker {
    type Model = World;

    // Solo lee su RNG por-agente (camino paralelizable: nada compartido).
    fn decide(&mut self, _id: AgentId, _model: &World, rng: &mut SimRng) {
        self.pending = rng.random_range(0..1_000_000);
    }

    fn apply(&mut self, _id: AgentId, _model: &mut World, _rng: &mut SimRng) {
        self.acc = self.acc.wrapping_add(self.pending);
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

fn build(n: usize) -> World {
    let mut agents = AgentSet::with_capacity(n);
    for _ in 0..n {
        agents.insert(Walker { pending: 0, acc: 0 });
    }
    World { agents }
}

fn accumulators(sim: &Simulation<World>) -> Vec<u64> {
    sim.model.agents().iter().map(|(_, w)| w.acc).collect()
}

#[test]
fn decide_paralelo_es_bit_identico_al_secuencial() {
    let (n, steps, seed) = (2000usize, 40u64, 12_345u64);

    let mut seq =
        Simulation::new(build(n), seed).with_schedule(Schedule::new(Activation::Simultaneous));
    seq.run(steps);

    let mut par =
        Simulation::new(build(n), seed).with_schedule(Schedule::new(Activation::Simultaneous));
    par.run_parallel(steps);

    assert_eq!(
        accumulators(&seq),
        accumulators(&par),
        "decide paralelo debe coincidir bit a bit con el secuencial"
    );
    // Cordura: hubo trabajo aleatorio real (no quedó todo en cero).
    assert!(accumulators(&seq).iter().any(|&a| a > 0));
}
