#!/usr/bin/env python3
"""Startup wall time (ms, taskset -c 4, min / median of N interleaved runs) of PUC, LuaJIT, base plain / PGO (cde96490), plain, PGO
on bench/startup/hello.lua and ten_ms.lua. Single core, nothing else running. Prints a markdown table (docs/bench-mfull-startup.md).
usage: bench_mfull_startup.py [N=100]   (binary paths as in scripts/bench-mfull.sh, via the same environment variables)"""
import os, statistics, subprocess, sys, time

root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
D = os.path.expanduser("~/.cache/mf-bin")
n = int(sys.argv[1]) if len(sys.argv) > 1 else 100
cols = [("PUC", [os.environ.get("PUC", os.path.join(root, "ref/55/build-seed0/lua"))]),
        ("LuaJIT", ["/usr/bin/luajit"]),
        ("base plain", ["env", "RUSTYNAIL_SEED=0", f"{D}/d/rnlua"]), ("base PGO", ["env", "RUSTYNAIL_SEED=0", f"{D}/e/rnlua"]),
        ("plain", ["env", "RUSTYNAIL_SEED=0", f"{D}/a/rnlua"]), ("PGO", ["env", "RUSTYNAIL_SEED=0", f"{D}/b/rnlua"])]
progs = ["hello.lua", "ten_ms.lua"]
t = {(p, c): [] for p in progs for c, _ in cols}
for _ in range(n):
    for p in progs:
        for c, cmd in cols:
            s = time.perf_counter()
            subprocess.run(["taskset", "-c", "4"] + cmd + [os.path.join(root, "bench/startup", p)], stdin=subprocess.DEVNULL,
                           stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
            t[(p, c)].append((time.perf_counter() - s) * 1000)
print("| program | " + " | ".join(c for c, _ in cols) + " |")
print("|---|" + "---|" * len(cols))
for p in progs:
    print(f"| {p} (ms, min / median of {n}) | " + " | ".join(f"{min(t[(p, c)]):.2f} / {statistics.median(t[(p, c)]):.2f}" for c, _ in cols) + " |")
