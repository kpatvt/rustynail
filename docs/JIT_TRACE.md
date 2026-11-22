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
* **Two E5 mutants survive**. `midfold_reg0_any` (the decision ignores writes of register 0): the record-time rules (`slot0_entry`, the push
  receiver rule) already keep a trace from folding or pushing after its own write of register 0, and outer entries check the current register
  0; only a side trace that starts after such a write could fold on the new receiver, and `e5_01` case (11) did not produce one.
  `midfold_rootret_inner` (no check at root returns): needs a native root return into a caller suspended across foreign code with a receiver
  whose lookup changed; since refused pushes end the recording, `e5_01` case (12) did not produce one. Both rules stay (they are what the
  soundness argument uses); the programs that would catch them are open work.
* The decision rules are formation-dependent: E4's rule looks at which root trace compiles first (in the tests' `always` mode queens' entry
  trace compiles first and is lazy-aware), E5's first-trace rule likewise. Policy only: both orders are sound.
* E5's checked entries call a Rust helper at every outer entry (a family with frequent outer entries pays ~80 instructions each); root returns
  usually take the in-line cache.

## 36. The lazy-chain defect of 35.8 (2026-11-22, base 7b121ea)

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

Mutants: registry 784 -> 785 (`lazy_sidefail_no_leave`, caught by `lz_side_entry_programs` on 5.5 and 5.4).
