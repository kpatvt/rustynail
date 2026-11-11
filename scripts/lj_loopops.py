#!/usr/bin/env python3
"""For each bench program: the IR opcodes LuaJIT keeps inside the loop of the trace of the benchmark body loop (after the
`------ LOOP ------` marker of `luajit -jdump=i`). A loop body without any CALL*/load/store/alloc opcode means LuaJIT
folded or hoisted the benchmarked work.  Usage: lj_loopops.py file.lua...  (stdin /dev/null)

H1: the trace is picked by source line: for the generated bench/lib programs the body loop is the LAST
`for k = 1, K do` line (earlier ones build the input arrays); for other programs the loop trace with the most IR ops
is used.  Output per program: `name: loop-ir=N [cyc/iter-ish: not measured here] ops=OP*count ...` where the loop-control
opcodes (ADD of the index, LE, PHI) are listed under `ctl=`.  `--lines` also prints the raw IR of that trace's loop part."""
import re, subprocess, sys

ansi = re.compile(r"\x1b\[[0-9;]*m")
CTL = {"PHI", "LE", "LT", "GE", "GT", "NOP", "RENAME"}


def analyse(f, lines=False):
    p = subprocess.run(["/usr/bin/luajit", "-jdump=i", f], stdin=subprocess.DEVNULL, capture_output=True, text=True, timeout=300)
    txt = ansi.sub("", p.stdout)
    src = open(f).read().splitlines()
    kl = [n + 1 for n, l in enumerate(src) if l.strip() == "for k = 1, K do"]
    body_line = kl[-1] if kl else 0
    parts = re.split(r"---- TRACE (\d+) IR", txt)
    starts = dict((m.group(1), int(m.group(2))) for m in re.finditer(r"---- TRACE (\d+) start \S+?:(\d+)", txt))
    best = None
    for i in range(1, len(parts), 2):
        tid = parts[i]
        body = parts[i + 1].split("---- TRACE")[0]
        if "LOOP ------------" not in body:
            continue
        if kl and starts.get(tid, 0) < body_line:
            continue
        post = body.split("LOOP ------------")[1]
        ops = []
        raw = []
        for l in post.splitlines():
            m = re.match(r"\s*\d+\s+[>+ ]*\s*(?:\w+\s+)?([A-Z][A-Z0-9]+)", l)
            if m:
                ops.append(m.group(1))
                raw.append(l)
        if best is None or len(ops) > len(best[0]):
            best = (ops, raw)
    if best is None:
        return None
    return best


def main():
    lines = "--lines" in sys.argv
    for f in [a for a in sys.argv[1:] if not a.startswith("--")]:
        b = analyse(f, lines)
        if b is None:
            print(f"{f}: no loop trace")
            continue
        ops, raw = b
        cnt = {}
        for o in ops:
            cnt[o] = cnt.get(o, 0) + 1
        work = {o: c for o, c in cnt.items() if o not in CTL}
        print(f"{f}: loop-ir={len(ops)} ops=" + " ".join(f"{o}x{c}" if c > 1 else o for o, c in sorted(work.items())))
        if lines:
            print("\n".join(raw))


main()
