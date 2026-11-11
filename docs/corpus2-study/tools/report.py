#!/usr/bin/env python3
"""Table + group geomeans from a bench2.py JSON. usage: report.py data/main.json [base_col] [other cols...]"""
import json, math, statistics, sys

d = json.load(open(sys.argv[1]))
R = d["results"]
cols = list(next(iter(R.values())).keys())


def group(k):
    s, p = k.split("/")
    if s == "corpus":
        return "corpus18"
    if s in ("dev", "heldout"):
        return s + ("-real" if p.startswith("real_") else "-new")
    return s


def gm(xs):
    xs = [x for x in xs if x and x > 0]
    return math.exp(sum(map(math.log, xs)) / len(xs)) if xs else float("nan")


rows = []
for k in sorted(R, key=lambda k: (group(k), k)):
    r = R[k]
    mn = {c: (min(r[c]["cyc"]) / 1e6 if r[c]["cyc"] else None) for c in cols}
    md = {c: (statistics.median(r[c]["cyc"]) / 1e6 if r[c]["cyc"] else None) for c in cols}
    ok = {c: r[c]["ok"] for c in cols}
    rows.append((k, group(k), mn, md, ok))

if "lj" in cols and "ours" in cols:
    print("| group | program | PUC Mcyc | LJ Mcyc | ours Mcyc | ours/LJ min | ours/LJ med | ours/PUC | flags |")
    print("|---|---|---|---|---|---|---|---|---|")
    agg = {}
    for k, g, mn, md, ok in rows:
        rl = mn["ours"] / mn["lj"]
        rlm = md["ours"] / md["lj"]
        rp = mn["ours"] / mn["puc"]
        flags = ",".join(c + "-out" for c in cols if not ok[c])
        print(f"| {g} | {k.split('/')[1]} | {mn['puc']:.0f} | {mn['lj']:.0f} | {mn['ours']:.0f} | **{rl:.2f}** | {rlm:.2f} | {rp:.3f} | {flags} |")
        a = agg.setdefault(g, [[], [], []])
        a[0].append(rl); a[1].append(rlm); a[2].append(rp)
    print("\n| group | n | ours/LJ geomean (min) | ours/LJ geomean (median) | ours/PUC geomean | programs > 1.5x LJ |")
    print("|---|---|---|---|---|---|")
    for g, a in agg.items():
        print(f"| {g} | {len(a[0])} | {gm(a[0]):.3f} | {gm(a[1]):.3f} | {gm(a[2]):.3f} | {sum(1 for x in a[0] if x > 1.5)} |")
    allnew = [x for g, a in agg.items() if g.startswith(("dev", "heldout")) for x in a[0]]
    print(f"\nall new programs (dev+heldout): n={len(allnew)} ours/LJ geomean {gm(allnew):.3f}")
else:
    base = sys.argv[2] if len(sys.argv) > 2 else cols[0]
    others = [c for c in cols if c != base]
    print("| program | " + " | ".join(others) + " |")
    print("|---|" + "---|" * len(others))
    agg = {c: [] for c in others}
    for k, g, mn, md, ok in rows:
        cells = []
        for c in others:
            r = mn[c] / mn[base] if mn[c] and mn[base] else float("nan")
            agg[c].append(r)
            cells.append(f"{r:.3f}{'' if ok[c] else '!'}")
        print(f"| {k} | " + " | ".join(cells) + " |")
    print("| **geomean** | " + " | ".join(f"{gm(agg[c]):.3f}" for c in others) + " |")
