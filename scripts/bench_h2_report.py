#!/usr/bin/env python3
"""BUNDLE-H2: ours/LuaJIT (min and median cycles) per program and set geomeans from docs/bench-g2-lj-<set>.json."""
import json, math, sys
def gm(x):
    return math.exp(sum(math.log(v) for v in x) / len(x))
for s in sys.argv[1:]:
    j = json.load(open(f"docs/bench-h2-lj-{s}.json"))["programs"]
    print(f"== {s}")
    cols = ("plain", "pgo", "bplain", "bpgo")
    acc = {c: ([], []) for c in cols}
    for n, v in j.items():
        lj = v["lj"]
        row = [n, "LJ %.1f/%.1f" % (lj["cycles_min"] / 1e6, lj["cycles_median"] / 1e6)]
        for c in cols:
            a = v[c]["cycles_min"] / lj["cycles_min"]
            b = v[c]["cycles_median"] / lj["cycles_median"]
            acc[c][0].append(a); acc[c][1].append(b)
            row.append("%s %.2f/%.2f" % (c, a, b))
        d = v["pgo2"]["cycles_min"] / v["pgo"]["cycles_min"]
        row.append("dup %.3f" % d)
        print("  ".join(row))
    for c in cols:
        print(f"geomean {c}: ours/LJ min {gm(acc[c][0]):.3f} median {gm(acc[c][1]):.3f}")
