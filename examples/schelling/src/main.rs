//! Modelo de segregación de Schelling (1971).
//!
//! Agentes de dos grupos sobre una grilla toroidal. Un agente está conforme
//! si la fracción de vecinos de su mismo grupo es ≥ `tolerance`; si no, se
//! muda a una celda vacía al azar. El resultado clásico: segregación
//! emergente incluso con tolerancias bajas.
//!
//! Uso: `cargo run --release -p schelling [semilla]`

use swarm_core::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Group {
    Red,
    Blue,
}

struct Person {
    pos: Pos,
    group: Group,
}

struct Schelling {
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

    fn fraction_happy(&self) -> f64 {
        if self.agents.is_empty() {
            return 1.0;
        }
        let happy = self.agents.iter().filter(|(_, p)| self.is_happy(p)).count();
        happy as f64 / self.agents.len() as f64
    }

    fn mean_similarity(&self) -> f64 {
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
        let i = rng.random_range(0..model.empties.len());
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

fn build(width: usize, height: usize, density: f64, tolerance: f64, seed: u64) -> Schelling {
    let mut rng = rng_from_seed(seed ^ 0x05E7_0F5E_ED00);
    let mut grid: Grid2D<Option<AgentId>> = Grid2D::new(width, height).with_torus(true);
    let mut agents = AgentSet::new();

    let mut coords: Vec<Pos> = grid.iter().map(|(p, _)| p).collect();
    coords.shuffle(&mut rng);

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

/// Valor de un flag `--nombre valor`, si está presente.
fn arg_value<T: std::str::FromStr>(args: &[String], name: &str) -> Option<T> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .and_then(|v| v.parse().ok())
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let seed: u64 = args
        .first()
        .filter(|a| !a.starts_with("--"))
        .and_then(|s| s.parse().ok())
        .unwrap_or(42);
    let csv = args.iter().any(|a| a == "--csv");
    let max_steps: u64 = arg_value(&args, "--steps").unwrap_or(200);

    let model = build(50, 50, 0.85, 0.375, seed);
    let n = model.agents.len();

    let mut sim = Simulation::new(model, seed);
    sim.add_reporter("fraccion_conforme", Schelling::fraction_happy);
    sim.add_reporter("similitud_media", Schelling::mean_similarity);

    let pasos = sim.run(max_steps);

    if csv {
        print!("{}", sim.data().to_csv());
        return;
    }

    println!("Schelling 50x50 (torus) | {n} agentes | tolerancia 0.375 | semilla {seed}");
    println!(
        "Convergió: {} ({pasos} pasos)",
        if sim.model.finished() { "sí" } else { "no" }
    );

    let conforme = sim.data().series("fraccion_conforme").unwrap_or(&[]);
    let similitud = sim.data().series("similitud_media").unwrap_or(&[]);
    println!("\n  paso  conforme  similitud_media");
    for (i, &step) in sim.data().steps().iter().enumerate() {
        if step % 10 == 0 || i + 1 == conforme.len() {
            println!("  {step:>4}  {:>8.3}  {:>15.3}", conforme[i], similitud[i]);
        }
    }
}
