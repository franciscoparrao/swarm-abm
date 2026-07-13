#!/usr/bin/env python3
"""Paridad distribucional C++ (sheep_fox) vs oráculo swarm-abm (sigrid).

Barre una grilla de parámetros que produce loss_rates variados, corre ambos
motores con las mismas semillas, y reporta la correlación (Pearson/Spearman),
RMSE y sesgo — misma metodología que la paridad vs Mesa (PARITY.md).
"""
import subprocess, re, sys, statistics as st

CPP = "./sheep_fox"
ORACLE = sys.argv[1] if len(sys.argv) > 1 else \
    "/tmp/claude-1000/-home-franciscoparrao-proyectos-swarm-abm/b24e00a1-b0d9-4ec3-876f-88cb5dc00095/scratchpad/sigrid-oracle/target/release/sigrid"
DAYS = 14
SEEDS = 3
SEED0 = 2000

# grilla de puntos: (sheep_density, fox_eff) — parámetros que AMBOS motores
# honran. sheep_density alto diluye el loss_rate (más ovejas, mismos zorros).
# NOTA: el CLI del oráculo (main.rs @ HEAD) no parsea --fox-density, así que se
# deja en el default (8.4) en ambos motores para comparar peras con peras.
GRID = [(sd, fe) for sd in (0.96, 2.0, 4.0) for fe in (0.08, 0.14, 0.26)]

def mean_loss(binpath, sd, fe):
    out = subprocess.run(
        [binpath, "--days", str(DAYS), "--seed", str(SEED0), "--seeds", str(SEEDS),
         "--sheep-density", str(sd), "--fox-eff", str(fe)],
        capture_output=True, text=True).stdout
    m = re.search(r"loss_rate medio\s+([\d.]+)%", out)
    return float(m.group(1)) if m else float("nan")

def pearson(a, b):
    n = len(a); ma, mb = sum(a)/n, sum(b)/n
    cov = sum((x-ma)*(y-mb) for x, y in zip(a, b))
    va = sum((x-ma)**2 for x in a) ** 0.5
    vb = sum((y-mb)**2 for y in b) ** 0.5
    return cov/(va*vb) if va and vb else float("nan")

def spearman(a, b):
    def ranks(v):
        idx = sorted(range(len(v)), key=lambda i: v[i]); r = [0]*len(v)
        for pos, i in enumerate(idx): r[i] = pos
        return r
    return pearson(ranks(a), ranks(b))

print(f"Paridad C++ vs swarm-abm | {DAYS} días, {SEEDS} semillas/punto, {len(GRID)} puntos\n")
print(f"{"sheep_d":>8} {'fox_eff':>7} {'C++':>8} {'swarm-abm':>10} {'|Δ|':>6}")
cpp_v, ora_v = [], []
for sd, fe in GRID:
    c = mean_loss(CPP, sd, fe)
    o = mean_loss(ORACLE, sd, fe)
    cpp_v.append(c); ora_v.append(o)
    print(f"{sd:>8.2f} {fe:>7.2f} {c:>7.1f}% {o:>9.1f}% {abs(c-o):>5.1f}")

rmse = (sum((c-o)**2 for c, o in zip(cpp_v, ora_v))/len(cpp_v)) ** 0.5
mae = sum(abs(c-o) for c, o in zip(cpp_v, ora_v))/len(cpp_v)
bias = sum(c-o for c, o in zip(cpp_v, ora_v))/len(cpp_v)
print(f"\nPearson r = {pearson(cpp_v, ora_v):.4f}   Spearman rho = {spearman(cpp_v, ora_v):.4f}")
print(f"RMSE = {rmse:.2f} pp   MAE = {mae:.2f} pp   sesgo(C++-oráculo) = {bias:+.2f} pp")
print(f"rango loss: C++ [{min(cpp_v):.0f},{max(cpp_v):.0f}]  oráculo [{min(ora_v):.0f},{max(ora_v):.0f}]")
