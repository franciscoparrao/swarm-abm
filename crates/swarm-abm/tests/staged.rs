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
