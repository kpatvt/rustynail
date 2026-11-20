#!/usr/bin/env python3
"""Join a perf profile with the trace log: which heads / protos own the cycles (docs/STUDY_COVERAGE.md).

  RUSTYNAIL_JIT_PERFMAP=2 RUSTYNAIL_JIT_VERBOSE=2 perf record -e cycles:u -o P.data rnlua prog.lua 2> log.txt
  perf report -i P.data --no-children --sort sym --stdio > rep.txt
  scripts/heads-perf.py rep.txt log.txt [N]

Prints: trace share per head (trace number -> `[trace] <n> start proto:<line> pc <pc>`; a side trace's install line names the head of its root),
tier-1 share per proto line (`lua_proto:<line>@..`), and the rest grouped by helper family (alloc, gc, table helpers, C-call glue...).
"""
import re
import sys
from collections import defaultdict

rep, log = sys.argv[1], sys.argv[2]
topn = int(sys.argv[3]) if len(sys.argv) > 3 else 25

head_of = {}  # trace no -> (line, pc)
last_head = None
for l in open(log, errors="replace"):
    m = re.match(r"\[trace\] (\d+) start proto:(\d+) pc (\d+)", l)
    if m:
        head_of[int(m[1])] = (int(m[2]), int(m[3]))
        continue
    m = re.match(r"\[trace\] (\d+) side.* head proto:(\d+) pc (\d+)$", l)
    if m:
        head_of[int(m[1])] = (int(m[2]), int(m[3]))

FAM = [
    ("alloc", r"jit_tr_alloc|jit_newtable|new_table|resize|setnodevector|newkey|alloc_block|new_closure|new_string|new_ustr"),
    ("gc", r"free_obj|gc_step|propagatemark|reallymark|sweep|free_block|atomic|singlestep|list_remove"),
    ("sunk-materialise", r"materialise|jit_tr_mat"),
    ("table helpers (tier 1)", r"jit_get|jit_set|get_int|psetshortstr|TableRef|mm_index|finish_get|luaH"),
    ("call glue (tier 1)", r"jit_call|precall|post_call|pre_tail_call|jit_tailcall|jit_ret|jit_vararg"),
    ("lib functions", r"tinsert|tremove|checktab|math_|str_|setmetatable|lib_"),
    ("exit/entry handlers", r"jit_tr_exit|jit_tr_hot|jit_tr_entry|exit_handler|jit_tr_"),
    ("interpreter", r"run_mode|vm_unsafe"),
]
trace = defaultdict(float)
tier1 = defaultdict(float)
fam = defaultdict(float)
other = defaultdict(float)
tot = 0.0
for l in open(rep, errors="replace"):
    m = re.match(r"\s*([\d.]+)%\s+(?:\[.\]\s+)?(.*)", l)
    if not m:
        continue
    pct, sym = float(m[1]), m[2].strip()
    tot += pct
    t = re.match(r"TRACE-(\d+)", sym)
    p = re.match(r"lua_proto:(\d+)@", sym)
    if t:
        trace[head_of.get(int(t[1]), ("?", "?"))] += pct
    elif p:
        tier1[int(p[1])] += pct
    else:
        for name, rx in FAM:
            if re.search(rx, sym):
                fam[name] += pct
                break
        else:
            other[sym[:60]] += pct

print(f"total {tot:.1f}%  traces {sum(trace.values()):.1f}%  tier1 {sum(tier1.values()):.1f}%  helpers+runtime {sum(fam.values())+sum(other.values()):.1f}%")
print("-- traces per head (proto line, pc)")
for k, v in sorted(trace.items(), key=lambda kv: -kv[1])[:topn]:
    print(f"  {v:5.1f}%  line {k[0]} pc {k[1]}")
print("-- tier 1 per proto line")
for k, v in sorted(tier1.items(), key=lambda kv: -kv[1])[:topn]:
    print(f"  {v:5.1f}%  line {k}")
print("-- helpers / runtime by family")
for k, v in sorted(fam.items(), key=lambda kv: -kv[1]):
    print(f"  {v:5.1f}%  {k}")
print("-- other symbols")
for k, v in sorted(other.items(), key=lambda kv: -kv[1])[:10]:
    print(f"  {v:5.1f}%  {k}")
