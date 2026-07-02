//! Evaluador masivo para análisis de sensibilidad: lee filas de parámetros
//! (CSV, 6 columnas en el orden del problema Sobol) y escribe el loss rate
//! medio sobre `--reps` semillas, una línea por fila. El muestreo y el análisis
//! Sobol los hace SALib (Python); aquí va solo la parte cara (la evaluación del
//! modelo), paralelizada con rayon.
//!
//! La operación que en Mesa costaba ~13.600 core-hours (inviable incluso con
//! servidores dedicados) corre aquí en minutos sobre un nodo.
//!
//! Uso: `sobol-eval --file rows.csv --days 30 --reps 1 [--seed-base 1000]`
//! Columnas: sheep_density,fox_eff,n_dogs,hare_density,chilla_density,lamb_prop

use std::fs;

use sigrid::{Params, run_loss_rate};

#[cfg(feature = "parallel")]
use rayon::prelude::*;

fn arg<T: std::str::FromStr>(args: &[String], name: &str) -> Option<T> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .and_then(|v| v.parse().ok())
}

fn params_from_row(row: &[f64]) -> Params {
    Params {
        sheep_density: row[0],
        fox_predation_effectiveness: row[1],
        n_dogs: row[2].round().max(0.0) as usize,
        hare_density: row[3],
        chilla_density: row[4],
        lamb_proportion: row[5],
        ..Params::default()
    }
}

fn eval_row(idx: usize, row: &[f64], days: u64, reps: u64, seed_base: u64) -> f64 {
    let p = params_from_row(row);
    let mut acc = 0.0;
    for r in 0..reps {
        let seed = seed_base + r * 100_000 + idx as u64;
        acc += run_loss_rate(p, seed, days);
    }
    acc / reps as f64
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let file: String = arg(&args, "--file").expect("--file <csv> requerido");
    let days: u64 = arg(&args, "--days").unwrap_or(30);
    let reps: u64 = arg(&args, "--reps").unwrap_or(1);
    let seed_base: u64 = arg(&args, "--seed-base").unwrap_or(1000);

    let text = fs::read_to_string(&file).expect("no se pudo leer --file");
    let rows: Vec<Vec<f64>> = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            l.split(',')
                .map(|v| v.trim().parse::<f64>().unwrap())
                .collect()
        })
        .collect();

    let losses: Vec<f64> = {
        #[cfg(feature = "parallel")]
        {
            rows.par_iter()
                .enumerate()
                .map(|(i, row)| eval_row(i, row, days, reps, seed_base))
                .collect()
        }
        #[cfg(not(feature = "parallel"))]
        {
            rows.iter()
                .enumerate()
                .map(|(i, row)| eval_row(i, row, days, reps, seed_base))
                .collect()
        }
    };

    let mut out = String::with_capacity(losses.len() * 8);
    for v in &losses {
        out.push_str(&format!("{v:.6}\n"));
    }
    print!("{out}");
}
