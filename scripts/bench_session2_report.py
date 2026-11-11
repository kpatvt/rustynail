#!/usr/bin/env python3
"""Tables for the 2026-11-08 session benchmark (scripts/bench_session.py JSON), three-baseline variant of bench_session_report.py.
Columns: puc, lj, nplain, nplain2 (dup), nint (JIT off), npgo, mplain/mpgo (c276c376), oplain/opgo (cb1af288).
usage: bench_session2_report.py passA.json passB.json rerun.json|- outdir
Writes outdir/tables.md and outdir/per_program.csv. Rerun json (serial CPU-4 9 runs) overrides pass A values only in the
'after serial re-run' columns of the winners/losers tables."""
import json, math, sys, os

A = json.load(open(sys.argv[1]))["results"]
B = json.load(open(sys.argv[2]))["results"]
RR = json.load(open(sys.argv[3]))["results"] if sys.argv[3] != "-" and os.path.exists(sys.argv[3]) else {}
OUT = sys.argv[4]
SETS = ["corpus", "corpus2/dev", "corpus2/heldout", "awfy", "programs", "jit-trace", "lib", "apps", "real"]
REALDIS = {"real_dis_x86", "real_dis_arm", "real_dis_mips", "real_dis_ppc"}


def vals(R, k, col, f):
    return [x for x in R.get(k, {}).get("cols", {}).get(col, {}).get(f, []) if x]


def cyc(R, k, col):
    v = vals(R, k, col, "cyc")
    return min(v) if v else None


def ins(R, k, col):
    v = vals(R, k, col, "ins")
    return min(v) if v else None


def gm(xs):
    xs = [x for x in xs if x and x > 0]
    return math.exp(sum(math.log(x) for x in xs) / len(xs)) if xs else float("nan")


def ratio(R, k, a, b, f=cyc):
    x, y = f(R, k, a), f(R, k, b)
    return x / y if x and y else None


def group(name, R=A):
    if name == "real_dis dev (2)":
        return [k for k in R if k.startswith("corpus2/dev/real_dis")]
    if name == "real_dis heldout (2)":
        return [k for k in R if k.startswith("corpus2/heldout/real_dis")]
    if name == "real_dis all (4)":
        return [k for k in R if k.split("/")[-1] in REALDIS]
    if name == "real + real_dis (9)":
        return [k for k in R if k.startswith("real/") or k.split("/")[-1] in REALDIS]
    return [k for k in R if k.startswith(name + "/")]


GROUPS = SETS + ["real_dis dev (2)", "real_dis heldout (2)", "real_dis all (4)", "real + real_dis (9)"]
KEYS = sorted(A)
L = []
P = L.append


def g3(R, ks, a, b):
    return gm([ratio(R, k, a, b) for k in ks])


P("## Headline geomeans, pass A (cycles:u, min of 5 interleaved runs; ratio < 1 means ours faster / change is a win)\n")
P("Pass A = CPUs 3-7 (one program per core, all columns on the same core; cores are mixed fast/slow across programs). "
  "plain = main 8965e73e default release build with jit; pgo = main PGO; interp = plain with RUSTYNAIL_JIT=off; "
  "c276 = c276c376 plain, c276pgo = c276c376 PGO; cb1 = cb1af288 plain, cb1pgo = cb1af288 PGO. dup = plain/plain2 (same binary twice).\n")
P("| set | n | plain/PUC | pgo/PUC | plain/interp | plain/LJ (n) | pgo/LJ | plain vs c276 | pgo vs c276pgo | plain vs cb1 | pgo vs cb1pgo | dup |")
P("|---|---|---|---|---|---|---|---|---|---|---|---|")
for g in GROUPS + ["ALL"]:
    ks = KEYS if g == "ALL" else group(g)
    nlj = [k for k in ks if ratio(A, k, "nplain", "lj")]
    P(f"| {g} | {len(ks)} | {g3(A,ks,'nplain','puc'):.3f} | {g3(A,ks,'npgo','puc'):.3f} | {g3(A,ks,'nplain','nint'):.3f} | "
      f"{g3(A,nlj,'nplain','lj'):.3f} ({len(nlj)}) | {g3(A,nlj,'npgo','lj'):.3f} | {g3(A,ks,'nplain','mplain'):.3f} | {g3(A,ks,'npgo','mpgo'):.3f} | "
      f"{g3(A,ks,'nplain','oplain'):.3f} | {g3(A,ks,'npgo','opgo'):.3f} | {g3(A,ks,'nplain','nplain2'):.3f} |")

P("\n## Headline, fast Zen5 cores only (pass B: CPUs 4, 6; all columns of a program on one core)\n")
P("| set | n with LJ | plain/LJ | pgo/LJ | c276/LJ | c276pgo/LJ | cb1/LJ | cb1pgo/LJ | plain/PUC | pgo/PUC | plain vs c276 | pgo vs c276pgo | plain vs cb1 | pgo vs cb1pgo |")
P("|---|---|---|---|---|---|---|---|---|---|---|---|---|---|")
KB = sorted(B)
for g in GROUPS + ["ALL"]:
    ks = KB if g == "ALL" else [k for k in KB if k in set(group(g, B))]
    nl = [k for k in ks if ratio(B, k, "nplain", "lj")]
    P(f"| {g} | {len(nl)} | {g3(B,nl,'nplain','lj'):.3f} | {g3(B,nl,'npgo','lj'):.3f} | {g3(B,nl,'mplain','lj'):.3f} | {g3(B,nl,'mpgo','lj'):.3f} | "
      f"{g3(B,nl,'oplain','lj'):.3f} | {g3(B,nl,'opgo','lj'):.3f} | {g3(B,ks,'nplain','puc'):.3f} | {g3(B,ks,'npgo','puc'):.3f} | "
      f"{g3(B,ks,'nplain','mplain'):.3f} | {g3(B,ks,'npgo','mpgo'):.3f} | {g3(B,ks,'nplain','oplain'):.3f} | {g3(B,ks,'npgo','opgo'):.3f} |")


def per_prog(title, ks, R=A):
    P(f"\n### {title}\n")
    P("Mcyc min (plain, pgo, interp, PUC, LJ); ratios are cycles ours/other. c276 = plain/c276plain, c276p = pgo/c276pgo, cb1 = plain/cb1plain, cb1p = pgo/cb1pgo. "
      "ins = instruction ratio plain/c276 and plain/cb1.\n")
    P("| program | plain | pgo | interp | PUC | LJ | plain/PUC | plain/interp | plain/LJ | pgo/LJ | c276 | c276p | cb1 | cb1p | ins c276 | ins cb1 | core |")
    P("|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|")
    f = lambda v, d=3: "-" if v is None else f"{v:.{d}f}"
    m = lambda k, c: (lambda v: "-" if v is None else f"{v / 1e6:.0f}")(cyc(R, k, c))
    for k in ks:
        short = k.replace("corpus2/", "") if k.startswith("corpus2/") else k.split("/", 1)[1]
        P(f"| {short} | {m(k,'nplain')} | {m(k,'npgo')} | {m(k,'nint')} | {m(k,'puc')} | {m(k,'lj')} | {f(ratio(R,k,'nplain','puc'))} | "
          f"{f(ratio(R,k,'nplain','nint'))} | {f(ratio(R,k,'nplain','lj'))} | {f(ratio(R,k,'npgo','lj'))} | "
          f"{f(ratio(R,k,'nplain','mplain'))} | {f(ratio(R,k,'npgo','mpgo'))} | {f(ratio(R,k,'nplain','oplain'))} | {f(ratio(R,k,'npgo','opgo'))} | "
          f"{f(ratio(R,k,'nplain','mplain',ins))} | {f(ratio(R,k,'nplain','oplain',ins))} | {R[k]['cpu']} |")


P("\n# Per-program tables (pass A)")
per_prog("real set (bench/real)", [k for k in KEYS if k.startswith("real/")])
per_prog("real_dis_* ports (corpus2: dev arm, x86; heldout mips, ppc)", [k for k in KEYS if k.split("/")[-1] in REALDIS])
for g in ["corpus", "corpus2/dev", "corpus2/heldout", "awfy"]:
    per_prog(g, [k for k in KEYS if k.startswith(g + "/")])
P("\n### real + real_dis on fast cores, pass B (Mcyc min; ratios cycles)\n")
P("| program | plain | pgo | PUC | LJ | plain/PUC | plain/LJ | pgo/LJ | plain/c276 | pgo/c276pgo | plain/cb1 | pgo/cb1pgo |")
P("|---|---|---|---|---|---|---|---|---|---|---|---|")
f3 = lambda v: "-" if v is None else f"{v:.3f}"
for k in sorted(group("real + real_dis (9)", B)):
    mm = lambda c: (lambda v: "-" if v is None else f"{v / 1e6:.0f}")(cyc(B, k, c))
    P(f"| {k} | {mm('nplain')} | {mm('npgo')} | {mm('puc')} | {mm('lj')} | {f3(ratio(B,k,'nplain','puc'))} | {f3(ratio(B,k,'nplain','lj'))} | "
      f"{f3(ratio(B,k,'npgo','lj'))} | {f3(ratio(B,k,'nplain','mplain'))} | {f3(ratio(B,k,'npgo','mpgo'))} | {f3(ratio(B,k,'nplain','oplain'))} | {f3(ratio(B,k,'npgo','opgo'))} |")


def winlose(tag, base, pbase):
    chg = []
    for k in KEYS:
        r = ratio(A, k, "nplain", base)
        if r:
            ib = ins(A, k, "nplain") / ins(A, k, base) if ins(A, k, base) else float("nan")
            chg.append((r, k, ratio(A, k, "npgo", pbase), cyc(A, k, "nplain"), ib))
    chg.sort()
    P(f"\n## Top 10 winners vs {tag} (plain cycles ratio, lowest first)\n")
    P("| program | plain/base | pgo/basepgo | plain Mcyc | instr ratio |")
    P("|---|---|---|---|---|")
    for r, k, rp, c, i in chg[:10]:
        P(f"| {k} | {r:.3f} | {f3(rp)} | {c/1e6:.0f} | {i:.3f} |")
    P(f"\n## Top 10 losers vs {tag} (plain cycles ratio, highest first)\n")
    P("| program | plain/base | pgo/basepgo | plain Mcyc | instr ratio | tiny (<100 Mcyc) |")
    P("|---|---|---|---|---|---|")
    for r, k, rp, c, i in chg[::-1][:10]:
        P(f"| {k} | {r:.3f} | {f3(rp)} | {c/1e6:.0f} | {i:.3f} | {'yes' if c < 1e8 else ''} |")
    P(f"\n### Programs more than 5% slower than {tag} (plain or pgo, pass A), instruction ratios, serial re-run result where available\n")
    P("| program | plain/base cyc | plain instr | pgo/basepgo cyc | pgo instr | plain Mcyc | serial CPU4 x9 plain | serial pgo | dup serial |")
    P("|---|---|---|---|---|---|---|---|---|")
    for r, k, rp, c, i in chg[::-1]:
        if r > 1.05 or (rp and rp > 1.05):
            ip = (ins(A, k, "npgo") or 0) / (ins(A, k, pbase) or 1)
            sr = f3(ratio(RR, k, "nplain", base)) if k in RR else "-"
            sp = f3(ratio(RR, k, "npgo", pbase)) if k in RR else "-"
            sd = f3(ratio(RR, k, "nplain", "nplain2")) if k in RR else "-"
            P(f"| {k} | {r:.3f} | {i:.3f} | {f3(rp)} | {ip:.3f} | {c/1e6:.0f} | {sr} | {sp} | {sd} |")


winlose("c276c376", "mplain", "mpgo")
winlose("cb1af288", "oplain", "opgo")

P("\n## Duplicate-column noise: programs where plain/plain2 differs by >3% (pass A)\n")
for k in KEYS:
    d = ratio(A, k, "nplain", "nplain2")
    if d and abs(d - 1) > 0.03:
        P(f"- {k}: {d:.3f} (plain {cyc(A,k,'nplain')/1e6:.0f} Mcyc)")
P("\n## Output / .expected check\n")
bad = []
for R, tag in ((A, "A"), (B, "B")):
    for k in sorted(R):
        for c, e in R[k]["cols"].items():
            if c[0] in "nmo":
                if e.get("exp") == "MISMATCH" or e.get("out") == "diff" or e.get("err"):
                    bad.append(f"- pass {tag} {k} [{c}]: exp={e.get('exp')} out={e.get('out')} err={e.get('err')}")
P("\n".join(bad) if bad else "All ours columns (main, c276, cb1; plain and PGO; interp) equal PUC and .expected on every program, passes A and B.")
P("\nLuaJIT problems (not runnable / output differs from PUC beyond numeric tolerance):\n")
for k in KEYS:
    e = A[k]["cols"].get("lj", {})
    if e.get("err") or e.get("out") == "diff":
        P(f"- {k}: {e.get('err') or 'output differs from PUC'}")
os.makedirs(OUT, exist_ok=True)
open(os.path.join(OUT, "tables.md"), "w").write("\n".join(L) + "\n")
cn = ["puc", "lj", "nplain", "nplain2", "nint", "npgo", "mplain", "mpgo", "oplain", "opgo"]
with open(os.path.join(OUT, "per_program_passA.csv"), "w") as f:
    f.write("program,core," + ",".join(c + "_cyc_min" for c in cn) + "," + ",".join(c + "_ins_min" for c in cn) + "\n")
    for k in KEYS:
        f.write(f"{k},{A[k]['cpu']}," + ",".join(str(cyc(A, k, c) or "") for c in cn) + "," + ",".join(str(ins(A, k, c) or "") for c in cn) + "\n")
with open(os.path.join(OUT, "per_program_passB.csv"), "w") as f:
    cn = [c for c in cn if c not in ("nplain2", "nint")]
    f.write("program,core," + ",".join(c + "_cyc_min" for c in cn) + "," + ",".join(c + "_ins_min" for c in cn) + "\n")
    for k in sorted(B):
        f.write(f"{k},{B[k]['cpu']}," + ",".join(str(cyc(B, k, c) or "") for c in cn) + "," + ",".join(str(ins(B, k, c) or "") for c in cn) + "\n")
print("\n".join(L[:50]))
