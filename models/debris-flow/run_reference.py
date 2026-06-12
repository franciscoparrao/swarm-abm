#!/usr/bin/env python3
"""Corre el modelo Python ORIGINAL (simulate_copiapo.py, sin modificar) sobre
el stack .f32 preparado, con la misma evaluación que el port Rust. Sirve de
referencia empírica para la paridad.

Uso: validation/.venv/bin/python models/debris-flow/run_reference.py \
        [--preset 18iters|optuna-t] [--agents N] [--steps N] [--runs N]
"""

import argparse
import json
import pathlib
import re
import time

import numpy as np

HERE = pathlib.Path(__file__).parent
DATA = HERE / "data/copiapo"
SRC = pathlib.Path.home() / "proyectos/debris-flow-abm/src/simulate_copiapo.py"

PRESET_18ITERS = {
    "rain_threshold": 0.13740758554107335,
    "sediment_threshold": 0.0684985686772649,
    "susceptibility_threshold": 0.36351725848573185,
    "friction_coefficient": 0.05278332008639008,
    "coastal_slope_threshold": 0.08885858007669371,
    "coastal_spread_factor": 4.086548259278382,
    "coastal_volume_threshold": 0.29506403616822596,
    "volume_decay_flat": 0.9771987820675018,
    "volume_decay_slope": 0.9897171396434301,
    "stream_attraction_weight": 2.827551022612925,
    "max_velocity": 28.857071411159623,
    "min_velocity": 0.6389789198396825,
    "critical_slope": 0.07253064397357342,
    "slope_acceleration_factor": 1.880467839015258,
    "stochastic_temperature": 0.0,
}

PRESET_OPTUNA_T = {
    "rain_threshold": 0.14419084613889552,
    "sediment_threshold": 0.24529086745327414,
    "susceptibility_threshold": 0.3157827201735703,
    "friction_coefficient": 0.03340256328354208,
    "coastal_slope_threshold": 0.05058152108995141,
    "coastal_spread_factor": 2.9224787720070977,
    "coastal_volume_threshold": 0.5999101191718836,
    "volume_decay_flat": 0.9689818263755272,
    "volume_decay_slope": 0.9803457730415996,
    "stream_attraction_weight": 1.915163555345845,
    "max_velocity": 23.585956684109266,
    "min_velocity": 0.45380131952627,
    "critical_slope": 0.0100850782208226,
    "slope_acceleration_factor": 1.9773728422635193,
    "stochastic_temperature": 0.28474034053018205,
}


class FakeTransform:
    a = 30.0
    e = -30.0


class FakeSrc:
    """Shim mínimo de rasterio.open para load_raster del modelo original."""

    def __init__(self, path, shape):
        self._path = path
        self._shape = shape
        self.transform = FakeTransform()

    def __enter__(self):
        return self

    def __exit__(self, *a):
        return False

    def read(self, _band):
        return np.fromfile(self._path, dtype=np.float32).reshape(self._shape)


def load_model_classes(shape):
    """Extrae las clases del script original SIN ejecutar su main."""
    import mesa

    src = SRC.read_text()
    classes = re.search(r"(class PixelTerrainModel.*?)\n# =+\n# SCRIPT", src, re.S).group(1)

    class FakeRasterio:
        @staticmethod
        def open(path):
            return FakeSrc(path, shape)

    ns = {"mesa": mesa, "np": np, "rasterio": FakeRasterio}
    exec(classes, ns)  # noqa: S102 — código propio, verificado idéntico al repo
    return ns["PixelTerrainModel"], ns["FlowPixelAgent"]


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--preset", default="18iters", choices=["18iters", "optuna-t"])
    ap.add_argument("--agents", type=int, default=100)
    ap.add_argument("--steps", type=int, default=500)
    ap.add_argument("--runs", type=int, default=1)
    args = ap.parse_args()

    meta = json.loads((DATA / "meta.json").read_text())
    w, h = meta["width"], meta["height"]
    win = meta["window"]
    params = PRESET_18ITERS if args.preset == "18iters" else PRESET_OPTUNA_T

    PixelTerrainModel, FlowPixelAgent = load_model_classes((h, w))
    gt = np.fromfile(DATA / "ground_truth.f32", dtype=np.float32).reshape(h, w) > 0
    gt_win = gt[win["row_start"] : win["row_end"], win["col_start"] : win["col_end"]]

    for run in range(args.runs):
        t0 = time.time()
        model = PixelTerrainModel(
            elevation_file=str(DATA / "dem.f32"),
            rain_raster_files=[str(DATA / f"rain_dia{d}.f32") for d in (1, 2, 3)],
            slope_file=str(DATA / "slope.f32"),
            rain_snow_file=str(DATA / "isotherm.f32"),
            sediment_file=str(DATA / "sediment.f32"),
            susceptibility_file=str(DATA / "susceptibility.f32"),
            streams_file=str(DATA / "streams.f32"),
            n_rain_agents=args.agents,
            **params,
            seed=42 + run,
        )
        for _ in range(args.steps):
            model.step()
        sim_s = time.time() - t0

        flow_map = np.zeros((h, w), dtype=np.float32)
        n_flows = 0
        hist_lens = []
        for agent in model.agents:
            if isinstance(agent, FlowPixelAgent):
                hist_lens.append(len(agent.history))
                for x, y, radius in agent.history:
                    if 0 <= x < w and 0 <= y < h:
                        y0, y1 = max(0, int(y - radius)), min(h, int(y + radius + 1))
                        x0, x1 = max(0, int(x - radius)), min(w, int(x + radius + 1))
                        yy, xx = np.mgrid[y0:y1, x0:x1]
                        d = np.sqrt((xx - x) ** 2 + (yy - y) ** 2)
                        wgt = np.maximum(0, 1.0 - d / radius)
                        flow_map[y0:y1, x0:x1] += np.where(d <= radius, wgt, 0) * agent.initial_volume
                n_flows += 1

        pred = (
            flow_map[win["row_start"] : win["row_end"], win["col_start"] : win["col_end"]] > 0
        )
        tp = int((pred & gt_win).sum())
        fp = int((pred & ~gt_win).sum())
        fn = int((~pred & gt_win).sum())
        iou = tp / (tp + fp + fn)
        print(
            f"PYTHON run {run}: IoU {iou:.4f} | precision {tp / (tp + fp):.3f} | "
            f"recall {tp / (tp + fn):.3f} | flujos {n_flows} | "
            f"área pred {(tp + fp) * 900 / 1e6:.1f} km² | "
            f"historia media {np.mean(hist_lens):.1f} | sim {sim_s:.0f}s",
            flush=True,
        )


if __name__ == "__main__":
    main()
