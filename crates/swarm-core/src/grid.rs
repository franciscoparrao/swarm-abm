//! Grilla 2D densa con vecindades de Moore y Von Neumann.

use std::ops::{Index, IndexMut};

use rand::Rng;

use crate::rng::SimRng;

/// Posición discreta `(x, y)` en una grilla. `x` es columna, `y` es fila.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Pos {
    /// Columna, en `0..width`.
    pub x: usize,
    /// Fila, en `0..height`.
    pub y: usize,
}

impl Pos {
    /// Crea una posición.
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

/// Tipo de vecindad sobre la grilla (radio 1 en v0.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Neighborhood {
    /// 8 celdas adyacentes (incluye diagonales).
    Moore,
    /// 4 celdas adyacentes (sin diagonales).
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

/// Iterador sin asignaciones sobre las posiciones vecinas (máx. 8).
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

/// Grilla 2D densa, almacenada en orden fila-mayor.
///
/// Con `torus = true` los bordes se conectan (mundo toroidal, estilo
/// NetLogo). En grillas toroidales con dimensión < 3, las posiciones
/// vecinas pueden repetirse (el wrap colisiona).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Grid2D<T> {
    width: usize,
    height: usize,
    torus: bool,
    cells: Vec<T>,
}

impl<T: Default + Clone> Grid2D<T> {
    /// Crea una grilla rellena con `T::default()`, sin torus.
    ///
    /// # Panics
    /// Si `width` o `height` es 0.
    #[must_use]
    pub fn new(width: usize, height: usize) -> Self {
        Self::fill(width, height, T::default())
    }
}

impl<T: Clone> Grid2D<T> {
    /// Crea una grilla rellena con copias de `value`, sin torus.
    ///
    /// # Panics
    /// Si `width` o `height` es 0.
    #[must_use]
    pub fn fill(width: usize, height: usize, value: T) -> Self {
        assert!(
            width > 0 && height > 0,
            "la grilla no puede tener dimensión 0"
        );
        Self {
            width,
            height,
            torus: false,
            cells: vec![value; width * height],
        }
    }
}

impl<T> Grid2D<T> {
    /// Crea una grilla evaluando `f(pos)` por celda, sin torus.
    ///
    /// # Panics
    /// Si `width` o `height` es 0.
    #[must_use]
    pub fn from_fn(width: usize, height: usize, mut f: impl FnMut(Pos) -> T) -> Self {
        assert!(
            width > 0 && height > 0,
            "la grilla no puede tener dimensión 0"
        );
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

    /// Activa o desactiva la topología toroidal (builder).
    #[must_use]
    pub fn with_torus(mut self, torus: bool) -> Self {
        self.torus = torus;
        self
    }

    /// Ancho de la grilla.
    #[must_use]
    pub fn width(&self) -> usize {
        self.width
    }

    /// Alto de la grilla.
    #[must_use]
    pub fn height(&self) -> usize {
        self.height
    }

    /// `true` si la grilla es toroidal.
    #[must_use]
    pub fn is_torus(&self) -> bool {
        self.torus
    }

    /// `true` si `pos` está dentro de la grilla.
    #[must_use]
    pub fn contains(&self, pos: Pos) -> bool {
        pos.x < self.width && pos.y < self.height
    }

    /// Referencia a la celda, o `None` si está fuera de rango.
    #[must_use]
    pub fn get(&self, pos: Pos) -> Option<&T> {
        self.contains(pos)
            .then(|| &self.cells[pos.y * self.width + pos.x])
    }

    /// Referencia mutable a la celda, o `None` si está fuera de rango.
    #[must_use]
    pub fn get_mut(&mut self, pos: Pos) -> Option<&mut T> {
        self.contains(pos)
            .then(|| &mut self.cells[pos.y * self.width + pos.x])
    }

    /// Intercambia el contenido de dos celdas.
    ///
    /// # Panics
    /// Si alguna posición está fuera de rango.
    pub fn swap(&mut self, a: Pos, b: Pos) {
        assert!(
            self.contains(a) && self.contains(b),
            "swap fuera de rango: {a:?}, {b:?}"
        );
        self.cells
            .swap(a.y * self.width + a.x, b.y * self.width + b.x);
    }

    /// Posiciones vecinas de `pos` según la vecindad, respetando torus/bordes.
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
                buf[len as usize] = p;
                len += 1;
            }
        }
        Neighbors { buf, len, i: 0 }
    }

    /// Una posición vecina uniforme al azar, sin asignar memoria.
    ///
    /// Equivale a recolectar [`neighbor_positions`](Self::neighbor_positions)
    /// y elegir un índice al azar (consume un solo draw del RNG), pero sin
    /// el `Vec` intermedio — importante en hot loops con muchos agentes.
    /// Devuelve `None` si la posición no tiene vecinas (grilla 1×1 sin torus).
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
        Some(vecinas.buf[rng.random_range(0..n)])
    }

    /// Itera sobre `(posición, &celda)` de las vecinas de `pos`.
    pub fn neighbors(
        &self,
        pos: Pos,
        neighborhood: Neighborhood,
    ) -> impl Iterator<Item = (Pos, &T)> {
        self.neighbor_positions(pos, neighborhood)
            .map(move |p| (p, &self.cells[p.y * self.width + p.x]))
    }

    /// Itera sobre todas las celdas como `(posición, &celda)`, fila por fila.
    pub fn iter(&self) -> impl Iterator<Item = (Pos, &T)> {
        self.cells
            .iter()
            .enumerate()
            .map(|(i, c)| (Pos::new(i % self.width, i / self.width), c))
    }

    /// Itera sobre todas las celdas como `(posición, &mut celda)`, fila por fila.
    pub fn iter_mut(&mut self) -> impl Iterator<Item = (Pos, &mut T)> {
        let width = self.width;
        self.cells
            .iter_mut()
            .enumerate()
            .map(move |(i, c)| (Pos::new(i % width, i / width), c))
    }

    /// Desplaza `pos` por `(dx, dy)`: con torus envuelve, sin torus devuelve
    /// `None` si sale de la grilla.
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
    /// Difunde el campo escalar un paso (semántica `diffuse` de NetLogo):
    /// cada celda reparte la fracción `rate` de su valor en partes iguales
    /// entre sus 8 (Moore) o 4 (Von Neumann) vecinas potenciales. Sin torus,
    /// la porción que saldría de la grilla se queda en la celda. La masa
    /// total se conserva siempre.
    ///
    /// La actualización es simultánea (doble buffer interno): el resultado
    /// no depende del orden de recorrido.
    ///
    /// # Panics
    /// Si `rate` no está en `[0, 1]`.
    pub fn diffuse(&mut self, rate: f64, neighborhood: Neighborhood) {
        assert!(
            (0.0..=1.0).contains(&rate),
            "rate de difusión fuera de [0, 1]: {rate}"
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

    /// Suma de todas las celdas (masa total del campo).
    #[must_use]
    pub fn total(&self) -> f64 {
        self.cells.iter().sum()
    }
}

impl<T> Index<Pos> for Grid2D<T> {
    type Output = T;

    /// # Panics
    /// Si `pos` está fuera de rango (usa [`Grid2D::get`] para la variante segura).
    fn index(&self, pos: Pos) -> &T {
        match self.get(pos) {
            Some(c) => c,
            None => panic!(
                "posición {pos:?} fuera de la grilla {}x{}",
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
            None => panic!("posición {pos:?} fuera de la grilla {w}x{h}"),
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
        assert!(!v.contains(&Pos::new(2, 2)), "la celda propia no es vecina");
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
                .expect("hay vecinas");
            assert!(vecinas.contains(&p));
        }

        // Mismo seed, misma secuencia de elecciones.
        let secuencia = |seed| {
            let mut rng = rng_from_seed(seed);
            (0..20)
                .map(|_| g.random_neighbor(centro, Neighborhood::Moore, &mut rng))
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
        // El centro retiene 1 - rate; cada vecina Moore recibe rate/8.
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
        // La esquina solo tiene 3 vecinas: entrega 3·(rate/8) y retiene el resto.
        assert!((g[esquina] - 65.0).abs() < 1e-12);
        assert!((g.total() - 80.0).abs() < 1e-12);
    }

    #[test]
    #[should_panic(expected = "fuera de [0, 1]")]
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
    #[should_panic(expected = "fuera de la grilla")]
    fn index_fuera_de_rango_panic() {
        let g: Grid2D<u8> = Grid2D::new(2, 2);
        let _ = g[Pos::new(5, 5)];
    }
}
