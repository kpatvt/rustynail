#!/usr/bin/env python3
"""bm3 extras: (1) ours/LJ per set for session-2026-11-11 (79e7441c), bm2 (80f1d96e as measured), bm3 (1e6ec4ca); common-program geomeans.
(2) knob attribution table from knobs_attribution_cpu4_15runs.json. usage: extra.py <results-dir>"""
import json, math, os, sys
R = sys.argv[1]
def load(d, f): return json.load(open(os.path.join(R, d, f)))["results"]
B3 = load("session-bm3", "passB_cpus4-6_fast_lj.json"); B2 = load("session-bm2", "passB_cpus4-6_fast_lj.json"); B0 = load("session-2026-11-11", "passB_cpus4-6_fast_lj.json")
REALDIS = {"real_dis_x86", "real_dis_arm", "real_dis_mips", "real_dis_ppc"}
def v(Rr, k, c, f):
    x = [y for y in Rr.get(k, {}).get("cols", {}).get(c, {}).get(f, []) if y]
    return min(x) if x else None
def rat(Rr, k, a, b, f="cyc"):
    x, y = v(Rr, k, a, f), v(Rr, k, b, f)
    return x / y if x and y else None
def gm(xs):
    xs = [x for x in xs if x]
    return math.exp(sum(math.log(x) for x in xs) / len(xs)) if xs else float("nan")
def pick(name, keys):
    isrd = lambda k: k.split("/")[-1] in REALDIS
    if name == "ALL": return list(keys)
    if name == "real_dis (4)": return [k for k in keys if isrd(k)]
    if name == "real (5)": return [k for k in keys if k.startswith("real/")]
    if name == "corpus2 dev": return [k for k in keys if k.startswith("corpus2/dev/")]
    if name == "corpus2 heldout": return [k for k in keys if k.startswith("corpus2/heldout/")]
    if name == "corpus (old)": return [k for k in keys if k.startswith("corpus/")]
    if name == "ALL excl real/real_dis": return [k for k in keys if not isrd(k) and not k.startswith("real/")]
    return [k for k in keys if k.startswith(name + "/")]
SETS = ["ALL", "ALL excl real/real_dis", "corpus (old)", "corpus2 dev", "corpus2 heldout", "awfy", "programs", "jit-trace", "lib", "apps", "real (5)", "real_dis (4)"]
print("## Ours/LuaJIT geomean cycles (pass B), three sessions, programs present in all three with a LuaJIT variant\n")
print("| set | n | 79e7441c plain | 80f1d96e plain (bm2) | 1e6ec4ca plain (bm3) | 79e7441c PGO | 80f1d96e PGO (bm2) | 1e6ec4ca PGO (bm3) |")
print("|---|---|---|---|---|---|---|---|")
common = [k for k in B3 if k in B2 and k in B0 and rat(B3, k, "nplain", "lj") and rat(B2, k, "nplain", "lj") and rat(B0, k, "nplain", "lj")]
for s in SETS:
    ks = pick(s, common)
    print(f"| {s} | {len(ks)} | {gm([rat(B0,k,'nplain','lj') for k in ks]):.3f} | {gm([rat(B2,k,'nplain','lj') for k in ks]):.3f} | {gm([rat(B3,k,'nplain','lj') for k in ks]):.3f} | "
          f"{gm([rat(B0,k,'npgo','lj') for k in ks]):.3f} | {gm([rat(B2,k,'npgo','lj') for k in ks]):.3f} | {gm([rat(B3,k,'npgo','lj') for k in ks]):.3f} |")
# LuaJIT drift
print("\n## LuaJIT-column drift bm3 vs bm2 (pass B cycles, ratio now/then), programs moving more than 8%\n")
for k in sorted(B3):
    r = rat({**{k: {"cols": {"a": B3[k]["cols"]["lj"], "b": B2[k]["cols"]["lj"]}}}}, k, "a", "b") if k in B2 and "lj" in B3[k]["cols"] and "lj" in B2[k]["cols"] else None
    if r and abs(r - 1) > 0.08:
        print(f"- {k}: LJ {r:.3f} (cores {B3[k]['cpu']} vs {B2[k]['cpu']}); ours plain bm3/bm2 {rat({k:{'cols':{'a':B3[k]['cols']['nplain'],'b':B2[k]['cols']['nplain']}}},k,'a','b'):.3f}")
p = os.path.join(R, "session-bm3", "knobs_attribution_cpu4_15runs.json")
if os.path.exists(p):
    K = json.load(open(p))["results"]
    cols = ["base", "base2", "libcall7", "t1q1amt0", "retf0", "rearm0", "alloff", "prev", "pbase", "palloff", "pprev"]
    print("\n## Knob attribution (CPU 4, 15 runs, min cycles in Mcyc; instructions in Minstr in the second line group)\n")
    print("base = 1e6ec4ca plain default; libcall7 = libcall=7; t1q1amt0 = t1q=1,amt=0; retf0 = retf=0,sidenest=0,tfmt=0; rearm0 = rearmside=0; alloff = all of these; prev = 80f1d96e plain; pbase / palloff = 1e6ec4ca PGO default / all knobs off; pprev = 80f1d96e PGO\n")
    print("| program | " + " | ".join(cols) + " | base/prev | alloff/prev | pbase/pprev | palloff/pprev |")
    print("|---|" + "---|" * (len(cols) + 4))
    for k in sorted(K):
        mc = [v(K, k, c, "cyc") for c in cols]
        f = lambda x: f"{x/1e6:.0f}" if x else "-"
        d = dict(zip(cols, mc))
        r = lambda a, b: f"{d[a]/d[b]:.3f}" if d[a] and d[b] else "-"
        print(f"| {k} | " + " | ".join(f(x) for x in mc) + f" | {r('base','prev')} | {r('alloff','prev')} | {r('pbase','pprev')} | {r('palloff','pprev')} |")
    print("\nInstruction ratios (base/prev, alloff/prev, pbase/pprev, palloff/pprev):\n")
    for k in sorted(K):
        ii = lambda a, b: f"{rat(K,k,a,b,'ins'):.3f}" if rat(K, k, a, b, "ins") else "-"
        print(f"- {k}: {ii('base','prev')} {ii('alloff','prev')} {ii('pbase','pprev')} {ii('palloff','pprev')}")
