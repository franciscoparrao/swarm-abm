#!/usr/bin/env bash
# Benchmark de escalamiento: kernel depredador-presa idéntico en C++ (a mano,
# cell list) y en Rust (motor swarm-abm, ContinuousSpace). Mismas reglas, mismo
# hardware, misma metodología. Mide ms/paso (mediana de REPS réplicas) al
# escalar N agentes en un solo nodo.
set -u
cd "$(dirname "$0")"

REPS=${REPS:-7}
SIZES=${SIZES:-"400 1600 6400 25600 40000"}
SEED=1

echo ">> Compilando C++ (g++ -O3 -march=native -flto)..."
g++ -std=c++17 -O3 -march=native -flto cpp/prey_predator.cpp -o cpp/prey_predator

echo ">> Compilando Rust (swarm-abm 0.4, opt-level 3 + LTO + target-cpu=native)..."
( cd rust && RUSTFLAGS="-C target-cpu=native" cargo build --release --quiet )

median() { python3 -c "import sys; v=sorted(float(x) for x in sys.argv[1:]); n=len(v); print('%.5f'%(v[n//2] if n%2 else (v[n//2-1]+v[n//2])/2))" "$@"; }

printf "\n%-9s %-8s %14s %14s %10s\n" "N" "steps" "C++ ms/paso" "swarm-abm ms/p" "C++ mas rap"
printf -- "------------------------------------------------------------\n"
for N in $SIZES; do
  # pasos adaptativos: trabajo ~constante para que cada corrida dure decenas de ms
  STEPS=$(python3 -c "print(max(40, 4000000//$N))")
  cpp_v=(); rust_v=()
  # warmup
  ./cpp/prey_predator "$N" "$STEPS" "$SEED" >/dev/null
  ./rust/target/release/prey_predator "$N" "$STEPS" "$SEED" >/dev/null
  for r in $(seq 1 "$REPS"); do
    cpp_v+=( "$(./cpp/prey_predator "$N" "$STEPS" "$SEED" | awk '{print $4}')" )
    rust_v+=( "$(./rust/target/release/prey_predator "$N" "$STEPS" "$SEED" | awk '{print $4}')" )
  done
  cpp_m=$(median "${cpp_v[@]}")
  rust_m=$(median "${rust_v[@]}")
  ratio=$(python3 -c "print('%.2f'%($rust_m/$cpp_m))")
  printf "%-9s %-8s %14s %14s %9sx\n" "$N" "$STEPS" "$cpp_m" "$rust_m" "$ratio"
done
