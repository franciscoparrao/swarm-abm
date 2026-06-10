//! Generación de números aleatorios sembrable y determinista.
//!
//! Todo el motor usa [`SimRng`] (ChaCha8), que garantiza la misma secuencia
//! para la misma semilla en cualquier plataforma y versión del crate `rand`.
//! Nunca uses `rand::rng()` dentro de un modelo: rompe la reproducibilidad.

use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;

/// RNG de la simulación: determinista, portable y sembrable.
pub type SimRng = ChaCha8Rng;

/// Construye el RNG de la simulación a partir de una semilla `u64`.
///
/// # Ejemplo
/// ```
/// use swarm_core::rng::{rng_from_seed, SimRng};
/// use rand::Rng;
///
/// let mut a: SimRng = rng_from_seed(42);
/// let mut b: SimRng = rng_from_seed(42);
/// assert_eq!(a.random_range(0..1000), b.random_range(0..1000));
/// ```
pub fn rng_from_seed(seed: u64) -> SimRng {
    ChaCha8Rng::seed_from_u64(seed)
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
}
