// Schelling en krABMaga — espejo del modelo usado para swarm-abm / Mesa /
// Agents.jl. Grilla torus, densidad 0.85, dos grupos 50/50, vecindad Moore,
// conforme si la fracción de vecinos ocupados del mismo grupo >= 0.375 (1.0 si
// aislado), el inconforme se muda a una celda vacía al azar.
//
// Implementación EFICIENTE para krABMaga: DenseNumberGrid2D (0 = vacía, 1 = grupo
// A, 2 = grupo B; get_value/set_value sin alocación), un único Updater que barre
// la grilla, actualización sincrónica vía lazy_update. Da a krABMaga su mejor
// versión (number-grid, no object-grid), igual que el espejo SIR.
//
// Modo bench: cronometra SOLO el stepping (100 pasos fijos), imprime steps,ms.

use krabmaga::engine::agent::Agent;
use krabmaga::engine::fields::dense_number_grid_2d::DenseNumberGrid2D;
use krabmaga::engine::fields::field::Field;
use krabmaga::engine::location::Int2D;
use krabmaga::engine::schedule::{Schedule, ScheduleOptions};
use krabmaga::engine::state::State;
use krabmaga::rand;
use krabmaga::rand::seq::SliceRandom;
use std::any::Any;
use std::fmt;
use std::hash::{Hash, Hasher};

const DENSITY: f64 = 0.85;
const TOLERANCE: f64 = 0.375;

struct Schelling {
    field: DenseNumberGrid2D<i32>,
    dim: (i32, i32),
}

impl Schelling {
    fn new(dim: (i32, i32)) -> Schelling {
        Schelling {
            field: DenseNumberGrid2D::new(dim.0, dim.1),
            dim,
        }
    }
}

impl State for Schelling {
    fn update(&mut self, _step: u64) {
        self.field.lazy_update();
    }
    fn reset(&mut self) {
        self.field = DenseNumberGrid2D::new(self.dim.0, self.dim.1);
    }
    fn init(&mut self, schedule: &mut Schedule) {
        let mut rng = rand::rng();
        let (w, h) = self.dim;
        let mut coords: Vec<(i32, i32)> = (0..w).flat_map(|x| (0..h).map(move |y| (x, y))).collect();
        coords.shuffle(&mut rng);
        let n_agents = ((w * h) as f64 * DENSITY).round() as usize;
        // Estado inicial de cada celda (write buffer; commit en el paso 0).
        let mut val = vec![0i32; (w * h) as usize];
        for (i, &(x, y)) in coords.iter().take(n_agents).enumerate() {
            val[(y * w + x) as usize] = if i % 2 == 0 { 1 } else { 2 };
        }
        for x in 0..w {
            for y in 0..h {
                self.field
                    .set_value_location(val[(y * w + x) as usize], &Int2D { x, y });
            }
        }
        schedule.schedule_repeating(Box::new(Updater {}), 0., 0);
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
    fn as_state_mut(&mut self) -> &mut dyn State {
        self
    }
    fn as_state(&self) -> &dyn State {
        self
    }
    fn before_step(&mut self, _schedule: &mut Schedule) {}
    fn after_step(&mut self, _schedule: &mut Schedule) {}
    fn end_condition(&mut self, _schedule: &mut Schedule) -> bool {
        false
    }
}

#[derive(Clone, Copy)]
struct Updater {}

impl Agent for Updater {
    fn step(&mut self, state: &mut dyn State) {
        let s = state.as_any().downcast_ref::<Schelling>().unwrap();
        let (w, h) = s.dim;
        let get = |x: i32, y: i32| s.field.get_value(&Int2D { x, y }).unwrap_or(0);

        // Pase 1 (lectura, estado actual): empties y agentes inconformes.
        let mut empties: Vec<(i32, i32)> = Vec::new();
        let mut movers: Vec<(i32, i32, i32)> = Vec::new();
        for x in 0..w {
            for y in 0..h {
                let g = get(x, y);
                if g == 0 {
                    empties.push((x, y));
                    continue;
                }
                let (mut same, mut occ) = (0i32, 0i32);
                for di in -1..=1 {
                    for dj in -1..=1 {
                        if di == 0 && dj == 0 {
                            continue;
                        }
                        // Torus: envolver coordenadas.
                        let nx = (x + dj).rem_euclid(w);
                        let ny = (y + di).rem_euclid(h);
                        let ng = get(nx, ny);
                        if ng != 0 {
                            occ += 1;
                            if ng == g {
                                same += 1;
                            }
                        }
                    }
                }
                let sim = if occ == 0 {
                    1.0
                } else {
                    same as f64 / occ as f64
                };
                if sim < TOLERANCE {
                    movers.push((x, y, g));
                }
            }
        }

        // Pase 2 (escritura): copiar estado actual al write buffer, luego mover.
        for x in 0..w {
            for y in 0..h {
                s.field.set_value_location(get(x, y), &Int2D { x, y });
            }
        }
        let mut rng = rand::rng();
        empties.shuffle(&mut rng);
        for (i, &(ox, oy, g)) in movers.iter().enumerate() {
            if i < empties.len() {
                let (ex, ey) = empties[i];
                s.field.set_value_location(0, &Int2D { x: ox, y: oy });
                s.field.set_value_location(g, &Int2D { x: ex, y: ey });
            }
        }
    }
    fn before_step(
        &mut self,
        _state: &mut dyn State,
    ) -> Option<Vec<(Box<dyn Agent>, ScheduleOptions)>> {
        None
    }
    fn after_step(
        &mut self,
        _state: &mut dyn State,
    ) -> Option<Vec<(Box<dyn Agent>, ScheduleOptions)>> {
        None
    }
}
impl Hash for Updater {
    fn hash<H: Hasher>(&self, _h: &mut H) {}
}
impl Eq for Updater {}
impl PartialEq for Updater {
    fn eq(&self, _o: &Updater) -> bool {
        true
    }
}
impl fmt::Display for Updater {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "updater")
    }
}

fn arg<T: std::str::FromStr>(args: &[String], name: &str, default: T) -> T {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let width: i32 = arg(&args, "--width", 50);
    let height: i32 = arg(&args, "--height", 50);
    let steps: u64 = arg(&args, "--steps", 100);

    let mut state = Schelling::new((width, height));
    let mut schedule = Schedule::new();
    state.init(&mut schedule);

    if args.iter().any(|a| a == "--bench") {
        let t0 = std::time::Instant::now();
        for _ in 0..steps {
            schedule.step(state.as_state_mut());
        }
        let ms = t0.elapsed().as_secs_f64() * 1000.0;
        println!("steps,ms\n{steps},{ms:.3}");
    } else {
        for _ in 0..steps {
            schedule.step(state.as_state_mut());
        }
        let mut occ = 0;
        for x in 0..width {
            for y in 0..height {
                if state.field.get_value(&Int2D { x, y }).unwrap_or(0) != 0 {
                    occ += 1;
                }
            }
        }
        println!("krABMaga Schelling {width}x{height} | {steps} pasos | ocupadas={occ}");
    }
}
