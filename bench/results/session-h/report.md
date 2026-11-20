# Benchmark session-h: subject 8c39465, Lua 5.5 and 5.4, plain and PGO

Data in this directory (`fast_all.json`, `rerun_15runs_cpu4.json`, `tables.md`, `vs_baseline.md`, `js_vs_node.md`, `per_program.csv`,
logs, `build_logs/`; `report_h.py` and `js_table.py` regenerate the tables). Method: `scripts/bench-fast.sh --matrix
55plain,55pgo,54plain,54pgo` with cached baselines (`bench/results/baselines/`), two quick runs per program, min-of-5 for the 122
programs whose instructions moved more than 0.5% or whose spread exceeded 2%, then 15 serial runs for 101 programs that regressed,
improved by more than 7% or were noisy. Set A is the fast pass alone (comparable to session-nbody pass B); set B merges the 15-run
reruns. All our outputs equal PUC and `.expected` on all 223 programs (two 5.4 programs, load_big and loops_arith, checked by hand
against PUC 5.4). All 30 JS ports match node.

Work since the previous subject 1a5aad0: S4 shape-guard fixes and compile-cost fixes (b9afe4c), H1 integer-key reads (binary_trees),
H4/H2/H5 (free, setmetatable, new-key store, tail call, sunk-table rebuild), H7 small recursion savings, gate and benchmark tooling.

## Headline: geomean cycles, below 1 means ours is faster (session-nbody in brackets; set A)

| build | ours/LuaJIT (219) | ours/PUC (223) | ours/LuaJIT instructions |
|---|---|---|---|
| 5.5 plain | 0.963 (0.958) | 0.220 (0.220) | 1.369 (1.385) |
| 5.5 PGO | 0.878 (0.874) | 0.201 (0.201) | 1.261 (1.279) |
| 5.4 plain | 1.027 (1.017) | 0.229 (0.228) | 1.460 |
| 5.4 PGO | 0.936 (0.931) | 0.209 (0.209) | 1.346 |

Set B: 0.955 / 0.871 / 1.020 / 0.929 against LuaJIT; cached 1a5aad0 on the same programs: 0.964 / 0.879 / 1.020 / 0.934.
The suite-wide geomean is flat: gains are in object, closure, allocation and application programs; the library micro-benchmarks
moved 0.5% the wrong way (lib 1.005 cycles, 1.002 instructions).

## Subject vs 1a5aad0 and 108b935 (set A / set B cycles, instructions)

| build | vs | cycles A | cycles B | instructions | >3% better / within / >3% worse (B) |
|---|---|---|---|---|---|
| 5.5 plain | 1a5aad0 | 0.999 | 0.991 | 0.983 | 31 / 175 / 17 |
| 5.5 plain | 108b935 | 0.993 | 0.985 | 0.962 | 48 / 159 / 16 |
| 5.5 PGO | 1a5aad0 | 0.999 | 0.991 | 0.980 | 29 / 186 / 8 |
| 5.5 PGO | 108b935 | 0.994 | 0.987 | 0.960 | 47 / 160 / 16 |
| 5.4 plain | 1a5aad0 | 1.007 | 1.000 | 0.987 | 27 / 179 / 17 |
| 5.4 PGO | 1a5aad0 | 1.003 | 0.995 | 0.985 | 26 / 190 / 7 |

By set (5.5 plain vs 1a5aad0, cycles / instructions): apps 0.901/0.894, real 0.932/0.936, corpus2 heldout 0.959/0.915, awfy
0.976/0.966, corpus 0.982/0.975, corpus2 dev 0.991/0.970, programs 0.994/0.983, jit-trace 0.999/0.991, lib 1.005/1.002.

Biggest wins vs 1a5aad0 (5.5 plain, cycles / instructions): roguelike_turns 0.680/0.471, apps/expr_lang 0.684/0.767, binary_trees
0.783/0.745 (PGO 0.820), game_loop 0.790/0.824, awfy/cd 0.809/0.824, real/lua_interp 0.883/0.882, oop_mixins 0.884/0.857,
classic_vector 0.891/0.872, lib/setmt 0.897/0.956, astar_jumper 0.906/0.808, deltablue 0.921/0.918. The session-nbody regressions
(roguelike_turns, oop_mixins, classic_vector, game_loop, astar_jumper, cd, deltablue) are repaid. Against 108b935 nbody is 0.669.

## Regressions vs 1a5aad0 (programs of 20 Mcyc or more; causes are hypotheses, none investigated)

Real (instructions move with cycles):
- apps/expr_lang on 5.4: +50% / +53% cycles, +22% / +21% instructions (plain / PGO). The same program is -32% on 5.5, so a 5.4-only
  path is not taking the new fast paths.
- apps/raytrace on 5.4: +7.5% / +22% cycles, +6.8% / +10.5% instructions.
- lib/tonumber_int +14% (5.5) / +10% (5.4) cycles, +1.3% instructions; lib/tinsert_*, tsort_int, tostring_int, fmt_d, concat_op,
  dijkstra_routes, real_dis_mips: +1.5% to +3.7% instructions (probably the integer-key checks of H1).

Cycles up with instructions equal or down (layout / IPC): table_hash +6.5% / +10% (spread 26-31%), load_big +6.3%, sieve +4.9%,
lib/load_small +5.4% / +9%, apps/raytrace 5.5 PGO +12.5% (-5.5% instructions), event_promise PGO +11.8%; 5.4 plain only: lib/gsub_fn
+26%, tonumber_bad +20%, diff_levenshtein +16% (the 5.4 PGO build is unchanged).

## Focus programs (plain vs LuaJIT; PGO vs LuaJIT with 1a5aad0 in brackets; ours/node)

| program | plain/LJ | PGO/LJ | PGO/PUC | ours/node default | ours/node -s 20 |
|---|---|---|---|---|---|
| programs/nbody | 1.008 | 1.007 (0.998) | 0.082 | 1.27 | - |
| programs/fib | 2.670 | 2.672 (2.654) | 0.303 | 1.75 | - |
| programs/binary_trees | 0.752 | 0.647 (0.789) | 0.296 | 1.88 | - |
| programs/queens | 0.966 | 0.966 (0.969) | 0.422 | 1.55 | - |
| apps/raytrace | 3.431 | 3.209 (2.852) | 0.495 | 0.88 | 4.66 |
| awfy/cd | 2.395 | 2.213 (2.702) | 0.545 | 1.33 | 5.37 |
| awfy/list | 1.399 | 1.387 (1.463) | 0.294 | 1.35 | 4.68 |
| awfy/richards | 1.715 | 1.650 (1.692) | 0.290 | 0.51 | 3.04 |
| awfy/deltablue | 1.044 | 0.978 (1.058) | 0.641 | 0.78 | 3.73 |
| awfy/havlak | 1.325 | 1.155 (1.174) | 0.612 | 0.55 | 3.00 |

nbody stays at LuaJIT parity; binary_trees is 0.65 of LuaJIT with PGO. Still weak vs LuaJIT: raytrace (3.2-3.4), fib (2.67), permute
(3.3), cd (2.2). Note: the raytrace -26% quoted for H2 was the `raytrace 40` micro workload (instructions), not `apps/raytrace`
(-2% cycles plain, +12.5% PGO here); the program that gained most in cycles is expr_lang.

## JS ports vs node (plain unstripped release build, JIT on; full 30-row tables in `js_vs_node.md`)

Default workloads: geomean ours/node 0.75 cycles (0.71 wall), ours/LuaJIT 1.36 over 25 programs. 20x workloads (14 programs):
geomean 2.79 cycles (2.44 wall), ours/LuaJIT 1.46. Node's start-up (47 Mcyc) inflates its default-size number, so read the 20x column
for short programs. Worst at default size: binary_trees 1.88, fib 1.75, queens 1.55, heapsort 1.43, fannkuch 1.41, list 1.35,
cd 1.33, nbody 1.27. At 20x: cd 5.37, list 4.68, storage 4.68, raytrace 4.66, deltablue 3.73, richards 3.04, havlak 3.00.

## Caveats

- Canary before the main pass: median drift 1.11%, worst 2.69% (PUC nbody), so PUC-relative ratios carry about 1%; before the reruns
  0.53% / 1.84%. Idle checks clean apart from Firefox and kwin (3-5% each); one worker only.
- Set B flatters the subject by about 1% (15-run min vs cached min-of-5 baselines); set A is slightly pessimistic for the 67
  quick-only programs. Compare set A with session-nbody.
- 13 to 17 of the 101 re-run programs per build have a 15-run spread above 10% (table_hash 26-31%, binary_trees PGO 36%,
  lib/type_call 63%, lib/sel_n 24-35%); astar_jumper's instruction count varies 4.5G to 5.1G between runs of the same binary.
- 1a5aad0 and 108b935 columns come from the cache (session-nbody), not rebuilt; only the canary controls drift between sessions.
- JIT-off and startup columns were not in this matrix. LuaJIT has no variant for 5 of the 30 JS ports.
