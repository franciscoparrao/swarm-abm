//! Simulación del evento Copiapó (marzo 2015) con el modelo de flujos de
//! detritos sobre swarm-core, y validación espacial contra ground truth.
//!
//! Uso: `cargo run --release -p debris-flow -- [--data DIR] [--seed N]
//!       [--seeds N] [--agents N] [--steps N] [--temperature T]`

use std::path::PathBuf;
use std::time::Instant;

use debris_flow::{DebrisFlowModel, Params, evaluate, evaluate_masked, load};
use swarm_core::prelude::*;

fn arg_value<T: std::str::FromStr>(args: &[String], name: &str) -> Option<T> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .and_then(|v| v.parse().ok())
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let data_dir: PathBuf = arg_value(&args, "--data")
        .unwrap_or_else(|| PathBuf::from("models/debris-flow/data/copiapo"));
    let seed0: u64 = arg_value(&args, "--seed").unwrap_or(42);
    let n_seeds: u64 = arg_value(&args, "--seeds").unwrap_or(1);
    let steps: u64 = arg_value(&args, "--steps").unwrap_or(300);

    let preset = arg_value::<String>(&args, "--preset");
    let mut params = match preset.as_deref() {
        Some("18iters") => Params::preset_18iters(),
        Some("de") => Params::preset_de(),
        Some("chanaral") => Params::preset_chanaral(),
        Some("chanaral-enhanced") => Params::preset_chanaral_enhanced(),
        _ => Params::default(),
    };
    // El stack por defecto de Chañaral vive en otro directorio.
    let is_chanaral = matches!(preset.as_deref(), Some("chanaral" | "chanaral-enhanced"));
    let data_dir = if is_chanaral && !args.iter().any(|a| a == "--data") {
        PathBuf::from("models/debris-flow/data/chanaral")
    } else {
        data_dir
    };
    if let Some(n) = arg_value::<usize>(&args, "--agents") {
        params.n_rain_agents = n;
    }
    if let Some(t) = arg_value::<f64>(&args, "--temperature") {
        params.stochastic_temperature = t;
    }

    println!("→ Cargando stack desde {}...", data_dir.display());
    let t0 = Instant::now();
    let data = load(&data_dir).unwrap_or_else(|e| {
        eprintln!("Error cargando datos: {e}\n(corre primero prepare_data.py)");
        std::process::exit(1);
    });
    let (w, h) = (data.layers.dem.width(), data.layers.dem.height());
    println!(
        "  {w}x{h} ({:.1}M celdas, 10 capas) en {:.1}s",
        (w * h) as f64 / 1e6,
        t0.elapsed().as_secs_f64()
    );
    println!(
        "→ {} agentes de lluvia, {steps} pasos máx, T={:.3}\n",
        params.n_rain_agents, params.stochastic_temperature
    );

    let layers = std::sync::Arc::new(data.layers);
    let mut ious = Vec::new();
    for seed in seed0..seed0 + n_seeds {
        let t0 = Instant::now();
        let model = DebrisFlowModel::new(
            std::sync::Arc::clone(&layers),
            params.clone(),
            data.pixel_size,
            seed,
        );
        let mut sim =
            Simulation::new(model, seed).with_schedule(Schedule::new(Activation::Ordered));

        let pasos = sim.run(steps);
        let elapsed = t0.elapsed().as_secs_f64();

        // Con bbox (Chañaral) la evaluación se restringe al dominio urbano.
        let m = match &data.bbox {
            Some(bbox) => evaluate_masked(
                &sim.model.footprint,
                &data.ground_truth,
                bbox,
                data.window,
                data.pixel_size,
            ),
            None => evaluate(
                &sim.model.footprint,
                &data.ground_truth,
                data.window,
                data.pixel_size,
            ),
        };
        ious.push(m.iou);
        println!(
            "seed {seed}: IoU {:.4} | precision {:.3} | recall {:.3} | F1 {:.3} | \
             flujos {} | área pred {:.1} km² (GT {:.1} km²) | {pasos} pasos en {elapsed:.1}s",
            m.iou,
            m.precision,
            m.recall,
            m.f1,
            sim.model.flows_created,
            m.area_pred_km2,
            m.area_gt_km2
        );
        println!(
            "         movimientos/flujo {:.1} | muertes: volumen {} / atascado {}",
            sim.model.total_moves as f64 / sim.model.flows_created.max(1) as f64,
            sim.model.deaths_volume,
            sim.model.deaths_stuck
        );
        // Volcado del footprint (u8 fila-mayor) para diagnóstico espacial.
        if let Some(path) = arg_value::<String>(&args, "--dump") {
            let bytes: Vec<u8> = sim
                .model
                .footprint
                .iter()
                .map(|(_, &v)| u8::from(v))
                .collect();
            std::fs::write(&path, &bytes).expect("escribir footprint");
            println!("         footprint volcado en {path}");
        }
    }

    if ious.len() > 1 {
        let mean = ious.iter().sum::<f64>() / ious.len() as f64;
        let sd =
            (ious.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (ious.len() - 1) as f64).sqrt();
        let max = ious.iter().cloned().fold(f64::MIN, f64::max);
        println!(
            "\nIoU: media {mean:.4} ± {sd:.4} | máx {max:.4} (n={})",
            ious.len()
        );
        let referencia = match preset.as_deref() {
            Some("chanaral" | "chanaral-enhanced") => {
                "Referencia Python (Config B, mejor caso): IoU 0.4653"
            }
            _ => "Referencia Python (Optuna withT, 1 corrida): IoU 0.1344",
        };
        println!("{referencia}");
    }
}
