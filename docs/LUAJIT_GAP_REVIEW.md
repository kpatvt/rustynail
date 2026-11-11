# LuaJIT gap review (main 6a257ac, read-only Opus review, 2026-11-03)

Data: min cycles:u over 9 runs, CPU 4 pinned, RUSTYNAIL_SEED=0, quiet machine (Zen 5). Geomean ours/LuaJIT: programs(25) 1.19, jit-trace(29) 1.18, real(5) 1.72, lib(90) 3.28.
Raw review data (not committed): scratch directory/opus_review/ (gap1 table, perf profiles, -jv/-jdump, dumps, scen.py/split.py). Benchmark outputs: scratch directory/lj2out/.
Orchestrator note: the percentages below are the reviewer's judgement (+-10 points); recovery estimates are estimates, not measurements.

## Answer: feature parity is not result parity
Share of the gap's log-weight over the 59 programs/jit-trace/real programs: ~55% implementation quality, policy and runtime under the JIT; ~34% missing algorithms (features exist but with less reach than LuaJIT's); ~12% Lua 5.4/5.5 targeting (partly paid back by the integer subtype). ~63% of the lib gap (3.28) is benchmark artifact: without the 28 folded/dead rows lib geomean is 1.36 (62 rows), 1.54 with lib_var substitutes.

### Quality / runtime (largest)
* Our interpreter/runtime is 1.15-3.7x slower than LuaJIT's interpreter (our JIT=off vs luajit -joff): tfor_loops 3.67, table_hash 2.67, coroutines 1.87, lua_interp 1.79, fib 1.57, numeric 1.15-1.3, sieve 0.85 (only win). This shows wherever traces do not run (warm-up, after exits, library-bound code).
* Runtime bugs: `number::tostring_int` is 31.8% of str_concat (83.7% of its samples on one movaps after a PLT memcpy: store-forwarding stall, ~84 cycles/conversion); `TableRef::next` is 65% of tfor_loops (`findindex` rehashes the key every step; 29.6M L1 misses vs LuaJIT 0.09M); the generic `jit_tr_alloc` bracket runs on every string probe and every TNEXT even if the string exists (14-18% of tfor_loops, knucleotide, str_concat).
* Backend emits 2.5-5x LuaJIT's instructions for nearly the same IR (field_method: 11 IR ops vs 12, ~49 x86 instr vs 16; 8.1 vs 4.2 cycles/iter). ~6 usable GPRs, loop-carried PHIs live in spill slots (store-to-load chain is the real latency cost); 5.5 negative-offset array addressing = 5 instr/access; HREFK = 3 compares vs 1 64-bit compare; POLL+BUDGET ~6 instr/iter; HSTORE re-stores the tag byte; redundant metatable==0 guard on proven non-nil reads; loops not rotated.
* Policy is NOT a lever: hot thresholds 56,10 or 16,4 move nothing beyond +-3% except game_loop (-9%). Compile cost matters only on game_loop (JIT compile 15% + ~9% malloc of 50 Mcyc).

### Missing algorithms (trace tree less connected than LuaJIT's)
* Side traces cannot return below the root frame (`side aborted LeaveFrame`): type_entry, nbody, field_method, tail_loop, short_inner. LuaJIT links the loop-exit side trace into the caller (0 aborts). We pay full exit + tier 1 + re-entry (type_entry ~90 of 192 cycles per inner loop; short_inner tier 1 = 39%).
* Short inner loops not unrolled into the outer trace (LuaJIT nbody root = outer loop with j loop inlined).
* No recursion-as-loop (up/down recursion traces): fib 39.0M exits for 39.1M calls, ~250 instr / 35 cycles per call vs LuaJIT 65 / 12.
* No ITERN-style carried pairs index. No TDUP / inline allocation / sinking through setmetatable (metamethods ~140 cycles per table).
* Few fast-function recorders: library calls via `jit_call` at 50-90 cycles (MAX, floor, ^2->MUL, STRREF/XLOAD, str.len, inline append/pop are IR in LuaJIT).
* No constant-closure / immutable-upvalue folding: tail_loop pre-roll is a 12-level dependent pointer chase (IPC 3.1 vs 5.6).

### Lua 5.4/5.5 targeting (small, partly paid back)
5.4 for-loop keeps an iteration count; integer % needs idiv + sign fix; int->float accumulator makes entry types unstable (type_entry); 5.5 array layout puts tags on a separate cache line (matmul 3.1 vs 1.3 L1 misses/iter); byte-serial PUC string hash kept for PUC-identical pairs order; exact hooks + budget cost 3-4% (programs), ~10% (micro loops). Integers pay back: int_sum 0.39, int_rec2 0.21, string_key_const 0.37.

### Lib artifacts
18 of 90 rows leave only `ADD ADD LE PHI PHI` in LuaJIT's loop (rep, rep_sep, fmt_s, tonumber_flt, tonumber_int, getmt, tunpack, str_eq, find_plain, sub, byte_multi, str_method_len, str_lt(_short), len_op, str_eq_dyn, type_call, len_tbl); 10 more never create the result (fmt_q, reverse, lower, upper, concat_op_str, setmt TNEW sunk, rawget_set, pcall_ok, assert_ok, sub_1char). Fix the generator: vary arguments, consume results (as lib_var does).

## Ranked gap table (class: a language, b representation, c missing algorithm, d implementation shortfall, e policy, f outside trace compiler)
| program/family | ours/LJ | cause | class | fix | est. ours/LJ after | effort | conf |
|---|---|---|---|---|---|---|---|
| rec_loop | 3.69 | no up/down-recursion traces; ack in tier 1 (38%) | c,d | recursion as loop | ~2.2 | L | med |
| table_hash | 3.37 | PUC byte hash + strtab chain walk in concat_result (21.5%), memcpy 15%, interpreter 2.67x | f,d,a | tostring_int fix; interpreter/table fast paths | ~3.2 (S only) | S/XL | med |
| lua_interp | 2.99 | interpreter 1.79x; tier1 42% + interp 28%; mutual recursion | f,c | down-frame sides, recursion; interpreter | ~2.4 | L/XL | med |
| fib | 2.93 | one stitching exit per call | c | recursion as loop | ~1.45 | L | med |
| tail_loop | 2.77 | LeaveFrame; 12-deep upvalue chase | c | down-frame sides + closure/upvalue fold | ~2.3 | L+M | med |
| tfor_loops | 2.49 | next->findindex per step (65%) + TNEXT bracket (14%) | c,d | carried node index (ITERN) | ~1.4 | S/M | high |
| game_loop | 2.24 | compile ~25%; allocation; SideShort/SideUntraced | e/d,c | compile diet, TDUP/sinking | ~1.7 | S/M | med |
| coroutines | 2.22 | Rust ccall/unroll/auxresume/CallInfo switch; 0 traces | f,b | coroutine fast path | ~2.2 | L/XL | med |
| nbody | 2.12 | short inner loops traced separately (9M entries/1M calls); backend | c,d | unroll inner loops; down-frame sides; backend | ~1.35 | L | med |
| knucleotide | 2.12 | jit_tr_alloc bracket even when string found (18%); byte hash | d,a | inline found fast path | ~1.6 | S/M | med |
| str_concat | 2.09 | tostring_int stall (31.8%), probe (28%) | d,a | write digits in place | ~1.5 | S | high |
| metamethods | 1.99 | alloc+GC ~53% (~140 cycles/table) | c,f | TDUP + bump alloc + SETMT sinking | ~1.2 | M/L | med |
| type_entry(+_stable) | 1.97/1.90 | LeaveFrame transitions; int->float entry | c,a | down-frame side link | ~1.1 | L | med |
| field_method | 1.91 | ~6 GPRs, PHIs spilled, 3-compare HREFK, tag re-store, POLL/BUDGET, idiv spills | d,b,a | backend diet (register file first) | ~1.45 | L | med-low |
| matmul | 1.89 | 5.5 split tag lines; 5 instr/array access; redundant mt guard; POLL/BUDGET | b/a,d | addressing, guard elision, registers | ~1.6 | M | med |
| load_big | 1.69 | Rust parser 1.7x LuaJIT's | f | parser/lexer work | - | M | high |
| json/wordfreq/json_roundtrip | 1.58/1.48/1.40 | C calls out of traces (jit_call, CFunc aborts); patterns/format via libc | c,f | fast-function recorders; Rust formatter | -7..10% | M | med |
| short_inner | 1.49 | LeaveFrame side aborts (tier 1 39%) | c | down-frame sides | ~1.0 | L | med |
| sieve | 1.47 | memory bound; extra L1 misses (tag lines); 4x instructions | b/a,d | backend | ~1.3 | - | med |
| spectral_norm | 1.47 | backend instructions; a=0 int entry | d,a | backend | ~1.3 | L | med-low |
| lib s:len / # | 13.2 (var) | method resolution + C call ~81 cycles | c | str.len as IR | 1-2x | S | high |
| lib math.max/floor/^2/sin | 7.8/5.4/3.9/2.9 | jit_call 50-90 cycles | c | intrinsics; ^2->MUL fold | 1-1.5x | S each | high |
| lib sub/byte/find plain/char | 6.9/6.0/5.5/2.2 (var) | call protocol + allocation | c | recorders (STRREF/XLOAD/SNEW) | ~1.5x | M | med |
| lib format %g/%f/%x | 2.7-4.3 | libc snprintf | d | Rust formatter | ~1.5x | M | med |
| lib math.random | 4.6-5.4 | RNG state in a Lua table | d | state in the C closure | ~1.5x | S | high |
| lib it_* pairs/next | 1.5-3.3 | findindex per step | c | ITERN index | ~1.2x | S/M | high |
| lib tinsert/tremove/tmove | 1.9-2.5 | helper calls | c | inline array ops | ~1.2x | M | med |

## Split
| set | net | gap-only | language | algorithm | quality/policy/runtime | wins factor |
|---|---|---|---|---|---|---|
| programs | 1.19 | 1.39 | 16% | 24% | 60% | x0.85 |
| jit-trace | 1.18 | 1.35 | 11% | 40% | 49% | x0.88 |
| real | 1.72 | 1.72 | 0% | 43% | 57% | - |
| all 59 | 1.22 | 1.40 | 12% | 34% | 55% | x0.88 |
About half of the quality share is outside the trace compiler (interpreter, string/table runtime, parser, coroutine switch); half is the backend.

## Reviewer's proposed plan
1. S fixes (high confidence): tostring_int in place; next/TNEXT carried node index (interpreter + traces); inline "found" path for string probe; ^2->MUL; math.floor/max/sin + string.len intrinsics; RNG state out of the Lua table.
2. Side traces returning below the root frame and linking + unroll short inner loops into the outer trace (L). (M1's "no gain" was a stitch to the interpreter, not recording into the caller.)
3. Backend: throwaway experiment first (fewer pinned registers in traces, measured on field_method and matmul); then loop-carried PHIs in registers, loop rotation, scaled 5.5 array addressing, metatable-guard elision, 64-bit HREFK compare vs hoisted constant, no tag re-store, merged POLL+BUDGET.
4. TDUP / bump allocation / SETMT sinking; fast-function recorder families by real-program frequency; direct C ABI for pure helpers instead of jit_call.
5. Recursion as loop + constant-closure/immutable-upvalue folding.
6. Compile-cost diet; Rust number formatter.
7. Decide on the interpreter (it caps every real-program number).
Estimated if 1-6 land: programs 1.19->~1.05, jit-trace 1.18->~0.96, real 1.72->~1.50; table_hash, lua_interp, tail_loop, coroutines, rec_loop would still be 2.2-3.2x. ~1.3-1.5x on library/interpreter-bound real code is the floor without an interpreter rewrite.

## Benchmark bias and wins
Bias: in-house set favours numeric loop nests and constant-key fields; few polymorphic-OOP, tree-walker, string-builder, callback-heavy programs; bench/lib uses constant args and unconsumed results. Add: are-we-fast-yet Lua ports (DeltaBlue, Richards, Havlak, CD, Json, Storage), scimark, ray/life/meteor/pidigits, pure-Lua md5/sha1/crc32, real libraries (dkjson/lunajson, lua-parser, serpent, Penlight).
Wins: pcall_error 0.30 real but measures LuaJIT's slow unwinding (PUC 2.1x faster than LuaJIT); loops_arith 0.38 mostly a variant artifact (LuaJIT variant emulates >>,~,& with floor/bit.bxor/% arithmetic: rewrite with bit.* before quoting); closures 0.57 real vs LuaJIT 2.1 but is an NYI fallback (FNEW, 13+ aborts); int_rec2/int_sum 0.21/0.39 real (int64 vs double<->int); string_key_const 0.37 real (acc+1+3 folds to +4); tfront_1k 0.22 real (N2 memmove); error_tbl/pcall_err/find_pat/sel_loop real but LuaJIT weaknesses.

## Uncertainties
L/A/Q fractions are judgement; backend recovery argued from two disassemblies + BD's partial ceilings (register-file effect itself not measured); down-frame side trace gain assumes linked side traces cost ~10-20 cycles; one Zen 5 machine (high IPC hides the instruction excess; lower-IPC cores would show larger gaps); LuaJIT is stripped (runtime costs inferred); no 5.4 build measured; m1_rdparse LuaJIT figure (2.2 Mcyc) not investigated.

## Follow-up (H1)
The lib artifacts, the `loops_arith` variant and the `m1_rdparse` figure were fixed or invalidated in H1; independent AWFY/application suites were added. Results and the new ranked gap list: docs/JIT_TRACE_REF.md section "H1 decisions and results". Summary: lib geomean 3.80 -> 1.85 with varying arguments; `loops_arith` ours/LuaJIT 0.56 (old variant) -> 0.87 (bit.* variant); `m1_rdparse` LuaJIT figure was a fully folded loop (varying-input version: ours/LuaJIT 3.2); awfy geomean 2.58, apps 2.07.
