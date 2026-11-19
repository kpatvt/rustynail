# Benchmark after the nbody work (session-nbody): HEAD 1a5aad0, Lua 5.5 and 5.4, plain and PGO

Data: `extra.md`, `vs_baseline.md`, per-pass JSON/CSV, logs, `build_logs/`, `machine_state.txt`.
Method as in session-final (min of 5 interleaved runs, `perf stat cycles:u,instructions:u`, `RUSTYNAIL_SEED=0`, pinned CPUs, 15-run serial
re-runs of noisy programs). The session-final subject 108b935 was rebuilt in the same session as baseline; it reproduces session-final's
geomeans within 0.001. All our outputs equal PUC and `.expected` on all 223 programs.

Changes since 108b935 that matter here: layout-id shape guards on HREFK (S4), HSTORE metatable guard, entry/side guard diet, HREFK address
folding, bytecode-liveness snapshot pruning (F4), side-entry guard elision (F2), register-carrying transitions (regcarry), plus several fixes
(recorder entry snapshot, stale `lazy_root`, `table.move` overlap).

## Headline: geomean cycles, below 1 means ours is faster (session-final in brackets)

| build | ours/LuaJIT 2.1 (219) | ours/PUC, same version (223) |
|---|---|---|
| 5.5 plain | 0.958 (0.965) | 0.220 (0.222) |
| 5.5 PGO | 0.874 (0.880) | 0.201 (0.202) |
| 5.4 plain | 1.017 (1.021) | 0.228 (0.229) |
| 5.4 PGO | 0.931 (0.934) | 0.209 (0.210) |

- Instructions vs LuaJIT: plain 1.385 (1.418), PGO 1.279 (1.310).
- Subject vs rebuilt 108b935 (pass A, 223 programs): cycles 0.994 plain / 0.996 PGO, instructions 0.979 for both.
  The suite-wide gain is small; it is concentrated in jit-trace (0.992 cycles, 0.910 instructions), programs and lib.
- JIT gain 0.300 on 5.5, 0.305 on 5.4; startup 909k cycles plain (896k before; PUC 709k), binary 3.6% larger.

## nbody

| | now | session-final | vs LuaJIT now (before) |
|---|---|---|---|
| programs/nbody | 344.8 Mcyc | 523.4 | 0.998 (1.50) |
| awfy/nbody | 92.3 Mcyc | 118.3 | 0.976 (1.24) |

nbody is at LuaJIT parity in cycles. JS ports vs node v26 (short runs, includes node compile time, not steady state): nbody ours/node 1.28
(wall 1.14: ours 81 ms, node 71 ms, LuaJIT 78 ms); geomean over the ten ports about 1.07 cycles, 1.00 wall. Worst: binary_trees 2.29, fib 1.78,
queens 1.56; best: sieve 0.25, spectral_norm 0.48.

## Regressions (deterministic instruction counts more than 3% worse than 108b935; cycles move with them)

| program | instructions | cycles (15-run) |
|---|---|---|
| corpus2/heldout/roguelike_turns | +87% | +47% |
| corpus2/dev/oop_mixins | +16% | +13% |
| corpus/classic_vector | +13% | +12% |
| real/game_loop | +12% | +25% |
| corpus/astar_jumper | +12% | +7% |
| awfy/cd | +11% | +19% |
| awfy/deltablue | +8% | +12% |
| awfy/richards | +7% | +7% |
| corpus2/heldout/prototype_objects | +6.5% | +4.5% |
| corpus2/dev/real_dis_x86 | +4.7% | +4% |
| apps/expr_lang | +4.1% | +6% |
| apps/lru_cache | +3.3% | +3% |

All are object, field-store and method programs, which fits the shape guards or the HSTORE guard costing instructions where they do not pay
back (cause not yet investigated). Ours/LuaJIT (PGO) for the worst four: roguelike_turns 1.22 to 1.87, game_loop 1.47 to 1.85, cd 2.23 to
2.59, classic_vector 2.35 to 2.60. Cycles up while instructions drop (IPC loss): jit-trace/sink_vec +33%, inline_2 +8%.
Layout effects (identical instructions, persistent): lib/fmt_q plain +11 to +14%, corpus/parser_combinator +4 to +5%.

## Biggest improvements (plain cycles vs 108b935)

programs/nbody 0.662, awfy/nbody 0.783, lib/type_call 0.805, jit-trace/field_method 0.839, lib/sel_n 0.857, lib/sel_loop 0.883,
lib/len_op 0.884, jit-trace/oop_override 0.889, lib/sel_count 0.907, corpus2/dev/diff_levenshtein 0.911 (full 15 in `extra.md`).

## Caveats

Only 108b935 was rebuilt as same-session baseline (9a9b8ed and 8a18eb3 were not). Wall-clock startup from the main hyperfine run read about
40% high; use the cycle counts. Idle checks clean at every pass boundary; swap was 10 GB used throughout (not under pressure).
