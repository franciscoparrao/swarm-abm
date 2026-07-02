//! **Juego de la Vida** de Conway: el modelo canónico de **activación
//! simultánea**. Cada celda es un agente; en `decide` lee el estado vivo/muerto
//! de sus 8 vecinas (modelo inmutable) y calcula su próximo estado; en `apply`
//! lo materializa. Todas las celdas avanzan a la vez.
//!
//! Sirve de banco para el **decide paralelo**: con la feature `parallel`, la
//! fase `decide` se reparte entre hilos y da un resultado bit-idéntico al
//! secuencial (lo garantiza el RNG por-agente y el modelo inmutable en `decide`).
//!
//! Uso:
//! ```text
//! cargo run --release -p life [semilla]
//! cargo run --release -p life -- --bench [--parallel] --width 2000 --height 2000 --steps 100
//! RAYON_NUM_THREADS=4 cargo run --release -p life -- --bench --parallel ...
//! ```

use swarm_abm::prelude::*;

struct Cell {
    pos: Pos,
    alive: bool,
    next: bool,
}

struct Life {
    agents: AgentSet<Cell>,
    grid: Grid2D<bool>,
    /// Costo de decisión por agente: nº de iteraciones de cómputo extra en
    /// `decide`. `0` = Vida pura (memory-bound). Valores altos representan
    /// agentes con lógica de decisión cara (utilidad, percepción, optimización
    /// local) — donde el paralelismo intra-paso rinde.
    work: usize,
}

impl Life {
    fn alive_count(&self) -> usize {
        self.agents.iter().filter(|(_, c)| c.alive).count()
    }
}

impl Agent for Cell {
    type Model = Life;

    fn decide(&mut self, _id: AgentId, model: &Life, _rng: &mut SimRng) {
        let mut n = 0u8;
        for (_, &a) in model.grid.neighbors(self.pos, Neighborhood::Moore) {
            n += u8::from(a);
        }
        // Carga de decisión sintética (compute-bound): representa el costo de
        // una decisión cara por agente. `std::hint::black_box` evita que el
        // optimizador la elimine.
        if model.work > 0 {
            let mut acc = (self.pos.x as f64 + 1.0).sqrt();
            for k in 0..model.work {
                acc += (acc + k as f64).sin().abs().sqrt();
            }
            if std::hint::black_box(acc) < 0.0 {
                n = n.wrapping_add(1);
            }
        }
        // Regla B3/S23: nace con 3 vecinas, sobrevive con 2 o 3.
        self.next = matches!((self.alive, n), (true, 2) | (true, 3) | (false, 3));
    }

    fn apply(&mut self, _id: AgentId, model: &mut Life, _rng: &mut SimRng) {
        self.alive = self.next;
        model.grid[self.pos] = self.alive;
    }
}

impl Model for Life {
    type Agent = Cell;
    fn agents(&self) -> &AgentSet<Cell> {
        &self.agents
    }
    fn agents_mut(&mut self) -> &mut AgentSet<Cell> {
        &mut self.agents
    }
}

fn build(width: usize, height: usize, seed: u64, work: usize) -> Life {
    let mut rng = rng_from_seed(seed ^ 0x11FE_0000);
    let mut grid = Grid2D::fill(width, height, false).with_torus(true);
    let mut agents = AgentSet::with_capacity(width * height);
    for y in 0..height {
        for x in 0..width {
            let pos = Pos::new(x, y);
            let alive = rng.random_bool(0.3);
            grid[pos] = alive;
            agents.insert(Cell {
                pos,
                alive,
                next: alive,
            });
        }
    }
    Life { agents, grid, work }
}

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
    let width: usize = arg_value(&args, "--width").unwrap_or(200);
    let height: usize = arg_value(&args, "--height").unwrap_or(200);
    let steps: u64 = arg_value(&args, "--steps").unwrap_or(100);
    let work: usize = arg_value(&args, "--work").unwrap_or(0);

    if args.iter().any(|a| a == "--bench") {
        let parallel = args.iter().any(|a| a == "--parallel");
        let mut sim = Simulation::new(build(width, height, seed, work), seed)
            .with_schedule(Schedule::new(Activation::Simultaneous));
        let t0 = std::time::Instant::now();
        if parallel {
            #[cfg(feature = "parallel")]
            sim.run_parallel(steps);
            #[cfg(not(feature = "parallel"))]
            sim.run(steps);
        } else {
            sim.run(steps);
        }
        let ms = t0.elapsed().as_secs_f64() * 1000.0;
        let mode = if parallel { "parallel" } else { "seq" };
        let threads = std::env::var("RAYON_NUM_THREADS").unwrap_or_else(|_| "?".into());
        println!(
            "mode,threads,cells,steps,ms\n{mode},{threads},{},{steps},{ms:.3}",
            width * height
        );
        return;
    }

    let mut sim = Simulation::new(build(width, height, seed, work), seed)
        .with_schedule(Schedule::new(Activation::Simultaneous));
    let vivas0 = sim.model.alive_count();
    sim.run(steps);
    println!(
        "Game of Life {width}x{height} (torus) | semilla {seed} | {steps} pasos\nVivas: {vivas0} → {}",
        sim.model.alive_count()
    );
}
