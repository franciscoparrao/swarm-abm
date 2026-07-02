//! **Sugarscape** (Epstein & Axtell, *Growing Artificial Societies*, 1996):
//! el modelo canónico de la economía basada en agentes.
//!
//! De una población casi homogénea emerge una distribución de riqueza
//! fuertemente sesgada (Gini alto) y la población se autorregula a la
//! capacidad de carga del paisaje. El modelo vive en
//! `swarm_models::sugarscape`; este binario es solo la CLI.
//!
//! Uso: `cargo run --release -p sugarscape [semilla] [--steps N] [--csv]`

use swarm_abm::prelude::*;
use swarm_models::sugarscape::{Sugarscape, SugarscapeConfig, build};

/// Valor de un flag `--nombre valor`, si está presente.
fn arg_value<T: std::str::FromStr>(args: &[String], name: &str) -> Option<T> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .and_then(|v| v.parse().ok())
}

/// Histograma ASCII de la distribución de riqueza (la firma sesgada).
fn print_wealth_histogram(model: &Sugarscape) {
    let mut wealth = model.wealths();
    if wealth.is_empty() {
        return;
    }
    wealth.sort_unstable();
    let max = *wealth.last().unwrap_or(&1).max(&1);
    let bins = 10usize;
    let mut counts = vec![0usize; bins];
    for &x in &wealth {
        let b = ((x as usize * bins) / (max as usize + 1)).min(bins - 1);
        counts[b] += 1;
    }
    let peak = counts.iter().copied().max().unwrap_or(1).max(1);
    println!("\nDistribución de riqueza (cola larga = pocos ricos):");
    for (i, &c) in counts.iter().enumerate() {
        let lo = i as u32 * max / bins as u32;
        let hi = (i as u32 + 1) * max / bins as u32;
        let bar = "█".repeat((c * 40 / peak).max(usize::from(c > 0)));
        println!("  [{lo:>3}–{hi:>3}] {bar} {c}");
    }
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

    let config = SugarscapeConfig::default();
    let model = build(config, seed);
    let pop0 = model.population();

    let mut sim = Simulation::new(model, seed).with_schedule(Schedule::new(Activation::Random));
    sim.add_reporter("poblacion", |m: &Sugarscape| m.population() as f64);
    sim.add_reporter("gini", Sugarscape::gini);
    sim.add_reporter("riqueza_media", Sugarscape::mean_wealth);

    sim.run(max_steps);

    if csv {
        print!("{}", sim.data().to_csv());
        return;
    }

    println!(
        "Sugarscape {}x{} | {pop0} agentes iniciales | growback {} | semilla {seed}",
        config.width, config.height, config.growback
    );
    let pop = sim.data().series("poblacion").unwrap_or(&[]);
    let gini = sim.data().series("gini").unwrap_or(&[]);
    let riqueza = sim.data().series("riqueza_media").unwrap_or(&[]);
    println!("\n  paso  poblacion  gini  riqueza_media");
    for (i, &step) in sim.data().steps().iter().enumerate() {
        if step % 10 == 0 || i + 1 == pop.len() {
            println!(
                "  {step:>4}  {:>9}  {:>4.3}  {:>13.2}",
                pop[i] as u64, gini[i], riqueza[i]
            );
        }
    }
    println!(
        "\nPoblación {pop0} → {} (autorregulada a la capacidad de carga)",
        sim.model.population()
    );
    println!(
        "Gini de riqueza: {:.3} (desigualdad emergente de agentes casi homogéneos)",
        sim.model.gini()
    );
    print_wealth_histogram(&sim.model);
}
