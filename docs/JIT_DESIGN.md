# JIT design: entry/exit protocol and tier-1 structure

Status: DRAFT (J0, 2026-10-28). Implements the invariants E1-E4, P1, C1 of `docs/JIT_PLAN.md` section 3.
Builders implement against this document; deviations are reported, not improvised. References are to
the code at main `40217b0`+ (vm_unsafe.rs, call.rs, state.rs).

## 1. Vocabulary

- **Interpreter** = `run`/`run_mode::<HOOK>` (vm_unsafe.rs:473-,509-). **Compiled code** = machine code emitted by
  `rn-jit` for one `Proto`.
- **Compiled-code entry** (a *resume point*): a native address plus the bytecode pc it corresponds to.
- **Exit**: compiled code returns control to the interpreter at a bytecode pc with `ci.savedpc` set; the
  interpreter continues from there (instruction not yet executed unless stated).
- **Pinned registers** inside compiled code (System V; callee-saved ones survive helper calls):
  `rbx = base` (pointer to register 0 of the running frame), `r12 = &mut State`, `r13 = ci` (`*mut CallInfo`),
  `r14 = kcache` (`*const Value`), `r15 = ` the proto's *block counter / jit data pointer* (see 5). `rbp` frame
  pointer chain for debuggers (P1/debuggability). `rax, rcx, rdx, rsi, rdi, r8-r11, xmm*` scratch.

## 2. The single entry mechanism (E1)

Under the `jit` cargo feature, `Proto` gets one extra field `jit: *mut JitProto` (null = not compiled) and a hot
counter. `JitProto` holds: the resume table, native entry addresses, code block handle, per-pc metadata
(static instruction counts, exit stubs), generation id, code-cache links.

**Resume table.** A sorted array of `(pc_index: u32, native_offset: u32)` for every point at which compiled code may
be entered:
1. function start (pc 0) and the first instruction after VARARGPREP (see 7);
2. every loop head (target of a backward jump, FORLOOP/TFORLOOP targets) = OSR points;
3. the instruction *after* every call-like instruction that can return into the frame via the interpreter
   (CALL, TAILCALL does not return, TFORCALL), i.e. call-return sites;
4. the instruction after every instruction that can complete a metamethod in place (the interpreter's
   `mm_return` target: `savedpc` as left by `finish_op` rules), and the instruction after every instruction that
   can resume after a coroutine yield (`finish_op` targets);
5. any pc where compiled code exited, so that the interpreter can hand the frame back (optional in J1: the
   interpreter re-enters only at the points above).

Lookup is `binary_search` on the table (cold: it happens at frame restart, not per instruction) or a direct
`Vec` indexed by pc (fast: one load; memory = 4 bytes per instruction, accepted for J1).

**Interpreter hook.** At the top of `'frame: loop` in `run_mode` (after the trap handling, before the main
dispatch), under `#[cfg(feature = "jit")]`:

```
if proto.jit != null { if let Some(addr) = jitproto.resume_addr(ci.savedpc) { match jit_enter(st, ci, addr) { ... } } }
else if hot(proto) { compile(proto) ... }
```

`jit_enter` is an `extern "C"` thunk (hand-assembled in rn-jit) that saves callee-saved registers, loads the
pinned registers from `st`/`ci`, jumps to `addr`, and on return restores the registers and yields a `JitExit`
value in `rax` (a small `u32` code plus data in `st`/`ci`). The interpreter then acts:

| JitExit | meaning | interpreter action |
|---|---|---|
| `Continue` | compiled code stored `ci.savedpc` and wants the interpreter to execute from there (unsupported opcode, slow path, guard failure, hook/trap, instruction-count expiry, GC point needing the interpreter) | `continue 'frame` after the usual `trap` handling; the frame restarts at `ci.savedpc` (hook-mode switch rules as today) |
| `FrameChanged` | compiled code pushed or popped Lua frames (flat call into an uncompiled callee, return to an uncompiled caller, tail call) and left `st.ci` = new frame with its `savedpc` stored | `ci = st.ci; continue 'frame` |
| `Returned` | the `CIST_FRESH` frame returned (what `return Ok(None)` does today) | `return Ok(None)` |
| `Error` | a helper reported a Lua error (status in `st`) | `return Err(status)` after the same unwinding as `protect!` does |
| `Yield` | not used in J1: yields happen only inside native helpers called through the existing paths, which already report `Err` | n/a |

Everything that today restarts a frame (CALL, `do_ret!`, `mm_return`, `unroll` after coroutine resume) already goes
through `'frame`, so one hook there serves function entry, OSR (loop head resume addresses when `savedpc` is a
loop head after the interpreter hits the hot counter), returns into compiled callers, and coroutine resume. No
other entry path exists.

**Cost budget.** Feature off: zero (the code is not compiled; the `.text` identity check is part of every gate).
Feature on: one null-check of `proto.jit` per frame restart, out of line for the hit case. Measured in J0
(prototype in `tools/` or a branch) with a budget of <= 0.5% interpreter instructions on the call/return heavy
benchmarks (fib, closures, oop_methods). If the budget is exceeded, move the check into `pre_call_lua_fast` and
`do_ret!` selectively (callee-compiled flag in `CallInfo.callstatus`).

## 3. Hotness and OSR

- `Proto.hot: u32`, incremented by the interpreter at (a) frame start (+weight for calls) and (b) FORLOOP/
  backward JMP/TFORLOOP (+1) *only when the `jit` feature is on*. Thresholds: function-entry 200, loop 1000
  initially (tuned in J1 by measurement; env `RUSTYNAIL_JIT_THRESHOLD=entry,loop`). On crossing, the interpreter
  calls `jit_compile(st, proto)` (synchronous, D7). Compile failure (unsupported feature, code cap exceeded,
  probe failed) marks the proto `jit = NOJIT` so it is not retried.
- Counting in the interpreter at back-edges adds instructions to the hottest loop. Mitigation: count only at
  FORLOOP/JMP-back/TFORLOOP via the existing `updatetrap!` structure: e.g. place the counter on the *proto* as a
  decrementing budget and test `== 0` together with `ci.trap` (one load/branch pair already exists at jumps). If
  that does not fit, use the J1b quickening variant (count in a patched opcode). Decided by measurement in J0.
- OSR: when the counter fires inside a loop, compile, then re-enter at the loop head resume address for the
  current `savedpc` immediately (the frame layout is identical, so nothing else needs translation).

## 4. Calls and returns inside compiled code (E2)

- **CALL to a Lua closure:** compiled code calls the helper `jit_precall(st, ci, a, nargs_plus1, nresults)` which is
  `pre_call_lua_fast` + the same stack-size and `CallInfo` handling (extern "C", returns the new `ci` or null for
  "use the interpreter"). If the callee proto is compiled (`proto.jit != null`) and has an entry for pc 0, compiled code
  loads `r13 = newci`, `rbx = new base`, `r14 = callee kcache` and *jumps* to the callee entry (no native recursion,
  no native stack growth). Otherwise it exits `FrameChanged` with `st.ci = newci`.
- **RETURN:** compiled `RETURN0/RETURN1/RETURN` template performs the result move (same code as `do_ret`'s
  `post_call`), pops `st.ci = prev`, then: if `prev` is a Lua frame whose proto is compiled and
  `resume_addr(prev.savedpc)` exists, jump there (load `rbx/r13/r14` for the caller); otherwise exit
  `FrameChanged`. If the returning frame has `CIST_FRESH` -> exit `Returned`; if `CIST_MMRET` -> exit
  `FrameChanged` so the interpreter runs `mm_return` (compiled code never completes metamethods).
- **Native functions:** through the existing inline native call path (`ccall`-style helper), `extern "C"`, status
  returned in `eax`; a non-OK status exits `Error`. After any helper that can move the stack or run GC:
  reload `rbx` from `st` (and `r14` if the frame was reentered) per section 6.
- Because Lua-to-Lua calls never recurse on the native stack, the OS stack depth of compiled code is bounded by
  helper nesting (`LUAI_MAXCCALLS`), exactly like today (tests/vm/err_08_stack_overflow.lua must pass with
  `RUSTYNAIL_JIT=always`).

## 5. Exits, per-instruction fallback (E3) and counting (C1)

- **Exit stub:** `mov dword [r13 + off(savedpc)] ...` cannot hold a 64-bit pc cheaply, so each exit stub loads
  `rax = <address of the instruction in the code array>` (an absolute immediate known at compile time; the code
  array of a `Proto` is stable while the proto lives) and stores it to `ci.savedpc`, then `mov eax, JitExit::Continue;
  jmp common_exit`. `common_exit` restores callee-saved registers via the thunk epilogue.
- **Unsupported opcode / slow path / guard failure:** jump to a per-instruction exit stub *before any side effect*.
  The interpreter re-executes that instruction from scratch, handling errors, metamethods, allocation, and GC.
  Fast-path templates must therefore be *pure until committed*: compute into scratch registers, check guards,
  then store results; a failing guard leaves the Value stack untouched.
- **Instruction counting (C1):** each basic block has a static `n` (bytecodes it contains, adding the interpreter's
  MMBIN accounting: a fast arithmetic path counts one instruction, the slow path two). Block head template:
  `sub dword [r12 + off(hookcount)], n ; js/jle exit_at_block_start`. The exit stub stores `savedpc = block start` and
  returns `Continue`; the hooked interpreter (HOOK = true) then executes the block exactly and fires the count hook
  at the right instruction. Only emitted when a count hook is active at compile time? No: hooks can be set
  later, so the check is always present but folded: the counter word is `hookcount` when a count hook is set, and a
  very large value otherwise; line/call hooks set `ci.trap` which compiled back-edges and calls test (7).
- **Interrupt/trap polling:** at back-edges and call/return sequences: `cmp dword [r13 + off(trap)], 0 ; jne
  exit_trap` and `cmp byte [INTERRUPT], 0` (a single combined flag word is a later optimisation, see PERF.md
  "Not adopted"). `exit_trap` stores `savedpc` and returns `Continue`.

## 6. Stack, GC and barrier discipline (invariants for every template)

- `rbx` (base) is valid only until the next helper call, GC point, or call/return transition. After every
  helper that can allocate, grow/shrink the stack, call Lua/C, or run finalizers (`check_gc`, `pre_call`, table
  resize, string creation, metamethod, ...): reload `rbx` from `st` (`slot_ptr(ci.func + 1)` equivalent: load the
  stack base pointer from the thread and add `(ci.func+1)*16`). `stack_gen` debug assertions remain in the helpers.
- Before any GC point `top` is set exactly as the interpreter does (`check_gc!`: `savedpc`, `set_top(lim)`): slots at
  and above `top` are cleared at atomic. The templates never *call* the GC themselves in J1: allocation sites
  (NEWTABLE, CONCAT, CLOSURE, string-creating ops) are exits or helper calls that include the `check_gc` sequence.
- Barriers: table stores use `barrier_back` semantics, SETUPVAL the forward barrier; emitted by the same helper
  functions the interpreter uses (`extern "C"` wrappers) in J1; inline barrier fast paths (test `marked` bits) come
  with the table templates in J2 with a documented bit-for-bit match to the helper, and tests that run the GC in
  incremental and generational modes with compiled code.
- No machine code embeds a pointer to a collectable object other than: the proto's code array address (kept alive
  by the proto), and `kcache` via `r14`. Constants are read through `r14`, never embedded.
- Panics: helpers are `extern "C"`; compiled code never calls C-API functions directly (P1).

## 7. Frame setup details

- **VARARGPREP/VARARG:** J1 treats VARARGPREP as an exit instruction (frame is set up by the interpreter, which then
  continues; the first resume point is the instruction after VARARGPREP). VARARG itself exits in J1, template in J2.
- **TBC / CLOSE:** CLOSE and TFORPREP's tbc marking exit in J1.
- **Hooks active:** `jit_enter` is not called when `hook_mask != 0` or `ci.trap` is set; the `'frame` hook tests
  `HOOK == false` copy only. A hook enabled while compiled code runs is noticed at the next back-edge/call poll or
  block counter, which exit to the interpreter.
- **Stack reallocation while a compiled frame is live:** the Value stack can move at any helper; compiled frames
  keep no stack pointers across helpers except through `rbx` reload. `CallInfo.func/top` are indices already.

## 8. Template IR and the Miri emulator

To keep template logic checkable under Miri (which cannot execute machine code), templates are written once as a
small straight-line IR of "micro-ops" (load tag, compare tag, load payload, int/float ops, store tag+payload, branch
to exit, ...), with two backends: the x86-64 emitter and a Rust interpreter of micro-ops operating on a real
`Value` slice. A template is a Rust function producing micro-ops for one bytecode; both backends run the same
output. Tests compare emulator results with the interpreter for randomly generated operand values, and the x86
backend against the emulator under a native-only differential test.

## 9. Debugging and tooling

- `RUSTYNAIL_JIT=off|on|always|verify` (builder option equivalent); `RUSTYNAIL_JIT_DUMP=dir` writes machine code and
  the pc map per proto; `RUSTYNAIL_JIT_PERFMAP=1` writes the `perf` map; `RUSTYNAIL_JIT_VERBOSE=1` logs compile/exit
  counts. `jit_stats()` returns per-proto entries/exits for tests. Frame-pointer chains in compiled code and the
  GDB JIT interface (spike) for backtraces (CLAUDE.md's `coredumpctl` flow).

## 10. Open items settled during J0/J1 (measurement)

1. Hot-counter placement (section 3): measured cost of the two variants.
2. Resume lookup: direct `Vec` vs binary search.
3. Whether `jit_precall` can inline the `CallInfo` setup (stack-size check and `next` ci) in machine code.
4. Exit stub sharing (one stub per block vs per instruction) vs code size.

## 11. As built: J1a skeleton (2026-10-29)

What exists: crate `rn-jit` with feature `jit` (rn-cli/rustynail `jit` = rn-core `jit + jit_counter_b + jit_check_e` +
`dep:rn-jit`; `rn_jit::register()` is called by `rnlua` `main` and `Builder::build`). Files: `ir.rs` (micro-op IR: 7
virtual registers, loads/stores of 1/4/8 bytes relative to frame/ci/thread/K/budget, add/sub/and/udiv, compare-branch,
`CountSub`, `Ret`), `emu.rs` (Rust emulator of the IR), `x86.rs` (IR -> x86-64, thunk), `lower.rs` (bytecode -> IR), `rt.rs`
(config, per-state arena, compile driver, enter, registration), `rn-core/src/jit_hooks.rs` (hook registry).

Templates: MOVE, LOADI, LOADF, LOADK, LOADFALSE, LFALSESKIP, LOADTRUE, LOADNIL, JMP, FORPREP/FORLOOP (integer loops,
including the native unsigned division of the iteration count; float loops, zero step and non-integer operands exit at the
instruction), RETURN0, RETURN1. Everything else is an exit stub (`savedpc = &code[pc]`, `Continue`). Blocks are counted
with `CountSub` (C1); guarded terminators count themselves after their guards. Back-edges poll `ci.trap` and `INTERRUPT`.

Entry protocol as implemented (differences from sections 2-5 are marked **DEV**):

- **DEV, entry flag instead of a `'frame` head test.** Placement (e) counts in `pre_call*` (`Proto.hot`, weight 5) and the
  loop tick (counter B) fires at back-edges. Neither site can run the frame itself, so they *flag* it:
  `ci.trap = jit_hooks::TRAP_JIT (2)`. The existing `ci.trap != 0` branch of the fast copy's `'frame` head consumes the flag:
  if hooks and interrupt are clear it resets `trap = 0` and calls `jit_hooks::enter`; the exit is handled right there
  (`Continue` falls into the normal dispatch at `savedpc`, `FrameChanged` -> `ci = st.ci(); continue 'frame`, `Returned` ->
  `return Ok(None)`). The hot path therefore gets no instruction beyond the already measured hooks (the flag test sits behind
  the `trap != 0` test that was there before). The hooked copy rewrites a stray flag to an ordinary trap. A compiled proto
  keeps `hot` parked at `COMPILED_PARK`, so every call flags its frame (this is the "JIT-pending marker in `ci.trap`" of J0).
- OSR: `hot_edge!` calls `jit_hot_loop(st, ci, pc)` when the tick fires. It compiles the running proto if needed and returns
  true iff `pc` has a resume entry (loop heads, function start, and every pc following an exit stub); the interpreter then
  stores `savedpc = pc`, sets the flag and restarts the frame.
- Resume table: `Vec<u32>` indexed by instruction (native offset, `u32::MAX` = none), one load.
- **DEV, hooks are detected through `ci.trap`.** RETURN0/1 exit at their own instruction when `ci.trap != 0` (the fast copy
  relies on the same invariant: hooks set `trap = 1` on all Lua frames, new frames get it from `OP_CALL`/`run`); `hook_mask`
  has no stable offset for machine code. `CIST_MMRET` frames also exit at the RETURN (the doc said `FrameChanged`, which would
  skip `mm_return`; the interpreter must execute the return itself).
- **DEV, `r15` = address of the budget word** (an `i32` in the boxed per-state data, reseeded to `i32::MAX` at every entry),
  not a proto pointer: with no helper calls there is nothing else to address. Because compiled code is never entered while a
  hook is installed, the budget only matters for hooks installed asynchronously and for the tests (`set_count_budget`).
- **DEV, returns never jump into a compiled caller yet** (RETURN0/1 pop the frame and exit `FrameChanged`/`Returned`;
  the caller continues in the interpreter and re-enters compiled code only through the flag/OSR). Planned for J1c together with
  flat calls.
- Per-state data lives in `GlobalState.jit` (opaque pointer + `VTable::free_state`), the code arena is
  `mem::CodeArena` with `Mem::charge_ext` accounting (`ext` only: the limit sees it, `collectgarbage("count")` does not).
- Knobs: `RUSTYNAIL_JIT=off|on|always|verify` (default `on`; `verify` = `always` + quarantined arena),
  `RUSTYNAIL_JIT_THRESHOLD=entry,loop` (`entry` = calls before compiling, `loop` = back-edges per tick; `always` = `1,1`;
  defaults 200 and 1000). `off` installs nothing. Thresholds are process-wide and must be set before the first state/proto is
  created (`Proto.hot` and the thread tick are initialised at creation).
- Compile failure (5.4 build, > 65536 instructions, jump out of range, W^X probe failure, code charge refused) marks
  `Proto.jit = 1` and parks the counter at `PARK_HIGH`.

Known gaps: no per-proto freeing of code/descriptors (freed at state close; needs the `gc/obj.rs` callback); no helpers, so
no calls, tables, arithmetic, comparisons, upvalues (all exit); returns into compiled callers; `RUSTYNAIL_JIT_DUMP/PERFMAP`
beyond the arena's `PerfMap` symbols; 5.4 refuses to compile; `jit_counter_a/c/g` placements only count (no compiler path).
Measured (single run set, cpu 4, min of 3, release): a template-only loop (`a=7 b=a` x 3e8) 720 ms -> 263 ms (2.7x);
loops_arith 1199 -> 1133 ms (`on`), 2286 ms (`always`); sieve 1108 -> 1015 ms (`on`), 3271 ms (`always`); fib 975 -> 1716 ms
(`on`; every call enters and immediately exits at its first stub). These are skeleton numbers: nearly every instruction exits.

## 14. As built: J1c calls, returns, native calls, code lifetime (2026-10-29)

Files: `crates/rn-jit/src/calls.rs` (child module of `lower`; templates and shared stubs), `rn-core/src/jit_helpers.rs`
(the `extern "C"` helpers), new IR ops (`Shl`, `SetFrame/SetCi/SetK`, `JmpR`, `ReloadFrame`, `CallH`; native code only, the
emulator reports `Stop::Unsupported` for the last five), `rt.rs` (`JitProto`, `free_code`, charging, `enter`).

**CALL (flat call, E2).** Guards before any side effect: `thread.hookmask != 0` leaves at the CALL (`Continue`, the interpreter
performs the call), `INTERRUPT` leaves with `trap = 1`, then `CountSub{1}` (the instruction counts itself, so an exit never
double counts). Then `thread.top` = end of the arguments (B == 0: already set) and `ci.savedpc` = the instruction after the CALL
(the interpreter's `savepc!()`). Fast path, taken when the callee is a Lua closure whose proto is compiled with a function entry
(`JitProtoHdr.entry0 != 0`), `nargs >= numparams`, `stack_size - top > maxstacksize` and `ci.next != null`: the next `CallInfo`
is filled exactly like `pre_call_lua_fast` (`func`, `top`, `callstatus = nresults + 1`, `trap = 0`, `savedpc = code`,
`kcache = k`), `thread.ci` is set and `r13/rbx/r14` are loaded; control jumps to `entry0`. No hotness counter is touched (the
callee is compiled; its `Proto.hot` stays parked). Everything else (natives, `__call`, uncompiled callees so that the hot
counters still run, missing arguments, stack growth, extending the `CallInfo` list, hooks) goes through `jit_call`, the
interpreter's `OP_CALL` slow path: `H_DONE` (native finished: `ReloadFrame`, poll `hookmask`, continue at `pc + 1`), `H_LUA` (a
frame was pushed: the shared `enter` stub reloads `r13/rbx/r14` from `thread.ci`, clears the `TRAP_JIT` flag and jumps to the
callee's entry if compiled, else exits `FrameChanged`), `H_ERR` (status in `JitSlot.err`, exit `Error`, the interpreter does
`return Err(take_error(st))`; yields are `Err(Yield)` and work the same way).

**RETURN0/RETURN1/RETURN.** The J1a result moves, then the shared `ret_tail`: `CIST_FRESH` -> `Returned`; else the caller
(`thread.ci`, already popped to) is resumed in compiled code when its proto is compiled and its resume table has an entry for
`caller.savedpc` (table of native offset + 1 per instruction, indexed by the byte offset; 0 = none); the target is
`base_m1 + entry`, with the caller's `r13/rbx/r14` loaded. Anything else exits `FrameChanged`. `CIST_MMRET` frames and
`ci.trap != 0` leave at the RETURN (the interpreter performs it), `INTERRUPT` leaves with `trap = 1`. Generic `RETURN`
(fixed or multiple results, `k == 0`, `C == 0`) is a template with the copy/nil-fill loop; with `k`/`C` set or `CIST_TBC` it is
an exit stub. `rt::enter` turns a `Continue` exit whose `thread.ci` differs from the entered frame into `FrameChanged`.

**TAILCALL.** Guards as CALL plus `CIST_MMRET`; `jit_tailcall` is the interpreter's `OP_TAILCALL` (closes upvalues, `pre_tail_call`).
`H_LUA`: the frame was rewritten in place, `enter` continues in compiled code or exits `FrameChanged`; `H_RETURNED` (native
tail callee finished and popped the frame): like RETURN after a `hookmask` test.

**Valid re-entry points** (`Lowered::resume`, `calls::valid_reentry`): pc 0, back-edge targets of JMP/FORLOOP/TFORLOOP, and the
instruction after every CALL. J1a also recorded the instruction after *every exit stub*; that was wrong for re-entry (the
instruction after an arithmetic op is its MMBIN, after NEWTABLE/SETLIST an EXTRAARG, after a compare the JMP it consumes) and is
gone. `lower` debug-asserts `valid_reentry` for the after-CALL entries; `tests/jit_calls.rs` checks it on real protos.

**Function entry only when it pays.** A prototype whose first instruction is an exit stub (`LowerCfg`/`Lowered::entry_ok`) gets
`entry0 = 0`: calls to it keep going through the interpreter (the J1a fib slowdown was exactly "enter, exit at the first stub"
on every call); its loops can still be entered (OSR). With J1b's templates most functions will have an entry.

**GC / stack discipline.** `top` is set before every helper like the interpreter's `Protect`; every helper return reloads
`rbx` (`ReloadFrame` = `stack ptr + (ci.func << 4) + 16`); no stack pointer survives a helper. `RUSTYNAIL_JIT_STRESS=stackmove`
(`jit_hooks::set_stress_stackmove`, tests call it directly) makes every helper move the stack to a fresh block and poison the
old one on return (`state::move_stack`; it does not set `trap`, so compiled code must cope): removing the `ReloadFrame` after
native calls fails `tests/jit_stress.rs`.

**Code lifetime.** `Proto.jit` is a `Box::into_raw` `JitProto` (`#[repr(C)]`, `JitProtoHdr` first). `gc/obj.rs::free_proto`
calls `jit_hooks::free_proto_code`, which asks the compiler to free the block (`VTable::free_code`) and refunds the code charge
(`Mem::uncharge_ext`). A collected prototype has no live frame (the frame's function slot keeps its closure alive, and a RETURN
only overwrites that slot as its last act before leaving the code), so no thread executes in the freed block; the owner
`JitData` outlives the heap's `free_all_objects` (field order of `GlobalState`). Compiles are packed: `CodeArena::reopen_tail`
flips the partially used last page back to RW so the next function continues in it (a compile runs no generated code, so
nothing executes there during the window; the commit of the same compile seals it again, and a failed reseal aborts the
process: the only alternative is executing a non-executable page). `CodeArena::free` returns the whole pages inside a freed
block to the OS (`MADV_DONTNEED`); address space inside a chunk is reused only when the chunk empties. Charging: whole pages up
front, refunded after the commit down to what was actually sealed (>= the 16-byte aligned size); failed compiles refund. A
refused compile (code cap, memory limit) is `Retry`, not `NOJIT`: the proto counts again from the entry threshold and the next
32 compile requests are refused without work.

**Known gaps / limitations.** TFORCALL, CLOSE, VARARG and everything J1b owns remain exit stubs, so on this branch compiled
code leaves at the first non-template instruction: calls between compiled frames only stay compiled when the code between them
is template-only (see the measurements in `docs/JIT_PLAN.md`, section 15). Hooks installed *asynchronously* (another thread
calling `lua_sethook`) are noticed at the next CALL/RETURN/back-edge poll (a count hook fires late; synchronous installs from a
native called by compiled code are noticed right after that native returns). A return from an interpreter-run callee into a
compiled caller still continues in the interpreter (only compiled callees jump back).

## 12. As built: J1b arithmetic, comparison and numeric-for templates (2026-10-29)

New file `crates/rn-jit/src/templates_arith.rs` (safe code; `lower.rs` only dispatches to it). IR additions (`ir.rs`,
`emu.rs`, `x86.rs`): `Mul Or Xor Neg Not ShlI ShrI ShlV ShrV` (variable shifts keep the count in `V6` = `rcx`),
`SDivRem` (signed, divisor never 0/-1), double ops on bit patterns kept in virtual registers (`FAdd FSub FMul FDiv I2F`)
and `FBr` (IEEE compare-and-branch, NaN false except `Ne`). Both backends run the same templates.

Templates (all mirror the fast path of `vm_unsafe.rs`; anything else exits at the instruction before any side effect):
ADD SUB MUL (int/int wrapping, float/float, mixed via `cvtsi2sd`), ADDI ADDK SUBK MULK, DIV DIVK (always float),
IDIV MOD IDIVK MODK (integers only: zero divisor exits so the interpreter raises, `-1` special-cased, floor correction
exactly as `number::idiv/imod`; any float operand exits), UNM BNOT NOT, BAND BOR BXOR SHL SHR and the K/I variants
(`shiftl` semantics incl. |y| >= 64, `i64::MIN`; floats and strings exit), EQ LT LE EQK EQI LTI LEI GTI GEI (int/int,
float/float, int/float mixed exactly: an integer outside `±2^53` exits, NaN compares false; nil/false/true same-tag
equality; strings, tables and anything that could call a metamethod exit), TEST TESTSET, GETUPVAL, SETUPVAL (non-collectable
values only; collectable values exit so the barrier stays in the interpreter), FORPREP/FORLOOP for float loops (and
integer loops as in J1a). POW, LEN, string/table ops are still exit stubs.

Conventions introduced here:
- **Followers.** A fast-path arithmetic template skips its MMBIN* (the stub is emitted out of line at the end of the
  function and is only reached by the interpreter's slow path) and a conditional template performs the jump of its JMP
  follower itself (the JMP is never emitted or counted, like `donextjump!`). A template whose follower is missing stays
  an exit stub.
- **Counting (C1).** One `CountSub` per block head pre-pays all templates of the block (`Plain`, `Guard`, `Branch`);
  a template that exits at itself refunds the pre-paid count of itself and the rest of its block (`Cx::rem`,
  `exit_r`), so exits never over-count. RETURN0/1 keep counting themselves after their guards.
- **Type knowledge.** `Cx::ty` records "register holds an int/float" inside a block (reset at heads), removing tag loads,
  tests and tag stores for registers written earlier in the block. Constants (`LowerCfg::consts`) specialise ADDK/EQK/...
  at compile time (an `ADDK` with an int constant is an `add reg, imm`).
- **Compile heuristic.** In `RUSTYNAIL_JIT=on` mode a proto is compiled only if it has a loop whose body is at least 30%
  templated (`Lowered::worth_compiling`); loop-free functions are never compiled (the enter/exit round trip costs more than
  interpreting them until J1c keeps calls and returns in compiled code). `always`/`verify` compile everything.
- **Review fixes (J1a):** the entry thunk now sets up `rbp` before saving the other callee-saved registers (frame-pointer
  chain), `set_mode(Off)` stops entering already compiled protos, `Config::parse` accepts `off/0/false` in any case, and
  `stats`/`set_count_budget`/`set_mode` no longer allocate per-state data when the compiler is not registered.

Tests: `tests/arith_diff.rs` (every template vs the interpreter on 45 special values, emulator and native code, exits must
not touch the arguments), `tests/jit_arith.rs` (programs with `always` vs `off`, small count budgets, hooks, coroutines,
metamethods, error messages), plus `scripts/jit-diff.sh` with `JITDIFF_ON=1`.

## 13. As built: J1d table templates (2026-10-29)

New file `crates/rn-jit/src/templates_table.rs` (safe code; `lower.rs` only dispatches to it, `templates_arith::upval_ptr`
became `pub(crate)`). No new micro-ops: the IR already had base+offset loads through a virtual register, and the node
walk uses `ShlV`/`Mul`/`Xor`/`Sub` (the 32-bit chain offset is sign-extended with `(x ^ 0x8000_0000) - 0x8000_0000`).

Templates (kind `Guard`; every miss is `exit_r(pc)`, i.e. an exit at the instruction before any side effect, with the
count refund of C1): GETI (constant `>= 1`), GETTABLE (register key: integer in the array part, or a short string found by
hash walk; floats, integer-valued floats, strings that are not short, and anything else exit), GETFIELD, GETTABUP
(any upvalue holding a table, so `_ENV` globals too), SELF (method found by raw hash lookup; `R[A+1]` is written before `R[A]`
and only after the lookup, so `A == B` is safe), SETI, SETTABLE (integer key, array part only), SETFIELD, SETTABUP.
LEN and NEWTABLE stay exit stubs (the border logic is not trivial; allocation belongs to the interpreter).

* **Reads.** Array: `u = k - 1`, exits unless `0 <= u < asize` (`asize` is a `u32` at `Table::ASIZE`; 5.5 has no `alimit` hint so
  `ARRAY_ALIMIT_TOUCH` is irrelevant, and the lowering already refuses to compile 5.4). The slot tag is at `array + 4 + u`, the
  payload at `array - 8 (u + 1)`. A slot whose `tag & 0x0F == 0` (nil, empty, abstkey) exits, because `__index` may exist.
  Hash: the constant's hash is read at compile time from the constant table (`StrView::hash`, a short string's stored hash); the
  *pointer* of the constant comes from `kcache` at run time (G1: no collectable address is embedded). The walk starts at
  `node[hash & (2^lsizenode - 1)]` and compares `key_tt == ctb(SHRSTR)` and the key pointer (interned short strings, dead keys
  have another tag); chain end = exit. The dummy node (`lsizenode = 0`, key nil, `next = 0`) needs no special case.
* **Writes** mirror `pset_existing_int_arr` (slot non-empty, or the table has no metatable: no `__newindex` question) and
  `pset_existing_shortstr_slot` (node present with a non-nil value; no insertion, no `invalidate_tm_cache`, nothing about
  `__newindex` since the key exists). Anything else exits, so insertion, rehash, `__newindex` and errors stay in the
  interpreter.
* **GC barrier.** The interpreter does `barrier_back(t, &v)` after the store. That function acts only when the value is
  collectable, the table has `BLACKBIT` and the value has a `WHITEBITS` bit; everything else (`barrier_back_slow`: gray-again
  list, touched1/touched2 ageing in generational mode) happens only then. The template evaluates exactly this condition
  *before* the store (`barrier_exit`: table `marked & BLACKBIT`, value tag `& BIT_ISCOLLECTABLE`, `value->marked & WHITEBITS`;
  constants and register values with known numeric type skip the parts decided at compile time) and **exits when it holds**, so
  no ageing/gray-list logic is duplicated in machine code and the two collectors are covered by construction. Consequence: a
  store of a fresh (white) object into a black/old table costs the exit; stores of numbers/booleans/nil, and of objects into
  white or gray tables, stay inline.
* **Counting.** Same as J1b: pre-paid count, refunded by `exit_r`.

Tests: `tests/table_diff.rs` (interpreter vs emulator vs machine code on real `Table` objects: arrays with holes, a 300-element
array, collisions, deleted keys, dead keys, integer hash keys, metatables with `__index`/`__newindex` and without, 35 key/value
arguments including `0`, negative, `1<<40`, `maxinteger/mininteger`, `2.0`, NaN, `-0.0`, strings, booleans, fresh objects,
under both collectors; exits must leave tables and registers untouched; `barrier_matrix` sets table/value colour bytes by hand and
requires an exit exactly when the table is black and the value white; SELF has its own checker) and `tests/jit_table.rs`
(always-vs-off programs: array/matrix/field loops, method calls, globals, error messages, holes, shape changes between runs,
fresh objects written into old tables with `collectgarbage` steps in both collectors; run it under `RUSTYNAIL_GC_STRESS=1`/`2`).
Note for test authors: tables created at different moments can have different colours, which changes the barrier-exit decision;
the diff test stops the collector while it builds its tables so all four twins match.

Measured (release, `perf stat` cycles:u, cpu 4, min of 7, `RUSTYNAIL_JIT=off` -> `on`; `always` in brackets): fannkuch 4118 ->
1711 Mcyc (0.416), matmul 3678 -> 1485 (0.404), heapsort 4709 -> 2676 (0.568), sieve 4853 -> 3501 (0.721); nbody 0.995,
spectral_norm 1.002, table_array 0.999, binary_trees 0.993 (not compiled in `on`: calls such as `math.sqrt`, string/table library
calls and NEWTABLE exit; they gain in J1c). `always` is slower than `on` on call-heavy programs (spectral_norm 1.425, binary_trees
1.270) because every call enters and immediately exits.

**J1c after merging J1b/J1d (2026-10-29).** (a) The thunk returns `rax:rdx = (exit code, r13)`; `rt::enter` compares the
returned `CallInfo` with the entered one to choose `Continue` vs `FrameChanged` (not `thread.ci`, which
`coroutine.close` of the running thread resets behind the frame's back). (b) A native's `Err(Status::Ok)` is a real value
(`JitSlot.err` is an `Option`). (c) After-CALL resume entries are recorded only when the next instruction is a template
(jumping into a stub would only bounce through the thunk). (d) `worth_compiling` also admits loop-free functions with a
function entry that are >= 60% templated; `interp_entry` (loops, calls inside, or >= 24 instructions) decides whether the
*interpreter* flags fresh frames of such a function for entry in `on` mode (a tiny leaf entered from the interpreter loses to
the interpreter by the thunk round trip, e.g. `Account:deposit` in oop_methods; compiled callers still flat-call it). `always`
and `verify` enter everything. (e) `jit_hot_call` counts with a plain load/store instead of `lock xadd`.

## 17. As built: J2b generic `for`, varargs, closing (2026-10-29)

New file `crates/rn-jit/src/templates_loops.rs` (safe code); helpers `jit_tforcall`, `jit_varargprep`, `jit_vararg`, `jit_close`,
`jit_tbc`, `jit_return_close` in `rn-core/src/jit_helpers.rs` (same shape as `jit_call`: `extern "C"`, `savedpc` = the instruction
after, status in `eax`: `H_DONE` or `H_ERR`; each is the interpreter's own code for the opcode, same functions, same order of effects).
Helper addresses are taken directly from `rn_core::jit_helpers` (no `CallEnv` field). Kinds: `TFORPREP` Guard, `TFORLOOP` Branch,
`TFORCALL/VARARGPREP/VARARG/CLOSE/TBC` Guarded (hook test where the interpreter has hook work, `CountSub{1}`, `savedpc`, `CallH`,
error exit, `ReloadFrame`, late `hookmask` test -> `exit_trap(pc + 1)`, continue). `RETURN` (generic, `calls.rs`) now also handles `k`
and `C`.

* **TFORPREP.** Inline swap of `R[A+2]`/`R[A+3]` when the closing value is `nil`/`false` (nothing to mark), then a jump to the `TFORCALL`
  at the end of the loop (a block head). A non-nil closing value exits at the instruction (the interpreter registers the tbc variable).
* **TFORCALL.** `jit_tforcall` = `do_tforcall`: copy function/state/control to `R[A+3..A+5]`, `top = A+6`, nested `State::call(.., C)`
  (a Lua iterator is entered through the normal frame-entry hook; yields in it come back as an `Err(Yield)` exit like natives).
  Fast paths without a call frame: the base library's `next` (identified by function address, registered at `rn_jit::register` through
  `rn_std::lib_base::jit_builtins` -> `jit_hooks::set_builtins`; `rn-std/jit` is the only change in rn-std) over a table calls
  `Table::next`, i.e. the builtin's traversal, order and modification-during-traversal rules; an invalid key makes the helper fall back to
  the real call, which raises the exact error from the native frame. `ipairs`'s iterator over a table: **inline machine code** for the
  array part (iterator address, table tag, integer control `0 <= c < asize`, non-nil slot -> `R[A+3] = c+1`, `R[A+4] = v`, remaining
  variables nil), the helper for everything else (hash part, end of the array, metatables with `__index`, other types). Active hooks
  exit at the TFORCALL.
* **TFORLOOP.** Inline nil test of `R[A+3]`, back-edge poll, jump.
* **VARARGPREP / VARARG.** The helpers are `adjust_varargs` (incl. the 5.5 named-vararg table, `PF_VATAB`) and `get_varargs`
  (fixed counts and `MULTRET`, which leaves `top` = end of the copies for the following `CALL B=0`/`RETURN B=0`/`SETLIST`); `ci.func`
  and `ci.top` move, `ReloadFrame` re-derives `rbx` from `ci.func`. Because VARARGPREP is now a template, vararg functions have a
  function entry: compiled callers flat-call them (the shared flat-call code needed no change except the fix below) and the vararg main
  chunk is entered at pc 0 instead of by OSR.
* **RETURN.** `C != 0` (vararg frame): the results are moved to `func - (nextraargs + C)` (the original function slot); `k != 0`: after the
  interpreter's guards, `jit_return_close` (`ci.u2 = n`, `top >= ci.top`, `State::close(base, CLOSEKTOP)` which runs `__close`
  metamethods as nested calls, `top = ra + n`), then the ordinary return. The guards (`trap`, `CIST_MMRET`, no caller) are tested before
  the helper so the interpreter can still perform the whole instruction; a yield inside `__close` resumes through `finish_op`, which
  repeats the RETURN in the interpreter (`savedpc` = instruction after, as in the interpreter).
* **CLOSE / TBC.** `State::close(R[A], Ok, true)` and `new_tbc_upval`. A `__close` metamethod that calls Lua runs nested exactly as
  in the interpreter (no special exit is needed); errors and yields propagate as error exits.
* **Bug fixed on the way (J1c):** the flat call with `B == 0` compared `numparams` with `nargs + 1`, so a callee that received exactly
  one argument too few was entered without the nil fill (stale register garbage; with a vararg callee `adjust_varargs` underflowed).
  Found by the official suite (`bitwise.lua`, `vararg.lua`) once vararg functions became flat-call targets.
* `lower()` accepts a generic `RETURN` as the last instruction (vararg functions end in `RETURN`, not `RETURN0/1`).

Not done / gaps: `next` over the hash part and `pairs` are helper calls (one `extern "C"` round trip per step, no frame); `VARARG` is
always a helper (no inline copy of 1-3 extra arguments); an iterator that is a Lua function runs through the nested `State::call`
(native recursion of one `run` per iteration, like the interpreter), not a flat call; `GETVARG`/`ERRNNIL` (5.5 named varargs) remain
exit stubs; 5.4 is covered in section 18.

Tests: `tests/jit_loops.rs` (15 always-vs-off twin programs with count budgets 3/17/200 and `stackmove` on for the whole binary;
mutation-checked: dropping the `ReloadFrame` after the helpers crashes it; `hot_loops_stay_compiled` bounds the exits), updated
`jit_skeleton/jit_arith/jit_lifetime` expectations (the vararg main chunk has a function entry now).

## 15. As built: J2a helper templates (allocation, concat, closures, table slow paths) (2026-10-29)

New file `crates/rn-jit/src/templates_alloc.rs`; the helpers are in `rn-core/src/jit_helpers.rs` (`jit_newtable`, `jit_setlist`,
`jit_len`, `jit_concat`, `jit_closure`, `jit_pow`, `jit_get`, `jit_set`), `vm_unsafe.rs` only widened `tonum`, `finish_set_op` and
`push_closure` to `pub(crate)`. `lower.rs` got the dispatch lines, a `cold: Vec<Cold>` list (out-of-line code, emitted before the lazily
created exit sequences) and the follower rule; `calls.rs` made `l_exit`/`calls_on` `pub(crate)`.

**Protocol (as J1c).** Every helper runs the interpreter's own code for the instruction (same `Table` set routines, `new_table`,
`concat`, `push_closure`, `finish_set_op`, `check_gc` with the same `top`). Compiled code stores `ci.savedpc` = what the interpreter's
`savepc!()` stores (the address after the instruction *and its EXTRAARG*), the helper sets `top` like `Protect`/`half_protect`/`check_gc!`
(`top = ra + 1` for NEWTABLE and CLOSURE, `ci.top` for SETLIST/SETTABLE.., the end of the operands for CONCAT). A new status `H_EXIT`
means "nothing observable happened, the interpreter must do this instruction": compiled code leaves at the instruction with the usual
count refund (`exit_r`). Metamethod *functions*, yields, errors with variable info and non-table indexing therefore stay in the
interpreter (E3). After a helper that can allocate (all but `jit_get`/`jit_len`/`jit_pow`) the template does `ReloadFrame`, polls
`thread.hookmask` (a finalizer can install a hook: leave at the next instruction with `trap = 1`, refunding the pre-paid count of the rest
of the block) and drops the block-local register type knowledge (`Cx::ty`; a finalizer can `debug.setlocal` the frame).

**Templates.** NEWTABLE (+EXTRAARG, a dead follower like the MMBIN/JMP followers: not emitted, not counted), SETLIST (EXTRAARG when `k`),
LEN (tables without `__len` and strings; result type `Int`), CONCAT (only when every operand is a string or number: otherwise `H_EXIT`
before any effect, so `__concat`, error messages with variable names and yields are the interpreter's), CLOSURE (`push_closure` then
`check_gc`: open-upvalue list handling is the interpreter's code), POW/POWK (both operands numbers: `number::pow_lua` exactly; result
type `Flt`; followed by its MMBIN like the other arithmetic ops). The nine table accessors keep their inline probes; the label that used to be
"exit at the instruction" is now an out-of-line stub calling `jit_get` (raw get, then the `__index` chain as long as every `__index` is
a table, string metatable included: nil result for tables without `__index`, SELF's two stores) or `jit_set` (the interpreter's raw
store, then, when the table has no `__newindex`, `finish_set_op`: insertion, rehash, `invalidate_tm_cache`, `barrier_back`; so the "white
value into black table" barrier case no longer exits). `H_EXIT` for `__index`/`__newindex` functions, non-tables, long chains. After
`jit_set` the block's register types are dropped.

**Counting.** Pre-paid like every `Guard` template; `H_EXIT`/hook exits refund exactly the instructions the interpreter will run again.
Error exits do not refund (the frame is unwinding).

**Not done** (still exit stubs): UNM/BNOT/NOT slow paths and MMBIN* (metamethod calls), SETUPVAL of collectable values, EQ/LT on
strings and tables, float MOD/IDIV, CLOSE (J2b).

Tests: `tests/jit_alloc.rs` (corpus `tests/alloc_progs/mod.rs`, always-vs-off twin incl. count budgets 3/17/200, plus a no-exit loop
check), `tests/jit_alloc_stress.rs` (stackmove; mutation-checked: removing the `ReloadFrame` fails it). Run both under
`RUSTYNAIL_GC_STRESS=1` and `=2`.

## 16. As built: entry back-off and exit profile (performance-regression pass, 2026-10-29)

Root causes of the J2a regressions (json 1.125x, metamethods 1.071x slower than JIT off), found with the new
`RUSTYNAIL_JIT_VERBOSE=1` exit profile (printed to stderr when the state closes: per prototype its shape, entries,
bytecodes per entry and back-off count; the top-25 `(proto, pc, opcode)` exit points with exit counts and rdtsc
cycles per stay in compiled code):

- **json**: 2.4M entries from the interpreter; 630K+360K+630K of them left again within ~30 cycles at `EQK` against
  a *string* constant (`c == "{"`, `d == "}"`), which had no template. Each enter/exit round trip costs ~150 cycles
  (counter hook -> `hot_call` -> `enter` -> thunk -> exit -> back in the interpreter), the interpreter would have
  run the 3-5 bytecodes in less.
- **metamethods**: 4M entries of the `__add` metamethod (17 bytecodes), which always ends in a `TAILCALL` that exits
  because the frame is a metamethod frame (`CIST_MMRET`: the interpreter finishes it); ~91 cycles in compiled code
  against the round trip.

Fixes:

1. **`EQK` with a short-string constant** is a template: the register is equal iff its tag is the short-string tag and
   its payload is the constant's (interned) object pointer; every other value is unequal (no exit). Long-string
   constants and everything else still exit. `arith_diff` now gives string registers real interned pointers.
2. **Entry back-off** (`rt.rs`, `Mon`; plan section 14 item 2). Every compiled stay entered *from the interpreter*
   (function entry or OSR) reports the bytecodes it executed (the consumed block-count budget). Every 64 stays the
   average is checked; below 48 bytecodes per stay the prototype is *blocked* for interpreter entries: `hot_call`
   answers `Retry` (the counter fires again after 200 calls, each firing consumes 200 of the skip budget) and `hot_loop`
   refuses ticks (one opportunity each since the review fix; they used to weigh 1000, which made the first loop block one tick long), for 64 opportunities, doubling at every failed retrial up to 2^20; a profitable window
   resets the back-off. Compiled callers keep flat-calling the prototype (`hdr.entry0` is untouched), so recursion
   (`fib`) and compiled call chains are unaffected. Active in `RUSTYNAIL_JIT=on` only (`always`/`verify` stay
   deterministic). No bytecode patching, per-prototype state only.
3. Not changed (inherent): feature-on interpreter cost of the hot counters (+1.2% instructions, docs/JIT_HOOK_COST.md)
   shows as a 0-2% slowdown on programs that never get compiled code (load_big, varargs, table_array, gc_churn).

## 18. As built: J2b-5.4, the template set under `--cfg rustynail_lua54` (2026-10-29)

`lower()` now accepts 5.4 prototypes. The object layouts (`Value`, `Table`, `Node`, `CallInfo`, `Proto`, `UpVal`) are identical in both
builds (`jit_layout.rs`), so every template that only touches those is shared unchanged: arithmetic, comparisons, TEST/TESTSET, the
upvalue accessors, string-key table probes (GETFIELD/GETTABUP/SETFIELD/SETTABUP), CALL/TAILCALL/RETURN*, NEWTABLE/SETLIST/LEN/CONCAT/
CLOSURE/POW (helpers; the helpers already had the 5.4 operand decoding: `B`/`C`/`MAXARG_C` instead of `vB`/`vC`), CLOSE/TBC, VARARGPREP/
VARARG (5.4 has no named varargs, so `jit_vararg` passes `vatab = -1`). The differences that needed code:

* **Numeric `for`** (`templates_arith.rs`, `t_forprep54`/`t_forloop54`). 5.4 layout: `R[A]` internal index, `R[A+1]` iteration count
  (integer loop; the limit in a float loop), `R[A+2]` step, `R[A+3]` control variable (a copy that the body may overwrite, so FORLOOP
  stores tag and payload). FORPREP mirrors `forprep_int54`/`State::for_prep`: integer loop only when init and step are integers and the
  limit is an integer (otherwise the interpreter's `forlimit`), `R[A+3] = init` is stored *before* the skip test (like the interpreter),
  step zero and non-numbers exit before any effect; the float loop sets `R[A]`, `R[A+3]` = init, `R[A+1]` = limit, `R[A+2]` = step.
  The skip and back-edge targets (`pc + Bx + 2`, `pc + 1 - Bx`) are the same as in 5.5.
* **Generic `for`** (`templates_loops.rs`). 5.4: `R[A]` iterator, `R[A+1]` state, `R[A+2]` control, `R[A+3]` closing value, loop variables
  from `R[A+4]`; there is no swap. TFORPREP with a nil/false closing value is just the jump to the TFORCALL (anything else exits: the
  interpreter registers the tbc variable); TFORLOOP tests `R[A+4]` and copies it to `R[A+2]` before the poll/back-edge. `jit_tforcall`
  has the 5.4 call layout (`R[A+4..A+6]` = iterator, state, control; call at `A+4`), and the frameless `next`/`ipairs` fast paths of the
  helper and the inline `ipairs` step are parametrised by (control slot, result slot) = (`A+3`, `A+3`) in 5.5 and (`A+2`, `A+4`) in 5.4.
* **SELF**: the key is `RKC` in 5.4 (a register when `k` is clear). Only constant short-string keys are compiled (`k` set); `jit_get`
  reads the key from the register for the exit/slow path of a `k == 0` SELF (never reached from a template, kept correct anyway).
* **`alimit_touch`** (`templates_table.rs`, `Cx::alimit_touch`; `jit_layout::table::{BITRAS, LENHINT}`). `luaH_getint`/`psetint`/
  `set_int` of 5.4 update the array limit when an access hits array index `u < asize` with `u >= lim` (`BITRAS` set, `lim` = the `u32` at
  `array + 0`): the new limit is `u + 1` if `u & !(lim - 1) < lim`. That changes later `#t` on arrays with holes, so **every inline
  integer-key array access (GETI, GETTABLE with an integer key, SETI, SETTABLE, the inline `ipairs` step) performs exactly this update
  after its bounds check (`u < asize`), before the nil/barrier tests**: load `flags`, test `BITRAS` (usually clear: one load, one test,
  one branch), else compare with the hint and store `u + 1` when the interpreter would. A later exit at the same instruction (nil slot,
  metatable, barrier) is safe: the interpreter redoes the access and its own touch is then a no-op (monotone, idempotent). Accesses with
  `u >= asize` never touch (hash part: the helper). Hash-part nodes: the templates only read nodes and overwrite non-nil values, so the
  5.4 `lastfree` rules (every non-dummy hash part has one) do not matter; insertion is the helper.
* **GC barrier**: 5.4 `luaC_barrierback` has the same condition as 5.5 (table black, value collectable and white; the generational
  `touched` ageing is inside `barrier_back_slow`), so the inline test (exit when it would fire) is unchanged; SETUPVAL stores
  non-collectable values only. `gen54.rs` is only reached through the exit/helper paths.

Opcodes that are still exit stubs in 5.4: `LOADKX`/`EXTRAARG`, `MMBIN*` (out-of-line followers), `EQK` against a constant that is not a
number, nil/boolean or short string, arithmetic-with-constant instructions whose constant is not a number, SELF/GETFIELD/GETTABUP/SETFIELD
with a long-string constant, and everything the 5.5 set also leaves (`GETVARG`/`ERRNNIL` do not exist in 5.4).

Tests under the 5.4 cfg: every `rn-jit` test file except `named_vararg_tables` (5.5 named varargs) runs; `table_diff` gained tables
with a limit hint below the real array size (15-18, `#t` observed through `dump_len`) and array-limit bodies; `jit_table::
array_limit_hint_sequences` (random access sequences, `#t` after each step, mutation-checked: removing the inline touch fails it and
`table_diff::sets_*`); `emu_diff` uses the 5.4 control-variable register.

## J4-T1 as built: IR and backend extensions for tier 1.5 (2026-10-29)

Spec: `docs/JIT_TIER15.md` 4.2, task T1. Tier-1 templates and tier-1 machine code are unchanged
(`scripts/jit-dump-identity.sh <base>` compares per-proto disassembly with masked `movabs` immediates on the 25 bench
programs under `RUSTYNAIL_JIT=always`, 5.5 and 5.4: IDENTICAL, 69 protos; `scripts/jit-text-identity.sh` also identical).

- `ir::NV = 9`: `V7 = rax`, `V8 = rdx`; `X = u8`, `NX = 16` (`xmm0..15`). The clobber list is in the `ir.rs` module docs.
  `SDivRem`/`UDiv` assert (emitter) that all operands are `V0..V6` and the emulator poisons `V7`/`V8`
  (`emu::POISON`) afterwards; `ReloadFrame` clobbers `V7`/`V8`, `CallH` everything.
- New ops: `LdF StF MovF FBin FBinM FSqrt FNeg FAbs GtoF FtoG I2Fx F2IExact FBrX BrM BrMi LdI StI LdFi StFi`;
  `FOp = Add Sub Mul Div Min Max`; indexed operand `Mi { base, idx, scale, off }` (T1b). `FAbs` and the `LdI/StI/..`
  family are additions beyond the table in the design (FAbs for the `math.abs` intrinsic).
- Semantics fixed here: `Min/Max` are exactly `minsd/maxsd` (second operand returned, unquieted, on NaN or equality).
  Other `FBin` ops follow x86: NaN first operand (quieted), else NaN second operand, invalid operation gives
  `0xFFF8_0000_0000_0000`; the emulator implements this explicitly so results do not depend on the host or Miri.
  `BrM/BrMi` compare signed at the access width (also `B1`, all six `Cc` tested), the immediate truncated to it.
  `F2IExact` = `cvttsd2si` + `cvtsi2sd` + `ucomisd` (`jp/jne` to the failure label); on failure `dst` holds the
  `cvttsd2si` result. `FNeg/FAbs/F2IExact` use `X0` as scratch, `X1` if the operand is `X0`; `FNeg/FAbs` also `V7`.
  `roundsd` has an encoder form but no IR op (needs SSE4.1 detection; not done).
- Encoder: new `movaps_rr`, `xorps_rr` only (all other forms already existed and are oracle-tested); both added to
  the iced exact-byte sweep.
- Emulator: `x: [u64; NX]`; `emu::run_until(prog, start, stop, env, steps) -> Until` (`Until::Stop(Stop) |
  Until::Label(L)`); the `Bind` of `start` never stops. `Stop` is unchanged (so existing matches still compile).
- Tests: `tests/emu_ops_diff.rs`: per op, 40-120 random programs x 300 random states (>= 10 000 per op; random XMM
  patterns incl. NaN payloads, inf, denormals, -0, 2^63 neighbours; random GPRs; random register aliasing) run natively
  and in the emulator, bit-exact on all registers and memory, plus plain-Rust models of every op; known-answer and
  `run_until` tests are Miri-clean (`scripts/miri.sh jit` now runs `emu_ops_diff`).

## J4 (tier 1.5 regions): summary and pointers (2026-10-30)

Status: **DONE** (T1-T8, T12, T13; T9 and T10 were skipped by user decision on 2026-10-30). The authoritative design and as-built record is
`docs/JIT_TIER15.md` (sections 13-23); this section only fixes the entry points into it.

- **Architecture.** A hot loop nest of a tier-1 prototype is *promoted*: the whole prototype is recompiled into one program holding the unchanged
  tier-1 code (the deopt target) plus a typed region for the nest (analysis `t15::analysis`, linear-scan allocation `t15::alloc`, codegen
  `t15::codegen*`, exit/resume stubs `t15::stubs`). Ints, booleans and table pointers live in GPRs, floats in XMMs; the VM stack slot stays the
  spill home, every exit writes back inline and jumps to the tier-1 label of the same pc, so tier-1 frames remain identical to the interpreter's
  (the E1/E3 protocols of sections 2 and 5 are unchanged). Inline table accesses (T5), `math` intrinsics and `GETUPVAL` (T6), and exit feedback with
  respecialisation and give-up (T7) are part of it. Superseded code versions are retired, not freed, until the prototype dies (section 14 of
  JIT_TIER15.md, `docs/UNSAFE.md` J4-RETIRE).
- **Flags.** `RUSTYNAIL_JIT_T15` = `off` (default) | `on` (promote after `RUSTYNAIL_JIT_T15_HOT` back-edges, default 4096) | `always` (first back-edge, for
  tests). `RUSTYNAIL_JIT_OPT` is a comma list: switches off passes (`-ra -xmm -hoist -fcache -intr -cntreg -calls -nest -peel -tables -respec`), `+force`,
  `+exithot=N`, and the verification options `+flushall`, `+verify`, `+randra=SEED`, `+shadow` (per-block emulator cross-check of pure blocks) and
  `+exitstorm=N` (pseudo-guards that force exits at every instruction boundary, stub and edge). The verification options are never default and change no
  byte of code when absent. `RUSTYNAIL_JIT_VERBOSE=1` adds region statistics. Tier 1.5 needs `--features jit`; the JIT-off `.text` and the T15-off tier-1
  code are byte-identical to before (`scripts/jit-text-identity.sh`, `scripts/jit-dump-identity.sh`).
- **Not done.** T9 (function regions, 5.4 regions) and T10 (`setlocal_epoch`, delayed stores, multiply-high) are skipped: call inlining and 5.4 coverage
  move to J5. Regions are generated for Lua 5.5 prototypes only.
- **Verification.** Mutation matrix (JIT_TIER15.md 22.4), `t15_stress`, jit-diff `t15*` modes (`docs/JIT_TESTING.md`), ASan/Miri/fuzz wiring (22.3, 22.5);
  unsafe inventory in `docs/UNSAFE.md` (J4 section).
- **Results (JIT_TIER15.md section 23, `docs/bench-t13-j4.txt`).** Geomean of cycles vs reference Lua 5.5: all 25 programs 0.521 (tier 1: 0.632), numeric
  0.275 (tier 1: 0.531). Targets (<= 0.60 / <= 0.30) met. Remaining weakness: call, closure and method-heavy programs, which are the J5 targets.
- **Next.** J5 (trace compiler, `docs/JIT_TRACE.md`) is next, to be started in a new session.
