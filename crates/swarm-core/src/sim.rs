//! Runner de la simulación: [`Simulation`].

use crate::agent::Agent;
use crate::data::DataCollector;
use crate::model::Model;
use crate::rng::{SimRng, rng_from_seed};
use crate::schedule::Schedule;

/// Ejecuta un [`Model`] paso a paso, con RNG sembrado y recolección de datos.
///
/// Cada paso: `before_step` → agentes (en el orden del [`Schedule`]) →
/// `after_step` → recolección. [`run`](Self::run) recolecta además el estado
/// inicial (paso 0) antes del primer paso.
#[derive(Debug)]
pub struct Simulation<M: Model> {
    /// Modelo simulado, accesible para inspección y mutación entre pasos.
    pub model: M,
    schedule: Schedule,
    rng: SimRng,
    collector: DataCollector<M>,
    steps_done: u64,
}

impl<M: Model> Simulation<M> {
    /// Crea una simulación con scheduler por defecto (activación aleatoria).
    #[must_use]
    pub fn new(model: M, seed: u64) -> Self {
        Self {
            model,
            schedule: Schedule::default(),
            rng: rng_from_seed(seed),
            collector: DataCollector::new(),
            steps_done: 0,
        }
    }

    /// Reemplaza el scheduler (builder).
    #[must_use]
    pub fn with_schedule(mut self, schedule: Schedule) -> Self {
        self.schedule = schedule;
        self
    }

    /// Registra un reporter de datos (ver [`DataCollector::add_reporter`]).
    pub fn add_reporter(
        &mut self,
        name: impl Into<String>,
        reporter: impl Fn(&M) -> f64 + 'static,
    ) {
        self.collector.add_reporter(name, reporter);
    }

    /// Ejecuta exactamente un paso de la simulación.
    pub fn step(&mut self) {
        self.model.before_step(&mut self.rng);

        let ids = self.model.agents().ids();
        for id in self.schedule.order(ids, &mut self.rng) {
            // Patrón take-out: el agente sale del set mientras corre su step,
            // lo que permite pasarle `&mut self.model` sin doble préstamo.
            if let Some(mut agent) = self.model.agents_mut().take(id) {
                agent.step(id, &mut self.model, &mut self.rng);
                self.model.agents_mut().put_back(id, agent);
            }
        }

        self.model.after_step(&mut self.rng);
        self.steps_done += 1;
        self.collector.collect(self.steps_done, &self.model);
    }

    /// Ejecuta hasta `max_steps` pasos, deteniéndose antes si
    /// [`Model::finished`] devuelve `true`. Devuelve los pasos ejecutados
    /// en esta llamada.
    ///
    /// Si aún no se recolectó nada, recolecta primero el estado inicial
    /// (paso 0).
    pub fn run(&mut self, max_steps: u64) -> u64 {
        if self.collector.is_empty() {
            self.collector.collect(self.steps_done, &self.model);
        }
        let mut done = 0;
        while done < max_steps && !self.model.finished() {
            self.step();
            done += 1;
        }
        done
    }

    /// Pasos ejecutados desde la creación.
    #[must_use]
    pub fn step_count(&self) -> u64 {
        self.steps_done
    }

    /// Datos recolectados.
    #[must_use]
    pub fn data(&self) -> &DataCollector<M> {
        &self.collector
    }

    /// Acceso mutable al RNG (p. ej. para inicializaciones posteriores).
    pub fn rng_mut(&mut self) -> &mut SimRng {
        &mut self.rng
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{AgentId, AgentSet};

    struct Contador {
        valor: u64,
    }

    struct Mundo {
        agents: AgentSet<Contador>,
        total: u64,
        tope: u64,
    }

    impl Agent for Contador {
        type Model = Mundo;
        fn step(&mut self, _id: AgentId, model: &mut Mundo, _rng: &mut SimRng) {
            self.valor += 1;
            model.total += 1;
        }
    }

    impl Model for Mundo {
        type Agent = Contador;
        fn agents(&self) -> &AgentSet<Contador> {
            &self.agents
        }
        fn agents_mut(&mut self) -> &mut AgentSet<Contador> {
            &mut self.agents
        }
        fn finished(&self) -> bool {
            self.total >= self.tope
        }
    }

    fn mundo(n_agentes: usize, tope: u64) -> Mundo {
        let mut agents = AgentSet::new();
        for _ in 0..n_agentes {
            agents.insert(Contador { valor: 0 });
        }
        Mundo {
            agents,
            total: 0,
            tope,
        }
    }

    #[test]
    fn step_activa_todos_los_agentes() {
        let mut sim = Simulation::new(mundo(5, u64::MAX), 1);
        sim.step();
        assert_eq!(sim.model.total, 5);
        assert!(sim.model.agents.iter().all(|(_, a)| a.valor == 1));
        assert_eq!(sim.step_count(), 1);
    }

    #[test]
    fn run_respeta_finished_y_max_steps() {
        // finished tras 3 pasos (5 agentes × 3 = 15).
        let mut sim = Simulation::new(mundo(5, 15), 1);
        assert_eq!(sim.run(100), 3);

        // max_steps corta antes del finished.
        let mut sim = Simulation::new(mundo(5, u64::MAX), 1);
        assert_eq!(sim.run(7), 7);
    }

    #[test]
    fn run_recolecta_estado_inicial() {
        let mut sim = Simulation::new(mundo(2, u64::MAX), 1);
        sim.add_reporter("total", |m: &Mundo| m.total as f64);
        sim.run(3);
        assert_eq!(sim.data().steps(), &[0, 1, 2, 3]);
        assert_eq!(sim.data().series("total"), Some(&[0.0, 2.0, 4.0, 6.0][..]));
    }
}
