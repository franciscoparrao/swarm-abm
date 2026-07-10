//! Runner for the simulation: [`Simulation`].

use crate::agent::{Agent, AgentId};
use crate::data::{AgentDataCollector, DataCollector};
use crate::model::Model;
use crate::rng::{SimRng, rng_from_seed};
use crate::schedule::{Activation, Schedule};

/// Runs a [`Model`] step by step, with a seeded RNG and data collection.
///
/// Each step: `before_step` → agents (in [`Schedule`] order) →
/// `after_step` → collection. [`run`](Self::run) also collects the
/// initial state (step 0) before the first step.
#[derive(Debug)]
pub struct Simulation<M: Model> {
    /// Simulated model, accessible for inspection and mutation between steps.
    pub model: M,
    schedule: Schedule,
    rng: SimRng,
    /// Original seed: root of the per-agent RNGs for the `decide` phase.
    seed: u64,
    collector: DataCollector<M>,
    agent_collector: AgentDataCollector<M>,
    /// Collection stride: data is only collected when `steps_done % every
    /// == 0` (see [`with_collect_every`](Self::with_collect_every)). `1`
    /// (default) collects every step, same as before this option existed.
    collect_every: u64,
    steps_done: u64,
    /// `true` once the initial state (step 0) has been collected. Checked
    /// instead of `collector.is_empty()` so the semantics don't depend on
    /// whether [`step`](Self::step) or [`run`](Self::run) is called first.
    initial_collected: bool,
    /// Activation order buffer, reused between steps to avoid allocating a
    /// `Vec` of ids per step (relevant with millions of agents).
    order_buf: Vec<AgentId>,
}

impl<M: Model> Simulation<M> {
    /// Creates a simulation with the default scheduler (random activation).
    #[must_use]
    pub fn new(model: M, seed: u64) -> Self {
        Self {
            model,
            schedule: Schedule::default(),
            rng: rng_from_seed(seed),
            seed,
            collector: DataCollector::new(),
            agent_collector: AgentDataCollector::new(),
            collect_every: 1,
            steps_done: 0,
            initial_collected: false,
            order_buf: Vec::new(),
        }
    }

    /// Reconstructs a simulation exactly where a previous run left off,
    /// from the **five minimal pieces** needed to resume bit-exactly: the
    /// model, the original seed, the current RNG state, the steps already
    /// run, and the [`Schedule`] that was in effect.
    ///
    /// `schedule` is a required parameter, not a default you can forget to
    /// override: for anything but `Activation::Random` (the type's
    /// `Default`), silently resuming under the wrong policy would activate
    /// agents in the wrong order (`Ordered`) or skip the decide/apply or
    /// staged phases entirely (`Simultaneous`/`Staged`, which fall back to
    /// plain sequential `step` under `Random`) — a checkpoint/resume that
    /// looks bit-exact but silently diverges from the original run. Pass
    /// the same [`Schedule`] the checkpointed simulation used (`Schedule`
    /// is `Copy`, so it costs nothing to keep around or re-derive).
    ///
    /// With the `serde` feature (which also enables `rand_chacha/serde`),
    /// [`model`](Self::model) is public and [`rng_state`](Self::rng_state)/
    /// [`seed`](Self::seed)/[`step_count`](Self::step_count) are
    /// serializable, so a checkpoint is as simple as serializing those four
    /// pieces (in whatever format you prefer — JSON, bincode, whatever)
    /// and reconstructing them here (alongside the schedule, which the
    /// caller already has) to continue.
    ///
    /// Does **not** restore reporters or already-collected data: reporters
    /// are closures (`Box<dyn Fn>`), which are generally not serializable.
    /// Re-register the ones you need with
    /// [`add_reporter`](Self::add_reporter)/
    /// [`add_agent_reporter`](Self::add_agent_reporter) after
    /// reconstructing. The collection stride is **not** restored either:
    /// it silently resets to `1`, so if the original run used
    /// [`with_collect_every`](Self::with_collect_every), chain
    /// `.with_collect_every(k)` with the **same** `k` on the reconstructed
    /// simulation — otherwise the resumed run collects every step and its
    /// data axis diverges from the uninterrupted run's. The initial state
    /// (step 0) is considered already collected in the previous session —
    /// `run`/`step` on the reconstructed simulation will not capture it
    /// again.
    #[must_use]
    pub fn from_checkpoint(
        model: M,
        seed: u64,
        rng: SimRng,
        steps_done: u64,
        schedule: Schedule,
    ) -> Self {
        Self {
            model,
            schedule,
            rng,
            seed,
            collector: DataCollector::new(),
            agent_collector: AgentDataCollector::new(),
            collect_every: 1,
            steps_done,
            initial_collected: true,
            order_buf: Vec::new(),
        }
    }

    /// Original seed of the simulation (root of `child_rng`; see
    /// [`from_checkpoint`](Self::from_checkpoint)).
    #[must_use]
    pub fn seed(&self) -> u64 {
        self.seed
    }

    /// Current state of the step RNG (the one that decides the order for
    /// `Activation::Random` and feeds `before_step`/`after_step` — not the
    /// derived per-agent RNG, which is always a pure function of `(seed,
    /// step, id)` and doesn't need to be saved). Together with
    /// [`model`](Self::model) (public), [`seed`](Self::seed), and
    /// [`step_count`](Self::step_count) it is everything needed for a
    /// bit-exact checkpoint — see
    /// [`from_checkpoint`](Self::from_checkpoint).
    #[must_use]
    pub fn rng_state(&self) -> &SimRng {
        &self.rng
    }

    /// Replaces the scheduler (builder).
    #[must_use]
    pub fn with_schedule(mut self, schedule: Schedule) -> Self {
        self.schedule = schedule;
        self
    }

    /// Collects only 1 out of every `every` steps, instead of all of them
    /// (builder). Step 0 (initial state) is always collected, regardless
    /// of `every` (`0 % every == 0` for any `every > 0`). Useful for
    /// running millions of steps without accumulating one row per step in
    /// memory. Applies equally to [`DataCollector`] and
    /// [`AgentDataCollector`], so their `steps()` axes stay aligned.
    ///
    /// # Panics
    /// If `every == 0`.
    #[must_use]
    pub fn with_collect_every(mut self, every: u64) -> Self {
        assert!(every > 0, "collect_every must be > 0");
        self.collect_every = every;
        self
    }

    /// Registers a model-level data reporter (see
    /// [`DataCollector::add_reporter`]).
    pub fn add_reporter(
        &mut self,
        name: impl Into<String>,
        reporter: impl Fn(&M) -> f64 + 'static,
    ) {
        self.collector.add_reporter(name, reporter);
    }

    /// Registers an **agent**-level data reporter (see
    /// [`AgentDataCollector::add_reporter`]): evaluated on every live agent
    /// at each collection, not just on the model.
    pub fn add_agent_reporter(
        &mut self,
        name: impl Into<String>,
        reporter: impl Fn(AgentId, &M::Agent) -> f64 + 'static,
    ) {
        self.agent_collector.add_reporter(name, reporter);
    }

    /// Collects the initial state (step 0) the first time it's invoked,
    /// regardless of whether it's reached via [`step`](Self::step) or
    /// [`run`](Self::run): the semantics of "step 0 is always in
    /// `data()`" don't depend on which entry point the caller used.
    fn collect_initial_if_needed(&mut self) {
        if !self.initial_collected {
            self.collector.collect(self.steps_done, &self.model);
            self.agent_collector.collect(self.steps_done, &self.model);
            self.initial_collected = true;
        }
    }

    /// Collects the row for `steps_done` if due, according to
    /// [`with_collect_every`](Self::with_collect_every).
    fn collect_if_due(&mut self) {
        if self.steps_done.is_multiple_of(self.collect_every) {
            self.collector.collect(self.steps_done, &self.model);
            self.agent_collector.collect(self.steps_done, &self.model);
        }
    }

    /// Runs exactly one step of the simulation.
    ///
    /// In simultaneous activation, the `decide` phase runs **sequentially**
    /// with a deterministic per-agent RNG, and does not see the other
    /// agents (only the model's environment); see
    /// [`step_parallel`](Self::step_parallel) to run it in parallel with a
    /// bit-identical result, and
    /// [`step_with_peers`](Self::step_with_peers) to have it see the other
    /// agents.
    pub fn step(&mut self) {
        self.collect_initial_if_needed();
        self.begin_step();
        match self.schedule.activation() {
            Activation::Simultaneous => {
                self.decide_phase_seq();
                self.apply_phase();
            }
            Activation::Staged(n) => self.staged_phases(n),
            _ => self.activate_sequential(),
        }
        self.end_step();
    }

    /// `before_step` + rebuild of the live ids buffer.
    fn begin_step(&mut self) {
        self.model.before_step(&mut self.rng);
        self.model.agents().collect_ids_into(&mut self.order_buf);
    }

    /// Sequential activation (fixed or random order) using the take-out
    /// pattern: the agent is removed from the set while its `step` runs,
    /// which allows passing it `&mut self.model` without a double borrow.
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

    /// Simultaneous `decide` phase, **sequential**. Takes the set out of
    /// the model (double-buffer pattern): this way `decide` receives
    /// `&Model` with an empty set — it reads the environment and the
    /// snapshots, not the live agents — and it's put back when done.
    /// Per-agent RNG: the result does not depend on iteration order.
    fn decide_phase_seq(&mut self) {
        let mut agents = std::mem::take(self.model.agents_mut());
        agents.decide_all(&self.model, self.seed, self.steps_done);
        *self.model.agents_mut() = agents;
    }

    /// Simultaneous `apply` phase (always sequential): materializes what
    /// was decided and resolves collisions in slot-index order — which
    /// equals insertion order until a removal is followed by an insertion
    /// (see [`AgentSet`](crate::agent::AgentSet)). Agents created in
    /// `apply` only become active on the following step.
    fn apply_phase(&mut self) {
        for i in 0..self.order_buf.len() {
            let id = self.order_buf[i];
            if let Some(mut agent) = self.model.agents_mut().take(id) {
                agent.apply(id, &mut self.model, &mut self.rng);
                self.model.agents_mut().put_back(id, agent);
            }
        }
    }

    /// Staged activation ([`Activation::Staged`]): `n` complete sweeps
    /// over all agents, in the same order across the `n` stages (no
    /// reshuffling between stages). Each sweep is its own take-out cycle,
    /// so stage `s` sees the model exactly as it was left after **all**
    /// agents completed stage `s-1`.
    fn staged_phases(&mut self, n: usize) {
        self.schedule
            .order_in_place(&mut self.order_buf, &mut self.rng);
        for stage in 0..n {
            self.stage_phase(stage);
        }
    }

    fn stage_phase(&mut self, stage: usize) {
        for i in 0..self.order_buf.len() {
            let id = self.order_buf[i];
            if let Some(mut agent) = self.model.agents_mut().take(id) {
                agent.stage(stage, id, &mut self.model, &mut self.rng);
                self.model.agents_mut().put_back(id, agent);
            }
        }
    }

    /// `after_step` + counter advance + data collection.
    fn end_step(&mut self) {
        self.model.after_step(&mut self.rng);
        self.steps_done += 1;
        self.collect_if_due();
    }

    /// Runs up to `max_steps` steps, stopping early if
    /// [`Model::finished`] returns `true`. Returns the number of steps
    /// executed in this call.
    ///
    /// If nothing has been collected yet, first collects the initial
    /// state (step 0) — even if [`step`](Self::step) was already called
    /// before, that first call already did it and this is a no-op.
    pub fn run(&mut self, max_steps: u64) -> u64 {
        self.collect_initial_if_needed();
        let mut done = 0;
        while done < max_steps && !self.model.finished() {
            self.step();
            done += 1;
        }
        done
    }

    /// Steps executed since creation.
    #[must_use]
    pub fn step_count(&self) -> u64 {
        self.steps_done
    }

    /// Data collected at the model level.
    #[must_use]
    pub fn data(&self) -> &DataCollector<M> {
        &self.collector
    }

    /// Data collected at the agent level (see
    /// [`add_agent_reporter`](Self::add_agent_reporter)).
    #[must_use]
    pub fn agent_data(&self) -> &AgentDataCollector<M> {
        &self.agent_collector
    }

    /// Mutable access to the RNG (e.g. for later initializations).
    pub fn rng_mut(&mut self) -> &mut SimRng {
        &mut self.rng
    }
}

/// Variant of the simultaneous `decide` phase with peer visibility (see
/// [`Agent::decide_with_peers`]). Requires `M::Agent: Clone` to take a
/// frozen snapshot of the `AgentSet` at the start of the phase — a
/// **separate** API from [`step`](Simulation::step)/[`run`](Simulation::run)
/// so that models that don't need `decide` to see other agents (most of
/// them) don't pay for that bound: `Simulation<M>::step`/`run` remain
/// generic without requiring `Clone`.
impl<M: Model> Simulation<M>
where
    M::Agent: Clone,
{
    /// Like [`step`](Self::step), but in simultaneous activation it invokes
    /// [`Agent::decide_with_peers`] (with a snapshot of the set taken at
    /// the start of the phase) instead of [`Agent::decide`]. For
    /// non-simultaneous activation it is equivalent to `step`.
    pub fn step_with_peers(&mut self) {
        self.collect_initial_if_needed();
        self.begin_step();
        match self.schedule.activation() {
            Activation::Simultaneous => {
                self.decide_phase_with_peers();
                self.apply_phase();
            }
            Activation::Staged(n) => self.staged_phases(n),
            _ => self.activate_sequential(),
        }
        self.end_step();
    }

    /// Like [`run`](Self::run) but with
    /// [`step_with_peers`](Self::step_with_peers).
    pub fn run_with_peers(&mut self, max_steps: u64) -> u64 {
        self.collect_initial_if_needed();
        let mut done = 0;
        while done < max_steps && !self.model.finished() {
            self.step_with_peers();
            done += 1;
        }
        done
    }

    fn decide_phase_with_peers(&mut self) {
        let mut agents = std::mem::take(self.model.agents_mut());
        let peers = agents.clone();
        agents.decide_all_with_peers(&self.model, &peers, self.seed, self.steps_done);
        *self.model.agents_mut() = agents;
    }
}

/// Parallel variant of the simultaneous `decide` phase (feature `parallel`).
///
/// Requires `M: Sync` and `M::Agent: Send` (needed to distribute the work
/// across threads). It's an **optional and explicit** API:
/// [`step`](Simulation::step) and [`run`](Simulation::run) remain generic
/// and sequential, so models that aren't `Sync`/`Send` and the WASM path
/// are unaffected.
#[cfg(feature = "parallel")]
impl<M: Model> Simulation<M>
where
    M: Sync,
    M::Agent: Send,
{
    /// Like [`step`](Self::step), but the `decide` phase of simultaneous
    /// activation runs **in parallel** (rayon). The result is
    /// **bit-identical** to that of `step`: each agent's RNG depends only
    /// on `(seed, step, id)` — never on the thread — and `decide` receives
    /// the model immutably. For non-simultaneous activation it is
    /// equivalent to `step`.
    pub fn step_parallel(&mut self) {
        self.collect_initial_if_needed();
        self.begin_step();
        match self.schedule.activation() {
            Activation::Simultaneous => {
                self.decide_phase_par();
                self.apply_phase();
            }
            Activation::Staged(n) => self.staged_phases(n),
            _ => self.activate_sequential(),
        }
        self.end_step();
    }

    /// Like [`run`](Self::run) but with [`step_parallel`](Self::step_parallel).
    pub fn run_parallel(&mut self, max_steps: u64) -> u64 {
        self.collect_initial_if_needed();
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

/// Combines [`step_with_peers`](Simulation::step_with_peers) and
/// [`step_parallel`](Simulation::step_parallel): the `decide` phase sees
/// the other agents **and** runs in parallel, with the same bit-for-bit
/// identity guarantee as `step_parallel` (the `peers` snapshot is
/// immutable and shared across threads; each agent's RNG still depends
/// only on `(seed, step, id)`).
#[cfg(feature = "parallel")]
impl<M: Model> Simulation<M>
where
    M: Sync,
    M::Agent: Send + Sync + Clone,
{
    /// Like [`step_with_peers`](Simulation::step_with_peers), but the
    /// `decide` phase runs in parallel.
    pub fn step_with_peers_parallel(&mut self) {
        self.collect_initial_if_needed();
        self.begin_step();
        match self.schedule.activation() {
            Activation::Simultaneous => {
                self.decide_phase_with_peers_par();
                self.apply_phase();
            }
            Activation::Staged(n) => self.staged_phases(n),
            _ => self.activate_sequential(),
        }
        self.end_step();
    }

    /// Like [`run_with_peers`](Simulation::run_with_peers) but with
    /// [`step_with_peers_parallel`](Self::step_with_peers_parallel).
    pub fn run_with_peers_parallel(&mut self, max_steps: u64) -> u64 {
        self.collect_initial_if_needed();
        let mut done = 0;
        while done < max_steps && !self.model.finished() {
            self.step_with_peers_parallel();
            done += 1;
        }
        done
    }

    fn decide_phase_with_peers_par(&mut self) {
        let mut agents = std::mem::take(self.model.agents_mut());
        let peers = agents.clone();
        agents.decide_all_with_peers_par(&self.model, &peers, self.seed, self.steps_done);
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
        // finished after 3 steps (5 agents × 3 = 15).
        let mut sim = Simulation::new(mundo(5, 15), 1);
        assert_eq!(sim.run(100), 3);

        // max_steps cuts off before finished.
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

    #[test]
    fn step_manual_antes_de_run_no_pierde_el_paso_0() {
        // Regression: `run()` only collected the initial state if the
        // collector was empty; if the user called `step()` manually
        // first (e.g. to initialize something before running in bulk),
        // step 0 was never collected and the `steps()` axis started
        // off by 1 with no warning.
        let mut sim = Simulation::new(mundo(2, u64::MAX), 1);
        sim.add_reporter("total", |m: &Mundo| m.total as f64);
        sim.step(); // total becomes 2, BEFORE run() collects anything
        sim.run(2);
        assert_eq!(sim.data().steps(), &[0, 1, 2, 3]);
        assert_eq!(sim.data().series("total"), Some(&[0.0, 2.0, 4.0, 6.0][..]));
    }

    #[test]
    fn run_dos_veces_no_duplica_el_paso_0() {
        let mut sim = Simulation::new(mundo(2, u64::MAX), 1);
        sim.add_reporter("total", |m: &Mundo| m.total as f64);
        sim.run(1);
        sim.run(2);
        assert_eq!(sim.data().steps(), &[0, 1, 2, 3]);
    }

    #[test]
    fn add_agent_reporter_recolecta_un_valor_por_agente_cada_paso() {
        let mut sim = Simulation::new(mundo(3, u64::MAX), 1);
        sim.add_agent_reporter("valor", |_id, a: &Contador| a.valor as f64);
        sim.run(2);
        let filas = sim
            .agent_data()
            .series("valor")
            .expect("reporter registrado");
        assert_eq!(filas.len(), 3, "step 0, 1, 2");
        assert_eq!(sim.agent_data().steps(), &[0, 1, 2]);
        // Step 0: all three agents start at 0.
        let valores0: Vec<f64> = filas[0].iter().map(|&(_, v)| v).collect();
        assert_eq!(valores0, vec![0.0, 0.0, 0.0]);
        // Step 2: each one incremented its `valor` once per step.
        let valores2: Vec<f64> = filas[2].iter().map(|&(_, v)| v).collect();
        assert_eq!(valores2, vec![2.0, 2.0, 2.0]);
    }

    #[test]
    fn with_collect_every_reduce_las_filas_pero_conserva_el_paso_0() {
        let mut sim = Simulation::new(mundo(2, u64::MAX), 1).with_collect_every(3);
        sim.add_reporter("total", |m: &Mundo| m.total as f64);
        sim.run(9);
        // Steps 0,3,6,9 -- the 9 because run(9) ends exactly on a
        // multiple of 3.
        assert_eq!(sim.data().steps(), &[0, 3, 6, 9]);
        // The agent_data axis stays aligned with that of data().
        sim.add_agent_reporter("valor", |_id, a: &Contador| a.valor as f64);
        // (the reporter was added late, it doesn't affect the axis already
        // collected by the base DataCollector, but confirms both share
        // `every`)
        let mut sim2 = Simulation::new(mundo(2, u64::MAX), 1).with_collect_every(3);
        sim2.add_agent_reporter("valor", |_id, a: &Contador| a.valor as f64);
        sim2.run(9);
        assert_eq!(sim2.agent_data().steps(), &[0, 3, 6, 9]);
    }

    #[test]
    #[should_panic(expected = "collect_every must be > 0")]
    fn with_collect_every_cero_panica() {
        let _ = Simulation::new(mundo(1, u64::MAX), 1).with_collect_every(0);
    }

    #[test]
    fn from_checkpoint_con_collect_every_reaplicado_mantiene_el_eje() {
        // Regression (audit M1): `from_checkpoint` resets `collect_every`
        // to 1, so a resumed run must re-apply `.with_collect_every(k)`
        // with the same `k` as the original run to keep the data axis
        // aligned with the uninterrupted one (documented contract).

        // Uninterrupted run: 12 steps, collecting every 3.
        let mut completa = Simulation::new(mundo(2, u64::MAX), 7).with_collect_every(3);
        completa.add_reporter("total", |m: &Mundo| m.total as f64);
        completa.run(12);
        assert_eq!(completa.data().steps(), &[0, 3, 6, 9, 12]);

        // Interrupted run: 6 steps, checkpoint, resume for 6 more with
        // the SAME stride re-applied.
        let mut primera = Simulation::new(mundo(2, u64::MAX), 7).with_collect_every(3);
        primera.add_reporter("total", |m: &Mundo| m.total as f64);
        primera.run(6);
        let rng = primera.rng_state().clone();
        let (seed, steps) = (primera.seed(), primera.step_count());
        // Own copies of the pre-checkpoint data: `from_checkpoint` takes
        // the model by value, consuming `primera`.
        let eje_previo: Vec<u64> = primera.data().steps().to_vec();
        let serie_previa: Vec<f64> = primera
            .data()
            .series("total")
            .expect("reporter registrado")
            .to_vec();
        let mut reanudada =
            Simulation::from_checkpoint(primera.model, seed, rng, steps, Schedule::default())
                .with_collect_every(3);
        reanudada.add_reporter("total", |m: &Mundo| m.total as f64);
        reanudada.run(6);

        // The concatenated axes reproduce the uninterrupted axis exactly.
        let eje: Vec<u64> = eje_previo
            .iter()
            .chain(reanudada.data().steps())
            .copied()
            .collect();
        assert_eq!(eje.as_slice(), completa.data().steps());

        // And the series values match too (2 agents, +2 per step).
        let serie: Vec<f64> = serie_previa
            .iter()
            .chain(
                reanudada
                    .data()
                    .series("total")
                    .expect("reporter registrado"),
            )
            .copied()
            .collect();
        assert_eq!(
            serie.as_slice(),
            completa
                .data()
                .series("total")
                .expect("reporter registrado")
        );
    }
}
