#!/usr/bin/env python3
"""P-PROF step 3: ceiling arithmetic. For a category set C and a speed-up factor f of those costs, a program whose trace-on
run spends share s of its cycles in C gets new/old = 1 - s * (1 - 1/f). Geomean over the 25 programs of bench/programs.
These are ESTIMATES built on MEASURED shares (perf self cycles, pprof-classify.py); f = inf is the delete-the-cost bound.
usage: pprof-ceilings.py RECDIR"""
import math, os, sys, glob
sys.argv = [sys.argv[0]] + sys.argv[1:]
src = open(os.path.join(os.path.dirname(os.path.abspath(__file__)), "pprof-classify.py")).read().replace("\nmain()\n", "\n")
exec(src)

rec = sys.argv[1]
progs = sorted(os.path.basename(f)[:-7] for f in glob.glob(rec + "/*.on.rep"))
shares = {}
for p in progs:
    tot, rows = parse(f"{rec}/{p}.on.rep")
    shares[p] = classify(rows)[0]

TASKS = [
    ("P-P allocator + GC (alloc+GC)", ["allocator", "GC"]),
    ("P-P allocator only", ["allocator"]),
    ("P-P GC only", ["GC"]),
    ("P-Q table internals + interning", ["table internals", "string interning"]),
    ("P-Q interning only", ["string interning"]),
    ("P-Q table internals only", ["table internals"]),
    ("JIT helper calls (jit_*: get/set/call/len/closure...)", ["JIT helpers"]),
    ("stdlib + libc (library code)", ["stdlib", "libc (memcpy etc.)", "number<->string"]),
    ("call/error machinery + interpreter + metamethods (code no trace covers)", ["interpreter dispatch", "call machinery", "metamethod paths"]),
    ("Lua compiler (parse/codegen)", ["Lua compiler"]),
    ("JIT compile (record/opt/asm)", ["JIT compile"]),
]


def gm(xs):
    return math.exp(sum(math.log(x) for x in xs) / len(xs))


print("| task / category | f = 2 | f = 4 | delete (inf) | top programs (delete bound, new/old) |")
print("|---|---|---|---|---|")
for name, cats in TASKS:
    row = []
    for f in (2, 4, float("inf")):
        r = {p: 1 - sum(shares[p][c] for c in cats) / 100 * (1 - (0 if f == float("inf") else 1 / f)) for p in progs}
        row.append(gm(list(r.values())))
    top = sorted(progs, key=lambda p: sum(shares[p][c] for c in cats), reverse=True)[:4]
    print(f"| {name} | {row[0]:.3f} | {row[1]:.3f} | {row[2]:.3f} | " + ", ".join(f"{p} {1 - sum(shares[p][c] for c in cats) / 100:.2f}" for p in top) + " |")
