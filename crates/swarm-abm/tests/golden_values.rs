//! Valores dorados: protegen la reproducibilidad contra deriva silenciosa
//! de una dependencia (`rand`, `rand_chacha`) o del compilador — no solo
//! contra deriva propia del motor (eso ya lo cubren los tests de
//! `mismo_seed ⇒ mismo_resultado` de cada módulo).
//!
//! Si alguno de estos tests falla tras un `cargo update`, un valor que algún
//! resultado ya publicado (paridad Mesa, Sobol de SIGRID, etc.) asumía como
//! estable dejó de serlo: hay que investigar, no solo re-pinnear. Ver P0-2 en
//! `docs/AUDIT.md`.

use rand::RngCore;
use swarm_abm::graph::Graph;
use swarm_abm::rng::{bernoulli, child_rng, rng_from_seed, shuffle, uniform_below, uniform_f64};

#[test]
fn stream_crudo_de_chacha8() {
    // El stream crudo (next_u64) es la especificación ChaCha8 misma —
    // rand_chacha no puede cambiarlo sin romper sus propios vectores de
    // prueba. Si este test falla, el problema es grave: toda garantía de
    // reproducibilidad del motor descansa en esto.
    let mut rng = rng_from_seed(42);
    let draws: Vec<u64> = (0..4).map(|_| rng.next_u64()).collect();
    assert_eq!(
        draws,
        vec![
            12578764544318200737,
            17529487244874322312,
            7886285670807131020,
            11572758976476374866,
        ]
    );
}

#[test]
fn child_rng_primeros_draws() {
    let mut rng = child_rng(42, 3, 7);
    let draws: Vec<u64> = (0..4).map(|_| rng.next_u64()).collect();
    assert_eq!(
        draws,
        vec![
            3360365477519844634,
            11599774640582612307,
            17561453714142158464,
            575054905332807681,
        ]
    );
}

#[test]
fn uniform_below_secuencia() {
    let mut rng = rng_from_seed(1);
    let draws: Vec<u64> = (0..8).map(|_| uniform_below(&mut rng, 100)).collect();
    assert_eq!(draws, vec![40, 8, 59, 21, 28, 71, 46, 15]);
}

#[test]
fn uniform_f64_secuencia() {
    let mut rng = rng_from_seed(2);
    let draws: Vec<f64> = (0..4).map(|_| uniform_f64(&mut rng)).collect();
    assert_eq!(
        draws,
        vec![
            0.8813407293505765,
            0.5484309665677798,
            0.7892508066800136,
            0.8573410623312527,
        ]
    );
}

#[test]
fn bernoulli_secuencia() {
    let mut rng = rng_from_seed(3);
    let draws: Vec<bool> = (0..16).map(|_| bernoulli(&mut rng, 0.3)).collect();
    assert_eq!(
        draws,
        vec![
            false, true, false, true, false, false, true, true, true, true, false, false, false,
            false, false, true,
        ]
    );
}

#[test]
fn shuffle_resultado_fijo() {
    let mut rng = rng_from_seed(5);
    let mut v: Vec<u32> = (0..10).collect();
    shuffle(&mut rng, &mut v);
    assert_eq!(v, vec![0, 4, 5, 3, 1, 9, 7, 8, 6, 2]);
}

#[test]
fn erdos_renyi_grafo_fijo() {
    let g = Graph::<()>::erdos_renyi(10, 0.3, &mut rng_from_seed(7));
    let edges: Vec<(usize, usize)> = g
        .node_ids()
        .flat_map(|a| {
            g.neighbors(a)
                .map(move |b| (a.as_usize(), b.as_usize()))
                .filter(|&(a, b)| a < b)
        })
        .collect();
    assert_eq!(
        edges,
        vec![
            (0, 1),
            (0, 2),
            (0, 7),
            (1, 3),
            (1, 6),
            (2, 3),
            (2, 5),
            (2, 8),
            (2, 9),
            (3, 4),
            (3, 7),
            (3, 8),
            (5, 7),
            (5, 8),
            (8, 9),
        ]
    );
}
