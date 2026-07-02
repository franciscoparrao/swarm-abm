//! Corre una simulación SIGRID y reporta el loss rate de ovejas.
//!
//! Uso: `cargo run --release -p sigrid -- [--days N] [--seed N] [--seeds N]
//!       [--sheep-density D] [--fox-eff E] [--dogs N] [--hare-density D]
//!       [--chilla-density D] [--lamb-prop P]`

use std::time::Instant;

use sigrid::{Params, build};
use swarm_abm::prelude::*;

fn arg<T: std::str::FromStr>(args: &[String], name: &str) -> Option<T> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .and_then(|v| v.parse().ok())
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let days: u64 = arg(&args, "--days").unwrap_or(30);
    let seed0: u64 = arg(&args, "--seed").unwrap_or(1000);
    let n_seeds: u64 = arg(&args, "--seeds").unwrap_or(1);

    let mut params = Params::default();
    if let Some(v) = arg(&args, "--sheep-density") {
        params.sheep_density = v;
    }
    if let Some(v) = arg(&args, "--fox-eff") {
        params.fox_predation_effectiveness = v;
    }
    if let Some(v) = arg(&args, "--dogs") {
        params.n_dogs = v;
    }
    if let Some(v) = arg(&args, "--hare-density") {
        params.hare_density = v;
    }
    if let Some(v) = arg(&args, "--chilla-density") {
        params.chilla_density = v;
    }
    if let Some(v) = arg(&args, "--lamb-prop") {
        params.lamb_proportion = v;
    }

    let m0 = build(params, seed0);
    let n_sheep = m0.agents().iter().filter(|(_, a)| a.alive).count();
    println!(
        "SIGRID | {:.0}x{:.0} m | densidad oveja {:.2}/ha | zorro_eff {:.2} | \
         perros {} | chilla {:.1}/km² | liebre {:.2}/ha | corderos {:.0}% | {days} días",
        params.width,
        params.height,
        params.sheep_density,
        params.fox_predation_effectiveness,
        params.n_dogs,
        params.chilla_density,
        params.hare_density,
        params.lamb_proportion * 100.0,
    );
    println!("agentes iniciales (vivos): {n_sheep}\n");

    let mut losses = Vec::new();
    let t0 = Instant::now();
    for s in 0..n_seeds {
        let seed = seed0 + s;
        let mut sim = Simulation::new(build(params, seed), seed)
            .with_schedule(Schedule::new(Activation::Random));
        sim.run(days * 24);
        let lr = sim.model.loss_rate_pct();
        losses.push(lr);
        println!(
            "  semilla {seed}: loss_rate {lr:.2}% | matadas {} (zorro {}, chilla {}) | liebres {} | intentos {}",
            sim.model.sheep_killed,
            sim.model.sheep_killed_by_fox,
            sim.model.sheep_killed_by_chilla,
            sim.model.hares_killed,
            sim.model.predation_attempts,
        );
    }
    let ms = t0.elapsed().as_secs_f64() * 1000.0;
    let mean = losses.iter().sum::<f64>() / losses.len() as f64;
    let var = losses.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / losses.len() as f64;
    println!(
        "\nloss_rate medio {mean:.2}% (sd {:.2}) sobre {n_seeds} semillas | {ms:.0} ms",
        var.sqrt()
    );
}
