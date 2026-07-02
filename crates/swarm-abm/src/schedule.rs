//! Activation order of agents at each step.

use crate::agent::AgentId;
use crate::rng::{SimRng, shuffle};

/// Agent activation policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Activation {
    /// Insertion order, fixed across all steps (trivially deterministic).
    Ordered,
    /// A new random permutation on each step, derived from the seeded RNG
    /// (deterministic given the seed). This is the default and the standard
    /// choice in ABM to avoid order artifacts.
    #[default]
    Random,
    /// Two-phase simultaneous activation: first **all** agents run
    /// [`Agent::decide`](crate::agent::Agent::decide) observing the same
    /// world state (immutable model), then all of them run
    /// [`Agent::apply`](crate::agent::Agent::apply). Both phases iterate
    /// over agents in insertion order (like Mesa); the order only affects
    /// the RNG stream and collision resolution in `apply`.
    Simultaneous,
    /// **Staged** activation: `N` full sweeps per step, one for each value
    /// in `0..N`. In stage `s`, **all** agents run
    /// [`Agent::stage`](crate::agent::Agent::stage)`(s, ...)` (with a
    /// mutable model, unlike `Simultaneous`) before any of them moves on to
    /// stage `s+1` — Mesa's `StagedActivation` pattern, for models where an
    /// entire phase of the step (e.g. "move", then "eat", then
    /// "reproduce") must complete across the whole population before the
    /// next one starts. This generalizes `Simultaneous` (which is, in
    /// essence, 2 stages with the immutable/mutable asymmetry of
    /// `decide`/`apply`); for N homogeneous stages — all with mutable
    /// access, without the parallelizability guarantee of `decide` — use
    /// this variant instead.
    ///
    /// The iteration order is the same across all stages of a step (no
    /// reshuffling between stages); it follows whatever policy applied
    /// before entering the staged phase (fixed, unless `Schedule` has its
    /// own shuffling — currently `Staged` does not shuffle, the same
    /// default as `Ordered`).
    Staged(usize),
}

/// Scheduler: decides the order and number of phases in which agents are
/// activated during a step.
#[derive(Debug, Clone, Copy, Default)]
pub struct Schedule {
    activation: Activation,
}

impl Schedule {
    /// Creates a scheduler with the given policy.
    #[must_use]
    pub fn new(activation: Activation) -> Self {
        Self { activation }
    }

    /// The configured policy.
    #[must_use]
    pub fn activation(&self) -> Activation {
        self.activation
    }

    /// Reorders `ids` in place according to this step's policy (no allocation).
    pub fn order_in_place(&self, ids: &mut [AgentId], rng: &mut SimRng) {
        if self.activation == Activation::Random {
            shuffle(rng, ids);
        }
    }

    /// Returns the ids in this step's activation order.
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
        assert_eq!(a, b, "same seed, same permutation");

        let mut ordenada = a.clone();
        ordenada.sort();
        assert_eq!(ordenada, v, "is a permutation of the original ids");
    }
}
