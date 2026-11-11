#!/usr/bin/env python3
import json, sys, math, glob, os
tag = ""
res = {}
for f in sorted(glob.glob("docs/bench-t1i-*.json")):
    s = os.path.basename(f)[len("bench-t1i-"):-5]
    res[s] = json.load(open(f))["results"]
def gm(v): return math.exp(sum(map(math.log, v)) / len(v)) if v else float("nan")
def row(name, items):
    c = {k: [] for k in ("dup", "on")}; i = []
    for p, r in items:
        b = min(r["base"]["cyc"]); bi = min(r["base"]["ins"])
        for k in c: c[k].append(min(r[k]["cyc"]) / b)
        i.append(min(r["on"]["ins"]) / bi)
    print(f"{name:34s} n={len(items):3d} dup {gm(c['dup']):.3f}  on {gm(c['on']):.3f}  ins {gm(i):.3f}")
allp = []
for s, d in res.items():
    items = sorted(d.items())
    if s in ("corpus2dev", "corpus2heldout"):
        row(s + " (no real_dis)", [x for x in items if "real_dis" not in x[0]])
        row(s + " real_dis", [x for x in items if "real_dis" in x[0]])
    else:
        row(s, items)
    allp += [x for x in items if "real_dis" not in x[0]]
row("ALL (no real_dis)", allp)
print("--- outliers (|cyc-1|>3% or |ins-1|>3%)")
for p, r in allp:
    b = min(r["base"]["cyc"]); bi = min(r["base"]["ins"])
    o = min(r["on"]["cyc"]) / b; oi = min(r["on"]["ins"]) / bi; d = min(r["dup"]["cyc"]) / b
    bad = "" if r["on"]["ok"] else " MISMATCH"
    if abs(o - 1) > 0.03 or abs(oi - 1) > 0.03:
        print(f"  {p:36s} cyc {o:.3f} ins {oi:.3f} dup {d:.3f}{bad}")
