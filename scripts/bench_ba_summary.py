#!/usr/bin/env python3
"""BUNDLE-A summary of docs/bench-ba-<set>-pgo.json (scripts/bench_lj.py runs with the columns
ref, plain, plain2 (duplicate), pgo, pgo2 (duplicate), hold (PGO trained without the benchmark sets), lj):
geomean of min and median cycles and of instructions for pgo/plain, hold/plain, the duplicate columns, and
ours/PUC and ours/LuaJIT for the plain and the PGO build."""
import json, math, os, sys

root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))


def gm(xs):
    xs = [x for x in xs if x and x > 0]
    return math.exp(sum(math.log(x) for x in xs) / len(xs)) if xs else float("nan")


def ratio(rows, a, b, key):
    out = {}
    for p, r in rows.items():
        if key in r.get(a, {}) and key in r.get(b, {}):
            out[p] = r[a][key] / r[b][key]
    return out


for s in sys.argv[1:] or ["programs", "awfy", "apps", "jit-trace", "lib"]:
    f = os.path.join(root, f"docs/bench-ba-{s}-pgo.json")
    if not os.path.exists(f):
        continue
    rows = json.load(open(f))["programs"]
    line = f"{s:<10} n={len(rows):<3}"
    for lab, a, b in (("pgo/plain", "pgo", "plain"), ("hold/plain", "hold", "plain"), ("plain2/plain", "plain2", "plain"), ("pgo2/pgo", "pgo2", "pgo")):
        mn = ratio(rows, a, b, "cycles_min")
        md = ratio(rows, a, b, "cycles_median")
        ins = ratio(rows, a, b, "instructions_min")
        line += f" | {lab} min {gm(mn.values()):.3f} med {gm(md.values()):.3f} instr {gm(ins.values()):.3f}"
    print(line)
    for lab, b in (("plain", "plain"), ("pgo", "pgo")):
        puc = ratio(rows, b, "ref", "cycles_min")
        lj = ratio(rows, b, "lj", "cycles_min")
        print(f"   {lab:<6} ours/PUC {gm(puc.values()):.3f} (n={len(puc)})  ours/LuaJIT {gm(lj.values()):.3f} (n={len(lj)})")
    r = ratio(rows, "pgo", "plain", "cycles_min")
    srt = sorted(r.items(), key=lambda kv: kv[1])
    print("   best : " + ", ".join(f"{p} {v:.3f}" for p, v in srt[:5]))
    print("   worst: " + ", ".join(f"{p} {v:.3f}" for p, v in srt[-5:]))
