#!/usr/bin/env bash
# Deriva un raster de disponibilidad de sedimento para Chañaral usando SurtGIS
# (el motor ráster del ecosistema) y prepara el stack `data/chanaral_twi`.
#
# Pipeline: SurtGIS hydrology all (fill→flow-dir→flow-acc→TWI) sobre el DEM,
# se normaliza el TWI a [0,1] y se usa como capa de sedimento. El raster de
# sedimento original deja la planicie urbana en NaN; el TWI la cubre y
# reactiva el entrainment del modelo.
#
# Requiere: SurtGIS compilado y python3 del sistema con GDAL.
set -euo pipefail

SURTGIS="${SURTGIS:-$HOME/proyectos/surtgis/target/release/surtgis}"
RASTERS="${RASTERS:-$HOME/proyectos/Agentes/Proyecto/rasters_fabdem_cropped}"
HERE="$(cd "$(dirname "$0")" && pwd)"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

echo "== SurtGIS: pipeline de hidrología sobre el DEM =="
"$SURTGIS" hydrology all "$RASTERS/dem_no_ocean.tif" --outdir "$WORK"

echo "== Normalizando TWI a [0,1] como disponibilidad de sedimento =="
python3 - "$WORK/twi.tif" "$WORK/sediment_twi.tif" <<'PY'
import sys
from osgeo import gdal
import numpy as np
gdal.UseExceptions()
src = gdal.Open(sys.argv[1]); twi = src.ReadAsArray().astype(np.float32)
valid = np.isfinite(twi)
p2, p98 = np.percentile(twi[valid], [2, 98])
norm = np.clip((twi - p2) / (p98 - p2), 0, 1).astype(np.float32)
norm[~valid] = 0.0
ds = gdal.GetDriverByName("GTiff").Create(
    sys.argv[2], src.RasterXSize, src.RasterYSize, 1, gdal.GDT_Float32,
    options=["COMPRESS=LZW"])
ds.SetGeoTransform(src.GetGeoTransform()); ds.SetProjection(src.GetProjection())
ds.GetRasterBand(1).WriteArray(norm); ds.FlushCache()
print(f"  TWI normalizado: clip [{p2:.1f},{p98:.1f}], mean {norm[valid].mean():.2f}")
PY

echo "== Preparando stack data/chanaral_twi =="
python3 "$HERE/prepare_chanaral.py" --rasters "$RASTERS" \
    --sediment "$WORK/sediment_twi.tif" --out "$HERE/data/chanaral_twi"
echo "✓ Stack listo: $HERE/data/chanaral_twi"
