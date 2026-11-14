# Full benchmark (session-bm5): HEAD 9a9b8ed, Lua 5.5 and 5.4, plain and PGO

Data: `tables.md`, `vs_baseline.md`, `extra.md`, `per_program_pass{A,B,54}.csv`, raw JSONs, logs, `build_logs/`, `startup_size.json`, `release_script_check.txt`, `machine_state.txt`.
Method as in session-release: min of 5 interleaved runs, `perf stat cycles:u,instructions:u`, `RUSTYNAIL_SEED=0`, SPIN_MS=300, equal-length binary paths, one script path for every column.
Pass A: 223 programs, CPUs 3-7. Pass B (5.5 vs LuaJIT, 219 programs with a LuaJIT variant) and pass C (5.4, 223 programs, 9 columns): CPUs 4 and 6. Then 15-run serial re-runs on CPU 4 of the 18 programs more than 3% worse than the baseline (5.5) and of the 20 programs with the largest relative 5.4 cost, and the startup/size table.
All our outputs (5.5 and 5.4; plain, PGO, JIT off, nojit build; baseline builds) equal PUC and `.expected` on all 223 programs.
`scripts/release-build.sh` produced all shipped builds (`sha256sum -c` OK in all six dist dirs, stripped). PGO training: 0 failed runs (181 / 179 / 181 runs).
Baseline: 8f422153 is gone (history squashed); the same-session baseline is 8a18eb3 rebuilt, plain and PGO, 5.5. Its tree is not verified byte-identical to 8f422153's.
LuaJIT is 2.1.1794386141 (session-release had 2.1.1793383622).

## Headline: geomean cycles, below 1 means ours is faster

| build | ours/LuaJIT | ours/PUC (same version) | session-release (LJ / PUC) |
|---|---|---|---|
| 5.5 plain | 0.967 | 0.222 | 0.995 / 0.230 |
| 5.5 PGO | 0.881 | 0.202 | 0.905 / 0.210 |
| 5.4 plain | 1.036 | 0.232 | 1.184 / 0.268 |
| 5.4 PGO | 0.949 | 0.213 | 1.081 / 0.245 |

- 5.5 without real and real_dis (210): 0.959 plain, 0.876 PGO.
- Baseline measured this session: plain/LJ 1.001, PGO/LJ 0.912 (pass B).
- vs baseline, pass A (223): plain 0.958 cycles / 0.959 instructions, PGO 0.959 / 0.966. Plain: 31 programs >3% faster, 178 within 3%, 14 >3% slower. PGO: 30 / 184 / 9.
- JIT gain (plain/JIT-off): 0.301 on 5.5, 0.312 on 5.4.
- 5.4 relative cost 1.048x (session-release 1.157).

## vs previous (baseline 8a18eb3, pass A; `vs_baseline.md`)
Improvements (plain/baseline cycles; instructions move with them): lib/m_type 0.19, m_random 0.34, m_random_f 0.38, it_pairs 0.40, it_next 0.40, it_next_empty 0.41, jit-trace/tail_loop 0.46, lib/it_pairs_hash 0.48, jit-trace/tfor_loops 0.51, rec_loop 0.67, awfy/permute 0.88, oop_mixins 0.89, raytrace 0.92, classic_vector 0.94. PGO adds lib/char 0.52 (session-release's PGO anomaly, 263 -> 139 Mcyc, fixed by 0993a47).
By set (plain cycles / instructions): jit-trace 0.937 / 0.961, lib 0.925 / 0.931, awfy 0.991 / 0.977, corpus 0.987 / 0.986, corpus2 dev 0.992 / 0.989, heldout 0.998 / 0.990, programs 1.003 / 0.983, real 0.998 / 0.991, apps 1.036 / 1.013.

Regressions (15-run values):

| program | plain cyc | plain ins | PGO cyc | PGO ins | verdict |
|---|---|---|---|---|---|
| apps/expr_lang | 1.280 | 1.134 | 1.256 | 1.128 | real, +13% instructions, not bisected |
| corpus2/heldout/stream_closures | 1.067 | 0.963 | 1.060 | 0.951 | IPC drop, not diagnosed |
| lib/fmt_q | 1.137 | 1.000 | 0.973 | 1.000 | layout lottery |
| programs/coroutines | 1.073 | 1.000 | 0.947 | 0.990 | layout lottery |
| lib/str_lt_short | 1.078 | 1.000 | 1.089 | 1.000 | layout lottery (both builds) |
| jit-trace/alloc_ctor | 1.066 | 1.000 | 0.889 | 0.989 | layout lottery |
| lib/sel_n | 1.049 | 0.999 | 1.065 | 0.999 | layout lottery |
| lib/load_small | 0.965 | 0.985 | 1.032 | 0.982 | PGO +3%, plain faster |
| awfy/cd | 1.033 | 1.040 | 1.026 | 1.047 | slightly real (+3.8% in session-release too) |
| lib/co_yield | 1.027 | 1.000 | 0.998 | 1.000 | marginal |

Pass A flagged these but the 15-run re-run put them within 3% (0.987-1.031): str_method_len, load_big, table_hash, string_ops, tonumber_num, tinsert_idx, m_tointeger, prototype_objects.

## PGO A/B (5.5, pass A): PGO/plain 0.913 cycles, 0.925 instructions
131 programs >3% faster, 88 within 3%, 4 >3% slower (session-release 135 / 81 / 7). Median 0.942, p10 0.788, p90 1.002.

| set | n | cyc | ins | trained |
|---|---|---|---|---|
| apps | 5 | 0.871 | 0.916 | yes |
| awfy | 14 | 0.940 | 0.948 | yes |
| corpus | 18 | 0.876 | 0.924 | held out |
| corpus2 dev | 19 | 0.876 | 0.904 | held out |
| corpus2 heldout | 13 | 0.912 | 0.922 | held out |
| jit-trace | 29 | 0.987 | 0.989 | yes |
| lib | 95 | 0.905 | 0.913 | yes |
| programs | 25 | 0.913 | 0.915 | yes |
| real | 5 | 0.879 | 0.907 | yes |
| real_dis | 4 | 0.869 | 0.893 | no |
| ALL | 223 | 0.913 | 0.925 | - |

- Held-out sets gain as much as trained ones, and instructions fall too, so it is not only layout.
- The four >3% slower: lib/m_ult 1.040 (instructions 1.048), lib/m_tointeger 1.040 (15 runs: 0.973, noise), knucleotide 1.036 (instructions 0.919), lib/os_clock 1.033 (syscall). lib/char is no longer a PGO regression.
- PGO costs 365 KB of stripped binary (3.34 vs 2.97 MB) and 0.8 MB startup RSS. Startup time is equal (0.503 vs 0.501 ms).

## 5.5 vs LuaJIT and PUC (pass B)

| set | n | plain/LJ | PGO/LJ | plain/PUC | PGO/PUC |
|---|---|---|---|---|---|
| apps | 5 | 1.399 | 1.223 | 0.533 | 0.466 |
| awfy | 14 | 1.445 | 1.355 | 0.273 | 0.256 |
| corpus | 18 | 1.078 | 0.939 | 0.441 | 0.384 |
| corpus2 dev | 19 | 1.186 | 1.044 | 0.429 | 0.377 |
| corpus2 heldout | 13 | 1.237 | 1.151 | 0.422 | 0.393 |
| jit-trace | 29 | 0.886 | 0.871 | 0.070 | 0.069 |
| lib | 91 | 0.836 | 0.753 | 0.200 | 0.181 |
| programs | 25 | 0.896 | 0.822 | 0.213 | 0.196 |
| real | 5 | 1.205 | 1.057 | 0.450 | 0.395 |
| real_dis | 4 | 1.142 | 0.999 | 0.564 | 0.493 |
| ALL | 219 | 0.967 | 0.881 | 0.222 | 0.202 |

Top-15 gaps to LuaJIT (PGO): awfy/permute 3.22, fib 2.94, raytrace 2.76, lua_lexer 2.52, classic_vector 2.31, awfy/cd 2.22, lib/fmt_s 2.14, byte_multi 2.13, stream_closures 2.12, towers 2.01, real/lua_interp 1.87, co_yield 1.77, queens 1.76, type_entry 1.76, lib/getmt 1.74.
Plain worst: raytrace 3.37, permute 3.25 (4.6x LuaJIT instructions), fib 2.93, lua_lexer 2.81, classic_vector 2.65, cd 2.48, stream_closures 2.29, fmt_s 2.29, co_yield 2.27, text_adventure 2.18.
Biggest wins vs LuaJIT (PGO): sel_loop 0.044, error_tbl 0.068, pcall_err 0.128, int_rec2 0.135, pcall_error 0.148, tfront_1k 0.188, tonumber_flt 0.217, it_next 0.245, lib/match 0.272, sha2_base64 0.286.
Against PUC 5.5, plain is faster on every program. PGO lib/os_clock is 1.013x PUC (plain 0.987, syscall-bound). Next closest: tsort_cmp 0.767, expr_lang 0.697, m_fmod 0.696.

## Lua 5.4 (pass C)
rel = (p54/PUC54)/(p55/PUC55), the 5.4 relative cost.

| set | n | p54/PUC54 | g54/PUC54 | p54/LJ | g54/LJ | rel p54 | rel g54 |
|---|---|---|---|---|---|---|---|
| apps | 5 | 0.545 | 0.476 | 1.328 | 1.159 | 1.015 | 1.017 |
| awfy | 14 | 0.280 | 0.263 | 1.488 | 1.398 | 1.025 | 1.028 |
| corpus | 18 | 0.456 | 0.398 | 1.167 | 1.016 | 1.044 | 1.039 |
| corpus2 dev | 19 | 0.453 | 0.397 | 1.316 | 1.152 | 1.069 | 1.070 |
| corpus2 heldout | 13 | 0.447 | 0.408 | 1.365 | 1.247 | 1.074 | 1.081 |
| jit-trace | 29 | 0.071 | 0.070 | 0.960 | 0.948 | 1.020 | 1.024 |
| lib | 95 | 0.211 | 0.193 | 0.893 | 0.811 | 1.055 | 1.064 |
| programs | 25 | 0.226 | 0.210 | 0.967 | 0.898 | 1.048 | 1.058 |
| real | 5 | 0.465 | 0.406 | 1.261 | 1.102 | 1.033 | 1.028 |
| real_dis | 4 | 0.573 | 0.500 | 1.168 | 1.018 | 1.027 | 1.031 |
| ALL | 223 | 0.232 | 0.213 | 1.036 | 0.949 | 1.048 | 1.053 |

5.4 relative cost, session-release to now: rawget_set 11.5 to 0.95, it_next 2.2 to 0.95, tfront_1k 8.6 to 1.09, class_middleclass 6.5 to 1.04, metamethods 6.1 to 1.17, sieve 3.0 to 0.64, table_array 2.1 to 1.11, tmove 2.7 to 1.21, tunpack 2.3 to 1.23, len_tbl 1.8 to 1.31, tinsert_append 1.5 to 1.13, int_rec2 3.4 to 2.11. Unchanged: tpack 1.46, array_rec 2.43, load_big 2.45 (largest remaining, p54/PUC54 1.03). Remaining top list: load_big 2.45, array_rec 2.43, concat_op_str 2.21, int_rec2 2.11, heapsort 1.83, regex_nfa 1.80, tpack 1.46, awfy/storage 1.42, rep_sep 1.40, str_eq_dyn 1.40. The causes (commits of 2026-11-12) are inferred from commit titles, not re-verified in the sources. Our 5.4 costs 1.071x our 5.5 in cycles (PUC 5.4 vs 5.5: 1.023x).

## Startup, size, memory

| artifact | shipped (stripped) | unstripped |
|---|---|---|
| 5.5 plain rnlua | 2.97 MB | 19.1 MB |
| 5.5 PGO rnlua | 3.34 MB | 21.1 MB |
| 5.4 plain / PGO | 2.79 / 3.12 MB | 17.9 / 19.8 MB |
| nojit (5.5 / 5.4) | 1.21 / 1.19 MB (stripped copy) | 7.0 / 7.0 MB |
| rnluac 5.5 / 5.4 | 555 / 534 KB | |
| PUC 5.5 / 5.4 | 341 / 328 KB | |

The release script now ships stripped files, so the benchmarked binaries are the shipped ones (session-release timed unstripped ones; speed is unaffected).

| build | empty cycles (k) | wall min (ms, hyperfine) | VmHWM empty |
|---|---|---|---|
| PUC 5.5 | 760 | 0.326 | 2.4 MB |
| LuaJIT | 821 | 0.311 | 2.9 MB |
| 5.5 plain | 977 | 0.501 | 4.3 MB |
| 5.5 PGO | 1013 | 0.503 | 5.1 MB |
| 5.5 JIT off | 981 | 0.490 | 4.3 MB |
| 5.5 nojit build | 931 | 0.474 | 3.8 MB |
| 5.4 plain | 1011 | 0.493 | 4.2 MB |

- We start about 175 us slower than PUC and 190 us slower than LuaJIT (session-release: 150 / 110). The JIT itself costs about 10 us.
- Tier-1 compile of a 1000-iteration loop: +412 k cycles (about 0.1 ms).
- `collectgarbage("count")` of an empty state: 29.3 KB (PUC 20.2, LuaJIT 48.2).

## Caveats
- The baseline is 8a18eb3, the squashed equivalent of 8f422153. Cross-session numbers are indicative only, and LuaJIT was updated between sessions.
- The machine was not fully idle: several idle readings were above 2% at pass boundaries (cpu12 up to 15%), and the 5.4 re-run started with 3-10% on all benchmark CPUs (desktop browser activity). See `machine_state.txt`. Pass A and B starts were mostly clean; no other jobs of ours ran during passes.
- Pass A duplicate-binary noise (plain/plain2): geomean 1.000, extremes 0.924 (programs/heapsort) to 1.063 (lib/sel_n).
- The first JIT-off hyperfine run was an anomaly (0.352 ms) and was re-measured (0.490 ms).
- The 5.4 pass has no duplicate column. The 15-run re-runs show p54/p54b of 0.967-1.024.
- The 5.4 vs 5.5 JIT-gain comparison mixes pass C and pass A.
- Not diagnosed: apps/expr_lang (+28% cycles, +13% instructions vs the baseline), stream_closures (IPC), the remaining 5.4 gaps (load_big, array_rec, concat_op_str).
- Machine: AMD Ryzen AI 7 350, governor powersave, boost on.
