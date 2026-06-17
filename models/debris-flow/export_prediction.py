#!/usr/bin/env python3
"""Exporta la predicción de la mejor versión del modelo (preset
`chanaral-enhanced`, IoU 0.543) como GeoTIFF georreferenciado.

Genera dos rásters en `outputs/`:
- `chanaral_prediction_best.tif`: footprint binario de la corrida de mayor IoU.
- `chanaral_prediction_prob.tif`: probabilidad de afectación (frecuencia de
  visita sobre `--runs` semillas) — mapa de ensemble, más informativo.

Requiere haber volcado los footprints con el binario (lo hace este script).
"""

import argparse
import json
import pathlib
import subprocess

import numpy as np
from osgeo import gdal

gdal.UseExceptions()

ROOT = pathlib.Path(__file__).resolve().parents[2]
HERE = pathlib.Path(__file__).parent
DATA = HERE / "data/chanaral"
OUT = HERE / "outputs"
# DEM fuente: de él tomamos CRS + geotransform para georreferenciar.
DEM_SRC = pathlib.Path.home() / "proyectos/Agentes/Proyecto/rasters_fabdem_cropped/dem_no_ocean.tif"
BIN = ROOT / "target/release/debris-flow"


def run_seed(seed, tmp):
    """Corre el modelo y vuelca el footprint; devuelve (footprint bool, IoU)."""
    out = subprocess.run(
        [str(BIN), "--preset", "chanaral-enhanced", "--steps", "500",
         "--seed", str(seed), "--seeds", "1", "--dump", str(tmp)],
        capture_output=True, text=True, cwd=ROOT,
    ).stdout
    iou = float([l for l in out.splitlines() if "IoU" in l][0].split("IoU")[1].split("|")[0])
    return iou


def write_gtiff(path, arr, dtype, nodata):
    ref = gdal.Open(str(DEM_SRC))
    drv = gdal.GetDriverByName("GTiff")
    ds = drv.Create(str(path), ref.RasterXSize, ref.RasterYSize, 1, dtype,
                    options=["COMPRESS=LZW"])
    ds.SetGeoTransform(ref.GetGeoTransform())
    ds.SetProjection(ref.GetProjection())
    band = ds.GetRasterBand(1)
    band.WriteArray(arr)
    band.SetNoDataValue(nodata)
    ds.FlushCache()
    print(f"  ✓ {path.name}  ({ref.RasterXSize}×{ref.RasterYSize}, {ref.GetProjection()[:31]}…)")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--runs", type=int, default=20)
    args = ap.parse_args()
    OUT.mkdir(exist_ok=True)
    meta = json.load(open(DATA / "meta.json"))
    W, H = meta["width"], meta["height"]

    acc = np.zeros((H, W), dtype=np.float32)
    best_iou, best_fp = -1.0, None
    tmp = OUT / "_tmp_fp.bin"
    print(f"→ {args.runs} corridas (preset chanaral-enhanced)…")
    for seed in range(100, 100 + args.runs):
        iou = run_seed(seed, tmp)
        fp = np.fromfile(tmp, dtype=np.uint8).reshape(H, W)
        acc += fp
        if iou > best_iou:
            best_iou, best_fp = iou, fp.copy()
    tmp.unlink(missing_ok=True)

    prob = acc / args.runs
    print(f"→ mejor IoU {best_iou:.3f} | ensemble de {args.runs} corridas")
    write_gtiff(OUT / "chanaral_prediction_best.tif", best_fp.astype(np.uint8),
                gdal.GDT_Byte, 0)
    write_gtiff(OUT / "chanaral_prediction_prob.tif", prob.astype(np.float32),
                gdal.GDT_Float32, 0.0)
    print(f"\n✓ Rásters en {OUT}/ (EPSG de la cuenca, listos para GIS)")


if __name__ == "__main__":
    main()
