#!/usr/bin/env python3
"""Compile-cost harness (docs/JIT_TRACE.md section 80.0). Use scripts/compile-cost.sh (memory cap, timeout, CPU pinning).

  compile_cost.py phases BIN[=NAME] ... [--programs set/prog,...] [--cpu 5] [--limits STR]
      one run per binary and program with RUSTYNAIL_JIT_PHASECTR=1 (the phase timers count user-mode instructions, deterministic at
      RUSTYNAIL_SEED=0) under `perf stat -e instructions:u`: total instructions, compile instructions (recorder steps with the pipeline
      they run + tier-1 compiles + deferred stubs) and their share, compiles, k instructions per trace pipeline and per phase.
  compile_cost.py cycles BIN[=NAME] ... [--programs ...] [--runs 15] [--cpu 5] [--limits STR]
      min of N interleaved runs of `perf stat -e instructions:u,cycles:u` (300 ms spin before each run), no counters.
  compile_cost.py whole BIN[=NAME] ... [--sets corpus,...] [--cpu 5]
      instructions:u of every program of the sets (one run each; instruction counts are deterministic) and the geomean ratio to the first.
"""
import glob, math, os, re, subprocess, sys, time

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
TARGETS = "apps/raytrace,awfy/cd,awfy/towers,awfy/json,lib/fmt_s"
SETS = "corpus,corpus2/dev,corpus2/heldout,awfy,programs,jit-trace,lib,apps,real"
PH = ["rec_step", "lockstep", "pass_sink", "pass_other", "ra", "exits", "asm", "x86", "install", "hash", "pipeline", "tier1",
      "lazystub", "front", "pipe_side", "res_env"]


def args_of(argv):
    bins, opt = [], {}
    k = 0
    while k < len(argv):
        a = argv[k]
        if a.startswith("--"):
            opt[a[2:]] = argv[k + 1]
            k += 2
        else:
            n = a.split("=", 1)
            bins.append((n[0], n[1] if len(n) > 1 else os.path.basename(os.path.dirname(n[0])) or n[0]))
            k += 1
    return bins, opt


def run(bin_, prog, cpu, events, extra_env, spin=False):
    env = dict(os.environ)
    env["RUSTYNAIL_SEED"] = "0"
    env.update(extra_env)
    if spin:
        t = time.time()
        while time.time() - t < 0.3:
            pass
    cmd = ["taskset", "-c", str(cpu), "perf", "stat", "-x,", "-e", events, bin_, os.path.join(ROOT, "bench", prog + ".lua")]
    p = subprocess.run(cmd, stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE, timeout=600,
                       cwd=os.path.join(ROOT, "bench", os.path.dirname(prog)), env=env)
    err = p.stderr.decode(errors="replace")
    res = {}
    for line in err.splitlines():
        f = line.split(",")
        if len(f) >= 3 and f[2].startswith(("instructions", "cycles")):
            try:
                res[f[2].split(":")[0]] = int(f[0])
            except ValueError:
                pass
        m = re.match(r"\[trace\] phases \(Minstr/calls\):(.*?) max_pipeline", line)
        if m:
            toks = m.group(1).split()
            for i in range(0, len(toks) - 1, 2):
                v, c = toks[i + 1].split("/")
                res["ph:" + toks[i]] = (float(v) * 1e6, int(c))
        m = re.search(r" sizes ins (\d+) snaps (\d+) snapents (\d+) ops (\d+) bytes (\d+) stubops (\d+) sharedops (\d+)", line)
        if m:
            res["sizes"] = [int(x) for x in m.groups()]
    res["rc"] = p.returncode
    return res


def phases(bins, opt):
    progs = opt.get("programs", TARGETS).split(",")
    cpu = opt.get("cpu", "5")
    lim = opt.get("limits")
    extra = {"RUSTYNAIL_JIT_PHASECTR": "1"}
    if lim:
        extra["RUSTYNAIL_JIT_TRACE_LIMITS"] = lim
    hdr = ("program", "bin", "instr M", "compile M", "share", "rec-only M", "pipe M", "n pipe", "k/pipe", "n side", "k/side",
           "tier1 M", "n t1", "stub M")
    print("| " + " | ".join(hdr) + " |")
    print("|" + "---|" * len(hdr))
    detail = []
    for prog in progs:
        for b, name in bins:
            r = run(b, prog, cpu, "instructions:u", extra)
            g = lambda k: r.get("ph:" + k, (0.0, 0))
            tot = r.get("instructions", 0)
            rec, pipe, t1, ls = g("rec_step")[0], g("pipeline"), g("tier1"), g("lazystub")
            # (the pipeline runs after the recorder step that closed the recording, outside its timer)
            comp = rec + pipe[0] + t1[0] + ls[0]
            side = g("pipe_side")
            np_ = max(pipe[1], 1)
            print(f"| {prog} | {name} | {tot / 1e6:.2f} | {comp / 1e6:.2f} | {100 * comp / max(tot, 1):.1f} % | {rec / 1e6:.2f} | "
                  f"{pipe[0] / 1e6:.2f} | {pipe[1]} | {pipe[0] / np_ / 1e3:.0f} | {side[1]} | {side[0] / max(side[1], 1) / 1e3:.0f} | "
                  f"{t1[0] / 1e6:.2f} | {t1[1]} | {ls[0] / 1e6:.2f} |")
            ph = {k: g(k)[0] / np_ / 1e3 for k in ("front", "ra", "exits", "res_env", "asm", "x86", "hash", "install")}
            rest = pipe[0] / np_ / 1e3 - sum(ph.values())
            sz = r.get("sizes", [0] * 7)
            nc = max(g("x86")[1], 1)
            detail.append((prog, name, ph, rest, pipe[0] / np_ / 1e3, [x / nc for x in sz]))
    print()
    ks = ("front", "ra", "exits", "res_env", "asm", "x86", "hash", "install")
    print("| program | bin | k/pipe | " + " | ".join(ks) + " | rest | IR ins | snaps | snap ents | micro-ops | bytes | stub ops | shared ops | ra+exits+asm+x86 per IR ins |")
    print("|" + "---|" * (len(ks) + 12))
    for prog, name, ph, rest, tot, sz in detail:
        be = (ph["ra"] + ph["exits"] + ph["asm"] + ph["x86"]) * 1e3 / max(sz[0], 1)
        print(f"| {prog} | {name} | {tot:.0f} | " + " | ".join(f"{ph[k]:.0f}" for k in ks) + f" | {rest:.0f} | "
              + " | ".join(f"{x:.0f}" for x in sz) + f" | {be:.0f} |")


def cycles(bins, opt):
    progs = opt.get("programs", TARGETS).split(",")
    cpu = opt.get("cpu", "5")
    runs = int(opt.get("runs", "15"))
    extra = {}
    if opt.get("limits"):
        extra["RUSTYNAIL_JIT_TRACE_LIMITS"] = opt["limits"]
    print("| program | " + " | ".join(n for _, n in bins) + " | " + " | ".join(f"{n} vs {bins[0][1]}" for _, n in bins[1:]) + " |")
    print("|" + "---|" * (2 * len(bins)))
    for prog in progs:
        cyc = {n: [] for _, n in bins}
        ins = {n: [] for _, n in bins}
        for i in range(runs):
            order = bins[i % len(bins):] + bins[:i % len(bins)]
            for b, n in order:
                r = run(b, prog, cpu, "instructions:u,cycles:u", extra, spin=True)
                if r["rc"] == 0:
                    cyc[n].append(r.get("cycles", 0))
                    ins[n].append(r.get("instructions", 0))
        base = min(cyc[bins[0][1]])
        cells = [f"{min(ins[n]) / 1e6:.2f} / {min(cyc[n]) / 1e6:.2f}" for _, n in bins]
        rel = [f"{100 * (min(cyc[n]) / base - 1):+.2f} %" for _, n in bins[1:]]
        print(f"| {prog} | " + " | ".join(cells) + " | " + " | ".join(rel) + " |", flush=True)


def whole(bins, opt):
    sets = opt.get("sets", SETS).split(",")
    cpu = opt.get("cpu", "5")
    progs = []
    for s in sets:
        progs += [f"{s}/{os.path.basename(x)[:-4]}" for x in sorted(glob.glob(os.path.join(ROOT, "bench", s, "*.lua")))]
    if opt.get("programs"):
        progs = opt["programs"].split(",")
    logs = {n: 0.0 for _, n in bins[1:]}
    cnt = 0
    worst = []
    for prog in progs:
        vals = []
        for b, n in bins:
            r = run(b, prog, cpu, "instructions:u", {})
            vals.append(r.get("instructions", 0) if r["rc"] == 0 else 0)
        if vals[0] == 0 or 0 in vals:
            print(f"{prog}: failed {vals}", flush=True)
            continue
        cnt += 1
        line = f"{prog}: " + " ".join(f"{v / 1e6:.2f}" for v in vals)
        for (b, n), v in zip(bins[1:], vals[1:]):
            d = v / vals[0]
            logs[n] += math.log(d)
            line += f" {n} {100 * (d - 1):+.2f} %"
            worst.append((100 * (d - 1), prog, n))
        print(line, flush=True)
    for n, l in logs.items():
        print(f"geomean {n} vs {bins[0][1]}: {100 * (math.exp(l / max(cnt, 1)) - 1):+.3f} % over {cnt} programs")
    worst.sort(reverse=True)
    print("largest increases:", ", ".join(f"{p} {n} {d:+.2f} %" for d, p, n in worst[:10]))


if __name__ == "__main__":
    mode = sys.argv[1]
    b, o = args_of(sys.argv[2:])
    {"phases": phases, "cycles": cycles, "whole": whole}[mode](b, o)
