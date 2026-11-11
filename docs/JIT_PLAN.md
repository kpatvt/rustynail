# JIT plan (Phase 10)

Status: DRAFT v2 (2026-10-28), revised after an independent code-level review (v1 had five blockers; see
section 12). Not started. Prerequisites are met: working 5.5 and 5.4 interpreters, official suites green,
measured baselines (docs/PERF.md), ASan/Miri/fuzz tooling (docs/UNSAFE.md).

Settled with the user (2026-10-28): the defaults of section 8 are accepted as written, and implementation is done by **Sonnet builders** (section 13).

## 1. Goal and what "performant" means here

Make hot Lua code run substantially faster than the interpreter while keeping every guarantee we have
today: official suites green, C API ABI, hooks/debug, coroutines, GC behaviour, error messages, memory
limits and the sandbox (including the instruction limit).

Baselines (docs/PERF.md; quiet machine, min-of-5, pinned core):

| | time vs reference PUC Lua 5.5 |
|---|---|
| our interpreter (default / fast build), 25 programs | 0.86 / 0.84 |
| LuaJIT 2.1 interpreter (`-joff`), 13 shared programs | about 0.72 (noisy run, re-measure in J0) |
| LuaJIT 2.1 with JIT, same 13 programs | about 0.21 (noisy run, re-measure in J0) |

The LuaJIT rows come from the load-affected Phase 8 run and compare a 13-program subset against our
25-program geomean; J0 re-measures them on a quiet machine over the same program set. LuaJIT's interpreter
is itself 25-35% faster than ours, so part of the gap is interpreter quality, but only a JIT closes the
numeric-loop gap.

### Where a JIT can and cannot help (measured by the review, Zen 5, cycles:u)

- Our interpreter executes 31-45 x86 instructions per bytecode on numeric programs (mandelbrot 31,
  loops_arith 35, sieve 38, nbody 43, fib 43) and 120-1500 on runtime-bound programs (json 124,
  pcall_error 184, gc_churn 322, string_ops 487, table_array 1548). IPC is already 4.5-6.6.
- A C emulation of template output (every bytecode reloads and stores tag+payload through the Value
  stack, no `savedpc`/trap stores, so roughly an upper bound for a plain template JIT) gave:
  mandelbrot 4.2G cycles -> 3.34G (1.28x) even with 5.6x fewer instructions, because float loops are
  latency-bound through store-to-load forwarding; with values kept in registers 2.04G (2.1x).
  loops_arith first loop 2.99G -> 0.72G (4.1x) with templates, 0.44G with registers.
- Reading: instruction counts predict almost nothing on this CPU. Integer/throughput-bound loops gain
  3-4x from templates; float loops need register allocation of values (tier 1.5) to gain.
- About 10 of the 25 programs are runtime-bound (strings, GC, pcall, coroutines, load, json) and will
  barely move under any JIT.

### Targets (gates; goals, not promises; we report what we measure)

Judged by median cycles:u and instructions:u over N >= 7 runs with a +-5% band, not wall time (the
machine's wall-time noise exceeds a few percent; docs/PERF.md). Wall time is reported for information.

| Tier | numeric/table subset (loops_arith, mandelbrot, sieve, matmul, fannkuch, heapsort, queens, nbody, spectral_norm) | all 25 programs (geomean) |
|---|---|---|
| Tier 1 (templates + OSR) | <= 0.45x PUC cycles | <= 0.70x |
| Tier 1.5 (local type inference, values in registers) | <= 0.30x | <= 0.60x |
| Tier 2 (numeric-loop traces; stretch) | <= 0.25x | <= 0.50x |

Rules for every tier: no program slower than the JIT-off build beyond the +-5% band; short-script cost
invisible (official suite wall time and `lua -e ""` startup within +-5% of JIT-off); the JIT-off build
stays byte-identical to today (section 3, F1).

Non-goals: aarch64 (after Phase 9; keep the backend behind an `arch/x86_64` boundary), FFI extensions, AOT,
changing the bytecode or `string.dump` format, background compile threads, deopt across C frames.

## 2. Design: tiers

1. **Tier 1: template (baseline) method JIT with OSR.** One machine-code template per bytecode, using the
   interpreter's exact frame layout (same `Value` stack, `CallInfo`, upvalues). Entry points exist at the
   function start, at every loop head, and at every resume point (call-return, metamethod-return,
   coroutine resume). An unsupported opcode is not a compile failure: its template stores `savedpc` and
   returns to the interpreter at that pc ("per-instruction exit"). Every slow path (type guard failure,
   metamethod, error, allocation, GC point) exits to the interpreter at the current instruction before
   any side effect; the interpreter re-executes the instruction itself. This removes helper duplication
   and keeps `savedpc`/`MMBIN` semantics exact (review B3).
2. **Tier 1.5: method JIT with simple local type inference.** Over a hot loop body: FORLOOP control
   variables are ints, constants are known, results of type-guarded ops are known, so values live in
   machine registers (ints and floats unboxed) inside the loop and are written back to the Value stack at
   exits and calls. Deopt is trivial because frames are identical: write back registers, set `savedpc`,
   exit to the interpreter. No recording, snapshots or trace linking. Expected to capture most of the
   float-loop gain.
3. **Tier 2: traces of numeric `for` loops only (stretch, gated).** Exits on calls, metamethods and GC
   points; trace recording, snapshots, side exits, linking. Only if tier 1.5 plateaus and the checkpoint
   (section 9) approves. Full LuaJIT-style tracing is explicitly out of scope for this plan version.

Alternatives considered and set aside: a closure-compilation/threaded-code interpreter or quickening with
inline caches (kept as an optional low-risk side project, see J1b); copy-and-patch (build complexity);
Cranelift/LLVM backends (dependency policy, compile latency, less control over exits); optimising method
JIT with speculation and full deopt (profile feedback infrastructure missing).

## 3. Constraints from the code (verified by review unless marked [I])

### Entry/exit protocol (the central design problem)
- Lua-to-Lua calls and returns do not recurse on the Rust stack: OP_CALL sets `ci = newci; continue 'frame`
  (vm_unsafe.rs:1527-1561), `do_ret!` sets `ci = prev; continue 'frame` (vm_unsafe.rs:701-719). The same path
  is taken after in-place metamethod returns (`mm_return`, vm_unsafe.rs:288) and coroutine resume
  (coro.rs:84-99: `unroll` -> `finish_op` -> executor(ci)).
- **Invariant E1 (single entry mechanism):** each compiled prototype has a table `pc -> native address`
  covering function start, loop heads, call-return sites, metamethod-return sites. The interpreter's
  `'frame` restart checks "is there compiled code for this proto and a resume address for `savedpc`" and
  enters it. This one mechanism serves OSR, returns into compiled callers, and coroutine resume (D4
  resolved: no separate fallback path). Its cost on the call/return path is budgeted and measured: `run`'s
  register allocation is fragile (docs/PERF.md), so the check is out-of-line and counted per change.
- **Invariant E2 (no native recursion for Lua calls):** compiled code pushes the `CallInfo` and jumps to
  the callee's entry; RETURN jumps to the caller's resume address from the E1 table. Reason: Lua recursion
  is limited only by `LUAI_MAXSTACK` = 1,000,000 slots (state.rs:53), not by `LUAI_MAXCCALLS`; the test
  tests/vm/err_08_stack_overflow.lua expects about 300k frames to end in a Lua "stack overflow" error, which
  native recursion would turn into a SIGSEGV on an 8 MB stack. Coroutines share the OS stack (coro.rs:5).
  Native nesting (helpers, `ccall`) stays bounded by `LUAI_MAXCCALLS`.
- **Invariant E3 (slow paths exit):** compiled code never calls `mm_frame` or runs a metamethod itself;
  any instruction that is not on its inline fast path exits to the interpreter at that instruction. In-place
  metamethod frames (`CIST_MMRET`) only work when the interpreter loop continues them.
- **Invariant E4 (`savedpc` exactness):** `finish_op`/`mm_return` decode `savedpc-1`, and for MMBIN
  `savedpc-2` (vm_unsafe.rs:301, 318, 1934-1941): an ADD whose slow path calls a metamethod must leave
  `savedpc` after the MMBIN, not after the ADD. Exiting before side effects at the instruction start (E3)
  sidesteps this; any template that stores `savedpc` mid-sequence needs a test per opcode class.

### Stack, GC, errors
- **Stack moves at every GC point**, not only on growth: the atomic phase shrinks the stack
  (gc/obj.rs:96-103) and nils slots from `top` upward. So `base` is reloaded after any allocation/`check_gc`
  equivalent, and `top` is set exactly before every GC point (the interpreter's `check_gc!`:
  vm_unsafe.rs:668 sets savedpc, `set_top(lim)`, reload). Compiled code never holds pointers into the stack
  across an exit/helper call.
- GC is incremental/generational (tri-colour invariant between steps), objects do not move. Barriers must
  be emitted exactly where the interpreter emits them: tables `barrier_back` (vm_unsafe.rs:1118-1201,
  SETLIST :1838), SETUPVAL forward `barrier` (:1017); stack stores need none. Machine code embeds no
  pointers to collectable objects except a prototype's own constants.
- **Panics (invariant P1):** C API errors and yields are Rust panics (`Throw`, glue.rs:52-71) caught by the
  C trampoline that called the C code; they never cross Lua frames provided compiled code never calls C
  functions directly. All native calls go through the existing native-call path. Every helper called from
  generated code is `extern "C"` (not `C-unwind`), so an internal `assert!` abort-panics deterministically
  instead of unwinding through frames without unwind tables.
- Errors inside rn-core are `Status` plus `pcall_with` restoring `ci`; `savedpc` matters for messages and
  line info.
- Hooks/Ctrl-C: two interpreter copies (`run_mode::<HOOK>`) and `ci.trap`; Ctrl-C is a separate global
  atomic `INTERRUPT` (state.rs:138). Compiled back-edges and calls must poll both `ci.trap` and
  `INTERRUPT`, and honour `trap` values left by stack reallocation (`fast_trap`, vm_unsafe.rs:495).
  Active line/call hooks: run the function in the interpreter (compiled code exits at the next
  instruction when a hook is enabled).
- **Instruction counting (invariant C1):** the facade's limit and the fuzzer's budget are a
  `LUA_MASKCOUNT` hook (crates/rustynail/src/lua.rs:356-361, callback.rs:302-307; fuzz/src/lib.rs:81).
  Compiled code counts exactly per basic block: the block's bytecode count is a static constant (counting
  fast paths as the interpreter does: an arithmetic fast path skips its MMBIN, a slow path fetches it);
  decrement `hookcount` once per block; if it would cross zero, exit to the hooked interpreter at the block
  start, which then fires the hook exactly. Cost: about one sub and one branch per block; keeps
  `debug.sethook(f, "", n)` exact under JIT. (This replaces D3a/b of v1.)
- **Invalidation sources for anything cached across calls/GC points (tiers 1.5 and 2):** `__gc` finalizers
  at any `check_gc`; `debug.setlocal(2, ...)` from a plain callee (no hook needed); `debug.upvaluejoin` /
  `setupvalue`; `setmetatable` in a callee; table rehash moves node and array arrays. Registers cached in
  machine registers are written back and re-guarded after every call and GC point.

### Layout and versions
- `Value`: 16 bytes; tag word is u64 but only the low byte is ever written (value.rs:172-175, 229-235),
  so compiled code compares the tag byte only. Table layout invariants T1-T4 (inverted 9-byte array slots,
  node layout, dummy node) are encoded by inline table probes, so layout constants (`offset_of!`) and
  layout-dependent emitters live in rn-core next to the layout, with tests; the architecture backend lives
  in `rn-jit` (D2).
- 5.4 differs more than a template set: `luaH_getint` mutates the table on reads (`alimit_touch`,
  table.rs:438, called at :1171, 1192, 1283, 1298, 1476), which changes later `#t`; an inline GETI/GETTABLE
  fast path that skips it changes program output. Also forprep differences (vm_unsafe.rs:411-454), its GC
  control layer (gen54.rs) and 25 `cfg` arms in vm_unsafe.rs. 5.4 is therefore its own phase (J2b), after
  5.5 tier 1 ships (D5).
- Plug-in points exist: `State::set_executor` (state.rs:1376) and `call::set_c_trampoline`. Freeing code
  with its prototype needs one more callback in gc/obj.rs.

### Process, memory, platform
- Unsafe policy: JIT code in one crate/module with a section in docs/UNSAFE.md; layout-dependent unsafe
  stays in rn-core (above). No new third-party runtime dependencies (D1/D6); `deny.toml` does not allow
  MPL-2.0 (dynasm-rs) or the Cranelift licence.
- **Executable memory:** `mmap` RW then `mprotect` RX (never RWX). A runtime probe at first use detects
  hosts that forbid it (systemd `MemoryDenyWriteExecute`, `PR_SET_MDWE`, SELinux `execmem`) and falls back
  to the interpreter silently with a one-line diagnostic available through the builder. Code arenas are per
  state (never re-flip a page another OS thread's state may be running); `mprotect` calls are batched (one
  per compile unit, not per function). This machine has no seccomp filter, so CI here cannot catch the
  failure: a test runs the probe under `PR_SET_MDWE`/seccomp explicitly. The facade's `sandbox()` preset keeps
  the JIT off until the probe exists and has shipped for a release.
- **Code memory accounting:** code and metadata are counted against a separate per-state code cap, charged
  to the sandbox memory limit (`Mem::set_limit`) but not routed through `Mem` accounting for
  `collectgarbage("count")` or GC debt, so GC pacing and finalizer timing in the GC tests do not change.
  Code is freed with its prototype (GC callback) and the cache has an eviction policy; "never free" is not
  acceptable (text `load` in a loop would grow without bound).
- Debuggability: CLAUDE.md's `coredumpctl` backtrace workflow breaks at JIT frames. Compiled code keeps a
  frame-pointer chain, writes a `perf` map file, and (J0 spike) registers with the GDB JIT interface.
- Random string-hash seed gives 1-4% instruction swings (docs/PERF.md): JIT tests and benchmarks use a
  fixed seed (existing seed knob).

### F1: build structure (decided: cargo feature)
JIT is a cargo feature (`jit`). The default build contains none of the hooks (hotness counters, `'frame`
entry check), so its interpreter `.text` and instruction counts stay byte-identical to main; verified at
every gate (`objcopy --only-section=.text` + `cmp`). Builds with the feature use a runtime switch
(`RUSTYNAIL_JIT=off|on|always|verify`, builder option).

## 4. Phases

### J0: Spikes and design (about 2 weeks)
1. **Template oracle (replaces the v1 "dispatch share" criterion):** hand-emit or C-emulate templates for
   4-6 hot loops (mandelbrot inner loop, loops_arith loops, sieve, a fib-style call, a table-get loop) and
   measure cycles. Kill or re-plan if the projected geomean over the 25 programs (bucket model: integer
   loops 2-2.5x, float loops 1.3-1.5x with templates / 2x with registers, calls 1.15-1.3x, runtime-bound
   1.0-1.05x) is above 0.75x PUC. The review's model projects about 0.64x (range 0.58-0.72) for tier 1
   complete.
2. Re-measure the LuaJIT rows quiet-machine over the same program set (add 5.1-compatible variants of
   benchmark programs as new files where `//` etc. block comparison).
3. Entry/exit protocol design doc (`docs/JIT_DESIGN.md`): E1-E4, resume table layout, the `'frame` check
   and its cost, handoff with `run`'s two copies, OSR entry, exit-at-pc, `trap`/`INTERRUPT` polling, block
   counting (C1). Prototype the `'frame` check and measure the interpreter instruction cost with the feature
   on (budget: <= 0.5% instructions, 0 when the feature is off; **revised 2026-10-29, see section 12 J0 results**).
4. Encoder spike: minimal hand-written x86-64 encoder; test oracle is a disassembler used only as a
   dev-dependency or script (an MIT crate such as iced-x86, or `objdump` in scripts), compared per
   instruction form.
5. Executable memory manager: arenas, W^X flip batching, probe and fallback, code cap, free-on-GC callback,
   `perf` map, frame pointers, GDB JIT interface spike.
6. LuaJIT study (two days): clone `github.com/LuaJIT/LuaJIT` to a gitignored `ref/luajit/`; read recorder,
   IR, snapshots, asm; write `docs/JIT_DESIGN_NOTES.md` mapping mechanisms to our design (study and
   reimplement; no code copying; MIT notice not needed that way).
7. Fixed-seed benchmarking mode and the cycles-based comparison script.
- Gate J0: a compiled empty function round-trips interpreter -> JIT -> interpreter (including OSR entry
  and return into a compiled caller) in a test; template oracle numbers reported; design doc reviewed;
  UNSAFE.md section drafted; `.text` identity check wired into CI.

### J1: Tier 1 core, 5.5 (about 6-8 weeks)
- Hotness: per-prototype counters (call count and back-edge count; thresholds by measurement), synchronous
  compile in the executing thread. OSR entry at loop heads from day one.
- Templates: MOVE, LOADI/LOADF/LOADK/LOADNIL/LOADTRUE/LOADFALSE, int and float arithmetic and bitwise with
  tag-guarded inline fast paths (guard failure = exit at instruction), comparisons and jumps, FORPREP/
  FORLOOP, TEST/TESTSET, GETUPVAL/SETUPVAL, RETURN0/RETURN1, plus per-instruction exits for everything
  not yet covered (so every function compiles, including main chunks).
- Block-level instruction counting (C1), `trap`/`INTERRUPT` polling at back-edges and calls.
- Test modes: `always` (compile everything on first call), `verify` (separate-process whole-program
  comparison of stdout/stderr against `off`; the shipped corpora already do this), and **block-level shadow
  execution** for side-effect-free blocks only (compile-time classification: no heap stores, no calls,
  no allocation): run the block in compiled code and in the interpreter from a saved copy of the frame
  registers and compare. Whole-heap lockstep is explicitly not used (it would execute side effects twice).
  Random **source-level** programs (not random bytecode: binary chunks are trusted, ADR-5) through both modes.
- Gate J1: official suite 31/31 with `always` and default thresholds; tests/vm, differential corpora,
  GC stress 1/2, hooks/Ctrl-C/instruction-limit tests pass; ASan clean; numeric subset improves (target set
  after the J0 oracle; at minimum >= 25% median cycles on loops_arith/sieve/queens/fannkuch/heapsort);
  no program regresses beyond the band; short-script cost within the band.

### J1b (optional, parallel, low risk): interpreter quickening
Use free opcode slots (85 of 128 used in 5.5) for hotness counters and inline-cache fast variants, keeping
the original code for `string.dump` and `getobjname`. Only if the J0 oracle shows tier 1 will leave GETFIELD/
SELF-style programs (oop_methods, json) behind. Independent of the JIT; judged by the same gates.

### J2: Tier 1 completion, 5.5 (about 8-10 weeks)
- Tables: GETTABLE/GETI/GETFIELD/SETTABLE/SETI/SETFIELD inline array and short-string hash probes with
  barriers, SELF, NEWTABLE, SETLIST, LEN, GETTABUP/SETTABUP; CONCAT and CLOSURE via exits/helpers.
- Calls (E2): flat JIT-to-JIT Lua calls with return via the resume table, tail calls, native-function
  calls through the existing inline native path, VARARGPREP/VARARG, TFORCALL/TFORLOOP with an inline `next`
  fast path, CLOSE/TBC; metamethods always exit (E3); coroutine resume enters through E1.
- Gate J2: all gates of the project (official 5.5 31/31 with `always`, cargo test, clippy, fmt, rustdoc,
  deny, ASan, Miri on the non-JIT parts plus a Miri-run **template emulator** (see section 5), fuzz smoke
  with the JIT on and a JIT-enabled count-hook budget path in the fuzz harness); tier-1 targets of section 1
  met, or the gap explained with profile evidence; `.text` of the feature-off build identical to main.

### J2b: 5.4 templates (about 4-6 weeks, after J2)
5.4 forprep forms, NEWTABLE/SETLIST/VARARG forms, and the `alimit_touch` semantics for every integer-key
access (inline paths either perform the touch or exit when the key is past the limit). Same gates under the
5.4 cfg.

### J3: Hardening (about 3 weeks)
- Fuzzing with the JIT on (`run_source` with `always`/`verify` and a JIT-aware budget), a differential fuzz
  target interpreter vs JIT on source-level generated programs, hours-long campaigns (24 h per target is
  the proposed bar; confirm with the user before J4).
- Debug interface on compiled frames (`debug.getinfo/getlocal/traceback`, hooks enabled while compiled
  frames are live), stack reallocation and shrink while compiled frames are live, GC stress with compiled
  code, coroutine stress, `PR_SET_MDWE`/seccomp fallback tests, code-cache eviction under memory limits.
- Checkpoint before tier 1.5.

### J4: Tier 1.5, local type inference (about 8-10 weeks)
**Status (2026-10-30): DONE.** T1-T8, T12 and T13 landed; T9 and T10 were skipped by user decision (call inlining and 5.4 coverage move to J5). T13 targets met on the quiet machine: all 25 programs 0.521 (target <= 0.60), numeric 0.275 (target <= 0.30), vs reference Lua 5.5. Details: `docs/JIT_TIER15.md` sections 22-23, summary in `docs/JIT_DESIGN.md` (J4 section), unsafe inventory in `docs/UNSAFE.md`. **Next: J5 (trace compiler, `docs/JIT_TRACE.md`), to be started in a new session.**
Per-loop analysis over bytecode (int/float/table-shape facts from control variables, constants and guards),
unboxed values in machine registers inside loops, write-back at exits/calls/GC points (invalidation
sources, section 3), simple register allocation (linear scan over the loop body), hoisting of invariant tag
guards and table-array bounds checks where provably safe. Gate: tier-1.5 targets of section 1, all
verification modes, fuzzing.

### J5: Tier 2 numeric-loop traces (stretch; about 8-12 weeks; only after checkpoint)
**Decision (2026-10-29, user): J5 goes ahead as a trace compiler, after the J4 gate (T13).** Sequence: a
design doc (`docs/JIT_TRACE.md`: recorder, SSA IR, snapshots, allocation sinking vs G1, reuse of the J4
encoder/IR/emulator/stub/oracle infrastructure) and then a prototype gate (integer + float `for` loops only,
measured against the J4 region numbers) before the full build. J6 (release) follows J5.
Design status: `docs/JIT_TRACE.md` v1.3 (2026-10-30: review of v1.2 applied; P-gate re-baselined on the T13
numbers, gate set (b) recomposed, prototype tasks P0-P9 with P3a/P3b and P8a/P8b, section 10.1).
Trace recording for `for` loops with numeric bodies, typed SSA IR, snapshots and side exits to the
interpreter, loop-invariant code motion, guard/load-store elimination, trace linking and blacklisting,
abort-reason logging (`RUSTYNAIL_JIT_VERBOSE`), size caps. Traces never cross calls to C functions, pcall,
coroutine switches, metamethods or `__gc` points. Wins to expect from Lua 5.3+ semantics: integers wrap,
so no overflow guards or narrowing pass for integer arithmetic.

### J6: Release (about 2-3 weeks)
`docs/JIT.md` (design, invariants, flags, limits, security notes), README and facade switch (default OFF in
the first release containing the JIT; flip after a release cycle of fuzzing; `sandbox()` preset stays off
until the probe has shipped), CI matrix with the `jit` feature, quiet-machine performance report vs
reference and LuaJIT, reproducibility check, review of executable-memory handling.

## 5. Testing strategy

- Modes: `off`, `on`, `always`, `verify` (separate-process whole-program comparison), shadow execution
  of side-effect-free blocks. No whole-heap lockstep.
- Every existing suite runs under `always` in CI for each tier: official 5.5/5.4, tests/vm, libs1/libs2
  diffs, table_diff, C API differential tests (compiled functions calling C and vice versa), GC stress
  1/2/VERIFY, coroutine corpus, hooks and instruction-limit tests, err_08 style deep-recursion tests.
- Source-level random program generators (arithmetic, table, loop, call, coroutine, metamethod mixes)
  executed in all modes with output compare. Property tests per opcode class: guard-failure exits,
  `savedpc` and `MMBIN` exactness, error messages and line info bit for bit (floats included).
- Miri cannot execute machine code (`cfg(miri)` disables the JIT). To keep helpers and invariants
  Miri-tested, templates are expressed in a small IR (J0 design) that has two backends: the x86-64 emitter
  and a Rust **emulator** that Miri can run. The emulator checks the template logic (guards, exits,
  `savedpc`, write-back) under Miri; the x86 emitter is checked against it by differential tests and by the
  disassembler oracle.
- ASan covers the Rust helpers and loader; generated code is opaque, hence shadow execution and fuzzing.
- Tooling: `RUSTYNAIL_JIT_DUMP=dir` (machine code and pc map), `perf` map file, GDB JIT interface.

## 6. Measurement

- Same harness (`scripts/bench.py`, quiet machine, pinned core) with median cycles:u and instructions:u over
  N >= 7, fixed string-hash seed, a +-5% band; wall time informational. Add compile-time/code-size accounting
  per benchmark and a short-script suite (official test files, `lua -e` startup).
- LuaJIT (`-joff` and JIT) on the shared programs, same method, as context only.
- Code placement noise (+-15% cycles at equal instructions): align function entries and loop heads (16/32
  bytes) and judge by instructions plus geomean across many programs.

## 7. Risks

| Risk | Mitigation |
|---|---|
| Gate unreachable by design (hot code in run-once functions) | OSR at loop heads and per-instruction exits from J1 (fixed in v2) |
| Native recursion overflows the OS stack | E2 flat calls, native nesting bounded by `LUAI_MAXCCALLS` |
| Semantic divergence (errors, hooks, line info, `#t` in 5.4) | E3 exit-before-side-effect, shadow execution, `always` suites, per-opcode `savedpc` tests |
| Memory safety of generated code (invisible to ASan/Miri) | W^X, template-IR emulator under Miri, disassembler oracle, differential fuzzing, review checklist |
| Panics unwinding through generated code | P1: `extern "C"` helpers, aborts on panic |
| GC interaction (stack shrink/move at GC points, barriers, finalizers) | exact top/base discipline, GC stress modes, write-back and re-guard after GC points |
| Sandbox instruction limit bypassed | C1 exact per-block counting |
| Hardened hosts forbid executable memory | runtime probe, silent fallback, tested under `PR_SET_MDWE` |
| Code cache DoS | per-state code cap charged to the memory limit, free with proto, eviction |
| Performance gain below plan | J0 template oracle with a kill/re-plan threshold; tiers gated separately |
| Interpreter regression from hooks | cargo feature F1, `.text` identity when off, <= 0.5% instructions when on |
| Schedule (tier 1 is 3-4 months, tier 1.5 and 2 more) | tiers ship independently; checkpoints; tier 2 optional |
| Noise masks small effects | cycles/instructions medians, +-5% band, fixed seed |

## 8. Decisions (accepted by the user as the defaults, 2026-10-28)

- **D1 Assembler:** hand-written minimal x86-64 encoder; no assembler/codegen crates at runtime. Disassembler
  (iced-x86 or objdump) only as a dev/test oracle.
- **D2 Location:** new crate `rn-jit` for the backend, emitters, memory manager; layout constants and
  layout-dependent emitters live in rn-core next to the layout; plug-in via `State::set_executor`-style
  hooks.
- **D3 Instruction limit/hooks:** JIT applies to the sandbox; exact per-block counting (C1); active line/call
  hooks run the function in the interpreter.
- **D4 Resume into compiled frames:** via the E1 resume table (no separate fallback).
- **D5 Default and scope:** feature `jit`, default OFF in the first release containing it, `sandbox()` preset
  OFF until the executable-memory probe has shipped; Lua 5.5 first, 5.4 in J2b.
- **D6 Platform and dependencies:** x86-64 Linux only; backend behind `arch/x86_64`; no dependency-policy
  change; other targets build with the JIT compiled out.
- **D7 Compilation:** synchronous in the executing thread.
- **D8 Memory:** JIT code counts against the memory limit through a separate code cap; no change to
  `collectgarbage("count")` or GC pacing.
- **D9 Hardened hosts:** supported by silent fallback to the interpreter; dual-mapped (memfd) pages are not
  used unless the probe design in J0 shows they are needed.
- **D10 Gates:** both the numeric subset and the all-programs geomean (section 1 table), judged by cycles.

## 9. Checkpoints and stop criteria

- After **J0**: template oracle projects tier 1 above 0.75x geomean, or the `'frame` entry check costs more
  than 0.5% interpreter instructions with no workaround, or the encoder/memory spikes are unworkable ->
  stop and re-plan or abandon.
- After **J1**: numeric subset at least -25% median cycles with no regressions beyond the band; otherwise
  fix or stop.
- After **J2/J2b**: report against tier-1 targets; tier 1 plateau above 0.80x geomean -> stop and document.
- After **J3**: fuzzing clean for the agreed duration (proposal 24 h per target) before tier 1.5.
- After **J4**: tier 2 starts only if tier 1.5 plateaus short of the stretch targets and the remaining gap is
  numeric-loop shaped. Tier 2 may be abandoned without affecting shipped tiers.

## 10. Rough effort (agent time, sequential, wide margins)

J0 about 2 weeks, J1 6-8, J2 8-10, J2b 4-6, J3 3, J4 8-10, J5 8-12 (optional), J6 2-3. Tier 1 for 5.5 and 5.4
(J0-J3): about 5-6 months. These are estimates from earlier phases and the review, not measurements.

## 11. LuaJIT source as a reference

Source is not on this machine (installed binary and headers only); `github.com/LuaJIT/LuaJIT` is reachable, so
J0 clones it into a gitignored `ref/luajit/`. LuaJIT is MIT licensed; the rule is **study and reimplement, do
not copy source**.

Useful as a reference: trace recorder (`lj_record.c`, `lj_trace.c`: hotcounts, aborts, blacklisting, linking),
IR and optimiser (`lj_ir.*`, `lj_opt_*`: fold rules, loop peeling, load/store forwarding, narrowing, sinking),
snapshots and exits (`lj_snap.c`, `lj_asm.c`), backend (`lj_asm_x86.h`: reverse linear-scan allocation, exit
stubs), DynASM encoding tables, and the NYI list as a map of what is hard to compile. Not a starting point:
different VM (5.1 bytecode, NaN-tagged 8-byte values, own GC and frame model), hand-written assembler
interpreter, macro-heavy C with global state, and no baseline method JIT to borrow. Its `-jv`/`-jdump` output
is a quality oracle for tiers 1.5 and 2 on the shared programs.

## 12. Review log

v1 was reviewed by an independent Opus pass against the code (2026-10-28). Blockers found and resolved in v2:
A1 (J1 gate unreachable: no OSR and function-level bail-out) -> OSR + per-instruction exits; A2 (returns,
metamethod returns and coroutine resume re-enter through `'frame`) -> E1 resume table; A3 (native recursion
overflows the OS stack) -> E2 flat calls; A4 (D3a leaves sandbox and fuzzer interpreter-only) -> C1 exact
per-block counting; A5 (whole-heap lockstep impossible) -> block-level shadow execution, separate-process
verify, source-level random programs. Also adopted: B1 panic invariant P1, B2 GC-point stack discipline, B3
`savedpc` exactness via exit-before-side-effect, B4 invalidation list, B5 5.4 as its own phase, B6 executable
memory probe, B7 code-memory accounting, B8 cycles-based gates, B9 crate split, B10 cargo feature F1; the
J0 criterion replaced by the template oracle; targets re-baselined (tier 1: 0.45x numeric / 0.70x overall;
tier 1.5: 0.30x / 0.60x; tier 2 stretch 0.25x / 0.50x); the LuaJIT baseline rows marked as noisy; the
tier-1.5 stage added; the Miri template-emulator idea added.

## 13. Execution model: Sonnet builders

Decided by the user (2026-10-28): the JIT is implemented by **Sonnet-model builder agents** (`model: sonnet`),
not Opus. The orchestrator (main session) plans, briefs, gates and merges. Consequences for how the work is
cut and checked:

- **Roles.**
  - Orchestrator: owns this plan, `docs/JIT_DESIGN.md`, `docs/UNSAFE.md` sections and the invariants (E1-E4, P1, C1);
    writes each builder brief, runs the gates, merges, and decides checkpoints.
  - Sonnet builders: implement one scoped task each in an isolated worktree, run the gate, commit, report.
    Commits made by builders carry no model-attribution trailer in this repository.
  - Reviewer: an independent read-only pass (Opus is acceptable and preferred here, since no builder shares its
    blind spots) on every unsafe-heavy or protocol-level change before merge: the `'frame` entry check, the
    resume table, W^X memory manager, every template group, block counting, invalidation rules. Reviews are
    cheap relative to a latent miscompile.
- **Task sizing for Sonnet.** One task = one opcode group or one subsystem with an explicit file list, the
  invariants it must respect (quoted from section 3, not paraphrased), the tests that must pass, and a stop
  condition. Prefer many small, mechanical tasks over open-ended ones; no task asks a builder to make an
  architecture decision. Design questions (protocol, IR shape, register allocation strategy) are settled by the
  orchestrator in `docs/JIT_DESIGN.md` first; builders implement against that document.
- **Design-heavy phases stay orchestrator-led.** J0 items 1 and 3 (template oracle, entry/exit protocol),
  tier-1.5 analysis design (J4) and any tier-2 work: the orchestrator writes the design and a reference
  implementation of the hardest piece (for example the first template group end to end); builders then extend
  by pattern. If a Sonnet builder reports a deviation from an invariant, it stops and reports instead of
  improvising.
- **Parallelism.** At most **2 builders run at the same time** (user decision, 2026-10-28; the three J0 builders launched before this rule finish first). Builders run in parallel only on disjoint files (for example: encoder + disassembler tests,
  executable-memory manager, benchmark/measurement scripts, test generators). Template groups for different
  opcode classes touch the same emitter and interpreter hooks, so they are sequenced or split by file with the
  shared interface fixed first. The same rule as the optimisation passes: one agent per hot file at a time.
- **Gates apply unchanged and are re-run by the orchestrator on the merged tree** (not trusted from the
  builder's report): official suites, `cargo test`, clippy, fmt, doc, deny, ASan, the `jit`-off `.text` identity
  check, and the JIT modes of section 5. Builders must show numbers (cycles/instructions, with the +-5% band),
  not conclusions.
- **Verification budget for generated code.** Because Sonnet builders are more likely to miss subtle
  semantic cases (`savedpc` after MMBIN, stack move at GC points, barrier placement), every template group
  ships with: its Miri-run emulator test (section 5), a per-opcode `savedpc`/exit test, and an entry in the
  source-level differential generator. The reviewer checks these exist before merge.
- **Escalation.** A builder that cannot make a gate pass in two attempts, or whose change moves interpreter
  instruction counts with the feature off, stops and reports; the orchestrator (or an Opus pass) takes over.
- **Briefs** follow the template used so far (goal, files, invariants, constraints from CLAUDE.md such as the
  memory caps and timeouts, gate commands, what to report) and always include: the relevant section-3 invariants
  verbatim, the exact test commands, and "do not change files outside this list".

## 14. Changes adopted from the LuaJIT study (docs/JIT_DESIGN_NOTES.md section 12, 2026-10-28)

The study's proposals P1-P12 and how they enter this plan (the notes tag every claim [V]erified/[M]easured/
[R]ecalled/[D]esign proposal; recalled items are not relied on):

1. **Tier 1.5 type source (P1).** Static inference alone leaves most loop live-ins unknown. Tier 1.5 reads the
   *tags of the live-in registers from the frame when the hot counter fires*, specialises with entry guards on
   those tags, and infers locals inside the loop by a bytecode fixpoint. Entry-guard failure exits to the
   interpreter (with backoff, below). This replaces "values known from control variables and constants" as the
   only source and is the main lever for reaching the 0.30x target.
2. **Backoff and blacklisting (P2).** Per (proto, loop head) in a side table, never by patching bytecode: compile
   failure or repeated exits penalise with exponential backoff plus randomness, capped, and blacklist after about
   11 failures. Innermost loops compile first.
3. **Tier 1.5 IR (P3).** Specified in `docs/JIT_DESIGN.md` before J4: basic-block, mutable virtual registers, about
   45 ops, static types `I F B Tab Str Any`. The Miri template emulator (section 5) interprets this same IR for
   tier 1 and tier 1.5, so one emulator covers both.
4. **Exits (P4, P5).** Tier 1.5 uses a shared exit trampoline plus a Rust table-driven restore (exit maps hold slots
   only; our frames are the interpreter's, so no stack-grow check is needed in restore). Every exit kind states in
   the design doc the `savedpc` and `top` it leaves (exit-before-side-effect restarts the instruction; LuaJIT's
   `pc+1` convention does not transfer).
5. **Rule G (roots), new invariant G1.** A collectable value must never live only in a machine register across a
   GC point; deferred write-back (keeping a value in a register past a store) is allowed only for int, float and
   bool. Table barriers: inline black-bit test plus the Rust slow path, skipped for statically int/float/bool
   stores, with a "barrier audit" test mode that checks every emitted store against the helper's behaviour.
6. **Invalidation list (P7).** Adds: locals captured by closures. Open upvalues point into the stack, so such
   registers are "escaped" and must not be register-cached across calls, hook points or GC points.
7. **Stress modes (P8).** Added to section 5: random register-allocation mode, per-pass disable switches
   (`RUSTYNAIL_JIT_OPT=-fwd,-loop,...`), a force-fail-every-guard "exit storm" mode, and a forced stack move at
   every GC point under `always` mode.
8. **Hooks (P9).** LuaJIT's compiled code does not check hooks at all; our exact C1 counting has no precedent to
   copy and gets extra tests (hook enable/disable at every instruction boundary of generated programs).
9. **Optional GC simplification (P10).** Evaluate refusing to shrink the stack of a thread that has live compiled
   frames, only after measuring `collectgarbage("count")` impact (must not change official test output).
10. **Optimisation priority (P11), tier 1.5/2.** Registers for ints and floats first, then table load/store
    forwarding in regions with no calls, then guard hoisting; loop peeling and allocation sinking last or never
    (LuaJIT's own `-O-fwd` costs about 25% on nbody, while `-O-loop` and `-O-sink` showed no visible effect on
    the programs tried).
11. **Section 11 update (P12).** The source is cloned to the gitignored `ref/luajit/` (commit a4c0645). The
    `-jv`/`-jdump`/`-jbc` oracle commands live in `docs/JIT_DESIGN_NOTES.md` section 11.1.
12. **J0 item 2 (LuaJIT baseline).** 15 of the 25 programs run unmodified under LuaJIT; the other 10 fail on `//`:
    closures, coroutines, fasta, heapsort, loops_arith, metamethods, oop_methods, pcall_error, table_array,
    varargs. Two are numeric gate programs (loops_arith, heapsort), so J0 produces LuaJIT-compatible variants of
    those two as new files (`bench/luajit/`), and the LuaJIT comparison table is restricted to programs with
    verified identical output.


## 15. J0 results log (2026-10-29)

- Encoder, executable memory manager, entry/exit design, LuaJIT study, template oracle with tables and calls: done and merged.
  Oracle: GO (projected all-25 geomean 0.57-0.70 vs stop threshold 0.75; tier-1 numeric 0.438 face value, 0.50-0.54 central, so
  the 0.45 numeric target is a stretch and the J1 gate for numeric is <= 0.55).
- Hook cost (docs/JIT_HOOK_COST.md, merged): feature OFF is `.text`-identical to main. Feature ON costs +1.2% instructions
  geomean (+1.5% call-heavy, worst sieve +3.7%) with the best deterministic variant (counter B + placement e). The 0.5% budget
  is unreachable with deterministic counters (floor ~2.5% of fib for one `sub`+`jle` per call). **Decision: the budget is
  revised to <= 1.5% geomean / <= 2% call-heavy for JIT-enabled builds** (the JIT is off by default and pays back far more);
  J1 uses `jit_check_e` + `jit_counter_b`. Timer-sampler hotness is a later option. Open for J1: returns into compiled callers
  (JIT-pending marker in `ci.trap`), tail-call counting, hooked copy has no counters.
- Remaining J0: LuaJIT-compatible benchmark variants + quiet baseline + cycles compare script (builder running), GDB JIT spike (optional).

## 16. J1c results log (2026-10-29)

- J1c (calls, returns, native calls, code lifetime; `docs/JIT_DESIGN.md` section 14, `docs/UNSAFE.md` "J1c additions"): done on its own
  branch, built against J1a only. J1b (arithmetic/compare/upvalue/for-loop templates) is separate, so on this branch compiled code
  still leaves at the first non-template instruction; call protocol numbers are therefore from template-only hot functions
  (cpu 4, release, min of 3, `RUSTYNAIL_JIT=off` vs `on`): `s = id(i)` loop (5e7 calls) 474 -> 253 ms (1.87x); two-level
  `g(id, i)` (3e7) 537 -> 270 ms (1.99x); native `math.floor` from a compiled loop (3e7) 238 -> 201 ms (1.18x).
  bench/programs: fib 877 -> 873 ms, closures 1068 -> 1132 ms (+6%), oop_methods 918 -> 928 ms: no faster yet, because their
  hot functions start with (or are dominated by) exit stubs (LTI, GETUPVAL, ADDI, GETFIELD...). The J1a fib slowdown (975 -> 1716 ms)
  is gone because a prototype whose first instruction is an exit stub no longer gets a function entry (`entry_ok`). Re-measure fib,
  closures, oop_methods after J1b merges: the oracle's 2.6x for fib needs LTI/GETUPVAL/ADDI/ADD templates.
- Review items of the J1a review: code freed with its prototype + charge refunded (done), compiles packed into shared pages
  (`reopen_tail`, done; a failed re-seal aborts), charge is real pages (refunded down to what was sealed), failed compiles refund,
  code-cap exhaustion is `Retry` (not NOJIT), resume entries restricted to valid re-entry points and asserted/tested (done),
  asynchronous hooks fire late (documented in `JIT_DESIGN.md` 14).
- Stress/test coverage: `crates/rn-jit/tests/{jit_calls,jit_stress,jit_lifetime}.rs` (+ shared `tests/progs/mod.rs`), the
  `RUSTYNAIL_JIT_STRESS=stackmove` mode (mutation-checked: dropping the `ReloadFrame` after native calls fails the stress test),
  25 corpus programs under `rnlua` with GC stress 1 and 2 (+verify) and stackmove, `RUSTYNAIL_JIT=verify`, vm_corpus (all 9
  tests: plain, GC stress, incremental) and the official suite with `RUSTYNAIL_JIT=always`, 3200 jit-diff seeds with `JITDIFF_ON=1`.

- J1a (skeleton), J1b (arithmetic/compare/upvalue/float-for templates), harness (tests/jitdiff) merged 2026-10-29; Opus review of J1a: no wrong-result bugs; fixes folded into J1b/J1c. J1b measured (on vs off cycles): loops_arith 0.46x, mandelbrot 0.62x, fib/sieve/nbody/fannkuch 1.00x (not yet compiled: no call/table templates). In flight: J1c (calls/returns/code lifetime), J1d (inline table templates).
- 2026-10-29 end of day: J1a-J1d merged and gated (workspace tests with jit, rn-jit tests, clippy 5.5/5.4/jit, fmt, official 31/31 with jit+always and GC stress 1, jit-diff ~3000+ seeds clean). Opus reviews of J1a and of table/arith templates: no soundness bugs. Measured on vs off cycles (cpu 4): fib 0.52, loops_arith 0.46, mandelbrot 0.62, fannkuch 0.42, matmul 0.40, heapsort 0.57, sieve 0.72-0.77, closures 0.88, oop_methods 1.0, nbody/spectral_norm/table_array/binary_trees 1.0 (need library-call, NEWTABLE, SELF/closure templates). Next: J2 (remaining opcodes: NEWTABLE, LEN, CONCAT, CLOSURE, VARARG, TFORCALL/generic for, SETLIST, upvalue closing, string ops), 5.4 templates (J2b), more stress modes, J1 gate run on full benchmark set.

## 18. J2b-vararg results log (2026-10-29)

- Done (`docs/JIT_DESIGN.md` section 17): TFORPREP/TFORCALL/TFORLOOP (`next` frameless via helper, inline `ipairs` array step), VARARGPREP/VARARG,
  CLOSE/TBC, RETURN with `k`/`C` (upvalue and tbc closing through the interpreter's `close`). A J1c flat-call bug (`B == 0`, exactly one missing
  argument skipped the nil fill) was found by the official suite and fixed.
- Gates: `tests/jit_loops.rs` 14 twin tests (+ stackmove, GC stress 1/2/verify clean), all rn-jit tests, vm_corpus x9 with `always`, official 31/31
  with `always` (jit default feature scratch), jit-diff `JITDIFF_ON=1` seeds 1-3200 and hook=all seeds 3201-3800 clean, clippy 5.5/5.4/jit, fmt,
  jit-off `.text` identical to the J1d base. The jit-diff `stack` mode (stackmove) times out on deep-recursion programs for the J1d base as well
  (quadratic stack copying), so that mode is only meaningful with a long `TMO`.
- Measured (cpu 5, min of 7 cycles:u, `RUSTYNAIL_JIT=on` vs off; J1d base in brackets): varargs 0.867 (1.007), fib 0.510 (0.511), nbody 0.587 (0.587),
  knucleotide 0.887 (0.900), closures 0.895 (0.884), table_array 0.970 (0.990), table_hash 0.971 (0.982), coroutines 0.974 (0.969), oop_methods 1.002
  (1.009); regressions that already exist in the J1d base: binary_trees 1.152 (1.164) and json 1.090 (1.076) (calls into functions whose hot
  instructions are NEWTABLE/CONCAT/string ops exit every time; J2a's templates should fix them).

- 2026-10-29 J2a (helper templates: NEWTABLE, SETLIST, LEN, CONCAT, CLOSURE, POW, table-miss slow paths; JIT_DESIGN.md section 15): gated with rn-jit tests (GC stress 1/2, stackmove), jit-diff 3200 seeds JITDIFF_ON=1 + 701 seeds x4 hook variants clean, official 31/31 (jit default + RUSTYNAIL_JIT=always), jit-off .text identical to main, clippy 5.5/5.4/jit, fmt. On vs off (cpu 4, min of 7, cycles:u): binary_trees 0.80, closures 0.90, fannkuch 0.42, fasta 0.68, fib 0.51, heapsort 0.52, knucleotide 0.90, loops_arith 0.46, mandelbrot 0.59, matmul 0.41, nbody 0.61, oop_methods 0.75, queens 0.55, sieve 0.67, spectral_norm 0.65; flat: coroutines 0.98, gc_churn 0.99, load_big 1.00, pcall_error 1.03, string_ops 0.97, table_array 1.00, table_hash 0.97, varargs 1.00; regressions: json 1.125, metamethods 1.071 (not analysed). Geomean all 25: 0.746 (numeric 0.534).

## 19. J2b-5.4 results log (2026-10-29)

- Done (`docs/JIT_DESIGN.md` section 18): `lower()` accepts 5.4 protos; the 5.5 templates are shared where the layouts agree; 5.4 variants for
  FORPREP/FORLOOP, TFORPREP/TFORLOOP/TFORCALL (incl. frameless `next`/`ipairs` paths), SELF (`RKC`), and the `alimit_touch` of every inline
  integer array access. Exit stubs remaining in 5.4: only what 5.5 also stubs (LOADKX/EXTRAARG, MMBIN* followers, non-numeric K operands,
  long-string constants); nothing 5.4-specific.
- Gates: whole `rn-jit` suite under the 5.4 cfg (the three `*_diff` files un-gated; `named_vararg_tables` gated as 5.5-only; table_diff has limit-hint
  tables and `#t` in its dump; `jit_table::array_limit_hint_sequences`, mutation-checked), also green with GC stress 1, stress 2 + VERIFY, and
  `stackmove` (jit_loops/jit_table/jit_alloc/jit_arith; the corpus and deep-recursion tests are too slow under stackmove, as before); 5.5 suite unchanged
  and green; official 5.4 30/30 with `RUSTYNAIL_JIT=always` (jit as default feature in a scratch edit); official 5.4 with GC stress 1 + `always`:
  28/30, the two failures (gc.lua:473, all.lua which includes it) are identical with `RUSTYNAIL_JIT=off` (stress artifact of the generational
  finalizer wait, see CLAUDE.md); jit-diff 5.4 binary seeds 1-1500 `JITDIFF_ON=1` and 1501-1800 `JITDIFF_HOOK=all`: 0 diffs; clippy -D warnings
  5.5/5.4 x with/without jit; fmt; jit-off release `.text` identical to the base commit for 5.5 and 5.4.
- Measured (cpu 5, min of 7, 5.4 fast build, on vs off): `docs/JIT_BENCH.md` "Lua 5.4": geomean all 0.739, numeric 0.564.

## 21. J1/J2 gate measurement (2026-10-29, quiet machine, main after 5.4 merge)

`scripts/bench-cycles.sh` (cpu 4, min of 7, fixed hash seed, `RUSTYNAIL_JIT=on`), results in bench/results/jit-j2.json. Cycle ratio vs reference Lua 5.5:

| build | all 25 (cycles) | numeric subset |
|---|---:|---:|
| interpreter (JIT off) | 0.853 | 0.964 |
| JIT on (tier 1) | **0.627** | **0.528** |
| LuaJIT 2.1 (23 programs) | 0.243 | 0.142 |

Targets (section 1): tier-1 all-25 <= 0.70 -> MET (0.627); tier-1 numeric <= 0.45 -> NOT met (0.528; the J0 re-gate was <= 0.55, met). Per program (JIT/ref): matmul 0.38, fannkuch 0.42, loops_arith 0.48, fib 0.54, heapsort 0.56, nbody 0.58, mandelbrot 0.58, spectral_norm 0.58, queens 0.53; weakest: metamethods 0.96, gc_churn 0.79, knucleotide 0.89, json 0.82, table_hash 0.82, sieve 0.77. Remaining gap to LuaJIT is the J4 (tier 1.5, values in registers) target.
## 22. J3c results log (2026-10-29)

Remaining J3 hardening items (debug interface, stack move/shrink, GC stress, coroutine stress, MDWE/seccomp, code cap). Tests:
`crates/rn-jit/tests/jit_j3c.rs` (+ `tests/j3c/*.lua`: twin of jit-off / `always` / three count budgets per program, 11 programs) and
`jit_j3c_env.rs` (child-process restrictions, code cap). Both pass on 5.5 and 5.4 cfgs, plain and under `RUSTYNAIL_GC_STRESS=1` and `=2`,
`RUSTYNAIL_JIT_STRESS=stackmove` (smaller depths; no 300k-frame overflow) and stackmove+GC stress 2.

- Covered: `getinfo` (all letters, levels through compiled frames, tail calls, vararg, method names, `currentline` at the exit of 12
  instruction classes via `xpcall` + traceback), `getlocal/setlocal` (register locals, the visible control variable, internal `(for state)`
  slots, generic-for state, tbc replacement, varargs, a callee writing its caller's locals, temporaries), `traceback` (own, other coroutine,
  suspended mid compiled loop, dead with error), hooks (call/return/line/count installed by callees, by `__gc` finalizers at allocation
  points, from other coroutines while the target is suspended inside a compiled loop, self-removing, inherited by new coroutines,
  `ftransfer/ntransfer`), `upvalueid/upvaluejoin/setupvalue` on running functions and on closures made in compiled loops; stack growth
  by deep recursion in loops with register locals live, shrink by `collectgarbage()`, yield from depth + shrink of a suspended
  coroutine, unwinding through pcall/xpcall with growing handlers, handler errors/overflow, stack overflow text and recovery, depth reached
  at "C stack overflow" for 13 nesting forms (pcall, `__index/__call/__add/__concat/__len/__tostring/__close`, gsub, sort, nested resume):
  identical to the interpreter; 300 coroutines (yield across pcall, nested wrap generators, close with pending tbc, close inside loops);
  suspended coroutines being the only owners of compiled prototypes (code stays alive, is freed when they die); write barriers into old
  objects, mode switches in compiled loops, weak tables, resurrection and erroring finalizers.
- Child-process tests: `PR_SET_MDWE`, a seccomp filter failing `mmap`/`mprotect` with `PROT_EXEC` (before the probe: JIT unavailable;
  after the probe: every compile fails), `RLIMIT_AS` making the code-chunk `mmap` fail: all give the interpreter's result with 0 enters.
  Known gap pinned by a test: a filter that appears *after* code was compiled makes the next compile abort the process (docs/JIT.md).
- Code cap: no eviction of live code existed. Implemented the simplest sound policy, which the arena already supported: at the cap a
  compile is refused (`Fail::Retry`, back-off of 32), freed space is reused. New knobs `RUSTYNAIL_JIT_CODE_CAP` / `rn_jit::set_code_cap`
  (`CodeArena::set_cap`). Tests: 3000 distinct hot functions under caps of 64/200/1100 KiB above the baseline (correct, mapped <= cap, many
  times the cap generated, mapped back to baseline + at most one pinned chunk after collecting), retained functions (`refused > 0`, resumes
  after they die), zero cap, 5 rounds of 1500 load()s (mapped and charged bytes do not grow).
- Bugs found in the JIT by these tests: none. Test-harness pitfalls fixed on the way: tail-called `__call`/`__tostring` chains recurse forever,
  `table.sort` with a recursive comparator and a `__close` that recurses are exponential, an `xpcall` handler overflow is quadratic under stackmove.

## 20. J3b results log (2026-10-29)

- Hardening: `scripts/asan-jit.sh` (official suite with `always` and `on`, tests/vm corpus off-vs-always, GC stress 1, stackmove, all rn-jit tests; 5.5): `ASAN-JIT OK`, no JIT report. Findings were harness-only (iced-x86 static tables and the tests' `Box::leak`ed interrupt flag reported by LSan: suppressed in `scripts/lsan-jit.supp`; ASan refusing the corpus' deliberate absurd allocation: `allocator_may_return_null=1`; `co_09` under stackmove is quadratic, skipped).
  `scripts/miri.sh jit`: 12 tests (monitor, encoder basics, `emu_diff`) clean under Stacked and Tree Borrows, 5.5 and 5.4. Not Miri-runnable: `table_diff` (the emulator dereferences GC addresses read back as integers: provenance-less pointers) and the iced-x86 oracle tests (Stacked Borrows UB inside iced-x86 1.21).
- Memory: descriptors, resume tables, monitors and the compile's lowering buffers are now charged to the memory limit (`rt::compile`, refunded with the prototype / at the end of the compile); tests `crates/rn-jit/tests/jit_memlimit.rs`. Facade: `Lua::set_jit_mode`, `Builder::jit`, `Lua::jit_stats` (default Off, sandbox preset unchanged), tests `crates/rustynail/tests/jit.rs` (5.5 and 5.4), including the count-hook behaviour of the instruction limit.
- CI: `jit` and `asan-jit` jobs, `miri-jit` (scheduled), `scripts/ci-local.sh` jit-* steps, `scripts/jit-text-identity.sh`. User docs: `docs/JIT.md`.

## 23. J3 completion (2026-10-29)
J3 done without the 24 h bar (user decision): J3a fuzz targets, J3b ASan/Miri/CI/facade/docs, J3c debug-interface/stack/coroutine/MDWE/code-cap tests (no JIT bugs found), J3f 60 min each of jit_diff (75k execs, cov 20.3k->23.5k) and run_source+jit (510k execs, cov 18.6k->21.3k) plus 5 min each of the five existing targets: no crashes, no divergences; the only defects were fuzz-harness bugs (tick budget reset), fixed. Known gap: seccomp denying exec mprotect after code was already compiled aborts the process on the next compile (documented in docs/JIT.md). Checkpoint before J4: tier-1 quiet numbers in section 21.

## 24. J4 completion (2026-10-30)
J4 (tier 1.5 regions) done: T1-T8, T12, T13; T9/T10 skipped (user decision). T13 quiet-machine geomeans vs reference: all-25 0.521, numeric 0.275 (tier 1: 0.632 / 0.531); both J4 targets met (`docs/JIT_TIER15.md` section 23, `docs/bench-t13-j4.txt`). Tier 1.5 is off by default (`RUSTYNAIL_JIT_T15`). J5 (trace compiler, `docs/JIT_TRACE.md`) is next, to be started in a new session; the T13 table is its prototype-gate baseline.
