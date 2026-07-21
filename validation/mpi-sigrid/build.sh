#!/bin/bash
# Compila la versión BSPonMPI del modelo SIGRID.
#
# Requiere BSPonMPI instalado (BSPlib sobre MPI). Para obtenerlo:
#   git clone https://github.com/wijnand-suijlen/bsponmpi.git
#   cd bsponmpi && ./configure --prefix=$HOME/.local/bsponmpi && make install
# y exportar BSP_PREFIX=$HOME/.local/bsponmpi (o pasarlo como 1er argumento).
#
# CRÍTICO: -ffp-contract=off. Sin él, bspcxx y g++ fusionan `a*b+c` en FMA de
# forma distinta -> ~1 ULP de diferencia que rompe la bit-identidad con el oráculo
# serial/OpenMP (ver README, "hallazgo de contracción FP").
set -e
BSP_PREFIX="${1:-${BSP_PREFIX:-$HOME/.local/bsponmpi}}"
BSPCXX="$BSP_PREFIX/bin/bspcxx"
if [ ! -x "$BSPCXX" ]; then echo "No encuentro bspcxx en $BSPCXX. Instala BSPonMPI y pasa su prefijo."; exit 1; fi
cd "$(dirname "$0")"
FLAGS="-std=c++17 -O3 -march=native -ffp-contract=off"
echo "Compilando sheep_fox_bsp con $BSPCXX ..."
"$BSPCXX" $FLAGS sheep_fox_bsp.cpp -o sheep_fox_bsp
echo "OK -> $(pwd)/sheep_fox_bsp"
echo "Correr: $BSP_PREFIX/bin/bsprun -n <P> ./sheep_fox_bsp --width 8000 --height 8000 --days 30 --seed 1000"
