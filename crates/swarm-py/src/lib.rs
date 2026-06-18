//! Bindings Python (PyO3) de swarm-abm.
//!
//! Estrategia: **modelos nativos + barridos**. Se exponen modelos del motor
//! ya compilados en Rust como clases Python parametrizables; el bucle de
//! simulación corre íntegro en Rust (conserva el speedup ~45–67× sobre Mesa)
//! y Python solo configura, dispara y recibe las series para analizarlas con
//! numpy/pandas/matplotlib. Los barridos de parámetros corren en paralelo
//! (rayon) liberando el GIL.
//!
//! ```python
//! import swarm_abm as sw
//! m = sw.Sir(size=200, beta=0.3, seed=42)
//! m.run(500)
//! infected = m.series("i")                  # -> list[float]
//! rows = sw.sir_sweep(betas=[0.1, 0.2, 0.3], seeds=range(50))
//! ```

use pyo3::prelude::*;

use swarm_core::batch::run_sweep;
use swarm_core::prelude::*;
use swarm_models::schelling::{self, Schelling as SchellingModel, SchellingConfig};
use swarm_models::sir::{self, Sir as SirModel, SirConfig, Status};
use swarm_models::sugarscape::{self, Sugarscape as SugarscapeModel, SugarscapeConfig};

/// Modelo SIR espacial sobre una grilla toroidal (ver `swarm_models::sir`).
///
/// El estado y el bucle viven en Rust; los métodos exponen control (`run`) y
/// lectura (`series`, getters de fracciones) hacia Python.
#[pyclass(name = "Sir", unsendable)]
struct PySir {
    sim: Simulation<SirModel>,
}

#[pymethods]
impl PySir {
    /// Crea el modelo. `size` es el lado de la grilla cuadrada.
    #[new]
    #[pyo3(signature = (size = 100, beta = 0.08, gamma = 0.1, initial_infected = 10, seed = 0))]
    fn new(size: usize, beta: f64, gamma: f64, initial_infected: usize, seed: u64) -> Self {
        let cfg = SirConfig {
            width: size,
            height: size,
            initial_infected,
            beta,
            gamma,
        };
        let mut sim = Simulation::new(sir::build(cfg, seed), seed);
        sim.add_reporter("s", |m: &SirModel| m.fraction(Status::Susceptible));
        sim.add_reporter("i", |m: &SirModel| m.fraction(Status::Infected));
        sim.add_reporter("r", |m: &SirModel| m.fraction(Status::Recovered));
        Self { sim }
    }

    /// Avanza la simulación hasta `steps` pasos (o hasta que no queden
    /// infectados). Devuelve cuántos pasos se ejecutaron realmente.
    fn run(&mut self, steps: u64) -> u64 {
        self.sim.run(steps)
    }

    /// Serie temporal de una métrica recolectada (`"s"`, `"i"` o `"r"`),
    /// como lista de fracciones por paso. `None` si el nombre no existe.
    fn series(&self, name: &str) -> Option<Vec<f64>> {
        self.sim.data().series(name).map(<[f64]>::to_vec)
    }

    /// Pasos en los que se recolectaron datos (eje temporal de `series`).
    fn steps(&self) -> Vec<u64> {
        self.sim.data().steps().to_vec()
    }

    /// Pasos ejecutados desde la creación.
    #[getter]
    fn step_count(&self) -> u64 {
        self.sim.step_count()
    }

    /// Fracción actual de susceptibles.
    #[getter]
    fn susceptible(&self) -> f64 {
        self.sim.model.fraction(Status::Susceptible)
    }

    /// Fracción actual de infectados.
    #[getter]
    fn infected(&self) -> f64 {
        self.sim.model.fraction(Status::Infected)
    }

    /// Fracción actual de recuperados (tamaño final de la epidemia).
    #[getter]
    fn recovered(&self) -> f64 {
        self.sim.model.fraction(Status::Recovered)
    }

    /// `True` si ya no quedan infectados.
    #[getter]
    fn finished(&self) -> bool {
        self.sim.model.finished()
    }
}

/// Barrido de parámetros del SIR, en paralelo y a velocidad Rust.
///
/// Corre el producto cartesiano `betas × seeds` (una réplica por celda) hasta
/// `max_steps` pasos, repartido entre hilos con el GIL liberado. Devuelve una
/// fila `(beta, seed, peak, r_final)` por réplica, lista para volcar a un
/// `pandas.DataFrame`.
#[pyfunction]
#[pyo3(signature = (betas, seeds, size = 100, gamma = 0.1, initial_infected = 10, max_steps = 500))]
fn sir_sweep(
    py: Python<'_>,
    betas: Vec<f64>,
    seeds: Vec<u64>,
    size: usize,
    gamma: f64,
    initial_infected: usize,
    max_steps: u64,
) -> Vec<(f64, u64, f64, f64)> {
    py.allow_threads(|| {
        let cells = run_sweep(
            &betas,
            &seeds,
            max_steps,
            |&beta, seed| {
                let cfg = SirConfig {
                    width: size,
                    height: size,
                    initial_infected,
                    beta,
                    gamma,
                };
                let mut sim = Simulation::new(sir::build(cfg, seed), seed);
                sim.add_reporter("i", |m: &SirModel| m.fraction(Status::Infected));
                sim.add_reporter("r", |m: &SirModel| m.fraction(Status::Recovered));
                sim
            },
            |&beta, sim| {
                let peak = sim
                    .data()
                    .series("i")
                    .map_or(0.0, |s| s.iter().copied().fold(0.0, f64::max));
                let r_final = sim
                    .data()
                    .series("r")
                    .and_then(|s| s.last().copied())
                    .unwrap_or(0.0);
                (beta, peak, r_final)
            },
        );
        cells
            .into_iter()
            .map(|c| (c.value.0, c.seed, c.value.1, c.value.2))
            .collect()
    })
}

/// Modelo de segregación de Schelling (ver `swarm_models::schelling`).
#[pyclass(name = "Schelling", unsendable)]
struct PySchelling {
    sim: Simulation<SchellingModel>,
}

#[pymethods]
impl PySchelling {
    /// Crea el modelo sobre una grilla `size × size` toroidal.
    #[new]
    #[pyo3(signature = (size = 50, density = 0.85, tolerance = 0.375, seed = 0))]
    fn new(size: usize, density: f64, tolerance: f64, seed: u64) -> Self {
        let cfg = SchellingConfig {
            width: size,
            height: size,
            density,
            tolerance,
        };
        let mut sim = Simulation::new(schelling::build(cfg, seed), seed);
        sim.add_reporter("conforme", SchellingModel::fraction_happy);
        sim.add_reporter("similitud", SchellingModel::mean_similarity);
        Self { sim }
    }

    /// Avanza hasta `steps` pasos (o hasta que todos estén conformes).
    fn run(&mut self, steps: u64) -> u64 {
        self.sim.run(steps)
    }

    /// Serie temporal de una métrica (`"conforme"` o `"similitud"`).
    fn series(&self, name: &str) -> Option<Vec<f64>> {
        self.sim.data().series(name).map(<[f64]>::to_vec)
    }

    /// Fracción de agentes conformes ahora.
    #[getter]
    fn fraction_happy(&self) -> f64 {
        self.sim.model.fraction_happy()
    }

    /// Índice de segregación (similitud media de vecindario) ahora.
    #[getter]
    fn mean_similarity(&self) -> f64 {
        self.sim.model.mean_similarity()
    }

    /// `True` si todos los agentes están conformes (sistema estable).
    #[getter]
    fn finished(&self) -> bool {
        self.sim.model.finished()
    }
}

/// Modelo Sugarscape (ver `swarm_models::sugarscape`).
#[pyclass(name = "Sugarscape", unsendable)]
struct PySugarscape {
    sim: Simulation<SugarscapeModel>,
}

#[pymethods]
impl PySugarscape {
    /// Crea el modelo sobre una grilla `size × size` con `n_agents` agentes.
    #[new]
    #[pyo3(signature = (size = 50, n_agents = 400, growback = 1, seed = 0))]
    fn new(size: usize, n_agents: usize, growback: u32, seed: u64) -> Self {
        let cfg = SugarscapeConfig {
            width: size,
            height: size,
            n_agents,
            growback,
        };
        let mut sim = Simulation::new(sugarscape::build(cfg, seed), seed)
            .with_schedule(Schedule::new(Activation::Random));
        sim.add_reporter("poblacion", |m: &SugarscapeModel| m.population() as f64);
        sim.add_reporter("gini", SugarscapeModel::gini);
        sim.add_reporter("riqueza_media", SugarscapeModel::mean_wealth);
        Self { sim }
    }

    /// Avanza hasta `steps` pasos (o hasta que la población se extinga).
    fn run(&mut self, steps: u64) -> u64 {
        self.sim.run(steps)
    }

    /// Serie temporal de una métrica (`"poblacion"`, `"gini"`, `"riqueza_media"`).
    fn series(&self, name: &str) -> Option<Vec<f64>> {
        self.sim.data().series(name).map(<[f64]>::to_vec)
    }

    /// Riqueza de cada agente vivo (para histogramas de la distribución).
    fn wealths(&self) -> Vec<u32> {
        self.sim.model.wealths()
    }

    /// Población viva ahora.
    #[getter]
    fn population(&self) -> usize {
        self.sim.model.population()
    }

    /// Coeficiente de Gini de la riqueza ahora.
    #[getter]
    fn gini(&self) -> f64 {
        self.sim.model.gini()
    }

    /// Riqueza media ahora.
    #[getter]
    fn mean_wealth(&self) -> f64 {
        self.sim.model.mean_wealth()
    }
}

/// Barrido de la tolerancia de Schelling, en paralelo y a velocidad Rust.
///
/// Corre `tolerances × seeds` hasta `max_steps` pasos y devuelve una fila
/// `(tolerance, seed, similitud_final, conforme_final)` por réplica.
#[pyfunction]
#[pyo3(signature = (tolerances, seeds, size = 50, density = 0.85, max_steps = 200))]
fn schelling_sweep(
    py: Python<'_>,
    tolerances: Vec<f64>,
    seeds: Vec<u64>,
    size: usize,
    density: f64,
    max_steps: u64,
) -> Vec<(f64, u64, f64, f64)> {
    py.allow_threads(|| {
        let cells = run_sweep(
            &tolerances,
            &seeds,
            max_steps,
            |&tolerance, seed| {
                let cfg = SchellingConfig {
                    width: size,
                    height: size,
                    density,
                    tolerance,
                };
                Simulation::new(schelling::build(cfg, seed), seed)
            },
            |&tolerance, sim| {
                (
                    tolerance,
                    sim.model.mean_similarity(),
                    sim.model.fraction_happy(),
                )
            },
        );
        cells
            .into_iter()
            .map(|c| (c.value.0, c.seed, c.value.1, c.value.2))
            .collect()
    })
}

/// Barrido del crecimiento (growback) de Sugarscape, en paralelo.
///
/// Corre `growbacks × seeds` hasta `max_steps` pasos y devuelve una fila
/// `(growback, seed, gini_final, poblacion_final)` por réplica.
#[pyfunction]
#[pyo3(signature = (growbacks, seeds, size = 50, n_agents = 400, max_steps = 200))]
fn sugarscape_sweep(
    py: Python<'_>,
    growbacks: Vec<u32>,
    seeds: Vec<u64>,
    size: usize,
    n_agents: usize,
    max_steps: u64,
) -> Vec<(u32, u64, f64, f64)> {
    py.allow_threads(|| {
        let cells = run_sweep(
            &growbacks,
            &seeds,
            max_steps,
            |&growback, seed| {
                let cfg = SugarscapeConfig {
                    width: size,
                    height: size,
                    n_agents,
                    growback,
                };
                Simulation::new(sugarscape::build(cfg, seed), seed)
                    .with_schedule(Schedule::new(Activation::Random))
            },
            |&growback, sim| (growback, sim.model.gini(), sim.model.population() as f64),
        );
        cells
            .into_iter()
            .map(|c| (c.value.0, c.seed, c.value.1, c.value.2))
            .collect()
    })
}

/// Módulo de extensión `swarm_abm`.
#[pymodule]
fn swarm_abm(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PySir>()?;
    m.add_class::<PySchelling>()?;
    m.add_class::<PySugarscape>()?;
    m.add_function(wrap_pyfunction!(sir_sweep, m)?)?;
    m.add_function(wrap_pyfunction!(schelling_sweep, m)?)?;
    m.add_function(wrap_pyfunction!(sugarscape_sweep, m)?)?;
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    Ok(())
}
