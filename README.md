# rustynail

Lua 5.5 and 5.4 in Rust, with a JIT compiler. It has the full language and standard libraries, a `lua`-compatible
command-line interpreter, a C API that is ABI compatible with PUC Lua's headers, and a safe Rust embedding API.

The executables have an `rn` prefix so they don't collide with a normal Lua install: `rnlua` (like `lua`) and
`rnluac` (like `luac`). The JIT is on by default on x86-64 Linux. Other platforms get the interpreter only.

- Lua 5.5.1 by default, Lua 5.4.9 with `--cfg rustynail_lua54` (chosen at build time).
- Same bytecode and binary chunk format as the reference `luac`.
- Incremental and generational GC, weak tables, finalizers, to-be-closed variables, coroutines, debug library.
- Error messages, number formatting, patterns, `string.format`/`pack` and table order (with a fixed hash seed)
  match the reference in differential tests.
- Sandboxing: memory limits, instruction budgets, text-only loading, library selection.
- Some LuaJIT-style extensions: `table.new`, `table.clear`, `string.buffer`, a `jit` table.

## Status

| | Lua 5.5 | Lua 5.4 |
|---|---|---|
| Official test suite (`cargo xtask official`) | 31/31 (reference 31/31) | 30/30 (reference 30/30) |
| Both GC modes | pass | pass |
| C API differential tests against the reference `liblua.a` | identical | identical |
| ASan, fuzzing (6 targets), Miri subset | clean | clean |

Known deviations are in `docs/STATUS.md` and `docs/ARCH.md`. Design notes: `docs/ARCH.md`, `docs/PORT_MAP.md`,
`docs/VERSION_DIFF.md`, `PLAN.md`.

## Performance

Geomean of CPU cycles over 223 benchmark programs (`bench/results/session-release/report.md`). Below 1 means
rustynail is faster. Measured on an AMD Ryzen AI 7 350. Outputs match PUC on every program.

| build | vs LuaJIT 2.1 | vs PUC Lua (same version) |
|---|---|---|
| Lua 5.5, plain | 0.995 | 0.230 |
| Lua 5.5, PGO (the release build) | 0.905 | 0.210 |
| Lua 5.4, plain | 1.184 | 0.268 |
| Lua 5.4, PGO | 1.081 | 0.245 |

- Every 5.5 benchmark is faster than PUC 5.5. The JIT speeds 5.5 programs up about 3.2x over the same build with
  the JIT off (2.8x on 5.4).
- Startup of an empty script is 0.49 ms (PUC 5.5: 0.32 ms, LuaJIT: 0.38 ms). The stripped `rnlua` is about 3 MB.
- 5.4 gets less out of the JIT: `rawget`/`rawset`/`next` and `#t` are not compiled, some integer-key table accesses
  are not compiled, and metamethod-heavy code gains little.
- Biggest gaps to LuaJIT on 5.5: `fib`, `raytrace`, `lua_lexer`, `classic_vector`, `tail_loop` (2.5-3.1x).
  Open items are in `docs/JIT_PARKED.md`; older numbers and method are in `docs/PERF.md`.

## Building

You need stable Rust (pinned in `rust-toolchain.toml`) and `gcc` (for a small variadic shim in the C API). Tests also
need `make` to build the reference Lua. `scripts/setup.sh` installs everything on Arch Linux.

```sh
cargo build --release -p rn-cli                       # target/release/rnlua and rnluac, JIT on
cargo build --release -p rn-cli --no-default-features # interpreter only
RUSTFLAGS="--cfg rustynail_lua54" CARGO_TARGET_DIR=target-54 \
  cargo build --release -p rn-cli                     # Lua 5.4
cargo install --path crates/rn-cli                    # install rnlua and rnluac
scripts/release-build.sh [--no-pgo] [--lua54] [--out dist]   # PGO build, stripped, into dist/
```

`scripts/release-build.sh` builds with profile-guided optimization (about 10 minutes), strips the binaries, and
writes `dist/` with `LICENSE`, `LICENSE-LUA`, `NOTICE` and `SHA256SUMS`. It remaps build paths and checks the
binaries for the builder's home directory and host name. Other variants (fast, BOLT) and PGO details are in
`docs/BUILDING.md`; release steps are in `docs/RELEASE.md`; reproducible builds in `docs/REPRODUCIBLE.md`.

## Repository layout

| Path | Contents |
|---|---|
| `crates/rn-core` | values, strings, tables, GC, state, coroutines, the interpreter |
| `crates/rn-compiler` | lexer, parser, code generator, dump/undump |
| `crates/rn-std` | standard libraries |
| `crates/rn-capi` | the C API (`librn_capi.a`) |
| `crates/rn-cli` | `rnlua` and `rnluac` |
| `crates/rn-jit` | the x86-64 JIT: template compiler and trace compiler (`docs/JIT.md`) |
| `crates/rustynail` | the safe Rust embedding API, and the dev tool `rnvmrun` |
| `xtask` | reference builds, test matrix, official suite runner |
| `vendor/` | unmodified Lua 5.5.1 and 5.4.9 sources and tests, used as the spec and test oracle |
| `tests/`, `fuzz/`, `bench/`, `scripts/` | differential tests, fuzz targets, benchmarks, build and CI scripts |

## Testing

Tests compare against the reference C Lua built from `vendor/`.

```sh
cargo xtask ref-build --all                        # builds ref/55 and ref/54, once
cargo test --workspace                             # Lua 5.5
cargo xtask test --lua-version 5.4                 # Lua 5.4 (needs cargo-nextest)
cargo xtask official --release                     # official 5.5 suite
cargo xtask official --release --lua-version 5.4   # official 5.4 suite
scripts/ci-local.sh [--full]                       # the CI gate, locally
```

Some tests allocate a lot of memory on purpose. Run long suites under a memory cap, for example
`systemd-run --user --scope -p MemoryMax=8G -p MemorySwapMax=0 cargo test ...` (see `CLAUDE.md`).
CI (`.github/workflows/ci.yml`) runs rustfmt, clippy, rustdoc, `cargo deny`, the test and official suites for both
versions, ASan, a Miri subset, a fuzz smoke run and the release build.

## Using rnlua

`rnlua` works like `lua`: `rnlua [options] [script [args]]` with `-e -l -i -v -E -W`, `LUA_INIT`, `LUA_PATH`,
`LUA_CPATH` and the REPL. `rnluac` is `luac`. Like the reference, it runs the collector in generational mode.
`--version` also prints the rustynail version and JIT status; `--license` prints the license texts.

Environment variables:

| variable | effect |
|---|---|
| `RUSTYNAIL_JIT=off\|on\|always\|verify` | unset means `on`. `off` is interpreter only; `always` compiles at first call (testing); `verify` adds checking |
| `RUSTYNAIL_JIT_TRACE=off\|on\|always` | the trace compiler; unset means on |
| `RUSTYNAIL_JIT_TRACE_LIMITS=k=v,...` | trace tuning knobs (`docs/JIT_TRACE_REF.md`) |
| `RUSTYNAIL_JIT_VERBOSE=1` | print compile and exit statistics at exit |
| `RUSTYNAIL_SEED=n` | fix the string hash seed (reproducible table order). Unset means random |
| `RUSTYNAIL_GC_MODE=inc` | incremental instead of generational GC |
| `RUSTYNAIL_GC_STRESS=1\|2`, `RUSTYNAIL_GC_VERIFY=1` | test aids: collect at every safepoint, verify the heap after every step |

Other `RUSTYNAIL_JIT_*` variables are tuning and debugging knobs (`docs/JIT.md`).

## Extensions

`rnlua` adds the data-oriented parts of LuaJIT's library (no FFI):

- `table.new(narray, nhash)` and `table.clear(t)`, also via `require("table.new")` and `require("table.clear")`.
- `require("string.buffer")`, with LuaJIT's serialization format in `buffer.encode` and `buffer.decode`.
- A global `jit` table: `jit.version`, `jit.os`, `jit.arch`, `jit.status()`, `jit.on()`, `jit.off()`, `jit.flush()`.

They were derived from LuaJIT 2.1 and checked against it (`scripts/ext-diff.sh`). There is no cdata, so
`reserve`, `commit`, `putcdata` and `ref` raise errors, and `jit.opt` and `jit.util` don't exist. Embedders enable
them with `StdLib::EXT` and `StdLib::JIT`. Details are in `docs/EXTENSIONS.md`.

## The JIT

The JIT has a template compiler for hot functions and loops, typed regions for hot loop nests, and a trace compiler
that follows hot loops across function calls. It starts compiling after about 200 calls or 1000 loop iterations.
Results are identical with the JIT on and off; the test suites run in both modes.

Machine code is mapped writable, then switched to executable, never both. If the host forbids that (SELinux
`execmem`, `MemoryDenyWriteExecute`, seccomp), the process runs in the interpreter. `RUSTYNAIL_JIT=off` disables the
JIT at run time and `--no-default-features` removes it at build time. `rnlua --version` shows `JIT: on|off|not built`.

For embedders the `jit` feature of the `rustynail` crate is opt-in, and a state runs without the JIT until you call
`Lua::set_jit_mode(JitMode::On)`. Design, knobs, limits and test strategy are in `docs/JIT.md`.

## C API and C modules

`crates/rn-capi` builds `librn_capi.a`, which exports the functions of `lua.h`, `lauxlib.h` and `lualib.h`. Lua errors
unwind through C frames like `longjmp`. Use the headers from `vendor/` for the matching version.

```sh
cargo build --release -p rn-capi
cc -Ivendor/lua-5.5/src prog.c target/release/librn_capi.a -lm -ldl -lpthread
```

`rnlua` exports the C API itself, so existing C modules load with `require` as with `lua`. Known deviations are in
`docs/ARCH.md`.

## Embedding in Rust

The `rustynail` crate is a safe wrapper with the semantics of the C API: a value stack, Rust closures as Lua
functions, typed userdata, registry references, coroutines, GC control and typed errors.

```rust
use rustynail::{Lua, Error};

fn main() -> rustynail::Result<()> {
    let mut lua = Lua::new()?;
    lua.push_function(|s| {
        let n = s.to_integer(1).ok_or_else(|| Error::runtime("integer expected"))?;
        s.push_integer(n * n)?;
        Ok(1)                                      // number of results
    })?;
    lua.set_global("square")?;
    lua.do_string("print(square(12))")?;           // 144
    Ok(())
}
```

Sandboxing untrusted code:

```rust
let mut lua = rustynail::Lua::builder()
    .sandbox()                     // safe libraries only, text-only `load`, no dofile/loadfile
    .memory_limit(16 << 20)        // bytes
    .instruction_limit(10_000_000) // not catchable by `pcall` in the script
    .build()?;
```

More in `crates/rustynail/examples/` (`cargo run -p rustynail --example userdata`) and `cargo doc -p rustynail --open`.

## Safety

- `unsafe` is limited to an allowlist of modules (values, heap/GC, stack, interpreter hot paths, tables, the C API).
  Each block has a `// SAFETY:` comment; `docs/UNSAFE.md` lists the invariants and the tests that cover them.
- Verification: differential tests against the reference, the official suites under GC stress, ASan
  (`scripts/asan.sh`), Miri over the core data structures, and fuzzing of the parser, undump, sandboxed execution,
  the string library, tables and the C API.
- Lua code can't crash the interpreter: running out of memory or stack becomes a Lua error. Precompiled binary
  chunks are trusted input, as in PUC Lua, because there is no bytecode verifier. If you run untrusted code, load
  text only (`Builder::sandbox`, or `load(s, name, "t")`).

## License

Copyright (c) 2026 Kiran Pashikanti <kpatvt@gmail.com>. MIT license, see `LICENSE`.

rustynail is a port of Lua (5.5.1 and 5.4.9, Copyright (C) 1994-2026 Lua.org, PUC-Rio), also MIT licensed. Lua's
notice is in `LICENSE-LUA` and must stay with copies. The vendored Lua sources and tests are unmodified
(`vendor/VENDOR.md`). A few benchmark programs under `bench/` derive from other MIT projects; they are listed in
`THIRD_PARTY.md`.

`NOTICE` lists the Rust crates linked into the release binaries (only `libc`). `scripts/gen-notice.py --check`
verifies it and `cargo deny` enforces the policy. `rnlua --license` and `rnluac --license` print `LICENSE`,
`LICENSE-LUA` and `NOTICE`.

Repository: <https://github.com/kpatvt/rustynail>
