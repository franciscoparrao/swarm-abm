#!/usr/bin/env python3
"""Convierte los rasters de Copiapó a raw Float32 + meta.json para el
modelo Rust (models/debris-flow). Requiere GDAL (osgeo) del sistema.

Uso: python3 prepare_data.py [--rasters DIR] [--out DIR]
"""

import argparse
import json
import pathlib
import struct

import numpy as np
from osgeo import gdal, ogr

gdal.UseExceptions()

RASTERS = {
    # nombre destino -> archivo fuente (mismos insumos que simulate_copiapo.py
    # vía calibrate_copiapo_optuna_with_T.py)
    "dem": "dem_filled_copiapo.tif",
    "slope": "slope_copiapo_fraction.tif",
    "rain_dia1": "rain_dia1_copiapo.tif",
    "rain_dia2": "rain_dia2_copiapo.tif",
    "rain_dia3": "rain_dia3_copiapo.tif",
    "isotherm": "isotherm_copiapo.tif",
    "sediment": "disponibilidad_sedimentos_copiapo_norm.tif",
    "susceptibility": "susceptibilidad_3subcuencas_copiapo_resampled.tif",
    "streams": "streams_copiapo.tif",
}


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument(
        "--rasters",
        type=pathlib.Path,
        default=pathlib.Path.home() / "proyectos/Agentes/Proyecto/rasters_copiapo",
    )
    ap.add_argument(
        "--out", type=pathlib.Path, default=pathlib.Path(__file__).parent / "data/copiapo"
    )
    args = ap.parse_args()
    args.out.mkdir(parents=True, exist_ok=True)

    ref = gdal.Open(str(args.rasters / RASTERS["dem"]))
    width, height = ref.RasterXSize, ref.RasterYSize
    gt = ref.GetGeoTransform()  # (x0, dx, 0, y0, 0, -dy)

    for name, src_name in RASTERS.items():
        ds = gdal.Open(str(args.rasters / src_name))
        assert (ds.RasterXSize, ds.RasterYSize) == (width, height), f"{name} desalineado"
        # Igual que el modelo Python: astype(float) SIN enmascarar nodata.
        arr = ds.ReadAsArray().astype(np.float32)
        arr.tofile(args.out / f"{name}.f32")
        print(f"  {name}: {arr.shape} -> {name}.f32")

    # Ground truth: polígonos 'Área afectada' rasterizados sobre la grilla del DEM.
    gt_path = args.rasters / "ground_truth_copiapo.shp"
    src_vec = ogr.Open(str(gt_path))
    layer = src_vec.GetLayer()
    layer.SetAttributeFilter("NOMBRE = 'Área afectada'")
    print(f"  ground truth: {layer.GetFeatureCount()} polígonos 'Área afectada'")

    mem = gdal.GetDriverByName("MEM").Create("", width, height, 1, gdal.GDT_Float32)
    mem.SetGeoTransform(gt)
    mem.SetProjection(ref.GetProjection())
    mem.GetRasterBand(1).Fill(0)
    gdal.RasterizeLayer(mem, [1], layer, burn_values=[1])
    gt_arr = mem.ReadAsArray().astype(np.float32)
    gt_arr.tofile(args.out / "ground_truth.f32")
    print(f"  ground truth: {int(gt_arr.sum()):,} píxeles afectados")

    # Window del bbox (igual que rasterio from_bounds + int truncation).
    bbox_ds = ogr.Open(str(args.rasters / "bbox_copiapo.shp"))
    bbox_layer = bbox_ds.GetLayer()
    minx, maxx, miny, maxy = bbox_layer.GetExtent()
    col_off = (minx - gt[0]) / gt[1]
    row_off = (maxy - gt[3]) / gt[5]
    win_w = (maxx - minx) / gt[1]
    win_h = (miny - maxy) / gt[5]
    window = {
        "row_start": int(row_off),
        "row_end": int(row_off + win_h),
        "col_start": int(col_off),
        "col_end": int(col_off + win_w),
    }
    print(f"  window: {window}")

    meta = {
        "width": width,
        "height": height,
        "pixel_size": (abs(gt[1]) + abs(gt[5])) / 2.0,
        "window": window,
        "layers": list(RASTERS) + ["ground_truth"],
    }
    (args.out / "meta.json").write_text(json.dumps(meta, indent=2))
    print(f"✓ Datos en {args.out}")


if __name__ == "__main__":
    main()
