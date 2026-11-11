# rustynail

A port of the Lua 5.5 interpreter (and Lua 5.4) to Rust: the full language, the standard libraries,
a `lua`-compatible command-line interpreter, a C API that is ABI compatible with PUC Lua's
`lua.h`/`lauxlib.h`/`lualib.h` (the vendored `lua.c` and real C modules link against it), and a safe
Rust embedding API. It is faster than the reference C implementation on most programs, and the release
binaries ship with a JIT compiler (template JIT plus trace compiler) **on by default** on x86-64 Linux.

The executables carry the `rn` prefix to tell them apart from the standard Lua tools: **`rnlua`** (the `lua`
interpreter) and **`rnluac`** (the `luac` bytecode compiler).

Repository: <https://github.com/kpatvt/rustynail>

- **Lua 5.5.1** (default) and **Lua 5.4.9** (`--cfg rustynail_lua54`), selected at compile time.
- Bytecode identical to the reference `luac` (same compiler output, same binary chunk format).
- Incremental and generational garbage collectors with the reference's pacing, weak tables,
  ephemerons, finalizers, to-be-closed variables, coroutines (including yields across metamethods,
  iterators and `pcall`), the debug library and hooks.
- Error messages, number formatting, `string.format`/`pack`, patterns, `os.date`, table iteration
  order (with a fixed hash seed) byte-identical to the reference in differential tests.
- Sandboxing: memory limits, instruction budgets, text-only loading, library selection.
- `unsafe` confined to an audited allowlist of modules (`docs/UNSAFE.md`); fuzzed, ASan- and
  Miri-checked.

Platform: x86-64 Linux, with the JIT. Other architectures build the interpreter only (the `jit` feature is inert
there; an aarch64 port is planned).

## Status

| | Lua 5.5 | Lua 5.4 |
|---|---|---|
| Official test suite (`cargo xtask official`) | **31/31** files (reference 31/31) | **30/30** (reference 30/30) |
| ... in both GC modes (generational, `RUSTYNAIL_GC_MODE=inc`) | yes | yes |
| C API (155 symbols) differential tests vs reference `liblua.a` | identical output | identical output |
| Vendored `lua.c` linked with our library runs the official suite | yes | yes |
| ASan (`scripts/asan.sh`), fuzzing (6 cargo-fuzz targets), Miri subset | clean | clean |
| Time vs reference (geomean, 25 benchmarks, default build) | see [Performance](#performance) | see [Performance](#performance) |

Known deviations are listed in `docs/STATUS.md` and `docs/ARCH.md` (C API section). Progress log
and design: `docs/STATUS.md`, `docs/ARCH.md`, `docs/PORT_MAP.md`, `docs/VERSION_DIFF.md`, `PLAN.md`.

## Repository layout

| Path | Contents |
|---|---|
| `crates/rn-core` | values, strings, tables, GC, state, calls, coroutines, the interpreter |
| `crates/rn-compiler` | lexer, parser, code generator, dump/undump (5.5 and 5.4 front ends) |
| `crates/rn-std` | standard libraries |
| `crates/rn-capi` | the C API (`librn_capi.a`) |
| `crates/rn-cli` | `rnlua` (the `lua` interpreter) and `rnluac` (`luac`); the cargo package is called `rn-cli` |
| `crates/rn-jit` | the x86-64 JIT: template compiler, typed regions, trace compiler (`docs/JIT.md`) |
| `crates/rustynail` | the safe Rust embedding API; also the dev/test tool `rnvmrun` (not a release artifact) |
| `xtask` | `cargo xtask`: reference builds, test matrix, official suite runner |
| `vendor/` | pristine Lua 5.5.1/5.4.9 sources and test suites (spec and test oracle; `vendor/VENDOR.md`) |
| `tests/`, `fuzz/`, `bench/` | differential corpora and C tests, cargo-fuzz targets, benchmarks |
| `scripts/` | build variants, benchmarks, sanitizers, CI, release |

## Building

Requirements: Rust stable (pinned in `rust-toolchain.toml`; the release toolchain was 1.99.1), `gcc`
(the C API's small variadic shim), and for the tests `make` (reference builds). Nightly Rust is only
needed for ASan, Miri and fuzzing; `perf` and `llvm-bolt` only for the BOLT build. `scripts/setup.sh`
installs everything on Arch Linux.

```sh
cargo build --release -p rn-cli                       # Lua 5.5: target/release/rnlua and rnluac, JIT on by default
cargo build --release -p rn-cli --no-default-features # the same without the JIT (interpreter only)
RUSTFLAGS="--cfg rustynail_lua54" CARGO_TARGET_DIR=target-54 \
  cargo build --release -p rn-cli                     # Lua 5.4: target-54/release/rnlua
target/release/rnlua -e 'print(_VERSION)'
cargo install --path crates/rn-cli                    # installs rnlua and rnluac into ~/.cargo/bin
scripts/release-build.sh [--no-pgo] [--lua54] [--out dist]   # PGO build, stripped: dist/{rnlua,rnluac,LICENSE,LICENSE-LUA,NOTICE,SHA256SUMS}
```

The `jit` cargo feature of `rn-cli` is a **default feature**; `--no-default-features` removes it (and the
JIT-related interpreter hooks), `--features jit_j4` (with `--no-default-features`) builds the template JIT without the
trace compiler. On a target other than x86-64 the JIT crate is not even a dependency and the default build is the
interpreter. The development tool `rnvmrun` (`cargo run -p rustynail --bin rnvmrun -- file.lua`) is a minimal runner for
tests; it is not installed and not part of the release.

`cargo xtask check|test|official --lua-version 5.4` passes the cfg for you. The release profile is
`opt-level=3`, fat LTO, `codegen-units=1`, `panic=unwind` (required: C API errors unwind through
C frames), line-table debug info (`debug=1`).

### Build variants

All variants are the same code; they differ in code generation and layout. Every script takes
`LUA_VERSION=5.4` for the 5.4 build (output under `target-54/` instead of `target/`).

| Variant | Command | Binary | When to use |
|---|---|---|---|
| default (dev) | `cargo build --release -p rn-cli` | `target/release/rnlua` | the plain build (JIT on by default): PIE, reproducible; the development and reference build (`scripts/release-build.sh --no-pgo` ships it, stripped). `--no-default-features` builds the interpreter only |
| fast | `scripts/build-fast.sh` | `target/fast/release/rnlua` | absolute (non-PIC) dispatch jump table, ~1-3% faster; non-PIE executable; reproducible |
| PGO (JIT) | `scripts/pgo-build.sh [out-dir]` | `target/pgo-jit/pgo/release/rnlua` | **the default release build** (what `scripts/release-build.sh` and `scripts/release.sh` produce by default; JIT + trace compiler, 0.913 of the plain build's cycles on 223 programs): profile-guided, trained on the list in `scripts/train.sh`; not bit-reproducible (see "PGO release build" below) |
| PGO (no JIT) | `scripts/pgo.sh` | `target/pgo/release/rnlua` | the same without the JIT feature (`PGO_FEATURES=jit scripts/pgo.sh` is what `pgo-build.sh` runs) |
| BOLT | `scripts/bolt.sh` | `target/bolt/rnlua.bolt` | fast build + profile-driven code layout (needs `llvm-bolt`, `perf` with branch sampling: `scripts/setup-bolt.sh`); measured on top of PGO + JIT in BUNDLE-P: no gain (0.999 of PGO cycles), not recommended |
| PGO + BOLT | `BOLT_PGO=1 scripts/bolt.sh` | `target/pgo-bolt/rnlua.bolt` | both |

### PGO release build

```sh
scripts/pgo-build.sh                  # JIT + trace compiler, PGO: target/pgo-jit/pgo/release/rnlua
scripts/pgo-build.sh ~/.cache/rn-pgo  # any out-dir (build dirs, profile and binary live there)
LUA_VERSION=5.4 scripts/pgo-build.sh  # Lua 5.4;  PGO_REPEAT=2: train twice;  PGO_HOLDOUT=1: see below
```

One command from a clean checkout, about 10 minutes: an instrumented build (`-Cprofile-generate`), a training run
over the one list in `scripts/train.sh` (the benchmark sets `bench/{awfy,apps,programs,lib,jit-trace,real}`, the
independent programs `bench/train/*` including GC-heavy and string-heavy ones, `bench/micro`, `tests/vm` and 17 files
of the official suite; all with `RUSTYNAIL_SEED=0`, no clocks, stdin from `/dev/null`), `llvm-profdata merge`,
and the optimised build (`-Cprofile-use`). It needs `llvm-profdata` of rustc's LLVM: the rustup component
`llvm-tools-preview` (listed in `rust-toolchain.toml`, installed on demand), or `LLVM_PROFDATA=/path`. Nothing is
committed: the merged profile (about 2 MB) belongs to one rustc and one source revision, so it is regenerated.
Training writes one raw profile per process (`LLVM_PROFILE_FILE=...%p.profraw`, a few thousand files, ~270 MB under
`<out-dir>/pgo-data/raw`, deleted after the merge) and merges them offline (`llvm-profdata merge --input-files`):
the online `%m` merge loses indirect-call and memop value profiles (about 0.8% cycles).

**The profile must be regenerated after every change to a hot path** (interpreter, tier-1 templates, trace compiler,
GC, tables, strings, the library): re-run `scripts/pgo-build.sh`; it always starts from a fresh profile. A stale profile
still builds, but LLVM drops the profile of every function whose CFG changed and the gain there is silently lost.
Because the benchmark sets are in the training list, speedups measured on them are optimistic; `PGO_HOLDOUT=1`
trains on the independent programs only and keeps the benchmarks as a hold-out (the numbers of both are in
`docs/JIT_TRACE_REF.md`, "BUNDLE-A decisions and results").

`scripts/release-build.sh` builds the PGO binary by default (`--no-pgo` for the plain build, `--lua54` for the 5.4 series;
`--pgo` is accepted and does nothing), strips `rnlua` and `rnluac` and writes `dist/`. The release scripts remap every
build-machine path (source tree, cargo and rustup homes, home directory; `scripts/release-env.sh`) and check the stripped
files for the builder's home directory, user and host names.

`scripts/release.sh [--versions 5.5,5.4] [--variants pgo,default,fast,bolt,pgo-bolt]` (default `pgo,default`; `pgo` is the JIT + PGO build) builds the chosen
variants into `dist/` with the (stripped) C library and headers, `LICENSE` files, `BUILD-INFO.txt` (commit, date, rustc,
target triple, platform) and `SHA256SUMS`. Reproducible builds: `docs/REPRODUCIBLE.md`. Release steps: `docs/RELEASE.md`.

## Testing

Tests compare against the reference C Lua built from `vendor/` (needs `gcc`, `make`):

```sh
cargo xtask ref-build --all                 # ref/55 and ref/54 (gitignored), once
cargo test --workspace                      # Lua 5.5: unit, differential, C API, examples
cargo xtask test --lua-version 5.4          # Lua 5.4 (cargo-nextest)
cargo xtask official --release              # official 5.5 suite: 31/31
cargo xtask official --release --lua-version 5.4   # official 5.4 suite: 30/30
scripts/ci-local.sh [--full]                # the whole CI gate locally (see below)
```

Some test programs allocate a lot of memory on purpose (`CLAUDE.md`): run long suites under a
memory cap, e.g. `systemd-run --user --scope -p MemoryMax=8G -p MemorySwapMax=0 cargo test ...`.

### Continuous integration

`.github/workflows/ci.yml` runs: rustfmt, clippy (`-D warnings`, both versions), rustdoc
(`-D warnings`, both versions), `cargo deny`, the test suites and the official suite for both versions
(with a cached reference build), ASan for both versions, a weekly Miri subset of `rn-core`, a
cargo-fuzz smoke run of every target, and the release build. `scripts/ci-local.sh` runs the same
steps locally (memory-capped, logs in `target/ci-local/`, summary table at the end; `--full` adds
ASan, Miri and fuzzing).

## Using the interpreter

`rnlua` behaves like `lua`: `rnlua [options] [script [args]]` with `-e`, `-l`, `-i`, `-v`,
`-E`, `-W`, `--`, `-`; `LUA_INIT`, `LUA_PATH`, `LUA_CPATH`; the REPL. Like the reference, it runs
the collector in generational mode. `rnluac` is `luac` (`-l`, `-p`, `-s`, `-o`). Diagnostics are prefixed with
`argv[0]`, as the reference does. Extensions: `--version` (like `-v`, plus a rustynail line with the version, `JIT: on|off|not
built` and the author) and `--license` (prints `LICENSE`, `LICENSE-LUA` and `NOTICE`; see [License](#license)).

### Environment variables (`rnlua`)

| variable | effect |
|---|---|
| `RUSTYNAIL_JIT=off\|on\|always\|verify` | the JIT; **unset = `on`** (tier 1 plus the trace compiler). `off` = interpreter only; `always` compiles at first call (testing); `verify` adds a quarantined code arena. Ignored by a `--no-default-features` build |
| `RUSTYNAIL_JIT_TRACE=off\|on\|always` | the trace compiler; unset = on. `off` = the template JIT with typed regions only |
| `RUSTYNAIL_JIT_TRACE_LIMITS=k=v,...` | trace policy limits and rules (`docs/JIT_TRACE_REF.md`) |
| `RUSTYNAIL_JIT_VERBOSE=1` | compile/exit profile on stderr at exit |
| `RUSTYNAIL_JIT_THRESHOLD=calls,edges`, `RUSTYNAIL_JIT_CODE_CAP=bytes`, `RUSTYNAIL_JIT_PERFMAP=1`, `RUSTYNAIL_JIT_DUMP=dir` | hotness thresholds, code-size cap (default 256 MiB), `perf` map file, machine-code dump; see `docs/JIT.md` |
| `RUSTYNAIL_SEED=n` | pin the string-hash seed (reproducible table order; differential testing). Unset = random |
| `RUSTYNAIL_GC_MODE=inc` | incremental instead of generational collector (default generational, like `lua.c`) |
| `RUSTYNAIL_GC_STRESS=1\|2`, `RUSTYNAIL_GC_VERIFY=1` | test aids: full collection (1) or one step (2) at every safepoint; heap verification after every step (slow) |
| `LUA_INIT`, `LUA_PATH`, `LUA_CPATH` (and `_5_5` variants) | as in the reference |

`RUSTYNAIL_JIT_*` variables other than the ones above are test and tuning knobs (`docs/JIT_TRACE_REF.md`).

## LuaJIT-compatible extensions

`rnlua` adds the data-oriented part of LuaJIT's library surface (no FFI): `table.new(narray, nhash)` and `table.clear(t)`
(also `require("table.new")` / `require("table.clear")`), `require("string.buffer")` (put/putf/get/skip/set/reset/free, and
`buffer.encode` / `buffer.decode` with LuaJIT's binary serialization format, `dict` and `metatable` options) and the global `jit`
table (`jit.version`, `jit.os`, `jit.arch`, `jit.status()`, `jit.on()`, `jit.off()`, `jit.flush()`; a stub when the JIT is not built or is off).
Behaviour and wire format were derived from LuaJIT 2.1 and are checked against it (`scripts/ext-diff.sh`: identical output on the
shared tests, byte-identical encodings for arrays and single-key maps, identical results and error texts for 40,000 mutated encodings).

Differences from LuaJIT: no cdata, so `reserve`/`commit`/`putcdata`/`ref` raise an error; Lua 5.5 has an integer subtype
(integers are written as doubles when exact, with the int64 tag beyond 2^53; integral doubles decode as integers); numbers print as Lua 5.5
prints them in `put`; the order of several hash keys in an encoding follows rustynail's table order (the decoded table is equal);
`jit.on/off(f)` per-function forms are accepted and ignored; `jit.opt` and `jit.util` do not exist. Embedders opt in with
`StdLib::EXT` / `StdLib::JIT` (not in `StdLib::SAFE`/`ALL`). API reference, wire-format table, error texts and semantics:
`docs/EXTENSIONS.md`.

## C API and C modules

`crates/rn-capi` builds `librn_capi.a`, which exports every function of `lua.h`, `lauxlib.h` and
`lualib.h` for the selected version (`extern "C-unwind"`; Lua errors unwind through C frames like
`longjmp` would). Use the vendored headers of the matching version:

```sh
cargo build --release -p rn-capi           # target/release/librn_capi.a
cc -Ivendor/lua-5.5/src prog.c target/release/librn_capi.a -lm -ldl -lpthread
# a shared library: cc -shared -o liblua.so -Wl,--whole-archive target/release/librn_capi.a -Wl,--no-whole-archive -lm -ldl
```

`rnlua` exports the C API from the executable (`-rdynamic`), so prebuilt C modules (`.so` files
built against the Lua headers) load with `require` exactly as with `lua`. Known deviations (e.g.
`lua_tocfunction` on Rust functions) are in `docs/ARCH.md`.

## Embedding in Rust

The `rustynail` crate is a thin, safe wrapper with the semantics of the C API: a value stack,
index-based operations, Rust closures as Lua functions, typed userdata, registry references,
coroutines, GC control, and typed errors.

```rust
use rustynail::{Lua, Error};

fn main() -> rustynail::Result<()> {
    let mut lua = Lua::new()?;                     // all standard libraries
    lua.push_function(|s| {                        // a Rust function: args at 1..=top
        let n = s.to_integer(1).ok_or_else(|| Error::runtime("integer expected"))?;
        s.push_integer(n * n)?;
        Ok(1)                                      // number of results
    })?;
    lua.set_global("square")?;
    lua.do_string("print(square(12))")?;          // 144

    lua.get_global("square")?;                     // call Lua (here: our function) from Rust
    lua.push_integer(7)?;
    lua.call(1, 1)?;
    assert_eq!(lua.to_integer(-1), Some(49));
    Ok(())
}
```

Sandboxing untrusted code:

```rust
let mut lua = rustynail::Lua::builder()
    .sandbox()                     // safe libraries only, text-only `load`, no dofile/loadfile
    .memory_limit(16 << 20)        // bytes
    .instruction_limit(10_000_000) // uncatchable by `pcall` in the script
    .build()?;
```

Examples (each also runs as a test): `crates/rustynail/examples/` (`hello`, `rust_functions`,
`call_lua`, `userdata`, `errors`, `sandbox`): `cargo run -p rustynail --example userdata`. API
documentation: `cargo doc -p rustynail --open`. The crate has no `unsafe` code; see its crate-level
documentation for the safety model.

## Performance

Release benchmark (`bench/results/session-release/report.md`): 223 programs (the benchmark sets plus the held-out
corpora), geomean of `perf stat` cycles, min of 5 interleaved runs, outputs equal to PUC's on every program. AMD Ryzen AI 7 350,
Linux. Below 1 means rustynail is faster.

| build | ours / LuaJIT 2.1 | ours / PUC Lua (same version) |
|---|---|---|
| Lua 5.5, plain | 0.995 | 0.230 |
| Lua 5.5, PGO (the release build) | **0.905** | **0.210** |
| Lua 5.4, plain | 1.184 | 0.268 |
| Lua 5.4, PGO | 1.081 | **0.245** |

- PGO versus the plain build (5.5): 0.913 cycles; held-out programs gain as much as the trained ones (0.88-0.91).
- Every program of the 5.5 set is faster than PUC 5.5. The JIT makes 5.5 programs about 3.2x faster than the same build
  with the JIT off (5.4: 2.8x).
- Startup of an empty script: 0.49 ms (PUC 5.5 0.32 ms, LuaJIT 0.38 ms); the stripped `rnlua` is 2.7-3.3 MB.
- Known weaker areas on 5.4 (about 16% less effective relative to PUC than 5.5): `rawget`/`rawset`/`next` are not
  recorded by the trace compiler (`lib/rawget_set` 11.5x relatively), inline `#t` is off, integer-key hash-part and
  `__index`/`__newindex` array accesses are not compiled, and metamethod-heavy programs (`class_middleclass`, `metamethods`)
  gain far less than on 5.5. On 5.5 the largest gaps to LuaJIT are `fib`, `raytrace`, `lua_lexer`, `lib/char` (a PGO
  slowdown, not diagnosed), `classic_vector` and `tail_loop` (2.5-3.1x).
  The list of open items is `docs/JIT_PARKED.md`.

Older, interpreter-only numbers (25 programs, `bench/programs`, JIT off, ratio to the reference built with `make linux`)
are in `docs/PERF.md`, together with the method, the per-benchmark tables, the LuaJIT comparison and the optimization
log. Benchmarks: `scripts/bench.py --bin ref=ref/55/build/lua --bin ours=target/release/rnlua` (output must equal the
reference's; wall time min-of-N on a pinned core, `perf stat` instructions, peak RSS).

## JIT (on by default, x86-64 Linux)

`rnlua` is built with an x86-64 JIT (cargo feature `jit`, a **default** feature of `rn-cli`): a template compiler for
hot functions and loops, typed regions for hot loop nests and a trace compiler for hot loops across function
boundaries. Nothing needs to be enabled: with `RUSTYNAIL_JIT` unset the JIT is on (hot code is compiled after about 200 calls or
1000 loop iterations); `RUSTYNAIL_JIT=off` turns it off at run time, `--no-default-features` removes it at build time
(`rnlua --version` shows `JIT: on|off|not built`). Results are identical with the JIT on and off (differential
tests, fuzzing and the official suites run in both modes). Machine code is mapped read+write and then flipped to
read+execute (never RWX); on hosts that forbid that (SELinux `execmem`, `MemoryDenyWriteExecute`, seccomp) the process
silently runs in the interpreter, and on non-x86-64 hosts the JIT is not built.

Measured (`bench/results/session-release/report.md`, see [Performance](#performance)): with the JIT rustynail takes 0.995x
(plain) and 0.905x (PGO) of LuaJIT 2.1's cycles (geomean over the 219 programs LuaJIT can run) and 0.230x / 0.210x of PUC Lua 5.5.

For embedders the library default is different: the `rustynail` crate has an opt-in `jit` feature and a state runs
without the JIT until `Lua::set_jit_mode(JitMode::On)` / `Builder::jit(..)`; the `Builder::sandbox()` preset does not enable
it (it stays off until the executable-memory probe has shipped in a release). Design summary, invariants, knobs,
limits, security notes, platforms, diagnosis and test strategy: `docs/JIT.md`.

## Safety

- `unsafe` is allowed only in an allowlist of modules (value representation, heap/GC, stack,
  the interpreter's hot paths, table internals, and the C API), each block with a `// SAFETY:`
  comment; `docs/UNSAFE.md` lists the invariants and the tests that cover them. The compiler, the
  standard library (except the documented `io`/`os`/`dlopen` FFI), the CLI and the embedding API are
  `unsafe`-free or close to it.
- Verification: differential tests against the reference, the official suites under GC stress,
  AddressSanitizer runs of the whole system (`scripts/asan.sh`), Miri over the core data structures,
  and six cargo-fuzz targets (parser, undump, sandboxed execution, string library, tables, C API).
- Lua code cannot crash the interpreter: resource exhaustion becomes a Lua error ("not enough
  memory", "stack overflow", ...). **Precompiled binary chunks are trusted input**, as in PUC Lua
  (there is no bytecode verifier): states that run untrusted code must load text only
  (`Builder::text_only`/`sandbox`, or `load(s, name, "t")`).

## License

Copyright (c) 2026 Kiran Pashikanti <kpatvt@gmail.com>. rustynail is licensed under the MIT license
(`LICENSE`, SPDX `MIT`).

rustynail is a derivative port of the Lua interpreter (Lua 5.5.1 and 5.4.9, Copyright (C) 1994-2026 Lua.org,
PUC-Rio, https://www.lua.org/), which is also MIT licensed; Lua's notice is in `LICENSE-LUA` and must be kept with
copies of rustynail. The vendored upstream sources and test suites are unmodified (`vendor/VENDOR.md`). A few
benchmark programs under `bench/` are derived from other MIT-licensed projects (LuaJIT's disassemblers, json.lua,
middleclass, ...); they and their notices are listed in `THIRD_PARTY.md`.

`NOTICE` lists the third-party Rust crates linked into the release binaries with their licences (today only `libc`,
MIT OR Apache-2.0, used under MIT); it is generated by `scripts/gen-notice.py` (`--check` verifies it) and the policy
is enforced by `cargo deny` (`deny.toml`). The executables embed all three texts: `rnlua --license` and
`rnluac --license` print `LICENSE`, `LICENSE-LUA` and `NOTICE`; `rnlua --version` / `rnluac --version` (also `-v`) print
the reference's `Lua 5.x.y  Copyright (C) 1994-2026 Lua.org, PUC-Rio` line followed by a line with the rustynail
version, the JIT status and the author, for example
`rnlua 0.0.1 (Lua 5.5 compatible; JIT: on) Copyright (c) 2026 Kiran Pashikanti <kpatvt@gmail.com>`. (`--version` and
`--license` are extensions: the reference `lua` rejects them as unknown options.) The entry points `crates/rn-cli/src/
main.rs` and `luac.rs` carry an `SPDX-License-Identifier: MIT` header; other source files rely on the repository
licence. Nothing is published to crates.io.
