//! Per-step ([`DataCollector`]) and per-agent ([`AgentDataCollector`]) data
//! series collection.

use std::fmt;
use std::fmt::Write as _;

use crate::agent::AgentId;
use crate::model::Model;

/// Reporter: extracts a scalar value from the model.
type Reporter<M> = Box<dyn Fn(&M) -> f64>;

struct Column<M> {
    name: String,
    reporter: Reporter<M>,
    values: Vec<f64>,
}

/// Collects time series (one `f64` per step and per reporter).
///
/// Reporters are `Fn(&M) -> f64` closures registered under a name;
/// [`collect`](Self::collect) evaluates all of them against the current
/// state.
pub struct DataCollector<M> {
    columns: Vec<Column<M>>,
    steps: Vec<u64>,
}

impl<M> Default for DataCollector<M> {
    fn default() -> Self {
        Self::new()
    }
}

impl<M> DataCollector<M> {
    /// Creates a collector with no reporters.
    #[must_use]
    pub fn new() -> Self {
        Self {
            columns: Vec::new(),
            steps: Vec::new(),
        }
    }

    /// Registers a named reporter. If the name already exists, it is
    /// replaced (the values already collected under that name are kept).
    pub fn add_reporter(
        &mut self,
        name: impl Into<String>,
        reporter: impl Fn(&M) -> f64 + 'static,
    ) {
        let name = name.into();
        if let Some(col) = self.columns.iter_mut().find(|c| c.name == name) {
            col.reporter = Box::new(reporter);
        } else {
            self.columns.push(Column {
                name,
                reporter: Box::new(reporter),
                values: Vec::new(),
            });
        }
    }

    /// Evaluates all reporters against `model` and appends a row.
    pub fn collect(&mut self, step: u64, model: &M) {
        self.steps.push(step);
        for col in &mut self.columns {
            col.values.push((col.reporter)(model));
        }
    }

    /// Full series for a reporter, or `None` if the name does not exist.
    #[must_use]
    pub fn series(&self, name: &str) -> Option<&[f64]> {
        self.columns
            .iter()
            .find(|c| c.name == name)
            .map(|c| c.values.as_slice())
    }

    /// Steps at which data was collected.
    #[must_use]
    pub fn steps(&self) -> &[u64] {
        &self.steps
    }

    /// Names of the registered reporters, in registration order.
    #[must_use]
    pub fn names(&self) -> Vec<&str> {
        self.columns.iter().map(|c| c.name.as_str()).collect()
    }

    /// Number of rows collected.
    #[must_use]
    pub fn len(&self) -> usize {
        self.steps.len()
    }

    /// `true` if no row has been collected yet.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }

    /// Serializes all series as CSV (`step,<reporters...>`).
    #[must_use]
    pub fn to_csv(&self) -> String {
        let mut out = String::from("step");
        for col in &self.columns {
            out.push(',');
            out.push_str(&col.name);
        }
        out.push('\n');
        for (i, step) in self.steps.iter().enumerate() {
            let _ = write!(out, "{step}");
            for col in &self.columns {
                let _ = write!(out, ",{}", col.values[i]);
            }
            out.push('\n');
        }
        out
    }
}

impl<M> fmt::Debug for DataCollector<M> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DataCollector")
            .field("reporters", &self.names())
            .field("rows", &self.len())
            .finish()
    }
}

/// Per-agent reporter: extracts a scalar value from a live agent.
type AgentReporter<A> = Box<dyn Fn(AgentId, &A) -> f64>;

struct AgentColumn<A> {
    name: String,
    reporter: AgentReporter<A>,
    /// `values[row][k] = (id, value)` for the k-th live agent in that row,
    /// in the `AgentSet` iteration order (see `AgentSet::iter`).
    values: Vec<Vec<(AgentId, f64)>>,
}

/// Collects **per-agent** series (complements [`DataCollector`], which
/// collects a single scalar from the *model* per step): distributions, not
/// just means — e.g. Sugarscape's Gini coefficient is currently computed
/// model-side because there was no way to ask for "the wealth of each
/// agent" directly.
///
/// Each reporter (`Fn(AgentId, &M::Agent) -> f64`) is evaluated over
/// **every** live agent at the time of [`collect`](Self::collect); unlike
/// `DataCollector`, each collected row is a `Vec` (one entry per agent), not
/// a scalar — with large populations and many rows this grows fast, so use
/// it judiciously (or with
/// [`Simulation::with_collect_every`](crate::sim::Simulation::with_collect_every)
/// to sample less often).
pub struct AgentDataCollector<M: Model> {
    columns: Vec<AgentColumn<M::Agent>>,
    steps: Vec<u64>,
}

impl<M: Model> Default for AgentDataCollector<M> {
    fn default() -> Self {
        Self::new()
    }
}

impl<M: Model> AgentDataCollector<M> {
    /// Creates a collector with no reporters.
    #[must_use]
    pub fn new() -> Self {
        Self {
            columns: Vec::new(),
            steps: Vec::new(),
        }
    }

    /// Registers a named per-agent reporter. If the name already exists, it
    /// is replaced (the values already collected under that name are kept).
    pub fn add_reporter(
        &mut self,
        name: impl Into<String>,
        reporter: impl Fn(AgentId, &M::Agent) -> f64 + 'static,
    ) {
        let name = name.into();
        if let Some(col) = self.columns.iter_mut().find(|c| c.name == name) {
            col.reporter = Box::new(reporter);
        } else {
            self.columns.push(AgentColumn {
                name,
                reporter: Box::new(reporter),
                values: Vec::new(),
            });
        }
    }

    /// Evaluates all reporters against every live agent of `model` and
    /// appends a row per reporter. Cheap no-op if no reporters are
    /// registered.
    pub fn collect(&mut self, step: u64, model: &M) {
        if self.columns.is_empty() {
            return;
        }
        self.steps.push(step);
        for col in &mut self.columns {
            let row: Vec<(AgentId, f64)> = model
                .agents()
                .iter()
                .map(|(id, a)| (id, (col.reporter)(id, a)))
                .collect();
            col.values.push(row);
        }
    }

    /// Full series for a reporter (one row per collected step, each row
    /// holding `(AgentId, value)` for every agent alive at that step), or
    /// `None` if the name does not exist.
    #[must_use]
    pub fn series(&self, name: &str) -> Option<&[Vec<(AgentId, f64)>]> {
        self.columns
            .iter()
            .find(|c| c.name == name)
            .map(|c| c.values.as_slice())
    }

    /// Steps at which data was collected.
    #[must_use]
    pub fn steps(&self) -> &[u64] {
        &self.steps
    }

    /// Names of the registered reporters, in registration order.
    #[must_use]
    pub fn names(&self) -> Vec<&str> {
        self.columns.iter().map(|c| c.name.as_str()).collect()
    }

    /// Number of rows collected.
    #[must_use]
    pub fn len(&self) -> usize {
        self.steps.len()
    }

    /// `true` if no row has been collected yet.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }
}

impl<M: Model> fmt::Debug for AgentDataCollector<M> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AgentDataCollector")
            .field("reporters", &self.names())
            .field("rows", &self.len())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recolecta_series_por_reporter() {
        let mut dc: DataCollector<i32> = DataCollector::new();
        dc.add_reporter("doble", |m| f64::from(*m) * 2.0);
        dc.add_reporter("mitad", |m| f64::from(*m) / 2.0);

        dc.collect(0, &10);
        dc.collect(1, &20);

        assert_eq!(dc.series("doble"), Some(&[20.0, 40.0][..]));
        assert_eq!(dc.series("mitad"), Some(&[5.0, 10.0][..]));
        assert_eq!(dc.series("nada"), None);
        assert_eq!(dc.steps(), &[0, 1]);
        assert_eq!(dc.len(), 2);
    }

    #[test]
    fn csv_bien_formado() {
        let mut dc: DataCollector<i32> = DataCollector::new();
        dc.add_reporter("x", |m| f64::from(*m));
        dc.collect(0, &1);
        dc.collect(1, &2);
        assert_eq!(dc.to_csv(), "step,x\n0,1\n1,2\n");
    }

    #[test]
    fn reporter_duplicado_se_reemplaza() {
        let mut dc: DataCollector<i32> = DataCollector::new();
        dc.add_reporter("x", |_| 1.0);
        dc.add_reporter("x", |_| 2.0);
        dc.collect(0, &0);
        assert_eq!(dc.names(), vec!["x"]);
        assert_eq!(dc.series("x"), Some(&[2.0][..]));
    }

    // --- AgentDataCollector -------------------------------------------

    use crate::agent::{Agent, AgentSet};

    struct Bicho {
        riqueza: u32,
    }

    struct Mundo {
        agents: AgentSet<Bicho>,
    }

    impl Agent for Bicho {
        type Model = Mundo;
    }

    impl Model for Mundo {
        type Agent = Bicho;
        fn agents(&self) -> &AgentSet<Bicho> {
            &self.agents
        }
        fn agents_mut(&mut self) -> &mut AgentSet<Bicho> {
            &mut self.agents
        }
    }

    fn mundo(riquezas: &[u32]) -> Mundo {
        let mut agents = AgentSet::new();
        for &r in riquezas {
            agents.insert(Bicho { riqueza: r });
        }
        Mundo { agents }
    }

    #[test]
    fn agent_collector_recolecta_un_valor_por_agente_vivo() {
        let mut adc: AgentDataCollector<Mundo> = AgentDataCollector::new();
        adc.add_reporter("riqueza", |_id, b: &Bicho| f64::from(b.riqueza));
        let m = mundo(&[10, 20, 30]);
        adc.collect(0, &m);
        let fila = adc.series("riqueza").expect("reporter registrado");
        assert_eq!(fila.len(), 1); // one step collected
        let valores: Vec<f64> = fila[0].iter().map(|&(_, v)| v).collect();
        assert_eq!(valores, vec![10.0, 20.0, 30.0]);
        assert_eq!(adc.steps(), &[0]);
    }

    #[test]
    fn agent_collector_no_recolecta_agentes_muertos() {
        let mut adc: AgentDataCollector<Mundo> = AgentDataCollector::new();
        adc.add_reporter("riqueza", |_id, b: &Bicho| f64::from(b.riqueza));
        let mut m = mundo(&[10, 20, 30]);
        let vivos = m.agents.ids();
        m.agents.remove(vivos[1]); // removes the middle one (20)
        adc.collect(0, &m);
        let valores: Vec<f64> = adc.series("riqueza").expect("reporter registrado")[0]
            .iter()
            .map(|&(_, v)| v)
            .collect();
        assert_eq!(valores, vec![10.0, 30.0]);
    }

    #[test]
    fn agent_collector_sin_reporters_no_recolecta_filas() {
        let mut adc: AgentDataCollector<Mundo> = AgentDataCollector::new();
        adc.collect(0, &mundo(&[1, 2]));
        assert!(
            adc.is_empty(),
            "with no reporters, collect() must not add rows"
        );
    }
}
