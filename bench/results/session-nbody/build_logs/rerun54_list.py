import json
C = json.load(open("pass54_cpus4-6.json"))["results"]
mn = lambda k, c: min([x for x in C[k]["cols"].get(c, {}).get("cyc", []) if x] or [0]) or None
xs = []
for k in C:
    a, b, c, d, m = mn(k, "p54"), mn(k, "puc"), mn(k, "p55"), mn(k, "ref55"), mn(k, "p54")
    if a and b and c and d and a >= 20e6: xs.append(((a / b) / (c / d), k))
xs.sort(reverse=True)
print(len(C))
open("rerun54_programs.txt", "w").write(",".join(k for _, k in xs[:20]))
print(xs[:20])
