//! Orden de activación de los agentes en cada paso.

use rand::seq::SliceRandom;

use crate::agent::AgentId;
use crate::rng::SimRng;

/// Política de activación de agentes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Activation {
    /// Orden de inserción, fijo en todos los pasos (determinista trivial).
    Ordered,
    /// Permutación aleatoria nueva en cada paso, derivada del RNG sembrado
    /// (determinista dada la semilla). Es el default y el estándar en ABM
    /// para evitar artefactos de orden.
    #[default]
    Random,
    /// Activación simultánea en dos fases: primero **todos** los agentes
    /// ejecutan [`Agent::decide`](crate::agent::Agent::decide) observando el
    /// mismo estado del mundo (modelo inmutable), y luego todos ejecutan
    /// [`Agent::apply`](crate::agent::Agent::apply). Ambas fases recorren a
    /// los agentes en orden de inserción (como Mesa); el orden solo afecta
    /// el stream del RNG y la resolución de colisiones en `apply`.
    Simultaneous,
}

/// Scheduler: decide en qué orden y en cuántas fases se activan los agentes
/// en un paso.
#[derive(Debug, Clone, Copy, Default)]
pub struct Schedule {
    activation: Activation,
}

impl Schedule {
    /// Crea un scheduler con la política dada.
    #[must_use]
    pub fn new(activation: Activation) -> Self {
        Self { activation }
    }

    /// Política configurada.
    #[must_use]
    pub fn activation(&self) -> Activation {
        self.activation
    }

    /// Reordena `ids` in situ según la política de este paso (sin asignar).
    pub fn order_in_place(&self, ids: &mut [AgentId], rng: &mut SimRng) {
        if self.activation == Activation::Random {
            ids.shuffle(rng);
        }
    }

    /// Devuelve los ids en el orden de activación de este paso.
    #[must_use]
    pub fn order(&self, mut ids: Vec<AgentId>, rng: &mut SimRng) -> Vec<AgentId> {
        self.order_in_place(&mut ids, rng);
        ids
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::AgentSet;
    use crate::rng::rng_from_seed;

    fn ids(n: usize) -> Vec<AgentId> {
        let mut set = AgentSet::new();
        (0..n).map(|i| set.insert(i)).collect()
    }

    #[test]
    fn ordered_preserva_el_orden() {
        let mut rng = rng_from_seed(0);
        let v = ids(10);
        assert_eq!(
            Schedule::new(Activation::Ordered).order(v.clone(), &mut rng),
            v
        );
    }

    #[test]
    fn random_es_permutacion_determinista() {
        let v = ids(50);
        let s = Schedule::new(Activation::Random);

        let a = s.order(v.clone(), &mut rng_from_seed(9));
        let b = s.order(v.clone(), &mut rng_from_seed(9));
        assert_eq!(a, b, "misma semilla, misma permutación");

        let mut ordenada = a.clone();
        ordenada.sort();
        assert_eq!(ordenada, v, "es una permutación de los ids originales");
    }
}
