//! Runner de la simulación: [`Simulation`].

use crate::agent::{Agent, AgentId};
use crate::data::DataCollector;
use crate::model::Model;
use crate::rng::{SimRng, rng_from_seed};
use crate::schedule::{Activation, Schedule};

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
    /// Semilla original: raíz de los RNG por-agente de la fase `decide`.
    seed: u64,
    collector: DataCollector<M>,
    steps_done: u64,
    /// Buffer de orden de activación, reutilizado entre pasos para no
    /// asignar un `Vec` de ids por paso (relevante con millones de agentes).
    order_buf: Vec<AgentId>,
}

impl<M: Model> Simulation<M> {
    /// Crea una simulación con scheduler por defecto (activación aleatoria).
    #[must_use]
    pub fn new(model: M, seed: u64) -> Self {
        Self {
            model,
            schedule: Schedule::default(),
            rng: rng_from_seed(seed),
            seed,
            collector: DataCollector::new(),
            steps_done: 0,
            order_buf: Vec::new(),
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
    ///
    /// En activación simultánea, la fase `decide` corre **secuencial** con un
    /// RNG por-agente determinista; ver
    /// [`step_parallel`](Self::step_parallel) para correrla en paralelo con
    /// resultado bit-idéntico.
    pub fn step(&mut self) {
        self.begin_step();
        match self.schedule.activation() {
            Activation::Simultaneous => {
                self.decide_phase_seq();
                self.apply_phase();
            }
            _ => self.activate_sequential(),
        }
        self.end_step();
    }

    /// `before_step` + reconstrucción del buffer de ids vivos.
    fn begin_step(&mut self) {
        self.model.before_step(&mut self.rng);
        self.model.agents().collect_ids_into(&mut self.order_buf);
    }

    /// Activación secuencial (orden fijo o aleatorio) con el patrón take-out:
    /// el agente sale del set mientras corre su `step`, lo que permite pasarle
    /// `&mut self.model` sin doble préstamo.
    fn activate_sequential(&mut self) {
        self.schedule
            .order_in_place(&mut self.order_buf, &mut self.rng);
        for i in 0..self.order_buf.len() {
            let id = self.order_buf[i];
            if let Some(mut agent) = self.model.agents_mut().take(id) {
                agent.step(id, &mut self.model, &mut self.rng);
                self.model.agents_mut().put_back(id, agent);
            }
        }
    }

    /// Fase `decide` simultánea, **secuencial**. Saca el set del modelo (patrón
    /// double-buffer): así `decide` recibe `&Model` con el set vacío —lee el
    /// entorno y los snapshots, no los agentes vivos— y se devuelve al terminar.
    /// RNG por-agente: el resultado no depende del orden de recorrido.
    fn decide_phase_seq(&mut self) {
        let mut agents = std::mem::take(self.model.agents_mut());
        agents.decide_all(&self.model, self.seed, self.steps_done);
        *self.model.agents_mut() = agents;
    }

    /// Fase `apply` simultánea (siempre secuencial): materializa lo decidido y
    /// resuelve colisiones en orden de inserción. Los agentes creados en
    /// `apply` recién se activan en el paso siguiente.
    fn apply_phase(&mut self) {
        for i in 0..self.order_buf.len() {
            let id = self.order_buf[i];
            if let Some(mut agent) = self.model.agents_mut().take(id) {
                agent.apply(id, &mut self.model, &mut self.rng);
                self.model.agents_mut().put_back(id, agent);
            }
        }
    }

    /// `after_step` + avance del contador + recolección de datos.
    fn end_step(&mut self) {
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

/// Variante paralela de la fase `decide` simultánea (feature `parallel`).
///
/// Requiere `M: Sync` y `M::Agent: Send` (necesario para repartir el trabajo
/// entre hilos). Es una API **opcional y explícita**: [`step`](Simulation::step)
/// y [`run`](Simulation::run) siguen siendo genéricos y secuenciales, de modo
/// que los modelos que no son `Sync`/`Send` y el camino WASM no se ven afectados.
#[cfg(feature = "parallel")]
impl<M: Model> Simulation<M>
where
    M: Sync,
    M::Agent: Send,
{
    /// Como [`step`](Self::step), pero la fase `decide` de la activación
    /// simultánea corre **en paralelo** (rayon). El resultado es **idéntico bit
    /// a bit** al de `step`: el RNG de cada agente depende solo de
    /// `(semilla, paso, id)` —nunca del hilo— y `decide` recibe el modelo
    /// inmutable. Para activación no simultánea equivale a `step`.
    pub fn step_parallel(&mut self) {
        self.begin_step();
        match self.schedule.activation() {
            Activation::Simultaneous => {
                self.decide_phase_par();
                self.apply_phase();
            }
            _ => self.activate_sequential(),
        }
        self.end_step();
    }

    /// Como [`run`](Self::run) pero con [`step_parallel`](Self::step_parallel).
    pub fn run_parallel(&mut self, max_steps: u64) -> u64 {
        if self.collector.is_empty() {
            self.collector.collect(self.steps_done, &self.model);
        }
        let mut done = 0;
        while done < max_steps && !self.model.finished() {
            self.step_parallel();
            done += 1;
        }
        done
    }

    fn decide_phase_par(&mut self) {
        let mut agents = std::mem::take(self.model.agents_mut());
        agents.decide_all_par(&self.model, self.seed, self.steps_done);
        *self.model.agents_mut() = agents;
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
