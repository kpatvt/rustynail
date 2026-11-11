#!/usr/bin/env python3
"""BUNDLE-P: peak RSS (ru_maxrss KiB, min of N runs) and the Lua heap size (collectgarbage('count'), KiB, read at the
end of the program) of PUC, LuaJIT and our binaries on the programs of the given sets.
usage: bp_rss.py N name=cmd [name=cmd ...] -- set [set ...]      names starting with `lj` run bench/lj/<set>/<p>.lua when it exists
(run under systemd-run + timeout; uses scripts/pprof-maxrss.c as the parent so that no python floor is in ru_maxrss)"""
import glob, math, os, subprocess, sys, tempfile

here = os.path.dirname(os.path.abspath(__file__))
root = os.path.dirname(here)
n = int(sys.argv[1])
i = sys.argv.index("--")
bins = [(b.split("=", 1)[0], b.split("=", 1)[1].split()) for b in sys.argv[2:i]]
sets = sys.argv[i + 1:]
tmp = tempfile.mkdtemp(prefix="bprss")
tool = os.path.join(tmp, "maxrss")
subprocess.check_call(["cc", "-O2", "-o", tool, os.path.join(here, "pprof-maxrss.c")])
wrapper = os.path.join(tmp, "cnt.lua")
open(wrapper, "w").write('local f = arg[1]; arg = {[0] = f}; dofile(f); io.stderr:write("COUNT ", collectgarbage("count"), "\\n")\n')


def path_for(name, s, p):
    if name.startswith("lj"):
        c = os.path.join(root, "bench/lj", s, p + ".lua")
        if os.path.exists(c):
            return c
    return os.path.join(root, "bench", s, p + ".lua")


def rss(cmd):
    best = None
    for _ in range(n):
        r = subprocess.run(["taskset", "-c", "4", tool] + cmd, stdout=subprocess.PIPE, stdin=subprocess.DEVNULL)
        t = r.stdout.decode().strip()
        if t.isdigit():
            best = int(t) if best is None else min(best, int(t))
    return best


def count(cmd):
    r = subprocess.run(cmd, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE, stdin=subprocess.DEVNULL)
    for line in r.stderr.decode().splitlines():
        if line.startswith("COUNT "):
            return float(line.split()[1])
    return None


print("| set/program | " + " | ".join(f"{nm} RSS MiB" for nm, _ in bins) + " | " + " | ".join(f"{nm} heap MiB" for nm, _ in bins) + " |")
print("|---|" + "---|" * (2 * len(bins)))
tot = {nm: [] for nm, _ in bins}
for s in sets:
    for f in sorted(glob.glob(os.path.join(root, "bench", s, "*.lua"))):
        p = os.path.basename(f)[:-4]
        r, c = [], []
        for nm, cmd in bins:
            path = path_for(nm, s, p)
            r.append(rss(cmd + [path]))
            c.append(count(cmd + [wrapper, path]))
        if any(x is None for x in r):
            continue
        for (nm, _), x in zip(bins, r):
            tot[nm].append(x)
        print(f"| {s}/{p} | " + " | ".join(f"{x / 1024:.1f}" for x in r) + " | " + " | ".join(f"{x / 1024:.1f}" if x else "-" for x in c) + " |", flush=True)
g = lambda xs: math.exp(sum(math.log(x) for x in xs) / len(xs))
base = bins[0][0]
print("\ngeomean RSS ratio to " + base + ": " + ", ".join(f"{nm} {g([a / b for a, b in zip(tot[nm], tot[base])]):.2f}" for nm, _ in bins))
