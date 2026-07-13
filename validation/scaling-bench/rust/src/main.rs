//! Kernel de escalamiento depredador-presa (espacial) — implementación sobre el
//! motor swarm-abm, usando su índice espacial `ContinuousSpace` y su RNG
//! sembrable. Espejo exacto, en reglas, del kernel C++ (../cpp). Mide el
//! throughput del stepping; la construcción queda fuera del cronómetro.
//!
//! Reglas idénticas a la versión C++ (ver ese archivo). Actualización
//! sincrónica: las consultas ven las posiciones del inicio del paso (índice
//! reindexado una vez por paso), y las nuevas posiciones se aplican al final.
//!
//! Uso: prey_predator <N> <steps> [seed]
use std::time::Instant;

use swarm_abm::continuous::{ContinuousSpace, Vec2};
use swarm_abm::rng::{rng_from_seed, uniform_f64};

const LAMBDA: f64 = 0.06366; // agentes por unidad^2 (~5 vecinos en R)
const R: f64 = 5.0; // radio de sensado
const MOVE_STEP: f64 = 1.0; // paso de movimiento
const PRED_FRAC: f64 = 0.2; // fracción de depredadores

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let n: usize = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(4000);
    let steps: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(50);
    let seed: u64 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(1);

    let l = (n as f64 / LAMBDA).sqrt();
    let n_pred = (PRED_FRAC * n as f64) as usize;
    let hi = l.next_down(); // límite superior del dominio [0, l)

    let mut rng = rng_from_seed(seed);
    // value = true si el punto es depredador.
    let mut space: ContinuousSpace<bool> = ContinuousSpace::new(l, l, R);
    let mut ids = Vec::with_capacity(n);
    let mut pred = Vec::with_capacity(n);
    for i in 0..n {
        let p = Vec2::new(uniform_f64(&mut rng) * l, uniform_f64(&mut rng) * l);
        let is_pred = i < n_pred;
        ids.push(space.add(p, is_pred));
        pred.push(is_pred);
    }
    space.reindex();

    let mut new_pos = vec![Vec2::new(0.0, 0.0); n];
    let mut contactos: u64 = 0;

    let t0 = Instant::now();
    for _ in 0..steps {
        for i in 0..n {
            let pi = space.position(ids[i]);
            let mut moved = false;
            if pred[i] {
                let mut best = R;
                let mut target: Option<Vec2> = None;
                space.for_each_within(pi, R, |_id, npos, is_pred, dist| {
                    if *is_pred {
                        return;
                    }
                    if dist < best {
                        best = dist;
                        target = Some(npos);
                    }
                });
                if let Some(t) = target {
                    contactos += 1;
                    let dir = Vec2::new(t.x - pi.x, t.y - pi.y).normalize_or_zero();
                    new_pos[i] = Vec2::new(pi.x + MOVE_STEP * dir.x, pi.y + MOVE_STEP * dir.y);
                    moved = true;
                }
            }
            if !moved {
                let ang = uniform_f64(&mut rng) * std::f64::consts::TAU;
                new_pos[i] = Vec2::new(pi.x + MOVE_STEP * ang.cos(), pi.y + MOVE_STEP * ang.sin());
            }
            // clamp a [0, l)
            let mut nx = new_pos[i].x;
            let mut ny = new_pos[i].y;
            if nx < 0.0 {
                nx = 0.0;
            } else if nx >= l {
                nx = hi;
            }
            if ny < 0.0 {
                ny = 0.0;
            } else if ny >= l {
                ny = hi;
            }
            new_pos[i] = Vec2::new(nx, ny);
        }
        for i in 0..n {
            space.set_pos(ids[i], new_pos[i]);
        }
        space.reindex();
    }
    let ms = t0.elapsed().as_secs_f64() * 1000.0;
    // N | steps | ms_total | ms_por_paso | contactos (sink)
    println!("{} {} {:.3} {:.5} {}", n, steps, ms, ms / steps as f64, contactos);
}
