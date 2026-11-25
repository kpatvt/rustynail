# Benchmark session-n: subject ab7d185, Lua 5.5 and 5.4, plain and PGO

Data: `fast_all.json` (fast pass, 223 programs), `rerun_15runs_cpu4.json` (9-run serial reruns of 127 flagged programs; the file name is historical), `tables.md`, `per_program.csv`, logs; `report_n.py` regenerates the tables.
Method as in session-l/m, with the shortened process: `scripts/bench-fast.sh --matrix 55plain,55pgo,54plain,54pgo`, CPU 4, cached PUC/LuaJIT columns, `FORCE_STALE=1` (LuaJIT was upgraded 2026-11-23, so every ours/LuaJIT ratio is approximate, the same caveat as session-m). No JS runs. Reruns: 9 runs (not 15) for the subject only; the baselines (7413bff, session-m 8cd1f06) keep their 15-run minima, so the comparison is slightly biased against us.
Subject ab7d185 = session-m (8cd1f06) plus: N2 follow-ups kept, T4 on, `sinkmm`/`resink`, `rapol=5`, `slink=2`, `cspeed` levers + `codemap` + rn-std pattern-cache fix (formation step), R1 `memfind`, R2 `t1g`, R5, R6 (runtime), the str_eq `ixfuse` displacement fix, `uses_gpr` hoist, FINDM/TSET fixes. Heavy gate, full mutant runner, oracle suites never run.
Verification before the benchmark: full `gate-light.sh --fast` on ab7d185: everything passed except the mutants group (432 of 438 caught). Five survivors were detector-coverage losses and were repaired in add8142 (detectors in tests only); `rec_inner_first_stale` is an older survivor. A further open item: `s1_frame_sides` under shipped `heatside` and `lazystub` 0/1 gave different exit counts (110530 vs 112294); the job is now pinned to `heatside=0`, so that divergence is untested (formation sensitivity or a real issue: not investigated).
All four builds equal PUC and `.expected` on all 223 programs except the known 5.4 harness artefacts (load_big, loops_arith).

## Headline (geomean over 223 programs; below 1 = faster)

| build | ours/LuaJIT cyc | /LuaJIT ins | vs session-m cyc / ins | vs 7413bff cyc / ins | vs 5a064d7 cyc / ins |
|---|---|---|---|---|---|
| 5.5 plain | 0.908 (m: 0.924, l: 0.939) | 1.268 | 0.983 / 0.984 | 0.971 / 0.950 | 0.969 / 0.945 |
| 5.5 PGO | 0.830 (m: 0.849, l: 0.858) | 1.172 | 0.979 / 0.983 | 0.971 / 0.950 | 0.970 / 0.946 |
| 5.4 plain | 0.958 (m: 0.984, l: 1.002) | 1.359 | 0.973 / 0.982 | 0.959 / 0.952 | 0.960 / 0.948 |
| 5.4 PGO | 0.882 (m: 0.904, l: 0.915) | 1.257 | 0.975 / 0.981 | 0.966 / 0.953 | 0.965 / 0.949 |

Since session-m: **-1.7% cycles on 5.5 plain, -2.1% on 5.5 PGO, -2.7% on 5.4 plain, -2.5% on 5.4 PGO**, with instructions -1.6% to -1.9%. Since session-l (the last run before this stretch of work): cycles -2.9% (5.5 plain) to -4.1% (5.4 plain). Against LuaJIT: 5.5 plain 0.939 -> 0.908, PGO 0.858 -> 0.830, 5.4 plain 1.002 -> 0.958, PGO 0.915 -> 0.882 (3-4% closer on every build since session-l; LuaJIT column stale). Per-set geomeans are in tables.md (vs 5a064d7 and vs 7413bff: e.g. corpus 0.947 and apps 0.955 vs 5a064d7 on 5.5 plain; real and real_dis flat to +2%).

## Biggest changes vs session-m (5.5 plain cycles / instructions)
- Improved: lib/str_eq 0.645 (the ixfuse displacement fix), corpus/classic_vector 0.665 / 0.641 (sunk-allocation replay), tonumber_int 0.83, lib/match 0.86, oop_methods 0.86, find_plain 0.90, lib/sub 0.90, awfy/cd 0.91, byte 0.91. Focus programs vs 7413bff: raytrace 0.859, lua_lexer 0.841, classic_vector 0.686, cd 0.903, richards 0.908, towers 0.944, roguelike_turns 0.938.
- Worse (cycles / instructions; most at equal instructions, i.e. layout or baseline luck): lib/oop_inherit 1.647 / 0.913 (instructions fell 9%, cycles +65%: unexplained, needs a look), lib/dot +33% at equal instructions, len_op +13%, it_next +9%, template_engine +8%, find_pat +7% (+3% ins), metamethods +7%, tonumber_flt +5%, alloc_ctor +5%, binary_trees +4%. On 5.4 the worst are dot +33%, it_next +10%, m_tointeger +5.5%, parser_combinator +4.9%, find_pat +4.9%.

## Where we stand vs LuaJIT now (plain / PGO cycles ratio)
fib 1.32 / 1.33; raytrace 2.29 / 2.15 (was 2.37); cd 2.14 / 1.98 (was 2.34); richards 1.61 / 1.57 (was 1.72); towers 1.88 / 1.83; permute 2.74 (LuaJIT lottery); lua_lexer 1.83 / 1.56; classic_vector 1.32 / 1.19 (was 1.99); havlak 1.27 / 1.10; ahead of LuaJIT on nbody 0.97, queens 0.95, binary_trees 0.60 / 0.49, deltablue 0.98 / 0.93, dijkstra_routes 0.89 / 0.86, rec_loop 0.54, real_dis_mips PGO 1.01.

## Open
- oop_inherit and dot cycle regressions at lower or equal instructions (not investigated); the s1_frame_sides lazystub/heatside divergence; `rec_inner_first_stale` survivor; 5.4 misses at equal instructions are layout.
- Off by default after bar misses: `t5`, `innerlink`, `memtune`, `t7`, TR24c `restarts`; T4 on. Not built (measured ceilings too small): frame model, NaN-boxing, function-entry roots, failed-recording predictor, further formation levers.
- Not pushed: main is 69 commits ahead of origin.
