//! Running **ensembles** and **parameter sweeps**.
//!
//! Generalizes the pattern a typical ABM study repeats by hand: run the
//! same model with many seeds (replicates) or many configurations, and keep
//! a summary per run. With the `parallel` feature (on by default), runs are
//! spread across threads with rayon; without it, they run sequentially
//! (useful for WASM).
//!
//! The caller supplies two closures:
//! - `build`: builds a [`Simulation`] from the seed (and, for sweeps, the
//!   configuration). Controls the scheduler, reporters, etc.
//! - `outcome`: extracts the result of interest from the finished
//!   simulation (a scalar, the series, whatever is `Send`).
//!
//! ```
//! use swarm_abm::prelude::*;
//! use swarm_abm::batch::run_ensemble;
//!
//! struct Bug;
//! struct World { agents: AgentSet<Bug>, total: u64 }
//! impl Agent for Bug {
//!     type Model = World;
//!     fn step(&mut self, _: AgentId, m: &mut World, rng: &mut SimRng) {
//!         m.total += rng.random_range(0..10);
//!     }
//! }
//! impl Model for World {
//!     type Agent = Bug;
//!     fn agents(&self) -> &AgentSet<Bug> { &self.agents }
//!     fn agents_mut(&mut self) -> &mut AgentSet<Bug> { &mut self.agents }
//! }
//!
//! // 20 replicates, 50 steps, returns each one's final total.
//! let totals = run_ensemble(0..20, 50, |_seed| {
//!     let mut agents = AgentSet::new();
//!     agents.insert(Bug);
//!     Simulation::new(World { agents, total: 0 }, _seed)
//! }, |sim| sim.model.total);
//! assert_eq!(totals.len(), 20);
//! ```

use crate::model::Model;
use crate::sim::Simulation;

#[cfg(feature = "parallel")]
use rayon::prelude::*;

/// Runs an **ensemble**: one replicate per seed in `seeds`, each up to
/// `max_steps` steps. Returns each replicate's `outcome`, in the same order
/// as `seeds`.
///
/// `build` receives the seed and returns the ready-to-run [`Simulation`]
/// (scheduler and reporters included). `outcome` inspects it after it runs.
pub fn run_ensemble<M, R, B, O>(
    seeds: impl IntoIterator<Item = u64>,
    max_steps: u64,
    build: B,
    outcome: O,
) -> Vec<R>
where
    M: Model,
    B: Fn(u64) -> Simulation<M> + Sync,
    O: Fn(&Simulation<M>) -> R + Sync,
    R: Send,
{
    let seeds: Vec<u64> = seeds.into_iter().collect();
    let run = |&seed: &u64| -> R {
        let mut sim = build(seed);
        sim.run(max_steps);
        outcome(&sim)
    };
    #[cfg(feature = "parallel")]
    {
        seeds.par_iter().map(run).collect()
    }
    #[cfg(not(feature = "parallel"))]
    {
        seeds.iter().map(run).collect()
    }
}

/// Result of one sweep cell: configuration index, seed, and value.
#[derive(Debug, Clone, Copy)]
pub struct SweepCell<R> {
    /// Index of the configuration in the original `configs`.
    pub config: usize,
    /// Seed this replicate ran with.
    pub seed: u64,
    /// Result extracted by `outcome`.
    pub value: R,
}

/// Runs a **parameter sweep**: every configuration in `configs` is
/// evaluated against every seed in `seeds` (cartesian product), up to
/// `max_steps` steps. Returns one [`SweepCell`] per combination.
///
/// This is the pattern behind a calibration or a sensitivity analysis:
/// `build` assembles the simulation from `(config, seed)` and `outcome`
/// summarizes the result.
pub fn run_sweep<P, M, R, B, O>(
    configs: &[P],
    seeds: &[u64],
    max_steps: u64,
    build: B,
    outcome: O,
) -> Vec<SweepCell<R>>
where
    P: Sync,
    M: Model,
    B: Fn(&P, u64) -> Simulation<M> + Sync,
    O: Fn(&P, &Simulation<M>) -> R + Sync,
    R: Send,
{
    let tasks: Vec<(usize, u64)> = configs
        .iter()
        .enumerate()
        .flat_map(|(ci, _)| seeds.iter().map(move |&s| (ci, s)))
        .collect();
    let run = |&(ci, seed): &(usize, u64)| -> SweepCell<R> {
        let mut sim = build(&configs[ci], seed);
        sim.run(max_steps);
        SweepCell {
            config: ci,
            seed,
            value: outcome(&configs[ci], &sim),
        }
    };
    #[cfg(feature = "parallel")]
    {
        tasks.par_iter().map(run).collect()
    }
    #[cfg(not(feature = "parallel"))]
    {
        tasks.iter().map(run).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prelude::*;

    struct Bug;
    struct World {
        agents: AgentSet<Bug>,
        total: u64,
    }
    impl Agent for Bug {
        type Model = World;
        fn step(&mut self, _: AgentId, m: &mut World, rng: &mut SimRng) {
            m.total += rng.random_range(0..10);
        }
    }
    impl Model for World {
        type Agent = Bug;
        fn agents(&self) -> &AgentSet<Bug> {
            &self.agents
        }
        fn agents_mut(&mut self) -> &mut AgentSet<Bug> {
            &mut self.agents
        }
    }

    fn build(n_agents: usize, seed: u64) -> Simulation<World> {
        let mut agents = AgentSet::new();
        for _ in 0..n_agents {
            agents.insert(Bug);
        }
        Simulation::new(World { agents, total: 0 }, seed)
            .with_schedule(Schedule::new(Activation::Ordered))
    }

    #[test]
    fn ensemble_corre_todas_las_replicas() {
        let totals = run_ensemble(0..16, 30, |s| build(5, s), |sim| sim.model.total);
        assert_eq!(totals.len(), 16);
        assert!(totals.iter().all(|&t| t > 0));
    }

    #[test]
    fn ensemble_es_determinista_y_paralelo_igual_a_secuencial() {
        // The result order follows the seeds, and every cell is
        // deterministic (same seed → same total), regardless of thread.
        let a = run_ensemble(0..32, 40, |s| build(8, s), |sim| sim.model.total);
        let b = run_ensemble(0..32, 40, |s| build(8, s), |sim| sim.model.total);
        assert_eq!(a, b);
        // A direct replicate of seed 7 matches cell 7.
        let mut sim = build(8, 7);
        sim.run(40);
        assert_eq!(a[7], sim.model.total);
    }

    #[test]
    fn sweep_cubre_el_producto_configs_por_semillas() {
        let configs = vec![1usize, 4, 16];
        let seeds = [0u64, 1, 2, 3];
        let out = run_sweep(
            &configs,
            &seeds,
            20,
            |&n, s| build(n, s),
            |&n, sim| (n, sim.model.total),
        );
        assert_eq!(out.len(), configs.len() * seeds.len());
        // More agents ⇒ higher average total (each agent adds every step).
        let mean = |c: usize| {
            let v: Vec<u64> = out
                .iter()
                .filter(|x| x.config == c)
                .map(|x| x.value.1)
                .collect();
            v.iter().sum::<u64>() as f64 / v.len() as f64
        };
        assert!(mean(0) < mean(1) && mean(1) < mean(2));
    }
}
