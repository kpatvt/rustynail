#!/usr/bin/env python3
"""BUNDLE-J2: writes docs/bench-j2-tables.md from docs/bench-j2-<tag>-<set>.json (scripts/bench_j2_report.py reads the same files).

Tags: plain-warm0, pgo-warm0 (all five sets), plain-default, pgo-default (awfy, apps, real, and jit-trace / programs for the targets).
Columns of every file: base (deep=0), i2 (deep=8), dup (= base). (K1: the `recdead=1` column i1 and the combined column `all` were dropped with the knob.) Ratios are COL / base, cycles:u and
instructions:u, min and median of RUNS 11 (instructions: min). usage: scripts/bench_j2_tables.py > docs/bench-j2-tables.md"""
import json
import math
import os

root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SETS = ["awfy", "apps", "real", "jit-trace", "programs"]
I2 = ["rec_loop", "lru_cache", "json", "oop_override"]


def gm(x):
    x = [v for v in x if v and v > 0]
    return math.exp(sum(math.log(v) for v in x) / len(x)) if x else float("nan")


def load(tag):
    out = {}
    for s in SETS:
        p = os.path.join(root, f"docs/bench-j2-{tag}-{s}.json")
        if os.path.exists(p):
            for prog, r in json.load(open(p))["programs"].items():
                out[f"{s}/{prog}"] = (s, r)
    return out


def find(d, prog):
    """The first set (awfy, apps, real, jit-trace, programs) that has a program of this name (json: awfy/json)."""
    for s in SETS:
        if f"{s}/{prog}" in d:
            return d[f"{s}/{prog}"]
    return None


def ratio(r, col, base="base"):
    a, b = r.get(base, {}), r.get(col, {})
    if "cycles_min" not in a or "cycles_min" not in b:
        return None
    return (b["cycles_min"] / a["cycles_min"], b["cycles_median"] / a["cycles_median"], b["instructions_min"] / a["instructions_min"])


def fmt(q):
    return "-" if q is None else f"{q[0]:.3f} / {q[1]:.3f} / {q[2]:.3f}"


def main():
    tags = [t for t in ["plain-warm0", "pgo-warm0", "plain-default", "pgo-default"] if load(t)]
    data = {t: load(t) for t in tags}
    print("# BUNDLE-J2 tables (cycles:u min / median / instructions:u min, COL / base; RUNS 11, CPU 4, `RUSTYNAIL_SEED=0`)\n")
    for item, col, targets, title in [("2", "i2", I2, "Item 2: per-site deeper inlining (`deep=8`), targets rec_loop lru_cache json oop_override")]:
        print(f"## {title}\n")
        names = targets
        print("| program | " + " | ".join(tags) + " |")
        print("|---|" + "---|" * len(tags))
        for p in names:
            print(f"| {p} | " + " | ".join(fmt(ratio(find(data[t], p)[1], col)) if find(data[t], p) else "-" for t in tags) + " |")
        print("| **targets geomean** | " + " | ".join(
            (lambda qs: f"**{gm([q[0] for q in qs]):.3f}** / {gm([q[1] for q in qs]):.3f} / {gm([q[2] for q in qs]):.3f} (n={len(qs)})" if qs else "-")(
                [q for q in (ratio(find(data[t], p)[1], col) for p in targets if find(data[t], p)) if q]) for t in tags) + " |")
        print()
    for col, name in [("i2", "item 2"), ("dup", "duplicate baseline")]:
        print(f"## Set geomeans, {name} ({col} / base)\n")
        print("| set (n) | " + " | ".join(tags) + " |")
        print("|---|" + "---|" * len(tags))
        for s in SETS + ["awfy+apps+real", "all"]:
            row = []
            for t in tags:
                qs = []
                for p, (ps, r) in data[t].items():
                    if s == "all" or ps == s or (s == "awfy+apps+real" and ps in ("awfy", "apps", "real")):
                        q = ratio(r, col)
                        if q:
                            qs.append(q)
                row.append(f"{gm([q[0] for q in qs]):.3f} / {gm([q[1] for q in qs]):.3f} / {gm([q[2] for q in qs]):.3f} (n={len(qs)})" if qs else "-")
            print(f"| {s} | " + " | ".join(row) + " |")
        print()
    for col, name in [("i2", "item 2")]:
        print(f"## All awfy / apps / real programs, {name} ({col} / base): trace-set side effects\n")
        print("| program | " + " | ".join(tags) + " |")
        print("|---|" + "---|" * len(tags))
        for p in sorted(p for p in data[tags[0]] if data[tags[0]][p][0] in ("awfy", "apps", "real")):
            print(f"| {p} | " + " | ".join(fmt(ratio(data[t][p][1], col)) if p in data[t] else "-" for t in tags) + " |")
        print()


if __name__ == "__main__":
    main()
