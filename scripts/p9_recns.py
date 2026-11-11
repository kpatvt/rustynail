#!/usr/bin/env python3
"""J5 P9 criterion 7: trace record + compile time as a fraction of the run's wall time.
Needs a `rnlua --features jit,trace` binary that prints `[trace] record+compile ns N` in its VERBOSE summary (the P9
instrumentation: Instant around driver_start/driver_step/driver_abort, scripts/p9_recns.patch).
   scripts/p9_recns.py <binary> [runs]      -> one line per program of bench/programs and bench/jit-trace"""
import glob, os, re, subprocess, sys, time

root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
binary = sys.argv[1]
runs = int(sys.argv[2]) if len(sys.argv) > 2 else 5
env = dict(os.environ, RUSTYNAIL_SEED="0", RUSTYNAIL_JIT="on", RUSTYNAIL_JIT_T15="off", RUSTYNAIL_JIT_TRACE="on",
           RUSTYNAIL_JIT_VERBOSE="1")
worst = 0.0
for f in sorted(glob.glob(root + "/bench/jit-trace/*.lua") + glob.glob(root + "/bench/programs/*.lua")):
    best = None
    for _ in range(runs):
        t0 = time.perf_counter()
        p = subprocess.run(["taskset", "-c", "4", binary, f], env=env, stdin=subprocess.DEVNULL,
                           stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)
        wall = (time.perf_counter() - t0) * 1e9
        m = re.search(rb"record\+compile ns (\d+)", p.stderr)
        if not m:
            continue
        ns = int(m.group(1))
        # the minimum fraction over the runs (the same method as the cycle benchmarks: min)
        fr = ns / wall
        if best is None or fr < best[0]:
            best = (fr, ns, wall)
    if best:
        worst = max(worst, best[0])
        print(f"{os.path.basename(f)[:-4]:<18} record+compile {best[1] / 1e6:9.3f} ms of {best[2] / 1e6:9.1f} ms = {100 * best[0]:6.3f} %")
print(f"worst fraction {100 * worst:.3f} % (limit 2 %)")
