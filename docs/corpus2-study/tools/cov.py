#!/usr/bin/env python3
"""Cycle coverage (trace / tier-1 / interp / helpers+exit / compile / other) per program, using the repo's
scripts/bp_coverage.py classifier. usage: CPUS=3,4,5 [XLIMITS=..] cov.py OUTDIR prog.lua ...  -> OUTDIR/<name>.on.rep, table on stdout"""
import os, queue, subprocess, sys, threading
sys.path.insert(0, os.path.expanduser("~/.cache/corpus2/scripts"))
import bp_coverage as bc

LUA = os.environ.get("LUA", os.path.expanduser("~/.cache/corpus2/target/release/rn-cli"))


def rec(prog, outdir, cpu):
    name = os.path.basename(os.path.dirname(prog)) + "__" + os.path.basename(prog)[:-4]
    data = os.path.join(outdir, name + ".data")
    env = {k: v for k, v in os.environ.items() if not k.startswith("RUSTYNAIL_")}
    env.update(RUSTYNAIL_SEED="0", RUSTYNAIL_JIT_PERFMAP="1")
    if os.environ.get("XLIMITS"):
        env["RUSTYNAIL_JIT_TRACE_LIMITS"] = os.environ["XLIMITS"]
    subprocess.run(["perf", "record", "-q", "-m", "32", "-o", data, "-e", "cycles:u", "-c", "150001", "taskset", "-c", str(cpu), LUA, prog],
                   env=env, stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=900,
                   cwd=os.path.dirname(prog))
    rep = subprocess.run(["perf", "report", "-i", data, "--no-children", "--stdio", "--sort", "dso,sym", "-g", "none",
                          "--percent-limit", "0.02"], stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, stdin=subprocess.DEVNULL)
    open(os.path.join(outdir, name + ".on.rep"), "wb").write(rep.stdout)
    os.remove(data)
    return name


def main():
    outdir = os.path.abspath(sys.argv[1])
    os.makedirs(outdir, exist_ok=True)
    cpus = [int(x) for x in os.environ.get("CPUS", "3,4,5").split(",")]
    q = queue.Queue()
    for p in sys.argv[2:]:
        q.put(os.path.abspath(p))
    names = []
    lock = threading.Lock()

    def work(cpu):
        while True:
            try:
                p = q.get_nowait()
            except queue.Empty:
                return
            n = rec(p, outdir, cpu)
            with lock:
                names.append(n)

    ths = [threading.Thread(target=work, args=(c,)) for c in cpus]
    [t.start() for t in ths]
    [t.join() for t in ths]
    ns = bc.load_classifier()
    print("| program | trace | tier-1 | interp | helpers+exit | compile | other | Mcyc |\n|---|---|---|---|---|---|---|---|")
    for n in sorted(names):
        sh, m = bc.shares(ns, os.path.join(outdir, n + ".on.rep"))
        print(f"| {n} | " + " | ".join(f"{x:.0f}" for x in sh) + f" | {m:.0f} |")


if __name__ == "__main__":
    main()
