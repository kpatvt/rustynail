# Benchmark 3 (session-bm3): main 1e6ec4ca vs 80f1d96e (rebuilt) and session-2026-11-11 (79e7441c)

Data: `tables.md`, `extra.md`, `machine_state.txt`, `per_program_pass{A,B}.csv`, raw JSONs (pass A/B, 15-run losers rerun, knob attribution runs), logs, `build_logs/`.

## Method and machine
- Same driver as bm2: `scripts/bench_session.py`, min of 5 interleaved runs, equal-length 8-char binary dirs, identical script path, `RUSTYNAIL_SEED=0`.
- Pass A: 223 programs, CPUs 3-7. Pass B: 219 programs with a LuaJIT variant, CPUs 4 and 6. New this time: `SPIN_MS=300` (300 ms spin on the pinned CPU before each timed run).
- Builds: plain, PGO and non-JIT for 1e6ec4ca; plain and PGO for 80f1d96e (rebuilt). PGO trained 181 runs, 0 failed, for both.
- Idle: all benchmark CPUs and siblings under 2% busy before and after every pass; no cargo/rustc/lua/perf processes running.
- Machine: Ryzen AI 7 350, SMT on, amd-pstate-epp, governor powersave, EPP balance_performance, boost on; cpu4 at 4.94 GHz (max 5.09); Tctl 88 C in the first seconds under load; no throttling visible.
- Dup-binary column: 0.999 ALL (per set 0.997-1.003). The rebuilt 80f1d96e reproduces bm2 within about 1% (ALL 0.999 plain, 0.995 PGO; worst set corpus2 heldout 1.005 / 1.012).
- All outputs of all ours columns equal PUC and `.expected`. No LuaJIT variant for: corpus/testrunner_luaunit, corpus2/dev/real_dis_arm, programs/coroutines, programs/load_big, programs/oop_methods (output differs), lib m_tointeger, m_type, m_ult, utf8_len (no equivalent).

## Headline: ours / LuaJIT (pass B, geomean cycles; ALL = 219 programs)

| | plain | PGO |
|---|---|---|
| now (1e6ec4ca) | **1.010** | **0.918** |
| bm2 (80f1d96e as measured then) | 1.040 | 0.943 |
| session-2026-11-11 (79e7441c) | 1.109 | 1.010 |
| 80f1d96e rebuilt now | 1.041 | 0.945 |

- Excluding real and real_dis (210 programs): 1.002 plain, 0.913 PGO (bm2: 1.033, 0.939).
- Same-session 1e6ec4ca vs 80f1d96e, pass A, ALL 223, cycles / instructions: plain 0.968 / 0.971, PGO 0.972 / 0.972.
- vs PUC 5.5 cycles: 0.227 plain, 0.207 PGO. Plain vs JIT off: 0.315; vs the non-JIT build: 0.320.

## Per set (ours/LuaJIT pass B; same-session new vs 80f1d96e pass A, cycles / instructions)

| set | n(LJ) | plain/LJ now | PGO/LJ now | plain/LJ bm2 | PGO/LJ bm2 | plain/LJ 79e7441c | PGO/LJ 79e7441c | vs 80f1d96e plain | vs 80f1d96e PGO |
|---|---|---|---|---|---|---|---|---|---|
| ALL | 219 | 1.010 | 0.918 | 1.040 | 0.943 | 1.109 | 1.010 | 0.968 / 0.971 | 0.972 / 0.972 |
| apps | 5 | 1.251 | 1.091 | 1.246 | 1.091 | 1.405 | 1.214 | 0.979 / 0.980 | 0.970 / 0.978 |
| awfy | 14 | 1.464 | 1.374 | 1.427 | 1.339 | 1.464 | 1.371 | 0.992 / 1.008 | 0.992 / 1.008 |
| corpus | 18 | 1.157 | 1.009 | 1.203 | 1.058 | 1.230 | 1.082 | 0.960 / 0.962 | 0.954 / 0.961 |
| corpus2 dev | 19 | 1.211 | 1.059 | 1.225 | 1.073 | 1.251 | 1.088 | 0.986 / 0.977 | 0.985 / 0.979 |
| corpus2 heldout | 13 | 1.231 | 1.123 | 1.260 | 1.138 | 1.294 | 1.169 | 0.969 / 0.975 | 0.974 / 0.977 |
| jit-trace | 29 | 0.964 | 0.952 | 0.965 | 0.957 | 0.995 | 0.981 | 0.996 / 1.000 | 0.998 / 1.001 |
| lib | 91 | 0.889 | 0.796 | 0.944 | 0.838 | 1.057 | 0.953 | 0.945 / 0.950 | 0.956 / 0.952 |
| programs | 25 | 0.903 | 0.831 | 0.907 | 0.838 | 0.928 | 0.854 | 0.998 / 0.990 | 0.988 / 0.991 |
| real (5) | 5 | 1.236 | 1.086 | 1.242 | 1.095 | 1.288 | 1.148 | 0.989 / 1.002 | 0.987 / 1.000 |
| real_dis (4) | 4 | 1.147 | 0.985 | 1.175 | 0.995 | 1.213 | 1.034 | 0.965 / 0.955 | 0.985 / 0.967 |

corpus2 dev/heldout include their real_dis ports; real and real_dis are reported separately. The "vs 79e7441c" ratios are cross-session (that build was not rebuilt). The lib set carries the gain (-5.5% cycles same-session); corpus -4 to -5%; real_dis -3.5% plain (x86 0.955, ppc 0.923, mips 0.988, arm 0.994). awfy: cycles -0.8% but instructions +0.8%; no gain on awfy, apps (-2%, noise level) or programs/jit-trace.

## Top-15 remaining gaps to LuaJIT (pass B; bm2 value in parentheses)
PGO build:
1. awfy/permute 4.36 (2.78; LuaJIT-side drift, see caveats)
2. corpus/classic_vector 3.81 (4.33)
3. programs/fib 3.18 (3.12)
4. apps/raytrace 2.96 (2.97)
5. corpus/lua_lexer 2.60 (2.58)
6. lib/char 2.50 (2.32; PGO layout)
7. jit-trace/tail_loop 2.48
8. awfy/cd 2.13
9. lib/fmt_s 2.13
10. lib/byte_multi 2.12
11. jit-trace/short_inner 2.01
12. awfy/towers 2.01
13. real/lua_interp 1.99
14. corpus2/heldout/stream_closures 1.93
15. jit-trace/type_entry 1.83

Plain build: permute 4.40, classic_vector 4.28 (4.79), raytrace 3.57, fib 3.19, lib/m_random 3.16 (6.4x LuaJIT instructions), lua_lexer 2.87, tail_loop 2.48, cd 2.34, fmt_s 2.25, lib/co_yield 2.24 (2.47), text_adventure 2.18, byte_multi 2.13, stream_closures 2.10, lua_interp 2.09, towers 2.07.

Largest absolute PGO gaps (Mcyc): fib 1003, text_adventure 949, short_inner 888, lua_lexer 717, type_entry 706, classic_vector 685, type_entry_stable 666, oop_mixins 581, astar_jumper 577, programs/coroutines 451.

## Top-15 improvements vs 80f1d96e (same session, plain cycles, programs >= 30 Mcyc; instructions in parentheses; PGO after the slash)
- lib/tmove 0.395 (0.426) / 0.520
- lib/rep_sep 0.408 (0.373) / 0.414
- lib/tunpack 0.454 (0.441) / 0.498
- lib/rep 0.518 (0.482) / 0.619
- lib/tonumber_bad 0.597 (0.592) / 0.597
- lib/str_idx_concat_key 0.713 (0.757) / 0.737
- corpus/class_middleclass 0.727 (0.781) / 0.733
- lib/sel_count 0.815 (1.000) / 0.840 (layout flag: equal instructions)
- lib/find_plain 0.816 (0.900) / 0.740
- lib/m_floor 0.827 (0.877) / 0.826 (the bm2 regression is fixed by f2ifl)
- corpus2/dev/bignum_arith 0.849 (0.751) / 0.835
- lib/fmt_q 0.852 (0.991) / 0.992 (layout flag)
- lib/fmt_x_pad 0.862 (0.912)
- lib/fmt_d 0.864 (0.838)
- corpus/msgpack_pure 0.864 (0.928) / 0.882

Further: template_etlua PGO 0.828 (plain 0.882), heapsort PGO 0.839 (instructions 1.000: layout), prototype_objects 0.875, classic_vector 0.905 plain / 0.881 PGO (instructions 0.908 / 0.880, from rearmside=2 with rearmclean), co_yield 0.951 / 0.952, programs/coroutines 0.975 (instructions 0.895), coro_scheduler 0.983. Not counted (dup differs >3%): int_rec2 0.942, event_promise 0.951, sieve 0.964, m_min 0.951, table_hash 0.953.

## Regressions vs 80f1d96e
No set regressed (every set <= 1.000 cycles apart from awfy instructions at 1.008). Twelve programs are >3% worse in plain or PGO cycles in pass A; all 12 re-run at 15 serial runs on CPU 4, and with knobs off in the same binary via RUSTYNAIL_JIT_TRACE_LIMITS. libcall=7, retf/sidenest/tfmt=0 and rearmside=0 changed nothing for any of them. "All off" = libcall=7, t1q=1, amt=0, retf=0, sidenest=0, tfmt=0, rearmside=0 together.

| program | plain (pass A, 15 runs) | PGO (pass A, 15 runs) | instr | attribution |
|---|---|---|---|---|
| lib/sel_loop | 1.083, 1.092 | 1.067, 1.105 | 1.000 | **Bundle S t1q bit 4 (Lua `__index`)**: +8-10% at equal instructions; t1q=1 gives 233 vs 251 Mcyc (recovers 7%); bit 8 alone does not; amt=0 does not. All-off 0.976 plain / 0.995 PGO. |
| lib/tonumber_num | 1.031, 1.015 | 1.104, 1.036 | 1.000 | **Bundle S t1q**: plain base 213 vs 193 Mcyc with t1q=1; t1q=5 and t1q=9 both stay 213/212, so both bits must be off. PGO with t1q=1 gives 0.985. Mechanism unverified (code placement?). |
| corpus2/dev/regex_nfa | 1.043, 1.048 | 1.048, 1.043 | 1.001 | **Bundle S t1q bit 8 (`__newindex`)**: plain 742 vs 685 with t1q=1 (0.97 vs 80f1d96e); t1q=5 gives 687, t1q=9 gives 733. |
| corpus/parser_combinator | 1.088, 1.088 | 1.007, 1.008 | 1.002 | Layout (plain only); no knob recovers it (all-off 1.084). |
| lib/gsub_tbl | 1.103, 1.078 | 0.994, 1.000 | 0.991 | Layout, plain only (all-off 1.086). |
| corpus/csv_ini_yaml | 1.034, 1.034 | 1.006, 0.984 | 0.997 | Layout, plain only. |
| corpus2/dev/varargs_util | 1.031, 1.036 | 1.017, 1.018 | 1.004 | Small; mostly layout (all-off still 1.016). |
| lib/tonumber_int | 1.032, 1.021 | 0.997, 1.000 | 1.000 | Layout, plain only. |
| lib/error_tbl | 1.031, 1.023 | 1.011, 1.014 | 1.000 | Layout (all-off 1.021). |
| lib/concat_op_str | 1.015, 1.020 | 1.039, 1.038 | 1.000/1.003 | PGO layout (all-off the same). |
| lib/pcall_err | 1.009, 1.011 | 1.035, 1.028 | 1.013 plain | Small; the +1.3% plain instructions are not explained by a knob; PGO layout. |
| lib/char | 0.998, 1.001 | 1.079, 1.081 | 1.004 | PGO-only layout (reproduced at 15 runs; plain-only in bm2). |

Only the t1q cases (sel_loop, tonumber_num, regex_nfa) are attributable to a bundle (S); they are cycles-per-instruction changes (instructions equal), flagged "needs a look". The bm2 regression cluster is gone (m_floor 0.827, str_idx_concat_key 0.713). Other PGO cases above 1.02, all equal-instruction layout: string_ops 1.027, lua_lexer 1.026, real_dis_arm 1.022, json_rxi 1.022, log_analyzer 1.020, inline_2 1.020. Top plain regressions among programs >= 30 Mcyc: gsub_tbl 1.103, parser_combinator 1.088, sel_loop 1.083, regex_nfa 1.043, csv_ini_yaml 1.034; PGO: tonumber_num 1.104, lib/char 1.079, sel_loop 1.067, regex_nfa 1.048, concat_op_str 1.039.

## Caveats
- awfy/permute: ours/LJ 2.78 -> 4.36, but ours is flat at 50.9 Mcyc plain (bm2 50.6); LuaJIT went 18.0 Mcyc (bm2, cpu6) to 11.5 (now, cpu4), 13.4 in session-2026-11-11 on cpu4: a LuaJIT core/placement effect, not ours.
- Other LuaJIT drift >8%: heapsort LJ 0.833, apps/lru_cache LJ 0.915 (cores differ). programs/heapsort ours -24% in pass B vs bm2 but equal instructions in pass A: partly layout, partly LuaJIT drift.
- Pass A dup-noise above 3%: sieve 0.964, event_promise 0.951, int_rec2 0.942, m_min 0.951, table_hash 0.953.
- Cross-session comparisons are indicative only; same-session columns are the honest comparison. awfy instructions +0.8% with cycles -0.8% was not investigated. The machine ran hot (Tctl 88 C at load start), as before; the dup column shows no drift. Not verified: the cycles-per-instruction mechanism behind the S-bundle t1q regressions (no profiling done).

## What changed since 80f1d96e (bm2)
Bundle P (f2ifl floor check, fast_hash byte tail, rearmside knob), Q (t1q: tier-1 table ==/~=), S (t1q bits 4/8: Lua __index/__newindex handlers in tier 1; amt: array access on metatabled tables), T (fused coroutine resume), R (libcall 1023: rep, unpack, move, tonumber failing, concat keys, lean find), N-A/N-B (RETF infrastructure; defaults sidenest=1, retf=2 inline re-push stub, tfmt=1), C45 (rearmside=2 with rearmclean=1).

Net: lib -5.5%, corpus -4%, real_dis -3.5%, ALL -3.2% cycles same-session. ours/LJ ALL 1.041 -> 1.010 plain and 0.945 -> 0.918 PGO (same-session rebuild).
