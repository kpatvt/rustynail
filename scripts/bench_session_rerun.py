#!/usr/bin/env python3
"""usage: bench_session_rerun.py passA.json passB.json rerun.json out.md -- table of the serial CPU-4 re-run of the >5% outliers."""
import json, sys
A, B, R = (json.load(open(p))["results"] for p in sys.argv[1:4])


def mn(D, k, c, f="cyc"):
    v = [x for x in D[k]["cols"].get(c, {}).get(f, []) if x]
    return min(v) if v else None


def rt(D, k, a, b, f="cyc"):
    x, y = mn(D, k, a, f), mn(D, k, b, f)
    return x / y if x and y else float("nan")


L = ["| program | plain/cb1 A (3-7) | B (4,6) | serial x9 | instr ratio | pgo/cb1pgo A | B | serial | instr | dup serial | plain Mcyc | verdict |",
     "|---|---|---|---|---|---|---|---|---|---|---|---|"]
for k in sorted(R):
    s, sp, d = rt(R, k, "nplain", "oplain"), rt(R, k, "npgo", "opgo"), rt(R, k, "nplain", "nplain2")
    i, ip = rt(R, k, "nplain", "oplain", "ins"), rt(R, k, "npgo", "opgo", "ins")
    c = mn(R, k, "nplain") / 1e6
    if max(s, sp) > 1.05:
        v = "real instr change" if max(i, ip) > 1.03 else "layout/timing noise (instr equal)" if max(s, sp) > 1.05 else ""
    elif min(s, sp) < 0.95:
        v = "win" + ("" if min(i, ip) < 0.97 else " (instr equal: layout/timing)")
    else:
        v = "noise (gone on serial re-run)"
    L.append(f"| {k} | {rt(A,k,'nplain','oplain'):.3f} | {rt(B,k,'nplain','oplain'):.3f} | {s:.3f} | {i:.3f} | {rt(A,k,'npgo','opgo'):.3f} | "
             f"{rt(B,k,'npgo','opgo'):.3f} | {sp:.3f} | {ip:.3f} | {d:.3f} | {c:.0f} | {v} |")
open(sys.argv[4], "w").write("\n".join(L) + "\n")
print("\n".join(L))
