# Gates and benchmark tooling

Tooling for verifying and measuring a change quickly without lowering coverage. Machine of the numbers below: 16 logical CPUs (Ryzen AI 7 350,
SMT), 30 GB RAM, base commit 994d210. Every script runs each step in a `systemd-run --user --scope` with `MemoryMax`, no swap, a timeout and
`</dev/null` (CLAUDE.md memory-safety rules) and, for the wide parallel steps, a CPU set (`AllowedCPUs`, default `2-15`: the desktop keeps
CPUs 0-1). A step waits while the load average is above 24 (`GATE_LOAD_MAX`).

## Tiers

| Script | When | What | Wall time |
|---|---|---|---|
| `scripts/gate-light.sh [pattern]` | every commit | fmt, clippy (5.5, 5.5 rn-jit with the test features, 5.4), rn-jit `--features trace` suite on 5.5 and 5.4, `vm_corpus` + rn-cli tests, jit-diff 1-50 (3 modes), mutant groups matching `pattern` | 5.4 min (324 s: suites 137 s + 132 s, vm_corpus 45 s) |
| `scripts/gate-heavy.sh` | before merge / push | light + oracle-feature suite 5.5 and 5.4 + `oracle_options` + jit-diff 1-200 (7 modes) + all 723 mutants | 66 min (3964 s) |

```
scripts/gate-light.sh [MUTANT_PATTERN[,..]] [--no-54] [--seeds 1-50]
scripts/gate-heavy.sh [--no-54] [--seeds 1-200] [--skip-mutants] [--skip-oracle]
scripts/gate-oracle.sh [--tests oracle_options,oracle_exits,oracle_trace_modes] [--shards N] [--workers W] [--version 5.4]
scripts/gate-mutants.sh [--only PATTERN] [--shards N] [--version 5.4]
```

Each prints one line per step (`PASS` / `FAIL` / `KNOWN`, wall seconds, log path) and a summary table; logs go to `target/gate/<gate>-<time>/`.
Exit status 0 means no `FAIL`. Common knobs (scripts/gate-lib.sh): `GATE_CPUS`, `GATE_MEM` (8G), `GATE_MEM_BIG` (12G, the wide steps),
`GATE_THREADS` (test threads per binary, 4), `GATE_ONLY` / `GATE_SKIP` (comma lists of step names), `GATE_P8A_JOBS` (workers inside the oracle/identity
tests: 4 in the trace suite, 3 in the oracle suite), `GATE_BIN_PAR` / `GATE_THREADS_BIG` (test binaries at a time, threads of the big ones), `GATE_LOAD_MAX`.

`ref/` (the reference Lua builds, untracked) is symlinked into a worktree from the main checkout automatically for `vm_corpus`.

### What is where

- Light (per commit): `cargo fmt --check`; clippy 5.5 (workspace, all targets) and 5.5 `rn-jit --features tr_mutants,t15_oracle` (compiles the
  gated tests), clippy 5.4; the rn-jit suite with `--features trace` (the shipped code) on 5.5 and 5.4 (`--no-fail-fast`); `vm_corpus` and the
  rn-cli tests (`cargo test --release -p rustynail -p rn-cli`, against the reference build); `jit-diff` seeds 1-50, modes `off tr trdef`; and,
  with a pattern argument, the mutant groups whose name contains it (`gate-light.sh cofuse_,amt_`: `TR_MUT_ONLY`).
- Heavy (before merge / push) = light + the rn-jit suite with `--features tr_mutants,t15_oracle` on 5.5 and 5.4 (this is where
  `tr_identity`, `t15_oracle`, `instruction_count_identity` and the oracle-gated `tr_core` tests run) + `oracle_options` sharded + `jit-diff`
  1-200 with the seven modes `off tr tron trdef trdefon trverify trstorm` (`JITDIFF_ENTERS=1`) + the full mutant runner (723 mutants).
- Not in any gate: `oracle_exits` and `oracle_trace_modes` (`ORACLE_TESTS=oracle_options,oracle_exits,oracle_trace_modes` or `--tests` adds
  them: same cost as `oracle_options`), `tr_core::mutants_are_caught_by_live_programs` (one test, over 30 minutes, contains the documented
  `rolled_no_writeback` hang; every one of its mutants is covered by the sharded runner), `scripts/asan-jit.sh`, GC-stress / hook sweeps,
  the official suite, fuzzing (release checklist, docs/RELEASE.md).

### Known failures (scripts/gate-known.txt)

A step whose failures all match the list is `KNOWN`, anything else `FAIL`. Classes: `oracle` (job labels of `tr_oracle`:
`tests/vm/table_insert_layout`, `core/bb_tset`: the program output depends on the load address / path, they pass or fail from run to run),
`test` (`tr_identity_pre::fetch_plus_insns_identity`, identity gaps on `tests/vm/tables_15_alimit_big_moves`: same cause), `mutant`
(documented survivors; none now). A failing shard without the oracle's own failure report, a crash, a build error or a failing test that is
not listed is always `FAIL`. The `inconclusive` jobs of the oracle tests (leash timeouts of four programs, docs/JIT_TESTING.md) are not
failures, as in the test.

## Measured timings (994d210, this machine)

"Before" = the way the earlier builders ran each item (one job at a time, `cargo test` binaries one after another, 3 oracle workers, 3 mutant
shards); every number is a measured wall time except where it says "documented".

| Item | Before | After |
|---|---|---|
| `cargo fmt --check` | 1 s | 1-3 s |
| clippy workspace (5.5 / 5.4, warm) | 9 s / n.m. | 1-2 s / 2 s |
| cold release `rn-cli`, cold test build | 31 s, 29-30 s | unchanged (builds are not the cost) |
| rn-jit `trace` suite, 5.5 (58 binaries) | 169 s (sum; `tr_core` 152 s of it) | 137 s |
| rn-jit `trace` suite, 5.4 | 144 s | 132 s |
| oracle-feature suite 5.5 (`tr_mutants,t15_oracle`, without the sharded long tests) | 948 s (685 s without `tr_core` + 263 s `tr_core`; `tr_identity` 430 s, `instruction_count_identity` 270 s, `t15_oracle` 170 s) | 575 s |
| oracle-feature suite 5.4 | not run per merge | 562 s (see the 5.4 known failure below) |
| `tr_core::mutants_are_caught_by_live_programs` | over 31 min, killed (contains the documented `rolled_no_writeback` hang) | left out of the gates; covered by the runner |
| `vm_corpus` + rn-cli tests (release) | 86 s (19 s build) | 45 s (warm) |
| jit-diff 1..40, 7 modes | 3 s | 3 s (1..200: 14 s) |
| `oracle_options` (3714 jobs) | 2035 s (3 workers) | 604 s (14 workers; 601 / 652 / 696 s in the other configurations) |
| mutant runner, all 723 | 50 min documented (3 shards x 2 x 2), 3000 s | 1852 s (1 process, 7 mutants x 2 workers, longest first; 2057 s unordered) |
| `scripts/asan-jit.sh` (not in a gate) | 1242 s measured (one run, unchanged; load 1-3 for most of it: the corpus sections are serial) | unchanged; the `tr_*` step of that run ended with `Terminated` (SIGTERM from outside the script, cause not found) after the official suites, the corpus differentials, the stack-move stress, the `rn-jit --features jit` tests and `t15_stress` had passed |
| full gate, all of the above in sequence | about 2 h 05 min (sum) | 66 min (3964 s) |
| light-equivalent (fmt, clippy, trace 5.5 + 5.4, vm_corpus, jit-diff) | about 7 min | 5.4 min |

Peak memory of the scopes (cgroup `memory.peak`): `oracle_options` with 14 workers 1.6 GB, the full mutant runner 1.3 GB, `cargo` test
builds 4.9 GB. The first versions of the parallel suite runner (4 binaries at once, 12 test threads each, pools of 6) reached load 45-50 on the
14 CPUs and made the 300 s leashes of the oracle jobs unreliable; the shipped settings keep the load average at 14-25 on the CPU set (2 CPUs stay
idle for the desktop). `tests-5.5-oracle` took 681 s with pools of 2 (load 27), 542 s with pools of 4 (load 26) and 575 s with 3 (load 23).

### Known failures found by the first full run (base commit 994d210)

- 5.4 only: `tr_core::instruction_count_identity`, 1 of 11872 identity jobs (`r1_live_flip [on2 budget=200]`, IDENT off=1614019 vs insns=1456470),
  deterministic (reproduced twice alone on a quiet machine); passes on 5.5. Listed as `test54` in `scripts/gate-known.txt`.
- 4 survivors of the 720 mutants (`b2hk_lea_off`, `rec_push_callstatus_off`, `rec_push_top_off`, `rec_ret_cframe_skip`), reproduced alone
  (their detectors pass with the mutant active); `rec_push_top_off` is the documented weak detector. Listed as `mutant`. Neither was investigated.
- `tr_identity_pre::fetch_plus_insns_identity` (documented base failure).
These are reported as KNOWN so that the gate can be used; they are not endorsed: the first two are failures the owner has to look at.

### What could not be made faster, and why

- Builds (30 s cold, 1-8 s incremental): see Builds; the tools tried do not help.
- `oracle_options`: 16 jobs always run into the 300 s leash (documented `inconclusive`), so two leash rounds (600 s with 14 workers) are the
  floor unless the leash is shortened, which would also give up jobs that finish late on a slow machine. They are scheduled first, so the
  rest (about 1300 worker-seconds) hides behind them.
- Mutant runner: 31 of the 720 units are leash-bound (a mutated trace hangs the detecting test until its 100-300 s timeout: 8414 of the
  10759 unit-seconds). Shortening the timeouts would turn slow-but-passing runs into false "caught"; more parallel shards did not help (31
  mutants: 429 s with 3 static shards, 437 s with 6: the slow ones land in a few shards); the baseline phase ends only with `instruction_count_identity`
  (270 s). `TR_MUT_STOP_AT_FIRST=1` (skip the other detectors of a caught mutant) is available but off: it changes what the run reports.
- `tr_core` is CPU-bound (220 tests, the matrices are the cost): the light gate cannot get below about 2 x 135 s for the two versions.
- `asan-jit.sh`: 20 minutes, mostly serial sections (official suite x2, corpus x8, stack-move stress). Its sections are independent and could run concurrently under one cap; not done (the script is the sanitizer reference and not part of the per-merge gates).
- jit-diff, fmt, clippy: already seconds.

## Sharding

- `tr_oracle`: the test already had a worker pool (`TR_P8A_JOBS`, default 3). `gate-oracle.sh` runs the same job set with one worker per CPU of
  the set (and optionally in `--shards N` processes with `TR_P8A_SHARD=i/N`, new, job index modulo N), all inside one scope, then aggregates the
  failing job labels of all shards. 3714 jobs in both. The jobs that always run into the 300 s leash are now scheduled first (test-only change:
  same jobs, same checks), so the floor of the sharded run is two leash rounds (about 600 s with 14 workers) instead of the sum.
  Equivalence check: 3714 jobs in the unsharded baseline (3 workers, the original binary), in 1 shard x 14 workers and in 3 shards x 4 workers; 16 `inconclusive` (leash) jobs each time; no unclassified failure. The two known ASLR-dependent jobs (`tests/vm/table_insert_layout`, `core/bb_tset`) pass in some runs (the unsharded baseline, the second sharded run) and mismatch in others (first sharded run: one, 3-shard run: both), which is why they are KNOWN. Injected mutants (`RUSTYNAIL_JIT_MUT`, corpus subset `core/inline`, 36 jobs): `snap_drop_last` 5 failing jobs, `refund_off_by_one` 25, `land_pre_mid_swap` 30, `budget_no_check` 23, with identical label sets in the original unsharded run, `gate-oracle.sh --shards 1` and `--shards 3`.
- Mutant runner: `TR_MUT_SHARD` / `TR_MUT_RESULTS` already existed. `gate-mutants.sh` builds once and runs ONE process whose pool has
  `MUT_JOBS` = CPUs/2 mutants in flight (each test binary with `MUT_CORE_JOBS=2` workers) inside one capped scope: dynamic balancing beats the
  static `i mod n` shards (`--shards N` still works: the aggregation reads every shard's result file). The result file got a 4th column (seconds
  per mutant, `TR_MUT_RESULTS`), and `scripts/gate-mutant-times.tsv` (written by `MUT_SAVE_TIMES=1` after a full run) feeds `TR_MUT_ORDER` so
  that the leash-bound mutants start first (2057 s -> 1852 s). Refresh the times file after a full run when the registry changes.
- jit-diff: already parallel (`JOBS`); the gates run it with `JOBS` = CPUs of the set (cap lifted with `JITDIFF_ALLOW_MANY=1`, the outer scope
  is the cap) and `JITDIFF_CAPPED=1` so that its own scope does not drop the CPU set. A 40-seed 7-mode run takes 3 s: it is not a bottleneck.
- `tr_identity` uses the same pool (`TR_P8A_JOBS`), set by the gates. `scripts/gate-runbins.sh` runs the test binaries of a build 3 at a time
  (cargo runs them one after another), the big ones (`tr_core`, `tr_identity`, `t15_oracle`) with more test threads, then prints their output in
  cargo's format so that the KNOWN classification (`gate_known_test_failures`) reads it.

## Builds

Cold builds are short on this machine (release `rn-cli` 31 s with fat LTO, test builds 29-30 s, incremental rebuilds 1-8 s), so build tooling
saves little; see the numbers in the section below. `scripts/fast-build-env.sh` (sourced by the gate scripts, never by the release scripts) uses
mold as the linker (`scripts/ld-fast.sh`, falls back to the plain driver) and `sccache` as `RUSTC_WRAPPER` when they are installed. Neither is
required. Install: `sudo pacman -S mold sccache` (Arch), `sudo apt install mold` + `cargo install sccache --locked`. `GATE_FASTLINK=0` /
`GATE_SCCACHE=0` turn them off. A shared `CARGO_TARGET_DIR` between concurrent builders is not used (unsafe: cargo's lock serialises them and
fingerprints clash across feature sets and `--cfg`).

Measured (cold = new empty target directory, `CARGO_TARGET_DIR` scratch dirs of 5 GB each, 8 GB memory cap, quiet machine):

| Build | default linker | mold (`GATE_FASTLINK=1`) |
|---|---|---|
| `cargo build --release -p rn-cli` cold | 30.1 s | 30.0 s |
| `cargo test -p rn-jit --features tr_mutants,t15_oracle --no-run` cold | 30.4 s | 30.0 s |
| same, after touching `rn-core/src/lib.rs` | 4.2 s | 12.1 s |
| same, after touching `rn-jit/src/lib.rs` | 4.4 s | 7.3 s |

mold is not faster here and is not enabled by default: rustc >= 1.90 links x86_64 Linux with its bundled `rust-lld` already (`rustc --print
link-args` shows `-fuse-ld=lld`), and `CARGO_TARGET_*_LINKER` replaces that with the gcc driver + mold. Where the 30 s go (`cargo build --timings`):
the workspace crates take 40.6 CPU-seconds (the fat-LTO link of `rn-cli` 14 s, `rn-jit` 13 s, `rn-core` 4 s, `rn-std` 4 s), the 10 registry crates 1.2
CPU-seconds. sccache keys workspace crates by absolute path (a new worktree never hits) and registry crates are 3% of the cost, so it cannot
help a cold worktree build here (it is not installed; the hook in `fast-build-env.sh` is inert without it). Incremental rebuilds are 1-8 s. The
build side is not where gate time goes; every scratch target directory is 5 GB, so `rm -rf target-x* target-54` of finished worktrees matters more.

## Benchmarks

### Where the time of a full benchmark goes (session-nbody, one subject, 5.5 + 5.4, plain + PGO)

| Phase | Wall time | Work |
|---|---|---|
| builds (`release-build.sh`, 4 subject builds + 2 nojit + the rebuilt baseline 108b935 plain/PGO) | 6 min | plain 28 s, PGO 87-90 s each (instrumented build + training + final build: not 10 min), nojit 11 s |
| pass A (5.5, 5 workers on CPUs 3-7) | 26 min | 223 programs x 10 columns (PUC, LuaJIT, LuaJIT -joff, plain, plain dup, JIT off, nojit, PGO, 108b935 plain/PGO) x 5 runs |
| pass B (5.5, 2 workers) | 30 min | 6 columns again (PUC, LuaJIT, plain, PGO, 108b935 plain/PGO) |
| pass C (5.4, 2 workers) | 56 min | 9 columns (PUC 5.4, PUC 5.5, LuaJIT, p54, g54, JIT off, nojit, p55, g55) |
| re-runs of noisy / changed programs | 27 + 10 min | 33 + 19 programs x 15 serial runs on CPU 4 |
| path-length sweep, startup / size / hyperfine | 10 + 7 min | 6 programs x 4 environment paddings x 11 runs |
| total timed passes | about 2 h 50 min | A + B + C = 302 core-minutes |

What does not change between sessions is more than half of it: PUC, LuaJIT, LuaJIT -joff, the rebuilt 108b935 (plain and PGO) and the
older 9a9b8ed / 8a18eb3 columns are 6 of 10 columns of pass A, 4 of 6 of pass B and 3 of 9 of pass C. Pass B repeats pass A on fewer workers.

### What is cached, and how

`bench/results/baselines/` holds 18 columns (PUC 5.5 and 5.4 seed-0 builds, LuaJIT 2.1 and `-joff`, 108b935 plain and PGO (rebuilt),
9a9b8ed and 8a18eb3 plain and PGO (session-final numbers), and the previous subject 1a5aad0: 5.5 plain / PGO / JIT off / nojit and the
four 5.4 columns), imported from the stored session JSONs with `scripts/bench_fast.py import` (per program: every cycle and instruction
reading, so min and median stay available; sessions' extra 15-run readings are merged in). `manifest.json` has one entry per column keyed
`<id>|<subject commit or binary sha256>|<machine id>` with the source file, the command at measurement time (the environment length matters:
layout effects of up to 14% were measured with the environment / path length), the binary sha256 when the binary exists, and the machine
(CPU model, CPU count, kernel, governor). `bench_fast.py status` lists them.

`scripts/bench-fast.sh [--matrix 55plain,54plain,...] [--bindir DIR] [--cpus 4]` measures only the subject columns:

1. A canary re-measures 3 programs with the cached columns that still have a runnable command (PUC, LuaJIT; min of 5 runs) and stops
   when the median drift is above 2.5% or the worst above 6% (the machine is not in the state of the cached runs). A cached column of
   another machine, or whose binary changed since it was measured, is refused.
2. Every program gets 2 interleaved runs of each subject column (the first run counts the instructions, deterministic to better than
   0.001%; the second gives the cycle spread). Subject columns run with an environment padded to the length of the cached columns.
3. The program gets the full 5 runs (3 more, interleaved the same way) when a subject column's instruction count differs by more than 0.5%
   from the paired cached column (by default the previous subject, `PREV_COMMIT`), or the spread of its cycles in the 2 runs is above 2%
   (`INS_TOL`, `CV_TOL`), or `--all-full`. Others keep min of 2 (the JSON has `n` and `flag` per column; min of 2 is up to about 1% above
   min of 5). Output: the schema of `bench_session.py` (cached columns copied in, marked `cached`), so the existing report scripts work.
4. `--matrix` trims the passes: `54plain` alone is 2.6 core-minutes, `55plain,55pgo` 5.7.

Expected cost of a full 5.5 + 5.4 run (8 subject columns, one pass instead of A + B + C), from the session-nbody readings: 62 core-minutes when
a quarter of the programs need the full runs, 79 when half do (302 before); on one CPU 62-79 minutes, on 3 CPUs 21-26 (if parallel runs are
allowed, below). Builds do not shrink (6 min, but the 108b935 pair, 2 of 7 builds, is not needed: 2 min).

The default full-benchmark invocation (`scripts/bench_session.py` as `run.sh` of session-nbody drives it) is unchanged and stays the
reference method; `--all-full` of the fast driver does the same measurement for the subject columns with cached baselines.

### Running it, and refreshing the cache

```
# builds of the subject (as in session-nbody), staged in equal-length directories
scripts/release-build.sh --no-pgo --out /tmp/x55p ; scripts/release-build.sh --out /tmp/x55g   # + --lua54, + --no-default-features builds for the nojit columns
mkdir -p ~/.cache/rnnb/bin/{p55,g55,n55,p54,g54,n54}; cp /tmp/x55p/rnlua ~/.cache/rnnb/bin/p55/rnlua   # ... one per column
scripts/bench-fast.sh --matrix 55plain,55pgo                    # whole matrix by default; --all-full for min-of-5 everywhere
python3 scripts/bench_fast.py status                            # what is cached, on which machine
# after a session whose numbers are accepted, make them the next "previous subject" (and keep the old ones):
python3 scripts/bench_fast.py import --from bench/results/<session>/passA....json --col plain --id p55-<commit> --commit <commit> --desc "..."
python3 scripts/bench_fast.py parcheck --cpus 3,4,5,6,7 --cmd "env RUSTYNAIL_SEED=0 ~/.cache/rnnb/bin/p55/rnlua"   # parallel-worker check
```

`PREV_COMMIT=<commit>` selects which imported subject the instruction counts are compared with. A column imported from another machine, or
whose binary changed, is refused: re-measure it with the full method (`bench_session.py`, the unchanged reference) and import that.

### Parallel pinned workers: measured, off by default

`bench_fast.py parcheck` runs the same programs on one pinned CPU and then on N CPUs at once, with every SMT sibling idle (checked: all CPUs
and siblings below 5% busy), 6 programs, min of 9: the single-worker noise floor (the same measurement twice) was 0.48% max; with 2 workers
the max difference was 1.41%, 3 workers 1.35%, 4 workers 2.12%, 5 workers 2.44% (the parallel cycles are systematically 0.2-2.4% lower, e.g.
awfy/richards -1.4%, lib/sel_loop -2.4% at 5 workers). The criterion was 1%: not met even for 2 workers, so `bench-fast.sh` uses one worker
(`bench/results/baselines/parallel-check.json` records `proven_workers: 1`; more CPUs are cut back unless `PAR_FORCE=1`). The earlier sessions'
pass A (5 workers) numbers therefore carry up to about 2% parallel bias that the single-core re-runs partly correct.

### Reproduction check (3 programs, this machine, subject = this worktree's release build)

Same binary, `bench_session.py` as in session-nbody (full mode: 5 runs, CPU 4, `SPIN_MS=300`) against `bench-fast.sh --matrix 55plain`:

| program | plain cycles full mode | plain cycles fast mode (runs) | ratio | instructions full / fast |
|---|---|---|---|---|
| awfy/richards | 59.60 M | 59.48 M (5, flagged) | 0.9980 | 183,011,679 / 183,016,389 |
| corpus/class_middleclass | 491.97 M | 491.85 M (5, flagged) | 0.9998 | 2,313,274,052 / 2,313,277,363 |
| programs/nbody | 345.93 M | 345.98 M (2, quick only) | 1.0001 | 2,028,254,372 / 2,028,259,055 |

Instruction counts agree to 3e-6 (the counter itself moves by a few thousand), cycles to 0.2%. The cache against a fresh measurement today
(108b935 rebuilt from a git archive: 2,976,768 bytes, the size recorded in session-nbody; PUC and LuaJIT as installed):

| program | 108b935 plain: now / cached | PUC 5.5: now / cached | LuaJIT: now / cached |
|---|---|---|---|
| awfy/richards | 1.0004 | 0.9999 | 1.0054 |
| corpus/class_middleclass | 0.9960 | 0.9897 | 1.0007 |
| programs/nbody | 0.9990 | 1.0257 | 0.9941 |

(cycles ratios; 108b935 instruction counts equal to the cached ones within 3e-6; the canary of `bench-fast.sh` over 9 PUC / LuaJIT points: median drift 0.66%, worst 2.4%, with a
background load present.) Time: the three programs take 5 s in fast mode.
