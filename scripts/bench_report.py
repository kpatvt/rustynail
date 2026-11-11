#!/usr/bin/env python3
"""Markdown tables from scripts/bench.py JSON results (docs/PERF.md "Final report").

  scripts/bench_report.py results.json [--base ref] [--metric time|instructions|rss|all]

For every benchmark: the baseline's absolute value and each other binary's ratio to it (time = min wall
time of the runs, cycles = cycles:u of the perf run, instructions = instructions:u, rss = peak RSS), then the geomean and the worst ratio.
Ratios below 1 mean faster/smaller than the baseline.
"""
import argparse, json, math


def geomean(xs):
    return math.exp(sum(math.log(x) for x in xs) / len(xs)) if xs else float("nan")


def value(r, metric):
    if metric == "time":
        return r.get("min")
    if metric in ("instructions", "cycles"):
        return r.get(metric)
    return r.get("rss_kb")


def table(res, base, bins, metric):
    unit = {"time": "ms", "instructions": "M instr", "cycles": "M cycles", "rss": "MB"}[metric]
    scale = {"time": 1000.0, "instructions": 1e-6, "cycles": 1e-6, "rss": 1 / 1024}[metric]
    out = [f"| benchmark | {base} ({unit}) | " + " | ".join(bins) + " |",
           "|---|---:|" + "---:|" * len(bins)]
    ratios = {b: [] for b in bins}
    for name, r in res.items():
        bv = value(r.get(base, {}), metric)
        if not bv:
            continue
        cells = []
        for b in bins:
            v = value(r.get(b, {}), metric)
            if v:
                ratios[b].append(v / bv)
                cells.append(f"{v / bv:.2f}")
            else:
                cells.append("n/a")
        out.append(f"| {name} | {bv * scale:,.0f} | " + " | ".join(cells) + " |")
    out.append("| **geomean** | | " + " | ".join(f"**{geomean(ratios[b]):.3f}**" for b in bins) + " |")
    out.append("| worst | | " + " | ".join(f"{max(ratios[b]):.2f}" if ratios[b] else "n/a" for b in bins) + " |")
    return "\n".join(out)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("json")
    ap.add_argument("--base", default="ref")
    ap.add_argument("--metric", default="all", choices=["time", "cycles", "instructions", "rss", "all"])
    a = ap.parse_args()
    res = json.load(open(a.json))
    bins = []
    for r in res.values():
        for b in r:
            if b != a.base and b not in bins:
                bins.append(b)
    for m in (["time", "cycles", "instructions", "rss"] if a.metric == "all" else [a.metric]):
        print(f"\n**{m}** (ratio to `{a.base}`)\n")
        print(table(res, a.base, bins, m))


main()
