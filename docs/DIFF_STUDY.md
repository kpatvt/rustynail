# LuaJIT vs ours differential study (Opus, read-only, base f9f3461a, 2026-11-04)

Method: LuaJIT -jv/-jdump + perf vs our RUSTYNAIL_JIT_VERBOSE=3/DUMP + perf with PERFMAP, CPUs 6/7/9. [M] measured, [X] experiment, [E] estimate. T3 (tier-1 inline __index) landed AFTER this study; its item is done.

## Headline
1. Gap is mostly COVERAGE not code quality. LuaJIT runs towers/richards/queens 77-85% in traces; we run 9% / 0.7% / 45%. raytrace and lru_cache enter NO trace. Traces are slower than no traces on deltablue (131 vs 171 Mcyc), expr_lang, raytrace, lru_cache.
2. LuaJIT builds one root per hot loop/recursion + dozens of side traces linked back (towers 46 links, richards 62, queens 27, cd 297). We flush trees (Ineffective, Rerec on type-guard exits), blacklist (FnShort), or abort sides (LeaveFrame, InnerLoop).
3. Most loops are NOT peeled: any trace with TLEN, LIB, SNEW, TNEW, CONCAT, STRTO stays rolled (`opt_mem::header_safe` false) -> no LICM, SLOAD guards and POLL/BUDGET every iteration.
4. permute is a LuaJIT artifact (trace thrash, 14.6-1476 Mcyc over 11 runs; ours 69 stable): min/min 4.7, median/median 1.03. Quote permute/events/expr_lang on medians of 11+ runs.
5. On json, cd, raytrace, events, lru_cache LuaJIT itself spends 54-80% outside traces (VM, alloc, table, string, GC): partly runtime-vs-runtime.

## Experiments (cycles new/base, same binary)
E1 methods copied into instances (no __index walk): towers 0.68, richards 0.79, queens 0.77. E2 `ineffective_never`: richards 0.68, deltablue 0.83. E3 towers without key 0 in hash: 0.89. E4 raytrace all-float literals: 0.87 (instr 0.74), trace entries 0 -> 88k. E5 json precomputed chars: 0.91. E6 traces on vs off: deltablue 1.30, expr_lang 1.10 (traces hurt). E7 header_safe true for TLEN/LIB/SNEW/TNEW/CONCAT/STRTO (unsound patch): len_op 0.29, str_method_len 0.25, byte 0.57, pcall_ok 0.59, byte_multi 0.69, str_eq_long 0.83, sub 1.38 (worse); no program changed.

## Per-program causes
- towers 7.4: no recursion in traces (LJ: one up-recursion RETF trace, 46 sides); family flushed Rerec then blacklisted; tier-1 __index ~25%; piles[0] in hash (IntMiss).
- richards 5.5: Ineffective flush kills the scheduler tree (2164 entries, flushed with 49 sides); 17 sides abort InnerLoop, 2 TailCall.
- queens 2.7: down-frame exits: 1.83M exits for 0.69M entries, `return true/false` sides abort LeaveFrame (~30% in exit handler + continuation); compile 12% (192 recordings, ~20 Mcyc vs LJ 1.7).
- cd 2.5-2.8: compile 11% (445 recordings at ~83k cyc vs LJ ~41k), 21 LeaveFrame + 14 SideShort, 24 FnShort, alloc+GC 12% (LJ sinks 204 of 635 allocs, we 0), tier 1 37%.
- json 2.7: runtime-bound both sides; jit_tr_alloc, libc, 1.96M exits; sub stitch only 0.91 (E5).
- raytrace 5.6: int/float subtype at same field sites -> HLOAD int guards fail -> Rerec -> blacklisted, never entered (E4); LeaveFrame sides; alloc via jit_set/mainposition/resize 15%.
- expr_lang 2.4: 44% interpreter + raw_equal 13% (string EQK); tier 1 3%, traces 1.2%; why eval leaves tier 1 not yet checked.
- lru_cache 1.7-2.2: 0 traces; IntMiss 14, TableHash 8, Nyi(Setupval) 3, Metamethod abort at __call site, FnShort: conjunctive blockers, measure together.
- events 2.1/1.3 median: LJ interprets emit itself (VARG/FNEW NYI); ours 237k exits at a short inner loop (n=1-4), alloc 12%.
- Lib rows: len_op/str_method_len/byte/byte_multi/pcall_ok = rolled loops (E7); str_eq_long: representation (LJ interns all strings), ~5x stays; it_next_empty, find_plain, fmt_*: CFunc abort, never traced.

## Cross-cutting (est. total gain)
1 Trace-tree connectivity: down-frame sides with PC guard, link-at-heads instead of InnerLoop abort (awfy -10..-15%, apps -5..-8%). 2 Flush policy (richards 0.68, deltablue 0.83). 3 Tier-1 inline __index (DONE in T3). 4 Peeling with pure ops. 5 Recursion families (RETF up-recursion, RETURN in trace). 6 Number widening for mixed int/float sites (raytrace 0.87). 7 Dynamic-key HREF for int/mixed keys + SETUPVAL (C2 code) + __call, landed together (lru_cache). 8 Compile-cost diet (80-100k cyc/recording vs LJ 40k; SipHash HashMaps, lockstep, ra, shape_hash_ir). 9 Allocation sinking/TDUP/inline ctor. 10 Non-stitching helper calls for find/format/next/sub.

## Ceiling [E]
With 1-9: awfy ~1.4-1.5 geomean (1.4 on medians), apps ~1.3. Parity blocked by backend (2.5-5x LJ machine instructions per IR op) and runtime (alloc/GC/table/string = 30-80% of LJ time on json/cd/raytrace/lru_cache), not by value layout. Layout/language costs: int subtype (fixable by widening), 1-based arrays (fast int hash key), non-interned long strings (str_eq_long ~5x stays), 16-byte values ~5-10% on array loops.

## Builder order proposed by the study
1 Policy (Ineffective/Rerec flush; side traces on type-guard exits) S. 2 Header-safe peeling M. 3 [done: T3]. 4 Down-frame + link-at-heads sides L. 5 Recursion families L. 6 Number widening M. 7 HREF int/mixed + SETUPVAL + __call M. 8 Compile diet M. 9 Hygiene (medians of 11+, pin the time gate) S.
