import json, glob, math, sys
base, dup, new = "bplain", "pgo2", "plain"
allr = {k: [] for k in ("cm", "cd", "im", "id")}
worst = []
gm = lambda x: math.exp(sum(map(math.log, x)) / len(x))
for f in sorted(glob.glob(sys.argv[1] + "-*.json")):
    d = json.load(open(f))
    s = d["set"]
    g = {k: [] for k in allr}
    for p, r in d["programs"].items():
        if "cycles_min" not in r.get(new, {}) or "cycles_min" not in r.get(base, {}):
            continue
        n, b = r[new], r[base]
        v = (n["cycles_min"] / b["cycles_min"], n["cycles_median"] / b["cycles_median"],
             n["instructions_min"] / b["instructions_min"], n["instructions_median"] / b["instructions_median"])
        for k, x in zip(allr, v):
            g[k].append(x)
            allr[k].append(x)
        du = r[dup]["cycles_min"] / b["cycles_min"] if "cycles_min" in r.get(dup, {}) else float("nan")
        worst.append((v[0], v[1], v[2], du, s + "/" + p))
    print("%-10s n=%3d cyc min %.4f med %.4f ins min %.4f med %.4f" % (s, len(g["cm"]), gm(g["cm"]), gm(g["cd"]), gm(g["im"]), gm(g["id"])))
print("ALL        n=%3d cyc min %.4f med %.4f ins min %.4f med %.4f" % (len(allr["cm"]), gm(allr["cm"]), gm(allr["cd"]), gm(allr["im"]), gm(allr["id"])))
worst.sort(reverse=True)
print("worst / best by cycles min (min, med, ins min, dup, prog):")
for w in worst[:8] + worst[-4:]:
    print("  %.3f %.3f %.3f %.3f %s" % w)
