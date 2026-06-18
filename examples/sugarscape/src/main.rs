//! **Sugarscape** (Epstein & Axtell, *Growing Artificial Societies*, 1996):
//! el modelo canónico de la economía basada en agentes.
//!
//! Un paisaje de dos picos de azúcar; agentes idénticos en todo salvo tres
//! atributos sorteados (visión, metabolismo y dote inicial). Cada paso, regla
//! de movimiento **M**: el agente mira a lo largo de los cuatro ejes hasta su
//! visión, va a la celda libre con más azúcar (la más cercana en empate), la
//! cosecha entera, y paga su metabolismo; si su azúcar llega a cero, muere.
//! El paisaje recupera azúcar a tasa fija (regla de crecimiento **G**).
//!
//! El resultado clásico —y la razón de su fama— es **emergente**: de una
//! población casi homogénea surge una **distribución de riqueza fuertemente
//! sesgada** (coeficiente de Gini alto, cola larga de pocos ricos), sin que
//! ninguna regla la imponga. La población, además, se autorregula hacia la
//! capacidad de carga del paisaje.
//!
//! Demuestra del motor: movimiento sobre grilla, **muerte de agentes**
//! (baja diferida en `after_step`), un paisaje con estado propio (`Grid2D`
//! de celdas con capacidad/azúcar/ocupante) y activación aleatoria secuencial.
//!
//! Uso: `cargo run --release -p sugarscape [semilla] [--steps N] [--csv]`

use swarm_core::prelude::*;

/// Una celda del paisaje: su capacidad máxima de azúcar, el azúcar presente
/// ahora, y el agente que la ocupa (si hay).
#[derive(Debug, Clone, Default)]
struct Cell {
    capacity: u32,
    sugar: u32,
    occupant: Option<AgentId>,
}

/// Un agente. Visión y metabolismo son fijos de por vida; `sugar` es su
/// riqueza acumulada (sube al cosechar, baja al metabolizar).
struct Ant {
    pos: Pos,
    vision: usize,
    metabolism: u32,
    sugar: u32,
}

struct Sugarscape {
    agents: AgentSet<Ant>,
    grid: Grid2D<Cell>,
    /// Azúcar que cada celda recupera por paso (regla de crecimiento G).
    growback: u32,
    /// Bajas del paso actual, materializadas en `after_step`.
    dead: Vec<AgentId>,
}

impl Sugarscape {
    fn population(&self) -> usize {
        self.agents.len()
    }

    /// Coeficiente de Gini de la riqueza (0 = igualdad total, →1 = un solo
    /// agente concentra todo). Es la firma del modelo.
    fn gini(&self) -> f64 {
        let mut w: Vec<u64> = self
            .agents
            .iter()
            .map(|(_, a)| u64::from(a.sugar))
            .collect();
        let n = w.len();
        if n == 0 {
            return 0.0;
        }
        w.sort_unstable();
        let sum: u64 = w.iter().sum();
        if sum == 0 {
            return 0.0;
        }
        // G = Σ_i (2(i+1) − n − 1)·x_i / (n · Σx),  x ordenado ascendente.
        let mut acc: i128 = 0;
        for (i, &x) in w.iter().enumerate() {
            let coef = 2 * (i as i128 + 1) - n as i128 - 1;
            acc += coef * x as i128;
        }
        acc as f64 / (n as f64 * sum as f64)
    }

    fn mean_wealth(&self) -> f64 {
        let n = self.agents.len();
        if n == 0 {
            return 0.0;
        }
        let sum: u64 = self.agents.iter().map(|(_, a)| u64::from(a.sugar)).sum();
        sum as f64 / n as f64
    }
}

impl Agent for Ant {
    type Model = Sugarscape;

    fn step(&mut self, id: AgentId, model: &mut Sugarscape, rng: &mut SimRng) {
        let (w, h) = (model.grid.width() as i64, model.grid.height() as i64);

        // Regla M: mejor celda libre (más azúcar; en empate, la más cercana).
        // Se parte de la celda propia (azúcar actual, distancia 0): quedarse
        // es siempre una opción válida.
        let mut best_pos = self.pos;
        let mut best_sugar = model.grid[self.pos].sugar;
        let mut best_dist = 0usize;

        // Orden de ejes barajado por paso: rompe el sesgo direccional en los
        // empates exactos (mismo azúcar y misma distancia) sin perder
        // determinismo (el RNG está sembrado).
        let mut axes: [(i64, i64); 4] = [(0, -1), (0, 1), (-1, 0), (1, 0)];
        axes.shuffle(rng);

        for (dx, dy) in axes {
            for d in 1..=self.vision as i64 {
                let (nx, ny) = (self.pos.x as i64 + dx * d, self.pos.y as i64 + dy * d);
                if nx < 0 || ny < 0 || nx >= w || ny >= h {
                    break; // fuera de la grilla: nada más que ver en este eje
                }
                let cand = Pos::new(nx as usize, ny as usize);
                let cell = &model.grid[cand];
                if cell.occupant.is_some() {
                    continue; // ve más allá de otros agentes, pero no puede ir ahí
                }
                let dist = d as usize;
                if cell.sugar > best_sugar || (cell.sugar == best_sugar && dist < best_dist) {
                    best_pos = cand;
                    best_sugar = cell.sugar;
                    best_dist = dist;
                }
            }
        }

        // Moverse (si cambió de celda) y cosechar el azúcar del destino.
        if best_pos != self.pos {
            model.grid[self.pos].occupant = None;
            model.grid[best_pos].occupant = Some(id);
            self.pos = best_pos;
        }
        self.sugar += model.grid[self.pos].sugar;
        model.grid[self.pos].sugar = 0;

        // Metabolizar: si la riqueza llega a cero (o menos), el agente muere.
        // La baja efectiva ocurre en `after_step`; aquí solo libera su celda.
        if self.sugar <= self.metabolism {
            model.grid[self.pos].occupant = None;
            model.dead.push(id);
        } else {
            self.sugar -= self.metabolism;
        }
    }
}

impl Model for Sugarscape {
    type Agent = Ant;

    fn agents(&self) -> &AgentSet<Ant> {
        &self.agents
    }

    fn agents_mut(&mut self) -> &mut AgentSet<Ant> {
        &mut self.agents
    }

    fn after_step(&mut self, _rng: &mut SimRng) {
        // Bajas diferidas (un agente no puede eliminarse en su propio `step`).
        for id in self.dead.drain(..) {
            self.agents.remove(id);
        }
        // Crecimiento del paisaje: cada celda recupera `growback`, sin pasar
        // su capacidad.
        let growback = self.growback;
        for (_, cell) in self.grid.iter_mut() {
            cell.sugar = (cell.sugar + growback).min(cell.capacity);
        }
    }

    fn finished(&self) -> bool {
        self.agents.is_empty()
    }
}

/// Capacidad de azúcar de una celda: dos picos en diagonal, con mesetas
/// concéntricas de nivel 4 → 0 (el paisaje canónico de dos montañas).
fn capacity_at(p: Pos, w: usize, h: usize) -> u32 {
    let peaks = [
        (w as f64 * 0.75, h as f64 * 0.25),
        (w as f64 * 0.25, h as f64 * 0.75),
    ];
    let maxr = w.min(h) as f64 * 0.55;
    let bands = [0.18, 0.37, 0.62, 1.0]; // fracciones de maxr → niveles 4,3,2,1
    let mut cap = 0u32;
    for (px, py) in peaks {
        let d = ((p.x as f64 - px).powi(2) + (p.y as f64 - py).powi(2)).sqrt();
        let level = bands
            .iter()
            .position(|&b| d < maxr * b)
            .map_or(0, |i| 4 - i as u32);
        cap = cap.max(level);
    }
    cap
}

fn build(width: usize, height: usize, n_agents: usize, growback: u32, seed: u64) -> Sugarscape {
    let mut rng = rng_from_seed(seed ^ 0x5_06A8_5CA9_E000);
    let mut grid = Grid2D::from_fn(width, height, |p| {
        let cap = capacity_at(p, width, height);
        Cell {
            capacity: cap,
            sugar: cap, // el paisaje arranca lleno
            occupant: None,
        }
    });

    // Celdas al azar para sembrar agentes (sin solapamiento).
    let mut coords: Vec<Pos> = grid.iter().map(|(p, _)| p).collect();
    coords.shuffle(&mut rng);

    let mut agents = AgentSet::with_capacity(n_agents);
    for &pos in coords.iter().take(n_agents) {
        // Rangos canónicos del libro: visión 1–6, metabolismo 1–4, dote 5–25.
        let vision = rng.random_range(1..=6);
        let metabolism = rng.random_range(1..=4);
        let sugar = rng.random_range(5..=25);
        let id = agents.insert(Ant {
            pos,
            vision,
            metabolism,
            sugar,
        });
        grid[pos].occupant = Some(id);
    }

    Sugarscape {
        agents,
        grid,
        growback,
        dead: Vec::new(),
    }
}

/// Valor de un flag `--nombre valor`, si está presente.
fn arg_value<T: std::str::FromStr>(args: &[String], name: &str) -> Option<T> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .and_then(|v| v.parse().ok())
}

/// Histograma ASCII de la distribución de riqueza (la firma sesgada).
fn print_wealth_histogram(model: &Sugarscape) {
    let mut wealth: Vec<u32> = model.agents.iter().map(|(_, a)| a.sugar).collect();
    if wealth.is_empty() {
        return;
    }
    wealth.sort_unstable();
    let max = *wealth.last().unwrap_or(&1).max(&1);
    let bins = 10usize;
    let mut counts = vec![0usize; bins];
    for &x in &wealth {
        let b = ((x as usize * bins) / (max as usize + 1)).min(bins - 1);
        counts[b] += 1;
    }
    let peak = counts.iter().copied().max().unwrap_or(1).max(1);
    println!("\nDistribución de riqueza (cola larga = pocos ricos):");
    for (i, &c) in counts.iter().enumerate() {
        let lo = i as u32 * max / bins as u32;
        let hi = (i as u32 + 1) * max / bins as u32;
        let bar = "█".repeat((c * 40 / peak).max(usize::from(c > 0)));
        println!("  [{lo:>3}–{hi:>3}] {bar} {c}");
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let seed: u64 = args
        .first()
        .filter(|a| !a.starts_with("--"))
        .and_then(|s| s.parse().ok())
        .unwrap_or(42);
    let csv = args.iter().any(|a| a == "--csv");
    let max_steps: u64 = arg_value(&args, "--steps").unwrap_or(200);

    let (width, height, n_agents, growback) = (50, 50, 400, 1);
    let model = build(width, height, n_agents, growback, seed);
    let pop0 = model.population();

    let mut sim = Simulation::new(model, seed).with_schedule(Schedule::new(Activation::Random));
    sim.add_reporter("poblacion", |m: &Sugarscape| m.population() as f64);
    sim.add_reporter("gini", Sugarscape::gini);
    sim.add_reporter("riqueza_media", Sugarscape::mean_wealth);

    sim.run(max_steps);

    if csv {
        print!("{}", sim.data().to_csv());
        return;
    }

    println!(
        "Sugarscape {width}x{height} | {pop0} agentes iniciales | growback {growback} | semilla {seed}"
    );
    let pop = sim.data().series("poblacion").unwrap_or(&[]);
    let gini = sim.data().series("gini").unwrap_or(&[]);
    let riqueza = sim.data().series("riqueza_media").unwrap_or(&[]);
    println!("\n  paso  poblacion  gini  riqueza_media");
    for (i, &step) in sim.data().steps().iter().enumerate() {
        if step % 10 == 0 || i + 1 == pop.len() {
            println!(
                "  {step:>4}  {:>9}  {:>4.3}  {:>13.2}",
                pop[i] as u64, gini[i], riqueza[i]
            );
        }
    }
    println!(
        "\nPoblación {pop0} → {} (autorregulada a la capacidad de carga)",
        sim.model.population()
    );
    println!(
        "Gini de riqueza: {:.3} (desigualdad emergente de agentes casi homogéneos)",
        sim.model.gini()
    );
    print_wealth_histogram(&sim.model);
}
