//! **Deterministic** experiment design: Latin hypercube sampling (LHS),
//! the Morris method (elementary effects), and Sobol' global sensitivity
//! analysis (S1/ST indices, Saltelli scheme, with bootstrap).
//!
//! This is Tier-1 item #1 of the SOTA analysis (`docs/SOTA.md`): the
//! engine's real differentiator is not *having* experiment design (krABMaga
//! and GAMA already have it) but making it **bit-for-bit reproducible by
//! construction**, just like the rest of the engine. The precedent is the
//! SIGRID port (`models/sigrid/PARITY.md`): the Sobol' analysis for that
//! validation was done with a hybrid harness (SALib sampled in Python, Rust
//! evaluated) because this module didn't exist yet. This internalizes it —
//! no SALib, no Python.
//!
//! ## Why it depends on the `sobol` crate (and why that doesn't repeat the P0-2 mistake)
//!
//! The engine reimplemented uniform sampling and shuffling (P0-2) because
//! `rand::Rng::random_range`/`SliceRandom::shuffle` are **unspecified-algorithm**
//! APIs: `rand` can change how it maps bits to a result between versions,
//! without notice. A Sobol' sequence is different in nature: given the same
//! **direction numbers** (Joe & Kuo's, published and stable — the `sobol`
//! crate embeds them as-is, without reprocessing them) and the
//! Antonov-Saleev recurrence (a canonical algorithm, with no implementation
//! ambiguity), the sequence is **mathematically determined** — there is no
//! "library choice" that a version update could change. It's a dependency
//! on a specification, not on an implementation detail.
//!
//! ## v1 scope (documented, not implicit)
//!
//! - One model evaluation per design point (seed derived deterministically
//!   from the point's index). Very noisy models would benefit from
//!   averaging several replicates per point — not implemented; the user can
//!   wrap `outcome` with their own averaging if needed.
//! - Morris uses a single step direction (`+delta`) instead of the random
//!   direction (±) of the original Morris (1991) design — this simplifies
//!   the implementation without affecting the interpretation of
//!   `mu`/`mu_star`/`sigma` (see [`morris`]).
//! - `sobol_design` supports up to 50 parameters (Joe & Kuo's `minimal`
//!   variant covers 100 dimensions; the Saltelli scheme uses `2·d`).

use crate::model::Model;
use crate::rng::{child_rng, rng_from_seed, shuffle, uniform_below, uniform_f64};
use crate::sim::Simulation;
use rand::RngCore;

/// Specification of a parameter: sampled uniformly in `[low, high]`.
#[derive(Debug, Clone)]
pub struct ParamSpec {
    /// Parameter name (used to identify the results).
    pub name: String,
    /// Lower bound of the range.
    pub low: f64,
    /// Upper bound of the range.
    pub high: f64,
}

impl ParamSpec {
    /// Creates a parameter specification.
    ///
    /// # Panics
    /// If `low >= high`.
    #[must_use]
    pub fn new(name: impl Into<String>, low: f64, high: f64) -> Self {
        assert!(low < high, "low must be < high");
        Self {
            name: name.into(),
            low,
            high,
        }
    }

    /// Scales `u ∈ [0,1]` to this parameter's `[low, high]` range.
    fn scale(&self, u: f64) -> f64 {
        self.low + u * (self.high - self.low)
    }
}

/// Evaluates the model at each `(point, seed)`, in parallel if the
/// `parallel` feature is enabled.
fn evaluate_all<M, B, O>(
    points: &[(Vec<f64>, u64)],
    max_steps: u64,
    build: &B,
    outcome: &O,
) -> Vec<f64>
where
    M: Model,
    B: Fn(&[f64], u64) -> Simulation<M> + Sync,
    O: Fn(&Simulation<M>) -> f64 + Sync,
{
    let run_one = |(point, seed): &(Vec<f64>, u64)| -> f64 {
        let mut sim = build(point, *seed);
        sim.run(max_steps);
        outcome(&sim)
    };
    #[cfg(feature = "parallel")]
    {
        use rayon::prelude::*;
        points.par_iter().map(run_one).collect()
    }
    #[cfg(not(feature = "parallel"))]
    {
        points.iter().map(run_one).collect()
    }
}

// ---------------------------------------------------------------------------
// Latin Hypercube Sampling (LHS)
// ---------------------------------------------------------------------------

/// **Latin hypercube** sampling: `n` points in parameter space such that,
/// projected onto each axis, exactly one falls in each of the `n` equal
/// intervals (stratified coverage, better than purely random sampling for
/// the same `n`). Deterministic given the seed.
#[must_use]
pub fn latin_hypercube(specs: &[ParamSpec], n: usize, seed: u64) -> Vec<Vec<f64>> {
    let d = specs.len();
    let mut rng = rng_from_seed(seed);
    // Per dimension: n intervals [i/n, (i+1)/n), shuffled order, uniform
    // jitter within the interval assigned to each row.
    let mut cols: Vec<Vec<f64>> = Vec::with_capacity(d);
    for _ in 0..d {
        let mut order: Vec<usize> = (0..n).collect();
        shuffle(&mut rng, &mut order);
        let col: Vec<f64> = order
            .iter()
            .map(|&i| (i as f64 + uniform_f64(&mut rng)) / n as f64)
            .collect();
        cols.push(col);
    }
    (0..n)
        .map(|row| (0..d).map(|j| specs[j].scale(cols[j][row])).collect())
        .collect()
}

// ---------------------------------------------------------------------------
// Sobol': low-discrepancy sampling + global sensitivity analysis
// ---------------------------------------------------------------------------

/// Sobol' design (Saltelli 2010 scheme): two independent matrices `A`/`B`
/// of `n` points each, generated from a `2·d`-dimensional Sobol' sequence
/// (the first `d` columns for `A`, the next `d` for `B` — decorrelated by
/// construction of the sequence). See [`sobol()`] to create it and
/// [`SobolDesign::run`] to evaluate it.
pub struct SobolDesign {
    specs: Vec<ParamSpec>,
    n: usize,
    a: Vec<Vec<f64>>,
    b: Vec<Vec<f64>>,
}

/// Creates a Sobol' design for global sensitivity analysis over `specs`,
/// with `n` base points (the total cost of evaluating the design is
/// `n·(d+2)`, see [`SobolDesign::run`]).
///
/// # Panics
/// If `specs` is empty or has more than 50 parameters (Joe & Kuo's
/// `minimal` direction-number table covers 100 dimensions; the Saltelli
/// scheme needs `2·d`).
#[must_use]
pub fn sobol(specs: &[ParamSpec], n: usize) -> SobolDesign {
    let d = specs.len();
    assert!(
        (1..=50).contains(&d),
        "sobol supports 1 to 50 parameters, {d} were requested"
    );
    let params = sobol::params::JoeKuoD6::minimal();
    // Skip a dyadically-aligned block, not just the all-zeros origin.
    // Dropping a single point is the anti-pattern of Owen (2020, "On
    // dropping the first Sobol' point"): a block of `n` consecutive points
    // that does not start at a multiple of 2^m is no longer a (t,m,s)-net,
    // and the estimator's convergence degrades toward the plain-Monte-Carlo
    // O(n^-1/2) rate (measured here: with `skip(1)`, the S1 error for
    // y=10·x1 at n=4096 was *worse* than at n=64 with an aligned skip).
    // Skipping `n.next_power_of_two()` points keeps the taken block
    // `[2^m, 2^m + n)` dyadically aligned (exactly `[2^m, 2^{m+1})` when
    // `n` is itself a power of two) and, as a side effect, still drops the
    // problematic all-zeros origin, which would otherwise give `A[0]`,
    // `B[0]` and every `AB_i[0]` the identical lower-corner value.
    // (SALib does the equivalent: it skips 1024 points — a power of two —
    // not 1.)
    let seq = sobol::Sobol::<f64>::new(2 * d, &params).skip(n.next_power_of_two());
    let mut a = Vec::with_capacity(n);
    let mut b = Vec::with_capacity(n);
    for point in seq.take(n) {
        let (pa, pb) = point.split_at(d);
        a.push(pa.iter().zip(specs).map(|(&u, s)| s.scale(u)).collect());
        b.push(pb.iter().zip(specs).map(|(&u, s)| s.scale(u)).collect());
    }
    SobolDesign {
        specs: specs.to_vec(),
        n,
        a,
        b,
    }
}

/// Result of a Sobol' sensitivity analysis: first-order indices (`s1`, the
/// individual effect of each parameter) and total-order indices (`st`,
/// individual effect + all interactions), with 95% confidence intervals
/// from bootstrap.
#[derive(Debug, Clone)]
pub struct SobolResult {
    /// Parameter names, in the same order as `s1`/`st`.
    pub names: Vec<String>,
    /// First-order indices.
    pub s1: Vec<f64>,
    /// (2.5%, 97.5%) confidence interval of each `s1`, from bootstrap.
    pub s1_conf: Vec<(f64, f64)>,
    /// Total-order indices.
    pub st: Vec<f64>,
    /// (2.5%, 97.5%) confidence interval of each `st`, from bootstrap.
    pub st_conf: Vec<(f64, f64)>,
}

impl SobolDesign {
    /// Evaluates the model at the `n·(d+2)` combinations of the design (`A`,
    /// `B`, and the `d` `AB_i` matrices, each `A` with its column `i`
    /// replaced by `B`'s) and computes S1/ST with bootstrap.
    ///
    /// `build` receives the point (`&[f64]`, in the order of `specs`) and a
    /// seed derived deterministically from `base_seed` and the point's row —
    /// same base seed ⇒ same design evaluated exactly the same way. `n_boot`
    /// is the number of bootstrap resamples (500 is a reasonable default; more
    /// gives more stable intervals at the cost of more computation — the
    /// bootstrap only recomputes the closed-form formula over evaluations
    /// already made, it does not re-run the model). `n_boot == 0` skips the
    /// bootstrap entirely: `s1`/`st` are still the real point estimates, but
    /// `s1_conf`/`st_conf` come back as `(NaN, NaN)` rather than a
    /// meaningless interval from zero resamples.
    ///
    /// **Common random numbers.** `A[j]` and every `AB_i[j]` (`i` in
    /// `0..d`) share the exact same seed — they differ from each other only
    /// in parameter `i`, so any difference in their outcome is attributable
    /// to that parameter, not to a different stochastic realization of the
    /// model. `B[j]` gets an independent seed (derived with a different
    /// domain tag), decorrelating it from `A[j]`/`AB_i[j]` as the Saltelli
    /// scheme requires. Without CRN pairing, the Jansen ST estimator
    /// `E[(y_A − y_AB_i)²]` conflates the parameter's effect with pure
    /// stochastic noise (`Δ_i + 2σ²_noise`), spuriously inflating ST for
    /// models where `step` consumes randomness — even for a parameter with
    /// no real effect. A deterministic `outcome` (like this module's own
    /// Ishigami validation) is invariant to this pairing, which is why that
    /// test alone couldn't have caught the earlier per-index-only seeding.
    pub fn run<M, B, O>(
        &self,
        base_seed: u64,
        max_steps: u64,
        n_boot: usize,
        build: B,
        outcome: O,
    ) -> SobolResult
    where
        M: Model,
        B: Fn(&[f64], u64) -> Simulation<M> + Sync,
        O: Fn(&Simulation<M>) -> f64 + Sync,
    {
        let d = self.specs.len();
        let n = self.n;

        // Domain-separated seed for row `row`, family `family` (0 = the
        // A/AB_i family, 1 = B): reuses `child_rng`'s already-validated
        // chain-hash instead of inventing a new mixer, then draws one u64
        // from the resulting stream as this point's model seed.
        let row_seed = |family: u64, row: usize| -> u64 {
            child_rng(base_seed, family, row as u64).next_u64()
        };

        let mut seeded: Vec<(Vec<f64>, u64)> = Vec::with_capacity(n * (d + 2));
        for (j, p) in self.a.iter().enumerate() {
            seeded.push((p.clone(), row_seed(0, j))); // block A: [0, n)
        }
        for (j, p) in self.b.iter().enumerate() {
            seeded.push((p.clone(), row_seed(1, j))); // block B: [n, 2n)
        }
        for i in 0..d {
            for j in 0..n {
                let mut p = self.a[j].clone();
                p[i] = self.b[j][i];
                // Same seed as A[j] (family 0): AB_i[j] differs from A[j]
                // only in column i, so the model runs the identical
                // stochastic realization save for that one parameter.
                seeded.push((p, row_seed(0, j))); // block AB_i: [(2+i)n, (3+i)n)
            }
        }
        let ys = evaluate_all(&seeded, max_steps, &build, &outcome);

        let y_a = &ys[0..n];
        let y_b = &ys[n..2 * n];
        let y_ab: Vec<Vec<f64>> = (0..d)
            .map(|i| ys[(2 + i) * n..(3 + i) * n].to_vec())
            .collect();

        let names: Vec<String> = self.specs.iter().map(|s| s.name.clone()).collect();
        sobol_indices_with_bootstrap(
            y_a,
            y_b,
            &y_ab,
            &names,
            n_boot,
            base_seed ^ 0xC0FF_EE00_D15E_A5E5,
        )
    }
}

fn variance(y: &[f64]) -> f64 {
    let n = y.len() as f64;
    let mean = y.iter().sum::<f64>() / n;
    y.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / n
}

/// Saltelli (2010, S1) and Jansen (1999, ST) estimators for a subset of
/// indices `idxs` (all of `0..n` for the point estimate, or a resample
/// with replacement for a bootstrap replicate).
fn compute_indices(
    y_a: &[f64],
    y_b: &[f64],
    y_ab: &[Vec<f64>],
    idxs: &[usize],
) -> (Vec<f64>, Vec<f64>) {
    let n = idxs.len() as f64;
    let combined: Vec<f64> = idxs.iter().flat_map(|&j| [y_a[j], y_b[j]]).collect();
    let var_y = variance(&combined);
    let d = y_ab.len();
    let mut s1 = vec![0.0; d];
    let mut st = vec![0.0; d];
    for i in 0..d {
        let mut sum_s1 = 0.0;
        let mut sum_st = 0.0;
        for &j in idxs {
            sum_s1 += y_b[j] * (y_ab[i][j] - y_a[j]);
            sum_st += (y_a[j] - y_ab[i][j]).powi(2);
        }
        // Three cases, deliberately distinguished:
        // - `var_y` non-finite (some evaluation returned NaN/±inf): the
        //   indices are undefined — propagate NaN honestly instead of
        //   silently collapsing everything to 0.0 (which would read as "no
        //   parameter matters" when the truth is "the model blew up").
        // - `var_y == 0.0` (constant model): every index is genuinely 0.
        // - otherwise: the Saltelli/Jansen estimators. Note a NaN confined
        //   to `y_ab[i]` still propagates through `sum_s1`/`sum_st` to that
        //   parameter's indices even when `var_y` is finite.
        (s1[i], st[i]) = if !var_y.is_finite() {
            (f64::NAN, f64::NAN)
        } else if var_y > 0.0 {
            ((sum_s1 / n) / var_y, (sum_st / (2.0 * n)) / var_y)
        } else {
            (0.0, 0.0)
        };
    }
    (s1, st)
}

/// `NaN` on an empty slice (reached when `n_boot == 0`: no confidence
/// interval is meaningful without resamples) instead of the `usize`
/// underflow (`sorted.len() - 1` with `len() == 0`) that a direct
/// computation would produce.
fn percentile(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() {
        return f64::NAN;
    }
    let idx = (((sorted.len() - 1) as f64) * p).round() as usize;
    sorted[idx]
}

fn sobol_indices_with_bootstrap(
    y_a: &[f64],
    y_b: &[f64],
    y_ab: &[Vec<f64>],
    names: &[String],
    n_boot: usize,
    boot_seed: u64,
) -> SobolResult {
    let n = y_a.len();
    let idxs_full: Vec<usize> = (0..n).collect();
    let (s1, st) = compute_indices(y_a, y_b, y_ab, &idxs_full);

    let d = s1.len();
    let mut s1_boot: Vec<Vec<f64>> = vec![Vec::with_capacity(n_boot); d];
    let mut st_boot: Vec<Vec<f64>> = vec![Vec::with_capacity(n_boot); d];
    let mut rng = rng_from_seed(boot_seed);
    for _ in 0..n_boot {
        let resample: Vec<usize> = (0..n)
            .map(|_| uniform_below(&mut rng, n as u64) as usize)
            .collect();
        let (s1_b, st_b) = compute_indices(y_a, y_b, y_ab, &resample);
        for i in 0..d {
            s1_boot[i].push(s1_b[i]);
            st_boot[i].push(st_b[i]);
        }
    }
    let conf_of = |boot: &mut [f64], point_estimate: f64| -> (f64, f64) {
        // A NaN point estimate means some evaluation was non-finite; a
        // bootstrap resample can exclude the offending rows by chance and
        // yield a misleadingly finite interval — propagate NaN instead.
        if point_estimate.is_nan() {
            return (f64::NAN, f64::NAN);
        }
        boot.sort_by(f64::total_cmp);
        (percentile(boot, 0.025), percentile(boot, 0.975))
    };
    let s1_conf: Vec<(f64, f64)> = s1_boot
        .into_iter()
        .zip(&s1)
        .map(|(mut v, &e)| conf_of(&mut v, e))
        .collect();
    let st_conf: Vec<(f64, f64)> = st_boot
        .into_iter()
        .zip(&st)
        .map(|(mut v, &e)| conf_of(&mut v, e))
        .collect();

    SobolResult {
        names: names.to_vec(),
        s1,
        s1_conf,
        st,
        st_conf,
    }
}

// ---------------------------------------------------------------------------
// Morris: elementary effects
// ---------------------------------------------------------------------------

/// Morris design (elementary effects): `n_trajectories` trajectories of
/// `d+1` points each, over a grid of `levels` levels per axis. See
/// [`morris`] to create it and [`MorrisDesign::run`] to evaluate it.
pub struct MorrisDesign {
    specs: Vec<ParamSpec>,
    delta: f64,
    /// Each trajectory: `d+1` points in `[0,1]^d` (unscaled) + the order of
    /// perturbed dimensions (length `d`).
    trajectories: Vec<(Vec<Vec<f64>>, Vec<usize>)>,
}

/// Creates a Morris design: `n_trajectories` runs, each perturbing one
/// dimension at a time (in random order, different per trajectory) in
/// fixed steps of size `delta = levels / (2·(levels-1))` over a grid of
/// `levels` levels per axis — the classic Morris (1991) design. The total
/// cost is `n_trajectories·(d+1)` evaluations.
///
/// **Deliberate simplification**: the step is always `+delta` (never
/// `-`), unlike the original design, which picks the direction at random.
/// This does not affect the interpretation of `mu`/`mu_star`/`sigma` (they
/// remain the mean, mean absolute value, and standard deviation of the
/// elementary effects respectively), it only reduces the symmetry of the
/// base-point sampling.
///
/// # Panics
/// If `levels < 2`.
#[must_use]
pub fn morris(
    specs: &[ParamSpec],
    n_trajectories: usize,
    levels: usize,
    seed: u64,
) -> MorrisDesign {
    assert!(levels >= 2, "morris requires levels >= 2");
    let d = specs.len();
    let max_level = levels - 1;
    let delta = levels as f64 / (2.0 * max_level as f64);
    let mut rng = rng_from_seed(seed);

    // Valid base levels: those that leave room for a +delta step without
    // going outside [0,1] (l/max_level + delta <= 1).
    let max_base_level = max_base_level(levels);

    let mut trajectories = Vec::with_capacity(n_trajectories);
    for _ in 0..n_trajectories {
        let mut x0 = vec![0.0; d];
        for slot in &mut x0 {
            let lvl = uniform_below(&mut rng, max_base_level + 1);
            *slot = lvl as f64 / max_level as f64;
        }
        let mut order: Vec<usize> = (0..d).collect();
        shuffle(&mut rng, &mut order);

        let mut points = Vec::with_capacity(d + 1);
        points.push(x0.clone());
        let mut current = x0;
        for &dim in &order {
            current[dim] += delta;
            points.push(current.clone());
        }
        trajectories.push((points, order));
    }

    MorrisDesign {
        specs: specs.to_vec(),
        delta,
        trajectories,
    }
}

/// Largest base level `l` such that `l/max_level + delta <= 1`, i.e. a
/// `+delta` step from level `l` stays inside `[0,1]`. With
/// `delta = levels / (2·(levels-1))` and `max_level = levels-1`:
/// `l <= max_level·(1-delta) = (levels-1) - levels/2 = (levels-2)/2`,
/// which integer division computes exactly for even and odd `levels`.
/// (The previous float formula `(max_level·(1-delta)).floor()` fell one
/// level short for `levels ∈ {30, 88, 150, 182, …}` due to rounding.)
fn max_base_level(levels: usize) -> u64 {
    debug_assert!(levels >= 2);
    (levels as u64 - 2) / 2
}

/// Elementary-effect statistics for a parameter (see [`morris`]).
#[derive(Debug, Clone)]
pub struct MorrisResult {
    /// Parameter name.
    pub name: String,
    /// Mean of the elementary effects: close to 0 if the effects cancel
    /// each other out (does not imply low influence — see `mu_star`).
    pub mu: f64,
    /// Mean of the **absolute value** of the elementary effects: an
    /// influence measure robust to sign cancellation, the one most
    /// commonly used in practice to rank parameters.
    pub mu_star: f64,
    /// **Sample** standard deviation (`n-1` denominator, matching Morris
    /// 1991 / Campolongo 2007 / SALib's `ddof=1`) of the elementary
    /// effects: high ⇒ the parameter's effect depends strongly on where in
    /// the space it is measured (nonlinearity and/or interaction with
    /// other parameters). `NaN` with fewer than 2 trajectories (a single
    /// elementary effect has no dispersion to estimate).
    pub sigma: f64,
}

impl MorrisDesign {
    /// Evaluates the model at all points of all trajectories and computes
    /// `mu`/`mu_star`/`sigma` per parameter.
    ///
    /// **Common random numbers.** All `d+1` points of the *same* trajectory
    /// share one seed (derived deterministically from `base_seed` and the
    /// trajectory index via [`child_rng`]); distinct trajectories get
    /// independent, decorrelated seeds. An elementary effect
    /// `EE = (y(x+Δe_i) − y(x)) / Δ` compares two consecutive points of one
    /// trajectory — under CRN both run the identical stochastic realization
    /// save for parameter `i`, so the difference is attributable to that
    /// parameter. With per-point seeds (the earlier bug, the exact mirror
    /// of the Sobol' CRN finding), the EE of an inert parameter is pure
    /// noise, inflating its `mu_star`/`sigma` from ~0 to O(σ_noise/Δ).
    pub fn run<M, B, O>(
        &self,
        base_seed: u64,
        max_steps: u64,
        build: B,
        outcome: O,
    ) -> Vec<MorrisResult>
    where
        M: Model,
        B: Fn(&[f64], u64) -> Simulation<M> + Sync,
        O: Fn(&Simulation<M>) -> f64 + Sync,
    {
        let d = self.specs.len();
        // One seed per trajectory (CRN, see the doc comment above), derived
        // through `child_rng`'s domain-separated chain-hash — the same
        // mechanism `SobolDesign::run` uses for its row seeds.
        let mut seeded: Vec<(Vec<f64>, u64)> = Vec::new();
        for (t, (points, _)) in self.trajectories.iter().enumerate() {
            let traj_seed = child_rng(base_seed, 0, t as u64).next_u64();
            for p in points {
                seeded.push((
                    p.iter()
                        .zip(&self.specs)
                        .map(|(&u, s)| s.scale(u))
                        .collect(),
                    traj_seed,
                ));
            }
        }
        let ys = evaluate_all(&seeded, max_steps, &build, &outcome);

        let mut ee: Vec<Vec<f64>> = vec![Vec::new(); d];
        let mut cursor = 0;
        for (_, order) in &self.trajectories {
            let mut prev_y = ys[cursor];
            for (k, &dim) in order.iter().enumerate() {
                let y = ys[cursor + 1 + k];
                ee[dim].push((y - prev_y) / self.delta);
                prev_y = y;
            }
            cursor += d + 1;
        }

        (0..d)
            .map(|i| {
                let vals = &ee[i];
                let n = vals.len() as f64;
                let mu = vals.iter().sum::<f64>() / n;
                let mu_star = vals.iter().map(|v| v.abs()).sum::<f64>() / n;
                // Sample variance (n-1): the method's standard (Morris
                // 1991, Campolongo 2007, SALib ddof=1). Undefined (NaN)
                // with a single elementary effect.
                let var = if vals.len() < 2 {
                    f64::NAN
                } else {
                    vals.iter().map(|v| (v - mu).powi(2)).sum::<f64>() / (n - 1.0)
                };
                MorrisResult {
                    name: self.specs[i].name.clone(),
                    mu,
                    mu_star,
                    sigma: var.sqrt(),
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::max_base_level;

    /// Regresión (hallazgo de auditoría L4): la fórmula flotante anterior
    /// `((max_level as f64) * (1.0 - delta)).floor()` caía un nivel por
    /// debajo del entero exacto `(levels-2)/2` para
    /// `levels ∈ {30, 88, 150, 182}` (p. ej. con `levels = 30` daba 13 en
    /// vez de 14, excluyendo silenciosamente el nivel base 14 del muestreo
    /// de Morris). La versión entera es exacta por construcción.
    #[test]
    fn max_base_level_es_exacto_para_los_niveles_que_la_version_flotante_fallaba() {
        for &(levels, esperado) in &[
            (2usize, 0u64),
            (3, 0),
            (4, 1),
            (5, 1),
            (30, 14),
            (88, 43),
            (150, 74),
            (182, 90),
        ] {
            assert_eq!(max_base_level(levels), esperado, "levels = {levels}");

            // Coherencia con la definición: desde el nivel base máximo, el
            // paso +delta se queda dentro de [0,1] (con tolerancia de
            // redondeo — la suma flotante 14/29 + 15/29 puede exceder 1.0
            // por un ULP, exactamente el artefacto que rompía la fórmula
            // flotante)...
            let max_level = (levels - 1) as f64;
            let delta = levels as f64 / (2.0 * max_level);
            let x = max_base_level(levels) as f64 / max_level;
            assert!(x + delta <= 1.0 + 1e-12, "levels = {levels}: {x} + {delta}");

            // ...y desde el nivel siguiente ya no (maximalidad).
            let x_next = (max_base_level(levels) + 1) as f64 / max_level;
            assert!(x_next + delta > 1.0 + 1e-12, "levels = {levels}");
        }
    }
}
