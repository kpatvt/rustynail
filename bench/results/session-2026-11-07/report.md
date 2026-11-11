# Full benchmark of main c276c376 vs cb1af288, PUC 5.5 and LuaJIT 2.1

Raw data and generated tables are in this directory (`tables.md`, `per_program_passA.csv`, `passA_*.json`, `passB_*.json`, `rerun_*`).
Drivers: `scripts/bench_session.py`, `scripts/bench_session_report.py`, `scripts/bench_session_rerun.py`.
This file is the prose analysis returned by the measurement builder (the per-program rows for programs, jit-trace, lib and apps are only in the CSV).

What main contains relative to cb1af288: gforlua (closure for-in iterators), loopany on with the loopanymax guard, a co_resume glue fix,
sinkmt/sinkstub/sinkroot=2 with sunk frame side traces, DYNMASK (dynamic-key run-time hash mask).

## Method
- 223 programs: corpus 18, dev 19, heldout 13, awfy 14, programs 25, jit-trace 29, lib 95, apps 5, real 5.
- `perf stat` cycles:u and instructions:u, min of 5 runs. Columns interleaved with the order rotated per run. `RUSTYNAIL_SEED=0`, stdin `/dev/null`,
  whole run under an 8 GB memory cap. Each worker pinned to one core and ran all columns of its program there. Environment length equalised across ours columns.
- **Pass A:** CPUs 3-7 (5 workers). Columns: PUC, LuaJIT, main plain (default release build with the `jit` feature), duplicate plain (noise column),
  main with `RUSTYNAIL_JIT=off` (interpreter), main PGO (`scripts/pgo-build.sh`, 181 training runs, 0 failed), cb1af288 plain and cb1af288 PGO.
- **Pass B:** CPUs 4 and 6 only (fast Zen5, the corpus2-study protocol), for the LuaJIT comparison.
- **Re-run:** 24 programs with a >5% outlier in either pass or the duplicate column were re-run serially on CPU 4, 9 runs each.
- Every ours column matches `.expected` where one exists, and PUC's output everywhere: 0 differences.
- LuaJIT skipped for 4 programs with no LuaJIT equivalent (lib m_type, m_tointeger, m_ult, utf8_len), so "with LuaJIT" is 219.

## Headline, pass A (geomean of per-program cycle ratios; below 1 means ours is faster)
"vs cb1" is main over cb1af288, below 1 = faster than before the session.

| set | n | plain/PUC | pgo/PUC | plain/interp | plain/LJ | pgo/LJ | plain vs cb1 | pgo vs cb1 PGO |
|---|---|---|---|---|---|---|---|---|
| corpus | 18 | 0.569 | 0.497 | 0.689 | 1.421 | 1.242 | 0.986 | 0.990 |
| corpus2 dev | 19 | 0.517 | 0.448 | 0.640 | 1.452 | 1.259 | **0.893** | **0.892** |
| corpus2 heldout | 13 | 0.493 | 0.440 | 0.552 | 1.457 | 1.302 | **0.918** | **0.900** |
| awfy | 14 | 0.283 | 0.265 | 0.300 | 1.421 | 1.330 | 1.014 | 1.002 |
| programs | 25 | 0.239 | 0.221 | 0.310 | 1.006 | 0.928 | 0.995 | 1.002 |
| jit-trace | 29 | 0.080 | 0.080 | 0.086 | 1.027 | 1.018 | 1.000 | 1.000 |
| lib | 95 | 0.288 | 0.261 | 0.406 | 1.198 | 1.077 | 1.000 | 0.998 |
| apps | 5 | 0.586 | 0.522 | 0.791 | 1.440 | 1.282 | 1.005 | 1.002 |
| real | 5 | 0.516 | 0.454 | 0.592 | 1.410 | 1.241 | 1.012 | 1.010 |
| real_dis dev | 2 | 0.698 | 0.603 | 0.887 | 1.329 | 1.148 | 1.011 | 1.016 |
| real_dis heldout | 2 | 0.755 | 0.631 | 0.916 | 1.613 | 1.348 | 1.012 | 0.987 |
| real + real_dis | 9 | 0.601 | 0.520 | 0.714 | 1.434 | 1.242 | 1.011 | 1.006 |
| ALL | 223 | 0.282 | 0.257 | 0.357 | 1.223 | 1.113 | 0.985 | 0.983 |

The duplicate column (same binary twice) is 1.000 (+-0.003) in every set. Pass A mixes fast and slow cores across programs, so use pass B for LuaJIT.

## ours/LuaJIT on fast cores (pass B)

| set | n | plain/LJ | pgo/LJ | cb1 plain/LJ | cb1 PGO/LJ |
|---|---|---|---|---|---|
| corpus | 18 | 1.398 | 1.224 | 1.414 | 1.233 |
| corpus2 dev | 19 | 1.450 | 1.257 | 1.630 | 1.411 |
| corpus2 heldout | 13 | 1.472 | 1.316 | 1.585 | 1.448 |
| awfy | 14 | 1.210 | 1.144 | 1.202 | 1.135 |
| programs | 25 | 0.989 | 0.907 | 1.000 | 0.904 |
| jit-trace | 29 | 1.030 | 1.018 | 1.028 | 1.018 |
| lib | 91 | 1.196 | 1.076 | 1.195 | 1.078 |
| apps | 5 | 1.408 | 1.260 | 1.401 | 1.249 |
| real | 5 | 1.420 | 1.244 | 1.396 | 1.229 |
| real_dis dev | 2 | 1.333 | 1.144 | 1.328 | 1.140 |
| real_dis heldout | 2 | 1.620 | 1.358 | 1.606 | 1.368 |
| real + real_dis | 9 | 1.441 | 1.245 | 1.424 | 1.238 |
| ALL | 219 | 1.207 | 1.098 | 1.225 | 1.115 |

The 173 programs of our own sets (169 with LuaJIT) are 1.146 plain and 1.054 PGO against LuaJIT; against PUC 0.233 plain and 0.214 PGO.
cb1 was 1.145 plain and 1.053 PGO, so these sets did not change.

## Comparison to earlier reference points (ours/LuaJIT, fast cores, plain)

| set | cb1af288 study | cb1af288 here | a185b431 mid-study | main now | main PGO |
|---|---|---|---|---|---|
| corpus2 dev | 1.624 | 1.630 | 1.439 | 1.450 | 1.257 |
| corpus2 heldout | 1.595 | 1.585 | 1.531 | 1.472 | 1.316 |
| old corpus | 1.494 | 1.414 | 1.396 | 1.398 | 1.224 |

- The cb1 column reproduces the study's dev and heldout numbers. Over the session dev improved 11% (1.63 to 1.45), heldout 7%.
- Since a185b431 dev is flat (within noise), heldout improved 1.531 to 1.472, the old corpus did not move.
- The study's 1.494 for the old corpus is not reproduced: cb1 measures 1.414 here. Cause not found.
- Our own sets are unchanged (0.99 to 1.01).
- PGO adds a clear step everywhere: ours/PUC 0.282 -> 0.257 overall, ours/LuaJIT 1.207 -> 1.098.
- The JIT is worth about 1.45x on corpus, 1.56x on dev, 1.8x on heldout, 3.3x on awfy, 12x on jit-trace and 2.5x on lib. On real_dis only 1.1 to 1.15x.

## Real programs (pass A, Mcyc min)

| program | plain | pgo | interp | PUC | LJ | plain/PUC | plain/interp | plain/LJ | pgo/LJ | plain vs cb1 | pgo vs cb1 PGO | core |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| real/game_loop | 36 | 34 | 211 | 214 | 22 | 0.167 | 0.170 | 1.607 | 1.543 | 0.999 | 0.984 | 4 |
| real/json_roundtrip | 586 | 491 | 636 | 793 | 463 | 0.739 | 0.922 | 1.267 | 1.062 | 0.981 | 1.005 | 6 |
| real/lua_interp | 126 | 118 | 269 | 222 | 58 | 0.569 | 0.468 | 2.164 | 2.020 | 1.060 | 1.072 | 4 |
| real/template_engine | 871 | 706 | 832 | 1228 | 943 | 0.709 | 1.047 | 0.923 | 0.748 | 1.020 | 0.992 | 7 |
| real/wordfreq | 157 | 136 | 165 | 213 | 114 | 0.735 | 0.950 | 1.371 | 1.187 | 1.000 | 1.002 | 4 |
| dev/real_dis_arm | 3318 | 2833 | 3649 | 4475 | 2060 | 0.742 | 0.909 | 1.611 | 1.376 | 1.022 | 1.016 | 4 |
| dev/real_dis_x86 | 1780 | 1555 | 2057 | 2710 | 1623 | 0.657 | 0.866 | 1.097 | 0.958 | 1.000 | 1.017 | 6 |
| heldout/real_dis_mips | 2927 | 2442 | 3220 | 3847 | 1802 | 0.761 | 0.909 | 1.624 | 1.355 | 1.031 | 1.017 | 7 |
| heldout/real_dis_ppc | 2986 | 2497 | 3237 | 3987 | 1863 | 0.749 | 0.923 | 1.603 | 1.340 | 0.992 | 0.958 | 3 |

- The real programs are unchanged by the session (+0.6 to +1.2% geomean, inside noise).
- real/lua_interp is the known environment-length program. Tiny (126 Mcyc); its +6% comes with instructions 0.98 of cb1: layout noise.
- real_dis_arm and real_dis_mips are +2 to +3% plain; their PGO builds are within +1.7%.
- real/template_engine beats LuaJIT (0.92 plain, 0.75 PGO). json_roundtrip is 6% from LuaJIT in PGO.
- Real code is the weakest versus PUC: about 0.52 plain and 0.45 PGO, and 0.70 to 0.75 for real_dis.
- real_dis_arm's LuaJIT output differs from PUC (the documented `#-0` lines); its timing is still included.

## Winners and losers vs cb1af288 (plain, confirmed by the serial re-run)

Winners (plain/cb1): iter_pipeline 0.114 (instructions 0.107), stream_closures 0.498 (0.422), scheme_interp 0.785 (0.833), markov_text 0.781 (0.693),
corpus/containers_lru_heap 0.921, json_rxi 0.928, particles_physics 0.906, classic_vector 0.942; lib/fmt_q 0.909 and corpus/template_etlua 0.946 are layout only (instructions equal).
The big wins are closure for-in iterators (iter_pipeline 8.8x, stream_closures 2x) and the dynamic-key work (markov_text, scheme_interp).
In pass A only, programs/table_hash (0.777) and lib/tmove_1k (0.931) looked like wins; the serial re-run gave 1.030 and 0.974 with equal instructions: noise.

Losers (plain/cb1, serial re-run):
- real/lua_interp 1.060: layout noise, instructions 0.98.
- lib/tunpack 1.054: instructions equal, PGO 0.985.
- lib/match 1.095: instructions equal, PGO 0.984.
- **awfy/cd 1.087: real regression** (below).
- heldout/prototype_objects 1.060: instructions equal; pass A gave 0.966 and pass B 1.151: timing noise.
- dev/expr_compiler 1.048: instructions +0.6%.
- dev/wordfreq_wrap 1.047: instructions equal.
- corpus/msgpack_pure 1.042: instructions equal.
- awfy/havlak 1.006 (pass A's 1.083 was noise); programs/heapsort 1.011 (pass A's 1.090 was noise).

**The one real regression above 5% is awfy/cd** (266 Mcyc): +6.8% instructions in plain and +3.4% in PGO; cycles +8.7% plain and +5.9% PGO,
consistent in passes A and B and the serial run. Cause not investigated (expected: the loopany guard, see docs/JIT_PARKED.md A25/BUNDLE-S).
Every other >5% outlier in any pass has equal instruction counts, so it is layout or timing noise.

## Noise caveats
- Pass A programs with a duplicate-column spread above 3%: awfy/havlak 1.060, heldout/prototype_objects 0.958, dev/csv_report 0.949,
  programs/heapsort 1.051, programs/table_hash 0.907, jit-trace/sink_tuple (13 Mcyc) 1.033.
- 20 of 223 programs run under 100 Mcyc (lib 12, awfy 4, jit-trace 3, real/game_loop). Their ratios swing by several percent between runs and cores.
- PGO is trained on awfy, apps, programs, lib, jit-trace and real: its numbers on those sets are optimistic; corpus and corpus2 PGO numbers are hold-out.
- LuaJIT output differs from PUC on corpus/testrunner_luaunit, dev/real_dis_arm, programs/coroutines, programs/load_big and programs/oop_methods; timings still included.
- Desktop session at load about 2, not dedicated. The duplicate column agrees to 1.000, so the geomeans are clean.

## Not run
- No interpreter-only column for cb1.
- No 5.4 build, no warm=0 pass, no startup/RSS/GC-stress measurements, no BOLT build.
- The cb1af288 columns ran the current tree's program files, including corpus2.
