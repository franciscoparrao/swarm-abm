# swarm-abm

Spatial agent-based modeling (ABM) engine: agents on a grid, graph, or
continuous space, configurable scheduling, time-series data collection, and
deterministic reproducibility — same seed, same result, bit for bit, even
under parallelism.

```rust
use swarm_abm::prelude::*;

struct Walker { pos: Pos }
struct World { agents: AgentSet<Walker>, visits: Grid2D<u32> }

impl Agent for Walker {
    type Model = World;
    fn step(&mut self, _id: AgentId, world: &mut World, rng: &mut SimRng) {
        if let Some(dest) = world.visits.random_neighbor(self.pos, Neighborhood::Moore, rng) {
            self.pos = dest;
            world.visits[self.pos] += 1;
        }
    }
}

impl Model for World {
    type Agent = Walker;
    fn agents(&self) -> &AgentSet<Walker> { &self.agents }
    fn agents_mut(&mut self) -> &mut AgentSet<Walker> { &mut self.agents }
}

let mut agents = AgentSet::new();
agents.insert(Walker { pos: Pos::new(5, 5) });
let mut sim = Simulation::new(World { agents, visits: Grid2D::new(10, 10) }, 42);
sim.run(100);
```

See the [full documentation](https://docs.rs/swarm-abm) and the
[repository](https://github.com/franciscoparrao/swarm-abm) for the complete
example set, benchmarks, and validation reports (numerical parity against
[Mesa](https://mesa.readthedocs.io/), determinism proofs, and the
reproducibility policy in `docs/REPRODUCIBILITY.md`).

## License

MIT OR Apache-2.0
