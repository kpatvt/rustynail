#!/usr/bin/env python3
"""BUNDLE-G1 before/after table from docs/bench-g1-<mode>-<flavor>-<set>.json (columns base / new / dup).
Usage: scripts/bench_g1_report.py [docs-dir]   -> markdown on stdout."""
import json, math, os, sys

D = sys.argv[1] if len(sys.argv) > 1 else os.path.join(os.path.dirname(__file__), "..", "docs")


def gm(xs):
    return math.exp(sum(math.log(x) for x in xs) / len(xs))


for mode in ("warm0", "default"):
    for fl in ("plain", "pgo"):
        rows = []
        for st in ("awfy", "apps"):
            p = os.path.join(D, f"bench-g1-{mode}-{fl}-{st}.json")
            if not os.path.exists(p):
                continue
            j = json.load(open(p))
            for name, v in j["programs"].items():
                b, n, d = v["base"], v["new"], v["dup"]
                rows.append((st + "/" + name, n["cycles_min"] / b["cycles_min"], n["cycles_median"] / b["cycles_median"],
                             n["instructions_min"] / b["instructions_min"], d["cycles_min"] / b["cycles_min"], d["cycles_median"] / b["cycles_median"],
                             b["cycles_min"] / 1e6, n["cycles_min"] / 1e6))
        if not rows:
            continue
        print(f"\n### {mode} {fl}\n\n| program | base Mcyc | new Mcyc | cycles min | cycles median | instructions | dup min | dup median |\n|---|---|---|---|---|---|---|---|")
        for r in rows:
            print(f"| {r[0]} | {r[6]:.1f} | {r[7]:.1f} | {r[1]:.3f} | {r[2]:.3f} | {r[3]:.3f} | {r[4]:.3f} | {r[5]:.3f} |")
        for lab, sel in (("awfy", lambda r: r[0].startswith("awfy")), ("apps", lambda r: r[0].startswith("apps")), ("awfy+apps", lambda r: True)):
            s = [r for r in rows if sel(r)]
            print(f"| **{lab} geomean ({len(s)})** | | | **{gm([r[1] for r in s]):.3f}** | {gm([r[2] for r in s]):.3f} | {gm([r[3] for r in s]):.3f} | {gm([r[4] for r in s]):.3f} | {gm([r[5] for r in s]):.3f} |")
