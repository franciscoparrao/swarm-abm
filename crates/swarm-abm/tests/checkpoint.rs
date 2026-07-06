#![cfg(feature = "serde")]
//! Test de checkpoint/restore (P1-7 de `docs/AUDIT.md`): serializar
//! `(modelo, semilla, estado del RNG, pasos corridos)` y reconstruir con
//! [`Simulation::from_checkpoint`] debe dar una corrida **bit-idéntica** a
//! la que nunca se interrumpió — la propiedad central del wedge
//! (determinismo por construcción) extendida a reanudar una corrida larga.

use serde::{Deserialize, Serialize};
use swarm_abm::prelude::*;

#[derive(Serialize, Deserialize)]
struct Walker {
    pos: Pos,
}

#[derive(Serialize, Deserialize)]
struct World {
    agents: AgentSet<Walker>,
    /// Cuenta de visitas por celda: la huella que debe coincidir entre la
    /// corrida sin interrupción y la reanudada.
    grid: Grid2D<u32>,
}

impl Agent for Walker {
    type Model = World;

    fn step(&mut self, _id: AgentId, model: &mut World, rng: &mut SimRng) {
        if let Some(dest) = model
            .grid
            .random_neighbor(self.pos, Neighborhood::Moore, rng)
        {
            self.pos = dest;
            model.grid[self.pos] += 1;
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

fn build(seed: u64) -> World {
    let mut rng = rng_from_seed(seed);
    let mut agents = AgentSet::new();
    for _ in 0..50 {
        let pos = Pos::new(uniform_usize(&mut rng, 20), uniform_usize(&mut rng, 20));
        agents.insert(Walker { pos });
    }
    World {
        agents,
        grid: Grid2D::new(20, 20).with_torus(true),
    }
}

fn huella(w: &World) -> Vec<(Pos, u32)> {
    w.grid.iter().map(|(p, &v)| (p, v)).collect()
}

#[test]
fn checkpoint_y_restore_es_bit_identico_a_no_interrumpir() {
    let seed = 123;

    // Corrida A: sin interrupción, 20 pasos.
    let mut sin_interrumpir =
        Simulation::new(build(seed), seed).with_schedule(Schedule::new(Activation::Random));
    sin_interrumpir.run(20);

    // Corrida B: 10 pasos, checkpoint (serializar + deserializar todo), 10
    // pasos más sobre la simulación reconstruida.
    let mut sim =
        Simulation::new(build(seed), seed).with_schedule(Schedule::new(Activation::Random));
    sim.run(10);

    let model_json = serde_json::to_string(&sim.model).expect("modelo serializable");
    let rng_json = serde_json::to_string(sim.rng_state()).expect("rng serializable");
    let seed_guardado = sim.seed();
    let pasos_guardados = sim.step_count();

    let modelo_restaurado: World =
        serde_json::from_str(&model_json).expect("modelo deserializable");
    let rng_restaurado: SimRng = serde_json::from_str(&rng_json).expect("rng deserializable");

    let mut resumida = Simulation::from_checkpoint(
        modelo_restaurado,
        seed_guardado,
        rng_restaurado,
        pasos_guardados,
        Schedule::new(Activation::Random),
    );
    resumida.run(10);

    assert_eq!(
        sin_interrumpir.step_count(),
        resumida.step_count(),
        "misma cantidad de pasos totales"
    );
    assert_eq!(
        huella(&sin_interrumpir.model),
        huella(&resumida.model),
        "el checkpoint debe reanudar bit-exactamente"
    );
}

/// Reproduce el escenario exacto de la auditoría (docs/AUDIT.md): antes,
/// `from_checkpoint` fijaba siempre `Schedule::default()` (Random),
/// ignorando en silencio la política real de la simulación original. Con
/// `Activation::Ordered` (que además, a diferencia de Random, no consume
/// RNG del stream de pasos para barajar), ese bug habría barajado el orden
/// de reanudación y consumido `SimRng` de más, divergiendo de la corrida
/// sin interrumpir. `from_checkpoint` ahora exige el `Schedule` como
/// parámetro — este test verifica que pasar el original (`Ordered`)
/// preserva la bit-exactitud.
#[test]
fn checkpoint_respeta_el_schedule_ordered_original() {
    let seed = 55;
    let schedule = Schedule::new(Activation::Ordered);

    let mut sin_interrumpir = Simulation::new(build(seed), seed).with_schedule(schedule);
    sin_interrumpir.run(20);

    let mut sim = Simulation::new(build(seed), seed).with_schedule(schedule);
    sim.run(10);

    let model_json = serde_json::to_string(&sim.model).expect("modelo serializable");
    let rng_json = serde_json::to_string(sim.rng_state()).expect("rng serializable");
    let modelo_restaurado: World =
        serde_json::from_str(&model_json).expect("modelo deserializable");
    let rng_restaurado: SimRng = serde_json::from_str(&rng_json).expect("rng deserializable");

    let mut resumida = Simulation::from_checkpoint(
        modelo_restaurado,
        sim.seed(),
        rng_restaurado,
        sim.step_count(),
        schedule,
    );
    resumida.run(10);

    assert_eq!(
        huella(&sin_interrumpir.model),
        huella(&resumida.model),
        "el checkpoint debe reanudar bit-exactamente bajo Ordered, no solo bajo el Random por defecto"
    );
}

#[test]
fn from_checkpoint_no_recolecta_el_paso_0_de_nuevo() {
    // El estado inicial ya se capturó en la sesión "anterior" (antes del
    // checkpoint); `from_checkpoint` no debe volver a etiquetar el estado
    // restaurado como "paso 0".
    let seed = 7;
    let mut sim = Simulation::new(build(seed), seed);
    sim.run(5);
    let steps_previos = sim.step_count();

    let mut resumida = Simulation::from_checkpoint(
        build(seed),
        seed,
        rng_from_seed(seed),
        steps_previos,
        Schedule::default(),
    );
    resumida.add_reporter("dummy", |_: &World| 0.0);
    resumida.run(3);
    // No hay una fila re-etiquetada como "paso 0"/`steps_previos`: la
    // primera fila recolectada es el primer paso REAL corrido después de
    // reanudar (steps_previos + 1), no una repetición del estado del
    // checkpoint.
    assert_eq!(
        resumida.data().steps(),
        &[steps_previos + 1, steps_previos + 2, steps_previos + 3]
    );
}
