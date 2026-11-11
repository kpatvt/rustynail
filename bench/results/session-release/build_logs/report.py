#!/usr/bin/env python3
"""session-release tables. usage: report.py <outdir>   (reads passA/passB/pass54 and optional rerun JSONs in outdir)
Columns pass A: puc lj plain plain2(dup) int nj pgo b3plain b3pgo ; pass B: puc lj plain pgo b3plain b3pgo
pass C (5.4): puc(=PUC 5.4) ref55(=PUC 5.5) lj p54 g54 i54 n54 p55 g55."""
import json, math, os, sys

OUT = sys.argv[1]
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
def G(R, ks, a, b, f=cyc):
    xs = [ratio(R, k, a, b, f) for k in ks]
    return gm(xs)


def pick(R, name):
    if name == "ALL": return list(R)
    if name == "real_dis (4)": return [k for k in R if isrd(k)]
    if name == "real (5)": return [k for k in R if k.startswith("real/")]
    if name == "corpus (old)": return [k for k in R if k.startswith("corpus/")]
    if name == "corpus2 dev": return [k for k in R if k.startswith("corpus2/dev/")]
    if name == "corpus2 dev excl real_dis": return [k for k in R if k.startswith("corpus2/dev/") and not isrd(k)]
    if name == "corpus2 heldout": return [k for k in R if k.startswith("corpus2/heldout/")]
    if name == "corpus2 heldout excl real_dis": return [k for k in R if k.startswith("corpus2/heldout/") and not isrd(k)]
    if name == "ALL excl real/real_dis": return [k for k in R if not isrd(k) and not k.startswith("real/")]
    return [k for k in R if k.startswith(name + "/")]
GROUPS = ["apps", "awfy", "corpus (old)", "corpus2 dev", "corpus2 dev excl real_dis", "corpus2 heldout", "corpus2 heldout excl real_dis",
          "jit-trace", "lib", "programs", "real (5)", "real_dis (4)", "ALL excl real/real_dis", "ALL"]

L = []
P = L.append
P("# Tables, session-release (HEAD 8f422153, rnlua; PGO via scripts/release-build.sh --pgo)\n")
P("Ratios below 1 mean the first column is faster. cyc = cycles:u, ins = instructions:u, min of 5 interleaved runs (15 serial runs for the re-runs). "
  "real (5) and real_dis (4) are reported separately; corpus2 dev/heldout rows include their real_dis ports unless marked excl.\n")

# ---------------- pass A
if A:
    KEYS = sorted(A)
    P("\n## C2. PGO A/B, pass A (5.5; CPUs 3-7; geomean over programs)\n")
    P("plain = release build (JIT on); PGO = release-build.sh --pgo; dup = plain run twice (noise); int = plain with RUSTYNAIL_JIT=off; nj = build without the jit feature; "
      "b3 = bm3 main 1e6ec4ca rebuilt with --features jit (plain / PGO).\n")
    P("| set | n | PGO/plain cyc | PGO/plain ins | plain/dup | plain/int | plain/nj | plain/PUC cyc | PGO/PUC cyc | plain/b3plain cyc | ins | PGO/b3pgo cyc | ins | PGO-training-set |")
    P("|---|---|---|---|---|---|---|---|---|---|---|---|---|---|")
    trained = {"apps", "awfy", "jit-trace", "lib", "programs", "real (5)"}
    for g in GROUPS:
        ks = pick(A, g)
        P(f"| {g} | {len(ks)} | {f3(G(A,ks,'pgo','plain'))} | {f3(G(A,ks,'pgo','plain',ins))} | {f3(G(A,ks,'plain','plain2'))} | {f3(G(A,ks,'plain','int'))} | "
          f"{f3(G(A,ks,'plain','nj'))} | {f3(G(A,ks,'plain','puc'))} | {f3(G(A,ks,'pgo','puc'))} | {f3(G(A,ks,'plain','b3plain'))} | {f3(G(A,ks,'plain','b3plain',ins))} | "
          f"{f3(G(A,ks,'pgo','b3pgo'))} | {f3(G(A,ks,'pgo','b3pgo',ins))} | {'trained' if g in trained else ('held out' if g.startswith('corpus') else '-')} |")
    P("\n(PGO training = scripts/train.sh: awfy, apps, programs, lib, jit-trace, real + bench/train, bench/micro, tests/vm, part of the official suite. corpus and corpus2 are NOT in the training set, they are hold-outs.)\n")

    def per_prog(title, ks, R=A):
        P(f"\n### {title}\n")
        P("Mcyc min. pgo/plain = PGO cycles / plain cycles; ip = same for instructions; dup = plain/plain2; b3 = plain / bm3 plain cycles; b3g = PGO / bm3 PGO.\n")
        P("| program | plain | pgo | interp | nonjit | PUC | LJ | pgo/plain | ip | dup | plain/PUC | plain/int | b3 | b3g | core |")
        P("|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|")
        mc = lambda k, c: (lambda v: "-" if v is None else f"{v / 1e6:.0f}")(cyc(R, k, c))
        for k in ks:
            P(f"| {k} | {mc(k,'plain')} | {mc(k,'pgo')} | {mc(k,'int')} | {mc(k,'nj')} | {mc(k,'puc')} | {mc(k,'lj')} | {f3(ratio(R,k,'pgo','plain'))} | {f3(ratio(R,k,'pgo','plain',ins))} | "
              f"{f3(ratio(R,k,'plain','plain2'))} | {f3(ratio(R,k,'plain','puc'))} | {f3(ratio(R,k,'plain','int'))} | {f3(ratio(R,k,'plain','b3plain'))} | {f3(ratio(R,k,'pgo','b3pgo'))} | {R[k]['cpu']} |")
    P("\n# Per-program tables (pass A, all programs)")
    per_prog("real (bench/real), reported separately", pick(A, "real (5)"))
    per_prog("real_dis_* ports, reported separately", sorted(pick(A, "real_dis (4)")))
    for g in ["corpus", "corpus2/dev", "corpus2/heldout", "awfy", "programs", "jit-trace", "apps", "lib"]:
        per_prog(g, [k for k in KEYS if k.startswith(g + "/") and not isrd(k)])

    # PGO wins/regressions
    rows = []
    for k in KEYS:
        r = ratio(A, k, "pgo", "plain")
        if r:
            rows.append(dict(k=k, r=r, ri=ratio(A, k, "pgo", "plain", ins), mc=cyc(A, k, "plain") / 1e6, dup=ratio(A, k, "plain", "plain2")))
    def rr(c):
        inr = c["k"] in RR
        return (f"| {c['k']} | {c['r']:.3f} | {f3(c['ri'])} | {c['mc']:.0f} | {f3(c['dup'])} | "
                f"{f3(ratio(RR,c['k'],'pgo','plain')) if inr else '-'} | {f3(ratio(RR,c['k'],'pgo','plain',ins)) if inr else '-'} | {f3(ratio(RR,c['k'],'plain','plain2')) if inr else '-'} | {f3(ratio(RR,c['k'],'plain','b3plain')) if inr else '-'} | {f3(ratio(RR,c['k'],'pgo','b3pgo')) if inr else '-'} | {f3(ratio(RR,c['k'],'plain','puc')) if inr else '-'} |")
    H = ["| program | PGO/plain cyc (A) | ins | plain Mcyc | dup | 15-run PGO/plain cyc | 15-run ins | 15-run dup | 15-run plain/b3 | 15-run PGO/b3g | 15-run plain/PUC |", "|---|---|---|---|---|---|---|---|---|---|---|"]
    P("\n## PGO regressions: programs where PGO is >3% worse than plain (cycles, pass A), with the 15-run serial CPU-4 re-measure\n")
    bad = sorted([c for c in rows if c["r"] > 1.03], key=lambda c: -c["r"])
    P("\n".join(H)); [P(rr(c)) for c in bad]
    P(f"\n{len(bad)} programs. Layout-lottery marker: instruction ratio within 1% of 1.000 and dup near 1.000.\n")
    P("\n## Top-15 PGO wins (pass A, programs >= 30 Mcyc)\n"); P("\n".join(H))
    for c in sorted([c for c in rows if c["mc"] >= 30], key=lambda c: c["r"])[:15]: P(rr(c))
    P("\n## Top-15 PGO regressions (pass A, programs >= 30 Mcyc)\n"); P("\n".join(H))
    for c in sorted([c for c in rows if c["mc"] >= 30], key=lambda c: -c["r"])[:15]: P(rr(c))
    with open(os.path.join(OUT, "rerun_programs.txt"), "w") as f:
        f.write(",".join(c["k"] for c in bad))
    # distribution
    rs = sorted(c["r"] for c in rows)
    P(f"\n## Distribution of PGO/plain cycles over {len(rs)} programs\n")
    P(f"min {rs[0]:.3f}, p10 {rs[len(rs)//10]:.3f}, median {rs[len(rs)//2]:.3f}, p90 {rs[9*len(rs)//10]:.3f}, max {rs[-1]:.3f}; "
      f"faster by >3%: {sum(1 for r in rs if r < 0.97)}, within 3%: {sum(1 for r in rs if 0.97 <= r <= 1.03)}, slower by >3%: {sum(1 for r in rs if r > 1.03)}")
    P("\n## Trend vs bm3 main 1e6ec4ca (same session, pass A): programs more than 3% slower than b3 (plain or PGO cycles, >= 30 Mcyc), and dup noise >3%\n")
    for k in KEYS:
        a, b = ratio(A, k, "plain", "b3plain"), ratio(A, k, "pgo", "b3pgo")
        if (a and a > 1.03 or b and b > 1.03) and (cyc(A, k, "plain") or 0) >= 30e6:
            P(f"- {k}: plain/b3 {f3(a)} (ins {f3(ratio(A,k,'plain','b3plain',ins))}), PGO/b3g {f3(b)} (ins {f3(ratio(A,k,'pgo','b3pgo',ins))}), dup {f3(ratio(A,k,'plain','plain2'))}")
    P("\nDup noise above 3% (pass A):\n")
    for k in KEYS:
        d = ratio(A, k, "plain", "plain2")
        if d and abs(d - 1) > 0.03: P(f"- {k}: {d:.3f} (plain {cyc(A,k,'plain')/1e6:.0f} Mcyc)")

# ---------------- pass B
def lj_tables(R, title, cols, puc="puc", extra=None):
    """cols: list of (label, col). Prints ours/LJ and ours/PUC per set and top gaps/wins for the first two columns."""
    P(f"\n## {title}\n")
    hdr = "| set | n(LJ) | " + " | ".join(f"{l}/LJ cyc" for l, _ in cols) + " | " + " | ".join(f"{l}/LJ ins" for l, _ in cols[:2]) + " | " + " | ".join(f"{l}/PUC cyc" for l, _ in cols) + " | " + " | ".join(f"{l}/PUC ins" for l, _ in cols[:2]) + " |"
    P(hdr); P("|" + "---|" * (hdr.count("|") - 1))
    for g in GROUPS:
        ks = pick(R, g)
        nl = [k for k in ks if ratio(R, k, cols[0][1], "lj")]
        P(f"| {g} | {len(nl)} | " + " | ".join(f3(G(R, nl, c, "lj")) for _, c in cols) + " | " + " | ".join(f3(G(R, nl, c, "lj", ins)) for _, c in cols[:2]) + " | "
          + " | ".join(f3(G(R, ks, c, puc)) for _, c in cols) + " | " + " | ".join(f3(G(R, ks, c, puc, ins)) for _, c in cols[:2]) + " |")

def gaps(R, label, col, other="lj", n=15, otherlabel="LJ"):
    xs = sorted([(ratio(R, k, col, other), k) for k in R if ratio(R, k, col, other)], reverse=True)
    P(f"\n### Top-{n} gaps to {otherlabel}, {label} (highest ours/{otherlabel} cycles)\n")
    P(f"| program | {label}/{otherlabel} cyc | {label}/{otherlabel} ins | {label} Mcyc | {otherlabel} Mcyc |"); P("|---|---|---|---|---|")
    for r, k in xs[:n]: P(f"| {k} | {r:.3f} | {f3(ratio(R,k,col,other,ins))} | {cyc(R,k,col)/1e6:.0f} | {cyc(R,k,other)/1e6:.0f} |")
    P(f"\n### Top-{n} wins vs {otherlabel}, {label} (lowest ours/{otherlabel} cycles, programs >= 20 Mcyc ours)\n")
    P(f"| program | {label}/{otherlabel} cyc | {label}/{otherlabel} ins | {label} Mcyc | {otherlabel} Mcyc |"); P("|---|---|---|---|---|")
    for r, k in sorted([x for x in xs if cyc(R, x[1], col) >= 20e6])[:n]: P(f"| {k} | {r:.3f} | {f3(ratio(R,k,col,other,ins))} | {cyc(R,k,col)/1e6:.0f} | {cyc(R,k,other)/1e6:.0f} |")
    P(f"\n### Biggest absolute gaps, {label} vs {otherlabel} (Mcyc)\n"); P("| program | ours | other | diff | ratio |"); P("|---|---|---|---|---|")
    for d, k in sorted([(cyc(R, k, col) - cyc(R, k, other), k) for k in R if ratio(R, k, col, other)], reverse=True)[:n]:
        P(f"| {k} | {cyc(R,k,col)/1e6:.0f} | {cyc(R,k,other)/1e6:.0f} | {d/1e6:.0f} | {ratio(R,k,col,other):.3f} |")

if B:
    lj_tables(B, "C3. Release benchmark 5.5, pass B (CPUs 4,6): ours vs LuaJIT and vs PUC 5.5 (geomean over programs; LJ columns over programs with a LuaJIT variant)",
              [("plain", "plain"), ("PGO", "pgo"), ("b3 plain", "b3plain"), ("b3 PGO", "b3pgo")])
    gaps(B, "plain", "plain"); gaps(B, "PGO", "pgo")
    gaps(B, "plain", "plain", "puc", otherlabel="PUC"); gaps(B, "PGO", "pgo", "puc", otherlabel="PUC")
    P("\n### Per-program (pass B)\n")
    P("| program | plain Mcyc | PGO Mcyc | LJ Mcyc | PUC Mcyc | plain/LJ | PGO/LJ | plain/PUC | PGO/PUC | plain/LJ ins | PGO/LJ ins | b3plain/LJ | b3pgo/LJ |"); P("|---|---|---|---|---|---|---|---|---|---|---|---|---|")
    mc = lambda R, k, c: (lambda v: "-" if v is None else f"{v / 1e6:.0f}")(cyc(R, k, c))
    for k in sorted(B):
        P(f"| {k} | {mc(B,k,'plain')} | {mc(B,k,'pgo')} | {mc(B,k,'lj')} | {mc(B,k,'puc')} | {f3(ratio(B,k,'plain','lj'))} | {f3(ratio(B,k,'pgo','lj'))} | {f3(ratio(B,k,'plain','puc'))} | {f3(ratio(B,k,'pgo','puc'))} | "
          f"{f3(ratio(B,k,'plain','lj',ins))} | {f3(ratio(B,k,'pgo','lj',ins))} | {f3(ratio(B,k,'b3plain','lj'))} | {f3(ratio(B,k,'b3pgo','lj'))} |")

# ---------------- pass C (5.4)
if C:
    P("\n# C4. Lua 5.4 builds, pass C (CPUs 4,6; all programs)\n")
    P("puc = PUC 5.4.9 (ref/54/build-seed0), ref55 = PUC 5.5.1, p54/g54 = our 5.4 plain/PGO (JIT on), i54 = p54 with RUSTYNAIL_JIT=off, n54 = 5.4 build without jit, p55/g55 = our 5.5 plain/PGO.\n")
    # runnable sets
    def okc(k, c): return bool(cyc(C, k, c))
    ok54 = [k for k in C if okc(k, "puc") and okc(k, "p54")]
    P(f"Programs: {len(C)}; runnable by PUC 5.4: {sum(1 for k in C if okc(k,'puc'))}; by our p54: {sum(1 for k in C if okc(k,'p54'))}; by both: {len(ok54)}. All ratios below are over the {len(ok54)} programs that both PUC 5.4 and p54 run (and p55/ref55 on the same programs).\n")
    Cok = {k: C[k] for k in ok54}
    def pickC(name): return [k for k in pick(Cok, name)]
    P("\n## Per set: our 5.4 build vs PUC 5.4 and LuaJIT, and vs our 5.5 build vs PUC 5.5\n")
    P("| set | n | p54/PUC54 cyc | g54/PUC54 cyc | p54/PUC54 ins | g54/PUC54 ins | n(LJ) | p54/LJ cyc | g54/LJ cyc | p55/PUC55 cyc | g55/PUC55 cyc | p55/PUC55 ins | p55/LJ cyc | g55/LJ cyc | (p54/PUC54)/(p55/PUC55) | (g54/PUC54)/(g55/PUC55) | p54/i54 (JIT gain 5.4) | p55/int 5.5 (pass A, ref) |")
    P("|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|")
    for g in GROUPS:
        ks = pickC(g)
        nl = [k for k in ks if ratio(Cok, k, "p54", "lj") and ratio(Cok, k, "p55", "lj")]
        a54, b54, a55, b55 = G(Cok, ks, "p54", "puc"), G(Cok, ks, "g54", "puc"), G(Cok, ks, "p55", "ref55"), G(Cok, ks, "g55", "ref55")
        pa = pick(A, g) if A else []
        ja = G(A, [k for k in pa if k in Cok], "plain", "int") if A else float("nan")
        P(f"| {g} | {len(ks)} | {f3(a54)} | {f3(b54)} | {f3(G(Cok,ks,'p54','puc',ins))} | {f3(G(Cok,ks,'g54','puc',ins))} | {len(nl)} | {f3(G(Cok,nl,'p54','lj'))} | {f3(G(Cok,nl,'g54','lj'))} | "
          f"{f3(a55)} | {f3(b55)} | {f3(G(Cok,ks,'p55','ref55',ins))} | {f3(G(Cok,nl,'p55','lj'))} | {f3(G(Cok,nl,'g55','lj'))} | {f3(a54/a55)} | {f3(b54/b55)} | {f3(G(Cok,ks,'p54','i54'))} | {f3(ja)} |")
    P("\n## 5.4 vs 5.5, our own builds on the same programs (JIT on): p54/p55 and g54/g55 cycles and instructions\n")
    P("| set | n | p54/p55 cyc | p54/p55 ins | g54/g55 cyc | g54/g55 ins | PUC54/PUC55 cyc (reference speed difference) | n54(nojit)/p54 cyc |")
    P("|---|---|---|---|---|---|---|---|")
    for g in GROUPS:
        ks = pickC(g)
        P(f"| {g} | {len(ks)} | {f3(G(Cok,ks,'p54','p55'))} | {f3(G(Cok,ks,'p54','p55',ins))} | {f3(G(Cok,ks,'g54','g55'))} | {f3(G(Cok,ks,'g54','g55',ins))} | {f3(G(Cok,ks,'puc','ref55'))} | {f3(G(Cok,ks,'n54','p54'))} |")
    lj_tables(Cok, "Our 5.4 build vs LuaJIT and PUC 5.4 (pass C, programs both PUC 5.4 and p54 run)", [("p54", "p54"), ("g54", "g54")])
    gaps(Cok, "p54", "p54"); gaps(Cok, "g54", "g54")
    gaps(Cok, "p54", "p54", "puc", otherlabel="PUC54"); gaps(Cok, "g54", "g54", "puc", otherlabel="PUC54")
    # biggest 5.4-specific relative regressions: (p54/puc54)/(p55/ref55)
    xs = []
    for k in ok54:
        a, b = ratio(Cok, k, "p54", "puc"), ratio(Cok, k, "p55", "ref55")
        if a and b and (cyc(Cok, k, "p54") or 0) >= 20e6: xs.append((a / b, k, a, b))
    P("\n## Programs where 5.4 is relatively slower than 5.5 (rel = (p54/PUC54)/(p55/PUC55), programs >= 20 Mcyc): top 25\n")
    P("| program | rel | p54/PUC54 | p55/PUC55 | p54/PUC54 ins | p55/PUC55 ins | p54 Mcyc | p54/i54 | p55/(JIT off) not in pass C |"); P("|---|---|---|---|---|---|---|---|---|")
    for rel, k, a, b in sorted(xs, reverse=True)[:25]:
        P(f"| {k} | {rel:.3f} | {a:.3f} | {b:.3f} | {f3(ratio(Cok,k,'p54','puc',ins))} | {f3(ratio(Cok,k,'p55','ref55',ins))} | {cyc(Cok,k,'p54')/1e6:.0f} | {f3(ratio(Cok,k,'p54','i54'))} | |")
    P("\n## Programs where 5.4 is relatively faster than 5.5: top 10\n"); P("| program | rel | p54/PUC54 | p55/PUC55 |"); P("|---|---|---|---|")
    for rel, k, a, b in sorted(xs)[:10]: P(f"| {k} | {rel:.3f} | {a:.3f} | {b:.3f} |")
    P("\n## Per-program (pass C)\n")
    P("| program | p54 Mcyc | g54 | i54 | n54 | PUC54 | PUC55 | LJ | p55 | g55 | p54/PUC54 | g54/PUC54 | p54/LJ | p55/PUC55 | p54/p55 ins | out p54 | out g54 | exp p54 |"); P("|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|")
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
    P("\n`.expected` files are Lua 5.5 outputs; 5.4 mismatches vs `.expected` where PUC 5.4 also mismatches (so not our bug):")
    for k in sorted(C):
        e = C[k]["cols"]
        if e.get("p54", {}).get("exp") == "MISMATCH": P(f"- {k}: p54 MISMATCH vs .expected; PUC 5.4 exp={e.get('puc',{}).get('exp')}, p54 out vs PUC54 = {e.get('p54',{}).get('out')}")
    P("\nDup/noise in pass C is not measured (no dup column); see the 15-run re-runs.\n")

# 15-run reruns of 5.4
if R54:
    P("\n## 5.4 15-run serial CPU-4 re-measure of the programs where p54/PUC54 is worst or 5.4 is relatively slower (p54b = dup of p54)\n")
    P("| program | p54 Mcyc | PUC54 Mcyc | p54/PUC54 cyc | ins | g54/PUC54 cyc | g54/p54 cyc | p54/p54b (dup) | pass C p54/PUC54 | pass C p54/PUC54 / (p55/PUC55) |"); P("|---|---|---|---|---|---|---|---|---|---|")
    for k in sorted(R54): P(f"| {k} | {cyc(R54,k,'p54')/1e6:.0f} | {cyc(R54,k,'puc')/1e6:.0f} | {f3(ratio(R54,k,'p54','puc'))} | {f3(ratio(R54,k,'p54','puc',ins))} | {f3(ratio(R54,k,'g54','puc'))} | {f3(ratio(R54,k,'g54','p54'))} | {f3(ratio(R54,k,'p54','p54b'))} | {f3(ratio(C,k,'p54','puc'))} | {f3((ratio(C,k,'p54','puc') or 0)/(ratio(C,k,'p55','ref55') or 1))} |")

if RR:
    P("\n## 5.5 15-run serial CPU-4 re-measure of all re-run programs (PGO regressions + trend suspects)\n")
    P("| program | plain Mcyc | PGO Mcyc | b3 plain | b3 PGO | PGO/plain cyc | ins | plain/b3 cyc | ins | PGO/b3g cyc | ins | dup |"); P("|---|---|---|---|---|---|---|---|---|---|---|---|")
    for k in sorted(RR):
        P(f"| {k} | {cyc(RR,k,'plain')/1e6:.0f} | {cyc(RR,k,'pgo')/1e6:.0f} | {cyc(RR,k,'b3plain')/1e6:.0f} | {cyc(RR,k,'b3pgo')/1e6:.0f} | {f3(ratio(RR,k,'pgo','plain'))} | {f3(ratio(RR,k,'pgo','plain',ins))} | {f3(ratio(RR,k,'plain','b3plain'))} | {f3(ratio(RR,k,'plain','b3plain',ins))} | {f3(ratio(RR,k,'pgo','b3pgo'))} | {f3(ratio(RR,k,'pgo','b3pgo',ins))} | {f3(ratio(RR,k,'plain','plain2'))} |")

# output check for 5.5
P("\n## 5.5 output / .expected check (passes A and B)\n")
bad = []
for R, tag in ((A, "A"), (B, "B")):
    for k in sorted(R):
        for c, e in R[k]["cols"].items():
            if c not in ("lj", "puc"):
                if e.get("exp") == "MISMATCH" or e.get("out") == "diff" or e.get("err"):
                    bad.append(f"- pass {tag} {k} [{c}]: exp={e.get('exp')} out={e.get('out')} err={e.get('err')}")
P("\n".join(bad) if bad else "All ours columns equal PUC 5.5 and .expected on every program, passes A and B.")
if A:
    P("\nLuaJIT problems / no LuaJIT variant (pass A):\n")
    for k in sorted(A):
        e = A[k]["cols"].get("lj", {})
        if e.get("err") or e.get("out") == "diff": P(f"- {k}: {e.get('err') or 'output differs from PUC'}")
open(os.path.join(OUT, "tables.md"), "w").write("\n".join(L) + "\n")
cn = ["puc", "lj", "plain", "plain2", "int", "nj", "pgo", "b3plain", "b3pgo"]
for R, name, cols in ((A, "per_program_passA.csv", cn), (B, "per_program_passB.csv", ["puc", "lj", "plain", "pgo", "b3plain", "b3pgo"]),
                      (C, "per_program_pass54.csv", ["puc", "ref55", "lj", "p54", "g54", "i54", "n54", "p55", "g55"])):
    if not R: continue
    with open(os.path.join(OUT, name), "w") as f:
        f.write("program,core," + ",".join(c + "_cyc_min" for c in cols) + "," + ",".join(c + "_ins_min" for c in cols) + "\n")
        for k in sorted(R):
            f.write(f"{k},{R[k]['cpu']}," + ",".join(str(cyc(R, k, c) or "") for c in cols) + "," + ",".join(str(ins(R, k, c) or "") for c in cols) + "\n")
print("\n".join(L[:50]))

# startup tables appended
import statistics as st
sf = os.path.join(OUT, "startup_size.json")
if os.path.exists(sf):
    d = json.load(open(sf)); M = []
    M.append("\n\n# C5. Startup and size\n")
    M.append("Cycles / instructions are `perf stat` user-mode, min of 50 runs pinned to CPU 4 (the numbers include exec, dynamic loading and process teardown). JIT-off = same plain binary with RUSTYNAIL_JIT=off, nojit-build = built without the jit feature.\n")
    names = list(d["empty"])
    for sc, title in (("empty", "empty script"), ("print", "print('hello')"), ("loop1k", "1000-iteration loop (reaches the tier-1 hotness threshold)"), ("fib20", "fib(20), ~22k calls"), ("tab_str", "200 string concats + table.concat")):
        M.append(f"\n## {title}\n"); M.append("| build | cycles (k) | instructions (k) | vs PUC 5.5 cyc | vs LuaJIT cyc |"); M.append("|---|---|---|---|---|")
        for n in names:
            v = d[sc][n]; M.append(f"| {n} | {v['cyc_min']/1e3:.0f} | {v['ins_min']/1e3:.0f} | {v['cyc_min']/d[sc]['PUC55']['cyc_min']:.2f} | {v['cyc_min']/d[sc]['LuaJIT']['cyc_min']:.2f} |")
    M.append("\n## `-e ''` (cycles, k)\n"); M.append("| build | cycles (k) |"); M.append("|---|---|")
    for n in names: M.append(f"| {n} | {d['dash_e'][n]['cyc_min']/1e3:.0f} |")
    M.append("\n## Memory of an empty state (read from /proc/self/status inside the script)\n"); M.append("| build | VmHWM empty (KB) | collectgarbage('count') empty (KB) | VmHWM after fib(22) (KB) | count after fib(22) (KB) |"); M.append("|---|---|---|---|---|")
    for n in names:
        a, b = d["rss_kb"][n]["rss.lua"], d["rss_kb"][n]["rss_fib.lua"]
        M.append(f"| {n} | {a[0]} | {float(a[2]):.1f} | {b[0]} | {float(b[2]):.1f} |")
    hf = os.path.join(OUT, "startup_hyperfine_summary.txt")
    if os.path.exists(hf):
        M.append("\n## Wall clock, hyperfine -N, 200 runs, pinned to CPU 4, empty script (peak RSS in the lines is hyperfine's and not meaningful)\n"); M.append("```"); M.append(open(hf).read().rstrip()); M.append("```")
    open(os.path.join(OUT, "tables.md"), "a").write("\n".join(M) + "\n")
