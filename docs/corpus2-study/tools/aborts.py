#!/usr/bin/env python3
"""Per program: VERBOSE=3 + EXITDUMP=1 run (default mode, seed 0). Summaries:
 - recording aborts by (reason, op, depth, head line)  (policy reasons separated)
 - tier-1 exits weighted by interpreter stay (exits * cyc/stay, Mcyc)
 - trace exits from [exitdump]: per (op at snapshot, last abort reason of the exit's side attempt, blacklisted) weighted by count
Usage: CPUS=0,1,2 aborts.py OUTDIR prog.lua ...   (writes OUTDIR/<name>.v3 (gz) and <name>.sum, prints a compact table)"""
import collections, gzip, os, queue, re, subprocess, sys, threading

LUA = os.environ.get("LUA", os.path.expanduser("~/.cache/corpus2/target/release/rn-cli"))
OPS = ("MOVE LOADI LOADF LOADK LOADKX LOADFALSE LFALSESKIP LOADTRUE LOADNIL GETUPVAL SETUPVAL GETTABUP GETTABLE GETI GETFIELD "
       "SETTABUP SETTABLE SETI SETFIELD NEWTABLE SELF ADDI ADDK SUBK MULK MODK POWK DIVK IDIVK BANDK BORK BXORK SHLI SHRI ADD SUB "
       "MUL MOD POW DIV IDIV BAND BOR BXOR SHL SHR MMBIN MMBINI MMBINK UNM BNOT NOT LEN CONCAT CLOSE TBC JMP EQ LT LE EQK EQI LTI "
       "LEI GTI GEI TEST TESTSET CALL TAILCALL RETURN RETURN0 RETURN1 FORLOOP FORPREP TFORPREP TFORCALL TFORLOOP SETLIST CLOSURE "
       "VARARG GETVARG ERRNNIL VARARGPREP EXTRAARG").split()
POLICY = {"StartRefused", "FnShort", "SideShort", "SideUntraced", "Identical"}


def analyse(prog, outdir, cpu):
    name = os.path.basename(prog)[:-4]
    src = open(prog).read().splitlines()
    env = {k: v for k, v in os.environ.items() if not k.startswith("RUSTYNAIL_")}
    env.update(RUSTYNAIL_SEED="0", RUSTYNAIL_JIT_VERBOSE="3", RUSTYNAIL_JIT_EXITDUMP="1")
    if os.environ.get("XLIMITS"):
        env["RUSTYNAIL_JIT_TRACE_LIMITS"] = os.environ["XLIMITS"]
    r = subprocess.run(["taskset", "-c", str(cpu), LUA, prog], stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL,
                       stderr=subprocess.PIPE, env=env, timeout=900, cwd=os.path.dirname(prog))
    err = r.stderr.decode(errors="replace")
    with gzip.open(os.path.join(outdir, name + ".v3.gz"), "wt") as f:
        f.write(err)
    head = None
    ab = collections.Counter()
    for line in err.splitlines():
        m = re.match(r"\[trace\] \d+ start proto:(\d+) pc (\d+)", line)
        if m:
            head = ("root", int(m.group(1)), int(m.group(2)))
            continue
        if line.startswith("[trace] side start"):
            head = ("side", None, None)
            continue
        m = re.match(r"\[trace\] abort (\S+) at pc (\d+) \(depth (\d+), op (\d+)\)", line)
        if m:
            reason, pc, depth, op = m.group(1), int(m.group(2)), int(m.group(3)), int(m.group(4))
            reason = re.sub(r"Nyi\((\d+)\)", lambda k: f"Nyi({OPS[int(k.group(1))]})", reason)
            opn = OPS[op] if op < len(OPS) else str(op)
            if head and head[0] == "root":
                ld = head[1]
                h = f"root L{ld}" + (f" '{src[ld - 1].strip()[:50]}'" if 0 < ld <= len(src) else " main")
            else:
                h = "side"
            ab[(reason, opn, depth, h)] += 1
    t1 = []
    for m in re.finditer(r"^\[jit\] exit proto@\S+ line (\d+) pc (\d+) Some\((\w+)\): (\d+) exits, ([\d.]+) cyc/stay", err, re.M):
        ln, pc, op, n, cyc = int(m.group(1)), int(m.group(2)), m.group(3), int(m.group(4)), float(m.group(5))
        t1.append((n * cyc / 1e6, ln, pc, op, n, cyc))
    t1.sort(reverse=True)
    ex = collections.Counter()
    for m in re.finditer(r"^\[exitdump\] \S+ trace \d+ side (\w+) head \d+ exit \d+ n (\d+) pc (\d+) op (?:Some\((\w+)\)|None) frames (\d+) gk (\d+) st (\S+(?: \S+)*?) fails \d+ last (?:Some\(\"(\w+)\"\)|None) black (\w+)", err, re.M):
        side, n, pc, op, frames, gk, st, last, black = m.groups()
        ex[(op or "-", last or "-", black, gk)] += int(n)
    summ = [l for l in err.splitlines() if re.match(r"\[trace\] (recorded|aborted|exits|side traces|links|sink)", l)]
    lines = []
    lines.append(f"== {name} (rc {r.returncode})")
    lines += summ
    lines.append("aborts (count reason op depth head):")
    for (reason, opn, depth, h), n in ab.most_common(40):
        lines.append(f"  {n:5} {reason:22} {opn:9} d{depth} {h}{'  [policy]' if reason in POLICY else ''}")
    lines.append("tier-1 exits by interpreter stay (Mcyc line pc op exits cyc/stay):")
    for row in t1[:12]:
        lines.append("  %.1f L%d pc%d %s %d %.0f" % row)
    lines.append("trace exits (exitdump; n op last-side-abort blacklisted guardclass):")
    for (op, last, black, gk), n in ex.most_common(20):
        lines.append(f"  {n:9} {op:10} {last:14} black={black} gk={gk}")
    open(os.path.join(outdir, name + ".sum"), "w").write("\n".join(lines) + "\n")
    return name, ab, t1, ex


def main():
    outdir = sys.argv[1]
    os.makedirs(outdir, exist_ok=True)
    cpus = [int(x) for x in os.environ.get("CPUS", "0,1,2").split(",")]
    q = queue.Queue()
    for p in sys.argv[2:]:
        q.put(os.path.abspath(p))
    lock = threading.Lock()

    def work(cpu):
        while True:
            try:
                p = q.get_nowait()
            except queue.Empty:
                return
            try:
                name, ab, t1, ex = analyse(p, outdir, cpu)
                with lock:
                    print(f"done {name}", flush=True)
            except Exception as e:
                with lock:
                    print(f"FAIL {p}: {e}", flush=True)

    ths = [threading.Thread(target=work, args=(c,)) for c in cpus]
    for t in ths:
        t.start()
    for t in ths:
        t.join()


if __name__ == "__main__":
    main()
