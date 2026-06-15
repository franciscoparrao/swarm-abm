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

use debris_flow::{Params, load, run_and_score};
use rayon::prelude::*;
use swarm_core::prelude::*;

/// Parámetro calibrable: nombre, rango y setter sobre `Params`.
struct Dim {
    name: &'static str,
    lo: f64,
    hi: f64,
    set: fn(&mut Params, f64),
}

/// Los 15 parámetros continuos del modelo, con los mismos rangos que la
/// calibración Optuna original (`calibrate_copiapo_optuna_with_T.py`).
/// `n_rain_agents` y `footprint_radius` quedan fijos (como en el original).
const DIMS: &[Dim] = &[
    Dim {
        name: "rain_threshold",
        lo: 0.01,
        hi: 0.3,
        set: |p, v| p.rain_threshold = v,
    },
    Dim {
        name: "sediment_threshold",
        lo: 0.01,
        hi: 0.3,
        set: |p, v| p.sediment_threshold = v,
    },
    Dim {
        name: "susceptibility_threshold",
        lo: 0.05,
        hi: 0.4,
        set: |p, v| p.susceptibility_threshold = v,
    },
    Dim {
        name: "friction_coefficient",
        lo: 0.01,
        hi: 0.1,
        set: |p, v| p.friction_coefficient = v,
    },
    Dim {
        name: "coastal_slope_threshold",
        lo: 0.01,
        hi: 0.15,
        set: |p, v| p.coastal_slope_threshold = v,
    },
    Dim {
        name: "coastal_spread_factor",
        lo: 2.0,
        hi: 5.0,
        set: |p, v| p.coastal_spread_factor = v,
    },
    Dim {
        name: "coastal_volume_threshold",
        lo: 0.1,
        hi: 1.5,
        set: |p, v| p.coastal_volume_threshold = v,
    },
    Dim {
        name: "volume_decay_flat",
        lo: 0.95,
        hi: 0.995,
        set: |p, v| p.volume_decay_flat = v,
    },
    Dim {
        name: "volume_decay_slope",
        lo: 0.98,
        hi: 0.998,
        set: |p, v| p.volume_decay_slope = v,
    },
    Dim {
        name: "stream_attraction_weight",
        lo: 1.0,
        hi: 10.0,
        set: |p, v| p.stream_attraction_weight = v,
    },
    Dim {
        name: "max_velocity",
        lo: 10.0,
        hi: 30.0,
        set: |p, v| p.max_velocity = v,
    },
    Dim {
        name: "min_velocity",
        lo: 0.1,
        hi: 1.0,
        set: |p, v| p.min_velocity = v,
    },
    Dim {
        name: "critical_slope",
        lo: 0.01,
        hi: 0.1,
        set: |p, v| p.critical_slope = v,
    },
    Dim {
        name: "slope_acceleration_factor",
        lo: 1.0,
        hi: 2.0,
        set: |p, v| p.slope_acceleration_factor = v,
    },
    Dim {
        name: "stochastic_temperature",
        lo: 0.0,
        hi: 2.0,
        set: |p, v| p.stochastic_temperature = v,
    },
];

fn arg_value<T: std::str::FromStr>(args: &[String], name: &str) -> Option<T> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .and_then(|v| v.parse().ok())
}

/// Convierte un vector de genes en `Params` (resto = defaults del modelo).
fn to_params(x: &[f64], n_agents: usize) -> Params {
    let mut p = Params {
        n_rain_agents: n_agents,
        ..Params::default()
    };
    for (dim, &v) in DIMS.iter().zip(x) {
        (dim.set)(&mut p, v);
    }
    p
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

    let d = DIMS.len();
    let evals_total = pop * (gens + 1);
    println!(
        "→ DE/rand/1/bin | {d} parámetros | población {pop} | {gens} generaciones \
         | {n_agents} agentes × {steps} pasos\n  ≈ {evals_total} evaluaciones (eval-seed {eval_seed})\n"
    );

    // IoU medio sobre `eval_seeds` semillas (robusto al ruido estocástico).
    let score = |x: &[f64]| -> f64 {
        let params = to_params(x, n_agents);
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
            DIMS.iter()
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
                        trial[j] = v.clamp(DIMS[j].lo, DIMS[j].hi);
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
        to_params(best, n_agents),
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
    let mut entries: Vec<String> = DIMS
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
