import json, sys
A = json.load(open("passA_cpus3-7_all_columns.json"))["results"]
mn = lambda k, c: min([x for x in A[k]["cols"][c]["cyc"] if x]) if A[k]["cols"].get(c, {}).get("cyc") else None
out = []
for k in sorted(A):
    p, g, bp, bg = mn(k, "plain"), mn(k, "pgo"), mn(k, "bplain"), mn(k, "bpgo")
    if not (p and g and bp and bg): continue
    if p / bp > 1.03 or g / bg > 1.03:
        out.append((k, p / bp, g / bg, p / 1e6))
for k, a, b, m in out: print(f"{k} plain/base {a:.3f} pgo/base {b:.3f} plain {m:.0f}M", file=sys.stderr)
print(len(out), file=sys.stderr)
open("rerun55_programs.txt", "w").write(",".join(k for k, *_ in out))
