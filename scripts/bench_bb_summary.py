#!/usr/bin/env python3
"""BUNDLE-B summary of docs/bench-bb-<set>.json (scripts/bench-cycles.sh columns base, new, base2, new2, no3, no2).

new = this tree (items 2 and 3), base = the BUNDLE-A build, base2/new2 = byte copies (noise columns), no3 = new with maxside=64 (item 2 only),
no2 = new with tset=0 (item 3 only). Prints the geomean of the per-program cycle ratio against `base` (min / median) and of the min instructions,
and the best and worst programs.
"""
import json, math, os, sys

root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))


def gm(x):
    return math.exp(sum(math.log(v) for v in x) / len(x))


def load(name):
    d = json.load(open(os.path.join(root, f"docs/bench-bb-{name}.json")))
    return d.get("programs", d)


for s in sys.argv[1:] or ["awfy", "apps", "programs", "jit-trace", "lib", "real", "awfy-default", "apps-default"]:
    try:
        progs = load(s)
    except FileNotFoundError:
        continue
    cols = [c for c in ["new", "base2", "new2", "no3", "no2"] if all(c in row for row in progs.values())]
    for c in cols:
        r = {p: (row[c]["cycles_min"] / row["base"]["cycles_min"], row[c]["cycles_median"] / row["base"]["cycles_median"],
                 row[c]["instructions_min"] / row["base"]["instructions_min"]) for p, row in progs.items()}
        srt = sorted(r.items(), key=lambda kv: kv[1][0])
        print("%-13s %-5s n=%-3d cyc min %.3f med %.3f instr %.3f" % (s, c, len(r), gm([v[0] for v in r.values()]),
                                                                    gm([v[1] for v in r.values()]), gm([v[2] for v in r.values()])))
        if c in ("new", "no3", "no2"):
            print("      best : " + ", ".join("%s %.3f" % (p, v[0]) for p, v in srt[:5]))
            print("      worst: " + ", ".join("%s %.3f (instr %.3f)" % (p, v[0], v[2]) for p, v in srt[-4:]))
