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
use swarm_abm::prelude::*;
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

// ---------------------------------------------------------------------------
// M9: full-trajectory golden value. The tests above pin RNG *primitives*,
// but none pins a complete `Simulation`: a change in how the engine
// CONSUMES the stream (reordering the shuffle vs `before_step`, an extra
// draw in the hot path, …) would leave every primitive test green while
// silently breaking every published result. This walker model pins the
// whole pipeline: seeding → Random schedule shuffle → per-agent step RNG →
// grid movement, over 20 steps.
// ---------------------------------------------------------------------------

struct GoldenWalker {
    pos: Pos,
}

struct GoldenWorld {
    agents: AgentSet<GoldenWalker>,
    grid: Grid2D<u32>,
}

impl Agent for GoldenWalker {
    type Model = GoldenWorld;

    fn step(&mut self, _id: AgentId, model: &mut GoldenWorld, rng: &mut SimRng) {
        if let Some(dest) = model
            .grid
            .random_neighbor(self.pos, Neighborhood::Moore, rng)
        {
            self.pos = dest;
            model.grid[self.pos] += 1;
        }
    }
}

impl Model for GoldenWorld {
    type Agent = GoldenWalker;
    fn agents(&self) -> &AgentSet<GoldenWalker> {
        &self.agents
    }
    fn agents_mut(&mut self) -> &mut AgentSet<GoldenWalker> {
        &mut self.agents
    }
}

/// FNV-1a (64-bit) over the byte stream, implemented inline — no external
/// dependency, stable across platforms (wasm32 included: only u64 math).
fn fnv1a(bytes: impl IntoIterator<Item = u8>) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        hash ^= u64::from(b);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// Si este test falla, se rompió el CONTRATO de reproducibilidad del motor
/// (ver `docs/REPRODUCIBILITY.md`): la misma semilla ya no reproduce las
/// trayectorias publicadas (paridad Mesa, Sobol de SIGRID, etc.). No basta
/// con re-pinnear el valor: el cambio exige bump de versión **minor** y una
/// entrada "Rompe determinismo" en el CHANGELOG.
///
/// Valor pinneado el 2026-07-10: grilla 10×10 toroidal, 10 agentes en
/// posiciones fijas `(i, (3·i) mod 10)`, `Activation::Random`, semilla 42,
/// 20 pasos.
#[test]
fn trayectoria_completa_de_simulacion_fija() {
    let mut agents = AgentSet::with_capacity(10);
    for i in 0..10 {
        // Fixed (non-random) placement: the golden value must depend only
        // on the engine's own RNG consumption, not on setup draws.
        agents.insert(GoldenWalker {
            pos: Pos::new(i, (3 * i) % 10),
        });
    }
    let grid = Grid2D::new(10, 10).with_torus(true);

    let mut sim = Simulation::new(GoldenWorld { agents, grid }, 42)
        .with_schedule(Schedule::new(Activation::Random));
    sim.run(20);

    let posiciones: Vec<(usize, usize)> = sim
        .model
        .agents
        .iter()
        .map(|(_, w)| (w.pos.x, w.pos.y))
        .collect();

    // Final positions in insertion order (10 agents, 20 steps each).
    assert_eq!(
        posiciones,
        vec![
            (0, 9),
            (3, 5),
            (2, 6),
            (3, 5),
            (6, 5),
            (9, 1),
            (1, 6),
            (4, 5),
            (4, 0),
            (5, 8),
        ]
    );

    // And the per-cell visit counts of the whole grid, compressed to one
    // FNV-1a hash (covers the footprint, not just where agents ended up).
    let hash = fnv1a(sim.model.grid.iter().flat_map(|(_, &v)| v.to_le_bytes()));
    assert_eq!(hash, 0x122c_2346_8b1f_cb97);
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
