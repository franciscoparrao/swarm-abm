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
use swarm_models::sir::{self, Sir as SirModel, SirConfig, Status};

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

/// Módulo de extensión `swarm_abm`.
#[pymodule]
fn swarm_abm(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PySir>()?;
    m.add_function(wrap_pyfunction!(sir_sweep, m)?)?;
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    Ok(())
}
