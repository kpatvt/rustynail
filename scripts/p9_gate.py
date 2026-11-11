#!/usr/bin/env python3
"""J5 P9: evaluates the prototype gate criteria 2-6 and the baseline band from the two bench_cycles.py JSON files
(docs/JIT_TRACE.md 1.6).   scripts/p9_gate.py micro.json progs.json
Columns: j4 (baseline), t1, tr, ref89 (main@89963c6 binary, T15=on)."""
import json, math, sys


def gm(xs):
    return math.exp(sum(math.log(x) for x in xs) / len(xs))


micro = json.load(open(sys.argv[1]))["programs"]
progs = json.load(open(sys.argv[2]))["programs"]


def c(src, p, col):
    return src[p][col]["cycles_min"] / 1e6


def ratio(src, p, a, b):
    return c(src, p, a) / c(src, p, b)


def src_of(p):
    return micro if p in micro else progs


GATE_B = ["licm_expr", "array_rec", "upval_acc", "int_rec2"]
GATE_C = ["inline_leaf", "inline_2"]
GATE_A = ["spectral_norm", "matmul"]
INFO_B = ["int_sum", "float_poly", "float_loop", "dot", "upval_inv"]
INFO_P = ["loops_arith", "mandelbrot", "fannkuch", "heapsort", "queens", "sieve"]
D = [("type_entry", "type_entry_stable"), ("type_late", "type_late_stable")]
res = {}


def line(s=""):
    print(s)


def verdict(ok):
    return "PASS" if ok else "FAIL"


for basecol, label in (("j4", "baseline = J4 column of the J5 binary (the gate's definition)"),
                       ("ref89", "sensitivity: baseline = main@89963c6 binary, T15=on")):
    line(f"=== {label}")
    r = {p: ratio(src_of(p), p, "tr", basecol) for p in GATE_A + GATE_B + GATE_C + INFO_B + INFO_P}
    line(f"{'program':<18}{'base Mcyc':>11}{'tr Mcyc':>10}{'tr/base':>9}{'t1/base':>9}")
    for grp, names in (("(a)", GATE_A), ("(b)", GATE_B), ("(c)", GATE_C), ("(b')", INFO_B), ("info", INFO_P)):
        for p in names:
            s = src_of(p)
            line(f"{p:<18}{c(s, p, basecol):>11.1f}{c(s, p, 'tr'):>10.1f}{r[p]:>9.3f}{ratio(s, p, 't1', basecol):>9.3f}  {grp}")
    sp, mm = r["spectral_norm"], r["matmul"]
    gb = gm([r[p] for p in GATE_B])
    gall = gm([r[p] for p in GATE_A + GATE_B + GATE_C])
    worst_gate = max(GATE_A + GATE_B + GATE_C, key=lambda p: r[p])
    gi = gm([r[p] for p in INFO_B + INFO_P])
    worst_info = max(INFO_B + INFO_P, key=lambda p: r[p])
    line(f"crit 2 spectral_norm {sp:.3f} (<= 0.70) {verdict(sp <= 0.70)}")
    line(f"crit 3 matmul        {mm:.3f} (<= 0.95) {verdict(mm <= 0.95)}")
    line(f"crit 4 set (b) geomean {gb:.3f} (<= 0.90) {verdict(gb <= 0.90)}")
    line(f"crit 5a gate set a+b+c geomean {gall:.3f} (<= 0.85) {verdict(gall <= 0.85)}; worst gate program {worst_gate} {r[worst_gate]:.3f} (<= 1.05) {verdict(r[worst_gate] <= 1.05)}")
    line(f"crit 6 informational (b' + 6) geomean {gi:.3f} (<= 1.15) {verdict(gi <= 1.15)}; worst {worst_info} {r[worst_info]:.3f} (<= 1.30) {verdict(r[worst_info] <= 1.30)}")
    above = [(p, r[p]) for p in INFO_B + INFO_P if r[p] > 1.05]
    line("   informational programs above 1.05 x base: " + ", ".join(f"{p} {v:.3f}" for p, v in above))
    line()

line("=== criterion 5b: every program of the 25 (bench/programs), tr / t1 (<= 1.05)")
bad = []
for p in sorted(progs):
    v = ratio(progs, p, "tr", "t1")
    flag = " <-- above 1.05" if v > 1.05 else ""
    if v > 1.05:
        bad.append(p)
    line(f"  {p:<14}{c(progs, p, 'tr'):>9.1f} / {c(progs, p, 't1'):>9.1f} = {v:.3f}{flag}")
line(f"crit 5b {verdict(not bad)}; above: {bad}")
line()
line("=== criterion 5c: set (d), tr column of each program vs the tr column of its stable twin (<= 1.05)")
for a, b in D:
    v = ratio(micro, a, "tr", "tr") and c(micro, a, "tr") / c(micro, b, "tr")
    line(f"  {a:<12} {c(micro, a, 'tr'):>8.1f} / {b:<18} {c(micro, b, 'tr'):>8.1f} = {v:.3f} {verdict(v <= 1.05)}")
line()
line("=== baseline verification: J5 binary J4 column vs main@89963c6 binary (T15=on), ref89 / j4, band +-5%")
out = []
for name, src in (("micro", micro), ("programs", progs)):
    for p in sorted(src):
        v = c(src, p, "ref89") / c(src, p, "j4")
        if abs(v - 1) > 0.05:
            out.append(f"{p} {v:.3f}")
        # instruction counts
    line(f"  {name}: outside the band: " + (", ".join(out) if out else "none"))
    out = []
line()
line("=== all programs, instructions:u identity of ref89 vs j4 (max relative difference)")
for name, src in (("micro", micro), ("programs", progs)):
    worst = max(src, key=lambda p: abs(src[p]["ref89"]["instructions_min"] / src[p]["j4"]["instructions_min"] - 1))
    line(f"  {name}: worst {worst} {src[worst]['ref89']['instructions_min'] / src[worst]['j4']['instructions_min']:.5f}")
