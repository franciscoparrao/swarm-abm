//! Ejecución de **ensembles** y **barridos de parámetros**.
//!
//! Generaliza el patrón que un estudio típico de ABM repite a mano: correr el
//! mismo modelo con muchas semillas (réplicas) o muchas configuraciones, y
//! quedarse con un resumen por corrida. Con la feature `parallel` (activa por
//! defecto) las corridas se reparten entre hilos con rayon; sin ella, corren
//! en secuencia (útil para WASM).
//!
//! El usuario aporta dos closures:
//! - `build`: construye una [`Simulation`] a partir de la semilla (y, en el
//!   barrido, la configuración). Controla el scheduler, los reporters, etc.
//! - `outcome`: extrae de la simulación ya corrida el resultado que interesa
//!   (un escalar, las series, lo que sea `Send`).
//!
//! ```
//! use swarm_core::prelude::*;
//! use swarm_core::batch::run_ensemble;
//!
//! struct Bug;
//! struct World { agents: AgentSet<Bug>, total: u64 }
//! impl Agent for Bug {
//!     type Model = World;
//!     fn step(&mut self, _: AgentId, m: &mut World, rng: &mut SimRng) {
//!         m.total += rng.random_range(0..10);
//!     }
//! }
//! impl Model for World {
//!     type Agent = Bug;
//!     fn agents(&self) -> &AgentSet<Bug> { &self.agents }
//!     fn agents_mut(&mut self) -> &mut AgentSet<Bug> { &mut self.agents }
//! }
//!
//! // 20 réplicas, 50 pasos, devuelve el total final de cada una.
//! let totals = run_ensemble(0..20, 50, |_seed| {
//!     let mut agents = AgentSet::new();
//!     agents.insert(Bug);
//!     Simulation::new(World { agents, total: 0 }, _seed)
//! }, |sim| sim.model.total);
//! assert_eq!(totals.len(), 20);
//! ```

use crate::model::Model;
use crate::sim::Simulation;

#[cfg(feature = "parallel")]
use rayon::prelude::*;

/// Corre un **ensemble**: una réplica por cada semilla de `seeds`, cada una
/// hasta `max_steps` pasos. Devuelve el `outcome` de cada réplica, en el
/// mismo orden que `seeds`.
///
/// `build` recibe la semilla y devuelve la [`Simulation`] lista (scheduler y
/// reporters incluidos). `outcome` la inspecciona tras correrla.
pub fn run_ensemble<M, R, B, O>(
    seeds: impl IntoIterator<Item = u64>,
    max_steps: u64,
    build: B,
    outcome: O,
) -> Vec<R>
where
    M: Model,
    B: Fn(u64) -> Simulation<M> + Sync,
    O: Fn(&Simulation<M>) -> R + Sync,
    R: Send,
{
    let seeds: Vec<u64> = seeds.into_iter().collect();
    let run = |&seed: &u64| -> R {
        let mut sim = build(seed);
        sim.run(max_steps);
        outcome(&sim)
    };
    #[cfg(feature = "parallel")]
    {
        seeds.par_iter().map(run).collect()
    }
    #[cfg(not(feature = "parallel"))]
    {
        seeds.iter().map(run).collect()
    }
}

/// Resultado de una celda del barrido: índice de configuración, semilla y valor.
#[derive(Debug, Clone, Copy)]
pub struct SweepCell<R> {
    /// Índice de la configuración en el `configs` original.
    pub config: usize,
    /// Semilla con que se corrió esta réplica.
    pub seed: u64,
    /// Resultado extraído por `outcome`.
    pub value: R,
}

/// Corre un **barrido de parámetros**: cada configuración de `configs` se
/// evalúa con cada semilla de `seeds` (producto cartesiano), hasta `max_steps`
/// pasos. Devuelve un [`SweepCell`] por combinación.
///
/// Es el patrón de una calibración o un análisis de sensibilidad: `build`
/// arma la simulación a partir de `(config, seed)` y `outcome` resume el
/// resultado.
pub fn run_sweep<P, M, R, B, O>(
    configs: &[P],
    seeds: &[u64],
    max_steps: u64,
    build: B,
    outcome: O,
) -> Vec<SweepCell<R>>
where
    P: Sync,
    M: Model,
    B: Fn(&P, u64) -> Simulation<M> + Sync,
    O: Fn(&P, &Simulation<M>) -> R + Sync,
    R: Send,
{
    let tasks: Vec<(usize, u64)> = configs
        .iter()
        .enumerate()
        .flat_map(|(ci, _)| seeds.iter().map(move |&s| (ci, s)))
        .collect();
    let run = |&(ci, seed): &(usize, u64)| -> SweepCell<R> {
        let mut sim = build(&configs[ci], seed);
        sim.run(max_steps);
        SweepCell {
            config: ci,
            seed,
            value: outcome(&configs[ci], &sim),
        }
    };
    #[cfg(feature = "parallel")]
    {
        tasks.par_iter().map(run).collect()
    }
    #[cfg(not(feature = "parallel"))]
    {
        tasks.iter().map(run).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prelude::*;

    struct Bug;
    struct World {
        agents: AgentSet<Bug>,
        total: u64,
    }
    impl Agent for Bug {
        type Model = World;
        fn step(&mut self, _: AgentId, m: &mut World, rng: &mut SimRng) {
            m.total += rng.random_range(0..10);
        }
    }
    impl Model for World {
        type Agent = Bug;
        fn agents(&self) -> &AgentSet<Bug> {
            &self.agents
        }
        fn agents_mut(&mut self) -> &mut AgentSet<Bug> {
            &mut self.agents
        }
    }

    fn build(n_agents: usize, seed: u64) -> Simulation<World> {
        let mut agents = AgentSet::new();
        for _ in 0..n_agents {
            agents.insert(Bug);
        }
        Simulation::new(World { agents, total: 0 }, seed)
            .with_schedule(Schedule::new(Activation::Ordered))
    }

    #[test]
    fn ensemble_corre_todas_las_replicas() {
        let totals = run_ensemble(0..16, 30, |s| build(5, s), |sim| sim.model.total);
        assert_eq!(totals.len(), 16);
        assert!(totals.iter().all(|&t| t > 0));
    }

    #[test]
    fn ensemble_es_determinista_y_paralelo_igual_a_secuencial() {
        // El orden del resultado sigue al de las semillas, y cada celda es
        // determinista (misma semilla → mismo total), sin importar el hilo.
        let a = run_ensemble(0..32, 40, |s| build(8, s), |sim| sim.model.total);
        let b = run_ensemble(0..32, 40, |s| build(8, s), |sim| sim.model.total);
        assert_eq!(a, b);
        // Réplica directa de la semilla 7 coincide con la celda 7.
        let mut sim = build(8, 7);
        sim.run(40);
        assert_eq!(a[7], sim.model.total);
    }

    #[test]
    fn sweep_cubre_el_producto_configs_por_semillas() {
        let configs = vec![1usize, 4, 16];
        let seeds = [0u64, 1, 2, 3];
        let out = run_sweep(
            &configs,
            &seeds,
            20,
            |&n, s| build(n, s),
            |&n, sim| (n, sim.model.total),
        );
        assert_eq!(out.len(), configs.len() * seeds.len());
        // Más agentes ⇒ mayor total medio (cada agente suma cada paso).
        let mean = |c: usize| {
            let v: Vec<u64> = out
                .iter()
                .filter(|x| x.config == c)
                .map(|x| x.value.1)
                .collect();
            v.iter().sum::<u64>() as f64 / v.len() as f64
        };
        assert!(mean(0) < mean(1) && mean(1) < mean(2));
    }
}
