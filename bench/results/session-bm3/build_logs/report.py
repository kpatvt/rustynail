#!/usr/bin/env python3
"""session-bm3 tables. usage: report.py passA.json passB.json rerun.json|- oldA.json oldB.json outdir
Columns now: puc, lj, nplain/npgo (1e6ec4ca, new main), nplain2 (dup), nint, nnj, pplain/ppgo (80f1d96e rebuilt in this session).
Old session (2026-11-11) JSON: nplain/npgo = 80f1d96e as measured then."""
import json, math, os, sys

A = json.load(open(sys.argv[1]))["results"]
B = json.load(open(sys.argv[2]))["results"]
RR = json.load(open(sys.argv[3]))["results"] if sys.argv[3] != "-" and os.path.exists(sys.argv[3]) else {}
OA = json.load(open(sys.argv[4]))["results"]
OB = json.load(open(sys.argv[5]))["results"]
OUT = sys.argv[6]
REALDIS = {"real_dis_x86", "real_dis_arm", "real_dis_mips", "real_dis_ppc"}


def isrd(k): return k.split("/")[-1] in REALDIS


def vals(R, k, col, f): return [x for x in R.get(k, {}).get("cols", {}).get(col, {}).get(f, []) if x]
def cyc(R, k, col):
    v = vals(R, k, col, "cyc"); return min(v) if v else None
def ins(R, k, col):
    v = vals(R, k, col, "ins"); return min(v) if v else None


def gm(xs):
    xs = [x for x in xs if x and x > 0]
    return math.exp(sum(math.log(x) for x in xs) / len(xs)) if xs else float("nan")


def ratio2(Rx, kx, cx, Ry, ky, cy, f=cyc):
    x, y = f(Rx, kx, cx), f(Ry, ky, cy)
    return x / y if x and y else None


def ratio(R, k, a, b, f=cyc): return ratio2(R, k, a, R, k, b, f)


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


GROUPS = ["corpus (old)", "corpus2 dev", "corpus2 dev excl real_dis", "corpus2 heldout", "corpus2 heldout excl real_dis", "awfy",
          "programs", "jit-trace", "lib", "apps", "real (5)", "real_dis (4)", "ALL excl real/real_dis", "ALL"]
KEYS = sorted(A)
L = []
P = L.append
f3 = lambda v: "-" if v is None or v != v else f"{v:.3f}"
G = lambda R, ks, a, b, f=cyc: gm([ratio(R, k, a, b, f) for k in ks])
# ours/LJ in pass B of the old session (80f1d96e as then measured)
def gl(R, ks, c, f=cyc): return gm([ratio(R, k, c, "lj", f) for k in ks if ratio(R, k, c, "lj")])

P("# Tables, session-bm3 (main 1e6ec4ca)\n")
P("Ratios below 1 mean ours is faster / the change is a win. cyc = cycles:u, ins = instructions:u, min of 5 interleaved runs.\n")
P("Columns: plain/pgo = main 1e6ec4ca; p = 80f1d96e (previous subject, REBUILT and measured in this session); old = the same 80f1d96e as "
  "measured in session-bm2; int = plain binary with RUSTYNAIL_JIT=off; nj = non-jit build of 1e6ec4ca; dup = plain run twice (noise).\n")

P("\n## Headline A: 1e6ec4ca vs 80f1d96e (same session; pass A: CPUs 3-7, geomean over programs)\n")
P("| set | n | plain cyc | plain ins | PGO cyc | PGO ins | plain/PUC cyc | PGO/PUC cyc | plain/PUC ins | PGO/PUC ins | plain/interp(JIT off) | plain/nonjit build | dup |")
P("|---|---|---|---|---|---|---|---|---|---|---|---|---|")
for g in GROUPS:
    ks = pick(A, g)
    P(f"| {g} | {len(ks)} | {f3(G(A,ks,'nplain','pplain'))} | {f3(G(A,ks,'nplain','pplain',ins))} | {f3(G(A,ks,'npgo','ppgo'))} | {f3(G(A,ks,'npgo','ppgo',ins))} | "
      f"{f3(G(A,ks,'nplain','puc'))} | {f3(G(A,ks,'npgo','puc'))} | {f3(G(A,ks,'nplain','puc',ins))} | {f3(G(A,ks,'npgo','puc',ins))} | "
      f"{f3(G(A,ks,'nplain','nint'))} | {f3(G(A,ks,'nplain','nnj'))} | {f3(G(A,ks,'nplain','nplain2'))} |")

P("\n## Headline B: 1e6ec4ca vs 80f1d96e as measured in session-bm2 (cross-session, pass A, different day/thermal state; indicative only)\n")
P("| set | n | plain cyc | plain ins | PGO cyc | PGO ins | rebuilt 80f1d96e now vs then: plain cyc | PGO cyc |")
P("|---|---|---|---|---|---|---|---|")
for g in GROUPS:
    ks = [k for k in pick(A, g) if k in OA]
    h = lambda a, ca, b, cb, f=cyc: gm([ratio2(A, k, ca, OA, k, cb, f) for k in ks])
    P(f"| {g} | {len(ks)} | {f3(h(0,'nplain',0,'nplain'))} | {f3(h(0,'nplain',0,'nplain',ins))} | {f3(h(0,'npgo',0,'npgo'))} | {f3(h(0,'npgo',0,'npgo',ins))} | "
      f"{f3(h(0,'pplain',0,'nplain'))} | {f3(h(0,'ppgo',0,'npgo'))} |")

P("\n## Headline C: pass B, fast Zen5 cores (CPUs 4, 6), ours / LuaJIT cycles, now vs session-bm2 (n = programs with a LuaJIT variant)\n")
P("old = session-bm2 pass B (80f1d96e). p = 80f1d96e rebuilt now.\n")
P("| set | n | now plain/LJ | now PGO/LJ | old plain/LJ | old PGO/LJ | p plain/LJ (now) | p PGO/LJ (now) | plain/PUC | PGO/PUC | plain vs p cyc | ins | PGO vs p cyc | ins | now plain/LJ ins | now PGO/LJ ins |")
P("|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|")
for g in GROUPS:
    ks = pick(B, g)
    nl = [k for k in ks if ratio(B, k, "nplain", "lj")]
    ol = [k for k in nl if k in OB]
    P(f"| {g} | {len(nl)} | {f3(G(B,nl,'nplain','lj'))} | {f3(G(B,nl,'npgo','lj'))} | {f3(gl(OB,ol,'nplain'))} | {f3(gl(OB,ol,'npgo'))} | "
      f"{f3(G(B,nl,'pplain','lj'))} | {f3(G(B,nl,'ppgo','lj'))} | {f3(G(B,ks,'nplain','puc'))} | {f3(G(B,ks,'npgo','puc'))} | "
      f"{f3(G(B,ks,'nplain','pplain'))} | {f3(G(B,ks,'nplain','pplain',ins))} | {f3(G(B,ks,'npgo','ppgo'))} | {f3(G(B,ks,'npgo','ppgo',ins))} | "
      f"{f3(G(B,nl,'nplain','lj',ins))} | {f3(G(B,nl,'npgo','lj',ins))} |")
P("\nLuaJIT ratios in pass A (all cores mixed): ")
for g in ("ALL", "corpus2 dev", "corpus2 heldout", "corpus (old)"):
    ks = pick(A, g); nl = [k for k in ks if ratio(A, k, "nplain", "lj")]
    P(f"- {g}: plain/LJ {f3(G(A,nl,'nplain','lj'))}, PGO/LJ {f3(G(A,nl,'npgo','lj'))} (n={len(nl)})")

# per program comparison table (pass B ratios vs LJ, now vs then) -- all programs
P("\n## Per-program comparison vs session-bm2 (pass B for LJ ratios, pass A for same-session plain/PGO vs p)\n")
P("old = session-bm2. cyc/ins ratios ours/LJ from pass B (fast cores). 'vs p' = 1e6ec4ca / 80f1d96e rebuilt, pass A cycles and instructions.\n")
P("| program | now plain/LJ cyc | old plain/LJ cyc | now plain/LJ ins | old plain/LJ ins | now PGO/LJ cyc | old PGO/LJ cyc | now PGO/LJ ins | old PGO/LJ ins | plain vs p cyc | plain vs p ins | PGO vs p cyc | PGO vs p ins | plain Mcyc |")
P("|---|---|---|---|---|---|---|---|---|---|---|---|---|---|")
for k in KEYS:
    o = k in OB
    P(f"| {k} | {f3(ratio(B,k,'nplain','lj'))} | {f3(ratio(OB,k,'nplain','lj')) if o else '-'} | {f3(ratio(B,k,'nplain','lj',ins))} | {f3(ratio(OB,k,'nplain','lj',ins)) if o else '-'} | "
      f"{f3(ratio(B,k,'npgo','lj'))} | {f3(ratio(OB,k,'npgo','lj')) if o else '-'} | {f3(ratio(B,k,'npgo','lj',ins))} | {f3(ratio(OB,k,'npgo','lj',ins)) if o else '-'} | "
      f"{f3(ratio(A,k,'nplain','pplain'))} | {f3(ratio(A,k,'nplain','pplain',ins))} | {f3(ratio(A,k,'npgo','ppgo'))} | {f3(ratio(A,k,'npgo','ppgo',ins))} | {(cyc(A,k,'nplain') or 0)/1e6:.0f} |")


def per_prog(title, ks, R=A):
    P(f"\n### {title}\n")
    P("Mcyc min; ratios are cycles ours/other unless stated. p = plain vs 80f1d96e plain (rebuilt), pp = PGO vs 80f1d96e PGO; ip / ipp = instruction ratio plain / PGO.\n")
    P("| program | plain | pgo | interp | nonjit | PUC | LJ | plain/PUC | plain/interp | plain/LJ | pgo/LJ | p | pp | ip | ipp | core |")
    P("|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|")
    mc = lambda k, c: (lambda v: "-" if v is None else f"{v / 1e6:.0f}")(cyc(R, k, c))
    for k in ks:
        P(f"| {k} | {mc(k,'nplain')} | {mc(k,'npgo')} | {mc(k,'nint')} | {mc(k,'nnj')} | {mc(k,'puc')} | {mc(k,'lj')} | {f3(ratio(R,k,'nplain','puc'))} | "
          f"{f3(ratio(R,k,'nplain','nint'))} | {f3(ratio(R,k,'nplain','lj'))} | {f3(ratio(R,k,'npgo','lj'))} | {f3(ratio(R,k,'nplain','pplain'))} | {f3(ratio(R,k,'npgo','ppgo'))} | "
          f"{f3(ratio(R,k,'nplain','pplain',ins))} | {f3(ratio(R,k,'npgo','ppgo',ins))} | {R[k]['cpu']} |")


P("\n# Per-program tables (pass A, all programs)")
per_prog("real (bench/real), reported separately", pick(A, "real (5)"))
per_prog("real_dis_* ports (dev arm, x86; heldout mips, ppc), reported separately", sorted(pick(A, "real_dis (4)")))
for g in ["corpus", "corpus2/dev", "corpus2/heldout", "awfy", "programs", "jit-trace", "apps", "lib"]:
    per_prog(g, [k for k in KEYS if k.startswith(g + "/") and not isrd(k)])

# top gaps (PGO and plain), improvements, regressions
def blk(title, rows, header):
    P(f"\n## {title}\n"); P(header[0]); P(header[1])
    for r in rows: P(r)

gp = sorted([(ratio(B, k, "npgo", "lj"), k) for k in B if ratio(B, k, "npgo", "lj")], reverse=True)
blk("Top-15 remaining gaps to LuaJIT, PGO build (pass B, fast cores)",
    [f"| {k} | {r:.3f} | {f3(ratio(OB,k,'npgo','lj')) if k in OB else '-'} | {f3(ratio(B,k,'nplain','lj'))} | {cyc(B,k,'npgo')/1e6:.0f} | {cyc(B,k,'lj')/1e6:.0f} | {f3(ratio(B,k,'npgo','lj',ins))} |" for r, k in gp[:15]],
    ("| program | PGO/LJ now | PGO/LJ old | plain/LJ now | PGO Mcyc | LJ Mcyc | PGO ins / LJ ins |", "|---|---|---|---|---|---|---|"))
gp2 = sorted([(ratio(B, k, "nplain", "lj"), k) for k in B if ratio(B, k, "nplain", "lj")], reverse=True)
blk("Top-15 remaining gaps to LuaJIT, plain build (pass B, fast cores)",
    [f"| {k} | {r:.3f} | {f3(ratio(OB,k,'nplain','lj')) if k in OB else '-'} | {cyc(B,k,'nplain')/1e6:.0f} | {cyc(B,k,'lj')/1e6:.0f} | {f3(ratio(B,k,'nplain','lj',ins))} |" for r, k in gp2[:15]],
    ("| program | plain/LJ now | plain/LJ old | plain Mcyc | LJ Mcyc | plain ins / LJ ins |", "|---|---|---|---|---|---|"))
blk("Biggest absolute gaps vs LuaJIT (PGO, Mcyc difference, top 15)",
    [f"| {k} | {cyc(B,k,'npgo')/1e6:.0f} | {cyc(B,k,'lj')/1e6:.0f} | {d/1e6:.0f} | {ratio(B,k,'npgo','lj'):.3f} |" for d, k in
     sorted([(cyc(B, k, "npgo") - cyc(B, k, "lj"), k) for k in B if ratio(B, k, "npgo", "lj")], reverse=True)[:15]],
    ("| program | PGO Mcyc | LJ Mcyc | diff Mcyc | ratio |", "|---|---|---|---|---|"))

chg = []
for k in KEYS:
    r = ratio(A, k, "nplain", "pplain")
    if r:
        d = ratio(A, k, "nplain", "nplain2")
        chg.append(dict(k=k, r=r, ri=ratio(A, k, "nplain", "pplain", ins), rp=ratio(A, k, "npgo", "ppgo"), rpi=ratio(A, k, "npgo", "ppgo", ins),
                        mc=cyc(A, k, "nplain") / 1e6, dup=d))
def row(c, key):
    lay = "layout?" if (abs((c["ri"] or 1) - 1) < 0.01 and abs(c["r"] - 1) > 0.04) else ""
    return f"| {c['k']} | {c['r']:.3f} | {f3(c['ri'])} | {f3(c['rp'])} | {f3(c['rpi'])} | {c['mc']:.0f} | {f3(c['dup'])} | {lay} |"
H = ("| program | plain cyc | plain ins | PGO cyc | PGO ins | plain Mcyc | dup | flag |", "|---|---|---|---|---|---|---|---|")
big = [c for c in chg if c["mc"] >= 30]
blk("Top-15 improvements vs 80f1d96e, plain cycles (programs >= 30 Mcyc; pass A, same session)", [row(c, "r") for c in sorted(big, key=lambda c: c["r"])[:15]], H)
blk("Top-15 improvements vs 80f1d96e, PGO cycles", [row(c, "rp") for c in sorted(big, key=lambda c: c["rp"] or 9)[:15]], H)
blk("Top-15 regressions vs 80f1d96e, plain cycles (programs >= 30 Mcyc)", [row(c, "r") for c in sorted(big, key=lambda c: -c["r"])[:15]], H)
blk("Top-15 regressions vs 80f1d96e, PGO cycles", [row(c, "rp") for c in sorted(big, key=lambda c: -(c["rp"] or 0))[:15]], H)

losers = sorted([c for c in chg if c["r"] > 1.03 or (c["rp"] and c["rp"] > 1.03)], key=lambda c: -c["r"])
P(f"\n## All programs more than 3% slower than 80f1d96e (plain or PGO cycles, pass A): {len(losers)}; 15-run serial CPU-4 re-measure where available\n")
P("| program | plain cyc A | plain ins | pgo cyc A | pgo ins | plain Mcyc | re-run plain cyc (15 runs) | re-run plain ins | re-run pgo cyc | re-run pgo ins | re-run dup |")
P("|---|---|---|---|---|---|---|---|---|---|---|")
for c in losers:
    k = c["k"]; inr = k in RR
    P(f"| {k} | {c['r']:.3f} | {f3(c['ri'])} | {f3(c['rp'])} | {f3(c['rpi'])} | {c['mc']:.0f} | "
      f"{f3(ratio(RR,k,'nplain','pplain')) if inr else '-'} | {f3(ratio(RR,k,'nplain','pplain',ins)) if inr else '-'} | "
      f"{f3(ratio(RR,k,'npgo','ppgo')) if inr else '-'} | {f3(ratio(RR,k,'npgo','ppgo',ins)) if inr else '-'} | {f3(ratio(RR,k,'nplain','nplain2')) if inr else '-'} |")
with open(os.path.join(OUT, "rerun_programs.txt"), "w") as f:
    f.write(",".join(c["k"] for c in losers))

P("\n## Duplicate-column noise: plain/plain2 differs by >3% (pass A)\n")
for k in KEYS:
    d = ratio(A, k, "nplain", "nplain2")
    if d and abs(d - 1) > 0.03:
        P(f"- {k}: {d:.3f} (plain {cyc(A,k,'nplain')/1e6:.0f} Mcyc)")

P("\n## Output / .expected check\n")
bad = []
for R, tag in ((A, "A"), (B, "B")):
    for k in sorted(R):
        for c, e in R[k]["cols"].items():
            if c != "lj" and c != "puc":
                if e.get("exp") == "MISMATCH" or e.get("out") == "diff" or e.get("err"):
                    bad.append(f"- pass {tag} {k} [{c}]: exp={e.get('exp')} out={e.get('out')} err={e.get('err')}")
P("\n".join(bad) if bad else "All ours columns (both subjects, plain and PGO, interp, non-jit) equal PUC and .expected on every program, passes A and B.")
P("\nLuaJIT problems (not runnable / output differs from PUC beyond numeric tolerance):\n")
for k in KEYS:
    e = A[k]["cols"].get("lj", {})
    if e.get("err") or e.get("out") == "diff":
        P(f"- {k}: {e.get('err') or 'output differs from PUC'}")
os.makedirs(OUT, exist_ok=True)
open(os.path.join(OUT, "tables.md"), "w").write("\n".join(L) + "\n")
cn = ["puc", "lj", "nplain", "nplain2", "nint", "nnj", "npgo", "pplain", "ppgo"]
for R, name, cols in ((A, "per_program_passA.csv", cn), (B, "per_program_passB.csv", [c for c in cn if c not in ("nplain2", "nint", "nnj")])):
    with open(os.path.join(OUT, name), "w") as f:
        f.write("program,core," + ",".join(c + "_cyc_min" for c in cols) + "," + ",".join(c + "_ins_min" for c in cols) + "\n")
        for k in sorted(R):
            f.write(f"{k},{R[k]['cpu']}," + ",".join(str(cyc(R, k, c) or "") for c in cols) + "," + ",".join(str(ins(R, k, c) or "") for c in cols) + "\n")
print("\n".join(L[:60]))
