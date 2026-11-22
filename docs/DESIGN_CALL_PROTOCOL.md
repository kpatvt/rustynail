# Design: call protocol and recursion (traces, tier 1, interpreter) (2026-11-21, base a536f04)

Status: DESIGN for the owner's decision, with measured evidence and prototypes. No production behaviour changes in this phase. The prototypes are a
patch file (`docs/proto-call-protocol.patch`, section 8), not merged code.

**Built (2026-11-21): stages 1 (E1) and 2 (E2)**, re-implemented from sections 4.1 / 4.2 (not from the patch), on by default (`recunroll=1`
adaptive, `unrollk=3`, `unrollcode=400`, `idfold=1`); design as built, policy criteria, soundness, tests and gates in docs/JIT_TRACE.md section 34.
Measured (5.5, instructions / cycles against the same binary with both knobs off; the prototype's figures in brackets): fib 32 E1 -35.8 % /
-36.5 % [-35.8 / -35.9], E1 + E2 **-44.5 % / -45.7 %** [-41.6 / -42.8], 62.4 Mcyc = **1.45x LuaJIT** (was 2.65x; the prototype reached 1.52x);
binary_trees 15 -22.8 % / -21.6 % [-21.1 / -21.0]; sumrec -37.5 % / -52.8 % [-33.5 / -50.4]; permute / towers 0 (fixed k gave +37 % / +36 %
cycles), list +0.2 % instructions (fixed k +65 %), cd 20 +0.2 % (5.4: -2.2 %); every other program of a 44-program sanity set within 0.07 %
instructions. 5.4 matches (fib -44.6 %, binary_trees -20.9 %).

**Built (2026-11-22): stages 3 (E4) and 4 (E5)**, on by default (`lazyloop=1`, `midfold=1`); K5 left out (nothing measurable left after E5);
design as built, soundness, tests, gates and findings in docs/JIT_TRACE.md section 35. Measured (5.5, one binary, against `lazyloop=0,midfold=0`):
permute 300 **-14.2 % instructions / -15.9 % cycles** (E4 alone -4.8 % / -6.0 %; E5 needs E4's region; 5.4 -13.7 % / -14.0 %); towers, fib,
binary_trees, list, queens, richards, deltablue, havlak, cd, raytrace and the 44-program sanity set within 0.09 % instructions (E5 is limited to
method families with loops by default: on towers it cost +2.6 % cycles for nothing). Two E5 mutants survive and a pre-existing lazy-chain defect
was found (JIT_TRACE 35.8).

**Built (2026-11-22): stage 5 (E6)**, on by default (`dlink=3`: bit 0 direct exit-to-side links, bit 1 the budget register across native
transfers); the lazy-chain defect of 35.8 fixed first and the two E5 survivors caught (JIT_TRACE 36.1, 36.2); design as built, soundness (patching
lifetime, flush ordering, TR5), tests and gates in JIT_TRACE section 36. Measured (5.5, one binary, `dlink=0` against `dlink=3`): fib **-8.3 %
instructions / -5.8 % cycles** (estimate -5.5 % instructions), permute -3.7 % / -2.3 %, towers -2.8 % / -0.4 %, binary_trees -2.2 % / -0.4 %
(estimate 2-4 %), sumrec -1.6 % / -2.5 %; queens **0** (estimate 2-7 %: queens is not a lazy-aware family, E6 is limited to them); every other
program of the key and 44-program sets within 0.12 % instructions. 5.4 matches (fib -8.3 % / -6.0 %). Stages 6-8 are not built.

Evidence tags as in the other JIT documents: **[M]** measured here (command and binary stated), **[D]** disassembly count (exact for the path
counted, the path frequencies from the call tree), **[E]** estimate with the derivation stated, **[J]** judgment.

## 0. Summary

**Where the gap is.** `fib` costs 99.8 instructions / 16.2 cycles per call against LuaJIT's 32.0 / 6.1 [M]. Splitting our 99 instructions by what
they do (section 2.1) gives about **28 instructions of per-frame work** (argument and function slot stores, the lazy `CallInfo` push, the pop, the
result store, the base-case guard, the body arithmetic), close to LuaJIT's ~25 for the same work, and about **71 instructions of per-fragment
overhead**: every call and every return starts a new trace fragment that repeats the poll (6), the budget charge / store / reload (8.5), type
guards on slots it reloads from memory (9), the 15-instruction callee identity chain (14.5), room checks (7.5), the return-site guards (9.5) and
the transfer through cells, stubs and indirect jumps (14). LuaJIT pays ~4 for its equivalents because one trace iteration covers three calls
(up-recursion unrolled three levels), its return guard is one compare, and its links are patched direct jumps. The gap is therefore not the frame
model (`CallInfo` versus slot frames) but the **number of fragment boundaries per call and what each boundary costs**.

**The design (section 4).** Keep `CallInfo` as the one frame format of all three layers (interpreter, tier 1, traces; also the debug API, the C API,
errors and coroutines). Traces keep frames virtual (inlined calls, as today) or lazy (native pushes write two words, completed at every exit or
leave, as today since C70). Cut fragment boundaries and make the remaining ones cheap:

| element | what | measured on the prototype (fib 32; instructions / cycles, min of 5) |
|---|---|---|
| **E1 up-recursion unrolling (H9)** | a function-entry trace inlines its self-call `k - 1` times and pushes `k` lazy frames at the end; leaves inside the inlined frames get frame side traces (S1 virtual frames) that return through the inlined callers; `k` adaptive per head (falls back to today's `k = 1`) | **k = 3: -35.8 % / -35.9 %**; k = 2: -20.6 % / -20.2 %; binary_trees -19.5 % / -19.1 %; linear recursion -34.4 % / -50.7 % |
| **E2 region-scoped callee identity** | a self-recursive upvalue callee is the running closure: the chain runs only at the region's checked entries (outer entries, returns of the region root), not at every push | -13.0 % / -9.6 %; **with E1: -41.6 % / -42.8 %** (65.2 Mcyc against LuaJIT's 42.9: 1.52x, from 2.65x) |
| E3 direct native return | the push stores the continuation in the frame; a lazy frame returns with two loads and a jump instead of guards, a site compare chain and an indirect cell jump | prototype (conservative form): -4.5 % / 0 %; after E1 + E2: 0 % cycles |
| E4 lazy frames for recursion families with loops | `permute`-like protos (a loop in the recursive function) use the eager push (18 instructions, 10 more than lazy) and the eager return guards | not prototyped; permute est. -5..6 % instructions alone, and E1 needs it there |
| E5 region-scoped method identity | method recursion (`self:permute`, `self:moveDisks`): the 34-instruction lookup chain per fragment runs at checked entries only | not prototyped; permute est. -13 % |
| E6 direct exit-to-side links | a guard whose exit has a side trace jumps to it with the budget refund folded and the values in registers (today: a 12-instruction stub with a counter read-modify-write, a budget store and reload, a cell jump) | not prototyped; fib est. -5.5 % instructions; every program with many linked exits |
| E7 poll placement | no poll in fragments whose every way out polls or leaves | -3.0 % / -2.8 %; after E1 + E2: 0 % cycles |
| E8 budget | unchanged (TR5 instruction-count identity is the test oracle); the removal ceiling is -5.0 % instructions and no cycle gain | - |
| E9 tier 1 | the same continuation word for tier-1 returns, a call-site inline cache, native returns of function-entry roots into tier 1 | not prototyped; P-M2 measured that -4 % tier-1 call instructions bought 0 % cycles |

**Recommendation (section 9): build E1 first, with its adaptive policy as part of the stage, then E2.** E1 is the largest measured cycle gain on the
target shape (-36 % fib, -19 % binary_trees, -51 % linear recursion), its soundness rests on machinery that already exists and is tested (inlined
frames, frame side traces with virtual frames, the lazy push), and the prototype passed 852 differential runs of the 213 VM test programs (trace
modes `on` / `always`, without and with GC stress 2) with no difference. Its risk is formation: with a fixed `k = 3` the prototype makes `permute` +37 %, `towers` +4 %
instructions (+36 % cycles) and `list` +65 % worse, so the stage must ship the fallback to `k = 1`, which is today's code path. E2 second: -11 %
cycles more on fib, sound only in the region form of section 4.2 (the naive prototype is caught by `tests/vm/k6_02_path_switch.lua`, which is the
proof that the existing twins cover it). E3 and E7 are instruction-only wins once E1 + E2 are in (measured: 0 % cycles); they are listed with the
measurement and not recommended now.

**What this does not fix.** `richards`, `deltablue`, `havlak`, `raytrace`, `queens`: every prototype bit leaves their instruction counts within 1.2 %
(`programs/queens` with E2 -1.2 %, `havlak` -0.6 %, the others within 0.1 %; `cd 50`: -2.5 %) [M]; their call costs are tier-1 calls, `FnShort` / `LeaveFrame` coverage and root `TAILCALL`s (docs/STUDY_COVERAGE.md), not the
trace recursion protocol. E9 (tier 1) is the only element that reaches them and it is unmeasured.

## 1. Method

Binaries: plain release `rn-cli` of a536f04 (`b0`) and the same tree with the prototype patch (`b1`, every prototype off unless `RN_PROTO` is set:
with `RN_PROTO` unset `b1` executes the same instructions as `b0`, 703.6 M on fib 32), copied to equal-length paths. `RUSTYNAIL_SEED=0`,
`taskset -c 4`, `perf stat -e instructions:u,cycles:u`, every run under `systemd-run --user --scope -p MemoryMax=4G -p MemorySwapMax=0 timeout`,
stdin `/dev/null`. Instructions are exact (first run); cycles are the minimum of 5 runs (fib) or 3 (the others) on an idle machine. The same
instruction count gave 113.9 and 122.1 Mcyc in `b1` and `b0` (layout), so cycle deltas are compared **within `b1`** only, and deltas under ~3 % are
noise. LuaJIT 2.1 (`/usr/bin/luajit`), `-jdump=+rsx` for its traces. Our traces: `RUSTYNAIL_JIT_DUMP` (IR + code bytes, `objdump -b binary`),
`RUSTYNAIL_JIT_PERFMAP=2` with `perf record -e instructions:u` for per-fragment shares, `RUSTYNAIL_JIT_VERBOSE=1` for heads, exits and side traces.
Call counts: `debug.sethook(.., "c")` with the JIT off. Workloads: fib 32 (7.05 M calls), permute 300 (2.60 M), towers 100 (~3.3 M), binary_trees 15,
`sumrec` (linear recursion `n + sum(n - 1)` from 100, 100000 times: 10.1 M calls; the source is in the patch file's header), queens 100,
richards 50, list 300, the micro set of `bench/micro`.

## 2. Where the instructions go

### 2.1 fib: per call, ours against LuaJIT

Our fib runs on five fragments [D] (`RUSTYNAIL_JIT_DUMP`, verified against `perf stat`): **E** the function-entry trace (internal node: poll, budget,
`n < 2` guard, identity chain, `n - 1`, slot stores, lazy push, room checks, jump to its own loop top: 45 instructions); the **leaf exit stub** of E
(12: exit counter compare and add, budget refund and store, exit cell load, compare, indirect jump); **S** the leaf side trace (35.5: budget reload,
native-return guards, budget, `n` type guard and the `n < 2` re-check, result store, native pop, site select, cell jump); **C7** the continuation
after the first self-call (46: room check of its inner entry, poll, budget, `n` reload, identity chain, `n - 2`, slot stores, lazy push, cell jump)
plus E's inner entry (8); **C11** the continuation after the second self-call (41.5: poll, return guards, budget, two typed reloads, add, result
store, native pop, site select, cell jump). Internal node 140.5, leaf 57.5, fib has as many leaves as internal nodes: **99 per call; measured
99.8** [M]. Per-fragment instruction-sample shares (`perf record`) agree: E 33 %, C7 23 %, C11 19 %, leaf stub + S 22 %.

LuaJIT [D] (fib 27 dump; 32.0 instructions per call measured on fib 32): trace 3 is the up-recursion loop (three levels per iteration, 45
instructions = 15 per call: one type guard, three `n < 2` guards, one function identity guard (`rorx/shr/cmp/jnz`), one stack check, three frames
of three stores each (argument, frame link, function), one base bump and `L->base` store, no poll, no budget); trace 4 and the side traces of
trace 3 are the down-recursion (`RETF` = `lea/cmp/jnz` on the frame link, `add rdx; mov [L->base], rdx`, the caller's code, the next call, a
patched jump back into trace 3).

| category (per call, average over the tree) | ours | LuaJIT | note |
|---|---:|---:|---|
| poll (hook / interrupt word) | 6.0 | 0 | LuaJIT does not poll in traces (hooks do not interrupt its compiled code); we must (TR22, section 5) |
| budget (charge, refund, store, reload) | 8.5 | 0 | TR5 instruction-count identity; no LuaJIT counterpart |
| first-iteration flag stores | 1.5 | 0 | |
| type guards + reloads at fragment starts | 9.0 | ~3 | values cross every fragment boundary through memory |
| base-case guard (incl. the side trace's re-check) | 3.0 | ~2.5 | |
| body arithmetic | 1.5 | ~1.5 | |
| **callee identity** | **14.5** | ~2 | 15-instruction chain (`SLOAD fn; UREF; UVREL; UGE; ULOAD; EQ`) at every push; LuaJIT: 4 per trace iteration |
| callee slot stores (function, arguments, tags) | 4.0 | ~3 | |
| frame push (`ci.next`, `ci.func`, caller `savedpc`, `r13`, `rbx`) | 8.0 | ~1 | LuaJIT: one base bump per three frames |
| room checks (stack, spare `CallInfo`) | 7.5 | ~1.7 | one stack check per three frames in LuaJIT |
| **return guards** (`previous`, lazy-root test, `savedpc` site compare chain) | **9.5** | ~3 | `RETF`: one compare |
| return pop + continuation select (`previous` / `savedpc` again, root test, site chain, `AddFrame`) | 10.0 | ~2 | |
| result store | 2.0 | ~1 | |
| **fragment transfer** (cell loads + null tests + indirect jumps, inner entries, the leaf's exit stub) | **14.0** | ~3 | LuaJIT: patched direct jumps |
| other (vmstate store, constant loads) | 0 | ~2 | |
| **total** | **99** (99.8 [M]) | ~29 (32.0 [M]) | LuaJIT categories are +-2 each (read from the dump, frequencies from the tree) |

Per-frame work (slot stores, push, pop, result store, guard, arithmetic) is ~28.5 of our 99; the other ~70.5 are per-fragment. Cycles follow the
same picture at a lower ratio: 16.2 cycles per call at IPC 6.2; removals of memory round trips (the budget store / reload, the reloads of `previous` /
`savedpc`, the slot reloads) matter more than removals of register instructions (JIT_TRACE.md 21.1: 9.5 store-forwards per call).

### 2.2 permute (method recursion with a loop; eager frames)

`permute` has a `for` loop, so `lazyci` does not apply (`maybe_lazy_proto` requires a loop-free proto) and its native pushes are **eager**: the entry
trace's push is 18 instructions (`ci.next`, `func`, `top`, `thread.top`, `callstatus`, `trap`, callee `savedpc`, `kcache`, caller `savedpc`,
`thread.ci`, `r13`, `rbx`) plus 5 of room checks [D] (trace 3 of `permute 300`). Every fragment repeats the method lookup of `self:permute` /
`self:swap`: `HMISS` of the key on the instance, metatable load and null test, flag test, `__index` `HREFK` + load, flag test, method `HREFK` + load,
`EQREF` on the proto: **34 instructions** per lookup [D]. Measured per call [M]: ours 266 instructions (permute 300), LuaJIT 53 at its best run
(its formation is a lottery: 53 .. 563 per call in consecutive runs, STUDY_LUAJIT_GAP 1). docs/STUDY_COVERAGE.md 4 decomposed ours as recursion
protocol + loop 136, method lookups 36, `count` 10, the two swaps 88. Per-call protocol categories (from the entry trace and the coverage study's
variants): poll 3-6, budget 4-6, eager push 18 (lazy would be 8), room 5, identity = the method chain 34 per fragment (2.3 fragments per call),
return guards with the eager `CIST` tests ~13.

### 2.3 towers, micro-benchmarks, non-recursive calls

- `towers 100` [M]: ours 107 instructions per call, LuaJIT 62-66 (two runs) (method recursion `self:moveDisks`, leaves call `moveTopDisk` which inlines `pushDisk` /
  `popDiskFrom`).
- Inlined, non-recursive calls are already at LuaJIT's level or better (docs/STUDY_ALLOC_CALL.md 3, per iteration): inlined call 1.7 cycles / 10
  instructions (LuaJIT 3.3 / 6), method call `o:get()` 1.6 / 12 (3.1 / 4), inlined tail call 2.3 / 18 (3.3 / 6). The callee's frame is virtual, the
  identity guard is hoisted into the pre-roll, nothing is stored (JIT_TRACE_REF P-E 1). `micro/self_call` 184.7 M / 29.4 Mcyc against LuaJIT 60.7 M /
  20.9 Mcyc; `micro/call_arg_ret` 203.1 M / 30.2 Mcyc against 80.7 M / 20.9 Mcyc [M] (the difference is loop code and the second budget charge, not
  frames).
- **The protocol problem is recursion (and calls that cannot be inlined: depth, `TAILCALL` of the root, C functions), not calls in general.**

### 2.4 tier 1 and the interpreter

fib 32 [M]: tier 1 (`RUSTYNAIL_JIT_TRACE=off`) 151 instructions / 28.2 cycles per call; our interpreter (`RUSTYNAIL_JIT=off`) 263 / 54.3; LuaJIT's
interpreter (`-joff`) 122 / 36.5. Tier 1 is faster than LuaJIT's interpreter in cycles. JIT_TRACE_REF P-M2 tallied the tier-1 protocol: `CALL` 56,
`RETURN1` 33, the shared `ret_tail` 29 (of which the caller's resume lookup `func -> stack -> closure -> proto -> jit` 9 and the resume-table lookup 9);
removing 4 % of tier-1 call instructions bought 0 % cycles there (dependent store-to-load chains through `CallInfo`). The interpreter is below 1 % of
the cycles of every benchmark in docs/STUDY_ALLOC_CALL.md 2.

## 3. The frame model the three layers share

**Decision [D]: `CallInfo` stays the only frame format.** The interpreter's `OP_CALL` / `luaD_precall` equivalent (`pre_call_lua_fast`), the tier-1
call template (`calls.rs`), the C API (`lapi`, `lua_getinfo`, `lua_Debug` strings), `debug.traceback` / `getinfo` / `sethook` / `setlocal`, error
unwinding and `pcall` (`old_errfunc`, `CIST_*`), coroutines (`nyield`, `k`, `ctx`), the GC's stack scan and shrink (`ci.top`), the stack overflow
checks and the 5.4 / 5.5 differences all read it. LuaJIT-style slot frames (function + frame link in the stack slots) would buy ~7 instructions per
call over the lazy push (section 2.1: our push 8, theirs ~1 per frame) and require rewriting every reader above; the measured gap is elsewhere.

What traces may do with frames (all of it exists today; the design extends it, it does not replace it):

- **Virtual frames** (TR3): an inlined callee's registers are SSA values; its `CallInfo` is written only by an exit (snapshot `FrameLink`s rebuilt by
  `exit.rs` / the inline `mat_stub`), or by a frame side trace that keeps them virtual (S1).
- **Lazy frames** (BUNDLE-K L1, ON since C70): a native push writes `ci.func` and the caller's `savedpc`; `top`, `callstatus`, `trap`, `kcache`, the
  callee's `savedpc`, `thread.top`, `thread.ci` are completed by `exit::lazy_leave` on every way out of the region.
- **The region** (invariant L-INV, JIT_TRACE_REF "BUNDLE-K L1" and docs/JIT_TRACE.md "lazy_root"): `ExitArea::lazy_root` is a complete frame; every
  frame above it up to `r13` was pushed natively **during the current region** (since the last outer entry or leave); inside the region no GC
  safepoint, no stack move, no Lua code outside the family's traces, no helper that runs Lua code.
- **Eager frames** for families that are not lazy-aware (protos with loops, mixed result counts, C-entered, vararg).

The new elements use one more word per frame, **`jret`** (E3; tier 1 in E9): the continuation of a natively pushed frame, a pointer to a stable
per-site pad. It is meaningful only where the reader can prove the frame was pushed by a writer of the word (a lazy frame above `lazy_root`; a tier-1
frame marked `CIST_JIT`); every other reader ignores it, so the interpreter never writes it.

## 4. The protocol elements

Each element: mechanism, files, soundness (the invariants it relies on, section 5 lists them), verification, gain.

### 4.1 E1: up-recursion unrolling with frame side traces (H9)

**Mechanism.** The function-entry recording of a native-recursion family (`rec_native`, `fn_root`, not a side or continuation recording) inlines its
self-call as an ordinary inlined call (F4 machinery, `call_lua`) at depths `0 .. k-2` and ends with the native push at depth `k - 1`
(`end_rec_call` at depth `d`): the callee guard compares with frame `d`'s closure ref, the TR6c room covers the new frame, TR6b needs `d + 1` spare
`CallInfo` nodes. The assembler's push (`rec_call_tail_lazy`) first turns the `d` inlined frames into lazy frames (per frame: `ci.next`, `func =
caller func + a + 1`, the caller's `savedpc` = the inlined `CALL`'s continuation, `SetCi`, `AddFrame`), then pushes the final frame and jumps to the
loop top, where frame `d + 1` runs the same trace. The writeback before the push already stores every live slot of every frame (root-relative slot
numbers). Exits inside the inlined frames (the leaves at depth 1 .. k-1) are frameful exits; their side traces are S1 frame side traces with the
inlined frames virtual, recorded through the leaf's `RETURN` back into the inlined caller, which continues at the continuation pc (LuaJIT's
down-recursion: in the prototype's fib these side traces fold whole `fib(2)` / `fib(3)` subtrees with guards, `RUSTYNAIL_JIT_DUMP` traces 14 and 16).
Returns from pushed frames into the materialised inlined frames go through the existing continuation heads (the inlined frames' sites are the same
`CALL`s).

**Adaptive `k` (required, not optional).** The prototype's fixed `k = 3` regresses programs whose recorded path reaches a leaf or a loop before level
`k` [M]: `permute` (the entry head goes `Black` after 8 `FnShort` and 2 `InnerLoop` aborts, +37 % instructions), `towers` (+4 % instructions, +36 %
cycles: 33 frame side traces instead of 21, more exits), `list` (+65 %). Policy: (1) the recording accepts the deepest self-call it reaches
(`k_eff = min(k, levels before the first leaf)`), and only `k_eff >= 2` makes an unrolled trace; (2) a head whose unrolled recordings abort
(`FnShort` / `InnerLoop` / `SideShort` at inlined depth) `n` times, or whose installed unrolled trace trips the ineffective rule, falls back to
`k = 1` for good (a per-proto flag, like K6's `fnself`); (3) knob `recunroll` (0/1 = off, default 3 after the gate). With the fallback the worst case
is a few extra recordings (as K6), and `k = 1` is today's code path, bit for bit.

**Files.** `tr/record.rs` (`call_lua` condition, `end_rec_call` at depth `d`, the fallback counters), `tr/ir.rs` (`RecCall::inl`), `tr/asm.rs`
(`rec_call_tail_lazy`; the eager push needs the same loop, or E1 waits for E4 on eager families), `tr/cache.rs` (policy, `TrProto` flag),
`tr/mutants.rs`.

**Soundness.** Relies on I1 (inlined frames are virtual and exits rebuild them: unchanged), I3 (the entry check covers the room of every inlined call
and `d + 1` nodes: the trace never pushes a frame the interpreter would not push on its fast path, so `stack overflow` is raised by tier 1 / the
interpreter at exactly the same depth), I2 (one budget charge for the whole recorded path, exits refund per snapshot: unchanged), I6 (the `d + 1`
frames are pushed during the region and are lazy; `lazy_leave` completes them like any lazy frame: `callstatus` from the `CALL`s' `C`, which
`lazy_cs` requires to be one value), I5 (the loop top polls once per `k` calls; the inlined calls need no poll, TR22: asynchronous hooks may fire late,
the same argument as for every inlined call today). No new kind of state exists: every intermediate state is one the existing F4 + lazy machinery
already produces (an inlined recursion, then lazy pushes). Debug API / tracebacks / errors see only completed frames (errors happen outside traces,
after the leave). Coroutines: unchanged (no region spans a C function).

**Verification.** `tr_core::h7_recursion_programs`, `k6_selfrec_programs`, `pm3_programs_run_on_traces`, `cp1_lazy_programs`, `bk_lazy_programs`
(observer programs under `RUSTYNAIL_GC_VERIFY=1`), all with `recunroll=1/2/3`; `tr_identity` (the count identity at every budget); `tr_oracle`
(stays at every exit); jit-diff template set `rc` (new: deep recursion to the overflow depth inside `pcall`, mutual recursion, leaf/non-leaf mix
changes, callee replaced mid-run, errors and tracebacks from depth, coroutines yielding inside recursion, hooks installed from a finalizer).
New mutants: `h9_inl_push_skip` (an inlined frame not materialised), `h9_inl_savedpc_off`, `h9_inl_bump_off`, `h9_maxci_short` (one spare node
too few: `exit.rs` asserts it), `h9_fallback_never` (policy). Prototype evidence: `b1` with `RN_PROTO=128` passed all 213 programs of `tests/vm`
and `tests/lazyci` in trace modes `on` / `always`, without and with `RUSTYNAIL_GC_STRESS=2` (852 runs, 0 differences; the one difference,
`tables_15_alimit_big_moves`, also occurs with the prototype off: the program depends on the string-hash seed, which the smoke script did not pin).

**Gain [M]** (b1, `RN_PROTO=128` / `64` against 0):

| program | instructions | cycles |
|---|---|---|
| fib 32, k = 3 | 703.6 -> 451.6 M (-35.8 %) | 113.9 -> 73.0 M (-35.9 %) |
| fib 32, k = 2 | -> 559.0 M (-20.6 %) | -> 91.0 M (-20.2 %) |
| binary_trees 15 | 3104 -> 2498 M (-19.5 %) | 612.8 -> 496.0 M (-19.1 %) |
| sumrec (linear) | 1055.8 -> 693.1 M (-34.4 %) | 233.9 -> 115.4 M (-50.7 %) |
| permute / towers / list (fixed k, no fallback) | +37.4 % / +4.4 % / +65.3 % | +34.1 % / +35.6 % / +34.9 % |
| queens, richards, deltablue, havlak, raytrace | within 0.6 % | within noise |

### 4.2 E2: region-scoped callee identity

**Mechanism.** In a native-recursion recording of the root frame, `GETUPVAL` of an upvalue whose current value is the running closure itself
records as the running closure's ref (`func_ref`): no `UREF`, no TR15 range guard, no `ULOAD`; the self-call guard compares the ref with itself and
disappears. The 15-instruction chain moves to the **checked entries** of the family: (a) every outer entry (dispatch cell: root entry, prelude, link
from another family), placed in the entry block before the loop label (it runs for leaves too, but only once per region entry); (b) every return
of the region root (`r13 == lazy_root` in `rec_ret_tail`: the caller becomes the root and may have been suspended while foreign code ran) continues
through the continuation head's checked entry (the plain dispatch cell) instead of the inner entry.

**Why the naive form is unsound and the region form is not.** The prototype (`RN_PROTO=1`) skips the chain everywhere and fails
`tests/vm/k6_02_path_switch.lua` ("callee replaced mid-run": `g = function ... g0(n) end` makes the old closure call the new one through the
upvalue). The upvalue can change only through (i) code outside the region (the main chunk's register store into an open upvalue, `SETUPVAL` in
tier 1 / the interpreter, `debug.setupvalue` / `upvaluejoin` / `setlocal`, a C function), or (ii) a `USTORE` in a trace of the family. (i) cannot run
while the region runs (L-INV: no Lua code, no C function, no GC finaliser inside a region); every way back into the family after foreign code is an
outer entry or a return of the region root (a frame below the root is never returned into by an inner entry: lazy returns happen only for frames
above the root, which were pushed in this region after the last foreign code), and both are checked entries. (ii) is excluded by a family rule: a
family with `idfold` traces admits no trace with an upvalue store (`USTORE` of any upvalue: `(closure, index)` pairs may alias, P3d); a recording that
would add one to such a family flushes the `idfold` traces first (or aborts; the cheaper choice). The open-upvalue range (TR15) is checked at the
same checked entries: an open upvalue below the root stays below it while the region only pushes frames above, and a closed one stays closed.

**Files.** `tr/record.rs` (`uload`, `end_rec_call`), `tr/asm.rs` (entry block, root-return target), `tr/cache.rs` (family rule, `idfold` flag),
`tr/mod.rs` (flush on a `USTORE` joining), `tr/mutants.rs`.

**Verification.** `tr_core::k6_selfrec_programs` (contains `k6_02`), `h7_recursion_programs` (`h7_*`: callee replaced mid-run, closures and upvalues
in recursion), new programs: the upvalue reassigned by a leaf that leaves the region (a C call in the leaf, then `debug.setupvalue` or a `SETUPVAL`
in tier 1) while frames of the old region are suspended (the root-return case), and a family with a `USTORE`. Mutants: `idfold_outer_unchecked`
(outer entry skips the chain), `idfold_rootret_inner` (root return through the inner entry), `idfold_ustore_family` (family rule off).

**Gain [M].** fib 32: 611.9 M (-13.0 %) / 103.0 Mcyc (-9.6 %); on top of E1 (bits 129 against 128): 451.6 -> 410.9 M (-9.0 %), 73.0 -> 65.2 Mcyc
(-10.7 %); binary_trees -5.4 % / -3.7 %; sumrec -11.3 % / -26.5 %. 0 on method recursion (permute, towers: the callee is not an upvalue; E5).

### 4.3 E3: direct native return through `jret`

**Mechanism.** The lazy push stores into the new frame the address of a **stable per-site pad** (memory owned by the continuation head, never freed
while the proto lives; the pad is `add rbx, -(A+1)*16; jmp <inner entry of the site's continuation trace>`, re-patched when the head's trace changes,
pointing to the leave stub when there is none). A frame above `lazy_root` returns with: lazy-root test, `r1 = [r13 + previous]`, `r7 = [r13 + jret]`,
`r13 = r1`, `jmp r7`; the return guards of such a frame are empty (only the root frame keeps the `CIST_FRESH` / `CIST_C` / site guards). The site
must be compatible with the returner (R2's wanted-count rule): the pusher writes a pad only for sites whose `C` equals the proto's single return
count, else 0 and the returner takes today's path.

**Files.** `rn-core/src/state.rs` (`CallInfo::jret`), `jit_layout.rs`, `tr/cache.rs` (pad memory in `HeadData`, re-patch on install / flush),
`tr/asm.rs` (`rec_call_tail_lazy`, `rec_ret_guards`, `rec_ret_tail`), `tr/mutants.rs`.

**Soundness.** I6: every frame above the root was pushed in this region by a push that wrote `jret` (the field is never read for the root or below);
pads are stable memory, so no frame can point into a flushed trace (the P-M2 objection). Mutants `jret_store_skip`, `jret_for_root`.

**Gain [M] (conservative prototype, `RN_PROTO=8`: two extra stores per push, the cell loaded through a pointer, two root tests).** fib -4.5 % instructions,
+0.8 % cycles (noise); with E2 and E7 (bits 25 against 0): -20.5 % / -14.8 %; **after E1 + E2 (bits 153 against 129): -7.4 % instructions, +0.3 %
cycles.** On single-site protos the conservative form is worse (`sumrec` +9.1 % instructions): the full form (one pad store, one root test) is
~10 instructions cheaper per return than the prototype by count [D]. **Not recommended now**: no cycle gain measured once E1 + E2 are in.

### 4.4 E4: lazy frames for recursion families with loops

**Mechanism.** Make `maybe_lazy_proto` accept protos with loops. Their loop traces then belong to a lazy-aware family: an outer entry of a loop trace
stores `lazy_root`, its exits complete the chain (`exit_entry` with `meta.lazy`), its side traces are lazy-aware. The RETF rule of C70 fix 2 (`Rec::retf`
refuses to pop into a maybe-lazy caller) must hold for the larger set (it already uses the superset `maybe_lazy_proto`, so widening the predicate
widens the refusal: re-measure `heapsort` / `field_rw`, which lost `RETF` once).

**Soundness.** I6 for loop roots: a loop trace's back edge stays in the region (no Lua code, no safepoint). Loop traces may allocate (class-A
helpers), which lazy regions already allow: an allocation whose GC step would run exits first (docs/JIT_TRACE.md 6.9), and every exit leaves the
region, so the collector never sees a lazy frame. What is new is a root kind (the loop trace) whose outer entry and back edge must keep `lazy_root`
right, and side traces of loop exits inside a region. Mutants: `lazy_loop_root_not_set`, `lazy_loop_exit_no_leave`.

**Files.** `tr/cache.rs` (`maybe_lazy_proto`, `TrProto::lazy`, `lazy_cs`), `tr/mod.rs` (`AsmEnv::lazy` for loop roots), `tr/asm.rs` (outer entry of
loop roots, `lz_leave`), `tr/exit.rs` (`lazy_leave`), `tr/record.rs` (`Rec::retf`).

**Gain [E].** permute: eager push 18 -> lazy 8 and eager return guards ~13 -> 3, per call ~-15 of 266 instructions (-5.6 %); E1 then applies to
permute (its recursion prefix is loop-free: the self-call precedes the loop). towers: `moveDisks` is loop-free and already lazy-aware (0).

### 4.5 E5: region-scoped method identity (method recursion)

**Mechanism.** For `self:m(...)` recursion where the recursive call passes the frame's own `self` (`SELF A B` with `B` = register 0 of the frame, so
the callee's register 0 is the same object, provable at record time): the lookup chain (`HMISS` on the instance, metatable, `__index`, method
`HREFK`, `EQREF` proto) runs at the checked entries only, like E2. Family rule: no trace of the family stores into the receiver's hash part
with a new key (`NEWREF`, which could create the method key; `HSTORE` through `HREFK` of an existing other key is fine), into any table on the lookup
path (metatable, `__index` table), and no `setmetatable` (a C function: leaves the region anyway). Within one trace, K5 (method lookup CSE,
docs/STUDY_COVERAGE.md 5) gives the same for the inlined levels of E1.

**Files.** `tr/record.rs` (`SELF` recording, receiver provenance), `tr/opt_mem.rs` (K5 CSE, the family store facts), `tr/cache.rs` (family rule),
`tr/asm.rs` (checked entries as in E2).

**Gain [E].** permute: 36 instructions per call (STUDY_COVERAGE `p_rec` - `p_plain`), -13.5 %; towers: one chain per fragment (~25) on ~107 per call,
-10..15 %; richards small (tail-called bodies stay in tier 1). Risk medium-high (the family store rule needs the alias facts of `opt_mem.rs`).

### 4.6 E6: direct exit-to-side links for recursion families

**Mechanism.** A guard whose exit has a side trace attached jumps straight to it: the guard's `jcc` is patched (as `lazystub` already patches the stub
address) to the side trace's carry entry; the budget refund of the exit is folded into the side trace's first charge (`n_side - refund`, so the
charge check and the exit state are those the stub would have produced); the side trace takes the exit map's registers (R1 `regcarry`, today
excluded for `rec_call` / `rec_ret` / lazy traces). Unlink on flush re-patches the `jcc` to the lazy stub.

**Files.** `tr/asm.rs` (`inline_stub`, carry path, the side trace's first `BUDGET`), `tr/cache.rs` (`carry_slots`, `install`, unlink on flush),
`tr/mod.rs` (`carry_exits_on` for lazy / native-recursion traces), `mem.rs` (patching).

**Soundness.** TR5 (the folded refund is exactly the stub's `vb + refund`: the identity tests catch any difference); R1 carry maps (the side trace's
entry loads are the parent's registers); flush ordering (the patch back happens before the side trace's code is released). Mutants:
`dlink_refund_off`, `dlink_unlink_skip` (flush stress).

**Gain [D/E].** fib: the leaf's stub (12 instructions incl. a counter read-modify-write and a budget store) and the side trace's budget reload ->
~2: -5.5 instructions per call (-5.5 %). Any program with many linked exits gains ~10 instructions per exit (queens 35 M exits, list 76 M,
binary_trees 30 M at the study sizes: 2-7 % by arithmetic, before checking which of them already take the R1 carry path).

### 4.7 E7: poll placement

**Rule.** A fragment needs a poll only if some way out of it reaches a bytecode of a new frame without polling first. In a lazy family a native
push through the `rec` cell lands at the entry trace's loop top, which polls; the null-cell path (`lazy_rec_jump` fallback) gets a poll on the cold
path. Files: `tr/asm.rs` (`TOp::Poll`, `lazy_rec_jump`). Return fragments poll only at returns of the region root (a chain of lazy returns is bounded by the region depth). **Gain [M]:** `RN_PROTO=16` (no
poll in push-through-cell and return fragments): -3.0 % / -2.8 %; after E1 + E2 the total of E3 + E7 is 0 % cycles (65.2 vs 65.4 Mcyc). **Not
recommended now.** If built: the latency of an asynchronous hook grows from one fragment to one region-root return (bounded by the depth), which the
semantics allow (asynchronous hooks have no instruction-exact delivery; synchronous `sethook` leaves the region through its C call).

### 4.8 E8: the budget stays

The budget (TR5) is what makes the instruction-count identity (`fetches + insns`) checkable in every mode; `tr_identity`, the count-budget twins
(3 / 17 / 200) and the oracle stand on it. Removal ceiling [M] (`RN_PROTO=4`, unsound): -5.0 % instructions and +11 % cycles (layout) on fib: no cycle lever.
With E1 one charge covers `k` calls already. What remains worth doing (part of E6): keep the budget in `r11` across native transfers whose target is a
trace inner entry and store it only on the leave / null-cell paths (1 store per transfer).

### 4.9 E9: tier 1

Tier 1 carries 16-47 % of the cycles of `richards`, `deltablue`, `raytrace`, `cd`, `list` (docs/STUDY_COVERAGE.md 2) and ~100 instructions of protocol
per call. Three items, none prototyped:
1. **`jret` for tier-1 returns:** the tier-1 `CALL` stores the caller's resume address (a stable per-site cell, so a T15 recompile only rewrites the cell)
   in the callee's `CallInfo`; `RETURN` of a `CIST_JIT` frame jumps through it instead of `ret_tail`'s `func -> closure -> proto -> jit` walk and resume
   table lookup (18 of its 29 instructions) [E: ~15 per call].
2. **Call-site inline cache:** guard the closure's proto against the cached one and jump to its cached tier-1 entry with the argument-count work done
   at compile time (the kind test, `entry0` load, `numparams` test: ~11) [E].
3. **Native return of function-entry traces** (`fn_end` exits today: the trace exits before its root `RETURN` and tier 1 returns): with (1) the trace
   pops natively and jumps through `jret` [E: an exit stub + a tier-1 `RETURN1` + `ret_tail`, ~75 instructions per function-root call; cd 50:
   ~3 %].
Files: `rn-jit/src/calls.rs` (`tpl_call`, `tpl_return`, `ret_tail`), `lower.rs`, `rt.rs` (resume cells), `rn-core/src/state.rs` (`jret`), `tr/asm.rs`
(`fn_end`). Evidence against a large cycle gain: P-M2 removed 4 % of tier-1 call instructions for 0 % cycles. Measure (1) before building (2) and (3).

### 4.10 The interpreter

No protocol change. The interpreter's `pre_call_lua_fast` / `OP_RETURN` keep writing complete frames; they do not write `jret` (readers never trust it
for interpreter frames: lazy frames are above `lazy_root` by construction, tier-1 frames carry `CIST_JIT`). Its share is below 1 % everywhere; its call is
263 instructions per fib call against LuaJIT's interpreter's 122 [M], which matters only for `JIT=off` users and the start of every program (not a
target of this design).

### 4.11 Considered and rejected

- **Machine `call` / `ret` as the return stack** (perfectly predicted returns): the machine stack would grow by 8 bytes per region frame (a region can
  reach the 500 000-frame Lua stack limit: 4 MB of C stack, and coroutines run on the host stack); every leave must restore `rsp`. Branch misses are
  not the cost (fib: 2 M for 7 M calls). Rejected.
- **A shadow return stack instead of lazy `CallInfo` nodes inside regions:** saves ~4 instructions per push (no `ci.next` load, no `func` store) and
  needs a new materialisation path at every leave and exit. Below the 5 % bar after E1 (one push per call remains in both).
- **LuaJIT slot frames:** section 3.
- **Removing the budget in production:** section 4.8.

## 5. Semantics that must stay identical, and the invariants behind them

Lua-visible behaviour that every element preserves (and why):

| behaviour | how it stays identical |
|---|---|
| debug hooks (call / return / line / count) | compiled code is never entered with a hook installed (J-ENTRY); an asynchronous install sets `TRACE_POLL`, every element keeps a poll before control reaches a new frame's bytecode without one (E1: the loop top, once per `k` calls, like inlined calls; E7 states its rule) and the poll exit hands over to the interpreter with `trap = 1` |
| count hooks / instruction counting | TR5: every segment charges its exact fetch count first, every exit refunds per snapshot; E1 charges once for the unrolled path; E6 folds the refund exactly |
| `stack overflow` at the same depth, same message | TR6c: a trace pushes a frame only where the interpreter's fast path would (entry check of every inlined call's room and of the final push); otherwise tier 1 / the interpreter push and raise; E1's `d + 1` frames are covered by the entry check (all or nothing: the trace exits before any push, tier 1 pushes one by one and raises at the reference depth) |
| C-stack limit (`LUAI_MAXCCALLS`, "C stack overflow") | native pushes are Lua-to-Lua calls, which do not count in either interpreter version; no element crosses a C boundary inside a region |
| errors, messages, `debug.traceback`, `getinfo` | no error is raised inside a region (instructions that can raise exit first); exits and leaves complete every lazy frame (`savedpc` of each caller = after its `CALL`, `func`, `top`, `callstatus`, `kcache`) before Lua code or the error machinery can look; E1's inlined frames become lazy frames with the same fields |
| coroutines (`yield` across frames, `resume`) | a region never spans a C function, so never a yield / resume; frames are complete at every leave |
| GC (finaliser timing, `collectgarbage("count")`, stack shrink) | no allocation and no safepoint inside a region (class N/S/A helpers only); `ci.top` (stack shrink) is completed at leave (C72 verifier check) |
| memory errors | nothing allocates inside a region; E4 must keep `Alloc` exits as leaves |

Invariants relied on (names used above):
- **I1 (TR3)** inlined frames are virtual; snapshots carry `FrameLink`s; exits (handler, `mat_stub`) and S1 frame side traces rebuild or keep them exactly.
- **I2 (TR5)** budget charge per segment, refund per snapshot, count identity; no overshoot within a stay.
- **I3 (TR6b / TR6c)** room and spare-`CallInfo` conditions of every call on the path are entry checks; native pushes repair spare nodes (CP1 /
  `cispare`) or leave.
- **I4 (TR15)** an open upvalue never points into the trace's frames (range guard; E2 moves it to checked entries, with the argument in 4.2).
- **I5 (TR22)** a poll per loop iteration and before every transfer that reaches a new frame's bytecode without one.
- **I6 (L-INV)** the lazy region: root complete, frames above it pushed natively in the current region, completed on every leave; no GC safepoint,
  stack move, foreign Lua code or C function inside.
- **I7 (R1 / exits)** exit maps, carry maps and their stores (E6 extends carry to lazy families).
- **I8 (sinking)** sunk tables are materialised at exits (unchanged; lazy traces use generic sunk stubs).
- **I9 (C70 fix 2)** `RETF` never pops into a maybe-lazy caller (E4 widens the set).
- **I10 (TR32)** dependency edges at inline time (E1's inlined self-calls register the same proto: nothing new).

## 6. Verification map (existing infrastructure, per element)

| element | twins / programs | oracle / identity | jit-diff | new mutants |
|---|---|---|---|---|
| E1 | `tr_core::{h7_recursion_programs, k6_selfrec_programs, pm3_programs_run_on_traces, cp1_lazy_programs, bk_lazy_programs, deep_recursion}` with `recunroll=1/2/3`, `+verify`, exit storms, budgets 3/17/200, GC stress 2, `+emurun` | `tr_identity`, `tr_oracle` | new set `rc` | `h9_inl_push_skip`, `h9_inl_savedpc_off`, `h9_inl_bump_off`, `h9_maxci_short`, `h9_fallback_never` |
| E2 | `k6_02` (catches the naive form), `h7_*`, new root-return and `USTORE`-family programs | as above | `rc` with callee replacement | `idfold_outer_unchecked`, `idfold_rootret_inner`, `idfold_ustore_family` |
| E3 | `pm3_*`, `r2_recursion_programs` (multi-site, wanted-count rule) | | | `jret_store_skip`, `jret_for_root` |
| E4 | `bk_lazy_programs` with loop protos, `nr_*` (RETF), `c70`-style deep recursion | | `bk` set with loops | `lazy_loop_root_not_set`, `lazy_loop_exit_no_leave` |
| E5 | `po_poly_programs`, new method-recursion programs with metatable / `__index` / instance-key changes inside the recursion | | `oop` set | `midfold_newref_family`, `midfold_receiver_other` |
| E6 | `r1_*` carry programs, flush stress (`maxtrace` small) | `tr_identity` (folded refund) | | `dlink_refund_off`, `dlink_unlink_skip` |
| E7 | `tr_poll`, hook programs of `h7_*` (hook installed from inside the recursion, from a finaliser) | | | `poll_rule_null_path_skip` |
| E9 | `t15_core`, `t1i_inline_programs`, `vm_corpus` | `t15_oracle` | | per item |

## 7. Staged plan

Gains: **[M]** prototype measurements (section 8), **[E]** derived from the category tables (section 2) with the derivation in section 4. Effort in
builder sessions (one Sonnet builder, light gates per commit, heavy gate per stage). Ordered by measured gain per risk; large stages are not dropped.

| # | stage | fib | permute | binary_trees | towers | queens / richards / cd / raytrace / havlak / deltablue | effort | risk | verification |
|---|---|---|---|---|---|---|---|---|---|
| 1 | **E1 up-recursion unrolling, adaptive `k`** (lazy families) **BUILT** (JIT_TRACE 34; fib -35.8 % / -36.5 % as built) | **-35.8 % instr / -35.9 % cyc [M]** | 0 with the fallback (fixed k: +37 % [M]); after stage 4: [E] -20..30 % | **-19.5 % / -19.1 % [M]** | 0 with the fallback (fixed k: +4 % / +36 % [M]) | 0 / 0 / -2.5 % instr (cd 50, with E2) / 0 / 0 / 0 [M] | 3 | medium (formation policy; existing frame machinery) | 4.1 |
| 2 | **E2 region-scoped callee identity** **BUILT** (JIT_TRACE 34; with 1: fib -44.5 % / -45.7 % as built) | -13.0 % / -9.6 % alone; **with 1: -41.6 % / -42.8 % [M]** | 0 | -5.4 % / -3.7 % [M] | 0 | 0 [M] | 2 | medium (new invariant, checked entries) | 4.2 |
| 3 | E4 lazy frames for families with loops **BUILT** (JIT_TRACE 35; only families whose first trace is the entry trace ending in a self-call) | 0 | [E] -5.6 % alone, enables 1; **as built -4.8 % / -6.0 % [M]** | 0 | 0 | 0 [J]; as built 0 | 2-3 | medium-high (loop roots in the region, RETF rule) | 4.4 |
| 4 | E5 method identity + K5 **BUILT** (E5 only; JIT_TRACE 35) | 0 | [E] -13.5 %; **as built with E4 -14.2 % / -15.9 % [M]** | 0 | [E] -10..15 %; **as built 0 (E5 off for loop-free families; on: +2.6 % cycles [M])** | richards [E] < 2 %; as built within 0.09 % [M] | 2-3 | medium-high (family store rule) | 4.5 |
| 5 | E6 direct exit links (+ budget in `r11` across transfers) **BUILT** (JIT_TRACE 36; fib -8.3 % / -5.8 %, queens 0: lazy families only) | [E] -5.5 % instr | [E] -2 % | [E] 2-4 % | [E] small | queens [E] 2-7 % (35 M exits; carry-path share unknown) | 2-3 | medium (patching lifetime) | 4.6 |
| 6 | E9 tier-1 `jret` return, then IC, then native `fn_end` returns | 0 | 0 | 0 | 0 | [E] cd ~3 %, raytrace / deltablue / list a few % of instructions; cycles unknown (P-M2: 0 % for -4 %) | 2 + 2 | medium | 4.9 |
| 7 | E3 direct native return | -4.5 % instr / 0 % cyc alone; **0 % cyc after 1 + 2 [M]** | needs 3 | [M] -1.8 % instr | 0 | 0 | 1-2 | low-medium | 4.3 |
| 8 | E7 poll placement | -3.0 % / -2.8 % alone; **0 % cyc after 1 + 2 [M]** | 0 | [M] -1.2 % instr | 0 | 0 | 1 | low-medium | 4.7 |

Stages under 5 % or not recommended, with the measurement that says so:
- **E3, E7**: bits 153 (E1 + E2 + E3 + E7) against 129 (E1 + E2): 380.7 vs 410.9 M instructions, 65.4 vs 65.2 Mcyc on fib (min of 5): no cycle gain.
- **E8 (budget removal)**: -5.0 % instructions, +11 % cycles on fib (`RN_PROTO=4`); and it would remove the instruction-count identity (TR5), the oracle of
  every twin test.
- **Shadow return stack**: ~4 instructions per push after E1 (per-frame work stays) = under 4 % of fib's remaining ~58 per call; new materialisation
  path. Not planned.
- **queens, richards, deltablue, havlak, raytrace**: every prototype bit leaves their instructions within 1.2 % [M]: the trace recursion protocol
  is not what they spend; they need H8 coverage and tier 1 (stage 6).

Expected position after stages 1 + 2 [M]: fib 65.2 Mcyc against LuaJIT's 42.9 (1.52x, from 2.65x); binary_trees 484 Mcyc against LuaJIT's 864 (0.56x,
from 0.71x); sumrec 116 against 86 (1.35x, from 2.73x). After 3 + 4 (permute) [E]: the protocol share of permute's 266 instructions per call falls by
roughly the fib ratio on the recursion part (136 -> ~80) and by the lookup chains (36 -> ~10): ~180 per call, 3.4x LuaJIT's best run.

## 8. Prototypes

**Patch:** `docs/proto-call-protocol.patch` (apply on a536f04 with `git apply`; touches `rn-core/src/jit_layout.rs` and `rn-jit/src/tr/{asm,cache,ir,record}.rs`;
every change is gated by `RN_PROTO=<bits>`, read once by `cache::proto_bits()`; with the variable unset the binary executes exactly the instructions of
the base). Not for merging: some bits are unsound by construction (stated per bit) and the code takes shortcuts a production version must not.

| `RN_PROTO` value | element | soundness of the prototype | what it does |
|---|---|---|---|
| 1 | E2 ceiling | **unsound** (no checked entries; fails `k6_02`) | `uload` returns `func_ref` for a self-referencing upvalue; `end_rec_call` drops the guard when the callee is the running closure |
| 2 | poll ceiling | **unsound** | no `POLL` in lazy traces |
| 4 | budget ceiling | **unsound** (identity, count budgets) | no `BUDGET` in lazy traces |
| 8 | E3 (conservative) | flush-unsafe (stores a cell address and a raw base pointer in `ck` / `ctx` of the `CallInfo`, fields Lua frames do not use) | lazy push stores the continuation cell and the caller's base; lazy frames return without guards or site chain |
| 16 | E7 | sound for lazy families except the null-cell path (no poll added there) | no `POLL` in fragments ending in a push through the entry head's cell or in a native return |
| 64 / 128 | E1, k = 2 / 3 | sound as far as tested (no adaptive policy: regresses permute, towers, list) | `call_lua` inlines self-calls below depth `k - 1`; `end_rec_call` at depth `k - 1` records the inlined frames (`RecCall::inl`); `rec_call_tail_lazy` turns them into lazy frames before the final push |

Results (b1, instructions first run, cycles min of 5 for fib, min of 3 otherwise):

| configuration (`RN_PROTO`) | fib 32 instr / cyc (M) | binary_trees 15 | sumrec | permute 300 | towers 100 | list 300 |
|---|---|---|---|---|---|---|
| 0 | 703.6 / 113.9 | 3104 / 612.8 | 1055.8 / 233.9 | 692.7 / 120.8 | 353.1 / 86.8 | 342.3 / 82.5 |
| 1 (E2) | 611.9 / 103.0 | 2938 / 590.1 | 936.0 / 171.9 | = / 120.4 | = / 86.8 | 337.8 / 81.8 |
| 2 (no poll) | 661.3 / 109.0 | | | | | |
| 4 (no budget) | 668.4 / 126.6 | | | | | |
| 8 (E3) | 671.9 / 114.9 | 3047 / 610.9 | 1152.3 / 242.6 | = / 120.7 | 350.5 / 88.2 | = / 83.9 |
| 16 (E7) | 682.5 / 110.7 | 3066 / 595.0 | 1025.8 / 242.9 | = / 121.1 | 351.5 / 85.4 | = / 84.5 |
| 25 (E2 + E3 + E7) | 559.1 / 97.1 | 2842 / 577.6 | 1002.6 / 182.0 | = / 121.3 | 348.9 / 87.5 | 337.8 / 81.5 |
| 31 (+ unsound poll / budget) | 502.8 / 96.8 | | | | | |
| 64 (E1, k = 2) | 559.0 / 91.0 | 2741 / 538.4 | 954.6 / 181.0 | 932.3 / 176.3 | 348.1 / 103.3 | 546.5 / 107.5 |
| 128 (E1, k = 3) | 451.6 / 73.0 | 2498 / 496.0 | 693.1 / 115.4 | 951.7 / 162.0 | 368.7 / 117.7 | 565.9 / 111.3 |
| 129 (E1 + E2) | **410.9 / 65.2** | **2448 / 484.1** | 702.4 / 116.1 | 951.7 / 162.0 | 368.7 / 119.2 | 565.8 / 112.8 |
| 153 (E1 + E2 + E3 + E7) | 380.7 / 65.4 | 2408 / 498.6 | 680.0 / 119.9 | 951.7 / 162.5 | 366.8 / 118.6 | 565.8 / 112.3 |
| LuaJIT | 227.0 / 42.9 | 3083 / 864.3 | 244.4 / 85.7 | 1462.7 / 31.3 (lottery) | 205.9 / 44.9 | 270.4 / 76.9 |

(`=`: identical instruction count to row 0; empty: not run.) E3 / E7 do not fire on permute: it is not a lazy family (stage 3). Source-level
unrolling as a second ceiling: fib with one recursion level written inline (`fibu2`, 3.52 M calls instead of 7.05 M) runs 451.2 M / 82.5 Mcyc on
the base (and 353.8 M / 72.7 Mcyc with bits 25), so E1's recorder unrolling reaches about what removing half of the frames by hand does.

Correctness smoke (not a gate; a scratch script, not committed): differential runs of `tests/vm/*.lua` and `tests/lazyci/*.lua` (213 programs) against `RUSTYNAIL_JIT=off`,
trace modes `on` and `always`: values 0, 128, 152, 129 and 128 with GC stress 2: no difference except `tables_15_alimit_big_moves` in every configuration
including 0 (seed-dependent output; the vm corpus pins `RUSTYNAIL_SEED=0`, the smoke script did not); value 1 (and every combination with it) differs on
`k6_02_path_switch` as section 4.2 predicts.

## 9. Recommendation

Build **stage 1 (E1 with the adaptive `k`) first**, then stage 2 (E2).

- It is the largest measured cycle gain on every recursion program that forms traces: fib -36 %, binary_trees -19 %, linear recursion -51 %.
- Its soundness argument introduces no new kind of state: an unrolled trace is an inlined recursion (F4) followed by lazy pushes (C70), and its leaves
  are S1 frame side traces with virtual frames, all of which exist, are mutant-covered and passed the prototype smoke (852 runs including GC stress 2).
- The risk is formation (fixed `k` regresses permute / towers / list by 4-65 % instructions); the fallback to `k = 1` is today's path, so the
  stage's gate is "no program above +1 % instructions on the 223 with the fallback, and fib / binary_trees within 5 % of the prototype".
- E2 after it: -11 % cycles on top, a self-contained invariant with checked entries, and the existing twins already catch its naive form.
- Then stage 3 + 4 for permute / towers (eager families and method recursion), stage 5 for exits, stage 6 for tier 1 after a measurement of its
  first item. E3 / E7 only if a later profile shows cycles in the return guards or polls again.

## 10. What could not be determined

- E1 on permute / towers / list with the adaptive policy (the prototype has none): whether the fallback keeps them at 0 %, and what permute gains
  after stage 3 (estimate only).
- Cycles of E4, E5, E6, E9: no prototype; the estimates are instruction counts, and P-M2 showed tier-1 instruction removals that bought no cycles.
- LuaJIT's per-category numbers are +-2 per row (read from `-jdump`; it has no per-trace instruction profile here); its permute is a formation lottery
  (31 .. 1367 Mcyc for the same file).
- Lua 5.4 builds were not measured; the prototype code is version-neutral (`cfg` free), the 5.4 matrix is part of each stage's gate.
- The cycle effect of code layout: equal instruction counts gave 113.9 and 122.1 Mcyc in two binaries, so single-digit cycle deltas between builds
  need the multi-path method of C69.
