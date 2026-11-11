#!/usr/bin/env python3
"""Line-based delta debugging of a diverging jit_diff program (docs/JIT_TESTING.md).

    systemd-run --user --scope -q -p MemoryMax=8G -p MemorySwapMax=0 \
        scripts/jit-minimize.py <jit_diff binary> <src.lua> <out.lua> [cfg]

`cfg` is the jit_diff config byte as a decimal number (default 128 = raw source; bit 0/1 GC mode,
bits 2-3 watchdog). A candidate "still fails" when the interpreter and JIT logs differ (the program
must compile and the watchdog must not fire; a crash of the target counts as failing too).
"""
import os
import subprocess
import sys
import tempfile

binary, src, out = sys.argv[1:4]
cfg = int(sys.argv[4]) if len(sys.argv) > 4 else 128
cfg |= 0x80
work = tempfile.mkdtemp(prefix="jitmin.")
runs = 0


def fails(lines):
    global runs
    runs += 1
    body = "\n".join(lines).encode()
    inp = os.path.join(work, "in.bin")
    with open(inp, "wb") as f:
        f.write(bytes([cfg]) + body)
    for n in ("interp.log", "jit.log"):
        try:
            os.unlink(os.path.join(work, n))
        except FileNotFoundError:
            pass
    env = dict(os.environ, JIT_DIFF_OUT=work)  # JIT_DIFF_TICKS=n in the environment is passed through
    try:
        r = subprocess.run([binary, inp], env=env, stdin=subprocess.DEVNULL,
                           stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=60)
    except subprocess.TimeoutExpired:
        return False
    if r.returncode == 0:
        return False
    a = os.path.join(work, "interp.log")
    b = os.path.join(work, "jit.log")
    if os.path.exists(a) and os.path.exists(b):
        return open(a, "rb").read() != open(b, "rb").read()
    return True  # target crashed before/while running the JIT side


lines = open(src).read().split("\n")
if not fails(lines):
    sys.exit("the input does not fail with cfg %d" % cfg)
n = 2
while len(lines) >= 2:
    chunk = max(1, len(lines) // n)
    reduced = False
    for i in range(0, len(lines), chunk):
        cand = lines[:i] + lines[i + chunk:]
        if cand and fails(cand):
            lines = cand
            n = max(n - 1, 2)
            reduced = True
            break
    if not reduced:
        if chunk == 1:
            break
        n = min(len(lines), n * 2)
open(out, "w").write("\n".join(lines) + "\n")
print("minimized to %d lines after %d runs -> %s" % (len(lines), runs, out))
