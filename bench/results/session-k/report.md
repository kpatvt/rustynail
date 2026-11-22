# Benchmark session-k: subject 5a064d7, Lua 5.5 and 5.4, plain and PGO

Data in this directory (`fast_all.json`, `b116_fast_all.json`, `rerun_*_15runs_cpu4.json`, `tables.md`, `vs_baseline.md`, `per_program.csv`,
`js_vs_node.md`, `js/`, `js20/`, logs, `build_logs/`; `report_k.py`, `extra.py`, `focus_extra.py`, `js_table.py` regenerate the tables).
Method as in session-j: `scripts/bench-fast.sh --matrix 55plain,55pgo,54plain,54pgo`, one worker on CPU 4, cached PUC/LuaJIT columns.
The previous subject b116447 was rebuilt in this session (four release builds each, separate worktree; its plain 5.5 binary is byte-identical
to session-j's) and imported as the `*-b116447` baselines, so the comparison is same-session. 15 serial runs for 91 programs on BOTH sides, so
set B is like for like. Binaries sit in equal-length directories (instruction counts depend on argv[0] length). All four builds equal PUC and
`.expected` on all 223 programs (the harness flags load_big and loops_arith on 5.4 because it compares with the 5.5 reference; checked by
hand: p54, g54 and PUC 5.4 print identical output). All 33 JS ports match node at default size and `-s 20`.

Work since b116447: E1 adaptive recursion unrolling, E2 region-scoped callee identity, E4 lazy frames for recursive functions with loops,
E5 method identity, E6 direct exit-to-side links, a lazy-chain defect fix, E9 `t1call` (knob, default off, so not in this measurement).

## Headline: geomean cycles (session-j published set A in brackets)

| build | ours/LuaJIT (219) | ours/PUC (223) | ours/LuaJIT instructions |
|---|---|---|---|
| 5.5 plain | 0.948 (0.960) | 0.216 (0.219) | 1.344 (1.350) |
| 5.5 PGO | 0.866 (0.874) | 0.198 (0.200) | 1.240 (1.245) |
| 5.4 plain | 1.010 (1.019) | 0.225 (0.227) | 1.436 (1.442) |
| 5.4 PGO | 0.924 (0.934) | 0.207 (0.209) | 1.325 (1.331) |

Set B ours/LuaJIT: 0.945 / 0.864 / 1.007 / 0.921. b116447 re-measured now (set B): 0.948 / 0.866 / 1.007 / 0.923 (session-j published
0.952 / 0.868 / 1.011 / 0.926). Against LuaJIT -joff (5.5): 0.320 plain, 0.292 PGO. Use the same-session columns: b116447 sits 0.4% below its
published numbers, which explains part of the 0.5-0.7% gain seen against session-j.

## Subject vs baselines (set B; cycles / instructions)

| build | vs b116447 | >3% better / within / >3% worse | vs b116447 set A | vs session-j published | vs 22670a1 |
|---|---|---|---|---|---|
| 5.5 plain | 0.997 / 0.996 | 6 / 213 / 4 | 0.995 / 0.996 | 0.993 / 0.995 | 0.995 / 0.988 |
| 5.5 PGO | 0.997 / 0.996 | 10 / 209 / 4 | 0.995 / 0.996 | 0.995 / 0.996 | 0.994 / 0.990 |
| 5.4 plain | 0.999 / 0.996 | 6 / 213 / 4 | 0.997 / 0.996 | 0.995 / 0.996 | 0.997 / 0.993 |
| 5.4 PGO | 0.998 / 0.996 | 6 / 212 / 5 | 0.996 / 0.996 | 0.996 / 0.996 | 0.996 / 0.993 |

By set vs b116447 (5.5 plain, cycles / instructions): programs 0.969/0.963, awfy 0.993/0.984, jit-trace 1.003/1.006, everything else within
1.011. The suite geomean is flat: the gains sit in a few recursion programs.

## Expected gains, verified (vs b116447; cycles / instructions; 5.5 plain, 5.5 PGO, 5.4 plain, 5.4 PGO)

| program | result |
|---|---|
| fib | 0.510/0.505, 0.509/0.505, 0.509/0.505, 0.508/0.505: about -49% (expected -45%) |
| binary_trees | 0.811/0.762, 0.813/0.738, 0.835/0.789, 0.816/0.769 |
| awfy/permute | 0.836/0.830, 0.836/0.829, 0.867/0.835, 0.866/0.834: about -16% on 5.5 |
| awfy/towers | 0.975, 0.968, 0.988, 0.987 (cycles) |
| list | flat in cycles (1.001 plain), instructions -2.4% |
| cd / raytrace / richards | 1.023 / 1.015 / 1.010 plain; 1.011 / 1.004 / 1.006 PGO (near flat as predicted) |
| jit-trace recursion | array_rec 1.000, int_rec2 1.005, int_sum 1.001; rec_loop regressed (below); there is no program named sumrec in the suite |

## Focus programs (plain/LJ, PGO/LJ, PGO/PUC, vs b116447 plain cycles/instructions, ours/node default and 20x)

| program | plain/LJ | PGO/LJ | PGO/PUC | vs b116447 | node default | node 20x |
|---|---|---|---|---|---|---|
| programs/nbody | 0.991 | 0.994 | 0.081 | 1.004/1.000 | 1.27 | - |
| programs/fib | 1.351 | 1.349 | 0.153 | 0.510/0.505 | 0.90 | - |
| programs/binary_trees | 0.620 | 0.528 | 0.241 | 0.811/0.762 | 1.51 | - |
| programs/queens | 0.970 | 0.971 | 0.424 | 1.003/1.000 | 1.57 | - |
| apps/raytrace | 2.785 | 2.584 | 0.399 | 1.015/1.001 | 0.78 | 3.78 |
| awfy/cd | 2.427 | 2.255 | 0.555 | 1.023/1.005 | 1.32 | 5.21 |
| awfy/list | 1.067 | 1.052 | 0.223 | 1.001/0.976 | 1.06 | 3.16 |
| awfy/richards | 1.747 | 1.685 | 0.296 | 1.010/1.002 | 0.54 | 3.06 |
| awfy/deltablue | 1.053 | 0.973 | 0.637 | 1.015/1.002 | 0.76 | 3.57 |
| awfy/havlak | 1.325 | 1.171 | 0.620 | 1.015/1.001 | 0.59 | 2.93 |
| awfy/permute | 2.797 | 2.766 | 0.147 | 0.836/0.830 | 0.42 | 1.99 |
| awfy/towers | 2.030 | 1.965 | 0.214 | 0.975/0.978 | 0.53 | 2.19 |
| corpus/lua_lexer | 2.421 | 2.118 | 0.289 | 1.005/1.000 | 0.97 | 1.22 |
| corpus/classic_vector | 1.981 | 1.839 | 0.150 | 0.997/1.001 | 2.64 | 8.61 |
| corpus2/heldout/dijkstra_routes | 0.937 | 0.909 | 0.366 | 1.005/1.000 | 1.33 | 1.64 |

fib/LuaJIT plain went from 2.680 (session-j) to 1.351; binary_trees/LuaJIT from 0.772 to 0.620. fib, binary_trees, queens and nbody have
no 20x workload. Still weak against LuaJIT: awfy/permute (2.8), apps/raytrace (2.8), awfy/cd (2.4), lua_lexer (2.4), towers (2.0),
classic_vector (2.0), richards (1.7).

## Regressions (cycles worse than 3%, programs of 20 Mcyc or more)

Real, one program: **jit-trace/rec_loop** (93 Mcyc): cycles +14.6% / +13.9% / +13.1% / +13.3% and instructions +17.9% / +17.9% / +17.7% /
+17.6% (5.5 plain, 5.5 PGO, 5.4 plain, 5.4 PGO). Not noise: instruction counts are deterministic and moved the same on all four builds, cycles
are the min of 20 readings per side. Hypothesis (not investigated, no source changes were allowed): E1/E4/E6 change the unroll and exit shape
of the "fib/ack called from a hot loop" family and the new shape executes more instructions per iteration. It is the only jit-trace program
that moved (jit-trace geomean 1.003-1.004 cycles, 1.006 instructions, all from rec_loop).

Instruction-equal movers above +3% after the 15-run reruns of both sides (layout / IPC, no instruction support): 5.5 plain load_small +4.4%,
find_pat +4.3%, load_big +3.1%; 5.5 PGO sha2_base64 +3.8%, parser_combinator +3.8%, mustache_lustache +3.1%; 5.4 plain regex_nfa +3.7%,
containers_lru_heap +3.3%, prototype_objects +3.1%; 5.4 PGO coroutines +5.8%, metamethods +3.6%, alloc_ctor +3.5% (instructions +0.9%),
co_yield +3.3%. Instructions above +1%: penlight_mix PGO +2.7% (cycles +1.9%), apps/markdown PGO +1.7%, markdown_conv PGO +1.3%,
csv_ini_yaml PGO +1.2%, find_cap PGO +1.4%, 5.4 load_small +1.5%/+1.7% (hypothesis for the PGO ones: PGO builds are not bit-reproducible).
Programs under 40 Mcyc (11): geomean 0.984 / 0.986 / 0.991 / 0.988, no regression.

## JS ports vs node (plain unstripped release build, JIT on, 33 ports; `js_vs_node.md`)

Default size: geomean ours/node 0.77 cycles (0.73 wall), ours/LuaJIT 1.31 over 28 programs. On the 30 programs common with session-j: ours/node
0.720 (session-j 0.738), ours/LuaJIT over 25 is 1.277 (1.299). Biggest movers vs node: fib 1.76 -> 0.90, binary_trees 1.85 -> 1.51, permute
0.50 -> 0.42. Moved the other way: table_array 0.66 -> 0.84, havlak 0.50 -> 0.59 (Lua side flat, probably node-side variance, not investigated).
20x (17 programs): ours/node 2.55 cycles (2.27 wall), ours/LuaJIT 1.43; on the 14 common with session-j 2.545 (2.609) and 1.388 (1.387).
Worst at 20x: classic_vector 8.61, cd 5.21, storage 4.65, raytrace 3.78, deltablue 3.57.

## Caveats

- Canary passed on every pass, no FORCE_STALE: b116447 pass median 0.54% / worst 1.99%; subject pass 1.17% / 3.41% (PUC 5.4 nbody); subject
  rerun 1.09% / 3.62%; b116447 rerun 1.87% / 3.23%. Cached PUC/LuaJIT ratios on nbody-sized programs may be inflated by up to ~3.5% in the later
  passes.
- Noise: 100 programs flagged noisy in the subject quick pass (31 for instruction moves), 107 stayed quick-only. Large spreads remain in
  co_yield (36%), binary_trees (15%), csv_ini_yaml (15%), prototype_objects (11%). Set A and set B differ by about 0.2%.
- PGO builds are not bit-reproducible (the b116447 PGO binary differs from session-j's).
- The b116447 cache entries hold the fast pass plus the 15-run reruns of the same 91 programs. JIT-off and nojit columns are not in this matrix.
  LuaJIT has 219 comparable programs; 5 of the 33 JS ports have no LuaJIT variant.
- The heavy gate was NOT run on any of E1-E9 (user decision); only gate-light per commit plus targeted shards (docs/JIT_TRACE.md sections 34-37).
