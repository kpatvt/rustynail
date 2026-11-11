#!/usr/bin/env python3
"""Table of docs/bench-lj-off-<set>.json (columns ref, ours, ourstr0 = traces off (J4 only), lj, ljoff = luajit -joff)."""
import json, os, sys

root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
print("| set | program | PUC Mcyc | ours (trace) | ours J4 only | LuaJIT | LuaJIT -joff | ours/LJ | ours-J4/LJ | ours/LJ-joff |")
print("|---|---|---|---|---|---|---|---|---|---|")
for s in ('programs', 'real', 'jit-trace'):
    r = json.load(open(os.path.join(root, f'docs/bench-lj-off-{s}.json')))
    rows = []
    for p, row in r['programs'].items():
        try:
            v = {k: row[k]['cycles_min'] / 1e6 for k in ('ref', 'ours', 'ourstr0', 'lj', 'ljoff')}
        except KeyError:
            continue
        rows.append((v['ours'] / v['lj'], p, v))
    for _, p, v in sorted(rows, reverse=True):
        print(f"| {s} | {p} | {v['ref']:.0f} | {v['ours']:.0f} | {v['ourstr0']:.0f} | {v['lj']:.0f} | {v['ljoff']:.0f} | "
              f"{v['ours'] / v['lj']:.2f} | {v['ourstr0'] / v['lj']:.2f} | {v['ours'] / v['ljoff']:.2f} |")
