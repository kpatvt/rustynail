#!/usr/bin/env python3
"""(Re)generate <prog>.expected next to every program of the given bench directories from the PUC reference and
check that ours and LuaJIT print exactly the same text (H1: every benchmark program must produce identical output
on PUC, ours and LuaJIT).

  scripts/bench_expected.py REF_LUA "OURS_CMD" LUAJIT dir [dir ...]      (run under systemd-run + timeout)

LuaJIT runs the variant bench/lj/<dir-name>/<prog>.lua when it exists (programs with `unpack` instead of
`table.unpack`, ...), skipping programs listed in bench/lj/SKIP.  Exit status 1 if anything differs."""
import glob, os, subprocess, sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))


def run(cmd, path):
    r = subprocess.run(cmd + [path], stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=600)
    return r.returncode, r.stdout, r.stderr.decode(errors="replace")[-200:]


def main():
    ref, ours, lj = sys.argv[1].split(), sys.argv[2].split(), sys.argv[3].split()
    skip = set()
    for line in open(os.path.join(ROOT, "bench/lj/SKIP")):
        line = line.split("#")[0].strip()
        if line:
            skip.add(line)
    bad = 0
    for d in sys.argv[4:]:
        d = os.path.join(ROOT, d) if not os.path.isabs(d) else d
        sset = os.path.basename(d.rstrip("/"))
        for p in sorted(glob.glob(os.path.join(d, "*.lua"))):
            name = os.path.basename(p)[:-4]
            rc, exp, err = run(ref, p)
            if rc != 0:
                print("FAIL(ref)", p, err)
                bad += 1
                continue
            open(p[:-4] + ".expected", "wb").write(exp)
            msg = []
            rc, o, err = run(ours, p)
            if rc != 0 or o != exp:
                msg.append("OURS-DIFF")
            if f"{sset}/{name}" not in skip:
                lp = os.path.join(ROOT, "bench/lj", sset, name + ".lua")
                rc, o, err = run(lj, lp if os.path.exists(lp) else p)
                if rc != 0 or o != exp:
                    msg.append("LJ-DIFF " + err.strip()[:80] + repr(o[:80]))
            if msg:
                bad += 1
            print(f"{sset}/{name:24} {'OK' if not msg else ' '.join(msg)}")
    sys.exit(1 if bad else 0)


main()
