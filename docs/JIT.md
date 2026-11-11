# The JIT compiler

rustynail has an x86-64 JIT for Lua bytecode (crate `rn-jit`): a template compiler, typed loop regions and a trace
compiler. **In the release binaries it is on by default**: `jit` is a default cargo feature of the `rn-cli` package
(`rnlua`), and with `RUSTYNAIL_JIT` unset a process runs with tier 1 and the trace compiler on. It is **off by default for
embedders** (the `rustynail` crate: opt-in feature `jit`, and a `Lua` state runs interpreted until `set_jit_mode`).
Design and history: `docs/JIT_PLAN.md`, `docs/JIT_DESIGN.md`; testing: `docs/JIT_TESTING.md`; the long list of trace-compiler
knobs and the results ledger: `docs/JIT_TRACE_REF.md`; `unsafe` inventory: `docs/UNSAFE.md`.

## Summary

**Design in one paragraph.** Every function runs in the interpreter first. The interpreter counts calls and loop back-edges
(hooks of `rn-core`, no clock: decisions depend on counters only, so runs are deterministic). A hot function or loop is
compiled by tier 1, a one-pass template compiler (one fixed machine-code sequence per bytecode instruction; anything
without a template, and every slow path, *exits* to the interpreter at the same bytecode position). Hot loop nests are
then recompiled by tier 1.5 with typed registers, and the trace compiler (tier 2) records hot loops, across function
boundaries, as straight-line SSA with guards, optimises it (forwarding, bounds-check and guard elimination, allocation
sinking, loop-invariant motion, register allocation) and emits x86-64; a failed guard leaves the trace through an exit
stub that restores interpreter state, and hot exits get side traces. Compiled frames are ordinary Lua frames.

**Invariants** (each has tests, see Testing):

1. *Semantics are the interpreter's.* Results, error messages, line numbers, `debug` library behaviour, tracebacks, hooks,
   finalizer and `__gc` semantics are identical with the JIT on and off (differential tests against the interpreter and the
   reference Lua; the GC observables of allocation sinking are the one documented drift, see "drift note" below, switch
   `RUSTYNAIL_JIT_TRACE_OPT=-sink`).
2. *Compiled code never outlives its prototype* and stale code pointers fault instead of running (quarantined arena in
   `RUSTYNAIL_JIT=verify`).
3. *W^X.* No page is ever writable and executable at the same time.
4. *Deterministic policy.* Hotness decisions use counters, never clocks or threads; the same program compiles the same way.
5. *Resource accounting.* Code pages and JIT structures count against the state's memory limit; an instruction limit disables
   compiled code, and active hooks are honoured (tests `jit_review_hooks`, `tr_hook`).
6. *Graceful degradation.* Any refusal (no executable memory, memory limit, code cap, unsupported instruction) leaves the
   function in the interpreter without an error.

**Flags and environment knobs** (the full trace-compiler list is in `docs/JIT_TRACE_REF.md`):

| knob | effect | default |
|---|---|---|
| cargo feature `jit` of `rn-cli` | compile the JIT into `rnlua` (`--no-default-features` removes it; x86-64 only) | on |
| cargo feature `jit_j4` | template JIT + regions without the trace compiler (use with `--no-default-features`) | off |
| `RUSTYNAIL_JIT=off\|on\|always\|verify` | run-time mode: `off` interpreter only; `on` compile hot code; `always` compile at first call; `verify` = `always` + quarantined arena | `on` (unset) |
| `RUSTYNAIL_JIT_TRACE=off\|on\|always` | trace compiler; `off` = tier 1 + regions only | `on` (unset) |
| `RUSTYNAIL_JIT_TRACE_LIMITS=k=v,...` | trace policy limits and rules (`docs/JIT_TRACE_REF.md`) | built-in |
| `RUSTYNAIL_JIT_TRACE_OPT=-sink,...` | trace optimisation switches | all on |
| `RUSTYNAIL_JIT_T15=off\|on\|always`, `RUSTYNAIL_JIT_T15_HOT=n` | tier 1.5 regions and their promotion threshold | on with traces |
| `RUSTYNAIL_JIT_THRESHOLD=calls,edges` | hotness thresholds of tier 1 | about 200 calls, 1000 back-edges |
| `RUSTYNAIL_JIT_CODE_CAP=bytes` | cap on mapped code per state | 256 MiB |
| `RUSTYNAIL_JIT_VERBOSE=1` | compile/exit/trace profile on stderr when the state closes | off |
| `RUSTYNAIL_JIT_PERFMAP=1` | write `/tmp/perf-<pid>.map` | off |
| `RUSTYNAIL_JIT_DUMP=dir` | write machine code of every prototype and `trace-<n>.txt` listings (IR + code) | off |
| `RUSTYNAIL_JIT_STRESS=stackmove` | test aid: every helper call moves the Lua stack | off |
| `RUSTYNAIL_SEED`, `RUSTYNAIL_GC_MODE`, `RUSTYNAIL_GC_STRESS`, `RUSTYNAIL_GC_VERIFY` | not JIT knobs, but part of the reproduction recipe (hash seed; collector mode; GC stress) | see README |

**Limits.** Code cap 256 MiB per state; hotness back-off for functions that do not pay; trace length, side-trace, inlining and
unroll limits (`RUSTYNAIL_JIT_TRACE_LIMITS`); code and structures are charged to the memory limit (see "Limits and
interaction with the facade"). Known gaps: "Known gaps" below and `docs/JIT_PARKED.md`.

**Security notes.** Code pages are mapped RW, filled, and flipped to RX with `mprotect` in batches (never RWX; no dual
mapping; see "Security notes" below for the probe and the fallbacks). Generated code is as trustworthy as the compiler: a
bug there is a memory-safety bug of the host process, so the `Builder::sandbox()` preset keeps the JIT off until the
executable-memory probe has shipped for a release, and untrusted code should run with `RUSTYNAIL_JIT=off` or in a sandboxed
embedder state. Precompiled binary chunks are trusted input with or without the JIT.

**Platforms.** x86-64 Linux. On other architectures `rn-cli` does not depend on `rn-jit` (a target-specific dependency
with `cfg(target_arch = "x86_64")`), so the default build is the interpreter and `rnlua --version` says `JIT: not built`
(checked with `cargo check --target aarch64-unknown-linux-gnu` for `rn-core`, `rn-std` and `rn-cli`; no aarch64 machine
was available to run it). Miri cannot execute machine code and `cfg(miri)` runs are interpreter-only; the IR emulator
(`emu`) runs the template logic under Miri.

**Diagnosing a problem.** First bisect with the switches: `RUSTYNAIL_JIT=off` (interpreter), `RUSTYNAIL_JIT_TRACE=off` (no
traces), `RUSTYNAIL_JIT=always` / `RUSTYNAIL_JIT_THRESHOLD=2,20` (compile early, makes a rare bug frequent),
`RUSTYNAIL_JIT_TRACE_OPT=-sink`. If the result differs between modes, it is a JIT bug: keep the script, `RUSTYNAIL_SEED`, the mode,
and `RUSTYNAIL_JIT_VERBOSE=1` output (exit profile: which instructions leave compiled code, which traces were recorded,
aborted or blacklisted and why). `RUSTYNAIL_JIT_PERFMAP=1` lets `perf` attribute samples to compiled Lua functions
(`lua_proto@...`); `RUSTYNAIL_JIT_DUMP=dir` writes the code for disassembly (`objdump -D -b binary -mi386:x86-64`) and the trace
IR listings. `scripts/jit-diff.sh` (generator-based differential fuzzing over many modes) and `scripts/jit-minimize.py`
shrink a failing program.

**Test strategy** (details in `docs/JIT_TESTING.md`): the IR emulator under Miri; per-opcode and exit tests for tier 1; a
region-analysis oracle; trace-compiler suites (`tr_*`: recording, optimiser passes, assembler, exits, register allocation,
hooks, coroutines, determinism) with a registry of about 470 deliberate-bug mutants that the suites must detect; differential
fuzzing (`scripts/jit-diff.sh`, cargo-fuzz targets with the JIT); the official Lua test suites run with the JIT in several
modes; ASan runs (`scripts/asan-jit.sh`); GC stress modes; instruction-count identity checks (`scripts/jit-text-identity.sh`)
proving that a build without the feature is byte-identical to one without JIT support.

**Verified performance** (`bench/results/session-release/report.md`, 223 programs, min of 5 interleaved runs, pinned CPUs, machine
idle, cycles; ratios below 1 mean rustynail is faster):

| comparison (geomean) | Lua 5.5 plain | Lua 5.5 PGO (release build) | Lua 5.4 plain | Lua 5.4 PGO |
|---|---|---|---|---|
| rustynail / LuaJIT 2.1 (219 programs LuaJIT can run; 5.4: all 223) | **0.995** | **0.905** | 1.184 | 1.081 |
| rustynail / PUC Lua (same version) | **0.230** | **0.210** | 0.268 | 0.245 |
| JIT on / JIT off (same binary) | 0.314 | | 0.354 | |

PGO / plain on 5.5: 0.913 cycles. LuaJIT stays ahead on a few programs (`fib`, `raytrace`, `lua_lexer`, `lib/char` under PGO,
`classic_vector`, `tail_loop`; see the report's gap list); on 5.4 the trace compiler has more gaps (`rawget`/`rawset`/`next`,
inline `#t`, integer-key hash accesses, metamethod-heavy programs), all listed in `docs/JIT_PARKED.md`. Method, per-set
tables and caveats are in the report.

## What it is

Three tiers share one code arena, one hotness mechanism and the interpreter's frame layout:

1. **Tier 1, template compiler** (always part of the `jit` feature; this section). No type inference.
2. **Tier 1.5, typed regions** (J4, `RUSTYNAIL_JIT_T15`, `docs/JIT_TIER15.md`): hot loop nests of one prototype are recompiled
   with typed registers and inline table/intrinsic fast paths.
3. **Tier 2, trace compiler** (J5, cargo feature `trace`, `RUSTYNAIL_JIT_TRACE`, `docs/JIT_TRACE.md`; overview in the section
   "J5 trace compiler" below): hot loops are recorded as straight-line SSA across function boundaries, optimised and compiled;
   guards exit back into tier 1 code of the same frame.

The following paragraphs describe tier 1, which is also what every exit of the higher tiers returns to.

A one-pass template compiler: every bytecode instruction of a hot function (or loop) becomes a short, fixed
machine-code sequence; instructions without a template (and every slow path: metamethods, errors, hooks,
stack growth, ...) *exit* to the interpreter, which carries on from the same bytecode position. There is no
type inference at this tier, and the Lua semantics, error messages, line information, stack layout,
`debug` library behaviour and garbage collector are those of the interpreter (compiled frames are ordinary
frames; `debug.getinfo`, tracebacks and `getlocal` are exercised on JIT runs by the differential fuzzer). Results must be identical with the JIT on
and off: that is what the test suites compare (below). The cost of a fast path that does not apply is a
round trip into the interpreter (about 150 cycles), which a per-function back-off keeps from dominating.

It supports Lua 5.5 and Lua 5.4 builds (the templates are shared where the VM layouts agree).

**Inline built-in calls (`t1inline`, default 1023 = all nine on).** The `CALL` template can run the fast path of the cheapest very hot built-ins itself
(`type`, `math.floor/abs/max/min/sqrt`, `string.byte/len`, `select('#', ...)`, `rawlen`), guarded on the callee's identity (the registered
function pointer, so a user-replaced `type` or `math.floor` takes the generic call) and with the generic `jit_call` path for everything
unusual (other argument types or counts, errors, hooks). Table-driven (`crates/rn-jit/src/t1inline.rs`); selected with
`RUSTYNAIL_JIT_TRACE_LIMITS=t1inline=<mask>` (a bit per built-in; `t1inline=0` turns them all off). It shipped off at first (one program's trace
policy was perturbed) and is on in the shipped configuration (`T1INLINE_PROCESS` in `crates/rn-jit/src/tr/cache.rs`); design, numbers and tests:
`docs/JIT_TRACE_REF.md` "BUNDLE-T1I", ledger entry in `docs/JIT_PARKED.md`.

## How to enable and disable

Scripts can query and switch the compiler of their own state through the global `jit` table of `rnlua` (`jit.status()`, `jit.on()`,
`jit.off()`, `jit.flush()`; LuaJIT-compatible shape, per-function forms ignored); see `docs/EXTENSIONS.md`.

`rnlua` has the JIT by default (no flag needed). Build-time removal: `cargo build --release -p rn-cli --no-default-features`
(the machine code of the interpreter is then identical to a build without JIT support). Run-time off: `RUSTYNAIL_JIT=off`.
For the embedding crate, `cargo build -p rustynail --features jit` compiles it in and `Lua::set_jit_mode(JitMode::On)` turns it on.

Build with the cargo feature (the feature adds about 1.2% instructions to the interpreter's hot paths
for the hotness counters; without it the machine code of the interpreter is identical to a build without JIT support, checked in CI):

    cargo build --release -p rn-cli                       # rnlua with the JIT (default feature)
    cargo build --release -p rn-cli --no-default-features # rnlua without it
    cargo build --release -p rustynail --features jit      # the embedding crate (opt-in there)

`rnlua` (standalone interpreter) reads the environment:

| variable | meaning |
|---|---|
| `RUSTYNAIL_JIT=off\|on\|always\|verify` | `on` (default when the feature is built in): compile hot code (about 200 calls / 1000 loop back-edges, back-off for functions that do not pay). `always`: compile everything at its first call (testing). `verify`: `always` plus a quarantined arena so stale code faults. `off`: interpreter only |
| `RUSTYNAIL_JIT_THRESHOLD=calls,edges` | override the hotness thresholds |
| `RUSTYNAIL_JIT_VERBOSE=1` | print a compile/exit profile to stderr when the state closes |
| `RUSTYNAIL_JIT_PERFMAP=1` | write `/tmp/perf-<pid>.map` so `perf` shows compiled functions as `lua_proto@...` |
| `RUSTYNAIL_JIT_STRESS=stackmove` | test aid: every helper call moves the Lua stack (finds stale stack pointers in generated code) |
| `RUSTYNAIL_JIT_DUMP=dir` | debugging: write the machine code of each prototype to `dir` |
| `RUSTYNAIL_JIT_CODE_CAP=bytes` | cap on mapped code bytes per state (default 256 MiB); see "Code cap" below. API: `rn_jit::set_code_cap` |
| `RUSTYNAIL_JIT_T15=off\|on\|always`, `RUSTYNAIL_JIT_T15_HOT=n` | tier 1.5 regions (back-edges per nest before promotion, default 4096); `RUSTYNAIL_JIT_OPT` holds its test options (`+shadow`, `+exitstorm=N`, `+force`, ...) |
| `RUSTYNAIL_JIT_TRACE=off\|on\|always` | trace compiler (part of the `jit` feature; see "J5 trace compiler" below for the knob family). Unset = `on` |

The embedding API (`crates/rustynail`) does **not** enable it by default, even with the feature compiled in
and even when `RUSTYNAIL_JIT` is unset:

```rust
use rustynail::{JitMode, Lua};
let mut lua = Lua::builder().jit(JitMode::On).build()?;   // or lua.set_jit_mode(JitMode::On)
// JitMode::Off (default), JitMode::On (hot code), JitMode::Always (everything, for testing)
lua.set_jit_mode(JitMode::On); // returns whether compiled code can actually run
let stats = lua.jit_stats();   // Option<JitStats>: compiled, enters, code_bytes, refused
```

Without the feature `set_jit_mode` is a no-op returning `false`. The facade honours `RUSTYNAIL_JIT` only
as a process-wide kill switch (`RUSTYNAIL_JIT=off` installs no compiler) and for the other tuning variables.
`JitMode::Always` lowers the process-wide compile thresholds for prototypes created afterwards (they are
global, not per state).

**`Builder::sandbox()` does not enable the JIT** (plan decision D8/D5): it stays off until the
executable-memory probe described below has shipped for a release. Calling `.sandbox().jit(...)`
explicitly is possible but then at your own risk.

## Platforms

x86-64 Linux only. `rn-cli` depends on `rn-jit` only for `target_arch = "x86_64"`, so on other architectures the
default build is the interpreter and the `jit` features are inert (the `rustynail` facade crate's opt-in `jit` feature is not
architecture-gated and builds on x86-64 only). Hosts that forbid the needed memory operations (see Security) silently stay
in the interpreter.

## Security notes

- **W^X, never RWX.** Code is written into pages mapped read+write, then `mprotect`ed to read+execute in
  batches (one `mprotect` per compilation unit); a page is never writable and executable at once and an
  executable page is only flipped back after everything on it was freed. Arenas are per Lua state.
- **Probe and fallback.** The first use runs a probe (map RW, write `ret`, flip to RX, call it). On hosts that
  deny it (systemd `MemoryDenyWriteExecute`, `prctl(PR_SET_MDWE)`, SELinux `execmem`, seccomp filters) the
  JIT is disabled for the process and every state runs in the interpreter, without an error. A test
  re-executes itself under `PR_SET_MDWE` to check the probe fails cleanly (`mem::tests::probe_fails_under_mdwe`);
  `crates/rn-jit/tests/jit_j3c_env.rs` runs whole programs in child processes under `PR_SET_MDWE`, under a
  seccomp filter that fails `mmap`/`mprotect` with `PROT_EXEC` (installed before and after the probe), and
  with the address space limited so that mapping a code chunk fails: the JIT stays out, silently, with the
  interpreter's results.
  Dual-mapped (memfd) pages are not used.
- **Not for untrusted code yet.** Generated code is as trustworthy as its templates: a bug there is a memory
  safety bug of the host process. The JIT has been fuzzed (see Testing) but not for the duration the plan
  asks for before it is recommended for sandboxes. Binary chunks are trusted input with or without the JIT.
- Resource limits: see below. Compiled code is charged to the memory limit; the instruction limit disables
  compiled code (below).

## Limits and interaction with the facade

- **Memory limit.** `Lua::set_memory_limit` counts the code pages (whole pages while compiling, refunded down
  to what was sealed; compiles are packed into shared pages), and the JIT's Rust-heap structures: the
  per-function descriptor (box, resume table, monitor; refunded when the function is garbage collected) and,
  for the duration of a compilation, an estimate of the lowering buffers (256 bytes per bytecode
  instruction). The charge does not appear in `collectgarbage("count")` and does not change GC pacing. A
  refused charge is a transient failure: the function stays in the interpreter and the compile is retried
  later (after a back-off of 32 requests) when memory has been freed; nothing is aborted. The code arena also
  has a cap of 256 MiB of mapped code per state.
- **Code cap and eviction.** There is no eviction of live code: the policy is "refuse, then reuse". When a
  compile would push the arena's mapped bytes over the cap (`RUSTYNAIL_JIT_CODE_CAP` bytes, or
  `rn_jit::set_code_cap`), it is refused as a transient failure (`Stats::refused`): the function keeps
  running in the interpreter and is retried after the back-off. Code is freed with its prototype; a chunk
  that becomes completely empty is pooled or unmapped, and its space is reused by later compiles, so a program
  that keeps loading and dropping hot chunks stays under the cap (tested with thousands of generated
  functions against a 64 KiB cap). A chunk that still holds one live function stays mapped (no compaction).
  Results are identical to the interpreter in all cases (`crates/rn-jit/tests/jit_j3c_env.rs`).
- **Instruction limit and hooks.** Compiled code is never entered while a hook (count, line or call) is
  active on the thread: a state with `set_instruction_limit` or `debug.sethook` runs entirely in the
  interpreter and the budget counts exactly as without the JIT (tested for equality in all three modes). A
  hook installed while compiled code is running (for example by a finalizer) takes effect at the next call,
  return or loop back-edge.
- Compilation is synchronous in the executing thread; the arena is per state, and a state is not shared
  between threads (as before).
- Code is freed with its prototype by the garbage collector; `load` in a loop does not leak code.

## Known gaps

From the plan's results logs (`docs/JIT_PLAN.md` sections 15-19, `docs/JIT_DESIGN.md`):

- Still exit stubs (the interpreter runs them): `LOADKX`/`EXTRAARG`, the metamethod followers `MMBIN*`,
  non-numeric constant operands of some comparisons, long-string constants, some slow paths of
  `UNM`/`BNOT`/`NOT`, `SETUPVAL` of collectable values. Programs dominated by those do not speed up.
- Comparison metamethods (`__lt` / `__le` / `__eq` Lua functions) are called from tier 1 (BUNDLE-B, docs/JIT_PARKED.md C21 update); the other metamethods (`__index` functions, `__newindex`, arithmetic) still leave tier 1 at the instruction.
- Programs whose hot code is string library calls, metamethods, `pcall`/error unwinding, coroutines or
  the allocator/GC show no gain (0.94-1.01 of the interpreter's cycles in the table below); the hotness
  counters cost about 1.2% instructions geomean when the feature is compiled in but a function never compiles.
- 5.4 builds: `gc_churn` measured 1.05x slower with the JIT (not analysed).
- If `mprotect(PROT_EXEC)` starts failing *after* the process has already compiled code (a seccomp filter
  installed mid-run; hosts that deny it from the start are detected by the probe and are fine), the compile
  that finds out cannot seal the page it re-opened for packing and the process aborts with a message
  (`reseal_or_abort`); the sound alternative would be to leave previously compiled functions unrunnable.
  A filter installed after the probe but before the first compile just makes every compile fail (tested).
- Under the `stackmove` stress mode deep-recursion programs are quadratic (test aid only).
- The fuzzing campaign length asked for before J4 (24 h per target) has not been run.

## Performance (tier 1 era, JIT on / off)

Older measurement of the template compiler alone, kept for the per-program picture of what tier 1 gains and where it does not
(the current numbers of the whole JIT are in the Summary above). Cycles (user mode, `perf stat -e cycles:u`), CPU 5, min of 7 runs, one `--features jit` release binary run
as `RUSTYNAIL_JIT=off` and `=on`, `RUSTYNAIL_SEED=0`, 25 programs of `bench/programs`; ratio = on / off, lower
is faster. Machine shared with other work (ratios repeat to about +-3%, `~` = within +-5%). From
`docs/JIT_BENCH.md` ("After J2b" for Lua 5.5, "Lua 5.4" for 5.4).

| program | 5.5 on/off | 5.4 on/off |
|---|---:|---:|
| binary_trees | 0.763 | 0.792 |
| closures | 0.619 | 0.661 |
| coroutines | 0.957 ~ | 0.980 ~ |
| fannkuch | 0.414 | 0.504 |
| fasta | 0.676 | 0.698 |
| fib | 0.512 | 0.508 |
| gc_churn | 1.010 ~ | 1.051 |
| heapsort | 0.531 | 0.573 |
| json | 0.953 ~ | 0.953 ~ |
| knucleotide | 0.911 | 0.841 |
| load_big | 1.003 ~ | 1.018 ~ |
| loops_arith | 0.447 | 0.494 |
| mandelbrot | 0.611 | 0.659 |
| matmul | 0.362 | 0.441 |
| metamethods | 1.003 ~ | 1.007 ~ |
| nbody | 0.598 | 0.627 |
| oop_methods | 0.744 | 0.705 |
| pcall_error | 0.940 | 0.951 ~ |
| queens | 0.550 | 0.580 |
| sieve | 0.647 | 0.703 |
| spectral_norm | 0.610 | 0.571 |
| string_ops | 0.983 ~ | 0.996 ~ |
| table_array | 0.966 ~ | 0.962 ~ |
| table_hash | 0.965 ~ | 0.954 ~ |
| varargs | 0.884 | 0.892 |
| **geomean, all 25** | **0.714** | **0.739** |
| **geomean, numeric subset (7)** | **0.515** | **0.564** |

(The 5.5 column is the table "After J2b" of `docs/JIT_BENCH.md`; both columns were measured on different
days.) Against LuaJIT the JIT is far behind: LuaJIT needs about 0.14x of the reference's cycles on the numeric
subset (`docs/JIT_BENCH.md` baseline), this JIT about 0.5x of our interpreter's, which is about parity with
the reference there. These are the only performance claims made here.

### Build-level options (P-I)

The release profile is fat LTO, `codegen-units = 1`, `panic = "unwind"` (unwinding is the C API's error mechanism, so
`abort` is not an option). **The release build is profile-guided**: `scripts/release-build.sh` runs `scripts/pgo-build.sh`
(`PGO_FEATURES=jit scripts/pgo.sh`: instrumented build, a training run over `scripts/train.sh`, rebuild with the profile,
about 10 minutes; `--no-pgo` builds the plain binary). Measured on 223 programs (session-release): PGO / plain 0.913 cycles
(earlier P-I numbers on 25 programs: -10% interpreter-only, -9% J4, -5% traces). Shipped binaries are stripped and
built with `--remap-path-prefix` (`scripts/release-env.sh`). The profile is not committed (tied to the exact rustc
and source). `-C target-cpu=x86-64-v3` and BOLT on top of PGO gave nothing measurable and are not recommended;
`scripts/build-fast.sh` (absolute dispatch table) is faster for the plain interpreter but slows some trace programs by
13% through code layout, so `pgo.sh` uses a PIE with `PGO_FEATURES`. Data and caveats: docs/JIT_TRACE_REF.md "P-I results".

## Testing

- `cargo test -p rn-jit --features jit` (micro-op emulator, template-by-template differential tests against
  the interpreter, whole-program twin tests in `always` mode, GC stress, `stackmove`, lifetime and memory
  limit tests), `cargo test -p rustynail --features jit --test jit` (facade).
- `crates/rn-jit/tests/jit_j3c.rs` (programs in `tests/j3c/*.lua`, jit-off twin; run also with
  `RUSTYNAIL_GC_STRESS=1|2` and `RUSTYNAIL_JIT_STRESS=stackmove`): `debug.getinfo/getlocal/setlocal/traceback/
  sethook/upvalue*` on compiled frames, hooks installed by callees, finalizers and other coroutines, stack growth
  and shrink with live compiled frames, stack/C-stack overflow and recovery, hundreds of coroutines, GC of
  suspended compiled coroutines, write barriers and finalizers in both collector modes.
- Official Lua test suite with `RUSTYNAIL_JIT=always`; `scripts/jit-diff.sh` (random program differential,
  interpreter vs `off`/`always`/`on`/stackmove/hook variants, `docs/JIT_TESTING.md`); fuzz targets
  `jit_diff` and `run_source --features jit` (`scripts/jit-fuzz.sh`).
- `scripts/asan-jit.sh`: the JIT build under AddressSanitizer (official suite in `always` and `on`, the
  `tests/vm` corpus differential, GC stress 1, `stackmove`, the `rn-jit` tests). `scripts/miri.sh jit`: the
  pure-Rust parts (emulator, lowering, encoder, monitor) under Miri. Executable-memory code cannot run
  under Miri.
- Trace compiler (J5): `cargo test -p rn-jit --features tr_mutants,t15_oracle` (about 50 `tr_*`/`t15_*` binaries: IR and pass
  tests, the recorder lockstep against the interpreter, assembler vs IR interpreter in the emulator, exits, policy, the
  `tr_core` twin matrix in all trace modes with GC stress, the count-identity tests and the oracle); the mutant runner
  `tests/tr_mutants.rs` (each named mutant of `tr/mutants.rs` must be caught; run it under `ulimit -v 16000000`);
  `scripts/jit-diff.sh` modes `tr tron trstack trstorm tremu trrand trverify trnoopt trnosink` (the `GCOBS` lines, which show
  the permitted GC drift, are only compared in modes that cannot sink); `scripts/jit-official.sh` (official suite with traces
  on/always); `scripts/asan-jit.sh` (traces included since F6) and `scripts/asan.sh` with `ASAN_FEATURES=trace`;
  fuzz targets `run_source`, `jit_diff` (trace escape byte 0xFF), `trace_ir` (`scripts/jit-fuzz.sh`, `FUZZ_FEATURES=trace`,
  seeds `fuzz/make_trace_seeds.sh`); `scripts/sink-drift.py` for the GC drift of sinking.
- CI: job `jit` in `.github/workflows/ci.yml` / `scripts/ci-local.sh --only jit-*`, including a check that
  the build without the feature has the same machine code as before the JIT hooks.

## J5 trace compiler: overview

The trace compiler is part of the `jit` feature of `rn-cli` (`rn-cli/jit` enables `rn-jit/trace`; in `rn-jit` alone it is the cargo feature `trace`, which the trace tests use). Without the feature the machine code
of the default build is identical to a build without the J5 hooks (`scripts/jit-text-identity.sh`; the plain `jit` build is
identical to before up to the larger `Mem`/`GcState` layouts of `jit_trace`, which only the *fast-copy* comparison sees:
`scripts/jit-text-identity.sh --fast-copy` reports moved displacements in the same 6275 instructions: accepted).

**What a trace is.** A loop head of a compiled prototype counts back-edges through a dispatch cell. When it is hot (or at the first
back-edge in `always` mode) the *recorder* follows the interpreter through one iteration (calls to Lua functions are inlined, up
to a depth limit; recursion unrolls a fixed number of levels; tail calls and `B=0`/`C=0` multiple-result sequences are
supported), builds SSA IR with a guard for every observed type, closure, shape and branch direction, optimises it (constant
folding, CSE, store-to-load forwarding, loop peeling with loop-invariant code motion, bounds-check hoisting, allocation
sinking) and compiles the result. A guard that fails *exits*: the exit handler writes the live values back into the stack slots,
rebuilds the `CallInfo` frames of inlined calls and continues in tier 1 at the right bytecode. Hot exits record *side traces*,
which link to the loop head or to an inner loop's trace (nests). Anything the recorder cannot represent aborts the recording
with a penalty (exponential back-off, blacklisting) and the loop stays in tier 1/1.5. Traces are flushed when a prototype,
callee or table shape they depend on dies or changes.

**What it supports.** Numeric `for`, `while` and generic `for` (`next`/`pairs`/`ipairs`), array and hash field loads and stores
(existing keys inline; new keys and array appends through class-A helpers that never run Lua code), string
creation/concatenation/number conversion helpers, upvalue reads and writes, intrinsic `math` functions, calls to Lua functions
(inlined) and to C functions (stitching). Lua 5.5 and Lua 5.4 builds (`--cfg rustynail_lua54`; 5.4 has its own `ATOUCH` length-hint
operation).

**What it does not trace** (the loop stays in tier 1/1.5): metamethod-driven code (`__index` class lookups, `__newindex`; these abort the recording), `pcall`/error paths,
coroutine switches inside the loop, loops running with a hook active (hooks disable all compiled code), and
anything that exceeds the limits above. The exact abort reasons are counted by name in the `RUSTYNAIL_JIT_VERBOSE=1` summary.

**Knobs** (all read once per process; unknown items are ignored with a warning printed at `RUSTYNAIL_JIT_VERBOSE>=1`):

| variable | meaning |
|---|---|
| `RUSTYNAIL_JIT_TRACE=off\|on\|always` | `on`: record hot loops (defaults 256 back-edges, 16 exits); `always`: record at the first back-edge, side traces at the second exit (testing). Unset: `on` in `rnlua` (any other value: `off`) |
| `RUSTYNAIL_JIT_TRACE_HOT=loop[,exit]` | back-edge and exit thresholds |
| `RUSTYNAIL_JIT_TRACE_OPT=...` | comma/space list: `-fold -cse -loop -fwd -abc -sink` switch passes off (`-loop` selects the rolled layout, `-sink` allocation sinking, see the drift note below), `-nestrule`, `+wb`, and the test options `+verify` (runtime assertions), `+exitstorm=N` (every guard fails every N-th time), `+randra=SEED` (random register eviction), `+emurun` / `+emuverify` (run / cross-check traces in the emulator), `+tracemove` (move the stack before each entry), `+membudget` |
| `RUSTYNAIL_JIT_TRACE_LIMITS=name=n,...` | trace policy limits and rules; every name, its shipped value and range is in the table below |
| `RUSTYNAIL_JIT_TRACE_PREF=trace\|region` | who owns a loop nest first when both the tracer and J4 regions are enabled (default `trace`) |
| `RUSTYNAIL_JIT_VERBOSE=1\|2` | 1: a summary at state close (`[trace] recorded N installed N entered N ...`, sink counts); 2: one line per start/loop/flush/abort |
| `RUSTYNAIL_JIT_DUMP=dir` | also writes `trace-<n>.txt` (IR and assembly) per installed trace |

**Trace policy knobs (`RUSTYNAIL_JIT_TRACE_LIMITS=name=value,...`).** The table is generated from `TrLimits` and the `*_PROCESS`
constants of `crates/rn-jit/src/tr/cache.rs`: "shipped" is the value of an `rnlua` process (`TrLimits::default()`, which the tests
use, equals it except where "tests:" says otherwise); the range is the clamp applied by the parser. Knobs whose shipped value is 0 are
**built but off**: `fnroot`, `contok` (test knob), `armm`, `sbump2`, `fibdiet`, `sinkside`, `t1ccall`, `lazyci`, `aidx`, `xregs`,
`lockstep` (an assert-only recorder cross-check, on in tests), and `warm` (a time gate; the work gate `wgate` replaced it).
Everything else in the table is on. The descriptions are the first sentence of the field documentation; the bundle names
(`BUNDLE-x`, `C36`, ...) are the ledger entries of `docs/JIT_TRACE_REF.md` and `docs/JIT_PARKED.md`. Names that are not in the table
are ignored with a warning at `RUSTYNAIL_JIT_VERBOSE>=1`.

| knob | shipped | range | meaning |
|---|---|---|---|
| `maxtrace` | 1000 | any | Traces per state; reaching it flushes all of them (global penalty). |
| `maxrecord` | 4000 | any | IR instructions per recording. |
| `maxsnap` | 500 | any | Snapshots per recording. |
| `maxside` | 16 | any | Side traces per root. |
| `polyside` | 48 | any | BUNDLE-O: side traces a family may hold beyond `maxside` (0 = none: the cap is hard). |
| `polywait` | 2 | any | See `polyside`: deferrals of a capped exit before it may exceed `maxside`. |
| `polyframes` | 2 | any | See `polyside`: only an exit that lies in at least `polyframes` inlined frames may exceed `maxside` (0 = any exit): the outcomes ... |
| `maxdepth` | 4 | any | Inlining depth. |
| `unroll` | 2 | any | Recursion unroll. |
| `sidemin` | 4 (tests: 0) | any | F2a: fewest bytecodes of a side trace (shorter ones are not installed, see `AbortReason::SideShort`); 0 = no minimum (the default ... |
| `nest` | 1 | 0-2 | F2b: linking level. 0: the prototype's `InnerLoop` abort; 1 (default): a root recording that reaches an inner loop head of its ... |
| `stitchmin` | 32 | any | F2b: stitching at C calls needs at least this many recorded bytecodes before the call (0: off). |
| `loopunroll` | 16 | any | P-E: backward edges of inlined callees' loops unrolled per recording (0: such a loop aborts `InnerLoop`). |
| `fnroot` | 0 | 0-1 | P-E2: function-entry roots (pc 0 of a loop-free, non-vararg prototype that makes calls is a trace head); 0 = off. |
| `recnat` | 1 | 0-1 | P-M3: native self-recursion (a function-entry root whose first self-call is a native frame push that jumps back to the trace ... |
| `contok` | 0 | 0-2 | BUNDLE-E (test knob, default 0, never set by a process default): continuation heads of `recany` also record for a prototype in ... |
| `recany` | 1 | 0-1 | R2: native recursion for any callee load (`SELF` methods, `GETTABUP` / `GETFIELD` globals and fields, not only `GETUPVAL`) ... |
| `siderooms` | 1 | 0-1 | R2: a side trace entered with virtual frames that needs more stack / spare `CallInfo` nodes than its parent's checks guarantee ... |
| `nilstore` | 1 | 0-1 | R2: a store into an existing hash node that holds nil (a field that was assigned `nil` and is set again) is recorded (0 = the ... |
| `fnmin` | 16 | any | P-E2: fewest bytecodes a function-entry recording must execute. |
| `loopany` | 1 | 0-1 | M1: `while` / `repeat` loops of inlined callees are unrolled (0 = the `InnerLoop` abort; struct default 0, process default ... |
| `loopanymax` | 4 | any | Guard of `loopany`: side-trace attempts at exits inside an inlined callee frame that a family with unrolled callee loops may take ... |
| `laprune` | 1 | 0-1 | BUNDLE-P: when the `loopany` guard fires and the family root unrolled no callee loop itself (only side traces did), flush only ... |
| `cispare` | 1 | 0-1 | C36: when the persistent-entry-failure rule fires for a trace that needs spare `CallInfo` nodes (`TR6b`: an inlined native call) ... |
| `sidestitchmin` | 4 | any | M1: the least number of recorded steps of such a side trace. |
| `t3idx` | 2 | 0-4 | T3: the tier-1 `GETFIELD` / `SELF` / `GETTABUP` templates probe this many `__index` tables (instance -> class -> base class ...) ... |
| `t1inline` | 1023 | 0-1023 | BUNDLE-T1I: bit mask of the built-ins the tier-1 `CALL` template runs inline (bit 0 `type`, 1 `math.floor`, 2 `math.abs`, 3 ... |
| `lockstep` | 0 (tests: 1) | 0-1000000 | BUNDLE-M C1: the recorder's lockstep cross-check (TR17: the IR so far is run on a model and compared with the VM after every ... |
| `dynmask` | 1 | 0-1 | DYNMASK: dynamic short-string keys (`tk`, `k` an SSA value) use `HREFV` with `HREF_DYN` (the table is the operand; the ... |
| `ineffcover` | 1 | 0-1 | P2: the ineffective-trace rule counts only the pre-roll exits that leave the trace (no side trace attached to the exit, or a ... |
| `typesides` | 1 | 0-1 | P2: a hot type-guard exit of a rolled root or of a side trace gets a side trace too (0 = it flushes the family, `Rerec`). |
| `hoptop` | 98 | 0-100 | BUNDLE-P: the deterministic hop rule (counters only): the percent of a root's entries that leave through ONE pre-roll site into a ... |
| `second` | 3 | 0-3 | C36: second chances for heads lost to a transient, once per head (bit 0: a head blacklisted by the abort count whose last abort ... |
| `sbump` | 1 | 0-4 | BUNDLE-C: a side recording that starts inside inlined frames at an exit whose previous attempt ended `SideShort` gets `sbump` ... |
| `ccalls` | 1 | 0-1 | BUNDLE-D (`ccalls`): the `LIB CCALLS` C-stack check is inline loads and a compare (the helper stays the slow path that answers 0 ... |
| `armm` | 0 | 0-1 | BUNDLE-D (`armm`): tier 1 continues in compiled code after a Lua `__add/__sub/__mul/__div/__unm/...` metamethod returns |
| `f2ifl` | 1 | 0-1 | BUNDLE-P P2 (`f2ifl`): `F2I_EXACT` of an `FFLOOR` result (already integral or NaN/inf/out of range) is `cvttsd2si; cmp r, 1; jo ... |
| `t1q` | 125 | 0-127 (bit 7 always set) | BUNDLE-Q (`t1q`): bit mask of the tier-1 exits removed (1 = `EQ` of objects without a real `__eq` call, 2 = float `MOD`, 4 = ... |
| `amt` | 1 | 0-1 | BUNDLE-S S2 (`amt`): array reads and stores of a table that has a metatable are recorded (a non-nil element is a raw access; a ... |
| `rearmside` | 2 | 0-64 | BUNDLE-P P1 (`rearmside`): a rearmed tick also enters a young head (fewer than REARM_MIN_ENTRIES entries) whose family has this ... |
| `rearmclean` | 1 | 0-1 | C45 (`rearmclean`): 1 = `rearmside` only enters young heads of clean families (default REARMCLEAN_PROCESS); 0 = the plain rule. |
| `sbump2` | 0 | 0-8 | BUNDLE-D (`sbump2`): per-exit extra recursion unroll for side attempts that ended `Recursion`/`Depth` inside inlined frames ... |
| `stitch0` | 1 | 0-1 | BUNDLE-F (`stitch0`): 1 = a side recording of a function root with nothing recorded yet does not stitch (no chains of empty side ... |
| `chaindepth` | 4 | 0-256 | BUNDLE-G (`chaindepth`): longest chain of callee-guard side traces (a megamorphic call site: link after link, each attached to ... |
| `fibdiet` | 0 | 0-15 | BUNDLE-F (`fibdiet`): bit 0 = the room check after a native push computes from the register-held new function (no reload of the ... |
| `fnpromo` | 6 | 0-64 | BUNDLE-C: per-site `fnroot`. |
| `fnpromow` | 3 | 0-12 | BUNDLE-C: the promoted head waits `64 << fnpromow` entries before its first generic recording (a short program cannot repay a ... |
| `innerretry` | 6 | 0-16 | C39: retry rounds of a head blacklisted by two `InnerLoop` aborts in a row (a loop of an inlined callee the recording cannot ... |
| `sidenest` | 1 | 0-1 | C38 slice 1: 1 = a SIDE recording that reaches, inside an inlined callee frame, a loop head of that callee with a root trace ends ... |
| `domtop` | 82 | 0-100 | C36: a root with at least `domn` entries since its trace was installed, of which `domtop` percent or more leave through ONE ... |
| `domn` | 65536 | any | C36: see `domtop`. |
| `ncallee` | 1 | 0-1 | P3: after an entry-failure flush of a root whose entry segment holds a callee identity guard (TR6d), the head is recorded again ... |
| `chainre` | 95 | 0-100 | P3: a side trace of which `chainre` percent or more of at least `chainn` entries leave through its callee-guard exit into a child ... |
| `chainn` | 65536 | any | P3: see `chainre`. |
| `tset` | 15 | 0-15 | BUNDLE-B item 2: inline store paths of tier 1 (bit 0 `SETTABLE` with a short-string key into an existing node, bit 1 integer key ... |
| `fck` | 64 | 1-1024 | H1: such an exit (a family rooted at a function-entry or continuation head) gets its side trace when it has been taken `fck` ... |
| `framemin` | 8 | any | H1: fewest bytecodes of a side trace that starts inside inlined frames and ends at a stitching exit (no link); shorter ones are ... |
| `allocguard` | 1 | 0-1 | P-B: the allocation storm guard (1 = on): a recording that allocates strings in every iteration aborts `AllocStorm` when the ... |
| `warm` | 0 | any | P-G: time-stamp-counter ticks that must pass between the first loop activity of the state (the first sampling tick of the ... |
| `wgate` | 30000 (tests: 0) | any | BUNDLE-1: deterministic start gate: back-edges (loop sampling ticks count as `tick_period` each, a head's hot firing as ... |
| `tailhook` | 1 | 0-2 | BUNDLE-1: the interpreter's tail call (`pre_tail_call`) counts calls of its Lua callee like a call does (the callee is compiled ... |
| `hdrlib` | 2 | 0-4 | BUNDLE-1: header-safe peeling: loops with string `TLEN` and read-only `LIB` kinds are peeled (see `opt_mem::header_safe_ins`). |
| `sinkmt` | 1 | 0-1 | BUNDLE-2: allocation sinking of tables that are given a metatable (`setmetatable({...}, V)`, the object-constructor idiom): the ... |
| `sinkside` | 0 | 0-1 | BUNDLE-5: allocation sinking in side and stitched traces (their writeback is dead by the link target's active locals, or never ... |
| `sinkroot` | 2 | 0-2 | SINK-R: the ineffective-root rule is sink-aware: a root that would be flushed as ineffective while the sunk tables its pre-roll ... |
| `sinkstub` | 1 | 0-1 | J1-2: an exit that must materialise sunk tables (frameless, not an entry check) is restored by its inline stub, which calls the ... |
| `deep` | 8 | 0-8 | J2-2: per-site deeper inlining: a loop recording that ended at the recursion or depth limit (`Recursion` / `Depth` abort) is ... |
| `deepins` | 300 | any | J2-2: most IR instructions of a deep recording (a bigger call tree aborts `TooLong`: the site keeps its penalties). |
| `deephop` | 1 | 0-1 | J2-2: a root recorded with the deep limits is exempt from the hop-rate rule of the ineffective-trace check (1; 0 = it is judged ... |
| `t1ccall` | 0 | 0-1 | BUNDLE-C: the tier-1 `CALL` template calls light C functions through the lean helper `jit_ccall` |
| `tfmt` | 1 | 0-1 | BUNDLE-C: `string.format` with a constant format of `%d %x %X %s %c` items (and plain widths) is recorded (`LIB FMT`) |
| `libcall` | 1023 | 0-1023 | BUNDLE-H: library calls on trace (bit 0 recorded calls, bit 1 lean `LIB` entries, bit 2 inline `type` / string compare / `random`) |
| `retf` | 3 | 0-15 | BUNDLE-N (RETF, docs/JIT_PARKED.md C38): frame-leaving side traces. |
| `retfmax` | 1 | 0-8 | BUNDLE-N: root `RETURN`s a recording may turn into `RETF` |
| `upbar` | 1 | 0-1 | BUNDLE-V V2: record upvalue stores of collectable values |
| `tnextinl` | 1 | 0-1 | BUNDLE-H: traversal steps on trace (bit 0: `TNEXT` is header-safe, the loops of `pairs` / `next` peel) |
| `centry` | 2 | 0-2 | BUNDLE-J: C-entered function roots. |
| `centrymin` | 60 | 0-100000 | BUNDLE-J: a C-entered function root whose trace has fewer IR instructions than this is not entered from C |
| `centrywait` | 8 | 0-1000 | BUNDLE-J: hot events from C before a registered function is entered |
| `lazyci` | 0 | 0-1 | BUNDLE-K L1 (`lazyci`): natively pushed self-call frames of the function-entry / continuation traces are lazy (only `func` and ... |
| `aidx` | 0 | 0-1 | BUNDLE-L A (`aidx`): the complemented index `~i` of an array load is an IR value (`NEG k` with a displacement for `k + c`, shared ... |
| `bhoist` | 1 | 0-1 | BUNDLE-L B (`bhoist`): the bounds checks `ULT k + c, asize` of a counted `for` loop (induction slot `k` with step 1, loop guard ... |
| `pollr15` | 2 | 0-2 | P2-3b: the trace poll is `cmp byte [r15 + disp], 0` when the poll word is within reach of the per-state data (0 = address in a ... |
| `xregs` | 0 | 0-3 | P2 bundle 1: extra general registers of the trace register allocator: bit 0 = `rbp` (`V9`, free: only the thunk's frame-pointer ... |
| `rearm` | 4 | 0-64 | R1: a stay of compiled code that hands a frame to the interpreter arms the sampling tick (`jit_hooks::rearm_tick`) for the next ... |

Trace code is charged to the same code cap and memory limit as tier 1 code (a refusal flushes traces and sets a global
penalty). The instruction limit and hooks behave as for tier 1: compiled code is not entered while a hook is active, and
the instruction count of a program is identical with traces on and off (`tr_identity`, `tr_record_count`, the trace oracle:
a trace counts the bytecodes it replaces, a budget that runs out inside a trace exits before the instruction that would overshoot).

**Interaction with tier 1.5.** With traces on, loop nests are traced first; J4 still compiles a nest as a region when the tracer
blacklists its head (`TRACE_PREF=region` reverses the order for experiments). Counting stubs/dispatch cells are the only
change to tier 1 code, and only with `RUSTYNAIL_JIT_TRACE != off`.

**Known gaps and deviations** (full list: `docs/JIT_TRACE_REF.md`, F8 section 6 and the F6 notes):

- Allocation sinking is the one place where results may differ from the interpreter in GC-observable ways (next section).
  There is **no in-trace collector-step/limit guard** for a sunk-allocation loop (TR29.3 first half): a loop that executes
  no real allocation reports an ever-growing `collectgarbage("count")` until something allocates, and `set_memory_limit` is
  checked against `limit + virtual bytes` when an exit materialises the table, not at the `NEWTABLE`.
- Memory error while an exit materialises sunk tables raises the ordinary "not enough memory" at the exit's pc (tested with
  an injected fault, also under `+emurun`).
- Performance claims for traces are in `docs/JIT_TRACE_REF.md` (gate P9b, F-task tables, F7b for 5.4) and `docs/JIT_BENCH.md`;
  the current geomeans are in the Summary above (traces are on by default in `rnlua`).
- The 24-hour fuzzing campaign asked for by the plan (decision D3) has **not** been run: short smoke campaigns (F6,
  `docs/JIT_TRACE_REF.md`) and the 30-minute-per-target campaign of "Verification B" in `docs/RELEASE.md`. Do not run untrusted code
  with traces on until it has (`RUSTYNAIL_JIT=off`, or the embedding API's `Builder::sandbox()`, which keeps the JIT off).
- Miri covers the pure modules only (IR, passes, RA, exit/snapshot logic and the emulator); the native code, the recorder
  against a live state and the cache/lifecycle code run under ASan (`scripts/asan-jit.sh`) and the differential harnesses.

## J5 trace compiler: allocation sinking and what it changes (drift note)

The trace compiler (`RUSTYNAIL_JIT_TRACE`, feature `trace`; design in `docs/JIT_TRACE.md`) can *sink* a table that
is built and dropped inside one loop iteration (`local v = { x = i, y = i + 1 }` used only through its fields): the
table is never allocated on the fast path and is only built (with the interpreter's own allocation and insertion
routines, same sizes, same insertion order, hence the same `#t` and `next` order) when an exit needs it. This is
the one place where the JIT is allowed to differ from the interpreter in ways a program can see. Switch it off with
`RUSTYNAIL_JIT_TRACE_OPT=-sink`: then no table is sunk, no virtual byte is counted and everything below holds exactly
(the byte counts and all collector decisions are then those of the interpreter).

With sinking on, the following are **not** guaranteed to match a run without it (policy TR29, decision D8):

- **GC-observable values**: `collectgarbage("count")` and the result of `collectgarbage("step")`; the step or cycle at
  which a finalizer runs, hence the interleaving of finalizer side effects with program output, and the moment weak
  table entries are cleared; generational ages. A sunk table is charged to the collector as a *virtual* allocation
  (same size as the real one, counted in the total and in the pacing debt, never against the memory limit) and
  credited back when the collector completes the cycle after the one in which it was charged (incremental) or at
  the next young/major collection (generational), and fully at every full collection. Measured drift
  (`scripts/sink-drift.py`, `tests/sink_drift.lua`, about 15 MB of churn): after full collections the counts differ by
  at most 0.28 KB (incremental) and 0.55 KB (generational; a dead stack slot keeps an older table alive); at 1000
  random points between collections at most 275 KB (mean 84 KB, incremental) and 104 KB (mean 42 KB, generational). On Lua 5.4 builds: 176 KB (mean 44 KB, incremental) and 110 KB (mean 43 KB, generational).
  A loop that sinks tables and executes no real allocation at all never reaches a collector step, so its `count` keeps rising (the numbers only; no memory is used) until the next real allocation steps the collector. With the collector stopped (`collectgarbage("stop")`) the byte count after a sinking loop equals the interpreter's
  exactly (`sink_count_stopped`).
- **Addresses**: `tostring`/`%p` of tables, functions, userdata and threads, and the `pairs` order of tables keyed by
  such objects, can differ (fewer allocations, so different addresses). This is already unreproducible between two
  runs of the interpreter under ASLR.
- **`debug.getlocal` of dead slots**: live locals in scope are always exact after any exit. A slot that no instruction
  of the iteration reads any more and that is a temporary of a popped inlined call or lies above the active locals
  (`Proto::locvars`) may show an older value than the interpreter's, because a sunk table that only a dead slot held
  is not stored back at the end of the iteration (decision D7/D8). A table held by a live local is never sunk.

Unchanged by sinking: error behaviour of the program's own operations (messages, positions, tracebacks), instruction
counts and count/line/call hook behaviour, `debug.*` results on frames except the dead slots above, and the content,
`#t` and `next` order of every table that is materialised. A host allocation failure while an exit builds a sunk table
raises the ordinary `not enough memory` error at that instruction (never a panic); the memory limit itself is not
checked against virtual bytes.

While a trace is being recorded the recorder holds handles to the tables it has seen; the collector flags the recording
when it frees one of them (a table the program dropped meanwhile) and the recording is abandoned without penalty. Nothing
is kept alive for the recorder, so with `-sink` the collector behaves exactly as the interpreter's.

Measured effect (S0/F8): microbenchmarks that build a temporary per iteration (`bench/jit-trace/sink_vec`,
`sink_tuple`) run 100x faster than with `-sink`; none of the 25 benchmark programs changes beyond noise (their
tables escape). The official `gc.lua` and `gengc.lua` pass with sinking on.

## J5 trace compiler: defaults and switches (F9)

In a build with the cargo feature `jit` (`rnlua`, `rustynail`) the trace compiler is compiled in and
`RUSTYNAIL_JIT_TRACE` defaults to **on** whenever the JIT itself runs; J4's regions (`RUSTYNAIL_JIT_T15`, default
`on` together with traces) then own only the loop nests the tracer gave up. Switches:

| switch | effect |
|---|---|
| `RUSTYNAIL_JIT_TRACE=off` | the documented off switch: exactly the J4 behaviour (regions own every hot nest, T15 default back to off); `on` / `always` select the mode explicitly |
| `RUSTYNAIL_JIT_T15=off` / `on` / `always` | J4 regions; an explicit value wins over the default tied to traces |
| `RUSTYNAIL_JIT_TRACE_OPT=-sink` | GC observables exactly as without the JIT (see the drift note above) |
| `RUSTYNAIL_JIT=off` | no JIT at all (also no traces) |
| `Builder::jit_trace(bool)` / `Lua::set_jit_trace(bool)` | facade, per state; `false` = J4 behaviour. Needs `JitMode` other than `Off` to have any effect; a process with `RUSTYNAIL_JIT_TRACE=off` never installs the trace driver, so `true` cannot enable traces there (the call returns `false`) |

Builds without the `jit` feature are unchanged (no trace code, the environment variables are ignored, same machine
code as before). `cargo test -p rn-jit --features jit` alone still builds the J4 compiler without traces; the trace
tests use `--features trace`. Scripts and CI steps that mean "J4" pin `RUSTYNAIL_JIT_TRACE=off` (`scripts/jit-diff.sh`
J4 modes, `jit-official.sh` calls in `scripts/ci-local.sh` and CI, `scripts/jit-dump-identity.sh`); the modes `trdef` /
`trdefon` of `jit-diff.sh` and the "default configuration" official run use the unpinned defaults.
