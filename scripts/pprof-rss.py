#!/usr/bin/env python3
"""Peak RSS (ru_maxrss, KiB) of every program of a directory: PUC vs ours (traces on) vs ours (JIT off).
usage: pprof-rss.py REF_LUA OURS_LUA_CLI [progdir=bench/programs] [N=3]   (min of N runs; run under systemd-run + timeout)
Uses scripts/pprof-maxrss.c (compiled on the fly) because a python parent would put a ~9 MB floor under ru_maxrss."""
import glob, os, subprocess, sys, math, tempfile

here = os.path.dirname(os.path.abspath(__file__))
ref, ours = sys.argv[1], sys.argv[2]
pdir = sys.argv[3] if len(sys.argv) > 3 else "bench/programs"
n = int(sys.argv[4]) if len(sys.argv) > 4 else 3
tool = os.path.join(tempfile.mkdtemp(prefix="maxrss"), "maxrss")
subprocess.check_call(["cc", "-O2", "-o", tool, os.path.join(here, "pprof-maxrss.c")])


def rss(cmd, env):
    best = None
    for _ in range(n):
        r = subprocess.run(["taskset", "-c", "4", tool] + cmd, stdout=subprocess.PIPE, stdin=subprocess.DEVNULL,
                           env=dict(os.environ, RUSTYNAIL_SEED="0", **env))
        t = r.stdout.decode().strip()
        if not t.isdigit():
            return None
        best = int(t) if best is None else min(best, int(t))
    return best


rows = []
for p in sorted(glob.glob(os.path.join(pdir, "*.lua"))):
    name = os.path.basename(p)[:-4]
    if name.startswith("mod_"):
        continue
    a, b, c = rss([ref, p], {}), rss([ours, p], {}), rss([ours, p], {"RUSTYNAIL_JIT": "off"})
    rows.append((name, a, b, c))
    print(f"{name:16} ref {a:>9} KiB  ours(trace) {b:>9} ({b / a:5.2f}x)  ours(jit off) {c:>9} ({c / a:5.2f}x)", flush=True)
g = lambda xs: math.exp(sum(math.log(x) for x in xs) / len(xs))
print(f"geomean ours(trace)/ref {g([b / a for _, a, b, _ in rows]):.3f}   ours(jit off)/ref {g([c / a for _, a, _, c in rows]):.3f}")
