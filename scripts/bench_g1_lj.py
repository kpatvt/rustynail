#!/usr/bin/env python3
"""BUNDLE-G1: ours/LuaJIT and ours/PUC per program (awfy + apps), min and median of cycles:u, from docs/bench-g1-lj-<set>.json."""
import json, math, os, sys
D = sys.argv[1] if len(sys.argv) > 1 else os.path.join(os.path.dirname(__file__), "..", "docs")
gm = lambda xs: math.exp(sum(math.log(x) for x in xs) / len(xs))
rows = {}
for st in ("awfy", "apps"):
    j = json.load(open(os.path.join(D, f"bench-g1-lj-{st}.json")))
    for n, v in j["programs"].items():
        rows[st + "/" + n] = v
print("| program | LJ Mcyc min/med | plain /LJ min | /LJ med | PGO /LJ min | /LJ med | PGO /PUC min | base PGO /LJ min |\n|---|---|---|---|---|---|---|---|")
acc = {k: [] for k in ("pm", "pd", "gm", "gd", "gp", "bm")}
for n, v in rows.items():
    lj, pl, pg, bp, rf = v["lj"], v["plain"], v["pgo"], v["bpgo"], v["ref"]
    r = lambda a, b, k: a[k] / b[k]
    vals = (r(pl, lj, "cycles_min"), r(pl, lj, "cycles_median"), r(pg, lj, "cycles_min"), r(pg, lj, "cycles_median"), r(pg, rf, "cycles_min"), r(bp, lj, "cycles_min"))
    for k, x in zip(acc, vals):
        acc[k].append(x)
    print(f"| {n} | {lj['cycles_min']/1e6:.1f} / {lj['cycles_median']/1e6:.1f} | " + " | ".join(f"{x:.3f}" for x in vals) + " |")
for lab, pre in (("awfy", "awfy/"), ("apps", "apps/")):
    idx = [i for i, n in enumerate(rows) if n.startswith(pre)]
    print(f"| **{lab} geomean** | | " + " | ".join(f"**{gm([acc[k][i] for i in idx]):.3f}**" for k in acc) + " |")
