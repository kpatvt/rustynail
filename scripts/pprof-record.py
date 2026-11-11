#!/usr/bin/env python3
"""P-PROF step 1: perf record (cycles:u) of programs, plus JIT stats and a bytecode count.

usage: pprof-record.py OUTDIR LUA_CLI prog.lua [prog.lua ...]      (run under systemd-run + timeout, see CLAUDE.md)
env:   CPU=4  FREQ=4999  COUNT=1 (also run the count-hook bytecode counter, slow)
Writes per program P into OUTDIR:  P.on.rep (perf report, all symbols, TRACE on, JIT perf map),
P.off.rep (RUSTYNAIL_JIT=off), P.j4.rep (RUSTYNAIL_JIT_TRACE=off), P.stats (RUSTYNAIL_JIT_VERBOSE=1 stderr),
P.count (bytecodes under JIT=off + hook). perf.data files are deleted after the report is written.
"""
import os, subprocess, sys, re

HERE = os.path.dirname(os.path.abspath(__file__))


def run(cmd, env_extra, out=None, err=None):
    env = dict(os.environ, RUSTYNAIL_SEED="0", **env_extra)
    return subprocess.run(cmd, env=env, stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL,
                          stderr=subprocess.PIPE if err else subprocess.DEVNULL, timeout=600)


def record(outdir, name, tag, lua, prog, env_extra):
    cpu, freq = os.environ.get("CPU", "4"), os.environ.get("FREQ", "4999")
    data = os.path.join(outdir, f"{name}.{tag}.data")
    env_extra = dict(env_extra)
    env_extra["RUSTYNAIL_JIT_PERFMAP"] = "1"
    r = run(["perf", "record", "-q", "-o", data, "-e", "cycles:u", "-F", freq, "taskset", "-c", cpu, lua, prog], env_extra)
    rep = subprocess.run(["perf", "report", "-i", data, "--no-children", "--stdio", "--sort", "dso,sym", "-g", "none",
                          "--percent-limit", "0.05"], stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, stdin=subprocess.DEVNULL)
    with open(os.path.join(outdir, f"{name}.{tag}.rep"), "wb") as f:
        f.write(rep.stdout)
    os.remove(data)
    for fn in os.listdir("/tmp"):  # perf map files of our children
        pass


def main():
    outdir, lua, progs = sys.argv[1], sys.argv[2], sys.argv[3:]
    os.makedirs(outdir, exist_ok=True)
    for prog in progs:
        name = os.path.basename(prog)[:-4]
        print(name, flush=True)
        record(outdir, name, "on", lua, prog, {})
        record(outdir, name, "j4", lua, prog, {"RUSTYNAIL_JIT_TRACE": "off"})
        record(outdir, name, "off", lua, prog, {"RUSTYNAIL_JIT": "off"})
        r = run([lua, prog], {"RUSTYNAIL_JIT_VERBOSE": "1"}, err=True)
        open(os.path.join(outdir, f"{name}.stats"), "wb").write(r.stderr)
        if os.environ.get("COUNT", "1") == "1":
            try:
                r = run([lua, os.path.join(HERE, "pprof-count.lua"), prog], {"RUSTYNAIL_JIT": "off"}, err=True)
                m = re.search(rb"BYTECODES (\d+)", r.stderr)
                open(os.path.join(outdir, f"{name}.count"), "w").write(m.group(1).decode() if m else "fail")
            except subprocess.TimeoutExpired:
                open(os.path.join(outdir, f"{name}.count"), "w").write("timeout")


main()
