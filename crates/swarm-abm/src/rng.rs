//! Seedable, deterministic random number generation.
//!
//! The whole engine uses [`SimRng`] (ChaCha8), which guarantees the same
//! sequence for the same seed on any platform and any version of the `rand`
//! crate. Never use `rand::rng()` inside a model: it breaks reproducibility.
//!
//! ## Why [`uniform_below`], [`uniform_f64`], [`bernoulli`] and [`shuffle`] exist
//!
//! The raw ChaCha8 stream (`next_u32`/`next_u64`, via `RngCore`) is a stable
//! cryptographic specification: `rand_chacha` cannot change it without
//! breaking compatibility with known test vectors. But the engine can NOT
//! rely on `Rng::random_range`, `Rng::random_bool` or `SliceRandom::shuffle`
//! from the `rand` crate for its reproducibility guarantee: those are
//! **unspecified-algorithm** APIs — `rand` is free to change how it maps raw
//! bits to a bounded range, a boolean, or a permutation between versions
//! (0.9 → 0.10, for example), and that change would silently invalidate any
//! already-published result run with the identical seed. These four
//! functions implement those operations **inside** the engine, on top of the
//! raw stream, so that the only external dependency that can affect
//! reproducibility is the ChaCha8 specification itself (which does not
//! change). Models and the engine should prefer them over the `rand::Rng`
//! equivalents.

use rand::{RngCore, SeedableRng};
use rand_chacha::ChaCha8Rng;

/// The simulation's RNG: deterministic, portable and seedable.
pub type SimRng = ChaCha8Rng;

/// Builds the simulation's RNG from a `u64` seed.
///
/// # Example
/// ```
/// use swarm_abm::rng::{rng_from_seed, SimRng};
/// use rand::Rng;
///
/// let mut a: SimRng = rng_from_seed(42);
/// let mut b: SimRng = rng_from_seed(42);
/// assert_eq!(a.random_range(0..1000), b.random_range(0..1000));
/// ```
pub fn rng_from_seed(seed: u64) -> SimRng {
    ChaCha8Rng::seed_from_u64(seed)
}

/// SplitMix64 finalizer: diffuses a seed's bits well.
fn mix64(mut z: u64) -> u64 {
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// **Per-agent** RNG, deterministically derived from `(seed, step, agent)`.
///
/// Gives each agent its own stream on every step, independent of activation
/// order and of the number of threads. This is what lets the simultaneous
/// `decide` phase run in parallel and produce the **exact same result, bit
/// for bit**, as sequential: an agent's randomness depends only on its id
/// and the step, never on when the scheduler happens to reach it.
///
/// Combines the three fields in a **chain** (hash-combine/SplitMix style,
/// each link `mix64(state ^ field)`), not the XOR of three independent
/// hashes used by an earlier version: that XOR admitted trivial structural
/// cancellations between distinct `(seed,step,agent)` triples (the mix64 in
/// between made them hard to construct by hand, but there was no argument
/// that they couldn't exist). The chain is the standard construction for
/// this problem — each link is a 64-bit permutation given the previous
/// state — and doesn't have that weakness. Exact collisions between triples
/// are still possible (the input domain, 2⁶⁴×2⁶⁴×2⁶⁴, is far larger than the
/// 64-bit seed codomain: they're inevitable by the pigeonhole principle, not
/// something any construction can avoid), but they're no longer reachable
/// through a cheap algebraic cancellation.
#[must_use]
pub fn child_rng(seed: u64, step: u64, agent: u64) -> SimRng {
    let mut s = mix64(seed ^ 0x9E37_79B9_7F4A_7C15);
    s = mix64(s ^ step);
    s = mix64(s ^ agent);
    rng_from_seed(s)
}

/// Unbiased uniform integer in `0..bound`, built on top of the raw stream
/// (Lemire's method: a 128-bit multiplication + bounded rejection, no
/// division in the common case). See the module note on why the engine
/// doesn't use `Rng::random_range` for this.
///
/// # Panics
/// If `bound == 0`.
#[must_use]
pub fn uniform_below(rng: &mut SimRng, bound: u64) -> u64 {
    assert!(bound > 0, "uniform_below requires bound > 0");
    let mut wide = u128::from(rng.next_u64()) * u128::from(bound);
    let mut low = wide as u64;
    if low < bound {
        // Rejection threshold: the largest multiple of `bound` that fits in 2^64.
        let threshold = bound.wrapping_neg() % bound;
        while low < threshold {
            wide = u128::from(rng.next_u64()) * u128::from(bound);
            low = wide as u64;
        }
    }
    (wide >> 64) as u64
}

/// Like [`uniform_below`], for indices (`usize`). With `bound == 0` (empty
/// range) returns `0` without consuming the RNG instead of panicking —
/// convenient for "pick an index from this collection" once you've already
/// checked it isn't empty; if `bound` can legitimately be 0, use
/// [`uniform_below`] and decide explicitly.
#[must_use]
pub fn uniform_usize(rng: &mut SimRng, bound: usize) -> usize {
    if bound == 0 {
        return 0;
    }
    uniform_below(rng, bound as u64) as usize
}

/// Uniform float in `[0, 1)`: the top 53 bits of the raw stream, scaled.
/// Standard technique (Vigna); doesn't depend on whatever convention `rand`
/// uses internally for `UniformFloat`.
#[must_use]
pub fn uniform_f64(rng: &mut SimRng) -> f64 {
    let bits = rng.next_u64() >> 11; // 53 mantissa bits
    (bits as f64) * (1.0 / (1u64 << 53) as f64)
}

/// Boolean that is `true` with probability `p`.
///
/// # Panics
/// If `p` is not in `[0, 1]`.
#[must_use]
pub fn bernoulli(rng: &mut SimRng, p: f64) -> bool {
    assert!((0.0..=1.0).contains(&p), "p out of [0,1]: {p}");
    uniform_f64(rng) < p
}

/// Shuffles `slice` in place (Fisher–Yates), without relying on
/// `rand::seq::SliceRandom::shuffle`.
pub fn shuffle<T>(rng: &mut SimRng, slice: &mut [T]) {
    for i in (1..slice.len()).rev() {
        let j = uniform_usize(rng, i + 1);
        slice.swap(i, j);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::Rng;

    #[test]
    fn misma_semilla_misma_secuencia() {
        let mut a = rng_from_seed(7);
        let mut b = rng_from_seed(7);
        let sa: Vec<u32> = (0..32).map(|_| a.random_range(0..u32::MAX)).collect();
        let sb: Vec<u32> = (0..32).map(|_| b.random_range(0..u32::MAX)).collect();
        assert_eq!(sa, sb);
    }

    #[test]
    fn semillas_distintas_secuencias_distintas() {
        let mut a = rng_from_seed(1);
        let mut b = rng_from_seed(2);
        let sa: Vec<u32> = (0..32).map(|_| a.random_range(0..u32::MAX)).collect();
        let sb: Vec<u32> = (0..32).map(|_| b.random_range(0..u32::MAX)).collect();
        assert_ne!(sa, sb);
    }

    #[test]
    fn child_rng_determinista_y_por_agente() {
        // Same (seed, step, agent) ⇒ same sequence (reproducible).
        let draw = |s, st, a| {
            let mut r = child_rng(s, st, a);
            (0..8)
                .map(|_| r.random_range(0..u32::MAX))
                .collect::<Vec<_>>()
        };
        assert_eq!(draw(42, 3, 7), draw(42, 3, 7));
        // Different agents in the same step ⇒ different streams.
        assert_ne!(draw(42, 3, 7), draw(42, 3, 8));
        // Same agent in different steps ⇒ different streams.
        assert_ne!(draw(42, 3, 7), draw(42, 4, 7));
        // Different seeds ⇒ different streams.
        assert_ne!(draw(42, 3, 7), draw(43, 3, 7));
    }
}
