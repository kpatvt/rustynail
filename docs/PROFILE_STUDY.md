# Where the cycles go, ours vs LuaJIT: profile study and LuaJIT source review

Read-only Opus study, base 42695981 (after BUNDLE-5). Raw JSON was in the session scratch directory (stat.json, stat3.json). Measured under load 3.4-4.5 (a maintainer test suite was running); repeat runs were stable (spread < 2%), but treat absolute numbers as +-2%.

Method: release `rnlua --features trace` (debug=1); 14 programs (awfy richards, cd, deltablue, json, havlak, bounce, storage, queens, towers; apps raytrace, lru_cache, markdown, expr_lang, events); CPU 2, `RUSTYNAIL_SEED=0`, `warm=0` plus a default-mode confirmation (perf totals 2405 vs 2409 Mcyc: no difference); `perf stat` min of 7 (ours/LuaJIT) or 5 (variants); `perf record -c 150001` summed over 5 runs (smaller periods are throttled by perf_event_max_sample_rate=49000, so the earlier P-PROF/coverage profiles were probably under-sampled, percentages should still hold); LuaJIT symbols via debuginfod.

## Headline

Most of the gap is code that LuaJIT runs in traces and we run in tier-1. Next: table-runtime slow paths, generational GC, exits, per-trace compile cost. Coverage bundles did not pay off because each extra trace costs ~2.5x what it costs LuaJIT and many of our traces barely beat tier-1. **Tier-1 is at LuaJIT-interpreter parity** (ours `JIT_TRACE=off` / `luajit -joff` = 0.73-1.32 per program). Only apps < 1.5 looks reachable; awfy lands ~1.65-1.75 with everything below.

The old "other 27% / 50%" bucket was table runtime, GC, allocator, libc and helpers; "other" is now 0.1%.

## Cycle categories, ours (warm=0) / LuaJIT, Mcyc

Columns: t1 = tier-1 code, int = interpreter (LuaJIT: lj_BC_*/lj_vm_* incl. its inline table ops), call = out-of-line call machinery, tbl = table runtime (jit_get/jit_set/finish_set/resize/mainposition/insertkey/newkey/len; LuaJIT lj_tab_*/lj_meta_tget), alloc = pool/glibc/jit_tr_alloc/string alloc+intern, comp = compile (ours incl. tier-1 compile and parser), libc+ = memmove etc.

| program | trace | t1 | int | call | tbl | alloc | gc | lib | exit | comp | libc+ | total |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| richards | 43/29 | 3/0 | 1/0 | 0/0 | 1/0 | 6/0 | 0/0 | 0/0 | 3/0 | 17/5 | 3/1 | 77/36 |
| cd | 57/33 | 65/0 | 3/10 | 5/5 | 22/9 | 42/22 | 23/7 | 2/1 | 22/3 | 48/25 | 7/3 | 296/116 |
| deltablue | 20/16 | 27/0 | 6/3 | 2/2 | 11/7 | 16/6 | 4/2 | 2/1 | 5/0 | 29/78 | 5/3 | 127/117 |
| json | 26/11 | 36/0 | 3/2 | 7/0 | 22/16 | 35/23 | 10/5 | 22/5 | 5/0 | 19/7 | 13/8 | 198/76 |
| havlak | 10/20 | 26/0 | 5/5 | 3/1 | 55/30 | 26/25 | 36/15 | 5/0 | 4/1 | 17/29 | 5/5 | 191/130 |
| bounce | 64/39 | 1/0 | 1/0 | 0/0 | 2/1 | 4/4 | 6/2 | 0/0 | 1/0 | 5/1 | 2/1 | 86/49 |
| storage | 17/23 | 10/0 | 0/9 | 0/0 | 59/49 | 35/57 | 36/15 | 0/0 | 0/0 | 2/1 | 21/5 | 182/157 |
| queens | 83/53 | 28/0 | 1/0 | 0/0 | 3/3 | 8/2 | 0/0 | 0/0 | 24/0 | 14/2 | 4/1 | 166/61 |
| towers | 12/19 | 75/0 | 0/0 | 0/0 | 35/0 | 1/0 | 0/0 | 0/0 | 6/0 | 3/5 | 2/1 | 134/26 |
| raytrace | 38/10 | 31/0 | 1/8 | 8/1 | 26/3 | 27/10 | 10/4 | 7/1 | 7/0 | 36/4 | 4/1 | 195/42 |
| lru_cache | 0/33 | 65/0 | 19/8 | 10/0 | 63/47 | 9/17 | 27/9 | 6/3 | 3/1 | 4/6 | 5/2 | 211/127 |
| markdown | 0/4 | 6/0 | 7/5 | 7/6 | 8/3 | 18/49 | 9/11 | 101/241 | 0/2 | 5/3 | 11/11 | 174/331 |
| expr_lang | 33/38 | 35/0 | 3/7 | 11/2 | 24/13 | 21/17 | 11/8 | 10/6 | 3/2 | 22/9 | 9/4 | 181/104 |
| events | 13/27 | 71/0 | 3/44 | 18/7 | 40/24 | 29/23 | 6/7 | 1/0 | 1/2 | 5/2 | 5/3 | 193/139 |
| **sum excl. markdown** | 416/350 | 472/0 | 46/96 | 64/20 | 364/203 | 257/205 | 171/73 | 56/17 | 83/9 | 220/174 | 84/38 | **2237/1180** |
| **% of gap (1057)** | 6.3 | **44.7** | -4.8 | 4.2 | **15.3** | 4.9 | **9.3** | 3.7 | **7.0** | 4.4 | 4.4 | 100 |

Absolute (perf stat min Mcyc): richards 76.1 vs 35.0 (2.17), cd 288.6 vs 107.2 (2.69), deltablue 124.0 vs 111.5 (1.11), json 192.3 vs 73.9 (2.60), havlak 182.2 vs 118.0 (1.54), bounce 84.5 vs 48.1 (1.76), storage 178.6 vs 154.5 (1.16), queens 162.4 vs 55.0 (2.96), towers 128.5 vs 24.0 (5.35), raytrace 190.9 vs 40.6 (4.70), lru_cache 206.4 vs 114.7 (1.80), markdown 171.0 vs 319.9 (0.53), expr_lang 177.8 vs 96.4 (1.84), events 188.6 vs 142.9 (1.32). LuaJIT spends 66% of deltablue compiling (its own thrash).

## Tier-1 parity, traces do not add enough

| program | LuaJIT JIT speedup over its interpreter | ours traces vs tier-1 only |
|---|---|---|
| cd | 2.84x | 1.10x |
| json | 2.42x | 0.91x (traces lose) |
| raytrace | 4.07x | 1.14x |
| towers | 6.3x | 1.38x |
| lru_cache | 1.65x | 0.99x (0 traces installed; LJ installs 163) |
| expr_lang | 1.74x | 1.13x |
| events | 1.41x | 1.16x |
| deltablue | 1.11x | 1.04x |

LuaJIT installs far more traces: cd 705 vs 261, expr_lang 405 vs 155, towers 78 vs 6.

Compile cost per trace: ours ~150k cycles per recording attempt (219 Mcyc / ~1490 attempts incl. tier-1 compile), LuaJIT ~60k. Top compile symbols: Rec::lockstep 22.5, Assembler::rm_op 19, ra 17, x86::emit 12, lower_ex 11.5, konst 8, shape_hash_ir 6, SipHash/hashbrown ~6.

Exits: ~9.7M exits over the 14 programs at ~8.5 cycles Rust-side handling (83 Mcyc) vs LuaJIT 11 Mcyc; the real cost is the tier-1 continuation. GC-driven allocation-refusal exits are negligible.

## Microbenchmarks (cycles per op, min of 5)

| operation | interp | tier-1 | trace | LJ -joff | LJ |
|---|---|---|---|---|---|
| Lua call+return f(i) | 24.6 (204 instr) | 20.8 (128 instr) | inlined | 10.9 (69 instr) | inlined |
| o:get() via __index table | 60 | 53 (361 instr) | inlined | 49 (279 instr) | inlined |
| recursive call (fib) | 55 | 27-36 | 17.6-19.8 | 36 | 6.0 |
| {} incl. GC | 93 | 88 | 80 | 97 | 74 |
| {x=i,y=i,z=i} incl. GC | 262 | 285 | 182 | 202 | 147 |
| same, GC stopped | | 213 | 104 | 181 | 120 |
| setmetatable({x,y,z},C) | 317 | 341 | 200 | 219 | 147 |
| {i,i,i} | 138 | 133 | 107 | 130 | 87 |
| closure creation | 155 | 154 | 157 | 165 | 168 |
| "k"..i | 220 | 204 | 184 | 385 | 293 (we win) |
| dynamic string-key t[k] (net) | 18 | 9.8 | 6 | 11 | ~1 |

GC cost per dead short-lived object: `{}` 50 cycles generational, 35 incremental, 25 LuaJIT; `{x,y,z}` 110 generational, 46 incremental, 50 LuaJIT. A young collection costs ~4.8k cycles on a 40 KB heap and runs every ~44 tables; `atomic2gen` shows in the profile (bounces into majors on small heaps). Raw allocation is cheaper than LuaJIT's ({} 50-59 vs 73-90 with GC stopped).

Hot templates: empty FORLOOP+ADD loop 34 instr / 4.4 cycles (LJ -joff 37 / 12); GETFIELD hit ~32 instr vs TGETS 14+6 (we recompute the mask from lsizenode, LJ stores hmask); RETURN1 ~40 instr vs RET1 ~16; array store 43 instr / 5.8 cycles vs 70 / 16.

## Ranked causes of the gap (13 programs excl. markdown; gap 1057 Mcyc)

1. Tier-1 execution of code LuaJIT runs in traces (towers 75, lru_cache 65, cd 65, events 71, json 36, expr_lang 35, raytrace 31, queens 28 Mcyc): ~45%, high confidence.
2. Table-runtime slow paths (jit_set 53, resize 49, mainposition 47, newkey 53, insertkey 33, finish_set 31, len 18): ~15%, high.
3. GC, generational minor-collection overhead (propagatemark 79, free_obj 24, gc_step 20, reallymark 16, atomic2gen 14): ~9%, high.
4. Exits plus Rust exit handling (queens 24, cd 22, raytrace 7): ~7%, high.
5. Trace code itself: ~6%, medium.
6. Allocation (jit_tr_alloc dispatcher 93, glibc malloc for blocks > 512 B and Rust Vecs 70): ~5%, medium.
7. Compile at 2.5x LuaJIT per trace: ~4%, high.
8. Call machinery (tier-1 21 vs 11 cycles; trace recursion 18 vs 6): ~4% plus part of 1, medium.
9. libc memmove/memset (unattributed, DWARF unwinding fails through JIT frames): ~4%, low.

## LuaJIT techniques worth porting (ranked)

[M] measured, [E] estimate (earlier estimates ran 2-5x high).

1. **GC pacing / cheaper generational minors** (`gc/generational.rs: young_collection` runs a full `atomic()` per minor, minormul 20 on tiny heaps). [M] `collectgarbage('param','minormul',100)`: awfy 0.963, apps 0.962 (lru_cache 0.918, storage 0.918, havlak 0.923, bounce 0.925, towers 0.943; worst queens +1.1%, richards +1.8%). [M] `RUSTYNAIL_GC_MODE=inc`: awfy 0.967, apps 0.979 (events +2.7%). Risk low but parameter defaults / `collectgarbage` return values / finalizer timing are observable. Effort S; deeper fix (find the ~4.8k fixed cost per minor, stop bouncing into majors on small heaps) effort M.
2. **Interpreter-style nil-slot stores** (vm_x64.dasc BC_TSETS: existing key with nil value, or empty array slot, stores inline after `metatable == NULL || nomm & __newindex`). Ours: `templates_table.rs` SETFIELD only handles existing non-nil nodes; everything else calls `jit_set` then `finish_set`. [M] towers rewritten to avoid nil-valued fields: 129 -> 104 Mcyc (-19%). [E] awfy -2..-3%, apps -0.5..-1.5%. Effort S-M. Also SETI/SETTABLE and the interpreter.
3. **Cheaper table constructors** (LuaJIT TDUP). Ours: `jit_newtable` -> `new_table` + `resize`, then `jit_set` per new key. [M] tier-1 `{x,y,z}` 285 vs LJ interp 202; with metatable 341 vs 219 (tier-1 slower than our interpreter). Full template is risky (pairs order must match PUC); port inline NEWREF of constant keys into tier-1/interpreter instead. [E] raytrace -4..-8%, apps -2..-3%, awfy -1%. Effort M.
4. **Compile cost per trace** (ours ~150k vs ~60k). [M] ceiling: compile is 9% of cycles; 2x speedup ~ -4.5% (richards -10%, deltablue/raytrace -9%, cd -8%). Strategic: every extra trace costs 150-450k cycles, which is part of why coverage bundles lost. Effort M-L.
5. **Drop traces that lose to tier-1.** [M] json 192 vs 175 tier-1 only (-9% available); havlak, lru_cache, markdown 1-4% worse with traces. Policy: flush when stay cycles per entry fall below tier-1 cost. [E] awfy -1%. Effort S-M.
6. **Thin tier-1 call frame** (LJ two stack slots, no CallInfo, ins_callt ~7 instr, RET1 ~16; ours ~45 per CALL, ~40 per RETURN). [M] 20.8 vs 10.9 cycles/call. [E] towers -8..-11%, events/lru_cache/cd -2..-4%. Lazy savedpc/top, no budget decrement on return, interrupt check folded into the budget. Risk medium (C API, debug hooks). Effort M-L.
7. **Coverage of lru_cache** (LJ 163 traces; ours 25 recorded, 0 installed): [E] -15..-30% only if compile cost (4) is fixed first. Effort L.
8. **Trace recursion cost** (6 vs 18-20 cycles/call): low priority.

Not worth porting: inline trace allocation (LuaJIT asm_tnew is a C call too), dlmalloc fast path (our pool beats it), string interning (we win `"k"..i`), hmask vs lsizenode (2-3 instr), colocated array part.

## Recommended bundles

- A (S): items 1 and 2. [E] awfy -5..-6%, apps -4..-5%.
- B (M): items 3 and 5. [E] apps -3%, awfy -2%.
- C (M-L): item 4, measured per pass.
- D (L): item 6, then re-attempt coverage (item 7, BUNDLE-2/5 leftovers) on top of B and C.

## What did not pan out / could not be measured

GC-step exits are negligible (counts unchanged under incremental or minormul 100); template SETFIELD with a metatable is not the towers cost (the nil value is); callers of memmove (62 Mcyc) and glibc malloc (70 Mcyc) are unattributed (DWARF unwinding fails through JIT frames); no perf-map entries for traces, so inline exit-stub cost is inside "trace"; single-exit cost incl. the tier-1 continuation could not be isolated; LuaJIT trace code is one bucket; all runs under load 3.4-4.5.

## Reachability

apps (1.61) < 1.5: probably yes (A + B + a lru_cache trace -> ~1.40-1.48, medium confidence). awfy (1.93 all 14; 2.11 on this 9-program subset): not with these items; A+B+C+D credibly -12..-18%, landing ~1.6-1.7. Closing awfy needs the trace JIT to gain 2-4x over tier-1 on OOP code as LuaJIT's does, which needs LuaJIT-level trace counts, affordable only after compile cost per trace drops ~2.5x and losing traces are dropped automatically. Validate each estimate with a throwaway patch measured by `perf stat` (min of 7, SEED=0, warm=0, interleaved) before building.
