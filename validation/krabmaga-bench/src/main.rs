// SIR espacial en krABMaga — espejo del modelo usado para swarm-abm / Mesa /
// Agents.jl. Grilla densa totalmente ocupada, vecindad Moore, susceptible con
// k vecinos infectados se contagia con prob 1-(1-beta)^k, infectado se recupera
// con prob gamma.
//
// Implementación EFICIENTE para krABMaga: usa DenseNumberGrid2D (estado escalar
// por celda, get_value sin alocación), no el object-grid (que aloca un Vec por
// consulta). Patrón idiomático: un único agente Updater que recorre la grilla,
// actualización sincrónica vía lazy_update. Da a krABMaga su mejor versión, para
// que la comparación no se pueda descartar por "usaste el field equivocado".
//
// Estado: 0 = S, 1 = I, 2 = R.
// Modo bench: cronometra SOLO el stepping (build fuera), imprime steps,ms.

use krabmaga::engine::agent::Agent;
use krabmaga::engine::fields::dense_number_grid_2d::DenseNumberGrid2D;
use krabmaga::engine::fields::field::Field;
use krabmaga::engine::location::Int2D;
use krabmaga::engine::schedule::{Schedule, ScheduleOptions};
use krabmaga::engine::state::State;
use krabmaga::rand;
use krabmaga::rand::Rng;
use std::any::Any;
use std::fmt;
use std::hash::{Hash, Hasher};

const BETA: f64 = 0.08;
const GAMMA: f64 = 0.1;

struct Sir {
    field: DenseNumberGrid2D<i32>,
    dim: (i32, i32),
    initial_infected: u32,
}

impl Sir {
    fn new(dim: (i32, i32), initial_infected: u32) -> Sir {
        Sir {
            field: DenseNumberGrid2D::new(dim.0, dim.1),
            dim,
            initial_infected,
        }
    }
    fn count(&self, want: i32) -> u32 {
        let mut n = 0;
        for x in 0..self.dim.0 {
            for y in 0..self.dim.1 {
                if self.field.get_value(&Int2D { x, y }) == Some(want) {
                    n += 1;
                }
            }
        }
        n
    }
}

impl State for Sir {
    fn update(&mut self, _step: u64) {
        self.field.lazy_update();
    }
    fn reset(&mut self) {
        self.field = DenseNumberGrid2D::new(self.dim.0, self.dim.1);
    }
    fn init(&mut self, schedule: &mut Schedule) {
        let mut rng = rand::rng();
        let mut infected = std::collections::HashSet::new();
        while (infected.len() as u32) < self.initial_infected {
            let x = rng.random_range(0..self.dim.0);
            let y = rng.random_range(0..self.dim.1);
            infected.insert((x, y));
        }
        for x in 0..self.dim.0 {
            for y in 0..self.dim.1 {
                let status = if infected.contains(&(x, y)) { 1 } else { 0 };
                self.field.set_value_location(status, &Int2D { x, y });
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
        let s = state.as_any().downcast_ref::<Sir>().unwrap();
        let (w, h) = s.dim;
        let mut rng = rand::rng();
        for x in 0..w {
            for y in 0..h {
                let cur = s.field.get_value(&Int2D { x, y }).unwrap();
                let next = if cur == 0 {
                    let mut k = 0i32;
                    for di in -1..=1 {
                        for dj in -1..=1 {
                            if di == 0 && dj == 0 {
                                continue;
                            }
                            let (nx, ny) = (x + dj, y + di);
                            if nx < 0 || ny < 0 || nx >= w || ny >= h {
                                continue;
                            }
                            if s.field.get_value(&Int2D { x: nx, y: ny }) == Some(1) {
                                k += 1;
                            }
                        }
                    }
                    if k > 0 && rng.random_bool(1.0 - (1.0 - BETA).powi(k)) {
                        1
                    } else {
                        0
                    }
                } else if cur == 1 {
                    if rng.random_bool(GAMMA) {
                        2
                    } else {
                        1
                    }
                } else {
                    2
                };
                s.field.set_value_location(next, &Int2D { x, y });
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
    let width: i32 = arg(&args, "--width", 100);
    let height: i32 = arg(&args, "--height", 100);
    let steps: u64 = arg(&args, "--steps", 150);
    let infected: u32 = arg(&args, "--infected", 5);

    let mut state = Sir::new((width, height), infected);
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
        println!(
            "krABMaga SIR {width}x{height} | {steps} pasos | S={} I={} R={}",
            state.count(0),
            state.count(1),
            state.count(2)
        );
    }
}
