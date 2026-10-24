#!/usr/bin/env python3
"""Barrido de `trigger_drops_per_km2` contra los puntos del catálogo (punto 3,
decisión 3 del PROTOCOLO_CALIBRACION_GATILLO.md), con la regla de elección
declarada por nowcast el 2026-10-23 ANTES de ver resultados:

    máximo lift a 150 m sobre el nulo de cauces, sujeto a recall ≥ 0.5 a 150 m;
    a igual lift, la menor densidad; si ninguna llega a 0.5, se reporta que no
    llega, sin bajar el piso.

Entradas: `entradas_densidad.json` de nowcast (5 ventanas, 56 eventos, series
horarias por ventana; las desagregadas traen además la cota uniforme diaria).
Corre el binario de nowcast (`runout_ondemand`) por ventana × serie × densidad
× semilla, y calcula por corrida las mismas métricas de
`nowcast/scripts/huellas_2021/validar_puntos.py` (acierto a 0/60/150 m, nulo de
cauce en la franja al p90 de la distancia de los puntos al cauce, nulo
uniforme). Restartable: no repite corridas con `stats.json`.

El recall se calcula de dos formas (pedido de nowcast): sobre TODOS los eventos
de las ventanas, incluidos los de ventanas donde el gatillo no dispara, y solo
sobre las ventanas que disparan (separa pérdida por forzante de pérdida por
modelo). Validación: dejar un temporal fuera.

Uso: barrido_densidad.py ENTRADAS.json SALIDA_DIR [--densidades 0.02,0.05,...]
     [--semillas 1,2,3] [--solo-correr] [--solo-analizar]
Los datos derivados del catálogo NO se suben a ningún repo.
"""
import argparse, json, os, subprocess, sys, time

import numpy as np

DENSIDADES = [0.02, 0.05, 0.1, 0.2, 0.5, 1.0]
SEMILLAS = [1, 2, 3]
TOLS = [0, 60, 150]
RNG_NULO = 20210130
RECALL_MIN = 0.5
TOL_REGLA = 150


def comando(w, serie, densidad, semilla, out, seeding="follow"):
    cmd = [os.path.expanduser(c) if c.startswith("~") else c for c in w["comando_base"]]
    rep = {"<serie>": ",".join(f"{x:.4f}" for x in serie), "<densidad>": str(densidad),
           "<isoterma_m>": str(w["isoterma_m"]), "<semilla>": str(semilla), "<dir>": out}
    cmd = [rep.get(c, c) for c in cmd]
    if seeding == "fixed":
        cmd[cmd.index("--trigger-seeding") + 1] = "fixed"
    if "--write-layers" not in cmd:
        cmd.append("--write-layers")  # streams.tif para el nulo de cauce
    return cmd


def variantes_serie(w):
    """(nombre, serie) por ventana: perfil siempre; uniforme si es desagregada."""
    s = w["serie_max"]
    out = [("perfil", s["mm_por_hora"])]
    if s.get("mm_por_hora_uniforme_diaria"):
        out.append(("uniforme", s["mm_por_hora_uniforme_diaria"]))
    return out


def correr_todo(ventanas, raiz, densidades, semillas, log):
    tareas = []
    for w in ventanas:
        for nombre_serie, serie in variantes_serie(w):
            for d in ["fixed"] + densidades:
                for s in semillas:
                    out = os.path.join(raiz, "corridas", f"{w['id']}_{nombre_serie}_d{d}_s{s}")
                    tareas.append((w, nombre_serie, serie, d, s, out))
    hechas = sum(os.path.exists(os.path.join(t[5], "stats.json")) for t in tareas)
    log(f"{len(tareas)} corridas, {hechas} ya hechas")
    t0 = time.time()
    for i, (w, nombre_serie, serie, d, s, out) in enumerate(tareas):
        if os.path.exists(os.path.join(out, "stats.json")):
            continue
        os.makedirs(out, exist_ok=True)
        cmd = comando(w, serie, 0.0 if d == "fixed" else d, s, out, "fixed" if d == "fixed" else "follow")
        t = time.time()
        p = subprocess.run(cmd, capture_output=True, text=True)
        open(os.path.join(out, "stdout.txt"), "w").write(p.stdout)
        if p.returncode != 0:
            open(os.path.join(out, "ERROR.txt"), "w").write(p.stderr[-2000:])
            log(f"  [{i+1}/{len(tareas)}] {os.path.basename(out)} FALLÓ: {p.stderr.strip().splitlines()[-1] if p.stderr.strip() else '?'}")
            continue
        log(f"  [{i+1}/{len(tareas)}] {os.path.basename(out)} {time.time()-t:.0f}s (acum {(time.time()-t0)/60:.1f} min)")


def metricas_corrida(out, eventos, rng):
    import rasterio
    from rasterio.warp import transform as warp_xy
    from scipy import ndimage
    with rasterio.open(os.path.join(out, "runout_footprint.tif")) as r:
        fp = r.read(1) == 1; T = r.transform; crs = r.crs
    with rasterio.open(os.path.join(out, "streams.tif")) as r:
        st = r.read(1) > 0
    px = T.a
    xs, ys = warp_xy("EPSG:4326", crs, [e["lon"] for e in eventos], [e["lat"] for e in eventos])
    dentro = []
    for (x, y), e in zip(zip(xs, ys), eventos):
        c_, r_ = (~T) * (x, y)
        if 0 <= r_ < fp.shape[0] and 0 <= c_ < fp.shape[1]:
            dentro.append((int(r_), int(c_), e["cod_rm"]))
    fila = {"n_puntos": len(dentro), "n_eventos_ventana": len(eventos),
            "huella_frac": float(fp.mean()), "huella_km2": float(fp.sum() * px * px / 1e6)}
    if not dentro:
        return fila
    d_fp = ndimage.distance_transform_edt(~fp) * px if fp.any() else np.full(fp.shape, np.inf)
    d_st = ndimage.distance_transform_edt(~st) * px
    dist_pts_st = np.array([d_st[r_, c_] for r_, c_, _ in dentro])
    ancho = float(np.percentile(dist_pts_st, 90))
    franja = np.argwhere(d_st <= ancho)
    nul = franja[rng.choice(len(franja), size=min(3000, len(franja)), replace=False)]
    fila["ancho_franja_m"] = ancho
    for t in TOLS:
        fila[f"aciertos_{t}"] = int(sum(d_fp[r_, c_] <= t for r_, c_, _ in dentro))
        fila[f"nulo_cauce_{t}"] = float(np.mean(d_fp[nul[:, 0], nul[:, 1]] <= t))
        fila[f"nulo_unif_{t}"] = float(np.mean(d_fp <= t))
    return fila


def disparo(out):
    """E máx, horas con E >= 1 y k objetivo, parseados del stdout del runner
    (stats.json no los expone en el primer nivel)."""
    import re
    r = {}
    try:
        txt = open(os.path.join(out, "stdout.txt")).read()
    except FileNotFoundError:
        return r
    m = re.search(r"E máx ([0-9.]+) · (\d+) de (\d+) h con E", txt)
    if m:
        r["e_max"], r["horas_excedidas"] = float(m.group(1)), int(m.group(2))
    m = re.search(r"k objetivo (\d+) gotas/h", txt)
    if m:
        r["k_por_hora"] = int(m.group(1))
    m = re.search(r"área excedida con compuertas ([0-9.]+) km²", txt)
    if m:
        r["area_excedida_km2"] = float(m.group(1))
    return r


def analizar(ventanas, raiz, densidades, semillas, log):
    rng = np.random.default_rng(RNG_NULO)
    filas = []
    for w in ventanas:
        for nombre_serie, _ in variantes_serie(w):
            for d in ["fixed"] + densidades:
                for s in semillas:
                    out = os.path.join(raiz, "corridas", f"{w['id']}_{nombre_serie}_d{d}_s{s}")
                    if not os.path.exists(os.path.join(out, "stats.json")):
                        continue
                    m = metricas_corrida(out, w["eventos"], rng)
                    m.update({"ventana": w["id"], "temporal": w["temporal"], "serie": nombre_serie,
                              "densidad": d, "semilla": s, **disparo(out)})
                    filas.append(m)
    json.dump({"filas": filas, "regla": f"max lift_{TOL_REGLA} sujeto a recall_{TOL_REGLA} >= {RECALL_MIN}; empate -> menor densidad",
               "densidades": densidades, "semillas": semillas},
              open(os.path.join(raiz, "barrido_densidad.json"), "w"), indent=1)
    return filas


def agregar(filas, serie, d, temporales):
    """Agrega una (serie, densidad) sobre las ventanas de `temporales`, media
    sobre semillas. Recall total: aciertos / todos los eventos de esas ventanas.
    Recall 'disparan': solo ventanas con huella no vacía."""
    g = [f for f in filas if f["serie"] == serie and f["densidad"] == d and f["temporal"] in temporales]
    if not g:
        return None
    por_v = {}
    for f in g:
        por_v.setdefault(f["ventana"], []).append(f)
    n_ev = sum(v[0]["n_eventos_ventana"] for v in por_v.values())
    ac = sum(np.mean([f.get(f"aciertos_{TOL_REGLA}", 0) for f in v]) for v in por_v.values())
    dispara = [v for v in por_v.values() if np.mean([f["huella_km2"] for f in v]) > 0]
    n_ev_disp = sum(v[0]["n_eventos_ventana"] for v in dispara)
    ac_disp = sum(np.mean([f.get(f"aciertos_{TOL_REGLA}", 0) for f in v]) for v in dispara)
    # nulo de cauce ponderado por puntos de cada ventana (solo ventanas con puntos)
    pesos = [(v[0]["n_puntos"], np.mean([f.get(f"nulo_cauce_{TOL_REGLA}", 0) for f in v])) for v in por_v.values() if v[0]["n_puntos"]]
    nulo = sum(n * x for n, x in pesos) / max(1, sum(n for n, _ in pesos))
    recall = ac / n_ev if n_ev else float("nan")
    acierto = ac / sum(n for n, _ in pesos) if pesos else float("nan")
    return {"serie": serie, "densidad": d, "n_eventos": n_ev, "recall": recall,
            "recall_disparan": (ac_disp / n_ev_disp) if n_ev_disp else float("nan"),
            "n_eventos_disparan": n_ev_disp, "acierto": acierto, "nulo_cauce": nulo,
            "lift": (acierto / nulo) if nulo > 0 else float("nan"),
            "huella_km2": float(np.mean([f["huella_km2"] for f in g]))}


def rangos_por_ventana(filas, serie, d, temporales):
    """Rango (max-min entre semillas) de aciertos a TOL_REGLA por ventana que dispara."""
    g = [f for f in filas if f["serie"] == serie and f["densidad"] == d and f["temporal"] in temporales]
    por_v = {}
    for f in g:
        por_v.setdefault(f["ventana"], []).append(f)
    out = {}
    for v, r in por_v.items():
        if np.mean([x["huella_km2"] for x in r]) > 0:
            ac = [x.get(f"aciertos_{TOL_REGLA}", 0) for x in r]
            out[v] = max(ac) - min(ac)
    return out


def elegir_v2(tabla, filas, serie, temporales):
    """Regla de la 2a vuelta (nowcast, 2026-10-23, declarada antes de correr):
    la MINIMA densidad con recall >= 0.8 sobre las ventanas que disparan y rango
    entre semillas <= 1 acierto en cada ventana. La perdida por forzante (eventos
    en ventanas que no disparan) se reporta aparte y no entra. Si ninguna
    cumple, se reporta que no hay y no se relaja nada."""
    for r in sorted((r for r in tabla if r["densidad"] != "fixed"), key=lambda r: r["densidad"]):
        rg = rangos_por_ventana(filas, serie, r["densidad"], temporales)
        r["rangos"] = rg
        if r["recall_disparan"] >= 0.8 and rg and all(x <= 1 for x in rg.values()):
            return r
    return None


def elegir(tabla):
    """Regla de la 1a vuelta: max lift con recall >= RECALL_MIN; empate -> menor densidad."""
    cand = [r for r in tabla if r["densidad"] != "fixed" and r["recall"] >= RECALL_MIN and np.isfinite(r["lift"])]
    if not cand:
        return None
    return max(cand, key=lambda r: (r["lift"], -r["densidad"]))


def informe(filas, densidades, log, regla="v1"):
    temporales = sorted({f["temporal"] for f in filas})
    n_semillas = len({f["semilla"] for f in filas})
    log(f"\n(regla {regla} · {n_semillas} semillas)")
    series = ["perfil", "uniforme"]
    for serie in series:
        log(f"\n## serie: {serie}  (recall y lift a {TOL_REGLA} m)")
        log(f"{'densidad':>8} {'n_ev':>4} {'recall':>7} {'rec_disp':>8} {'n_disp':>6} {'acierto':>8} {'nulo':>6} {'lift':>6} {'km2':>7}")
        tabla = []
        for d in ["fixed"] + densidades:
            r = agregar(filas, serie, d, temporales)
            if r:
                tabla.append(r)
                log(f"{str(d):>8} {r['n_eventos']:4d} {r['recall']:7.2f} {r['recall_disparan']:8.2f} {r['n_eventos_disparan']:6d} {r['acierto']:8.2f} {r['nulo_cauce']:6.2f} {r['lift']:6.2f} {r['huella_km2']:7.1f}")
        if regla == "v2":
            for r in tabla:
                if r["densidad"] != "fixed":
                    rg = rangos_por_ventana(filas, serie, r["densidad"], temporales)
                    log(f"      d{r['densidad']:<5} rangos entre semillas por ventana: {rg}")
            e = elegir_v2(tabla, filas, serie, temporales)
            perdida = next((r for r in tabla if r["densidad"] != "fixed"), None)
            if perdida:
                log(f"  pérdida por forzante (ventanas que no disparan): {perdida['n_eventos'] - perdida['n_eventos_disparan']} de {perdida['n_eventos']} eventos")
            log(f"  → regla v2: {'densidad ' + str(e['densidad']) + f' (recall disparan {e['recall_disparan']:.2f}, rangos {e['rangos']}, lift {e['lift']:.2f})' if e else 'NINGUNA densidad cumple las dos condiciones; se reporta que no hay'}")
        else:
            e = elegir(tabla)
            log(f"  → regla: {'densidad ' + str(e['densidad']) + f' (lift {e['lift']:.2f}, recall {e['recall']:.2f})' if e else f'NINGUNA densidad llega a recall {RECALL_MIN}; se reporta que no llega'}")
        # por temporal y dejando uno fuera
        for t in temporales:
            sub = [agregar(filas, serie, d, [t]) for d in densidades]
            sub = [r for r in sub if r]
            if sub:
                log(f"  solo {t}: " + " ".join(f"d{r['densidad']}:R{r['recall']:.2f}/L{r['lift']:.2f}" for r in sub))
            fuera = [agregar(filas, serie, d, [x for x in temporales if x != t]) for d in densidades]
            fuera = [r for r in fuera if r]
            e = elegir_v2(fuera, filas, serie, [x for x in temporales if x != t]) if regla == "v2" else elegir(fuera)
            log(f"  sin {t}: " + (f"regla elige {e['densidad']} (lift {e['lift']:.2f}, recall {e['recall']:.2f}, disparan {e['recall_disparan']:.2f})" if e else "no cumple / no llega"))


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("entradas"); ap.add_argument("salida")
    ap.add_argument("--densidades", default=",".join(map(str, DENSIDADES)))
    ap.add_argument("--semillas", default=",".join(map(str, SEMILLAS)))
    ap.add_argument("--solo-correr", action="store_true"); ap.add_argument("--solo-analizar", action="store_true")
    ap.add_argument("--regla", default="v1", choices=["v1", "v2"])
    a = ap.parse_args()
    densidades = [float(x) for x in a.densidades.split(",")]
    semillas = [int(x) for x in a.semillas.split(",")]
    os.makedirs(a.salida, exist_ok=True)
    logf = open(os.path.join(a.salida, "barrido.log"), "a")
    def log(m):
        print(m, flush=True); logf.write(m + "\n"); logf.flush()
    ventanas = json.load(open(a.entradas))
    log(f"=== {time.strftime('%Y-%m-%d %H:%M')} · {len(ventanas)} ventanas · densidades {densidades} · semillas {semillas}")
    if not a.solo_analizar:
        correr_todo(ventanas, a.salida, densidades, semillas, log)
    if not a.solo_correr:
        filas = analizar(ventanas, a.salida, densidades, semillas, log)
        log(f"{len(filas)} corridas analizadas")
        informe(filas, densidades, log, a.regla)


if __name__ == "__main__":
    main()
