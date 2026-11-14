#!/usr/bin/env python3
"""vs-baseline tables: pass A (5 runs) and the 15-run serial re-runs, plain and PGO, for two baselines rebuilt in the same session:
b9 = 9a9b8ed (session-bm5 subject), b8 = 8a18eb3 (baseline of the previous release table).
usage: vs_baseline.py <outdir> > vs_baseline.md"""
import json, math, os, sys
D = sys.argv[1] if len(sys.argv) > 1 else "."
A = json.load(open(os.path.join(D, "passA_cpus3-7_all_columns.json")))["results"]
RR = {}
for f in sorted(os.listdir(D)):
    if f.startswith("rerun55") and f.endswith(".json"):
        for k, v in json.load(open(os.path.join(D, f)))["results"].items():
            RR.setdefault(k, v)  # the first file (the main re-run) wins
def v(R, k, c, f):
    x = [y for y in R.get(k, {}).get("cols", {}).get(c, {}).get(f, []) if y]
    return min(x) if x else None
def r(R, k, a, b, f):
    x, y = v(R, k, a, f), v(R, k, b, f)
    return x / y if x and y else None
gm = lambda xs: math.exp(sum(math.log(x) for x in xs) / len(xs))
f3 = lambda x: "-" if x is None else f"{x:.3f}"
P = print
P("# vs baselines (same session, all rebuilt with scripts/release-build.sh): pass A min of 5, 15-run serial CPU-4 re-run where available\n")
P("Subject = HEAD 108b935. Baseline 9a9b8ed = the session-bm5 subject; baseline 8a18eb3 = the squashed first commit, the baseline of the previous release table. 5.5 builds, plain and PGO.\n")
REAL = {"real_dis_x86", "real_dis_arm", "real_dis_mips", "real_dis_ppc"}
for bn, bp, bg in (("9a9b8ed", "b9p", "b9g"), ("8a18eb3", "b8p", "b8g")):
    P(f"\n# Baseline {bn}\n")
    for lab, a, b in (("plain", "plain", bp), ("PGO", "pgo", bg)):
        cs = [r(A, k, a, b, "cyc") for k in A if r(A, k, a, b, "cyc")]
        ins = [r(A, k, a, b, "ins") for k in A if r(A, k, a, b, "ins")]
        P(f"- {lab}/{bn} geomean over {len(cs)} programs: cycles {gm(cs):.3f}, instructions {gm(ins):.3f}; "
          f">3% faster: {sum(1 for x in cs if x < 0.97)}, within 3%: {sum(1 for x in cs if 0.97 <= x <= 1.03)}, >3% slower: {sum(1 for x in cs if x > 1.03)}")
    for lab, a, b in (("plain", "plain", bp), ("PGO", "pgo", bg)):
        rows = []
        for k in sorted(A):
            c, i = r(A, k, a, b, "cyc"), r(A, k, a, b, "ins")
            if c is None: continue
            c15 = r(RR, k, a, b, "cyc") if k in RR else None
            i15 = r(RR, k, a, b, "ins") if k in RR else None
            rows.append((k, c, i, c15, i15, v(A, k, a, "cyc") / 1e6))
        def row(x):
            k, c, i, c15, i15, m = x
            use = c15 if c15 is not None else c
            ui = i if i15 is None else i15
            tag = ""
            if abs(use - 1) > 0.03 and ui is not None and abs(ui - 1) <= 0.01: tag = "layout lottery (instructions equal)"
            elif ui is not None and abs(use - 1) > 0.03 and abs(ui - 1) > 0.01 and (ui - 1) * (use - 1) > 0: tag = "real (instructions move the same way)"
            return f"| {k} | {m:.0f} | {f3(c)} | {f3(i)} | {f3(c15)} | {f3(i15)} | {tag} |"
        H = "| program | Mcyc | cyc A | ins A | cyc 15-run | ins 15-run | note |\n|---|---|---|---|---|---|---|"
        P(f"\n## {lab}/{bn}: top-20 improvements (programs >= 30 Mcyc)\n"); P(H)
        for x in sorted([x for x in rows if x[5] >= 30], key=lambda x: x[1])[:20]: P(row(x))
        P(f"\n## {lab}/{bn}: programs >3% worse in pass A (all sizes)\n"); P(H)
        for x in sorted([x for x in rows if x[1] > 1.03], key=lambda x: -x[1]): P(row(x))
    P(f"\n## Per set, geomean plain/{bn} and PGO/{bn} (cycles, instructions), pass A\n")
    P("| set | n | plain cyc | plain ins | PGO cyc | PGO ins |\n|---|---|---|---|---|---|")
    sets = {}
    for k in A:
        s = "real_dis" if k.split("/")[-1] in REAL else ("/".join(k.split("/")[:2]) if k.startswith("corpus2") else k.split("/")[0])
        sets.setdefault(s, []).append(k)
    for s, ks in sorted(sets.items()):
        g = lambda a, b, f: gm([r(A, k, a, b, f) for k in ks if r(A, k, a, b, f)])
        P(f"| {s} | {len(ks)} | {g('plain',bp,'cyc'):.3f} | {g('plain',bp,'ins'):.3f} | {g('pgo',bg,'cyc'):.3f} | {g('pgo',bg,'ins'):.3f} |")
