# Agent notes

## Names
Binaries: `rnlua` (interpreter), `rnluac` (compiler), `rnvmrun` (dev/test tool). Crates carry the `rn-` prefix: `rn-core`,
`rn-compiler`, `rn-std`, `rn-jit`, `rn-capi` (the C API, `librn_capi.a`), `rn-cli` (the package that builds `rnlua`/`rnluac`:
`cargo build --release -p rn-cli` -> `target/release/rnlua`, `rnluac`), plus `rustynail` (embedding API, `rnvmrun`) and `xtask`.
The JIT is a default feature of `rn-cli` (`--no-default-features` = interpreter only); `rnlua --version` shows
`JIT: on|off|not built`. `rnlua --license` prints the embedded LICENSE/LICENSE-LUA/NOTICE (regenerate NOTICE with
`scripts/gen-notice.py` when dependencies change).

## Release builds
`scripts/release-build.sh [--no-pgo] [--lua54] [--out DIR]`: PGO is the default, shipped files are stripped, every build path
remaps build-machine paths (`scripts/release-env.sh`) and the result is checked for home dir / user / host strings
(`docs/RELEASE.md`). Plain `cargo build --release` is the dev build. The PGO build takes about 10 minutes: run it in the
background under the memory cap.

## Memory safety (important)
Failure scenario: a test/script allocated tens of GB (RAM + swap hit >90%), and the system's OOM daemon killed the whole
terminal session (symptom: the shell and the agent session die with no useful error). Evidence, if it happens: the system
journal (grep for `oomd`).

Known causes (all were test cases, not real bugs):
- `string.rep('x', 1e10)` and similar legitimate multi-GB allocations. The reference Lua
  also allocates them, and our port copies on top (e.g. test helper `J` runs
  `tostring` + `table.concat` on the result: ~3x the size).
- `string.rep(..., 1 << 31, sep)` (4 GB) in libs1_string.

Rules:
1. Run every cargo test / rnvmrun / rnlua / refrun invocation under a memory cap AND a timeout:

       systemd-run --user --scope -q -p MemoryMax=8G -p MemorySwapMax=0 timeout 300 cargo test ...

   (fallback: `ulimit -v 8000000; timeout 300 ...`). Never run uncapped test loops.
2. Redirect stdin (`</dev/null`) for tests/scripts; an in-process `io.read` on an inherited
   terminal/socket stdin hangs forever.
3. When writing test cases, never request allocations over ~100 MB. To test "too large"
   errors use sizes that fail without allocating (`1 << 62`, `math.maxinteger`), and
   report only `#result` for big strings rather than the string itself.
4. Don't `pkill -f <name>` with a pattern that appears in your own command line (it kills
   your own shell); use `pgrep -f` to look first, or kill by PID.
5. Run long suites with `run_in_background` and capture output to a file.
6. Never wait for a process with a `pgrep -f '<pattern>'` loop (`while pgrep -f ...; do sleep 5;
   done`, `until ! pgrep -f ...`). The waiting shell's own command line contains the pattern, so
   `pgrep` matches itself and the loop never ends: stale loops piled up for hours and kept
   re-firing agent completion notifications. Run the command in the foreground under `timeout`,
   or with `run_in_background` and wait for its completion notification. If you must poll, wait on
   a PID (`while kill -0 $PID`) or on a file that the command is guaranteed to write, with a
   bounded loop (`for i in $(seq 120); do ...; done`), never on a pattern.

## Test harness gotchas
- Tests in one binary run on parallel threads sharing a pid: temp dirs must be unique per call.
- Compare outputs with `0x<hex>` addresses normalized; rnlua's stderr has a program-name
  prefix the reference lacks (stripped in vm_corpus `run_ours`).
- GC-stress runs legitimately change finalizer timing (see `skip_heavy` in vm_corpus.rs).

## GC modes and stress knobs (rnlua / tests)
- `rnlua` starts in **generational** mode like the reference `lua.c`; `RUSTYNAIL_GC_MODE=inc` keeps
  it incremental (the switch is done by `collectgarbage("generational")` in `pmain`).
- `RUSTYNAIL_GC_STRESS=1`: full collection at every safepoint. `=2`: ONE incremental step (or one
  young collection in generational mode) at every safepoint. `RUSTYNAIL_GC_VERIFY=1`: run
  `Heap::verify()` (colour/age invariants, dangling refs) after every step (slow; tiny programs only).
- Under stress 2 in generational mode, scripts that wait for a *finalizer of an old object* in a
  loop (`repeat local a = {} until name`, official `db.lua`) never finish: objects age to OLD
  within three allocations and only major collections reclaim them. That is expected, not a bug.
- Tests that observe finalizer timing/order must `collectgarbage("stop")` while building objects
  (see `tests/vm/meta_13_gc_finalizers.lua`), otherwise they depend on collector mode and pacing.
- Heap-level tests (`tests_gc.rs`): the raw `TableRef::set*` API does NOT issue write barriers; a
  mutator must call `heap.barrier_back(...)`/`barrier(...)` itself (the VM and `State::raw_set` do).
- Debugging a hang/crash of a script: no ptrace here; use `timeout -s ABRT N rnlua ...` and read
  the backtrace with `coredumpctl info rnlua`.

## C API (crates/rn-capi)
- Tests: `cargo test -p rn-capi` (differential C tests vs the reference `ref/55/.../liblua.a`, the
  vendored `lua.c` running the official suite, layouts, panic) and `cargo test -p rn-cli --test cmodule`
  (a prebuilt C module `require`d by rnlua). Needs `cc`.
- To debug a section by hand: build `tests/c/capi_diff.c` against both libs
  (`cc -Ivendor/lua-5.5/src tests/c/capi_diff.c <lib> -lm -ldl -lpthread`) and diff `./prog <section>`.
- The reference crashes (SIGSEGV) on API misuse that we tolerate (e.g. `lua_dump` of a non-function,
  `lua_settop` on a dead coroutine): keep the tests valid C API usage.
- `lua_Debug` strings (`name`, `what`, ...) are interned forever (`lapi_debug::intern`).

## Fuzzing and sanitizers (V7/V8)
- `scripts/asan.sh [target-dir]`: builds rnlua + rn-capi with ASan (nightly, build-std), runs the
  official suite through rnlua and through `lua.c`+the ASan library, the C API sections and the
  sample C module; prints `ASAN OK`. (Rust has no UBSan: use Miri and `debug-assertions`.)
- `fuzz/` is a separate nightly workspace (cargo-fuzz, ASan on by default). Targets: `lex_parse`,
  `undump`, `run_source` (sandbox: pure libs, instruction budget, 64 MB cap, GC mode/stress by
  first byte), `string_lib`, `table_ops` (model-checked) and `capi_calls` (random C API call
  programs). Seed corpora: `fuzz/make_seeds.sh`; run: `cd fuzz && cargo +nightly fuzz run <t> --
  -max_total_time=600 -rss_limit_mb=2500 -timeout=30` (always cap memory with `systemd-run`).
  Crash inputs land in `fuzz/artifacts/<t>/`; once fixed, add a regression test.
- The fuzz targets must respect internal API contracts (e.g. `Table::resize` only on empty tables):
  an assertion in an internal primitive misused by the harness is a harness bug.
