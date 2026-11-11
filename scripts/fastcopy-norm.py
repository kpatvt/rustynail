#!/usr/bin/env python3
"""Relocation-normalised disassembly of one function of a linked binary (docs/JIT_TRACE.md TR23.1).

usage: fastcopy-norm.py <binary> <demangled symbol>

Prints one line per instruction: the offset from the function start and the instruction with
  * branch / call targets inside the function as offsets from its start, outside it as `EXT`,
  * rip-relative displacements, absolute-address comments and `<symbol+off>` annotations masked.
Two builds whose fast interpreter copy differs only in where the linker placed things print identical text.
"""
import re
import subprocess
import sys

binp, sym = sys.argv[1], sys.argv[2]
nm = subprocess.run(["nm", "-C", "-n", "--defined-only", binp], capture_output=True, text=True, check=True).stdout
syms = []
for line in nm.splitlines():
    p = line.split(" ", 2)
    if len(p) == 3 and p[1] in "tTwW":
        syms.append((int(p[0], 16), p[2]))
start = end = None
for i, (a, n) in enumerate(syms):
    if n == sym:
        start = a
        end = next(b for b, _ in syms[i + 1:] if b > a)
        break
if start is None:
    sys.exit("symbol not found: " + sym)
out = subprocess.run(
    ["objdump", "-d", "-C", "--no-show-raw-insn", "-M", "intel", f"--start-address={start}", f"--stop-address={end}", binp],
    capture_output=True, text=True, check=True,
).stdout.splitlines()
for line in out:
    m = re.match(r"^\s*([0-9a-f]+):\t(.*)$", line)
    if not m:
        continue
    addr, ins = int(m.group(1), 16), m.group(2).strip()
    mb = re.match(r"^(j\w+|call|loop\w*)\s+([0-9a-f]+)\s+<(.*)>$", ins)
    if mb:
        t = int(mb.group(2), 16)
        tgt = f"@{t - start:#x}" if start <= t < end else "EXT"
        ins = f"{mb.group(1)} {tgt}"
    else:
        ins = re.sub(r"\[rip\+0x[0-9a-f]+\]", "[rip+DISP]", ins)
        ins = re.sub(r"\s*#\s*[0-9a-f]+ <.*>$", " # ABS", ins)
        ins = re.sub(r"<[^>]*>", "<SYM>", ins)
    print(f"{addr - start:#x}\t{ins}")
