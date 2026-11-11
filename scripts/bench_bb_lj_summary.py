#!/usr/bin/env python3
"""BUNDLE-B: geomean of the per-program cycle ratios ours/PUC and ours/LuaJIT from docs/bench-bb-lj-<set>.json."""
import json, math, os

root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))


def gm(x):
    return math.exp(sum(math.log(v) for v in x) / len(x))


for s in ["awfy", "apps", "programs", "jit-trace", "lib"]:
    p = json.load(open(os.path.join(root, f"docs/bench-bb-lj-{s}.json")))["programs"]
    puc = [r["ours"]["cycles_min"] / r["ref"]["cycles_min"] for r in p.values() if "cycles_min" in r.get("ref", {}) and "ours" in r]
    lj = [r["ours"]["cycles_min"] / r["lj"]["cycles_min"] for r in p.values() if "cycles_min" in r.get("lj", {}) and "ours" in r]
    print("%-10s n=%-3d ours/PUC %.3f   ours/LuaJIT %.3f (n=%d)" % (s, len(puc), gm(puc), gm(lj) if lj else float("nan"), len(lj)))
