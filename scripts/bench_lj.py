#!/usr/bin/env python3
"""LuaJIT comparison driver (task P-LJ). See docs/JIT_TRACE_REF.md "LuaJIT comparison".

  SET=programs|real|lib|jit-trace  OUT=docs/bench-lj-<set>.json RUNS=7 CPU=4 \
  scripts/bench-lj.sh ref="ref/55/build-seed0/lua" ours="env RUSTYNAIL_SEED=0 .../rnlua" lj=/usr/bin/luajit

Binary names with special meaning: `lj*` binaries run the LuaJIT variant of a program (bench/lj/<SET>/<p>.lua, else
bench/luajit/<p>.lua for SET=programs, else the original), every other binary runs the original program.
A program is skipped for LuaJIT if it is listed in bench/lj/SKIP (one `<set>/<prog>` per line, `#` comments).
Per (program, binary): min/median of cycles:u and instructions:u over RUNS interleaved pinned runs; the first binary
is the baseline. The stdout of every binary is compared with the first binary's (exact, then numerically with 1e-9
tolerance: LuaJIT prints %.14g and has no integer subtype), recorded as "same" / "numeq" / "diff".
H1: if <prog>.expected exists next to the original program (bench/lib, bench/awfy, bench/apps have them) every binary's
stdout is also compared with it exactly and recorded as "expected": "ok" / "MISMATCH" (the H1 sets are written so that
all three engines print the same text). SET=awfy|apps|lib|lib_const work like the older sets.
"""
import glob, json, os, statistics, subprocess, sys
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from bench_cycles import measure, geomean, ROOT


def numeq(a, b):
    ta, tb = a.split(), b.split()
    if len(ta) != len(tb):
        return False
    for x, y in zip(ta, tb):
        if x == y:
            continue
        try:
            fx, fy = float(x), float(y)
        except ValueError:
            return False
        if abs(fx - fy) > 1e-9 * max(abs(fx), abs(fy), 1):
            return False
    return True


def main():
    cpu = int(os.environ.get("CPU", "4"))
    runs = int(os.environ.get("RUNS", "7"))
    sset = os.environ.get("SET", "programs")
    only = [x for x in os.environ.get("ONLY", "").split(",") if x]
    out_json = os.environ.get("OUT", "")
    bins = [(b.split("=", 1)[0], b.split("=", 1)[1].split()) for b in sys.argv[1:]]
    names = [n for n, _ in bins]
    base = names[0]
    skip = set()
    sk = os.path.join(ROOT, "bench/lj/SKIP")
    if os.path.exists(sk):
        for line in open(sk):
            line = line.split("#")[0].strip()
            if line:
                skip.add(line)
    d = os.path.join(ROOT, "bench", sset)
    progs = sorted(os.path.basename(p)[:-4] for p in glob.glob(os.path.join(d, "*.lua")))
    if only:
        progs = [p for p in progs if p in only]

    def path_for(bn, p):
        if bn.startswith("lj"):
            for c in (os.path.join(ROOT, "bench/lj", sset, p + ".lua"),
                      os.path.join(ROOT, "bench/luajit", p + ".lua") if sset == "programs" else ""):
                if c and os.path.exists(c):
                    return c, True
        return os.path.join(d, p + ".lua"), False

    samples, outs, notes, variant, compat = {}, {}, {}, {}, {}
    active = {}
    for p in progs:
        active[p] = [n for n in names if not (n.startswith("lj") and f"{sset}/{p}" in skip)]
        for n, cmd in bins:
            if n not in active[p]:
                continue
            path, isvar = path_for(n, p)
            variant[(p, n)] = isvar
            o, r = measure(cmd + [path], cpu)
            if o is None:
                notes[(p, n)] = "FAILED: " + str(r).strip().replace("\n", " ")
                active[p].remove(n)
                continue
            outs[(p, n)] = o.decode(errors="replace")
            samples[(p, n)] = []
    for run in range(runs):
        print(f"run {run + 1}/{runs}", file=sys.stderr, flush=True)
        for p in progs:
            for n, cmd in bins:
                if n in active[p]:
                    o, r = measure(cmd + [path_for(n, p)[0]], cpu)
                    if o is not None:
                        samples[(p, n)].append(r)
    res = {"cpu": cpu, "runs": runs, "set": sset, "bins": {n: " ".join(c) for n, c in bins}, "programs": {}}
    for p in progs:
        row = {}
        b0 = outs.get((p, base))
        for n in names:
            s = samples.get((p, n))
            if not s:
                row[n] = {"note": notes.get((p, n), "skipped")}
                continue
            e = {"variant": variant[(p, n)]}
            for ev in ("cycles", "instructions"):
                v = [x[ev] for x in s if ev in x]
                e[ev + "_min"], e[ev + "_median"] = min(v), int(statistics.median(v))
            e["runs"] = len(s)
            o = outs[(p, n)]
            e["output"] = "same" if o == b0 else ("numeq" if numeq(o, b0 or "") else "diff")
            xp = os.path.join(d, p + ".expected")
            if os.path.exists(xp):
                e["expected"] = "ok" if o == open(xp, errors="replace").read() else "MISMATCH"
            row[n] = e
        res["programs"][p] = row
    if out_json:
        json.dump(res, open(out_json, "w"), indent=1)
    print(f"{'program':<16}" + "".join(f"{n + ' Mcyc':>14}" for n in names))
    for p, row in res["programs"].items():
        print(f"{p:<16}" + "".join(
            f"{row[n]['cycles_min'] / 1e6:>14.1f}" if "cycles_min" in row[n] else f"{'-':>14}" for n in names))


if __name__ == "__main__":
    main()
