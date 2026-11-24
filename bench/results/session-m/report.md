# Benchmark session-m: subject 8cd1f06, Lua 5.5 and 5.4, plain and PGO

Data here: `fast_all.json`, `rerun_15runs_cpu4.json` (subject), `rerun_7413_15runs_cpu4.json` (7413bff), `tables.md` (all numbers), `js_vs_node.md`, `js/`, `js20/`, logs. `report_m.py` and `js_table.py` regenerate the tables.
Method as in session-l: `scripts/bench-fast.sh --matrix 55plain,55pgo,54plain,54pgo`, one worker on CPU 4, cached PUC/LuaJIT columns, equal-length binary directories, `FORCE_STALE=1`
(LuaJIT was upgraded on 2026-11-23: the cached LuaJIT column is from the older build, so every ours/LuaJIT figure carries that caveat, as in session-l). 145 flagged programs were rerun 15 times serially on the subject
and on 7413bff (session-l subject, binaries kept) on the same day. Four release builds (plain/PGO, 5.5/5.4); all four equal PUC and `.expected` on all 223 programs
(the harness flags load_big and loops_arith on 5.4 against the 5.5 reference: known artefact, same as session-l). All 33 JS ports match node at default size and at 20x.

**Caveat on the "7413bff" comparison:** its column is the minimum over session-l's readings (taken a day earlier) plus today's 15-run reruns, so unflagged programs compare across days. The cached 5a064d7 column is two sessions old. The like-for-like comparison is only the flagged 145.

Work since 7413bff (all light-gated; heavy gate and full mutant runner never run): heatside 48 to 12, N1 loop-body diet, N2 and N2b coroutine resume, TR24c (off), T2b cheaper recording, T6 tier-1 constructor and min/max, T8 pattern find/match on trace, `blfirst`, T4 late native-return guards (ON by default after you flipped it), T9 `innerlink` (off), T5 `t5` (off), T7 `t7` (off).

## Headline

| build | /LuaJIT cycles | /LuaJIT instructions | vs 5a064d7 cyc / ins | vs 7413bff cyc / ins |
|---|---|---|---|---|
| 5.5 plain | 0.924 | 1.290 | 0.986 / 0.961 | 0.987 / 0.965 |
| 5.5 PGO | 0.849 | 1.192 | 0.991 / 0.963 | 0.992 / 0.967 |
| 5.4 plain | 0.984 | 1.385 | 0.987 / 0.965 | 0.985 / 0.969 |
| 5.4 PGO | 0.904 | 1.281 | 0.990 / 0.967 | 0.991 / 0.971 |

(geomeans over 223 programs; ours/LuaJIT over the 219 with a LuaJIT reading; below 1 = faster.) Session-l ours/LuaJIT were 0.939, 0.858, 1.002, 0.915 on the same programs (0.936, 0.856, 0.999, 0.913 with today's readings for 7413bff), so we are 1 to 2 percent closer to LuaJIT.

**Instructions are down 3.5 to 4 percent against 5a064d7 and about 3 percent against 7413bff on every build, but cycles moved only 1 to 1.5 percent.** Most of the instruction savings do not show up as time: IPC dropped, and many programs lose cycles at equal or lower instructions (list below). Against 7413bff, 43 programs are more than 3% faster and 13 more than 3% slower on 5.5 plain.
JS ports: ours/node 0.79 (cycles, 33 programs) at default size, 2.54 at 20x (17 programs); ours/LuaJIT 1.27 and 1.35. Session-l was 0.79 and 2.60.

## What improved (cycles vs 7413bff, 5.5 plain)

lib/co_yield -23.1% (ins -8.1%), coro_generators -22.1%, programs/coroutines -19.3%, upval_acc -26.9%, lib/find_pat -22.6%, lua_lexer -15.9% (-24% vs 5a064d7; PGO -16.8%), apps/raytrace -11.2% (-13.9% vs 5a064d7), expr_compiler -9.5%, inline_leaf -10.3%, lib/m_min -9.9%. cd cycles -0.9% (PGO -3.0%); instructions -6.6% vs 5a064d7.
Per-set geomean cycles vs 7413bff (5.5 plain): apps 0.980, awfy 0.990, corpus 0.979, corpus2/dev 0.976, jit-trace 0.978, lib 0.990, programs 0.990; real and corpus2/heldout are flat (real_dis +0.6%, heldout -0.5%).

## Regressions and misses (vs 7413bff unless noted; >=20 Mcyc)

- **lib/str_eq: +31% cycles at -7.8% instructions on 5.5 and 5.4** (spread 0.7%, so not noise). The largest miss and unexplained: instruction count falls, time rises. Not caused by anything we intended; needs a look (likely a changed helper layout or a string compare path whose branch pattern got worse).
- 5.5: regex_nfa +10.5% cyc / +4.3% ins (7.6% spread), tinsert_idx +6.3% / +4.8% (consistent on 5.4 too: +2.2%/+4.0%), real_dis_ppc +5.9% / +5.4%, error_tbl +5.9% (equal ins), sel_n +4.1%, type_call +3.6%, len_tbl +3.3%, string_ops +3.2% (26.8% spread), it_pairs +3.2%, classic_vector +3.0% (equal ins), parser_combinator +2.2% / +3.1% ins, storage +1.7% / +2.4% ins, vending_fsm +3.3% ins.
- 5.4: lib/fmt_q +10.9% (ins -1.2%), programs/table_hash +10.7% (equal ins, spread 33.8%), gsub_tbl +7.3%, string_ops +4.5%, pcall_error +4.1%, stream_closures +13.5% instructions (cycles -1.4%: the `blfirst` formation landing noted by its builder), real_dis_ppc +3.2% ins, ecs_world +3.2% ins, tinsert_append +3.8% ins.
- Programs where cycles rose at equal instructions are layout/IPC effects (the builders measured the same for regex_nfa, prototype_objects, expr_lang, sumrec). prototype_objects is +4.8% (5.5) and +6.6% (5.4) vs 5a064d7.
- Open since session-l: roguelike_turns 5.5 +3.5% vs 5a064d7 (vs 7413bff -2.1%), real_dis_mips -3.5% to -3.9% cycles now. The formation lottery that hit them has not been removed.

## Where we stand against LuaJIT (5.5 plain, cycles)

Gaps: permute 2.76 (baseline artefact still to re-measure vs LuaJIT median), fmt_s 2.41, raytrace 2.37 (2.57 before), cd 2.34, text_adventure 2.20, byte_multi 2.00, classic_vector 1.99, towers 1.98, stream_closures 1.94, find_plain 1.85, json 1.85, lua_lexer 1.83 (2.14 before), co_yield 1.79, tremove_pop 1.79. Instruction gaps are larger than cycle gaps (geomean 1.29 / 0.92). We beat LuaJIT on binary_trees (0.58), rec_loop (0.57), dijkstra_routes (0.92), deltablue PGO (0.95), real_dis_mips PGO (1.03 vs 1.06).
fib is unchanged at 1.32 (PGO 1.35). Raytrace and lua_lexer improved the most; cd, towers, permute, richards (1.72) did not move in cycles.

## Items expected from the builders vs what the benchmark shows

| item | builder claim | benchmark |
|---|---|---|
| T8 find/match | lua_lexer -16/-23%, find_pat -21% | lua_lexer -15.9% (5.5), find_pat -22.6%: confirmed |
| T6 + blfirst | raytrace -6.6% -4.1% ins | raytrace -11.2% cyc, ins -11.8% (with N1, T2b): confirmed |
| N2b | co_yield -17% cyc | -23% vs 7413bff (incl. N2): confirmed |
| N1 | ins -4 to -5% on sample | suite ins -3.5% to -3.9%, jit-trace set -12%: confirmed |
| T4 | cd -5.9% ins | cd ins -6.6% vs 5a064d7, cycles -0.9% vs 7413bff: instructions confirmed, little time gain at default size |
| heatside / TR24c / T2b | misc | no regressions at richards, roguelike -2% vs 7413bff |
| expected misses | prototype_objects, regex_nfa cycles | prototype_objects and regex_nfa both show up: confirmed |

## Not covered
Gate-heavy, the full mutant runner, tr_oracle, all-223 PGO runs of the individual knobs were never run on this code. LuaJIT column is stale (see above). T5, T7, T9, TR24c stay off (knobs only).
