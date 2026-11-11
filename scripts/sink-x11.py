#!/usr/bin/env python3
"""S0 experiment X11 (docs/JIT_TRACE.md 6.8, S0 check 5): PHIs, `Compile(phi)` aborts and sinks with and without the TR3b
writeback (`+wb`), per program. usage: scripts/sink-x11.py <rnlua> [dir ...]   (default: bench/programs bench/jit-trace/sink_*)
Prints one row per program: peeled loops / PHIs / wb stores / Compile(phi) aborts / tables sunk for {sink, sink +wb}.
Counters are process-wide and count compile attempts (a trace that later aborts is counted too)."""
import glob, os, re, subprocess, sys
binp = sys.argv[1]
progs = sorted(glob.glob("bench/programs/*.lua")) + sorted(glob.glob("bench/jit-trace/sink_*.lua"))
env0 = {"RUSTYNAIL_SEED": "0", "RUSTYNAIL_JIT": "on", "RUSTYNAIL_JIT_T15": "off", "RUSTYNAIL_JIT_TRACE": "on",
        "RUSTYNAIL_JIT_VERBOSE": "1"}
variants = [("nosink", "-sink"), ("nosink+wb", "-sink,+wb"), ("sink", ""), ("sink+wb", "+wb")]


def run(p, opt):
    e = dict(os.environ)
    e.update(env0)
    if opt:
        e["RUSTYNAIL_JIT_TRACE_OPT"] = opt
    r = subprocess.run(["timeout", "300", binp, p], env=e, stdin=subprocess.DEVNULL, capture_output=True, text=True)
    err = r.stderr
    m = re.search(r"sink: tables (\d+) traces (\d+); loops peeled (\d+) phis (\d+) wb-stores (\d+)", err)
    phi_ab = sum(int(n) for n in re.findall(r"aborted Compile\(phi\): (\d+)", err))
    inst = re.search(r"installed (\d+)", err)
    return (int(m.group(3)), int(m.group(4)), int(m.group(5)), phi_ab, int(m.group(1)),
            int(inst.group(1)) if inst else 0) if m else None


tot = {v: [0, 0, 0, 0, 0, 0] for v, _ in variants}
print(f"{'program':<18}" + "".join(f"{v:>34}" for v, _ in variants))
print(f"{'':<18}" + "".join(f"{'peeled/phis/wb/phiab/sunk/inst':>34}" for _ in variants))
for p in progs:
    row = []
    for v, o in variants:
        r = run(p, o)
        row.append(r)
        if r:
            for i in range(6):
                tot[v][i] += r[i]
    print(f"{os.path.basename(p)[:-4]:<18}" + "".join(f"{('/'.join(map(str, r)) if r else 'n/a'):>34}" for r in row))
print(f"{'TOTAL':<18}" + "".join(f"{'/'.join(map(str, tot[v])):>34}" for v, _ in variants))
