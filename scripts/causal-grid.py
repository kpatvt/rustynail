#!/usr/bin/env python3
"""Causal ablation grid (docs/STUDY_CAUSAL_GAP.md): pinned, spun, interleaved perf-stat runs of configurations x programs.

  scripts/causal-grid.py SPEC.json OUT.json

SPEC = {"cpu": 4, "spin_ms": 300, "runs": 9, "runs_over": {"awfy/permute": 15},
        "events": "cycles:u,instructions:u",            # one perf-stat group per run (<= 6 events on Zen 5: no multiplexing)
        "programs": ["awfy/permute", ...],                 # bench/<p>.lua, run from the repo root with no argument
        "args": {"awfy/towers": ["90"]},                   # optional per-program arguments
        "configs": [{"name": "lj", "cmd": ["/usr/bin/luajit", "-O-sink"], "env": {}}, ...],
        "check": true}                                     # false: do not compare stdout (scripts/causal-reps.lua)

Every run: spin the CPU for spin_ms, then `perf stat -x, -e EVENTS -- taskset -c CPU <cmd> bench/<p>.lua <args>` with stdin
from /dev/null. The configuration order is rotated per run index. Every environment is padded (RUSTYNAIL_PAD) to the same
length, so stack alignment does not differ between configurations. stdout is compared with bench/<p>.expected when it exists,
else with the first configuration's stdout. Results are written after every program:
  {"spec": ..., "results": {prog: {cfg: {"ev": {event: [values]}, "out": "ok|diff|err", "rc": [..]}}}}
Run it under the memory cap and a timeout (CLAUDE.md).
"""
import json, os, subprocess, sys, tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))


def spin(cpu, ms):
    if ms > 0:
        subprocess.run(["taskset", "-c", str(cpu), "python3", "-c",
                        f"import time\nt = time.time()\nwhile time.time() - t < {ms / 1000.0}: pass"], stdin=subprocess.DEVNULL)


def measure(cmd, env, cpu, events, tmpdir, timeout):
    with tempfile.NamedTemporaryFile("r", suffix=".perf", dir=tmpdir) as f:
        full = ["perf", "stat", "-x,", "-o", f.name, "-e", events, "--", "taskset", "-c", str(cpu)] + cmd
        try:
            p = subprocess.run(full, stdout=subprocess.PIPE, stderr=subprocess.PIPE, stdin=subprocess.DEVNULL,
                               timeout=timeout, env=env, cwd=ROOT)
        except subprocess.TimeoutExpired:
            return -9, b"", {}
        res = {}
        for line in f.read().splitlines():
            fld = line.split(",")
            if len(fld) > 2 and fld[0].strip().isdigit():
                res[fld[2]] = int(fld[0])
        return p.returncode, p.stdout, res


def main():
    spec = json.load(open(sys.argv[1]))
    out = sys.argv[2]
    cpu, ms, runs = spec.get("cpu", 4), spec.get("spin_ms", 300), spec.get("runs", 9)
    events = spec.get("events", "cycles:u,instructions:u")
    timeout = spec.get("timeout", 120)
    base_env = {k: v for k, v in os.environ.items() if not k.startswith("RUSTYNAIL_")}
    base_env["RUSTYNAIL_SEED"] = "0"
    cfgs = spec["configs"]
    envlen = lambda c: sum(len(k) + len(v) + 2 for k, v in c.get("env", {}).items())
    m = max(envlen(c) for c in cfgs) + 8
    envs = []
    for c in cfgs:
        e = dict(base_env)
        e.update(c.get("env", {}))
        e["RUSTYNAIL_PAD"] = "x" * (m - envlen(c))
        envs.append(e)
    tmpdir = tempfile.mkdtemp(prefix="cgrid", dir=spec.get("tmp", "/tmp"))
    results = {}
    if os.path.exists(out) and spec.get("resume"):
        results = json.load(open(out)).get("results", {})
    for prog in spec["programs"]:
        if prog in results:
            continue
        path = os.path.join("bench", prog + ".lua")
        args = spec.get("args", {}).get(prog, [])
        exp_path = os.path.join(ROOT, "bench", prog + ".expected")
        expected = open(exp_path, "rb").read() if os.path.exists(exp_path) and not args else None
        r = {c["name"]: {"ev": {}, "rc": [], "out": "ok"} for c in cfgs}
        n = spec.get("runs_over", {}).get(prog, runs)
        ref = expected
        for i in range(n):
            order = list(range(len(cfgs)))
            k = i % len(order)
            order = order[k:] + order[:k]
            for j in order:
                c = cfgs[j]
                spin(cpu, ms)
                rc, so, ev = measure(c["cmd"] + [path] + args, envs[j], cpu, events, tmpdir, c.get("timeout", timeout))
                rr = r[c["name"]]
                rr["rc"].append(rc)
                for kk, v in ev.items():
                    rr["ev"].setdefault(kk, []).append(v)
                if rc != 0:
                    rr["out"] = "err"
                elif not spec.get("check", True):
                    pass
                elif ref is None and j == 0:
                    ref = so
                elif ref is not None and so != ref and rr["out"] == "ok":
                    rr["out"] = "diff"
        results[prog] = r
        json.dump({"spec": spec, "results": results}, open(out + ".tmp", "w"), indent=0)
        os.replace(out + ".tmp", out)
        cyc = {c["name"]: sorted(r[c["name"]]["ev"].get("cycles:u", [0]))[len(r[c["name"]]["ev"].get("cycles:u", [0])) // 2]
               for c in cfgs}
        print(prog, " ".join(f"{k}={v / 1e6:.1f}{'' if r[k]['out'] == 'ok' else '!' + r[k]['out']}" for k, v in cyc.items()),
              flush=True)


if __name__ == "__main__":
    main()
