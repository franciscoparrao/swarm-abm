//! **Network** (graph) space for network ABMs: contagion on social
//! networks, propagation in infrastructure, information diffusion,
//! mobility.
//!
//! It is the analog of [`Grid2D`](crate::grid::Grid2D) when the neighborhood
//! is not geometric but topological: a [`NodeId`] identifies each node and
//! [`neighbors`](Graph::neighbors) gives its adjacent nodes. Each node holds
//! data `N`; each edge optionally holds data `E` (weight, capacity, contact
//! intensity — `E = ()` by default, unweighted graph). Undirected unless
//! built with [`directed`](Graph::directed).
//! Includes generators for the canonical topologies (always undirected and
//! unweighted, as in the literature), all deterministic given the seed of
//! the [`SimRng`].

use std::ops::{Index, IndexMut};

use crate::rng::{SimRng, bernoulli, uniform_usize};

/// Stable identifier of a node: its index in the graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct NodeId(usize);

impl NodeId {
    /// Internal index of the node.
    #[must_use]
    pub fn as_usize(self) -> usize {
        self.0
    }
}

/// Graph with data `N` per node, data `E` per edge (`()` by default:
/// unweighted graph), and adjacency lists.
///
/// Undirected by default: [`add_edge`](Self::add_edge)/
/// [`add_weighted_edge`](Self::add_weighted_edge) connect both directions.
/// With [`directed(true)`](Self::directed), only the `a → b` direction. Does
/// not support self-loops or multiple edges between the same ordered pair.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Graph<N, E = ()> {
    nodes: Vec<N>,
    adj: Vec<Vec<(NodeId, E)>>,
    directed: bool,
}

impl<N, E> Default for Graph<N, E> {
    fn default() -> Self {
        Self::new()
    }
}

impl<N, E> Graph<N, E> {
    /// Creates an empty, undirected graph.
    #[must_use]
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
            adj: Vec::new(),
            directed: false,
        }
    }

    /// Creates a graph of `n` nodes with no edges, evaluating `f` per node.
    pub fn from_fn(n: usize, mut f: impl FnMut(NodeId) -> N) -> Self {
        let nodes = (0..n).map(|i| f(NodeId(i))).collect();
        Self {
            nodes,
            // `(0..n).map(...).collect()` instead of `vec![Vec::new(); n]`:
            // the latter requires `E: Clone` (it clones the template n
            // times), even though the element is trivially an empty Vec.
            adj: (0..n).map(|_| Vec::new()).collect(),
            directed: false,
        }
    }

    /// Marks the graph as directed (builder). Affects the `add_edge`/
    /// `add_weighted_edge` calls made **after** this: a directed edge only
    /// appears in the source's adjacency list, not the destination's. The
    /// canonical topology generators (`complete`, `ring`, `erdos_renyi`,
    /// `watts_strogatz`, `barabasi_albert`) are always undirected, as
    /// defined in the literature — this builder is for graphs the user
    /// assembles by hand.
    #[must_use]
    pub fn directed(mut self, directed: bool) -> Self {
        self.directed = directed;
        self
    }

    /// `true` if the graph is directed.
    #[must_use]
    pub fn is_directed(&self) -> bool {
        self.directed
    }

    /// Adds a node with the given data and returns its identifier.
    pub fn add_node(&mut self, value: N) -> NodeId {
        let id = NodeId(self.nodes.len());
        self.nodes.push(value);
        self.adj.push(Vec::new());
        id
    }

    /// Connects `a → b` with edge data `weight`. In an undirected graph it
    /// also connects `b → a` with a copy of `weight` (hence `E: Clone`).
    /// Returns `false` if the edge already existed, is a self-loop, or
    /// either node is out of range.
    pub fn add_weighted_edge(&mut self, a: NodeId, b: NodeId, weight: E) -> bool
    where
        E: Clone,
    {
        if a == b || a.0 >= self.nodes.len() || b.0 >= self.nodes.len() || self.has_edge(a, b) {
            return false;
        }
        self.adj[a.0].push((b, weight.clone()));
        if !self.directed {
            self.adj[b.0].push((a, weight));
        }
        true
    }

    /// `true` if an edge `a → b` exists (in an undirected graph, equivalent
    /// to `b → a`).
    #[must_use]
    pub fn has_edge(&self, a: NodeId, b: NodeId) -> bool {
        self.adj
            .get(a.0)
            .is_some_and(|ns| ns.iter().any(|&(n, _)| n == b))
    }

    /// Neighbors of a node, without weights (no allocation). For the edge
    /// data, see [`neighbors_weighted`](Self::neighbors_weighted).
    pub fn neighbors(&self, node: NodeId) -> impl Iterator<Item = NodeId> + '_ {
        self.adj[node.0].iter().map(|&(n, _)| n)
    }

    /// Neighbors of a node together with the data of the edge connecting
    /// them.
    pub fn neighbors_weighted(&self, node: NodeId) -> impl Iterator<Item = (NodeId, &E)> {
        self.adj[node.0].iter().map(|(n, w)| (*n, w))
    }

    /// Degree (number of neighbors) of the node. In a directed graph this
    /// is the out-degree (edges leaving `node`).
    #[must_use]
    pub fn degree(&self, node: NodeId) -> usize {
        self.adj[node.0].len()
    }

    /// Number of nodes.
    #[must_use]
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// `true` if the graph has no nodes.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Number of edges: directed edges are counted once, undirected ones
    /// once per pair (not twice, even though internally they are stored in
    /// both lists).
    #[must_use]
    pub fn edge_count(&self) -> usize {
        let total: usize = self.adj.iter().map(Vec::len).sum();
        if self.directed { total } else { total / 2 }
    }

    /// Node data, or `None` if out of range.
    #[must_use]
    pub fn node(&self, id: NodeId) -> Option<&N> {
        self.nodes.get(id.0)
    }

    /// Mutable node data, or `None` if out of range.
    #[must_use]
    pub fn node_mut(&mut self, id: NodeId) -> Option<&mut N> {
        self.nodes.get_mut(id.0)
    }

    /// Identifiers of all nodes, in order.
    pub fn node_ids(&self) -> impl Iterator<Item = NodeId> {
        (0..self.nodes.len()).map(NodeId)
    }

    /// Iterates over `(NodeId, &data)`.
    pub fn iter(&self) -> impl Iterator<Item = (NodeId, &N)> {
        self.nodes.iter().enumerate().map(|(i, v)| (NodeId(i), v))
    }

    /// Iterates over `(NodeId, &mut data)`.
    pub fn iter_mut(&mut self) -> impl Iterator<Item = (NodeId, &mut N)> {
        self.nodes
            .iter_mut()
            .enumerate()
            .map(|(i, v)| (NodeId(i), v))
    }

    /// A uniformly random neighbor, without allocation. `None` if the node
    /// is isolated.
    #[must_use]
    pub fn random_neighbor(&self, node: NodeId, rng: &mut SimRng) -> Option<NodeId> {
        let ns = &self.adj[node.0];
        if ns.is_empty() {
            None
        } else {
            Some(ns[uniform_usize(rng, ns.len())].0)
        }
    }
}

impl<N, E: Default + Clone> Graph<N, E> {
    /// Convenience: [`add_weighted_edge`](Self::add_weighted_edge) with
    /// `E`'s default weight — for unweighted graphs (`E = ()`, the common
    /// case) no edge data needs to be passed.
    pub fn add_edge(&mut self, a: NodeId, b: NodeId) -> bool {
        self.add_weighted_edge(a, b, E::default())
    }
}

impl<N, E> Index<NodeId> for Graph<N, E> {
    type Output = N;

    fn index(&self, id: NodeId) -> &N {
        &self.nodes[id.0]
    }
}

impl<N, E> IndexMut<NodeId> for Graph<N, E> {
    fn index_mut(&mut self, id: NodeId) -> &mut N {
        &mut self.nodes[id.0]
    }
}

// ---------------------------------------------------------------------------
// Canonical topology generators (nodes set to `N::default()`, undirected,
// unweighted — `E = ()`, as defined in the literature).
// ---------------------------------------------------------------------------

impl<N: Default> Graph<N> {
    /// Complete graph: every pair of nodes connected.
    #[must_use]
    pub fn complete(n: usize) -> Self {
        let mut g = Graph::from_fn(n, |_| N::default());
        for a in 0..n {
            for b in (a + 1)..n {
                g.add_edge(NodeId(a), NodeId(b));
            }
        }
        g
    }

    /// Regular ring: each node connected to its `k` nearest neighbors on
    /// each side (2k edges per node). Basis of the small-world model.
    ///
    /// # Panics
    /// If `2k >= n`.
    #[must_use]
    pub fn ring(n: usize, k: usize) -> Self {
        assert!(n == 0 || 2 * k < n, "the ring requires 2k < n");
        let mut g = Graph::from_fn(n, |_| N::default());
        for a in 0..n {
            for d in 1..=k {
                g.add_edge(NodeId(a), NodeId((a + d) % n));
            }
        }
        g
    }

    /// **Erdős–Rényi** G(n, p): each pair of nodes is connected with
    /// probability `p`. Mean degree ≈ `p·(n−1)`; degrees follow a Poisson
    /// distribution.
    #[must_use]
    pub fn erdos_renyi(n: usize, p: f64, rng: &mut SimRng) -> Self {
        let mut g = Graph::from_fn(n, |_| N::default());
        for a in 0..n {
            for b in (a + 1)..n {
                if bernoulli(rng, p) {
                    g.add_edge(NodeId(a), NodeId(b));
                }
            }
        }
        g
    }

    /// **Watts–Strogatz**: regular ring of `k` neighbors per side where
    /// each edge is rewired with probability `beta` to a random
    /// destination. Produces the *small-world* regime (short path length +
    /// high clustering).
    ///
    /// # Panics
    /// If `2k >= n`. Also if `2k` is close enough to `n` that the resulting
    /// graph approaches the complete graph (`n·k` close to `n·(n-1)/2`):
    /// the construction is greedy and may exhaust a node's valid
    /// destinations before finishing. This is not the intended usage
    /// regime of a *small-world* model (which assumes `k ≪ n`); with `k`
    /// reasonably smaller than `n` (most real use cases) it does not
    /// occur.
    #[must_use]
    pub fn watts_strogatz(n: usize, k: usize, beta: f64, rng: &mut SimRng) -> Self {
        assert!(n == 0 || 2 * k < n, "small-world requires 2k < n");
        let mut g = Graph::from_fn(n, |_| N::default());
        // Base ring + rewiring of the "forward" edges.
        for a in 0..n {
            for d in 1..=k {
                let b = (a + d) % n;
                let objetivo = if bernoulli(rng, beta) {
                    Self::destino_recableo_valido(&g, a, n, rng)
                } else {
                    b
                };
                // The "desired" destination (rewired or the original `b`)
                // may already be a neighbor of `a`: this can happen if an
                // earlier rewiring of THIS SAME node (a different `d`,
                // processed earlier) landed on exactly that destination.
                // In that case `add_edge` does nothing and the iteration
                // would silently lose its edge, breaking edge conservation
                // (a real bug, found when replacing the sampling: with
                // `rand::Rng::random_range` it never manifested in the
                // seeds tested, but the risk had always existed). On
                // collision, it is rewired again — with the same success
                // guarantee as the normal rewiring.
                let objetivo = if g.has_edge(NodeId(a), NodeId(objetivo)) {
                    Self::destino_recableo_valido(&g, a, n, rng)
                } else {
                    objetivo
                };
                let anadida = g.add_edge(NodeId(a), NodeId(objetivo));
                debug_assert!(
                    anadida,
                    "destino_recableo_valido must always be a new destination"
                );
            }
        }
        g
    }

    /// Valid destination for rewiring `a` in Watts–Strogatz: different from
    /// `a` and not already connected. In the normal usage regime (`k ≪ n`)
    /// the degree of `a` at any point during construction is low, so
    /// rejection sampling almost always finishes in a few attempts. The
    /// deterministic fallback (linear scan) covers unlucky streaks; if even
    /// that finds no destination, it is because `a` is already connected to
    /// all other nodes — possible only if `2k` is very close to `n`, see
    /// the `# Panics` section of [`watts_strogatz`](Self::watts_strogatz)
    /// — and this is reported with an explicit panic instead of silently
    /// losing the edge.
    fn destino_recableo_valido(g: &Graph<N>, a: usize, n: usize, rng: &mut SimRng) -> usize {
        for _ in 0..(4 * n + 16) {
            let c = uniform_usize(rng, n);
            if c != a && !g.has_edge(NodeId(a), NodeId(c)) {
                return c;
            }
        }
        (0..n)
            .find(|&c| c != a && !g.has_edge(NodeId(a), NodeId(c)))
            .unwrap_or_else(|| {
                panic!(
                    "watts_strogatz: node {a} already connected to all other \
                 nodes ({n} nodes) — 2k too close to n for this generator"
                )
            })
    }

    /// **Barabási–Albert**: growth with *preferential attachment*. Starts
    /// with `m` nodes in a star and each new node connects to `m` existing
    /// nodes with probability proportional to their degree. Produces a
    /// *scale-free* network (heavy-tailed degree distribution, with hubs).
    ///
    /// # Panics
    /// If `m == 0` or `m >= n`.
    #[must_use]
    pub fn barabasi_albert(n: usize, m: usize, rng: &mut SimRng) -> Self {
        assert!(m > 0 && m < n, "Barabási–Albert requires 0 < m < n");
        let mut g = Graph::from_fn(n, |_| N::default());
        // Initial core: m+1 nodes in a star (guarantees degrees > 0).
        for a in 1..=m {
            g.add_edge(NodeId(0), NodeId(a));
        }
        // `repeated` contains each node as many times as its degree:
        // sampling from here gives probability ∝ degree (preferential
        // attachment).
        let mut repeated: Vec<usize> = Vec::new();
        for a in 0..=m {
            for _ in 0..g.degree(NodeId(a)) {
                repeated.push(a);
            }
        }
        for new in (m + 1)..n {
            // Vec instead of HashSet: a HashSet's iteration order is not
            // deterministic across runs (RandomState), and that order
            // decides the sequence in which edges get added to
            // `repeated`, contaminating the sampling of subsequent nodes.
            // A Vec preserves discovery order (tied to the RNG draw) and
            // `m` is small, so the linear `contains` costs nothing.
            let mut targets: Vec<usize> = Vec::with_capacity(m);
            while targets.len() < m {
                let t = repeated[uniform_usize(rng, repeated.len())];
                if !targets.contains(&t) {
                    targets.push(t);
                }
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
        assert!(!g.add_edge(NodeId(1), NodeId(0)), "duplicate (symmetric)");
        assert!(!g.add_edge(NodeId(2), NodeId(2)), "self-loop");
        // Symmetry: 1 is a neighbor of 0 and 0 is a neighbor of 1.
        assert_eq!(g.neighbors(NodeId(0)).collect::<Vec<_>>(), vec![NodeId(1)]);
        assert_eq!(g.neighbors(NodeId(1)).collect::<Vec<_>>(), vec![NodeId(0)]);
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
        assert_eq!(a, g(7), "same seed, same graph");
        // Mean degree ≈ p(n-1) = 0.02·499 ≈ 10 (wide tolerance).
        let mean_deg = 2.0 * a.edge_count() as f64 / a.node_count() as f64;
        assert!((mean_deg - 10.0).abs() < 3.0, "mean degree {mean_deg}");
    }

    #[test]
    fn watts_strogatz_conserva_numero_de_aristas() {
        let g = Graph::<()>::watts_strogatz(200, 3, 0.1, &mut rng_from_seed(1));
        // Rewiring moves edges but neither creates nor destroys them.
        assert_eq!(g.edge_count(), 200 * 3);
        assert_eq!(
            g,
            Graph::<()>::watts_strogatz(200, 3, 0.1, &mut rng_from_seed(1))
        );
    }

    #[test]
    fn watts_strogatz_conserva_aristas_en_muchos_seeds_y_parametros() {
        // Regression: a rewiring could collide with the "original"
        // destination of another `d` of the same node (not yet
        // processed), and `add_edge` would silently fail, losing an edge
        // (observed with seed=1, n=200, k=3, beta=0.1 after changing the
        // sampling — see P0-2 in docs/AUDIT.md). High `beta` and `2k`
        // large relative to `n` maximize the collision probability
        // (without entering the degenerate near-complete-graph regime,
        // `2k ≈ n`, where the greedy construction can exhaust valid
        // destinations through saturation — that is not the bug being
        // targeted here).
        for &(n, k) in &[(20usize, 4usize), (30, 6), (50, 5), (200, 3), (500, 10)] {
            for &beta in &[0.0, 0.1, 0.5, 0.9, 1.0] {
                for seed in 0..10u64 {
                    let g = Graph::<()>::watts_strogatz(n, k, beta, &mut rng_from_seed(seed));
                    assert_eq!(
                        g.edge_count(),
                        n * k,
                        "n={n} k={k} beta={beta} seed={seed}: lost edge"
                    );
                }
            }
        }
    }

    #[test]
    fn barabasi_albert_es_determinista() {
        // Regression: destination sampling used a HashSet whose iteration
        // order (RandomState) is not deterministic across constructions,
        // even with the same RNG seed. Covers multiple seeds because the
        // bug did not always manifest on the first one tested.
        for seed in 0..20 {
            let a = Graph::<()>::barabasi_albert(500, 3, &mut rng_from_seed(seed));
            let b = Graph::<()>::barabasi_albert(500, 3, &mut rng_from_seed(seed));
            assert_eq!(a, b, "seed {seed}: same seed, different graph");
        }
    }

    #[test]
    fn barabasi_albert_es_scale_free() {
        let g = Graph::<()>::barabasi_albert(1000, 2, &mut rng_from_seed(3));
        let degrees: Vec<usize> = g.node_ids().map(|v| g.degree(v)).collect();
        let max_deg = *degrees.iter().max().expect("hay nodos");
        let mean_deg = degrees.iter().sum::<usize>() as f64 / degrees.len() as f64;
        // Heavy tail: the most connected hub far exceeds the mean degree.
        assert!(
            max_deg as f64 > 8.0 * mean_deg,
            "max {max_deg} vs mean {mean_deg}"
        );
        assert!(g.node_ids().all(|v| g.degree(v) >= 1), "no isolated nodes");
    }

    #[test]
    fn random_neighbor_es_vecino_valido() {
        let g: Graph<()> = Graph::ring(8, 1);
        let mut rng = rng_from_seed(5);
        let vecinos: HashSet<NodeId> = g.neighbors(NodeId(0)).collect();
        for _ in 0..50 {
            let v = g
                .random_neighbor(NodeId(0), &mut rng)
                .expect("node 0 has neighbors");
            assert!(vecinos.contains(&v));
        }
    }

    #[test]
    fn grafo_ponderado_guarda_y_lee_el_peso() {
        let mut g: Graph<(), f64> = Graph::from_fn(3, |_| ());
        assert!(g.add_weighted_edge(NodeId(0), NodeId(1), 2.5));
        assert!(g.add_weighted_edge(NodeId(1), NodeId(2), 7.0));
        let pesos: Vec<(NodeId, f64)> = g
            .neighbors_weighted(NodeId(1))
            .map(|(n, &w)| (n, w))
            .collect();
        assert_eq!(pesos.len(), 2);
        assert!(pesos.contains(&(NodeId(0), 2.5)));
        assert!(pesos.contains(&(NodeId(2), 7.0)));
        // Undirected: the weight also appears from the other end.
        assert_eq!(
            g.neighbors_weighted(NodeId(0)).next().map(|(n, &w)| (n, w)),
            Some((NodeId(1), 2.5))
        );
    }

    #[test]
    fn grafo_dirigido_solo_conecta_un_sentido() {
        let mut g: Graph<()> = Graph::from_fn(3, |_| ()).directed(true);
        assert!(g.is_directed());
        assert!(g.add_edge(NodeId(0), NodeId(1)));
        assert_eq!(g.neighbors(NodeId(0)).collect::<Vec<_>>(), vec![NodeId(1)]);
        assert_eq!(g.neighbors(NodeId(1)).collect::<Vec<_>>(), vec![]);
        assert!(!g.has_edge(NodeId(1), NodeId(0)));
        assert_eq!(g.edge_count(), 1);
        // A second edge in the opposite direction is a DIFFERENT edge
        // (directed), not a duplicate.
        assert!(g.add_edge(NodeId(1), NodeId(0)));
        assert_eq!(g.edge_count(), 2);
    }
}
