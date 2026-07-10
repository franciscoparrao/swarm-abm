//! Dense 2D grid with Moore and Von Neumann neighborhoods.

use std::ops::{Index, IndexMut};

use crate::rng::{SimRng, uniform_below, uniform_usize};

/// Discrete `(x, y)` position on a grid. `x` is the column, `y` is the row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Pos {
    /// Column, in `0..width`.
    pub x: usize,
    /// Row, in `0..height`.
    pub y: usize,
}

impl Pos {
    /// Creates a position.
    #[must_use]
    pub const fn new(x: usize, y: usize) -> Self {
        Self { x, y }
    }
}

impl From<(usize, usize)> for Pos {
    fn from((x, y): (usize, usize)) -> Self {
        Self { x, y }
    }
}

/// Neighborhood type on the grid (radius 1 in v0.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Neighborhood {
    /// 8 adjacent cells (includes diagonals).
    Moore,
    /// 4 adjacent cells (no diagonals).
    VonNeumann,
}

const MOORE: [(i64, i64); 8] = [
    (-1, -1),
    (0, -1),
    (1, -1),
    (-1, 0),
    (1, 0),
    (-1, 1),
    (0, 1),
    (1, 1),
];
const VON_NEUMANN: [(i64, i64); 4] = [(0, -1), (-1, 0), (1, 0), (0, 1)];

/// Allocation-free iterator over neighboring positions (max. 8).
#[derive(Debug, Clone)]
pub struct Neighbors {
    buf: [Pos; 8],
    len: u8,
    i: u8,
}

impl Iterator for Neighbors {
    type Item = Pos;

    fn next(&mut self) -> Option<Pos> {
        if self.i < self.len {
            let p = self.buf[self.i as usize];
            self.i += 1;
            Some(p)
        } else {
            None
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let rem = (self.len - self.i) as usize;
        (rem, Some(rem))
    }
}

impl ExactSizeIterator for Neighbors {}

/// Lazy, allocation-free iterator over neighbors at an **arbitrary
/// radius** (see [`Grid2D::neighbor_positions_r`]). Unlike [`Neighbors`]
/// (fixed buffer of 8, radius 1 only), the number of neighbors grows with
/// `r²` and does not fit in a compile-time fixed-size buffer — each
/// position is computed on the fly in `next()`.
#[derive(Clone)]
pub struct NeighborsR<'g, T> {
    grid: &'g Grid2D<T>,
    pos: Pos,
    kind: Neighborhood,
    r: i64,
    // On a torus, bounded to `min(2r+1, height|width)`: beyond that the
    // wrap would necessarily repeat an already-visited cell — dedup by
    // construction, no `HashSet` needed for that part (same *goal* as
    // `ContinuousSpace::for_each_within`, P1-4, though the mechanism differs:
    // here the bounded window feeds a Manhattan-distance filter for
    // `VonNeumann`, and that filter has to use the true toroidal distance,
    // not the raw window-relative offset — see the `torus_dist` fix in
    // `next()` below). Without torus, always `2r+1` (there is never
    // wrapping; `offset` filters out-of-range values).
    row_span: i64,
    col_span: i64,
    dr: i64,
    dc: i64,
}

impl<'g, T> Iterator for NeighborsR<'g, T> {
    type Item = Pos;

    fn next(&mut self) -> Option<Pos> {
        loop {
            if self.dr >= self.row_span {
                return None;
            }
            let dy = self.dr - self.r;
            let dx = self.dc - self.r;
            self.dc += 1;
            if self.dc >= self.col_span {
                self.dc = 0;
                self.dr += 1;
            }
            let en_radio = match self.kind {
                Neighborhood::Moore => true, // already bounded to [-r,r]×[-r,r]
                Neighborhood::VonNeumann => {
                    // On a torus, `dx`/`dy` are the pre-wrap offsets within
                    // the (possibly bounded) generation window, not
                    // necessarily the offset of least magnitude for that
                    // axis — when `col_span`/`row_span` is bounded to the
                    // grid dimension (`r` large relative to it), the window
                    // can yield a `dx` on one side of the wrap while the
                    // true toroidal distance is shorter going the other way
                    // (bug found by audit: VonNeumann + torus + large `r`
                    // silently under-included neighbors, since a filter on
                    // the window-relative `dx`/`dy` overestimates the real
                    // distance). Recompute the true per-axis toroidal
                    // distance before summing for the Manhattan filter.
                    // Without torus there is no wrap, so `dx.abs()` is
                    // already exact.
                    let torus_dist = |delta: i64, dim: i64| -> i64 {
                        let m = delta.rem_euclid(dim);
                        m.min(dim - m)
                    };
                    let (dx_mag, dy_mag) = if self.grid.torus {
                        (
                            torus_dist(dx, self.grid.width as i64),
                            torus_dist(dy, self.grid.height as i64),
                        )
                    } else {
                        (dx.abs(), dy.abs())
                    };
                    dx_mag + dy_mag <= self.r
                }
            };
            if !en_radio {
                continue;
            }
            if let Some(p) = self.grid.offset(self.pos, dx, dy) {
                if p == self.pos {
                    // The cell itself is not a neighbor. Note: compare the
                    // ALREADY-WRAPPED position, not `(dx,dy) == (0,0)` — with
                    // a large `r` on a small torus, the conceptual offset is
                    // never literally `(0,0)` (it's outside the bounded
                    // range), but after wrapping it can resolve to the same
                    // value as `pos`. Comparing before wrapping would let the
                    // cell itself slip through as if it were just another
                    // neighbor.
                    continue;
                }
                return Some(p);
            }
            // Out of range (no torus): move on to the next offset.
        }
    }
}

/// Dense 2D grid, stored in row-major order.
///
/// With `torus = true` the edges connect (toroidal world, NetLogo style).
/// On toroidal grids with dimension < 3, neighboring positions may repeat
/// (distinct offsets wrap to the same cell); the cell itself is never
/// among them (offsets that wrap back onto the queried position are
/// skipped, same criterion as [`NeighborsR`]).
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Grid2D<T> {
    width: usize,
    height: usize,
    torus: bool,
    cells: Vec<T>,
}

impl<T: Default + Clone> Grid2D<T> {
    /// Creates a grid filled with `T::default()`, without torus.
    ///
    /// # Panics
    /// If `width` or `height` is 0.
    #[must_use]
    pub fn new(width: usize, height: usize) -> Self {
        Self::fill(width, height, T::default())
    }
}

impl<T: Clone> Grid2D<T> {
    /// Creates a grid filled with copies of `value`, without torus.
    ///
    /// # Panics
    /// If `width` or `height` is 0.
    #[must_use]
    pub fn fill(width: usize, height: usize, value: T) -> Self {
        assert!(width > 0 && height > 0, "the grid cannot have dimension 0");
        Self {
            width,
            height,
            torus: false,
            cells: vec![value; width * height],
        }
    }
}

impl<T> Grid2D<T> {
    /// Creates a grid evaluating `f(pos)` per cell, without torus.
    ///
    /// # Panics
    /// If `width` or `height` is 0.
    #[must_use]
    pub fn from_fn(width: usize, height: usize, mut f: impl FnMut(Pos) -> T) -> Self {
        assert!(width > 0 && height > 0, "the grid cannot have dimension 0");
        let mut cells = Vec::with_capacity(width * height);
        for y in 0..height {
            for x in 0..width {
                cells.push(f(Pos::new(x, y)));
            }
        }
        Self {
            width,
            height,
            torus: false,
            cells,
        }
    }

    /// Enables or disables the toroidal topology (builder).
    #[must_use]
    pub fn with_torus(mut self, torus: bool) -> Self {
        self.torus = torus;
        self
    }

    /// Grid width.
    #[must_use]
    pub fn width(&self) -> usize {
        self.width
    }

    /// Grid height.
    #[must_use]
    pub fn height(&self) -> usize {
        self.height
    }

    /// `true` if the grid is toroidal.
    #[must_use]
    pub fn is_torus(&self) -> bool {
        self.torus
    }

    /// `true` if `pos` is inside the grid.
    #[must_use]
    pub fn contains(&self, pos: Pos) -> bool {
        pos.x < self.width && pos.y < self.height
    }

    /// Reference to the cell, or `None` if out of range.
    #[must_use]
    pub fn get(&self, pos: Pos) -> Option<&T> {
        self.contains(pos)
            .then(|| &self.cells[pos.y * self.width + pos.x])
    }

    /// Mutable reference to the cell, or `None` if out of range.
    #[must_use]
    pub fn get_mut(&mut self, pos: Pos) -> Option<&mut T> {
        self.contains(pos)
            .then(|| &mut self.cells[pos.y * self.width + pos.x])
    }

    /// Swaps the contents of two cells.
    ///
    /// # Panics
    /// If either position is out of range.
    pub fn swap(&mut self, a: Pos, b: Pos) {
        assert!(
            self.contains(a) && self.contains(b),
            "swap out of range: {a:?}, {b:?}"
        );
        self.cells
            .swap(a.y * self.width + a.x, b.y * self.width + b.x);
    }

    /// Neighboring positions of `pos` according to the neighborhood, respecting torus/edges.
    #[must_use]
    pub fn neighbor_positions(&self, pos: Pos, neighborhood: Neighborhood) -> Neighbors {
        let offsets: &[(i64, i64)] = match neighborhood {
            Neighborhood::Moore => &MOORE,
            Neighborhood::VonNeumann => &VON_NEUMANN,
        };
        let mut buf = [Pos::new(0, 0); 8];
        let mut len = 0u8;
        for &(dx, dy) in offsets {
            if let Some(p) = self.offset(pos, dx, dy) {
                if p == pos {
                    // On a torus with some axis of dimension 1, a radius-1
                    // offset can wrap back onto `pos` itself; the cell is
                    // not its own neighbor. Compare the ALREADY-WRAPPED
                    // position, the same criterion as `NeighborsR` (audit
                    // L2: without this, a 1×5 torus returned the queried
                    // cell among its own neighbors, and `random_neighbor`
                    // could pick it).
                    continue;
                }
                buf[len as usize] = p;
                len += 1;
            }
        }
        Neighbors { buf, len, i: 0 }
    }

    /// A uniformly random neighboring position, without allocating memory.
    ///
    /// Equivalent to collecting [`neighbor_positions`](Self::neighbor_positions)
    /// and picking a random index (consumes a single RNG draw), but without
    /// the intermediate `Vec` — important in hot loops with many agents.
    /// Returns `None` if the position has no neighbors (1×1 grid, with or
    /// without torus).
    #[must_use]
    pub fn random_neighbor(
        &self,
        pos: Pos,
        neighborhood: Neighborhood,
        rng: &mut SimRng,
    ) -> Option<Pos> {
        let vecinas = self.neighbor_positions(pos, neighborhood);
        let n = vecinas.len();
        if n == 0 {
            return None;
        }
        Some(vecinas.buf[uniform_usize(rng, n)])
    }

    /// Like [`neighbor_positions`](Self::neighbor_positions) but at an
    /// **arbitrary radius** `r` (`r=1` yields the same set as
    /// `neighbor_positions`, except for the order). Without allocating memory:
    /// the number of neighbors scales with `r²`, so it does not fit in a
    /// fixed buffer — it uses a lazy iterator ([`NeighborsR`]) instead of a
    /// `Vec`.
    #[must_use]
    pub fn neighbor_positions_r(
        &self,
        pos: Pos,
        neighborhood: Neighborhood,
        r: usize,
    ) -> NeighborsR<'_, T> {
        let r = r as i64;
        let span = 2 * r + 1;
        let row_span = if self.torus {
            span.min(self.height as i64)
        } else {
            span
        };
        let col_span = if self.torus {
            span.min(self.width as i64)
        } else {
            span
        };
        NeighborsR {
            grid: self,
            pos,
            kind: neighborhood,
            r,
            row_span,
            col_span,
            dr: 0,
            dc: 0,
        }
    }

    /// Like [`random_neighbor`](Self::random_neighbor) but at radius `r`.
    /// No fixed-size buffer is possible (see
    /// [`neighbor_positions_r`](Self::neighbor_positions_r)), so instead of
    /// "count and pick an index" it uses **reservoir sampling**
    /// (Algorithm R): it walks the lazy iterator once, without allocating
    /// memory, and ends up with a candidate uniformly chosen among all
    /// neighbors — at the cost of one RNG draw per visited neighbor, not a
    /// single one as in `random_neighbor` (which can afford a fixed buffer).
    #[must_use]
    pub fn random_neighbor_r(
        &self,
        pos: Pos,
        neighborhood: Neighborhood,
        r: usize,
        rng: &mut SimRng,
    ) -> Option<Pos> {
        let mut chosen = None;
        let mut count: u64 = 0;
        for p in self.neighbor_positions_r(pos, neighborhood, r) {
            count += 1;
            if uniform_below(rng, count) == 0 {
                chosen = Some(p);
            }
        }
        chosen
    }

    /// Iterates over `(position, &cell)` for the neighbors of `pos`.
    pub fn neighbors(
        &self,
        pos: Pos,
        neighborhood: Neighborhood,
    ) -> impl Iterator<Item = (Pos, &T)> {
        self.neighbor_positions(pos, neighborhood)
            .map(move |p| (p, &self.cells[p.y * self.width + p.x]))
    }

    /// Like [`neighbors`](Self::neighbors) but at an arbitrary radius `r`
    /// (see [`neighbor_positions_r`](Self::neighbor_positions_r)).
    pub fn neighbors_r(
        &self,
        pos: Pos,
        neighborhood: Neighborhood,
        r: usize,
    ) -> impl Iterator<Item = (Pos, &T)> {
        self.neighbor_positions_r(pos, neighborhood, r)
            .map(move |p| (p, &self.cells[p.y * self.width + p.x]))
    }

    /// Iterates over all cells as `(position, &cell)`, row by row.
    pub fn iter(&self) -> impl Iterator<Item = (Pos, &T)> {
        self.cells
            .iter()
            .enumerate()
            .map(|(i, c)| (Pos::new(i % self.width, i / self.width), c))
    }

    /// Iterates over all cells as `(position, &mut cell)`, row by row.
    pub fn iter_mut(&mut self) -> impl Iterator<Item = (Pos, &mut T)> {
        let width = self.width;
        self.cells
            .iter_mut()
            .enumerate()
            .map(move |(i, c)| (Pos::new(i % width, i / width), c))
    }

    /// Shifts `pos` by `(dx, dy)`: with torus it wraps, without torus it
    /// returns `None` if it goes outside the grid.
    fn offset(&self, pos: Pos, dx: i64, dy: i64) -> Option<Pos> {
        let (x, y) = (pos.x as i64 + dx, pos.y as i64 + dy);
        if self.torus {
            let (w, h) = (self.width as i64, self.height as i64);
            Some(Pos::new(x.rem_euclid(w) as usize, y.rem_euclid(h) as usize))
        } else if x >= 0 && y >= 0 && (x as usize) < self.width && (y as usize) < self.height {
            Some(Pos::new(x as usize, y as usize))
        } else {
            None
        }
    }
}

impl Grid2D<f64> {
    /// Diffuses the scalar field one step (NetLogo `diffuse` semantics):
    /// each cell distributes the fraction `rate` of its value in equal parts
    /// among its 8 (Moore) or 4 (Von Neumann) potential neighbors. Without
    /// torus, the portion that would leave the grid stays in the cell. Total
    /// mass is always conserved.
    ///
    /// The update is simultaneous (internal double buffer): the result does
    /// not depend on traversal order.
    ///
    /// # Panics
    /// If `rate` is not in `[0, 1]`.
    pub fn diffuse(&mut self, rate: f64, neighborhood: Neighborhood) {
        assert!(
            (0.0..=1.0).contains(&rate),
            "diffusion rate out of [0, 1]: {rate}"
        );
        let divisor = match neighborhood {
            Neighborhood::Moore => 8.0,
            Neighborhood::VonNeumann => 4.0,
        };
        let mut next = vec![0.0; self.cells.len()];
        for (i, &value) in self.cells.iter().enumerate() {
            let pos = Pos::new(i % self.width, i / self.width);
            let share = value * rate / divisor;
            let mut given = 0.0;
            for p in self.neighbor_positions(pos, neighborhood) {
                next[p.y * self.width + p.x] += share;
                given += share;
            }
            next[i] += value - given;
        }
        self.cells = next;
    }

    /// Sum of all cells (total mass of the field).
    #[must_use]
    pub fn total(&self) -> f64 {
        self.cells.iter().sum()
    }
}

impl<T> Index<Pos> for Grid2D<T> {
    type Output = T;

    /// # Panics
    /// If `pos` is out of range (use [`Grid2D::get`] for the safe variant).
    fn index(&self, pos: Pos) -> &T {
        match self.get(pos) {
            Some(c) => c,
            None => panic!(
                "position {pos:?} out of grid bounds {}x{}",
                self.width, self.height
            ),
        }
    }
}

impl<T> IndexMut<Pos> for Grid2D<T> {
    fn index_mut(&mut self, pos: Pos) -> &mut T {
        let (w, h) = (self.width, self.height);
        match self.get_mut(pos) {
            Some(c) => c,
            None => panic!("position {pos:?} out of grid bounds {w}x{h}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn posiciones(g: &Grid2D<u8>, p: Pos, n: Neighborhood) -> HashSet<Pos> {
        g.neighbor_positions(p, n).collect()
    }

    #[test]
    fn moore_centro_son_8() {
        let g: Grid2D<u8> = Grid2D::new(5, 5);
        let v = posiciones(&g, Pos::new(2, 2), Neighborhood::Moore);
        assert_eq!(v.len(), 8);
        assert!(v.contains(&Pos::new(1, 1)));
        assert!(v.contains(&Pos::new(3, 3)));
        assert!(
            !v.contains(&Pos::new(2, 2)),
            "the cell itself is not a neighbor"
        );
    }

    #[test]
    fn von_neumann_centro_son_4() {
        let g: Grid2D<u8> = Grid2D::new(5, 5);
        let v = posiciones(&g, Pos::new(2, 2), Neighborhood::VonNeumann);
        assert_eq!(
            v,
            HashSet::from([
                Pos::new(2, 1),
                Pos::new(1, 2),
                Pos::new(3, 2),
                Pos::new(2, 3)
            ])
        );
    }

    #[test]
    fn esquina_sin_torus_recorta() {
        let g: Grid2D<u8> = Grid2D::new(5, 5);
        assert_eq!(posiciones(&g, Pos::new(0, 0), Neighborhood::Moore).len(), 3);
        assert_eq!(
            posiciones(&g, Pos::new(4, 4), Neighborhood::VonNeumann).len(),
            2
        );
    }

    #[test]
    fn esquina_con_torus_envuelve() {
        let g: Grid2D<u8> = Grid2D::new(5, 5).with_torus(true);
        let v = posiciones(&g, Pos::new(0, 0), Neighborhood::Moore);
        assert_eq!(v.len(), 8);
        assert!(v.contains(&Pos::new(4, 4)));
        assert!(v.contains(&Pos::new(4, 0)));
        assert!(v.contains(&Pos::new(0, 4)));
    }

    #[test]
    fn index_y_get() {
        let mut g: Grid2D<u32> = Grid2D::new(3, 2);
        g[Pos::new(2, 1)] = 9;
        assert_eq!(g[Pos::new(2, 1)], 9);
        assert_eq!(g.get(Pos::new(3, 0)), None);
        assert_eq!(g.get(Pos::new(0, 2)), None);
    }

    #[test]
    fn iter_recorre_todo_en_orden() {
        let g = Grid2D::from_fn(3, 2, |p| p.x + 10 * p.y);
        let todo: Vec<(Pos, usize)> = g.iter().map(|(p, &v)| (p, v)).collect();
        assert_eq!(todo.len(), 6);
        assert_eq!(todo[0], (Pos::new(0, 0), 0));
        assert_eq!(todo[5], (Pos::new(2, 1), 12));
    }

    #[test]
    fn random_neighbor_es_vecina_valida_y_determinista() {
        use crate::rng::rng_from_seed;
        let g: Grid2D<u8> = Grid2D::new(5, 5).with_torus(true);
        let centro = Pos::new(2, 2);
        let vecinas: HashSet<Pos> = g.neighbor_positions(centro, Neighborhood::Moore).collect();

        let mut rng = rng_from_seed(3);
        for _ in 0..100 {
            let p = g
                .random_neighbor(centro, Neighborhood::Moore, &mut rng)
                .expect("there are neighbors");
            assert!(vecinas.contains(&p));
        }

        // Same seed, same sequence of choices.
        let secuencia = |seed| {
            let mut rng = rng_from_seed(seed);
            (0..20)
                .map(|_| g.random_neighbor(centro, Neighborhood::Moore, &mut rng))
                .collect::<Vec<_>>()
        };
        assert_eq!(secuencia(9), secuencia(9));
    }

    #[test]
    fn neighbor_positions_r_uno_coincide_con_neighbor_positions() {
        let g: Grid2D<u8> = Grid2D::new(7, 7).with_torus(true);
        let centro = Pos::new(3, 3);
        for nh in [Neighborhood::Moore, Neighborhood::VonNeumann] {
            let r1: HashSet<Pos> = g.neighbor_positions_r(centro, nh, 1).collect();
            let base: HashSet<Pos> = g.neighbor_positions(centro, nh).collect();
            assert_eq!(r1, base, "{nh:?}: r=1 must match neighbor_positions");
        }
    }

    #[test]
    fn neighbor_positions_r_moore_radio_2_son_24() {
        // Moore r=2 without torus, far from the edge: (2r+1)^2 - 1 = 24.
        let g: Grid2D<u8> = Grid2D::new(11, 11);
        let v: HashSet<Pos> = g
            .neighbor_positions_r(Pos::new(5, 5), Neighborhood::Moore, 2)
            .collect();
        assert_eq!(v.len(), 24);
        assert!(v.contains(&Pos::new(3, 3)));
        assert!(v.contains(&Pos::new(7, 7)));
        assert!(
            !v.contains(&Pos::new(5, 5)),
            "the cell itself is not a neighbor"
        );
    }

    #[test]
    fn neighbor_positions_r_von_neumann_radio_2_son_12() {
        // Von Neumann r=2: Manhattan diamond, 4r^2+4r... specifically 12
        // cells for r=2 (perimeter of radii 1 and 2: 4 + 8).
        let g: Grid2D<u8> = Grid2D::new(11, 11);
        let v: HashSet<Pos> = g
            .neighbor_positions_r(Pos::new(5, 5), Neighborhood::VonNeumann, 2)
            .collect();
        assert_eq!(v.len(), 12);
        assert!(v.contains(&Pos::new(5, 3))); // Manhattan distance 2
        assert!(!v.contains(&Pos::new(3, 3))); // Manhattan distance 4, out of range
    }

    #[test]
    fn neighbor_positions_r_torus_radio_grande_no_duplica() {
        // Regression (same fix as ContinuousSpace, P1-4): a radius covering
        // the whole torus must not visit any cell twice.
        let g: Grid2D<u8> = Grid2D::new(5, 5).with_torus(true);
        let v: Vec<Pos> = g
            .neighbor_positions_r(Pos::new(2, 2), Neighborhood::Moore, 10)
            .collect();
        assert_eq!(v.len(), 24, "the 25 cells of the torus minus itself");
        let unicos: HashSet<Pos> = v.iter().copied().collect();
        assert_eq!(unicos.len(), 24, "no duplicates");
    }

    #[test]
    fn neighbor_positions_r_von_neumann_torus_radio_grande_no_sub_incluye() {
        // Regression (audit finding, distinct from the Moore test above):
        // with VonNeumann on a torus and `2r+1 > dim` on some axis, the
        // Manhattan filter used to run against the pre-wrap, window-relative
        // `dx`/`dy` instead of the true toroidal distance, silently
        // UNDER-including neighbors that were, in fact, within radius
        // (checked concretely: a 5x5 torus at (0,0) with r=3 returned 16
        // instead of the true 20, missing (2,1)/(1,2)/(4,2)/(2,4)).
        let g: Grid2D<u8> = Grid2D::new(5, 5).with_torus(true);
        let centro = Pos::new(0, 0);
        let r = 3i64;
        let got: HashSet<Pos> = g
            .neighbor_positions_r(centro, Neighborhood::VonNeumann, 3)
            .collect();

        let torus_dist = |a: i64, dim: i64| {
            let m = a.rem_euclid(dim);
            m.min(dim - m)
        };
        let mut truth: HashSet<Pos> = HashSet::new();
        for y in 0..5i64 {
            for x in 0..5i64 {
                if (x, y) == (0, 0) {
                    continue;
                }
                if torus_dist(x, 5) + torus_dist(y, 5) <= r {
                    truth.insert(Pos::new(x as usize, y as usize));
                }
            }
        }
        assert_eq!(got, truth);
        assert_eq!(got.len(), 20);
    }

    #[test]
    fn random_neighbor_r_es_vecina_valida_y_determinista() {
        use crate::rng::rng_from_seed;
        let g: Grid2D<u8> = Grid2D::new(9, 9).with_torus(true);
        let centro = Pos::new(4, 4);
        let vecinas: HashSet<Pos> = g
            .neighbor_positions_r(centro, Neighborhood::Moore, 2)
            .collect();

        let mut rng = rng_from_seed(3);
        for _ in 0..100 {
            let p = g
                .random_neighbor_r(centro, Neighborhood::Moore, 2, &mut rng)
                .expect("there are neighbors");
            assert!(vecinas.contains(&p));
        }

        let secuencia = |seed| {
            let mut rng = rng_from_seed(seed);
            (0..20)
                .map(|_| g.random_neighbor_r(centro, Neighborhood::Moore, 2, &mut rng))
                .collect::<Vec<_>>()
        };
        assert_eq!(secuencia(9), secuencia(9));
    }

    #[test]
    fn diffuse_conserva_masa() {
        for torus in [false, true] {
            let mut g: Grid2D<f64> = Grid2D::new(7, 5).with_torus(torus);
            g[Pos::new(3, 2)] = 100.0;
            g[Pos::new(0, 0)] = 50.0;
            for _ in 0..20 {
                g.diffuse(0.5, Neighborhood::Moore);
            }
            assert!((g.total() - 150.0).abs() < 1e-9, "torus={torus}");
        }
    }

    #[test]
    fn diffuse_reparte_a_vecinas() {
        let mut g: Grid2D<f64> = Grid2D::new(5, 5).with_torus(true);
        let centro = Pos::new(2, 2);
        g[centro] = 80.0;
        g.diffuse(0.5, Neighborhood::Moore);
        // The center retains 1 - rate; each Moore neighbor receives rate/8.
        assert!((g[centro] - 40.0).abs() < 1e-12);
        for p in g.neighbor_positions(centro, Neighborhood::Moore) {
            assert!((g[p] - 5.0).abs() < 1e-12);
        }
    }

    #[test]
    fn diffuse_sin_torus_borde_retiene() {
        let mut g: Grid2D<f64> = Grid2D::new(5, 5);
        let esquina = Pos::new(0, 0);
        g[esquina] = 80.0;
        g.diffuse(0.5, Neighborhood::Moore);
        // The corner only has 3 neighbors: it gives away 3·(rate/8) and retains the rest.
        assert!((g[esquina] - 65.0).abs() < 1e-12);
        assert!((g.total() - 80.0).abs() < 1e-12);
    }

    #[test]
    #[should_panic(expected = "out of [0, 1]")]
    fn diffuse_rate_invalido_panic() {
        let mut g: Grid2D<f64> = Grid2D::new(3, 3);
        g.diffuse(1.5, Neighborhood::Moore);
    }

    #[test]
    fn iter_mut_modifica_celdas() {
        let mut g = Grid2D::from_fn(3, 2, |p| p.x as f64);
        for (p, v) in g.iter_mut() {
            *v += p.y as f64 * 10.0;
        }
        assert_eq!(g[Pos::new(2, 1)], 12.0);
    }

    #[test]
    #[should_panic(expected = "out of grid bounds")]
    fn index_fuera_de_rango_panic() {
        let g: Grid2D<u8> = Grid2D::new(2, 2);
        let _ = g[Pos::new(5, 5)];
    }

    #[test]
    fn torus_degenerado_no_incluye_la_propia_celda() {
        // Regression (audit L2): on a 1×5 torus, the ±1 offsets along the
        // degenerate axis wrap back onto the queried cell, and
        // `neighbor_positions` used to include it (twice). It must be
        // skipped, matching both the "the cell itself is not a neighbor"
        // contract and the set `neighbor_positions_r` yields at r=1.
        let g: Grid2D<u8> = Grid2D::new(1, 5).with_torus(true);
        let centro = Pos::new(0, 2);
        let v = posiciones(&g, centro, Neighborhood::VonNeumann);
        assert_eq!(v, HashSet::from([Pos::new(0, 1), Pos::new(0, 3)]));
        let r1: HashSet<Pos> = g
            .neighbor_positions_r(centro, Neighborhood::VonNeumann, 1)
            .collect();
        assert_eq!(v, r1, "r=1 must match neighbor_positions");
    }

    #[test]
    fn torus_uno_por_uno_no_tiene_vecinas() {
        use crate::rng::rng_from_seed;
        let g: Grid2D<u8> = Grid2D::new(1, 1).with_torus(true);
        let p = Pos::new(0, 0);
        for nh in [Neighborhood::Moore, Neighborhood::VonNeumann] {
            assert_eq!(g.neighbor_positions(p, nh).len(), 0, "{nh:?}");
            assert_eq!(g.random_neighbor(p, nh, &mut rng_from_seed(1)), None);
        }
    }

    #[test]
    fn random_neighbor_en_torus_degenerado_nunca_devuelve_la_propia_celda() {
        use crate::rng::rng_from_seed;
        let g: Grid2D<u8> = Grid2D::new(1, 5).with_torus(true);
        let centro = Pos::new(0, 2);
        let mut rng = rng_from_seed(3);
        for _ in 0..200 {
            let p = g
                .random_neighbor(centro, Neighborhood::VonNeumann, &mut rng)
                .expect("there are neighbors");
            assert_ne!(p, centro, "the cell itself is not a neighbor");
            assert!([Pos::new(0, 1), Pos::new(0, 3)].contains(&p));
        }
    }
}
