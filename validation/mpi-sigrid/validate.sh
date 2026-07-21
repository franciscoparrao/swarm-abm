#!/bin/bash
# Validación de bit-identidad: la versión BSPonMPI (a P=2 y P=4) debe dar EXACTAMENTE
# el mismo loss_rate/matadas/intentos que el oráculo serial, en todo el barrido de
# screening (todas las especies). Ambos binarios se compilan con -ffp-contract=off.
set -e
BSP_PREFIX="${1:-${BSP_PREFIX:-$HOME/.local/bsponmpi}}"
cd "$(dirname "$0")"
FC="-ffp-contract=off"
echo "Compilando oráculo serial y BSP (-ffp-contract=off)..."
g++ -std=c++17 -O3 -march=native $FC ../cpp-sigrid/sheep_fox.cpp -o /tmp/sf_oracle
"$BSP_PREFIX/bin/bspcxx" -std=c++17 -O3 -march=native $FC sheep_fox_bsp.cpp -o sheep_fox_bsp
BSPRUN="$BSP_PREFIX/bin/bsprun"

CFGS=(
  "--width 4000 --height 4000 --days 10 --seed 1000"                                             # sin perros (guarda activa)
  "--width 4000 --height 4000 --days 10 --seed 2000 --dogs 3"                                    # perros
  "--width 4000 --height 4000 --days 10 --seed 1000 --hare-density 4"                            # liebres
  "--width 4000 --height 4000 --days 10 --seed 1000 --chilla-density 3"                          # chillas
  "--width 5000 --height 5000 --days 8  --seed 3000 --dogs 2 --hare-density 3 --chilla-density 2" # todo junto
)
pass=0; fail=0
for C in "${CFGS[@]}"; do
  s=$(/tmp/sf_oracle $C 2>/dev/null | grep -o "loss_rate.*intentos [0-9]*")
  b2=$($BSPRUN -n 2 ./sheep_fox_bsp $C 2>/dev/null | grep -o "loss_rate.*intentos [0-9]*")
  b4=$($BSPRUN -n 4 ./sheep_fox_bsp $C 2>/dev/null | grep -o "loss_rate.*intentos [0-9]*")
  if [ "$s" = "$b2" ] && [ "$s" = "$b4" ]; then echo "✓ serial==P2==P4  [$s]  <= $C"; pass=$((pass+1));
  else echo "✗ serial=[$s] P2=[$b2] P4=[$b4]  <= $C"; fail=$((fail+1)); fi
done
echo "=== $pass OK / $fail FALLA ==="
