#!/usr/bin/env python3
"""Post-M full benchmark, parallel runner (queue of PROGRAMS over N pinned workers).

Like scripts/bench_lj.py (same JSON schema per set: docs/bench-mfull-<mode>-<set>.json), but the unit of work is one program:
a worker pinned to one CPU (taskset) takes a program from the queue and runs ALL columns of it, interleaved, RUNS times
(the column order is rotated by the run index), so every column of a program sees the same core and the same neighbours.

  CPUS=4,5,6,7 RUNS=11 MODE=default|warm0 OUTPFX=docs/bench-mfull-default [SETS=awfy,apps,...] [ONLY=set/prog,...] \
  [COLS=puc,lj,plain,pgo,pgo2,bplain,bpgo] [TAG=...] scripts/bench-mfull.sh

Columns are defined by the environment (absolute paths, equal length): PUC, LUAJIT, B_PLAIN, B_PGO, B_PGO2, B_BPLAIN, B_BPGO.
Ours columns run as `env RUSTYNAIL_SEED=0 [RUSTYNAIL_JIT_TRACE_LIMITS=warm=0 in MODE=warm0] <bin> <abs script path>`.
Column names in the JSON: ref (PUC), lj, plain, pgo, pgo2, bplain, bpgo (bplain / bpgo = the cde96490 binaries).
"""
import glob, json, os, statistics, sys, threading, queue, time
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from bench_cycles import measure, ROOT
from bench_lj import numeq

ALL_SETS = ["awfy", "apps", "programs", "jit-trace", "lib", "real"]  # BUNDLE-N: SETS=corpus adds the 18 third-party-style programs (bench/corpus)


def main():
    cpus = [int(x) for x in os.environ.get("CPUS", "4").split(",")]
    runs = int(os.environ.get("RUNS", "11"))
    mode = os.environ.get("MODE", "default")
    outpfx = os.environ["OUTPFX"]
    sets = [s for s in os.environ.get("SETS", ",".join(ALL_SETS)).split(",") if s]
    only = set(x for x in os.environ.get("ONLY", "").split(",") if x)
    env = os.environ
    prefix = ["env", "RUSTYNAIL_SEED=0"] + (["RUSTYNAIL_JIT_TRACE_LIMITS=warm=0"] if mode == "warm0" else [])
    defs = [("ref", [env["PUC"]]), ("lj", [env["LUAJIT"]]),
            ("plain", prefix + [env["B_PLAIN"]]), ("pgo", prefix + [env["B_PGO"]]), ("pgo2", prefix + [env["B_PGO2"]]),
            ("bplain", prefix + [env["B_BPLAIN"]]), ("bpgo", prefix + [env["B_BPGO"]])]
    want = os.environ.get("COLS", "ref,lj,plain,pgo,pgo2,bplain,bpgo" if mode == "default" else "plain,pgo,pgo2,bplain,bpgo").split(",")
    bins = [(n, c) for n, c in defs if n in want]
    names = [n for n, _ in bins]
    skip = set()
    for line in open(os.path.join(ROOT, "bench/lj/SKIP")):
        line = line.split("#")[0].strip()
        if line:
            skip.add(line)

    def path_for(sset, bn, p):
        d = os.path.join(ROOT, "bench", sset)
        if bn == "lj":
            for c in (os.path.join(ROOT, "bench/lj", sset, p + ".lua"),
                      os.path.join(ROOT, "bench/luajit", p + ".lua") if sset == "programs" else ""):
                if c and os.path.exists(c):
                    return c, True
        return os.path.join(d, p + ".lua"), False

    tasks = []
    for s in sets:
        d = os.path.join(ROOT, "bench", s)
        for p in sorted(os.path.basename(x)[:-4] for x in glob.glob(os.path.join(d, "*.lua"))):
            if only and f"{s}/{p}" not in only:
                continue
            tasks.append((s, p))
    q = queue.Queue()
    for t in tasks:
        q.put(t)
    results = {}
    lock = threading.Lock()
    done = [0]

    def work(cpu):
        while True:
            try:
                sset, p = q.get_nowait()
            except queue.Empty:
                return
            active = [n for n in names if not (n == "lj" and f"{sset}/{p}" in skip)]
            samples, outs, notes, variant = {}, {}, {}, {}
            for n, cmd in bins:
                if n not in active:
                    continue
                path, isvar = path_for(sset, n, p)
                variant[n] = isvar
                o, r = measure(cmd + [path], cpu)
                if o is None:
                    notes[n] = "FAILED: " + str(r).strip().replace("\n", " ")
                    active.remove(n)
                    continue
                outs[n] = o.decode(errors="replace")
                samples[n] = []
            for run in range(runs):
                k = run % max(1, len(active))
                for n in active[k:] + active[:k]:
                    cmd = dict(bins)[n]
                    o, r = measure(cmd + [path_for(sset, n, p)[0]], cpu)
                    if o is not None:
                        samples[n].append(r)
            row = {}
            base = names[0]
            b0 = outs.get(base)
            for n in names:
                s = samples.get(n)
                if not s:
                    row[n] = {"note": notes.get(n, "skipped")}
                    continue
                e = {"variant": variant[n]}
                for ev in ("cycles", "instructions"):
                    v = [x[ev] for x in s if ev in x]
                    e[ev + "_min"], e[ev + "_median"] = min(v), int(statistics.median(v))
                e["runs"] = len(s)
                o = outs[n]
                e["output"] = "same" if o == b0 else ("numeq" if numeq(o, b0 or "") else "diff")
                xp = os.path.join(ROOT, "bench", sset, p + ".expected")
                if os.path.exists(xp):
                    e["expected"] = "ok" if o == open(xp, errors="replace").read() else "MISMATCH"
                row[n] = e
            with lock:
                results[(sset, p)] = row
                done[0] += 1
                print(f"[{time.strftime('%H:%M:%S')}] {done[0]}/{len(tasks)} {sset}/{p} (cpu {cpu})", file=sys.stderr, flush=True)

    ths = [threading.Thread(target=work, args=(c,)) for c in cpus]
    for t in ths:
        t.start()
    for t in ths:
        t.join()
    for s in sets:
        progs = {p: results[(ss, p)] for (ss, p) in sorted(results) if ss == s}
        if not progs:
            continue
        res = {"cpu": cpus, "runs": runs, "set": s, "mode": mode, "bins": {n: " ".join(c) for n, c in bins}, "programs": progs}
        json.dump(res, open(f"{outpfx}-{s}.json", "w"), indent=1)
        with open(f"{outpfx}-{s}.txt", "w") as f:
            f.write(f"{'program':<16}" + "".join(f"{n + ' Mcyc':>14}" for n in names) + "\n")
            for p, row in progs.items():
                f.write(f"{p:<16}" + "".join(
                    f"{row[n]['cycles_min'] / 1e6:>14.1f}" if "cycles_min" in row[n] else f"{'-':>14}" for n in names) + "\n")


if __name__ == "__main__":
    main()
