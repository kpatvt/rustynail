#!/usr/bin/env python3
"""Correctness: stdout of ours vs PUC for every program under several modes.

  CPUS=0,1,2,8,9,10 LUA=... check.py prog.lua ...   (SMALL file: lines 'name N' giving the small N for stress runs)
Full-size modes compare with <prog>.expected; stress modes compare with PUC run at the small N.
"""
import os, queue, subprocess, sys, threading

PUC = os.environ.get("PUC") or os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..", "..", "ref/55/build-seed0/lua")
LUA = os.environ.get("LUA", os.path.expanduser("~/.cache/corpus2/target/release/rn-cli"))
FULL = [
    ("off", {"RUSTYNAIL_JIT": "off"}),
    ("t1", {"RUSTYNAIL_JIT_TRACE": "off", "RUSTYNAIL_JIT_T15": "off"}),
    ("j4", {"RUSTYNAIL_JIT_TRACE": "off"}),
    ("trdef", {}),
    ("always", {"RUSTYNAIL_JIT": "always", "RUSTYNAIL_JIT_TRACE": "always"}),
    ("hot2", {"RUSTYNAIL_JIT": "on", "RUSTYNAIL_JIT_THRESHOLD": "2", "RUSTYNAIL_JIT_TRACE_HOT": "2,2"}),
    ("noseed", None),
]
SMALLM = [
    ("stress1", {"RUSTYNAIL_GC_STRESS": "1"}),
    ("stress1always", {"RUSTYNAIL_GC_STRESS": "1", "RUSTYNAIL_JIT": "always", "RUSTYNAIL_JIT_TRACE": "always"}),
    ("stress2", {"RUSTYNAIL_GC_STRESS": "2", "RUSTYNAIL_JIT": "always", "RUSTYNAIL_JIT_TRACE": "always"}),
]
extra = os.environ.get("EXTRA_LIMITS", "")
if extra:
    FULL = [(f"lim:{x}", {"RUSTYNAIL_JIT_TRACE_LIMITS": x}) for x in extra.split(";")]
    SMALLM = []


def run(cmd, env, cpu, cwd, timeout):
    e = {k: v for k, v in os.environ.items() if not k.startswith("RUSTYNAIL_")}
    if env is not None:
        e["RUSTYNAIL_SEED"] = "0"
        e.update(env)
    try:
        r = subprocess.run(["taskset", "-c", str(cpu)] + cmd, stdin=subprocess.DEVNULL, capture_output=True, timeout=timeout, env=e, cwd=cwd)
        return r.returncode, r.stdout, r.stderr
    except subprocess.TimeoutExpired:
        return "TIMEOUT", b"", b""


def main():
    cpus = [int(x) for x in os.environ.get("CPUS", "0,1,2,8,9,10").split(",")]
    small = {}
    if os.environ.get("SMALL"):
        for line in open(os.environ["SMALL"]):
            if line.strip():
                n, v = line.split()
                small[n] = v
    outdir = os.path.expanduser("~/.cache/corpus2/diffs")
    os.makedirs(outdir, exist_ok=True)
    q = queue.Queue()
    for p in sys.argv[1:]:
        q.put(os.path.abspath(p))
    lock = threading.Lock()
    tally = {}

    def work(cpu):
        while True:
            try:
                p = q.get_nowait()
            except queue.Empty:
                return
            name = os.path.basename(p)[:-4]
            cwd = os.path.dirname(p)
            exp = open(p[:-4] + ".expected", "rb").read()
            res = []
            for cn, env in FULL:
                rc, so, se = run([LUA, p], env, cpu, cwd, 600)
                st = "ok" if (rc == 0 and so == exp) else f"DIFF({rc})"
                if st != "ok":
                    open(os.path.join(outdir, f"{name}_{cn}.txt"), "wb").write(so[-20000:] + b"\n---stderr---\n" + se[-3000:])
                res.append((cn, st))
            if SMALLM and name in small:
                sn = small[name]
                rc0, ref, _ = run([PUC, p, sn], {}, cpu, cwd, 120)
                for cn, env in SMALLM:
                    rc, so, se = run([LUA, p, sn], env, cpu, cwd, 1800)
                    st = "ok" if (rc == 0 and so == ref and rc0 == 0) else f"DIFF({rc})"
                    if st != "ok":
                        open(os.path.join(outdir, f"{name}_{cn}.txt"), "wb").write(so[-20000:] + b"\n---stderr---\n" + se[-3000:])
                    res.append((cn, st))
            with lock:
                for cn, st in res:
                    tally.setdefault(cn, [0, 0])[0 if st == "ok" else 1] += 1
                print(f"{name:26} " + " ".join(f"{cn}:{st}" for cn, st in res), flush=True)

    ths = [threading.Thread(target=work, args=(c,)) for c in cpus]
    for t in ths:
        t.start()
    for t in ths:
        t.join()
    print("TALLY (ok, bad):", tally)


if __name__ == "__main__":
    main()
