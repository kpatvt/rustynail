#!/usr/bin/env python3
"""Formation-step study (docs/STUDY_FORMATION.md): phase split of perf leaf samples, for one run or the warm-up of a program.

  scripts/formation-phases.py record OUT.json CPU REPS N -- <cmd...>
      runs `perf record -e cycles:u -c PERIOD` (PERIOD from FP_PERIOD, default 100003: shorter periods hit the
      kernel's 100 k samples/s throttle and lose samples) N times on CPU (300 ms spin before each run,
      CAUSAL_REPS=REPS in the environment, stdin /dev/null, RUSTYNAIL_JIT_PERFMAP=2, RUSTYNAIL_SEED=0) and stores, per run, the
      sample sequence classified into phases (one letter per sample) plus the first sample index of every trace symbol.
  scripts/formation-phases.py sum OUT.json            median Mcyc per phase over the runs
  scripts/formation-phases.py warm ONE.json THREE.json  warm-up by phase: rep1 - (rep3 - rep1) / 2 (the run-it-twice method)
  scripts/formation-phases.py timeline OUT.json W     phase shares per window of W Mcyc of the (first) run, cumulative
  scripts/formation-phases.py lj ...                  LuaJIT: samples in JIT machine code / libluajit / libc / other

Phases (leaf symbol of each sample; no call chains: dwarf unwinding of this binary through JIT frames gives nothing):
  I interpreter (run_mode<false>)            R recording (run_mode<true> = the hooked interpreter, rec_step, tr::record, tr::snap)
  1 tier-1 machine code (lua_proto:*)        T root / U side trace code (TRACE-<n>r / TRACE-<n>s; the shipped
    perf map names every trace TRACE-<n>, which counts as T)        S exit stubs (TRACE-n-STUB)
  X exit handler (tr::exit restore / materialise / exit_entry / jit_tr_mat / jit_tr_exit, rt::enter / rearm_after_exit)
  B trace back end (passes, ra, asm, exit builder, install, encoder of traces; x86 encoder shared with tier 1 -> E)
  E x86 encoder (shared by tier 1 and traces)  C tier-1 compile (lower, templates, t15, live)
  F formation control and compile-side Rust support (tr::cache other than the runtime entries, tr::ir, hashbrown, alloc, fmt)
  H runtime helpers called by JIT code (jit_helpers, tr::asm::jit_tr_*)   G collector (heap, gc_step)
  K tables / strings / state runtime (rn_core other)   L library (rn_std)   M libc (malloc, free, memcpy, unnamed libc)
  P load / parse (rn_compiler)   O other
"""
import json, os, re, subprocess, sys, tempfile, time

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
PH = "IR1TUSXBECFHGKLMPO"
NAMES = {"I": "interp", "R": "record", "1": "tier1 code", "T": "root trace code", "U": "side trace code", "S": "exit stubs", "X": "exit handler",
         "B": "trace back end", "E": "x86 encoder", "C": "tier-1 compile", "F": "formation/compile support",
         "H": "JIT helpers", "G": "GC", "K": "core runtime", "L": "library", "M": "libc", "P": "load/parse", "O": "other"}

EXIT_FN = ("exit_entry", "materialise", "jit_tr_mat", "slot_value", "restore", "restore_chain", "jit_tr_exit", "replay_fill",
           "replay_copy", "frame_mat")


def classify(sym, dso):
    if dso.startswith("[JIT]") or dso.startswith("/tmp/perf-") or sym.startswith("TRACE-") or sym.startswith("lua_proto"):
        if sym.startswith("TRACE-"):
            if sym.endswith("-STUB"):
                return "S"
            return "U" if sym.endswith("s") else "T"
        if sym.startswith("lua_proto"):
            return "1"
        return "O"
    s = sym
    if "run_mode::<false>" in s:
        return "I"
    if "run_mode::<true>" in s or "jit_hooks::rec_step" in s or "rn_jit::tr::record" in s or "rn_jit::tr::snap" in s:
        return "R"
    if "rn_jit::tr::exit::" in s:
        tail = s.split("rn_jit::tr::exit::")[1]
        if any(tail.startswith(f) for f in EXIT_FN):
            return "X"
        return "B"
    if "rn_jit::rt::enter" in s or "rearm_after_exit" in s:
        return "X"
    if re.search(r"rn_jit::tr::asm::[a-z0-9_]*_core\b", s) or "rn_jit::tr::asm::jit_" in s or ("rn_jit::tr::exit::jit_" in s and "jit_tr_mat" not in s and "jit_tr_exit" not in s):
        return "H"
    if "rn_jit::tr::asm::jit_tr_" in s or "rn_core::jit_helpers" in s or "rn_jit::rt::" in s or "rn_jit::calls" in s:
        return "H"
    if "rn_jit::x86" in s or "rn_jit::arch::x86_64::asm" in s:
        return "E"
    if any(k in s for k in ("rn_jit::tr::ra", "rn_jit::tr::asm", "rn_jit::tr::sink", "rn_jit::tr::opt_", "rn_jit::tr::fold",
                            "rn_jit::tr::presize", "rn_jit::tr::shape", "shape_hash_ir", "rn_jit::tr::cache::install",
                            "rn_jit::tr::pipeline", "rn_jit::tr::entry_checks", "rn_jit::tr::split_type_snaps",
                            "rn_jit::tr::interp", "rn_jit::tr::print")):
        return "B"
    if any(k in s for k in ("rn_jit::lower", "rn_jit::templates", "rn_jit::t1inline", "rn_jit::t15", "rn_jit::live",
                            "rn_jit::emu", "lower::Cx", "rn_jit::mem")):
        return "C"
    if "rn_jit::" in s or "hashbrown" in s or "alloc::" in s or s.startswith("core::") or "<core::" in s or "drop_glue" in s:
        return "F"
    if "rn_core::heap" in s or "gc_step" in s or "rn_core::gc" in s or "Heap>::" in s:
        return "G"
    if "rn_compiler" in s or "rn_core::lex" in s or "rn_core::parse" in s:
        return "P"
    if "rn_core" in s:
        return "K"
    if "rn_std" in s:
        return "L"
    if "libc.so" in dso or "ld-linux" in dso or s in ("malloc", "cfree", "realloc", "__libc_calloc"):
        return "M"
    return "O"


LINE = re.compile(r"^\s*([0-9a-f]+)\s+(.*?)\s+\((.*)\)\s*$")


def parse_script(text):
    syms, ix, idx = [], {}, []
    for ln in text.splitlines():
        m = LINE.match(ln)
        if not m:
            continue
        key = m.group(2) + "\t" + m.group(3)
        if key not in ix:
            ix[key] = len(syms)
            syms.append(key)
        idx.append(ix[key])
    return syms, idx


def classified(d):
    """Adds `seq` (one phase letter per sample) and `first` (first sample index of each trace) to every run."""
    for r in d["runs"]:
        if "seq" in r:
            continue
        cl = [classify(*k.split("\t", 1)) for k in r["syms"]]
        r["seq"] = "".join(cl[i] for i in r["idx"])
        first = {}
        for n, i in enumerate(r["idx"]):
            if cl[i] in "TUS":
                first.setdefault(r["syms"][i].split("\t")[0].split("-STUB")[0], n)
        r["first"] = first
    return d


def spin(cpu, ms=300):
    subprocess.run(["taskset", "-c", str(cpu), "python3", "-c",
                    f"import time\nt=time.time()\nwhile time.time()-t<{ms / 1000}: pass"], stdin=subprocess.DEVNULL)


def record(out, cpu, reps, n, cmd):
    period = int(os.environ.get("FP_PERIOD", "100003"))
    env = {k: v for k, v in os.environ.items() if not k.startswith("RUSTYNAIL_")}
    env.update(RUSTYNAIL_SEED="0", RUSTYNAIL_JIT_PERFMAP="2", CAUSAL_REPS=str(reps))
    for k, v in os.environ.items():
        if k.startswith("RUSTYNAIL_") and k not in ("RUSTYNAIL_SEED",):
            env[k] = v
    tmp = tempfile.mkdtemp(prefix="fph", dir=os.environ.get("FP_TMP", "/tmp"))
    runs = []
    for i in range(n):
        spin(cpu)
        data = os.path.join(tmp, "p.data")
        subprocess.run(["perf", "record", "-q", "-o", data, "-e", "cycles:u", "-c", str(period), "--", "taskset", "-c", str(cpu)]
                       + cmd, stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, env=env, cwd=ROOT,
                       timeout=600)
        txt = subprocess.run(["perf", "script", "-i", data, "-F", "ip,sym,dso", "--no-inline"], stdout=subprocess.PIPE,
                             stderr=subprocess.DEVNULL, text=True, timeout=600).stdout
        syms, idx = parse_script(txt)
        runs.append({"syms": syms, "idx": idx})
        os.remove(data)
    for f in os.listdir("/tmp"):
        if f.startswith("perf-") and f.endswith(".map"):
            try:
                if os.stat(os.path.join("/tmp", f)).st_uid == os.getuid():
                    os.remove(os.path.join("/tmp", f))
            except OSError:
                pass
    os.rmdir(tmp)
    json.dump({"period": period, "cmd": cmd, "reps": reps, "runs": runs}, open(out, "w"))


def med(v):
    v = sorted(v)
    return v[len(v) // 2] if len(v) % 2 else (v[len(v) // 2 - 1] + v[len(v) // 2]) / 2


def phase_mcyc(d):
    p = d["period"] / 1e6
    return {c: med([r["seq"].count(c) * p for r in d["runs"]]) for c in PH}, med([len(r["seq"]) * p for r in d["runs"]])


def ctx_seq(seq):
    """M (libc) and F (Rust support) samples take the phase of the nearest preceding primary sample (compile-side allocation
    and hashing are called from the recorder / back end / tier-1 compile, runtime allocation from the code that runs)."""
    out, last = [], "O"
    for c in seq:
        if c in "MF":
            out.append(last)
        else:
            out.append(c)
            if c not in "KGO":
                last = c
    return "".join(out)


def mean_mcyc(d):
    if CTX:
        d = dict(d, runs=[dict(r, seq=ctx_seq(r["seq"])) for r in d["runs"]])
    p = d["period"] / 1e6
    n = len(d["runs"])
    return {c: sum(r["seq"].count(c) for r in d["runs"]) * p / n for c in PH}, sum(len(r["seq"]) for r in d["runs"]) * p / n


def cmd_sum(path):
    ph, tot = mean_mcyc(classified(json.load(open(path))))
    print(f"total {tot:.1f} Mcyc (mean of runs)")
    for c in PH:
        if ph[c] > 0.05:
            print(f"  {NAMES[c]:28s} {ph[c]:8.2f}  {100 * ph[c] / tot:5.1f} %")


def cmd_warm(p1, p3):
    a, ta = mean_mcyc(classified(json.load(open(p1))))
    b, tb = mean_mcyc(classified(json.load(open(p3))))
    print(f"rep1 {ta:.1f}  rep3-total {tb:.1f}  steady/rep {(tb - ta) / 2:.1f}  warm-up {ta - (tb - ta) / 2:.1f} Mcyc")
    print("| phase | rep 1 | steady rep | warm-up |")
    for c in PH:
        st = (b[c] - a[c]) / 2
        w = a[c] - st
        if abs(a[c]) > 0.05 or abs(st) > 0.05:
            print(f"| {NAMES[c]} | {a[c]:.2f} | {st:.2f} | {w:+.2f} |")


def cmd_timeline(path, w):
    d = classified(json.load(open(path)))
    p = d["period"] / 1e6
    per = max(1, int(round(float(w) / p)))
    groups = [("interp", "I"), ("record", "R"), ("tier1", "1"), ("trace", "T"), ("exit", "SX"), ("compile", "BECF"),
              ("helpers", "H"), ("gc+rt", "GKLM"), ("other", "PO")]
    n = len(d["runs"])
    L = min(len(r["seq"]) for r in d["runs"])
    print("| window (Mcyc) | " + " | ".join(g for g, _ in groups) + " |")
    print("|---|" + "---:|" * len(groups))
    for s in range(0, L, per):
        cnt = {g: 0 for g, _ in groups}
        tot = 0
        for r in d["runs"]:
            seg = r["seq"][s:s + per]
            tot += len(seg)
            for g, cs in groups:
                cnt[g] += sum(seg.count(c) for c in cs)
        print(f"| {s * p:.0f}-{(s + per) * p:.0f} | " + " | ".join(f"{100 * cnt[g] / max(tot, 1):.0f}" for g, _ in groups) + " |")
    fs = sorted((v * p, k) for k, v in d["runs"][0]["first"].items())
    print("first sample of each trace (run 0, Mcyc):", " ".join(f"{k.split('-')[1]}@{t:.1f}" for t, k in fs))


CTX = False


def main():
    global CTX
    a = sys.argv[1:]
    if not a:
        print(__doc__)
        return
    if "--ctx" in a:
        a.remove("--ctx")
        CTX = True
    if a[0] == "record":
        i = a.index("--")
        record(a[1], int(a[2]), int(a[3]), int(a[4]), a[i + 1:])
    elif a[0] == "sum":
        cmd_sum(a[1])
    elif a[0] == "warm":
        cmd_warm(a[1], a[2])
    elif a[0] == "timeline":
        cmd_timeline(a[1], a[2])
    elif a[0] == "lj":
        cmd_lj(a[1], a[2] if len(a) > 2 else None)


def lj_class(key):
    sym, dso = key.split("\t", 1)
    if "libluajit" in dso or dso.endswith("/luajit") or "luajit-2.1" in dso:
        return "V"  # interpreter + compiler + runtime + GC (stripped: split with -jp=v)
    if "libc.so" in dso or "libm.so" in dso or "ld-linux" in dso:
        return "M"
    if dso in ("[unknown]", "") or "anon" in dso or "[JIT]" in dso or dso.startswith("/tmp/perf-"):
        return "T"  # trace machine code (LuaJIT's mcode area has no symbols)
    return "O"


def cmd_lj(path, w=None):
    """LuaJIT run: Mcyc in trace machine code (T), libluajit (V), libc/libm (M), other (O); with W: per window of W Mcyc."""
    d = json.load(open(path))
    p = d["period"] / 1e6
    n = len(d["runs"])
    seqs = []
    for r in d["runs"]:
        cl = [lj_class(k) for k in r["syms"]]
        seqs.append("".join(cl[i] for i in r["idx"]))
    tot = sum(len(s) for s in seqs) * p / n
    print(f"total {tot:.1f} Mcyc; " + ", ".join(f"{c} {sum(s.count(c) for s in seqs) * p / n:.2f}" for c in "TVMO"))
    if w:
        per = max(1, int(round(float(w) / p)))
        L = min(len(s) for s in seqs)
        print("| window (Mcyc) | trace % | libluajit % | libc % |")
        for s0 in range(0, L, per):
            seg = "".join(s[s0:s0 + per] for s in seqs)
            print(f"| {s0 * p:.0f}-{(s0 + per) * p:.0f} | " + " | ".join(f"{100 * seg.count(c) / max(len(seg), 1):.0f}" for c in "TVM") + " |")


if __name__ == "__main__":
    main()
