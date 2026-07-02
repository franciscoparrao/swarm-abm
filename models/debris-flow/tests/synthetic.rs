//! Tests del modelo de flujos de detritos sobre terreno sintético.

use debris_flow::{DebrisAgent, DebrisFlowModel, Layers, Params, Physics};
use swarm_abm::prelude::*;

/// Plano inclinado que baja hacia +y, condiciones óptimas para generar flujo.
fn layers_plano(width: usize, height: usize, slope_frac: f32) -> Layers {
    let dem = Grid2D::from_fn(width, height, |p| (height - p.y) as f32 * 30.0 * slope_frac);
    Layers {
        dem,
        slope: Grid2D::fill(width, height, slope_frac),
        rain: vec![
            Grid2D::fill(width, height, 10.0),
            Grid2D::fill(width, height, 10.0),
            Grid2D::fill(width, height, 10.0),
        ],
        isotherm: Grid2D::fill(width, height, 1.0),
        sediment: Grid2D::fill(width, height, 1.0),
        susceptibility: Grid2D::fill(width, height, 1.0),
        streams: Grid2D::fill(width, height, 0.0),
    }
}

fn sim(layers: Layers, params: Params, seed: u64) -> Simulation<DebrisFlowModel> {
    let model = DebrisFlowModel::new(std::sync::Arc::new(layers), params, 30.0, seed);
    Simulation::new(model, seed).with_schedule(Schedule::new(Activation::Ordered))
}

fn footprint_cells(m: &DebrisFlowModel) -> Vec<Pos> {
    m.footprint
        .iter()
        .filter(|&(_, &v)| v)
        .map(|(p, _)| p)
        .collect()
}

#[test]
fn genera_flujos_y_descienden() {
    let params = Params {
        n_rain_agents: 5,
        stochastic_temperature: 0.0,
        ..Params::default()
    };
    let mut s = sim(layers_plano(60, 200, 0.3), params, 1);
    s.run(300);

    assert!(s.model.flows_created > 0, "la lluvia debe generar flujos");
    let cells = footprint_cells(&s.model);
    assert!(!cells.is_empty());

    // En un plano que baja hacia +y, el footprint debe extenderse hacia
    // abajo: su y máximo supera con claridad al de cualquier origen posible.
    let max_y = cells.iter().map(|p| p.y).max().expect("hay celdas");
    assert!(
        max_y >= 195,
        "los flujos deben llegar cerca del borde inferior (max_y={max_y})"
    );

    // Tras el evento, todos los flujos murieron (bajas diferidas funcionan).
    assert_eq!(s.model.active_flows(), 0);
    assert!(s.model.finished());

    // Las gotas de lluvia siguen vivas (solo mueren los flujos).
    let raindrops = s
        .model
        .agents
        .iter()
        .filter(|(_, a)| matches!(a, DebrisAgent::Raindrop(_)))
        .count();
    assert_eq!(raindrops, 5);
}

#[test]
fn determinismo_con_temperatura() {
    let params = Params {
        n_rain_agents: 10,
        stochastic_temperature: 0.5,
        ..Params::default()
    };
    let correr = || {
        let mut s = sim(layers_plano(80, 120, 0.2), params.clone(), 7);
        s.run(300);
        footprint_cells(&s.model)
    };
    assert_eq!(correr(), correr(), "misma semilla, mismo footprint");
}

#[test]
fn temperatura_dispersa_el_flujo() {
    let base = Params {
        n_rain_agents: 10,
        ..Params::default()
    };
    let correr = |t: f64| {
        let params = Params {
            stochastic_temperature: t,
            ..base.clone()
        };
        let mut s = sim(layers_plano(120, 120, 0.2), params, 3);
        s.run(300);
        footprint_cells(&s.model).len()
    };
    let area_t0 = correr(0.0);
    let area_t2 = correr(2.0);
    assert!(
        area_t2 > area_t0,
        "mayor temperatura debe dispersar más (T0={area_t0}, T2={area_t2})"
    );
}

#[test]
fn spread_costero_divide_el_flujo() {
    // Pendiente bajo el umbral costero: el primer flujo con volumen alto
    // debe dividirse en int(2.92) = 2 hijos.
    let mut layers = layers_plano(60, 60, 0.3);
    layers.slope = Grid2D::fill(60, 60, 0.01); // < coastal_slope_threshold
    let params = Params {
        n_rain_agents: 1,
        stochastic_temperature: 0.0,
        ..Params::default()
    };
    let mut s = sim(layers, params, 11);
    // La primera lluvia llega en la hora 6 (día 1); el flujo nace con
    // volumen 10*0.7*2 = 14 > coastal_volume_threshold.
    s.run(8);
    assert!(
        s.model.flows_created >= 3,
        "debe haber padre + 2 hijos (flows_created={})",
        s.model.flows_created
    );
}

#[test]
fn sin_lluvia_no_hay_flujos() {
    let mut layers = layers_plano(40, 40, 0.3);
    layers.rain = vec![
        Grid2D::fill(40, 40, 0.0),
        Grid2D::fill(40, 40, 0.0),
        Grid2D::fill(40, 40, 0.0),
    ];
    let mut s = sim(layers, Params::default(), 5);
    s.run(300);
    assert_eq!(s.model.flows_created, 0);
    assert!(footprint_cells(&s.model).is_empty());
}

#[test]
fn fisica_coastal_radio_dinamico_y_descenso() {
    // La variante Coastal (selección por menor elevación, radio de footprint
    // dinámico, velocidad con drag) genera flujos que descienden el plano.
    let params = Params {
        physics: Physics::Coastal,
        n_rain_agents: 5,
        ..Params::default()
    };
    let mut s = sim(layers_plano(60, 200, 0.3), params, 1);
    s.run(300);
    assert!(s.model.flows_created > 0, "Coastal debe generar flujos");
    let cells = footprint_cells(&s.model);
    let max_y = cells.iter().map(|p| p.y).max().expect("hay celdas");
    assert!(
        max_y >= 150,
        "los flujos Coastal deben descender (max_y={max_y})"
    );
    // Determinismo de la variante.
    let correr = || {
        let p = Params {
            physics: Physics::Coastal,
            n_rain_agents: 5,
            ..Params::default()
        };
        let mut s = sim(layers_plano(60, 200, 0.3), p, 1);
        s.run(300);
        footprint_cells(&s.model)
    };
    assert_eq!(correr(), correr());
}
