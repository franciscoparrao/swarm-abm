"""SIR espacial en Mesa — espejo exacto de examples/sir (swarm-core).

Especificación compartida: grilla torus totalmente ocupada, vecindad
Moore, susceptible con k vecinos infectados se contagia con probabilidad
1-(1-beta)^k, infectado se recupera con probabilidad gamma, activación
aleatoria por paso, término cuando no quedan infectados.
"""

import argparse
import csv
import pathlib
import time

from mesa import Agent, Model
from mesa.space import SingleGrid

BETA = 0.08
GAMMA = 0.1


class Person(Agent):
    def __init__(self, model):
        super().__init__(model)
        self.status = "S"

    def step(self):
        if self.status == "S":
            k = sum(
                1
                for n in self.model.grid.get_neighbors(
                    self.pos, moore=True, include_center=False
                )
                if n.status == "I"
            )
            if k and self.model.random.random() < 1.0 - (1.0 - BETA) ** k:
                self.status = "I"
        elif self.status == "I":
            if self.model.random.random() < GAMMA:
                self.status = "R"


class SirModel(Model):
    def __init__(self, width, height, infected, seed):
        super().__init__(seed=seed)
        self.grid = SingleGrid(width, height, torus=True)
        for y in range(height):
            for x in range(width):
                self.grid.place_agent(Person(self), (x, y))
        for a in self.random.sample(list(self.agents), infected):
            a.status = "I"
        self.n = len(list(self.agents))

    def fractions(self):
        counts = {"S": 0, "I": 0, "R": 0}
        for a in self.agents:
            counts[a.status] += 1
        return counts["S"] / self.n, counts["I"] / self.n, counts["R"] / self.n

    def finished(self):
        return all(a.status != "I" for a in self.agents)

    def step(self):
        self.agents.shuffle_do("step")


def run_one(seed, width, height, infected, max_steps, out_dir):
    model = SirModel(width, height, infected, seed)
    rows = [(0, *model.fractions())]
    step = 0
    while step < max_steps and not model.finished():
        model.step()
        step += 1
        rows.append((step, *model.fractions()))

    path = out_dir / f"mesa_sir_{seed}.csv"
    with open(path, "w", newline="") as f:
        w = csv.writer(f)
        w.writerow(["step", "s", "i", "r"])
        w.writerows(rows)


def bench_one(seed, width, height, infected, max_steps):
    """Corre una réplica midiendo solo la fase de simulación (sin setup ni
    recolección de métricas) y reporta steps,ms — espejo de `sir --bench`."""
    model = SirModel(width, height, infected, seed)
    t0 = time.perf_counter()
    step = 0
    while step < max_steps and not model.finished():
        model.step()
        step += 1
    ms = (time.perf_counter() - t0) * 1000.0
    print(f"steps,ms\n{step},{ms:.3f}")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--seeds", type=int, default=20)
    ap.add_argument("--width", type=int, default=50)
    ap.add_argument("--height", type=int, default=50)
    ap.add_argument("--infected", type=int, default=5)
    ap.add_argument("--steps", type=int, default=300)
    ap.add_argument("--out", type=pathlib.Path)
    ap.add_argument("--bench", type=int, metavar="SEED", default=None)
    args = ap.parse_args()
    if args.bench is not None:
        bench_one(args.bench, args.width, args.height, args.infected, args.steps)
        return
    if args.out is None:
        ap.error("--out es requerido salvo en modo --bench")
    args.out.mkdir(parents=True, exist_ok=True)
    for seed in range(args.seeds):
        run_one(seed, args.width, args.height, args.infected, args.steps, args.out)
        print(f"sir mesa seed {seed} listo")


if __name__ == "__main__":
    main()
