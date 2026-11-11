#!/usr/bin/env python3
"""Markdown table from a bench_cycles.py json: per program the ratio of every column to the baseline, sorted by the ratio of
column COL (default the first non-baseline column), worst first.  usage: pprof-table.py run.json [col] [--min-ratio R]"""
import json, sys

d = json.load(open(sys.argv[1]))
names = list(d["bins"])
col = sys.argv[2] if len(sys.argv) > 2 and not sys.argv[2].startswith("--") else names[1]
rows = []
for p, r in d["programs"].items():
    b = r.get(names[0], {})
    if "cycles_min" not in b:
        continue
    rows.append((r[col]["cycles_min"] / b["cycles_min"], p, b, r))
rows.sort(reverse=True)
print("| program | " + names[0] + " Mcyc | " + " | ".join(f"{n} (ratio)" for n in names[1:]) + " | instr ratio " + col + " |")
print("|---|---|" + "---|" * (len(names) - 1) + "---|")
for _, p, b, r in rows:
    cells = []
    for n in names[1:]:
        x = r.get(n, {})
        cells.append(f"{x['cycles_min'] / b['cycles_min']:.3f}" if "cycles_min" in x else "-")
    ir = r[col]["instructions_min"] / b["instructions_min"]
    print(f"| {p} | {b['cycles_min'] / 1e6:.0f} | " + " | ".join(cells) + f" | {ir:.2f} |")
