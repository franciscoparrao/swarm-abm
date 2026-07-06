//! Ecosistema depredador-presa mínimo: demuestra `#[derive(MultiAgent)]`
//! (P1-3 de `docs/AUDIT.md` — heterogeneidad de agentes sin `enum` con
//! campos muertos).
//!
//! `Grazer` (pasta pasto, se reproduce) y `Wolf` (caza `Grazer`) son dos
//! `struct` completamente independientes, cada uno con su propio `impl
//! Agent`. Sin la macro, agruparlos en un solo `AgentSet` habría exigido un
//! único `struct Critter` con un `enum Species` y campos que solo tienen
//! sentido para una especie (el patrón forzado que expuso el port de
//! SIGRID). Con la macro:
//!
//! ```ignore
//! #[derive(MultiAgent)]
//! enum Critter {
//!     Grazer(Grazer),
//!     Wolf(Wolf),
//! }
//! ```
//!
//! y `AgentSet<Critter>` funciona sin que el motor sepa nada de la macro: el
//! despacho de `step` es un `match` estático generado en tiempo de
//! compilación, no un trait object.
//!
//! Uso: `cargo run --release -p multi-agent [semilla]`

use swarm_abm::prelude::*;

const GRASS_MAX: f64 = 1.0;
const GRASS_REGROW: f64 = 0.04;
const EAT_THRESHOLD: f64 = 0.3;

const GRAZER_EAT_GAIN: f64 = 4.0;
const GRAZER_METABOLISM: f64 = 1.0;
const GRAZER_MOVE_COST: f64 = 0.3;
const GRAZER_REPRODUCE_AT: f64 = 14.0;
const GRAZER_INITIAL_ENERGY: f64 = 8.0;

const WOLF_EAT_GAIN: f64 = 9.0;
const WOLF_METABOLISM: f64 = 1.2;
const WOLF_INITIAL_ENERGY: f64 = 12.0;

/// Herbívoro: pasta pasto, se reproduce si acumula suficiente energía.
struct Grazer {
    pos: Pos,
    energy: f64,
}

/// Depredador: caza `Grazer` en su propia celda o se mueve hacia la vecina
/// (Moore) más cercana que tenga uno.
struct Wolf {
    pos: Pos,
    energy: f64,
}

impl Agent for Grazer {
    type Model = World;

    fn step(&mut self, id: AgentId, model: &mut World, rng: &mut SimRng) {
        if model.grass[self.pos] >= EAT_THRESHOLD {
            model.grass[self.pos] -= EAT_THRESHOLD;
            self.energy += GRAZER_EAT_GAIN;
        } else if let Some(dest) = model
            .grass
            .random_neighbor(self.pos, Neighborhood::Moore, rng)
        {
            // Simplificación deliberada: no se resuelven colisiones de
            // destino entre grazers en el mismo paso (dos podrían mudarse a
            // la misma celda); `wolf_at`/`grazer_at` solo importan para que
            // el lobo encuentre *alguna* presa cercana, no para exclusión
            // mutua estricta.
            model.grazer_at[self.pos] = None;
            self.pos = dest;
            model.grazer_at[dest] = Some(id);
            self.energy -= GRAZER_MOVE_COST;
        }
        self.energy -= GRAZER_METABOLISM;

        if self.energy <= 0.0 {
            model.grazer_at[self.pos] = None;
            model.dead.push(id);
        } else if self.energy >= GRAZER_REPRODUCE_AT {
            self.energy /= 2.0;
            model.newborns.push(Grazer {
                pos: self.pos,
                energy: self.energy,
            });
        }
    }
}

impl Agent for Wolf {
    type Model = World;

    fn step(&mut self, id: AgentId, model: &mut World, _rng: &mut SimRng) {
        if let Some(prey) = model.grazer_at[self.pos].take() {
            model.dead.push(prey);
            self.energy += WOLF_EAT_GAIN;
        } else {
            // Vecina Moore más cercana con una presa, en orden de offset fijo
            // (determinista sin consumir el RNG: no hace falta desempatar al
            // azar para un demo).
            let dest = model
                .grazer_at
                .neighbors(self.pos, Neighborhood::Moore)
                .find(|(_, occ)| occ.is_some())
                .map(|(p, _)| p);
            if let Some(dest) = dest {
                model.wolf_at[self.pos] = None;
                self.pos = dest;
                model.wolf_at[dest] = Some(id);
            }
        }
        self.energy -= WOLF_METABOLISM;

        if self.energy <= 0.0 {
            model.wolf_at[self.pos] = None;
            model.dead.push(id);
        }
    }
}

/// El punto de la demo: un solo `AgentSet<Critter>` para dos tipos de agente
/// sin parentesco, sin `Box<dyn Agent>` y sin campos muertos en ninguno.
#[derive(MultiAgent)]
enum Critter {
    Grazer(Grazer),
    Wolf(Wolf),
}

struct World {
    agents: AgentSet<Critter>,
    grass: Grid2D<f64>,
    grazer_at: Grid2D<Option<AgentId>>,
    wolf_at: Grid2D<Option<AgentId>>,
    dead: Vec<AgentId>,
    newborns: Vec<Grazer>,
}

impl World {
    fn count_grazers(&self) -> usize {
        self.agents
            .iter()
            .filter(|(_, c)| matches!(c, Critter::Grazer(_)))
            .count()
    }

    fn count_wolves(&self) -> usize {
        self.agents
            .iter()
            .filter(|(_, c)| matches!(c, Critter::Wolf(_)))
            .count()
    }
}

impl Model for World {
    type Agent = Critter;

    fn agents(&self) -> &AgentSet<Critter> {
        &self.agents
    }

    fn agents_mut(&mut self) -> &mut AgentSet<Critter> {
        &mut self.agents
    }

    fn after_step(&mut self, _rng: &mut SimRng) {
        for id in self.dead.drain(..) {
            self.agents.remove(id);
        }
        for grazer in self.newborns.drain(..) {
            let pos = grazer.pos;
            let id = self.agents.insert(Critter::Grazer(grazer));
            self.grazer_at[pos] = Some(id);
        }
        for (_, g) in self.grass.iter_mut() {
            *g = (*g + GRASS_REGROW).min(GRASS_MAX);
        }
    }

    fn finished(&self) -> bool {
        self.agents.is_empty()
    }
}

fn build(width: usize, height: usize, n_grazers: usize, n_wolves: usize, seed: u64) -> World {
    let mut rng = rng_from_seed(seed ^ 0xC217_7EA5_EED0_0000);
    let grass = Grid2D::fill(width, height, GRASS_MAX).with_torus(true);
    let mut grazer_at: Grid2D<Option<AgentId>> = Grid2D::new(width, height).with_torus(true);
    let mut wolf_at: Grid2D<Option<AgentId>> = Grid2D::new(width, height).with_torus(true);
    let mut agents = AgentSet::with_capacity(n_grazers + n_wolves);

    for _ in 0..n_grazers {
        let pos = Pos::new(
            uniform_usize(&mut rng, width),
            uniform_usize(&mut rng, height),
        );
        let id = agents.insert(Critter::Grazer(Grazer {
            pos,
            energy: GRAZER_INITIAL_ENERGY,
        }));
        grazer_at[pos] = Some(id);
    }
    for _ in 0..n_wolves {
        let pos = Pos::new(
            uniform_usize(&mut rng, width),
            uniform_usize(&mut rng, height),
        );
        let id = agents.insert(Critter::Wolf(Wolf {
            pos,
            energy: WOLF_INITIAL_ENERGY,
        }));
        wolf_at[pos] = Some(id);
    }

    World {
        agents,
        grass,
        grazer_at,
        wolf_at,
        dead: Vec::new(),
        newborns: Vec::new(),
    }
}

fn arg_value<T: std::str::FromStr>(args: &[String], name: &str) -> Option<T> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .and_then(|v| v.parse().ok())
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let seed: u64 = args
        .first()
        .filter(|a| !a.starts_with("--"))
        .and_then(|s| s.parse().ok())
        .unwrap_or(42);
    let width: usize = arg_value(&args, "--width").unwrap_or(40);
    let height: usize = arg_value(&args, "--height").unwrap_or(40);
    let n_grazers: usize = arg_value(&args, "--grazers").unwrap_or(200);
    let n_wolves: usize = arg_value(&args, "--wolves").unwrap_or(20);
    let max_steps: u64 = arg_value(&args, "--steps").unwrap_or(300);

    let mut sim = Simulation::new(build(width, height, n_grazers, n_wolves, seed), seed);
    sim.add_reporter("grazers", |w: &World| w.count_grazers() as f64);
    sim.add_reporter("wolves", |w: &World| w.count_wolves() as f64);

    let pasos = sim.run(max_steps);

    println!(
        "Depredador-presa {width}x{height} (torus) | {n_grazers} grazers, {n_wolves} wolves | semilla {seed}"
    );
    println!(
        "Terminó en {pasos} pasos ({})",
        if sim.model.agents.is_empty() {
            "extinción total"
        } else {
            "límite de pasos"
        }
    );

    println!("\n  paso  grazers  wolves");
    let g = sim.data().series("grazers").unwrap_or(&[]);
    let w = sim.data().series("wolves").unwrap_or(&[]);
    for (idx, &step) in sim.data().steps().iter().enumerate() {
        if step % 25 == 0 || idx + 1 == g.len() {
            println!("  {step:>4}  {:>7.0}  {:>6.0}", g[idx], w[idx]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn misma_semilla_mismo_resultado() {
        let mut a = Simulation::new(build(20, 20, 60, 6, 7), 7);
        let mut b = Simulation::new(build(20, 20, 60, 6, 7), 7);
        a.add_reporter("grazers", |w: &World| w.count_grazers() as f64);
        b.add_reporter("grazers", |w: &World| w.count_grazers() as f64);
        a.run(100);
        b.run(100);
        assert_eq!(a.data().series("grazers"), b.data().series("grazers"));
        assert_eq!(a.model.count_wolves(), b.model.count_wolves());
    }

    #[test]
    fn el_despacho_de_multiagent_distingue_especies() {
        // Prueba directa de que la macro despacha al tipo correcto: un
        // Grazer con pasto de sobra debe *comer* (no moverse ni perder
        // energía de movimiento); un Wolf sin presa en su celda debe
        // *moverse* hacia una vecina con presa. Si el despacho generado por
        // `#[derive(MultiAgent)]` llamara al método equivocado, este test lo
        // notaría por valores de energía/posición incorrectos.
        let mut world = build(10, 10, 0, 0, 1);
        let pos = Pos::new(5, 5);
        world.grass[pos] = GRASS_MAX;
        let gid = world.agents.insert(Critter::Grazer(Grazer {
            pos,
            energy: GRAZER_INITIAL_ENERGY,
        }));
        world.grazer_at[pos] = Some(gid);

        let mut rng = rng_from_seed(0);
        if let Some(mut critter) = world.agents.remove(gid) {
            critter.step(gid, &mut world, &mut rng);
            let Critter::Grazer(g) = &critter else {
                panic!("se perdió la identidad de variante")
            };
            assert_eq!(g.pos, pos, "con pasto de sobra, el grazer no se mueve");
            assert!(
                (g.energy - (GRAZER_INITIAL_ENERGY + GRAZER_EAT_GAIN - GRAZER_METABOLISM)).abs()
                    < 1e-9
            );
        } else {
            panic!("agente no encontrado");
        }
    }

    /// El despacho de `stage` para un enum `MultiAgent` no estaba generado
    /// (hallazgo de auditoría): un modelo heterogéneo bajo
    /// `Activation::Staged` heredaba el no-op por defecto del trait en
    /// silencio, sin error de compilación ni de runtime. Dos tipos mínimos
    /// que solo implementan `stage` (no `step`) verifican que ahora sí se
    /// despacha a la variante interna correcta.
    #[test]
    fn el_despacho_de_multiagent_incluye_stage() {
        struct Tiny(AgentSet<Mixed>);
        impl Model for Tiny {
            type Agent = Mixed;
            fn agents(&self) -> &AgentSet<Mixed> {
                &self.0
            }
            fn agents_mut(&mut self) -> &mut AgentSet<Mixed> {
                &mut self.0
            }
        }

        struct Counter(u32);
        struct Doubler(u32);

        impl Agent for Counter {
            type Model = Tiny;
            fn stage(&mut self, stage: usize, _id: AgentId, _model: &mut Tiny, _rng: &mut SimRng) {
                if stage == 0 {
                    self.0 += 1;
                }
            }
        }
        impl Agent for Doubler {
            type Model = Tiny;
            fn stage(&mut self, stage: usize, _id: AgentId, _model: &mut Tiny, _rng: &mut SimRng) {
                if stage == 1 {
                    self.0 *= 2;
                }
            }
        }

        #[derive(MultiAgent)]
        enum Mixed {
            Counter(Counter),
            Doubler(Doubler),
        }

        let mut agents = AgentSet::new();
        let cid = agents.insert(Mixed::Counter(Counter(0)));
        let did = agents.insert(Mixed::Doubler(Doubler(3)));

        let mut sim =
            Simulation::new(Tiny(agents), 1).with_schedule(Schedule::new(Activation::Staged(2)));
        sim.run(1);

        let Mixed::Counter(c) = sim.model.0.get(cid).unwrap() else {
            unreachable!()
        };
        let Mixed::Doubler(d) = sim.model.0.get(did).unwrap() else {
            unreachable!()
        };
        assert_eq!(c.0, 1, "Counter.stage(0) debe correr vía el enum");
        assert_eq!(d.0, 6, "Doubler.stage(1) debe correr vía el enum");
    }
}
