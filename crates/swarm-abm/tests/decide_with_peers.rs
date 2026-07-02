//! Tests de `Agent::decide_with_peers` / `Simulation::step_with_peers`
//! (P1-2 de `docs/AUDIT.md`): la fase `decide` puede leer el estado de
//! **otros** agentes sin que el usuario lo duplique a mano en el entorno —
//! la limitación que SIGRID resolvió duplicando posiciones en una grilla
//! auxiliar.
//!
//! El modelo de prueba (`Node`) es deliberadamente el caso que `decide` a
//! secas NO puede resolver: cada agente necesita la suma del `value` de
//! **todos** los demás, un dato que no vive en ningún entorno espacial.

use swarm_abm::prelude::*;

#[derive(Clone)]
struct Node {
    value: u64,
    next_sum: u64,
}

struct World {
    agents: AgentSet<Node>,
}

impl Agent for Node {
    type Model = World;

    // Deliberadamente NO se implementa `decide` (queda el no-op por
    // defecto): la visibilidad de pares solo debe llegar por
    // `decide_with_peers`, nunca "gratis" a través de `decide`.
    fn decide_with_peers(
        &mut self,
        _id: AgentId,
        _model: &World,
        peers: &AgentSet<Self>,
        _rng: &mut SimRng,
    ) {
        // Suma el `value` de TODOS los agentes (incluido self) tal como
        // estaban al empezar la fase — exactamente el dato que `decide` a
        // secas no puede leer sin que el modelo lo duplique a mano.
        // `wrapping_add`: con muchos pasos la suma crece geométricamente
        // (cada agente pasa a valer la suma total) y desborda u64 rápido;
        // wrapping mantiene el test determinista sin ese ruido.
        self.next_sum = peers
            .iter()
            .fold(0u64, |acc, (_, n)| acc.wrapping_add(n.value));
    }

    fn apply(&mut self, _id: AgentId, _model: &mut World, _rng: &mut SimRng) {
        self.value = self.next_sum;
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

fn build(values: &[u64]) -> World {
    let mut agents = AgentSet::with_capacity(values.len());
    for &v in values {
        agents.insert(Node {
            value: v,
            next_sum: 0,
        });
    }
    World { agents }
}

fn values(w: &World) -> Vec<u64> {
    w.agents.iter().map(|(_, n)| n.value).collect()
}

#[test]
fn decide_with_peers_ve_el_estado_de_otros_agentes() {
    let mut sim = Simulation::new(build(&[1, 2, 3, 4]), 0)
        .with_schedule(Schedule::new(Activation::Simultaneous));
    sim.step_with_peers();
    // Cada agente ve la suma total (1+2+3+4=10) tomada al empezar la fase —
    // no un acumulado a medio actualizar, que dependería del orden y
    // rompería la semántica "simultánea".
    assert_eq!(values(&sim.model), vec![10, 10, 10, 10]);
}

#[test]
fn decide_a_secas_no_hereda_la_visibilidad_de_peers() {
    // Control: `Node` solo implementa `decide_with_peers`, no `decide`. Con
    // el `step()` normal (que llama a `decide`, no a `decide_with_peers`),
    // `next_sum` se queda en el no-op por defecto (0) y `apply` lo
    // materializa. Confirma que ambos caminos son independientes: la
    // visibilidad de pares no se "filtra" a `decide` a secas.
    let mut sim = Simulation::new(build(&[1, 2, 3, 4]), 0)
        .with_schedule(Schedule::new(Activation::Simultaneous));
    sim.step();
    assert_eq!(values(&sim.model), vec![0, 0, 0, 0]);
}

#[test]
fn step_with_peers_es_determinista() {
    let correr = || {
        let mut sim = Simulation::new(build(&[3, 1, 4, 1, 5, 9, 2, 6]), 7)
            .with_schedule(Schedule::new(Activation::Simultaneous));
        sim.run_with_peers(5);
        values(&sim.model)
    };
    assert_eq!(correr(), correr());
}

#[cfg(feature = "parallel")]
#[test]
fn step_with_peers_paralelo_es_bit_identico_al_secuencial() {
    let seed = 99;
    let vals: Vec<u64> = (0..500).collect();

    let mut seq =
        Simulation::new(build(&vals), seed).with_schedule(Schedule::new(Activation::Simultaneous));
    seq.run_with_peers(10);

    let mut par =
        Simulation::new(build(&vals), seed).with_schedule(Schedule::new(Activation::Simultaneous));
    par.run_with_peers_parallel(10);

    assert_eq!(
        values(&seq.model),
        values(&par.model),
        "decide_with_peers paralelo debe coincidir bit a bit con el secuencial"
    );
}
