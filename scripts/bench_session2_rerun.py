#!/usr/bin/env python3
"""usage: bench_session2_rerun.py passA.json passB.json rerun.json out.md -- serial CPU-4 x9 re-run of the outliers (c276 and cb1 baselines)."""
import json, sys
A, B, R = (json.load(open(p))["results"] for p in sys.argv[1:4])


def mn(D, k, c, f="cyc"):
    v = [x for x in D.get(k, {}).get("cols", {}).get(c, {}).get(f, []) if x]
    return min(v) if v else None


def rt(D, k, a, b, f="cyc"):
    x, y = mn(D, k, a, f), mn(D, k, b, f)
    return x / y if x and y else float("nan")


L = ["| program | plain/c276 A | B | serial | instr | pgo/c276pgo A | B | serial | instr | plain/cb1 A | B | serial | instr | pgo/cb1pgo serial | instr | dup serial | plain Mcyc |",
     "|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|"]
for k in sorted(R):
    row = [k]
    for a, b in (("nplain", "mplain"), ("npgo", "mpgo"), ("nplain", "oplain")):
        row += [f"{rt(A,k,a,b):.3f}", f"{rt(B,k,a,b):.3f}", f"{rt(R,k,a,b):.3f}", f"{rt(R,k,a,b,'ins'):.3f}"]
    row += [f"{rt(R,k,'npgo','opgo'):.3f}", f"{rt(R,k,'npgo','opgo','ins'):.3f}", f"{rt(R,k,'nplain','nplain2'):.3f}", f"{mn(R,k,'nplain')/1e6:.0f}"]
    L.append("| " + " | ".join(row) + " |")
open(sys.argv[4], "w").write("\n".join(L) + "\n")
print("\n".join(L))
