#!/usr/bin/env bash
# Benchmark cross-engine SIR: swarm-core (Rust) vs Mesa (Python) a varias
# escalas. Genera validation/data/bench.csv y validation/BENCHMARKS.md.
set -euo pipefail

SIZES="${SIZES:-25 50 100 200}"
SEEDS="${SEEDS:-0 1 2}"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DATA="$ROOT/validation/data"
PY="$ROOT/validation/.venv/bin/python"
CSV="$DATA/bench.csv"

mkdir -p "$DATA"

echo "== Compilando (release) =="
cargo build --release -p sir --manifest-path "$ROOT/Cargo.toml"

echo "engine,side,agents,seed,steps,ms" > "$CSV"

for side in $SIZES; do
  for seed in $SEEDS; do
    out=$("$ROOT/target/release/sir" "$seed" --bench \
      --width "$side" --height "$side" --infected 5 --steps 300 | tail -1)
    echo "rust,$side,$((side * side)),$seed,$out" >> "$CSV"
    echo "rust  ${side}x${side} seed $seed: $out"

    out=$("$PY" "$ROOT/validation/mesa/sir_mesa.py" --bench "$seed" \
      --width "$side" --height "$side" --infected 5 --steps 300 2>/dev/null | tail -1)
    echo "mesa,$side,$((side * side)),$seed,$out" >> "$CSV"
    echo "mesa  ${side}x${side} seed $seed: $out"
  done
done

echo "== Reporte =="
"$PY" "$ROOT/validation/bench_report.py"
