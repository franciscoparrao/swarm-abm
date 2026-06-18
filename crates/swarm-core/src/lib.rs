//! # swarm-core
//!
//! Motor de modelado basado en agentes (ABM) espacial: agentes sobre una
//! grilla 2D, scheduling configurable, recolección de series de datos y
//! reproducibilidad determinista (misma semilla → mismos resultados).
//!
//! ## Conceptos
//!
//! - [`Agent`](agent::Agent): comportamiento por paso de cada agente.
//! - [`Model`](model::Model): estado global (agentes + entorno espacial).
//! - [`Grid2D`](grid::Grid2D): grilla densa con vecindades Moore /
//!   Von Neumann y topología toroidal opcional.
//! - [`Schedule`](schedule::Schedule): activación en orden fijo, aleatorio o
//!   simultánea en dos fases (`decide` con modelo inmutable + `apply`).
//! - [`DataCollector`](data::DataCollector): series temporales por reporter.
//! - [`Simulation`](sim::Simulation): runner con RNG sembrado.
//!
//! ## Ejemplo: caminantes aleatorios
//!
//! ```
//! use swarm_core::prelude::*;
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
//!         if let Some(destino) =
//!             world.visits.random_neighbor(self.pos, Neighborhood::Moore, rng)
//!         {
//!             self.pos = destino;
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
//! sim.add_reporter("total_visitas", |w: &World| {
//!     w.visits.iter().map(|(_, &v)| f64::from(v)).sum()
//! });
//!
//! sim.run(100);
//! assert_eq!(sim.step_count(), 100);
//! assert_eq!(sim.data().series("total_visitas").unwrap().last(), Some(&100.0));
//! ```

#![warn(missing_docs)]
#![warn(clippy::unwrap_used)]

pub mod agent;
pub mod batch;
pub mod data;
pub mod grid;
pub mod model;
pub mod rng;
pub mod schedule;
pub mod sim;

/// Re-exporta los tipos necesarios para definir y correr un modelo.
pub mod prelude {
    pub use crate::agent::{Agent, AgentId, AgentSet};
    pub use crate::batch::{SweepCell, run_ensemble, run_sweep};
    pub use crate::data::DataCollector;
    pub use crate::grid::{Grid2D, Neighborhood, Pos};
    pub use crate::model::Model;
    pub use crate::rng::{SimRng, rng_from_seed};
    pub use crate::schedule::{Activation, Schedule};
    pub use crate::sim::Simulation;
    pub use rand::Rng;
    pub use rand::seq::SliceRandom;
}
