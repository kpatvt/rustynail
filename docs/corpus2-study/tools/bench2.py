#!/usr/bin/env python3
"""Parallel interleaved cycles:u benchmark for the corpus2 study.

  CPUS=3,4,5,6,7 RUNS=7 OUT=file.json COLS='name=cmd...;name2=cmd...' bench2.py prog.lua ...

A worker pinned to one CPU (taskset) takes a program and runs ALL columns of it RUNS times, the column order rotated by
the run index (interleaved). Ours columns: a command starting with `env` gets RUSTYNAIL_PAD so that every column's
environment additions have the same length. stdout of every run is compared with <prog>.expected (if present):
the result records ok / mismatch per column. Writes JSON {prog: {col: {cyc: [...], ins: [...], ok: bool, err: str}}}.
"""
import json, os, queue, subprocess, sys, tempfile, threading, time


def measure(cmd, cpu, cwd):
    with tempfile.NamedTemporaryFile("r", suffix=".perf", dir=os.path.expanduser("~/.cache/corpus2/tmp")) as f:
        full = ["perf", "stat", "-x,", "-o", f.name, "-e", "cycles:u,instructions:u", "--", "taskset", "-c", str(cpu)] + cmd
        p = subprocess.run(full, stdout=subprocess.PIPE, stderr=subprocess.PIPE, stdin=subprocess.DEVNULL, cwd=cwd, timeout=600)
        res = {}
        for line in f.read().splitlines():
            fld = line.split(",")
            if len(fld) > 2 and fld[0].isdigit():
                res[fld[2].split(":")[0]] = int(fld[0])
        return p.returncode, p.stdout, p.stderr.decode(errors="replace")[-300:], res


def main():
    cpus = [int(x) for x in os.environ.get("CPUS", "3,4,5,6,7").split(",")]
    runs = int(os.environ.get("RUNS", "7"))
    out = os.environ["OUT"]
    cols = []
    for item in os.environ["COLS"].split(";"):
        item = item.strip()
        if item:
            n, c = item.split("=", 1)
            cols.append((n, c.split()))
    # equalise environment additions of `env` columns
    def envlen(c):
        return sum(len(x) + 1 for x in c[1:] if "=" in x) if c and c[0] == "env" else 0
    m = max([envlen(c) for _, c in cols] + [0])
    fixed = []
    for n, c in cols:
        if c and c[0] == "env":
            pad = m - envlen(c) + 8
            c = ["env", "RUSTYNAIL_PAD=" + "x" * pad] + c[1:]
        fixed.append((n, c))
    cols = fixed
    progs = [os.path.abspath(p) for p in sys.argv[1:]]
    q = queue.Queue()
    for p in progs:
        q.put(p)
    results = {}
    lock = threading.Lock()
    t0 = time.time()

    def work(cpu):
        while True:
            try:
                p = q.get_nowait()
            except queue.Empty:
                return
            exp = None
            if os.path.exists(p[:-4] + ".expected"):
                exp = open(p[:-4] + ".expected", "rb").read()
            r = {n: {"cyc": [], "ins": [], "ok": True, "err": ""} for n, _ in cols}
            for i in range(runs):
                order = cols[i % len(cols):] + cols[:i % len(cols)]
                for n, c in order:
                    rc, so, se, res = measure(c + [p], cpu, os.path.dirname(p))
                    if rc != 0:
                        r[n]["ok"] = False
                        r[n]["err"] = f"rc{rc}: {se}"
                        continue
                    if exp is not None and so != exp:
                        r[n]["ok"] = False
                        r[n]["err"] = "output mismatch"
                    r[n]["cyc"].append(res.get("cycles"))
                    r[n]["ins"].append(res.get("instructions"))
            key = os.path.basename(os.path.dirname(p)) + "/" + os.path.basename(p)[:-4]
            with lock:
                results[key] = r
                json.dump(results, open(out + ".part", "w"))
                print(f"[{time.time() - t0:7.0f}s cpu{cpu}] {key} " +
                      " ".join(f"{n}={min(v['cyc']) / 1e6 if v['cyc'] else -1:.0f}{'' if v['ok'] else '!'}" for n, v in r.items()), flush=True)

    ths = [threading.Thread(target=work, args=(c,)) for c in cpus]
    for t in ths:
        t.start()
    for t in ths:
        t.join()
    json.dump({"cols": {n: " ".join(c) for n, c in cols}, "runs": runs, "results": results}, open(out, "w"), indent=1)
    os.remove(out + ".part")


if __name__ == "__main__":
    main()
