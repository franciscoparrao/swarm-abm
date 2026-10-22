//! ¿Responde la huella al gatillo externo ([`Layers::trigger`])?
//!
//! Replica el experimento de sensibilidad a la lluvia (nowcast,
//! `sensibilidad_lluvia.py`) pero por la vía del gatillo: misma semilla y mismo
//! stack, escalando la exceedancia `E`. Separa los dos efectos posibles:
//!
//! - **inicio**: cuántas celdas-hora superan `E >= 1` (y por lo tanto cuántos
//!   flujos nacen);
//! - **magnitud**: cuánto crece la huella, que depende del volumen inicial y
//!   por eso puede quedar plana aunque el inicio sí responda.
//!
//! Uso: `cargo run --release -p debris-flow --bin trigger_sensitivity --
//!       [--data DIR] [--preset chanaral-enhanced] [--seed N] [--steps N]
//!       [--id-threshold MM_H]`
use std::sync::Arc;

use debris_flow::{DebrisFlowModel, HOURLY_PATTERNS, Params, load};
use swarm_abm::prelude::*;

fn arg<T: std::str::FromStr>(args: &[String], name: &str) -> Option<T> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .and_then(|v| v.parse().ok())
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let preset = arg::<String>(&args, "--preset").unwrap_or_else(|| "chanaral-enhanced".into());
    let data_dir: std::path::PathBuf = arg(&args, "--data").unwrap_or_else(|| {
        match preset.as_str() {
            "chanaral-enhanced" => "models/debris-flow/data/chanaral_twi",
            "chanaral" => "models/debris-flow/data/chanaral",
            _ => "models/debris-flow/data/copiapo",
        }
        .into()
    });
    let seed: u64 = arg(&args, "--seed").unwrap_or(42);
    let steps: u64 = arg(&args, "--steps").unwrap_or(300);
    // Umbral intensidad–duración con el que se normaliza la lluvia a
    // exceedancia: E = intensidad_horaria / id_threshold.
    let id_threshold: f64 = arg(&args, "--id-threshold").unwrap_or(5.0);
    let gradiente = args.iter().any(|a| a == "--gradiente");

    let mut params = match preset.as_str() {
        "chanaral-enhanced" => Params::preset_chanaral_enhanced(),
        "chanaral" => Params::preset_chanaral(),
        "de" => Params::preset_de(),
        _ => Params::default(),
    };

    if let Some(n) = arg::<usize>(&args, "--agents") {
        params.n_rain_agents = n;
    }

    let data = load(&data_dir).expect("cargar stack");
    let (w, h) = (data.layers.dem.width(), data.layers.dem.height());
    println!(
        "stack {} · {w}x{h} · preset {preset} · semilla {seed} · {} gotas · umbral I-D {id_threshold} mm/h\n",
        data_dir.display(),
        params.n_rain_agents
    );
    println!(
        "{:>10} | {:>12} | {:>8} | {:>9} | {:>9}",
        "mm/día", "celdas-hora", "flujos", "huella", "Δ huella"
    );
    println!(
        "{:->10}-+-{:->12}-+-{:->8}-+-{:->9}-+-{:->9}",
        "", "", "", "", ""
    );

    let mut previa: Option<usize> = None;
    for mm_dia in [0.3_f64, 1.0, 5.0, 15.0, 50.0, 200.0] {
        // Capa de gatillo: una grilla por hora simulada. La lluvia diaria se
        // reemplaza por `mm_dia` uniforme y se desagrega con el mismo patrón
        // horario del modo lluvia, luego se normaliza por el umbral I-D.
        let n_horas = (HOURLY_PATTERNS.len() * 24) as u64;
        let mut trigger = Vec::with_capacity(n_horas as usize);
        let mut celdas_hora = 0usize;
        for hora in 1..=n_horas {
            let dia = (hora / 24) as usize;
            let hora_del_dia = hora % 24;
            let factor = HOURLY_PATTERNS
                .get(dia)
                .and_then(|p| p.iter().find(|(hh, _)| *hh == hora_del_dia))
                .map_or(0.0, |(_, f)| *f);
            let e = mm_dia * factor / id_threshold;
            // Con `--gradiente`, E decae linealmente hacia el oeste: al subir la
            // lluvia, la banda con E >= 1 se ensancha (es el caso real, donde la
            // tormenta cubre más área). Sin la bandera, E es uniforme y el área
            // gatillada no cambia: aísla el efecto del volumen.
            let grid = if gradiente {
                Grid2D::from_fn(w, h, |p| (e * (p.x as f64 / (w - 1).max(1) as f64)) as f32)
            } else {
                Grid2D::fill(w, h, e as f32)
            };
            celdas_hora += grid.iter().filter(|&(_, &v)| v >= 1.0).count();
            trigger.push(grid);
        }

        let mut layers = data.layers.clone();
        layers.trigger = Some(trigger);
        let model = DebrisFlowModel::new(Arc::new(layers), params.clone(), data.pixel_size, seed);
        let mut sim =
            Simulation::new(model, seed).with_schedule(Schedule::new(Activation::Ordered));
        sim.run(steps);

        let huella = sim.model.footprint.iter().filter(|&(_, &v)| v).count();
        let delta = previa.map_or(String::from("—"), |p| {
            format!("{:+.1}%", (huella as f64 / p as f64 - 1.0) * 100.0)
        });
        println!(
            "{mm_dia:>10} | {celdas_hora:>12} | {:>8} | {huella:>9} | {delta:>9}",
            sim.model.flows_created
        );
        previa = Some(huella);
    }
}
