import json, math, os, sys
D = sys.argv[1]
ld = lambda f: json.load(open(os.path.join(D, f)))["results"]
gm = lambda xs: math.exp(sum(math.log(x) for x in xs) / len(xs))
def v(R, k, c, f="cyc"):
    x = [y for y in R.get(k, {}).get("cols", {}).get(c, {}).get(f, []) if y]; return min(x) if x else None
A = ld("passA_cpus3-7_all_columns.json"); B = ld("passB_cpus4-6_fast_lj.json")
print("pass A: plain/LJ", f"{gm([v(A,k,'plain')/v(A,k,'lj') for k in A if v(A,k,'lj')]):.3f}", "pgo/LJ", f"{gm([v(A,k,'pgo')/v(A,k,'lj') for k in A if v(A,k,'lj')]):.3f}")
d = [v(A, k, 'plain') / v(A, k, 'plain2') for k in A]
print("dup A: >3% off", sum(1 for x in d if abs(x - 1) > 0.03), "max", f"{max(d):.3f}", "min", f"{min(d):.3f}", "geomean", f"{gm(d):.3f}")
for c in ("plain", "pgo", "b1p", "b1g", "lj", "puc"):
    r = [v(A, k, c) / v(B, k, c) for k in B if v(A, k, c) and v(B, k, c)]
    big = [k for k in B if v(A, k, c) and v(B, k, c) and abs(v(A, k, c) / v(B, k, c) - 1) > 0.05 and v(A, k, c) > 30e6]
    print("A/B", c, f"gm {gm(r):.4f}", "n>5% off (>=30M):", len(big))
print("--- path-length sweep (min of 11, CPU 4): plain/b1p , PGO/b1g (instr ratios)")
RR = ld("rerun55_serial_cpu4_15runs.json")
S = {"pad0 (15-run)": RR}
for p in (1, 5, 12, 29): S[f"+{p}"] = ld(f"sweep_pad{p}.json")
progs = ["corpus/parser_combinator", "lib/find_cap", "lib/fmt_g", "lib/fmt_q", "lib/tonumber_bad", "lib/tunpack"]
print("| program | " + " | ".join(S) + " |"); print("|---|" + "---|" * len(S))
for k in progs:
    row = []
    for n, R in S.items():
        row.append(f"{v(R,k,'plain')/v(R,k,'b1p'):.3f}, {v(R,k,'pgo')/v(R,k,'b1g'):.3f} (ins {v(R,k,'plain','ins')/v(R,k,'b1p','ins'):.3f}/{v(R,k,'pgo','ins')/v(R,k,'b1g','ins'):.3f})")
    print(f"| {k} | " + " | ".join(row) + " |")
print("--- startup (cycles k, empty)")
s = json.load(open(os.path.join(D, "startup_size.json")))
for n, x in s["empty"].items(): print(f"{n:18s} cyc {x['cyc_min']/1e3:.0f}k ins {x['ins_min']/1e3:.0f}k")
print("rss", {n: s["rss_kb"][n]["rss.lua"][0] for n in s["rss_kb"]})
print("--- hyperfine")
for f in ["startup_hyperfine.json"] + [f"startup_hf_jit{m}_{i}.json" for i in (1, 2, 3) for m in ("on", "off")]:
    for r in json.load(open(os.path.join(D, f)))["results"]:
        w = r["summary"]["time_wall_clock"]
        print(f"{f:28s} {r['name']:22s} min {w['min']*1000:.3f} median {w['median']*1000:.3f} ms")
print("--- JS ports (min of 5, CPU 4)")
J = json.load(open(os.path.join(D, "js_vs_node.json")))
print("| program | node Mcyc | node ms | plain Mcyc | plain ms | PGO Mcyc | PGO ms | LuaJIT Mcyc | LuaJIT ms | plain/node cyc | PGO/node cyc | plain/node wall | PGO/node wall | LJ/node wall |"); print("|" + "---|" * 14)
for p, d in J.items():
    m = lambda n, f: min(x for x in d[n][f] if x)
    lj_c, lj_w = m("luajit", "cyc"), m("luajit", "wall")
    if p == "heapsort":
        lj_c, lj_w = 1229.8e6, None
    print(f"| {p} | {m('node','cyc')/1e6:.0f} | {m('node','wall')*1000:.0f} | {m('plain','cyc')/1e6:.0f} | {m('plain','wall')*1000:.0f} | {m('pgo','cyc')/1e6:.0f} | {m('pgo','wall')*1000:.0f} | {lj_c/1e6:.0f} | {'-' if lj_w is None else round(lj_w*1000)} | {m('plain','cyc')/m('node','cyc'):.2f} | {m('pgo','cyc')/m('node','cyc'):.2f} | {m('plain','wall')/m('node','wall'):.2f} | {m('pgo','wall')/m('node','wall'):.2f} | {'-' if lj_w is None else round(lj_w/m('node','wall'),2)} |")
