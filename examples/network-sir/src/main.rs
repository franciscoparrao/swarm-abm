//! Contagio SIR **sobre una red** (no sobre una grilla): cada nodo es una
//! persona, las aristas son contactos. Demuestra que el mismo motor que corre
//! modelos espaciales (`Grid2D`) corre modelos sobre grafos (`Graph`), sin
//! cambiar el trait `Agent`/`Model`.
//!
//! Compara la dinámica epidémica sobre tres topologías canónicas —aleatoria
//! (Erdős–Rényi), mundo pequeño (Watts–Strogatz) y libre de escala
//! (Barabási–Albert)— con el mismo grado medio. El resultado clásico: las
//! redes con hubs (scale-free) y atajos (small-world) propagan más rápido y
//! más lejos.
//!
//! Uso: `cargo run --release -p network-sir [semilla]`

use swarm_abm::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Status {
    Susceptible,
    Infected,
    Recovered,
}

struct Person {
    node: NodeId,
    status: Status,
}

struct NetworkSir {
    agents: AgentSet<Person>,
    /// Topología de contactos (solo aristas).
    net: Graph<()>,
    /// Mapeo nodo → agente que lo habita (indexado por `NodeId::as_usize`).
    node_agent: Vec<AgentId>,
    beta: f64,
    gamma: f64,
}

impl NetworkSir {
    fn count(&self, status: Status) -> usize {
        self.agents
            .iter()
            .filter(|(_, p)| p.status == status)
            .count()
    }

    fn infected_neighbors(&self, node: NodeId) -> u32 {
        self.net
            .neighbors(node)
            .filter(|&nb| {
                let aid = self.node_agent[nb.as_usize()];
                self.agents
                    .get(aid)
                    .is_some_and(|p| p.status == Status::Infected)
            })
            .count() as u32
    }
}

impl Agent for Person {
    type Model = NetworkSir;

    fn step(&mut self, _id: AgentId, model: &mut NetworkSir, rng: &mut SimRng) {
        match self.status {
            Status::Susceptible => {
                let k = model.infected_neighbors(self.node);
                if k > 0 && bernoulli(rng, 1.0 - (1.0 - model.beta).powi(k as i32)) {
                    self.status = Status::Infected;
                }
            }
            Status::Infected => {
                if bernoulli(rng, model.gamma) {
                    self.status = Status::Recovered;
                }
            }
            Status::Recovered => {}
        }
    }
}

impl Model for NetworkSir {
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

/// Construye el modelo: un agente por nodo del grafo, `initial` infectados al
/// azar. El grafo aporta la topología; el mapeo nodo→agente se llena aquí.
fn build(net: Graph<()>, initial: usize, seed: u64) -> NetworkSir {
    let mut agents = AgentSet::with_capacity(net.node_count());
    let mut node_agent = Vec::with_capacity(net.node_count());
    for node in net.node_ids() {
        let id = agents.insert(Person {
            node,
            status: Status::Susceptible,
        });
        node_agent.push(id); // node_ids() va en orden 0..n
    }
    // Infectar `initial` agentes al azar (RNG sembrado, reproducible).
    let mut rng = rng_from_seed(seed ^ 0x531A_5EED);
    let mut ids: Vec<AgentId> = agents.ids();
    shuffle(&mut rng, &mut ids);
    for &id in ids.iter().take(initial) {
        if let Some(p) = agents.get_mut(id) {
            p.status = Status::Infected;
        }
    }
    NetworkSir {
        agents,
        net,
        node_agent,
        beta: 0.06,
        gamma: 0.1,
    }
}

fn run_topology(name: &str, net: Graph<()>, seed: u64) {
    let n = net.node_count();
    let mean_deg = 2.0 * net.edge_count() as f64 / n as f64;
    let total = n as f64;

    let mut sim =
        Simulation::new(build(net, 5, seed), seed).with_schedule(Schedule::new(Activation::Random));
    sim.add_reporter("i", move |m: &NetworkSir| {
        m.count(Status::Infected) as f64 / total
    });
    sim.add_reporter("r", move |m: &NetworkSir| {
        m.count(Status::Recovered) as f64 / total
    });
    let steps = sim.run(400);

    let i = sim.data().series("i").unwrap_or(&[]);
    let (paso_pico, pico) = i
        .iter()
        .enumerate()
        .fold((0, 0.0), |a, (k, &v)| if v > a.1 { (k, v) } else { a });
    let r_final = sim
        .data()
        .series("r")
        .and_then(|s| s.last().copied())
        .unwrap_or(0.0);
    println!(
        "  {name:<14} grado medio {mean_deg:>4.1} | pico {:>5.1}% (paso {paso_pico:>3}) | \
         epidemia final {:>5.1}% | {steps} pasos",
        pico * 100.0,
        r_final * 100.0
    );
}

/// Valor de un flag `--nombre valor`, si está presente.
fn arg_value<T: std::str::FromStr>(args: &[String], name: &str) -> Option<T> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .and_then(|v| v.parse().ok())
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let seed: u64 = args
        .get(1)
        .filter(|a| !a.starts_with("--"))
        .and_then(|s| s.parse().ok())
        .unwrap_or(42);

    // Modo benchmark: throughput (agente-pasos/s) del espacio de GRAFO. Mide
    // solo el stepping (construcción fuera del cronómetro).
    if args.iter().any(|a| a == "--bench") {
        let nodes: usize = arg_value(&args, "--nodes").unwrap_or(50_000);
        let steps: u64 = arg_value(&args, "--steps").unwrap_or(200);
        let net = Graph::watts_strogatz(nodes, 4, 0.1, &mut rng_from_seed(seed));
        let mut sim = Simulation::new(build(net, 5, seed), seed)
            .with_schedule(Schedule::new(Activation::Random));
        let t0 = std::time::Instant::now();
        let ran = sim.run(steps);
        let ms = t0.elapsed().as_secs_f64() * 1000.0;
        let aps = nodes as f64 * ran as f64 / (ms / 1000.0);
        println!("nodes,steps,ms,agent_steps_per_s\n{nodes},{ran},{ms:.3},{aps:.0}");
        return;
    }

    let (n, k) = (4000usize, 4);

    println!(
        "SIR sobre redes | {n} nodos, grado medio ~{} | beta 0.06, gamma 0.1 | semilla {seed}\n",
        2 * k
    );
    // Tres topologías con grado medio comparable (~8).
    run_topology(
        "aleatoria",
        Graph::erdos_renyi(
            n,
            2.0 * k as f64 / (n as f64 - 1.0),
            &mut rng_from_seed(seed),
        ),
        seed,
    );
    run_topology(
        "mundo-pequeño",
        Graph::watts_strogatz(n, k, 0.1, &mut rng_from_seed(seed)),
        seed,
    );
    run_topology(
        "libre-de-escala",
        Graph::barabasi_albert(n, k, &mut rng_from_seed(seed)),
        seed,
    );
}
