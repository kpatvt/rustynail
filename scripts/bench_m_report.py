#!/usr/bin/env python3
"""BUNDLE-M report: per-item target geomeans and set geomeans from scripts/bench_cycles.py JSON files, and the compile share.

  scripts/bench_m_report.py <tag> [--dir docs] [--base base] [--dup dup] [--cols c1,c2,c4,all] [--items C1,C2,C4]
  scripts/bench_m_report.py phases <tag> --bin <rnlua> --a "<limits A>" --b "<limits B>" [--dir docs] [--runs 3]
                                  (compile share before / after per program, written to <dir>/bench-m-<tag>-phases.txt)

Reads <dir>/bench-m-<tag>-<set>.json for set in awfy, apps, real, lib, jit-trace (whichever exist; the output of
`bench-cycles.sh base=... c1=... c2=... c4=... all=... dup=...`: one binary, `RUSTYNAIL_JIT_TRACE_LIMITS` strings of equal length switch the
items on / off, dup = the base column again). For each item column: the geomean of the cycles (min, median) and instruction ratios over
the item's target programs, the worst program, then the set geomeans of every column / base and dup / base, and every program that
moved more than 3%.
"""
import json, math, os, re, statistics, subprocess, sys

TARGETS = {
    # C1 (the task's named programs): game_loop, deltablue, cd, raytrace, havlak, json, expr_lang
    "C1n": ["real/game_loop", "awfy/deltablue", "awfy/cd", "apps/raytrace", "awfy/havlak", "awfy/json", "apps/expr_lang"],
    # C1: the named ones plus the study's compile-heavy set (compile >= 9% of the run: richards, towers, queens, bounce)
    "C1": ["real/game_loop", "awfy/deltablue", "awfy/cd", "apps/raytrace", "awfy/havlak", "awfy/json", "apps/expr_lang", "awfy/richards",
           "awfy/towers", "awfy/queens", "awfy/bounce"],
    # C2: programs with many FnShort probes (the study's list: deltablue, havlak, json, towers; plus the others whose runs show >= 40 probes)
    "C2": ["awfy/deltablue", "awfy/havlak", "awfy/json", "awfy/towers", "awfy/richards", "awfy/cd", "apps/raytrace", "apps/expr_lang",
           "apps/events", "apps/markdown", "real/json_roundtrip", "real/template_engine", "apps/lru_cache"],
    # C4: raytrace and the programs with unchangeable aborts
    "C4": ["apps/raytrace", "awfy/cd", "awfy/json", "apps/expr_lang", "real/lua_interp", "awfy/deltablue", "awfy/havlak"],
}
SETS = ("awfy", "apps", "real", "lib", "jit-trace", "programs")


def geo(xs):
    xs = [x for x in xs if x and x > 0]
    return math.exp(sum(math.log(x) for x in xs) / len(xs)) if xs else float("nan")


def load(d, tag):
    out = {}
    for s in SETS:
        p = os.path.join(d, f"bench-m-{tag}-{s}.json")
        if os.path.exists(p):
            for prog, v in json.load(open(p))["programs"].items():
                out[f"{s}/{prog}"] = v
    return out


def ratio(data, progs, num, den, key):
    r = {}
    for p in progs:
        v = data.get(p)
        if v and num in v and den in v and v[num].get(key) and v[den].get(key):
            r[p] = v[num][key] / v[den][key]
    return r


def line(name, data, progs, num, den):
    cm, cd, im = (ratio(data, progs, num, den, k) for k in ("cycles_min", "cycles_median", "instructions_min"))
    worst = max(cm.items(), key=lambda kv: kv[1], default=("-", float("nan")))
    print(f"{name:34s} n={len(cm):3d}  cycles min {geo(cm.values()):.3f}  median {geo(cd.values()):.3f}  instr {geo(im.values()):.3f}  worst {worst[0]} {worst[1]:.3f}")


def phases(argv):
    arg = lambda k, dflt: argv[argv.index(k) + 1] if k in argv else dflt
    tag, binp, la, lb = argv[0], arg("--bin", ""), arg("--a", ""), arg("--b", "")
    d, runs = arg("--dir", "docs"), int(arg("--runs", "3"))
    root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    data = load(d, tag)

    def one(limits, prog):
        env = dict(os.environ, RUSTYNAIL_SEED="0", RUSTYNAIL_JIT_VERBOSE="1", RUSTYNAIL_JIT_TRACE_LIMITS=limits)
        cmd = ["systemd-run", "--user", "--scope", "-q", "-p", "MemoryMax=8G", "-p", "MemorySwapMax=0", "timeout", "300", "taskset", "-c", "4", binp, prog]
        vals = []
        for _ in range(runs):
            p = subprocess.run(cmd, env=env, stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)
            m = re.search(rb"rec_step ([0-9.]+)/\d+ .*? pipeline ([0-9.]+)/", p.stderr)
            if m:
                vals.append(float(m.group(1)) + float(m.group(2)))
        return statistics.median(vals) if vals else float("nan")

    rows = []
    for key in sorted(data):
        s, prog = key.split("/")
        path = os.path.join(root, "bench", s, prog + ".lua")
        if not os.path.exists(path):
            continue
        a, b = one(la, path), one(lb, path)
        cyc = data[key].get("base", {}).get("cycles_min", 0) / 2.5e6  # Mtsc (K = 2.5 cycles / tick)
        rows.append((key, a, b, cyc))
    out = [f"compile Mtsc (rec_step + pipeline, median of {runs}), A = {la}, B = {lb}; share = compile / run (run = min cycles of the base column / 2.5)",
           f"{'program':24s} {'A Mtsc':>8s} {'B Mtsc':>8s} {'B/A':>6s} {'share A':>8s} {'share B':>8s}"]
    for key, a, b, cyc in rows:
        out.append(f"{key:24s} {a:8.2f} {b:8.2f} {b / a if a else float('nan'):6.3f} {100 * a / cyc if cyc else float('nan'):7.1f}% {100 * b / cyc if cyc else float('nan'):7.1f}%")
    sa, sb = sum(r[1] for r in rows), sum(r[2] for r in rows)
    out.append(f"{'sum':24s} {sa:8.2f} {sb:8.2f} {sb / sa if sa else float('nan'):6.3f}")
    text = "\n".join(out)
    print(text)
    open(os.path.join(d, f"bench-m-{tag}-phases.txt"), "w").write(text + "\n")


def main():
    if len(sys.argv) < 2:
        print(__doc__)
        return
    if sys.argv[1] == "phases":
        return phases(sys.argv[2:])
    tag = sys.argv[1]
    arg = lambda k, dflt: sys.argv[sys.argv.index(k) + 1] if k in sys.argv else dflt
    d, base, dup = arg("--dir", "docs"), arg("--base", "base"), arg("--dup", "dup")
    # column -> item (the target list the column is judged on)
    cmap = dict(kv.split("=") for kv in arg("--map", "c1=C1,c2a=C2,c2b=C2,c2c=C2,c4a=C4,c4b=C4,all=C1").split(","))
    data = load(d, tag)
    cols = [c for c in arg("--cols", ",".join(cmap)).split(",") if any(c in v for v in data.values())]
    print(f"== {tag}: {len(data)} programs")
    for col in cols:
        item = cmap.get(col)
        if item is None or item not in TARGETS:
            continue
        print(f"-- column {col} on the {item} targets ({col}/{base})")
        line(f"{col} {item}", data, TARGETS[item], col, base)
        if item == "C1":
            line(f"{col} C1n (the seven named)", data, TARGETS["C1n"], col, base)
        for p in TARGETS[item]:
            if p in data:
                line("  " + p, data, [p], col, base)
    print(f"-- sets, every column / {base} (and {dup}/{base})")
    for s in SETS:
        ps = [p for p in data if p.startswith(s + "/")]
        if ps:
            for c in cols + [dup]:
                line(f"{s} {c}", data, ps, c, base)
    for c in cols + [dup]:
        line(f"all {c}", data, list(data), c, base)
    for c in cols:
        print(f"-- programs moved more than 3% ({c}/{base} cycles min; instructions alongside)")
        r = ratio(data, list(data), c, base, "cycles_min")
        ri = ratio(data, list(data), c, base, "instructions_min")
        for p, v in sorted(r.items(), key=lambda kv: -kv[1]):
            if v > 1.03 or v < 0.97:
                print(f"  {p:28s} cycles {v:.3f}  instr {ri.get(p, float('nan')):.3f}")


if __name__ == "__main__":
    main()
