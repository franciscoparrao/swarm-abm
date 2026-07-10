//! Tests de `Activation::Staged` / `Agent::stage` (P1-8 de `docs/AUDIT.md`):
//! `N` barridos completos por paso, donde **todos** los agentes completan
//! la etapa `s` antes de que cualquiera entre a la etapa `s+1` — el patrón
//! `StagedActivation` de Mesa.
//!
//! El modelo de prueba verifica exactamente esa garantía: en la etapa 1,
//! cada agente comprueba que *todos los demás* ya completaron la etapa 0
//! (`counter == 1`), sin importar el orden de recorrido. Sin la garantía de
//! barrido completo (p. ej. si esto se hiciera con `Agent::step` en un solo
//! paso por agente), los agentes procesados primero verían a los últimos
//! todavía sin pasar por la etapa 0.

use swarm_abm::prelude::*;

struct Node {
    counter: u32,
    vio_a_todos_en_etapa_0: bool,
}

struct World {
    agents: AgentSet<Node>,
}

impl Agent for Node {
    type Model = World;

    fn stage(&mut self, stage: usize, _id: AgentId, model: &mut World, _rng: &mut SimRng) {
        match stage {
            0 => self.counter = 1,
            1 => {
                // `model.agents()` aquí son TODOS los demás (este agente está
                // afuera, en take-out): deben estar todos en counter==1 si el
                // barrido de la etapa 0 de verdad terminó para todos antes de
                // que empezara la etapa 1 de cualquiera.
                self.vio_a_todos_en_etapa_0 = model.agents.iter().all(|(_, n)| n.counter == 1);
            }
            2 => self.counter += 10,
            _ => unreachable!("solo se configuraron 3 etapas"),
        }
    }
}

impl Model for World {
    type Agent = Node;
    fn agents(&self) -> &AgentSet<Node> {
        &self.agents
    }
    fn agents_mut(&mut self) -> &mut AgentSet<Node> {
        &mut self.agents
    }
}

fn build(n: usize) -> World {
    let mut agents = AgentSet::with_capacity(n);
    for _ in 0..n {
        agents.insert(Node {
            counter: 0,
            vio_a_todos_en_etapa_0: false,
        });
    }
    World { agents }
}

#[test]
fn todos_completan_la_etapa_0_antes_de_que_alguno_entre_a_la_etapa_1() {
    let mut sim = Simulation::new(build(50), 3).with_schedule(Schedule::new(Activation::Staged(3)));
    sim.step();
    assert!(
        sim.model
            .agents
            .iter()
            .all(|(_, n)| n.vio_a_todos_en_etapa_0),
        "cada agente debía ver a TODOS los demás ya en counter==1 al llegar a la etapa 1"
    );
}

#[test]
fn las_tres_etapas_se_ejecutan_en_orden() {
    let mut sim = Simulation::new(build(10), 1).with_schedule(Schedule::new(Activation::Staged(3)));
    sim.step();
    // Etapa 0 pone counter=1, etapa 2 suma 10: si las tres corrieron en
    // orden, todos terminan en exactamente 11.
    assert!(sim.model.agents.iter().all(|(_, n)| n.counter == 11));
}

#[test]
fn staged_es_determinista() {
    let correr = || {
        let mut sim =
            Simulation::new(build(30), 7).with_schedule(Schedule::new(Activation::Staged(3)));
        sim.run(4);
        sim.model
            .agents
            .iter()
            .map(|(_, n)| n.counter)
            .collect::<Vec<_>>()
    };
    assert_eq!(correr(), correr());
}

#[test]
fn cero_etapas_no_hace_nada() {
    let mut sim = Simulation::new(build(5), 1).with_schedule(Schedule::new(Activation::Staged(0)));
    sim.step();
    assert!(sim.model.agents.iter().all(|(_, n)| n.counter == 0));
}

// ---------------------------------------------------------------------------
// M7a: the `Activation::Staged(n) => self.staged_phases(n)` arm is duplicated
// by hand in every stepping entry point of `Simulation` (`step`,
// `step_with_peers`, `step_parallel`, `step_with_peers_parallel`). If a
// refactor drops one of those arms, that entry point silently falls through
// to the default no-op `Agent` hooks (`decide`/`apply`/`step`) — exactly the
// bug class of F1. This test runs the SAME staged model through all four
// entry points with the same seed and requires the final state to be
// (a) bit-identical across entry points and (b) different from the initial
// state, which proves `stage` actually ran and did not degrade to a no-op.
// ---------------------------------------------------------------------------

/// Staged model that consumes RNG and mutates both agent and model state,
/// so a silent fall-through to no-op hooks is impossible to miss.
/// `Clone` + plain data: satisfies the extra bounds of the peers/parallel
/// entry points (`M::Agent: Clone + Send + Sync`, `M: Sync`).
#[derive(Clone)]
struct Marcher {
    counter: u64,
}

struct Field {
    agents: AgentSet<Marcher>,
    /// Model-level state mutated during stage 1 (exercises `&mut Model`).
    total: u64,
}

impl Agent for Marcher {
    type Model = Field;

    fn stage(&mut self, stage: usize, _id: AgentId, model: &mut Field, rng: &mut SimRng) {
        match stage {
            // Stage 0 draws from the shared step RNG: any entry point that
            // skipped it would leave the stream (and everyone after) off.
            0 => self.counter = self.counter.wrapping_add(rng.random_range(1..1_000)),
            // Stage 1 couples agents through the model, order-sensitively:
            // a reshuffle or a dropped sweep changes the result.
            1 => {
                self.counter = self.counter.wrapping_mul(3).wrapping_add(model.total);
                model.total = model.total.wrapping_add(self.counter);
            }
            _ => unreachable!("solo se configuraron 2 etapas"),
        }
    }
}

impl Model for Field {
    type Agent = Marcher;
    fn agents(&self) -> &AgentSet<Marcher> {
        &self.agents
    }
    fn agents_mut(&mut self) -> &mut AgentSet<Marcher> {
        &mut self.agents
    }
}

fn build_field(n: usize) -> Field {
    let mut agents = AgentSet::with_capacity(n);
    for _ in 0..n {
        agents.insert(Marcher { counter: 0 });
    }
    Field { agents, total: 0 }
}

/// Builds an identical staged simulation (same seed, `Activation::Staged(2)`
/// under Random ordering so the shuffle also consumes step RNG), advances it
/// 5 steps via `avanzar`, and returns the final per-agent counters.
fn correr_staged(avanzar: impl Fn(&mut Simulation<Field>)) -> Vec<u64> {
    let mut sim =
        Simulation::new(build_field(25), 42).with_schedule(Schedule::new(Activation::Staged(2)));
    for _ in 0..5 {
        avanzar(&mut sim);
    }
    sim.model.agents.iter().map(|(_, m)| m.counter).collect()
}

#[test]
fn staged_es_bit_identico_en_los_cuatro_entry_points() {
    let con_step = correr_staged(|sim| sim.step());

    // Guard against the no-op degradation: if `stage` never ran, every
    // counter would still be 0 (the initial state).
    assert_ne!(
        con_step,
        vec![0; 25],
        "el estado final debe diferir del inicial: `stage` tiene que haber corrido"
    );

    let con_peers = correr_staged(|sim| sim.step_with_peers());
    assert_eq!(
        con_step, con_peers,
        "step_with_peers debe despachar Staged igual que step"
    );

    #[cfg(feature = "parallel")]
    {
        let con_parallel = correr_staged(|sim| sim.step_parallel());
        assert_eq!(
            con_step, con_parallel,
            "step_parallel debe despachar Staged igual que step"
        );

        let con_peers_parallel = correr_staged(|sim| sim.step_with_peers_parallel());
        assert_eq!(
            con_step, con_peers_parallel,
            "step_with_peers_parallel debe despachar Staged igual que step"
        );
    }
}
