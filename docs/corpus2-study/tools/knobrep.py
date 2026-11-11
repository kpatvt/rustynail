#!/usr/bin/env python3
"""Knob retest report: per knob column, ratio knob/base (min cycles) per program; affected = instructions changed by > 0.3%.
usage: knobrep.py data/knobs_dev.json"""
import json, math, statistics, sys

d = json.load(open(sys.argv[1]))
R = d["results"]
cols = [c for c in next(iter(R.values())) if c not in ("base",)]


def gm(xs):
    return math.exp(sum(map(math.log, xs)) / len(xs)) if xs else float("nan")


rows = []
for c in cols:
    rs, aff, insr = [], [], []
    for p, r in R.items():
        if not r[c]["cyc"] or not r["base"]["cyc"]:
            continue
        x = min(r[c]["cyc"]) / min(r["base"]["cyc"])
        xm = statistics.median(r[c]["cyc"]) / statistics.median(r["base"]["cyc"])
        ir = min(r[c]["ins"]) / min(r["base"]["ins"])
        rs.append(x)
        if abs(ir - 1) > 0.003:
            aff.append((p, x, xm, ir, r[c]["ok"]))
    bad = [p for p, r in R.items() if not r[c]["ok"]]
    g_all = gm(rs)
    g_aff = gm([a[1] for a in aff]) if aff else float("nan")
    g_affm = gm([a[2] for a in aff]) if aff else float("nan")
    best = min(aff, key=lambda a: a[1]) if aff else None
    worst = max(aff, key=lambda a: a[1]) if aff else None
    rows.append((c, len(aff), g_all, g_aff, g_affm, best, worst, bad))
print("| knob | programs affected (instr. changed) | geomean all dev | geomean affected (min) | geomean affected (median) | best | worst | output/rc problems |")
print("|---|---|---|---|---|---|---|---|")
for c, na, ga, gf, gfm, best, worst, bad in rows:
    b = f"{best[0].split('/')[1]} {best[1]:.3f}" if best else "-"
    w = f"{worst[0].split('/')[1]} {worst[1]:.3f}" if worst else "-"
    flag = " **POSITIVE**" if na and gf <= 0.97 else ""
    print(f"| {c.replace('k_', '').replace(':', '=').replace('+', ',')} | {na} | {ga:.3f} | {gf:.3f}{flag} | {gfm:.3f} | {b} | {w} | {','.join(x.split('/')[1] for x in bad) or '-'} |")
if len(sys.argv) > 2:
    c = sys.argv[2]
    for p, r in sorted(R.items()):
        print(p, f"{min(r[c]['cyc']) / min(r['base']['cyc']):.3f}", f"ins {min(r[c]['ins']) / min(r['base']['ins']):.4f}")
