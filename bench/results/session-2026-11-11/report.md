# Benchmark session 2026-11-11 (main 79e7441c)

Data: `tables.md`, `per_program_pass{A,B}.csv`, `pass{A,B}*.json`, `rerun_serial_cpu4_losers_15runs.json`, `build_logs/`.
Method as session-2026-11-10: min of 5, equal-length binary dirs, identical script paths, pass A CPUs 3-7 (all columns), pass B CPUs 4/6 (vs LuaJIT), idle checked (siblings 11-15). Plain and PGO (scripts/pgo-build.sh, 181 training runs, 0 failed) rebuilt for all six subjects. All 223 outputs match .expected and PUC. 51 programs re-run at 15 runs serial on CPU 4. Column "53133e8d" in the LuaJIT table is copied from the 2026-11-10 report. In tables.md part 1, header "q" means 4ce16b7a.

## Ours / LuaJIT (pass B, fast cores, geomean cycles, plain / PGO)
| set | n | 79e7441c | 4ce16b7a | 53133e8d |
|---|---|---|---|---|
| ALL | 219 | 1.109 / 1.010 | 1.152 / 1.050 | 1.181 / 1.078 |
| corpus (old) | 18 | 1.230 / 1.082 | 1.314 / 1.152 | 1.325 / 1.168 |
| corpus2 dev | 19 | 1.251 / 1.088 | 1.277 / 1.121 | 1.339 / 1.178 |
| corpus2 heldout | 13 | 1.294 / 1.169 | 1.312 / 1.192 | 1.351 / 1.237 |
| awfy | 14 | 1.464 / 1.371 | 1.457 / 1.368 | 1.502 / 1.421 |
| programs | 25 | 0.928 / 0.854 | 0.965 / 0.884 | 0.982 / 0.901 |
| jit-trace | 29 | 0.995 / 0.981 | 0.983 / 0.970 | 1.027 / 1.015 |
| lib | 91 | 1.057 / 0.953 | 1.123 / 1.012 | 1.145 / 1.027 |
| apps | 5 | 1.405 / 1.214 | 1.482 / 1.307 | 1.523 / 1.377 |
| real | 5 | 1.288 / 1.148 | 1.355 / 1.198 | 1.352 / 1.218 |
| real_dis (4) | 4 | 1.213 / 1.034 | 1.315 / 1.143 | 1.313 / 1.147 |

Older baselines (ALL plain/PGO): 8965e73e 1.198/1.090, c276c376 1.228/1.116, cb1af288 1.246/1.134. Vs PUC 0.255 plain / 0.233 PGO. Plain JIT / JIT-off 0.343; plain / non-JIT build 0.344.

## New vs 4ce16b7a (pass A, cycles / instructions)
| set | n | plain | PGO |
|---|---|---|---|
| ALL | 223 | 0.962 / 0.967 | 0.963 / 0.963 |
| corpus | 18 | 0.935 / 0.932 | 0.940 / 0.929 |
| corpus2 dev | 19 | 0.981 / 0.965 | 0.971 / 0.963 |
| corpus2 heldout | 13 | 0.974 / 0.979 | 0.970 / 0.976 |
| programs | 25 | 0.965 / 0.983 | 0.974 / 0.978 |
| lib | 95 | 0.943 / 0.949 | 0.944 / 0.944 |
| awfy | 14 | 1.005 / 1.008 | 1.001 / 1.005 |
| apps | 5 | 0.949 / 0.901 | 0.925 / 0.891 |
| jit-trace | 29 | 1.010 / 1.033 | 1.014 / 1.032 |
| real | 5 | 0.952 / 0.918 | 0.955 / 0.920 |
| real_dis (4) | 4 | 0.923 / 0.912 | 0.904 / 0.911 |

## New vs 53133e8d (pass A, cycles / instructions)
ALL 223: plain 0.940 / 0.925, PGO 0.940 / 0.922. lib 0.929/0.919, programs 0.944/0.948, apps 0.866/0.847, real 0.960/0.908, real_dis 0.929/0.903 (plain). Vs 8965e73e 0.927/0.916, c276c376 0.905/0.893, cb1af288 0.892/0.877. Noise 1.001.

## Winners vs 4ce16b7a (plain cyc/instr)
lib/match 0.240/0.211 (real; 15-run re-run 59 vs 245 Mcyc; output verified), tonumber_int 0.471, pcall_err 0.536, it_next 0.540, find_plain 0.606, it_next_empty 0.634, class_middleclass 0.656, tonumber_flt 0.660, programs/pcall_error 0.710. (table_hash 0.739 is noise.)

## Regressions vs 4ce16b7a (41 programs >3% in plain or PGO, full list in tables.md)
- Layout only (instr equal, PGO fine): lib/tremove_front 1.208, tinsert_front 1.173, programs/gc_churn 1.115.
- Real instruction increases, cycles track: lib/m_type 1.13, m_min 1.11, m_abs 1.10, m_max 1.09, sel_count 1.08, str_eq_dyn 1.10, awfy/sieve 1.08, jit-trace/inline_leaf 1.08, int_sum 1.06, type_entry_stable 1.06, m_random +12% instr. Cause UNVERIFIED (builder's guess: chaindepth / Bundle G); needs a bisect (knob chaindepth=0 is a one-flag check).
- jit-trace set is +1.0% cycles / +3.3% instructions vs 4ce16b7a: net regression on that set.
- lib/find_pat: plain flat vs 4ce16b7a (0.993, 15-run 1.007; instr 0.902); PGO +2-3% (1.069 vs 53133e8d).

## real
Cycles now track instructions: 0.952 / 0.918 vs 4ce16b7a (last session flat). wordfreq 0.909, template_engine 0.884, json_roundtrip 0.967, lua_interp 1.003 (PGO +2.6%), game_loop 1.004.

## Gaps vs LuaJIT (PGO, pass B)
classic_vector 4.37, awfy/permute 3.69, fib 3.14, raytrace 3.07, lua_lexer 2.63, jit-trace/tail_loop 2.55, lib/m_random_f 2.49, lib/type_call 2.48, then fmt_s/sub/byte_multi/char/find_plain 2.3-2.4. Largest absolute: fib 986 Mcyc, text_adventure 985, oop_mixins 962, short_inner 895, classic_vector 825. awfy 1.371 and apps 1.214 remain the weak sets.
