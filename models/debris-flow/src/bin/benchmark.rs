//! Benchmark comparativo de metaheurísticas para calibrar el modelo de
//! flujos de detritos — el estudio que el paper original no pudo hacer por
//! costo: cada método se corre N veces (semillas independientes del
//! optimizador) con el MISMO presupuesto de evaluaciones, para comparar
//! distribuciones de IoU con potencia estadística.
//!
//! Paraleliza a nivel de tarea (método × corrida): las M×N optimizaciones
//! son independientes y saturan los cores de forma uniforme; cada optimizador
//! corre secuencial por dentro (así SA, intrínsecamente secuencial, encaja
//! sin caso especial). El stack raster se comparte por `Arc`.
//!
//! Salida: `models/debris-flow/data/benchmark.csv` (method,run,best_iou,evals)
//! → `validation/calibration_benchmark.py` hace los tests (Friedman/Wilcoxon).
//!
//! Uso: `cargo run --release -p debris-flow --bin benchmark -- \
//!       [--runs N] [--budget N] [--steps N] [--eval-seeds K]`

use std::sync::Arc;
use std::time::Instant;

use debris_flow::{Bounds, Method, PARAM_DIMS, load, params_from_genes, run_and_score};
use rayon::prelude::*;
use swarm_core::prelude::*;

fn arg_value<T: std::str::FromStr>(args: &[String], name: &str) -> Option<T> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .and_then(|v| v.parse().ok())
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let data_dir = arg_value::<String>(&args, "--data")
        .unwrap_or_else(|| "models/debris-flow/data/copiapo".into());
    let runs: u64 = arg_value(&args, "--runs").unwrap_or(8);
    let budget: usize = arg_value(&args, "--budget").unwrap_or(150);
    let steps: u64 = arg_value(&args, "--steps").unwrap_or(200);
    let n_agents: usize = arg_value(&args, "--agents").unwrap_or(50);
    // Semillas de evaluación del objetivo (robustez); 1 = single-seed.
    let eval_seeds: u64 = arg_value(&args, "--eval-seeds").unwrap_or(1);
    let eval_seed0: u64 = 42;

    println!("→ Cargando stack desde {data_dir}...");
    let t_load = Instant::now();
    let data = load(std::path::Path::new(&data_dir)).unwrap_or_else(|e| {
        eprintln!("Error: {e}");
        std::process::exit(1);
    });
    let layers = Arc::new(data.layers);
    let gt = Arc::new(data.ground_truth);
    let (window, pixel_size) = (data.window, data.pixel_size);
    println!("  cargado en {:.1}s", t_load.elapsed().as_secs_f64());

    let bounds = Bounds {
        lo: PARAM_DIMS.iter().map(|d| d.lo).collect(),
        hi: PARAM_DIMS.iter().map(|d| d.hi).collect(),
    };

    // Objetivo: IoU medio sobre `eval_seeds` semillas (idéntico para todos).
    let objective = |x: &[f64]| -> f64 {
        let params = params_from_genes(x, n_agents);
        (0..eval_seeds)
            .map(|k| {
                run_and_score(
                    &layers,
                    &gt,
                    window,
                    pixel_size,
                    params.clone(),
                    eval_seed0 + k,
                    steps,
                )
                .iou
            })
            .sum::<f64>()
            / eval_seeds as f64
    };

    let methods = Method::ALL;
    let total_tasks = methods.len() as u64 * runs;
    let sims_per_task = budget as u64 * eval_seeds;
    println!(
        "→ {} métodos × {runs} corridas = {total_tasks} tareas | presupuesto {budget} evals/corrida \
         | {n_agents} agentes × {steps} pasos | objetivo {eval_seeds}-seed",
        methods.len()
    );
    println!("  ≈ {} simulaciones totales\n", total_tasks * sims_per_task);

    // Lista de tareas (método, corrida); se evalúan todas en paralelo.
    let tasks: Vec<(Method, u64)> = methods
        .iter()
        .flat_map(|&m| (0..runs).map(move |r| (m, r)))
        .collect();

    let t0 = Instant::now();
    let mut results: Vec<(Method, u64, f64, usize)> = tasks
        .par_iter()
        .map(|&(method, run)| {
            // Semilla del optimizador derivada de método+corrida (reproducible
            // y distinta por celda); el objetivo usa siempre las mismas
            // semillas de evaluación, así la comparación es justa.
            let opt_seed = 1000 * (method as u64 + 1) + run;
            let mut rng = rng_from_seed(opt_seed);
            let out = method.run(&objective, &bounds, budget, &mut rng);
            (method, run, out.best_f, out.evals)
        })
        .collect();
    results.sort_by_key(|r| (r.0 as usize, r.1));

    let elapsed = t0.elapsed().as_secs_f64();
    println!(
        "✓ {total_tasks} optimizaciones en {elapsed:.0}s ({:.1} tareas/s)\n",
        total_tasks as f64 / elapsed
    );

    // CSV crudo para el análisis estadístico en Python.
    let mut csv = String::from("method,run,best_iou,evals\n");
    for &(m, r, f, e) in &results {
        csv.push_str(&format!("{},{r},{f:.6},{e}\n", m.name()));
    }
    let out_csv = std::path::Path::new(&data_dir)
        .parent()
        .unwrap_or(std::path::Path::new("."))
        .join("benchmark.csv");
    std::fs::write(&out_csv, &csv).expect("escribir benchmark.csv");
    println!("→ CSV: {}", out_csv.display());

    // Resumen por método (media ± sd del IoU sobre las corridas).
    println!("\nMétodo  IoU medio ± sd        mejor    (n={runs})");
    for &m in &methods {
        let xs: Vec<f64> = results.iter().filter(|r| r.0 == m).map(|r| r.2).collect();
        let mean = xs.iter().sum::<f64>() / xs.len() as f64;
        let sd = (xs.iter().map(|x| (x - mean).powi(2)).sum::<f64>()
            / (xs.len() - 1).max(1) as f64)
            .sqrt();
        let best = xs.iter().cloned().fold(f64::MIN, f64::max);
        println!("  {:<4}  {mean:.4} ± {sd:.4}      {best:.4}", m.name());
    }

    // Re-evaluar el mejor global de cada método con K=8 semillas de validación
    // (incluye semillas fuera del ajuste) → generalización honesta.
    println!("\nValidación del mejor de cada método (IoU medio sobre 8 semillas frescas):");
    for &m in &methods {
        // Recuperar el mejor de las corridas reconstruyendo su solución: como
        // sólo guardamos best_f, re-corremos la mejor corrida para su best_x.
        let (best_run, _) = results
            .iter()
            .filter(|r| r.0 == m)
            .map(|r| (r.1, r.2))
            .fold((0u64, f64::MIN), |a, b| if b.1 > a.1 { b } else { a });
        let opt_seed = 1000 * (m as u64 + 1) + best_run;
        let mut rng = rng_from_seed(opt_seed);
        let out = m.run(&objective, &bounds, budget, &mut rng);
        let params = params_from_genes(&out.best_x, n_agents);
        // 8 semillas de validación frescas (100..108), fuera del ajuste.
        let mets: Vec<_> = (100..108)
            .map(|s| run_and_score(&layers, &gt, window, pixel_size, params.clone(), s, steps))
            .collect();
        let ious: Vec<f64> = mets.iter().map(|m| m.iou).collect();
        let mean = ious.iter().sum::<f64>() / ious.len() as f64;
        let sd =
            (ious.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (ious.len() - 1) as f64).sqrt();
        let f1 = mets.iter().map(|m| m.f1).sum::<f64>() / mets.len() as f64;
        println!(
            "  {:<4}  IoU {mean:.4} ± {sd:.4} | F1 medio {f1:.3}",
            m.name()
        );
    }
}
