"""Schelling en Mesa — espejo exacto de examples/schelling (swarm-core).

Especificación compartida: grilla 50x50 torus, densidad 0.85, dos grupos
50/50, vecindad Moore, conforme si similitud >= 0.375, el inconforme se
muda a una celda vacía uniforme al azar, activación aleatoria por paso,
término cuando todos están conformes (máx. 200 pasos).
"""

import argparse
import csv
import pathlib

from mesa import Agent, Model
from mesa.space import SingleGrid

WIDTH = HEIGHT = 50
DENSITY = 0.85
TOLERANCE = 0.375
MAX_STEPS = 200


class Person(Agent):
    def __init__(self, model, group):
        super().__init__(model)
        self.group = group

    def similarity(self):
        neighbors = self.model.grid.get_neighbors(
            self.pos, moore=True, include_center=False
        )
        if not neighbors:
            return 1.0
        same = sum(1 for n in neighbors if n.group == self.group)
        return same / len(neighbors)

    def step(self):
        if self.similarity() >= TOLERANCE:
            return
        empties = sorted(self.model.grid.empties)
        if not empties:
            return
        self.model.grid.move_agent(self, self.model.random.choice(empties))


class SchellingModel(Model):
    def __init__(self, seed):
        super().__init__(seed=seed)
        self.grid = SingleGrid(WIDTH, HEIGHT, torus=True)
        coords = [(x, y) for y in range(HEIGHT) for x in range(WIDTH)]
        self.random.shuffle(coords)
        n_agents = round(WIDTH * HEIGHT * DENSITY)
        for i, pos in enumerate(coords[:n_agents]):
            self.grid.place_agent(Person(self, i % 2), pos)

    def fraction_happy(self):
        ags = list(self.agents)
        return sum(1 for a in ags if a.similarity() >= TOLERANCE) / len(ags)

    def mean_similarity(self):
        ags = list(self.agents)
        return sum(a.similarity() for a in ags) / len(ags)

    def finished(self):
        return all(a.similarity() >= TOLERANCE for a in self.agents)

    def step(self):
        self.agents.shuffle_do("step")


def run_one(seed, out_dir):
    model = SchellingModel(seed)
    rows = [(0, model.fraction_happy(), model.mean_similarity())]
    step = 0
    while step < MAX_STEPS and not model.finished():
        model.step()
        step += 1
        rows.append((step, model.fraction_happy(), model.mean_similarity()))

    path = out_dir / f"mesa_schelling_{seed}.csv"
    with open(path, "w", newline="") as f:
        w = csv.writer(f)
        w.writerow(["step", "fraccion_conforme", "similitud_media"])
        w.writerows(rows)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--seeds", type=int, default=20)
    ap.add_argument("--out", type=pathlib.Path, required=True)
    args = ap.parse_args()
    args.out.mkdir(parents=True, exist_ok=True)
    for seed in range(args.seeds):
        run_one(seed, args.out)
        print(f"schelling mesa seed {seed} listo")


if __name__ == "__main__":
    main()
