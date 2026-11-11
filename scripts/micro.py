#!/usr/bin/env python3
"""Instructions per loop iteration of each micro-benchmark (bench/micro), minus the empty loop."""
import glob, os, subprocess, sys
bins = [b.split("=", 1) for b in sys.argv[1:]]
root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
def instr(b, p):
    r = subprocess.run(["perf", "stat", "-x,", "-e", "instructions:u", "taskset", "-c", "3", b, p],
                       stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)
    for l in r.stderr.decode().splitlines():
        f = l.split(",")
        if len(f) > 2 and f[0].isdigit(): return int(f[0])
N = 20000000
base = {n: instr(b, root + "/bench/micro/empty.lua") for n, b in bins}
print(f"{'micro':16}" + "".join(f"{n:>10}" for n, _ in bins) + "   ratio   (instr / loop iteration, empty loop removed; 4 ops per iteration)")
for p in sorted(glob.glob(root + "/bench/micro/*.lua")):
    name = os.path.basename(p)[:-4]
    if name == "empty": continue
    vals = [(instr(b, p) - base[n]) / N for n, b in bins]
    print(f"{name:16}" + "".join(f"{v:10.1f}" for v in vals) + (f"  {vals[-1]/vals[0]:6.2f}" if len(vals) > 1 and vals[0] > 0 else ""))
print("empty loop:", {n: base[n] / N for n in base})
