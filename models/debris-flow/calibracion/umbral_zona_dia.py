#!/usr/bin/env python3
"""Calibración del umbral de inicio con la tabla zona-día de nowcast (punto 3,
decisión 2 del PROTOCOLO_CALIBRACION_GATILLO.md), como problema PU:

  R(θ) = fracción de zonas-día `evento` con X ≥ θ (y versión ponderada por los
         eventos del catálogo que representan);
  A(θ) = fracción de zonas-día `no_evento` con X ≥ θ (tasa de disparo; NO son
         falsos positivos: el catálogo es incompleto).

Entrega la curva R vs A por variable (agregado declarado `pp_max_mm` y las
sensibilidades), los θ que conservan recall 1.0 / 0.9 / 0.8 con su tasa de
disparo y alarmas esperadas por año, la precisión como COTA INFERIOR (π =
prevalencia observada, que es cota inferior de la real), y la validación
dejando un temporal fuera: θ elegido con los otros temporales (máximo θ que
conserva el recall objetivo) y recall en el temporal excluido. Los eventos con
< 1 mm en el agregado se excluyen y se reportan (protocolo §4).

Uso: umbral_zona_dia.py zona_dia.csv SALIDA_DIR
Los derivados del catálogo NO se suben a ningún repo.
"""
import csv, json, os, sys
from collections import defaultdict
import numpy as np

VARS = ["pp_max_mm", "pp_media_mm", "pp_cercana_mm", "pp3_max_mm", "pph_max_mm"]
RECALLS = [1.0, 0.9, 0.8]
MIN_MM = 1.0


def leer(path):
    filas = list(csv.DictReader(open(path, encoding="utf-8")))
    for f in filas:
        for v in VARS:
            f[v] = float(f[v]) if f[v] not in ("", "nan") else np.nan
        f["n_eventos"] = int(f["n_eventos"] or 0)
        f["temporal"] = f["fecha"][:7]
    return filas


def curva(xe, we, xn):
    """θ en la rejilla de valores de los eventos; R, R ponderado, A."""
    thetas = np.unique(np.concatenate([[0.0], xe]))
    out = []
    for t in thetas:
        out.append({"theta": float(t), "R": float(np.mean(xe >= t)),
                    "R_ev": float(np.sum(we[xe >= t]) / we.sum()), "A": float(np.mean(xn >= t))})
    return out


def theta_para(curva_, r):
    """Máximo θ que conserva R ≥ r."""
    ok = [c for c in curva_ if c["R"] >= r - 1e-12]
    return max(ok, key=lambda c: c["theta"]) if ok else None


def main():
    filas = leer(sys.argv[1]); out_dir = sys.argv[2]; os.makedirs(out_dir, exist_ok=True)
    log_lines = []
    def log(s=""):
        print(s); log_lines.append(s)
    ev = [f for f in filas if f["etiqueta"] == "evento"]
    ne = [f for f in filas if f["etiqueta"] == "no_evento"]
    zonas = {(f["zona_lon"], f["zona_lat"]) for f in filas}
    n_dias = len({f["fecha"] for f in filas})
    # La tabla solo trae zonas-día con lluvia: las alarmas por año se escalan al
    # período cubierto (fechas extremas), no a 365 días por fecha presente.
    fechas = sorted({f["fecha"] for f in filas})
    from datetime import date
    d0, d1 = (date.fromisoformat(fechas[0]), date.fromisoformat(fechas[-1]))
    anios = max(1e-9, (d1 - d0).days / 365.25)
    log(f"zonas-día: {len(ev)} evento ({sum(f['n_eventos'] for f in ev)} eventos) · {len(ne)} no_evento · {len(zonas)} zonas · {n_dias} fechas con lluvia en {anios:.1f} años ({fechas[0]}..{fechas[-1]}) · fechas distintas en eventos: {len({f['fecha'] for f in ev})}")
    conv = [f for f in ev if not (f["pp_max_mm"] >= MIN_MM)]
    log(f"evento con pp_max < {MIN_MM} mm (convectivos / sin dato): {len(conv)} zonas-día, {sum(f['n_eventos'] for f in conv)} eventos → excluidos de R y reportados")
    ev_ok = [f for f in ev if f["pp_max_mm"] >= MIN_MM]

    log("\n## Las zonas-día evento (agregado declarado y sensibilidades, mm)")
    log(f"{'fecha':10} {'temporal':8} {'n_ev':>4} {'max':>6} {'media':>6} {'cercana':>7} {'3d':>6} {'horaria':>7} {'n_est':>5}")
    for f in sorted(ev, key=lambda f: (f["fecha"], -f["n_eventos"])):
        h = f"{f['pph_max_mm']:7.1f}" if np.isfinite(f["pph_max_mm"]) else "      —"
        log(f"{f['fecha']:10} {f['temporal']:8} {f['n_eventos']:4d} {f['pp_max_mm']:6.1f} {f['pp_media_mm']:6.1f} {f['pp_cercana_mm']:7.1f} {f['pp3_max_mm']:6.1f} {h} {f['n_est']:>5}")

    res = {"variables": {}, "excluidos_lt_1mm": len(conv)}
    pi_obs = len(ev) / len(filas)
    for v in VARS:
        e = [f for f in ev_ok if np.isfinite(f[v])]; n = [f for f in ne if np.isfinite(f[v])]
        if len(e) < 3:
            log(f"\n## {v}: solo {len(e)} zonas-día evento con dato; se omite"); continue
        xe = np.array([f[v] for f in e]); we = np.array([f["n_eventos"] for f in e], float); xn = np.array([f[v] for f in n])
        c = curva(xe, we, xn)
        log(f"\n## {v}  ({len(e)} zonas-día evento con dato, {len(n)} no_evento; π observada {pi_obs:.2e})")
        log(f"{'recall':>7} {'θ mm':>7} {'R_ev':>5} {'A':>7} {'alarmas/año':>11} {'prec≥':>6}")
        pts = {}
        for r in RECALLS:
            p = theta_para(c, r)
            if not p: continue
            alarmas = p["A"] * len(n) / anios  # zonas-día de alarma por año calendario, red actual
            prec = pi_obs * p["R"] / (pi_obs * p["R"] + (1 - pi_obs) * p["A"]) if p["A"] > 0 else float("nan")
            pts[r] = {**p, "alarmas_anio": alarmas, "precision_cota_inf": prec}
            log(f"{r:7.1f} {p['theta']:7.1f} {p['R_ev']:5.2f} {p['A']:7.4f} {alarmas:11.0f} {prec:6.3f}")
        # tasa de disparo a umbrales redondos
        log("   A a umbrales fijos: " + "  ".join(f"{t:g}mm→R{np.mean(xe>=t):.2f}/A{np.mean(xn>=t):.4f}" for t in ([10, 20, 30, 40, 60] if v != 'pph_max_mm' else [2, 4, 5.5, 8, 12])))
        # dejar un temporal fuera
        temporales = sorted({f["temporal"] for f in e})
        folds = {}
        for t in temporales:
            dentro = [f for f in e if f["temporal"] != t]; fuera = [f for f in e if f["temporal"] == t]
            if len(dentro) < 2: continue
            xd = np.array([f[v] for f in dentro]); wd = np.array([f["n_eventos"] for f in dentro], float)
            cd = curva(xd, wd, xn)
            fila = {}
            for r in (1.0, 0.8):
                p = theta_para(cd, r)
                if p:
                    xf = np.array([f[v] for f in fuera]); wf = np.array([f["n_eventos"] for f in fuera], float)
                    fila[r] = {"theta": p["theta"], "R_fuera": float(np.mean(xf >= p["theta"])),
                               "R_ev_fuera": float(np.sum(wf[xf >= p["theta"]]) / wf.sum()), "n_fuera": len(fuera)}
            folds[t] = fila
            log(f"   sin {t} ({len(fuera)} zd): " + " | ".join(f"R_in≥{r}: θ={x['theta']:.1f} → R_fuera {x['R_fuera']:.2f} (ev {x['R_ev_fuera']:.2f})" for r, x in fila.items()))
        res["variables"][v] = {"curva": c, "puntos": pts, "folds": folds}
    # ---- Unidad alternativa (post hoc, a declarar antes de la próxima vuelta):
    # recall por ZONA-EVENTO con el máximo de la variable sobre los días de su
    # ventana [ini-1, fin+1]; tasa de disparo por zona-día no_evento, igual.
    # Motivo: con la unidad zona-día, el día anterior/posterior a la lluvia entra
    # en la clase positiva con lluvia ~0 y ningún θ > 2.5 mm conserva el recall.
    ze = defaultdict(lambda: {v: np.nan for v in VARS} | {"n_ev": 0, "dias": 0})
    for f in ev:
        k = (f["zona_lon"], f["zona_lat"], f["temporal"]); z = ze[k]
        for v in VARS:
            if np.isfinite(f[v]):
                z[v] = np.nanmax([z[v], f[v]])
        z["n_ev"] = max(z["n_ev"], f["n_eventos"]); z["dias"] += 1
    log(f"\n## Unidad zona-evento (máximo de la ventana): {len(ze)} zona-eventos")
    for k, z in sorted(ze.items(), key=lambda kv: kv[0][2]):
        log(f"   {k[2]} zona({k[0]},{k[1]}) n_ev {z['n_ev']:2d} · días {z['dias']} · " + " · ".join(f"{v[:-3]} {z[v]:.1f}" for v in VARS if np.isfinite(z[v])))
    res["zona_evento"] = {}
    for v, ths in (("pp_max_mm", [10, 14.4, 20, 30, 34.4, 40, 60]), ("pp3_max_mm", [20, 30, 40, 60, 80, 100]), ("pph_max_mm", [2, 3.5, 5.5, 8, 10.7])):
        xe = np.array([z[v] for z in ze.values()]); we = np.array([z["n_ev"] for z in ze.values()], float)
        ok = np.isfinite(xe); xe, we = xe[ok], we[ok]
        xn = np.array([f[v] for f in ne if np.isfinite(f[v])])
        fila = [{"theta": t, "R": float(np.mean(xe >= t)), "R_ev": float(we[xe >= t].sum() / we.sum()), "A": float(np.mean(xn >= t)),
                 "alarmas_anio": float(np.mean(xn >= t) * len(xn) / anios)} for t in ths]
        res["zona_evento"][v] = fila
        log(f"   {v} ({len(xe)} zona-eventos, {int(we.sum())} eventos): " + "  ".join(f"θ{r['theta']:g}→R{r['R']:.2f}(ev {r['R_ev']:.2f})/A{r['A']:.3f}/{r['alarmas_anio']:.0f}/año" for r in fila))
    json.dump(res, open(os.path.join(out_dir, "umbral.json"), "w"), indent=1)
    open(os.path.join(out_dir, "umbral.log"), "w").write("\n".join(log_lines) + "\n")


if __name__ == "__main__":
    main()
