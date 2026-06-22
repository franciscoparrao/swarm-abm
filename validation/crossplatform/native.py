# Métricas nativas (x86-64) vía los bindings PyO3 — bits IEEE-754 exactos.
import json, struct
import swarm_abm as sw
h = lambda x: struct.pack('<d', x).hex()
out = {}
m = sw.Sir(size=100, beta=0.08, gamma=0.1, initial_infected=10, seed=42); m.run(500)
out["sir"] = {"recovered": h(m.recovered), "infected": h(m.infected)}
s = sw.Schelling(size=50, density=0.85, tolerance=0.375, seed=42); s.run(200)
out["schelling"] = {"happy": h(s.fraction_happy), "similarity": h(s.mean_similarity)}
g = sw.Sugarscape(size=50, n_agents=400, growback=1, seed=42); g.run(200)
out["sugarscape"] = {"population": g.population, "gini": h(g.gini)}
print(json.dumps(out, sort_keys=True))
