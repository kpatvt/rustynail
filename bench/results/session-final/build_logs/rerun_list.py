import json, sys
D = sys.argv[1]
A = json.load(open(D + "/passA_cpus3-7_all_columns.json"))["results"]
mn = lambda k, c: min([x for x in A[k]["cols"].get(c, {}).get("cyc", []) if x] or [0]) or None
s9, s8 = [], []
for k in sorted(A):
    p, g, a, b, c, d = (mn(k, x) for x in ("plain", "pgo", "b9p", "b9g", "b8p", "b8g"))
    if p and a and g and b and (p / a > 1.03 or g / b > 1.03): s9.append((k, p / a, g / b, p / 1e6))
    if p and c and g and d and (p / c > 1.03 or g / d > 1.03): s8.append((k, p / c, g / d, p / 1e6))
print("vs9", len(s9)); [print(" ", x[0], f"{x[1]:.3f} {x[2]:.3f} {x[3]:.0f}M") for x in s9]
print("vs8", len(s8)); [print(" ", x[0], f"{x[1]:.3f} {x[2]:.3f} {x[3]:.0f}M") for x in s8]
open(D + "/rerun55_programs.txt", "w").write(",".join(sorted({x[0] for x in s9} | {x[0] for x in s8})))
print(len({x[0] for x in s9} | {x[0] for x in s8}))
