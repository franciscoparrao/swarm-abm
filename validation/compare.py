"""Compara los ensambles swarm-core (Rust) vs Mesa y genera REPORT.md.

La paridad es distribucional: cada motor corre N réplicas con semillas
distintas (los RNG difieren, no hay paridad bit a bit posible). Para cada
métrica se aplica un test z de dos muestras sobre las medias de ensamble:
PASS si |z| < 1.96 (α = 0.05). Las curvas se comparan tras rellenar cada
réplica con su último valor hasta el horizonte común (válido: tras el
término, el estado del modelo es constante).
"""

import math
import pathlib
import sys

import pandas as pd

DATA = pathlib.Path(__file__).parent / "data"
REPORT = pathlib.Path(__file__).parent / "REPORT.md"


def load_ensemble(engine, model, n_seeds):
    """Lista de DataFrames, uno por semilla."""
    runs = []
    for seed in range(n_seeds):
        path = DATA / f"{engine}_{model}_{seed}.csv"
        if not path.exists():
            sys.exit(f"falta {path} — corre run_validation.sh completo")
        runs.append(pd.read_csv(path))
    return runs


def pad_to(df, horizon):
    """Extiende la serie hasta `horizon` repitiendo la última fila."""
    if len(df) >= horizon + 1:
        return df.iloc[: horizon + 1].reset_index(drop=True)
    last = df.iloc[-1]
    extra = pd.DataFrame([last] * (horizon + 1 - len(df)))
    out = pd.concat([df, extra], ignore_index=True)
    out["step"] = range(horizon + 1)
    return out


def mean_curves(runs, columns, horizon):
    padded = [pad_to(df, horizon) for df in runs]
    stacked = pd.concat(padded).groupby("step")
    return {c: stacked[c].mean() for c in columns}


def ztest(a, b):
    """Test z de dos muestras sobre listas de valores escalares por réplica."""
    na, nb = len(a), len(b)
    ma = sum(a) / na
    mb = sum(b) / nb
    va = sum((x - ma) ** 2 for x in a) / (na - 1)
    vb = sum((x - mb) ** 2 for x in b) / (nb - 1)
    se = math.sqrt(va / na + vb / nb)
    z = 0.0 if se == 0 else (ma - mb) / se
    return ma, mb, z


def metric_rows(name_metrics, rust_runs, mesa_runs):
    rows = []
    for name, extract in name_metrics:
        rust_vals = [extract(df) for df in rust_runs]
        mesa_vals = [extract(df) for df in mesa_runs]
        m_rust, m_mesa, z = ztest(rust_vals, mesa_vals)
        ok = abs(z) < 1.96
        rows.append((name, m_rust, m_mesa, z, ok))
    return rows


def fmt_table(rows):
    out = ["| Métrica | swarm-core | Mesa | z | Paridad |", "|---|---|---|---|---|"]
    for name, mr, mm, z, ok in rows:
        verdict = "✅ PASS" if ok else "❌ FAIL"
        out.append(f"| {name} | {mr:.4f} | {mm:.4f} | {z:+.2f} | {verdict} |")
    return "\n".join(out)


def curve_diff_table(rust_runs, mesa_runs, columns, horizon):
    rust_curves = mean_curves(rust_runs, columns, horizon)
    mesa_curves = mean_curves(mesa_runs, columns, horizon)
    out = ["| Serie | max \\|Δ media de ensamble\\| |", "|---|---|"]
    for c in columns:
        diff = (rust_curves[c] - mesa_curves[c]).abs().max()
        out.append(f"| {c} | {diff:.4f} |")
    return "\n".join(out)


def main():
    n_seeds = int(sys.argv[1]) if len(sys.argv) > 1 else 20
    sections = []
    all_ok = True

    # --- Schelling ---
    rust = load_ensemble("rust", "schelling", n_seeds)
    mesa = load_ensemble("mesa", "schelling", n_seeds)
    metrics = [
        ("Pasos hasta converger", lambda df: df["step"].iloc[-1]),
        ("Similitud media final", lambda df: df["similitud_media"].iloc[-1]),
        ("Fracción conforme inicial", lambda df: df["fraccion_conforme"].iloc[0]),
    ]
    rows = metric_rows(metrics, rust, mesa)
    all_ok &= all(r[4] for r in rows)
    horizon = 30
    sections.append(
        "## Schelling (50×50, densidad 0.85, tolerancia 0.375)\n\n"
        + fmt_table(rows)
        + "\n\nCurvas (medias de ensamble, horizonte 30 pasos, relleno con último valor):\n\n"
        + curve_diff_table(rust, mesa, ["fraccion_conforme", "similitud_media"], horizon)
    )

    # --- SIR ---
    rust = load_ensemble("rust", "sir", n_seeds)
    mesa = load_ensemble("mesa", "sir", n_seeds)
    metrics = [
        ("Pico de infectados (fracción)", lambda df: df["i"].max()),
        ("Paso del pico", lambda df: df["i"].idxmax()),
        ("Tamaño final de la epidemia (R)", lambda df: df["r"].iloc[-1]),
        ("Duración (pasos)", lambda df: df["step"].iloc[-1]),
    ]
    rows = metric_rows(metrics, rust, mesa)
    all_ok &= all(r[4] for r in rows)
    horizon = 300
    sections.append(
        "## SIR espacial (50×50 lleno, β=0.08, γ=0.1, 5 infectados iniciales)\n\n"
        + fmt_table(rows)
        + "\n\nCurvas (medias de ensamble, horizonte 300 pasos, relleno con último valor):\n\n"
        + curve_diff_table(rust, mesa, ["s", "i", "r"], horizon)
    )

    verdict = "✅ PARIDAD NUMÉRICA CONFIRMADA" if all_ok else "❌ HAY MÉTRICAS FUERA DE PARIDAD"
    report = (
        "# Paridad numérica: swarm-core (Rust) vs Mesa (Python)\n\n"
        f"Réplicas por motor: **{n_seeds}** (semillas 0..{n_seeds - 1}). "
        "Test z de dos muestras por métrica, α = 0.05.\n\n"
        f"**Veredicto: {verdict}**\n\n" + "\n\n".join(sections) + "\n\n"
        "Generado por `validation/compare.py` "
        "(datos en `validation/data/`, espejos Mesa en `validation/mesa/`).\n"
    )
    REPORT.write_text(report)
    print(report)
    sys.exit(0 if all_ok else 1)


if __name__ == "__main__":
    main()
