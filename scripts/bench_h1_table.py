#!/usr/bin/env python3
"""Per-program ratio table and geomeans from bench_lj.py JSON outputs (H1).

  scripts/bench_h1_table.py docs/bench-h1-lib.json docs/bench-h1-awfy.json ... [--min-lj MCYC]

For every program: Mcyc (min of RUNS) of ref/ours/lj, ours/LJ, ours/PUC, output class, expected-file status;
per file: geomean ours/LJ, PUC/LJ, ours/PUC over the comparable rows; then the ranked list of ours/LJ >= 1.5.
Rows whose output differs from the expected file are excluded from the geomeans and flagged."""
import json, math, sys


def gm(xs):
    xs = [x for x in xs if x and x > 0]
    return math.exp(sum(math.log(x) for x in xs) / len(xs)) if xs else float("nan")


def main():
    files = [a for a in sys.argv[1:] if not a.startswith("--")]
    allrows = []
    for f in files:
        d = json.load(open(f))
        rows = []
        for p, r in d["programs"].items():
            ref, ours, lj = (r.get(k, {}) for k in ("ref", "ours", "lj"))
            if "cycles_min" not in ours or "cycles_min" not in lj:
                continue
            bad = any(r.get(k, {}).get("expected") == "MISMATCH" for k in ("ref", "ours", "lj"))
            rows.append((p, ref.get("cycles_min", 0), ours["cycles_min"], lj["cycles_min"], lj.get("output", "?"), bad))
        print(f"== {f} (set {d['set']}, runs {d['runs']})")
        print(f"{'program':<22}{'PUC':>9}{'ours':>9}{'LJ':>9}{'ours/LJ':>9}{'ours/PUC':>10}  out")
        for p, r, o, l, out, bad in sorted(rows, key=lambda x: -x[2] / x[3]):
            print(f"{p:<22}{r / 1e6:>9.1f}{o / 1e6:>9.1f}{l / 1e6:>9.1f}{o / l:>9.2f}{(o / r if r else 0):>10.3f}  {out}{' EXPECTED-MISMATCH' if bad else ''}")
        ok = [x for x in rows if not x[5]]
        print(f"geomean ({len(ok)} rows): ours/LJ {gm([o / l for _, _, o, l, _, _ in ok]):.3f}   PUC/LJ {gm([r / l for _, r, _, l, _, _ in ok]):.3f}"
              f"   ours/PUC {gm([o / r for _, r, o, _, _, _ in ok]):.3f}   ours faster than LJ in {sum(1 for x in ok if x[2] < x[3])}/{len(ok)}")
        allrows += [(f, x) for x in ok]
        print()
    gaps = sorted(((x[2] / x[3], f, x[0]) for f, x in allrows if x[2] / x[3] >= 1.5), reverse=True)
    print("ours/LJ >= 1.5 (all files):")
    for g, f, p in gaps:
        print(f"  {g:6.2f}  {p}  ({f})")


main()
