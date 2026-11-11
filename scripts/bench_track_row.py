#!/usr/bin/env python3
"""Appends a row to docs/bench-history.tsv from a bench_cycles.py json (columns ref, cur, j4, dup).
usage: bench_track_row.py run.json history.tsv rev load1 noisy label"""
import json, math, sys, time, os

run, hist, rev, load1, noisy, label = sys.argv[1:7]
d = json.load(open(run))
progs = list(d["programs"])


def g(xs):
    xs = [x for x in xs if x]
    return math.exp(sum(math.log(x) for x in xs) / len(xs)) if xs else float("nan")


def cyc(p, n):
    return d["programs"][p].get(n, {}).get("cycles_min")


def ratio(a, b):
    return g([cyc(p, a) / cyc(p, b) for p in progs if cyc(p, a) and cyc(p, b)])


cols = ["date", "rev", "label", "load1", "noisy", "runs", "cpu", "n_progs",
        "gm_cur_vs_ref", "gm_j4_vs_ref", "gm_dup_vs_ref", "gm_cur_vs_dup", "gm_cur_vs_j4",
        "worst_cur_vs_dup", "worst_prog"] + [f"{p}_cur_Mcyc" for p in progs]
worst = max(progs, key=lambda p: (cyc(p, "cur") / cyc(p, "dup")) if cyc(p, "cur") and cyc(p, "dup") else 0)
row = [time.strftime("%Y-%m-%dT%H:%M"), rev, label, load1, "NOISY" if noisy == "1" else "quiet", d["runs"], d["cpu"], len(progs),
       f"{ratio('cur', 'ref'):.4f}", f"{ratio('j4', 'ref'):.4f}", f"{ratio('dup', 'ref'):.4f}", f"{ratio('cur', 'dup'):.4f}",
       f"{ratio('cur', 'j4'):.4f}", f"{cyc(worst, 'cur') / cyc(worst, 'dup'):.4f}", worst] + \
      [f"{cyc(p, 'cur') / 1e6:.1f}" if cyc(p, "cur") else "-" for p in progs]
new = not os.path.exists(hist)
if not new:
    first = open(hist).readline().rstrip("\n").split("\t")
    if first != cols:  # program set changed: start a new header block
        new = True
with open(hist, "a") as f:
    if new:
        f.write("\t".join(cols) + "\n")
    f.write("\t".join(str(x) for x in row) + "\n")
print("\t".join(cols))
print("\t".join(str(x) for x in row))
