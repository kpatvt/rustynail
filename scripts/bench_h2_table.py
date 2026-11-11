#!/usr/bin/env python3
"""BUNDLE-H2: before/after table (cycles min/median, instructions ratio) from docs/bench-h2-{plain,pgo}_<set>.json."""
import json, math, sys


def gm(x):
    return math.exp(sum(math.log(v) for v in x) / len(x))


for s in sys.argv[1:]:
    pl = json.load(open(f"docs/bench-h2-plain_{s}.json"))["programs"]
    pg = json.load(open(f"docs/bench-h2-pgo_{s}.json"))["programs"]
    print(f"**{s}**: Mcyc min / median, before and after; ratios after/before (min)\n")
    print("| program | plain before | plain after | plain min ratio | PGO before | PGO after | PGO min ratio | PGO instr ratio |")
    print("|---|---|---|---|---|---|---|---|")
    acc = {"pl": [], "pg": [], "pi": []}
    for n in pl:
        a, b = pl[n]["pb"], pl[n]["pn"]
        c, d = pg[n]["gb"], pg[n]["gn"]
        r1 = b["cycles_min"] / a["cycles_min"]
        r2 = d["cycles_min"] / c["cycles_min"]
        r3 = d["instructions_min"] / c["instructions_min"]
        acc["pl"].append(r1); acc["pg"].append(r2); acc["pi"].append(r3)
        f = lambda x: "%.1f / %.1f" % (x["cycles_min"] / 1e6, x["cycles_median"] / 1e6)
        print(f"| {n} | {f(a)} | {f(b)} | {r1:.3f} | {f(c)} | {f(d)} | {r2:.3f} | {r3:.3f} |")
    print(f"\ngeomean cycles ratio plain {gm(acc['pl']):.3f}, PGO {gm(acc['pg']):.3f}, PGO instructions {gm(acc['pi']):.3f}\n")
