# JIT benchmark tooling and baseline (J0 item "measurement tooling")

Date: 2026-10-29. Tooling for `docs/JIT_PLAN.md` (sections 1, 9, 13): a cycle-count benchmark that compares
N interpreters on `bench/programs`, plus LuaJIT-compatible program variants so LuaJIT can be a
context row on all programs it can run.

## Method

`scripts/bench-cycles.sh name=cmd [name=cmd ...]` (wrapper: `systemd-run` memory cap 8G, no swap, `timeout`,
`</dev/null`) runs `scripts/bench_cycles.py`:

- every run is `perf stat -x, -e cycles:u,instructions:u -- taskset -c $CPU <cmd> <program>`; user-mode
  counters only, so kernel noise (page faults, timers) is excluded;
- **interleaved**: for each run, for each program, for each binary (drift hits all binaries alike); one warm-up
  run per (program, binary) first (checks it runs and compares stdout with the baseline);
- `RUNS` (default 9) runs; per (program, binary) the **minimum** and the **median** of cycles and instructions
  are kept; the table shows min cycles and the ratio to the first binary (the baseline);
- ratios within +-5% are flagged `~` (noise band; the machine runs the `powersave` governor, so wall time and
  even cycles move with frequency scaling; instructions are the stable signal and are in the JSON);
- geomeans of the ratios: over all programs run for that binary, and over the **numeric subset**
  (fannkuch, loops_arith, mandelbrot, matmul, nbody, sieve, spectral_norm: the loop/arithmetic/array-bound
  programs a first template JIT should win on; fib and queens are call/recursion heavy and left out of it);
- env knobs: `CPU` (default 4; use 4 or higher, never 0-3), `RUNS`, `ONLY`, `OUT` (JSON path),
  `LJ_BINS` (binaries that take `bench/luajit/<prog>.lua` when it exists), `EXCLUDE=name:prog+prog`.
- hash seed: `rnlua` honours `RUSTYNAIL_SEED=0` (used, via `env RUSTYNAIL_SEED=0 ...`); the reference is the
  constant-seed build `ref/55/build-seed0/lua`; LuaJIT has no per-run seed knob (string hashes are
  not randomised per process in the distribution build as far as observed, but it is not controllable).

Command used for the baseline (nothing else of ours running; CPU 4, 9 runs):

    OUT=bench/results/jit-baseline.json LJ_BINS=lj EXCLUDE=lj:json+load_big scripts/bench-cycles.sh \
      ref=ref/55/build-seed0/lua "ours=env RUSTYNAIL_SEED=0 target/release/rnlua" lj=luajit

Binaries: `ref` = reference Lua 5.5 (seed-0 build), `ours` = `target/release/rnlua` (default release build,
2026-10-28, commit before the hook-cost prototype, JIT features off), `lj` = `/usr/bin/luajit`
(LuaJIT 2.1.1792588164, distribution build, JIT on). LuaJIT is not run on `json` (float formatting differs) and
`load_big` (bytecode format).

## LuaJIT variants (`bench/luajit/`)

Ten programs use 5.3+ syntax and fail under LuaJIT: closures, coroutines, fasta, heapsort, loops_arith,
metamethods, oop_methods, pcall_error, table_array, varargs. The variants replace `//` by `math.floor(a / b)`,
`>>`, `~`, `&` by arithmetic / `bit.bxor`, `table.pack` by a table constructor, and print integer results
with `%d`. All ten run under reference 5.5, our rnlua and LuaJIT with identical output between the
three (verified). LuaJIT numbers are doubles, so exactness above 2^53 needed care (heapsort's LCG and
loops_arith's 32-bit multiply are split into 16-bit halves; same values as the originals). Output equals the
original program's output for closures, fasta, heapsort, loops_arith, table_array, varargs. Deliberate
divergences (the original is not portable to doubles / 5.1 semantics):

| program | divergence |
|---|---|
| coroutines | running sums reduced mod 2^32 (original wraps int64); other two counters identical |
| metamethods | LuaJIT ignores `__len` on tables, so `#acc` became `(-acc)` with `__unm` (same unary metamethod path) |
| oop_methods | `Savings:interest` deposit taken `% 1000` (original compounds past int64 and wraps) |
| pcall_error | output identical except `err_count`, which includes the script path length in the error message |

The programs are therefore not byte-for-byte the same workload as the originals for LuaJIT rows (marked `*`),
and the cross-engine rows for those 4 are close but not exactly equal work. The `!` flags in the table are
the coroutines and oop_methods checksum differences against the original programs run by `ref`.

## Baseline results (cycles:u, min of 9, CPU 4)

Ratio = binary / ref (lower is faster). `~` = within +-5% of ref. `*` = LuaJIT variant program, `!` = stdout
differs from the baseline program's output (expected, see above).

| program | ref Mcyc | ours | ratio | luajit | ratio |
|---|---:|---:|---:|---:|---:|
| binary_trees | 4354 | 3456 | 0.794 | 2387 | 0.548 |
| closures | 4849 | 4891 | 1.009 ~ | 1031 | 0.213 * |
| coroutines | 3692 | 1900 | 0.515 | 1196 | 0.324 *! |
| fannkuch | 4023 | 3855 | 0.958 ~ | 1069 | 0.266 |
| fasta | 3623 | 3153 | 0.870 | 1495 | 0.413 * |
| fib | 4084 | 4132 | 1.012 ~ | 472 | 0.116 |
| gc_churn | 4478 | 3464 | 0.773 | 3451 | 0.771 |
| heapsort | 3877 | 3893 | 1.004 ~ | 1228 | 0.317 * |
| json | 3075 | 2636 | 0.857 | - | - |
| knucleotide | 4077 | 3987 | 0.978 ~ | 686 | 0.168 |
| load_big | 1767 | 809 | 0.458 | - | - |
| loops_arith | 4350 | 4330 | 0.995 ~ | 1825 | 0.420 * |
| mandelbrot | 4666 | 4142 | 0.888 | 522 | 0.112 |
| matmul | 3338 | 3151 | 0.944 | 269 | 0.081 |
| metamethods | 4575 | 4304 | 0.941 | 527 | 0.115 * |
| nbody | 4299 | 4059 | 0.944 | 390 | 0.091 |
| oop_methods | 4567 | 4717 | 1.033 ~ | 305 | 0.067 *! |
| pcall_error | 4048 | 3068 | 0.758 | 8286 | 2.047 * |
| queens | 4383 | 4309 | 0.983 ~ | 1888 | 0.431 |
| sieve | 3095 | 3374 | 1.090 | 1116 | 0.361 |
| spectral_norm | 4353 | 4217 | 0.969 ~ | 171 | 0.039 |
| string_ops | 2682 | 1870 | 0.697 | 2171 | 0.810 |
| table_array | 3407 | 2276 | 0.668 | 598 | 0.176 * |
| table_hash | 1661 | 1418 | 0.854 | 414 | 0.249 |
| varargs | 5295 | 4558 | 0.861 | 2179 | 0.412 * |

Geomeans of the cycle ratios vs ref (min / median over runs; instruction ratio in brackets):

| | all programs | numeric subset (7) |
|---|---:|---:|
| ours | 0.857 / 0.859 (instr 0.912), 25 programs | 0.968 / 0.973 (instr 1.005) |
| luajit | 0.246 / 0.244 (instr 0.170), 23 programs | 0.144 / 0.143 (instr 0.072) |

Reading: on the numeric subset our interpreter is at parity with the reference (3% fewer cycles, equal
instructions), which is the figure a JIT must beat; LuaJIT with its trace JIT needs about 0.14x the cycles
there (7x faster). The JIT plan's target on the numeric subset should be judged against 0.97 (ours) and
0.14 (LuaJIT). Odd points: LuaJIT is 2x *slower* than ref on pcall_error (error unwinding / trace aborts),
and sieve is 9% slower in ours than ref.

Raw data (min and median cycles/instructions per program and binary, ratios, geomeans):
`bench/results/jit-baseline.json`.

## JIT progress

Cycles:u, CPU 4, min of 7, interleaved, `RUSTYNAIL_JIT=off` vs `on` of the same `--features jit` release binary
(`scripts/bench-cycles.sh "off=env RUSTYNAIL_SEED=0 RUSTYNAIL_JIT=off BIN" "on=env RUSTYNAIL_SEED=0 RUSTYNAIL_JIT=on BIN"`).
Ratio = on / off (lower is faster); `~` = within +-5%. Run 1 is J2a (before this pass, from docs/JIT_PLAN.md section 16);
run 2 is after the performance-regression pass (docs/JIT_DESIGN.md section 16: `EQK` string template + entry back-off).
Note: the machine was shared with another builder, the absolute `off` figures move by up to 30% between runs
(e.g. matmul off 3478 / 6375 Mcyc in two runs), so only the ratios inside one run are meaningful; ratios inside a run
repeat to about +-3%.

| program | J2a ratio | after ratio | off Mcyc | on Mcyc |
|---|---:|---:|---:|---:|
| binary_trees | 0.80 | 0.811 | 3485 | 2825 |
| closures | 0.90 | 0.896 | 5127 | 4595 |
| coroutines | 0.98 | 0.968 ~ | 1937 | 1876 |
| fannkuch | 0.42 | 0.419 | 4027 | 1687 |
| fasta | 0.68 | 0.697 | 3119 | 2175 |
| fib | 0.51 | 0.512 | 4266 | 2184 |
| gc_churn | 0.99 | 0.995 ~ | 3775 | 3757 |
| heapsort | 0.52 | 0.535 | 3965 | 2123 |
| json | **1.125** | **0.977 ~** | 2655 | 2594 |
| knucleotide | 0.90 | 0.896 | 4180 | 3745 |
| load_big | 1.00 | 1.001 ~ | 831 | 831 |
| loops_arith | 0.46 | 0.465 | 4480 | 2083 |
| mandelbrot | 0.59 | 0.614 | 4458 | 2738 |
| matmul | 0.41 | 0.361 | 3478 | 1255 |
| metamethods | **1.071** | **0.993 ~** | 4422 | 4391 |
| nbody | 0.61 | 0.599 | 4187 | 2509 |
| oop_methods | 0.75 | 0.726 | 4632 | 3362 |
| pcall_error | 1.03 | 0.912 | 2974 | 2712 |
| queens | 0.55 | 0.571 | 4277 | 2443 |
| sieve | 0.67 | 0.520 | 5581 | 2901 |
| spectral_norm | 0.65 | 0.575 | 4756 | 2736 |
| string_ops | 0.97 | 0.722 | 3670 | 2651 |
| table_array | 1.00 | 1.002 ~ | 2287 | 2291 |
| table_hash | 0.97 | 0.733 | 4022 | 2950 |
| varargs | 1.00 | 1.003 ~ | 4651 | 4667 |

Geomean on/off: all 25 0.708 (median-of-runs 0.704), numeric subset 0.499. Rows whose ratio moved by more than 5%
without a code change that explains it (sieve, string_ops, table_hash, spectral_norm) are noise from the shared
machine: in a third run string_ops was 1.087 and 0.97 in J2a, with the same binary, and instructions say the JIT
build executes 2% fewer on it; treat those four as 'flat or better'. A run of the previous commit's build under
the same conditions was not repeated.

Per-program root causes and the profile are in docs/JIT_DESIGN.md section 16. In short: json and metamethods
were enter/exit churn (string-constant `EQK` exits right after entry; a metamethod function that always exits at its
final `TAILCALL`), 150 cycles per round trip for 3-17 bytecodes; the flat programs (load_big, varargs, table_array,
gc_churn, string_ops, table_hash) either never compile anything profitable or leave at `TFORCALL`/`VARARGPREP` (J2b)
and pay only the counters' +1.2% instructions.

### After J2b (generic for, varargs, closing) merged with J2a and the back-off pass

Cycles:u, CPU 5, min of 7, interleaved, `RUSTYNAIL_SEED=0`, `RUSTYNAIL_JIT=off` vs `on` of the same merged release binary
(`--features jit`), run on a machine shared with other builders (their jit-diff campaigns ran on CPUs 0-2).

| program | off Mcyc | on Mcyc | ratio |
|---|---:|---:|---:|
| binary_trees | 3603 | 2750 | 0.763 |
| closures | 5198 | 3218 | 0.619 |
| coroutines | 1989 | 1904 | 0.957 ~ |
| fannkuch | 4096 | 1695 | 0.414 |
| fasta | 3212 | 2172 | 0.676 |
| fib | 4343 | 2224 | 0.512 |
| gc_churn | 3722 | 3759 | 1.010 ~ |
| heapsort | 4033 | 2140 | 0.531 |
| json | 2621 | 2497 | 0.953 ~ |
| knucleotide | 4079 | 3717 | 0.911 |
| load_big | 847 | 850 | 1.003 ~ |
| loops_arith | 4657 | 2083 | 0.447 |
| mandelbrot | 4485 | 2742 | 0.611 |
| matmul | 3408 | 1233 | 0.362 |
| metamethods | 4426 | 4441 | 1.003 ~ |
| nbody | 4179 | 2498 | 0.598 |
| oop_methods | 4489 | 3339 | 0.744 |
| pcall_error | 2975 | 2796 | 0.940 |
| queens | 4480 | 2466 | 0.550 |
| sieve | 3048 | 1972 | 0.647 |
| spectral_norm | 4495 | 2740 | 0.610 |
| string_ops | 1741 | 1711 | 0.983 ~ |
| table_array | 2315 | 2237 | 0.966 ~ |
| table_hash | 1275 | 1230 | 0.965 ~ |
| varargs | 4653 | 4114 | 0.884 |

Geomean on/off: all 25 0.714 (instructions 0.691), numeric subset 0.515. J2b's own effect: varargs 1.003 -> 0.884, closures 0.896 -> 0.619
(CLOSE/VARARG templates keep the closure-creating loops compiled), table_array 1.002 -> 0.966, binary_trees 0.811 -> 0.763. sieve,
string_ops and table_hash differ from the previous table mostly through the shared machine (the `off` column of the previous table was
inflated for those, e.g. sieve off 5581 vs 3048 here); no program regresses beyond +1%. The back-off (`Mon`) did not interfere with the
vararg function entries or the TFORCALL helper: no regression among the 25 programs.

## Lua 5.4

2026-10-29, J2b-5.4. `fast` build of the 5.4 `rnlua` with the `jit` feature
(`RUSTFLAGS="--cfg rustynail_lua54" cargo rustc --release -p rn-cli --features jit -- -C relocation-model=static`), one binary run as
`RUSTYNAIL_JIT=off` (baseline) and `RUSTYNAIL_JIT=on`, plus the 5.4 reference (`ref/54/build/lua`, not the seed-0 build, so only a context
column). `scripts/bench-cycles.sh`, CPU 5, min of 7, cycles:u, `RUSTYNAIL_SEED=0`.

| program | off Mcyc | on Mcyc | on/off | ref Mcyc | ref/off |
|---|---:|---:|---:|---:|---:|
| binary_trees | 3510 | 2779 | 0.792 | 4360 | 1.242 |
| closures | 4912 | 3249 | 0.661 | 5019 | 1.022 ~ |
| coroutines | 1935 | 1896 | 0.980 ~ | 3517 | 1.818 |
| fannkuch | 3887 | 1960 | 0.504 | 4152 | 1.068 |
| fasta | 3070 | 2143 | 0.698 | 3790 | 1.234 |
| fib | 4167 | 2117 | 0.508 | 4134 | 0.992 ~ |
| gc_churn | 3274 | 3440 | 1.051 | 5411 | 1.653 |
| heapsort | 3927 | 2250 | 0.573 | 4050 | 1.031 ~ |
| json | 2634 | 2509 | 0.953 ~ | 3170 | 1.204 |
| knucleotide | 4294 | 3611 | 0.841 | 4043 | 0.941 |
| load_big | 574 | 584 | 1.018 ~ | 625 | 1.088 |
| loops_arith | 4171 | 2059 | 0.494 | 4893 | 1.173 |
| mandelbrot | 4124 | 2719 | 0.659 | 5801 | 1.407 |
| matmul | 3062 | 1351 | 0.441 | 3701 | 1.209 |
| metamethods | 4338 | 4368 | 1.007 ~ | 4762 | 1.098 |
| nbody | 4011 | 2516 | 0.627 | 4744 | 1.183 |
| oop_methods | 4594 | 3240 | 0.705 | 4670 | 1.016 ~ |
| pcall_error | 3102 | 2951 | 0.951 ~ | 4309 | 1.389 |
| queens | 4288 | 2489 | 0.580 | 4184 | 0.976 ~ |
| sieve | 3245 | 2282 | 0.703 | 4315 | 1.330 |
| spectral_norm | 4820 | 2754 | 0.571 | 4342 | 0.901 |
| string_ops | 1737 | 1730 | 0.996 ~ | 2592 | 1.492 |
| table_array | 2096 | 2017 | 0.962 ~ | 3593 | 1.714 |
| table_hash | 1310 | 1249 | 0.954 ~ | 1465 | 1.119 |
| varargs | 4458 | 3975 | 0.892 | 5070 | 1.137 |

Geomean on/off: all 25 0.739 (instructions 0.737), numeric subset 0.564 (5.5 on a different day: 0.714 / 0.515). The only regression above
noise is gc_churn (1.051, not analysed; the 5.5 table has it flat). The 5.4 inline array accesses pay one extra load/test/branch for the
`alimit` check (matmul 0.441, table_array 0.962, sieve 0.703 vs 5.5's 0.362 / 0.966 / 0.647).

## J5 prototype gate (task P9; JIT_TRACE.md 1.6; raw data `docs/bench-p9-*.txt`, tables `docs/bench-p9-results.txt`)

Method: the T13 method (`scripts/bench-cycles.sh`, `RUNS=9`, min of 9 `cycles:u`, CPU 4, `RUSTYNAIL_SEED=0`, interleaved,
+-5% band). One `--features jit,trace` release binary of `8154671`; columns J4 (`T15=on TRACE=off`, the baseline), tier 1
(`T15=off TRACE=off`), prototype (`T15=off TRACE=on`) and a `--features jit` binary of main@89963c6 (`T15=on`). Micro-benchmarks:
`PROGDIR=bench/jit-trace`; spectral_norm, matmul and the 25 programs from `bench/programs`. Two sessions (the second with equal
environment length and binary path per column) agree to 1%. **Verdict NO-GO** (criteria 5 and 6 fail; 2, 3, 4 pass).

Mcycles (primary session), trace / J4 in the last column:

| program | set | tier 1 | J4 | 89963c6 | trace | trace/J4 |
|---|---|---|---|---|---|---|
| spectral_norm | a | 2760 | 2757 | 2759 | 269 | **0.097** |
| matmul | a | 1240 | 658 | 656 | 526 | **0.800** |
| licm_expr | b | 9055 | 2026 | 2028 | 469 | 0.232 |
| array_rec | b | 6492 | 4503 | 4506 | 1819 | 0.404 |
| upval_acc | b | 5362 | 5429 | 5033 | 1846 | 0.340 |
| int_rec2 | b | 5293 | 2551 | 2732 | 1225 | 0.480 |
| inline_leaf | c | 6369 | 6364 | 6373 | 663 | 0.104 |
| inline_2 | c | 6271 | 6272 | 6255 | 414 | 0.066 |
| int_sum | b' | 3105 | 2039 | 2039 | 1247 | 0.612 |
| float_poly | b' | 7597 | 1815 | 1814 | 3149 | **1.735** |
| float_loop | b' | 11119 | 2911 | 2911 | 2777 | 0.954 |
| dot | b' | 5664 | 2255 | 2252 | 1713 | 0.759 |
| upval_inv | b' | 7612 | 3834 | 3791 | 2167 | 0.565 |
| loops_arith | info | 2083 | 800 | 800 | 694 | 0.867 |
| mandelbrot | info | 2742 | 573 | 573 | 1036 | **1.806** |
| fannkuch | info | 1695 | 1214 | 1209 | 1255 | 1.034 |
| heapsort | info | 2122 | 1673 | 1688 | 1471 | 0.879 |
| queens | info | 2451 | 2563 | 2559 | 2444 | 0.954 |
| sieve | info | 2358 | 1679 | 1695 | 2674 | **1.592** |

Geomeans trace/J4: set (b) 0.352 (limit 0.90), gate set a+b+c 0.231 (0.85), informational 0.994 (1.15). Set (d), trace column
against stable twin: type_entry 2148 / 1989 = 1.080, type_late 2538 / 1791 = 1.417 (limit 1.05, both FAIL; inner heads end
`Traced` after 0 / 1 re-records). All 25 programs trace / tier 1: sieve 1.134 FAIL, all others <= 1.016 (e.g. closures 0.698,
heapsort 0.693, fannkuch 0.740, matmul 0.424, mandelbrot 0.378, loops_arith 0.333, spectral_norm 0.098, nbody 1.005).

**T13 comparison (25 programs).** J4 column of the J5 binary vs the T13 table (`bench-t13-j4.txt`, regions column): absolute
cycles geomean 0.999, J4 / tier 1 geomean 0.825 in both; every program within 5% of its T13 value. Trace / J4 over the 25: 0.947
geomean (spectral_norm 0.097, matmul 0.800, heapsort 0.879, loops_arith 0.867, closures 0.696; mandelbrot 1.806, sieve 1.592,
nbody 2.199 untraced - hash fields).

**Baseline verification.** Not byte-identical to main@89963c6 because of the intentional P0y exit sequence (budget refund + exit
tails; all other lines of all 69 tier-1 and 121 region protos equal); 61680f8 (before P0y) is identical. All 25 bench/programs within
the band of 89963c6 (geomean 0.999); six micro programs outside (int_rec2 1.07, upval_acc 0.93, type_entry 0.83,
type_entry_stable 0.82, type_late 1.25, type_late_stable 0.80) with identical instructions: code-layout effects (a probe shows
+-42% cycles for the same trace code at different arena offsets, `bench-p9-layout-probe.txt`).

**Cost (criterion 7).** Trace record + compile <= 0.48% of the wall time of every program (new Instant counters, patch in
`scripts/p9_recns.patch`); `tests/vm` 132 short scripts geomean trace / tier 1 0.991 (two above 1.05: 1.14 and 1.09); trace
code bytes / tier-1 code bytes of the same program <= 1.26 (queens), gate programs 0.14-0.35.

## J5 prototype gate, rerun (task P9b; `docs/bench-p9b-*`, text in `docs/bench-p9-results.txt`)

Method as in P9 above (one PAD session, binary built at `7de3f4a` = P9 + Q1 + Q2). Mcycles, trace / J4 with the P9 ratio alongside:

| program | set | J4 | trace | trace/J4 | P9 |
|---|---|---|---|---|---|
| spectral_norm | a | 2772.3 | 269.2 | **0.097** | 0.097 |
| matmul | a | 652.5 | 519.6 | **0.796** | 0.800 |
| licm_expr | b | 2025.1 | 470.2 | **0.232** | 0.232 |
| array_rec | b | 4501.4 | 1818.9 | **0.404** | 0.404 |
| upval_acc | b | 5428.6 | 2010.2 | **0.370** | 0.340 |
| int_rec2 | b | 2518.6 | 1224.2 | **0.486** | 0.480 |
| inline_leaf | c | 6367.2 | 663.2 | **0.104** | 0.104 |
| inline_2 | c | 6268.9 | 409.4 | **0.065** | 0.066 |
| int_sum | b' | 2039.3 | 1246.5 | **0.611** | 0.612 |
| float_poly | b' | 1815.6 | 1824.0 | **1.005** | 1.735 |
| float_loop | b' | 2908.3 | 2867.4 | **0.986** | 0.954 |
| dot | b' | 2246.4 | 1763.8 | **0.785** | 0.759 |
| upval_inv | b' | 3835.9 | 2175.3 | **0.567** | 0.565 |
| loops_arith | info | 800.3 | 697.0 | **0.871** | 0.867 |
| mandelbrot | info | 573.6 | 1035.4 | **1.805** | 1.806 |
| fannkuch | info | 1216.2 | 1295.6 | **1.065** | 1.034 |
| heapsort | info | 1673.9 | 1479.8 | **0.884** | 0.879 |
| queens | info | 2547.5 | 2439.8 | **0.958** | 0.954 |
| sieve | info | 1693.2 | 2391.6 | **1.412** | 1.592 |

| criterion | P9b | P9 | verdict |
|---|---|---|---|
| 1 correctness | 666 + 84 (mem) + 84 (reg) tests, mutants 62 reg / 51 caught / 0 NOT caught / 11 not injected; official 32/32 x3 modes; jit-diff 1..2000 (7 modes) 0 diffs; + trnoopt/trnosink/trverify 1..1000 0 diffs; GC stress 1/2 under TRACE=always and =on OK; text identity vs 3fcfe13 / 89963c6 / fast-copy IDENTICAL | 664 + 83 | PASS |
| 2 spectral_norm <= 0.70 | 0.097 | 0.097 | PASS |
| 3 matmul <= 0.95 | 0.796 | 0.800 | PASS |
| 4 set (b) geomean <= 0.90 | 0.360 (licm_expr 0.232, array_rec 0.404, upval_acc 0.370, int_rec2 0.486) | 0.352 | PASS |
| 5a gate geomean <= 0.85, each <= 1.05 | 0.234 / worst matmul 0.796 | 0.231 / 0.800 | PASS |
| 5b each of the 25 <= 1.05 x tier 1 | worst sieve 1.008, all others <= 1.007 | sieve 1.134 | **PASS** (was FAIL) |
| 5c set (d) <= 1.05 x twin | type_entry **1.090** FAIL, type_late 1.001 PASS; heads Traced | 1.080 / 1.417 | **FAIL** (type_entry: F2a) |
| 6 informational geomean <= 1.15, each <= 1.30 | geomean 0.946 PASS; mandelbrot **1.805**, sieve **1.412** FAIL; fannkuch 1.065 (attributed F2a/F2b); float_poly 1.005 | 0.994; float_poly 1.735, mandelbrot 1.806, sieve 1.592 | **FAIL** |
| 7 record+compile <= 2% | 0.477% (spectral_norm) | 0.48% | PASS |
| 7 short scripts within band | 132 scripts, geomean tr/t1 0.989 (two above 1.05: 1.137, 1.103) | 0.991 | PASS |
| 7 trace code <= 1.5 x tier 1 per program | sieve **1.67**, queens 1.38, others <= 0.55 | worst queens 1.26 | **FAIL** (sieve) |

Verdict: **still NO-GO** by the unchanged rule (5c fails; 6 and 7-code-size fail). Cured by Q1/Q2: float_poly, type_late, sieve vs tier 1, the
layout dependence (probe 1.79-1.83 Gcyc in all placements). Remaining: mandelbrot (per-pixel entry/exit, F2b), type_entry (entry-type exit, F2a),
sieve (traces flushed Ineffective / Rerec and installed again: 12 traces of 3 loops, 1.41 x J4, 1.67 x tier-1 bytes; F2a / F3, and the repeated
re-install of an identical ineffective trace is not covered by a named task). Mild Q1/Q2 side effects: upval_acc 0.340 -> 0.370, dot 0.759 -> 0.785,
float_loop 0.954 -> 0.986, fannkuch 1.034 -> 1.065 (not investigated; layout is the likely cause). Raw data `docs/bench-p9b-*`, tables and full text
in `docs/bench-p9-results.txt` ("P9b gate results").
