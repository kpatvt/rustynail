#!/usr/bin/env python3
"""session-l tables. usage: report_l.py <session-dir>  -> tables.md (all numbers), per_program.csv
Reads fast_all.json (bench-fast.sh: subject columns + cached baselines) and rerun_all_15runs_cpu4.json (15-run serial reruns).
Ours columns use the minimum over every reading of the program (quick, full and rerun); cached columns are as stored."""
import json, math, os, sys, csv

D = sys.argv[1]
A = json.load(open(os.path.join(D, "fast_all.json")))["results"]
rp = os.path.join(D, "rerun_all_15runs_cpu4.json")
RR = json.load(open(rp))["results"] if os.path.exists(rp) else {}
USE_RR = False
BL = "/home/themadmin/Projects/rustynail/.claude/worktrees/agent-a70b4d7021ef0234a/bench/results/baselines/"
SJ = "/home/themadmin/Projects/rustynail/.claude/worktrees/agent-a70b4d7021ef0234a/bench/results/session-k/"
for col, fn in (("b_plain", "p55-5a064d7"), ("b_pgo", "g55-5a064d7"), ("b_p54", "p54-5a064d7"), ("b_g54", "g54-5a064d7"),
                ("i_plain", "p55-b116447"), ("i_pgo", "g55-b116447"), ("i_p54", "p54-b116447"), ("i_g54", "g54-b116447")):
    d = json.load(open(BL + fn + "-47266a41.json"))["results"]
    for k, e in A.items():
        if k in d:
            e["cols"][col] = {"cyc": d[k]["cyc"], "ins": d[k]["ins"]}
# session-k published numbers (set B as in its per_program.csv: min over all readings) and the 5a064d7 fast pass of this session (set A)
JR = {r["program"]: r for r in csv.DictReader(open(SJ + "per_program.csv"))}
for c0, c1 in (("plain", "j_plain"), ("pgo", "j_pgo"), ("p54", "j_p54"), ("g54", "j_g54")):
    for k, e in A.items():
        r = JR.get(k)
        if r and r.get(c0 + "_cyc_min"):
            e["cols"][c1] = {"cyc": [float(r[c0 + "_cyc_min"])], "ins": [float(r[c0 + "_ins_min"])]}
BA = json.load(open(os.path.join(D, "b5a_fast_all.json")))["results"]
for c0, c1 in (("plain", "bA_plain"), ("pgo", "bA_pgo"), ("p54", "bA_p54"), ("g54", "bA_g54")):
    for k, e in A.items():
        if k in BA and BA[k]["cols"].get(c0):
            e["cols"][c1] = {"cyc": BA[k]["cols"][c0]["cyc"], "ins": BA[k]["cols"][c0]["ins"]}
OURS = ["plain", "pgo", "p54", "g54"]
II = {"plain": "i_plain", "pgo": "i_pgo", "p54": "i_p54", "g54": "i_g54"}
HAA = {"plain": "bA_plain", "pgo": "bA_pgo", "p54": "bA_p54", "g54": "bA_g54"}
JJ = {"plain": "j_plain", "pgo": "j_pgo", "p54": "j_p54", "g54": "j_g54"}
PREV = {"plain": "b_plain", "pgo": "b_pgo", "p54": "b_p54", "g54": "b_g54"}
REF = {"plain": "puc", "pgo": "puc", "p54": "puc54", "g54": "puc54"}
LAB = {"plain": "5.5 plain", "pgo": "5.5 PGO", "p54": "5.4 plain", "g54": "5.4 PGO"}
REALDIS = {"real_dis_x86", "real_dis_arm", "real_dis_mips", "real_dis_ppc"}


def vals(k, c, f):
    out = []
    for R in ((A, RR) if USE_RR else (A,)):
        out += [x for x in R.get(k, {}).get("cols", {}).get(c, {}).get(f, []) if x]
    return out


def cyc(k, c):
    v = vals(k, c, "cyc") if c in OURS else [x for x in A[k]["cols"].get(c, {}).get("cyc", []) if x]
    return min(v) if v else None


def ins(k, c):
    v = vals(k, c, "ins") if c in OURS else [x for x in A[k]["cols"].get(c, {}).get("ins", []) if x]
    return min(v) if v else None


def ratio(k, a, b, f=cyc):
    x, y = f(k, a), f(k, b)
    return x / y if x and y else None


def gm(xs):
    xs = [x for x in xs if x and x > 0]
    return math.exp(sum(math.log(x) for x in xs) / len(xs)) if xs else float("nan")


f3 = lambda v: "-" if v is None or v != v else f"{v:.3f}"
K = sorted(A)
isrd = lambda k: k.split("/")[-1] in REALDIS


def setof(k):
    if isrd(k):
        return "real_dis"
    return "/".join(k.split("/")[:2]) if k.startswith("corpus2") else k.split("/")[0]


SETS = sorted({setof(k) for k in K})
L = []
P = L.append
P("# Tables, session-l (subject 7413bff)\n")
P("Ratios below 1: first column faster/fewer. Ours = min over all readings (2 quick, 5 full, 15-run serial reruns). Cached columns as stored.\n")

def agg_block(tag):
    global HL, VS
    # ---- headline
    P("## Headline geomeans (cycles, ours/ref)\n")
    P("| build | /LuaJIT cyc (n) | /LuaJIT ins | /PUC same version cyc (223) | /PUC ins |")
    P("|---|---|---|---|---|")
    HL = {}
    for c in OURS:
        kl = [k for k in K if cyc(k, "lj")]
        HL[c] = (gm([ratio(k, c, "lj") for k in kl]), gm([ratio(k, c, "lj", ins) for k in kl]), gm([ratio(k, c, REF[c]) for k in K]), gm([ratio(k, c, REF[c], ins) for k in K]), len(kl))
        h = HL[c]
        P(f"| {LAB[c]} | {f3(h[0])} ({h[4]}) | {f3(h[1])} | {f3(h[2])} | {f3(h[3])} |")
    P("")
    for c in ("plain", "pgo"):
        kl = [k for k in K if cyc(k, "ljoff")]
        P(f"- {LAB[c]} / LuaJIT -joff ({len(kl)}): {f3(gm([ratio(k, c, 'ljoff') for k in kl]))}")
    kl = [k for k in K if cyc(k, "lj")]
    for lab, pre in (("5a064d7 same-session (set B)", "b_"), ("session-k published 5a064d7", "j_"), ("b116447 cached (session-k set B)", "i_")):
        P(f"- {lab} (same programs): 5.5 plain/LJ {f3(gm([ratio(k,pre+'plain','lj') for k in kl]))}, PGO/LJ {f3(gm([ratio(k,pre+'pgo','lj') for k in kl]))}, 5.4 plain/LJ {f3(gm([ratio(k,pre+'p54','lj') for k in kl]))}, 5.4 PGO/LJ {f3(gm([ratio(k,pre+'g54','lj') for k in kl]))}")
    P(f"- our 5.4 build / our 5.5 build cycles: plain {f3(gm([ratio(k,'p54','plain') for k in K]))}, PGO {f3(gm([ratio(k,'g54','pgo') for k in K]))}; PGO/plain: 5.5 {f3(gm([ratio(k,'pgo','plain') for k in K]))}, 5.4 {f3(gm([ratio(k,'g54','p54') for k in K]))}")
    
    # ---- vs prev / b1
    P("\n## Subject vs 5a064d7 (same session, set B), 5a064d7 set A, session-k published and b116447 (geomean, 223 programs)\n")
    P("| build | vs | cycles | instructions | cyc >3% better / within / >3% worse | ins >3% fewer / more |")
    P("|---|---|---|---|---|---|")
    VS = {}
    for c in OURS:
        for tag, o in (("5a064d7", PREV[c]), ("5a064d7 setA", HAA[c]), ("session-k published", JJ[c]), ("b116447", II[c])):
            if not o:
                continue
            cs = [ratio(k, c, o) for k in K if ratio(k, c, o)]
            isr = [ratio(k, c, o, ins) for k in K if ratio(k, c, o, ins)]
            VS[(c, tag)] = (gm(cs), gm(isr))
            P(f"| {LAB[c]} | {tag} | {f3(gm(cs))} | {f3(gm(isr))} | {sum(1 for x in cs if x<0.97)} / {sum(1 for x in cs if 0.97<=x<=1.03)} / {sum(1 for x in cs if x>1.03)} | {sum(1 for x in isr if x<0.97)} / {sum(1 for x in isr if x>1.03)} |")
    P("\n### Per set, geomean subject / 5a064d7 and subject / b116447 (cycles / instructions)\n")
    P("| set | n | plain vs b cyc | ins | PGO vs b cyc | ins | 5.4 plain vs b cyc | ins | 5.4 PGO vs b cyc | ins | plain vs i cyc | ins | PGO vs i cyc | ins | 5.4 plain vs i cyc | ins | 5.4 PGO vs i cyc | ins |")
    P("|" + "---|" * 18)
    for s in SETS + ["ALL"]:
        ks = K if s == "ALL" else [k for k in K if setof(k) == s]
        row = [s, str(len(ks))]
        for c in OURS:
            row += [f3(gm([ratio(k, c, PREV[c]) for k in ks])), f3(gm([ratio(k, c, PREV[c], ins) for k in ks]))]
        for c in OURS:
            row += [f3(gm([ratio(k, c, II[c]) for k in ks])), f3(gm([ratio(k, c, II[c], ins) for k in ks]))]
        P("| " + " | ".join(row) + " |")
    
    

for USE_RR, tag in ((False, "A: the fast pass only (min of 2 or 5 runs per program)"), (True, "B: with the 15-run serial reruns of 119 programs merged (min over all readings)")):
    P(f"\n# Aggregates, readings {tag}\n")
    agg_block(tag)
USE_RR = True
M20 = lambda k, c: (cyc(k, c) or 0) >= 20e6


def listing(title, c, o, key, rev, n, cond=None, minM=20e6):
    P(f"\n### {title}\n")
    P("| program | Mcyc | cyc ratio | ins ratio | cycle spread of readings | note |")
    P("|---|---|---|---|---|---|")
    rows = []
    for k in K:
        cr, ir = ratio(k, c, o), ratio(k, c, o, ins)
        if cr is None or (cyc(k, c) or 0) < minM:
            continue
        v = vals(k, c, "cyc")
        sp = max(v) / min(v) - 1 if len(v) > 1 else 0
        if cond and not cond(cr, ir):
            continue
        rows.append((cr if key == "cyc" else ir, k, cr, ir, sp))
    rows.sort(reverse=rev)
    for _, k, cr, ir, sp in rows[:n]:
        note = ""
        if abs(cr - 1) > 0.03 and abs(ir - 1) <= 0.01:
            note = "instructions equal: layout / IPC"
        elif abs(cr - 1) > 0.03 and (cr - 1) * (ir - 1) > 0 and abs(ir - 1) > 0.01:
            note = "instructions move the same way"
        P(f"| {k} | {cyc(k,c)/1e6:.0f} | {cr:.3f} | {ir:.3f} | {sp*100:.1f}% ({len(vals(k,c,'cyc'))} runs) | {note} |")
    return len(rows)


for nm, M in (("5a064d7", PREV), ("b116447", II)):
    for c in OURS:
        listing(f"{LAB[c]} vs {nm}: 20 biggest improvements by cycles (>= 20 Mcyc)", c, M[c], "cyc", False, 20)
    for c in OURS:
        listing(f"{LAB[c]} vs {nm}: regressions (instructions > +2% or cycles > +3%, >= 20 Mcyc)", c, M[c], "cyc", True, 80, cond=lambda cr, ir: cr > 1.03 or (ir or 0) > 1.02)
# ---- vs LJ per program gaps
for c in ("plain", "pgo"):
    xs = sorted([(ratio(k, c, "lj"), k) for k in K if ratio(k, c, "lj")], reverse=True)
    P(f"\n### {LAB[c]}: 15 biggest gaps to LuaJIT (ours/LJ cycles)\n")
    P("| program | Mcyc | cyc/LJ | ins/LJ |"); P("|---|---|---|---|")
    for r, k in xs[:15]:
        P(f"| {k} | {cyc(k,c)/1e6:.0f} | {r:.3f} | {f3(ratio(k,c,'lj',ins))} |")
    P(f"\n### {LAB[c]}: 15 biggest wins over LuaJIT (>= 20 Mcyc)\n")
    P("| program | Mcyc | cyc/LJ | ins/LJ |"); P("|---|---|---|---|")
    for r, k in sorted([x for x in xs if cyc(x[1], c) >= 20e6])[:15]:
        P(f"| {k} | {cyc(k,c)/1e6:.0f} | {r:.3f} | {f3(ratio(k,c,'lj',ins))} |")

# ---- focus programs
FOC = ["programs/nbody", "programs/fib", "programs/binary_trees", "programs/queens", "apps/raytrace", "awfy/cd", "awfy/list", "awfy/richards", "awfy/deltablue", "awfy/havlak", "awfy/permute", "awfy/towers", "corpus/lua_lexer", "corpus/classic_vector", "corpus2/heldout/dijkstra_routes", "corpus2/dev/oop_mixins", "jit-trace/rec_loop", "corpus2/heldout/roguelike_turns", "corpus2/heldout/vending_fsm", "corpus2/heldout/real_dis_mips", "awfy/queens"]
P("\n## Focus programs (cycles in M; ratios of cycles)\n")
P("| program | plain Mcyc | PGO Mcyc | LJ Mcyc | PUC Mcyc | plain/LJ | PGO/LJ | plain/PUC | PGO/PUC | plain vs b cyc | ins | PGO vs b cyc | ins | plain vs i (b116447) cyc | PGO vs i cyc | PGO/LJ 5a064d7 |")
P("|" + "---|" * 16)
for k in FOC:
    if k not in A:
        continue
    P(f"| {k} | {cyc(k,'plain')/1e6:.1f} | {cyc(k,'pgo')/1e6:.1f} | {(cyc(k,'lj') or 0)/1e6:.1f} | {cyc(k,'puc')/1e6:.1f} | {f3(ratio(k,'plain','lj'))} | {f3(ratio(k,'pgo','lj'))} | {f3(ratio(k,'plain','puc'))} | {f3(ratio(k,'pgo','puc'))} | "
      f"{f3(ratio(k,'plain','b_plain'))} | {f3(ratio(k,'plain','b_plain',ins))} | {f3(ratio(k,'pgo','b_pgo'))} | {f3(ratio(k,'pgo','b_pgo',ins))} | {f3(ratio(k,'plain','i_plain'))} | {f3(ratio(k,'pgo','i_pgo'))} | {f3(ratio(k,'b_pgo','lj'))} |")

# ---- per program
P("\n## Per-program table (5.5 and 5.4; Mcyc, ratios vs 5a064d7 and LJ)\n")
P("| program | n runs (plain) | plain | PGO | p54 | g54 | LJ | PUC55 | PUC54 | plain/LJ | PGO/LJ | plain vs b cyc | ins | PGO vs b cyc | ins | p54 vs b cyc | ins | g54 vs b cyc | ins | out/exp (all 4) |")
P("|" + "---|" * 20)
for k in K:
    e = A[k]["cols"]
    ok = all(e[c].get("out") == "same" and e[c].get("exp") in ("ok", "none", "", None) and not e[c].get("err") for c in OURS)
    mc = lambda c: (lambda v: "-" if v is None else f"{v/1e6:.1f}")(cyc(k, c))
    P(f"| {k} | {len(vals(k,'plain','cyc'))} | {mc('plain')} | {mc('pgo')} | {mc('p54')} | {mc('g54')} | {mc('lj')} | {mc('puc')} | {mc('puc54')} | {f3(ratio(k,'plain','lj'))} | {f3(ratio(k,'pgo','lj'))} | "
      + " | ".join(f"{f3(ratio(k,c,PREV[c]))} | {f3(ratio(k,c,PREV[c],ins))}" for c in OURS) + f" | {'ok' if ok else 'CHECK'} |")

# ---- correctness
P("\n## Output / .expected check\n")
bad = []
for k in K:
    for c in OURS:
        e = A[k]["cols"][c]
        if e.get("out") != "same" or e.get("exp") == "MISMATCH" or e.get("err"):
            bad.append(f"- {k} [{c}]: out={e.get('out')} exp={e.get('exp')} err={e.get('err')}")
for k in RR:
    for c in OURS:
        e = RR[k]["cols"].get(c, {})
        if e and (e.get("out") != "same" or e.get("exp") == "MISMATCH" or e.get("err")):
            bad.append(f"- rerun {k} [{c}]: out={e.get('out')} exp={e.get('exp')} err={e.get('err')}")
P("\n".join(bad) if bad else "All four ours columns equal PUC (same version) output and .expected on all 223 programs (quick, full and rerun readings).")
P("\nLuaJIT: programs without a LuaJIT variant or whose LuaJIT output differs:\n")
for k in K:
    e = A[k]["cols"].get("lj", {})
    if not e or e.get("err") or e.get("out") == "diff":
        P(f"- {k}: {e.get('err') or ('no reading' if not e else 'output differs')}")

open(os.path.join(D, "tables.md"), "w").write("\n".join(L) + "\n")
with open(os.path.join(D, "per_program.csv"), "w") as f:
    cols = ["plain", "pgo", "p54", "g54", "lj", "ljoff", "puc", "puc54", "b_plain", "b_pgo", "b_p54", "b_g54", "i_plain", "i_pgo", "i_p54", "i_g54", "j_plain", "j_pgo", "j_p54", "j_g54", ]
    f.write("program," + ",".join(c + "_cyc_min" for c in cols) + "," + ",".join(c + "_ins_min" for c in cols) + "\n")
    for k in K:
        f.write(k + "," + ",".join(str(cyc(k, c) or "") for c in cols) + "," + ",".join(str(ins(k, c) or "") for c in cols) + "\n")
print("\n".join(L[:30]))
