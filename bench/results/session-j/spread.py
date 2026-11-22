#!/usr/bin/env python3
"""reading distributions now (fast pass + 15-run rerun) vs session-i cached readings, for selected programs. usage: spread.py prog..."""
import json, sys, os
D = os.path.dirname(os.path.abspath(__file__))
A = json.load(open(D + "/fast_all.json"))["results"]
R = json.load(open(D + "/rerun_15runs_cpu4.json"))["results"]
P = {c: json.load(open(D + f"/../baselines/{c}-22670a1-47266a41.json"))["results"] for c in ("p55", "g55", "p54", "g54")}
cm = {"plain": "p55", "pgo": "g55", "p54": "p54", "g54": "g54"}
for k in sys.argv[1:]:
    if k not in A:
        print(k, "missing"); continue
    for c in ("plain", "pgo", "p54", "g54"):
        v = sorted(x / 1e6 for x in A[k]["cols"][c]["cyc"] + R.get(k, {}).get("cols", {}).get(c, {}).get("cyc", []) if x)
        pv = sorted(x / 1e6 for x in P[cm[c]][k]["cyc"])
        pi = min(P[cm[c]][k]["ins"]) / 1e6
        ni = min(A[k]["cols"][c]["ins"]) / 1e6
        print(f"{k:36s}{c:6s} now n={len(v):2d} min {v[0]:7.0f} med {v[len(v)//2]:7.0f} max {v[-1]:7.0f} | prev n={len(pv):2d} min {pv[0]:7.0f} med {pv[len(pv)//2]:7.0f} max {pv[-1]:7.0f} | ins {ni:.0f} vs {pi:.0f}")
