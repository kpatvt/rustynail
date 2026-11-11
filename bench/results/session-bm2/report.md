# Benchmark 2 (session-bm2): main 80f1d96e vs 79e7441c (session-2026-11-11)

Data: `tables.md` (all per-program tables, top-15 lists, >3% list with 15-run reruns, output check), `per_program_pass{A,B}.csv`, raw JSONs, logs, `build_logs/`.

## Method
- Same driver as session-2026-11-11 (`scripts/bench_session.py`): min of 5 interleaved runs, equal-length binary directories, identical script paths, `RUSTYNAIL_SEED=0` with padded environment.
- Pass A: 223 programs, CPUs 3-7. Pass B: 219 programs with a LuaJIT variant, CPUs 4 and 6.
- Idle checked before and after every pass (benchmark CPUs and siblings 11-15 under 2% busy; other processes at a few percent on other cores).
- All 223 outputs of every ours column match `.expected` and PUC.
- Built plain, PGO and non-JIT for 80f1d96e, and plain and PGO for 79e7441c (rebuilt, so "vs previous" ratios are same-session). The rebuilt 79e7441c reproduces last session within 0.4%. Older subjects were not rebuilt.
- The 300 ms CPU spin is not part of `bench_session.py` as used last session; it was not added.
- Duplicate-binary noise column: 1.001.

## Headline: ours / LuaJIT (pass B, geomean cycles, ALL = 219 programs)

| | plain | PGO |
|---|---|---|
| now (80f1d96e) | 1.040 | 0.943 |
| before (79e7441c, session-2026-11-11) | 1.109 | 1.010 |
| 79e7441c rebuilt and measured now | 1.105 | 1.005 |

- Honest ALL delta (same-session rebuild): plain 1.105 -> 1.040 (-5.9%), PGO 1.005 -> 0.943 (-6.2%).
- Excluding real and real_dis (210 programs): 1.033 plain, 0.939 PGO.
- Same-session 80f1d96e vs 79e7441c, ALL 223 programs, cycles / instructions: plain 0.938 / 0.934, PGO 0.938 / 0.931.
- vs PUC 5.5 cycles: 0.240 plain, 0.219 PGO. Plain vs JIT off: 0.324 (vs the non-JIT build 0.329).

## Per set: ours / LuaJIT, and 80f1d96e vs 79e7441c in pass A (cycles / instructions)

| set | n (LJ) | plain/LJ now | PGO/LJ now | plain/LJ before | PGO/LJ before | vs 79e7441c plain | vs 79e7441c PGO |
|---|---|---|---|---|---|---|---|
| ALL | 219 | 1.040 | 0.943 | 1.109 | 1.010 | 0.938 / 0.934 | 0.938 / 0.931 |
| corpus | 18 | 1.203 | 1.058 | 1.230 | 1.082 | 0.976 / 0.976 | 0.978 / 0.976 |
| corpus2 dev | 19 | 1.225 | 1.073 | 1.251 | 1.088 | 0.977 / 0.982 | 0.982 / 0.978 |
| corpus2 heldout | 13 | 1.260 | 1.138 | 1.294 | 1.169 | 0.988 / 0.979 | 0.983 / 0.984 |
| awfy | 14 | 1.427 | 1.339 | 1.464 | 1.371 | 0.993 / 0.989 | 0.997 / 0.990 |
| programs | 25 | 0.907 | 0.838 | 0.928 | 0.854 | 0.959 / 0.973 | 0.981 / 0.977 |
| jit-trace | 29 | 0.965 | 0.957 | 0.995 | 0.981 | 0.969 / 0.925 | 0.974 / 0.925 |
| lib | 91 | 0.944 | 0.838 | 1.057 | 0.953 | 0.892 / 0.893 | 0.884 / 0.886 |
| apps | 5 | 1.246 | 1.091 | 1.405 | 1.214 | 0.991 / 0.995 | 0.999 / 0.994 |
| real | 5 | 1.242 | 1.095 | 1.288 | 1.148 | 0.958 / 0.947 | 0.952 / 0.945 |
| real_dis (4) | 4 | 1.175 | 0.995 | 1.213 | 1.034 | 0.981 / 0.969 | 0.967 / 0.960 |

corpus2 dev/heldout rows include the real_dis ports (tables.md also lists them excluding real_dis). The lib set carries the gain (about -11% cycles); jit-trace -3% cycles / -7.5% instructions; programs and real -4 to -5%; awfy, apps, corpus -1 to -2%.

## Top-15 gaps to LuaJIT (PGO, pass B; before in parentheses)
1. classic_vector 4.33 (4.37)
2. programs/fib 3.12 (3.14)
3. apps/raytrace 2.97 (3.07)
4. awfy/permute 2.78 (3.69; LuaJIT-side drift, see caveats)
5. lua_lexer 2.58 (2.63)
6. jit-trace/tail_loop 2.48 (2.55)
7. lib/find_plain 2.32 (2.28)
8. lib/char 2.32 (2.30)
9. lib/fmt_s 2.10 (2.45)
10. lib/byte_multi 2.09 (2.38)
11. awfy/cd 2.09 (2.17)
12. class_middleclass 2.08 (2.14)
13. lib/rep_sep 2.07 (2.04)
14. real/lua_interp 2.05 (2.04)
15. awfy/towers 2.05 (2.05)

Plain build worst: classic_vector 4.79, raytrace 3.54, lib/m_random 3.19 (6.4x LuaJIT instructions), fib 3.12, lua_lexer 2.88. Largest absolute PGO gaps: fib 997 Mcyc, text_adventure 973, short_inner 890, classic_vector 807, lua_lexer 713, type_entry 693, type_entry_stable 664.

## Top improvements vs 79e7441c (same session, plain cycles, programs >= 30 Mcyc; instruction ratio in parentheses)
lib/lower 0.396 (0.489), lib/upper 0.400 (0.489), lib/reverse 0.401 (0.473), lib/m_random_f 0.483 (0.489), lib/type_call 0.576 (0.601), lib/sub 0.604 (0.680), lib/str_eq_dyn 0.654 (0.671), lib/char 0.703 (0.550), lib/str_lt 0.704 (0.781), lib/m_type 0.725 (0.735), lib/tostring_tbl 0.729 (0.827), lib/sub_1char 0.751 (0.821), lib/it_next_empty 0.762 (0.771).

Not counted (noisy; duplicate column differs): programs/table_hash 0.541 (dup 0.912, PGO 0.784), jit-trace/int_rec2 0.728 (dup 0.960). lib/char is plain-only (PGO 0.987, layout). Extra PGO winners: fmt_q 0.680, m_random 0.683, gsub_str 0.711, str_lt_short 0.731, it_next 0.760. Instructions agree with cycles on the real ones: these are Bundle H (libcall, tnextinl) and Bundle J/M3 (string hash) gains.

## Regressions vs 79e7441c
No set regressed. 15 programs are >3% worse in plain or PGO cycles in pass A; all 15 re-run at 15 runs serial on CPU 4. 15-run cycle ratios (instruction ratio where relevant):

- **lib/m_floor (real):** plain 1.128, PGO 1.133, instructions 0.96. Slower in both builds despite 4% fewer instructions: cycles per instruction went up. Needs a look.
- **lib/str_idx_concat_key (real):** plain 1.040, PGO 1.046, instructions 0.96. Same pattern as m_floor.
- **corpus2/dev/memo_cache (probably real):** plain 1.032, PGO 1.046, instructions 0.99. 1802 Mcyc program; cause unverified (M3 reduced the dual-hash cost; this is vs the pre-J baseline).
- **lib/tsort_cmp (small, real):** plain 1.014, PGO 1.030, instructions +1.7%.
- **jit-trace/alloc_ctor:** PGO 1.112, plain 1.011, instructions +0.8%; PGO layout/profile, reproduces at 15 runs.
- **lib/find_pat:** PGO 1.050, plain 1.014, equal instructions (PGO layout).
- **lib/setmt:** PGO 1.035, plain 1.020, equal instructions (PGO layout).
- **Plain-only layout cases:** lib/tonumber_flt 1.087, lib/tonumber_int 1.073, corpus2/dev/diff_levenshtein 1.038, lib/fmt_g 1.037.
- **Noise level:** lib/co_yield 1.021/1.036, lib/it_ipairs 1.019/1.014, corpus2/dev/expr_compiler 1.010/1.028, lib/os_clock 1.013/0.992.

The previous session's regression cluster (m_abs, m_min, m_max, sel_count, inline_leaf, int_sum, type_entry_stable) is gone; lib/m_type is now 0.725.

## Caveats
- LuaJIT's own geomean is stable across sessions (1.003), but two programs moved a lot on the LuaJIT side: awfy/permute is 34% slower and apps/events 40% slower than last session. Ours is unchanged on both (1.008, 1.007 vs the rebuilt 79e7441c). The permute gap (3.69 -> 2.78) and part of the apps gain are LuaJIT drift.
- For apps and awfy use the rebuilt 79e7441c/LuaJIT ratios as the honest baseline: apps 1.259 plain / 1.097 PGO (real change about 0), awfy 1.440 / 1.349 (about -1%).
- Noisy single-run ratios (duplicate columns differ by >3%) in pass A: table_hash, int_rec2, sel_count, str_eq_long, fmt_s.
- Programs without a usable LuaJIT variant are excluded from LuaJIT geomeans, as before: output differs from PUC (corpus/testrunner_luaunit, corpus2/dev/real_dis_arm, programs/coroutines, programs/load_big, programs/oop_methods); no LuaJIT equivalent (lib m_tointeger, m_type, m_ult, utf8_len).
- Layout-lottery cases (equal instructions, one build only) are flagged "layout?" in `tables.md`.
- The script path is the runner's worktree `bench/`, not the main checkout; the 79e7441c rebuild matches last session within 0.4%, so no visible path effect.

## What changed since 79e7441c
Instruction-regression fix, knob pruning K1-K3b, Bundle G (chaindepth, pcallerr), H (libcall=7, tnextinl=1), J (dual string hash LAZY_HASH, centry=2, centrywait 8), L (bhoist=1 via M2, aidx off), M3 (string tag in hnext). lazyci stays off; Bundle N stopped at design (no code).
