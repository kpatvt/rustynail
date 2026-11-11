#!/usr/bin/env python3
"""Post-M full run (4-way parallel; bplain / bpgo = commit cde96490 binaries; calibration against docs/bench-mfull-single-*.json): report pieces from docs/bench-mfull-<mode>-<set>.json (scripts/bench_lj.py output; columns plain / pgo = this tree,
bplain / bpgo = the BUNDLE-H3 tree 8aef493b (plain, PGO regenerated on it, built in ~/.cache), pgo2 = byte copy of pgo = duplicate column).

usage: bench_mfull_report.py ahead  COL            programs where ours is faster than LuaJIT (min and median), per set and in total
       bench_mfull_report.py ba     MODE COL BASE SET...   per-program before/after rows (Mcyc min, ratio cycles min/median, instructions) + geomeans
       bench_mfull_report.py sets   MODE COL BASE          per set geomeans of COL/BASE (cycles min, median, instructions), worst and best program
       bench_mfull_report.py dup    MODE COL DUP           worst deviation of the duplicate column from COL (cycles min, median, instructions)
       bench_mfull_report.py worst  MODE COL BASE [PCT]    programs where COL is more than PCT % worse than BASE
       bench_mfull_report.py rows   SET COL                awfy / apps style rows: Mcyc, PUC Mcyc, LJ Mcyc, ours/PUC, ours/LJ min and median
       bench_mfull_report.py geo|top|bpvar ...   delegated to bench_bp_report.py with BENCH_PREFIX=mfull (geo MODE COL / top default COL N)
       bench_mfull_report.py cal                          calibration: 4-way / single-core ratios and ours/LuaJIT in both setups
       bench_mfull_report.py movers MODE COL BASE [N]  top N improvements and regressions of COL against BASE (cycles min, median, instructions)
Sets: awfy apps programs jit-trace lib real. Everything is cycles:u."""
import json, math, os, sys

root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SETS = ["awfy", "apps", "programs", "jit-trace", "lib", "real"]


def gm(x):
    x = [v for v in x if v and v > 0]
    return math.exp(sum(math.log(v) for v in x) / len(x)) if x else float("nan")


def load(mode, s):
    p = os.path.join(root, f"docs/bench-mfull-{mode}-{s}.json")
    return json.load(open(p))["programs"] if os.path.exists(p) else None


def ahead(col):
    tot = {"min": [0, 0], "med": [0, 0]}
    print("| set | faster than LuaJIT by min | by median | programs with LuaJIT |")
    print("|---|---|---|---|")
    for s in SETS:
        P = load("default", s)
        n = a = b = 0
        for p, r in P.items():
            o, j = r.get(col, {}), r.get("lj", {})
            if "cycles_min" not in o or "cycles_min" not in j:
                continue
            n += 1
            a += o["cycles_min"] < j["cycles_min"]
            b += o["cycles_median"] < j["cycles_median"]
        tot["min"][0] += a
        tot["med"][0] += b
        tot["min"][1] += n
        print(f"| {s} | {a} | {b} | {n} |")
    print(f"| **all** | **{tot['min'][0]}** | **{tot['med'][0]}** | {tot['min'][1]} |")


def pair(r, col, base):
    a, b = r.get(base, {}), r.get(col, {})
    if "cycles_min" not in a or "cycles_min" not in b:
        return None
    return (b["cycles_min"] / a["cycles_min"], b["cycles_median"] / a["cycles_median"], b["instructions_min"] / a["instructions_min"], a, b)


def ba(mode, col, base, sets):
    print(f"| program | {base} Mcyc | {col} Mcyc | cycles min | cycles median | instructions |")
    print("|---|---|---|---|---|---|")
    for s in sets:
        P = load(mode, s)
        rs = []
        for p, r in P.items():
            q = pair(r, col, base)
            if q is None:
                continue
            rs.append(q)
            print(f"| {s}/{p} | {q[3]['cycles_min'] / 1e6:.1f} | {q[4]['cycles_min'] / 1e6:.1f} | {q[0]:.3f} | {q[1]:.3f} | {q[2]:.3f} |")
        print(f"| **{s} geomean ({len(rs)})** | | | **{gm([q[0] for q in rs]):.3f}** | **{gm([q[1] for q in rs]):.3f}** | **{gm([q[2] for q in rs]):.3f}** |")


def sets(mode, col, base):
    print(f"| set (n) | {col}/{base} cycles min | median | instructions | worst program (min) | best program (min) |")
    print("|---|---|---|---|---|---|")
    allr = []
    for s in SETS:
        P = load(mode, s)
        rs = [(q[0], q[1], q[2], p) for p, r in P.items() if (q := pair(r, col, base))]
        allr += rs
        w, b = max(rs), min(rs)
        print(f"| {s} ({len(rs)}) | {gm([x[0] for x in rs]):.3f} | {gm([x[1] for x in rs]):.3f} | {gm([x[2] for x in rs]):.3f} | {w[3]} {w[0]:.3f} | {b[3]} {b[0]:.3f} |")
    print(f"| **all ({len(allr)})** | **{gm([x[0] for x in allr]):.3f}** | {gm([x[1] for x in allr]):.3f} | {gm([x[2] for x in allr]):.3f} | | |")


def dup(mode, col, d):
    worst = [1, 1, 1]
    for s in SETS:
        P = load(mode, s)
        for p, r in P.items():
            q = pair(r, d, col)
            if q:
                for i in range(3):
                    worst[i] = max(worst[i], q[i], 1 / q[i])
    print(f"duplicate column {d} against {col} ({mode}): worst deviation cycles min {worst[0]:.3f}, median {worst[1]:.3f}, instructions {worst[2]:.3f}")


def worst(mode, col, base, pct):
    lim = 1 + pct / 100
    n = 0
    for s in SETS:
        P = load(mode, s)
        for p, r in P.items():
            q = pair(r, col, base)
            if q is None:
                continue
            n += 1
            if q[0] > lim or q[2] > lim:
                print(f"{s}/{p}: cycles min {q[0]:.3f} median {q[1]:.3f} instructions {q[2]:.3f} ({q[3]['cycles_min'] / 1e6:.1f} -> {q[4]['cycles_min'] / 1e6:.1f} Mcyc)")
    print(f"({n} programs compared)")


def rows(s, col):
    print("| program | ours Mcyc | PUC Mcyc | LJ Mcyc | ours/PUC | ours/LJ min | ours/LJ median |")
    print("|---|---|---|---|---|---|---|")
    P = load("default", s)
    for p, r in P.items():
        o, f, j = r.get(col, {}), r.get("ref", {}), r.get("lj", {})
        if "cycles_min" not in o:
            continue
        g = lambda e: f"{e['cycles_min'] / 1e6:.1f}" if "cycles_min" in e else "-"
        pu = f"{o['cycles_min'] / f['cycles_min']:.3f}" if "cycles_min" in f else "-"
        lm = f"{o['cycles_min'] / j['cycles_min']:.2f}" if "cycles_min" in j else "-"
        ld = f"{o['cycles_median'] / j['cycles_median']:.2f}" if "cycles_median" in j else "-"
        print(f"| {p} | {g(o)} | {g(f)} | {g(j)} | {pu} | {lm} | {ld} |")


def movers(mode, col, base, n):
    allr = []
    for s in SETS:
        P = load(mode, s)
        for p, r in P.items():
            q = pair(r, col, base)
            if q:
                allr.append((q[0], q[1], q[2], f"{s}/{p}", q[3]["cycles_min"] / 1e6, q[4]["cycles_min"] / 1e6))
    allr.sort()
    for title, lst in (("improvements", allr[:n]), ("regressions", allr[::-1][:n])):
        print(f"top {n} {title} ({col} / {base}):\n")
        print("| program | Mcyc base -> this | cycles min | cycles median | instructions |")
        print("|---|---|---|---|---|")
        for a, b, c, p, x, y in lst:
            print(f"| {p} | {x:.1f} -> {y:.1f} | {a:.3f} | {b:.3f} | {c:.3f} |")
        print()


def cal():
    """Calibration: 4-way (default files) against single-core (single files) for the programs of the single run, columns plain/pgo/bpgo."""
    cols = ["plain", "pgo", "bpgo"]
    S, F = {}, {}
    for s in SETS:
        a, b = load("single", s), load("default", s)
        if a:
            for p, r in a.items():
                S[f"{s}/{p}"], F[f"{s}/{p}"] = r, b[p]
    print("| program | " + " | ".join(f"{c} cyc min / med / instr" for c in cols) + " | LJ min / med |")
    print("|---|---|---|---|---|")
    ratios = {c: ([], [], []) for c in cols}
    for k in S:
        cells = []
        for c in cols:
            x, y = S[k][c], F[k][c]
            r = (y["cycles_min"] / x["cycles_min"], y["cycles_median"] / x["cycles_median"], y["instructions_min"] / x["instructions_min"])
            for i in range(3):
                ratios[c][i].append(r[i])
            cells.append(f"{r[0]:.3f} / {r[1]:.3f} / {r[2]:.4f}")
        lj = (F[k]["lj"]["cycles_min"] / S[k]["lj"]["cycles_min"], F[k]["lj"]["cycles_median"] / S[k]["lj"]["cycles_median"])
        print(f"| {k} | " + " | ".join(cells) + f" | {lj[0]:.3f} / {lj[1]:.3f} |")
    print("| **geomean** | " + " | ".join(f"**{gm(ratios[c][0]):.3f}** / {gm(ratios[c][1]):.3f} / {gm(ratios[c][2]):.4f}" for c in cols) + " | |")
    print()
    print("| program | setup | " + " | ".join(f"{c}/LJ min / med" for c in cols) + " |")
    print("|---|---|---|---|---|")
    g = {(c, n, i): [] for c in cols for n in ("single", "4-way") for i in range(2)}
    for k in S:
        for n, D in (("single", S), ("4-way", F)):
            cells = []
            for c in cols:
                a, j = D[k][c], D[k]["lj"]
                m = (a["cycles_min"] / j["cycles_min"], a["cycles_median"] / j["cycles_median"])
                for i in range(2):
                    g[(c, n, i)].append(m[i])
                cells.append(f"{m[0]:.3f} / {m[1]:.3f}")
            print(f"| {k} | {n} | " + " | ".join(cells) + " |")
    for n in ("single", "4-way"):
        print(f"| **geomean** | {n} | " + " | ".join(f"**{gm(g[(c, n, 0)]):.3f}** / {gm(g[(c, n, 1)]):.3f}" for c in cols) + " |")


if __name__ == "__main__":
    a = sys.argv[1:]
    if a[0] in ("geo", "top", "bpvar"):
        os.environ["BENCH_PREFIX"] = "mfull"
        import bench_bp_report as bp
        {"geo": lambda: bp.geo(a[1], a[2]), "top": lambda: bp.top(a[2], int(a[3]) if len(a) > 3 else 15),
         "bpvar": lambda: bp.var(a[2], a[3], a[1])}[a[0]]()
        sys.exit(0)
    if a[0] == "cal":
        cal()
        sys.exit(0)
    if a[0] == "movers":
        movers(a[1], a[2], a[3], int(a[4]) if len(a) > 4 else 10)
        sys.exit(0)
    {"ahead": lambda: ahead(a[1]), "ba": lambda: ba(a[1], a[2], a[3], a[4:]), "sets": lambda: sets(a[1], a[2], a[3]),
     "dup": lambda: dup(a[1], a[2], a[3]), "worst": lambda: worst(a[1], a[2], a[3], float(a[4]) if len(a) > 4 else 5.0),
     "rows": lambda: rows(a[1], a[2])}[a[0]]()
