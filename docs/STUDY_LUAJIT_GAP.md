# Study: the programs still weak against LuaJIT (2026-11-21, base 1e3a827)

Question: where do the cycles and instructions of the programs that session-i still lists as weak against LuaJIT go (cycle ratios plain /
LuaJIT: `awfy/permute` 3.3, `programs/fib` 2.7, `apps/raytrace` 2.6-2.8, `awfy/cd` 2.3-2.4, `corpus/lua_lexer` 2.76, `corpus/classic_vector`
2.67; `richards` 1.7, `havlak` 1.3, `deltablue` 1.07), what structurally differs from LuaJIT, and which handles are worth building. The two
corpus programs had never been profiled. Things already measured and ruled out in docs/STUDY_COVERAGE.md, docs/STUDY_ALLOC_CALL.md and
docs/JIT_TRACE.md sections 17-29 are cited, not re-derived. Two handles came out of it and are built (L1, L2: docs/JIT_TRACE.md section 30).

Method. Plain release `rn-cli` (JIT and traces on, generational GC) of 1e3a827, `RUSTYNAIL_SEED=0`, `taskset -c 4`, `perf stat -e
cycles:u,instructions:u`, min of 3 (cycles) and the first run's instructions (deterministic); binaries compared at equal-length paths
(`scratchpad/b0`, `b1`, `bx`). Profiles: `perf record -e cycles:u` / `-e instructions:u -F 5000` with `RUSTYNAIL_JIT_PERFMAP=2` and
`RUSTYNAIL_JIT_VERBOSE=2`, `perf annotate` of the helpers, `RUSTYNAIL_JIT_DUMP` for trace IR and code sizes, `RUSTYNAIL_JIT_VERBOSE=1
RUSTYNAIL_JIT_SAMPLE=on` head tables. LuaJIT 2.1 (`/usr/bin/luajit`) on the same files (`-jv` for its trace shapes, `perf record` by DSO).
Benchmark default sizes unless stated (that is what the session ratios are).

## 1. Headline (bench default sizes, base 1e3a827)

| program | ours Mcyc | ours Minstr | LuaJIT Mcyc | LuaJIT Minstr | cyc ratio | instr ratio |
|---|---:|---:|---:|---:|---:|---:|
| awfy/permute | 42 | 235 | 13 (min) .. 1367 | 111 .. 5813 | 3.2 (vs the min) | 2.1 |
| programs/fib | 1266 | 7780 | 470 | 2491 | 2.69 | 3.12 |
| apps/raytrace | 117 | 383 | 40 | 165 | 2.9 | 2.3 |
| awfy/cd | 261 | 805 | 135 | 414 | 1.93 | 1.94 |
| corpus/lua_lexer | 1261 | 5127 | 452 | 1908 | 2.79 | 2.69 |
| corpus/classic_vector | 641 | 2794 | 245 | 1144 | 2.62 | 2.44 |
| awfy/richards | 59 | 180 | 35 | 121 | 1.69 | 1.49 |
| awfy/havlak | 156 | 517 | 115 | 311 | 1.36 | 1.66 |
| awfy/deltablue | 109 | 335 | 104 | 303 | 1.05 | 1.11 |

Against LuaJIT the gap is instructions on every program (IPC is comparable: ours 3.1-6.1, LuaJIT 3.3-5.3). A measurement note on permute:
LuaJIT's own cycle count for `awfy/permute` varies by two orders of magnitude between identical runs (13, 38, 828, 1367 Mcyc in four
consecutive runs, instructions 111..5813 M): its trace formation for this program is a lottery and the session ratio (3.3) is taken
against its best run. Ours is stable (42 Mcyc +-1 %).

## 2. Per program

### corpus/lua_lexer (never profiled before)

Shares (cycles, base): helpers 50 % (`jit_tr_alloc` **17.9 %**, `tconcat` 3.1, `jit_tr_lib_sub` 3.1, `new_str` 2.8, `short_core` 3.4,
pattern matcher 5.5, `jit_call` 2.4, `exit_entry` + `restore` 2.9, `str_sub` 1.6, `lib_char` 1.2), traces 36 %, tier 1 13.7 %
(`Lexer:next` 6.7, the main chunk 4.4).

* **`string.byte` of the source text goes through the generic helper.** `jit_tr_alloc` at 17.9 % is the `ALLOC_LIB` / `lib::BYTE` arm
  (`perf annotate`: `lib_core` lines 1729-1759; a scratch counter: 8.05 M `BYTE` helper calls, everything else under 0.1 M). The inline
  `string.byte` of F2 (`Asm::byte_fast`) reads only regular long strings (`shrlen == -1`); the lexer's source is a 138904-byte
  `table.concat` result, which 5.5 adopts without a copy as an **external string** (`LSTRMEM`, `shrlen == -3`: the bytes are behind a
  pointer at the contents offset). Every `sbyte(src, p)` of the lexer (the whitespace loop, the identifier loop, the dispatch) therefore
  paid the helper's frame, the dispatch `match` and the bound logic (~85 instructions) instead of 6 inline instructions. LuaJIT has no
  external strings (all string data is inline). Library-buffer results (`string.rep` and friends, `shrlen == -4`, bytes inline) were
  refused the same way. **Handle L1, built** (section 30.1 of docs/JIT_TRACE.md): lua_lexer **-12.1 % cycles (min of 5; -10 % in the first min-of-3 pass), -13.8 % instructions**.
* After L1 (1111 Mcyc / 4421 Minstr): tier 1 is 17 % of the instructions (`Lexer:next` 10.4, main chunk 4.4), `tconcat` 4.6 (the
  minifier's `table.concat(out)` per round and `read_string`; LuaJIT pays the same), `short_core` + `new_str` 7.8 (interning the `sub`
  and `char` results: one hash and one chain walk already, P-B; LuaJIT interns too), the pattern matcher 5 % (the three anchored
  `string.find`s of the number branch: our matcher is faster than LuaJIT's and PUC's on these patterns: a micro of the three finds,
  3 M iterations: 1817 / 3251 / 6481 Mcyc for ours / LuaJIT / PUC 5.5), exits 3.5 %.
* Why tier 1 remains: the side recordings of `Lexer:next`'s loop traces end `LeaveFrame` (29 side aborts; they are the exits at the
  final `RETURN` of function-entry families, `fn_end` refuses a side trace that starts at the `RETURN` itself) or `MaxSide` (21), and the
  number branch dies `Nyi(Call)` at `record.rs:10592` (a real pattern in `string.find` is not recorded; in a side recording the stitching
  fallback is not allowed, so turning the `Nyi` into `lib_unrecorded` only renames the abort). LuaJIT stitches these finds and is itself
  mostly not in traces here: by DSO, LuaJIT spends 50 % in `libluajit` (interpreter and library), 28 % in libc and only 21 % in trace
  code; its lead is a cheap interpreter / library path, not trace quality.
* Candidate L3 (not built): a `FIND` kind for real patterns without captures (the helper runs the matcher, results are two integers or
  nil, no allocation; malformed / too-complex patterns return "do nothing" and the trace exits). Gain (arithmetic): the number tokens
  are 9.6 % of the tokens; their tier-1 path plus three C-call glues is ~3 % of the instructions; below the 5 % bar alone.

### corpus/classic_vector (never profiled before)

Workload: 2.77 M objects (`Object:__call` -> `setmetatable({}, self)` -> `obj:new(x, y)` -> `self.x = ..; self.y = ..`), the vector
arithmetic of a spring system. Ours 231 cycles per object, LuaJIT 88.

* **Half of the instructions are the two key insertions of every new object.** Instruction shares: `newkey` 9.2, `jit_tr_newref` 8.6,
  `setnodevector` 8.2, `resize_hash` 6.6, `insertkey_collide` 5.7, `newcheckedkey` 5.4, `newref_shortstr` 4.6 = 48 % = 241
  instructions per `NEWREF` (5.55 M of them). Both keys rehash: the table is born with the dummy node, the first key grows it to one node,
  the second to two (allocate 2 nodes, reinsert, free 1 node). Micro (`setmetatable({}, mt); t.x = i; t.y = i`, 5 M): ours 1001 Mcyc
  / 5875 Minstr, LuaJIT 849 / 4793 Mcyc / Minstr; with the constructor presized (`{x = false, y = false}`): ours 406, LuaJIT 608. So the
  rehash chain costs us ~120 cycles per object against LuaJIT's ~48.
* **Handle L2, built** (section 30.2): the two growth steps of a fresh table without an array part (dummy -> 1 node, 1 node with a live
  non-integer key -> 2 nodes) take a short path in rn-core (`insert_small`) that does exactly what the general chain does (same node
  vector sizes, the old key reinserted first, the same `insertkey`, the same accounting and barriers), without `rehash`'s counting loop,
  `newcheckedkey`'s array test, the dispatch through four functions and the second lookup. It serves every caller of `newkey` (traces'
  `NEWREF`, tier-1 `SETFIELD` helpers, the interpreter). classic_vector **-12.8 % cycles, -15.0 % instructions**; the micro -19 % cycles
  (995 -> 805 Mcyc, LuaJIT 849).
* After L2 (560 Mcyc / 2376 Minstr): `insert_small` 24 % of the cycles (the allocation of the node vectors into cold pool blocks: the
  stores after the pop of the free list stall), materialisation of sunk tables 3.7 % (555 K tables materialised at exits), the GC list
  maintenance 4 %, traces 21 %.
* Candidate P1 (**built**, section 6 and docs/JIT_TRACE.md section 32): **presize the hash part of a `TNEW` from the `NEWREF`s the trace records on it**.
  Ceiling measured by the source rewrite (`{x = false, y = false}`): 560 -> 453 Mcyc (-19 % after L2; -29 % against the base). The
  layout of a table grown by two string keys equals the layout of a two-node part filled in the same order (shown in section 30.2 for the
  `insert_small` argument), so the end state is exact; the obstacle is TR29.1 in the window between the `TNEW` and the second `NEWREF`: an
  exit there hands the interpreter a two-node table where it would have a dummy or one-node part, and an interpreter path that then
  inserts other keys (an integer key goes to the hash part instead of the array part) produces another layout and `next` order. A sound
  version needs the window exits to shrink the part back (a snapshot entry "presized table, j keys inserted" applied by the exit stub
  before the handler or a side trace, through `resize_hash`), or a proof that the window holds no exit. Effort L, risk medium-high
  (snapshots, side traces, sinking: the sunk materialisation must keep the recorded sizes). A blanket presize of every empty `TNEW` in
  traces (scratch) was 3x slower (shape guards and array-only `{}` tables), so it needs the per-table recording.

### apps/raytrace

Covered by docs/STUDY_COVERAGE.md (C1 headless callees `vec`, `V:add` in tier 1 under a tier-1 caller; K4 measured twice and not
shipped, section 27.4). New measurement at `raytrace 40` (after K1/K3): tier 1 28.9 % (`vec` 9.9, `render` 7.8, `trace` 3.9), traces
39 %, `setnodevector` **9.4 %** (annotate: 37 % of its samples on the instruction after the node-initialising stores, 10 % on the free-list
`next` load: the stores into a cold pool block stall), `list_remove_after` 2.7 %, `jit_tr_alloc` 2.1 %, `math_min` 1.5 % (tier 1),
`jit_call` 1.5 %. Per `vec` the allocation side is the same cold-store cost as in classic_vector. L1 / L2 do not apply (constructors with
fields are presized by `NEWTABLE`). No new handle above the bar: the known ones are C1 (coverage of the shading section) and H3 (inline
`TNEW`), both L.

### awfy/cd (at the default size)

* **A quarter of the default-size run is the trace compiler.** `rn_jit` code outside the runtime helpers is 25.9 % of the cycles
  (`rm_op` 2.5, `allocate_hinted` 1.4, `build_exits_ex` 1.3, `Em::operand` 1.1, `inline_stub_ops_slots` 1.1, `emit` 1.0,
  `stubs_and_tail` 0.9, ...): 241 side recordings (139 failed), 195 pipelines. The installed code is **1.58 MB** (8.1 KB per trace; the
  largest, trace 11: 286 IR instructions, 128 snapshots, 80 KB of code, ~620 bytes per snapshot): the exit stubs are inline and restore
  their snapshot's slots, sunk tables and frames in place, so code size and assembly time grow with snapshots x live slots. LuaJIT's
  exit stubs are a few bytes each (the snapshot is restored by one generic handler from data). raytrace (3.4 %), lua_lexer (7 %) and
  classic_vector (3.8 %) pay much less compile time; it is cd's many short-lived side traces. `sinkstub=0` (no inline materialisation)
  makes the code 15 % smaller but the run 10 % slower (the hot exits need the inline form).
* The rest is the coverage / materialisation picture of docs/STUDY_COVERAGE.md (materialise + `jit_tr_mat` 9.6 %, tier 1 15 %: `put`,
  `compareTo`, `isRed`).
* Candidate S1 (**built** as `lazystub`, docs/JIT_TRACE.md section 31: exit stubs assembled at the first exit, cd -2..3 % cycles, -4.6 % instructions, trace code 1.58 MB -> 0.45 MB + 0.15 MB of stubs; the arithmetic below overestimated the cycle gain): **lazy inline stubs** (a short generic stub per exit, the inline stub assembled when the exit turns hot and
  patched in). Arithmetic: assembly + x86 + exits + install are ~40 % of the pipeline time; if two thirds of the stub bytes are never
  needed, cd's compile share drops by ~10 points: cd -6..10 % at the default size, ~0 at 20x sizes. Effort L (patching, the stub
  variants of K/J/N bundles), risk medium.

### programs/fib, awfy/permute

Protocol-bound, as docs/STUDY_ALLOC_CALL.md section 4 and docs/JIT_TRACE.md section 21 measured (fib: 99.5 instructions per call against
LuaJIT's 32; permute: 271 per call against 53, 6.1 IPC). The remaining items there are either unsound (identity-chain removal, entry-only
poll, return-guard fusion), already ceiling-measured small (H7: sound part -5 % instructions, 0 % cycles), or need cross-trace facts
(method-lookup chain across trace entries, section 28). Nothing new was found that changes that: L1 / L2 do not apply (no strings, no
table growth in the hot path). Note the LuaJIT variance on permute above.

### richards, havlak, deltablue

richards (1.69) and deltablue (1.05) are the coverage classes of docs/STUDY_COVERAGE.md (C2/C6 task bodies reached by tail calls;
C3/C5 `Plan:execute`). havlak (1.36 cycles, but 1.66 instructions): L2 moves it by +0.2 % instructions (its tables grow past two keys).
No cheap handle found.

## 3. What structurally differs from LuaJIT (the cross-program view)

| difference | where it shows | evidence |
|---|---|---|
| external / buffer strings refused by inline string code | lua_lexer (`byte`), any program reading a big `table.concat` / `string.rep` result | 8 M helper calls, 17.9 % cycles; **L1** |
| table growth through a long generic chain (four functions, a counting loop, a second lookup per key) | classic_vector, every `setmetatable({}, C)` + field-init object idiom (30 programs of the suite have it) | 241 instructions per `NEWREF`; **L2** |
| no presizing from trace knowledge (LuaJIT does not presize either, but its rehash is cheaper) | classic_vector, OOP idioms | -19 % ceiling after L2; P1 |
| inline exit stubs (code size, compile time) | cd at default size: 26 % compiler, 1.58 MB code | S1 |
| cold-block allocation stores | raytrace `setnodevector` 9.4 %, classic_vector `insert_small` 24 % | H3 / H4 class (pool reuse order) |
| recursion / frame protocol | fib, permute | sections 21, 28 |
| tier-1 residue behind refused / failed side recordings | lua_lexer, raytrace, cd | STUDY_COVERAGE C1-C7 |

## 4. Handles, ranked (estimated or measured gain on the named program, cycles)

| # | handle | gain | effort | soundness risk | status |
|---|---|---|---|---|---|
| L1 | inline `string.byte` of external and buffer long strings | lua_lexer **-12.1 % (measured)** | S | low (bound checks unchanged, a kind dispatch; one mutant) | **built** |
| L2 | `insert_small`: the first two keys of a fresh table without the generic rehash chain | classic_vector **-12.8 % (measured)**, micro -19 % | S-M | low (same layout by construction; rn-core property test against the general path, a seed-0 reference test, one mutant) | **built** |
| P1 | presize a recorded `TNEW` from its `NEWREF`s (with the window exits shrinking the part back) | classic_vector -19 % more (source-rewrite ceiling) | L | medium-high (TR29.1, snapshots, side traces, sinking) | **built** in the narrower form of section 6 (no window exits at all instead of shrinking): classic_vector **-14.8 % (measured)** |
| S1 | lazy inline exit stubs | cd -6..10 % (arithmetic); **measured -2..3 %** (expr_lang -3.5 %, raytrace -3.4 %) | L | medium | **built** (`lazystub`, JIT_TRACE section 31) |
| L3 | `string.find` with a real pattern (no captures) as a recorded helper | lua_lexer -2..4 % (arithmetic) | M | low-medium | not built: below the bar |
| - | H3 inline `TNEW` / pool reuse order | raytrace / classic_vector a few % | L | high (STUDY_ALLOC_CALL) | not revisited |
| - | protocol items for fib / permute | see section 21 | - | unsound or measured small | not revisited |

## 5. Result of the built handles (base 1e3a827 -> 6ee0b68, cycles min of 5, CPU 4, equal-length binary paths)

| program | Mcyc before -> after | Minstr before -> after | ratio to LuaJIT (cycles) |
|---|---|---|---|
| corpus/lua_lexer | 1239 -> 1089 (-12.1 %) | 5127 -> 4421 (-13.8 %) | 2.74 -> 2.41 |
| corpus/classic_vector | 640 -> 561 (-12.3 %) | 2794 -> 2376 (-15.0 %) | 2.61 -> 2.29 |
| corpus/msgpack_pure | 2023 -> 1974 (-2.4 %) | 7901 -> 7781 (-1.5 %) | - |
| apps/expr_lang | 143 -> 140 (-2.1 %) | 565 -> 557 (-1.4 %) | - |

The other 17 programs of the sanity set (permute, fib, raytrace, cd, richards, havlak, deltablue, nbody, binary_trees, queens, list, tremove_front,
dijkstra_routes, json, string_ops, class_middleclass, lru_cache) have identical instructions (havlak +0.2 %, deltablue -0.3 %) and cycles within
+-2 % (docs/JIT_TRACE.md section 30.3 has the full table). permute, fib, raytrace and cd are unchanged: their gaps are the protocol, coverage,
allocation and compile-time items of sections 2-4, all of them L-effort, and P1 / S1 are the next candidates with a credible >= 5 % gain.

## 6. P1 built (base 504b832, docs/JIT_TRACE.md section 32)

Built in a narrower form than section 2 sketched: instead of exits that shrink the part back, a group is presized only when **nothing between its
first and last insertion can exit or look at the table** (the recorder's `__newindex` memo removes the repeated metatable guards that sat there),
the first insertion presizes through a helper that **replays the interpreter's growth steps on the allocator** (accounting, memory-limit checks and
free lists end as the interpreter's), the others are A1 inline plans, and the group is applied only when the simulated presized layout **equals
the layout the interpreter's own growth produced at record time** (always for two keys; for three or more only for some key sets). Knob `presize`.
Result (cycles min of 5, CPU 4): classic_vector **550 -> 469 Mcyc (-14.8 %), 2372 -> 2051 Minstr (-13.5 %)** on 5.5, **785 -> 647 (-17.6 %),
-21.9 % instructions** on 5.4; ratio to LuaJIT 2.29 -> 1.91. Only classic_vector and deltablue (-0.7 % instructions) form groups among 42
programs; the object idioms of oop_mixins / class_middleclass allocate in one trace and initialise in another (function-entry traces of `init`),
which this design does not reach (the `TNEW` must be in the same trace). No program executes measurably more instructions (max +0.34 %, the binary;
same with the knob off).

