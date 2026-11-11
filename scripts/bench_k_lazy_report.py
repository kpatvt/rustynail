#!/usr/bin/env python3
"""BUNDLE-K report from scripts/bench_session.py JSON.

  scripts/bench_k_lazy_report.py FILE.json NEW BASE [DUP] [--rows prog,prog,...] [--worst PCT] [--movers N]

Per program: ratio NEW/BASE of min cycles and min instructions (and of the median cycles); geomeans over all programs and per set; the
duplicate column's deviation (DUP/NEW) as the noise estimate; programs worse than PCT % in cycles or 1 % in instructions; the N best and
worst movers; --rows prints before -> after (Mcyc, Minstr) for the named programs (match on the part after the set, or set/prog).
"""
import json, math, sys


def gm(xs):
    xs = [x for x in xs if x and x > 0]
    return math.exp(sum(math.log(x) for x in xs) / len(xs)) if xs else float("nan")


def mn(v):
    v = [x for x in v if x]
    return min(v) if v else None


def med(v):
    v = sorted(x for x in v if x)
    return v[len(v) // 2] if v else None


def main():
    a = sys.argv[1:]
    f, new, base = a[0], a[1], a[2]
    dup = a[3] if len(a) > 3 and not a[3].startswith("--") else None
    opt = {}
    i = 3 + (1 if dup else 0)
    while i < len(a):
        opt[a[i]] = a[i + 1] if i + 1 < len(a) else ""
        i += 2
    d = json.load(open(f))["results"]
    rows = []
    for k, r in sorted(d.items()):
        c = r["cols"]
        if new not in c or base not in c or not c[new].get("cyc") or not c[base].get("cyc"):
            continue
        cn, cb = mn(c[new]["cyc"]), mn(c[base]["cyc"])
        inn, ib = mn(c[new]["ins"]), mn(c[base]["ins"])
        mcn, mcb = med(c[new]["cyc"]), med(c[base]["cyc"])
        dd = None
        if dup and c.get(dup, {}).get("cyc"):
            dd = mn(c[dup]["cyc"]) / cn
        rows.append((k, cn / cb, inn / ib, mcn / mcb, cn, cb, inn, ib, dd, c[new].get("exp"), c[new].get("out")))
    print(f"programs {len(rows)}; cycles(min) geomean {gm([r[1] for r in rows]):.4f}  instr geomean {gm([r[2] for r in rows]):.4f}  median-cycles geomean {gm([r[3] for r in rows]):.4f}")
    if dup:
        ds = [r[8] for r in rows if r[8]]
        print(f"dup/new cycles geomean {gm(ds):.4f}  worst {max(ds):.3f} best {min(ds):.3f}")
    sets = {}
    for r in rows:
        sets.setdefault(r[0].split("/")[0] if not r[0].startswith("corpus2") else r[0].rsplit("/", 1)[0], []).append(r)
    for s, rs in sorted(sets.items()):
        print(f"  {s:18} n={len(rs):3} cycles {gm([r[1] for r in rs]):.4f} instr {gm([r[2] for r in rs]):.4f}")
    bad = [r for r in rows if r[9] == "MISMATCH"]
    print("expected-output mismatches:", [r[0] for r in bad])
    pct = float(opt.get("--worst", "3"))
    print(f"\nworse than {pct}% cycles or 1% instructions:")
    for r in sorted(rows, key=lambda r: -r[1]):
        if r[1] > 1 + pct / 100 or r[2] > 1.01:
            print(f"  {r[0]:34} cyc {r[1]:.3f} (med {r[3]:.3f}) ins {r[2]:.3f}   {r[5]/1e6:8.1f} -> {r[4]/1e6:8.1f} Mcyc" + (f"  dup {r[8]:.3f}" if r[8] else ""))
    n = int(opt.get("--movers", "12"))
    print(f"\nbest {n}:")
    for r in sorted(rows, key=lambda r: r[1])[:n]:
        print(f"  {r[0]:34} cyc {r[1]:.3f} (med {r[3]:.3f}) ins {r[2]:.3f}   {r[5]/1e6:8.1f} -> {r[4]/1e6:8.1f} Mcyc")
    if "--rows" in opt:
        print("\nrows:")
        want = opt["--rows"].split(",")
        for r in rows:
            if r[0] in want or r[0].split("/")[-1] in want:
                print(f"  {r[0]:34} {r[5]/1e6:9.1f} -> {r[4]/1e6:9.1f} Mcyc ({r[1]:.3f}, med {r[3]:.3f})   {r[7]/1e6:9.1f} -> {r[6]/1e6:9.1f} Minstr ({r[2]:.3f})")


main()
