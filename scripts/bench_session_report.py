#!/usr/bin/env python3
"""Tables for the session benchmark (scripts/bench_session.py JSON).
usage: bench_session_report.py passA.json [passB.json (fast-core LuaJIT pass)] outdir
Writes outdir/tables.md and outdir/per_program.csv."""
import json, math, sys, os

A = json.load(open(sys.argv[1]))["results"]
B = json.load(open(sys.argv[2]))["results"] if len(sys.argv) > 3 else None
OUT = sys.argv[-1]
SETS = ["corpus", "corpus2/dev", "corpus2/heldout", "awfy", "programs", "jit-trace", "lib", "apps", "real"]
REALDIS = {"real_dis_x86", "real_dis_arm", "real_dis_mips", "real_dis_ppc"}


def cyc(R, k, col, stat=min):
    c = R.get(k, {}).get("cols", {}).get(col, {})
    v = [x for x in c.get("cyc", []) if x]
    return stat(v) if v else None


def ins(R, k, col):
    v = [x for x in R.get(k, {}).get("cols", {}).get(col, {}).get("ins", []) if x]
    return min(v) if v else None


def gm(xs):
    xs = [x for x in xs if x and x > 0]
    return math.exp(sum(math.log(x) for x in xs) / len(xs)) if xs else float("nan")


def ratio(R, k, a, b):
    x, y = cyc(R, k, a), cyc(R, k, b)
    return x / y if x and y else None


def group(name):
    if name == "real+real_dis (9)":
        return [k for k in A if k.startswith("real/") or k.split("/")[-1] in REALDIS]
    if name == "real_dis dev (2)":
        return [k for k in A if k.startswith("corpus2/dev/real_dis")]
    if name == "real_dis heldout (2)":
        return [k for k in A if k.startswith("corpus2/heldout/real_dis")]
    return [k for k in A if k.startswith(name + "/")]


GROUPS = SETS + ["real_dis dev (2)", "real_dis heldout (2)", "real+real_dis (9)"]
KEYS = sorted(A)
lines = []
P = lines.append


def cols_ratio(R, ks, a, b):
    return gm([ratio(R, k, a, b) for k in ks])


P("## Headline geomeans (cycles:u min of 5 interleaved runs; ratio < 1 means ours is faster / the change is a win)\n")
P("Pass A = CPUs 3-7 (one program per core, all columns on the same core). Columns: plain = default release build with jit; pgo = PGO build (scripts/pgo-build.sh); "
  "interp = plain binary with RUSTYNAIL_JIT=off; cb1 = cb1af288 plain, cb1pgo = cb1af288 PGO.\n")
P("| set | n | plain/PUC | pgo/PUC | plain/interp | plain/LJ (n) | pgo/LJ | plain vs cb1 | pgo vs cb1pgo | dup noise (plain/plain2) |")
P("|---|---|---|---|---|---|---|---|---|---|")
rows = {}
for g in GROUPS + ["ALL"]:
    ks = KEYS if g == "ALL" else group(g)
    nlj = [k for k in ks if ratio(A, k, "nplain", "lj")]
    r = dict(n=len(ks), puc=cols_ratio(A, ks, "nplain", "puc"), pgopuc=cols_ratio(A, ks, "npgo", "puc"),
             interp=cols_ratio(A, ks, "nplain", "nint"), lj=cols_ratio(A, nlj, "nplain", "lj"), pgolj=cols_ratio(A, nlj, "npgo", "lj"),
             nlj=len(nlj), vs=cols_ratio(A, ks, "nplain", "oplain"), vspgo=cols_ratio(A, ks, "npgo", "opgo"),
             dup=cols_ratio(A, ks, "nplain", "nplain2"))
    rows[g] = r
    P(f"| {g} | {r['n']} | {r['puc']:.3f} | {r['pgopuc']:.3f} | {r['interp']:.3f} | {r['lj']:.3f} ({r['nlj']}) | {r['pgolj']:.3f} | {r['vs']:.3f} | {r['vspgo']:.3f} | {r['dup']:.3f} |")

if B:
    P("\n## ours/LuaJIT on fast Zen5 cores only (pass B: CPUs 4, 6; all columns of a program on the same core)\n")
    P("| set | n with LJ | plain/LJ | pgo/LJ | cb1/LJ | cb1pgo/LJ | plain/PUC | plain vs cb1 | pgo vs cb1pgo |")
    P("|---|---|---|---|---|---|---|---|---|")
    KB = sorted(B)
    for g in GROUPS + ["ALL"]:
        ks = KB if g == "ALL" else [k for k in KB if k in set(group(g))]
        nl = [k for k in ks if ratio(B, k, "nplain", "lj")]
        P(f"| {g} | {len(nl)} | {cols_ratio(B, nl, 'nplain', 'lj'):.3f} | {cols_ratio(B, nl, 'npgo', 'lj'):.3f} | {cols_ratio(B, nl, 'oplain', 'lj'):.3f} | "
          f"{cols_ratio(B, nl, 'opgo', 'lj'):.3f} | {cols_ratio(B, ks, 'nplain', 'puc'):.3f} | {cols_ratio(B, ks, 'nplain', 'oplain'):.3f} | {cols_ratio(B, ks, 'npgo', 'opgo'):.3f} |")


def per_prog(title, ks, R=A):
    P(f"\n### {title}\n")
    P("Mcyc min; ratios are cycles (ours over the other column). `chg` = plain/cb1, `chg pgo` = pgo/cb1pgo. `!` = see flags.\n")
    P("| program | plain | pgo | interp | PUC | LJ | plain/PUC | plain/interp | plain/LJ | pgo/LJ | chg | chg pgo | core |")
    P("|---|---|---|---|---|---|---|---|---|---|---|---|---|")
    f = lambda v, d=3: "-" if v is None else f"{v:.{d}f}"
    m = lambda k, c: (lambda v: "-" if v is None else f"{v / 1e6:.0f}")(cyc(R, k, c))
    for k in ks:
        short = k.split("/", 1)[1] if "/" in k and k.split("/")[0] != "corpus2" else k.replace("corpus2/", "")
        P(f"| {short} | {m(k,'nplain')} | {m(k,'npgo')} | {m(k,'nint')} | {m(k,'puc')} | {m(k,'lj')} | {f(ratio(R,k,'nplain','puc'))} | "
          f"{f(ratio(R,k,'nplain','nint'))} | {f(ratio(R,k,'nplain','lj'))} | {f(ratio(R,k,'npgo','lj'))} | {f(ratio(R,k,'nplain','oplain'))} | "
          f"{f(ratio(R,k,'npgo','opgo'))} | {R[k]['cpu']} |")


for g in ["corpus", "corpus2/dev", "corpus2/heldout", "awfy", "real"]:
    per_prog(g, [k for k in KEYS if k.startswith(g + "/")])
per_prog("real_dis_* ports (corpus2)", [k for k in KEYS if k.split("/")[-1] in REALDIS])

# winners / losers vs cb1 (plain), min and median agree check
chg = []
for k in KEYS:
    r = ratio(A, k, "nplain", "oplain")
    if r:
        chg.append((r, k, ratio(A, k, "npgo", "opgo"), cyc(A, k, "nplain"), ins(A, k, "nplain"), ins(A, k, "oplain")))
chg.sort()
P("\n## Top 10 winners vs cb1af288 (plain/cb1 cycles, lowest first)\n")
P("| program | plain/cb1 | pgo/cb1pgo | plain Mcyc | instr ratio plain/cb1 |")
P("|---|---|---|---|---|")
for r, k, rp, c, i1, i0 in chg[:10]:
    P(f"| {k} | {r:.3f} | {rp and f'{rp:.3f}'} | {c/1e6:.0f} | {i1 / i0:.3f} |")
P("\n## Top 10 losers vs cb1af288 (plain/cb1 cycles, highest first)\n")
P("| program | plain/cb1 | pgo/cb1pgo | plain Mcyc | instr ratio plain/cb1 | tiny (<100 Mcyc) |")
P("|---|---|---|---|---|---|")
for r, k, rp, c, i1, i0 in chg[::-1][:10]:
    P(f"| {k} | {r:.3f} | {rp and f'{rp:.3f}'} | {c/1e6:.0f} | {i1 / i0:.3f} | {'yes' if c < 1e8 else ''} |")
P("\n## Programs > 5% slower than cb1af288 (plain or pgo), with instruction ratios\n")
P("| program | plain/cb1 cyc | plain/cb1 instr | pgo/cb1pgo cyc | pgo/cb1pgo instr | plain Mcyc |")
P("|---|---|---|---|---|---|")
for r, k, rp, c, i1, i0 in chg[::-1]:
    ip = (ins(A, k, "npgo") or 0) / (ins(A, k, "opgo") or 1)
    if r > 1.05 or (rp and rp > 1.05):
        P(f"| {k} | {r:.3f} | {i1 / i0:.3f} | {rp and f'{rp:.3f}'} | {ip:.3f} | {c/1e6:.0f} |")
P("\n## Duplicate-column noise: programs where plain/plain2 differs by >3%\n")
for k in KEYS:
    d = ratio(A, k, "nplain", "nplain2")
    if d and abs(d - 1) > 0.03:
        P(f"- {k}: {d:.3f} (plain {cyc(A,k,'nplain')/1e6:.0f} Mcyc)")
P("\n## Output / .expected check\n")
bad = []
for k in KEYS:
    for c, e in A[k]["cols"].items():
        if c.startswith("n") or c.startswith("o"):
            if e.get("exp") == "MISMATCH" or e.get("out") == "diff" or e.get("err"):
                bad.append(f"- {k} [{c}]: exp={e.get('exp')} out={e.get('out')} err={e.get('err')}")
P("\n".join(bad) if bad else "All ours/cb1 columns equal PUC and .expected on every program.")
P("\nLuaJIT problems (not runnable / output differs from PUC beyond numeric tolerance):\n")
for k in KEYS:
    e = A[k]["cols"].get("lj", {})
    if e.get("err") or e.get("out") == "diff":
        P(f"- {k}: {e.get('err') or 'output differs from PUC'}")
os.makedirs(OUT, exist_ok=True)
open(os.path.join(OUT, "tables.md"), "w").write("\n".join(lines) + "\n")
with open(os.path.join(OUT, "per_program.csv"), "w") as f:
    cn = ["puc", "lj", "nplain", "nplain2", "nint", "npgo", "oplain", "opgo"]
    f.write("program,core," + ",".join(c + "_cyc_min" for c in cn) + "," + ",".join(c + "_ins_min" for c in cn) + "\n")
    for k in KEYS:
        f.write(f"{k},{A[k]['cpu']}," + ",".join(str(cyc(A, k, c) or "") for c in cn) + "," + ",".join(str(ins(A, k, c) or "") for c in cn) + "\n")
print("\n".join(lines[:30]))
