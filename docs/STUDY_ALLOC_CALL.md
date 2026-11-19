# Study: allocation-heavy and call-heavy programs (2026-11-19, base 324e3e0)

Question: why are we slow against node (and partly LuaJIT) on `binary_trees`, `fib`, `queens`, `list`, `cd`, `raytrace`,
`deltablue`, `richards`, `towers`, and which handles are worth building, in which order. Study only: no code change is
merged. Prototypes were scratch patches, measured and reverted (described where used).

Method. Plain release `rn-cli` of 324e3e0 (JIT on, traces on, generational GC), `RUSTYNAIL_SEED=0`, pinned core
(`taskset -c 4`), `perf stat -e cycles:u,instructions:u,branch-misses:u,cache-misses:u,L1-dcache-load-misses:u`, one run
each (instruction counts are exact; cycles move by 1-3 %). Profiles: `perf record -e cycles:u` and `-e instructions:u`
with `RUSTYNAIL_JIT_PERFMAP=1` plus a scratch patch that registers every installed trace in the perf map as `TRACE-<n>`
(4 lines in `tr/cache.rs::install`; worth keeping as a knob, see 6). Instruction-sample shares are biased towards
instructions after long-latency loads (skid), so per-op instruction counts derived from them are approximate; A/B
`perf stat` numbers are exact. Call counts come from a `debug.sethook(.., "c")` run with the JIT off at a small workload,
scaled linearly. LuaJIT 2.1 (`/usr/bin/luajit`) runs the same files; `-jv` / `-jdump` for its trace shapes. Larger
workloads than the default so that steady state dominates: `binary_trees 17`, `fib 37`, `queens 13`, `richards 500`,
`deltablue 400`, `raytrace 40`, `list 7000`, `cd 20`, `towers 900`.

## 1. Headline counters

| program | rn Mcyc | rn Minstr | LJ Mcyc | LJ Minstr | rn/LJ cyc | Lua calls (M) | rn cyc/call | LJ cyc/call | exits (M) |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| binary_trees 17 | 3952 | 19437 | 4421 | 14294 | 0.89 | 60 (30 M tables) | 66 | 74 | 30.5 |
| fib 37 | 1253 | 8211 | 473 | 2532 | 2.65 | 78.2 | 16.0 | 6.0 | 39.1 |
| queens 13 | 1873 | 5529 | 1899 | 4335 | 0.99 | 4.7 (60 M loop iters) | - | - | 35.3 |
| richards 500 | 856 | 2851 | 580 | 2200 | 1.48 | 20.2 (7.7 tail, 1.5 C) | 42 | 29 | 5.7 |
| deltablue 400 | 1229 | 4570 | 604 | 2187 | 2.03 | 20.2 (1.4 C) | 61 | 30 | 5.4 |
| raytrace 40 | 2032 | 7707 | 678 | 3057 | 3.00 | 20.6 (5.5 tail, 8.8 C) | 99 | 33 | 4.8 |
| list 7000 | 2509 | 12947 | 1746 | 6262 | 1.44 | 39.9 (4.9 tail) | 63 | 44 | 75.9 |
| cd 20 | 3249 | 12052 | 1581 | 5032 | 2.05 | 42.3 (7.0 tail, 7.4 C) | 77 | 37 | 27.7 |
| towers 900 | 637 | 2906 | 368 | 1808 | 1.73 | 29.5 | 21.6 | 12.5 | 11.0 |

Branch and cache misses are not the story: `queens` has 26.6 M branch misses in both engines, `binary_trees` and `cd`
have *fewer* cache misses than LuaJIT. Against LuaJIT the gap is instructions; against node it is also allocation and
GC cycles (section 3).

## 2. Where the cycles go (cycles:u shares, categories)

`trace` = compiled trace code (incl. exit stubs and native call/return sequences); `tier1` = method-JIT code
(`lua_proto@`); `t1-helpers` = Rust helpers called from tier 1 (table get/set, C-call glue, tail calls, `setmetatable`,
table library); `alloc` = `jit_tr_alloc`, `jit_newtable`, `resize*`, `newkey`, pool `alloc_block`; `gc` = `free_obj`,
`gc_step`, marking, pool `free_block`; `mat` = materialisation of sunk tables at exits.

| program | trace | tier1 | t1-helpers | alloc | gc | mat | other (compile, misc) |
|---|---:|---:|---:|---:|---:|---:|---:|
| binary_trees | 35 | 7 | 15 (`jit_get` + `TableRef::get` on the leaf) | 25 | 18 | 0 | 0 |
| fib | 100 | 0 | 0 | 0 | 0 | 0 | 0 |
| queens | 92 | 7 | 0 | 0 | 0 | 0 | 1 |
| richards | 79 | 16 | 1 | 0 | 0 | 0 | 4 |
| deltablue | 32 | 34 | 12 (`tremove`/`tinsert`/`checktab` 4.4, `jit_call` 1.5, concat, index chain) | 9 | 2 | 0 | 11 |
| raytrace | 16 | 28 | 33 (`jit_set` 16.9, `post_call` 4.2, `math_min` 2.5, `setmetatable` 2.3, `psetshortstr_after` 2.1, `jit_call`, `precall_c`) | 13 | 4 | 2 | 4 |
| list | 41 | 46 | 11 (`pre_tail_call` 8.0, `jit_tailcall` 3.4) | 1 | 0 | 0 | 1 |
| cd | 24 | 24 | 4 (`jit_set`, `tinsert`) | 11 | 8 | 17 (`materialise` 7.3, `psetshortstr` 5.3, `jit_tr_mat` 4.5) | 12 (exit handler 1.8, compile) |
| towers | 93 | 3 | 0 | 0 | 0 | 0 | 4 |

The interpreter (`run_mode`) is below 1 % everywhere: "outside traces" means tier 1, not the interpreter. Tier 1 plus
its helpers is 45-60 % of `raytrace`, `deltablue`, `list` and `cd`, 22 % of `binary_trees`, 16 % of `richards`.

Why code stays in tier 1 (verbose recorder logs):
- `binary_trees`: the leaf `tree[1]` on `{}` (integer key absent from an empty table) aborts the side recording `IntMiss`
  4 times, the exit is blacklisted, and every leaf (15 M) leaves `ItemCheck`'s root trace into tier 1, which calls
  `jit_get`. This is the parked CP5 (docs/JIT_PARKED.md C68).
- `list`: `tail`'s continuation heads (pc 17, 22) end in a root-frame `TAILCALL` and abort `TailCall` until blacklisted,
  so `tail` runs in tier 1 and calls `isShorterThan` from tier 1 (34 % of the cycles are `isShorterThan`'s tier-1 code:
  short loops entered from tier 1).
- `richards`: the task bodies are reached by tail calls (`return self.task:run(message)`, `return
  self.scheduler:waitCurrent()`); side recordings abort `LeaveFrame` (18) and `TailCall` (4).
- `raytrace`, `cd`: the vector constructor idiom `return vec(..)` -> `return setmetatable({x=..,y=..,z=..}, V)` is two tail
  calls; `trace()` (raytrace line 63) is recursive with inner loops and polymorphic `objs[i]:hit(..)`; its heads end
  `Nyi(Call)`, `InnerLoop`, `StartRefused` back-offs and blacklisting.
- `deltablue`: 29 prototypes with a few attempts each; the dominant log entries are `StartRefused (Wait)` back-offs and
  `LeaveFrame` side aborts (72), `InnerLoop` (30), `LeaveLoop` (35).

Exits: `fib`'s 39 M exits are the leaf guard, linked to a side trace (cheap). `binary_trees`' 30.5 M are mostly the
blacklisted leaf exit (expensive: tier 1). `cd` materialises 3.87 M sunk tables at exits (`[trace] sink: ...
materialised 3870200`); about 1.2 G instructions are spent in `materialise` alone, ~300 instructions per table.

## 3. Per-operation costs (micro-benchmarks, 10 M iterations, per iteration)

Files: one `for i = 1, N` loop that stores the new object into `keep[i % 64 + 1]` (escapes, so not sunk); generated by
the study script (not committed). node numbers are marginal costs (50 M minus 10 M iterations).

| op | rn trace cyc / ins | LuaJIT cyc / ins | node cyc | rn tier 1 only (`TRACE=off`) cyc / ins |
|---|---:|---:|---:|---:|
| loop + array store (base) | 3.2 / 22 | 3.4 / 12 | 3 | - |
| `{}` | 48.8 / 279 | 71.7 / 376 | ~16 | 54.4 / 320 |
| `{t, t}` (array part 2) | 71.1 / 426 | 85.0 / 435 | - | - |
| `{a = i, b = i}` | 77.1 / 474 | 127.6 / 711 | ~16 | 139.3 / 900 |
| `setmetatable({a, b}, V)` | 85.6 / 532 | 133.3 / 712 | - | 192.9 / 1201 |
| `vec(i, i, i)` (constructor function, 3 fields + metatable) | 99.4 / 593 | 148.5 / 806 | - | 289.7 / 1736 |
| same, result not escaping (sunk) | 2.4 / 13 | 3.3 / 6 | - | - |
| inlined call `f(s, i)` | 1.7 / 10 | 3.3 / 6 | - | 29.1 / 172 |
| inlined tail call `return g(a, b)` | 2.3 / 18 | 3.3 / 6 | - | 75.6 / 418 |
| method call `o:get()` | 1.6 / 12 | 3.1 / 4 | - | - |
| recursive call (depth 10), per call | **17.1 / 105** | **4.0 / 24** | - | - |

Reading:
- Allocation in traces is already cheaper than LuaJIT's, but 3-5x node's cycles. Profile of the `{}` loop
  (instructions): `jit_tr_alloc` 45 % (~126 instructions per `{}`: the Rust frame, state/exit-area setup, the dispatch
  `match`, `new_table_bytes`, `jit_alloc_ok`, the `Mem` layer's dyn allocator call, header init, `allgc` link), `free_obj`
  22 % (~60 per dead table), trace code 25 %, the rest of the GC 7 %. With a hash or array part, `resize` and a second
  pool block add ~150 more. The pool pop itself is 11 instructions but its `next` load misses (LIFO free list of
  blocks freed by the sweep).
- Inlined calls, method calls and tail calls inside traces are fine. **Recursive calls cost 4.4x LuaJIT's instructions.**
- Tier 1 is expensive per operation: a Lua call ~100 instructions (call sequence ~45 + prologue/return), a tail call
  ~250 more, `setmetatable` ~300 (a C call through `jit_call` / `precall_c` / `post_call`), a new-key `SETFIELD` ~200
  (`jit_set` -> `psetshortstr_after`). Every program with a large tier-1 share pays these.

## 4. Program breakdowns

### binary_trees (allocation + GC + leaf exit)

Per tree node (19437 M / 30.1 M nodes = 646 instructions; LuaJIT 475; instruction-sample shares):

| part | instr / node | share |
|---|---:|---:|
| allocation helpers (`jit_tr_alloc` 98, `resize` ~45 avg = ~89 per `{l, r}`, pool alloc ~24) | ~166 | 25.7 % |
| GC (`free_obj` ~57, `gc_step` ~25, pool free ~14, marking ~12) | ~117 | 18 % |
| leaf `tree[1]` (`jit_get` + `TableRef::get` ~169 per leaf, tier-1 `ItemCheck` ~103 per leaf) | ~135 | 21 % |
| trace code (6 fragments, native call/return, 2 inline barriers per node) | ~230 | 35 % |

Experiments:
- **Prototype: tier-1 `GETI` fast path in `jit_get`** (integer key, table without metatable: `get_int`, store, `H_DONE`
  before the generic chain; scratch patch, ~10 lines): cycles 3976 -> 3797 and 4012 -> 3826 M (**-4.5 %**), instructions
  19407 -> 18593 M (-4.2 %), branch misses 9.5 -> 7.2 M. `jit_get` is still ~130 instructions per call afterwards
  (`get_int` hashing a key on the dummy node, the helper frame).
- Collector mode: incremental 7770 M cycles vs generational 4029 M: generational already helps.
- Nursery size (`collectgarbage("param", "minormul", 25 / 50 / 100 / 200)`): 3923-4109 M, noise. Not a lever: the cost
  is per dead object (sweep + free), not the collection frequency.
- Write barriers (`TBAR`, 2 per internal node, inline colour test) are < 1 %.

### fib (call protocol of recursion)

Four fragments (function-entry trace, leaf side trace, continuations at pc 7 and pc 11), stitched by native push /
return through dispatch cells. Instructions per execution (instruction profile, 78.2 M calls, 39.1 M leaves):

| fragment | instr | runs per call | content |
|---|---:|---:|---|
| entry trace (inner entry loop) | 46 | 1 | POLL 3, BUDGET 2, tag + `n < 2` guard 4, callee identity chain `SLOAD fn; UREF; UVREL; UGE; ULOAD; EQ` ~12, frame push (2 slot pairs, `ci` link, savedpc, room checks stack 4 + ci 3, first flag) ~22, jump |
| leaf side trace | 28 | 0.5 | BUDGET, guard, stitch exit, native return |
| continuation pc 7 | 38 | 0.5 | return-entry check (callstatus, savedpc compare), POLL, BUDGET, identity chain again, push |
| continuation pc 11 + return | 52 | 0.5 | ADD, lazy-root return test, pop, savedpc compare chain to pick the caller's continuation, indirect jump through a cell |

Average 105 per call; LuaJIT 32: its `fib` is an up-recursion trace unrolled 3 levels (a frame push is the function
slot, the frame link and the arguments: ~6 stores and a base bump; the identity guard is one compare per iteration) plus
return traces chained by `RETF` guards (~8 instructions per level). The gap is protocol, as the C68 study found (then
115 vs 31): our frames are `CallInfo` nodes (lazy since C70), every fragment repeats POLL / BUDGET / room checks, the
identity chain runs twice per internal node, and returns dispatch through a compare chain plus an indirect jump.
`towers` (98 instructions per call against LuaJIT's 61, 93 % in traces) and `queens`' recursion have the same shape.

### queens

LuaJIT parity (1899 vs 1873 Mcyc, same branch misses). The node gap is loop and branch code generation on boolean
arrays, not allocation or calls (4.7 M calls for 60 M loop iterations). Out of scope here.

### raytrace, cd, deltablue (tier-1 coverage, constructor idiom, materialisation)

- `raytrace`: 5.7 M `setmetatable` calls and 5.4 M tail calls into `vec`. ~13 % of the instructions are the tier-1 C-call
  glue around `setmetatable` (`post_call` 6.4, `setmetatable` 4.0, `jit_call` 1.8, `precall_c` 1.0: ~180 instructions
  per call), ~20 % the tier-1 constructor (`jit_set` 9.6 + `psetshortstr_after` 4.5 for new keys, `jit_newtable` 3.5,
  `resize`, pool). Sinking matters: `-sink` is +13 % cycles.
- `cd`: materialisation 17 % of the cycles (3.87 M tables, ~300 instructions each in `materialise` plus generic
  `psetshortstr` per field); 6.3 M `setmetatable`; tier 1 24 %. `-sink` +5.5 %. `framemin=0` / `fck=1`: no change.
- `deltablue`: tier 1 34 % with 29 small prototypes, `table.insert/remove` from tier 1 (4.4 %), hash growth (`newkey`,
  `resize_hash` 3.5 %).

### list, richards (tail calls)

- `list`: tier 1 46 % + `pre_tail_call` / `jit_tailcall` 11.4 % (~260 instructions per tier-1 tail call). Root cause:
  root-frame `TAILCALL` aborts the recording of `tail`'s continuations. LuaJIT also falls back to its interpreter
  here ("call unroll limit") and still needs 0.48x our instructions, so its fallback path is much cheaper than our
  tier 1 (not profiled further).
- `richards`: 79 % traced; the remaining tier 1 is the tail-called task bodies.

## 5. Handles, ranked

Gains are cycles per program, estimated from the measured shares and per-op costs (instructions saved x op count), or
measured where a prototype exists. Risk: soundness / GC invariants / coverage needed.

| # | handle | expected gain | risk | effort | files |
|---|---|---|---|---|---|
| H1 | **Absent integer keys.** (a) tier-1 `GETI`/`GETTABLE` integer fast path in `jit_get` (and `jit_set` for existing int keys); (b) CP5: integer `HMISS` in traces (key tag + 64-bit compare on the chain from the integer main position, `asize` / `lsizenode` / no-`__index` guards) so the leaf side trace installs | (a) **bt -4.5 % measured**; (a)+(b) bt -15..20 %; towers / astar / havlak small | (a) low; (b) medium: guards for the absent path, four mutants (bound, lsizenode, metatable, key tag) | (a) S; (b) M | `rn-core/src/jit_helpers.rs`; `rn-jit/src/tr/record.rs` (`table_for`, `IntMiss`), `ir.rs`, `asm.rs` (`Hmiss` aux), `opt_mem.rs`, `mutants.rs` |
| H2 | **Tier-1 OOP / constructor fast paths**: (a) `setmetatable(t, mt)` as a tier-1 intrinsic (table, no `__metatable` on the old mt, no `__gc` in mt; else the C call); (b) `SETFIELD` of a new short-string key into a table with a free node (inline or a lean helper without the generic get + metamethod walk); (c) Lua->Lua `TAILCALL` in tier 1 without `jit_tailcall` -> `pre_tail_call` (fixed args, no hook, no `__call`) | raytrace -20..25 %, cd -5..8 %, list -8..10 %, deltablue -2..4 %, richards -2..4 % | low-medium: tier-1 templates with the C call / helper as fallback; `CIST_TAIL` and hooks keep the old path | M | `rn-jit/src/lower.rs` (tier-1 templates), `rn-core/src/jit_helpers.rs`, `state.rs` (`pre_tail_call`), `rn-std/src/lib_base.rs` (`setmetatable` semantics) |
| H3 | **Inline `TNEW` in traces**: exact debt / limit check from `jit_gc_cells`, pop of the pool's free-list head for the table's size class (fallback helper when empty, or when the state's allocator is not the `PoolAllocator`), header init with the current white, `allgc` link, `total_bytes += size`; then a presized array / node block for constructor sizes the same way | bt -12..18 %; alloc micro `{}` -30..40 %; storage / havlak / json small; cd / raytrace only through what is traced | **medium-high**: GC invariants (white, `allgc` order, accounting identical to `new_table` so TR29 and `collectgarbage("count")` stay exact, memory limit, emergency path), embedder allocators; mutants: debt not counted, wrong white, not linked, wrong class | L | `rn-core/src/alloc.rs` (expose class heads), `gc/mod.rs` (`JitGcCells`), `jit_layout.rs` (Table offsets), `table.rs`; `rn-jit/src/tr/asm.rs`, `ra.rs`, `mutants.rs` |
| H4 | **Cheaper free of dead objects**: a table arm in the sweep that frees header + array + node blocks without `free_obj`'s generic dispatch, the `watch` probe and per-block dyn calls; batch the `Mem` accounting per sweep step | bt -4..6 %, every allocation-heavy program -1..3 % | low (no observable behaviour; GC tests + stress modes cover it) | S-M | `rn-core/src/gc/mod.rs` (`free_obj`, sweep), `table.rs` (`free_table`), `alloc.rs` / `Mem` |
| H5 | **Faster materialisation of sunk tables** at exits: replay the sunk stores through a raw "known-absent short-string key" insert (no lookup, no metamethod check) on a table created with the NEWTABLE's exact sizes (TR26c: same sizes, same insertion order, so the node layout and `next` order stay identical); avoid the per-table generic setup | cd -6..10 %, raytrace -1..2 % | low-medium (TR26c identity: the existing sink tests and `next`-order checks) | M | `rn-jit/src/tr/exit.rs` (`materialise`, `jit_tr_mat`), `rn-core/src/table.rs` |
| H6 | **Root-frame `TAILCALL` in traces**: a trace whose root frame tail-calls a Lua function replaces the frame (move args down, `CIST_TAIL`) and enters the callee's function-entry trace through its dispatch cell (or stitches) instead of aborting | list -25..40 %, richards -5..10 %, raytrace / cd -3..8 % (with H2c the tier-1 part is cheaper already) | medium-high: frame replacement under `lazyci` / `lazy_root`, hooks, instruction-count identity, `debug.traceback` tail marks; twins + mutants | L | `rn-jit/src/tr/record.rs` (`TailCall` abort), `asm.rs`, `exit.rs`, `cache.rs` |
| H7 | **Recursion protocol diet**: callee-identity hoist (C70 b: a per-entry-kind prefix copy plus a family-level "no upvalue store" invariant), POLL only at the function-entry trace (CP3, re-measure cycles with layout control), CP4 first-flag stores, a direct link for returns to a known continuation instead of the savedpc compare chain + indirect jump | fib -15..25 % (identity hoist alone ~13 % instructions per C70), towers / permute -5..10 %, bt -3 % | **high** (family invariant + invalidation; CP3 lost cycles once) | L | `rn-jit/src/tr/asm.rs`, `cache.rs`, `record.rs`, `opt_*` |
| H8 | **Trace coverage of OOP code** (deltablue, raytrace, cd): first make the verbose log say *why* (`StartRefused` sub-reason, the call shape of `Nyi(Call)`, `LeaveFrame` site), then fix the top blockers per program | ceiling: tier 1 + its helpers are 45-60 % of these programs; realistic -10..30 % each | medium; results move with the C36 / C69 formation lottery, measure with several script paths | L (open-ended) | `rn-jit/src/tr/record.rs`, `cache.rs` (start gates, blacklisting) |
| H9 | LuaJIT-style up-recursion traces (unrolled self-calls, frames as stack slots without `CallInfo` nodes inside the trace) | fib up to -50 %, towers -30 % | very high (frame model, debug API, C70 machinery) | XL | research; not recommended now |

Not handles (measured): nursery size (noise), write barriers (< 1 %), the interpreter (< 1 %), side-trace length knobs
for `cd` / `raytrace` (`framemin=0`, `fck=1`: no change), `queens` (LuaJIT parity; a loop-codegen question).

Expected position after H1-H5 (estimates, not measured): binary_trees from 2.49x node to ~1.6-1.8x; raytrace 20x from 6.5x
to ~5x (H8 is needed for more); cd 20x from 5.9x to ~5x; list 20x from 5.0x to ~4.5x (H6 for more); fib unchanged
until H7 (to ~1.4x node).

## 6. Recommended order (one builder at a time, light gates)

1. **H1a** tier-1 integer get fast path (S). Gate: `binary_trees 17` instructions -4 % (prototype: 19407 -> 18593 M),
   `vm_corpus`, the metatable / `__index` corners (`tests/vm`), 223-program instruction check optional.
2. **H1b** CP5 integer `HMISS` (M). Gate: `binary_trees` side trace installs (`[trace] side traces` installed +1, the
   leaf exit not blacklisted), cycles -10 % or better; `tr_core` twins, four mutants caught.
3. **H2** tier-1 `setmetatable` intrinsic, new-key `SETFIELD`, Lua->Lua tail call (M; can be three small builders).
   Gate: micro `ctor` with `RUSTYNAIL_JIT_TRACE=off` (1736 instructions per iteration today), `raytrace 40`, `cd 20`,
   `list 7000`; error / hook / `__metatable` / `__gc` tests.
4. **H4** cheaper free (S-M). Gate: micro `{}` (279 instructions per iteration), `binary_trees`, GC stress 1 / 2 + verify
   on the GC tests.
5. **H3** inline `TNEW` (L). Gate: micro `{}` / `{a, b}` / `{t, t}`, `binary_trees`; `collectgarbage("count")` equality
   tests, memory-limit tests, GC stress, the new mutants. Do H4 first: it shrinks the GC side so H3's gain is visible.
6. **H5** materialisation (M). Gate: `cd 20` (`materialise` 7.3 % cycles today), sink / `next`-order tests.
7. **H6** root-frame `TAILCALL` (L). Gate: `list 7000` (tier 1 46 % today), `richards 500`; `tr_core` twins, mutants.
8. **H7** recursion diet (L, highest risk). Gate: `fib 37` 8211 M instructions today, `towers 900`.
9. **H8** coverage, starting with the diagnostics.

Keep the trace perf-map symbols as a knob (`RUSTYNAIL_JIT_PERFMAP=2` naming `TRACE-<n>` blocks, 4 lines in
`tr/cache.rs::install` plus a raw-range `CodeArena::register` variant in `mem.rs`): every per-trace profile in this study
depended on it.

## 7. Raw notes

- Tier-1 call sequence (dumped `call.lua`, traces off): argument moves, hook byte test, BUDGET, savedpc, closure /
  `JitProto` checks, stack room, `ci->next` and 8 `CallInfo` field stores, indirect jump: ~45 instructions plus ~45 in
  the callee's prologue and return.
- `binary_trees` traces: function-entry roots for `BottomUpTree` (`TNEW` + 2 `ASTORE` + 2 `TBAR`) and `ItemCheck`
  (array bound guard `ULT #0 asize` fails on `{}` = the blacklisted leaf exit); 6 stitches; `jit_tr_alloc` is called for
  every node.
- `cd` exits: top snapshots 1.77 M, 1.33 M, 1.18 M, 1.17 M ... over 100+ traces; 4.4 M of the 27.7 M exits are
  blacklisted ones.
- LuaJIT `binary_trees`: up-recursion root plus a chain of 16 side traces; more L1 misses (161 M vs 96 M) than ours.
