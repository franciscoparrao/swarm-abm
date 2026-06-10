//! Tests de la activación simultánea usando el Juego de la Vida de Conway.
//!
//! Life requiere actualización síncrona: bajo activación secuencial el
//! blinker NO oscila correctamente, así que distingue ambas semánticas.

use swarm_core::prelude::*;

struct Cell {
    pos: Pos,
    alive: bool,
    next: bool,
}

struct Life {
    agents: AgentSet<Cell>,
    grid: Grid2D<bool>,
}

impl Agent for Cell {
    type Model = Life;

    fn decide(&mut self, _id: AgentId, model: &Life, _rng: &mut SimRng) {
        let vivos = model
            .grid
            .neighbors(self.pos, Neighborhood::Moore)
            .filter(|&(_, &v)| v)
            .count();
        self.next = matches!((self.alive, vivos), (true, 2) | (true, 3) | (false, 3));
    }

    fn apply(&mut self, _id: AgentId, model: &mut Life, _rng: &mut SimRng) {
        self.alive = self.next;
        model.grid[self.pos] = self.alive;
    }
}

impl Model for Life {
    type Agent = Cell;

    fn agents(&self) -> &AgentSet<Cell> {
        &self.agents
    }

    fn agents_mut(&mut self) -> &mut AgentSet<Cell> {
        &mut self.agents
    }
}

fn life(width: usize, height: usize, vivas: &[(usize, usize)], torus: bool) -> Life {
    let mut agents = AgentSet::with_capacity(width * height);
    let mut grid = Grid2D::fill(width, height, false).with_torus(torus);
    for y in 0..height {
        for x in 0..width {
            let pos = Pos::new(x, y);
            let alive = vivas.contains(&(x, y));
            agents.insert(Cell {
                pos,
                alive,
                next: false,
            });
            grid[pos] = alive;
        }
    }
    Life { agents, grid }
}

fn estado(model: &Life) -> Vec<bool> {
    model.grid.iter().map(|(_, &v)| v).collect()
}

#[test]
fn blinker_oscila_con_periodo_2() {
    let horizontal = [(1, 2), (2, 2), (3, 2)];
    let model = life(5, 5, &horizontal, false);
    let mut sim = Simulation::new(model, 0).with_schedule(Schedule::new(Activation::Simultaneous));

    let t0 = estado(&sim.model);
    sim.step();
    let t1 = estado(&sim.model);
    sim.step();
    let t2 = estado(&sim.model);

    assert_ne!(t0, t1, "el blinker debe rotar a vertical");
    assert_eq!(
        t0, t2,
        "el blinker debe volver al estado inicial en 2 pasos"
    );

    // La fase vertical es exactamente la esperada.
    let esperado_vertical = life(5, 5, &[(2, 1), (2, 2), (2, 3)], false);
    assert_eq!(t1, estado(&esperado_vertical));
}

#[test]
fn bloque_es_estable() {
    let bloque = [(1, 1), (2, 1), (1, 2), (2, 2)];
    let model = life(4, 4, &bloque, false);
    let mut sim = Simulation::new(model, 0).with_schedule(Schedule::new(Activation::Simultaneous));
    let t0 = estado(&sim.model);
    sim.run(5);
    assert_eq!(t0, estado(&sim.model), "el bloque es naturaleza muerta");
}

#[test]
fn secuencial_rompe_el_blinker() {
    // Bajo activación secuencial (default decide+apply inmediato), las celdas
    // ven actualizaciones a medio paso: el blinker NO debe comportarse como
    // blinker. Esto confirma que Simultaneous tiene semántica distinta.
    let horizontal = [(1, 2), (2, 2), (3, 2)];
    let model = life(5, 5, &horizontal, false);
    let mut sim = Simulation::new(model, 0).with_schedule(Schedule::new(Activation::Ordered));

    let t0 = estado(&sim.model);
    sim.step();
    sim.step();
    assert_ne!(
        t0,
        estado(&sim.model),
        "secuencial no reproduce el período 2 del blinker"
    );
}

#[test]
fn simultaneous_es_determinista() {
    // Glider en torus: configuración no trivial, 30 pasos, dos corridas.
    let glider = [(1, 0), (2, 1), (0, 2), (1, 2), (2, 2)];
    let correr = || {
        let mut sim = Simulation::new(life(10, 10, &glider, true), 7)
            .with_schedule(Schedule::new(Activation::Simultaneous));
        sim.run(30);
        estado(&sim.model)
    };
    let a = correr();
    assert_eq!(a, correr());
    // En el torus el glider sobrevive: la población se mantiene en 5.
    assert_eq!(a.iter().filter(|&&v| v).count(), 5);
}
