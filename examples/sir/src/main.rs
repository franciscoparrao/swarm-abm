//! SIR espacial: epidemia sobre una grilla totalmente ocupada.
//!
//! Cada celda tiene un agente fijo. Un susceptible con `k` vecinos (Moore)
//! infectados se contagia con probabilidad `1 - (1-beta)^k`; un infectado se
//! recupera con probabilidad `gamma` por paso. La simulación termina cuando
//! no quedan infectados.
//!
//! El modelo vive en `swarm_models::sir` (compartido con los bindings Python);
//! este binario es solo la interfaz de línea de comandos.
//!
//! Uso: `cargo run --release -p sir [semilla]`

use swarm_core::prelude::*;
use swarm_models::sir::{Sir, SirConfig, Status, build};

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

    let config = SirConfig {
        width,
        height,
        initial_infected: infected,
        ..Default::default()
    };

    // Modo ensemble: N réplicas en paralelo (batch runner del motor), reporta
    // la distribución del pico y del tamaño final de la epidemia.
    if let Some(runs) = arg_value::<u64>(&args, "--ensemble") {
        let outcomes = run_ensemble(
            seed..seed + runs,
            max_steps,
            move |s| {
                let mut sim = Simulation::new(build(config, s), s);
                sim.add_reporter("i", move |m: &Sir| m.fraction(Status::Infected));
                sim.add_reporter("r", move |m: &Sir| m.fraction(Status::Recovered));
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

    // Modo benchmark: sin reporters (Mesa tampoco mide métricas en su modo
    // --bench), solo la fase de simulación. Se reporta el mínimo de varias
    // repeticiones para filtrar ruido del SO (la corrida es determinista).
    if args.iter().any(|a| a == "--bench") {
        let reps: u32 = arg_value(&args, "--bench-reps").unwrap_or(5);
        let mut mejor_ms = f64::INFINITY;
        let mut pasos = 0;
        for _ in 0..reps {
            let mut sim = Simulation::new(build(config, seed), seed);
            let t0 = std::time::Instant::now();
            pasos = sim.run(max_steps);
            mejor_ms = mejor_ms.min(t0.elapsed().as_secs_f64() * 1000.0);
        }
        println!("steps,ms\n{pasos},{mejor_ms:.3}");
        return;
    }

    let mut sim = Simulation::new(build(config, seed), seed);
    sim.add_reporter("s", move |m: &Sir| m.fraction(Status::Susceptible));
    sim.add_reporter("i", move |m: &Sir| m.fraction(Status::Infected));
    sim.add_reporter("r", move |m: &Sir| m.fraction(Status::Recovered));

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

    println!("SIR espacial {width}x{height} (torus) | beta 0.08, gamma 0.1 | semilla {seed}");
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
