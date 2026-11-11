#!/usr/bin/env python3
"""P-PROF step 2: classifies the perf reports written by pprof-record.py into cost categories.
usage: pprof-classify.py RECDIR prog [prog ...]   -> markdown to stdout

Categories (first match wins, see CATS): trace code (unsymbolised [JIT] samples), tier-1/J4 code (lua_proto@...),
JIT compile, Lua compiler, interpreter dispatch, call machinery, metamethod paths, JIT helpers, allocator, GC,
string interning, table internals (hash lookup, insert, resize, len), stdlib, number<->string, teardown, libc (unnamed
= memcpy/memcmp/strlen...), other.
The bytecode share is an ESTIMATE: interpreter self cycles (run_mode, finish_op, ...) of the TRACE-ON run divided by
the interpreter self cycles per bytecode of the JIT-off run (bytecodes counted with a count hook)."""
import os, re, sys, collections

CATS = [
    ("trace code", lambda d, s: d.startswith("[JIT]") and "lua_proto@" not in s),
    ("tier-1 (J4) code", lambda d, s: "lua_proto@" in s),
    ("trace exit (Rust)", lambda d, s: "rn_jit::tr::exit::" in s or "snap::land_info" in s),
    ("JIT compile", lambda d, s: "rn_jit::" in s and "jit_tr_alloc" not in s),
    ("Lua compiler", lambda d, s: "rn_compiler::" in s or "protodata" in s or "Proto>::from_data" in s or "hashbrown" in s or "RandomState" in s or "sip::Hasher" in s or "DefaultHasher" in s or "proto_to_data" in s),
    ("interpreter dispatch", lambda d, s: re.search(r"vm_unsafe::(run_mode|finish_op|finish_set_op|index_)|vm::execute", s)),
    ("metamethod paths", lambda d, s: re.search(r"vm_unsafe::mm_|call_meta|try_func_tm|get_metafield|compare_lt_le", s)),
    ("call machinery", lambda d, s: re.search(r"State>::(pre_call|post_call|pre_tail_call|precall_c|ccall|unroll|resume|pcall_k|close)|shrink_stack|lib_coroutine|do_error|base_error|base_pcall|base_xpcall|get_info_aux|where_|error_msg|new_thread", s)),
    ("JIT helpers", lambda d, s: "jit_helpers::" in s),
    ("allocator", lambda d, s: re.search(r"PoolAllocator|push_closure|raw_vec|__rust_alloc|jit_tr_alloc|State>::new_table|reserve::do_reserve", s) or (d.startswith("libc") and re.search(r"^(malloc|cfree|realloc|free|calloc)", s))),
    ("string interning", lambda d, s: re.search(r"Heap>::(new_str|free_string|resize_strtab|jit_str_probe)", s)),
    ("GC", lambda d, s: re.search(r"heap::Heap|gc::objsize|gc_step|with_roots|Heap>::|Gc<", s) and "drop_glue" not in s),
    ("table internals", lambda d, s: re.search(r"core::table::|TableRef|State>::(finish_set|finish_get|raw_set_int)|obj_len|Value>::raw_equal", s)),
    ("number<->string", lambda d, s: re.search(r"number::|cfmt::|flt2dec|bignum|NumBuf|core::fmt|from_str|spec_to_string", s)),
    ("stdlib", lambda d, s: "rn_std::" in s or "concat" in s or re.search(r"State>::(push_bytes|push_vec|push_filled|tolstring)", s)),
    ("teardown", lambda d, s: "drop_glue" in s),
    ("libc (memcpy etc.)", lambda d, s: d.startswith("libc") or "strlen" in s),
    ("kernel/unknown", lambda d, s: d.startswith("[unknown]") or d.startswith("[kernel")),
]
INTERP = "interpreter dispatch"


def parse(path):
    total = 0
    rows = []
    for line in open(path, errors="replace"):
        m = re.match(r"\s*([\d.]+)%\s+(.+?)\s+\[(.)\]\s+(.*)$", line)
        if m:
            rows.append((float(m.group(1)), m.group(2).strip(), m.group(4).strip()))
        m = re.search(r"Event count \(approx\.\): (\d+)", line)
        if m:
            total = int(m.group(1))
    return total, rows


def classify(rows):
    cat = collections.OrderedDict((c, 0.0) for c, _ in CATS)
    cat["other"] = 0.0
    per = collections.defaultdict(list)
    for pct, d, s in rows:
        for name, f in CATS:
            if f(d, s):
                cat[name] += pct
                per[name].append((pct, s))
                break
        else:
            cat["other"] += pct
            per["other"].append((pct, s))
    return cat, per


def short(s):
    s = re.sub(r"<(lua_\w+::)+(\w+)(<[^>]*>)?( as [^>]*)?>::", r"\2::", s)
    s = re.sub(r"lua_(core|std|jit)::", "", s)
    return s[:48]


SHORT = {"trace exit (Rust)": "EXIT", "trace code": "TRC", "tier-1 (J4) code": "T1", "JIT compile": "JC", "Lua compiler": "LC", "interpreter dispatch": "INT",
         "metamethod paths": "MM", "call machinery": "CALL", "JIT helpers": "HLP", "allocator": "ALLOC", "string interning": "INTERN",
         "GC": "GC", "table internals": "TBL", "number<->string": "NUM", "stdlib": "LIB", "teardown": "TD",
         "libc (memcpy etc.)": "LIBC", "kernel/unknown": "K", "other": "OTH"}


def summary(rec, tag):
    """One line per program: % of cycles per category for the run `tag` (on / j4 / off)."""
    cats = [c for c, _ in CATS] + ["other"]
    print("%-13s" % f"prog ({tag})" + "".join("%7s" % SHORT[c] for c in cats) + "   Mcyc")
    for f in sorted(os.listdir(rec)):
        if f.endswith(f".{tag}.rep"):
            tot, rows = parse(os.path.join(rec, f))
            cat, _ = classify(rows)
            print("%-13s" % f[: -len(tag) - 5] + "".join("%7.1f" % cat[c] for c in cats) + "%7.0f" % (tot / 1e6))


def main():
    if sys.argv[1] == "--summary":
        return summary(sys.argv[2], sys.argv[3] if len(sys.argv) > 3 else "on")
    rec, progs = sys.argv[1], sys.argv[2:]
    for p in progs:
        print(f"### {p}")
        info = {}
        for tag in ("on", "j4", "off"):
            f = os.path.join(rec, f"{p}.{tag}.rep")
            if not os.path.exists(f):
                continue
            tot, rows = parse(f)
            cat, per = classify(rows)
            info[tag] = (tot, cat, per)
        if "on" not in info:
            continue
        names = [c for c in info["on"][1]]
        print("| category | trace on | J4 only | JIT off |\n|---|---|---|---|")
        for c in names:
            v = [info[t][1][c] if t in info else float("nan") for t in ("on", "j4", "off")]
            if max(v[0], v[1] if v[1] == v[1] else 0, v[2] if v[2] == v[2] else 0) < 0.5:
                continue
            print(f"| {c} | " + " | ".join(f"{x:5.1f}%" for x in v) + " |")
        print(f"| total Mcyc (perf, sampled) | " + " | ".join(f"{info[t][0] / 1e6:.0f}" if t in info else "-" for t in ("on", "j4", "off")) + " |")
        # top helpers, top symbols
        per = info["on"][2]
        h = sorted(per.get("JIT helpers", []), reverse=True)[:5]
        print("\ntop helpers (on): " + ", ".join(f"{short(s).replace('jit_helpers::', '')} {pct:.1f}%" for pct, s in h))
        allr = sorted(((pct, s) for c in per for pct, s in per[c]), reverse=True)[:8]
        print("top symbols (on): " + ", ".join(f"{short(s)} {pct:.1f}%" for pct, s in allr))
        # bytecode share estimate
        cf = os.path.join(rec, f"{p}.count")
        if os.path.exists(cf) and cf and open(cf).read().strip().isdigit() and "off" in info:
            T = int(open(cf).read())
            interp_off = info["off"][1][INTERP] / 100 * info["off"][0]
            cpb = interp_off / T
            est = info["on"][1][INTERP] / 100 * info["on"][0] / cpb / T if cpb else float("nan")
            print(f"bytecodes (hook count, JIT off): {T / 1e6:.1f} M; interpreter self cycles per bytecode (JIT off) {cpb:.2f}; "
                  f"ESTIMATED share of bytecodes still interpreted with traces on: {min(est, 1.0) * 100:.0f}% (raw {est * 100:.0f}%)")
        sf = os.path.join(rec, f"{p}.stats")
        if os.path.exists(sf):
            st = [l for l in open(sf, errors="replace").read().splitlines() if l.startswith("[trace] recorded") or l.startswith("[trace] aborted") or l.startswith("[trace] exits")]
            print("stats: " + " / ".join(x.replace("[trace] ", "") for x in st[:6]))
        print()


main()
