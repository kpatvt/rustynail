# Benchmark session 2026-11-09: main 53133e8d

Subject: main `53133e8d` (plain and PGO builds). Baselines (plain and PGO): `8965e73e` (the previous benchmark's subject), `c276c376`, `cb1af288`. Also the interpreter (JIT off), PUC Lua 5.5 and LuaJIT. Method as in `bench/results/session-2026-11-08/report.md`: pass A on CPUs 3-7 (one program per core, 5 workers), pass B on fast Zen5 CPUs 4 and 6 for the LuaJIT comparison, min of 5 interleaved runs, `RUSTYNAIL_SEED=0`, a dup noise column, serial re-runs (CPU 4, 9 runs) for outliers. All 223 programs match `.expected` and PUC on every ours column (no mismatch). LuaJIT ratios cover 219 programs.

Hygiene change this time (from the dispatch/BOLT study): all 8 binaries were built first, no cargo ran during timing, and before each pass the benchmark CPUs and their hyperthread siblings (11-15) were checked idle. Load average at pass start: A 0.23, B 0.18, serial re-run 0.40.

PGO is trained on awfy, apps, programs, lib, jit-trace and real, so PGO numbers on those sets are optimistic; corpus and corpus2 PGO numbers are hold-out.

Data: `passA_cpus3-7_all_columns.json`, `passB_cpus4-6_fast_lj.json`, `per_program_pass{A,B}.csv`, `tables.md`, `rerun_*`, logs. Scripts: `scripts/bench_session3_report.py`, `scripts/bench_session3_rerun.py`.

## Headline, pass B (fast cores, ours/LuaJIT, below 1 = ours faster)

| set | n | plain | PGO | 8965e73e plain | 8965e73e PGO | c276c376 plain | cb1af288 plain |
|---|---|---|---|---|---|---|---|
| corpus (old) | 18 | 1.330 | 1.204 | 1.358 | 1.195 | 1.423 | 1.439 |
| corpus2 dev | 19 | 1.332 | 1.193 | 1.407 | 1.232 | 1.448 | 1.630 |
| corpus2 heldout | 13 | 1.349 | 1.245 | 1.402 | 1.272 | 1.466 | 1.599 |
| awfy | 14 | 1.298 | 1.236 | 1.290 | 1.218 | 1.300 | 1.288 |
| programs | 25 | 0.973 | 0.900 | 0.985 | 0.899 | 0.993 | 0.991 |
| jit-trace | 29 | 1.024 | 1.011 | 1.025 | 1.015 | 1.024 | 1.025 |
| lib | 91 | 1.141 | 1.024 | 1.156 | 1.040 | 1.198 | 1.199 |
| apps | 5 | 1.510 | 1.366 | 1.403 | 1.253 | 1.433 | 1.432 |
| real | 5 | 1.348 | 1.202 | 1.373 | 1.209 | 1.388 | 1.379 |
| real_dis (4) | 4 | 1.309 | 1.156 | 1.337 | 1.156 | 1.478 | 1.476 |
| real + real_dis | 9 | 1.330 | 1.181 | 1.357 | 1.185 | 1.427 | 1.421 |
| **ALL (219)** | 219 | **1.166** | **1.070** | 1.183 | 1.078 | 1.214 | 1.233 |

Against PUC (all programs): 0.271 plain, 0.249 PGO; against our interpreter: 0.353.

## Headline, pass A (cycle geomeans across all cores)

| set | n | plain/PUC | PGO/PUC | plain/LJ | PGO/LJ | plain vs 8965 | PGO vs 8965 | plain vs c276 | plain vs cb1 |
|---|---|---|---|---|---|---|---|---|---|
| corpus (old) | 18 | 0.532 | 0.482 | 1.301 | 1.178 | 0.980 | 1.010 | 0.936 | 0.924 |
| corpus2 dev | 19 | 0.476 | 0.427 | 1.336 | 1.198 | 0.943 | 0.974 | 0.917 | 0.815 |
| corpus2 heldout | 13 | 0.450 | 0.420 | 1.341 | 1.251 | 0.952 | 0.976 | 0.909 | 0.836 |
| awfy | 14 | 0.281 | 0.268 | 1.492 | 1.422 | 1.004 | 1.013 | 0.996 | 1.003 |
| programs | 25 | 0.232 | 0.216 | 0.971 | 0.905 | 0.974 | 0.997 | 0.967 | 0.980 |
| jit-trace | 29 | 0.081 | 0.080 | 1.022 | 1.012 | 0.996 | 0.995 | 0.996 | 0.995 |
| lib | 95 | 0.275 | 0.248 | 1.140 | 1.022 | 0.989 | 0.984 | 0.955 | 0.953 |
| apps | 5 | 0.628 | 0.571 | 1.526 | 1.389 | **1.073** | **1.100** | 1.055 | 1.054 |
| real | 5 | 0.504 | 0.451 | 1.361 | 1.217 | 0.977 | 0.994 | 0.965 | 0.974 |
| real_dis (4) | 4 | 0.640 | 0.567 | 1.305 | 1.157 | 0.982 | 1.001 | 0.887 | 0.884 |
| real + real_dis | 9 | 0.560 | 0.499 | 1.336 | 1.190 | 0.979 | 0.997 | 0.929 | 0.933 |
| **ALL (223)** | 223 | **0.271** | **0.249** | 1.174 | 1.079 | 0.984 | 0.992 | 0.959 | 0.945 |

Dup column geomean 0.990-1.004 per set, 1.001 overall.

## What changed since the last benchmark (8965e73e, plain)

- **Overall gain is small: 1.6%** (ALL plain 0.984, corpus2 dev 0.943, heldout 0.952). PGO versus PGO: 0.992 overall.
- **The `apps` set got 7-10% slower, entirely from `apps/expr_lang` (a real regression, see below).** Without it apps is flat.
- `awfy`, `jit-trace` flat. `programs` plain 0.974 (mostly `table_hash` layout noise).
- Against c276c376 (start of the previous stretch): ALL plain 0.959, corpus2 dev 0.917, real_dis 0.887. Against cb1af288 (start of the session): ALL plain 0.945, corpus2 dev 0.815.

### Real programs, pass B, versus 8965e73e (plain / PGO)

real_dis_arm 0.970 / 0.992, real_dis_x86 1.025 / 1.049, real_dis_mips 0.954 / 0.969, real_dis_ppc 0.968 / 0.991. real/game_loop 0.997 / 0.978, json_roundtrip 0.986 / 1.023, lua_interp 0.950 / 1.009, template_engine 0.972 / 0.993, wordfreq 1.006 / 0.968.
Plain/LuaJIT: real_dis_arm 1.385, x86 1.114, mips 1.325, ppc 1.435; game_loop 1.541, json_roundtrip 1.222, lua_interp 2.070, template_engine 0.868 (faster than LuaJIT), wordfreq 1.314. LuaJIT's real_dis_arm output differs from PUC's (documented `#-0` lines), so its time is not strictly comparable.

## Winners versus 8965e73e (plain, pass A; instruction ratio in brackets)

lib/len_tbl 0.610 (0.740), corpus2/dev/diff_levenshtein 0.658 (0.648), corpus2/dev/regex_nfa 0.661 (0.691), lib/tinsert_append 0.744 (0.899), lib/tinsert_idx 0.777 (0.894), corpus/classic_vector 0.805 (0.812), heldout/forth_vm 0.838 (0.910), lib/gmatch_words 0.854 (0.917), jit-trace/alloc_ctor 0.879 (0.937). (programs/table_hash 0.768 has instructions 1.000, serial 0.994: layout noise.)

These are the BUNDLE-A (inline `#t`, key-0 SETI/GETI), BUNDLE-B (tier-1 comparison metamethods), BUNDLE-C (`sbump`) and BUNDLE-GM (gmatch) wins as predicted by their own A/B runs.

## Losers and regressions versus 8965e73e (plain, pass A; serial CPU 4 x9 re-run, instruction counts checked)

**Real regression: `apps/expr_lang`** plain 1.558 (pass A), 1.586 (pass B), 1.564 serial; instructions +29.8% (PGO 1.617, instructions +27.8%). About 230 Mcyc. It is about 1.5x slower than c276c376 and cb1af288 too, which are equal to 8965e73e, so it regressed between 8965e73e and 53133e8d. Bundle C's own A/B (versus 17a24ba5) had expr_lang improving, so the cause is probably in C39, BUNDLE-A, BUNDLE-B or BUNDLE-GM. **A bisect builder was started** (results to be appended to PERF.md and the queue memory).

Small genuine instruction increases (plain): `lib/sub_1char` +7.9% instr (cycles 1.102 serial; PGO flat), `programs/knucleotide` +5.6% instr (cycles +5.2%). To be checked by the same bisect.

Layout or noise (instructions equal or lower): lib/str_lt (1.124 serial; slow versus c276 too, a persistent layout effect), awfy/towers (1.124 serial, instructions 0.957; the known `sbump` cost, only 48 Mcyc), lib/match, lib/tconcat (instructions equal), class_middleclass 1.038 (instr 1.038, the accepted BUNDLE-B cost), corpus/lua_lexer 1.033 (instr 0.982), real_dis_x86 plain 1.025 / PGO 1.050 (instr 1.011).

PGO-only layout (instructions about equal; plain is fine): corpus/parser_combinator 1.136, mustache_lustache 1.103, programs/gc_churn 1.088, corpus/json_rxi 1.065, corpus2/dev/lua_parser 1.064, heldout/log_analyzer 1.056, lib/pcall_err 1.054 (instr +1.7%, the known path-length effect), lib/type_call 1.038, csv_ini_yaml 1.041.
Gone on the serial re-run: lib/tmove_1k (dup column also 1.063), csv_report, memo_cache, string_ops, table_hash PGO.
Known swings: real/lua_interp PGO +7-8% versus cb1 (instructions 0.967, environment-length), lib/fmt_q plain +5.4% versus c276 (same as last time).

## Biggest remaining gaps (PGO / LuaJIT, pass B, worst first)

classic_vector 4.59, lib/it_next_empty 3.53, lib/find_plain 3.49, class_middleclass 3.24, lib/sub 3.15, apps/raytrace 3.08, programs/fib 3.03, lua_lexer 2.68, lib/tonumber_int 2.64, jit-trace/tail_loop 2.61, lib/m_random_f 2.49, lib/fmt_s 2.48 (8 Mcyc), lib/type_call 2.40, lib/byte_multi 2.37, lib/char 2.35, iter_pipeline 2.32, lib/reverse 2.29, lib/getmt 2.29, lib/str_lt 2.21, lib/it_pairs_hash 2.20, awfy/cd 2.16, apps/expr_lang 2.16, lib/byte 2.13, lib/rep 2.13, jit-trace/type_entry 2.12.

New names compared with the last list (many are tiny lib micros where call overhead dominates; `apps/raytrace`, `programs/fib` and `jit-trace/tail_loop` deserve a look). Several of these (`classic_vector`, `class_middleclass`, `lua_lexer`, `iter_pipeline`, `cd`) are the known frame-model gaps (C21/C37/C38 in JIT_PARKED.md).

## Notes

- The dup column and the benchmark hygiene held: dup swings over 3% only on havlak 1.040, log_analyzer 0.968, prototype_objects 0.915, tmove_1k 1.063, gc_churn 0.966, table_hash 1.041. Instruction counts decide.
- Test coverage gaps stated by the builders (full every-mutant runner, 5.4 build tests) are unchanged; they belong to the release phase.

## Addendum: regression resolved (commit b8afcc6a)

- `apps/expr_lang` (+29.8% instructions) came from C39 `innerretry` (531c17d5): a callee `while` loop head retried in round 2 recorded an 84 KB root that was flushed `Ineffective`, and the re-record installed a 120 KB root with 0 entries and 41814 exits (side traces 64 -> 192, deep recordings 3 -> 137: compile cost in a 100 ms program). Fixed by x4 retry waits and no re-record after an ineffective retried trace: 810 M -> 643 M instructions (8965e73e: 640 M). The C39 wins on lua_lexer and iter_pipeline are kept.
- `lib/sub_1char` (+7.9%) and `programs/knucleotide` (+5.6%) came from BUNDLE-GM (53133e8d), not the gmatch recorder: the new `jit_gmatch_step` changed an LTO inlining decision (`Heap::jit_str_probe` stopped being inlined into `jit_tr_alloc`). Fixed with `#[inline]`.
- **Measurement caveat for this and earlier reports:** instruction counts depend on the script path length AND on the binary path length, even with `RUSTYNAIL_SEED=0` (corpus/classic_vector: 5.74 G at one binary path, 6.52 G on a copy at a longer path; expr_lang 810 M vs 634 M at two script paths). This benchmark used binaries at different paths, so some old-corpus swings (classic_vector in particular) include this effect. Future benchmarks must place all columns' binaries in equal-length directories and use identical script paths.
- PGO builds were not re-measured after the fix.
