//! Calibración del modelo de detritos **enriquecido** (entrainment, Voellmy,
//! inercia) en Chañaral, por Differential Evolution. Responde la pregunta:
//! ¿la física extra supera el baseline de Config B (IoU 0.4653)?
//!
//! Objetivo = IoU medio sobre K semillas, evaluado en el bbox urbano
//! (`evaluate_masked`). Los mejores parámetros se re-evalúan en 8 semillas
//! frescas (validación fuera de muestra) para descartar sobreajuste.
//!
//! Uso: `cargo run --release -p debris-flow --bin calibrate_chanaral --
//! [--pop N] [--gens N] [--steps N] [--eval-seeds K] [--seed N]`

use std::sync::Arc;
use std::time::Instant;

use debris_flow::{
    Bounds, DebrisFlowModel, Method, PARAM_DIMS_CHANARAL, evaluate_masked, load,
    params_chanaral_from_genes,
};
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
        .unwrap_or_else(|| "models/debris-flow/data/chanaral".into());
    let pop: usize = arg_value(&args, "--pop").unwrap_or(28);
    let gens: usize = arg_value(&args, "--gens").unwrap_or(25);
    let steps: u64 = arg_value(&args, "--steps").unwrap_or(500);
    let eval_seeds: u64 = arg_value(&args, "--eval-seeds").unwrap_or(3);
    let de_seed: u64 = arg_value(&args, "--seed").unwrap_or(1);
    let budget = pop * (gens + 1);

    println!("→ Cargando stack Chañaral desde {data_dir}...");
    let data = load(std::path::Path::new(&data_dir)).unwrap_or_else(|e| {
        eprintln!("Error: {e}\n(corre primero prepare_chanaral.py)");
        std::process::exit(1);
    });
    let layers = Arc::new(data.layers);
    let gt = Arc::new(data.ground_truth);
    let bbox = Arc::new(data.bbox.expect("Chañaral debe traer bbox.f32"));
    let (window, pixel_size) = (data.window, data.pixel_size);

    // Una evaluación = correr el modelo enriquecido y medir IoU en el bbox.
    let score_seed = |x: &[f64], seed: u64| -> f64 {
        let params = params_chanaral_from_genes(x);
        let model = DebrisFlowModel::new(Arc::clone(&layers), params, pixel_size, seed);
        let mut sim =
            Simulation::new(model, seed).with_schedule(Schedule::new(Activation::Ordered));
        sim.run(steps);
        evaluate_masked(&sim.model.footprint, &gt, &bbox, window, pixel_size).iou
    };
    // Objetivo robusto: media − desviación estándar sobre `eval_seeds`
    // semillas. Penalizar la varianza evita óptimos frágiles (que brillan en
    // una semilla y colapsan en otra) — el modo de fallo observado con la
    // física enriquecida, que tiene más grados de libertad.
    let objective = |x: &[f64]| -> f64 {
        let v: Vec<f64> = (42..42 + eval_seeds).map(|s| score_seed(x, s)).collect();
        let m = v.iter().sum::<f64>() / v.len() as f64;
        let sd = (v.iter().map(|x| (x - m).powi(2)).sum::<f64>() / v.len() as f64).sqrt();
        m - sd
    };

    let bounds = Bounds {
        lo: PARAM_DIMS_CHANARAL.iter().map(|d| d.lo).collect(),
        hi: PARAM_DIMS_CHANARAL.iter().map(|d| d.hi).collect(),
    };

    println!(
        "→ DE | {} parámetros (8 base + 6 físicos) | población {pop} × {gens} gen ≈ {budget} evals \
         | {steps} pasos | objetivo {eval_seeds}-seed\n  baseline Config B: IoU 0.4653\n",
        PARAM_DIMS_CHANARAL.len()
    );

    let t0 = Instant::now();
    let mut rng = rng_from_seed(de_seed);
    let out = Method::De.run(&objective, &bounds, budget, &mut rng);
    println!(
        "✓ Mejor IoU (objetivo {eval_seeds}-seed) {:.4} en {:.0}s ({} evals)",
        out.best_f,
        t0.elapsed().as_secs_f64(),
        out.evals
    );

    // Validación fuera de muestra (8 semillas frescas) + métricas completas.
    let params = params_chanaral_from_genes(&out.best_x);
    let mets: Vec<_> = (100..108)
        .map(|s| {
            let model = DebrisFlowModel::new(Arc::clone(&layers), params.clone(), pixel_size, s);
            let mut sim =
                Simulation::new(model, s).with_schedule(Schedule::new(Activation::Ordered));
            sim.run(steps);
            evaluate_masked(&sim.model.footprint, &gt, &bbox, window, pixel_size)
        })
        .collect();
    let ious: Vec<f64> = mets.iter().map(|m| m.iou).collect();
    let mean = ious.iter().sum::<f64>() / ious.len() as f64;
    let sd =
        (ious.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (ious.len() - 1) as f64).sqrt();
    let max = ious.iter().cloned().fold(f64::MIN, f64::max);
    let prec = mets.iter().map(|m| m.precision).sum::<f64>() / mets.len() as f64;
    let rec = mets.iter().map(|m| m.recall).sum::<f64>() / mets.len() as f64;
    let f1 = mets.iter().map(|m| m.f1).sum::<f64>() / mets.len() as f64;
    println!(
        "\nValidación fuera de muestra (8 semillas frescas):\n  IoU {mean:.4} ± {sd:.4} | máx \
         {max:.4} | precision {prec:.3} | recall {rec:.3} | F1 {f1:.3}"
    );
    let verdict = if max > 0.4653 {
        "✓ SUPERA el baseline Config B (0.4653)"
    } else {
        "✗ no supera el baseline Config B (0.4653)"
    };
    println!("  {verdict}");

    // Parámetros calibrados (incluida la física enriquecida).
    let mut lines: Vec<String> = PARAM_DIMS_CHANARAL
        .iter()
        .zip(&out.best_x)
        .map(|(d, v)| format!("    \"{}\": {v}", d.name))
        .collect();
    lines.push(format!("    \"iou_oos_mean\": {mean:.6}"));
    lines.push(format!("    \"iou_oos_max\": {max:.6}"));
    let json = format!("{{\n{}\n}}\n", lines.join(",\n"));
    let out_path = std::path::Path::new(&data_dir)
        .parent()
        .unwrap_or(std::path::Path::new("."))
        .join("best_params_chanaral_enhanced.json");
    if std::fs::write(&out_path, &json).is_ok() {
        println!("→ Parámetros guardados en {}", out_path.display());
    }
}
