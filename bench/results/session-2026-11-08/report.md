# Full benchmark of main 8965e73e vs c276c376 and cb1af288 (2026-11-08)

Data and full tables are in this directory (`tables.md`, `per_program_passA.csv`, `per_program_passB.csv`, `passA_*.json`, `passB_*.json`, `rerun_*`).
Drivers: `scripts/bench_session2_report.py`, `scripts/bench_session2_rerun.py`. Prose below is the measurement builder's analysis (condensed; per-program tables for
programs, jit-trace, lib and apps are only in the CSVs).

Merged since the previous full benchmark (c276c376): laprune, tier-1 inline built-ins (t1inline default 1023), BUNDLE-LIB (format / number formatting / tostring),
C36 (cispare, domtop=82, second=3), P-TINS (table insert path). cb1af288 is the start of the session.

## Method
- 223 programs in 9 sets; `perf stat` cycles:u and instructions:u, min of 5 interleaved rotated runs; `RUSTYNAIL_SEED=0`; stdin /dev/null; 8 GB cap.
- Env length equalised across ours columns (no trace-limits env). Each commit built from `git archive` into its own target with sources touched first.
- Columns: PUC 5.5, LuaJIT 2.1, main plain, duplicate plain (noise), main `RUSTYNAIL_JIT=off` (verified: class_middleclass 3431 Mcyc off vs 1012 on),
  main PGO, c276c376 plain+PGO, cb1af288 plain+PGO. PGO builds via `scripts/pgo-build.sh` (181 training runs, 0 failed) for all three commits.
- Pass A: CPUs 3-7 (mixed cores). Pass B: CPUs 4 and 6 (fast Zen5) for the LuaJIT comparison. Serial re-run: CPU 4, 9 runs, 9 programs.
- Every ours column matches `.expected` and PUC output on all 223 programs in both passes. Duplicate column 0.989-1.005 per set.
- LuaJIT skipped for 4 lib programs with no LuaJIT equivalent (m_type, m_tointeger, m_ult, utf8_len): LuaJIT ratios cover 219 programs.

## Headline, pass A (geomean of per-program cycle ratios; below 1 = ours faster)

| set | n | plain/PUC | pgo/PUC | plain/interp | plain/LJ | pgo/LJ | plain vs c276 | pgo vs c276 | plain vs cb1 | pgo vs cb1 |
|---|---|---|---|---|---|---|---|---|---|---|
| corpus (old) | 18 | 0.542 | 0.478 | 0.677 | 1.347 | 1.187 | 0.959 | 0.967 | 0.948 | 0.960 |
| corpus2 dev | 19 | 0.505 | 0.444 | 0.630 | 1.417 | 1.245 | 0.972 | 0.985 | 0.873 | 0.879 |
| corpus2 heldout | 13 | 0.471 | 0.431 | 0.542 | 1.387 | 1.268 | 0.949 | 0.975 | 0.876 | 0.886 |
| awfy | 14 | 0.281 | 0.265 | 0.299 | 1.486 | 1.403 | 0.991 | 0.987 | 0.999 | 0.997 |
| programs | 25 | 0.233 | 0.215 | 0.310 | 0.989 | 0.915 | 0.985 | 0.996 | 0.983 | 0.997 |
| jit-trace | 29 | 0.081 | 0.080 | 0.088 | 1.026 | 1.017 | 0.997 | 0.999 | 0.998 | 1.000 |
| lib | 95 | 0.280 | 0.254 | 0.404 | 1.157 (91) | 1.044 | 0.966 | 0.967 | 0.966 | 0.967 |
| apps | 5 | 0.579 | 0.508 | 0.784 | 1.378 | 1.210 | 0.992 | 0.967 | 0.994 | 0.968 |
| real (bench/real) | 5 | 0.509 | 0.445 | 0.600 | 1.380 | 1.208 | 0.988 | 0.974 | 1.001 | 1.001 |
| real_dis dev | 2 | 0.652 | 0.563 | 0.879 | 1.259 | 1.087 | 0.942 | 0.939 | 0.956 | 0.952 |
| real_dis heldout | 2 | 0.663 | 0.567 | 0.888 | 1.422 | 1.215 | 0.881 | 0.916 | 0.892 | 0.910 |
| real_dis all | 4 | 0.658 | 0.565 | 0.883 | 1.338 | 1.149 | 0.911 | 0.928 | 0.924 | 0.931 |
| real + real_dis | 9 | 0.570 | 0.495 | 0.713 | 1.361 | 1.181 | 0.953 | 0.953 | 0.966 | 0.970 |
| ALL | 223 | 0.275 | 0.252 | 0.356 | 1.193 (219) | 1.091 | 0.974 | 0.978 | 0.960 | 0.964 |

## Headline, pass B (fast cores only, ours/LuaJIT)

| set | n | plain/LJ | pgo/LJ | c276/LJ | c276pgo/LJ | cb1/LJ | cb1pgo/LJ |
|---|---|---|---|---|---|---|---|
| corpus (old) | 18 | 1.352 | 1.192 | 1.416 | 1.236 | 1.429 | 1.242 |
| corpus2 dev | 19 | 1.419 | 1.240 | 1.449 | 1.263 | 1.624 | 1.413 |
| corpus2 heldout | 13 | 1.408 | 1.274 | 1.469 | 1.326 | 1.597 | 1.445 |
| awfy | 14 | 1.410 | 1.330 | 1.417 | 1.335 | 1.408 | 1.332 |
| programs | 25 | 0.979 | 0.896 | 0.987 | 0.901 | 0.982 | 0.903 |
| jit-trace | 29 | 1.021 | 1.013 | 1.025 | 1.013 | 1.024 | 1.013 |
| lib | 91 | 1.157 | 1.045 | 1.198 | 1.079 | 1.198 | 1.081 |
| apps | 5 | 1.392 | 1.223 | 1.402 | 1.270 | 1.398 | 1.256 |
| real | 5 | 1.368 | 1.202 | 1.389 | 1.228 | 1.367 | 1.199 |
| real_dis dev | 2 | 1.277 | 1.096 | 1.349 | 1.171 | 1.339 | 1.154 |
| real_dis heldout | 2 | 1.423 | 1.212 | 1.607 | 1.327 | 1.594 | 1.331 |
| real_dis all | 4 | 1.348 | 1.152 | 1.473 | 1.246 | 1.461 | 1.240 |
| real + real_dis | 9 | 1.359 | 1.180 | 1.426 | 1.236 | 1.408 | 1.217 |
| ALL | 219 | 1.189 | 1.085 | 1.219 | 1.110 | 1.236 | 1.127 |

### Reference points (plain, ours/LuaJIT, fast cores)

| set | cb1af288 | c276c376 | main 8965e73e |
|---|---|---|---|
| corpus2 dev | 1.63 | 1.450 | **1.419** |
| corpus2 heldout | 1.585 | 1.472 | **1.408** |
| old corpus | 1.414 | 1.398 | **1.352** |

Re-measured in this run: cb1 1.624 / 1.597 / 1.429, c276 1.449 / 1.469 / 1.416 (agree within ~1%). Since c276c376 main gained ~2% on dev, 4% on heldout, 4.5% on the
old corpus, 3% over all 219 programs; lib gained 3.3% (fmt, tostring and table paths). awfy, programs, jit-trace are flat.

## Real programs in detail (pass A, Mcyc; ratios are plain over that baseline)

| program | plain | pgo | interp | PUC | LJ | plain/PUC | plain/LJ | pgo/LJ | vs c276 | pgo vs c276 | vs cb1 | pgo vs cb1 | instr vs c276 |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| real/game_loop | 35 | 34 | 211 | 212 | 22 | 0.166 | 1.577 | 1.522 | 0.988 | 0.975 | 0.981 | 0.973 | 0.977 |
| real/json_roundtrip | 574 | 475 | 642 | 797 | 470 | 0.721 | 1.222 | 1.011 | 0.971 | 0.985 | 0.968 | 0.987 | 0.980 |
| real/lua_interp | 129 | 116 | 247 | 223 | 59 | 0.577 | 2.175 | 1.957 | 1.021 | 0.990 | 1.085 | 1.055 | 0.987 |
| real/template_engine | 839 | 673 | 790 | 1229 | 942 | 0.683 | 0.891 | 0.714 | 0.976 | 0.915 | 0.987 | 0.987 | 0.968 |
| real/wordfreq | 154 | 137 | 164 | 213 | 115 | 0.722 | 1.343 | 1.196 | 0.983 | 1.009 | 0.987 | 1.008 | 0.988 |
| dev/real_dis_arm | 2974 | 2546 | 3380 | 4507 | 2051 | 0.660 | 1.450 | 1.241 | 0.901 | 0.908 | 0.919 | 0.914 | 0.897 |
| dev/real_dis_x86 | 1749 | 1524 | 1993 | 2713 | 1601 | 0.645 | 1.093 | 0.952 | 0.985 | 0.972 | 0.994 | 0.992 | 0.975 |
| heldout/real_dis_mips | 2502 | 2133 | 2913 | 3789 | 1801 | 0.660 | 1.389 | 1.184 | 0.871 | 0.897 | 0.895 | 0.907 | 0.893 |
| heldout/real_dis_ppc | 2643 | 2264 | 2880 | 3965 | 1816 | 0.666 | 1.455 | 1.247 | 0.890 | 0.936 | 0.890 | 0.914 | 0.912 |

Fast-core pass B (plain/LJ, pgo/LJ, plain vs c276): real_dis_arm 1.506, 1.283, 0.906; real_dis_x86 1.082, 0.935, 0.988; real_dis_mips 1.400, 1.194, 0.874;
real_dis_ppc 1.445, 1.230, 0.896; game_loop 1.541, 1.518, 0.977; json_roundtrip 1.237, 1.018, 0.984; lua_interp 2.164, 1.965, 1.007; template_engine 0.883, 0.713, 0.966;
wordfreq 1.313, 1.159, 0.988.

- The disassembler ports gained 10-13% (arm, mips, ppc) or 1.5% (x86) vs c276c376 (instruction ratios 0.89-0.98): the BUNDLE-LIB / t1inline / laprune stretch.
- The `real` set is only ~1-3% faster; it is flat vs cb1 (1.001). real/lua_interp is 8.5% slower than cb1 with instructions 3% lower (129 Mcyc, known env-length swing; stable on serial re-run).
- LuaJIT's real_dis_arm output differs from PUC's (documented `#-0` lines): its time is not strictly comparable.

## Top winners vs c276c376 (plain, pass A)
lib/fmt_x_pad 0.544, lib/tostring_tbl 0.578, lib/fmt_mixed 0.582, lib/fmt_g 0.628, lib/tostring_flt 0.685, lib/fmt_f 0.694 (instructions fell 28-49%: BUNDLE-LIB; small 70-150 Mcyc programs),
corpus/serpent_ser 0.817, lib/len_tbl 0.829, programs/table_hash 0.839 (layout noise: instructions 0.973, serial 0.995, dup 1.151), programs/string_ops 0.856.
Also real_dis_mips 0.871, scheme_interp 0.874, real_dis_ppc 0.890, real_dis_arm 0.901, corpus/template_etlua 0.870.

## Top winners vs cb1af288 (plain, pass A)
iter_pipeline 0.115, stream_closures 0.499, lib/fmt_x_pad 0.555, lib/tostring_tbl 0.575, lib/fmt_mixed 0.597, lib/fmt_g 0.642, scheme_interp 0.683, lib/tostring_flt 0.687,
lib/fmt_f 0.693, markov_text 0.780.

## Losers
vs c276c376 (plain): lib/load_small 1.107 (instr 1.002), lib/fmt_q 1.037, lib/find_plain 1.033, lib/gsub_fn 1.025, lib/find_cap 1.024, programs/matmul 1.022, programs/heapsort 1.022,
real/lua_interp 1.021, programs/pcall_error 1.020. Only load_small is over 5% and its instruction count is equal; the rest are known layout-noise.
vs cb1af288 (plain): real/lua_interp 1.085, lib/load_small 1.066, lib/co_yield 1.042, lib/find_pat 1.040, lib/setmt 1.037, lib/gsub_fn 1.036, programs/coroutines 1.034, lib/find_plain 1.032,
heldout/vending_fsm 1.032, lib/match 1.028.

## Regressions > 5% (serial CPU 4 x9 re-run, instruction ratios)

| program | verdict | evidence |
|---|---|---|
| lib/load_small | plain +11% vs c276, +7% vs cb1; PGO flat | persistent in passes A, B and serial; instr 1.002: plain-build code layout (load/parse path); not investigated; ~250 Mcyc |
| lib/co_wrap | PGO +15% vs both baselines; plain flat | serial 1.151/1.149; instr 1.002; c276/cb1 PGO ~200 Mcyc vs main PGO 230: PGO layout/profile effect, reproducible |
| lib/pcall_err | PGO +8.7% vs c276, +10% vs cb1; plain flat | instr +1.6-2.2%: small real instruction increase plus layout |
| corpus2/dev/regex_nfa | PGO +5.3% vs c276, +3.9% vs cb1; plain flat | instr 0.992: PGO layout |
| programs/coroutines | PGO +3.4% serial (A and B 5.2-5.5%); plain +1.3% | instr 1.000: layout |
| real/lua_interp | +8.5% vs cb1 (plain), PGO +5.5%; +1.6% vs c276 | instr 0.968-0.970: known env-length swing, 128 Mcyc |
| corpus2/heldout/prototype_objects | pass A PGO +15.9% | gone serially (0.985); noise |
| programs/table_hash | pass A PGO +8% (dup 1.151) | gone serially (0.99); noise |
| corpus2/dev/csv_report | none | serial 0.938 vs c276: win or neutral |

No program has a real instruction-count regression beyond +3% (largest: lib/fmt_q +3.1%, lib/pcall_err +1.6-2.2%).

## Noise caveats
- Pass A mixes fast and slow cores; use pass B for the LuaJIT comparison.
- Programs under ~30 ms / ~100 Mcyc are noisy; the lib set is mostly 70-270 Mcyc so single-program lib ratios have a 2-4% layout floor.
- Duplicate column swung >3% on 6 programs in pass A: csv_report 0.943, wordfreq_wrap 1.032, log_analyzer 0.968, prototype_objects 0.897, find_plain 1.037, table_hash 1.151.
- PGO is trained on awfy, apps, programs, lib, jit-trace and real: its numbers on those sets are optimistic; corpus and corpus2 PGO numbers are hold-out.

## Not run
5.4 build; GC-stress run; trace-limits env variants; Miri/ASan; serial re-runs of programs that are outliers only in the winning direction (verified by instruction ratios);
a bisect of the lib/load_small plain and lib/co_wrap PGO slowdowns.
