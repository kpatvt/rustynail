#!/usr/bin/env python3
"""BUNDLE-K report: per-item target geomeans and set geomeans from scripts/bench_cycles.py JSON files.

  scripts/bench_k_report.py <tag> [--dir docs] [--cols base,k,konlyA,dup]

Reads <dir>/bench-k-<tag>-<set>.json for set in awfy, apps, real, lib (the output of
`bench-cycles.sh base=... k=... konlyA=... dup=...`, columns: base = main, k = BUNDLE-K, konlyA = BUNDLE-K with
RUSTYNAIL_JIT_TRACE_LIMITS=strself=0 (item A, and C if built, only), dup = byte copy of base).

Prints, for each item, the geomean of the cycles (min, median) and instruction ratios over its target programs, the worst program, and the
set geomeans (awfy, apps, real, lib, all) of k/base and of the duplicate column.
  item A (pattern captures not zeroed)   = konlyA / base
  item B (inline SELF on strings)        = k / konlyA
  total                                  = k / base
"""
import json, math, os, sys

TARGETS = {
    "A": ["real/json_roundtrip", "real/template_engine", "real/wordfreq", "apps/markdown",
          "lib/find_cap", "lib/find_pat", "lib/find_plain", "lib/gmatch_words", "lib/gsub_fn", "lib/gsub_str", "lib/gsub_tbl", "lib/match"],
    "B": ["real/json_roundtrip", "real/template_engine", "real/wordfreq", "real/lua_interp", "apps/markdown",
          "lib/str_method_len", "lib/sub", "lib/lower", "lib/upper", "lib/sub_1char"],
    "C": ["real/wordfreq", "real/json_roundtrip", "real/template_engine", "apps/markdown"],
}


def geo(xs):
    xs = [x for x in xs if x and x > 0]
    return math.exp(sum(math.log(x) for x in xs) / len(xs)) if xs else float("nan")


def load(d, tag):
    out = {}
    for s in ("awfy", "apps", "real", "lib"):
        p = os.path.join(d, f"bench-k-{tag}-{s}.json")
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
    d = "docs"
    if "--dir" in sys.argv:
        d = sys.argv[sys.argv.index("--dir") + 1]
    data = load(d, tag)
    print(f"== {tag}: {len(data)} programs")
    for item, num, den in (("A", "konlyA", "base"), ("B", "k", "konlyA"), ("C", "k", "base")):
        print(f"-- item {item} targets ({num}/{den})")
        line(f"item {item}", data, TARGETS[item], num, den)
        for p in TARGETS[item]:
            if p in data:
                line("  " + p, data, [p], num, den)
    print("-- sets, k/base (and dup/base)")
    for s in ("awfy", "apps", "real", "lib"):
        ps = [p for p in data if p.startswith(s + "/")]
        line(s, data, ps, "k", "base")
        line(s + " dup", data, ps, "dup", "base")
    line("all", data, list(data), "k", "base")
    line("all dup", data, list(data), "dup", "base")
    print("-- programs worse than 3% (k/base cycles min)")
    r = ratio(data, list(data), "k", "base", "cycles_min")
    ri = ratio(data, list(data), "k", "base", "instructions_min")
    for p, v in sorted(r.items(), key=lambda kv: -kv[1]):
        if v > 1.03:
            print(f"  {p:28s} cycles {v:.3f}  instr {ri.get(p, float('nan')):.3f}")


if __name__ == "__main__":
    main()
