#!/usr/bin/env python3
"""Tables for the 2026-11-10 full benchmark (scripts/bench_session.py JSON).
Columns: puc, lj, nplain/npgo (new main 4ce16b7a), nplain2 (dup), nint (plain with RUSTYNAIL_JIT=off), nnj (non-jit build of 4ce16b7a),
qplain/qpgo (53133e8d, previous subject), mplain/mpgo (8965e73e), oplain/opgo (c276c376), rplain/rpgo (cb1af288).
usage: bench_session4_report.py passA.json passB.json rerun.json|- outdir
rerun.json (15 runs, serial CPU 4, loser re-measure) is only used in the 'losers' table.
Writes outdir/tables.md, per_program_passA.csv, per_program_passB.csv."""
import json, math, os, sys

A = json.load(open(sys.argv[1]))["results"]
B = json.load(open(sys.argv[2]))["results"]
RR = json.load(open(sys.argv[3]))["results"] if sys.argv[3] != "-" and os.path.exists(sys.argv[3]) else {}
OUT = sys.argv[4]
REALDIS = {"real_dis_x86", "real_dis_arm", "real_dis_mips", "real_dis_ppc"}
BASES = [("q", "53133e8d"), ("m", "8965e73e"), ("o", "c276c376"), ("r", "cb1af288")]


def isrd(k): return k.split("/")[-1] in REALDIS


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

P("# Tables, session 2026-11-10 (main 4ce16b7a)\n")
P("Ratios below 1 mean ours is faster / the change is a win. cyc = cycles:u, ins = instructions:u, min of 5 interleaved runs.\n")
P("Columns: plain/pgo = main 4ce16b7a; q = 53133e8d (previous subject); m = 8965e73e; o = c276c376; r = cb1af288 (each plain and PGO); "
  "int = plain binary with RUSTYNAIL_JIT=off; nj = non-jit build of 4ce16b7a; dup = plain run twice (noise).\n")

P("\n## Headline A: ours vs previous subject 53133e8d (pass A: CPUs 3-7, cycles and instructions, geomean over programs)\n")
P("| set | n | plain cyc | plain ins | PGO cyc | PGO ins | plain/PUC cyc | PGO/PUC cyc | plain/PUC ins | PGO/PUC ins | plain/interp(JIT off) | plain/nonjit build | dup |")
P("|---|---|---|---|---|---|---|---|---|---|---|---|---|")
for g in GROUPS:
    ks = pick(A, g)
    P(f"| {g} | {len(ks)} | {f3(G(A,ks,'nplain','qplain'))} | {f3(G(A,ks,'nplain','qplain',ins))} | {f3(G(A,ks,'npgo','qpgo'))} | {f3(G(A,ks,'npgo','qpgo',ins))} | "
      f"{f3(G(A,ks,'nplain','puc'))} | {f3(G(A,ks,'npgo','puc'))} | {f3(G(A,ks,'nplain','puc',ins))} | {f3(G(A,ks,'npgo','puc',ins))} | "
      f"{f3(G(A,ks,'nplain','nint'))} | {f3(G(A,ks,'nplain','nnj'))} | {f3(G(A,ks,'nplain','nplain2'))} |")

P("\n## Headline B: ours vs older baselines (pass A, cycles / instructions, plain and PGO)\n")
P("| set | n | plain vs 8965 cyc | ins | PGO vs 8965 cyc | ins | plain vs c276 cyc | ins | PGO vs c276 cyc | ins | plain vs cb1 cyc | ins | PGO vs cb1 cyc | ins |")
P("|---|---|---|---|---|---|---|---|---|---|---|---|---|---|")
for g in GROUPS:
    ks = pick(A, g)
    row = [g, str(len(ks))]
    for b, _ in BASES[1:]:
        row += [f3(G(A, ks, "nplain", b + "plain")), f3(G(A, ks, "nplain", b + "plain", ins)),
                f3(G(A, ks, "npgo", b + "pgo")), f3(G(A, ks, "npgo", b + "pgo", ins))]
    P("| " + " | ".join(row) + " |")

P("\n## Headline C: pass B, fast Zen5 cores (CPUs 4, 6), ours / LuaJIT cycles (n = programs with a LuaJIT variant)\n")
P("| set | n | plain/LJ | PGO/LJ | 53133e8d/LJ | 53133e8d PGO/LJ | 8965/LJ | 8965 PGO/LJ | c276/LJ | c276 PGO/LJ | cb1/LJ | cb1 PGO/LJ | plain/PUC | PGO/PUC | plain vs 5313 cyc | ins | PGO vs 5313 cyc | ins |")
P("|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|")
for g in GROUPS:
    ks = pick(B, g)
    nl = [k for k in ks if ratio(B, k, "nplain", "lj")]
    P(f"| {g} | {len(nl)} | {f3(G(B,nl,'nplain','lj'))} | {f3(G(B,nl,'npgo','lj'))} | {f3(G(B,nl,'qplain','lj'))} | {f3(G(B,nl,'qpgo','lj'))} | "
      f"{f3(G(B,nl,'mplain','lj'))} | {f3(G(B,nl,'mpgo','lj'))} | {f3(G(B,nl,'oplain','lj'))} | {f3(G(B,nl,'opgo','lj'))} | {f3(G(B,nl,'rplain','lj'))} | {f3(G(B,nl,'rpgo','lj'))} | "
      f"{f3(G(B,ks,'nplain','puc'))} | {f3(G(B,ks,'npgo','puc'))} | {f3(G(B,ks,'nplain','qplain'))} | {f3(G(B,ks,'nplain','qplain',ins))} | {f3(G(B,ks,'npgo','qpgo'))} | {f3(G(B,ks,'npgo','qpgo',ins))} |")
P("\nLuaJIT ratios in pass A (all cores mixed): ")
for g in ("ALL", "corpus2 dev", "corpus2 heldout", "corpus (old)"):
    ks = pick(A, g); nl = [k for k in ks if ratio(A, k, "nplain", "lj")]
    P(f"- {g}: plain/LJ {f3(G(A,nl,'nplain','lj'))}, PGO/LJ {f3(G(A,nl,'npgo','lj'))} (n={len(nl)})")


def per_prog(title, ks, R=A):
    P(f"\n### {title}\n")
    P("Mcyc min; ratios are cycles ours/other unless stated. q/m/o/r = plain vs 53133e8d / 8965e73e / c276c376 / cb1af288, qp = PGO vs 53133e8d PGO; "
      "iq / iqp = instruction ratio plain / PGO vs 53133e8d.\n")
    P("| program | plain | pgo | interp | nonjit | PUC | LJ | plain/PUC | plain/interp | plain/LJ | pgo/LJ | q | qp | m | o | r | iq | iqp | core |")
    P("|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|")
    mc = lambda k, c: (lambda v: "-" if v is None else f"{v / 1e6:.0f}")(cyc(R, k, c))
    for k in ks:
        P(f"| {k} | {mc(k,'nplain')} | {mc(k,'npgo')} | {mc(k,'nint')} | {mc(k,'nnj')} | {mc(k,'puc')} | {mc(k,'lj')} | {f3(ratio(R,k,'nplain','puc'))} | "
          f"{f3(ratio(R,k,'nplain','nint'))} | {f3(ratio(R,k,'nplain','lj'))} | {f3(ratio(R,k,'npgo','lj'))} | {f3(ratio(R,k,'nplain','qplain'))} | {f3(ratio(R,k,'npgo','qpgo'))} | "
          f"{f3(ratio(R,k,'nplain','mplain'))} | {f3(ratio(R,k,'nplain','oplain'))} | {f3(ratio(R,k,'nplain','rplain'))} | "
          f"{f3(ratio(R,k,'nplain','qplain',ins))} | {f3(ratio(R,k,'npgo','qpgo',ins))} | {R[k]['cpu']} |")


P("\n# Per-program tables (pass A, all programs)")
per_prog("real (bench/real), reported separately", pick(A, "real (5)"))
per_prog("real_dis_* ports (dev arm, x86; heldout mips, ppc), reported separately", sorted(pick(A, "real_dis (4)")))
for g in ["corpus", "corpus2/dev", "corpus2/heldout", "awfy", "programs", "jit-trace", "apps", "lib"]:
    per_prog(g, [k for k in KEYS if k.startswith(g + "/") and not isrd(k)])

P("\n### real + real_dis on fast cores, pass B (Mcyc min)\n")
P("| program | plain | pgo | PUC | LJ | plain/PUC | plain/LJ | pgo/LJ | plain/5313 | pgo/5313pgo | plain/8965 | plain/c276 | plain/cb1 | ins plain/5313 |")
P("|---|---|---|---|---|---|---|---|---|---|---|---|---|---|")
for k in sorted(pick(B, "real (5)") + pick(B, "real_dis (4)")):
    mm = lambda c: (lambda v: "-" if v is None else f"{v / 1e6:.0f}")(cyc(B, k, c))
    P(f"| {k} | {mm('nplain')} | {mm('npgo')} | {mm('puc')} | {mm('lj')} | {f3(ratio(B,k,'nplain','puc'))} | {f3(ratio(B,k,'nplain','lj'))} | {f3(ratio(B,k,'npgo','lj'))} | "
      f"{f3(ratio(B,k,'nplain','qplain'))} | {f3(ratio(B,k,'npgo','qpgo'))} | {f3(ratio(B,k,'nplain','mplain'))} | {f3(ratio(B,k,'nplain','oplain'))} | {f3(ratio(B,k,'nplain','rplain'))} | {f3(ratio(B,k,'nplain','qplain',ins))} |")

# winners and losers vs previous subject
chg = []
for k in KEYS:
    r = ratio(A, k, "nplain", "qplain")
    if r:
        chg.append((r, k, ratio(A, k, "npgo", "qpgo"), cyc(A, k, "nplain"), ratio(A, k, "nplain", "qplain", ins), ratio(A, k, "npgo", "qpgo", ins)))
chg.sort()
P("\n## Top 10 winners vs 53133e8d (plain cycles ratio, lowest first)\n")
P("| program | plain cyc | plain ins | pgo cyc | pgo ins | plain Mcyc |")
P("|---|---|---|---|---|---|")
for r, k, rp, c, i, ip in chg[:10]:
    P(f"| {k} | {r:.3f} | {f3(i)} | {f3(rp)} | {f3(ip)} | {c/1e6:.0f} |")
P("\n## Top 10 winners vs 53133e8d (PGO cycles ratio, lowest first)\n")
P("| program | pgo cyc | pgo ins | plain cyc | plain ins | pgo Mcyc |")
P("|---|---|---|---|---|---|")
for r, k, rp, c, i, ip in sorted(chg, key=lambda x: x[2] or 9)[:10]:
    P(f"| {k} | {f3(rp)} | {f3(ip)} | {r:.3f} | {f3(i)} | {(cyc(A,k,'npgo') or 0)/1e6:.0f} |")
P("\n## Top 10 losers vs 53133e8d (plain cycles ratio, highest first)\n")
P("| program | plain cyc | plain ins | pgo cyc | pgo ins | plain Mcyc | tiny (<100 Mcyc) |")
P("|---|---|---|---|---|---|---|")
for r, k, rp, c, i, ip in chg[::-1][:10]:
    P(f"| {k} | {r:.3f} | {f3(i)} | {f3(rp)} | {f3(ip)} | {c/1e6:.0f} | {'yes' if c < 1e8 else ''} |")
P("\n## Top 10 losers vs 53133e8d (PGO cycles ratio, highest first)\n")
P("| program | pgo cyc | pgo ins | plain cyc | plain ins | pgo Mcyc |")
P("|---|---|---|---|---|---|")
for r, k, rp, c, i, ip in sorted(chg, key=lambda x: -(x[2] or 0))[:10]:
    P(f"| {k} | {f3(rp)} | {f3(ip)} | {r:.3f} | {f3(i)} | {(cyc(A,k,'npgo') or 0)/1e6:.0f} |")

losers = [x for x in chg[::-1] if x[0] > 1.03 or (x[2] and x[2] > 1.03)]
P(f"\n## All programs more than 3% slower than 53133e8d (plain or PGO cycles, pass A): {len(losers)}; 15-run serial CPU-4 re-measure where available\n")
P("| program | plain cyc A | plain ins | pgo cyc A | pgo ins | plain Mcyc | re-run plain cyc (15 runs) | re-run plain ins | re-run pgo cyc | re-run pgo ins | re-run dup |")
P("|---|---|---|---|---|---|---|---|---|---|---|")
for r, k, rp, c, i, ip in losers:
    inr = k in RR
    P(f"| {k} | {r:.3f} | {f3(i)} | {f3(rp)} | {f3(ip)} | {c/1e6:.0f} | "
      f"{f3(ratio(RR,k,'nplain','qplain')) if inr else '-'} | {f3(ratio(RR,k,'nplain','qplain',ins)) if inr else '-'} | "
      f"{f3(ratio(RR,k,'npgo','qpgo')) if inr else '-'} | {f3(ratio(RR,k,'npgo','qpgo',ins)) if inr else '-'} | {f3(ratio(RR,k,'nplain','nplain2')) if inr else '-'} |")

P("\n## Biggest gaps vs LuaJIT, PGO build (pass B fast cores, worst first; top 30)\n")
gp = sorted([(ratio(B, k, "npgo", "lj"), k) for k in B if ratio(B, k, "npgo", "lj")], reverse=True)
P("| program | PGO/LJ | plain/LJ | PGO Mcyc | LJ Mcyc | PGO ins / LJ ins |")
P("|---|---|---|---|---|---|")
for r, k in gp[:30]:
    P(f"| {k} | {r:.3f} | {f3(ratio(B,k,'nplain','lj'))} | {cyc(B,k,'npgo')/1e6:.0f} | {cyc(B,k,'lj')/1e6:.0f} | {f3(ratio(B,k,'npgo','lj',ins))} |")
P("\n### Biggest absolute gaps vs LuaJIT (PGO, Mcyc difference, top 15)\n")
P("| program | PGO Mcyc | LJ Mcyc | diff Mcyc | ratio |")
P("|---|---|---|---|---|")
for d, k in sorted([(cyc(B, k, "npgo") - cyc(B, k, "lj"), k) for k in B if ratio(B, k, "npgo", "lj")], reverse=True)[:15]:
    P(f"| {k} | {cyc(B,k,'npgo')/1e6:.0f} | {cyc(B,k,'lj')/1e6:.0f} | {d/1e6:.0f} | {ratio(B,k,'npgo','lj'):.3f} |")

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
P("\n".join(bad) if bad else "All ours columns (all subjects, plain and PGO, interp, non-jit) equal PUC and .expected on every program, passes A and B.")
P("\nLuaJIT problems (not runnable / output differs from PUC beyond numeric tolerance):\n")
for k in KEYS:
    e = A[k]["cols"].get("lj", {})
    if e.get("err") or e.get("out") == "diff":
        P(f"- {k}: {e.get('err') or 'output differs from PUC'}")
os.makedirs(OUT, exist_ok=True)
open(os.path.join(OUT, "tables.md"), "w").write("\n".join(L) + "\n")
cn = ["puc", "lj", "nplain", "nplain2", "nint", "nnj", "npgo", "qplain", "qpgo", "mplain", "mpgo", "oplain", "opgo", "rplain", "rpgo"]
for R, name, cols in ((A, "per_program_passA.csv", cn), (B, "per_program_passB.csv", [c for c in cn if c not in ("nplain2", "nint", "nnj")])):
    with open(os.path.join(OUT, name), "w") as f:
        f.write("program,core," + ",".join(c + "_cyc_min" for c in cols) + "," + ",".join(c + "_ins_min" for c in cols) + "\n")
        for k in sorted(R):
            f.write(f"{k},{R[k]['cpu']}," + ",".join(str(cyc(R, k, c) or "") for c in cols) + "," + ",".join(str(ins(R, k, c) or "") for c in cols) + "\n")
print("\n".join(L[:70]))
