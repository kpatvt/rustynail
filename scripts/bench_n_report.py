import json, sys, math
f = sys.argv[1]
new = sys.argv[2] if len(sys.argv) > 2 else "plain"
base = "bplain"
dup = "pgo2"
d = json.load(open(f))["programs"]
only = set(sys.argv[3].split(",")) if len(sys.argv) > 3 else None
print("%-20s %8s %8s %8s %8s | %8s %8s | dup(min) exp" % ("prog", "cyc_min", "cyc_med", "ins_min", "ins_med", "base cyc", "base ins"))
g = {k: [] for k in ("cm", "cd", "im", "id")}
for p, r in d.items():
    if only and p not in only:
        continue
    if new not in r or "cycles_min" not in r[new] or "cycles_min" not in r[base]:
        continue
    n, b, u = r[new], r[base], r.get(dup, {})
    cm = n["cycles_min"] / b["cycles_min"]
    cd = n["cycles_median"] / b["cycles_median"]
    im = n["instructions_min"] / b["instructions_min"]
    idd = n["instructions_median"] / b["instructions_median"]
    du = u["cycles_min"] / b["cycles_min"] if "cycles_min" in u else float("nan")
    g["cm"].append(cm); g["cd"].append(cd); g["im"].append(im); g["id"].append(idd)
    print("%-20s %8.3f %8.3f %8.3f %8.3f | %8.0f %8.0f | %6.3f %s" % (p, cm, cd, im, idd, b["cycles_min"] / 1e6, b["instructions_min"] / 1e6, du, n.get("expected", "")))
gm = lambda x: math.exp(sum(map(math.log, x)) / len(x))
print("geomean (%d): cyc min %.4f med %.4f ins min %.4f med %.4f" % (len(g["cm"]), gm(g["cm"]), gm(g["cd"]), gm(g["im"]), gm(g["id"])))
