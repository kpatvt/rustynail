import json, sys
D = sys.argv[1]
A = json.load(open(D + "/passA_cpus3-7_all_columns.json"))["results"]
mn = lambda k, c, f="cyc": min([x for x in A[k]["cols"].get(c, {}).get(f, []) if x] or [0]) or None
s = []
for k in sorted(A):
    p, g, a, b = (mn(k, x) for x in ("plain", "pgo", "b1p", "b1g"))
    pi, gi, ai, bi = (mn(k, x, "ins") for x in ("plain", "pgo", "b1p", "b1g"))
    flag = (p and a and p / a > 1.03) or (g and b and g / b > 1.03) or (pi and ai and pi / ai > 1.03) or (gi and bi and gi / bi > 1.03)
    if flag: s.append((k, p / a if p and a else 0, g / b if g and b else 0, pi / ai if pi and ai else 0, gi / bi if gi and bi else 0))
for x in s: print(" ", x[0], " ".join(f"{y:.3f}" for y in x[1:]))
open(D + "/rerun55_programs.txt", "w").write(",".join(x[0] for x in s))
print(len(s))
