# Benchmark session-i: subject 22670a1, Lua 5.5 and 5.4, plain and PGO

Data in this directory (`fast_all.json`, `rerun_15runs_cpu4.json`, `rerun2_15runs_cpu4.json`, `tables.md`, `js_vs_node.md`,
`per_program.csv`, `js/`, `js20/`, logs, `build_logs/`; `report_i.py` and `js_table.py` regenerate the tables). Method as in
session-h: `scripts/bench-fast.sh --matrix 55plain,55pgo,54plain,54pgo` with cached baselines (the session-h set B is imported as
`*-8c39465` in `bench/results/baselines/`), two quick runs per program, min-of-5 for 109 flagged programs, 15 serial runs for 83
(two batches: 57, then 26). Selection of the 83: any build more than 3% worse or more than 7% better than session-h, or a
quick-run spread above 5% at 20 Mcyc or more. Set A is the fast pass alone; set B merges the reruns. The four builds are fresh
`release-build.sh` builds (home-dir string check 0 on all four). All our outputs equal PUC and `.expected` on all 223 programs
(the harness flags programs/load_big and programs/loops_arith on 5.4 because it compares against the 5.5 reference; checked by hand,
both are byte-identical to PUC 5.4.9). All 30 JS ports match node at default size and at `-s 20`.

Work since session-h (8c39465): K4 (5.4 expr_lang/raytrace regressions), K5 (carried-entry-check budget bug), K6 (fnself), H8
diagnostics (off by default), release-only test fix, mixed int/float min/max + `table.remove(t, pos)` + `fnrec` (method recursion),
K3 frame-loop exits (`frame_link_short`, `fretry`), slotlive. K4 of the coverage study (side recording ending at a loop head) was
prototyped twice and not shipped.

## Headline: geomean cycles, below 1 means ours is faster (session-h set A in brackets)

| build | ours/LuaJIT (219) | ours/PUC (223) | ours/LuaJIT instructions |
|---|---|---|---|
| 5.5 plain | 0.955 (0.963) | 0.218 (0.220) | 1.360 (1.369) |
| 5.5 PGO | 0.873 (0.878) | 0.200 (0.201) | 1.253 (1.261) |
| 5.4 plain | 1.016 (1.027) | 0.227 (0.229) | 1.447 (1.460) |
| 5.4 PGO | 0.929 (0.936) | 0.208 (0.209) | 1.334 (1.346) |

Set B against LuaJIT: 0.950 / 0.869 / 1.010 / 0.925 (session-h set B 0.955 / 0.871 / 1.020 / 0.929). 5.5 vs LuaJIT -joff: 0.323
plain, 0.295 PGO. The suite-wide gain is about 0.8% in cycles and 0.7% in instructions; the gains are concentrated in a few
programs (list, raytrace, 5.4 expr_lang, tremove_front).

## Subject vs session-h and 1a5aad0 (cycles A / B, instructions, >3% better / within / >3% worse for A and B)

"8c39465 setA" is our set A against session-h's set A (like for like). "8c39465 merged" is against the cached merged column and looks
worse because session-h merged 101 reruns and this session 83.

| build | vs | cycles A / B | instr | counts A | counts B |
|---|---|---|---|---|---|
| 5.5 plain | 8c39465 setA | 0.992 / 0.987 | 0.993 | 23 / 196 / 4 | 31 / 189 / 3 |
| 5.5 plain | 8c39465 merged | 1.000 / 0.995 | 0.993 | 12 / 198 / 13 | 19 / 199 / 5 |
| 5.5 plain | 1a5aad0 | 0.991 / 0.986 | 0.976 | 36 / 168 / 19 | 45 / 163 / 15 |
| 5.5 PGO | 8c39465 setA | 0.995 / 0.991 | 0.993 | 12 / 206 / 5 | 20 / 201 / 2 |
| 5.5 PGO | 8c39465 merged | 1.002 / 0.998 | 0.993 | 8 / 198 / 17 | 11 / 208 / 4 |
| 5.5 PGO | 1a5aad0 | 0.994 / 0.989 | 0.974 | 28 / 173 / 22 | 32 / 175 / 16 |
| 5.4 plain | 8c39465 setA | 0.989 / 0.983 | 0.991 | 21 / 194 / 8 | 32 / 188 / 3 |
| 5.4 plain | 8c39465 merged | 0.997 / 0.991 | 0.991 | 16 / 191 / 16 | 21 / 196 / 6 |
| 5.4 plain | 1a5aad0 | 0.996 / 0.990 | 0.978 | 28 / 170 / 25 | 33 / 178 / 12 |
| 5.4 PGO | 8c39465 setA | 0.992 / 0.988 | 0.991 | 13 / 204 / 6 | 20 / 199 / 4 |
| 5.4 PGO | 8c39465 merged | 1.000 / 0.996 | 0.991 | 8 / 202 / 13 | 11 / 208 / 4 |
| 5.4 PGO | 1a5aad0 | 0.995 / 0.991 | 0.976 | 28 / 175 / 20 | 31 / 183 / 9 |

By set, 5.5 plain vs session-h (fast-pass cycles / instructions): awfy 0.989/0.965, apps 0.982/0.963, corpus2 heldout 1.002/0.991,
lib 1.000/0.994, jit-trace 1.003/1.000, corpus 1.007/0.999, real 1.014/0.999. 5.4 plain apps: 0.882/0.888.

## Effects of the new work (cycles vs session-h: 5.5 plain / PGO / 5.4 plain)

- **K3 frame-loop exits:** awfy/list 0.777 / 0.766 / 0.755 (instructions 0.639), i.e. -22%, as the builder measured. list/LuaJIT went
  from 1.40 to 1.09 plain and 1.06 PGO. Also apps/raytrace 0.827 / 0.825 / 0.757, stream_closures 0.870 / 0.869 / 0.844,
  particles_physics 0.948.
- **Mixed min/max + table.remove + fnrec:** lib/tremove_front 0.591 / 0.595 / 0.634 (instructions 0.547), the largest single gain.
  tremove_pop and m_min/m_max are flat on 5.5. The raytrace-40 and permute-variant workloads that the builder measured are not in the
  223-program suite; no jit-trace program moved more than 1% in instructions or 4% in cycles.
- **slotlive:** instructions drop on permute (0.976), fannkuch (0.976), towers and richards (0.991). Cycles: permute 0.990, fannkuch
  0.999 plain / 0.983 PGO, towers 0.979, richards 0.995.
- **K4/K5/K6 5.4 regressions repaid:** 5.4 expr_lang is 0.673 of session-h (instructions 0.798) and 1.010 of 1a5aad0, where it was
  +50%. 5.4 raytrace is 0.757 of session-h and 0.813 of 1a5aad0 (it was +7.5% plain, +22% PGO); PGO raytrace is 0.922 of 1a5aad0.
- **H8 diagnostics:** no change seen (5.5 expr_lang 1.005 vs session-h). The K5 entry-check bug and fnself show up only through the
  5.4 programs; there is no separate signature in the 5.5 suite.

Biggest wins vs session-h (5.5 plain, cycles / instructions): lib/tremove_front 0.591/0.547, awfy/list 0.777/0.639, apps/raytrace
0.827/0.837, stream_closures 0.870/0.849, load_big 0.908/1.000 (instructions equal, so layout), co_yield 0.939 (15% spread, noise).
5.4 also gains on expr_lang (0.673), gsub_fn (0.786, instructions equal) and diff_levenshtein (0.897).

## Regressions vs session-h (causes are hypotheses; none investigated)

- **corpus2/heldout/dijkstra_routes, 5.5 only:** +13.8% cycles plain, +14.0% PGO, instructions +29%. Deterministic (spread 1-2%); the
  5.4 builds are not affected. Hypothesis: a path from the new work (`table.remove(stack)`, `table.insert(path, 1, x)`, `math.min`, or
  a frame-exit side trace) exits or retries more than before. **This is the one real regression to look at.**
- **corpus/astar_jumper:** +4.2% plain, +5.5% PGO on 5.5; +14% plain, +13% PGO on 5.4; instructions +7% to +16%. The program's
  instruction count is unstable between runs (4.5G-5.1G in session-h), but the 5.4 shift is larger than that noise.
- 5.4 only: regex_nfa +11% (instructions +13%), lib/m_min +11.5% (instructions equal, layout), diff_levenshtein PGO +7%,
  parser_combinator PGO +4.8%. scheme_interp is -4% in cycles but +6.5% in instructions.
- Cycles up with equal instructions (layout, under 7%): table_hash +4%, pcall_error +3.4%, tonumber_bad PGO +7%, fib PGO +2.8%,
  binary_trees +2.3%, cd +2%, deltablue +2.6% / +1.9%.

## Focus programs (cycle ratios; ours/node from the JS ports)

| program | plain/LJ | PGO/LJ | PGO/PUC | vs session-h plain (cyc / ins) | ours/node default | ours/node -s 20 |
|---|---|---|---|---|---|---|
| nbody | 0.998 | 0.995 | 0.081 | 0.990 / 0.984 | 1.24 | - |
| fib | 2.677 | 2.748 | 0.312 | 1.003 / 1.000 | 1.75 | - |
| binary_trees | 0.769 | 0.661 | 0.302 | 1.023 / 1.000 | 1.89 | - |
| queens | 0.964 | 0.965 | 0.421 | 0.998 / 1.000 | 1.56 | - |
| apps/raytrace | 2.837 | 2.649 | 0.409 | 0.827 / 0.837 | 0.73 | 3.83 |
| cd | 2.439 | 2.268 | 0.558 | 1.018 / 1.003 | 1.41 | 5.35 |
| list | 1.087 | 1.063 | 0.225 | 0.777 / 0.639 | 1.03 | 3.16 |
| richards | 1.707 | 1.640 | 0.288 | 0.995 / 0.991 | 0.57 | 3.03 |
| deltablue | 1.072 | 0.997 | 0.653 | 1.026 / 1.002 | 0.79 | 3.88 |
| havlak | 1.327 | 1.177 | 0.623 | 1.001 / 0.997 | 0.63 | 2.96 |
| awfy/permute | 3.336 | 3.289 | 0.175 | 0.990 / 0.976 | 0.55 | 2.41 |

Still weak against LuaJIT: permute 3.3, fib 2.7, raytrace 2.6-2.8, cd 2.3-2.4, lua_lexer 2.76, classic_vector 2.67.

## JS ports vs node (plain unstripped release build; full 30 rows in `js_vs_node.md`)

Default size: ours/node 0.75 cycles (0.71 wall), ours/LuaJIT 1.32 over 25 programs (session-h 0.75 / 0.71 / 1.36). 20x size
(14 programs): 2.60 cycles (2.28 wall), ours/LuaJIT 1.41 (session-h 2.79 / 2.44 / 1.46). list at 20x went from 4.68 to 3.16 and
raytrace from 4.66 to 3.83. Worst at default size: binary_trees 1.89, fib 1.75, queens 1.56, cd 1.41. Worst at 20x: cd 5.35,
storage 4.48, deltablue 3.88.

## Caveats

- **Canary drift was above the stop threshold.** PUC 5.4 alone drifted +4% to +9% (richards +7-9%, class_middleclass +5-6%); PUC 5.5
  and LuaJIT stayed within 0-3.2%. The 6% worst-case stop tripped, so the first two attempts halted and the main pass, the first
  rerun batch and the second batch (worst 6.85%) ran with `FORCE_STALE=1`. Median |drift| per attempt: 1.3%, 2.2%, 1.9% (main pass),
  1.7% (rerun 1), 0.7% (rerun 2).
- Why it was accepted: the PUC 5.4 binary is unchanged (mtime 2026-10-27), and the drift does not depend on environment size. The
  session-h binaries re-measured the same day were within about 1% of their cached numbers (+0.5 / +0.9 / +1.3% on 5.5 plain,
  -0.05 / -0.3 / +1.0% on 5.4 plain; LuaJIT +1.4 / -0.8 / +0.3%).
- Effect on ratios: the cached PUC 5.4 column was not re-measured, so ours/PUC-5.4 uses the old reading. If PUC 5.4 really is about
  5% slower today, the true ratio is about 5% lower. Ours/LuaJIT and the 5.5 columns are not affected by that.
- Set B flatters the subject by about 1%; set A is slightly pessimistic for the 114 quick-only programs. Compare set A with set A.
- Several rerun programs have 15-run spreads above 10%: find_pat 32%, co_yield 15%, astar_jumper 14.5%, regex_nfa 12%, table_hash 10-12%.
- The 1a5aad0 and 108b935 columns come from the cache, not rebuilt. JIT-off and startup columns were not in this matrix. LuaJIT has
  no variant for 5 of the 30 JS ports.
