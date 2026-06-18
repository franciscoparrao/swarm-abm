"""Demo de los bindings Python de swarm-abm.

Construir e instalar el módulo (en un venv):

    python -m venv .venv && . .venv/bin/activate
    pip install maturin
    cd crates/swarm-py && maturin develop --release

Luego: `python demo.py`. Solo usa la librería estándar; con pandas/numpy/
matplotlib en el entorno, las series y el barrido se grafican directamente.
"""

import swarm_abm as sw


def curva_epidemica():
    """Corre un SIR y muestra la curva de infectados (todo el bucle en Rust)."""
    m = sw.Sir(size=200, beta=0.15, gamma=0.1, initial_infected=20, seed=42)
    pasos = m.run(500)
    infect = m.series("i")
    pico = max(infect)
    print(f"SIR 200x200 | pasos={pasos} | pico I={pico * 100:.1f}% "
          f"| R final={m.recovered * 100:.1f}%")
    # Mini-curva ASCII de infectados.
    ancho = 50
    for paso, frac in enumerate(infect):
        if paso % 10 == 0:
            barra = "#" * round(frac / pico * ancho) if pico else ""
            print(f"  t={paso:>3} {barra} {frac * 100:.1f}%")


def barrido_beta():
    """Barrido paralelo de beta (rayon, GIL liberado), agregado en Python."""
    betas = [0.05, 0.10, 0.15, 0.20, 0.30]
    seeds = list(range(30))
    rows = sw.sir_sweep(betas=betas, seeds=seeds, size=120, max_steps=500)
    print(f"\nBarrido: {len(rows)} réplicas ({len(betas)} betas × {len(seeds)} seeds)")

    agg = {b: [] for b in betas}
    for beta, _seed, peak, _rfin in rows:
        agg[beta].append(peak)
    print("  beta  pico_medio  (±sd)")
    for beta in betas:
        v = agg[beta]
        media = sum(v) / len(v)
        sd = (sum((x - media) ** 2 for x in v) / len(v)) ** 0.5
        print(f"  {beta:>4}  {media * 100:>9.1f}%  ±{sd * 100:.1f}")

    # Con pandas sería simplemente:
    #   import pandas as pd
    #   df = pd.DataFrame(rows, columns=["beta", "seed", "peak", "r_final"])
    #   df.groupby("beta")["peak"].agg(["mean", "std"])


if __name__ == "__main__":
    print(f"swarm_abm {sw.__version__}\n")
    curva_epidemica()
    barrido_beta()
