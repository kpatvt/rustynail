#!/usr/bin/env python3
"""BUNDLE-L report: per-item target geomeans and set geomeans from scripts/bench_cycles.py JSON files.

  scripts/bench_l_report.py <tag> [--dir docs] [--on on] [--base base] [--dup dup] [--items D1,D2,D3]

Reads <dir>/bench-l-<tag>-<set>.json for set in awfy, apps, real, lib, jit-trace (whichever exist; the output of
`bench-cycles.sh base=... on=... dup=...`: one binary, `RUSTYNAIL_JIT_TRACE_LIMITS` strings of equal length switch the item on / off, dup = the
base column again). Prints for each item the geomean of the cycles (min, median) and instruction ratios over its target programs, the worst
program, and the set geomeans of on/base and dup/base, then every program worse than 3%.
"""
import json, math, os, sys

TARGETS = {
    # D1: lru_cache + the programs whose recorder logs showed the blockers (IntMiss / TableHash / Setupval) in the effectiveness study
    "D1": ["apps/lru_cache", "awfy/cd", "apps/expr_lang", "awfy/havlak"],
    # D2: events + the programs that use `...`
    "D2": ["apps/events"],
    # D3: sinking
    "D3": ["apps/raytrace", "awfy/cd", "awfy/json", "apps/expr_lang", "awfy/richards", "awfy/deltablue"],
}


def geo(xs):
    xs = [x for x in xs if x and x > 0]
    return math.exp(sum(math.log(x) for x in xs) / len(xs)) if xs else float("nan")


def load(d, tag):
    out = {}
    for s in ("awfy", "apps", "real", "lib", "jit-trace", "programs"):
        p = os.path.join(d, f"bench-l-{tag}-{s}.json")
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


def main():
    if len(sys.argv) < 2:
        print(__doc__)
        return
    tag = sys.argv[1]
    arg = lambda k, dflt: sys.argv[sys.argv.index(k) + 1] if k in sys.argv else dflt
    d, on, base, dup = arg("--dir", "docs"), arg("--on", "on"), arg("--base", "base"), arg("--dup", "dup")
    items = arg("--items", "D1,D2,D3").split(",")
    data = load(d, tag)
    print(f"== {tag}: {len(data)} programs")
    for item in items:
        print(f"-- item {item} targets ({on}/{base})")
        line(f"item {item}", data, TARGETS[item], on, base)
        for p in TARGETS[item]:
            if p in data:
                line("  " + p, data, [p], on, base)
    print(f"-- sets, {on}/{base} (and {dup}/{base})")
    for s in ("awfy", "apps", "real", "lib", "jit-trace", "programs"):
        ps = [p for p in data if p.startswith(s + "/")]
        if ps:
            line(s, data, ps, on, base)
            line(s + " dup", data, ps, dup, base)
    line("all", data, list(data), on, base)
    line("all dup", data, list(data), dup, base)
    print(f"-- programs worse than 3% ({on}/{base} cycles min)")
    r = ratio(data, list(data), on, base, "cycles_min")
    ri = ratio(data, list(data), on, base, "instructions_min")
    for p, v in sorted(r.items(), key=lambda kv: -kv[1]):
        if v > 1.03:
            print(f"  {p:28s} cycles {v:.3f}  instr {ri.get(p, float('nan')):.3f}")


if __name__ == "__main__":
    main()
