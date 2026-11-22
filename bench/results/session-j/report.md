# Benchmark session-j: subject b116447, Lua 5.5 and 5.4, plain and PGO

Data in this directory (`tables.md` has all aggregates and per-program tables, `js_vs_node.md`, JSON/CSV, logs, `build_logs/`, `noisecheck.txt`).
Method as in session-i: four fresh `release-build.sh` builds (home-dir/user string check 0 hits on all four), `bench-fast.sh --matrix
55plain,55pgo,54plain,54pgo` with `PREV_COMMIT=22670a1`, one worker on CPU 4, two quick runs per program, min-of-5 for flagged programs, then
15 serial runs for 116 programs (session-i: 83): any build more than 3% worse or more than 7% better than session-i, or a spread above 5% at
20 Mcyc or more. Set A is the fast pass alone, set B merges the reruns. Baselines `p55/g55/p54/g54-22670a1` were imported from session-i's merged
set B; 8c39465, 1a5aad0 and 108b935 columns are kept.

Correctness: all our outputs equal PUC and `.expected` on all 223 programs (the harness flags programs/load_big and programs/loops_arith on 5.4
because it compares against the 5.5 reference, as in session-i). All 30 JS ports match node at default size and `-s 20`.

Work since session-i (22670a1): K7 AREF-PHI fix (dijkstra_routes), test-determinism fixes (no perf change expected), L1 bytext (lua_lexer), L2
insert_small and P1 presize (classic_vector), S1 lazystub (smaller trace code, lower compile cost), rn-core tests in gate-light.

## Canary
The canary passed on every attempt, nothing was re-measured and no `FORCE_STALE` was used. Main pass median 0.82%, worst 2.98%; rerun median 0.89%,
worst 2.02%. PUC 5.4 drifted +4..9% in session-i; now within +0.8, +3.0, +0.2% (main) and -0.2, +1.6, -0.1% (rerun). All PUC, LuaJIT, LuaJIT -joff
and older-subject columns are the cached ones.

## Headline: geomean cycles, below 1 means ours is faster (session-i set A in brackets)

| build | ours/LuaJIT (219) | ours/PUC (223) | ours/LuaJIT instructions |
|---|---|---|---|
| 5.5 plain | 0.960 (0.955) | 0.219 (0.218) | 1.350 (1.360) |
| 5.5 PGO | 0.874 (0.873) | 0.200 (0.200) | 1.245 (1.253) |
| 5.4 plain | 1.019 (1.016) | 0.227 (0.227) | 1.442 (1.447) |
| 5.4 PGO | 0.934 (0.929) | 0.209 (0.208) | 1.331 (1.334) |

Set B against LuaJIT: 0.952 / 0.868 / 1.011 / 0.926 (session-i set B 0.950 / 0.869 / 1.010 / 0.925). Against LuaJIT -joff: 0.322 plain, 0.293 PGO
on 5.5. Net: cycles are flat against session-i (+0.2..1% set A, +0.1..0.2% set B) while instructions are down 0.3..0.7%. The wins are concentrated in a
handful of programs; the flat suite geomean reflects machine state on a subset of programs (see caveats), not a code regression.

## Subject vs session-i / session-h / 1a5aad0 (cycles A / B, instructions, counts >3% better / within / >3% worse)

"22670a1 setA" is like for like (our set A against session-i's set A).

| build | vs | cycles A / B | instr | counts A | counts B |
|---|---|---|---|---|---|
| 5.5 plain | 22670a1 | 1.010 / 1.002 | 0.993 | 6/187/30 | 7/200/16 |
| 5.5 plain | 22670a1 setA | 1.006 / 0.997 | 0.993 | 9/192/22 | 12/199/12 |
| 5.5 plain | 8c39465 | 1.006 / 0.997 | 0.986 | 14/185/24 | 17/192/14 |
| 5.5 plain | 1a5aad0 | 0.997 / 0.988 | 0.969 | 29/158/36 | 38/160/25 |
| 5.5 PGO | 22670a1 | 1.006 / 0.999 | 0.994 | 7/191/25 | 7/205/11 |
| 5.5 PGO | 22670a1 setA | 1.001 / 0.994 | 0.994 | 10/193/20 | 13/202/8 |
| 5.5 PGO | 8c39465 | 1.004 / 0.996 | 0.987 | 10/184/29 | 12/196/15 |
| 5.5 PGO | 1a5aad0 | 0.995 / 0.988 | 0.968 | 23/176/24 | 31/176/16 |
| 5.4 plain | 22670a1 | 1.009 / 1.001 | 0.997 | 5/188/30 | 8/207/8 |
| 5.4 plain | 22670a1 setA | 1.003 / 0.995 | 0.997 | 10/193/20 | 13/205/5 |
| 5.4 plain | 8c39465 | 1.000 / 0.992 | 0.988 | 14/176/33 | 18/189/16 |
| 5.4 plain | 1a5aad0 | 0.999 / 0.992 | 0.975 | 28/150/45 | 31/166/26 |
| 5.4 PGO | 22670a1 | 1.009 / 1.001 | 0.998 | 3/202/18 | 5/212/6 |
| 5.4 PGO | 22670a1 setA | 1.005 / 0.996 | 0.998 | 6/203/14 | 6/213/4 |
| 5.4 PGO | 8c39465 | 1.005 / 0.996 | 0.989 | 11/186/26 | 13/197/11 |
| 5.4 PGO | 1a5aad0 | 1.000 / 0.991 | 0.974 | 23/174/26 | 27/179/17 |

Programs with more than 3% fewer instructions than session-i: 11 (5.5 plain), 8 (5.5 PGO), 8 (5.4 plain), 5 (5.4 PGO); none or one has more.
By set vs session-i (set B, 5.5 plain, cycles / instructions): apps 1.000/0.975, awfy 0.999/0.988, corpus 0.973/0.968, corpus2 heldout 0.991/0.955,
corpus2 dev 1.028/0.994 (csv_report), jit-trace 1.001/1.000, lib 1.002/1.000, programs 1.011/1.000, real 1.016/0.996.

## Effects of the new work (cycles / instructions vs session-i; 5.5 plain, PGO, 5.4 plain, 5.4 PGO)

- **K7 AREF-PHI, dijkstra_routes:** 5.5 plain 0.823 / 0.641, PGO 0.825 / 0.639 (-17.7% cycles, -36% instructions; expected -19% / -36%). 5.4 is
  smaller: 0.958 / 0.935 and 0.957 / 0.934. Against session-h the program is now 0.94 on both versions: session-i's +14% regression is repaid.
- **L1 bytext, lua_lexer (5.5 only):** 0.879 / 0.861 plain, 0.881 / 0.849 PGO (expected -12%); 5.4 unchanged. sha2_base64 also gains 0.892 / 0.936.
- **L2 insert_small + P1 presize, classic_vector:** 5.5 plain 0.746 / 0.734 (-25.4%; expected about -25%), PGO 0.773 / 0.810; 5.4 plain 0.803 / 0.782
  (-19.7%), PGO 0.826 / 0.828. Against session-h 0.763 (5.5) and 0.794 (5.4). classic_vector/LuaJIT went from 2.67 to 1.99 plain and 1.84 PGO. This is
  the largest gain of the session.
- **S1 lazystub:** instructions fall everywhere it applies, cycles follow only partly. cd 1.002 / 0.957 plain (instructions -4.3%, cycles flat),
  expr_lang 0.970 / 0.966, raytrace 0.972 / 0.953, lru_cache 1.008 / 0.958 plain, 0.992 / 0.969 PGO; 5.4: expr_lang 0.989, raytrace 0.965, lru_cache
  0.991. The expected cd -2% and lru_cache -2% are not visible in cycles: the builder's single-session measurements and the noise on these programs
  (lru_cache spread 20%) are as large as the effect.
- **K3 fretry shift, 5.4 astar_jumper:** no visible effect (cycles 0.989 / 0.995, instructions 0.998); session-i's +14% over session-h is unchanged.
  5.5 PGO instructions are 4851 M vs 4667 M cached (+4%) at equal cycles; this program's instruction count is unstable between runs.
- **Test-determinism fixes:** no change; the jit-trace set is 1.001 in cycles and 1.000 in instructions on all four builds.

### Regression check for lazystub and presize
- Instructions: no program executes more than 0.5% above session-i except small programs: lib/tpack +1.2% plain / +2.3% PGO / +0.7% 5.4,
  lib/concat_op_str PGO +1.7%, tinsert_append and tinsert_idx on 5.4 +1.1 / +1.2%, programs/varargs +0.6 / +1.0%. Their cycles move +0.5..5%
  (concat_op_str PGO +5.4%, tpack PGO +3.3%). These are the only candidates worth a look; cause unknown.
- jit-trace set (exit storms): geomean 1.001 / 1.000 (5.5 plain), 1.002 / 0.999 PGO, 1.002 / 1.000 5.4. Worst jit-trace/int_rec2 +1.9% plain, +2.4% PGO,
  +2.8% 5.4 at equal instructions (layout, not exits).
- Short programs (under 40 Mcyc, 10 programs): geomean 1.002 / 0.995 / 0.996 / 0.991; only lib/fmt_s (7 Mcyc) is above +3% on plain, noise at that size.

## Biggest wins vs session-i (5.5 plain cycles, set B)
classic_vector 0.746, dijkstra_routes 0.823, lua_lexer 0.879, sha2_base64 0.892; then 0.94-0.98: tconcat, find_pat, expr_lang 0.970, raytrace 0.972,
roguelike_turns, scheme_interp, towers, and many lib programs with equal instructions (layout noise). 5.4: classic_vector 0.803, m_min 0.940 and
diff_levenshtein 0.947 (both equal instructions, so layout), dijkstra_routes 0.958, raytrace 0.965.

## Regressions vs session-i (causes are hypotheses)
Every listed regression has instruction counts within 0.6% of session-i; there is no instruction regression above 2.3%.
- **corpus2/heldout/prototype_objects +16.4% plain, +14.5% PGO, +16.8% / +19.2% on 5.4; corpus2/dev/csv_report +12.1% / +11.5% / +10.1% / +8.5%.**
  Environmental, not code. Readings are bimodal (prototype_objects plain min 1694, median 2144, max 2694 Mcyc against session-i's 1455 / 1515 / 1554).
  Re-measured right after the pass (`noisecheck.txt`, 12 interleaved runs, CPU 4): PUC 5.5 on prototype_objects min 3013 / median 3767 against cached
  2533 / 2671 (+19% min), LuaJIT min 1610 / median 2137 against cached 1281 / 1440 (+26%); csv_report PUC 2374 vs 2165 (+10%), LuaJIT 1340 vs 1263
  (+6%); our own build min 1542 (prototype_objects) and 1199 (csv_report), 9% and 4% above session-i's cached min. Likely cause: kernel/memory
  state hitting allocation-heavy programs with large live heaps.
- 5.5 only, instructions equal: regex_nfa +9.1% plain / +4.9% PGO, real/lua_interp +8.0% / +6.8%, lib/co_yield +7.7%, tonumber_bad +6.6%,
  programs/coroutines +5.6%, oop_mixins +5.5% / +4.2%, events +5.1% (spread 39%), tunpack +5.0%.
- iter_pipeline +4.1% / +6.3% / +3.8% / +3.5% on the four builds (the only one seen on all four, flat in instructions). table_hash +3.7% / +5.3% /
  +7.0% / +5.3% with a 63-73% spread (bimodal, instruction-neutral, the same memory-state effect).
- 5.4 only, instructions equal: m_tointeger +5.8%, load_big +5.6%, find_pat +5.6%, lua_parser +3.8%.

## Focus programs (cycle ratios; ours/node from the JS ports)

| program | plain/LJ | PGO/LJ | PGO/PUC | vs session-i plain (cyc/ins) | ours/node default | ours/node -s 20 |
|---|---|---|---|---|---|---|
| nbody | 1.006 | 0.996 | 0.081 | 1.008/0.999 | 1.24 | - |
| fib | 2.680 | 2.683 | 0.304 | 1.001/1.000 | 1.76 | - |
| binary_trees | 0.772 | 0.657 | 0.300 | 1.004/1.000 | 1.85 | - |
| queens | 0.970 | 0.972 | 0.424 | 1.007/1.000 | 1.57 | - |
| apps/raytrace | 2.757 | 2.587 | 0.399 | 0.972/0.953 | 0.74 | 3.72 |
| cd | 2.445 | 2.265 | 0.557 | 1.002/0.957 | 1.39 | 5.34 |
| list | 1.090 | 1.081 | 0.229 | 1.003/0.995 | 1.01 | 3.15 |
| richards | 1.721 | 1.658 | 0.291 | 1.008/0.981 | 0.51 | 3.05 |
| deltablue | 1.060 | 0.982 | 0.643 | 0.989/0.963 | 0.77 | 3.64 |
| havlak | 1.311 | 1.174 | 0.622 | 0.988/0.997 | 0.50 | 3.23 |
| awfy/permute | 3.350 | 3.311 | 0.176 | 1.004/0.999 | 0.50 | 2.52 |
| lua_lexer | 2.427 | 2.155 | 0.294 | 0.879/0.861 | no port | no port |
| classic_vector | 1.994 | 1.841 | 0.150 | 0.746/0.734 | no port | no port |
| dijkstra_routes | 0.942 | 0.923 | 0.372 | 0.823/0.641 | no port | no port |

Still weak against LuaJIT (5.5 plain): permute 3.35, raytrace 2.76, fib 2.68, cd 2.45, lua_lexer 2.43, co_yield 2.38, text_adventure 2.14,
towers 2.08, classic_vector 1.99. Biggest wins over LuaJIT unchanged: sel_loop 0.04, error_tbl 0.09, pcall_err, pcall_error.

## JS ports vs node (plain unstripped release build; 30 rows in `js_vs_node.md`)
Default size: ours/node 0.74 cycles (0.70 wall), ours/LuaJIT 1.30 over 25 programs (session-i 0.75 / 0.71 / 1.32). Worst: binary_trees 1.85,
fib 1.76, queens 1.57, heapsort 1.42, cd 1.39. 20x (14 programs): ours/node 2.61 cycles (2.29 wall), ours/LuaJIT 1.39 (session-i 2.60 / 2.28 /
1.41). Worst: cd 5.34, storage 4.61, raytrace 3.72, deltablue 3.64, havlak 3.23, list 3.15. Against session-i at 20x: unchanged within 1-3% except
raytrace 3.83 to 3.72 and deltablue 3.88 to 3.64.

## Caveats
- **Machine state differs from session-i on a subset of programs.** The 3-program canary was clean, but PUC and LuaJIT re-measured on
  prototype_objects and csv_report read 6-26% above their cached values. The effect is instruction-neutral and bimodal. The cached LuaJIT/PUC
  columns are therefore too fast for those two programs, and their ratios are inflated (about +9% and +4% in cycles on our own min); they are left in
  the geomeans as measured.
- Noisier pass than session-i: 135 of 223 programs were flagged noisy in the quick runs (many lib programs of 30-250 Mcyc), hence 116 reruns instead
  of 83. Set B flatters this session slightly more than session-i's; compare set A with set A ("22670a1 setA" rows). On 5.5 plain both show about
  +0.5% cycles with -0.7% instructions; lib (95 programs, equal instructions) is +0.7% on A and +0.2% on B, programs +2.7% on A and +1.1% on B
  (layout or machine state).
- Idle check: Firefox ran on the desktop throughout. The harness idle check (5% per CPU) failed at least twice on CPU 4 or its sibling before
  passing; the wrapper retried and the passes ran at 2.6% / 0.5% measured idle.
- Large 15-run spreads: prototype_objects 59%, table_hash 73%, tinsert_idx 76%, events 39%, csv_report 32%, towers 16-47%.
- Not in this matrix: JIT-off and startup columns. LuaJIT has 219 comparable programs (testrunner_luaunit, real_dis_arm, coroutines and load_big
  produce different output; m_tointeger, m_type, m_ult and utf8_len are 5.3+ only); five of the 30 JS ports have no LuaJIT variant.
