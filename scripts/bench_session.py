#!/usr/bin/env python3
"""Session full benchmark driver (2026-11-07): all sets incl. corpus, corpus2/{dev,heldout}; interleaved pinned runs.

  CPUS=3,4,5,6,7 RUNS=5 OUT=file.json SETS=corpus,corpus2/dev,... [ONLY=set/prog,...] \
  COLS='puc=/path/lua;lj=/usr/bin/luajit;nplain=env RUSTYNAIL_SEED=0 /path/rnlua;...' scripts/bench_session.py

One worker per CPU (taskset) takes a program and runs ALL columns RUNS times, column order rotated by the run index
(same method as docs/corpus2-study/tools/bench2.py and scripts/bench_mfull.py). Columns whose name starts with `lj` run the
LuaJIT variant of a program (bench/lj/<set>/<p>.lua, bench/luajit/<p>.lua for `programs`) and are skipped for programs in
bench/lj/SKIP. `env` columns get a RUSTYNAIL_PAD so that all `env` columns have the same environment length.
stdout of every run is compared with the `puc` column (exact / numeric / diff) and with <prog>.expected (exact).
JSON: {"cols":..., "results": {"set/prog": {col: {cyc:[...], ins:[...], out: same|numeq|diff, exp: ok|MISMATCH|none, err}}}}
"""
import glob, json, os, queue, subprocess, sys, tempfile, threading, time

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from bench_lj import numeq

TMP = os.path.expanduser("~/.cache/sb/tmp")


SPIN = float(os.environ.get("SPIN_MS", "0")) / 1000.0


def measure(cmd, cpu):
    if SPIN > 0:  # BUNDLE-Q: spin the core for SPIN_MS before the timed run (wakes it from a low-power state; the frequency is steady)
        subprocess.run(["taskset", "-c", str(cpu), "python3", "-c", f"import time\nt = time.time()\nwhile time.time() - t < {SPIN}: pass"], stdin=subprocess.DEVNULL)
    with tempfile.NamedTemporaryFile("r", suffix=".perf", dir=TMP) as f:
        full = ["perf", "stat", "-x,", "-o", f.name, "-e", "cycles:u,instructions:u", "--", "taskset", "-c", str(cpu)] + cmd
        try:
            p = subprocess.run(full, stdout=subprocess.PIPE, stderr=subprocess.PIPE, stdin=subprocess.DEVNULL, timeout=180)
        except subprocess.TimeoutExpired:
            return -9, b"", "timeout", {}
        res = {}
        for line in f.read().splitlines():
            fld = line.split(",")
            if len(fld) > 2 and fld[0].isdigit():
                res[fld[2].split(":")[0]] = int(fld[0])
        return p.returncode, p.stdout, p.stderr.decode(errors="replace")[-200:], res


def main():
    os.makedirs(TMP, exist_ok=True)
    cpus = [int(x) for x in os.environ["CPUS"].split(",")]
    runs = int(os.environ.get("RUNS", "5"))
    out = os.environ["OUT"]
    sets = [s for s in os.environ["SETS"].split(",") if s]
    only = set(x for x in os.environ.get("ONLY", "").split(",") if x)
    cols = []
    for item in os.environ["COLS"].split(";"):
        if item.strip():
            n, c = item.split("=", 1)
            cols.append((n.strip(), c.split()))
    envlen = lambda c: sum(len(x) + 1 for x in c[1:] if "=" in x) if c and c[0] == "env" else 0
    m = max([envlen(c) for _, c in cols] + [0])
    cols = [(n, (["env", "RUSTYNAIL_PAD=" + "x" * (m - envlen(c) + 8)] + c[1:]) if c and c[0] == "env" else c) for n, c in cols]
    skip = set()
    for line in open(os.path.join(ROOT, "bench/lj/SKIP")):
        line = line.split("#")[0].strip()
        if line:
            skip.add(line)

    def path_for(col, s, p):
        if col.startswith("lj"):
            base = s.split("/")[0]
            for c in (os.path.join(ROOT, "bench/lj", s, p + ".lua"),
                      os.path.join(ROOT, "bench/luajit", p + ".lua") if s == "programs" else ""):
                if c and os.path.exists(c):
                    return c
        return os.path.join(ROOT, "bench", s, p + ".lua")

    q = queue.Queue()
    for s in sets:
        for p in sorted(os.path.basename(x)[:-4] for x in glob.glob(os.path.join(ROOT, "bench", s, "*.lua"))):
            if only and f"{s}/{p}" not in only:
                continue
            q.put((s, p))
    total = q.qsize()
    results, lock, t0 = {}, threading.Lock(), time.time()

    def work(cpu):
        while True:
            try:
                s, p = q.get_nowait()
            except queue.Empty:
                return
            key = f"{s}/{p}"
            active = [(n, c) for n, c in cols if not (n.startswith("lj") and key in skip)]
            xp = os.path.join(ROOT, "bench", s, p + ".expected")
            exp = open(xp, "rb").read() if os.path.exists(xp) else None
            r = {n: {"cyc": [], "ins": [], "out": "", "exp": "none" if exp is None else "ok", "err": "",
                     "variant": path_for(n, s, p) != path_for("x", s, p)} for n, _ in active}
            for n, _ in cols:
                if (n, _) not in active:
                    r[n] = {"err": "skipped (no LuaJIT equivalent)"}
            outs = {}
            for i in range(runs):
                k = i % len(active)
                for n, c in active[k:] + active[:k]:
                    rc, so, se, res = measure(c + [path_for(n, s, p)], cpu)
                    e = r[n]
                    if rc != 0:
                        e["err"] = f"rc{rc}: {se.strip()[-120:]}"
                        continue
                    if n not in outs:
                        outs[n] = so
                    if exp is not None and so != exp and not n.startswith("lj"):
                        e["exp"] = "MISMATCH"
                    e["cyc"].append(res.get("cycles"))
                    e["ins"].append(res.get("instructions"))
            ref = outs.get("puc")
            for n, _ in active:
                o = outs.get(n)
                if o is None:
                    continue
                dec = lambda b: b.decode(errors="replace")
                r[n]["out"] = "same" if o == ref else ("numeq" if ref is not None and numeq(dec(o), dec(ref)) else "diff")
                if n.startswith("lj") and exp is not None:
                    r[n]["exp"] = "ok" if o == exp else "differs"
            with lock:
                results[key] = {"cpu": cpu, "cols": r}
                json.dump({"cols": {n: " ".join(c) for n, c in cols}, "runs": runs, "results": results}, open(out + ".part", "w"))
                print(f"[{time.time() - t0:6.0f}s {len(results)}/{total} cpu{cpu}] {key} " + " ".join(
                    f"{n}={min(v['cyc']) / 1e6:.0f}" if v.get("cyc") else f"{n}=-" for n, v in r.items()), flush=True)

    ths = [threading.Thread(target=work, args=(c,)) for c in cpus]
    [t.start() for t in ths]
    [t.join() for t in ths]
    json.dump({"cols": {n: " ".join(c) for n, c in cols}, "runs": runs, "results": results}, open(out, "w"), indent=1)
    os.remove(out + ".part")


if __name__ == "__main__":
    main()
