#!/usr/bin/env python3
"""P-I: summarise a bench_cycles.py JSON (columns p1 = baseline release build, p1b = duplicate of p1 for the noise
estimate, others = build variants).  usage: scripts/pi_report.py file.json [metric]   (metric: cycles_min | instructions_min)
Prints per column: geomean ratio vs p1, the worst and best program, and the programs above 1.05 / below 0.95."""
import json, math, sys

d = json.load(open(sys.argv[1]))["programs"]
met = sys.argv[2] if len(sys.argv) > 2 else "cycles_min"
cols = [c for c in next(iter(d.values())).keys() if c != "p1"]
g = lambda xs: math.exp(sum(map(math.log, xs)) / len(xs))
print(f"{sys.argv[1]}  metric={met}  n={len(d)}")
for c in cols:
    r = {}
    for p, row in d.items():
        if met in row.get("p1", {}) and met in row.get(c, {}):
            r[p] = row[c][met] / row["p1"][met]
    if not r:
        continue
    w = max(r.items(), key=lambda x: x[1])
    b = min(r.items(), key=lambda x: x[1])
    hi = sorted([(round(v, 3), p) for p, v in r.items() if v > 1.05], reverse=True)
    print(f"  {c:4s} geomean {g(r.values()):.4f}  worst {w[0]} {w[1]:.3f}  best {b[0]} {b[1]:.3f}  above1.05: {hi[:6]}  n_above={len(hi)}")
