//! Vuelca el stream de números aleatorios del motor como **bytes crudos** a
//! stdout, para alimentar baterías estadísticas externas (PractRand, dieharder).
//!
//! Dos streams, ambos relevantes para la reproducibilidad del motor:
//!
//! - `single`  — un stream [`SimRng`] (ChaCha8) directo. Control de cordura:
//!   ChaCha8 es de grado criptográfico y debe pasar trivialmente.
//! - `interagent` — el stream **relevante para ABM paralelo**: la *primera*
//!   extracción de cada agente (`child_rng(seed, step, id)`), ciclando agentes
//!   y luego pasos. Pone a prueba la **decorrelación entre los streams de
//!   agentes vecinos** — la propiedad que el `decide` paralelo necesita y que
//!   un revisor de simulación cuestionaría.
//!
//! Uso: `cargo run --release -p rng-dump -- interagent [semilla] | ./RNG_test stdin64`

use std::io::{self, Write};

use swarm_core::prelude::*;
use swarm_core::rng::child_rng;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mode = args.first().map_or("single", String::as_str);
    let seed: u64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(42);

    let stdout = io::stdout();
    let mut out = io::BufWriter::with_capacity(1 << 16, stdout.lock());

    match mode {
        "single" => {
            let mut rng = rng_from_seed(seed);
            loop {
                let v: u64 = rng.random();
                if out.write_all(&v.to_le_bytes()).is_err() {
                    break; // pipe cerrado (PractRand leyó lo que necesitaba)
                }
            }
        }
        "interagent" => {
            const AGENTS: u64 = 100_000;
            let mut step: u64 = 0;
            'outer: loop {
                for id in 0..AGENTS {
                    let v: u64 = child_rng(seed, step, id).random();
                    if out.write_all(&v.to_le_bytes()).is_err() {
                        break 'outer;
                    }
                }
                step += 1;
            }
        }
        _ => {
            eprintln!("modo desconocido '{mode}': usar `single` o `interagent`");
            std::process::exit(1);
        }
    }
    let _ = out.flush();
}
