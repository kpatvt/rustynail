#!/usr/bin/env python3
"""BUNDLE-F: before/after tables from docs/bench-bf-<mode>-<set>.json (scripts/bench_lj.py output; columns plain / pgo =
this tree, bplain / bpgo = the base tree cb3288c0, pgo2 = byte copy of pgo = duplicate column).

usage: bench_bf_report.py ba   default|warm0 COL BASE SET...   per-program before/after rows (Mcyc min, ratio cycles min/median, instructions)
       bench_bf_report.py worst default|warm0 COL BASE [PCT]   programs where COL is more than PCT % worse than BASE (cycles min, median or instructions)
       bench_bf_report.py vsbp  default COL                    geomean ratios ours/PUC and ours/LuaJIT (min) per set: this run vs docs/bench-bp-default-*.json (Bundle P)
"""
import json, math, os, sys

root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SETS = ["awfy", "apps", "programs", "jit-trace", "lib", "real"]


def gm(x):
    x = [v for v in x if v and v > 0]
    return math.exp(sum(math.log(v) for v in x) / len(x)) if x else float("nan")


def load(prefix, mode, s):
    p = os.path.join(root, f"docs/bench-{prefix}-{mode}-{s}.json")
    return json.load(open(p))["programs"] if os.path.exists(p) else None


def ba(mode, col, base, sets):
    print(f"| program | {base} Mcyc | {col} Mcyc | cycles min | cycles median | instructions |")
    print("|---|---|---|---|---|---|")
    for s in sets:
        P = load("bf", mode, s)
        rs = []
        for p, r in P.items():
            a, b = r.get(base, {}), r.get(col, {})
            if "cycles_min" not in a or "cycles_min" not in b:
                continue
            q = (b["cycles_min"] / a["cycles_min"], b["cycles_median"] / a["cycles_median"],
                 b["instructions_min"] / a["instructions_min"])
            rs.append(q)
            print(f"| {s}/{p} | {a['cycles_min'] / 1e6:.1f} | {b['cycles_min'] / 1e6:.1f} | {q[0]:.3f} | {q[1]:.3f} | {q[2]:.3f} |")
        print(f"| **{s} geomean ({len(rs)})** | | | **{gm([q[0] for q in rs]):.3f}** | **{gm([q[1] for q in rs]):.3f}** | **{gm([q[2] for q in rs]):.3f}** |")


def worst(mode, col, base, pct):
    lim = 1 + pct / 100
    n = 0
    for s in SETS:
        P = load("bf", mode, s)
        if not P:
            continue
        for p, r in P.items():
            a, b = r.get(base, {}), r.get(col, {})
            if "cycles_min" not in a or "cycles_min" not in b:
                continue
            q = (b["cycles_min"] / a["cycles_min"], b["cycles_median"] / a["cycles_median"],
                 b["instructions_min"] / a["instructions_min"])
            n += 1
            if q[0] > lim or q[2] > lim:
                print(f"{s}/{p}: cycles min {q[0]:.3f} median {q[1]:.3f} instructions {q[2]:.3f} ({a['cycles_min'] / 1e6:.1f} -> {b['cycles_min'] / 1e6:.1f} Mcyc)")
    print(f"({n} programs compared)")


def vsbp(col):
    print("| set | ours/PUC min (BF) | ours/PUC min (BP) | ours/LJ min (BF) | ours/LJ min (BP) | ours/LJ median (BF) | ours/LJ median (BP) |")
    print("|---|---|---|---|---|---|---|")
    allv = {k: [] for k in ("pf", "pp", "lf", "lp", "df", "dp")}
    for s in SETS:
        F, B = load("bf", "default", s), load("bp", "default", s)
        v = {k: [] for k in allv}
        for p, r in F.items():
            o, f_ref, f_lj = r.get(col, {}), r.get("ref", {}), r.get("lj", {})
            ob = B.get(p, {}).get(col, {})
            if "cycles_min" not in o:
                continue
            if "cycles_min" in f_ref:
                v["pf"].append(o["cycles_min"] / f_ref["cycles_min"])
            if "cycles_min" in ob and "cycles_min" in B[p].get("ref", {}):
                v["pp"].append(ob["cycles_min"] / B[p]["ref"]["cycles_min"])
            if "cycles_min" in f_lj:
                v["lf"].append(o["cycles_min"] / f_lj["cycles_min"])
                v["df"].append(o["cycles_median"] / f_lj["cycles_median"])
            if "cycles_min" in ob and "cycles_min" in B[p].get("lj", {}):
                v["lp"].append(ob["cycles_min"] / B[p]["lj"]["cycles_min"])
                v["dp"].append(ob["cycles_median"] / B[p]["lj"]["cycles_median"])
        for k in allv:
            allv[k] += v[k]
        print(f"| {s} | " + " | ".join(f"{gm(v[k]):.3f}" for k in allv) + " |")
    print("| **all** | " + " | ".join(f"{gm(allv[k]):.3f}" for k in allv) + " |")


if __name__ == "__main__":
    a = sys.argv[1:]
    if a[0] == "ba":
        ba(a[1], a[2], a[3], a[4:])
    elif a[0] == "worst":
        worst(a[1], a[2], a[3], float(a[4]) if len(a) > 4 else 5.0)
    else:
        vsbp(a[2])
