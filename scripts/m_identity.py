#!/usr/bin/env python3
"""BUNDLE-M: do two settings of RUSTYNAIL_JIT_TRACE_LIMITS (or two binaries) record byte-identical traces?

  scripts/m_identity.py --bin rnlua [--bin2 rn-cli2] --a lockstep=1 --b lockstep=0 [--mode on|always] [--arg N]
                        [--jobs 3] [--keep dir] prog.lua [prog.lua ...]

Every program runs twice under `systemd-run` (8 GB cap, timeout, stdin from /dev/null) with RUSTYNAIL_JIT_DUMP=<dir>,
RUSTYNAIL_SEED=0: once with limits A (binary 1), once with limits B (binary 2, default binary 1). The dumps are compared after
normalising what legitimately differs between two processes: hex addresses in `trace-<n>.txt` (IR, snapshots, register allocation,
micro-ops), and in `trace-<n>.bin` (the machine code) the immediates of `movabs` (heap / helper addresses). A trace that exists on one
side only, a different text or a different length is a difference. The program output (stdout) and the `[trace]` statistics lines are
compared as well. Exit status 1 when anything differs. Prints one line per program and a summary.
"""
import argparse, os, re, shutil, subprocess, sys, tempfile, concurrent.futures as cf

HEX = re.compile(rb"0x[0-9a-fA-F]+")
MASK_POLL = False


def run(binp, limits, mode, prog, arg, dumpdir, extra_env):
    env = dict(os.environ)
    env.update(RUSTYNAIL_SEED="0", RUSTYNAIL_JIT_DUMP=dumpdir, RUSTYNAIL_JIT_TRACE_LIMITS=limits, RUSTYNAIL_JIT_VERBOSE="1")
    if mode:
        env["RUSTYNAIL_JIT"] = mode
        env["RUSTYNAIL_JIT_TRACE"] = mode if mode in ("always", "on") else "on"
    env.update(extra_env)
    cmd = ["systemd-run", "--user", "--scope", "-q", "-p", "MemoryMax=8G", "-p", "MemorySwapMax=0", "timeout", "300", binp, prog]
    if arg:
        cmd.append(arg)
    p = subprocess.run(cmd, env=env, stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    return p.returncode, p.stdout, p.stderr


def norm_txt(b):
    return HEX.sub(b"0xA", b)


def disasm(path):
    """Intel-syntax disassembly of a raw code dump, with the parts that depend on where the process put things masked: `movabs`
    immediates, `call` targets, `rip`-relative operands, and branch targets outside the dump."""
    out = subprocess.run(["objdump", "-D", "--no-show-raw-insn", "-b", "binary", "-mi386:x86-64", "-M", "intel", path],
                         stdout=subprocess.PIPE, check=True).stdout.decode(errors="replace")
    size = os.path.getsize(path)
    lines = []
    for l in out.splitlines():
        m = re.match(r"\s*([0-9a-f]+):\s+(\S+)\s*(.*)", l)
        if not m:
            continue
        off, op, rest = m.group(1), m.group(2), m.group(3)
        if MASK_POLL:
            # (K1/K2: the displacement of the interrupt poll word from r15 depends on the binary's static layout; 5+ hex digits is no state field)
            rest = re.sub(r"\[r15\+0x[0-9a-f]{5,}\]", "[r15+POLL]", rest)
        if op.startswith("movabs") or "rip" in rest or op == "call":
            rest = re.sub(r"0x[0-9a-f]+", "IMM", rest)
        elif op == "mov" and "DWORD PTR [" in rest:
            rest = re.sub(r",0x[0-9a-f]+$", ",IMM", rest)  # a pointer stored as two 32-bit halves
        elif op.startswith("j"):
            t = re.search(r"\b([0-9a-f]+)\b", rest)
            if t and int(t.group(1), 16) >= size:
                rest = "OUT"
        lines.append(f"{off}: {op} {rest}")
    return lines


def bins_equal(pa, pb):
    if os.path.getsize(pa) != os.path.getsize(pb):
        return False
    return disasm(pa) == disasm(pb)


def stat_lines(err):
    out = []
    for l in err.splitlines():
        # (`probe recordings: run N` exists only when the knob is on: how many probes ran is not a difference of the trace set)
        if l.startswith(b"[trace] ") and not l.startswith((b"[trace] phase", b"[trace] compile", b"[trace] probe recordings")) and b"Mtsc" not in l and b"tsc" not in l:
            out.append(l)
    return out


def compare(da, db):
    diffs = []
    fa = sorted(f for f in os.listdir(da) if f.startswith("trace-"))
    fb = sorted(f for f in os.listdir(db) if f.startswith("trace-"))
    if fa != fb:
        diffs.append(f"trace file sets differ: {len(fa)} vs {len(fb)}")
    for f in fa:
        if f not in fb:
            continue
        pa, pb = os.path.join(da, f), os.path.join(db, f)
        if f.endswith(".txt"):
            if norm_txt(open(pa, "rb").read()) != norm_txt(open(pb, "rb").read()):
                diffs.append(f"{f}: text differs")
        elif not bins_equal(pa, pb):
            diffs.append(f"{f}: machine code differs")
    return diffs, len(fa)


def one(a, prog):
    da, db = tempfile.mkdtemp(prefix="mid_a_", dir=a.tmp), tempfile.mkdtemp(prefix="mid_b_", dir=a.tmp)
    try:
        ra = run(a.bin, a.a, a.mode, prog, a.arg, da, {})
        rb = run(a.bin2 or a.bin, a.b, a.mode, prog, a.arg, db, {})
        diffs, n = compare(da, db)
        if ra[0] != rb[0]:
            diffs.append(f"exit status {ra[0]} vs {rb[0]}")
        if ra[1] != rb[1] and not a.ignore_stdout:
            diffs.append("stdout differs")
        sa, sb = stat_lines(ra[2]), stat_lines(rb[2])
        if sa != sb:
            diffs.append("trace statistics differ: " + "; ".join(sorted(set(x.decode(errors='replace') for x in sa) ^ set(x.decode(errors='replace') for x in sb))[:4]))
        if a.keep and diffs:
            for tag, d in (("a", da), ("b", db)):
                shutil.copytree(d, os.path.join(a.keep, os.path.basename(prog) + "." + tag), dirs_exist_ok=True)
        return prog, n, diffs
    finally:
        shutil.rmtree(da, ignore_errors=True)
        shutil.rmtree(db, ignore_errors=True)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--bin", required=True)
    ap.add_argument("--bin2")
    ap.add_argument("--a", default="lockstep=1")
    ap.add_argument("--b", default="lockstep=0")
    ap.add_argument("--mode", default="")
    ap.add_argument("--arg", default="")
    ap.add_argument("--jobs", type=int, default=3)
    ap.add_argument("--keep", default="")
    ap.add_argument("--tmp", default=os.path.expanduser("~/.cache/rnm/tmp"))
    ap.add_argument("--ignore-stdout", action="store_true")
    ap.add_argument("--mask-poll", action="store_true", help="mask the [r15+poll word] displacement (two different binaries)")
    ap.add_argument("progs", nargs="+")
    a = ap.parse_args()
    global MASK_POLL
    MASK_POLL = a.mask_poll
    os.makedirs(a.tmp, exist_ok=True)
    if a.keep:
        os.makedirs(a.keep, exist_ok=True)
    bad = 0
    total_traces = 0
    with cf.ThreadPoolExecutor(a.jobs) as ex:
        for prog, n, diffs in ex.map(lambda p: one(a, p), a.progs):
            total_traces += n
            print(f"{'DIFF' if diffs else 'same'} {os.path.basename(prog):28s} traces {n:4d} {'; '.join(diffs[:3])}", flush=True)
            bad += bool(diffs)
    print(f"compared {len(a.progs)} programs, {total_traces} traces (A={a.a} B={a.b}): {'ALL IDENTICAL' if not bad else str(bad) + ' DIFFER'}")
    sys.exit(1 if bad else 0)


main()
