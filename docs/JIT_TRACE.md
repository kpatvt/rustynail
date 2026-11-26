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
- **TR3 relaxation F4 (`snapprune`, user-approved relaxation of TR3 / D7 for dead numeric temporaries).** A snapshot taken
  before the instruction at `pc` of the **root frame** (no inlined frames, no open `top`, a trace without `RETF` segments)
  leaves out an entry when (1) its slot is a *temporary*: at or above the active locals of `pc - 1 ..= pc + 1` per
  `Proto::locvars` (a prototype without local-variable debug information, or with `TBC`, is never pruned), and not a
  register an inner closure captures; (2) the slot is *dead* at `pc`: no path of the bytecode CFG reads it before writing it
  (`live.rs`; every open range, `CALL`/`RETURN`/`SETLIST B=0`, generic-`for`, `CLOSE`, `VARARGPREP` and unknown opcode counts
  as reading every register from `A` up, definitions count only when unconditional); (3) the recorded value is a number
  (`Int`, `Num`) or `nil` / a boolean. Collectables (strings, tables, closures, userdata, threads), function slots and sunk
  tables are never pruned, and neither is any local that is active around `pc`. The exit restore leaves the slot as it is
  (stale, never read before it is rewritten, by the liveness proof). The loop optimiser prunes the entries it adds for earlier
  iterations by the same rule, so a loop PHI that only fed such entries disappears (`opt_loop::Em::new_snap`). Observable:
  `debug.getlocal` of a dead numeric *temporary* inside a hook that fires right after an exit may show a stale value
  (PUC does not specify the contents of temporaries; the locals of the function stay exact). The collector marks only below the
  running top and clears above it, so a stale slot does not change object lifetimes. Tier 1 writes only the payload of a
  register whose tag its static types already know, so the typed mid-block landing labels still verify the tags of **every**
  known register (`lower.rs`, P-X), dead ones included: an exit whose pruned register holds another tag than tier 1 assumes
  leaves to the interpreter at that pc (correct, slower); the filter "check live registers only" was tried and is unsound
  (a payload-only store over a stale string tag corrupted the stack, `n5_str_variants`). `snapprune=0|1` (default 1)
  (`RUSTYNAIL_JIT_TRACE_LIMITS`); `snapprune=2` is a measurement-only experiment that also prunes dead *locals* (not
  conforming: `debug.getlocal` of a dead local shows a stale value). Mutants `prune_live_temp`, `prune_local`,
  `prune_collectable`, `prune_loop_carried`, `prune_after_call`; the old `snap_prune_dead` prunes everything unconditionally.
- **F5 (`hstoreguard`): the old-value check of `HSTORE`.** A store into an existing short-string or integer hash node whose
  recorded value is not nil used to exit when the node's value is nil at run time (an insertion or a `__newindex` for the
  interpreter). With `hstoreguard=1` (default) the store is unconditional and the recorder emits the TR14 `__newindex` guard of
  the table instead (`FLOAD metatable == 0`, or `metatable != 0` and the flag-cache bit says no `__newindex`): a node that
  `HREFK` found (live key) takes a plain raw store whatever its old value when no `__newindex` can run, exactly like the R2
  nil store. The metatable/flags `FLOAD`s follow the header-fact rules (CSE'd and hoisted over header-safe instructions, dropped
  at calls, `SETMT`, insertions, allocation), so a table is guarded once per stretch. Keys that may be metamethod names
  (`__*`: the store of a nil slot invalidates the flag cache) and tables whose metatable may have `__newindex` keep the old
  check, which the loop optimiser still drops when the nearest access of the node is a typed non-nil load or a non-nil store.
  Mutants `hstore_nomt_check_skipped`, `hstore_mt_stale`, `hstore_known_nonnil_wrong`.
- **G1 (`guarddiet`): the guard diet of the transitions of a chain.** The types a root's entry guards proved are inherited
  through the whole chain root -> side -> side -> link, per slot, while no trace of the chain writes the slot (`SSTORE`;
  nothing else writes the stack between an entry and an exit but the stubs and the writeback): the exit snapshots' `ents`
  of a side trace now include the entry tags of its own start state (`exit::entry_ents`), so a child attached later knows
  the slots its parent never touched, and the writeback of a side trace skips the tag stores of those slots (`wb_ents_ex`).
  A link to the guard-free second entry also skips the room checks (TR6a / TR6b) when the chain's room is at least the
  target's: the side trace's room is `max(parent's, own)` (it either needs no more than the parent promised or checks its own,
  `AsmEnv::room_have`), the target's `(extent, depth)` is part of the interned signature the link compares, and the link
  jumps to `HeadData::e2n`, the second entry past its room checks (the frame, its base and its spare `CallInfo` list are the
  chain's: the same room argument as the rolled loop's back-edge, which does not re-check either). Interpreter and plain
  dispatch entries keep every check. `guarddiet=0|1` (default 1). Mutants `entry2_transitive_stale`, `entry_inherit_wrong_slot`,
  `room_skip_bigger_frame`.
- **H1 (`hkaddr`): the `HREFK` address folded into its users.** An `HREFK` whose users are all `HLOAD` / `HSTORE` addresses (and whose node
  operand is a register) is *fused* like an `AREF` (`ra::RaOpts::hkaddr`): it keeps its key / shape guards at its own position, defines no
  register and emits no `lea`; its users read `[node + idx * 24 + field]` (`Cx::hk_base`), and the node operand stays live up to them
  (`operand_refs` hands the users `[node, key, value]`). Any other user (snapshot, PHI, store value, loop end) unfuses it. nbody: 46 static
  `lea` in its traces -> 7, -5.3 % instructions. `hkaddr=0|1` (default 1). Mutant `hrefk_fold_wrong_disp`.
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
| `RUSTYNAIL_JIT_VERBOSE` | `1`: summary (recorded, aborted by reason, entered, exits by trace/snapshot, flushes); `2`: one line per event (`[trace] 3 start proto:line pc`, `[trace] --- abort ... reason`, `[trace] 3 loop`, `[trace] 5 side of 3/7 -> link 3`); `>=1` also prints the per-head table (`[heads]`: recordings, aborts by reason and site, refusals, flushes, blacklist time; H8, docs/STUDY_COVERAGE.md) | |
| `RUSTYNAIL_JIT_SAMPLE` | `on` (1000 Hz) or `<hz>`; needs `VERBOSE>=1` | H8: in-process pc sampler (`ITIMER_PROF`), classified per head / tier-1 prototype at close (`[heads] samples ...`); docs/STUDY_COVERAGE.md |
| `RUSTYNAIL_JIT_PERFMAP` | `1`: tier-1 blocks `lua_proto@<ptr>`; `2`: also traces as `TRACE-<n>` and `lua_proto:<line>@<ptr>` | perf map `/tmp/perf-<pid>.map` |
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

## 15. Shape guards (S4, `shapeguard`)

**What.** A hash part with at least 8 nodes has a 64-bit *layout id* slot in front of its node vector (rn-core `table.rs`: `[id @ node-16][lastfree @ node-8][nodes]`).
`0` = not known; otherwise the id names the interned *descriptor* of the vector: `lsizenode` and, for every node whose key is not nil, `(node index, key tag, key bits)`
(`TableRef::layout_descriptor`). Descriptors are content: equal layouts of different tables get the same id (the five `bodies[i]` of nbody share one), and an id never
names an object. With `shapeguard=1` (`RUSTYNAIL_JIT_TRACE_LIMITS`, `shapeguard=0|1`, default 1) the recorder emits, per table access, after the
`lsizenode` guard and `FLOAD node`, one guard `HSHAPE node, #id` (`cmp qword [node-16], id; jne cold`, op bits = `lsizenode`) with the id of the table's layout at record
time, and marks the `HREFK`s of that node vector `HREFK_COVERED`: they assemble without key-tag and key compares (the `HLOAD` after them checks its own value tag).
Per `HREFK` that is about 2 instructions fewer (movabs + cmp + the fused 16-bit tag compare become one byte compare), per table visit about 2 more (the guard); nbody
-397 instructions per step (-16 %).

**Policy (what is carried).** A root offers `e3` only if at least `cache::CARRY_MIN_SLOTS` (4) of its leading `SLOAD`s are carried: fewer cost more in the per-exit
`stale` test than a transition saves (`mandelbrot` +0.6 %, `richards` +0.8 % with one or two). Only values the trace keeps live anyway are carried (used inside the loop
body or at/behind its last guard, `carry_dense`): a dense entry extends the live range of its `SLOAD` to every guard, and a carried value that was otherwise dead early
cost spills in every iteration (`fannkuch`, `queens` +1 to 3 %). A side trace gets a carry entry only if the family root has a third entry
(`cache::carry_slots`: else there is nothing to carry back into). Exit stubs of chain sites carry nothing. Measured design history (insns, programs/nbody 1M, fannkuch,
queens): dense stores on every root exit -> +4 %/+4 %; a three-instruction carry-cell test on every inline exit -> +2 %/+2 %; the cold dispatch and the flag -> neutral.

**Soundness argument.**

1. *The slot is cleared at every change of which key lives in which node* (rn-core `clear_layout_id`: `insertkey` free-main-position and collide-move paths, GC
   `clearkey` of dead keys including weak-table clears, a rehash allocates a fresh vector born with 0, the trace's inline `NEWREF`); `tests_shape.rs` / `tr_shape.rs` and
   the mutants `shape_*` pin each clear. So `slot == id` means the vector has had the descriptor's layout since the id was stored, and `HSHAPE` passing means the same
   facts the covered `HREFK` compares (tag and bits of the key at node `i`).
2. *Which tables the guard may stand in for.* `HSHAPE` is a pure guard of its operands like `HREFK` (CSE'd with an identical one; hoisted into the pre-roll when
   its node-vector reference is loop invariant). Two `HSHAPE` / `HREFK` that share a node-vector *reference* are separated by no instruction that is not header safe
   (`opt_mem::header_safe`): a node-vector `FLOAD` is a header fact that is dropped at `NEWREF` / `TNEW` / `SNEW` / `CONCAT` / `TSET` / `TLEN` / unsafe `LIB`
   (every operation that can insert a key into a table or run a collector step that clears dead keys), and a reload gets a new reference that needs its own `HSHAPE`.
   The header-safe ops never insert a key and never run a collector step (`HDRSUB` strings allocate without a step; `TVIRT` only counts bytes; `HSTORE` writes the
   value of an existing key; a nil store leaves the key in its node). Detectors: `tr_record::s4_covered_hrefks_are_dominated_by_their_shape_guard` (every covered
   `HREFK` has an `HSHAPE` on the same node-vector reference, also after an insertion and after an allocation) and the mutants `shape_guard_skipped`,
   `shape_stale_across_rehash`, `shape_header_fact_kept_over_alloc`.
3. **Key constants and address reuse (the ABA question).** The descriptor contains key *addresses*. They are never dereferenced and the argument does not need them to
   stay unique: the id is a function of the bits only. A table whose slot holds `id` has, in node `i`, a key whose tag and bits are the descriptor's (point 1). If the
   string that had lived at that address was freed and another string allocated there, a table that holds the new string at that node has the same descriptor, the same
   id, and the guard asserts exactly what it asserts for the old one: "node `i` holds a key with these bits". That is the same statement the `HREFK` compare it replaces
   makes, and it is used exactly as before: the key constant of the trace is either a proto constant (pinned by the proto: a trace is flushed before its prototype is
   freed, TR32) or a string seen at record time that an `EQREF` guard compares with the register (a live string at that address), and a node that references the
   address holds a live string (`clearkey` runs, and clears the slot, before a key string can be freed). Both sides of every comparison are live objects, so an
   equal address is the same object. A stale descriptor therefore cannot make a trace accept a different key: the worst case is a table that legitimately has the
   same bits. The interning map is never flushed and ids are never reused (monotonic from 1, below `2^31`: the guard compares against a sign-extended 32-bit
   immediate; at `2^31 - 1` or 4096 distinct descriptors no new id is made, `layout_id` answers 0 and the recorder emits no shape guard). Mutant `shape_id_reuse`.
4. *A table whose slot is 0 at the guard* (its id was never computed: a table built after the trace was recorded, or one whose layout changed): the cold path of the
   guard stores the node vector and its `lsizenode` in the exit area and leaves through the generic stub tagged `EXIT_SHAPE`; the handler computes and stores the id
   (`shape::fill_node`), restores the snapshot like any exit and lands in the interpreter at the snapshot's pc, so the next entry passes the guard. It does **not** count the
   exit towards hotness (no side trace forms on a shape guard; mutant `shape_miss_counts_hot`). The handler's fills are budgeted per state (`ExitArea::shape_fills`,
   `shapedemote=N`, default 256): past the budget it sets `ExitArea::shape_nofill`, and the cold path then treats a zero id like another layout (point 5) instead of calling the
   handler (mutants `shape_demote_never`: the budget never ends, `shape_nofill_ignored`: the cold path ignores the flag). A loop that inserts a key every iteration, or meets a
   fresh table every iteration, is therefore bounded to 256 handler exits and nothing is re-recorded (see section 17 for why the earlier policy, flushing the family at 48 misses, was removed).
5. *A table whose slot holds another id* (another layout, section 17): the guard stands for the key compares of the `HREFK`s it covers, so the cold path does exactly those
   compares for every covered `HREFK` of the same node-vector reference (key tag byte and key address at node `idx`) and returns to the guard's successor when they pass. This is sound
   for the reason a covered `HREFK` is: the node vector has not changed between the guard and the covered `HREFK`s (point 2), so the key compare done at the guard's position is the
   compare of the `HREFK`; a failed compare leaves through the guard's own ordinary exit (counted towards hotness, a side trace can attach through the inline stub). A table whose
   layout differs only in keys the trace never reads passes as it did before S4 (mutant `shape_fallback_key_skip`: the address compare is left out).
6. *`shapemin` (default 3).* The assembler drops the guard of a node vector (and the `covered` bit of its `HREFK`s) when the trace covers fewer than `shapemin` distinct
   `(node index, key)` pairs of it: one compare per table visit against about two instructions saved per covered key. Dropping a guard nobody relies on, and un-covering an `HREFK`,
   is the pre-S4 code (mutant `shape_drop_keeps_cover`: the `HREFK`s stay covered, the guard is gone). nbody reads 7 keys of every body, so its code is unchanged.

**Measured** (instructions, `perf stat`, seed 0, release build, knob 0 vs 1 on one binary): nbody (1M steps) 2486.6M -> 2089.8M (-16.0 %, 2487 -> 2090 per step), awfy/nbody
-13.4 %; apps/raytrace +0.05 %, oop_methods -0.1 %, json -0.07 %, table_hash / binary_trees / spectral_norm / fib +-0.001 %; a loop that inserts a key every
iteration -0.2 %; fifty instances with differing extra keys: +0.7 % over ten rotations of the instance order (single runs scatter -25 % .. +25 % either way: the
demotion re-records the family at an arbitrary instance and the polymorphic trace tree of that site depends on it, with or without the knob).

**Cost model / when it loses.** One extra guard per table visit and one handler call per distinct table whose id is not computed yet (nbody: four). A site that sees other layouts pays
the cold path's key compares (about 5 instructions per covered key) per visit instead of one compare per `HREFK`; `shapemin` keeps guards that cover one or two keys out. `layout_id` scans
at most 64 nodes.

**Tests.** `tr_core::s4_shape_guard_programs` (19 programs x trace modes x exit storms / verification / GC stress), `s4_shape_guard_effectiveness_and_demotion`,
`tr_record::s4_covered_hrefks_are_dominated_by_their_shape_guard`, `tr_shape::shape_ids_are_content_addressed`, and the rn-core clears (`tests_shape.rs`).

## 16. Register-carrying transitions (R1, `regcarry`)

**What.** A trace-to-trace transition used to go through the stack: the exit stub of the parent stores the whole snapshot (tag and payload per slot), the side trace
re-loads what it reads, writes back what it changed at its link, and the target root re-loads its leading `SLOAD` run behind the guard-free second entry. An `nbody`
step makes six trace entries and five exits (the inner `j` loop averages two iterations, the outer iteration is a side trace that links back to the root), so most of
its instructions beyond the arithmetic are these stores and loads. With `regcarry` (`RUSTYNAIL_JIT_TRACE_LIMITS=regcarry=N`; 0 = off, 1 = both halves (default),
2 = exits only, 3 = links only) a transition between two traces whose both ends are JIT code leaves the values in registers (or spill slots):

* **Half B, link into the root (`HeadData::e3`).** The root that has a guard-free second entry (A2) also publishes a *third entry* `e3`: the point behind its leading
  `SLOAD` run (`exit::lead_run`), together with a layout `exit::CarryInfo` (per live leading `SLOAD`: slot, type, the register and the spill slots the allocation holds
  its value in at that point, `ra::Allocation::carry`, captured by the allocator after it has processed the instruction behind the run). A side trace whose link was
  compiled against that layout (`AsmEnv::carry_head` / `carry_sig`: the head's `e3sig` at compile time) ends with: signature check (`cmp [e3sig], sig`), the poll,
  the root's entry bookkeeping (`entry_ok`, `pending`: it is the root's entry), the root's `BUDGET n` charge (done by the link: the third entry is behind it), a
  parallel move from the locations of its writeback values (`end_locs`) to the root's locations (the allocator is told the root's registers as hints, so most moves are
  empty), loads from the stack for the slots the trace did not write, and `jmp [e3]`. A mismatching signature (the root was recompiled or flushed: `e3sig` is reset with `e2sig`) falls
  to the plain path, which first stores the entries the early writeback left out. After the signature check the link sets `HeadData::stale` (one store).
* **Half A, exit into a side trace (carry cells).** Behind the exit cells of a trace its allocation reserves one *carry cell* per snapshot (`cells[n + k]`, 0 until a
  side trace compiled for register-carried entry is attached). The inline stub of a frameless exit (no frames, no sunk tables, `top`, constants or dynamic strings in its
  map; budget in the register; not a chain site) costs nothing extra while no carrying side trace is attached: `cache::install` parks the counter of the exit at the
  divert value (`hot_exit - 1`) when it sets the carry cell, so the stub's existing equality test (`cmp [ctr], hot-1; je`) lands in a *cold block* (`Cx::carry_slow`,
  emitted behind the stubs) that looks at the cell: unset (the side trace was flushed, or the counter reached the value on its own) -> the generic twin as before; set ->
  the carry path, which adds `refund` to the budget register and `jmp`s to the **carry entry** of the side trace with every value where the exit map has it, storing
  only the tags the stack lacks. An exit that takes the carry path is therefore *not counted* (the counter stays at the divert value; the ineffective-trace rule and
  the statistics see only the other exits of the site). The side trace was recorded with the slots of the exit map pre-defined as *carried loads*
  (`SLOAD` with `SLOAD_CARRIED`, unchecked, emitted before everything else, `SlotMap::def` on the slot): every snapshot of it, snapshot 0 included, and its writeback
  therefore name them; they occupy a register from the start of the trace (the allocator skips their definition and records where they live at the start,
  `capture_carried_loads`, hinted to the parent's registers). The plain entry of the side trace (through the exit cell, after the handler or a stub stored the
  state) loads them from the stack first; the carry entry moves them from `TraceIr::carry_in` (the parent's locations).

**Soundness argument.** The one thing that changes is the invariant "the stack holds every slot that a snapshot does not name" (TR3, sparse snapshots): behind a
carrying transition the stack slots of the carried values are *stale*. Everything that can observe the stack is an exit, and nothing else can run on a trace (TR1:
no GC step, no hook, no error, no yield, no stack reallocation between an entry and the end of an exit handler), so it is enough that **every exit reachable after
a carrying transition restores the carried slots, tag included**:

1. *Roots (half B).* The cost of the dense entries is paid only behind a flag: `SlotRestore::dense` marks the entries `mod::carry_dense` added; the inline exit stub of a
   root tests `HeadData::stale` (`Mov V7, &flag; cmp dword [V7], 0; jne cold`) and only then, in an out-of-line block (`Cx::dense_cold`), clears the flag and stores the dense
   entries (payload; tag when not redundant), then jumps back; every other store of the stub is unconditional as before. The flag is set by every carrying link before it
   enters through `e3` and is cleared only by that cold block *after* its stores, so "flag clear => the stack is complete for the dense slots" holds whenever a stub
   reads it; a flag left set (an exit through the handler, `mat_stub`, `link_mat_stub` and the sunk/frame stubs store every slot including the dense ones
   unconditionally) only makes a later stub store values the stack already holds. The carry path of half A (above) hands the dense slots over in registers like any
   other slot of the exit map and leaves the flag set (the side trace's snapshots restore them). Beyond the flag, `mod::carry_dense` gives every snapshot used by a guard behind the leading run a `SNAP_CARRY` entry (slot -> the unmodified leading `SLOAD`) for
   each carried slot it does not name itself (carried = the run's `Int` / `Num` / table slots that no `SSTORE` of the trace writes: an `SSTORE`d slot is omitted from
   snapshots because the stack holds its value, which would not be the `SLOAD`'s). A snapshot shared by a guard of the run and a later one (the entry snapshot that
   hoisted callee guards keep) is copied for the later guards. The root's other uses of the stack are unchanged: it is entered from the interpreter with a complete
   stack, and the dense entries are equal to the stack there. The entry bookkeeping and budget of the plain second entry are replicated by the link; the poll and the
   budget of the link exit through a cold stub that stores the carried writeback entries first (`Cx::rc_cold`) and continues in the ordinary poll stub of the link.
   Those stores' tags are never elided: `build_exits` removes the dense slots from `ExitSnap::ents` (the tag the stack "provably" holds is the one of the guarded entry, which
   a carrying entry did not write).
2. *Side traces (half A).* A side trace entered through the carry entry has a stale stack for exactly the slots of the parent's exit map. All of them are defined by carried loads, so
   every snapshot of the side trace and its writeback restore them (they are ordinary defs to the recorder); the F4 pruning of dead numeric temporaries applies to them
   like to any def (the dead slot is never read before it is rewritten, so a stale value there is unobservable), also for the writeback at the link's pc
   (`mod::pipeline_inner`). `stack_ents`, `entry_ents` and `wb_ents` exclude carried slots (no tag elision, no inherited type), `link_proof` treats them as unknown unless
   the writeback names them. The carry entry skips the room checks of the plain entry, so a side trace with carried loads aborts (`carry-room`) when it needs more room
   than the parent's checks guarantee (`max_extent` / `max_ci`). **The entry snapshot** (TR6d: the snapshot of the hoistable callee-identity guards, the empty map at
   the head pc, `Recorder::entry_snap`) is taken from the slot map of snapshot 0 (`entry_sm`: the carried loads), not from an empty map: a side trace entered
   through the carry entry has a stale stack for the slots of the parent's exit map, and an entry guard that fails restores nothing but that snapshot (see the bug story
   at the end of this section).
3. *Chains.* Root -> side -> root: the side trace's snapshots are dense in the parent's map, the root's in the carried slots of its run; a side trace attached to a side
   trace takes over the exit map of its parent the same way. Nothing assumes that the stack is complete except the plain entries, and those are only reached after a
   stub or the handler stored the state.
4. *Flushes.* A carry cell is cleared when its parent or its side trace is flushed (`flush`, `carry_cells_kept` / `carry_cell_side_stale` mutants); `e3` / `e3sig` follow `e2` /
   `e2sig` (a compiled link whose `e3sig` no longer matches takes the plain path). The layout is interned and immutable.
5. *Registers.* `r14` (V10) is never a destination or a source (the reload of the constants pointer in front of every `JmpR` would clobber it); the stub's source registers
   exclude the scratch `V7`; a spill slot source/destination is a slot of the one per-state spill area, and the parallel moves (`ra::sequentialise_x`) read every source before
   any destination is written. The budget register is the same in both traces (`ra::BUDGET_GPR`, `membudget` off), else nothing carries.
6. *Observers.* Exits to the interpreter see a complete stack (all of the above); a hook or error in the interpreter after an exit sees the state `restore` leaves, as before;
   `Heap::verify`, GC roots and coroutine yields need nothing (no GC point on a trace, TR9); the entry bookkeeping counters are updated once per entry as in the plain path.
   Mutants (registered in `mutants::NAMES`, all caught by `tr_core::r1_regcarry_programs`): `carry_plain_no_store`, `carry_drop_move`, `carry_in_drop_move`,
   `carry_store_dropped`, `carry_exit_tag_skip`, `carry_no_stale_flag`, `carry_dense_skip`, `carry_sig_ignored` (needs `r1_live_flip`: a re-recorded root), `carry_wb_prune_live`,
   `carry_demote_copied`. Rules for which no live program was found that can tell (tried: all `r1_*` programs in every mode, budget 17, hooks, GC stress, exit storm), so
   they have no mutant: the poll and the budget charge of the link (omitting one only delays a yield / hook / budget exhaustion by one transition, nothing observable
   differs), the room check of the carry plan, `r14` as a carry destination, the flush of carry cells (a stale cell would need a flushed side trace re-entered through a
   parent that keeps its stub), the link's tag store (the stack tags are right already in every program tried), `carry_stored_slot`. They are covered by the
   argument above and by the `tr_asm` / `tr_exit` structural tests only.

**Bug story: the entry snapshot of a carried side trace (found with `carrymin=1,carrylive=0,sidemin=0`, fixed on `regcarry-fix`).** The recorder gives the guards that are
valid at the trace entry (callee identity of an upvalue / global / field call, no store before it, operands entry values) one shared snapshot, `entry_snap`: the empty
map at the head pc ("re-execute the iteration"). Snapshot 0 of a side trace entered through a carry entry names the carried loads, but this one was built from
`SlotMap::new(0)`, so it restored nothing for the slots the parent's exit left only in registers. When such a guard failed (the callee of the inner loop had been
replaced by another function, reached through an upvalue that changes between calls: `r1_callee_upval`, `r1_callee_alt`, `r1_callee_open`; a table field `t.f` behaves the same), the interpreter resumed at the head pc with the stale stack: wrong numbers (`6135667752.500` instead of `212.500`), and, when the stale
slot was a loop counter, an endless loop (the reported hang of the harness in this mode: the `for` limit / index slot held a leftover, the interpreter ran until killed;
`chain check` lines of the verbose log count the exits of the hot side trace that never stops). The root had the analogous problem solved by `carry_dense` (copy of the
shared snapshot for the guards behind the lead run); the side-trace half had no equivalent. The fix is the one line above, rule: **every snapshot of a trace entered
through a carry entry, the shared entry snapshot included, names every carried slot** (carried loads are ordinary defs to the slot map). Mutant `carry_entry_snap_bare`
(restores the empty map), caught by `tr_core::r1_regcarry_programs`. Reachability: the faulty path needs a side trace with carried loads (the family's root must
offer a third entry: with the default `carrymin=4` / `carrylive=1` that is a root with four live carried values, nbody-shaped) whose recording begins with an entry
guard before any store; no program with the default policy was found that misbehaved before the fix (about twenty variants, see the task report), every miscompiling
program needed `carrylive=0`; but nothing in the code depends on the policy, so the fix is needed for the default configuration too.

## lazy_root: the region root of lazy CallInfo chains (C70), the link-root hole and its fix

**Invariant.** `ExitArea::lazy_root` is one word per state (behind the budget word, shared by every coroutine of the state; no region survives a helper call,
a collector step or a nested entry, so one word suffices). Whenever a lazy-aware trace (`ExitMeta::lazy`) can run or exit, `lazy_root` is a `CallInfo` **on the chain from the running frame
down to the base frame** (the running frame itself, or one of its callers), and everything at or below it is complete. Outside a region the word keeps its last value
(it is never cleared on unwinding): it is only meaningful because every lazy-aware *entry* stores `root := r13` before anything can leave through the handler.
Stores: (1) the outer entry of a root trace, of a prelude and of a **root that ends with a link** (`entry_checks`, after the TR6 room checks, whose failure goes straight to tier 1 without the handler); (2) the root frame's native return
(`root := previous`); (3) `lazy_leave` (`root := running frame`). Inner entries (native push / return through `HeadData::rec`) and side traces inherit it. Readers: `exit_entry` (`meta.lazy && lazy_root != 0`
-> `lazy_leave`), the leave blocks, the native-return guards.

**The hole (fixed).** `entry_checks` stored the root under `side.is_none() || prelude`. A *root that ends with a link* (`pipeline_inner::link_root`, produced by `nest=2` frame links:
a loop-free self-recursive root whose inlined callee has a loop) is assembled with `env.side = Some(..)` and `env.head != 0`, so it matched neither arm and entered with whatever
root the last region left behind. The first exit of that trace after a deeper descent had unwound called `lazy_leave` with a root that is not on the chain: the walk
`while c != root` never met it, reached the base frame and dereferenced `(*null).savedpc` (SIGSEGV in a release build; the debug assertion "lazy frames are of one prototype" in a debug build;
with a stale root that happened to be an ancestor it rewrote the `callstatus` / `top` / `kcache` of complete frames, e.g. a `CIST_FRESH` frame called from `pcall`).
Reach: only `nest=2` (not a default; the default is `nest=1`, `sidenest=1`) in the tests; `instruction_count_identity` hit it in 3 of 28 `g1_room_deep` cells (the `nest=2,stitchmin=4` cells).
Fix: the store condition is `side.is_none() || prelude || head != 0`. Mutant `lazy_root_link_root_unset` (the old condition), detector `tr_core::bk_lazy_link_root_programs`
(`bk_lazy_link_root`, `g1_room_deep` with `nest=2`; plain programs `tests/vm/jit_lazyroot_01_rec_inner_loop.lua`).

**Defence in depth.** `lazy_leave` checks `previous` before it dereferences it and fails stop (`lazy_root_off_chain`, a `#[cold]` panic with the two addresses) if the chain ends without meeting the root;
the check sits in a loop that already tests the frame pointer, so the normal path pays one compare per completed frame in the Rust helper only.

## table.move over one table: guards between stores (BUNDLE-TM)

`lib_tmove` (`record.rs`) unrolls `table.move(t, f, e, t2)` of at most 16 elements as an array load and store per element. A move within ONE table
whose ranges overlap is not idempotent: a guard that fails between two stores exits to the `CALL`, which the interpreter re-runs from the start over the
half-moved table. BUNDLE-VA already read all source elements before the first store (`tmove_no_preread`); this is the second half. Each `array_set`
emitted its own metatable / bounds guards right before its store, and they stayed between the stores whenever the optimiser did not merge them
with an earlier guard (a destination index no source index covers, or `-cse`). Found by `tr_core::instruction_count_identity`, `br_lib` (`m2`:
`move(t,2,6,3) move(t,3,7,2)`) with `-fold -cse +exitstorm=7`: `1,3,3,3,3,4,4,8,9,10` instead of `1,2,3,4,5,6,6,8,9,10`.

**Reachability.** A guard between two stores can only fail on a storm exit (`+exitstorm`, every guard fails every N-th time): the bounds guards of a move
are monotone (the first one of the copy order decides) and the table does not change in between, the element type guards are all before the first store.
With the default options (CSE on) the stray guards are merged away, and the whole matrix (modes tr, tron, trdef, trdefon, trverify, trstorm 3 and 7, `-fold -cse`,
GC stress 2; programs `br_lib`, `tm_tmove_guards`, `tests/vm/tables_06_move_overlap.lua`) is clean without the fix; it only fails with `-cse` AND `+exitstorm`. No default-flag
program was affected; the fix removes the dependence on the optimiser.

**Fix.** When the move is within one table and every destination element is inside the array part, `lib_tmove` emits `array_guards_raw` for ALL destination
elements first (after the pre-read of the sources) and then the `ASTORE` (+ `TBAR`) sequence with no guard between them. Moves between two tables are idempotent and
keep the old path; the instruction count of every move is unchanged. Mutant `tmove_interleaved_guards` (registry 695), detector `tr_core::tm_tmove_guards_programs`
(program `tm_tmove_guards`); plain program `tests/vm/tables_06_move_overlap.lua`.

## 17. Regression study of the S4 / R1 work (nbody session): shape guards on polymorphic sites

**Symptom.** After the nbody work (S4 `shapeguard`, `hstoreguard`, `guarddiet`, `hkaddr`, `snapprune`, F2 side-entry guards, `regcarry`) twelve object-heavy programs executed more
instructions than at 108b935 (`corpus2/heldout/roguelike_turns` +88 %, `oop_mixins` +16 %, `classic_vector` +13 %, `game_loop` +12 %, `astar_jumper` +12 %, `awfy/cd` +11 %,
`deltablue` +8 %, `richards` +7 %, `prototype_objects` +6.5 %, `lru_cache` +3.4 %, `expr_lang` +1 %); instruction counts are deterministic, so this is not noise. Method: per-knob sweep on one
binary (`RUSTYNAIL_JIT_TRACE_LIMITS=shapeguard=0`, `hstoreguard=0`, `regcarry=0`, `guarddiet=0`, `hkaddr=0`, `snapprune=0`, `f2ifl=0`, and all of them), `RUSTYNAIL_JIT_VERBOSE=2`
(trace counts, re-records, exits, the new `shape-guard exits` figure) and `perf record -e instructions:u` against a build of 108b935.

**Findings (instructions vs 108b935, `RUSTYNAIL_SEED=0`).** `regcarry`, `hstoreguard`, `guarddiet`, `hkaddr` and `f2ifl` explain none of the twelve. `shapeguard=0` alone brings
roguelike_turns to -4 %, oop_mixins +0.5 %, classic_vector +0.3 %, game_loop -0.2 %, astar -1.9 %, richards -0.8 %; three programs keep a part that is not a knob:

1. **The shape guard is stronger than the guard it replaces.** A plain `HREFK` passes for every table that has the key at the same node, whatever else its layout holds; `HSHAPE`
   fails for any other layout. Sites that read objects with differing extra keys, insertion orders or phases (richards' task kinds, game_loop, astar, classic_vector's particles)
   left the trace through the shape exit, which was not counted towards hotness (no side trace), and after 48 such exits the S4 policy flushed the whole family and recorded it
   again with plain `HREFK`s. The re-record happens at an arbitrary iteration: classic_vector's second root took the rare path (80 % of its entries left through one side trace
   instead of 25 %), roguelike_turns lost its outer `while` trace to six `IntMiss` aborts (an absent integer key in `world.occ`: data-dependent, the recording started in another
   iteration) and was blacklisted, richards lost its `proto:172` loop. The 108b935 numbers have the same lottery, but the S4 re-record made it systematic (the first recording is at
   the earliest point, the re-record is not). Also the exits of the handler path do not use the inline exit stub (no patched jump).
2. **Zero ids are not storms.** The 48-miss rule also counted the zero ids filled by the handler (every table created after the trace was recorded needs one fill): deltablue fills 9011
   zero ids over 100k entries, richards 179, astar 16k, so the rule fired on programs that merely create objects.
3. **`snapprune` cost 5 % of prototype_objects** in the *recorder*: `snap::prune_info` (bytecode liveness, quadratic in the locals of a long chunk through `nact`) ran at the close of
   every recording (53 + 222 of them). It is now cached in the prototype's trace table (`cache::prune_cached`, `TrProto::prune`; not for `snapprune=2` or with a pruning mutant).
4. Not attributable: `lru_cache` keeps +3 % with every knob off (rn-core table changes, different trace formation), raytrace +0.7 %, havlak +0.5 %, cd +1.5 %.

**Fixes** (all in `crates/rn-jit/src/tr`, none changes what a trace may assume):

* The cold path of `HSHAPE` (asm `ShapeCold`) re-does the key compares of the covered `HREFK`s when the slot holds another id (section 15, point 5): sites with other layouts that
  keep the key where the trace expects it no longer exit; a key at another node leaves through the guard's ordinary, counted exit with a patchable stub, so a side trace forms.
* No demotion. The handler's fills are budgeted per state (`ExitArea::shape_fills` / `shape_nofill`, `shapedemote`, default 256); past the budget the cold path re-checks the keys
  of a zero-id table as for another layout. Nothing is flushed or re-recorded because of shape guards, `HeadData::noshape` is gone.
* `shapemin` (default 3, assembler): a node vector whose guard covers fewer than three distinct keys in the trace gets no guard (section 15, point 6). This is pure code
  generation: trace formation is unchanged, which is what makes the result robust (a recorder-level version of the same rule changed which iteration was recorded and moved
  programs by +-20 % in both directions).
* `snapprune` analysis cached per prototype.

**Result** (instructions, 108b935 -> before -> after this change): roguelike_turns 3166M -> +87.9 % -> -4.2 %; oop_mixins +16.3 % -> -0.1 %; classic_vector +13.1 % -> +0.1 %;
game_loop +11.7 % -> -7.2 %; astar_jumper +11.7 % -> -7.9 %; cd +11.4 % -> +1.9 %; deltablue +8.0 % -> +1.3 %; richards +6.6 % -> +1.6 %; prototype_objects +6.5 % -> -0.3 %;
expr_lang +1.0 % -> +0.7 %; lru_cache +3.4 % -> +3.8 % (not S4); real_dis_x86 -0.3 % -> 0.0 %; sink_vec -11.7 %, inline_2 -15.7 % unchanged. nbody is unchanged to the
instruction (programs/nbody -31.6 %, awfy/nbody -23.2 %). Check set (fib, fannkuch, queens, spectral_norm, matmul, mandelbrot, oop_methods, table_hash, binary_trees, json, bounce,
type_call, field_method, oop_override): identical to before the change.

**Mutants** (registry 698): `shape_fallback_key_skip` (`s4_shape_guard_programs`), `shape_nofill_ignored` (`s4_shape_guard_effectiveness_and_demotion`), `shape_drop_keeps_cover`
(`s4_shapemin_programs`, new program `s4_two_keys`); `shape_demote_never` now means "the budget of fills never ends". The S4 mechanism tests run with `shapemin=0`.

### 17.1 Second pass: the remainder was compile cost (instruction counts, 108b935 vs this change)

**Method.** Instruction counts depend on `argv[0]` and on how the script path is spelled (apps/lru_cache: 405.1M with `./rnlua`-style relative paths, 413.7M with the long absolute
path of the same binary), so a comparison needs both binaries started the same way: copy them to two paths of equal length (`/tmp/cmpA/rnlua`, `/tmp/cmpB/rnlua`), same
working directory, same script path, `RUSTYNAIL_SEED=0`. The earlier +3.0 % of lru_cache with every JIT knob off was mostly this artifact plus compile cost (the interpreter alone
is +0.2 %, so rn-core's layout-id slot is not the cause). To split run time from compile time the `phases` line of `RUSTYNAIL_JIT_VERBOSE=2` was temporarily switched from `rdtsc` to
a `perf_event_open` instruction counter in both trees (the phase timers are then retired instructions, deterministic): the whole difference of lru_cache, richards and deltablue sat in the compile
phases (pipeline: richards +3.5M, deltablue +5.6M, lru_cache +3.4M instructions of 181M, 334M, 402M), none in the trace code those programs run.

**Causes found and removed** (all pure compile-time work, no change in what a trace may assume or in the code a trace gets, except the third item):

* `exit::stack_ents_ex` (A1/G1: per snapshot, the types of the entry `SLOAD`s that precede its first guard) scanned every instruction for every snapshot, with a linear `stored.contains` and
  a linear duplicate test per candidate (snapshots x instructions x slots). It now builds the candidate list once (first `SLOAD` per slot, in IR order), a snapshot takes a prefix of it
  (`partition_point`), and snapshots with the same prefix share one `Arc<[(u16, Ty)]>` (`ExitSnap::ents` is an `Arc` slice: the three `ExitSnap::clone`s per inline stub no longer copy it).
  The same for `wb_ents_own` / `entry_ents` (a per-slot `SSTORE` bitmap instead of a scan of the instructions per slot), `wb_ents_ex` (a 256-bit set) and `build_exits_ex` (the frame window of a
  snapshot was summed over all earlier snapshots: now accumulated).
* The assembler looked its exit stubs and dispatchers up with linear scans of the stub list (`stub_label`, `poll_stub_label`, `shape_stub_label`, `disp_label`): now hash-indexed. `shape_dropped`
  was nodes x instructions: one pass. `inline_stub` cloned the whole `ExitSnap` three times to cut the slot list into head, tail and dense part: `exit::inline_stub_ops_slots` takes the slots as an argument.
* The cold path of `HSHAPE` compares each covered key address with `movabs v7, key; cmp [node + KEY], v7; jne` (as the plain `HREFK`, H3-F) instead of two 32-bit memory-immediate compares (one
  instruction less per key at polymorphic sites; `v7` is free between two IR instructions and dead at every exit).
* The layout interner and one recorder map used the std SipHash `HashMap`; both use `fx::HashMap` now.

**What is left.** With the knobs unchanged: richards, deltablue and cd vs 108b935 are listed in the table below. The remainder of richards and deltablue is run time of shape guards at
polymorphic sites plus trace-formation luck: richards moves by +-3 % between `shapedemote=16`, `64`, `256` and `1024` (180.6M at 1024, 188.9M at 16) without any change of the code a trace
contains, and by -1.5 % with `shapemin=8`, which loses nbody (+5.5 % with `shapemin=7`: nbody covers six keys of the bodies in some traces). A per-guard miss counter that drops the guard in code
generation needs the trace to be re-assembled, which is the re-record this section removed (the arbitrary re-record point is what made the earlier policy systematic); a counter that exits
after K misses turns every extra layout into a side-trace exit (about 100+ instructions against about 15 for the cold path) and was not built. The cold path costs, per guard visit with another
layout, one failed compare, the zero test, one tag byte and one 8-byte compare per covered key and a jump back; the plain `HREFK` form costs the key compares only.

**Result** (instructions vs 108b935, same invocation for both binaries, `RUSTYNAIL_SEED=0`; before this pass -> after): lru_cache +0.9 % -> +0.3 %, richards +1.4 % -> +0.4 %, deltablue +1.8 % -> +1.05 %, cd +0.8 % -> 0.0 %, havlak +0.7 %
-> +0.7 %, raytrace +0.6 % -> +0.3 %, expr_lang +0.4 % -> +0.1 %; roguelike_turns -4.3 %, game_loop -8.0 %, astar_jumper -8.2 %, classic_vector -0.1 %, oop_mixins -0.2 % (unchanged); programs/nbody 2027.3M,
awfy/nbody 555.6M (unchanged to 0.02 %). Against 9250356 no program of the check set moves by more than +0.01 %. Deltablue and havlak keep their remainder (the part of deltablue that is
not compile cost is the run time of its polymorphic shape guards: `shapeguard=0` is 0.6 % above 108b935).
**Mutants:** none added (registry 698): nothing here changes what a trace may assume or a guard's meaning; the mutants that pin the touched code (`entry2_transitive_stale`, `entry_inherit_wrong_slot`,
`tagskip_*`, `side_known_unmod_all`, `shape_*`, `carry_*`, the dense-store mutant) all stay caught.

## 18. H1: absent integer keys (H1a tier-1 get fast path, H1b = CP5 absent-key read in traces)

Source: `docs/STUDY_ALLOC_CALL.md` (H1; `binary_trees`' leaf `tree[1]` on `{}`, parked as CP5 in `docs/JIT_PARKED.md` C68).

**H1a (`rn-core/src/jit_helpers.rs::jit_get`).** `GETI` / `GETTABLE` with an integer key on a table: `TableRef::get_int` (the array part, then the hash part); a non-nil result, or a nil / absent result of a table
**without a metatable**, is the answer (`H_DONE`). A metatable present with a nil result, a float key (also an integral one: `GETTABLE` with a float register), a non-table or any other opcode keep the generic
`__index` chain below it unchanged. Instructions: `binary_trees 17` 19483 M -> 18774 M (-3.6 %), cycles -4 %. Tests: `tests/vm/jit_h1_01_absent_int_key.lua` (vm corpus against the reference: leaf reads, a
metatable added later / removed, `__index` function and table chains, array growth and shrink between reads, zero / negative / huge / float keys, error messages of non-table values).

**H1b (`tr/record.rs::int_absent`, called from `array_get_k` before `table_for`).** The read `t[k]` with `k` an `Int` (or an exactly integral float, `F2I_EXACT`) outside the array part, **absent from the
hash part** (`TableRef::int_node(k) == None`, so not even a nil-valued node) of a table **without a metatable** records:

1. the TR14 metatable guard (`metatable == 0`: a metatable added later may hold `__index`, so the trace exits);
2. `UGE key - 1, asize` (the array part has not grown over the key; 5.4: a key outside the real array part never touches `alimit` in the interpreter either);
3. `EQ lsizenode, #n` (the node count the walk below is compiled for);
4. `HREFV node, key` with the op bits `lsizenode | HREF_INT | HREF_ABS`: the `hashint` chain walk (main position `key % ((sizenode - 1) | 1)`, key tag `LUA_VNUMINT`, 64-bit key compare, `next` links) that
   **exits when it finds the key** and falls through when the chain ends. Both bits already existed (`HREF_INT` for present keys, `HREF_ABS` for the dynamic string-key absence proof; the assembler form
   covers the combination: no new IR op, no new aux form, `interp.rs` models it as "exit if a known node holds the key").

The result is the constant nil (no `HLOAD`). Soundness: the recorded path is the interpreter's `luaH_getint` -> `luaH_getn`-free hash miss -> `__index` absent, and every input of that path is a guard
(metatable, array size, node count, the walk itself). A key inserted into the hash part later is found by the walk (exit); an array part that grows over the key fails guard 2; a rehash that
changes the node count fails guard 3; a rehash with the same node count re-walks the new chain at run time (the walk reads the table when it executes, it does not embed a node index). Only tables without a
metatable are recorded (a table with a metatable keeps `mm_index_int` / `table_for` as before).
Knob: `RUSTYNAIL_JIT_HASHINT=0` (the integer hash path of H1) also switches this off.

**The second part of the fix: `fnpromo` kept the head's leaf-sample budget (`cache.rs::promote_fn`).** With the leaf recorded as a short trace instead of an `IntMiss` abort, the `FnShort` rule
(leaf-call samples; `MAX_FN_SHORT = 4` per head, `kl`) decided the fate of `ItemCheck`: its probe head took four leaf samples in a row (the retry distances are phase-locked with the pre-order of the tree), was
blacklisted, promoted to a generic root (`fnpromo`, "a fresh abort budget") and blacklisted again by the next leaf sample because `promote_fn` reset `k` but not `kl`: `binary_trees` then ran with *fewer*
traces than before (instructions +2 %, entered 22 k instead of 1.7 M). `promote_fn` now sets `kl = MAX_FN_SHORT - 2` (the promoted root gets two samples, not one). Price: a head that never records anything
but leaves pays one more recording (about 0.1-0.5 M instructions each: deltablue has several such heads; a full reset to 0 cost deltablue +1.9 %, havlak +0.8 %, awfy/json +1.0 %, so it was not used). Mutant
`fnpromo_kl_keep` (the old behaviour; `tr_core::fn_root_short_recordings_blacklist_the_head` now expects 6 `FnShort` aborts with promotion, 4 without).

**Tests.** `tr_record_calls::h1b_absent_int_key_records_its_guards` (IR: the metatable `EQ`, the `UGE` against `FLOAD asize`, the node-count `EQ`, the integer `HREFV` with `HREF_ABS`, no `HLOAD`),
`tr_core::h1b_int_absent` (program `h1b_int_absent`: binary-tree leaf recursion, leaf reads of tables that get an element, a table that gains and loses keys between calls of the compiled loop (hash insertion,
array growth over the key, rehash, nil-ing), a metatable with `__index` (function, table, empty) added and removed between calls, float / zero / negative / 2^40 keys; every trace mode with `+verify`, exit storms,
budget 17, `-loop`, `-fwd`, the process limits (recursion traces), stack-move stress, `RUSTYNAIL_JIT_HASHINT=0`), `tr_core::h1b_binary_tree_leaf_side_trace` (program `h1b_bintree`: at least one side trace under the
process limits), `tests/vm/jit_h1_01_absent_int_key.lua`.
**Mutants** (registry 698 -> 702): `iabs_meta_skip`, `iabs_array_guard_skip`, `iabs_lsize_skip` (the three guards; caught by the IR test, the last two also by `h1b_int_absent`; `iabs_meta_skip` also by
`h1b_int_absent` through its `hot` section), `fnpromo_kl_keep`. `habs_found_ignored` (the walk) stays caught by `dm_programs`.

**Results** (release `rnlua`, `RUSTYNAIL_SEED=0`, `taskset -c 4`, equal-length paths; instructions exact, cycles +-2 %):

| program | base fa155dd | H1a | H1a + H1b |
|---|---:|---:|---:|
| binary_trees 17 instructions | 19483 M | 18774 M (-3.6 %) | 15504 M (-20.4 %) |
| binary_trees 17 cycles | 4.01-4.07 G | 3.79-3.93 G | 3.27-3.35 G (-18 %) |
| `[trace] side traces` installed (n = 14) | 1 | 1 | 2 |
| lru_cache | 403.7 M | 399.9 M | 336.3 M (-16.7 %) |
| roguelike_turns | 3029 M | 3006 M | 2817 M (-7.0 %) |

Check set (instructions vs the base): fib 8210.6 M, fannkuch, queens, spectral_norm, matmul, mandelbrot, programs/nbody 2027.3 M, awfy/nbody 555.6 M, cd, game_loop, table_hash, json, bounce,
field_method, oop_methods, raytrace (+0.02 %) unchanged to 0.01 %; deltablue +0.07 %, havlak -0.3 %, awfy/json -0.4 %, richards +0.3 % (the extra recording of an abandoned head); astar_jumper
moves between 4.58 G and 5.07 G from run to run in both binaries (bimodal trace formation, not this change).

## 19. H4 / H2 / H5: cheaper table free, tier-1 constructor / call paths, faster rebuild of sunk tables

Source: `docs/STUDY_ALLOC_CALL.md` (H2, H4, H5). One commit per item; mutant registry 702 -> 720.

**H4 (`rn-core/src/alloc.rs`, `table.rs::free_table`, `gc/mod.rs`, `gc/generational.rs`).** Three changes, no observable behaviour: (1) `Mem` holds the default `PoolAllocator` by value
(`AllocImpl::Pool`, found through `Any`; every other allocator stays a `Box<dyn Allocator>`): the pool's alloc / free / realloc are direct calls the compiler inlines, not virtual ones (this alone is
-20 instructions per `{}`: allocation and free); (2) `Allocator::free_table_blocks` frees header, hash part and array part with one call and `Mem::free_table_blocks` subtracts the **sum of the accounted
sizes** once (the same bytes the three `free` / `free_counted` calls subtracted: `collectgarbage("count")`, the debt and the memory limit are unchanged; unit test `alloc::tests::free_table_blocks_accounting`);
(3) both sweep loops (`sweep_some`, `list_remove_after` of the generational sweep) free a table in the loop (`plain table` = tag test, and no trace-recorder watch list active, which `free_obj` still
handles) instead of entering the `free_obj` frame and its kind dispatch. Weak tables, ephemerons, finalizers and ages are not touched (they act before the sweep frees). Gates: rn-core tests, GC stress 1 / 2
with `RUSTYNAIL_GC_VERIFY=1` in both modes, vm corpus.

**H2a (`t1inline.rs`, `calls.rs::tpl_tailcall`).** `setmetatable(t, mt)` is a new `t1inline` row (`B_SETMT` = 1024, `t1inline` default 1023 -> 2047; shape `B = 3`, also `C = 1`, a call as a statement, whose
`R[A]` is not written and `top = R[A]`, exactly what `native_fast_with` leaves). Guards, all before any store, any failure takes the generic call with nothing changed (so every error message, hook,
Ctrl-C and count behaviour is the old path's): `R[A+1]` and `R[A+2]` are tables; `mt.flags` has the cached "no `__gc`" bit (`fast_tm(mt, Gc)` would return `None` without a lookup: the first call per
metatable takes the helper, which caches it); `t` is not black (the barrier `luaC_objbarrier` would be a no-op: white, or gray); `t.metatable == null` (no `__metatable` field to protect, no old
metatable to compare); the frame is not a metamethod frame (`CIST_MMRET`: with the inline store there, `instruction_count_identity` of `bd_mm_depth`, recursion through metamethods
to the C-stack limit, was off by 534 instructions, cause not found; the generic call there costs nothing that matters). Then `t.metatable = mt`, result `t` in `R[A]` and the dead copy at the slot where the native pushes it, `top` as above. These are the guards of the trace compiler's inline `SETMT`
(`Asm::setmt_fast`, section A1). **Tail position:** the t1inline rows are emitted at `TAILCALL` sites too (`site_hints` takes `Tailcall`; any `C` of the caller, `B` including 0 for `select('#', ...)`):
the result goes to `R[A]`, `top` after it, and the frame is **returned right there** by the body of the `RETURN` that the compiler always emits after a `TAILCALL` (`B` = 0, no `k`; the
inline path is not emitted otherwise): `return_prologue_noexit` + `return_body` (the result move of `tpl_return`, shared), `CIST_FRESH` / the caller's resume entry through the shared `ret_tail` sequence. There is no
exit point between the native result and the return (an exit at the `RETURN` would leave the interpreter in front of an instruction it never executes after a native tail callee: `tr_oracle` reported
exactly that on `vararg_04_adjust_and_call` with a first version that jumped into the `RETURN` template), so everything that could make the return leave is decided before anything changes and sends the call
to the helper: a hook (`trap`), a frame with to-be-closed variables, a metamethod frame (`CIST_MMRET`, which returns through `Cold::MmRet`) and the base frame. The return is not charged (the interpreter
does not execute the `RETURN` either). Mutants: `t1i_setmt_gc_skip`, `t1i_setmt_black_skip`, `t1i_setmt_old_skip`, `t1i_setmt_arg_skip`, `t1i_tail_count_extra` (the inline path also charges the `RETURN`), `t1i_tail_mm_skip`.

**H2b (`templates_table.rs::emit_newkey`, `tset` bit 4, default 31).** `SETFIELD` of a constant short-string key: after the inline chain walk misses, the cold stub `Cold::NewKey` stores the key and value at the
**main position** of the key when that node's value is empty (what `insertkey` does first; the fall-through path of an existing key is unchanged, no extra jump). Guards before the first store: the table has a hash
part (not the dummy node: `BITDUMMY`), is not black (the helper's key / value barrier; strictly stronger than needed), the value is not nil (the helper answers that no-op), the metatable is absent or its flags
have the cached "no `__newindex`" bit (`no_tm`). Stores: key tag + key address, value tag + payload, `flags &= ~MASKFLAGS` (`invalidate_tm_cache`: the key may be a metamethod name), and **the layout id slot
at `node - 16` is zeroed for parts of 8 or more nodes** (section 15 rule 1: every change of which key lives in which node clears it). A taken main position (collision, free-position scan, node move), a full part
(rehash) and everything else exit to the old helper (`jit_set`) before anything changed. Mutants: `newkey_black_skip`, `newkey_nil_value`, `newkey_nonempty_skip`, `newkey_mt_skip`, `newkey_flags_skip`,
`newkey_layout_skip` (detector `tr_shape::shape_tier1_newkey_clears_the_id`, the layout id is not observable from Lua), `newkey_mainpos_off`, `newkey_dummy_skip`.

**H2c (`calls.rs::tpl_tailcall`).** The R1 inline tail call already existed for fixed argument counts; `return f(a, g(...))` has `B = 0` (the last argument is a call, the count comes from `top`), which is the
shape of `awfy/list` (`return tail(tail(..), tail(..), tail(..))`; a debug run showed every `jit_tailcall` of the program had `B = 0`). The inline path now takes `B = 0`: `top` is read from the thread, the guard
`numparams <= nargs` uses `nargs = top - (func + A + 1)`, the slots are moved by a loop (`top - slot(R[A])` slots over the frame's function slot, destination below source), `thread.top = top - (A + 1)` (= `func + narg1`),
everything else (callee compiled with a function entry, room, `k` with open upvalues, vararg caller) as for fixed `B`. Mutants: `tailfast_dyn_count_short`, `tailfast_dyn_top_high`.

**H5 (`tr/exit.rs::materialise`, `table.rs::insert_absent_shortstr`).** A sunk table is created with the `NEWTABLE` sizes and its fields are inserted in recorded order. For a table whose keys are distinct and fit
the presized part (`SunkTable::fast`, decided once when the snapshot is built: `fields.len() <= hsize`, no repeated key) the fields go through `insertkey` directly (no lookup, no type dispatch, no error
plumbing; a nil value stores nothing as `set_str` does; no free node falls back to `set_str`), and the flags are invalidated once after the loop. Same final sizes, same insertion order, hence the same node layout,
`next` order and layout ids as `set_str` (`insertkey` is what `set_str` ends in); the fault-injection counter of `set_materialise_fault` sees the same points. The recorder never produces repeated keys or nil fields
(measured: mutants making `seal` ignore repeats, store nil fields, or ignore a full part are not reachable), so those checks are defences, not rules with mutants. Mutants: `sinkfast_order_reversed`,
`sinkfast_flags_skip` (a materialised table used as a metatable afterwards).

**Tests.** `tr_core::h2_setmt_programs` (program `h2_setmt`: every call form, abnormal arguments and their error messages, protected / old / `__gc` / weak metatables, old and black tables in both collector
modes, metamethod frames, a replaced function, hooks, coroutines, tail calls of the other inline rows), `h2_newkey_programs` (generated insertion sequences read back in `pairs` order under a pinned hash seed,
metamethod flags, `__newindex`, nil values, old tables, 8+ node parts under shape guards), `h2_tail_programs`, `h5_sink_next_programs`; `tr_shape::shape_tier1_newkey_clears_the_id`; `tests/vm/h2_tier1_setmetatable.lua`
(the vm corpus against the reference). Every one runs in the trace modes with `+verify`, exit storms, count budgets, GC stress 1 / 2 (both modes) and the stack-move stress; the `h2_*` programs also with the
limits off.

**Results** (release `rnlua`, `RUSTYNAIL_SEED=0`, `taskset -c 4`, equal-length paths, instructions M; base 3e2b827):

| program | base | H4 | + H2a | + H2b | + H2c | + H5 |
|---|---:|---:|---:|---:|---:|---:|
| micro `{}` (insns / iteration) | 273.5 | 253.5 | 252.5 | 252.5 | 252.5 | 252.5 |
| micro ctor, tier 1 only (traces never form) | 1756 | - | 1392 | 924 | 924 | 927 (final 914) |
| micro setmetatable, tier 1 only | 638 | - | 404 | 404 | 404 | 404 (final 408) |
| raytrace 40 | 7704 | 7641 | 6758 | 5781 | 5781 | 5752 (final, after the H2a fix: 5725, -25.7 %) |
| cd 20 | 12053 | 11939 | 11828 | 11503 | 11502 | 10874 (final 10871, -9.8 %) |
| list 7000 | 12936 | 12937 | 12929 | 12925 | 12719 (-1.7 %) | 12719 |
| binary_trees 17 | 15510 | 14781 (-4.7 %) | 14751 | 14751 | 14751 | 14752 (-4.9 %) |

The tier-1-only rows are measured with `RUSTYNAIL_JIT_TRACE=on RUSTYNAIL_JIT_TRACE_HOT=2000000000,2000000000` (the tier-1 inline paths are, by the existing rule, off with `RUSTYNAIL_JIT_TRACE=off`, which stays J4
exactly; that setting still shows only the H4 gain: ctor 1729 -> 1708). Cycles: raytrace 2032 -> 1448 M, cd 3211 -> 2953 M, list 2566 -> 2360 M, binary_trees 3399 -> 3148 M.
Check set vs the base (instructions): fib, fannkuch, queens, spectral_norm, matmul, mandelbrot, nbody (2027.8 M), awfy/nbody (555.7 M), field_method, oop_methods +-0.02 %; richards +0.27 %, game_loop +0.2 %,
table_hash +0.76 % (instructions; cycles -7 %: the string-allocation path of H4 and a codegen shift in `table.rs`, not a path any of these changes added), deltablue -0.5 %, havlak -2.2 %, json -0.1 %, bounce -0.3 %,
lru_cache -0.2 %, roguelike -0.1 %.

## 20. H6 study: root-frame `TAILCALL` (2026-11-20, base 9d5d359; study only, no code change merged)

**Question.** docs/STUDY_ALLOC_CALL.md (H6) expected list 7000 -25..40 %, richards -5..10 %, raytrace / cd -3..8 % from recording a `TAILCALL` of the *root* frame
(the callee replaces the root frame and the trace goes on in it, instead of `Abort::TailCall` / the stitching exit before the instruction). The study was written
on 324e3e0, before H2c (inline Lua tail call in tier 1, section 19). The numbers below were measured on 9d5d359 (release `rnlua`, `RUSTYNAIL_SEED=0`,
`taskset -c 4`, `perf stat`), with scratch patches that were reverted (nothing in `crates/` changed).

**What the recorder does today at a root `TAILCALL`** (`Rec::tailcall`, `cur_depth == 0`): a function-entry root (`fn_root`) ends with the always-taken exit before the
instruction (`fn_end`, tier 1 runs the tail call; this is the final exit of such a trace and is an inline stub, not a handler exit); every other root (continuation
heads, side recordings whose root is the frame of the exit) aborts `TailCall`. A `TAILCALL` of an *inlined* frame (depth >= 1) already replaces that frame
(`call_lua(.., tail = true)`, `FrameLink::tail`); setmetatable / `string.*` / intrinsics / `__call` tables in tail position are recorded as the native followed by a `RETURN`.

**Findings.**

1. *List is not blocked by `TailCall`.* `RUSTYNAIL_JIT_VERBOSE=1` on `awfy/list 3000`: aborts `FnShort` 6, `StartRefused` 40, `Recursion` 1, `Unstable` 1, **no `TailCall`**. The
   continuation heads of `tail` (pc 4, 12, 17, 22) never start a recording: `driver_start` refuses a continuation head (`recany`) of a prototype that is not known to be
   recursive (`!proto_recursive && !cok`), and `tail` never becomes known because every recording started at its entry (pc 0) takes the leaf path (`return z`: `FnShort`,
   6 times, then blacklisted; with `fnroot=1` the same recordings abort `LeaveFrame`). `fnmin=0`, `contok=1/2`, `recnat=0`: instructions unchanged (12717 M) or worse (recnat=0: 13224 M).
   `tier 1` is 47.5 % of the cycles (`tail`'s own tier-1 code) plus 15.5 % (`isShorterThan`); `pre_tail_call` / `jit_tailcall` are gone from the profile since H2c.
2. *With the refusal lifted* (scratch: `driver_start` ignores the `recany` refusal) the four continuation heads do record and then abort `TailCall` (4 aborts, as the study
   said): list 12717 -> 12556 M instructions (-1.3 %), cycles 2332 -> 2299 M. If in addition those recordings end before the root `TAILCALL` with the stitching exit (the cheapest
   possible "handling" of the instruction: `end_exit`), 11 traces install and list gets *worse*: 13321 M instructions, 2500 M cycles (the continuation traces are entered 14 M times and
   exit before the tail call into tier 1 again). Recording `tail`'s root `TAILCALL` for real would continue in `tail` itself (a self tail call), which only pays once the entry head of `tail`
   exists as a trace, and that is the `FnShort` / leaf-path problem above, not a tail-call problem.
3. *Cost of a root tail call that does reach tier 1.* Micro (`scratchpad/mt.lua`, `string.gsub(s, ".", f)` so that `f` is entered from C, 10 M calls): `f = function(c) return g(c, 1) end`
   (trace of `f` ends before the `TAILCALL`, tier 1 does the tail call, `g` runs in tier 1) 878 instructions per call and 1514 M cycles; `local r = g(c, 1) return r` (`g` inlined into
   `f`'s trace) 848 instructions, 1498 M cycles; `return c` 631. The whole transition (final exit stub, the inline tier-1 `TAILCALL` of H2c, the callee's entry) is **30 instructions
   (3.5 %) per call on a loop of 600 instructions of C-call overhead**; there is nothing like the 260 instructions of the old `pre_tail_call` path left to remove.
4. *Cheap proxy of "root `TAILCALL` handled" on the targets* (scratch: every non-`fn_root` root `TAILCALL` ends the recording with the stitching exit before it, which removes every
   `TailCall` abort of the programs; instructions M, base -> proxy): richards 500 2846 -> 2861 (+0.5 %), raytrace 40 5905 -> 5826 (-1.3 %), cd 20 10874 -> 10871 (0 %), list 7000 12717 -> 13321
   (+4.7 %, finding 2). The `TailCall` aborts that exist at all: richards 15 (side), raytrace 1, cd 2, list 0 (4 behind the refusal). The hot root-`TAILCALL` exit of raytrace is `Sphere:normal`'s
   `return p:sub(self.c):mul(..)` (7426 exits in the run, after which the rest of `trace()` stays in tier 1: that is the `trace()` coverage problem of H8, not the tail call).

**Conclusion (not built).** Handling the root `TAILCALL` as the spec asks (a root replaced by the callee, exit snapshots that rewrite the root `CallInfo`: new function slot value at slot -1,
`top`, `kcache`, `CIST_TAIL`, a per-snapshot root prototype; stub support in `inline_stub_ops`; the optimizer passes that assume one root prototype per trace: sink, peel, `snapprune`, dead-slot
analysis; the `Seg` / lockstep / VM-side re-root machinery of RETF as the nearest precedent) is a large change whose measured upper bound on the four targets is 0..3 %
(finding 3: 30 instructions per transition; finding 4: the proxy), not the -5..40 % of the study, because (a) H2c already made the tier-1 tail call cheap, and (b) list's tier-1 share comes
from the recursion / continuation start policy of `tail`. It was therefore not implemented: the work does not pay for the soundness surface (new mutants, twins, five passes to teach).

**What would pay instead** (not done): (1) for list, make the entry head of a recursive function with a data-dependent leaf record the *non-leaf* path (a recording armed at a leaf could be
re-armed once, or `fn_min` / leaf detection could count the invocation as a "probe" and not as an `FnShort` towards the blacklist) so that `proto_recursive` becomes true and the continuation
heads record; the root `TAILCALL` of the continuation at pc 22 then needs only the `end_exit` handling of finding 2 *together with* an installed entry trace of `tail`; (2) H8 diagnostics (why
`trace()` of raytrace stays in tier 1); (3) if root tail calls ever matter, the cheaper variant is the "final exit after the call": a snapshot of the replaced frame (callee proto, pc 0, args moved
down) landing on the callee's function-head label, which needs the root-rewrite restore above but none of the recorder model changes.

## 21. H7: recursion protocol diet (2026-11-20, base 5855559; sub-gains built, the big items measured and not built)

**Task.** docs/STUDY_ALLOC_CALL.md H7 expected `fib 37` -15..-25 %, `towers 900` -5..10 % from (1) a callee-identity hoist, (2) polls / budget / room checks only at function entry,
(3) fewer flag stores, (4) direct return links. Method: release `rnlua`, `RUSTYNAIL_SEED=0`, `taskset -c 4`, `perf stat -e instructions:u,cycles:u`, equal-length binary paths, same script path;
instructions are exact, cycles carry +-1..2 % (min of 3 for the table below).

### 21.1 Where the instructions are (`fib 37`, base 5855559, disassembly of the four fragments: `RUSTYNAIL_JIT_DUMP=dir`, `objdump -D -b binary -mi386:x86-64 -M intel trace-N.bin`)

Internal node = 39 M, leaf = 39 M, 78 M calls. Instructions executed per visit of a fragment (hot path, counted from the disassembly):

| piece | entry trace (trace 3, internal node) | continuation pc 7 (trace 6) | return + continuation pc 11 (trace 5) |
|---|---:|---:|---:|
| `POLL` (`movabs; cmp byte [rax],0; jne`) | 3 | 3 | 3 |
| `BUDGET` (`sub r11; jl`) | 2 | 2 | 2 |
| tag + base-case guard, `n` load | 5 | 2 | 4 (two tag guards) |
| callee identity chain (`SLOAD fn; UREF; UVREL; UGE; ULOAD; EQ`) | 15 | 15 | - |
| frame push (4 slot stores, `ci.next`, callee `func`, caller `savedpc`, 3 pointer moves, stack room 4, `ci` room 3, first flag 1, jump) | 22 (before this work) | 18 + cell dispatch 5 | - |
| native return: guards (`previous`, `lazy_root` test, `savedpc` compare chain over the sites) | - | - | 8..11 |
| native return: pop (`previous`, `savedpc`, root test again, `SetCi`, budget store, site compare chain again, frame move, cell dispatch) | - | - | 17 |

`perf stat` of `fib 37`: IPC 6.5, `de_no_dispatch_per_slot.backend_stalls` 36 % of the slots, 738 M store-forwards (9.5 per call). The loop is throughput-bound with a
store-forwarding component: instruction removals help cycles at about a quarter to a half of their instruction share (measured below), removals of memory round trips would help more.

### 21.2 Built (all encoding-level, no new soundness conditions)

| commit | change | `fib 37` instructions | mutants / tests |
|---|---|---:|---|
| H7a | micro-op `AddFrame(imm)` (`add rbx, imm`) replaces `lea` + `SetFrame` in the native push (eager and lazy), the native pop and `RETF` | 8210 -> 8054 M | `addframe_push_off` (caught by `pm3_programs_run_on_traces`); pop is covered by the existing `rec_ret_base_off`; emulator moves its frame base like `SetFrame` |
| H7b | `UVREL` loads into its destination and subtracts the frame: no extra `mov` (3 -> 2 instructions after the load, once per identity chain) | | `tests/vm/h7_01_upval_alias_frame.lua` (open upvalue at register 0 / 1 / 3 of a loop frame) in `tr_core::h7_recursion_programs`; no mutant: the range guard is unsigned and any wrong offset only makes it fail or pass in the same cases for real stacks (a survivor was found and the mutant dropped) |
| H7c | stack-room and spare-`CallInfo` checks compare against the memory word (`BrMR` now takes every condition, `BrM` for the one-level `ci.next` test): V0 is no longer loaded | 8054 -> 7780 M | `stack_cmp_strict` (boundary frame let through), `ci_next_wrong_word`, both caught by `tr_asm::hand_entry_checks_tr6` |

Registry 720 -> 723. New programs (`tests/vm/h7_01..07`, also `tr_core::h7_recursion_programs`, every trace mode, `+verify`, `recnat=1`, exit storms, small budget, GC stress 2): recursion shapes (fib,
ackermann, mutual, tak, global function, callee replaced mid-run), varargs / closures / upvalues in recursion, depth to `stack overflow` (depth reached and message must equal the interpreter's;
C-stack limit by `pcall` nesting), errors / `pcall` at every level / tracebacks at depth, coroutines yielding inside recursion, count / call-return / line hooks and a hook installed from inside the recursion.
The programs do not print exact depths (a harness frame shifts `debug.getinfo` depth and the overflow depth by one against `ref/55/build/lua`: 499994 vs 499995 in `vm_corpus`; the same program run directly equals the reference).

### 21.3 Measured ceilings, not built (scratch patches, reverted)

1. **Callee-identity chain removed everywhere (unsound ceiling).** Skipping `UVREL / UGE / ULOAD / EQ` of every native-push trace: fib 7780 -> 6842 M instructions (-12.1 %), cycles 1254 -> 1190 (-5.2 %). Half of it
   is the entry trace's chain, half the continuation's.
   *Entry trace (self push) is locally sound:* the pusher has just verified `u.value == C` for the same closure `C` and nothing runs between the push and the callee's own check; the `UGE` range test of
   the callee follows from stack discipline (an open upvalue of the running closure points below the running frame or into another stack, never into a frame that does not exist yet). It would need a second copy of the
   body prefix (poll, budget, `SLOAD n`, guard) that enters behind the chain, with snapshot / RA bookkeeping for the copy, and is worth about -6 % instructions / -2.5 % cycles on fib and nothing elsewhere (towers
   uses method calls, no upvalue callee). *The continuation's chain cannot be hoisted without a family invariant* (no `SETUPVAL` of the callee upvalue in any trace reachable natively from the subtree that ran in
   between, nor in any helper-called Lua code), which is what C70 (b) already concluded. Not built: complexity (a second assembled body) against 2.5 % cycles on one program shape.
2. **POLL only at function-entry loop tops (CP3 retest).** Fragments that end in a native push through a link cell (continuations) without their own poll: fib 7780 -> 7663 M (-1.5 %), cycles 1250 -> 1225 (-2.1 %),
   towers 900 646 -> 638 cycles. Also dropping the poll of native-return fragments: 7546 M (-3.0 %), cycles 1217 (-2.3 %). So CP3's cycle loss of C68 was layout, as suspected.
   **Not built because soundness is not arguable:** a native push does not run `luaD_precall`'s hook test. The poll in front of every push is what makes a pending hook (`TRACE_POLL` is also set by `sethook`
   from a finalizer or another coroutine) stop the native chain. If the continuation's poll is dropped, the push goes through `rec_link_cell`, whose target is the entry trace only while that trace is installed:
   with a counting stub or tier 1 behind the cell nothing polls and the callee frame starts without its call hook. Making it sound needs the continuation to depend on the entry trace (flush with it) or a poll in the
   counting stub; the return fragments would in addition delay an interrupt by the length of an unwinding chain (bounded by the stack depth, but visible to the hook-timing tests).
3. **Fusing the return guards and the pop** (the guard loads `previous` / `savedpc` and tests the sites; the pop loads them again and walks the same sites): 5..8 instructions per return by reserving `V1`..`V3`
   across the body (`ra::op_clobbers` on `BUDGET` of `rec_ret` traces, reload after a helper call). Not built: about 2 % of fib, an allocator constraint on every return trace.
4. **Direct return links / never-zero rec cells** (`movabs; jmp [cell]` instead of `movabs; mov; cmp; je; jmp`): 3 instructions per dispatch, needs a per-trace fallback target in the shared cell or patching on
   flush. Not built: the cells are shared by every trace of the head, a null cell is the "no trace" signal of `lazy_rec_jump` and `cache.rs`.
5. **CP4 (first-flag store)** stays unsound (docs/JIT_PARKED.md C68): the flag lives in the shared exit area, nothing clears it when a region is left.

### 21.4 Results (base 5855559 vs this branch, release, instructions M / cycles M, one run, cycles +-2 %)

| program | instructions | cycles |
|---|---|---|
| fib 37 | 8210 -> 7780 (-5.2 %) | 1247 -> 1249 (3-run: 1263..1294 vs 1263..1268) |
| towers 900 | 2919 -> 2897 (-0.8 %) | 642 -> 643 |
| binary_trees | 7468 -> 7304 (-2.2 %) | 1551 -> 1504 |
| queens (programs) | 5528 -> 5391 (-2.5 %) | 1875 -> 1857 |
| fannkuch / spectral_norm / matmul / mandelbrot / nbody / awfy nbody | -0.5 / 0 / 0 / -0.3 / -0.4 / -0.4 % | noise |
| richards / deltablue / havlak / raytrace 40 / cd 20 / list 7000 | -0.6 / -0.3 / 0 / -0.4 / -0.5 / -0.6 % | havlak 147 -> 150..158 and raytrace +-3 % at equal instructions: layout noise in the repeats |
| lru_cache / roguelike_turns / game_loop / json / bounce / field_method / oop_methods / awfy towers / awfy queens | -0.6 / -0.6 / 0 / -0.1 / -0.6 / 0 / 0 / -0.5 / -0.8 % | noise |

No program is above +0.0 % instructions. The H7 estimate (-15..25 %) is not reached: the sound part is -5 %; the rest of the estimate is the identity chain (ceiling -12 %, 2.5 % cycles reachable locally)
and the poll (ceiling -3 %, unsound as stated).

## 22. K4: the 5.4-only regressions of session-h (2026-11-20, base 61cee86)

**Task.** `bench/results/session-h` (subject 8c39465 vs 1a5aad0): on 5.4, `apps/expr_lang` +22 % instructions (5.5: -23 %), `apps/raytrace` +6.8 % (plain) / +10.5 % (PGO) instructions
(5.5: -2 %), `lib/gsub_fn`, `lib/tonumber_bad`, `diff_levenshtein` +16..26 % cycles at equal instructions (5.4 plain only), and +1.5..3.7 % instructions on a dozen `lib/` programs.
Method: scratch worktree per commit, `LUA_VERSION=5.4 RUSTFLAGS="--cfg rustynail_lua54"` release builds, `RUSTYNAIL_SEED=0 perf stat -e instructions:u,cycles:u`, `taskset -c 4..7`, the
script given by the same path for every binary. **Instruction counts depend on the script path** (the chunk name is part of the heap: GC pacing moves with its length): `raytrace` takes 505..515 M
(1a5aad0) and 526..541 M (HEAD) for script paths of 7 .. 16 characters, `regex_nfa` is bimodal (5.56 / 6.82 / 7.70 G for the same binary at three path lengths, 1a5aad0 as well), so every
comparison below uses one fixed path (the main checkout's `bench/` for the tables, a directory-name sweep for the path checks) and instructions, not cycles.

### 22.1 `apps/expr_lang` (5.4): H2c changed the recording moment, `innerside`

`git bisect` on instructions, 5.4: b9afe4c 641 M, 3e2b827 (H1b) 640 M, 15570aa (H2a) 640 M, c9f9ee9 (H2b) 635 M, **816f74b (H2c, inline Lua tail call with `B = 0`) 787 M**, HEAD 764 M. On 5.5 the same commit
gives 598 -> 561 M (-6 %, the intended tier-1 gain) and the verbose trace summary is identical before and after. On 5.4 it is not: the loop of `eval` (`for i = 1, #args do vals[i] = eval(args[i], env, prog) end`,
recursive) was aborted `InnerLoop` at its recording before H2c and is *installed* after it (the path of the one iteration that gets recorded depends on the data at the recording moment, and H2c moves that moment:
`return evalBin(.., eval(..), eval(..))` is a `TAILCALL` with `B = 0`; with the inline `B = 0` path off the run is 622 M). The installed trace is 111 KB (the recursion unrolled `unroll` times) and spawns 198 side
recordings, 120 failed (53 / 55 of them `InnerLoop`: the path of an exit inside the inlined recursion reaches the loop of the next level, by construction, whatever the data), the pipeline grows from 11 to 42 M TSC
cycles (44 -> 117 compiles) and cycles +50 %. `polyside`, `maxside`, `loopany`, `sidestitch` and `maxdepth` do not remove it (instructions 764 -> 762 .. 739 M); `polyside=0` gives 683 M; a family-wide limit on
*all* failed side recordings (8 .. 16) cures it (624 .. 672 M) but costs 2 programs +10 % (`oop_mixins`, `prototype_objects`: their failures are normal, their traces pay).

**Rule `innerside` (`TrLimits::innerside`, default 4, 0 = off).** Family counter `side_inner` (family root, next to `side_faults`: side recordings that aborted `InnerLoop`); once it reaches `innerside`, an exit
*inside inlined frames* of that family gets no side recording (blacklisted at once, reason `InnerFamily`, like `MaxSide`). Counter-based, deterministic, no timing. The exits stay in tier 1, where they were before.
Sweep over all 223 programs on 5.4: `innerside=2`: expr_lang 615 M, stream_closures -7.7 %, richards +2.1 %, real_dis_mips +2.1 %; **4**: expr_lang 625 M, stream_closures -7.7 %, nothing else beyond +-1 %;
8: 650 M; 16: 694 M. Mutant `innerside_ignored`; `tr_core::k4_innerside_programs` (`k4_eval_loop`, a recursive evaluator with a loop over the arguments: `InnerLoop` side aborts 34 unlimited, 16 with the default,
8 with 2; the output equals the interpreter's in every mode: `+verify`, exit storms, budget 17, GC stress 2, `+emurun`).

### 22.2 `apps/raytrace` (5.4 and 5.5): spare `CallInfo` nodes at the entry of a root, `cispare` in place

`git bisect` on instructions (5.4, script path of 2 characters): b9afe4c 494 M, 15570aa (H2a, inline `setmetatable` / native tail calls in tier 1; its parent is H4) 612 M, its fix 054c888 540 M, HEAD 539 M.
5.5: 480 -> 497 (15570aa) -> 439 M (054c888). With the tier-1 `setmetatable` row switched off (`t1inline=1023`) 5.4 gives 435 M (-19 %) and 6197 M at 40 rounds against 7545 M, 5.5 gives 414 M, and no other program
of the 223 moves by more than 1.5 % in that run: the row is only the trigger. The summary lines show the mechanism: `cispare: entry failures repaired` 1343 on 5.4 (58 before), 271 on 5.5; `entered` 87 K against 174 K
trace entries; 24 `flush Unsink` (the `sinkroot` rule re-records roots without sinking) against none; with `sinkroot=0` 676 M instead of 539 M.

*Cause.* TR6b, the entry check of a root, wants `depth` spare `CallInfo` nodes behind the frame. Every collection shrinks the chain (`luaE_shrinkCI`: half of the unused nodes), and tier 1 with inline built-ins
(`t1inline`: `setmetatable`, `math.floor`, ...) never makes the call that would extend it again. A failing check booked an entry failure; only after `ENTRY_FAIL_MIN` = 64 of them did the C36 persistent-failure rule
repair the chain (`jit_tr_entry_fail` -> `ci_spare_repair`) and restart the account. An allocation-heavy loop (raytrace allocates a vector per operation) therefore lost 64 trace entries per collection
(1343 x 64 = the 87 K missing entries, exactly), and the pre-roll exits of the failing entries fed the `ineffective` / `sinkroot` rules. Why 5.4 loses more than 5.5 (the repair counts differ 5:1) was not isolated;
inline `setmetatable` makes it worse because `vec` used to extend the chain through the real call.

**Fix (`cispare` is now a bit set, default 3).** Bit 2: the TR6b check at the entry of a root (`room_ci_check_root`, from `entry_checks`: the first and the guard-free second entry) takes a cold path on a missing node:
`jit_tr_ci_spare_root` (the same `ensure_spare_ci_after` as the CP1 repair of natively entered frames, new stat `ci_root_repaired`, line `cispare: root entries repaired in place`) allocates `depth` nodes behind the
running frame and the entry goes on; an allocation failure takes the entry-failure label as before. Bit 1 is the C36 rule, unchanged (`cispare=1` is exactly the old behaviour; the in-place repair leaves that rule
without a program, so `c36_cispare_programs` names `cispare=1` to keep `cispare_never` / `cispare_flush_anyway` detectable, and `cispare=3` for the new path). Side traces with entry room checks (R2) keep the plain
check. No Lua code runs and nothing is collected inside the helper (TR1). Mutants `rootci_never`, `rootci_short` (one node too few: `exit.rs` asserts the spare node when an inlined frame is rebuilt). `tr_asm`
envs updated, `tr_policy::c36_policy` covers the new default and values.

### 22.3 The other session-h items

* `lib/gsub_fn`, `lib/tonumber_bad`, `corpus2/dev/diff_levenshtein` (5.4 plain, +16..26 % cycles, instructions equal): not reproduced in instructions (HEAD / 1a5aad0 at one path: 1.002, 0.99, 0.988); 5.4 PGO is
  unchanged, so layout / alignment of the plain build, as the report guessed. Not investigated further (cycles at equal instructions).
* `lib/` instruction growth of +1.5..3.7 % (`tinsert_idx` / `_front` / `_append`, `tsort_int`, `tostring_int`, `fmt_d`, `concat_op`, `tonumber_int`, `dijkstra_routes`, `real_dis_mips`), bisected on 5.4 (instructions are exact):
  - **b4f00a9 (H4, cheaper free of dead tables) is the first step** (`tostring_int` 597 -> 611.6 M, `fmt_d` 410.1 -> 418.3 M, `tinsert_idx` 1196.3 -> 1208.9 M, `concat_op` 570.7 -> 581.9 M; 15570aa adds nothing): the
    allocator is an enum (`AllocImpl::Pool | Dyn`) matched on every allocation / free, and both sweep loops test `tt == LUA_VTABLE && watch.is_empty()` for every dead object, which strings do not use. It is the price of
    `binary_trees` -4.7 % at that commit (the program is 20231 -> 15582 M on 5.4 now); not changed.
  - **f1a90fe (H5) is the second step, only for table-insertion programs** (`tinsert_idx` +26 M, `tinsert_append` +25 M, `tremove_front`): the `#[inline]` wrapper `TableRef::insert_absent_shortstr` makes a second inlined copy of
    `insertkey`, and the inlining of `newkey` / `resize` / `setnodevector` changes for every table insertion (the `perf record` profile moves `setnodevector` out of `resize`; JIT off is identical, 5.5 and 5.4 alike).
    `#[inline(never)]` on the wrapper cures it (`tinsert_idx` 1235 -> 1209 M on 5.4, 1042 -> 1014 M on 5.5, `tinsert_append` -2.0 %, `tsort_int` -1.5 % on 5.5, nothing else above 1 % over the 223 programs) **but costs
    `awfy/cd 20` +1.6 % on 5.5 (10815 -> 10989 M), above the 10871 M ceiling this task was given (H5's own -5.5 %)**, so it was not committed. `#[inline]` on `setnodevector` does nothing.
  - `real_dis_mips` / `dijkstra_routes` follow the recording moment (`dijkstra_routes` -3.7 % on 5.4 and -5.9 % on 5.5 with 22.2): formation noise, not a rule.

### 22.4 Results (instructions M, `RUSTYNAIL_SEED=0`, one run, one fixed script path; cycles in brackets, +-3 %)

| program | 5.4: 1a5aad0 | 5.4: 61cee86 | 5.4: K4 | 5.5: 1a5aad0 | 5.5: 61cee86 | 5.5: K4 |
|---|---|---|---|---|---|---|
| apps/expr_lang | 637 (157) | 764 (226) | 630 (162) | 617 (154) | 558 (135) | 565 (141) |
| apps/raytrace (2 rounds) | 526 (154) | 536 (159) | 375 (110) | 488 (142) | 442 (132) | 360 (103) |
| apps/raytrace 40 | 8077 | 7426 | 5546 | 7702 | 5855 | 5325 |
| awfy/cd 20 | 13258 | 12215 | 12189 | 11852 | 10815 | 10815 |
| binary_trees 17 | 20231 | 15582 | 15582 | 19484 | 14419 | 14419 |
| programs/nbody, awfy/nbody | | | | 2028, 557 | 2020, 554 | 2020, 554 |

All 223 programs, instructions K4 / 61cee86: 5.4 geomean 0.9977 (raytrace 0.70, expr_lang 0.83, dijkstra_routes 0.96, `regex_nfa` 1.13 = the bimodal formation above, nothing else beyond +-1 %); 5.5 geomean 0.9983
(raytrace 0.81, dijkstra_routes 0.94, testrunner_luaunit 0.95, particles_physics 0.96, astar_jumper 0.96, `expr_lang` 1.012: one more installed trace after a different recording moment, 565 against 558 M,
`real_dis_mips` 1.011). Registry 723 -> 726.


## 23. K5: the 5.4 identity cell, four mutant survivors

**The identity failure (5.4 only, `r1_live_flip [on2 budget=200]`, 168 instructions short; reduced to a 4-field row program with a type change of one field: any budget near 200 gave 84..240).**
Root cause: `mod::entry_checks` moves every checked `SLOAD` of the iteration behind `POLL` / `BUDGET` and gives it the entry snapshot (`refund = n`, valid after the charge). A side trace
entered with carried values (R1 half A) *starts* with its carried loads (unchecked, no code), so the "leading POLL / BUDGET" prefix was empty: the checks were moved in front of the
`BUDGET` charge, and a failing check there left with `refund = n` for a charge that was never made: the budget word grew by `n`, the stay counted `n` too few instructions. The same pass
also built the entry snapshot as an empty map, so for a carry-entered trace the check's exit restored none of the carried slots (the stack is stale for them). It showed on 5.4 only because
only there the program's recordings produced the shape (a chain of side traces started at the budget exits of a carried side trace, with checked loads that are not in the parent's exit
map); `regcarry=0` / `=3` (no half A) hid it. Fix: the prefix is `POLL`, `BUDGET` and the carried loads; a freshly created entry snapshot of a trace with `carry_in` copies the entries of
snapshot 0 (the carried loads). Detector: `tr_core::r1_entry_check_budget` (the count identity of `r1_entry_budget` at eight budgets, two modes, shipped limits and all carry knobs, plus a
lockstep / exit-storm comparison); mutant `entry_checks_before_budget` restores both halves of the old behaviour (each half alone is invisible: the over-refund needs the bare snapshot, whose
exit is an entry check of the free-peel rule, and the bare snapshot alone changes nothing a program observes: no program was found in which an entry check of a carry-entered trace fails
while the slots it would restore matter, so that half rests on the argument above). Registry 726 -> 727. `instruction_count_identity` passes on 5.4 and 5.5.

**The four survivors (all four were detector holes, none hid a bug).** `lazyci` is on by default (`LAZYCI_PROCESS = 1`): every loop-free recursion whose `CALL`s want the same result
count pushes lazy frames, so the *eager* native push (`rec_call_tail`: `CallInfo.top`, `callstatus`) and the eager native return guards (`CIST_FRESH` / `CIST_C` tests of `rec_ret_guards`)
were not generated by the programs the detectors ran, and the detectors had no job with `lazyci=0`. `rec_push_top_off`, `rec_push_callstatus_off`: jobs `pm3_rec`, `bm_rec_top`, `r2_rec` with
`recnat=1,lazyci=0` in `pm3_programs_run_on_traces`; `rec_ret_cframe_skip`: a `recnat=1,recany=1,lazyci=0` limits row in `r2_recursion_programs`. `b2hk_lea_off`: the unfused `HREFK` address
(`lea d, [node + idx * 24]`) is only generated with `hkaddr=0` (the default folds it into the `HLOAD` / `HSTORE`): two `hkaddr=0` jobs in `b2_programs`. Each is caught on 5.5 now, the
unmutated programs pass. `scripts/gate-known.txt` has no `test54` and no `mutant` entry any more.

## 24. K6: leaf samples of a self-recursive function (`fnself`, 2026-11-20, base 75e7316)

**Study (awfy/list 3000, `RUSTYNAIL_JIT_VERBOSE=2`, 5.5, `RUSTYNAIL_SEED=0`).** `tail` (proto line 31) is `local function tail(x, y, z) if isShorterThan(y, x) then return tail(tail(..), tail(..), tail(..)) else return z end end`:
a tree recursion in which most invocations are leaves (`return z`). Its entry head (pc 0) records `rec_only` (nothing but a native self-call; `fn_end` of a path that returns before a self-call aborts
`FnShort`). Recordings that do start at the entry sample one *invocation* each, and a leaf is the likely one: all 6 entry recordings of the run were leaf samples (`FnShort` waits 70, 148, 265, 514, 275, then
`MAX_FN_SHORT = 4` blacklists the head; `fnpromo` promotes it once more but `closure_self_ref` keeps it a `rec_only` root, so the same thing happens). `proto_recursive` (set only when a recording ends in the native self-call)
therefore never became true and the 4 continuation heads (pc 4, 12, 17, 22) were refused by `driver_start` 40 times with growing waits until blacklisted. (Section 20 finding 1 said the same.)
Everything else of the policy is fine: when an entry recording does see a non-leaf invocation, `end_rec_call` marks the prototype recursive, pc 4 and 12 record (and install), pc 17 and 22 abort `TailCall`
(blacklisted after two; section 20: they cost more when stitched).

**Change.** `TrLimits::fnself` (process default 16, `RUSTYNAIL_JIT_TRACE_LIMITS=fnself=N`, 0 = old behaviour): `driver_start` marks a prototype whose closure reaches itself through an upvalue
(`closure_self_ref`, syntactic recursion of `local function f ... f() ... end`) as `selfref`; for such a prototype, while it is not yet recursive, the 5th and later `FnShort` samples of its entry head no
longer blacklist: the head retries after a flat wait of `16 + 3 * kl + rand4` entries until `fnself` samples were taken (the first four samples wait exactly as before, so every program that does not
get past four leaf samples behaves identically; a first variant with a flat wait from the first sample moved `programs/json` by +5.8 % through a different recording phase of an unrelated loop head). A
self-referential function that never recurses pays at most 12 more cheap recordings (each is a leaf-sample abort: no trace, no install).

**Soundness argument.** No recorder, assembler or runtime path changes: the rule only changes how often and when an entry head is armed, i.e. when the same recorder (with its guards, snapshots and
lockstep) runs; a head that is armed more often is a head that records more. The `selfref` flag is only a hint for the penalty (a wrong flag costs recordings, not correctness). Mutants:
`fnself_never` (no extended budget), `fnself_unbounded` (no blacklist at the limit), `fnself_flag_ignored` (rule for every prototype), `fnself_recursive_ignored` (rule after the prototype is
recursive) are caught by `tr_policy::k6_policy`; `fnself_mark_skip` (driver_start never marks) by `tr_core::k6_selfrec_programs` (list-like program, default thresholds: 8 installs against 5 with
`fnself=0`). Twins: `tests/vm/k6_01..05` (leaf-heavy trees, path switching, mutual recursion, a callee replaced mid-run, tail calls, varargs, multiple results, deep recursion, errors, coroutines) in every
trace mode, `+verify`, exit storms, budgets, GC stress 2, emulator, with `fnself=0 / 16 / 200`.

**Result** (release `rnlua`, `perf stat` instructions:u, `RUSTYNAIL_SEED=0`, CPU 4; M instructions, base 75e7316 -> K6; env length is equal in each pair):

| program | 5.5 base | 5.5 K6 | 5.4 base (`fnself=0`) | 5.4 K6 |
|---|---|---|---|---|
| awfy/list 7000 | 12637 | 10180 .. 10457 (-17..-19 %) | 13345 | 9827 (-26 %) |

The 5.5 spread of the new column is the path / allocation lottery (10180 with `RUSTYNAIL_JIT_TRACE_LIMITS=fnself=16` set in the environment, 10457 with no variable; cycles 2352 -> 2020..2060 M).
All other programs of the check set (richards 500, deltablue 400, towers 900, queens, permute, havlak, cd 20, awfy/nbody, awfy/json, bounce, mandelbrot, fib, fannkuch, spectral_norm, matmul, nbody, raytrace 40,
binary_trees 17, lru_cache, game_loop, field_method, oop_methods, expr_lang, roguelike_turns, programs/json) are within 0.02 % instructions on both versions (raytrace 5.4: +0.02 %; every other row identical).
Remaining in `list` after K6: `tail`'s root `TAILCALL` heads (pc 17, 22) still abort `TailCall` (section 20 measured that stitching them is worse); the entry trace and the pc 4 / 12 continuations now run.

## 25. `t15_core_emu::intrinsic_calls_match_tier1_at_every_pc` in release test builds (2026-11-20, base fc520a0)

Finding (docs/GATES.md "Debug or release"): the test failed in the `--release` gate profile (`CARGO_PROFILE_RELEASE_LTO=off`, 16 codegen units; `in_sin_flt`: slot 11 of the
region frame holds the callee, tier 1 the result) and passed with debug assertions. It is neither of the two guesses: no budget check and no shipped-code difference.

Root cause: the test's helper model (`Model::try_math`) recognises the libm helper by address, `f == rn_jit::t15::codegen::jit_t15_math as usize`, and answers it with the
real result. In that build the binary holds TWO copies of `jit_t15_math` (`nm`: two symbols, 0xc4720 and 0x18d330): the one baked into the generated code by
`calls.rs` (`Op::CallH { f: jit_t15_math ... }`) and the one the test crate takes the address of. The body has no calls once the MIR inliner has run, so rustc's automatic
cross-crate-inline heuristic (opt-level >= 1 without debug assertions: the overflow panics of the debug build are calls, so it does not apply there) marks it
cross-crate-inlinable, and without LTO / with several CGUs every user gets a local copy at its own address. The region's call then reached the model as an unknown helper
(`call 1 Other -> 0`, nothing written to the result slot), and the frame differed at the next event. Reproduced only with `--release` + `lto=off` + `codegen-units=16`
(plain `cargo test --release` with fat LTO / 1 CGU, and every debug-assertion build, pass). The shipped binary is fat LTO with one CGU: one copy; no behaviour of `rnlua` differs.

Fix (test-visible only, no production behaviour change): `#[inline(never)]` on `jit_t15_math` and on `jit_t15_verify_fail` (the other helper compared by address in the tests),
so the address is unique in every profile. The detector is the existing test itself, which fails before / passes after in the release gate profile and passes in debug (it
cannot fail in the debug profile before the fix: that profile never duplicated the function). No test or mutant was added or moved (registry unchanged, 732).
Note for later: any helper whose address a test compares must stay out of the cross-crate-inlinable set; the Lua C functions that the intrinsic table matches by address
(`rn_std::lib_math::jit_builtins` vs `open`) live in one crate and are large, so they are unaffected.

## 26. K1 / K2: mixed `math.min` / `math.max`, `table.remove(t, pos)`, method recursion in the `FnShort` rule (2026-11-20, base d8ab9f3)

Handles K1 and K2 of docs/STUDY_COVERAGE.md. Registry 732 -> 740 (six K1 mutants, two K2 mutants).

### K1a. `math.min` / `math.max` of an integer and a float

`lib_intrinsic` (the `min`/`max` arm, `record.rs`) refused any pair of different types (`Nyi(Call)`, which killed the side recording of the whole lights loop of raytrace).
The mixed pair now runs through `Rec::compare` (the compare of the `LT` bytecode): `max` takes `b` iff `a < b`, `min` iff `b < a`, exactly the library's
`lua_compare(LT)` loop, and the result is the ORIGINAL argument of the winning side (`lua_pushvalue`), so its type is the type of the branch the recording took
(`max(0, -1.5)` is the integer 0, `max(0, 2.5)` the float 2.5, `max(2, 2.0)` the integer 2: ties keep the first). The direction is a guard (`compare` emits the integer
bound guards of the conversion, which fold for constants, and an `FCMP` guard with the observed outcome and NaN as "false"); the other direction is a side exit that gets
its own trace. An integer outside +-2^53 stays unrecorded (`compare`'s `Nyi`: no float conversion is exact, so no guard can express the compare). 3-argument and later
arguments fold pairwise as before; same-type pairs keep their old code (`Fmin`/`Fmax`, integer `LT`/`GE` guard): the instruction counts of every program without a mixed pair
are unchanged (below). Mutants: `k1_mix_take_a` (the guard is recorded, the result is always the accumulator), `k1_mix_le` (`<=`: `max(2, 2.0)` returns the float).

### K1b. `table.remove(t, pos)`

`lib_tremove` handled `remove(t)` only. The two-argument form for a table without metatable, an integer `pos` and `1 <= pos <= #t` (`#t` the only border,
`lib_border_exact`):

* `pos == #t` is the pop (as `tremove` itself: `pos != size` is not validated): guard `pos == TLEN`, then the old `res = t[n]; t[n] = nil`;
* `pos < #t` with at most 16 elements to shift (`MAX_REMOVE`): `#t` and `pos` guarded to the values seen (`pos` folds when it is a constant: `remove(todo, 1)`), the
  elements `t[pos .. n]` read first (`ALOAD` type guards), the bounds / metatable guard of every destination `array_guards_raw` BEFORE the first store (an exit between two
  stores would make the interpreter re-run the call over a half-shifted table: the `table.move` rule of section "table.move over one table"), then
  `t[q] = t[q + 1]` for `q = pos .. n-1` (with the write barrier for collectable values) and `t[n] = nil`; the result is the element read at `pos`;
* everything else (a position that is not an integer or nil, out of range = an error, `pos == #t + 1`, an empty table, a metatable, several borders, a longer shift) is
  now a plain C call as for `table.move` (`lib_unrecorded`: stitch, or `CFunc` in a root); it was `Nyi(Call)` before. `remove(t)` is unchanged.

The model elements `t[pos .. n]` are copied into the lockstep model at record time (the lazy copy would read after the VM removed them). Mutants:
`k1_trm_len_guard_skip`, `k1_trm_pos_guard_skip`, `k1_trm_shift_off` (shifts from `q + 2`), `k1_trm_last_keep` (the last slot is not cleared).

**deltablue is not helped.** The study expected deltablue -1..2 % from `makePlan`. With the shift recorded (checked with `MAX_REMOVE` 1000: `todo` is longer than 16 in the
benchmark) the head `makePlan` pc 11 moves from `Nyi(Call)` to `InnerLoop@L303:5` (the loop of `addConstraintsConsumingTo`, class C3), so it stays blacklisted; with the limit 16 the abort is
`CFunc` (the stitched call at the head) instead of `Nyi(Call)`. Instruction counts and cycles of deltablue are unchanged. The recorded form pays for itself only where the
shifted tables are short and the region has no inner loop (none in the benchmark set).

### K2. Method recursion marks the prototype `selfref`

K6's `selfref` (extended `FnShort` budget of the entry head: up to `fnself` = 16 leaf samples with a short flat wait instead of 4) was set only for a closure that reaches itself through an upvalue
(`local function f .. f() ..`). `permute`'s `self:permute(n1)` and `awfy/list`-style `self:tail(..)` recurse through a table, so their entry heads died after 4-6
leaf samples (`permute_nc`: `FnShort:6` at `L14:23 Return0`, head `Black`). New knob `fnrec` (default 4 = [`FNREC_PROCESS`], `RUSTYNAIL_JIT_TRACE_LIMITS=fnrec=N`, 0 = old): `driver_start`
marks the prototype `selfref` also when an ACTIVATION OF THE SAME PROTOTYPE is among the `fnrec` nearest caller frames (`recursion_on_stack`: walks `Ci::previous`, a C frame
counts as a frame; a function called from another prototype, richards' tail-called task bodies, deltablue's methods, never match). It is the same policy hint as K6's flag (a wrong one costs
cheap leaf-sample recordings, not correctness); no recorder, assembler or runtime path changes. The rule is narrower than `fnmin=0` (which lets every short recording install) and
leaves `fnmin` alone. Mutants: `fnrec_mark_skip` (never marks), `fnrec_proto_ignored` (marks whenever any Lua frame is below).

### Measured (release `rnlua`, `perf stat` instructions:u / cycles:u, CPU 4, `RUSTYNAIL_SEED=0`; instructions of one run, cycles min of 5 (5.4: 3); binaries copied to one path; the three columns share the same environment length (`RUSTYNAIL_JIT_TRACE_LIMITS=fnrec=0|4`; the base binary ignores the key))

| program (args) | base d8ab9f3 instr M / cyc M | K1+K2, `fnrec=0` | K1+K2, `fnrec=4` | effect |
|---|---|---|---|---|
| apps/raytrace 40 | 5328 / 1312 | 4630 / 1185 | 4630 / 1170 | **instr -13.1 %, cycles -9.7 % (K1a)**; the `Nyi(Call)@record.rs:7755` aborts of `trace` pc 17/50/53/56 and pc 84 are gone |
| awfy/permute without the `count` store (`permute_nc 3000`) | 20258 / 3318 | 20258 / 3293 | **6775 / 1129** | **instr -66.6 %, cycles -66 % (2.9x, K2)**; head `L14 pc 0` `Black` -> `Traced` |
| list with `self:tail` (method form of awfy/list, 4000 iterations) | 7417 / 1523 | (same as base) | 5867 / 1228 | instr -21 %, cycles -19 % (K2) |
| awfy/permute 3000 | 7036 / 1174 | 7036 / 1182 | 7036 / 1174 | 0 (identical instructions) |
| awfy/deltablue 400 | 4526 / 1244 | 4526 / 1240 | 4526 / 1211 | 0 (instructions identical; cycle spread is noise) |
| awfy/richards 500 | 2830 / 798 | 2830 / 795 | 2830 / 794 | 0 |
| awfy/cd 20 | 10819 / 2926 | 10819 / 2928 | 10819 / 2913 | 0 |
| awfy/list 7000 | 10180 / 1996 | 10180 / 2006 | 10180 / 2006 | 0 |
| awfy/havlak 20 | 1425 / 459 | 1424 / 425 | 1425 / 424 | 0 (instructions) |
| programs/nbody 1000000 | 2019 / 345 | 2019 / 345 | 2019 / 344 | 0 |
| programs/fib 32 | 703 / 114 | 703 / 113 | 703 / 114 | 0 |
| programs/binary_trees 14 | 1563 / 311 | 1563 / 304 | 1563 / 306 | 0 |

5.4 (same method, 3 cycle reps): raytrace 5553 / 1407 -> 4541 / 1225 (-18 % instr, -13 % cycles); permute_nc 20752 / 3395 -> 6974 / 1191 (-66 % / -65 %); deltablue, richards, cd, list,
havlak, nbody, fib, binary_trees identical instructions (havlak cycles 612 -> 694 in the first pass is noise: five-rep rerun 541 / 537 with the rule against 540 / 539 without it).
No awfy program changes with `fnrec`: their recursive functions are upvalue recursions (list `tail`, cd `put`), already covered by K6; the rule is a protection for method-recursive
programs (the cliff of the study: `permute` without its `count` store, `self:tail`). K2 cost: nothing measurable (a short walk of the call-info list at recording start only).

### Tests

* `tr_core::k1_programs` (programs `k1_mixed_minmax`, `k1_tremove`: every trace mode, `+verify`, exit storms, budgets, `-loop`, `-fwd -cse -loop -sink`, emulator, GC stress 1/2 in both
  collector modes, tier 1): ties, NaN, signed zeros, 2^53 and beyond, three arguments, results used in integer arithmetic; queue pops, middle removals, constant and loop
  positions, long shifts, bounds errors, a float position, element kinds with barriers, a metatable installed mid-loop. Detector of the six K1 mutants.
* `tr_core::k2_methrec_programs` (`tests/vm/k2_01..04`: method recursion with leaf-heavy trees, subclass overrides, a replaced method, errors, coroutines, mutual / vararg / field recursion,
  a list-like tail-calling method, non-recursive leaf methods; every mode with `fnrec=0` and `fnrec=4`, plus the process start gate `wgate=30000,sidemin=4`): same output as the interpreter;
  the list-like program takes another formation with the rule (`on2`, 5.5), the non-recursive program the same number of `FnShort` samples. Detector of `fnrec_mark_skip`, `fnrec_proto_ignored`.
* `tr_policy::k2_policy`: default, parse and cap of `fnrec`.
* `tests/vm/k2_*` run in the plain vm corpus as well.

### Heavy gate (2026-11-20)

`gate-heavy.sh`: light, mutants (all 740 caught), tests-5.5-oracle (KNOWN `tr_identity_pre::fetch_plus_insns_identity` only), tests-5.4-oracle, jitdiff 1-200 (7 modes): PASS. `oracle_options` (3822 jobs): one new
unknown failure, `core/t1i_inline [always +exitstorm=7 budget=3]` ("stay end" checkpoints at the tail of the run: innermost prototype differs; the instruction-count identity `tr_core::instruction_count_identity`
holds). Not a K1 / K2 defect: with the mixed `min`/`max` path switched off the full program passes, but the job also fails (same signature) on the BASE commit d8ab9f3 for a reduced program (`t1i_inline` without its
`run2("max"|"min"|..., f_max|f_min, ...)` lines, which contain no change K1 can see), and a family of other reductions (constant arguments, integer-only lists) flips between pass and fail: a formation lottery of the
exit-storm job in the swap / hook tail of the program. K1 shifts the lottery; the job is listed as KNOWN in `scripts/gate-known.txt` (oracle class) with this note. Not investigated further (it needs the oracle's
checkpoint model for tier-1 stays under `budget=3`).

## 27. K3 / K4: side traces for loop exits inside inlined frames (2026-11-20, base b8aeacc)

Handles K3 and K4 of docs/STUDY_COVERAGE.md (classes C5 / C3). K3 is shipped (two small rules, `awfy/list` -21 % cycles / -27 % instructions); K4 was prototyped in two forms and **not shipped**
(no gain on its targets, one form a regression); numbers below.

### 27.1 What was wrong (list)

`tail`'s function-entry root trace holds an unrolled copy of `isShorterThan`'s loop with five frame exits (the loop test and the two `nxt` loads of the last unrolled iterations, `S14-S18`).
Every exit is hot (32 M exits in `list 7000`) and every side recording from them died `SideShort` at `L21:10 Jmp`. Two independent causes:

1. The side recording runs `GETFIELD`, `JMP` (back edge) and reaches `isShorterThan`'s loop head `pc 2`, which has a root trace (`sidenest`: the frame-linking exit). That path
   is 2-3 bytecodes, below `sidemin` (4): `SideShort`. For a side that starts inside inlined frames the alternative to the short side is not "a few bytecodes in tier 1" but the whole rest of the loop in
   tier 1 (the exit materialises the frames and tier 1 runs on), so the `sidemin` rule does not describe its cost.
2. The recordings that get past that end at `tail`'s root `RETURN` after about 6 bytecodes: `end_exit` with frames refuses it (`framemin` = 8, H1: a side trace that starts inside frames and ends at a
   stitching exit with fewer bytecodes is `SideShort`; it exists to keep `expr_lang` from compiling 24 extra short side traces).

### 27.2 K3a: a short path at a callee loop head goes on (`Rec::frame_link_short`)

In `on_step` the `frame_head` arm (a side recording inside an inlined frame reaching a loop head with a root trace) used to abort `SideShort` when fewer than `sidemin` bytecodes were recorded.
Now, when that is the case and the frame's loop budget is not spent (`loopunroll`, `inl_back`), the arm is skipped and the recording continues exactly as at a loop head **without** a root trace
(the callee's loop is unrolled per recording, the existing P-E / M1 path: `target` counts the back edge, `InnerLoop` beyond `loopunroll`). No new recording machinery: only the case that used to abort is
routed into the existing one, so snapshots, regcarry cells, sunk tables and `lazy_root` behave as for every unrolled side recording. Mutant `frame_short_aborts` (restores the abort).
Alone this gives `list` 2036 -> 1800 Mcyc.

### 27.3 K3b: the retry is not held to `framemin` (`fretry`)

`framemin` keeps its job for the first attempt. An exit whose previous attempt ended `SideShort` (`t.last[k]`; it stayed hot: the exit counter was reset and reached the threshold again, `fck x hot_exit` exits)
gets its next attempt with `frame_min = 0` (`SideStart::fretry`, `cache::fretry_due`; limit `fretry`, default 1, process and struct). With `fck=64` an exit is attempted only after about
1000 hot exits, which is the evidence that the short path pays; the `expr_lang` cost that `framemin` protects is paid once per exit only after that. Mutants `fretry_never` (no exemption) and
`fretry_always` (first attempts exempt too, i.e. `framemin=0`). Together with K3a: `list` 2036 -> 1598 Mcyc (`framemin=0` and `sidemin=0` give the same 1606 / 1606: nothing is lost by the retry gating).
`sidenest=0,sidemin=0,framemin=0` (the side recordings unroll instead of linking) is also 1629: the link into the callee's root trace was never the gain, the unrolled continuation is.

### 27.4 K4: not shipped

K4 asked for ending a side recording at a traced loop head of the same frame instead of aborting `InnerLoop` / `TooLong[sideins]`. Two prototypes, both measured and reverted:

* **`preplink`**: a side recording at a `FORPREP` of an inlined frame with data-dependent bounds records the prep (guards, loop registers in the exit snapshot) and the next step links to the root trace
  of the loop head behind it (the `InnerLoop` abort of `raytrace` loop `pc 84`, `L63:10 Forprep`, 4 -> 0). Sound by construction (it reuses `close_frame_link`; the recorded dynamic count never reaches a
  `FORLOOP`: a non-link next step aborts `InnerLoop` as before) but **no effect**: raytrace 40 1181 -> 1183 Mcyc (instructions -0.1 %), deltablue 1212 -> 1206, cd 2864 -> 2844 (K3 alone), i.e. noise.
  The aborting sites are not where the time goes: after `preplink` the same loop's sides end `SideShort` at `L63:114 Forloop` (a 3-bytecode path into the lights loop head, below `sidemin`; `sidemin=0` does
  not move cycles either: 1181 / 1169). `nest=2` (root recordings frame-link too, `render` / `Constraint:satisfy` `InnerLoop` at `Forprep`) also gives nothing: deltablue 1217 / 1212, raytrace 1188 / 1171
  against 1206 / 1181.
* **`sidecut`**: a side recording that exceeds `MAX_SIDE_INS` after 24 steps ends with a stitching exit before the instruction (the continuation is a side trace of that exit: a chain) instead of
  `TooLong[sideins]`. Raytrace's loop `pc 11` exit has about 2000 IR of inlined `vec` / `V:add` constructors before the next loop head. Result: raytrace 100 2842 -> 2923 Mcyc (+2.8 %), instructions -1 %;
  the family hits `maxside` (22 `MaxSide` side aborts in `raytrace 40`) and every cut materialises the live sunk tables and frames at the stitching exit. Cycles worse: not shipped. Raising `MAX_SIDE_INS`
  stays as the study found (worse).

So the raytrace / deltablue estimates of K4 (-10..15 % / -5..8 %) are not reachable by these rules; what is left there is C1 (headless callees behind tier-1 callers: `vec` 14.5 %, `V:add` 9.7 %,
table constructors) and the side-aborts that are not exit-path problems (`TooLong` from the size of the inlined constructors).

### 27.5 Measurements

Release `rn-cli` (JIT on), `RUSTYNAIL_SEED=0`, `taskset -c 4`, `perf stat cycles:u,instructions:u`, min of 5 serial runs, both binaries copied to the same path before every run; base b8aeacc against this branch.

| program | Mcycles base -> K3 | Minstr base -> K3 |
|---|---:|---:|
| awfy/list 7000 | 1993 -> 1573 (**-21.1 %**) | 10249 -> 7438 (**-27.4 %**) |
| awfy/cd 20 | 2888 -> 2849 (-1.4 %) | 10767 -> 10717 (-0.5 %) |
| awfy/deltablue 400 | 1218 -> 1223 (+0.4 %) | 4516 -> 4516 |
| awfy/richards 500 | 803 -> 812 (+1.1 %) | 2828 -> 2828 |
| awfy/permute 3000 | 1162 -> 1163 | 7035 -> 7035 |
| awfy/havlak | 158 -> 157 | 510 -> 510 |
| apps/raytrace 40 / default | 1198 -> 1180 / 116 -> 116 | 4619 -> 4618 / 380 -> 380 |
| apps/expr_lang | 136 -> 137 (+0.7 %) | 560 -> 561 (+0.2 %) |
| nbody 1000000, fib 32, binary_trees 14 | 343 -> 342, 111 -> 110, 303 -> 302 | 2160, 703, 1547: identical |
| bounce, json, queens, sieve, storage, towers, mandelbrot (awfy) | +-0.0 | identical (json -0.4 %) |
| apps/events, lru_cache, markdown | +0.6, +0.7, -1.7 % (noise: identical instructions) | identical |
| programs/fannkuch, heapsort, oop_methods, spectral_norm, metamethods, closures, string_ops, matmul | -0.3 .. +1.7 % (noise) | identical |

Deltablue and richards are unchanged in instructions: their `LeaveFrame` side aborts (`Plan:execute` `L216:9 Return0`: the loop exits into the function's `RETURN`) are exits of the *root* frame, not
frame exits, and K3 does not touch them (the `retf` machinery owns that path).

### 27.6 Tests, mutants

* `tr_core::k3_frame_loop_programs` (program `tests/vm/k3_01_frame_loop_exits.lua`: the `list` shape plus a callee `while` loop with several early returns called from a numeric `for`): same output as the
  interpreter in `always` / `on2` with `+verify`, exit storms, budget 17, GC stress 2, emulator, with `fretry=1/0`, `framemin=8/0`, `sidemin=4/0`. Assertions: the retry installs more side traces than `fretry=0`
  (`fretry_never`), records more than `framemin=0` (`fretry_always`), and `sidemin=4` installs about as many side traces as `sidemin=0` in `always` (`frame_short_aborts`).
* Mutants: 740 -> **743** (`frame_short_aborts`, `fretry_never`, `fretry_always`; registry count in `mutants.rs`, rows in `tests/tr_mutants.rs`, `scripts/gate-mutant-map.tsv` regenerated). All three are caught by
  the detector above (checked one by one with `RUSTYNAIL_JIT_MUT`).
* Oracle: every program of `PROGS` is an oracle job, so `k3_01_frame_loop_exits` is covered by `oracle_options` in every cell.
* New limit `fretry` (default 1) parsed in `TrLimits::parse`.
* Two existing tests pin `fretry=0` because they measure rules the retry would blur: `h1_frame_side_thresholds` (an unreachable `framemin` must record fewer side traces: with the retry the
  second attempt ignores it) and `sinkroot_programs` (formation-sensitive count of blacklisted heads). `k3_frame_loop_programs` asserts the `fretry_always` difference in `always` only (5.4 `on2`
  records the same number of side traces in both runs).

### 27.7 Heavy gate (2026-11-21, commit ba6e311)

`gate-heavy.sh`: light (5.5 and 5.4 trace suites, vm_corpus, clippy, mutant-map) PASS; mutants PASS (all caught, registry 743); tests-5.5-oracle KNOWN (`tr_identity_pre::fetch_plus_insns_identity` only), tests-5.4-oracle
PASS; jitdiff 1-200 in the seven modes PASS (0 diffs); `oracle_options` KNOWN: the only mismatching jobs are `core/t1i_inline [always +exitstorm=7 budget=3]` and `core/bb_tset [always +membudget budget=17]`, both listed in
`scripts/gate-known.txt` (no new oracle failure; the new program `k3_01_frame_loop_exits` passes every cell).

## 28. K5: store-over-live-slot guard elision (`slotlive`; 2026-11-21, base daf82ae)

**What the study assumed and what the IR shows.** The handle K5 expected the 12-op method-lookup chain and `self.v` to be redone for every swap
*inside one trace*. They are not: the permute traces are cut at every call (function-entry root, loop continuation, the continuation after the first
self-call), each holds ONE swap and ONE `self:permute` lookup, and inside a trace the optimiser already shares everything (`HREFK` / `FLOAD node` of
the second method name reuse the first chain, `self.v` is loaded once, `asize` / array base once). The chain is repeated *across* traces (each trace
entry re-validates `self` -> metatable -> `__index` -> method), which needs facts that survive a trace transition (a version stamp on metatables or a
carried-in-registers guard set): not an IR dedupe, not built. Measured on the traces of `permute 3000` (`RUSTYNAIL_JIT_DUMP` + `perf record -e cycles:u`
with the IP histogram mapped on the dumped code): 6.1 IPC (7036 M instr / 1160 M cycles), so instructions are nearly free; the samples sit on the
dependent loads of the frame protocol (`[ci]` reloads after the frame stores: 15 % of the cycles in the entry sequences) and on the chain loads (about
8 %). Cutting instructions therefore moves permute's cycles by a fraction of its instruction share: `-16..23 %` is not reachable by deduping.

**What shipped (`TrLimits::slotlive`, default 1, `slotlive=0` = before).** A store into a table slot that the recording already proved non-nil needs
neither the `__newindex` guard (hash: `FLOAD metatable` + flag-cache bit, F5; array: `FLOAD metatable == 0` or the old-value `ALOAD` guard) nor an
old-value check: `__newindex` applies only to an absent key / nil old value, and the interpreter's fast path stores the same way. `Rec::slot_live`
(`record.rs`) decides, at recording time, from the IR built so far: the nearest earlier access of the *same location* must be a typed (`F_GUARD`,
bit 0) `HLOAD` / `ALOAD` of a non-nil type or a store of a non-nil value; between it and the store only pure ops, guards, loads, `TBAR`, `USTORE`,
`ATOUCH`, `POLL` / `BUDGET` and stores may lie, and a store of nil (or of a value of unknown type) only when it provably cannot alias (another constant
key, another constant / `x + c` array index, the other memory class: `opt_mem::alias`). Anything else (calls, `NEWREF`, `TSET`, `LIB`, allocation, ...)
ends the search with "unknown" (guards kept). Locations are compared by value, not by ref, because the recording emits one `HREFK` per access (each with
its own snapshot; merged by the optimiser later) and one `HLOAD` per `self.v` (merged by forwarding later): `same_val` treats two `HREFK`s of the same
node vector / key / index, header `FLOAD`s of equal tables, and two typed `HLOAD`s of one slot with no may-alias store between them as equal. Hash facts
are only for `HREFK` (constant keys), never `HREFV`. The elided guard is never CSE-shared away from another store: every store that is not itself live
still emits its own guard, so the removal cannot weaken a later store.

Soundness argument: straight-line trace; the typed load guard (or the non-nil store) establishes "slot non-nil" at a point P; every instruction between P
and the store is classified (cannot write nil / provably another slot / unknown -> not live); exits leave the trace before the store, and loop peeling
re-emits the same pre-roll order, where a body load folds into the pre-roll load or store only under the optimiser's own no-may-alias-store rule, which
is stricter than the one here. The fact is per run-time table, not per recorded table: a store through a *different* table ref that aliases at run time
is a may-alias store of nil (kept by the rule above); tests below pin that.

**Measured (same binary, `slotlive=0` against `=1`, equal path, `RUSTYNAIL_SEED=0`, taskset CPU 4; instructions deterministic, cycles min of 5).**

| program | instr M 0 -> 1 | d instr | cycles M 0 -> 1 | d cycles |
|---|---|---:|---|---:|
| permute 3000 | 7036.2 -> 6863.3 | -2.46 % | 1175.9 -> 1165.9 | -0.85 % |
| fannkuch | 4980.8 -> 4863.2 | -2.36 % | 1062.7 -> 1058.7 (min of 9) | -0.4 % |
| richards | 181.8 -> 180.1 | -0.94 % | 59.4 -> 58.9 | -0.9 % |
| towers | 187.3 -> 185.6 | -0.93 % | 52.2 -> 48.5 | -7.0 % (layout; instr -0.9 %) |
| bounce | 309.3 -> 308.8 | -0.18 % | 60.6 -> 60.0 | -1.0 % |
| json | 465.0 -> 463.5 | -0.33 % | 141.7 -> 140.7 | -0.7 % |
| game_loop | 150.4 -> 149.8 | -0.44 % | 33.8 -> 33.5 | -0.9 % |
| oop_methods | 851.8 -> 851.1 | -0.08 % | 142.7 -> 139.8 | -2.0 % |
| deltablue, cd, havlak, list, raytrace (apps and `raytrace 40`), expr_lang, events, lru_cache, markdown, classic_vector, roguelike_turns, nbody, fib, binary_trees, queens, sieve, heapsort, matmul, storage, lua_interp | within +-0.1 % (one json-sized exception above) | ~0 | all within +-0.7 % except noise (events 160.7 -> 161.6 at 9 runs) | ~0 |

nbody, fib, binary_trees instruction counts are identical (to the 0.01 %); richards moves -0.94 % as listed. So the shipped part is a **sound but
small** gain (instructions -2.5 % on permute / fannkuch, <= 1 % elsewhere; cycles within noise except permute -0.9 %, fannkuch -0.4 %, richards -0.9 %),
not the study's -16..23 %. The OOP programs (deltablue, cd, game_loop, roguelike_turns, classic_vector) have their field updates behind other
accesses (a table loaded through a field, a call in between): no change.

**Measured and not built.** (a) `__index == metatable` identity guard (the class idiom `P.__index = P`): replaces the `FLOAD lsizenode` / `FLOAD node`
/ shape checks of P by one compare, about 2 of ~36 instructions per chain at 6 IPC: < 0.5 % cycles, plus a new guard kind in the exit / loop machinery;
not worth it. (b) Method-lookup facts across trace transitions (the real 13 %): needs metatable version stamps or a guard set carried by `regcarry`
cells; every variant must be re-validated at every call (finalizers, `debug.setmetatable`, hooks); left as a K8-class item. (c) Removing the bound
`ULT` of a second index: different indices need their own check.

**Tests and mutants (746 now, 743 before; frozen count in `mutants.rs`).** Programs `tests/vm/k5_01_slotlive_hash.lua`, `k5_02_slotlive_array.lua`,
`k5_03_slotlive_effect.lua` (class field updates, the permute swap with and without a metatable on the array, nil written through a reference that is
another table at record time and the same table every fifth iteration, through a variable key / index, a table re-loaded after a store changed the
field it comes from, another index of the same array, a metatable installed in the loop); `tr_core::k5_slotlive_programs` runs them in the usual
mode matrix with `slotlive=1` and `0` and asserts that the effect program has fewer metatable / flag `FLOAD`s with the rule. Mutants:
`slotlive_nil_between` (a may-alias nil store ignored), `slotlive_index_off` (array offset ignored), `slotlive_hload_stale` (two `HLOAD`s equal across a
store); all three are caught by `k5_slotlive_programs`. A fourth candidate (barrier ops treated as neutral) was dropped: no recordable op that is a
barrier writes a nil into a slot the recording then still sees as non-nil (`table.remove` refuses tables with a metatable, a store after a nil written
by a library call sees `ev == nil`), so it is an equivalent mutant.

## 29. K7: an `AREF` is never a PHI (dijkstra_routes 5.5 regression); the three address-dependent oracle jobs (2026-11-21, base d2c0958)

### 29.1 `corpus2/heldout/dijkstra_routes` on 5.5 (+13.8 % cycles, +29 % instructions in session-i against session-h)

**Bisect by knob** (release binary of 22670a1, `RUSTYNAIL_JIT_TRACE_LIMITS`, equal path, instructions M): default 4767; `fnrec=0` 4768; `fretry=0` 4768; `slotlive=0` **3727**; all three off 3727.
So K5 (`slotlive`) is the switch, but it is not slotlive's elision that costs: the head table (`RUSTYNAIL_JIT_VERBOSE=1`) shows the sift-down loop of `heap_pop` (`L30 pc 13`, 4.2 M
entries) installed with `slotlive=0` and **blacklisted** with `slotlive=1`, `aborts {Compile(asm-unsupported):2}`. `VERBOSE=2` gives the reason: `asm: Unsupported("an AREF that is not fused into its memory users")`
(the dump of the failing IR is now printed after `[trace] FAILIR` at `VERBOSE=2`).

**Cause.** The loop is `i = s = l` on the recorded path, so the body's `h[i]` is the address the pre-roll computed for `h[l]` (`AREF %11 %9`). The body copy emitted the same `AREF` with the
substituted index operand, CSE found the pre-roll `AREF`, and because the body copy of the pre-roll `AREF` is another instruction (the next iteration's `h[l]`) the loop pass made a
`PHI p64 %12 %57` of the address. An array address is a base and an index (values and tags live in two arrays, the asm fuses it into the `ALOAD` / `ASTORE` that use it); an `AREF` that is a
`PHI` operand is unfused and the assembler refuses it. This is an older latent bug, not caused by `slotlive`: the same program with `slotlive=0` also has one `asm-unsupported` abort at that head
(`rec 2 ... aborts {Compile(asm-unsupported):1}`); the second recording took the `s = r` path (no address reuse) and installed. With `slotlive=1` both recordings took the left path, i.e. the
rule only moved the recording order. (The other `slotlive=1` difference is that there is no old-value `ALOAD` guard in front of the stores; it does not change the CSE of the addresses.)

**Fix (`opt_loop.rs`, `Em::emit` arm for `TOp::Aref` in the body copy).** The body `AREF` is looked up as before (`Builder::find`); if the hit is a pre-roll instruction whose own body copy is
another instruction (`creator[r]` with `sub[creator] != r`) the `AREF` is emitted again (`emit_raw`); its operands are PHIs of integers as usual (`ADD`, supported). Invariant addresses (the body copy
of the pre-roll `AREF` is itself) still merge. Mutant `aref_phi_cse` (the old behaviour). Test `tr_core::k7_aref_phi_programs` (program `tests/vm/k7_01_aref_phi.lua`: heap sift-down with the left child
always smaller, right-child and mixed variants, in every mode of the usual matrix, `aidx=0`; asserts no `Compile(asm-` abort in plain `always`).

**Measured** (release `rnlua`, `perf stat` instructions:u / cycles:u, CPU 4, `RUSTYNAIL_SEED=0`, binaries at paths of equal length, instructions of one run, cycles min of 5; base d2c0958 against base + K7):

| program | base instr M / cyc M | K7 instr M / cyc M | effect |
|---|---|---|---|
| corpus2/heldout/dijkstra_routes | 4767 / 1465 | **3070 / 1193** | instr -35.6 %, cycles -18.6 % (session-h was about 3700 / 1290: now -17 % instr) |
| awfy/list | 396 / 92 | 396 / 92 | 0 |
| awfy/deltablue | 335 / 108 | 335 / 108 | 0 |
| awfy/cd | 805 / 259 | 805 / 258 | 0 |
| awfy/richards | 180 / 58 | 180 / 59 | 0 |
| awfy/permute | 235 / 43 | 235 / 43 | 0 |
| awfy/nbody | 553 / 91 | 553 / 92 | 0 |
| apps/raytrace | 356 / 113 | 356 / 113 | 0 |
| apps/raytrace 40 | 4631 / 1207 | 4631 / 1186 | instr 0, cycles noise |
| lib/tremove_front | 862 / 147 | 862 / 145 | 0 |
| programs/fib | 7780 / 1236 | 7780 / 1242 | 0 |
| programs/binary_trees | 7269 / 1469 | 7269 / 1483 | 0 (cycles +1 %, noise) |
| corpus/astar_jumper (5.5) | 5416 / 1642 | 5415 / 1665 | 0 (instruction count is noisy) |

(Instruction counts identical to the unit: the change only acts when a pre-roll `AREF` would have become a `PHI`, which never compiled.)

**`corpus/astar_jumper` on 5.4: not the same cause.** No `asm-unsupported` abort in its run. `RUSTYNAIL_JIT_TRACE_LIMITS` on the session-i 5.4 binary (instructions M, three runs of the default are
5739-5742): `slotlive=0` 5742, `fnrec=0` 5738, `fretry=0` **5339** (cycles 1765 against 1860-1880, -5.5 %). With `fretry=1` the function-entry root `L158 pc 0` (527 k entries, `LeaveFrame:41` side aborts at
`L158:15 Return0`) and two continuation roots at `L22` form and the root at `L52 pc 0` does not; with `fretry=0` the formation is the one of session-h. So K3b's retry (`fretry`) changes which family forms in this
program (+7 % instructions); it is a formation shift, not an abort bug, and is not fixed here (the K3 gains on `list` need the retry).

### 29.2 The oracle jobs listed KNOWN (`core/t1i_inline [always +exitstorm=7 budget=3]`, `core/bb_tset [... budget=17]`, `tests/vm/table_insert_layout`)

All three are the same inherent nondeterminism: the oracle compares the digest of live locals at equal clocks between the traced run and a JIT-off run of the same program in one process, and **the two
runs lay out the heap differently**, so anything that depends on an object's address differs. Not a model error of the oracle's stay checkpoints and not a JIT discrepancy.

* **t1i_inline** (deterministic failure in the cell, 3 of 3 runs; periodic mismatches every 1240 instructions, `stay end ... pc 3 (trace run) vs 34`): `TR_ORACLE_DEBUG=1 TR_ORACLE_RANGE` shows both runs at
  `(proto 0xd9d2, pc 3)` at the same clock 619793, i.e. the stay position is right; the digest differs in a live **string** slot (`fmt`'s `v`). The first reported reason is that of the first candidate
  fetch in the +-16 window (`pc 3 vs 34`), which hides the real reason (the `fail` of a checkpoint keeps only the first candidate's). The string is `"table: 0x..."`: the section
  `swap_loop("swap_type_print", b_type, ... type = tostring ...)` makes `type({})` return the address text of a table. The sampling of checkpoints (first 3000, then every 16th) decides per cell whether a
  checkpoint falls on such a frame, which is why only `+exitstorm=7 budget=3` failed and the full program "passed by the formation lottery" on other bases. Fix: the variant body no longer has `type({})`
  (`b_type_na` in `tests/tr_core/progs.rs`); the job passes 3 of 3, `oracle_exits` and `oracle_trace_modes` of the program pass too.
* **core/bb_tset** (a different cell failed in each of 3 runs: `-fold -cse`, `+randra=3`, `+membudget`; `stay end (Stay): pc 11 vs 7`, nested): the program stores through a table used as a key
  (`names[46] = names`); a pointer-keyed node sits at an address-dependent place of the hash part, so collisions and the insert path (and with them the tier-1 stay ends) differ between the two runs,
  and `keys()` builds strings of the key's `tostring`. Fix: the table key moved to a new program `bb_tset_tkey` (run by `tr_core::bb_tset_programs` in every mode, output compared with the interpreter,
  which is order independent), and `bb_tset_tkey` is left out of the oracle (`run_cells` filter). `bb_tset` itself passes 6 of 6 runs.
* **tests/vm/table_insert_layout** (about 1 run in 3): `live stack slot 17 ... tag 0x44 (string) vs 0x45 (table)`: `pairs` over tables with table keys visits the keys in an address-dependent order, so the
  loop's `k` is a table in one run and a string in the other at the same clock. The program is the layout-exactness check against the reference Lua (its output is already order independent); it cannot be made
  order independent in the oracle without dropping its table keys, so it is excluded from the oracle (`ADDR_ORDER` in `tr_oracle.rs`) and stays in `vm_corpus`. (Making the printed order sorted did not help: the live
  locals still differ.)

`scripts/gate-known.txt` has no `oracle` entries any more.

### 29.3 Tests, mutants

* Programs `tests/vm/k7_01_aref_phi.lua` (`tr_core::k7_aref_phi_programs`), `bb_tset_tkey` (in `bb_tset_programs`). Mutant `aref_phi_cse` (caught by `k7_aref_phi_programs`): registry 746 -> **747**
  (`mutants.rs`, `tests/tr_mutants.rs` row, `scripts/gate-mutant-map.tsv` regenerated).

## 30. L1 / L2: inline `string.byte` of external strings, the first two keys of a fresh table (2026-11-21, base 1e3a827)

Handles of docs/STUDY_LUAJIT_GAP.md (the study of the programs still weak against LuaJIT: permute, fib, raytrace, cd, lua_lexer, classic_vector).
Two built; the larger ones that study ranks (P1 presize from recorded `NEWREF`s, S1 lazy inline exit stubs, L3 recorded pattern `find`) are not built
(reasons and estimates there). Registry 747 -> 749.

### 30.1 L1 (`bytext`): `string.byte` of long strings with external bytes

`Asm::byte_fast` (F2) read only regular long strings (`shrlen == -1`) inline and called the `ALLOC_LIB` helper for every other long kind. In 5.5 a library
buffer longer than `BUFFER_STR_MIN` (1024 bytes) is adopted without a copy: `table.concat`, `string.pack`, `string.dump` results are **external** strings
(`LSTRMEM`, `shrlen == -3`; `LSTRFIX` -2 from the C API), whose bytes are behind a pointer at the contents offset, and `push_filled` results
(`string.rep`, ...) are library-buffer strings (`shrlen == -4`, bytes inline). `corpus/lua_lexer` lexes a 138904-byte `table.concat` result: 8.05 M
`string.byte` helper calls, `jit_tr_alloc` 17.9 % of the cycles. Now the inline form dispatches on the kind byte: -1 / -4 read behind the header as before, -2 / -3
load the pointer and read through it (the same bound checks against the length at `u.len`, which every long kind keeps), anything else (none exists) takes
the helper. 5.4 copies buffers into regular strings, so the change is inert there. Knob `bytext` (default 1; 0 = the old inline form). Mutant
`bytext_inline_read` (an external string read behind the header), caught by `tr_core::f2_programs` with the new program `l1_byte_ext` (external,
buffer, regular and short strings; positions at both ends, past the end, zero, negative, constant and variable; one site seeing four kinds in turn; the
strings die and are re-made; every trace mode, verify, exit storms, budgets, GC stress 1/2, stack-move stress, emulator).

### 30.2 L2: `insert_small`, the first two keys of a fresh table

The object idiom `setmetatable({}, C)` + `self.x = ..; self.y = ..` grows the hash part twice (dummy -> 1 node -> 2 nodes). The general path
(`newkey` -> `insertkey` failure -> `rehash` (counting loop) -> `resize_hash` -> `setnodevector`, reinsert, free -> `newcheckedkey` (array test) -> `insertkey`, then the
caller's second lookup) cost 241 instructions per key in classic_vector. rn-core `insert_small` (called from `newkey` for a short-string key into a table
with `asize == 0` and `lsizenode == 0`) does the same for exactly two shapes and declines everything else:

* the dummy part: one node (`rehash` counts the dummy as one deleted entry: `nsize = 1 + (1 >> 2) = 1`), the key inserted by `insertkey` (its
  `mainposition` computes the string's hash on first use: I-H lazy hashes; a first version that wrote node 0 directly left the hash uncomputed and
  `tests_gc::stress_random_workload_with_invariants` lost a field: every later lookup of that string was a miss);
* one node holding a live entry whose key is not an integer (an integer key sends `rehash` to the counting path, a deleted entry is reused in place,
  the same key is no insertion): two nodes, the old entry reinserted first, the old vector freed, then the new key, both through `insertkey`, exactly
  `resize_hash` + `newcheckedkey`'s order (the order decides which key gets the free node on a collision);
* then the two write barriers of `newkey`.

Result: same node vector sizes, layout, `lastfree` state (none below 8 nodes), accounting, barriers. 5.4 keeps the general path (its `rehash` differs; not
measured there). Tests: rn-core `tests_table::smallins_matches_generic` (400 seeds x 200 fresh tables, 1-4 insertions of pooled / fresh / integer / float /
boolean keys, nil stores, one-node constructor parts holding each key kind, deleted entries; after every insertion the fingerprint (sizes, nodes, links,
traversal, border), the accounted bytes, and a lookup of every key against the general path with the thread-local `NO_INSERT_SMALL`); the existing
`growth_fast_paths_match_generic` and the GC workload tests. rn-jit `tr_smallins::smallins_pairs_order_matches_the_reference` runs
`tests/vm_seed0/l2_small_insert_order.lua` (object idiom, three and four keys, constructors, deletions, array parts, `rawset`, a hot loop) in-process with
seed 0 against the seed-0 reference's output (`tests/data/l2_small_insert_order.expected`); mutant `smallins_order_swap` (the new key inserted
first). An "integer key let through" variant is caught by the unit test only (no Lua program reaches it: our VM never leaves the key 1 in a one-node part
of a table without an array part, and for every other integer key the general path sizes the same two-node part), so it is not a registry mutant.
Note: the rn-core unit tests are not part of `gate-light.sh` / `gate-heavy.sh`; run `cargo test -p rn-core` for a change there.

### 30.3 Measured (release `rn-cli`, `RUSTYNAIL_SEED=0`, `taskset -c 4`, min of 5 cycles, instructions of the first run, binaries at equal-length paths)

| program | base 1e3a827 Mcyc / Minstr | L1+L2 (6ee0b68) Mcyc / Minstr | d cycles | d instr | ratio to LuaJIT (cycles) before -> after |
|---|---|---|---:|---:|---|
| awfy/permute | 42 / 235 | 42 / 235 | +0.0 % | +0.0 % | 3.23 -> 3.23 |
| programs/fib | 1244 / 7780 | 1250 / 7780 | +0.5 % | +0.0 % | 2.65 -> 2.66 |
| apps/raytrace | 116 / 383 | 116 / 383 | +0.0 % | +0.0 % | 2.90 -> 2.90 |
| awfy/cd | 260 / 805 | 258 / 805 | -0.8 % | +0.0 % | 1.93 -> 1.91 |
| corpus/lua_lexer | 1239 / 5127 | 1089 / 4421 | -12.1 % | -13.8 % | 2.74 -> 2.41 |
| corpus/classic_vector | 640 / 2794 | 561 / 2376 | -12.3 % | -15.0 % | 2.61 -> 2.29 |
| awfy/richards | 59 / 180 | 59 / 180 | +0.0 % | +0.0 % | 1.69 -> 1.69 |
| awfy/havlak | 158 / 516 | 155 / 517 | -1.9 % | +0.2 % | 1.37 -> 1.35 |
| awfy/deltablue | 108 / 335 | 109 / 334 | +0.9 % | -0.3 % | 1.04 -> 1.05 |
| programs/nbody | 353 / 1986 | 353 / 1986 | +0.0 % | +0.0 % | - |
| programs/binary_trees | 1481 / 7298 | 1449 / 7298 | -2.2 % | +0.0 % | - |
| programs/queens | 1838 / 5391 | 1825 / 5391 | -0.7 % | +0.0 % | - |
| awfy/list | 95 / 414 | 95 / 414 | +0.0 % | +0.0 % | - |
| lib/tremove_front | 145 / 862 | 146 / 862 | +0.7 % | +0.0 % | - |
| corpus2/heldout/dijkstra_routes | 1170 / 3071 | 1174 / 3071 | +0.3 % | +0.0 % | - |
| awfy/json | 140 / 463 | 141 / 463 | +0.7 % | +0.0 % | - |
| apps/expr_lang | 143 / 565 | 140 / 557 | -2.1 % | -1.4 % | - |
| programs/string_ops | 1107 / 2879 | 1111 / 2879 | +0.4 % | +0.0 % | - |
| corpus/class_middleclass | 480 / 2254 | 481 / 2254 | +0.2 % | +0.0 % | - |
| apps/lru_cache | 145 / 343 | 146 / 343 | +0.7 % | +0.0 % | - |
| corpus/msgpack_pure | 2023 / 7901 | 1974 / 7781 | -2.4 % | -1.5 % | - |

Cycles move by +-1..2 % run to run at equal instructions (fib, binary_trees, queens, havlak: layout). The micro of the object idiom
(`setmetatable({}, mt); t.x = i; t.y = i`, 5 M): 995 -> 805 Mcyc (LuaJIT 849). 5.4: both handles are inert by construction (5.4 copies library
buffers into regular strings; `insert_small` is 5.5 only); the 5.4 suites pass. LuaJIT ratios use its min of 3 at the default size (permute: its
best run of a very unstable series, see the study). Not kept: a prefetch of the next free pool block at every pop (`alloc_small`): classic_vector
559 -> 553, raytrace 115 -> 117, cd 258 -> 257, binary_trees 1472 -> 1487 Mcyc: noise.

### 30.4 Gates

Light gate per commit (L1 at 1c9c5d0, L2 at 6ee0b68): PASS (fmt, mutant map, clippy 5.5 / 5.5-jit / 5.4, trace suites 5.5 and 5.4, vm_corpus, jit-diff 1-50, the changed-file mutant groups incl. `bytext_inline_read`; for L2 the `smallins_`, `shape_`, `newkey_`, `newref_`, `tins_`, `tnew_` groups). `cargo test -p rn-core` (5.5 and 5.4 lib): PASS.

`gate-heavy.sh` (2026-11-21, at 6ee0b68, 4451 s): OK. light PASS; tests-5.5-oracle KNOWN (`tr_identity_pre::fetch_plus_insns_identity` only); tests-5.4-oracle PASS;
`oracle_options` PASS; jit-diff 1-200 (7 modes) PASS; mutant runner PASS (741 ran, 741 caught, no survivors; registry 749). `scripts/gate-mutant-times.tsv` refreshed by that run.

## 31. S1 (`lazystub`): exit stubs assembled at the first exit (2026-11-21, base d2f5114)

Candidate S1 of docs/STUDY_LUAJIT_GAP.md. Finding there: at the default size `awfy/cd` spends a quarter of its cycles in the trace compiler, and the
installed trace code is 1.58 MB, most of it exit stubs. Measured at the base (stub region per trace, `[trace] code` line of this change): cd 1111 KB of
stubs in 1578 KB of trace code (70 %; about 300 bytes per stub), expr_lang 402 of 575 KB, lru_cache 531 of 786 KB, raytrace 485 of 628 KB; and 91 % of
the snapshots of cd (4111 of 4499) never exit at all (exit counters at the flush: 4 snapshots exit once, 21 two or three times, 48 four to fifteen
times, 315 sixteen times or more). An oracle prototype (stubs of the snapshots that exit in a previous run assembled as before, the others reduced
to the generic stub) gave the bound: cd -5 % cycles and -4.9 % instructions, expr_lang -6.6 %, raytrace -5.5 %, lru_cache -3.8 %, deltablue -2.6 %
(within the instrumented binary).

### 31.1 Design

* **What is deferred.** Every stub the loop of `Cx::stubs_and_tail` would give an inline part: the `inline_stub` (frameless, frame-cell, sunk-table,
  `mat_stub` frame rebuild), `sunk_frame_stub` and `link_mat_stub` kinds. Generic-only stubs (they are already short), entry-check stubs (they use the
  extension slots of the trace's allocation), poll and shape stubs keep their form. The decision is the old code moved into `Cx::stub_plan`
  (`StubPlan::{SunkFrames, LinkMat, Inline, Generic}`), the emission into `Cx::emit_stub`; the eager path is unchanged (`lazystub=0`).
* **The trace's code for a deferred exit** (`Cx::defer_stub`): at the stub label a 5-byte `nop`, then the generic stub tagged
  `exit::EXIT_DEFER` (bit 28 of `exitno`): 5 + about 21 bytes instead of about 300. What the eager stub tells the installer is told at once (the
  double-counting `mat_stub` snapshot goes into `Asm::dbl`).
* **First exit** (`exit::jit_tr_exit` -> `defer_build`, before `exit_entry` runs, so nothing is restored or counted): the handler assembles the stub of
  that snapshot with `asm::assemble_deferred` (a fresh `Cx` over the same `ExitSnap`, `AsmEnv` and IR-derived tables, kept in `ExitMeta::defer` as
  `DeferCtx`; the stub plan is recomputed from the same inputs and must not be `Generic`), with its generic twin jumping to the trace's embedded shared
  exit routine (untagged `exitno`), places it (`rt::place_code_with`, the J-PACK tail page), patches the branches of the trace that go to the `nop`
  (the `jmp` / `jcc rel32` fixups of the stub label, collected by `x86::emit_ex` and checked by opcode) and the `nop` itself (`jmp rel32`) to the new
  stub (`CodeArena::patch`: the pages RW, written, RX again, docs/UNSAFE.md J-PATCH), and returns `exit::EXIT_RESUME` (2) with the stub address in
  `ExitArea.args[0]`. The shared exit routine (`append_shared_exit` with `resume`) then reloads every GPR (except the dead `V7`) and XMM register from
  the exit area, where it had saved them, and jumps to the stub (`KEEP_R14`: no `ReloadK` in front of that jump, nor in front of the twin's jump to the
  shared routine, so a value of the exit map in `r14` survives). So the first exit, and every later one, runs exactly the stub the eager assembly would
  have emitted, with the same registers: counting, hot diverts, side-trace cells, carry paths, sunk-table materialisation, lazy leave blocks and frame
  rebuilds behave as before by construction (the twin test below checks that the exit counts and installs are identical).
* **Later exits** branch straight to the stub (no extra jump: the guard's own `jcc` was patched). Should a patch be impossible (a rel32 out of
  range, `mprotect` refused), the exit keeps going through the tagged generic stub and the handler resumes in the built stub every time
  (`DeferInfo::built`); should the assembly or the placement fail (code cap, memory limit), the exit is marked failed and handled as an untagged
  exit by the handler from then on (correct; its counter then advances by one per exit through the handler, as in `generic_only` mode).
* **Shared blocks.** The frame-rebuild chains (`mat_shared`) and the lazy leave blocks are emitted once per trace: the first deferred stub that needs
  one emits it, later ones jump to it (`DeferShared`, the jump retargeted to a direct `jmp rel32` once the stub's address is known:
  `DeferredStub::relocate`). Without that the rebuild code was duplicated per stub and the L1i misses of cd went up (3.8 M -> 6.6 M).
* **Lifetime.** The stub blocks belong to the trace (`Trace::dstubs`, charged like the trace code, freed and refunded in `cache::retire`); perf map names
  `TRACE-<no>-STUB`; the sampler attributes them to the trace.
* **Not under the emulator** (`+emurun` / `+emuverify`: the emulated program would need the code assembled later), and the trace modes that send
  every exit to the handler keep doing so.
* Knob `lazystub` (default 1; 0 = every stub with the trace). Verbose statistics: `[trace] code: traces N B; lazystub: stubs built S (B B), patched P,
  failed F`, and the compile phase `lazystub` (assembly + placement + patch of the deferred stubs) in `[trace] phases`.

What did not work or did not matter (measured on cd, equal-length binaries): a jump through a per-exit data cell instead of patching (+3 instructions
and a dependent load per exit: -1.7 % instead of -2.3 %); patching only the `nop` (one extra `jmp` per exit: +1 instruction per exit on the exit storms,
fib +39 M instructions); 64-byte alignment of the stub blocks (short_inner 1120 vs 1101 Mcyc at 16-byte alignment); four extra `mprotect` pairs per
stub (no change in user cycles; the kernel time of the two `mprotect` pairs per built stub is about 2.6 us each pair, 1 ms on cd, inside the wall-time
gain below). The gap to the oracle (about 1.5 %) is build cost per stub (a fresh assembler and x86 emitter, about 7.8 k TSC ticks per stub of which
the system calls are most) and layout (branch misses +4 %).

### 31.2 Measured (release `rn-cli`, `RUSTYNAIL_SEED=0`, `taskset -c 4`, binaries at equal-length paths; base d2f5114 = `bas`)

Code size and compile time per program, the same binary with `lazystub=0` (eager, byte-identical stubs to the base) and `lazystub=1`: trace code KB
(the blocks of the traces), the deferred stubs built (KB, count), the assembly-side compile phases `asm + x86 + install` in million TSC ticks (min of
3 runs), the `lazystub` phase (assembling, placing and patching the deferred stubs, system calls included), and the change of the whole compile
pipeline (`pipeline + lazystub` against `pipeline`):

| program | trace code KB eager -> lazy | stubs built KB (n) | asm+x86+install Mt eager -> lazy | lazystub Mt | pipeline |
|---|---|---|---|---:|---:|
| awfy/cd | 1541 -> 447 | 147 (380) | 20.6 -> 11.4 | 5.59 | -4.9 % |
| apps/expr_lang | 562 -> 174 | 36 (103) | 5.29 -> 2.73 | 1.09 | -14.9 % |
| apps/lru_cache | 768 -> 245 | 96 (236) | 7.15 -> 3.93 | 2.68 | -1.6 % |
| apps/raytrace | 614 -> 153 | 21 (55) | 5.55 -> 2.76 | 0.69 | -16.8 % |
| awfy/deltablue | 391 -> 135 | 19 (74) | 3.93 -> 2.24 | 0.78 | -10.9 % |
| awfy/richards | 179 -> 74 | 14 (49) | 1.87 -> 1.23 | 0.59 | +3.7 % |
| awfy/havlak | 139 -> 71 | 11 (52) | 1.70 -> 1.14 | 0.49 | -1.5 % |
| awfy/json | 428 -> 177 | 23 (92) | 4.38 -> 2.71 | 0.95 | -6.7 % |
| awfy/list | 144 -> 61 | 14 (48) | 1.58 -> 0.97 | 0.48 | -2.4 % |
| corpus/lua_lexer | 394 -> 153 | 34 (129) | 4.01 -> 2.40 | 1.29 | -1.9 % |
| corpus/classic_vector | 312 -> 137 | 7 (16) | 3.46 -> 2.26 | 0.26 | -9.2 % |
| corpus/class_middleclass | 513 -> 181 | 40 (111) | 5.22 -> 3.28 | 1.24 | -3.9 % |
| corpus/msgpack_pure | 283 -> 125 | 26 (83) | 3.25 -> 2.15 | 0.99 | +2.5 % |
| corpus2/heldout/dijkstra_routes | 771 -> 258 | 87 (320) | 6.93 -> 4.06 | 3.17 | +6.1 % |
| programs/nbody | 34 -> 13 | 1 (3) | 0.44 -> 0.28 | 0.05 | -6.0 % |
| programs/queens | 43 -> 31 | 5 (43) | 0.63 -> 0.54 | 0.32 | +22 % (of 1.5 Mt) |
| programs/fib, binary_trees, permute, short_inner | 7-24 KB, nearly every stub is taken | 0.5-3 | | 0.05-0.13 | +3..37 % (of 0.5 Mt or less) |

Trace code shrinks by 60-75 % (with the stubs built: cd 1541 -> 594 KB). The pipeline gains least where most stubs are taken and the traces are few
(a deferred stub costs about twice its eager assembly: a fresh assembler, emitter and placement with two `mprotect` pairs each); those programs
compile for well under a millisecond either way.

Cycles and instructions (`cycles:u`, `instructions:u`, min of 5; the jit-trace set min of 3):

| program | Mcyc bas -> new | d cyc | Minstr bas -> new | d instr |
|---|---|---:|---|---:|
| awfy/cd | 257.6 -> 253.4 | -1.6 % | 805.3 -> 770.8 | -4.3 % |
| apps/expr_lang | 139.4 -> 133.9 | -3.9 % | 557.5 -> 544.3 | -2.4 % |
| apps/lru_cache | 145.6 -> 142.3 | -2.2 % | 337.2 -> 322.0 | -4.5 % |
| apps/raytrace | 116.7 -> 112.9 | -3.3 % | 383.1 -> 367.1 | -4.2 % |
| awfy/deltablue | 108.5 -> 107.0 | -1.4 % | 334.6 -> 325.6 | -2.7 % |
| awfy/richards | 58.3 -> 59.1 | +1.3 % | 180.0 -> 176.7 | -1.8 % |
| awfy/havlak | 154.3 -> 152.7 | -1.0 % | 516.9 -> 515.0 | -0.4 % |
| programs/nbody | 353.5 -> 352.9 | -0.2 % | 1986.7 -> 1985.6 | -0.1 % |
| programs/fib | 1241.7 -> 1250.7 | +0.7 % | 7780.7 -> 7780.7 | 0.0 % |
| programs/binary_trees | 1480.9 -> 1466.0 | -1.0 % | 7301.1 -> 7301.2 | 0.0 % |
| programs/queens | 1827.3 -> 1850.4 | +1.3 % | 5391.2 -> 5391.2 | 0.0 % |
| awfy/list | 95.5 -> 95.5 | 0.0 % | 411.6 -> 409.4 | -0.5 % |
| awfy/permute | 42.7 -> 43.0 | +0.8 % | 235.6 -> 235.2 | -0.2 % |
| corpus/lua_lexer | 1092.0 -> 1090.5 | -0.1 % | 4421.5 -> 4415.1 | -0.1 % |
| corpus/classic_vector | 560.6 -> 554.7 | -1.1 % | 2376.5 -> 2372.1 | -0.2 % |
| corpus2/heldout/dijkstra_routes | 1173.0 -> 1171.4 | -0.1 % | 3071.3 -> 3056.7 | -0.5 % |
| lib/tremove_front | 145.4 -> 150.0 | +3.2 % | 862.1 -> 862.0 | 0.0 % |
| awfy/json | 140.2 -> 140.0 | -0.1 % | 463.6 -> 454.6 | -1.9 % |
| programs/string_ops | 1090.4 -> 1108.1 | +1.6 % | 2879.1 -> 2879.1 | 0.0 % |
| corpus/class_middleclass | 480.7 -> 482.7 | +0.4 % | 2254.9 -> 2244.3 | -0.5 % |
| corpus/msgpack_pure | 1986.5 -> 1988.8 | +0.1 % | 7781.3 -> 7776.7 | -0.1 % |

The 29 jit-trace programs (the hot-exit and exit-storm set): instructions identical or lower everywhere (largest -0.5 %, rec_loop), cycles -0.6 ..
+2.9 % (tfor_loops +2.9 %, sink_tuple +1.8 %, alloc_ctor +1.4 %, the rest within +-1.2 %). No program executes more instructions: a hot exit runs the
same stub code as before (its guard was patched to branch to the stub directly; the first version that patched only the `nop` cost one `jmp` per exit,
+39 M instructions on fib). The cycle increases at equal instructions are layout: lib/tremove_front measures 150-153 Mcyc with the new binary at
`lazystub=0` as well as `=1` (base 145-149: the binary's own code moved), tfor_loops 223-233 in both settings of the new binary against 220-238 at the
base, string_ops lower with the new binary in both settings; fib and queens are bimodal by ASLR at the base already (fib 1244-1407 over 14 base runs).
Wall time of cd (20 runs, kernel time included): 53.8 -> 52.6 ms.

Against the time box: cd gains -1.6 .. -2.6 % cycles over the runs of this session (-4.3 % instructions), below the 3 % bar the task set for cd;
the change was kept because it is behaviour-identical by construction (the twin test), knob-guarded, never adds instructions, cuts trace code by
60-75 % and the compile pipeline by 5-17 % on the short programs, and gains 2-4 % cycles on expr_lang, raytrace, lru_cache, deltablue. The study's
-6..10 % for cd was an overestimate (it assumed the stub share of compile time was larger than the 15 % the oracle shows).

### 31.3 Tests and mutants

* `tr_core::s1_lazystub_programs`: eight program / limit combinations (`s1_lazy_exits`, new: first exits with live floats and integers, inlined
  frames, sunk tables, hot exits; `h2_fmat`, `sinkf_side`, `j1_sinkf`, `r1_nbody_pairs`, `s1_frame_sides`; `s1_lazy_exits` and `p2_xregs` with
  `xregs=3`) x `always` / `on2` x plain, `+verify`, exit storms 3 and 7 (`+randra=5`), budget 17, GC stress 2, stack-move stress, each with
  `lazystub=1` and `lazystub=0`: every job equals the interpreter, the twins have the same exits, installs, side traces and recordings, no deferred
  stub fails, at least one is built, and the lazy trace code is smaller.
* `mem::tests::patch_rewrites_committed_code` (two `mprotect`s per patch, refusal outside committed code). The existing suites run with deferral
  on (it is the default of the test limits too), so every trace test exercises it; the emulator modes run with it off.
* Mutants (registry 749 -> **754**), all caught by `s1_lazystub_programs`: `lazystub_resume_xmm` (the resume path does not reload the XMM registers),
  `lazystub_resume_v8` (nor `V8`), `lazystub_reloadk` (a `ReloadK` in front of the jumps that must keep `r14`), `lazystub_dbl_skip` (the
  double-counting snapshot of a deferred `mat_stub` is not told to the installer; *corrected in section 33*: it changes only the P3 chain re-order, and
  its only detection here was the seed flake, now `s1_dbl_chain`), `lazystub_site_rel_pos` (the patched
  rel32 is relative to the field, not the end of the instruction). Perf-only variants (no patch, an indirect jump to the shared blocks, a copy of the
  rebuild chain per stub) are not registry mutants: they keep behaviour.
* Bug found by the gate and fixed before the commit: under `+emurun` the program is replaced after assembly, so the embedded shared routine's label
  must not be looked up in the emitted code (index out of range in 299 trace-suite tests); the lookup now happens only when a stub was deferred.

### 31.4 Gates

Light gate (with the changed-file mutant plan: 162 mutants incl. the five new ones): PASS (1047 s; `core-5.5` / `core-5.4` are the new rn-core steps).

`gate-heavy.sh` (2026-11-21, at acaf7de, 4373 s): OK. light PASS; tests-5.5-oracle KNOWN (`tr_identity_pre::fetch_plus_insns_identity` only); tests-5.4-oracle PASS; `oracle_options` PASS;
jit-diff 1-200 (7 modes) PASS; mutant runner PASS (746 ran, 746 caught, no survivors; registry 754). `scripts/gate-mutant-times.tsv` refreshed by that run.

## 32. P1 (`presize`): constant-key insertion groups into a fresh table presized to the interpreter's final layout (2026-11-21, base 504b832)

Candidate P1 of docs/STUDY_LUAJIT_GAP.md. Finding there: on `corpus/classic_vector` 48 % of the instructions were the two rehashes of every
`setmetatable({}, C)` + `self.x = ..; self.y = ..` object (L2 cut part of it, -12 %), and the source rewrite `{x = false, y = false}` showed a further
-19 % ceiling. The obstacle the study named is TR29.1 (content, `#t` and `next` order of every table identical to the interpreter's) in the window
between the allocation and the last insertion.

### 32.1 What the interpreter's growth is, and when presizing reproduces it

* A fresh table (`{}`: dummy hash part, no array part) that receives short-string keys `k1..kn` grows at the 1st, 2nd, 3rd, 5th, 9th key to
  1, 2, 4, 8, 16 nodes (5.5 `rehash`: the dummy counts as one deleted entry, `1 + 1 >> 2 = 1`; 5.4: no bonus, the same sizes), each step allocating
  the new vector, reinserting the old nodes **in node-index order**, freeing the old vector, then inserting the new key.
* Presizing to `N = 2^ceillog2(n)` nodes and inserting `k1..kn` in order gives the same node vector for two keys always (the one-node step holds
  only `k1`, the two-node step reinserts it and inserts `k2`: exactly the presized order). For three or more keys it depends on the hashes: a growth
  step reinserts colliding keys in node order, which can differ from insertion order (rn-core `tests_table::presize_matches_growth`: both outcomes
  occur, for 3..8 keys). So the final layout **is** a pure function of the key sequence and the key hashes, but presizing is exact only for some key
  sets; the compiler must check each group.
* No size hint for the interpreter or tier 1 (a `NEWTABLE` hint derived from the constructor's following `SETFIELD`s): a hinted `{}` would hold
  the presized layout at every intermediate point of the interpreter's execution (observable with `pairs` / `next` between the stores, with integer
  keys that the growth sends to the array part, and in `collectgarbage("count")` between statements), would differ from the growth for 3+ keys on
  some key sets (seed dependent, not checkable at compile time) and from the reference's `pairs` order (tests/vm_seed0 expectations). Rejected.

### 32.2 Design (the narrowest sound form)

* **Recorder notes** (`RecCfg::presize`): at every helper `NEWREF` of a constant key into a table allocated by this trace (a `TNEW` after the last
  `TSET`, the A1 condition), the recorder tags the instruction (`nrplan::NOTE` + index) and keeps a `PresizeNote`: was the table empty before, and
  the VM's `flags` and hash image right after its insertion (read at the next recorder step). These images are the **interpreter's own layout** for
  the recorded key objects.
* **The group** (pass `presize::run`, last pass before register allocation, so it sees the final IR): a run `NEWREF(t, k1) .. NEWREF(t, kn)` (2..8
  members, all tagged, the first on an empty table) in which every instruction between the first and the last member is a member, the `NE` guard of
  a member, an `HSTORE` into a member's node, a `TBAR` of `t`, or a pure instruction without guard (arithmetic, conversions, `SLOAD` / `FLOAD` of
  other objects; no `FLOAD` of `t`). No other guard, no call, no allocation, no snapshot use: **nothing can exit or look at the table between the
  insertions**, so no exit, side trace, collector step or observer ever sees an intermediate state. A guard between two stores (a type guard of a
  loaded value, `x or 0` with an unknown `x`) ends the group there.
* **Exactness check**: `rn_core::table::presize_sim(keys, flags)` simulates the presized group with the table's own `insertkey` on a scratch node
  vector outside the heap and returns the A1 plan of every key after the first and the final image; the group is rewritten only when that image
  equals the interpreter's image after the last member (the note), every plan is encodable and the flags agree. A group whose image differs is tried
  again with its prefixes (the rest then grows through the helper from the interpreter's own intermediate state).
* **Rewrite**: the first member becomes the presizing helper (`nrplan::PRESIZE | lsize`; `class_a` passes `lsize` as `x`, `jit_tr_newref` ->
  `TableRef::newref_presize`), the others inline plans (A1: stores at the planned nodes, no helper call, their `NE` guards assembled to nothing).
* **`newref_presize`** checks at run time that the table is still empty (dummy part, no array part) and otherwise does nothing (the trace exits
  before the instruction, the interpreter inserts). It **replays the interpreter's growth steps on the allocator**: the same `alloc_counted` /
  `free_counted` calls with the same sizes in the same order (`A(1) A(2) F(1) A(4) F(2) .. A(N) F(N/2)`), without initialising the intermediate
  vectors, then initialises the final vector as `setnodevector` does and inserts `k1` through `insertkey` (barriers, `invalidateTMcache`). So the
  accounted bytes, every memory-limit check (each step checks what the interpreter's step checks; a failing step makes the helper free what it took
  and do nothing, and the interpreter then fails at the same key with its own error) and the pool allocator's free lists end as the interpreter
  leaves them: later objects get the same addresses (`tests_table::presize_matches_growth` compares the allocator traffic block by block).
  The replay costs about 2 % of classic_vector's instructions (a prototype without it: 2203 instead of 2253 M instructions); with it, nothing
  address-dependent changes.
* **The `__newindex` memo** (recorder): the second store of `self.x = ..; self.y = ..` repeated the metatable guards of the first (the `NEWREF` is a
  header-mutating op, so nothing merged them), and those guards sat in the window. `Rec::p1_memo_holds`: a `NEWREF` into a table of this trace
  leaves out its `__newindex` guard when the previous `NEWREF` into the same table took or skipped it, its key does not start with `__`, and the IR
  since then holds only that `NEWREF`, its `NE` guard, the `HSTORE` into its node, a `TBAR` of the table and pure instructions. Then neither the
  metatable field nor any table's contents changed except by that one key, which cannot be `__newindex` (so even a table that is its own metatable
  gets no metamethod); the record-time check `no_tm` still runs. Without a group the memo is still sound (it only removes a guard that cannot fail).
* **Exit storms**: the guard of the presizing `NEWREF` joins the allocation guards that `+exitstorm` leaves alone (it fails for real only when the
  helper did nothing; a spurious exit would hand the interpreter the presized part).
* Knob `presize` (default 1; 0 = no notes, no memo, every insertion through the helper as before); verbose statistic `[trace] presize: groups N`.
  Applies to 5.4 as well (same growth sizes; 5.4 hash parts all carry `lastfree`, which the plans already handle; the 5.4 unit test and trace
  suite run it).

Not covered (by design): a constructor whose `TNEW` is in another trace than the stores (function-entry traces of `init` methods, `oop_mixins`,
`class_middleclass`), stores separated by guards, tables with a constructor hash size (they take A1 directly), dynamic keys.

### 32.3 Soundness argument (TR29.1, TR29.6)

1. *End state*: after the last member the table's node vector (keys, chain links, empty values, `lastfree`, layout-id slot), `lsizenode`, flags and
   array part are the interpreter's: the image equality was checked against the VM's own result for the same key objects, the image is a function of
   the key sequence and their hashes, which do not change between runs (constant keys, the table is new and checked empty at the first member).
2. *No intermediate state is observable*: between the first member's guard and the last member nothing can exit, call, allocate, step the collector
   or read the table (the group condition); the first member's guard fails only when the helper did nothing (the table is still empty: exactly the
   interpreter's state before the instruction).
3. *Accounting and allocator*: equal by construction (the replayed calls), checked by the unit test's allocator log; GC colour and age of the table
   are untouched (no collector step), barriers are the interpreter's (`newkey`'s two `barrier_back`s; the inline plans rely on the table being white
   as A1 does).
4. *Memory errors*: the replay makes the interpreter's checks in its order; any failure means no change and an exit before the first store.
5. With sinking off (TR29.6) every GC observable is therefore exactly JIT-off: the trace never makes the table differ from the interpreter's at a
   point where anything can look.

### 32.4 Tests and mutants

* rn-core `tests_table::presize_matches_growth` (60 seeds x 150 tables, 2..8 keys from a 48-name pool, two heaps with logging allocators: plans of
  the real presized table = simulated plans, presized image = simulated image, accounted bytes and allocator traffic = the growth's, `pairs` order
  and border = the growth's whenever the images are equal; both outcomes occur), `presize_memory_limit_and_refusals` (5.5 and 5.4).
* `tests/vm_seed0/p1_presize_order.lua`: groups of 2..9 keys with several name sets, growth past the presized part and integer keys after it, the
  object idiom (constructor + `init`, three-field constructor), a guard inside a group whose exit is followed by an integer key, metatables changed
  between two stores (`__newindex` appears), a table that becomes its own metatable at run time and then gets `__newindex` from the first store,
  nil / dynamic keys inside a group, constructors with a hash size. `tr_presize::presize_pairs_order_matches_the_reference`: its interpreter output
  equals `ref/55/build-seed0/lua`'s (`tests/data/p1_presize_order.expected`; `ref/54` prints the same).
* `tr_core::p1_presize_programs`: that script (`p1_presize`) in `always` / `on2` x plain, `+verify`, exit storms 3 and 7 (`+randra=5`), `-sink`,
  budget 17, GC stress 1 / 2 (generational and incremental), the stack-move stress, each with `presize=1` and the twin `presize=0`, plus `+emurun`:
  every run prints the interpreter's output, `presize=0` presizes nothing, the plain runs presize at least four groups.
* Mutants (registry 754 -> **759**), all caught by `p1_presize_programs`: `p1_image_check_skip` (a 3+ key group whose growth layout differs is
  presized: `pairs` order), `p1_window_any` (a guard inside the group: its exit leaves the presized part, the later `t[1]` goes to the hash part),
  `p1_memo_any` (the second store's `__newindex` guard is left out across a `setmetatable`), `p1_memo_dunder` (left out after a `__newindex` store
  into a table that is its own metatable), `presize_no_free` (rn-core: the last replayed vector is not freed: the heap's leak check at the end).
  The storm exemption of the presizing guard and the allocator replay are not registry mutants: no program output depends on them (a storm exit
  there resumes at the same store, which then finds its key and the free nodes; the replay only fixes which addresses later objects get).

### 32.5 Measured (release `rn-cli`, `RUSTYNAIL_SEED=0`, `taskset -c 4`, cycles min of 5, instructions of the first run, binaries at equal-length paths; base 504b832)

| program | 5.5 Mcyc base -> P1 | d | 5.5 Minstr | d | 5.4 Mcyc base -> P1 | d | 5.4 Minstr | d |
|---|---|---:|---|---:|---|---:|---|---:|
| corpus/classic_vector | 550.2 -> 468.7 | **-14.8 %** | 2371.5 -> 2050.5 | **-13.5 %** | 785.0 -> 646.6 | **-17.6 %** | 3541.2 -> 2765.0 | **-21.9 %** |
| awfy/deltablue | 108.4 -> 107.2 | -1.1 % | 325.5 -> 323.1 | -0.7 % | 117.1 -> 117.5 | +0.3 % | 350.4 -> 347.8 | -0.7 % |
| corpus2/dev/oop_mixins | 1420.1 -> 1421.5 | +0.1 % | 5547.7 -> 5551.0 | +0.06 % | 1456.0 -> 1419.4 | -2.5 % | 5773.1 -> 5777.3 | +0.07 % |
| real/game_loop | 33.6 -> 33.9 | +0.9 % | 148.1 -> 148.1 | 0 | 38.3 -> 38.3 | 0 | 170.3 -> 170.2 | 0 |
| corpus2/heldout/roguelike_turns | 800.4 -> 802.5 | +0.3 % | 2773.7 -> 2774.0 | 0 | 862.3 -> 859.3 | -0.3 % | 3074.9 -> 3074.6 | 0 |
| programs/binary_trees | 1491.2 -> 1489.5 | -0.1 % | 7305.1 | 0 | 1766.3 -> 1745.8 | -1.2 % | 8237.4 | 0 |
| apps/raytrace | 108.0 -> 107.9 | -0.1 % | 352.0 -> 351.1 | -0.3 % | 116.4 -> 117.4 | +0.9 % | 364.3 -> 363.5 | -0.2 % |
| apps/raytrace 40 | 1174.0 -> 1166.4 | -0.6 % | 4614.2 -> 4607.4 | -0.2 % | 1196.3 -> 1191.2 | -0.4 % | 4519.6 -> 4509.9 | -0.2 % |
| awfy/cd | 253.8 -> 253.0 | -0.3 % | 770.5 -> 770.2 | 0 | 279.9 -> 279.0 | -0.3 % | 841.2 -> 840.9 | 0 |
| awfy/richards | 59.5 -> 59.6 | +0.2 % | 176.7 | 0 | 58.9 -> 59.7 | +1.4 % | 177.5 | 0 |
| programs/nbody | 347.6 -> 347.4 | -0.1 % | 1985.6 | 0 | 350.8 -> 350.5 | -0.1 % | 2037.6 | 0 |
| programs/fib | 1232.1 -> 1233.4 | +0.1 % | 7780.7 | 0 | 1242.1 -> 1248.1 | +0.5 % | 7780.7 | 0 |
| programs/queens | 1859.6 -> 1847.9 | -0.6 % | 5391.2 | 0 | 1919.2 -> 1923.0 | +0.2 % | 5878.6 | 0 |
| awfy/list | 93.0 -> 92.5 | -0.5 % | 394.3 -> 394.4 | 0 | 97.3 -> 97.0 | -0.3 % | 414.4 -> 414.5 | 0 |
| corpus/lua_lexer | 1089.8 -> 1092.8 | +0.3 % | 4415.7 -> 4415.8 | 0 | 1149.5 -> 1138.0 | -1.0 % | 4496.3 -> 4496.4 | 0 |
| apps/expr_lang | 147.4 -> 147.0 | -0.3 % | 570.5 -> 571.2 | +0.1 % | 158.7 -> 156.4 | -1.4 % | 609.0 -> 609.5 | +0.1 % |
| corpus2/heldout/dijkstra_routes | 1174.5 -> 1169.1 | -0.5 % | 3056.6 -> 3056.7 | 0 | 1461.0 -> 1450.2 | -0.7 % | 4124.0 | 0 |
| lib/tremove_front | 149.9 -> 147.1 | -1.9 % | 862.0 | 0 | 183.6 -> 182.3 | -0.7 % | 1044.7 -> 1044.8 | 0 |
| awfy/permute | 43.5 -> 43.0 | -1.1 % | 235.2 | 0 | 43.6 -> 43.7 | +0.2 % | 242.0 | 0 |

Sanity set (23 more programs: havlak, json, string_ops, class_middleclass, lru_cache, msgpack_pure, storage, bounce, towers, sieve, mandelbrot,
awfy/nbody, ecs_world, particles_physics, text_adventure, lua_parser, event_promise, memo_cache, events, markdown, json_roundtrip,
template_engine): identical output, instructions within +-0.35 % on both versions (the largest: lru_cache +0.34 % / +0.26 %, particles_physics
-0.25 %). Only classic_vector (27 groups) and deltablue (3) form presized groups at all; every other difference is the binary. Cycles at equal
instructions: msgpack_pure +2.1 % and template_engine +1.5..2.0 % on 5.5, text_adventure / json_roundtrip / event_promise +2 % on 5.4 (event_promise
on 5.5 varies 345..386 Mcyc between identical runs); the same binary with `presize=0` measures the same as with `presize=1` on those programs
(msgpack 2034 / 2033, template_engine 730 / 730), so they are code placement of the new binary, not the change's behaviour (a variant with the new
functions marked `#[cold]` moved them around: msgpack +1.5 %, template_engine -0.8 %, classic_vector +0.7 % instructions; not kept). The knob-off
residue of +0.3 % instructions on lru_cache is in the compile pipeline and the GC step code of the rebuilt crate, the JIT's decisions are identical
(verbose summaries equal).

5.4 gains more (no `insert_small` there: every one of the two growth steps took the general rehash chain, which P1 now skips). Against LuaJIT
(245 Mcyc): classic_vector 2.29 -> 1.91 (5.5). The source-rewrite ceiling was 453 Mcyc after L2; P1 reaches 469 (the remaining difference: the
first key still goes through a helper call and the replayed growth steps).

### 32.6 Gates

Light gate (at b2a3881, base 504b832, changed-file mutant plan of 96 mutants): fmt / mutant-map FAIL (a rustfmt line of `presize.rs` and the map's
test lines after the last edit: fixed in 685d03e, both steps PASS), clippy 5.5 / 5.5-jit / 5.4, core-5.5, core-5.4, trace suites 5.5 and 5.4,
vm_corpus, jit-diff 1-50: PASS; mutant group 92 of 96 caught, the four survivors `lazystub_*` because the baseline of their detector failed (see
below). The five P1 mutants: caught (also in separate `gate-mutants.sh --only p1_,presize_no_free` runs).

`gate-heavy.sh` (2026-11-21, at 685d03e, 4550 s): light PASS; tests-5.5-oracle KNOWN (`tr_identity_pre::fetch_plus_insns_identity` only);
tests-5.4-oracle PASS; `oracle_options` PASS; jit-diff 1-200 (7 modes) PASS; mutant runner 751 ran, 750 caught, survivor `lazystub_dbl_skip`.

**Pre-existing flake, not P1:** `tr_core::s1_lazystub_programs` is not deterministic in the oracle-feature build (`tr_mutants,t15_oracle`): the job
`sinkf_side [always +exitstorm=7 +verify +randra=5]` sometimes gives the `lazystub=1` and `lazystub=0` twins different exit / side-trace counts
(e.g. 19105 vs 19000 exits). Measured at the base 504b832 (its own tree and target): 3 failures in 8 identical runs; with this change 3 in 6
(`presize=1`) and 1 in 6 (`presize=0`). `lazystub_dbl_skip` is detected only through that job's exit counts, so it survives whenever the mutated
run happens to agree (it was caught in 1 of 2 manual runs). Owner: S1 (the twin comparison or the run-to-run determinism of storm exits with
`+randra`).

*Amended (section 33):* not a lazystub bug and not `+randra`: the test did not pin the string-hash seed, so each child process (both twins) ran
with its own random seed, and that job's exit count is a function of the seed (19000 or 19105 exits); `lazystub=0` alone differs from itself
across runs as often. The `lazystub_dbl_skip` "catches" were that flake: the mutant does not change this job at any seed. Fixed in the harness;
the mutant now has a deterministic detector (`s1_dbl_chain`).

## 33. The `s1_lazystub_programs` flake: an unpinned string-hash seed, not lazystub (2026-11-21, base a536f04)

Section 32.6 recorded that the job `sinkf_side [always +exitstorm=7 +verify +randra=5]` of `tr_core::s1_lazystub_programs` (oracle-feature build)
sometimes gave the `lazystub=1` and `lazystub=0` twins different counts (19105 vs 19000 exits), and that `lazystub_dbl_skip` was detected only
through it. Classification: **(c) a test-harness artifact**, with an input dependence of type (b) behind it. No production change.

### 33.1 Evidence (the debug test binary of `cargo test -p rn-jit --features tr_mutants,t15_oracle --test tr_core`, the job run directly as a child)

* **The eager twin is nondeterministic on its own.** 40 runs of the job with `lazystub=0`: 32 x 19000 exits / 43 failed side traces, 8 x 19105 /
  40; with `lazystub=1`: 37 x 19000, 3 x 19105 (the same two outcomes, no third one). So the twins disagree whenever their two processes land on
  different outcomes; nothing lazystub-specific.
* **The outcome is a function of the string-hash seed.** `tr_core::one_state` uses `TR_CORE_SEED` when set and otherwise
  `rn_std::new_state_with_libs` = `auxlib::make_seed()` (time and a stack address: a different seed per process). With `TR_CORE_SEED=0` 40 of 40
  lazy runs give 19000. Seeds 1..30, two runs of each twin per seed: every seed is stable and the twins are equal at every seed; seeds 13, 14, 29,
  30 give 19105, the other 26 give 19000. With the mutant `lazystub_dbl_skip` the job gives the same exits as without it at seeds 0, 1, 13, 14, 29
  (it does not affect this job at all).
* **Mechanism.** Seed 1 against seed 13 (eager): `fused HREFK+HLOAD tag checks` 516 vs 524, `barrier helper calls: table` 2 vs 1, sunk tables 75
  vs 78, trace code 606502 vs 621601 B, hot exits 117 vs 114. The node positions of the string keys (`fx`, `fy`, `x`, `y`, `__index`, ...) follow
  the seed, so the compiled guards differ (a key off its main position, a collision chain); `+exitstorm=7` fails every 7th guard execution of a
  trace, so a different guard count moves the storm phase and with it which exits are taken and which side traces fail to record. Seeds 1 / 13
  with `+verify` alone: identical counts (3905 exits, 17 side traces); `+exitstorm=7` without `+randra`: 25527 exits both, but 83 vs 82 side
  traces (also a twin mismatch); `+randra=5` is deterministic per compile and only changes which counters move. The other jobs of the test happen
  not to be seed-sensitive in their compared counters.
* **Cross-test leak.** `h2_newkey_programs`, `h5_sink_next_programs` and `p1_presize_programs` pinned the seed with
  `std::env::set_var("TR_CORE_SEED", "0")`, which is process-global: the children of any test of the same binary started after it (on parallel test
  threads) inherited seed 0. Whether `s1_lazystub_programs` ran with random or fixed seeds therefore depended on scheduling, which is why the rates
  in 32.6 varied (3 in 6 with P1, 1 in 6 without) and why runs of the single test fail more often than the full binary.
* **`lazystub_dbl_skip`** (`defer_stub` does not push the snapshot of a deferred frame-cell `mat_stub` into `Asm::dbl`): `Trace::dbl` is read only by
  the P3 chain re-order (`norm_count`, the `chain_off` re-base in `chain_check`, `trace_exits` for the re-based part; the re-based part times two is
  the same total, so `exits` itself is unchanged). None of the eight programs of the test re-orders a chain (`chainre` needs `chainn`, 65536 by
  default, and a chain of three callee side traces), so the mutant was invisible and was "caught" exactly when the flake fired: at the base, 4 of
  12 mutant runs failed, all on the `sinkf_side` twin assertion; 2 of 12 unmutated runs failed the same way.

### 33.2 Fix (tests only)

* `Job::seed(n)`: the child gets `TR_CORE_SEED=n` (set by `run_job`, no `set_var`); `compare_try` keeps one interpreter baseline per (program,
  seed). The three tests above pin seed 0 per job instead of `set_var`.
* `s1_lazystub_programs` pins seed 0 for every job, runs the storm job of `sinkf_side` additionally at seed 13 (the 19105 formation stays covered,
  deterministically), and compares the chain re-orders of the twins too (`chain_flushes`).
* New program `s1_dbl_chain` (`tests/tr_core/progs.rs`): the dispatch of `p3_chain` (12 callees whose hot set changes mid-run) with a sunk
  metatabled temporary live across the megamorphic call inside the inlined `execute`, so the callee-guard exits of the chain of side traces are
  deferred frame-cell `mat_stub`s; limits `chainre=95,chainn=1000,ncallee=1,chaindepth=0,maxside=64`, in the full mode matrix of the test. Twins
  (`always`, plain): 320603 exits, 1 re-order, 27 side traces. With the mutant (`lazystub=1` only; `lazystub=0` is unchanged): 241939 exits,
  5 re-orders, 51 side traces (the doubled counts cross `chainn` early). The same at 8 random seeds and in `on2` (320604 / 241940). The test asserts
  that the plain job re-orders at least one chain over built deferred stubs with frame stubs, so the detector cannot become vacuous unnoticed.

### 33.3 Pass rates (debug oracle-feature binary; each run = one invocation of the single test)

| | base a536f04 | this change |
|---|---|---|
| `s1_lazystub_programs` | 10 / 12 | 12 / 12 |
| `p1_presize_programs` | 12 / 12 | 12 / 12 |
| `s1_lazystub_programs` with `lazystub_dbl_skip` (caught) | 4 / 12 (flake only) | **12 / 12** (`s1_dbl_chain [always]` twins differ) |
| other `lazystub_*` mutants (resume_xmm, resume_v8, reloadk, site_rel_pos) | | 3 / 3 each (13-128 failing jobs each) |
| P1 mutants (image_check_skip, window_any, memo_any, memo_dunder, presize_no_free) | | 3 / 3 each (12-14 failing jobs each) |

`h2_newkey_programs` and `h5_sink_next_programs` (now seed per job) pass. Registry unchanged: **759** mutants (`scripts/gate-mutant-map.tsv`
regenerated for the moved test lines).

### 33.4 Gates

Light gate (changed-file plan, 60 mutants incl. the five `lazystub_*`, the P1 and the `newkey_*` / `sinkfast_*` mutants of the re-seeded tests): OK (882 s).
`gate-heavy.sh` (2026-11-21, at 41467fe, 4479 s): OK. light PASS; tests-5.5-oracle KNOWN (`tr_identity_pre::fetch_plus_insns_identity` only); tests-5.4-oracle
PASS; `oracle_options` PASS; jit-diff 1-200 (7 modes) PASS; mutant runner 751 ran, 751 caught, no survivors (`lazystub_dbl_skip` included).
`scripts/gate-mutant-times.tsv` refreshed by that run.

## 34. E1 / E2: up-recursion unrolling and region-scoped callee identity (stages 1 and 2 of docs/DESIGN_CALL_PROTOCOL.md; 2026-11-21, base f076f17)

Stages 1 and 2 of the call-protocol redesign, re-implemented from the design (sections 4.1, 4.2) and not from the prototype patch: the patch's E1 had
no policy (fixed `k = 3` regressed `permute` / `towers` / `list`) and its E2 skipped the identity chain everywhere (fails `k6_02_path_switch`). Knobs
(`RUSTYNAIL_JIT_TRACE_LIMITS`): `recunroll` (0 = off, 1 = adaptive, **default**; 2 = fixed `k`, no demotion: the tests' and measurements' twin),
`unrollk` (2..4, default 3), `unrollcode` (IR instructions of an unrolled family, default 400, 0 = no limit), `idfold` (0 / 1, default 1). The name `unroll` was taken (F4: inlined
recursion levels of loop traces), hence `recunroll`.

### 34.1 E1 as built

**Recording** (`record.rs`, `call_lua`). A function-entry recording (`fn_root`, not a side, not a continuation) of native recursion gets
`RecCfg::rec_unroll = k` from the driver (`cache::unroll_k`: the prototype is lazy-aware and not demoted). A self-call of the prototype is an
**unroll-chain call** when (a) it satisfies every condition of a native push (`push_ok`: not a tail call, not vararg, a fixed result count, `nargs ==
numparams`, and under E2's region invariant the record-time callee is the calling frame's closure), (b) every frame of the recording is an
activation of the root prototype (`uses == d + 1`) and each inlined one was itself entered by a chain call (`RecFrame::unr`), (c) the callee was read
from an upvalue (`callee_from_upvalue`: a `ULOAD`, or the root closure folded by E2: the shape rule of 34.2), (d) the depth is below `k` and below
the inlining limit. A chain call at depth `d < k - 1` is inlined like any call (F4 machinery: identity guard, frame, TR6b / TR6c room); at depth
`k - 1` it ends the recording with the native push (`end_rec_call` at depth `d`). A frame of the chain that returns or tail-calls before its own
self-call breaks the chain: the recording fails with `AbortReason::Unroll` (and so does any other abort after the root's self-call was inlined:
the `k = 1` recording would have ended at that call). `end_rec_call` records the chain in `RecCall::inl` (per inlined frame: the `CALL`'s `A` in
its caller, the caller's continuation address), guards the callee against frame `d`'s closure, and asks for `d + 1` spare `CallInfo` nodes and the
room of the final frame (the inlined frames' room was added by their own calls).

**Assembly** (`asm.rs`, `rec_call_tail_lazy`). After the writeback (every live slot of every frame, root-relative), the inlined frames become lazy
frames outermost first, exactly as `d` lazy pushes would have made them (`ci.next`, `func = caller func + A + 1`, the caller's `savedpc` = the
continuation, `SetCi`, `AddFrame`), then the final frame is pushed as before and control jumps to the loop top (or through the entry head's cell).
The eager push refuses an unrolled call (`incons`): only lazy-aware prototypes unroll (`driver_start`), so permute-like protos with loops never do.
Leaves inside the inlined frames are frameful exits; their side traces are S1 frame side traces with the inlined frames virtual (unchanged code).

### 34.2 The adaptive policy (criteria from the measurements of 34.6)

| criterion | rule | data behind it |
|---|---|---|
| lazy-aware only | `TrProto::unroll_k` returns 1 unless the prototype is lazy-aware (loop-free, one result count, not vararg, not C-entered) | the prototype's `permute` +37 % instructions: the recording reached the loop of an inlined level (`InnerLoop`) and the entry head went `Black`; permute is not lazy-aware: 0 % now |
| shape | only a callee read from an upvalue unrolls (`local function f .. f() ..`); method / field / global recursion does not | `towers` (method recursion `self:moveDisks`, lazy-aware) with fixed `k = 3`: +3.4 % instructions, **+36 % cycles**; L1i misses 0.73 -> 2.35 M, branch misses 0.30 -> 0.53 M, family code 80 -> 158 KB: every level repeats the 34-instruction lookup chain and the leaf side traces of every level inline `moveTopDisk` / `pushDisk` / `popDiskFrom`. With the rule: identical instruction count |
| chain break | a frame of the chain that returns before its self-call aborts at once (`Unroll`); the trace is k copies of the path from the entry to the first self-call (LuaJIT's up-recursion), never a leaf path baked into a level | the prototype's `list` +65 % (leaf paths of `tail` recorded into the levels); with the rule `list` +0.2 % |
| failure budget | an `Unroll` abort costs the head a flat wait of `UNROLL_WAIT + rand4` entries, no `k` / `kl` (no blacklisting); `UNROLL_FAILS` = 12 failures in a row lower `k` by one, below 2 the prototype is demoted for good; a success resets the count | fib: 62 % of the invocations that pass the root's self-call are too shallow for `k = 3` (`n` in {2, 3}); with 6 failures per level `fib 27` fell to `k = 2` in one run (0.62^6 = 5.6 %); 12 gives 0.3 % |
| code growth | an unrolled family (root + side traces) holding more than `unrollcode` = 400 IR instructions demotes the prototype; the family is flushed at its next hot exit through the handler (`unroll_doom_check`, the graveyard path every policy flush of the handler uses) and re-recorded with `k = 1`. IR instructions, not code bytes: bytes depend on whether stubs are deferred (`lazystub`), and a byte budget made the `lazystub=1/0` twins of `s1_lazystub_programs` form differently | family weights with `k = 3`: fib 61, sumrec 32, list 123, binary_trees 145; cd's `delete` 1385 (cd: +0.06 % instructions unrolled, neutral; 85 KB of code); towers (excluded by the shape rule) 150 KB of code, same order as cd |
| ineffective | an unrolled root flushed by the ineffective-trace rule demotes the prototype (the re-record is the `k = 1` trace) | safety net; not triggered by the measured set |

**A finding, not fixed here.** Flushing a lazy-aware function-entry family *at the end of a side trace's pipeline* (the first implementation of the
code-growth rule) corrupts the run: `awfy/cd 20` died with "attempt to call a nil value (method 'compareTo')" or a SIGSEGV in `TableRef::finish_set`,
and it does so with `unrollk=1` as well (a scratch hook that applied the rule to the `k = 1` family of `delete`), so it is not an E1 defect: some
state outside the trace table still refers to the family at that point. Flushing the same family from the exit handler (deferred to the
graveyard) is correct in every run. The rule therefore only marks the family (`Trace::unroll_doom`); recorded as a risk in 34.8.

### 34.3 E2 as built

**Decision** (`cache::idfold_decide`, once per prototype at its first recording): folding is on when the prototype is lazy-aware, has no installed
trace, the running closure reaches *itself* (the same object) through an upvalue `n` (`TrProto::idfold_uv`), and the prototype has no `SETUPVAL`
of its own (it would only trip the family rule); otherwise off for good. **Region invariant in force** (`idfold_cfg`): folding is on, or a folded
trace of the prototype is still installed (`TrProto::idfold_live`).

**Recording.** `GETUPVAL n` in a frame that runs the region closure (the root frame; a frame whose closure ref is the root's, i.e. identity-guarded)
whose record-time value is that closure, whose value address is outside the trace's frames, records as the closure's ref (`func_ref`): no `UREF`,
TR15 guard or `ULOAD`; `end_rec_call` drops the then trivial self-compare. Under the invariant every native push and every inlined chain frame is
guarded by identity with the calling frame's closure (`idfold_ident`: a push of another closure of the prototype is never made, the push guard is
never the prototype-only `EQREF`), so every frame of a region runs the closure its checked entry verified.

**Checked entries** (`asm.rs`). (a) Every outer entry of every trace of the family while the invariant is in force (where `entry_checks` stores
`lazy_root`: roots, preludes, link roots) runs `idfold_check` with the TR6 room checks: the running closure `C`, `v = C.upvals[n]->v`, `v - base`
unsigned `>= maxstacksize * 16` (TR15), `*v` is a Lua closure equal to `C`; a failure is a TR6 failure (the trace is not entered, tier 1 runs the
frame). (b) A return of the region root (`rec_ret_tail`, `r13 == lazy_root`): the caller, which becomes the root and may have been suspended while
foreign code ran, gets the same check in line after the pop; if it holds the return continues at the inner entry as before, else through the
continuation head's plain dispatch cell (an outer entry, the leave block's fast path). The in-line form matters: `sumrec` returns through its region
root at every level after a leaf exit (no side trace covers it): with the plain-cell form every such return was an outer entry, +26 % instructions;
with the in-line check +0.1 %.

**Family rule** (`pipeline`): while the invariant is in force, a trace of the family with a `USTORE` (any upvalue: `(closure, index)` pairs may
alias the self upvalue, P3d; an inlined callee's store counts) is refused (`Compile("idfold_ustore")`), folding goes off for good
(`idfold_ban`) and every family that holds a folded trace is doomed (flushed at its next hot exit, as in 34.2); once `idfold_live` is 0 traces with
upvalue stores install as before. The open-upvalue range (TR15) is part of the checks above.

### 34.4 Soundness

**E1.** No new kind of state: an unrolled trace is an inlined recursion (F4) followed by lazy pushes (C70).
* I1 (TR3): the chain frames are virtual inlined frames until the push; every exit before it rebuilds them from the snapshot `FrameLink`s (the
  existing `exit.rs` / `mat_stub` paths); S1 side traces keep them virtual.
* I3 (TR6b / TR6c): the entry checks cover the room of every inlined call and of the final frame and `d + 1` spare nodes, all or nothing: the trace
  never pushes a frame the interpreter's fast path would not push; when the room is short the trace exits before any push and tier 1 / the
  interpreter call one frame at a time, growing the stack or raising `stack overflow` at the reference depth (`e1_02`: cold and warm overflow depths
  equal, messages equal; `h7_04` near the limit). The post-push checks of the next iteration are the trace's own (`ir.extent`, `ir.depth = k`).
* I2 (TR5): one budget charge for the whole recorded path, exits refund per snapshot (unchanged); the unrolled path is a longer straight-line
  segment.
* I6 (L-INV): the `d` chain frames and the pushed one are pushed during the region by a trace of the family and are lazy; `lazy_leave`
  completes them like any lazy frame (`callstatus` from the `CALL`s' `C`, one value for a lazy-aware prototype; `savedpc` = the continuation of each
  inlined `CALL`, `func`, `top`, `kcache`), so errors, `debug.traceback` / `getinfo`, hooks and the GC only ever see complete frames (tracebacks at
  depth, `getinfo` from the bottom frame and errors at depths 1..40 in `e1_02` / `e1_03`).
* I5 (TR22): the loop top polls once per `k` calls; the inlined calls need no poll (asynchronous hooks may fire one iteration late, the argument of
  every inlined call). A count hook installed from inside the recursion and a hook installed by a finalizer: `e1_03`.
* I9 / I10: the chain frames register the root prototype (no new dependency); `RETF` is off for lazy prototypes (unchanged).
* Coroutines: no region spans a C function; yields at the leaves and in the middle of an unrolled recursion (`e1_03`).
* Policy state (failure counts, `unroll_off`, `unroll_doom`) only decides *what* is recorded or *when* a family is flushed, never how a trace runs.

**E2.** The fold assumes `C.upvals[n] == C` for the region closure `C` at every folded `GETUPVAL`.
* *Established* at every checked entry: an outer entry verifies it for the frame it enters, which becomes the region root (`lazy_root`), and so does
  a root return for the caller that becomes the root.
* *Every region frame runs `C`*: frames above the root were pushed in this region by family traces, each push (and each inlined chain frame) guarded
  by identity with the pushing frame's closure (`idfold_ident`), by induction `C`.
* *Nothing in a region can change it.* Code outside the family's traces (the main chunk's register store into an open upvalue, `SETUPVAL` in tier 1
  / the interpreter, `debug.setupvalue` / `upvaluejoin`, a C function, a finalizer, another coroutine) cannot run inside a region (I6: no Lua code
  outside the family's traces, no C function, no GC safepoint); a family trace cannot store an upvalue (family rule; the refused trace is never
  installed, and traces compiled before the ban held no store); slot stores of the family's traces write frames at or above the root and the root
  return writes one register of the caller: the self upvalue's value address is outside the root frame (TR15 at the check), not above it (open
  upvalues point into live frames, which at the check are at or below the root), and outside the caller's frame at a root return (the in-line check
  repeats TR15 for the new root); and a closure's own frames never hold its self upvalue (it is a local of the enclosing function).
* *Every way back into the family after foreign code is a checked entry*: outer entries (dispatch cells), root returns; inner entries (native push,
  native return of a frame above the root) are taken only by frames pushed in the current region. Exits leave the region (`lazy_leave`); a side
  trace attached to an exit continues the region and is a family trace (the rule and the fold apply to it).
* *Transitions*: the decision happens before the prototype's first trace; folding never turns on later; while a folded trace lives every new family
  trace keeps the checked entries, so a region started by an unfolded trace still verifies the invariant.
* The naive form (no checked entries) fails `k6_02_path_switch` and `e2_01` (the prototype's measurement: mutant `idfold_outer_unchecked`).

Lua-visible behaviour (design section 5): hooks and count hooks, the budget semantic (`instruction_count_identity`, `tr_identity`), stack overflow
depth and message, C-stack limit (native pushes are Lua-to-Lua calls), errors / tracebacks / `getinfo`, coroutines, GC and memory errors (nothing
allocates in a region that did not before) are unchanged by both elements for the reasons above; each has a program in 34.5.

### 34.5 Tests and mutants

New programs (`tests/vm`, also in the `vm_corpus` against the reference builds): `e1_01_unroll_shapes` (tree / linear / allocating / float
recursion, mutual recursion, tail recursion beside a non-tail self-call, vararg recursion, closures captured per level, method recursion,
Ackermann, data-dependent paths), `e1_02_unroll_depth_errors` (stack overflow: cold and warm depths equal, same message; recursion near the limit;
errors at depths 1..12 with traceback frames; `pcall` at every level; arithmetic errors on values from deep frames; `getinfo` from the bottom),
`e1_03_unroll_coro_hooks` (yields at the leaves and mid-recursion, a count hook and a call hook installed from inside, `traceback` at depth, a
finalizer, coroutines inside recursion), `e1_04_unroll_method_only` (towers: method recursion only), `e2_01_idfold_callee_switch` (13 cases: the
upvalue reassigned by the main chunk; replaced by `debug.setupvalue` from a leaf; `debug.upvaluejoin` onto another cell; a global recursion
replaced; many closures of one prototype; crossed self upvalues, linear and tree-shaped (the root-return case); `__call` and `__index` replaced;
upvalue stores in the family by an inlined callee, on the self upvalue, mid-recursion on a hot path; a field switched to another closure whose
upvalue names the first).

`tr_core` (every job compared with the interpreter): `e1_unroll_programs` (7 programs x `always` / `on2` x `recunroll` 0 / 1 / fixed k 2 / 3 / 4,
`+verify`; for fixed k 3 and 4: exit storm, budgets 3 / 17 / 200, GC stress 1 / 2 in both collector modes, stack-move stress; asserts unrolled
installs with fixed k and none with 0), `e1_unroll_policy_programs` (shape rule: no unrolled trace in `e1_04` with `recunroll=1`; code-growth rule
with `unrollcode=1`: demotions and doomed-family flushes; adaptive installs), `e2_idfold_programs` (7 programs incl. `k6_02_path_switch` x
`idfold` 0 / 1 alone and with unrolling, `+verify`, exit storms, budgets, GC stress 1 / 2 both modes, stack-move stress; asserts folded traces
and the family rule firing in `e2_01`). `tr_policy::e1_policy` (knobs, `unroll_k`, failure budget, degradation, demotion, the `Unroll` wait).
The emulator (`+emurun`) refuses native self-calls (unchanged), so it adds nothing here; `tr_identity` / `instruction_count_identity` / the
oracle run in the heavy gate over the corpus with the new defaults.

Mutants (registry 759 -> **773**): `h9_inl_push_skip`, `h9_inl_savedpc_off`, `h9_inl_bump_off`, `h9_maxci_short` (frames of the chain; caught by
`e1_unroll_programs`), `h9_fallback_never`, `h9_fail_penalised`, `h9_demote_ignored` (policy; `e1_policy`), `h9_code_rule_never`,
`h9_doom_never`, `h9_shape_any` (`e1_unroll_policy_programs`), `idfold_outer_unchecked` (the naive form), `idfold_rootret_inner`,
`idfold_ustore_family`, `idfold_push_proto` (`e2_idfold_programs`; `idfold_rootret_inner` survived the first run and is caught since case (13),
the tree-shaped crossed recursion, was added). Pinned to `recunroll=0`: `bc_sbump_programs` (its side-trace counts are those of the `k = 1`
formation the rule was measured on).

### 34.6 Measured (release `rn-cli`, `RUSTYNAIL_SEED=0`, `taskset -c 4`, instructions of the first run, cycles min of 5 (5.5 key set) / 3 (sanity set, 5.4); binaries at equal-length paths, one environment string of equal length in every column)

Columns: `base` = f076f17 (`b55`); the new binary with `recunroll=0,idfold=0` (`off`: the code of before; its cycle column is the layout
reference), `recunroll=1,idfold=0` (**E1**), `recunroll=0,idfold=1` (**E2** alone), `recunroll=1,idfold=1` (**E1 + E2**, the shipped default).
Deltas are against `off` (same binary: no layout effect). LuaJIT 2.1 from docs/DESIGN_CALL_PROTOCOL.md section 8 (same machine and method).

| program (5.5) | base | off | E1 | E2 | E1 + E2 | vs prototype (E1 / E1+E2) | LuaJIT | ratio cyc E1+E2 (was) |
|---|---|---|---|---|---|---|---|---|
| fib 32 | 703.6 / 113.5 | 703.6 / 114.9 | 451.5 / 73.0 (**-35.8 % / -36.5 %**) | 612.0 / 100.4 (-13.0 / -12.6 %) | **390.2 / 62.4 (-44.5 % / -45.7 %)** | 451.6 / 73.0; 410.9 / 65.2 | 227.0 / 42.9 | **1.45x** (2.65x) |
| binary_trees 15 | 3104.1 / 601.7 | 3104.1 / 616.7 | 2496.2 / 495.1 (-19.6 / -19.7 %) | 2938.8 / 577.2 (-5.3 / -6.4 %) | **2397.5 / 483.8 (-22.8 / -21.6 %)** | 2498 / 496.0; 2448 / 484.1 | 3083 / 864.3 | 0.56x (0.71x) |
| sumrec (linear) | 1055.8 / 232.2 | 1055.8 / 233.0 | 693.1 / 110.7 (-34.4 / -52.5 %) | 1057.2 / 184.7 (+0.1 / -20.7 %) | **659.8 / 110.0 (-37.5 / -52.8 %)** | 693.1 / 115.4; 702.4 / 116.1 | 244.4 / 85.7 | 1.28x (2.73x) |
| permute 300 | 692.7 / 122.9 | 692.7 / 120.7 | = / 120.4 | = / 120.3 | = / 121.0 | prototype fixed k: +37.4 % instr | 1462.7 / 31.3 (lottery) | |
| towers 100 | 353.1 / 85.4 | 353.2 / 86.2 | = / 86.6 | = / 85.9 | = / 85.6 | prototype fixed k: +4.4 % / +35.6 % | 205.9 / 44.9 | |
| list 300 | 342.4 / 81.8 | 342.5 / 81.4 | 343.1 / 81.9 (+0.2 %) | = / 81.5 | 343.1 / 81.9 (+0.2 % / +0.6 %) | prototype fixed k: +65.3 % | 270.4 / 76.9 | |
| cd 20 | 10716.2 / 2891.6 | 10712.4 / 2875.9 | 10731.7 / 2897.2 (+0.18 / +0.7 %) | 10721.2 / 2901.9 (+0.08 %) | 10735.7 / 2918.2 (+0.22 % / +1.5 %) | | | |
| queens, pqueens, richards, deltablue, havlak, raytrace, raytrace 40, nbody, awfy nbody, lua_lexer, classic_vector, dijkstra_routes, expr_lang, lru_cache, tremove_front | | | within 0.04 % instructions of `off` | | within 0.07 % (havlak +0.07 %, deltablue +0.04 %); cycles within the run-to-run spread (largest: raytrace +1.3 %, classic_vector +0.3 %, lua_lexer -0.3 %) | | | |

`base` against `off` (knob off, a different binary): instructions within 0.12 % (classic_vector +0.12 %, cd -0.03 %; the allocator addresses
behind pointer-keyed hashing move with the binary), cycles up to +5 % on nbody (layout: equal instructions).

**Sanity set** (44 programs: the 14 awfy programs, the 25 of `bench/programs`, the 5 apps; min of 3): only `programs/fib` (7780.7 -> 4287.8 M,
-44.9 %; 1243 -> 678 Mcyc), `programs/binary_trees` (7403.9 -> 5755.4, -22.3 %; 1583 -> 1254 Mcyc), `awfy/cd` (+0.22 %) and `awfy/list`
(+0.15 %) move by more than 0.1 % instructions; every other program is within 0.07 %. Cycle differences at equal instruction counts (same binary)
reach +3.5 % (`programs/closures`), +3.3 % (`awfy/storage`), +1.4 % (`programs/string_ops`): run-to-run spread of min-of-3 on paths that execute
the same instructions; nothing above +1 % instructions.

**Lua 5.4** (the design applies unchanged: the code is version-neutral; min of 3):

| program (5.4) | base | off | E1 | E1 + E2 |
|---|---|---|---|---|
| fib 32 | 703.6 / 113.5 | 703.6 / 114.1 | 451.5 / 72.6 (-35.8 %) | 390.1 / 62.2 (-44.6 % / -45.5 %) |
| binary_trees 15 | 3402.2 / 664.5 | 3402.2 / 675.5 | 2788.5 / 559.7 (-18.0 %) | 2689.9 / 542.1 (-20.9 % / -19.7 %) |
| sumrec | 1056.0 / 232.7 | 1056.0 / 233.3 | 693.3 / 112.1 | 660.0 / 113.3 (-37.5 % / -51.4 %) |
| cd 20 | 12539.2 / 3389.2 | 12535.7 / 3369.2 | 12246.2 / 3278.3 (-2.3 %) | 12255.0 / 3308.2 (-2.2 % / -1.8 %) |
| list 300 | 359.0 / 85.4 | 359.1 / 85.2 | 356.0 / 85.6 (-0.9 %) | 355.9 / 84.7 (-0.9 %) |
| permute, towers, queens, richards, deltablue, havlak, raytrace (40), nbody, lua_lexer, classic_vector, dijkstra_routes, expr_lang, lru_cache, tremove_front | | | within 0.04 % | within 0.1 % (havlak +0.05 %); raytrace cycles +2.8 % at equal instructions (spread) |

**Compile cost** (verbose stats, `off` -> E1 -> E1 + E2; code bytes = traces + deferred stubs, pipeline time in Mtsc):
fib 32: 7.4 -> 12.1 -> 12.9 KB, 0.34 -> 0.59 -> 0.54 Mtsc (3 -> 4 recordings); binary_trees: 16.0 -> 32.3 -> 33.7 KB, 0.50 -> 1.01 -> 0.90 Mtsc;
cd 20: 826 -> 891 -> 883 KB (+7 %), 68 -> 77 -> 76 recordings, pipeline 45.7 -> 36.1 -> 35.4 Mtsc (single runs: the pipeline time varies by
+-25 % between runs); expr_lang: identical (no lazy-aware recursion).

### 34.7 Gates

E1 commit (1f54faa, E2 machinery present with `idfold=0`): light gate all steps PASS (fmt, mutant map, clippy 5.5 / 5.5-jit / 5.4, rn-core
5.5 / 5.4, trace suites 5.5 / 5.4, `vm_corpus` + rn-cli, jit-diff 1-50); mutant group of the changed files 241 ran, 240 caught;
`idfold_rootret_inner` survived and is caught (re-run) since `e2_01` case (13).

E2 commit (`idfold=1` by default): light gate on the final tree: fmt, mutant map, clippy x3, rn-core x2, `vm_corpus`, jit-diff 1-50 PASS;
the trace suites 5.5 / 5.4 failed on `cloreuse_programs` only (the program swaps the self upvalue mid-run: with E2 its function-entry traces are
flushed by the entry-failure rule in that phase, 34.8; the "traces ran" assertion now uses `cloreuse=1,idfold=0`, outputs are still compared
with E2 on; the test passes on 5.5 and 5.4 after the change); the mutant group (241) left `cloreuse_guard_skip` alive for the same reason, caught
after the change (re-run of `cloreuse_*`, `idfold_*`, `h9_*`: 15 ran, 15 caught).

**Heavy gate: not run on these commits.** One heavy run of the final tree was started and stopped after ~4 minutes on the lead's instruction
(the lead believed the heavy run of 41467fe, section 33.4, covered this work; that run predates every source change of E1 / E2). In its place the
oracle-feature tests that E1 / E2 can affect were run on the final tree, on 5.5 and 5.4: `tr_core::instruction_count_identity` (the budget
semantic in every mode), `tr_core::{e1_unroll_programs, e1_unroll_policy_programs, e2_idfold_programs}` (oracle build), and the whole
`tr_identity` binary (`identity_budget_sweep`, `identity_options`, `identity_trace_modes`): all PASS on both versions (`instruction_count_identity` 212-223 s, `tr_identity` 5 of 5 in 452-477 s). Not run: `oracle_options` (sharded),
jit-diff 1-200 in seven modes, the full mutant runner (773): the lead's heavy gate should cover them.

### 34.8 Remaining risks

* **The pipeline-time flush of a lazy-aware family** (34.2 finding) is a latent defect outside E1 / E2: nothing ships that flushes at that point
  now, but any future policy that does (or an existing path that reaches it in a way not exercised here) can corrupt a run. Reproduction: apply the
  code-growth rule to every function-entry family (any prototype, `unrollk=1`) with an immediate `request_flush` at the end of `pipeline` and run
  `awfy/cd 20`.
* E1's policy is calibrated on the measured set: a heavy *upvalue* recursion whose family stays under 400 IR instructions but whose leaves are
  costly is not demoted (no such program in the 22 + 44 measured; cd's `delete` crosses the budget and is neutral either way).
* E2 changes the behaviour of a prototype whose self upvalue keeps changing between outer entries (k6_02-like): its entries fail the check, the
  entry-failure rule flushes and the head ends in tier 1 (sound; slower than before for that shape only).
* Global-function and method recursion get neither element (the shape rule; E5 is the design's answer for methods).
* The emulator does not run native self-calls (unchanged), so `+emurun` twins add no coverage for E1 / E2.

## 35. E4 / E5: lazy frames for recursion families with loops, region-scoped method identity (stages 3 and 4 of docs/DESIGN_CALL_PROTOCOL.md; 2026-11-22, base 9812416)

Stages 3 and 4 of the call-protocol redesign, built from the design (sections 4.4, 4.5) on top of the as-built E1 / E2 (section 34: same region,
checked entries, family store rule, demotion by flush through the handler). Knobs (`RUSTYNAIL_JIT_TRACE_LIMITS`): `lazyloop` (0 = off, **1 = on,
default**, 2 = on and E1 may unroll such prototypes), `midfold` (0 = off, **1 = method families with loops (E4 candidates), default**, 2 = every
lazy-aware method family). K5 (method-lookup CSE inside one trace, docs/STUDY_COVERAGE.md 5) was
**left out**: the existing CSE already shares the lookup prefix inside a trace (the second `SELF` of a trace re-emits only `HMISS` / `HREFK` /
`HLOAD` / `EQREF` of its own key: permute's traces 8-10 in the dump of 35.6), the remaining ops differ by key, and E5 removes the recursive key's
chain at the source; nothing measurable was left for K5 on the measured set.

### 35.1 E4 as built: which prototypes with loops become lazy-aware

**The naive form is a regression.** Dropping the loop condition of the lazy decision (`dispatch`: `env.backedges.is_empty()`) makes every
function with a loop and a function-entry head lazy-aware, recursive or not: `programs/heapsort` +101 % instructions, `programs/nbody` +13.4 %,
`awfy/nbody` +7.0 %, `programs/queens` +10.0 %, `awfy/storage` +2.5 %, `awfy/richards` +2.0 % (one binary, the knob on against off). A lazy
family pays at its links (always through the leave block: no guard-free second entry, no register carry) and at every leave (completion of the
chain); it gains only per native push and return. A family that never pushes natively, or whose regions end at every call, loses.

**The rule** (`cache::lazy_loop_rule`, `TrProto::lazy_loop_decide`). A native-recursion prototype with loops that satisfies every other condition
of the lazy decision (function-entry head, not vararg, not C-entered, one `CALL` result count) is a *candidate* (`TrProto::lazy_st = LAZY_CAND`).
The decision happens at the compile of its **first root trace**: lazy-aware (`LAZY_ON`: `TrProto::lazy`, every head's `lazy` and `rec` / `e2`
cells via `set_rec`) when that trace is its **function-entry trace ending in a native self-call** without `RETF` segments; eager for good
(`LAZY_OFF`) otherwise. A family whose first trace is a loop trace or a leaf path (queens: the column loop; storage: the leaf's fill loop) has
its regions end at every call (the leaves return through exits of loop-family side traces, which cannot return natively): with the rule both
are unchanged. Root recordings of an undecided candidate run as if it were lazy-aware (`driver_start`: no `RETF`), so the first trace never holds
a `RETF` that the decision would make unsound.

**RETF into a candidate** (`Rec::retf`, I9). The static superset `maybe_lazy_proto` stays for loop-free prototypes; for a prototype with loops
`cache::retf_into_loop_proto` decides dynamically: a lazy-aware one refuses, an undecided candidate is decided **eager for good** at that moment
(a `RETF` trace that pops into it may then be installed; it never becomes lazy-aware under it), a prototype without a dispatch table refuses
(it could still become a candidate). Effect: `heapsort` keeps its `RETF` (0.00 %).

**E1 on such prototypes** stays off by default (`unroll_k_loops`: only `lazyloop=2`); the method recursion permute has is excluded by E1's
shape rule anyway (section 34.2), and E5's receiver rule refuses pushes from inlined frames (35.4), so unrolling a method family needs receiver
provenance through inlined frames: not built. E2 is not applied to candidates either (its decision needs `lazy` at the first recording, which a
candidate does not have yet).

**Considered and dropped (measured, no gain).** `lazylink`: a side trace of a lazy family links like an eager one (guard-free second entry, G1's
room skip) when no lazy frame exists (`r13 == lazy_root`). It was built and measured: identical instruction counts on the key and the 44-program
sets (storage's cost is the push into a non-direct head, not the link), so it was removed. A handler shortcut (`lazy_leave` skipping the closure
lookup when the running frame is the root): no measurable change, removed.

### 35.2 E4 soundness

* **I6 (L-INV) for loop roots.** A loop trace of a lazy family is entered only at its outer entry, which stores `lazy_root := r13` like every root
  (`entry_checks`, unchanged; mutant `lazyloop_root_not_set` skips it for loop roots only, caught). Loop traces push no frames (`rec_call` ends a
  trace, `LOOP` ends another), so inside a loop trace the running frame is the region root; its back edge stays in the region (no Lua code, no
  safepoint: TR1 holds for every trace; class-A helpers exit before a GC step). Every exit completes the chain (`exit_entry` with `meta.lazy`,
  the leave blocks; unchanged code, the predicate only widened).
* **Pushes from continuation traces that crossed a loop back edge** (permute: the continuation after the in-loop self-call runs the swap, the
  `FORLOOP`, and pushes): the lazy push of C70 with the same `lazy_cs` (one `CALL` result count is part of the candidate rule), the same
  completion at leave.
* **I9 (RETF never pops into a maybe-lazy caller).** Static for loop-free prototypes, dynamic for prototypes with loops (above): a candidate
  becomes lazy-aware only at its first root compile and only if no `RETF` recording popped into it before (the query decides it eager); a
  lazy-aware one is refused. Mutant `lazyloop_retf_ok` (the dynamic refusal skipped) is caught by `tr_core::e4_lazyloop_programs`
  (`e4_04_lazy_loop_retf`; 5.5 only, RETF is 5.5 only).
* **The decision cannot change a running family.** It happens before the prototype's first trace is installed (`untraced`); afterwards the state
  is final. The heads' cells are recomputed (`set_rec`) at that point, when no trace of the prototype exists.
* **Lua-visible behaviour** (each with a program, `e4_02`): stack overflow at the same depth and with the same message cold and warm (a recursion
  inside a loop), recursion close to the limit, errors and tracebacks from depth, `pcall` at every level inside the loop, arithmetic errors on
  values from deep frames, yields at the leaves and in the middle of the loop, a count hook and a call hook installed from inside, `getinfo` /
  `traceback` at depth, a finalizer and collections in a recursion that allocates in its loops, a metatable changed during method recursion.

### 35.3 E4 measured

permute 300: 692.7 -> 659.6 M instructions (-4.8 %), cycles -5.9 % (one binary, `lazyloop` 1 against 0; the design's estimate was -5.6 %
instructions); every other program of the key set and of the 44-program sanity set: identical instruction count except raytrace -0.05 %.
Full tables: 35.6.

### 35.4 E5 as built: region-scoped method identity

**Decision** (`driver_start`, `midfold_key`, `cache::midfold_decide`, once per prototype at its first recording, searched only while pending):
on when the prototype is a candidate with loops (with `midfold=2` also any lazy-aware prototype), has no installed trace, **its register 0 is never written by an instruction** (`OP_PROPS[op].a && A == 0`, the
interpreter's "sets register A" bit) **and never captured by a closure it makes** (an upvalue descriptor `instack && idx == 0`), and a
`SELF A 0 K` of it (`K` a short-string constant) whose lookup on the running frame's register 0 — raw gets and `__index` links of tables only,
at most `MIDFOLD_DEPTH` tables — gives the running closure. The key's constant index is kept (`midfold_kidx`). **First-trace rule**
(`TrProto::midfold_first`): when the first root trace of the prototype compiles without a folded lookup, folding goes off for good (nothing
relies on it yet). **Loop families only by default**: on towers (loop-free method recursion, lazy-aware since C70) E5 folds but buys nothing
(+0.11 % instructions) and costs +2.6..3.0 % cycles (5.5 and 5.4, interleaved min-of-5 runs: the helper at every region start, towers' regions are
short); permute (a loop family, E4) gains -14.2 % / -16 %. Earlier forms on towers: helper at every root return +9.7 % instructions; first
trace without a fold (the first-trace rule) and the root-return cache below brought it to +0.1 % instructions, the cycles remained.

**Recording** (`Rec::midfold_self`). `SELF A 0 K` of the key in the root frame of the recording (`cur_depth == 0`), with register 0 still its entry
load (`slot0_entry`) and the record-time lookup giving the running closure, records as `R[A] = func_ref` (the running closure's `SLOAD`),
`R[A+1] = register 0`: no `HMISS` / metatable / `__index` / `HREFK` / `HLOAD` / `EQREF` chain (13 IR instructions in permute's entry trace).
`TraceIr::midfold` carries it through the optimiser (`opt_loop::run` copies it: a pass that rebuilds the IR must).

**Pushes under the invariant** (`midfold_ident`: folding on, or a folded trace installed). A native push from the root frame requires the callee
to be the running closure (identity, as E2's `idfold_ident`) **and its register 0 to be the pusher's own, unchanged register 0**
(`midfold_recv_ok`: slot-map refs, no loads). Otherwise the push is not made and **the recording ends at that call with an exit**
(`end_exit`): tier 1 makes the call and the callee's frame starts its own region through a checked entry. (Inlining such a call instead, the
path a refused push took first, met a defect of the inlined-recursion path under lazy regions that predates E5: 35.8.)

**Checked entries.** (a) Every outer entry of a family trace while the invariant is in force (`entry_checks`, where `lazy_root` is stored) calls
`cache::jit_tr_midfold_check(thread, ci, kidx)` (class N: reads only) with the room checks: register 0 of the frame is a table whose lookup of
the key gives the frame's closure; a failure is a TR6 failure (tier 1 runs the frame). A passing check stores the verified pair (receiver,
closure) into `ExitArea::mf_recv` / `mf_cl`. (b) A return of the region root (`rec_ret_tail`, `r13 == lazy_root`): the caller gets the check in
line: a table receiver and a Lua closure equal to `mf_recv` / `mf_cl` pass without a lookup (every frame of a method region shares the root's
receiver and closure, and a caller with the same receiver and closure has the same lookup result), otherwise the helper; failing, the return
goes through the continuation head's plain dispatch cell (an outer entry), as E2's.

**Family rule** (`pipeline_inner`, `midfold_stores_ok`). While the invariant is in force no trace of the family may: store into a hash part
through an `HREFK` of the key or of `__index`, or through an `HREFV` of a string key (an integer `HREFV` is fine), insert (`NEWREF`) the key or
`__index`, or call `setmetatable` (`LIB SETMT`). Array stores, integer-key sets (`TSET`), upvalue stores are fine (register 0 is not captured).
A refused trace bans folding (`midfold_ban`: off for good, folded families doomed and flushed at their next hot exit through the handler, as
E1 / E2). A folded trace of a family that turns out not lazy-aware (a candidate decided eager at that compile) is refused the same way.

### 35.5 E5 soundness

The fold assumes, for every frame `X` of a region that runs a folded trace, `lookup(X.R0, K) == X.closure`.

* *Established* at every checked entry: the outer entry verifies it for the frame it enters (the region root); a root return verifies it for the
  caller that becomes the root (the cache case: the caller has the same receiver object and the same closure as the region root, whose fact was
  verified at the region's start and is preserved, so the caller's holds).
* *Every frame of a region has the root's receiver and closure*: frames above the root were pushed in this region by family traces, each push
  from a recording's root frame with callee == running closure (identity) and register 0 == the pusher's unchanged entry register 0
  (`midfold_recv_ok`); by induction from the root. Inlined frames never push under the invariant (refused), side traces start in their parent's
  frame.
* *A frame's register 0 does not change while it lives*, except by foreign code: no instruction of the prototype writes it and no closure
  captures it (the decision), `debug.setlocal` is a C function (outside any region; the frame's next region entry is checked: `e5_02` swaps a
  suspended receiver with `setlocal`).
* *Nothing in a region changes a lookup of K*: the lookup depends on the receiver's hash entry for K, its metatable, that metatable's `__index`
  entry and the next table's K entry (and so on down the chain). Code outside the family cannot run in a region (I6). A family trace cannot store
  K or `__index` into any table, cannot store a string key it does not know, cannot set a metatable (family rule; key-based, so it holds for every
  table on every chain at once), and insertions of other keys only rehash (a rehash keeps the mapping). `rawset` is recorded as those stores.
  An `__index` function is never followed (the check and the decision require tables).
* *Every way into a region after foreign code is a checked entry*: outer entries, root returns. Inner entries (native push, native return of a
  frame above the root) and side traces continue a region without foreign code in between.
* *Transitions*: the decision happens before the first trace; folding never turns on later; the first-trace rule and bans only turn it off;
  while a folded trace lives every family trace keeps the checks and the push rule.

Lua-visible behaviour (design section 5; programs `e5_01`, `e5_02`): instances that override the method, a subclass through the same call
sites, the class method rebound by a leaf inside the region (the family rule fires), `setmetatable` on the receiver and `__index` replaced,
another receiver passed to the running closure (`f(self.peer, ..)`), another closure of the prototype called on the receiver, register 0
reassigned (the prototype is not folded), an `__index` function; stack overflow depth and message, errors and method tracebacks, `pcall` at
every level, yields, hooks installed from inside, `getinfo` / `getlocal` of the receiver at depth.

### 35.6 Measured (release `rn-cli`, one binary; `RUSTYNAIL_SEED=0`, `taskset -c 4`; instructions of the first run, cycles min of 5 (key set) / 3 (sanity set, 5.4); deltas against the same binary with `lazyloop=0,midfold=0` (E1 / E2 on))

**Lua 5.5** (cycles min of 5; one environment string of equal length in every column):

| program (5.5) | ref (E1+E2) instr M / cyc M | E4 | E5 | E4 + E5 |
|---|---|---|---|---|
| fib | 390.2 / 62.5 | 390.2 / 62.5 (+0.00 % / +0.0 %) | 390.2 / 62.4 (+0.00 % / -0.2 %) | 390.2 / 62.4 (+0.00 % / -0.2 %) |
| binary_trees | 2397.5 / 482.7 | 2397.5 / 480.8 (+0.00 % / -0.4 %) | 2397.5 / 480.9 (+0.00 % / -0.4 %) | 2397.5 / 477.4 (+0.00 % / -1.1 %) |
| sumrec | 659.8 / 112.8 | 659.8 / 110.5 (+0.00 % / -2.0 %) | 659.8 / 112.5 (+0.00 % / -0.3 %) | 659.8 / 108.4 (+0.00 % / -3.9 %) |
| permute | 692.7 / 120.7 | 659.6 / 113.4 (-4.78 % / -6.0 %) | 692.7 / 120.4 (+0.00 % / -0.2 %) | 594.5 / 101.5 (-14.18 % / -15.9 %) |
| towers | 353.3 / 86.3 | 353.3 / 85.9 (+0.00 % / -0.5 %) | 353.3 / 86.0 (+0.00 % / -0.3 %) | 353.3 / 86.0 (+0.00 % / -0.3 %) |
| list | 343.1 / 82.2 | 343.1 / 82.2 (+0.00 % / +0.0 %) | 343.2 / 82.1 (+0.03 % / -0.1 %) | 343.2 / 82.2 (+0.03 % / +0.0 %) |
| queens | 511.5 / 95.6 | 511.5 / 95.6 (+0.00 % / +0.0 %) | 511.6 / 95.7 (+0.02 % / +0.1 %) | 511.6 / 95.7 (+0.02 % / +0.1 %) |
| pqueens | 5391.4 / 1849.1 | 5391.4 / 1846.9 (+0.00 % / -0.1 %) | 5391.4 / 1848.9 (+0.00 % / -0.0 %) | 5391.4 / 1845.3 (+0.00 % / -0.2 %) |
| richards | 176.8 / 59.8 | 176.8 / 59.6 (+0.00 % / -0.3 %) | 176.9 / 59.6 (+0.06 % / -0.3 %) | 176.9 / 59.5 (+0.06 % / -0.5 %) |
| deltablue | 323.6 / 108.0 | 323.6 / 107.6 (+0.00 % / -0.4 %) | 323.8 / 107.8 (+0.06 % / -0.2 %) | 323.8 / 108.6 (+0.06 % / +0.6 %) |
| havlak | 515.8 / 160.1 | 515.7 / 160.2 (-0.02 % / +0.1 %) | 515.8 / 158.2 (+0.00 % / -1.2 %) | 516.0 / 158.4 (+0.04 % / -1.1 %) |
| cd | 10736.4 / 2944.8 | 10736.2 / 2929.0 (-0.00 % / -0.5 %) | 10736.5 / 2944.0 (+0.00 % / -0.0 %) | 10736.6 / 2930.4 (+0.00 % / -0.5 %) |
| raytrace | 365.7 / 113.5 | 365.6 / 112.8 (-0.03 % / -0.6 %) | 365.6 / 114.2 (-0.03 % / +0.6 %) | 365.6 / 113.5 (-0.03 % / +0.0 %) |
| raytrace40 | 4601.7 / 1173.5 | 4601.7 / 1162.0 (+0.00 % / -1.0 %) | 4601.8 / 1177.6 (+0.00 % / +0.3 %) | 4601.5 / 1169.5 (-0.00 % / -0.3 %) |

**Lua 5.4** (cycles min of 3; measured before the loop-only default of `midfold`, so its E5 column is `midfold=1` folding every lazy-aware method
family: towers +0.11 % instructions / +2.5 % cycles there, which is what made `midfold=1` loop-only; E4 + E5 on permute -13.7 % / -14.0 %):

| program (5.4) | ref (E1+E2) instr M / cyc M | E4 | E5 | E4 + E5 |
|---|---|---|---|---|
| fib | 390.2 / 61.9 | 390.2 / 62.1 (+0.00 % / +0.3 %) | 390.2 / 62.3 (+0.00 % / +0.6 %) | 390.2 / 61.9 (+0.00 % / +0.0 %) |
| binary_trees | 2689.9 / 550.7 | 2689.9 / 548.3 (+0.00 % / -0.4 %) | 2689.9 / 563.1 (+0.00 % / +2.3 %) | 2689.9 / 549.6 (+0.00 % / -0.2 %) |
| sumrec | 660.0 / 114.2 | 660.0 / 119.7 (+0.00 % / +4.8 %) | 660.0 / 109.4 (+0.00 % / -4.2 %) | 660.0 / 108.7 (+0.00 % / -4.8 %) |
| permute | 712.7 / 121.0 | 679.7 / 114.7 (-4.63 % / -5.2 %) | 712.7 / 121.0 (+0.00 % / +0.0 %) | 614.9 / 104.1 (-13.72 % / -14.0 %) |
| towers | 358.5 / 87.0 | 358.5 / 87.0 (+0.00 % / +0.0 %) | 358.9 / 89.2 (+0.11 % / +2.5 %) | 358.9 / 89.0 (+0.11 % / +2.3 %) |
| list | 356.5 / 85.8 | 356.5 / 85.5 (+0.00 % / -0.3 %) | 356.5 / 86.1 (+0.00 % / +0.3 %) | 356.5 / 85.8 (+0.00 % / +0.0 %) |
| queens | 430.4 / 80.4 | 430.4 / 80.8 (+0.00 % / +0.5 %) | 430.4 / 80.5 (+0.00 % / +0.1 %) | 430.4 / 80.6 (+0.00 % / +0.2 %) |
| pqueens | 5878.8 / 1959.6 | 5878.8 / 1968.7 (+0.00 % / +0.5 %) | 5878.8 / 1971.1 (+0.00 % / +0.6 %) | 5878.8 / 1961.1 (+0.00 % / +0.1 %) |
| richards | 177.6 / 59.9 | 177.6 / 59.8 (+0.00 % / -0.2 %) | 177.6 / 59.9 (+0.00 % / +0.0 %) | 177.7 / 59.2 (+0.06 % / -1.2 %) |
| deltablue | 348.4 / 118.6 | 348.4 / 118.6 (+0.00 % / +0.0 %) | 349.0 / 119.6 (+0.17 % / +0.8 %) | 349.0 / 117.7 (+0.17 % / -0.8 %) |
| havlak | 576.2 / 177.1 | 576.2 / 178.2 (+0.00 % / +0.6 %) | 576.6 / 180.2 (+0.07 % / +1.8 %) | 576.8 / 172.5 (+0.10 % / -2.6 %) |
| cd | 12255.0 / 3310.2 | 12255.0 / 3345.1 (+0.00 % / +1.1 %) | 12255.7 / 3354.1 (+0.01 % / +1.3 %) | 12255.8 / 3328.5 (+0.01 % / +0.6 %) |
| raytrace | 367.3 / 117.7 | 367.2 / 117.9 (-0.03 % / +0.2 %) | 367.4 / 118.0 (+0.03 % / +0.3 %) | 367.3 / 118.4 (+0.00 % / +0.6 %) |
| raytrace40 | 4510.0 / 1198.7 | 4509.9 / 1221.1 (-0.00 % / +1.9 %) | 4510.1 / 1193.1 (+0.00 % / -0.5 %) | 4510.1 / 1199.3 (+0.00 % / +0.1 %) |

**Sanity set** (the 44 programs: 14 awfy, 25 `bench/programs`, 5 apps; min of 3; default against `lazyloop=0,midfold=0`): `awfy/permute`
235.3 -> 202.5 M instructions (-13.9 %), 43.6 -> 37.0 Mcyc (-15.1 %); every other program within 0.09 % instructions (`awfy/deltablue` +0.09 %,
`awfy/richards` +0.06 %, `awfy/towers` +0.06 %: the decision's key search at the first recording of each prototype; the rest 0.00 %). Cycle
differences above 2 % occur only at equal instruction counts and are spread: `awfy/havlak` +6.2 % / `awfy/storage` +2.8 % in the batch run,
re-measured interleaved (min of 5, two rounds): havlak 156.4 / 159.5 against 157.7 / 161.6 Mcyc, storage 100.0 / 100.5 against 101.1 / 101.1;
`programs/matmul` -4.1 %. No program above +1 % instructions or +2 % cycles.

**Per stage.** E4 alone: permute -4.8 % instructions / -6.0 % cycles (5.4: -4.6 % / -5.2 %), nothing else moves. E5 alone (`lazyloop=0`):
nothing (permute is folded only once it is lazy-aware: E5 needs E4's region). E4 + E5: permute **-14.2 % / -15.9 %** (5.4: -13.7 % / -14.0 %);
the design's estimates were -5.6 % (E4) and -13.5 % (E5). towers: 0 (E5 is off for loop-free method families by default; with it on, +2.6 %
cycles: 35.4). E1 on loop families (`lazyloop=2`): no program changes (permute's method recursion is excluded by E1's shape rule), so it stays
off. Lua 5.4 permute absolute: 712.7 -> 614.9 M instructions, 121.0 -> 104.1 Mcyc.

### 35.7 Tests, mutants, gates

New programs (`tests/vm`, also in `vm_corpus` against the reference builds, identical on 5.5 and 5.4): `e4_01_lazy_loop_shapes`,
`e4_02_lazy_loop_errors`, `e4_03_lazy_loop_policy`, `e4_04_lazy_loop_retf`, `e5_01_method_identity`, `e5_02_method_errors`. `tr_core`:
`e4_lazyloop_programs` (5 programs incl. `k6_02_path_switch`, `deep_recursion` x `lazyloop` 0 / 1 / 2, `+verify`; for 1: exit storm, budgets
3 / 17 / 200, GC stress 1 / 2 in both collector modes, stack-move stress; asserts lazy-aware candidates with the knob on and none off),
`e5_midfold_programs` (`e5_01`, `e5_02`, `e1_04` (towers), `e4_01` x `midfold` 0 / 1, `+verify`; for 1 the same stresses; asserts folded traces
and the family rule firing in `e5_01`); `tr_policy::e4_policy` (knob, decision rule, state transitions, the RETF query, the unroll rule),
`tr_policy::e5_policy` (knob, first-trace rule). `cp1_leaf_programs` asserts its repair counter with `lazyloop=0` (its permute-shaped program is
lazy-aware with E4 in 5.4 `on2`; outputs compared in both).

Mutants (registry 773 -> **784**): `lazyloop_retf_ok`, `lazyloop_decide_any`, `lazyloop_root_not_set` (all caught), `midfold_outer_unchecked`,
`midfold_check_true`, `midfold_store_family`, `midfold_recv_any`, `midfold_push_proto`, `midfold_first_keep` (caught), **`midfold_reg0_any` and
`midfold_rootret_inner` survive** (35.8).

Gates (light gate per commit, nothing heavier, by instruction): E4 commit 20b8121: all steps PASS except `tests-5.4-trace` (`cp1_leaf_programs`, a
stat assertion: fixed, re-run 5.5 / 5.4 PASS); mutant group (96 mutants incl. `lazyloop_*`, `idfold_*`, `centry_*`) PASS. E5 commit 991838d: all
steps PASS except the trace suites (`exit_area_layout_is_frozen`, the two new area words: fixed, re-run PASS) and the mutant group (the two
survivors of 35.8). The policy commit (loop-only `midfold`, pending-only key search, these docs): light gate as reported in its message. Also
run: `gate-mutants.sh --only lazyloop_` (3 / 3), `--only midfold_` (5.5: 6 / 8), `--only lazyloop_,midfold_ --version 5.4` (8 / 10,
`lazyloop_retf_ok` is 5.5-only); `tr_core::{e4_lazyloop_programs, e5_midfold_programs, cp1_*}` and `tr_policy::{e4_policy, e5_policy}` on 5.5
and 5.4. **Not run** (by instruction): the heavy gate, the oracle-feature suites (`tr_identity`, `instruction_count_identity`,
`oracle_options`), jit-diff 1-200 in seven modes, the full mutant runner.

### 35.8 Findings and remaining risks

* **The 34.8 hazard did not reproduce on this base.** A scratch hook that flushed every lazy-aware family above 400 IR instructions (and a
  variant flushing at its first side trace) at the end of a side trace's pipeline ran `awfy/cd 20` in `on` / `always`, seeds 0-3,
  `unrollk=1` / `recunroll` 0 / 1 / 2, and the 213 `tests/vm` programs in `on` / `always`: no difference, no crash. The deferred doom stays (E5's
  ban uses it too); the hazard remains documented, root cause unknown.
* **A pre-existing defect of the lazy chain, found by the new programs** (**fixed since, root cause and fix in 36.1**; the text below is the
  report as it stood). With every new knob off (`lazyloop=0,midfold=0,recunroll=0,
  idfold=0`) a debug build fails `debug_assert!(ci.kcache == ...)` in `run_mode`: the interpreter returns into frames whose `callstatus` and
  `kcache` are 0 (fresh `CallInfo` nodes of lazy frames never completed). Reproductions: (1) the two method recursions of `e5_01` cases (3)
  and (9) together under `RUSTYNAIL_GC_STRESS=1` or `2`, `RUSTYNAIL_JIT_TRACE=always` (`lazyci=0` or `recany=0` make it pass; the GC-stress check
  of every frame below `thread.ci` at each collector step never fires, so the incomplete frames appear between collections); (2) any self-call
  whose native push is refused and that is inlined instead, with a count budget of 17 (`e5_01` case (7) with the push refused): the caller
  resumes with the closure where the call's result belongs (needs `lazyci`, `recany` and `unroll`). The Rust completion helper's "one
  prototype" assertion does not fire, so it is not a walk over frames of another prototype. E5 avoids (2) by ending the recording at a refused
  push; the `e5_01` twin skips GC stress 1 in `always` mode for (1). Release builds do not check `kcache`; a frame whose `kcache` is 0 and whose
  function reads a constant would crash.
* **Two E5 mutants survive** (**both caught since, 36.2**: `e5_01` cases (13) and (14)). `midfold_reg0_any` (the decision ignores writes of register 0): the record-time rules (`slot0_entry`, the push
  receiver rule) already keep a trace from folding or pushing after its own write of register 0, and outer entries check the current register
  0; only a side trace that starts after such a write could fold on the new receiver, and `e5_01` case (11) did not produce one.
  `midfold_rootret_inner` (no check at root returns): needs a native root return into a caller suspended across foreign code with a receiver
  whose lookup changed; since refused pushes end the recording, `e5_01` case (12) did not produce one. Both rules stay (they are what the
  soundness argument uses); the programs that would catch them are open work.
* The decision rules are formation-dependent: E4's rule looks at which root trace compiles first (in the tests' `always` mode queens' entry
  trace compiles first and is lazy-aware), E5's first-trace rule likewise. Policy only: both orders are sound.
* E5's checked entries call a Rust helper at every outer entry (a family with frequent outer entries pays ~80 instructions each); root returns
  usually take the in-line cache.

## 36. The lazy-chain defect of 35.8, the two E5 survivors, E6: direct exit-to-side links for recursion families (stage 5 of docs/DESIGN_CALL_PROTOCOL.md; 2026-11-22, base 7b121ea)

### 36.1 The lazy-chain defect (35.8): root cause and fix

**Root cause.** A side trace of a lazy-aware family that is entered by an exit of its parent runs *inside the parent's region*: the frames
the region pushed natively above `lazy_root` are still lazy (only `func` and the caller's `savedpc` written). A frameless side trace keeps
its own TR6 room checks when the frames it inlines need more stack or more spare `CallInfo` nodes than its parent's entry checks promised
(`no_room_checks` is false when `ir.extent > max_extent` or `ir.depth > max_ci`, CP2). A failing check took the trace's common `entry_fail`
path: the entry-failure bookkeeping (nothing for a side trace: `head == 0`) and `jmp pcl[H]`, the tier-1 label of the side's start pc,
**without completing the lazy chain**. Tier 1 then ran in a lazy frame, made calls on top of it and returned into the frames below it: the
interpreter's `debug_assert!(ci.kcache == ...)` in `run_mode` saw `kcache == 0` / `callstatus == 0`.

Every other way from a lazy region to tier 1 already completed the chain: exits (the handler's `lazy_leave`, the in-line leave of the
inline stubs and of `mat_stub`), links (`emit_link`: the leave block), the native push's `rec_fail`, the null `rec` cell of
`lazy_rec_jump`. The side trace's entry failure was the one path that did not. Found with temporary probes (a chain walk at every handler
entry and exit, at every interpreter entry of compiled code, and a `CallH` probe in every leave block and stub tail): the last transfer
before the first incomplete frame was always the entry of side trace `e5_01`'s case (9) inlines (`W2:hop` twice, `depth = 2`), and its
disassembly shows the TR6b check (`ci.next.next == 0`) failing straight to the tier-1 label.

**Why only under GC stress, and only some programs.** (1) The TR6b check fails when the `CallInfo` list has fewer spare nodes than the side
trace's inlined frames need, which happens after `shrink_ci` (every collection frees half of the unused nodes); GC stress collects at
every safepoint. (2) A lazy frame is incomplete only *visibly* when its `CallInfo` node is fresh: a reused node keeps the `kcache` /
`callstatus` of the last complete frame it held, which for a recursion family is usually the same prototype with the same call site, so
the stale values happen to be right. After `shrink_ci` the nodes are freshly allocated (zeroed) and the missing completion shows. In a
release build (no `kcache` assertion) a frame on a reused node of *another* prototype would run with that prototype's constants. (3) The
second trigger of 35.8 (a refused push inlined instead, count budget 17, `e5_01` case (7)) is the same path: with E5's sidestep bypassed
(scratch build) it reproduces before the fix and passes after it.

**Fix** (`asm.rs`, `stubs_and_tail`): for a lazy-aware trace entered inside a region (`Cx::lazy_region_side`: a side trace that is not a
prelude and not a link root; those two are outer entries, reached from a dispatch cell with every frame complete, and store `lazy_root`
themselves), the entry failure continues through the leave block: `V6 = pcl[H]`, `exit::lazy_leave` (in line when the trace knows the
family's constants, else the helper), then `jmp V6`. `lazy_root` is valid there: the parent's region root (entered through an inline stub's
cell) or the running frame (entered after the handler's `lazy_leave`, the leave's fast path). Registers are dead at an entry failure (no
carry for lazy families), the budget word is the stub's. Mutant `lazy_sidefail_no_leave` (the old path).

**The E5 sidestep** (35.4: a refused push ends the recording with an exit instead of inlining the call) is no longer needed for soundness;
it stays (the instruction to keep it unless measurements say otherwise; it is also the cheaper formation for those calls: no inlined
recursion levels that can only exit).

**Detector.** `tests/vm/e6_00_lazy_side_entry.lua` (cases (3) and (9) of `e5_01` together, the same with collections and a deeper inlined
callee, a function recursion whose rare path calls a helper recursion, a tree whose leaves call a method on a peer) and
`tr_core::lz_side_entry_programs`: `e6_00` and `e5_01` x `always` / `on2` x {shipped knobs, every E-knob off, `lazypoison=1`,
`lazypoison=1,midfold=2`} x GC stress 1 / 2 in both collector modes, plus `+verify` and budget 17. Without the fix `e6_00` fails in
`always` mode under GC stress 1 and 2 (generational mode, debug `rnlua`: the `kcache` assertion), deterministically (`always` mode,
pinned seed); it passes in every job with it, on 5.5 and 5.4. The `e5_midfold_programs` twin runs its GC-stress-1 `always` job for
`e5_01` again (it skipped it because of this defect).

**New test knob `lazypoison`** (default 0, never a process default): a lazy push (and E1's chain frames) also stores `kcache = 0` and
`callstatus = 0` into the frame it pushes. Every completion writes both, so with the knob an uncompleted lazy frame is visible in a reused
`CallInfo` node too (point (2) above no longer hides a missing completion). The 224 programs of `tests/vm` and `tests/lazyci` in `always` /
`on` with `lazypoison=1` (debug `rnlua`, against the interpreter): no difference.

Mutants: registry 784 -> 785 (`lazy_sidefail_no_leave`, caught by `lz_side_entry_programs` on 5.5 and 5.4). Gate (light, commit 91bd3f7): fmt, clippy x3,
core x2, `vm_corpus`, jit-diff 1-50 PASS; the trace suites and the mutant map failed on the registry count / stale map (fixed, re-run PASS); mutant
group 124 ran, 122 caught (the two survivors of 35.8, caught by 36.2).

### 36.2 The two E5 survivors of 35.8

Both are observable; the programs that catch them needed a formation the earlier cases did not produce.

* **`midfold_reg0_any`** (the decision ignores writes of register 0). `e5_01` case (13): `Z:z` writes `self = self.nx` on the `n == 3`
  path and then tests `self.val`. The side trace of the write path does not fold (its register 0 is not the entry load); its exit at the
  `val` test starts a side trace *after* the write, whose entry register 0 is the new receiver: recorded while the new receivers are `Z`
  instances, the record-time lookup gives `Z.z`, the `SELF` folds and the push passes the receiver rule. Later the same side trace is
  entered with `Z2` instances (nothing on its path guards their class), so a fold calls `Z.z` on them. The outer calls use depth 9, so that
  the first root trace (which the first-trace rule needs to fold) is recorded on a path without the write; the receivers alternate their
  `val` so that whichever branch the write path records, the side trace of the other branch exists before the `Z2` phase. With the
  mutant: output differs in `always` and `on2` (`reg0write` 17835 instead of 32640); without it the prototype is never folded.
* **`midfold_rootret_inner`** (no check at root returns). Case (12) never folded: its `pcall` statement wants 0 results and the
  self-calls 1 (mixed result counts: the prototype is not lazy-aware), and `self:r(n - 1)` passes 2 arguments to a 3-parameter function
  (no native push). Case (14) is (12) with every call passing all three arguments and wanting one result, and a first phase in which the
  peer calls run without a victim (the continuation after the peer call gets its trace and becomes a native-return site). In the second
  phase the peer changes the class of the waiting receiver with `pcall(setmetatable, victim, RR2)` (foreign code, outside every region),
  then returns natively as the root of its region into that receiver's continuation trace, whose folded `self:r` must not run: the check
  fails (the receiver's lookup now gives `RR2.r`) and the return continues through the head's plain dispatch cell. With the mutant: output
  differs in `on2` (`rootret2` 4298 instead of 4900).

Both rules were right; the mutants model real failures and stay. `gate-mutants.sh --only
midfold_,lazy_sidefail`: 9 ran, 9 caught on 5.5 and on 5.4.

### 36.3 E6 as built: direct exit-to-side links (`dlink` bit 0)

Knob `dlink` (`RUSTYNAIL_JIT_TRACE_LIMITS`): bit 0 = exit links, bit 1 = the budget register across native transfers (36.4); 0 = off,
**3 = both, default** (`DLINK_PROCESS`).

**What a linked exit cost.** The stub of an exit of a lazy-aware family with a side trace attached (an inline stub, assembled at the first exit
since S1) ran per exit: the exit counter (`mov r7, &ctr; cmp [r7], hot-1; je divert; add [r7], 1`), the slot stores of the exit map, the budget
(`mov r7, vb; add r7, refund; mov [cnt], r7`), the cell (`mov r7, &cell; mov r7, [r7]`; a frameless exit of a lazy family also `mov r6, land;
cmp r7, r6; je leave`), `jmp r7`; the side trace then reloaded the budget word (`mov vb, [cnt]`). A frame-cell exit (an exit inside inlined
frames: the leaves of an unrolled E1 trace) ran the same through `mat_stub`'s attached path (`cmp cell, 0`, the counter `+2`).

**The link.** A side trace attached to such an exit is assembled with a *link entry* (`asm::assemble_dl`, `DlinkIn`): the parent exit's slot
stores (the slot part of the parent's inline stub, `exit::inline_stub_ops_slots` over the parent's `ExitSnap`: the parent's registers hold the
exit map when its guard branches there), `add vb, refund` (the budget stays in `ra::BUDGET_GPR`), `jmp` behind the plain entry's budget load.
At the install of the side trace (`cache::install` -> `dlink_attach`) every branch of the parent to that exit is re-pointed at the link entry:
the exit's patchable `nop` (S1: the stub label; `jmp stub` once the stub is assembled) becomes `jmp link`, and every `jmp` / `jcc rel32` of the
parent that branches to the `nop` or to the assembled stub (`DeferInfo::sites`, the list S1 patches) branches to the link. The bytes before
the patch are kept with the side trace (`Trace::dl_saved`) and written back when the side trace is flushed while its parent lives
(`flush_now`), before its block can be released. Per linked exit: the stores, one `add`, one `jmp` (the guard's branch lands in the side
trace's block directly).

**Which exits** (`DeferInfo::dl_ok`, decided by the parent's assembly; checked again by `cache::dlink_source` at the side trace's compile):
the parent is lazy-aware (`ExitMeta::lazy`) and keeps the budget in `BUDGET_GPR`; the exit's stub is deferred (S1, `lazystub=1`, the default:
without it nothing links) and is an inline stub that continues through its cell (frameless, or a frame-cell exit with plain frames); it is
counted only to find its hot count (not a pre-roll site, not a chain site, not an entry check); no sunk tables, no `top`, no `RETF` segment
pops. The side trace has no room checks of its own (CP2 for frameless side traces, S1 for frameful ones: R2's own room checks of a frameful side
trace come behind the budget load and fail through its start snapshot, whose stub stores the register), no carried loads (R1 is off for lazy
families), is not a prelude; nothing under `+emurun` / `+emuverify`. Not for families that are not lazy-aware (queens, richards and the other
loop programs: their exits keep the stubs; R1 covers non-lazy side traces with carried values).

### 36.4 E6 as built: the budget register across native transfers (`dlink` bit 1)

A native push (`rec_call_tail_lazy`, through the entry head's `rec` cell) and a native return (`rec_ret_tail`, through the continuation head's
`rec` cell) stored the budget register into the word before `lazy_rec_jump`. The inner entry they jump to never reloads it (it joins behind the
plain entry's budget load and continues with the register, P-M3), so the store served only the null-cell path, which leaves (the leave block,
then the head's dispatch cell, tier 1 or an outer entry: both read the word). With bit 1 the store moved onto that path (`lazy_rec_jump`, behind
its `cmp cell, 0`); the root return with an E2 / E5 check keeps its own store (the check's helper clobbers the register, the code reloads it
from the word); `rec_fail` (an inner entry's room check failing) stores it as before.

### 36.5 Soundness

* **Same state, same destination.** A linked exit continues exactly where the stub would have continued: the cell of an exit with a side trace
  attached holds that side trace's entry (TR8: install sets it; it changes only by a flush of the side trace, which unlinks first, below). On
  entry to the side trace's code behind its budget load, the stack holds the exit map (the link entry made the stub's slot stores, from the same
  registers, with the same tag elision) and the budget register holds `vb + refund`, the value the plain entry would have loaded from the word the
  stub stored. Everything behind that point is the side trace's unchanged code. The side trace's plain entry before its budget load does nothing
  else for an in-region side trace without room checks (no bookkeeping, no `lazy_root` store: 36.1's `lazy_region_side`).
* **What the link skips.** (1) The exit counter: it finds the hot count, at which the side trace was attached; afterwards the stub's equality test
  never fires again (pre-roll and chain sites, whose counters drive later policy, are not linked). A linked exit is not counted, like R1's carry
  path: the flush statistics and `exits` show only the other exits. (2) The budget word store and reload: the word is not read between the stub
  and the side trace's load (no helper, no exit in between); every later reader (an exit stub, a link, a leave, the handler) gets the register
  stored as before. (3) The cell load / compare: the cell's value is known (above).
* **TR5 (exact charge / refund).** On the link path `vb' = vb + refund - n_side` after the side trace's first `BUDGET`; on the stub path the word
  is `vb + refund` and the side trace charges `n_side` from it: equal. A budget exit of that `BUDGET` restores `vb' + n_side = vb + refund`, as on
  the stub path. Count hooks see the same counts (`e6_01` case (6)); with the oracle feature `e6_dlink_programs` checks the instruction-count
  identity at budgets none / 3 / 17 (mutant `dlink_refund_off`: the refund off by one).
* **Patching lifetime.** The patched bytes are 5-byte `jmp`s over the `nop` and 4-byte `rel32` fields of the parent's branches, all inside the
  parent's block (one chunk: `CodeArena::patch` flips the pages RW and back, never RWX). The patch is made at the install (from the recorder: no
  trace code is running, a trace never calls Lua, TR1) and undone at the side trace's flush while the parent's slot is live (a flush inside an exit
  handler of the parent returns into its shared exit routine, which is not patched; flushes inside the side trace's own handler are deferred, TR18).
  When the parent goes first (its family is flushed; its kids follow in `flush_tree`), its slot is stale and nothing is written: its block is
  never entered again. The unpatch happens before `retire` can free the side trace's block, so no branch of a live trace points into freed code;
  a failing unpatch is a fail stop (the block would otherwise be freed under the branches). S1's own patch of the exit (`defer_build`) cannot run
  while the exit is linked: it is reached only through the generic stub, which a linked exit does not reach; after an unlink the saved bytes put
  the exit back exactly as S1 left it (`jmp stub`, the branches to the stub) or as assembled (`nop`, the branches to it). Mutant `dlink_unlink_skip`:
  `e6_01` case (4) (a side trace that inlines a `load`ed function is flushed with its prototype, `Dep`, while the parent lives, and the next round's
  side trace is attached to the same exit) runs into the freed block.
* **Flush ordering, carry maps.** The cell goes back to its landing before the unpatch, so between the two the exit still went to the side trace's
  block, which is not yet released. The link entry carries no values in registers beyond what the stub would have stored (no carried loads),
  so R1's carry-map invariants are untouched; the side trace's SLOADs and their guards run as on the plain entry.
* **Count hooks and polls.** A linked exit is a jump between two traces of a region: no Lua bytecode of a new frame starts without a poll that the
  stub path would have run (the stub has no poll); asynchronous hooks are delivered at the same polls.
* **The budget register across native transfers (bit 1).** The inner entry reached through a `rec` cell continues with the register (it has
  never reloaded the word); the only path that leaves stores it first (mutant `dlink_vb_fb_nostore`: the null-cell path without the store, caught
  by `e6_dlink_programs`). An inner entry's failing room check stores it (`rec_fail`, unchanged).
* **Lazy region.** A linked exit continues the region like the cell jump it replaces (36.1: the side trace's own entry failure leaves); the
  frames above `lazy_root` stay lazy until an exit or leave completes them (`lazypoison` jobs of `e6_dlink_programs`).

### 36.6 Measured (release `rn-cli`, one binary; `dlink=0` against `dlink=3`, every earlier stage on; `RUSTYNAIL_SEED=0`, `taskset -c 4`; instructions of the first run, cycles min of 5 (5.5 key set) / 3 (sanity set, 5.4))

| program (5.5) | `dlink=0` instr M / cyc M | `dlink=3` | delta instr / cyc |
|---|---|---|---|
| fib 32 | 390.2 / 62.5 | 357.7 / 58.9 | **-8.33 % / -5.8 %** |
| binary_trees 15 | 2397.6 / 478.3 | 2345.8 / 476.5 | -2.16 % / -0.4 % |
| sumrec | 659.8 / 111.5 | 649.6 / 108.7 | -1.55 % / -2.5 % |
| permute 300 | 594.5 / 101.2 | 572.3 / 98.9 | -3.73 % / -2.3 % |
| towers 100 | 353.4 / 83.2 | 343.5 / 82.9 | -2.80 % / -0.4 % |
| list 300 | 343.1 / 82.3 | 343.1 / 82.1 | 0 / -0.2 % |
| queens | 511.6 / 95.1 | 511.6 / 95.7 | 0 / +0.6 % |
| pqueens | 5391.4 / 1841.9 | 5391.4 / 1849.6 | 0 / +0.4 % |
| richards | 176.9 / 59.4 | 177.0 / 59.6 | +0.06 % / +0.3 % |
| deltablue | 324.2 / 107.1 | 324.2 / 107.6 | 0 / +0.5 % |
| havlak | 512.4 / 148.7 | 511.9 / 148.6 | -0.10 % / -0.1 % |
| cd 20 | 10736.6 / 2904.3 | 10732.4 / 2920.4 | -0.04 % / +0.6 % |
| raytrace | 365.0 / 111.9 | 365.0 / 112.2 | 0 / +0.3 % |
| raytrace 40 | 4608.9 / 1156.8 | 4609.0 / 1179.0 | 0 / +1.9 % (interleaved re-run: 1158.1 / 1168.5 against 1153.9 / 1167.8) |

Per bit (fib 32, 5.5): bit 0 alone 363.8 M / 59.5 Mcyc (-6.8 % / -4.5 %), bit 1 alone 384.1 M / 62.2 Mcyc (-1.6 % / -0.2 %), both 357.7 M /
59.2 Mcyc. sumrec gains mostly from bit 1 (651.0 M / 107.6 Mcyc alone): its leaves return through the region root (no side trace on the hot path).

**Lua 5.4** (min of 3): fib -8.33 % / -6.0 %, binary_trees -1.92 % / -0.6 %, sumrec -1.55 % / -9.2 % (min-of-3 spread), permute -3.59 % /
-1.8 %, towers -2.76 % / -3.3 %; list, queens, pqueens, richards, deltablue, havlak, cd, raytrace (40) within 0.03 % instructions and +1.0 % /
-1.8 % cycles.

**Sanity set** (44 programs, min of 3): `awfy/permute` -3.65 % / -2.2 %, `awfy/towers` -2.34 % / -1.1 %, `programs/binary_trees` -2.09 % / 0.0 %,
`programs/fib` -8.40 % / -6.1 %; every other program within 0.12 % instructions (`awfy/havlak` -0.12 %, `awfy/cd` -0.04 %, `awfy/richards` +0.06 %).
Cycle differences above +1 % occur only at equal instruction counts: `programs/varargs` +3.8 %, `programs/string_ops` +3.2 %,
`programs/coroutines` +2.6 %, `awfy/list` +1.5 % in the batch; re-measured interleaved (min of 5, two rounds, ref / dlink): varargs 1206.9 / 1209.6
against 1205.3 / 1212.3, coroutines 1761.4 / 1751.5 against 1774.5 / 1752.1, string_ops 1123.7 / 1155.0 against 1143.0 / 1183.5 (the same
configuration spreads by 3 % between rounds), list 92.8 / 92.6 against 93.6 / 93.6 (+0.9 %). No program above +1 % instructions; no cycle
difference above +2 % that survives an interleaved re-run except `string_ops`, whose two configurations overlap within its own spread
(instructions equal to 0.1 M: no linked exit runs in it).

Against the design's estimates (DESIGN_CALL_PROTOCOL 4.6, 7): fib -5.5 % instructions estimated, -8.3 % measured (both bits; -5.8 % cycles);
binary_trees 2-4 % estimated, -2.2 % instructions / -0.4 % cycles; permute -2 % estimated, -3.7 % / -2.3 %; queens 2-7 % estimated, **0**: queens is
not a lazy-aware family (its first trace is the column loop, 35.1), so its exits keep their stubs. E6 as built does not reach non-lazy families.

### 36.7 Tests, mutants

New program `tests/vm/e6_01_dlink.lua` (also in `vm_corpus`): fib with a live value at the leaf guard, a tree whose leaves take four paths (float,
string, table values live), method recursion with a loop, a leaf side trace that inlines a `load`ed function collected between rounds (flushed alone
with its prototype and re-attached: unlinks), the region left by a C call in a leaf, a count hook installed from inside after the links formed,
errors from depth. `tr_core::e6_dlink_programs`: `e6_01`, `e1_01`, `e4_01`, `e5_01`, `e6_00`, `deep_recursion` x `always` / `on2` x `dlink` 0 / 1 /
2 / 3, `+verify`; for `dlink=3`: exit storm, `lazypoison=1`, budgets 3 / 17 / 200, GC stress 1 / 2 in both collector modes, stack-move stress;
with the oracle feature the identity at budgets none / 3 / 17; asserts direct links (and unlinks in `e6_01`) with bit 0 and none without.
`tests/vm` + `tests/lazyci` (226 programs) in `always` / `on` and in `always` with GC stress 2, debug `rnlua`, default knobs, against the
interpreter: no difference. Mutants (registry 785 -> **789**): `dlink_refund_off`, `dlink_store_skip`, `dlink_unlink_skip`, `dlink_vb_fb_nostore`,
all caught by `e6_dlink_programs` (5.5 and 5.4).

### 36.8 Gates, what ran and what did not, risks

Ran: light gate per commit (results in each commit message; the E6 commit: fmt, mutant map, clippy x3, core x2, `vm_corpus`, jit-diff 1-50 PASS, mutant group 169 ran, 169 caught; the trace suites 5.5 / 5.4 failed on `s1_lazystub_programs` only (its `lazystub=1` / `lazystub=0` twins compare exit counts, and linked exits exist only with deferred stubs and are not counted: the twins run with `dlink=2`; re-run PASS)); `gate-mutants.sh --only dlink_,lazy_sidefail,lazy_rec,lazy_stub,lazy_mat` (10 / 10),
`--only dlink_ --version 5.4`, `--only midfold_,lazy_sidefail` on 5.5 and 5.4 (9 / 9 each); `tr_core::{e6_dlink_programs, lz_side_entry_programs,
e5_midfold_programs, e4_lazyloop_programs, e1_unroll_programs, e2_idfold_programs, bk_lazy_*}` on 5.4; the differential runs above. **Not run**
(by instruction): the heavy gate, the oracle-feature suites (`tr_identity`, `instruction_count_identity`, `oracle_options`; the identity jobs of
`e6_dlink_programs` ran only inside the mutant runner's oracle build), jit-diff 1-200 in seven modes, the full mutant runner.

Risks. (1) Code patching from the installer and the flush (S1 patched only from the exit handler of the trace that owns the bytes): the argument
is TR1 (no trace code runs while the recorder or a flush runs) plus TR18 for flushes inside handlers; a future path that installs or flushes while
trace code of the parent is mid-flight would break it. (2) Linked exits are not counted: a policy that one day reads the counters of exits with
side traces attached would see fewer exits (R1's carry path has the same property). (3) `lazystub=0` disables the links (no patch sites).
(4) The gain is limited to lazy-aware families; the queens-type exits stay on stubs. (5) The two survivors of 35.8 are caught (36.2); the formation
dependence of their programs is real: case (14) catches `midfold_rootret_inner` only in `on2`.

## 37. E9: the tier-1 call protocol (stage 6 of docs/DESIGN_CALL_PROTOCOL.md; 2026-11-22, base f0a9288)

Stage 6 has three items, built and measured in order (design 4.9: "measure (1) before building (2) and (3)"; cycles decide, not instructions). Result:
**item 1 (`jret`) is built, sound and tested, and behind the default-off knob `t1call`** (it buys no cycles on cd / richards / deltablue / raytrace /
list in the shipped configuration, but -5 .. -10 % cycles on recursion in tier 1 alone); **items 2 (call-site inline cache) and 3 (native return of
function-entry traces) are not built**: their ceilings, measured below, are under the 1 % bar on all five target programs.

### 37.1 Item 1 as built: `jret` for tier-1 returns (`t1call` bit 0)

* **Mark and cell.** The tier-1 `CALL` template's flat call (`calls.rs::tpl_call`, the path that fills the callee's `CallInfo` like
  `pre_call_lua_fast` and jumps to `entry0`) stores `callstatus = C | CIST_T1RET` (one immediate, as before) and the address of its **site's resume
  cell** into the new frame's `ck` word (`jit_layout::callinfo::JRET`; `ck` holds a C continuation and is read only for C frames whose `k` is the C
  trampoline, never for a Lua frame). `CIST_T1RET` is bit 28 (`rn-core/src/state.rs`), free in both versions (5.4's `CIST_LEQ` is bit 26). No
  `CallInfo` field was added (a new field would change the thread's memory accounting, `collectgarbage("count")` and GC pacing).
* **Cells.** One `usize` per `CALL` instruction of a prototype (bytecode order; `Cx::call_ord` maps a pc to its index), a `Box<[usize]>` owned by the
  prototype's `JitProto` (`jret_cells`, allocated before the first lowering so the code can embed the addresses, charged with the descriptor). Cell
  `k` = the absolute address of the current code version's resume entry of the instruction after the `k`-th `CALL` (`base - 1 + resume[pc + 1]`),
  0 when that instruction has no entry (the next instruction is an exit stub, or a flagged S1 entry). `rt::fill_jret_cells` writes them at the
  compile and in the swap of a tier-1.5 recompile (`recompile_inner`), where no generated code runs; the box never moves, so frames pushed by the
  `CALL` of a retired version carry a cell that now names the new version's entry.
* **The return.** The shared `ret_tail` stub (every tier-1 `RETURN0` / `RETURN1` / `RETURN`, and the `TAILCALL` whose native callee popped the
  frame) first tests the popped frame's `CIST_T1RET`; with it set and a non-zero cell it loads the caller (`thread.ci`), computes the caller's
  frame base from `caller.func` and the current stack pointer and its constants from `caller.kcache` exactly as the walk does, and jumps to the cell's
  address. Otherwise (bit clear, or cell 0) the walk runs as before: `CIST_FRESH` -> `Returned`, `func -> closure -> proto -> jit -> resume
  table[savedpc]`. The jret path replaces five dependent loads, the `jit <= 1` test, the `savedpc` arithmetic and the resume-table lookup by two
  loads (`ck`, the cell) and one test: fib in tier 1 alone, -8 instructions per call (the `CALL` pays 2 more: the cell address and its store).
* **Knob.** `RUSTYNAIL_JIT_TRACE_LIMITS=t1call=N` (`TrLimits::t1call`, process / struct default `T1CALL_PROCESS = 0`); like `t1q`, a named value also
  applies with `RUSTYNAIL_JIT_TRACE=off` (bit 7 marks it named), so `RUSTYNAIL_JIT_TRACE=off` stays J4 exactly unless asked. With the knob off no
  cell is allocated, `CallEnv::jret` is 0 and the generated code is the code of f0a9288 (the descriptor charge too: the empty box is not charged).

### 37.2 Soundness of item 1

The claim: whenever the jret path runs, its target is the address the walk would have computed, with the same `r13` / `rbx` / `r14`.

* **Who writes the mark.** Only the tier-1 flat call. Every other creator of a Lua or C frame writes `callstatus` whole: the interpreter's
  `pre_call_lua_fast` (`callstatus = nresults + 1`, also for `mm_frame`, which ORs `CIST_MMRET` afterwards), `prep_call_info` (C frames and the general
  path), the base frame, the trace compiler's eager push (`asm.rs`), the inline frame materialisation of exits (`exit::mat_stub` code), the exit
  handler's restore (`exit.rs`), `lazy_leave` (the completion of lazy frames), the tier-1 metamethod frames (pushed through the interpreter's helpers).
  A lazy frame's `callstatus` is stale until its completion (it may hold an old mark), but a lazy frame never reaches a tier-1 `RETURN`: it returns
  natively inside the region, and every exit or leave completes it first (I6), which rewrites `callstatus`. The ORs and clears of single bits
  (`CIST_TAIL`, `CIST_HOOKED`, `CIST_FIN`, `CIST_YPCALL`, ...) leave bit 28 alone. So a marked frame was pushed by a tier-1 `CALL` at pc of proto `P`,
  and its `ck` holds `cell(P, pc)`.
* **The caller is still where the `CALL` left it.** The caller's frame is suspended at `savedpc = pc + 1` until the callee returns: nothing changes
  a suspended Lua frame's `savedpc` or function slot (the debug API writes locals only; errors unwind both frames together; a coroutine keeps both).
  The interpreter's tail call (`pre_tail_call`, and tier 1's inline tail call) reuses the marked node for another function but keeps the caller and
  its continuation; it ORs `CIST_TAIL` and keeps the mark and `ck`, which stay right. A `CIST_FRESH` frame is never marked (the flag is ORed only
  into frames that `ccall` itself pushed), so the jret path skipping the `CIST_FRESH` test is exact.
* **The cell names the walk's answer.** The walk reads the caller's *current* descriptor (`p.jit` of a live prototype is set once, at its compile,
  and cleared only when the prototype is freed; a live caller activation keeps its closure and so its prototype) and its current resume table; the
  cell holds the same entry of the same table (both are written from the same `resume_table` in the same step, at compile and in a recompile's
  swap). Cell 0 is exactly "no entry", where the walk would exit `FrameChanged`; the jret path then falls into the walk.
* **Lifetime.** The cells live and die with the descriptor, which lives as long as the prototype; a marked frame points at the cells of its
  caller's prototype, alive while the caller activation is. A stale `ck` in a reused node is never read (the mark is clear). `ck` is not a GC
  reference; `CallInfo` nodes never move (the stack does, and the jret path recomputes the caller's base from `caller.func` and the current stack
  pointer, like the walk: `RUSTYNAIL_JIT_STRESS=stackmove` jobs). Shrinking the `CallInfo` list frees only nodes above the running frame.
* **T15 recompile while frames are live.** Frames pushed by the old version return through the rewritten cell into the new version, which is what
  the walk does today (it reads the current resume table); TR19 / JIT_TIER15 12 condition (3) makes every version have the same resume points. The old
  block is retired, not freed.
* **Lua-visible behaviour.** Nothing is counted on the return path (TR5: the walk does not count either; the oracle-feature identity jobs of
  `e9_t1call_programs` check it in tier 1 and with traces). Hooks: the `RETURN` prologue's `trap` / interrupt tests run before `ret_tail` as before,
  so a hook installed while the frame ran sends the return to the interpreter, which never reads the mark; count and line hooks installed from
  inside a marked frame give the interpreter's counts (`e9_01` case (11)). `stack overflow` (depth, message): the call side is unchanged (the
  same guards, the same helper on failure), case (8). `pcall` / `error` / `debug.traceback` / `getinfo`: they read `func`, `savedpc`, `callstatus`
  bits other than 28, all as before (cases (7), (9)). Coroutines: the mark lives in the coroutine's own `CallInfo` list (case (9)). The C API sees
  `ck` only for C frames.
* **Invariants I1-I10.** None is touched: the item is tier-1 only. I2 (budget) as above; I3 (room / spare node checks) are the `CALL`'s unchanged
  guards; I6 (lazy region) is relied on only for "a lazy frame never reaches a tier-1 `RETURN` before its completion"; I9 / `Retf` and the other
  trace guards on `callstatus` mask explicit bits (`RETF_CS_MASK`, the `CIST_FRESH` / `CIST_C` tests of `rec_ret`): bit 28 is not compared, so a
  marked root frame returns natively into a continuation trace and is popped by `Retf` as before. Tier-1 <-> trace <-> interpreter transitions do
  not read `ck`.

### 37.3 Measured (release `rn-cli`, one binary; `t1call=0` against `t1call=1`, every earlier stage on; `RUSTYNAIL_SEED=0`, `taskset -c 4`; instructions of the first run, cycles min of 5 (key set, interleaved) / 3)

Shipped configuration (traces on), 5.5:

| program | `t1call=0` instr M / cyc M | `t1call=1` | delta instr / cyc |
|---|---|---|---|
| fib 32 | 357.7 / 58.7 | 357.7 / 58.9 | 0 / +0.4 % |
| binary_trees 15 | 2345.8 / 470.6 | 2345.2 / 474.7 | -0.03 % / +0.9 % |
| sumrec | 649.6 / 108.3 | 650.2 / 116.9 | +0.09 % / +7.9 % (ASLR bimodality, below) |
| permute 300 | 572.3 / 98.9 | 572.3 / 98.6 | 0 / -0.3 % |
| towers 100 | 343.4 / 82.7 | 343.6 / 83.3 | +0.04 % / +0.7 % |
| list 300 | 343.2 / 82.2 | 343.6 / 82.7 | +0.13 % / +0.6 % (re-run, min of 7: -0.3 %) |
| queens | 511.6 / 96.1 | 512.9 / 96.1 | +0.26 % / 0 |
| pqueens | 5391.4 / 1850.7 | 5391.9 / 1857.1 | +0.01 % / +0.3 % |
| richards | 176.9 / 59.3 | 177.1 / 59.1 | +0.11 % / -0.2 % (re-run: -0.2 %) |
| deltablue | 324.2 / 108.0 | 323.7 / 107.8 | -0.17 % / -0.2 % (re-run: -0.5 %) |
| havlak | 515.9 / 156.8 | 515.3 / 158.2 | -0.11 % / +0.9 % |
| cd 20 | 10732.6 / 2956.1 | 10746.8 / 2941.6 | +0.13 % / -0.5 % (re-run: +0.1 %) |
| raytrace | 365.7 / 113.1 | 365.9 / 115.0 | +0.06 % / +1.7 % (re-run: +0.2 %) |
| raytrace 40 | 4601.7 / 1185.9 | 4604.1 / 1169.1 | +0.05 % / -1.4 % |

`sumrec` at equal instructions: ten single runs per configuration give 109.2 .. 124.6 Mcyc (`t1call=0`) and 113.5 .. 122.3 (`t1call=1`): the base has
a low mode (109-110) that a run reaches or not by its address layout; the descriptor's extra allocation moves the heap. Not a cost of the item.

Tier 1 alone (`RUSTYNAIL_JIT_TRACE=off`, `t1call` named), 5.5, min of 3 (the five targets re-run interleaved, min of 7, in brackets):

| program | delta instr / cyc |
|---|---|
| fib 32 | **-5.29 % / -9.7 %** |
| binary_trees 15 | -2.18 % / -7.6 % |
| sumrec | -5.02 % / -5.7 % |
| towers 100 | -1.30 % / -5.0 % |
| permute 300 | -0.49 % / -2.6 % |
| pqueens | -0.48 % / -0.4 % |
| queens | +0.19 % / +0.9 % |
| list 300 | -0.37 % / -1.2 % [-1.6 %] |
| richards | -0.04 % / +1.2 % [+2.5 %] |
| deltablue | -0.12 % / -0.7 % [+1.1 %] |
| havlak | -0.01 % / -3.8 % [-0.1 %] |
| cd 20 | -0.05 % / +0.7 % [+0.2 %] |
| raytrace | +0.03 % / +0.1 % [-1.4 %] |

The five targets move by +-2.5 % at equal instructions between rounds: tier 1 alone, item 1 is a recursion win (every call a flat call, every
return a marked one) and nothing on them.

**Why nothing on the targets.** Exact counts of tier-1 returns (a scratch build with counters in `ret_tail`, not committed; shipped configuration,
`t1call=1`): cd 20 11.31 M returns, of which 1.48 M through a cell; deltablue 250 k / 117 k; havlak 222 k / 102 k; raytrace 40 2.28 M / 163 k;
richards 34 k / 2 k; list 37 k / 4 k; queens 438 k / 1.5 k. Most tier-1 returns of these programs pop frames that the flat call did not push
(frames rebuilt by trace exits inside inlined callees, eager trace pushes, the interpreter), and the ones that take the cell save ~8 instructions
each: 1.48 M x 8 = 12 M instructions on cd's 10.7 G. Extending the mark to the frames exits rebuild (four writers: `mat_stub`, the handler's
restore, `lazy_leave`, the eager push, each needing its caller's cell) would reach cd's other 9.8 M returns: ~80 M instructions (0.75 %), ~26 Mcyc
at fib's 2.7 cycles per return (0.9 %) by arithmetic. Not built: under the bar, for four new writers.

**Lua 5.4** (min of 3), shipped configuration: every key program within +0.31 % instructions (queens; cd +0.12 %, richards +0.05 %, deltablue
-0.15 %, list +0.13 %, raytrace +0.03 %) and -0.9 .. +1.6 % cycles (havlak +1.6 %, sumrec +4.5 % as on 5.5); tier 1 alone: fib -5.29 % / -9.0 %,
binary_trees -2.03 % / -6.4 %, sumrec -5.02 % / -6.1 %, towers -1.30 % / -6.2 %, list -0.37 % / -1.6 %, raytrace +0.03 % / -2.0 %, cd -0.05 % /
+1.0 %, richards / deltablue / havlak within 0.8 %.

**Sanity set** (44 programs, 5.5, shipped configuration, min of 3): no program above +0.26 % instructions (`awfy/queens`; `programs/closures`
+0.21 %, `awfy/storage` +0.15 %, `awfy/cd` +0.13 %; the largest decrease `programs/json` -0.42 %). Cycle differences above +2 % in the batch, re-run
interleaved: `programs/table_hash` +4.4 % (re-runs +3.0 %, then +0.4 % at min of 7: identical instruction counts, no flat call on its hot path:
layout), `programs/binary_trees` +3.5 % (re-run +0.2 %), `awfy/towers` +2.1 % (re-run 0.0 %), `programs/closures` +1.9 % (re-runs +2.3 %, +1.9 %
with +0.21 % instructions: the two stores the `CALL` adds per flat call on a program whose returns mostly close upvalues; under the +2 % line, and the
knob is off by default).

### 37.4 Item 2 (call-site inline cache): not built, ceiling measured

The cache would replace, at a flat call or an inline tail call, the `jit <= 1` test, the `entry0 == 0` test and the `numparams` load and test by a
compare of the closure's proto with the cached one (plus a miss path, and an invalidation story: a proto freed and another allocated at the same
address must not hit). Its ceiling is the code without the tests it removes and without anything it adds: a scratch build (`RN_E9CEIL`, not
committed; unsound for callees with too few arguments or without a function entry, harmless on these programs: outputs equal) dropped the
`entry0` test and, for fixed argument counts, the `numparams` test in both templates. With `t1call=1` in both columns:

| program | traces on: delta instr / cyc | tier 1 alone: delta instr / cyc |
|---|---|---|
| fib 32 | 0 / +0.8 % | -4.19 % / -2.8 % |
| binary_trees 15 | -0.02 % / +0.6 % | -1.67 % / -0.4 % |
| permute 300 | -0.01 % / +0.1 % | -1.13 % / -1.8 % |
| towers 100 | -0.15 % / -0.4 % | -0.99 % / -2.0 % |
| list 300 | -0.58 % / -0.2 % | -0.98 % / -0.3 % |
| queens | -0.25 % / -1.5 % | -1.04 % / -1.7 % |
| richards | -0.21 % / +1.4 % | -0.49 % / +0.2 % |
| deltablue | -0.47 % / -1.0 % | -0.93 % / -2.4 % |
| havlak | -0.25 % / +0.6 % | -0.23 % / -1.5 % |
| cd 20 | -0.59 % / +0.7 % | -0.79 % / -1.8 % |
| raytrace | -0.28 % / +0.8 % | -0.46 % / -0.2 % |

(min of 5 with traces, 3 in tier 1 alone.) In the shipped configuration the ceiling moves cycles within the +-1.5 % noise of equal-instruction
runs on all five targets (deltablue -1.0 %, the best, at -0.47 % instructions); a real cache adds its compare and miss path back. Left out.

### 37.5 Item 3 (native return of function-entry traces): not built, bound measured

A function-entry trace whose root frame reaches its `RETURN` ends with the always-taken exit of `fn_end` (P-E2); tier 1 then runs the `RETURN` and
`ret_tail`. Item 3 would pop the frame inside the trace and jump through the cell of item 1 (design 4.9: ~75 instructions per instance). The number
of instances is bounded by the outer entries of function-entry traces (`[heads] ... fn Traced ... entries`, exact counters of the entry sequence,
`RUSTYNAIL_JIT_VERBOSE=1`) and by the tier-1 returns of 37.3:

| program | fn-trace entries | tier-1 returns | bound at 75 instructions each |
|---|---|---|---|
| cd 20 | 333 k (`cd.lua:168, 122, 204, 140, 289, 88`) | 11.31 M | 25 M of 10.7 G: 0.23 % |
| deltablue | 32.7 k (`deltablue.lua:303, 126, 198, 211`) | 250 k | 2.5 M of 324 M: 0.76 % |
| raytrace | 12.4 k (`raytrace.lua:16`) | 120 k | 0.9 M of 366 M: 0.25 % |
| list 300 | 500 k (`list.lua:31`, a recursion family: its returns are native already) | 37 k | at most 2.8 M of 343 M: 0.8 % |
| richards | 341 (`richards.lua:172`) | 34 k | 26 k: 0.01 % |

Every bound is below 1 % of instructions before asking how much of it would show in cycles; the item would add a second native-return path to
the trace assembler (budget charge of the `RETURN`, hook / interrupt tests, the leave of the region, the result moves). Left out.

### 37.6 The interpreter (design 4.10)

No change: `pre_call_lua_fast`, `OP_RETURN` and `do_ret!` write and read what they did; they never write `ck` for a Lua frame, and the only reader
trusts `ck` under `CIST_T1RET`, which the interpreter's pushes clear (its tail call keeps it, with the caller, above). The interpreter returning
into a tier-1 caller continues it in the interpreter as before.

### 37.7 Tests, mutants

New program `tests/vm/e9_01_t1call.lua` (also in `vm_corpus`; outputs equal to the reference 5.5 and 5.4): one callee with call sites for 0 / 1 / 2 /
all results and constants used right after each return; the same callee and depth pushed alternately by the flat call and by the general helper
(one argument too few: a node holds a stale cell when the helper reuses it); tail calls in marked frames (Lua, C, `select`); vararg callees;
upvalue and method recursion; metamethod frames between marked frames; errors from depth, a traceback and `getinfo` from depth; stack overflow
through marked frames; a coroutine yielding from inside marked frames; a sort comparator making flat calls; count and line hooks installed from
inside a marked frame; the callee replaced while marked frames are suspended; upvalues closed by the return; a recursive function whose loop runs
after its call (tier 1.5 recompiles it while the outer activations wait: their returns go through rewritten cells into the new version, seen with
`RUSTYNAIL_JIT_VERBOSE=1`: `promoted nest ... version 0 -> 1`).
`tr_core::e9_t1call_programs`: `e9_01`, `e6_01`, `deep_recursion` x `t1` (tier 1 alone) / `always` / `on2` x `t1call=0` / `1`, each also with
tier 1.5 in `always` mode (`TR_CORE_T15`, new job environment read by `one_state`); for `t1call=1`: `+verify` (traced modes), budgets 3 / 17 / 200,
GC stress 1 / 2 in both collector modes, stack-move stress (not `e9_01` in `t1`: its stack-overflow case is quadratic under that stress with the
knob off too, past the 120 s leash), the oracle-feature identity at budgets none / 3 / 17. Mutants (registry 789 -> **792**): `t1_jret_site_next`
(a `CALL` stores the next site's cell), `t1_jret_ignore_mark` (the return trusts `ck` without the mark), `t1_jret_no_setk` (the jret path keeps the
callee's constants): all caught by `e9_t1call_programs` on 5.5 and 5.4 (`gate-mutants.sh --only t1_jret_`: 3 / 3 each).

### 37.8 Gates, what ran and what did not, risks

Ran: `gate-light.sh` on the commit: fmt, mutant map, clippy x3, core x2, trace suites 5.5 / 5.4, `vm_corpus`, jit-diff 1-50 PASS, mutant group 78 ran, 78 caught (465 s; a first run failed on the frozen registry count and on a `debug.traceback` line count of `e9_01` that depends on how the chunk is started, both fixed before the commit); `gate-mutants.sh --only t1_jret_` on 5.5 and 5.4 (3 / 3 caught each); the
`tr_core::e9_t1call_programs` matrix (debug, 5.5) inside the light gate's trace suites; the differential runs of `e9_01` against the reference 5.5
and 5.4 (`RUSTYNAIL_JIT=off`, tier 1 alone with the knob on and off, traces `on` / `always` with the knob, tier 1.5 `always`). **Not run** (by
instruction): `gate-heavy.sh`, the oracle-feature suites (`tr_identity`, `instruction_count_identity`, `oracle_options`; the identity jobs of
`e9_t1call_programs` ran only inside the mutant runner's oracle build), jit-diff 1-200 in seven modes, the full mutant runner, no full benchmark.

Risks. (1) The mark's soundness rests on "every frame creator writes `callstatus` whole": a future pusher that ORs into a reused node's
`callstatus` (or a new field writer of `ck` for Lua frames) would let a stale cell be trusted; `t1_jret_ignore_mark` shows the tests catch a
trusted stale cell. (2) `ck` is shared with the C API's continuation for C frames: a change that makes Lua frames carry a C continuation would
collide. (3) The item is off by default: the shipped configuration does not exercise it outside `e9_t1call_programs` (vm_corpus and jit-diff run
the default knobs). (4) Items 2 and 3 stay open with their ceilings; the reach of item 1 in the shipped configuration is limited by which frames
the flat call pushes (37.3), and the extension to exit-rebuilt frames is estimated at under 1 % on cd.

## 38. The `rec_loop` regression of session-k: a formation lottery at deep sites, and `deepprobe` (2026-11-22, base 2222703)

Benchmark session-k (bench/results/session-k/report.md) found one real regression: `jit-trace/rec_loop` (`fib(i % 3 + 4) + ack(1, i % 4)` summed in
a hot loop of `run`, called 6000 times) +14.6 % cycles / +17.9 % instructions against b116447 (before E1 / E2 / E4 / E5 / E6), the same on 5.5 / 5.4,
plain / PGO.

### 38.1 Bisection

One binary (2222703, 5.5, `RUSTYNAIL_SEED=0`), instructions of `rec_loop`: shipped knobs 516.7 M; `recunroll=0` 437.6 M; `idfold=0`, `lazyloop=0`,
`midfold=0`, `dlink=0`, `t1call=0` each within 0.03 % of the shipped value; every E-knob off 438.1 M; b116447 (built at an equal-length path)
438.0 M. So the whole delta follows **E1's `recunroll`**; no commit bisection was needed beyond b116447 against the knob (E1 is the only stage whose
knob moves it).

### 38.2 Root cause: which iteration the loop's root trace is recorded on

E1 does not make any iteration of `rec_loop` more expensive. The time is spent in the family of the loop trace of `run` (98 % of the samples;
`fib`'s own unrolled family is entered 1407 times). That loop is a **deep site** (J2-2): its first recording stops at the recursion limit, and
the re-record with the deep limits records whatever iteration comes next. The recursion inlined into the root then depends on `i`: the root inlines
`fib(i % 3 + 4)` and `ack(1, i % 4)` of that iteration, the other depths leave through side traces, and the cost of the whole tree depends on that
choice (a recorded `fib(6)` lets the shallower calls leave early at the base-case guards; a recorded `fib(4)` makes the deeper calls continue in
chains of side traces that inline more levels; exits per iteration 1.85 against 2.70).

* With `recunroll=0` the loop head fires at `i = 23` of the second `run` call (`fib(6)`, `ack(1, 3)`, path 190 bytecodes, the longest variant);
  with E1 at `i = 24` (`fib(4)`, `ack(1, 0)`, 66 bytecodes). Read with a temporary print of the `for` index at the recording start.
* The shift is not E1-specific: the hotness of a loop with calls is counted in compiled code and by sampling ticks (rt.rs `hot_loop`), and E1's
  unrolled `fib` family records three more side traces (exits inside the inlined frames) while `run`'s loop warms up, which moves the firing by one
  iteration.
* It is a lottery, not a cost. `RUSTYNAIL_JIT_TRACE_HOT` = 250 .. 268 (19 thresholds; 10 of the 12 residues of `i mod 12` are the recorded iteration at one of them at least):

| binary / knobs | mean | min | max | at the default 256 |
|---|---|---|---|---|
| b116447 | 485.0 M | 438.0 | 529.3 | **438.0** (the luckiest of the 19) |
| 2222703 shipped (`deepprobe=0` on the new binary) | 482.4 M | 462.5 | 529.9 | 516.7 |
| new (`deepprobe=12`) | **447.6 M** | 444.3 | 452.0 | 444.5 |

By recorded iteration (residue `i mod 12`, instructions M): `fib(6)` + `ack(1,3)` 437.6; `fib(6)` + `ack(1,1)` 462; `fib(5)` + `ack(1,3)` 463;
`fib(6)` + `ack(1,0)` 469; `fib(5)` + `ack(1,2)` 470; `fib(5)` + `ack(1,1)` 488; `fib(5)` + `ack(1,0)` 493; `fib(4)` + `ack(1,2)` 504;
`fib(4)` + `ack(1,0)` 516; `fib(4)` + `ack(1,1)` 529. The longer the recorded path, the cheaper the tree. b116447 sat on the best phase at the
default threshold, 2222703 on one of the worst; averaged over the phases E1 is neutral (-0.5 %).

### 38.3 The fix: a deep site records several probe iterations and compiles one of the longest (`deepprobe`)

Knob `deepprobe` (`RUSTYNAIL_JIT_TRACE_LIMITS`, 0 = off, **12 = default**, at most 64). Only root recordings of a head that is a deep site
(`HeadData::deep`, set by the J2-2 re-record) are affected; everything else is unchanged.

* When such a recording **closes** (`finish_outcome`, before the pipeline), `cache::deep_probe_drop` asks `TrProto::deep_probe` with the length of
  the recorded path (bytecodes). The first `deepprobe` closed recordings are **probes**: dropped (`AbortKind::Probe`: no penalty counter changes,
  the head records again after one or two back-edges, alternately, so that the probes' strides of two and three iterations cover loops whose depth
  pattern has an even period: with one-back-edge waits only every other iteration was seen, 6 of the 12 phases of `rec_loop`), the longest path kept.
* Then the first closed recording whose path is at least 7/8 of the longest probe is compiled (the strides do not repeat the longest probe's iteration
  exactly; 7/8 took a `fib(6)` iteration at every threshold), at most `deepprobe` tries, the last one is taken whatever its length.
* **Uniform sites** stop early: after four probes with one path the fourth is compiled (`DEEPPROBE_UNIFORM`; `awfy/list`'s site recorded twelve
  probes of 99 bytecodes each and the delay cost it +1.6 % instructions; with the rule +0.4 %).
* **Aborts end the probing** (`TrProto::deep_probe_end`, from `finish_abort` for any root abort of the head but a dropped probe): a site whose
  recordings do not close reliably compiles the next one that closes. `corpus2/heldout/scheme_interp`'s site closed one recording after many
  `InnerLoop` / `LeaveLoop` aborts; dropping it as a probe cost +0.8 % instructions; with the rule the run is identical to `deepprobe=0`.
* A site probes once: once a recording is taken (or an abort ended the probing) `deep_n = DEEP_DONE`; a re-record after a flush compiles its first
  closed recording, as before.
* Cost: a probe is a full recording (lockstep with the interpreter) that is not compiled; on `rec_loop` 14 dropped recordings, `rec_step` 1.33 ->
  2.34 Mtsc. Fewer probes do not cover the phases: `deepprobe` 4 / 6 / 8 left 440-508 / 441-473 / 443-473 M over thresholds 250-262, 12 gives
  444-452.

### 38.4 Soundness

A policy decision only: which closed recording of a head is compiled. A dropped probe is a recording that ends without a trace, exactly like an abort
that is not the head's fault (the recording's id is freed, its buffers uncharged, its dependency edges dropped by `end_rec_common`, the head returns
to `Warm` with no trace); nothing of it was installed, no cell, exit or dispatch entry ever pointed at it, and the interpreter ran the iteration
itself (the recorder runs beside it). The compiled recording is an ordinary root recording with the deep limits, unchanged code from the recorder
to the install. Decisions taken at a prototype's first recording (E2's `idfold_decide`, E5's `midfold_decide`) happen at the first recording
whether it is later dropped or not, as for an aborted first recording; decisions taken at the first compile (E4's `lazy_loop_decide`) happen at the
compile of the recording that is taken. No invariant of this document (TR1-TR32, I1-I10, the region and identity arguments of sections 34-36) refers
to which iteration a trace was recorded on.

Lua-visible behaviour is unchanged: hooks, errors, `pcall` from depth, coroutines and GC see the interpreter run the probe iterations as they run any
recorded iteration (`e10_01` cases (5) and (6): a callee switched while the site probes, errors from depth inside the probed loop).

### 38.5 Measured (release `rn-cli`; `RUSTYNAIL_SEED=0`; binaries at equal-length paths, environment strings of equal length; instructions of the first run (deterministic), cycles min of 5 interleaved on CPU 4 (`rec_loop`, `list`, `coro_generators`: min of 10))

Columns: b116447, `off` = the new binary with `deepprobe=0` (2222703's behaviour), `new` = the new binary, shipped knobs.

| program (5.5) | instr M b116447 / off / new | new vs b116447 | new vs off | cycles M b116447 / off / new | new vs b116447 | new vs off |
|---|---|---|---|---|---|---|
| jit-trace/rec_loop | 438.0 / 516.7 / 444.5 | **+1.48 %** | **-13.97 %** | 78.4 / 89.9 / 80.8 (min of 15) | +3.0 % | **-10.2 %** |
| programs/fib | 7780.9 / 3928.0 / 3928.0 | -49.52 % | 0 | 1244.5 / 641.1 / 631.9 | -49.2 % | -1.4 % (equal instructions) |
| programs/binary_trees | 7301.4 / 5526.5 / 5526.5 | -24.31 % | 0 | 1552.3 / 1206.4 / 1190.8 | -23.3 % | -1.3 % (equal) |
| awfy/permute | 235.5 / 195.3 / 195.3 | -17.04 % | 0 | 43.9 / 36.2 / 36.0 | -18.2 % | -0.6 % (equal) |
| awfy/towers | 179.2 / 175.3 / 175.3 | -2.17 % | 0 | 49.0 / 47.7 / 48.4 | -1.1 % | +1.5 % (equal) |
| awfy/list | 412.7 / 395.7 / 397.3 | -3.74 % | +0.40 % | 97.1 / 93.8 / 93.9 | -3.3 % | +0.06 % |
| awfy/cd | 770.8 / 774.4 / 774.4 | +0.46 % | 0 | 260.2 / 262.3 / 264.9 | +1.8 % | +1.0 % (equal) |
| apps/raytrace | 366.3 / 366.7 / 366.6 | +0.08 % | -0.03 % | 114.4 / 114.0 / 113.6 | -0.7 % | -0.3 % |
| awfy/richards | 177.0 / 177.2 / 177.2 | +0.13 % | 0 | 60.3 / 60.7 / 60.4 | +0.2 % | -0.5 % (equal) |
| awfy/deltablue | 323.3 / 324.1 / 324.1 | +0.25 % | 0 | 141.0 / 170.4 / 138.4 | -1.8 % | (equal; `off` reading disturbed) |
| awfy/havlak | 515.8 / 516.0 / 516.1 | +0.07 % | +0.03 % | 292.5 / 295.3 / 313.5 | +7.2 % | +6.2 % (equal: noise, see below) |
| awfy/queens | 511.7 / 511.8 / 511.8 | +0.03 % | 0 | 143.5 / 161.5 / 142.2 | -0.9 % | (equal) |
| programs/queens | 5391.4 / 5391.6 / 5391.6 | 0 | 0 | 2377.2 / 2352.9 / 2406.1 | +1.2 % | +2.3 % (equal) |
| corpus2/dev/coro_generators | 4096.3 / 4036.2 / 4028.8 | -1.65 % | -0.18 % | 1254.2 / 1263.8 / 1253.6 | -0.05 % | -0.8 % |

`new` and `off` are the same binary: where their instruction counts are equal, the trace sets are equal and a cycle difference is measurement spread
(this round shared the machine with another builder's study: havlak, deltablue, queens moved by 6-19 % between configurations that execute the same
instructions). The E-stage gains are intact (fib -49.5 %, binary_trees -24.3 %, permute -17.0 % instructions against b116447).

**Sanity set** (instructions, one run each, 5.5): the 14 awfy programs, the 25 of `bench/programs`, the 5 apps, the 35 jit-trace programs and the six
other programs with deep sites (`corpus/{astar_jumper,parser_combinator}`, `corpus2/dev/{coro_generators,expr_compiler,lua_parser}`,
`corpus2/heldout/scheme_interp`): `new` against `off` within 0.05 % everywhere except `rec_loop` (-13.97 %), `awfy/list` (+0.40 %) and
`coro_generators` (-0.18 %); `corpus/astar_jumper` is not deterministic in either configuration (bimodal, 4.80 / 5.55 G instructions from run to
run, no probe dropped: `deep recordings 2 probes dropped 0`). Nothing above +1 % instructions.

**Formation sensitivity.** The programs with deep sites are sensitive to the binary path and the environment (pointer-keyed hashing moves the
formation): `scheme_interp` closes a deep recording with the absolute binary path and none with a relative one; the numbers above use the bench
method (absolute, equal-length paths). `awfy/list` moves by the same +0.4 % under both environments tried.

**Lua 5.4** (the same binaries' 5.4 builds, `target-54`; instructions deterministic): `rec_loop` 441.3 / 519.5 / 447.7 M (b116447 / off / new:
**+1.45 %** against b116447, **-13.82 %** against off); fib, binary_trees, permute, towers, cd, raytrace, richards, deltablue, havlak, queens,
scheme_interp, parser_combinator, lua_parser, expr_compiler: `new` = `off` within 0.01 % (the E-stage gains against b116447 as on 5.5: fib
-49.5 %, binary_trees -21.2 %, permute -16.5 %); `awfy/list` +0.39 %, `coro_generators` +0.33 %, `apps/expr_lang` **-1.76 %** (its site probes on
5.4). The 5.4 cycle round ran while another builder's test suite held the machine at load 31 (havlak +12 % at equal instructions): not usable,
not reported.

### 38.6 Tests and mutants

* `tests/vm/e10_01_deep_probe.lua` (also in `vm_corpus`; same output as the reference 5.5 and 5.4): the `rec_loop` shape, depths with an even
  period, a rare deep iteration, a recursion deeper than any limit, a callee switched while the site probes and after its trace is installed, errors
  from depth under `pcall`, tables and floats through the recursion.
* `tests/vm/e10_02_deep_probe_tries.lua`: three sites with known counts (tries dropped until a deep iteration comes round again, a uniform site, a
  site whose recording aborts after it became deep).
* `tr_core::dp_deep_probe_programs`: `e10_01`, `e10_02`, `j2_deep` x `always` / `on2` x `deepprobe` 0 / 3 / 12, `+verify`; for 12: exit storm,
  budgets 3 / 17 / 200, GC stress 1 / 2 in both collector modes, stack-move stress; every job against the interpreter. Pins `e10_02` in `always`
  mode at 3 deep sites and **17 dropped recordings** (12 probes and 2 tries, 3 uniform probes, none at the aborting site); none with `deepprobe=0`;
  at least 12 for `e10_01`.
* `tr_policy::dp_policy`: the knob (struct default = shipped, parse, cap), probes, the 7/8 rule at its boundary, the cap on tries, the uniform rule,
  the abort rule, a non-deep head, knob 0.
* Mutants (registry 792 -> **796**): `deepprobe_never` (no probing), `deepprobe_any_try` (the first try after the probes taken whatever its length),
  `deepprobe_uniform_keep` (no uniform rule), `deepprobe_abort_keeps` (aborts do not end the probing). All four caught by `tr_policy::dp_policy`;
  the first three also by `dp_deep_probe_programs` (17 -> 0 / 15 / 26 dropped recordings).

### 38.7 Gates, what ran and what did not, risks

**What is left of the regression, and why.** Against b116447 `rec_loop` stays +1.5 % instructions / +3.0 % cycles (5.4: +1.45 % / +3.1 %,
min of 15, 78.6 / 81.0 Mcyc). That is a fixed cost, not a per-iteration one: the probes (14 recordings that are not compiled, `rec_step`
+1.0 Mtsc) and three more side traces of the later-forming tree. With four times the work (`rec_loop.lua 24000`) the difference is +0.38 %
instructions (1686.0 -> 1692.5 M) and +0.89 % cycles (291.8 -> 294.3 M, min of 5). Getting b116447's exact number back at the default threshold
would need its lucky phase, which no policy can promise (at 18 of the 19 thresholds b116447 is slower than the new binary, by up to 17 %). The
target of "within +2 %" is met for instructions, not for cycles at the default size (+3.0 %); the stage behind the regression is E1 (38.1), and
the regression was its phase shift, which this section removes from the formation instead of from E1.

**Gates.** `scripts/gate-light.sh --base 2222703` (GATE_CPUS=2-9) on the commit before the last test change: fmt, mutant map, clippy 5.5 / 5.5-jit
/ 5.4, core 5.5 / 5.4, trace suites 5.5 (193 s) / 5.4 (188 s), `vm_corpus` (with `e10_01` / `e10_02` against the reference 5.5), jit-diff 1-50 PASS;
the mutant group (115 mutants) ran, 114 caught: `rec_inner_first_stale` survived. Its detector `tr_core::pm4_recursion_entry_counters` depends on
the formation of `pm4_rec_counters`, whose loop is a deep site: with probing the stale-first-word case is no longer reached. The job is now pinned
to `deepprobe=0` (the formation the rule was written on, like `bc_sbump_programs` with `recunroll=0`): caught again
(`gate-mutants.sh --only rec_inner_first_stale,deepprobe_`: 5 ran, 5 caught on 5.5 and on 5.4). A second `gate-light.sh` ran on the final commit
(all steps PASS, trace suites 175 s / 166 s, mutant group 115 ran, 115 caught, 506 s). An earlier run failed at clippy (`tr_asm.rs` builds `HeadData` literally: the three new fields were missing) and the
mutant-map step (stale map); both fixed before the runs above. **Not run** (by instruction): `gate-heavy.sh`, the oracle-feature suites
(`tr_identity`, `instruction_count_identity`, `oracle_options`), jit-diff 1-200 in seven modes, the full mutant runner (796), no full benchmark.

**Risks.** (1) The path length is a proxy for "the deepest call tree"; a loop whose longest iteration is a rare path (an error branch with deep
recursion) would now record that path and leave the common one to side traces. The 7/8 rule and the 12-try cap bound the delay, not that choice;
no measured program has such a site (the deep sites of the sanity set: `rec_loop`, `list`, `coro_generators`, `expr_lang` on 5.4 probe; the
others abort first). (2) Probes cost recordings: a deep site with very long iterations (bounded by `deepins` = 300 IR instructions) pays up to 24
of them once. (3) The formation of programs with deep sites stays sensitive to the binary path and environment (38.5); the probes make
`rec_loop` itself insensitive to the phase, not the rest of such programs. (4) Tests that pin formations of deep-site programs may need
`deepprobe=0` (as `pm4_recursion_entry_counters` did); the full mutant runner has not run with the new default.

## 39. T1 / T3 / T2 of docs/STUDY_TIER1_RESIDUE.md: RETF for function-entry families by selector, side traces beyond `maxside` by evidence, cheaper compile (2026-11-22, base 23d1692)

Three handles of the tier-1 residue study, built in its recommended order, each measured before the next. Measurement method for every table of this
section unless it says otherwise: release `rn-cli` (5.5), `RUSTYNAIL_SEED=0`, one binary per handle with the new knob(s) off as the reference column
(every earlier stage on), binaries copied to equal-length paths, the knob strings padded to equal length (`RUSTYNAIL_JIT_TRACE_LIMITS` + a pad
variable), instructions of one run (deterministic; parallel runs on CPUs 2-15 except 4), cycles min of 5 interleaved runs pinned to CPU 4 with no gate
or other build running. "Suite" = the 223 programs of the bench sets (`corpus`, `corpus2/dev`, `corpus2/heldout`, `awfy`, `programs`, `jit-trace`,
`lib`, `apps`, `real`), which contain the 44-program sanity set (awfy, `bench/programs`, apps) and the jit-trace set.

### 39.1 T1 (`retfsel`): RETF for function-entry families where the caller's loop is traced and no recursion is popped

**The bisection the study left open.** A scratch build (reverted) took a per-family list (`RN_SCRATCH_RETF=<line>@<head pc>:...`) and set `retf` bit 2
only for side recordings of those families; each program's families with RETF-capable side recordings were enabled one at a time against all and
none (instructions, `retf=3` reference):

| program | all (`retf=7`) | families that move it (alone) |
|---|---:|---|
| corpus/lua_lexer | -17.1 % | `Lexer:next` L80 loop families pc 1 (-14.0 %) and pc 99 (-3.1 %); together -16.8 % |
| awfy/queens | -16.5 % | L16 `tryIt`'s loop family pc 4: -16.1 % (pops into `tryIt` itself: recursion) |
| corpus2/dev/expr_compiler | -9.5 % | L174 `run`'s loop family pc 7: -9.1 % (pops into the main chunk's loop, which has no root trace: `InnerLoop`) |
| corpus2/heldout/roguelike_turns | +112 % | L65 `line_clear`'s loop family pc 26: +100.5 %; L120 pc 7 +1.0 %, L194 pc 15 +0.8 % |
| awfy/list | +55 % | L31 `tail`, continuation family pc 2: +55.1 % (recursion) |
| real/lua_interp | +35 % | L102 `eval`, function-entry family: +34.9 % (recursion) |
| corpus2/dev/regex_nfa | +20 % | L144 `match`'s loop family pc 72: +19.8 % (pops into the main loop, no root trace) |
| awfy/deltablue | +2.9 % | L303 pc 6 +3.0 %, L198 fn +1.2 %; L216 pc 5 -1.7 % |
| corpus2/heldout/sudoku_solver | +16.5 % | L51 `eliminate`'s loop family pc 59 (+15.3 %: mutual recursion through `assign`) |

So the paying families are mostly **loop** families of prototypes that also have a function-entry head (`head_is_fn(pr, 0)` makes every side
recording of the prototype `fn_root`, and `fn_root` sides pop only with bit 2), which is why the study's "loop-root families only" scratch should have
kept lua_lexer; its zero there was most likely a defect of that scratch (not re-examined). But the losers are loop families too, so "loop root" is not the
selector either. The loss mechanisms, read from verbose logs and profiles:

* **Recursion** (list, lua_interp, sudoku_solver, part of deltablue): the side trace pops into a frame of the same recursion and inlines the next
  level; the work that the recursion's own traces did (entries of `eval`'s family 836 k -> 477 k) moves into chains of side traces that exit to
  tier 1. queens is the counter-example (its recursion pays).
* **Untraced caller loop** (regex_nfa, deltablue 303): the continuation in the caller reaches a callee loop without a root trace and the RETF
  recording aborts `InnerLoop` (8 of 8 in regex_nfa); the exits get `noretf` and are re-recorded, and the program forms differently (the tier-1 share of
  `match` 5.5 -> 10.8 %).
* **A formation lottery in an unrelated loop** (roguelike_turns): one RETF side trace (`line_clear` -> `compute_fov` -> frame link back into
  `line_clear`'s loop) changes when `run_level`'s monster loop (L234 pc 96) is recorded; with it the recording starts at the first iteration of an
  `ipairs` loop whose `who` slot is nil there and a table afterwards, aborts `Unstable` four times (in-place restarts are 0 in the pipeline) and the
  head is blacklisted: tier 1 15.9 -> 69.8 %. The same build with TR24c in-place restarts on (`restarts = 1`, scratch) is +2.2 % instead of +100 %
  (and moves raytrace by -3.9 % on its own); not part of this handle (a global formation change), recorded as a lead.

**Selector as built.** Knob `retfsel` (`RUSTYNAIL_JIT_TRACE_LIMITS`, 0 / 1, **default 1**, struct default = shipped). With `retf` bit 2 off (the
shipped `retf=3`), a side recording of a function-entry family (`RecCfg::fn_root`) may execute `RETF` at its root `RETURN` exactly when
`cache::retf_sel_rule(caller_loop, recursive)` holds:

* `caller_loop`: the head of the innermost loop of the caller `C` that contains `C`'s `CALL` (`cache::enclosing_loop_head`: the largest target `h` of
  a backward `JMP` / `FORLOOP` / `TFORLOOP` at `j` with `h <= call_pc < j`), asked through the existing `retf_head` query (`head_traced`): it must have
  an installed root trace (`Some(true)`). A head without one, a call outside every loop, or a pc that is not a head refuse.
* `recursive`: the popped prototype is among the `RETFSEL_DEPTH` (16) frames from `C` down (`C` itself included). Any such frame refuses.

The question is asked in `Rec::on_step` (where the state is at hand) at a `RETURN*` of the root frame before the first pop, and `Rec::ret` reads the
answer (`retf_sel_ok`); a refusal keeps the old behaviour (`fn_end`: the side trace ends before the `RETURN`), it never aborts or waits. Both
conditions were measured against alternatives (scratch, suite instructions): the caller-loop rule alone keeps lua_lexer -7.6 / queens -16.1 and
removes list, lua_interp, regex_nfa, deltablue, but leaves sudoku_solver +11.3 %, lua_parser +3.4 %, cd +1.2 %; refusing self-pops only (caller =
popped prototype) loses queens and leaves sudoku; the frame walk catches the mutual recursion. **Waiting** for an untraced caller head instead of
refusing (the exit `SideUntraced`-waits until the head is traced, always or once per exit, or only for heads without aborts) recovers lua_lexer's other
half (-16.5 %) and scheme_interp -5.6 %, but costs json_rxi +15.4 %, csv_report +7.9 %, dijkstra_routes +3.4 %: a deferred side trace there
changes the formation of the whole program. Not built.

Why lua_lexer keeps only half of `retf=7`: 36 of its RETF recordings pass (caller loop L0 pc 127 traced), 9 are refused because the main loop's
trace is not installed yet or was flushed (`Dep`) at that moment, 6 are pops of the `skip_long_bracket` / `read_string` families into `Lexer:next` at call sites outside its
loops.

**Soundness.** The selector only restricts where an existing mechanism is used: every RETF it allows is one `retf` bit 2 (`retf=7`, covered by
`nr_matrix` / `nr_n3_programs` in all modes) allows, through the same `Rec::retf` with every condition of N2c / N3a unchanged, including I9 (no pop
into a maybe-lazy caller: `retf_nolazy`), the lazy-root and `no_retf` exclusions (`retf_head` is `None` there, so the selector answers no), the
caller being a Lua frame suspended at an `OP_CALL`, the `RETURN` operands, upvalue aliasing (`RetfAlias`). The answer depends on the head state at
recording time only; a trace recorded under it stays correct if the caller's root trace is flushed later (the RETF trace's link goes through the
head's cell, which then leads to tier 1, as for every side trace that links). Snapshots, hooks (a hook aborts the recording), errors after the pop,
coroutines and GC see the same RETF traces as under `retf=7`. Lua 5.4: RETF is inert there by design (C63), so `retfsel` changes nothing on 5.4.

**Measured** (instructions; cycles min of 5, CPU 4; reference = same binary `retfsel=0`):

| program | instr M off / on | instr | cycles M off / on | cycles |
|---|---|---:|---|---:|
| awfy/cd | 773.9 / 774.0 | +0.02 % | 262.3 / 262.4 | +0.01 % |
| apps/raytrace | 364.8 / 364.9 | +0.02 % | 114.8 / 113.4 | -1.21 % |
| corpus/lua_lexer | 4415.5 / 4078.9 | -7.62 % | 1095.0 / 1022.1 | -6.66 % |
| awfy/richards | 177.1 / 177.2 | +0.01 % | 59.4 / 59.9 | +0.86 % |
| awfy/deltablue | 324.3 / 324.4 | +0.03 % | 108.5 / 109.1 | +0.55 % |
| awfy/havlak | 512.0 / 512.8 | +0.15 % | 157.3 / 153.1 | -2.67 % |
| awfy/list | 397.2 / 397.2 | +0.01 % | 94.6 / 94.4 | -0.30 % |
| awfy/queens | 511.8 / 511.8 | +0.00 % | 96.2 / 97.0 | +0.77 % |
| awfy/permute | 195.3 / 195.3 | +0.00 % | 36.0 / 36.2 | +0.51 % |
| awfy/towers | 175.3 / 175.3 | +0.01 % | 47.0 / 46.8 | -0.50 % |
| programs/fib | 3928.0 / 3928.0 | +0.00 % | 637.5 / 635.1 | -0.38 % |
| programs/binary_trees | 5635.3 / 5635.3 | +0.00 % | 1247.0 / 1251.4 | +0.36 % |
| corpus/classic_vector | 2053.9 / 2053.9 | +0.00 % | 474.7 / 472.3 | -0.50 % |
| corpus2/dev/oop_mixins | 5548.5 / 5548.6 | +0.00 % | 1492.5 / 1511.9 | +1.30 % |
| corpus2/heldout/roguelike_turns | 2776.6 / 2799.4 | +0.82 % | 811.0 / 823.7 | +1.56 % |
| corpus2/dev/regex_nfa | 3609.2 / 3609.4 | +0.00 % | 691.8 / 691.6 | -0.03 % |
| real/lua_interp | 561.9 / 561.9 | +0.00 % | 106.0 / 105.7 | -0.23 % |
| corpus2/heldout/scheme_interp | 5882.3 / 5882.4 | +0.00 % | 1441.5 / 1448.8 | +0.50 % |
| corpus2/dev/expr_compiler | 5691.5 / 5691.7 | +0.00 % | 1323.0 / 1330.7 | +0.58 % |
| corpus/astar_jumper | 4821.1 / 4988.5 | +3.47 % | 1674.8 / 1699.7 | +1.48 % (not deterministic, see below) |
| corpus2/heldout/dijkstra_routes | 3057.1 / 3021.8 | -1.15 % | 1192.1 / 1183.1 | -0.76 % |
| awfy/json | 455.2 / 452.0 | -0.71 % | 140.2 / 141.0 | +0.51 % |
| corpus2/heldout/prototype_objects | 3621.4 / 3643.6 | +0.61 % | 1685.8 / 1676.2 | -0.57 % |
| corpus2/dev/lua_parser | 5083.2 / 5097.0 | +0.27 % | 1313.8 / 1329.8 | +1.22 % |

Suite (223 programs, instructions): geomean 0.9993; better than -0.2 %: lua_lexer -7.62, dijkstra_routes -1.15, json (awfy) -0.70; worse than +0.2 %:
roguelike_turns +0.82 (the formation lottery above, smaller), prototype_objects +0.61 (cycles -0.6 %), lua_parser +0.27; `corpus/astar_jumper` is not
deterministic in either configuration (four modes 4.80 / 4.98 / 5.38 / 5.55 G instructions from run to run, ASLR; 72 runs each: mean 5207 M off,
5136 M on, low mode 29 / 72 against 39 / 72): no change attributable. Nothing else moves by more than 0.2 %. Lua 5.4: inert (see above).

**Tests and mutants.** `tests/vm/t1_01_retfsel.lua` (lexer shape: a scanner with two loops returning tokens to a hot caller loop; the caller loop inside
a repeatedly called function with table stores and floats in the continuation; an error raised in the caller after the pop under `pcall` with the
message checked; a coroutine driver that yields every 500 tokens; collections in the continuation) and `tests/vm/t1_02_retfsel_recursion.lua` (a
backtracking search whose loop recurses, a mutual recursion through two functions, both called through a wrapper that has no loop), both in
`vm_corpus` (same output as the reference 5.5 and 5.4). `tr_core::t1_retfsel_programs`: both x `always` / `on2` x `retfsel` 0 / 1, `+verify`; for the
shipped value exit storm, emulator, budgets 3 / 17 / 200, GC stress 1 / 2 in both collector modes, stack-move stress; every job against the
interpreter; on 5.5 `t1_01` installs more RETF traces with the selector than without it (`always`), `t1_02` the same number (the selector pops nothing
inside the recursions; bit 0 RETF of loop families exists in both); on 5.4 none anywhere. `tr_policy::t1_retfsel_policy`: the knob (default, parse,
cap), the rule's truth table, and `enclosing_loop_head` on compiled bytecode (nested numeric, generic and `while` loops, calls outside every loop).
Mutants (registry 796 -> 800): `retfsel_never` (caught by both tests), `retfsel_untraced_ok` (policy), `retfsel_recursion_ok` (both),
`retfsel_outer_loop` (policy: the outermost loop chosen); `gate-mutants.sh --only retfsel_`: 4 ran, 4 caught.

### 39.2 T3 (`heatside`): side traces beyond `maxside` for exits that keep coming back

**Starting point.** A family holds at most `maxside` (16) side traces; past that, an exit that gets hot is blacklisted at its first report
(`MaxSide`), except dispatch-guard exits inside at least two inlined frames, which BUNDLE-O's `polyside` defers and then admits up to `maxside +
polyside`. The study's ceiling `maxside=64` (same binary, instructions): oop_mixins -30.8, vending_fsm -5.9, heapsort -4.8, regex_nfa -4.2,
lua_lexer -4.8, richards (250) -7.7; but cd +5.1, expr_compiler +17.5, lru_cache +5.0, particles_physics +3.5, raytrace +0.6.

**Rule as built.** Knobs `heatside` (extra side traces per family, **default 48**, 0 = off, at most 256), `heatwait` (reports before a grant,
**16**, 1..255), `heatbudget` (grants per state, **64**), `heatdepth` (**8**); struct defaults = shipped. In `side_attempt`, after the `polyside`
branch, an exit of a full family that would be blacklisted `MaxSide` is instead **deferred** (`side_failed(.., fault false, "HeatWait")`: the exit
stays `Free`, its counter restarts, so it reports again after `hot_exit` more exits) while `cache::heat_side_rule` holds: the family holds fewer than
`maxside + heatside`, the state has granted fewer than `heatbudget`, and the exit's trace lies at most `heatdepth` side links below its family root
(`side_depth`). Once the exit has been reported `heatwait` times since its family filled up (its own counter `hdefer`; about `heatwait x hot_exit`
= 256 more exits in the default mode, deterministic), it gets its side recording like any free exit (`heat_waited`), counted in `heat_granted`
(`[trace] heatside: granted N`). When the rule no longer holds the exit is blacklisted `MaxSide` as before. Everything after the grant is the
ordinary side-trace path; the rule only decides whether an exit of a full family is recorded.

Two refinements came from measurements of the first form (cap and wait only):

* **Depth limit.** expr_compiler stayed at +18 %: its grants formed one chain, each new side trace hanging off the previous one at the same exit
  (exit 4, pc 0, depths 16 -> 63). The deepest grant of every paying program is at depth 6 or less (oop_mixins 6, regex_nfa 5, lua_lexer 5,
  vending_fsm 4, heapsort 3, richards 2); `heatdepth` 8 stops the chain (expr_compiler -0.6 %). Depth 2 / 4 / 6 / 8 measured: 4..8 equal except
  real_dis_mips (5.5) -3.7 % from depth 6 on.
* **The `loopany` guard counts a deferred exit once.** 5.4 `dijkstra_routes` was +6.6 %: every report of a deferred exit inside inlined frames passed
  through the `loopany` guard's attempt counter (`la_sides`), which tripped at 49 "attempts" and flushed the family, and the re-formed tree lost
  `L89`'s loop trace (`Ineffective`). A report of an exit with `hdefer > 0` is not counted again (`heat_redeferred`): +0.15 %.

Rejected variants (measured): "a granted side trace's own exits get no grant" (one level per exit tree): roguelike unchanged, oop_mixins -20.5
instead of -28.8; `heatwait` 4 / 32 / 64 / 128: oop_mixins -29.5 / -27.3 / -24.9 / -19.6, vending_fsm -6.2 / -4.9 / -4.8 / +2.7, richards default
size -0.0 / +1.1 / +1.7 / +3.7, particles_physics +3.3 at 4 and 64 (a lottery: -0.05 at 16 and 32): 16 kept.

**Soundness.** A grant is an ordinary side recording of an exit that a non-full family would have recorded at its first report; the deferral only
changes *when* and *whether* that happens (the exit state is `Free` instead of `Black`, its counter restarts as after any non-fault refusal such
as `Busy` or `PolyWait`). No new code shape, no new exit or link kind: invariants I1-I10, snapshots, the side-trace protocol and its tests apply
unchanged. Bounded cost: per family at most `heatside` extra traces, per state at most `heatbudget` extra recordings, per deferred exit at most
`heatwait` extra handler reports. The deferred exit's counter restart reaches the ineffective-trace rule (Q2) as the restarts of `PolyWait` do;
the `loopany` guard is protected explicitly (above). Lua 5.4: the rule is version-independent and active there too.

**Measured.** Instructions, 5.5, same binary `heatside=0` against `heatside=48` (the other heat knobs at their defaults):

| program | instr M off / on | instr | cycles M off / on | cycles |
|---|---|---:|---|---:|
| awfy/cd | 774.0 / 774.1 | +0.01 % | 261.3 / 261.1 | -0.08 % |
| apps/raytrace | 365.0 / 365.0 | +0.01 % | 113.8 / 115.8 | +1.73 % (15 runs: 113.5 / 113.2, -0.2 %) |
| corpus/lua_lexer | 4078.9 / 3951.7 | -3.12 % | 1019.8 / 984.2 | -3.49 % |
| awfy/richards | 177.2 / 177.8 | +0.34 % | 59.6 / 61.5 | +3.20 % (15 runs: 60.3 / 63.2, +4.7 %) |
| awfy/deltablue | 324.4 / 324.4 | +0.00 % | 107.5 / 106.8 | -0.63 % |
| awfy/havlak | 512.9 / 513.0 | +0.03 % | 157.1 / 164.5 | +4.72 % (10 runs: 153.1 / 150.8, -1.5 %) |
| awfy/list | 397.3 / 397.3 | -0.00 % | 94.4 / 94.3 | -0.13 % |
| awfy/queens | 511.8 / 511.8 | +0.00 % | 96.1 / 95.4 | -0.71 % |
| awfy/permute | 195.4 / 195.4 | -0.00 % | 36.3 / 36.4 | +0.08 % |
| awfy/towers | 175.3 / 175.3 | -0.00 % | 47.8 / 47.9 | +0.25 % |
| programs/fib | 3928.0 / 3928.0 | -0.00 % | 638.2 / 631.2 | -1.10 % |
| programs/binary_trees | 5635.3 / 5635.3 | -0.00 % | 1242.2 / 1243.6 | +0.11 % |
| corpus/classic_vector | 2054.0 / 2054.0 | -0.00 % | 477.6 / 479.5 | +0.40 % |
| corpus2/dev/oop_mixins | 5549.0 / 3949.0 | -28.83 % | 1487.0 / 1161.5 | -21.89 % |
| corpus2/heldout/roguelike_turns | 2799.4 / 2836.6 | +1.33 % | 826.4 / 847.4 | +2.54 % (10 runs: +3.6 %; lottery, see text) |
| corpus2/dev/regex_nfa | 3609.4 / 3455.4 | -4.27 % | 663.6 / 659.6 | -0.59 % |
| real/lua_interp | 561.9 / 561.9 | -0.00 % | 105.5 / 106.2 | +0.60 % |
| corpus2/heldout/scheme_interp | 5882.5 / 5882.5 | -0.00 % | 1443.1 / 1454.1 | +0.76 % |
| corpus2/dev/expr_compiler | 5691.8 / 5657.1 | -0.61 % | 1308.2 / 1314.7 | +0.50 % |
| corpus/astar_jumper | 4838.5 / 4811.3 | -0.56 % | 1706.2 / 1786.5 | +4.71 % (not deterministic) |
| corpus2/heldout/vending_fsm | 3742.6 / 3524.2 | -5.83 % | 1033.2 / 987.0 | -4.47 % |
| programs/heapsort | 2913.9 / 2779.0 | -4.63 % | 1114.3 / 1105.5 | -0.79 % |
| corpus2/dev/real_dis_arm | 7424.8 / 7466.2 | +0.56 % | 2476.1 / 2506.1 | +1.21 % |
| corpus2/heldout/real_dis_mips | 7264.6 / 7273.9 | +0.13 % | 2219.0 / 2211.4 | -0.34 % |
| corpus/containers_lru_heap | 5503.3 / 5466.8 | -0.66 % | 1313.6 / 1318.9 | +0.40 % |
| corpus2/dev/memo_cache | 4961.6 / 4940.4 | -0.43 % | 1699.2 / 1730.8 | +1.86 % |
| corpus/testrunner_luaunit | 5304.6 / 5304.6 | -0.00 % | 1459.4 / 1463.3 | +0.27 % |
| corpus2/heldout/dijkstra_routes | 3021.8 / 3011.3 | -0.35 % | 1178.4 / 1188.9 | +0.88 % |

Suite (223 programs, 5.5): geomean 0.9972; better than -0.2 %: oop_mixins -28.83, vending_fsm -5.83, heapsort -4.63, regex_nfa -4.27, lua_lexer
-3.12, containers_lru_heap -0.67, expr_compiler -0.61, memo_cache -0.43, dijkstra_routes -0.35, testrunner_luaunit -0.30 (astar_jumper -10.7: not
deterministic, see 39.1); worse than +0.2 %: roguelike_turns +1.33, real_dis_arm +0.57, richards +0.34. Lua 5.4 (128 programs of the same sets
without `lib`): geomean 0.9964; vending_fsm -11.2, oop_mixins -27.7, forth_vm -3.7, expr_compiler -1.4; real_dis_mips +2.09, real_dis_arm +0.80,
queens +0.70, roguelike_turns +0.97, richards +0.39.

The programs above +1 % are formation lotteries, measured across hot thresholds (`RUSTYNAIL_JIT_TRACE_HOT` 248..264, step 2, nine runs each, same
binary): roguelike_turns (5.5) off 2797..6033 M, mean 3217, on 2784..5767, mean 3193 (-0.8 %; the shipped threshold 256 is the off column's
luckiest phase); real_dis_mips (5.4) off 7338..7628, mean 7487, on 7437..7689, mean 7533 (+0.6 %; again 256 is off's best phase). richards is
threshold-sensitive in both columns (off 176..201, on 177..248 M at single thresholds).

**Cycles** (min of 5, CPU 4; richards / raytrace / havlak / roguelike re-run with 10-15 runs because the first round moved by up to 9 % at equal
instructions): oop_mixins -21.9 %, vending_fsm -4.5 %, lua_lexer -3.5 %, heapsort -0.8 %, regex_nfa -0.6 %; **richards (default size) +4.7 %** at
+0.34 % instructions (15 runs: 60.3 against 63.2 Mcyc): the 13 granted side traces of its scheduler family do not reduce its exits (278 k in both)
and add branch mispredictions (0.33 M -> 0.41 M) and L1i misses (0.80 M -> 0.85-1.00 M) in a 60 Mcyc program; at `richards 250` the same rule is
-7.4 % instructions, -7.3 % cycles (1422 -> 1317 M, 456 -> 423 Mcyc): the study's large-size target. roguelike_turns +2.5..3.6 % (the formation
lottery above). Everything else within noise at equal instructions.

**Tests and mutants.** `tests/vm/t3_01_heatside.lua` (a 40-way branch chain in one hot loop; a chain whose late branches stop being taken; an error
raised inside a late branch under `pcall`, a collection in another; same output as the reference 5.5 and 5.4). `tr_core::t3_heatside_programs`:
`always` / `on2` x `heatside` 0 / 48, `+verify`; exit storm, emulator, budgets 3 / 17 / 200, GC stress 1 / 2 in both collector modes, stack-move
stress for the shipped value; every job against the interpreter; in `always` mode the shipped rule grants (>= 8), `heatside=0` grants nothing,
`heatbudget=3` exactly 3, and `heatside=4`, `heatwait=255`, `heatdepth=0` each fewer than the shipped values. `tr_policy::t3_heatside_policy`: knob
defaults / parse / caps, the rule at each boundary, `heat_waited`, `heat_redeferred`. Mutants (800 -> **806**): `heatside_never`,
`heatside_cap_ignored`, `heatside_budget_ignored`, `heatwait_ignored`, `heatdepth_ignored` (each caught by both tests), `heatdefer_counts_la`
(policy test); `gate-mutants.sh --only heatside_,heatwait_,heatdepth_,heatdefer_`: 6 ran, 6 caught.

### 39.3 T2: cheaper compile at small sizes (no change to the emitted code)

**Profile.** Instruction samples (`perf record -e instructions:u -c 100000`, 4-6 runs merged per program, flat) of the T3 binary at the bench default
size, then a scratch build with frame pointers (`RUSTFLAGS=-C force-frame-pointers=yes`, its own target directory, removed) for call graphs
(`perf record --call-graph fp --user-callchains`; plain `-g` is refused at `perf_event_paranoid` 2). `[trace] phases` of raytrace: `rec_step` 19.7
Mtsc for 12.5 k recorded bytecodes (about 1.6 k tsc each) against `pipeline` 19.8 Mtsc for 45 compiles: the **recorder** costs as much as the whole
back end, and inside it (inclusive, fp profile, raytrace): `on_step` 18.3 % of all instructions, of which `SlotMap::entries` 3.7 % (3.1 % of it
`Vec::push` regrowth), `SlotMap::clone_from` 2.3 % (malloc / free), table-key recording (`hash_get_chain`) 4.6 %, IR emission and folding; the back end
(`pipeline_inner` 17.5 %): register allocation 3.7, exit building 3.1, assembler 2.8, sinking 2.3, loop optimisation 1.8; tier-1 compiles
(`rt::compile`) 4.1 % (7.1 % on deltablue), whose x86 encoder (`Assembler::rm_op`) is 1.8-2.9 % self.

**Fixes (all four keep the emitted code identical):**

1. **`SlotMap::clone_from` without allocation.** The recorder copies its slot map before every step (`Rec::st_sm`, the restart state of an
   inlined metamethod); `#[derive(Clone)]` gives `clone_from = *self = source.clone()`, i.e. six allocations and six frees per recorded bytecode.
   A hand-written `Clone` copies field by field into the existing vectors.
2. **`SlotMap::entries` over a bit set of the written slots** (`wmask`, kept in step with `written` by `def` / `set_written`; the two places in
   `record.rs` that wrote `written` directly use `set_written`), with the result vector sized once by the popcount. It scanned every slot of the
   frame for every snapshot and grew its result one entry at a time. The order (ascending slot) and the filter are unchanged.
3. **`entry_checks` without the quadratic membership test** (`moved.contains(i)` for every instruction of the trace: now a mask).
4. **The registered-descriptor set `JitData::live` on FxHash** (looked up by every head query of the recorder; never iterated, so no order can
   change).

Not done (measured, larger and riskier): the tier-1 encoder, the recorder's table-key path, the back-end passes; a lazy copy instead of the per-step
slot-map copy (an undo log) was not attempted.

**Identical code.** `RUSTYNAIL_JIT_DUMP` of both binaries (the IR and register-allocation text of every installed trace with addresses normalised,
the machine-code size of every trace and of every tier-1 prototype) and the `[trace]` statistics lines (timing lines excluded), compared
program by program (`ident.py` in the builder's scratch): **81 of 83 identical** (14 awfy, 25 `bench/programs`, 5 apps, 29 jit-trace, 10 programs of
the key set); the two others are the run-to-run non-deterministic `oop_mixins` (its `entered` / `exits` counters differ between two runs of the
same binary; its traces are identical) and `astar_jumper` (formation differs between runs of one binary). Unit test
`snap::tests::entries_by_mask_equal_the_full_scan` (4000 random definitions, redefinitions, cleared flags, growth across mask words and `clone_from`
into a map of another size, against the old scan).

**Instructions saved** (default size, same environment; suite of 223: geomean 0.9983, no program above +0.06 % except the non-deterministic
`real_dis_x86`, 5010-5105 M from run to run in either binary):

| program | instr M before / after | instr | cycles M before / after | cycles |
|---|---|---:|---|---:|
| apps/raytrace | 365.0 / 346.8 | -4.99 % | 111.7 / 106.9 | -4.30 % |
| awfy/cd | 774.1 / 763.2 | -1.42 % | 256.3 / 252.7 | -1.41 % |
| awfy/deltablue | 324.4 / 318.6 | -1.79 % | 106.7 / 105.0 | -1.52 % |
| awfy/richards | 177.8 / 176.4 | -0.79 % | 60.9 / 60.5 | -0.77 % |
| awfy/towers | 175.3 / 173.6 | -0.96 % | 46.8 / 45.9 | -1.88 % |
| awfy/havlak | 512.9 / 508.3 | -0.89 % | 147.7 / 145.1 | -1.72 % |
| awfy/list | 397.3 / 395.2 | -0.52 % | 93.2 / 92.2 | -1.08 % |
| corpus/classic_vector | 2054.0 / 2038.7 | -0.74 % | 476.6 / 466.8 | -2.04 % |
| apps/expr_lang | 573.9 / 569.3 | -0.80 % | 146.1 / 146.4 | +0.20 % |
| apps/lru_cache | 322.8 / 318.8 | -1.23 % | 141.7 / 139.7 | -1.42 % |
| awfy/json | 452.0 / 448.7 | -0.74 % | 139.0 / 138.2 | -0.60 % |
| jit-trace/rec_loop | 444.4 / 442.0 | -0.56 % | 80.7 / 80.0 | -0.84 % |
| corpus2/heldout/stream_closures | 2856.2 / 2830.5 | -0.90 % | 767.5 / 747.6 | -2.60 % |
| corpus2/dev/particles_physics | 1375.3 / 1366.9 | -0.61 % | 401.0 / 397.9 | -0.77 % |
| awfy/permute | 195.4 / 195.2 | -0.08 % | 35.7 / 35.7 | -0.16 % |

Lua 5.4 (the same commits' 5.4 builds, instructions): raytrace -4.38 %, deltablue -1.72, cd -1.21, lru_cache -1.06, towers -0.94, richards -0.80,
havlak -0.59, classic_vector -0.56.

Against the compile share of the study (compile + libc at the default size: raytrace 36.7 %, cd 26.5, deltablue 24.9, richards 22.7, towers 15.5),
these fixes remove about 14 % of raytrace's compile cost and 4-7 % of the others'. The study's "halve the compile" estimate needs the recorder's
per-bytecode path and the back end, not allocation and scan fixes.

### 39.4 Decisions, gates, what ran and what did not, risks

| handle | knob (default) | decision | the data it rests on |
|---|---|---|---|
| T1 | `retfsel` (1) | shipped | lua_lexer -7.6 % instructions / -6.7 % cycles, dijkstra_routes -1.2 / -0.7, json -0.7; worst roguelike_turns +0.8 / +1.6 (formation); suite 0.9993; 5.4 inert |
| T3 | `heatside` 48, `heatwait` 16, `heatbudget` 64, `heatdepth` 8 | shipped | oop_mixins -28.8 / -21.9, vending_fsm -5.8 / -4.5, heapsort -4.6, regex_nfa -4.3, lua_lexer -3.1 / -3.5, richards 250 -7.4 / -7.3; richards default +0.3 / +4.7 (branch misses of 13 extra side traces); suite 0.9972 (5.4: 0.9964) |
| T2 | none (always on) | shipped | identical traces (81 / 83, the rest non-deterministic in either binary); raytrace -5.0 / -4.3, deltablue -1.8 / -1.5, cd -1.4 / -1.4, towers -1.0 / -1.9, suite 0.9983 |

**Gates** (`scripts/gate-light.sh --base HEAD`, default `GATE_CPUS`, one gate at a time, never during a timing run):

* T1 commit: all steps PASS (fmt, mutant map, clippy 5.5 / 5.5-jit / 5.4, core 5.5 / 5.4, trace suites 5.5 158 s / 5.4 148 s, `vm_corpus`,
  jit-diff 1-50, mutant group 76 ran / 76 caught).
* T3 commit: the first run failed `tr_core::side_traces_install_and_run` on 5.5 and 5.4 (its `maxside=1` bound now sees the `heatside` grants)
  and therefore the mutant `side_maxside_ignored` (its detector); the test pins `maxside=1,heatside=0` for the hard cap and bounds
  `maxside=1,heatside=2` by 3; the second run: all steps PASS, mutant group 81 ran / 81 caught.
* T2 commit: all steps PASS except `mutant-map` (the injection-site lines moved); the map was regenerated and `fmt` + `mutant-map` re-run PASS;
  mutant group 76 ran / 76 caught. The documentation was added to the T2 commit after its gate (text only).
* Mutant groups per handle: `gate-mutants.sh --only retfsel_` 4 / 4, `--only heatside_,heatwait_,heatdepth_,heatdefer_` 6 / 6,
  `--only side_maxside_ignored` 1 / 1. Registry 796 -> **806**.

**Not run** (by instruction): `scripts/gate-heavy.sh`, the oracle-feature suites (`tr_identity`, `instruction_count_identity`, `t15_oracle`,
`oracle_options`), jit-diff 1-200 in seven modes, the full mutant runner (806), any full benchmark. The 5.4 suite of programs was measured for
instructions only (no 5.4 cycles).

**Risks.** (1) T1 and T3 change the formation of the trace tree; formation-fragile programs (roguelike_turns, real_dis_mips, richards at some
hot thresholds, astar_jumper, which is non-deterministic) move by a few percent either way, and the per-threshold means say the shipped defaults
land on lottery outcomes, not systematic losses, but a session benchmark may show such programs moving. (2) T3's richards default-size cycle cost
(+4.7 %) is real: extra side traces that the program's length does not repay; the large size gains -7.3 %. A tighter wait was measured and is
worse for both richards and the winners. (3) T3 interacts with every rule that counts exit reports (the `loopany` guard is handled, Q2's
counters see the same restarts as with `polyside`); a rule added later that counts side attempts must know that a deferred exit reports again.
(4) T1 relies on the caller's head state at recording time; a caller trace flushed later leaves the RETF side trace linking through the head's
cell into tier 1 (correct, slower). (5) T2's `wmask` must stay in step with `written`: every write goes through `def` / `set_written` now; a
future direct write to `written` would make `entries` miss a slot (the unit test compares with the full scan, the oracle suites would see it).
(6) In-place `Unstable` restarts (TR24c, `restarts = 1`) cure roguelike's first-iteration lottery and gave raytrace -3.9 % in a scratch build;
recorded here as a lead, not built.

## 40. The regressions of session-l (subject 7413bff against the rebuilt 5a064d7): causes, `heatside` 48 -> 12 (2026-11-23, base 7258220)

Session-l (`bench/results/session-l/report.md`) found four programs above the bar. Method of this section: instructions of one run (deterministic,
`RUSTYNAIL_SEED=0`, release `rn-cli` builds of 5.5 and 5.4 via `scripts/release-build.sh --no-pgo`) against the same-session `5a064d7` columns of
`bench/results/baselines`; cycles min of 5-15 pinned to CPU 4 for the cycle-only items; sample = `scripts/bench-fast.sh --matrix 55plain,54plain`
over 40 programs (awfy 14, jit-trace 6, programs 3, corpus 2, corpus2 15), `PREV_COMMIT=5a064d7 FORCE_STALE=1`.

### 40.1 Where the regressions come from

Knob bisection on the 7413bff binaries (instructions M, 5.5; `RUSTYNAIL_JIT_TRACE_LIMITS`; 5a064d7: 2775.7):

| roguelike_turns | all on | `heatside=0` | `retfsel=0` | `deepprobe=0` | all three off |
|---|---:|---:|---:|---:|---:|
| 5.5 | 2823.6 (+1.7 %) | 2785.2 (+0.3 %) | 2799.0 | 2823.7 | 2764.2 |
| 5.4 (5a064d7: 3076.3) | 3102.6 (+0.9 %) | 3070.9 | 3102.3 | 3102.6 | 3070.5 |

* `deepprobe` has no effect on roguelike_turns; T1 (`retfsel`) accounts for -25 M of the 5.5 figure (+0.9 % when T3 is also on, +0.3 % alone);
  **T3 (`heatside`) accounts for the rest** on both builds. T2 builds identical code. `real_dis_mips` 5.4: 7499.8 M with `heatside` on (every
  value of `heatside` / `heatbudget` / `heatwait` / `heatdepth` gives 7499.8), 7338.1 M with `heatside=0` (5a064d7: 7337.6): again T3.
* The T3 mechanism behind both is not the grants. In `real_dis_mips` 5.4 **no side trace is ever granted** (`[trace] heatside: granted` absent):
  26 deferrals (`HeatWait`) happen, the families are flushed or the run ends before an exit reaches `heatwait` reports. What differs from
  `heatside=0` is that the deferred exit stays `Free` where the old rule blacklisted it `MaxSide` (a fault, `black_exit`): after a flush of a sibling
  subtree the family has room again, the exit is recorded (a side start at slot 3 exit 20, which then aborts `CFunc`), and the formation downstream
  differs (`fnpromo` 5 against 4 heads, `rearm: armed 228` against 58, J4 region entries 955 against 758): a different formation, not a cost of the
  rule itself (compile cost is lower with the rule on: `pipeline` 9.2 against 10.1 Mtsc, exits 2.66 M against 3.14 M, instructions +2.2 %).
  Marking the family faulted at the first deferral (to keep `rearmclean` as before; scratch build) did not change the result, and cost vending_fsm
  its T3 gain (3519 -> 3771 M): rejected.
* In `roguelike_turns` 5.5 the first 8 grants (`heatbudget=8`) cost +0.1 %, the 9th (an exit inside one inlined frame of the `Lexer:next`-like
  family at proto line 80, `heatbudget=9`: 2834.3 M) adds +1.6 %, the 10th takes part of it back (2826.5); exits go down (11.6 M -> 10.8 M), the
  extra instructions are interpreter / J4 work after a changed formation.
* **They are formation lotteries, as 39.2 said.** Hot threshold sweep (`RUSTYNAIL_JIT_TRACE_HOT` 248 / 252 / 256 / 260 / 264, instructions M):
  5.4 real_dis_mips `heatside=0` 7586 7334 7343 7491 7625, `heatside=12` 7586 7496 7505 7491 7683 (equal at 3 of 5 thresholds; the +2.2 % is two
  thresholds where off is lucky, 256 among them); 5.4 roguelike off 3085 3074 3071 3315 3218, on 3110 3075 3103 6325 3224; 5.5 roguelike off
  2831 2811 2786 3038 2945, on 2808 2846 2819 3056 2885; 5.5 real_dis_mips off 7298 7301 7609 7276 7409, on 7614 7302 7293 7276 7436. No sign
  of a systematic loss, the shipped threshold 256 lands where T3-off happens to be best. Candidate separators of "useful grant" from "wasted
  deferral" were measured and rejected: the rate of reports of the deferred exit (reports of any exit between its first deferral and its grant:
  oop_mixins 15..434, vending_fsm 31..267, lua_lexer 62..295, richards 68..175, roguelike 18..1102: no gap); the number of inlined frames of the
  exit (`heatframes=0` loses all of oop_mixins' gain: 5541 M; richards, vending_fsm, and real_dis_mips deferrals are all at `frames 1` like
  oop_mixins' paying grants); `heatwait` 4 / 8 / 12 / 24 / 32 / 64, `heatbudget` 16 / 24 / 32 / 48, `heatdepth` 2 / 4 / 6 (all move the lottery
  without a pattern: e.g. `heatwait=4` roguelike 5.5 +8.9 %, `heatwait=8` +3.1 %).
* **richards (cycles at equal instructions)** is the cost of grants that do not reduce its exits (39.2). Cycles, min of 15, same binary, CPU 4:
  `heatside=0` 59.5 / 59.9 M, `heatside=8` 59.9 / 59.8, 12: 60.4, 16: 61.7, 24: 62.3, 48: 62.2 / 62.2 (+4.1 %). The cycles follow the number of
  granted traces (13 at the old cap; the traces are granted in family sizes 16..30). Instructions: 175.8 (off), 172.5 (8), 173.1 (12), 176.1 (>= 16).
* **binary_trees**: `RUSTYNAIL_JIT_VERBOSE=2` trace dumps of 5a064d7 and 7413bff are identical apart from addresses and GC-timing-dependent flush counters
  (the same traces, side-cell snapshots, code size and instruction count 5632.1 M): no code difference, **layout noise**, nothing done.

### 40.2 The change: `heatside` 48 -> 12 (a knob-level trade-off)

A family may now exceed `maxside` (16) by at most 12 granted side traces (`HEATSIDE_PROCESS`, struct default, doc 39.2 text "default 48" is
superseded); the rule and the other three knobs are unchanged (`heatwait` 16, `heatbudget` 64, `heatdepth` 8), tests pin their own values.
Instructions M, 5.5 / 5.4 (grid, same binaries, `heatside` on the command line):

| program | off (0) | 12 | 14 | 20 | 48 (7413bff) |
|---|---:|---:|---:|---:|---:|
| roguelike_turns 5.5 | 2785.2 | 2799.0 | 2830.0 | 2820.5 | 2823.6 |
| richards 5.5 | 175.8 | 173.1 | 175.0 | 176.1 | 176.1 |
| oop_mixins 5.5 | 5541.7 | 3992.0 | 3946.2 | 3941.3 | 3941.0 |
| oop_mixins 5.4 | 5766.0 | 4217.1 | 4171.9 | 4166.6 | 4166.3 |
| vending_fsm 5.5 | 3709.2 | 3634.1 | 3610.7 | 3553.1 | 3519.3 |
| lua_lexer 5.5 | 4074.1 | 3971.1 | 3959.5 | 3954.9 | 3946.9 |
| regex_nfa 5.5 | 3606.1 | 3578.9 | 3578.9 | 3579.8 | 3451.4 |
| heapsort 5.5 | 2910.5 | 2776.7 | 2776.7 | 2776.7 | 2776.7 |

Cycles, same binary, min of 7 (CPU 4): oop_mixins 1450.2 (off) / 1165.7 (12) / 1139.6 and 1166.1 (48): the same -19.6 % as at 48.
richards 250: instructions 1421 M (off) / 1322 (12) / 1315 (48), cycles 462 / 392 / 400 M: the large-size gain is kept.

Sample against the same-session 5a064d7 columns (`bench-fast`, 40 programs, 5.5 plain / 5.4 plain; cycles ratio / instruction ratio; the
7413bff binary in the same session in brackets):

| program | 5.5 (7413bff) | 5.5 (this) | 5.4 (7413bff) | 5.4 (this) |
|---|---|---|---|---|
| awfy/richards | 1.055 / 0.995 | **1.010 / 0.979** | 1.059 / 0.996 | 1.007 / 0.979 |
| corpus2/heldout/roguelike_turns | 1.058 / 1.017 | 1.041 / **1.010** | 1.024 / 1.009 | 1.023 / 1.009 |
| corpus2/heldout/real_dis_mips | 1.010 / 1.001 | 1.016 / 1.001 | 1.046 / 1.022 | 1.055 / 1.022 |
| corpus2/dev/oop_mixins | 0.815 / 0.710 | 0.839 / 0.719 | 0.805 / 0.722 | 0.805 / 0.731 |
| corpus2/heldout/vending_fsm | 0.962 / 0.946 | 0.978 / 0.977 | 0.932 / 0.887 | 0.943 / 0.904 |
| corpus2/dev/regex_nfa | 0.886 / 0.956 | 0.917 / 0.992 | 0.958 / 1.000 | 0.977 / 1.000 |
| corpus/lua_lexer | 0.916 / 0.894 | 0.929 / 0.899 | 1.006 / 1.000 | 1.003 / 1.000 |
| programs/binary_trees | 1.039 / 1.000 | 1.032 / 1.000 | 1.031 / 1.000 | 1.030 / 1.000 |

The other 32 programs move by under 1 % in instructions between the two columns (the
geomean over the 40 is 0.9961 / 0.9805 for this against 0.9988 / 0.9789 cycles / instructions for 7413bff, 5.5; 5.4 0.9999 / 0.9834 against
1.0035 / 0.9829).

**Bars.** Met: richards (+1.0 % cycles, instructions -2.1 %), oop_mixins (instructions -28 % / -27 %, cycles -19.6 % against `heatside=0`, -16
to -20 % against the cached 5a064d7 column), roguelike_turns 5.5 (+1.0 % instructions: at the bar, was +1.7 %). **Not met, unchanged:**
real_dis_mips 5.4 (+2.2 % instructions, +4.5 % cycles) and roguelike_turns 5.4 (+0.9 %): lotteries of the deferral (above); nothing cheap
separates them from the paying grants. **The price:** T3's gain on vending_fsm shrinks from -5.1 % to -2.0 % instructions (5.5; 5.4 -11.0 % to
-9.6 %), on regex_nfa from -4.3 % to -0.75 % (5.5), on lua_lexer from -3.1 % to -2.5 %; oop_mixins, heapsort and richards 250 keep theirs.

## 41. N1, the trace loop-body diet (docs/STUDY_LUAJIT_GAP_2.md section 4; 2026-11-23, base 432a77d)

The study counted one array-loop iteration of `jit-trace/type_entry_stable` (`for j = 1, m do a = a + t[j] * 0.5 end`): 21 instructions per iteration for
us against 9 for LuaJIT, 18 against 8 in the loop body. Ten extra: poll 2 and budget 2 (the price of exact hooks / instruction counts: kept), index
arithmetic 4 (`mov; add -1`, `mov; not`), loop control 2 (a second iteration count). N1 removes the six that are plain code quality. Method of this
section: instructions of one run (deterministic, `RUSTYNAIL_SEED=0`), cycles min of 5 interleaved pinned runs (CPU 4, `perf stat -e instructions:u,cycles:u`)
of release `rn-cli` builds (5.5 and `--cfg rustynail_lua54`; plain `cargo build --release`, no PGO); the control is a build of main 432a77d in the same session;
the per-item columns are the final binary with `RUSTYNAIL_JIT_TRACE_LIMITS` switching the later items off (`a` = `cloop=0,nidx=0,leaadd=0`, `ac` =
`nidx=0,leaadd=0`, `acb` = `leaadd=0`, `all` = the shipped defaults, `off` = every N1 knob 0: the same binary as `all`, so that code-layout noise of two
binaries shows). 36 programs (jit-trace 6, awfy 13, programs 14, lib 2, apps 1, real 1) were chosen as the array-loop programs named by the study plus a spread
of the rest.

### 41.1 What was built (each item one commit, one knob, `RUSTYNAIL_JIT_TRACE_LIMITS=name=0|1`, docs/JIT.md table)

| item | knob | default | what |
|---|---|---|---|
| (a) constant index offset | `ixfuse` | **1** | an integer `ADD x, #c` whose users are all the index operand of fused `AREF`s is no instruction and no register: `ALOAD` / `ASTORE` address `[base + x + tag base + c]` (tag byte) and `[base + 8 * ~x - 8 * c]` (payload), `x` stays live (`ra.rs fuse_index_adds`, `operand_refs`, `asm.rs ix_c / ix_tag_off / payload_scaled_c`). `ADD`s that anything else names (a bounds guard, a snapshot, a `PHI`) stay as they were. |
| (c) counted-loop back-edge | `cloop` | **1** | the loop-ending guard `NE count, #0`, the count's `ADD -1` and the in-place index adds become `add idx, step; sub count, 1; jae head` (new micro-op `SubJae`: `x86.rs`, `emu.rs`); the fall-through of the loop's last exit restores `count` and the indices and leaves through the guard's own exit stub. One branch instead of two, 3 instructions instead of 5 (`asm.rs cloop_plan / cloop_tail`). |
| (b) carried negated index | `nidx` | **1**, with a policy | in a peeled counted loop the complemented index `~(k + c)` of an `ALOAD` whose index is made of the loop's induction value `e` (`ADD x, #s` of the entry `SLOAD`, carried through a slot) is the carried value `ne = -e` (a PHI: `NEG e` at the end of the pre-roll, `ADD ne, #-s` at the end of the body) plus a displacement: the `aidx` mechanism of BUNDLE-L A (`Aref` third operand) planned from the induction slot (`opt_loop.rs plan_nidx / nidx_aux / emit_nidx_updates`). Policy: only in loops without a helper call (`ra::has_call`), for `ALOAD`s that read a payload, and where the general registers have room (`peel_loop`: PHIs + loop-invariant values of the body in general registers, the new PHI included, plus 2 must not exceed `ra::free_gprs`; otherwise the loop is peeled again without `nidx`, as when its induction value is not updated by `+ step`). |
| (extra) `lea` for `ADD` | `leaadd` | **1** | an integer `ADD a, #imm` into another register is `lea d, [a + imm]` (one instruction for `mov d, a; add d, imm`; `asm.rs ibin`). |

(`aidx` itself keeps its meaning and its default 0.) `cloop`'s register-step form (a loop with a run-time step, `for j = i * i, n, i`) is included.

### 41.2 Why (c) is not "exit on index against the limit"

The study proposed to drop the iteration-count PHI and test `idx == last`. Every mid-body exit of the loop needs the count in its snapshot (the interpreter's
`FORLOOP` reads it), so a count that is not carried needs a *derived* snapshot entry (`count = (last - idx) / step` computed by the exit stub). That
touches the IR (a snapshot-only value), the allocator (two locations per entry, liveness through every guard), `build_exits` / `ValSrc`, the inline and generic
stub generators, the Rust restore, `regcarry` (`CarrySrc`) and the emulator: seven consumers. What was built instead has the same instruction and branch count
for the control (`add idx; sub count; jae` = LuaJIT's `add; cmp; jle`), needs no snapshot change, and does not free the count register (which the derived
form would). The exit state is exact: the fall-through runs only when `count == 0` before the subtraction (the borrow), re-adds 1 and re-subtracts the index
steps, then jumps to the stub of the guard's own snapshot (state in front of the updates, as before). The exit-storm / verify modes' forced exits come
earlier (at the guard, before the updates). The index fix-up is not observable by any program (the loop slots are dead when the loop ends; no mutant can be
written for it; `cloop_no_fixup_cnt` / `cloop_cnt_imm2` cover the count), it is kept so that the snapshot is exact.

What is not changed: `POLL`, `BUDGET`, every guard, every snapshot, GC accounting, the hooks; a wrapped or overflowing index or limit is decided by the same
guards as before (`forprep` guards, the `count >= 0` guard, the bounds guards `ULT x + c, asize` in 64-bit unsigned arithmetic: `x + c` of the displacement
form wraps exactly as the `ADD` did).

### 41.3 The loop of `type_entry_stable`

```
before (main 432a77d, 18 instr)          after (all four, 13 instr)
cmp byte [r15+POLL],0 ; jne              cmp byte [r15+POLL],0 ; jne
sub r11,4 ; jl                           sub r11,4 ; jl
mov r8,rdi ; add r8,-1                   cmp byte [rdx+rdi+3],0x13 ; jne
cmp byte [rdx+r8+4],0x13 ; jne           movsd xmm1,[rdx+r8*8]            (r8 = -j, carried)
mov rax,r8 ; not rax                     mulsd xmm1,[r15+0x938]
movsd xmm1,[rdx+rax*8]                   addsd xmm0,xmm1
mulsd ; addsd                            add rdi,1 ; add r8,-1 ; sub rsi,1 ; jae top
cmp rsi,0 ; je ; add rsi,-1 ; add rdi,1 ; jmp top
```
Instructions per iteration of the program 21.2 -> 16.4 (`type_entry_stable` 10588 -> 8198 M, -22.6 %).

### 41.4 Tests, mutants, gates

* `tests/tr_core/n1_progs.rs` (7 programs: offsets of both signs with int / float elements, holes, type changes and reads past the end at the first, middle and last iteration;
  positive / negative / large / run-time steps and loops that run once or not at all; the integer limits (`maxinteger - 3 .. maxinteger`, `mininteger ..`, steps of
  `maxinteger`, float and string limits); exits in the middle of an iteration and after the last one (an index or count that is off by one runs the loop one
  iteration more or less after the exit); a matrix product; stores through offsets with overlapping reads and writes; the program's own `-i`, `~i`, `0 - i` next to the
  carried negated index) in `tr_core::n1_programs`: 6 knob combinations (each alone, all, none, with `aidx=1`) x `always` / `on2` x 7 option sets (`+verify`,
  `+exitstorm=3`, `+exitstorm=1 +verify`, `+randra=5 +verify`, `+emurun +verify`, `-loop`) + GC stress 2 + budget 17; `n1_programs_enter_traces`; the `tr_asm` environment of
  the emulator tests runs with `cloop` and `leaadd` on. 5.5 and 5.4.
* Mutants (9 new, registry 806 -> 815, all caught by `n1_programs`, `TR_MUT_ONLY=ixfuse_,cloop_,nidx_,leaadd_`): `ixfuse_tag_disp0` (the tag displacement ignores `c`),
  `ixfuse_tag_disp_neg` (sign), `ixfuse_pay_disp_pos` (payload displacement `+8c`), `cloop_cnt_imm2` (the count steps by 2: an off-by-one limit),
  `cloop_no_fixup_cnt` (the exit leaves the count one short), `nidx_disp_off1`, `nidx_step_sign`, `nidx_entry_off1` (the entry value `~e` instead of `-e`),
  `leaadd_disp_off1`.
* A bug the first gate run found (`bq_mod`, `b5_sink`, `bn_random`, ... : 15 failing tests): the planned `NEG e` of the pre-roll was in the CSE chain, so the
  program's own `-i` of the body merged with it and became loop invariant. The planned values are appended outside the chain (`TraceIr::emit`) and
  `n1_negidx` pins it.

### 41.5 Measurements

Columns are cumulative against the control build of main 432a77d (`base`): `off` = the final binary with every N1 knob 0 (a measure of the noise of two builds), `a` =
(a), `ac` = (a) + (c), `acb` = (a) + (c) + (b), `all` = + `leaadd` = the shipped defaults. Each cell is the change of instructions (change of cycles, min of 5
interleaved runs); **!** marks a cell above the bar (+1 % instructions or +2 % cycles).

**Per item, `jit-trace/type_entry_stable` (the study's example), 5.5, instructions M (cycles M):** control 10588 (1509); `a` 9608 (1549); `ac` 8688 (1366);
`acb` 8228 (1363); `all` 8198 (1364). Per iteration 21.2 -> 19.2 -> 17.4 -> 16.5 -> 16.4 instructions; cycles -9.6 %. 5.4: 11682 (1543); 11682 (1540); 10762 (1509); 10302
(1507); 9792 (1501): 5.4 does not hoist the bounds check of a single access (`bhoist` needs two covered checks there), so the `ADD` stays and (a) does not fire
on `type_entry`; (c), (b) and `leaadd` do.

* **(a) `ixfuse`**: instructions -8.8 % on `type_entry` / -9.3 % `_stable` / -8.6 % `dot` / -11.5 % `array_rec` / -6.0 % `matmul` / -3.7 % `awfy/sieve` / -4.8 % `lib/byte`
  (5.5; 5.4: `dot` -8.5 %, `array_rec` -11.4 %, `matmul` -5.3 %, `lib/byte` -4.8 %, the loops with a `ULT` of the same index keep their `ADD`). **Cycles do not follow on
  the two `type_entry` programs: +3.7 % and +2.7 % (5.5, (a) alone)**: the loop is limited by its five branches (poll, budget, tag guard, count guard, back-edge: about 3 cycles per
  iteration at an IPC of 7), not by instructions, and the shorter loop lands in a worse code-placement zone (Q2, `HEAD_OFFSET`); `programs/sieve` +2.9 % cycles with
  equal instructions is the same effect (a alone), `programs/heapsort` +1.7 % instructions (a live index kept longer: a spill that `leaadd` takes back, `all` -0.1 %).
  So (a) alone does not meet the cycle bar on three programs; **together with (c) it does, which is the shipped state**, and the knob stays a
  separate bisect point (`ixfuse=0`).
* **(c) `cloop`**: the control of the loop goes from 5 instructions and 2 branches to 3 and 1. `type_entry` -8.2 % further instructions (-17.0 % total), cycles **-8.1 %**
  against the control (the one item that moves them: 4 branches instead of 5), `type_entry_stable` -9.5 % cycles, `awfy/sieve` -7.5 % / -4.1 %, `programs/loops_arith` -5.7 % / -4.5 %,
  `programs/sieve` -4.7 % / -2.8 %, `programs/matmul` -11.9 % / -3.3 %, `array_rec` -23.0 % / -0.1 %; 5.4 the same instructions (type_entry -7.4 %).
* **(b) `nidx`**: `type_entry` -4.1 % more instructions (-21.1 % total), `dot` -12.9 % more (-30.0 % total: two accesses per iteration), cycles equal (the loop is
  branch-bound). The policy keeps it out of loops that do not pay: **`programs/matmul`'s inner loop (6 general values held of 7 free registers: -3 % instructions and -2..-3 %
  cycles in a run without the rule, the price of the rule) and `spectral_norm` (6 held: +4.7 % instructions without the rule) are left out**, `lib/byte` (a `string.byte` helper call:
  +5.3 % instructions, +10 % cycles without the rule) and 5.4 `awfy/bounce` (+2.7 %) likewise. The policy was found by these regressions in the first full runs: helper call
  (`ra::has_call`), then a carried-value count that counted written temporaries (wrong), then the free registers of the allocator pool (`ra::free_gprs`: 7 after the budget
  register, minus what the loop clobbers).
* **(extra) `leaadd`**: `awfy/sieve` -13.5 % total (-6 % more), `programs/spectral_norm` -11.0 % (-4 %), `programs/sieve` -10.2 % (-5.5 %), `programs/heapsort` +1.6 % -> -0.1 %,
  `programs/fannkuch` -3.4 %, `programs/queens` -3.0 %; 5.4: `spectral_norm` -5.2 %, `awfy/sieve` -11.4 %, `programs/sieve` -9.6 %.

**Geomean over the 36 programs (instructions, cycles) against the control:** 5.5: `off` -0.01 % (+0.08 %), `a` -1.71 % (-0.14 %), `ac` -3.58 % (-1.27 %), `acb` -4.30 % (-1.44 %),
**`all` -5.01 % (-1.50 %)**. 5.4: `off` +0.03 % (-0.13 %), `a` -0.90 % (-0.48 %), `ac` -2.56 % (-0.99 %), `acb` -3.22 % (-1.02 %), **`all` -4.22 % (-1.08 %)**.

**Programs over the bar** (5.5): in `all`: none (largest cycles: `programs/queens` +1.8 % with -3.0 % instructions, `programs/heapsort` +1.2 %; a 15-run
rerun of `programs/queens` gave +0.5 % and of `heapsort` +1.3 %, i.e. noise of the min of 5). In the partial configurations: `type_entry` / `_stable` / `programs/sieve` in `a`
(above), `programs/heapsort` +1.6 / 1.7 % instructions in `a` / `ac` / `acb` (gone in `all`), and `real/lua_interp`, whose `off` column (+2.7 % cycles, instructions -0.6 %) is
the same binary with the knobs off: a layout difference of the two builds (the interpreter), not of N1. 5.4: none, in any column.

Not moved (the study expected -5..-8 % for them): `awfy/permute` (-0.8 % instructions: its indices are `i` itself, `self.v[i + 1]` is `ADD #1` then `ADD #-1`, folded to `i`), `awfy/queens`
(-0.7 %), `awfy/towers` (0.0 %), `programs/queens` (-3.0 %: `ixfuse` -1.0, `cloop` -0.7, `leaadd` -1.3), `nbody`, `mandelbrot`, `richards`, `deltablue`, `havlak`, `json`: the loops of these programs
are not counted array loops, or their traces end in calls and returns; the diet cannot reach them.

### 41.6 What was not built, and what is open

* The derived-count form of (c) (above); its only extra gain would be the freed register.
* A negated index for `ASTORE` (the operand array of an `AREF` user is `[base, index, value]`; a fourth operand is a change of every consumer of `operand_refs`). Stores through
  `t[i + c]` keep `mov; not` for the payload; stores of nil / false / true write the tag byte only and are already minimal (`awfy/sieve`'s fill loop is 8 instructions).
* An `ADD x, #c` that a bounds guard also reads (`ULT x + c, asize` in loops whose bound check is not hoisted: 5.4 single access, run-time steps, a second table per
  iteration as in `matmul`'s `b[k][j]`) keeps its register: `lea` (`leaadd`) is the one instruction instead of two there.
* The three item-(a) cycle regressions in the table are a placement lottery of `type_entry`'s loop, not chased (the instruction counts and the final column are the signal).
* `nidx` policy numbers (5 held, 7 free registers) come from six programs; a register-pressure estimate from the allocator itself (compile twice) would replace them.

### 41.7 Full tables (5.5, then 5.4; the 36 programs)

| program | base ins M | off ins (cyc) | a ins (cyc) | ac ins (cyc) | acb ins (cyc) | all ins (cyc) |
|---|---:|---:|---:|---:|---:|---:|
| jit-trace/type_entry | 10958 / 1548.6 cyc | +0.0% (+0.7%) | -8.8% (+3.7%) **!** | -17.0% (-8.1%) | -21.1% (-8.1%) | -21.4% (-8.2%) |
| jit-trace/type_entry_stable | 10588 / 1508.8 cyc | +0.0% (-0.1%) | -9.3% (+2.7%) **!** | -17.9% (-9.5%) | -22.3% (-9.6%) | -22.6% (-9.6%) |
| jit-trace/dot | 9307 / 1220.2 cyc | +0.0% (-0.0%) | -8.6% (-0.3%) | -17.1% (-0.4%) | -30.0% (-0.1%) | -30.0% (-0.2%) |
| jit-trace/float_loop | 9305 / 2773.9 cyc | +0.0% (+0.1%) | +0.0% (+0.1%) | +0.0% (+0.2%) | +0.0% (+0.1%) | +0.0% (+0.1%) |
| jit-trace/array_rec | 5192 / 1795.4 cyc | +0.0% (+0.0%) | -11.5% (-0.1%) | -23.0% (-0.1%) | -23.0% (-0.1%) | -23.0% (-0.1%) |
| jit-trace/short_inner | 4944 / 1099.9 cyc | +0.0% (-0.1%) | +0.0% (-0.2%) | -0.0% (-0.1%) | -0.0% (-0.1%) | -0.0% (-0.2%) |
| awfy/permute | 195 / 36.1 cyc | +0.0% (-0.3%) | +0.0% (-0.6%) | +0.0% (-1.1%) | +0.0% (-1.0%) | -0.8% (-0.6%) |
| awfy/queens | 511 / 95.8 cyc | +0.0% (+0.2%) | +0.0% (+0.7%) | +0.0% (+0.5%) | +0.0% (-0.8%) | -0.7% (-0.1%) |
| awfy/towers | 173 / 46.8 cyc | +0.0% (+0.1%) | +0.1% (-0.6%) | +0.1% (-0.9%) | +0.1% (-0.4%) | +0.0% (-1.5%) |
| awfy/sieve | 903 / 124.5 cyc | +0.0% (+0.2%) | -3.7% (-0.6%) | -7.5% (-4.1%) | -7.5% (-5.3%) | -13.5% (-8.0%) |
| awfy/nbody | 553 / 92.6 cyc | +0.0% (-0.3%) | -0.2% (-0.2%) | -0.2% (-0.3%) | -0.1% (-0.1%) | -0.7% (-0.3%) |
| awfy/bounce | 307 / 61.0 cyc | +0.0% (-0.8%) | -0.2% (-1.2%) | -0.2% (-1.3%) | -0.1% (-1.0%) | -0.2% (-1.2%) |
| awfy/list | 408 / 96.4 cyc | +0.1% (+0.0%) | +0.1% (-0.2%) | +0.1% (-0.7%) | +0.1% (-0.9%) | +0.1% (-0.6%) |
| awfy/mandelbrot | 520 / 272.5 cyc | +0.0% (+0.1%) | +0.0% (+0.1%) | +0.0% (-0.0%) | +0.0% (+0.0%) | +0.0% (+0.0%) |
| awfy/storage | 487 / 102.1 cyc | +0.0% (+1.6%) | +0.0% (-0.0%) | +0.0% (-0.3%) | +0.0% (-0.1%) | -0.0% (-0.2%) |
| awfy/richards | 173 / 60.0 cyc | +0.0% (+0.1%) | +0.1% (-0.9%) | +0.1% (-1.1%) | +0.2% (-1.5%) | +0.1% (-1.6%) |
| awfy/deltablue | 318 / 106.6 cyc | +0.0% (-0.1%) | -0.1% (-0.9%) | -0.1% (+0.0%) | +0.1% (-0.3%) | +0.1% (-0.5%) |
| awfy/havlak | 508 / 149.1 cyc | +0.0% (-0.9%) | -0.0% (-0.1%) | -0.0% (-0.4%) | +0.0% (+0.3%) | -0.1% (-0.0%) |
| awfy/json | 448 / 137.5 cyc | +0.0% (+0.7%) | +0.1% (+0.1%) | +0.1% (+0.4%) | +0.2% (+0.3%) | +0.2% (+0.8%) |
| programs/matmul | 2131 / 439.8 cyc | +0.0% (-0.6%) | -6.0% (-4.2%) | -11.9% (-3.3%) | -11.9% (-7.0%) | -11.9% (-5.7%) |
| programs/queens | 5391 / 1855.1 cyc | +0.0% (+0.1%) | -1.0% (+0.3%) | -1.7% (+1.5%) | -1.7% (+0.5%) | -3.0% (+1.8%) |
| programs/sieve | 2854 / 577.2 cyc | +0.0% (+1.0%) | +0.0% (+2.9%) **!** | -4.7% (-2.8%) | -4.7% (-2.5%) | -10.2% (-2.9%) |
| programs/spectral_norm | 1454 / 180.3 cyc | +0.0% (-0.2%) | -4.2% (+1.0%) | -7.0% (-0.2%) | -7.0% (-0.3%) | -11.0% (+0.3%) |
| programs/fannkuch | 4860 / 1035.6 cyc | +0.0% (+0.3%) | -1.0% (+0.6%) | -1.7% (-1.3%) | -2.0% (-1.1%) | -3.4% (-1.5%) |
| programs/nbody | 1985 / 341.7 cyc | +0.0% (+0.1%) | -0.3% (+0.3%) | -0.3% (+0.2%) | -0.5% (+0.1%) | -1.0% (+0.2%) |
| programs/heapsort | 2777 / 1037.7 cyc | +0.0% (-0.1%) | +1.7% (+1.0%) **!** | +1.6% (+0.8%) **!** | +1.6% (+0.2%) **!** | -0.1% (+1.2%) |
| programs/table_array | 1427 / 367.5 cyc | +0.0% (-1.0%) | -0.0% (-1.0%) | -0.1% (-1.3%) | -0.1% (-1.3%) | -0.2% (-1.1%) |
| programs/loops_arith | 3154 / 476.7 cyc | +0.0% (-0.1%) | +0.0% (-0.1%) | -5.7% (-4.5%) | -5.7% (-4.7%) | -5.7% (-4.5%) |
| programs/mandelbrot | 1644 / 532.6 cyc | +0.0% (+0.1%) | +0.0% (-0.0%) | +0.0% (-0.0%) | +0.0% (+0.0%) | +0.0% (-0.0%) |
| programs/string_ops | 2879 / 1124.6 cyc | +0.0% (-0.4%) | +0.0% (-2.5%) | +0.0% (-1.3%) | +0.0% (-0.3%) | +0.0% (-1.7%) |
| programs/fasta | 4527 / 1294.6 cyc | +0.0% (-1.0%) | +0.0% (-1.7%) | +0.0% (-1.3%) | +0.0% (-1.9%) | -0.2% (-1.5%) |
| programs/binary_trees | 5527 / 1177.9 cyc | -0.0% (+1.0%) | -0.0% (+0.7%) | -0.0% (+1.7%) | -0.0% (+0.0%) | -0.1% (+1.6%) |
| lib/byte | 393 / 50.2 cyc | +0.0% (+0.3%) | -4.8% (-4.8%) | -4.8% (-5.7%) | -4.8% (-5.9%) | -4.8% (-5.5%) |
| lib/byte_multi | 303 / 40.7 cyc | +0.0% (-0.2%) | -1.9% (-1.0%) | -1.9% (-1.7%) | -1.9% (-1.5%) | -2.8% (-2.7%) |
| apps/raytrace | 351 / 110.7 cyc | +0.0% (-0.3%) | +0.2% (+0.2%) | +0.2% (-0.3%) | +0.7% (+0.6%) | +0.7% (+0.5%) |
| real/lua_interp | 561 / 105.6 cyc | -0.6% (+2.7%) **!** | -0.6% (+2.5%) **!** | -0.6% (+1.9%) | -0.6% (+2.9%) **!** | -0.7% (+0.8%) |
| geomean (36 programs) | - | -0.01% (+0.08%) | -1.71% (-0.14%) | -3.58% (-1.27%) | -4.30% (-1.44%) | -5.01% (-1.50%) |

| program | base ins M | off ins (cyc) | a ins (cyc) | ac ins (cyc) | acb ins (cyc) | all ins (cyc) |
|---|---:|---:|---:|---:|---:|---:|
| jit-trace/type_entry | 12102 / 1584.0 cyc | +0.0% (+0.2%) | +0.0% (+0.7%) | -7.4% (+0.2%) | -11.2% (-1.3%) | -15.3% (-1.7%) |
| jit-trace/type_entry_stable | 11682 / 1543.3 cyc | +0.0% (-0.6%) | +0.0% (-0.2%) | -7.9% (-2.3%) | -11.8% (-2.4%) | -16.2% (-2.7%) |
| jit-trace/dot | 9340 / 1226.6 cyc | +0.0% (-0.2%) | -8.5% (-0.6%) | -17.1% (-0.5%) | -29.9% (-0.1%) | -29.9% (-0.2%) |
| jit-trace/float_loop | 9433 / 2793.8 cyc | +0.0% (-0.0%) | +0.0% (+0.1%) | +0.0% (-0.0%) | +0.0% (-0.0%) | +0.0% (+0.0%) |
| jit-trace/array_rec | 5261 / 1820.7 cyc | +0.0% (-0.0%) | -11.4% (-0.1%) | -22.7% (-0.0%) | -22.7% (-0.0%) | -22.7% (-0.1%) |
| jit-trace/short_inner | 4352 / 994.5 cyc | +0.0% (-0.1%) | +0.0% (-0.1%) | -0.0% (+0.0%) | -0.0% (-0.1%) | -0.0% (-0.1%) |
| awfy/permute | 202 / 37.1 cyc | +0.0% (-0.1%) | +0.0% (-0.2%) | +0.0% (-0.1%) | +0.0% (-0.3%) | -0.7% (-0.1%) |
| awfy/queens | 433 / 82.6 cyc | +0.0% (-0.1%) | +0.0% (-0.5%) | +0.0% (-0.9%) | +0.0% (-1.0%) | -0.8% (-0.7%) |
| awfy/towers | 175 / 47.6 cyc | -0.0% (-0.2%) | +0.1% (-1.4%) | +0.1% (-0.9%) | +0.1% (+0.1%) | +0.0% (-1.1%) |
| awfy/sieve | 953 / 128.9 cyc | +0.0% (-0.7%) | +0.0% (-1.1%) | -3.6% (-5.8%) | -3.6% (-3.5%) | -11.4% (-9.0%) |
| awfy/nbody | 566 / 97.4 cyc | -0.0% (-2.3%) | +0.0% (-2.8%) | +0.0% (-2.6%) | +0.1% (-2.5%) | -0.6% (-1.4%) |
| awfy/bounce | 320 / 64.4 cyc | +0.0% (-0.0%) | +0.0% (-0.5%) | +0.0% (-0.7%) | +0.1% (-0.0%) | -0.3% (-0.7%) |
| awfy/list | 410 / 96.8 cyc | +0.0% (+0.3%) | +0.0% (-0.4%) | +0.0% (-0.2%) | +0.0% (-0.5%) | +0.0% (-0.7%) |
| awfy/mandelbrot | 523 / 272.6 cyc | +0.0% (-0.1%) | +0.0% (-0.1%) | +0.0% (-0.0%) | +0.0% (-0.2%) | +0.0% (-0.1%) |
| awfy/storage | 675 / 147.5 cyc | -0.3% (-1.1%) | -0.3% (-0.9%) | -0.3% (-1.0%) | -0.3% (-1.3%) | -0.3% (-1.6%) |
| awfy/richards | 174 / 59.6 cyc | +0.0% (+0.1%) | +0.2% (-0.3%) | +0.1% (-0.2%) | +0.2% (-0.2%) | +0.1% (+0.9%) |
| awfy/deltablue | 343 / 114.4 cyc | +0.0% (+0.8%) | +0.1% (+0.3%) | +0.1% (+0.7%) | +0.2% (+0.8%) | +0.2% (+1.1%) |
| awfy/havlak | 573 / 172.2 cyc | +0.0% (-0.7%) | +0.0% (-0.5%) | +0.0% (-0.8%) | +0.1% (-1.1%) | +0.0% (-1.0%) |
| awfy/json | 490 / 151.0 cyc | +0.7% (-0.5%) | +0.7% (-0.5%) | +0.7% (-0.2%) | +0.8% (-0.3%) | +0.8% (-0.2%) |
| programs/matmul | 2384 / 441.0 cyc | +0.0% (+1.4%) | -5.3% (-0.9%) | -10.6% (-0.5%) | -10.6% (-1.2%) | -10.7% (-0.3%) |
| programs/queens | 5878 / 1918.3 cyc | +0.0% (+0.3%) | +0.0% (+0.5%) | -0.6% (+0.2%) | -0.6% (-0.3%) | -2.4% (+0.3%) |
| programs/sieve | 3006 / 613.7 cyc | +0.0% (-0.2%) | +0.0% (+0.5%) | -4.5% (-5.0%) | -4.5% (-4.6%) | -9.6% (-5.1%) |
| programs/spectral_norm | 1539 / 185.3 cyc | +0.0% (+0.3%) | +0.0% (-0.2%) | +0.0% (-0.0%) | +0.0% (+0.1%) | -5.2% (+1.3%) |
| programs/fannkuch | 5134 / 1085.2 cyc | +0.0% (+0.1%) | -0.9% (+0.3%) | -1.6% (-2.9%) | -1.9% (-3.0%) | -2.7% (-2.8%) |
| programs/nbody | 2037 / 350.7 cyc | +0.0% (+0.0%) | +0.0% (-0.0%) | +0.0% (-0.1%) | +0.0% (+0.0%) | -0.8% (-0.1%) |
| programs/heapsort | 9586 / 2201.7 cyc | +0.0% (+0.1%) | -0.0% (-0.5%) | -0.0% (+0.4%) | -0.1% (-0.0%) | -0.1% (-0.4%) |
| programs/table_array | 1644 / 404.2 cyc | +0.0% (-1.1%) | +0.0% (-0.9%) | -0.0% (-1.0%) | -0.0% (-1.2%) | -0.1% (-0.8%) |
| programs/loops_arith | 3154 / 480.1 cyc | +0.0% (+0.1%) | +0.0% (-0.0%) | -5.7% (-4.4%) | -5.7% (-4.4%) | -5.7% (-4.4%) |
| programs/mandelbrot | 1648 / 533.3 cyc | +0.0% (-0.0%) | +0.0% (-0.0%) | +0.0% (+0.0%) | +0.0% (+0.0%) | +0.0% (-0.0%) |
| programs/string_ops | 2866 / 1070.5 cyc | +0.0% (+0.8%) | +0.0% (+1.0%) | +0.0% (+0.4%) | +0.0% (-1.0%) | +0.0% (+1.6%) |
| programs/fasta | 5034 / 1365.6 cyc | -0.0% (+0.1%) | -0.0% (-0.4%) | -0.0% (-0.3%) | -0.0% (-0.2%) | -0.2% (+0.1%) |
| programs/binary_trees | 6476 / 1483.5 cyc | +0.3% (-0.9%) | +0.3% (-0.9%) | +0.3% (-0.1%) | +0.3% (-0.5%) | +0.2% (-0.7%) |
| lib/byte | 394 / 49.9 cyc | +0.0% (+0.2%) | -4.8% (-5.3%) | -4.8% (-4.8%) | -4.8% (-4.5%) | -4.8% (-5.5%) |
| lib/byte_multi | 303 / 40.7 cyc | +0.0% (+0.1%) | -1.9% (-1.2%) | -1.9% (-1.5%) | -1.9% (-1.3%) | -2.8% (-2.3%) |
| apps/raytrace | 361 / 119.9 cyc | +0.0% (-0.4%) | +0.2% (-0.3%) | +0.2% (+0.3%) | +0.7% (+0.3%) | +0.7% (+0.5%) |
| real/lua_interp | 577 / 111.0 cyc | +0.0% (-0.2%) | +0.0% (+0.1%) | +0.0% (-0.8%) | +0.0% (-0.7%) | -0.0% (+0.0%) |
| geomean (36 programs) | - | +0.03% (-0.13%) | -0.90% (-0.48%) | -2.56% (-0.99%) | -3.22% (-1.02%) | -4.22% (-1.08%) |

### 41.8 Gates and the bench-fast sample

* `scripts/gate-light.sh` per commit (fmt, mutant map, clippy x3, core 5.5 / 5.4, rn-jit trace suites 5.5 / 5.4, vm_corpus, rn-cli, jit-diff 1-50, the mutant groups of the commit): OK
  at the final state (742 s). The first run of the nidx commit failed 15 tests (the CSE bug above), fixed before the commit.
* `JITDIFF_MODES="off tr trdef" scripts/jit-diff.sh 1 40` on the final 5.5 and 5.4 release binaries: 40 seeds done, 0 diffs, 0 timeouts, both.
* `scripts/bench-fast.sh --matrix 55plain,54plain` over 30 programs (`PREV_COMMIT=5a064d7 FORCE_STALE=1`, cached columns of 5a064d7), once for the final build and once for a build of main 432a77d
  (same session): against the cached column the final build has instructions 0.9345 (5.5) / 0.9466 (5.4), cycles 0.9988 / 1.0049; the control 0.9936 / 0.9969 and 1.0140 / 1.0130 (the control
  is itself +1.4 % in cycles over the cached 5a064d7 column: heatside 12 and machine drift). Final / control: instructions -6.0 % (5.5), -5.0 % (5.4); cycles -1.5 % (5.5), -0.8 % (5.4).

## 42. N2, the coroutine switch (docs/STUDY_LUAJIT_GAP_2.md sections 2.2 and 3; 2026-11-23, base 505bcfa)

The study measured one `resume` + `yield` pair of `lib/co_yield` at 753 instructions / 167 cycles (LuaJIT 400 / 69) and estimated N2 ("`yield` recognised in the
interpreter CALL path, no C frame, no unwinding; resume re-enters the Lua frame directly") at -30 % cycles per pair. Method of this section: instructions of one run
(deterministic, `RUSTYNAIL_SEED=0`), cycles min of 9-15 pinned runs (CPU 4 / 8, `perf stat -e instructions:u,cycles:u`), release `rn-cli` builds of 5.5 and
`--cfg rustynail_lua54` (`scripts/release-build.sh --no-pgo`); the control is a build of main 505bcfa in the same session; the per-step columns are the final
tree built with the cargo features below switching steps off (`off` = `coro_noyield,coro_noresume`: the instruction counts of the control to the digit on every
program that does not call a coroutine function).

### 42.1 What was built (three commits; the knobs are cargo features of rn-core, forwarded by rn-cli, like `coro_nofuse`; there is no run-time knob)

| step | feature that switches it off | default | what |
|---|---|---|---|
| 1 `coyield` | `coro_noyield` | **on** | `native_fast_with` (the one native-call sequence of `OP_CALL`, tier-1 `jit_call` / `jit_ccall` and `pre_call_native_fast`) compares the callee with the registered `coroutine.yield` (`coro::register_yield_fn`, called by `lib_coroutine::open`; a static load + compare) and, for a yieldable thread, does what `co_yield` + `yield_k(nargs, 0, None)` do (`State::yield_direct`: `status = Yield`, `ci.u2 = nargs`, `Err(Status::Yield)`) without the indirect call. The C frame is pushed as before (it is what `debug.traceback(co)`, `debug.getinfo(co, 0)` and `resume_fused` read), hooks never get here (`native_fast_with` refuses them), a thread that is not yieldable (main thread, `table.sort` comparator, `gsub` callback, `__tostring`, ...) runs the library function, which raises the reference's error. |
| 2 `coinl` | `coro_noresume` | **on** | `resume_fused` runs its protected part inline (`raw_run_protected_inl`: the body of `raw_run_protected`, no closure call) and takes `resume_finish`'s result count directly when the status is not an error (`precover` does nothing then); errors still go through `resume_finish`. |
| 3 `cowrap` | (code motion, no knob; `git revert` of the commit) | on | `auxwrap` and `co_resume` keep only the fused-resume hot path in the function (fewer callee-saved registers and a smaller frame on it); error / slow / too-many-results go to `#[inline(never)]` functions (`auxwrap_rest`, `co_resume_rest`) that do exactly what the old function did after the fused attempt. |

### 42.2 Where the 753 instructions are (control build, static path counts from the disassembly [E], one pair of `co_yield`)

| piece | instr | removable? |
|---|---:|---|
| tier-1 loop of the caller (`FORLOOP`, `MOVE`, `CALL` stub, `ADD`) | 68 | no |
| `jit_call` for a native C closure (tag tests, `native_fast_with` frame push, call, result move, return code) | ~100 | partly (the push and the result move are protocol) |
| `auxwrap` (thread lookup, `nargs`, result matching; 6 pushes, 0x88-byte frame) | ~45 | step 3 |
| `resume_fused` (11 checks ~45, argument copy ~15, result copy ~25, prologue / epilogue ~20) | ~105 | small |
| `raw_run_protected` closure + `post_call` inline + `finish_op` test + `execute` call | ~50 | step 2 |
| `vm::execute` / `run` / `run_mode` entry (frame head, pc, base) | ~65 | no |
| coroutine body `n = n + 1; coroutine.yield(n)`: `ADDI`, `GETTABUP coroutine`, `GETFIELD yield`, `MOVE`, `CALL`, `JMP` in the interpreter | ~190 | no (LuaJIT interprets it too) |
| `CALL` of `yield`: `pre_call_lua_fast` miss, `native_fast_with` frame push, `co_yield` + `yield_k` (18 + indirect call + a `Result` returned through memory) | ~85 | step 1 |
| exit: `run_mode` epilogue, `rec_on_err`, `execute`, closure `?`, `raw_run_protected` tail | ~40 | small |
| `resume_finish` (`precover`, status tests) | ~19 | step 2 |

About 190 of the 753 are the coroutine's own bytecode and 68 the caller's loop; the protocol is ~500, spread over eight functions of 20-100 instructions each,
every one of them already tight. (perf's sampled per-function shares are not usable for this: `instructions:u` samples skid onto the instruction after a
load stall; the function shares moved by 12 points between two runs of the same binary. IBS and valgrind are unavailable here: no `-a` permission, and
valgrind cannot decode the AVX-512 code of this machine's `ld.so`.) The study's estimate removed "co_yield 34 + the C-call sequence + unwinding" = 40 % of the
pair; the unwinding is a `Result` travelling through three returns (about 40) and the C-call sequence is shared with every native call (the frame push is
what `traceback` needs), so what could go without changing observable behaviour was ~100 of 753.

### 42.3 Results, the coroutine programs (instructions M / cycles Mcyc, min of 9; control = main 505bcfa)

5.5:

| program | control | step 3 only (`off`) | step 1 + 3 (`coro_noresume`) | step 2 + 3 (`coro_noyield`) | all three |
|---|---:|---:|---:|---:|---:|
| lib/co_yield (1.488 M pairs) | 1120.6 / 238.7 | -0.5 % / +2.8 % | -2.3 % / -3.9 % | -2.7 % / -10.7 % | **1071.5 / 217.6 = -4.4 % / -8.8 %** (720 instr per pair) |
| lib/co_wrap | 1087.2 / 245.6 | +0.5 % / +3.9 % | +0.7 % / +3.4 % | -0.1 % / +0.2 % | 0.0 % / -2.1 % |
| programs/coroutines | 8566.6 / 1835.9 | -0.5 % / +1.1 % | -2.0 % / -5.6 % | -2.3 % / -2.7 % | **-3.7 % / -6.3 %** |
| corpus/coro_scheduler | 6568.9 / 1493.7 | -0.1 % / 0.0 % | -0.6 % / +0.2 % | -0.9 % / -0.3 % | -1.3 % / +0.6 % |
| corpus2/dev/coro_generators | 4024.8 / 1289.8 | -0.2 % / -0.1 % | -1.1 % / -0.2 % | -1.4 % / -1.2 % | -2.3 % / -1.3 % |
| generator loop, `for v in gen` (2 M yields) | 1657.3 / 361.2 | -0.5 % / +1.2 % | -1.9 % / -6.3 % | -2.4 % / -4.5 % | -3.9 % / -5.8 % |
| `wrap` called directly (2 M switches) | 1573.7 / 336.6 | -0.8 % / +1.3 % | -2.4 % / -6.2 % | -2.5 % / -2.2 % | -4.2 % / -6.3 % |

5.4 has the same shape (`co_yield` -4.4 % instructions / -1.5 % cycles, `coroutines` -3.7 % / -4.4 %, `coro_scheduler` -1.2 % / -0.8 %, `coro_generators` -2.0 % / -1.4 %). The
interleaved `bench-fast` run of 46 programs (42.6) gives `co_yield` 1121.1 / 244.9 -> 1072.0 / 199.3 (5.5) and 1124.0 / 244.3 -> 1075.0 / 200.9 (5.4), `coroutines`
8567.1 / 1844.4 -> 8248.4 / 1587.2 (5.5) and 8614.4 / 1839.7 -> 8294.7 / 1745.5 (5.4), `coro_scheduler` -1.3 % / -1.2 % instructions, `coro_generators` -2.3 % / -2.0 %. Single cycle
readings of two builds of identical code differ by 2-4 % (the `off` column), so the cycle columns are indicative; the instruction columns are exact.

**The study's -30 % was not reached: -4.4 % instructions on `co_yield` (753 -> 720 per pair), -3.7 % on `programs/coroutines`, -1 .. -2 % on the scheduler and
generator programs, whose cost is mostly their own Lua code.** Cycles follow instructions at IPC 4.6 (31 k branch misses and 27 k L1 misses per 1.1 G instructions: the pair is
throughput- and dependency-bound), and the cycle gain of steps 1 and 2 is larger than their instruction gain (the indirect `co_yield` call with its `Result` through memory and the closure call
are on the dependency chain): -6 .. -9 % on the programs that are all switches.

### 42.4 Not built / tried

* **Entering the compiled frame at the resume** (set `TRAP_JIT` on the resumed frame so that the interpreter's frame head calls `rt::enter` at the instruction after the `yield` call, which is a
  tier-1 re-entry point: the coroutine body would run in tier 1 instead of the interpreter): **+15.6 % instructions** (`wrap` loop 1508 -> 1744 M, `co_yield` 1072 -> 1244 M). `enter`
  (monitor `Rc`, budget, thunk, exit conversion) costs more than the ~190 instructions of interpreted bytecode it replaces. Reverted.
* No C frame at all for `yield` (the study's wording): the frame is what `debug.traceback(co)` / `getinfo(co, 0)` (`[C]: in function 'coroutine.yield'`) and `resume_fused` read; removing it
  would change observable behaviour, so it stays.
* A direct `auxwrap` in `native_fast_with` (like step 1): saves the indirect call and one result copy (~25 instructions, 3 %) and needs the whole of `auxwrap`'s error path in rn-core; not built.
* Special cases for one argument / one result in the copy loops of `resume_fused`: -1.5 M of 1083 M; dropped.
* Per-resume work that is only a heuristic (the interpreter's entry counter `jit_tick_dec`, run once per resumed frame) was left alone: skipping it would change when functions compile.

### 42.5 Tests, mutants, checks

* `tests/vm/co_16_direct_yield.lua` (+ `crates/rn-jit/tests/data/co_16_direct_yield.expected` / `.expected54`, produced by `ref/55` / `ref/54`; test `tr_coro::co_direct_yield_matches_the_reference`;
  also run by `vm_corpus` and `tr_oracle`): yield from the main thread (bare, through `pcall`, tail call); every wanted-result count (0, 1, 2, 3, 5, multiple) and argument count
  (0..3); 300 values (the stack must grow: the call falls back to the library path); `return yield(...)`, method / alias / vararg callers; `table.sort` / `gsub` / `load` / `tostring`
  / `pcall(string.rep, .., yield)` / `xpcall` callbacks (the "attempt to yield across a C-call boundary" cases), `isyieldable` inside them and of suspended coroutines, a yielding message
  handler; yield through `pcall`, `xpcall`, `pcall(yield, ..)`, every metamethod (`__index`, `__newindex`, `__add`, `__lt`, `__le`, `__eq`, `__concat`, `__len`, `__call`, `__unm`), a
  generic-`for` iterator and `for k in yield`; generators and an accumulator through `wrap`; `debug.getinfo` / `traceback` / `getlocal` of a suspended coroutine; call / return / line / count
  hooks on the coroutine thread (and a hook set by the resumer between two resumes, and from inside the coroutine around a yield); `close` with pending `<close>` variables (and an
  erroring `__close`); `status` of normal / running / suspended; errors after a yield, in `wrap` (position prefix), non-string error objects; `resume` of itself; recursion 150 deep
  before the yield; 2000 switches; suspended coroutines with `__gc` objects after `collectgarbage`. Output equal to the reference on 5.5 and 5.4.
* Mutants (registry 815 -> 819; flag bits 7..10 of `coro::set_mut_cofuse`): `coyield_yieldable_skip` (the main-thread / `table.sort` yields), `coyield_nargs_wrong` (one value too many),
  `coyield_status_skip` (a yield that does not mark the thread suspended) and `coinl_err_as_ok` (an error status taken through the inline finish: the `pcall`-after-yield recovery is skipped), all
  caught by `co_direct_yield_matches_the_reference` (`TR_MUT_ONLY=coyield_,coinl_,cofuse_`: 12 ran, 12 caught, with the seven `cofuse_` mutants of BUNDLE-T). Two other candidates were *equivalent* and
  were dropped: the result count of a yield from `top` instead of `u2` (equal by construction) and not restoring `nCcalls` after the inline run (every resume sets it).
* `cargo test -p rn-capi` (the C API `lua_yield` / `lua_yieldk` / `lua_resume` sections, layouts, the official suite through `lua.c`) passes; the official `coroutine.lua`, `cstack.lua`, `db.lua`,
  `events.lua`, `calls.lua`, `locals.lua` pass on 5.5 and 5.4 through rnlua (`-e "_port=true;_soft=true"`); `RUSTYNAIL_GC_STRESS=1` / `2`, `GC_MODE=inc` and `RUSTYNAIL_GC_VERIFY=1` (tiny programs) on
  `co_10` .. `co_16` print what the reference prints (5.5 and 5.4); `co_10` .. `co_16` and three jit-diff coroutine programs (`e1_03`, `h7_06`, `jit_f4_06`) are equal to the reference under
  `JIT=off`, `JIT=always`, `JIT=always TRACE=always`, `JIT=on THRESHOLD=2 TRACE=on TRACE_HOT=2,2` (5.5 and 5.4).

### 42.6 The bench-fast sample (46 programs, 5.5 and 5.4, subject against control interleaved in one run, 9 runs each)

`scripts/bench-fast.sh --matrix 55plain,55pgo,54plain,54pgo --all-full --runs 9` with the subject binaries in the `p55` / `p54` directories and the control in the `g55` / `g54` directories (the
columns are labelled `plain` / `pgo` by the script; both are plain non-PGO builds here), `PREV_COMMIT=5a064d7 FORCE_STALE=1 CANARY=0`: 8 coroutine programs (`co_yield`, `co_wrap`, `coroutines`, `coro_scheduler`,
`coro_generators`, `serpent_ser`, `stream_closures`, `wordfreq`) and 38 others (programs 20, awfy 5, apps 2, real 2, lib 8, jit-trace 1). Geomean subject / control over the 46: instructions
0.9978 (5.5) / 0.9979 (5.4), cycles 0.9932 / 0.9945.

**Instruction bar (+1 %): met on every program.** The largest increases are `real/template_engine` +0.25 % (5.5) / +0.36 % (5.4), `programs/varargs` +0.28 % / +0.29 %, `corpus/serpent_ser` +0.14 % / +0.20 %,
`programs/json` +0.14 %, `lib/find_pat` +0.20 %: the compare of the callee with `coroutine.yield` in the native-call sequence (a load, a compare and a branch per native call).

**Cycle bar (+2 %): five (program, version) readings of the bench run are over it, all at equal or almost equal instructions:** `apps/events` 5.5 +6.1 % (instructions +0.0000 %; 15 interleaved runs: +2.4 %),
`programs/closures` 5.5 +2.6 % (instructions equal to the digit; 15 runs: +2.3 %), `programs/json` 5.5 +2.9 % (+0.14 % instructions; 15 runs: +2.85 %), `lib/fmt_s` 5.5 +2.3 % (26 M instructions, 7 Mcyc; 15 runs +2.5 %),
`lib/find_pat` 5.4 +3.8 % (+0.2 % instructions; 15 runs +0.6 %). The 5.5 programs that stay over it at 15 runs (events, closures, json, fmt_s) are **code layout**: the build with every N2 feature off
(`coro_noyield,coro_noresume`, instruction counts of the control to the digit) shows the same cycles: json 1850.6 (off) vs 1799.4 (control) vs 1854.6 (final) Mcyc, closures 465.5 / 458.2 / 471.3, events
164.9 / 163.7 / 166.9; and the `coro_noyield` build (step 2 only) is back at the control on json (1792.7) and closures (458.3). The interpreter loop and the helpers sit at other addresses after any change
of rn-core: the lottery of docs/PERF.md BUNDLE-T. Reported, not chased.

### 42.7 Gates

* `GATE_CPUS=2-15 scripts/gate-light.sh coyield_,coinl_,cofuse_ --seeds 1-40` at the final commit: OK (407 s: fmt, mutant map, clippy x3, core 5.5 / 5.4, rn-jit trace suites 5.5 / 5.4, vm_corpus, jit-diff 1-40, mutant group). The first
  runs failed two steps, both fixed before the commit that follows them: the frozen registry count of `mutants::tests::registry_is_the_frozen_list` (815 -> 819) and a dead `auxresume` after step 3 (clippy).
* `JITDIFF_MODES="off tr trdef" JITDIFF_ENTERS=1 scripts/jit-diff.sh 1 40` on the final release binaries, 5.5 and 5.4: 40 seeds done, 0 diffs, 0 timeouts, both.
* `cargo test -p rn-capi` (4 tests) and `cargo test -p rn-cli --test cmodule`: OK. Not run: the heavy gate, the full mutant runner, `tr_oracle`, PGO builds, all-223 benchmarks.

### 42.8 Decision

Default: both features on (steps 1 and 2; step 3 is code motion). The instruction bar is met on all 46 programs; the cycle readings over the bar are the layout lottery of identical code (above). If a later
session does not accept that, `--features coro_noyield,coro_noresume` on `rn-cli` is the A/B build. N2 is *not* the -30 % of the study: what is left in the pair is the coroutine's own bytecode, the
caller's loop and a protocol (`jit_call`, `native_fast_with`, `auxwrap`, `resume_fused`, `execute`, `run_mode` entry / exit) whose every part is 20-100 instructions; the next reduction would need a
different protocol (one fused native-call-to-resume path in rn-core with the `wrap` / `resume` library functions inside it, ~-3 %, and a tier-1 body that is re-entered at a lower cost than `enter`, which is
more than the interpreter costs now).

## 43. TR24c (in-place `Unstable` restarts, knob `restarts`) and T2b (cheaper recording at identical code) (2026-11-23, base 70c4ba8)

Method (light): release `rn-cli` builds (`scripts/release-build.sh --no-pgo`, 5.5 and 5.4), `RUSTYNAIL_SEED=0`, **control = a build of main 70c4ba8 made in the same session**; instructions of one
run (`perf stat instructions:u`, deterministic), cycles = min of 5-9 interleaved runs pinned to CPU 4 (`cycles:u`). Sample = 30 programs (awfy 14, apps 5, corpus2/heldout 6, corpus2/dev 4,
corpus 1) plus the 128 programs of `awfy`, `apps`, `programs`, `corpus`, `corpus2/dev`, `corpus2/heldout`, `jit-trace`, `real` for the instruction sweep of 43.1. Not `bench-fast.sh`: the same
quantities (instruction counts of the pinned run, min of interleaved pinned cycles) were taken by hand because the control had to be the same session's main build for both versions.

**A fact that decides how to read every table below.** Binaries that differ only by a field or a function are not formation- or cycle-equal. With the knob at its default (`restarts` 0 = main's
behaviour) the commit-1 build already moves 5.5 `scheme_interp` 5892.3 -> 5937.1 M (+0.8 %) and 5.4 `prototype_objects` 2977.0 -> 3818.6 M (+28 %), `expr_lang` 623.8 -> 597.8 M, `parser_combinator`
10111.5 -> 9800.5 M; `prototype_objects` 5.4 under the hot-threshold sweep (`RUSTYNAIL_JIT_TRACE_HOT` 200 / 230 / 248 / 252 / 256 / 264 / 300 / 400) is 3189 3328 3574 3563 **2977** 4075 3589 3631 on the
main build and 3278 3557 3765 3249 **3818** 3606 3656 3196 on this branch (means 3494 / 3515): the shipped threshold lands on the lucky end for main and on another for the branch. The formation of
the trace tree depends on something that any struct-size change shifts (heap addresses behind hash-ordered maps; not traced to its source here). Instruction-equal cycle misses are the layout of
the code (`regex_nfa` 5.5: instructions 3573.0 / 3573.0 / 3572.0 M on main / commit 1 / commit 2, cycles 686 / 672 / 726 M, L1 icache misses 20.7 / 3.6 / 7.2 M in one run each; two runs of main: 688 / 686).

### 43.1 TR24c: in-place `Unstable` restarts (commit 1)

**What was built.** The recorder already carried the prototype of section 3.3 (`RecCfg::restarts`: at the closing step of a root recording, a slot whose type at the end of the iteration
differs from the type its first read checked restarts the recording at the head from the types the finished iteration produced, instead of aborting `Unstable` and waiting for the next
back-edge); nothing set it. Now: knob `restarts` of `RUSTYNAIL_JIT_TRACE_LIMITS` (`restarts=N`, 0..4, `TrLimits::restarts`, **default 0** = `RESTARTS_PROCESS`), wired in `driver_start` for root
recordings (a side recording never restarts: its `Unstable` test does not exist). `Rec::reset` was completed for the state that accumulates over an iteration and that `new` starts empty
(`atg_tabs`, `hs_guarded`, `str_allocs`, `idfolded` / `midfolded`, `unr_started`, the pending string state); the other fields are either reset already or only set at the end of a recording.
Counted: `[trace] restarts: N` in the statistics; `RUSTYNAIL_JIT_VERBOSE=3` prints `[trace] unstable: slot S T0 -> T1 at head H`. Tests: `tr_core::tr24c_restarts_programs`
(`tests/vm/tr24c_01_restarts.lua`: integer accumulator that becomes a float, nil slot that becomes a table, a type that flips every iteration, a string / number slot with an error under `pcall`,
allocation with collections; x `always` / `on2` x `restarts` 0 / 1 / 2, `+verify`, exit storm, emulator, budgets 3 / 17 / 200, GC stress 1 / 2 in both modes, stack-move stress, all against the
interpreter; `restarts=0` restarts nothing, `restarts=1` / `2` restart), `tr_policy::tr24c_restarts_policy`, the existing `tr_record::unstable_rerecords_from_the_new_types`; mutants
`restarts_never`, `restarts_unbounded` (both caught). Registry 819 -> 821.

**Numbers (instructions M; 5.5 first, 5.4 in brackets; main / `restarts=1`):**

| program | main | restarts 1 (2 and 3 equal) | cycles M (min of 5) |
|---|---|---|---|
| apps/raytrace | 349.5 (346.3) | **337.8 (-3.3 %)** (346.3, inert) | 112.2 -> 105.9 (-5.6 %) |
| roguelike_turns | 2788.0 (3081.3) | 2790.8 (+0.1 %) (3081.2, inert) | |
| real_dis_mips | 7315.2 (7506.1) | 7312.6 (7509.6, +0.05 %): **the 5.4 `heatside` formation difference is not removed** | |
| lua_parser | 4998.4 (5921.0) | **5157.9 (+3.2 %)** (5973.9) | 1294.2 -> 1315.9 (+1.7 %) |

The 128-program sweep (restarts=1 against restarts=0 of the same binary): 5.5 changes raytrace -3.3 %, lua_parser +3.2 %, real_dis_arm +0.45 %, nothing else above +-0.3 % (`astar_jumper` is
non-deterministic); 5.4: lua_parser +0.4 %, real_dis_arm +0.55 %, nothing else. **It is a formation lottery, not a systematic gain**: with `restarts` 0 / 1 at the hot thresholds 200 / 230 / 248 / 252 / 264 /
300 / 400 raytrace is 341.7 343.7 343.4 342.1 338.2 346.7 365.7 against 341.7 343.7 347.6 342.1 338.2 346.7 365.7 (equal at six of seven, +1.2 % at one), roguelike_turns equal at six of seven
(+7.5 % at 400), lua_parser 5084 5063 4931 5023 5050 5047 5050 against 5181 4943 5045 4994 5024 5068 5050 (mean 5035 / 5043). The restarts that happen: raytrace 1, lua_parser 2 (slot 5 `Int` -> `ShrStr`
at the `pairs` loop of `countNodes`, which the old rule blacklists after four `Unstable`s, and `True` -> `ShrStr`), roguelike_turns 2 (slot 5 `Nil` -> `Num`). roguelike_turns is not helped at the default
threshold (+0.1 % either way): the "-3.9 % on raytrace, roguelike cured" lead of sections 39 / 40 was a lottery landing of the scratch build.

**Default: OFF (0).** The bar (no program over +1 % instructions) is missed by lua_parser +3.2 % (5.5; +1.7 % cycles), and the gain on raytrace does not hold over thresholds. Flip
`RESTARTS_PROCESS` to 1 to take it. Verification: `jit-diff.sh 1 40` modes `off tr trdef`, limits `lockstep=1` and `lockstep=1,restarts=2`, on the 5.5 and 5.4 release binaries: 0 diffs, 0 timeouts;
the official suite (`vendor/lua-tests-5.5` 32 files, `-5.4` 31 files) with `RUSTYNAIL_JIT=always RUSTYNAIL_JIT_TRACE=always RUSTYNAIL_JIT_TRACE_LIMITS=restarts=2`: 0 failures.

### 43.2 T2b: cheaper recording at identical code (commit 2)

**Profile** (`perf record -e instructions:u -c 20000 --call-graph fp --user-callchains` of a frame-pointer build in its own target directory, removed; inclusive shares of the
program's instructions at the bench size):

| program | recorder (`driver_step` -> `Rec::on_step`) | pipeline (`finish_outcome`) | of the pipeline: RA / assembler + x86 / `opt_loop::roll` / exits / sink |
|---|---:|---:|---|
| apps/raytrace | 16.2 % (`record_one` 9.4: `hash_get` 5.0; `guard_with` -> `snap::take` 2.7) | 17.0 % | 5.1 / 2.9 / 1.3+0.9 / 1.9 / 2.4 |
| awfy/cd | 7.3 % | 22.1 % | 4.3 / 7.2 / 4.3 / 2.7 / 1.3 |
| awfy/towers | 5.9 % | 16.5 % | 3.0 / 5.5 / 4.4 / 1.8 / - |
| awfy/richards | 5.3 % | 13.9 % | 2.7 / 5.0 / 3.1 / 1.3 / - |

`Vec` growth (reallocations, malloc / realloc / memcpy) is 4.9 % of raytrace and 6.5 % of cd, spread over about thirty sites (`Vec<Vec<FrameRestore>>` of the exit builder 0.8 % in cd, the
register allocator's per-instruction `pre` / `post` / `stores` vectors 0.6 + 0.3 %, `snapmap` 0.3 %, `ins` 0.3 %). There is no single hot spot: every function of the recorder and of the
back end is below 2 % self, which is what T2 left. What is cheap and exact was done:

1. **Timers.** The `phase::now` / `add` pairs (four `rdtsc` per recorded bytecode: `rec_step` and `lockstep`) run only when the statistics that print them are on (`RUSTYNAIL_JIT_VERBOSE >= 1`);
   a timer read while off is 0 and `add` ignores it.
2. **`snap::take` builds the map in place** at the end of `ir.snapmap` (`SlotMap::entries_into`, `TraceIr::snapshot_built`): no temporary vector per snapshot (allocate, fill, copy, free),
   the function-slot entries merged by `binary_search` / `insert` on the tail. Unit test `snap::tests::take_builds_the_map_in_place` against the old composition (replace, insert, no frames).
3. **Presized recorder vectors** (`Builder::for_recording`: `ins` 512, `snaps` 64, `snapmap` 1024) and the per-snapshot `frames` / `slots` of `build_exits`.
4. `[trace] abort` lines of `RUSTYNAIL_JIT_VERBOSE=3` carry the recorded step count (used below).

**Identical code.** `RUSTYNAIL_JIT_DUMP` of the commit-1 build and the final one (`trace-N.txt` normalised, `.bin` sizes, proto sizes, the `[trace]` statistics without the entry /
exit counters): raytrace (43 traces), cd (196), richards (33), towers (27), deltablue (47): **SAME** on all five (the harness run on one binary twice gives SAME on four).

**Instructions** (M; main / this; 5.5, then 5.4): raytrace 349.5 / 346.9 (-0.74 %), 346.3 / 343.6 (-0.78 %); cd 768.4 / 764.4 (-0.52 %), 835.8 / 831.5 (-0.51 %); richards 173.5 / 172.8 (-0.40 %),
174.4 / 173.7 (-0.40 %); towers 173.7 / 173.0 (-0.40 %), 175.7 / 175.0 (-0.40 %); deltablue 318.2 / 316.5 (-0.53 %), 341.4 / 339.6 (-0.53 %); json -0.27 %, lru_cache -0.5 %. Geomean over the 30-program
sample 0.9987 (5.5), 0.9971 (5.4 without `prototype_objects`). Cycles (min of 7, CPU 4, 5.5): raytrace 109.8 / 108.0 (-1.6 %), cd 256.4 / 255.0 (-0.5 %), richards 60.2 / 59.7 (-0.8 %), towers 47.2 / 46.9
(-0.6 %), deltablue 107.1 / 106.8 (-0.3 %). **This is far from the study's "halve rec_step": about 0.5 % of instructions per compile-heavy program.** The study's 1245 tsc per bytecode measured 660 (unpinned) to 1000 (pinned, timers on) at the start of this section (after T2), and the timer-on reading is no longer the shipped cost; what is left is the IR volume itself (about one guard, therefore one snapshot of 10-60 entries, per recorded bytecode), not an inefficiency
of one function.

**Wasted recordings of raytrace.** `recorded 22 installed 7` (roots; sides 48 / 36): the 15 failed roots are `TooLong` 5 (the four counted in the step-count build ended after 530 recorded steps on average: 2109 steps), `Compile(pass)` 4
(`too many snapshots` in `split_type_snaps`, **after** the whole recording, `sink` and the loop pass), `InnerLoop` 2 (74 steps each), `MultRet` 2, `DepFreed` 1, `Unstable` 1: about 2800 of the 11344
recorded steps (25 %, 3-4 % of the program's instructions) plus the pipeline front of the four `Compile(pass)` ones. Every `unchangeable` abort is already blacklisted at its second
occurrence with the same reason (`on_abort_with`: `repeat`); blacklisting at the first one removes 2 `TooLong`, 1 `InnerLoop`, 2 `Compile(pass)` here (about 1.5 % of raytrace) but is a
formation policy (the second recording is a different iteration), not a cost cut, and was **not built**. Predicting `too many snapshots` at the closing step is not exact (the loop pass
and the sink change the snapshot count before `split_type_snaps` counts it): not built.

**Bar.** Instructions: no program of either sample is over +1 % except 5.4 `prototype_objects` (+28 %, the formation lottery above: commit 1 alone moves it identically with the knob off; the
shipped threshold is lucky for main) and 5.5 `scheme_interp` (+0.74 %, same cause, also at commit 1). Cycles (min of 5, 5.5 / 5.4): over +2 % at equal instructions: `regex_nfa` 5.5 +6.5 % (code layout:
icache misses above), `apps/events` 5.5 +1.5 % (control run twice: 167.2 / 165.4, commit 1 168.8, this 167.4: inside the spread), 5.4 `prototype_objects` +16 %, `nbody` +4.3 %, `sieve` +1.9 %,
`lru_cache` +1.4 % (5.4 spreads of identical-instruction runs). Cycle geomean over the sample 0.9980 (5.5), 0.9926 (5.4).

### 43.3 Gates and what was not run

* Commit 1: `gate-light.sh restarts_` OK after two fixes before the commit (fmt of a long assertion; the frozen registry count 819 -> 821); mutant group `restarts_` 2 / 2 caught.
* Commit 2: fmt, clippy x3, core 5.5 / 5.4, trace suites 5.5 (172 s) / 5.4 (162 s), vm_corpus, jit-diff 1-50 PASS; `mutant-map` regenerated (line shifts); mutant group `snap_drop_last,snap_prune_dead,restarts_`
  4 / 4 caught (425 s). The light gate's automatic group (160 mutants chosen from the diff) was stopped after the other steps had passed and replaced by that group.
* Commit 3 (the `[trace] unstable` log line of `RUSTYNAIL_JIT_VERBOSE=3`, this section): all steps PASS (`mutant-map` failed once on the shifted lines and was regenerated and re-checked after it ran), mutant group `restarts_` 2 / 2.
* Not run: `gate-heavy.sh`, the full mutant runner, `tr_oracle`, PGO builds, all-223 benchmarks, `bench-fast.sh` (see the method note).

## 44. T6 (tier-1 residue paths), T8 (pattern `find` / `match` on trace) and `blfirst` (blacklist at the first unchangeable abort) (2026-11-23, base fdc8eda)

Method (light): release `rn-cli` builds (`cargo build --release -p rn-cli`, 5.5; `RUSTFLAGS="--cfg rustynail_lua54"` and `LUA_VERSION=5.4`, 5.4), `RUSTYNAIL_SEED=0`, **control = a build of main fdc8eda made in this session** (5.5 from the worktree before the first
edit, 5.4 from `git archive`), binaries in equal-length directories; instructions of one run (`perf stat instructions:u`, deterministic), cycles = min of 7 interleaved runs pinned to CPU 4 (`cycles:u`; 11 runs for the re-checks).
Sample = the 30 programs of section 43 (awfy 14, apps 5, corpus2/heldout 6, corpus2/dev 4, corpus 1). Per-item attributions are the **same binary** under `RUSTYNAIL_JIT_TRACE_LIMITS` (the three knobs below, env of equal length) because
binaries that differ in any struct or function are not formation-equal (section 43). Not `bench-fast.sh`, no PGO, no all-223 sweep, no `gate-heavy.sh`.

Knobs (all in `RUSTYNAIL_JIT_TRACE_LIMITS`, defaults are the struct defaults of the tests and the process defaults):

| knob | meaning | default |
|---|---|---|
| `t6=N` | bit 0 = fused `NEWTABLE` + constant-key `SETFIELD`s in tier 1 (`templates_ctor.rs`), bit 1 = `math.min` / `math.max` of one integer and one float inline in tier 1 (`t1inline.rs`); traces on only, like `tset` | 3 |
| `libcall=N` | the BUNDLE-H / R mask, two new bits: 1024 = `string.find` with a pattern without captures (`LIB FINDP`), 2048 = `string.match` with no capture or one string capture (`LIB FINDM` + `SUB`) | 4095 (was 1023) |
| `blfirst=N` | mask of the `unchangeable` abort reasons that blacklist a head at the first occurrence: 1 `TooLong`, 2 `Compile(..)`, 4 `InnerLoop`, 8 the other unchangeable reasons, 16 = also for the exits of side traces | 3 |

### 44.1 T6: the tier-1 per-call cost of the constructor idiom (commits `t6 bit 0`, `t6 bit 1`)

**Where the instructions of one `vec(i, i, i)` go in tier 1** (`mb_vec.lua`: `for i = 1, N do keep[i % 64 + 1] = vec(i, i, i) end`, `vec` = `return setmetatable({x = x, y = y, z = z}, V)`, per iteration = (N = 1.2 M minus N = 0.2 M)
/ 1 M, tier 1 only; instruction-sampled `perf record -c 100000`, shares of the run times the per-iteration total):

| | study / `RUSTYNAIL_JIT_TRACE=off` (J4 tier 1) | tier 1 inside a trace-enabled run (control) | after `t6` bit 0 |
|---|---:|---:|---:|
| total per iteration | 1717 | 935 | **798** |
| code of `vec` (NEWTABLE call glue, 3 SETFIELD, GETTABUP, inline `setmetatable`, tail return) | 451 | 388 | 301 |
| the loop and the Lua call of `vec` | 127 | 171 | 137 |
| `jit_set` 253 + `psetshortstr_after` 125 (the three new keys through the helper) | 378 | 0 (`tset` bit 4, H2b, inline) | 0 |
| `post_call`, `base_setmetatable`, `precall_c`, `jit_tailcall`, `pre_tail_call`, `get_metafield`, `set_metatable` (the C call) | 417 | 0 (H2a, `t1inline` bit 10, inline) | 0 |
| `jit_newtable` + `State::new_table` + `resize` + `setnodevector` (NEWTABLE with a hash part, the pooled blocks) | 227 | 266 | 246 |
| young collection (`list_remove_after`, `propagatemark`, `gc_step`, `reallymark`) | 100 | 105 | 107 |

The 1700 instructions of the study are the J4 numbers (`RUSTYNAIL_JIT_TRACE=off` keeps tier 1 as J4: no `tset` inline paths, no `t1inline` rows); in a normal run H2a / H2b / H2c already took the `setmetatable` C call and the new-key helper out
(935 per `vec` is the residue the handle is about). What was left was the probe-and-insert of every `SETFIELD`: a hash chain walk that misses (about 50 instructions), then `Cold::NewKey`
(dummy / black / nil / metatable / main-position / non-empty tests, the stores, the flags and the layout id: about 60), three times.

**What was built.**

1. `t6` bit 0, `templates_ctor.rs`. A `NEWTABLE` with a hash size followed by `SETFIELD`s of distinct constant short-string keys into its register: the main position of every key in the presized node vector is known when the function is
   compiled (the PUC hash of a constant is fixed, `2^(B-1)` nodes), so when no two keys of the group share a main position each key is a store into a free node of a fresh table. The fused code is emitted behind the helper call of the
   `NEWTABLE` template and jumps over the generic templates of the group (which stay in place as the fall-through): guards before the first store: the table is not black (a collection step inside the helper marks a table that is
   reachable from the stack), no stored value is nil (a nil store is a no-op that must not create the key). Per key: key tag, key pointer, value tag, payload at `node + pos * 24`; once: `flags &= ~MASKFLAGS`, the layout id slot
   at `node - 16` cleared for 8 nodes or more. A key whose main position is taken ends the group (the instructions from there on are the generic templates, which find the earlier keys where the interpreter's insertions put them), as does
   anything that is not a `SETFIELD` of the same register with a constant short-string key and a non-nil constant or a register other than the table. No barrier (the table is not black), no `__newindex` (it has no metatable).
2. `t6` bit 1, `t1inline.rs::t1i_minmax`: raytrace's `min(1, c.x)` / `max(0, n:dot(l))` have one integer and one float operand, which the inline path (two integers or two floats) sent to `jit_call` (`math_min` + `jit_call` + `precall_c` +
   `post_call`: about 4 % of raytrace's instructions). One integer and one float now compare as `lua_compare` does (the integer converted, exact for `|i| <= 2^53`; beyond that the generic path; NaN never wins), the result is the winning operand with its
   own type. This is the "leaf call" of the handle; the `setmetatable` call, the Lua call protocol and the tail call were already the tier-1 inline paths of H2a / H2c / E9 (the remaining ~137 of the loop are the call of `vec`, E9's domain).

Measured: **`vec(i, i, i)` in tier 1 935 -> 798 instructions (-14.6 %), 193 -> 160 Mcycles per 1.2 M iterations (-17 %)**: not the "halve" of the handle, which would need the allocation (setnodevector with its pooled block, `new_table`,
the Lua call) as well, both of which were left alone (the allocator accounting is shared with every program; the call protocol is stage 6 of DESIGN_CALL_PROTOCOL).

| instructions M (same binary, the other knobs off) | `t6=0` | `t6=1` | `t6=3` |
|---|---:|---:|---:|
| 5.5 apps/raytrace | 335.3 | 322.2 (-3.9 %) | 313.2 (-6.6 %) |
| 5.5 awfy/cd | 763.8 | 757.0 (-0.9 %) | 757.0 |
| 5.5 corpus2/dev/lua_parser | 5078.0 | 5028.0 (-1.0 %) | 5028.0 |
| 5.5 corpus2/dev/expr_compiler | 5672.2 | 5622.0 (-0.9 %) | 5622.0 |
| 5.4 apps/raytrace | 348.1 | 338.2 (-2.8 %) | 329.6 (-5.3 %) |
| 5.4 awfy/cd | 833.1 | 826.8 (-0.8 %) | 826.8 |

(the other programs of the sample: no change above 0.25 % under either bit.) Tests: `tr_core::t6_ctor_programs` (`tests/vm/t6_01_ctor.lua`: record constructors, nil values, metamethod-name keys,
60 + 90 generated key sets with and without a foreign store behind them, collections, errors; every job against the interpreter, new trace mode `t1x` = traces on and nothing hot, so everything runs in tier 1; `always` / `on2`; `t6` 0 / 1;
`+verify`, `+emurun`, budgets 3 / 17 / 200, GC stress 1 / 2 in generational and incremental mode, with the heap verifier, stack-move stress; three more string-hash seeds), `tr_core::t6_minmax_programs` (`t6_02_minmax.lua`: all pairs of 36
edge values), `tr_shape::shape_tier1_ctor_clears_the_id`. Mutants `ctor_nil_value`, `ctor_flags_skip`, `ctor_layout_skip`, `ctor_mainpos_off`, `ctor_collide`, `ctor_any_reg`, `t6_mm_big_skip`, `t6_mm_conv_skip` (8, all caught;
registry 823 -> 831 with them). The black-table guard has no mutant: a fresh table is black only when the collector step inside the `NEWTABLE` helper marked it (an incremental cycle's atomic phase), which no program can force;
`ctor_black_skip` and a duplicate-key mutant were written, found not to be detectable (the first) or unreachable (the second: a repeated key is a collision at its own position) and removed. The guard is the one of `Cold::NewKey`, whose
mutant `newkey_black_skip` is caught on old tables.

### 44.2 T8: pattern `find` / `match` on trace (commits `libcall bit 10`, `libcall bit 11`)

**Which library calls dominate** (`debug.sethook(.., "c")` counts, JIT off, default size; the first rows are the programs the study names):

| program | C calls |
|---|---|
| corpus/lua_lexer | `byte` 8.05 M, `sub` 2.55 M, `char` 0.96 M, **`find` 0.53 M**, `format` 90 k, `concat` 75 k |
| corpus2/dev/lua_parser | `sub` 1.80 M, **`find` 1.71 M**, `type` 1.07 M, **`match` 0.28 M**, `concat` 81 k |
| apps/expr_lang | **`find` 102 k**, `insert` 27 k, `sub` 23 k |
| corpus2/dev/text_adventure | `remove` 0.81 M, `format` 0.80 M, `gmatch` 0.80 M, `max`, `lower`, `floor` 0.80 M each, `concat` 224 k, `tostring` 36 k |
| awfy/json | `sub` 305 k, `insert` 292 k, `concat` 24 k |
| corpus2/heldout/stream_closures | `max` 2.8 M, `remove` 25 k, `match` 25 k |
| apps/markdown | `gsub` 65 k, `match` 58 k, `insert` 28 k |

`table.concat` (24-224 k calls, the work is the copy), `table.insert(t, pos, v)` (events: 8.5 k calls; the positional form does not occur in the hot loops of the sample), `string.char` with more than three
arguments (none: the lexer calls it with one) and `tostring` of strings / tables (36 k in text_adventure) were **not built**: their call counts are one to three orders of magnitude below the pattern calls, and the calls that are
recorded already (`byte`, `sub`, `char`, `gmatch`, `max`, `remove`, `lower`, `format`) are the ones the gaps of `lib/` (`byte_multi`, `str_lt_short`, `tremove_pop`, `find_plain`) are made of: their profile is the trace code and
the helper (`byte_multi` 96 % in `TRACE-1`, `str_lt_short` 85 % in `jit_tr_lib_cmp` + libc `memcmp`), not a missing recording.

The two that pay: **a `string.find` / `string.match` whose pattern has specials aborted the recording `Nyi(Call)` at the call** (`record.rs` `lib_find`: "a real pattern"), so every loop with one stayed in tier 1 (lua_lexer, lua_parser,
expr_lang, expr_compiler, real_dis_mips).

* `libcall` bit 10 (`LIB FINDP`, `ir.rs::lib::FINDP`): the recorder runs the library's matcher once at record time (`rn_std::pattern::find_with`) to learn the outcome (match / no match; a capture, a position capture or a pattern error stays
  `Nyi`), then emits `FINDP(s, p, init)` and guards the outcome seen. The helper (`asm.rs::findp_core`) runs the same matcher on the operands (so a pattern that changes at run time is searched with the new pattern) and answers 1 (no
  match), `end << 32 | (start + 1)` (start and end of the match, 1-based, both below 2^31), or 0 when the matcher finds a capture, a position capture or an error or a string has 2^31 bytes or more: the guard
  then exits and the interpreter runs the real call. The IR splits the word (`BAND`, `SUB`, `SHR`); a call that wants only the start does not compute the end. The lockstep model reads the packed word from the two result slots (`pend_str` marker
  `FINDP_PACK`).
* `libcall` bit 11 (`LIB FINDM`): `string.match` with no capture or exactly one string capture. `FINDM` answers the same packed span of the *result* (the capture, or the whole match), the recorder follows it with the existing `SUB(s, start, end)`,
  which makes (and interns) the result string, its own recorded-kind guard covering the short / long class. The model value of the `FINDM` result is a literal computed by the recorder (`FINDP_LIT`: the VM slot holds the matched string,
  not the span).

| instructions M, same binary, the other knobs off | `libcall=1023` | `=2047` (+ FINDP) | `=4095` (+ FINDM) |
|---|---:|---:|---:|
| 5.5 corpus/lua_lexer | 3968.4 | 3333.8 (-16.0 %) | 3333.8 |
| 5.5 corpus2/dev/lua_parser | 5078.0 | 4663.6 (-8.2 %) | 4402.3 (-13.3 %) |
| 5.5 corpus2/dev/expr_compiler | 5672.2 | 5672.3 | 5302.3 (-6.5 %) |
| 5.5 corpus2/heldout/real_dis_mips | 7281.9 | 7281.9 | 7018.8 (-3.6 %) |
| 5.5 apps/expr_lang | 568.0 | 513.3 (-9.6 %) | 513.3 |
| 5.5 lib/find_pat | 1628.2 | 1289.4 (-20.8 %) | |
| 5.4 corpus/lua_lexer | 4494.7 | 3450.3 (-23.2 %) | 3450.1 |
| 5.4 corpus2/dev/lua_parser | 6024.7 | 5695.4 (-5.5 %) | 5425.3 (-9.9 %) |
| 5.4 corpus2/dev/expr_compiler | 5813.5 | 5813.5 | 5571.7 (-4.2 %) |
| 5.4 corpus2/heldout/real_dis_mips | 7513.9 | 7513.9 | 7233.9 (-3.7 %) |
| 5.4 apps/expr_lang | 596.2 | 568.3 (-4.7 %) | 568.3 |

(the rest of the 30-program sample moves by less than 0.05 % under either bit; `lib/find_cap` (a capture with `find`: the results include the captures) is unchanged: 1634.0 = 1634.0.) The study estimated -2..-4 % for lua_lexer; the real
share was larger because the aborted recording left the whole loop (the lexer's dispatch chain, not only the call) in tier 1.
Tests: `tr_core::t8_findp_programs` (`t8_01_findp.lua`: a lexer loop, start only, init values, empty matches, captures that stay calls, a pattern that turns into a capture after a long run of capture-free ones, malformed patterns under
`pcall`, long strings, tail calls) and `t8_findm_programs` (`t8_02_findm.lua`): `always` / `on2` x `libcall` off / on, `+verify`, `+emurun`, exit storm, budgets 3 / 17 / 200, GC stress 1 / 2 in both collector modes; every job against the
interpreter; `always` installs more traces with the bit on. Mutants `findp_init_skip`, `findp_caps_skip`, `findp_end_off`, `findp_outcome_skip`, `findm_span_off`, `findm_caps_skip`, `findm_outcome_skip` (7 caught; two of them,
`findp_caps_skip` and `findm_caps_skip`, survived the first versions of the programs, which asked only for the first result of the changed pattern: sections (10) of both programs were added). Registry 831 -> 838.

### 44.3 `blfirst`: blacklist at the first `unchangeable` abort (commit `T2b/blfirst`)

Section 43.2 left it as a policy question: an `unchangeable` abort (`TooLong`, `Compile(..)`, `InnerLoop`, `Nyi`, ...) blacklists a head at its *second* identical occurrence (`on_abort_with`: `repeat`); blacklisting at the first would
save the second recording. Built as a knob (`TrLimits::blfirst`, mask above; `cache.rs::blfirst_applies`; roots and, with bit 16, the exits of side traces). Instructions M, 5.5, same binary (the build of the first two commits: the constructor fusion on,
min / max and the pattern ops not yet there), `RUSTYNAIL_JIT_TRACE_LIMITS=blfirst=<mask>`:

| `blfirst` | raytrace | cd | richards | towers | deltablue |
|---|---:|---:|---:|---:|---:|
| 0 (the old rule) | 322.2 | 757.1 | 172.6 | 172.8 | 316.6 |
| 1 (`TooLong`) | 316.7 (-1.7 %) | 757.1 | 172.6 | 172.8 | 316.6 |
| 2 (`Compile(..)`) | 314.7 (-2.3 %) | 757.0 | 172.6 | 172.8 | 316.6 |
| 3 | **309.1 (-4.1 %)** | 757.1 | 172.6 | 172.8 | 316.6 |
| 4 (`InnerLoop`) | 322.0 | 755.8 (-0.2 %) | 172.6 | 172.8 | 320.6 (**+1.3 %**) |
| 7 | 308.8 | 755.8 | 172.6 | 172.8 | 320.7 (+1.3 %) |
| 15 (every unchangeable reason) | 308.7 | 758.0 (+0.1 %) | 172.6 | 172.8 | 320.6 (+1.3 %) |
| 31 (and the exits of side traces) | 305.0 (-5.4 %) | 758.7 (+0.2 %) | **219.9 (+27.4 %)** | **181.5 (+5.0 %)** | 314.6 |

Bits 4, 8 and 16 cost deltablue, richards and towers (an `InnerLoop` / `Nyi` at a side exit is a property of the path of that recording, not of the exit, which the second occurrence tells: the rule exists for a reason); **bits 1 and 2 (default 3)
are what the study's "wasted recordings" are**: `TooLong` and `Compile(pass)` recordings are about 25 % of raytrace's recorded steps, and a recording that is too long or has too many snapshots is that again every time. 30-program sample,
`blfirst=3` against 0 (5.5): raytrace -4.1 %, expr_lang -4.3 % (its `Compile` failures), nothing else beyond +0.02 % (`bounce` ... `lua_lexer`). **Is the effect systematic?** Over the hot thresholds of sections 40 / 43
(`RUSTYNAIL_JIT_TRACE_HOT` 200 230 248 252 **256** 264 300 400, instructions M, 5.5, `blfirst` 0 -> 3): raytrace 327.6 -> 316.8 (-3.3 %), 334.2 -> 317.7 (-5.0 %), 319.2 -> 308.8 (-3.2 %), 334.7 -> 320.5 (-4.2 %), **322.2 -> 309.1 (-4.1 %)**,
336.2 -> 309.2 (-8.0 %), 323.8 -> 318.4 (-1.7 %), 348.6 -> 335.4 (-3.8 %): a gain at all eight, 1.7 to 8.0 %; cd, richards, towers, deltablue unchanged (+-0.02 %) at all of them; expr_lang equal at 200 / 230 / 248, -4.6 % at 252, **+3.1 % at 264**,
-1.5 % at 300, -2.0 % at 400 (a formation landing: its second recording at that threshold is the one that compiles). 5.4 shows the same shape: raytrace -4.7 %, expr_lang -2.6 %, and one miss at the shipped threshold: **`stream_closures` +13.8 %
instructions** (3191.7 -> 3633.0 M; cycles -2.7 %) which the threshold sweep shows to be a landing (the program's instructions at `blfirst=0` alone range 3163 .. 3686 over the eight thresholds; `blfirst=3` against 0 per
threshold: -6.2, 0.0, -2.6, -14.4, -0.1, -0.1, -0.1 %, and +13.8 % at 256: mean -1.2 %). The bar (no program over +1 % instructions) is therefore missed once, on 5.4, and the knob is shipped on: flip `BLFIRST_PROCESS` to 0 to undo it.

**"Too many snapshots" predicted earlier: not built, and why.** `split_type_snaps` fails when it needs a snapshot past `MAX_SNAPS` (500) for the type-check guards that share one. Over the 30-program sample (`blfirst=0`, 1923 compiled
recordings and 6 failed ones) the recordings that compiled have at most **263** snapshots when they close (stream_closures; everything else <= 250), the five that failed `too many snapshots` are all raytrace's, **454 - 493** at the
closing step (the sixth failure is `install: Refused`). A predictor "abort at the closing step when the recording holds 300 or more snapshots" would therefore be exact on this sample, and would save the pipeline front of the failing
recordings (sink, loop pass, the part of `split_type_snaps` before it fails: about 0.2-0.3 % of raytrace after `blfirst` has removed the second attempt), but it is not sound in general: the bound is a property of the passes (the type
check guards sharing a snapshot and the peeling of the loop add up to 2x the recorded count), not of the recorder's count, so any threshold below 499 can refuse a recording that would have compiled, and the exact count needs the passes. It
buys little next to `blfirst` (which removes the repeat) and was left out.

### 44.4 All three against main (5.5 and 5.4, 30-program sample, min of 7, CPU 4)

Instructions and cycles M, control (main fdc8eda, same session) -> this branch; only the rows that moved by more than 1 % instructions or 2 % cycles, the others are within +-0.5 % instructions and +-1.5 % cycles:

| program | 5.5 instructions | 5.5 cycles | 5.4 instructions | 5.4 cycles |
|---|---|---|---|---|
| apps/raytrace | 335.3 -> 300.0 (**-10.5 %**) | 104.2 -> 95.0 (**-8.8 %**) | 348.1 -> 313.1 (**-10.0 %**) | 113.0 -> 104.7 (**-7.4 %**) |
| apps/expr_lang | 568.0 -> 512.7 (-9.7 %) | 147.1 -> 128.5 (-12.6 %) | 596.2 -> 552.1 (-7.4 %) | 151.3 -> 138.3 (-8.6 %) |
| corpus/lua_lexer | 3968.4 -> 3333.8 (-16.0 %) | 987.5 -> 819.1 (-17.1 %) | 4495.4 -> 3450.1 (-23.3 %) | 1143.5 -> 852.7 (-25.4 %) |
| corpus2/dev/lua_parser | 5078.4 -> 4360.4 (-14.1 %) | 1260.4 -> 1177.7 (-6.6 %) | 6024.6 -> 5397.3 (-10.4 %) | 1548.7 -> 1442.6 (-6.9 %) |
| corpus2/dev/expr_compiler | 5672.2 -> 5290.9 (-6.7 %) | 1298.3 -> 1210.3 (-6.8 %) | 5813.5 -> 5560.9 (-4.3 %) | 1342.0 -> 1259.2 (-6.2 %) |
| corpus2/heldout/real_dis_mips | 7281.9 -> 7018.8 (-3.6 %) | 2213.5 -> 2087.4 (-5.7 %) | 7514.0 -> 7233.9 (-3.7 %) | 2266.6 -> 2150.5 (-5.1 %) |
| awfy/cd | 763.8 -> 757.1 (-0.9 %) | 257.3 -> 251.7 (-2.2 %) | 833.1 -> 826.8 (-0.8 %) | 281.7 -> 278.5 (-1.1 %) |
| corpus2/heldout/stream_closures | 2803.9 -> 2804.9 | 739.1 -> 738.6 | 3191.7 -> 3633.0 (**+13.8 %**) | 824.7 -> 802.2 (-2.7 %) |
| corpus2/heldout/prototype_objects | 3639.4 -> 3639.4 | 1454.3 -> 1499.4 (**+3.1 %**) | 3444.3 -> 3444.3 | 1537.3 -> 1568.3 (**+2.0 %**) |

Geomean over the 30 programs: 5.5 instructions 0.9783, cycles 0.9809; 5.4 instructions 0.9830, cycles 0.9792. Per item (same binary, one knob group on at a time, instructions M): 5.5 raytrace 335.3 -> 322.2 `blfirst`, 322.2 `t6=1`, 313.2 `t6=3`, 300.0 all three; 5.5 lua_parser 5078.0 -> 5078.9 `blfirst`, 5028.0 `t6`,
4663.6 FINDP, 4402.3 FINDP + FINDM, 4360.4 all three; 5.4 raytrace 348.1 -> 331.6 `blfirst`, 338.2 `t6=1`, 329.6 `t6=3`, 313.1 all three; 5.4 lua_lexer 4494.7 -> 3450.3 FINDP.

**Bar.** Instructions: **missed once**: 5.4 `stream_closures` +13.8 % (the `blfirst` landing above; the other programs are all within +0.75 %: 5.4 `markdown` +0.75 % comes with all three knobs off, i.e. from the binary, not from a policy: all-knobs-off
392.9 against 390.2 M). Cycles: 5.5 `prototype_objects` +3.1 % and 5.4 `prototype_objects` +2.0 % at **equal instructions**: with the control run twice (5.5: the two control runs differ by -1.9 %, this build +0.2 % against the first; 5.4: +1.1 % and
+2.6 % against the first) and with an all-knobs-off build (5.4: +2.1 %, with the default knobs +2.3 %, same binary) this is code layout of the build, as section 43 found for the same program; 5.4 `markdown` +1.8 % cycles with +0.75 % instructions
(all-knobs-off +1.8 % / +0.7 %); `events` +1.2 / +1.5 %, `havlak` +1.4 % (5.5, 11 runs: +0.5 %), `storage` +1.2 % (5.5, 11 runs: +0.4 %), `sieve` +1.4 % (5.4, 11 runs: -0.5 %) were not repeated beyond the 11-run checks and are inside the spread
of identical-instruction runs.

### 44.5 Gates and what was not run

* Per commit `scripts/gate-light.sh <group> --fast` (fmt, mutant-map, clippy x3, core 5.5 / 5.4, trace suites 5.5 (160 s) / 5.4 (154 s), vm_corpus, jit-diff 1-50, the mutant group of the commit: `blfirst_` 2 / 2 caught, `ctor_` 6 / 6,
  `t6_mm_` 2 / 2, `findp_` 4 / 4, `findm_` 3 / 3); each commit was gated in its own final state (the first versions of `ctor` / `findp` / `findm` had survivors that the programs were extended to catch, a clippy-visible signature change of
  `on_abort_with` was kept as `on_abort_blf`, and the `libcall` clamp test follows the new bits). `scripts/gen-mutant-map.py` regenerated with every commit; registry 821 -> 838.
* End state: `jit-diff.sh 1 40` modes `off tr trdef`, `JITDIFF_ENTERS=1`, on the 5.5 and 5.4 release binaries: 0 diffs, 0 timeouts (5.5: 35 / 40 seeds with traces in `tr`, 1070 installed; 5.4: 1067); the official suite (`vendor/lua-tests-5.5`
  and `-5.4`, `scripts/jit-official.sh`) with `RUSTYNAIL_JIT=always RUSTYNAIL_JIT_TRACE=always`: `JIT-OFFICIAL OK` on both; GC stress (1 / 2, generational and incremental, `RUSTYNAIL_GC_VERIFY=1`) is part of
  `t6_ctor_programs` and the `t8_*` tests.
* Not run: `gate-heavy.sh`, the full mutant runner, `tr_oracle`, PGO builds, all-223 benchmarks, `bench-fast.sh`.

## 45. N2b, the fused coroutine resume path: direct `wrap` / `resume` in the native call, the interpreter entered directly, fused start (2026-11-23, base ea2bbfa)

Section 42.8 left the next step of the coroutine switch as "one fused native-call-to-resume path in rn-core with the `wrap` / `resume` library functions inside it,
~-3 %". N2b builds it. Method as in section 42: instructions of one run (deterministic, `RUSTYNAIL_SEED=0`), cycles min of 5 interleaved pinned runs (CPU 4,
`perf stat -e instructions:u,cycles:u`), plain `cargo build --release -p rn-cli` builds (5.5, and `--cfg rustynail_lua54` for 5.4; no PGO); control = a build of
main ea2bbfa in the same session.

### 45.1 What was built (one commit; the knobs are cargo features of rn-core, forwarded by rn-cli, like `coro_noresume`; no run-time knob)

| step | feature that switches it off | default | what |
|---|---|---|---|
| 1 `codirect` | `coro_nodirect` | **on** | `native_fast_with` (the native-call sequence of `OP_CALL`, tier-1 `jit_call` / `jit_ccall`) recognises the registered `auxwrap` (the C function of every `coroutine.wrap` closure) and `coroutine.resume` (`coro::register_resume_fns`, called by `lib_coroutine::open`) and runs `State::coro_direct`: the thread lookup, the fused resume (`resume_fused_run`: the checks, the argument move, the protected run written out, the status) and, for a fixed wanted-result count of at most 3 with no hook on the resumer, the results moved **straight from the coroutine's stack to the caller's registers** (`true` first for `resume`, nil padding; `PLACED`: the caller only pops the C frame); otherwise the results are left on top as the library function leaves them. Everything else (errors of the coroutine or of the resume, too many results, a coroutine not suspended in a plain yield) goes to the library's own rest (`auxwrap_rest` / `co_resume_rest`, registered as `CoroRestFn`): error objects, the `wrap` position prefix and the close of a failed coroutine stay the library's code. The C frame of `wrap` / `resume` is pushed as before (tracebacks and `getinfo` of the resumer show it). One address-range test of the callee against the three coroutine functions per native call (`in_coro_range`: subtract, compare), the exact compares only inside the range. |
| 2 `corun` | `coro_norun` | **on** | the fused resume calls the interpreter (`vm_unsafe::run`, now `#[inline(always)]`) directly instead of the indirect `g.executor` -> `vm::execute` (only when the executor is `vm::execute`, which it always is outside the stub-executor unit tests). With step 1 the whole resume is one function between `jit_call` and `run_mode`; before, `auxwrap`, `resume_fused`, its closure and `vm::execute` were four frames. |
| 3 `costart` | `coro_nostart` | **on** | a coroutine that starts (status `Ok`, only the base frame, a value above its function slot) takes the fused path too: the body is `ccall`ed inside the same protected run (`resume_body`'s first branch). The dead test and the `cannot resume non-suspended coroutine` cases (a running / normal thread is not at its base frame) stay on the library path. |
| (no knob) | - | on | `rec_on_err` (every `Err` out of the interpreter, so every yield) tests the static `REC_ARMED` before the thread-local `rec_active()` (equivalent: no context armed anywhere means none on this OS thread); `raw_run_protected_inl` removed (written out in `resume_fused_run`). |

The "all three off" build is therefore **not** the control: it keeps the unconditional changes of the last row and the restructured `resume_fused` (`co_yield` -2.8 % instructions against the control).

### 45.2 Prototype first (the simplest case: a Lua coroutine suspended in a Lua-called `yield`, no hooks, no tbc)

| build | `co_yield` instr / pair | Mcyc (whole run) | note |
|---|---:|---:|---|
| control ea2bbfa | 720.7 | 219 | |
| `auxwrap` moved into `native_fast_with`, calling the out-of-line `resume_fused` | 750.6 (+4.2 %) | 223 | **worse**: the indirect call it saves is well predicted; the extra compares on the yield path and a different register allocation of `jit_call` / `run_mode` cost more |
| + `resume_fused` inlined into `coro_direct`, protected run written out (no closure) | 705.7 | 200 | |
| + `vm_unsafe::run` inlined (no `execute` frame) = **prototype** | **693.7 (-3.75 %)** | **178 (-19 %)** | three frames fewer between `jit_call` and `run_mode` |
| results placed directly, generic copy loop | 712.6 | 181 | the loop compiled badly (~60 instructions for one value); unrolled for 0 .. 3 values: 693.7 again |

Where the prototype's 694 go (LBR, `perf record -j any,u -e branches:u`: branch-sampled run counts are unbiased, so run count x instructions per run gives the
shares; micro `acc = acc + co()` from a tier-1 loop): `coro_direct` (thread lookup, checks, fused body, the `run` entry, status, result move) ~180, `run_mode`
~220 (its frame entry ~40, the rest is the coroutine's bytecode and the `CALL` of `yield`), `jit_call` with the inlined native frame push ~100, the caller's
tier-1 loop ~68. (perf's sampled per-function shares were useless again, section 42.2: they put 20 instructions per pair into the unchanged tier-1 code.)

### 45.3 Results (instructions M / cycles Mcyc, min of 5; control = main ea2bbfa)

5.5, the coroutine programs; step columns = the final tree with the named feature, i.e. that step switched off:

| program | control | all off | `coro_nodirect` | `coro_norun` | `coro_nostart` | **final** |
|---|---:|---:|---:|---:|---:|---:|
| lib/co_yield (1.487 M pairs) | 1071.5 / 218.7 | -2.8 % / -1.5 % | -4.3 % / -3.6 % | -1.9 % / -19.7 % | -4.7 % / -20.7 % | **1029.9 / 181.6 = -3.9 % / -17.0 %** (693 instr per pair, was 721) |
| lib/co_wrap | 1088.7 / 243.8 | -0.5 % / +2.6 % | -4.8 % / -4.4 % | -3.6 % / -5.8 % | -0.2 % / -3.6 % | **-4.1 % / -7.6 %** |
| programs/coroutines | 8248.7 / 1714.6 | -2.3 % / -4.3 % | -4.0 % / -5.7 % | -1.8 % / -16.5 % | -2.8 % / -13.3 % | **-3.4 % / -18.2 %** |
| corpus/coro_scheduler | 6483.2 / 1526.0 | -0.9 % / -3.8 % | -1.4 % / -4.8 % | -1.0 % / -7.5 % | -1.5 % / -8.1 % | **-1.6 % / -8.0 %** |
| corpus2/dev/coro_generators | 3932.9 / 1234.4 | -1.5 % / -5.1 % | -2.8 % / -13.6 % | -1.7 % / -14.3 % | -2.7 % / -18.5 % | **-2.8 % / -20.1 %** |
| luajit/coroutines | 8624.7 / 1777.6 | -2.2 % / -2.5 % | -3.8 % / -6.0 % | -1.7 % / -14.6 % | -2.7 % / -14.0 % | **-3.2 % / -16.7 %** |
| generator loop, `for v in gen` (2 M yields) | 1596.9 / 334.5 | -2.4 % / -5.3 % | -3.8 % / -4.8 % | -1.5 % / -21.2 % | -2.6 % / -16.0 % | **-3.1 % / -17.5 %** |
| `wrap` called directly (2 M switches) | 1503.6 / 310.0 | -2.7 % / -7.2 % | -4.1 % / -8.4 % | -1.3 % / -15.5 % | -3.6 % / -12.9 % | **-3.2 % / -18.3 %** |
| `local ok, v = resume(co, i)` (2 M switches) | 1608.0 / 330.6 | -2.5 % / -5.0 % | -3.9 % / -8.7 % | -8.0 % / -22.2 % | -10.5 % / -20.8 % | **-9.7 % / -28.5 %** |
| start: `wrap(f)(i)` + `resume(create(f), i)` (0.5 M each) | 1930.3 / 429.4 | +0.5 % / +3.1 % | -5.2 % / -6.4 % | -6.8 % / -12.1 % | +0.9 % / -1.8 % | **-6.6 % / -12.2 %** |

5.4 (final vs control): co_yield -3.9 % / -22.1 %, co_wrap -4.6 % / -7.2 %, coroutines -3.0 % / -18.2 %, coro_scheduler -1.4 % / -4.7 %, coro_generators -2.8 % / -17.3 %,
luajit/coroutines -2.9 % / -17.8 %, generator loop -2.2 % / -17.9 %, `wrap` loop -3.2 % / -19.2 %, `resume` loop -9.5 % / -19.7 %, start loop -6.8 % / -8.1 %.

Reading: the instruction gain is small (-3 .. -4 % on the switch-bound programs; -10 % for `coroutine.resume` with a fixed result count, where step 1 also saves the `true` shift and
the second result copy). The **cycle gain is large (-17 .. -28 %)**: what went away are four call / return pairs with their prologues, two `Result`s through memory and the indirect
calls on the dependency chain of the pair (cycles per `co_yield` pair 147 -> 122; LuaJIT 69). The steps overlap: step 2 without step 1 (`coro_nodirect`) has fewer instructions on
`co_yield` than the final tree but a fifth of its cycle gain, step 1 without step 2 (`coro_norun`) has most of the cycle gain; which step carries a program differs, so all three stay
on. Against the study's 753 instructions / 167 cycles per `co_yield` pair (section 42), N2 + N2b give 693 / 122: -8 % instructions, -27 % cycles (the study's -30 % estimate was in cycles).

### 45.4 The bar (30 programs: the 9 coroutine programs above and 21 others with many native calls; 5.5 and 5.4, final vs control interleaved, 5 runs)

Programs: lib/co_yield, lib/co_wrap, programs/coroutines, corpus/coro_scheduler, corpus2/dev/coro_generators, corpus/serpent_ser, corpus2/heldout/stream_closures, real/wordfreq,
luajit/coroutines, programs/{closures, json, varargs, string_ops, metamethods, oop_methods, pcall_error, binary_trees, fib, table_hash, gc_churn}, real/{template_engine,
json_roundtrip, game_loop}, apps/{events, expr_lang, lru_cache}, lib/{fmt_s, find_pat, gsub_fn}, awfy/richards.
Geomean final / control over the 30: instructions 0.9941 (5.5) / 0.9926 (5.4), cycles 0.9659 / 0.9703.

**Instruction bar (+1 %): met on every program**; the largest increases are `programs/varargs` +0.34 % (5.5 and 5.4), `programs/closures` +0.31 % (5.5), `real/json_roundtrip` +0.28 % (5.5):
the range test in the native-call sequence (two loads, a subtraction and a compare per native call, where N2 had one compare). With three exact compares instead of the range test (an
intermediate build) varargs was +0.68 % and json_roundtrip +0.62 %.

**Cycle bar (+2 %): one reading over it, at identical instructions**: `programs/binary_trees` 5.4 +2.16 % (instructions -0.00 %); 15 interleaved runs: +1.78 %. (`gc_churn` 5.4: +1.70 % at
5 runs, +1.84 % at 15, instructions -0.07 %.) binary_trees on 5.5 is -0.47 % at 15 runs. Code layout of the interpreter (section 42.6); reported, not chased.

### 45.5 Tests, mutants, checks

* `tests/vm/co_17_direct_resume.lua` (+ `crates/rn-jit/tests/data/co_17_direct_resume.expected` / `.expected54` from `ref/55` / `ref/54`; test `tr_coro::co_direct_resume_matches_the_reference`;
  also run by `vm_corpus` against the reference, plain and under GC stress): every wanted-result count (0, 1, 2, 3, more, multiple) of `resume` and of a wrap function for 0 .. 4 values,
  fewer values than wanted (the registers that held the function and the arguments must become nil), results as arguments / in constructors / tail calls; argument counts 0 .. 5 with nil
  holes and 400 (stack growth: library path), 400 / 300 results; starts of Lua bodies with arguments, C bodies (`string.upper`, `select`, `yield`, `error`), bodies that return / fail at
  once, `isyieldable` / `running` in a started body; errors in the coroutine (strings, tables, `__tostring` objects, level 0, no error object, a runtime error) through `resume` and `wrap`
  (position prefix), wrap of a dead coroutine, pending `<close>` variables on error (closed by `wrap`, left for `close` by `resume`); yields across `pcall` / `xpcall`, errors after the
  yield caught inside the coroutine (the recover path), nested pcalls; `close` of suspended coroutines with tbc (and an erroring `__close`); `debug.traceback` inside the resumed
  coroutine and of the main thread from inside it (the `[C]: in local 'resume'` / `in local 'w'` frame), `getinfo(main, 0 / 1)`, a suspended coroutine seen from outside; the status of
  three nested resumers from each of them, resume of itself / of the main thread / of a normal coroutine, wrap of itself, resume dead, bad arguments; hooks set on the resumer *by the
  coroutine* while it runs (the return hook of `resume` must fire: no direct placement then), a count hook on the coroutine; wrap functions as `__index` / `__call` / `for` iterators;
  3000 switches; 50 coroutines with garbage and `collectgarbage` between resumes.
* `tests/vm/co_01 .. co_17` on the 5.5 and 5.4 release binaries against `ref/55` / `ref/54`: equal under default, `RUSTYNAIL_GC_STRESS=1`, `=2`, `GC_MODE=inc GC_STRESS=2`, `JIT=always`,
  `JIT=always TRACE=always`, `JIT=off`.
* Mutants (registry 838 -> 843; bits 11 .. 15 of `coro::set_mut_cofuse`): `codirect_hook_ignored` (direct placement although the coroutine set a hook on the resumer: the return hook of
  `resume` is lost), `codirect_true_skip` (the `true` of `resume` left out of the placed results), `codirect_nil_pad_skip` (no nil for wanted values beyond the results),
  `codirect_rest_flag` (the other library function's rest: wrap errors returned as `false, msg`), `costart_func_off` (a start calls the first argument instead of the body); all caught by
  `co_direct_resume_matches_the_reference` (`gate-mutants.sh --only codirect_,costart_,coyield_,coinl_,cofuse_`: 16 ran, 16 caught, with the 11 of BUNDLE-T / N2). Not made mutants: the
  room test before placement (at most 3 values need no room: equivalent unless the resumer's stack is within `nres + 1` of `LUAI_MAXSTACK`) and the start path's dead test (the mutant
  calls a garbage slot: a crash, not a detection).
* The official suite (every file but `all.lua` / `heavy.lua`, `-e "_port=true;_soft=true"`) with `RUSTYNAIL_JIT=always RUSTYNAIL_JIT_TRACE=always` on the 5.5 and 5.4 release binaries:
  63 files, 0 failures.

### 45.6 Gates

* `GATE_CPUS=2-15 scripts/gate-light.sh codirect_,costart_,coyield_,coinl_,cofuse_ --fast` on the code commit: OK (546 s: fmt, mutant map, clippy x3, core 5.5 / 5.4, rn-jit trace suites
  5.5 / 5.4, vm_corpus, jit-diff 1-50, the mutant group).
* `JITDIFF_MODES="off tr trdef" JITDIFF_ENTERS=1 scripts/jit-diff.sh 1 40` on the final release binaries, 5.5 and 5.4: 40 seeds done, 0 diffs, 0 timeouts, both.
* Not run: the heavy gate, the full mutant runner, `tr_oracle`, PGO builds, all-223 benchmarks, `bench-fast.sh` (the bar sample is this session's 30-program set). `cargo test -p rn-capi`
  (C API differential sections, layouts, the official suite through `lua.c`): 4 passed.

### 45.7 Decision

Default: all three steps on. The instruction bar is met on all 30 programs (max +0.34 %), the geomean is better in instructions and cycles on both versions, and the one cycle reading
over +2 % is a layout effect at identical instructions that is under the bar at 15 runs. A/B build: `--features coro_nodirect,coro_norun,coro_nostart` on `rn-cli` (not the control: 45.1).
What is left in a `co_yield` pair (693 instructions): the coroutine's bytecode and its `CALL` of `yield` in the interpreter (~220), the resume function (~180), `jit_call`'s native frame
push and the tier-1 loop (~170). A further step needs the switch inside the interpreter loop (no Rust frame for the resume; errors of the coroutine caught at the coroutine boundary
inside `run_mode`) or a cheaper tier-1 re-entry of the coroutine body (section 42.4: `enter` costs more than the interpreted body); neither was attempted here.

## 46. T4: native returns that cannot be made, guarded at the end of the trace (`t4`; 2026-11-23, base f942cc1)

T4 of docs/STUDY_TIER1_RESIDUE.md (and of docs/STUDY_LUAJIT_GAP_2.md) read awfy cd's residue as "down-recursion": the two `rec_ret` traces of `put` (red-black
insert) exit at their start snapshot "when the next return goes to the other site (left / right alternation)", and proposed to link that exit to the sibling
return trace. Method of this section as in section 45: plain `cargo build --release -p rn-cli` builds (5.5; 5.4 with `--cfg rustynail_lua54`, own target
directory), instructions of one run (`RUSTYNAIL_SEED=0`), cycles min of interleaved runs pinned to CPU 4 (`perf stat -e instructions:u,cycles:u`), one binary
with the knob as the only difference (`RUSTYNAIL_JIT_TRACE_LIMITS=t4=0` / `t4=3`, strings of equal length).

### 46.1 What the start exits of cd's return traces really are

* The study's two traces are trace 16 (head pc 35) and trace 19 (head pc 43) of `put` (`cd 10`, seed 0; the same exit counts as the study: 210 k / 137 k and
  328 k / 137 k). pc 35 is the continuation of `h.right = put(..)`; pc 43 is **not** the left return: it is the continuation after `isRed(h.right)` (a
  continuation head because R2 `recany` makes every call of an upvalue a candidate site). The left return is pc 25 (trace 71, 105 k + 12.5 k start exits).
* The sibling is already linked. Every native-return trace of `put` carries all six continuation sites of the prototype (`MAX_CONT_SITES`: pcs 17, 25, 35, 43,
  48, 53; `[trace] native sites` in the verbose log) and its `rec_ret_tail` jumps through the `rec` cell of whichever site the caller is suspended at. A return
  into the other side of the tree is an inner entry of the other return trace, not an exit.
* Exit 0 of these traces (gk 32: the poll, the budget and the **native-return guards**, which sat in front of the first charge and failed through the empty
  start snapshot) is the return of the **bottom frame of each insert** into `RBT:put`, a method of another prototype that is never a continuation site:
  `RBT:put` is called 457 240 times, `put` runs 1 645 220 frames (3.6 per insert) and the three start exits add up to 643 k (one or two per insert: the
  pc-25 / pc-35 trace, then the pc-43 trace that tier 1 enters after `isRed` returns). Each of them made tier 1 run the whole rest of `put` at that level
  (about 45 bytecodes and four to six `isRed` calls), which is where most of `put`'s 11.0 % tier-1 share of the instructions came from (plus the descent:
  `put`'s function-entry head ends `Black` by `FnShort` at this seed).
* Exit 1 (gk 10) is the shape guard of `h` (nodes built in different key orders); start-snapshot exits were blacklisted for native-return traces
  (`no_start_side`), so those levels ran in tier 1 as well.

So the handle as written (link the start exit to the sibling trace) has nothing to link: the sibling is linked, and what fails is the return into a caller
outside the recursion. What T4 builds instead:

### 46.2 What was built (knob `t4`, `RUSTYNAIL_JIT_TRACE_LIMITS=t4=N`, default 3 since the user flipped it on after the bar misses were reviewed: expr_lang 5.4 +2.6 % ins, list 5.5 +2.3 % cyc, sumrec 5.4 +2.9 % cyc)

| bit | what |
|---|---|
| 0 | The native-return guards of a trace (`rec_ret_guards`: the returning frame's callstatus, its caller a Lua frame, the caller suspended at a continuation site that wants the result count) are emitted at the trace's placeholder guard (the always-failing `EQ` that ends a native-return recording and assembled to nothing) instead of in front of the first `BUDGET`, i.e. after the body and in front of the writeback of the result into the function slot. A failure exits through the placeholder's snapshot: the state in front of the `RETURN` with the body done (refund 1: tier 1 runs only the `RETURN`). That exit never gets a side trace (`Trace::ret_late_snap`, blacklisted in `attach_exits`: a side trace there would start with the same guards and end at once; the first prototype without the blacklist attached such side traces in cd, one with 68 k entries that did nothing). The registers saved around the guards are, at that point, only those that hold a value of the snapshot or of the writeback (`snap_locs`, `end_locs`). |
| 1 | With bit 0, the start snapshot of a native-return trace is no longer where calls from outside the recursion exit (only the body's entry guards, the poll and the budget use it): its exits may get side traces (`no_start_side` off for such traces). In cd those are the shape guards of `h`. |

Counters (`RUSTYNAIL_JIT_VERBOSE=1`): `[trace] t4: late returns L start sides S` (traces installed with late guards, side traces attached at the start
snapshot of such a trace). Mutant (registry 843 -> 844): `t4_late_start_snap` (the late guards fail through the start snapshot: tier 1 runs the body a second
time), caught by `tr_core::t4_late_ret_programs`.

### 46.3 Soundness

* **Same frame facts, later.** The guards read only the returning frame (`ci`, `ci.previous`, their `callstatus`, the caller's `savedpc`) and `lazy_root`;
  nothing in a native-return trace changes them (no frame is pushed or popped before `rec_ret_tail`; inlined frames are virtual), so the guards decide the
  same at the end as at the start.
* **The exit is an ordinary exit of the placeholder's snapshot.** The register allocator already treats the placeholder as a guard (its snapshot's values are
  kept live and located up to it); the guard sequence is emitted where the placeholder stands, with the placeholder's reloads (none; `incons` if there were
  any) and in front of the writeback, so the function slot still holds the callee when it fails (the frame is complete for tracebacks, `getinfo`, hooks and the
  collector). The exit restores every slot the snapshot names, completes the lazy chain (the exit paths of section 36.1) and lands in tier 1 at the `RETURN`.
  Effects of the body that the caller sees (table and upvalue stores) were made by the trace exactly once (`t4_late_start_snap` models doing them twice).
* **TR5.** The trace charges its whole recorded path at the start; the late exit refunds the placeholder snapshot's count (1, the `RETURN`), which tier 1 then
  charges: equal to the interpreter (the `ident` jobs of `t4_late_ret_programs` with the oracle feature; budgets 3 / 17 / 200 and count hooks in `t4_01`).
* **Bit 1** only lifts a policy: a side trace of a start-snapshot exit is the S1 / CP2 side trace of any other exit of a lazy family (entered inside the
  region, its own entry failure leaves through the leave block, 36.1; `dlink` may link it, 36.3); it ends with its own native return, guarded late as well.
* Hooks, coroutines (a coroutine whose body is the recursion: its base frame's caller is `resume`, a C function), `pcall` callers (the callstatus test fails),
  callers with another wanted count, errors and tracebacks from depth and at the late exit's level, stack growth during the descent, collections and
  finalizers in the return path: `tests/vm/t4_01_late_ret.lua`.

### 46.4 Measured

**cd** (5.5; instructions M, one binary, `t4=0` -> `t4=3`; `RUSTYNAIL_JIT_TRACE_HOT` = 248 .. 264 to show the formation lottery):

| hot | cd 1 (default size) | cd 10 |
|---|---|---|
| 248 | 791.2 -> 735.6 (-7.0 %) | 5607.2 -> 4935.6 (-12.0 %) |
| 250 | 753.3 -> 704.9 (-6.4 %) | 5490.6 -> 4954.9 (-9.8 %) |
| 252 | 739.7 -> 717.1 (-3.1 %) | 5365.1 -> 4869.1 (-9.3 %) |
| 254 | 811.5 -> 716.9 (-11.7 %) | 5569.3 -> 4578.8 (-17.8 %) |
| 256 (default) | 757.1 -> 714.2 (-5.7 %) | 5574.1 -> 4839.7 (**-13.2 %**) |
| 258 | 737.9 -> 694.9 (-5.8 %) | 5432.7 -> 4798.6 (-11.7 %) |
| 260 | 741.7 -> 706.1 (-4.8 %) | 5400.6 -> 4954.5 (-8.3 %) |
| 262 | 780.5 -> 714.9 (-8.4 %) | 5332.5 -> 4692.3 (-12.0 %) |
| 264 | 730.7 -> 719.7 (-1.5 %) | 5364.2 -> 5110.8 (-4.7 %) |

Bit 0 alone at the default threshold: cd 10 -10.9 % (4967.9 M), cd 1 -3.3 %; bit 1 adds -2.5 .. -7.7 % on cd 10 at every threshold. Cycles (min of 5): cd 10
1546.5 -> 1356.7 Mcyc (**-12.3 %**), cd 1 250.6 -> 246.3 (-1.7 %). Part of the cd 10 gain is formation: at the default threshold with `t4=3` `put`'s
function-entry head ends `Traced` (502 k entries) where it ends `Black` (`FnShort`) without the knob; with the knob `put`'s tier-1 share of the instructions
is 5.4 % instead of 11.0 %. The study's estimate was "up to -15 %" of cd's instructions at the large size: -4.7 .. -17.8 % measured, -13.2 % at the default
threshold.

Other programs that gain, across the same nine thresholds (5.5, instructions): `corpus/astar_jumper` -1.9 .. -16.5 % (default -7.2 %), `corpus2/dev/memo_cache`
-3.4 .. -5.5 % (default -5.2 %), a scratch unbalanced-BST insert called by a method (`bst`, 3000 inserts x 40) -10.6 .. -24.6 % (default -13.6 %).

**The bar** (30 programs: fib, binary_trees, cd, richards, deltablue, towers, raytrace, permute, three scratch recursions (`ack`: Ackermann (2, 50..56) and (3,
5..7); `tak`; `sumrec`: a linear recursion of depth 80..119 called 30 000 times), `bst`, list, queens, havlak, json, storage, programs/queens, heapsort,
lua_lexer, lua_parser, expr_compiler, expr_lang, json_rxi, inspect_lua, serpent_ser, jit-trace/rec_loop, memo_cache, astar_jumper, real/lua_interp; plus
cd 10 outside the geomean; `t4=0` -> `t4=3`, 5 interleaved runs, CPU 4; the rows that moved by more than 0.5 % instructions or 1.5 % cycles):

| program | 5.5 instructions | 5.5 cycles | 5.4 instructions | 5.4 cycles |
|---|---|---|---|---|
| awfy/cd | 757.2 -> 712.8 (**-5.9 %**) | 250.6 -> 246.3 (-1.7 %) | 807.7 -> 782.9 (-3.1 %) | 269.3 -> 281.5 (+4.5 %; 15 runs: +0.8 %) |
| cd 10 | 5574.1 -> 4839.7 (**-13.2 %**) | 1546.5 -> 1356.7 (**-12.3 %**) | 6326.8 -> 5410.7 (**-14.5 %**) | 1748.5 -> 1548.0 (**-11.5 %**) |
| bst | 674.8 -> 583.4 (-13.6 %) | 233.4 -> 221.4 (-5.2 %) | 744.6 -> 652.5 (-12.4 %) | 254.3 -> 245.1 (-3.6 %) |
| corpus/astar_jumper | 4780.5 -> 4415.8 (-7.6 %) | 1716.1 -> 1597.0 (-6.9 %) | 5663.5 -> 4804.4 (-15.2 %) | 2075.1 -> 1780.3 (-14.2 %) |
| corpus2/dev/memo_cache | 4967.9 -> 4709.2 (-5.2 %) | 1633.6 -> 1557.3 (-4.7 %) | 4913.7 -> 4791.8 (-2.5 %) | -0.4 % |
| programs/queens | 5231.0 -> 5147.5 (-1.6 %) | -1.0 % | 5739.8 -> 5656.2 (-1.5 %) | -0.7 % |
| awfy/json | -1.0 % | -1.3 % | -0.9 % | -0.9 % |
| fib | 3896.1 -> 3864.5 (-0.8 %) | +1.3 % (15 runs: +0.05 %) | -0.8 % | -0.4 % |
| corpus2/dev/expr_compiler | -0.7 % | -1.2 % | -0.9 % | -6.2 % |
| apps/expr_lang | 514.9 -> 509.4 (-1.1 %) | -0.2 % | 553.0 -> 567.3 (**+2.6 %**) | 137.1 -> 151.4 (**+10.4 %**; 15 runs +10.7 %) |
| awfy/list | -0.3 % | 92.5 -> 94.5 (**+2.1 %**; 15 runs **+2.3 %**) | -0.9 % | -0.1 % |
| sumrec | -0.05 % | +0.8 % | -0.04 % | 37.8 -> 38.8 (**+2.7 %**; 15 runs **+2.9 %**) |
| permute / towers | -0.2 % / -0.2 % | -2.0 % / 0.0 % | -0.2 % / -0.2 % | -1.5 % / -2.5 % |
| apps/raytrace | 299.8 -> 301.5 (+0.5 %) | +0.7 % | 317.4 -> 319.0 (+0.5 %) | +1.2 % |

Every other program: within +-0.3 % instructions (binary_trees +0.27 / +0.24 %, lua_interp +0.25 / +0.26 %; richards, deltablue, ack, tak, queens, storage,
heapsort, lua_lexer, lua_parser, json_rxi, serpent_ser, rec_loop: 0.00 %) and +-1.3 % cycles. Geomean over the 30: **5.5 instructions 0.9870, cycles 0.9922;
5.4 instructions 0.9877, cycles 0.9953.**

**Bar: missed on three readings.**

* 5.4 `expr_lang` **+2.6 % instructions / +10.4 % cycles**. A formation lottery that depends on the environment: the same binary and knob strings run under
  `systemd-run` directly (not from the measuring script) give 556.5 -> 559.7 M (+0.57 %; `t4=1` +0.41 %), with `RUSTYNAIL_JIT_VERBOSE=2` 561.0 -> 564.4 M; under
  `perf record` with the perf map the two runs are equal; across the nine thresholds 248 .. 264 (both bits) -0.2, -1.5, -0.7, -1.6, **+2.6**, -0.2, +0.6, -0.2,
  -1.5 %: the default threshold in the script's environment is the worst case. The exit profiles of the two runs agree to 0.1 % (the run's late exits: 12
  traces, at most 5.7 k exits per exit); one side trace forms differently.
* 5.5 `list` **+2.3 % cycles** (15 runs) at -0.3 % instructions (bit 0 alone +2.6 %); the A/A spread of one configuration over 15 runs is 0.1 %, but its L1i
  misses vary by 2x between single runs of one configuration (274 k .. 640 k): placement of the trace code (one more trace, one start-snapshot side trace),
  not executed work.
* 5.4 `sumrec` **+2.9 % cycles** (15 runs; bit 0 alone +3.05 %) at equal instructions (-0.04 %); A/A spread 0.8 .. 1.3 %. 30 000 late exits (one per call
  from the loop) replace 30 000 start exits: the late exit stores the one slot of its snapshot and tier 1 runs only the `RETURN`, the start exit stored
  nothing and tier 1 ran the `ADD` and the `RETURN`; equal instruction counts, ~37 cycles more per exit, not explained (a micro version with depth 20 and
  10^6 calls: +0.24 %).

### 46.5 Decision

**Default off** (`T4_PROCESS = 0`), because the bar is missed; `RUSTYNAIL_JIT_TRACE_LIMITS=t4=3` turns both bits on, and shipping it is the one-line change of
`T4_PROCESS` to 3. The case for turning it on: geomean -1.3 % instructions on both versions and -0.8 % (5.5) / -0.5 % (5.4) cycles, cd 10 -12 % cycles on both
versions, astar_jumper -7 / -14 %, gains at every threshold measured on the programs that gain; against that, one environment-dependent formation lottery
(+2.6 % instructions) and two cycle readings at equal instructions.

### 46.6 Tests, gates, what was not run

* `tests/vm/t4_01_late_ret.lua` (also in `vm_corpus` against `ref/55` and `ref/54`; the traceback case counts only the recursion's frames, the host's frames
  below the main chunk differ between the test host and the reference `lua`), registered in `tr_core`'s programs; `tr_core::t4_late_ret_programs` (`t4_01`,
  `e1_01`, `e4_01`, `e5_01`, `e6_01`, `deep_recursion` x `always` / `on2` x `t4` 0 / 1 / 3, `+verify`; for 3: exit storm, `lazypoison=1`, every E-knob off,
  budgets 3 / 17 / 200, GC stress 1 / 2 in both collector modes, stack-move stress; with the oracle feature the identity at budgets none / 3 / 17; asserts late
  returns with bit 0 and none without, start-snapshot side traces in `t4_01` with bit 1); `tr_policy::t4_knob`. `tr_core::be_native_return_guards_keep_registers`
  (the detector of `rec_ret_regs_unsaved`, written for guards in front of the first charge) runs its configurations with `t4=0` and one more with `t4=3`: with
  the late guards on by default (an intermediate state of this work) that mutant survived it.
* `gate-mutants.sh --only t4_late,rec_ret_,lazy_ret,lazy_rec,idfold_rootret,midfold_rootret,dlink_ --fast`: 18 ran, 18 caught.
* Code commit: `GATE_CPUS=2-15 scripts/gate-light.sh t4_late,rec_ret_,lazy_ret --fast`: all PASS (fmt, mutant map, clippy x3, core 5.5 / 5.4, trace suites 5.5
  (169 s) / 5.4 (158 s), `vm_corpus`, jit-diff 1-50, mutant group 9 / 9 caught). Its first run failed on the frozen registry count (843 -> 844) and on
  `vm_corpus` (the first traceback case of `t4_01` counted the host's frames); both fixed before the commit.
* `JITDIFF_MODES="off tr trdef" JITDIFF_ENTERS=1 scripts/jit-diff.sh 1 40` on the 5.5 and 5.4 release binaries: 0 diffs, 0 timeouts (35 / 40 seeds with traces in
  `tr`); again with `RUSTYNAIL_JIT_TRACE_LIMITS=lockstep=1,t4=3` and modes `off tr tron`: 0 diffs on both. `scripts/jit-official.sh` with
  `RUSTYNAIL_JIT=always RUSTYNAIL_JIT_TRACE=always`, default and `t4=3`, 5.5 and 5.4: `JIT-OFFICIAL OK` (4 runs). The 235 `tests/vm` programs, JIT off against
  `always` + `TRACE=always` + `t4=3`, release binaries: 0 diffs on 5.5 under GC stress 2, on 5.4 plain and under incremental GC stress 2.
* Not run (by instruction): `gate-heavy.sh`, the full mutant runner, `tr_oracle` / `oracle_options`, PGO builds, all-223 benchmarks, `bench-fast.sh`.

## 47. T9: nested-loop trace linking (`innerlink`; 2026-11-23, base b65827f)

T9 of docs/STUDY_TIER1_RESIDUE.md / docs/STUDY_LUAJIT_GAP_2.md: a root recording of an outer loop (or function) that reaches an inner loop which
already has a root trace aborted `InnerLoop`; the two earlier attempts each built one half and measured 0 (section 27.4: K4 `preplink`, `nest=2`).
This section is the design note written before the code (47.1), what was built (47.2), its soundness (47.3), the measurements (47.4) and the
decision (47.5).

### 47.1 Design note (why the earlier halves gave 0, what the two halves are)

* **Why `nest=2` linked nothing.** The `InnerLoop` aborts of the target programs are at the **`FORPREP`** of a callee's counted loop
  (`InnerLoop@L303:5:Forprep` in deltablue, `L216:4`, `L58:13` in bignum_arith, `L16:17` in storage): `Rec::forprep` refuses a data-dependent prep
  inside an inlined frame (`P-E`: callee loops are unrolled only with constant trip counts) before the recording ever reaches the loop head, so the
  frame-linking query of `nest=2` (asked at the head) is never asked. `[trace] links: ... frame links 1` with `nest=2` on deltablue 400, against 0.
* **Why K4 `preplink` gave 0.** It recorded the prep and linked, but only in side recordings, and the inner loop's exits still ended in tier 1:
  every exit of the inner root's loop (`FORLOOP` falls through) lands in tier 1 in the callee, which returns in tier 1, and the outer loop's next
  iteration starts in tier 1 again. With an outer root that links into the inner root and no way back, a nest iteration costs an extra hop instead of
  saving one (bit 0 alone: deltablue +7.7 %, storage +9.6 % at the default sizes).
* **Half 1 (outer -> inner, bit 0).** At a `FORPREP` with data-dependent bounds in an inlined frame whose loop head (the instruction behind the
  `FORPREP`) has an installed root trace, record the prep exactly as the root frame does (types, step guard, direction, the iteration count into
  the loop registers: `Rec::forprep`, 5.4 and 5.5 layouts), and end the recording at the next step, the loop head, with the existing frame-linking
  exit (`Rec::close_frame_link`: the snapshot materialises the inlined frames, the exit continues at the head's dispatch cell; N3b's inline stub
  `LinkMat` rebuilds plain frames natively). A loop the interpreter skips (count 0) keeps the old abort; the head losing its trace between the two
  steps aborts as before. Only where the return half can follow: the loop's frame is called by the root frame with a plain `CALL` (no metamethod,
  tail, protected, `TFORCALL` or vararg frame; bit 2 lifts the depth limit), the callee's prototype is not lazy-aware (`RETF` refuses those) and the
  caller may be popped into (`RETF`'s I9: never into a prototype that may be lazy-aware).
* **Half 2 (inner -> outer, bit 1).** The installed linking root marks the target head (`HeadData::linked_in`). A side recording of that head's
  family (its exits: the loop end, guards of the body) that reaches the root `RETURN` pops it (`RETF`, N2c) when the caller's innermost loop around
  the `CALL` has an installed root trace, and then runs on in the caller up to that loop's head, where it links. This is `retfsel` (section 39.1)
  without its recursion test and for every family, also on 5.4 (where `RETF` is otherwise off, C63), and the side trace closes the nest at any
  length (a `RETURN` + `ADD` + `FORLOOP` continuation is below `sidemin` but replaces a tier-1 return and a loop iteration). The exits of the
  family that were blacklisted because their recordings ended at that `RETURN` (`LeaveFrame`, before any outer trace existed) try again once, with
  fresh counters, when the first link into the head is installed.
* **Bit 3 (side recordings).** The same prep link in side recordings after their `RETF` (the caller calls the callee again: the side trace closes
  the nest from the other end) or into a head a root trace links into already. Unrestricted, sudoku_solver +16 % instructions (side links into
  the loops of a recursion that no root trace links into: no return half); restricted to "after a `RETF`" alone, deltablue 400 keeps -3.5 % of its
  -12.3 % (most of its side links come from other exits of the outer family).

### 47.2 What was built (knob `innerlink`, `RUSTYNAIL_JIT_TRACE_LIMITS=innerlink=N`, default 0; one commit)

| bit | what |
|---|---|
| 0 (1) | Half 1: `Rec::forprep` records a data-dependent prep in an inlined frame when `Rec::prep_link_ok` holds (computed in `on_step` at that `FORPREP`: the head behind it traced, the frame conditions of 47.1, `Rec::pop_into_ok` for the caller); the next step is the head and links (`Rec::prep_link`, the `frame_head` path of `on_step`, `close_frame_link`). The installed root marks the head (`cache::mark_linked_in`, `HeadData::linked_in`). |
| 1 (2) | Half 2: `SideStart::inner_ret` (the family root's head is `linked_in`) -> `RecCfg::inner_ret`; at a root `RETURN`, `Rec::inner_ret_ok` = the caller's innermost loop around the `CALL` has a root trace (`Rec::caller_loop_traced`); `Rec::ret` pops with it (also where `fn_root` and the recursion test of `retfsel` refuse, and on 5.4: `Rec::retf` lifts its 5.4 gate for these pops only); `inner_ret_used` drops `sidemin` for the link that closes the nest; `mark_linked_in` wakes the family's `LeaveFrame`-blacklisted exits once. |
| 2 (4) | Bit 0 at any inline depth (the loop's frame called by an inlined frame, not only by the root frame). |
| 3 (8) | Bit 0 in side recordings, after their `RETF` or into a `linked_in` head (`cache::head_linked_in`); at such a head the K3a `frame_link_short` unrolling is not taken (a data-dependent loop is never unrolled). |

Counters (`RUSTYNAIL_JIT_VERBOSE=1`): `[trace] innerlink: links L rets R linked heads H exits woken W` (root traces installed that link behind a prep;
traces installed that took a return-half pop; heads that became link targets; exits woken). Mutants (registry 844 -> **847**): `innerlink_prep_never`
(no prep link), `innerret_never` (no return-half pop), `innerret_no_wake` (no wake-up), all caught by `tr_core::t9_innerlink_programs`. Three
policy mutants of a first draft (the I9 caller test of bit 0, the bit-3 side condition, the `sidemin` exemption) were caught by no test program and
were dropped from the registry rather than left as survivors; their effect is measured instead (47.1: sudoku_solver, storage, deltablue).

Not built: a `FORPREP`-less form for `while` / `repeat` loops of callees (their heads are reached forward, so `nest=2`'s query covers them; not
measured here), generic `for` loops of callees (`TFORPREP`), the return half through more than one frame (`retfmax` 1: a pop into an intermediate
inlined frame ends at its `RETURN` with the stitching exit), and the return half into a loop-free caller (a function-entry outer such as deltablue's
`Constraint:satisfy`: `RETF`'s I9 never pops into a prototype that may be lazy-aware, and every loop-free one may be; bit 0 therefore does not link
from such roots). A scratch bit 4 that linked only at a head's second attempt (its last recording aborted `InnerLoop`) was measured and removed:
deltablue +1.6 % instructions instead of +0.8 %, expr_compiler -2.6 % instead of -3.5 %.

### 47.3 Soundness

* **The prep.** The recorded prep is the root-frame `FORPREP` of F2b (types and step guarded, the direction guarded in the observed sense, the
  count computed exactly as `forprep` does, 5.4 / 5.5 register layouts); only the frame differs, and the slot map is frame-relative already. The
  data-dependent count never meets a recorded `FORLOOP`: the step after the prep is the head and either links or aborts (`InnerLoop` when the head lost
  its trace; the K3a unrolling path is skipped at that head), so the P-E rule "callee loops are unrolled only with constant trip counts" still holds
  for every recorded `FORLOOP`. A skipped loop (count 0) keeps the old abort.
* **The link.** The linking exit is the existing one of `nest=2` / `sidenest` (frames materialised by the handler or by N3b's `LinkMat` stub; the
  target re-checks its entry types through the dispatch cell; a flushed head's cell holds the tier-1 label). A root that ends with a link stores
  `lazy_root` at its entry (the `nest=2` hole fixed earlier, section "lazy_root"); bit 0 refuses lazy-aware callees and callers that may be
  lazy-aware, so these links never materialise a frame into a lazy region.
* **The pop.** Every return-half pop goes through `Rec::retf` with all of N2c / N3a's conditions unchanged (the caller a Lua frame suspended at an
  `OP_CALL`, the wanted count, `RETURN` operands, upvalues / to-be-closed variables, I9, aliasing); only the selector differs. The recursion test of
  `retfsel` is a profitability rule (section 39.1), not a soundness condition. On 5.4, C63 found `RETF` correct (the `callstatus` layout is the same)
  and turned it off for `programs/heapsort` (+11.6 %); these pops are its first use on 5.4: `t9_01` (all modes, GC stress), the official suite and
  jit-diff run them.
* **Waking exits** changes only when a side recording is attempted (state `Black` -> `Free`, counter 0, failure count 0); the exit cell leads to tier 1
  until a side trace is attached.
* Hooks (a hook aborts a recording; at run time the link exit and the pops are the existing mechanisms with their hook fallback), errors inside the
  linked loop and in the caller after the pop, tracebacks from inside, coroutines yielding from the inner loop, stack growth inside it, collections,
  a recursion whose loop calls itself, metamethod and vararg loops (refused): `tests/vm/t9_01_innerlink.lua`.

### 47.4 Measured

Method of sections 45 / 46: plain `cargo build --release -p rn-cli` (5.5; 5.4 with `--cfg rustynail_lua54`, own target directory), one binary, the
knob as the only difference with strings of equal length (`innerlink=00` / `innerlink=03` / `innerlink=11`), `RUSTYNAIL_SEED=0`, instructions of the
first run, cycles min of 5 interleaved runs pinned to CPU 4 (`perf stat -e instructions:u,cycles:u`).

**Before the return half** (bit 0 alone, 5.5, instructions): deltablue 400 -10.2 % (cycles -11.5 %), but deltablue default +7.7 %, storage +9.6 %,
bignum_arith +3.8 % (the inner exits ended in tier 1, 47.1). `nest=2` alone: deltablue 400 4397.1 -> 4395.3 M (0.0 %).

**The bar** (30 programs: deltablue, richards, havlak, list, cd, raytrace, awfy nbody, json, storage, queens, towers, permute, mandelbrot, sieve,
spectral_norm, matmul, heapsort, fannkuch, programs/nbody, lua_lexer, astar_jumper, expr_compiler, bignum_arith, bigint_rsa, sudoku_solver,
diff_levenshtein, expr_lang, lua_interp, lua_parser, jit-trace/short_inner; plus deltablue 400 outside the geomean). Rows that moved by more than
0.5 % instructions or 1.5 % cycles (instructions M, cycles in %):

| program | 5.5 `11` instructions | 5.5 `11` cycles | 5.4 `11` instructions | 5.4 `11` cycles | 5.5 `03` instr / cycles | 5.4 `03` instr / cycles |
|---|---|---|---|---|---|---|
| awfy/deltablue | 316.8 -> 319.3 (+0.8 %) | **+6.6 %** (15 runs **+6.7 %**) | 339.2 -> 334.8 (-1.3 %) | **+6.4 %** (15 runs **+7.4 %**) | **+3.5 %** / **+4.9 %** | **+2.1 %** / **+3.6 %** |
| deltablue 400 | 4397.4 -> 3856.2 (**-12.3 %**) | **-13.0 %** | 4828.5 -> 4300.7 (**-10.9 %**) | **-11.5 %** | -2.7 % / -4.8 % | -2.6 % / -4.8 % |
| corpus2/dev/expr_compiler | 5287.3 -> 5104.2 (-3.5 %) | -3.5 % | 5434.0 -> 5299.6 (-2.5 %) | -2.5 % | -5.4 % / -5.1 % | -3.6 % / -3.1 % |
| corpus2/heldout/bigint_rsa | 5657.7 -> 5512.7 (-2.6 %) | -2.0 % | 7072.9 -> 6897.7 (-2.5 %) | -1.8 % | -2.4 % / -2.3 % | -2.2 % / -1.8 % |
| awfy/storage | 487.3 -> 477.5 (-2.0 %) | -1.4 % | 674.3 -> 664.2 (-1.5 %) | +0.3 % | -2.0 % / 0.0 % | -1.5 % / +1.2 % |
| awfy/nbody | 548.5 -> 539.2 (-1.7 %) | -1.3 % | 562.6 -> 553.2 (-1.7 %) | -0.5 % | -1.7 % / -0.6 % | -1.7 % / -0.3 % |
| programs/nbody | 1966.4 -> 1934.4 (-1.6 %) | -1.1 % | 2021.7 -> 1989.6 (-1.6 %) | -0.9 % | -1.6 % / -0.8 % | -1.6 % / -0.8 % |
| apps/raytrace | 299.9 -> 296.4 (-1.2 %) | -0.5 % | 315.0 -> 311.9 (-1.0 %) | -0.6 % | -2.7 % / -3.2 % | -2.6 % / -2.5 % |
| awfy/json | +0.3 % | +1.5 % | +0.3 % | +1.4 % | +0.3 % / +0.4 % | +0.3 % / +1.1 % |
| programs/matmul | 0.00 % | +4.0 % (15 runs +0.1 %) | 0.00 % | -1.1 % | 0.00 % / +3.1 % | 0.00 % / -1.4 % |
| corpus/astar_jumper | +3.0 % (not deterministic, section 39.1) | -7.8 % | 0.0 % | -0.6 % | -0.1 % / -6.8 % | 0.0 % / -1.2 % |

Every other program: within +-0.3 % instructions (identical where no link forms) and +-1.5 % cycles (5.5 `03` havlak -6.3 % cycles at equal
instructions: noise). **Geomean over the 30**: `innerlink=11` 5.5 instructions **0.9971**, cycles **0.9977**; 5.4 instructions **0.9963**, cycles
**1.0002**. `innerlink=03`: 5.5 0.9957 / 0.9941; 5.4 0.9965 / 0.9985.

**Formation lottery** (5.5, instructions, `innerlink=00` -> `11` in %, `RUSTYNAIL_JIT_TRACE_HOT` = 248 .. 264):

| program | 248 | 250 | 252 | 254 | 256 | 258 | 260 | 262 | 264 |
|---|---|---|---|---|---|---|---|---|---|
| awfy/deltablue (default size) | -2.8 | +1.8 | +1.6 | -6.2 | +0.8 | -1.2 | -5.0 | -4.1 | -1.6 |
| deltablue 400 | -11.6 | -8.2 | -9.5 | -13.7 | -12.3 | -12.7 | -13.1 | -12.6 | -10.4 |
| corpus2/dev/expr_compiler | -2.9 | -2.8 | -3.3 | -2.4 | -3.5 | -3.0 | -2.7 | -3.5 | -6.0 |
| corpus2/heldout/bigint_rsa | -0.7 | -1.7 | -3.3 | -2.5 | -2.6 | -3.0 | -2.2 | -3.4 | -1.8 |
| apps/raytrace | +0.4 | -1.4 | -2.1 | -3.2 | -1.2 | -1.1 | -4.2 | +1.3 | -0.3 |
| awfy/nbody | -1.7 | -1.7 | -1.7 | -1.7 | -1.7 | -1.7 | -1.7 | -1.7 | -1.7 |
| awfy/storage | -2.0 | -0.5 | -3.8 | +0.1 | -2.0 | -2.4 | 0.0 | 0.0 | -6.3 |

The gains of the longer runs (deltablue 400, expr_compiler, bigint_rsa, nbody) hold at every threshold; deltablue at its default size is a lottery
(-6.2 .. +1.8 % instructions, +0.8 % at the shipped threshold), and its cycle regression there is the compile cost of the bigger recordings
(`[trace] phases` pipeline 8.9 -> 17.6 M TSC, 47 -> 61 compiles, three outer roots of about 10 KB each in a 0.1 s run), not executed trace code: at
size 400 the same traces save 13 % of the cycles.

### 47.5 Decision

**Default off** (`INNERLINK_PROCESS = 0`): the bar is missed by one program on both versions, deltablue at its default size, by cycles (+6.6 % /
+6.4 %; 15 runs +6.7 % / +7.4 %) at +0.8 % / -1.3 % instructions; `innerlink=03` (without the side links) also misses it by instructions (+3.5 % /
+2.1 %). The geomean is better on 5.5 (0.997 instructions / 0.998 cycles) and level on 5.4 (0.996 / 1.000). The recommended setting is
`innerlink=11` (bits 0, 1, 3; bit 2's deep links cost deltablue default instructions: +7.4 % with bits 0-2 against +3.5 % without bit 2, +2.6 %
with all four against +0.6 % without bit 2, in scans of an intermediate build):
turning it on is the one-line change of `INNERLINK_PROCESS` to 11, and the case for it is deltablue -11 .. -13 % on longer runs and expr_compiler,
bigint_rsa, nbody, storage, raytrace -1 .. -3.5 % instructions on both versions, against the compile cost of one short run.

### 47.6 Tests, gates, what was not run

* `tests/vm/t9_01_innerlink.lua` (13 sections, 47.3; also in `vm_corpus` against `ref/55` and `ref/54`), registered in `tr_core`'s programs;
  `tr_core::t9_innerlink_programs` (`t9_01`, `t1_01`, `k3_01` x `always` / `on2` x `innerlink` 0 / 1 / 3 / 11 / 15, `+verify`; for 15: exit storm,
  budgets 3 / 17 / 200, GC stress 1 / 2 in both collector modes, stack-move stress; with the oracle feature the identity at budgets none / 3 / 17;
  asserts links with bit 0 and none without, return-half traces and woken exits with bit 1 and none without); `tr_policy::t9_innerlink_knob`. Each of
  the three mutants checked by hand (`RUSTYNAIL_JIT_MUT`) against the detector: caught.
* `t9_01` by hand against the references, 5.5 and 5.4, `TRACE=always` / `on`, `innerlink` 0 / 1 / 3 / 7 / 11 / 15, also under GC stress 2 (both
  collector modes) and stack-move stress: same output everywhere.
* `JITDIFF_MODES="off tr trdef" JITDIFF_ENTERS=1 scripts/jit-diff.sh 1 40` on the 5.5 and 5.4 release binaries with `RUSTYNAIL_JIT_TRACE_LIMITS`
  `lockstep=1` and `lockstep=1,innerlink=15`: 0 diffs, 0 timeouts (35 / 40 seeds with traces in `tr`) in all four runs. `scripts/jit-official.sh` with
  `RUSTYNAIL_JIT=always RUSTYNAIL_JIT_TRACE=always`, `innerlink=0` and `innerlink=15`, 5.5 and 5.4: `JIT-OFFICIAL OK` (4 runs). The 236 `tests/vm`
  programs, JIT off against `always` + `TRACE=always` + `innerlink=15`, release binaries: 5.5 plain and under GC stress 2, 5.4 plain and under
  incremental GC stress 2: no difference except `t6_01_ctor` (its `pairs` order differs between JIT off and on with `innerlink=0` too) and, under
  5.5 GC stress 2, `tables_15_alimit_big_moves` (its printed hash depends on the string-hash seed, which this sweep did not pin; `innerlink=0`
  gives the same two values depending on the environment; no link forms in it).
* Code commit: `GATE_CPUS=2-15 scripts/gate-light.sh innerlink_,innerret_ --fast`: all PASS (fmt, mutant map, clippy x3, core 5.5 / 5.4, trace suites 5.5
  (160 s) / 5.4 (152 s), `vm_corpus`, jit-diff 1-50, mutant group 3 / 3 caught). Its first run failed on test constructors of `HeadData` / `Recorded`
  and an unused counter in the new test; fixed before the commit.
* Not run (by instruction): `gate-heavy.sh`, the full mutant runner, `tr_oracle` / `oracle_options`, PGO builds, all-223 benchmarks, `bench-fast.sh`.

## 48. T5 (object keys on trace, knob `t5`) and T7 (leaf-function roots, knob `t7`) (2026-11-23, base 202cfe7)

Two handles of docs/STUDY_TIER1_RESIDUE.md section 5, built knob-gated in two commit series (T5 first). 48.1-48.5 are T5, 48.6 on T7.

### 48.1 T5: what was missing

`t[k]` / `t[k] = v` with a table, function, userdata or thread as the key aborted the recording `TableHash` (`Rec::int_key`: not a number), so every loop
of the havlak shape (`number[node] = n`, `if number[target] == nil`, `inPool[s] = true`) was either untraced or ended its trace at the access
(`awfy/havlak` records 8 `TableHash` aborts, plus the regions behind them). LuaJIT has `HREF` for any key type. The integer form of `HREFV`
(H1, `hashint`) and the short-string form (N6) already walk the chain of a node vector at run time; the object form is the third.

### 48.2 What was built (knob `t5`, `RUSTYNAIL_JIT_TRACE_LIMITS=t5=N`, default 0)

| bit | what |
|---|---|
| 0 (1) | `GETTABLE` with an object key. Present non-nil value: guarded node-count (`FLOAD lsizenode == k`), `FLOAD node`, `HREFV ... HREF_OBJ` (walk of the chain from the main position `(address as u32) % ((size - 1) | 1)`, `hashmod` of `mainposition`; a node matches when its key tag is the tag of the IR type of the key operand and its key word the address; no match exits), typed `HLOAD`. The `__index` metatable is irrelevant for a present non-nil value. Nil-valued node: the same, with the TR14 `__index` guard of the table (a metatable with `__index` aborts `Metamethod`). Absent key: the TR14 `__index` guard, the node-count guard and `HREFV ... HREF_OBJ | HREF_ABS` (exits when the walk finds the key, so a key inserted later is found), result `nil`. |
| 1 (2) | `SETTABLE` over a present object key: `hash_int_set_k` (the H1 integer-key store with the object form of `HREFV`): `HSTORE` guarded against a nil old value, or, for a nil-valued node, the `__newindex` guard and an unconditional store; the `TBAR` of a collectable value. The key needs no barrier (it is not stored). |
| 2 (4) | Insertion of an absent object key: the generic `TSET` helper (`jit_tr_tset`) with the key's tag in `a >> 16` (tag 0 = integer, the old meaning): `TableRef::set` (rehash, key write barrier `barrier_back(t, key)` in `newkey`, tm-cache invalidation), then the `NE` guard of its result and the `TBAR` of a collectable value. `__newindex` guard as for integer keys. Needs bit 1 (the store dispatch). |

Key types: table, Lua closure, C closure, full userdata, thread (`obj_key_ty`; light userdata and light C functions hash the same way but are not
traced). A nil key, NaN, float and boolean keys are not objects: they take the old paths (`int_key` aborts), so key normalisation is untouched.

Pieces: `ir::HREF_OBJ` (op bit 12); `asm.rs` `hrefv` (the integer-hash code with the key truncated to its low 32 bits: `Shl 32; ShrI 32`, the key tag
from `tag_of(ty_of(key))`, one more scratch register; `ra.rs` clobber of `rdx` for lsize >= 2 as for the integer form); `interp.rs` needs no change
(its `HREFV` already finds a node by key word; the model's `fill_node` now gives an object key node its model word: the handle of a table or Lua
closure (`val_of`), the address otherwise); `TableRef::obj_node` (rn-core, read-only `getgeneric`); `Rec::obj_get` / `obj_set` /
`hash_ref_int_k` / `hash_int_set_k`; the `TSET` key tag (`tset_core`). Aliasing: `opt_mem` already treats a variable-key `HREFV` as `May` against
everything but the same key on the same node vector (an object key is an SSA value like a variable string). Loop optimiser: unchanged (an
insertion is a `TSET`, which is not `header_safe`, so no `FLOAD` of the node vector or of `lsizenode` survives it and no `HREFV` is hoisted over
it).

### 48.3 Soundness notes

* **Identity.** A passing `HREFV` means that a node of this table holds a key with this tag and this address; a dead key has the dead tag, so a
  collected key whose address was reused is no match. The key operand is a live register value of the recorded type (`SLOAD` type guard / typed
  `ALOAD` / `HLOAD`), so the address is that object's.
* **Rehash / resize.** The node-count guard precedes every walk (the mask / modulus is an immediate), the `FLOAD` of the node vector follows the
  guard; an insertion between two accesses (`TSET`) kills the `FLOAD` forwarding, so the second access re-guards. A key moved by a rehash is found
  at its new position or, if the table shrank, the node-count guard exits.
* **GC.** The store of a collectable value keeps its `TBAR`; the inserted key's barrier is `newkey`'s (`heap.barrier_back(t, key)`), the helper is the
  table's own `set`. Weak-keyed tables: a cleared entry is a dead key (no match: exit for a read of a present key, the absence walk passes). GC stress
  1 and 2, generational and incremental, `GC_VERIFY` (`tr_core::t5_gc_stress`).
* **Metamethods.** `__index` / `__newindex` are consulted exactly where the interpreter consults them: a raw hit of a non-nil value never, a nil-valued
  or absent key by the TR14 flag guards (which exit when a metatable gains the field), an insertion by the `__newindex` guard. A table with
  `__index` / `__newindex` already present aborts `Metamethod` (the integer-key `mm` paths are not extended).
* **Not objects.** `t[nil]` (read: nil; write: error), `t[0/0]` (read: nil; write: error), float keys with an integer value and booleans never reach
  `obj_*` (their register type is not an object type); a loop whose key changes type exits its type guard.

### 48.4 Tests

* `tests/vm/jit_t5_01_object_keys.lua` (all the programs below in one file; `vm_corpus` runs it against `ref/55` and `ref/54`; checked by hand with
  `t5=7`, `JIT=always TRACE=always`, GC stress 1 and 2: same output).
* `tr_core`: programs `t5_get_present`, `t5_get_absent`, `t5_nilvalue_meta`, `t5_set_present`, `t5_insert`, `t5_havlak_shape`, `t5_gc_small`,
  `t5_many_keys` (`tests/tr_core/t5_progs.rs`); `t5_object_keys` (3 trace modes x {plain, `+verify`, exit storm, budget 17, `-loop`, `-fwd`} x {`t5=7`,
  process limits + `t5=7`} + stack-move stress + `t5=1`, `t5=3`, `t5=0`), `t5_gc_stress` (stress 1 / 2, both collector modes, `GC_VERIFY`),
  `t5_object_keys_do_not_exit_per_key` (200 keys, a wrong modulus is an exit per key); `tr_record_calls::t5_object_keys_record_their_guards`;
  `tr_policy::t5_objkey_knob`.
* Mutants (registry 847 -> 853): `hrefo_lsize_skip`, `hrefo_hash_64` (the key is not truncated to 32 bits), `hrefo_nil_meta_skip`,
  `hrefo_abs_meta_skip`, `hrefo_abs_lsize_skip`, `tset_key_tag_int` (the helper takes the key for an integer): 6 / 6 caught.
* `JITDIFF_TSET=t5` (new, two templates: a mutating object-key set, the havlak numbering): seeds 1..40 `off tr trdef` and 41..200 `off tr tron`
  with `lockstep=1,t5=7`, 5.5 and 5.4 (the 5.4 run 1..40): 0 diffs; the default set 1..40 `off tr trdef` with `t5=7` on both versions: 0 diffs;
  `scripts/jit-official.sh` `JIT=always TRACE=always` `t5=7`, 5.5 and 5.4: `JIT-OFFICIAL OK`.

### 48.5 Measurements and decision (T5)

Method: `perf stat` instructions:u / cycles:u, `RUSTYNAIL_SEED=0`, CPU 4, one binary with `RUSTYNAIL_JIT_TRACE_LIMITS=t5=0` against `t5=7` (equal
length strings), min of 3 per column over a 30-program sample (14 awfy, 5 apps, 5 real, `corpus/` astar_jumper, classic_vector, lua_lexer,
json_rxi, `corpus2/dev` event_promise, memo_cache).

| program | 5.5 instr | 5.5 cycles | 5.4 instr | 5.4 cycles |
|---|---:|---:|---:|---:|
| awfy/havlak (default size) | 508.0 -> 469.3 M **-7.6 %** | 148.0 -> 146.9 M -0.8 % | 572.9 -> 535.4 M **-6.5 %** | -1.8 % |
| awfy/havlak (30) | 2068 -> 1804 M **-12.8 %** | 657.5 -> 600.0 M **-8.8 %** | - | - |
| corpus/astar_jumper | **+1.16 %** (lottery, below) | +0.9 % (min of 9: -1.1 %) | -4.4 % | -0.9 % |
| any other program | max +0.03 % (json_rxi) | max +1.33 % (queens, at +0.01 % instructions: noise) | max +0.01 % | max +1.52 % (lua_lexer, +0.01 % instructions) |
| geomean of 30 | 0.9978 | 0.9993 | 0.9963 | 0.9992 |

Hot-threshold sweep (`RUSTYNAIL_JIT_TRACE_HOT` 248 .. 264 step 2, 5.5, one run per cell, instructions M):

```
havlak        A (t5=0) 504.9 505.4 506.6 509.8 508.2 506.0 515.4 508.9 504.0
              B (t5=7) 462.4 454.5 459.8 463.0 469.3 465.7 470.6 461.8 457.6     d% -8.4 -10.1 -9.2 -9.2 -7.7 -8.0 -8.7 -9.3 -9.2
              cycles d% -2.5 -3.0 -5.1 -2.0 -1.2 -1.0 -4.9 -2.4 -4.0
astar_jumper  A (t5=0) 4525 4360 4634 4541 4422 4555 4868 4586 4503
              B (t5=7) 4501 4618 4722 4755 4485 4617 4593 4599 4429     d% -0.5 +5.9 +1.9 +4.7 +1.4 +1.4 -5.7 +0.3 -1.7 (mean +0.9)
```

**astar_jumper is a formation lottery, not a systematic cost.** The same binary and knobs give 4367 .. 4693 M instructions over 22 runs at the
default threshold (cycles 1599 .. 1741 M) with `t5=0`; with `t5=7` the runs fall into two groups (4481 .. 4498 M in 6 of 10 runs, 4650 .. 4661 M in
the other 4, a root trace entered 2.74 M against 3.26 M times) and the cycles are lower in every run (1579 .. 1701 M against 1599 .. 1741 M, mean
-3.3 %). The program inserts into `toClear[neighbour]` and runs `pairs` over it, so it takes the new `TSET` path in both groups. Instructions over
20 runs each: mean +1.8 %; cycles -3.3 %.

**Decision: default off (`T5_PROCESS = 0`).** The bar (max +1 % instructions / +2 % cycles on any program, geomean not worse) is missed on 5.5 by one
program by instructions (astar_jumper, +1.16 % at the min-of-9, +1.8 % mean, with cycles 3 % better), met on 5.4 and everywhere else; the geomean is
better on both versions. Turning it on is the one-line change of `T5_PROCESS` to 7; the case for it is havlak -7.6 .. -12.8 % instructions on both
versions and a lower cycle count on astar_jumper, the case against the strict bar is the instruction count of one lottery program. What is not
covered if it ships on: `jit-diff` `trdef` / `trdefon` run the shipped limits, so they test `t5` only through the explicit `t5=7` runs above.

### 48.6 T7: leaf-function roots (knob `t7`, default 0)

**The mechanism that exists.** A function-entry head (`fnroot=0`: a native-recursion probe) records ONE invocation of the function. A leaf invocation
ends at the root `RETURN` before `fnmin` (16) bytecodes: `FnShort` (`Rec::fn_end`). Four such samples blacklist the head; `fnpromo` then promotes it
ONCE per prototype to a generic function root (`promote_fn`: fresh abort budget, `64 << fnpromow` entries of waiting, not for a function that reaches
itself through an upvalue), which records like `fnroot=1` but still ends `FnShort` below `fnmin`. So a short non-recursive function never gets a
function trace; its callers inline it when they are traced, and the calls that stay in tier 1 / the interpreter (deltablue, richards, json) pay the
call protocol of tier 1. `fnmin=0` is the study's ceiling (STUDY_TIER1_RESIDUE section 3 row 3: deltablue L -2.0, richards L -1.4, towers +4.4).

**What was built (one decision point in `driver_start`, `fn_min`).**

| bit | what |
|---|---|
| 0-1 (`t7 & 3`) | a PROMOTED head (`fnpromo`) records bodies of at least 0 (1), 4 (2) or 8 (3) bytecodes instead of `fnmin` (all other heads, the recursion probes, keep `fnmin`: unchanged) |
| 2 (4) | such a head waits `fnpromow + 2` (four times as many entries) before its first recording |
| 3 (8) | only for a STATIC leaf: a prototype with no `CALL` / `TAILCALL` / `TFORCALL` (`proto_is_leaf`) |

Mutants `t7_fnmin_kept` (the promoted head keeps `fnmin`), `t7_leaf_any` (bit 3 admits a function that holds calls); detector
`tr_core::t7_leaf_roots` (`t7_towers` / `t7_accessors`, `fnpromow=0`: `t7=1` installs more roots than `t7=0`, `t7=9` exactly as many as `t7=0` on
towers, whose leaf methods hold `error` calls; every output equal to the interpreter's in `always` / `on2` / `on`, `+verify`, exit storm, budgets,
stack-move, GC stress 1 / 2, all classes); `tr_policy::t7_leaf_knob`; `tests/vm/jit_t7_01_leaf_roots.lua` (vs `ref/55`, by hand with `t7=1,fnpromow=0`,
`t7=15` under `JIT=always TRACE=always`, GC stress 1 / 2: same output). 855 mutants in the registry.

**Measurements** (same method as 48.5; 5.5 unless noted). Large sizes, instructions M / cycles M, `t7=0` -> `t7=1`:

| program | instructions | cycles |
|---|---:|---:|
| towers (450) | 1413 -> 1479 **+4.6 %** | 312 -> 337 **+8.2 %** |
| richards (250) | 1319.6 -> 1318.8 -0.06 % | 382 -> 396 (noisy: 381-415 across runs) |
| deltablue (200) | 2280 -> 2233 **-2.1 %** | 640 -> 634 -0.9 % |

Sample of 30, default sizes, `t7=0` against `t7=1`:

| | 5.5 instr | 5.5 cycles | 5.4 instr | 5.4 cycles |
|---|---:|---:|---:|---:|
| awfy/towers | **+4.6 %** | **+7.4 %** | **+4.6 %** | **+9.5 %** |
| awfy/richards | **+1.04 %** | **+2.4 %** | **+1.03 %** | **+2.2 %** |
| apps/markdown | **+1.49 %** | **+3.7 %** | **+1.48 %** | **+4.1 %** |
| apps/events | +0.11 % | +0.6 % | **+12.2 %** | **+11.4 %** |
| awfy/storage | 0 | **+2.8 %** | 0 | +0.5 % |
| awfy/json | **-5.4 %** | -3.1 % | -2.2 % | -1.7 % |
| real/json_roundtrip | **-4.9 %** | -3.6 % | -4.7 % | -2.8 % |
| corpus/json_rxi | **-3.3 %** | -0.6 % | -3.2 % | -1.5 % |
| awfy/deltablue | -0.67 % | +0.5 % | -0.59 % | -0.1 % |
| geomean of 30 | 0.9977 | **1.0042** | 1.0025 | **1.0058** |

Hot-threshold sweep (248 .. 264 step 2, 5.5, one run per cell), `t7=0` -> `t7=1` instructions / cycles d%: `apps/markdown` +1.4 .. +1.5 % /
+3.0 .. +4.6 % at **every** threshold (a systematic cost); `awfy/richards` +0.2 .. +1.1 % / -1.4 .. +5.1 % (outside 254) (7 of 9 thresholds +0.9 .. +1.1 % instructions;
at 254 both columns are in a bad formation: 242.8 -> 207.5 M); `awfy/json` -1.9 .. -6.5 % / -3.6 .. +0.3 % (systematic gain); `awfy/deltablue`
-2.2 .. +1.3 % / -2.5 .. +2.6 % (nothing); `awfy/towers` -1.8 .. +4.6 % / -2.9 .. +17.9 % (the 256 column is +4.6 % / +17.9 %: lottery, mean +0.9 % / +3 %; at the large size
the +4.6 % is steady).

**Variants measured, none meets the bar** (instructions M, towers 450 / deltablue 200 / richards 250 unless marked; base 1413 / 2280 / 1320):

* Minimum length (the same as the `fnmin` knob: only promoted heads record short bodies): `fnmin=12 .. 16` identical to the base on all three;
  `fnmin=10` towers +4.6 %, deltablue -0.3 %; `fnmin=8` towers +4.6 %, deltablue -1.0 %; `fnmin=0` towers +4.6 %, deltablue -2.1 %. The functions that
  cost towers (`pushDisk`, `popDiskFrom`, `moveTopDisk`: 11-15 bytecodes with an `error` call) are as long as the ones that pay in deltablue, so no
  length separates them; classes 1, 2 and 3 are the same on the programs measured (every short promoted body is below 8).
* Longer wait (bit 2, and `fnpromow` 5 / 7 / 9 / 12 with `t7=1`): towers +2.5 % (bit 2 = `fnpromow` 5), +2.2 % (7 .. 12); deltablue -1.6 % (5), -0.5 % (7),
  +2.2 % (9), +8 % (12: the roots arrive too late to pay); richards -0.4 .. -0.2 %. The towers cost stays at any wait.
* Static leaf only (bit 3): **identical to the base** in all three programs (the promoted heads that gain in deltablue and lose in towers all hold calls).
* What the towers cost is not entries and exits of the new roots: total exits 1382484 -> 1389716 (+0.5 %), `entered` +23 k of 339 k; the extra instructions
  are in how the callers form around roots of their callees (`stitches` 6 -> 8 at the default size, richards 0 -> 5, markdown 4 -> 10, `entered`
  2244 -> 23437 and `exits` 4686 -> 51781 in markdown): a caller recorded after the callee has a root trace stitches to it where it would have inlined it.
  Not measured further (no per-trace attribution of the instructions was done).

**Decision: default off (`T7_PROCESS = 0`).** The bar is missed on both versions by towers (+4.6 % instructions, +7.4 .. +9.5 % cycles), richards (+1.0 % /
+2.2 .. +2.4 %), markdown (+1.5 % / +3.7 .. +4.1 %) and, on 5.4, events (+12 %); the geomean of cycles is worse on both (1.0042 / 1.0058). No formulation of the three
above keeps the json gain (-2 .. -6 % instructions, three programs, both versions) without the losses. What would be next (not done): a selector for a
leaf root that looks at the callers (do not install a root for a function that an installed trace of a caller already inlines, or retire the root when a
caller's recording stitches to it), or the per-trace attribution of the towers cost.

### 48.7 Gates, what was not run

* T5 commit: `GATE_CPUS=2-15 scripts/gate-light.sh hrefo_,tset_key_tag --fast`: all PASS (fmt, mutant map, clippy x3, core 5.5 / 5.4, trace suites 5.5 (162 s) /
  5.4 (153 s), `vm_corpus`, jit-diff 1-50 (default limits, `off tr trdef`), mutant group 6 / 6 caught). Its first run failed two registry tests (the frozen
  count and the TABLE order); fixed before the commit.
* T7 commit: `GATE_CPUS=2-15 scripts/gate-light.sh t7_ --fast`: all PASS (same steps; trace suites 5.5 161 s / 5.4 153 s; mutant group 2 / 2 caught, 24 s).
* Not run (by instruction): `gate-heavy.sh`, the full mutant runner, `tr_oracle` / `oracle_options`, PGO builds, all-223 benchmarks, `bench-fast.sh`,
  the official suite with `t7` (T5 was run through it with `t5=7`, both versions).

## 49. C1: compile-speed levers of the trace back end and the encoder (knob `cspeed`, check `ccheck`; 2026-11-24, base b12bba7)

Task: the C1 cause of docs/STUDY_WHY_GAP.md (compile + record cost is 36-56 % of the cycle gap to LuaJIT on richards, raytrace, cd, towers,
fmt_s; each trace costs 3.7-8.9x LuaJIT's). Goal: 2-3x cheaper compiles at identical generated code. **Result: identical code, 7-11 % cheaper
compiles, -0.8 % instructions / -1.0 % cycles over the 30-program sample (5.5), -1.1 .. -3.8 % instructions and -1.5 .. -4.2 % cycles on the five
compile-heavy programs.
The 2-3x goal and the double-digit cycle target are missed by a wide margin; section 49.5 says why and what it would take.**

### 49.1 Method and the measured compile profile

* Builds: plain `cargo build --release -p rn-cli` (5.5; 5.4 with `--cfg rustynail_lua54`, `LUA_VERSION=5.4`), base b12bba7 and the final commit
  built from `git archive` trees of equal path length (`wsb` / `wsn`), staged in equal-length directories (`b/bas`, `b/new`, `b/b54`, `b/n54`).
* Totals: `perf stat -e instructions:u` (one run, deterministic) and `cycles:u` (min of 5 interleaved runs, CPU 4, 300 ms spin before each run),
  `RUSTYNAIL_SEED=0`, default bench size.
* Compile cost, exact: two ways. (1) The same binary with the levers off and on (`RUSTYNAIL_JIT_TRACE_LIMITS=cspeed=0` against the default): the
  generated code is identical (49.3), so the instruction difference is the compile pipeline alone. (2) Per phase: a scratch build (not committed)
  that runs one phase a second time on the same inputs (`black_box`) under an environment variable; the instruction delta of a run is the exact
  cost of that phase in that program (`TWICE`: the encoder, traces and tier 1; `ASM2`: `asm::assemble_dl`; `RA2`: `ra::allocate_hinted`; `EX2`:
  `exit::build_exits_ex`; `FRONT2`: the passes before allocation, on a copy of the recording; `CLONE`: that copy alone).
* Profiles: `perf record -e instructions:u --call-graph fp --user-callchains` of a frame-pointer build (`-C force-frame-pointers=yes`, its own
  target directory, removed), attributed by call chain to recorder / pipeline / tier 1 / lazy stubs and their sub-phases; AMD IBS op sampling
  (`ibs_op/cnt_ctl=1,swfilt=1/u`, precise IP) where the skid of `instructions:u` sampling (samples pile up behind store-forwarding stalls and
  mispredicted indirect branches) made the line-level picture unreliable.

**Baseline profile (b12bba7, IBS / fp call chains, share of all instructions):** raytrace: compile 35 % (recorder 10, pipeline 18, tier 1 6);
cd: 35 % (recorder 8, pipeline 23, tier 1 4); richards: 29 % (recorder 7, pipeline 16, tier 1 6). Inside the compile instructions (cd /
raytrace / richards): x86 encoder (traces and tier 1) 15 / 14 / 28 %; register allocation 12 / 12 / 11 %; assembler 15 / 8 / 9 %; loop pass
13 / 8 / 13 %; recorder steps 20 / 32 / 28 %; exits 8 / 5 / 3 %; sinking 4 / 10 / 0.2 %; tier-1 lowering 3 / 5 / 12 %; malloc / free / realloc
about 5 % (the study's 16-22 % counted the libc internals of a stripped libc as well). Exact per-phase costs of the old paths (instructions M, `cspeed=0`
of the final binary, which runs every old path except the FxHash maps of 49.2):

| program | total | encoder (twice) | assembler | allocator | exits | passes | recording copy |
|---|---:|---:|---:|---:|---:|---:|---:|
| apps/raytrace | 302.1 | 13.49 | 7.53 | 10.51 | 5.89 | 13.68 | 0.22 |
| awfy/cd | 715.1 | 35.80 | 28.09 | 26.40 | 14.03 | 37.72 | 0.66 |
| awfy/richards | 173.1 | 10.84 | 4.11 | 5.46 | 1.45 | 6.18 | 0.11 |
| awfy/towers | 172.8 | 4.42 | 2.38 | 3.21 | 1.23 | 3.88 | 0.07 |
| lib/fmt_s | 26.3 | 1.11 | 0.09 | 0.13 | 0.03 | 0.15 | 0.00 |

Three findings the study's breakdown did not have: (1) the encoder ran at 156-172 instructions per micro-op (`Assembler::rm_op` pushed every
byte through a checked `Vec::push`, took the operand through memory and re-read it at a stall); (2) `Allocation::uses_gpr` / `uses_xmm` scanned
the whole allocation for every query, up to 16 times per constant float compare: 5.7 % of cd's compile instructions; (3) sinking's "store between
two guards of one snapshot" check scanned every snapshot map once per store of every candidate (quadratic; `sink::rebuild` 84 % self on raytrace).
Everything else is linear in the snapshot volume (one snapshot of 10-60 entries per guard-bearing bytecode, scanned by the recorder, the loop
pass, sinking, the allocator, the exit builder and the shape hash): no single function above 5 % of the compile after the fixes.

### 49.2 The levers (knob `cspeed` of `RUSTYNAIL_JIT_TRACE_LIMITS`, bits of `crate::cspeed`, default all on = 123; `cspeed=127` is the same)

| bit | lever | what changed | commit |
|---|---|---|---|
| 1 `MASKS` | allocator register masks | `uses_gpr` / `uses_xmm` from masks built by one scan at the first query (`Allocation::used`, `OnceCell`) | 1 |
| 2 `ENC` | x86 encoder | `[prefix][REX] opcode ModRM [SIB] [disp]` written in place into 16 zeroed bytes and cut back (no per-byte push, no variable-length `memcpy`); the encoding inlined into every emitter (operand in registers); buffers presized from the program (`Assembler::with_capacity`) | 2 |
| 8 `FXHASH` | shape hash | `cache::shape_hash_ir` with the Fx word mix (the FNV variant mixed every byte of a slice); the value is compared only within the process (Q3) | 1 |
| 16 `PASSES` | passes / assembler | sinking's check from an index of the snapshots naming each candidate, its per-instruction candidate lookup from the candidate map; the loop pass's output builder and the assembler's micro-op program presized | 3 |
| 32 `RA` | allocator | free-register choice and eviction scan without a vector per query; the per-snapshot set-up (unfuse, validity check, snapshot-only marks) once per snapshot, not once per guard (TR27: the guards of a snapshot share its map) | 1, 4 |
| 64 `REC` | pipeline front | the pipeline takes the closed recording over instead of copying it | 3 |

Also exact and not knob-gated (commit 1): the tier-1 lowering's label maps (`exit_at`, `exit_trap`, `land_of`, warm-up maps) and the pipeline's
resolver map on FxHash instead of SipHash; every iteration over them is sorted first, so the order cannot reach the code.

`cspeed=0` restores every old path (the hash maps above excepted). The levers are read from a thread-local cell that the trace pipeline and the
tier-1 compile set from the state's limits (`crate::cspeed::set`).

### 49.3 Identical code: `ccheck` and the regression test

`RUSTYNAIL_JIT_TRACE_LIMITS=ccheck=1`: every trace compile runs (a) its passes a second time on a copy of the recording with `cspeed=0` and compares
the IR text (`print::dump`) and the writeback, (b) its allocator and exit builder again with `cspeed=0` and compares the allocation and the exit
structures, (c) its assembler and encoder again with `cspeed=0` on the checked allocation and exits and compares the machine code, the label
offsets and the stub references byte for byte; every tier-1 compile re-encodes its program with the old encoder and compares. A difference aborts
the compile (`aborted ccheck`) and is counted: `[trace] ccheck: compared N differ M` in the `RUSTYNAIL_JIT_VERBOSE=1` statistics.

* Regression test `tr_core::c1_cspeed_identical_code`: the whole `tr_core` corpus (`selected(false)`) x `always` / `on2` with `ccheck=1`, the
  programs that must trace with `+verify` and with GC stress 1 and 2, plus a `cspeed=0` control run; every program must give the interpreter's
  results, no compile may differ, more than 100 compiles must be compared. Runs in both versions' trace suites (5.5 and 5.4), 12 s.
* Mutants (each detected by that test; registry 855 -> 859): `cspeed_ra_last_free` (the fast free-register choice takes the last free register),
  `cspeed_enc_disp32` (the fast encoder writes a byte-sized displacement in four bytes), `cspeed_sink_cand_miss` (the fast candidate lookup misses),
  `cspeed_ra_unfuse_skip` (the once-per-snapshot set-up skips every snapshot). Each mutant sits on the fast path only, so the old path of `ccheck`
  shows the difference.
* `ccheck=1` on the final release builds: raytrace 56 / 61 compiles compared (5.5 / 5.4), cd 270 / 294, richards 51 / 51, towers 32 / 32, fmt_s
  3 / 3, deltablue 95 / 95, classic_vector 34 / 32, lua_parser 155 / 138: 0 differ.

### 49.4 Numbers

**Compile cost (same binary, levers off -> on, instructions; the code is identical, so the difference is the compile pipeline):**

| program | total M, levers off / on | saved M | compile M (IBS share x total), off / on | compile instructions | compile `Mtsc` (phase timers, min of 7), off / on |
|---|---|---:|---|---:|---|
| apps/raytrace | 302.2 / 294.9 | 7.3 | 102.8 / 95.5 (32.4 %) | **-7.1 %** | 23.67 / 22.06 (-6.8 %) |
| awfy/cd | 714.4 / 685.9 | 28.5 | 259.6 / 231.1 (33.7 %) | **-11.0 %** | 46.59 / 42.30 (-9.2 %) |
| awfy/richards | 173.0 / 168.5 | 4.5 | 53.5 / 49.0 (29.1 %) | **-8.4 %** | 9.25 / 8.47 (-8.4 %) |
| awfy/towers | 172.7 / 170.3 | 2.4 | 33.7 / 31.3 (18.4 %) | **-7.1 %** | 5.25 / 4.86 (-7.4 %) |
| lib/fmt_s | 26.2 / 25.9 | 0.33 | 4.3 / 3.9 (15.2 %) | **-7.7 %** | 0.74 / 0.75 (0 %, tier 1 only) |

(The compile share is the IBS share of the samples whose call chain is inside the recorder, the pipeline, a tier-1 compile or a lazy stub, at the
final code; the "off" compile cost adds the saved instructions back. `Mtsc` = `rec_step + pipeline + tier1 + lazystub` of `[trace] phases`.)

Per phase (exact, same binary; instructions M, levers off -> on): encoder raytrace 13.49 -> 9.88 (-27 %), cd 35.71 -> 25.93 (-27 %), richards
10.84 -> 7.90 (-27 %), towers 4.41 -> 3.20, fmt_s 1.11 -> 0.81; assembler raytrace 7.55 -> 5.34, cd 27.97 -> 16.46 (-41 %, the register-mask scans),
richards 4.10 -> 3.53; allocator raytrace 10.67 -> 9.94, cd 26.94 -> 25.29, richards 5.57 -> 5.18 (-6 .. -7 %); exits unchanged (5.8 / 14.0 / 1.4);
passes raytrace 13.64 -> 13.21, cd 37.60 -> 36.00, richards 6.22 -> 5.97 (-3 .. -4 %). The recorder and the tier-1 lowering were not changed.
Same-binary instruction deltas over the 30-program sample (5.5 / 5.4): no program worse; the largest gains cd -3.98 / -3.97 %, richards -2.59 /
-2.60 %, lru_cache -2.47 / -1.98 %, raytrace -2.42 / -2.66 %, deltablue -2.25 / -2.09 %, json -1.52 / -1.35 %, expr_lang -1.51 / -1.46 %, towers
-1.38 / -1.39 %, fmt_s -1.26 / -1.29 %, havlak -1.00 / -0.96 %; the long corpus programs -0.05 .. -0.8 %.

**Whole programs, base b12bba7 against the final commit (30-program sample of section 43 plus `lib/fmt_s`; instructions of one run, cycles min of
5 interleaved runs, CPU 4; first-pass outliers re-run with 11 runs in brackets):**

| program | 5.5 instr M base / new | 5.5 instr | 5.5 cycles M base / new | 5.5 cycles | 5.4 instr | 5.4 cycles |
|---|---|---:|---|---:|---:|---:|
| awfy/bounce | 306.5 / 305.1 | -0.46 % | 60.6 / 60.2 | -0.63 % | -0.45 % | -1.43 % |
| awfy/cd | 713.2 / 686.0 | **-3.81 %** | 247.2 / 236.9 | **-4.18 %** | -3.81 % | -4.44 % |
| awfy/deltablue | 317.2 / 310.7 | -2.04 % | 105.7 / 102.8 | -2.81 % | -1.90 % | -2.09 % |
| awfy/havlak | 508.5 / 503.4 | -1.00 % | 146.6 / 147.8 | +0.84 % (-0.42 %) | -0.87 % | +0.21 % |
| awfy/json | 445.2 / 439.1 | -1.37 % | 138.3 / 136.3 | -1.48 % | -1.20 % | -1.77 % |
| awfy/list | 393.5 / 391.5 | -0.51 % | 96.2 / 94.8 | -1.38 % | -0.47 % | -0.73 % |
| awfy/mandelbrot | 519.9 / 519.5 | -0.08 % | 272.3 / 272.4 | +0.02 % | -0.07 % | -0.03 % |
| awfy/nbody | 548.5 / 547.7 | -0.15 % | 92.8 / 93.6 | +0.85 % | -0.15 % | +1.18 % |
| awfy/permute | 193.1 / 192.7 | -0.21 % | 36.0 / 35.8 | -0.65 % | -0.20 % | -1.83 % |
| awfy/queens | 507.7 / 506.4 | -0.25 % | 95.4 / 95.8 | +0.44 % | -0.39 % | -1.29 % |
| awfy/richards | 172.6 / 168.5 | **-2.37 %** | 59.7 / 58.3 | **-2.32 %** | -2.39 % | -0.74 % |
| awfy/sieve | 780.7 / 780.4 | -0.05 % | 113.6 / 114.0 | +0.30 % | -0.04 % | -2.87 % |
| awfy/storage | 499.9 / 499.5 | -0.08 % | 101.7 / 102.1 | +0.47 % | -0.06 % | +2.00 % (-0.02 %) |
| awfy/towers | 172.5 / 170.3 | **-1.24 %** | 46.9 / 45.7 | **-2.38 %** | -1.25 % | -2.36 % |
| apps/events | 691.0 / 689.2 | -0.25 % | 164.3 / 160.8 | -2.14 % | -0.24 % | -1.16 % |
| apps/expr_lang | 510.3 / 503.3 | -1.37 % | 130.8 / 128.6 | -1.66 % | -1.33 % | -2.48 % |
| apps/lru_cache | 317.6 / 310.5 | -2.24 % | 142.4 / 137.0 | -3.83 % | -1.80 % | -2.10 % |
| apps/markdown | 382.3 / 380.0 | -0.59 % | 119.8 / 117.1 | -2.27 % | -0.45 % | -1.07 % |
| apps/raytrace | 301.7 / 294.8 | **-2.28 %** | 95.4 / 93.8 | **-1.76 %** | -2.49 % | -1.68 % |
| heldout/bigint_rsa | 5737.5 / 5732.2 | -0.09 % | 1124.1 / 1118.1 | -0.53 % | -0.06 % | -0.46 % |
| heldout/dijkstra_routes | 2980.8 / 2970.6 | -0.34 % | 1190.4 / 1235.3 | +3.77 % (+0.57 %) | -0.19 % | -0.36 % |
| heldout/forth_vm | 3843.7 / 3841.0 | -0.07 % | 813.5 / 801.1 | -1.52 % | -0.06 % | +1.63 % (+0.20 %) |
| heldout/prototype_objects | 3645.5 / 3631.8 | -0.37 % | 2434.1 / 2373.7 | -2.48 % | -0.49 % | +1.39 % |
| heldout/roguelike_turns | 2781.8 / 2761.9 | -0.72 % | 851.1 / 837.5 | -1.60 % | -0.41 % | -0.44 % |
| heldout/scheme_interp | 5879.5 / 5872.5 | -0.12 % | 1502.3 / 1487.7 | -0.98 % | -0.05 % | -0.89 % |
| dev/lua_parser | 4603.3 / 4587.1 | -0.35 % | 1295.4 / 1278.9 | -1.27 % | -0.32 % | -1.80 % |
| dev/oop_mixins | 3995.1 / 3980.3 | -0.37 % | 1449.7 / 1470.2 | +1.42 % (-1.34 %) | -0.36 % | +3.21 % (-0.03 %) |
| dev/particles_physics | 1360.9 / 1352.7 | -0.60 % | 408.6 / 404.5 | -1.00 % | -0.55 % | -1.48 % |
| dev/regex_nfa | 3599.3 / 3595.8 | -0.10 % | 694.4 / 674.1 | -2.93 % | -0.03 % | +0.31 % |
| corpus/classic_vector | 2037.7 / 2031.5 | -0.31 % | 475.9 / 479.9 | +0.84 % | -0.22 % | **+4.68 % (+5.06 %)** |
| lib/fmt_s | 26.2 / 25.9 | -1.13 % | 7.0 / 7.2 | +1.92 % (-1.52 %) | -1.14 % | +1.81 % (-1.86 %) |
| **geomean (31)** | | **-0.81 %** | | **-0.95 %** | **-0.76 %** | **-0.57 %** |

The trace code is the same in both binaries: `RUSTYNAIL_JIT_DUMP` of base and final (every `trace-N.txt` with addresses normalised, every trace and
tier-1 code size), 5.5: raytrace (41 traces), cd (231), richards (33), towers (27), fmt_s (2), deltablue (47), json (57), lru_cache (81), classic_vector
(17), havlak (28): **SAME** on all ten.

### 49.5 Bar, misses, and why the goal was not reached

**Bar.** Instructions: no program worse in either version (every program -0.03 .. -3.8 %). Geomean better in both versions, instructions and cycles.
Cycles: **one miss**, 5.4 `corpus/classic_vector` +5.1 % (min of 11) at -0.22 % instructions. It is code layout, not a lever: the final 5.4 binary
runs it at 671-675 Mcyc with the levers **off** (`cspeed=0`) as well as on, against 638-641 for the base binary in either setting, with the same
trace dumps; L1 instruction-cache misses 4.0 -> 7.3 M (the same kind of effect as `regex_nfa` in section 43). The other first-pass outliers (5.5
dijkstra_routes +3.8 %, 5.4 oop_mixins +3.2 %, storage +2.0 %, fmt_s +1.8 / +1.9 %) were noise: min of 11 gives +0.6 %, -0.0 %, -0.0 %, -1.5 / -1.9 %.

**Goal missed: 2-3x cheaper compiles and double-digit total cycles on raytrace / cd / richards / towers / fmt_s.** Measured: compiles 7-11 % cheaper
(instructions; 7-9 % in compile time), total cycles cd -4.2 %, deltablue -2.8 %, towers -2.4 %, richards -2.3 %, raytrace -1.8 %, fmt_s about -1.5 %.
Why, from the numbers above:

1. **The planned structural levers change the code.** A single backward pass that allocates while it emits (LuaJIT's `lj_asm`) makes different
   allocation decisions than the current allocator, so its code cannot be byte-identical; the same holds for dropping or lazily building tier-1 code
   for traced prototypes (the exit cells start at tier-1 landing addresses, the dispatch goes through tier 1) and for delta-encoded or deduplicated
   snapshots (exit numbering, stubs and counters follow the snapshot table; the recorder already shares one snapshot between the guards of a bytecode).
   Under the identical-code rule those are out of scope; they are the next step, each as its own knob-gated change with its own soundness argument
   and the ordinary code-changing bar. Upper bounds from the profile: tier-1 compiles are 3.5 / 3.6 / 6.2 / 2.0 / 10.1 % of all instructions (raytrace
   / cd / richards / towers / fmt_s); the allocator 3.4 / 3.7 / 3.1 / 1.8 / 0.5 %.
2. **What is left is linear in the IR volume.** After the three non-linear costs were removed (register-mask scans, sinking's snapshot scan, the
   per-byte encoder), the profile has no function above 5 % of the compile; every phase walks the snapshot maps (one snapshot of 10-60 entries per
   guard-bearing bytecode) and the instructions. Halving that needs fewer or smaller snapshots, i.e. item 1.
3. **Allocation was a smaller bucket than estimated.** malloc / free / realloc / `finish_grow` are about 5 % of the compile instructions with frame-
   pointer call chains (the study's 16-22 % came from skidded samples and counted the internals of a stripped libc); presizing the large vectors and
   dropping the recording copy took the visible part. A per-trace arena would save at most that 5 %.
4. **Compile instructions are a third of the total at most** (32-34 % on raytrace / cd, 29 % richards, 18 % towers, 15 % fmt_s, IBS). Even a 2x
   cheaper compile would give about -15 % instructions on cd and -9 % on towers.

Not built (measured or reasoned above, no prototype): the per-trace arena (bounded by about 5 % of the compile), a table-driven one-pass encoder
(the remaining encoder cost is about 100 instructions per micro-op, a third of it the dispatch), scratch buffers for the loop pass's per-snapshot
vectors (about 8 % of the passes, 0.4 % of cd), a bound on the tier-1 lowering's 256-slot type scan per pc (0.1 % of richards).

### 49.6 Gates and what was not run

* Each of the four code commits: `GATE_CPUS=2-15 scripts/gate-light.sh cspeed_ --fast --base HEAD~1`: all steps PASS (fmt, mutant map, clippy 5.5 /
  5.5-jit / 5.4, core 5.5 / 5.4, trace suites 5.5 169-187 s / 5.4 161-172 s, `vm_corpus`, jit-diff 1-50, the `cspeed_` mutant group: 1, 2, 3, 4
  mutants caught). This documentation commit: the same gate.
* Final binaries (5.5 and 5.4): `JITDIFF_MODES="off tr trdef" scripts/jit-diff.sh 1 40`, default limits and `lockstep=1,ccheck=1`: 40 seeds, 0 diffs,
  0 timeouts, both versions; the official suite (`scripts/jit-official.sh`, `RUSTYNAIL_JIT=always RUSTYNAIL_JIT_TRACE=always`): OK on 5.5 and 5.4, and
  OK on 5.5 with `ccheck=1`; `ccheck=1` on raytrace, cd, richards, towers, fmt_s, deltablue, classic_vector, lua_parser, both versions: 3-294 compiles
  compared per program, 0 differ. GC stress 1 / 2 run in the regression test (the programs that must trace).
* Not run (by instruction): `gate-heavy.sh`, the full mutant runner, the oracle suites, PGO builds, the all-223 benchmark, `bench-fast.sh`.

## 50. Direct side links for traces that are not lazy-aware, register passing, carried sunk tables (knob `slink`; 2026-11-24, base c31b7d9)

Task: cause C2 (b) / (c) of docs/STUDY_WHY_GAP.md: an exit with a side trace attached runs its stub (exit counter, slot stores, budget word, cell
load, `jmp *`), the side trace reloads the slots; sunk tables are materialised at the exit. Goal (A) register-passing side links, (B) sunk tables carried
into the side trace. **Result: (A) is built in two forms behind the knob `slink`. The plain link (bit 1: the exit's branches jump straight into the side
trace, which still receives the exit map through the stack; the exit stays counted) is the default (`SLINK_PROCESS = 2`): -0.50 % instructions /
-0.81 % cycles geomean over the 30-program sample, -0.6 .. -2.7 % instructions on the exit-heavy programs, no program above +0.08 % instructions. The register-passing link (bit 0:
the side trace takes the exit map's registers) is built, sound and tested, and loses (the side trace's snapshots become dense); it stays off. (B) is not
built: its measured ceiling and the blocker are in 50.6. The study's estimates (-13 % classic_vector, -13 % cd, -7 % lua_lexer, -6 % raytrace) are missed
by a wide margin: a link saves 3-7 instructions per exit; classic_vector's 1080 instructions per exit are materialisation (50.6).**

### 50.1 Which exits there are (measured before building)

`RUSTYNAIL_JIT_EXITDUMP=1` at c31b7d9 (release `rn-cli`, seed 0, default size): every hot exit with a side trace in lua_lexer, cd, raytrace and richards is
an exit **inside inlined frames** (1-3 frames; lua_lexer's ten hottest: 0.24-0.75 M exits each, all with a side trace attached). R1's carry path (16, half A)
takes only frameless exits whose family root offers a third entry, so it reached none of them: `regcarry=2` (carry without the root condition) changed
nothing (lua_lexer 3331.3 -> 3322.0 M instructions, the others within 0.1 % or worse: raytrace +2.5 %). classic_vector's hot exits are frameful **with sunk
tables** (`materialised 555850` for 301 k exits; metamethod frames, so the sunk-frame stub does not apply and they go through the Rust handler). Instruction
share of the exit stubs (`TRACE-n-STUB` symbols, `instructions:u` sampling): lua_lexer 5.6 % (25.6 instructions per exit, slot stores included), cd 3.2 %,
richards 3.7 %, classic_vector 0.75 % (its exit cost is in `materialise` / `exit_entry`).

### 50.2 As built

Knob `slink` (`RUSTYNAIL_JIT_TRACE_LIMITS`; `SLINK_PROCESS` = 2): 0 = off, bit 1 = plain link, bit 0 = carry link (bit 1 wins when both are set).

* **Which exits** (`Cx::stubs_and_tail`, `DeferInfo::dl_ok`, now also for traces that are not lazy-aware): the exit's stub is deferred (S1) and an inline
  stub that continues through its cell, frameless or with a frame cell (the frames stay virtual in the side trace, S1); not a pre-roll site, a chain site or
  an entry check; no sunk tables, no `top`, no `RETF` segment pops; the budget in `ra::BUDGET_GPR`; and (new) the trace did not allocate `r14` (its stubs
  reload the constants pointer before their jump; a link would skip that). E6 keeps the lazy-aware families; `slink` takes every other trace
  (`cache::slink_exit_ok`).
* **Plain link (bit 1).** The side trace attached to such an exit gets E6's link entry (36.3) unchanged: the parent's slot stores of the exit (the slot part of
  its inline stub, same registers, same tag elision), `add vb, refund`, `jmp` behind the plain entry's budget load (`cache::dlink_source` accepts a non-lazy
  parent under bit 1). `cache::install` re-points the exit's patchable `nop` and every branch to it or to its assembled stub (`dlink_attach`), and the flush
  of the side trace writes the saved bytes back while the parent lives (`dl_saved`, 36.5). Per linked exit the counter's divert test, the budget word
  store and the side trace's reload (4), the cell load and indirect jump (3) are gone; the slot stores stay. **Unlike E6 the link keeps counting the exit**
  (`mov r7, &ctr; add dword [r7], n`, `n` = 2 for the double-counting frame-cell stubs, `DlinkIn::count`, `cache::slink_count`): the ineffective-trace rule
  reads the body exits of a root (`n` of `cache::side_ratio_applies`: a loop whose body exits as often as its pre-roll completes iterations between the
  hops); without the count `tr_core::mixed_string_loops_are_not_flushed_as_ineffective` flushed `n5_cliff_shape` as ineffective twice (mutant
  `slink_count_skip`). The first version without the count measured -0.57 % instructions on the sample; the count costs about 1 instruction per linked exit.
* **Carry link (bit 0).** The side recording starts with the exit map's register / spill values as R1 carried loads (`SLOAD_CARRIED`; `cache::slink_carry`):
  no root third entry is needed and the frames may be virtual (`Rec::begin` defines the carried slots up to the innermost frame's extent). Entries that cannot
  be carried (constants, function slots of the virtual frames, nil / booleans, strings of dynamic variant, `V7` / `V10`) are stored by the link entry as the
  stub would; carried entries whose tag the stack lacks are stored (tag and payload) as by R1's stub carry path. The link entry (`DlinkIn::carry`) is: those
  stores, `add vb, refund`, `jmp` to the carry entry (the parallel move into the side trace's allocation, hinted to the parent's registers). The exit's stub
  has no carry path for such a side trace (`TraceIr::carry_link`, `TraceInstall::carry_link`: its carry cell is not set); before the attach, or after an
  unlink, the stub stores the whole map and the plain entry loads the carried slots. A frameful side trace gets a carry entry also when it needs its own R2
  room checks (they sit in the body behind both entries and save the two registers they use); the room condition of `carry_entry` stays for frameless ones.
* `mod::carry_demote` is linear now (it scanned every snapshot per carried load: `pipeline_front` was 4.5 % of cd's instructions with bit 0), and every entry
  of a trace with `RETF` segments stores the segment oracle's `CallInfo` (`+verify`; the link entries skipped it and `seg_oracle_entry_func` read a null slot).

### 50.3 Soundness

The plain link is E6's link (36.5) applied to traces outside a lazy region; the arguments carry over, with what differs:

* **Same state, same destination.** A linked exit continues where the stub went: the stub of such an exit (frameless inline stub, or the attached path of
  `mat_stub`, which is the counter, the slot stores, the budget and the cell jump) does nothing else once a side trace is attached; the link makes the same
  stores from the same registers and hands the budget over in the register. Nothing of a lazy region applies (no `lazy_root`, no leave), and a side trace
  books no entry (`head == 0`), so the plain entry does nothing between its start and the join point but the budget load and the segment oracle (`+verify`),
  which the link entry now stores too.
* **Counting.** A plain-linked exit is counted by the link exactly as by its stub (+1, or +2 for a double-counting frame-cell stub); the stub's divert at
  `hot - 1` is not needed (the side trace exists). A carry link into a side trace that R1's stub carry path also enters is not counted (that path parks the
  counter, as before). The sites whose counters drive other policy in the stub itself (pre-roll: Q2; chain sites: P3; entry checks) are never linked.
  `exits` therefore keeps its meaning (classic_vector, cd, lua_lexer, raytrace, richards, towers, deltablue: identical exit counts with and without the knob).
* **Patching lifetime, flushes, multiple parents.** Unchanged from 36.5 (install from the recorder, unpatch at the flush of the side trace before its block
  can be released, nothing written when the parent goes first). A side trace has exactly one parent exit; a side trace of a side trace is linked to its own
  parent's exit like any other.
* **Carry link.** R1's argument (16, half A): behind a carry entry the stack is stale exactly for the carried slots, every snapshot of the side trace (the
  entry snapshot included) and its writeback name them, nothing between an entry and an exit can observe the stack (TR1: no GC step, hook, error, yield or
  stack reallocation on a trace). New: (1) the entries that are not carried are stored by the link entry (mutant `slink_store_skip`), so the stack holds them
  as after the stub; (2) carried entries whose tag the stack does not provably hold get their tag stored (`slink_tag_skip`); (3) the refund is the exit's
  (`slink_refund_off`); (4) frameful: the virtual frames' function slots are always stored (constants, not carried), and the recorder reads them from the stack
  before the carried loads; the R2 room checks save `V0` / `V1`, the only registers they write; (5) GC: the carried values are live in registers across no
  safepoint (there is none on a trace); write barriers in the side trace see the same objects. Floats travel in XMM registers (`CarrySrc::X`), integers and
  references in GPRs, spilled values from the parent's spill slot (one per-state area; the moves read every source before writing a destination).

### 50.4 Measured (release `rn-cli`, one binary, `slink=0` / `1` / `2`; `RUSTYNAIL_SEED=0`, `taskset -c 4`; instructions of one run, cycles min of 3)

| program | exits | linked (bit 1) | instr M 0 / 2 | delta | cycles M 0 / 2 (min of 3) | instr saved per linked exit | bit 0 instr M (uncounted links) | bit 0 per exit |
|---|---:|---:|---|---:|---|---:|---:|---:|
| corpus/classic_vector | 301 297 | 52 774 | 2031.35 / 2031.00 | -0.02 % | 469.3 / 473.9 | 6.6 | 2032.02 | +24 (worse) |
| awfy/cd | 1 589 999 | 1 030 505 | 694.74 / 690.38 | -0.63 % | 245.9 / 245.5 | 4.2 | 699.43 | +7.5 (worse) |
| corpus/lua_lexer | 7 281 305 | 4 395 317 | 3331.34 / 3305.13 | -0.79 % | 823.0 / 821.3 | 6.0 | 3311.67 | 4.9 |
| apps/raytrace | 255 623 | 101 768 | 295.18 / 294.87 | -0.11 % | 97.2 / 98.7 (min of 7: 94.9 / 95.3) | 3.0 | 296.95 | +17.5 (worse) |
| awfy/richards | 278 617 | 258 446 | 168.73 / 167.22 | -0.89 % | 59.6 / 59.2 | 5.8 | 168.32 | 1.9 |
| awfy/towers | 147 164 | 0 | 170.57 / 170.55 | -0.01 % | 47.6 / 46.7 | - (a lazy family: E6 links it already) | 170.57 | - |
| awfy/deltablue | 138 455 | 32 898 | 310.79 / 310.79 | 0.00 % | 106.0 / 107.0 | 0.0 | 311.28 | +16 (worse) |

(Linked = the exits that took the link: the exit count of the build without the count, 50.2.) Per-exit cost of the stub path before and after: lua_lexer's stub code was 25.6 instructions per exit (5.6 % of 3331 M over 7.28 M exits); a linked exit
saves 6.0 of them (the counter's test, the budget word and the cell; 7.0 before the count was added); the slot stores (and, in the side trace, the slot
loads) remain. Stub-code share with bit 1 (uncounted build): lua_lexer 5.6 -> 3.2 %, cd 3.2 -> 2.2 %, richards 3.7 -> 1.9 %. The remaining exits of classic_vector and cd are mostly sunk-table exits (not linkable, 50.6).

**30-program sample** (the 7 above, awfy havlak / json / list / nbody / permute / queens / bounce / storage, corpus astar_jumper / class_middleclass /
containers_lru_heap / json_rxi / markdown_conv / penlight_mix / parser_combinator, programs binary_trees / fib / heapsort / oop_methods / metamethods /
string_ops, apps expr_lang / events; `slink=0` against `slink=2`, interleaved per program, final code): **geomean -0.50 % instructions, -0.81 %
cycles**. Largest gains: awfy/list -2.67 %, programs/heapsort -1.83 % / -2.0 %, awfy/bounce -1.52 % / -1.7 %, class_middleclass -1.35 % / -2.1 %,
awfy/queens -1.22 %, apps/expr_lang -1.17 %, corpus/astar_jumper -1.12 % / -2.8 %, awfy/nbody -0.95 % / -1.2 %. Worst instructions:
corpus/containers_lru_heap +0.08 % (cycles -1.6 %); every other program at or below +0.00 %. Cycles above +2 % in the batch were at equal instructions;
re-run min of 7 interleaved: binary_trees +4.64 -> -4.34 %, fib +2.59 -> -1.04 %, json_rxi +3.04 -> +1.04 %, raytrace +1.49 -> +0.39 %, apps/events +2.09
-> +2.77 % (one link attached; three further rounds of min of 7 each: 168.8 / 165.2 / 166.9 against 165.4 / 166.0 / 162.9 Mcyc, min over all 21 runs
+1.4 %, at +0.004 % instructions). Bar (no program above +1 % instructions / +2 % cycles, geomean not worse): met, apps/events at +1.4 .. +2.8 %
cycles depending on the round is the closest. The uncounted first version measured -0.57 % / -0.44 % on the same sample.

**Formation lottery** (`RUSTYNAIL_JIT_TRACE_HOT` 248 / 252 / 256 / 260 / 264, instructions, `slink=0` -> `2`): classic_vector -0.02 .. -0.11 %, cd -0.55 ..
-0.89 %, lua_lexer -0.78 .. -1.57 %, raytrace -0.13 .. -0.18 %, richards -1.07 .. -1.20 %, towers -0.01 .. +0.01 %, deltablue -0.04 .. +0.05 %. The
thresholds move the totals by up to 17 % (lua_lexer 3331 M at 256, 3914 M at 260; richards 169 / 203 M at 256 / 248) with or without the knob; the knob's
effect has the same sign at every threshold.

### 50.5 Why register passing (bit 0) loses

A carried slot is a definition of the side trace (R1): every snapshot of the side trace names it, so every exit of the side trace that is not itself linked
stores it, where after a plain link the stack holds it and the snapshot leaves it out. What the carry link saves per exit (the stores of the carried values
in the link, their loads in the side trace: 1-4 values in lua_lexer's side traces) is paid back at the later exits of the side trace and by the longer live
ranges from the start of the side trace (spilled carried values: `allocation [(.., None, [6]), ..]` in the `carry-in` lines of `RUSTYNAIL_JIT_VERBOSE=2`).
Measured against off: cd +0.68 % instructions (+1.5 % against the plain link), raytrace +0.60 %, deltablue +0.16 %; lua_lexer -0.59 % (plain link -0.93 %).
A first version was +4.6 % on cd because `carry_demote` was quadratic (fixed above). LuaJIT pays the same dense restore at its side exits but links almost
every one of them; ours still leave through stubs to tier 1 often (cd: 0.56 M exits remain with the plain link). Making the carry link pay would take carried
values that the side trace's snapshots leave out (a stale flag per side trace, as R1's half B `HeadData::stale` for roots) or the plain link's stores together
with the registers (saves only the side trace's slot loads, about one instruction per carried slot); neither was built.

### 50.6 (B) Carried sunk tables: not built; the measured ceiling and the blocker

* **Ceiling.** Instruction share of the exit path of exits with sunk tables (`materialise`, `jit_tr_mat`, `exit_entry`, `restore`, `restore_chain`; profiles
  at `slink=2`): classic_vector 13.1 % (+ `setnodevector` 3.4 %, part of it tier 1), cd 13.0 % (+ 3.5 %). classic_vector: 301 k exits, 1.85 sunk tables
  each, about 1000 instructions per exit, through the Rust handler (its frames are metamethod frames: `exit::sunk_frame_stub_ok` excludes them). With carried
  sunk tables the side trace still allocates the tables that escape in it (in `Spring:apply`'s side traces the force vectors are stored into `self.force`;
  `d` and the intermediate `f` do not escape), so the ceiling is an upper bound; the study's sinking-off experiment (+13.5 % classic_vector, -2.2 % cd)
  brackets it from the other side.
* **Blocker.** A replayed allocation must be re-emitted at the start of the side trace (LuaJIT's `snap_replay`): `TNEW` and the field stores, whose values come
  from the parent's exit (registers and spill slots on a link; on the paths through the stub or the handler the exit area, which `exit::sunk_save_ops` and the
  shared exit routine fill). Our `TNEW` is a class-A allocation with a guard (`NE %tnew, 0`: the helper refuses when a collector step is due or the
  allocation would fail), and its exit re-executes the `NEWTABLE` in tier 1. At the start of a side trace there is no `NEWTABLE` to re-execute: the snapshot
  of that guard must describe the table as *sunk* (materialise it from the field values) even when the side trace's own sinking keeps the `TNEW` because the
  table escapes later in the side trace. Sinking decides sunk-ness per allocation, not per snapshot; and the recorder's model adopts the real table the VM
  made at a `NEWTABLE` (`Rec::adopt_new_table`), which a replay at the trace start does not have (the handler materialised one; the replay would have to
  adopt it as the trace's own). Both need a design of their own (per-snapshot sunk descriptions for replayed allocations, a replay mode of the recorder's
  table model, the exit area as an entry location for field values, the double materialisation and the virtual-byte accounting on unlinked paths). Not
  prototyped within this task's time box.
* **A cheaper step found on the way:** classic_vector's sunk-table exits go through the generic stub and the Rust handler only because their frames are
  metamethod frames (`sunk_frame_stub_ok`: `!f.mm`); with a side trace attached the frames stay virtual and need no rebuild. A sunk-frame stub for metamethod
  frames that diverts to the handler only when the cell is 0 (and after a policy flush inside the helper) would remove `exit_entry` / `restore` /
  `restore_chain` (about 6 % of classic_vector's instructions), not the materialisation. Not built.

### 50.7 Tests, mutants, gates, what was not run

* `tr_core::s50_slink_programs`: the programs of `tests/tr_core/s50_progs.rs` (`s50_frame_sides`: methods inlined in a loop root whose branches flip, every
  value class live; `s50_spill_values`: more live values than registers, floats; `s50_observers`: an error raised behind a linked exit with
  `debug.getlocal` of the frame, a count hook installed from inside, collections in the loop; `s50_coro_flush`: the loop inside a coroutine that yields
  between batches, a callee replaced by a `load`ed function between rounds so that side traces are flushed under live parents and their links undone) and
  `r1_nbody_pairs`, `r1_type_flip`, `r1_error_locals`, `r1_many_slots`, `s1_frame_sides`, `e6_01_dlink`, in `always` / `on2` x `slink` 0 / 1 / 2, `+verify`,
  `+exitstorm=3 +verify`, budgets 3 / 17, GC stress 1 / 2 in both collector modes, `RUSTYNAIL_JIT_STRESS=stackmove` with `+tracemove`, `regcarry=0` / `2`,
  `lazystub=0` (no patch sites: asserted no links), `sidemin=0`; with the oracle feature the instruction-count identity (budgets none / 3 / 17). Asserts
  links in every `s50_*` program but `s50_spill_values` with a bit set and none with `slink=0`. Lua 5.5 and 5.4 (the gate runs the suite on both).
  Existing tests adapted to the default: `e6_dlink_programs` runs its `dlink` jobs with `slink=0` (it asserts that the links it counts are E6's).
* Mutants (registry 859 -> 863): `slink_store_skip`, `slink_tag_skip`, `slink_refund_off` (caught by `s50_slink_programs`), `slink_count_skip` (caught by
  `mixed_string_loops_are_not_flushed_as_ineffective`); `s50_slink_programs` is also a detector of `dlink_store_skip` and `dlink_refund_off`.
* Gates. Commit 9ce7ff8 (knob, default off): `GATE_CPUS=2-15 scripts/gate-light.sh slink_,dlink_ --fast --base HEAD~1`: every step PASS (fmt, mutant map,
  clippy 5.5 / 5.5-jit / 5.4, core 5.5 / 5.4, trace suites 5.5 167 s / 5.4 156 s, `vm_corpus`, jit-diff 1-50, the mutant group); the first run failed on the
  frozen registry count and a stale mutant map, both fixed in the commit. Commit with the default on: `gate-light.sh slink_,dlink_,lazystub,carry_ --fast
  --base HEAD~1`: every step PASS, mutant group 24 ran, 24 caught. Its first run (default on, links not counted) failed four trace tests
  (`e6_dlink_programs`: links with `dlink=0` that were `slink`'s; `s1_lazystub_programs` and `bf_stitch0_programs`: exit counts that compare two
  configurations; `mixed_string_loops_are_not_flushed_as_ineffective`: the ineffective rule, 50.2): the count and the test adaptation are the answer.
* Final binaries (5.5 and 5.4 release `rn-cli`): `JITDIFF_MODES="off tr trdef" scripts/jit-diff.sh 1 40`: 40 seeds, 0 diffs, 0 timeouts, both versions;
  the official suite (`scripts/jit-official.sh`, `RUSTYNAIL_JIT=always RUSTYNAIL_JIT_TRACE=always`): OK on 5.5 and 5.4.
* Not run (by instruction): `gate-heavy.sh`, the full mutant runner, the oracle-feature suites (the identity jobs of `s50_slink_programs` ran only inside
  the mutant runner's oracle build), PGO builds, the all-223 benchmark; no 5.4 performance numbers.

## 51. Trace register allocator policy (knob `rapol`; 2026-11-24, base 7f153aa)

Task: cause C3 (spills) of docs/STUDY_WHY_GAP.md: the allocator's policy, not the register count, as the lever; measure the spill traffic first,
then build levers in the order of the measured classes. **Result: three levers behind one knob (`rapol`, bits of `ra::RAPOL_*`); bits 0 and 2
are the default (`RAPOL_PROCESS = 5`), bit 1 is built, sound and tested but off. Measured `rapol=0` -> `5` (one binary) over a 33-program sample:
geomean -1.40 % instructions / -1.22 % cycles (5.5), -1.26 % instructions (5.4); byte_multi -8.7 % / -5.3 %, fmt_s -11.0 % / -9.1 %,
oop_methods -9.6 % / -13.1 %, heapsort -3.8 % / +0.2 %; no program above +0.04 % instructions or +1.7 % cycles. The study's estimates (byte_multi
-25 % instructions, towers -9 %, lua_lexer -5 %) are missed: -8.7 %, -0.9 %, -0.4 %. The straight-line programs' spill classes are small
(1-6 % of their instructions in all, 51.1) and none of them is one big class. No backward-linear-scan rewrite: the existing allocator already is
one (LuaJIT's scheme, 7.2); what the measurement found are policy choices, each fixable in place.**

### 51.1 Step 0: what the allocator's spill code costs (measured before building)

Method: a scratch build (not committed: a diagnostic in `asm.rs` / `mod.rs` / `ra.rs`) binds a label before and after every piece the assembler
emits for the allocation of an IR instruction (the reloads in front of it, the instruction, its spill stores, its reloads behind it), around the
back-edge and loop-entry moves and around the save / reload of each helper-call site, and writes the label offsets and the allocation (with the
value of every reload) into the `RUSTYNAIL_JIT_DUMP` text; the machine code is byte-identical with and without the labels (addresses aside).
AMD IBS op sampling in the same runs (`perf record -e ibs_op/cnt_ctl=1,swfilt=1/u -c 20000`, precise IPs, CPU 4, `RUSTYNAIL_SEED=0`,
`RUSTYNAIL_JIT_PERFMAP=2`): every sample in a trace is mapped to its label region and classified (spill stores by why the value has a slot,
reloads by the reloaded value, `mov r,r` / `movaps` / `movabs imm64` inside an instruction by pattern). Runs repeated to 6 000-39 000 samples
(byte_multi 15, towers 15, richards 10, raytrace 8, fmt_s 30, classic_vector 3, lua_lexer 2); shares of all sampled ops of the program
(about all instructions; +-0.1-0.3 points).

| class (% of all instructions) | byte_multi | fmt_s | towers | richards | raytrace | classic_vector | lua_lexer |
|---|---:|---:|---:|---:|---:|---:|---:|
| trace code, all | 87.9 | 33.2 | 50.4 | 37.5 | 17.1 | 29.4 | 45.4 |
| eviction: spill store at the definition | 6.52 (loop body) | 1.37 (body) | 0.73 | 0.39 | 0.08 | 0.19 | 0.59 |
| eviction: reload behind the evicting instruction | 10.64 (body) | 4.21 (body) | 0.55 | 1.22 | 0.15 | 0.21 | 0.17 |
| frame-slot loads (`SLOAD`) spilled and reloaded | 0 | 0.02 | 1.47 | 0.20 | 0.20 | 0.22 | 2.41 (R1 lead run) |
| snapshot-only slot stores (live in a snapshot after the last use, no free register) | 2.30 (body) | 1.20 (body) | 0.83 | 0.20 | 0.03 | 0.25 | 0.28 |
| loop-carried values through memory at the back-edge | 4.72 | 1.27 | 0.07 | 0 | 0 | 0.02 | 0.03 |
| loop invariants reloaded from slot homes | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| helper-call save / reload of live registers | 0 | 0 | 0 | 0 | 0.31 (`TNEW`) | 2.77 (`TNEW`, `NEWREF`) | 1.71 (`LIB`) |
| register moves inside instructions (two-address, `movaps`) | 6.95 | 2.80 | 0.78 | 0.09 | 0.53 | 0.50 | 0.58 |
| 64-bit constants (`movabs` into `V7`; not allocator work) | 0 | 0 | 3.46 | 2.97 | 1.22 | 2.45 | 1.25 |

Upper bound of the allocator classes (constants excluded): byte_multi 31 %, fmt_s 11 %, towers 4.4 %, richards 2.1 %, raytrace 1.3 %,
classic_vector 4.0 %, lua_lexer 5.8 % of all instructions. What it says:

* **Two kinds of programs.** byte_multi and fmt_s run one loop trace whose body evicts: 5 body registers (8 minus the budget register `V5` minus
  two array-base homes). The victim was the value with the lowest ref: in byte_multi the string `S[k]` that the next three `string.byte` reads
  use again, so four reloads per iteration. Every other program runs straight-line traces (no `LOOP` in the hot traces): no class above 2.8 %.
* lua_lexer's largest class is the leading `SLOAD` run of an R1 root (24 frame slots loaded and stored into the spill slots that R1's carry entry
  fills): not removable by rematerialising from the frame (the stack is stale behind a carry entry, section 16).
* Call-site save / reload runs only at calls without an inline fast path (`TNEW`, `NEWREF`, the helper `LIB` kinds); `TBAR`, `TLEN`,
  `string.byte` keep it on their cold path.
* No loop invariant lives in a slot home in the seven programs: the home policy (`max_gpr_homes`) is not a lever here.

### 51.2 The levers (knob `rapol` of `RUSTYNAIL_JIT_TRACE_LIMITS`; `cache::RAPOL_PROCESS = 5`; `ra::RaOpts::pol`)

| bit | lever | what changed | default | commit |
|---|---|---|---|---|
| 1 `RAPOL_VICTIM` | Belady victims in loop bodies | inside a loop body the victim is the value whose previous read (going up from the evicting instruction) is furthest away; the reads are indexed once per trace at the first body eviction (flat array, counting sort); straight-line code keeps the lowest-ref choice | on | 1 |
| 2 `RAPOL_CALLSPILL` | spill across calls | at a call that is always made (`CALLN`, `TNEW`, `NEWREF`, `TSET`, `SNEW`, `CONCAT`, `STRTO`, `LIB` kinds without a fast path) every value in a caller-saved register is evicted: one store at its definition, a reload behind a call only when it is read before the next call; loop homes and values pinned by an open snapshot keep the save | off | 2 |
| 4 `RAPOL_RBP` | `rbp` allocatable | `V9` (`rbp`: callee-saved, read only by the thunk's frame-pointer chain; the register of `xregs` bit 0) is allocatable in every trace, last in the free order; calls keep it | on | 3 |

Tried and dropped (measured, scratch): Belady in straight-line traces too (richards +0.5 %, raytrace +0.2-0.4 % instructions: the furthest-read
victim often has no slot yet, so the eviction adds a store, and lua_lexer changed formation); a PHI bias on top of Belady (fmt_s, heapsort worse);
preferring victims that already have a slot (no effect); `rbp` only for loop traces whose body spills (a second allocation; its compile cost ate the
gain on cd / raytrace); a hint putting values that cross an always-made call into `rbp` (cd, raytrace slightly worse than plain `rbp`). Not built:
rematerialising `SLOAD` values from the frame (the measured class is R1 lead runs, unsound there), constant registers (the `movabs` class is in
straight-line traces), two-address forms of the in-instruction moves (not examined further).

Why `rbp` alone is now a default although `xregs=1` was not (docs/PERF.md BUNDLE-H): the gate failure then was `rec_loop` +5.9 % cycles at fewer
instructions, undiagnosed. On this base `rec_loop` is -1.7 % instructions / -2.9 % cycles and heapsort +0.2 % cycles at -3.8 % instructions
(min of 3; the 0-vs-7 pass, taken while a gate ran on other CPUs, read heapsort +23 % cycles: noise, not reproduced in the clean pass).

### 51.3 Soundness and tests

* Every lever only changes which valid allocation is produced: a victim choice, extra evictions (the existing eviction: store at the definition,
  reload behind the instruction, TR27 pins respected) and one more allocatable register (the `xregs` bit 0 path, tested since P2 bundle 1).
* `RAPOL_CALLSPILL`: an evicted value leaves the call site's saved set because its register is free at the call (the reload follows the
  instruction); pinned values (their register is in an exit map on both sides of the call) and loop homes are never evicted. Mutant
  `callspill_no_reload` (the value leaves the saved set without a reload; registry 863 -> 864) is caught by `tr_ra::allocation_matches_interpreter`
  (and the straight-line variant) and `tr_core::s51_rapol_programs`.
* `tests/tr_ra.rs`: the differential allocator test runs every case with all levers off, all on, and the random seeds with a rotating lever set
  (`+randra` with `xregs` 0-3); the lowering model now destroys every caller-saved register a call site does not save (before, a wrong live set
  could not fail it).
* `tr_core::s51_rapol_programs`: `tests/tr_core/s51_progs.rs` (`s51_loop_pressure`: twelve live integers / floats in a loop body, rotations,
  invariants, type-flip exits; `s51_call_values`: values across table constructors, concatenation, `tostring`, `string.rep` in a loop and its
  side traces; `s51_phi_rotate`: swaps and three-cycles of both classes with a call in the body) and `s50_spill_values`, `r1_many_slots`,
  `r1_nbody_pairs`, under `rapol` 0 / 1 / 2 / 4 / 7 x `always` / `on2` x plain, `+verify`, budget 17; with `rapol=7` also `+exitstorm=3 +verify
  +randra=5`, budget 3, `xregs=3`, GC stress 1 / 2 in both collector modes, `RUSTYNAIL_JIT_STRESS=stackmove +tracemove`, and with the oracle
  feature the instruction-count identity (budgets none / 3 / 17). Runs in both versions' trace suites.

### 51.4 Measured (release `rn-cli`, one binary, `rapol=0` against the levers; `RUSTYNAIL_SEED=0`, `taskset -c 4`; instructions of one run, cycles min of 3 interleaved)

Sample: the 30 programs of section 50 plus lib/byte_multi, lib/fmt_s and jit-trace/rec_loop (33).

| lever step | geomean instr | geomean cycles | largest gains (instr / cycles) | worst |
|---|---:|---:|---|---|
| 0 -> 1 (`VICTIM`) | -0.29 % | -0.29 % | fmt_s -4.4 / -9.0, byte_multi -3.9 / -0.3, heapsort -1.5 / -1.0 | oop_methods **+1.04 %** instr (-4.6 % cycles: one memory op fewer, two register moves more in its back-edge cycle) |
| 1 -> 3 (`CALLSPILL`) | +0.56 % | +0.48 % | classic_vector -0.6, binary_trees -0.6, metamethods -0.4 | **lua_lexer +22.9 %** (formation, below) |
| 3 -> 7 (`RBP`) | -1.24 % | -0.37 % (pass disturbed by a gate run) | oop_methods -10.5, fmt_s -7.0, byte_multi -5.1, heapsort -2.3, bounce -1.2 | none above 0 in instructions |
| **0 -> 5 (shipped)** | **-1.40 %** | **-1.22 %** | fmt_s -11.0 / -9.1, oop_methods -9.6 / -13.1, byte_multi -8.7 / -5.3, heapsort -3.8 / +0.2, rec_loop -1.7 / -2.9, bounce -1.3 / -1.2, towers -0.9 / -3.3, deltablue -0.9 / -0.6 | instructions: storage +0.04, fib / string_ops +0.00; cycles: expr_lang +1.70, fib +1.03 |
| 0 -> 5, Lua 5.4 (instructions only) | -1.26 % | not run | fmt_s -10.9, oop_methods -9.6, byte_multi -8.7, rec_loop -1.5, deltablue -1.0 | storage +0.04 |

byte_multi: 102.7 -> 93.7 instructions per iteration (2.86 M iterations; LuaJIT 29.6). **Bar** (no program above +1 % instructions / +2 % cycles,
geomean not worse): met for the shipped `rapol=5` in 5.5 (5.4: instructions only). `VICTIM` alone misses it on oop_methods (+1.04 %
instructions), `CALLSPILL` misses it on lua_lexer; neither is shipped alone.

**Formation lottery.** Hot thresholds 248 / 252 / 256 / 260 / 264, instructions `rapol=0` -> `5`: classic_vector -0.42 / -0.46 / -0.47 / -0.42 /
-0.47 %, cd -0.13 / -0.13 / -0.13 / -0.12 / -0.11, lua_lexer -0.37 / -0.34 / -0.43 / -0.20 / -0.24, raytrace -0.13 / -0.12 / -0.15 / -0.12 / -0.12,
richards -0.40 / -0.51 / -0.51 / -0.52 / -0.77, towers -0.99 / -0.69 / -0.92 / -0.83 / -0.70, deltablue -0.89 / -0.81 / -0.89 / -0.82 / -0.87,
byte_multi -8.74 at all five: same sign everywhere. With `CALLSPILL` (bit 1) lua_lexer is -0.14 .. -0.28 % at 248 / 252 / 260 / 264 and
+22.9 % at 256: at that threshold any change of its R1 root's allocation (`carry: root e3 ... dsts` registers; seen also with straight-line Belady
and with a call hint) moves the side-trace formation to a known slow attractor (4.05-4.73 G instructions, side trace of exit 34 of trace 11 never
formed). Not chased; it is why bit 1 stays off.

**Compile cost** (`[trace] phases`, min of 3, `rapol=0` -> `5`): allocator `ra` Mtsc cd 6.22 -> 5.61, raytrace 1.94 -> 1.95, richards 1.06 ->
1.03, towers 0.68 -> 0.67, fmt_s 0.06 -> 0.06; whole pipeline cd 30.2 -> 27.6, raytrace 8.51 -> 8.46, richards 4.61 -> 4.56, towers 3.21 ->
3.25: no measurable change (the reads index of `VICTIM` is built only when a loop body evicts; a first version that built it for every trace cost
richards +0.5 % instructions).

### 51.5 Gates and what was not run

* Commit 1 (knob, `VICTIM`, tests): `GATE_CPUS=5-11,13-15 scripts/gate-light.sh --fast --base HEAD~1`: every step PASS (trace suites 5.5 196 s,
  5.4 188 s, `vm_corpus`, jit-diff 1-50) except the mutant group: 80 ran, 79 caught; the survivor `rec_inner_first_stale`
  (`tr_core::pm4_recursion_entry_counters`) also survives on the base 7f153aa (checked with `gate-mutants.sh --only rec_inner_first_stale` on a
  `git archive` of the base): pre-existing, not caused by this work (the default code is unchanged in that commit).
* Commit 2 (`CALLSPILL`): `gate-light.sh callspill_,spill_no_reload,exit_rename_ignored,cycle_stale_xmm,phi_swap --fast --base HEAD~1`: all PASS.
* Commit 3 (`RBP`): `gate-light.sh xregs_,callspill_,spill_no_reload --fast --base HEAD~1`: all PASS. Default commit (`rapol=5`):
  `gate-light.sh --fast --base HEAD~1`: every step PASS (trace suites 5.5 164 s / 5.4, `vm_corpus`, jit-diff 1-50, the planned mutant group)
  after regenerating the stale mutant map (line numbers in `cache.rs`).
* Final binaries (5.5 and 5.4 release `rn-cli`, default `rapol=5`): `JITDIFF_MODES="off tr trdef" scripts/jit-diff.sh 1 40`: 40 seeds, 0 diffs,
  0 timeouts, both versions; the official suite (`scripts/jit-official.sh`, `RUSTYNAIL_JIT=always RUSTYNAIL_JIT_TRACE=always`): OK on 5.5 and 5.4.
  With every lever (`rapol=7`, 5.5): jit-diff 1-40 `off tr` 0 diffs, official suite OK.
* Not run (by instruction): `gate-heavy.sh`, the full mutant runner, the oracle-feature suites (the identity jobs of `s51_rapol_programs` run
  only in oracle builds), PGO builds, the all-223 benchmark; 5.4 cycles.

## 52. Sunk-frame stub for metamethod frames (knob `sinkmm`) and sunk allocations replayed into side traces (knob `resink`; 2026-11-24, base 075b9f3)

Task: candidate 1 of docs/STUDY_CAUSAL_GAP.md (section 6): the steady-state gap of the allocating programs is sinking across side-trace boundaries
(LuaJIT's `snap_replay`). (1) A sunk-frame stub for exits whose frames include metamethod frames (the "cheaper next step" of 50.6); (2) part (B) of
50.6, which was not built: the side trace replays the sunk tables of its parent's exit and sinks them again. **Result: both are built, each behind
its own knob, both default on (`SINKMM_PROCESS = 1`, `RESINK_PROCESS = 1`). Final binary, both knobs off -> on: classic_vector 2029.9 -> 1308.4 M
instructions (-35.5 %), 460.3 -> 304.9 Mcyc (-33.8 %; LuaJIT 248: the ratio goes from 1.9x to 1.23x); cd 692.3 -> 648.5 M (-6.3 %), 243.1 -> 237.0 Mcyc
(-2.5 %); raytrace, havlak, richards unchanged (within +-0.2 % instructions, +-1.2 % cycles). 30-program sample: `sinkmm` geomean -0.13 % / -0.49 %,
`resink` geomean -1.58 % instructions / -1.33 % cycles, no program above the bar. The study's expectation (classic_vector -11 %, cd -7 %) is exceeded
on classic_vector (3x) and missed on cd (0.4x).**

### 52.1 Ceiling first (scratch build, not committed; `RNHACK` bits of a copy of 075b9f3, separate target dir, removed)

Bit 1 materialises every sunk table a second time (garbage) at exactly the exits that continue in an attached side trace (in `exit_entry` and
`jit_tr_mat`: the upper bound of what re-sinking can remove); bit 2 runs `restore` a second time (budget word and `nCcalls` saved around it) in the
handler for frameful exits with sunk tables (the handler part a stub would remove). Counters of the same build (release `rn-cli`, seed 0):

| program | exits | sunk-exit exits into a side trace / tables | into tier 1 / tables | handler materialisations | bit 1: Minstr / Mcyc | bit 2: Minstr / Mcyc |
|---|---:|---:|---:|---:|---:|---:|
| classic_vector | 301 311 | 235 976 / 554 797 | 521 / 1 031 | 119 873 | **+574 (+28.3 %) / +114 (+23.9 %)** | +80 (+3.9 %) / +14 (+2.9 %) |
| cd | 1 589 999 | 135 732 / 172 953 | 7 046 / 15 115 | 79 | **+96 (+13.9 %) / +22 (+9.2 %)** | 0 / 0 |
| raytrace | 256 018 | 4 162 / 4 162 | 295 / 298 | 96 | +2.6 (+0.9 %) / 0 | 0 / 0 |
| havlak | 133 925 | 0 / 0 | 9 451 / 9 451 | 16 | 0 / 0 | 0 / 0 |
| richards | 278 617 | 0 / 0 | 0 / 0 | 0 | 0 / 0 | 0 / 0 |

(Instructions of one run, cycles min of 5, `taskset -c 4`, nothing else running.) What it says before building: only classic_vector and cd have
materialisations into side traces; raytrace's +16 % and havlak's +11 % for LuaJIT's side-trace sinking come from tables our sinking does not sink at
all in the root (array parts, non-string keys, escapes the analysis does not prove dead), which this task does not change. Ceiling of the two levers
together: classic_vector ~128 Mcyc (27 %), cd ~22 Mcyc (9 %), the others 0.

### 52.2 `sinkmm`: the sunk-frame stub for metamethod frames (commit 1)

classic_vector's hot exits with sunk tables sit inside metamethod frames (`__sub` / `__mul` / `__call` constructors), which `sunk_frame_stub_ok`
excluded: every exit went through the generic stub and the handler (`exit_entry`, `restore`, `restore_chain`, `materialise`), side trace attached or
not. With `sinkmm` (`RUSTYNAIL_JIT_TRACE_LIMITS`, `cache::SINKMM_PROCESS = 1`, `AsmEnv::sink_mm`) the sunk-frame stub takes such an exit when it has a
frame cell (`exit::sunk_frame_stub_mm_ok`):

* attached path (cell != 0, a side trace with the frames virtual): count, slot stores, budget, save ops, `jit_tr_mat`, jump through the cell. Nothing
  rebuilds a metamethod frame on this path: the side trace keeps them virtual, exactly as the handler path did after unwinding;
* no side trace (cell 0): straight to the generic twin (the handler rebuilds metamethod frames: `nCcalls`, `CIST_MMRET`, `ctx`); the shared inline
  rebuild (`frame_mat_shared_ops`) knows only plain and tail frames;
* the side trace went away inside the helper (a flush deferred until `handler_leave`): `jit_tr_mat` reads the cell after its bookkeeping and, when it
  is 0 or the landing, rebuilds the frames itself (`restore_chain`, metamethod frames included) and returns 2; the stub reloads `thread.ci`, the frame
  and the constants and lands in tier 1.

Measured (one binary, `sinkmm=0` / `1`): classic_vector 2023.7 -> 1898.4 M instructions (-6.19 %), 469.7 -> 429.7 Mcyc (-8.52 %); stub calls 116 624
-> 236 187 (stub-frames 114 633 -> 234 196). cd unchanged (its sunk exits have no metamethod frames). 30-program sample geomean -0.13 % / -0.49 %;
the only line above the bar, astar_jumper +2.48 % instructions in one run, is a nondeterministic program: seven runs each give 4332..4547 M
(`sinkmm=0`) and 4335..4592 M (`sinkmm=1`), medians 4412 / 4421 (+0.2 %). Hot thresholds 248..264: classic_vector -5.1 .. -6.4 %, the others 0.

### 52.3 `resink`: sunk allocations replayed into side traces (commit 2)

The two blockers of 50.6 and how they are met:

* **The allocation guard and its snapshot.** The replay is all-or-nothing per side trace. The side recording starts with the replay (below); the sink
  pass first runs on a copy (`sink::run`): if every replayed `TNEW` is sunk again, the copy is kept and the side trace is marked `resink`; otherwise the
  whole replay is reverted before sinking (`sink::revert_replay`): each replayed `TNEW` becomes the unchecked `SLOAD` of the slot that holds it, every
  other replay instruction goes, and the parent's exit keeps materialising as before. A kept replay never has a live `TNEW` guard: sunk `TNEW`s become
  `TVIRT` and lose their guards, so no snapshot ever has to describe a table the side trace allocated as sunk. The replay's own `setmetatable` gets no
  `__gc`-absence guard: the parent's guard passed in the run that left through this exit, and no Lua code runs between the exit and the side trace's
  start (with the guard, the snapshot's guard range would span the replay of the next table and the snapshot-ambiguity rule unsank it). A
  `setmetatable` the side trace records itself on a replayed table keeps its guard (`n3b_setmt_gc_exit`: a `__gc` added after the loop is hot).
* **The recorder's table model.** The recording starts in the interpreter after the handler materialised the tables (as before): the model already
  holds those real tables (`val_of` of the slots), which have exactly the sizes, fields (insertion order) and metatable of the replay. `Rec::replay_sunk`
  makes the replayed `TNEW` stand for that table: the slots and aliases are defined to the `TNEW` (`SlotMap::def`), the lockstep values of the replay
  instructions are entered directly (the `TNEW`'s value is the model handle of the real table, `XLOAD`s the model values of its fields) and the replay
  is not evaluated (`evald` moves past it); the light model gets `ls_tnew`. The insertion planner and `presize` treat a replayed `TNEW` as not fresh
  (its layout comes from the parent's exit), the shape guard as fresh (no `HSHAPE`, like any `TNEW` of the trace).

The replay at the start of the side recording, per sunk table of the parent's exit snapshot (`exit::replay_tabs`, `cache::SideStart::replay`): `XLOAD`
of every field value and of the metatable, `TNEW` (hash operand of the `NEWTABLE`) + `NE %t, 0`, per field `NEWREF` + `NE` + `HSTORE` (+ `TBAR`),
`LIB setmetatable` + `NE`; the guards name the start snapshot, which is taken right after with the slots defined, so its sunk group carries every field
and the metatable. `XLOAD` (new op, `TOp::Xload`) loads replay word `a` of the exit area (`ExitArea::rp`, `RP_MAX = 16` words) with the field's static
type. Replayable exits: no array part, no field of dynamic string variant, at most 16 words, frameless or with virtual frames, no carried slots, not a
prelude, not under `+emurun` / `+emuverify` (the emulator has no exit area). A replayed table charges no virtual bytes (`TVIRT 0`): the parent's
`TVIRT` charged them, and the side exit that materialises it credits them back once.

The parent side: every trace compiled with the knob on gets replay cells (`cache::alloc_exit_rcells`, one word per exit, `AsmEnv::exit_rcells`).
`install` clears the exit's replay cell before the cell points at a new side trace; right after the install the pipeline sets it to the side trace's
entry when that side trace kept its replay (`cache::set_rcell`). The two materialising stubs (frameless `jit_tr_mat` stub, sunk-frame stub) do after
the slot stores, the budget and the save ops (`Asm::replay_path`): load the cell and the replay cell; if the replay cell is nonzero and equal to the
cell, copy the field values from the exit area into the replay words (`exit::replay_copy_ops`) and jump through the cell without the helper. The
handler path (`exit_entry`) does not materialise when the exit's cell equals its replay cell; after the policy it decides again: if the policy
flushed, started a recording or the cell changed, the tables are made then (before the probe, the frame still the restored one), otherwise the
replay words are filled (`exit::replay_fill`) and the exit continues in the side trace. So no path materialises a table that a side trace also
replays (the collector's byte count stays the interpreter's: `s52_resink_count`, `sink_count_stopped` under exit storms). A stale replay cell (the
side trace was flushed, the cell holds the landing again) never equals the cell. Entry checks of a side trace at a head label stay behind the replay:
the `XLOAD`s, `TVIRT`s and unchecked `SLOAD`s in front of the `BUDGET` belong to the prefix (only in front of it: a `TVIRT` behind the charge is an
allocation a failing check must not have charged), and the entry snapshot names the replayed slots (`entry_checks`); `run_fwd` keeps the flag.

Found by the tests on the way (all fixed before the commit): `run_fwd` rebuilt the IR without the `resink` flag (the side trace read replay words
nobody had written: cd raised `attempt to call a nil value`); the first entry-check prefix took `TVIRT`s behind the charge (byte counts of
`sink_count_stopped` under exit storms); guard dropping for every `setmetatable` of a replayed table instead of the replay's own (finalizers lost).

**Exactness.** Identity: a kept replay is never made twice (no exit path materialises a table its side trace replays). Metatables: the replay
sets the same metatable value (a replay word); a finalizable metatable is never sunk (the root's guard). GC: nothing allocates between the parent's
exit and the side trace's start; the replay words hold field values only in that interval (no safepoint); a table materialised at a side exit is a new
object (as the parent's materialisation was), with the interpreter's size and node layout (same `NEWTABLE` operand, fields in insertion order: `pairs`
order checked under two hash seeds). Weak tables and escapes: a store anywhere in the side trace unsinks the table and reverts the replay. Restore
from any exit, stack growth, hooks, coroutines, errors: the side trace's exits are ordinary sunk-table exits.

Measured (one binary before the last fixes of this section, `resink=0` / `1`; instructions of one run, cycles min of 3, `taskset -c 4`; the final
binary's numbers are in the summary at the top and below the table):

| program | instr M 0 / 1 | delta | Mcyc 0 / 1 | delta | materialised (0 / 1) | side traces kept / reverted |
|---|---|---:|---|---:|---|---|
| classic_vector | 1897.9 / 1308.4 | **-31.1 %** | 427.8 / 304.4 | **-28.8 %** | 555 828 / 1 204 | 11 kept |
| cd | 692.5 / 648.5 | **-6.3 %** | 243.2 / 236.7 | **-2.7 %** | 188 068 / 107 739 | 25 kept |
| raytrace | 293.8 / 293.2 | -0.2 % | 92.7 / 94.2 | +1.6 % (noise: -0.2 % in the sample) | 4 460 / 1 282 | 14 kept |
| havlak | 506.7 / 506.5 | 0.0 % | 163.4 / 157.5 | -3.6 % (lottery) | | |
| richards | 166.2 / 166.3 | 0.0 % | 57.2 / 57.6 | +0.8 % | | |

Final binary (min of 5, the three configurations interleaved): classic_vector 2029.9 / 460.3 (both off) -> 1897.9 / 422.6 (`sinkmm`) -> 1308.4 /
304.9 (`resink`), cd 692.3 / 243.1 -> 692.3 / 242.6 -> 648.5 / 237.0, raytrace 293.8 / 93.6 -> 293.8 / 93.3 -> 293.2 / 94.6, havlak 506.9 / 153.9 ->
506.8 / 154.0 -> 507.0 / 152.8, richards 166.2 / 57.5 -> 166.2 / 57.3 -> 166.3 / 56.9 (M instructions / Mcyc).

30-program sample, final binary (`resink=0` -> `1`, interleaved, min of 3): **geomean -1.58 % instructions, -1.33 % cycles**; classic_vector -31.1 /
-28.8, cd -6.3 / -2.6, astar_jumper -3.8 / -0.6 (nondeterministic, as above); worst instructions +0.02 % (havlak, richards), worst cycles at equal
instructions fib +1.92 %, expr_lang +1.46 %, string_ops +1.14 % (the build before the last fixes read fib -2.4 %, string_ops -5.2 %: noise both
ways). Bar met (no program above +1 % instructions / +2 % cycles, geomean better). Hot thresholds 248 / 252 / 256 / 260 / 264 (instructions):
classic_vector -38.4 / -31.1 / -31.1 / -38.4 / -31.1 %, cd -9.9 / -5.8 / -6.3 / -4.2 / -8.4 %, raytrace -0.2 .. -2.2 %, havlak and richards within
+-0.1 %: the same sign at every threshold (classic_vector's totals move by 15 % with the threshold with or without the knob).

**Ceiling vs achieved.** classic_vector: ceiling (bit 1 + bit 2) ~128 Mcyc; achieved `sinkmm` 40 + `resink` 123 = 163 Mcyc. Above the doubling
ceiling because removing the materialisation also removes the stub's helper call and save ops, the GC work of the garbage tables (setnodevector,
sweeping) and the store-to-load conflicts of 3.4 in the study, none of which a doubled materialisation counts in full. cd: ceiling 22 Mcyc, achieved
6.5 (0.3): 80 k of its 188 k materialisations remain, at exits whose side traces let the table escape (reverted), whose snapshots are not replayable,
or that land in tier 1.

### 52.4 Tests, mutants, gates, what was not run

* `tr_core::s52_sinkmm_programs`: `s52_mm_frames` (classic-style `__call` constructors and arithmetic metamethods inlined in loops, receivers of two
  classes, an error raised and a traceback taken inside a metamethod behind such an exit, a count hook, a coroutine, a method replaced between
  rounds) and the SINK-F programs (`sinkf_side`, `j1_sinkf`, `sink_alias_frames`), knob off / on, `always` / `on2`, `+verify`, exit storms, budgets 3 /
  17, GC stress 1 / 2 in both modes, stack-move stress, `sinkroot=2`; asserts the stub is taken.
* `tr_core::s52_resink_programs`: `s52_resink_paths` (field values of every kind, aliases, two tables per exit, escapes into an array, a weak table, an
  upvalue: reverted replays; a late branch flip: side traces of side traces; `pairs` order of a materialised table), `s52_resink_observers` (an error
  with the table live read through `debug.getlocal`, a hook installed from inside, a coroutine yield, `__index` / `__gc` on the metatable, collections
  in the loop, a callee replaced between rounds) and the programs above, knob off / on, the handler path alone (`sinkstub=0`), without `sinkmm`,
  `lazystub=0`, budgets, GC stress 1 / 2 in both collector modes, stack-move stress, two pinned hash seeds; asserts kept replays. With the default
  on, every other trace test runs with `resink=1` too.
* `tr_core::s52_resink_programs` also runs `s52_resink_count` (the collector stopped: the bytes of a loop whose temporaries are re-sunk equal the
  interpreter's, in every mode including exit storms and the handler path). Tests whose subject is the materialising frame stub itself run with
  `resink=0`: `s52_sinkmm_programs`, `j1_sinkstub_programs`, and `s1_dbl_chain` in `s1_lazystub_programs` (its chain re-order is built from the
  double-counting stubs' counts).
* Mutants (registry 864 -> 871): `sinkmm_unatt_inline` (the unattached path does not divert and the helper does not rebuild), `resink_copy_skip` (a
  replay word keeps a stale value), `resink_flag_ignored` (the stub skips the replay-cell compare), `resink_rcell_skip` (the replay cell is never set,
  the side trace reads stale words), `resink_revert_slot_off` (a reverted table is loaded from the wrong slot), `resink_setmt_guard_any` (a recorded
  `setmetatable` of a replayed table loses its `__gc` guard), `resink_handler_mat` (the handler materialises the replayed tables as well).
* Gates: commit 1 `GATE_CPUS=2-15 scripts/gate-light.sh sinkmm_,sunkframe_,sinkstub_ --fast --base HEAD` (on the uncommitted change): every step
  PASS, mutant group 6 ran, 6 caught. Commit 2 `gate-light.sh resink_,sinkmm_,sunkframe_,sinkstub_,sinkmt_,entry_checks --fast --base HEAD`: every
  step PASS (trace suites 5.5 169 s / 5.4 159 s, `vm_corpus`, jit-diff 1-50, mutant group 17 ran, 17 caught). Its first run with the default on failed
  ten trace tests per version: the three bugs above, and three tests about the materialising frame stub itself, now pinned to `resink=0`.
* Final binaries (5.5 and 5.4 release `rn-cli`, defaults `sinkmm=1`, `resink=1`): `JITDIFF_MODES="off tr trdef" scripts/jit-diff.sh 1 40` with the
  default template set and with `JITDIFF_TSET=sink`: 40 seeds, 0 diffs, 0 timeouts each, both versions; the official suite (`scripts/jit-official.sh`,
  `RUSTYNAIL_JIT=always RUSTYNAIL_JIT_TRACE=always`): OK on 5.5 and 5.4.
* Not run (by instruction): `gate-heavy.sh`, the full mutant runner, the oracle-feature suites, PGO builds, the all-223 benchmark; no 5.4 timing.

### 52.5 Side finding: why `cspeed=0` costs raytrace / cd 12-15 % (docs/STUDY_CAUSAL_GAP.md 2.2)

Instruction profile of cd at 075b9f3 (`instructions:u`, period 100 k, `cspeed=123` vs `cspeed=0`): under `cspeed=0`, `ra::Allocation::uses_gpr` is
11.9 % of all instructions and `uses_xmm` 1.0 % (both absent from the default profile, where `used_masks` is computed once per allocation: 0.13 %);
the encoder's plain paths (`Assembler::rm_op_plain` 2.2 %) are the lever section 49 measured. The caller is section 50's `slink` test in the
deferred-stub loop of `Cx::stubs_and_tail` (`!self.al.uses_gpr(10)`, once per exit): with the `MASKS` lever off, every call scans the whole
allocation (instructions, operands, reloads, every snapshot location), so the stub loop is quadratic in the trace size. That one call is about 105 M of
cd's ~130 M extra instructions under `cspeed=0`; the rest is the old encoder paths. The shipped default is not affected (`MASKS` is on). `cspeed=0`
measured the section-49 "before" only up to section 50; hoisting the call out of the loop (one `uses_gpr(10)` per trace) would make it a valid
"before" again. Not changed here.

## 53. Cheaper formation steps: measured ceilings per lever, `codemap`, `memtune`, the side-trace `cspeed` levers (2026-11-24, base 65f3453)

Task: docs/STUDY_FORMATION.md builder 1, a formation step (side-trace compile path first, then root, then recording) **2x cheaper in cycles**, code
allowed to change, every candidate lever measured first with a scratch ceiling experiment. **Result: the 2x goal is missed by a wide margin.** What was
built makes a step 4-9 % cheaper in cycles (cd 5.5 of its ~96 Mcyc of formation, 6 %; raytrace 4 %, richards 6 %, towers 9 %; cd 10 % with `memtune`);
whole programs -0.97 % cycles (geomean, 31 programs, 5.5, shipped defaults), -2.05 % with `memtune` on, which misses the bar on `awfy/nbody` and is
therefore shipped off. Section 53.5 says why the structural levers were not reached.

### 53.1 Method

* Binaries: release `rn-cli` (5.5; 5.4 with `--cfg rustynail_lua54`, `LUA_VERSION=5.4`), base 65f3453 built in this session, scratch builds (never
  committed, removed) for the ceiling experiments. Copies with names of equal length, so that `argv[0]` does not move the stack (see 53.4).
* Totals: `perf stat -e cycles:u,instructions:u,task-clock,page-faults`, CPU 4 (`taskset`), nothing else running (checked), 200 ms spin before each
  run, `RUSTYNAIL_SEED=0`, configurations interleaved, every limits variable of equal length, and an environment padding variable rotated over
  5 lengths (0 / 24 / 72 / 136 / 200 bytes) across the runs of a configuration (the layout lottery of 53.4). Cycles = min (and median) of 5 runs (3 for
  5.4), instructions = min. `task-clock` is the wall time of the process including the kernel; `cycles:u` does not see page faults, `mprotect` or
  TLB shoot-downs (`cycles:k` is not available at `perf_event_paranoid` 2), so both are reported.
* The phase timers of `RUSTYNAIL_JIT_VERBOSE=1` were not used for decisions: they count TSC ticks (wall time, 2.52 cycles per tick at full clock),
  and the clock of CPU 4 varied between 3.3 and 5.0 GHz between back-to-back runs without the spin, which moved a phase by up to 35 %.
* Profiles: a frame-pointer build (`-C force-frame-pointers=yes`, own target directory, removed), `perf record -e cycles:u -c 100003 --call-graph fp
  --user-callchains`, 5 runs per program, inclusive shares of the run. (DWARF unwinding lost 16 % of the samples and most chains.)
* Ceilings by doubling (scratch switches): a phase run a second time on the same inputs and its result dropped; the cycle difference is the cost of the
  warm second run (the first, cold, run costs about 1.6x that, docs/STUDY_FORMATION.md sections 0 and 3.1), i.e. a lower bound of what removing the phase saves.

### 53.2 Where a step goes, and the measured ceilings

**Profile (fp call chains, % of the program's cycles; cd / raytrace / richards / towers / deltablue):** formation 40.3 / 37.1 / 31.9 / 25.1 / 32.7. Of it:
recording (the recorder's own work per bytecode, `rec_step` without the pipeline it calls; includes the few tier-1 compiles it triggers) 9.2 / 12.4 / 9.2 / 5.2 / 13.2; trace pipeline 25.7 / 20.1 / 18.1 /
16.1 / 12.7 (front passes 7.8 / 6.1 / 5.0 / 3.8 / 2.9, allocator 5.1 / 4.9 / 4.3 / 3.6 / 2.9, exits 2.6 / 2.1 / 1.1 / 1.3 / 0.8, assembler 4.3 / 3.5 / 3.7 / 3.2 / 2.6, shape
hash 1.05 / 0.81 / 0.83 / 0.74 / 0.50, x86 encoder of traces and tier 1 3.6 / 3.0 / 3.7 / 2.4 / 4.1); tier-1 compile 3.6 / 4.5 / 6.4 / 2.7 / 7.1; deferred stubs 2.4 /
1.0 / 1.0 / 1.3 / 1.0. Allocation leaves under formation (malloc / free / realloc, `RawVec` growth, `hashbrown`) 8.3 / 7.7 / 6.9 / 4.5 / 7.5, spread over about
40 call sites, none above 0.7 %. Inside the recorder, `record_one` (guards, snapshots, table-lookup guards) is the cost; the per-step bookkeeping of
`on_step` around it is under 1 %. Of a doubled side-trace back end (cd), the forwarding pass is 17 %, sinking 6 %, the allocator 21 %, the assembler
13 %, the exit builder 10.5 %, the encoder 8 %, the shape hash 6 %.

**Ceiling experiments [M] (cycles min / task-clock, % of the program; raytrace / cd / richards / towers / deltablue / fmt_s):**

| lever | experiment | ceiling |
|---|---|---|
| (1b/1c) kernel side of the compile: page faults, `mprotect` | scratch: code chunks mapped RWX (no `mprotect`), pre-faulted, the heap pre-faulted and never trimmed | -1.0/-3.1, -3.5/-8.0, -1.0/-2.7, +0.1/0.0, -1.2/-3.3, +0.5/+6.5 |
| | of that: no `mprotect` (RWX) | -0.9/-2.4, -1.0/-4.0, +0.2/-1.0, +0.1/-1.1, -0.6/-1.7, 0/0 |
| | of that: glibc tunables (trim 256 MiB, top pad 16 MiB) | +1.1/-0.1, -1.8/-3.3, -0.8/-2.0, -0.4/-1.0, -0.3/-1.6, +0.6/+1.2 |
| (1a) side-trace back end | doubled (front, allocator, exits, assembler, encoder, hash) | +8.8, +13.1, +10.5, +7.9, +3.5, 0 (2x cheaper, cold: about -7 / -10 / -8 / -6 / -3 %) |
| root back end | doubled | +3.9, +6.9, +0.1, +0.8, +5.3, +4.6 |
| side forwarding pass alone | doubled | +2.0, +3.6, +3.7, +3.3, +1.5, 0 |
| exit builder alone | doubled | +1.4, +3.5, +0.4, -0.9, +1.0 |
| shape hash alone | doubled | instructions +0.46 / +0.56 / +0.38 / +0.24 / +0.24 % |
| tier-1 compile | doubled | +2.5, +3.0, +2.7, +0.1, +4.7, +6.7 |
| (1d) recording | profile share (halving: half of it) | 12.4 / 9.2 / 9.2 / 5.2 / 13.2 % |
| skip the side forwarding pass when it changes nothing | count of side traces whose IR the pass leaves unchanged | 27 / 38, 123 / 234, 1 / 31, 8 / 27, 15 / 40: no sound skip worth building |
| (2) failed-recording predictor | docs/STUDY_FORMATION.md 3.4 (blacklist ceiling) | -2 / -4 / -1 / -8 / -10 % and +22 % on lua_lexer; not re-measured, not built |

(1c) cold start: its page-fault part is the first row; the cold instruction cache of the compiler code between compiles is not removable without
batching compiles, which changes formation (not attempted). A per-compile arena (1b proper) was not built: the allocation leaves are 4.5-8.3 % of the
run over about 40 sites (the allocator swap of the study and the tunables above bound what a faster allocator buys), and an arena means threading an
allocator through every pass.

### 53.3 What was built (knobs of `RUSTYNAIL_JIT_TRACE_LIMITS`, constants in `tr/cache.rs` / `crate::cspeed`)

| commit | lever | knob, default | what | verification |
|---|---|---|---|---|
| F53a | `memtune` | `memtune`, `MEMTUNE_PROCESS` **0** (off) | at registration glibc `mallopt`: keep up to 64 MiB of freed heap top, grow the heap by 8 MiB, blocks up to 32 MiB from the heap | `tr_core::f53_memtune_knob`; changes no result |
| F53b | `SIDEHASH` | `cspeed` bit 128, on | no shape hash for a side trace (only a root's is ever compared, `cache::install` / `retire`) | identical code (`ccheck`) |
| F53c | `FWDSNAP`, `SINKSKIP` | `cspeed` bits 256 / 512, on | side forwarding pass: snapshots renamed into one buffer, pruned in place, frame links by a running offset, its clean-up keeps the (unchanged) snapshot tables; sinking returns before its analysis without a `TNEW` | identical code: `ccheck` now also compares the snapshots of the passes (headers, entries, frame links, steps; `print::dump` shows instructions only); mutant `fwdsnap_frame_stale` |
| F53d | `codemap` | `codemap`, `CODEMAP_PROCESS` 1 | code chunks dual-mapped: a shared anonymous mapping (RW view) and a second view of its pages (`mremap` with `old_size` 0) made RX; writes go through the RW view, so installs, deferred stubs and patches need no `mprotect` and commits pack without page rounding; `pthread_atfork` child handler gives a forked child private copies; quarantine (`RUSTYNAIL_JIT=verify`) keeps the single mapping | `mem::tests::dual_mapping_*` (packing, patching, freeing, a fork whose child runs, patches and compiles without reaching the parent), `tr_core::f53_codemap_programs` (codemap 0 / 1, exit storm, GC stress 1 / 2 in both modes), docs/UNSAFE.md J-DUAL; with the default on every trace test runs dual-mapped |
| F53e | pattern cache | (a fix, no knob) | rn-std's compiled-pattern cache picks its two-slot set from the pattern's first and last eight bytes and length instead of its address (53.4) | rn-std tests; markdown 380-381 M instructions under every layout tried |

`cspeed` default is now 1019 (all levers; `cspeed=123` = the section-49 set, `cspeed=0` = the old paths). A `memfd` view was tried first for `codemap`
and dropped: `perf` treats a `memfd:` mapping as a file and stops resolving trace symbols through the perf map (the shared anonymous mapping shows as
`/dev/zero (deleted)`, which it treats as anonymous). Pre-faulting a dual chunk (`MAP_POPULATE` + `MADV_POPULATE_READ`) was measured and dropped:
cycles -0.3..-1.2 %, but `lib/fmt_s` +16 % wall (1 MiB faulted up front).

### 53.4 Numbers

**Per lever, same binary, knob off -> on (6 programs; cycles min of 9 / task-clock):**

| lever | raytrace | cd | richards | towers | deltablue | fmt_s |
|---|---|---|---|---|---|---|
| `memtune` | -1.0 / -2.5 | -1.7 / -3.5 | -0.7 / -1.8 | -0.7 / -0.7 | -0.3 / -1.4 | -0.8 / +0.6 |
| `codemap` | +0.1 / -0.5 | -0.7 / -3.3 | -0.6 / -1.9 | -0.3 / -1.2 | -0.4 / -0.9 | -0.1 / +1.2 |
| `SIDEHASH` + `FWDSNAP` (7 runs; instructions) | -1.5 / -0.57 % | -2.9 / -1.15 % | -2.2 / -0.86 % | -1.1 / -0.70 % | -0.3 / -0.20 % | -0.1 / -0.04 % |

Instructions of the compile-heavy programs at the hot thresholds 248 / 252 / 256 / 260 / 264 (base -> final, the code is identical so formation is the
same): raytrace -0.52 / -0.64 / -0.56 / -0.61 / -0.56 %, cd -1.16 / -1.20 / -1.14 / -1.00 / -1.02 %, richards -1.43 / -0.92 / -0.94 / -0.90 / -0.83 %, towers -0.67 /
-0.58 / -0.69 / -0.66 / -0.65 %, deltablue -0.15 / -0.17 / -0.20 / -0.21 / -0.23 %, fmt_s +0.02 % at each: no lottery.

**Ceiling against achieved:** kernel side (1b/1c), cycles / wall: cd ceiling -3.5 / -8.0 %, achieved -2.4 / -6.8 % with `memtune` and `codemap` (sum of the
two same-binary rows above; shipped, `codemap` alone: -0.7 / -3.3 %); raytrace -1.0 / -3.1 against -0.9 / -3.0; richards -1.0 / -2.7 against -1.3 / -3.7; towers
+0.1 / 0 against -1.0 / -1.9; deltablue -1.2 / -3.3 against -0.7 / -2.3. Side back end (1a): 2x ceiling about -7 / -10 / -8 / -6 / -3 % (raytrace / cd / richards / towers / deltablue), achieved -1.5 / -2.9 / -2.2 /
-1.1 / -0.3 %. Recording (1d), tier-1 compile, root back end, the predictor (2): not built, 0.

**Whole programs, 31-program sample (section 49's), base 65f3453 against the final binary (5.5: 5 runs, 5.4: 3 runs; cycles min):**

| | 5.5 shipped (`memtune` 0) | 5.5 `memtune=1` | 5.4 shipped | 5.4 `memtune=1` |
|---|---:|---:|---:|---:|
| geomean cycles | **-0.97 %** | -2.05 % | **-0.54 %** | -1.44 % |
| geomean instructions | -0.18 % | -0.19 % | -0.11 % | -0.12 % |
| geomean task-clock | -1.51 % | -3.33 % | -0.84 % | -2.29 % |
| worst instructions | markdown +0.45 %, expr_lang +0.28 % | the same | lua_parser +0.78 %, markdown +0.46 % | the same |
| worst cycles | markdown +1.9 %, havlak +1.0 % | **nbody +2.2 %** | prototype_objects +5.1 %, regex_nfa +2.7 % (7 runs) | regex_nfa +2.6 %, forth_vm +1.9 % |
| best cycles | lua_parser -5.9, events -3.6, prototype_objects -3.0, regex_nfa -2.5, cd -2.3, towers -2.2, lru_cache -2.1, richards -2.0 % | prototype_objects -10.7, lua_parser -5.9, particles_physics -4.6, events -4.2, cd -3.9, lru_cache -3.8, regex_nfa -3.3 % | cd -2.3, expr_lang -2.3, scheme_interp -2.1, richards -1.8, towers -1.8 % | prototype_objects -7.7, dijkstra_routes -4.5, cd -4.1, expr_lang -3.6, lru_cache -3.2 % |

Per program (5.5, shipped, cycles / instructions): bounce -0.4 / -0.10, cd -2.3 / -1.14, deltablue -0.6 / -0.20, havlak +1.0 / +0.03, json -1.3 / -0.13, list
-0.9 / -0.15, mandelbrot 0 / -0.01, nbody +0.1 / -0.02, permute -0.2 / -0.05, queens -1.5 / -0.03, richards -2.0 / -0.94, sieve +0.5 / 0, storage -0.2 / 0, towers
-2.2 / -0.69, events -3.6 / 0, expr_lang -2.0 / +0.28, lru_cache -2.1 / -0.70, markdown +1.9 / +0.45, raytrace -1.4 / -0.56, bigint_rsa +0.5 / 0, dijkstra_routes
-0.1 / -0.08, forth_vm -0.7 / -0.01, prototype_objects -3.0 / -0.08, roguelike_turns -1.1 / -0.18, scheme_interp +1.0 / -0.02, lua_parser -5.9 / -1.09, oop_mixins
-0.3 / -0.08, particles_physics -0.6 / -0.11, regex_nfa -2.5 / -0.02, classic_vector +0.4 / -0.08, fmt_s -0.2 / +0.02 (%).

**Steady state** (5x the default size: raytrace 10, cd 5, richards 125, towers 225, deltablue 100, json 175, lru_cache 450000, havlak 30, expr_lang 30; 5.5, 3
runs): shipped -0.67 % cycles, -0.08 % instructions (geomean; every program -0.03..-0.38 % instructions except expr_lang +0.50 %, the pattern hash);
`memtune=1` -2.03 % (havlak -6.6 %, cd -3.4 %, lru_cache -3.2 %). Cheaper steps did not lower code quality (the trace code is identical).

**The layout lottery.** The first bar run (binaries named `base` / `fin` / `fin`, limits strings of different length) showed markdown +8.4 %
instructions. Cause: rn-std's compiled-pattern cache indexed its sets by string address, and `memtune` moved markdown's hot patterns into one set
(410 M instructions at every `argv[0]` length tried, 378 M with `memtune=0`; with a relative binary path both 377 M). F53e indexes by content (380-381 M
everywhere). Since then the runs control `argv[0]`, the length of the limits variable and rotate an environment pad. The remaining cycle outliers
at identical instructions are layout too: 5.4 prototype_objects / regex_nfa are +3.6 % / +3.6 % with every new knob off in the same final binary.

### 53.5 Bar, misses, and why 2x was not reached

**Bar** (no program over +1 % instructions / +2 % cycles at default size, geomean not worse, steady state not worse): shipped defaults meet it on 5.5
(worst markdown +0.45 % instructions / +1.9 % cycles). 5.4 misses on cycles at identical instructions: prototype_objects +5.1 %, regex_nfa +2.7 %; the
same binary with all new knobs off shows +3.6 % on both, so these are the binary's code layout (as in 49.5), not a lever. **`memtune` misses** on 5.5
nbody: +1.8..+2.7 % cycles at identical instructions in four separate series, from the top pad (top pads of 1, 2, 4 and 8 MiB all give +1.9..+3.3 %; on
5.4 the same knob gives nbody -2.5 %; with transparent huge pages `always` the padded heap is backed differently). It is the largest lever measured here
(5.5 geomean -2.05 % instead of -0.97 %, prototype_objects -10.7 %, havlak -2.9 % at default size and -6.6 % at 5x), so it stays as a knob:
`RUSTYNAIL_JIT_TRACE_LIMITS=memtune=1`, or `MEMTUNE_PROCESS = 1` to ship it.

**Goal missed: a formation step 2x cheaper.** Achieved: a step 4-9 % cheaper in cycles (cd 10 % with `memtune`); the side forwarding pass is the
only pass made substantially cheaper (`SINKSKIP` was not measured on its own). Why, from 53.2:

1. **The step has no hot spot.** The pipeline is ten passes, each linear in the snapshot volume (one snapshot of 10-60 entries per guard-bearing
   bytecode, copied and walked by the recorder, the forwarding pass, sinking, the allocator, the exit builder, the assembler); after C1 and this
   section no function is above 2 % of the run. Halving it needs fewer or smaller snapshots and fewer passes: delta-encoded snapshots of a side
   trace against its parent's exit and one pass that allocates while it emits. Both change every stage of the back end and its tests (the exit
   numbering, stubs, counters and `ccheck` follow the snapshot table), far beyond the time box; neither was prototyped.
2. **The kernel and allocator part was smaller than the hypothesis.** Page faults were 5-10x the interpreter's (cd 2290 against 412), but removing
   them and every `mprotect` gave 1-3.5 % of cycles and 3-8 % of wall time; `memtune` + `codemap` reach that ceiling.
3. **The skip levers are not sound or not large.** The side forwarding pass changes the IR in most side traces (richards 30 of 31), so "skip when
   nothing to forward" is no lever; the shape hash and sinking without allocations were the safe skips (built). A sound predictor of failing
   recordings was not found (the study's blacklist ceiling includes lua_lexer's +22 %).
4. **Recording** (5-13 % of the run) is `record_one` itself: guards and snapshots per bytecode, the same volume as item 1.

### 53.6 Gates and what was not run

* Each commit: `GATE_CPUS=2-15 scripts/gate-light.sh <group> --fast --base HEAD~1`. F53a (automatic group) and F53b (`cspeed_`): every step PASS
  except `mutant-map` (line shifts), regenerated and checked (`gen-mutant-map.py --check`) and amended into the commit. F53c (`fwdsnap_,cspeed_`): its
  first run had `fwdsnap_no_prune` survive (an equivalent mutant: the renamed entries of a kept snapshot are never prunable, and `ccheck` did not compare
  snapshots); replaced by `fwdsnap_frame_stale` and `ccheck` extended, then all steps PASS (mutant group 5 caught). F53d: the first run failed
  `tr_runtime::lifetime_verify_mode_quarantines_flushed_trace_blocks` (it expected the private `r-xp` mapping; a dual chunk is `r-xs`), fixed; the automatic
  mutant group had one survivor, `rec_inner_first_stale`, which also survives at the base 65f3453 (checked with the same test binary configuration) and is
  not caused by this work; rerun with `cspeed_,fwdsnap_`: all PASS. F53e (`cspeed_`): all PASS. F53f (`memtune` default off, this section,
  `cspeed_`): all PASS.
* `ccheck=1` on raytrace, cd, richards, towers, deltablue, fmt_s, lru_cache, havlak, json, classic_vector (final 5.5 binary, snapshots compared): 3-278
  compiles compared per program, 0 differ.
* Final binaries (5.5 and 5.4, shipped defaults: `codemap=1`, `cspeed=1019`, `memtune=0`): `JITDIFF_MODES="off tr trdef" scripts/jit-diff.sh 1 40`: 40
  seeds, 0 diffs, 0 timeouts on each version; the official suite (`scripts/jit-official.sh`, `RUSTYNAIL_JIT=always RUSTYNAIL_JIT_TRACE=always`): OK on 5.5
  and 5.4.
* Not run (by instruction): `gate-heavy.sh`, the full mutant runner, the oracle suites, PGO builds, the all-223 benchmark, `bench-fast.sh`; no 5.4 steady
  state; the hot-threshold check on six programs only; lua_lexer not measured (no predictor was built). `cargo test -p rn-std` in release fails
  `interp_scripts` gen_04 / inc_04 / pool_04 (a C-stack depth count, 142852 vs 142854) at the base as well.

## 54. Runtime and tier-1 levers for interpreter-shaped programs: R1 `memfind`, R2 generic `for` (`t1g`), R5 string-table compare, R6 MULTRET / C tail calls (2026-11-24, base d7bab19)

Measurement first: docs/STUDY_RUNTIME_GAP.md (per-operation costs on both engines, in-context doubling on text_adventure, an IBS survey of the 40
programs with a gap above 1.4x, a per-opcode split of tier-1 code). Its result decides the size of this section: **the library functions are not
where interpreter-shaped programs lose** (in context, `gmatch`, table appends, lookups, `ipairs` scans and vararg calls cost us what they cost
LuaJIT; `format` costs what LuaJIT's interpreter pays); the measurable per-operation gaps are spread over call overheads, `pairs` / `next`
(2.1-2.9x LuaJIT's interpreter in tier 1), the coroutine switch (1.31x) and string creation from traces. Four fixes were built, each its own
commit, in order of measured ceiling x plausibility; the coroutine lever with the largest ceiling was measured and rejected (54.5).

Method: release `rn-cli` (5.5; 5.4 with `RUSTFLAGS="--cfg rustynail_lua54"`), **control = d7bab19 built in this session** (5.4 from `git archive`),
binaries in equal-length directories, the same environment string on every run (`RUSTYNAIL_STR_FAST=23 RUSTYNAIL_JIT_TRACE_LIMITS=t1g=3`, which
the control ignores; the all-knobs-off column uses `RUSTYNAIL_STR_FAST=07 RUSTYNAIL_JIT_TRACE_LIMITS=t1g=0`, same length), `RUSTYNAIL_SEED=0`,
cycles = min of 5 (5.4: 3) interleaved runs pinned to CPU 4 after a 300 ms spin, instructions = min of the same runs. Sample: the 29 programs of
section 49's sample plus 15 target programs (44).

### 54.1 R1: plain substring search 16 starts at a time (`rn-std`, STR_FAST bit 16 `MEMFIND`, default on)

`pattern::memfind` (the plain `string.find`, `find_plain_pos` of the trace `FIND` helper) found a candidate with a SWAR first-byte search and
compared the rest with `memcmp`. Now (x86_64, needle of 2+ bytes, 16+ start positions): load 16 bytes at the start positions and 16 at start +
`len - 1`, compare with the needle's first and last bytes (SSE2), AND the masks, verify the candidates lowest first with overlapping word loads
(`eq_short`). The last block overlaps the previous one (its already-seen starts masked out), so no load reads past the haystack. Same result (the
first occurrence). Ceiling (STUDY_RUNTIME_GAP 5 #2): find_plain 60 Mcyc against LuaJIT. Achieved: find_plain 122.2 -> 112.6 Mcyc (-7.9 %),
785.8 -> 698.5 M instructions (-11.1 %); same binary with the bit off 129.7 (the old code in the new binary is slower than in the old binary: a
layout difference of the binary). Realised 0.16 of the ceiling against the base, 0.29 against the bit off. Test:
`rn-std/tests/strfast_diff.rs::memfind_pair_matches_the_scalar_search` (40 000 random haystacks of 0-80 bytes around the block boundaries, needles of
1-40 bytes from two-letter and ten-letter alphabets, matches at the start / end / overlapping, `find_plain_pos` with random `init`, bit on and
off against a naive scan). No mutant: the one guard whose removal would be wrong (the masked starts below `i`) is not observable (a start below
`i` was already rejected, it is rejected again).

### 54.2 R2: tier-1 generic `for` (knob `t1g`, default 3) and four results inline

* **Four results inline** (`rn-core` `native_fast_with`): the inline result move of a native call handled 0-3 wanted results; the 5.5 generic `for`
  asks its iterator factory (`pairs`, `ipairs`, `string.gmatch`) for four, which went through the general `post_call` (13.6 % of a tier-1
  `pairs` call on an empty table). Not knob-gated (no behaviour change).
* **`t1g` bit 0: `CLOSE` without its helper** when no to-be-closed variable is registered (`thread.tbc` length 0) and no open upvalue lies at or
  above `R[A]` (the open list is sorted, its head is the highest): `luaF_close` would do nothing. Every generic `for` of 5.4 / 5.5 ends with a
  `CLOSE`. The count of the instruction comes first, as in the helper template.
* **`t1g` bit 1: `TFORCALL` of `next` through the lean helper `jit_tfor_next`**: the template checks the C-stack depth, that `R[A]` is the
  registered `next` (by address) and `R[A+1]` a table, then calls a helper that cannot call, raise, allocate or move the stack (no `savedpc`
  store, no frame reload, no hook re-test after it); an invalid key returns `H_EXIT` and the general `jit_tforcall` raises the error.
* Knob: `RUSTYNAIL_JIT_TRACE_LIMITS=t1g=N` (`TrLimits::t1g`, `T1G_PROCESS = 3`, `CallEnv::t1g`); like `t1q`, a named value also applies with
  `RUSTYNAIL_JIT_TRACE=off` (bit 7), so `RUSTYNAIL_JIT_TRACE=off` stays J4 exactly unless asked.

Tier 1 per operation (cycles, same binary, `t1g=0` -> `3`): `pairs` over an empty table 119 -> 101, over 3 keys 218 -> 193, over 12 keys 519 ->
479, `next` 3 keys 145 -> 123, `ipairs` 3 125 -> 113, `gmatch` 1 word 413 -> 398. The per-iteration cost of `next` stays about 33 cycles against
LuaJIT's interpreter's 11 (the lean helper is ~60 instructions; the rest is the loop's template code and `next_hinted`): realised 0.18 of the
`pairs3` gap. Programs (instructions, same binary `t1g=0` -> `3`, at the hot thresholds 248 / 252 / 256 / 260 / 264): text_adventure -0.65 /
-0.64 / -0.59 / -0.66 / -0.65 %, inspect_lua -0.94 / -0.80 / -0.78 / -0.84 / -0.81 %, lua_parser -0.56 / -0.71 / -0.90 / -0.93 / -0.70 %, json_rxi
-0.23 / -0.24 / -0.35 / -0.27 / -0.21 %, parser_combinator / roguelike_turns / expr_lang within +-0.03 %: a gain at every threshold, no formation
landing. Test `tr_core::r2_genfor_programs` (`tests/vm/r2_01_genfor.lua`: pairs / next / ipairs over hash, array and mixed tables, nested loops and
breaks, modification during the traversal, an invalid key to `next`, light C function / C closure / Lua / `__pairs` iterators, closures capturing
loop variables, to-be-closed variables in blocks and loops, a 300-key traversal with a collection in the middle; `t1x` / `off` / `always` / `on2` x
`t1g` 0-3, `+verify`, the emulator, budgets 3 / 17 / 200, GC stress 1 / 2 in both collector modes, stack-move stress, three more seeds; every job
against the interpreter). Mutants `t1g_close_tbc_skip`, `t1g_close_level_skip`, `t1g_next_fn_skip` (3 caught; registry 872 -> 875).

### 54.3 R5: the string-table probes compare with word loads (`rn-core`, A/B cargo feature `rn-core/streq_libc`)

`Heap::new_str` / `jit_str_probe` / `jit_str_bytes` compared a candidate whose hash matched with a slice comparison, a `bcmp` call through the PLT
(5 % of `short_core`'s samples on `lib/sub`); a hit compares every byte. `short_bytes_eq` uses overlapping unaligned 4 / 8-byte loads (at most five
for 40 bytes). Ceiling (STUDY_RUNTIME_GAP 5 #7): `string.sub` in a trace 36.6 against LuaJIT's 19.7 cycles. Achieved (A/B builds, same session):
lib/sub 124.2 -> 116.2 Mcyc (-6.5 %, -3.5 % instructions), knucleotide 1017.5 -> 988.4 (-2.9 %), json_rxi and lib/char unchanged. A first version
with a loop over 8-byte words cost lib/sub +2.3 % instructions (over the bar) and was replaced by the straight-line form. Test
`heap::tests::short_bytes_eq_matches_slice_equality` (every length 0-40, every byte flipped by 1 and 0x80, and through the string table).

### 54.4 R6: native calls with all results wanted, light C tail calls (`rn-core`)

`native_fast_with` moves the results of a `LUA_MULTRET` call (the last argument or expression of a list: `f(a, g(x))`, `{g(x)}`,
`print(fmt(...))`) inline instead of through `post_call`; `pre_tail_call` runs a light C function tail-called with all results wanted (no `__call`
on the way, not a coroutine function: those keep `precall_c`) through the same inline path. Tier 1 (cycles per operation, before -> after):
`max(1, floor(x))` 104 -> 96, `return floor(x)` 90 -> 83, `return select('#', ...)` 151 -> 143, `insert(t, sub(s, 1, 5))` 169 -> 165,
`fmt('%s!', tostring(k))` 231 -> 222. Sample instructions: scheme_interp -0.77 %, everything else within +-0.3 %. Ceiling (`selectn`'s 57-cycle gap
to LuaJIT's interpreter): realised 0.14. Not knob-gated (no behaviour change). Test `tests/vm/r6_01_multret.lua` (vm_corpus against the
reference: result counts 0 to 300, beyond `LUA_MINSTACK`, constructors, tail-called C functions that raise, call metamethods, run with a hook,
inside coroutines; checked by hand on 5.5 and 5.4 in the interpreter, tier 1, traces, GC stress 1 / 2 in both collector modes).

### 54.5 Measured and not built

* **Coroutine bodies in tier 1** (the largest ceiling: the interpreter + `coro_direct` are about 1900 Mcyc over the gap programs, co_yield's round
  trip 127 against LuaJIT's 67.5 cycles). Experiment (scratch, `RUSTYNAIL_CORO_T1`): after the fused resume finishes the `yield` call, flag the
  coroutine's Lua frame `TRAP_JIT` so the interpreter enters its tier-1 code at the resume point. co_yield 176.0 -> 203.9 Mcyc (+16 %),
  programs/coroutines 1377 -> 1653 (+20 %): `rt::enter` costs 35 cycles per resume and a `yield` from tier 1 leaves through the error exit, more
  than interpreting the five bytecodes between two yields. A win needs a resume entry without the `enter` protocol and a `yield` that returns
  from compiled code directly; not attempted.
* **E9 return cells** (`t1call=1`) on the interpreter-shaped programs: text_adventure +0.4 %, parser_combinator +0.6 %, json_rxi -0.4 %, lua_interp
  +2.0 %, events -2.6 %, inspect_lua -1.0 % cycles (same binary): the return stub's walk is not where `RETURN` spends its time.
* **`t1ccall=1`** (lean tier-1 call of light C functions): text_adventure -0.1 %, parser_combinator -4.0 % cycles at +0.2 % instructions, json_rxi
  +1.6 % at +0.4 %, inspect_lua +0.5 %: formation-dependent, left off.
* **`table.concat`, `tostring`, positional `table.insert` fast paths**: 1.24-1.45x LuaJIT's interpreter, but executed 36-224 k times in the target
  programs (ceilings 0.1-0.7 %).
* **tremove_pop** (1.79x): not runtime. The inner loop's trace is installed, then flushed by the nest rule because the outer loop's
  `table.move(base, 1, 50, 1, {})` (50 > `MAX_MOVE` = 16) aborts `CFunc` at 7 recorded steps (too short to stitch); the inner loop then runs in
  tier 1 with `remove` through `jit_call` (80 cycles against 14 in LuaJIT's trace). `table.move` itself is 1.11x LuaJIT's.

### 54.6 All four against the control (44 programs, 5.5 and 5.4)

Cycles base -> new (Mcyc), instructions (new against base), and the same new binary with every knob of this section off (`STR_FAST=07`, `t1g=0`;
R5 / R6 and the four-result move have no knob, so this column is "binary layout + the unknobbed changes"):

| program | 5.5 cycles base -> new | 5.5 instr | 5.5 knobs off (cyc / instr vs base) | 5.4 cycles base -> new | 5.4 instr | 5.4 knobs off |
|---|---|---:|---:|---|---:|---:|
| awfy/bounce | 58.4 -> 58.9 (+0.8 %) | -0.02 % | +0.4 % / -0.02 % | 63.4 -> 63.3 (-0.2 %) | -0.01 % | +0.3 % / -0.01 % |
| awfy/cd | 231.8 -> 231.2 (-0.3 %) | -0.02 % | -0.1 % / -0.02 % | 252.9 -> 254.8 (+0.8 %) | -0.03 % | +0.5 % / -0.03 % |
| awfy/deltablue | 102.9 -> 102.7 (-0.2 %) | -0.04 % | -0.1 % / -0.04 % | 111.8 -> 111.1 (-0.7 %) | -0.07 % | -0.5 % / -0.07 % |
| awfy/havlak | 157.0 -> 153.2 (-2.4 %) | -0.06 % | +0.3 % / -0.02 % | 172.2 -> 174.8 (+1.5 %) | +0.03 % | +0.3 % / +0.02 % |
| awfy/json | 135.2 -> 135.7 (+0.4 %) | -0.07 % | +0.4 % / -0.07 % | 149.9 -> 148.0 (-1.2 %) | -0.03 % | -1.0 % / -0.03 % |
| awfy/list | 92.7 -> 92.4 (-0.3 %) | -0.00 % | -0.4 % / -0.00 % | 94.5 -> 94.4 (-0.1 %) | -0.01 % | +0.0 % / -0.01 % |
| awfy/mandelbrot | 272.1 -> 271.8 (-0.1 %) | -0.00 % | -0.1 % / -0.00 % | 272.0 -> 272.1 (+0.0 %) | +0.00 % | +0.2 % / +0.00 % |
| awfy/nbody | 90.4 -> 90.5 (+0.1 %) | -0.00 % | -0.0 % / -0.00 % | 94.0 -> 93.6 (-0.4 %) | -0.00 % | -0.8 % / -0.00 % |
| awfy/permute | 35.8 -> 35.2 (-1.6 %) | -0.00 % | -1.8 % / -0.00 % | 36.8 -> 36.6 (-0.5 %) | -0.00 % | -0.3 % / -0.00 % |
| awfy/queens | 93.6 -> 93.3 (-0.3 %) | -0.00 % | -0.4 % / -0.00 % | 80.0 -> 79.6 (-0.5 %) | +0.00 % | -0.4 % / +0.00 % |
| awfy/richards | 56.5 -> 56.1 (-0.8 %) | -0.02 % | -1.1 % / -0.02 % | 57.6 -> 57.5 (-0.1 %) | -0.01 % | -0.2 % / -0.01 % |
| awfy/sieve | 110.9 -> 110.4 (-0.5 %) | -0.00 % | +0.3 % / -0.00 % | 116.5 -> 114.7 (-1.6 %) | -0.00 % | -0.4 % / -0.00 % |
| awfy/storage | 102.4 -> 102.3 (-0.1 %) | -0.00 % | -0.6 % / -0.00 % | 147.4 -> 145.5 (-1.3 %) | -0.00 % | -1.1 % / -0.00 % |
| awfy/towers | 44.2 -> 43.4 (-1.7 %) | -0.02 % | -1.9 % / -0.02 % | 44.9 -> 45.4 (+1.0 %) | -0.03 % | +0.3 % / -0.02 % |
| apps/events | 163.2 -> 161.5 (-1.1 %) | +0.00 % | +1.1 % / -0.00 % | 177.6 -> 184.4 (+3.8 %) | +0.00 % | +1.0 % / -0.00 % |
| apps/expr_lang | 126.6 -> 126.7 (+0.1 %) | -0.01 % | -0.3 % / -0.02 % | 141.7 -> 145.0 (+2.3 %) | -0.01 % | +1.3 % / -0.01 % |
| apps/lru_cache | 137.5 -> 140.3 (+2.0 %) | -0.01 % | -0.2 % / -0.01 % | 175.6 -> 174.9 (-0.4 %) | -0.01 % | -0.6 % / -0.01 % |
| apps/markdown | 118.3 -> 119.9 (+1.3 %) | -0.14 % | +1.5 % / -0.19 % | 128.5 -> 127.9 (-0.4 %) | -0.20 % | +0.3 % / -0.25 % |
| apps/raytrace | 93.2 -> 93.8 (+0.7 %) | -0.03 % | +1.0 % / -0.03 % | 102.7 -> 102.5 (-0.2 %) | -0.01 % | +0.8 % / -0.01 % |
| corpus2/heldout/bigint_rsa | 1111.2 -> 1110.5 (-0.1 %) | +0.00 % | -0.0 % / +0.00 % | 1361.8 -> 1365.2 (+0.3 %) | +0.00 % | +0.1 % / +0.00 % |
| corpus2/heldout/dijkstra_routes | 1131.4 -> 1132.8 (+0.1 %) | -0.07 % | -0.6 % / -0.01 % | 1432.5 -> 1440.0 (+0.5 %) | -0.72 % | +0.9 % / +0.00 % |
| corpus2/heldout/forth_vm | 800.6 -> 796.7 (-0.5 %) | -0.01 % | -0.6 % / -0.01 % | 985.0 -> 993.4 (+0.8 %) | +0.00 % | +0.8 % / +0.00 % |
| corpus2/dev/oop_mixins | 1109.1 -> 1090.2 (-1.7 %) | +0.01 % | -1.3 % / +0.02 % | 1098.0 -> 1114.4 (+1.5 %) | -0.01 % | +0.1 % / -0.02 % |
| corpus2/heldout/prototype_objects | 1346.0 -> 1346.7 (+0.0 %) | -0.09 % | +1.8 % / -0.09 % | 1444.8 -> 1450.3 (+0.4 %) | -0.03 % | -0.1 % / -0.03 % |
| corpus2/heldout/roguelike_turns | 792.2 -> 794.8 (+0.3 %) | -0.04 % | -0.1 % / -0.01 % | 837.5 -> 836.3 (-0.1 %) | -0.61 % | +1.0 % / -0.00 % |
| corpus2/heldout/scheme_interp | 1427.6 -> 1420.9 (-0.5 %) | -0.78 % | -0.7 % / -0.78 % | 1530.3 -> 1498.5 (-2.1 %) | -0.66 % | -1.4 % / -0.65 % |
| corpus2/dev/lua_parser | 1182.7 -> 1212.8 (+2.5 %) | -1.09 % | +2.7 % / -0.19 % | 1294.0 -> 1270.0 (-1.9 %) | -1.54 % | -1.2 % / -0.21 % |
| corpus2/dev/particles_physics | 385.8 -> 387.6 (+0.5 %) | +0.05 % | -0.0 % / -0.02 % | 490.2 -> 490.2 (-0.0 %) | +0.02 % | +0.3 % / -0.02 % |
| corpus2/dev/regex_nfa | 664.7 -> 652.2 (-1.9 %) | -1.11 % | +3.6 % / -0.04 % | 1359.8 -> 1319.7 (-3.0 %) | -1.54 % | -1.4 % / -0.59 % |
| corpus2/dev/text_adventure | 2588.6 -> 2558.5 (-1.2 %) | -1.20 % | +0.1 % / -0.61 % | 2788.0 -> 2689.5 (-3.5 %) | -1.31 % | -3.2 % / -0.57 % |
| corpus2/heldout/stream_closures | 761.2 -> 765.8 (+0.6 %) | +0.05 % | +0.8 % / -0.02 % | 800.2 -> 801.5 (+0.2 %) | +0.08 % | -0.8 % / -0.02 % |
| lib/find_plain | 122.2 -> 112.6 (-7.9 %) | -11.12 % | +6.0 % / -0.49 % | 121.5 -> 108.8 (-10.4 %) | -11.12 % | +5.2 % / -0.49 % |
| corpus/lua_lexer | 807.7 -> 819.8 (+1.5 %) | -0.34 % | +0.6 % / -0.21 % | 843.6 -> 843.7 (+0.0 %) | -0.33 % | -0.9 % / -0.19 % |
| lib/co_yield | 180.7 -> 174.6 (-3.4 %) | +0.00 % | -3.4 % / +0.00 % | 191.0 -> 176.4 (-7.7 %) | -0.00 % | -7.5 % / -0.00 % |
| lib/tremove_pop | 240.4 -> 237.7 (-1.2 %) | +0.14 % | -0.8 % / +0.14 % | 223.1 -> 226.3 (+1.5 %) | +0.05 % | +1.7 % / +0.05 % |
| real/lua_interp | 102.1 -> 100.9 (-1.2 %) | -0.17 % | -0.4 % / -0.01 % | 103.7 -> 102.3 (-1.3 %) | -0.17 % | -1.1 % / -0.03 % |
| lib/fmt_s | 6.4 -> 6.4 (-0.3 %) | -0.06 % | -0.6 % / -0.06 % | 6.4 -> 6.4 (+0.1 %) | -0.06 % | -0.1 % / -0.07 % |
| programs/string_ops | 1146.3 -> 1137.5 (-0.8 %) | +0.03 % | +0.6 % / +0.03 % | 1088.6 -> 1101.7 (+1.2 %) | +0.02 % | +1.6 % / +0.02 % |
| corpus/parser_combinator | 2434.5 -> 2318.3 (-4.8 %) | -0.23 % | -5.7 % / -0.21 % | 2508.8 -> 2493.8 (-0.6 %) | -0.24 % | +1.8 % / -0.20 % |
| corpus/json_rxi | 1818.7 -> 1776.1 (-2.3 %) | -0.55 % | -1.2 % / -0.17 % | 1920.7 -> 1893.6 (-1.4 %) | -0.63 % | -0.3 % / -0.17 % |
| corpus/inspect_lua | 1532.4 -> 1473.1 (-3.9 %) | -1.09 % | -3.9 % / -0.27 % | 1682.4 -> 1592.8 (-5.3 %) | -1.25 % | -4.4 % / -0.21 % |
| programs/knucleotide | 1010.1 -> 993.4 (-1.7 %) | -0.46 % | -1.9 % / -0.46 % | 1036.2 -> 1013.0 (-2.2 %) | -0.46 % | -2.5 % / -0.46 % |
| lib/sub | 124.5 -> 116.0 (-6.8 %) | -3.46 % | -6.2 % / -3.46 % | 126.1 -> 117.2 (-7.1 %) | -3.46 % | -7.3 % / -3.46 % |
| programs/coroutines | 1408.9 -> 1356.3 (-3.7 %) | +0.45 % | -4.7 % / +0.01 % | 1499.7 -> 1388.6 (-7.4 %) | +0.42 % | -6.8 % / +0.00 % |

Geomean over the 44 programs: **5.5 cycles -0.97 %, instructions -0.51 %; 5.4 cycles -1.08 %, instructions -0.56 %.** New against its own
knobs-off column: 5.5 -0.54 % / -0.34 %, 5.4 -0.46 % / -0.38 %. Largest gains (5.5 / 5.4 cycles): find_plain -7.9 / -10.4 %, lib/sub -6.8 /
-7.1 %, parser_combinator -4.8 / -0.6 %, inspect_lua -3.9 / -5.3 %, coroutines -3.7 / -7.4 %, co_yield -3.4 / -7.7 %, json_rxi -2.3 / -1.4 %,
knucleotide -1.7 / -2.2 %, text_adventure -1.2 / -3.5 %. The coroutine programs gain at equal or higher instruction counts (the R6 / four-result
paths of `coroutine.wrap` calls and layout); not attributed further.

**Bar** (no program over +1 % instructions or +2 % cycles; geomean not worse). Instructions: met on both versions (worst 5.5 coroutines +0.45 %,
5.4 coroutines +0.42 %). Geomeans: met. **Cycles: three misses, all at equal or lower instruction counts, re-measured with 11 runs:**

* 5.5 `lua_parser` +2.2 % cycles at -1.09 % instructions. The same binary with every knob off is +2.8 % (at -0.19 % instructions), and the R5
  binary (before R6) was -0.2 %: it is the binary's code layout after R6 (L1 icache misses 21.6 / 22.0 M with the R5 binary, 27.0 / 24.1 M with
  the final one, branch misses equal). Binary layout.
* 5.4 `events` +3.7 % at equal instructions (731.6 M): +1.3 % with every knob off (binary layout), and within the same binary `t1g` bit 1 adds
  +1.6 % at +0.003 % instructions (`t1g` 0 / 1 / 2 / 3: 179.2 / 179.7 / 182.1 / 182.2 Mcyc; on 5.5 the same bit gives -1.0 %): the bit makes
  every `TFORCALL` template longer and shifts the tier-1 code of events, which runs 49 % in tier 1 and executes none of the new path. Tier-1 code
  layout.
* 5.4 `expr_lang` +2.3 % in the 3-run pass, +0.7 % with 11 runs (knobs off +1.4 %): noise / layout, not a miss at 11 runs.
* 5.5 `lru_cache` +2.0 % in the 5-run pass, +0.0 % with 11 runs: not a miss.

### 54.7 Gates and what was not run

* Per commit `scripts/gate-light.sh [group] --base HEAD~1 --fast`: R1 (default mutant set; the first run was cut by an outer `timeout` of mine
  during the mutant step, which was rerun alone: PASS), R2 (`t1g_`; the first run failed `registry_is_the_frozen_list` and
  `table_covers_the_registry_exactly_once`: the registry count and the TABLE order, fixed and amended; then every step PASS, mutant group 3 / 3
  caught), R5 and R6 (default sets): every step PASS. `scripts/gen-mutant-map.py` regenerated with R2.
* Final binaries (5.5 and 5.4): `JITDIFF_MODES="off tr trdef" JITDIFF_ENTERS=1 scripts/jit-diff.sh 1 40`: 0 diffs, 0 timeouts on both (5.5: 35 / 40
  seeds with traces, 1067 installed; 5.4: 1061); the official suite (`scripts/jit-official.sh`, `RUSTYNAIL_JIT=always RUSTYNAIL_JIT_TRACE=always`):
  `JIT-OFFICIAL OK` on both. `r2_01_genfor` and `r6_01_multret` against the reference 5.5 and 5.4 binaries under `RUSTYNAIL_JIT=off`, JIT +
  traces always, tier 1 only with GC stress 2, and GC stress 1 / 2 in generational and incremental mode: identical output.
* Not run (by instruction): `gate-heavy.sh`, the full mutant runner, the oracle suites, PGO builds, the all-223 benchmark, `bench-fast.sh`; no
  steady-state runs.

## 55. Side-trace compile path at identical code: kill test (2026-11-24, base d7bab19; nothing built)

**Kill test failed: no lever, and no short path, makes the side-trace compile 2x cheaper at identical machine code.** Measured with a scratch build (not committed): the phase timers of `[trace] phases` read a user-mode `instructions` counter of the thread (`perf_event_open`, exclude_kernel) instead of `rdtsc`, and add into a second side-only table while `pipeline` runs a side trace (the side forwarding pass timed as well). Deterministic, one run per configuration, `RUSTYNAIL_SEED=0`, default size.

| program | side traces | side pipeline M instr, `cspeed` 1019 / 0 | k instr per side trace | sinking | forwarding | allocator | exits | assembler | encoder | install + rest |
|---|---:|---|---:|---:|---:|---:|---:|---:|---:|---:|
| awfy/cd | 213 | 78.4 / 114.8 | 368 | 9.1 % | 15.0 % | 22.8 % | 13.3 % | 17.8 % | 14.2 % | 7.7 % |
| apps/raytrace | 44 | 46.8 / 76.9 | 1064 | 20.5 % | 6.8 % | 25.7 % | 15.1 % | 15.4 % | 10.8 % | 5.7 % |
| awfy/richards | 31 | 18.2 / 25.6 | 587 | 0.2 % | 25.1 % | 26.3 % | 7.4 % | 20.5 % | 13.6 % | 6.9 % |
| awfy/towers | 21 | 10.4 / 12.8 | 494 | 0 % | 22.7 % | 25.1 % | 10.3 % | 18.6 % | 16.8 % | 6.5 % |

Why there is no 2x at identical code: the biggest phase is the allocator at 23-26 %, so dropping the allocator and the exit builder entirely would still leave the side pipeline at 62-67 % of its cost. 2x means taking half out of every phase together. The flat profile of cd (`perf record -e instructions:u`) has no compile function above 2.3 % of the run, and malloc / free / `finish_grow` together are about 2.5 % of the run, so an arena is bounded at about 10 % of the side pipeline. The r14 retry allocation never runs on these four programs (`xregs` bit 2 is off), so there is no duplicated allocator run to remove either. The side pipeline is 11.4 % of cd's 686 M instructions, so even 2x would give about -5.7 % on cd. The remaining route is structural and changes the code: emitting x86 straight from the IR in one backward pass that also allocates registers (replacing the allocator, the assembler's micro-op program and the encoder, 55-60 % of the side cost), plus snapshots stored as deltas to the parent's exit (sinking, forwarding and the exits walk the full maps). That is a new back end with its own soundness bar, not a 30-minute prototype.

## 56. Session-m regressions: causes and fixes (2026-11-24, base 4e89dd9)

Regressions of the session-m tables (`bench/results/session-m/tables.md`, "regressions vs 7413bff" / "vs 5a064d7") traced to a cause, one item
at a time. Method: instruction counts (deterministic) per knob (`RUSTYNAIL_JIT_TRACE_LIMITS=<knob>=0`, `RUSTYNAIL_STR_FAST=15`), a
commit bisect where no knob moved the count, and cycles as min of 3-5 `perf stat` runs on CPU 4 (`RUSTYNAIL_SEED=0`; plain
`scripts/release-build.sh --no-pgo` builds; old = the session-l 7413bff binary). Fixes are commits 39e2342, 91a4ecf, 6db451e, 7ada56f.

| program | cause | fix | cycles M: 7413bff / 4e89dd9 / fixed | instructions M: 7413bff / 4e89dd9 / fixed |
|---|---|---|---|---|
| lib/str_eq (5.5 and 5.4) | N1a `ixfuse` payload displacement: Zen 5 front-end bound | `neg` for index offset -1 (39e2342) | 159.1 / 209.8 / **134.7** | 1276.0 / 1175.2 / 1175.2 |
| corpus2/heldout/real_dis_ppc | T8 FINDM (`libcall` bit 11): a `captures()` vector per call, and a formation landing | vector removed (6db451e); landing kept | 2130.9 / 2250.2 / 2238.8 | 7011.0 / 7354.9 / 7309.8 |
| lib/tinsert_idx | T5 (a7439fb): the object-key arm in `tset_core` stopped `set_int` inlining into `jit_tr_tset` | object arm `#[cold]` out of line (7ada56f) | 171.9 / 183.1 / 175.4 | 1041.4 / 1090.3 / 1070.7 |
| corpus2/dev/regex_nfa | `heatside` 48 -> 12 (4e1d026, section 40) | none: trade-off (richards, roguelike) | 640.5 / 673.2 / 667.4 | 3452.8 / 3521.5 / 3521.5 |
| awfy/cd, `cspeed=0` | `uses_gpr(10)` per exit in `stubs_and_tail` (quadratic) | asked once per trace (91a4ecf), identical code | 260.7 / 244.9 (old / new) | 767.7 / 678.3 |

### 56.1 lib/str_eq: fewer instructions, more cycles

The loop is `if S[k] == SR[k]` over two 512-element arrays. N1a (`ixfuse`, 45eb888) folds the `k - 1` into the accesses: the tag compare
becomes `cmp byte [base + k + 3], 0x44` and the payload load `mov rsi, [base + 8 * ~k + 8]`, one `lea` fewer per iteration (23 instructions
instead of 24). Cycles per iteration went from 2.7 (`ixfuse=0`) to 4.2. Branch misses are equal (135 k both); the counters say front end:
`de_no_dispatch_per_slot.no_ops_from_frontend` 458 M slots (7413bff) against 1198 M (8cd1f06), backend stalls 342 M against 125 M, ops all from
the op cache in both. An asm micro-benchmark of the loop body (scratch, not committed) reproduces it only with the loop head included (the poll
`cmp byte [r15 + disp32], 0` and the budget `sub r11, 4; jl`), and isolates the trigger: the payload load with a nonzero displacement and a
scaled index. Same addresses, same instruction count: `lea r10, [r9 - 1]` + `[base + 8 * ~r10]` 129 M cycles, `[base + 8 * ~r9 + 8]` 204 M;
the tag compares with displacement 3 straight off `r9` are fast (129 M) as long as the payload loads carry no displacement. Not alignment (a
4-byte nop in place of the `lea` stays slow), not the index register (the same loop through a copy of `r9` stays slow). Most likely the op
cache's shared immediate / displacement fields: five memory ops with displacements plus immediates in one 64-byte window.

Fix: `payload_scaled_c` emits `mov V7, x; neg V7` and no displacement when the fused offset is -1 (the 1-based `t[i]`, the common case):
`~(x - 1) == -x`. Same instruction count as before; lib/str_eq 209.8 -> 134.7 M cycles (also 15 % under 7413bff), str_eq_long unchanged
(34.7 -> 35.5, noise). Other offsets keep the displacement form. Mutants `ixfuse_pay_disp_pos`, `b2arr_no_not`, `b2arr_off8` take the
old path so they still bite.

### 56.2 real_dis_ppc: T8 FINDM

`libcall=2047` (bit 11 off) gives 7004 M instructions and 2129 M cycles, which is all of the regression. Two parts: (a) `findm_core` built the
`captures()` `Vec` on every call to read its first element; now it asks `ncaptures` and `get_onecapture(0)` (-45 M instructions, -0.6 %);
(b) formation: with `s:match(...)` on trace the `out` callback's root trace reaches the `mnem[m]` GETTABLE, whose exit (749 k times) gets
side traces that abort on `Nyi(Call)` and is blacklisted; without FINDM that exit is covered by a side trace. Part (b) is a formation landing,
not fixed here (bit 11 still wins on the programs it was built for, section 44.2).

### 56.3 lib/tinsert_idx: T5

No knob moved the count; the commit bisect lands on a7439fb (T5, knob `t5` off by default). The traces are identical before and after (IR and
code size), the change is `jit_tr_tset`: `tset_core` with both the integer and the object arm. With the object arm moved to a `#[cold]
#[inline(never)] tset_obj` the helper is again the 7413bff body (inline array store, `set_int_hash` call) plus the key-tag test: 1090.3 ->
1070.7 M instructions, 183.1 -> 175.4 M cycles. The rest (+29 M against 7413bff) is not in `jit_tr_tset`; sampled profiles move it between the
helper and TRACE-1 (instruction sampling has skid here, no precise event on this machine). Not chased further.

### 56.4 Skipped: layout, noise or no regression at 4e89dd9

- 5.4 lib/gsub_tbl (+4.3 % cycles at -1.3 % instructions here): the same in interpreter-only mode, no JIT knob moves it, and 5.5 with the same
  source change is faster (169.4 -> 160.7 M): binary layout of the 5.4 build.
- 5.4 lib/fmt_q (145.2 old / 141.8 new), programs/table_hash (502.2 / 503.9), programs/pcall_error (1442.9 / 1434.4, fixed build 1457.7
  at identical instructions), 5.5 lib/error_tbl (277.2 / 276.8): no regression at 4e89dd9 in min-of-5 cycles.
- 5.4 stream_closures: 3680.6 M instructions at 7413bff and 3612.1 M at 4e89dd9 here (cycles 833.1 / 803.2): no regression in this
  environment. `blfirst=0` saves 13 % of the instructions (3143.9 M) but not cycles (809.7 M): the blfirst landing is a wash, kept.
- 5.5 programs/string_ops (26 % spread) and corpus/classic_vector (equal instructions): layout, as the tables say.

Verification: `scripts/jit-diff.sh 1 10` with `off tr trdef` on 5.5 and 5.4 (0 diffs); `ccheck=1` on awfy/cd (278 compared, 0 differ) and
apps/raytrace (56, 0); `scripts/gate-light.sh ixfuse_,findm_,tset_,b2arr_ --fast`: every step passed (11 mutants killed) except `fmt` and
`mutant-map`, both fixed in the follow-up commit (rustfmt line break, regenerated `scripts/gate-mutant-map.tsv`; rechecked OK).

## 57. Session-n regressions: `rapol` bit 2 (`rbp` allocatable) side effects (2026-11-25, base 80d4fa1)

Method: perf stat `cycles:u,instructions:u` on CPU 4 (`taskset -c 4`, 300 ms spin, `RUSTYNAIL_SEED=0`, equal-length binary paths
and environments, min of 5-7, binaries rotated). A heavy gate ran on CPUs 8-15 during the session: absolute cycles moved by up to 2x between
series (oop_inherit 49-99 Mcyc for the same binary), so every comparison below is within one series; instructions are deterministic.

Re-measurement of the shipped binaries (session-l 7413bff / session-m 8cd1f06 / session-n ab7d185, min of 7, one series):

| program | bin-l cyc | bin-m cyc | bin-n cyc | bin-n / bin-m ins |
|---|---|---|---|---|
| jit-trace/oop_inherit | 83.4 M | 84.6 M | 98.8 M (+17 %) | 0.914 |
| jit-trace/dot | 1813 M | 1770 M | 2168 M (+22 %) | 0.999 |

Both are real. Knob bisection on bin-n: `rapol=1` (bit 2 off, `rbp` not allocatable; default 5 since c0bf0ce) gives dot 2243 -> 1790 Mcyc and
oop_inherit 98.6 -> 76.1 Mcyc (at +5 % instructions: more spills). Two different mechanisms, both new code shapes that bit 2 produces:

* **dot** (inner loop `s = s + a[i] * b[i]`): the loop-invariant array part pointer of `a` became a loop home in `rbp` (`make_homes` takes the
  pool reversed, `rbp` first). `rbp` as a SIB base has no disp-less encoding, so the payload load is `movsd xmm1, [rbp + r8*8 + 0]`: the
  scaled-index-plus-displacement form of section 56 (ixfuse), again a front-end stall at identical instruction count. **Fix**: `ra::Ra::idxbase`
  marks the base operands of fused `ALOAD` / `ASTORE` with a register index; for them `make_homes` moves `rbp` to the end of its pool (it is
  still taken when nothing else is free).
* **oop_inherit**: the extra register changed the loop-back parallel moves into a memory-to-memory 2-cycle (two spill slots swapped). With a
  memory-to-memory step present, `sequentialise_x` saved the cycle's destination in the free double register: `movsd xmm0, [slot A]` (a value
  just stored by an integer `mov`) ... `movsd [slot B], xmm0`, then read by `mov r8, [slot B]` at the top of the next iteration. The carried
  integer chain crossed the GPR/FP domain twice per iteration through memory (no memory renaming / slower forwarding on Zen 5): +29 % cycles
  at -2.6 % instructions against the slot save. **Fix**: the double scratch register only saves double-register destinations; a general register
  or slot destination of a cycle with a memory-to-memory step is saved in the scratch slot (`mov rax, [A]; mov [scratch], rax` ... all in the
  integer domain), as when no double register is free.

After the fix (b0 = 80d4fa1, b2 = fix; same series, min of 5):

| program | b0 cyc | fix cyc | b0 ins | fix ins |
|---|---|---|---|---|
| jit-trace/dot | 2623 M | 1937 M (-26 %) | 6509.9 M | 6509.9 M |
| jit-trace/oop_inherit | 98.8 M | 88.0 M (-11 %) | 226.7 M | 232.7 M |
| jit-trace/oop_inherit (2nd series, with bin-m) | 95.9 M | 49.5 M (bin-m 57.5 M) | 226.7 M | 232.7 M |

oop_inherit now runs below session-m in the same series (the -8.7 % instructions of session-n are partly given back: +2.6 %).

Not changed by the fix and not caused by `rapol` (rapol=1 vs 5 within 2 %, instructions equal), min of 3 in one series, bin-m / b0 / fix:
lib/len_op 85.8 / 91.2 / 90.3 Mcyc (instructions equal), lib/it_next 81.8 / 85.5 / 85.2 (equal), real/template_engine 1151 / 1191 / 1212
(+0.5 % ins), programs/metamethods 1139 / 1140 / 1156 (-0.4 % ins): within the cross-series noise of this session (a gate ran on the other
CPUs); left as layout / noise. lib/find_pat 360 / 378 / 390 Mcyc at +3.3 % instructions (1292.8 -> 1335.5 M): a real instruction increase
from the runtime side (rn-std pattern / FINDM changes between 8cd1f06 and ab7d185), not bisected in this session.

Verification: `cargo test -p rn-jit --features trace --test tr_ra` (19 passed), `scripts/jit-diff.sh 1 10` with `off tr trdef` (0 diffs),
`scripts/gen-mutant-map.py` regenerated (line shifts). `scripts/gate-light.sh phi_,cycle_,spill_ --fast` was started but stopped before
its first test step (it sat in `gate_wait_load` behind the heavy gate on CPUs 8-15, past the time box): still to run.

## 58. First `gate-heavy.sh --fast` oracle run: the 7 failing `oracle_options` jobs are harness artefacts (2026-11-25, base 80d4fa1)

The first heavy gate on 80d4fa1 failed 7 of 4056 `tr_oracle::oracle_options` jobs. Both causes predate every knob added since 7413bff and
neither is a trace-compiler bug; the two programs join the existing exclusion lists of `crates/rn-jit/tests/tr_oracle.rs`.

- `tests/vm/jit_t5_01_object_keys` (cells `+exitstorm=2`, `+exitstorm=7 budget=3`, `-loop -fwd -abc -sink`; P8A-MISMATCH at pc 147 `ADD`,
  pc 173 `ADDI`): pc 147 is `for k, v in pairs(strong) do n = n + v end` (line 313), whose keys are tables. Their node positions depend
  on object addresses, which differ between the oracle's two runs, so the partial sums differ at the same clock (the section 29 class,
  `ADDR_ORDER`). Evidence: with that loop changed to `n = n + 1` the pc 147 mismatches go away; the same job with traces off
  (`TR_OR_MODE=t1 TR_OR_BUDGET=3`) shows the same pc 147 mismatch, and the pc 173 one (the `pairs(fs)` loop over closure and thread
  keys, a constant offset of 5) also shows with traces off. The mismatches are flaky across runs (another layout), they show at a7439fb
  (the commit that added the program), and `rnlua` prints the same output with traces off and in all three option sets. Now in `ADDR_ORDER`.
- `tests/vm/e4_02_lazy_loop_errors` (the four `budget=3/17` cells; CRASH `memory allocation of 1048576 bytes failed`): the program
  overflows the stack in a recursion inside a loop. With a stay budget the traces end at every depth and every stay checkpoint digests
  the live slots of all frames (`Dig::live`), so the checkpoint log grows with depth squared: the plain `always budget=17` cell fails the
  same way, more than 6.5 GB (`TR_P8A_AS_MB=6500`) is not enough, while the cells without a budget peak at 88 MB. It fails identically at
  7413bff and a7439fb, so `codemap` / `memtune` / the new default knobs are not involved. The oracle children's 2 GB `RLIMIT_AS`
  (`TR_P8A_AS_MB`) is right; the program joins `MEM_HEAVY` beside `err_08_stack_overflow` (vm_corpus and the identity test cover it).

The 40 `inconclusive` jobs are all in the same four budget cells (300 s leash, timeouts): the four `LEASH_PROGS` and the deep-recursion
error programs `e1_02`, `e5_02`, `e9_01`, `h7_04`, `k6_04` (the same checkpoint growth, slower instead of over the cap; `e5_02` and
`h7_04` under `-fold -cse budget=17` also hit the 300 s leash with the a7439fb oracle binary, before every section 50-56 knob). The heavy gate
would have reported both failures at 7413bff already; it had not been run on this code before.

## 59. Library fast paths M1 `tmovepre`, R1 `tpop` and recorder handles B1 `pureb`, B2 `opencall` (2026-11-25, base bb9bd4d)

Production versions of four prototypes of the runtime-gap study (branch `worktree-agent-a943255528a978664`, commits 61dc6bb / f7fbba6 / bebc427,
which sat behind `RN_PROTO` bits; none of that code is taken). Not built: B3 (dropping the `pc - 1 ..= pc + 1` local neighbourhood of the
`snapprune` liveness: it changes what `debug.getlocal` sees in a hook), the plain-`find` variants, the one-load `getmetatable` and the tier-1
doubling ceiling (slower or killed in the study). Every handle is exact: the same results, errors, borders, traversal order and memory as
with the handle off.

* **M1 `tmovepre`** (rn-std `RUSTYNAIL_STR_FAST` bit 32 `TMOVEPRE`, in the default mask). `table.move(a, f, e, 1, b)` with `b` empty (no
  metatable, no array part, the dummy node), `a ~= b`, `a[f..e]` in `a`'s array part and **every value non-nil**: `b` first gets the array
  part the element copy grows to, `2^ceillog2(n)` with no hash part (`TableRef::move_presize`; every insertion of key `k` into the full
  array `1..k-1` resizes to `2^ceillog2(k)`: 5.5 `rehash_append` / `rehash_dummy_int`, 5.4 `computesizes`), so the N2 bulk copy
  (`arr_copy_to`) applies. A nil in the range is not inserted by the copy and changes the growth, so holes, a destination offset other than 1,
  a non-empty or metatabled destination and a range outside the array part keep the element loop. A failed allocation leaves `b` empty
  (the loop then fails as before). Mutant `tmove_presize_holes` (hole test dropped: `collectgarbage("count")` of the copy differs from the
  same copy into a metatabled destination).
* **R1 `tpop`** (`STR_FAST` bit 64 `TPOP`, default on). `table.remove(t)` with one argument and a table without a metatable: raw border,
  raw read, push, raw store of nil at `size` (also `size == 0`: `t[0]` is read and cleared, as the general code does). Mutant
  `tpop_meta_skip` (the metatable test dropped).
* **B1 `pureb`** (knob `pureb`, `PUREB_PROCESS = 1`). A recorded `string.byte(s, i, j)` range of 2..8 results guards `j <= #s` once
  (a `TLEN`, beside the existing `i >= 1` and `j == i + n - 1` guards) and emits its `LIB BYTE` ops without the per-result `NE 0` guard,
  flagged `F_PUREB`: not DCE roots in `opt_loop`, so dead results are dropped. With the three guards every position is in the string and
  the helper never answers 0. Mutant `pureb_len_guard_skip` (no `j <= #s` guard: a shorter string yields `-1` instead of an exit).
* **B2 `opencall`** (knob `opencall`, `OPENCALL_PROCESS = 0` (shipped OFF, see the session-o note below)). In the `snapprune` bytecode liveness (`live.rs`, section F4) a `CALL` with
  `C == 0` defines every register from `A` up: its results are `A .. top`, the open reader that always follows reads only those, and the
  registers above are free temporaries the code generator writes before reading. Only dead numeric temporaries can become prunable, the
  F4 relaxation unchanged (locals and their neighbourhood stay protected). The per-prototype cached analysis is the one of the process
  default; a state with the other value computes its own. Mutant `opencall_defs_low` (defines from `A - 2`: kills the pending operand of
  an enclosing expression).

**Measurements** (one release binary, `perf stat` cycles:u / instructions:u, min of 5, `taskset -c 4`, `RUSTYNAIL_SEED=0`, equal-length
environment: `RUSTYNAIL_STR_FAST=023|055|087|119`, `RUSTYNAIL_JIT_TRACE_LIMITS=pureb=0,opencall=0|pureb=1,opencall=1`; `off` = all four off):

| program | off Mcyc / Mins | handles | on Mcyc / Mins | cycles |
|---|---|---|---|---|
| lib/tremove_pop | 243.0 / 1482.7 | M1 | 174.0 / 1039.8 | -28.4 % |
| | | R1 | 205.7 / 1252.5 | -15.4 % |
| | | M1+R1 | 135.8 / 809.6 | -44.1 % |
| lib/byte_multi | 38.78 / 268.5 | B1 | 34.65 / 251.3 | -10.6 % |
| | | B1+B2 | 33.41 / 248.4 | -13.8 % (B2 -3.6 % on B1) |

Regression sample, all four off -> on (Mcyc; instructions): lib/byte 44.49 -> 44.31 (-0.05 %), lib/tmove 101.57 -> 101.43 (+0.003 %),
lib/tmove_1k 97.55 -> 98.18 (+0.04 %: ~19 instructions per `table.move` into a non-empty destination, the bit and table tests),
lib/tremove_front 145.13 -> 145.33 (-0.01 %), programs/json 1810 -> 1825 (-0.01 %), programs/string_ops 1373 -> 1310 (-0.01 %),
programs/table_array 358.8 -> 358.3 (-0.10 %), programs/fasta 1275 -> 1287 (-0.02 %), awfy/deltablue 106.1 -> 104.0 (-0.89 %),
awfy/havlak 204.8 -> 190.7 (-0.71 %). No program over +1.0 % cycles (json +0.8 %, fasta +1.0 %, tmove_1k +0.6 %: layout noise at flat
instructions); the largest instruction rise is +0.04 %. All four ship on. Every output equals its `.expected`.

**Tests.** `tests/vm/tables_21_move_presize_pop.lua` (dense and holed sources of every size class against a metatabled destination:
borders, `pairs` order, and `collectgarbage("count")` when `S59_COST` is set, which only the in-process detector does: under the GC-stress runs of the vm corpus a collector step inside the measurement frees unrelated garbage; offsets, sub-ranges, hash-part ranges, non-empty / same-table / empty ranges;
`__index` on the source, `__newindex` / `__len` / `__index` on the destination; negative, overflowing and wrapping ranges; `remove` of
empty, `[0]`-only, holed, appended (`t[#t+1]`), shrinking and sparse tables, positional forms, out-of-bounds and non-table errors,
`__index` / `__newindex` / `__len` / non-integer `__len` metatables; the hot move + pop loop) and `tests/vm/jit_s59_byte_range_opencall.lua`
(ranges with dead results, `i > j`, out of range, negative positions, strings shrinking below `j` under a hot trace; open calls inside
expressions with callees from a table so that the callee guard exits at the `CALL`; a line hook reading every local with
`debug.getlocal` while the open-call loop runs): both equal to the 5.5 reference (vm corpus). `rn-jit` tests `tr_tlib_fast::
tlib_fast_paths_match_reference` (the first script in-process, bits on and off, against `tests/data/s59_tlib{,54}.expected` from the
references) and `tr_core::s59_byte_opencall_programs` (the second as `s59_byte_opencall`, knobs on / off / lockstep, `+verify`, exit
storms, budgets, `-loop`, `+emurun`, GC stress 1 and 2, stack-move stress). Each of the four mutants is caught by its test.

**Session-o note (2026-11-26): `opencall` ships OFF.** The session-o benchmark showed `lib/rep_sep` +8 % cycles and +15.7 % instructions against session-n on all four builds
(5.5 plain 432.2 -> 471.9 M instructions); `opencall=0` restores 433.2 M and 86.6 M cycles (93.7 M with it on, min of 3). The snapshot at pc 71 keeps 3 slots instead of 4 and the
side-trace chain of the `rep` loop forms differently (14 traces against 11). B2 gained only 3.6 % on `byte_multi` (B1 alone: -10.6 %), so with the regression bar missed
the default is OFF; B1 `pureb`, M1 and R1 stay ON. The cause of the different chain (a real loss of the pruned shape or formation luck) is not investigated.

## 60. Root choice on the minority direction of a loop branch: `reroot` (awfy/queens; 2026-11-25, base bb9bd4d)

Task: awfy/queens loses 24 % of its cycles to the formation lottery of the loop root at `queens.lua:16` (`RUSTYNAIL_JIT_TRACE_HOT=128` against the default
256). **Result: the cause is the path the root records, not the side-exit linking; the handle `reroot` (re-record once, steered away from the old path)
gives queens -19.3 % cycles / -22.1 % instructions and is built, tested and default OFF (`REROOT_PROCESS = 0`): awfy/cd regresses +2.9 .. +3.8 % cycles.**

### 59.1 Cause (measured with `RUSTYNAIL_JIT_EXITDUMP=1` and `RUSTYNAIL_JIT_VERBOSE=2`, release `rn-cli`)

* The loop body is `if self:getRowColumn(r, c)` with `getRowColumn` = `freeRows[r+1] and freeMaxs[c+r+1] and freeMins[c-r+8]` inlined. At HOT 256 the
  iteration that makes the head hot has `freeRows[r+1] == true, freeMaxs[..] == false`; the root records that path (50-instruction pre-roll, guards
  `ALOAD true` at `freeRows` and `ALOAD false` at `freeMaxs`, pre-roll exits S17 / S19, body exits S11 / S20). At HOT 128 the iteration has
  `freeRows[r+1] == false`: one guard `ALOAD false` (39-instruction pre-roll). The two directions are about equally frequent (S17 + S11 = 1.22 M
  exits of the bad root, S9 + S14 = 1.12 M of the good one), but the bad path needs two branch outcomes (about 25 % of the iterations), the good one
  one (about 50 %).
* Every iteration off the root's path goes: pre-roll up to the guard, exit (plain `slink` link into the side trace), side trace, writeback and
  **link to the root's entry** at the head (`Rec::close`: `self.link = Some(at)`, the plain link snapshot; the side trace never enters at the
  `LOOP` label). On the bad root nearly every entry leaves in the pre-roll: entries 1.56 M, pre-roll exits into side traces 1.37 M (88 %; S17 alone
  939 k = 60 %); on the good root entries 0.80 M, pre-roll exits 0.30 M (38 %). The common iteration costs a full pre-roll + hop on the bad root and
  a 7-instruction body iteration on the good one.
* `slink=2` links only the exit -> side direction (50.2); the side -> root direction always goes through the root's entry and pre-roll (as in LuaJIT,
  whose side traces also link to the root's start). The existing rules do not fire: the ineffective rule counts only the uncovered pre-roll exits
  (`ineffcover`: p = 0 here), the hop / C36 `domtop` rules need 90 % of the entries in pre-roll exits (88 %) and 82 % in one site (60 %).
* Linking side traces to the `LOOP` label (candidate (i)) is not built: the body expects the hoisted invariants of the pre-roll in registers
  (`freeRows` / `freeMaxs` array pointers and sizes, `c`), which a pre-roll exit before their definition (S17 precedes the `freeMaxs` loads) does not
  have and which any store or call of the side trace may invalidate; an exact form must re-derive them in the side trace (a pre-roll of its own).

### 59.2 As built (knobs `reroot`, `rerootall`; `RUSTYNAIL_JIT_TRACE_LIMITS`)

* Rule (`cache::pre_exit_check`, counters only, once per head `HeadData::reroot_done`): entries `e >= domn` (65536), the busiest pre-roll site with a
  side trace has `reroot` percent of `e` or more, and all pre-roll exits together have `rerootall` percent (default 80). Then the avoided path is
  computed (`reroot_avoid_path`): the root's recorded path (pc, prototype, depth per step; kept per trace id in `TrState::rr_paths` while the knob is on,
  `cache::reroot_closed`) up to the first step where the side trace's recorded path, aligned at the exit's snapshot pc and depth, leaves it. If the
  side path never leaves the root's (a type-only difference) nothing happens. Otherwise the family is flushed (`FlushWhy::Dom`: no penalty) and the head
  keeps the prefix (`HeadData::reroot_avoid`, `REROOT_TRIES` = 16 drops).
* Constraint (`Rec::on_step`, root recordings only, `RecCfg::avoid`): a recording whose path equals the prefix when it reaches its length aborts
  `Reroot` (`AbortKind::Reroot`: no penalty, the head re-arms with wait 1, `cache::abort_reroot`); after 16 drops the constraint is dropped. A closed
  root recording of the head clears it. The re-record therefore takes the side trace's direction at the branch where the side trace left the old path.
* Counters: `[trace] reroot: roots R drops D given up G` (`RUSTYNAIL_JIT_VERBOSE=1`).
* Exactness: the knob only chooses which iteration a root records (a recording is dropped before it is compiled); no guard, snapshot or link changes.

### 59.3 Measured (release `rn-cli`, one binary, `reroot=0` against `reroot=50`, `taskset -c 12`, min of 9 cycles, instructions of that run)

| program | rule fires | cycles M 0 / 50 | delta | instructions M 0 / 50 | delta |
|---|---|---|---:|---|---:|
| awfy/queens | 1 root (60 % / 88 %) | 90.84 / 73.34 | -19.26 % | 479.11 / 373.18 | -22.11 % |
| awfy/cd | 1 root (83 % / 91 %) | 243.37 / 252.49; rerun 243.55 / 250.67 | +3.75 %; +2.92 % | 648.29 / 660.42; 650.65 / 656.81 | +1.87 %; +0.95 % |
| corpus/astar_jumper | 2-3 roots without `rerootall`, 0 with 80 | 1728.35 / 1691.46 | -2.13 % | 4423.90 / 4162.83 | -5.90 % |
| corpus/containers_lru_heap | 1 root (63 % / 89 %) | 1300.42 / 1265.14 | -2.71 % | 5389.67 / 5278.56 | -2.06 % |
| corpus/penlight_mix | 1 root (65 % / 99 %) | 2233.97 / 2213.76 | -0.90 % | 10236.30 / 10231.98 | -0.04 % |
| corpus2/heldout/stream_closures | 2 roots (56 % / 84 %, 73 % / 110 %) | 760.30 / 769.34 | +1.19 % | 2862.39 / 2888.08 | +0.90 % |
| corpus/lua_lexer | 0 with `rerootall=80` (64 %) | 812.75 / 814.67 | +0.24 % | 3274.84 / 3279.77 | +0.15 % |
| awfy/richards, awfy/towers, apps/raytrace | 0 | +0.67 / -1.69 / +0.10 % (min of 7) | | +0.21 / +0.01 / +0.38 % | |
| sample: deltablue, havlak, json, list, nbody, bounce, storage, class_middleclass, json_rxi, binary_trees | 0 | -0.6 .. +4.0 % (min of 7) | | -3.2 .. +2.6 % | |

The rule does not fire on the sample, richards, towers or raytrace: their differences are the run-to-run formation noise of this build (deltablue
305 / 310 / 313 M instructions in three runs of one configuration; astar_jumper 4184 .. 4510 M). richards and towers lose to exits into tier 1
(section 50.4), not to pre-roll hops: `reroot` does not apply to them. **Decision: default OFF** (cd above +2 % cycles and up in instructions).
`reroot=50` gives queens its HOT-128 tree; what stays open is a stabiliser that compares the re-recorded tree with the old one (cd's re-record has a
worse body) before keeping it.

### 59.4 Tests, mutants

* `tr_core::s59_reroot_programs`: `tests/tr_core/s59_progs.rs` (`s59_queens`: the awfy loop; `s59_side_errors`: errors raised on the side path,
  caught by `xpcall` with `debug.getlocal` in the handler, a count hook installed from inside, collections; `s59_phases`: a branch whose majority
  changes between phases, a float type change at the end) and `s50_frame_sides`, `r1_type_flip`, in `always` / `on2` x `reroot=0`,
  `reroot=1,domn=64`, `reroot=1,domn=64,rerootall=0`, `+verify`, `+exitstorm=3 +verify`, budgets 3 / 17, GC stress 1 / 2 in both collector modes,
  stack moves with `+tracemove`; with the oracle feature the instruction-count identity. Asserts the rule fired in `s59_queens` (`rerootall=0`) and
  never with `reroot=0` (at the test thresholds it also fires with drops in `s50_frame_sides` and `s59_queens`).
* Mutant (registry 875 -> 876): `reroot_never`, caught by `s59_reroot_programs`.

## 61. Cheaper loop POLL / BUDGET: the hoisted chunk budget (`budhoist`) and the chunk poll (`pollk`) (2026-11-25, base bb9bd4d)

**Task.** The ceiling study (prototype `RN_HACK_NOPB`, branch worktree-agent-a3df8dadf8615ee45 a5800a4) measured dropping both ops from loop bodies at
-11.4 % cycles on `jit-trace/type_entry`. Goal: get as much as possible of that **exactly** (budget accounting, hooks, count / interrupt / GC
behaviour identical, including at exits).

### 59.1 Design (`asm.rs`: `BhPlan`, `bh_plan`, `bh_setup`, `bh_wrap`, the trampolines next to the `bud_cold` stubs, `cloop_tail`)

Applies to a peeled root loop that has the `cloop` shape (section 41) and no helper call (`ra::has_call`), no `HSHAPE`, a body that starts with
`POLL; BUDGET n` (behind the PHIs), a count PHI used only by the loop control, in the count register at every body exit and never spilled, the
A2 second entry (if any) in front of `LOOP`, and room for the bias slot (extension slot 10). Every body exit is then a guard exit (no helper can
raise an error, run a hook, yield or collect inside the body), and the count is invisible to Lua code.

* **Chunks.** At the loop entry (behind the entry moves) and at every chunk end, `setup` runs with the count register holding the true FORLOOP count
  `c` (`c + 1` iterations left) and the budget register in front of the next iteration's charge. A chunk of `k = min(c + 1, kmax)` iterations is
  charged at once (`budget -= k * n`); the count register runs the chunk (`k - 1 .. 0`) and the bias slot holds `c + 1 - k` (true count = register
  + bias, modulo 2^64, so a count of 2^64 - 1 is exact too). When the budget cannot pay `k * n`, a chunk of one iteration if it pays `n` (bias `c`),
  otherwise control leaves through the stub of the loop's own `BUDGET` guard in exactly the state where that guard fails (head snapshot, budget
  uncharged). So an exhausted budget leaves at the same iteration head as before, with the same budget value: tier 1 then runs out at the same
  instruction (TR5.6).
* **Body.** No `BUDGET` instruction. The `cloop` back-edge (`sub cnt, 1; jae head`) ends the chunk; its fall-through checks the bias: zero = the
  loop really ended (the old fall-through: undo the updates, the loop guard's exit), nonzero = `cnt += bias` (true count of the next head) and back
  to `setup`.
* **Exits.** Every body guard (also the `+exitstorm` forced ones, the loop-ending guard and the kept poll) jumps to a per-stub trampoline:
  `budget += (cnt + extra) * n` (the chunk iterations the per-iteration form would not have charged yet; `extra = 1` for guards in front of the
  `BUDGET` position, i.e. the poll), `cnt += bias`, `jmp stub`. The exit then sees exactly the budget and count of the per-iteration form. Side
  traces and E6 direct links attach behind the trampoline (the trampoline's `jmp` is one of the branches `dlink_attach` re-points).
* **`pollk = K` (b).** The body's `POLL` is dropped; `setup` polls first (before the charge, the per-iteration order) and `kmax = K`, so the poll runs
  at least every `K` iterations. Exactness argument: the poll word `TRACE_POLL` has two writers, `request_interrupt` (signal handler / another OS
  thread) and `State::set_hook` (`lua_sethook`). In a call-free body no code of the running thread other than the trace runs, so within the loop the
  word can only change asynchronously, and an asynchronous event has no defined position in the instruction stream: a poll `j <= K` call-free
  iterations later is the observation of an execution in which the event arrived `j` iterations later, which the per-iteration form also allows.
  A synchronous `sethook` before the loop is seen by the `setup` poll in front of the first iteration, as before. The relaxation is therefore
  latency only (bounded by `K` short iterations), not observable behaviour; if that argument is not accepted, `pollk` stays 0 (it is a separate
  knob).

### 59.2 Measured (release `rn-cli`, one binary, knob settings `budhoist=0,pollk=0` / `budhoist=1` / `budhoist=1,pollk=64`; `RUSTYNAIL_SEED=0`,
`taskset -c 5`, `perf stat -e instructions:u,cycles:u`, min cycles of 7 interleaved runs; outputs identical in all three settings)

| program | cycles off (M) | `budhoist` | `+pollk=64` | instructions off (M) | `budhoist` | `+pollk=64` |
|---|---|---|---|---|---|---|
| jit-trace/type_entry | 1438 | 1377 (-4.2%) | 1296 (-9.9%) | 8538 | -9.1% | -20.1% |
| jit-trace/type_entry_stable | 1356 | 1301 (-4.0%) | 1218 (-10.2%) | 8128 | -9.8% | -21.7% |
| jit-trace/dot | 1216 | 1203 (-1.1%) | 1199 (-1.4%) | 6510 | -12.2% | -22.9% |
| jit-trace/short_inner | 1087 | 1089 (+0.2%) | 1086 (-0.1%) | 4865 | +0.0% | +0.0% |
| programs/matmul | 413 | 409 (-0.9%) | 408 (-1.2%) | 1875 | -6.7% | -12.6% |
| programs/sieve | 532 | 523 (-1.7%) | 512 (-3.8%) | 2546 | -4.1% | -9.0% |
| awfy/sieve | 111 | 109 (-1.6%) | 108 (-2.6%) | 767 | -2.2% | -6.4% |
| programs/spectral_norm | 178 | 178 (-0.1%) | 178 (-0.2%) | 1249 | -3.3% | -6.2% |
| programs/mandelbrot | 532 | 532 (-0.1%) | 532 (-0.0%) | 1637 | +0.0% | +0.0% |
| programs/nbody | 335 | 335 (+0.1%) | 336 (+0.2%) | 1933 | -0.0% | +0.0% |
| programs/fannkuch | 1017 | 1006 (-1.0%) | 1014 (-0.3%) | 4581 | +0.1% | -0.9% |
| programs/heapsort | 1110 | 1093 (-1.5%) | 1093 (-1.5%) | 2619 | -0.1% | -0.2% |
| programs/queens | 1817 | 1842 (+1.4%) | 1841 (+1.3%) | 4973 | +2.5% | +1.6% |
| awfy/json | 135 | 136 (+0.2%) | 136 (+0.1%) | 438 | +0.0% | +0.0% |
| awfy/permute | 35 | 35 (+0.1%) | 35 (+0.1%) | 191 | +0.0% | +0.0% |
| awfy/towers | 44 | 44 (-0.2%) | 44 (+0.0%) | 168 | -0.0% | -0.0% |
| programs/binary_trees | 1174 | 1169 (-0.5%) | 1169 (-0.5%) | 5537 | -0.0% | -0.0% |

`pollk=64` reaches 87 % of the ceiling on `type_entry` (-9.9 % of -11.4 %). **Defaults: both 0.** The bar (no cycle regression > 2 %, instructions not
up) fails on `programs/queens`: +1.4 % cycles, +2.5 % instructions (+1.6 % with `pollk`). Its loops are short (`n = 8`) and mostly left early by a
guard: each entry pays `setup` (about 8 instructions, against the 2 of the first iteration's charge) and each early exit the trampoline, and the
iterations saved do not pay for it. A follow-up could keep the per-iteration form for loops whose recorded body exits early or whose entry count is
small (a run-time `c < 4` test in `setup` that enters an un-hoisted copy is not possible with one body; a static policy from the exit profile is).

### 59.3 The loop body of `type_entry` and the induction updates

Body instructions per iteration: 13 before (`cmp byte [r15+POLL]; jne`, `sub r11, 4; jl`, tag `cmp; jne`, `movsd`, `mulsd`, `addsd`, `add rdi, 1`,
`add r8, -1`, `sub rsi, 1; jae`), 11 with `budhoist`, **9 with `pollk`** (LuaJIT: 9). The three remaining induction updates are the element index
`j` (`rdi`, addresses the tag byte `[rdx + rdi + 3]`), the carried negated index (`r8`, addresses the payload `[rdx + 8 * r8]`: the payload part grows
downwards and x86 has no negative scale) and the FORLOOP count (`rsi`). Reducing them: (1) the count could go if exits derived it (`(limit - j) /
step`), the seven-consumer snapshot change rejected in 41.2; (2) one of the two indexes could go only with a payload layout that grows upwards
(an object-layout change) or by addressing the tag from the negated index (`[rdx - r8 + 3]` is not encodable either). Neither is cheap; not built.

### 59.4 Tests and mutants

`tests/tr_core/s59_progs.rs` (6 programs: nested loops of lengths 0..300 around the chunk sizes, steps; early breaks, type changes, holes, an integer
element in a float loop at every chunk position (the interpreter finishes the loop with the snapshot's count), reads past the end; an error behind a
body exit, caught; count and line hooks set while the traces run with counts 1 / 3 / 5 / 17, the hook recording line and locals; coroutines yielding
between loops; counts near 2^64 and long many-chunk loops). `tr_core::s59_programs` (4 knob settings x 7 programs (with `n1_exits`) x {plain,
`+verify`, `+exitstorm=3`, `+emurun +verify`, `on2`, count budgets 1 / 3 / 17 / 200, GC stress 2}), `s59_count_identity` (`t15_oracle`: the
`fetches + insns` identity under budgets none / 1 / 3 / 17 / 200, with and without `+exitstorm=7`), `s59_programs_enter_traces`. Mutants:
`bh_refund_skip` (caught by `s59_count_identity`), `bh_bias_skip`, `bh_chunk_end_bias_long` (caught by all three). Unobservable variants that were
tried and dropped: ignoring the chunk end (it degrades to an exit through the loop guard with the true count: exact, only slower), a short slow-path
budget test, the poll of `setup` and the poll's `extra` refund (only an asynchronous poll word reaches them).

## 62. Probation check of a `reroot` re-record (`rerootchk`, `rerootkeep`; 2026-11-25, base 4f15581)

Task: make `reroot` (section 60) safe to ship default ON with an exact check that keeps a re-recorded root only when it is better than the flushed one.
**Result: the check is built and separates the programs cleanly, but it cannot fix awfy/cd: cd's cost is the reroot flush itself, not a worse
re-recorded body, and reverting adds a second flush. `reroot` stays default OFF.**

### 62.1 Design (`cache::pre_exit_check`, `HeadData::reroot_old`)

* When the `reroot` rule fires it stores the flushed root's pre-roll exit fraction (all pre-roll exits per mille of the entries, `p_all / e`) in the head.
* The next root of that head is on probation. Its check runs in the same place as every pre-roll rule (`pre_exit_check`, called each time a pre-roll site
  reaches another multiple of `INEFF_EXITS` = 1024 exits). The check runs once, at the first call with `e >= rerootchk` entries since install (default 1024;
  0 = no check). The re-record is **kept** when its fraction is below `rerootkeep` percent (default 75) of the old one. Otherwise it is **reverted**:
  flushed (`FlushWhy::Dom`, no penalty), the path constraint is cleared and the head records unconstrained. `reroot_done` stays set, so the rule
  never fires at that head again. A re-record that rarely leaves in its pre-roll never reaches the check and is kept, which is the good outcome.
* Exactness: counters only (deterministic). The check only decides whether a compiled root is flushed; no guard, snapshot or link changes.
* Counters: `[trace] reroot: roots R drops D given up G kept K reverted V`. `RUSTYNAIL_JIT_VERBOSE=2` logs each decision (`rerootchk: ...`).

### 62.2 Why this metric (release `rn-cli`, `reroot=50,rerootchk=1024`, verbose logs)

| program | old root pre-roll / entries | re-record pre-roll / entries (at check) | ratio | decision |
|---|---|---|---:|---|
| awfy/queens | 882 per mille | 379 (3684 entries) | 0.43 | kept |
| corpus/containers_lru_heap | 897 | 321 (4432) | 0.36 | kept |
| corpus2/heldout/stream_closures root 1 / root 2 | 842 / 1089 | 1006 (2049) / 76 (38215) | 1.19 / 0.07 | reverted / kept |
| corpus/penlight_mix | 992 | 780 (1424) | 0.79 | reverted |
| awfy/cd | 928 | 914 (1510) | 0.98 | reverted |

The fraction separates queens (0.43) from cd (0.98) by a wide margin. The other candidates were not needed. Machine-code size is ASLR-noisy (identical
recordings differ by about 10 B), and side exits per entry follow the pre-roll fraction on these roots.

### 62.3 Why it does not save cd

The cd head (`cd.lua:122`, `RBT:get`'s `while` loop) gets 71102 root entries in the whole run, and the rule fires at 66483 of them (`domn` = 65536). The
re-record lives for about 4600 entries (6 % of the loop's life), so how good or bad its body is cannot matter. What costs is flushing a family of 17
traces and rebuilding it, plus the formation changes that follow; a revert repeats both. Measured (CPU 4, min of 9, `RUSTYNAIL_SEED=0`, one binary):

| program | `reroot=0` cycles / instr M | `reroot=50,rerootchk=0` | `reroot=50,rerootchk=1024` |
|---|---|---|---|
| awfy/cd | 237.57 / 641.24 | 240.72 (+1.3 %) / 645.99 (+0.7 %) | 243.25 (+2.4 %) / 649.85 (+1.3 %) |
| awfy/queens | 93.74 / 496.94 | 75.51 (-19.4 %) / 387.57 (-22.0 %) | 75.30 (-19.7 %) / 387.57 (-22.0 %) |
| corpus2/heldout/stream_closures | 814.28 / 3513.30 | 806.51 (-1.0 %) / 3513.40 | 805.89 (-1.0 %) / 3513.40 |
| corpus/containers_lru_heap | 1274.34 / 5435.20 | 1305.24 (+2.4 %) / 5384.69 (-0.9 %) | 1294.72 (+1.6 %) / 5383.90 (-0.9 %) |
| corpus/penlight_mix | 2223.70 / 10302.47 | 2211.09 (-0.6 %) / 10296.65 | 2225.94 (+0.1 %) / 10306.32 |

(An earlier pair on cd, cycles / instructions M: 231.56 / 641.23 against 236.14 / 645.95, +2.0 % / +0.7 %. stream_closures has the same instruction count in all three columns,
so under `RUSTYNAIL_SEED=0` the rule probably did not fire there. lru_heap's cycles go up while its instructions go down: formation noise.)

**Decision: `reroot` stays default OFF.** cd's instructions go up with and without the check, so the bar fails. `REROOTCHK_PROCESS = 1024` and
`REROOTKEEP_PROCESS = 75` take effect only when `reroot` is on. The rule's decision comes too late in the loop's life on cd. What could fix that:
(i) fire earlier, at a smaller `domn`, which also changes queens' timing; (ii) a rule that also needs the head's entry rate to still be rising (not
available from counters); (iii) re-record the root without flushing the family (keep the old root's side traces for the exits they still serve).

### 62.4 Tests, mutants

* `tr_core::s60_reroot_programs`: new program `s60_worse` (`tests/tr_core/s60_progs.rs`). It is a three-way round-robin branch inside an inlined callee,
  so every direction is a minority and the re-record leaves through its pre-roll as often as the flushed root did. It also has a collection, an error
  raised and caught by `pcall` late in the loop, and a float type change at the end. It runs in the existing matrix (`always` / `on2`; `+verify`,
  `+exitstorm=3 +verify`, budgets 3 / 17, GC stress 1 / 2 in both collector modes, stack moves, oracle identity), with the knobs set explicitly
  (`reroot=1,domn=64,rerootall=0|80,rerootchk=1024,rerootkeep=75`), plus `rerootchk=0` jobs of `s60_worse` and `s60_queens`. Outputs match the
  interpreter in every job. Asserts: `s60_worse` is reverted, `s60_phases` is kept, and `rerootchk=0` never checks.
* Mutants (registry 883 -> 885): `rerootchk_keep_always`, `rerootchk_revert_always`, both caught by `s60_reroot_programs`
  (`scripts/gate-mutant-map.tsv` regenerated).
