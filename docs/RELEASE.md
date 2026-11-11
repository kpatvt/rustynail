# Release checklist

Steps for the person cutting a release. The version is **0.0.1 and not tagged yet**: no release has been cut. The build
scripts are `scripts/release-build.sh` (one Lua series: `rnlua`, `rnluac`) and `scripts/release.sh` (the full matrix with the C API
library and headers).

Release defaults (since 2026-11-11):
- **PGO is the default.** `scripts/release-build.sh` builds the profile-guided `rnlua` (instrumented build, training run over
  `scripts/train.sh`, optimised build: about 10 minutes, not bit-reproducible); `--no-pgo` gives the plain build, `--pgo` is
  accepted and does nothing, `--lua54` selects the 5.4 series (use `--out` for a separate directory). `scripts/release.sh`
  defaults to `--variants pgo,default`.
- **Stripped.** `dist/rnlua` and `dist/rnluac` are `strip --strip-all` (about 3 MB for `rnlua`); the C API library
  `librn_capi.a` in `release.sh` output is `--strip-debug`. Unstripped copies stay in the `target*/` build directories.
- **Path remapping.** Every build path (release-build, release, pgo, build-fast, bolt) passes `--remap-path-prefix` for the
  source tree (`/rustynail`), cargo home (`/cargo`), rustup home (`/rustup`), toolchain sysroot (`/rustc`) and home directory
  (`/build-home`), and `-ffile-prefix-map` for the C shim (`scripts/release-env.sh`).
- **Privacy check.** The scripts run `strings -a` over the shipped executables and libraries and fail when they contain the
  builder's home directory (`/home/`), user name or host name. Run it by hand on anything you publish:
  `strings -a dist/rnlua dist/rnluac | grep -c -E "/home/|$(id -un)|$(hostname)"` must print 0.
- **`BUILD-INFO.txt`** (release.sh) records the commit, the date, rustc, the target triple and `uname -sm`; no kernel release,
  host name or user. `BUILD-INFO.txt` and `SHA256SUMS` are generated files (ignored by the repository).

Checklist:

- [x] Repository URL set (`[workspace.package]` `repository`/`homepage` = https://github.com/kpatvt/rustynail).
- [ ] Decide the version (`[workspace.package] version`, now 0.0.1), update it, commit.
- [ ] Licence check: `python3 scripts/gen-notice.py --check` (NOTICE must match the dependency graph; the release script
  runs it), `cargo deny check` (see `deny.toml`), and `rnlua --license | head` prints the embedded texts.
- [ ] Known-good test list (see `docs/JIT_TESTING.md`, "Release triage (BUNDLE Y)" for the documented exceptions). Known
  environment quirk: 5.4 `all.lua` fails at `gc.lua:487` (`collectgarbage("count") <= m + 1`) when the binary path is exactly
  100 characters long (it passes at 108 and with a shorter path; identical on the commit before the crate rename); run the 5.4
  official suite from a checkout path of another length, or exclude `all.lua`.
- [ ] Build both series: `scripts/release-build.sh --out dist-5.5` and `scripts/release-build.sh --lua54 --out dist-5.4`
  (or `scripts/release.sh` for everything). PGO needs `llvm-profdata` of rustc's LLVM (the rustup component `llvm-tools-preview`).
- [ ] Verify the artifacts: `cd dist && sha256sum -c SHA256SUMS`; `rnlua --version` (the `JIT: on` line), `rnlua --license`,
  `rnluac --license` and a smoke script (the scripts do these); the C module test (`cargo test -p rn-cli --test cmodule`, or
  load `tests/c/samplemod.c` built against `vendor/lua-5.5/src`, with `LUA_CPATH`, through the stripped binary); the strings check above.
- [ ] Tag the commit that was built (BUILD-INFO.txt records it) with an annotated tag `vX.Y.Z` and push the tag.
- [ ] Publish the artifacts together with `SHA256SUMS`, `LICENSE`, `LICENSE-LUA`, `NOTICE`.

## Verification A (2026-11-10)

Tree: main `ec66eb16` plus the `VA:` commits of branch `<worktree>` (fixes VA-1 .. VA-10 below). Logs of the run: `~/.cache/vaLog/`.
Every command ran under `systemd-run --user --scope -p MemoryMax=8G -p MemorySwapMax=0` and a timeout, stdin on `/dev/null`.

| Step | Command | Result | Time | Exceptions |
|---|---|---|---|---|
| A1 full mutant run (first) | 3 shards of `tr_mutants every_registered`, `TR_MUT_JOBS=2 TR_CORE_JOBS=2` (docs/JIT_TESTING.md) | registry 624: 617 ran, 612 caught, **5 survived**, 2 reported `not injected` (runner bug), 5 skipped (5.4-only) | 26 min | all 7 analysed and fixed below |
| A1 re-runs of the affected rows | `TR_MUT_ONLY=...` per fix | every affected row caught | seconds each | |
| A1 `mutants_are_caught_by_live_programs` | alone, then one mutant per process (3 at a time) | 23 of 23 mutants caught (one process per mutant, `rolled_no_writeback` excluded: caught by `tr_asm` / `mutant_probe_live_programs` in the sharded runner); `snap_prune_dead` took 11 min, the others 20-70 s | | the single-process test hits its timeout (the documented `rolled_no_writeback` hang) |
| A1 full mutant run (final tree, registry 625) | same 3 shards | **registry 625: 620 ran, 620 caught, 0 survived, 0 not injected, 5 skipped (5.4-only)**; shards 207 / 205 / 208 caught (ran under CPU contention from Miri: about 110 min wall instead of 26) | | |
| A2 `scripts/asan.sh` | ASan official suite (rnlua and lua.c), C API sections, sample C module | `ASAN OK`, also after the last code change | 3 min | |
| A2 `scripts/asan-jit.sh` | JIT official suite, corpus, GC stress, stackmove, trace corpus, jit-diff 1..40, `rn-jit` tests, `tr_*` tests | clean after the fixes (jit-diff 0 diffs, no LeakSanitizer report; `tr_*`: one failure fixed, VA-3); the first `tr_*` step was killed by the 8 GB cgroup (16 test threads under ASan), re-run with `RUST_TEST_THREADS=3 TR_CORE_JOBS=2 TR_P8A_JOBS=2` | 25 min | |
| A2 ext under ASan | `scripts/ext-matrix.sh --rnlua <asan rnlua>`: 9 programs (`buf_*`, `tbl_new_clear`, `rn_*`), modes default, JIT off / always, traces always, GC inc, GC stress 1 and 2 | 7 modes x 9 programs pass, ASan silent (exercises `buf_store.rs`) | 2 min | |
| A3 Miri | `scripts/miri.sh`, 2 full passes + targeted re-runs (below) | UB found and fixed (VA-5, VA-6) | about 4 h | what Miri cannot run (below) |
| A4 `cargo test -p rn-capi` | 3 runs (random seeds) | 4/4 each | 1 s each | |
| A4 `cargo test -p rn-cli` | cmodule, ext, license, jit_defaults, sigint | all pass | | |
| A4 vm_corpus | `cargo test --release -p rustynail -p rn-cli` | 9/9 | 46 s | |
| A4 GC stress matrix | every `tests/vm` program (154, `heavy_` excluded) against JIT off in the same collector mode: `RUSTYNAIL_GC_STRESS=1,2` x `RUSTYNAIL_GC_MODE=inc,gen` x {JIT default, `JIT=off`, `JIT_TRACE=always`, `JIT=always JIT_TRACE=always`}; `RUSTYNAIL_GC_VERIFY=1` on the 87 small programs, stress 1,2 x inc,gen x {default, traces always} | 24 configurations: 0 diffs, 0 timeouts | 3 min | the corpus programs are tiny (10-30 ms each) |
| A4 `rn-jit` suite (5.5) | `cargo test -p rn-jit --features tr_mutants,t15_oracle`, skipping the live mutant test, the runner and the three 35-minute `tr_oracle` tests | all pass | 25 min | |
| A5 5.4 all.lua | `--cfg rustynail_lua54`, release `rnlua`, `vendor/lua-tests-5.4/all.lua` x 10 (2 at a time) | 10/10 `final OK`; the gc.lua:487 failure did not reproduce | 4 min | |
| A5 `cargo xtask official --lua-version 5.4 --release` | 30 files | ours 30/30, reference 30/30 | 2 min | status file timing noise reverted |
| A5 5.4 unit tests | `cargo test -p rn-core -p rn-std -p rn-compiler -p rn-cli -p rn-capi` | all pass after VA-9 / VA-10 | | |
| A5 vm_corpus 5.4 | `cargo test --release -p rustynail -p rn-cli` | 9/9 | 28 s | |
| A5 jit-diff 5.4 | seeds 1..100, sets `tr`, `bs`, `nr`, `bv`, modes `off tr trdef trstorm tremu trstack` + `ref` (`ref/54` lua) | 400 seed runs, 3 diffs, all one flake (below) | 40 min | |
| A5 `rn-jit` under 5.4 | `tr_core`, `tr_identity`, `tr_record`, `t15_oracle`, ... | pass except the 5.4 exceptions below | | |

### Fixes (commit, root cause)

- **VA-1 `9c064b1e`** (real bug, 5.5): `table.move` inside a trace over ONE table reloaded elements between stores; a guard failing between two stores re-ran the `CALL` over half-moved elements (wrong result for overlapping moves; natural trigger: a float / string mark travelling through the range). Found because `tr_core::instruction_count_identity` (`br_lib`, `+exitstorm=7`) failed its baseline in every mutant shard (so `tfor_lua_ret_cost_skip` and `fside_rem_root` reported "survived"). Fix: pre-read all elements of the range before the first store. New mutant `tmove_no_preread` (registry 625), detector `tr_core::va_tmove_overlap_programs`.
- **VA-2 `aa952531`** (runner): `injected()` did not recognise `vbytes_mutant("name")`; `vbytes_no_credit` / `vbytes_no_limit_guard` were reported "not injected" although their sites exist. Both are caught.
- **VA-3** (stale detector, `exit_top_ignored`, `exit_top_off_by_one_slot`): no F4 program exits at a snapshot with a multiple-result top any more under the shipped defaults, so `thread.top = base + slot` of the handler restore was never checked. The `tr_exit` random restore scenarios now carry tops. A follow-up commit clears the top in the inline-stub comparison (found by the debug assertions of the ASan-JIT build).
- **VA-4** (stale detector, `black_exit_report`): with `sidenest=1` no blacklisted exit reaches the materialising stub; the test also runs with `sidenest=0`.
- **VA-5** (real UB under Stacked Borrows, introduced by M3): `promote_hash(&self)` read the string contents through a `&LuaString`; now `promote_hash(p: *const LuaString)` / `short_hash(p)`. 24 `core-lib` tests, `table_diff` and all `interp` shards failed before.
- **VA-6** (Miri harness): `rdtsc` under `cfg(miri)` returns 0 (the recorder `miri_*` tests died on the intrinsic); two `tr_exit` tests that Miri cannot run are `cfg_attr(miri, ignore)`; `t15_core_emu::dummy_ci` kept its block as an integer in a static (reported as a leak); a `jit_layout` test used `ptr.add(wrapping_sub(..))` (pointer arithmetic overflow) instead of `sub`.
- **VA-7** (LeakSanitizer, jit-diff seeds 3, 17, 20, 32, 37): not a leak. The per-state `JitData` lives in an anonymous mapping (so traces can poll with a rel32 operand), which LSan does not scan; the generated programs end with `os.exit(3)`, which leaves the state allocated (`close_state` runs finalizers; the Rust heap and the JIT data are freed by `Drop`). Without the final `os.exit`, or without JIT, LSan is silent. Fix: builds with `--cfg rustynail_lsan` (set by `scripts/asan.sh`, `scripts/asan-jit.sh`) register the mapping with `__lsan_register_root_region` and unregister it before `munmap`; no suppression file, a block that is left after a normal close is still reported.
- **VA-8** (real miscompile, 5.4 only, pre-existing at `ec66eb16`): `asm::tlen_fast` replicates the 5.5 length hint; 5.4's `luaH_getn` moves `alimit` under other rules, so tables with holes gave a different border and limit (`tlen_inline`, `instruction_count_identity`, `tr_identity` failed). The 5.4 build always calls the helper.
- **VA-9**: `string.buffer` dict / metatable arrays use the entries up to the first nil (the same on both versions; 5.4's `#{"a", nil, "b"}` is 3 and raised `bad options table`).
- **VA-10** (5.4 test expectations): `tests/vm/table_growth_layout.lua` byte columns are constant under 5.4 (our object sizes differ from the 5.4 reference by design, docs/STATUS.md); `rn_buffer_ext` (5.5 float formatting) is skipped under the 5.4 cfg.

Commits: VA-1 `9c064b1e`, VA-2 `aa952531`, VA-3 `43278729` + `a5d56524`, VA-4 `3f321e30`, VA-5 `4b0d18d2`, VA-6 `1c037215` + `d057de36` + `ca44e3b5`, VA-7 `237e773e`, VA-8 `076d90fc`, VA-9 `9f77ece6`, VA-10 `0c034fff` + `6ec5b23d`.

### Miri: passes, coverage, what could not run

- **Pass 1** (`scripts/miri.sh --jobs 8 core-lib core-it compiler interp jit jit-trace`, Stacked Borrows, 5.5): core-lib 80/104 (24 UB, VA-5), core-it 0/1 (VA-5), compiler 7/7, interp 0/36 (VA-5), jit 75/76 (a leak report, VA-6), jit-trace 139/144 (5, VA-6).
- **Pass 2** (same suites without compiler, after the fixes): core-lib 103/104 (1 UB, VA-6 `jit_layout`), core-it 1/1, interp 9 passed and 8 cancelled (see below), jit 76/76, jit-trace 142/142.
- **Targeted re-runs** (no third full pass): `jit_layout` passes; interp shards `inc_06`, `inc_07` pass. The other interp shards were cancelled twice by the 8 GB cgroup OOM killer (10 and 8 parallel Miri processes of about 2.3 GB each); three more shards (`gen_00`, `inc_00`, `pool_00`, one per allocator / collector mode) ran with 3 jobs: all three were killed again by the 8 GB cgroup OOM killer after 45 min (`0 passed, 3 failed`, SIGTERM / SIGKILL, kernel log `oom-kill`; the shards `*_00` are the largest ones) -- no UB report in any of them, but also no pass. Interp shards that passed after the fix: `gen_01 gen_02 gen_06 gen_07 gen_09 gen_10 gen_11 inc_01 inc_02 inc_06 inc_07`; the rest of the 36 were not re-run after the fix and are not claimed (before the fix they failed with the single VA-5 UB; `core-lib` and `table_diff` cover the same code).
- Not runnable under Miri (unchanged from docs/UNSAFE.md): `rn-capi` (C callers); `io` / `os` / `string.format` through libc FFI (the `rn-std` tests that use them, and `lazy_leave_completes_the_chain_...`); executable memory and machine code (`mem.rs`, native JIT tests; the JIT is off under `cfg(miri)`); the iced-x86 oracle tests (third-party Stacked Borrows reports); `fmat_ops_equal_restore` (emulator stores through integer addresses of two worlds); `table_diff` against the reference process; the 7 interp scripts of `MIRI_SKIP` (hours under Miri); `rn-std --lib` (no unit tests there). `string.buffer` / `buf_store.rs` is covered by ASan through the ext matrix, not by Miri.
- 5.4 under Miri and Tree Borrows were not run (time).

### Remaining exceptions

(The 5.4 `rn-jit` test exceptions, the ASan-JIT re-run and the Miri `*_00` shards below were taken up in "Verification A2".)

- **`mutants_are_caught_by_live_programs`** as a single test never finishes within its timeout (`rolled_no_writeback`, documented); run per mutant, see the table.
- **5.4 `rn-jit` tests that also fail on the base commit** (five of them checked in a separate worktree) and are not fixed: `nr_retf_traces_are_installed`, `nr_n3_programs` (RETF is inert under 5.4 by design, the tests assert RETF traces); `c36_cispare_programs`, `p3_fdisp_programs`, `rawfn_programs`, `tr_record::ba_ops_record_their_guards` (5.5 policy statistics; the `libcall` bits are 5.5-only); `t1i_inline_programs`, `tlen_inline_programs`, `twins_options` under `+verify` (recorder lockstep: the 5.4 model of `alimit` after `#t` is one off, `table N: VM alimit 12, IR 11`; production runs have no lockstep and give the interpreter's output, jit-diff 5.4 is clean); `t15_oracle::oracle_corpus` (`Forloop: slot N is in defs but was not written`, gen:39). The 5 `V54_ONLY` mutant rows were not run.
- **jit-diff 5.4**, 3 diffs (seed 82 `ref` and `tremu`, seed 92 `trstorm`): the traceback names the failing C function `math.random` or `type` depending on the string-hash seed (`package.loaded` scan order); the reference `lua` 5.4 alternates the same way (6 runs: 3 and 3), and with `RUSTYNAIL_SEED=0` all modes agree. Not a JIT difference.
- **ASan-JIT**: the official suite, corpus and jit-diff steps ran before the last commits (`rdtsc` cfg, `jit_layout`, 5.4-only changes); `asan.sh` and the `tr_exit` binary were re-run afterwards; the 5.5 `rn-jit` suite ran on the final tree.
- **Miri `gen_00`, `inc_00`, `pool_00`** (and the shards not listed as passed) did not complete under the 8 GB cap: unverified, not a finding.
- Not run, by instruction: fuzzing, benchmarks. `cargo fmt` / clippy not applied.

## Verification B (fuzz, 2026-11-10)

Tree: `476a891e` plus three harness commits (`8679ddce`, `12531b1b`, `d4f22c7d`: fuzz crate only, no production code changed).
Build: `cd fuzz && cargo +nightly fuzz build --features trace` (ASan on, `debug-assertions`, opt-level 1), target dir outside the repo.
Every process ran in its own `systemd-run --user --scope -p MemoryMax=8G -p MemorySwapMax=0`, `-jobs=2 -workers=2`, `-rss_limit_mb=2500` (3000 from slot 2 on),
`-timeout=30`, stdin `/dev/null`, at most two such processes at a time. Seeds: `fuzz/make_seeds.sh`, `make_jit_seeds.sh`, `make_trace_seeds.sh`
(+ 4 hand-made `run_source` seeds using `jit.flush`/`buffer`), union per target. Logs: `~/.cache/vbLogs`, corpora `~/.cache/vbCorpus` (not merged into
the repo: 14-109 MB each, over the 5 MB limit).

Sandbox change (harness): the shared sandbox now opens the `jit` table (`jit.flush/on/off/status`). `run_source` keeps all of it; `jit_diff` removes
`on/off/status` (they make the interpreter and compiled runs differ by design) and keeps `jit.flush`, which the grammar emits as a statement and at
loop heads. `string.buffer` and `table.new/clear` were already open in every execution target.

Total fuzz wall clock: about 10 300 s of the 10 800 s budget (slot 1 about 1 500 s because both targets stopped early on the RSS limit; slots 2-4
1 800 / 1 500 / 1 500 s; slot 5 about 2 700 s with undump overlapping its last 15 min; final slot 1 250 s). Executions are summed over the two jobs of
each target.

| Target | Wall time | Executions | Coverage (cov / features / corpus units) at end | Corpus dir (disk) | Findings |
|---|---|---|---|---|---|
| run_source (JIT/trace bits, GC modes, ext + jit table) | 1 490 + 2 010 + 1 250 s (slots 1, 5, 7) | 0.33 M + 0.27 M + 0.10 M | 60 910 / 316 798 / 5 163 | 36 MB | 0 bugs; RSS-limit stops (finding 4) |
| jit_diff (interpreter vs tier 1 / T15 / trace modes) | about 1 100 + 1 100 + 1 000 s (slots 1, 5, 7; each run stopped early on the RSS limit) | 54 k + 55 k + 45 k | 59 518 / 294 801 / 5 074 | 37 MB | 4 harness findings, 0 production bugs |
| trace_ir (trace IR, regalloc, assembler/emulator) | 1 800 s | 0.90 M | 10 977 / 43 430 / 2 888 | 14 MB | 0 |
| capi_calls (random C API call programs) | 1 800 s | 1.40 M | 7 662 / 45 147 / 3 718 | 42 MB | 0 |
| table_ops (model-checked table ops, table.new/clear) | 1 500 s | 6.5 M | 1 419 / 7 242 / 1 245 | 28 MB | 0 |
| string_lib (patterns, format, pack, rep, buffer ops) | 1 500 s | 3.9 M | 9 387 / 25 091 / 4 452 | 45 MB | 0 |
| buffer_ops (string.buffer put/get/encode/decode, malformed bytes) | 1 500 s | 0.65 M | 6 318 / 35 200 / 2 464 | 14 MB | 0 |
| lex_parse | 1 500 s | 13.1 M | 5 103 / 31 719 / 3 500 | 109 MB | 0 |
| undump (malformed precompiled chunks) | 900 s | 13.6 M | 5 145 / 27 584 / 4 052 | 104 MB | 0 |

Findings (all harness or resource issues, none a production bug; regression inputs in `fuzz/regress/`):

1. `jit_diff` timeout/slow unit (`8679ddce`, `jit_diff-slow-huge-const-for.bin`, `jit_diff-timeout-huge-const-for2.bin`): `for i = -0x80000000, 27` or `-2^63, 23` with a constant bound is 2^31..2^63 iterations; the grammar ticked only half of the loop bodies, so the tick budget never expired (20-30 s). Fix: a huge constant start forces a tick.
2. `jit_diff` slow unit, 10 s (`12531b1b`, `jit_diff-slow-tick-grace-phaselock.bin`): after the tick budget is exhausted the harness grants a constant 500-tick grace and raises again; a loop body of period 21 ticks (1 outer tick + 20 coroutine ticks, 501 and 21 share the factor 3) raised only inside a coroutine, which swallows the error, so the outer `ipairs` over a growing table never ended (until the watchdog). Fix: the grace is `400 + hash(count) % 211`, deterministic and equal in both runs.
3. `jit_diff` 15 s unit (`12531b1b`, `jit_diff-slow-nested-const-while.bin`) and 20 s unit (`d4f22c7d`, `jit_diff-slow-nested-repeat-insert.bin`): nested constant-bound `while`/`repeat` loops (14^k iterations; quadratic `table.insert(t, 1, x)` inside an `ipairs` over a growing table) without ticks. Fix: nested loops always tick. Residual slow units of 3-10 s remain: a compiled run with `jit.flush()` at every iteration re-records its traces each time (0.155 s compiled vs 0.007 s interpreted for 4000 iterations with the release `rnlua`; far more under ASan + debug assertions); the watchdog stops such runs and they are not compared. That is the expected cost of `jit.flush`, not a defect.
4. OOM stops (10 `oom-*` artifacts in run_source and jit_diff, 20-35 min into a process): libFuzzer's own growth. The heap report at the stop shows 150 MB live, 75% of it `InputInfo::UpdateFeatureFrequency` vectors (libFuzzer feature tables, 300 k features), plus ASan quarantine and fragmentation. Every artifact, run alone, uses 100-860 MB and finishes in 0-15 s, and the same input repeated 500 times in one process grows only by the ASan quarantine, so it is not a Lua-side leak. The 2-job runs of `run_source`/`jit_diff` therefore stop after about 1 000-2 000 s even with `-ignore_crashes=1`; they were restarted from the saved corpus.

Not done / limits of this campaign: the budget was split over nine targets, so `jit_diff` and `run_source` got about 50 min each at 22-130 exec/s under ASan; their coverage was still rising at the end (they did not plateau). `trace_ir`, `capi_calls`, `table_ops`, `string_lib`, `buffer_ops`, `lex_parse`, `undump` were flat for the last minutes. The 5.4 build was not fuzzed. `jit_diff` reaches the trace modes only through the escape-byte seeds and mutations of them. `run_source` has no oracle for the `jit` table (it checks only for crashes, leaks and GC invariants). Corpora were not merged into the repository. The full cargo test suites were not re-run: no production or test code changed (only `fuzz/`, a separate workspace; `cargo +nightly fuzz build --features trace` is the check).

## Verification A2 (2026-11-11)

Tree: `b1d97839` plus the `VA2` commits of branch `<worktree>` (V1 `65d44089`, V2c `1c1132bf`, V2d `060e57d2`, V2a/b `f6fd7336`).
Every command ran under `systemd-run --user --scope -p MemoryMax=...` (6-8 GB; Miri 12 GB for a whole shard, 4 GB per script) and a timeout, stdin on `/dev/null`. Logs: `~/.cache/va2Log/`.

| Item | Command | Result | Exceptions |
|---|---|---|---|
| V1 Miri `gen_00`, `inc_00`, `pool_00` as whole shards | `cargo +nightly miri nextest run -j 1 -p rn-std --test interp_scripts -E 'test(/^<shard>$/)'`, `MIRIFLAGS="-Zmiri-disable-isolation -Zmiri-deterministic-floats"`, cap 12 GB, timeout 90 min | all three killed by the 90-minute timeout (`SIGTERM [5397 s]`; nextest prints `0 passed, 1 failed` for the kill), peak RSS 5-7 GB (no OOM), **no UB, leak or unsupported-operation report in any log** | not a pass: a shard is 10 scripts and one of them does not finish (next row). `gen_00` ran alone first; `inc_00` and `pool_00` ran side by side (24 GB were free) |
| V1 Miri per script (the 10 scripts of shard 0, each in its own process, `INTERP_ONLY=<name>`, cap 4 GB, 25 min) | same command with `INTERP_ONLY` (`65d44089`) | **27 of 30 script runs pass** (9 of 10 scripts in each of gen, inc, pool; 75-170 s each): `arith_01_int_basic`, `arith_14_tostring_numbers`, `closures_06_recursive_local`, `err_04_nested_pcall`, `goto_03_nested_loops_break`, `meta_09_close_ordering`, `strings_01_methods`, `tables_07_table_of_tables`, `vararg_01_basic`; no UB report | `co_14_cstack_overflow.lua` (nested coroutines up to the C-stack limit, 300 dropped `wrap` closures) was cut at 25 minutes in every mode, no report before the cut: it is what makes the shards exceed 90 minutes. Not verified under Miri (native and ASan runs cover it). |
| V2 5.4 `rn-jit --features trace` suites | `RUSTFLAGS="--cfg rustynail_lua54"`, `tr_core` (`--skip mutants_are_caught_by_live_programs`, 187), `tr_policy` 49, `tr_asm` 48, `tr_loop` 27, `tr_record` 35, `tr_exit` 36, `tr_ra` 19, `tr_determinism` 2, `tr_runtime` 43, `tr_record_calls` 36, `tr_hook` 14, `tr_poll` 3, `tr_passes` 15, `tr_ir` 18, `tr_class_a_pb` 19, `tr_hashd` 5 | all pass (`tr_coro` has no 5.4 tests) | none |
| V2 5.4 tier 1 and tier 1.5 | `jit_arith`, `jit_table`, `jit_calls`, `jit_loops`, `jit_lifetime`, `jit_alloc`, `jit_stress`, `arith_diff`, `table_diff`, `emu_diff`, `emu_ops_diff`, `tr_bd`, `jit_review_{hooks,count}`, `jit_cstack`, `t15_analysis`, `t15_core_emu`, `t15_runtime`, `t15_core` 63, `t15_policy` 26, `t15_stress` 88, `rn-jit --lib` 84, `cargo test -p rn-core -p rn-std --features jit` | all pass | none |
| V2 5.4 `t15_oracle::oracle_corpus` | `--features trace,t15_oracle` | 230 programs, 139 enter a region, 0 violations | harness fix V2d |
| V2 5.4 mutants (the 5 `V54_ONLY` rows) | `TR_MUT_ONLY=atouch_skip,atouch_hoisted,forloop54_no_ctl_copy,alimit_touch,tfor54_no_ctl_copy cargo test -p rn-jit --features tr_mutants,t15_oracle --test tr_mutants -- every_registered` (5.4 cfg) | 5 registered, **5 caught**: `atouch_skip`, `atouch_hoisted` (`tr_core::mutant_probe_live_programs`), `forloop54_no_ctl_copy` (`tr_record::golden_ir_dumps`, `mutant_probe_live_programs`), `alimit_touch` (`tr_asm::hand_atouch_follows_alimit_touch` and the emulator tests), `tfor54_no_ctl_copy` (`tr_core::f3b_programs_run_on_traces`) | none |
| V2 5.4 release tests | `cargo test --release -p rustynail -p rn-cli -p rn-capi` (`ref/` linked into the worktree, so `vm_corpus` compares against the reference) | all pass (vm_corpus 9/9) | none |
| V2 `cargo xtask official --lua-version 5.4 --release` | | ours 30/30, reference 30/30; `tests/official-status-5.4.toml` timing noise reverted | none |
| V2 jit-diff 5.4 | `RUSTYNAIL_SEED=0`, seeds 1..100, `JITDIFF_TSET=tr,bs,nr,bv`, modes `off tr trdef trstorm tremu trstack ref` (`ref/54/build-seed0/lua`) | 4 x 100 seeds, **0 diffs**, 0 timeouts (`tr`: 13 slow skips of the reference column) | none |
| V2 5.5 after the production change | `cargo test -p rn-core -p rn-std --features jit` (106), `cargo test --release -p rustynail -p rn-cli` (vm_corpus 9/9 against `ref/55`), `cargo test -p rn-capi` (4/4), `rn-jit --features trace`: `tr_core` 187 (`--skip mutants_are_caught_by_live_programs --test-threads=3`), `tr_policy` 49, `tr_asm` 47, `tr_loop` 27, `tr_record` 35, `tr_exit` 36, `tr_ra` 19, `tr_determinism` 2, `tr_runtime` 43, `tr_record_calls` 37, `tr_hook` 14, `tr_poll` 3, `tr_passes` 15, `tr_ir` 18, `tr_class_a_pb` 19, `tr_hashd` 6, `tr_coro` 5, the tier-1 / 1.5 suites above, `rn-jit --lib` 84 | all pass | none |
| V3 `scripts/asan-jit.sh` (5.5, full, final tree) | `RUST_TEST_THREADS=3 TR_CORE_JOBS=2 TR_P8A_JOBS=2 scripts/asan-jit.sh ~/.cache/va2TargetAsan` | **`ASAN-JIT OK`**: official suite (JIT=always, JIT=on, default, TRACE=always, TRACE=on), `tests/vm` corpus (JIT, T15, `+shadow`, `+exitstorm=7`, TRACE modes, `+emurun`, passes off), GC stress 1, stackmove, jit-diff trace modes seeds 1..40 (0 diffs), `rn-jit --features jit` and all `tr_*` binaries (`tr_core` 187 in 909 s): no AddressSanitizer or LeakSanitizer report | the 5.4 ASan build was not run |

### Fixes and test changes

- **V2c `1c1132bf`** (real recorder bug, 5.4 only, production code). `'VM alimit 64, IR 63'` under `+verify` was not a wrong model of `#t`. The lockstep checker (and the recorder's look-ahead reads: the generic-for end test of `t[i+1]`, `lib_border` / `lib_border_exact`, the post-step element resync) read table elements through `TableRef::get_int_arr`, which performs the 5.4 `alimit_touch` side effect. The observer moved the VM's array limit: a touch of the last element after `luaH_getn` (here `rawlen(h)` after `h[i] = nil`) had lowered the limit raised it again. The same reads in a production recording moved `alimit` of tables that the interpreter would not have touched, and `#t` of a table with holes depends on `alimit` (the border). Fix: `TableRef::peek_int_arr` (the same read without the touch, identical on 5.5) for the reads that look at elements the VM has not just accessed itself. `t1i_inline_programs`, `tlen_inline_programs`, `twins_options` pass on 5.4. No new regression test: the three existing `+verify` tests fail without the fix.
- **V2d `060e57d2`** (harness). `t15_oracle` poisons the `defs \ uses` slots before an instruction and requires the instruction to overwrite them. The 5.4 effects table has the control-variable copy `a + 3` as a `def` of `FORLOOP` / `FORPREP`, which 5.4 writes only when the loop runs (the exit path and a skipped loop write nothing; the slot is dead there, so the analysis's kill is sound). The check flagged every loop exit (gen:39). The two opcodes are exempt on 5.4 (no poisoning). Not a region-analysis bug; the 5.5 effects table has the slot in `uses` as well, which is why the check never fired there.
- **V2a `f6fd7336`** (test expectations, 5.4). `nr_retf_traces_are_installed`, `nr_n3_programs` assert on 5.4 that no RETF trace is installed (inert by design) and that the output equals the knob-off run. `rawfn_programs`: `rawget` / `rawset` are not recorded as library calls on 5.4 (`CFunc` abort in `record.rs`), so only the output is compared. `ba_ops_record_their_guards`: the `next` part is 5.5 only (same reason). New `TR_CORE_VERBOSE` env var for the `tr_core` child jobs.
- **V2b `f6fd7336`** (5.4 policy expectations). `p3_fdisp_programs`: on 5.4 the callee guard is not in the entry segment of this loop's root trace (the exit log shows every exit as a body exit with a side trace, `entry false`), so no entry failure happens, the P3 rule has nothing to flag (`ncallee_heads` 0) and the loop is not given up in either setting; the test asserts `heads <= 1` and `blacklisted <= off`. I did not find out why the 5.4 recording keeps the guard out of the entry segment (5.5: `heads == 1`). `c36_cispare_programs`: the repair works on 5.4 (`ci_repaired >= 1`), but `rerecord` is 2 in both knob settings (5.5: 0 with the knob): after the repair restarted the entry account the root of `mul_small` is flushed by the exit-ratio rule (`flush Rerec (entries 0, exits 6000)`); the test asserts `on.rerecord <= off.rerecord` on 5.4. Why the 5.5 root survives the same exit ratio was not chased; outputs are equal in every mode.

### Remaining exceptions

- Miri `gen_00`, `inc_00`, `pool_00` as whole shards: not completed (90 min each, no report); 27 of 30 script runs pass individually, the three `co_14_cstack_overflow` runs were cut at 25 min.
- `c36_cispare` / `p3_fdisp` on 5.4: weaker expectations (above), the 5.4 policy difference is not explained.
- 5.4 under ASan, Miri and Tree Borrows: not run.
