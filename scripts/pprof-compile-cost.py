#!/usr/bin/env python3
"""JIT compile cost per program from the pprof-record.py reports: cycles in rn_jit:: compile symbols (JIT compile category,
trace-on run) next to the number of traces installed / recorded (stats). usage: pprof-compile-cost.py RECDIR [RECDIR ...]"""
import glob, os, re, sys

src = open(os.path.join(os.path.dirname(os.path.abspath(__file__)), "pprof-classify.py")).read().replace("\nmain()\n", "\n")
sys.argv = [sys.argv[0]] + sys.argv[1:]
dirs = sys.argv[1:]
exec(src)
print("| program | total Mcyc | JIT compile Mcyc (share) | Lua compiler Mcyc (share) | traces recorded / installed | JIT compile Mcyc per installed trace |")
print("|---|---|---|---|---|---|")
for rec in dirs:
    for f in sorted(glob.glob(rec + "/*.on.rep")):
        p = os.path.basename(f)[:-7]
        tot, rows = parse(f)
        cat, _ = classify(rows)
        jc, lc = cat["JIT compile"] / 100 * tot / 1e6, cat["Lua compiler"] / 100 * tot / 1e6
        st = open(f"{rec}/{p}.stats", errors="replace").read() if os.path.exists(f"{rec}/{p}.stats") else ""
        m = re.search(r"\[trace\] recorded (\d+) installed (\d+)", st)
        rc, ins = (int(m.group(1)), int(m.group(2))) if m else (0, 0)
        per = f"{jc / ins:.2f}" if ins else "-"
        print(f"| {p} | {tot / 1e6:.0f} | {jc:.1f} ({cat['JIT compile']:.1f}%) | {lc:.1f} ({cat['Lua compiler']:.1f}%) | {rc} / {ins} | {per} |")
