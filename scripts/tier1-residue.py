#!/usr/bin/env python3
"""Where a program's instructions go, and which trace exits land in tier 1 (docs/STUDY_TIER1_RESIDUE.md).

  RUSTYNAIL_JIT_PERFMAP=2 RUSTYNAIL_JIT_VERBOSE=2 RUSTYNAIL_JIT_EXITDUMP=1 \
      perf record -e instructions:u -c 20000 -o P.data rnlua prog.lua 2> log.txt
  perf report -i P.data --no-children --sort dso,sym --stdio > rep.txt
  cp /tmp/perf-<pid>.map map.txt          # the pid of rnlua: perf script -i P.data -F pid | head -1
  scripts/tier1-residue.py rep.txt log.txt map.txt [--json out.json] [--top N]

Prints
  * the category shares (traces, tier 1, interpreter, exits/materialisation, trace helpers, tier-1 helpers, allocation, GC,
    library, strings, table ops, compiler, libc, parser, diagnostics, other);
  * trace share per head (join of `TRACE-<n>` with `[trace] <n> start proto:<line> pc <pc>` / `[trace] <n> side .. head proto:<l> pc <p>`);
  * tier-1 share per prototype line (`lua_proto:<line>@..`);
  * the trace exits that land in tier 1 (`[exitdump]` lines whose state is not a side trace), with their landing prototype
    (the `land` address looked up in the perf map) and their counts, grouped by landing prototype;
  * the head table rows of the prototypes with the largest tier-1 share.
"""
import json
import re
import sys
from collections import defaultdict

args = [a for a in sys.argv[1:]]
jout = None
topn = 20
if "--json" in args:
    i = args.index("--json")
    jout = args[i + 1]
    del args[i : i + 2]
if "--top" in args:
    i = args.index("--top")
    topn = int(args[i + 1])
    del args[i : i + 2]
rep, log = args[0], args[1]
mapf = args[2] if len(args) > 2 else None

CATS = [
    ("traces", r"^TRACE-"),
    ("tier1", r"^lua_proto:"),
    ("jit-other", r"^\[JIT\]"),
    ("interp", r"rn_core::vm_unsafe::|rn_core::vm::"),
    ("exits", r"rn_jit::tr::exit::(materialise|jit_tr_mat|exit_entry|restore|jit_tr_exit|handle|lazy|mat_|land|exit_)"),
    ("trace-helpers", r"rn_jit::tr::\w+::jit_tr_|rn_jit::tr::jit_tr_|rn_jit::rt::"),
    ("gc", r"rn_core::heap::|gc_step|free_obj|rn_core::gc::|sweep|propagatemark|<rn_core::alloc::PoolAllocator>::free|free_block"),
    ("alloc", r"rn_core::table::(setnodevector|resize|insert_small|newkey|new_|alloc)|new_table|rn_core::alloc::|alloc_block|new_closure|newref"),
    ("tier1-helpers", r"rn_core::jit_helpers::|pre_call|pre_tail_call|post_call|precall|jit_call|jit_tailcall|jit_ret|ccall"),
    ("lib", r"rn_std::"),
    ("strings", r"rn_core::string|new_str|short_core|intern|rn_core::lstr"),
    ("table-ops", r"rn_core::table::|TableRef"),
    ("diag", r"alloc::fmt::|core::fmt::|rn_jit::tr::diag::|vlog|eprint|std::io::"),
    ("compiler", r"rn_jit::|alloc::raw_vec|hash_one|sip::Hasher|hashbrown|RandomState|core::slice::sort|alloc::vec::"),
    ("parser", r"rn_compiler::"),
    ("libc", r"^libc"),
]


def classify(dso, sym):
    key = sym if (sym.startswith("TRACE-") or sym.startswith("lua_proto:")) else sym
    if dso.startswith("[JIT]") and not (sym.startswith("TRACE-") or sym.startswith("lua_proto:")):
        return "jit-other"
    if dso.startswith("libc"):
        return "libc"
    for name, rx in CATS:
        if re.search(rx, key):
            return name
    if dso == "[unknown]":
        return "other"
    return "other"


head_of = {}
for l in open(log, errors="replace"):
    m = re.match(r"\[trace\] (\d+) start proto:(-?\d+) pc (\d+)", l)
    if m:
        head_of[int(m[1])] = (int(m[2]), int(m[3]))
        continue
    m = re.match(r"\[trace\] (\d+) side.* head proto:(-?\d+) pc (\d+)", l)
    if m:
        head_of[int(m[1])] = (int(m[2]), int(m[3]))

cat = defaultdict(float)
trace = defaultdict(float)
tier1 = defaultdict(float)
syms = defaultdict(float)
tot = 0.0
for l in open(rep, errors="replace"):
    m = re.match(r"\s*([\d.]+)%\s+(\S+(?: tid \d+)?)\s+\[(.)\]\s+(.*)", l)
    if not m:
        continue
    pct, dso, sym = float(m[1]), m[2], m[4].strip()
    tot += pct
    c = classify(dso, sym)
    cat[c] += pct
    if c not in ("traces", "tier1"):
        syms[(c, sym[:110])] += pct
    t = re.match(r"TRACE-(\d+)", sym)
    p = re.match(r"lua_proto:(-?\d+)@", sym)
    if t:
        trace[head_of.get(int(t[1]), ("?", "?"))] += pct
    elif p:
        tier1[int(p[1])] += pct

# perf map: tier-1 ranges -> proto line
ranges = []
if mapf:
    for l in open(mapf, errors="replace"):
        parts = l.split(None, 2)
        if len(parts) < 3:
            continue
        name = parts[2].strip()
        m = re.match(r"lua_proto:(-?\d+)@", name)
        if m:
            ranges.append((int(parts[0], 16), int(parts[1], 16), int(m[1])))
    ranges.sort()


def land_line(a):
    import bisect

    i = bisect.bisect_right(ranges, (a, 1 << 62, 0)) - 1
    if i >= 0 and ranges[i][0] <= a < ranges[i][0] + ranges[i][1]:
        return ranges[i][2]
    return None


nested_links = []
exits = []  # (n, trace, side, line_of_trace_head, landing line, pc, op, frames, st, last, black)
for l in open(log, errors="replace"):
    if not l.startswith("[exitdump]"):
        continue
    m = re.search(
        r"\] (\w+) trace (\d+) side (\w+) head \d+ exit (\d+) n (\d+) pc (\d+) op Some\((\w+)\) frames (\d+) gk \d+ st (.*?) fails (\d+) last (.*?) black (\w+)(?: land (0x[0-9a-f]+))?(?: link (\w+))?",
        l,
    )
    if not m:
        continue
    st = m[9]
    if st.startswith("Some(Has("):
        continue  # linked to a side trace
    if m[14] == "true":
        nested_links.append((int(m[5]), int(m[2])))
        continue  # F2b frame-linking exit: frames materialised, continues in the callee loop's root trace (not tier 1)
    tno = int(m[2])
    ll = land_line(int(m[13], 16)) if m[13] else None
    exits.append(
        dict(
            n=int(m[5]),
            trace=tno,
            side=m[3] == "true",
            head=head_of.get(tno, ("?", "?")),
            land=ll,
            pc=int(m[6]),
            op=m[7],
            frames=int(m[8]),
            st=st.replace("Some(", "").rstrip(")"),
            fails=int(m[10]),
            last=m[11].replace("Some(", "").replace(")", "").strip('"'),
            black=m[12] == "true",
            link=m[14] == "true",
        )
    )

heads = [l.rstrip("\n") for l in open(log, errors="replace") if l.startswith("[heads]")]

order = [c for c, _ in CATS] + ["other"]
print(f"total samples share {tot:.1f}%")
print("categories: " + ", ".join(f"{c} {cat[c]:.1f}" for c in order if cat[c] >= 0.05))
print("\n-- traces by head (line, pc) --")
for (h, v) in sorted(trace.items(), key=lambda x: -x[1])[:topn]:
    print(f"  {v:6.2f}%  head L{h[0]} pc {h[1]}")
print("\n-- tier 1 by prototype line --")
for (h, v) in sorted(tier1.items(), key=lambda x: -x[1])[:topn]:
    print(f"  {v:6.2f}%  L{h}")
print("\n-- top non-trace symbols --")
for ((c, s), v) in sorted(syms.items(), key=lambda x: -x[1])[:topn]:
    print(f"  {v:6.2f}%  [{c}] {s}")
print(f"\n-- frame-linking exits (materialise frames, continue in a callee loop trace): {sum(n for n, _ in nested_links)} --")
print("\n-- exits landing in tier 1 (no side trace), by landing prototype --")
byland = defaultdict(lambda: [0, []])
for e in exits:
    byland[e["land"]][0] += e["n"]
    byland[e["land"]][1].append(e)
for land, (n, es) in sorted(byland.items(), key=lambda x: -x[1][0])[:topn]:
    print(f"  land L{land}: {n} exits")
    for e in sorted(es, key=lambda e: -e["n"])[:4]:
        print(
            f"      n {e['n']:>8} trace {e['trace']} ({'side' if e['side'] else 'root'} of L{e['head'][0]} pc {e['head'][1]}) pc {e['pc']} {e['op']} frames {e['frames']} st {e['st']} fails {e['fails']} last {e['last']}"
        )
print("\n-- head table rows of the top tier-1 prototypes --")
for (h, v) in sorted(tier1.items(), key=lambda x: -x[1])[:8]:
    rows = [r for r in heads if re.search(rf":{h} pc ", r)]
    print(f"  L{h} ({v:.2f}% tier 1): {len(rows)} head rows")
    for r in rows[:6]:
        print("     " + r[8:][:260])
if jout:
    json.dump(
        dict(
            cats={c: cat[c] for c in order},
            trace={f"L{h[0]}:{h[1]}": v for h, v in trace.items()},
            tier1={f"L{h}": v for h, v in tier1.items()},
            exits=exits,
            heads=heads,
            syms={f"{c}|{s}": v for (c, s), v in syms.items()},
        ),
        open(jout, "w"),
        indent=0,
    )
