#!/usr/bin/env python3
"""S0 check (4): drift of collectgarbage("count") under allocation sinking (docs/JIT_TRACE.md 6.8, TR29.4).

usage: scripts/sink-drift.py <rnlua>   (a --features trace binary; run under a memory cap, from the repo root)
Runs tests/sink_drift.lua with the JIT off, traces with -sink (must be identical to JIT off) and traces with sinking,
in incremental and generational mode; prints the full-collection differences and the maximum / mean absolute difference
at the 1000 random points, and exits 1 when a bound fails: full collections within FULL_TOL KB, random points within
RAND_TOL KB (a few collector steps' worth of virtual bytes)."""
import os, subprocess, sys
FULL_TOL, RAND_TOL = 2.0, 600.0
binp = sys.argv[1]


def run(env, mode):
    e = dict(os.environ)
    e.update(env)
    e["RUSTYNAIL_SEED"] = "0"
    r = subprocess.run(["timeout", "300", binp, "tests/sink_drift.lua", mode], env=e, stdin=subprocess.DEVNULL,
                       capture_output=True, text=True)
    rows = [l.split() for l in r.stdout.splitlines() if l[:2] in ("R ", "F ")]
    tot = [l for l in r.stdout.splitlines() if l.startswith("total")]
    return rows, tot


tr = {"RUSTYNAIL_JIT": "on", "RUSTYNAIL_JIT_T15": "off", "RUSTYNAIL_JIT_TRACE": "always"}
ok = True
for mode in ("incremental", "generational"):
    off, t0 = run({"RUSTYNAIL_JIT": "off"}, mode)
    nos, t1 = run(dict(tr, RUSTYNAIL_JIT_TRACE_OPT="-sink"), mode)
    on, t2 = run(tr, mode)
    assert len(off) == len(nos) == len(on) > 1000, (len(off), len(nos), len(on))
    assert t0 == t1 == t2, (t0, t1, t2)
    same = off == nos
    f = [abs(float(a[2]) - float(b[2])) for a, b in zip(off, on) if a[0] == "F"]
    r = [abs(float(a[2]) - float(b[2])) for a, b in zip(off, on) if a[0] == "R"]
    print(f"{mode}: -sink identical to JIT off: {same}; full collections: max |diff| {max(f):.3f} KB over {len(f)}; "
          f"random points: max {max(r):.1f} KB mean {sum(r)/len(r):.1f} KB over {len(r)}")
    ok &= same and max(f) <= FULL_TOL and max(r) <= RAND_TOL
print("DRIFT OK" if ok else "DRIFT FAILED")
sys.exit(0 if ok else 1)
