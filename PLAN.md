# rustynail — Lua 5.5 (and 5.4) in Rust: Implementation Plan

Status: DRAFT v3 (post-Opus-review). Changes vs v2: 5.5 facts corrected (float format, default GC = generational, GC API), C-variadic shim decision, binary-chunk trust policy, allocator/OOM layer, expanded unwind rules, continuations designed in Phase 0, vendoring moved to Phase 0, builder load rebalanced, dual-version via cfg + forked modules, valgrind/perf fallback, relaxed perf gate definition, sandbox/limits section.
Owner: orchestrator (main agent). Executors: Builder A and Builder B (max 2).
Location: the repository root (all paths in this file are repo-relative)

---------------------------------------------------------------------------

## 0. Decisions (settled with the user)

| Topic | Decision |
|---|---|
| Target | **Lua 5.5 (primary) and Lua 5.4 (also supported)**. Highest 5.5.x and highest 5.4.x on lua.org at vendoring time (host has 5.5.1 and 5.4.9 libs); pin versions + SHA256. See §0.1. |
| Unsafe policy | **Contained unsafe.** Safe by default; `unsafe` only in an allowlisted set of modules (see §6). |
| Scope | Full language + standard libraries + **C API (`lua.h`/`lauxlib.h`/`lualib.h`) ABI compatibility**, so the vendored `lua.c` and real C modules can link against our library. Rust embedding API is also provided. |
| Speed goal | **Stage 1: parity** with PUC Lua 5.5 (built from vendored source, `-O2`, same machine). **Stage 2: beat it by 10–30%** on the benchmark suite geomean. |
| Builders | 2 builders max, in separate git worktrees. Orchestrator integrates, gates, and measures. |
| C-variadic API | `lua_pushfstring`, `lua_pushvfstring`, `luaL_error`, `lua_gc` are C-variadic and cannot be defined on stable Rust. **A tiny audited C shim (~30 lines, built by `build.rs` via the `cc` crate) formats into a buffer and calls Rust.** It is the only C in the product. "Pure Rust" therefore means: all VM/compiler/stdlib logic in Rust; `libc` crate FFI and system libc allowed; the single vararg shim is the documented exception (`docs/UNSAFE.md`). |
| Binary chunks | **Trusted, as in PUC Lua** (no bytecode verifier). Provide a sandbox load mode that forces text-only (`mode "t"`) for hosts running untrusted code. The "no UB on any Lua input" guarantee covers source text and malformed-input *loading* (undump must not crash), but **excludes executing attacker-crafted precompiled bytecode**. |
| Generational GC | **Required for parity in both versions.** Verified in the vendored sources: the library default (`lua_newstate`, `g->gckind = KGC_INC`) is incremental in 5.4.9 and 5.5.1, but the standalone `lua.c` calls `lua_gc(L, LUA_GCGEN…)` at startup in **both** versions (5.4 `lua.c:646`, 5.5 `lua.c:754`). The reference `lua` binary used for all baselines therefore runs generational, so rustynail's CLI must do the same and G3 is needed for G-parity. (Earlier drafts wrongly said 5.5 alone defaults to generational.) |

### Honest expectations (read before planning around speed)
- PUC Lua is a hand-tuned C interpreter. A safe-ish Rust interpreter typically lands at 0.9–1.2x of it; reaching **+10–30%** requires deliberate design (§5) and PGO. "Significant" (2x+) speedups are **not** plausible without a JIT and are out of scope. The plan treats parity as a hard gate and the +10–30% as a stretch to be earned benchmark by benchmark.
- **Memory safety is partial by construction**: a GC'd VM with a C ABI needs `unsafe`. The guarantee we deliver is: safe public Rust API, `unsafe` confined/audited/tested (Miri, sanitizers, fuzzing), no panics or UB on any Lua input.
- **Correction to an earlier suggestion:** NaN-boxing is *rejected*. Lua 5.4+/5.5 has full 64-bit integers and the C API exposes `lua_Integer`; NaN-boxing cannot hold them without boxing. We use a 16-byte tagged value like PUC.
- **C API compat constrains the design**: C code calling `lua_error` expects a non-local exit through C frames (longjmp in PUC). See §4.5 for how we handle it; this is the highest technical risk and gets a spike in Phase 0.

### 0.1 Dual-version strategy (5.5 primary, 5.4 also)
- **Mechanism: compile-time version selection via `--cfg rustynail_lua54` / `--cfg rustynail_lua55`, set by `cargo xtask` (default 5.5)** — not mutually exclusive Cargo features, which break feature unification, `--all-features` (clippy, docs, IDEs). A `build.rs` `compile_error!` fires if neither or both cfgs are set. Separate binaries/libs per version; no runtime switch (it would tax the interpreter loop).
- **Shared code is the default; divergence is isolated, but not forced into abstractions prematurely.** Bit-identical bytecode (V2) means `lparser`/`lcode`/`lopcodes`/`ltable` deltas are pervasive, so the compiler is **two forked modules** (`compiler::v54`, `compiler::v55`) sharing lexer/utility code, and `opcodes`, array-part policy, `rn-capi` ABI deltas, `LUA_VERSION_*` and stdlib deltas live in `ver/`. Do **not** build `ver/` abstractions before `VERSION_DIFF.md` exists. `cargo xtask check-cfg` forbids stray `cfg(rustynail_*)` outside `ver/`, `compiler/v5x` and `rn-capi`.
- **Deliverable `docs/VERSION_DIFF.md` (orchestrator, Phase 0):** produced by diffing the two vendored trees (`lopcodes.h`, `lparser.c`, `lcode.c`, `ltable.c`, `lgc.c`, `lvm.c`, `lua.h`, `lauxlib.h`, `luaconf.h`, each `l*lib.c`) and the two manuals' incompatibility sections. It is the authoritative list of what is 5.5-only vs shared (e.g. `global` declarations, 5.5 loop-variable rules, compact arrays, any C API signature changes). Do not rely on memory of either version.
- **Seed facts for `VERSION_DIFF.md`** (verified by the Opus review against installed 5.5.1/5.4.9 headers and binaries; still re-derive from the vendored sources): 
  - `LUA_NUMBER_FMT` is `"%.15g"` in 5.5 with `LUA_NUMBER_FMT_N "%.17g"` fallback when %.15g does not round-trip (`1/3` prints `0.33333333333333331`); 5.4 uses `%.14g`.
  - `lua_newstate(f, ud, unsigned seed)` in 5.5; new `luaL_makeseed`, `luaL_alloc`, `lua_pushexternalstring`, `lua_numbertocstring` (+ `LUA_N2SBUFFSZ`), `luaL_openselectedlibs` (`luaL_openlibs` becomes a macro), `lua_resetthread` becomes a macro; `lua_setcstacklimit`/`debug.setcstacklimit` removed.
  - `collectgarbage("setpause"/"setstepmul")` removed in 5.5 (replaced by `"param"`, `LUA_GCPARAM`, `LUA_GCP*`); `LUA_GCGEN`/`LUA_GCINC` are 7/8 in 5.5 and 10/11 in 5.4.
  - `LUA_RIDX_MAINTHREAD` is 3 (5.5) vs 1 (5.4); `LUA_REGISTRYINDEX` is `-(INT_MAX/2+1000)` (5.5) vs `-LUAI_MAXSTACK-1000` (5.4); `LUAI_MAXSTACK` is no longer in 5.5's public `luaconf.h`.
  - `lua_Debug` gains `extraargs`; `ftransfer`/`ntransfer` become `int`.
  - Named varargs `function f(...t)` with a new `GETVARG` opcode; `global` declarations (contextual keyword under `LUA_COMPAT_GLOBAL=1`, the default); `table.create`; numeric `for` uses one fewer stack slot; `luaL_Buffer` identical in 5.4 and 5.5.
  - Upstream 5.4 `src/Makefile` builds with `-DLUA_COMPAT_5_3` (e.g. `math.pow`, `log10` exist in the system 5.4.9, not in 5.5).
- **Vendoring doubled:** `vendor/lua-5.4/`, `vendor/lua-5.5/`, `vendor/lua-tests-5.4/`, `vendor/lua-tests-5.5/`. Reference builds go to `ref/54/` and `ref/55/`. Every harness layer (V2–V6, V10) is parameterized `--lua-version {5.4,5.5}`; `xtask ci` runs the matrix.
- **Bytecode/dump compatibility is per version** (chunks are not interchangeable; `LUAC_VERSION` differs); V2/V5 compare against the matching reference `luac`.
- **Sequencing:** the version cfg mechanism exists from Phase 0 and `cargo xtask check --lua-version 5.4` (compile-only) runs in CI from Phase 2 so 5.4 never rots, but implementation order is **5.5 through parity, then 5.4 as Phase 6b**. 5.4 is expected to be a delta on the same code, so 6b should be much smaller than the original port.
- **Perf gates apply to both versions**, each against its own PUC reference.

---------------------------------------------------------------------------

## 1. Host environment (current state, verified)

- Linux (Arch Linux), 16 cores, bash.
- **Missing**: `cargo`, `rustc`, `rustup`, `hyperfine`, `perf`, `valgrind`.
- **Present**: `gcc`, `clang`, `make`, `lua` 5.5.1, `luajit` 2.1, system `liblua5.1/5.4/5.5`.
- Repo dir exists (`scripts/` and this plan only) but is not yet a git repo.

### 1.1 Toolchain setup
**`scripts/setup.sh` is authoritative** (Arch Linux; idempotent; `--check`, `--no-cargo-tools`, `--no-nightly`). Run it via `! scripts/setup.sh` since the pacman step needs sudo. It installs rustup (stable + nightly with Miri), clippy/rustfmt/llvm-tools, clang/lld/mold, hyperfine, perf, valgrind, and the cargo tools used by the plan. Pin the exact stable version in `rust-toolchain.toml` and `docs/TOOLCHAIN.md`; nightly is for Miri, sanitizers, cargo-fuzz only.
Pin in `rust-toolchain.toml`: `channel = "stable"` (record exact version in `docs/TOOLCHAIN.md`); nightly is used **only** for Miri, sanitizers, cargo-fuzz and the optional `become` tail-call experiment, never for the shipped build unless Phase 7 proves a win and the feature is justified.

### 1.2 Repo bootstrap (orchestrator, Phase 0)
```
git init rustynail && cd rustynail
```
Layout:
```
rustynail/
  Cargo.toml                 # workspace, shared [profile.*]
  rust-toolchain.toml
  PLAN.md  docs/{STATUS.md,ARCH.md,UNSAFE.md,TOOLCHAIN.md,PERF.md}
  vendor/
    lua-5.5/ lua-5.4/             # pristine upstream source trees (read-only reference) + SHA256SUMS + LICENSE
    lua-tests-5.5/ lua-tests-5.4/ # official test suites, pristine
    VENDOR.md       # URL, version, hash, date, local patches (should be none)
  crates/
    rn-core/       # values, strings, tables, GC, State, CallInfo, interpreter, metamethods, errors
    rn-compiler/   # lexer, parser, codegen (port of llex/lparser/lcode), ldump/lundump
    rn-std/        # base, string(+pattern,+format,+pack), table, math, io, os, coroutine, utf8, debug, package
    rn-capi/       # lua_* / luaL_* extern "C-unwind" exports; crate-type staticlib+cdylib+rlib; ships C headers
    rustynail/         # safe Rust embedding facade (Lua, Table, Function, UserData, Scope...)
    rn-cli/        # `lua`-compatible binary in pure Rust (REPL, -e -l -i -v -W, arg table, LUA_INIT)
  tests/
    lua/            # our own .lua tests + golden outputs
    c/              # C-API tests: vendored lua.c relinked against rn-capi, sample C modules
    differential/   # scripts + corpus for ref-vs-ours comparison
  bench/            # benchmark programs (.lua), runner, baselines (JSON), compare tool
  fuzz/             # cargo-fuzz targets
  xtask/            # `cargo xtask <cmd>`: ref-build, test, diff, bench, ci, unsafe-audit
  ref/              # build output of the *reference* C Lua: ref/54, ref/55 (gitignored)
```
Rationale for crate split: compiler depends on core's object model (strings, Proto), not vice versa; `load` in core goes through a registered `Loader` fn pointer set by the facade/CLI, avoiding a cycle.

### 1.3 Cargo profiles (workspace `Cargo.toml`)
```
[profile.release]      opt-level=3  lto="fat"  codegen-units=1  debug=1  panic="unwind"   # see §4.5 for why unwind
[profile.bench]        inherits="release"
[profile.profiling]    inherits="release"  debug=2  # + RUSTFLAGS=-C force-frame-pointers=yes
[profile.dev]          opt-level=1  debug-assertions=true  overflow-checks=true
[profile.test]         opt-level=1
```
- Benchmarks built with `-C target-cpu=native` **locally only** and compared against C built with `-march=native`. Shipped artifacts use `x86-64-v2` baseline.
- PGO via `cargo pgo` (instrument → run bench corpus → optimize). Reference C Lua also gets a PGO build for a fair "best vs best" comparison; report both plain and PGO ratios.

---------------------------------------------------------------------------

## 2. Vendoring (orchestrator, **Phase 0** — must precede opcode/Proto contracts)

1. For **each** of 5.5 and 5.4: download `lua-5.x.y.tar.gz` and `lua-5.x.y-tests.tar.gz` from lua.org into a **fresh empty directory in the scratch directory**; verify SHA256 against the values published on lua.org's download page; extract into `vendor/lua-<ver>` and `vendor/lua-tests-<ver>`. Never run anything from inside the extracted dirs; build the reference out-of-tree.
2. Record in `vendor/VENDOR.md`: URL, version, SHA256, date, "no local patches". Keep upstream `COPYRIGHT`/MIT text. Our Rust code is a derivative port, so ship the Lua MIT notice in `LICENSE-LUA` alongside our own license.
3. **Vendored C is a spec and test oracle, not a build input** for the Rust crates. Exceptions: (a) `xtask ref-build` compiles it to `ref/lua`, `ref/luac`, `ref/liblua.a`; (b) `tests/c/` compiles `lua.c` and the public headers against our `rn-capi` library.
4. `rn-capi/include/55/` and `include/54/` contain copies of `lua.h`, `luaconf.h`, `lualib.h`, `lauxlib.h` byte-identical to vendor (CI check with `diff`). ABI-visible values come from them: `LUA_EXTRASPACE`, `LUAL_BUFFERSIZE`, `LUA_IDSIZE`, `lua_Integer`/`lua_Number` types, `LUAI_MAXSTACK`, pseudo-indices, `LUA_REGISTRYINDEX`.
5. **Compat flags are pinned per version** for both the reference build and ours, and recorded in `vendor/VENDOR.md` (5.4 reference uses upstream's default `-DLUA_COMPAT_5_3`; decide per version whether tests need compat and keep both sides identical).
6. The vararg C shim (`rn-capi/shim/varargs.c`) is the only C compiled into the product; it is reviewed line by line and listed in `docs/UNSAFE.md`.
5. Port-tracking doc `docs/PORT_MAP.md`: table mapping each upstream C file → Rust module → owner → status (not started / ported / verified). Builders update it in every merge.

Reference builds (`cargo xtask ref-build --lua-version V`): **one default build per version (upstream `make linux`-style, gcc `-O2`) is the gating baseline.** A clang build, a PGO build and `luajit -joff` are recorded as *informational* baselines only. The system `lua` 5.5.1 binary is used only for quick sanity checks, never as the oracle (pin/compat flags may differ).

---------------------------------------------------------------------------

## 3. Verification harness (built in Phase 0/1, before real code)

Tests must exist before the implementation they check. Layers, cheapest first:

| # | Layer | What | Tool |
|---|---|---|---|
| V1 | Rust unit tests | per-module (value ops, string interning, table, number parsing/formatting, lexer tokens) | `cargo nextest` |
| V2 | **Bytecode differential** | for every `.lua` in corpus (official tests + ours + fuzz corpus), `ref/<V>/luac -l -l -p` vs our `rnlua --list` (a port of `luac.c`'s `print.c` listing; **normalize `0x…` addresses**) — instruction streams, constants, upvalues, line info must match textually. Requires exact per-version opcode set/encoding. Strongest check on the compiler port. | `xtask diff-bytecode` |
| V3 | **Output differential** | run each script under `ref/lua` and ours; compare stdout, stderr (normalized for paths/addresses), exit code. Includes error messages and tracebacks. | `xtask diff-run` |
| V4 | **Official test suite** | `vendor/lua-tests-<V>/all.lua` under our `lua` with `_port=true` (skip parts needing the instrumented `T` library). Each file aborts at its first failed assert, so track **per-file status plus the first failing line** and trend it (`tests/official-status.toml`); keep a list of files/sections unusable without `T` (e.g. parts of `api.lua`, `code.lua`, `gc.lua`) so "100% of ref-passing" is well-defined. CI gate: no file regresses. | `xtask official` |
| V5 | dump/undump | `string.dump` output byte-compatible with ref (same header, format); load ref-dumped chunks and vice versa. | V2/V3 extension |
| V6 | C API conformance | (a) build vendored `lua.c` + `lauxlib` consumers against `rn-capi`, run V4 through *that* binary; (b) sample C modules (a hand-written one covering every `lua_*` entry point incl. `lua_callk/pcallk/yieldk`, userdata/uservalues, `luaL_Buffer`, `lua_getextraspace`, `lua_newstate` with custom allocator; plus real-world: lpeg, lfs, lua-cjson if fetchable) loaded via `require`. | `tests/c/`, `xtask capi-test` |
| V7 | Fuzzing | cargo-fuzz targets: lexer, parser/compiler (differential vs ref `luac -p` accept/reject), undump (must never crash/UB on malformed bytes), string.format/pack/patterns, `load`+run with instruction/memory limits, table ops. Corpus seeded from official tests. Nightly CI job runs N minutes; crashes become regression tests. | `cargo fuzz` |
| V8 | Memory-safety tooling | Miri on `rn-core`/`rn-compiler` unit tests only (Miri cannot run FFI); ASan+UBSan builds (nightly `-Zsanitizer`, `-Zbuild-std`, C side built with `-fsanitize` too) running V4; `debug-assertions` + `overflow-checks` in dev; **GC stress mode** (`RUSTYNAIL_GC_STRESS=1`): full collection at **every GC safepoint** (the PUC `luaC_checkGC` points), *not* at every allocation — collecting mid-resize/mid-compile would create bugs PUC cannot have. | `xtask miri`, `xtask san`, `xtask gcstress` |
| V9 | Unsafe audit | `xtask unsafe-audit`: fail if `unsafe` appears outside the allowlist in §6, if any `unsafe` block lacks a `// SAFETY:` comment (`clippy::undocumented_unsafe_blocks`), or if per-module unsafe count grows without an `docs/UNSAFE.md` entry. `cargo geiger` report archived. | CI |
| V10 | Performance regression | see §7. **Report-only until G-parity**; after that fails on >3% geomean or >7% single-benchmark regression vs stored baseline. Low-noise signal = `perf stat -e instructions:u` (see §7 re valgrind). | `xtask bench` |

`cargo xtask ci` runs V1, V2, V3, V4, V9, clippy (`-D warnings`), rustfmt, `cargo deny`. Full gate (`xtask ci --full`) adds V5–V8 and V10, run by the orchestrator before each phase milestone merge.

Flakiness rules: tests are deterministic. If `ltable` and the string hash are ported exactly and the reference is built with a fixed seed (`lua_newstate` seed param in 5.5; hash seed in 5.4), `pairs` order for string/integer keys should match the reference, so most scripts need no key sorting; only address-keyed tables differ. Address-bearing outputs (`table: 0x…`) are normalized. The harness lints scripts that print `pairs` order of address-keyed tables.

---------------------------------------------------------------------------

## 4. Architecture

### 4.1 Value and objects
- `Value`: 16 bytes `#[repr(C)]` {tag: u8 (type + variant bits mirroring PUC's `LUA_V*` so type checks are mask/compare), payload: union{i64, f64, bool-as-int, raw ptr}}. Rust enum is avoided for the hot representation; a hand-rolled tagged union with safe accessors (`as_int() -> Option<i64>`) and an unsafe-contained constructor layer.
- GC objects: `#[repr(C)]` header {next: *mut GcHeader, tt: u8, marked: u8} followed by type-specific body, same family as PUC: `LuaString` (short interned ≤40 bytes; long lazily hashed), `Table`, `LuaClosure`, `RustClosure`/`CClosure`, `Proto`, `UpVal`, `UserData` (with user values), `Thread`. Typed handle `Gc<T>` is `Copy`, `NonNull<T>`-based; **not** dereferenceable without a `&GcCtx`/live-state witness in the safe layer.
- **Allocation & OOM:** `lua_newstate(lua_Alloc, ud[, seed])` and "allocation failure → `not enough memory`" require *every* VM allocation (stacks, CallInfo, tables, strings, Protos, compiler buffers) to go through the user allocator and fail recoverably with GC-debt accounting. `Vec`/`Box` abort on OOM and `allocator_api` is nightly-only, so `rn-core` uses its own **fallible raw buffers** (`RawVec`-like, `try_reserve` semantics) over an `Allocator` abstraction; std collections are banned in `rn-core`/`rn-compiler` hot or Lua-reachable structures (clippy `disallowed_types`). Default allocator = system via `std::alloc`; small-object size-class freelists for GC headers (pure Rust). **GC is non-moving, permanently** (`lua_tostring`/`lua_touserdata` pointers must stay stable). `LuaString` must support externally owned buffers (`lua_pushexternalstring`, 5.5) — decide layout in B1.

### 4.2 Strings
- Short strings interned in a global open-chaining table with per-state random seed; equality = pointer compare. Long strings: length + lazy hash. Keep `\0`-safe bytes (`&[u8]`, never `str`). Cache for string→number coercions and `__name`-like metamethod-name strings pre-created (`TM_*` array).

### 4.3 Tables
- Array part + hash part (node array, chained scatter with Brent's variation à la PUC). 5.5 changed array-part layout/sizing (compact array); **builder must read upstream `ltable.c` 5.5 and mirror semantics of `#`/`next`/rehash exactly** because border behavior and `next` order are observable and tested.
- `flags` byte caching "metamethod absent" bits per table (as PUC) for fast `__index`/`__newindex`/`__eq`… negative lookups.
- Fast paths: `get_int(i64)`, `get_short_str(&LuaString)`, `get_generic`, plus set variants; slow paths cold-marked (`#[cold]`).

### 4.4 Interpreter
- Bytecode = exact 5.5 instruction encoding/opcodes (enables V2/V5, and lets us port `lvm.c` handler by handler).
- Single `execute()` function: `loop { let i = *pc; pc += 1; match i.opcode() { … } }` with the opcode as a `#[repr(u8)]` enum so LLVM emits a jump table; `#[inline(always)]` on the small handlers; slow paths in `#[cold] #[inline(never)]` functions.
- Cached locals in registers/stack frame: `pc: *const u32`, `base: *mut Value`, `k: *const Value`; reloaded only after operations that may reallocate the stack or run arbitrary code (the equivalent of PUC's `Protect`/`updatebase`). Stack growth is the #1 source of dangling pointers — enforced by funnelling through a few `#[must_use]` helpers and checked in debug via a stack-generation counter.
- Lua→Lua calls and returns **do not recurse the Rust stack** (reuse `execute` frame via `CallInfo` push/pop, like PUC's `startfunc/ret` gotos). Rust-function→Lua calls do recurse and are depth-limited (`LUAI_MAXCCALLS` analogue, "C stack overflow" error identical in text).
- Errors inside the VM: Lua error objects are arbitrary values, so the internal error type is a **1-byte status** (`Result<T, Status>`, fits in a register) with the error value stored in the `State` — the same path used for C-API errors. No unwinding for ordinary Lua-to-Lua/Rust-stdlib errors. Metamethod/`__close`/`pcall` semantics identical to ref.
- Numeric for loops, integer/float arithmetic with exact Lua semantics (wrap-around, floor div/mod, shifts, string→number coercion, float→int exactness) ported from the matching version's `lvm.c`/`lobject.c`/`llimits.h`. **Number→string is per-version**: 5.5 `tostringbuff`/`lua_numbertocstring` uses `%.15g` with `%.17g` fallback if it does not round-trip; 5.4 uses `%.14g`. Parsing (`l_str2d`) must match exactly too. Implementation: for the hot `tostring` path build `%g` output from Rust `format!("{:.*e}", p-1, x)` (exact rounding); for `string.format` with arbitrary C specs (`%5.2f`, `%-10g`, `%a`) call libc `snprintf` via FFI (exact by construction). Validate with millions of randomized values against libc, per version.

### 4.5 Errors, coroutines and the C API boundary (HIGHEST RISK — Phase 0 spike)
- Coroutines: each `Thread` owns its own Lua stack + CallInfo list; `resume`/`yield` implemented by switching the *active thread* inside the same Rust stack (as PUC does: yield returns up the C stack to `resume`). Yield from Lua code = unwind of the interpreter loop via a `Yield` result, no OS threads/stack switching. Yield across a Rust/C function requires continuation (`lua_callk`/`lua_pcallk`/`lua_yieldk`); otherwise error "attempt to yield across a C-call boundary", same as ref.
- **Continuations are designed in Phase 0, not retrofitted.** `pcall`/`xpcall`, metamethods, generic-`for` iterators, `__close` and `__concat` must be yieldable, mirroring PUC's `luaV_finishOp` and `unroll`. Consequences baked into ARCH.md: the internal native-function convention has a continuation form (`k` + ctx); `CallInfo` carries `CIST_*` flags and `k`/`ctx`/`old_errfunc`; `execute` re-entry for metamethods must be resumable. Some natives intentionally stay non-yieldable (e.g. `table.sort` comparator, `string.gsub` callback) with PUC's exact "attempt to yield across a C-call boundary" error.
- Native (Rust) stdlib functions use an **internal fast calling convention** (`fn(&mut Ctx, args: Range) -> Result<usize, Error>` operating directly on stack slots), not the index-based `lua_*` API. The index-based API exists only in `rn-capi` as a thin layer over internal functions. This keeps C-API compat from taxing performance.
- C → Lua error: `lua_error`/`luaL_error` called from C must exit through arbitrary C frames. Choice: `extern "C-unwind"` + Rust panic as the unwinding mechanism (sound on x86-64/aarch64 Linux where gcc/clang emit `.eh_frame` by default). Rules (each tested in the spike):
  1. **Every `lua_*`/`luaL_*` entry that can raise converts its internal `Result` error into a panic** — not only `lua_error`: `lua_gettable` metamethods, `lua_call`, memory errors, `lua_arith`, etc.
  2. **A panic only ever crosses foreign (C) frames plus one API frame.** `catch_unwind` at every Lua→C call site and at `lua_pcallk`/`lua_resume` entry. Panics never cross `execute` frames.
  3. Raise with `std::panic::resume_unwind` (skips the panic hook; otherwise every Lua error prints "thread panicked"). Payload is a private marker; the real error value lives in the `State`.
  4. **Every C callback type is declared `extern "C-unwind"`** on the Rust side (`lua_CFunction`, `lua_KFunction`, `lua_Alloc`, `lua_Reader`, `lua_Writer`, `lua_Hook`, `lua_WarnFunction`); a panic through plain `extern "C"` aborts.
  5. Unprotected errors: keep a protected-depth counter; if an error is raised with no catcher, call `lua_atpanic` handler then abort with a message.
  6. Unsupported/documented: frames built with `-fno-asynchronous-unwind-tables`, i686, C++ modules (`catch(...)` swallows Lua errors; destructors run), Windows SEH.
  7. Shipped profile is `panic = "unwind"`; measure the cost (a panic is microseconds vs ~20 ns for longjmp; only C-originated errors pay it).
- **Phase 0 spike deliverable** (orchestrator, using the *vendored* headers): minimal `rn-capi` with `lua_newstate/lua_pushcfunction/lua_pcall/lua_error`; C tests for: `lua_error` through two C frames caught by `lua_pcall`; error raised by a metamethod during `lua_gettable` from C; `lua_callk` across yield+resume then continuation runs; `lua_yieldk` from a C function (a `luaD_throw` in PUC); the C vararg shim (`lua_pushfstring`/`luaL_error`) calling back into Rust; `valgrind` status on this host (already verified: glibc builds SIGILL; musl-target builds work — record in `docs/TOOLCHAIN.md`; primary signal remains `perf stat -e instructions:u`). Go/no-go recorded in `docs/ARCH.md`. If no-go, fallback: documented reduced C-API scope (reported to user).
- `lua_State*` = pointer to the `Thread` object; `LUA_EXTRASPACE` bytes allocated *before* the thread header so `lua_getextraspace` macro (pointer arithmetic on `L`) works. `luaL_Buffer`, `lua_Debug`, `luaL_Reg`, `luaL_Stream` are `#[repr(C)]` and checked by compile-time size/offset assertions against C via a `build.rs`-generated test using the vendored headers (bindgen-free: a C test file compiled by `cc` crate in `[dev-dependencies]` build script only).

### 4.6 GC
- Precise tracing, tri-colour, same observable semantics as ref: `collectgarbage` options **per version** (5.4: `setpause`/`setstepmul`; 5.5: `"param"` with `LUA_GCPARAM`/`LUA_GCP*`, `setpause` raises "invalid option"; `LUA_GCGEN`/`LUA_GCINC` constants differ), `__gc` (resurrection, order, finalizers run at close), weak/ephemeron tables, `__close`, `__mode`. **Collection happens only at explicit safepoints** (the equivalents of `luaC_checkGC`/`luaC_condGC`), never inside an arbitrary allocation, exactly like PUC.
- Staged delivery (owner: B): **(G1)** stop-the-world mark-sweep, correct, with write barriers *stubbed but call sites present*; **(G2)** incremental with barriers (forward barrier for tables/closures/upvalues, backward for tables; `luaC_barrier` equivalents) + threshold/param tuning; **(G3)** generational mode (age bits, minor/major, touched lists). **G3 is a Phase 4 deliverable and required for G-parity in both versions** because the reference `lua.c` switches to generational at startup (verified in vendored 5.4.9 and 5.5.1), so allocation-heavy baselines (e.g. binary-trees) run generational; the library default (`lua_newstate`) stays incremental. No aliasing generational→incremental. Behavior validated by official `gengc.lua`.
- Roots: main thread, registry, global metatables per basic type, open upvalues, thread stacks (scan up to `top`, clear the rest), string cache, and any Rust-held `Gc` handles created through the embedding API (pinned via registry refs; no reliance on the Rust stack scan).
- Safe-layer rule: no `Gc<T>` is ever held across a possible GC point unless anchored on the Lua stack/registry; enforced by API shape (handles borrow `&Lua` and are branded with a lifetime or are registry refs).

### 4.7 Standard library
- Port function by function, same error messages (they are tested). Pattern matching (`lstrlib.c`), `string.format` (`%a`, `%q`, `%g`), `string.pack/unpack`, `os.date/time` (via `libc`-free `std` where possible; `strftime`/`localtime_r`/`setlocale` need `libc` FFI — acceptable: **"pure Rust" = no C source compiled into the product; the `libc` crate (FFI declarations only) and system libc symbols are allowed**), `io` (buffered `FILE*` semantics via `libc` `FILE*` for `popen`/`setvbuf`/`io.lines` compat — decide in Phase 4 whether to use `libc::FILE*` (max compat) or a Rust reimplementation (more safe); default: libc `FILE*` behind a safe wrapper), `package` (`require`, searchers, `package.loadlib` via `dlopen` — needed by C-modules), `utf8`, `debug` (hooks: `lua_sethook` count/line/call/return are part of VM design, implement the "trap" check in the interpreter from the start).
- **`io` decision (settled): libc `FILE*` behind a safe wrapper**, because C modules (e.g. lfs) use `luaL_Stream`/`FILE*`. A pure-Rust `io` is not an option for C-API compat.
- An internal Rust **auxlib** layer (argument-error messages `luaL_argerror`/`luaL_typeerror`, `luaL_tolstring` + `__name`, `luaL_where`, traceback) is built in Phase 2/3 (owner A) because stdlib error text depends on it; `rn-capi`'s `luaL_*` are thin wrappers over it.

---------------------------------------------------------------------------

## 5. Performance design (where speed is won or lost)

Parity-first techniques (must-have, mirror PUC's tricks):
1. 16-byte tagged `Value`; tag compare for fast paths; integer/float fast paths inline in arithmetic/compare/for-loop opcodes.
2. Interned short strings → `GETFIELD`/`SETFIELD`/`SELF` with constant key use `get_short_str` (pointer compare in node chain).
3. Table metamethod-absence flags; `__index` chains iterated, not recursed, bounded by `MAXTAGLOOP`.
4. No allocation on Lua→Lua call; `CallInfo` array reused; stack check once per call (`luaD_checkstack` analogue).
5. Opcode superinstruction semantics as in 5.5 (`ADDI`, `ADDK`, `EQK`, `GETI`, `LFALSESKIP`, `TFORCALL` etc.) — already in bytecode; we just implement them.
6. Cold-path outlining (`#[cold]`, `#[inline(never)]`) so the hot loop stays small; `likely/unlikely` via `#[cold]` helper pattern (stable Rust has no `likely`).
7. Bounds-check elimination in `execute` via raw pointers + `debug_assert!`; every raw access is inside the allowlisted `vm_unsafe` module.

Beyond-parity experiments (Phase 7, each is a measured, revertible experiment with a benchmark that justifies it):
- E1 **Inline cache for field access**: side table `Proto.ic: Box<[u32]>` indexed by pc holding last node index; verify `node[idx].key == k` before using; fall back to normal lookup. No bytecode change ⇒ V2/V5 untouched.
- E2 **Dispatch**: compare `match`-in-loop vs. threaded handlers using `become` (explicit tail calls; nightly-only unless stabilized by then — verify at that time) vs. duplicated dispatch tails via macro (`dispatch!()` at each handler end to improve branch prediction). Pick by `perf stat` branch-miss + wall time.
- E3 **Table layout**: node size/packing (key tag + value tag overlap), prefetching on chain walk, array-part fast growth for `t[#t+1]=x` append pattern, `table.insert/remove/concat/sort/unpack` specialized for array-part-only tables.
- E4 **Strings**: faster hash (e.g. ahash/wyhash-style, seeded) for long→short interning; SIMD-assisted `string.find` plain, `string.rep`, `table.concat` with exact pre-sizing; avoid double copy in `..` chains (`CONCAT n` already batches).
- E5 **GC tuning**: allocation-rate-aware step size, sweep with prefetch, size-class pools to cut malloc cost, generational parameters. The library default stays incremental and the CLI switches to generational at startup, matching `lua.c`; do not change observable `collectgarbage` behavior.
- E6 **Call/return**: specialize fixed-arg/fixed-result calls; avoid rewriting `nresults` fixups; keep `CallInfo` in contiguous cache-friendly layout.
- E7 **PGO + BOLT-style layout** via `cargo pgo` (bolt optional); reported separately as informational. Verify at E2 whether `become` (explicit tail calls) is still nightly-only and whether `std::hint::cold_path` is stable.
- E8 **Coroutine switch** cost: keep resume/yield allocation-free.
- Explicitly *not* planned: JIT, NaN-boxing, changing bytecode format, relaxing semantics.

Each experiment records: hypothesis, change, `bench` before/after (geomean + per-benchmark + `perf stat` instructions/branch-misses), keep/revert decision in `docs/PERF.md`.

---------------------------------------------------------------------------

## 6. Unsafe policy (enforced by V9)

- Every crate: `#![deny(unsafe_op_in_unsafe_fn)]`, `#![warn(clippy::undocumented_unsafe_blocks, clippy::missing_safety_doc)]`.
- `#![forbid(unsafe_code)]` on: `rn-compiler` (except where it touches GC allocation via a narrow safe `Heap` API — goal is zero), `rnlua`, `rustynail` facade internals except one `raw` module, and all of `rn-std` except a documented list (`io` FILE* wrapper, `os` time/locale/exec, `package` dlopen).
- Allowlisted `unsafe` modules in `rn-core`: `value` (tag/payload union), `gc/heap` (alloc/free/linking/sweeping), `gc/mark`, `stack` (realloc + pointer fix-up), `vm_unsafe` (interpreter hot paths), `table_raw` (node arrays). In `rn-capi`: everything is FFI so unsafe by nature; keep each `extern` fn tiny: validate index → call safe internal.
- `docs/UNSAFE.md` lists each module, the invariants it relies on, and which tests cover them. A new `unsafe` block requires the invariant in a `// SAFETY:` comment and, where feasible, a `debug_assert!` that checks it plus a Miri-runnable unit test.
- Never panic on Lua-controlled input (`unwrap`/indexing/arith overflow audited; `clippy::unwrap_used`, `indexing_slicing` denied in compiler/std outside tests). Resource exhaustion (memory, deep recursion, huge string.rep) returns Lua errors, never aborts or hangs: `LUAI_MAXCCALLS`, stack limit (`LUAI_MAXSTACK` = 1,000,000), allocation failure → "not enough memory".
- **Binary chunks are trusted** (no bytecode verifier); sandbox mode forces text-only `load`. The opcode decode (`#[repr(u8)]` enum from a 7-bit field) must be checked or proven total so malformed *loading* never causes UB.
- **Rust stack budget:** each metamethod/C re-entry recurses `execute` up to the `LUAI_MAXCCALLS` analogue (200) times the frame size, and LTO inlining can enlarge frames. Measure frame size in Phase 2 (B4), set the limit accordingly, and run tests/embedders on a thread with an explicit large stack (`RUST_MIN_STACK`/`thread::Builder::stack_size`). `lua_sethook` must use atomic stores only (the `lua.c` SIGINT handler calls it).

### 6.1 Sandboxing and resource limits (first-class features, tested)
Memory limit via the allocator abstraction; instruction/step limits via hooks; text-only load mode; ability to omit `io`/`os`/`package`/`debug` libs; PUC's nesting limits reproduced (parser nesting `LUAI_MAXCCALLS`, pattern-matching recursion `MAXCCALLS`, `string.rep`/`string.format`/`table.concat` size checks, stack limit, C-stack overflow errors). Fuzz targets run with these limits enabled so they cannot hang or OOM.

---------------------------------------------------------------------------

## 7. Benchmarks and measurement

Suite in `bench/` (each prints a checksum that must match ref; runtime targeted 1–5 s on ref):
fib (recursive calls), loops/arith int+float, nbody, spectral-norm, fannkuch-redux, mandelbrot, binary-trees (GC alloc heavy), richards/deltablue-style OOP + method calls, table array append/read/sort/insert/remove, hash tables with string/int keys, string concat/format/rep/sub/find/gsub/gmatch, closure create/call/upvalue, varargs/select/table.unpack, coroutine ping-pong and generators, `pcall`/`error` throw-catch, metamethod-heavy (`__index` chain, `__add`, `__call`), k-nucleotide, fasta, json encode/decode in Lua, and a "startup + load big file" benchmark (compiler speed). Plus GC-stress memory benchmarks tracking peak RSS.

Method:
- Runner: `xtask bench` → for each benchmark: warm-up, ≥10 runs, pin to a core (`taskset`), `perf stat -e instructions:u,cycles:u,branch-misses:u`, wall time via hyperfine-style stats (median, MAD). **`valgrind` crashes (SIGILL in `ld.so`) on glibc binaries on this host** (Arch Linux znver4 glibc uses AVX-512; valgrind 3.25 lacks it). **Verified workaround: build with `--target x86_64-unknown-linux-musl`** (static, skips the system loader); cachegrind then agrees with `perf stat` instruction counts within 0.1%. Policy: `perf stat -e instructions:u` (works at `perf_event_paranoid`=2) is the primary low-noise signal and is used for all Rust-vs-C comparisons; musl+cachegrind/callgrind is an optional extra for per-function instruction profiles of the Rust build (musl `malloc` differs, so relative regressions only; the C reference would need its own musl build to be compared this way; `dlopen`/C-module tests are not run in musl builds). Governor set to performance if possible; noisy runs flagged and repeated.
- Baselines stored in `bench/baselines/<machine-id>.json`: ref-gcc, ref-clang, ref-PGO, luajit -joff (info), ours. Report = table of ratio (ours/ref time) per benchmark + geomean, plus peak RSS ratio, plus compile-time (`load`) throughput.
- Gates (apply per version, each vs its own default-build PUC reference): **G-parity** (end of Phase 6 / 6b): geomean ≤ 1.05× ref time, no benchmark worse than 1.35×. **G-target** (Phase 7): geomean ≤ 0.90× (≥10% faster) vs the default `make` reference build, stretch 0.77×; PGO-vs-PGO ratios are reported for information only (beating a PGO'd C build by 10% without a JIT and with identical bytecode is very ambitious). V10 is report-only before G-parity.

---------------------------------------------------------------------------

## 8. Phases, work breakdown and builder assignment

Roles
- **Orchestrator (main agent)**: Phase 0–1 itself; defines interface contracts; writes task briefs; runs gates; merges; owns `PLAN.md`, `STATUS.md`, `PORT_MAP.md`, `UNSAFE.md`, `PERF.md`; reviews every new `unsafe`; keeps builders unblocked. Does not write bulk port code except spikes/scaffolding.
- **Builder A — "Front-end & Library"**: compiler, dump/undump, stdlib, CLI, package.
- **Builder B — "Runtime & Perf"**: value/string/table/GC/interpreter/coroutines/C API.
- **Load rebalancing (B previously held ~70% of the hard work):** A also owns the `lapi.c` index/pseudo-index layer, `ldebug.c` (`getobjname`/varinfo — bytecode symbolic execution is compiler knowledge — and `luaL_traceback`), the internal auxlib, `luac` listing, and the Ctrl-C/`lua_sethook` REPL piece (coordinate with B's hook design). B keeps value/string/table/GC (G1–G3)/stack/interpreter/coroutines/continuations/error machinery and then perf. Removed duplicate ownership: `luaL_traceback` is A only.
- Each builder works in its own **git worktree** (`git worktree add ../rustynail-A -b builderA/<task>`), delivers small, reviewable commits per task, never edits files owned by the other builder without a contract change from the orchestrator. Merge only after `cargo xtask ci` is green and `PORT_MAP.md` updated.

### Phase 0 — Environment, vendoring, scaffolding, spikes (orchestrator; builders may start A1/A3/B1 early, see below)
- P0.1 User runs `scripts/setup.sh` (§1.1). P0.2 `git init`, workspace skeleton, profiles, `xtask`, lint config (incl. `disallowed_types` for std collections in core).
- P0.2b **Vendor sources and write `docs/VERSION_DIFF.md`** (§2, §0.1) and `xtask ref-build` — these precede any interface contract.
- P0.3 **C-API unwinding / vararg-shim / valgrind spike (§4.5)** → go/no-go note in `docs/ARCH.md`.
- P0.4 Write `docs/ARCH.md` with the **interface contracts** builders depend on: `opcodes.rs` (exact 5.5 opcode enum + encode/decode, copied/ported by orchestrator from `lopcodes.h` and unit-tested), `Proto` struct, `Value`/`Gc` API surface, `Heap`/`Ctx` API the compiler uses to create strings/protos/constants, error type, `Loader` hook signature. Also fixes: `Proto`, CallInfo/continuation shape (§4.5), error `Status` type, allocator/fallible-buffer API, string layout incl. external strings. Provide a **stub `Heap`/string-creation API** so Builder A is not blocked on B1.
- Exit: `cargo build` and `cargo xtask ci` run on an empty skeleton; spike result recorded.

### Phase 1 — Reference + harness (orchestrator, overlapping with early builder work)
- P1.1 (vendoring done in P0.2b) Build V2/V3/V4 runners; port `luac`'s `print.c` listing for V2.
- **Early builder work during Phase 0/1 (avoid idle builders):** Builder A starts A3 undump (small; lets B4 later run real `ref/luac` output) and A1 lexer; Builder B starts B1 (value, number formatting, strings) once the P0.4 contracts and allocator API are frozen.
- Exit: `xtask official` runs both official suites on their refs and reports baselines; `docs/VERSION_DIFF.md` written; `xtask bench --ref-only` produces a baseline JSON.

### Phase 2 — Parallel core (A ∥ B)
Builder A (front-end):
- A1 `lexer` (llex.c): tokens, numerals (hex floats, `0x` ints wrap), long strings/comments, escapes (`\z \x \u{} \ddd`), error messages/line numbers. Tests: V1 + token-stream diff vs ref `luac -l` where feasible.
- A2 `parser` + `codegen` (lparser.c, lcode.c): expression descriptors, register allocation, constant folding, jump patching, `goto`/labels, attribs `<const>/<close>`, **5.5 `global` declarations and read-only for-loop control variable** (read 5.5 manual §3 and upstream source; don't trust memory of 5.4), integer-for semantics, varargs. Acceptance: **V2 bytecode-identical** on the full corpus.
- A3 `dump`/`undump` (ldump.c, lundump.c), header check, malformed-input robustness (fuzz target).
Builder B (runtime):
- B1 `value`, `object`, number conversion/formatting (lobject.c: `luaO_str2num`, `tostringbuff`, `%g`), `string` + interning, `Heap`/allocator abstraction (lmem.c), G1 GC.
- B2 `table` (ltable.c 5.5) with unit tests + property tests against a `HashMap` model, including `next` iteration-with-modification rules and `#` border behavior vs ref (differential test generating random op sequences, run in both ref and ours).
- B3 `State`/`Thread`/stack/`CallInfo`/upvalues (lstate.c, ldo.c basics, lfunc.c), metamethod lookup (ltm.c).
- B4 interpreter `execute` (lvm.c) for all opcodes except coroutine/yield bits; run hand-built `Proto`s from unit tests until A2 lands.
- Sync point S2: orchestrator merges A1–A3 and B1–B4; wires `rnlua` `-e`/file run. Exit: `print(("hello"):rep(3))`, fib, loops run; V2 passes 100% on corpus; first official-suite files pass (`constructs.lua`, `math.lua`, `calls.lua`…).

### Phase 3 — Stdlib bring-up and test-suite climb
- Builder A: base lib (`print tostring tonumber pairs ipairs next select rawget/rawset/rawequal/rawlen setmetatable getmetatable assert error pcall xpcall load loadfile dofile collectgarbage`), `string` (pattern matching, format, pack/unpack, rep, sub, byte/char, upper/lower, len, reverse, gsub/gmatch/find/match), `table`, `math` (xoshiro256** PRNG, **same seeding/algorithm as ref** so `math.randomseed(n)` sequences match), `utf8`, `os`, `io`, CLI behaviors.
- Builder B: error handling & `pcall` (ldo.c: `luaD_throw` analogue, `luaD_rawrunprotected`), tracebacks/`luaL_traceback`, error-message variable naming (`ldebug.c`: "attempt to index a nil value (field 'x')" — heavily tested), `__close`/tbc variables, `__gc` + weak tables (G1 completion), integer/float edge cases.
- Exit: official suite pass-list ≥ 60% of files; V3 clean on our own corpus.

### Phase 4 — Coroutines, debug lib, incremental GC
- B: coroutines (`lua_resume/yield`, `coroutine.*`, wrap, close, `isyieldable`, yields in metamethods/`pcall`/iterators via continuations), hooks (`debug.sethook`), G2 incremental GC with barriers, GC stress mode. **Also G3 generational GC (required for 5.5 parity, §4.6).**
- A: `debug` library, `package`/`require` (path/cpath searchers, preload, `package.loadlib` with `dlopen`), `io` completion, locale/`os` details, REPL (`-i`, multi-line continuation, `_PROMPT`), `arg`, `LUA_INIT`, signal handling for Ctrl-C (`lua.c` uses `lua_sethook` + `SIGINT`).
- Exit: official suite ≥ 95% of ref-passing files; V8 (Miri/ASan/GC-stress) green.

### Phase 5 — C API (B leads, A supports) 
- B: `rn-capi` exports for **every** symbol in `lua.h`/`lauxlib.h`/`lualib.h` (list generated by script from headers; CI fails if any symbol missing via `nm`); index-to-slot translation (positive/negative/pseudo-indices/upvalue indices), `lua_Debug` fill, `lua_newstate` custom allocator, `lua_atpanic`, `lua_setwarnf`, `lua_closethread`, `lua_toclose`, etc.
- A: port `lauxlib.c` (`luaL_*`, `luaL_Buffer` with the exact box/userdata strategy so `luaL_Buffer` behavior matches), `linit.c` (`luaL_openlibs`), `loadlib` hooks; build `tests/c/` (vendored `lua.c` + samples + real-world modules) and `xtask capi-test`.
- Exit: vendored `lua.c` linked to the `rustynail` C-API library passes the official suite same as the pure-Rust CLI; real C modules load and work, **including a prebuilt `.so` module that resolves `lua_*` symbols from the host executable — so `rnlua` must export them dynamically (`-C link-arg=-Wl,--export-dynamic` + `#[no_mangle]` retained under LTO) and a test `require`s such a module**; ABI struct layout assertions pass; `lua_newthread` copies the main thread's extra space (aligned).
- *Scheduling note*: the orchestrator may start Phase 5 earlier for B’s idle time but C-API must not dictate internal layout; internal fast path stays primary.

### Phase 6 — Parity hardening and the parity gate
- B: profile (`perf record`, flamegraph, `cargo-pgo`), fix hot spots until **G-parity** (§7) is met; A: fix remaining official-test failures, message/format discrepancies found by V3/V7; both: fuzz campaign (≥ 24 h aggregate CPU per target), triage and fix.
- Exit: G-parity, 100% of ref-passing official files, fuzzers clean for a defined budget, `UNSAFE.md` complete.

### Phase 6b — Lua 5.4 support (A and B)
- Fill in `ver::v54`: opcodes, table array policy, parser/lexer deltas, C API header deltas, stdlib deltas, all driven by `docs/VERSION_DIFF.md`.
- Run the full harness matrix for 5.4 (V2 bytecode diff, V4 official 5.4 suite, V6 C API, V7 fuzz, V10 perf vs ref 5.4). Exit: 5.4 reaches the same gates as 5.5 (G-parity, 100% of ref-passing official files).

### Phase 7 — Optimization campaign (beyond parity)
- B runs experiments E1–E8 sequentially (one change per commit, benchmark evidence per commit); A builds bench tooling improvements, adds benchmarks covering weak spots, implements std-library fast paths (E3/E4) and G3 generational GC if not done earlier; orchestrator arbitrates keep/revert from numbers, re-runs full V1–V10 after every kept experiment.
- Exit: **G-target** or documented plateau with profile evidence of why.

### Phase 8 — Release hardening
- Facade `rustynail` safe embedding API (+ docs/examples), `cargo doc`, README, versioned release profile with PGO build script, `cargo deny`/license audit, reproducible build notes, final `docs/PERF.md` report vs ref (and LuaJIT interpreter for context).

---------------------------------------------------------------------------

## 9. Builder task brief template (orchestrator fills in per task)

```
Task ID / title:
Branch & worktree:
Goal (1–2 sentences):
Upstream C reference files + functions (path:line):
Rust files you own / may create:
Files you must NOT touch:
Interfaces you implement / consume (link to ARCH.md section):
Acceptance tests (exact commands), e.g.  cargo nextest run -p rn-compiler ; cargo xtask diff-bytecode --corpus tests/lua ; cargo xtask ci
Perf constraints (if any):
Unsafe allowance: none | module X, invariant Y
Done = tests green + PORT_MAP.md updated + short handoff note appended to docs/STATUS.md (what's done, known gaps, surprises)
```
Rules of engagement for builders:
1. Read the upstream C function and the 5.5 manual section **before** writing Rust; port semantics, not Rust-idiomatic reinterpretations, when behavior is observable (error messages, evaluation order, edge cases).
2. Tests first for each behavior; if the ref binary can answer a question, ask it (`ref/lua -e ...`) instead of guessing.
3. Keep commits small; run `cargo xtask ci` before handing off; never `#[allow]` a lint without a comment.
4. Anything unclear in a contract → write the question in `docs/STATUS.md` and ask the orchestrator; do not invent a cross-boundary API.
5. No benchmarks claims without `xtask bench` output attached.

Orchestrator cadence: at each sync point (S2 end of Phase 2; then end of each phase) merge both branches, run `xtask ci --full`, update `STATUS.md` (pass counts, bench ratios, open risks), and rewrite the next task briefs. Builders rebase on main at each sync point.

---------------------------------------------------------------------------

## 10. Risks and mitigations

| Risk | Impact | Mitigation |
|---|---|---|
| Unwinding through C frames unsound/unportable (§4.5) | C-API scope | Phase 0 spike, go/no-go, documented fallback |
| Parity not reached (Rust codegen in `execute` worse than gcc computed-goto) | speed goal | early profiling at B4; E2 dispatch experiments; PGO; inspect asm for the 10 hottest opcodes; accept parity+ISA-specific wins |
| 5.4/5.5 semantics differ from each other and from memory | correctness | always port from the vendored source of the matching version; `VERSION_DIFF.md`; V2/V4 catch drift |
| Stack-realloc dangling pointers, GC-barrier omissions | memory unsafety | funnel helpers, debug generation counter, GC stress + ASan + Miri, fuzzing |
| `pairs` order / hash differences | flaky diffs | sort keys in test scripts, fixed seed, lint |
| Float formatting/parsing mismatches | wrong output | per-version formatting (5.5 `%.15g`/`%.17g`, 5.4 `%.14g`), randomized comparison against libc |
| Binary-chunk exploit (no verifier) | memory unsafety on hostile bytecode | trusted-chunk policy documented; text-only sandbox load mode (§6) |
| C-variadic API needs C shim | "pure Rust" purity, build complexity | single ~30-line audited shim, in P0.3 spike, listed in UNSAFE.md |
| valgrind SIGILLs on glibc binaries (znver4 host) | CI noise | `perf stat` primary; musl-target builds work under valgrind (verified) |
| Custom allocator/OOM vs std collections | correctness of `lua_Alloc`/OOM tests | own fallible buffers; `disallowed_types` lint |
| Builder interface drift | integration pain | contracts in ARCH.md, orchestrator-only changes, sync points |
| Benchmark noise | false regressions | instruction-count gate, pinning, multiple runs, MAD-based thresholds |
| Scope (C API + generational GC + full stdlib) is large | schedule | staged GC (G1→G3), parity gate before experiments, stdlib ported in test-suite order |
| Licensing | legal | Lua is MIT; keep notices; no GPL code (e.g. don't copy LuaJIT/others) |

---------------------------------------------------------------------------

## 11. Decisions and remaining open items

Settled (user): Lua 5.5 primary + 5.4; contained unsafe; C API ABI compat; parity then +10–30%; tiny audited C shim for C-variadics; binary chunks trusted with text-only sandbox mode; apply all Opus review fixes.

Settled by plan: prebuilt `.so` modules are supported (rnlua exports `lua_*` dynamically); `libc` crate allowed; `panic = "unwind"` shipped (cost measured); `io` uses libc `FILE*`; generational GC required for G-parity in both versions (CLI switches at startup like `lua.c`); ltests/`T` torture tests out of scope; project license MIT (matches Lua) with `LICENSE-LUA` notice.

Settled (user, latest): **x86-64 Linux first; aarch64 later** (post-v1 milestone, see Phase 9). Env vars and artifact names use the `RUSTYNAIL_*` / `rustynail` prefix everywhere.

Still open: none.

### Architecture scope notes
- v1 targets x86-64 Linux only. Do not write x86-specific code outside `arch/` modules (SIMD string search, any inline asm, `target-cpu` flags), so aarch64 is a port, not a rewrite.
- CI and the C-API unwinding spike run on x86-64 only for v1. The aarch64 port must re-run the spike (unwind tables, `extern "C-unwind"`), the full V1-V10 matrix, and re-baseline benchmarks against aarch64 PUC builds.

### Phase 9 — aarch64 port (after v1, not part of the parity/target gates)
- Add `aarch64-unknown-linux-gnu` to CI (cross-build + qemu or native runner); re-run P0.3 spike items; port `arch/` fast paths (NEON where useful); re-tune dispatch/PGO; verify `perf stat` availability; repeat G-parity per version on aarch64.
