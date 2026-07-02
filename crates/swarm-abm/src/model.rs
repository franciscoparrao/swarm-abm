//! [`Model`] trait: the global state of the simulation.

use crate::agent::{Agent, AgentSet};
use crate::rng::SimRng;

/// Global state of an agent-based simulation.
///
/// The model owns its agents (via [`AgentSet`]) and any spatial environment
/// (e.g. a [`Grid2D`](crate::grid::Grid2D)). The
/// [`Simulation`](crate::sim::Simulation) executes each step as follows:
///
/// 1. [`before_step`](Self::before_step)
/// 2. each agent's `step`, in the order dictated by the
///    [`Schedule`](crate::schedule::Schedule)
/// 3. [`after_step`](Self::after_step)
/// 4. data collection
pub trait Model: Sized {
    /// Type of agent that inhabits this model.
    type Agent: Agent<Model = Self>;

    /// Read-only access to the agent set.
    fn agents(&self) -> &AgentSet<Self::Agent>;

    /// Mutable access to the agent set.
    fn agents_mut(&mut self) -> &mut AgentSet<Self::Agent>;

    /// Hook run before activating the agents. Does nothing by default.
    fn before_step(&mut self, _rng: &mut SimRng) {}

    /// Hook run after activating the agents. Does nothing by default. This
    /// is the right place for deferred agent removals.
    fn after_step(&mut self, _rng: &mut SimRng) {}

    /// Termination condition. [`Simulation::run`](crate::sim::Simulation::run)
    /// stops when this returns `true`. Never, by default.
    fn finished(&self) -> bool {
        false
    }
}
