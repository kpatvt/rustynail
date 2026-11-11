#!/usr/bin/env python3
"""BUNDLE-P: trace coverage table (shares of cycles in trace code / tier-1 / interpreter / helpers+exit / compile / other).

  bp_coverage.py record OUTDIR LUA_CLI SET [SET ...]   perf record -c 150001 (cycles:u, JIT perf map) of every program of
                                                       bench/<SET>, default mode, RUSTYNAIL_SEED=0, on CPU $CPU (default 2),
                                                       perf report -> OUTDIR/<set>/<prog>.on.rep
  bp_coverage.py table OUTDIR SET [SET ...]            markdown table per set + cycle-weighted line (classifier of
                                                       scripts/pprof-classify.py, same columns as docs/bench-b1-coverage.md:
                                                       helpers+exit = JIT helpers + trace exit (Rust); compile = JIT compile;
                                                       the Lua compiler, GC, allocator, tables, library ... are 'other')
Run under systemd-run + timeout (CLAUDE.md); profile on a CPU other than the benchmark CPUs 4 / 12."""
import os, re, subprocess, sys, glob, collections

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)


def load_classifier():
    src = open(os.path.join(HERE, "pprof-classify.py")).read().replace("\nmain()\n", "\n")
    ns = {"__name__": "pprof_classify"}
    exec(compile(src, "pprof-classify.py", "exec"), ns)
    return ns


def record(outdir, lua, sets):
    cpu = os.environ.get("CPU", "2")
    for s in sets:
        os.makedirs(os.path.join(outdir, s), exist_ok=True)
        for prog in sorted(glob.glob(os.path.join(ROOT, "bench", s, "*.lua"))):
            name = os.path.basename(prog)[:-4]
            data = os.path.join(outdir, s, name + ".data")
            env = dict(os.environ, RUSTYNAIL_SEED="0", RUSTYNAIL_JIT_PERFMAP="1")
            subprocess.run(["perf", "record", "-q", "-o", data, "-e", "cycles:u", "-c", "150001", "taskset", "-c", cpu, lua, prog],
                           env=env, stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=900)
            rep = subprocess.run(["perf", "report", "-i", data, "--no-children", "--stdio", "--sort", "dso,sym", "-g", "none",
                                  "--percent-limit", "0.02"], stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, stdin=subprocess.DEVNULL)
            open(os.path.join(outdir, s, name + ".on.rep"), "wb").write(rep.stdout)
            os.remove(data)
            print(s, name, flush=True)


def shares(ns, path):
    tot, rows = ns["parse"](path)
    cat, _ = ns["classify"](rows)
    t, t1, it = cat["trace code"], cat["tier-1 (J4) code"], cat["interpreter dispatch"]
    h, c = cat["JIT helpers"] + cat["trace exit (Rust)"], cat["JIT compile"]
    s = sum(cat.values())
    other = max(0.0, s - t - t1 - it - h - c)
    k = 100.0 / s if s else 0
    return [t * k, t1 * k, it * k, h * k, c * k, other * k], tot / 1e6


def table(outdir, sets):
    ns = load_classifier()
    allw = collections.defaultdict(float)
    for s in sets:
        print(f"#### {s}\n| program | trace | tier-1 | interp | helpers+exit | compile | other | Mcyc |\n|---|---|---|---|---|---|---|---|")
        w = [0.0] * 6
        tm = 0.0
        for f in sorted(glob.glob(os.path.join(outdir, s, "*.on.rep"))):
            sh, m = shares(ns, f)
            print(f"| {os.path.basename(f)[:-7]} | " + " | ".join(f"{x:.0f}" for x in sh) + f" | {m:.0f} |")
            w = [a + b * m for a, b in zip(w, sh)]
            tm += m
        if tm:
            print("| **cycle-weighted** | " + " | ".join(f"{x / tm:.1f}" for x in w) + f" | {tm:.0f} |\n")


if __name__ == "__main__":
    if sys.argv[1] == "record":
        record(sys.argv[2], sys.argv[3], sys.argv[4:])
    else:
        table(sys.argv[2], sys.argv[3:])
