#!/usr/bin/env python3
"""session-nbody tables. usage: report_nbody.py <outdir> <session-final-dir>  -> tables.md, vs_baseline.md, per_program_pass{A,B,54}.csv
Same-session baseline b1 = 108b935 (the session-final subject), rebuilt with the same scripts. Cross-session: session-final per_program_passB.csv."""
import json, math, os, sys, csv

OUT, SF = sys.argv[1], sys.argv[2]
def ld(f):
    p = os.path.join(OUT, f)
    return json.load(open(p))["results"] if os.path.exists(p) else {}
A, B, C = ld("passA_cpus3-7_all_columns.json"), ld("passB_cpus4-6_fast_lj.json"), ld("pass54_cpus4-6.json")
RR, R54 = ld("rerun55_serial_cpu4_15runs.json"), ld("rerun54_serial_cpu4_15runs.json")
REALDIS = {"real_dis_x86", "real_dis_arm", "real_dis_mips", "real_dis_ppc"}
isrd = lambda k: k.split("/")[-1] in REALDIS

def vals(R, k, col, f): return [x for x in R.get(k, {}).get("cols", {}).get(col, {}).get(f, []) if x]
def cyc(R, k, col):
    v = vals(R, k, col, "cyc"); return min(v) if v else None
def ins(R, k, col):
    v = vals(R, k, col, "ins"); return min(v) if v else None
def gm(xs):
    xs = [x for x in xs if x and x > 0]
    return math.exp(sum(math.log(x) for x in xs) / len(xs)) if xs else float("nan")
def ratio(R, k, a, b, f=cyc):
    x, y = f(R, k, a), f(R, k, b)
    return x / y if x and y else None
f3 = lambda v: "-" if v is None or v != v else f"{v:.3f}"
def G(R, ks, a, b, f=cyc): return gm([ratio(R, k, a, b, f) for k in ks])

# session-final stored (pass B: puc lj plain pgo ...; pass A: ...; pass 54)
def load_csv(name):
    p = os.path.join(SF, name)
    d = {}
    if os.path.exists(p):
        for r in csv.DictReader(open(p)):
            d[r["program"]] = {k: (float(v) if v not in ("", None) and k not in ("program",) else None) for k, v in r.items() if k != "program"}
    return d
SFB, SFA, SF54 = load_csv("per_program_passB.csv"), load_csv("per_program_passA.csv"), load_csv("per_program_pass54.csv")
def sf(D, k, col, f):
    x = D.get(k, {}).get(f"{col}_{f}_min")
    return x if x else None

def pick(R, name):
    if name == "ALL": return list(R)
    if name == "real_dis (4)": return [k for k in R if isrd(k)]
    if name == "real (5)": return [k for k in R if k.startswith("real/")]
    if name == "corpus (old)": return [k for k in R if k.startswith("corpus/")]
    if name == "corpus2 dev": return [k for k in R if k.startswith("corpus2/dev/")]
    if name == "corpus2 heldout": return [k for k in R if k.startswith("corpus2/heldout/")]
    if name == "ALL excl real/real_dis": return [k for k in R if not isrd(k) and not k.startswith("real/")]
    return [k for k in R if k.startswith(name + "/")]
GROUPS = ["apps", "awfy", "corpus (old)", "corpus2 dev", "corpus2 heldout", "jit-trace", "lib", "programs", "real (5)", "real_dis (4)", "ALL excl real/real_dis", "ALL"]

L = []
P = L.append
P("# Tables, session-nbody (HEAD 1a5aad0, rnlua; PGO via scripts/release-build.sh)\n")
P("Ratios below 1 mean the first column is faster. cyc = cycles:u, ins = instructions:u, min of 5 interleaved runs (15 for re-runs). b1 = 108b935 (session-final subject) rebuilt in this session; SF = stored session-final pass-B numbers (same program, different session and script path).\n")

def lj_block(R, title, cols, other="lj"):
    P(f"\n## {title}\n")
    hdr = "| set | n(LJ) | " + " | ".join(f"{l}/LJ cyc" for l, _ in cols) + " | " + " | ".join(f"{l}/LJ ins" for l, _ in cols) + " | " + " | ".join(f"{l}/PUC cyc" for l, _ in cols) + " | " + " | ".join(f"{l}/PUC ins" for l, _ in cols) + " |"
    P(hdr); P("|" + "---|" * (hdr.count("|") - 1))
    for g in GROUPS:
        ks = pick(R, g)
        nl = [k for k in ks if ratio(R, k, cols[0][1], "lj")]
        P(f"| {g} | {len(nl)} | " + " | ".join(f3(G(R, nl, c, "lj")) for _, c in cols) + " | " + " | ".join(f3(G(R, nl, c, "lj", ins)) for _, c in cols) + " | "
          + " | ".join(f3(G(R, ks, c, "puc")) for _, c in cols) + " | " + " | ".join(f3(G(R, ks, c, "puc", ins)) for _, c in cols) + " |")

if B:
    lj_block(B, "C3. 5.5, pass B (CPUs 4,6): ours vs LuaJIT and PUC 5.5 (geomean)", [("plain", "plain"), ("PGO", "pgo"), ("b1 plain", "b1p"), ("b1 PGO", "b1g")])
    # session-final stored comparison, pass B
    P("\n## Same figures in session-final (stored pass B, 5.5; plain = PGO-less 108b935)\n")
    P("| set | n | plain/LJ cyc | PGO/LJ cyc | plain/PUC cyc | PGO/PUC cyc | plain/LJ ins | PGO/LJ ins |"); P("|---|---|---|---|---|---|---|---|")
    def sgm(ks, a, b, f):
        xs = [sf(SFB, k, a, f) / sf(SFB, k, b, f) for k in ks if sf(SFB, k, a, f) and sf(SFB, k, b, f)]
        return gm(xs)
    for g in GROUPS:
        ks = [k for k in pick(B, g) if k in SFB]
        nl = [k for k in ks if sf(SFB, k, "lj", "cyc")]
        P(f"| {g} | {len(nl)} | {f3(sgm(nl,'plain','lj','cyc'))} | {f3(sgm(nl,'pgo','lj','cyc'))} | {f3(sgm(ks,'plain','puc','cyc'))} | {f3(sgm(ks,'pgo','puc','cyc'))} | {f3(sgm(nl,'plain','lj','ins'))} | {f3(sgm(nl,'pgo','lj','ins'))} |")
    # per-program pass B
    mc = lambda R, k, c: (lambda v: "-" if v is None else f"{v / 1e6:.1f}")(cyc(R, k, c))
    P("\n## Per-program, pass B (5.5)\n")
    P("cur = this session, b1 = same-session 108b935, SF = stored session-final. X/LJ cycles and instructions, ours vs same-session b1 (cyc, ins) and vs SF ours/LJ (cyc).\n")
    P("| program | plain Mcyc | PGO Mcyc | LJ Mcyc | PUC Mcyc | plain/LJ cyc | plain/LJ ins | PGO/LJ cyc | PGO/LJ ins | SF plain/LJ cyc | SF PGO/LJ cyc | plain/b1p cyc | plain/b1p ins | PGO/b1g cyc | PGO/b1g ins | plain/SF ins | PGO/SF ins |")
    P("|" + "---|" * 17)
    def sfr(k, col, f):
        x, y = sf(SFB, k, col, f), ins(B, k, col) if f == "ins" else cyc(B, k, col)
        return (y / x) if x and y else None
    def sflj(k, col):
        x, y = sf(SFB, k, col, "cyc"), sf(SFB, k, "lj", "cyc")
        return x / y if x and y else None
    for k in sorted(B):
        P(f"| {k} | {mc(B,k,'plain')} | {mc(B,k,'pgo')} | {mc(B,k,'lj')} | {mc(B,k,'puc')} | {f3(ratio(B,k,'plain','lj'))} | {f3(ratio(B,k,'plain','lj',ins))} | {f3(ratio(B,k,'pgo','lj'))} | {f3(ratio(B,k,'pgo','lj',ins))} | "
          f"{f3(sflj(k,'plain'))} | {f3(sflj(k,'pgo'))} | {f3(ratio(B,k,'plain','b1p'))} | {f3(ratio(B,k,'plain','b1p',ins))} | {f3(ratio(B,k,'pgo','b1g'))} | {f3(ratio(B,k,'pgo','b1g',ins))} | {f3(sfr(k,'plain','ins'))} | {f3(sfr(k,'pgo','ins'))} |")
    # gaps
    def gaps(label, col, other="lj", n=12):
        xs = sorted([(ratio(B, k, col, other), k) for k in B if ratio(B, k, col, other)], reverse=True)
        P(f"\n### Top-{n} gaps to {other.upper()}, {label} (ours/{other.upper()} cycles)\n"); P("| program | cyc | ins | ours Mcyc | SF cyc |"); P("|---|---|---|---|---|")
        for r, k in xs[:n]: P(f"| {k} | {r:.3f} | {f3(ratio(B,k,col,other,ins))} | {cyc(B,k,col)/1e6:.0f} | {f3(sflj(k,col) if other=='lj' else None)} |")
        P(f"\n### Top-{n} wins vs {other.upper()}, {label} (programs >= 20 Mcyc)\n"); P("| program | cyc | ins | ours Mcyc | SF cyc |"); P("|---|---|---|---|---|")
        for r, k in sorted([x for x in xs if cyc(B, x[1], col) >= 20e6])[:n]: P(f"| {k} | {r:.3f} | {f3(ratio(B,k,col,other,ins))} | {cyc(B,k,col)/1e6:.0f} | {f3(sflj(k,col) if other=='lj' else None)} |")
    gaps("plain", "plain"); gaps("PGO", "pgo")

if A:
    KEYS = sorted(A)
    P("\n## C2. Pass A (CPUs 3-7): plain vs PGO, JIT gain, vs b1 (geomeans)\n")
    P("| set | n | PGO/plain cyc | PGO/plain ins | plain/dup | plain/int (JIT-off) | plain/nj | plain/LJoff | plain/PUC | PGO/PUC | plain/b1p cyc | ins | PGO/b1g cyc | ins |"); P("|" + "---|" * 14)
    for g in GROUPS:
        ks = pick(A, g)
        nl = [k for k in ks if ratio(A, k, "plain", "ljoff")]
        P(f"| {g} | {len(ks)} | {f3(G(A,ks,'pgo','plain'))} | {f3(G(A,ks,'pgo','plain',ins))} | {f3(G(A,ks,'plain','plain2'))} | {f3(G(A,ks,'plain','int'))} | {f3(G(A,ks,'plain','nj'))} | {f3(G(A,nl,'plain','ljoff'))} | {f3(G(A,ks,'plain','puc'))} | {f3(G(A,ks,'pgo','puc'))} | "
          f"{f3(G(A,ks,'plain','b1p'))} | {f3(G(A,ks,'plain','b1p',ins))} | {f3(G(A,ks,'pgo','b1g'))} | {f3(G(A,ks,'pgo','b1g',ins))} |")
    nl = [k for k in A if ratio(A, k, "plain", "ljoff")]
    P(f"\nvs LuaJIT -joff ({len(nl)} programs): plain {G(A,nl,'plain','ljoff'):.3f}, PGO {G(A,nl,'pgo','ljoff'):.3f}, JIT-off interpreter {G(A,nl,'int','ljoff'):.3f}, LuaJIT (JIT on) {G(A,nl,'lj','ljoff'):.3f}")
    P("\n## Per-program, pass A (Mcyc min; ratios cyc/ins vs same-session b1)\n")
    P("| program | plain | pgo | int | nj | PUC | LJ | LJoff | plain/b1p cyc | ins | PGO/b1g cyc | ins | dup | PGO/plain cyc |"); P("|" + "---|" * 14)
    for k in KEYS:
        P(f"| {k} | {mc(A,k,'plain')} | {mc(A,k,'pgo')} | {mc(A,k,'int')} | {mc(A,k,'nj')} | {mc(A,k,'puc')} | {mc(A,k,'lj')} | {mc(A,k,'ljoff')} | {f3(ratio(A,k,'plain','b1p'))} | {f3(ratio(A,k,'plain','b1p',ins))} | {f3(ratio(A,k,'pgo','b1g'))} | {f3(ratio(A,k,'pgo','b1g',ins))} | {f3(ratio(A,k,'plain','plain2'))} | {f3(ratio(A,k,'pgo','plain'))} |")
    P("\nDup noise above 3% (pass A):\n")
    for k in KEYS:
        d = ratio(A, k, "plain", "plain2")
        if d and abs(d - 1) > 0.03: P(f"- {k}: {d:.3f} (plain {cyc(A,k,'plain')/1e6:.0f} Mcyc)")

# ---------------- vs baseline (session-final)
V = []
Q = V.append
Q("# vs session-final (108b935): same-session rebuild (b1) and stored session-final numbers\n")
Q("Subject = HEAD 1a5aad0. b1 = 108b935 rebuilt with scripts/release-build.sh in this session (pass A, 5-run min; 15-run serial re-run values where available). SF = stored session-final pass A (script path differs, so instruction counts differ slightly).\n")
def agg(R, a, b, label, f):
    xs = [(r, k) for k in R if (r := ratio(R, k, a, b, f))]
    return xs
for lab, a, b, sfa in (("plain", "plain", "b1p", "plain"), ("PGO", "pgo", "b1g", "pgo")):
    cs = agg(A, a, b, lab, cyc); ii = agg(A, a, b, lab, ins)
    Q(f"- {lab}/b1 geomean over {len(cs)} programs: cycles {gm([x for x,_ in cs]):.3f}, instructions {gm([x for x,_ in ii]):.3f}; cycles >3% faster {sum(1 for x,_ in cs if x<0.97)}, within 3% {sum(1 for x,_ in cs if 0.97<=x<=1.03)}, >3% slower {sum(1 for x,_ in cs if x>1.03)}; instructions >3% fewer {sum(1 for x,_ in ii if x<0.97)}, >3% more {sum(1 for x,_ in ii if x>1.03)}")
    xs = []
    for k in A:
        x, y = sf(SFA, k, sfa, "ins"), ins(A, k, a)
        c, d = sf(SFA, k, sfa, "cyc"), cyc(A, k, a)
        if x and y: xs.append((y / x, (d / c) if c and d else None, k))
    Q(f"- {lab} vs stored SF pass A: instructions geomean {gm([r for r,_,_ in xs]):.3f}, cycles geomean {gm([c for _,c,_ in xs if c]):.3f} over {len(xs)} programs")
Q("")
sets = {}
for k in A:
    s = "real_dis" if isrd(k) else ("/".join(k.split("/")[:2]) if k.startswith("corpus2") else k.split("/")[0])
    sets.setdefault(s, []).append(k)
Q("## Per set, geomean subject/b1 (pass A)\n"); Q("| set | n | plain cyc | plain ins | PGO cyc | PGO ins |"); Q("|---|---|---|---|---|---|")
for s, ks in sorted(sets.items()): Q(f"| {s} | {len(ks)} | {f3(G(A,ks,'plain','b1p'))} | {f3(G(A,ks,'plain','b1p',ins))} | {f3(G(A,ks,'pgo','b1g'))} | {f3(G(A,ks,'pgo','b1g',ins))} |")
def rv(k, a, b, f):
    if k in RR and ratio(RR, k, a, b, f): return ratio(RR, k, a, b, f)
    return ratio(A, k, a, b, f)
for lab, a, b, sfa in (("plain", "plain", "b1p", "plain"), ("PGO", "pgo", "b1g", "pgo")):
    rows = []
    for k in sorted(A):
        c, i = ratio(A, k, a, b, cyc), ratio(A, k, a, b, ins)
        if c is None: continue
        c15, i15 = (ratio(RR, k, a, b, cyc), ratio(RR, k, a, b, ins)) if k in RR else (None, None)
        si = (ins(A, k, a) / sf(SFA, k, sfa, "ins")) if sf(SFA, k, sfa, "ins") and ins(A, k, a) else None
        rows.append((k, c, i, c15, i15, si, cyc(A, k, a) / 1e6))
    H = "| program | Mcyc | cyc A | ins A | cyc 15-run | ins 15-run | ins vs stored SF | note |\n|---|---|---|---|---|---|---|---|"
    def row(x):
        k, c, i, c15, i15, si, m = x
        use = c15 if c15 else c; ui = i15 if i15 else i
        tag = ""
        if abs(use - 1) > 0.03 and abs(ui - 1) <= 0.01: tag = "layout lottery (instructions equal)"
        elif abs(use - 1) > 0.03 and abs(ui - 1) > 0.01 and (ui - 1) * (use - 1) > 0: tag = "real (instructions move the same way)"
        return f"| {k} | {m:.0f} | {f3(c)} | {f3(i)} | {f3(c15)} | {f3(i15)} | {f3(si)} | {tag} |"
    Q(f"\n## {lab}/b1: 15 biggest improvements by cycles (programs >= 20 Mcyc)\n"); Q(H)
    for x in sorted([x for x in rows if x[6] >= 20], key=lambda x: x[1])[:15]: Q(row(x))
    Q(f"\n## {lab}/b1: 15 biggest improvements by instructions (programs >= 20 Mcyc)\n"); Q(H)
    for x in sorted([x for x in rows if x[6] >= 20], key=lambda x: x[2])[:15]: Q(row(x))
    Q(f"\n## {lab}/b1: programs with instructions >3% above b1 (same session) or above stored SF\n"); Q(H)
    for x in sorted([x for x in rows if x[2] > 1.03 or (x[5] and x[5] > 1.03)], key=lambda x: -x[2]): Q(row(x))
    Q(f"\n## {lab}/b1: programs with cycles >3% above b1 in pass A\n"); Q(H)
    for x in sorted([x for x in rows if x[1] > 1.03], key=lambda x: -x[1]): Q(row(x))
    # the same by stored SF cycle
    Q(f"\n## {lab} vs stored SF (cross-session): 10 largest instruction increases / decreases (programs >= 20 Mcyc)\n"); Q("| program | Mcyc | ins ratio vs SF | cyc ratio vs SF |"); Q("|---|---|---|---|")
    xs = []
    for k in A:
        x, y = sf(SFA, k, sfa, "ins"), ins(A, k, a); c, d = sf(SFA, k, sfa, "cyc"), cyc(A, k, a)
        if x and y and d and d >= 20e6: xs.append((y / x, d / c if c else None, k, d / 1e6))
    xs.sort()
    for r, c, k, m in xs[-10:][::-1]: Q(f"| {k} | {m:.0f} | {r:.3f} | {f3(c)} |")
    Q("| ... | | | |")
    for r, c, k, m in xs[:10]: Q(f"| {k} | {m:.0f} | {r:.3f} | {f3(c)} |")

# nbody focus
Q("\n# nbody focus (pass A 5-run min, CPUs 3-7; 15-run re-run where available)\n")
Q("| program | build | Mcyc | Mins | cyc/LJ | ins/LJ | cyc/PUC | cyc vs b1 | ins vs b1 | SF Mcyc | SF cyc/LJ(passB) |"); Q("|---|---|---|---|---|---|---|---|---|---|---|")
for k in ("programs/nbody", "awfy/nbody"):
    for lab, c, b, sfc in (("plain", "plain", "b1p", "plain"), ("PGO", "pgo", "b1g", "pgo"), ("b1 plain", "b1p", None, "plain"), ("b1 PGO", "b1g", None, "pgo")):
        R = RR if k in RR else A
        Q(f"| {k} | {lab} | {cyc(R,k,c)/1e6:.1f} | {ins(R,k,c)/1e6:.0f} | {f3(ratio(R,k,c,'lj'))} | {f3(ratio(R,k,c,'lj',ins))} | {f3(ratio(R,k,c,'puc'))} | {f3(ratio(R,k,c,b) if b else None)} | {f3(ratio(R,k,c,b,ins) if b else None)} | {sf(SFA,k,sfc,'cyc')/1e6:.1f} | {f3(sf(SFB,k,sfc,'cyc')/sf(SFB,k,'lj','cyc'))} |")
open(os.path.join(OUT, "vs_baseline.md"), "w").write("\n".join(V) + "\n")

# ---------------- pass C (5.4)
if C:
    P("\n# C4. Lua 5.4 builds, pass C (CPUs 4,6)\n")
    ok54 = [k for k in C if cyc(C, k, "puc") and cyc(C, k, "p54")]
    Cok = {k: C[k] for k in ok54}
    P(f"Programs both PUC 5.4 and p54 run: {len(ok54)} of {len(C)}.\n")
    P("| set | n | p54/PUC54 cyc | g54/PUC54 cyc | p54/PUC54 ins | n(LJ) | p54/LJ cyc | g54/LJ cyc | p55/PUC55 cyc | g55/PUC55 cyc | p54/i54 (JIT gain) | p54/p55 cyc | p54/p55 ins | g54/g55 cyc | (p54/PUC54)/(p55/PUC55) |"); P("|" + "---|" * 15)
    for g in GROUPS:
        ks = pick(Cok, g)
        nl = [k for k in ks if ratio(Cok, k, "p54", "lj")]
        a54, a55 = G(Cok, ks, "p54", "puc"), G(Cok, ks, "p55", "ref55")
        P(f"| {g} | {len(ks)} | {f3(a54)} | {f3(G(Cok,ks,'g54','puc'))} | {f3(G(Cok,ks,'p54','puc',ins))} | {len(nl)} | {f3(G(Cok,nl,'p54','lj'))} | {f3(G(Cok,nl,'g54','lj'))} | {f3(a55)} | {f3(G(Cok,ks,'g55','ref55'))} | {f3(G(Cok,ks,'p54','i54'))} | {f3(G(Cok,ks,'p54','p55'))} | {f3(G(Cok,ks,'p54','p55',ins))} | {f3(G(Cok,ks,'g54','g55'))} | {f3(a54/a55)} |")
    sfk = [k for k in ok54 if k in SF54]
    def sg(a, b, f):
        return gm([sf(SF54, k, a, f) / sf(SF54, k, b, f) for k in sfk if sf(SF54, k, a, f) and sf(SF54, k, b, f)])
    P(f"\nSession-final stored pass C, same programs ({len(sfk)}): p54/PUC54 {sg('p54','puc','cyc'):.3f}, g54/PUC54 {sg('g54','puc','cyc'):.3f}, p54/LJ {sg('p54','lj','cyc'):.3f}, g54/LJ {sg('g54','lj','cyc'):.3f}; this session p54/LJ {G(Cok,[k for k in ok54 if ratio(Cok,k,'p54','lj')],'p54','lj'):.3f}, g54/LJ {G(Cok,[k for k in ok54 if ratio(Cok,k,'g54','lj')],'g54','lj'):.3f}")
    xs = []
    for k in ok54:
        a, b = ratio(Cok, k, "p54", "puc"), ratio(Cok, k, "p55", "ref55")
        if a and b and (cyc(Cok, k, "p54") or 0) >= 20e6: xs.append((a / b, k, a, b))
    P("\n## Programs where 5.4 is relatively slower than 5.5 (rel = (p54/PUC54)/(p55/PUC55), >= 20 Mcyc): top 20\n")
    P("| program | rel | p54/PUC54 | p55/PUC55 | p54 Mcyc |"); P("|---|---|---|---|---|")
    for rel, k, a, b in sorted(xs, reverse=True)[:20]: P(f"| {k} | {rel:.3f} | {a:.3f} | {b:.3f} | {cyc(Cok,k,'p54')/1e6:.0f} |")
    P("\n## Per-program (pass C)\n")
    P("| program | p54 Mcyc | g54 | i54 | n54 | PUC54 | PUC55 | LJ | p55 | g55 | p54/PUC54 | g54/PUC54 | p54/LJ | p55/PUC55 | p54/p55 ins | out p54 | out g54 | exp p54 |"); P("|" + "---|" * 18)
    for k in sorted(C):
        e = C[k]["cols"]
        P(f"| {k} | {mc(C,k,'p54')} | {mc(C,k,'g54')} | {mc(C,k,'i54')} | {mc(C,k,'n54')} | {mc(C,k,'puc')} | {mc(C,k,'ref55')} | {mc(C,k,'lj')} | {mc(C,k,'p55')} | {mc(C,k,'g55')} | "
          f"{f3(ratio(C,k,'p54','puc'))} | {f3(ratio(C,k,'g54','puc'))} | {f3(ratio(C,k,'p54','lj'))} | {f3(ratio(C,k,'p55','ref55'))} | {f3(ratio(C,k,'p54','p55',ins))} | {e.get('p54',{}).get('out','')} | {e.get('g54',{}).get('out','')} | {e.get('p54',{}).get('exp','')} |")
    P("\n## 5.4 output / error check (reference = PUC 5.4 output)\n")
    nb = 0
    for k in sorted(C):
        e = C[k]["cols"]
        for c in ("p54", "g54", "i54", "n54"):
            x = e.get(c, {})
            if x.get("err") or x.get("out") == "diff":
                nb += 1; P(f"- {k} [{c}]: out={x.get('out')} err={x.get('err')}  (PUC 5.4: err={e.get('puc',{}).get('err')!r})")
    if not nb: P("none: p54, g54, i54, n54 all equal PUC 5.4 output on every program")
    P("\nPUC 5.4 itself cannot run:")
    for k in sorted(C):
        x = C[k]["cols"].get("puc", {})
        if x.get("err"): P(f"- {k}: {x['err']}")

if R54:
    P("\n## 5.4 15-run serial CPU-4 re-measure (p54b = dup of p54)\n")
    P("| program | p54 Mcyc | PUC54 Mcyc | p54/PUC54 cyc | ins | g54/PUC54 cyc | g54/p54 cyc | p54/p54b (dup) | pass C p54/PUC54 |"); P("|---|---|---|---|---|---|---|---|---|")
    for k in sorted(R54): P(f"| {k} | {cyc(R54,k,'p54')/1e6:.0f} | {cyc(R54,k,'puc')/1e6:.0f} | {f3(ratio(R54,k,'p54','puc'))} | {f3(ratio(R54,k,'p54','puc',ins))} | {f3(ratio(R54,k,'g54','puc'))} | {f3(ratio(R54,k,'g54','p54'))} | {f3(ratio(R54,k,'p54','p54b'))} | {f3(ratio(C,k,'p54','puc'))} |")
if RR:
    P("\n## 5.5 15-run serial CPU-4 re-measure of re-run programs\n")
    P("| program | plain Mcyc | PGO Mcyc | b1p | b1g | PGO/plain cyc | plain/b1p cyc | ins | PGO/b1g cyc | ins | dup |"); P("|" + "---|" * 11)
    for k in sorted(RR):
        P(f"| {k} | {cyc(RR,k,'plain')/1e6:.0f} | {cyc(RR,k,'pgo')/1e6:.0f} | {cyc(RR,k,'b1p')/1e6:.0f} | {cyc(RR,k,'b1g')/1e6:.0f} | {f3(ratio(RR,k,'pgo','plain'))} | {f3(ratio(RR,k,'plain','b1p'))} | {f3(ratio(RR,k,'plain','b1p',ins))} | {f3(ratio(RR,k,'pgo','b1g'))} | {f3(ratio(RR,k,'pgo','b1g',ins))} | {f3(ratio(RR,k,'plain','plain2'))} |")

P("\n## 5.5 output / .expected check (passes A and B)\n")
bad = []
for R, tag in ((A, "A"), (B, "B")):
    for k in sorted(R):
        for c, e in R[k]["cols"].items():
            if c not in ("lj", "ljoff", "puc"):
                if e.get("exp") == "MISMATCH" or e.get("out") == "diff" or e.get("err"):
                    bad.append(f"- pass {tag} {k} [{c}]: exp={e.get('exp')} out={e.get('out')} err={e.get('err')}")
P("\n".join(bad) if bad else "All ours columns equal PUC 5.5 and .expected on every program, passes A and B.")
if A:
    P("\nLuaJIT problems / no LuaJIT variant (pass A):\n")
    for k in sorted(A):
        e = A[k]["cols"].get("lj", {})
        if e.get("err") or e.get("out") == "diff": P(f"- {k}: {e.get('err') or 'output differs from PUC'}")
open(os.path.join(OUT, "tables.md"), "w").write("\n".join(L) + "\n")
cn = ["puc", "lj", "ljoff", "plain", "plain2", "int", "nj", "pgo", "b1p", "b1g"]
for R, name, cols in ((A, "per_program_passA.csv", cn), (B, "per_program_passB.csv", ["puc", "lj", "plain", "pgo", "b1p", "b1g"]),
                      (C, "per_program_pass54.csv", ["puc", "ref55", "lj", "p54", "g54", "i54", "n54", "p55", "g55"])):
    if not R: continue
    with open(os.path.join(OUT, name), "w") as f:
        f.write("program,core," + ",".join(c + "_cyc_min" for c in cols) + "," + ",".join(c + "_ins_min" for c in cols) + "\n")
        for k in sorted(R):
            f.write(f"{k},{R[k]['cpu']}," + ",".join(str(cyc(R, k, c) or "") for c in cols) + "," + ",".join(str(ins(R, k, c) or "") for c in cols) + "\n")
print("\n".join(L[:40])); print("\n".join(V[:40]))
