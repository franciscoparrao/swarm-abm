//! **Boids** (Reynolds, 1987): flocking emergente en el **espacio continuo**.
//!
//! Cada boid ajusta su velocidad por tres reglas locales sobre sus vecinos
//! dentro de un radio de percepción: separación (evitar choques), alineación
//! (igualar rumbo) y cohesión (acercarse al grupo). De reglas locales emerge
//! un comportamiento colectivo: el *parámetro de orden* de Vicsek
//! φ = ‖Σ v̂ᵢ‖ / N sube de ~0 (caótico) hacia ~1 (bandada alineada).
//!
//! Demuestra el tercer espacio del motor (continuo) y su índice por vecindad,
//! con el mismo trait `Agent`/`Model` que los modelos sobre grilla y grafo.
//!
//! Uso: `cargo run --release -p boids [semilla]`

use swarm_core::prelude::*;

struct Boid {
    pos: Vec2,
    vel: Vec2,
    point: PointId,
}

struct Params {
    radius: f64,
    sep_w: f64,
    ali_w: f64,
    coh_w: f64,
    max_speed: f64,
}

struct Flock {
    agents: AgentSet<Boid>,
    /// Índice espacial; el dato por punto es la velocidad (snapshot del paso).
    space: ContinuousSpace<Vec2>,
    params: Params,
}

impl Flock {
    /// Parámetro de orden de Vicsek: 0 = caótico, 1 = perfectamente alineado.
    fn order(&self) -> f64 {
        let n = self.agents.len();
        if n == 0 {
            return 0.0;
        }
        let sum = self
            .agents
            .iter()
            .fold(Vec2::ZERO, |acc, (_, b)| acc + b.vel.normalize_or_zero());
        sum.length() / n as f64
    }
}

impl Agent for Boid {
    type Model = Flock;

    fn step(&mut self, _id: AgentId, model: &mut Flock, _rng: &mut SimRng) {
        let p = &model.params;
        let (mut sep, mut ali, mut coh) = (Vec2::ZERO, Vec2::ZERO, Vec2::ZERO);
        let mut count = 0.0;
        // Vecinos del snapshot espacial (posiciones del inicio del paso).
        model
            .space
            .for_each_within(self.pos, p.radius, |pid, npos, &nvel, dist| {
                if pid == self.point {
                    return;
                }
                let to = model.space.delta(self.pos, npos); // vector hacia el vecino (toroidal)
                sep = sep - to * (1.0 / dist.max(0.01)); // separación: alejarse, más de los cercanos
                ali = ali + nvel; // alineación: igualar velocidades
                coh = coh + to; // cohesión: ir hacia el centro de masa local
                count += 1.0;
            });

        if count > 0.0 {
            let inv = 1.0 / count;
            let steer = sep.normalize_or_zero() * p.sep_w
                + (ali * inv).normalize_or_zero() * p.ali_w
                + (coh * inv).normalize_or_zero() * p.coh_w;
            self.vel = (self.vel + steer).clamp_length(p.max_speed);
        }
        self.pos = model.space.wrap(self.pos + self.vel);
    }
}

impl Model for Flock {
    type Agent = Boid;

    fn agents(&self) -> &AgentSet<Boid> {
        &self.agents
    }

    fn agents_mut(&mut self) -> &mut AgentSet<Boid> {
        &mut self.agents
    }

    /// Refresca el índice espacial con las posiciones/velocidades actuales
    /// antes de activar a los agentes (snapshot consistente del paso).
    fn before_step(&mut self, _rng: &mut SimRng) {
        let Self { agents, space, .. } = self;
        for (_, b) in agents.iter() {
            space.set_pos(b.point, b.pos);
            space[b.point] = b.vel;
        }
        space.reindex();
    }
}

fn build(n: usize, size: f64, seed: u64) -> Flock {
    let mut rng = rng_from_seed(seed);
    let mut agents = AgentSet::with_capacity(n);
    let mut space = ContinuousSpace::new(size, size, 6.0).with_torus(true);
    for _ in 0..n {
        let pos = Vec2::new(rng.random_range(0.0..size), rng.random_range(0.0..size));
        // Velocidad inicial al azar (rumbos dispersos → orden ≈ 0).
        let ang = rng.random_range(0.0..std::f64::consts::TAU);
        let vel = Vec2::new(ang.cos(), ang.sin());
        let point = space.add(pos, vel);
        agents.insert(Boid { pos, vel, point });
    }
    let params = Params {
        radius: 6.0,
        sep_w: 1.5,
        ali_w: 1.0,
        coh_w: 0.8,
        max_speed: 1.0,
    };
    Flock {
        agents,
        space,
        params,
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
    let args: Vec<String> = std::env::args().collect();
    let seed: u64 = args
        .get(1)
        .filter(|a| !a.starts_with("--"))
        .and_then(|s| s.parse().ok())
        .unwrap_or(42);

    // Modo benchmark: throughput (agente-pasos/s) del espacio CONTINUO. Mide
    // solo el stepping; densidad fija (área escala con n).
    if args.iter().any(|a| a == "--bench") {
        let n: usize = arg_value(&args, "--agents").unwrap_or(20_000);
        let steps: u64 = arg_value(&args, "--steps").unwrap_or(200);
        let size = (n as f64 / 0.06).sqrt(); // densidad ~0.06 agentes/celda²
        let mut sim = Simulation::new(build(n, size, seed), seed)
            .with_schedule(Schedule::new(Activation::Ordered));
        let t0 = std::time::Instant::now();
        let ran = sim.run(steps);
        let ms = t0.elapsed().as_secs_f64() * 1000.0;
        let aps = n as f64 * ran as f64 / (ms / 1000.0);
        println!("agents,steps,ms,agent_steps_per_s\n{n},{ran},{ms:.3},{aps:.0}");
        return;
    }

    let (n, size, steps) = (800usize, 120.0, 400u64);

    let mut sim = Simulation::new(build(n, size, seed), seed)
        .with_schedule(Schedule::new(Activation::Ordered));
    sim.add_reporter("orden", |f: &Flock| f.order());

    println!("Boids | {n} agentes en espacio continuo {size}×{size} (torus) | semilla {seed}");
    println!("Parámetro de orden de Vicsek (0 = caótico, 1 = bandada alineada):\n");
    let report_every = 50;
    for block in 0..(steps / report_every) {
        sim.run(report_every);
        let orden = sim
            .data()
            .series("orden")
            .and_then(|s| s.last().copied())
            .unwrap_or(0.0);
        println!("  paso {:>4}: orden {orden:.3}", (block + 1) * report_every);
    }
    let serie = sim.data().series("orden").unwrap_or(&[]);
    println!(
        "\norden inicial {:.3} → final {:.3}  (el flocking emerge de reglas locales)",
        serie.first().copied().unwrap_or(0.0),
        serie.last().copied().unwrap_or(0.0),
    );
}
