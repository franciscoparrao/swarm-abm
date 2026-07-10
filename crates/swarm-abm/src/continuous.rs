//! **Continuous** 2D space for off-lattice ABMs: movement, *flocking*,
//! particles — where position is real-valued (`f64`), not a cell.
//!
//! This is the engine's third space, alongside [`Grid2D`](crate::grid::Grid2D)
//! (discrete geometric) and [`Graph`](crate::graph::Graph) (topological). The
//! neighborhood is **radius-based**, and queries use a *spatial hash* (a grid
//! of buckets) to avoid O(n²): after moving the points,
//! [`reindex`](ContinuousSpace::reindex) is called once per step, and then
//! [`for_each_within`](ContinuousSpace::for_each_within) walks only the
//! cells that cover the search disk.
//!
//! Points live in a **generational arena** (same design as
//! [`AgentSet`](crate::agent::AgentSet)): [`PointId`] is `{index,
//! generation}`, [`remove`](ContinuousSpace::remove) frees the slot for a
//! future [`add`](ContinuousSpace::add) without leaving permanent gaps, and a
//! `PointId` issued before a `remove` never resolves to anything again
//! (the ABA problem is avoided by construction).

use std::ops::{Add, Index, IndexMut, Mul, Neg, Sub};

/// 2D vector / point with the usual operations.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Vec2 {
    /// Horizontal coordinate.
    pub x: f64,
    /// Vertical coordinate.
    pub y: f64,
}

impl Vec2 {
    /// The zero vector.
    pub const ZERO: Vec2 = Vec2 { x: 0.0, y: 0.0 };

    /// Creates a vector.
    #[must_use]
    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    /// Euclidean norm.
    #[must_use]
    pub fn length(self) -> f64 {
        self.length_sq().sqrt()
    }

    /// Squared norm (avoids the square root when only comparing).
    #[must_use]
    pub fn length_sq(self) -> f64 {
        self.x * self.x + self.y * self.y
    }

    /// Unit vector; the zero vector if the norm is 0.
    #[must_use]
    pub fn normalize_or_zero(self) -> Vec2 {
        let len = self.length();
        if len > 0.0 {
            self * (1.0 / len)
        } else {
            Vec2::ZERO
        }
    }

    /// Clamps the norm to a maximum (leaves the direction intact).
    #[must_use]
    pub fn clamp_length(self, max: f64) -> Vec2 {
        let len = self.length();
        if len > max && len > 0.0 {
            self * (max / len)
        } else {
            self
        }
    }
}

impl Add for Vec2 {
    type Output = Vec2;
    fn add(self, o: Vec2) -> Vec2 {
        Vec2::new(self.x + o.x, self.y + o.y)
    }
}
impl Sub for Vec2 {
    type Output = Vec2;
    fn sub(self, o: Vec2) -> Vec2 {
        Vec2::new(self.x - o.x, self.y - o.y)
    }
}
impl Mul<f64> for Vec2 {
    type Output = Vec2;
    fn mul(self, s: f64) -> Vec2 {
        Vec2::new(self.x * s, self.y * s)
    }
}
impl Neg for Vec2 {
    type Output = Vec2;
    fn neg(self) -> Vec2 {
        Vec2::new(-self.x, -self.y)
    }
}

/// Stable identifier for a point in the space.
///
/// Generational arena, same as [`AgentId`](crate::agent::AgentId): a
/// `PointId` issued before a [`remove`](ContinuousSpace::remove) never
/// resolves to anything again, neither to the original (removed) point nor
/// to a new one that reuses its slot (different generation).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PointId {
    index: u32,
    generation: u32,
}

impl PointId {
    /// Internal slot index (useful for indexing your own array *while* the
    /// point is still alive). Not a long-term stable key: after a
    /// `remove`, the index can be reused for a different point — see
    /// the equivalent note on
    /// [`AgentId::as_usize`](crate::agent::AgentId::as_usize).
    #[must_use]
    pub fn as_usize(self) -> usize {
        self.index as usize
    }
}

#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
enum PointState<T> {
    Occupied { pos: Vec2, data: T },
    Free { next: Option<u32> },
}

#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
struct Slot<T> {
    generation: u32,
    state: PointState<T>,
}

/// 2D continuous space `[0,width) × [0,height)` with one `T` payload per
/// point and a spatial hash index. With `torus = true` the edges wrap
/// around and distances are the shortest ones under wrapping.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ContinuousSpace<T> {
    width: f64,
    height: f64,
    torus: bool,
    cell: f64,
    cols: usize,
    rows: usize,
    slots: Vec<Slot<T>>,
    /// Head of the free list of vacant slots (see
    /// [`AgentSet`](crate::agent::AgentSet)).
    free_head: Option<u32>,
    live: usize,
    /// Spatial index in a **flat** layout (counting sort), rebuilt by
    /// [`reindex`](Self::reindex): `bucket_start[c]..bucket_start[c+1]` is
    /// the range of `bucket_points` that falls in cell `c`. Replaces a
    /// `Vec<Vec<PointId>>` (one heap-allocated `Vec` per cell, with the
    /// churn that implies) with two contiguous `Vec`s reused across calls.
    bucket_start: Vec<u32>,
    bucket_points: Vec<u32>,
    /// Scratch buffer reused inside `reindex` (per-cell write cursor); it
    /// carries no state of its own between calls.
    cursor: Vec<u32>,
}

impl<T> ContinuousSpace<T> {
    /// Creates an empty `width × height` space with hash cells of side
    /// `cell_size` (use a value ≈ the typical neighborhood radius for
    /// efficiency).
    ///
    /// # Panics
    /// If any dimension or `cell_size` is not positive.
    #[must_use]
    pub fn new(width: f64, height: f64, cell_size: f64) -> Self {
        assert!(
            width > 0.0 && height > 0.0 && cell_size > 0.0,
            "positive dimensions"
        );
        let cols = ((width / cell_size).ceil() as usize).max(1);
        let rows = ((height / cell_size).ceil() as usize).max(1);
        Self {
            width,
            height,
            torus: false,
            cell: cell_size,
            cols,
            rows,
            slots: Vec::new(),
            free_head: None,
            live: 0,
            bucket_start: vec![0; cols * rows + 1],
            bucket_points: Vec::new(),
            cursor: Vec::new(),
        }
    }

    /// Enables toroidal topology (builder).
    #[must_use]
    pub fn with_torus(mut self, torus: bool) -> Self {
        self.torus = torus;
        self
    }

    /// Adds a point and returns its identifier. Reuses the most recently
    /// freed vacant slot, if any (LIFO free list). Requires
    /// [`reindex`](Self::reindex) before the next query.
    pub fn add(&mut self, pos: Vec2, value: T) -> PointId {
        let pos = self.wrap(pos);
        if let Some(index) = self.free_head {
            let slot = &mut self.slots[index as usize];
            let PointState::Free { next } = slot.state else {
                unreachable!("free_head always points to a Free slot")
            };
            self.free_head = next;
            slot.state = PointState::Occupied { pos, data: value };
            self.live += 1;
            return PointId {
                index,
                generation: slot.generation,
            };
        }
        // The LIFO free list recycles indices, so the limit is the peak
        // number of concurrent slots, not the historical insert count.
        let index =
            u32::try_from(self.slots.len()).expect("more than u32::MAX concurrent point slots");
        self.slots.push(Slot {
            generation: 0,
            state: PointState::Occupied { pos, data: value },
        });
        self.live += 1;
        PointId {
            index,
            generation: 0,
        }
    }

    /// Removes a point and returns its data, freeing the slot for a future
    /// `add`. `None` if `id` is no longer valid (removed, or stale
    /// generation). Requires [`reindex`](Self::reindex) before the next
    /// query so the spatial index stops referencing it.
    pub fn remove(&mut self, id: PointId) -> Option<T> {
        let slot = self.slots.get_mut(id.index as usize)?;
        if slot.generation != id.generation {
            return None;
        }
        match &mut slot.state {
            PointState::Occupied { .. } => {
                let PointState::Occupied { data, .. } = std::mem::replace(
                    &mut slot.state,
                    PointState::Free {
                        next: self.free_head,
                    },
                ) else {
                    unreachable!()
                };
                slot.generation = slot.generation.wrapping_add(1);
                self.free_head = Some(id.index);
                self.live -= 1;
                Some(data)
            }
            PointState::Free { .. } => None,
        }
    }

    /// Number of live points.
    #[must_use]
    pub fn len(&self) -> usize {
        self.live
    }

    /// `true` if there are no live points.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.live == 0
    }

    /// Identifiers of all live points.
    pub fn point_ids(&self) -> impl Iterator<Item = PointId> + '_ {
        self.slots.iter().enumerate().filter_map(|(i, slot)| {
            matches!(slot.state, PointState::Occupied { .. }).then_some(PointId {
                index: i as u32,
                generation: slot.generation,
            })
        })
    }

    /// Position of the point.
    ///
    /// # Panics
    /// If `id` does not refer to a live point (removed, or stale
    /// generation).
    #[must_use]
    pub fn position(&self, id: PointId) -> Vec2 {
        match self.slots.get(id.index as usize) {
            Some(slot) if slot.generation == id.generation => match &slot.state {
                PointState::Occupied { pos, .. } => *pos,
                PointState::Free { .. } => panic!("Invalid or removed PointId: {id:?}"),
            },
            _ => panic!("Invalid or removed PointId: {id:?}"),
        }
    }

    /// Moves a point (wraps if toroidal). The index does not reflect the
    /// change until the next [`reindex`](Self::reindex).
    ///
    /// # Panics
    /// If `id` does not refer to a live point.
    pub fn set_pos(&mut self, id: PointId, new_pos: Vec2) {
        let wrapped = self.wrap(new_pos);
        match self.slots.get_mut(id.index as usize) {
            Some(slot) if slot.generation == id.generation => match &mut slot.state {
                PointState::Occupied { pos, .. } => *pos = wrapped,
                PointState::Free { .. } => panic!("Invalid or removed PointId: {id:?}"),
            },
            _ => panic!("Invalid or removed PointId: {id:?}"),
        }
    }

    /// Rebuilds the spatial index from the current positions. Call once
    /// per step, after moving (or adding/removing) points — typically in
    /// `before_step`.
    ///
    /// Flat layout via a two-pass O(n) *counting sort*, with no
    /// allocations: `bucket_start`/`bucket_points`/`cursor` are buffers
    /// reused across calls (they grow only if the population or the grid
    /// grows).
    pub fn reindex(&mut self) {
        let n_buckets = self.cols * self.rows;
        self.bucket_start.clear();
        self.bucket_start.resize(n_buckets + 1, 0);
        for slot in &self.slots {
            if let PointState::Occupied { pos, .. } = &slot.state {
                let idx = self.cell_index(*pos);
                self.bucket_start[idx + 1] += 1;
            }
        }
        for i in 0..n_buckets {
            self.bucket_start[i + 1] += self.bucket_start[i];
        }
        self.cursor.clear();
        self.cursor
            .extend_from_slice(&self.bucket_start[..n_buckets]);
        self.bucket_points.clear();
        self.bucket_points.resize(self.live, 0);
        for (i, slot) in self.slots.iter().enumerate() {
            if let PointState::Occupied { pos, .. } = &slot.state {
                let idx = self.cell_index(*pos);
                let dest = self.cursor[idx];
                self.bucket_points[dest as usize] = i as u32;
                self.cursor[idx] += 1;
            }
        }
    }

    /// Euclidean distance between two positions (the shortest one, if
    /// toroidal).
    #[must_use]
    pub fn distance(&self, a: Vec2, b: Vec2) -> f64 {
        let mut dx = (a.x - b.x).abs();
        let mut dy = (a.y - b.y).abs();
        if self.torus {
            dx = dx.min(self.width - dx);
            dy = dy.min(self.height - dy);
        }
        (dx * dx + dy * dy).sqrt()
    }

    /// Shortest vector from `from` to `to` (crosses the edge if that is
    /// shorter in a toroidal space). Useful for consistent
    /// `cohesion`/`separation` behaviors.
    #[must_use]
    pub fn delta(&self, from: Vec2, to: Vec2) -> Vec2 {
        let mut d = to - from;
        if self.torus {
            if d.x.abs() > self.width / 2.0 {
                d.x -= self.width * d.x.signum();
            }
            if d.y.abs() > self.height / 2.0 {
                d.y -= self.height * d.y.signum();
            }
        }
        d
    }

    /// Wraps/clamps a position to the space's domain.
    #[must_use]
    pub fn wrap(&self, p: Vec2) -> Vec2 {
        if self.torus {
            // `rem_euclid` can round to exactly `width`/`height` when the
            // input is a negative residue of magnitude smaller than half an
            // ulp of the modulus (e.g. `(-1e-15).rem_euclid(100.0) ==
            // 100.0`), which falls outside the documented half-open domain
            // `[0, width) × [0, height)`. Fold that edge back to `0.0` —
            // the toroidally equivalent point (NOT `next_down`, which would
            // be geometrically wrong here).
            let mut x = p.x.rem_euclid(self.width);
            if x >= self.width {
                x = 0.0;
            }
            let mut y = p.y.rem_euclid(self.height);
            if y >= self.height {
                y = 0.0;
            }
            Vec2::new(x, y)
        } else {
            // The domain is half-open `[0, width) × [0, height)`;
            // clamping to `width` would include the edge, breaking the
            // invariant that `distance`/`delta` and user code assume it is
            // open.
            Vec2::new(
                p.x.clamp(0.0, self.width.next_down()),
                p.y.clamp(0.0, self.height.next_down()),
            )
        }
    }

    /// Invokes `f(id, position, &data, distance)` for each point within
    /// distance `≤ radius` of `center`. Walks only the hash cells that
    /// cover the search disk, with no deduplicating `HashSet`: in the
    /// toroidal case, the number of row/column offsets is bounded to at
    /// most `rows`/`cols` (beyond that, wrapping would necessarily repeat
    /// a cell), so each cell is visited **at most once** by construction.
    pub fn for_each_within(
        &self,
        center: Vec2,
        radius: f64,
        mut f: impl FnMut(PointId, Vec2, &T, f64),
    ) {
        // A non-finite (`INFINITY`) or astronomically large radius makes
        // `(radius / cell).ceil() as i64` saturate to `i64::MAX`, and the
        // `+ 1` then overflows: panic in debug, and in release the wrap
        // silently visits a single bucket (empty result). Clamp to the
        // grid size first — more cells than `max(rows, cols)` can never be
        // needed (the span bounding below already covers the whole grid at
        // that point), so `INFINITY` naturally means "all buckets".
        let cr = ((radius / self.cell).ceil() as i64).min(self.rows.max(self.cols) as i64) + 1;
        let (cc, cr_row) = self.cell_coords(center);

        if self.torus {
            let row_span = (2 * cr + 1).min(self.rows as i64);
            let col_span = (2 * cr + 1).min(self.cols as i64);
            let row0 = cr_row as i64 - cr;
            let col0 = cc as i64 - cr;
            for dr in 0..row_span {
                let row = (row0 + dr).rem_euclid(self.rows as i64) as usize;
                for dc in 0..col_span {
                    let col = (col0 + dc).rem_euclid(self.cols as i64) as usize;
                    self.visit_bucket(row, col, center, radius, &mut f);
                }
            }
        } else {
            let row_lo = (cr_row as i64 - cr).max(0);
            let row_hi = (cr_row as i64 + cr).min(self.rows as i64 - 1);
            let col_lo = (cc as i64 - cr).max(0);
            let col_hi = (cc as i64 + cr).min(self.cols as i64 - 1);
            if row_lo > row_hi || col_lo > col_hi {
                return;
            }
            for row in row_lo..=row_hi {
                for col in col_lo..=col_hi {
                    self.visit_bucket(row as usize, col as usize, center, radius, &mut f);
                }
            }
        }
    }

    /// Visits the points of an index cell, filtering by radius.
    fn visit_bucket(
        &self,
        row: usize,
        col: usize,
        center: Vec2,
        radius: f64,
        f: &mut impl FnMut(PointId, Vec2, &T, f64),
    ) {
        let idx = row * self.cols + col;
        let start = self.bucket_start[idx] as usize;
        let end = self.bucket_start[idx + 1] as usize;
        for &slot_idx in &self.bucket_points[start..end] {
            let slot = &self.slots[slot_idx as usize];
            if let PointState::Occupied { pos, data } = &slot.state {
                let d = self.distance(center, *pos);
                if d <= radius {
                    let id = PointId {
                        index: slot_idx,
                        generation: slot.generation,
                    };
                    f(id, *pos, data, d);
                }
            }
        }
    }

    /// Points within distance `≤ radius` of `center`, as `(id, distance)`.
    /// Convenience method: in hot paths prefer
    /// [`for_each_within`](Self::for_each_within), which allocates no
    /// memory.
    #[must_use]
    pub fn within(&self, center: Vec2, radius: f64) -> Vec<(PointId, f64)> {
        let mut out = Vec::new();
        self.for_each_within(center, radius, |id, _, _, d| out.push((id, d)));
        out
    }

    fn cell_coords(&self, p: Vec2) -> (usize, usize) {
        let col = ((p.x / self.cell) as usize).min(self.cols - 1);
        let row = ((p.y / self.cell) as usize).min(self.rows - 1);
        (col, row)
    }

    fn cell_index(&self, p: Vec2) -> usize {
        let (col, row) = self.cell_coords(p);
        row * self.cols + col
    }
}

impl<T> Index<PointId> for ContinuousSpace<T> {
    type Output = T;

    /// # Panics
    /// If `id` does not refer to a live point.
    fn index(&self, id: PointId) -> &T {
        match self.slots.get(id.index as usize) {
            Some(slot) if slot.generation == id.generation => match &slot.state {
                PointState::Occupied { data, .. } => data,
                PointState::Free { .. } => panic!("Invalid or removed PointId: {id:?}"),
            },
            _ => panic!("Invalid or removed PointId: {id:?}"),
        }
    }
}

impl<T> IndexMut<PointId> for ContinuousSpace<T> {
    fn index_mut(&mut self, id: PointId) -> &mut T {
        match self.slots.get_mut(id.index as usize) {
            Some(slot) if slot.generation == id.generation => match &mut slot.state {
                PointState::Occupied { data, .. } => data,
                PointState::Free { .. } => panic!("Invalid or removed PointId: {id:?}"),
            },
            _ => panic!("Invalid or removed PointId: {id:?}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn vec2_operaciones() {
        let a = Vec2::new(3.0, 4.0);
        assert!((a.length() - 5.0).abs() < 1e-12);
        assert!((a.normalize_or_zero().length() - 1.0).abs() < 1e-12);
        assert_eq!(a + Vec2::new(1.0, 1.0), Vec2::new(4.0, 5.0));
        assert_eq!((a * 2.0), Vec2::new(6.0, 8.0));
        assert_eq!(Vec2::ZERO.normalize_or_zero(), Vec2::ZERO);
        assert!((a.clamp_length(2.5).length() - 2.5).abs() < 1e-12);
    }

    #[test]
    fn wrap_sin_torus_respeta_dominio_semiabierto() {
        let s: ContinuousSpace<()> = ContinuousSpace::new(50.0, 30.0, 5.0);
        for p in [
            Vec2::new(50.0, 30.0),
            Vec2::new(1000.0, 5.0),
            Vec2::new(5.0, -10.0),
            Vec2::new(50.0, 0.0),
        ] {
            let w = s.wrap(p);
            assert!(w.x < 50.0, "x={} is not < width", w.x);
            assert!(w.y < 30.0, "y={} is not < height", w.y);
            assert!(w.x >= 0.0 && w.y >= 0.0);
        }
    }

    #[test]
    fn within_encuentra_solo_los_cercanos() {
        let mut s: ContinuousSpace<()> = ContinuousSpace::new(100.0, 100.0, 5.0);
        let centro = s.add(Vec2::new(50.0, 50.0), ());
        let cerca = s.add(Vec2::new(53.0, 50.0), ()); // d=3
        let _lejos = s.add(Vec2::new(80.0, 80.0), ()); // d≈42
        s.reindex();
        let ids: HashSet<PointId> = s
            .within(Vec2::new(50.0, 50.0), 4.0)
            .into_iter()
            .map(|(i, _)| i)
            .collect();
        assert!(ids.contains(&centro) && ids.contains(&cerca));
        assert_eq!(ids.len(), 2, "the far one is left out");
    }

    #[test]
    fn distancia_toroidal_envuelve() {
        let s: ContinuousSpace<()> = ContinuousSpace::new(100.0, 100.0, 10.0).with_torus(true);
        // 2 and 98 are at distance 4 (crossing the edge), not 96.
        assert!((s.distance(Vec2::new(2.0, 0.0), Vec2::new(98.0, 0.0)) - 4.0).abs() < 1e-9);
        let d = s.delta(Vec2::new(98.0, 0.0), Vec2::new(2.0, 0.0));
        assert!((d.x - 4.0).abs() < 1e-9, "delta crosses the edge: {}", d.x);
    }

    #[test]
    fn within_toroidal_cruza_el_borde() {
        let mut s: ContinuousSpace<()> = ContinuousSpace::new(100.0, 100.0, 10.0).with_torus(true);
        let a = s.add(Vec2::new(2.0, 50.0), ());
        let b = s.add(Vec2::new(98.0, 50.0), ()); // at distance 4 from `a` via the edge
        s.reindex();
        let ids: HashSet<PointId> = s
            .within(Vec2::new(2.0, 50.0), 5.0)
            .into_iter()
            .map(|(i, _)| i)
            .collect();
        assert!(ids.contains(&a) && ids.contains(&b));
    }

    #[test]
    fn set_pos_y_reindex_actualizan_la_vecindad() {
        let mut s: ContinuousSpace<i32> = ContinuousSpace::new(50.0, 50.0, 5.0);
        let p = s.add(Vec2::new(10.0, 10.0), 7);
        s.reindex();
        assert_eq!(s.within(Vec2::new(40.0, 40.0), 3.0).len(), 0);
        s.set_pos(p, Vec2::new(40.0, 40.0));
        s.reindex();
        assert_eq!(s.within(Vec2::new(40.0, 40.0), 3.0).len(), 1);
        assert_eq!(s[p], 7); // the data is preserved
    }

    #[test]
    fn coincide_con_la_busqueda_por_fuerza_bruta() {
        use crate::rng::rng_from_seed;
        use rand::Rng;
        let mut rng = rng_from_seed(4);
        let mut s: ContinuousSpace<()> = ContinuousSpace::new(200.0, 200.0, 8.0);
        let pts: Vec<Vec2> = (0..400)
            .map(|_| Vec2::new(rng.random_range(0.0..200.0), rng.random_range(0.0..200.0)))
            .collect();
        for &p in &pts {
            s.add(p, ());
        }
        s.reindex();
        let c = Vec2::new(100.0, 100.0);
        let r = 15.0;
        let hash: HashSet<PointId> = s.within(c, r).into_iter().map(|(i, _)| i).collect();
        let brute: HashSet<usize> = pts
            .iter()
            .enumerate()
            .filter(|&(_, &p)| s.distance(c, p) <= r)
            .map(|(i, _)| i)
            .collect();
        assert_eq!(
            hash.len(),
            brute.len(),
            "the spatial hash matches brute force"
        );
    }

    #[test]
    fn radio_grande_relativo_al_torus_no_duplica_visitas() {
        // Conceptual regression test for the P1-4 fix: previously we
        // deduplicated with a HashSet because a search radius large
        // relative to a small torus could visit the same cell twice
        // (wrap-around). Now the range of offsets is bounded to at most
        // `rows`/`cols`, with no duplicates by construction. A 5×5 torus
        // with a radius that covers the whole grid is exactly that edge
        // case.
        let mut s: ContinuousSpace<()> = ContinuousSpace::new(25.0, 25.0, 5.0).with_torus(true);
        let ids: Vec<PointId> = (0..25)
            .map(|i| {
                let (x, y) = (i % 5, i / 5);
                s.add(Vec2::new(x as f64 * 5.0 + 2.5, y as f64 * 5.0 + 2.5), ())
            })
            .collect();
        s.reindex();
        let hits = s.within(Vec2::new(12.5, 12.5), 100.0); // radius larger than the whole torus
        assert_eq!(hits.len(), 25, "each point must appear exactly once");
        let unicos: HashSet<PointId> = hits.iter().map(|(id, _)| *id).collect();
        assert_eq!(unicos.len(), 25, "no duplicates");
        let _ = ids;
    }

    #[test]
    fn remove_libera_el_slot_y_el_id_viejo_queda_invalido() {
        let mut s: ContinuousSpace<&str> = ContinuousSpace::new(50.0, 50.0, 5.0);
        let a = s.add(Vec2::new(1.0, 1.0), "a");
        assert_eq!(s.remove(a), Some("a"));
        assert_eq!(s.len(), 0);
        // The next add reuses `a`'s slot.
        let c = s.add(Vec2::new(2.0, 2.0), "c");
        assert_eq!(c.as_usize(), a.as_usize(), "reuses the same index");
        assert_ne!(c, a, "but the full PointId differs (generation)");
        // The old id no longer resolves: a double remove is harmless.
        assert_eq!(s.remove(a), None);
    }

    #[test]
    fn removido_no_aparece_en_consultas_tras_reindex() {
        let mut s: ContinuousSpace<()> = ContinuousSpace::new(50.0, 50.0, 5.0);
        let a = s.add(Vec2::new(10.0, 10.0), ());
        let b = s.add(Vec2::new(10.5, 10.0), ());
        s.reindex();
        assert_eq!(s.within(Vec2::new(10.0, 10.0), 2.0).len(), 2);
        s.remove(a);
        s.reindex();
        let hits: Vec<PointId> = s
            .within(Vec2::new(10.0, 10.0), 2.0)
            .into_iter()
            .map(|(id, _)| id)
            .collect();
        assert_eq!(hits, vec![b]);
    }

    #[test]
    #[should_panic(expected = "Invalid or removed PointId")]
    fn position_de_punto_eliminado_panica() {
        let mut s: ContinuousSpace<()> = ContinuousSpace::new(50.0, 50.0, 5.0);
        let a = s.add(Vec2::new(1.0, 1.0), ());
        s.remove(a);
        let _ = s.position(a);
    }

    #[test]
    fn wrap_toroidal_nunca_devuelve_el_borde_superior() {
        // Regression (audit M3): `rem_euclid` with a tiny negative residue
        // rounds to exactly the modulus (`(-1e-15).rem_euclid(100.0) ==
        // 100.0`), leaking outside the half-open domain [0, 100).
        let s: ContinuousSpace<()> = ContinuousSpace::new(100.0, 100.0, 10.0).with_torus(true);
        let w = s.wrap(Vec2::new(-1e-15, -1e-15));
        assert!(w.x >= 0.0 && w.x < 100.0, "x={} out of [0, 100)", w.x);
        assert!(w.y >= 0.0 && w.y < 100.0, "y={} out of [0, 100)", w.y);
        assert_eq!(w.x, 0.0, "the toroidally equivalent point is 0.0");
        assert_eq!(w.y, 0.0, "the toroidally equivalent point is 0.0");
    }

    #[test]
    fn set_pos_toroidal_con_residuo_negativo_diminuto_queda_en_dominio() {
        let mut s: ContinuousSpace<()> = ContinuousSpace::new(100.0, 100.0, 10.0).with_torus(true);
        let p = s.add(Vec2::new(50.0, 50.0), ());
        s.set_pos(p, Vec2::new(-1e-15, -1e-15));
        let pos = s.position(p);
        assert!(pos.x >= 0.0 && pos.x < 100.0, "x={} out of [0, 100)", pos.x);
        assert!(pos.y >= 0.0 && pos.y < 100.0, "y={} out of [0, 100)", pos.y);
        assert_eq!(pos.x, 0.0);
        assert_eq!(pos.y, 0.0);
        // And the index accepts it without panicking (cell_coords clamps).
        s.reindex();
        assert_eq!(s.within(Vec2::new(0.0, 0.0), 1.0).len(), 1);
    }

    #[test]
    fn radio_infinito_o_astronomico_devuelve_todos_los_puntos() {
        // Regression (audit L1): `(radius / cell).ceil() as i64 + 1` with
        // radius = INFINITY (or >= ~9.2e18·cell) saturated the cast and
        // the `+ 1` overflowed — panic in debug, silently empty result in
        // release. With the clamp, an unbounded radius means "all buckets".
        for torus in [false, true] {
            let mut s: ContinuousSpace<()> =
                ContinuousSpace::new(100.0, 100.0, 10.0).with_torus(torus);
            for i in 0..10 {
                for j in 0..10 {
                    s.add(Vec2::new(i as f64 * 10.0 + 5.0, j as f64 * 10.0 + 5.0), ());
                }
            }
            s.reindex();
            for radius in [f64::INFINITY, 1e300] {
                let hits = s.within(Vec2::new(50.0, 50.0), radius);
                assert_eq!(hits.len(), 100, "torus={torus} radius={radius}");
                let unicos: HashSet<PointId> = hits.iter().map(|(id, _)| *id).collect();
                assert_eq!(unicos.len(), 100, "no duplicates: torus={torus}");
            }
        }
    }
}
