"""Agrega validation/data/bench.csv y genera validation/BENCHMARKS.md.

Metodología: SIR espacial idéntico en ambos motores (ver REPORT.md para la
paridad numérica), medición en proceso de la fase de simulación (excluye
setup y recolección de métricas), mediana de ms/paso entre réplicas.
"""

import pathlib
import platform
import subprocess

import pandas as pd

HERE = pathlib.Path(__file__).parent
CSV = HERE / "data" / "bench.csv"
OUT = HERE / "BENCHMARKS.md"


def cpu_model():
    try:
        for line in open("/proc/cpuinfo"):
            if line.startswith("model name"):
                return line.split(":", 1)[1].strip()
    except OSError:
        pass
    return platform.processor() or "desconocido"


def tool_version(cmd):
    try:
        return subprocess.run(cmd, capture_output=True, text=True).stdout.strip()
    except OSError:
        return "?"


def main():
    df = pd.read_csv(CSV)
    df["ms_step"] = df["ms"] / df["steps"]

    g = (
        df.groupby(["side", "agents", "engine"])["ms_step"]
        .median()
        .unstack("engine")
        .reset_index()
        .sort_values("side")
    )
    g["speedup"] = g["mesa"] / g["rust"]
    g["rust_agentsteps_s"] = g["agents"] / (g["rust"] / 1000.0)

    lines = [
        "# Benchmarks: swarm-core (Rust) vs Mesa (Python)",
        "",
        "Modelo: SIR espacial idéntico en ambos motores (paridad numérica",
        "verificada en `REPORT.md`). Se mide solo la fase de simulación, en",
        "proceso, sin reporters; mediana de ms/paso sobre réplicas con",
        "semillas distintas. β=0.08, γ=0.1, 5 infectados iniciales, máx. 300",
        "pasos, vecindad Moore, torus.",
        "",
        "| Grilla | Agentes | Rust (ms/paso) | Mesa (ms/paso) | Speedup | Rust (agente-pasos/s) |",
        "|---|---|---|---|---|---|",
    ]
    for _, r in g.iterrows():
        lines.append(
            f"| {r.side:.0f}×{r.side:.0f} | {r.agents:,.0f} | {r.rust:.3f} "
            f"| {r.mesa:.2f} | **{r.speedup:.0f}×** | {r.rust_agentsteps_s:,.0f} |"
        )

    n_seeds = df["seed"].nunique()
    lines += [
        "",
        f"Réplicas por celda: {n_seeds}. NetLogo queda pendiente (requiere "
        "instalación JVM); los microbenchmarks del motor (escalamiento de "
        "caminantes a 1M de agentes, Life simultáneo, `diffuse`) corren con "
        "criterion: `cargo bench -p swarm-core`.",
        "",
        "## Entorno",
        "",
        f"- CPU: {cpu_model()}",
        f"- OS: {platform.system()} {platform.release()}",
        f"- Rust: {tool_version(['rustc', '--version'])} (perfil release: lto, codegen-units=1)",
        f"- Python: {platform.python_version()}, Mesa: "
        + tool_version(
            [str(HERE / '.venv/bin/python'), '-c', 'import mesa; print(mesa.__version__)']
        ),
        "",
        "Generado por `validation/bench_report.py` desde `validation/data/bench.csv`.",
    ]
    OUT.write_text("\n".join(lines) + "\n")
    print("\n".join(lines))


if __name__ == "__main__":
    main()
