//! Modelo de segregación de Schelling (1971).
//!
//! Agentes de dos grupos sobre una grilla toroidal. Un agente está conforme
//! si la fracción de vecinos de su mismo grupo es ≥ `tolerance`; si no, se
//! muda a una celda vacía al azar. El resultado clásico: segregación
//! emergente incluso con tolerancias bajas.
//!
//! El modelo vive en `swarm_models::schelling`; este binario es solo la CLI.
//!
//! Uso: `cargo run --release -p schelling [semilla]`

use swarm_core::prelude::*;
use swarm_models::schelling::{Schelling, SchellingConfig, build};

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
    let width: usize = arg_value(&args, "--width").unwrap_or(50);
    let height: usize = arg_value(&args, "--height").unwrap_or(50);

    let config = SchellingConfig {
        width,
        height,
        ..Default::default()
    };

    // Modo benchmark: pasos fijos (sin corte por convergencia), sin reporters,
    // cronometrando solo el stepping. Reporta steps,ms como sir --bench.
    if args.iter().any(|a| a == "--bench") {
        let bench_steps: u64 = arg_value(&args, "--steps").unwrap_or(100);
        let mut sim = Simulation::new(build(config, seed), seed);
        let t0 = std::time::Instant::now();
        for _ in 0..bench_steps {
            sim.step();
        }
        let ms = t0.elapsed().as_secs_f64() * 1000.0;
        println!("steps,ms\n{bench_steps},{ms:.3}");
        return;
    }

    let model = build(config, seed);
    let n = model.population();

    let mut sim = Simulation::new(model, seed);
    sim.add_reporter("fraccion_conforme", Schelling::fraction_happy);
    sim.add_reporter("similitud_media", Schelling::mean_similarity);

    let pasos = sim.run(max_steps);

    if csv {
        print!("{}", sim.data().to_csv());
        return;
    }

    println!(
        "Schelling {}x{} (torus) | {n} agentes | tolerancia {} | semilla {seed}",
        config.width, config.height, config.tolerance
    );
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
