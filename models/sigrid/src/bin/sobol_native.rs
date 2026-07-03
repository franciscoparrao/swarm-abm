//! Análisis de sensibilidad global de Sobol' **nativo**: muestreo Saltelli,
//! evaluación y estimación de S1/ST (con bootstrap) todo en Rust, vía
//! `swarm_abm::experiment`. Reemplaza el arnés híbrido `sobol_rust.py`
//! (`Isla_Riesco/experiments/sobol_rust.py`): antes SALib (Python) hacía el
//! muestreo y el análisis, y solo la evaluación —la parte cara— corría en
//! Rust (`sobol-eval`). Ahora no hay Python ni SALib en el camino: una sola
//! invocación determinista, misma semilla ⇒ mismo resultado bit a bit.
//!
//! `sobol-eval` (el evaluador por CSV) se mantiene: lo sigue usando
//! `Isla_Riesco/experiments/parity.py` para la comparación punto a punto
//! contra Mesa, que no es un diseño de muestreo sino puntos factoriales fijos.
//!
//! Uso: `cargo run --release -p sigrid --features experiment,parallel \
//!       --bin sobol-native -- --n 512 --days 30 --seed 1 --n-boot 500`

use std::sync::Mutex;
use std::time::Instant;

use sigrid::{Params, SigridModel, build};
use swarm_abm::experiment::{ParamSpec, sobol};
use swarm_abm::prelude::*;

fn arg<T: std::str::FromStr>(args: &[String], name: &str) -> Option<T> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .and_then(|v| v.parse().ok())
}

/// Problema Sobol: los mismos 6 parámetros/rangos que `sobol_pilot.py`
/// (PROBLEM), para que los resultados sean comparables con las corridas Mesa
/// y con el historial documentado en `PARITY.md`.
fn specs() -> Vec<ParamSpec> {
    vec![
        ParamSpec::new("sheep_density", 0.96, 1.5),
        ParamSpec::new("fox_predation_effectiveness", 0.08, 0.26),
        ParamSpec::new("n_dogs", 0.0, 2.0),
        ParamSpec::new("hare_density", 0.0, 0.35),
        ParamSpec::new("chilla_density", 0.0, 18.7),
        ParamSpec::new("lamb_proportion", 0.1, 0.3),
    ]
}

fn params_from_point(point: &[f64]) -> Params {
    Params {
        sheep_density: point[0],
        fox_predation_effectiveness: point[1],
        n_dogs: point[2].round().max(0.0) as usize,
        hare_density: point[3],
        chilla_density: point[4],
        lamb_proportion: point[5],
        ..Params::default()
    }
}

fn build_sim(point: &[f64], seed: u64) -> Simulation<SigridModel> {
    Simulation::new(build(params_from_point(point), seed), seed)
        .with_schedule(Schedule::new(Activation::Random))
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let n: usize = arg(&args, "--n").unwrap_or(64);
    let days: u64 = arg(&args, "--days").unwrap_or(30);
    let seed: u64 = arg(&args, "--seed").unwrap_or(1);
    let n_boot: usize = arg(&args, "--n-boot").unwrap_or(500);

    let specs = specs();
    let d = specs.len();
    let n_evals = n * (d + 2);
    println!(
        "[sobol-native] N={n} -> {n_evals} evaluaciones ({d} params) | {days} días | \
         seed={seed} | n_boot={n_boot}"
    );

    // Recolecta los Y crudos vía el propio closure `outcome` (side effect):
    // no cambia el resultado del análisis, solo permite reportar la
    // distribución de salida (media/sd/min/max) además de S1/ST.
    let y_log: Mutex<Vec<f64>> = Mutex::new(Vec::with_capacity(n_evals));
    let outcome = |sim: &Simulation<SigridModel>| {
        let lr = sim.model.loss_rate_pct();
        y_log.lock().expect("mutex de Y no envenenado").push(lr);
        lr
    };

    let t0 = Instant::now();
    let design = sobol(&specs, n);
    let result = design.run(seed, days * 24, n_boot, build_sim, outcome);
    let wall = t0.elapsed().as_secs_f64();

    println!(
        "[sobol-native] eval+análisis: {wall:.2}s | {:.1} ms/eval",
        wall / n_evals as f64 * 1000.0
    );

    println!("\n[sobol-native] índices (S1 = primer orden, ST = total):");
    println!(
        "  {:<32} {:>8} {:>18} {:>8} {:>18}",
        "param", "S1", "S1 IC95%", "ST", "ST IC95%"
    );
    let mut order: Vec<usize> = (0..d).collect();
    order.sort_by(|&a, &b| result.st[b].total_cmp(&result.st[a]));
    for i in order {
        println!(
            "  {:<32} {:>8.3} [{:>6.3}, {:>6.3}] {:>8.3} [{:>6.3}, {:>6.3}]",
            result.names[i],
            result.s1[i],
            result.s1_conf[i].0,
            result.s1_conf[i].1,
            result.st[i],
            result.st_conf[i].0,
            result.st_conf[i].1,
        );
    }
    let sum_st: f64 = result.st.iter().sum();
    println!("\n[sobol-native] sum(ST)={sum_st:.2}");

    let ys = y_log.into_inner().expect("mutex de Y no envenenado");
    let mean = ys.iter().sum::<f64>() / ys.len() as f64;
    let var = ys.iter().map(|y| (y - mean).powi(2)).sum::<f64>() / ys.len() as f64;
    let (min, max) = ys
        .iter()
        .fold((f64::MAX, f64::MIN), |(lo, hi), &y| (lo.min(y), hi.max(y)));
    println!(
        "[sobol-native] loss_rate Y: mean {mean:.2}% sd {:.2} min {min:.2} max {max:.2} (n={})",
        var.sqrt(),
        ys.len()
    );
}
