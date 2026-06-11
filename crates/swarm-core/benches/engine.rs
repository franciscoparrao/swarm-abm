//! Benchmarks del motor con criterion.
//!
//! - `walkers_step`: escalamiento de agentes móviles (10k → 1M) en un paso.
//! - `sir_full_50x50`: epidemia SIR completa (macro-benchmark end-to-end).
//! - `life_step_200x200`: paso con activación simultánea (decide + apply).
//! - `diffuse_256x256`: la primitiva de campo escalar.

use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use std::hint::black_box;
use swarm_core::prelude::*;

// ---------- caminantes aleatorios (escalamiento) ----------

struct Walker {
    pos: Pos,
}

struct WalkerWorld {
    agents: AgentSet<Walker>,
    grid: Grid2D<u8>,
}

impl Agent for Walker {
    type Model = WalkerWorld;

    fn step(&mut self, _id: AgentId, world: &mut WalkerWorld, rng: &mut SimRng) {
        let destinos: Vec<Pos> = world
            .grid
            .neighbor_positions(self.pos, Neighborhood::Moore)
            .collect();
        self.pos = destinos[rng.random_range(0..destinos.len())];
    }
}

impl Model for WalkerWorld {
    type Agent = Walker;

    fn agents(&self) -> &AgentSet<Walker> {
        &self.agents
    }

    fn agents_mut(&mut self) -> &mut AgentSet<Walker> {
        &mut self.agents
    }
}

fn build_walkers(n: usize, side: usize, seed: u64) -> Simulation<WalkerWorld> {
    let mut rng = rng_from_seed(seed);
    let mut agents = AgentSet::with_capacity(n);
    for _ in 0..n {
        let pos = Pos::new(rng.random_range(0..side), rng.random_range(0..side));
        agents.insert(Walker { pos });
    }
    let grid = Grid2D::new(side, side).with_torus(true);
    Simulation::new(WalkerWorld { agents, grid }, seed)
}

fn bench_walkers(c: &mut Criterion) {
    let mut g = c.benchmark_group("walkers_step");
    for &n in &[10_000usize, 100_000, 1_000_000] {
        g.throughput(Throughput::Elements(n as u64));
        if n >= 1_000_000 {
            g.sample_size(20);
        }
        g.bench_function(n.to_string(), |b| {
            let mut sim = build_walkers(n, 1000, 42);
            b.iter(|| {
                sim.step();
                black_box(sim.step_count())
            });
        });
    }
    g.finish();
}

// ---------- SIR completo (mismo modelo que examples/sir) ----------

#[derive(Clone, Copy, PartialEq, Eq)]
enum Status {
    S,
    I,
    R,
}

struct SirPerson {
    pos: Pos,
    status: Status,
}

struct Sir {
    agents: AgentSet<SirPerson>,
    grid: Grid2D<Option<AgentId>>,
    beta: f64,
    gamma: f64,
}

impl Agent for SirPerson {
    type Model = Sir;

    fn step(&mut self, _id: AgentId, model: &mut Sir, rng: &mut SimRng) {
        match self.status {
            Status::S => {
                let k = model
                    .grid
                    .neighbors(self.pos, Neighborhood::Moore)
                    .filter(|(_, cell)| {
                        cell.is_some_and(|id| {
                            model.agents.get(id).is_some_and(|n| n.status == Status::I)
                        })
                    })
                    .count() as i32;
                if k > 0 && rng.random_bool(1.0 - (1.0 - model.beta).powi(k)) {
                    self.status = Status::I;
                }
            }
            Status::I => {
                if rng.random_bool(model.gamma) {
                    self.status = Status::R;
                }
            }
            Status::R => {}
        }
    }
}

impl Model for Sir {
    type Agent = SirPerson;

    fn agents(&self) -> &AgentSet<SirPerson> {
        &self.agents
    }

    fn agents_mut(&mut self) -> &mut AgentSet<SirPerson> {
        &mut self.agents
    }

    fn finished(&self) -> bool {
        self.agents.iter().all(|(_, p)| p.status != Status::I)
    }
}

fn build_sir(side: usize, infected: usize, seed: u64) -> Simulation<Sir> {
    let mut rng = rng_from_seed(seed ^ 0x0510_5EED);
    let mut agents = AgentSet::with_capacity(side * side);
    let mut grid = Grid2D::new(side, side).with_torus(true);
    let mut ids = Vec::with_capacity(side * side);
    for y in 0..side {
        for x in 0..side {
            let pos = Pos::new(x, y);
            let id = agents.insert(SirPerson {
                pos,
                status: Status::S,
            });
            grid[pos] = Some(id);
            ids.push(id);
        }
    }
    ids.shuffle(&mut rng);
    for &id in ids.iter().take(infected) {
        if let Some(p) = agents.get_mut(id) {
            p.status = Status::I;
        }
    }
    Simulation::new(
        Sir {
            agents,
            grid,
            beta: 0.08,
            gamma: 0.1,
        },
        seed,
    )
}

fn bench_sir(c: &mut Criterion) {
    let mut g = c.benchmark_group("sir_full");
    g.sample_size(30);
    g.bench_function("50x50", |b| {
        b.iter(|| {
            let mut sim = build_sir(50, 5, 7);
            sim.run(300);
            black_box(sim.step_count())
        });
    });
    g.finish();
}

// ---------- Game of Life (activación simultánea) ----------

struct Cell {
    pos: Pos,
    alive: bool,
    next: bool,
}

struct Life {
    agents: AgentSet<Cell>,
    grid: Grid2D<bool>,
}

impl Agent for Cell {
    type Model = Life;

    fn decide(&mut self, _id: AgentId, model: &Life, _rng: &mut SimRng) {
        let vivos = model
            .grid
            .neighbors(self.pos, Neighborhood::Moore)
            .filter(|&(_, &v)| v)
            .count();
        self.next = matches!((self.alive, vivos), (true, 2) | (true, 3) | (false, 3));
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

fn build_life(side: usize, seed: u64) -> Simulation<Life> {
    let mut rng = rng_from_seed(seed);
    let mut agents = AgentSet::with_capacity(side * side);
    let mut grid = Grid2D::fill(side, side, false).with_torus(true);
    for y in 0..side {
        for x in 0..side {
            let pos = Pos::new(x, y);
            let alive = rng.random_bool(0.3);
            agents.insert(Cell {
                pos,
                alive,
                next: false,
            });
            grid[pos] = alive;
        }
    }
    Simulation::new(Life { agents, grid }, seed)
        .with_schedule(Schedule::new(Activation::Simultaneous))
}

fn bench_life(c: &mut Criterion) {
    let mut g = c.benchmark_group("life_step");
    let n = 200 * 200;
    g.throughput(Throughput::Elements(n as u64));
    g.bench_function("200x200", |b| {
        let mut sim = build_life(200, 11);
        b.iter(|| {
            sim.step();
            black_box(sim.step_count())
        });
    });
    g.finish();
}

// ---------- diffuse ----------

fn bench_diffuse(c: &mut Criterion) {
    let mut grid = Grid2D::fill(256, 256, 0.0_f64).with_torus(true);
    grid[Pos::new(128, 128)] = 1_000_000.0;
    c.bench_function("diffuse_256x256", |b| {
        b.iter(|| {
            grid.diffuse(0.5, Neighborhood::Moore);
            black_box(grid.total())
        });
    });
}

criterion_group!(benches, bench_walkers, bench_sir, bench_life, bench_diffuse);
criterion_main!(benches);
