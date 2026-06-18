//! SIR espacial: epidemia sobre una grilla totalmente ocupada.
//!
//! Cada celda tiene un agente fijo. Un susceptible con `k` vecinos (Moore)
//! infectados se contagia con probabilidad `1 - (1-beta)^k`; un infectado se
//! recupera con probabilidad `gamma` por paso. La simulación termina cuando
//! no quedan infectados.
//!
//! Uso: `cargo run --release -p sir [semilla]`

use swarm_core::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Status {
    Susceptible,
    Infected,
    Recovered,
}

struct Person {
    pos: Pos,
    status: Status,
}

struct Sir {
    agents: AgentSet<Person>,
    grid: Grid2D<Option<AgentId>>,
    beta: f64,
    gamma: f64,
}

impl Sir {
    fn count(&self, status: Status) -> usize {
        self.agents
            .iter()
            .filter(|(_, p)| p.status == status)
            .count()
    }

    fn infected_neighbors(&self, pos: Pos) -> u32 {
        self.grid
            .neighbors(pos, Neighborhood::Moore)
            .filter(|(_, cell)| {
                cell.is_some_and(|id| {
                    self.agents
                        .get(id)
                        .is_some_and(|n| n.status == Status::Infected)
                })
            })
            .count() as u32
    }
}

impl Agent for Person {
    type Model = Sir;

    fn step(&mut self, _id: AgentId, model: &mut Sir, rng: &mut SimRng) {
        match self.status {
            Status::Susceptible => {
                let k = model.infected_neighbors(self.pos);
                if k > 0 {
                    let p = 1.0 - (1.0 - model.beta).powi(k as i32);
                    if rng.random_bool(p) {
                        self.status = Status::Infected;
                    }
                }
            }
            Status::Infected => {
                if rng.random_bool(model.gamma) {
                    self.status = Status::Recovered;
                }
            }
            Status::Recovered => {}
        }
    }
}

impl Model for Sir {
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

fn build(width: usize, height: usize, initial_infected: usize, seed: u64) -> Sir {
    let mut rng = rng_from_seed(seed ^ 0x0510_5EED);
    let mut agents = AgentSet::with_capacity(width * height);
    let mut grid: Grid2D<Option<AgentId>> = Grid2D::new(width, height).with_torus(true);

    let mut ids = Vec::with_capacity(width * height);
    for y in 0..height {
        for x in 0..width {
            let pos = Pos::new(x, y);
            let id = agents.insert(Person {
                pos,
                status: Status::Susceptible,
            });
            grid[pos] = Some(id);
            ids.push(id);
        }
    }

    ids.shuffle(&mut rng);
    for &id in ids.iter().take(initial_infected) {
        if let Some(p) = agents.get_mut(id) {
            p.status = Status::Infected;
        }
    }

    Sir {
        agents,
        grid,
        beta: 0.08,
        gamma: 0.1,
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
    let width: usize = arg_value(&args, "--width").unwrap_or(100);
    let height: usize = arg_value(&args, "--height").unwrap_or(100);
    let infected: usize = arg_value(&args, "--infected").unwrap_or(10);
    let max_steps: u64 = arg_value(&args, "--steps").unwrap_or(500);

    // Modo ensemble: N réplicas en paralelo (batch runner del motor), reporta
    // la distribución del pico y del tamaño final de la epidemia.
    if let Some(runs) = arg_value::<u64>(&args, "--ensemble") {
        let total = (width * height) as f64;
        let outcomes = run_ensemble(
            seed..seed + runs,
            max_steps,
            move |s| {
                let mut sim = Simulation::new(build(width, height, infected, s), s);
                sim.add_reporter("i", move |m: &Sir| m.count(Status::Infected) as f64 / total);
                sim.add_reporter("r", move |m: &Sir| {
                    m.count(Status::Recovered) as f64 / total
                });
                sim
            },
            |sim: &Simulation<Sir>| {
                let peak = sim
                    .data()
                    .series("i")
                    .map_or(0.0, |s| s.iter().copied().fold(0.0, f64::max));
                let r_final = sim
                    .data()
                    .series("r")
                    .and_then(|s| s.last().copied())
                    .unwrap_or(0.0);
                (peak, r_final)
            },
        );
        let mean =
            |f: fn(&(f64, f64)) -> f64| outcomes.iter().map(f).sum::<f64>() / outcomes.len() as f64;
        let sd = |f: fn(&(f64, f64)) -> f64, m: f64| {
            (outcomes.iter().map(|o| (f(o) - m).powi(2)).sum::<f64>() / outcomes.len() as f64)
                .sqrt()
        };
        let (mp, mr) = (mean(|o| o.0), mean(|o| o.1));
        println!(
            "SIR ensemble: {runs} réplicas {width}x{height} | pico {:.1}% ± {:.1} | R final {:.1}% ± {:.1}",
            mp * 100.0,
            sd(|o| o.0, mp) * 100.0,
            mr * 100.0,
            sd(|o| o.1, mr) * 100.0,
        );
        return;
    }

    let model = build(width, height, infected, seed);
    let n = model.agents.len() as f64;

    let mut sim = Simulation::new(model, seed);

    // Modo benchmark: sin reporters (Mesa tampoco mide métricas en su modo
    // --bench), solo la fase de simulación. Se reporta el mínimo de varias
    // repeticiones para filtrar ruido del SO (la corrida es determinista).
    if args.iter().any(|a| a == "--bench") {
        let reps: u32 = arg_value(&args, "--bench-reps").unwrap_or(5);
        let mut mejor_ms = f64::INFINITY;
        let mut pasos = 0;
        for _ in 0..reps {
            let mut sim = Simulation::new(build(width, height, infected, seed), seed);
            let t0 = std::time::Instant::now();
            pasos = sim.run(max_steps);
            mejor_ms = mejor_ms.min(t0.elapsed().as_secs_f64() * 1000.0);
        }
        println!("steps,ms\n{pasos},{mejor_ms:.3}");
        return;
    }

    sim.add_reporter("s", move |m: &Sir| m.count(Status::Susceptible) as f64 / n);
    sim.add_reporter("i", move |m: &Sir| m.count(Status::Infected) as f64 / n);
    sim.add_reporter("r", move |m: &Sir| m.count(Status::Recovered) as f64 / n);

    let pasos = sim.run(max_steps);

    if csv {
        print!("{}", sim.data().to_csv());
        return;
    }

    let i_serie = sim.data().series("i").unwrap_or(&[]);
    let (paso_pico, pico) =
        i_serie.iter().enumerate().fold(
            (0, 0.0),
            |acc, (i, &v)| if v > acc.1 { (i, v) } else { acc },
        );
    let r_final = sim
        .data()
        .series("r")
        .and_then(|s| s.last().copied())
        .unwrap_or(0.0);

    println!("SIR espacial 100x100 (torus) | beta 0.08, gamma 0.1 | semilla {seed}");
    println!("Epidemia terminada en {pasos} pasos");
    println!(
        "Pico de infectados: {:.1}% en el paso {paso_pico}",
        pico * 100.0
    );
    println!("Tamaño final de la epidemia (R): {:.1}%", r_final * 100.0);

    println!("\n  paso      S      I      R");
    let s = sim.data().series("s").unwrap_or(&[]);
    let r = sim.data().series("r").unwrap_or(&[]);
    for (idx, &step) in sim.data().steps().iter().enumerate() {
        if step % 25 == 0 || idx + 1 == s.len() {
            println!(
                "  {step:>4}  {:>5.3}  {:>5.3}  {:>5.3}",
                s[idx], i_serie[idx], r[idx]
            );
        }
    }
}
