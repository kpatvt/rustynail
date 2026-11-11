#!/usr/bin/env python3
"""Benchmark runner (V10): compares interpreters on bench/programs/*.lua.

  scripts/bench.py --bin ref=ref/55/build/lua --bin ours=target/release/rnlua [options]

For every program and binary: one warm-up run (its stdout must be identical for all binaries), then
`--runs` timed runs pinned to one core (wall time, peak RSS via wait4) and one `perf stat` run
(instructions:u, cycles:u, branch-misses:u; the low-noise signal on this machine, whose CPU governor
is `powersave`). The first binary is the baseline of all ratios. Writes JSON with `--json`.
"""
import argparse, glob, json, math, os, re, shlex, statistics, subprocess, sys, time

def run_once(cmd, cpu):
    full = ["taskset", "-c", str(cpu)] + cmd
    t0 = time.perf_counter()
    p = subprocess.Popen(full, stdout=subprocess.PIPE, stderr=subprocess.PIPE, stdin=subprocess.DEVNULL)
    out, err = p.communicate()
    return p.returncode, out, err, time.perf_counter() - t0

def perf_stat(cmd, cpu):
    full = ["perf", "stat", "-x,", "-e", "instructions:u,cycles:u,branch-misses:u", "taskset", "-c", str(cpu)] + cmd
    p = subprocess.run(full, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE, stdin=subprocess.DEVNULL)
    res = {}
    for line in p.stderr.decode().splitlines():
        f = line.split(",")
        if len(f) > 2 and f[0].isdigit():
            res[f[2].split(":")[0]] = int(f[0])
    return res

def rss_kb(cmd, cpu):
    import resource
    full = ["taskset", "-c", str(cpu)] + cmd
    p = subprocess.Popen(full, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, stdin=subprocess.DEVNULL)
    _, status, ru = os.wait4(p.pid, 0)
    return ru.ru_maxrss

def geomean(xs):
    return math.exp(sum(math.log(x) for x in xs) / len(xs)) if xs else float("nan")

def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--bin", action="append", required=True, help="name=path or name='path args', e.g. lj='luajit -joff' (first = baseline)")
    ap.add_argument("--runs", type=int, default=7)
    ap.add_argument("--cpu", type=int, default=3)
    ap.add_argument("--only", default="", help="comma separated benchmark names")
    ap.add_argument("--args", default="", help="extra argument passed to every program")
    ap.add_argument("--json", default="")
    ap.add_argument("--no-perf", action="store_true")
    a = ap.parse_args()
    bins = [tuple(b.split("=", 1)) for b in a.bin]
    root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    progs = sorted(glob.glob(os.path.join(root, "bench/programs/*.lua")))
    only = [x for x in a.only.split(",") if x]
    results = {}
    for p in progs:
        name = os.path.basename(p)[:-4]
        if only and name not in only:
            continue
        results[name] = {}
        ref_out = None
        for bn, bp in bins:
            cmd = shlex.split(bp) + [p] + ([a.args] if a.args else [])
            rc, out, err, _ = run_once(cmd, a.cpu)  # warm-up + correctness
            if rc != 0:
                print(f"{name}/{bn}: exit {rc}: {err.decode()[:200]}", file=sys.stderr)
                results[name][bn] = {"error": err.decode()[:200]}
                continue
            if ref_out is None:
                ref_out = out
            elif out != ref_out:
                print(f"{name}/{bn}: OUTPUT DIFFERS from baseline", file=sys.stderr)
                results[name][bn] = {"error": "output differs"}
                continue
            times = [run_once(cmd, a.cpu)[3] for _ in range(a.runs)]
            r = {"min": min(times), "median": statistics.median(times), "rss_kb": rss_kb(cmd, a.cpu)}
            if not a.no_perf:
                r.update(perf_stat(cmd, a.cpu))
            results[name][bn] = r
        print(f"  ran {name}", file=sys.stderr, flush=True)
    base = bins[0][0]
    for bn, _ in bins[1:]:
        print(f"\n{bn} vs {base}  (ratios > 1 mean {bn} is slower)")
        print(f"{'benchmark':14} {'base ms':>9} {bn+' ms':>9} {'time':>6} {'instr':>6} {'br-miss':>7} {'rss':>6}")
        tr, ir = [], []
        for name, r in results.items():
            b, o = r.get(base, {}), r.get(bn, {})
            if "min" not in b or "min" not in o:
                print(f"{name:14} {'ERROR':>9}")
                continue
            t = o["min"] / b["min"]
            i = o["instructions"] / b["instructions"] if "instructions" in o and "instructions" in b else float("nan")
            br = o["branch-misses"] / b["branch-misses"] if "branch-misses" in o and b.get("branch-misses") else float("nan")
            rs = o["rss_kb"] / b["rss_kb"]
            tr.append(t)
            if not math.isnan(i): ir.append(i)
            print(f"{name:14} {b['min']*1000:9.0f} {o['min']*1000:9.0f} {t:6.2f} {i:6.2f} {br:7.2f} {rs:6.2f}")
        print(f"{'GEOMEAN':14} {'':9} {'':9} {geomean(tr):6.3f} {geomean(ir):6.3f}   max time ratio {max(tr):.2f}")
    if a.json:
        json.dump(results, open(a.json, "w"), indent=1)

main()
