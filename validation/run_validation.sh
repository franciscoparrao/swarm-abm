#!/usr/bin/env bash
# Paridad numérica swarm-core vs Mesa: corre N réplicas en ambos motores
# y genera validation/REPORT.md. Requiere validation/.venv con mesa+pandas.
set -euo pipefail

SEEDS="${1:-20}"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DATA="$ROOT/validation/data"
PY="$ROOT/validation/.venv/bin/python"

mkdir -p "$DATA"

echo "== Compilando ejemplos Rust (release) =="
cargo build --release -p schelling -p sir --manifest-path "$ROOT/Cargo.toml"

echo "== Réplicas swarm-core (Rust) =="
for s in $(seq 0 $((SEEDS - 1))); do
  "$ROOT/target/release/schelling" "$s" --csv > "$DATA/rust_schelling_$s.csv"
  "$ROOT/target/release/sir" "$s" --csv --width 50 --height 50 --infected 5 --steps 300 \
    > "$DATA/rust_sir_$s.csv"
done
echo "rust: $SEEDS réplicas de cada modelo listas"

echo "== Réplicas Mesa (Python) =="
"$PY" "$ROOT/validation/mesa/schelling_mesa.py" --seeds "$SEEDS" --out "$DATA"
"$PY" "$ROOT/validation/mesa/sir_mesa.py" --seeds "$SEEDS" --out "$DATA" \
  --width 50 --height 50 --infected 5 --steps 300

echo "== Comparación =="
"$PY" "$ROOT/validation/compare.py" "$SEEDS"
