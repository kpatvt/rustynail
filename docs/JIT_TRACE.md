# JIT tier 2: trace compiler (J5 design)

Status: DESIGN v1.3 (2026-10-30), for review before any J5 builder starts.

Changelog: **v1.3** applies the independent review of v1.2 and the user's decisions on it (section 14, "v1.2 review"):
type-stability rules for the prototype (hot type-guard exits and `Unstable` re-record, free-peel entry failures not
counted, TR21/TR24c); TR13.3 alias rule corrected (must = both tables and indices must-alias); J4 baseline, 1.5
projections and the 1.6 gate re-derived from the measured T13 numbers (T9/T10 skipped, baseline = J5 branch with
`TRACE=off` verified against main@89963c6, min of 9 runs); gate set (b) recomposed around forwarding, LICM and
peeling (pure register loops informational) plus stability programs (d); the unpeeled (`-loop`) layout defined (7.8);
P3 and P8 split, pipeline driver owned by PI, mutants/stress hooks owned by the task that owns the code, UNSAFE.md
merged by the orchestrator, NOTRACES aggregated; TR3a 5.4 `ATOUCH` exception, recorder clears `trap` on all frames
at Leave, TR23.5 restated for intrinsics; full-build requirements for end-of-iteration writeback (S0), entry-guard
placement (7.7), J4 version/stub/nest interaction (3.2, 3.4, F5), counting stubs instead of a per-back-edge counter
(TR20), dependency edges at inline time (3.4, F3). **v1.2** fixes the independent review of v1.1 (at `9c9814e`; section 14 lists every finding and its
disposition): recorder sees callee frames (hooked `'frame` head sets `trap = 1`, TR23.2); recording state lives behind
`JitSlot.data`; one error choke point plus lazy validation (TR23.5); C1 restated as the fetch+insns identity with a
no-overshoot rule (TR5); exit state covers popped inlined frames and native-call stack/CallInfo conditions (TR3,
TR6b/c); TR29 restated without overclaims; loop-end snapshot and budget/poll placement (7.8, TR27); micro-op claims
corrected (V7 is scratch; no memory add; trace id via the exit area); prototype gate tightened (frozen benchmarks,
per-program and non-inlining criteria, precise baseline, inlining-only = no-go for the full build); tasks re-cut (P0
smaller, P5 split, P4 after P5a, early integration task, UNSAFE.md per task, F2 split, 5.4 tax per task, S0 sized L);
numbers recomputed and labelled; rule texts for TR2/TR4/TR31. **v1.1** user decisions D1-D5 (section 13). **v1.0**
first design. The user's decisions of 2026-10-29
(section 13: allocation sinking in scope with a GC-observable policy, 5.4 traces in scope, `RUSTYNAIL_JIT_TRACE`
default on in `jit` builds) are binding and override any older wording. Becomes binding as a whole after an Opus
adversarial review; amendments go into a "Review amendments" section that overrides the text where they conflict (the pattern of
`docs/JIT_TIER15.md` section 12). Scope: `docs/JIT_PLAN.md` J5 (user decision 2026-10-29: a LuaJIT-style trace
compiler, built after the J4 gate T13, with a prototype gate measured against the J4 region numbers before the full
build). Constraints carried over unchanged: E1-E4, P1, C1, G1, F1 (plan section 3), the J4 invariants T-EXACT ..
T-5.5 (`docs/JIT_TIER15.md` 4.10) where they concern tier-1 code, the Sonnet-builder execution model (plan section 13),
and CLAUDE.md's memory/timeout rules for every test command.

Evidence tags as in the other JIT docs: **[M]** measured (with the source), **[V]** verified in the code at `da7d645`,
**[D]** design decision, **[E]** estimate with the method stated, **[J]** judgment (an estimate without a derivation), **[?]** uncertain: an experiment in the prototype
decides (section 9.9 lists them as X1..X11). v1.3 numbers marked [M] T13 are from `docs/bench-t13-j4.txt` (J4
measurement at `43ade31`, identical code at `89963c6`); v1.3 code facts were re-checked at `89963c6`.

Rules of this document are numbered **TR1..TR32** (TR29-TR31 added by the decisions, TR32 by v1.3) (with sub-rules such as TR3a, TR6b) (index in section 12). A builder quotes the rules verbatim in its
brief; a change that would violate one stops and reports.

## 0. Summary

- **What a trace compiler adds over J4 regions.** Regions (tier 1.5) are method-level: one prototype, real control flow,
  frames identical to the interpreter's, calls are opaque barrier ops. A trace is a recorded straight-line path through
  one loop iteration *across function boundaries*, in SSA form, with every observed type and callee speculated by a
  guard. That enables what regions cannot do: **inline Lua calls**, forward table loads/stores across iterations
  (LuaJIT's `-O-fwd`), hoist invariant guards and heap loads by loop peeling, and compile only the hot path of branchy
  code (side traces for the rest).
- **Honest expectation [E].** The J4 regions already reach LuaJIT's level on pure register loops (measured: mandelbrot
  about 0.12x of the reference against LuaJIT 0.11x; loops_arith about 0.18x against LuaJIT's 0.42x on its variant,
  section 1.3). Traces gain there little or nothing, and lose on short inner loops and unpredictable branches unless
  linking and side traces are complete. The real J5 wins are concentrated: spectral_norm (a Lua call in the inner loop;
  regions measured 0.606x, no gain over tier 1; trace 0.15-0.25x), nbody (field forwarding: about 25% in LuaJIT's own
  ablation), and the method-call programs (oop_methods, closures). Projection (section 1.5; [J] factors on the
  measured T13 numbers): the numeric-7 geomean (the `bench_cycles.py` set the plan's targets use) from 0.275 [M] to
  **0.19-0.24**; numeric-9 (+ heapsort, queens) from 0.316 [M] to **0.23-0.29**; all-25 from 0.521 [M] to
  **0.45-0.50**. The plan's 0.25 numeric stretch target lies above the whole numeric-7 range, but the range is
  judgment and depends on nbody and spectral_norm, which the prototype does not prove (nbody is not traced there); no
  probability is claimed. Allocation sinking
  (in scope by decision D1) adds little on the 25 programs (binary_trees' allocations escape; LuaJIT's `-O-sink` showed
  no visible effect); its value is in allocation-heavy user loops (temporary vectors, tuples), and its cost is the
  GC-observable divergence policy of section 6.8.
- **Design in one paragraph [D].** Hot loop heads in tier-1 code dispatch through a per-head *data cell* (no code
  patching). When a head's counter fires, tier-1 code leaves to the interpreter, which runs one iteration in its
  *hooked copy* with a recorder callback before every instruction (rn-core change only in cold paths, cfg feature
  `jit_trace`; the JIT-off build is byte-identical, F1). The recorder emits typed SSA IR with snapshots, checks every
  recorded instruction against the interpreter's real result (lockstep), and stops when the loop closes. The IR is
  optimised (fold, CSE, guard elimination, peeling-based LICM, array forwarding), register-allocated by a reverse
  linear scan, and lowered to the **existing micro-op IR**, so the x86 emitter, the code arena and the Miri emulator are
  reused unchanged. **Nothing that can run Lua code or a GC step ever happens on a trace** (TR1): calls to C functions,
  metamethod functions, allocation with a pending GC step, finalizers, hooks, errors and yields all happen in tier-1
  code after an exit. Every exit restores the exact interpreter state (including inlined frames) and continues in the
  **tier-1 code of the same instruction**, so tier 1 is the deopt target, as in J4.
- **Decisions (section 13).** D1 allocation sinking is part of J5 (a late task with its own prototype check, so the
  core is not blocked; GC-observable values may differ from JIT-off within the bounds of TR29, kill switch
  `RUSTYNAIL_JIT_TRACE_OPT=-sink`). D2 Lua 5.4 traces are part of J5 (same crate, `cfg(rustynail_lua54)`, TR30). D3
  `RUSTYNAIL_JIT_TRACE` defaults to **on** whenever the `jit` cargo feature is compiled in (the feature itself stays
  off by default; the JIT-off build stays byte-identical), with `RUSTYNAIL_JIT_TRACE=off` as the documented off switch;
  ownership is trace first, J4 region after blacklisting (TR19). The 24-hour fuzz campaign is run by the user after
  J5 is complete; the builders run the shipping gates of section 9.11.
- **Prototype gate (P).** Integer and float numeric `for`/`while` loops, array-part tables only, inlined Lua calls and
  a few `math` intrinsics, no side traces and no linking, measured against J4 (the finished J4 regions, run from the
  J5 branch with `TRACE=off` and verified against main@89963c6) with benchmark files frozen before implementation
  (P0). Go needs spectral_norm and matmul individually, a non-inlining geomean over loops where forwarding, LICM and
  peeling apply, and the whole gate set to pass their bounds, and type-unstable loops must not degrade (section 1.6); a
  pass that comes only from inlining is a no-go for the full trace build (then leaf inlining into regions is built
  instead).
- **Effort [J].** Prototype: 16 builder tasks (5 of them L or M-L), about 1.3-1.8x the size of the whole of J4. Full
  build including 5.4 and sinking: about 14 more tasks, 2.4-3.0x J4. Total J5 about 3.7-4.8x J4 (section 10.5).

## 1. Goals, non-goals, expected wins

### 1.1 Goals

1. Faster hot loops than J4 regions where the trace mechanisms apply (1.3), never slower than tier 1 beyond the +-5%
   band on any of the 25 programs (plan section 1 rule), and no change of observable behaviour: output, errors, line
   info, hooks, `debug.*`, coroutines, memory limits, the instruction limit (C1). GC observables
   (`collectgarbage("count")`, finalizer order and timing, weak-table clearing timing) are identical to JIT-off
   *except* where allocation sinking is active; there they, and the address-dependent outputs listed in TR29.2, may
   differ as TR29 states (decision D1; the address part is open question N1).
2. Every existing gate keeps working with `RUSTYNAIL_JIT_TRACE=always`: official suite 31/31 (with `-sink` for
   `gc.lua`/`gengc.lua`, 9.8), `tests/vm`, jit-diff, GC stress 1/2/VERIFY, stackmove, count budgets, the J4 oracle,
   ASan, Miri on the pure parts, for **both 5.5 and 5.4** (decision D2).
3. Shipping state (decision D3): `RUSTYNAIL_JIT_TRACE=on` is the runtime default of every build with the `jit` cargo
   feature, so traces must pass the shipping gates of 9.11 before J5 is complete.
4. Infrastructure reuse (section 11): micro-op IR, x86 emitter, emulator, code arena, tier-1 code as deopt target,
   T15 runtime plumbing (versions, `pcmap`, continuation cells), the J4 oracle and mutation framework.

### 1.2 Non-goals (J5)

- No trace ever calls a C function other than whitelisted pure helpers (TR1); natives end the trace (stitching through
  tier 1, full build).
- No inlining of metamethod *functions* (`__add`, `__index` functions, `__call`, `__eq` ...). Table-valued `__index`
  chains are a full-build item (TR16).
- No allocation sinking in the *prototype*; it is a late full-build task (F8) behind its own prototype check (6.8).
- No recursion beyond a small unroll (fib stays at tier-1 speed), no vararg callees, no `TFORCALL` generic `for` loops
  in the prototype (full build: `next`/`ipairs` fast paths as recorded builtins).
- No 5.4 traces in the *prototype* (5.5 only, like T-5.5). 5.4 is part of the J5 full build and gate (TR30, tasks
  F7a/F7b, decision D2).
- No machine-code patching (TR8), no background compilation (plan decision D7), no new runtime dependency (plan
  decisions D1/D6). (Plan decisions are prefixed "plan"; D1-D5 without prefix are this document's, section 13.)

### 1.3 Where traces beat J4 regions (mechanism by mechanism)

Region numbers are the J4 measurement T13 ([M] `docs/bench-t13-j4.txt`, `docs/JIT_TIER15.md` section 23: min of 9
interleaved runs, cycles). Ratios are cycles vs the reference PUC 5.5.

| mechanism | regions (J4) | traces | programs it matters for | expected effect [E] |
|---|---|---|---|---|
| **Call inlining** (Lua callee recorded inline, guarded by callee proto identity) | impossible: CALL is a barrier op, flush + flat call + resume stub + re-guard (spectral_norm: 47% of JIT instructions in the leaf `A(i,j)` and its call/return, [M] T15 1.2) | callee body becomes 5-10 IR instructions, no frame push, no flush | spectral_norm, oop_methods/closures (full build: SELF/hash), queens (partial) | spectral_norm 0.606 -> 0.15-0.25; oop_methods 0.770 -> 0.46-0.58 (full build) |
| **Store-to-load forwarding and load CSE across iterations** | probe CSE within a barrier-free stretch only; no alias analysis, no value forwarding (J4 decision 9.4) | per-class alias analysis (TR13), forwarding across the back-edge via peeling (TR24e) | nbody (fields), array recurrences (`t[i] = t[i-1] + x`), upvalue accumulators | nbody 0.266 -> 0.16-0.21 (LuaJIT's `-O-fwd` ablation: 25% [M] design notes 11.1); recurrences through memory: the store-to-load latency leaves the loop-carried chain; 0 on the numeric programs that keep accumulators in locals |
| **LICM by peeling** (invariant guards, header loads, upvalue loads and invariant arithmetic run once) | stub guards once per entry; header hoist "only if registers are left" (J4 5.1); upvalue hoist only when the region has no `SETUPVAL` at all (J4 M3); **no arithmetic LICM** (J4 decision 9.3: no peeling) | all invariant instructions fold into the pre-roll | matmul, heapsort, queens, sieve inner loops; loops with invariant `sqrt`/division subexpressions | 3-10% on table loops; large where an invariant divide/sqrt is in the loop |
| **Redundant guard / bounds / lookup elimination** | per-access bounds check and tag check (merged with nil test) | CSE of guards, ABC hoisting with the for-loop range (TR12), one bounds check per (table, index) per iteration | matmul (`b[k][j]`), fannkuch | 0-5% (J4 measured the per-access check at 1 compare + 1 predicted branch, decision 9.2) |
| **Allocation sinking** | none | possible, but changes GC observables (6.8) | binary_trees: no (allocations escape); LuaJIT `-O-sink`: no visible effect [M] | about 0 on the 25 programs; gains on allocation-heavy user loops (temporaries that die in the iteration); built late (F8), GC observables bounded by TR29 |
| **Side traces for branchy code** | real control flow, no cost | the hot path is straight-line; other paths are side traces linked by cells | fannkuch (44 of 48 LuaJIT traces are side traces [M]), queens (49/51) | *negative* in the prototype (no side traces), about neutral in the full build |
| **Type specialisation of values loaded from the heap** | `prof[pc]` last-tag speculation, def guards | the recorder sees the value | all | similar; traces respecialise per path, regions per site |

### 1.4 Where traces do not beat regions (or lose)

- **Pure register loops.** Regions already keep ints and floats in registers with entry guards once per nest; J4 is at
  LuaJIT's level on mandelbrot and well below it on loops_arith (whose LuaJIT variant pays `math.floor`). Expect
  0.9-1.0x of J4.
- **Short inner loops and loop nests.** A region covers the whole nest (nbody's inner loop averages 2.5 iterations,
  fannkuch's at most 10, [M] J4 1.4). A trace covers one loop; every inner-loop execution costs a trace entry
  (typechecks of the live-ins, about 5-15 cycles) and a loop-exit side exit (5-20 cycles inline, section 7.6) unless
  the outer loop links into the inner trace (full build, section 7.8). mandelbrot's inner `while` runs about 20
  iterations per pixel on average [?], so the prototype pays entry plus exit per pixel (estimated +10-20% vs J4; X3).
- **Unpredictable branches** (`if k % 3 == 0` in loops_arith loop 3): each off-trace iteration exits to tier 1 in the
  prototype (+20-60 cycles); side traces in the full build reduce this to a cell jump plus an entry.
- **Divider- or helper-bound loops** (loops_arith loop 3, sieve's insertion loop): the bottleneck is not the code the
  trace improves.
- **Recursion** (fib, binary_trees): call unroll limits; LuaJIT also aborts there ("call unroll limit reached", [M]
  design notes 11.3). fib stays at tier-1 speed.
- **Runtime-bound programs** (strings, GC, pcall, coroutines, json, load_big): unchanged, as for every tier.
- **Compile cost and code size.** A trace is recorded in the slow hooked interpreter, optimised and allocated: about
  3-10x the work of a region compile per loop [E], and each trace carries a pre-roll and a loop copy (about 2x the code
  of one iteration) plus exit stubs. Short scripts must not get slower (plan rule: official suite and `lua -e ""`
  within the band). Measured in the prototype (X7).

### 1.5 Per-program projection

Labels per cell: **[M]** measured (T13, `docs/bench-t13-j4.txt`: min of 9 interleaved runs, cycles vs PUC 5.5;
LuaJIT uses the `bench/luajit` variants where present); **[J]** judgment: a base value times a factor chosen from the
mechanisms of 1.3/1.4, with the base and the basis named in the last column. The base is J4 except where J4 gained
nothing over tier 1 (spectral_norm, queens): there the factor applies to tier 1, because the trace replaces tier-1 code,
not a region. Basis "LuaJIT ablation" means the factor is transplanted from LuaJIT's `-O-fwd` effect (about 25%, [M]
design notes 11.1) on a VM with a different value layout and is therefore weak evidence. v1.2's J4 *projections* were
too optimistic for spectral_norm (projected 0.48, measured 0.606), heapsort (0.36 / 0.445) and queens (0.40 / 0.602,
slower than tier 1's 0.575); v1.3 recomputes every row from the measured values, keeping v1.2's factors except where the
base changed. The prototype runs with regions off (TR19), so non-traced loops run at tier-1 speed there; a prototype
geomean over the numeric set would therefore be meaningless and is not given.

| program | tier 1 [M] | J4 [M] | J5 prototype [J] | J5 full [J] | LuaJIT [M] | base and basis of the J5 factor |
|---|---|---|---|---|---|---|
| loops_arith | 0.479 | 0.184 | 0.17-0.20 | 0.17-0.20 | 0.420 | J4 x 0.94-1.11 / 0.94-1.06; none applies (divider floor in loop 3; branchy) |
| mandelbrot | 0.591 | 0.123 | 0.12-0.15 | 0.10-0.12 | 0.112 | J4 x 1.0-1.25 / 0.83-1.0; entry+exit per pixel (prototype); nest linking (full) |
| matmul | 0.372 | 0.196 | 0.17-0.21 | 0.16-0.20 | 0.081 | J4 x 0.85-1.05 / 0.8-1.0; peeling of invariant header loads, ABC (<= 10%) |
| spectral_norm | 0.601 | 0.606 | 0.18-0.30 | 0.15-0.25 | 0.037 | **tier 1** x 0.3-0.5 / 0.25-0.42; 47% of JIT instructions in `A` + call/return ([M] T15 1.2) removed by inlining; needs the type-stability rules of 3.3 (both inner loops are type-unstable at entry, 1.6 set (d)) |
| nbody | 0.588 | 0.266 | not traced (hash fields) | 0.16-0.21 | 0.091 | J4 x 0.6-0.8; LuaJIT ablation (field forwarding) |
| fannkuch | 0.421 | 0.299 | 0.30-0.40 | 0.27-0.33 | 0.265 | J4 x 1.0-1.33 / 0.9-1.1; side traces needed; branchy |
| heapsort | 0.558 | 0.445 | 0.41-0.52 | 0.37-0.45 | 0.321 | J4 x 0.92-1.17 / 0.83-1.0; peeling; linking into the inlined `sift` loop |
| queens | 0.575 | 0.602 | 0.55-0.66 | 0.50-0.60 | 0.442 | **tier 1** x 0.95-1.15 / 0.87-1.04; recursive `place` aborts every recording through the call (`Recursion`), only the skip path traces; side traces (full) |
| sieve | 0.757 | 0.547 | 0.52-0.62 | 0.50-0.56 | 0.360 | J4 x 0.95-1.13 / 0.91-1.02; none applies (insertion loop stays in tier 1 in the prototype: `Insert`) |
| **numeric-9 geomean** (the 9 rows above) | 0.538 | 0.316 | n/a | **0.23-0.29** | 0.177 | computed from the rows |
| **numeric-7 geomean** (`bench_cycles.py` NUMERIC: the 9 without heapsort, queens; the plan's targets) | 0.531 | 0.275 | n/a | **0.19-0.24** | 0.143 | computed from the rows |
| fib (not in the numeric sets) | 0.532 | 0.532 | 0.53 | 0.53 | 0.113 | none (recursion) |
| all-25 geomean | 0.632 | 0.521 | n/a | **0.45-0.50** | 0.252 | numeric-9 factor (0.72-0.90)^(9/25) on J4's 0.521, times (oop_methods 0.6-0.75 x closures 0.8-1.0)^(1/25) [J] |

v1.2 printed 0.285 and 0.14 as the J4 and LuaJIT "numeric-9" geomeans; those were projections and the numeric-7
LuaJIT value respectively. The measured numeric-9 values are 0.316 (J4) and 0.177 (LuaJIT).

### 1.6 The prototype gate (P-gate)

**Scope of the prototype (exactly).** Root traces of innermost loops only (no side traces, no trace-to-trace linking,
no stitching), recorded opcodes: `MOVE LOADI LOADF LOADK(number/short string) LOADFALSE LFALSESKIP LOADTRUE LOADNIL`,
integer and float arithmetic and bitwise ops with their K/I forms (MMBIN followers skipped as in tier 1),
`UNM BNOT NOT`, compares + JMP followers, `TEST TESTSET JMP`, `FORLOOP` (int and float, 5.5), backward `JMP`
(while/repeat), `GETUPVAL` (and `SETUPVAL` of non-collectable values), `GETI GETTABLE SETI SETTABLE` on the array part
with integer (or integral float) keys and tables without metatable, `CALL`/`RETURN0`/`RETURN1`/`RETURN` (fixed counts)
to Lua closures (inlined, depth <= 4, no recursion, no varargs), and `CALL` of the intrinsics `math.sqrt abs floor ceil
min max` (2 arguments) whose callee value comes from an upvalue or a register. Everything else aborts the recording
(4.6) and the loop stays in tier 1 (or in its J4 region, section 3.2).

**Benchmarks are frozen before implementation.** P0 commits `bench/jit-trace/*.lua` (sets b, b', c, d below and
`short_inner` for X3) and records N and expected output for every gate program, including the two of set (a), which
stay in `bench/programs`; P9 may not add or change gate programs (new programs can only be informational). Every
set-(b) program is a function whose loop is called repeatedly from the main chunk (the main-chunk loop around the call
is not traceable in the prototype: `InnerLoop`), with arrays of at most 10^4 elements (CLAUDE.md memory rule).

**Gate set.** (a) **spectral_norm** and **matmul** (real programs, judged individually).
(b) the non-inlining micro-benchmarks, each chosen so that a trace mechanism J4 regions do not have applies (v1.3,
decision D6: v1.2's set was half pure register loops, where traces are projected at 0.9-1.0x J4, 1.4):

| program | loop body (one line) | trace mechanism vs J4 | projection (trace / J4) [E] |
|---|---|---|---|
| `licm_expr` | `s = s + t[i] * (sqrt(a * a + b * b) / c)`, `sqrt` an upvalue holding `math.sqrt`, `a b c` invariant float locals | peeling hoists the invariant `fmul fadd FSQRT fdiv` and the callee-identity guard (6.4); J4 has no arithmetic LICM (J4 decision 9.3) and runs a sqrt and a divide per iteration, which share the divider and bound its throughput | 0.45-0.70 (J4 about 8-10 cycles per iteration on the divider; trace bound by the 4-cycle `fadd` chain) |
| `array_rec` | `t[i] = t[i - 1] * c + x` (floats) | the body's `t[i-1]` load forwards from the previous iteration's `t[i]` store (TR13.3 must-alias after folding `(i+1)-1`, TR24e PHI); J4 reloads through memory, so store-to-load latency (about 5 cycles) sits on the `fmul+fadd` chain | 0.60-0.80 |
| `upval_acc` | `acc = acc + t[i]` (integers), `acc` an upvalue of the looping function | `UREF` hoisted (range guard once), the body's `ULOAD` forwards from the previous `USTORE` through the same invariant `UREF` (TR13.5 must), `acc` stays in a register; J4 hoists no upvalue in a region containing `SETUPVAL` (J4 M3) and forwards nothing: closure -> `UpVal` -> value reload and a store-to-load chain per iteration | 0.40-0.65 |
| `int_rec2` | `t[i] = (t[i - 1] + t[i - 2]) & 0xFFFFFF` | two forwarded values per iteration: `t[i-1]` from the previous store, `t[i-2]` by load CSE across a store proven distinct by its index (TR13.3 "no"); both become PHIs; J4 reloads both with the store-to-load latency on the 2-op integer chain | 0.45-0.70 |

Geomean of the projections 0.47-0.71, so criterion 4 (<= 0.90) is reachable without inlining with margin for the
uncertainty of [E]; a set that misses it means the trace mechanisms themselves underperform. (b') **informational**
pure register and plain array loops, measured and explained but not in the gate geomean: `int_sum`, `float_poly`,
`float_loop` (the J0 oracle programs), `dot` (two arrays), `upval_inv` (invariant upvalue table read in the loop, no
`SETUPVAL`: J4 hoists it too).
(c) the inlining micro-benchmarks `inline_leaf` (a loop calling a 3-line local function) and `inline_2` (two levels).
(d) **type-stability programs** (finding v1.3-1, both patterns of spectral_norm), each with a stable twin that differs
only in the type: `type_entry` (nested loops, the inner accumulator starts `local a = 0` (integer) and becomes a float
after one iteration; twin `type_entry_stable` starts `local a = 0.0`) and `type_late` (the looping function is first
called on an integer array, which makes its inner loop hot and recorded, and afterwards only on float arrays; twin
`type_late_stable` uses float arrays throughout).
**Informational set** (measured and explained): loops_arith, mandelbrot, fannkuch, heapsort, queens, sieve (branchy,
nested or helper-bound: they need side traces and linking), and (b').

**Baseline (precise, v1.3).** J4 is finished (T13 measured at `43ade31`; main@`89963c6` has the same code). Because a
J4 build has no `trace` feature, one binary for every column is impossible; the baseline is the **J5 branch built with
`--features jit,trace`**: J4 column `RUSTYNAIL_JIT=on RUSTYNAIL_JIT_T15=on RUSTYNAIL_JIT_TRACE=off`, prototype column
`RUSTYNAIL_JIT=on RUSTYNAIL_JIT_T15=off RUSTYNAIL_JIT_TRACE=on`, tier-1 column `T15=off TRACE=off`. The J4 column is
**verified** against a `--features jit` build of main@89963c6 before any ratio is used: `scripts/jit-dump-identity.sh`
(tier-1 and region code identical with `TRACE=off`) and, in the same interleaved session, every gate and informational
program within the +-5% band of main@89963c6 with `T15=on`; a program outside the band is explained before P9
reports. Method: the **T13 method**, so the T13 table stays a valid reference for the 25 programs: `scripts/bench-cycles.sh`,
`RUNS=9`, **min** of 9 cycles:u, CPU 4, `RUSTYNAIL_SEED=0`, interleaved, +-5% band (v1.2's "median of >= 7" is
withdrawn); micro-benchmarks through the same script with `PROGDIR=bench/jit-trace` (option added in P0). J4 T9
(function regions) and T10 were skipped, so no region-inlining comparison exists.

**GO** iff all of:
1. correctness: section 9 gates for the prototype (official suite with `TRACE=always`, jit-diff 2000 seeds in the
   trace modes with `JITDIFF_ENTERS=1` including the trace-fraction check of 9.8, the fetch+insns identity and the
   no-overshoot rule (9.5), the trace oracle, mutation checks caught, GC stress 1/2, stackmove);
2. spectral_norm (trace / J4) <= **0.70**;
3. matmul (trace / J4) <= **0.95**;
4. non-inlining micro geomean (set b) (trace / J4) <= **0.90**, so forwarding, peeling/LICM and register allocation
   must contribute on their own;
5. whole gate set (a+b+c) geomean <= **0.85**; no gate program of (a), (b), (c) above 1.05 x J4; no program of the 25
   above 1.05 x tier 1; every set-(d) program <= 1.05 x its stable twin (both with `TRACE=on`), with its inner head
   ending `Traced` (not `Black`) after at most 3 re-records (VERBOSE);
6. informational set: geomean (trace / J4) <= 1.15 **and** every program <= 1.30; every program above 1.05 is
   attributed by the VERBOSE profile to exits/entries/aborts that a named full-build task removes (side traces F2a,
   linking F2b, hash part F1); an unattributed excess fails the criterion;
7. cost: trace record + compile time <= 2% of the cycles of every program, short-script suite within the band, code
   bytes of traces <= 1.5 x the tier-1 code bytes of the same program (`jit_stats.code_bytes` of `TRACE=off T15=off`;
   J4 bytes are not used, they include retired copies, J4 advisory A2).

**NO-GO** if any of 1-5 fails, or 6 fails without a fix inside the prototype. If 2 passes but 4 fails, the gain is
inlining only: **no-go for the full trace build**; the costed alternative **leaf inlining into regions** (inline
loop-free, call-free Lua callees with a guarded proto identity as a J4 region act, 2-3 builder tasks [J]) is built
instead, and the trace prototype is archived on its branch (decision D4).

**Placement.** After J4 (done), before any full-build task (section 10).

## 2. Architecture

```
tier-1 code (TRACE on)                    interpreter (hooked copy)                 trace compiler (rn-jit::tr)
+---------------------------+  ctr<0     +------------------------------+  done   +---------------------------+
| loop head H: dispatch     | ---------> | recorder callback before     | ------> | IR (SSA) + snapshots      |
|   jmp [cell[H]]           |  exit at H | every instruction; lockstep  |         | fold/CSE/guards/peel/fwd  |
| cell[H] = count stub,     |            | check of every result        |         | reverse linear-scan RA    |
|   trace entry or pcl[H]   |            +------------------------------+         | lower to micro-op Program |
+---------------------------+                         ^ abort: back to fast copy   | x86 emit / emulator       |
         ^    | enter (cell)                                                         +-------------+-------------+
         |    v                                                                                    | install:
   +-----+--------------------------------------------+                                            | cell[H] = entry
   | trace: entry checks, pre-roll, LOOP, body, PHIs  |  exit k: inline stub (frameless) or        |
   | exits: restore snapshot -> jmp tier-1 label(P)   |  shared exit routine -> Rust restore       |
   +--------------------------------------------------+  (inlined frames) -> tier-1 label(P)       |
```

Components (all in `crates/rn-jit/src/tr/`, feature `trace` of rn-jit, which enables rn-core `jit_trace`):
`ir.rs` (SSA IR types, buffers, constants), `fold.rs` (fold + CSE), `interp.rs` (IR reference interpreter, Miri),
`record.rs` (recorder), `snap.rs` (snapshots, exit maps), `exit.rs` (Rust restore), `opt_loop.rs` (peeling, PHIs),
`opt_mem.rs` (alias analysis, forwarding, ABC), `ra.rs` (register allocation), `asm.rs` (IR -> micro-ops, stubs),
`cache.rs` (trace cache, cells, deps, flush, penalties), `print.rs` (dumps), `mutants.rs` (feature `tr_mutants`: the
frozen name registry; each mutant's injection point lives in the file of the task that owns that code, 10.1), and
`mod.rs` (frozen shared types and `TraceOpts` from P0; the pipeline driver record -> optimise -> RA -> asm -> place ->
install, "Finish" of 4.2, added by PI).

## 3. Hot-spot detection and interaction with tier 1 / tier 1.5

### 3.1 Counters and dispatch cells

- **TR20 (dispatch, v1.3).** With `RUSTYNAIL_JIT_TRACE != off`, tier-1 lowering of a prototype routes every edge into
  each loop head `H` that is a trace candidate (every back-edge, the fall-through into `H`, the resume-table entry of
  `H`) through a per-head **dispatch jump** `D_H: Mov V7, &cell[H]; Ld V7, [V7]; JmpR V7` placed *before* `H`'s
  `CountSub`. No counter is decremented on this path (v1.2's per-back-edge `ctr[H] -= 1` was a memory load/sub/store
  on every iteration forever, including `Black` heads; J0 measured up to 31% for such a counter, [M] JIT_ORACLE 2).
  `cell[H]` holds exactly one of, by head state (3.3): **Warm/Recording**: the address of `H`'s out-of-line
  **counting stub** `C_H` in the same tier-1 code version (`Ld/Sub/St ctr[H]`; if `ctr[H] < 0` `CallH
  jit_tr_hot(head)` (3.1, below); then `Mov V7, pcl[H]; JmpR V7`); **Traced**: the entry of the installed root trace;
  **Black**: `pcl[H]` of the current version (full build with the nest region-eligible: J4's warm-up stub `W_H`,
  TR19b). `Traced` and `Black` heads therefore pay only the load and the predicted indirect jump. **`pcl[H]` (and
  `W_H`) are downstream of `D_H`**: they are reached from `D_H` through the cell or by a fall-through that does not pass
  `D_H` again, and trace entry failures and `S_H` exits land on `pcl[H]`, never on `D_H` (otherwise a failed entry
  would jump back into the trace forever). With `TRACE=off` tier-1 code is byte-identical to today
  (`scripts/jit-dump-identity.sh`). [D]
- Candidates are all loop heads of the prototype found by `lower`'s back-edge rules (`Lowered::backedges`, [V]);
  innermost loops become hot first by construction. `ctr[H]` and `cell[H]` live in a boxed, address-stable,
  never-reallocated per-prototype `TrProto` (the T3 `T15Data` discipline, J4-DATA: `Cell`s only, fixed-length box,
  freed with the descriptor).
- **Threshold.** `RUSTYNAIL_JIT_TRACE_HOT=loop[,exit]`, defaults 256 back-edges per head and 16 exits per exit site
  [?] (LuaJIT: about 56 and 10, tuned for its interpreter; ours runs tier 1 meanwhile, so higher is fine; X7 decides).
  `always`: 1 and 2, no random penalty bits (deterministic tests).
- `jit_tr_hot(thread, ci, head)` (extern "C", J-HELP contract) decides: head state `Warm` and no recording active in
  this state -> arm the recorder (4.2) and return 1; otherwise re-arm the counter and return 0. The counting stub on 1: jumps
  to tier-1's `exit_at(H)` label (savedpc = `H`, `Continue`, no refund: `H`'s count has not been paid). The helper
  has set `ci.trap = 1`; on `Continue` the fast copy's `'frame` head falls through to its existing `if ci.trap != 0 &&
  fast_trap(st, ci)` test right after the JIT entry ([V] vm_unsafe.rs:551), and `fast_trap` (TR23.1) hands the frame to
  the hooked copy before `H` executes. No conversion of the exit code is needed.

**Function-entry heads (P-E2).** A prototype without any back-edge that is not vararg, contains a `CALL` and no `TBC` gets
one more trace candidate: **pc 0**, the function-entry head (`TrProto::fn_head`, opt-in: limit `fnroot=1`, default 0 since the P-E2 merge). `D_0` is
placed in front of pc 0 like any `D_H` (the resume entry `entry0` that flat calls and `rt::enter` use is the dispatch jump),
so every entry into the function, from tier-1 callers (flat calls), from the interpreter and from Rust callbacks, counts
down `ctr[0]` while the head is `Warm`; `jit_tr_hot` arms a recording in the fresh callee frame (`exit_at(0)`, the same stub
exit as a loop head). The recording ends, instead of at a back-edge, **before the `RETURN` / `TAILCALL` of the root
frame**: the always-taken exit of a stitching trace (`Rec::fn_end` -> `end_exit`), after at least `fnmin` (16) bytecodes
(`FnShort`, its own penalty class like `LeaveLoop`: data, not a fault). Callees (including recursive ones, `unroll` /
`maxdepth`) are inlined as in a loop recording; beyond the unroll limit `stitch` ends the recording with the same exit. The
installed trace is a straight-line root (no loop, entry checks and type-snapshot splitting as for a stitched root). Side traces
of such a root (exits in inlined frames get side traces with virtual frames since S1; for function-entry roots and the continuation / native-recursion traces since BUNDLE-H1, `fcfn`) end the same way (`SideInfo::no_link`: no link
target, `SideEnv::link_cell == 0`), and a side trace that would start at the final `RETURN` exit is refused (`LeaveFrame`):
that exit is taken by every entry. **Penalty:** every exit of a function root except the final one counts for the
ineffective-trace rule (`TraceIr::fn_root`, `exit::pre_roll_snaps`) with the ratio `FN_INEFF_RATIO_PCT = 20` of the entries
(a pre-roll exit of a loop root needs 90%): a function root whose entries mostly leave early is flushed, re-recorded up to
`MAX_RERECORD` times and then blacklisted. Exits that have a side trace attached bypass the exit handler, so a root with a
good side trace is not flushed by this rule.

### 3.2 Ownership of a loop (TR19)

**TR19 (one owner per head).** At any instant the iterations that start at a loop head `H` are run by exactly one of:
tier 1, a J4 region containing `H`, or the root trace of `H`. Prototype and prototype gate: the two optimised tiers are
never combined (`T15=on, TRACE=off` vs `T15=off, TRACE=on`). Full build (task TR-F5): **trace first, region
fallback**: while any head of a nest is `Recording` or `Traced`, the nest is not promoted to a region. **"Never got
hot" (v1.3 definition):** every head starts `Warm` (the initial state); a `Warm` head whose trace counter has not
fired is *cold*. A nest becomes eligible for J4 promotion when no head is `Recording` or `Traced` and at least one
head is `Black`, all others `Black` or cold; promotion sets every head of the nest `Black` for the tracer (a nest
already promoted to a region is never traced).
**TR19b (full build, F5; v1.3).** (a) *Nest rule:* if the outer head of a nest goes `Black` while an inner head is
`Traced` but not linked from an outer trace, the nest's traces are flushed (TR18, deferred), every head of the nest
is set `Black`, and the nest goes to J4 (otherwise the outer loop would run in tier 1 forever around a short inner
trace, e.g. mandelbrot). Risk: a spectral_norm-like nest whose inner trace inlines calls is better as a trace than as
a region; F5 measures this (X5) and may restrict the rule to inner traces without inlined calls, recorded in section 13.
(b) *Stub composition:* on one head the order is `D_H` (trace dispatch) -> cell -> {`C_H` (Warm: trace counter only),
trace entry (Traced), `W_H` (Black and nest region-eligible: J4's M7 warm-up/promote stub with J4's own counter),
`pcl[H]` (Black otherwise)}. Exactly one counter runs on a head at any time: the trace counter while `Warm`, J4's
warm-up counter only after `Black`. (c) *Code versions:* `cell[H]` lives in `TrProto` across tier-1 code versions but
its non-trace targets (`C_H`, `W_H`, `pcl[H]`) are labels of one version; the J4 version-install path (promotion,
respecialisation, retirement) rewrites every non-trace cell of the proto to the new version's corresponding label in
the same step that installs the version. Cells pointing to traces are unchanged (trace landings keep using the
version recorded at compile time, TR4), and a retired block referenced by any installed trace's landing labels is
never freed before the proto (J4-RETIRE already keeps retired blocks until `free_code`; F5 asserts it in `+verify`). `RUSTYNAIL_JIT_TRACE_PREF=trace|region`
switches the order for the experiment X5. **Decision D5 (follows D3):** the shipped default is trace first, region after
blacklisting; X5 on the 25 programs may only change the default if region-first is better by more than the +-5%
band on the all-25 geomean without a per-program regression beyond the band, and the change is recorded in section
13. Consequently, in `jit` builds the J4 regions' default (`RUSTYNAIL_JIT_T15`) becomes **on** together with traces,
acting only as the fallback owner of nests the tracer blacklisted (or never got hot as traces); `RUSTYNAIL_JIT_TRACE=off`
restores J4's behaviour exactly (regions own every hot nest as today). Exits of traces always land in tier-1 code,
never in region code (v1).

### 3.3 Penalties and blacklisting (TR21)

Per head (in `TrProto`, never in bytecode, plan s14 item 2):

| event | action |
|---|---|
| recording aborted | penalty `p = 2^k * 64 + rand4` back-edges before the next attempt (`k` = abort count, starts at 0); `k >= 6` -> `Black` (6 failures; LuaJIT about 11, but our failures cost more: a slow hooked iteration) |
| same abort reason twice with an *unchangeable* cause (NYI opcode, C function, vararg callee, ...) | `Black` immediately |
| compile refused for a transient reason (code cap, memory limit) | separate counter `kt`: penalty `2^kt * 64 + rand4`, `kt <= 10`; at `kt = 10` -> `Black`. Transient refusals do not increment `k` |
| root entry check failure (typechecks, extent, CallInfo spare) that is **not a free peel** | `entry_fail += 1`; at `entry_fail >= 64 && entry_fail * 4 > entry_ok`: prototype: flush the trace and re-record (`rerec += 1`); full build: side trace from exit 0 (7.9) |
| root entry check failure followed by a successful entry of `H` (a **free peel**: e.g. `local a = 0` before the loop while the trace expects the float the first iteration produces) | not counted |
| recording reaches `H` again with a slot whose type differs from its type at the start (`Unstable`, TR24c) | no penalty; `rerec += 1`; the counter of `H` is set to fire at the next back-edge, so the next recording starts from the types the finished iteration produced |
| **type-guard** exit hot (an `SLOAD`/`ALOAD`/`ULOAD` typecheck exit with count >= exit threshold) | prototype: flush the trace (deferred, TR18) and re-record at the next back-edge of `H` from the observed types (`rerec += 1`); full build: side trace (7.9) |
| other exit site hot (branch, bounds, loop-exit, callee-identity guards) | prototype: counted only (VERBOSE); full build: side trace (7.9) |
| `rerec > 3` | `Black` |

**Free peel (v1.3).** "Followed by a successful entry" is tracked per head: an entry failure path does `entry_fail +=
pending[H]; pending[H] = 1` (branch-free memory updates of `TrProto` words, all allocatable registers are free at
entry), a successful entry does `pending[H] = 0; entry_ok += 1`; the cold helper `jit_tr_entry_fail` is called only
when the threshold is reached, and it only queues the flush (the failure path runs in the trace's code block, TR18).
A failure is thus counted only when the next entry attempt of `H` fails too. (This approximates the reviewer's "next
back-edge of the same activation": the clearing entry may belong to another activation; that only delays counting.)
Patterns this must handle (spectral_norm, [V] `bench/programs/spectral_norm.lua`): (a) both inner loops start with
`local a = 0` (integer) and the trace is recorded at a back-edge where `a` is a float: one free-peel failure per inner
run, never counted; (b) `Av`'s inner loop becomes hot in the first call (N = 1000 > 256) while `u[i] = 1` holds integers,
so its `ALOAD` typecheck expects integers, and every later call passes float arrays: the typecheck exits at the first
iteration of every entry; the hot type-guard rule re-records it once with floats. Both are frozen as gate set (d) and as
twin programs (9.3).

`always`: penalties are deterministic (`rand4` from a hash of `(proto, pc)`). Penalty state (including `rerec`)
survives a flush of the trace but not the collection of the prototype.

### 3.4 Code lifetime, invalidation, retirement

- **TR7 (embedded addresses).** Trace code may embed as immediates only: (a) addresses of prototypes in the trace's
  dependency set `deps` (the root and every inlined proto), used for callee-identity compares and never dereferenced
  from trace code; (b) addresses of those prototypes' constants (interned short strings, the `k` array) and their tier-1
  code blocks (exit landing labels of the version current at compile time); (c) addresses of light C functions
  (intrinsic guards); (d) per-state data (budget word, spill/exit area, `INTERRUPT`) and per-trace/per-proto data boxes
  (cells, exit counters). No table, closure, metatable, userdata, upvalue or thread address is ever embedded.
- **TR8 (no code patching).** Machine code of a trace or of tier 1 is never modified after commit. Every redirect is a
  write to a data cell: `cell[H]` (tier-1 dispatch), `exit_cell[t][k]` (full build: parent exit -> side trace), and
  link cells. W^X therefore needs no extra `mprotect` for linking.
- **Dependencies.** Each `JitProto` of a proto in `deps` records the trace ids that depend on it; `free_code` (called
  from `free_proto`, [V] J-OWN) flushes those traces *before* the descriptor is freed. This closes the ABA hazard of
  TR7(a): a callee-identity guard can never compare against the address of a freed and reallocated prototype.
  **TR32 (dependency edges during recording, v1.3).** The edge from an inlined proto is registered **when the recorder inlines it**
  (against the active recording's id), not at install: between inlining and install a GC step (full build: allocating
  recorded instructions, F3) can free the callee proto after its closure was overwritten. `free_code` of a proto with an
  edge to the active recording aborts that recording (`Abort::DepFreed`) before the descriptor is freed. Trace and
  recording ids carry a **generation counter** (`TrId { slot, gen }`): an edge whose id is stale (its trace was already
  flushed, e.g. a root freed before its callee) is ignored, so no trace is flushed twice and no reused slot is flushed
  by mistake. `free_code` also removes the freed key from the active recording's `root`/`deps` (a descriptor compiled
  during the recording by TR4b can reuse the address; the recording then has no root and its abort penalises nothing).
  A freed dependency is transient: the abort carries **no penalty**. The recorder must abort when `register_dep`
  returns false (the callee was compiled while traces were off, or the edge could not be registered). Prototype: no recorded instruction allocates, so the abort path is unreachable there; P3b/P6 build the
  mechanism anyway (it is the same code), F3 adds the test that reaches it.
  Inlined callees therefore must have a descriptor: **TR4b**: the recorder compiles every inlined callee to tier 1 (in
  `always`-style, ignoring `worth_compiling`) before inlining it; a refused compile aborts the recording. (A compiled
  callee may still run interpreted under tier 1, e.g. `entry_ok = false` or `Kind::Stub` instructions; that moves
  instructions between the interpreter's fetch count and `Stats::insns`, which TR5 allows.)
- **TR4 (landing).** Every exit of a trace continues either at a tier-1 label of the innermost frame's prototype (the
  `pcl`/mid-block label of the snapshot pc in the code version recorded at compile time) or, for side traces and links
  (full build), at another trace's entry through a data cell. Exits never continue in the interpreter directly and
  never in region code; the interpreter is reached only through tier 1's own exits.
- **TR18 (flush safety).** A trace may be freed only when no activation of it exists. Because of TR1 (no Lua code, no
  GC, no helper that can reach the collector runs on a trace) and TR2 (single activation), this holds at every point
  where the interpreter, tier-1 code, a helper or the collector runs, *except inside the Rust exit handler*, which is
  called from trace code. Therefore: (a) the exit handler is reached through a per-arena **shared** exit routine (its
  `CallH` returns into shared code, never into a trace block); (b) a flush requested while an exit handler runs
  (cap pressure, re-record decisions, a dependency freed by a collection that runs *inside* the handler) is split:
  the **head transition happens at once** (`cell[H]` -> `C_H` with the counter re-armed, or `Black`; the dependency edges
  are dropped; the `TrId` becomes stale, so a repeated hot-exit or entry-failure report is a no-op and a trace is queued at
  most once) and only the **freeing of the block and the metadata** (`CodeArena::free`, the exit counters, the refund) is
  parked in a graveyard that is emptied at the next safe point (`jit_tr_hot`, the end of a recording, `rt::enter` before
  entering, state close). `free_code` is **not** a safe point (the collector can run inside a handler): it detaches the
  trace like any other flush and parks it when a handler is active. The re-record then starts at the head's next back-edge,
  whose `jit_tr_hot` is itself a safe point, so a call-free loop with a persistently failing trace queues one entry, not one
  per iteration.
- **Flush triggers:** a dependency proto freed (that trace and its side traces); code cap reached or more than
  `maxtrace` traces (flush all traces of the state, LuaJIT's policy, then a global penalty of 2^k x 1024 back-edges
  before recording resumes); `set_mode(Off)`; `RUSTYNAIL_JIT_TRACE` switched off for the state; re-record decisions
  (3.3). Flushing writes `cell[H]` = `C_H` (re-record pending, counter re-armed) or the `Black` target (TR20) of the
  current tier-1 version, then frees the code block (`CodeArena::free`;
  `verify` mode quarantines it, [V] mem.rs) and the metadata, and refunds the charges.
- **Tier-1 recompiles of a dependency proto** (J4 promotion/respecialisation of *another* nest) retire its old block,
  which stays mapped and correct until the proto is freed ([V] J4-RETIRE). Trace exits that land in the retired
  version are correct; the activation moves to the current version at its next back-edge dispatch (the dispatch stub
  reads the cell, and on its next `jit_tr_hot`/`promote` call the T3 version rule applies). The recompiled version
  must keep the same `cell[H]` boxes (they live in `TrProto`, not in the version) and the same `BlockSig` ([V] J4-SIG),
  so trace C1 accounting stays valid across versions; the version-install path rewrites every non-trace cell to the
  new version's labels and never frees a retired block that a trace landing references (TR19b(c), F5).
- No other invalidation is needed: every other assumption of a trace (types, callee identity, table shapes, upvalue
  pointers, metatable absence, hook state) is either re-checked by an entry/body guard every time it is used, or cannot
  change on a trace (TR1).

### 3.5 Charging and memory limits

As J3b/J4 ([V] J-CHARGE, J4-CHARGE): recording buffers (IR, snapshots, maps; capped at 4000 IR instructions x 16 B +
500 snapshots) are charged as transient bytes with `Mem::charge_ext` when recording starts and refunded when it ends;
trace code through `place_code` (pages up front, refund of the unused part); trace metadata (snapshots, exit maps,
cells, counters, deps) as descriptor bytes, refunded on flush. A refused charge aborts the recording with a transient
penalty (3.3). `collectgarbage("count")` and GC debt never see JIT memory (D8).

### 3.6 Hooks, traps, `INTERRUPT`

- Compiled code (tier 1, regions, traces) is never entered while a hook is installed ([V] J-ENTRY). A trace is entered
  only from tier-1 code (dispatch cells), so the same holds for it.
- A hook can be installed while a trace runs only asynchronously (another OS thread, a signal handler): TR1 excludes
  every synchronous path. **TR22 (polls).** Every trace polls `ci.trap` (of the root frame `r13`; `settraps` sets it on
  every frame, [V] tier-1 relies on it) and `INTERRUPT` once per loop iteration (first instruction after `LOOP`, with
  the loop-head snapshot `S_H`, TR27b) and before every link jump (full build); a poll failure exits with the tier-1 `exit_trap(H)` semantics (savedpc = `H`,
  `trap = 1`, `Continue`). **P-L: one word.** The trace tests the single byte `rn_core::state::TRACE_POLL`, which `request_interrupt` sets
  (after `INTERRUPT`) and `State::set_hook` with a nonzero mask sets (after the `trap` stores of the frames); the poll exit (`EXIT_POLL` generic stub)
  restores the snapshot, clears the word, sets `ci.trap = 1` and leaves for the interpreter at the loop head (landing address 0), which then looks at
  `INTERRUPT` and the hooks. Flag cleared before the state it announces is examined, set after the state changed: no event is lost, a stale flag
  costs one spurious exit. The interpreter, not tier 1, takes over, so no trace can be entered between the clear and the examination. See "P-L decisions". Inlined calls need no poll (tier 1 polls at CALL/RETURN only for asynchronous events,
  which are allowed to be noticed late, [V] JIT_DESIGN 14 "asynchronous hooks fire late").
- `rec_clear_traps` (TR23.4) writes `trap` of frames of the recording thread; a `sethook` from another OS thread can
  race with it (same class as the reference's `settraps` race: both are plain stores, the worst case is a hook noticed
  one transition late).
- `debug.sethook`, `debug.setlocal`, `debug.upvaluejoin`, `debug.setupvalue`, `setmetatable` can only run in tier-1
  code or the interpreter (they are C functions or are reached from Lua code); their effect on a later trace entry is
  caught by the entry and body guards (types, callee identity, metatable absence, upvalue pointer ranges).

### 3.7 Instruction counting (C1) across a trace

C1's purpose is that compiled code never runs past the budget it was entered with, that budget exits land where tier
1's own budget exits would leave a consistent state, and that the count of executed instructions is exact. Which
instructions run in the interpreter and which in compiled code legitimately differs between modes: an iteration that
is recorded runs interpreted (in the hooked copy), an aborted recording too, and an inlined callee that tier 1 would
run in the interpreter (`entry_ok = false`, `Kind::Stub` instructions) runs compiled on the trace. **Only the sum of
interpreter fetches and compiled instructions is mode-independent**, and the point at which a small per-entry budget
runs out depends on where the compiled stays start. TR5 is stated accordingly.

Definitions: *fetches(P)* = instructions the interpreter fetches executing the recorded path up to (not including)
`P` (MMBIN/JMP/EXTRAARG followers subsumed by their leader are not fetched, exactly as the interpreter skips them).
Tier 1's prepaid model ([V] `Lowered::sig`: `head`, `cnt_at`, `rem`; `lower.rs` `Kind`): at its pre-count label of a
block head `P` tier 1 holds `budget = B - fetched`; at a mid-block label of `P` it holds `budget = B - fetched -
rem[P]` (the rest of the block, `P` included, was prepaid at the head), where `B` is the budget at entry and
`fetched` the instructions it executed so far.

- **TR5 (C1 for traces).**
  1. The recorder computes `fetches(P)` for every snapshot pc on the recorded path and the per-segment total `n` (one
     iteration of the pre-roll, one of the body; full build: each side trace). It uses each prototype's `Lowered` (of
     the code version recorded in the trace) only to classify landing labels (head or mid-block) and to read `rem`; no
     other count logic exists.
  2. A segment charges `n` once at its start: `if budget < n { exit (snapshot of the segment start) } ; budget -= n`.
     Tier 1 running the same path charges each block's `cnt_at` at its head and self-counting instructions after their
     guards; every charge is `<=` the fetches of the path up to the end of that block, and the last one reaches `n`,
     so tier 1 would complete the segment without a budget exit iff `budget >= n`.
  3. The budget is an SSA value (loop-carried, PHI), loaded from `[r15]` at entry and stored at every exit and link.
  4. An exit at `P` (re-executed in tier 1) stores `budget_after_charge + n - fetches(P) - land(P)` where `land(P) = 0`
     for a pre-count head label and `rem[P]` for a mid-block label. This is the tier-1 state at that label, without any
     separate head term (fixes the double count of v1.1).
  5. The same formula holds for exits into inlined frames (the callee's `Lowered` gives its labels and `rem`).
  6. **No overshoot:** within one compiled stay (one `rt::enter`), the instructions executed by trace code plus tier-1
     code never exceed the stay's budget, and an exhausted budget always leaves through a tier-1 budget exit label
     (`exit_at(head)`) in tier 1's state for that label.
- **Invariant (the oracle clock):** for every mode and every count budget, `fetches(run) + Stats::insns(run) =
  fetches(JIT-off run)`, and each individual compiled stay satisfies TR5.6. `Stats::insns` alone is **not** compared
  across modes. X6 (now in P0x, 10.1) checks the identity for tier 1 and J4 before TR5 relies on it.
- Cost: one compare, one subtract and one predicted branch per iteration, in a register (the J0 oracle measured up to
  31% for a per-block *memory* counter on the tightest loop, [M] JIT_ORACLE 2 reading 4; a register avoids the
  store-forwarding chain).

## 4. Recorder

### 4.1 Hooking the interpreter without changing the JIT-off `.text` (TR23)

The recorder runs **before every instruction of the recorded frames, in the interpreter's hooked copy**
(`run_mode::<true>`, which already calls `trace_exec` per instruction when `ci.trap != 0`, [V] vm_unsafe.rs main loop).
It never runs in the fast copy. This mirrors LuaJIT's dispatch swap: recording is a mode of the slower copy, so the
hot interpreter is untouched.

**TR23 (VM changes).** All rn-core changes are under a new cargo feature `jit_trace = ["jit"]` (enabled by
rn-jit's feature `trace`) and are confined to cold code. The recorder's state is **not** added as fields of
`JitSlot`/`GlobalState` (they are not `repr(C)`; new fields could move offsets that the fast copy uses, e.g. `st.g().heap`):
it lives in rn-jit's per-state data behind the existing `JitSlot.data` pointer, and rn-core reaches it only through a
registered `RecVTable` (cold, out-of-line calls).
1. `fast_trap` (`#[cold]`, [V] vm_unsafe.rs:495), first under `jit_trace`: `if jit_hooks::rec_armed() { ci.trap = 1;
   return true }`. `rec_armed` is one relaxed load of a process-global count of armed recordings, chosen so LLVM's
   inferred attributes of `fast_trap` do not change. It over-approximates `rec_wants`: a diverted frame that the
   recorder does not expect is either handed back by `rec_step` (foreign state: `savedpc = pc`, `trap = 0`, `Leave`,
   context and recorder untouched), aborts (`Frame`, same state), or takes the hooked copy's normal no-hook path and
   returns to the fast copy at its next frame transition (performance only). While any recording is active, every
   stack reallocation (which sets `trap = 1` on all frames) also diverts the frames of all states into the hooked copy
   (a cost, not a correctness issue).
2. Hooked copy, `'frame` head ([V] vm_unsafe.rs:516-528): **first**, under `jit_trace`, `if rec_wants(st, ci) { ci.trap
   = 1 }` (before the `TRAP_JIT` rewrite and before the "no hooks left: back to the fast copy" return). This is what
   keeps inlined callees in the hooked copy: `OP_CALL` sets `trap = 1` on the new frame only when `hook_mask() != 0`
   ([V] vm_unsafe.rs:1666-1669), `pre_call_lua_fast` never writes `trap` ([V] call.rs:457), and in the `jit_check_e`
   build `jit_hot_call` may have set `TRAP_JIT` on a compiled callee, which the hooked head would turn into 0. With the
   fix, the callee frame reaches `trace_call`/`trace_exec` and is recorded. `trace_call` does **not** call `hook_call`
   while recording without hooks (`hook_call` resets `oldpc` even when no call hook is set, [V] call.rs:120).
   `rec_wants(st, ci)` is true only for the recording thread and for the frame the recorder expects next (the root
   frame or the inlined callee it just recorded a CALL to).
3. The hooked main loop calls `jit_hooks::rec_step(st, ci, pc)` **before** `trace_exec` (not inside it), when the
   recorder is active for this OS thread; `trace_exec` only keeps `trap = 1` while recording with `hook_mask() == 0`
   (today it clears it, [V] vm.rs:80-83). A `Leave` with a hook installed falls through to `trace_exec` (the hooked
   copy keeps the frame; returning to the fast copy would re-run `trace_call` and fire a late call hook).
4. Hooked copy main loop, right after `trace_exec`: if the recorder asked to *leave* (recording finished or aborted),
   `ci.savedpc = pc` (the instruction not yet executed), `ci.trap = TRAP_JIT` if the recording installed a trace and
   the pc has a resume entry, else 0, and `return Ok(Some((ci, false)))` (hand the frame to the fast copy, which then
   enters compiled code through the existing E1 path, [V] `'frame` head). **v1.3:** before returning `Leave`, the
   recorder sets `trap = 0` on every *other* frame of its frame stack (the root and the inlined callers of the current
   frame; an abort inside an inlined callee would otherwise leave `trap = 1` on them, which TR23.3 kept set while
   recording). The same clearing runs in `rec_abort` for the frames that still exist.
5. **Error choke point.** Recorded frames run only in the hooked copy, and the recorder aborts before every native
   call except the intrinsics (`math.sqrt abs floor ceil min max`). An intrinsic is entered only after the recorder
   has checked its callee identity and its argument count and types (it aborts otherwise), and with number arguments
   these functions neither raise nor call Lua code nor yield (v1.2's "aborts before any native call" was false for
   intrinsics). So no `pcall` can run inside recorded frames, and an error raised in a recorded frame always
   propagates out of `run_mode::<true>` as `Err`. `run()` ([V] vm_unsafe.rs:473-487), under `jit_trace`, calls `jit_hooks::rec_abort(st,
   Abort::Error)` when **either copy** (`run_mode::<true>` or `run_mode::<false>`, `Err(Yield)` maps to `Abort::Frame`)
   returns `Err`; the fast copy matters because `rec_arm` happens there and an error can precede the first hooked
   step. **Arm contract:** the arm site hands the frame off immediately (returns to the interpreter without running
   anything that can raise before the frame's next trap poll). Ownership: the context carries the owning state
   (`g`, the address of its `JitSlot`) and a monotonic `token` (taken at `rec_arm`, passed to every recorder callback;
   a recorder treats a foreign token as a no-op). `rec_step` of a frame of another state returns `Leave` without
   touching the context; closing a state delivers `Abort::Close` only for its own recording; `REC_ARMED` is released by
   the thread-local's `Drop` at thread exit. A context stranded on T1 by a state that is armed on T1 but used or
   closed on T2 stays stale until T1 exits (no later state can match it: `g` differs); `rec_arm` on T1 is refused
   meanwhile. The reused-node ABA case where the abort is lost is rejected by the root `func`/closure check, and,
   failing that, by the recorder's expected-pc/depth check (TR23.5 validation is necessary, not sufficient). No catch site is enumerated. **Lazy validation** in addition:
   every `rec_wants`/`rec_step` checks that the thread is the recording thread, that `ci` is the expected frame by
   identity *and* by content (the function slot holds the expected closure, `savedpc` is inside its proto, the frame
   depth from the root matches the recorder's frame stack, and the root `CallInfo` still has the `func` index it had
   when recording was armed), and that `pc` is the recorder's expected successor; any mismatch aborts. A reused
   `CallInfo` node (ABA after an unwind) therefore never continues a stale recording.
6. (S0/F8 only) the virtual-allocation accounting of TR29.4: `Heap::charge_virtual/uncharge_virtual` and the credit at
   cycle ends in the 5.5 collector and `gen54.rs`, also under `jit_trace`; with sinking off (`-sink`) the virtual count is
   always 0, so the collector behaves exactly as without the feature.
- Gates: `scripts/jit-text-identity.sh` (JIT-off build byte-identical); the fast copy `run_mode::<false>` compared
  between `jit` and `jit,jit_trace` builds by **relocation-normalised disassembly** (`objdump -d -r` of the symbol with
  rel32 displacements, absolute addresses and symbol suffixes masked, like `jit-dump-identity.sh` masks `movabs`
  immediates): identical instruction sequence; plus the `docs/JIT_HOOK_COST.md` method: the `jit_trace` build's
  instruction counts on the 25 programs with `TRACE=off` within 0.2% of the `jit` build.
- Alternative considered and rejected for v1: a **dry-run recorder** that simulates one iteration in Rust on a copy of
  the frame without executing it (no rn-core change at all). It would duplicate VM semantics for every recorded
  opcode, cannot observe results of library code (`#t`, intrinsics) for free, and loses the lockstep check (4.5). Kept
  as the fallback if experiment X1 shows the hooked-copy route perturbs the fast copy.

### 4.2 Protocol and state machine

```rust
// rn-core/src/jit_hooks.rs (feature jit_trace)
pub struct RecVTable {
    pub step: fn(&mut State, Ci, *const u32) -> RecStep,  // before each instruction of a recorded frame
    pub abort: fn(&mut State, Abort),                      // error unwinding, coroutine switch, close
}
pub enum RecStep { Keep, Leave }   // Leave: recording finished or aborted, return to the fast copy at this pc
// no JitSlot fields: the recorder state (thread, root ci + its func index, frame stack, expected pc, leave flag)
// lives in rn-jit's per-state data behind JitSlot.data (TR23)
```

- **Arm** (`jit_tr_hot`): `rec_thread = this thread`, `rec_ci = ci`, recorder state `Start { head: H }`, `ci.trap = 1`.
- **Step**: `rec_step(st, ci, pc)` is called with the frame consistent (the interpreter's `savedpc` discipline before
  `trace_exec`). It (1) completes the *previous* instruction: lockstep check (4.5), expected successor (pc and frame
  depth) equals the actual one; (2) records the instruction at `pc` (4.3), or closes the loop when `pc == H` in the
  root frame after at least one instruction, or aborts.
- **Frames**: the recorder keeps a frame stack `RecFrame { proto, base_off (slot of register 0 relative to the root
  base), call_pc, nresults }`. A callback from a `ci` that is not the expected innermost recorded frame aborts
  (`Abort::Frame`). A `CALL` to a Lua closure that is inlined marks the callee frame expected next; the interpreter
  pushes its real `CallInfo`, which does **not** inherit `trap` (v1.1 was wrong here): the hooked `'frame` head sets
  `trap = 1` for it because `rec_wants` is true (TR23.2). A `RETURN` from an inlined callee continues in the caller,
  whose `trap` is still 1 (the hooked copy does not clear it while recording, TR23.3).
- **Recording liveness contract (P6b).** The cache holds one `active` recording per state. If the recorder never calls
  `abort_*`/`install` (the root frame was unwound by an error, the coroutine was abandoned, the VM disarmed the hook,
  a dependency was freed and only flagged), the next `jit_tr_hot` of **any** head drops it: when `!rec_active_for(st)` or
  the recording is flagged `dep_freed`, it is aborted as `Abandoned` (no penalty, charged buffers refunded, head back to
  `Warm`) and the state records again. The recorder therefore treats a stale `TrId` (`rec_aborted`, `install` ->
  `Stale`) as a normal outcome and must arm the VM hook (`rec_arm`) before relying on its recording staying active.
  `flush_all(Off)` (mode `off`, `set_mode(Off)`) aborts the active recording and `install` refuses while off
  (`InstallError::Off`); switching back on re-arms every `Warm` head (they were parked at `2^30`).
- **Finish** (the pipeline driver in `tr/mod.rs`, owned by PI): optimise, allocate, assemble, place, install
  (`cell[H] = entry`), return `Leave`. Any failure: abort.
- **TR16 (recording isolation).** A recording exists for at most one thread of a state at a time, only while
  `hook_mask() == 0`, and only for frames of its own frame stack, validated lazily as in TR23.5. The recorder never
  mutates VM state except `ci.trap` of recorded frames and tier-1 compiles of callees (TR4b, which run no generated code).

### 4.3 What is recorded

For each instruction the recorder reads the operand **values** from the frame (tags and payloads), the constants, and,
for tables, the shape it needs (`asize`, `array` slot tag at the index, metatable pointer null or not; full build:
`lsizenode`, node index of a constant key, metatable flag bits). It emits IR for the *observed* case:

| bytecode | recorded as (prototype) |
|---|---|
| register read | first read of a slot in the trace: `SLOAD slot, type` with typecheck (type = observed tag); later reads: the slot's current SSA ref |
| arithmetic | typed op on the observed operand types (int/int -> int op, mixed -> `CONV` + float op); non-number operand -> abort (`Abort::Coerce`/`Abort::Metamethod`) |
| `IDIV MOD` | int: guards divisor != 0 (exit) and the `-1` case as tier 1 (`divmod_leaf`); float `//` `%`: pure helper `CALLN fmod/floor` |
| compare + JMP | guard asserting the observed outcome; the follower is consumed as in tier 1 |
| `TEST/TESTSET` | type guard on the observed tag (truthiness is a function of the tag) |
| `FORLOOP` (5.5) | int: `guard count != 0`, `count - 1`, `idx + step` (wrapping); float: `idx + step`, guard of the observed outcome of the step-sign-dependent compare (step sign guarded once as invariant) |
| `GETUPVAL` | `UREF closure, n` -> guard `uv.v` outside the trace's frames (TR15) -> `ULOAD` with typecheck; N1: an open upvalue at a slot of the root frame is the register access (`EQ UVREL, 16*slot` identity guard, no `ULOAD`) |
| `GETI/GETTABLE` | guards: table type, `metatable == null`, integer key (float keys: `F2I_EXACT`), `0 <= k-1 < asize`; `ALOAD` with typecheck of the observed element tag; a nil element aborts (prototype) |
| `SETI/SETTABLE` | guards as above plus "slot non-empty or no metatable" (tier-1 rule); `ASTORE`; `TBAR` when the value may be collectable (TR10) |
| `CALL` (Lua closure) | guard function slot tag LCL and `closure.p == proto` (TR7a); arguments become the callee frame's slots (missing ones nil); callee frame pushed in the recorder; no IR for the call itself |
| `RETURN*` (inlined frame) | the results are moved to the caller's slots `A ..` (nil fill to `C-1`), the recorder pops the frame |
| `CALL` (intrinsic) | guard callee tag LCF and payload == builtin address; typed IR op (`FSQRT`, `ABS`, `FFLOOR` + exact conversion guard, `MIN/MAX` with the operand order of J4 M4) |
| loop head `H` reached again in the root frame | close the loop: `LOOP` marker, then peeling (6.4) |

**Branch direction** and **callee identity** are thus guards; **loop-carried types** come from the second visit of `H`
(the recorder records exactly one iteration; peeling replays it, 6.4).

### 4.4 Inlining rules

- Inline only Lua closures with fixed argument count (`B != 0`) and fixed result count
  (`C != 0`; F4 and P-E relax both through the explicit `top`; P-E also inlines **vararg** callees without a vararg
  table, see "Vararg frames (P-E)" below), inline depth <= 4 (prototype; `maxdepth` knob), no recursion (a proto already on the recorder's frame
  stack aborts with `Abort::Recursion`; full build: unroll up to 2), callee tier-1 compiled (TR4b), dependency edge
  registered at the moment of inlining (TR32).
- A `RETURN` at depth 0 (the root frame returns) aborts (`Abort::LeaveFrame`), as does a `TAILCALL` (prototype).
  **F4:** recursion is inlined up to `unroll` (default 2) activations of a prototype (the root counts); the call beyond
  ends the recording with the stitching exit of F2b (or aborts `Recursion` when the prefix is shorter than `stitchmin`, in
  a side recording, or with `unroll=0`). A `TAILCALL` of a Lua closure in an inlined frame *replaces* that frame (same
  function slot, `CallInfo`, caller pc and wanted results; the callee's prototype; `CIST_TAIL`); a `TAILCALL` of an
  intrinsic is the intrinsic followed by a `RETURN` of its result; the root frame and other C functions keep `TailCall`.
  `CALL`/`RETURN` with `B == 0` or `C == 0` are recorded with an explicit `top` (TR22b below).
- **Vararg frames (P-E).** A `CALL` of a vararg Lua closure (not the 5.5 vararg-table kind `...t`, not a `TAILCALL`, not a
  vararg root) is inlined with the *pre-adjust* layout, and its `VARARGPREP` step is recorded as `luaT_adjustvarargs`
  performs it: with `totalargs = max(nargs, numparams)` the function and the fixed parameters are copied to
  `func + 1 + totalargs`, the original parameters become nil, `R[numparams]` of the new frame is nil, and the frame's
  function slot, base and `CallInfo::func` move up by `totalargs + 1` (`nextra = totalargs - numparams` extra arguments stay
  below the new function slot). No guard is emitted in that window, so no snapshot sees the pre-adjust layout. The number
  of extra arguments is a recorder constant (the inlined `CALL`'s argument count), hence `VARARG A C` is a plain slot copy
  (`C-1` values nil-filled, or all with an explicit `top` for `C = 0`), `select('#', ...)` is the constant `nargs-1` (a
  guard on the selector string's identity) and `select(i, ...)` the arguments from `i` on (a guard on the selector's
  value; negative `i` counts from the end), both recorded as the native call (callee-identity guard on the registered
  `select`, native frame room/`CallInfo`, the native's push above its arguments, results moved to the call register).
  `RETURN` of a vararg frame (`C = numparams + 1`) returns to the original function slot (`func - delta`). Snapshots carry
  `FrameLink.nextra` (the restore writes `CallInfo::nextraargs`; `func_slot` is the shifted slot); the stack condition adds
  `VARARGPREP`'s `check_stack(maxstacksize + 1)` at `func + 1 + totalargs` and, for `VARARG C = 0`, `check_stack(nextra)`
  above the frame's `top`. **Callee loops:** a counted `for` of an inlined callee whose bounds are trace constants (typically
  `for i = 1, select('#', ...)`) is unrolled (each iteration recorded, up to `loopunroll` = 16 backward edges per
  recording, then `InnerLoop` as before; the loop ends with a guard `count == 0`); a loop with a data-dependent trip
  count keeps the `InnerLoop` abort. **M1 (callee loops, closes, recursion):** `ipairs` / `pairs` loops of callees (5.5: the end of the traversal is the guarded
  fall-through of N7) are unrolled under the same `loopunroll` bound (knob `gforinl`; `while` / `repeat` loops of callees only with
  the opt-in `loopany=1`); a `RETURN` / `TAILCALL` with the close flag `k` in an inlined frame is a no-op (the
  recorder entered the frame itself and records no `CLOSURE` / `TBC` / `TFORPREP` in a way that leaves something open: knob
  `closeret`); the depth limit and the recursion `unroll` limit end the recording with a stitching exit (also in side recordings of
  loop roots, which get `unroll`: knobs `depthstitch`, `sidestitch`); an array element that is nil is an `ALOAD` of type nil (`nilload`). Not done: `{...}` / `table.pack(...)` (the
  constructor's array must be resized to the vararg count: no IR op for it, and element-wise insertion would give another
  table layout), a vararg root frame (`nextraargs` is a runtime value), `TAILCALL` into a vararg callee, `pcall`.
- **TR6c (stack room, exact).** For every call on the recorded path the recorder records the condition under which
  the interpreter takes its **non-growing fast path**, root-relative: a Lua call at function slot `f` with `nargs`
  arguments (`top = f + 1 + nargs` at the call) needs `stack_size - top > maxstacksize(callee)` (strict, exactly
  `pre_call_lua_fast`, [V] call.rs:457-461); an intrinsic call (a native in JIT-off) needs `stack_size - top >
  LUA_MINSTACK` (exactly `pre_call_native_fast`, [V] call.rs:344). The entry check uses the maximum over all such sites
  of `required_top + required_room`, and also covers the largest slot written (`base_off + maxstacksize` of every frame).
  If the interpreter would have taken a slow (possibly growing, GC-checking) path anywhere on the path, the trace is
  not entered and tier 1 runs the iteration with interpreter semantics. Conservative by design: a slow path that would
  not have grown also sends the iteration to tier 1, which is correct.
- **TR6b (CallInfo spares, exact).** The recorder records the maximum CallInfo depth reached on the path, counting
  inlined Lua frames **and** the native frame of every intrinsic call (`pre_call_native_fast` takes `ci.next` too and
  falls back to the slow path, which may extend the list, when it is null, [V] call.rs:343-345); the entry checks that
  many non-null `next` nodes. The interpreter writes fields of the native frame's node (`func`, `callstatus`, `top`,
  `k`); those nodes are above the current `ci` after the call and not observable, so the trace does not write them.

### 4.5 Lockstep check (TR17)

At every step, before recording the next instruction, the recorder checks the previous one against the interpreter:
for each slot it defined (def set from `t15::effects`, [V]), the frame's tag equals the IR type of the def, and for
numbers and booleans the payload equals the value the **IR interpreter** (`tr::interp`, 5.7) computes for the recorded
IR on the concrete inputs (bit-exact; NaN payloads included). The successor pc and frame depth equal the expected ones.
Slots the model holds that the instruction did not define must still have the modelled tag (catches state changed
behind the recorder's back, e.g. a finalizer's `debug.setlocal` during a GC step of an allocating instruction in the
full build). A mismatch aborts with `Abort::Diverge`; in debug builds and in `+verify` a mismatch with no GC step,
hook or nested frame in between is an assertion failure (it is a recorder or IR-semantics bug). The check costs
O(live slots) per recorded instruction, only while recording (about one loop iteration).

### 4.6 Abort reasons (reported by `RUSTYNAIL_JIT_VERBOSE`, counted per reason)

`Nyi(op)` (opcode not in the recorded set), `CFunc` (call of a non-intrinsic C function; full build: stitch instead),
`Metamethod` (an operand type or table state that would call a metamethod function), `Coerce` (string -> number
coercion; full build: recorded), `MetaTable` (table with a metatable on a path that needs its absence; full build:
flag guards), `TableHash` (hash-part access; full build: recorded), `TableNil` (nil array element read; full build:
recorded with `metatable == null`), `Insert` (store to an empty slot of a table, i.e. a possible insertion), `Vararg`
(vararg callee or `VARARG`), `MultRet` (`B == 0` or `C == 0` calls/returns, top-dependent), `TailCall`, `Recursion`,
`Depth` (inline depth), `LeaveFrame` (return below the root), `LeaveLoop` (the loop exited or a different loop head was
reached before `H`: inner loop or break), `InnerLoop` (a loop head inside the recorded path; full build: link), `TooLong`
(more than `maxrecord` = 4000 IR instructions or 500 snapshots or 200 slots of extent), `Unstable` (a slot's type at the
second visit of `H` differs from its type at the start, int -> float included: the head is re-armed for the next
back-edge and re-recorded from the new types, counted in `rerec`, 3.3/TR24c; prototype and full build),
`DepFreed` (a proto with an edge to the active recording was freed, TR32), `UpvalAlias` (an open upvalue pointing into the trace's frames), `Hook` (hook installed during recording), `Error`
(an instruction raised, from the unwinding hook), `Frame` (callback from an unexpected frame or thread: coroutine
switch, nested run), `Diverge` (lockstep mismatch), `Charge` (memory/code cap refused), `Cap` (`maxtrace` reached),
`Compile(reason)` (register allocation or assembly failure: too many spill slots, PHI shuffle, code size).

### 4.7 Never traced (TR1 restated per construct)

C functions other than the intrinsics; `pcall`, `xpcall`, `error`; `coroutine.*` (a trace never spans a yield or a
resume); every metamethod *function*; `__gc`/GC steps/finalizers (an allocating instruction whose GC step would run
exits first, 6.9); `debug.*`; `setmetatable`/`getmetatable` (C functions); string operations that allocate (prototype:
all); `CLOSURE`, `NEWTABLE`, `CONCAT`, `VARARG`, `TFORCALL`, `CLOSE`, `TBC` (prototype; full build: `NEWTABLE` and
`CONCAT` as class-A helpers, `TFORCALL` over `ipairs`/`next` as recorded builtins); anything that raises.

### 4.8 Errors, hooks and GC during recording

- The recorder runs *before* the interpreter executes the instruction, and the interpreter then executes it with
  reference semantics (the hooked copy *is* the reference behaviour): recording never changes program behaviour.
- An instruction that raises: the unwinding hook (TR23 item 5) aborts the recording; its frames are gone, the
  interpreter continues in the catching frame. A recorder that can predict the error (non-number arithmetic operand
  without a metamethod, indexing a non-table) aborts before the instruction instead.
- A hook installed during recording (async, or by a native called from the recorded loop, which already aborted at
  the CALL): the next step sees `hook_mask() != 0` and aborts; the hooked copy then fires hooks normally.
- A GC step during recording (full build: an allocating instruction): finalizers run as nested calls *above* the
  innermost recorded frame. Callbacks from such nested frames are not recorded (the step returns `Keep` and clears
  their `trap`); a callback from any other unexpected frame aborts (`Frame`). Whatever a finalizer did to recorded
  slots (`debug.setlocal`) is caught by the lockstep check (4.5) at the next recorded instruction. In the prototype no
  recorded instruction allocates, so this case cannot occur there.
- Coroutine switch inside the recorded loop is only possible through a C function, which aborts at the CALL.

## 5. SSA IR

### 5.1 Format

```rust
pub type Ref = u16;                 // REF_BIAS = 0x8000; refs below the bias are constants, above are instructions
#[repr(C)]
pub struct Ins { pub op: TOp, pub t: Ty, pub flags: u8 /* GUARD, PHI, LOOPINV, MARK */,
                 pub a: Ref, pub b: Ref, pub prev: Ref /* CSE chain of the same op */, pub aux: u32 }
pub struct TraceIr {
    pub ins: Vec<Ins>, pub k: Vec<KVal>,          // KVal: Int(i64) | Num(u64 bits) | Pri(Ty) | Ptr(u64, PtrKind)
    pub snaps: Vec<Snap>, pub snapmap: Vec<SnapEntry>, pub frames: Vec<FrameLink>,
    pub loop_at: Option<Ref>,                     // the LOOP marker (pre-roll below, body above)
    pub deps: Vec<ProtoKey>, pub head: (ProtoKey, u32), pub extent: u16, pub depth: u8,
    pub counts: Vec<u32>,                          // per segment total n (TR5)
}
```

Instructions are appended in order (straight line), each optimisation is a backward look at already-emitted IR
(LuaJIT's design, [V] design notes 2.1), "defined before" is an integer compare, constants are interned (one ref per
value). Limits: 4000 instructions, 500 snapshots (`TooLong`).

### 5.2 Types

`Nil False True Int Num ShrStr LngStr Tab Lfn Lcf Ccl Udata LUdata Thread` (one per VM tag byte; the mapping table is
generated from `rn_core::value` constants exactly as `t15::TagSet` is, [V]), plus raw machine types `I64` (counts,
indices), `P64` (untagged pointers: array base, node, `UpVal`, `Proto`), `U32` (asize). A Lua-typed value is the
unboxed payload; its tag is implied by the type and written only when the value is stored to a slot (exit, array
store). Every Lua value on a trace has a singleton type (a polymorphic value is guarded at its load).

### 5.3 Instruction set and mapping to the micro-op IR

"class": N = pure, can be CSE'd/hoisted; G = guard (has a snapshot); L = load (memory class in brackets); S = store;
C = helper call (class N/S/A of TR1). Micro-ops are the existing `ir::Op`s ([V] ir.rs); `x`/`v` are allocated
registers or spill slots, `rbx` the root frame base.

| op | class | semantics | micro-ops |
|---|---|---|---|
| `KINT KNUM KPRI KPTR` | const | constants | `Src::Imm` or `Mov`; floats `Mov V7, bits; GtoF` (V7 = trace scratch, 7.1) |
| `SLOAD slot [check]` | L[stack] + G | value of root-relative slot at trace entry; `check` = typecheck | `BrM Ne B1 [rbx+16s] tag -> exit`; `Ld B8` / `LdF [rbx+16s+8]` |
| `ADD SUB MUL NEG BAND BOR BXOR BNOT SHL SHR` (Int) | N | wrapping, Lua shift semantics (counts >= 64, negative) | `Add Sub Mul Neg And Or Xor Not ShlI ShrI ShlV ShrV` (register or 32-bit immediate operands only: `Src` is `V` or `Imm`, [V] ir.rs; 64-bit constants via `Mov V7`; + the `shiftl` composition of `templates_arith`) |
| `IDIV MOD` (Int) | N + G | floor semantics, guard divisor != 0 | `SDivRem` + floor correction (`divmod_leaf` pattern), `-1` special case (J4 M5) |
| `FADD FSUB FMUL FDIV FNEG FABS FSQRT FMIN FMAX` | N | IEEE, operand order preserved (TR11) | `FBin FBinM FNeg FAbs FSqrt` |
| `CONV.num.int` / `F2I_EXACT` | N / G | int -> float; float -> int exact (keys, floor) | `I2Fx` / `F2IExact -> exit` |
| `EQ NE LT LE GT GE ULT UGE` (Int) | G | assert relation | `Br cc a b -> exit` (negation allowed for integers) |
| `FCMP cc expect` (Num) | G | assert `(a cc b) == expect` | `expect = true`: `FBrX cc a b -> ok; Jmp exit; Bind ok`; `false`: `FBrX cc a b -> exit` (never a negated cc, TR11) |
| `EQREF` (P64 / Lua ref) | G | identity (callee proto, builtin) | `Mov V7, imm64; Br Ne a V7 -> exit` |
| `FLOAD tab field` | L[hdr] | `ASIZE ARRAY METATABLE FLAGS LSIZENODE NODE` | `Ld B4/B8/B1 [t+off]` (offsets from `jit_layout::table`, [V]) |
| `AREF arr idx` | N | address of element `idx` | folded into the access (indexed `Mi`) |
| `ALOAD aref [check]` | L[array] + G | element tag check + payload | `BrMi Ne B1 [arr+4+u] tag -> exit`; payload at `arr - 8(u+1)` |
| `ASTORE aref v` | S[array] | tag byte + payload | `StI B1`, `StI B8` / `StFi` |
| `UREF fn n`, `ULOAD`, `USTORE` | L/S[upval] | `UpVal.v` pointer, value | `Ld` through `lclosure::UPVALS`, `upval::V` ([V] jit_layout) |
| `TBAR tab v` | C(S) | `barrier_back` condition, slow path on the cold side | inline `marked & BLACKBIT`, value collectable and `& WHITEBITS` tests; cold `CallH` bracketed by save/restore (7.5) |
| `CALLN f args` | C(N) | pure helpers: `pow_lua`, `fmod`, `floor` of floats | cold-or-hot `CallH` bracketed (7.5) |
| `LOOP` | marker | start of the loop body copy | `Bind` |
| `PHI a b` | marker | loop-carried value (pre-roll ref, body ref) | parallel moves at the back-edge (7.3) |
| `POLL` | G | `TRACE_POLL` (P-L; was trap + INTERRUPT) | `Mov V7, &TRACE_POLL; BrM Ne B1 [V7] 0` (to a generic stub tagged `EXIT_POLL`) |
| `BUDGET n` | G | TR5.2 | `Br Lt vb n -> exit_H; Sub vb n` |
| full build: `HREFK tab key slotidx`, `HLOAD`, `HSTORE`, `NEWREF`, `TNEW`, `STRTO`, `SNEW/CONCAT` | L/S/C(A) | hash part, insertion and allocation through class-A helpers (TR1), string coercion | section 10, tasks F1/F3 |
| 5.4 only: `ATOUCH tab u` | S[hint] | `alimit_touch` (TR30) | the `Cx::alimit_touch` sequence of `templates_table.rs` |

Needed micro-op additions (T1 pattern: emulator + x86 + `emu_ops_diff` + iced oracle; one task, P0b): unsigned
`Cc::{Ult, Uge}` for single-compare bounds checks [?] (tier 1 uses two signed compares today; X8 measures whether it
matters), and emulator support for `SetCi SetK JmpR` (today `Stop::Unsupported`, [V] JIT_DESIGN 14) so the shared exit
routine and cell jumps run in the emulator. Facts the asm must respect [V] ir.rs: there is no memory-destination
arithmetic (`Add` takes a register destination; a counter increment is `Ld; Add; St`); `CallH` passes only two `u32`
immediates besides thread and ci; `Jmp(L)` targets labels of the same `Program` (anything in another code block, such
as the shared exit routine or a tier-1 label, is reached by `Mov V7, imm64; JmpR V7`). No other new op is expected for
the prototype.

### 5.4 Guards and snapshots

- **TR3a (re-execute convention).** Every guard belongs to the instruction `P` whose recording emitted it, and its
  snapshot describes the state **before `P`**; the exit re-executes `P` in tier 1 (E3; LuaJIT's `pc + 1` convention
  does not transfer, [V] design notes 3.2). Guards of one instruction are emitted before any of its effects
  (stores), so a guard failure never leaves a half-executed instruction. **Exception (5.4, v1.3):** the `ATOUCH` of
  an integer-key array read (TR30) sits after the bounds guard and before the element typecheck of the same
  instruction, as in `templates_table.rs`. This is safe because `alimit_touch` is idempotent and monotone for a given
  `(t, u)`: when the typecheck exits, tier 1 re-executes the instruction and performs the same touch again with the
  same result, so the state equals a single execution. No other effect may precede a guard of its instruction.
- A snapshot is taken at the first guard of each instruction that changed something since the previous snapshot
  (LuaJIT's merge rule); guards of the same instruction share it. Within a snapshot all guards exit with the same
  register mapping (the RA's exit-consistency rule, 7.2).

```rust
pub struct Snap { pub pc: u32, pub frame: u8 /* depth of P's frame */, pub map: (u32, u16), pub nframes: u8,
                  pub refund: i32 /* TR5.4 */, pub land: Land /* Pre | Mid */, pub count: u32 }
pub struct SnapEntry { pub slot: u16 /* root-relative */, pub r: Ref, pub flags: u8 /* FUNC */ }
pub struct FrameLink { pub proto: ProtoKey, pub func_slot: u16, pub call_pc: u32, pub nresults: i16 }
```

- **TR3 (exact state at exits).** After an exit's restore the VM state equals the interpreter's state before `P` at the
  same dynamic instruction: every stack slot in `[root base, root base + extent)` that was written on the recorded
  path up to `P` holds the value the interpreter left there, **including slots written by inlined frames that have
  already returned** (the interpreter leaves a callee's registers in the stack after it returns; they can keep objects
  alive while below the GC's scan top and are visible to `debug.getlocal` of temporaries, the analogue of J4 R-EXACT;
  snapshot maps are root-relative and never drop a slot because its frame was popped), dead or not, temporaries
  included; function slots of inlined frames hold their closures; the
  `CallInfo`s of inlined frames are filled as the interpreter would have filled them; `savedpc` of every frame,
  `thread.ci`, `thread.top` and the budget word are as the interpreter/tier 1 leave them. **No dead-slot pruning**:
  `debug.getlocal` reads temporaries ([V] J4 R-EXACT), and a stale collectable left in a dead slot would extend an
  object's lifetime, changing finalizer timing, weak-table contents and `collectgarbage("count")`. Sole relaxation:
  with sinking on (F8), dead or out-of-scope slots may differ as TR29.2(c) allows (decision D7); with `-sink` TR3 holds
  exactly.
- **Cost of TR3 (v1.3, full-build requirement).** Every slot written in an iteration, inlined-callee temporaries
  included, must be restorable at the exits of the *next* iteration until it is redefined, so in the peeled layout its
  value stays live across the back-edge as a PHI even when the next iteration never reads it: register pressure,
  back-edge moves, `Compile(phi)` aborts (TR24d); and a `NEWTABLE` result held in a body local is a PHI operand at `S_H`,
  so TR26a as written would sink almost nothing. Requirement: **end-of-iteration writeback** (TR3b below), decided and
  measured in S0 (X11) before F8; sinking stays behind S0.
- **TR3b (writeback, v1.3).** Before `Jmp LOOP` a trace may store (tag + payload) to the stack every slot that the
  next iteration does not read before redefining it; such a slot then needs no PHI. Legal under TR1: nothing can
  observe the stack between the store and the next exit, and the stored value is exactly the interpreter's value at
  `H`. The rolled (`-loop`) layout of 7.8 is the degenerate case (all written slots). Stores to the stack are allowed
  only here and in exits.
- Snapshot entries are *sparse*: a slot is omitted when **the stack already holds its value** (v1.3 generalisation:
  its current ref is the unmodified `SLOAD` of the same slot, or the slot was written back by TR3b and not redefined
  since). Slots never written on the path are omitted.
- **TR22b (top).** Every exit sets `thread.top = ci.top` of the innermost frame (the interpreter's invariant for
  instructions that do not use `top`), **except** the snapshots of an instruction that reads `top` (`CALL B=0`,
  `RETURN B=0`, `TAILCALL B=0`): since F4 the recorder models the `top` a producer (`CALL C=0` of an inlined callee whose
  `RETURN` fixes the count, an intrinsic with `C=0`) leaves as a root-relative slot, the lockstep compares it with the VM's
  `thread.top` after the producer, and `Snap.top` (`top_slot + 1`, 0 = none) makes the exit restore `thread.top = base +
  top_slot` instead. Such snapshots take the generic (Rust) exit path and never start side traces. `SETLIST B=0` and
  `VARARG` stay aborts (`MultRet`, `Vararg`).

### 5.5 Frame reconstruction (inlined frames)

The root frame is the real `CallInfo` in `r13`; inlined frames have no `CallInfo` on the trace. A snapshot with
`nframes > 0` restores, in order (all in the Rust restore, `tr::exit`):
1. all snapshot slots of all frames (root-relative, so callee slots are `base_off + i`), from registers, spill slots or
   constants, tag + payload; function slots from their FUNC entries;
2. for frame `k = 1..nframes`: `ci_k = ci_{k-1}.next` (guaranteed non-null by the entry check TR6b), filled exactly as
   `pre_call_lua_fast`/the tier-1 flat call fill it ([V] J-FLAT: `func`, `top = func + 1 + maxstacksize`,
   `callstatus = nresults + 1`, `savedpc`, `kcache`; `trap = 0` as the tier-1 flat call writes it, where the
   interpreter would leave the node's stale value: a stale nonzero `trap` only makes `fast_trap` clear it, so the
   difference is unobservable), and the caller's `savedpc` = instruction after its `CALL` (the interpreter's `savepc!()`
   at OP_CALL);
3. `ci_d.savedpc = &code_d[P]`, `thread.ci = ci_d`, `thread.top = ci_d.top`, budget word (TR5.4);
4. continuation = the tier-1 label of `P` in proto `d` (`Pre`/`Mid`, from its `pcmap`, [V] T3) with `rbx/r13/r14` for
   frame `d`.
- Open upvalues: the prototype creates none (no `CLOSURE`) and refuses upvalues that point into trace frames (TR15),
  so no upvalue list changes. Varargs: none (no vararg callee). `CallInfo` allocation: none (TR6b), so GC debt is
  unchanged (`luaE_extendCI` charges GC debt in PUC; the interpreter would have allocated at the same call only if the
  list was short, and the entry check sends that case to tier 1, which allocates exactly like the interpreter).
- A frameless snapshot (`nframes == 0`, the common case) is restored by an **inline stub** (7.6) with the same
  semantics; both paths are generated from the same `Snap` (no second source of truth).
- **TR2 (single activation).** At most one trace activation exists per state at any time: trace code calls only
  class N/S/A helpers (TR1), none of which can enter compiled code, run Lua code or switch coroutines, so no second
  trace (or tier-1 stay) can start before the first one has exited. Per-state areas (exit area, spill slots, argument
  words, continuation cell) are therefore never shared by two activations; `+verify` asserts it with an "on trace"
  flag set at entry and cleared by every exit path.

### 5.6 Exit state

`ExitArea` in the per-state `JitData`, addressed from `r15` (the budget word, [V] J-REGS): `trace_id: u32`, `exitno:
u32`, `gpr: [u64; 9]`, `xmm: [u64; 16]`, `spill: [u64; NSPILL = 256]`, `args: [u64; 2]`. A generic stub stores its
`trace_id` and `exitno` there (`St B4` immediates) and jumps to the shared exit routine (`Mov V7, imm64; JmpR V7`: it is
another code block). The shared routine is one per arena and cannot carry a trace id itself: it stores the allocatable
registers (V7 holds no value at that point, 7.1), then `CallH jit_tr_exit(thread, ci, 0, 0)`, which reads `trace_id`
and `exitno` from the area, restores (5.5), updates exit counters and policy, and writes the continuation address to
the state's continuation cell; the routine reloads `r13` from `thread.ci`, `ReloadFrame`, `r14` from `ci.kcache`, and
`JmpR`s. TR2 makes one area per state sufficient.

### 5.7 IR interpreter and the Miri story (TR28)

`tr::interp` executes `TraceIr` on a model frame and a model heap (array tables, upvalues): every op's semantics in
plain Rust, NaN/-0.0 behaviour copied from `emu.rs` (the emulator's explicit x86 NaN rules, [V] J4-T1), guards return
`Exit(snapshot)`. It is the **reference semantics of the IR** and is used by (1) the recorder lockstep (4.5), (2)
property tests of every optimisation pass (random well-typed IR before/after the pass must give the same exits and
final states), (3) the register allocator/assembler tests (IR interpreter vs emulator running the assembled micro-ops on
random frames), (4) the restore tests (snapshot -> frames). All four run under Miri. The emulator runs the assembled
trace (micro-ops) under Miri on synthetic frames, as for tier 1 and J4; the x86 code is checked against the emulator by
native differential tests (`emu_ops_diff` pattern) and by whole-program `+emurun` (9.6).

## 6. Optimisations and their soundness conditions

All passes have a `RUSTYNAIL_JIT_TRACE_OPT=-name` switch; every pass is property-tested against `tr::interp`.

### 6.1 Constant folding (`-fold`)

- **TR12 (integers).** Integer ops wrap (Lua 5.3+): no overflow guards, no narrowing pass. Algebraic identities are
  folded only where exact for all inputs (`x + 0`, `x * 1`, `x & -1`, `x ~ 0`, shifts by constants with the VM's
  semantics). `IDIV/MOD` by a constant other than 0/-1 may become multiply-high only with a proof test (J4 T10c, which
  would have provided a shared implementation, was skipped: J5 writes its own if X9 shows it matters).
- **TR11 (floats).** Never fold an operation whose result is NaN or depends on a NaN payload or the sign of zero
  (`x + 0.0` is not `x` for `-0.0`; `x * 1.0` quiets a signalling NaN, observable through `string.pack`); fold
  constant x constant only when inputs and result are not NaN (Rust's NaN results are not portable, and Miri
  randomises them). No FMA, no reassociation, no conversion of compares (`not (a < b)` is not `a >= b`). `min/max`
  keep the operand order rule of J4 M4.
- Guards folded to constant true are removed; folded to constant false: the recording aborts (`Compile(const_guard)`;
  it means the recorder disagreed with itself).

### 6.2 CSE (`-cse`)

Per-op chains (`Ins.prev`), search stops at the larger operand ref ([V] design notes 2.1). Only class-N ops and guards
are CSE'd by this pass; loads go through 6.6. Guards CSE: a guard identical to a dominating guard (same op, operands,
expectation) is removed; on a straight-line trace "dominating" = "earlier".

### 6.3 Guard elimination

- Type guards on refs whose type is known by construction are never emitted (an `ALOAD` with typecheck makes later
  uses typed).
- Range facts from the numeric `for` (control variable in `[init, limit]`) remove the `k >= 1` half of array bounds
  checks; with `Ult` (5.3) the remaining check is one compare.
- **TR14 (metatables).** No metatable *identity* guard exists (TR7). The only metatable guards are `FLOAD(t,
  METATABLE) == 0` and, in the full build, flag-cache bits of the metatable ("no `__index`/`__newindex`"), which the
  interpreter's `invalidate_tm_cache` keeps sound (it is reached by every store that inserts a key into a table, which
  happens only in a class-A helper or in tier 1). A metatable guard on a table whose identity is invariant on the trace
  (same ref) is hoisted by peeling, because no `setmetatable` can run on a trace (TR1).

### 6.4 LICM by loop peeling (`-loop`)

LuaJIT's copy-substitution ([V] design notes 5.1): after the recorded iteration (the *pre-roll*) the optimiser emits
`LOOP`, then re-emits every pre-roll instruction through the whole pipeline (fold, CSE, forwarding) with operands
substituted (`subst[ref]`: SLOAD of a slot -> the slot's ref at the end of the pre-roll). Invariant instructions fold to
their pre-roll refs and disappear from the body; variant ones remain; for every slot whose ref differs between the
start and the end of an iteration a `PHI(pre, body)` is emitted after the body. Without this pass (`-loop`, and before
P7 exists) the trace uses the rolled layout of 7.8. Soundness conditions:
- **TR24 (peeling).** (a) The body is exactly one more iteration of the same recorded path, so every body guard is the
  pre-roll guard on substituted operands, and every body snapshot maps the same slots (with substituted refs); (b) a
  load folds to a pre-roll load only through the memory rules (6.6) applied to the stores *between them*, which include
  the pre-roll's stores after the original load and the body's stores before the copy; (c) type instability (v1.3:
  one mechanism, re-record; no `CONV` repair on the trace): a slot whose type at the second visit of `H` differs from
  its type at the start aborts the recording with `Unstable`, and `H` is re-recorded from the next back-edge, where the
  recorder sees the types the iteration produced (3.3, `rerec`, max 3 then `Black`). An `Int -> Num` accumulator thus
  gets a `Num` trace whose entry fails once per loop execution (the free peel, not counted, 3.3) and runs the first
  iteration in tier 1; (d) PHI count <= 64, else `Compile(phi)`; (e) **non-slot PHIs (v1.3):** when a body instruction
  forwards or CSEs to a pre-roll ref that is itself variant (the body has its own copy), e.g. the body's `t[i-1]` load
  forwarded to the pre-roll's `t[i]` store value, or a `ULOAD` forwarded to the pre-roll's `USTORE`, a `PHI(pre ref,
  body copy)` is emitted for that ref exactly as for a slot (LuaJIT emits PHIs for every variant pre-roll ref used in the
  body); these PHIs count towards (d). Fold normalises index arithmetic so that the body's index matches the pre-roll's
  ref: `x - c` -> `x + (-c)`, `(x + c1) + c2` -> `x + (c1 + c2)` (wrapping, exact, TR12).
- Measured value of peeling is small in LuaJIT on these programs (`-O-loop`: no visible effect on nbody, [M]), but
  hoisting is also what moves invariant guards (types, metatable, callee identity of inlined calls, upvalue pointer
  checks) out of the loop; for inlined calls this is most of the gain (X9 measures `-loop`).

### 6.5 Bounds-check hoisting (ABC, `-abc`, prototype optional)

For `ALOAD/ASTORE` on an invariant table ref `t` with index `i = ctl + c` (`ctl` the integer for-loop control, `c` a
constant): replace the body bounds check by one check in the pre-roll that the whole remaining range is in bounds
(`limit + c - 1 < asize` for a positive step, both ends for negative). **TR25 (ABC).** Allowed only if `asize` and the
array pointer are invariant on the trace: no `NEWREF`, no class-A helper and no store that can resize any table that
may alias `t` exists in the loop (prototype: always true, nothing can resize). The loop limit must be the integer
for-loop count (5.5 precomputed count, [V] FORPREP), not a `while` condition. J4 measured the per-access check as 1
compare + 1 predicted branch: expected gain < 3% (decision 9.2 of J4); kept optional, built only if X8 shows >= 3%.

### 6.6 Memory: alias analysis and forwarding (`-fwd`)

Access classes and their store chains: `stack` (none: slots are SSA values on the trace), `array` (`ALOAD/ASTORE`),
`hash` (full build: `HLOAD/HSTORE` via `HREFK`), `hdr` (`FLOAD`: asize, array, node, lsizenode, metatable, flags),
`upval` (`ULOAD/USTORE`). **TR13 (alias rules).**
1. Different classes never alias. In particular array and hash never alias: a given integer key lives in exactly one
   part of a table, the trace guards which one, and the parts cannot change on a trace except through a `NEWREF`
   (rule 6).
2. Table refs: *must alias* if they are the same ref; *no alias* if both are distinct fresh allocations of the trace
   (full build `TNEW`) or one is a fresh allocation that has not escaped (not stored anywhere, not passed to a helper);
   otherwise *may alias*. (Different tables of the program may well be the same object: `a == b` in `a[i] = b[i]`.)
3. Array refs (v1.3; v1.2's "same index ref -> must" on may-aliasing tables was unsound: a store `a[i]` would have
   been forwarded to a load `b[i]` with `a ~= b`): **must** only if the table refs must-alias (rule 2: same ref) **and**
   the index refs must-alias (same ref after folding); **no** if the indices are provably different (`x + c1` and
   `x + c2` with the same base `x` and constants `c1 != c2`, or two different constants), whatever the tables;
   **may** in every other case. Keys are normalised integers
   (float keys pass `F2I_EXACT` first), so `t[2]` and `t[2.0]` are the same key (a difference from LuaJIT,
   [V] design notes 6.2).
4. Hash refs (full build): same table ref and same constant key -> must; different constant keys -> no (whatever the
   tables); else may.
5. Upvalues: `ULOAD`/`USTORE` through the same `UREF` ref -> must; **every other pair may alias**, including two
   different upvalue indices of the same closure (`debug.upvaluejoin(f, 1, f, 2)` makes them share one `UpVal`; LuaJIT's
   "distinct upvalues of the same function" rule would be unsound for Lua 5.2+).
6. `NEWREF`, class-A helpers and any helper that can rehash kill every `hdr`, `array` and `hash` fact of every table
   that may alias their table (the array and node pointers move). Prototype: none exist.
7. Forwarding: a load returns the value of the nearest earlier *must-alias* store if no *may-alias* store lies between;
   else the nearest earlier must-alias load with no may-alias store between (load CSE); else it stays a load.
   Forwarded loads keep their type (the store's value type), so their typecheck disappears.
8. Forwarding a `nil` stored value is allowed only when `FLOAD(t, METATABLE) == 0` is guarded earlier on the same table
   ref (a real nil load would exit for `__index`).
9. Dead-store elimination: **not done** (every exit observes the heap; a store between two snapshots cannot be dropped
   without store sinking, which is part of 6.8).

### 6.7 What is never moved

No instruction is moved across a guard it depends on (all loads are below the guards that make them safe: type, bounds,
metatable). Stores are never reordered with respect to each other or to exits.

### 6.8 Allocation sinking (in scope by decision D1; late task F8 behind prototype check S0; kill switch `-sink`)

Our VM boxes nothing (numbers are inline in `Value`), so "boxed temporaries" do not exist; sinking removes `NEWTABLE`
allocations that do not escape the trace and materialises them at exits. `CLOSURE` sinking is not in J5 (open upvalues
would point into trace frames, TR15). Facts that shape the policy [D]:
- A sunk table is never allocated, so the collector's view differs from JIT-off: `collectgarbage("count")` (total bytes
  + debt) and the pacing (debt), hence when later steps, cycles and finalizers happen, cannot be bit-identical. The user
  accepted this (D1); TR29 states what stays identical and how the rest is bounded.
- No benchmark among the 25 is expected to gain (binary_trees' allocations escape; LuaJIT's `-O-sink`: no visible
  effect, [M] design notes 11.1). The value is in user loops with short-lived temporaries; S0 must show it.

**TR29 (GC-observable policy, with sinking on).** v1.1 overclaimed; this is what can honestly be promised.
1. *Identical to JIT-off, for every program:* error behaviour of the program's own operations (which instruction
   raises, message, line info, traceback, `pcall` results) except memory errors (3); instruction counts and
   count/line/call hook semantics (C1, TR5: sinking changes no bytecode count); `debug.*` results on frames, except
   2(c); the
   content, `#t` and `next` order of every materialised table (TR26c: same sizes, same insertion order). Output that
   does not depend on (2) is identical.
2. *May differ:* (a) GC-observable values: `collectgarbage("count")`, the result of `collectgarbage("step")`, the
   GC step/cycle at which a finalizer runs and hence the interleaving of finalizer side effects with program output
   *and with program state* (a finalizer that changes reachable data can make everything after it differ), the moment
   weak-table entries are cleared, generational ages; (b) **address-dependent output**: sinking removes allocations, so
   later objects get other addresses; `tostring`/`%p` of tables, functions, userdata and threads, and the `pairs`/`next`
   order of tables whose keys are such objects (their hash is the address), can differ. (b) is already not reproducible
   across JIT-off runs with ASLR and is not guaranteed by Lua; whether the user's acceptance of GC-observable
   differences covers it is open question N1 (default: yes, documented); (c) (v1.3, decision D7) what
   `debug.getlocal` shows for **dead or out-of-scope** slots after an exit (temporaries of popped inlined frames, slots
   above the active locals) when S0's writeback/sinking choice leaves such a slot holding a different (e.g. not
   materialised or older) value; live locals in scope are always exact (TR3).
3. *Memory limit:* every sunk allocation site keeps a **virtual allocation** count `vbytes` (SSA value, the bytes the
   interpreter's `new_table` would have charged for the NEWTABLE's sizes) and guards that the interpreter's
   `check_gc` after this allocation would not step and that `real bytes + pending virtual bytes + size` is within the
   memory limit; if either fails the trace exits *before* the NEWTABLE (snapshot materialises the live sunk objects),
   and tier 1 allocates for real with the interpreter's emergency-collection and error semantics. Hence a memory error
   is only ever raised by tier 1 or the interpreter at a real allocation, with the interpreter's message and position.
   Whether and where it happens can differ from JIT-off only through (2a) (a different live set at that moment).
4. *Bounded pacing drift (virtual debt):* at every exit and loop iteration end, `vbytes` of the *dead* sunk objects
   is charged to the collector as virtual bytes (`Heap::charge_virtual`, feature `jit_trace`, counted in total bytes and
   debt exactly like an allocation), and credited back (`uncharge_virtual`) when the collector completes the cycle after
   the one in which they were charged (incremental), at the next minor or major collection (generational, 5.5 and
   `gen54`), and fully at every full collection (explicit `collectgarbage()`/`"collect"`, emergency). Intended effect:
   step points stay close to where the interpreter's allocation volume puts them. **Target, not guarantee** [?]: for
   programs whose finalizers do not change reachable data, `count` after an explicit full collection equals JIT-off and
   `|count(on) - count(off)|` stays within the virtual bytes of the current and previous cycle. S0 measures both on the
   corpus; F8 documents the measured bound in `docs/JIT.md`.
5. Determinism: with a fixed seed, two runs in the same mode and process layout give identical output.
6. Everything in (1)-(5) holds with `-sink` trivially and *exactly* (no virtual bytes, no sunk allocation).

**TR26 (sinking soundness).**
- (a) *Sinkable:* a `TNEW` (NEWTABLE, full build F3) whose table never gets a metatable on the trace (always: no
  `setmetatable` on traces), whose every use is an `ASTORE/HSTORE/ALOAD/HLOAD/FLOAD` with the sunk ref as table, an
  identity compare (folds: a fresh table equals no other object), or a snapshot entry; that is not stored into any
  non-sunk table or upvalue, not passed to a helper, not a PHI operand (v1: lifetime within one iteration), and whose
  stores use constant keys or integer indices inside the NEWTABLE's array size. Everything else unsinks the
  allocation (it becomes a class-A `TNEW`, TR1). Sunk stores are not emitted; loads from a sunk table forward from its
  stores (TR13.2: fresh tables do not alias); a load of a never-stored key is `nil` (no metatable).
- (b) *Snapshots:* a snapshot entry whose ref is a sunk `TNEW` has flag `SUNK`; its contents are the sunk stores on the
  trace before the snapshot point in program order (found by scanning the IR, LuaJIT's method), their values in
  registers, spill slots or constants. The RA treats those values as snapshot uses (weak, 7.2).
- (c) *Materialisation* (Rust restore only; snapshots with sunk entries always use the generic exit): (1) write every
  non-sunk snapshot slot first (TR3), so every collectable held by the trace that is not inside a sunk object is rooted;
  (2) set `GCSTP_GC` (no step, no emergency collection while materialising; [V] collect.rs:205 uses the same flag);
  (3) create each sunk table with the interpreter's own `new_table` and the NEWTABLE's exact sizes, store it into its
  slot immediately (rooted), then replay its stores **in recorded order** through the interpreter's raw-set routines
  followed by the interpreter's `barrier_back` (a no-op for these white/young tables, kept so the code path is the
  interpreter's): equal sizes and insertion order give the same node layout, hence the same `next` order and `#t`;
  collectable field values are rooted from the moment they are stored, and nothing can collect before (2) is undone;
  (4) move their bytes from virtual to real accounting (TR29.4), clear `GCSTP_GC`; no `check_gc` here (the step points
  are the trace's guard in TR29.3). Colour and age: objects come from the normal allocator, so they get the current
  white and `G_NEW`, valid in every collector phase; a slot referencing them needs no barrier (stack, [V] design notes
  7.2). A host allocation failure during (3) (the limit was pre-checked by TR29.3) raises "not enough memory" at `P`
  with the frame consistent.
- (d) Side traces see sunk parent objects already materialised (they enter through the stack, 7.8).
- (e) Kill switch: `RUSTYNAIL_JIT_TRACE_OPT=-sink` disables the pass (and virtual accounting stays at 0, so GC
  observables are then exactly JIT-off, TR1/TR9); `-sink` is also what the GC-observable tests run with (9.8).

**Prototype check S0** (before F8 is merged; F8 can be dropped without affecting the core): (1) `bench/jit-trace/sink_*`
micro-benchmarks (temporary `{x=, y=}` vectors, tuple returns from inlined functions) >= 1.5x faster than `-sink`;
(2) no program of the 25 slower than `-sink` beyond the band; (3) official suite with sinking on except `gc.lua` and
`gengc.lua` (and files listed by triage, 9.8) green, those two green with `-sink`; (4) drift of `collectgarbage("count")`
on the corpus within TR29.4's bound, measured by a test that samples `count` after full collections (must be equal) and
at 1000 random points (bounded); (5) **writeback question (v1.3, X11):** S0 builds TR3b writeback and reports, on
the 25 programs and the `sink_*` benchmarks, the PHI count, `Compile(phi)` aborts and cycles with and without it,
and decides how a sunk table still held by a dead local at `S_H` is treated: materialise at writeback (no gain from
sinking for it), LuaJIT-style sunk PHIs (both PHI operands sunk allocations of the same site; a TR26a extension that
then needs its own review), or not sunk; the decision is recorded in section 13 before F8. Failing S0 drops F8
(decision D7: no fallback design) and records the reason in section 13.

### 6.9 GC interaction: the rules that make a trace GC-safe

- **TR1 (nothing runs on a trace).** Between a trace's entry and the end of its exit handler, no Lua code, no GC step,
  no finalizer, no emergency collection, no stack reallocation and no hook runs. Helpers callable from trace code are
  classified: **N** (pure, reads only: `pow_lua`, `fmod`, `str2number`), **S** (stores without allocation:
  `barrier_back_slow`, the upvalue forward barrier slow path), **A** (full build: allocates *without* a GC step: it
  computes whether the interpreter's `check_gc` after this allocation would step and whether the allocation could fail,
  and if either holds it returns `H_EXIT` having done nothing, so the trace exits before the instruction and tier 1
  performs allocation, step, emergency collection or error exactly like the interpreter). Prototype: N and S only.
- **TR9 (G1 on traces).** G1 (no collectable only in a register across a GC point) holds vacuously on a trace: there
  is no GC point on a trace. Collectables (tables, strings, closures) may therefore live in registers on the trace. At
  exits all snapshot values are written to slots before anything that can allocate (the restore allocates
  nothing except sunk objects, TR6b; materialisation of sunk objects follows TR26c: slots first, then allocation
  under `GCSTP_GC`), and inline stubs only store.
- **TR10 (barriers).** Every table store whose value is not statically non-collectable evaluates the interpreter's
  `barrier_back` condition inline (table `BLACKBIT`, value collectable and white, [V] J1d) and calls the slow path
  (class S) when it holds; generational ages (`touched1/2`, gray-again list) stay inside the Rust slow path ([V]
  design notes 7.2). Stores of `Int/Num/Bool/Nil` skip it. `USTORE` of a collectable value applies the interpreter's
  upvalue barrier condition the same way (prototype: only non-collectable `USTORE`). Barriers are never hoisted or
  CSE'd: the colour of a table can change by a barrier of the same trace, and the next store's value may be white.
- Stack: neither moved nor shrunk on a trace (no GC, no growth: TR6c); `rbx` is valid for the whole activation.
  Weak tables and ephemerons: cleared only at GC points, never on a trace. Finalizers: never on a trace; objects with a
  `__gc` metatable cannot be created on a trace (`setmetatable` is a C function).
- Recording itself runs in the interpreter, so it inherits every GC behaviour of the interpreter.

## 7. Register allocation and code generation

### 7.1 Registers

Pinned (unchanged): `rbx` root frame base, `r12` thread, `r13` root `CallInfo`, `r14` root `kcache`, `r15` budget
word / exit area base, `rbp`, `rsp`. **`V7` (`rax`) is the trace scratch register and is never allocated** (64-bit
immediates for `EQREF` and constants, dispatch and stub address jumps, counter increments, the interrupt-flag address,
and the clobber of `FNeg/FAbs/SDivRem/ReloadFrame`). Allocatable: the eight `V0..V6, V8` (`rsi rdi r8 r9 r10 r11 rcx
rdx`) with the constraints of [V] ir.rs (variable shifts need `V6`; `SDivRem` clobbers `V8` and needs operands in
`V0..V6`) and `X0..X15` (`X0/X1` clobbered by `FNeg/FAbs/F2IExact`, so they hold values only between
such ops). Constants of inlined protos are IR constants (folded immediates or materialised), never read through
`r14` (which is the root's `kcache`).

### 7.2 Reverse linear scan (LuaJIT's scheme, [V] design notes 4.1)

Walk the IR from the last instruction to the first, emitting micro-ops into a buffer that is reversed at the end. A
value gets a register at its last use (first seen), is freed at its definition. Eviction picks the value with the
lowest defining ref (longest remaining range) with a large bonus for PHI and loop-carried values. Spilled values get a
slot in `ExitArea.spill` (`[r15 + SPILL + 8k]`, `Base::Cnt` + offset is an existing addressing mode, [V] ir.rs), the
spill store is emitted at the definition; constants are rematerialised. Hints: two-address destination = first
operand; PHI ends hint each other; invariant values prefer registers the body does not write.
**Exit consistency (TR27).** The register/spill location of every ref in a snapshot is fixed when its first guard is
assembled; a later rename of a snapshot ref before another guard of the same snapshot forces a spill (LuaJIT
`asm_snap_checkrename`). The exit map of each guard is derived from the allocator state at that guard, and the
`+verify` assembler asserts that every guard of a snapshot sees identical locations.
Snapshot-only refs are *weak*: they get a register only if one is free, else a spill slot.

### 7.3 PHIs and the back-edge

At the end of the body, the parallel move "PHI destination <- body ref" is resolved with the textbook algorithm
(sequentialise acyclic moves, break cycles with the scratch `V7` for GPR cycles and a spill slot for XMM cycles: no XMM
register is reserved). The algorithm is a pure
function with a property test (random permutations with aliasing, Miri). Abort `Compile(phi)` if more than 64 PHIs.

### 7.4 Spills

Up to 256 spill slots (`Compile(spill)` beyond). The spill area is per state (TR2: one activation at a time); the
exit routine copies it into `ExitArea` implicitly (it *is* `ExitArea.spill`).

### 7.5 Helper calls on a trace

`CallH` clobbers every `V` and `X` ([V] ir.rs). A class N/S helper call is bracketed by "store every live allocatable
register to `ExitArea`, call, reload" (cold paths: barrier slow path; hot paths: `pow`). Arguments that are not
32-bit immediates are passed through two argument words in `ExitArea`. No `ReloadFrame` is needed (TR1: these helpers
cannot move the stack); `+verify` asserts the stack pointer is unchanged after each.

### 7.6 Exit stubs

- **Inline stub** (snapshot without inlined frames): `for (slot, loc, ty) in map: St B1 [rbx+16s] tag; St/StF
  [rbx+16s+8] loc; ` budget (TR5.4, through `V7`: `MovR`/`Add` then `St B4 [r15]`); exit counter: `Mov V7, &ctr[t][k];
  Ld V8', [V7]; Add V8', 1; St [V7], V8'` using a register that is dead at this exit (every snapshot value is already
  stored), plus the compare with the side-exit threshold (full build: hot -> shared exit routine); then `Mov V7,
  imm64(tier-1 label of P); JmpR V7`
  (TR7b embedding). About 2 stores per slot plus 6-8 ops: 5-20 cycles.
- **Generic stub**: `St B4 [r15 + TRACE_ID] t; St B4 [r15 + EXITNO] k; Mov V7, imm64(shared_exit); JmpR V7` (5.6). Used for snapshots with inlined frames and for hot
  inline exits (full build). Measured cost target < 300 cycles (X2).
- Stubs are emitted out of line after the loop, deduplicated by `(snapshot, location map)`.

### 7.7 Trace entry

Entered from `cell[H]` with `rbx/r12/r13/r14/r15` valid and the frame exact (tier 1 keeps everything in slots). In
order, each failure jumping to `pcl[H]` (pre-count tier-1 label of the same version, embedded, downstream of `D_H`,
TR20) with nothing changed except the `TrProto` counters of 3.3:
- **TR6 (entry checks).** (a) TR6c stack room: `thread.stack_size - (ci.func + 1 + room) > 0` with `room` the maximum
  of 4.4's per-site requirements (strict, as the interpreter's fast paths); (b) TR6b: `depth` spare `CallInfo` nodes
  after `ci` (`depth` loads of `next`, non-null; `depth` counts intrinsic native frames); (c) the budget check of the
  pre-roll (TR5.2); (d) the pre-roll's `SLOAD` typechecks, callee-identity guards and upvalue-range guards, which are
  ordinary guards whose snapshot is snapshot 0 (state at `H`, empty map) and so also land on `pcl[H]`. Failures of
  (a)-(b) and (d) count `entry_fail` (3.3, free-peel rule).
- **TR6d (which guards may use snapshot 0, v1.3).** A guard is placed in (d) with snapshot 0 only if **every operand
  is an entry value**: an `SLOAD`, a constant, or a load (`ULOAD`, `FLOAD`, `ALOAD`) of entry values with **no
  may-alias store** (TR13) on the recorded path before the guard's original position. Otherwise the guard stays at its
  instruction with that instruction's snapshot (its exit re-executes only that instruction). Example: a callee whose
  closure is read from an upvalue after a `USTORE` to an upvalue that may alias it (`debug.upvaluejoin`) keeps its
  identity guard at the `CALL`. Snapshot 0 re-executes the whole iteration from `H`, so it is valid only for guards
  that precede every effect of the trace, which this placement guarantees. Applies to the prototype (P3b marks the
  guards, P5b places them; mutant `entry_guard_past_store`) and to every full-build entry (F2a entry-0 side traces,
  F2b links re-checking a target's entry guards).

### 7.8 Linking (full build unless noted)

- **Rolled root loop (v1.3; `-loop`, `trnoopt`, and every trace before P7 exists, i.e. PI).** Layout: `[entry
  checks TR6a-b] TOP: [POLL, BUDGET n, snapshot 0] [the recorded iteration as recorded: SLOADs with their typechecks at
  first use, guards with their own snapshots, invariant guards not hoisted] [writeback: tag + payload of every slot
  written in the iteration (inlined-callee slots included, TR3), from its final ref] Jmp TOP`. No `LOOP` marker, no
  PHIs: the stack carries every loop-carried value (TR3b, degenerate case), so each iteration's `SLOAD`s re-read the
  values written back, and the sparse-snapshot rule holds unchanged (the stack holds every value not redefined in the
  current iteration). `TOP`'s guards use snapshot 0 (empty map, lands on `pcl[H]`, uncharged budget: `fetches = 0`,
  `land = 0`), which is exactly `S_H` because after the writeback the whole state at `H` is in the stack; there is no
  separate pre-roll budget check. The writeback stores cannot fail, so TR27b holds trivially (no guard after them).
  Owners: P3a produces the closed iteration with `loop_at = None` and the writeback list (the slots written and their
  final refs, identical to the `S_H` map); P5b assembles it; P7 replaces it with the peeled layout when `-loop` is not
  given. Slower than peeling (typechecks and stack round trips per iteration); used for correctness modes and as the
  baseline of the X9 `-loop` ablation.
- **Peeled root loop** (prototype, with P7). Layout: `[entry checks] [BUDGET n0, snapshot 0] [pre-roll] LOOP: [POLL, BUDGET n,
  snapshot S_H] [body] [PHI moves] Jmp LOOP`. **TR27b (loop-end order).** No guard sits after the PHI moves: the poll
  and the budget check of the next iteration are the first instructions *after* `LOOP`, and their shared snapshot
  `S_H` is the full state at `H` after an iteration: every slot written in the pre-roll or the body maps to its PHI
  ref (or invariant pre-roll ref), whose locations are exactly the PHI destinations (the moves have completed). `S_H`
  exits land on `pcl[H]` (pre-count) with the uncharged budget (TR5.4 with `fetches = 0`, `land = 0`). The pre-roll's
  own budget check uses snapshot 0 (state at entry, empty map). The PHI moves themselves cannot fail, so TR27 (no
  rename between guards of one snapshot) is unaffected; the RA treats the `Jmp LOOP` edge as the point where PHI
  destinations become the locations of `S_H`. With TR3b writeback (S0/F8), the written-back slots are stored before
  the PHI moves and are omitted from `S_H` (the stack holds them).
- **Side traces** start at a hot exit: the exit is restored to the stack first (inline stub semantics), then the side
  trace is recorded from `P` with the parent snapshot's types as known types (its `SLOAD`s of slots in the snapshot map
  need no typecheck). Entry from the parent: the parent stub's final jump goes through `exit_cell[t][k]` (TR8) instead
  of the embedded tier-1 label. Passing values through the stack costs a few store-forwards per exit, but keeps the
  two allocators independent (LuaJIT passes registers; deferred, X4). A side trace ends when it reaches a loop head
  with a trace (jump to that trace's entry, which re-checks its types: link), or its own parent's head, or after
  `maxrecord`; reaching an untraced head aborts.
- **Nests**: when recording an outer loop reaches the head of an inner loop that has a root trace, the outer trace
  ends with a link into it (`InnerLoop` in the prototype). If the inner loop head belongs to an *inlined* frame of the
  outer trace (heapsort's `sift`), the link first materialises the inlined frames exactly as an exit does (5.5) and
  then enters the inner trace (whose root frame is that callee): a "linking exit".
- **Stitching** at a C function call: the trace ends with an exit at the `CALL` (re-execute in tier 1); tier 1 performs
  the call and runs on to the next dispatch (`H`), which enters the trace again. No continuation traces in J5.
- **As built (F2b, docs/JIT_TRACE_REF.md "F2b decisions").** Every link goes through the target head's dispatch cell (TR8), so the target always re-checks its own
  entry and a flush of it needs no patching of the sources. Nest roots (same frame) are on by default (`nest=1`) and are roots in the cache that end with the F2a
  link epilogue; the loop-exit guards of the inner trace get side traces that continue through the rest of the outer iteration (a side recording now continues behind a
  `FORLOOP` that ends, `FORPREP` of integer loops is recorded in side traces and nest roots) and link back to the outer trace; exits parked on an untraced head wait for
  the next root trace of the prototype. Linking exits with frame materialisation exist (`nest=2`: `ExitSnap.link_cell`) but are off by default: the generic handler
  costs more than the tier-1 call they replace (heapsort +15%, short_inner +31%). Stitching exists (`stitchmin`, default 32) and is not used by any of the 25 programs.
- Every link and every loop back-edge polls (TR22).

### 7.9 Exit-count policy (full build)

Exit counter per (trace, snapshot); at `exit threshold` (default 16, `always` 2) the shared exit routine starts a side
recording at `P` (unless `maxside` = 64 side traces for this root or the exit failed 4 times: then it is blacklisted and
keeps landing in tier 1). Root entry failures are exit 0: a hot exit 0 records a side trace from `H` with the new
types (LuaJIT's handling of persistent entry mismatches).

**As built (F2a, docs/JIT_TRACE_REF.md "F2a decisions").** Side traces start only at frameless snapshots (exits with inlined frames keep landing in tier 1; S1 later added side traces whose frames stay virtual, docs/JIT_TRACE_REF.md "S1 decisions");
the exit is entered through its `exit_cell` in the state of the tier-1 label of its pc; `maxside=0` is the prototype's policy. A side trace ends with a link
through the head's dispatch cell (a prelude, below, through the head's `entry`). Entry-0 side traces whose root fails its entry check about as often as it is
entered become the head's **prelude** (the dispatch cell targets them; their failing checks continue in the root's entry), which is what removes type_entry's
second exit per run. A persistent entry failure still flushes and re-records the root (the side trace of exit 0 would ping-pong with the failing entry).

### 7.10 Growth limits and flush policy (TR20b)

`maxtrace` 1000 per state, `maxrecord` 4000 IR, 500 snapshots, `maxside` 64 per root, side-trace length 1000 IR,
`maxdepth` 4 (prototype) / 8, recursion unroll 0 / 2, extent 200 slots, code: the existing per-state code cap
(`RUSTYNAIL_JIT_CODE_CAP`, [V] J3c). Reaching `maxtrace` or the code cap flushes all traces of the state (3.4) with a
global penalty; `RUSTYNAIL_JIT_TRACE_LIMITS=name=value,...` overrides (tests).

## 8. Lua semantics hazards

| hazard | rule on traces |
|---|---|
| integer `for` (5.5 precomputed unsigned iteration count, [V] FORPREP templates) | `FORLOOP` uses the count exactly as `t_forloop` (guard count != 0); no overflow checks (wrap); `FORPREP` is not on the path of a root trace of the loop (it precedes `H`); inside side traces it is recorded as tier 1's typed path (zero step and non-numbers exit) |
| float `for` | `idx += step` then the compare with the sign of the step decided on its bits as J4 T4b (`0 < bits <= 0x7FF0...`, so NaN/zero steps behave as `luai_numlt(0, step)`); step sign is an invariant guard |
| string -> number coercion | prototype: abort; full: `STRTO` (class N, guarded result type), the same conversion function as the VM |
| integer wrap | all integer ops wrapping; constant folding wraps (TR12) |
| `//` and `%` | integer: divisor guards and `-1` as `divmod_leaf` (J4 M5); float: `CALLN` of the VM's own functions |
| NaN, -0.0 | TR11; float keys normalised by `F2I_EXACT` (`-0.0` -> key 0, as the VM) |
| int/float compares | mixed compares as tier 1: exact within +-2^53, otherwise exit (no guard on a lossy conversion) |
| metamethods | TR14; every operand/table state that would call a metamethod function aborts the recording or exits |
| upvalues | **TR15**: `GETUPVAL/SETUPVAL` through `UpVal.v`; an `UpVal.v` inside the root frame `[root base, root base + nslots)` is **the register** (N1: recorded as `rd`/`def` of that slot under an `EQ UVREL, 16*slot` guard; the slot is an SSA value on the trace), one inside the callee area `[nslots, extent)` aborts (`UpvalAlias`); the range check is a guard in the pre-roll (hoisted); all upvalue pairs may alias except the same `UREF` (TR13.5) |
| varargs, tbc, `CLOSE` | not on traces (abort) |
| coroutines | a trace never spans a yield/resume (TR1); a trace may be entered in any coroutine (state per activation) |
| `debug.sethook/setlocal/upvaluejoin/setupvalue`, `setmetatable` | only off-trace; caught at the next entry by guards (3.6) |
| stack reallocation (`stackmove` stress) | impossible on a trace; `RUSTYNAIL_JIT_STRESS=stackmove` still moves the stack at every tier-1 helper, and a new `+tracemove` option moves it right before every trace entry (catches a stale base across entries) |
| GC at any allocation | TR1 class A (full build): exit before the allocation when a step is due |
| errors | never raised on a trace: every error case is a guard exit, tier 1/interpreter raises with exact message and line |
| 5.4 | **TR30**: the prototype refuses 5.4; F7a/F7b add 5.4 (same `tr` modules, `cfg(rustynail_lua54)` arms as in tier 1): `FORPREP/FORLOOP` 5.4 layout (`R[A+3]` control copy stored every iteration), `TFORPREP/TFORLOOP` 5.4 layout, `SELF` with `RKC`; every integer-key array access (`ALOAD`, `ASTORE`, the `ipairs` step) records an `ATOUCH t u` (class S[hint]) right after its bounds guard that performs `alimit_touch` exactly as `templates_table.rs` (`BITRAS`, `LENHINT`, monotone and idempotent: CSE'd only with an identical `(t, u)`, never hoisted, kills `LEN`/hint loads of may-alias tables, never sunk away: a sunk table has no hint to touch until materialised, and materialisation replays the stores through `psetint`, which touches); GC: `gen54.rs` barriers have the same condition (TR10) and the virtual accounting of TR29.4 hooks the 5.4 cycle ends; `OP_CALL`/`CallInfo` fill as the 5.4 tier-1 flat call |

## 9. Verification plan

### 9.1 Unit and property tests (Miri-runnable, `crates/rn-jit/tests/tr_*.rs`)

1. `tr_ir`: IR builder, interning, CSE chains, printer round trips.
2. `tr_passes`: random well-typed IR generators (int/float/array/upvalue ops, guards, PHIs); for each pass and each
   `-pass` combination: `interp(before) == interp(after)` on random inputs (exits, snapshot states, final states).
3. `tr_ra`: random IR -> RA -> micro-ops -> emulator == `tr::interp` on random frames; parallel-move property test;
   exit-consistency assertion (TR27) on every generated program; `+randra=SEED` (random eviction and register refusal).
4. `tr_exit`: restore of snapshots with 0-4 inlined frames on synthetic frames/`CallInfo` lists: slots, `CallInfo`
   fields equal to what `pre_call_lua_fast` writes (compared against running the real call on a twin state), budget
   refund, landing label.
5. `tr_record`: the recorder on hand-written programs: expected IR (golden dumps), abort reasons, count events vs
   `Lowered::sig` (TR5.1).

### 9.2 Recorder-vs-interpreter lockstep

Always on during recording (4.5); `+verify` turns mismatches into assertion failures. Every twin program runs once with
`RUSTYNAIL_JIT_TRACE_OPT=+verify`.

### 9.3 Twin programs (`tests/tr_core.rs`, the `t15_core` harness pattern)

Each program runs with `RUSTYNAIL_JIT=off` and with `RUSTYNAIL_JIT=always RUSTYNAIL_JIT_TRACE=always` (and `on` with
thresholds 2) under budgets none/3/17/200 (compared on output; under budgets additionally the identity and the
no-overshoot rule of 9.5, not equal `Stats::insns`), GC stress 1/2 (+VERIFY on small ones), stackmove, `+tracemove`, every
`-pass`, `+randra=1,2,3,7`, `+exitstorm=1,2,7,61` (every guard fails every N-th execution: exercises every exit map and
landing label), `+emurun` (9.6). Required programs: type change at iteration N in {1, 2, 3, 100} of a live-in, an array
element, a loop-carried accumulator, an inlined callee's argument and result; callee identity change mid-loop (a
function variable reassigned at iteration K, also to a C function and to a table with `__call`); inline depth limits;
NaN/-0.0/inf/2^53/maxinteger values through every float/int op; aliasing arrays (`a[i] = b[i] + 1` with `a == b` for
some calls; v1.3: store `a[i]` then load `b[i]` at the same index with `a ~= b` and with `a == b`, TR13.3); the two
type-stability patterns of 3.3 (an accumulator `local a = 0` that becomes a float, entered from an outer loop; an
array whose element type changes after the inner loop was recorded) with VERBOSE checks: the free-peel failures are not
counted, the hot type-guard exit causes exactly one re-record, no head goes `Black`; the rolled layout (`-loop`) for
every program; arrays with holes, out of bounds, metatables appearing mid-run; upvalues closed/open, `upvaluejoin` of a
loop's upvalue mid-run; count hooks/line hooks installed from a callee and from a finalizer while the loop runs;
`debug.setlocal` from a callee; coroutines running the same traced function and yielding between calls; deep recursion
around traced loops (stack growth between entries); error in the iteration after many traced ones (`xpcall` +
traceback: line numbers and locals); Ctrl-C (`INTERRUPT`) inside a trace.

### 9.4 The trace oracle (extends the J4 VM-truth oracle)

Feature `tr_oracle` of rn-jit (requires rn-core `t15_oracle`, OFF by default, [V] oracle.rs). Clock = interpreter
fetches (oracle callback count) + `Stats::insns` (TR5 identity). Run 1 (`TRACE=always`, oracle callback counting
fetches): at every trace exit, after restore, log `(clock, proto, pc, frame digest)` where the digest covers every slot
of every frame up to `ci.top` and every slot of `[root base, root base + extent)` (TR3: popped callees' leftovers) (tag; payload for numbers/booleans; content for strings; for other collectables only the
tag) and the `CallInfo` chain (proto, savedpc, func). Run 2 (`JIT=off` with the callback): at instruction number
`clock` the interpreter must be at the same `(proto, pc)` with the same digest. Both runs in child processes with a
fixed seed (the J4 oracle harness, [V] `t15_oracle.rs`). Corpus: `tests/vm`, `bench/programs` (small N), jit-diff
generator seeds, `tests/tr_oracle_progs/*.lua` (review repros). This checks TR3 and TR5 at every exit of every trace
without any lockstep of the heap.

### 9.5 Instruction-count identity and no overshoot

`tr_core::instruction_count_identity` (feature `tr_oracle`): for every program, every mode (`JIT=on/always`, `TRACE=
on/always`, every pass mode) and every budget (none/3/17/200): `fetches(off) == fetches(run) + Stats::insns(run)`.
`tr_core::no_overshoot`: a test-only counter per stay checks that compiled instructions of a stay never exceed its
budget and that every budget exit is at a tier-1 `exit_at(head)` label with the TR5.4 budget value (checked by `+verify`
assertions in the exit paths). `Stats::insns` is **not** compared between `TRACE=off` and `TRACE=on` (recorded
iterations and interpreted callees move counts, 3.7). Precondition X6 is checked in P0x for tier 1 and J4 first; a
failing identity there is fixed or explained before TR5 is relied on.

Note (P0x/P0y, tier 1 / J4 identity now exact, `crates/rn-jit/tests/tr_identity_pre.rs`): the `fetches` model must
reproduce two chains the interpreter executes WITHOUT fetching: the `TFORCALL` after a `TFORPREP` (and the `TFORLOOP`
after that), and the `TFORLOOP` after a `TFORCALL` (`do_tforcall!`/`do_tforloop!` in vm_unsafe.rs). Tier 1 counts
them, so a `Lowered::fetches` that only subsumes `Dead` pcs falls short by about one per generic-for iteration. P3a
implements the chains; where the iterator raises or yields inside the `TFORCALL` the chained `TFORLOOP` does not run.

### 9.6 Emulator differential

`+emurun`: every trace runs in the emulator instead of natively (the shared exit routine and the Rust restore are
shared code; tier-1 code stays native). jit-diff mode `tremu` compares it with the native trace run (stdout, stderr,
exit code, `jit_stats` exit counts). `+emuverify`: for traces without stores, at each entry the frame is copied; after
the native run reaches an exit, the emulator re-runs the same trace from the copy (the heap is unchanged: no stores)
and must reach the same exit with the same restored frame. Native vs emulator per op is the J4-T1 `emu_ops_diff`.

### 9.7 Mutation tests (feature `tr_mutants`, `RUSTYNAIL_JIT_MUT=name`, each must fail a test)

`snap_drop_last` (omit a snapshot entry), `snap_prune_dead` (prune dead slots), `refund_off_by_one`, `land_pre_mid_swap`,
`budget_no_check`, `budget_no_store`, `phi_skip_move`, `phi_swap`, `spill_no_reload`, `exit_rename_ignored` (TR27),
`fcmp_negated`, `fold_nan`, `fold_x_plus_zero`, `alias_may_as_no` (array), `alias_upval_by_index`, `fwd_across_store`,
`alias_maytable_as_must` (v1.3: same index ref treated as must on may-aliasing tables, TR13.3), `cse_across_store`,
`tbar_skip`, `entry_no_extent`, `entry_no_ci_spare`, `entry_no_typecheck`, `callee_guard_skip`,
`upval_range_skip`, `poll_skip`, `frame_savedpc_caller_off` (caller's savedpc = CALL instead of CALL+1),
`frame_nresults_off`, `dep_edge_skip` (with a proto-churn test that reuses addresses), `flush_in_handler` (flush
not deferred, TR18), `peel_subst_stale`, `abc_with_store` (if ABC is built); sinking (F8): `sink_escape_ignored`,
`sink_store_order`, `sink_slots_after_alloc` (materialise before writing the other slots), `sink_no_gcstp`,
`popped_frame_drop` (TR3), `native_room_skip` / `native_ci_skip` (TR6b/c at intrinsic sites), `hooked_head_no_trap`
(TR23.2), `rec_no_validate` (TR23.5), `phi_before_guard` (TR27b),
`vbytes_no_credit`, `vbytes_no_limit_guard`; 5.4 (F7a): `atouch_skip`, `atouch_hoisted`, `forloop54_no_ctl_copy`;
v1.3: `entry_guard_past_store` (TR6d), `leave_trap_kept` (TR23.4), `nonslot_phi_skip` (TR24e), `rolled_no_writeback`
(7.8), `dep_edge_at_install` (TR32, with the F3 GC-during-recording test), `cell_stale_version` (TR19b(c), F5). The
runner is the T12 child-process `every_mutant` test (P8a). **Ownership (v1.3):** P0 freezes the name registry
(`tr/mutants.rs`); each mutant's injection point is written by the task that owns the code it breaks (table in 10.1),
and that task's acceptance includes "its mutants are caught by its own tests"; P8a only adds the runner and the
whole-suite check.

### 9.8 Harness, fuzzing, sanitizers, suites

- **jit-diff modes**: `tr` (`JIT=always TRACE=always`), `tron` (`JIT=on THRESHOLD=2 TRACE=on HOT=2,2`), `trstorm`
  (`+exitstorm=7`), `trrand` (`+randra=<seed>`), `trverify` (`+verify`), `tremu` (`+emurun`), `trnoopt`
  (`-fold,-cse,-loop,-fwd,-abc`), and, in the full build, `trt15` (trace + regions). `JITDIFF_ENTERS=1` additionally
  sums `[jit] trace: recorded N entered N exits N` per (hook variant, mode), aggregated over the seed range as the
  existing `NOENTERS` check is, and fails with `NOTRACES` when a trace mode never entered a trace. **v1.3:** under the
  prototype opcode subset most generated programs abort (hash fields, globals, C calls), so a per-seed check would
  report `NOTRACES` on most seeds and an aggregate check alone proves little; the driver therefore also counts the
  seeds whose run entered at least one trace and fails with `FEWTRACES` when that fraction is below 25% in `tr` and
  `tron` (the threshold is checked on seeds 1-2000 at the P-gate and recorded), and the generator gains
  **prototype-shaped templates** (below) weighted so that most seeds contain one.
- **Prototype-shaped templates (v1.3, P8b):** numeric `for`/`while` loops inside local functions over integer/float
  arrays with no metatable, upvalue accumulators, invariant float subexpressions with `math.sqrt` from an upvalue,
  recurrences `t[i]` from `t[i-k]`, may-alias array pairs, calls to small local functions (1-4 levels) with fixed
  argument/result counts, type changes of accumulators and elements after iteration K; values printed after the
  loop.
- **New generator templates** (`tests/jitdiff/gen.lua`): small local/upvalue functions called in hot loops (1-3
  levels), callee reassigned at iteration K (Lua/C/`__call` table), polymorphic arguments, result count mismatches,
  aliasing arrays, array recurrences, nested loops with inner trip counts 0..12, `while`/`repeat` with rare branches,
  intrinsics with special values and a replaced `math.sqrt`, upvalue writes from inlined closures, `upvaluejoin` mid-loop.
- **Fuzz**: new target `trace_ir` (random well-typed IR -> passes -> RA -> emulator vs `tr::interp`, ASan, also a
  Miri smoke); `jit_diff` config byte gains trace mode, pass-disable mask and exit-storm period (the J4 extension
  pattern); `run_source` gains the trace bit. Builders run 60-minute campaigns per target at the P-gate and at F6;
  the **24-hour campaign is run by the user after J5 is complete** (decision D3), not by a builder task.
- **ASan**: `scripts/asan-jit.sh` with `TRACE=always` (official suite, corpus, rn-jit tests). **Miri**:
  `scripts/miri.sh jit` gains `tr_ir tr_passes tr_ra tr_exit` and the emulator trace tests.
- **Suites**: official 5.5 31/31 with `JIT=always TRACE=always` (and with GC stress 1), `tests/vm` corpus x9 modes,
  workspace tests with `jit,trace`, clippy (5.5/5.4/jit/trace/mutants/oracle), fmt, `jit-text-identity.sh`,
  `jit-dump-identity.sh` with `TRACE=off`, the fast-copy symbol identity of TR23.
- **GC-observable split (sinking on, TR29).** The official suite runs with sinking on, except `gc.lua` and `gengc.lua`
  (5.5; `gc.lua` and `gengc.lua` in 5.4), which run with `RUSTYNAIL_JIT_TRACE_OPT=-sink` (and once with sinking on as
  an informational run: no crash, no heap-verify failure; assertion failures on counts/finalizer timing are listed). Any
  further official file that fails only because of GC timing with sinking on is added to the `-sink` list after triage,
  with the failing assertion recorded in `docs/JIT_TESTING.md`; it must pass with `-sink`. The `tests/vm` corpus uses
  the same mechanism (a `sink_off` list next to `skip_heavy`, [V] vm_corpus.rs). **jit-diff**: generator templates that
  print GC observables (`collectgarbage("count")`, `"step"` results, output from `__gc`, weak-table contents before a
  full collection) prefix those lines with `GCOBS:`; in modes with sinking on the driver drops `GCOBS:` lines before
  comparing, and every seed is also run in a `trnosink` mode (`-sink`) that compares everything. **Every** line that
  prints GC state is prefixed, also after an explicit `collectgarbage()` (finalizers with side effects make equality
  there unguaranteed, TR29.4). Address-dependent output (`tostring` of tables/functions/userdata, `%p`, `pairs` over
  tables with address-hashed keys) is already avoided or normalised by the generator ([V] JIT_TESTING "Harness pitfalls").

### 9.9 Experiments the prototype must run (decide the [?] items)

X1 recording slowdown of the hooked copy per recorded instruction and the feature-on fast-copy cost (TR23 gates);
X2 cycles per exit: inline stub vs generic; X3 entry cost vs inner trip count 1..20 (`bench/jit-trace/short_inner`), and
mandelbrot's per-pixel entry+exit share; X4 stack-passing vs register-passing side-trace entries (full build); X5
trace-first vs region-first ownership over the 25 programs (full build); X6 the fetch+insns identity for tier 1
and J4 (task P0x, before P3a); X7 hot thresholds (64/256/1024) vs compile time and short-script cost; X8 `Ult` single-compare bounds and ABC;
X9 `-loop`, `-fwd`, `-cse` ablations per program (`-loop` = the rolled layout of 7.8); X10 code bytes per program vs
J4; X11 (S0) TR3b writeback: PHI count, `Compile(phi)` aborts and cycles with and without writeback, and the treatment
of sunk tables in dead locals at `S_H` (6.8 S0 item 5).

### 9.10 New knobs

| variable | values | meaning |
|---|---|---|
| `RUSTYNAIL_JIT_TRACE` | `on` (**default in every build with the `jit` cargo feature** after F9, D3; `off` on `main` until then), `off` (the documented off switch: exactly J4 behaviour), `always` | trace compiler; `always` = record at the first back-edge, side traces at the second exit, deterministic penalties. Facade: `Builder::jit_trace(bool)` / `rn_jit::set_trace` (per state); the `sandbox()` preset keeps the whole JIT off as today |
| `RUSTYNAIL_JIT_TRACE_HOT` | `loop[,exit]` | thresholds (default 256, 16) |
| `RUSTYNAIL_JIT_TRACE_OPT` | `-fold -cse -loop -fwd -abc -sink +randra=S +exitstorm=N +verify +emurun +emuverify +tracemove` | pass switches (all passes on by default once built; `-loop` selects the rolled layout of 7.8; `-sink` is the GC-observable kill switch, TR26e) and stress modes. P0 freezes the option set as `TraceOpts` in `tr/mod.rs`; P6 parses it; each option's effect is implemented by the owning task (10.1) |
| `RUSTYNAIL_JIT_T15` | `on` (default with `jit`, D5), `off`, `always` | J4 regions; with traces on they own only nests the tracer blacklisted (TR19) |
| `RUSTYNAIL_JIT_TRACE_LIMITS` | `maxtrace=..,maxrecord=..,maxsnap=..,maxside=..,maxdepth=..,unroll=..,loopunroll=..` | limits (tests); P-E: `maxdepth` (default 4, at most 8 = `exit::MAX_FRAMES`) is now wired into the recorder, `loopunroll` (default 16) bounds the backward edges of unrolled callee loops; R1: `monwarm` (default 2; 1 = only when the entry back-off refused; 0 = off) first-tick entry and counter credit for loops with calls, `tailfast` (default 1) inline `TAILCALL` of tier 1; M1: `closeret` (default 1) `RETURN` / `TAILCALL` with the close flag in inlined frames and the tier-1 `k` fast paths, `sidestitch` (default 1) / `sidestitchmin` (default 4) side recordings of loop roots stitch at recursive calls beyond `unroll` (the side recordings get `unroll`), `nilload` (default 1) nil array elements as `ALOAD` of type nil, `gforinl` (default 1; 5.5 only) `ipairs` / `pairs` loops of inlined callees unrolled (guarded end of the traversal), `loopany` (process default 1 since BUNDLE-S, struct default 0) backward edges of inlined `while` / `repeat` loops unrolled (`loopunroll` per recording), guarded by `loopanymax` (default 4: side attempts inside callee frames before the family is flushed and its loops are blocked; 0 = no guard), `depthstitch` (default 1) the inlining depth limit ends the recording with a stitching exit instead of aborting `Depth`, `fnloops` (default 0, experimental) function-entry heads for functions with loops |
| `RUSTYNAIL_JIT_TRACE_PREF` | `trace`, `region` | ownership order (full build, X5) |
| `RUSTYNAIL_JIT_VERBOSE` | `1`: summary (recorded, aborted by reason, entered, exits by trace/snapshot, flushes); `2`: one line per event (`[trace] 3 start proto:line pc`, `[trace] --- abort ... reason`, `[trace] 3 loop`, `[trace] 5 side of 3/7 -> link 3`) | |
| `RUSTYNAIL_JIT_DUMP=dir` | | per trace `trace-<n>.txt`: recorded bytecode path, IR before/after each pass, snapshots, RA, micro-ops; machine code via the existing dump |
| `RUSTYNAIL_JIT_MUT=name` | | mutants (feature `tr_mutants`) |

### 9.11 Shipping gates for `RUSTYNAIL_JIT_TRACE=on` by default (task F9; both 5.5 and 5.4)

All with the defaults that will ship (`JIT=on`, `T15=on`, `TRACE=on`, sinking on unless S0 failed), plus `always`
variants: official suite (5.5 31/31, 5.4 30/30; GC-observable split of 9.8; GC stress 1 with the known 5.4 generational
artifacts identical to JIT-off); `cargo test` workspace with `jit` and every rn-jit test incl. `tr_*`, under GC stress
1/2, stackmove and `+tracemove`; trace oracle and instruction-count identity on the full corpus; every mutant caught;
jit-diff 3000 seeds per version in all trace modes with `JITDIFF_ENTERS=1` and the hook variants, 0 diffs; 60-minute
fuzz per trace-relevant target, 0 findings; ASan (`asan-jit.sh` with traces) and Miri (`miri.sh jit`) clean; clippy,
fmt, rustdoc, deny; JIT-off `.text` identity and fast-copy symbol identity (TR23); no program of the 25 slower than
J4 (`TRACE=off`) beyond the band on either version; short-script suite within the band; `RUSTYNAIL_JIT_TRACE=off`
reproduces J4 byte-for-byte in `jit-dump-identity.sh`. The user's 24 h fuzz campaign follows, outside the builder tasks.

### 9.12 Lua 5.4 matrix (F7a/F7b)

Every `tr_*` test file, the oracle, the insns identity, the mutants and the twin programs under `--cfg
rustynail_lua54`, plus: `ATOUCH` sequences (random array access patterns with `#t` after each step, the
`jit_table::array_limit_hint_sequences` pattern, [V] J2b-5.4) under traces with forwarding and peeling on; 5.4 `for`
edge cases (control copy overwritten by the body, float loops, overflow); `gen54` collector modes with sinking on/off;
official 5.4 with `TRACE=always` (+ GC stress 1); jit-diff 5.4 binary seeds 1-1500 plain and 1501-1800 hook variants.
Prototype tasks must keep the 5.4 build compiling and refusing traces (clippy with the 5.4 cfg in every P task).

## 10. Task breakdown, review points, risks, decisions, effort

Builders are Sonnet, at most 2 concurrently, disjoint files, each brief with its rules quoted verbatim, the exact test
commands under `systemd-run ... MemoryMax=8G ... timeout`, stdin `</dev/null`, and "do not change files outside this
list" (plan s13). Opus reviews are marked **R**. Sizes: S < 600 lines, M 600-1500, L > 1500 (code + tests).

### 10.1 Prototype (P) tasks

Every task that adds unsafe code, embedded addresses or new machine-code/data contracts writes its `docs/UNSAFE.md`
paragraph **in its task notes** (cells, `JmpR` into tier 1 and the shared exit routine, the Rust `CallInfo` restore,
the exit area, ...); the orchestrator merges the paragraphs into the "J5" section after the task's review (v1.3: no
builder edits `UNSAFE.md` except P0's skeleton, because P4 and P5b run concurrently). Every P task also keeps the 5.4
build compiling with traces refused (clippy under the 5.4 cfg).

**Ownership of mutants and stress hooks (v1.3).** P0 freezes the mutant name registry (`tr/mutants.rs`) and the
`TraceOpts` option set (`tr/mod.rs`); P6 parses the options. Each mutant's injection point and each stress option's
effect is written by the task that owns the file it lives in, and that task's acceptance includes "its mutants are
caught by its own tests" (listed as "mutants/hooks" in the acceptance column). P8a/P8b build only runners, oracle, jit-diff and fuzzing.

Order: **P0 -> (P0b || P0x) -> (P1 || P2) -> (P3a || P6) -> (P3b || P5a) -> (P4 || P5b) -> PI -> (P7 || P8a) -> P8b
-> P9 (gate)** (at most 2 concurrent builders). Dependencies: P3a after P0x, P1, P2; P6 after P0, P2; P3b after P3a and
P6 (TR32 edge API, `TrId`); P5a after P2; P4 and P5b after P5a (register-save and spill layout), P5b also after P3b
(TR6d marks) and P6 (`TrProto` counter addresses); PI after P3b, P4, P5b, P6; P7 and P8a after PI; P8b after P7 and
P8a. Only one task edits `ir.rs/emu.rs/x86.rs` (P0b). Files shared between tasks are edited only sequentially
(`record.rs/snap.rs`: P3a then P3b; `print.rs`: P2 then P6; `tests/tr_passes.rs`: P2 then P7; `tr/mod.rs`: P0, PI, then
P7's pass-list line; `tests/tr_core.rs`: PI then P8a).

| # | task | files | acceptance / tests | size | review |
|---|---|---|---|---|---|
| **P0** | Orchestrator reference: freeze `tr::ir` types (`Ins`, `Snap`, `SnapEntry`, `FrameLink`), the `ExitArea` layout, the `RecVTable` interface, the count-model API on `Lowered` (`fetches`, label kind, `rem` per pc), the frozen `bench/jit-trace/*.lua` files (1.6 v1.3: set (b) `licm_expr array_rec upval_acc int_rec2`, (b') `int_sum float_poly float_loop dot upval_inv`, (c) `inline_leaf inline_2`, (d) `type_entry type_entry_stable type_late type_late_stable`, X3's `short_inner`; N and expected output of these and of spectral_norm/matmul), `PROGDIR` option of `scripts/bench_cycles.py`, the mutant name registry and `TraceOpts`, the UNSAFE.md J5 skeleton, and **one hand-written IR trace** for `for i=1,n do s = s + i end` lowered by a minimal hand-written lowering to micro-ops and run natively and in the emulator from a test-only entry (no recorder, no runtime, no RA): `docs/JIT_TRACE_REF.md` + reference code | `src/tr/{mod,ir,mutants}.rs`, `lower.rs` (API only), `bench/jit-trace/`, `scripts/bench_cycles.py`, `docs/UNSAFE.md` (skeleton) | the slice matches tier 1 on 100 random frames; `.text`/dump identity; every frozen program runs under `bench-cycles.sh` with `PROGDIR` | S-M | orchestrator writes, **R** |
| **P0b** | micro-op additions (5.3): `Cc::{Ult, Uge}`, emulator `SetCi/SetK/JmpR` | `ir.rs`, `emu.rs`, `x86.rs`, `tests/emu_ops_diff.rs`, oracle tests | T1-pattern diff tests; tier-1 dumps identical | S | light |
| **P0x** | experiment X6: the fetch+insns identity for tier 1 and J4 over the corpus and budgets (existing `t15_oracle` feature) | `tests/tr_identity_pre.rs` | identity holds, or each gap is explained and fixed before P3a relies on it | S | orchestrator |
| **P1** | rn-core recording hook (TR23 items 1-5, incl. the hooked-head `trap = 1` and the `run()` choke point), `jit_hooks::RecVTable`, feature `jit_trace`; a test recorder that logs `(proto, pc, depth)` and must equal the oracle sequence **including inlined callees** (`jit_check_e` build); aborts on error/frames/hooks/ABA | `rn-core/src/{vm.rs, vm_unsafe.rs (cfg blocks in cold paths only), jit_hooks.rs, Cargo.toml}`, `rn-jit/tests/tr_hook.rs`, `scripts/jit-text-identity.sh` (normalised fast-copy mode) | JIT-off `.text` identical; normalised fast-copy disassembly identical `jit` vs `jit,jit_trace`; X1 numbers; ABA test (error unwinds, same `CallInfo` reused by a new activation); mutants/hooks: `hooked_head_no_trap` (rn-core cfg forwarded by rn-jit's `tr_mutants`) | S-M | **R** (VM change) |
| **P2** | IR core: builder, constants, fold (TR11/TR12, index normalisation of TR24e), CSE, `tr::interp`, printer, random IR generator, `tr_ir`/`tr_passes` | `src/tr/{ir,fold,interp,print}.rs`, `tests/tr_{ir,passes}.rs` | property tests incl. Miri; mutants/hooks: `fold_nan`, `fold_x_plus_zero` | M | light |
| **P3a** | recorder core: protocol and state machine (4.2), the prototype opcodes of 4.3 **without** Lua calls and intrinsics (root frame only: arithmetic, compares, `FORLOOP`, `JMP`, upvalues, array get/set), lockstep (TR17), count model (TR5.1), root-frame snapshots (TR3, TR3a, TR22b), abort reasons (4.6) incl. `Unstable` re-arm (TR24c), `trap` clearing at Leave (TR23.4), lazy validation (TR23.5), the rolled closing (7.8: `loop_at = None` + writeback list) | `src/tr/{record,snap}.rs`, `tests/tr_record.rs` | golden IR dumps for 12 call-free programs; abort matrix; lockstep on the call-free corpus; mutants/hooks: `snap_drop_last`, `snap_prune_dead`, `refund_off_by_one`, `land_pre_mid_swap`, `rec_no_validate`, `upval_range_skip`, `leave_trap_kept` | M-L | **R** (semantics, TR5, TR15) |
| **P6** | runtime: dispatch jumps, counting stubs and cells in tier-1 lowering (TR20, `lower_ex` hook), `TrProto`, trace cache, `TrId` generations and dependency edges (TR32 API) + flush on `free_code` (TR18), penalties incl. free-peel/`pending`, `rerec` and hot type-guard re-record (TR21, 3.3), charging (3.5), `TraceOpts` parsing, VERBOSE/DUMP, `+tracemove` | `rt.rs`, `lower.rs` (hook), `src/tr/{cache,print}.rs`, `tests/tr_runtime.rs` | `TRACE=off` dump identity; `Black` and `Traced` heads: dispatch path is load + `JmpR` only (dump check); lifetime tests (proto freed with live traces, suspended coroutines, `verify` quarantine, stale `TrId`); memory-limit tests; mutants/hooks: `dep_edge_skip`, `flush_in_handler` | M | **R** (lifetime) |
| **P3b** | recorder calls: inlining of Lua calls (4.4) with frame stack and popped-frame snapshots (TR3), stack/CallInfo conditions (TR6b/c), intrinsics (lib_math::jit_builtins exists, [V] lib_math.rs:336), callee-identity guards and the TR6d snapshot-0 marks, dependency edges at inline time (TR32) | `src/tr/{record,snap}.rs` (after P3a), `tests/tr_record_calls.rs` | golden IR dumps for 8 inlining programs; abort matrix for calls; lockstep on the corpus; mutants/hooks: `popped_frame_drop`, `native_room_skip`, `native_ci_skip`, `callee_guard_skip`, `entry_guard_past_store`, `dep_edge_at_install` | M | **R** (frames, TR6, TR32) |
| **P5a** | register allocation: reverse linear scan, spill slots, weak snapshot refs, PHI parallel moves (slot and non-slot PHIs, TR24e), TR27/TR27b, `+randra`; pure (allocation result + move lists, no micro-ops) | `src/tr/ra.rs`, `tests/tr_ra.rs` | property tests vs `tr::interp` through a reference lowering (Miri); parallel-move property test; mutants/hooks: `phi_skip_move`, `phi_swap`, `spill_no_reload`, `exit_rename_ignored`, `phi_before_guard`, `+randra` | M-L | **R** |
| **P4** | Rust exit handler and restore (5.5, 5.6) on P5a's layout, shared exit routine (micro-ops), `tr_exit` | `src/tr/exit.rs`, `tests/tr_exit.rs` | restore tests with 0-4 frames vs real calls on a twin state, incl. popped-frame slots; Miri; mutants/hooks: `frame_savedpc_caller_off`, `frame_nresults_off` | M | **R** (frames, TR18) |
| **P5b** | assembly: IR -> micro-ops on P5a's allocation, inline/generic stubs, entry checks (TR6) with TR6d placement and the 3.3 entry counters, budget (TR5.2-5.4), polls, **both** loop layouts of 7.8 (rolled and peeled), `+exitstorm` | `src/tr/asm.rs`, `tests/tr_asm.rs` | emulator vs `tr::interp` on random IR in both layouts; native vs emulator; mutants/hooks: `budget_no_check`, `budget_no_store`, `entry_no_extent`, `entry_no_ci_spare`, `entry_no_typecheck`, `poll_skip`, `fcmp_negated`, `tbar_skip`, `rolled_no_writeback`, `+exitstorm` | L | **R** |
| **PI** | first integration and the **pipeline driver** (`tr/mod.rs`: record -> optimise -> RA -> asm -> place -> install, "Finish" of 4.2), `+emurun`/`+emuverify` at install; the recorder, RA, asm, exits and runtime together in the rolled layout on the vertical-slice family and 30 twin programs (9.3 subset: arithmetic, float, arrays, one-level inlining, the two type-stability patterns) under budgets, GC stress 1/2 and stackmove | `src/tr/mod.rs` (driver), `tests/tr_core.rs` (first version) | twins green in `always` and `on`; identity (9.5) on them; VERBOSE shows the 3.3 type-stability behaviour | M | yes |
| **P7** | loop optimisation: peeling with substitution and PHIs incl. non-slot PHIs (TR24, TR24e), invariant hoisting, array/upvalue alias analysis with the v1.3 TR13.3 rule and forwarding (TR13), optional ABC (TR25) | `src/tr/{opt_loop,opt_mem}.rs`, `tests/tr_passes.rs` (extend), `src/tr/mod.rs` (pass list line only) | property tests; X9 ablations; set (b) programs show forwarding/hoisting in their DUMP; mutants/hooks: `alias_may_as_no`, `alias_maytable_as_must`, `alias_upval_by_index`, `fwd_across_store`, `cse_across_store`, `peel_subst_stale`, `nonslot_phi_skip`, `abc_with_store` (if ABC is built) | M-L | **R** (TR13, TR24) |
| **P8a** | oracle and identity: trace oracle (9.4), instruction-count identity and no-overshoot (9.5), the `every_mutant` runner over the frozen registry | `tests/tr_{oracle,identity,mutants}.rs`, `tests/tr_core.rs` (extend), `tests/tr_oracle_progs/` | oracle and identity green on the corpus in every trace mode; every registered mutant caught | M | yes |
| **P8b** | jit-diff and fuzzing: modes of 9.8, `NOTRACES`/`FEWTRACES`, prototype-shaped and general templates, fuzz `trace_ir` + `jit_diff`/`run_source` config bits, CI/local, ASan and Miri scripts | `scripts/{jit-diff.sh,ci-local.sh,asan-jit.sh,miri.sh}`, `tests/jitdiff/gen.lua`, `fuzz/fuzz_targets/{trace_ir,jit_diff}.rs`, `docs/JIT_TESTING.md` | all modes green on seeds 1-2000 with the trace fraction >= 25% in `tr`/`tron`; 60 min fuzz per target clean | M | yes |
| **P9** | gate measurement against J4 per the 1.6 baseline (J5 branch with `TRACE=off`, verified against main@89963c6, min of 9): X1-X10 numbers, the P-gate table, VERBOSE abort/exit/re-record profiles of the informational set and set (d) | `docs/JIT_TRACE.md` (as-built section), `docs/JIT_BENCH.md` | the go/no-go table with numbers, not conclusions; no change to the frozen gate files | S | orchestrator decides |

### 10.2 Full-build (F) tasks (only after GO)

| # | task | size | review |
|---|---|---|---|
| F1 | hash part: `HREFK/HLOAD/HSTORE` for existing keys, `GETFIELD/SETFIELD/GETTABUP/SETTABUP/SELF`, field forwarding (TR13.4), metatable flag guards (TR14), nil reads with `metatable == null` | L | **R** |
| F2a | side traces (7.8, 7.9): exit counters, side recording from snapshots (types from the parent map), `exit_cell`s, entry-0 side traces, `maxside`/tryside policy; hot type-guard exits get side traces instead of the prototype's re-record (3.3). Acceptance adds: entry-0 side traces obey TR6d (twin with a may-alias `USTORE` before a callee load); the set-(d) programs stay within 1.05 x their stable twins | L | **R** |
| F2b | linking: trace-to-trace links, inner/outer nests, linking exits with frame materialisation (heapsort's `sift`), stitching at C calls. Acceptance adds: a link enters the target's entry with the target's TR6d guards re-checked (mutant `entry_guard_past_store` also caught through a link) | M-L | **R** |
| F3 | class-A helpers (TR1): `NEWTABLE`, `CONCAT`/string ops with "step due / may fail -> `H_EXIT`", `NEWREF` (insertion) with TR13.6 kills, `STRTO`, `LEN`, `next`/`ipairs` recorded builtins. Acceptance adds (TR32): a proto-churn test in which a GC step during recording frees an inlined callee whose closure was overwritten (aborts with `DepFreed`, no stale guard), a root freed before its callee (no double flush, stale `TrId` ignored); mutant `dep_edge_at_install` caught by it | L | **R** (GC) |
| F4 | recursion unroll 2, `TAILCALL`, multiple results with explicit `top` snapshots (relaxing TR22b with a top entry) | M | **R** |
| F5 | policy integration with J4 (TR19 trace-first/region fallback), X5, thresholds, `on`-mode heuristics, flush/penalty tuning. Acceptance adds (TR19b, TR20): the stub order `D_H` -> {`C_H`, trace, `W_H`, `pcl[H]`} with one running counter per head; the version-install path rewrites every non-trace cell (test: J4 respecialises a proto with `Warm`, `Black` and `Traced` heads; mutant `cell_stale_version`); retired blocks referenced by trace landings survive until `free_code` (`+verify` assertion); the nest rule of TR19b(a) with mandelbrot, spectral_norm and heapsort no slower than with `TRACE_PREF=region`/the prototype beyond the band (else restrict the rule, section 13); all-`Black` dispatch overhead: the 25 programs with every head forced `Black` within the band of `TRACE=off` | M | **R** |
| F6 | hardening: 60-minute fuzz campaigns per target (5.5 and 5.4), ASan/Miri, official suite variants, review of the orchestrator-merged UNSAFE.md J5 section (embedded addresses TR7, cells TR8, exit area, shared exit routine, flush deferral), `docs/JIT.md` | M | yes |
| F7a | 5.4 traces (TR30) for the prototype feature set: recorder/IR arms for the 5.4 opcode layouts, `ATOUCH`, 5.4 frame fill in the restore, `cfg(rustynail_lua54)` in `tr::{record,ir,asm,exit,interp}`, the 5.4 twin subset. Runs right after GO, before F1. **5.4 tax on every later F task:** each F task implements its 5.4 arm itself (opcode layouts, `ATOUCH` interaction, `gen54`), runs the `tr_*` suite and the official 5.4 suite under the 5.4 cfg as part of its acceptance, and is sized about 15-20% larger for it | M | **R** (`alimit_touch`) |
| F7b | 5.4 verification and measurement matrix (9.12): every `tr_*` test under the 5.4 cfg, official 5.4 with `TRACE=always`, jit-diff 5.4 seeds, oracle, insns identity, 5.4 mutants, `docs/JIT_BENCH.md` "Lua 5.4" trace columns | M | yes |
| S0 | sinking prototype check (6.8): minimal sinking of `TNEW` with constant-key stores, the virtual accounting in the rn-core collectors (5.5 incremental and generational, `gen54`; `jit_trace` only, JIT-off identity, GC_VERIFY clean), drift test, `sink_*` micro-benchmarks; TR3b end-of-iteration writeback and the X11 measurement, with the decision on sunk tables in dead locals at `S_H` (6.8 S0 item 5) recorded in section 13; go/no-go for F8 (if S0 fails, F8 is dropped, D7) | L | **R** (GC accounting) |
| F8 | allocation sinking complete (TR26, TR29): snapshot `SUNK` entries, materialisation with `GCSTP_GC`, virtual debt credit at cycle ends for 5.5 incremental/generational and `gen54`, `-sink` kill switch, mutants, jit-diff `GCOBS` normalisation, official-suite split (9.8), 5.4 arm; writeback per S0's decision; acceptance adds: live in-scope locals exact under `debug.getlocal` after every exit (oracle digest restricted accordingly with sinking on), only dead/out-of-scope slots may differ (TR29.2c), and with `-sink` the full TR3 digest is exact | L | **R** (GC) |
| F9 | ship: flip `RUSTYNAIL_JIT_TRACE` and `RUSTYNAIL_JIT_T15` defaults to `on` for `jit` builds (D3/D5) after the shipping gates of 9.11 pass on 5.5 and 5.4; facade `Builder::jit_trace`; `docs/JIT.md` (off switch, GC-observable policy TR29) | S | orchestrator |
| F10 | final gate measurement (tier-2 targets of plan s1), 5.5 and 5.4 | S | orchestrator |

Order of the full build: F7a -> F1 -> F2a -> F2b -> F3 -> (F4 || F5) -> S0 -> F8 -> (F6 || F7b) -> F9 -> F10. F8 is the last
feature task and depends only on F1/F3; if S0 fails, F8 is skipped and F9/F10 proceed (decision D1 is then recorded
as "in scope, not shipped" in section 13).

**Post-F10 phase (scheduled by the user, 2026-10-31; starts only after the F10 final gate):** four optimiser/recorder
extensions, each with the usual acceptance (5.5 and 5.4, mutants, oracle, jit-diff, official suites, T13 measurement):
P-A `__index` chain method/field lookup (OOP `obj:method()`: guards on the instance metatable and class tables,
HREFK through the chain); P-B library-call intrinsics (`table.insert/remove/unpack`, `string.sub/byte/char`,
`math.*`, `select`, `#`) replacing exits at C calls; P-C number narrowing (float to integer when provably integral);
P-D array bounds-check elimination (TR25, designed but unbuilt). Vectorisation and closure escape analysis are
optional later work, not scheduled.

**Post-F10 addition (user decision, 2026-10-31, after the F10 measurement):** P-E **call and recursion performance**:
cheap Lua-to-Lua call frames in traces (inlined calls without exits for the callee's prologue/epilogue, no per-call
interpreter frame setup where the frame is provably not observed), true inlined recursion (replace the stitch-minimum
32 heuristic by measured unroll/stitch depth), `VARARG` as a trace op (fixed-count and `select('#')`/`...` forwarding),
and `pcall` of a Lua function as a trace construct. Motivation (F10 table vs LuaJIT, ratio to PUC): fib 0.53 vs 0.11,
varargs 0.82 vs 0.41, oop_methods/metamethods (call part) 0.75/0.97 vs 0.07/0.12. Acceptance as the other post-F10
tasks, plus: fib <= 0.30 of PUC (tr/J4 <= 0.60), varargs tr/J4 <= 0.80, no gate program above 1.05 x J4.
**P-E status (built; results and the honest list of what was not reached in docs/JIT_TRACE_REF.md "P-E decisions and results"):**
vararg callees / `VARARG` / `select` and constant-bound callee loops are inlined (varargs program tr/J4 0.62 on 5.5, 0.60 on
5.4); inlined calls were already free in steady state (item 1: nothing to remove); `fib` has no loop, so recursion needs a new
trace kind (function-entry roots), not a heuristic (item 2, not reached); `{...}` / `table.pack` need a table-resize IR op
(not done); `pcall` is a Rust frame and not a trace construct (item 4, reasons in the REF).
Order: P-A -> P-E -> P-B -> (P-C || P-D); P-A and P-E may run in parallel builders (disjoint: recorder lookup vs call
frames), measured per task with the T13 method. Expected effect of P-A, P-E, P-B together: geomean trace/PUC about
0.43 -> 0.33-0.35 (estimate, not a gate).

**Measurement tasks queued behind P-B (user decision, 2026-10-31):** each is a profile-first task; a fix is built only if the
profile names a removable cause, otherwise the finding is recorded. P-F sieve/fannkuch vs J4 (F10: 1.21x / 1.08x): exit,
side-trace and linking profile, target no gate or informational program above 1.05 x J4. P-G short-script regression
(F10: tables_08 1.30 vs tier 1, short-script geomean tr/t1 1.003 vs 0.989 at P9b): attribute the fixed per-state cost of
the default-on trace driver (larger Mem/GcState, init work) and remove it; target geomean <= 0.99, no script above 1.10. **Done (docs/JIT_TRACE_REF.md "P-G decisions and results"): the cause was not a fixed per-state cost (none exists) but two scripts that pay for compile/record work; landing cache, structural shape hash, cheaper dispatch lowering and a time start gate; tables_08 1.32 -> 1.04, strings_04 1.14 -> 1.04 (cycles), the 0.99 geomean is below the machine's noise.**
P-H cost of exact frames and hooks: an experimental, never-shipped build with frame/CallInfo writes, budget updates and
hook checks disabled in traces, to measure what the exactness costs on fib, varargs, oop_methods and the 25 programs; the
result goes to the user as input to a semantics decision, nothing from the experiment is merged except the report.
**Further measurement tasks (orchestrator, 2026-11-01), after P-H, profile-first, no semantic change:** P-I build-level gains
for code that never traces: PGO + LTO + `-C target-cpu` for the interpreter and tier 1 (and BOLT if available), measured on the
25 programs and the short-script suite; P-J inline allocation fast path (bump/free-list allocation of TNEW/SNEW inside traces,
preshaped table constructors); P-K quickened/inline-cached interpreter opcodes (`GETFIELD`/`SELF`/`GETTABUP` caches keyed by
node index, superinstructions) for the untraced 0.8-1.0 band. Each is built only if its profile shows a gain >= 3% geomean
on the programs it targets.
**P-I done (docs/JIT_TRACE_REF.md "P-I results"): PGO adopted as the recommended build (the release default since 2026-11-11) (`PGO_FEATURES=jit scripts/pgo.sh`, PIE): cycles
0.90-0.93 interpreter-only / J4, 0.95 traces on the 25 programs, 0.94-0.97 on the short suite; LTO/cgu already in place, `panic = "abort"` unsafe (unwinding is the
C API error path), `target-cpu=x86-64-v3` and BOLT-on-PGO rejected (no gain); the static dispatch table of `build-fast.sh` costs `oop_override` 13% in trace mode.**
**P-F status (built; attribution table and the honest list in docs/JIT_TRACE_REF.md "P-F decisions and results"):** sieve's loss against J4
was a recorder choice, not trace cost: `FORLOOP` guarded the step as a constant, so the marking loop `for j = i * i, N, i` (step = the outer
index) failed its pre-roll guard on every other run, was flushed `Ineffective` twice and ended in tier 1 (75% of the cycles); a step that is
not a literal of the prototype is now an invariant operand (`FORPREP` in side traces guards its sign only): sieve tr/J4 1.25 -> 0.90-0.94.
fannkuch (1.06-1.08) is the cost of 38 M entry/exit transitions of 1-4 iteration loops; the process default `sidemin=4` (no side trace of fewer
than 4 bytecodes) takes it to 1.04-1.05, the rest is structural (side trace -> loop-body links, path choice by frequency, inlining tiny
inner loops) and left.
**P-M3 / P-M4 status (native self-recursion, `recnat`; details in docs/JIT_TRACE_REF.md "P-M3 decisions and results", "P-M4 decisions and results"):** a loop-free, non-vararg
function that calls itself through an upvalue gets a function-entry head, a continuation head after each such `CALL`, and traces that push the callee's frame
and return into the caller natively (no interpreter or tier-1 round trip per call). Default **on in a process** since P-M4 (`TrConfig::apply_process_limits`,
`RECNAT_PROCESS`; the struct default used by the tests stays 0, a `recnat=` in `RUSTYNAIL_JIT_TRACE_LIMITS` wins). `fib` 0.63 x, `binary_trees` 0.84 x,
`rec_loop` 0.81 x of `recnat=0` cycles. Statistics: `[trace] entered` sums the `entry_ok` counters, which only the *full* entry sequence of a head bumps; native
pushes and returns go through the inner entry (room checks only) and are not counted.
**P-H scope (user decision, 2026-10-31): mirror LuaJIT's accepted relaxations and measure each separately.** The experimental
build gets one switch per relaxation so the gain is an ablation, not a lump: (a) hooks never run in compiled code (a set hook
only switches to the interpreter, as LuaJIT does); (b) no exact instruction counting in traces (no budget updates, count hooks
and instruction limits only checked in the interpreter); (c) `debug.getlocal/getinfo/traceback` inexact inside compiled code
(no frame/CallInfo writes except at exits that need them, dead slots and inlined frames not reconstructed); (d) GC and
finalizer timing relaxed (no virtual-byte accounting for sunk allocations, no drift bound, no count credits); (e) all four
together. Each variant is run on fib, varargs, oop_methods, metamethods, closures, binary_trees, the 25 programs and
bench/jit-trace (T13 method). The report states per relaxation: the gain (tr/J4 and tr/PUC geomean and the biggest movers),
which official-suite files and which of our identity tests/oracles would break, what an opt-in "fast" mode would have to
document, and an estimate of what LuaJIT-style relaxations leave on the table beyond those (no semantics of ordinary
results are touched in any variant).

### 10.3 Risks

| risk | mitigation |
|---|---|
| Gains concentrate in call inlining only (cheaper alternatives exist) | P-gate criteria 2-3 and D4; region leaf-inlining as the costed alternative |
| Sinking: wrong materialisation (layout, barriers, ages) or GC drift beyond TR29 | separate S0 check; materialisation only through the interpreter's own allocation/raw-set/barrier routines; `GCSTP_GC`; drift test; `RUSTYNAIL_GC_VERIFY=1` after every materialisation in tests; sinking mutants; `-sink` kill switch |
| GC-observable diffs hide real bugs in the harness | only lines tagged `GCOBS` are dropped, only in sink modes; every program also runs with `-sink` and must then match exactly |
| 5.4 `alimit_touch` on reads breaks forwarding/CSE | `ATOUCH` modelled as a store (TR30), mutant `atouch_skip`, `array_limit_hint_sequences`-style tests under traces |
| Default-on ships an immature tier | defaults flip only in F9 after the shipping gates (9.11) on both versions; `RUSTYNAIL_JIT_TRACE=off` restores J4 exactly; the 24 h fuzz campaign (user) before a release |
| Register allocation around PHIs and snapshot renames (LuaJIT's historical bug classes, [R] design notes 10.10) | TR27 assertion in every build of `tr_ra`; random RA; property tests vs `tr::interp`; mutants `phi_*`, `exit_rename_ignored`, `nonslot_phi_skip`; Opus reviews of P5a and P5b |
| Exit state wrong for one snapshot kind | one `Snap` source for inline and generic stubs; trace oracle at every exit (9.4); exit storm; `+emuverify` |
| Hidden Lua code or GC on a trace (TR1 violated by a helper) | helper classes are a closed list in `tr::asm`; `+verify` asserts GC debt, `ci`, stack pointer and `nCcalls` unchanged across each helper; GC stress 1/2 twins |
| ABA on embedded proto addresses | TR7 + dependency edges + mutant `dep_edge_skip` with an address-reuse test |
| Flush of a trace with a live activation | TR18 (shared exit routine, deferred flushes); `verify` quarantine; mutant `flush_in_handler` |
| VM hook perturbs the interpreter | TR23: cold-path-only changes, no new `GlobalState`/`JitSlot` fields, normalised disassembly identity of the fast copy, `.text` identity off, X1 |
| Recorder loses frames or follows a stale `CallInfo` | TR23.2 hooked-head `trap = 1`, TR23.5 choke point + lazy validation, P1's oracle-sequence and ABA tests |
| Gate passes on inlining alone | frozen benchmarks (P0), per-program and non-inlining criteria (1.6 2-4; set (b) recomposed in v1.3 so criterion 4 measures forwarding/LICM/peeling, not register loops), inlining-only = no-go for the full build |
| Type-unstable loops thrash or blacklist (spectral_norm's `local a = 0` and late float arrays) | free-peel and hot type-guard re-record rules (3.3, TR24c); gate set (d) and twins (9.3); `rerec` limit |
| Short inner loops / branchy code slower than regions | trace-first only after X5; informational criterion 6; TR19b nest rule; linking/side traces in F2; ownership fallback to regions (TR19) |
| Compile cost visible on short scripts | thresholds by X7; record limits; short-script band in the gate |
| Code size (pre-roll + body + side traces + stubs) | limits 7.10, code cap, X10 |
| Semantic drift between recorder and VM | lockstep TR17 on every recorded instruction; IR interpreter as single semantics for recorder, passes and tests |
| Schedule (trace compilers are the hardest JIT tier) | P-gate before the full build; every F task independently revertible behind `TRACE=off`; the runtime default stays off on `main` until F9 |

### 10.4 Decisions

The former open questions were answered by the user on 2026-10-29; they are recorded as decisions D1-D5 in section 13.
One new open question (N1, address-dependent output under sinking) is listed at the end of section 13; experiment-
dependent choices (X1-X11) are decided by the orchestrator from measurements and logged in section 13.

### 10.5 Effort, honestly relative to J4 [J]

J4 from its design to the end of T4b took about 5 hours of wall time for 7 builder tasks and 3 Opus reviews (git log
2026-10-29 11:18-16:14); the unit is J4 complete (T1-T8, T12, T13; T9/T10 skipped). The plan's week figures are
human-scale estimates and do not match this cadence, so the estimate is in J4 units (judgment, not derived):
- **Prototype**: 16 tasks (P0, P0b, P0x, P1, P2, P3a, P3b, P4, P5a, P5b, P6, PI, P7, P8a, P8b, P9; v1.2 said 12 but
  listed 14), 5 of them L or M-L with novel risk (P3a recorder semantics, P5a reverse RA with PHIs and exit
  consistency, P5b with two loop layouts, P7, and P3b as M with frame risk): **1.3-1.8x J4** (the splits add hand-offs,
  not work; the v1.3 additions, rolled layout and type-stability policy, add about 0.1x), more if P5a needs a second
  review round (likely).
- **Full build**: 14 tasks (F1, F2a, F2b, F3-F6, F7a, F7b, S0, F8, F9, F10): F1-F6 about 1.6-2x J4 including the 5.4
  tax; 5.4 base and matrix (F7a/F7b) about 0.3-0.4x J4; sinking (S0 + F8, with GC accounting changes in two collectors
  under review) about 0.5-0.6x J4.
- **Total J5: about 3.7-4.8x J4.** A no-go at the P-gate is a realistic outcome (then only region leaf inlining is
  built), and so is S0 dropping F8; no probabilities are claimed.

## 11. Reuse vs rewrite

| component | J5 use |
|---|---|
| `ir.rs` micro-op IR | **reused unchanged**, plus `Cc::{Ult, Uge}` (P0b) |
| `x86.rs`, `arch/x86_64` encoder | **unchanged** (two condition codes) |
| `emu.rs` emulator, `Emu`/`Helpers` model (T12) | **reused**; adds `SetCi/SetK/JmpR` (P0b) |
| `mem.rs` `CodeArena`, W^X, quarantine, `place_code`, charging | **unchanged** |
| thunk / `rt::enter` / `JitData` / `Stats` / knobs | extended (trace stats, exit area, recorder state behind `JitSlot.data`) |
| tier-1 templates (`templates_*.rs`, `calls.rs`) | **unchanged**: tier 1 is the deopt target and runs everything off-trace |
| `lower.rs` | dispatch-cell hook in `lower_ex` (like J4's `RegionHook`), count-event API; tier-1 bytes unchanged with `TRACE=off` |
| `BlockSig`, `pcl`/`pcl_post`, `pc_labels`/`pcmap` (J4) | **reused** as the landing-label and C1 model (TR4, TR5) |
| T15 runtime: versions, retired blocks, J4-DATA boxes, continuation cell, `free_code` path | **reused patterns**; `TrProto` follows `T15Data`'s address-stability rules |
| `t15::effects` def/use tables | **reused** by the recorder (written slots, lockstep) |
| `t15::analysis`, `t15::alloc/codegen/stubs` | not reused (region-specific); their review checklists are the template for P5a's/P5b's |
| `rn_core::jit_helpers` | not called from traces (they may run Lua or GC); new N/S helpers; F3's class-A helpers wrap the same `Table`/string routines |
| `jit_hooks` VTable, builtin registry | extended (`RecVTable`; `lib_math::jit_builtins` already exists, [V] lib_math.rs:336) |
| J4 VM-truth oracle (`oracle.rs`, `t15_oracle.rs` harness) | **reused** as the clock-synchronised trace oracle |
| `+verify` tier-1 type assertions at exit-target labels (T12) | **reused** for trace landings |
| mutation framework (`t15_mutants`, `every_mutant`) | **reused pattern** (`tr_mutants`) |
| jit-diff, `JITDIFF_ENTERS`, fuzz targets, ASan/Miri scripts | extended (modes, templates, `trace_ir`) |

## 12. Rule index (review checklist)

| rule | statement (short) | section |
|---|---|---|
| TR1 | no Lua code, GC step, finalizer, emergency GC, stack move or hook on a trace; helpers N/S/A only | 6.9 |
| TR2 | one trace activation per state; per-state exit/spill area | 5.5 |
| TR3 | exact VM state at every exit, all written slots in `[root base, +extent)` restored (popped callees too), no dead-slot pruning (sinking relaxation: TR29.2c) | 5.4 |
| TR3a | guards belong to an instruction, snapshot = state before it, exit re-executes it; only 5.4 `ATOUCH` may precede a guard of its instruction | 5.4 |
| TR3b | end-of-iteration writeback of slots the next iteration does not read; sparse snapshots omit every slot the stack holds | 5.4 |
| TR4 | exits land in tier-1 code of the innermost frame's proto, or in another trace through a cell | 3.4 |
| TR4b | every inlined callee is tier-1 compiled before inlining | 3.4 |
| TR5 | C1: segment charge, exit budget `= after_charge + n - fetches(P) - land(P)`, no overshoot; only fetches+insns is mode-invariant | 3.7 |
| TR6 | entry checks (stack room TR6c and CallInfo spares TR6b, both exact incl. intrinsic native frames; budget; typechecks); failure to `pcl[H]` without effect; TR6d: a guard uses snapshot 0 only if its operands are entry values with no may-alias store before it | 4.4, 7.7 |
| TR7 | embedded addresses: deps protos, their constants and tier-1 code, light C functions, JIT data only | 3.4 |
| TR8 | no machine-code patching; data cells | 3.4 |
| TR9 | G1 vacuous on traces; restore writes slots before anything allocates | 6.9 |
| TR10 | barrier condition inline + Rust slow path; never hoisted | 6.9 |
| TR11 | float semantics: no negated cc, no NaN/-0 folds, no FMA/reassociation, min/max order | 6.1 |
| TR12 | integer wrap, no overflow guards, div/mod guards | 6.1 |
| TR13 | alias classes and rules (array must = must-alias tables and indices; provably different indices = no; upvalues may alias except same `UREF`) | 6.6 |
| TR14 | metatable guards: null or flag bits only | 6.3 |
| TR15 | upvalues into the root frame are register accesses (N1; before: abort) under an identity guard, into callee frames abort; range guard | 8 |
| TR16 | recording isolation (one thread, no hooks, own frames) | 4.2 |
| TR17 | lockstep check of every recorded instruction | 4.5 |
| TR18 | flush only without activation; shared exit routine; deferred flushes | 3.4 |
| TR19 | one owner per loop head; region eligibility (cold/`Black` heads) | 3.2 |
| TR19b | full build: nest rule, stub composition with J4's warm-up stub, cell rewrite at version install | 3.2 |
| TR20 | dispatch jump before the head's count; cell = counting stub (Warm), trace, or `pcl[H]` (Black); `pcl[H]` downstream; `TRACE=off` identity | 3.1 |
| TR20b | growth limits and flush-all | 7.10 |
| TR21 | penalties and blacklisting per head; free peels not counted; hot type-guard exits and `Unstable` re-record (max 3) | 3.3 |
| TR22 | polls at every back-edge and link | 3.6 |
| TR22b | no snapshot at top-reading instructions; `top = ci.top` | 5.4 |
| TR23 | VM hook under `jit_trace`, cold paths only, hooked-head `trap = 1`, `trap` cleared on all recorded frames at Leave, `run()` error choke point (intrinsics cannot raise), lazy validation, state behind `JitSlot.data`, normalised identity gates | 4.1 |
| TR24 | peeling soundness; type instability re-records (no `CONV` repair); non-slot PHIs (TR24e) | 6.4 |
| TR25 | ABC only without possible resize | 6.5 |
| TR26 | sinking soundness, snapshots, materialisation, kill switch | 6.8 |
| TR27 | exit consistency of register maps per snapshot | 7.2 |
| TR27b | loop layout: no guard after the PHI moves; `S_H` after `LOOP`; rolled layout for `-loop` | 7.8 |
| TR28 | IR interpreter is the single reference semantics; Miri coverage | 5.7 |
| TR29 | GC-observable policy with sinking: what stays identical, what may differ (GC values, finalizer/weak timing, address-dependent output, `debug.getlocal` of dead/out-of-scope slots), limit guard, virtual-debt target | 6.8 |
| TR30 | Lua 5.4 traces: layouts, `ATOUCH`, `gen54`, test matrix | 8, 9.12 |
| TR31 | defaults: `TRACE` and `T15` on in `jit` builds after F9, `RUSTYNAIL_JIT_TRACE=off` reproduces J4 exactly, `-sink` restores exact GC observables | 13 |
| TR32 | dependency edges registered at inline time; `free_code` aborts the active recording; `TrId` generations | 3.4 |

## 13. Decisions (user, 2026-10-29; binding)

- **D1 Allocation sinking is in scope** (former Q1). The user accepts that GC-observable behaviour
  (`collectgarbage("count")`, finalizer timing and order of interleaving with output, weak-table clearing timing) may
  differ from a JIT-off run. Policy: TR29 (what stays identical, what may differ, the limit guard and the virtual-debt
  bound), soundness TR26, kill switch `RUSTYNAIL_JIT_TRACE_OPT=-sink` (with it, GC observables are exactly JIT-off).
  Sinking is a separate late task (S0 check, then F8) so the core is never blocked by it; the official `gc.lua` and
  `gengc.lua` run with `-sink`, everything else with sinking on (9.8).
- **D2 Lua 5.4 traces are in scope** (former Q4): same crate under `cfg(rustynail_lua54)` (TR30), tasks F7a/F7b, matrix
  9.12, shipping gates 9.11 on both versions. The prototype and its gate stay 5.5-only.
- **D3 Runtime default on** (former Q5): `RUSTYNAIL_JIT_TRACE` defaults to `on` whenever the `jit` cargo feature is
  compiled in; the cargo feature stays off by default and the JIT-off build stays byte-identical (F1). The off switch is
  `RUSTYNAIL_JIT_TRACE=off` (and `Builder::jit_trace(false)` / `rn_jit::set_trace`), documented in `docs/JIT.md`; it
  restores J4 behaviour exactly (TR31). The defaults flip in task F9 once the shipping gates of 9.11 pass; until then
  `main` keeps `off` so intermediate states never ship traces by default. Builders run the usual gates (jit-diff,
  official, ASan, Miri, 60-minute fuzz); **the 24-hour fuzz campaign is run by the user after J5 is complete.**
- **D4 Go/no-go thresholds** (former Q2): the criteria of 1.6 as revised in v1.2 (spectral_norm <= 0.70x J4, matmul
  <= 0.95x, non-inlining micro geomean <= 0.90x, gate set <= 0.85x, hard bounds on the informational set); a pass that
  comes only from inlining is a no-go for the full trace build and region leaf inlining is built instead. v1.3:
  thresholds unchanged; the T9(a) comparison clause is gone (T9 skipped); set (b) recomposed (D6); set (d) added.
- **D5 Ownership** (former Q3): trace first, J4 region after blacklisting (TR19). Consequently J4 regions are **on by
  default** in `jit` builds together with traces, as the fallback owner of nests the tracer blacklisted or never
  traced; with `RUSTYNAIL_JIT_TRACE=off` regions own every hot nest as in J4. The final order is decided by experiment
  X5 under the rule in 3.2 and logged here. **This supersedes the clause of `docs/JIT_TIER15.md` section 12 ("T15
  stays opt-in (off) until a fuzz campaign is clean")**: by the user's decision the long fuzz campaign runs once at the
  very end, after J5; the coordinator amends JIT_TIER15 section 12 to point here.
- **D6 Gate set (b) recomposed** (user, 2026-10-30, review decision D-a): set (b) consists of loops where forwarding,
  invariant hoisting and peeling apply (`licm_expr`, `array_rec`, `upval_acc`, `int_rec2`, 1.6); the pure register
  loops become informational; the thresholds stay (criterion 2: 30% gain on spectral_norm; criterion 5: 15% on the
  gate set; criterion 4: 10% on set (b)).
- **D7 Sinking stays behind S0** (user, 2026-10-30, review decision D-b): if S0 fails, F8 is dropped with no fallback
  design. The user accepts that sinking (and the writeback S0 chooses) may change what `debug.getlocal` shows for dead or
  out-of-scope slots after an exit (TR29.2c); live in-scope locals stay exact.

**TR31 (defaults and off switches).** Until task F9, `RUSTYNAIL_JIT_TRACE` and `RUSTYNAIL_JIT_T15` default to `off` on
`main`. F9 flips both to `on` for builds with the `jit` cargo feature, after the shipping gates of 9.11 pass on 5.5 and
5.4. In every build: `RUSTYNAIL_JIT_TRACE=off` reproduces J4 behaviour exactly (dump identity), `RUSTYNAIL_JIT_T15=off`
removes regions, `RUSTYNAIL_JIT_TRACE_OPT=-sink` makes GC observables exactly JIT-off, `RUSTYNAIL_JIT=off` removes the
JIT; the facade exposes the same switches (`Builder::jit_trace`, `rn_jit::set_trace`); the `sandbox()` preset keeps
the JIT off; the cargo feature stays off by default and the JIT-off build is byte-identical (F1).

**F5 restriction of the nest rule (TR19b(a); measured, see docs/JIT_TRACE_REF.md "F5: measurements").** The rule is
applied only when no inner trace of the nest inlines calls (`Trace::deps` empty). Unrestricted, `closures` (the inner loop
calls a closure; J4 cannot generate a region for the nest) ran 1.41 x slower than with the rule off, because the
inner trace was flushed and the loop went to tier 1. With the restriction every one of the 25 programs is within the band
of the prototype (`new/base` geomean 0.999). `RUSTYNAIL_JIT_TRACE_OPT=-nestrule` switches the rule off, and
`RUSTYNAIL_JIT_TRACE_PREF=region` gives the experiment X5 (region first). The default of D5 (trace first) stands:
X5 shows region-first is no better on the 25 programs (all-25 geomean 1.157 of the prototype, with heapsort, loops_arith, nbody and
matmul clearly slower and spectral_norm 10 x slower) and not faster than traces on any program.

- **D8 S0 result and the writeback decision** (S0, 2026-10-31; measurements in `docs/JIT_TRACE_REF.md` "S0"). The sinking
  prototype passes every S0 check that was run, F8 is a GO (details and what was not run in JIT_TRACE_REF.md). Decisions:
  (a) *A sunk table still held at the loop end by a dead slot* is **dropped from the writeback, with no PHI and no store**
  (a slot is dead when no instruction of the iteration reads it from the stack and it is a slot of an inlined callee or
  lies above the locals active at the head, `Proto::locvars`; decision D7 covers what `debug.getlocal` shows there); a
  table held by a live local, or in a prototype without debug information, is **not sunk** (it stays a class-A `TNEW`).
  Materialising at the writeback would allocate once per iteration (no gain), sunk PHIs were not needed by any measured
  program and stay a possible F8 extension. (b) *General TR3b writeback* (`+wb`: store the slots the next iteration does not
  read, no PHI): built and measured (PHIs of the 25 programs 131 -> 80, `Compile(phi)` aborts 0 -> 0), but **slower**
  (all-25 geomean 1.023, `spectral_norm` 1.49, `matmul` 1.08) because the stores execute every iteration while the PHIs
  of those loops cost registers only; it is **off by default** and not needed by sinking. (c) *Virtual accounting* is a
  charge at the allocation site, inline (two `add [mem], imm` on the collector's byte counters), undone by
  `uncharge_virtual` at the exit that materialises the table and at the end of the next incremental cycle / at every
  minor, major or explicit collection; virtual bytes count for the collector but **not for the memory limit** (no false
  `not enough memory`), so TR29.3's in-trace limit guard is not needed and not built (a host allocation failure while
  materialising is fatal in the prototype; F8 must make it a Lua error at `P`).

**Open question for the user (new in v1.2).**
- **N1 Address-dependent output under sinking.** Sinking changes which addresses later objects get, so `tostring` of
  tables/functions, `%p` and `pairs` order over tables keyed by such objects can differ from a JIT-off run (TR29.2b).
  These already vary between JIT-off runs under ASLR and Lua does not specify them. Does the accepted GC-observable
  divergence (D1) cover this? *Recommended default: yes, documented in `docs/JIT.md`; `-sink` is the switch for users
  who need reproducible address-dependent output within one process layout.*

## 14. Review log

**v1.1 independent review (read-only, at `9c9814e`), dispositions in v1.2.** All findings were checked against the
code; none was rejected outright.
1. Callee frames escape the recorder: **confirmed** (vm_unsafe.rs:1666-1669 sets `trap` only with hooks; call.rs:457
   never writes it; the hooked head clears `TRAP_JIT`). Fixed: TR23.2, 4.2.
2. `Stats::insns` identity false: **confirmed**. Fixed: TR5 restated (fetch+insns identity, no overshoot), 9.3, 9.5.
3. Exit state: (a) popped frames: **confirmed**, TR3; (b) native-call stack/CallInfo needs: **confirmed**
   (call.rs:344-345), TR6b/TR6c; (c) exact strict condition on `top`: **confirmed** (call.rs:457-461), TR6c.
4. TR29 overclaims: **confirmed**. Fixed: TR29 restated (finalizer side effects, address-dependent output), jit-diff
   prefixes every GC-state line; new question N1.
5. Loop-end exits and the `cnt_at` double count: **confirmed**. Fixed: TR27b/7.8 layout, TR5.4 formula.
6. TR23 gates and `JitSlot` fields: **confirmed**; recorder state behind `JitSlot.data`, normalised disassembly.
   The `Continue -> FrameChanged` conversion is indeed unnecessary (vm_unsafe.rs:551 reaches `fast_trap`): removed.
7. Catch-site enumeration fragile: **accepted** with a different fix than proposed: the single choke point is `run()`
   (every error of a recorded frame leaves `run_mode::<true>`, because recorded frames never contain a `pcall`), plus
   the lazy validation the reviewer proposed (TR23.5).
8. Micro-op inexactness: **confirmed** (no memory add, `CallH` u32 immediates only, `Jmp` within one `Program`, `V7`
   used as scratch, no `lib_math::jit_builtins` yet). Fixed in 5.3, 5.6, 7.1, 7.6, P3.
9. D5 vs JIT_TIER15 section 12: **accepted**; D5 states that it supersedes that clause.
10. Gate provable by inlining alone: **accepted**: frozen files, individual and non-inlining criteria, inlining-only =
    no-go for the full build, comparison with T9(a)/leaf inlining.
11. Soft baseline: **accepted**: baseline = J4 gate build, same method; criteria 6 and 7 rewritten (hard bounds; tier-1
    code bytes as reference).
12. Task cut: **accepted**: P0 reduced, P0b/P0x/PI added, P5 split, P4 after P5a, UNSAFE.md per task, F2 split, 5.4 tax
    stated, S0 sized L.
13. Numbers: **accepted**: numeric-9 recomputed (fib removed: 0.21-0.26), labels per cell, probabilities dropped.
    Minor items (plan D6/D7 references, "TR1's spike", 3.3 counters, goal numbering, missing TR2/TR4/TR31 texts): fixed.

**v1.2 review (independent Opus review, 2026-10-30) and user decisions D6/D7, dispositions in v1.3.** Numbers and code
facts were re-checked at `89963c6`.
1. Type instability (spectral_norm): **confirmed** against `bench/programs/spectral_norm.lua`: both inner loops start
   `local a = 0` (integer) and `A` returns a float, so `a` is a float from the second iteration on; `u[i] = 1` makes the
   first `Av` call see integer elements, and N = 1000 exceeds the 256 threshold inside that call. In the prototype the
   dispatch at every back-edge would even make pattern (b) enter and exit once per iteration. Fixed: 3.3 (free peel not
   counted, hot type-guard exits and `Unstable` re-record, shared `rerec` limit 3), TR24c (re-record only, no `CONV`
   repair), 4.6, gate set (d), twins 9.3.
2. TR13.3 unsound on may-aliasing tables: **confirmed**. Fixed: must = tables and indices must-alias; provably
   different indices = no whatever the tables; mutant `alias_maytable_as_must`.
3. Stale J4 baseline: **confirmed**; quoted numbers verified (spectral_norm 0.606, heapsort 0.445, queens 0.602 vs tier 1
   0.575, nbody 0.266). Additionally found: v1.2's "numeric-9" J4 0.285 and LuaJIT 0.14 were a projection and the
   numeric-7 LuaJIT value; measured numeric-9 is 0.316 (J4) and 0.177 (LuaJIT); the plan's targets use the numeric-7 set
   of `bench_cycles.py` (J4 0.275). Fixed: 0, 1.3, 1.5 (all rows re-derived; spectral_norm and queens based on tier 1),
   1.6 (baseline = J5 branch with `TRACE=off` verified against main@89963c6; T13 method, min of 9; T9 clause removed),
   TR12 (T10c), P3b (`lib_math::jit_builtins` exists).
4. Gate criterion 4 (D6): **accepted**. Set (b) = `licm_expr`, `array_rec`, `upval_acc`, `int_rec2` with per-program
   mechanisms and projections (geomean 0.47-0.71); pure register loops, `dot` and `upval_inv` informational (b');
   frozen-file list in P0 updated. Needed for it and added: TR24e (non-slot PHIs, index normalisation in fold).
5. `-loop` layout undefined: **confirmed**. Fixed: rolled layout in 7.8 (writeback instead of PHIs, `TOP` with
   snapshot 0 = `S_H`), owners P3a/P5b, used by PI, `trnoopt`, X9; mutant `rolled_no_writeback`.
6. Task breakdown: **accepted**. P3 -> P3a (core) / P3b (calls); P8 -> P8a (oracle/identity/mutant runner) / P8b
   (jit-diff/fuzz); mutants and stress hooks owned by the task owning the code (10.1); driver in `tr/mod.rs` owned by PI;
   UNSAFE.md paragraphs merged by the orchestrator; `FEWTRACES` fraction check and prototype-shaped templates (9.8);
   section 11 now agrees with 10.1 (`Ult`, `SetCi` in P0b). Task count corrected (v1.2 said 12, listed 14; now 16).
7. Minor: **accepted**: TR3a 5.4 `ATOUCH` exception; TR23.4 clears `trap` on all recorded frames at Leave (mutant
   `leave_trap_kept`); TR23.5 restated for intrinsics.
8. Cost of no dead-slot pruning: **accepted** as a full-build requirement: TR3b writeback, generalised sparse rule,
   decided in S0 (X11, item 5) with the treatment of sunk tables in dead locals; sinking stays behind S0 (D7), and the
   user accepts the `debug.getlocal` difference for dead/out-of-scope slots (TR29.2c).
9. Snapshot-0 guards: **accepted**: TR6d (7.7), applying to the prototype as well (a may-alias `USTORE` before a callee
   load is possible there); F2a/F2b acceptance; mutant `entry_guard_past_store`.
10. J4 interaction: **accepted**: TR19 cold/eligibility definition, TR19b nest rule (with a measured escape hatch for
    inlining-heavy nests: the reviewer's rule can hand a spectral_norm-like nest to regions, which are slower there),
    stub composition and cell rewrite at version install; F5 acceptance.
11. Dispatch cost: **accepted** for the prototype already: TR20 counting stubs, `Black`/`Traced` heads pay load +
    indirect jump, `pcl[H]` downstream of `D_H`; P6 and F5 acceptance.
12. Dependency hole during recording: **accepted**: TR32 (edges at inline time, `DepFreed` abort, `TrId` generations);
    unreachable in the prototype (no recorded allocation), built in P3b/P6, tested in F3.
