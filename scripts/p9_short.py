import json, math, sys
d = json.load(open(sys.argv[1]))["programs"]
rs = {p: r["tr"]["cycles_min"] / r["t1"]["cycles_min"] for p, r in d.items()}
rj = {p: r["tr"]["cycles_min"] / r["j4"]["cycles_min"] for p, r in d.items()}
g = lambda xs: math.exp(sum(map(math.log, xs)) / len(xs))
print("n", len(rs), "geomean tr/t1 %.4f" % g(rs.values()), "max %s %.3f" % max(rs.items(), key=lambda x: x[1]), "min %s %.3f" % min(rs.items(), key=lambda x: x[1]))
print("geomean tr/j4 %.4f" % g(rj.values()), "max %s %.3f" % max(rj.items(), key=lambda x: x[1]))
over = sorted([(v, p) for p, v in rs.items() if v > 1.05], reverse=True)
print("tr/t1 above 1.05:", [(p, round(v, 3), round(d[p]["t1"]["cycles_min"] / 1e6, 1)) for v, p in over])
over = sorted([(v, p) for p, v in rj.items() if v > 1.05], reverse=True)
print("tr/j4 above 1.05:", [(p, round(v, 3), round(d[p]["j4"]["cycles_min"] / 1e6, 1)) for v, p in over])
