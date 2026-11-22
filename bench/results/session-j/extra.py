#!/usr/bin/env python3
"""extra analyses for the report: instruction regressions, short programs, jit-trace set, lazystub/presize targets. Uses per_program.csv"""
import csv, math, os
D = os.path.dirname(os.path.abspath(__file__))
rows = list(csv.DictReader(open(D + "/per_program.csv")))
f = lambda r, c, k: float(r[f"{c}_{k}_min"]) if r[f"{c}_{k}_min"] else None
gm = lambda xs: math.exp(sum(math.log(x) for x in xs) / len(xs)) if xs else float("nan")
print("== instruction ratio vs session-i > 1.005 (any build)")
for r in rows:
    out = []
    for c, p in (("plain", "prev_plain"), ("pgo", "prev_pgo"), ("p54", "prev_p54"), ("g54", "prev_g54")):
        a, b = f(r, c, "ins"), f(r, p, "ins")
        if a and b and a / b > 1.005:
            out.append(f"{c} ins {a/b:.3f} cyc {f(r,c,'cyc')/f(r,p,'cyc'):.3f}")
    if out:
        print(r["program"], "; ".join(out))
print("== short programs (< 40 Mcyc plain), cycles vs session-i, plain/pgo/p54/g54")
sh = []
for r in rows:
    if f(r, "plain", "cyc") < 40e6:
        v = [f(r, c, "cyc") / f(r, p, "cyc") for c, p in (("plain", "prev_plain"), ("pgo", "prev_pgo"), ("p54", "prev_p54"), ("g54", "prev_g54"))]
        sh.append(v)
        if max(v) > 1.03:
            print(r["program"], " ".join(f"{x:.3f}" for x in v))
print("short programs n", len(sh), "geomean", [round(gm([s[i] for s in sh]), 3) for i in range(4)])
print("== jit-trace set vs session-i geomean")
jt = [r for r in rows if r["program"].startswith("jit-trace/")]
for c, p in (("plain", "prev_plain"), ("pgo", "prev_pgo"), ("p54", "prev_p54"), ("g54", "prev_g54")):
    print(c, "cyc", round(gm([f(r, c, "cyc") / f(r, p, "cyc") for r in jt]), 3), "ins", round(gm([f(r, c, "ins") / f(r, p, "ins") for r in jt]), 3), "worst", max((f(r, c, "cyc") / f(r, p, "cyc"), r["program"]) for r in jt))
print("== targets (cyc/ins vs session-i) plain pgo p54 g54")
for k in ["corpus2/heldout/dijkstra_routes", "corpus/lua_lexer", "corpus/classic_vector", "awfy/cd", "apps/expr_lang", "apps/raytrace", "apps/lru_cache", "corpus/astar_jumper", "awfy/raytrace", "corpus/expr_lang"]:
    for r in rows:
        if r["program"] == k:
            print(k, " | ".join(f"{c} {f(r,c,'cyc')/f(r,p,'cyc'):.3f}/{f(r,c,'ins')/f(r,p,'ins'):.3f}" for c, p in (("plain", "prev_plain"), ("pgo", "prev_pgo"), ("p54", "prev_p54"), ("g54", "prev_g54"))))
print("== 1a5aad0-total for targets (plain, p54)")
for k in ["corpus/classic_vector", "corpus2/heldout/dijkstra_routes", "corpus/lua_lexer"]:
    for r in rows:
        if r["program"] == k:
            print(k, f"plain vs h {f(r,'plain','cyc')/f(r,'h_plain','cyc'):.3f} p54 vs h {f(r,'p54','cyc')/f(r,'h_p54','cyc'):.3f}")
