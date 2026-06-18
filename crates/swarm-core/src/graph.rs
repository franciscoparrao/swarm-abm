//! Espacio de **red** (grafo no dirigido) para ABM sobre redes: contagio en
//! redes sociales, propagación en infraestructura, difusión de información.
//!
//! Es el análogo de [`Grid2D`](crate::grid::Grid2D) cuando la vecindad no es
//! geométrica sino topológica: un [`NodeId`] identifica cada nodo y
//! [`neighbors`](Graph::neighbors) da sus adyacentes. Cada nodo guarda un dato
//! `T` (p. ej. el agente que lo habita). Incluye generadores de las
//! topologías canónicas, todos deterministas dada la semilla del
//! [`SimRng`].

use std::ops::{Index, IndexMut};

use rand::Rng;

use crate::rng::SimRng;

/// Identificador estable de un nodo: su índice en el grafo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NodeId(usize);

impl NodeId {
    /// Índice interno del nodo.
    #[must_use]
    pub fn as_usize(self) -> usize {
        self.0
    }
}

/// Grafo no dirigido con un dato `T` por nodo y listas de adyacencia.
///
/// Las aristas son simétricas: [`add_edge`](Self::add_edge) conecta ambos
/// sentidos. No admite bucles ni aristas múltiples.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Graph<T> {
    nodes: Vec<T>,
    adj: Vec<Vec<NodeId>>,
}

impl<T> Default for Graph<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> Graph<T> {
    /// Crea un grafo vacío.
    #[must_use]
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
            adj: Vec::new(),
        }
    }

    /// Crea un grafo de `n` nodos sin aristas, evaluando `f` por nodo.
    pub fn from_fn(n: usize, mut f: impl FnMut(NodeId) -> T) -> Self {
        let nodes = (0..n).map(|i| f(NodeId(i))).collect();
        Self {
            nodes,
            adj: vec![Vec::new(); n],
        }
    }

    /// Añade un nodo con el dato dado y devuelve su identificador.
    pub fn add_node(&mut self, value: T) -> NodeId {
        let id = NodeId(self.nodes.len());
        self.nodes.push(value);
        self.adj.push(Vec::new());
        id
    }

    /// Conecta `a` y `b` (en ambos sentidos). Devuelve `false` si la arista ya
    /// existía, si es un bucle, o si algún nodo está fuera de rango.
    pub fn add_edge(&mut self, a: NodeId, b: NodeId) -> bool {
        if a == b
            || a.0 >= self.nodes.len()
            || b.0 >= self.nodes.len()
            || self.adj[a.0].contains(&b)
        {
            return false;
        }
        self.adj[a.0].push(b);
        self.adj[b.0].push(a);
        true
    }

    /// Vecinos de un nodo (sin asignar memoria).
    #[must_use]
    pub fn neighbors(&self, node: NodeId) -> &[NodeId] {
        &self.adj[node.0]
    }

    /// Grado (número de vecinos) del nodo.
    #[must_use]
    pub fn degree(&self, node: NodeId) -> usize {
        self.adj[node.0].len()
    }

    /// Número de nodos.
    #[must_use]
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// `true` si el grafo no tiene nodos.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Número de aristas (no dirigidas).
    #[must_use]
    pub fn edge_count(&self) -> usize {
        self.adj.iter().map(Vec::len).sum::<usize>() / 2
    }

    /// Dato del nodo, o `None` si está fuera de rango.
    #[must_use]
    pub fn node(&self, id: NodeId) -> Option<&T> {
        self.nodes.get(id.0)
    }

    /// Dato mutable del nodo, o `None` si está fuera de rango.
    #[must_use]
    pub fn node_mut(&mut self, id: NodeId) -> Option<&mut T> {
        self.nodes.get_mut(id.0)
    }

    /// Identificadores de todos los nodos, en orden.
    pub fn node_ids(&self) -> impl Iterator<Item = NodeId> {
        (0..self.nodes.len()).map(NodeId)
    }

    /// Itera sobre `(NodeId, &dato)`.
    pub fn iter(&self) -> impl Iterator<Item = (NodeId, &T)> {
        self.nodes.iter().enumerate().map(|(i, v)| (NodeId(i), v))
    }

    /// Itera sobre `(NodeId, &mut dato)`.
    pub fn iter_mut(&mut self) -> impl Iterator<Item = (NodeId, &mut T)> {
        self.nodes
            .iter_mut()
            .enumerate()
            .map(|(i, v)| (NodeId(i), v))
    }

    /// Un vecino uniforme al azar, sin asignar memoria. `None` si el nodo es
    /// aislado.
    #[must_use]
    pub fn random_neighbor(&self, node: NodeId, rng: &mut SimRng) -> Option<NodeId> {
        let ns = &self.adj[node.0];
        if ns.is_empty() {
            None
        } else {
            Some(ns[rng.random_range(0..ns.len())])
        }
    }
}

impl<T> Index<NodeId> for Graph<T> {
    type Output = T;

    fn index(&self, id: NodeId) -> &T {
        &self.nodes[id.0]
    }
}

impl<T> IndexMut<NodeId> for Graph<T> {
    fn index_mut(&mut self, id: NodeId) -> &mut T {
        &mut self.nodes[id.0]
    }
}

// ---------------------------------------------------------------------------
// Generadores de topologías canónicas (nodos a `T::default()`).
// ---------------------------------------------------------------------------

impl<T: Default> Graph<T> {
    /// Grafo completo: cada par de nodos conectado.
    #[must_use]
    pub fn complete(n: usize) -> Self {
        let mut g = Graph::from_fn(n, |_| T::default());
        for a in 0..n {
            for b in (a + 1)..n {
                g.add_edge(NodeId(a), NodeId(b));
            }
        }
        g
    }

    /// Anillo regular: cada nodo conectado a sus `k` vecinos más cercanos a
    /// cada lado (2k aristas por nodo). Base del modelo small-world.
    ///
    /// # Panics
    /// Si `2k >= n`.
    #[must_use]
    pub fn ring(n: usize, k: usize) -> Self {
        assert!(n == 0 || 2 * k < n, "el anillo requiere 2k < n");
        let mut g = Graph::from_fn(n, |_| T::default());
        for a in 0..n {
            for d in 1..=k {
                g.add_edge(NodeId(a), NodeId((a + d) % n));
            }
        }
        g
    }

    /// **Erdős–Rényi** G(n, p): cada par de nodos se conecta con probabilidad
    /// `p`. Grado medio ≈ `p·(n−1)`; grados con distribución de Poisson.
    #[must_use]
    pub fn erdos_renyi(n: usize, p: f64, rng: &mut SimRng) -> Self {
        let mut g = Graph::from_fn(n, |_| T::default());
        for a in 0..n {
            for b in (a + 1)..n {
                if rng.random_range(0.0..1.0) < p {
                    g.add_edge(NodeId(a), NodeId(b));
                }
            }
        }
        g
    }

    /// **Watts–Strogatz**: anillo regular de `k` vecinos por lado donde cada
    /// arista se recablea con probabilidad `beta` a un destino al azar.
    /// Produce el régimen *small-world* (camino corto + alto agrupamiento).
    ///
    /// # Panics
    /// Si `2k >= n`.
    #[must_use]
    pub fn watts_strogatz(n: usize, k: usize, beta: f64, rng: &mut SimRng) -> Self {
        assert!(n == 0 || 2 * k < n, "small-world requiere 2k < n");
        let mut g = Graph::from_fn(n, |_| T::default());
        // Anillo base + recableado de las aristas "hacia adelante".
        for a in 0..n {
            for d in 1..=k {
                let b = (a + d) % n;
                if rng.random_range(0.0..1.0) < beta {
                    // Reconectar a un destino nuevo, válido y no repetido.
                    let mut intentos = 0;
                    loop {
                        let c = rng.random_range(0..n);
                        if c != a
                            && !g.adj[a].contains(&NodeId(c))
                            && g.add_edge(NodeId(a), NodeId(c))
                        {
                            break;
                        }
                        intentos += 1;
                        if intentos > 2 * n {
                            g.add_edge(NodeId(a), NodeId(b)); // se rinde: arista original
                            break;
                        }
                    }
                } else {
                    g.add_edge(NodeId(a), NodeId(b));
                }
            }
        }
        g
    }

    /// **Barabási–Albert**: crecimiento con *preferential attachment*. Arranca
    /// con `m` nodos en estrella y cada nodo nuevo se une a `m` existentes con
    /// probabilidad proporcional a su grado. Produce una red *scale-free*
    /// (distribución de grados de cola pesada, con hubs).
    ///
    /// # Panics
    /// Si `m == 0` o `m >= n`.
    #[must_use]
    pub fn barabasi_albert(n: usize, m: usize, rng: &mut SimRng) -> Self {
        assert!(m > 0 && m < n, "Barabási–Albert requiere 0 < m < n");
        let mut g = Graph::from_fn(n, |_| T::default());
        // Núcleo inicial: m+1 nodos en estrella (garantiza grados > 0).
        for a in 1..=m {
            g.add_edge(NodeId(0), NodeId(a));
        }
        // `repeated` contiene cada nodo tantas veces como su grado: muestrear
        // de aquí da probabilidad ∝ grado (preferential attachment).
        let mut repeated: Vec<usize> = Vec::new();
        for a in 0..=m {
            for _ in 0..g.degree(NodeId(a)) {
                repeated.push(a);
            }
        }
        for new in (m + 1)..n {
            let mut targets = std::collections::HashSet::new();
            while targets.len() < m {
                let t = repeated[rng.random_range(0..repeated.len())];
                targets.insert(t);
            }
            for &t in &targets {
                g.add_edge(NodeId(new), NodeId(t));
                repeated.push(new);
                repeated.push(t);
            }
        }
        g
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rng::rng_from_seed;
    use std::collections::HashSet;

    #[test]
    fn aristas_simetricas_sin_bucles_ni_duplicados() {
        let mut g: Graph<()> = Graph::from_fn(4, |_| ());
        assert!(g.add_edge(NodeId(0), NodeId(1)));
        assert!(!g.add_edge(NodeId(1), NodeId(0)), "duplicada (simétrica)");
        assert!(!g.add_edge(NodeId(2), NodeId(2)), "bucle");
        // Simetría: 1 es vecino de 0 y 0 es vecino de 1.
        assert_eq!(g.neighbors(NodeId(0)), &[NodeId(1)]);
        assert_eq!(g.neighbors(NodeId(1)), &[NodeId(0)]);
        assert_eq!(g.edge_count(), 1);
    }

    #[test]
    fn complete_tiene_todas_las_aristas() {
        let g: Graph<()> = Graph::complete(6);
        assert_eq!(g.edge_count(), 6 * 5 / 2);
        assert!(g.node_ids().all(|v| g.degree(v) == 5));
    }

    #[test]
    fn ring_grado_regular() {
        let g: Graph<()> = Graph::ring(10, 2);
        assert!(g.node_ids().all(|v| g.degree(v) == 4));
        assert_eq!(g.edge_count(), 10 * 4 / 2);
    }

    #[test]
    fn erdos_renyi_grado_medio_y_determinismo() {
        let g = |seed| Graph::<()>::erdos_renyi(500, 0.02, &mut rng_from_seed(seed));
        let a = g(7);
        assert_eq!(a, g(7), "misma semilla, mismo grafo");
        // Grado medio ≈ p(n-1) = 0.02·499 ≈ 10 (tolerancia amplia).
        let mean_deg = 2.0 * a.edge_count() as f64 / a.node_count() as f64;
        assert!((mean_deg - 10.0).abs() < 3.0, "grado medio {mean_deg}");
    }

    #[test]
    fn watts_strogatz_conserva_numero_de_aristas() {
        let g = Graph::<()>::watts_strogatz(200, 3, 0.1, &mut rng_from_seed(1));
        // El recableado mueve aristas pero no las crea ni destruye.
        assert_eq!(g.edge_count(), 200 * 3);
        assert_eq!(
            g,
            Graph::<()>::watts_strogatz(200, 3, 0.1, &mut rng_from_seed(1))
        );
    }

    #[test]
    fn barabasi_albert_es_scale_free() {
        let g = Graph::<()>::barabasi_albert(1000, 2, &mut rng_from_seed(3));
        let degrees: Vec<usize> = g.node_ids().map(|v| g.degree(v)).collect();
        let max_deg = *degrees.iter().max().expect("hay nodos");
        let mean_deg = degrees.iter().sum::<usize>() as f64 / degrees.len() as f64;
        // Cola pesada: el hub más conectado supera con creces el grado medio.
        assert!(
            max_deg as f64 > 8.0 * mean_deg,
            "max {max_deg} vs media {mean_deg}"
        );
        assert!(g.node_ids().all(|v| g.degree(v) >= 1), "sin nodos aislados");
    }

    #[test]
    fn random_neighbor_es_vecino_valido() {
        let g: Graph<()> = Graph::ring(8, 1);
        let mut rng = rng_from_seed(5);
        let vecinos: HashSet<NodeId> = g.neighbors(NodeId(0)).iter().copied().collect();
        for _ in 0..50 {
            let v = g
                .random_neighbor(NodeId(0), &mut rng)
                .expect("nodo 0 tiene vecinos");
            assert!(vecinos.contains(&v));
        }
    }
}
