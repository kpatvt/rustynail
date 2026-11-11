#!/usr/bin/env python3
"""BUNDLE-P: markdown tables from docs/bench-<BENCH_PREFIX, default bp>-<mode>-<set>.json (scripts/bench_lj.py output).

usage: bench_bp_report.py geo  default|warm0 COL        geomean table of column COL against PUC (`ref`) and LuaJIT (`lj`)
       bench_bp_report.py var  default COL BASE          column COL against column BASE per set (geomean / worst / best program)
       bench_bp_report.py rows default|warm0 SET COL     per-program rows (Mcyc min, ours/PUC, ours/LJ min and median)
       bench_bp_report.py top  default COL [N]           largest ours/LJ ratios (min and median) over all sets
Sets: awfy apps programs jit-trace lib real. Ratios are cycles:u. warm0 files hold only our columns: ratios against
ref / lj are then taken from the default files (same programs, other run)."""
import json, math, os, sys

root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SETS = ["awfy", "apps", "programs", "jit-trace", "lib", "real"]


def gm(x):
    x = [v for v in x if v and v > 0]
    return math.exp(sum(math.log(v) for v in x) / len(x)) if x else float("nan")


def load(mode, s):
    p = os.path.join(root, f"docs/bench-{os.environ.get('BENCH_PREFIX', 'bp')}-{mode}-{s}.json")
    return json.load(open(p))["programs"] if os.path.exists(p) else None


def rows_for(mode, s, col):
    P, D = load(mode, s), load("default", s)
    if P is None:
        return []
    out = []
    for p, r in P.items():
        d = D.get(p, {}) if D else {}
        o = r.get(col, {})
        if "cycles_min" not in o:
            continue
        out.append((p, o, d.get("ref") or r.get("ref") or {}, d.get("lj") or r.get("lj") or {}))
    return out


def geo(mode, col):
    A = {k: [] for k in ("pm", "pd", "lm", "ld", "lq")}
    allnz = sorted(((max(o["cycles_median"] / o["cycles_min"], j["cycles_median"] / j["cycles_min"]), s + "/" + p)
                    for s in SETS for p, o, _, j in rows_for(mode, s, col) if "cycles_min" in j), reverse=True)
    t5 = {p for _, p in allnz[:5]}
    print("5 noisiest programs overall (max of median/min of ours or LuaJIT): " + ", ".join(f"{p} {x:.2f}" for x, p in allnz[:5]) + "\n")
    print(f"| set (n) | ours/PUC min | ours/PUC median | ours/LJ min (n with LJ) | ours/LJ median | ours/LJ median without the 5 noisiest overall | dropped here |")
    print("|---|---|---|---|---|---|---|")
    for s in SETS:
        rows = rows_for(mode, s, col)
        if not rows:
            continue
        pm = [o["cycles_min"] / r["cycles_min"] for _, o, r, _ in rows if "cycles_min" in r]
        pd = [o["cycles_median"] / r["cycles_median"] for _, o, r, _ in rows if "cycles_median" in r]
        lm = [o["cycles_min"] / j["cycles_min"] for _, o, _, j in rows if "cycles_min" in j]
        ld = [o["cycles_median"] / j["cycles_median"] for _, o, _, j in rows if "cycles_median" in j]
        lq = [o["cycles_median"] / j["cycles_median"] for p, o, _, j in rows if "cycles_min" in j and s + "/" + p not in t5]
        for k, v in zip(A, (pm, pd, lm, ld, lq)):
            A[k] += v
        print(f"| {s} ({len(rows)}) | {gm(pm):.3f} | {gm(pd):.3f} | {gm(lm):.3f} ({len(lm)}) | {gm(ld):.3f} | {gm(lq):.3f} | {sum(1 for p, o, _, j in rows if s + '/' + p in t5)} |")
    print(f"| **all** ({len(A['pm'])}) | {gm(A['pm']):.3f} | {gm(A['pd']):.3f} | {gm(A['lm']):.3f} ({len(A['lm'])}) | {gm(A['ld']):.3f} | {gm(A['lq']):.3f} | |")


def var(col, base, mode="default"):
    print(f"| set (n) | {col}/{base} cycles min | median | instructions | worst program | best program |")
    print("|---|---|---|---|---|---|")
    allr = []
    for s in SETS:
        P = load(mode, s)
        if P is None:
            continue
        rs = []
        for p, r in P.items():
            if "cycles_min" in r.get(col, {}) and "cycles_min" in r.get(base, {}):
                rs.append((r[col]["cycles_min"] / r[base]["cycles_min"], r[col]["cycles_median"] / r[base]["cycles_median"],
                           r[col]["instructions_min"] / r[base]["instructions_min"], p))
        if not rs:
            continue
        allr += rs
        w, b = max(rs), min(rs)
        print(f"| {s} ({len(rs)}) | {gm([x[0] for x in rs]):.3f} | {gm([x[1] for x in rs]):.3f} | {gm([x[2] for x in rs]):.3f} | {w[3]} {w[0]:.3f} | {b[3]} {b[0]:.3f} |")
    print(f"| all ({len(allr)}) | {gm([x[0] for x in allr]):.3f} | {gm([x[1] for x in allr]):.3f} | {gm([x[2] for x in allr]):.3f} | | |")


def prow(mode, s, col):
    print("| program | Mcyc min | PUC Mcyc | LJ Mcyc | ours/PUC | ours/LJ min | ours/LJ median |")
    print("|---|---|---|---|---|---|---|")
    for p, o, r, j in rows_for(mode, s, col):
        f = lambda e: f"{e['cycles_min'] / 1e6:.1f}" if "cycles_min" in e else "-"
        pu = f"{o['cycles_min'] / r['cycles_min']:.3f}" if "cycles_min" in r else "-"
        lm = f"{o['cycles_min'] / j['cycles_min']:.2f}" if "cycles_min" in j else "-"
        ld = f"{o['cycles_median'] / j['cycles_median']:.2f}" if "cycles_median" in j else "-"
        print(f"| {p} | {f(o)} | {f(r)} | {f(j)} | {pu} | {lm} | {ld} |")


def top(col, n):
    allr = []
    for s in SETS:
        for p, o, _, j in rows_for("default", s, col):
            if "cycles_min" in j:
                allr.append((o["cycles_min"] / j["cycles_min"], o["cycles_median"] / j["cycles_median"], f"{s}/{p}"))
    print("| rank | program | ours/LJ min | ours/LJ median |")
    print("|---|---|---|---|")
    for i, (a, b, p) in enumerate(sorted(allr, reverse=True)[:n]):
        print(f"| {i + 1} | {p} | {a:.2f} | {b:.2f} |")
    print(f"\nprograms with min ratio > 2: {sum(1 for a, _, _ in allr if a > 2)} of {len(allr)}; with median ratio > 2: {sum(1 for _, b, _ in allr if b > 2)}")


if __name__ == "__main__":
    a = sys.argv[1:]
    {"geo": lambda: geo(a[1], a[2]), "var": lambda: var(a[2], a[3], a[1]), "rows": lambda: prow(a[1], a[2], a[3]),
     "top": lambda: top(a[2], int(a[3]) if len(a) > 3 else 15)}[a[0]]()
