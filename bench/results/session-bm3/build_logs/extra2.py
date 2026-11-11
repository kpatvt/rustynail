#!/usr/bin/env python3
"""bm3: t1q-vs-amt knob split table. usage: extra2.py <knobs2.json>"""
import json, sys
K = json.load(open(sys.argv[1]))["results"]
cols = ["base", "amt0", "t1q1", "t1q5", "t1q9", "prev", "pbase", "pamt0", "pt1q1", "pprev"]
print("\n## Knob split: t1q bits vs amt (CPU 4, 15 runs, min Mcyc). t1q=13 default; t1q=1 bits 4/8 off; t1q=5 = bit 4 on; t1q=9 = bit 8 on (bit 4 off)\n")
print("base = 1e6ec4ca plain; amt0 = amt=0; t1qN = t1q=N; prev = 80f1d96e plain; pbase/pamt0/pt1q1 = PGO; pprev = 80f1d96e PGO\n")
print("| program | " + " | ".join(cols) + " |")
print("|---|" + "---|" * len(cols))
for k in sorted(K):
    row = []
    for c in cols:
        x = [y for y in K[k]["cols"][c]["cyc"] if y]
        row.append(f"{min(x)/1e6:.0f}")
    print(f"| {k} | " + " | ".join(row) + " |")
