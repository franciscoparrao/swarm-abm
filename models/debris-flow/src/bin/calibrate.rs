//! Calibración del modelo de flujos de detritos por Differential Evolution
//! (DE/rand/1/bin), maximizando el IoU contra el ground truth de Copiapó.
//!
//! La clave que vuelve esto factible: el stack raster (1.2 GB) se carga UNA
//! vez y se comparte por `Arc` entre todas las evaluaciones; la población de
//! cada generación se evalúa en paralelo (rayon). Con el modelo Python
//! original (~130 s/corrida, sin reproducibilidad) un DE de cientos de
//! evaluaciones era impráctico; aquí cada corrida es ~2 s y la superficie
//! objetivo es estable (RNG sembrado).
//!
//! Uso: `cargo run --release -p debris-flow --bin calibrate -- \
//!       [--pop N] [--gens N] [--steps N] [--seed N] [--eval-seed N]`

use std::sync::Arc;
use std::time::Instant;

use debris_flow::{PARAM_DIMS, load, params_from_genes, run_and_score};
use rayon::prelude::*;
use swarm_abm::prelude::*;

// El espacio de búsqueda (`PARAM_DIMS`) y el mapeo genes→`Params`
// (`params_from_genes`) viven en `debris_flow::model`, compartidos con
// `bin/benchmark`.

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
    let pop: usize = arg_value(&args, "--pop").unwrap_or(24);
    let gens: usize = arg_value(&args, "--gens").unwrap_or(12);
    let steps: u64 = arg_value(&args, "--steps").unwrap_or(300);
    let n_agents: usize = arg_value(&args, "--agents").unwrap_or(50);
    let de_seed: u64 = arg_value(&args, "--seed").unwrap_or(1);
    // Semilla base de evaluación. Con `--eval-seeds k > 1` el objetivo es la
    // MEDIA del IoU sobre k semillas: calibración robusta que no sobreajusta
    // al ruido de una sola realización (colocación de agentes + softmax).
    let eval_seed: u64 = arg_value(&args, "--eval-seed").unwrap_or(42);
    let eval_seeds: u64 = arg_value(&args, "--eval-seeds").unwrap_or(1);
    let f = 0.6_f64; // factor de mutación
    let cr = 0.9_f64; // probabilidad de cruce

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

    let d = PARAM_DIMS.len();
    let evals_total = pop * (gens + 1);
    println!(
        "→ DE/rand/1/bin | {d} parámetros | población {pop} | {gens} generaciones \
         | {n_agents} agentes × {steps} pasos\n  ≈ {evals_total} evaluaciones (eval-seed {eval_seed})\n"
    );

    // IoU medio sobre `eval_seeds` semillas (robusto al ruido estocástico).
    let score = |x: &[f64]| -> f64 {
        let params = params_from_genes(x, n_agents);
        (0..eval_seeds)
            .map(|k| {
                run_and_score(
                    &layers,
                    &gt,
                    window,
                    pixel_size,
                    params.clone(),
                    eval_seed + k,
                    steps,
                )
                .iou
            })
            .sum::<f64>()
            / eval_seeds as f64
    };

    // Población inicial uniforme dentro de los rangos.
    let mut rng = rng_from_seed(de_seed);
    let mut population: Vec<Vec<f64>> = (0..pop)
        .map(|_| {
            PARAM_DIMS
                .iter()
                .map(|dm| rng.random_range(dm.lo..dm.hi))
                .collect()
        })
        .collect();

    let t0 = Instant::now();
    let mut fitness: Vec<f64> = population.par_iter().map(|x| score(x)).collect();

    let (mut best_i, _) = argmax(&fitness);
    println!(
        "gen  0/{gens}: IoU best {:.4} | media {:.4} | {:.0} evals/s",
        fitness[best_i],
        mean(&fitness),
        pop as f64 / t0.elapsed().as_secs_f64()
    );

    for generation in 1..=gens {
        let tg = Instant::now();
        // Construir los trial vectors (secuencial → DE determinista).
        let trials: Vec<Vec<f64>> = (0..pop)
            .map(|i| {
                let (r1, r2, r3) = three_distinct(pop, i, &mut rng);
                let jrand = rng.random_range(0..d);
                let mut trial = population[i].clone();
                for j in 0..d {
                    if j == jrand || rng.random_range(0.0..1.0) < cr {
                        let v = population[r1][j] + f * (population[r2][j] - population[r3][j]);
                        trial[j] = v.clamp(PARAM_DIMS[j].lo, PARAM_DIMS[j].hi);
                    }
                }
                trial
            })
            .collect();

        // Evaluar la generación completa en paralelo.
        let trial_fit: Vec<f64> = trials.par_iter().map(|x| score(x)).collect();

        // Selección greedy elemento a elemento.
        for i in 0..pop {
            if trial_fit[i] >= fitness[i] {
                population[i] = trials[i].clone();
                fitness[i] = trial_fit[i];
            }
        }
        let (bi, _) = argmax(&fitness);
        best_i = bi;
        println!(
            "gen {generation:2}/{gens}: IoU best {:.4} | media {:.4} | {:.0} evals/s",
            fitness[best_i],
            mean(&fitness),
            pop as f64 / tg.elapsed().as_secs_f64()
        );
    }

    let best = &population[best_i];
    let m = run_and_score(
        &layers,
        &gt,
        window,
        pixel_size,
        params_from_genes(best, n_agents),
        eval_seed,
        steps,
    );
    println!(
        "\n✓ Mejor IoU {:.4} en {:.0}s ({} evaluaciones)\n  precision {:.3} | recall {:.3} | F1 {:.3} | área pred {:.1} km² (GT {:.1} km²)",
        m.iou,
        t0.elapsed().as_secs_f64(),
        evals_total,
        m.precision,
        m.recall,
        m.f1,
        m.area_pred_km2,
        m.area_gt_km2
    );
    println!("  baseline (Optuna-T del original, esta superficie): IoU ~0.07–0.10\n");

    // JSON con los mejores parámetros.
    let mut entries: Vec<String> = PARAM_DIMS
        .iter()
        .zip(best)
        .map(|(dm, v)| format!("    \"{}\": {v}", dm.name))
        .collect();
    entries.push(format!("    \"n_rain_agents\": {n_agents}"));
    let json = format!(
        "{{\n  \"best_iou\": {:.6},\n  \"eval_seed\": {eval_seed},\n  \"steps\": {steps},\n  \"de\": {{ \"pop\": {pop}, \"gens\": {gens}, \"f\": {f}, \"cr\": {cr}, \"seed\": {de_seed} }},\n  \"parameters\": {{\n{}\n  }}\n}}\n",
        m.iou,
        entries.join(",\n")
    );
    let out = std::path::Path::new(&data_dir)
        .parent()
        .unwrap_or(std::path::Path::new("."))
        .join("best_params_de.json");
    if std::fs::write(&out, &json).is_ok() {
        println!("→ Parámetros guardados en {}", out.display());
    }
}

fn mean(v: &[f64]) -> f64 {
    v.iter().sum::<f64>() / v.len() as f64
}

fn argmax(v: &[f64]) -> (usize, f64) {
    v.iter().enumerate().fold(
        (0, f64::MIN),
        |acc, (i, &x)| if x > acc.1 { (i, x) } else { acc },
    )
}

/// Tres índices distintos entre sí y distintos de `i`.
fn three_distinct(n: usize, i: usize, rng: &mut SimRng) -> (usize, usize, usize) {
    let pick = |rng: &mut SimRng, excl: &[usize]| loop {
        let r = rng.random_range(0..n);
        if !excl.contains(&r) {
            return r;
        }
    };
    let r1 = pick(rng, &[i]);
    let r2 = pick(rng, &[i, r1]);
    let r3 = pick(rng, &[i, r1, r2]);
    (r1, r2, r3)
}
