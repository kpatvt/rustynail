#!/usr/bin/env python3
"""BUNDLE-A item 1 summary of docs/bench-ba-<set>-gc.json (scripts/bench-cycles.sh columns base, dup, new, new2)."""
import json, math, os

root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))


def gm(x):
    return math.exp(sum(math.log(v) for v in x) / len(x))


for s in ["awfy", "apps", "programs", "jit-trace", "lib"]:
    d = json.load(open(os.path.join(root, f"docs/bench-ba-{s}-gc.json")))
    progs = d.get("programs", d)
    r = {p: (row["new"]["cycles_min"] / row["base"]["cycles_min"], row["new"]["cycles_median"] / row["base"]["cycles_median"],
             row["new"]["instructions_min"] / row["base"]["instructions_min"], row["dup"]["cycles_min"] / row["base"]["cycles_min"])
         for p, row in progs.items()}
    srt = sorted(r.items(), key=lambda kv: kv[1][0])
    print("%-10s n=%-3d new/base cyc min %.3f med %.3f instr %.3f dup %.3f" % (
        s, len(r), gm([v[0] for v in r.values()]), gm([v[1] for v in r.values()]), gm([v[2] for v in r.values()]), gm([v[3] for v in r.values()])))
    print("   best : " + ", ".join("%s %.3f" % (p, v[0]) for p, v in srt[:6]))
    print("   worst: " + ", ".join("%s %.3f" % (p, v[0]) for p, v in srt[-4:]))
