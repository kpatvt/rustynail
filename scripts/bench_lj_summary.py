#!/usr/bin/env python3
"""Geomean summary over docs/bench-lj-{programs,real,jit-trace,lib}.json (see scripts/bench_lj.py)."""
import json, math, os

root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))


def gm(xs):
    return math.exp(sum(math.log(x) for x in xs) / len(xs)) if xs else float("nan")


def load(s):
    r = json.load(open(os.path.join(root, f"docs/bench-lj-{s}.json")))
    out = {}
    for p, row in r["programs"].items():
        if "cycles_min" in row.get("lj", {}) and "cycles_min" in row.get("ours", {}):
            out[p] = (row["ref"]["cycles_min"], row["ours"]["cycles_min"], row["lj"]["cycles_min"], row["lj"]["output"])
    return out


def rep(label, d):
    if not d:
        return
    print(f"{label:<44} n={len(d):<3} ours/LJ {gm([o / l for _, o, l, _ in d.values()]):6.3f}  PUC/LJ {gm([r / l for r, _, l, _ in d.values()]):6.3f}"
          f"  ours/PUC {gm([o / r for r, o, _, _ in d.values()]):5.3f}  ours faster than LJ: {sum(1 for _, o, l, _ in d.values() if o < l)}/{len(d)}")


allp = {}
for s in ("programs", "real", "jit-trace", "lib"):
    d = load(s)
    rep(f"{s} (all comparable)", d)
    rep(f"{s} (output same/numeq only)", {p: v for p, v in d.items() if v[3] != "diff"})
    if s == "lib":
        nf = {p: v for p, v in d.items() if v[2] >= 60e6}
        rep("lib, LuaJIT >= 60 Mcyc (not folded/hoisted)", nf)
        rep("lib, LuaJIT <  60 Mcyc (folded/hoisted suspects)", {p: v for p, v in d.items() if v[2] < 60e6})
    for p, v in d.items():
        allp[(s, p)] = v
core = {k: v for k, v in allp.items() if k[0] in ("programs", "real", "jit-trace")}
rep("programs+real+jit-trace", core)
rep("programs+real (the 30 'workload' programs)", {k: v for k, v in core.items() if k[0] != "jit-trace"})
