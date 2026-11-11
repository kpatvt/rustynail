#!/usr/bin/env python3
"""J5 P9: the 25-program comparison with the T13 table (docs/bench-t13-j4.txt).   scripts/p9_t13.py progs.json
T13 columns: ref (PUC 5.5), interp, tier1, regions (J4), lj.  P9 columns: j4 (J5 binary T15=on TRACE=off), t1, tr, ref89
(main@89963c6 binary).  Mcycles are absolute cycles:u (min of 9) on the same machine class but not the same day."""
import json, math, re, sys

t13 = {}
for l in open(__file__.replace("scripts/p9_t13.py", "docs/bench-t13-j4.txt")):
    f = l.split()
    if len(f) >= 10 and re.match(r"^[a-z_0-9]+$", f[0]) and re.match(r"^[0-9.]+$", f[1]):
        t13[f[0]] = dict(ref=float(f[1]), interp=float(f[2]), tier1=float(f[4]), regions=float(f[6]))
p9 = json.load(open(sys.argv[1]))["programs"]
print(f"{'program':<13}{'T13 t1':>8}{'P9 t1':>8}{'T13 J4':>8}{'P9 j4':>8}{'P9 89963c6':>11}{'P9 trace':>9}{'tr/j4':>7}{'tr/t1':>7}{'T13 J4/t1':>10}{'P9 J4/t1':>9}")
g = {k: [] for k in ("tr_j4", "tr_t1", "j4_t1_13", "j4_t1_p9", "p9j4_t13j4")}
for p in sorted(p9):
    r = p9[p]
    c = lambda k: r[k]["cycles_min"] / 1e6
    t = t13[p]
    print(f"{p:<13}{t['tier1']:>8.0f}{c('t1'):>8.0f}{t['regions']:>8.0f}{c('j4'):>8.0f}{c('ref89'):>11.0f}{c('tr'):>9.0f}"
          f"{c('tr') / c('j4'):>7.3f}{c('tr') / c('t1'):>7.3f}{t['regions'] / t['tier1']:>10.3f}{c('j4') / c('t1'):>9.3f}")
    g["tr_j4"].append(c("tr") / c("j4"))
    g["tr_t1"].append(c("tr") / c("t1"))
    g["j4_t1_13"].append(t["regions"] / t["tier1"])
    g["j4_t1_p9"].append(c("j4") / c("t1"))
    g["p9j4_t13j4"].append(c("j4") / t["regions"])
gm = lambda xs: math.exp(sum(map(math.log, xs)) / len(xs))
print()
print("geomean over the 25: tr/j4 %.3f  tr/t1 %.3f  | J4/t1: T13 %.3f, P9 %.3f | P9 j4 / T13 J4 (absolute cycles, other day) %.3f" % (
    gm(g["tr_j4"]), gm(g["tr_t1"]), gm(g["j4_t1_13"]), gm(g["j4_t1_p9"]), gm(g["p9j4_t13j4"])))
