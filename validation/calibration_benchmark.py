"""Análisis estadístico del benchmark de metaheurísticas de calibración.

Lee `models/debris-flow/data/benchmark.csv` (method, run, best_iou, evals) y
aplica las pruebas estándar para comparar optimizadores sobre múltiples
corridas (Demšar 2006):

- **Friedman**: test ómnibus no paramétrico — ¿hay diferencia entre métodos?
- **Wilcoxon signed-rank** por pares (post-hoc) con corrección de Holm.

Genera `models/debris-flow/BENCHMARK_OPTIM.md` con la tabla de ranking y los
resultados de los tests.
"""

import itertools
import pathlib

import numpy as np
import pandas as pd
from scipy import stats

HERE = pathlib.Path(__file__).parent.parent
CSV = HERE / "models/debris-flow/data/benchmark.csv"
OUT = HERE / "models/debris-flow/BENCHMARK_OPTIM.md"


def holm_correction(pairs):
    """Corrección de Holm-Bonferroni sobre una lista de (label, p)."""
    ordered = sorted(pairs, key=lambda kv: kv[1])
    m = len(ordered)
    out = {}
    running_max = 0.0
    for i, (label, p) in enumerate(ordered):
        adj = min(1.0, (m - i) * p)
        running_max = max(running_max, adj)  # monotonicidad
        out[label] = running_max
    return out


def main():
    df = pd.read_csv(CSV)
    methods = list(df["method"].unique())
    # Matriz corridas × métodos (corridas pareadas por índice de run).
    wide = df.pivot(index="run", columns="method", values="best_iou")[methods]
    n_runs = len(wide)

    # Ranking por corrida (1 = mejor, mayor IoU).
    ranks = wide.rank(axis=1, ascending=False)
    mean_rank = ranks.mean().sort_values()

    # Friedman ómnibus.
    fr_stat, fr_p = stats.friedmanchisquare(*[wide[m].to_numpy() for m in methods])

    # Wilcoxon por pares + Holm.
    raw = []
    for a, b in itertools.combinations(methods, 2):
        x, y = wide[a].to_numpy(), wide[b].to_numpy()
        try:
            _, p = stats.wilcoxon(x, y)
        except ValueError:  # todos los diffs cero
            p = 1.0
        raw.append((f"{a} vs {b}", p))
    holm = holm_correction(raw)

    lines = [
        "# Benchmark de metaheurísticas para calibrar el modelo de detritos",
        "",
        f"Comparación de {len(methods)} metaheurísticas (las mismas familias que el",
        "paper original) sobre el port Rust, con **el rigor que el costo de Python",
        "impedía**: cada método se corrió de forma independiente con el mismo",
        f"presupuesto de evaluaciones, y se comparan las distribuciones de IoU.",
        "",
        f"- Corridas independientes por método: **{n_runs}**",
        f"- Métrica: IoU máximo alcanzado por corrida (objetivo de optimización)",
        "",
        "## Ranking (IoU medio sobre corridas, mayor es mejor)",
        "",
        "| Método | IoU medio | sd | mejor | rango medio |",
        "|---|---|---|---|---|",
    ]
    order = mean_rank.index.tolist()
    for m in order:
        col = wide[m]
        lines.append(
            f"| {m} | {col.mean():.4f} | {col.std(ddof=1):.4f} | {col.max():.4f} "
            f"| {mean_rank[m]:.2f} |"
        )

    sig = "SÍ" if fr_p < 0.05 else "no"
    lines += [
        "",
        "## Test de Friedman (ómnibus)",
        "",
        f"χ² = {fr_stat:.3f}, p = {fr_p:.4f} → diferencias entre métodos: **{sig}** "
        "(α = 0.05).",
        "",
        "## Wilcoxon signed-rank por pares (p ajustado por Holm)",
        "",
        "| Par | p (Holm) | significativo |",
        "|---|---|---|",
    ]
    for label, _ in sorted(raw, key=lambda kv: holm[kv[0]]):
        p_adj = holm[label]
        lines.append(f"| {label} | {p_adj:.4f} | {'sí' if p_adj < 0.05 else 'no'} |")

    best = order[0]
    lines += [
        "",
        "## Lectura",
        "",
        f"Mejor método por IoU medio y rango: **{best}**. "
        + (
            "Friedman detecta diferencias globales; ver pares significativos arriba."
            if fr_p < 0.05
            else "Friedman NO detecta diferencias significativas: con este presupuesto "
            "los métodos son estadísticamente equivalentes (resultado en sí valioso — "
            "el original no podía afirmarlo)."
        ),
        "",
        "El valor del motor: este estudio son miles de simulaciones; en el flujo "
        "Python original (una corrida por método, presupuestos recortados por "
        "memoria) era inviable afirmar nada con respaldo estadístico.",
        "",
        "Generado por `validation/calibration_benchmark.py` desde `benchmark.csv`.",
    ]
    OUT.write_text("\n".join(lines) + "\n")
    print("\n".join(lines))


if __name__ == "__main__":
    main()
