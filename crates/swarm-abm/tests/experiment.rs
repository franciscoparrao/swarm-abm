#![cfg(feature = "experiment")]
//! Validación de `swarm_abm::experiment` (P3-4 de `docs/AUDIT.md`) contra
//! la **función de Ishigami**, el benchmark estándar de análisis de
//! sensibilidad: tiene índices de Sobol' S1/ST con forma cerrada conocida
//! (Saltelli et al. 2008, *Global Sensitivity Analysis: The Primer*), así
//! que es la única forma real de verificar que las fórmulas de
//! Saltelli/Jansen están bien implementadas — no solo que el código
//! "corre", sino que da el número correcto. Sin esto, un error de signo o
//! de índice en la fórmula produciría resultados plausibles pero
//! silenciosamente incorrectos.
//!
//! f(x1,x2,x3) = sin(x1) + a·sin²(x2) + b·x3⁴·sin(x1), xᵢ ~ Uniform(-π,π)
//! con a=7, b=0.1 (los valores canónicos del benchmark):
//!   S1 ≈ [0.3139, 0.4424, 0.0]      (x3 no tiene efecto de primer orden)
//!   ST ≈ [0.5574, 0.4424, 0.2436]   (pero sí tiene efecto total vía
//!                                     interacción con x1 — la propiedad
//!                                     que hace de Ishigami un caso
//!                                     interesante: S1[3]=0 pero ST[3]≠0)

use std::f64::consts::PI;
use swarm_abm::experiment::{ParamSpec, latin_hypercube, morris, sobol};
use swarm_abm::prelude::*;

struct NoOp;
impl Agent for NoOp {
    type Model = Ishigami;
}

struct Ishigami {
    value: f64,
    agents: AgentSet<NoOp>,
}

impl Model for Ishigami {
    type Agent = NoOp;
    fn agents(&self) -> &AgentSet<NoOp> {
        &self.agents
    }
    fn agents_mut(&mut self) -> &mut AgentSet<NoOp> {
        &mut self.agents
    }
}

fn ishigami_value(x: &[f64]) -> f64 {
    let (x1, x2, x3) = (x[0], x[1], x[2]);
    let (a, b) = (7.0, 0.1);
    x1.sin() + a * x2.sin().powi(2) + b * x3.powi(4) * x1.sin()
}

fn build(point: &[f64], _seed: u64) -> Simulation<Ishigami> {
    Simulation::new(
        Ishigami {
            value: ishigami_value(point),
            agents: AgentSet::new(),
        },
        0,
    )
}

fn outcome(sim: &Simulation<Ishigami>) -> f64 {
    sim.model.value
}

fn ishigami_specs() -> Vec<ParamSpec> {
    vec![
        ParamSpec::new("x1", -PI, PI),
        ParamSpec::new("x2", -PI, PI),
        ParamSpec::new("x3", -PI, PI),
    ]
}

#[test]
fn sobol_indices_coinciden_con_ishigami_analitico() {
    let specs = ishigami_specs();
    let design = sobol(&specs, 4096);
    let result = design.run(1, 0, 200, build, outcome);

    // Referencia (Saltelli et al. 2008, a=7, b=0.1).
    let s1_ref = [0.3139, 0.4424, 0.0];
    let st_ref = [0.5574, 0.4424, 0.2436];
    let tol = 0.05;

    for i in 0..3 {
        assert!(
            (result.s1[i] - s1_ref[i]).abs() < tol,
            "S1[{}] = {:.4}, esperado ≈ {:.4} (nombre {})",
            i,
            result.s1[i],
            s1_ref[i],
            result.names[i]
        );
        assert!(
            (result.st[i] - st_ref[i]).abs() < tol,
            "ST[{}] = {:.4}, esperado ≈ {:.4} (nombre {})",
            i,
            result.st[i],
            st_ref[i],
            result.names[i]
        );
    }

    // La firma cualitativa de Ishigami: x3 no tiene efecto de primer orden
    // pero sí un efecto total sustancial (interacción con x1). Es la
    // propiedad que hace de Ishigami un test no trivial — confirma que ST
    // no es simplemente una copia de S1.
    assert!(result.s1[2] < 0.05, "S1[x3] debe ser ≈0");
    assert!(
        result.st[2] > 0.15,
        "ST[x3] debe ser sustancial (interacción)"
    );

    // Los intervalos de confianza del bootstrap deben contener la
    // estimación puntual (son intervalos DE esa estimación).
    for i in 0..3 {
        let (lo, hi) = result.s1_conf[i];
        assert!(lo <= result.s1[i] + 1e-9 && result.s1[i] - 1e-9 <= hi);
        let (lo, hi) = result.st_conf[i];
        assert!(lo <= result.st[i] + 1e-9 && result.st[i] - 1e-9 <= hi);
    }
}

#[test]
fn sobol_design_es_determinista() {
    let specs = ishigami_specs();
    let correr = || sobol(&specs, 256).run(7, 0, 50, build, outcome).s1;
    assert_eq!(correr(), correr());
}

#[test]
fn latin_hypercube_estratifica_y_respeta_rango() {
    let specs = vec![
        ParamSpec::new("x", 0.0, 10.0),
        ParamSpec::new("y", -5.0, 5.0),
    ];
    let n = 20;
    let puntos = latin_hypercube(&specs, n, 3);
    assert_eq!(puntos.len(), n);

    for dim in 0..2 {
        let (lo, hi) = (specs[dim].low, specs[dim].high);
        let mut valores: Vec<f64> = puntos.iter().map(|p| p[dim]).collect();
        assert!(valores.iter().all(|&v| v >= lo && v <= hi));
        // Estratificación: un valor por cada uno de los n intervalos.
        valores.sort_by(f64::total_cmp);
        for (i, &v) in valores.iter().enumerate() {
            let ancho = (hi - lo) / n as f64;
            let (a, b) = (lo + i as f64 * ancho, lo + (i as f64 + 1.0) * ancho);
            assert!(
                v >= a - 1e-9 && v <= b + 1e-9,
                "punto {i} fuera de su intervalo: {v}"
            );
        }
    }

    assert_eq!(
        latin_hypercube(&specs, n, 3),
        puntos,
        "determinista dada la semilla"
    );
}

#[test]
fn morris_produce_estadisticas_por_parametro_y_es_determinista() {
    let specs = ishigami_specs();
    let disenio = morris(&specs, 100, 4, 11);
    let resultado = disenio.run(5, 0, build, outcome);

    assert_eq!(resultado.len(), 3);
    for r in &resultado {
        assert!(r.mu_star >= 0.0);
        assert!(r.sigma >= 0.0);
        assert!(r.mu.is_finite() && r.mu_star.is_finite() && r.sigma.is_finite());
    }
    // x2 tiene el efecto más consistente/aislado (sin interacciones en
    // Ishigami): su mu_star no debería ser cero.
    let x2 = resultado
        .iter()
        .find(|r| r.name == "x2")
        .expect("x2 presente");
    assert!(x2.mu_star > 0.1);

    let repetir = morris(&specs, 100, 4, 11).run(5, 0, build, outcome);
    let mu_stars: Vec<f64> = resultado.iter().map(|r| r.mu_star).collect();
    let mu_stars2: Vec<f64> = repetir.iter().map(|r| r.mu_star).collect();
    assert_eq!(mu_stars, mu_stars2, "determinista dada la semilla");
}
