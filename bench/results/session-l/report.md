# Benchmark session-l: subject 7413bff, Lua 5.5 and 5.4, plain and PGO

Data in this directory (`fast_all.json`, `b5a_fast_all.json`, `rerun_15runs_cpu4.json` (= `rerun_all_15runs_cpu4.json`), `rerun_5a0_15runs_cpu4.json`,
`tables.md`, `per_program.csv`, `js_vs_node.md`, `js/`, `js20/`, logs, `build_logs/`; `report_l.py`, `js_table.py` regenerate the tables).
Method as in session-k: `scripts/bench-fast.sh --matrix 55plain,55pgo,54plain,54pgo`, one worker on CPU 4, cached PUC/LuaJIT columns, binaries in
equal-length directories. The previous subject 5a064d7 was rebuilt in this session (four release builds) and imported as the `*-5a064d7` baselines
(fast pass plus the 15-run reruns of the same programs), so the comparison is same-session. 119 programs were flagged and rerun 15 times serially
on BOTH sides (subject 3241 s, 5a064d7 3410 s), so "set B" is like for like. All four builds equal PUC and `.expected` on all 223 programs
(the harness flags load_big and loops_arith on 5.4 because it compares with the 5.5 reference: same known artefact as session-k). All 33 JS ports
match node at default size and at `-s 20`.

Work since 5a064d7 (all light-gated only; the heavy gate was never run on it): `deepprobe`, T1 `retfsel`, T3 `heatside`, T2 compile-cost fixes
(docs/JIT_TRACE.md sections 38-39).

## Headline

Cycles are flat against the same-session 5a064d7 (+0.1% plain, +0.4% on 5.4 plain), instructions are 0.4% lower. Against the session-k published
numbers cycles are 0.7% lower, against b116447 1.0% lower. The gains are concentrated in a few programs (oop_mixins, lua_lexer, rec_loop,
raytrace, regex_nfa); no big regressions, but several programs miss the +1% instruction / +2% cycle bar (list below).

Geomean ours/LuaJIT cycles (219 programs; set B in brackets set A): 5.5 plain 0.939 (0.943), 5.5 PGO 0.858 (0.861), 5.4 plain 1.002 (1.007), 5.4 PGO
0.915 (0.920). Ours/PUC cycles (223): 0.214 / 0.196 / 0.224 / 0.205. Against LuaJIT -joff (5.5): 0.317 plain, 0.290 PGO.

Note the same-session baseline: the rebuilt 5a064d7 measures 0.938 / 0.857 / 0.998 / 0.914 against LuaJIT, i.e. 0.7% better than the session-k
published 0.945 / 0.864 / 1.007 / 0.921 (machine/kernel drift or PGO non-reproducibility), which is why the subject looks 0.7% better against the
published numbers and flat against the rebuild. Use the same-session columns.

## Subject vs baselines (set B; cycles / instructions; geomean of 223)

| build | vs 5a064d7 (same session) | >3% better / within / >3% worse | vs 5a064d7 set A | vs session-k published | vs b116447 |
|---|---|---|---|---|---|
| 5.5 plain | 1.001 / 0.996 | 10 / 200 / 13 | 1.000 / 0.996 | 0.993 / 0.996 | 0.990 / 0.991 |
| 5.5 PGO | 1.001 / 0.996 | 10 / 201 / 12 | 1.000 / 0.996 | 0.993 / 0.996 | 0.990 / 0.992 |
| 5.4 plain | 1.004 / 0.996 | 8 / 202 / 13 | 1.003 / 0.996 | 0.995 / 0.996 | 0.995 / 0.992 |
| 5.4 PGO | 1.001 / 0.996 | 10 / 207 / 6 | 0.999 / 0.996 | 0.993 / 0.997 | 0.991 / 0.992 |

By set vs 5a064d7 (5.5 plain, cycles / instructions): apps 0.991/0.987, awfy 1.006/0.995, corpus 0.999/0.992, corpus2/dev 0.991/0.976,
corpus2/heldout 1.017/0.994, jit-trace 0.998/0.994, lib 1.001/1.000, programs 1.005/0.998, real 0.998/0.999, real_dis 1.014/1.001.

## Expected items, checked (5a064d7 same session; cycles / instructions; 5.5 plain, 5.5 PGO, 5.4 plain, 5.4 PGO)

| program | result | expected | verdict |
|---|---|---|---|
| lua_lexer (T1) | 0.910/0.894, 0.915/0.892, 1.003/1.000, 0.991/1.001 | about -7% cycles | met on 5.5 (-9%); **no effect on the 5.4 builds** (not investigated) |
| oop_mixins (T3) | 0.805/0.710, 0.791/0.707, 0.793/0.722, 0.810/0.722 | about -22% cycles | -19% to -21%, instructions -28% to -29% |
| raytrace (T2) | 0.970/0.956, 0.970/0.966, 0.958/0.960, 0.970/0.969 | about -4% | -3.0% to -4.2%, instructions also lower (-3% to -4%): this is the combined effect of T1/T2/T3/deepprobe, not T2 alone, which was built as identical code |
| rec_loop (deepprobe) | 0.897/0.856, 0.893/0.855, 0.934/0.857, 0.931/0.857 | about +1.5% instr vs b116447 | 5a064d7 was +17.9% instr vs b116447, so +0.9% instr vs b116447; cycles vs b116447 +1.4% plain / +1.1% PGO / +4.3% 5.4 (spread of readings 14-62%: noisy program) |
| richards (T3, known) | 1.033/0.995, 1.035/0.993, 1.030/0.996, 1.029/0.993 | +4.7% (known) | **+3.0% to +3.5% cycles, above the +2% bar**; instructions slightly lower, so layout/IPC; user decision was to keep T3 on |
| roguelike_turns | 1.057/1.017, 1.048/1.018, 1.024/1.009, 1.004/1.009 | about +3% | **worse than expected on 5.5: +5.7% / +4.8% cycles, instructions +1.7% / +1.8%, both over the bar** |
| real_dis_mips, 5.4 | plain 1.045/1.022, PGO 1.033/1.021 (5.5: 1.004/1.001, 0.984/0.999) | +2% | **+4.5% / +3.3% cycles and +2.2% / +2.1% instructions on 5.4**, both over the bar |
| fib | 1.000/1.000 (vs b116447 0.507/0.505 plain) | keep session-k gain | kept |
| binary_trees | vs 5a064d7: 1.040/1.000, 1.031/1.000; vs b116447 0.824 plain / 0.815 PGO | keep session-k gain | gain kept against b116447, but **+4.0% / +3.1% cycles against the rebuilt 5a064d7 with equal instructions** (spread 8% / 20%). The rebuilt 5a064d7 is itself 3% faster than its published figure (PGO/LJ 0.513 vs 0.528) |
| permute | 1.007/0.999 (vs b116447 0.831) | keep | kept |

## Misses against the bar (+1% instructions / +2% cycles, programs of 20 Mcyc or more, vs same-session 5a064d7)

Instructions over +1%: roguelike_turns (+1.7% / +1.8% on 5.5), real_dis_mips 5.4 (+2.2% / +2.1%). Everything else is cycles only, with equal
instructions, so layout / IPC: 5.5 plain prototype_objects +13.3% (spread of readings 90%, noise-dominated), gc_churn +6.0%, gmatch_words +5.3%,
concat_op_str +4.1%, tfor_loops +4.0%, binary_trees +4.0%, havlak +3.4%, event_promise +3.3%, richards +3.3%, m_min +3.2%, lua_parser +3.1%,
iter_pipeline +3.0%. PGO: prototype_objects +15.2%, it_pairs_hash +4.9%, gc_churn +4.9%, sel_n +4.7%, event_promise +4.3%. 5.4 plain:
prototype_objects +17.5%, str_method_len +10.8%, len_op +9.0% (spread 68%), it_pairs_hash +7.1%, wordfreq_wrap +6.4%, m_tointeger +6.1%,
gc_churn +5.7%. 5.4 PGO: prototype_objects +15.3%, load_small +6.6%, gc_churn +4.5%. Full lists with spreads: `tables.md`. The counts of
programs more than 3% worse (13 / 12 / 13 / 6) exceed the counts more than 3% better (10 / 10 / 8 / 10) on three of four builds; the geomean is
flat because the improvements are larger.

## Focus programs (set B; plain/LJ, PGO/LJ, ours cycles vs 5a064d7 same session / instructions, ours/node)

| program | plain/LJ | PGO/LJ | vs 5a064d7 plain | vs 5a064d7 PGO | ours/node default | 20x |
|---|---|---|---|---|---|---|
| nbody | 0.989 | 0.986 | 0.996/1.000 | 0.986/1.000 | 1.25 | - |
| fib | 1.344 | 1.337 | 1.000/1.000 | 0.995/1.000 | 0.90 | - |
| binary_trees | 0.630 | 0.529 | 1.040/1.000 | 1.031/1.000 | 1.62 | - |
| queens | 0.969 | 0.967 | 0.993/1.000 | 0.999/1.000 | 1.58 | - |
| raytrace | 2.668 | 2.492 | 0.970/0.956 | 0.970/0.966 | 0.70 | 3.91 |
| cd | 2.365 | 2.238 | 0.996/0.986 | 1.008/0.994 | 1.44 | 5.38 |
| list | 1.058 | 1.045 | 1.003/0.999 | 1.011/1.000 | 1.05 | 3.16 |
| richards | 1.773 | 1.707 | 1.033/0.995 | 1.035/0.993 | 0.55 | 2.97 |
| deltablue | 1.032 | 0.957 | 0.998/0.982 | 0.993/0.989 | 0.79 | 3.81 |
| havlak | 1.322 | 1.139 | 1.034/0.993 | 1.010/0.993 | 0.54 | 2.94 |
| permute | 2.780 | 2.745 | 1.007/0.999 | 1.007/0.999 | 0.45 | 2.03 |
| towers | 1.986 | 1.935 | 0.997/0.990 | 0.998/0.994 | 0.55 | 2.26 |
| lua_lexer | 2.192 | 1.956 | 0.910/0.894 | 0.915/0.892 | 1.09 | 1.33 |
| classic_vector | 1.930 | 1.794 | 0.986/0.993 | 0.998/0.995 | 2.63 | 8.92 |
| dijkstra_routes | 0.929 | 0.905 | 1.002/0.984 | 1.005/0.984 | 1.37 | 1.56 |

Still weak against LuaJIT (5.5 plain, ours/LJ): permute 2.78, raytrace 2.67, cd 2.37, lua_lexer 2.19, towers 1.99, classic_vector 1.93, richards
1.77 (session-k: 2.80 / 2.79 / 2.43 / 2.42 / 2.03 / 1.98 / 1.75). Only lua_lexer moved much. The `lib/` and `corpus2` programs with the biggest
gaps (fmt_s 2.33, co_yield 2.33, text_adventure 2.17, coroutines 2.05, byte_multi 2.04) are listed in `tables.md`.

## JS ports vs node (plain unstripped release build, JIT on, 33 ports; `js_vs_node.md`)

Default size: geomean ours/node 0.79 cycles (0.77 wall) over 33 programs, ours/LuaJIT 1.22 over 28. 20x (17 programs): ours/node 2.60 cycles (2.35
wall), ours/LuaJIT 1.39. Session-k: 0.77 (0.73 wall) / 1.31 default, 2.55 (2.27 wall) / 1.43 at 20x, so ours/node is 2-3% worse in both runs
(single run of 5 per program, no rerun; classic_vector 2.64 -> 2.63 default, binary_trees 1.51 -> 1.62, queens 1.57 -> 1.58, fib 0.90 -> 0.90,
permute 0.42 -> 0.45). Worst at 20x: classic_vector 8.92, cd 5.38, storage 4.63, raytrace 3.91, deltablue 3.81.

## Caveats

- **LuaJIT was upgraded during the session** (2.1.1794386141 in session-k, 2.1.1795119671 from 2026-11-23 08:40; arch package update). The cached
  LuaJIT columns in the fast-pass and rerun tables are the old ones (the fast pass ran before the upgrade, so it was consistent); the two 15-run
  reruns ran with `FORCE_STALE=1` because the staleness check refused, which only affects the LuaJIT baseline, not the subject-vs-5a064d7
  comparison. The JS run used the NEW LuaJIT, so the ours/LuaJIT figures in the JS section are not strictly comparable with session-k (the node
  figures are). Canary drift of the LuaJIT column during the reruns was -0.7% / -0.4% / +2.1% (class_middleclass, nbody, richards), so the new
  build gives about the same cycles on those three.
- The first rerun attempt had been stopped at program 70 of 119 (the run was not resumable); that partial file is kept as
  `rerun_15runs_cpu4.stopped-at-70.part` and not used. The machine had also been rebooted since the fast pass.
- Canary: fast pass median |drift| 0.50% / worst 2.32% (ljoff class_middleclass); 5a064d7 pass 0.43% / 1.42%; subject rerun 0.75% / 2.83% (PUC
  nbody); 5a064d7 rerun 0.54% / 2.00%. Cached PUC/LuaJIT ratios may be inflated by up to ~3% on nbody-sized programs in the later passes.
- Noise: the fast pass flagged 136 programs noisy (30 for instruction moves), 79 stayed quick-only. Large spreads remain in prototype_objects (45-100%),
  gc_churn (8-26%), parser_combinator (40%), csv_report (44%), len_op (68%), rec_loop (14-62%). Set A and set B differ by 0.1%. Several of the
  "regressions" above sit in these programs and should not be read as real.
- PGO builds are not bit-reproducible, and the rebuilt 5a064d7 differs from the one measured in session-k (0.7% faster against LuaJIT).
- The heavy gate, oracle-feature suites, jit-diff 1-200 in seven modes and the full 806-mutant runner were NOT run on deepprobe / T1 / T2 / T3
  (user decision). Not investigated here: lua_lexer's missing gain on 5.4, roguelike_turns (+5.7% / instructions +1.7%) and 5.4 real_dis_mips
  (+4.5% / instructions +2.2%) being above the expectation, binary_trees' cycles against the rebuilt baseline.
