//! # swarm-abm
//!
//! Spatial agent-based modeling (ABM) engine: agents on a grid, graph, or
//! continuous space, configurable scheduling, time-series data collection,
//! batch runs and experiment design, and deterministic reproducibility —
//! same seed, same result, bit for bit, even under parallelism.
//!
//! ## Concepts
//!
//! - [`Agent`](agent::Agent): per-step behavior of each agent.
//! - [`Model`](model::Model): global state (agents + spatial environment).
//! - Three spatial paradigms:
//!   - [`Grid2D`](grid::Grid2D): dense 2D grid with Moore / Von Neumann
//!     neighborhoods, optional toroidal topology, and NetLogo-style
//!     [`diffuse`](grid::Grid2D::diffuse).
//!   - [`Graph`](graph::Graph): networks as space, with deterministic
//!     Erdős–Rényi, Watts–Strogatz, and Barabási–Albert generators.
//!   - [`ContinuousSpace`](continuous::ContinuousSpace): 2D continuous
//!     space with radius neighborhood queries via spatial hashing.
//! - [`Schedule`](schedule::Schedule): [`Activation`](schedule::Activation)
//!   policies — fixed-order, random (a fresh seeded permutation per step),
//!   two-phase simultaneous (`decide` with an immutable model + `apply`),
//!   or staged (Mesa-style `N` sweeps per step).
//! - [`DataCollector`](data::DataCollector) /
//!   [`AgentDataCollector`](data::AgentDataCollector): time series per
//!   reporter, model-level and per-agent.
//! - [`Simulation`](sim::Simulation): runner with a seeded RNG. With the
//!   `parallel` feature (default), the simultaneous `decide` phase runs
//!   across threads with results bit-identical to the sequential run; with
//!   the `serde` feature, checkpoint/resume via
//!   [`Simulation::from_checkpoint`](sim::Simulation::from_checkpoint).
//! - [`rng`]: engine-owned deterministic primitives
//!   ([`uniform_usize`](rng::uniform_usize), [`uniform_f64`](rng::uniform_f64),
//!   [`bernoulli`](rng::bernoulli), [`shuffle`](rng::shuffle),
//!   [`child_rng`](rng::child_rng)) — the basis of the reproducibility
//!   guarantee (see `docs/REPRODUCIBILITY.md`).
//! - [`batch`]: [`run_ensemble`](batch::run_ensemble) /
//!   [`run_sweep`](batch::run_sweep) for replicate ensembles and parameter
//!   sweeps (parallel with the `parallel` feature, sequential without it).
//! - `experiment` (feature `experiment`): deterministic experiment design —
//!   Sobol' global sensitivity analysis (Saltelli scheme with bootstrap),
//!   Morris screening, and Latin hypercube sampling.
//! - [`MultiAgent`](prelude::MultiAgent): derive macro for heterogeneous
//!   agents as an `enum`, dispatching to the active variant.
//!
//! ## Example: random walkers
//!
//! ```
//! use swarm_abm::prelude::*;
//!
//! struct Walker {
//!     pos: Pos,
//! }
//!
//! struct World {
//!     agents: AgentSet<Walker>,
//!     visits: Grid2D<u32>,
//! }
//!
//! impl Agent for Walker {
//!     type Model = World;
//!
//!     fn step(&mut self, _id: AgentId, world: &mut World, rng: &mut SimRng) {
//!         if let Some(dest) =
//!             world.visits.random_neighbor(self.pos, Neighborhood::Moore, rng)
//!         {
//!             self.pos = dest;
//!             world.visits[self.pos] += 1;
//!         }
//!     }
//! }
//!
//! impl Model for World {
//!     type Agent = Walker;
//!
//!     fn agents(&self) -> &AgentSet<Walker> {
//!         &self.agents
//!     }
//!
//!     fn agents_mut(&mut self) -> &mut AgentSet<Walker> {
//!         &mut self.agents
//!     }
//! }
//!
//! let mut agents = AgentSet::new();
//! agents.insert(Walker { pos: Pos::new(5, 5) });
//! let visits = Grid2D::new(10, 10).with_torus(true);
//!
//! let mut sim = Simulation::new(World { agents, visits }, 42);
//! sim.add_reporter("total_visits", |w: &World| {
//!     w.visits.iter().map(|(_, &v)| f64::from(v)).sum()
//! });
//!
//! sim.run(100);
//! assert_eq!(sim.step_count(), 100);
//! assert_eq!(sim.data().series("total_visits").unwrap().last(), Some(&100.0));
//! ```

#![warn(missing_docs)]
#![warn(clippy::unwrap_used)]

pub mod agent;
pub mod batch;
pub mod continuous;
pub mod data;
#[cfg(feature = "experiment")]
pub mod experiment;
pub mod graph;
pub mod grid;
pub mod model;
pub mod rng;
pub mod schedule;
pub mod sim;

/// Re-exports the types needed to define and run a model.
pub mod prelude {
    pub use crate::agent::{Agent, AgentId, AgentSet};
    pub use crate::batch::{SweepCell, run_ensemble, run_sweep};
    pub use crate::continuous::{ContinuousSpace, PointId, Vec2};
    pub use crate::data::DataCollector;
    pub use crate::graph::{Graph, NodeId};
    pub use crate::grid::{Grid2D, Neighborhood, Pos};
    pub use crate::model::Model;
    pub use crate::rng::{
        SimRng, bernoulli, rng_from_seed, shuffle, uniform_below, uniform_f64, uniform_usize,
    };
    pub use crate::schedule::{Activation, Schedule};
    pub use crate::sim::Simulation;
    /// Derives `impl Agent` for an `enum` of heterogeneous agents,
    /// dispatching `decide`/`apply`/`step` to the active variant's inner
    /// type. See the `swarm_abm_derive` rustdoc for each variant's
    /// requirements.
    pub use swarm_abm_derive::MultiAgent;
    // `rand::Rng`/`SliceRandom` remain available for arbitrary float ranges
    // or other uses not covered by the primitives above; for what they do
    // cover (indices, probability-weighted booleans, shuffling) prefer
    // `uniform_usize`/`bernoulli`/`shuffle`: they don't depend on `rand`'s
    // internal algorithm (see the note in `rng`).
    pub use rand::Rng;
    pub use rand::seq::SliceRandom;
}
