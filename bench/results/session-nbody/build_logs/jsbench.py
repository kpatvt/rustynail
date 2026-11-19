#!/usr/bin/env python3
"""node vs ours vs LuaJIT on the 10 bench/js ports. min of 5 interleaved runs, taskset CPU 4, perf stat cycles:u (all threads) and wall."""
import json, os, subprocess, sys, tempfile, time
ROOT = "/home/themadmin/Projects/rustynail/.claude/worktrees/agent-aad64bca5daa7f0b7"
B = os.path.expanduser("~/.cache/rnnb/bin")
OUT = sys.argv[1]
progs = ["binary_trees", "fannkuch", "fib", "heapsort", "mandelbrot", "matmul", "nbody", "queens", "sieve", "spectral_norm"]
TMP = os.path.expanduser("~/.cache/sb/tmp")
env = dict(os.environ, RUSTYNAIL_SEED="0")
def run(cmd):
    with tempfile.NamedTemporaryFile("r", suffix=".perf", dir=TMP) as f:
        t = time.perf_counter()
        p = subprocess.run(["perf", "stat", "-x,", "-o", f.name, "-e", "cycles:u,instructions:u", "--", "taskset", "-c", "4"] + cmd, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, stdin=subprocess.DEVNULL, timeout=300, env=env)
        w = time.perf_counter() - t
        r = {}
        for line in f.read().splitlines():
            fl = line.split(",")
            if len(fl) > 2 and fl[0].isdigit(): r[fl[2].split(":")[0]] = int(fl[0])
        return p.stdout.decode().strip(), w, r
res = {}
for p in progs:
    cols = {"node": ["node", f"{ROOT}/bench/js/{p}.js"], "plain": [f"{B}/p55/rnlua", f"{ROOT}/bench/programs/{p}.lua"],
            "pgo": [f"{B}/g55/rnlua", f"{ROOT}/bench/programs/{p}.lua"], "luajit": ["/usr/bin/luajit", f"{ROOT}/bench/programs/{p}.lua"]}
    names = list(cols); d = {n: {"cyc": [], "ins": [], "wall": [], "out": None} for n in names}
    for i in range(5):
        for k in range(len(names)):
            n = names[(i + k) % len(names)]
            subprocess.run(["taskset", "-c", "4", "python3", "-c", "import time\nt=time.time()\nwhile time.time()-t<0.3: pass"])
            o, w, r = run(cols[n])
            d[n]["cyc"].append(r.get("cycles")); d[n]["ins"].append(r.get("instructions")); d[n]["wall"].append(w); d[n]["out"] = o
    res[p] = d
    print(p, {n: (round(min(d[n]["wall"]), 3), min(x for x in d[n]["cyc"] if x) // 10**6 if any(d[n]["cyc"]) else None) for n in names}, "out-equal(plain,node):", d["plain"]["out"] == d["node"]["out"], flush=True)
json.dump(res, open(OUT, "w"), indent=1)
