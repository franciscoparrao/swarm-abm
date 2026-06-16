//! Metaheurísticas de optimización de caja negra para calibrar el modelo,
//! con interfaz uniforme para compararlas de forma justa (mismo presupuesto
//! de evaluaciones). Todas **maximizan** la función objetivo en un cubo
//! `[lo, hi]^D` y son deterministas dada la semilla del [`SimRng`].
//!
//! Implementadas: Differential Evolution, Genetic Algorithm, Particle Swarm
//! Optimization, Simulated Annealing y Grey Wolf Optimizer — el mismo
//! conjunto que el paper original de `debris-flow-abm`, ahora comparables con
//! rigor estadístico gracias al rendimiento del motor.

use swarm_core::prelude::*;

/// Rangos por dimensión del espacio de búsqueda.
pub struct Bounds {
    pub lo: Vec<f64>,
    pub hi: Vec<f64>,
}

impl Bounds {
    pub fn dim(&self) -> usize {
        self.lo.len()
    }

    fn clamp(&self, x: &mut [f64]) {
        for (xi, (&lo, &hi)) in x.iter_mut().zip(self.lo.iter().zip(&self.hi)) {
            *xi = xi.clamp(lo, hi);
        }
    }

    fn sample(&self, rng: &mut SimRng) -> Vec<f64> {
        self.lo
            .iter()
            .zip(&self.hi)
            .map(|(&lo, &hi)| rng.random_range(lo..hi))
            .collect()
    }
}

/// Resultado de una corrida de optimización.
pub struct Outcome {
    pub best_x: Vec<f64>,
    pub best_f: f64,
    /// Mejor fitness acumulado tras cada bloque de evaluaciones (convergencia).
    pub history: Vec<f64>,
    pub evals: usize,
}

/// Las cinco metaheurísticas disponibles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    De,
    Ga,
    Pso,
    Sa,
    Gwo,
}

impl Method {
    pub const ALL: [Method; 5] = [Method::De, Method::Ga, Method::Pso, Method::Sa, Method::Gwo];

    pub fn name(self) -> &'static str {
        match self {
            Method::De => "DE",
            Method::Ga => "GA",
            Method::Pso => "PSO",
            Method::Sa => "SA",
            Method::Gwo => "GWO",
        }
    }

    pub fn parse(s: &str) -> Option<Method> {
        Method::ALL
            .into_iter()
            .find(|m| m.name().eq_ignore_ascii_case(s))
    }

    /// Ejecuta el método con el presupuesto de evaluaciones dado.
    pub fn run(
        self,
        obj: &impl Fn(&[f64]) -> f64,
        bounds: &Bounds,
        budget: usize,
        rng: &mut SimRng,
    ) -> Outcome {
        match self {
            Method::De => de(obj, bounds, budget, rng),
            Method::Ga => ga(obj, bounds, budget, rng),
            Method::Pso => pso(obj, bounds, budget, rng),
            Method::Sa => sa(obj, bounds, budget, rng),
            Method::Gwo => gwo(obj, bounds, budget, rng),
        }
    }
}

/// Variable normal estándar por Box-Muller (el `SimRng` solo da uniformes).
fn randn(rng: &mut SimRng) -> f64 {
    let u1: f64 = rng.random_range(1e-12..1.0);
    let u2: f64 = rng.random_range(0.0..1.0);
    (-2.0 * u1.ln()).sqrt() * (std::f64::consts::TAU * u2).cos()
}

fn argmax(v: &[f64]) -> usize {
    v.iter()
        .enumerate()
        .fold((0, f64::MIN), |a, (i, &x)| if x > a.1 { (i, x) } else { a })
        .0
}

// ---------------------------------------------------------------------------
// Differential Evolution (DE/rand/1/bin)
// ---------------------------------------------------------------------------

fn de(obj: &impl Fn(&[f64]) -> f64, b: &Bounds, budget: usize, rng: &mut SimRng) -> Outcome {
    let d = b.dim();
    let np = (10 * d).min(budget.max(4) / 2).max(4);
    let (f, cr) = (0.6, 0.9);
    let mut pop: Vec<Vec<f64>> = (0..np).map(|_| b.sample(rng)).collect();
    let mut fit: Vec<f64> = pop.iter().map(|x| obj(x)).collect();
    let mut evals = np;
    let mut history = vec![fit[argmax(&fit)]];

    while evals < budget {
        for i in 0..np {
            let (r1, r2, r3) = three_distinct(np, i, rng);
            let jrand = rng.random_range(0..d);
            let mut trial = pop[i].clone();
            for j in 0..d {
                if j == jrand || rng.random_range(0.0..1.0) < cr {
                    trial[j] = pop[r1][j] + f * (pop[r2][j] - pop[r3][j]);
                }
            }
            b.clamp(&mut trial);
            let tf = obj(&trial);
            evals += 1;
            if tf >= fit[i] {
                pop[i] = trial;
                fit[i] = tf;
            }
            if evals >= budget {
                break;
            }
        }
        history.push(fit[argmax(&fit)]);
    }
    finish(pop, fit, history, evals)
}

fn three_distinct(n: usize, i: usize, rng: &mut SimRng) -> (usize, usize, usize) {
    let pick = |rng: &mut SimRng, excl: &[usize]| loop {
        let r = rng.random_range(0..n);
        if !excl.contains(&r) {
            return r;
        }
    };
    let r1 = pick(rng, &[i]);
    let r2 = pick(rng, &[i, r1]);
    let r3 = pick(rng, &[i, r1, r2]);
    (r1, r2, r3)
}

// ---------------------------------------------------------------------------
// Genetic Algorithm (torneo + cruce blend-α + mutación gaussiana + elitismo)
// ---------------------------------------------------------------------------

fn ga(obj: &impl Fn(&[f64]) -> f64, b: &Bounds, budget: usize, rng: &mut SimRng) -> Outcome {
    let d = b.dim();
    let pop_size = (8 * d).min(budget.max(4) / 2).max(6);
    let (cx_alpha, mut_rate) = (0.5, 0.2);
    let mut pop: Vec<Vec<f64>> = (0..pop_size).map(|_| b.sample(rng)).collect();
    let mut fit: Vec<f64> = pop.iter().map(|x| obj(x)).collect();
    let mut evals = pop_size;
    let mut history = vec![fit[argmax(&fit)]];

    let tournament = |rng: &mut SimRng, fit: &[f64]| {
        let a = rng.random_range(0..fit.len());
        let c = rng.random_range(0..fit.len());
        if fit[a] >= fit[c] { a } else { c }
    };

    while evals < budget {
        // Elitismo: conserva el mejor.
        let elite = argmax(&fit);
        let mut new_pop = vec![pop[elite].clone()];
        let mut new_fit = vec![fit[elite]];

        while new_pop.len() < pop_size && evals < budget {
            let p1 = tournament(rng, &fit);
            let p2 = tournament(rng, &fit);
            // Cruce blend-α (BLX-α).
            let mut child: Vec<f64> = (0..d)
                .map(|j| {
                    let (lo, hi) = (pop[p1][j].min(pop[p2][j]), pop[p1][j].max(pop[p2][j]));
                    let range = (hi - lo).max(1e-12);
                    rng.random_range((lo - cx_alpha * range)..(hi + cx_alpha * range))
                })
                .collect();
            // Mutación gaussiana relativa al rango de cada dimensión.
            for (j, cj) in child.iter_mut().enumerate() {
                if rng.random_range(0.0..1.0) < mut_rate {
                    *cj += 0.1 * (b.hi[j] - b.lo[j]) * randn(rng);
                }
            }
            b.clamp(&mut child);
            let cf = obj(&child);
            evals += 1;
            new_pop.push(child);
            new_fit.push(cf);
        }
        pop = new_pop;
        fit = new_fit;
        history.push(fit[argmax(&fit)]);
    }
    finish(pop, fit, history, evals)
}

// ---------------------------------------------------------------------------
// Particle Swarm Optimization (inercia + cognitivo + social)
// ---------------------------------------------------------------------------

fn pso(obj: &impl Fn(&[f64]) -> f64, b: &Bounds, budget: usize, rng: &mut SimRng) -> Outcome {
    let d = b.dim();
    let n = (8 * d).min(budget.max(4) / 2).max(6);
    let (w, c1, c2) = (0.729, 1.49445, 1.49445);
    let mut x: Vec<Vec<f64>> = (0..n).map(|_| b.sample(rng)).collect();
    let mut v: Vec<Vec<f64>> = (0..n)
        .map(|_| {
            (0..d)
                .map(|j| 0.1 * (b.hi[j] - b.lo[j]) * (rng.random_range(-1.0..1.0)))
                .collect()
        })
        .collect();
    let mut pbest = x.clone();
    let mut pbest_f: Vec<f64> = x.iter().map(|xi| obj(xi)).collect();
    let mut evals = n;
    let g0 = argmax(&pbest_f);
    let mut gbest = pbest[g0].clone();
    let mut gbest_f = pbest_f[g0];
    let mut history = vec![gbest_f];

    while evals < budget {
        for i in 0..n {
            for j in 0..d {
                let r1: f64 = rng.random_range(0.0..1.0);
                let r2: f64 = rng.random_range(0.0..1.0);
                v[i][j] = w * v[i][j]
                    + c1 * r1 * (pbest[i][j] - x[i][j])
                    + c2 * r2 * (gbest[j] - x[i][j]);
                x[i][j] += v[i][j];
            }
            b.clamp(&mut x[i]);
            let f = obj(&x[i]);
            evals += 1;
            if f >= pbest_f[i] {
                pbest[i] = x[i].clone();
                pbest_f[i] = f;
                if f >= gbest_f {
                    gbest = x[i].clone();
                    gbest_f = f;
                }
            }
            if evals >= budget {
                break;
            }
        }
        history.push(gbest_f);
    }
    Outcome {
        best_x: gbest,
        best_f: gbest_f,
        history,
        evals,
    }
}

// ---------------------------------------------------------------------------
// Simulated Annealing (cadena única, vecino gaussiano, enfriamiento geométrico)
// ---------------------------------------------------------------------------

fn sa(obj: &impl Fn(&[f64]) -> f64, b: &Bounds, budget: usize, rng: &mut SimRng) -> Outcome {
    let mut cur = b.sample(rng);
    let mut cur_f = obj(&cur);
    let mut evals = 1;
    let mut best = cur.clone();
    let mut best_f = cur_f;
    let mut history = vec![best_f];
    // Enfriamiento geométrico de T0 a ~T0/1000 a lo largo del presupuesto.
    let t0 = 0.1_f64;
    let cooling = (0.001_f64).powf(1.0 / budget.max(2) as f64);
    let mut temp = t0;

    while evals < budget {
        let mut cand = cur.clone();
        for (j, cj) in cand.iter_mut().enumerate() {
            *cj += 0.1 * (b.hi[j] - b.lo[j]) * randn(rng);
        }
        b.clamp(&mut cand);
        let cf = obj(&cand);
        evals += 1;
        let delta = cf - cur_f; // maximizar
        if delta >= 0.0 || rng.random_range(0.0..1.0) < (delta / temp).exp() {
            cur = cand;
            cur_f = cf;
            if cf > best_f {
                best = cur.clone();
                best_f = cf;
            }
        }
        temp *= cooling;
        history.push(best_f);
    }
    Outcome {
        best_x: best,
        best_f,
        history,
        evals,
    }
}

// ---------------------------------------------------------------------------
// Grey Wolf Optimizer (alpha/beta/delta, parámetro a decreciente)
// ---------------------------------------------------------------------------

fn gwo(obj: &impl Fn(&[f64]) -> f64, b: &Bounds, budget: usize, rng: &mut SimRng) -> Outcome {
    let d = b.dim();
    let n = (8 * d).min(budget.max(4) / 2).max(6);
    let mut pack: Vec<Vec<f64>> = (0..n).map(|_| b.sample(rng)).collect();
    let mut fit: Vec<f64> = pack.iter().map(|x| obj(x)).collect();
    let mut evals = n;
    let iters = (budget.saturating_sub(n)) / n.max(1) + 1;
    let mut history = Vec::new();

    let top3 = |pack: &[Vec<f64>], fit: &[f64]| {
        let mut idx: Vec<usize> = (0..fit.len()).collect();
        idx.sort_by(|&a, &c| fit[c].partial_cmp(&fit[a]).unwrap());
        (
            pack[idx[0]].clone(),
            pack[idx[1.min(idx.len() - 1)]].clone(),
            pack[idx[2.min(idx.len() - 1)]].clone(),
            fit[idx[0]],
        )
    };

    for it in 0..iters {
        if evals >= budget {
            break;
        }
        let (alpha, beta, delta, alpha_f) = top3(&pack, &fit);
        history.push(alpha_f);
        // a decrece linealmente de 2 a 0.
        let a = 2.0 * (1.0 - it as f64 / iters.max(1) as f64);
        for i in 0..n {
            for j in 0..d {
                let upd = |leader: f64, x: f64, rng: &mut SimRng| {
                    let (r1, r2): (f64, f64) =
                        (rng.random_range(0.0..1.0), rng.random_range(0.0..1.0));
                    let a_coef = 2.0 * a * r1 - a;
                    let c_coef = 2.0 * r2;
                    let dist = (c_coef * leader - x).abs();
                    leader - a_coef * dist
                };
                let x1 = upd(alpha[j], pack[i][j], rng);
                let x2 = upd(beta[j], pack[i][j], rng);
                let x3 = upd(delta[j], pack[i][j], rng);
                pack[i][j] = (x1 + x2 + x3) / 3.0;
            }
            b.clamp(&mut pack[i]);
            fit[i] = obj(&pack[i]);
            evals += 1;
            if evals >= budget {
                break;
            }
        }
    }
    finish(pack, fit, history, evals)
}

/// Empaqueta el mejor individuo de una población como [`Outcome`].
fn finish(pop: Vec<Vec<f64>>, fit: Vec<f64>, history: Vec<f64>, evals: usize) -> Outcome {
    let i = argmax(&fit);
    Outcome {
        best_x: pop[i].clone(),
        best_f: fit[i],
        history,
        evals,
    }
}
