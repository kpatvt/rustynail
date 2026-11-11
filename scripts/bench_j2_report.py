#!/usr/bin/env python3
"""BUNDLE-J2: report pieces from docs/bench-j2-<tag>-<set>.json (scripts/bench_cycles.py output, columns named on the command line).

usage: bench_j2_report.py TAG COL BASE [--sets awfy,apps,real] [--targets a,b,c] [--dup DUP] [--rows] [--worst PCT]

  TAG      file group: docs/bench-j2-TAG-<set>.json
  COL      the column under test, BASE the column it is compared with (ratio = COL / BASE, lower is faster)
  --sets   sets to read (default awfy,apps,real)
  --targets  comma list of programs whose geomean is the gate of the item (cycles min, median and instructions)
  --dup    a duplicate of BASE: its deviation from BASE is the noise floor of the run
  --rows   one row per program (Mcyc min of BASE and COL, ratios of cycles min / median and instructions)
  --lj     LJCOL: also print COL / LJCOL per program (cycles min and median) and their geomeans (the LuaJIT comparison of scripts/bench-lj.sh)
  --worst  list the programs more than PCT % worse than BASE in cycles min or instructions (the 5% rule; a row whose instructions are
           unchanged is layout / trace-set noise, see docs/JIT_TRACE_REF.md)
Everything is cycles:u / instructions:u. The geomean line of every set is printed with its count."""
import json
import math
import os
import sys

root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))


def gm(x):
    x = [v for v in x if v and v > 0]
    return math.exp(sum(math.log(v) for v in x) / len(x)) if x else float("nan")


def load(tag, s):
    p = os.path.join(root, f"docs/bench-j2-{tag}-{s}.json")
    return json.load(open(p))["programs"] if os.path.exists(p) else {}


def pair(r, col, base):
    a, b = r.get(base, {}), r.get(col, {})
    if "cycles_min" not in a or "cycles_min" not in b:
        return None
    return (b["cycles_min"] / a["cycles_min"], b["cycles_median"] / a["cycles_median"], b["instructions_min"] / a["instructions_min"], a, b)


def main():
    a = sys.argv[1:]
    if len(a) < 3:
        print(__doc__)
        sys.exit(2)
    tag, col, base = a[:3]
    opt = {}
    flags = set()
    i = 3
    while i < len(a):
        if a[i] in ("--rows",):
            flags.add(a[i])
            i += 1
        else:
            opt[a[i]] = a[i + 1]
            i += 2
    sets = opt.get("--sets", "awfy,apps,real").split(",")
    targets = [t for t in opt.get("--targets", "").split(",") if t]
    allr = []
    print(f"| set (n) | {col}/{base} cycles min | median | instructions | worst (min) | best (min) |")
    print("|---|---|---|---|---|---|")
    per = {}
    for s in sets:
        P = load(tag, s)
        rs = []
        for p, r in P.items():
            q = pair(r, col, base)
            if q:
                rs.append((q[0], q[1], q[2], p, q))
                per[p] = (s, q)
        if not rs:
            continue
        allr += rs
        w, b = max(rs), min(rs)
        print(f"| {s} ({len(rs)}) | {gm([x[0] for x in rs]):.3f} | {gm([x[1] for x in rs]):.3f} | {gm([x[2] for x in rs]):.3f} | {w[3]} {w[0]:.3f} | {b[3]} {b[0]:.3f} |")
    if allr:
        print(f"| **all ({len(allr)})** | **{gm([x[0] for x in allr]):.3f}** | {gm([x[1] for x in allr]):.3f} | {gm([x[2] for x in allr]):.3f} | | |")
    if targets:
        t = [per[p][1] for p in targets if p in per]
        miss = [p for p in targets if p not in per]
        if t:
            print(f"\ntargets ({len(t)}{', missing ' + ','.join(miss) if miss else ''}): cycles min **{gm([x[0] for x in t]):.3f}**, median **{gm([x[1] for x in t]):.3f}**, instructions {gm([x[2] for x in t]):.3f}")
    if "--rows" in flags:
        print(f"\n| program | {base} Mcyc | {col} Mcyc | cycles min | cycles median | instructions |")
        print("|---|---|---|---|---|---|")
        for p, (s, q) in per.items():
            print(f"| {s}/{p} | {q[3]['cycles_min'] / 1e6:.1f} | {q[4]['cycles_min'] / 1e6:.1f} | {q[0]:.3f} | {q[1]:.3f} | {q[2]:.3f} |")
    if "--dup" in opt:
        worst = [1, 1, 1]
        rs = []
        for s in sets:
            for p, r in load(tag, s).items():
                q = pair(r, opt["--dup"], base)
                if q:
                    rs.append(q)
                    for k in range(3):
                        worst[k] = max(worst[k], q[k], 1 / q[k])
        print(f"\nduplicate {opt['--dup']} against {base}: geomean cycles min {gm([x[0] for x in rs]):.3f}, median {gm([x[1] for x in rs]):.3f}, "
              f"instructions {gm([x[2] for x in rs]):.3f}; worst deviation cycles min {worst[0]:.3f}, median {worst[1]:.3f}, instructions {worst[2]:.3f}")
    if "--lj" in opt:
        lc = opt["--lj"]
        rows = []
        print(f"\n| program | {col} Mcyc | {lc} Mcyc | {col}/{lc} cycles min | median |")
        print("|---|---|---|---|---|")
        for s_ in sets:
            for p, r in load(tag, s_).items():
                o, j = r.get(col, {}), r.get(lc, {})
                if "cycles_min" in o and "cycles_min" in j:
                    q = (o["cycles_min"] / j["cycles_min"], o["cycles_median"] / j["cycles_median"])
                    rows.append(q)
                    print(f"| {s_}/{p} | {o['cycles_min'] / 1e6:.1f} | {j['cycles_min'] / 1e6:.1f} | {q[0]:.2f} | {q[1]:.2f} |")
        print(f"| **geomean ({len(rows)})** | | | **{gm([x[0] for x in rows]):.3f}** | **{gm([x[1] for x in rows]):.3f}** |")
    if "--worst" in opt:
        lim = 1 + float(opt["--worst"]) / 100
        print(f"\nprograms more than {opt['--worst']}% worse than {base}:")
        for p, (s, q) in per.items():
            if q[0] > lim or q[2] > lim:
                print(f"  {s}/{p}: cycles min {q[0]:.3f} median {q[1]:.3f} instructions {q[2]:.3f} ({q[3]['cycles_min'] / 1e6:.1f} -> {q[4]['cycles_min'] / 1e6:.1f} Mcyc)")


if __name__ == "__main__":
    main()
