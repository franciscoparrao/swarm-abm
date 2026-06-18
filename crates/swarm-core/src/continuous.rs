//! Espacio **continuo** 2D para ABM *off-lattice*: movimiento, *flocking*,
//! partículas — donde la posición es real (`f64`), no una celda.
//!
//! Es el tercer espacio del motor, junto a [`Grid2D`](crate::grid::Grid2D)
//! (geométrico discreto) y [`Graph`](crate::graph::Graph) (topológico). La
//! vecindad es por **radio**, y las consultas usan un *spatial hash* (rejilla
//! de buckets) para no ser O(n²): tras mover los puntos se llama
//! [`reindex`](ContinuousSpace::reindex) una vez por paso, y luego
//! [`for_each_within`](ContinuousSpace::for_each_within) recorre solo las
//! celdas que cubren el disco buscado.

use std::collections::HashSet;
use std::ops::{Add, Index, IndexMut, Mul, Neg, Sub};

/// Vector / punto 2D con las operaciones habituales.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Vec2 {
    /// Coordenada horizontal.
    pub x: f64,
    /// Coordenada vertical.
    pub y: f64,
}

impl Vec2 {
    /// El vector nulo.
    pub const ZERO: Vec2 = Vec2 { x: 0.0, y: 0.0 };

    /// Crea un vector.
    #[must_use]
    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    /// Norma euclídea.
    #[must_use]
    pub fn length(self) -> f64 {
        self.length_sq().sqrt()
    }

    /// Norma al cuadrado (evita la raíz cuando solo se compara).
    #[must_use]
    pub fn length_sq(self) -> f64 {
        self.x * self.x + self.y * self.y
    }

    /// Versor (vector unitario); el vector nulo si la norma es 0.
    #[must_use]
    pub fn normalize_or_zero(self) -> Vec2 {
        let len = self.length();
        if len > 0.0 {
            self * (1.0 / len)
        } else {
            Vec2::ZERO
        }
    }

    /// Recorta la norma a un máximo (deja la dirección intacta).
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

/// Identificador estable de un punto en el espacio.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PointId(usize);

impl PointId {
    /// Índice interno del punto.
    #[must_use]
    pub fn as_usize(self) -> usize {
        self.0
    }
}

/// Espacio continuo 2D `[0,width) × [0,height)` con un dato `T` por punto y un
/// índice espacial por *hash*. Con `torus = true` los bordes se conectan y las
/// distancias son las más cortas con envolvente.
#[derive(Debug, Clone)]
pub struct ContinuousSpace<T> {
    width: f64,
    height: f64,
    torus: bool,
    cell: f64,
    cols: usize,
    rows: usize,
    pos: Vec<Vec2>,
    data: Vec<T>,
    buckets: Vec<Vec<PointId>>,
}

impl<T> ContinuousSpace<T> {
    /// Crea un espacio vacío de `width × height` con celdas de hash de lado
    /// `cell_size` (úsalo ≈ al radio de vecindad típico para eficiencia).
    ///
    /// # Panics
    /// Si alguna dimensión o `cell_size` no es positiva.
    #[must_use]
    pub fn new(width: f64, height: f64, cell_size: f64) -> Self {
        assert!(
            width > 0.0 && height > 0.0 && cell_size > 0.0,
            "dimensiones positivas"
        );
        let cols = (width / cell_size).ceil() as usize;
        let rows = (height / cell_size).ceil() as usize;
        Self {
            width,
            height,
            torus: false,
            cell: cell_size,
            cols: cols.max(1),
            rows: rows.max(1),
            pos: Vec::new(),
            data: Vec::new(),
            buckets: Vec::new(),
        }
    }

    /// Activa la topología toroidal (builder).
    #[must_use]
    pub fn with_torus(mut self, torus: bool) -> Self {
        self.torus = torus;
        self
    }

    /// Añade un punto y devuelve su identificador. Requiere
    /// [`reindex`](Self::reindex) antes de la siguiente consulta.
    pub fn add(&mut self, pos: Vec2, value: T) -> PointId {
        let id = PointId(self.pos.len());
        self.pos.push(self.wrap(pos));
        self.data.push(value);
        id
    }

    /// Número de puntos.
    #[must_use]
    pub fn len(&self) -> usize {
        self.pos.len()
    }

    /// `true` si no hay puntos.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.pos.is_empty()
    }

    /// Identificadores de todos los puntos.
    pub fn point_ids(&self) -> impl Iterator<Item = PointId> {
        (0..self.pos.len()).map(PointId)
    }

    /// Posición del punto.
    #[must_use]
    pub fn position(&self, id: PointId) -> Vec2 {
        self.pos[id.0]
    }

    /// Mueve un punto (envuelve si es toroidal). El índice no refleja el cambio
    /// hasta el próximo [`reindex`](Self::reindex).
    pub fn set_pos(&mut self, id: PointId, pos: Vec2) {
        self.pos[id.0] = self.wrap(pos);
    }

    /// Reconstruye el índice espacial desde las posiciones actuales. Llamar una
    /// vez por paso, tras mover los puntos (típicamente en `before_step`).
    pub fn reindex(&mut self) {
        self.buckets.clear();
        self.buckets.resize(self.cols * self.rows, Vec::new());
        for (i, &p) in self.pos.iter().enumerate() {
            let idx = self.cell_index(p);
            self.buckets[idx].push(PointId(i));
        }
    }

    /// Distancia euclídea entre dos posiciones (la más corta, si es toroidal).
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

    /// Vector más corto de `from` a `to` (cruza el borde si es más corto en un
    /// espacio toroidal). Útil para `cohesión`/`separación` consistentes.
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

    /// Envuelve/recorta una posición al dominio del espacio.
    #[must_use]
    pub fn wrap(&self, p: Vec2) -> Vec2 {
        if self.torus {
            Vec2::new(p.x.rem_euclid(self.width), p.y.rem_euclid(self.height))
        } else {
            Vec2::new(p.x.clamp(0.0, self.width), p.y.clamp(0.0, self.height))
        }
    }

    /// Invoca `f(id, posición, &dato, distancia)` para cada punto a distancia
    /// `≤ radius` de `center`. Sin asignar memoria; recorre solo las celdas del
    /// hash que cubren el disco.
    pub fn for_each_within(
        &self,
        center: Vec2,
        radius: f64,
        mut f: impl FnMut(PointId, Vec2, &T, f64),
    ) {
        let cr = (radius / self.cell).ceil() as i64 + 1;
        let (cc, cr_row) = self.cell_coords(center);
        let mut visited = HashSet::new();
        for drow in -cr..=cr {
            for dcol in -cr..=cr {
                let (col, row) = (cc as i64 + dcol, cr_row as i64 + drow);
                let (col, row) = if self.torus {
                    (
                        col.rem_euclid(self.cols as i64),
                        row.rem_euclid(self.rows as i64),
                    )
                } else if col < 0 || row < 0 || col >= self.cols as i64 || row >= self.rows as i64 {
                    continue;
                } else {
                    (col, row)
                };
                let idx = row as usize * self.cols + col as usize;
                if !visited.insert(idx) {
                    continue;
                }
                for &pid in &self.buckets[idx] {
                    let p = self.pos[pid.0];
                    let d = self.distance(center, p);
                    if d <= radius {
                        f(pid, p, &self.data[pid.0], d);
                    }
                }
            }
        }
    }

    /// Puntos a distancia `≤ radius` de `center`, como `(id, distancia)`.
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
    fn index(&self, id: PointId) -> &T {
        &self.data[id.0]
    }
}

impl<T> IndexMut<PointId> for ContinuousSpace<T> {
    fn index_mut(&mut self, id: PointId) -> &mut T {
        &mut self.data[id.0]
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
        assert_eq!(ids.len(), 2, "el lejano queda fuera");
    }

    #[test]
    fn distancia_toroidal_envuelve() {
        let s: ContinuousSpace<()> = ContinuousSpace::new(100.0, 100.0, 10.0).with_torus(true);
        // 2 y 98 están a 4 (cruzando el borde), no a 96.
        assert!((s.distance(Vec2::new(2.0, 0.0), Vec2::new(98.0, 0.0)) - 4.0).abs() < 1e-9);
        let d = s.delta(Vec2::new(98.0, 0.0), Vec2::new(2.0, 0.0));
        assert!((d.x - 4.0).abs() < 1e-9, "delta cruza el borde: {}", d.x);
    }

    #[test]
    fn within_toroidal_cruza_el_borde() {
        let mut s: ContinuousSpace<()> = ContinuousSpace::new(100.0, 100.0, 10.0).with_torus(true);
        let a = s.add(Vec2::new(2.0, 50.0), ());
        let b = s.add(Vec2::new(98.0, 50.0), ()); // a 4 de `a` por el borde
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
        assert_eq!(s[p], 7); // el dato se conserva
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
        let brute: HashSet<PointId> = pts
            .iter()
            .enumerate()
            .filter(|&(_, &p)| s.distance(c, p) <= r)
            .map(|(i, _)| PointId(i))
            .collect();
        assert_eq!(
            hash, brute,
            "el spatial hash debe coincidir con la fuerza bruta"
        );
    }
}
