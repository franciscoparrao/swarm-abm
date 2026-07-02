//! Visor WASM de swarm-abm.
//!
//! Expone tres modelos del motor a JavaScript vía `wasm-bindgen`. El bucle de
//! simulación corre en WebAssembly (camino secuencial, sin rayon: el target
//! `wasm32-unknown-unknown` no tiene hilos); cada modelo entrega `pixels()`, un
//! buffer RGBA listo para `putImageData` sobre un `<canvas>`. La página
//! `www/index.html` lo maneja: configura, anima y dibuja.
//!
//! Construir: `wasm-pack build --target web --out-dir www/pkg`.

use wasm_bindgen::prelude::*;

use swarm_abm::prelude::*;
use swarm_models::{schelling, sir, sugarscape};

/// Convierte categorías por celda en un buffer RGBA (4 bytes por celda),
/// usando una paleta indexada por categoría.
fn to_rgba(cells: &[u8], palette: &[[u8; 3]]) -> Vec<u8> {
    let mut out = Vec::with_capacity(cells.len() * 4);
    for &c in cells {
        let [r, g, b] = palette[c as usize];
        out.extend_from_slice(&[r, g, b, 255]);
    }
    out
}

/// Visor del modelo de segregación de Schelling.
#[wasm_bindgen]
pub struct Schelling {
    sim: Simulation<schelling::Schelling>,
}

#[wasm_bindgen]
impl Schelling {
    /// Crea el modelo sobre una grilla `size × size` toroidal.
    #[wasm_bindgen(constructor)]
    #[must_use]
    pub fn new(size: usize, density: f64, tolerance: f64, seed: u32) -> Schelling {
        let cfg = schelling::SchellingConfig {
            width: size,
            height: size,
            density,
            tolerance,
        };
        Schelling {
            sim: Simulation::new(schelling::build(cfg, u64::from(seed)), u64::from(seed)),
        }
    }

    /// Avanza `n` pasos (se detiene antes si todos están conformes).
    pub fn step(&mut self, n: u32) {
        self.sim.run(u64::from(n));
    }

    /// Buffer RGBA por celda: gris (vacía), rojo (grupo A), azul (grupo B).
    #[must_use]
    pub fn pixels(&self) -> Vec<u8> {
        const P: [[u8; 3]; 3] = [[235, 235, 235], [219, 68, 68], [66, 108, 219]];
        to_rgba(&self.sim.model.cells(), &P)
    }

    /// Ancho de la grilla (celdas).
    #[wasm_bindgen(getter)]
    #[must_use]
    pub fn width(&self) -> usize {
        self.sim.model.width()
    }

    /// Alto de la grilla (celdas).
    #[wasm_bindgen(getter)]
    #[must_use]
    pub fn height(&self) -> usize {
        self.sim.model.height()
    }

    /// Fracción de agentes conformes (felices).
    #[wasm_bindgen(getter)]
    #[must_use]
    pub fn happy(&self) -> f64 {
        self.sim.model.fraction_happy()
    }

    /// Índice de segregación (similitud media de vecindario) — métrica sensible
    /// a la configuración exacta.
    #[wasm_bindgen(getter)]
    #[must_use]
    pub fn mean_similarity(&self) -> f64 {
        self.sim.model.mean_similarity()
    }

    /// `true` si todos los agentes están conformes (sistema estable).
    #[wasm_bindgen(getter)]
    #[must_use]
    pub fn finished(&self) -> bool {
        self.sim.model.finished()
    }
}

/// Visor del modelo SIR espacial.
#[wasm_bindgen]
pub struct Sir {
    sim: Simulation<sir::Sir>,
}

#[wasm_bindgen]
impl Sir {
    /// Crea el modelo sobre una grilla `size × size` toroidal.
    #[wasm_bindgen(constructor)]
    #[must_use]
    pub fn new(size: usize, beta: f64, gamma: f64, initial_infected: usize, seed: u32) -> Sir {
        let cfg = sir::SirConfig {
            width: size,
            height: size,
            initial_infected,
            beta,
            gamma,
        };
        Sir {
            sim: Simulation::new(sir::build(cfg, u64::from(seed)), u64::from(seed)),
        }
    }

    /// Avanza `n` pasos (se detiene antes si no quedan infectados).
    pub fn step(&mut self, n: u32) {
        self.sim.run(u64::from(n));
    }

    /// Buffer RGBA por celda: verde (susceptible), rojo (infectado), gris
    /// (recuperado).
    #[must_use]
    pub fn pixels(&self) -> Vec<u8> {
        const P: [[u8; 3]; 3] = [[88, 168, 88], [219, 60, 60], [142, 142, 150]];
        to_rgba(&self.sim.model.cells(), &P)
    }

    /// Ancho de la grilla (celdas).
    #[wasm_bindgen(getter)]
    #[must_use]
    pub fn width(&self) -> usize {
        self.sim.model.width()
    }

    /// Alto de la grilla (celdas).
    #[wasm_bindgen(getter)]
    #[must_use]
    pub fn height(&self) -> usize {
        self.sim.model.height()
    }

    /// Fracción de infectados ahora.
    #[wasm_bindgen(getter)]
    #[must_use]
    pub fn infected(&self) -> f64 {
        self.sim.model.fraction(sir::Status::Infected)
    }

    /// Fracción de recuperados ahora (tamaño de la epidemia).
    #[wasm_bindgen(getter)]
    #[must_use]
    pub fn recovered(&self) -> f64 {
        self.sim.model.fraction(sir::Status::Recovered)
    }

    /// `true` si ya no quedan infectados.
    #[wasm_bindgen(getter)]
    #[must_use]
    pub fn finished(&self) -> bool {
        self.sim.model.finished()
    }
}

/// Visor del modelo Sugarscape.
#[wasm_bindgen]
pub struct Sugarscape {
    sim: Simulation<sugarscape::Sugarscape>,
}

#[wasm_bindgen]
impl Sugarscape {
    /// Crea el modelo sobre una grilla `size × size` con `n_agents` agentes.
    #[wasm_bindgen(constructor)]
    #[must_use]
    pub fn new(size: usize, n_agents: usize, growback: u32, seed: u32) -> Sugarscape {
        let cfg = sugarscape::SugarscapeConfig {
            width: size,
            height: size,
            n_agents,
            growback,
        };
        Sugarscape {
            sim: Simulation::new(sugarscape::build(cfg, u64::from(seed)), u64::from(seed))
                .with_schedule(Schedule::new(Activation::Random)),
        }
    }

    /// Avanza `n` pasos (se detiene antes si la población se extingue).
    pub fn step(&mut self, n: u32) {
        self.sim.run(u64::from(n));
    }

    /// Buffer RGBA por celda: escala de azúcar (claro→ámbar) y casi negro para
    /// las celdas ocupadas por un agente.
    #[must_use]
    pub fn pixels(&self) -> Vec<u8> {
        const P: [[u8; 3]; 6] = [
            [255, 255, 245],
            [255, 244, 170],
            [255, 224, 110],
            [248, 205, 70],
            [235, 185, 25],
            [28, 28, 32],
        ];
        to_rgba(&self.sim.model.cells(), &P)
    }

    /// Ancho de la grilla (celdas).
    #[wasm_bindgen(getter)]
    #[must_use]
    pub fn width(&self) -> usize {
        self.sim.model.width()
    }

    /// Alto de la grilla (celdas).
    #[wasm_bindgen(getter)]
    #[must_use]
    pub fn height(&self) -> usize {
        self.sim.model.height()
    }

    /// Población viva ahora.
    #[wasm_bindgen(getter)]
    #[must_use]
    pub fn population(&self) -> usize {
        self.sim.model.population()
    }

    /// Coeficiente de Gini de la riqueza ahora.
    #[wasm_bindgen(getter)]
    #[must_use]
    pub fn gini(&self) -> f64 {
        self.sim.model.gini()
    }
}
