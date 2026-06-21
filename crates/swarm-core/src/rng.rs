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

/// Finalizador splitmix64: difunde bien los bits de una semilla.
fn mix64(mut z: u64) -> u64 {
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// RNG **por-agente** derivado de forma determinista de `(seed, step, agent)`.
///
/// Da a cada agente su propio stream en cada paso, independiente del orden de
/// activación y del número de hilos. Es lo que permite que la fase `decide`
/// simultánea corra en paralelo y produzca el **mismo resultado bit a bit** que
/// en secuencial: la aleatoriedad de un agente solo depende de su id y del
/// paso, nunca de cuándo lo toca el scheduler.
#[must_use]
pub fn child_rng(seed: u64, step: u64, agent: u64) -> SimRng {
    let s = mix64(seed.wrapping_add(0x9E37_79B9_7F4A_7C15))
        ^ mix64(step.wrapping_add(0xD1B5_4A32_D192_ED03))
        ^ mix64(agent.wrapping_add(0xCBF2_9CE4_8422_2325));
    rng_from_seed(mix64(s))
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
        // Mismo (seed, step, agent) ⇒ misma secuencia (reproducible).
        let draw = |s, st, a| {
            let mut r = child_rng(s, st, a);
            (0..8)
                .map(|_| r.random_range(0..u32::MAX))
                .collect::<Vec<_>>()
        };
        assert_eq!(draw(42, 3, 7), draw(42, 3, 7));
        // Agentes distintos en el mismo paso ⇒ streams distintos.
        assert_ne!(draw(42, 3, 7), draw(42, 3, 8));
        // Mismo agente en pasos distintos ⇒ streams distintos.
        assert_ne!(draw(42, 3, 7), draw(42, 4, 7));
        // Semillas distintas ⇒ streams distintos.
        assert_ne!(draw(42, 3, 7), draw(43, 3, 7));
    }
}
