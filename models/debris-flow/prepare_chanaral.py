#!/usr/bin/env python3
"""Convierte el stack de Chañaral (modelo coastal, el mejor caso) a raw
Float32 + meta.json para el modelo Rust. A diferencia de Copiapó, el ground
truth y el bounding box de evaluación son rásters (no shapefiles).

Uso: python3 prepare_chanaral.py [--rasters DIR] [--results DIR] [--out DIR]
"""

import argparse
import json
import pathlib

import numpy as np
from osgeo import gdal

gdal.UseExceptions()

# Mismos archivos que la calibración que produjo Config B
# (calibrate_v2_full_metaheuristics.py): DEM con el océano enmascarado, clave
# en una cuenca costera como Chañaral para que los flujos no se derramen.
RASTERS = {
    "dem": "dem_no_ocean.tif",
    "slope": "slope.tif",
    "rain_dia1": "rain_dia1.tif",
    "rain_dia2": "rain_dia2.tif",
    "rain_dia3": "rain_dia3.tif",
    "isotherm": "isotherm.tif",
    "sediment": "sediment.tif",
    "susceptibility": "susceptibility.tif",
    "streams": "streams_fabdem.tif",
}


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument(
        "--rasters",
        type=pathlib.Path,
        default=pathlib.Path.home() / "proyectos/Agentes/Proyecto/rasters_fabdem_cropped",
    )
    ap.add_argument(
        "--results",
        type=pathlib.Path,
        default=pathlib.Path.home() / "proyectos/Agentes/resultados",
        help="dir con area_real_afectada.tif y bbox_mask.tif",
    )
    ap.add_argument(
        "--out", type=pathlib.Path, default=pathlib.Path(__file__).parent / "data/chanaral"
    )
    ap.add_argument(
        "--sediment", type=pathlib.Path, default=None,
        help="override del raster de sedimento (p.ej. el TWI de SurtGIS)",
    )
    args = ap.parse_args()
    if args.sediment is not None:
        RASTERS["sediment"] = str(args.sediment)
    args.out.mkdir(parents=True, exist_ok=True)

    ref = gdal.Open(str(args.rasters / RASTERS["dem"]))
    width, height = ref.RasterXSize, ref.RasterYSize
    gt = ref.GetGeoTransform()
    pixel_size = (abs(gt[1]) + abs(gt[5])) / 2.0

    for name, src in RASTERS.items():
        ds = gdal.Open(str(args.rasters / src))
        assert (ds.RasterXSize, ds.RasterYSize) == (width, height), f"{name} desalineado"
        arr = ds.ReadAsArray().astype(np.float32)
        arr.tofile(args.out / f"{name}.f32")
        print(f"  {name}: {arr.shape} -> {name}.f32")

    # Ground truth: area_real_afectada == 1.
    area = gdal.Open(str(args.results / "area_real_afectada.tif")).ReadAsArray()
    assert area.shape == (height, width), "area_real_afectada desalineado con el DEM"
    gt_arr = (area == 1).astype(np.float32)
    gt_arr.tofile(args.out / "ground_truth.f32")
    print(f"  ground truth: {int(gt_arr.sum()):,} píxeles afectados")

    # Bounding box de evaluación.
    bbox = gdal.Open(str(args.results / "bbox_mask.tif")).ReadAsArray()
    assert bbox.shape == (height, width), "bbox_mask desalineado con el DEM"
    bbox_arr = (bbox > 0).astype(np.float32)
    bbox_arr.tofile(args.out / "bbox.f32")
    print(f"  bbox: {int(bbox_arr.sum()):,} píxeles | afectado∩bbox: "
          f"{int(((area == 1) & (bbox > 0)).sum()):,}")

    # Ventana = bounding rectangular del bbox (acota el barrido de evaluación).
    rows = np.where(bbox_arr.any(axis=1))[0]
    cols = np.where(bbox_arr.any(axis=0))[0]
    window = {
        "row_start": int(rows.min()),
        "row_end": int(rows.max()) + 1,
        "col_start": int(cols.min()),
        "col_end": int(cols.max()) + 1,
    }
    print(f"  window: {window}")

    meta = {
        "width": width,
        "height": height,
        "pixel_size": pixel_size,
        "window": window,
        "layers": list(RASTERS) + ["ground_truth", "bbox"],
    }
    (args.out / "meta.json").write_text(json.dumps(meta, indent=2))
    print(f"✓ Datos en {args.out} (pixel_size={pixel_size:.1f} m)")


if __name__ == "__main__":
    main()
