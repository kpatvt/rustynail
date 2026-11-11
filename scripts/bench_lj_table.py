#!/usr/bin/env python3
"""Sorted tables + geomeans from docs/bench-lj-<set>.json (see scripts/bench_lj.py).
  python3 scripts/bench_lj_table.py docs/bench-lj-programs.json [...] > docs/bench-lj-table.md
Columns: Mcycles (min cycles:u) of PUC / ours / LuaJIT, ours/LJ, PUC/LJ, ours dup-noise (ours2/ours), output class of LJ
(same | numeq = equal after numeric tolerance | diff = output differs: workload comparable only, see notes)."""
import json, math, sys


def gm(xs):
    xs = [x for x in xs if x and x > 0]
    return math.exp(sum(math.log(x) for x in xs) / len(xs)) if xs else float("nan")


def main():
    allrows = []
    for f in sys.argv[1:]:
        r = json.load(open(f))
        for p, row in r["programs"].items():
            try:
                ref, o, o2, lj = (row[k] for k in ("ref", "ours", "ours2", "lj"))
            except KeyError:
                continue
            if "cycles_min" not in lj or "cycles_min" not in o:
                allrows.append((r["set"], p, None, row))
                continue
            allrows.append((r["set"], p, dict(ref=ref["cycles_min"], ours=o["cycles_min"], lj=lj["cycles_min"],
                                              noise=o2["cycles_min"] / o["cycles_min"], out=lj["output"],
                                              var=lj["variant"]), row))
    sets = sorted(set(x[0] for x in allrows))
    for s in sets:
        rows = [(p, d) for (st, p, d, _) in allrows if st == s and d]
        rows.sort(key=lambda x: -x[1]["ours"] / x[1]["lj"])
        print(f"### {s}\n")
        print("| program | PUC Mcyc | ours Mcyc | LuaJIT Mcyc | ours/LJ | PUC/LJ | dup noise | LJ output | LJ variant |")
        print("|---|---|---|---|---|---|---|---|---|")
        for p, d in rows:
            print(f"| {p} | {d['ref'] / 1e6:.1f} | {d['ours'] / 1e6:.1f} | {d['lj'] / 1e6:.1f} | **{d['ours'] / d['lj']:.2f}** | "
                  f"{d['ref'] / d['lj']:.2f} | {d['noise']:.3f} | {d['out']} | {'yes' if d['var'] else ''} |")
        sk = [p for (st, p, d, _) in allrows if st == s and not d]
        if sk:
            print(f"\nNot comparable (no LuaJIT run): {', '.join(sk)}")
        for lab, sel in (("all comparable", lambda d: True), ("exact/numeq output only", lambda d: d["out"] != "diff")):
            q = [d for _, d in rows if sel(d)]
            print(f"\n{s} geomean ({lab}, n={len(q)}): ours/LJ {gm([d['ours'] / d['lj'] for d in q]):.3f}, "
                  f"PUC/LJ {gm([d['ref'] / d['lj'] for d in q]):.3f}, ours/PUC {gm([d['ours'] / d['ref'] for d in q]):.3f}")
        print()


if __name__ == "__main__":
    main()
