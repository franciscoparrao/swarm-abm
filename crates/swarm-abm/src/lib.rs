//! # swarm-abm
//!
//! Spatial agent-based modeling (ABM) engine: agents on a 2D grid,
//! configurable scheduling, time-series data collection, and deterministic
//! reproducibility (same seed → same results).
//!
//! ## Concepts
//!
//! - [`Agent`](agent::Agent): per-step behavior of each agent.
//! - [`Model`](model::Model): global state (agents + spatial environment).
//! - [`Grid2D`](grid::Grid2D): dense grid with Moore / Von Neumann
//!   neighborhoods and optional toroidal topology.
//! - [`Schedule`](schedule::Schedule): fixed-order, random, or two-phase
//!   simultaneous activation (`decide` with an immutable model + `apply`).
//! - [`DataCollector`](data::DataCollector): time series per reporter.
//! - [`Simulation`](sim::Simulation): runner with a seeded RNG.
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
