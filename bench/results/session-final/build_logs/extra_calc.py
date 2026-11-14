import json, math, sys, os
D = sys.argv[1]
ld = lambda f: json.load(open(os.path.join(D, f)))["results"]
gm = lambda xs: math.exp(sum(math.log(x) for x in xs) / len(xs))
def v(R, k, c, f="cyc"):
    x = [y for y in R.get(k, {}).get("cols", {}).get(c, {}).get(f, []) if y]; return min(x) if x else None
A = ld("passA_cpus3-7_all_columns.json"); B = ld("passB_cpus4-6_fast_lj.json")
ks = [k for k in A if v(A, k, "ljoff") and v(A, k, "plain")]
print("plain/LJoff geomean", len(ks), f"{gm([v(A,k,'plain')/v(A,k,'ljoff') for k in ks]):.3f}", " int/LJoff", f"{gm([v(A,k,'int')/v(A,k,'ljoff') for k in ks]):.3f}", " LJ/LJoff", f"{gm([v(A,k,'lj')/v(A,k,'ljoff') for k in ks if v(A,k,'lj')]):.3f}", " pgo/LJoff", f"{gm([v(A,k,'pgo')/v(A,k,'ljoff') for k in ks]):.3f}")
print("pass A: plain/LJ", f"{gm([v(A,k,'plain')/v(A,k,'lj') for k in A if v(A,k,'lj')]):.3f}", "pgo/LJ", f"{gm([v(A,k,'pgo')/v(A,k,'lj') for k in A if v(A,k,'lj')]):.3f}")
d = [v(A,k,'plain')/v(A,k,'plain2') for k in A]
print("dup A: >3% off", sum(1 for x in d if abs(x-1)>0.03), "max", max(d), "min", min(d))
for c in ("plain", "pgo", "b9p", "b9g", "b8p", "b8g", "lj", "puc"):
    r = [v(A,k,c)/v(B,k,c) for k in B if v(A,k,c) and v(B,k,c)]
    big = [(k, v(A,k,c)/v(B,k,c)) for k in B if v(A,k,c) and v(B,k,c) and abs(v(A,k,c)/v(B,k,c)-1) > 0.05 and v(A,k,c) > 30e6]
    print("A/B", c, f"gm {gm(r):.4f}", "n>5% off (>=30M):", len(big))
print("--- path-length sweep (min of 11, CPU 4)")
S = {"pad0 (15-run rerun)": ld("rerun55_serial_cpu4_15runs.json")}
for p in (1, 5, 12, 29): S[f"+{p}"] = ld(f"sweep_pad{p}.json")
progs = ["awfy/towers","lib/co_yield","lib/m_tointeger","lib/sel_n","lib/tonumber_base","real/template_engine","lib/gsub_fn"]
print("| program | " + " | ".join(f"{n}: plain/9a9 , PGO/9a9" for n in S) + " |")
print("|---|" + "---|" * len(S))
for k in progs:
    row = []
    for n, R in S.items():
        row.append(f"{v(R,k,'plain')/v(R,k,'b9p'):.3f} , {v(R,k,'pgo')/v(R,k,'b9g'):.3f} (ins {v(R,k,'plain','ins')/v(R,k,'b9p','ins'):.3f}/{v(R,k,'pgo','ins')/v(R,k,'b9g','ins'):.3f})")
    print(f"| {k} | " + " | ".join(row) + " |")
