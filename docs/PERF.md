# Performance

> **Names.** The binaries are `rnlua`, `rnluac` and `rnvmrun`; the crates carry the `rn-` prefix (`rn-core`, `rn-cli`, ...; the cargo
> package of the interpreter is `rn-cli`). Historical sections below, and the benchmark reports under `bench/results/` and
> `docs/bench-*`, keep the binary and crate names of their time (the executable was once named after its cargo package, and
> the `lua-*` crates were renamed to `rn-*`). Builds before BUNDLE W had the JIT opt-in (`--features jit`); it is now a default
> feature (see docs/JIT.md). Since the release-build change of 2026-11-11 the shipped binaries are PGO builds, stripped.

## Release benchmark (session-release)

Full report: `bench/results/session-release/report.md` (tables in `tables.md`, `extra.md`, per-program CSVs, raw JSON, build logs).
223 programs (all benchmark sets plus the held-out corpus and corpus2), min of 5 interleaved runs, `perf stat cycles:u,instructions:u`,
`RUSTYNAIL_SEED=0`; every output of every build equals PUC's and the `.expected` files. Geomean of cycles, below 1 means ours is faster:

| build | ours / LuaJIT 2.1 | ours / PUC (same version) |
|---|---|---|
| 5.5 plain | 0.995 | 0.230 |
| 5.5 PGO (the release build) | 0.905 | 0.210 |
| 5.4 plain | 1.184 | 0.268 |
| 5.4 PGO | 1.081 | 0.245 |

- PGO versus plain (5.5): 0.913 cycles, 0.919 instructions; 135 programs more than 3% faster, 81 within 3%, 7 more than 3% slower
  (`lib/char` +48% under PGO, cause not diagnosed). Held-out sets gain as much as the trained ones, so PGO is the release default.
- The JIT: 5.5 programs run about 3.2x faster than with `RUSTYNAIL_JIT=off` (5.4: 2.8x). Nothing regressed against the previous
  benchmark session (bm3, rebuilt in the same session): 0.989 cycles plain, 0.994 PGO.
- 5.4 is about 16% less effective relative to PUC than 5.5: `rawget`/`rawset`/`next` not recorded, inline `#t` off, integer-key
  hash-part and `__index`/`__newindex` array accesses off, metamethod-heavy programs (RETF inert). Candidates and status:
  `docs/JIT_PARKED.md`.
- Startup of an empty script 0.49 ms (PUC 5.5 0.32, LuaJIT 0.38); stripped `rnlua` 2.94 MB (5.5 plain), 3.31 MB (5.5 PGO),
  2.73 / 2.99 MB (5.4 plain / PGO); empty-state `VmHWM` 4.2 MB (PUC 2.3 MB).

The older sections below keep their original measurements (interpreter only, 25 programs, wall time or instructions) and are
not superseded by the table above: they answer a different question (interpreter versus reference without the JIT).

## Final report (Phase 8, 2026-10-28)

**Method.** `scripts/bench.py --runs 5 --cpu 3`: the 25 programs of `bench/programs` (each 1-2 s on the
reference); per program and binary one warm-up run whose stdout must equal the reference's, 5 timed runs
pinned to CPU 3 (`taskset`; **time = minimum** wall time), one peak-RSS run (`wait4`) and one
`perf stat -e instructions:u` run. Ratios are ours / reference: below 1 is faster or smaller. Raw data:
`bench/results/final-55.json`, `final-54.json`, `final-luajit.json`; tables regenerated with
`scripts/bench_report.py <json>`.

**Quiet-machine rerun (merged main f1bf16e).** `bench/results/quiet-55.json`, `quiet-54.json`; 5 runs, CPU 3, no other jobs.
Geomean of ours/reference: Lua 5.5 default time 0.860, fast 0.840 (instructions 0.912 / 0.874, RSS 1.00);
Lua 5.4 default time 0.853, fast 0.824 (instructions 0.939 / 0.900, RSS 0.91). Worst programs by time: sieve 1.12
(5.5 default, 1.11 fast), queens 1.11 (5.4 default, 1.06 fast), closures 1.03 (5.4 default).
These replace the load-affected wall times of the first Phase 8 run.

**Machine.** AMD Ryzen AI 7 350 (Zen 5, 8 cores / 16 threads, 16 MB L3), 30 GB RAM, Linux
(Arch family), CPU governor `powersave` / EPP `balance_performance` (not changeable here: wall times carry a
few percent of noise; **instruction counts are the stable signal**). During these runs another job kept
~8 cores busy (Miri), so absolute times are a little slower and noisier than in earlier logs; all
binaries were measured interleaved per program under the same conditions.

**Builds.** Reference: upstream 5.5.1 / 5.4.9, `make linux` (gcc 16.2.1 `-O2`), `ref/55/build/lua`,
`ref/54/build/lua`. Ours (rustc 1.99.1, release profile: `opt-level=3`, fat LTO, 1 CGU):
`default` = `cargo build --release`; `fast` = `scripts/build-fast.sh` (absolute dispatch jump table);
`pgo` = `scripts/pgo.sh`; `bolt` = `scripts/bolt.sh` (fast + BOLT, LLVM 23.1.4); `pgo-bolt` =
`BOLT_PGO=1 scripts/bolt.sh`. PGO and BOLT train on `scripts/train.sh` (disjoint from the benchmarks).

### Summary

| | Lua 5.5 vs ref 5.5.1 | Lua 5.4 vs ref 5.4.9 |
|---|---|---|
| **instructions** geomean: default / fast / PGO / BOLT / PGO+BOLT | **0.915** / 0.877 / 0.855 / 0.866 / **0.854** | **0.940** / 0.899 / 0.862 / 0.890 / **0.863** |
| instructions, worst programs (default build) | loops_arith 1.06, queens 1.06, heapsort 1.04 | loops_arith 1.07, fannkuch 1.06, queens 1.06 |
| programs with fewer instructions than ref (default / fast; within 0.5% counted as equal) | 16 (+2 equal) / 24 (+1 equal) of 25 | 17 / 23 (+2 equal) of 25 |
| cycles:u geomean (one run each, see the noise note) | 0.848 / 0.840 / 0.795 / 0.817 / 0.802 | 0.852 / 0.795 / 0.774 / 0.809 / 0.788 |
| wall time geomean (min of 5, noisy this run) | 0.861 / 0.839 / 0.805 / 0.824 / 0.788 | 0.876 / 0.740 / 0.743 / 0.853 / 0.806 |
| wall time on a quiet machine (earlier runs, same code) | default 0.861, fast 0.847, BOLT 0.830 (`step16-bolt.json`) | default 0.843, fast 0.816 (`lua54-step1.json`) |
| peak RSS geomean / worst | 1.013 / 1.17 (string_ops) | 0.90 / 1.01 |

Reading: the instruction counts are the reliable comparison (geomean within 0.3% of the step-16 run;
single programs move by up to 1-4% with the random string-hash seed);
the code is the same as at the end of Phase 7 (the 5.5 release binary's `.text` is byte-identical), so the
quiet-machine wall times of the step-16/17 logs still describe it: **about 14-16% faster than the
reference** on the default build, 15-18% with `build-fast.sh`, a further 1-3% with PGO and/or BOLT.
In this run individual wall-time and cycle ratios swing by up to 2x (e.g. fannkuch 1.71 vs 1.03
instructions, mandelbrot 0.97 vs 0.33 between near-identical builds) because the SMT sibling of the
pinned core was busy for whole programs; those per-program time ratios are not meaningful and are
only kept in the JSON. Ranking of the variants by instructions: PGO+BOLT ~ PGO < BOLT < fast < default.
PGO's gain is mostly fewer instructions (table_array 0.80 -> 0.67, gc_churn 0.74 -> 0.66), BOLT's is
layout (cycles) on top of the fast build. Memory: equal to the reference within 3% except string_ops
(+17%, the string table and buffer pacing), binary_trees (+13%) and knucleotide (+8%, 5.5).

### Lua 5.5: instructions per program (ratio to the reference)

**instructions** (ratio to `ref`)

| benchmark | ref (M instr) | default | fast | pgo | bolt | pgo-bolt |
|---|---:|---:|---:|---:|---:|---:|
| binary_trees | 20,991 | 0.78 | 0.75 | 0.69 | 0.74 | 0.69 |
| closures | 25,098 | 1.00 | 0.95 | 0.89 | 0.93 | 0.89 |
| coroutines | 9,715 | 0.94 | 0.92 | 0.89 | 0.91 | 0.90 |
| fannkuch | 20,004 | 1.03 | 0.96 | 0.97 | 0.95 | 0.97 |
| fasta | 13,622 | 0.94 | 0.90 | 0.91 | 0.89 | 0.91 |
| fib | 18,177 | 1.03 | 0.98 | 1.01 | 0.98 | 1.00 |
| gc_churn | 19,842 | 0.74 | 0.72 | 0.66 | 0.71 | 0.66 |
| heapsort | 18,210 | 1.04 | 0.97 | 0.99 | 0.95 | 0.99 |
| json | 12,471 | 0.94 | 0.91 | 0.87 | 0.90 | 0.87 |
| knucleotide | 22,323 | 0.94 | 0.92 | 0.87 | 0.90 | 0.89 |
| load_big | 6,571 | 0.50 | 0.49 | 0.46 | 0.49 | 0.46 |
| loops_arith | 24,765 | 1.06 | 1.00 | 1.02 | 1.00 | 1.01 |
| mandelbrot | 26,759 | 1.00 | 0.94 | 0.96 | 0.94 | 0.97 |
| matmul | 14,519 | 0.98 | 0.93 | 0.95 | 0.92 | 0.95 |
| metamethods | 23,145 | 0.90 | 0.87 | 0.84 | 0.86 | 0.84 |
| nbody | 24,002 | 0.94 | 0.89 | 0.90 | 0.88 | 0.87 |
| oop_methods | 25,827 | 0.90 | 0.86 | 0.84 | 0.84 | 0.83 |
| pcall_error | 14,308 | 0.95 | 0.93 | 0.86 | 0.92 | 0.86 |
| queens | 15,646 | 1.06 | 0.98 | 0.99 | 0.97 | 0.99 |
| sieve | 13,032 | 1.03 | 0.97 | 1.00 | 0.96 | 1.00 |
| spectral_norm | 23,293 | 1.03 | 0.97 | 0.99 | 0.96 | 0.99 |
| string_ops | 6,716 | 0.82 | 0.80 | 0.81 | 0.79 | 0.81 |
| table_array | 18,916 | 0.80 | 0.80 | 0.67 | 0.78 | 0.67 |
| table_hash | 2,581 | 0.85 | 0.83 | 0.80 | 0.82 | 0.80 |
| varargs | 28,489 | 0.91 | 0.87 | 0.83 | 0.85 | 0.81 |
| **geomean** | | **0.915** | **0.877** | **0.855** | **0.866** | **0.854** |
| worst | | 1.06 | 1.00 | 1.02 | 1.00 | 1.01 |

### Lua 5.4: instructions per program (ratio to the reference)

**instructions** (ratio to `ref`)

| benchmark | ref (M instr) | default | fast | pgo | bolt | pgo-bolt |
|---|---:|---:|---:|---:|---:|---:|
| binary_trees | 20,704 | 0.80 | 0.77 | 0.69 | 0.76 | 0.69 |
| closures | 24,813 | 1.01 | 0.96 | 0.87 | 0.95 | 0.87 |
| coroutines | 9,935 | 0.92 | 0.90 | 0.87 | 0.90 | 0.87 |
| fannkuch | 19,920 | 1.06 | 1.00 | 0.98 | 0.98 | 0.98 |
| fasta | 13,899 | 0.92 | 0.88 | 0.86 | 0.87 | 0.86 |
| fib | 17,942 | 1.04 | 0.99 | 0.98 | 0.98 | 0.98 |
| gc_churn | 19,303 | 0.73 | 0.71 | 0.65 | 0.70 | 0.65 |
| heapsort | 18,356 | 1.04 | 0.98 | 0.95 | 0.96 | 0.95 |
| json | 12,549 | 0.93 | 0.90 | 0.86 | 0.90 | 0.85 |
| knucleotide | 22,324 | 0.95 | 0.93 | 0.88 | 0.92 | 0.86 |
| load_big | 2,415 | 1.03 | 1.00 | 0.98 | 0.99 | 0.98 |
| loops_arith | 25,285 | 1.07 | 0.99 | 0.97 | 0.99 | 0.97 |
| mandelbrot | 27,882 | 0.96 | 0.90 | 0.89 | 0.89 | 0.90 |
| matmul | 15,246 | 0.98 | 0.93 | 0.91 | 0.92 | 0.91 |
| metamethods | 23,934 | 0.86 | 0.83 | 0.80 | 0.83 | 0.80 |
| nbody | 24,951 | 0.88 | 0.83 | 0.84 | 0.82 | 0.81 |
| oop_methods | 25,701 | 0.93 | 0.87 | 0.83 | 0.85 | 0.88 |
| pcall_error | 14,440 | 0.95 | 0.94 | 0.86 | 0.93 | 0.86 |
| queens | 15,875 | 1.06 | 0.99 | 0.96 | 0.97 | 0.96 |
| sieve | 15,283 | 0.96 | 0.90 | 0.88 | 0.89 | 0.88 |
| spectral_norm | 23,693 | 1.03 | 0.97 | 0.94 | 0.96 | 0.94 |
| string_ops | 6,617 | 0.83 | 0.81 | 0.82 | 0.80 | 0.82 |
| table_array | 17,601 | 0.87 | 0.87 | 0.81 | 0.86 | 0.81 |
| table_hash | 2,636 | 0.86 | 0.84 | 0.78 | 0.83 | 0.78 |
| varargs | 27,002 | 0.92 | 0.88 | 0.81 | 0.87 | 0.83 |
| **geomean** | | **0.940** | **0.899** | **0.862** | **0.890** | **0.863** |
| worst | | 1.07 | 1.00 | 0.98 | 0.99 | 0.98 |

### Peak RSS (Lua 5.5)

**rss** (ratio to `ref`)

| benchmark | ref (MB) | default | fast | pgo | bolt | pgo-bolt |
|---|---:|---:|---:|---:|---:|---:|
| binary_trees | 49 | 1.13 | 1.13 | 1.13 | 1.13 | 1.13 |
| closures | 17 | 1.00 | 1.00 | 1.00 | 1.00 | 1.00 |
| coroutines | 17 | 1.00 | 1.00 | 1.00 | 1.00 | 1.00 |
| fannkuch | 17 | 1.00 | 1.00 | 1.00 | 1.00 | 1.00 |
| fasta | 48 | 1.03 | 1.03 | 1.03 | 1.03 | 1.03 |
| fib | 17 | 1.00 | 1.00 | 1.00 | 1.00 | 1.00 |
| gc_churn | 42 | 0.96 | 0.96 | 0.95 | 0.96 | 0.96 |
| heapsort | 21 | 1.03 | 1.03 | 1.02 | 1.03 | 1.03 |
| json | 17 | 1.00 | 1.00 | 1.00 | 1.00 | 1.00 |
| knucleotide | 74 | 1.08 | 1.08 | 1.08 | 1.08 | 1.10 |
| load_big | 17 | 1.00 | 1.00 | 1.00 | 1.00 | 1.00 |
| loops_arith | 17 | 1.00 | 1.00 | 1.00 | 1.00 | 1.00 |
| mandelbrot | 17 | 1.00 | 1.00 | 1.00 | 1.00 | 1.00 |
| matmul | 17 | 1.00 | 1.00 | 1.00 | 1.00 | 1.00 |
| metamethods | 17 | 1.00 | 1.00 | 1.00 | 1.00 | 1.00 |
| nbody | 17 | 1.00 | 1.00 | 1.00 | 1.00 | 1.00 |
| oop_methods | 17 | 1.00 | 1.00 | 1.00 | 1.00 | 1.00 |
| pcall_error | 17 | 1.00 | 1.00 | 1.00 | 1.00 | 1.00 |
| queens | 17 | 1.00 | 1.00 | 1.00 | 1.00 | 1.00 |
| sieve | 148 | 0.99 | 0.99 | 0.99 | 0.99 | 0.99 |
| spectral_norm | 17 | 1.00 | 1.00 | 1.00 | 1.00 | 1.00 |
| string_ops | 79 | 1.17 | 1.17 | 1.17 | 1.17 | 1.17 |
| table_array | 17 | 1.00 | 1.00 | 1.00 | 1.00 | 1.00 |
| table_hash | 52 | 0.96 | 0.96 | 0.95 | 0.95 | 0.95 |
| varargs | 17 | 1.00 | 1.00 | 1.00 | 1.00 | 1.00 |
| **geomean** | | **1.013** | **1.013** | **1.012** | **1.013** | **1.013** |
| worst | | 1.17 | 1.17 | 1.17 | 1.17 | 1.17 |

### LuaJIT 2.1 (context only, not a gate)

`/usr/bin/luajit` (LuaJIT 2.1.1792588164, the distribution build) in interpreter mode (`-joff`) and with
the JIT, against the reference 5.5 and our default and fast builds. Only programs that LuaJIT runs
**unmodified with output identical to the reference** are compared (13 of 25):
binary_trees, fannkuch, fib, gc_churn, knucleotide, mandelbrot, matmul, nbody, queens, sieve,
spectral_norm, string_ops, table_hash. Excluded: 11 programs use the integer-division operator `//`
(Lua 5.3+; a syntax error in LuaJIT): closures, coroutines, fasta, heapsort, loops_arith, metamethods,
oop_methods, pcall_error, table_array, varargs; `json` runs but formats floats differently
(5.3+ prints `3.0`, LuaJIT `3`, so the checksum differs); `load_big` sums `string.dump` sizes, and
LuaJIT's bytecode format is a different one. No compatibility shim was used: rewriting `//` (and
integer/float semantics) would change what the programs measure.

Instructions:

**instructions** (ratio to `ref`)

| benchmark | ref (M instr) | default | fast | luajit-joff | luajit |
|---|---:|---:|---:|---:|---:|
| binary_trees | 20,991 | 0.78 | 0.75 | 0.53 | 0.34 |
| fannkuch | 20,004 | 1.03 | 0.96 | 0.68 | 0.12 |
| fib | 18,177 | 1.03 | 0.98 | 0.52 | 0.16 |
| gc_churn | 19,769 | 0.74 | 0.72 | 0.46 | 0.46 |
| knucleotide | 22,390 | 0.93 | 0.91 | 0.53 | 0.16 |
| mandelbrot | 26,759 | 1.00 | 0.94 | 0.60 | 0.04 |
| matmul | 14,519 | 0.98 | 0.93 | 0.69 | 0.08 |
| nbody | 24,542 | 0.89 | 0.85 | 0.49 | 0.07 |
| queens | 15,646 | 1.06 | 0.98 | 0.63 | 0.28 |
| sieve | 13,032 | 1.03 | 0.97 | 0.76 | 0.08 |
| spectral_norm | 23,293 | 1.03 | 0.97 | 0.49 | 0.03 |
| string_ops | 6,717 | 0.82 | 0.80 | 0.65 | 0.53 |
| table_hash | 2,581 | 0.85 | 0.83 | 0.52 | 0.26 |
| **geomean** | | **0.930** | **0.889** | **0.574** | **0.143** |
| worst | | 1.06 | 0.98 | 0.76 | 0.53 |

Wall time (min of 5, same noise caveat as above):

**time** (ratio to `ref`)

| benchmark | ref (ms) | default | fast | luajit-joff | luajit |
|---|---:|---:|---:|---:|---:|
| binary_trees | 2,427 | 0.81 | 0.77 | 0.76 | 0.59 |
| fannkuch | 2,039 | 1.00 | 0.92 | 0.83 | 0.19 |
| fib | 1,812 | 0.99 | 1.00 | 0.54 | 0.13 |
| gc_churn | 2,639 | 0.79 | 0.77 | 0.62 | 0.62 |
| knucleotide | 2,256 | 0.89 | 0.91 | 0.56 | 0.16 |
| mandelbrot | 1,492 | 0.87 | 1.74 | 1.08 | 0.12 |
| matmul | 2,037 | 0.98 | 1.01 | 0.59 | 0.07 |
| nbody | 2,367 | 1.00 | 0.90 | 0.58 | 0.08 |
| queens | 1,904 | 1.05 | 1.03 | 0.71 | 0.30 |
| sieve | 915 | 1.53 | 1.52 | 1.40 | 0.39 |
| spectral_norm | 1,315 | 0.96 | 0.94 | 0.84 | 0.04 |
| string_ops | 868 | 0.73 | 0.72 | 1.05 | 1.26 |
| table_hash | 997 | 0.90 | 0.91 | 0.37 | 0.29 |
| **geomean** | | **0.948** | **0.980** | **0.722** | **0.211** |
| worst | | 1.53 | 1.74 | 1.40 | 1.26 |

Reading: LuaJIT's interpreter (hand-written assembly, NaN-boxed values, no 64-bit integers) executes
about 0.57x the reference's instructions on these programs, ours 0.93x (default) / 0.89x (fast); in
time LuaJIT `-joff` was ~0.72x of the reference and ours ~0.95x in this (noisy) run, so LuaJIT's
interpreter is roughly 25-35% faster than ours here. The JIT is in another class (0.21x time; 0.04x on
spectral_norm), as expected. string_ops is the exception where both of ours beat LuaJIT (0.73 vs 1.05
interpreter / 1.26 JIT): the pattern matcher and `string.format`/concatenation rewrites of Phase 7.

Reproduce: `scripts/bench.py --runs 5 --cpu 3 --only <list> --bin ref=ref/55/build/lua --bin
default=target/release/rn-cli --bin fast=target/fast/release/rn-cli --bin "luajit-joff=luajit -joff"
--bin luajit=luajit` (bench.py accepts binaries with arguments).

---------------------------------------------------------------------------------------------------

# Performance log (Phases 6-7)

Method: `scripts/bench.py` (25 programs in `bench/programs`, wall time min-of-N pinned to a core,
`perf stat` instructions/branch-misses, peak RSS; output must equal the reference's) and
`scripts/micro.py` (instructions per bytecode, `bench/micro`). Reference: upstream 5.5.1 built with
`make linux` (gcc -O2) in `ref/55/build/lua`. Ours: `cargo build --release -p rn-cli` (fat LTO,
codegen-units 1). Gate G-parity: geomean time <= 1.05x, no benchmark > 1.35x. The CPU governor is
`powersave`, so wall times are noisy; instruction counts and `ls_bad_status2.stli_other`
(store-forward failures) are the stable signals.

| step | geomean time | worst | notes |
|---|---|---|---|
| baseline (bench/results/baseline0.json) | 1.59x | 11.5x load_big | |
| `trap` in memory, `base_idx`/`cl`/`p` not loop-carried, field-wise `Value` loads/stores | 1.27x | 11.4x load_big | store-forward stalls were 3x on table-heavy loops |

## Findings
- **Store-to-load forwarding** (biggest single win): `Value` is a 1-byte tag + 8-byte payload written
  with two stores; reading it back as a 16-byte aggregate (`movups`) stalls ~12 cycles. `matmul` had 6.8x
  more `ls_bad_status2.stli_other` than PUC. All hot-path loads/stores now use `Value::load/store`
  (field-wise). Never copy a `Value` through a stack temporary + `movups`.
- **Dispatch**: ~19 instructions per bytecode vs PUC's ~8: relative jump table (+3, `-Crelocation-model=static`
  would remove it but breaks PIE linking of `librn_capi.a`), opcode range check (+2), `ci.trap` memory test.
- The interpreter keeps few values in registers across the loop; keep loop-carried state minimal.
- PUC's `trap` is a local initialised from `L->hookmask` at `startfunc`; `luaG_tracecall` leaves it 0 for
  vararg functions until VARARGPREP. Emulated through `ci.trap` (see `trace_call`/`Varargprep`).

## Call path: inline Lua-call fast path + single stack-pointer load on frame restart
- Hypothesis: `pre_call` (generic loop, `check_stack`, `prep_call_info`, `next_ci`) is out of line and the
  frame restart loads the thread's stack pointer twice.
- Change: `State::pre_call_lua_fast` (inline; falls back to `pre_call` when stack must grow, no spare
  `CallInfo`, or callee isn't a Lua closure); restart derives the closure from `base - 1`.
- Result (quick.sh, ours/ref time): call_lua 1.57 -> 1.24, call_arg_ret 1.52 -> 1.28, self_call 1.39 -> 1.23,
  fib instructions 1.48 -> 1.28. Official suite 31/31, `cargo test --workspace` green.
- Kept. Remaining: Return1/Return0 + `do_ret!` restart chain (ci->previous->func->tag load latency).

## RSS: library allocations never triggered a GC step (table.pack etc.)
- Observation: `varargs` RSS 28x the reference; `table.pack` x2M reached 199 MB (also gmatch, load,
  debug.getinfo, io.tmpfile). Native functions allocated without `check_gc`.
- Change: `check_gc()` after every native call (`precall_c`, results still anchored) and in
  `new_table_sized` (like `lua_createtable`). Regression: `tests/vm/gc_16_native_alloc_collected.lua`.
- Result: leak probe flat for all library functions; official 31/31, cargo test green.

## Arithmetic float fast path, bare jump table, integer FORLOOP on payloads
- `arith!`/`arithf!` test float+float before the generic `tonum` path: add_flt 1.54 -> 1.23.
- Dispatch: `Reserved*` arm is `unreachable_unchecked`; the undumper now rejects any opcode >= NUM_OPCODES
  ("bad binary format (invalid opcode)"), so the jump table has no range check.
- `Forloop` integer path reads/writes only payloads (forprep guarantees integer tags): empty loop 49 -> 37
  instructions/iteration (reference 28); loops_arith instr 1.39 -> 1.29.
- Gate: official 31/31, cargo test --workspace, clippy -D warnings.

## Table access fast paths, spill-free comparisons
- `get_int_arr` (array hit without nil normalisation) for GETTABLE/GETI: gettable_int 1.43 -> 1.24.
- `pset_existing_shortstr` / `pset_existing_int_arr`: inline in-place overwrite for SETFIELD/SETTABLE/SETI,
  falling back to the out-of-line `psetshortstr`/`psetint`: setfield 1.43 -> 1.07.
- LT/LE slow paths re-read their operands (`rd!`) instead of passing the early `Value` copies, which kept
  them spilled on the stack: lt_jump 1.39 -> 1.31.
- Tried and reverted: the same re-read trick on GETTABLE/GETFIELD/finish_set (instruction counts got worse).

## Store-forwarding stalls in slow paths (knucleotide 1.45 -> ~1.25 cycles)
- Finding (perf annotate): 85% of `finish_get` samples sat on a 4-byte load of a stack temporary
  that overlapped two earlier partial stores (copy of the `OpLoc` enum / of `Value` padding); then a
  16-byte vector load of a `Result` written in two 8-byte halves in `try_alloc`.
- Changes: `Value.tag` is now an 8-byte `tagw` (copies and field stores overlap exactly; `Value::store`
  writes 8 bytes); `finish_get/finish_set` no longer reassign an `OpLoc` (use `first`); `try_alloc`
  rebuilds its `Result` field by field and is `inline(always)`.
- Result: knucleotide cycles 6.19G -> 5.19G (ref 4.10G); gate: official 31/31, tests, clippy green.
- Lesson: when a hot function shows >30% on one instruction right after a stack store, look for a
  whole-aggregate copy of a value that was written field-wise.

- Correction: `Value::store` must keep writing the tag as ONE BYTE. The 8-byte tag store regressed
  table_array 0.74 -> 1.39 (cycles) and fib/closures/mandelbrot by 8-10%; with the byte store the
  geomean of cycle ratios is 1.057 (was 1.077) and knucleotide stays at 1.23.

## Constants pointer cached in the CallInfo; MOD split per operand kind; shifts take the int path first
- `k` is no longer an interpreter local (one fewer value live across the dispatch jump); `kv!` reads
  `ci.kcache`. loops_arith cycles 6.07G -> 5.54G (ref 4.39G), fib 4.99G -> 4.76G (ref 4.17G).
- `pc` is still spilled to the stack by LLVM (every instruction does load/add/store on `(%rsp)`) and
  `ls_bad_status2.stli_other` is 7x the reference on loops_arith (100M vs 13M): next target.

## GC checks at allocation sites (not after every native call); in-place array growth
- Correction to the earlier RSS entry: a `check_gc` after *every* native call changed collector pacing
  versus the reference (a young collection right after a table is filled promotes it, so sieve held two
  12M-element arrays: 298 MB vs 152 MB). The reference checks only where `lua_createtable`, `lua_pushcclosure`,
  `lua_newuserdatauv`, string creation and the parser do; ours now does the same: `new_table_sized`,
  closures in gmatch/coroutine.wrap/io.lines/package, io userdata, `load_chunk` (parser).
- `resizearray` grows in place with `realloc` (+ two `memmove`s of tags/values) instead of alloc+copy+free.
- Result: sieve RSS 1.74x -> 1.00x; leak probe (`tests/vm/gc_16_native_alloc_collected.lua`) still flat.

## load_big RSS: GC pace during a big load
- Cause: the reference parser has a GC safepoint per closed function (thousands per big chunk), so major
  cycles complete while loading; ours parses in Rust and has one safepoint after the load, so garbage
  of earlier loads accumulated (43 MB live vs 5 MB).
- Change: `load_chunk` takes `alloc / stepsize` collector steps (1..=256) after a successful load.
- Result: load_big RSS 54 MB -> 14 MB (ref 12 MB), time ratio still 0.47.

## Status after step 8 (5 runs, pinned core)
- time geomean 1.074 (an earlier 5-run pass measured 1.045: noise is about +-3%), worst loops_arith 1.36; cycles geomean 1.070.
- RSS now ~1.0 except fasta 1.35, string_ops 1.26, table_hash 1.19 (LuaString header is ~24 bytes larger than TString).
- Open: `pc` spilled to the stack by LLVM (loops_arith/mandelbrot), string layout.

## pc dead across protected calls
- `protect!`/`protect_nt!` re-read `pc` from `ci.savedpc` after the call (it was saved just before), so `pc`
  is no longer live across calls; LLVM now keeps it in a register and spills `base` (read-mostly) instead.
- Cycles (min of 5, before -> after): heapsort 5.87G -> 5.22G, queens 5.28G -> 5.05G, sieve 3.82G -> 3.70G,
  closures 5.97G -> 5.69G, loops_arith 5.51G -> 5.41G. Instruction counts are slightly higher.

## __index chain: short-string lookup in finish_get
- `finish_get` used the generic `TableRef::get` (tag jump table + call) for the second and later levels of an
  `__index` chain; it now uses `get_short_str` for short-string keys. oop_methods cycles ratio 1.26 -> 1.20.

## Memory: string header 48 -> 32 bytes; Miri-clean Value load/store
- `extra`/`shrlen`/`hash` moved into the padding of `GcHeader`; `len` and the bucket link `hnext` share a
  union (short strings keep their length in `shrlen`, as in the reference). `LuaString` is 32 bytes
  (reference: 24 + bytes at offset 24; going to 24 would need bytes addressed from `&LuaString`, which
  is not sound under Stacked Borrows, so the `contents` pointer stays).
- 1M-entry string-keyed table: 98 -> 82 bytes/entry (reference 74). Short-string objects 8 bytes larger
  than the reference instead of 24.
- `Value::load/store` copy the payload as the `Payload` union instead of a `u64` (a `u64` read drops
  pointer provenance; Miri flagged it in `gc_stress_workload_with_threads_closures_userdata`). Same
  machine code. Miri: all 87 `rn-core` lib tests pass.
- Pacing differences (string_ops peak 130 MB vs 80 MB) remain: transient `Vec`s in string functions
  are outside the GC accounting.

## Memory: build string.rep/upper/lower/reverse results in place
- `State::push_filled(len, fill)` / `Heap::new_str_filled` create the final string object and let the
  library write into it (the reference 5.5 `luaL_Buffer` also turns a big buffer into the string without a
  copy). Removes the transient 14 MB `Vec` next to the 14 MB result in fasta (hwm 57 -> 44 MB at that step,
  reference 43). Dead thread-local scratch buffer removed.
- Open: gsub/format/table.concat still build in a `Vec` and copy; string_ops peak (130 MB vs 80 MB) comes from
  collector pacing: ours reaches the `rep .. i` loop in a major (GenMajor) cycle with ~2x more garbage
  objects than the reference (1.2M vs 0.8M), so the sweep takes >400 steps and 60 MB of garbage piles up.

## Memory/pacing: library-buffer strings are not collector-visible (as in the reference)
- Observation: in the reference, `collectgarbage("count")` does not change when `table.concat` builds an
  8 MB string: 5.5's `luaL_Buffer` hands a big buffer (> `LUAL_BUFFERSIZE`) to the string as external memory
  allocated outside `luaM_`, so it never feeds GC debt. Ours counted it, which brought major cycles
  forward and left more garbage live (string_ops peak 130 MB vs 80 MB).
- Change: `StrKind::LongBuf` (inline bytes, only the 32-byte header counted; `Mem::alloc_split`/`free_split`
  keep the bytes in the memory *limit* but out of the collector's total). Results > 1 KB of rep/upper/lower/
  reverse (`push_filled`) and of concat/gsub/format/pack/dump/io.read (`push_buffer`) use it.
- Result: GC event sequence (young/major step counts) now matches the instrumented reference on the
  string-building phases; string_ops peak RSS 130 -> 107 MB (reference 80), fasta 58 -> 55 MB (reference 49).

## Memory: 24-byte string header (bytes right after the header), `StrView`
- `LuaString` is `hdr(16) + u(8)`; inline strings keep their bytes at offset 24 and external strings a content
  pointer there (`ExtFixedString`/`ExtString`), as in the reference. To keep this sound under Stacked Borrows a
  string is read through `StrView` (a `Copy` handle that keeps the whole allocation's provenance and derefs to
  the header only); `LuaString::as_bytes` no longer exists, raw users call `LuaString::bytes_raw(p)`.
- 1M-entry string-keyed table: 82 -> 73.85 bytes/entry (reference 73.85). RSS ratios now: fasta 1.04,
  string_ops 1.25, table_hash 1.00, knucleotide 1.05, json 1.01. Miri passes the string/heap/gc/table tests.

## Status after step 10 (5 runs, pinned core)
- time geomean 1.033, worst sieve 1.29; cycles geomean 1.032. RSS ratios 1.00-1.05 except string_ops 1.25,
  binary_trees 1.11. Phase 6 gate (geomean <= 1.05, worst <= 1.35) met.

## Memory: tables (56 -> 48 bytes) and closures (-8 bytes) match the reference
- `Table`, `LClosure`, `CClosure` start with the three `GcHeader` fields and keep their small fields
  (`flags`/`lsizenode`/`asize`, `nupvalues`) in the header's spare bytes (compile-time size assertions).
- Tried and reverted for `Udata`: the reference gives userdata 16-byte aligned payloads
  (the differential C API test `align16` caught it), so its header stays 48 bytes.
- binary_trees' peak RSS is a collector-timing coin flip (the same program peaks at 45 or 66 MB depending on
  where a major cycle lands); object sizes are now identical to the reference.

## Phase 7 (beating the reference): measurement caveat
- Code placement alone moves interpreter-bound benchmarks by 5-20%: the same `run` machine code (byte-identical
  after normalising addresses) shifted by 0x270 bytes took heapsort from 4.64G to 5.14G cycles and queens from
  5.02G to 5.39G, with identical instruction, branch-miss and op-cache counts. `-align-all-functions=6` /
  `-align-all-nofallthru-blocks=4|5` only reshuffle the luck (oop_methods 1.17 -> 1.43 with one of them).
  Judge interpreter changes by instruction counts plus the geomean over many benchmarks, never by one program.

## Small-object pool allocator (`PoolAllocator`)
- Hypothesis: the reference pays glibc `malloc`/`free` (8-byte header, 16-byte rounding, ~15% of binary_trees);
  the collector always frees with the exact size (`lua_Alloc` contract), so a header-less segregated free list
  is both faster and denser.
- Change: `alloc::PoolAllocator` (16-byte size classes up to 512 bytes, carved from 256 KB arenas, larger blocks
  from `malloc`; arenas released with the state) is the allocator of `State::with_seed` (rn-cli, rn-std).
  C API states keep their `lua_Alloc`; `RUSTYNAIL_SYSTEM_MALLOC=1` (set by `scripts/asan.sh`) falls back to malloc
  so ASan still sees use-after-free; the fuzz sandbox uses its own capped `SystemAllocator`.
- Result (bench.py, 5 runs, cycles ratio vs reference, before -> after): binary_trees 1.09 -> 0.87, gc_churn
  1.01 -> 0.92, string_ops 1.14 -> 1.00, json 1.10 -> 1.00, varargs 1.14 -> 1.02, pcall_error 0.88 -> 0.79,
  metamethods 1.15 -> 1.06; RSS binary_trees 1.35 -> 1.11, gc_churn 0.98 -> 0.91, table_hash 1.02 -> 0.95,
  string_ops 1.25 -> 1.17. Interpreter-only programs moved +-10% from code placement only (see above).
- Kept.

## Interpreter: no opcode live across arms, closure from `base - 1`, floats loaded as floats
- `op` stayed live after the jump-table dispatch because the shared `Idivk | Idiv` arm and the `Mmbin*`
  arm re-tested it (`op == OpCode::Idivk`, inner `match op`): every instruction paid a `mov`/`and` and LLVM
  allocated registers around it. Split into separate arms (`idivop!` macro). Instructions -5..-9% and cycles
  -3..-11% on fib/loops_arith/sieve/heapsort/mandelbrot/queens/fannkuch (min of 3, `perf stat`).
- `lcl!()` (GETUPVAL/SETUPVAL/GETTABUP/SETTABUP/CLOSURE) loads the closure from `base - 1` (one load) instead of
  re-deriving the function slot from `ci.func` and the stack pointer: getupval cycles -13%, setupval -26%,
  heapsort -7%, queens -4%.
- Arithmetic and LT/LE fast paths test both tags from memory and read float payloads straight into float
  registers (`Value::load_float_payload`): the `Value` copy put the payload in a GPR and the `movq` to XMM sat on
  the dependency chain. add_flt/mul_flt cycles 1.18 -> 1.00 of the reference, float_for -10%, loops_arith -4%,
  mandelbrot -4%.
- Kept (official 31/31, tests, clippy).

## Two interpreter copies: no per-instruction `trap` test in the fast one
- Hypothesis: the `ci.trap` test at the top of the loop is a separate block every arm jumps to; removing it
  (experiment: hooks broken) cut instructions 8-10% and cycles 5-10% on loops/fib/sieve.
- Change: `run_mode::<HOOK>` is instantiated twice. `trap` only becomes set inside calls/metamethods/GC/stack
  reallocation (`protect!`, `check_gc!`, frame transitions), by an asynchronous `lua_sethook` or by the interrupt
  flag; the fast copy (`HOOK = false`) checks `ci.trap | INTERRUPT` only at those points and at jumps (where the
  reference's `updatetrap` reloads `trap`). If hooks or an interrupt need tracing, it finishes the current
  instruction (`trapped` flag, constant-false on paths without a check) and hands the frame to the hooked copy
  (`resume`: no second call hook); the hooked copy hands back at a frame transition once no hook is left. A
  `trap` left by a stack reallocation alone is cleared (base is reloaded after every call anyway).
- Regression test: `tests/vm/dbg_02_hook_mode_switch.lua` (hooks set by a native call, inside `__index`/`__lt`,
  in a callee, a count hook before a call-free loop, a hook on another coroutine); Ctrl-C still interrupts
  call-free loops.
- Result (min of 3, ours before -> after): instructions -6..-14% (move -14%, loops_arith -7.5%, heapsort -9%);
  cycles loops_arith -9%, heapsort -7%, fannkuch -7%, mandelbrot -6%, fib -4%, sieve -3.5%, queens -3%.
- Kept (official 31/31, tests, clippy).

## Call/return: constants pointer set once per frame, field-wise argument reads
- `ci.kcache` is set where a Lua frame is created (`pre_call`, `pre_call_lua_fast`, `pre_tail_call`, which
  already hold the prototype) instead of at every (re)start of a frame, so a return no longer walks
  closure -> proto -> k; RETURN1 stores its result through `base - 1`.
- `State::arg` read arguments as a whole-`Value` copy of a slot the VM had just written field-wise
  (store-forwarding failure, 44% of `str_sub` samples): it now uses `Value::load`. knucleotide
  `ls_bad_status2.stli_other` 26M -> 6.4M (reference 28M).
- Result (min of 3, before -> after): call_lua instructions -6% / cycles -5%, call_arg_ret -6% / -9%,
  fib instructions -6%, knucleotide cycles -2..-5%.
- Kept (official 31/31, tests, clippy).

## PGO build (`scripts/pgo.sh`; the release default since 2026-11-11)
- BUNDLE-A: the recommended release build with the JIT is `scripts/pgo-build.sh` (README, "PGO release build"); the
  training list lives in `scripts/train.sh` only and now includes the benchmark sets (`PGO_HOLDOUT=1` keeps them out).
  The profile must be regenerated (re-run the script) after every change to a hot path. The note below describes the
  original (disjoint-mix, no-JIT) build.
- Instrumented build trained on bench/micro, tests/vm and 17 official test files (disjoint from
  bench/programs), then `-Cprofile-use` into `target/pgo/release/rnlua`. (At the time of this note it was not the default build; it is now the release default, see "Release benchmark" above.)
- Library-heavy programs gain most (json 0.95 -> 0.84, knucleotide 1.14 -> 0.96, string_ops 1.07 -> 0.86,
  varargs 0.96 -> 0.72, gc_churn 0.94 -> 0.76 cycles vs reference); a few interpreter loops lose
  (heapsort 1.01 -> 1.13, matmul 1.01 -> 1.08, nbody 0.98 -> 1.11), consistent with the placement noise above.

## Status after step 11 (bench/results/step11.json, 5 runs, pinned core)
- Default release build: time geomean 0.935 (cycles 0.932), worst sieve 1.20; the step-10 binary measured
  1.035 in the same run. Faster than the reference on 13/25 (binary_trees 0.79, pcall_error 0.75, fasta 0.84,
  gc_churn 0.87, table_hash 0.92, json 0.96, varargs 0.98, queens 0.99, plus coroutines/load_big/table_array).
- PGO build: time geomean 0.863 (cycles 0.870), worst sieve 1.22.
- Tried and reverted: tag-pair (`tag1 | tag2 << 8`) arithmetic dispatch (no gain); dropping the
  interrupt poll alone (RA got worse); LLVM `-tail-dup-*` options (dispatch not replicated);
  `-C relocation-model=static` (absolute jump table, -5% instructions, -2..5% cycles) would make
  `librn_capi.a` unlinkable into PIE programs, so not adopted.

## Phase 7b: native calls inline in OP_CALL
- Hypothesis: a call of a native function went `pre_call_lua_fast` (miss) -> `pre_call` -> `precall_c` ->
  `f` -> `post_call` -> `move_results`, all out of line (~80 instructions per call vs ~57 in the reference).
- Change: `State::pre_call_native_fast` (inline, tried after the Lua fast path so Lua calls pay nothing
  extra): light natives and native closures, no hook, `LUA_MINSTACK` free slots and a spare `CallInfo`;
  frame setup, the call and the 0/1-result `post_call` inline (anything else falls back to `post_call`).
- Result (instructions vs the step-11 binary, min of 3): call_c micro 0.68, knucleotide 0.92, varargs 0.88,
  json 0.94, coroutines 0.89, pcall_error 0.97; Lua-call programs +-0.5%. An out-of-line variant
  (`inline(never)`) gained less (call_c 0.77, knucleotide 0.94). Checking natives first (before the Lua fast
  path) cost fib/call_lua +3-4% instructions.
- Kept (official 31/31, tests, clippy).

## Absolute dispatch jump table without breaking `librn_capi.a` (`scripts/build-fast.sh`)
- With fat LTO the whole program is generated in the final crate's rustc session, so
  `cargo rustc --release -p rn-cli --bin rn-cli -- -C relocation-model=static` makes every `match` jump
  table absolute (`jmp *table(,%rax,8)`) while all library crates, and the `rn-capi` staticlib built by
  its own session, keep the default PIC model. The executable becomes non-PIE (still `-rdynamic`, C modules
  load: checked with `tests/c/samplemod.lua`). Cargo cannot scope rustflags to one package on stable, so
  this is a separate script (binary `target/fast/release/rn-cli`), not `cargo build --release`.
- Result (same source, min of 3, vs reference): instructions geomean 0.996 -> 0.945, cycles 0.924 -> 0.887;
  loops_arith cycles 1.10 -> 0.95, mandelbrot 1.06 -> 0.93, spectral_norm 1.08 -> 0.95, sieve 1.18 -> 1.14,
  fib 1.02 -> 0.98 (oop_methods 1.12 -> 1.18: placement). Official files pass with the fast binary.

## `__index` chains of tables without `finish_get` (GETFIELD/SELF/GETTABUP)
- Hypothesis: a method call through a class (`obj:m()` with `m` in `getmetatable(obj).__index`) left the
  inline path for `finish_get` under `protect!` (savestate, out-of-line generic loop, reload).
- Change: after a raw miss with a short-string key, `index_chain_short_str` (out of line) follows up to 4
  `__index` tables, returns nil when a level has no metatable or a flag-cached absent `__index`, and
  `ABSTKEY` ("general path") for functions / uncached absence / longer chains. Test:
  `tests/vm/meta_20_index_chain_inline.lua`.
- Register allocation lesson: inlined, the helper cost fib/queens/heapsort +3-5% instructions; out of line
  LLVM still spilled `base` at *every dispatch* (`mov %r11,0x18(%rsp)` before the jump table) because it was
  live across the call. A `reload!()` right after the call makes `base` dead across it and removes the
  spill (Lua-only programs back to +-0.5%).
- Result (fast build, instructions / cycles vs reference, before -> after): oop_methods 1.014 -> 0.877 /
  1.08 -> 0.98, metamethods 1.039 -> 0.982 / 1.06 -> 1.01; geomean instructions 0.945 -> 0.939.
- Kept (official 31/31, tests, clippy).
- Follow-up: the same `reload!()` after the other out-of-line table calls of the fast paths (`get`,
  `psetshortstr`, `pset`/`psetint`, `len`): instructions geomean -0.2%, cycles -0.8% (fib -3% instructions,
  sieve +1.5%). Kept (small but both geomeans improve).

## GC: one size computation per marked object, prefetch in the sweep
- `reallymark` and `propagatemark` each called the out-of-line `objsize` (a second type switch) besides
  their own switch; the size (same values, so the pacing is unchanged) is now computed in the arm of each
  kind. `sweep_some` (64% of its samples on the `marked` load: a pointer chase) prefetches the next
  node's header while the current one is processed/freed.
- Result (fast build, min of 5, before -> after): gc_churn instructions -2.9%, cycles -5.8%; binary_trees
  instructions -0.6%, cycles -4.9%; others unchanged. Kept (official 31/31, tests, clippy).

## PGO training mix (`bench/train`), PGO + absolute jump table; BOLT
- `bench/train/{numeric,objects,text}.lua`: program-like workloads written independently of
  bench/programs (LU solve, game of life, Collatz, class hierarchies, a matrix type with metamethods,
  proxies, varargs/pcall, coroutine scheduler, CSV parsing, templates via gsub, serializer + load,
  sorting with comparators, string/table/utf8 library calls). `scripts/pgo.sh` trains on them first
  (`PGO_TRAIN_PROGRAMS=0` = previous mix) and builds the final binary with `relocation-model=static` on
  the binary crate (as `scripts/build-fast.sh`; `PGO_PIE=1` for a PIE).
- Result (min of 3, vs reference, instructions / cycles geomean): fast build 0.936 / 0.871, PGO with the
  old mix 0.907 / 0.847, PGO with the new mix 0.901 / 0.851. New mix: table_array 0.79 -> 0.68,
  spectral_norm 1.01 -> 1.00, loops_arith 1.046 -> 1.034 instructions; cycles within noise. PGO still
  loses on interpreter loops versus the fast build (fannkuch 1.04 vs 0.96, heapsort 1.04 vs 0.98,
  matmul 0.97 vs 0.92 cycles): the profile moves blocks of `run_mode` around, see the placement caveat.
- BOLT (at the time of this step; see the final report for the later BOLT builds): `llvm-bolt`/`perf2bolt` are not installed on this machine (not in the rustup LLVM tools either), so
  not evaluated.

## Table fast paths: tag test without an `Option<TableRef>` null test
- `if let Some(t) = v.as_table()` compiled to the tag compare *and* `test ptr,ptr; je` (the `Option` niche
  test survives: `NonNull::new_unchecked`/`assert_unchecked` hints in `from_raw` did not remove it, the
  assumption is dropped when LLVM turns the tag branch into a select). The hot arms now test the tag and use
  `Value::table_unchecked`/`strref_unchecked` (GETTABLE/GETI/GETFIELD/GETTABUP/SELF/SET*/LEN).
- Result (fast build, min of 3): instructions geomean -1.0% (oop_methods -4.5%, nbody -4.2%, matmul -2.6%,
  heapsort/fannkuch -1.7%); cycles geomean +1.0%, all of it mandelbrot +15% / loops_arith +8% with identical
  instruction counts (op-cache misses 0.76G -> 1.26G on mandelbrot: code placement, see the caveat above).
  Kept by the instruction-count rule (official 31/31, tests, clippy).
- Tried and reverted: splitting SETTABLE/SETI/SETFIELD into a fast block whose operand copies are never
  passed by reference (+ a by-value barrier): LLVM then spilled `base` and reloaded it from the stack at
  every dispatch (+3.6% instructions everywhere).

## Not adopted: folding the interrupt flag into `ci.trap`
- Upper bound measured first by simply dropping the `INTERRUPT` poll from the fast `updatetrap!` (what a
  fold would leave: one `cmpl $0, trap(ci)` per jump/FORLOOP; a real fold also needs a volatile read and a
  signal handler that marks every `CallInfo` of the running thread). Fast build, min of 3: empty loop
  -13% instructions, but loops_arith +0.5%, mandelbrot +2.3%, spectral_norm +2%, heapsort +2%, nbody +2.3%
  (register allocation of the dispatch changes), geomean instructions -0.5%, cycles +3%. Same outcome as the
  earlier "drop the poll alone" attempt; not worth the signal-handler complexity.
- Follow-up: `ccall` (TFORCALL iterators, metamethods, `lua_call`) also tries `pre_call_native_fast`
  before `pre_call`: coroutines -5.3% instructions, json/table_hash -0.9%, others +-0.5%
  (knucleotide +4.6% cycles at -0.4% instructions: op-cache misses +26%, placement). Kept.

## `#[cold]` on metamethod / error slow paths
- `try_bin_tm`, `try_bini_tm`, `try_bin_assoc_tm`, `call_orderi_tm`, `less_than`, `less_equal`, `obj_len`,
  `run_error`, `type_error`, `call_error` are `#[cold]`: LLVM moves the blocks that call them out of the
  hot handlers (more compact interpreter code). Fast build, min of 3: instructions -0.3%, cycles -1.6%
  (mandelbrot -12%, loops_arith -8%, fib -5%, closures -6%; queens +3%). Kept (official, tests, clippy).

## Phase 7c (step 13)

### SET*/GETTABLE: no address-taken operand copies
- Finding (objdump of SETTABLE): the key and value copies were stored to the stack (`mov %rax,0x78(%rsp)`,
  4 stores per SETTABLE) on every execution because `psetint`/`pset`/`finish_set` take `&Value`; the array
  store reloaded `t->array` after the tag store (possible alias).
- Change: the slow path of SETTABLE/SETI/SETFIELD/SETTABUP is `finish_set_op(st, ci, i, hres)` (cold, out of
  line) which re-reads the operands from the frame; `pset`/`psetint`/`psetshortstr` take `Value` by value;
  GETTABLE's generic key path calls `TableRef::get_by_val` (out of line, by value); `arr_set` computes both
  addresses before storing. (Unlike the reverted split attempt, nothing live crosses into the calls, and
  `reload!` already follows them.)
- Result (default build, min of 3, instructions / cycles vs step 12): sieve -4.9% / -8.6%, heapsort -3.1%,
  matmul -3.8% / -4.7%, fannkuch -3.9% / -3.6%, queens -2.2%; geomean instructions -0.9%, cycles -0.7%.
- Tried and reverted: an inline short-string branch in GETTABLE (`t[k]` with a string variable): gettable_str
  micro -27% instructions but fib +5% / heapsort +3% (register allocation of the whole loop).

### OP_CONCAT: results built in place, no buffer
- Finding: `a = s .. s` (1-byte strings) cost ~515 instructions per concatenation vs ~343 in the reference;
  only ~2 per call were in the string table chain walk. The rest: a general loop with out-of-line
  `concat_tostring` calls, a zeroed 40-byte stack buffer, `copy_from_slice` + `memcmp` calls, generic
  `bytes_raw` per operand. (`perf record -e instructions:u` attributions are useless here: samples pile up
  behind stalled loads; deltas between builds with a code path skipped were what located the cost.)
- Change: `State::concat` first scans the operands from the top down (converting numbers in place exactly
  where the reference does; metamethods, empty operands and long-string identity go to
  `concat_general`, the old loop). Short results: `Heap::intern_concat` hashes and compares the operands
  piecewise against the bucket chain (hash compared before length/bytes; tiny parts compared bytewise)
  and fills a new string straight from them; long results: `new_long_str_with` writes into the new
  (collector-counted, `LongReg`) string. `intern_short` also compares the stored hash first.
- Result (instructions / cycles vs reference): concat micro 1.47 / 1.34 -> 1.18 / 1.00; 1+30-byte concat
  1.16 / 0.89 -> 1.02 / 0.81; table_hash (`"key" .. i`) 0.96 -> 0.93 instructions. Regression test:
  `tests/vm/concat_06_inplace_boundaries.lua` (short/long boundary, empties, `%p` identity, numbers next to
  `__concat` operands - the official `events.lua` caught a first version that converted those).
- Noise note: programs whose hot paths hash strings (oop_methods, metamethods, table_hash) vary by +-2%
  instructions from run to run (random string-hash seed -> different chain collisions); min-of-3 is not
  enough to judge 1% changes on them.

### Pattern matcher: positions instead of `Result<Option<usize>, Vec<u8>>`
- string_ops spends ~45% in the matcher (gmatch/gsub); ours took ~1G more instructions than the reference's
  `match`. Every recursion level and every `classend` returned a 24-32-byte `Result` through memory.
- Change: internal functions return a `usize` (position, `NOM` = no match, `ERR` = error with the message
  in `MatchState::error`); the public `do_match` keeps its `PResult<Option<usize>>` signature.
- Result: string_ops instructions 1.219 -> 1.188 of the reference (with the concat change), cycles 0.95 -> 0.92.

### Store-forwarding failures in call_c / self_call: not a bottleneck
- `ls_bad_status2.stli_other` per call (min of 3): call_c ours 6.4 vs reference 1.8, self_call 2.5 vs 0.14,
  but call_arg_ret 4.5 vs 3.9 and `local f = o.get; f(o)` 3.1 vs 3.2: the counts swing by 10x between
  near-identical programs in *both* interpreters (they follow the return-value / argument slots that are
  written and re-read within a few dozen instructions), and our cycles are lower than the reference's on
  all of them (call_c 2.81G vs 3.26G, self_call 4.05G vs 4.93G).
- All CallInfo / stack-slot stores and loads in the call path were checked in the disassembly: same widths
  and offsets (no vector copies left). Tried: reading `callstatus` with one 32-bit load instead of the byte
  tests LLVM narrows it to (`testb $1,0x5e(ci)` after a 32-bit store): stall count unchanged (or higher),
  +0.7% instructions; reverted. IBS (precise load sampling) is not available to unprivileged users here
  (`perf_event_paranoid` = 2), which would be the tool to pin the remaining ones down.

### `#[cold]` on more out-of-line slow paths
- `finish_get`, `pre_call` (generic call path), `table_error`, `try_concat_tm`: instructions geomean -0.1%,
  cycles +0.4% (noise level); mandelbrot op-cache misses 1.00G -> 0.74G. Kept.
- Tried and reverted: a cold `finish_get_op(st, ci, i)` for the GET* slow paths re-reading the operands (as
  `finish_set_op`): +1.3% instructions geomean (loops_arith +3.1%, matmul +4.8% - none of them reach the
  slow path: register allocation of the whole loop changed).

### Not done: per-instruction inline cache for GETFIELD/SELF
- Short-string lookups that hit the main position already cost a mask, a load and a compare; an IC (side
  array per prototype indexed by `pc - code`) would need the prototype's code base in the handler (one more
  live value or two dependent loads) and saves only chain walks and the `__index` level of method calls,
  which `index_chain_short_str` already handles out of line (oop_methods is at 0.84x the reference's
  instructions). Given that every handler-level change so far moved unrelated loops by +-3% (register
  allocation), the expected net gain did not justify it in this round.

### Static-relocation build as the default: not feasible on stable
- The absolute jump table needs `-C relocation-model=static` in the final (LTO) rustc session of rn-cli.
  Stable Cargo cannot scope rustflags to one package (`profile-rustflags` is nightly-only; profile package
  overrides do not cover codegen flags; `cargo:rustc-flags` from build.rs only accepts `-l`/`-L`), and a
  global `rustflags` would also build `librn_capi.a` non-PIC, so C programs linking it as PIE (the
  `rn-capi` differential tests, any user) fail to link. `scripts/build-fast.sh` stays the way to get it.

### Status after step 13 (bench/results/step13.json, 5 runs, pinned core)
- Default build: time geomean 0.883 (step 12: 0.889), instructions 0.952 (0.961); worst metamethods 1.07,
  sieve 1.06 (instructions 1.13 -> 1.07), oop_methods 1.05 (instructions 0.86), knucleotide/loops_arith 1.03.
- Fast build: time geomean 0.864 (0.868), instructions 0.915 (0.920); worst metamethods 1.06, sieve 1.03.
- Open: string_ops instructions 1.19 (pattern matcher: per-call `MatchState` init and `Vec` of captures in
  gmatch/gsub, `classend` re-scanning), loops_arith instructions 1.12 (dispatch: relative jump table),
  metamethods (`__index`/`__newindex` functions through `finish_get`/`finish_set`).

## Phase 7d (step 14)

### Pattern matcher: start-position filter, table-driven classes, no capture `Vec`s
- Finding (string_ops split into phases, instructions ours/ref): the gmatch loop 1.21, gsub 1.26. Per
  start position the matcher ran `m` -> `match_inner` -> `classend` -> out-of-line `singlematch` ->
  out-of-line `match_class` (lower-casing + range tests); every match allocated a `Vec` of captures;
  `add_s` pushed the replacement byte by byte; plain `find` was `windows().position()` (a `memcmp` per
  position).
- Changes: `MatchState::start_filter` (the first pattern item is a literal or a single-character class
  without `*`/`?`/`-`, no capture/`%b`/`%f`/back reference) lets `find`/`match`/`gmatch`/`gsub` skip
  start positions where that item cannot match (word-at-a-time byte search for literals, inline
  `singlematch` for classes); the reference tries `match` at every position. `match_class` uses two
  256-entry tables (`CTYPE`, `CLBIT`) and is inlined with `singlematch`; captures are pushed straight
  from the `MatchState` (`find_with`, `push_captures`, `Matched::get`); `add_s` copies runs between `%`
  escapes; `memfind` searches the first byte and compares the rest. (The 512-byte capture array
  zeroing per `gmatch_aux` call was measured at ~13M instructions of 6.3G: not worth unsafe code.)
- Result (default build, instructions vs the step-13 binary): string_ops 0.80 (vs reference 1.19 ->
  0.94; cycles 0.79 of the reference), gmatch phase 0.89, gsub phase 0.78; others unchanged.
  Regression test: `tests/vm/strings_04_pattern_start_filter.lua`.

### SELF on strings without `finish_get`
- Finding: knucleotide spent 19% of its instructions in `finish_get`: `seq:sub(i, j)` (SELF on a string)
  always took the general path (`protect!`, `get_tm_by_obj`, a second loop iteration).
- Change: `index_string_short_str` (out of line, like `index_chain_short_str`): string metatable ->
  `__index` table -> raw short-string lookup (then its chain); anything else goes the general way.
- Result (instructions / cycles vs the previous step, min of 3): knucleotide 0.919 / 0.917, json 0.955 /
  0.979, coroutines 0.988, string_ops 0.990; all others within +-0.9% (hash-seed noise); geomean
  instructions 0.994, cycles 0.996.

### `s .. ""`: empty-operand shortcut in a small `concat` entry
- Finding: `x = s .. e` cost ~194 instructions per concatenation vs ~122 in the reference: `concat`'s
  scan broke on the empty operand and called `concat_general`, which handled it; the scan function's
  prologue (six callee-saved pushes, a 0x88-byte frame) ran before any test.
- Change: `concat` is a small entry (one push) testing `total == 2`, both tags string, one of them
  empty; everything else tail-jumps to `concat_scan` (the old body). Tag tests come before
  `as_strref` (which also null-tests the pointer: +7 instructions on the non-empty path).
- Result: empty-concat micro 194 -> 131 instructions per concatenation (1.47 -> 1.06 of the
  reference); non-empty concatenations +4 instructions (`"key" .. i` +0.07%).
  Regression test: `tests/vm/strings_05_int_tostring_concat_empty.lua`.

### Integer -> string without `core::fmt`
- Finding: `"key" .. i` spent most of its time in `concat_tostring` -> `write!(b, "{i}")`
  (`core::fmt::write` + `<i64 as Display>::fmt`, ~150 instructions).
- Change: `number::tostring_int` writes two digits per division from a 200-byte table.
- Result (instructions vs the previous step, min of 5): table_hash 0.90 (cycles 0.977), gc_churn 0.965,
  pcall_error 0.978; `"key" .. (i & 1023)` micro 0.88 (0.65 of the reference, which uses `snprintf`).

### Bitwise operations: integer tags tested in memory
- `BAND`/`BOR`/`BXOR`/`SHL`/`SHR` and the `K` forms copied both operands as `Value`s and went through
  `as_int()`; `bitwisep!` tests both tags in memory and reads integer payloads directly (as `arith!`),
  falling back to the old path otherwise.
- Result (instructions, min of 3/9): band micro 0.91 (1.13 -> 1.02 of the reference), loops_arith 0.992;
  geomean 0.998 (nbody/json moved +-1% between runs: string-hash seed).

### `string.format` without per-item allocations
- Finding: `string.format("%d:%s:%5.2f", ...)` cost 1.31x the reference's instructions: per item a `Vec`
  for the specification, another for the `ll` variant, a NUL-terminated copy, a zeroed 512-byte
  `snprintf` buffer, the result `Vec`, and a copy of every `%s` argument; literal text was pushed byte
  by byte.
- Change: specifications are built in a stack array (`Form`); `cfmt::fmt_*` append to the output,
  `snprintf` writing straight into its spare capacity (`run_into`); literal runs are copied in bulk;
  plain `%d`/`%i` use `number::tostring_int` (same digits as `%lld`); `%s` without modifiers appends
  through `FormatSource::tolstring_into`.
- Result (instructions): format micro 0.73 (0.95 of the reference), string_ops 0.88 (vs reference
  0.94 -> 0.83), json 0.998, load_big 0.993. Regression test: `tests/vm/strings_06_format_paths.lua`.
- Found on the way (fixed): `print` did not call a `__tostring` of the string metatable for string
  arguments (`luaL_tolstring` does).

### `next`: short-string key lookup; native calls with 0/2/3 wanted results inline
- Finding (pairs_loop micro 1.11x the reference): `next` found the previous key's node with the generic
  `getgeneric` (`mainposition` type switch, `equalkey` generic compare), and TFORCALL's call of `next`
  (2 results wanted) left `pre_call_native_fast` for the general `post_call`.
- Change: `findindex` follows the chain of a short-string key comparing pointers (live or dead key);
  `pre_call_native_fast` moves results inline for 0..=3 wanted results (multiple results and
  to-be-closed slots still go through `post_call`).
- Result (instructions): pairs_loop micro 0.79 (0.88 of the reference); pcall_error 0.979, table_hash
  0.989, varargs 0.994; others +-0.5%. Regression test: `tests/vm/tables_20_next_shortstr_native_results.lua`.

### Registers addressed as `base` + byte offset
- `rd!`/`wr!`/`rp!` compute `base.byte_add(index * 16)` (trait `Reg` for `usize`/`u32`) instead of
  `base.add(index)`: same addresses, but LLVM fuses the operand extraction and the scaling of B/C into
  one shift + mask more often.
- Result (instructions / cycles vs the previous step, min of 3): loops_arith 0.963 / 0.968, mandelbrot
  0.956 / 0.975, spectral_norm 0.976, sieve 0.986, matmul 0.991, knucleotide 0.986; oop_methods 1.015 /
  1.05 (min of 9: SELF now spills register A around its slow path); geomean over the 25 programs about
  0.994 instructions, 0.997 cycles.
- Tried: extracting A itself as a byte offset in the dispatch (`(i >> 3) & 0xff0`, a newtype for the
  register): dispatch length unchanged, `base` spilled to the stack (one extra load per handler);
  geomean -1.4% instructions on interpreter loops but fib +1.7%, closures +0.9%. Not kept.

### Tail calls of Lua functions: tested first, arguments moved through one stack pointer
- Finding: closures spent 28% in `pre_tail_call` (`compose` chains: `return f(g(x))`); its Lua branch came
  after three native-function tests and moved the arguments with `get`/`set` (stack pointer reloaded per
  value, `ci.func` re-read per value).
- Change: Lua closures are tested first; function and arguments are copied down with `Value::load/store`
  through two raw slot pointers; the `CallInfo` fields are written once.
- Result (instructions): closures 0.971 (1.03 -> 1.00 of the reference); others unchanged.
- Tried and reverted: constants addressed as `kcache` + byte offset (like registers): geomean -0.1%,
  sieve +1.1% (noise level).
- Tried and reverted: the conditional-jump handlers share a tail that materialises the `trapped` flag
  (`xor`/`test`/`je` on the not-taken path). Handing over to the hooked interpreter directly from
  `dojump!`/FORLOOP (`updatetrap_last!`, the jump being the last action of its instruction) removed it,
  but the loop's register allocation changed: geomean +1.0% instructions, +0.9% cycles.

### Metamethod calls (measured, not changed)
- Per call of a Lua metamethod, ours vs reference instructions (micro, one metamethod per iteration):
  `__index` function +45, `__len` +54, `__call` +92, `__add` +106, `__eq` +116, `__lt` +128,
  `__newindex` function +135, `__concat` +144; a direct Lua call and `pcall(f)` are at or below the
  reference. The extra cost is spread over `try_bin_tm`/`call_bin_tm` (Results through memory),
  `finish_set_op` + `finish_set` (~145 instructions for `__newindex`), `ccall` (~70) and re-entering
  `run_mode` (prologue, frame checks, `run`'s mode loop). The metamethods benchmark is still 1.00x
  the reference in instructions because its other operations are cheaper.
- Idea not done (see remaining ideas): run Lua metamethods in the *same* `run_mode` invocation (push the
  frame like OP_CALL, mark it, and apply `finish_op` to the caller when it returns), which would remove
  `ccall`/`run_mode` re-entry. Needs care with `nCcalls` accounting ("C stack overflow" limits),
  yields (resume resets `nCcalls`) and hooks.
- Valgrind/callgrind cannot run on this machine ("Illegal instruction" in `ld.so`), so per-function
  exact counts are not available; perf sampling skid makes `instructions:u` attributions approximate.

### Dispatch (measured, not changed)
- The fast interpreter's dispatch block is 11 instructions (`pc` copy, `a` extraction, relative jump
  table: `movslq` + `add` + `jmp *`, table base held in a register) plus a `jmp` back from every
  handler; FORLOOP's `updatetrap` costs 5 (trap + interrupt poll; the reference keeps `trap` in a
  register).
- Upper bound of tail-duplicating the dispatch into the handlers (`-C llvm-args=-tail-dup-pred-size=500
  -tail-dup-succ-size=500 -tail-dup-indirect-size=60 -tail-dup-size=60`, an experiment only: it needs
  rustflags): LLVM duplicated it into 14 handlers; instructions -0.7%, cycles -0.3% on 9 interpreter
  benchmarks. Not worth a build flag.

### Status after step 14 (bench/results/step14.json, 5 runs, pinned core)
- Default build: time geomean 0.861 (step 13: 0.883), instructions 0.919 (0.952); worst sieve 1.07,
  metamethods 1.06, queens 1.04, loops_arith 1.03; string_ops 0.71, knucleotide 0.93, table_hash 0.87.
- Fast build: time geomean 0.854 (0.864), instructions 0.879 (0.915); worst sieve 1.06, loops_arith 1.04.
- Open: Lua metamethods re-entering `run_mode` (+45..+145 instructions per call), dispatch (relative
  jump table, shared dispatch block, interrupt poll at jumps), sieve/heapsort/queens handler-level
  register allocation, `SETFIELD` double lookup before `__newindex`.

## Phase 7e (step 15, final pass)

### Lua metamethods run in the caller's interpreter loop
- Hypothesis (step-14 measurement): a Lua metamethod cost +45..+145 instructions per call over the
  reference, spent in `finish_get`/`finish_set`/`try_bin_tm` -> `call_tm_res` -> `ccall` -> a fresh
  `run_mode` (prologue, frame checks, `run`'s mode loop) and back.
- First attempt (reverted): keep the generic slow paths but let `call_tm_res`/`call_tm` push the frame
  and unwind to the interpreter with an internal `Status` (the frame finished by the generic
  `finish_op` on return). Instruction counts got *worse* (`__index` 401 -> 432 per call): the work
  only moved (`push_mm_frame` + `finish_op` + the second frame restart cost what `ccall` + re-entry
  did) while `finish_get`'s generic loop and `call_tm_res` stayed.
- Second attempt (reverted): one out-of-line launcher decoding the instruction (`mm_op(st, ci, i)`)
  at every site, or all slow paths jumping to one shared block (`break 'slow`): both changed register
  allocation of the whole loop (`pc`/`base` spilled, jump-table address re-materialised per dispatch):
  +3..+21% instructions on the plain micros.
- Kept: small out-of-line launchers called by the handlers' slow paths before their old slow path
  (`mm_index` for GETTABUP/GETTABLE/GETI/GETFIELD/SELF, `mm_newindex` for SETTABLE/SETI/SETFIELD,
  `mm_bin` for MMBIN/MMBINI/LT/LE/LTI../CONCAT of two operands, `mm_eq`, LEN): when the metamethod is a
  Lua closure, no hook is set and `pre_call_lua_fast` succeeds, the frame is pushed at `ci.top`
  (`mm_frame`, marked `CIST_MMRET`) and the interpreter `continue 'frame`s into it. `do_ret!` tests
  `CIST_FRESH | CIST_MMRET` in one go; `mm_return` writes the result to the caller's register (or does
  the conditional skip) like `finish_op` (which it calls for CONCAT/SET*). Yields inside such a
  metamethod resume through `unroll` and come back through the same `mm_return`; errors unwind like
  any Lua frame. `nCcalls` is incremented per in-place frame as `luaD_call` would (the old count is
  kept in the frame's unused `ctx`; `mm_return` decrements only if the count was not reset by a
  resume), so unbounded metamethod recursion still ends in "C stack overflow" at the same depth
  (`tests/vm/err_08_stack_overflow.lua` caught a first version without it). MMBINK/UNM/BNOT keep the
  old path: adding launchers there too crossed a register-allocation threshold (+1 instruction per
  dispatch on every program).
- Result (instructions per metamethod call, ref / before / after): `__add` 330/401/302, `__index`
  298/401/306, `__lt` 318/439/306, `__len` 262/313/259, `__concat` 384/545/372, `__eq` 315/431/335,
  `__newindex` 287/423/342. metamethods benchmark instructions 1.012 -> 0.898 of the reference
  (cycles 1.08 -> 0.98); geomean over the 25 programs 0.918 -> 0.915; plain micros unchanged.
  Regression test: `tests/vm/meta_21_inplace_metamethods.lua` (every instruction kind, polarities,
  vararg/tail-calling/multi-result metamethods, errors and tracebacks, yields across each kind,
  hooks set inside, recursion limits).

### SETFIELD: one key lookup before an insertion or `__newindex`
- SETFIELD tried `pset_existing_shortstr` inline and, on a miss, `psetshortstr` out of line, which looked
  the key up again before inserting it (or answering "go to `finish_set`" for `__newindex`).
  `pset_existing_shortstr_slot` now hands the lookup's node (or null) to `psetshortstr_after`.
- Result (instructions): `__newindex` call 342 -> 327, three new-key stores into a fresh table 3104 ->
  3056 per iteration; oop_methods 0.947, nbody 0.964 (both partly hash-seed noise), geomean 0.997.

### Varargs: slot copies with raw pointers
- `get_varargs` (no vararg table) and `adjust_varargs` (no vararg table) did every copy through
  `State::get/set` (thread -> stack pointer reloaded per value, `ci.func` re-read per value).
  They now call `vm_unsafe::varargs_fixed` / `adjust_varargs_fast` (one pointer per side; the copy is
  bounds-checked against the stack size once).
- Tried and reverted: handling `OP_VARARG` with a fixed count inline in the handler (and VARARGPREP
  calling the fast function directly): `local a, b = ...` -30 instructions per call, but the
  handler change moved register allocation elsewhere (nbody +3.3%, getfield micro +4%): geomean 1.000.
- Result: varargs benchmark 0.993 of the previous instructions (min of 5); the micro's call still costs
  ~+50 over the reference (`protect!` around both calls, `post_call` for vararg returns).

### Array stores into empty slots of tables without a metatable inline
- SETTABLE/SETI's inline store only overwrote non-empty array slots; filling a pre-sized array
  (`flags[i] = true` after the array part grew, `t[#t + 1] = v`) called `psetint` out of line to learn
  that there is no `__newindex`. The inline test now also accepts an empty slot when the table has no
  metatable at all (one more load on that path).
- Result (instructions): sieve 0.984; others within noise; geomean 0.9985.

### Tried and reverted in this phase
- Polling `trap`/the interrupt flag only on backward jumps (every loop has one, so Ctrl-C and
  asynchronous hooks would still be noticed): the sign test cost as much as the poll saved on forward
  jumps (while_loop micro +6%); geomean +0.2%.
- MOD/MODK with both integer tags tested in memory (as `arith!`): mod_int micro unchanged (LLVM already
  produced the same code from the `Value` copies), loops_arith +0.3%.

### Measured, not changed
- gettable_str (`t[k]`, string variable `k`): +11 instructions per access over the reference. The
  generic key path `get_by_val` is a tail jump to `TableRef::get`, whose ABI passes the key by pointer
  and returns the `Value` through memory (key spilled by the caller, three pushes, tag jump table). The
  inline short-string branch that would avoid it is on the do-not-redo list (register allocation).
- concat micro (`s .. s`, 1-byte strings): ~110 vs ~85 instructions per concatenation; by LBR (see
  tooling note) about 39 in `intern_concat` (hash + chain compare + new string), 25 in `concat_scan`, 7 in the
  `concat` entry, the rest in the handler (`protect_nt!` + `check_gc!`). No single piece dominates.
- Tooling note: `perf record -b` (AMD LBR, 16 entries) works here, unlike IBS/callgrind. Counting the
  instructions of each straight-line run between consecutive branch records (objdump addresses) gives
  exact per-function / per-instruction execution counts, free of the skid that makes `instructions:u`
  sampling useless in this interpreter (normalising to calls per iteration is only approximate: entry
  runs are under-counted). This is what located the costs above and in the metamethod work.

### Status after step 15 (bench/results/step15.json, 5 runs, pinned core)
- Default build: time geomean 0.864 (step 14: 0.861, within the +-3% run-to-run noise), instructions
  0.916 (0.919); metamethods 1.06 -> 0.95 time / 0.90 instructions. Still slower than the reference:
  sieve 1.08 (instructions 1.03), loops_arith 1.05 (1.06), heapsort 1.03 (1.04), queens 1.03 (1.06),
  closures 1.02; everything else at or below 1.00.
- Fast build (`scripts/build-fast.sh`): time geomean 0.857, instructions 0.877; worst sieve 1.07,
  heapsort 1.04, loops_arith/queens 1.02.

## Summary of Phase 6/7 (performance work, steps 1-15)
- Overall: time geomean 1.59x -> 0.86x of the reference (instructions ~0.92x), peak RSS ~1.0x except
  string_ops/binary_trees (collector pacing). The largest single wins, roughly in order: field-wise
  `Value` loads/stores (store-forwarding), the two interpreter copies without a per-instruction `trap`
  test, the small-object pool allocator, inline Lua/native call fast paths, the absolute-jump-table
  build, string-library rewrites (pattern matcher, `format`, concatenation), and in-place Lua metamethods.
- Method lessons: judge by instruction counts plus the geomean over many programs (code placement moves
  cycles by +-15% at identical instruction counts; the string-hash seed moves hash-heavy programs by
  +-2%); any handler-level change can move register allocation of the whole `run_mode` loop (watch the
  dispatch block in the disassembly: `base`/`pc` spills or a re-materialised jump-table address mean
  the change costs more than it saves); keep values dead across out-of-line calls (`reload!`, `pc`
  re-read from `savedpc`); prefer out-of-line helpers that re-read operands from the frame over
  passing them by reference; LBR (`perf record -b`) gives exact execution counts here.
- Remaining ideas (not done):
  - Interpreter-bound loops (sieve, loops_arith, heapsort, queens: +3..6% instructions): the relative
    jump table (+3 instructions per dispatch, removed only by the non-default fast build), the
    interrupt poll at jumps/FORLOOP, and per-handler register allocation. A tail-duplicated dispatch
    (one indirect jump per handler, e.g. via a `become`-style threaded interpreter once stable Rust has
    guaranteed tail calls) is the remaining structural lever.
  - MMBINK/UNM/BNOT and `__index` functions deeper than the first chain level still use `ccall`; a
    version of the launchers that does not disturb register allocation (e.g. after a future handler
    restructuring) would cover them.
  - Vararg calls (~+50 instructions per call in the micro): `protect!` around VARARGPREP/VARARG and the
    out-of-line `post_call` for vararg returns.
  - gettable_str / concat micros (1.14 / 1.30): see "Measured, not changed" above.
  - New-key stores into fresh tables (`{x = .., y = ..}` then `setmetatable`): ~1.13x the reference
    per insertion (`insertkey`/rehash path).
  - PGO as the default build. (BOLT: installed and evaluated later, see step 16 and the final report.)

## Status after step 12 (bench/results/step12.json, 5 runs, pinned core; `ours` = default build, `fast` = scripts/build-fast.sh)
- Default build: time geomean 0.889 (step 11: 0.935), instructions 0.961; worst sieve 1.16, knucleotide 1.07,
  metamethods 1.06, oop_methods 1.03, loops_arith/heapsort 1.02.
- Fast build (absolute jump table): time geomean 0.868, instructions 0.920; worst sieve 1.12, knucleotide 1.03,
  metamethods 1.03, oop_methods 1.01; faster than the reference on 21/25.
- Open: sieve (array append through `finish_set`, SETTABLE fast-path operands spilled because they are passed
  by reference to the slow path; restructuring made LLVM spill `base` instead), string_ops instructions 1.20
  (interning chain walks, `concat` buffer copies), code placement / op-cache sensitivity of `run_mode`.

## Lua 5.4 optimization (step 17, `--cfg rustynail_lua54`)
Baseline: the merged Phase 6b build (bench/results/lua54-vs-ref54.json: time geomean 0.863, instructions
0.976 of `ref/54/build/lua` 5.4.9; worst load_big 1.28, oop_methods 1.12, heapsort 1.08, queens 1.07).
Most of the Phase 6/7 work is in code shared by both versions (interpreter loop, calls, metamethods,
strings, pattern matcher, allocator) and already applied to 5.4; the 5.4-only code is the compiler
(`rn-compiler/src/v54`), the 5.4 opcode arms of `run_mode`, the 5.4 table policies (`alimit`) and
`gc/gen54.rs`. Every change below is `cfg(rustynail_lua54)` or 5.4-only code: the 5.5 release binary's
`.text` is byte-identical to the one before this step (checked with `objcopy --only-section=.text` +
`cmp`). Method as in Phase 7 (min-of-N `perf stat -e instructions:u,cycles:u`, pinned, geomean over the
25 programs; LBR/`perf annotate` to locate costs).

### Kept
- **SELF on a short-string key never took the fast path** (bug): the 5.4 `RKC` arm tested
  `rc.tag() == LUA_VSHRSTR`, but a stored string tag carries the collectable bit, so every method call
  went `mm_index` -> `finish_get`. Now `ctb(LUA_VSHRSTR)`. Instructions vs ref54: oop_methods 1.13 ->
  0.89, knucleotide 1.06 -> 0.96, json 1.00 -> 0.93, string_ops 0.84 -> 0.83.
- **5.4 compiler** (load_big 1.30 -> 1.02 instructions, cycles 1.24 -> ~0.95):
  - the constant cache (the scanner table `ls->h`, shared by all functions of a chunk) keyed string
    constants by a `Vec` copy of their bytes and hashed with SipHash: `KKey::Str` now holds the string's
    identity (`CompilerHost::str_id`: short strings are interned and the lexer keeps one object per long
    string, as `luaX_newstring` does), an FxHash-style hasher, and `addk` does one entry lookup;
  - `search_var`/`search_upvalue` compare names by identity (`eqstr` in 5.4 `lparser.c` is a pointer
    compare; `StrRef ==` read both string headers);
  - `testnext`/`check` compare token tags (`tok_is`) instead of calling the derived `Token::eq`;
  - lexer (5.4 cfg only): reserved words matched as a packed `u64` instead of byte-string compares, names
    and strings lent from the buffer to `new_string` instead of cloned;
  - parser results are `Result<_, Box<CompileError>>` (`v54::R`, errors via the cold `perr`): the
    24-byte `Result<_, CompileError>` went through memory in every parser function (-1.6% on load_big).
- **Integer `forprep` through one stack pointer** (`vm_unsafe::forprep_int54`, called first by
  `State::for_prep`): fannkuch/queens -0.8% instructions; the interpreter loop is unchanged.

### Tried and reverted
- Lexer: scanning a name directly in the input slice and copying it to the buffer with one
  `extend_from_slice` (+3.4% instructions on load_big: a `memcpy` call per name beats nothing); building the
  packed reserved-word key while scanning (+1.5% instructions); two overlapping loads to pack the key (same
  count, the 2-byte reload of just-stored bytes stalls on store forwarding); boxing the lexer's own error type
  so `R<Token>` returns in registers (-0.2% instructions but +11% cycles on load_big, reproducible).
- `alimit` (the 5.4 array limit, `alimit_touch` on every array access): upper bound measured by skipping
  the touch: +2.4% instructions (register allocation of `run_mode` got worse, the touch itself is a flag test
  and a branch). Out of line as a cold call: table_array +26% (`t[#t + 1] = v` keeps the limit below the size,
  so the touch is hot there). Testing `u >= lenhint` instead of the flag: geomean +0.3%, table_array +4.5%.

### Measured, not changed
- Bytecode is the same as 5.5's for the interpreter-bound programs (fannkuch, heapsort, queens, sieve,
  oop_methods; listings diffed), so the remaining gaps to ref54 are mostly the shared interpreter costs of
  "Summary of Phase 6/7" (relative jump table, dispatch register shuffles, interrupt poll) plus two 5.4
  semantics: FORLOOP also stores the control copy `ra + 3` (empty loop 35 instructions per iteration vs
  32 for 5.5, reference 29) and `alimit_touch` on array accesses (settable_int micro 204 vs 184 per 4 stores:
  the touch's slow block makes LLVM reload two spilled registers on the fast path).
- `scripts/build-fast.sh` works for 5.4 (`RUSTFLAGS="--cfg rustynail_lua54" CARGO_TARGET_DIR=target-54/fast
  scripts/build-fast.sh`): instructions geomean 0.90 of ref54 (default build 0.94), every program at or
  below 1.00 except load_big 1.004.

### Status after step 17 (bench/results/lua54-step1.json, 5 runs, pinned core, vs ref54)
- Default 5.4 build: time geomean 0.843 (the pre-step binary measured 0.849 in the same run), instructions
  0.939 (0.967). load_big 1.25 -> 0.99 time / 1.30 -> 1.03 instructions, oop_methods 1.14 -> 0.95 / 1.17 ->
  0.88, knucleotide 1.04 -> 0.96, heapsort 1.05 -> 0.99 (time only: identical instructions), json 0.83 ->
  0.77. Still above 1.00 in time: queens 1.09 (instructions 1.06), closures ~1.00. mandelbrot moved 0.72 ->
  0.88 at identical instruction counts (code placement, see the Phase 7 caveat).
- Fast build (`build-fast.sh` with the 5.4 cfg): time geomean 0.816, instructions 0.901.
- Remaining ideas (5.4): the shared interpreter items of "Summary of Phase 6/7"; a cheaper `alimit` scheme
  (e.g. a header copy of the limit so the fast path compares against it alone, as the reference does — the
  48-byte `Table` has no spare bytes); the lexer's `Result<Token, CompileError>` returned through memory
  (boxing it regressed cycles, see above).

## BUNDLE-LIB: library function bodies (string.format / number formatting / tolstring)

Micro suite: `bench/lib/*` plus a per-op harness (cycles/op = (cycles:u of the op loop - empty loop) / N, min of 3, CPU 3). The
study's finding (library bodies are the largest remaining pool vs LuaJIT) held for the formatting paths only: they went through
`snprintf`, `core::fmt` or per-call string interning. Measured on this branch (cycles/op, before -> after):

| op | before | after | LuaJIT | PUC 5.5 |
|---|---|---|---|---|
| `format("%08x  %-7s %s%s\n", ...)` | 1064 | 490 | 306 | 999 |
| `format("%-7s|", s)` | 652 | 180 | - | 324 |
| `format("%c", n)` | 438 | 111 | - | 198 |
| `format("%5.2f", x)` | 584 | 378 | 457 | 926 |
| `format("%.3f", x)` | 909 | 284 | 145 | 792 |
| `format("%g", x)` / `("%.14g")` | 831 / 920 | 383 / 446 | 467 / 464 | 861 / 1078 |
| `format("%e", x)` | 926 | 431 | 480 | 930 |
| `tostring(x)` float | 1210 | 541 | 461 | 2120 |
| `"f" .. x` float | 605 | 430 | 377 | 1198 |
| `format("%s x6")` | 685 | 435 | 425 | 1156 |
| `tostring(t)` (`table: 0x..`) | 329 | 180 | 99 | 364 |

What was done (one commit each):
* `%s` with width / precision and `%c` are native (no `snprintf`, no temporary `Vec`); text goes straight into the output buffer.
* `nfmt`: exact 128-bit scaled-integer digit generation (`scaled(x, k) = round_half_even(x * 10^k)` from the double's mantissa and
  exponent, `k` in 0..=19) for `%e %g tostring(float)` (up to 19 significant digits) and `%.Nf` (N <= 19); the shortest-round-trip flag
  of `tostring` is the Clinger fast-path read-back test. Everything else (huge values, tiny values, longer precisions) keeps the
  `core::fmt` path. Checked against glibc `snprintf` with `nfmt_fuzz` (60M cases, `NFMT_CASES`).
* `luaL_tolstring` / `print` / `format("%s")`: no metafield lookup for values without a metatable, `__tostring` is a fixed interned key
  (`GlobalState::tostring_key`, `__name` reuses `name_key`) instead of hashing and interning the key per call, numbers are written
  straight into the output buffer, `type: 0x..` text is built on the stack.
* `gmatch_aux`: integer upvalue stores without the barrier path (noise-level).
* JIT recorder (`lib_tail`): the dead slice above a native's results is `stale` for the `+verify` lockstep. A GC step inside
  `tostring` clears that slice; which call it lands on depends on the allocation pattern, so any extra startup allocation made
  `b4_programs`, `p_laprune_programs`, `s_loopany_guard_programs`, `twins_options`, `m_lockstep_identity` fail (latent, found
  because this branch added one fixed string).

Tried, no gain (reverted): a split capture array in `MatchState` (the 512-byte capture zeroing is a `memset` call that shows up in
perf, but removing it did not change cycles: `match_cap2` 338 -> 348, `match_kv` 668 -> 739 with the extra index branch);
`count_matches` (class decoded once per `max_expand` run: all pattern micros within +-3%); a 16-byte inline copy instead of
`extend_from_slice` in `table.concat` (`tconcat3` -7%, `tconcat10` +5%). `table.insert/remove/move` already use `arr_move`; their
remaining cost is call overhead (`TableRef::len`, `checktab`). Not changeable: `lua_hash` is a 4-cycle-per-byte dependency chain
and its value is observable through `pairs` order, so short-string creation (hash + chain walk, about 100 cycles for a 25-byte
string) stays the dominant cost of `sub`/`concat`/`format` results; `resize_strtab` (~10% of unique-string micros) is the reference's
grow/shrink policy.

Program A/B (base = main 78e0dd9e, new = this branch, `bench2.py`-style: CPUs 3-7, min of 5 rotated, duplicate column):
real_dis_arm 0.912, real_dis_mips 0.889, real_dis_ppc 0.912, real_dis_x86 0.984, serpent_ser 0.856, template_etlua 0.833,
string_ops 0.875, template_gsub 0.922, sudoku_solver 0.944; set geomeans (cycles): lib 0.966, corpus2 dev 0.994, heldout 0.982,
corpus 0.988, awfy 0.998, programs 1.001, jit-trace 1.001, apps 1.017, real 0.982. Layout effects at equal or different
instruction counts (not code cost): corpus/classic_vector 1.08 (+6% instructions; reproduced by merely allocating two more
permanent strings at startup, i.e. address-dependent hashing), lib/match 1.04-1.09 and lib/find_cap 1.04-1.06 (instructions
identical, the pattern code is untouched: three builds of this branch measure 0.993, 1.041 and 1.094), programs/pcall_error 1.05-1.09
(known), apps +1.7% geomean at equal instructions.

## P-TINS: table insert path (`finish_set`, `newkey`, `insertkey`, `rehash`, NEWREF helper), 2026-11-08

Study (feature-6 / thread.ci): `newkey` 1.57 Gcyc and `finish_set` 1.49 Gcyc over 18 programs, `TableRef::finish_set` = 35 % of all
store-to-load interlocks of `text_adventure`. Micros: `bench`-style loops in `~/.cache/tins/m.lua` (cycles per operation, min of 3, CPU 3,
`rn-cli --features jit` built with `scripts/build-fast.sh` flags; `grow8` and `mixed` are per N/4 and N/2 iterations of 8 and 5 stores).

| op (cycles/op) | before | after | LuaJIT | PUC 5.5 |
|---|---|---|---|---|
| `t[#t+1] = v` (64-element tables) | 54 | 50 | 26 | 79 |
| `table.insert(t, v)` | 60 | 56 | 26 | 124 |
| fresh `{}` + 4 named fields, escaping | 523 | 374 | 309 | 752 |
| dynamic short-string key insert (`dynins`) | 95 | 79 | 60 | 133 |
| integer key insert into the hash part (`intins`) | 175 | 158 | 92 | 156 |
| fresh `{}` + `t[1..5]` | 406 | 418 | 432 | 624 |
| fresh `{}` + `t[1..8]` (x4 per N) | 120 | 122 | 118 | 176 |
| mixed fields + array + `#t+1` (x2 per N) | 387 | 347 | 282 | 497 |
| interpreter only (no JIT): fresh `{}` + 4 fields | 489 | 462 | - | - |
| interpreter only: `t[1..5]` | 601 | 492 | - | - |

The interlock was **the 16-byte copy of the key**. The VM writes the key/value operands of the slow path as two 8-byte stores (tag word,
payload); `TableRef::finish_set` did `let mut actk = *key;` and `TableRef::set` passed `*key, *value` by value: LLVM emits one `movupd`
16-byte load of two just-stored 8-byte halves, which cannot be store-forwarded (`ls_bad_status2.stli_other`; ~12+ cycles). Evidence:
`perf annotate` of `finish_set` put 88-96 % of its samples on the instruction right after that `movupd`; with `Value::load` (tag byte +
payload qword) the same function drops to 5 % and `stli_other` counts fall 9.06 M -> 6.25 M on `text_adventure` and 33.5 M -> 24.7 M on
`classic_vector` (the rest of the interlocks are in the allocator/GC: `free_obj`, `free_block`, `gc_step`, and the `Counters` zeroing, now
avoided on the string-key path). The `insertkey` node move `*f = *mp` (24-byte struct copy of a node written by narrow stores) is now field-wise.

What was done (one commit each, numbers in the commit messages):
* **P-TINS1** `finish_set` / `set` / `insertkey` collision move: no 16-byte loads of freshly stored values (`Value::load`, field-wise node copy).
* **P-TINS2** `mainposition` inline for short strings and integers (the out-of-line `match` was 15-22 % of a fresh-table insert); other kinds go to `mainposition_slow`.
* **P-TINS3** NEWREF helper (`newref_core`): `TableRef::newref_shortstr` = presence test + `newkey` + one lookup for the node (was `short_str_node`, `set_str` -> `pset` + `finish_set`, `short_str_node`: four lookups).
* **P-TINS4** `rehash` for a non-integer key into a hash part without integer keys: only the live/deleted node count matters (`na` = 0, array size kept): no `Counters` (a zeroed 33-entry array), no `countint`/`numusearray`/`computesizes`. Exactly the sizes of the general path.
* **P-TINS5** `insertkey`: free-main-position case inline, collisions out of line (`insertkey_collide`).
* **P-TINS6** lean helper entries `jit_tr_tlen` / `jit_tr_tset` / `jit_tr_newref` next to `jit_tr_tnext` (the general `jit_tr_alloc` frame is 6 saved registers + 0x88 bytes of stack + a jump table).
No layout, node size or field offset changed (`jit_layout` untouched); table sizes (`collectgarbage("count")` of tables with 4 fields, 9 array
slots, mixed, sparse) are identical to the base binary. Hash order, `#` borders and `next` order are untouched: `tests/vm/table_insert_layout.lua`
compares them with the reference (string-key tables print sorted keys because the harness does not pin the string seed; 5.5 also starts its
hash-part border search at a seed-dependent probe, so random-insert tables print only "is `#t` a border").

Program A/B (base = 836bff43, new = this branch, `bench2.py`: CPUs 3-7, 5 workers, min of 5 rotated, duplicate column, cycles:u / instructions:u), geomean new/base:

| set | n | cycles | instructions | dup/base |
|---|---|---|---|---|
| awfy | 14 | 0.997 | 0.995 | 1.001 |
| corpus (old) | 18 | 0.985 | 0.994 | 1.000 |
| corpus2 dev (without real_dis) | 17 | 0.981 | 0.998 | 0.996 |
| corpus2 heldout (without real_dis) | 11 | 0.984 | 0.989 | 0.996 |
| real_dis (dev 2 + heldout 2) | 4 | 1.000 / 0.999 | 0.999 / 1.003 | 0.998 / 0.999 |
| programs | 25 | 1.024 (spectral_norm run on a disturbed core: dup column 1.46; serial re-run 0.995) | 0.994 | 1.034 |
| jit-trace | 29 | 0.999 | 0.998 | 1.000 |
| lib | 95 | 0.998 | 0.995 | 0.999 |
| apps | 5 | 0.988 | 0.988 | 1.003 |
| real | 5 | 0.971 | 0.994 | 1.004 |
| all 223 | | 0.997 | 0.995 | |

Biggest wins: lib/len_tbl 0.832, lib/tpack 0.923, corpus/parser_combinator 0.929, real/template_engine 0.936, dev/regex_nfa 0.937, corpus/classic_vector 0.940
(0.984 cycles in the first A/B before P-TINS4-6: instructions 0.924), dev/csv_report 0.942, dev/oop_mixins 0.946, programs/varargs 0.946.
Slower than 3 % after serial re-runs (7 runs, CPU 3, duplicate column): programs/pcall_error 1.15 (known), lib/error_tbl 1.075, lib/fmt_q 1.04, lib/pcall_err 1.01:
instructions identical to the base to 5 digits and no table insertion in their loops (pcall/error with a table value, `%q`): code-layout/alignment of the
binary, the same class as the documented pcall_error noise. programs/queens 1.094 and programs/string_ops 1.034 were noise (duplicate column 1.096 / 1.227; serial re-run 1.00 / 0.98).

Tried without gain / not done: (1) a fast append path for `t[#t+1]` that skips `rehash` (needs a scan of the array for holes to give the same
`computesizes` result, which is what `numusearray` already does: no saving on small arrays); (2) a presized hash part for `{}` (not layout-exact, see
docs/JIT_PARKED.md C22 follow-up); (3) `State::finish_set` anchoring push/pop (needed for GC safety around a possible emergency collection); (4) the
lookup-miss side of `dynins` (`TableRef::get` + `mm_index` on `if t[k]`) is 55 % of that micro and not part of the insert path. Remaining gap vs LuaJIT
is mostly structural: `grow` (rehash 0->1->2->4->8 with a fresh array allocation each time, 98 cycles per insert), allocator/GC (`free_obj`, `free_block`,
`gc_step` carry 45 % of the remaining interlocks on classic_vector), and the out-of-line helper call per `TLEN`/`TSET`/`NEWREF` from trace code (LuaJIT inlines them).

## C39: `innerretry` (retry rounds of heads blacklisted by `InnerLoop`) and the C21 re-test, 2026-11-08

Mechanism, tests and mutants: docs/JIT_TRACE_REF.md "C39". Process default `innerretry=6`. Same binary, knobs explicit (equal-length LIMITS strings, `RUSTYNAIL_PAD`), CPUs 3-7, one program per core, columns rotated, `RUSTYNAIL_SEED=0`, min of 5 runs (min of 3 for the second table), cycles:u / instructions:u. Columns: `base` = `innerretry=0`, `dup` = `innerretry=0` again (noise), `r6` = `innerretry=6`, `r6+ie` = `ientry=1,innerretry=6` (the C21 always-enter hack, scratch patch, not merged).

| set (programs) | dup | r6 | r6+ie (C21 hack) |
|---|---|---|---|
| awfy (14) | 0.999 / 1.000 | 0.998 / 1.001 | 0.999 / 1.007 |
| corpus (18) | 0.999 / 1.000 | 1.000 / 0.999 | 0.963 / 0.973 |
| corpus2 dev (17, without real_dis) | 0.998 / 1.000 | 0.995 / 0.999 | 0.995 / 0.998 |
| corpus2 heldout (11, without real_dis) | 1.003 / 1.000 | 0.994 / 0.990 | 0.990 / 0.982 |
| real_dis_* (4: arm, x86, mips, ppc) | 0.999 / 1.000 | 1.001 / 1.000 | 0.993 / 0.996 |
| real (5) | 1.003 / 1.000 | 1.002 / 1.001 | 1.005 / 0.979 |
| programs/heapsort (1) | 1.000 / 1.000 | 1.003 / 1.005 | 0.994 / 1.005 |
| programs (25), jit-trace (29), lib (95), apps (5), min of 3, `r6` only | | 0.998 / 1.000, 1.000 / 1.001, 0.998 / 1.000, 1.002 / 1.000 | |

`r6` over `base` (cycles / instructions) on the named programs: iter_pipeline 0.986 / 0.993, stream_closures 0.995 / 0.990, awfy/cd 0.992 / 1.000, lua_lexer 0.978 / 0.972, heapsort 1.003 / 1.005, classic_vector 0.996 / 1.000, class_middleclass 0.992 / 1.000, expr_compiler 1.004 / 0.998, inspect_lua 0.999 / 1.000, awfy/bounce 1.000 / 1.000, awfy/list 0.998 / 1.000; wins sudoku_solver 0.934 / 0.912, lua_lexer 0.972 instr. No program is more than 0.5 % worse in instructions; cycle outliers of the 3-run tables (`programs/table_hash`, `lib/upper`) have identical instruction counts (layout noise, dup column 0.77 resp. 0.995).

**C21 re-test (`ientry=1` always enter, over `r6`).** iter_pipeline is cured (317 Mcyc, instructions 0.994 of base; without the fix 2748 Mcyc, 8.7x), but the rule still loses elsewhere: containers_lru_heap 1.063 / 1.090 (cycles / instr), inspect_lua 1.034 / 1.063, regex_nfa 1.073 / 1.058, awfy/cd instructions +3.5 % (cycles -4 %), awfy/richards instructions +3.6 %, real_dis_x86 1.033 / 1.025. Wins: class_middleclass 0.628 / 0.687, classic_vector 0.785 / 0.784, expr_compiler 0.920 / 0.923, forth_vm 0.959 / 0.930, lua_lexer 0.944 / 0.959, stream_closures 0.943 / 0.979, real/lua_interp 1.050 / 0.924 (cycles worse at fewer instructions), real/wordfreq 0.977 / 0.972. The losers are not `InnerLoop` cases (regex_nfa: 8 -> 10 heads blacklisted for `LeaveLoop`/`IntMiss`, 2.64M -> 4.16M trace entries: a different trace set, as the C21 entry described). Decision: **ientry stays off, no knob is shipped** (the instruction said keep only without regressions). Patch kept at `~/.cache/inner/ientry-C39.patch`.

## C38 slice 1: `sidenest` A/B, 2026-11-08

See docs/JIT_PARKED.md C38 "Slice 1 built" for the per-set table (same binary, CPUs 3-7, min of 5, dup column). Gate failed (awfy queens 1.358 instructions, roguelike_turns 1.132, havlak 1.034, pcall_err 1.026): knob ships off.

## BUNDLE-A: inline `#t` (trace `TLEN`, tier-1 `LEN`) and tier-1 `GETI` / `SETI` with the constant key 0, 2026-11-08

Mechanism and tests: docs/JIT_TRACE_REF.md "BUNDLE-A". Base = `93775736` (C38 sidenest docs commit, `git archive` into its own target), new = this branch; both `cargo build --release -p rn-cli --features jit`, `RUSTYNAIL_SEED=0`, env strings equal length (`RUSTYNAIL_PAD`), CPUs 3-7, one program per core, columns rotated, min of 5 runs, cycles:u / instructions:u (`scripts/bench_session.py`, 223 programs in 9 sets); `dup` = the base binary again (noise column). Geomean of per-program ratios new/base, cycles / instructions (dup in the same format):

| set (n) | new/base | dup/base |
|---|---|---|
| awfy (14) | 1.001 / 0.998 | 1.002 / 1.000 |
| corpus old (18) | 0.989 / 0.996 | 0.998 / 1.000 |
| corpus2 dev (17, incl. real_dis_arm/x86) | 0.966 / 0.968 | 1.000 / 1.000 |
| corpus2 heldout (11, incl. real_dis_mips/ppc) | 0.970 / 0.980 | 1.004 / 1.000 |
| programs (25) | 1.004 / 0.996 | 1.008 / 1.000 |
| lib (95) | 0.991 / 0.994 | 1.000 / 1.000 |
| jit-trace (29) | 0.997 / 0.998 | 1.000 / 1.000 |
| apps (5) | 0.992 / 0.995 | 0.998 / 1.000 |
| real (5, bench/real) | 0.996 / 0.995 | 1.000 / 1.000 |
| real_dis_* (4, reported separately) | 0.992 / 0.996 | 0.997 / 1.000 |
| ALL (223) | 0.991 / 0.992 | 1.001 / 1.000 |

No program is more than 3 % worse in instructions (every ratio outside [0.97, 1.03] is a win). `programs/table_hash` cycles 1.132 at equal instructions (dup column 1.083: layout noise). All outputs match `.expected`.

Targets (Mcyc / M instructions, base -> new, ratios):

| program | base | new | cyc | ins |
|---|---|---|---|---|
| corpus2 dev/diff_levenshtein | 728 / 3648 | 463 / 2364 | 0.636 | 0.648 |
| corpus2 heldout/forth_vm | 1148 / 5651 | 970 / 5099 | 0.845 | 0.902 |
| corpus2 dev/regex_nfa | 1128 / 6059 | 1072 / 5876 | 0.950 | 0.970 |
| corpus2 dev/text_adventure | 2779 / 9045 | 2760 / 8902 | 0.993 | 0.984 |
| corpus astar_jumper | 2045 / 6597 | 1969 / 6543 | 0.963 | 0.992 |

Other wins (`#` on short stacks / constructor tables): lib/len_tbl 0.608 / 0.740, lib/tinsert_append 0.753 / 0.900, lib/tinsert_idx 0.782 / 0.894, programs/fasta 0.908 / 0.919, jit-trace/alloc_ctor 0.869 / 0.935, corpus2 heldout/bigint_rsa 0.947 / 0.956.

Findings, in order of measurement:
* **diff_levenshtein** had 48067 `Some(Seti)` exits for `cur[0] = i`: `templates_table::kind` refused `SETI` with `B == 0` and `GETI` with `C == 0` (the array fast path indexes slot `k - 1`), so the instruction was never compiled and every execution left tier 1. Both now compile to an unconditional jump to the out-of-line `jit_get` / `jit_set` call (the helpers already implement `Geti` / `Seti`: array hit, hash-part insert via `finish_set`, `__index` / `__newindex`, barrier). The nonzero-constant, GETFIELD / SETFIELD / GETTABLE / SETTABLE and upvalue miss paths already used the helper (`slow_label`), not an exit; nothing else needed changing. 728 -> 463 Mcyc matches the variable-key rewrite of the study (463).
* **The first version of the inline length (hint `h` answers when `arr[h-1]` is set and `arr[h]` is nil, or the hash part is dummy at `h == asize`) gave forth_vm only 0.96**: a stack alternates push and pop, so the hint is always one off (after `t[#t+1] = v`, `arr[h]` is set; after `t[#t] = nil`, `arr[h-1]` is nil) and the exact states of the spec went to the helper. The helper's own one-step moves (`TableRef::len`, 5.5 branch: `MAXVICINITY = 4` steps up or down from the hint, then a hint store) are replicated inline, hint store included; that is what makes the stack case fast.
* **A second regression and its fix:** fresh constructor tables carry the hint `asize / 2` (0 for one slot): `jit-trace/alloc_ctor` (`{i, i + 1}` then `#t`) went to the helper after the failed inline probe and was +5.9 % instructions. The helper's upward scan is replicated for the constructor shapes (`asize - h <= 4`, or `asize <= 5` for hint 0); alloc_ctor is now 0.935. A version that required `asize - h <= 4` for every push cost lib/tinsert_* +4 % instructions (big array, stale hint); the bound now applies only after the common first step (nil at `h + 1`, or the array's end, answered at once).
* Not taken: tier-1 `LEN` of a table with a metatable (any metatable goes to the helper), strings (helper), hash-part borders (`hash_search` is randomised by the string-hash seed: the inline path never answers for a populated hash part).

Tests: tr_core `tlen_inline_programs`, mutants `tlen_inline_{prev,next,dummy,mt,hint,zero_bound}_skip` (registry 455 -> 461), tests/vm `tables_12_len_inline_hot.lua` and `tables_13_const_zero_key_hot.lua`, jit-diff template `len` in `tests/jitdiff/gen.lua` (tr template set), see JIT_TRACE_REF.md.

## Regression check of the 2026-11-08 session report: lib/load_small, lib/co_wrap, lib/pcall_err (no code change)

Builds: c276c376, 0a30941e, c2951cd2, 78e0dd9e, 2ee839f0, 72720bbb, HEAD (425bc3eb), and HEAD with `T1INLINE_PROCESS = 0` ("HEADoff"), plain and PGO (`scripts/pgo-build.sh`, c276c376 and HEAD); `perf stat` instructions:u / cycles:u, `RUSTYNAIL_SEED=0`, CPUs 3-7, min of 3-7 runs, programs `bench/lib/{load_small,co_wrap,pcall_err}.lua` by absolute path.

* **lib/pcall_err, instructions (+1.6 % plain, +2.1 % PGO): not a code regression.** The bisect over the 37 commits lands on `72720bbb` / the t1inline default (1026.6 M -> 1043.5 M), but the generated tier-1 code is byte-identical with and without the feature for this program (`code 21938 B`, same trace events, `HEADoff` equals c276c376 to 4 k instructions on every script path tried). The program's instruction count depends strongly on the length of the script path, because `error("x")` prepends `chunkid:line:` (880 M at 3 extra path characters, 1026 M at 24: the message crosses the short-string limit and the truncated chunk id changes the work in `chunk_id` / `where_`), and HEAD vs c276c376 flips sign with the path length (-1.3 % ... +0.6 % plain over 8 path lengths, -1.6 % ... +0.8 % PGO over 4). c276c376 and HEADoff agree within 0.001 % on all 8, so the difference is an allocation-pattern effect of the feature's bookkeeping (the hint map), not instructions of the call path. Single-site test scripts (`pcall` with / without `error`) differ by <= 0.06 %.
* **lib/load_small (cycles +5..11 % plain, PGO flat): layout.** Instructions are equal (891.9 M vs 893.7 M, +0.2 %, the extra 1.8 M come in with `c2951cd2`: the one-time site-hint and builtin tables). Cycles per intermediate build (min of 7, Mcyc): c276c376 222.7, 0a30941e 226.2, c2951cd2 244.2, 78e0dd9e 244.8, 2ee839f0 231.6, 72720bbb 230.7, HEADoff 230.6, HEAD 232.1: the binary alone moves it 222..245 with no change in the load/parse code (`lexer`, `parser`, `load_chunk` are untouched since c276c376; the profile shows `Proto::from_data` and `concat_num` as separate symbols at HEAD, i.e. other inlining decisions). The PGO build of HEAD is faster than the PGO build of c276c376 (164.0 vs 169.8 Mcyc). No mitigation applied: nothing in the path is a clearly cold branch inside a hot function.
* **lib/co_wrap (PGO +15 %): does not reproduce.** Fresh PGO builds of c276c376 and HEAD (the training set includes `bench/lib/co_wrap.lua`, so coverage is not the cause): 201.4 vs 201.9 Mcyc, instructions 938.0 M vs 936.6 M; plain 250.2 vs 244.4 Mcyc. The session's +15 % came from that build's particular profile/layout; re-running `scripts/pgo-build.sh` is the cure if it shows up again.
* Side observation (not a regression): the error path (`get_info_aux`, `i32::to_string` in `where_`, `chunk_id`, `realloc`) is ~30 % of lib/pcall_err; building the position text without `format!`/`Vec` growth would be a real, separate optimisation.

## BUNDLE-B: tier-1 calls of Lua `__lt` / `__le` / `__eq`, 2026-11-08

Mechanism: docs/JIT_PARKED.md C21 "Update (BUNDLE-B)". Base = `0a2ec136`, new = this branch; both `cargo build --release -p rn-cli --features jit`, binaries copied to equal-length paths, `RUSTYNAIL_SEED=0`, env strings equalised (`RUSTYNAIL_PAD`), same script paths, CPUs 3-7, one program per core, columns rotated, min of 5 runs, cycles:u / instructions:u (`scripts/bench_session.py`, 223 programs in 9 sets), `dup` = the base binary again. Geomean new/base, cycles / instructions (dup in the same format; `centry` columns are in the next paragraph):

| set (n) | new/base | dup/base |
|---|---|---|
| apps (5) | 0.993 / 1.002 | 0.997 / 1.000 |
| awfy (14) | 1.000 / 1.001 | 1.002 / 1.000 |
| corpus old (18) | 0.994 / 0.993 | 0.999 / 1.000 |
| corpus2 dev (17, incl. real_dis_arm/x86) | 1.001 / 0.999 | 1.002 / 1.000 |
| corpus2 heldout (11, incl. real_dis_mips/ppc) | 1.002 / 1.001 | 0.997 / 1.000 |
| jit-trace (29) | 1.001 / 1.000 | 1.000 / 1.000 |
| lib (95) | 0.997 / 1.000 | 0.998 / 1.000 |
| programs (25) | 1.000 / 1.000 | 0.999 / 1.000 |
| real (5) | 0.990 / 1.000 | 1.000 / 1.000 |
| real_dis (4, separately) | 0.996 / 1.000 | 1.003 / 1.000 |

Per program (instructions): corpus/classic_vector **0.837** (cycles 0.828), corpus/astar_jumper 0.984, corpus2/dev/oop_mixins 0.983 (cycles 1.010: layout, dup 1.00x), micro (5M `lt(x, y)` + 5M inline `x < y` with a Lua `__lt`, traces off) cycles 1201 -> 856..907 M, instructions 6674 -> 5052 M.
Gate: **one program over 3 % worse in instructions: corpus/class_middleclass 1.038 (cycles 1.055, dup 1.008)**. Its `a < world[j]` runs `Entity.__lt` (`a.energy` through a middleclass `__index`
function chain) and `__eq`; the callee is entered from tier 1 and leaves tier 1 again at its first `__index` function, so the attempt adds a helper call + entry + exit per comparison where the
base took one exit (the mechanism of the N1(c) losers: callee entered, then back to the interpreter). Not mitigated. lib/pcall_err 1.031 instructions is the documented allocation / path-length
effect of the section above (it also read 1.031 before the table-`==` change, byte-identical tier-1 code for that program). awfy/deltablue was +6.6 % in a first revision that answered
raw-equal / `__eq`-less table `==` in the helper (trace set 35 -> 48 recordings); that part was removed and deltablue reads 1.002.
**`centry` (item 2, `RUSTYNAIL_JIT_TRACE_LIMITS=centry=1`, same run, default stays 0):** instructions identical to the new column on every program (sets: all 223); cycles within the noise (awfy 0.995, programs 1.005, real 0.988; table_hash 1.237 is the known
layout-noise program, dup 0.957). Measured on oop_mixins with `centry=2` (also without the entry back-off monitor): enters 3038 -> 3166, instructions 6824.9 M -> 6824.8 M, cycles 2020 -> 2231 M. No gain: the `table.sort`
comparators there are called too rarely for compiled entry to matter, so the knob is off (C40).
Tests of this change: tests/vm/meta_23_tier1_compare_mm.lua (identical to the reference Lua in JIT off / on / always and under GC stress 2; all 144 tests/vm scripts identical to `refrun` in `on`, `always`, `always` + GC stress 2, `on` with threshold 2), `tr_core::bb_cmpmm_programs` (all trace modes, exit storms, budgets, GC stress, stack-move), `tr_identity` (instruction accounting: the interpreter fetches the `JMP` after a metamethod comparison whose condition holds as an instruction of its own, so the tier-1 continuation counts it too; the first revision was 1..14 instructions short on `meta_21`), mutants `mmret_nc_skip` / `mmret_flag_false` / `mmcmp_savedpc_skip` (registry 462 -> 465), jit-diff `JITDIFF_TSET=cmp` (new comparison-metamethod templates, not part of the other sets) and `tr`, seeds 1..40, modes `off tr trdef`, `JITDIFF_ENTERS=1`: 0 diffs. A second full run's cycle columns were taken while a test suite ran on the same machine (ignore them; its instruction columns equal the table above).

## BUNDLE-C: `sbump` and `fnpromo` (per-site `unroll` / `fnroot`), 2026-11-08

Mechanism and measurements of the individual rules: docs/JIT_TRACE_REF.md "BUNDLE-C". Base = `17a24ba5` (HEAD of BUNDLE-B), new = this branch, both `cargo build --release -p rn-cli --features jit`, binaries at equal-length paths, equal-length env (`RUSTYNAIL_JIT_TRACE_LIMITS=RUSTYNAIL_X=1` in both columns, so the process defaults are in force), `RUSTYNAIL_SEED=0`, CPUs 3-7 one program per core, min of 5 rotated runs, cycles:u / instructions:u (`scripts/bench_session.py`, 223 programs in 9 sets), `dup` = the base binary again. All outputs equal `.expected` (0 mismatches in 223 x 3 columns).

| set (n) | new/base cycles | new/base instructions | dup/base cycles |
|---|---|---|---|
| apps (5) | 0.906 | 0.941 | 0.997 |
| awfy (14) | 1.012 | 0.994 | 0.999 |
| corpus old (18) | 0.993 | 0.994 | 1.000 |
| corpus2 dev (19, incl. real_dis_arm/x86 0.982 / ~1.00) | 0.988 | 0.980 | 1.000 |
| corpus2 heldout (13, incl. real_dis_mips/ppc) | 0.999 | 0.993 | 1.004 |
| jit-trace (29) | 1.002 | 1.000 | 1.002 |
| lib (95) | 1.002 | 1.000 | 1.000 |
| programs (25) | 0.995 | 1.000 | 1.003 |
| real (5) | 0.993 | 1.001 | 1.000 |

Named programs (instructions / cycles, new/base): regex_nfa **0.707 / 0.779**, astar_jumper **0.881 / 0.940**, expr_lang **0.786 / 0.652** (apps; not a target: `sbump` makes the stitch exits of the recursive-descent parser installable), awfy/json 0.972 / 0.972, stream_closures 0.929 / 0.967, lru_cache 0.949 / 0.943, havlak 0.970 / 1.035, forth_vm 1.008 / 1.032 (target, not improved), class_middleclass 1.000 / 0.992 (not worsened), classic_vector 1.023 / 1.015, richards 1.011 / 1.009, bounce 1.000 / 1.002, list 1.000 / 1.001, deltablue 1.001 / 1.028, cd 0.999 / 1.008, lua_parser 0.994 / 0.990.
Gate: **no program more than 3 % worse in instructions** (worst: classic_vector 1.023, richards 1.011, forth_vm 1.008). Cycle outliers with equal instructions are the layout-noise list (`lib/load_small` 1.070, `lib/match` 0.938, `lib/utf8_len` 0.913, dup columns 1.005 / 0.982 / 0.998 on the first two). **One real cycle regression with fewer instructions: `awfy/towers` 0.957 instr / 1.110 cyc** (reproducible with the same binary and `sbump=0/1`, 3 runs: 42.9 vs 49.7 Mcyc; its recursion gets deeper side traces whose frameful exits are dearer than the instructions saved); the gate rule is instructions, so it ships, but it is the price of `sbump`.
What the rules do NOT deliver: the `fnroot=1` wins on `class_middleclass` (0.66) and `classic_vector` (0.67) and `forth_vm` (-9 % cycles); astar gets 0.88 of the 0.79 `fnroot=1` reaches. See JIT_TRACE_REF.md for the `fnpromow` sweep (class_middleclass reaches 0.746 instructions only at `fnpromow` 0..1, where `apps/events` 1.13, `deltablue` and `parser_combinator` 1.05 lose).

## BUNDLE-GM: recorded `gmatch` iterator step (`gmatch=1`), 2026-11-08

Mechanism, tests and mutants: docs/JIT_TRACE_REF.md "BUNDLE-GM". Process default `gmatch=1`. Same binary (base = main d5b17875 + this change), knobs explicit (`RUSTYNAIL_JIT_TRACE_LIMITS=gmatch=0` / `gmatch=1`, equal length, `RUSTYNAIL_PAD`), same script paths, `RUSTYNAIL_SEED=0`, CPUs 3-7 one program per core (hyperthread siblings idle, nothing else running), columns rotated, min of 5, cycles:u / instructions:u (`scripts/bench_session.py`, 223 programs in 9 sets); `dup` = `gmatch=0` again (noise column). Geomean new/base, cycles / instructions (dup in the same format):

| set (programs) | dup | gmatch=1 |
|---|---|---|
| awfy (14) | 1.000 / 1.000 | 0.997 / 1.000 |
| corpus (18) | 0.998 / 1.000 | 0.999 / 0.999 |
| corpus2 dev (17, without real_dis) | 0.998 / 1.000 | 0.995 / 0.998 |
| corpus2 heldout (11, without real_dis) | 1.000 / 1.000 | 0.998 / 1.000 |
| real_dis_* (4: arm, x86, mips, ppc) | 0.992 / 1.000 | 0.991 / 0.975 |
| programs (25) | 1.004 / 1.000 | 0.999 / 0.999 |
| lib (95) | 1.000 / 1.000 | 0.999 / 0.999 |
| jit-trace (29) | 1.001 / 1.000 | 0.999 / 1.000 |
| apps (5) | 0.999 / 1.000 | 1.002 / 1.000 |
| real (5) | 0.998 / 1.000 | 0.996 / 1.000 |

Programs outside +-2 % instructions (instructions / cycles): lib/gmatch_words 0.917 / 0.875, corpus2/dev/wordfreq_wrap 0.954 / 0.966, real_dis_mips 0.955 / 0.978, real_dis_ppc 0.969 / 1.003, corpus/testrunner_luaunit 0.973 / 0.987, real_dis_arm 0.975 / 0.989; programs/string_ops 0.981 / 0.971. Worst: corpus2/dev/text_adventure 1.006 / 1.013 (instructions / cycles; 2-3 words per `gmatch` loop, the trace is entered 1.8 M times against 0.38 M, each entry ends with the loop-end exit), class_middleclass 1.000 / 1.001 (unchanged from BUNDLE-B), lib/match, lib/find_cap, lib/gsub_fn, json_rxi, mustache_lustache, lua_parser, real_dis_x86 all 1.000 instructions. No program is more than 0.6 % worse in instructions; all outputs matched their `.expected` files. Cycle outliers have dup columns of the same size (noise).

## Regression hunt of the 2026-11-09 benchmark: apps/expr_lang (C39 retry rounds), lib/sub_1char and programs/knucleotide (inlining), 2026-11-09

Benchmark finding (main 53133e8d vs 8965e73e): apps/expr_lang +30 % instructions (+56 % cycles), lib/sub_1char +7.9 %, programs/knucleotide +5.6 %.

**Method.** `git archive` builds of 8965e73e (a0), 3cb7324d (a1: after C39 + C38 sidenest), 21d1893d (a2: BUNDLE-A), 17a24ba5 (a3: BUNDLE-B), 5ef87f6b (a4: BUNDLE-C), 53133e8d (a5: BUNDLE-GM), `cargo build --release --features jit -p rn-cli`; `perf stat -e instructions:u`, min of 3, `RUSTYNAIL_SEED=0`, CPU pinned. **Two path effects make the measurement chaotic and must be held fixed: the length of the binary path (classic_vector: 5.74 G vs 6.52 G instructions for the same binary copied to a longer path) and the length of the script path (expr_lang: 640 M at a 55-character script path, 634 M at 110 characters for the same build, the 810 M of the report at the short one).** The A/B below uses binaries copied to equal-length directories (`bin/{b,n,o}/rn-cli`) and the same script paths. (The session reports compared binaries at different paths; some of its per-program swings of +-10 % in the old corpus are this effect, classic_vector in particular.)

Instructions (min of 3), expr_lang at `bench/apps/expr_lang.lua`:

| build | expr_lang | lib/sub_1char | programs/knucleotide |
|---|---|---|---|
| 8965e73e | 627.9 M / 640.3 M | 1291.3 M | 8517.4 M |
| 3cb7324d (C39 + C38) | 808.2 M | 1291.3 M | 8517.4 M |
| 21d1893d (BUNDLE-A) | 805.8 M | 1291.3 M | 8517.5 M |
| 17a24ba5 (BUNDLE-B) | 806.2 M | 1291.3 M | 8517.5 M |
| 5ef87f6b (BUNDLE-C) | 810.4 M | 1291.3 M | 8517.5 M |
| 53133e8d (BUNDLE-GM) | 810.5 M | 1393.0 M | 8997.5 M |
| 53133e8d, `innerretry=0` | 642.6 M | | |

**expr_lang = C39 `innerretry` (531c17d5).** `innerretry=0` restores 640 M, `=1` 643 M, `=2` and above 810 M. The loop at proto:189 pc 8 (a callee `while` loop) is blacklisted by two `InnerLoop` aborts; round 1 (4102 back-edges) aborts again, round 2 (8202 back-edges) records an 84 KB root (trace 124) whose exits are hot (an unrolled data-dependent loop); it is flushed `Ineffective` after 1215 entries / 1862 exits, then the Q3 re-record installs a 120 KB root (trace 180, 0 entries, 41814 exits, flushed by `Dep`), and 192 instead of 64 side traces are recorded (104 installed), 123 instead of 54 pipeline runs, 137 deep recordings instead of 3 (the deep-site rule of J2-2 fires on the neighbouring proto:171). All of it is compile cost in a 100 ms program (pipeline 50 Mtsc instead of 11). The other C39 cases: lua_lexer needs round 3 (5815 M -> 5655 M, a root of 27 KB that stays effective, entries 1.6 M); sudoku_solver gains 0.3 % from round 1.

**Fix (two per-site rules, no knob changed).** (1) The wait of every retry round gets `INNERRETRY_WAIT_SHIFT = 2` more doublings (x4: 16 k, 32 k, 64 k ... back-edges): a round costs a recording and, when it works, a compile, so it is paid back only by loops that run long; lua_lexer (1.8 M entries), sudoku_solver and iter_pipeline keep their wins, expr_lang never reaches round 2. (2) `TrProto::on_ineffective`: a head that has had retry rounds (`inner_n > 0`) whose trace is flushed `Ineffective` is blacklisted at once instead of being re-recorded (the first-flush re-record is for a trace the first compile got wrong; after a retry round the shape is the data-dependent loop itself). Each alone: (2) only 810 -> 707 M, (1) alone not measured; both 643.1 M (8965e73e 640.3 M). Test: `tr_policy::c39_policy` (waits `>= 64 << (MAX_ABORTS + SHIFT + n)`, the ineffective rule with and without a round).

**sub_1char and knucleotide = BUNDLE-GM (53133e8d), not C39 and not the gmatch recorder** (`gmatch=0` leaves sub_1char at 1393.0 M; neither program uses `gmatch`). The commit adds `jit_gmatch_step` to rn-std and the profile of sub_1char moves `Heap::jit_str_probe` from 1.8 % to 27 % of the samples: with the new code in the LTO unit the compiler stops inlining the probe into `jit_tr_alloc` (the one-byte-string early return becomes a call, plus the tuple return through memory). A deterministic inlining decision (identical with 4 env paddings), not a logic change. `#[inline]` on `Heap::jit_str_probe` fixes both (sub_1char 1287.1 M = -0.3 % vs 8965e73e, knucleotide 8507.5 M, expr_lang unchanged).

**A/B (final, 3 columns, 223 programs, CPUs 3-7 one per core, columns rotated, min of 5, equal-length binary paths, instructions / cycles geomean new / base (53133e8d)):** corpus 0.999 / 1.000, corpus2 dev 0.999 / 1.006, real_dis 0.999 / 0.997, corpus2 heldout 0.994 / 1.008, awfy 0.998 / 0.999, programs 0.998 / 1.001, jit-trace 0.999 / 0.999, lib 0.998 / 0.999, apps 0.941 / 0.896, real 1.000 / 1.005. Worst single program: programs/heapsort +1.4 % instructions (cycles 0.999). Moved: apps/expr_lang 0.738 / 0.569, lib/sub_1char 0.924 / 0.887, lib/sub 0.951, knucleotide 0.946, markov_text 0.960, tostring_int 0.959. new / 8965e73e: corpus 0.979 (class_middleclass +3.8 % as before, not worsened), corpus2 dev 0.943, heldout 0.973, awfy 0.993, programs 0.996 (heapsort +1.9 %), jit-trace 0.998, lib 0.992, apps 0.987 (expr_lang +1.0 %), real 0.996; no program other than class_middleclass is more than 2 % worse than 8965e73e.

## C22-C / C22-E: exact-layout table growth fast paths (small hash growth, small array growth), 2026-11-09

Study (Opus, `~/.cache/study-p1`): 14-40 % of the instructions of table-building programs are growth steps (storage 40 %, tinsert_append 37 %, bigint_rsa 33 %, tinsert_idx 31 %, regex_nfa 21 %, diff_levenshtein 18 %, sudoku 15 %, classic_vector 14 %). Presizing from allocation-site feedback was rejected (observable: `pairs`/`next` order, `#t` borders, `collectgarbage("count")` differ from the reference). Everything below is exact by construction: layout (array size, node vector, chain links, `lastfree`, traversal order), `#t`, GC byte accounting and the memory-limit check are those of the generic path, which stays in the tree and is what the differential tests compare against.

**C: `resize_hash` (table.rs).** `rehash` of a table whose array part keeps its size (the string-key/field case: `setmetatable({}, C)` then `self.x = ..; self.y = ..`, and the P-TINS4 branch of `rehash`) called the general `resize`: temporary table header, `setnodevector`, two `exchangehashpart`, array bookkeeping, then `newcheckedkey` (`keyinarray` + `insertkey`) per old node. `resize_hash(t, nhsize)` does exactly what the general path does for `newasize == oldasize` and nothing else: allocate the new node vector first (failure leaves `t` intact), reset the length hint of a non-empty array part (`asize / 2`, the general path does it too), reinsert the live nodes in node order with `insertkey` (the array test of `newcheckedkey` is vacuous: an integer key in `1..=asize` is never in the hash part, dead nodes are skipped), free the old node vector. Not used under `--cfg rustynail_lua54`.

**E1: spare capacity of 1- and 2-slot array parts.** `arrcap(asize)` = 4 for `asize` 1 and 2, `asize` otherwise (not under 5.4). The block of a 1- or 2-slot array part has room for 4 slots, so the growth steps 1 -> 2 -> 4 of `t[#t + 1] = x` construction (rehash only yields powers of two) are an in-place `recount` of the accounted bytes: no allocation, copy or free. The array pointer is the end of the value slots, value slot `k` is at `array - 8 (k + 1)`, hint at `array + 0`, tags at `array + 4 + k`: none of that depends on the capacity, so **no JIT-visible layout changes** (checked: `jit_layout.rs`, tier-1 inline table templates, `jit_tr_alloc` / `jit_tr_newref` / A1 inline NEWREF, `finish_set`: all index from the array pointer and bound-check against `asize`; nothing outside `table.rs` computes a block base or size; the P-TINS finish_set 8-byte-load fix is untouched). The capacity follows from `asize` alone, nothing is stored. A header flag bit was considered and rejected: the A1 inline NEWREF stores a whole precomputed `flags` byte, so a flag bit could be clobbered by a trace. 3-slot parts keep exact capacity (`table.pack(a, b, c)`); constructors `{a, b}` pay a 48-byte instead of 32-byte pool block (+2 instructions per table, +0.4 % on an alloc-only loop).
GC accounting: `Mem::alloc_counted(size, counted)` / `free_counted` / `recount(old, new)` (alloc.rs) account and limit-check exactly the logical `concretesize(asize)` (not through `ext`, unlike `alloc_split`), so `Table::byte_size`, `collectgarbage("count")`, GC debt and the memory limit are unchanged. One deliberate difference: an in-place 1 -> 2 -> 4 step checks the limit for the delta (like the reference's `luaM_realloc_`) instead of for old + new at once, so within 22 bytes of an exact memory limit an allocation can succeed that failed before.

**E2: `rehash_append`.** `rehash` for `t[k] = v` with `k == asize + 1` (the append), a full array part (no nil slot: `array_dense`, 8 tags at a time) and no integer key in the hash part: `computesizes` provably returns `P = 2^ceillog2(k)` with `na = k` (derivation in the doc comment: `k` is in the top class, `a = k` there, `P <= 3k`, nothing above), and the hash part gets `live` nodes (+ a quarter when a node with an empty value, the dummy node included, was seen). So the `Counters` (zeroed `nums[32]`), `numusearray`, `numusehash`, `computesizes` are skipped and `resize(P, nsize)` is called directly. Any other shape (hole in the array, integer key in the hash, `k != asize + 1`) takes the generic count.

**Tests.** `tests_table::growth_fast_paths_match_generic` (1500 seeds x 300 ops: two tables of one heap in lockstep, the second with the fast paths switched off by a test-only thread-local; after every op equal result, array size, node count, `lsizenode`, the full hash image incl. chain links and `lastfree`, traversal order, `#`, `byte_size` and the bytes the op added to the heap; random key shapes with collisions, appends at `asize + 1`, nil stores (holes), explicit resizes) and `append_growth_sizes_and_bytes` (sizes and accounted bytes of 1..70 appends and the shrink path); both also pass under Miri (Miri found a null `array.add(4)` in `array_dense` for `asize == 0`, fixed). A mutation of `rehash_append` is caught. `tests/vm/table_growth_layout.lua` (~280 construction shapes: appends, index fills, reverse fills, objects, deletions, hash+array mixes, holes, integer keys in the hash, constructors, random op sequences; pairs order, `#t`, exact `collectgarbage("count")` byte deltas) is byte-identical to `ref/55/build-seed0/refrun` in off / always / on2 / trace-always / trace-on / incremental / GC stress 2 (float keys: seed independent, runs under vm_corpus). `tests/vm_seed0/table_growth_layout_str.lua` is the string-key version; the sizes after deletions depend on the string-hash seed, so it is compared with `RUSTYNAIL_SEED=0` only (not run by vm_corpus). Also: `cargo test -p rn-core` (103, with `jit` 105), `rn-core --test table_diff`, `rn-jit` `table_diff` / `jit_table` / `jit_memlimit` / `jit_alloc` / `tr_asm`, `vm_corpus` (all 9, interpreter build), `tests/vm/*.lua` interpreter vs JIT always / trace always / on2 + trace-hot-2 (0 diffs), jit-diff seeds 1..40 modes off tr trdef with `JITDIFF_ENTERS=1` (0 diffs, 35/40 seeds ran traces), GC stress 1 / 2 + `RUSTYNAIL_GC_VERIFY=1` on a small growth program (interpreter, incremental, trace-always), and the lua54 cfg build of the rn-core table tests.

**Micro (instructions, 200 k tables kept alive in a ring, JIT on):** `t[1]..t[4]` stores into `{}`: 330.0 M -> 255.7 M (-22 %; ~375 fewer instructions per table over the 3 growth steps); `t[#t+1]` x 8 into `{}`: 301.4 M -> 250.8 M (-17 %); `setmetatable({}, C)` + 4 field stores: 436.7 M -> 400.5 M (-8 %; the rest is the A1/NEWREF helper path, `newcheckedkey` of the new key and chain collisions, which is exact work); `{o, o}` / `table.pack(o, 2, 3)` creation: +0.4 % / +0.1 %.

**A/B (3 columns base / new / dup(base), 223 programs, CPUs 3-7 one per core, columns rotated, min of 5, equal-length binary paths `tgb` / `tgn`, `RUSTYNAIL_SEED=0`; other studies were running, so instructions are the signal; the dup column's cycle geomean is 0.995-1.006).** Geomean new / base, instructions / cycles: awfy 0.980 / 0.989, corpus 0.987 / 0.983, corpus2/dev 0.984 / 0.987, corpus2/heldout 0.979 / 0.977, programs 0.995 / 1.006, lib 0.995 / 0.992, jit-trace 1.000 / 1.003, apps 0.986 / 0.965, real 0.997 / 1.018, all 223: 0.992 / 0.993. **No program is worse than +0.41 % in instructions** (jit-trace/alloc_ctor +0.41, lib/tpack +0.22), all expected outputs match. Targets (instructions / cycles): awfy/storage 0.826 / 0.821, lib/tinsert_idx 0.881 / 0.858, lib/tinsert_append 0.887 / 0.853, lib/tinsert_front 0.892 / 0.892, corpus2/heldout/bigint_rsa 0.889 / 0.884, corpus2/dev/regex_nfa 0.906 / 0.864, corpus2/heldout/sudoku_solver 0.934 / 0.942, corpus/classic_vector 0.965 / 0.972, corpus/parser_combinator 0.966 / 0.970, corpus2/dev/diff_levenshtein 0.998 / 1.012 (its growth is not on these paths). Other movers: lib/tremove_pop 0.945, corpus2/dev/bignum_arith 0.948, programs/fasta 0.955, programs/gc_churn 0.958, awfy/havlak 0.966, apps/events 0.966, programs/json 0.970. real_dis_arm / real_dis_mips 0.990.
First run (3 runs, before `alloc_array` / `free_array_block` trimmed the constructor overhead): lib/tpack +1.4 %, jit-trace/alloc_ctor +2.5 % instructions (the capacity logic in alloc and free); both are within +-1 % in the final run.

**Not done / open.** (1) The general `resize` (array and hash part both rebuilt: appending to a table that also has hash fields) still goes through the temporary header; `rehash_append` only saves the counting there. (2) gen.lua has no dedicated growth template (adding one to `TT` would renumber every existing seed); `tests/vm/table_growth_layout.lua` runs under every JIT mode instead. (3) 5.4 keeps the old paths (exact capacity, generic rehash).

## P3: `ncallee` and `chainre` (megamorphic dispatch), 2026-11-09

Mechanism: docs/JIT_TRACE_REF.md "P3". Base = `e4273567`, new = this branch, both `cargo build --release -p rn-cli --features jit`, binaries copied to equal-length directories (`ba/` and `nw/`), identical absolute script paths, `RUSTYNAIL_SEED=0`, env strings equalised (`RUSTYNAIL_PAD`), CPUs 3-7 one program per core, columns rotated, min of 5 runs, cycles:u / instructions:u (`scripts/bench_session.py`, 223 programs in 9 sets), `dup` = the base binary again. Process defaults (`ncallee=1`, `chainre=95`, `chainn=65536`) in force. All outputs equal `.expected` (0 mismatches in 223 x 3 columns).

Geomean new/base, cycles / instructions (dup in the same format): apps (5) 0.992 / 1.000 (dup 1.000 / 1.000); awfy (14) 1.009 / 1.001 (1.005 / 1.000); corpus (18) 1.001 / 1.001 (1.000 / 1.000); corpus2 dev (19) 1.000 / 0.999 (0.999 / 1.000); corpus2 heldout (13) 0.982 / 0.984 (0.999 / 1.000); jit-trace (29) 1.004 / 1.000 (1.003 / 1.000); lib (95) 1.002 / 1.000 (1.000 / 1.000); programs (25) 1.012 / 1.000 (1.014 / 1.000); real (5) 1.002 / 1.000 (1.001 / 1.000); all 223: 1.002 / 0.999. **No program above 1.03 in instructions** (largest: class_middleclass 1.0002). real_dis_x86 / arm / mips / ppc: instructions 1.0004 / 1.0000 / 1.0000 / 1.0002. Cycle outliers at equal instructions are noise (the dup column shows them too): awfy/havlak 1.077 (dup 1.063), programs/table_hash 1.317 (dup 1.401), lib/find_plain 1.051. **Only mover: `corpus2/heldout/forth_vm` 0.813 instructions, 0.818 cycles** (5142 M -> 4183 M instructions, 1068 -> 912 Mcyc on the single run of the chain study). The task expected >= 20 % instructions; the same-work ceiling of the study was -27 %. What is left: the chain is now in hotness order for this run (links 81..85 carry the four hot primitives: 6.5 M / 4.4 M / 2.3 M / 0.2 M exits at the links, 22 M pass-throughs against 41 M before), the rest is the pass-through cost of the first links and the new stub test (+1 `cmp/je`).

Micros (`study-p3/w/micro`: `fdisp.lua K mode`, `mdisp.lua K mode`; CPU 3, one run each, Mcyc base -> new): fdisp rand K=2 361 -> 363, **K=4 1233 -> 465 (2.7x)**, K=8 1049 -> 752, **K=16 1126 -> 1314 (+17 %: a chain of 15 side traces costs more than tier 1)**; fdisp round-robin K=2 330 -> 311, 4 543 -> 541, 8 701 -> 672, 16 1236 -> 1179; mdisp (`__index` dispatch; no callee guard in the entry segment, only the stub test applies) rand K=2/4/8/16 424 -> 423, 597 -> 604, 869 -> 896, 1388 -> 1451, rr 433 -> 433, 526 -> 536, 773 -> 793, 1278 -> 1334: the extra test of the periodic check costs 1-5 % on exits that are nothing but pass-throughs (instructions mdisp rand K=4: 3.559 G -> 3.597 G, +1.1 %).

Not shipped from the task list: the "child subtree only" flush (S itself is flushed too, see JIT_TRACE_REF), a chain re-order for root exits (a root's callee guard exit is not re-ordered), the K=16 case (tier 1 would be better than a 15-link chain; the base also loses mdisp K=16 by the same chain, a chain-depth give-up rule would be a separate study).

## P2 bundle 3: loop-top check diet and float constant pool (`budsub`, `pollr15`, `fpool`), 2026-11-09

Mechanism, tests and mutants: docs/JIT_TRACE_REF.md "P2 bundle 3". Base = `184d799a` (P3 merge), new = this branch, both `cargo build --release -p rn-cli --features jit`, binaries copied to equal-length directories (`ba/`, `nw/`), `RUSTYNAIL_SEED=0`, env strings equalised, identical absolute script paths, CPUs 3-7 one program per core, columns rotated, min of 5 runs, cycles:u / instructions:u (`scripts/bench_session.py`, 223 programs in 9 sets), `dup` = the base binary again (noise column; instructions 1.000 everywhere, cycles 0.999-1.007). All outputs equal `.expected` (0 mismatches in 223 x 3 columns). Process defaults `budsub=1,pollr15=1,fpool=1`. Geomean new / base, instructions / cycles: **all 223: 0.970 / 0.989**, awfy 0.978 / 0.992, corpus 0.998 / 1.001, corpus2 dev 0.995 / 1.002, corpus2 heldout 0.995 / 1.002, programs 0.971 / 0.984, lib 0.975 / 0.991, jit-trace 0.901 / 0.960, apps 0.998 / 1.015, real 0.991 / 1.001. **No program is worse than +0.1 % in instructions** (corpus/class_middleclass 0.992; real_dis_x86 / arm / mips / ppc 0.999 / 0.998 / 0.999 / 0.999 instructions, cycles 1.011 / 0.990 / 1.008 / 1.005). Targets (instructions / cycles): programs/mandelbrot 0.854 / 0.965, programs/spectral_norm 0.897 / 0.880, programs/matmul 0.952 / 0.999, programs/fannkuch 0.964 / 0.961, programs/nbody 0.990 / 0.996, jit-trace/dot 0.938 / 0.998, jit-trace/field_method 0.955 / 0.899. Largest jit-trace movers: float_poly 0.656, inline_2 0.679, float_loop 0.772, field_global 0.835. Per-step on one binary (limits explicit, instructions): jit-trace/dot 12.93 G -> 12.53 G (`budsub`) -> 12.13 G (`pollr15`) -> 12.13 G (`fpool`, no constants in the loop); programs/mandelbrot 2.094 G -> 2.020 G -> 1.957 G -> 1.789 G.

Reading: the ceiling measured with mutants (poll_skip + budget_no_check, 5 of 6 instructions: 0.911 / 0.977 on 68 programs) was approached on instructions (0.970 over all 223 including programs without hot trace loops); cycles move about a third of the instruction gain, as the study predicted (about 0.25 cycle per removed instruction). apps cycles +1.5 % with instructions 0.998 is within the noise of that set (dup column +0.6 %).


## P2 loop-top placement fix (`HEAD_OFFSET` 24 -> 48, `pollr15` process default off), 2026-11-10

Raw numbers: docs/bench-p2fix.txt. Trigger: the 2026-11-10 session benchmark found P2 bundle 3 +13 % cycles on `lib/tonumber_num` (instructions 0.861) and +9 % on `programs/fib` (instructions 0.966) against `53133e8d`. One binary, limits explicit per column, min of 15 (tonumber_num) / 5 (fib):

* **`lib/tonumber_num` is a layout lottery, not a knob cost.** Its hot loop is 12 instructions / 56 bytes (`cmp byte [r15+d],0; jne; sub r11,6; jl; ...; jmp`). Per knob (Mcyc): all on 259, `budsub=0` 245, `pollr15=0` 247, `budsub=0,pollr15=0` 230 = old 230: any pair of the knobs shortens the body and moves it into a slow zone of the loop head. The sweep of `asm::HEAD_OFFSET` (0..56, step 8, new code) gives 194 226 **258 259** 205 221 215 221; the old code gives 498 231 230 230 231 234 242 243 (offset 0 is the old slow zone, P-L). `perf stat`: 9 micro-ops per iteration in both, but `de_no_dispatch_per_slot.no_ops_from_frontend` 901 M (new) vs 248 M (old) slots: front-end (op cache) bound. The mechanism is the one of P-L / Q2 (loop head position relative to the 64-byte line); no model of the zone was found, so no adaptive rule.
* **`programs/fib` is the poll form, not placement.** The hot trace is the recursion loop (`trace-2`, loop at `cmp byte [r15+0x402a00],0`). With `pollr15=1` it is 1.49-1.61 G at every head offset, with `pollr15=0` 1.44-1.46 G at every offset (and 1.51 at 56). Variants of the poll (fib, Mcyc): `cmp byte [r15+d],0` 1570; `movzx esi,[r15+d]; cmp` 1556; `lea rax,[r15+d]; cmp byte [rax],0` 1611; the data block shifted inside its page by 64..2112 bytes (4K-aliasing test) 1560-1609 (no trend); `movabs rax,&poll; cmp byte [rax],0` (`pollr15=0`) 1448. Only an address that does not derive from `r15` is fast; `perf stat` shows `de_no_dispatch_per_slot.backend_stalls` 3.69 G vs 2.55 G slots, `perf record` puts the samples on the loads that wait for the call-frame stores (`cmp byte [rbx]`, `cmp byte [rax],0x46`): the loop is a store-forwarding latency chain and the r15-based poll load disturbs it (not diagnosed further: not the address-generation latency, not the instruction count, not 4K aliasing, not the poll word itself).
* **Fix 1 (placement): `HEAD_OFFSET` 24 -> 48.** Screening of offsets 8, 24, 32, 40, 48, 56 over all 223 programs (3 runs): 32 puts `jit-trace/type_entry` into a +20 % zone, 8 and 40 do the same for `sink_vec` (+24 %), 0 / 8 for `field_global` (+25 %), and with `pollr15=0` 32 / 40 hit `inline_leaf` (+30..70 %); 48 has none of them. `lib/tonumber_num` at 48: 205-215 (old 230).
* **Fix 2: `pollr15` PROCESS default off** (`cache::POLLR15_PROCESS = 0`; `budsub` and `fpool` stay on; `RUSTYNAIL_JIT_TRACE_LIMITS=pollr15=1` is the old default). At offset 48, all 223 programs (5 runs), geomean cycles / instructions against the all-knobs-off code: `pollr15=1` 0.985 / 0.971, `pollr15=0` 0.989 / 0.983; jit-trace 0.962 / 0.901 vs 0.973 / 0.930. The poll is the only one of the three that costs cycles in a way no tried form removes, and with it on fib stays +9 %.

Final (new = offset 48, `pollr15` off, five runs, 223 programs, same method as bundle 3; `old` = `budsub=0,pollr15=0,fpool=0` at offset 24 in the same binary family, `dup` = noise column), geomean cycles / instructions new / old: **all 223 0.995 / 0.983** (main, i.e. bundle 3 as shipped: 0.990 / 0.970), awfy 1.012 / 0.989, corpus 0.997 / 1.000, corpus2 dev 1.002 / 0.998, heldout 1.005 / 0.999, programs 0.994 / 0.986, lib 0.994 / 0.987, jit-trace 0.977 / 0.930, apps 0.999 / 1.000, real 0.996 / 0.995. Targets at 15 runs (Mcyc, main / new / old): `lib/tonumber_num` 255 / **208** / 230, `programs/fib` 1586 / **1495** / 1491; (5-run screen, main / new / old) `programs/spectral_norm` 203 / 209 / 232, `programs/mandelbrot` 541 / 540 / 558, `jit-trace/field_method` 681 / 707 / 759, `jit-trace/dot` 1247 / 1241 / 1249, `programs/fannkuch` 1142 / 1149 / 1175, `programs/matmul` 434 / 437 / 434, `programs/nbody` 526 / 527 / 528. What the fix costs against main (0.5 % cycles, 1.3 % instructions geomean): the `pollr15` instruction savings (jit-trace/dot, int_sum, ...). Programs still more than 3 % slower than old at 15 runs: `lib/pcall_err` 237 vs 222 (+6.8 %, known from the benchmark: queue item "bisect pcall_err"), `corpus2/dev/regex_nfa` 724 vs 688 (+5 %), `lib/tunpack` 272 vs 260 (+4.6 %), `lib/m_max` 274 vs 267 (+2.6 %); against main also `awfy/sieve` 137 vs 125 (old 134), i.e. the layout lottery moved. Outputs equal `.expected` everywhere (0 mismatches). `tr_core` 148 / `tr_policy` 44 / `tr_asm` 44 pass (`tr_core::mutants_are_caught_by_live_programs` was skipped: the 8 GB cap OOM-kills the whole binary, see CLAUDE.md; the six loop-diet mutants pass individually with `TR_MUT_ONLY`), jit-diff 1..40 (7 trace modes) 0 diffs.

## P2 bundle 1: `rbp` and `r14` for the trace register allocator (`xregs`), 2026-11-09

Mechanism, tests and mutants: docs/JIT_TRACE_REF.md "P2 bundle 1". Base = `8cf0f821` (P2 loop-diet merge). Method: one binary (`cargo build --release -p rn-cli --features jit`, copied to `~/.cache/p2r/lua-p2r`), the knob explicit per column (`RUSTYNAIL_JIT_TRACE_LIMITS=xregs=0|1|3`, equal-length strings, so every column has the same binary path length and the same script paths), `RUSTYNAIL_SEED=0`, CPUs 3-7 one program per core (hyperthread siblings idle), columns rotated, min of 5 runs, cycles:u / instructions:u (`scripts/bench_session.py`, 223 programs in 9 sets), `dup` = `xregs=0` again (noise column). All outputs equal `.expected` (0 mismatches in 223 x 4 columns). Geomean new / base (`xregs=3` vs `xregs=0`), instructions / cycles: **all 223: 0.985 / 0.982** (`xregs=1`, `rbp` only: 0.990 / 0.987; dup 1.000 / 0.999), awfy 0.999 / 0.999, corpus 1.000 / 1.002, corpus2 dev 0.998 / 1.000, corpus2 heldout 0.995 / 0.988, programs 0.991 / 0.987, lib 0.980 / 0.969, jit-trace 0.965 / 0.979, apps 1.001 / 0.993, real 0.999 / 1.001 (real_dis_x86 / arm / mips / ppc cycles 1.012 / 1.000 / 1.000 / 0.994), corpus/class_middleclass 0.998 / 0.994. Targets (instructions / cycles, `xregs=3`): jit-trace/field_method **0.691 / 0.759** (`xregs=1`: 0.834 / 0.886), jit-trace/int_rec2 0.891 / 0.802, awfy/bounce 0.988 / 0.986, programs/queens 0.996 / 0.994, programs/spectral_norm 0.975 / 1.078 (below), programs/nbody 0.998 / 0.997, programs/fannkuch 0.993 / 1.009. Other movers: lib/str_eq_dyn 0.759 / 0.673, lib/byte 0.786 / 0.771, lib/rawget_set 0.839 / 0.804, lib/getmt 0.855 / 0.796, programs/oop_methods 0.857 / 0.880, jit-trace/field_rw 0.749 / 0.995, jit-trace/multret 0.897 / 0.974.

**Gate: not clean, so the process default stays 0** (`XREGS_PROCESS`; `xregs=3` is one constant away). Programs more than 3 % worse in either metric: `programs/spectral_norm` 0.975 / 1.078 (deterministic, instructions down: a layout lottery of the loop head, `HEAD_OFFSET` 24 is a slow zone for the 10 bytes shorter loop; at offsets 8 / 16 / 32 / 40 / 48 the program is 0.92-0.95 in cycles), `jit-trace/rec_loop` 0.993 / 1.062 (exit-heavy informational microbenchmark, +4..6 % at every head offset, also with `rbp` alone, same branches / misses / IPC down: unexplained), `corpus/astar_jumper` 1.005 / 1.032 (not reproduced in two other runs, dup column moves +-3 % on it). With `xregs=1`: spectral_norm and rec_loop the same. Earlier variants and what they taught: (1) both registers always on: `lib/m_type` +21 % cycles because `CallH` saved / reloaded the callee-saved registers around every helper call (fixed: they survive the call); (2) `r14` always on: awfy/cd, queens, richards +2.7..3.1 % cycles (+1..1.6 % instructions: one `ReloadK` per exit of short traces), corpus2/dev/regex_nfa +5.8 %, geomean unchanged: `V10` is now kept only where it removes a loop-carried spill round trip (`carried_spills`), which keeps field_method and drops the others (awfy 0.999 / 0.999). Compile time: the allocator runs twice for traces that used `V10` (`ra` 0.96 -> 1.76 Mtsc over the 26 compiles of awfy/richards, `pipeline` 5.4 -> 7.2 Mtsc, about 1 ms).

Phase 1 (the throwaway experiment, same method, `xregs=3` before the call-clobber fix, `V10` always on): all 223 programs 0.986 / 0.986 against 0.985 / 0.982 above; the four target programs field_method 0.691 / 0.760, int_rec2 0.891 / 0.811, bounce 0.984 / 0.975, queens 1.005 / 1.008.


## P2 bundle 2: upvalue / closure specialisation (`uvsep`, `uvroll`), 2026-11-09

Mechanism, tests and mutants: docs/JIT_TRACE_REF.md "P2 bundle 2". Base = `9419321e` (P2 bundle 1 merge), new = this branch; **one binary** (`cargo build --release -p rn-cli --features jit`), the knob explicit per column through `RUSTYNAIL_JIT_TRACE_LIMITS` (`u0` = `uvsep=0`, `u1` = `uvsep=1` (the shipped process default), `u3` = `uvsep=3`, `dup` = `uvsep=0` again as the noise column), `RUSTYNAIL_SEED=0`, identical script paths and env lengths, CPUs 3-7 (siblings 11-15 idle), one program per core, columns rotated, min of 5 runs, cycles:u / instructions:u (`scripts/bench_session.py`, all 223 programs in 9 sets, `real_dis_*` split out of corpus2). All outputs equal `.expected` in 223 x 4 columns (0 mismatches; the first run found the `cleanup` bug below: `awfy/json` failed with `uvsep=3`, fixed before the numbers here). Geomean new / `u0`, **cycles / instructions**:

| set | n | `u1` (shipped) | `u3` | `dup` (noise) |
|---|---|---|---|---|
| ALL | 223 | 0.9970 / 0.9977 | 0.9951 / 0.9968 | 0.9992 / 1.0000 |
| awfy | 14 | 1.0013 / 1.0001 | 1.0007 / 0.9991 | 0.9990 / 1.0000 |
| corpus | 18 | 1.0005 / 1.0004 | 0.9964 / 0.9985 | 0.9993 / 1.0005 |
| corpus2 dev (without real_dis) | 17 | 0.9620 / 0.9699 | 0.9628 / 0.9694 | 0.9988 / 1.0000 |
| corpus2 heldout (without real_dis) | 11 | 0.9940 / 0.9993 | 0.9809 / 0.9908 | 0.9953 / 1.0000 |
| programs | 25 | 1.0012 / 1.0000 | 0.9987 / 0.9980 | 1.0016 / 1.0000 |
| lib | 95 | 1.0003 / 1.0000 | 0.9998 / 1.0000 | 0.9996 / 1.0000 |
| jit-trace | 29 | 0.9984 / 1.0000 | 0.9978 / 1.0000 | 0.9979 / 1.0000 |
| apps | 5 | 1.0036 / 1.0001 | 0.9971 / 0.9992 | 0.9969 / 1.0000 |
| real | 5 | 1.0004 / 1.0000 | 0.9959 / 1.0001 | 0.9997 / 1.0000 |
| real_dis (x86 arm mips ppc) | 4 | 0.9962 / 1.0000 | 0.9914 / 0.9995 | 0.9982 / 1.0000 |

Targets (cycles / instructions): **`corpus2/dev/iter_pipeline` 0.554 / 0.594 (`u1`), 0.552 / 0.594 (`u3`)** (317 -> 175 Mcyc; the five copies of the closure-value chain per iteration become one set before the `LOOP`), `corpus2/heldout/stream_closures` 0.992 / 0.997 (`u1`), 0.959 / 0.988 (`u3`) (most of its traces hold a table constructor: that is `uvroll`), `awfy/queens` 1.001 / 1.000 and 0.993 / 1.000 at 15 runs (no upvalue access in its traces: nothing to gain). Other `u3` winners: `corpus2/heldout/forth_vm` 0.926 / 0.937, `programs/varargs` 0.947 / 0.953, `corpus/astar_jumper` 0.950 / 0.988, `corpus2/heldout/vending_fsm` 0.967 / 0.974, `real/lua_interp` 0.956 / 1.000 (not reproduced as instructions: layout). Noise: the `dup` column spreads 0.95-1.03 in cycles per program (instructions 1.000 everywhere but 4 programs at +-1 %).

Stop rule: overall cycles 0.9970 (`u1`), 0.9951 (`u3`) is about the 0.5 % line, the target `iter_pipeline` is far past 3 %, so the bundle was kept. **Gate for the default** (no program more than 3 % worse in cycles, programs over 2 % re-measured at 15 runs): `u1`: worst cycle ratios in the 5-run table are 1.030 (`lib/it_pairs_hash`), 1.025 (`lib/tmove`, `programs/gc_churn`), all noise (`dup` has 1.023) and 0.998 / 1.000 / 1.007 at 15 runs; instructions never worse than +0.6 % (`corpus/serpent_ser`). `u3`: `corpus/lua_lexer` 1.029 (5 runs), **1.037 and 1.028 at 15 runs, 1.027 with `uvsep=2` alone and 1.000 with `uvsep=1` alone, instructions 0.995**, counters (perf stat) show equal branches and branch misses, so it is a cycle effect of the new code (layout or the shape of the forwarded loads) and not a policy effect: over the gate. Shipped: `UVSEP_PROCESS = 1` (bit 0 only); `uvroll` stays off, `RUSTYNAIL_JIT_TRACE_LIMITS=uvsep=3` turns it on (one-line switch in `tr/cache.rs`). If `lua_lexer` is ever fixed (it is 3x LuaJIT for other reasons, JIT_PARKED C38) `uvsep=3` is worth +0.2 % overall and 4-7 % on four programs. The failed first run of `uvsep=3` on `awfy/json` led to the `cleanup` third-operand fix (always on, no measurable effect on any set).

Correctness runs with the knob on: `jit-diff.sh` seeds 1..40, modes `off tr trdef trstorm tremu`, `JITDIFF_ENTERS=1`, `RUSTYNAIL_JIT_TRACE_LIMITS=lockstep=1,uvsep=3` (final binary; trdef takes the process default `uvsep=1`): **0 diffs** (default `tr` set: tr installed 983 / entered 869 k / exits 2.20 M, trstorm 1706 / 377 k / 934 k; `JITDIFF_TSET=oop`: 0 diffs, tr 1033 / 935 k / 1.83 M). `cargo test -p rn-jit --features trace,t15_oracle` (without the mutant runner): all pass except `t15_oracle::oracle_corpus`, `tr_identity_pre::fetch_plus_insns_identity`, `tr_oracle::{oracle_exits, oracle_options, oracle_trace_modes}` (`P8A-MISMATCH` of `core/t1i_inline` and `tests/vm/table_insert_layout`, the known base failures); `tr_core` (146 tests) passes by default and with `TR_CORE_UVSEP=3` (every job with both bits; the first run of that found the `f2_lib` / `RANDOM` problem of docs/JIT_PARKED.md C43, which `uvroll` now avoids). The 9 new mutants (registry 490 -> 499) are caught one by one with `TR_MUT_ONLY`.

## R1 rearm: a rearmed sampling tick after a hand-off to the interpreter (`rearm`, process default 4), 2026-11-10

Mechanism, soundness, tests and mutants: docs/JIT_TRACE_REF.md "R1". Raw numbers: docs/bench-r1.txt. Base = `93b98afb`. Trigger: `corpus/class_middleclass` spent 35-43 % of its instructions in the interpreter (frames handed over by exits from inlined method frames and metamethod exits stay interpreted until the next tick of 1000 back-edges; `RUSTYNAIL_JIT_THRESHOLD=,30` gave 4807 -> 3101 M instructions).

Method: **one binary** (`cargo build --release -p rn-cli --features jit`, copied to `~/.cache/r1/bin/rn-cli`), the knob explicit per column through `RUSTYNAIL_JIT_TRACE_LIMITS` (`rearm=0` is the base column, `r0b` = `rearm=0` again = noise column, `r4`, `r16`, `r64`), `RUSTYNAIL_SEED=0`, identical script paths and env lengths (`scripts/bench_session.py` pads), CPUs 3-7 one program per core (siblings 11-15 idle, checked before and after), columns rotated, min of 5 runs, cycles:u / instructions:u, all 223 programs in 9 sets with `real_dis_*` split out. Outputs equal the `rearm=0` column and `.expected` in all 223 x 5 columns (0 mismatches). Geomean new / `rearm=0`, **cycles / instructions**:

| set | n | `r4` (shipped) | `r16` | `r64` | `r0b` (noise) |
|---|---|---|---|---|---|
| ALL | 223 | 0.9962 / 0.9986 | 0.9976 / 0.9985 | 0.9973 / 0.9986 | 0.9997 / 1.0000 |
| awfy | 14 | 0.9911 / 0.9989 | 1.0047 / 0.9990 | 0.9964 / 0.9991 | 0.9989 / 1.0000 |
| corpus | 18 | 0.9805 / 0.9847 | 0.9860 / 0.9842 | 0.9896 / 0.9845 | 1.0056 / 0.9998 |
| corpus2 dev | 17 | 0.9951 / 0.9967 | 0.9997 / 0.9955 | 0.9969 / 0.9956 | 1.0031 / 1.0000 |
| corpus2 heldout | 11 | 0.9925 / 1.0036 | 0.9870 / 1.0037 | 0.9911 / 1.0041 | 0.9939 / 1.0000 |
| programs | 25 | 0.9967 / 0.9999 | 0.9908 / 0.9999 | 0.9955 / 0.9999 | 0.9984 / 1.0000 |
| lib | 95 | 0.9997 / 1.0000 | 0.9999 / 1.0000 | 0.9992 / 1.0000 | 0.9993 / 1.0000 |
| jit-trace | 29 | 0.9982 / 1.0000 | 0.9996 / 1.0000 | 0.9988 / 1.0000 | 0.9997 / 1.0000 |
| apps | 5 | 0.9975 / 1.0009 | 1.0092 / 1.0010 | 1.0041 / 1.0010 | 1.0088 / 1.0000 |
| real | 5 | 0.9938 / 0.9995 | 0.9919 / 0.9996 | 0.9978 / 0.9997 | 0.9907 / 1.0000 |
| real_dis | 4 | 1.0024 / 1.0013 | 1.0131 / 1.0015 | 1.0020 / 1.0016 | 0.9967 / 1.0000 |

Targets (cycles / instructions; `r4`, `r16`, `r64`; 15-run values for class_middleclass in the text): `corpus/class_middleclass` **0.754 / 0.786**, 0.743 / 0.785, 0.744 / 0.785 (15 runs: 0.748, 0.743, 0.742); `corpus/classic_vector` 0.885 / 0.944, 0.899 / 0.944, 0.922 / 0.944; `awfy/queens` 0.925, 0.925, 0.916 (cycles); `awfy/deltablue` 0.934 / 0.929; `corpus2/dev/ecs_world` 0.925 / 0.939 / 0.952; `corpus/expr_compiler` 0.990 / 0.994 / 0.992. C21 losers (cycles, `r4` / `r16`; noise column in brackets): `iter_pipeline` 1.017 / 1.001 (1.023), `regex_nfa` 1.007 / 1.001 (1.000), `containers_lru_heap` 1.010 / 1.015 (1.010), `inspect_lua` 1.000 / 1.012 (1.002), `real_dis_x86` 1.006 / 1.011 (0.989), `awfy/cd` 0.982 / 1.037 (1.009; 15 runs 1.001 / 0.999 / 1.001), `awfy/richards` 0.997 / 1.043 (1.052; 15 runs 0.994 / 0.993 / 0.994). `corpus2/heldout/stream_closures` has +9.5 % instructions with every `rearm >= 1` (cycles 0.994 / 1.002 / 1.007 at 15 runs).

Gate (no program more than 3 % worse in cycles): the 5-run table has outliers in both directions that the noise column shows to be noise (`r0b` alone reaches 1.115 on `particles_physics`, 1.052 on `richards` and `markdown`); the flagged programs were re-measured at 15 runs (havlak, particles_physics, event_promise, towers, richards, cd, stream_closures, csv_ini_yaml, alloc_ctor): all within 1.0 +- 0.01 except `corpus2/dev/event_promise` at `r16` / `r64` (+7.5 %, instructions +0.1 %, trace statistics identical to the base: the armed tick found no traced head, 512 cold hook calls). That program is **bimodal run to run**: pinned to one CPU, same binary, `rearm=0` takes 359 M or 390 M cycles in different processes (alloc / ASLR layout), `rearm=16` 388-391 M. So no program is measurably worse and the knob ships on (`REARM_PROCESS = 4`; `rearm=0` turns it off). The stop rule (gain < 0.5 % cycles overall **and** < 5 % on class_middleclass) did not apply: overall 0.4 %, class_middleclass 25 %.

What the first versions taught (all measured on the same binary, `rearm=4`/`16`, before the final rules; instructions): (1) restoring the saved countdown on every rearmed tick **froze the formation countdown** (every interpreter back-edge during a span is a rearmed tick) and entering heads of any age bounced; both ideas together lost loop traces in `corpus2/dev/iter_pipeline` (841 M -> 13.4 G instructions, a `reduce` recording that aborted `InnerLoop`, then its function head blacklisted `TooLong`), `programs/heapsort` (3.33 G -> 5.86 G: the `sift` trace never formed, entered 11 times instead of 13 M), `programs/queens` +40 % cycles, `apps/expr_lang` +60 % in the first 5-run pass (geomean ALL 1.0125, corpus2 dev 1.17). The fixes are in the mechanism: the rearmed back-edge counts for the formation countdown, arming only for the frame the stay was entered at, and the `REARM_MIN_ENTRIES` = 1024 gate (min 16: both cliffs; 128: iter_pipeline; 512 / 1024 / 4096: none). The cliffs are formation lotteries (the bases are insensitive to the tick period 900..1300): a rearmed entry into a young trace changes when its exits get hot. (2) The gate costs gain: `class_middleclass` 0.60 (cycles, `rearm=16`, no gate, no entry-frame rule) -> 0.74.

Not built (docs/JIT_PARKED.md C45): R2-R4 of the study; arming for protos whose heads are only tier 1 (a tier-1-only loop re-enters only at the tick); rearmed entry into young traces after a maturity test other than entry count.

Tests: `tr_core` 144 passed (whole binary with `TR_CORE_REARM=4` = every job with `rearm=4`, minus the mutant runner `mutants_are_caught_by_live_programs`; 77 s), `tr_policy` 45, `tr_asm` 43 (+2 ignored), `tr_determinism` 2, all pass; the five R1 mutants are caught one by one (`TR_MUT_ONLY`-style runs of their detecting tests with `RUSTYNAIL_JIT_MUT`; registry 499 -> 504, the frozen-count test updated); jit-diff seeds 1..40, modes `off tr tron trstorm tremu trdefon` with `RUSTYNAIL_JIT_TRACE_LIMITS=lockstep=1,rearm=4`, `JITDIFF_ENTERS=1`: 0 diffs, 85-87 % of the seeds traced. The known base failures (t15_core::random_float_programs, ... see docs/JIT_PARKED.md C41) were not run.

## HDRSUB: header-safe `LIB sub` (`hdrlib` 2), lean `jit_tr_lib_sub` (`libsub`), 2026-11-10

Mechanism, soundness, tests and mutants: docs/JIT_TRACE_REF.md "HDRSUB". Raw numbers: docs/bench-hdrsub.txt. Base = `62719481`. Trigger: loops with `string.sub` (`lib/sub`, `lib/sub_1char`, `programs/knucleotide`) were never peeled because allocating `LIB` kinds were not header-safe (old B8, a miscompile that turned out to be the `cleanup` third-operand bug fixed in 4ce16b7a).

Method: **one binary** (`cargo build --release -p rn-cli --features jit`), the knob explicit per column through `RUSTYNAIL_JIT_TRACE_LIMITS` (`puc` = `hdrlib=1,libsub=0` = base; `r0b` = the same again = noise column; `lean` = `hdrlib=1,libsub=1` (lean entry alone, nothing new peels); `hA` = `hdrlib=2,libsub=1` (shipped); `hB` = `hdrlib=4,libsub=1` (also `char`, `tostring`)), `RUSTYNAIL_SEED=0`, identical script paths and env lengths (`scripts/bench_session.py` pads), CPUs 3-7 one program per core (siblings 11-15 idle, checked before and after), columns rotated, min of 5 runs, cycles:u / instructions:u, all 223 programs in 10 sets (`real_dis_*` split out). Outputs equal the base column and `.expected` in all 223 x 5 columns (0 mismatches). Geomean new / base, **cycles / instructions**:

| set | n | `lean` | `hA` (shipped) | `hB` | `r0b` (noise) |
|---|---|---|---|---|---|
| ALL | 223 | 0.9992 / 0.9986 | **0.9974 / 0.9964** | 0.9972 / 0.9958 | 1.0013 / 1.0000 |
| awfy | 14 | 1.0004 / 0.9991 | 1.0012 / 0.9981 | 1.0031 / 0.9981 | 1.0032 / 1.0000 |
| corpus | 18 | 1.0015 / 0.9987 | 1.0024 / 0.9979 | 1.0024 / 0.9989 | 1.0035 / 0.9997 |
| corpus2 dev | 17 | 0.9971 / 0.9995 | 1.0003 / 0.9995 | 1.0006 / 0.9994 | 0.9997 / 1.0000 |
| corpus2 heldout | 11 | 1.0085 / 0.9948 | 1.0076 / 0.9895 | 1.0062 / 0.9895 | 1.0068 / 1.0000 |
| programs | 25 | 0.9978 / 0.9971 | 0.9924 / 0.9938 | 0.9981 / 0.9938 | 1.0062 / 1.0000 |
| lib | 95 | 0.9984 / 0.9984 | 0.9943 / 0.9951 | 0.9937 / 0.9936 | 0.9995 / 1.0000 |
| jit-trace | 29 | 0.9994 / 1.0000 | 0.9997 / 1.0000 | 0.9985 / 1.0000 | 0.9997 / 1.0000 |
| apps | 5 | 1.0058 / 1.0000 | 1.0070 / 1.0000 | 0.9967 / 1.0000 | 1.0028 / 1.0000 |
| real | 5 | 0.9860 / 1.0000 | 0.9962 / 1.0000 | 0.9868 / 1.0000 | 0.9934 / 1.0000 |
| real_dis | 4 | 1.0031 / 1.0000 | 0.9997 / 1.0000 | 0.9972 / 0.9983 | 1.0061 / 1.0000 |

Targets (cycles / instructions; `lean`, `hA`, `hB`): `lib/sub` 0.908 / 0.943, **0.824 / 0.837**, 0.825 / 0.837; `lib/sub_1char` 0.959 / 0.915, **0.785 / 0.751**, 0.785 / 0.751; `programs/knucleotide` 0.929 / 0.938, **0.858 / 0.864**, 0.857 / 0.864; `corpus2/heldout/markov_text` 0.950 / 0.953, 0.914 / 0.897 (`hA`); `corpus/json_rxi` 0.961 / 0.984 (`hA`, first pass); `programs/string_ops` 0.989 / 1.000 (`hA`, first pass). The lean entry alone is a third of the sub gain; the peeled loop is the rest. `lib/char` (the recorder does not produce a `LIB char` for the 3-argument call: 1.000), `lib/tostring_int` 0.938 / 0.906 and `tostring_flt` 0.978 / 0.958 only with `hB` (levels 3 and 4): too small to ship, the levels stay available (`hdrlib=4`).

Gate. Stop rule (gain < 0.3 % cycles overall **and** < 8 % on `lib/sub`) did not apply: 0.26 % overall (0.4 % against the noise column), 17.6 % on `lib/sub`. No-program-over-3 %-worse rule: in the 5-run table the flagged programs were `corpus2/heldout/prototype_objects` 1.102, `log_analyzer` 1.050, `lib/match` 1.039, `oop_mixins` 1.034, `int_sum` 1.032 (`programs/table_hash` is bimodal run to run: the noise column alone reaches 1.285); at 15 runs: 1.005, 1.011, 0.982, 0.998, 0.992, `table_hash` 1.070 (noise column 1.122, instructions equal); none of them runs `string.sub`. So `HDRLIB_PROCESS` = 2 and `LIBSUB_PROCESS` = 1 ship; `RUSTYNAIL_JIT_TRACE_LIMITS=hdrlib=1,libsub=0` is the previous behaviour.

**Inline short-string compare in `Heap::jit_str_probe` (`eq_short`): built, measured, NOT shipped.** Replacing the `bcmp` of the chain walk by overlapping 8-byte loads (length <= 40) saves 1.5 % of the instructions on all 223 programs (0.959 on `lib/sub`) and cycles on the three targets (15 runs: `sub_1char` 0.948, `knucleotide` 0.950, `lib/sub` 0.985), but on whole-suite cycles it is +0.1 % / +0.4 % (noise column -0.2 %) and re-measured losers do not come back to 1.0 with equal instruction counts, i.e. code-layout shifts of the binary: `lib/find_plain` 1.047, `jit-trace/alloc_ctor` 1.057 / 1.035, `lib/pcall_err` 1.075 (one variant), `havlak` 1.029. That breaks the 3 % rule and the earlier LTO-inlining experience (5-8 % on `sub_1char` / `knucleotide`) is the same kind of effect in the other direction. Not behind a knob (heap routine); code reverted, numbers in docs/bench-hdrsub.txt. A retry would need a layout-stable build (PGO / explicit function alignment).

Tests: `tr_core` 151 passed (plain; `mutants_are_caught_by_live_programs` skipped); with `TR_CORE_PROCESS=1` 138 passed and the same 13 failures as with the old process defaults (`dm_growth_exits_are_bounded`, `emuverify_entry_rolled_hot`, `fn_root_short_recordings_blacklist_the_head`, `j2_recgate_programs`, `m1_interp_shapes`, `mixed_string_loops_are_not_flushed_as_ineffective`, `m_probe_programs`, `po_poly_programs`, `s1_frame_side_programs`, `side_traces_install_and_run`, `sinkroot_programs`, `twins_emulator_modes`, `twins_options`: re-run with `HDRLIB_PROCESS` = 1 / `LIBSUB_PROCESS` = 0 and the same 13 fail, so they come from other process defaults, not from this change); `tr_policy` 45, `tr_asm` 44, `tr_loop` 27, all pass; `--cfg rustynail_lua54` build: `hs_sub_ring_matrix`, `hs_sub_peels_only_from_level_2`, `b1_hdr*`, `tr_loop`, `tr_policy` pass. jit-diff seeds 1..500, modes `off tr tron trstack trstorm tremu trverify trrand trnoopt trnosink trdef trdefon` (default limits plus `lockstep=1`), `JITDIFF_ENTERS=1`: 0 diffs (84 % of the seeds traced); modes `off tr tron trstorm tremu trverify` with `hdrlib=4,libsub=1`: 0 diffs. Mutants `hdrlib_sub_always`, `hdrlib_sub_phi_stale`, `libsub_lean_args_swapped` caught one by one (`TR_MUT_ONLY`); registry 504 -> 507. Known base failures not run.

## BUNDLE-A: `libnum` (`tonumber(string)`), `libnext` (direct `next(t [, k])`), `libfind` (plain `string.find`), 2026-11-10

Mechanism, soundness, tests and mutants: docs/JIT_TRACE_REF.md "BUNDLE-A". Raw numbers: docs/bench-bundle-a.txt. Base = `67ddddf2`. Trigger: the three library calls aborted loop recording (`Nyi(Call)` / `CFunc`), the loop was blacklisted and ran as a T15 region with a tier-1 `jit_call` per call (`lib/tonumber_int` 404 instr/iter, `lib/it_next_empty` 300, `lib/find_plain` 505; LuaJIT 180 / 34 / 118).

Method: **one binary** (`cargo build --release -p rn-cli --features jit`), knobs explicit per column through `RUSTYNAIL_JIT_TRACE_LIMITS` (`base` = `libnum=0,libnext=0,libfind=0`; `num`, `next`, `find` = that knob alone; `all` = all three), `RUSTYNAIL_SEED=0`, identical script paths and env lengths (`scripts/bench_session.py` pads), CPUs 3-7 one program per core (siblings 11-15 idle), columns rotated, min of 5 runs, cycles:u / instructions:u, all 223 programs, outputs equal `.expected` in all 5 columns (0 mismatches). Geomean new / base, **cycles / instructions**:

| set | n | `num` | `next` | `find` | `all` |
|---|---|---|---|---|---|
| ALL | 223 | 0.9932 / 0.9962 | 0.9951 / 0.9945 | 0.9966 / 0.9968 | **0.9870 / 0.9876** |
| awfy | 14 | 0.9999 / 1.0003 | 0.9989 / 1.0000 | 0.9983 / 1.0000 | 1.0008 / 1.0003 |
| corpus | 18 | 0.9952 / 0.9999 | 1.0010 / 0.9996 | 0.9952 / 0.9982 | 0.9961 / 0.9980 |
| corpus2 dev | 17 | 0.9975 / 1.0001 | 0.9957 / 1.0000 | 0.9929 / 0.9921 | 0.9924 / 0.9921 |
| corpus2 heldout | 11 | 1.0053 / 1.0000 | 1.0033 / 1.0000 | 0.9979 / 1.0000 | 1.0052 / 1.0000 |
| programs | 25 | 0.9986 / 1.0000 | 1.0069 / 1.0000 | 1.0017 / 1.0000 | 1.0060 / 1.0000 |
| lib | 95 | 0.9857 / 0.9911 | 0.9862 / 0.9871 | 0.9942 / 0.9944 | 0.9699 / 0.9728 |
| jit-trace | 29 | 1.0002 / 1.0000 | 1.0023 / 1.0000 | 1.0021 / 1.0000 | 0.9993 / 1.0000 |
| apps | 5 | 0.9929 / 1.0000 | 1.0023 / 1.0000 | 0.9913 / 1.0000 | 0.9981 / 1.0000 |
| real | 5 | 1.0002 / 1.0000 | 1.0049 / 1.0004 | 0.9978 / 1.0000 | 1.0025 / 1.0004 |
| real_dis | 4 | 0.9970 / 1.0000 | 0.9954 / 1.0000 | 1.0008 / 1.0000 | 0.9955 / 1.0000 |

Targets (cycles / instructions, own knob): `lib/tonumber_int` **0.477 / 0.558** (404 -> 225 instr/iter), `lib/tonumber_flt` 0.701 / 0.763, `lib/it_next_empty` **0.626 / 0.584** (300 -> 175), `lib/it_next` 0.549 / 0.501, `lib/find_plain` **0.595 / 0.584** (505 -> 295). Other `lib/find_*` (`find_cap`, `find_pat`: real patterns, not recorded) and `lib/tonumber_num` / `tonumber_bad` / `tonumber_base` (instructions 1.000: number identity path, nil results, the two-argument form) are unchanged. All three exceed the 10 % stop-rule threshold by a wide margin.

Gate (no program > 3 % worse in cycles): the 5-run table flags 24 (column, program) pairs, **every one with instruction ratio exactly 1.0000** (programs that never call the three functions: the knob is not consulted, the code executed is identical, so the difference is layout / timing noise of the same binary). Re-measured at 15 runs: only `programs/table_hash` (1.129 `num`, 1.212 `next`, 1.033 `find`; bimodal, the HDRSUB noise column alone reached 1.285 at 5 and 1.122 at 15 runs) and `lib/sel_count` (`find` 1.032) stay above 3 %, both with equal instruction counts; `all` is clean at 15 runs for all 16 re-measured programs. Judgment call: noise, so all three ship: `LIBNUM_PROCESS` = `LIBNEXT_PROCESS` = `LIBFIND_PROCESS` = 1 (`RUSTYNAIL_JIT_TRACE_LIMITS=libnum=0,libnext=0,libfind=0` restores the previous behaviour; each can be turned off alone).

Not done / follow-ups: lean entries for the `STRTO` / `FIND` helpers in the style of `jit_tr_lib_sub` / `jit_tr_tlen` (the targets still spend 175-295 instr/iter against LuaJIT's 34-180; most of it is the class-A helper frame `jit_tr_alloc` and the exit-area stores); `tonumber(s)` of non-numerals (nil result), `string.find` with patterns, and `find` / `next` / `tonumber` as tail calls stay on the old path.

## BUNDLE-B: Rust string runtime, `RUSTYNAIL_STR_FAST` (S1 `pat1`, S2 `fmtc`), 2026-11-10

Raw numbers: docs/bench-bundle-b.txt. Base = `a90cf5ac`. Study finding: string.format +245M cycles and pattern matching +168M vs LuaJIT on `real_dis_arm`; interning / GC are not the gap (untouched: `lua_hash`, seed, table sizes, `pairs` order, accounting).

**Knob.** `RUSTYNAIL_STR_FAST` = bit mask read once per process (`crates/rn-std/src/strfast.rs`, process default `DEFAULT` there; tests force it with `strfast::set_mask`): bit 1 `pat1` (S1), bit 2 `fmtc` (S2), bit 4 `patc` (S3, not built), bit 8 `copy` (S4, not built). **Default = 2 (`fmtc` only).**

**Always on (not observable, no knob): `MatchState.capture` is `[MaybeUninit<Capture>; 32]`** (only slots below `level` are read; the 512 byte clear per call is gone). `lib/match` 0.96 cycles. `classend` is `#[inline(always)]` and `m` / `fast_match` / `analyze` are `#[inline(never)]`: without that the compiler changed the inlining of the recursive matcher and `lib/match` got 20 % slower (instructions +12 %).

**S2 `fmtc`** (`format.rs`, `mod fmtc`): format strings seen twice at the same address and length are compiled to op lists (literal runs, `%d`, int / float conversions with pre-parsed `nfmt::Spec`, `%c`, `%s` plain and with width/precision; anything else, and every erroneous spec, runs the reference per-item code `item_slow`, so argument checks, error order and messages are unchanged). 32 slots, 2-way, thread-local (not per `State`: the cache holds only owned byte copies, so sharing between states on a thread is harmless), verified against the BYTES on every hit, formats over 512 bytes are not cached, not charged to GC debt (`collectgarbage("count")` identical, checked). `__tostring` / `__name` are still consulted per call (the op calls `tolstring_into`).
**S1 `pat1`** (`pattern.rs`, `Plan`): a pattern that is exactly one item (literal, `.`, `%class`, `[set]`) with an optional `? * + -`, optionally wrapped in one whole-match capture, is matched by a direct scan (class masks and a lazily built 256-bit set bitmap for runs over 32 bytes); every other pattern, and every malformed one, takes the old code. The analysis is packed in one `u64` (a 24-byte `Option<Plan>` written through the stack cost a store-forwarding stall of 13 % in `json_roundtrip`).

Measured (one binary `rn-cli --features jit`, mask per column, identical `d0/lua` / `d1/lua` path length and identical env string length, CPUs 3-7 one job per core, siblings idle, min of 5, all 223 programs; geomean cycles / instructions, **vs the all-off column `s0`**; `base` = the previous main binary):

| set | n | base | `pat1` | `fmtc` | both |
|---|---|---|---|---|---|
| ALL | 223 | 1.002 / 1.004 | 1.000 / 0.999 | **0.997 / 0.993** | 0.996 / 0.992 |
| lib | 95 | 1.002 / 1.002 | 0.998 / 0.997 | 0.996 / 0.989 | 0.992 / 0.986 |
| corpus2 dev | 19 | 1.005 / 1.008 | 1.005 / 1.003 | 0.999 / 0.994 | 0.996 / 0.995 |
| corpus2 heldout | 13 | 0.994 / 1.004 | 1.002 / 1.005 | 0.987 / 0.984 | 0.991 / 0.989 |
| real | 5 | 1.026 / 1.020 | 1.005 / 0.978 | 1.002 / 0.999 | 1.000 / 0.977 |
| awfy / corpus / programs / jit-trace / apps | | | all within 0.99-1.01 | | |

Targets (cycles / instructions vs `s0`): `fmtc`: `real_dis_arm` **0.945 / 0.942**, `real_dis_mips` 0.916 / 0.927, `real_dis_ppc` 0.955 / 0.933, `real_dis_x86` 0.998 / 0.949, `lib/fmt_x_pad` **0.799 / 0.661**, `fmt_mixed` 0.828 / 0.734, `fmt_f` 0.908 / 0.823, `fmt_g` 0.948 / 0.883 (`fmt_d` 1.027 at 15 runs, instructions 0.957; `fmt_q` 1.024, goes through the reference item code). `pat1`: `real/wordfreq` 0.914 / 0.865, `lib/gsub_fn` 0.878, `gsub_tbl` 0.849, `gmatch_words` 0.953; but `real/json_roundtrip` 1.050 / 1.027, `lib/find_cap` 1.042 / 1.044, `template_engine` 1.041, `markdown_conv` 1.040, `parser_combinator` 1.043 (patterns that are not one item pay the per-call analysis; ~35 instructions per `find` / `gmatch` step).

**Decision.** `fmtc` ships ON: real_dis -2 to -8 % cycles, format programs -10 to -20 %. Programs > 3 % worse in the 5-run table (`lib/sel_count`, `corpus/class_middleclass`, `dev/lua_parser`, `lib/str_idx_concat_key`, `dev/coro_generators`) re-measured at 15 runs: all within noise (`sel_count` 1.088 remains, with instruction ratio exactly 1.000 and no format call: the same code, an env-string / layout effect, as in earlier bundles); `fmt_d` 1.027, `fmt_q` 1.024 under the 3 % line. `pat1` stays OFF: stop rule (wordfreq gain 8.6 % < 10 %, json_roundtrip +5 %, 9 programs > 3 % worse). It is a knob away (`RUSTYNAIL_STR_FAST=3`) and fully tested; the clean fix is deciding the item kind once per pattern (S3 cache) instead of per call.

Tests: `crates/rn-std/tests/strfast_diff.rs` (60k random patterns x find / match / gmatch / gsub with every replacement kind, init values, malformed patterns, on vs off in process; 30k random format strings x args with arg-access order and errors, three rounds each so the first, compile and cached paths all run; byte-keyed cache vs address reuse; mutant-checked), `tests/vm/strings_30_pattern_fast_paths.lua` and `strings_31_format_spec_cache.lua` (differential vs the reference in `vm_corpus`, also under `RUSTYNAIL_GC_STRESS=1/2`, all four masks), Miri on the pattern differential (`cargo +nightly miri test -p rn-std --test strfast_diff`, 983 s, clean). `collectgarbage("count")` identical for masks 0 and 3.

## BUNDLE-C: `t1ccall` (lean tier-1 call of light C functions) and `tfmt` (`string.format` recorded as a `LIB` helper), 2026-11-11

Raw numbers: docs/bench-bundle-c.txt. Base = `79b98056`. One binary (`rn-cli --features jit`), each piece explicit per column through `RUSTYNAIL_JIT_TRACE_LIMITS=t1ccall=A,tfmt=B` (`base` 0,0; `t1c` 1,0; `tf` 0,1; `both` 1,1; `dup` = base again), identical binary path and environment length, same script paths, CPUs 3-7 one job per core (11-15 idle, checked), all 223 programs, min of 5; the programs more than 3 % worse and the targets re-measured at 15 runs. All outputs equal their `.expected` files (0 mismatches in 223 x 5 columns).

**Knobs (struct default 0 for the tests, process default `cache::T1CCALL_PROCESS` / `TFMT_PROCESS` = 0 for both: both pieces ship OFF).** `t1ccall`: the tier-1 `CALL` template sends a callee that is a light C function to the new helper `jit_helpers::jit_ccall` instead of `jit_call` (tag check `LCF` in the slow path; the helper is `pre_call_native_fast` with the native known, `State::native_fast_with`, and the result move specialised for 0 / 1 / 2 wanted results; the general `pre_call` for a growing stack, no spare `CallInfo`, hooks; same return codes and error path as `jit_call`). It applies with traces off too when the knob is named (the process default is 0, so `RUSTYNAIL_JIT_TRACE=off` stays J4 exactly). `tfmt`: `string.format` with a constant short-string format of `%d %x %X %s %c` items (flags `-` / `0`, width digits, `%%`; one to five arguments) is recorded as `LIB FMT` (docs/JIT_TRACE_REF.md "BUNDLE-C").

| set | n | `t1ccall` | `tfmt` | both | dup (noise) |
|---|---|---|---|---|---|
| ALL | 223 | 1.001 / 1.002 | 0.997 / 0.999 | 1.000 / 1.001 | 1.000 / 1.000 |
| lib | 95 | 1.000 / 1.002 | 0.996 / 0.999 | 1.000 / 1.001 | 1.000 / 1.000 |
| real_dis (4) | 4 | 1.006 / 1.003 | 1.005 / 0.992 | 1.010 / 0.994 | 1.000 / 1.000 |
| awfy, corpus, corpus2 dev / heldout, programs, jit-trace, apps, real | 14-29 each | 0.994-1.010 / 1.000-1.002 | 0.995-1.001 / 0.997-1.001 | 0.993-1.010 / 0.999-1.002 | 0.998-1.005 / 1.000 |

(geomean cycles / instructions vs `base`; the full per-set table is in docs/bench-bundle-c.txt.)

**`t1ccall`: no gain, stays OFF (stop rule).** The helper still costs about 60 instructions per call (`jit_call` was about 96; what remains is prologue / epilogue, the `CallInfo` fill, the call, the result move, `finish`): total instructions go UP 0.2 % (`real/json_roundtrip` 1.006, `lib/fmt_d` 1.006, `real_dis_*` 1.003) and cycles are flat on every target (`lib/find_plain` 0.999, `tonumber_int` 0.993, `it_next_empty` 0.999, `string_ops` 1.003, `real_dis_x86` 1.009). Programs more than 3 % worse at 15 runs with equal instruction counts (code-layout shift of the tier-1 template, the same family as C46): `lib/sel_loop` 1.144, `lib/m_abs` 1.130, `lib/m_sqrt` 1.093, `corpus/parser_combinator` 1.092, `corpus2/dev/regex_nfa` 1.084. The stop rule (< 1 % on the target sets) and the 3 % rule both say no. `lib/find_plain` (the C-call target of the study) no longer calls through `jit_call` since `libfind` records it; a lean helper cannot beat the `pre_call_native_fast` bookkeeping it must still do; a real saving needs the native's own prologue (argument checks) or recording the call.

**`tfmt`: large wins on format-bound loops, nothing on the real_dis targets, one program over 3 %: stays OFF (rule).** 5-run table above: ALL 0.997 / 0.999, `lib` 0.996 / 0.999. Targets (5 runs, cycles / instructions): `lib/fmt_d` **0.895 / 0.916**, `lib/fmt_x_pad` **0.913 / 0.951**, `fmt_s` 0.977, `fmt_f` / `fmt_g` / `fmt_mixed` / `fmt_q` 0.995-1.020 (not recorded: `%f` / `%g` / `%q` / precision stay stitched), `real/json_roundtrip` 0.991, `programs/string_ops` 1.002; real_dis (15 runs): `x86` **0.963 / 0.957**, `arm` 1.008 / 0.993, `ppc` 1.009 / 0.996, `mips` **1.042 / 1.022** (`dup` 0.998). The geomean over the four real_dis programs is 1.005 cycles / 0.992 instructions: no gain on that target set although the hot `string.format("%08x  %s  %-7s %s%s\n", ...)` of all four (400 k calls per run) is recorded (five arguments through the stash op). Why: the helper still builds the output with the reference engine (`format_into`, spec cache) and allocates the result; what is saved is the stitch (`jit_call` + trace exit / re-entry), roughly 100-110 instructions per call, a few percent of programs whose time goes to the string runtime; and **a formation interaction costs `real_dis_mips`**: the side trace of the main loop's exit 6 used to end at the stitched `format` call and install; with the call recorded the recording continues to the end of the loop body and aborts `LeaveLoop` (the C38 limitation, docs/JIT_PARKED.md C38), so that path stays in tier 1 and the program's other C calls (`string.char`, `gmatch`, `ccall`) run un-recorded (+1.8-2.2 % instructions). `programs/gc_churn`, `lib/sel_n`, `programs/table_hash` > 3 % in single measurements have equal instruction counts and `dup` columns of the same size (noise). Retest trigger: C38 landed (then `real_dis_mips` should recover and the flip is `TFMT_PROCESS = 1`), or a cheaper helper (inline fast paths for `%d` / `%s` without `format_into`).

Tests: `tr_core::{tc_fmt_matrix, tc_ccall_matrix, tc_t1_modes, tc_fmt_knob_changes_the_traces}` (programs `tc_fmt`, `tc_ccall`, `tc_fmt_hot`: lockstep, `+verify`, exit storms, `+randra`, budgets, `-loop`, `+emurun`, GC stress 1 / 2 in both collector modes, GC verifier, stack-move stress, tier-1-only modes; errors through `pcall` / `xpcall`, tracebacks inside and after C calls, `debug.getinfo`, yield across `pcall`, `coroutine.wrap`, hooks set during the run, `__tostring` / number metatable appearing late, stack growth in C calls), `tr_class_a_pb::lib_fmt_follows_the_library`, `tr_policy::bc_process_defaults`, jit-diff template set `cc` (`JITDIFF_TSET=cc`; seeds 1..200 with modes `off tr tron trstack trstorm trrand tremu trverify` and both knobs on, `off always stack on2 on10` with `t1ccall=1` (200 seeds) and `tfmt=1,t1ccall=1` (100 seeds), the default `tr` mix with both knobs on (200 seeds, plus `trdef`), hook `count` (60) and the knobs-off control (60): 0 diffs). Mutants (registry 516 -> 520): `tfmt_kind_skip`, `tfmt_fmt_guard_skip`, `tfmt_args_swap`, `tfmt_stash_stale`, each run individually with `TR_MUT_ONLY`: all caught. Suites: tr_core 155 passed (`--test-threads=3`, without the live-mutant runner), tr_policy, tr_asm, tr_loop, tr_class_a_pb / tr_record / tr_ir / tr_passes / tr_ra, rn-std, vm_corpus (9) pass; the `tr_mutants` table-order test passes (the `next_callee_guard_skip` / `libbundle_default_ignored` rows were out of registry order at the base; reordered).

## BUNDLE-S3: compiled pattern cache `patc` (+ `pat1` on top of it), 2026-11-11

Raw numbers: docs/bench-bundle-s3.txt. Base = `f41f004d`. Knob: bit 4 of `RUSTYNAIL_STR_FAST` (`crates/rn-std/src/strfast.rs`); **process default is now 7 = `fmtc` + `pat1` + `patc`**; `0` / `2` give the old matcher / the old default.

**What it is** (`crates/rn-std/src/pattern/patc.rs`). A pattern string seen twice at the same address and length is compiled to a flat program: literal runs, single-character items (`.`, a literal, `%class` / `[set]` as a 256-bit bitmap built once, so a byte test is one load + bit test and sets are never re-parsed per subject byte), `(` `)` `()`, `$`, `%bxy`, `%f[set]`, `%1`, and the end; plus the start filter (first byte / first bitmap, looking through up to 8 leading `(`) and the `pat1` one-item analysis, all decided once per pattern. Malformed patterns compile to an error item at the position where the reference reports them (lazy: matching must reach the item). The matcher `cm` has the same recursion structure as `match_inner` (a recursive call exactly where `m` is called: `?`, each backtracking position of `* + -`, captures), with these differences that cannot be observed: the depth is an argument (a `matchdepth` kept in memory made a store-forwarding chain through every level), the next item is tested for "can it start here" before the recursive call of a backtracking loop (never when the depth limit is reached, so `pattern too complex` is still raised there), a trailing `.*` / `.+` run needs no loop, straight-line programs (only literal runs, unquantified single items, a final `$`) are matched without recursion.
- **Key and cache.** Thread-local, 256 slots (2-way sets) that ARE the programs (a lookup is a computed address plus independent loads; a slot -> `Rc` -> key pointer chain cost ~15 cycles per call on short subjects). A hit compares the pattern BYTES (`same_bytes`, no `bcmp` call up to 16 bytes), so GC address reuse can never produce a wrong hit; the first sight only records `(address, length)`, the second compiles. Not charged to GC debt (`collectgarbage("count")` identical for masks 0 / 2 / 4 / 7 / 15, checked). Patterns over 48 bytes, over 32 items, over 8 distinct sets or over 48 literal bytes are remembered as "does not fit" and run on the general matcher (so a compiled program can never reach the `matchdepth` limit of 200; the general matcher reports `pattern too complex` for the long ones as before).
- **Slot lifetime.** A `&Prog` handed to a `MatchState` stays valid because slots are written only by `miss` (first/second sight) and only while no `PinGuard` of the thread is alive. States that keep using their program after foreign code could run (a `gsub` with a replacement function/table, `GMatch`) pin; everything else is done with its program before any Lua code can run (captures are copied out without touching the program). Pinned: a miss writes nothing (the call runs the general matcher).
- **`pat1` on top.** `Plan` now comes from the cached program (no per-call `analyze`), its set run uses the program bitmap. `pat1` alone stays a knob (bit 1) for the tests.

**Measured** (`scripts/bench_session.py`, one binary, mask per column, same binary / script paths / env length, CPUs 3-7 one job per core, siblings 11-15 idle, min of 5, all 223 programs; geomean cycles / instructions vs `s0` (mask 0); `def` = mask 2 = previous default, `dup` = `def` again = noise):

| set | n | def | patc (6) | patc + pat1 (7, new default) | dup |
|---|---|---|---|---|---|
| ALL | 223 | 0.997 / 0.994 | 0.980 / 0.975 | **0.979 / 0.974** | 0.996 / 0.994 |
| awfy | 14 | 1.002 / 1.000 | 1.001 / 1.000 | 1.004 / 1.000 | 1.001 / 1.000 |
| corpus | 18 | 0.996 / 0.997 | 0.957 / 0.956 | 0.955 / 0.954 | 0.998 / 0.997 |
| corpus2 dev | 19 | 0.993 / 0.992 | 0.981 / 0.976 | 0.983 / 0.974 | 0.998 / 0.992 |
| corpus2 heldout | 13 | 0.990 / 0.985 | 0.981 / 0.979 | 0.986 / 0.979 | 0.988 / 0.985 |
| programs | 25 | 0.997 / 0.998 | 0.995 / 0.990 | 0.990 / 0.990 | 0.995 / 0.998 |
| lib | 95 | 0.996 / 0.990 | 0.975 / 0.970 | 0.974 / 0.967 | 0.995 / 0.990 |
| jit-trace | 29 | 1.000 / 1.000 | 1.000 / 1.000 | 0.998 / 1.000 | 0.999 / 1.000 |
| apps | 5 | 1.002 / 0.999 | 0.937 / 0.899 | 0.941 / 0.899 | 1.000 / 0.999 |
| real | 5 | 0.998 / 1.000 | 0.939 / 0.935 | 0.937 / 0.923 | 0.998 / 1.000 |
| real_dis (4) | 4 | 0.950 / 0.941 | 0.915 / 0.908 | 0.920 / 0.909 | 0.953 / 0.941 |

Targets (cycles / instructions vs `s0`; `def` -> `patc` -> `patc + pat1`): `real_dis_arm` 0.948 -> 0.918 -> 0.917, `real_dis_x86` 0.965 -> 0.921 -> 0.926, `real_dis_mips` 0.939 -> 0.902 -> 0.906, `real_dis_ppc` 0.949 -> 0.920 -> 0.930; `real/wordfreq` 1.004 -> 0.905 -> **0.878** (instructions 0.803); `json_roundtrip` 0.995 -> 0.954 -> 0.969; `template_engine` 0.994 -> 0.851 -> 0.858; `markdown_conv` 0.999 -> 0.883 -> 0.889 (apps/markdown 0.73); `parser_combinator` 1.000 -> 0.996 -> 0.956; `lib/gsub_fn` 0.998 -> 0.885 -> 0.848; `gsub_tbl` 0.982 -> 0.861 -> 0.822; `gmatch_words` 1.002 -> 0.901 -> 0.827; `lib/match` 0.997 -> 0.204 -> 0.204 (a capture-first pattern scanned position by position: the start filter looks through the `(`); `find_pat` 0.998 -> 1.006 -> 1.007; `find_cap` 0.996 -> 1.024 -> 1.023; `gsub_str` 0.998 -> 1.014 -> 1.017.

**Decision.** Stop rule not triggered (real_dis geomean gain 3.1 % over the previous default, wordfreq 12.6 %). `pat1` on top of `patc` is better than `patc` alone almost everywhere (it was a loss alone: its per-call analysis is now cached), so the default is 7. Gate (no program > 3 % worse in cycles): in the 5-run table the programs above 1.03 for column 7 are `lib/fmt_d` (1.050) / `fmt_q` (1.034), which are the already-shipped `fmtc` (the `def` and `dup` columns show 1.045 / 1.050 and 1.033), `corpus2/heldout/prototype_objects` (1.057) and `awfy/havlak` (1.030), both with exactly equal instruction counts (re-measured at 15 runs: 1.000 and 1.006 for column 7, 1.014 and 1.001 for `dup`: layout noise); the pattern-related micro-benchmarks re-measured at 15 runs: `lib/find_cap` 1.029 (instructions 1.006), `lib/gsub_str` 1.017 (1.021), `lib/find_pat` 1.005 (0.988): all under 3 %. Lesson of this bundle: these three one-shot-`find` micro-benchmarks (a trivial match per call) swing from +1 % to +10 % between builds of the same source with the instruction count unchanged (C46 layout class); the cost that remains is the cache probe (~35 instructions) against about 30 instructions saved in the matcher.

**Things tried and dropped.** (1) A subject-length threshold for using the cache (64 / 128 / 256 bytes): fixes the micro-benchmarks but loses most of the gain (real_dis 0.959 vs 0.923, template_engine 1.002 vs 0.862). (2) Running the first attempt of `find` / `match` on the general matcher and looking the program up only when it fails: loses the `real_dis` gain (they match on the first attempt with sets and captures that benefit). (3) Moving `$` / `%b` / `%f` / back references out of the hot `cm` loop into an `#[inline(never)]` function: +4.6 % instructions on `find_cap` (inlining changed). (4) `Rc<Prog>` slots with a pointer chain: lookup ~15 cycles; leaked `&'static Prog`: no better; slot-resident programs with a pin counter: kept. (5) The `matchdepth` field in the compiled recursion: a memory RMW chain; replaced by an argument (find_cap -4 %).

Tests: `crates/rn-std/tests/strfast_diff.rs` (`patc_matches_the_general_matcher`: 40k random patterns, half with several items / nested captures, x find / match / gmatch / gsub with every replacement kind, `init` values, masks `patc` / `patc+pat1` / `pat1`, three rounds each so the first-sight, compile and cached paths all run; `patc_malformed_errors_are_lazy_and_positional`: every malformed shape at every position; `patc_depth_and_capture_limits`: the `matchdepth` / 32-capture boundaries, leading capture openers in front of the first consuming item; `patc_is_keyed_by_bytes`: other bytes at the address of a cached pattern), `tests/vm/strings_32_pattern_cache.lua` (differential vs the reference, byte-identical for masks 0 / 1 / 2 / 4 / 6 / 7 / 15 and under `RUSTYNAIL_GC_STRESS` 1 / 2: reused patterns, equal strings at other addresses with `collectgarbage` between rounds, address-reuse churn, iterators interleaved with other pattern calls, oversize patterns, depth / capture boundaries, malformed patterns at every position), `tests/vm/strings_30_pattern_fast_paths.lua` for the masks too. Mutants checked by hand against the Rust tests: start filter looking through non-openers (`star_filter`), key compare without bytes, literal run off by one, `%f` previous byte, straight-line `$`: all caught; `lead < 40` and dropping the depth guard of the backtracking loop survive because they cannot change a result (the end position is always tried and reproduces the error; a compiled program has at most 32 items so it never reaches depth 200). jit-diff seeds 1..100 with masks 7 in the `gm`, `cc` and default template sets (`off always tr tron trdef`): 0 diffs.


## BUNDLE-D: `ccalls` (inline C-stack check, SHIPPED), `armm` (tier-1 arithmetic metamethod continuation, built, OFF), `sbump2` (escalating side unroll, built, OFF) (2026-11-11)

Raw data: one binary, the knob set per column through `RUSTYNAIL_JIT_TRACE_LIMITS` (`base` = all three 0, `cc` = `ccalls=1`, `am` = `armm=1`, `sb` = `sbump2=2`, `all` = the three, `base2` = a second base column for noise), `scripts/bench_session.py` (CPUs 3-7, one job per core, hyperthread siblings 11-15 idle, min of 5, same script paths, env padded to equal length), all 223 programs, stdout checked against `.expected` (no mismatch). Geomean cycles / instructions versus `base`:

| set | n | cc | am | sb | all | base2 (noise) |
|---|---|---|---|---|---|---|
| ALL | 223 | 1.000 / 0.999 | 1.003 / 1.003 | 1.003 / 1.003 | 1.005 / 1.005 | 1.001 / 1.000 |
| awfy | 14 | 0.997 / 1.000 | 0.997 / 1.000 | 1.028 / 1.022 | 1.025 / 1.022 | 1.002 / 1.000 |
| corpus | 18 | 0.992 / 0.991 | 1.002 / 1.001 | 1.001 / 1.000 | 0.994 / 0.992 | 1.000 / 1.000 |
| corpus2 dev | 19 | 0.998 / 1.000 | 0.993 / 0.995 | 1.008 / 1.010 | 1.006 / 1.005 | 0.998 / 1.000 |
| corpus2 heldout | 13 | 1.001 / 1.000 | 1.001 / 1.000 | 1.002 / 1.005 | 1.002 / 1.005 | 1.003 / 1.000 |
| programs | 25 | 0.998 / 0.997 | 1.032 / 1.027 | 1.001 / 1.002 | 1.030 / 1.028 | 1.003 / 1.000 |
| jit-trace | 29 | 1.002 / 1.000 | 1.001 / 1.000 | 1.001 / 1.000 | 1.003 / 1.000 | 1.002 / 1.000 |
| lib | 95 | 1.001 / 1.000 | 1.001 / 1.000 | 0.999 / 1.000 | 1.001 / 1.000 | 1.001 / 1.000 |
| apps | 5 | 0.996 / 1.000 | 0.993 / 1.001 | 1.001 / 0.999 | 0.991 / 1.000 | 0.997 / 1.000 |
| real | 5 | 0.997 / 1.000 | 0.993 / 1.000 | 1.000 / 1.001 | 0.991 / 1.001 | 1.000 / 1.000 |
| real_dis (4) | 4 | 1.000 / 1.000 | 1.001 / 1.000 | 1.005 / 1.001 | 0.998 / 1.002 | 1.002 / 1.000 |

Targets (cycles / instructions): `class_middleclass` cc 0.890 / 0.877, am 1.013, sb 1.008; `classic_vector` cc 0.995 / 0.993, am 1.009, sb 1.004; `raytrace` 0.998 / 1.000 / 1.007; `metamethods` cc 0.933 / 0.932, **am 2.237 / 1.957**, sb 0.996; `richards` 0.992 / 0.989 / 0.996; `deltablue` 0.994 / 0.992 / 0.998; `queens` 1.004 / 1.005 / 0.997.

### `ccalls` (R2): shipped, `CCALLS_PROCESS = 1`

`LIB CCALLS` (the C-stack check of an inlined `pcall` and of every inlined metamethod frame: `ccalls + k + 8 < LUAI_MAXCCALLS`) is lowered inline (`asm.rs` `ccalls_fast`): `nCcalls & 0xFFFF` of the thread (`r12`) against the immediate `LUAI_MAXCCALLS - k - 8`, a register move on success; the helper call stays as the slow path (it answers 0 near the limit, the guard behind it exits and the interpreter raises "C stack overflow" itself, so the error and its depth are the interpreter's). Gains: `class_middleclass` -11 % cycles (the study's 8.7 % in `jit_tr_alloc` was this one arm called per trace entry), `metamethods` -6.7 %, `astar_jumper` -2.5 %. Not built: skipping the check in side traces whose parent already checked (the inline form costs ~4 instructions).

Re-measure of the flagged programs (cycles with equal instructions, 15 runs, base / cc / base2 in Mcyc): `lib/sel_count` 252 / 241 / 245, `lib/sel_n` 361 / 367 / 383, `lib/it_next` 144 / 143 / 143, `programs/table_hash` 607 / 609 / 610, `jit-trace/alloc_ctor` 212 / 211 / 211: layout noise (the 5-run flags 1.084, 1.041, 1.037, 1.036, 1.025 are inside the base-versus-base2 spread). Nothing above 3 %.

### `armm` (TIER1-ARITH-MM): built, sound, OFF (`ARMM_PROCESS = 0`)

Tier 1 pushes the Lua `__add/__sub/__mul/__div/__mod/__idiv/bitwise (not `__pow`)/__unm/__bnot` metamethod frame in place from the non-number arm of the arithmetic template (`Cold::ArithMm`, helper `jit_arith_mm`), and `jit_mm_ret` finishes the instruction (stores the result in the destination register, `savedpc` stays after the `MMBIN*`) when the callee returns, continuing at a landing label that re-checks the tags tier 1 assumes at that mid-block pc. Count accounting: the `MMBIN*` is counted when pushed, the pre-paid rest of the block is given back while the metamethod runs and paid again at the continuation, so the instruction count identity holds whoever pushed the frame (`tr_core::bd_armm_programs` with `t15_oracle`). Yields, errors, hooks and a nearly full C stack fall back to the interpreter before anything observable happens.

**Why it is off.** (1) `programs/metamethods` is 2.24x slower (instructions 1.96x): with the interpreter handling `acc + one`, the loop reaches the interpreter's hot-loop machinery and runs as a trace; with tier 1 continuing, the loop stays in tier 1 (metamethod call + return through helpers on every iteration) and the trace is never entered (`entered 0`, osr entries instead). (2) The `return Vec(...)` / `return setmetatable(...)` metamethods that real vector code writes are tail calls, and `TAILCALL` inside a `CIST_MMRET` frame still exits to the interpreter, so `classic_vector` (1.009) and `raytrace` (1.000) see no continuation at all. Stop rule: no target gains 1 %. What would be needed: a policy that does not take the metamethod arm when the loop has a trace or is about to get one (counter-driven), and tail calls in metamethod frames.

### `sbump2` (R3): built, OFF (`SBUMP2_PROCESS = 0`)

Per exit: an exit whose last side attempts (inside inlined frames) ended `SideShort`/`Recursion`/`Depth` gets `min(failed attempts, sbump2)` extra recursion unrolls on its next attempt, and the "second identical abort blacklists the exit" rule is suspended while the escalation has attempts left (`cache.rs` `sbump2_extra` / `sbump2_pending`, counters only). First measurement (instructions, single runs): `class_middleclass` 3856.2 M -> 3856.4 M (cycles -0.8 % at best, `sbump2=1`; `sbump=0..4` alone: 0.0 %), nothing else gains; full pass: `awfy/list` +22 %, `awfy/towers` +22 %, `diff_levenshtein` +17 %. The `class_middleclass` loss is not an unroll problem: 288 M of its 800 M cycles are the stays after the interpreter-side exits at `SELF` in `isSubclassOf` (line 112, 472 exits of 611 k cycles each).

Tests: `tr_core::{bd_ccalls_programs, bd_armm_programs, bd_sbump2_programs}` (programs `bd_mm_depth`, `bd_arith_mm`, `bd_vec_loop`; modes always / on2 / on, `+verify`, `+exitstorm`, budgets, GC stress 1 and 2 in both collectors, stack-move, `+emurun`, t1; `bd_ccalls_programs` also demands equal entries/exits for `ccalls=0` and `ccalls=1`), `tr_bd::{bd_armm_lowering, bd_sbump2_rule}`, `tr_policy::bd_process_defaults`, jit-diff template set `bd` (`JITDIFF_TSET=bd`; seeds 1..200, modes off/tr/trverify/tron/trstorm3/trdefon/tremu/trstack with all knobs on: 0 diffs; sets `tr` and `cmp` too). Mutants (registry 528): `ccalls_inline_off_by_one`, `armm_never`, `armm_count_skip`, `armm_land_skip`, `armm_pay_skip`, `armm_dst_off`, `sbump2_never`, `sbump2_unbounded`.

## R4: polymorphic loop-invariant receivers (2026-11-11): stopped at the ceiling experiment, docs only

Stop rule hit before any code: with the pair loop of `corpus/class_middleclass` hand-specialised per receiver class (the ideal result of a per-class loop copy; four textual loop copies, no hop) the SELF-guard hops disappear (EXITDUMP exits 12.56 M -> 5.43 M, instructions 3.383 G -> 3.090 G = 0.913) but cycles stay at 710-715 M (1.003; one binary, `RUSTYNAIL_SEED=0`, CPU 3, 3 runs, instruction counts reproducible to 1e-6). The hop is cheap (IPC 4.8); the remaining cycles are interpreter stays after exits from inlined method frames. `classic_vector`'s receivers vary per iteration (spring table), so a per-class loop does not apply. Nothing changed in the code; see docs/JIT_PARKED.md C51.


## BUNDLE-F: `stitch0` (SHIPPED, `STITCH0_PROCESS = 1`), `fibdiet` (built, OFF), `lands` (NOT BUILT) (2026-11-12)

Base = `b7e98ce6`, new = this branch; ONE binary (`cargo build --release -p rn-cli --features jit`), the pieces explicit per column through
`RUSTYNAIL_JIT_TRACE_LIMITS` (`base` = `stitch0=0,fibdiet=0`, `s0` = `stitch0=1`, `fd` = `fibdiet=1`, `all` = both), `RUSTYNAIL_SEED=0`, env strings equalised
(`RUSTYNAIL_PAD`), same script paths, CPUs 3-7 (siblings 11-15 idle), one program per core, columns rotated, min of 5 runs, cycles:u / instructions:u
(`scripts/bench_session.py`, 222 programs of the 9 sets; `real_dis_*` are the four `corpus2` programs). Geomean new/base, cycles / instructions:

| set (n) | `s0` stitch0 | `fd` fibdiet | `all` |
|---|---|---|---|
| ALL (223) | 1.000 / 1.000 | 1.001 / 1.000 | 0.999 / 1.000 |
| awfy (14) | 0.996 / 0.998 | 0.999 / 1.000 | 0.995 / 0.998 |
| corpus (18) | 1.000 / 1.000 | 1.000 / 1.000 | 0.998 / 1.000 |
| corpus2 dev (17) | 0.996 / 0.999 | 1.000 / 1.000 | 0.997 / 0.999 |
| corpus2 heldout (11) | 1.005 / 1.000 | 1.003 / 1.000 | 1.006 / 1.000 |
| programs (25) | 1.000 / 1.000 | 1.002 / 1.000 | 1.003 / 1.000 |
| lib (95) | 1.001 / 1.000 | 1.001 / 1.000 | 0.998 / 1.000 |
| jit-trace (29) | 0.999 / 1.000 | 1.000 / 1.000 | 0.999 / 1.000 |
| apps (5) / real (5) | 0.996 / 0.999, 1.002 / 0.999 | 1.000, 0.999 | 0.999, 1.001 |
| real_dis (4) | 0.993 / 0.996 | 1.000 / 1.000 | 0.990 / 0.996 |

Programs more than 3 % worse in one column (5 runs): `table_hash`, `string_ops`, `log_analyzer`, `sel_count`, `m_max`, `tmove_1k`, `it_next`, `it_pairs`; every one has
**identical instruction counts (1.000)** in all columns. Re-measured at 15 runs, all four columns of each flagged program: within +-1.3 % of each other and in no consistent
direction (`table_hash` 589.6 / 592.3 / 591.7 / 592.2 Mcyc; `string_ops` 1015.9 / 1013.4 / 1014.2 / 1008.4; `log_analyzer` 2171 / 2184 / 2183 / 2178; `sel_count`
254.1 / 251.6 / 247.4 / 250.5; `m_max` 278.2 / 281.5 / 277.5 / 282.2): layout noise, not a regression of any piece.

**`stitch0`** (F1; `Rec::stitch`): a side recording of a function root (`fn_root`) used to stitch after 0 bytecodes (the exit before the `CALL` has no link target, so the
recording is `BUDGET; EQ #0 #1` at the same `CALL` pc: an empty trace whose own final exit is hot at once, up to 16 deep). `fn_end` already refused the identical case
(`path.len() <= 1` -> `LeaveFrame`); `stitch` now returns `Err(why)` for `side.is_some() && path.len() <= 1`. Micro (`~/.cache/stitchstudy/micro/st.lua`, a loop with one
`math.ult` call): 719 -> 405 instructions per iteration, side traces 16 -> 0; `awfy/json` empty traces 23 -> 0 (instructions -1.1 %). On the suite the rule is
neutral to slightly positive (real_dis 0.993 / 0.996, awfy 0.996 / 0.998) and nothing is worse: ON.

**`fibdiet`** (F2): only the piece that is sound by construction was built, `fibdiet` bit 0 (`asm.rs` `room_stack_check_from`): TR6a of the frame just pushed by a native self-call
is computed from V2 (the register that holds the new function index) instead of reloading `ci.func` that the push stored three instructions before (the store-forwarding
chain that was the hottest sample of `fib`). Same limit as the interpreter (`stack_size > func + 1 + extent`), checked by the stack-overflow programs (depth reached and
message equal). Measured on `fib`: 1445.1 -> 1440.8 Mcyc (-0.3 %), instructions -0.4 %; `binary_trees` -0.1 %, `permute`/`towers` +0.3/+0.4 %, `queens`/`richards`/`rec_loop` within
+-0.3 %. That is under the noise of C44, so by the stop rule the knob stays OFF (`FIBDIET_PROCESS = 0`). Not built: the fused CIST_FRESH + caller-C test (two loads of two
different `CallInfo` words: the fusion saves at most one instruction of ~13), the `n < 2` re-check in B (needs the parent trace's guard as a fact of the child trace: a
cross-trace type fact the recorder does not have), the hoist of the callee upvalue chain out of the D loop (`UREF/UVREL/UGE/ULOAD` is per frame: each frame has its own
closure instance, the guard `ULOAD == running closure` is what proves the callee; hoisting it would need the closure identity of the previous iteration, i.e. the
`uvsep` machinery for a recursion that has no loop). The measured ceilings of the unsound mutants (all room/CIST/TR6 checks off = 0.860) are not reachable soundly with
these pieces.

**`lands`** (F3): not built, see docs/JIT_PARKED.md C52.

Tests: `tr_core::{bf_stitch0_programs, bf_fibdiet_programs}` (programs `bf_stitch_chain`, `bf_rec`, `bf_rec_deep`: lockstep, `+verify`, exit storms, budgets, `-loop`, GC stress 1 / 2 in both collectors, stack-move (not for `bf_rec_deep`, quadratic), `+emurun`, t1; the deep programs print the depth reached and the overflow message, which equal the interpreter's), `tr_policy::bf_process_defaults`, jit-diff template set `bf` (`JITDIFF_TSET=bf`; seeds 1..200, modes `off tr tron trverify trstorm3 trdefon tremu trstack` with `stitch0=1,fibdiet=1`: 0 diffs; seeds 1..100 `off tr tron trverify trdefon` with both 0: 0 diffs). Suites: tr_policy 49, tr_asm 43, tr_loop, tr_record, tr_bd, rn-std pass; tr_core 163 pass (`--test-threads=3`; `twins_stackmove` and `tr_oracle` skip `bf_rec_deep`, which recurses to the stack limit). Mutants (registry 531): `stitch_side_empty`, `fd_room_extent_off` (caught individually with `TR_MUT_ONLY`). `tr_mutants::table_covers_the_registry_exactly_once` was failing at the base (rows `ccalls_guard_off` / `armm_*` out of registry order); TABLE re-sorted.

## BUNDLE-G: `chaindepth` (callee-guard chain cap) and `pcallerr` (allocation-free error position prefix), 2026-11-12

Mechanism, tests and mutants: docs/JIT_TRACE_REF.md "BUNDLE-G". Method: G1 one binary, the knob explicit per column (`RUSTYNAIL_JIT_TRACE_LIMITS=chaindepth=00|04`, `dup` = `=00` again, equal-length strings); G2 two binaries
in equal-length directories (`ba/` = main `9dee3775`, `nw/` = this branch), `dup` = `ba/` again; both: all 223 programs (`scripts/bench_session.py`), identical absolute script paths, `RUSTYNAIL_SEED=0`, env strings equalised,
CPUs 3-7 one program per core (siblings 11-15 idle, checked by `scripts/cpu_idle.py`), columns rotated, min of 5 runs (flagged programs re-measured at 15), cycles:u / instructions:u. All outputs equal `.expected` (0 mismatches in 223 x 3 columns, twice).

**G1 `chaindepth`.** Micro-benchmarks (`fdisp.lua K mode`: K closures called through one site; `mdisp.lua K mode`: `obj:step()` over K classes; `rand` / `rr` = random / round-robin order; CPU 3, min of 3, Mcyc, `chaindepth=0` -> `4`):

| | K=4 | K=8 | K=12 | K=16 | K=32 |
|---|---|---|---|---|---|
| fdisp rand | 482 -> 482 | 776 -> 777 | 1049 -> 1054 | 1320 -> **1252** | 2284 -> **1526** |
| fdisp rr | 535 -> 533 | 675 -> 677 | 936 -> 937 | 1196 -> **997** | 2132 -> **1281** |
| mdisp rand | 595 -> 592 | 866 -> 868 | 1168 -> 1162 | 1425 -> 1422 | 2486 -> **1880** |
| mdisp rr | 522 -> 523 | 771 -> 770 | 1035 -> 1042 | 1300 -> **1231** | 2442 -> **1712** |

(Plain caps with no coverage test, `chaindepth=4`: K=8 mdisp rand 875 -> 994 (+14 %), K=12 mdisp rand 1168 -> 1272 (+9 %), K=8 fdisp rand 776 -> 809; caps 6 / 8 / 12 / 16 help only K >= 32 and cost +3..6 % at K=16 (mdisp rand 1423 -> 1514 at 6). Tier 1 alone (`RUSTYNAIL_JIT_TRACE=off`) is 984 / 1016 / 1052 for fdisp rand K=8 / 16 / 32 but 2505 / 2535 / 2603 for mdisp rand: a chain beats tier 1 for method dispatch at every K measured, so the answer to a deep chain is a cap, not a give-up of the loop.)
The K=16 fdisp rand case of the P3 notes (+17 % against tier 1, 1126 -> 1314 Mcyc pre-P3 -> P3) is 1252 now; the chain of 15 links is 3-4 links plus an exit. Whole benchmark, `chaindepth=4` / `=0` geomean cycles / instructions: apps 1.000 / 1.000, awfy 1.001 / 1.000, corpus 0.999 / 1.000, corpus2 dev 1.000 / 1.000,
heldout 1.010 / 1.000, jit-trace 1.001 / 1.000, lib 0.999 / 1.000, programs 0.997 / 1.000, real 1.002 / 1.000, real_dis 1.001 / 1.000, **ALL 223: 0.9997 / 1.0000**; `dup` 1.0004 / 1.0000. **The instruction counts of all 223 programs are identical
to the instruction** (the rule never fires on them: `forth_vm` has a chain of at most 8 links, `class_middleclass` / `deltablue` / `richards` / `cd` fewer); the cycle outliers (heldout/prototype_objects 1.048, lib/sel_n 1.042, log_analyzer 1.039) are the noise of the `dup` column (0.951 / 1.029 / 1.062 for the same programs). Targets at 5 runs, Mcyc `=0` / `=4`:
forth_vm 843 / 848 (4156 M instructions both), richards 55 / 55, deltablue 102 / 102, cd 251 / 251, class_middleclass 703 / 701 (dup 846, 55, 102, 250, 701). Default: `CHAINDEPTH_PROCESS = 4` (nothing to lose, wide dispatch gains 5-40 % cycles).
Tuning history: cap only; cap with a coverage test on `served = reach - left` (wrong: the head counter counts since the root's creation, the last link's since its own, so coverage looked ~100 %: no effect); per-link product above (right); coverage 40 % (K=12 mdisp +9 %) -> 30 %.

**G2 `pcallerr`.** Profile of `lib/pcall_err` (299 k caught `error("x")`, `perf record -e instructions:u`): `get_info_aux` 10 %, `where_` 6 %, `i32::to_string` 5 %, `chunk_id` 2.3 %, `memcpy` (moving `DebugInfo`) ~20 %, `realloc` / `grow` / `free` ~8 %,
`shrink_stack` 4.3 % (the reference's `luaD_shrinkstack` in `luaD_pcall`; kept, `collectgarbage("count")` would see the difference). 2970 instructions per caught error. After the change: 1790 (**-39.7 %**), flat profile (`shrink_stack` 10 %, `jit_call` 8 %, `base_pcall` 7 %, `where_buf` 6.5 %, `new_str` 6 %).

| Mcyc / Ginstr | base | new | new / base |
|---|---|---|---|
| lib/pcall_err | 215.3 M / 0.888 G | 116 M / 0.536 G | **0.546 cyc / 0.542 ins** |
| programs/pcall_error (1 M calls, 3 error shapes) | 2263 M | 1481 M | **0.653 cyc / 0.711 ins** (`error("bad " .. i)`: 22 % fewer instructions single-handedly; `error({code=1})` and `xpcall` unchanged) |

(Both decisively over the stop rule of 5 % instructions / 1 % cycles.) Whole benchmark, new / base geomean cycles / instructions: apps 1.010 / 1.000, awfy 1.005 / 1.000, corpus 1.007 / 1.000, corpus2 dev 0.995 / 1.000, heldout 1.002 / 1.000, jit-trace 1.003 / 1.000, lib 0.995 / 0.993, programs 0.983 / 0.987,
real 1.004 / 1.000, real_dis 1.007 / 1.000, **ALL 223: 0.997 / 0.9956** (`dup` 1.001 / 1.000). Instructions are equal (+-0.1 %) on every program that does not raise errors. Layout at equal instructions (second binary, the lottery): `lib/find_pat` **+5.0 % cycles at 15 runs, instructions 1.000** (254 -> 267 M, `MatchState::cm` / `str_find_aux` inlined differently; dup 1.007), the only program over the 3 % line after the re-measure at 15 runs
(programs/varargs +10.7 % and lib/find_pat's +0.7 % instructions were the inlining of the smaller `l_error`, cured by `#[inline(never)]`; string_ops 1.083 / 0.996, sel_count 1.070 / 1.001, havlak 1.039 / 0.976 were noise: the `dup` column showed the same). Judgement: shipped; `find_pat` is a pattern-matcher alignment effect of the new binary and no error path is involved.
Not done (no measurable share left or semantic risk): skipping `shrink_stack` / `shrink_ci` on a pcall that failed (the reference's behaviour, visible through `collectgarbage("count")`), a cached `chunk_id` per prototype (the buffer fill is ~60 instructions), an `add_info` variant for runtime errors (`attempt to ...` messages still use `chunk_id` + `to_string`; not on the benchmarks' paths).
Not changed: `pcall` inside traces (the `pcall` intrinsic leaves the trace on error through the same `jit_call` / `base_pcall` as before; `jit-diff` seeds below).

Correctness of BUNDLE-G: `tr_core` 170 tests (incl. `bg_chaindepth_programs`: `bg_wide` 24 callees, side recordings 156 -> 50 and exits 49741 -> 21217 with `chaindepth=4`; `bg_narrow` untouched; lockstep +verify, `+exitstorm=3`, budget 17, GC stress 2, `+emurun`, both trace modes), `tr_asm`, `tr_bd`, `tr_loop`, `tr_record` (all pass; `--test-threads=3`), `tr_policy` (50, incl. `bg_policy`), `tr_exit` / `tr_hook` / `tr_ir` / `tr_ra` / `tr_runtime` pass, `rn-std` / `rn-core` / `rn-compiler` / `rn-capi` pass,
`vm_corpus` (9 modes incl. incremental / generational GC stress, new `tests/vm/err_16_where_prefix.lua` byte-identical to `ref/55`), jit-diff seeds 1..200 with `JITDIFF_ENTERS=1`: default set `off tr trdef trstorm tremu trstack` 0 diffs (tr installed 6269, entered 2.73 M), new set `JITDIFF_TSET=bg` (wide function-table and method dispatch) `off tr trdef trstorm tremu trstack trverify` 0 diffs.
Mutants: registry 531 -> 533, `chaindepth_ignored` and `chaindepth_cover_ignored` caught one by one with `TR_MUT_ONLY` (`bg_chaindepth_programs` + `bg_policy`; the second only by `bg_policy`), `table_covers_the_registry_exactly_once` and `registry_is_the_frozen_list` pass. Failures identical on the base commit `9dee3775`: `t15_core` (`mid_run_mutation_programs` aborts the binary in `rt.rs:1618` "block structure differs between code versions", re-checked on a clean `git archive` of `9dee3775`), `t15_policy` (aborts the same way in `instruction_counts_are_identical_across_respecialised_versions`), `t15_oracle::oracle_corpus`, `tr_identity_pre::fetch_plus_insns_identity`, `tr_oracle::{oracle_exits, oracle_options, oracle_trace_modes}` (`P8A-MISMATCH` of the two known programs).

## Regression fix after the 2026-11-11 benchmark: `pollr15` default 2, `jit_str_probe` always inlined (2026-11-12)

Report: bench/results/session-2026-11-11 listed real instruction increases against `4ce16b7a` (lib/m_type +13 %, m_min +7 %, m_abs +6 %, m_max +6 %, sel_count +7 %, awfy/sieve +5 %, jit-trace/inline_leaf +4.5 %, int_sum +4.8 %, type_entry_stable +4.4 %; jit-trace set +3.3 % instructions), blamed (unverified) on `chaindepth`. **Not `chaindepth`, `stitch0` or `ccalls`:** on `79e7441c` the knobs `chaindepth=0`, `stitch0=0`, `ccalls=0` change m_type / int_sum by under 0.002 %. Bisect (archive copies under `~/.cache/rg`, all source mtimes touched, otherwise cargo reuses the previous commit's objects and every build looks identical), min of 5 instructions:

* `4ce16b7a` .. `62719481`/`93b98afb`: m_min 1.719 G -> **1.840 G at `93b98afb`** (the P2 placement fix): `POLLR15_PROCESS` 0 turned the r15-addressed trace poll (`cmp byte [r15+d],0`) back into `movabs rax,&poll; cmp byte [rax],0`, one more instruction per loop iteration in every trace (the P2 note above recorded it as a trade for `programs/fib`, but it cost the short-loop `lib/m_*` programs 6-9 % cycles). `programs/fib` is slow with the r15 poll only in its native self-recursion trace (call-frame stores, store-forwarding chain).
* `a90cf5ac` .. `79e7441c`: m_type 1.332 G -> **1.501 G at `79e7441c`**: `pcallerr` (`new_where_str`, a new caller of the string intern code) made LLVM stop inlining `Heap::jit_str_probe` into `asm::short_core` (`jit_tr_alloc` string results: `math.type`, `type`); a separate function with spills, +13 % instructions.

Fix: (1) `pollr15` value **2** = r15 poll everywhere except in traces with `rec_call` / `rec_ret` (native self-recursion); process default `POLLR15_PROCESS = 2` (`0` off, `1` everywhere as before; the struct default stays 0). (2) `#[inline(always)]` on `Heap::jit_str_probe`.

Result, one binary (`new`) with knob columns plus the `79e7441c` binary (`main`) and `dup` (noise), 223 programs, 9 sets, min of 5, CPUs 3-7, geomean cycles / instructions: new/main **0.9935 / 0.9852** (dup/main 0.9934 / 0.9852), `pollr15=1` 0.9939 / 0.9851, `pollr15=0` (the old default) 0.9991 / 0.9966; jit-trace 0.986 / 0.968, lib 0.992 / 0.983, programs 0.995 / 0.986, awfy 0.994 / 0.991, corpus 1.003 / 0.997, corpus2 0.998 / 0.994, apps 1.001 / 0.999, real 0.991 / 0.998. Flagged programs (Mcyc main -> new): m_type 231 -> 210, m_min 254 -> 233, m_abs 267 -> 245, m_max 283 -> 255, sel_count 258 -> 241, str_eq_dyn 215 -> 200, sieve 138 -> 129, inline_leaf 489 -> 458, int_sum 988 -> 962, type_entry_stable 1515 -> 1525 (instructions -4 %); `programs/fib` 1467 -> 1465 (`pollr15=1` would be 1602). m_random is flat (1110 -> 1107 Mi). Programs more than 3 % slower than `79e7441c` in cycles at equal or lower instructions (layout lottery, the same slowness appears in the `pollr15=0` column and in `4ce16b7a`: `79e7441c` was the lucky one): lib/tonumber_int 143 vs 125 (`4ce16b7a` 265), tonumber_flt 204 vs 190 (288), m_floor 273 vs 241 (273; `pollr15=0` keeps 240 at +4 % instructions), str_lt 141 vs 126 (133), str_lt_short 124 vs 114 (117), fmt_q 208 vs 194 (193), tonumber_bad 264 vs 256 (263), corpus/parser_combinator 2343 vs 2248 (2376), corpus2/dev/regex_nfa 726 vs 695 (737). Against `4ce16b7a` none is worse. Outputs equal `.expected` everywhere.

## K1: knob pruning, step 1 (2026-11-12): `vargt1 vargtr fnloops pchk centry sinkpol sinkrate hopcyc recgate recdead probe onestrike ubar absmiss ckey pollbud sinkdemote polygk polyrate framesides uvroll` deleted

Behaviour-preserving cleanup (about 1950 net lines, 27 mutants; docs/JIT_PARKED.md "K1"). Base = `ca12cb50`, new = this branch, both `cargo build --release -p rn-cli --features jit`, binaries at equal-length paths,
`RUSTYNAIL_SEED=0`, 223 programs in 9 sets. Gate: default-config trace dumps (`RUSTYNAIL_JIT_DUMP`: IR text per trace and machine code per trace / proto, addresses masked) identical, instruction counts within 0.2 %.
Results (`scripts/bench_session.py`, min of 3, CPUs 3-7, `dup` = the base binary again):

* **Instructions:user, new / base, 223 programs: geomean 0.99976** (dup 1.00001). 215 programs are within 0.2 %; the 8 outside it are all reductions: `awfy/cd` 0.9928, `corpus2/dev/particles_physics` 0.9956,
  `apps/lru_cache` 0.9973, `corpus/astar_jumper` 0.9974, `corpus/classic_vector` 0.9974, `corpus2/heldout/stream_closures` 0.9975, `apps/events` 0.9975, `lib/co_wrap` 0.9977. Likely cause (not separately measured): the sinking programs lose the
  per-exit `PENDING` thread-local read and the per-materialisation shape accounting that `sinkdemote` / `sinkrate` had left in the exit handler even when off. Cycles geomean 0.994 (3 runs; not a measurement).
* **Trace dumps** (`RUSTYNAIL_JIT_DUMP`, default config, `RUSTYNAIL_SEED=0`, every program of the 9 sets): the IR / snapshot text of all 3203 traces is identical, the machine code of all 5026 trace and proto files is identical after masking
  heap addresses (`movabs` immediates, the two-halves pointer stores, the `[r15+poll word]` displacement, which depends on the binary's static layout), and the `[trace]` / `[jit]` summary lines of 222 of 223 programs are identical.
  The exceptions are noise of the base binary itself, not of the change: `corpus/astar_jumper` has 173 to 179 traces from run to run in BOTH binaries (6 runs each), and `corpus/template_etlua` has 64 vs 66 `proto-<address>.bin` files because
  freed protos reuse an address (the file name) differently in the two binaries (`[jit] compiled 75`, code bytes and all trace statistics are equal).
* Correctness: `jit-diff.sh` seeds 1..100, modes `off tr trdef trstorm`, `JITDIFF_ENTERS=1`: 0 diffs; `tr_core` 164 passed (incl. the mutant runner and `instruction_count_identity`), `tr_policy` 47, `tr_asm` 42 (+2 ignored), `tr_loop` 25,
  `tr_record` 35, `tr_runtime` 44, `tr_exit` 34, `tr_bd` 2, `tr_determinism` 2, the other `tr_*` suites and `rn-jit --lib` (registry count 506) all pass; `rn-std` passes.


## K2: knob pruning, step 2 (2026-11-12): 28 struct-ON knobs hard-wired (`reccont pack gfor mm setmt tailnat closeret tailx idxdeep gforinl gforlua depthstitch nilload sidestitch nrinline setmtinl f1pow f1math f1floor f1strlen monwarm tailfast pcall sideframes fcfn fmat sidefwd landcache`)

Behaviour-preserving cleanup (about 190 inserted / 350 deleted lines in code, tests and scripts; docs/JIT_PARKED.md "K2", docs/JIT_TRACE_REF.md "K2"). Base = `b3a75d85`, new = this branch, both
`cargo build --release -p rn-cli --features jit`, binaries at equal-length paths (`.../tgb/` and `.../tgn/`), `RUSTYNAIL_SEED=0`, 223 programs in 9 sets (apps awfy corpus corpus2/dev corpus2/heldout jit-trace lib programs real), idle machine (CPUs 3-7).

* **Trace dumps** (`scripts/m_identity.py --mask-poll`, default config, `RUSTYNAIL_JIT_DUMP`, every program): the IR / snapshot text and the disassembled machine code of all 6270 traces are identical after masking heap addresses and the `[r15+poll word]` displacement
  (`--mask-poll` is new: it masks r15 displacements of five or more hex digits, which depend on the binary's static layout; unmasked, 175 programs differ in that displacement only), the program outputs and the `[trace]` statistic lines are identical, for 222 of 223 programs.
  The exception is `corpus/astar_jumper`, whose trace count (346-352) differs from run to run in the BASE binary against itself (three base-vs-base runs: 348/348, 352/350, 348/352 traces, text differs): not a result of the change.
* **Instructions:user** (`scripts/bench_session.py`, min of 3, `dup` = the base binary again): new / base geomean **0.99996** (dup 0.99999); all 223 outputs match their `.expected`; 222 programs are within 0.2 %, the worst is
  `corpus2/heldout/real_dis_ppc` at +0.03 % (no program worse than +0.05 %); the only program outside 0.2 % is `corpus/astar_jumper` at 0.9928 (the self-noisy program above).
* **Tests** (`--features tr_mutants`): `tr_core` 160 passed (`--test-threads=3`), `tr_policy` 47, `tr_asm` 42 (+2 ignored), `tr_bd` 2, `tr_loop` 25, `tr_record` 35, `tr_determinism` 2, `tr_exit` 34, `tr_ra` 19, `tr_runtime` 44, `tr_record_calls` 38, `tr_hook` 13,
  `tr_poll` 3, `tr_passes` 14, `tr_ir` 16, `rn-std` all; mutant registry count 506 (`registry_is_the_frozen_list`) and `tr_mutants::table_covers_the_registry_exactly_once` pass; the mutant runner re-checked the mutants whose detectors lost an OFF comparison
  (`fcfn_cells_off`, `fcfn_family_excluded`, `sidefwd_off`, `frame_side_never`, `side_stitch_never`) and the families `tailfast_*`, `aload_nil_skip`, `depth_stitch_never`, `nr_*`, `pcall_*`, `mm_*`, `setmt_*`, `f1_*`, `gfor_*`, `fmat_*`.
* **jit-diff** seeds 1..100, modes `off tr trdef trstorm`, `JITDIFF_ENTERS=1`: 0 diffs (traces in 86 / 85 / 86 % of the seeds).
* Known base failures not re-run: `t15_core::mid_run_mutation_programs`, `t15_policy` (abort `rt.rs`), `t15_oracle::oracle_corpus`, `tr_identity_pre::fetch_plus_insns_identity`, `tr_oracle::{oracle_exits, oracle_options, oracle_trace_modes}`.


## K3a: the tests run the shipped configuration (2026-11-12): `TrLimits::default()` = the process defaults

Test-configuration change only (65 struct defaults moved to their `*_PROCESS` value, `apply_process_limits` keeps `sidemin`, `wgate` / `warm`, `lockstep`; docs/JIT_PARKED.md "K3a", docs/JIT_TRACE_REF.md "K3a"). Production behaviour must not change. Base = `64626ae5`, new = this branch, both
`cargo build --release -p rn-cli --features jit`, binaries at equal-length paths, `RUSTYNAIL_SEED=0`, default configuration (empty `RUSTYNAIL_JIT_TRACE_LIMITS`), 223 programs in 9 sets (apps awfy corpus corpus2/dev corpus2/heldout jit-trace lib programs real).

* **Trace dumps** (`scripts/m_identity.py --mask-poll`): IR / snapshot text and disassembled machine code of all 6276 traces identical, program outputs and `[trace]` statistic lines identical, for 222 of 223 programs. The exception is again `corpus/astar_jumper` (trace count 346-356 differs from run to
  run in the base binary against itself: base vs base 348/372 and 352/354, base vs new 346/354).
* **Instructions:user** (`scripts/bench_session.py`, min of 2, CPUs 9, 10, 13, 15 while other jobs ran; instruction counts are load independent): new / base geomean **0.99999**; all 223 outputs match their `.expected`; 222 programs within 0.2 % (worst `corpus/testrunner_luaunit` +0.18 %), the only one outside is `corpus/astar_jumper` at 0.9972 (the self-noisy program).
* **Tests** (`--features tr_mutants,t15_oracle`): `tr_core` 162 passed (`--skip mutants_are_caught_by_live_programs`, `--test-threads=3`; before: 160 default, and 13 failures under `TR_CORE_PROCESS=1`), `tr_policy` 47, `tr_asm` 42 (+2 ignored; 14 failed with the shipped knobs before the `tier1_pc_addrs` fix), `tr_bd` 2, `tr_loop` 25, `tr_record` 35, `tr_determinism` 2,
  `tr_exit` 34, `tr_ra` 19, `tr_runtime` 44, `tr_record_calls` 38, `tr_hook` 16, `tr_poll` 3, `tr_passes` 14, `tr_ir` 16, `rn-jit --lib` 84 (registry count 506), `tr_mutants::table_covers_the_registry_exactly_once`, `rn-std` all. Known base failures, same before and after: `t15_core` and `t15_policy` (abort in `rt.rs`),
  `t15_oracle::oracle_corpus`, `tr_identity_pre::fetch_plus_insns_identity`, `tr_oracle::{oracle_exits, oracle_options, oracle_trace_modes}`.
* **Mutants** (sharded runner, rows whose detector tests changed: all 21 `*_default_ignored`, `chain_*`, `fside_*`, `frame_*`, `polywait_ignored`, `polyframes_ignored`, `sink_demote_ignored`, `sinkroot_never`, `dynmask_mask_off_by_one`, `fn_short_budget_ignored`, `prelude_*`, `exit_cell_ignored`, `side_maxside_ignored`,
  `side_short_ignored`, `tailfast_close_skip`, `side_stitch_never`, `aload_nil_skip`, `depth_stitch_never`): 62 caught, **1 survived: `chain_no_rebase`** (the detector's `chain_checks >= 2` no longer told 10 checks, with the mutant, from 95, without it: threshold raised to 30; caught afterwards). The full 506 were not re-run.
* **jit-diff** seeds 1..100, modes `off tr trdef trstorm`, `JITDIFF_ENTERS=1`: 0 diffs (traces in 86 / 85 / 86 % of the seeds).

## K3b: mature process-ON knobs hard-wired (2026-11-09)

Behaviour-preserving removal of 25 knobs (docs/JIT_PARKED.md "K3b": library recorders, backend flags, `strself`). Base = `ec084529` (`git archive` into its own tree), new = this branch (three code commits on top of the K3b part-1 test commit), both `cargo build --release -p rn-cli --features jit`,
binaries at equal-length paths (`k3b-tgb` / `k3b-tgn`), `RUSTYNAIL_SEED=0`, default configuration (empty `RUSTYNAIL_JIT_TRACE_LIMITS`), 223 programs in 9 sets.

* **Trace dumps** (`scripts/m_identity.py --mask-poll`, `--a "" --b ""`): IR / snapshot text and disassembled machine code of all 6276 traces identical, program outputs and `[trace]` statistic lines identical, for 222 of 223 programs; the exception is again the self-noisy `corpus/astar_jumper` (352 against 348 traces).
* **Instructions:user** (`scripts/bench_session.py`, min of 3, CPUs 11-15 because 2 and 8-10 were taken; instruction counts are load independent): new / base geomean **1.00003**; all outputs equal the base column; the worst program is `corpus/astar_jumper` at 1.0066 (the self-noisy one),
  the next is `awfy/havlak` at 1.0002; no other program differs by more than 0.02 %.
* **Tests** (`--features tr_mutants,t15_oracle`): `tr_core` 162 passed (`--skip mutants_are_caught_by_live_programs`, `--test-threads=3`), `tr_policy` 40 (K3a: 47; 7 process-default tests of removed knobs deleted), `tr_asm` 42 (+2 ignored), `tr_bd` 2, `tr_loop` 25, `tr_record` 35, `tr_exit` 34, `tr_ra` 19, `tr_determinism` 2,
  `tr_runtime` 44, `tr_record_calls` 38, `tr_hook` 16, `tr_poll` 3, `tr_passes` 14, `tr_ir` 16, `rn-jit --lib` 84, `tr_mutants::table_covers_the_registry_exactly_once` and the order test, `rn-std` all.
* **Mutants:** part 1 ran the full sharded runner on `ec084529` (498 ran, 8 survivors, all fixed: docs/JIT_TESTING.md). After each group the rows whose detectors changed were re-run: library 36 caught, backend 13 caught, `strself` 3 caught. The registry is 499 (7 `*_default_ignored` mutants removed).
* **jit-diff** seeds 1..100, modes `off tr trdef trstorm`, `JITDIFF_ENTERS=1`: 0 diffs (traces in 86 / 85 / 86 % of the seeds).


## BUNDLE-H (2026-11-13): H1 `libcall` SHIPPED (process default 7), H3 `xregs=3` re-measured: stays OFF; H2 below

Method: one binary (`rn-cli --features jit`, release), the knob explicit per column in `RUSTYNAIL_JIT_TRACE_LIMITS` (`libcall=0|1|3|7`, `xregs=0|1|2|3`; `dup` = the `=0` column again), equal-length env strings, all 223 programs
(`scripts/bench_session.py`), identical script paths, `RUSTYNAIL_SEED=0`, CPUs 3-7 one program per core (siblings 11-15 idle), columns rotated, min of 5 runs; flagged programs re-measured at 15 runs serially on CPU 4.
cycles:u / instructions:u. Outputs equal `.expected` for all programs and columns (0 mismatches). Raw: `~/.cache/bunH/{h3,h3c,g2}.json` (not committed).

### H1 `libcall` (bit 0 recorded calls, bit 1 lean `LIB` entries, bit 2 inline `type`, bit 3 `string.rep`; process default **7**)

* **bit 0.** `string.char(a, b)` / `(a, b, c)` (`LIB CHAR2` / `CHAR3`), `math.random()` (`LIB RANDF`: the helper draws and returns the top 53 bits plus one, the trace computes `(raw - 1) * 2^-53`, exactly `Rand::to_float`; the lockstep
  reads the float from the VM slot), `s:lower()` / `upper()` / `reverse()` (`LIB LOWER` / `UPPER` / `REVERSE`, result class = argument class). Before: no recordable kind, `Nyi(Call)` / `CFunc`, the short loop was blacklisted and ran in tier 1.
* **bit 1.** Lean direct entries (`jit_tr_lib_char` / `_random` / `_cmp` / `_strfn`, macro `lean_lib!`) for `CHAR*`, `RANDOM` / `RANDF`, `STREQ` / `STRLT` / `STRLE`, `LOWER` .. `REPS`: no `jit_tr_alloc` dispatch frame. The operands are read with the volatile
  `arg()` (a first version that copied `area.args[0..3]` as an array was merged into a 16-byte load that stalls on store forwarding: `lib/char` 172 -> 245 Mcyc; the F1 comment on `arg()` is right).
* **bit 2.** Inline `type(v)` (`type_fast`: compare the closure's function pointer with `type`'s, then load the upvalue; the helper stays the slow path).
* **bit 3 (OFF).** `string.rep(s, n [, sep])` (`LIB REP` / `REPS`, helper cap `REP_MAX` = 4 MiB, 0 above it). `lib/rep` **+18 % instructions** (about 1457 -> 1720 per iteration), `rep_sep` +3 %: the benchmark's results are 15..25 copies of 1..3 bytes, i.e. both below and above the 40-byte short-string limit,
  so (not verified with exit counts) the result-class guard exits; a trace with a second class behind a side trace costs more than the tier-1 call (the same problem as `SUB`'s class guard, which has no remedy short of a class-polymorphic string type).
  Kept as a knob bit (`libcall=15`), tested (`bh_lib` with `libcall=15`, `lib_rep_follows_the_library`), not shipped.
* **Not built:** inline `STRLT` / `STRLE` of short strings (needs a `bswap` micro-op in the portable IR and the emulator: the lean entry only), inline `RANDOM` generator step (lean entry only), lean entries for `BYTE` (inline already) / `FIND` / `GMATCH` / `FMT` / `TOSTR` / `TYPENAME` / `SUB` (has its own).

Targets (instructions and cycles per call, 5 runs, `off` -> `libcall=7`; `lc1` / `lc3` = bits 0 / 0+1 instructions):

| program | instr/iter off | lc1 | lc3 | lc7 | cyc/iter off -> lc7 | target |
|---|---|---|---|---|---|---|
| lib/char (3 args) | 565.7 | 356.9 | 327.0 | 327.0 | 92.3 -> 60.2 | -340 (got -239) |
| lib/m_random_f | 346.1 | 222.2 | 169.3 | 169.3 | 53.5 -> 25.9 | -95 (got **-177**) |
| lib/lower | 996.1 | 850.6 | 781.7 | 781.7 | 274.1 -> 217.0 | -230 (got -214) |
| lib/upper | 994.8 | 849.4 | 780.5 | 780.5 | 268.6 -> 211.9 | -230 (got -214) |
| lib/reverse | 1000.9 | 828.0 | 768.3 | 768.3 | 274.7 -> 211.9 | -230 (got -233) |
| lib/rep | 1456.8 | (bit 3 off) | | 1456.8 | 295.5 -> 294.7 | -200 (not shipped) |
| lib/type_call | 193.9 | 193.9 | 193.9 | **112.3** | 30.1 -> 17.4 | -85 (got -82) |
| lib/str_lt_short | 193.5 | 193.5 | 163.5 | 163.5 | 33.4 -> 26.0 | -95 (got -30: lean entry only) |
| lib/m_random | 296.0 | 296.0 | 238.1 | 238.1 | 66.3 -> 60.7 | -150..-220 (got -58: lean entry only) |
| lib/str_lt | | | | ratio 0.819 ins / 0.645 cyc | | |

ALL 223 (cycles / instructions geomean, `libcall=N` / `0`): **7: 0.9836 / 0.9839**, 3: 0.9861 / 0.9871, 1: 0.9945 / 0.9927 (and 0.9918 / 0.9920 in the first run); `dup` 1.0011 / 1.0000.
No program above 1.001 in instructions (worst: awfy/deltablue 1.001; `lib/tostring_tbl` 0.983 and `load_big` 0.991 are the best of the non-targets).
Worst cycles at 5 runs, `libcall=7` / `0`, and the 15-run recheck (CPU 4; `dup` in brackets): programs/table_hash 1.059 -> **0.998** (1.002), programs/heapsort 1.039 -> **1.000** (0.999), programs/matmul 1.021 -> **0.999**, lib/fmt_mixed 1.018 -> 1.005 (1.009),
corpus2/dev/event_promise 1.017 -> **0.999**; the other 5-run outliers (prototype_objects 1.051 in `lc1`, int_sum 1.069 in `lc1`, sel_n, csv_report, gc_churn) are the `dup` column's noise (1.045 / 1.034 / ...) and 0.995-1.008 at 15 runs. All with equal instructions.
Gate: **passed** (no program over 3 % at 15 runs, none over 1 % in instructions).

### H3 `xregs=3` re-measure: stays OFF (`XREGS_PROCESS = 0`)

223 programs, 5 runs, `xregs=N` / `0` (cycles / instructions): **3: 0.9820 / 0.9847**, 1 (`rbp` only): 0.9870 / 0.9896, 2 (`r14` only): 0.9907 / 0.9920; `dup` 0.9995 / 1.0000. By set for `xregs=3`: lib 0.9695 / 0.9790, jit-trace 0.9788 / 0.9651,
corpus2 heldout 0.9836, programs 0.9907, awfy 1.0006, corpus 1.0009. Best: lib/str_eq_dyn 0.667, lib/byte 0.750, jit-trace/field_method 0.759, lib/getmt 0.795 (4.7 cyc, was 6.2).
Gate failures at 15 runs (CPU 4, `dup` in brackets): `xregs=3`: programs/heapsort **1.057** cycles (instructions 0.978), jit-trace/rec_loop **1.060** (0.993; IPC 4.84 -> 4.55 at fewer instructions and equal branch/cache misses), awfy/cd 1.022 cycles and **1.0116 instructions**,
awfy/richards 1.012 / 1.0091, awfy/queens 1.012 / 1.006; `xregs=1`: rec_loop **1.059** (sel_count 1.107 -> 0.896 and tonumber_num 1.092 -> 1.029 [dup 1.028] were noise); `xregs=2`: richards 1.035 cycles / **1.015 instructions**, cd 1.015 instructions.
The `spectral_norm` +7.8 % of the old measurement is gone (HEAD_OFFSET 48); `heapsort` and `rec_loop` are new losers with fewer instructions, i.e. a micro-architectural effect of using `rbp` that was not diagnosed.
Experiment (reverted): excluding `rbp` from traces with a native frame push (`rec_call` / `rec_ret`, `xregs` bit 2): `rec_loop` stays at 1.059, so the loss is not in the recursion traces.
Decision: not shipped, the gate rule is not met (two programs above 3 % in cycles, `cd` / `richards` above 1 % in instructions). Retest trigger: a diagnosis of `heapsort` / `rec_loop` (perf annotate of the loop trace), or per-trace allocation of `rbp` only when it removes a loop-carried spill (as the `r14` rule does).
Cheap retest: YES (`XREGS_PROCESS = 3`).

### H2 `tnextinl` (bit 0: `TNEXT` header-safe; process default **1**; the inline traversal step is NOT built)

Mechanism: `opt_mem::header_safe_ins` treats `TNEXT` like a read (it moves only the heap's traversal hint, no table header, array pointer or length hint), so the loop optimiser may peel loops that contain a traversal step; before, the trace of a
`pairs` / `next` loop was rolled and re-ran its entry guards for every element. The generated code is unchanged (the lean `jit_tr_tnext` entry of F1); the hint-carrying `next_hinted` is what the helper already does.

| program (instr / element, cycles / element, 5 runs, `tnextinl=0` -> `1`) | instructions | cycles | ratio ins / cyc | target |
|---|---|---|---|---|
| lib/it_pairs (array, 50 elements) | 156.0 -> 134.5 | 26.8 -> 23.4 | 0.862 / 0.873 | ~40 (not reached: -22) |
| lib/it_pairs_hash | 158.6 -> 137.2 | 26.7 -> 22.6 | 0.865 / 0.848 | ~40 (not reached: -21) |
| lib/it_next (`next(t, k)` by hand) | 164.4 -> 134.1 | 29.3 -> 23.8 | 0.816 / 0.814 | |
| lib/it_next_empty | 176.9 -> 139.0 | 28.2 -> 21.6 | 0.786 / 0.768 | ~45 (not reached: -38) |

ALL 223 (`tnextinl=1` / `0`): **0.9962 cycles / 0.9960 instructions** (`dup` 1.0005 / 1.0000); no program above 1.002 in instructions (corpus/testrunner_luaunit 1.002, astar_jumper 1.003 noise). Worst cycles at 5 runs: programs/table_hash 1.118 (`dup` 1.109: the `off` column's lucky layout),
awfy/havlak 1.051, lib/m_min 1.029, lib/fmt_mixed 1.025; at 15 runs (CPU 4): table_hash **0.994** (`dup` 0.997, instructions 0.9957), havlak 1.004 (`dup` 1.005), m_min 1.013 (`dup` 1.011), fmt_mixed 0.991, oop_mixins 0.999, co_yield 0.995. Gate: passed.
The remaining ~135 instructions per element are the protocol around the helper (arguments to the exit area, budget and live registers saved / reloaded, the pseudo-node read back, the guards on the key tag) and the helper itself (`next_hinted`, `jit_tr_tnext` frame).
**Not built:** the inline scan (array part and hash part in generated code from the carried hint, fast path for empty tables): it needs the pseudo-node result cell protocol of `TNEXT` (`ExitArea.args[1..4]` read by the `HLOAD`s that follow) replaced by register results,
the array tag array and node layout in `asm.rs`, and a hint-mismatch fallback that re-enters the helper; estimated 1-2 bundles. Retest/next step: docs/JIT_PARKED.md C50.

### BUNDLE-H final gate (both shipped knobs together, `libcall=7,tnextinl=1` vs `0,0`, one binary, 223 programs, 5 runs)

ALL **0.9816 cycles / 0.9800 instructions** (`dup` 1.0001 / 1.0000). By set (cycles / instructions): lib 0.9591 / 0.9600, real 0.9597 / 0.9617, corpus 0.9886 / 0.9879, corpus2 dev 0.9985 / 0.9979, real_dis 0.9929 / 0.9924, apps 1.0004 / 1.0000, awfy 1.0005 / 0.9995,
heldout 1.0086 / 1.0000, programs 1.0145 / 0.9998 (the `programs` cycles are `table_hash` and `heapsort`, below). Worst instructions: corpus/testrunner_luaunit 1.002 (all others <= 1.000). Worst cycles at 5 runs and the 15-run recheck on CPU 4 (`dup` in brackets):
programs/table_hash 1.293 -> **0.998** (0.996; instructions 0.9957), programs/heapsort 1.075 -> **0.999** (1.003), corpus2/heldout/prototype_objects 1.055 -> 1.008 (1.002), lib/fmt_s 1.036 -> 0.999, programs/json 1.032 -> 0.999, awfy/havlak 1.027 -> 1.006 (1.000): the 5-run outliers are
layout noise of the `off` column (equal instructions); `programs/table_hash` swings by 8-30 % between columns of the same binary at 5 runs and is stable at 15. Gate: passed.


## BUNDLE-J (2026-11-09): J2 `hash-D` lazy PUC hash SHIPPED (const `LAZY_HASH`), J1 `centry` C-entered function roots SHIPPED (process default 2)

Method: release `rn-cli --features jit`, base = `ac52dd4f` built from a `git archive` copy, new = the tree; equal-length binary directories (`~/.cache/bunJ/{ba,bb,nb,n1}/release/rn-cli`, `bb` = byte copy of `ba` = the `dup` column), identical script paths,
`RUSTYNAIL_SEED=0`, all 223 programs (`scripts/bench_session.py`, CPUs 3-7, siblings idle, columns rotated), min of 5 runs, outliers re-run at 15 runs serially on CPU 4. J1 columns are one binary with `RUSTYNAIL_JIT_TRACE_LIMITS=centry=0|2`.
Outputs equal `.expected` for all programs and columns.

### J2 hash-D (rn-core: `string.rs`, `heap.rs`, `table.rs`, `gc/mod.rs`; feature `eager_hash` = promote at creation, for A/B)

A new short string stores `fast_hash(bytes) ^ seed` (word-at-a-time, u128 folded multiply, last byte added after the mixing) with `extra == 0`; the `luaS_hash` value is computed when the string first becomes a table node key (`promote_hash`: seed recovered
as `field ^ fast_hash`, `extra = 1`). Invariant I-H: an `extra == 0` string is never a node key, so lookups read the raw field (`hgetshortstr`, `findindex`) and, new in the final version, return a miss at once without walking the node vector (the walk started at an arbitrary
node: a cache miss the insertion that usually follows could not reuse; memo_cache +8.5 % -> about +4..10 % cycles, load_small +9 % -> +1 %). Every insert goes through `mainposition` (promotes). The string table keys on `fast_hash ^ seed` (`strtab_hash` for promoted
strings recomputes it from the bytes: `remove_short`, `rehash` growth; shrinking uses the slot number), the chain compare is `(hash == h || extra != 0) && len && bytes`, `intern_concat` gathers into a stack buffer. JIT code unchanged (the `SHORT_HASH` loads are lookups only).

| program | instr base (M) | instr new (M) | ratio | cycles base (M) | cycles new (M) | ratio |
|---|---|---|---|---|---|---|
| lib/lower | 659.6 | 409.6 | 0.621 | 184.2 | 91.3 | 0.496 |
| lib/upper | 713.3 | 442.7 | 0.621 | 194.3 | 99.5 | 0.512 |
| lib/reverse | 741.5 | 455.0 | 0.614 | 205.7 | 105.7 | 0.514 |
| lib/sub | 1190.4 | 910.5 | 0.765 | 193.4 | 136.5 | 0.706 |
| lib/m_type | 1355.3 | 1094.0 | 0.807 | 212.3 | 163.8 | 0.771 |
| lib/tostring_tbl | 699.7 | 577.6 | 0.825 | 151.1 | 109.4 | 0.724 |
| lib/fmt_q | 911.8 | 794.8 | 0.872 | 196.9 | 148.4 | 0.754 |
| lib/gsub_str | 1055.3 | 940.1 | 0.891 | 216.0 | 168.2 | 0.779 |
| lib/gsub_tbl | 936.7 | 860.8 | 0.919 | 191.4 | 153.9 | 0.804 |
| lib/gsub_fn | 1010.6 | 950.7 | 0.941 | 194.2 | 176.4 | 0.908 |
| lib/tostring_int | 578.0 | 537.1 | 0.929 | 134.2 | 130.8 | 0.975 |
| lib/tostring_flt | 306.4 | 293.6 | 0.959 | 85.3 | 81.8 | 0.960 |
| lib/concat_op | 579.1 | 538.7 | 0.930 | 146.2 | 141.6 | 0.968 |
| lib/rep | 1193.4 | 1118.0 | 0.937 | 241.3 | 217.2 | 0.900 |
| lib/char | 937.9 | 887.1 | 0.946 | 172.7 | 176.4 | 1.022 |
| programs/string_ops | 2976.3 | 2844.2 | 0.956 | 1120.3 | 1158.2 | 1.034 |
| programs/table_hash | 1455.5 | 1422.5 | 0.977 | 1032.0 | 727.0 | 0.704 (15 runs 0.836) |
| lib/str_idx_concat_key | 1464.1 | 1402.0 | 0.958 | 283.1 | 288.1 | 1.018 |
| corpus2/dev/memo_cache | 5538.4 | 5580.8 | 1.008 | 1739.2 | 1909.7 | 1.098 (15 runs 1.128) |
| lib/load_small | 894.2 | 859.4 | 0.961 | 217.9 | 219.9 | 1.009 |
| corpus2/heldout/markov_text | 5263.7 | 5060.2 | 0.961 | 1190.6 | 1258.0 | 1.057 (15 runs 1.021) |
| real/wordfreq | 463.5 | 446.1 | 0.962 | 115.5 | 112.4 | 0.974 |

All 223 (new / base, 5 runs): **instructions 0.9830, cycles 0.9793** (`dup` 1.0000 / 0.9998). lib 0.9674 / 0.9565, programs 0.9950 / 0.9874, corpus2 0.9926 / 1.0017, real 0.9881 / 0.9989, apps 0.9940 / 1.0000, awfy 1.0007 / 1.0036.
Worst instructions: memo_cache 1.008, co_yield / oop_mixins 1.006, classic_vector 1.005 (none above 1.01). Gate (lib instr geomean <= 0.975: **0.9674**; lower / upper / reverse <= 0.75: **0.62**; sub <= 0.80: **0.765**; table_hash and tostring_int cycles <= 1.00: **0.836 / 0.964** at 15 runs;
all-223 instructions <= 0.995: **0.9830**; no program above +3 % instructions: **+0.8 % worst**; outputs identical): **passed**.
Cycle outliers at 15 runs (CPU 4, `dup` in brackets): memo_cache **+12.8 %** (instructions +0.8 %), tonumber_flt +7 % and tonumber_num +9.9 % / -7 % in two runs (equal instructions, no strings touched: layout lottery), load_small +9 % before the lookup shortcut (+1 % after),
tremove_pop +4.0 % (equal instructions, `dup` 1.001), the rest <= 2.6 %. **memo_cache is a real cost, not fully explained:** the pattern is lookup-miss-then-insert with promoted keys of 6-12 bytes (`table.concat({...}, ",")` keys); `rehash` growth and `remove_short` recompute `fast_hash` of promoted strings
(the bytes are a second cache line for strings that straddle it), eager promotion (`--features rn-core/eager_hash`) costs +1.6 % cycles on it, the lazy build +8..12 %; the JIT-compiled dynamic-key lookups (`HREFV`) still walk the chain of an unpromoted key (no `extra == 0` shortcut in generated code).
Retest / next step: docs/JIT_PARKED.md C54.

Tests: `cargo test -p rn-core -p rn-std --features jit` (24 result lines, all ok), `-p rn-capi` (4 tests incl. the official suite through `lua.c`, random nonzero seeds, run 3 times), `--release -p rustynail -p rn-cli` (vm_corpus 9 incl. GC stress), `tr_hashd` (pairs order of run-time created keys
through ten producers and ten insert paths against `ref/55/build-seed0/refrun`, `tests/vm_seed0/lazy_hash_pairs_order.lua` + `tests/data/hashd_pairs_order.expected`; promoted hash = `lua_hash(bytes, seed)` for seeds 0, 1, 0xA5A5_1234, 0xFFFF_FFFF over lengths 0..40;
string-table consistency over growth, sweep, shrink and churn with seeds 0 and 0xDEADBEEF; the recorder's main position), `RUSTYNAIL_GC_VERIFY=1 RUSTYNAIL_GC_STRESS=2` over 28 small programs x 2 seeds (verify checks the bucket of every string with `strtab_hash`).
Mutants (registry 513 -> 518): `promote_skip`, `strtab_idx_raw`, `chain_cmp_hash_only`, `record_mainpos_raw`, `promote_seed0`, all caught by `tr_hashd` in the sharded runner. Compiler constants are always promoted (the parser's constant cache is a table keyed by them), so the
recorder's absent-key proof (`short_str_mainpos`) only meets unpromoted strings for run-time keys; the detector is a direct call.

### J1 `centry`: C-entered function roots (process default 2, `centrymin` = 60)

Finding behind it: the old tier-1 `centry` (C40) never fired because `CIST_FRESH` is set after `pre_call_lua_fast` has already counted the entry; the caller's frame (native) is what tells at that point (`rt::hot_call`). Mechanism: (1) a Lua function whose caller's
frame is native (`table.sort` comparator, `gsub` callback, `pcall`, coroutine body) is registered before its first compile (`TrState::centry`), (2) its tier-1 code gets the function-entry head although it has no `CALL` (`DispatchCfg::c_entered`) and the head is a generic function root from the
start (`fnpromo` flag set: no recursion probe) exempt from `fnmin`, (3) it is entered from C although small (`interp_entry`, `heuristic`) and, with `centry=2`, without the entry back-off monitor (`centry=1` keeps it: the monitor blocks the comparators of oop_mixins), (4) the entry from C costs about 280
instructions more than the interpreter's own (hot_call vtable call, `rt::enter`, thunk, tier-1 prologue, exit through the thunk), so a root pays only if it replaces enough interpreter work: a root installed with fewer than `centrymin` (60) IR instructions, or a head that goes `Black`,
switches the prototype back to the interpreter for calls from C (`TrProto::cent_off`; the trace stays installed for compiled callers). Not built: a lean entry path (would let the 6-IR `a > b` comparator win), a native `RETURN` for fresh frames (the function root ends in an always-taken exit before `RETURN`, tier 1 returns).

| program | instr base (M) | instr new (M) | ratio | cycles base (M) | cycles new (M) | ratio |
|---|---|---|---|---|---|---|
| corpus2/dev/oop_mixins | 6700.7 | 5711.5 | 0.852 | 2035.5 | 1517.7 | **0.746** (15 runs 0.737) |
| lib/tsort_cmp | 1034.3 | 1034.2 | 1.000 | 271.8 | 272.2 | 1.001 |
| lib/tsort_int | 905.5 | 905.5 | 1.000 | 260.8 | 261.5 | 1.003 |
| lib/tsort_str | 1016.7 | 1016.7 | 1.000 | 256.0 | 255.4 | 0.997 |
| lib/gsub_fn | 950.7 | 951.8 | 1.001 | 176.0 | 177.0 | 1.006 |
| lib/pcall_ok | 191.2 | 191.0 | 0.999 | 29.2 | 29.6 | 1.012 |
| programs/pcall_error | 7111.8 | 7109.7 | 1.000 | 1484.2 | 1477.2 | 0.995 |
| lib/co_wrap | 1118.9 | 1117.3 | 0.999 | 252.9 | 251.9 | 0.996 |
| corpus/msgpack_pure | 8807.6 | 9164.3 | **1.041** | 2396.7 | 2456.7 | 1.025 (15 runs 1.025) |
| corpus/serpent_ser | 7039.0 | 7046.0 | 1.001 | 2034.8 | 2013.9 | 0.990 |
| corpus/sha2_base64 | 2141.9 | 2141.9 | 1.000 | 425.4 | 431.1 | 1.013 |
| corpus2/dev/template_gsub | 6103.8 | 6132.4 | 1.005 | 1697.2 | 1724.3 | 1.016 |

Target 0.55-0.7 was not reached (0.746 / 0.737): oop_mixins' comparators call `__lt` Lua metamethods on three receiver classes; the exits inside the inlined metamethod frames are blacklisted for side traces (783 K handler calls remain) and each call still pays the entry. Other sort programs (sort_*, tsort_*) and the gsub /
pcall programs have light callbacks (6-30 IR): unchanged by construction (the weight rule), not faster.
All 223 (centry=2 / centry=0, 5 runs): **cycles 0.9992, instructions 0.9995** (`dup` 1.0006 / 1.0000). Weight rule history: `centrymin` 16 lost `co_wrap` +9.1 % instructions, `pcall_error` +3.3 %, `sha2_base64` +1.8 %, `template_engine` +12 % cycles (roots of 19-30 IR); 32 lost `serpent_ser` +3.2 % cycles (46 IR);
60 keeps oop_mixins (roots of 70 and 76 IR) and no other program above +1 % instructions except **msgpack_pure +4.1 % instructions / +2.5 % cycles (15 runs)**: its comparator root (23 IR) is light and switched off, but the earlier recording of that head shifts the recording order of other roots (proto 28's loop root is 15.6 KB instead of 20.7 KB, one
loop is no longer compiled in tier 1): the formation lottery of docs/JIT_PARKED.md C36, not an entry cost (`centry=1` is exactly neutral on it). Explained, shipped. Cycle outliers at 5 runs (`table_hash` 1.16-1.23, `sel_count` 1.08-1.15, `tonumber_num`) move equally in the `dup` column or at 15 runs (0.91-1.03).
Tests: `tr_core::{centry_programs_run_on_traces (4 programs x 2 trace modes x centry 0/1/2 + exit storms, small budget, emulator, GC stress 1/2), centry_small_comparator_is_a_root, centry_blacklisted_comparator_leaves_c_entry}` (programs `j1_sort_objects`, `j1_callbacks`, `j1_errors` (comparator errors, tracebacks,
yield across the comparator = error as in the reference, error objects), `j1_gc_hooks` (collector inside callbacks, call / line hooks, replaced comparators), `j1_small_cmp`, `j1_loop_cmp`), `tr_policy::bj_centry_process_default`, jit-diff set `bj` (JITDIFF_TSET=bj: object sorts with `__lt`/`__le`, gsub callbacks, pcall / xpcall,
coroutine bodies with illegal yields). `fn_root_short_recordings_blacklist_the_head` pins `centry=0` (its `gsub` callback is a C-entered root now). Mutants (registry 518 -> 524): `centry_default_ignored`, `centry_fnmin_kept`, `centry_weight_ignored`, `centry_no_head`, `centry_not_promoted`, `centry_black_keeps_entering`, all caught by the sharded runner;
the fn-root / fnpromo / pack / hrefv / hmiss families re-run and still caught.


### BUNDLE-J follow-up (coordinator review): memo_cache (J2) and msgpack_pure (J1)

Binaries as above (base built from a `git archive` of ac52dd4f; this time every J2 / J1 variant is a separate target dir), final tree vs base, all 223 programs, 5 runs: **cycles 0.9783, instructions 0.9829** (`dup` 0.9990 / 1.0000); lib instructions 0.9675, programs 0.9950,
corpus2 0.9897; worst instructions **+0.6 %** (`lru_cache`, `co_yield`), none above +1 %. J1 alone (`centry` default vs `centry=0`, same binary): 0.9999 cycles / 0.9997 instructions. 15 runs on CPU 4 (`dup` 0.9979): table_hash **0.831** cycles,
tostring_int **0.956**, lower / upper / reverse 0.497 / 0.511 / 0.515 cycles (instructions 0.621 / 0.621 / 0.614), sub 0.707 (0.765), oop_mixins **0.816** cycles / 0.932 instructions, msgpack_pure **0.999 instructions** / 1.008 cycles, pcall_error 0.977 / 0.983.

**J1 msgpack_pure fixed (+4.1 % -> -0.1 % instructions).** `centrywait` (default 8): a C-entered function is compiled with its head at its first hot event from C as before, but is *entered* only from its 8th (`TrState::centry_seen`; each earlier event returns `HotCall::Retry`).
The shift was not the recording moment of the head (a head wait `64 << w` via `promote_fn`, w = 6, 9, 11: msgpack unchanged at 9169 M instructions) and not the weight rule (registration is unchanged): the first entries into compiled code from C change the order in which other roots form.
Rejected variants: deferring the registration (and so the compile) of small functions by 8 hot events fixed msgpack_pure too but made `pcall_error` +26 % instructions, `coro_scheduler` +9 %, `string_key_dyn` +3 % (base-line functions that must compile at the first event). Cost of the shipped variant: oop_mixins 0.74 -> 0.82 cycles (6205 M instructions
instead of 5712 M; `centrywait=1` gives the old numbers and the msgpack_pure shift back; msgpack_pure is neutral from `centrywait=4`; oop_mixins at 2 / 4 / 6 / 8 / 12: 6197 / 6269 / 6490 / 6205 / 6229 M). The wait applies to registered functions only (a first version also delayed the vararg main chunk
called from `xpcall`/`pcall` and failed `tr_runtime` and four `tr_core` tests).

**J2 memo_cache: NOT fixed to 1.03; best variant +5.9 % cycles at 15 runs (1.059; 1.128 before the tier-1 shortcut).** Findings: (1) the cost is the dual hash itself, not the laziness: the eager build (`--features rn-core/eager_hash`, PUC hash at creation) costs the same
+6..7 % in the same run, and with the JIT off the gap is +5..11 % for both; `rehash` growth and `remove_short` recompute `fast_hash` of every promoted string from its bytes (perf: `rehash` 149 samples against 48 for the old `resize_strtab`, about 3 % of the run; promote_hash / intern_short replace `new_str`),
the rest of the gap is spread over cache-bound functions (`propagatemark`, `insertkey_collide`, `adjust_varargs`: more L1 misses, +0.5 M). (2) Variants measured on memo_cache (single runs, cycles, base 1630..1645 M; instructions 5535 M):

| variant | cycles (M) | note |
|---|---|---|
| committed lazy: unpromoted lookup walks the node vector at the raw hash | about 1765 | +7.4 % |
| + miss at once in `hgetshortstr` / `findindex` (60cdc45b) | 1663..1775 | noisy, +2..8 % |
| promote on a missing interpreter lookup, then walk (the "promote on the insert-miss path" variant) | 1761..1807 | no gain |
| + `extra == 0` miss in the tier-1 `GETTABLE` / `SETTABLE` templates and in trace `HREFV` | 1726..1732 | +5.3 %; **`lib/string_key_dyn` +3.1 % instructions / +3.4 % cycles** (3 instructions in each dynamic-key lookup) |
| tier-1 templates only (**shipped**, `string::EXTRA`) | 1727..1743 | 15 runs 1.061; `string_key_dyn` neutral; the `HREFV` check bought nothing on memo_cache and cost the +3 % |
| eager promotion (`eager_hash`) | 1741 | same as lazy |

Next steps (parked, docs/JIT_PARKED.md C54): keep the strtab hash of promoted strings without touching their bytes (16 spare bits of `hnext`, or 8 bits of `extra`/`shrlen`, enough for tables up to 2^13..2^16 buckets) so that growth and sweep need no recompute.
Layout lottery on equal instructions (same binary pair, `dup` 1.000): `lib/tremove_front` +22.9 % cycles in one build (1.02 in the earlier `vb` build of the same tree, 1.01 eager; not above 1.03 in the final 15-run set), `tonumber_flt` +13.3 %, `load_small` +6.0 % (instructions 0.961; +1 % in `vb`), `lib/tsort_cmp` +3.3 % (equal instructions): hot loops of unrelated library code whose alignment moves with any change of
the code before them (docs/JIT_PARKED.md C46); not chased.


## BUNDLE-K (2026-11-09): L1 `lazyci` lazy CallInfo BUILT, gate NOT passed, process default OFF; L2 and L3 not built

Method: release `rn-cli --features jit`; base = `235c1132` (`~/.cache/bunK/ba`), new = the tree (`nz`), `dup` = byte copy of `nz` (`nb`), `lazy` = `nz` with `RUSTYNAIL_JIT_TRACE_LIMITS=lazyci=1` (same binary as `new`); equal-length binary directories, identical script paths, `RUSTYNAIL_SEED=0`, all 223 programs
(`scripts/bench_session.py`, CPUs 3-7, columns rotated), min of 5 runs, reported with `scripts/bench_k_lazy_report.py`. Outputs equal the `.expected` files in every column. Pinned A/B for fib: `ab.sh`-style loop on CPU 5 after a 1 s spin, min of 8.

**Knob.** `lazyci` (`TrLimits::lazyci`, `LAZYCI_PROCESS` = 0, struct default 0). Design and invariants: docs/JIT_TRACE_REF.md "BUNDLE-K L1". Off: every program's instruction count is identical to the base (all 223: instructions geomean 1.0001, cycles 0.9996; dup 1.0012; the cycle outliers `tinsert_front` +21 %,
`tonumber_num` +4.5 %, `template_engine` +4 % have equal instructions, are the layout lottery of docs/JIT_PARKED.md C46 and move in the `dup` column).

### Gate (lazyci=1 against base, 223 programs, min of 5)
| gate | result |
|---|---|
| fib <= 0.88 cycles | **0.919** (instructions 0.893); pinned A/B 0.93-0.95 (min of 8: 123.4 vs 132.1 M cycles, 851 vs 954 M instructions) |
| permute, towers, binary_trees <= 1.01 | permute 0.987, **towers 1.036** (instructions 1.010), **binary_trees 1.037** (instructions 1.030) |
| no program > 3 % worse cycles | only `binary_trees` +3.7 % (and cycle outliers with equal instructions, `dup` explains: `sel_count` +10.7 % with `dup` 0.88, `m_fmod`, `tmove_1k`, `it_pairs`) |
| no program > 1 % worse instructions | `binary_trees` +3.0 %, `astar_jumper` +1.6 %, `coro_generators` +1.3 %, `towers` +1.0 %, `cd` +0.8 % (unexplained beyond: exits that leave a region with lazy frames pay the chain completion that the eager push paid at the push) |
| all-223 geomeans (lazy / base) | cycles 1.0006, instructions 0.9999 |
Result: **L1 fails the fib and the binary_trees / towers gates, so `lazyci` stays 0** (docs/JIT_PARKED.md C55).

| program | cycles base (M) | cycles lazy (M) | ratio (min / median) | instr base (M) | instr lazy (M) | ratio |
|---|---|---|---|---|---|---|
| programs/fib | 1457.1 | 1338.7 | 0.919 / 0.927 | 10556.2 | 9422.7 | 0.893 |
| programs/binary_trees | 2036.8 | 2112.4 | 1.037 / 1.089 | 10434.4 | 10745.9 | 1.030 |
| awfy/towers | 48.1 | 49.8 | 1.036 / 1.036 | 194.6 | 196.6 | 1.010 |
| awfy/permute | 50.3 | 49.7 | 0.987 / 0.983 | 301.2 | 301.3 | 1.000 |
| awfy/queens | 111.7 | 111.9 | 1.002 / 1.002 | 505.3 | 505.4 | 1.000 |
| programs/queens | 1891.3 | 1907.0 | 1.008 / 1.005 | 5989.4 | 5989.5 | 1.000 |
| awfy/list | 140.8 | 139.9 | 0.994 / 0.999 | 719.2 | 719.4 | 1.000 |
| awfy/richards | 56.7 | 57.1 | 1.007 / 1.001 | 180.3 | 180.4 | 1.001 |
| awfy/deltablue | 102.9 | 102.7 | 0.999 / 1.000 | 328.4 | 329.2 | 1.002 |
| awfy/havlak | 161.7 | 162.7 | 1.006 / 0.982 | 511.9 | 512.5 | 1.001 |
| awfy/cd | 252.1 | 249.4 | 0.990 / 0.994 | 799.8 | 806.0 | 1.008 |
| jit-trace/rec_loop | 133.5 | 134.7 | 1.009 / 1.001 | 649.0 | 648.9 | 1.000 |
| corpus/astar_jumper | 1774.2 | 1808.7 | 1.019 / 1.005 | 5230.5 | 5313.6 | 1.016 |
| corpus2/dev/coro_generators | 1381.8 | 1372.7 | 0.993 / 0.998 | 4340.0 | 4397.8 | 1.013 |
Per-set geomeans (lazy / new): apps 1.0024, awfy 1.0014, corpus 1.0004, corpus2/dev 1.0015, corpus2/heldout 0.9996, jit-trace 0.9988, lib 1.0016, programs 0.9999, real 1.0046 (cycles). The programs the study listed (queens: recursion is not native, gap = coverage; ackermann
needs a native tail self-call) are unchanged, as predicted.

### What was measured on the way (fib(32), pinned, `cycles:u`)
* Study ceiling patch re-applied to this tree: eager 133.0 M; lazy fields only (no `top`, `callstatus`, `trap`, `kcache`, callee `savedpc`, `thread.top`) 119.0 M; plus no `CIST` guards on return 113.5 M; both plus the first-flag store 115 M. So the sound design had 0.85-0.89 available, and the first working version
  measured 116.9-119 M (0.878-0.89).
* That first version let the root frame's return leave the trace (the return guard needs a complete frame, so `r13 == root` failed to the exit): `awfy/towers` ran **5.15x** slower (1.55 M exits against 0.53 M; regions of 2-6 frames end on every call), `binary_trees` 1.33x, `astar_jumper` 1.48x. Fix: the root frame returns natively
  with the eager `CIST` tests and eager pops and hands the root to its caller (6 instructions per return: fib back to 0.93-0.95).
* The frameful exits (inlined frames, `frame_cells`, the H2 `mat` stub) had been forced to the handler in the first version (towers 2.4x after the root fix); keeping them in line (the cell stub continues in the side trace, the `mat` stub completes the chain first) brought towers to 1.04.
* `binary_trees` leaves a region 2.8 M times on a missing spare `CallInfo` after the collector shrank the list (`rec_fail`, eager tier-1 path before); the Rust completion helper took 10 % of the run, the in-line completion loop (`AsmEnv::lazy_cs`, all CALLs of the prototype want the same results) took it to +3 % instructions.
* Per call of fib after L1 (instructions, dump): down half 49 (poll 3, budget 2, type guard 2, `n < 2` 3, callee-identity chain 16, writeback 5, push 9, room checks 7, first flag 2), the callee-identity chain is the L2 target (docs/JIT_PARKED.md C55).

### L2 (callee-identity hoist) and L3 (transition diet)
Not built: both are gated "not worse than L1" and L1 is not ON. The analysis (what is redundant after a native self-push, what the layout change in the IR would be, the instruction counts) is in docs/JIT_PARKED.md C55.

### Tests, mutants (docs/JIT_TESTING.md "BUNDLE-K")
`tr_core` without the mutant runner: 165 -> 168 passed with the process default OFF and also with the default switched to ON (168 passed, temporarily); `tr_policy`, `tr_asm`, `tr_loop`, `tr_record`, `tr_exit` (+1), `tr_ra`, `tr_determinism`, `tr_runtime`, `tr_record_calls`, `tr_hook`, `tr_poll`, `tr_passes`, `tr_ir`, `tr_class_a_pb`, `tr_hashd` pass;
observer programs against the reference interpreter in 8 modes: 16/16 identical; jit-diff seeds 1..60 (not 1..200: the 200-seed runs exceeded the 50-minute leash) with `lockstep=1,lazyci=1`, modes off tr trdef trstorm tremu trstack: set `bk` 0 diffs (47 of 60 seeds ran traces in `tr`), default set 0 diffs.
Mutants: 16 new (registry 525 -> 541), all caught (`TR_MUT_ONLY=lazy_`); three further injections were dropped as unobservable (see docs/JIT_TRACE_REF.md).

## BUNDLE-L (2026-11-09): array loops. A `aidx` and B `bhoist` BUILT, both process default OFF (gates not met); C, D, E+H not built

*Update (BUNDLE-M M2): `bhoist` is ON by default now (`BHOIST_PROCESS = 1`), see "BUNDLE-M" below; `aidx` stays OFF.*

Study input: ~/.cache/studyMatmul (matmul 39 instructions per iteration against LuaJIT's 17; the cycle gap is the cache footprint of the split layout, 3 cache lines per row, not the instruction count). Ranked steps A, C, D, B, E+H; this bundle built **A and B** (C needs a decision about the observability of pruned
temporaries, D needs value rematerialisation in exit stubs; see docs/JIT_PARKED.md C56).

Method: release `rn-cli --features jit`, base = `b8ee57c1` (`ba`), `new` = the tree with every knob off (`nw`), `dup` = byte copy of `new` (`nb`), `a` / `b` / `ab` = `new` with `RUSTYNAIL_JIT_TRACE_LIMITS=aidx=1` / `bhoist=1` / `aidx=1,bhoist=1`; equal-length binary directories, identical script paths, `RUSTYNAIL_SEED=0`, all 223 programs,
`scripts/bench_session.py` (CPUs 3-7, columns rotated), min of 5, `scripts/bench_k_lazy_report.py`; outliers rechecked with 15 runs on one pinned CPU (`~/.cache/bunL/ab.py`). Outputs equal the `.expected` files in every column. `new` against `base`: instructions geomean 1.0001, cycles 0.9996 (the lib/* cycle outliers with equal instructions are
the layout lottery of docs/JIT_PARKED.md C46: `new` and `dup` move against `base` and each other).

### A `aidx` (`TrLimits::aidx`, `AIDX_PROCESS` = 0): the complemented array index `~i` as an IR value
The B2 payload address `[base + 8 * ~u]` was `mov V7, u; not V7` in front of every `ALOAD`. With `aidx` the loop optimiser gives the `AREF` of every `ALOAD`-ed non-constant index (peeled traces only) a third operand in `aux` (`Ins::third`, low half = ref, high half = displacement, `ir::aref_aux` / `aref_disp`): `NEG k` with
displacement `-c - 1` for `u = k + c` (`~(k + c) == -k + (-c - 1)`, wrapping), `BNOT u` otherwise. The value is an ordinary pure instruction: CSE shares it between the loads of one index, it is hoisted when the index is loop invariant, the register allocator keeps it live to the loads (`ra::operand_refs`: `ALOAD` operands `[base, u, n]`), the assembler addresses
`[base + 8 * n + 8 * d]`. `ASTORE` ignores it (four operands would be needed; an `AREF` shared by an `ALOAD` and an `ASTORE` still carries it, the store keeps `mov; not`). Cleanup drops it when the `ALOAD` was forwarded away.
**Result: no gain.** matmul instructions 1.000 (2563.8 M -> 2564.7 M): the 7 allocatable registers are all busy in the inner loop (two array bases, two sizes, `j - 1`, `k`, `count`, the budget), so the new values are spilled (`mov [r15 + off], r; ... mov r, [r15 + off]`) and the saved `mov; not` pairs are paid back by store/reload pairs.
| program | cycles new -> a | instructions new -> a |
|---|---|---|
| programs/matmul | 433.3 -> 456.1 M (1.053) | 2563.8 -> 2564.7 M (1.000) |
| programs/spectral_norm | 199.1 -> 206.3 M (1.036) | 1742.8 -> 1781.6 M (1.022) |
| programs/heapsort | 1472.1 -> 1503.2 M (1.021) | 3251.3 -> 3257.1 M (1.002) |
| programs/nbody, fannkuch, jit-trace/dot, array_rec | 0.998, 1.000, 1.000, 1.000 | 1.000, 1.001, 1.000, 1.000 |
| programs/sieve | 562.5 -> 568.4 M (1.011) | 3206.4 -> 3276.1 M (1.022) |
All 223 (a / new): cycles geomean 1.0010, instructions 1.0002. **Gate (matmul <= 0.88 cycles) FAILED**, `aidx` stays 0. Cheap retest: after a change that frees registers in the inner loop (C, D, or `xregs=3` made cheaper) run `RUSTYNAIL_JIT_TRACE_LIMITS=aidx=1`: the mechanism is complete and tested.

### B `bhoist` (`TrLimits::bhoist`, `BHOIST_PROCESS` = 0 when built, 1 since BUNDLE-M): bounds checks of a counted loop hoisted into its pre-roll
`opt_loop::Em::plan_bhoist`, in `peel_loop` right after the pre-roll copy: for a loop with the continue guard `NE count, 0` whose slot ends the pre-roll as `count - 1`, and bounds guards `ULT k + c, asize` of a slot `k` that ends the pre-roll as `k + 1` (the `FORLOOP` shape; step 1 is the recorded `EQ step, 1`), it emits per bound
`count0 ULT asize`, per `(bound, c)` `k0 + c ULT asize` and `k0 + count0 + c ULT asize` (`count0`, `k0` = the pre-roll's final values) on a snapshot of the loop-top state, and the body copy skips the covered `ULT` when its bound is the very pre-roll SSA value (`bhoist_covers`). Why that is enough: body iteration `i` runs only if
`i <= count0`, its index is `k0 + i + c`, `count0 < asize < 2^32` rules out wrapping, and a peeled trace has loop-invariant headers (the optimiser gives up on header-mutating ops, TR13.4). The `count0` guard is needed for counters that are not a `for` loop (a `while` with `n = n - 1` from `n = -3` wraps). Fallback: a root with hoisted guards that leaves
before its body most of the time (the loop runs past the array part: `sieve`) is flushed once as ineffective by `cache::pre_exit_check`, and the head is recorded again without the hoist (`HeadData::nohoist`, `head_nohoist`, `Trace::hoisted`, `TraceIr::hoisted`; no penalty, like `sinkroot`). Without the fallback `programs/sieve` ran 2.63 x slower.
| program | cycles new -> b | instructions new -> b |
|---|---|---|
| programs/matmul | 433.3 -> 427.5 M (0.987) | 2563.8 -> 2202.9 M (0.859) |
| programs/spectral_norm | 199.1 -> 189.7 M (0.953) | 1742.8 -> 1678.2 M (0.963) |
| programs/fannkuch | 1119.1 -> 1114.3 M (0.996) | 5722.1 -> 5594.9 M (0.978) |
| jit-trace/dot | 1234.9 -> 1220.5 M (0.988) | 12130.2 -> 10138.7 M (0.836) |
| jit-trace/array_rec | 1818.8 -> 1818.8 M (1.000) | 5851.0 -> 5256.9 M (0.898) |
| programs/heapsort | 1472.1 -> 1506.1 M (1.023, layout: the instructions are equal) | 3251.3 -> 3249.9 M (1.000) |
| programs/nbody | 525.0 -> 523.6 M (0.997) | 2993.9 -> 3021.0 M (1.009) |
| jit-trace/int_rec2 | 928.1 -> 688.3 M (0.742) | 8198.5 -> 6710.6 M (0.819) |
| lib/str_eq_dyn, len_op, getmt, byte | 0.707, 0.803, 0.800, 0.793 | 0.688, 0.787, 0.813, 0.787 |
All 223 (b / new): cycles geomean **0.9877**, instructions **0.9835**; per set cycles apps 1.001, awfy 0.997, corpus 0.998, corpus2/dev 1.002, heldout 1.003, jit-trace 0.989, lib 0.975, programs 1.001, real 0.998 (instructions: jit-trace 0.963, lib 0.977, programs 0.992, the others 0.995-1.000). `ab` (both): cycles 0.9893, instructions 0.9833 (matmul 0.974 / 0.809).
**Gate** (matmul <= 0.62 instructions and <= 0.93 cycles, which assumed A + C + D): **FAILED** (0.859 / 0.987): matmul's cycles are the cache footprint, the removed instructions do not show; the 2-D access `b[k][j]` keeps its per-row bound check (the row is loaded in the loop). Universal gate: no program more than 3 % worse in cycles after the 15-run
recheck (gc_churn +7 %, string_ops +6 %, awfy/havlak +3 % in the 5-run table were noise: 15 runs 0.997 / 0.991 / 1.000 with `dup` at 0.996 / 0.996 / 0.998; lib/match +4.5 %, corpus2/heldout/log_analyzer +2.3 %, lib/it_ipairs +4 % (ab) have **equal instructions** and `dup` differs by 0.9-1.6 % (layout lottery, reported, not chased));
instructions more than 1 % worse (explained): lib/tsort_int +3.5 % and lib/tsort_cmp +1.8 % (one more trace is compiled: recorded 3 -> 4), lib/type_call +3.6 % (short loops: the pre-roll guards are paid per entry and the loop saves less than they cost), corpus2/dev/expr_compiler +1.3 % (cycles +1.8 %, different trace dynamics: entered 3.5 M -> 2.8 M),
corpus2/dev/event_promise +1.2 %, programs/queens +1.6 % (ab only), lib/sel_n. Outputs equal in all columns. `bhoist` stays 0 because the stated gate is not met; the all-223 numbers are the evidence for switching it on (`BHOIST_PROCESS = 1`, a one-line change, no other effect) once C and D exist or the owner accepts the universal gate alone.

### Not built
* **C** (snapshot dead-slot pruning): the registry already holds the forbidden form (`snap_prune_dead`); the sound form needs a policy for `debug.getlocal` of temporaries (an `xpcall` handler reading the frame sees `(temporary)` slots; `tests/bl/bl_obs1.lua` prints them against the reference). See docs/JIT_PARKED.md C56.
* **D** (single loop variable) needs a rematerialised snapshot entry (`count = end - k`) in the exit stubs; **E + H** (fold single-use loads, drop redundant tag stores) were not started. F and G stay parked.

### Tests, mutants
See docs/JIT_TESTING.md "BUNDLE-L".

## BUNDLE-M (2026-11-09): hygiene

### M2 `bhoist` is ON by default (`BHOIST_PROCESS = 1`)
The only blocker was `tr_core::type_stability`: its `type_late` expectations ("exactly one re-record", `ineffective 1`, the root and its side trace ping-pong until rule Q2) encode the pre-hoist trace dynamics. With the hoisted pre-roll guards the same program is handled by side traces whose recordings abort with `LeaveFrame` (6 recorded / 1 installed / 5 failed) and never reaches Q2 (`rerec 0`, `ineff 0`, no hot type-guard exit, no Black head, the float trace entered 398 times). This is a changed
dynamic, not a policy problem: `bench/jit-trace/type_late.lua 300000` runs in 0.185 s with `bhoist=0` and with `bhoist=1` (cycles and instructions of the 223-set `jit-trace/type_late`: 1.000 / 1.000). `type_stability` now pins `bhoist=0` (its intent is unchanged: the pre-hoist dynamics stay covered) and the new `type_stability_hoisted` restates the `type_late` intent under hoisting (at most one re-record, no hot type-guard exit, a side trace installed, no head Black, the float trace runs, no trace explosion).

Re-measure, release `rn-cli --features jit`, same binary with `RUSTYNAIL_JIT_TRACE_LIMITS=bhoist=0` (off) against the default (on), `RUSTYNAIL_SEED=0`, identical script paths, one pinned CPU (4), spin 300 ms, **15 interleaved runs, min** (cycles / instructions, on / off):

| program | cycles | instructions |
|---|---|---|
| programs/matmul | 0.985 | 0.859 |
| programs/spectral_norm | 0.954 | 0.963 |
| programs/fannkuch | 0.994 | 0.978 |
| jit-trace/dot | 0.990 | 0.836 |
| jit-trace/array_rec | 1.000 | 0.898 |
| jit-trace/int_rec2 | 0.743 | 0.819 |
| programs/heapsort / nbody / sieve | 1.002 / 0.998 / 1.000 | 1.000 / 1.009 / 1.000 |
| lib/tsort_int | 1.011 | 1.033 |
| lib/tsort_cmp | 1.003 | 1.018 |
| lib/type_call | 0.983 | 1.035 |
| corpus2/dev/expr_compiler | 1.007 | 1.013 |
| corpus2/dev/event_promise | 0.996 | 1.000 |
| programs/queens | 0.995 | 0.999 |
| lib/sel_n | 1.023 (equal instructions: layout lottery) | 1.000 |

No program is more than 3 % worse in cycles (worst: `lib/sel_n` +2.3 % with equal instructions, `lib/tsort_int` +1.1 %). The whole-suite numbers of BUNDLE-L (all 223: cycles 0.9877, instructions 0.9835) are unchanged by this switch. Instruction regressions as listed there (`tsort_int` +3.3 %, `type_call` +3.5 %, `expr_compiler` +1.3 %). Opt-out: `RUSTYNAIL_JIT_TRACE_LIMITS=bhoist=0`.
Tests with the default on: `tr_core` (all but `mutants_are_caught_by_live_programs`; 171 + `type_stability_hoisted`), `tr_policy` 43, `tr_loop` 27, `tr_exit`, `tr_runtime`, `tr_record*`, `tr_hook`, `tr_poll`, `tr_passes`, `tr_ir`, `tr_class_a*`, `tr_hashd`, `tr_ra`, `tr_asm`, `tr_determinism`, `tr_bd`, `tr_tbl_move`, `tr_ref` and the lib tests all pass; jit-diff seeds 1..100, modes `off tr trdef trstorm tremu trstack`, sets `tr` and `bl`: 0 diffs, 0 timeouts (`JITDIFF_ENTERS=1`).

### M3 strtab tag: no `fast_hash` recompute in string-table growth and sweep (J2 leftover, memo_cache)
Problem (docs/JIT_PARKED.md C54): a promoted short string keeps the PUC hash in `hash`, so `rehash` growth and `remove_short` recomputed `fast_hash(bytes) ^ seed` from the string's bytes to find its bucket. Fix: `promote_hash` (cold) now stores the low 19 bits of the string-table hash (the `hash` field just before it is overwritten) in the free bits of the `hnext` word of the string (bits 48..63 = `tag >> 3`, bits 0..2 = `tag & 7`; x86-64 only, user pointers < 2^47, strings 8-aligned; other targets keep
the plain pointer and recompute). `LuaString::hnext` masks the tag on every chain step, `set_hnext` keeps it, `init_hnext` (insertion, not promoted yet) writes a plain link, `strtab_index(p, seed, size)` takes the bucket from `hash` (unpromoted), from the tag (promoted, `size <= 2^19`) or recomputes (promoted, larger table). `u` of `LuaString` is an `UnsafeCell<StrU>` so that the cold promotion may write the link word through `&self`. `Heap::verify` (GC_VERIFY) checks the tag of every promoted string in the table. With `eager_hash` the promotion runs after the link.
A first version tagged every string at insertion (`assert!` in the link writes, tag computed per new string): lib instructions +0.22 % (tostring_int +2.9 %); tagging at promotion costs nothing for strings that are never keys.
memo_cache's string table reaches 2^19 buckets (262144 strings), exactly the tag width; larger tables recompute (`hashd_string_table_beyond_the_tag`, 530 K strings, covers the switch and the shrink back).
Numbers (release `--features jit`, `ba` = base `77b55a83`, `nw` = this tree, `dp` = copy of `nw`, equal-length directories, `RUSTYNAIL_SEED=0`, all 223 programs, CPUs 3-7, min of 5; then 15 serial runs on CPU 4 for the rows). Outputs equal the `.expected` files in every column.
| | cycles nw/ba | instructions nw/ba |
|---|---|---|
| all 223 (geomean) | 1.0009 (noise: dup/nw 1.002; the 5-run table has +-10 % outliers on loaded runs, none reproduced at 15) | 1.0002 |
| lib (95) | 1.0012 | 1.0005 |
| corpus2/dev/memo_cache (15 runs) | **0.975** (dup 0.977) | 0.997 |
| programs/table_hash (15 runs) | 0.974 (dup 0.974) | 0.993 |
| lib/tostring_int | 1.006 | 1.001 |
| lib/sub | 1.017 (dup 1.017: deterministic) | 1.007 |
| lib/lower, upper, reverse | 0.992, 0.997, 1.004 | 1.004, 1.004, 1.004 |
For reference the `eager_hash` build is 0.991 on memo_cache against the same base: the lazy dual hash costs nothing there any more. The +5.9 % of C54 is gone (memo_cache is 2.5 % *faster* than the tree before the change, the walks of `rehash` / `remove_short` no longer read the string bytes). J2 gates (all against the pre-J2 base, unchanged by +0.4..0.7 % instructions of the chain-walk mask): lib instructions geomean +0.05 % over the previous tree, lower / upper / reverse / sub instructions +0.4..0.7 %, tostring_int +0.1 %, table_hash cycles 0.974. Cost accepted: one `and` per chain step.
Tests: `tr_hashd` (5), `rn-core` + `rn-std --features jit`, `rn-capi` differential (see docs/JIT_TESTING.md BUNDLE-M). jit-diff seeds 1..100, modes `off tr trdef trstorm tremu trstack`, sets `tr` and `bl` on the final tree: 0 diffs, 0 timeouts. Mutants (registry 549 -> 552): `strtag_zero` (the tag reads 0), `strtag_shift` (index from `tag >> 1`), `strtag_wide` (the tag is used for tables larger than 2^19): all crash or fail `hashd_string_table_stays_consistent` / `hashd_string_table_beyond_the_tag`.


## BUNDLE-P (2026-11-14): P2 `f2ifl` SHIPPED (process 1), P3 `fast_hash_fresh` SHIPPED (const), P1 `rearmside` built, OFF

Mechanisms, tests and mutants: docs/JIT_TRACE_REF.md "BUNDLE-P"; diagnosis and what is not built: docs/JIT_PARKED.md C58. Commits: P2 `40247e8d`, P3 `fa4f8dd6`, P1 `97cd3b64` (base `2c199d9b`, plain `cargo build --release -p rn-cli --features jit`).

**Method.** Three binaries at equal-length paths (`bin/{a,f,g}/rn-cli`: a = base, f = base + P1 + P2 + P3, g = f with `BYTE_TAIL = false`), `RUSTYNAIL_SEED=0`, equal-length environment (`RUSTYNAIL_PAD`), identical script paths, CPUs 3, 5, 6, 7 (one program per core; CPU 4 for the serial rechecks), columns rotated, min of 5 runs, cycles:u / instructions:u (`scripts/bench_session.py`, 223 programs in 9 sets), outputs equal `.expected` in every column (0 mismatches). Columns: `base`, `new` (= f, shipped defaults), `nof2` (f, `f2ifl=0`), `side3` (f, `rearmside=3`), `nobt` (g), `based` (the base binary again: the noise column). Geomean over the 223 programs, ratio to `base` (cycles / instructions): `new` 0.9983 / 0.9990 (noise column 1.0002 / 1.0000), P2 alone (`new` / `nof2`) 1.0001 / 0.9993, P3 alone (`new` / `nobt`) 0.9985 / 1.0004, `side3` 0.9966 / 0.9984.

### Targets (5 runs on four cores, 15 runs serial on CPU 4 in brackets)

| program | base Mcyc | new cycles | new instructions | note |
|---|---|---|---|---|
| lib/m_floor | 272.9 | **0.826** (0.831) | 0.877 | P2, gate <= 0.85 |
| lib/str_idx_concat_key | 288.7 | **0.942** (0.940) | 1.027 | P3, gate <= 0.95 (`nobt`: 1.003 / 1.007) |
| lib/tostring_int | 130.3 | **0.886** (0.893) | 1.021 | P3 (`nobt`: 1.002 / 0.998) |
| corpus/classic_vector | 1182.8 | 0.989 (1.000) | 1.000 | P1 OFF: **gate <= 0.92 NOT met in the shipped configuration** |
| corpus/classic_vector, `rearmside=3` | | 0.904 (0.912) | 0.915 | gate met with the knob, see below |

P2 neighbours (cycles / instructions): lib/m_abs 0.994 / 1.000, m_min 1.007 / 1.000 (noise column 1.014), m_max 1.000 / 1.000, m_sqrt 0.976 / 1.000, m_fmod 0.999 / 1.000, sel_loop 1.000 / 1.000, sel_count 0.900 / 1.000 (noise 0.911), sel_n 1.044 / 1.000 (noise column 1.032; 15 runs 1.029 against 1.021 for the base binary again). There is no `lib/m_ceil` program; `math.ceil` shares the `FFLOOR` path and is in `bp_floor`. P3 neighbours (the gate list): concat_op 0.988, sub 1.010 (15 runs 1.014, instructions 1.000, `nobt` 1.012: layout), fmt_d 0.980 / 0.967, lower 0.992, upper 0.997, reverse 0.994, table_hash 1.061 at 5 runs but 0.999 at 15 runs (noise column 1.017), memo_cache 1.015 at 5 runs, 1.001 at 15, m_type 0.991 / 1.000. All-223 instructions 0.9990 (gate: not above +0.3 %). The first version of P3 used the byte loads in every `fast_hash`: `lib/m_type` +5.8 % instructions / +7.5 % cycles, `lib/sub` +1.8 % with a run-time flag; the shipped `fast_hash_fresh` with a const-generic `short_core` restores both (m_type, sub: instructions equal to the base).

### Programs outside the gates (new / base, cycles; instructions in brackets; noise column in braces)

Equal instructions, i.e. code-layout effects of a different binary (the base binary run again gives 1.000 in each case; each is reproducible; 15-run serial min in the last column; the `nobt` binary, which differs from `new` only by P3's const, moves most of them, so this is the layout of the binary, not the hash):

| program | 5 runs | 15 runs | `nobt` (15 runs) |
|---|---|---|---|
| lib/m_random_f | 1.143 (1.000) {0.997} | 1.149 | 1.072 |
| lib/gsub_str | 1.080 (0.992) {0.998} | 1.087 | 0.981 |
| lib/gsub_tbl | 1.079 (0.990) {0.987} | 1.083 | 1.081 |
| corpus/parser_combinator | 1.044 (1.003) {1.002} | 1.046 | 1.019 |
| corpus/markdown_conv | 1.040 (0.999) {1.004} | 1.043 | 1.000 |
| corpus/template_etlua | 1.036 (1.002) {1.000} | 1.038 | 1.033 |
| apps/markdown | 1.031 (1.002) {1.000} | 1.031 | 1.012 |
| programs/json | 1.041 (1.001) {1.005} | 1.029 | 1.030 |

These eight break the "no program more than 3 % worse" rule in the letter and are **not chased**, as the layout-lottery rule asks (equal instructions, noise column 1.000). `lib/m_random_f` and `lib/gsub_str` are the large ones (+15 % and +9 %); PGO builds (`scripts/pgo-build.sh`) have pinned layouts and were not measured here. No program is more than 1 % worse in instructions except where the change intends it: lib/tostring_int +2.1 %, lib/str_idx_concat_key +2.7 % (P3 trades 8 byte loads for a 4-byte forwarding stall: cycles -11 % / -6 %), lib/pcall_err +1.3 % (cycles 1.000 at 15 runs), lib/tostring_tbl +1.0 %, programs/table_hash +0.9 %.

### P1 `rearmside` (built, OFF): `rearmside=3` against the same binary without it

Winners and losers of the knob (5 runs, cycles / instructions): corpus/classic_vector 0.904 / 0.915 (15 runs 0.912); losers: **corpus/lua_lexer 1.043 / 1.048** (15 runs 1.041), awfy/json 1.034 / 1.013 (15 runs 1.028), awfy/queens 1.032 / 0.962 (15 runs 1.033); all other programs within 2 % of the run without the knob (apps/markdown 1.041 against 1.031 for `new` is the layout effect above), geomean 0.9984 / 0.9994. Sweep of `k` on classic_vector (instructions, one binary of the study): 1: 0.87 (heapsort 1.53x, iter_pipeline 15x, awfy/queens 1.37x), 2: 0.88 (awfy/queens 1.37x, sudoku 1.05x), 3: 0.915, 4: 0.943, 5: 0.95, 6: 0.963; a minimum age at `k` = 3 (`entry_ok >= 16 / 32 / 64`) changes nothing (0.915, `lua_lexer` stays), 128: 0.950, 256: 0.985. Global alternatives, all 223 programs: `hot_exit` 8: classic_vector 0.868, geomean 1.008, heapsort 1.57x, regex_nfa 1.44x, awfy/queens 1.33x; `hot_exit` 4: 0.817, geomean 1.021, iter_pipeline 16x; `REARM_MIN_ENTRIES` 512: nothing gained, 256: iter_pipeline 10.6x, 128: 15.7x. The gate (classic_vector <= 0.92 and no program > 3 % / 1 %) is missed by lua_lexer (+4.8 % instructions) and awfy/queens (+3.3 % cycles), so the default is 0. The knob is `RUSTYNAIL_JIT_TRACE_LIMITS=rearmside=3` (one constant, `REARMSIDE_PROCESS`).

### Correctness runs

`cargo test -p rn-core -p rn-std --features jit` (106 + 0 ignored pass), rn-jit `--features trace`: `tr_core` 173 (`--skip mutants_are_caught_by_live_programs --test-threads=3`), `tr_policy` 45, `tr_asm` 42, `tr_loop` 27, `tr_record` 34, `tr_exit` 34, `tr_ra` 19, `tr_determinism` 2, `tr_runtime` 43, `tr_record_calls` 37, `tr_hook` 13, `tr_poll` 3, `tr_passes` 14, `tr_ir` 16, `tr_class_a_pb` 19, `tr_hashd` 6, `mutants::tests` registry (557) and `tr_mutants::table_covers_the_registry_exactly_once`: all pass. Mutants: the 5 new ones all caught; 34 mutants of the touched families (`snew_*`, `concat_*`, `lib_bytes_small`, `lib_no_gc_check`, `promote_*`, `strtab_*`, `chain_cmp_hash_only`, `record_mainpos_raw`, `strtag_*`, floor / `f2i` / `tostr` / `rearm_*` / `tnext*`) re-run with `TR_MUT_ONLY`: 34 caught, 0 survived. jit-diff seeds 1..100, modes `off tr trdef trstorm tremu trstack` (+ the reference build as `ref`): set `bp` 0 diffs (80 % of the seeds ran traces), set `bp` with `lockstep=1,rearmside=3` 0 diffs, default set `tr` with `lockstep=1,rearmside=3` 0 JIT diffs (86 % ran traces; the only diff is seed 27 `off` against `ref`: a `collectgarbage("count")` delta, nothing to do with the JIT), set `t1i` 0 diffs. `bp_floor` and `bp_floor_minint` also compared byte for byte with `ref/55/build/lua` in default, `JIT=always TRACE=always` and `JIT=on` threshold-2 runs. Not run: the 5.4 build, the ASan scripts (`scripts/asan.sh`), the fuzz targets, the `rn-capi` differential, GC-verify runs of the new programs beyond `RUSTYNAIL_GC_STRESS=1/2` inside `tr_core::bp_*`, the full 557-mutant run, PGO benchmarks.

## BUNDLE-Q (2026-11-10): `t1q` bit 0 (tier-1 `==` / `~=` of objects) SHIPPED (process 1), bit 1 (float `%`) built, OFF

Mechanisms, tests and mutants: docs/JIT_TRACE_REF.md "BUNDLE-Q"; what was not built and why bit 1 is off: docs/JIT_PARKED.md C59. Commits `3bee69e9` (mechanism, both bits, default 3), `1321f8bc` (default 1, jit-diff set `bq`, `SPIN_MS` in `bench_session.py`); base `4906d177`, plain `cargo build --release -p rn-cli --features jit`. The knob is `RUSTYNAIL_JIT_TRACE_LIMITS=t1q=<mask>` (1 = `EQ` of objects, 2 = float `MOD`/`MODK`; `t1q=0` is the base behaviour; a named value also applies with `RUSTYNAIL_JIT_TRACE=off`).

**Method.** Binaries at equal-length paths (`bin/a/rn-cli` = base, `bin/q/rn-cli` = new), `RUSTYNAIL_SEED=0`, equal-length environment (padded), identical script paths (this worktree's `bench/`), CPUs 3-7 (one program per core), a 300 ms spin of the core before every timed run (`SPIN_MS=300`), columns rotated, min of 5 runs, cycles:u / instructions:u (`scripts/bench_session.py`, 223 programs in 9 sets), outputs equal `.expected` (0 mismatches). Columns of the shipped pass: `base`, `based` (base again: the noise column), `new` (shipped default, `t1q=1`), `q0` (new binary, `t1q=0`: separates the binary layout from the mechanism), `q3` (new binary, both bits). Loser rechecks: 15 serial runs on CPU 4 (`rr1`). An earlier pass (default 3) had the extra columns `q1`, `q2`.

### Geomeans, ratio to `base` (cycles / instructions)

| set | n | noise (`based`) | **new (t1q=1)** | q0 (new binary, off) | q3 (both bits) |
|---|---|---|---|---|---|
| ALL | 223 | 1.0004 / 1.0000 | **0.9971 / 0.9984** | 0.9983 / 1.0001 | 0.9965 / 0.9987 |
| GATE apps+corpus+real | 28 | 1.0002 / 1.0001 | **0.9864 / 0.9898** | 0.9991 / 1.0006 | 0.9880 / 0.9919 |
| apps | 5 | 1.0019 / 1.0000 | 0.9829 / 0.9789 | 1.0042 / 1.0003 | 0.9787 / 0.9797 |
| awfy | 14 | 1.0003 / 1.0000 | 1.0008 / 1.0018 | 0.9995 / 1.0000 | 1.0026 / 1.0018 |
| corpus | 18 | 0.9996 / 1.0001 | 0.9837 / 0.9901 | 0.9967 / 1.0008 | 0.9869 / 0.9931 |
| corpus2/dev | 19 | 1.0025 / 1.0000 | 0.9986 / 1.0000 | 1.0006 / 1.0000 | 0.9986 / 1.0002 |
| corpus2/heldout | 13 | 0.9964 / 1.0000 | 0.9824 / 0.9952 | 0.9924 / 1.0001 | 0.9846 / 0.9955 |
| jit-trace | 29 | 0.9984 / 1.0000 | 1.0001 / 1.0000 | 0.9990 / 1.0000 | 1.0003 / 1.0000 |
| lib | 95 | 1.0002 / 1.0000 | 0.9998 / 0.9997 | 0.9985 / 1.0000 | 0.9986 / 0.9998 |
| programs | 25 | 1.0041 / 1.0000 | 0.9997 / 1.0000 | 0.9964 / 1.0000 | 0.9952 / 0.9999 |
| real | 5 | 1.0005 / 1.0000 | 1.0001 / 1.0000 | 1.0023 / 1.0000 | 1.0017 / 1.0000 |

Gate: gate-set cycles geomean 0.9864 (<= 0.995: met), all-223 0.9971. The first pass (default 3, columns `q1` = `t1q=1`, `q2` = `t1q=2`): gate set `new` 0.9876 / 0.9917, `q1` 0.9846 / 0.9897, `q2` 0.9985 / 1.0024, all-223 0.9970 / 0.9957 (`q1`) / 0.9983 (`q2`).

### Targets (5 runs, min; 15-run serial cycles in the last column where rechecked)

| program | base Mcyc | base Minstr | new Mcyc | new Minstr | cycles | instr | noise col | 15 runs | `t1q=3` cyc / instr |
|---|---|---|---|---|---|---|---|---|---|
| corpus/astar_jumper | 1746.0 | 5130.7 | 1674.9 | 4997.9 | **0.959** | 0.974 | 1.008 | 0.955 (instr 0.962) | 1.030 / 1.026 |
| apps/lru_cache | 174.3 | 525.6 | 161.1 | 475.8 | **0.925** | 0.905 | 1.005 | 0.919 | 0.931 / 0.908 |
| corpus2/heldout/scheme_interp | 1634.5 | 7173.6 | 1545.6 | 6985.7 | **0.946** | 0.974 | 0.998 | 0.937 | 0.941 / 0.974 |
| corpus/containers_lru_heap | 1656.9 | 7226.2 | 1536.9 | 6901.9 | **0.928** | 0.955 | 1.002 | 0.931 | 0.931 / 0.955 |
| corpus/testrunner_luaunit | 1666.9 | 6346.6 | 1584.9 | 5959.1 | **0.951** | 0.939 | 1.001 | 0.954 | 0.950 / 0.939 |
| corpus/class_middleclass | 674.6 | 3267.4 | 654.2 | 3216.4 | **0.970** | 0.984 | 1.001 | - | 0.968 / 0.984 |
| corpus/inspect_lua | 1613.4 | 6369.9 | 1576.9 | 6248.4 | **0.977** | 0.981 | 0.997 | - | 0.968 / 0.981 |
| corpus2/heldout/prototype_objects | 1725.7 | 4235.4 | 1524.5 | 4113.9 | 0.883 | 0.971 | 0.955 | 0.959 (base again 0.987) | 0.906 / 0.973 |
| apps/events | 164.0 | 702.7 | 164.4 | 697.9 | 1.002 | 0.993 | 1.005 | - | 0.982 / 0.994 |
| corpus/sha2_base64 | 425.3 | 2121.6 | 426.1 | 2121.6 | 1.002 | 1.000 | 0.994 | - | 1.003 / 1.000 |
| awfy/deltablue | 102.4 | 327.5 | 103.0 | 334.8 | 1.006 | 1.022 | 0.997 | 1.015 | 1.015 / 1.022 |
| awfy/json | 141.0 | 494.9 | 143.7 | 494.1 | 1.019 | 0.998 | 1.004 | 1.011 | 1.013 / 0.998 |
| awfy/richards | 56.3 | 180.1 | 56.0 | 180.1 | 0.994 | 1.000 | 1.000 | 0.997 | 1.000 / 1.000 |

`corpus/astar_jumper` with the pieces apart (first pass, cycles / instructions against base): `t1q=1` 0.953 / 0.971, `t1q=2` **1.062 / 1.062**, `t1q=3` 1.013 / 1.020. The census's `lru_cache` 0.956 / `scheme_interp` 0.947 / `containers` 0.937 / `astar` 0.964 / `testrunner` 0.971 / `class_middleclass` 0.972 / `inspect` 0.976 are reproduced or beaten (lru_cache 0.925, testrunner 0.951, containers 0.928). `sha2_base64` does not move: its `l % 256` is not on a float path in this build (instructions 2121.6 M in every column).

### Gate misses and what they are

* **Bit 1 (float `%`) fails the gate on its own target: astar +6 % cycles and instructions** (`t1q=2` alone; the cheap helper is cheaper than the exit per operation, 168k `MODK` exits only, so the extra 280 M instructions are not the helper: the loop that the exit used to send through the interpreter now stays in tier 1 and the program's trace/entry pattern changes; main's tier-1 compiled share rises, the interpreter's falls, the total rises; root cause not found). It is built, tested (6 mutants share `bq_t1q_programs`) and **OFF** (`T1Q_PROCESS` = 1). Nothing else gains from it (all-223 `q2` against `q0`: 1.0016 / 1.0002 cycles / instructions).
* **awfy/deltablue 1.006 cycles (15 runs 1.015) / 1.022 instructions**: the formation effect the code comment in `jit_cmp` warned about: with the answered `==` the loops of deltablue stay in tier 1 where they used to exit, and the trace set changes (the census measured +1.9 % cycles / +1.6 % instructions; here +1.5 % at 15 runs, +2.2 % instructions). Inside the 3 % cycle gate, over the 1 % instruction gate and explained by the formation shift. `awfy/json` 1.019 cycles at 5 runs, 1.011 at 15, instructions 0.998 (noise column 1.004): within the gate, formation or layout. `awfy/richards` unchanged (the program with equal instructions in every column).
* **Layout-lottery cases (equal instructions, `q0` column reproduces them, i.e. the binary, not the mechanism):** `lib/find_pat` 1.068 (15 runs 1.067), `lib/it_pairs` 1.051 (1.053), `lib/load_small` 1.065 (1.061), `lib/sel_n` 1.040 at 5 runs but 0.978 at 15 (noise), `lib/it_next` 1.037 / 1.015, `lib/str_eq_long` 1.028 / 0.992, `lib/fmt_s` 1.037 / 1.002, `jit-trace/sink_vec` 1.025 / 0.997, `programs/heapsort` 1.074 / 0.999 (the noise column 1.117 at 5 runs), `lib/tonumber_num` 0.943 (noise column 0.921 at 5 runs, 1.080 at 15). `find_pat`, `it_pairs` and `load_small` are over the 3 % rule in the letter at equal instructions (1.000) and are **not chased**; none is a changed trace formation (their trace sets are identical, instructions equal to the digit). The formation-caused ones are deltablue and awfy/json above; richards is not affected.
* **The eight Bundle-P lib programs (+3..+15 % cycles at equal instructions after Bundle P), re-measured here against the Bundle-P binary (`base`):** `lib/m_random_f` **0.872** (15 runs 0.872; `q0` 0.871: the binary of this build gives the +14 % back), `lib/gsub_str` 0.993 (15 runs 0.985), `lib/gsub_tbl` 1.006 (0.998), `corpus/parser_combinator` 0.967 (0.969; instructions 0.997 with the new `==`), `corpus/markdown_conv` 1.001, `corpus/template_etlua` 0.990 (0.988), `apps/markdown` 0.993 (0.997), `programs/json` 0.986 (1.007). None persists as a loss in this build; they move with the binary (layout), which is the Bundle-P conclusion.

### Correctness runs

See docs/JIT_TESTING.md "BUNDLE-Q". Summary: all listed suites pass; `bq_eq` / `bq_mod` equal `ref/55/build/lua` byte for byte in 16 modes (interpreter, tier 1 alone with `t1q` 0 / 1 / 2 / 3 and default, `on`, `on` threshold 2, traces always / on, stack moves, GC stress 1 and 2); jit-diff set `bq` seeds 1..100, modes `off tr trdef trstorm tremu trstack` (default and `t1q=3`), and against `ref`: 0 diffs.


## BUNDLE-S (2026-11-10): `t1q` bits 2 / 3 (tier-1 Lua `__index` / `__newindex` frames) and `amt` (array accesses of tables with a metatable) SHIPPED (process `t1q` 13, `amt` 1)

Mechanisms, tests and mutants: docs/JIT_TRACE_REF.md "BUNDLE-S"; what was not built: docs/JIT_PARKED.md C60. Commits (this branch, base `97021092`): S2 `90c82aa4`, S1 mechanism `10184619`, S1 flagged resume entries + `RETURN` with upvalues + defaults + jit-diff set `bs` `195790c8`, then the lockstep fix / test additions / docs (see the final report). Plain `cargo build --release -p rn-cli --features jit`. Knobs: `RUSTYNAIL_JIT_TRACE_LIMITS=t1q=<mask>` (1 `EQ` of objects, 2 float `%` (OFF), **4 Lua `__index` frames, 8 Lua `__newindex` frames**) and `amt=<0|1>`.

**Diagnosis (census deep tables + fresh `RUSTYNAIL_JIT_VERBOSE=1/3` runs).** Tier-1 exits at a Lua `__index` function (the stay behind each: the handler *and* the rest of the caller in the interpreter): `msgpack_pure` 875k `GETTABLE` exits (the `unpackers[val]` dispatch table: its `__index` builds and returns a closure), `template_etlua` 319k `GETTABUP` (sandbox environment with an `__index` function), `real_dis_ppc` 38k `GETTABLE` x 30k cycles per stay, `class_middleclass` `SELF` (472 exits x 474k cycles: `isSubclassOf` through `__index` functions), `lru_cache` proxy; trace aborts `MetaTable` 64 / 51 / 24 (`bignum_arith`, `penlight_mix`, `testrunner_luaunit`), **all of them `GETTABLE` on an array of a table with a metatable** (`table_for`). The recorder already inlines `__index` / `__newindex` functions (`mm_index_fn`, `mm_index_int`, `mm_newindex_fn`, `Metamethod` aborts 39 + 46 in two programs), so S1 was a tier-1 piece (mirroring `Cold::ArithMm` / `jit_mm_pop`) and S2 a recorder piece.

**Method.** Binaries at equal-length paths (`bin/a/rn-cli` = base, `bin/x/rn-cli` = new), `RUSTYNAIL_SEED=0`, equal-length environment (padded), identical script paths (this worktree's `bench/`), CPUs 3-7 (one program per core, siblings idle), a 300 ms spin of the core before every timed run (`SPIN_MS=300`), columns rotated, min of 5 runs, cycles:u / instructions:u (`scripts/bench_session.py`, 223 programs in 9 sets), plus a second copy of the base binary (`based`: noise column) and, for attribution, the new binary with both knobs off (`off`: `t1q=1,amt=0`; it carries the binary-layout effect of the new code), with only S1 (`s1only` = `amt=0`) and only S2 (`amtonly` = `t1q=1`). Outliers re-measured serially on CPU 4, 15 runs (`rr3`).

### Geomeans, ratio to `base` (cycles / instructions)

| set | n | noise (`based`) | **new (defaults)** | off (new binary, knobs off) | s1only | amtonly |
|---|---|---|---|---|---|---|
| ALL | 223 | 0.9985 / 1.0000 | **0.9934 / 0.9973** | 0.9979 / 1.0004 | 0.9938 / 0.9987 | 0.9973 / 0.9991 |
| GATE apps+corpus+real | 28 | 0.9994 / 1.0001 | **0.9742 / 0.9864** | 0.9981 / 1.0013 | 0.9749 / 0.9867 | 0.9990 / 1.0008 |
| apps | 5 | | 0.9827 / 1.0002 | 0.9971 / 1.0017 | 0.9864 / 1.0004 | 0.9973 / 1.0016 |
| awfy | 14 | | 0.9999 / 1.0015 | 1.0002 / 1.0005 | 0.9989 / 1.0015 | 0.9988 / 1.0005 |
| corpus | 18 | | 0.9675 / 0.9787 | 0.9995 / 1.0014 | 0.9677 / 0.9790 | 1.0003 / 1.0007 |
| corpus2/dev | 19 | | 0.9911 / 0.9873 | 0.9930 / 1.0015 | 0.9981 / 1.0022 | 0.9871 / 0.9866 |
| corpus2/heldout | 13 | | 1.0044 / 0.9985 | 1.0054 / 1.0009 | 1.0044 / 0.9985 | 1.0030 / 1.0009 |
| jit-trace | 29 | | 0.9991 / 1.0001 | 1.0007 / 1.0000 | 1.0000 / 1.0001 | 1.0006 / 1.0000 |
| lib | 95 | | 0.9938 / 1.0001 | 0.9968 / 1.0000 | 0.9945 / 1.0002 | 0.9967 / 1.0000 |
| programs | 25 | | 0.9991 / 1.0005 | 0.9974 / 1.0003 | 0.9942 / 1.0005 | 0.9983 / 1.0003 |
| real | 5 | | 0.9903 / 1.0008 | 0.9943 / 1.0002 | 0.9896 / 1.0008 | 0.9962 / 1.0002 |

Gate: gate-set cycles 0.9742 (<= 0.995: met), all-223 0.9934. The pieces apart, second pass (flagged resume entries, before the general `RETURN` with open upvalues in handler frames; base `97021092`): `t1q=5` (`__index` only) gate set 0.9867 / 0.9887, `t1q=9` (`__newindex` only) 1.0001 / 1.0006, `t1q=13` 0.9849 / 0.9888, with `amt=1` added 0.9861 / 0.9886; `amt=1` alone 1.0058 / 1.0007 (the binary's layout: `bignum_arith` 0.853 and no instruction regression). The first pass (public resume entries) had gate 0.9775 for `t1q=13` but `expr_lang` +5 % and ~10 programs moved by formation effects. The table above is the final tree (`RETURN` with open upvalues in handler frames: msgpack 0.957 -> 0.846).

### Targets (5 runs min; 15-run serial cycles in the last column)

| program | base Mcyc | base Minstr | new Mcyc | new Minstr | cycles | instr | noise col | off (cyc / instr) | 15 runs new / based / off cycles (new instr) |
|---|---|---|---|---|---|---|---|---|---|
| corpus/msgpack_pure | 2438.7 | 8761.9 | 2062.8 | 7996.1 | **0.846** | 0.913 | 0.999 | 0.984 / 1.002 | 0.844 / 0.997 / 0.978 (0.913) |
| corpus/template_etlua | 1832.5 | 7634.9 | 1620.4 | 6947.0 | **0.884** | 0.910 | 1.003 | 0.998 / 1.001 | 0.879 / 1.001 / 0.995 (0.910) |
| corpus/class_middleclass | 656.5 | 3216.4 | 485.5 | 2552.2 | **0.739** | 0.793 | 1.002 | 1.001 / 1.000 | 0.737 / 1.000 / 1.003 (0.793) |
| corpus2/dev/bignum_arith | 995.7 | 3979.8 | 834.3 | 3003.7 | **0.838** | 0.755 | 1.003 | 0.994 / 1.003 | 0.838 / 1.003 / 0.994 (0.755) |
| corpus2/heldout/real_dis_ppc | 2313.3 | 7769.1 | 2242.6 | 7473.0 | **0.969** | 0.962 | 1.002 | 1.001 / 1.000 | 0.970 / 1.001 / 1.003 (0.962) |
| corpus2/dev/real_dis_x86 | 1653.8 | 5441.8 | 1659.6 | 5446.2 | 1.004 | 1.001 | 0.999 | 1.004 / 1.000 | - |
| corpus2/dev/real_dis_arm | 2555.8 | 7887.8 | 2538.2 | 7895.5 | 0.993 | 1.001 | 0.994 | 0.995 / 1.001 | - |
| corpus2/heldout/real_dis_mips | 2122.3 | 7150.0 | 2115.0 | 7154.1 | 0.997 | 1.001 | 0.993 | 0.999 / 1.000 | - |
| apps/lru_cache | 162.6 | 475.8 | 159.4 | 472.5 | **0.980** | 0.993 | 0.997 | 1.003 / 1.005 | 0.980 / 1.000 / 1.010 (0.993) |
| corpus/penlight_mix | 2303.3 | 10772.7 | 2263.8 | 10686.4 | 0.983 | 0.992 | 0.985 | 0.990 / 1.000 | 0.995 / 0.999 / 0.998 (0.992) |
| corpus/testrunner_luaunit | 1598.7 | 5959.1 | 1607.7 | 5974.0 | 1.006 | 1.003 | 1.004 | 1.004 / 1.001 | 1.007 / 1.001 / 1.008 (1.003) |
| corpus2/heldout/prototype_objects | 1523.6 | 4113.9 | 1619.3 | 4123.6 | 1.063 | 1.002 | 1.018 | 1.063 / 1.002 | 0.996 / 0.990 / 0.992 (1.002) |
| corpus/astar_jumper | 1685.5 | 4962.3 | 1666.0 | 5024.5 | 0.988 | 1.013 | 0.986 | 1.003 / 1.008 | 1.000 / 1.007 / 1.006 (1.006) |
| corpus/containers_lru_heap | 1538.5 | 6901.4 | 1574.4 | 6950.9 | 1.023 | 1.007 | 1.002 | 1.010 / 1.004 | 1.020 / 1.000 / 1.009 (1.007) |
| awfy/havlak | 145.2 | 517.3 | 148.3 | 520.9 | 1.021 | 1.007 | 1.048 | 1.009 / 1.004 | 1.007 / 0.995 / 1.002 (1.007) |
| corpus2/dev/oop_mixins | 1656.3 | 6218.5 | 1696.4 | 6277.0 | 1.024 | 1.009 | 1.010 | 0.986 / 1.009 | 1.002 / 0.996 / 1.000 (1.009) |
| lib/sel_loop | 229.8 | 2083.3 | 254.4 | 2083.3 | 1.107 | 1.000 | 1.033 | 1.014 / 1.000 | 1.129 / 1.005 / 1.005 (1.000) |

The census estimates ([I]: classic_vector / bignum 0.85-0.9, others 0.97-0.99) are met or beaten: `bignum_arith` 0.838 (S2: 64 aborts gone, instructions 0.755), `class_middleclass` 0.737, `msgpack_pure` 0.844, `template_etlua` 0.879; `real_dis_ppc` 0.970; `lru_cache` 0.980; `penlight_mix` (0.983 at 5 runs, 0.995 at 15, instructions 0.992) / `testrunner_luaunit` (1.006) / the other `real_dis_*` (0.993 to 1.004) do not regress; their MetaTable aborts are gone but the aborted loops were not where their time goes. `classic_vector` was not a target of this bundle (its cost is the warm-up, C58).

### Gate misses and what they are

* **Nothing exceeds +3 % cycles at 15 runs except `lib/sel_loop` (+12.9 %, instructions 1.000).** The `off` column (same binary, both knobs off) is 1.005, `s1only` is 1.105, `amtonly` 1.014, and `sel_count` (0.85) / `sel_n` (0.91) / `programs/heapsort` (0.87) move the other way at equal instructions: the new out-of-line stubs of the table templates change where the hot loops of *other* prototypes sit in the code buffer. Layout lottery, reported, not chased (program has no metatable; the S1 helper is never called in it).
* **`corpus2/heldout/prototype_objects` 1.063 at 5 runs: 0.996 at 15** (noise column 0.990): a layout/frequency artefact of the 5-run pass. `programs/string_ops` 1.037 (15 runs 1.008), `corpus2/dev/event_promise` 1.037 (0.989), `jit-trace/alloc_ctor` 1.027 (1.000), `corpus2/dev/oop_mixins` 1.024 / 1.009 instructions (1.002 at 15 runs; the `off` column has the same 1.009 instructions: the binary, not the knobs), `awfy/havlak` 1.021 (1.007).
* **Residual real costs (15 runs):** `corpus/containers_lru_heap` +2.0 % cycles / +0.7 % instructions (`off` 1.009 / 1.004: half of it is the binary), `corpus2/dev/regex_nfa` +2.3 % (equal instructions, `off` 0.986), `lib/gsub_fn` +2.4 % (`off` +3.3 %: binary), `lib/m_type` +2.5 % (`off` 2.5 %: binary), `programs/gc_churn` +2.8 % (5 runs 1.024, instructions 1.002; `off` 0.999; the one case where the knobs themselves seem to cost; not chased), `corpus/testrunner_luaunit` +0.7 % / +0.3 % instructions, `corpus/astar_jumper` instructions +1.3 % at 5 runs (+0.6 % at 15; `off` 1.008 / 1.006: astar's trace set changes with anything that moves the environment or the binary).
* **Gate over 1 % instructions:** only `astar_jumper` 5-run 1.013 (explained: the `off` column has 1.008 and 15 runs give 1.006; its trace formation is the known sensitive one, docs/PERF.md "BUNDLE-Q" `t1q` bit 1).
* **The S1 formation trap (found and fixed, kept as a record):** the first S1 build registered the continuation after every `GET*` / `SET*` as an ordinary resume entry; `apps/expr_lang` (no `__index` function at all) went to 1.095 cycles / 1.058 instructions with `t1q=5` (and `parser_combinator` / `prototype_objects` moved by +5 % / +2 %). The entries are private to the metamethod return now (docs/JIT_TRACE_REF.md "BUNDLE-S").
* **Not measured:** PGO builds, the 5.4 build, ASan / fuzz.

### Correctness runs

See docs/JIT_TESTING.md "BUNDLE-S". Summary: `bs_amt` / `bs_ixmm` equal `ref/55/build/lua` byte for byte in 7 modes x `t1q` 0 / 4 / 8 / 12 / 15 (35 runs each); `tr_core` 176 tests (`bs_amt_programs`, `bs_ixmm_programs` incl. the oracle identity jobs, `r1_rearm_programs` pinned to `t1q=1`) and all the listed suites pass; jit-diff seeds 1..100: set `bs` (`off tr trdef trstorm tremu trstack`, and `off tr ref`, and with the knobs off) 0 diffs, default set `tr` (`off tr trdef trstorm tremu`) 0 diffs, `oop`, `bq`, `bd` 0 diffs; mutants 11 of 11 caught.

## BUNDLE-T (2026-11-10): fused coroutine resume (`coro_fused`) SHIPPED; the 10 % gate on `lib/co_yield` and `programs/coroutines` is MISSED

Mechanism and tests: docs/JIT_TRACE_REF.md "BUNDLE-T", docs/JIT_TESTING.md "BUNDLE-T"; profile and what was not built: docs/JIT_PARKED.md C9. Commits (branch `<worktree>`, base `0a4f3a33`): T0 `95faa41f` (the `vbytes_*` mutant calls of `rt::recompile` go through `vbytes_mutant`, so `cargo check -p rn-jit --features jit` without `trace` compiles again), T1 `eac8a8f5` (fused resume, tests, mutants), jit-diff set `bt`, the `jit_call` revert `4b6ed8bd`, docs. Plain `cargo build --release -p rn-cli --features jit`. Knob: cargo feature `coro_nofuse` (`coro::CORO_FUSED = false`; rn-cli forwards it) for the A/B column; there is no run-time knob.

**What it does.** `coroutine.wrap` closures and `coroutine.resume` move the arguments, run `lua_resume` and move the results back in one function (`State::resume_fused`) when the coroutine is suspended in a plain `yield` called from a Lua frame, nobody has a hook and the stacks have room; everything else takes the old path unchanged. It removes the layering `auxwrap -> auxresume -> check_stack_api / xmove -> resume -> resume_body -> post_call -> unroll -> finish_op -> execute` (about 70 instructions of 825 per resume + yield pair), not the interpreter's work for the coroutine body (about 40 % of the instructions) nor the native-call framing of the two calls.

**Method.** Binaries at equal-length paths (`ta` = base, `td` = a copy of the base: noise column, `tj` = new, `tn` = new with `coro_nofuse`), `RUSTYNAIL_SEED=0`, identical script paths (this worktree's `bench/`), CPUs 3-7 (one program per core), a 300 ms spin of the core before every timed run, columns rotated, min of 5 runs, cycles:u / instructions:u (`scripts/bench_session.py`, 223 programs in 9 sets; every output equals the program's `.expected` where one exists, 193 of 223). Outliers re-run serially on CPU 4, 15 runs, interleaved, with the base-again column.

### Geomeans, ratio to `base` (cycles / instructions)

| set | n | noise (`based`) | **new** | `coro_nofuse` build |
|---|---|---|---|---|
| ALL | 223 | 1.0002 / 1.0000 | **1.0024 / 0.9988** | 1.0004 / 1.0000 |
| GATE apps+corpus+real | 28 | 1.0022 / 0.9997 | **1.0062 / 0.9989** | 1.0059 / 0.9998 |
| apps | 5 | 1.0015 / 1.0000 | 1.0084 / 1.0000 | 1.0184 / 1.0000 |
| awfy | 14 | 1.0028 / 1.0000 | 1.0133 / 1.0000 | 1.0043 / 1.0000 |
| corpus | 18 | 1.0024 / 0.9995 | 1.0063 / 0.9982 | 1.0022 / 0.9997 |
| corpus2/dev | 19 | 0.9970 / 1.0000 | 0.9962 / 0.9974 | 1.0004 / 1.0000 |
| corpus2/heldout | 13 | 0.9942 / 1.0000 | 1.0081 / 1.0000 | 0.9914 / 1.0000 |
| jit-trace | 29 | 1.0010 / 1.0000 | 0.9983 / 1.0000 | 0.9995 / 1.0000 |
| lib | 95 | 0.9995 / 1.0000 | 1.0009 / 0.9990 | 0.9989 / 1.0000 |
| programs | 25 | 1.0038 / 1.0000 | 1.0044 / 0.9964 | 1.0037 / 1.0000 |
| real | 5 | 1.0024 / 1.0000 | 1.0037 / 1.0000 | 1.0068 / 1.0000 |

The instruction counts of the `coro_nofuse` build equal the base's to the digit on every program (the only code that changed is the fused path), so the cycle differences in the other columns at equal instructions are code layout (below). The five coroutine programs together: cycles 0.962, instructions 0.948.

### Coroutine programs (5 runs min; Mcyc / Minstr)

| program | base | new | cycles | instr | `coro_nofuse` cycles | noise col |
|---|---|---|---|---|---|---|
| lib/co_yield | 251.4 / 1228.0 | 243.4 / 1121.0 | **0.968** | 0.913 | 1.010 | 0.990 |
| programs/coroutines | 1935.9 / 9374.6 | 1853.7 / 8575.9 | **0.958** | 0.915 | 0.996 | 1.010 |
| corpus/coro_scheduler | 1591.5 / 7104.8 | 1553.3 / 6894.6 | 0.976 | 0.970 | 0.991 | 0.996 |
| corpus2/dev/coro_generators | 1373.9 / 4340.2 | 1248.1 / 4127.0 | **0.908** | 0.951 | 1.013 | 1.001 |
| lib/co_wrap | 245.8 / 1123.4 | 246.7 / 1116.9 | 1.004 | 0.994 | 1.042 | 1.007 |
| corpus2/heldout/stream_closures | 896.9 / 3891.6 | 894.3 / 3891.6 | 0.997 | 1.000 | 0.990 | 0.994 |
| corpus/serpent_ser | 2024.1 / 7017.0 | 2033.7 / 7016.8 | 1.005 | 1.000 | 1.004 | 1.002 |

Micro programs (`~/.cache/bunT/s`, 1.49 M resume + yield pairs, min of 5 on CPU 5): body `n = n + 1; yield(n)` with `yield` in an upvalue and the result consumed (`y1`): 216.5 -> 197.4 Mcyc (**0.912**), 1103 -> 996 Minstr (742 -> 670 per pair); body `while true do yield() end` (`y3`): 172.5 -> 147.8 Mcyc (**0.857**), 968 -> 866 Minstr (650 -> 583 per pair). `lib/co_yield` (the body looks `coroutine.yield` up in `_ENV` twice per switch) gets only 0.968: see the profile.

### Gate

* **`lib/co_yield` and `programs/coroutines` >= 10 % fewer cycles: NOT MET** (0.968 and 0.958; the census estimate [I] was 5-8 % after the haircut). `coro_generators` 0.908 and `coro_scheduler` 0.976 are the others that gain; `co_wrap` (thread creation dominates: `new_thread`, `free_obj`, the first `CallInfo` allocations) and the iterator programs that do not switch in a hot loop do not.
* **Instructions:** no program is worse than 1.0002 (`havlak`); the five coroutine programs are 0.913-0.994; ALL 0.9988.
* **Cycles, no program > 3 % worse: NOT MET at 5 runs (13 programs) and 7 programs at 15 runs, all at *equal instructions* and with a base-again column of 1.00:** `awfy/storage` +11.7 % (15 runs; base-again 1.013), `corpus2/heldout/bigint_rsa` +8.0 % (1.002), `lib/tinsert_idx` +8.7 % (1.002), `lib/tinsert_append` +8.4 % (1.001), `corpus2/heldout/sudoku_solver` +6.4 % (1.001), `corpus/parser_combinator` +5.0 % (0.998), `corpus2/dev/bignum_arith` +3.4 % (0.998). The others of the 5-run list fell inside 3 % at 15 runs: `programs/heapsort` +0.1 %, `table_hash` +0.6 % (base-again 0.995), `havlak` +1.2 %, `lib/sel_count` +1.3 %, `sel_n` +0.8 %, `tremove_pop` +2.8 %, `fasta` +2.1 %, `awfy/json` +2.0 %, `programs/json` +1.6 %. The `coro_nofuse` build (the same tree minus the fused path) moves other programs (`heapsort` +5.7 % at 5 runs, `table_hash` +4.6 %) and not these: layout lottery (the interpreter loop sits at `0xb6750` instead of `0xb6010`, the helpers after the added code moved), reported, not chased. Other programs move the other way by 3-5 % at equal instructions (`prototype_objects` 0.950, `fmt_s` 0.954, `load_small` 0.968, `alloc_ctor` 0.970).

### Profile of `lib/co_yield` (base binary; the reason the gate is out of reach with this mechanism)

Per resume + yield pair: 826 instructions, 171 cycles (IPC 4.8, branch-misses 36 K in the whole run, L1D misses 30 K); PUC 5.5 890 instructions / 332 cycles, LuaJIT 401 / 69. `perf record -e cycles:u`: `run_mode` 53.5 % (the coroutine body: entry, seven bytecodes, the `yield` call), `auxwrap` 11.0, `State::resume` 10.3, `auxresume` 5.5, `jit_call` 4.4, `post_call` 4.1, tier-1 code 3, `unroll` 2.8, `finish_op` 1.7, `co_yield` 1.4, `execute` 0.9. Instruction shares from LBR block counts (`perf record -b`, `~/.cache/bunT/s/lbr.py`, text of `rn-cli` only): `run_mode` 40, `jit_call` 18, `auxresume` 9, `auxwrap` 8, `resume` 7, `post_call` 5, `unroll` 4, `execute` 3, `finish_op` 2.6, `co_yield` 1.2. Findings:

* The switch glue (everything but `run_mode` and `jit_call`) was about 35 % of the instructions; the fused path took 70 of 826 (the rest of the glue is the protected run, the executor entry, the native-call frames of `auxwrap` and `co_yield`, the result moves).
* **Cycles do not follow instructions when the body looks `coroutine.yield` up in `_ENV`.** `y1` (same switch path, `yield` an upvalue) gains 8.8 % cycles for 9.7 % instructions, `co_yield` gains 3.2 % for 8.7 %: with the shorter switch path the `GETTABUP _ENV "coroutine"` -> `GETFIELD "yield"` -> `CALL` dependency chain (about seven dependent loads each, about 85 instructions; their cost grew from 26 to 31 cycles per switch) is exposed. About 20 % of the cycles of `co_yield` sit on the `movzbl` of the callee slot's tag in the `OP_CALL` handler of the coroutine body, which waits for the second lookup. The chain is the interpreter's own; it would need a cache for `GETTABUP` / `GETFIELD` on constant keys (a different bundle), or entering tier 1 on resume, which `rt::enter` (a reference-count clone, statistics, the thunk round trip, the stay monitor) probably makes a loss for a body of seven bytecodes (not measured).
* A synthetic `n=n+1` and `acc=acc+co()` consumer add about 30 cycles per pair on top of their instructions (`y1` vs the sum of its parts): the argument and result values travel through four store-to-load forwards (59 forwards per pair in all, `ls_stlf`), a serial chain across the switch that the out-of-order window cannot overlap.

### What was tried and not kept

* A result-move specialisation of `jit_call` for native callees by wanted-result count (as `jit_ccall` has): `co_yield` 0.969, but `lib/m_fmod` +3.1 %, `m_ult` +2.8 %, `tonumber_bad` / `tonumber_base` / `utf8_len` +1.6..1.8 % **instructions** (four inlined copies of `native_fast_with` made the helper's prologue and spills heavier for every native called from tier 1); reverted (`4b6ed8bd`), the coroutine gains are the same without it.
* `stitchmin=1` / trace-level stitching at `yield` / `resume`: not repeated (docs/JIT_PARKED.md C9: 0.112 -> 0.094 s on a pure resume loop and nothing on the others in the earlier investigation); the recorder still aborts with `CFunc` at these calls (4 aborts per program, 0.15 Mtsc).
* Not built: a flat resume entry that re-enters the executor at the coroutine's savedpc without `run` / `run_mode`'s frame setup (touches the interpreter loop, the layout lottery of every program); stackless switching inside the interpreter loop; allocation of `CallInfo` nodes with the thread (`co_wrap`); the start path (first resume) of the fused function.
* Not measured: PGO builds, the 5.4 build beyond `cargo check`, ASan, fuzz, the full 581-mutant run (the 7 new ones: 7 of 7 caught).


## BUNDLE-R (2026-11-10): more library calls on trace: `libcall` bits 3..9 SHIPPED (process default 7 -> 1023)

Mechanisms, tests and mutants: docs/JIT_TRACE_REF.md "BUNDLE-R"; what was not built, the census and the retest notes: docs/JIT_PARKED.md C61; tests: docs/JIT_TESTING.md "BUNDLE-R". Commits on the branch (base `3a24e470`): R4 `dd61e63e`, unpack `ca6280a5`, tonumber `89e6ca0c`, move `4e6e2b9d`, concat keys `e2b4d939`, lean find + fixes `35ccee5b`, defaults + docs (last commit).

**Method.** One binary (`bin/a/rn-cli`, release `--features jit`), every column a `RUSTYNAIL_JIT_TRACE_LIMITS=libcall=N` value (`base` = 7 = the BUNDLE-H state, `dup` = 7 again), plus `main` = a binary built from `git archive 3a24e470` (`bin/m/rn-cli`, equal-length path); equal-length environments (`bench_session.py` pads), identical script paths, `RUSTYNAIL_SEED=0`, all 223 programs, CPUs 3-7 (one program per core, siblings 11-15 idle), 300 ms spin before each timed run, columns rotated, min of 5 runs; flagged programs re-run serially at 15 runs on CPU 4. Outputs equal `.expected` in every program and column (0 mismatches). Raw: `~/.cache/bunR/b2.json`, `c2.json` (not committed).

### Geomeans, ratio to `base` (cycles / instructions)

| column | knob | ALL (223) | GATE apps+corpus+real (28) | apps | awfy | corpus | dev | heldout | jit-trace | lib | programs | real |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| main | binary of `3a24e470`, `libcall=7` | 0.9996 / 1.0000 | 1.0035 / 1.0000 | 1.0047 / 1.0001 | 1.0042 / 1.0000 | 1.0053 / 1.0000 | 1.0009 / 1.0000 | 0.9992 / 1.0000 | 0.9996 / 1.0000 | 0.9994 / 1.0001 | 0.9927 / 1.0000 | 0.9957 / 1.0003 |
| dup | new binary, `libcall=7` again (noise) | 0.9995 / 0.9999 | 1.0026 / 0.9996 | 1.0018 / 1.0000 | 1.0007 / 1.0000 | 1.0029 / 0.9993 | 1.0020 / 1.0000 | 0.9876 / 1.0000 | 1.0000 / 1.0000 | 1.0010 / 1.0000 | 0.9937 / 1.0000 | 1.0020 / 1.0000 |
| r4 | `libcall=31` (+ rep, class guard) | 0.9916 / 0.9921 | 0.9995 / 0.9997 | 0.9994 / 1.0000 | 0.9997 / 1.0000 | 0.9983 / 0.9995 | 0.9946 / 0.9979 | 0.9931 / 1.0000 | 1.0028 / 1.0000 | 0.9824 / 0.9822 | 0.9971 / 1.0000 | 1.0035 / 1.0000 |
| r1a | `libcall=39` (+ unpack) | 0.9965 / 0.9963 | 1.0084 / 1.0000 | 1.0014 / 1.0000 | 1.0033 / 1.0000 | 1.0118 / 1.0000 | 1.0034 / 1.0001 | 0.9881 / 1.0000 | 0.9993 / 1.0000 | 0.9911 / 0.9914 | 0.9960 / 1.0000 | 1.0032 / 1.0000 |
| r1b | `libcall=71` (+ move) | 0.9954 / 0.9962 | 1.0024 / 1.0000 | 1.0029 / 1.0000 | 0.9978 / 1.0000 | 1.0005 / 1.0000 | 1.0036 / 1.0000 | 0.9938 / 1.0000 | 1.0012 / 1.0000 | 0.9907 / 0.9911 | 0.9918 / 1.0000 | 1.0089 / 1.0000 |
| r2 | `libcall=135` (+ tonumber) | 0.9995 / 0.9977 | 1.0017 / 1.0000 | 1.0012 / 1.0000 | 1.0009 / 1.0000 | 1.0021 / 1.0001 | 0.9984 / 1.0000 | 0.9936 / 1.0000 | 1.0019 / 1.0000 | 0.9957 / 0.9945 | 1.0121 / 1.0000 | 1.0010 / 1.0000 |
| r5 | `libcall=263` (+ concat keys) | 0.9988 / 0.9977 | 1.0100 / 1.0003 | 0.9958 / 0.9993 | 1.0033 / 1.0000 | 1.0162 / 1.0006 | 1.0048 / 0.9999 | 0.9925 / 0.9984 | 0.9988 / 1.0000 | 0.9968 / 0.9968 | 0.9904 / 0.9922 | 1.0022 / 1.0000 |
| r3 | `libcall=519` (+ lean find) | 1.0009 / 0.9995 | 1.0131 / 0.9998 | 1.0010 / 1.0000 | 1.0004 / 1.0000 | 1.0191 / 0.9997 | 0.9957 / 0.9996 | 0.9883 / 1.0000 | 0.9997 / 1.0000 | 0.9977 / 0.9989 | 1.0116 / 1.0000 | 1.0037 / 1.0000 |
| all | `libcall=1023` (the new default) | 0.9821 / 0.9797 | 1.0096 / 0.9998 | 1.0024 / 0.9993 | 0.9988 / 1.0000 | 1.0141 / 0.9999 | 0.9957 / 0.9975 | 0.9861 / 0.9984 | 1.0006 / 1.0000 | 0.9563 / 0.9556 | 1.0081 / 0.9922 | 1.0011 / 1.0000 |

Gate: every targeted program gains far more than 8 % cycles; the gate-set cycles (1.0096 at 5 runs) are layout lottery at equal instructions (0.9998): the same binary's `dup` column reads 1.0026 and `main` 1.0035, and at 15 runs the worst gate-set programs are 1.002 .. 1.007 (below). The all-223 gain (0.9821 / 0.9797) comes from the `lib` set (0.956 / 0.956); no real program has these calls in its hot loop except `programs/table_hash` (concatenated keys).

### Targets (5 runs min; Mcyc / Minstr; `piece` = the column with only that bit added to 7)

| program | piece | base Mcyc | base Minstr | piece Mcyc | piece Minstr | cycles | instr | all-bits Mcyc | all cycles | 15 runs all/base cycles (dup) |
|---|---|---|---|---|---|---|---|---|---|---|
| lib/rep | r4 `libcall` 31 (bits 3+4) | 215.1 | 1112.9 | 111.6 | 539.3 | **0.519** | 0.485 | 111.6 | 0.519 |  |
| lib/rep_sep | r4  | 228.1 | 1210.5 | 92.0 | 452.4 | **0.403** | 0.374 | 94.5 | 0.414 |  |
| lib/tunpack | r1a bit 5 | 259.7 | 1616.7 | 125.4 | 713.6 | **0.483** | 0.441 | 123.2 | 0.474 |  |
| lib/tmove | r1b bit 6 | 259.2 | 1607.8 | 106.8 | 685.1 | **0.412** | 0.426 | 107.3 | 0.414 |  |
| lib/tonumber_bad | r2 bit 7 | 262.6 | 1754.3 | 157.4 | 1037.9 | **0.599** | 0.592 | 158.7 | 0.604 |  |
| lib/str_idx_concat_key | r5 bit 8 | 271.4 | 1448.6 | 210.1 | 1067.3 | **0.774** | 0.737 | 206.3 | 0.760 |  |
| programs/table_hash | r5 bit 8 | 960.4 | 1426.8 | 675.8 | 1174.4 | **0.704** | 0.823 | 982.6 | 1.023 | 0.957 (1.003) |
| lib/find_plain | r3 bit 9 | 155.3 | 909.7 | 127.1 | 818.7 | **0.818** | 0.900 | 126.9 | 0.817 |  |


The numbers behind each piece (instructions per call, before -> after): `lib/rep` 1358 -> 658 per iteration (the BUNDLE-H trace was 1597: the class-mismatch exits; the doubling helper is most of the rest), `lib/rep_sep` 2181 -> 815, `lib/tunpack` 1216 -> 536 per iteration (the `{1, 2, k, 4, 5}` construction is part of the remainder), `lib/tmove` 623 -> 266 per 8-element move, `lib/tonumber_bad` 506 -> 299, `lib/str_idx_concat_key` 716 -> 527, `lib/find_plain` 309 -> 278 (lean entry only).

### 15-run serial rechecks (CPU 4, min of 15; `all` / `base` cycles | `dup` / `base` | instructions `all` / `base`)

| program | all / base | dup / base | instr |
|---|---|---|---|
| corpus/containers_lru_heap | 1.003 | 0.997 | 1.0099 |
| corpus/csv_ini_yaml | 1.002 | 0.989 | 0.9984 |
| corpus/inspect_lua | 1.007 | 1.003 | 1.0025 |
| corpus/json_rxi | 1.002 | 1.003 | 1.0000 |
| corpus/msgpack_pure | 1.004 | 0.999 | 1.0000 |
| corpus2/dev/csv_report | 0.996 | 0.989 | 0.9907 |
| corpus2/dev/expr_compiler | 1.006 | 1.001 | 1.0015 |
| corpus2/dev/particles_physics | 0.997 | 0.997 | 1.0000 |
| lib/it_pairs | 1.001 | 0.997 | 1.0000 |
| programs/string_ops | 1.012 | 1.013 | 1.0000 |
| programs/table_hash | 0.957 | 1.003 | 0.8232 |

Worst 5-run programs of the `all` column that triggered the recheck: `programs/string_ops` 1.169 (15 runs 1.012, dup 1.013), `corpus/csv_ini_yaml` 1.072 (1.002), `corpus/json_rxi` 1.047 (1.002), `corpus/msgpack_pure` 1.047 (1.004), `corpus2/dev/csv_report` 1.047 (0.996), `lib/it_pairs` 1.034 (1.001), `particles_physics` 1.031 (0.997): all equal instructions, noise of the 5-run minimum. Instructions: the worst program is `corpus/containers_lru_heap` **+0.99 %** (bit 8: the concatenated-key loop of its priority queue goes through the `HREFV` walk instead of the tier-1 path; cycles 1.003 at 15 runs), then `inspect_lua` +0.25 % and `expr_compiler` +0.15 %; `programs/table_hash` instructions 0.823 (cycles 0.957 at 15 runs; 0.70 .. 1.13 between columns at 5 runs, the known swing of that program).

### Pieces and what each does (details in JIT_TRACE_REF)

* **R4 `string.rep` (bits 3 + 4)**. H's diagnosis ("the result length straddles the 40-byte limit and the class guard exits") was right, the cause was not: the class mismatch was the helper's answer 0 behind the `NE %lib, 0` guard, an `ExitSite::Alloc` exit that gets no side traces and is counted by the exit-storm rule (verbose: 396 k exits in 819 k iterations of `lib/rep`, `libcall=15`). The class is now an ordinary guard on the length computed from `TLEN` ops; the other class gets a side trace. With the guard alone `lib/rep` was 218 -> 194 Mcyc; the rest (194 -> 112) is the helper building the result by doubling (`extend_from_within`) instead of one `extend_from_slice` per copy. Mixed-class loops of `rep` with side traces: `bh_rep_mixed_hot`.
* **R1 `table.unpack(t)` (bit 5) and `table.move` (bit 6)**: no helper at all, unrolled IR (`TLEN` guarded to the length / `f e t` guarded to the values seen, array loads and stores); 16 elements at most.
* **R2 `tonumber(s)` of a non-numeral (bit 7)**, **concatenated keys (bit 8)**, **lean `find` (bit 9)**.
* Not shipped / not built: `math.random(m, n)` inline step, inline short-string `<`, positional `table.insert` / `table.remove`, `table.concat`, float `string.format`, `tonumber(s, base)`: see JIT_PARKED C61 with the census (which untraced library functions are left, weighted by cycle share).

### Deviations from the task text

* `fmt_s` and `byte_multi` were already recorded (BUNDLE-C / F2): nothing left to build for them except their loop overhead, not changed. `lib/char` is the BUNDLE-H state (no change). `lib/m_random` and `lib/str_lt_short` unchanged (R5/R6 not built, reasons in C61).
* The mutant runner of the full registry was not run (hours); the 17 mutants of BUNDLE-H `libcall` and BUNDLE-R were run with `TR_MUT_ONLY` on the final tree: 17 / 17 caught.

## BUNDLE-N (N-A): RETF infrastructure, steps N2a-N2c (2026-11-10): no performance goal; knob `retf` OFF by default

Base = `aa1eef10` (BUNDLE-R), new = branch commits `e8c78561` (N2a + N2b), `f1344534` (N2c), `eea90e62` (jit-diff set, cheaper handler checks, docs). Design and status: docs/JIT_TRACE_REF.md "BUNDLE-N: RETF", docs/JIT_PARKED.md C38; tests: docs/JIT_TESTING.md "BUNDLE-N". With `retf = 0` (the process default) IR and machine code are the base's; the knob only turns on side-trace `RETF` that ends right after the pop (little value alone: it saves the tier-1 `RETURN`).

**Identity gate** (`scripts/m_identity.py --mask-poll`, default configuration, `RUSTYNAIL_SEED=0`, equal-length binary directories `bin/a` (base) and `bin/x` (new), 223 programs in 9 sets; run after N2a, after N2b, after N2c and on the final tree): IR / snapshot text and the disassembled machine code of all ~7020 traces identical, program outputs and `[trace]` statistic lines identical, for 221 of 223 programs every time. The two exceptions are self-noisy against the unchanged base binary (base vs base differs the same way): `corpus/astar_jumper` (the trace count varies from run to run, 358-366) and `corpus2/dev/oop_mixins` (the traces are identical, two counters of the `[trace]` statistics lines differ by ~100 in 1.4 M: `centry` entries, `blacklisted exits reported`).

**Instructions and cycles** (`scripts/bench_session.py`, all 223 programs, CPUs 3-7 one program per core, 300 ms spin before every run, columns rotated, min of 5; columns `a` = base, `x` = new, `d` = a second copy of the base: the noise column; every output equals its `.expected` where one exists, 0 mismatches):

| | x / a | d / a |
|---|---|---|
| instructions geomean | **1.00006** | - |
| cycles geomean | **1.00075** | 0.99952 |

* **Instructions are not exactly identical: 17 of 223 programs are outside +-0.0005**, all deterministic (base spread 0): `corpus2/dev/oop_mixins` +0.29 % (the self-noisy program above), `corpus/lua_lexer` +0.15 %, `awfy/json` +0.09 %, `awfy/towers` / `cd` / `real_dis_x86` / `containers_lru_heap` +0.08 %, `lib/tconcat` -0.07 %, `corpus/astar_jumper` -0.4 % (the self-noisy program). With the trace compiler off (`RUSTYNAIL_JIT_TRACE=off`) and with the JIT off the instruction counts of `awfy/towers` are equal to within 1e-6 (+304 and +1104 of 915 M / 1007 M); with traces on `towers` runs 167 k instructions (0.086 %) more. The trace code is byte-identical, so the difference is in the Rust side of the trace compiler (recorder, passes, exit handler); the cheap candidates were removed (`emit_raw` per-instruction test, the oracle check moved to a cold function, one test of `seg_pops` in the handler) and the difference did not move, so it was not chased further (a symbol-level `perf` diff failed: too few samples). The N2 target "1.000 +-0.0005 on all programs" is NOT met: the worst non-self-noisy program is +0.15 %.
* **Cycles: 11 programs above 1.03 at 5 runs; at 15 serial runs on CPU 4 (min, with the dup column)**: `lib/co_yield` 1.0885 (dup 1.0137), `lib/tconcat` 1.0753 (1.0032), `lib/tremove_pop` 1.0394 (1.0056), `lib/find_pat` 1.0346 (0.9972), `lib/tonumber_int` 1.0309 (0.9998), `corpus/lua_lexer` 1.0176 (0.9985), `lib/concat_op_str` 1.0092, `corpus2/dev/oop_mixins` 1.0086, `lib/it_pairs` 0.9968, `corpus2/dev/event_promise` 0.9814, `jit-trace/inline_leaf` 1.0021, `programs/string_ops` 1.0008. The library programs have **identical instruction counts** (1.0000) and do not touch RETF code: these are the layout effects of the new binary (the project's "layout lottery", see C45 / BUNDLE-Q notes), not a cost of the feature. They are NOT explained away by a measurement: five library programs are 3-9 % slower in cycles in a binary whose instruction counts equal the base's. Whoever merges this should decide if that is acceptable for a knob-off infrastructure change (earlier bundles carried such layout effects and measured them with an `off` column; the gate for flipping `retf` is the N3 gate below, measured on the then-current binaries).
* Other builds: 5.4 build (`--cfg rustynail_lua54`) compiles and runs the `n_retf_*` programs with `retf=1` inert (`Rec::retf` refuses on 5.4: 0 RETF traces, outputs equal to the JIT-off run, `always` and `+verify`). ASan (nightly `-Zbuild-std`, `--features trace`, `RUSTYNAIL_SYSTEM_MALLOC=1`) with the knob on: the 16 `n_retf_*` programs in `always`, `+verify` and `+exitstorm=3` equal the interpreter, no AddressSanitizer error; the `nr` jit-diff seeds 1..40 (`off tr tremu trstorm`) report 7 seeds with a LeakSanitizer leak, which also happens with `retf=0` on the same binary (not caused by RETF, not investigated). The full `scripts/asan-jit.sh` suite was not run.

**Tests run on the final tree** (`--features trace`, `systemd-run` caps, `--test-threads=3`): `tr_core` 180 passed (`--skip mutants_are_caught_by_live_programs`; includes `nr_matrix` and `nr_retf_traces_are_installed`; 16 new programs in the default modes too), `tr_policy` 48, `tr_asm` 46, `tr_loop` 27, `tr_record` 34, `tr_exit` 35, `tr_ra` 19, `tr_determinism` 2, `tr_runtime` 43, `tr_record_calls` 37, `tr_hook` 14, `tr_poll` 3, `tr_passes` 15, `tr_ir` 18, `tr_class_a_pb` 19, `tr_hashd` 6, `tr_coro` 5, `tr_bd` 2, the mutant registry tests (606) and `TR_MUT_ONLY=retf_,seg_` (16 of 16 caught), tier 1 (`jit_arith`, `jit_table`, `jit_calls`, `jit_loops`, `jit_lifetime`, `jit_alloc`, `jit_stress`, `arith_diff`, `table_diff`, `emu_diff`, `emu_ops_diff`, `jit_review_{hooks,count}`, `jit_cstack`, `t15_analysis`, `t15_core_emu`, `t15_runtime`), `cargo test -p rn-core -p rn-std --features jit`; `cargo check -p rn-jit --all-targets` for `jit`, `jit,trace`, `trace,t15_oracle`, `t15_mutants`, `tr_mutants,trace`. jit-diff `nr` seeds 1..100, modes `off tr trdef trstorm tremu trstack` with `lockstep=1,retf=1,retfmax=1`: 0 diffs (83 % of the seeds entered traces; RETF traces installed in 25 of the first 40 seeds, 157 in total); the same with the shipped defaults and `JITDIFF_REF=ref/55/build/lua`: 0 diffs. Not run: `t15_oracle` / `tr_oracle` / `tr_identity_pre` (known failures), the full mutant runner (hours), `scripts/asan-jit.sh`, fuzz, a PGO build, a benchmark with the knob ON (no performance goal in this bundle).

**Gate for flipping the default in N3** (unchanged): `corpus/lua_lexer` <= 0.92 cycles, no program > 3 % worse in cycles (min of 5, outliers at 15 serial runs on CPU 4), none > 1 % worse in instructions unless explained, a 5.4 build and an ASan run with the knob on, the full mutant run.


## BUNDLE-N-B (N3a, N3b, N4, N5; 2026-11-10): RETF recording, inline re-push stub, `sidenest` and `tfmt` ON

Base = BUNDLE-N-A head `59148eb1` (knobs off = byte-identical traces, see below). Commits: `c1c37611` (N3a + N3b), `efdda8f9` (mutants, programs, jit-diff templates), `a0084c57` (N4: `sidenest` = 1, `retf` = 2), the N5 commit (`tfmt` = 1). Design and status: docs/JIT_TRACE_REF.md "BUNDLE-N-B", docs/JIT_PARKED.md C38; tests docs/JIT_TESTING.md "BUNDLE-N-B".

**What ships and what does not.**

| knob | default | what |
|---|---|---|
| `retf` bit 1 (value 2) | **ON** (`RETF_PROCESS` 2 in N-B, **3** since the RETF flip) | N3b: the frame-linking exit of a side trace is an inline stub (frames rebuilt, jump through the head's dispatch cell) instead of the Rust handler |
| `sidenest` | **ON** (`SIDENEST_PROCESS = 1`) | C38 slice 1: a side recording that reaches a callee loop head with a root trace ends with the frame-linking exit (needs the stub: without it queens +43 %, roguelike_turns +16 % cycles) |
| `tfmt` | **ON** (`TFMT_PROCESS = 1`) | N5: `string.format` of constant simple formats recorded; the earlier real_dis_mips loss (1.042) is gone |
| `retf` bit 0 (value 1), `retfmax` | OFF in N-B, **ON since the RETF flip** (`retfmax` 1) | N3a: side recordings pop the root frame (RETF) and go on in the caller |
| `retf` bit 2 (value 4) | OFF | N3c-lite: the same for function-entry families (needed for `lua_lexer`); regressions on recursion shapes |

The lua_lexer gate of the N3 plan (<= 0.92 cycles) is NOT met with any setting; N-B therefore shipped with the RETF recording bits OFF. `retf=3,retfmax=1` passed every other gate and was flipped afterwards with the gate waived by the user (section "RETF flip" at the end of this bundle's tables, below).

**Method.** One binary (release `--features jit`, commit `efdda8f9` + knob defaults as stated), the columns are `RUSTYNAIL_JIT_TRACE_LIMITS` strings in the same binary (same file, same script path, equal-length environments through `RUSTYNAIL_PAD`), `scripts/bench_session.py`, all 223 programs, CPUs 3-7 one program per core, 300 ms spin before every timed run, columns rotated, min of 5 (`perf stat -e cycles:u,instructions:u`), a duplicate column `d` of the base, every output equals its `.expected` (0 mismatches). Outliers rechecked at 15 serial runs on CPU 4. The "off" base of every ratio is `sidenest=0,retf=0,tfmt=0`.

**All 223 programs (geomean new / base; cycles / instructions):**

| column (limits) | cycles | instructions | programs > 3 % cycles (5 runs) | > 1 % instructions |
|---|---|---|---|---|
| `retf=2,sidenest=1` (the N4 default) | 1.0006 / 0.9999 / 1.0006 (three runs of the suite) | 1.0003 / 1.0002 / 1.0003 | 5 / 2 / 4 | 3 |
| dup of the base | 0.9999 / 1.0006 / 1.0003 | 1.0000 | 2 | 0 |
| `retf=3,retfmax=1,sidenest=1` | **0.9970** | **0.9983** | 5 | 3 |
| `retf=7,retfmax=1,sidenest=1` | 1.0015 | 1.0032 | 17 | 11 |
| `retf=7,retfmax=2` (no sidenest) | 1.0018 | 0.9994 | 12 | 10 |
| defaults + `tfmt=1` | **0.9987** | **0.9986** | 2 | 3 |

Every program above +3 % cycles in the default column has instructions 1.000 (the same machine code executes): `programs/string_ops`, `lib/sel_count`, `programs/table_hash`, `corpus2/dev/event_promise`, `corpus2/dev/csv_report` swing between 0.96 and 1.10 from one suite run to the next with the same binary (layout lottery, the dup column does the same: 1.065, 1.054, 1.045). At 15 serial runs on CPU 4 (columns `a` base, `b` = `retf=2,sidenest=1`): `awfy/cd` 1.019, `corpus/msgpack_pure` 1.014, `corpus/lua_lexer` 1.014, `lib/str_eq_dyn` 1.011, all others 0.98-1.005; dup 0.995-1.009.

**Instructions above +1 % (default column), explained:** `awfy/queens` +7.1 % instructions but **0.884 cycles** (the frame-linking exit now costs ~15 inline instructions instead of a handler call; the same program was 1.164 cycles / 1.358 instructions with `sidenest=1` before the stub); `corpus/msgpack_pure` +2.2 % instructions, 1.014 cycles (15 runs); `corpus2/heldout/sudoku_solver` +1.8 %, 0.982 cycles; `awfy/cd` +0.8 %, 1.019. All are the same mechanism: a side trace links into a callee loop trace (materialise frames inline + enter) instead of unrolling the loop path.

**Targets (cycles / instructions of each column over the base):**

| program | N4 default (`retf=2,sidenest=1`) | `retf=3,retfmax=1,sidenest=1` | `retf=7,retfmax=1,sidenest=1` | `retf=7,retfmax=2` (sidenest 0) |
|---|---|---|---|---|
| corpus/lua_lexer | 1.016 / 1.000 | 1.020 / 1.000 | 0.977 / 0.919 | 0.992 / 0.941 |
| jit-trace/short_inner | 1.000 / 1.000 | **0.637 / 0.867** | 0.639 / 0.874 | 0.763 / 0.930 |
| corpus2/heldout/stream_closures | 1.006 / 1.000 | 1.000 / 0.999 | 1.048 / 1.115 | 1.052 / 1.115 |
| awfy/queens | **0.884** / 1.071 | 0.884 / 1.071 | 0.783 / 0.920 | 1.013 / 0.863 |
| corpus2/heldout/roguelike_turns | 1.011 / 0.977 | 1.011 / 0.971 | 1.446 / 1.782 | 0.993 / 0.941 |
| awfy/list | 1.001 / 1.000 | 0.944 / 0.963 | 0.919 / 0.945 | 1.239 / 1.265 |
| jit-trace/field_rw | 1.000 / 1.000 | 0.922 / 0.920 | 0.922 / 0.920 | 0.922 / 0.921 |
| corpus2/heldout/dijkstra_routes | 0.992 / 0.976 | 0.964 / 0.939 | 1.256 / 1.571 | 0.997 / 0.972 |
| corpus2/dev/expr_compiler | 0.998 / 0.996 | 1.000 / 0.996 | 0.936 / 0.926 | 0.934 / 0.934 |
| real/lua_interp | 0.998 / 1.000 | 0.998 / 1.000 | **1.403 / 1.286** | 1.401 / 1.286 |
| corpus2/dev/regex_nfa | 1.019 / 1.000 | 1.003 / 1.000 | 1.252 / 1.077 | 1.255 / 1.077 |
| awfy/havlak | 0.995 / 0.999 | 1.010 / 1.000 | 1.102 / 1.086 | 1.065 / 1.052 |
| corpus2/heldout/real_dis_mips | 1.003 / 1.000 | 0.999 / 1.000 | 1.002 / 1.000 | 1.004 / 1.000 |
| corpus/parser_combinator | 1.004 / 0.999 | 0.999 / 0.998 | 1.031 / 1.052 | 0.976 / 0.998 |
| corpus2/heldout/scheme_interp | 0.996 / 0.999 | 0.995 / 0.999 | 0.980 / 0.912 | 0.981 / 0.905 |
| corpus2/dev/class_middleclass / corpus2/heldout/classic_vector | 1.004 / 1.005 | 1.004 / 1.009 | 1.017 / 1.003 | 1.006 / 0.994 |

Reading: (1) the inline stub is what makes `sidenest` shippable (queens, roguelike_turns were the earlier losers); (2) side RETF of loop-root families (bit 0, `retfmax=1`) gives `short_inner` -36 % cycles, `list` -5.6 %, `field_rw` -7.8 %, `dijkstra_routes` -3.6 %, no regression above noise; (3) the function-entry bit (4) is what helps `lua_lexer` (0.977 cycles, 0.919 instructions: far from the hand-inlined ceiling 0.78 of the N3 probe and from the 0.92 gate) and also what produces the regressions (`lua_interp` 1.40, `regex_nfa` 1.25, `roguelike_turns` 1.45, `dijkstra_routes` 1.26): native-recursion families (`rec_call` / `rec_ret`) and the `Compile(asm-inconsistent)` aborts seen in `lua_interp` (a fail-stop, the recording is dropped) show that RETF in function-entry roots needs its own policy work; (4) `retfmax=2` costs `awfy/list` +24 % (the second pop lands in a polymorphic caller) and does not help any target, so `retfmax = 1`.

**Per-step table (same binary, cycles / instructions over `sidenest=0,retf=0,tfmt=0`, min of 5 on CPU 4):** 

| column | lua_lexer | short_inner | stream_closures | queens | roguelike_turns | list |
|---|---|---|---|---|---|---|
| N3a only: side RETF (retf=1,max 1, sidenest 0) | 0.996 / 1.000 | 0.769 / 0.938 | 0.998 / 0.999 | 0.997 / 1.000 | 1.009 / 1.008 | 0.932 / 0.961 |
| N3a+N3b (retf=3, sidenest 0) | 0.994 / 1.000 | 0.769 / 0.938 | 0.994 / 0.999 | 1.002 / 1.000 | 1.010 / 1.008 | 0.934 / 0.961 |
| sidenest only, no stub (retf=0, sidenest 1) | 1.026 / 1.010 | 1.002 / 1.000 | 0.997 / 1.000 | 1.164 / 1.361 | 1.158 / 1.158 | 1.008 / 1.000 |
| N3b+N4 = the shipped default (retf=2, sidenest 1) | 1.009 / 1.000 | 0.999 / 1.000 | 0.997 / 1.000 | 0.885 / 1.071 | 1.009 / 0.977 | 1.001 / 1.000 |
| N3a+N3b+N4 (retf=3,max 1, sidenest 1) | 1.015 / 1.000 | 0.637 / 0.867 | 0.995 / 0.999 | 0.881 / 1.071 | 1.007 / 0.971 | 0.951 / 0.963 |
| + function-entry bit (retf=7,max 1, sidenest 1) | 0.970 / 0.919 | 0.638 / 0.874 | 1.056 / 1.122 | 0.784 / 0.920 | 1.440 / 1.782 | 0.918 / 0.945 |

(`tfmt=0` in all columns; base = `sidenest=0,retf=0`. `short_inner` needs side RETF and `sidenest` together: -23 % with RETF alone, -36 % with both; `queens` / `roguelike_turns` are cured by the stub alone.)

**N5 (`tfmt`) retest** (columns `a` = base, `b` = `retf=2,sidenest=1,tfmt=1`; 9 runs, CPUs 3-7, 15 serial runs for the small ones): real_dis_arm 0.981 / 0.973, real_dis_x86 0.938 / 0.937, real_dis_mips **0.999 / 0.993** (was 1.042 / 1.022 in BUNDLE-C; the mechanism was not re-diagnosed: the number is measured with `sidenest` on), real_dis_ppc 0.962 / 0.966, lib/fmt_d 0.888 / 0.867, lib/fmt_x_pad 0.888 / 0.922, lib/fmt_f 0.997, fmt_g 1.001, fmt_mixed 0.993, fmt_q 0.988, fmt_s 0.992 (instructions 1.000: the formats `tfmt` does not take). Whole suite with the defaults + `tfmt=1`: 0.9987 cycles / 0.9986 instructions, 2 programs above +3 % (both instructions 1.000, noise). jit-diff set `cc` (the `tfmt` templates) 80 seeds with the reference build: 0 diffs.

**Identity with the new knobs OFF** (`scripts/m_identity.py --mask-poll`, `RUSTYNAIL_SEED=0`, A = the N-A head binary with its defaults, B = this tree with `sidenest=0,retf=0,tfmt=0`, equal-length directories): 223 programs, 7026 traces, 221 identical (IR text, disassembled machine code, output, `[trace]` statistics lines); the two others are the self-noisy programs of the N-A report (`corpus/astar_jumper`: 364 vs 358 traces, `corpus2/dev/oop_mixins`: two counters).

**Gate summary for flipping `retf=3,retfmax=1`** (not flipped): lua_lexer <= 0.92: FAIL (1.020); two of {short_inner, stream_closures, lua_lexer} clearly better: short_inner only (stream_closures 1.000, lua_lexer 1.020); no program > 3 % cycles at 15 runs: pass; instructions > 1 %: queens / msgpack_pure / sudoku_solver / cd (same list as the default column, explained); 5.4 build: pass; ASan: pass; mutants: pass. Cheap retest: `RUSTYNAIL_JIT_TRACE_LIMITS=retf=3,retfmax=1` columns with `scripts/bench_session.py`.

## RETF flip (2026-11-10): `RETF_PROCESS` 2 -> 3

`RETF_PROCESS = 3`, `RETFMAX_PROCESS = 1`, bit 2 (function-entry families) OFF. The lua_lexer gate (<= 0.92 cycles) was waived by the user; the evidence for the flip is the `retf=3,retfmax=1` column of BUNDLE-N-B above (all-223 0.9970 cycles / 0.9983 instructions against retf=2 plus the movers there).

**Method.** One release binary (`--features jit`, commit of the flip + oracle fix), columns are `RUSTYNAIL_JIT_TRACE_LIMITS` strings in the same binary: `r3` = `retf=3` (the new default, given explicitly), `r2` = `retf=2` (the old default), `r3b` = a dup of `r3` (noise), `r0` = `retf=0` (15-run table only). `scripts/bench_session.py`, `RUSTYNAIL_SEED=0`, 223 programs, CPUs 3-7, 300 ms spin before every timed run, columns rotated, **min of 5** cycles:u / instructions:u. Outliers re-measured with **15 serial runs on CPU 4** (matmul and it_next also on CPU 6).

**All 223 programs (geomean r3 / r2):** cycles **0.9975**, instructions **0.9980** (the dup column `r3b` / `r2`: 0.9966 / 0.9980; `r3` / `r3b`: 1.0010 / 1.0000, so 0.1 % is the noise floor of the geomean). By set (cycles / instructions): corpus 0.998 / 1.000, corpus2/dev 1.002 / 1.000, corpus2/heldout 0.996 / 0.994, awfy 0.997 / 0.997, programs 1.008 / 1.000, jit-trace 0.975 / 0.989, lib 1.001 / 1.000, apps 0.998 / 1.000, real 1.001 / 1.000. Gate set (lua_lexer, queens, list, field_rw, dijkstra_routes, stream_closures, roguelike_turns, short_inner): 0.925 / 0.963. **Programs above +1 % instructions: none.** Above +3 % cycles in the 5-run table: 5 (the dup column has 3): `programs/heapsort` 1.083 (instructions 0.997; 15 runs on CPU 4: 1.003, dup 1.003), `programs/matmul` 1.035, `lib/sel_count` 1.035 (15 runs 0.976), `lib/it_next` 1.034 (15 runs 1.033 on CPU 4 but dup 1.001, and 0.97 on CPU 6), `programs/table_hash` 1.031 (15 runs 1.001).

| program (15 runs, CPU 4) | cycles r3 / r2 | instructions r3 / r2 | dup r3b / r2 | cycles r3 / r0 (retf=0) | instructions r3 / r0 |
|---|---|---|---|---|---|
| jit-trace/short_inner | **0.637** | 0.867 | 0.637 | 0.635 | 0.867 |
| jit-trace/field_rw | **0.923** | 0.920 | 0.923 | 0.923 | 0.920 |
| awfy/list | **0.937** | 0.963 | 0.935 | 0.939 | 0.963 |
| corpus2/heldout/dijkstra_routes | 0.999 | 0.965 | 0.999 | 0.948 | 0.913 |
| awfy/queens | 0.998 | 1.000 | 0.997 | 0.762 | 0.787 |
| corpus2/heldout/roguelike_turns | 0.993 | 0.999 | 0.991 | 0.849 | 0.843 |
| corpus2/heldout/stream_closures | 1.002 | 0.999 | 1.000 | 1.004 | 0.999 |
| corpus/lua_lexer | **0.997** (gate: within +2 %) | 1.000 | 0.998 | 0.986 | 0.990 |
| programs/matmul | 1.030 | 1.000 | 1.026 | 1.025 | 1.000 |
| lib/it_next | 1.033 | 1.000 | 1.001 | 1.032 | 1.000 |
| programs/heapsort | 1.003 | 0.997 | 1.003 | 1.003 | 0.997 |
| lib/sel_count | 0.976 | 1.000 | 0.975 | 1.012 | 1.000 |
| programs/table_hash | 1.001 | 1.000 | 0.999 | 1.009 | 1.000 |

(`r3 / r0` for queens and roguelike_turns is the effect of `sidenest` + stub on top of `retf=0`; the RETF bit adds nothing there: `r3 / r2` ~ 1.0. `corpus2/heldout/dijkstra_routes` 0.964 and `stream_closures` 0.995 of the N-B table reproduce as 0.999 / 1.002 against `retf=2` here: the instruction gain of dijkstra_routes (0.965) does not show in cycles.)

**`programs/matmul` is the one open item of the gate.** 15-run minima 1.030 (CPU 4, dup 1.026) and 1.033 (CPU 6, dup 1.019), 5-run 1.035; medians 1.017; instructions +0.016 % (2203422321 vs 2203066779); the `retf=0` column equals `retf=2` (0.995 / 1.000 of it). `RUSTYNAIL_JIT_VERBOSE=1` shows identical trace statistics for `retf=3` and `retf=2` (6 traces installed, 19 side attempts, 6 installed, no RETF trace, no frame link), so the installed machine code is the same as far as the statistics tell; the effect is a code-placement / alignment difference of a few hundred thousand instructions of different aborted side recordings, not diagnosed. It is at the 3 % limit and the identical-config dup column differs by 1.4 % from `r3` on the same program, so it is treated as noise at the limit, not as a regression; a retest after any later change to side-recording order is cheap (`RUSTYNAIL_JIT_TRACE_LIMITS=retf=2|retf=3` columns).

**A bug found by the flip (fixed in the same commit).** The `+verify` segment oracle read the entry frame's function slot after `restore` had reused the popped frame's `CallInfo` node: 15 `tr_core` tests failed with "segment oracle: the function slot of the caller frame" the moment `retf` bit 0 was on by default (test-only check, release code unaffected). Details: docs/JIT_TRACE_REF.md "RETF flip". Six more `tr_core` tests had premises tied to the shape of the pre-RETF traces and now name `retf=2` in their limits (`c39_inner_programs`, `frame_links_and_stitches`, `p_laprune_programs`, `j1_sinkstub_alloc_failure_raises_memory_error`, `sink_materialise_alloc_failure_raises_memory_error`, `side_traces_install_and_run`); `nr_matrix` runs `retf=0` and the process default as controls.

## BUNDLE-C45 (2026-11-10): `rearmside=2` + `rearmclean=1` (young rearm entries into clean families) SHIPPED

Mechanism (docs/JIT_TRACE_REF.md "BUNDLE-C45", study ~/.cache/studyC45): BUNDLE-P's `rearmside` made the rearmed tick enter a young head (root trace entered fewer than 1024 times) once its family had `k` installed side traces: classic_vector 0.904, but the entries
changed the formation of families that were still being shaped (`lua_lexer`, `awfy/json`, `awfy/queens`, `heapsort`, `iter_pipeline`). The C45 study found that the winners are families whose side recordings all succeeded and the losers are families
that already had a failed side recording or unrolled a callee loop (`Trace::la`). New rule: a young head is entered only if its family has `rearmside` side traces **and** `side_faults == 0` **and** no member unrolled a callee loop (`la_any`);
process defaults `REARMSIDE_PROCESS` = 2 (was 0), new knob `rearmclean` (`REARMCLEAN_PROCESS` = 1; 0 = the plain BUNDLE-P rule).

**Method.** One release binary (`--features jit`, commit e0764f15), columns = `RUSTYNAIL_JIT_TRACE_LIMITS` strings of equal length: `off` = `rearmside=0,rearmclean=1`, `on` = `rearmside=2,rearmclean=1` (the shipped values), `dup` = `rearmside=0,rearmclean=0` (identical
behaviour to `off`: the noise column). All 223 programs (`scripts/bench_session.py`, CPUS 3-7, `SPIN_MS=300`, 5 runs, min, identical script paths, same environment length), perf stat `cycles:u` / `instructions:u`. Outputs equal the reference Lua's on all 223 (exact or numeric) and every `.expected` matches.
Outliers re-run serially on CPU 4, 15 runs, same three columns.

**Results (all 223, `on` / `off`).** Geomean cycles 0.9994, instructions 0.9989 (dup/off 0.9994 / 1.0000). Gate numbers:

| gate | result |
|---|---|
| corpus/classic_vector cycles <= 0.92 | 0.881 (15 runs: 0.881) |
| corpus/classic_vector instructions <= 0.92 | 0.885 |
| any program > +1 % instructions | none (worst: `forth_vm` +0.83 %, `lua_parser` +0.12 %; every other program within +0.05 %) |
| any program > +3 % cycles (5 runs) | `programs/string_ops` 1.056 (dup 1.031), `lib/it_pairs` 1.036 (dup 1.011): both at 1.0000 instructions, i.e. the knob changes nothing there; at 15 runs on CPU 4: 1.012 / 1.004 (dup 1.004 / 1.010) |
| cycles at 15 runs: lua_lexer / awfy/cd / vending_fsm / lua_parser | 0.991 / 1.001 / 0.997 / 1.010 (dup 1.000 / 1.001 / 1.001 / 0.998) |

Programs whose instruction count changes by more than 0.05 % (5-run min; `dup` = same binary with the knob off, noise estimate):

| program | cycles on/off | instructions on/off | dup cycles |
|---|---|---|---|
| corpus/classic_vector | 0.881 | 0.8846 | 1.004 |
| corpus2/heldout/sudoku_solver | 0.961 | 0.8992 | 0.994 |
| corpus2/heldout/vending_fsm | 0.998 | 0.9893 | 0.996 |
| corpus/astar_jumper | 1.005 | 0.9927 | 1.001 |
| corpus2/heldout/dijkstra_routes | 0.977 | 0.9939 | 0.996 |
| corpus2/heldout/roguelike_turns | 1.026 (15 runs 1.018) | 0.9949 | 1.002 |
| corpus/lua_lexer | 0.988 (15 runs 0.991) | 0.9979 | 1.001 |
| programs/spectral_norm | 1.001 | 0.9982 | 1.002 |
| corpus/serpent_ser | 1.001 | 0.9993 | 0.997 |
| corpus2/dev/lua_parser | 1.004 (15 runs 1.010) | 1.0012 | 1.001 |
| corpus2/heldout/forth_vm | 1.025 (15 runs 1.009) | 1.0083 | 1.003 |

All other 212 programs: instructions within 0.05 % of the `off` column (the rule does not fire). Full per-program table (cycles on/off, instructions on/off, dup/off cycles) in `docs/bench-c45-all223.md`.
The study's numbers (classic_vector 0.866 instr, geomean 0.9994, worst instr +0.8 %) reproduce on the current main (N-B merged); classic_vector's gain is smaller in instructions (0.885) because N-B changed the trace set.

Seeds (`RUSTYNAIL_SEED` 1..3, classic_vector, one run each, CPU 4): instructions 0.885 / 0.885 / 0.885 (on/off), cycles 0.892 / 0.934 / 0.883 (the 0.934 is a single noisy run: instructions are the stable signal).
The knob is a no-op for `heapsort`, `iter_pipeline`, `awfy/queens`, `awfy/json` (instructions within 0.05 %: their families are unclean at every rearm tick, `rearm: ... unclean F/L` in the trace statistics counts the refusals).

**Counters.** `[trace] rearm: armed A entered E missed M restored R unclean F/L`: F = young heads refused (with enough side traces) because of a failed side recording, L = because a member unrolled a callee loop.


## BUNDLE-V (2026-11-10): `t1q` bits 4 / 5 / 6 (tier-1 arithmetic metamethod frames, tail calls and general returns in metamethod frames) and `upbar` (recorded upvalue stores of collectable values) SHIPPED (process `t1q` 125, `upbar` 1); float `%` (V3) diagnosed, stays OFF

Mechanisms, tests and mutants: docs/JIT_TRACE_REF.md "BUNDLE-V", docs/JIT_TESTING.md "BUNDLE-V"; what was not built: docs/JIT_PARKED.md C58 / C59 / C60 / C62. Commits (branch `<worktree>`, base `bcd52469`): `3631b57e` (V1 + V2 mechanism, tests, mutants, jit-diff set `bv`), then the follow-up commits listed in the final report. Plain `cargo build --release -p rn-cli --features jit`. Knobs: `RUSTYNAIL_JIT_TRACE_LIMITS=t1q=<mask>` (1 `EQ`, 2 float `%` (OFF), 4 / 8 `__index` / `__newindex` frames, **16 arithmetic metamethod frames, 32 `TAILCALL` in metamethod frames, 64 general `RETURN` in metamethod frames**, clamped to 127) and `upbar=<0|1>`.

**Diagnosis that led to V1.** BUNDLE-D's `armm` (tier-1 arithmetic metamethod frames, `Cold::ArithMm`) had been off since 2026-11-11 because `programs/metamethods` went 2.24x: the continuation after the metamethod was an *ordinary* resume entry after every arithmetic instruction, i.e. an interpreter re-entry point, which moved the trace formation (BUNDLE-S found and fixed the same effect for table reads and stores, docs/JIT_PARKED.md C60, and listed "private entries for `armm`" as an untried idea). With the continuation pushed into the private, flagged entry list (`resume_mm`, one line) `armm` costs `programs/metamethods` nothing (0.99). `classic_vector` needed a second piece: its handlers are `return Vec(a.x + b.x, ...)`, a `TAILCALL` of a table with a vararg `__call`, and tier 1 left to the interpreter at every `TAILCALL` (and at the vararg `RETURN`) of a `CIST_MMRET` frame: with `armm` alone `t1q=29` it did not move (1.000), with bit 5 and 6 it is 0.67.

**Method.** As BUNDLE-S: equal-length paths (`bin/a/rn-cli` = base `bcd52469`, `bin/d` = a copy of it (noise column `based`), `bin/x` = new), `RUSTYNAIL_SEED=0`, equal-length environment (padded), identical script paths, CPUs 3-7 (one program per core), a 300 ms spin of the core before every timed run, columns rotated, min of 5 runs, cycles:u / instructions:u (`scripts/bench_session.py`, 223 programs in 9 sets; outputs equal `ref/55/build/lua` and `.expected`: 0 mismatches), plus the new binary with the V1 bits off (`off` column: `t1q=13`; it still has `upbar=1`). Outliers re-measured serially on CPU 4, 15 runs, with two more columns: `up0` (`upbar=0`) and `off0` (`t1q=13,upbar=0`, the new binary with both pieces off).

### Geomeans, ratio to `base` (cycles / instructions)

| set | n | noise (`based`) | **new (defaults)** | off (`t1q=13`) |
|---|---|---|---|---|
| ALL | 223 | 1.0007 / 1.0000 | **0.9955 / 0.9963** | 0.9990 / 0.9985 |
| GATE apps+corpus+real | 28 | 1.0021 / 1.0003 | **0.9840 / 0.9723** | 0.9977 / 0.9868 |
| apps | 5 | 0.9953 / 1.0000 | 0.9995 / 0.9677 | 0.9944 / 0.9680 |
| awfy | 14 | 0.9996 / 1.0000 | 1.0024 / 1.0016 | 0.9997 / 1.0015 |
| corpus | 18 | 1.0034 / 1.0004 | 0.9727 / 0.9659 | 0.9970 / 0.9883 |
| corpus2/dev | 19 | 0.9948 / 1.0000 | 0.9932 / 0.9960 | 1.0001 / 1.0005 |
| corpus2/heldout | 13 | 1.0028 / 1.0000 | 0.9939 / 0.9999 | 0.9981 / 1.0001 |
| jit-trace | 29 | 1.0013 / 1.0000 | 0.9997 / 1.0001 | 1.0013 / 1.0000 |
| lib | 95 | 1.0005 / 1.0000 | 0.9970 / 1.0001 | 0.9970 / 1.0000 |
| programs | 25 | 1.0032 / 1.0000 | 0.9967 / 1.0000 | 1.0045 / 1.0000 |
| real | 5 | 1.0044 / 1.0000 | 1.0098 / 1.0005 | 1.0035 / 1.0007 |

The `off` column carries `upbar=1`: the difference between `off` and `new` is V1 (bits 4 / 5 / 6), the difference between `off` and 1.0 is V2 (`upbar`) plus the layout of the new binary. Per piece: **V1** = `classic_vector` (0.674), `bignum_arith` (0.941 / 0.922 instructions: its `__add` / `__mul` handlers); nothing else moves by more than noise. **V2** = `apps/lru_cache` (0.947 / 0.846) and `corpus/containers_lru_heap` (0.887 / 0.831; both in the `off` column, i.e. not V1).

### Targets (5 runs min; Mcyc / Minstr; the `off` column is the new binary with `t1q=13`)

| program | base Mcyc | base Minstr | new Mcyc | new Minstr | cycles | instr | noise col | off (cyc / instr) |
|---|---|---|---|---|---|---|---|---|
| corpus/classic_vector | 1022.4 | 4592.3 | 688.6 | 3024.2 | **0.674** | 0.659 | 1.012 | 1.011 / 1.000 |
| apps/lru_cache | 161.7 | 472.6 | 153.1 | 400.0 | **0.947** | 0.846 | 1.000 | 0.948 / 0.847 |
| corpus/containers_lru_heap | 1587.0 | 7012.4 | 1407.9 | 5825.1 | **0.887** | 0.831 | 1.001 | 0.892 / 0.831 |
| corpus2/dev/bignum_arith | 855.5 | 3004.0 | 805.2 | 2769.0 | **0.941** | 0.922 | 1.006 | 1.002 / 1.002 |
| corpus2/dev/oop_mixins | 1634.8 | 6295.5 | 1658.1 | 6318.6 | **1.014** | 1.004 | 1.013 | 1.014 / 1.004 |
| awfy/cd | 255.3 | 816.0 | 262.9 | 832.3 | **1.030** | 1.020 | 1.015 | 1.027 / 1.020 |
| programs/metamethods | 712.1 | 4076.9 | 704.2 | 4077.6 | **0.989** | 1.000 | 0.992 | 0.990 / 1.000 |
| corpus2/heldout/roguelike_turns | 801.8 | 3156.1 | 791.9 | 3156.0 | **0.988** | 1.000 | 1.006 | 0.992 / 1.000 |
| corpus/astar_jumper | 1700.8 | 4987.5 | 1715.4 | 5036.9 | **1.009** | 1.010 | 1.006 | 1.012 / 1.004 |
| corpus/class_middleclass | 488.4 | 2552.7 | 490.7 | 2553.2 | **1.005** | 1.000 | 1.003 | 0.993 / 1.000 |
| apps/expr_lang | 145.2 | 620.0 | 150.8 | 620.4 | **1.038** | 1.001 | 1.002 | 1.031 / 1.003 |
| jit-trace/inline_leaf | 460.8 | 4414.8 | 481.0 | 4414.8 | **1.044** | 1.000 | 1.019 | 1.056 / 1.000 |
| lib/tonumber_bad | 147.1 | 1037.9 | 156.1 | 1037.9 | **1.061** | 1.000 | 0.993 | 1.007 / 1.000 |

15-run serial check of the programs that were slow or interesting in the 5-run pass (min / median cycles ratio to `base`; instruction ratio in brackets; columns `new` / `off` (`t1q=13`) / `up0` (`upbar=0`) / `off0` (both pieces off) / `based`):

| program | based | new | off | up0 | off0 |
|---|---|---|---|---|---|
| corpus/classic_vector | 1.004 / 1.009 | **0.668 / 0.674** (0.659) | 1.010 / 1.016 | 0.668 / 0.676 (0.659) | 1.006 / 1.016 |
| apps/lru_cache | 1.005 / 1.003 | **0.958 / 0.954** (0.846) | 0.948 / 0.942 | 1.012 / 1.012 (1.000) | 1.000 / 0.993 |
| corpus/containers_lru_heap | 1.006 / 1.003 | **0.888 / 0.889** (0.831) | 0.888 / 0.896 | 1.001 / 1.006 (1.000) | 1.000 / 1.006 |
| corpus2/dev/oop_mixins | 0.997 / 0.994 | 0.989 / 0.987 (1.004) | 0.980 / 0.987 | 0.990 / 0.994 | 0.974 / 0.982 |
| awfy/cd | 0.997 / 0.995 | 1.017 / 1.016 (**1.020**) | 1.020 / 1.014 | 1.005 / 1.000 (1.001) | 0.994 / 0.994 |
| apps/expr_lang | 1.000 / 1.004 | **1.036 / 1.033** (1.001) | 1.020 / 1.018 | 1.038 / 1.036 | 1.012 / 1.019 |
| corpus2/dev/diff_levenshtein | 1.003 / 1.005 | 1.026 / 1.029 (1.000) | 1.015 / 1.017 | 1.020 / 1.027 | 1.014 / 1.018 |
| programs/closures | 0.996 / 0.998 | 1.028 / 1.019 (1.000) | 1.027 / 1.018 | 1.025 / 1.021 | 1.023 / 1.016 |
| jit-trace/inline_leaf | 0.990 / 1.006 | 1.012 / 1.021 (1.000) | 1.000 / 0.997 | 1.030 / 1.018 | 0.996 / 0.991 |
| lib/tonumber_bad | 1.002 / 0.999 | 1.002 / 1.027 (1.000) | 1.010 / 1.031 | 0.996 / 1.020 | 0.999 / 1.062 |
| lib/str_method_len | 1.000 / 1.004 | 1.002 / 1.024 (1.000) | 1.001 / 1.005 | 1.014 / 1.022 | 1.014 / 1.030 |
| corpus2/dev/wordfreq_wrap | 1.004 / 1.004 | 1.003 / 1.006 (1.000) | 1.007 / 1.003 | 1.003 / 1.007 | 1.001 / 1.008 |

### Gate and what misses it

* **Gate:** targeted programs improve: `classic_vector` 0.67 (V1), `lru_cache` 0.95 and `containers_lru_heap` 0.89 (V2, not V1), `bignum_arith` 0.94; **`oop_mixins` and `awfy/cd` do not improve** (their metamethod / `__call` work is not on the tier-1 exit list: cd's 200 exits are in `getupval` / `gettabup` heads, `oop_mixins` has no exit above 130; their time is in traces), `programs/metamethods` 0.99, `corpus2/heldout/roguelike_turns` 0.988 (V2 removed its `Nyi(Setupval)` aborts (2 -> 0) but the loops still stop at `IntMiss`; no gain). All-223 0.9955 cycles / 0.9963 instructions; gate set 0.9840 / 0.9723.
* **Cycles worse than +3 % at 15 runs: `apps/expr_lang` +3.6 % (min) / +3.3 % (median) at 1.001 instructions.** The `off` column (same binary, V1 off) has +2.0 % / +1.8 %, `off0` +1.2 % / +1.9 %, `up0` (V1 on, V2 off) +3.8 %: roughly +2 % of binary layout (cold stubs shift later blocks, BENCHMARK 3 notes) and +1.6 % attributable to the V1 bits, with identical hot-path instructions (`expr_lang` has no metamethod handler; the new code is cold). Reported, not chased. Everything else of the 5-run outliers (`tonumber_bad` 1.061 at 5 runs, `inline_leaf` 1.044, `wordfreq_wrap` 1.033, `str_method_len` 1.034) is 1.002-1.012 at 15 runs; `closures` (+2.8 %) and `diff_levenshtein` (+2.6 %) are the binary (`off0` 1.023 / 1.014).
* **Instructions worse than +1 %: `awfy/cd` +2.0 %** (cycles +1.7 % at 15 runs): it is V2: `up0` 1.001 instructions, `off` 1.020. `cd` stores tables into upvalues in a hot loop; with `upbar` the loops that used to abort `Nyi(Setupval)` are recorded and the trace set changes (more instructions in the new traces, no gain). `corpus/astar_jumper` +1.0 % (5 runs; `off` 1.004; its trace set moves with anything: BUNDLE-Q / S notes).
* **Layout lottery, unchanged by the knobs:** `lib/tonumber_bad`, `lib/m_random_f`, `lib/tonumber_flt`, `jit-trace/alloc_ctor`, `corpus/csv_ini_yaml` moved by 4-7 % in single 5-run passes at equal instructions and are within 3 % at 15 runs or in the `off` column.
* **Found on the way (do not repeat):** (1) `class_middleclass` +7 % cycles at -3 % instructions with *native* tail callees (`return rawget(dict, k)`) in `__index` frames staying in tier 1: formation again (11 traces recorded instead of 14, 53 side traces installed instead of 44); `jit_mm_tail_ok` keeps those with the interpreter. The arithmetic frames need them (`programs/metamethods`'s `return setmetatable(...)`: with native tail calls refused for arithmetic callers `metamethods` is 2.3x slower, the old `armm` result), so the rule is by caller instruction. (2) The vararg `RETURN` support is needed (`classic_vector` 0.67 with it, 1.10 without: its `Object:__call(...)` returns from a vararg frame, which is reached in compiled code). (3) A test that calls `collectgarbage("step")` inside a loop never ran a trace (`CFunc` abort): the first `bv_upbar` did not exercise the barrier.
* **Not measured:** PGO builds, the 5.4 build run (only `cargo check`), ASan, fuzz.

### V3: float `%` (`t1q` bit 2): diagnosis, stays OFF

`astar_jumper`, 5 runs min, `t1q=125` against `t1q=127` (same binary): **1657 -> 1814 Mcyc, 5027 -> 5338 Minstr**. Per-program counters (`RUSTYNAIL_JIT_VERBOSE=1`): with the bit off the `MODK` of `percolate_up` (line 22, `index % 2` on a float) exits 208938 times at 2599 cycles per stay (543 Mcyc, a third of the run); the stays are what the rearm machinery (R1) feeds on: `rearm: armed 124686 entered 105 missed 75533 restored 75638` with the bit off against `armed 131 entered 29 missed 2110 restored 2139` with it on, trace entries 3.49 M -> 3.81 M, trace exits 9.58 M -> 14.82 M, traces installed 22 -> 19, side traces installed 160 -> 183 (recorded 317 -> 283, failed 269 -> 188). So the helper removes the exits, the rearm tick that used to be armed by them is no longer armed (in tier-1 code the same loop runs without a trace entry), the entry gate that the rearm used to open stays shut, and the program runs in tier 1 + the interpreter-side remainder more than before: the +280 M instructions are not the helper (it executes ~209 k times) but the missing entries. This is the same mechanism as the `armm` result (a tier-1 continuation that keeps a loop out of its trace), and the same cure applies in principle (tick the rearm counter from the tier-1 `MODK` helper; or an inline integral-float fast path with an exit for the rest). **Not built, bit stays OFF:** cheap retest `RUSTYNAIL_JIT_TRACE_LIMITS=t1q=127` (`bq_mod` matrix and six mutants exist).
