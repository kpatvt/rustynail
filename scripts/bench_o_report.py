#!/usr/bin/env python3
"""BUNDLE-O report: ratios new / base (< 1 is faster) over the per-set JSON files of scripts/bench_mfull.py.

  scripts/bench_o_report.py <prefix> <new> <base> <dup> [targets]   e.g. docs/bench-o/plain-default plain bplain bpgo class_middleclass
Columns in the JSON: plain, pgo, bplain, bpgo ... (see scripts/bench-mfull.sh; BUNDLE-O: plain = new, pgo = second variant, bplain = base, bpgo = base duplicate).
Prints per program (cycles min/med, instructions min/med, duplicate-baseline ratio), per-set and overall geomeans, the target geomean, the worst programs.
"""
import glob, json, math, os, sys

prefix, new, base, dup = sys.argv[1:5]
targets = set(sys.argv[5].split(",")) if len(sys.argv) > 5 else set()
gm = lambda x: math.exp(sum(map(math.log, x)) / len(x)) if x else float("nan")
rows = []
for f in sorted(glob.glob(prefix + "-*.json")):
    sset = os.path.basename(f)[len(os.path.basename(prefix)) + 1:-5]
    for p, r in json.load(open(f))["programs"].items():
        if new not in r or base not in r or "cycles_min" not in r[new] or "cycles_min" not in r[base]:
            continue
        n, b = r[new], r[base]
        u = r.get(dup, {})
        rows.append(dict(set=sset, prog=p, cm=n["cycles_min"] / b["cycles_min"], cd=n["cycles_median"] / b["cycles_median"],
                         im=n["instructions_min"] / b["instructions_min"], id=n["instructions_median"] / b["instructions_median"],
                         du=u["cycles_min"] / b["cycles_min"] if "cycles_min" in u else float("nan"),
                         dud=u["cycles_median"] / b["cycles_median"] if "cycles_median" in u else float("nan"),
                         bcyc=b["cycles_min"] / 1e6))
def line(name, rs):
    return "%-14s n=%3d  cyc min %.4f med %.4f | ins min %.4f med %.4f | dup min %.4f med %.4f" % (
        name, len(rs), gm([r["cm"] for r in rs]), gm([r["cd"] for r in rs]), gm([r["im"] for r in rs]), gm([r["id"] for r in rs]),
        gm([r["du"] for r in rows_ok(rs)]), gm([r["dud"] for r in rows_ok(rs)]))
def rows_ok(rs):
    return [r for r in rs if r["du"] == r["du"]]
print("%-26s %7s %7s %7s %7s | %8s | dup" % ("program", "cycmin", "cycmed", "insmin", "insmed", "base Mcyc"))
for r in sorted(rows, key=lambda r: -r["cm"]):
    flag = "  <-- worse" if r["cm"] > 1.03 else ("  <-- better" if r["cm"] < 0.97 else "")
    if flag or r["prog"] in targets:
        print("%-26s %7.3f %7.3f %7.3f %7.3f | %8.0f | %6.3f%s" % (r["set"] + "/" + r["prog"], r["cm"], r["cd"], r["im"], r["id"], r["bcyc"], r["du"], flag))
print()
for s in sorted(set(r["set"] for r in rows)):
    print(line(s, [r for r in rows if r["set"] == s]))
print(line("ALL", rows))
old = [r for r in rows if r["set"] != "corpus"]
print(line("173 existing", old))
if targets:
    print(line("TARGETS", [r for r in rows if r["prog"] in targets]))
