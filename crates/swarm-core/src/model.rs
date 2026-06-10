//! Trait [`Model`]: el estado global de la simulación.

use crate::agent::{Agent, AgentSet};
use crate::rng::SimRng;

/// Estado global de una simulación basada en agentes.
///
/// El modelo es dueño de sus agentes (vía [`AgentSet`]) y de cualquier
/// entorno espacial (p. ej. una [`Grid2D`](crate::grid::Grid2D)). El
/// [`Simulation`](crate::sim::Simulation) ejecuta cada paso así:
///
/// 1. [`before_step`](Self::before_step)
/// 2. `step` de cada agente, en el orden que dicte el
///    [`Schedule`](crate::schedule::Schedule)
/// 3. [`after_step`](Self::after_step)
/// 4. recolección de datos
pub trait Model: Sized {
    /// Tipo de agente que habita este modelo.
    type Agent: Agent<Model = Self>;

    /// Acceso de lectura al conjunto de agentes.
    fn agents(&self) -> &AgentSet<Self::Agent>;

    /// Acceso mutable al conjunto de agentes.
    fn agents_mut(&mut self) -> &mut AgentSet<Self::Agent>;

    /// Hook ejecutado antes de activar a los agentes. Por defecto no hace nada.
    fn before_step(&mut self, _rng: &mut SimRng) {}

    /// Hook ejecutado después de activar a los agentes. Por defecto no hace
    /// nada. Es el lugar correcto para bajas diferidas de agentes.
    fn after_step(&mut self, _rng: &mut SimRng) {}

    /// Condición de término. [`Simulation::run`](crate::sim::Simulation::run)
    /// se detiene cuando devuelve `true`. Por defecto, nunca.
    fn finished(&self) -> bool {
        false
    }
}
