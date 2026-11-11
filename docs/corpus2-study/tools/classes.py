#!/usr/bin/env python3
"""Blocker-class signatures per program from the VERBOSE=3/EXITDUMP logs (ab/*/<name>.v3.gz) + source greps."""
import collections, glob, gzip, json, os, re, sys
OPS = ("MOVE LOADI LOADF LOADK LOADKX LOADFALSE LFALSESKIP LOADTRUE LOADNIL GETUPVAL SETUPVAL GETTABUP GETTABLE GETI GETFIELD "
       "SETTABUP SETTABLE SETI SETFIELD NEWTABLE SELF ADDI ADDK SUBK MULK MODK POWK DIVK IDIVK BANDK BORK BXORK SHLI SHRI ADD SUB "
       "MUL MOD POW DIV IDIV BAND BOR BXOR SHL SHR MMBIN MMBINI MMBINK UNM BNOT NOT LEN CONCAT CLOSE TBC JMP EQ LT LE EQK EQI LTI "
       "LEI GTI GEI TEST TESTSET CALL TAILCALL RETURN RETURN0 RETURN1 FORLOOP FORPREP TFORPREP TFORCALL TFORLOOP SETLIST CLOSURE "
       "VARARG GETVARG ERRNNIL VARARGPREP EXTRAARG").split()
src_of = {}
for d in ("bench/corpus", "c2/dev", "c2/heldout"):
    for f in glob.glob(d + "/*.lua"):
        src_of[os.path.basename(f)[:-4]] = open(f).read()
res = {}
for f in sorted(glob.glob("ab/*/*.v3.gz")):
    n = os.path.basename(f)[:-6]
    t = gzip.open(f, "rt").read()
    c = collections.Counter()
    for m in re.finditer(r"^\[trace\] abort (\S+) at pc \d+ \(depth (\d+), op (\d+)\)", t, re.M):
        r, d, op = m.group(1), int(m.group(2)), OPS[int(m.group(3))]
        if r.startswith("Nyi(") and op in ("TFORCALL", "TFORPREP"): c["tfor_lua_iter(Nyi TFORCALL)"] += 1
        elif r == "CFunc" and op == "TFORCALL": c["c_iterator(CFunc TFORCALL)"] += 1
        elif r == "Vararg": c["vararg"] += 1
        elif r == "InnerLoop" and d >= 1: c["callee_loop(InnerLoop d>=1)"] += 1
        elif r == "InnerLoop": c["inner_loop(d0)"] += 1
        elif r == "LeaveFrame": c["leave_frame(side trace returns)"] += 1
        elif r == "LeaveLoop": c["leave_loop"] += 1
        elif r in ("CFunc",) or (r.startswith("Nyi(") and op == "CALL"): c["lib_call_not_recorded(CFunc/Nyi CALL)"] += 1
        elif r.startswith("Nyi(") and op == "CLOSURE": c["closure(Nyi CLOSURE)"] += 1
        elif r in ("TailCall",): c["tailcall"] += 1
        elif r in ("Recursion", "Depth"): c["recursion/depth"] += 1
        elif r in ("TableHash", "IntMiss", "Insert", "Unstable"): c["table_shape(TableHash/IntMiss/Insert/Unstable)"] += 1
        elif r in ("MetaTable", "Metamethod"): c["metamethod_record"] += 1
        elif r in ("MultRet",): c["multret"] += 1
    for m in re.finditer(r"^\[jit\] exit proto@\S+ line \d+ pc \d+ Some\((\w+)\): (\d+) exits", t, re.M):
        if m.group(1) in ("Lt", "Le", "Eq") and int(m.group(2)) >= 1000:
            c["t1_exit_mm_compare(>=1000 exits)"] += int(m.group(2))
    for m in re.finditer(r"^\[exitdump\] .* n (\d+) .* last Some\(\"MaxSide\"\) black true", t, re.M):
        c["maxside_blacklisted_exits"] += int(m.group(1))
    s = src_of.get(n, "")
    if re.search(r"coroutine\.(wrap|resume)", s): c["uses_coroutines(src)"] += 1
    res[n] = dict(c)
json.dump(res, open("data/classes.json", "w"), indent=1)
allc = sorted({k for v in res.values() for k in v})
print("| program | " + " | ".join(allc) + " |")
print("|---|" + "---|" * len(allc))
for n, v in res.items():
    print(f"| {n} | " + " | ".join(str(v.get(k, "")) for k in allc) + " |")
