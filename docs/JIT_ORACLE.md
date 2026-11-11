# JIT template oracle (J0 item 1)

Status: measured 2026-10-28 on the Zen 5 dev machine. This is a throwaway prototype that answers one question:
**how many cycles does a plain template JIT save versus our interpreter on register-only numeric loops?** It is
not a design and none of its code is meant to be reused (the real templates are specified by `docs/JIT_DESIGN.md`).

## 1. Method

Code: `crates/rn-jit/examples/oracle/` (`decode.rs`, `v1.rs`, `v2.rs`, `main.rs`), programs in
`bench/jit-oracle/*.lua`. Build: `cargo build --release -p rn-jit --example oracle`; run
`target/release/examples/oracle <v1|v1o|v2|dump> file.lua N`.

- The project's compiler (`rn-compiler`, `TestHost`) compiles the Lua file; the oracle takes the first nested
  proto (`local function f(N) ... end`) and decodes its Lua 5.5 bytecode. Supported subset: MOVE, LOADI/F/K,
  ADD/SUB/MUL(+I,+K), DIV(+K), IDIV(+K), MOD(+K), BAND/BOR/BXOR(+K), SHL/SHR/SHLI/SHRI, UNM, JMP,
  EQ/LT/LE/EQK/EQI/LTI/LEI/GTI/GEI (+ the following JMP, fused into one compare-and-branch), FORPREP/FORLOOP
  (int loops with the precomputed count, float loops, both exactly as `vm_unsafe.rs` 5.5), RETURN1; MMBIN* are
  skipped (the fast path steps over them). Any type mismatch jumps to an abort stub that records the pc and exits.
- The compiled code runs once on a hand-made frame (`Vec<[u64;2]>`, 16 bytes per slot, tag word then payload,
  tags `LUA_VNUMINT`/`LUA_VNUMFLT`, kcache in the same layout): no `State`, heap or GC. `rbx` = base, `r14` =
  kcache, `r13` = a small state block (trap word, block counter, abort pc). Entry is an `extern "C"`
  thunk `(base, k, state) -> u64`; the result is the payload of the RETURN1 register and is printed in `lua.c`
  format. **Every oracle result was compared with the real interpreter's printed result and is identical**
  (7 benchmarks at full N, plus two self-tests `selftest_int.lua` / `selftest_float.lua` covering negative `%`,
  `//`, dynamic shifts including counts >= 64 and negative, divisor -1, negative-step float loops, NaN and inf
  comparisons; all three variants match `rnlua` on them).
- **V1 (plain templates).** Every bytecode loads operands from the Value stack in memory, guards the tag byte
  of each register operand (`cmp byte [rbx+off], tag`; constants and immediates are known at compile time so
  they need no guard), computes, stores the payload and then an **8-byte tag-word store** (`mov qword [slot],
  tag`, like `Value::store`) to the destination slot. Registers whose type is unknown get a dual path: int
  first (`cmp tag,INT; jne float_path`), then float; mixed int/float aborts. Nothing is kept in machine
  registers across bytecodes. No `savedpc`, trap or hook stores. Comparisons are fused with the following JMP
  (`cmp; jcc target`). IDIV/MOD with a constant divisor skip the zero/-1 checks (compile-time fold); there is no
  strength reduction (a real `idiv` is executed).
- **V1o (V1 plus the overheads the real design has, `JIT_DESIGN.md` section 5).** Per back-edge (FORLOOP and
  backward JMP): `cmp dword [r13],0; jne exit`. Per basic block head (leaders = pc 0, jump targets, successors
  of branches/loop ends): `sub qword [r13+8], n; js exit` with `n` = bytecodes in the block (MMBIN not
  counted). Still no `savedpc` stores, so it is a lower bound of the real overhead.
- **V2 (register-allocated upper bound).** A forward type inference over the bytecode (int/float per register,
  joined at merges; this replaces the "pre-run type profile") types every register; each (Lua register, type)
  pair that is ever defined gets one machine register for the **whole function** (9 GPRs, 12 xmm available;
  no liveness analysis; mandelbrot_regs uses all 9 GPRs and 7 xmm, so it just fits). No tag guards, no tag
  stores, no stack traffic except the parameter load, no poll, no block counter. It is a general (if
  trivial) allocator, not hand-picked per benchmark. It is an upper bound for tier 1.5 because (a) it applies
  to the whole function, not only loop bodies, with no entry/exit load/store, (b) it has no guards at all,
  (c) the int FORLOOP uses `sub cnt,1; jb` (the count is dead after the loop) and (d) float constants are
  memory operands from the kcache (no hoisting needed).
- **Measurement.** `perf stat -x, -e cycles:u,instructions:u` under `taskset -c 3`, inside
  `systemd-run --user --scope -p MemoryMax=8G -p MemorySwapMax=0 timeout 300`, stdin `/dev/null`, 9 runs per
  (program, binary), the six binaries interleaved inside each run. Median and min below. Binaries:
  `target/release/rnlua` (default release build, generational GC mode as in `lua.c`), `ref/55/build/lua`
  (PUC 5.5.1), the oracle. Each program takes 0.7-1.1 s in the oracle. **Startup/compile cost:** the oracle in
  compile-only mode (`v1c`) costs 0.5-0.7 M cycles and 0.5 M instructions (process start, parse, assemble,
  mmap); it is under 0.2% of every run (smallest run 420 M cycles) and is **not subtracted**.
- The machine ran at about 2.2 GHz during the runs (cycles / wall time); only cycles are compared.

Programs (all register-only, `print(f(N))` outside `f`; N is argv[1]): `int_sum` (N=2e8), `float_poly` (1e8, two
float mul-add recurrences, latency bound), `mandelbrot_regs` (1500x1500 grid, while loop with compare),
`collatz` (7e5 starts, `%`, `//`), `bitops` (1e8 xorshift64), `float_div` (1.5e8, `/` and `+`), and an extra
`float_loop` (3e8 iterations of a float `for`; the oracle needed a float-FORLOOP case so one benchmark
exercises it).

## 2. Results

Cycles:u in millions (median of 9, min in parentheses); speedup = interpreter median / variant median.

| program | interpreter | PUC 5.5 | V1 plain | V1o (+poll+counter) | V2 regs |
|---|---|---|---|---|---|
| int_sum | 4651.8 (4648.3) | 4910.0 | 1379.5 (1370.3) **3.37x** | 1801.3 (1801.2) **2.58x** | 422.2 (417.3) **11.0x** |
| float_poly | 3691.9 (3626.9) | 3975.2 | 3602.0 (3601.7) **1.02x** | 3602.0 **1.02x** | 901.0 **4.10x** |
| mandelbrot_regs | 4206.8 (4079.5) | 4747.4 | 2390.8 (2388.6) **1.76x** | 2407.5 **1.75x** | 547.9 **7.68x** |
| collatz | 4867.4 (4859.5) | 4790.2 | 1760.9 (1749.8) **2.76x** | 1722.0 **2.83x** | 1596.1 **3.05x** |
| bitops | 4448.2 (4443.7) | 4232.1 | 1401.1 **3.17x** | 1401.1 **3.17x** | 600.8 **7.40x** |
| float_div | 4234.2 (4003.9) | 4342.4 | 1949.6 **2.17x** | 1951.4 **2.17x** | 600.9 **7.05x** |
| float_loop | 4734.8 (4729.4) | 4920.8 | 3838.4 **1.23x** | 3841.7 **1.23x** | 1246.4 **3.80x** |
| geomean int (int_sum, collatz, bitops) | | | **3.09x** | **2.85x** | **6.29x** |
| geomean float (4 float programs) | | | **1.48x** | **1.48x** | **5.39x** |

Ratio to PUC 5.5 (cycles, median; lower is better): interpreter 0.95 / 0.93 / 0.89 / 1.02 / 1.05 / 0.97 / 0.96;
V1 0.28 / 0.91 / 0.50 / 0.37 / 0.33 / 0.45 / 0.78; V1o 0.37 / 0.91 / 0.51 / 0.36 / 0.33 / 0.45 / 0.78;
V2 0.09 / 0.23 / 0.12 / 0.33 / 0.14 / 0.14 / 0.25 (order: int_sum, float_poly, mandelbrot_regs, collatz,
bitops, float_div, float_loop).

Instructions:u in millions (median), interpreter / PUC / V1 / V1o / V2:
int_sum 32601 / 31601 / 8201 / 9001 / 2401; float_poly 23901 / 23801 / 5101 / 5501 / 1501;
mandelbrot_regs 26530 / 26596 / 6907 / 7373 / 1414; collatz 20002 / 18382 / 3901 / 4802 / 1945;
bitops 30001 / 28301 / 6801 / 7201 / 1901; float_div 24751 / 24901 / 6901 / 7501 / 1651;
float_loop 32401 / 30001 / 8701 / 9901 / 3601.

Readings:

1. **Integer loops: a plain template JIT gains 2.8-3.4x** (geomean 3.1x), above the plan's 2-2.5x bucket.
   collatz is bounded by the 64-bit `idiv` (V2 is only 3.05x there: the divide chain, not the interpreter,
   is the limit; a real tier 1 would strength-reduce constant divisors, which this oracle deliberately does not).
2. **Float loops: plain templates gain only 1.0-2.2x** (geomean 1.48x; plan 1.3-1.5x, confirmed). float_poly
   gains nothing (1.02x): its recurrences are chains through the Value stack, and every hop costs a
   store-to-load forward (36 cycles per iteration in V1 vs 9 in V2). Instruction count falls 4-5x in V1 yet cycles
   do not: the review's observation ("instruction counts predict almost nothing on this CPU") holds.
3. **Keeping values in registers is the big lever for floats** (V2: 3.8-7.7x, geomean 5.4x) and even for ints
   (6.3x geomean, 11x on int_sum), well above the plan's "2x with registers" for floats. As an upper bound it
   ignores guards, entry/exit traffic and anything that is not a pure register loop.
4. **Block counter + back-edge poll cost 0-31%** in V1: nothing measurable in five programs (the counter's
   read-modify-write chain hides under the loop body's own latency), but +31% on int_sum, whose loop is the
   shortest (about 7 cycles per iteration in V1, about 9 with the counter). On a loop this tight the
   `sub [mem],n` store-forward chain becomes the critical path. Note this is only the counter and poll: the real
   design also stores `savedpc` at exits and helper calls, which this does not emulate.
5. The interpreter runs at 0.89-1.05x PUC cycles on these programs (geomean about 0.96), a little worse than
   its 0.86 on the 25-program suite: pure register loops are where PUC's dispatch is relatively good.

## 3. Projection with the plan's bucket model

(Updated with measured table and call numbers in section 6; this section is the register-only first pass.)

Method: per-program cycles ratio vs PUC from `bench/results/quiet-55.json` (default build, 25 programs, geomean
0.859 now; numeric subset 0.983). Buckets (my assignment, uncertain for mixed programs): integer = loops_arith,
sieve, fannkuch, queens, heapsort; float = mandelbrot, nbody, matmul, spectral_norm; calls = fib, closures,
oop_methods, varargs (x1.2); runtime-bound = the other 12 (x1.025). The bucket speedups are applied to the
interpreter's ratio. "Derated" takes the square root of the measured gain, because 8 of the 9 numeric-subset
programs do **table** accesses that this oracle does not measure (inline array fast paths, bounds, barriers
are part of tier 1 but not part of this experiment) and real programs mix int and float work.

| scenario | int x | float x | numeric subset vs PUC | all 25 geomean vs PUC |
|---|---|---|---|---|
| plan model, low (2 / 1.3 / calls 1.15 / rt 1.0) | 2.0 | 1.3 | 0.595 | 0.701 |
| plan model, high (2.5 / 1.5 / 1.3 / 1.05) | 2.5 | 1.5 | 0.493 | 0.628 |
| **tier 1 measured V1, taken at face value** | 3.09 | 1.48 | **0.441** | **0.618** |
| tier 1 measured V1o (with poll/counter) | 2.85 | 1.48 | 0.461 | 0.628 |
| tier 1 measured V1o, derated | 1.69 | 1.22 | 0.673 | 0.720 |
| tier 1.5 with the plan's numbers (2.5 / 2.0) | 2.5 | 2.0 | 0.434 | 0.615 |
| tier 1.5 measured V2, taken at face value | 6.29 | 5.39 | 0.167 | 0.436 |
| tier 1.5 measured V2, derated | 2.51 | 2.32 | 0.406 | 0.600 |

Interpretation, with the uncertainty stated:

- **Overall geomean for tier 1: 0.62-0.72 vs PUC** (the plan's review model said about 0.64, range 0.58-0.72;
  consistent). This is below the section 9 stop threshold of 0.75 for J0, so the **go/no-go criterion on the
  oracle is "go"**, but the margin is thin in the derated case (0.72 vs 0.75), and what moves it is the
  runtime-bound and table-heavy programs that this oracle cannot measure.
- **Tier 1 numeric target (<= 0.45x PUC): not demonstrated.** It is reached only if the measured register-only
  gains carry over unchanged to the table-heavy numeric programs (0.44-0.46, right at the line). With any derating it
  is 0.55-0.67. The honest reading: **achievable only for the pure-integer/float register loops; unlikely for the
  full 9-program subset with plain templates alone.** The plan's own model (0.49-0.60) said the same.
- **Tier 1 all-programs target (<= 0.70x):** 0.62-0.72, borderline; depends on the 12 runtime-bound programs
  staying at 1.0x (any JIT call overhead there pushes the wrong way) and on the interpreter-entry check cost.
- **Tier 1.5 numeric target (<= 0.30x): plausible but unproven.** V2 at face value gives 0.17, derated 0.41. The gain
  that matters is exactly what V2 isolates (keeping values in registers: float loops 1.5x -> 5.4x), but the
  real tier 1.5 has guards, entry/exit write-back, table loads/stores that stay in memory, and calls that
  spill. Reaching 0.30 needs the table-heavy programs to get more than the square-root share of the V2 gain.
  Tier 1.5 all-programs (<= 0.60): derated 0.60, i.e. on the line.
- **Where the risk concentrates:** floats. Plain templates alone give float loops almost nothing (float_poly
  1.02x, float_loop 1.23x); tier 1 will look weak on nbody/spectral_norm/mandelbrot until tier 1.5 exists, and
  the plan already gates them separately. Integer templates over-deliver relative to the plan.

## 4. Caveats and limitations (all of them)

- Register-only programs: no tables, calls, upvalues, strings, globals, metamethods, allocation, GC safepoints.
  These are the programs most favourable to a JIT. The numeric subset of the plan is mostly table code, so the
  derated rows are the more realistic ones; the "face value" rows are upper estimates.
- Only 7 programs, hand-written by me for this experiment; geomeans over 3 int and 4 float programs. The bucket
  assignment of the 25 programs is mine. Calls (x1.2) and runtime-bound (x1.025) factors are taken from the
  plan, not measured.
- V1 omits `savedpc`/`ci.trap`/hook handling, helper-call exits and the instruction-limit/hook count beyond the
  emulated counter and poll; real V1 is slower. The emulation puts the counter `sub` in memory with a 64-bit
  operand (the design says dword; same cost) and aborts instead of exiting through exit stubs.
- Type handling in V1: dual int/float paths with int first; a real tier 1 would also have the slow-path exit
  stubs and per-bytecode metadata, which cost code size, not hot-path cycles. Mixed int/float arithmetic and
  float loops with an int limit abort (not exercised by the benchmarks).
- V2 is a whole-function allocator with static types: no guard failures can happen, no write-back at exits is
  measured, no register pressure beyond what these small functions need (9 GPR / 12 xmm available; no spilling
  implemented). It is an upper bound, not an estimate. Float constants not in K (`LTI` with a float immediate)
  are rebuilt each iteration with `mov rax,imm; movq` in V2, which slightly understates a hoisting allocator.
- `movsd xmm,[mem]` loads zero the upper half; `cvtsi2sd` is preceded by `xorpd`; V2 uses `movaps` for
  register copies (hand-encoded). I did not examine false dependencies beyond that.
- The measured machine is a single Zen 5 laptop-class part at about 2.2 GHz with `powersave`; results on Intel or
  on other Zen generations (store-forwarding latency, memory renaming) may differ, especially the float V1 numbers
  that hinge on store-to-load forwarding. The SMT sibling of core 3 was not controlled.
- Medians over 9 runs were extremely stable (min within 0.1-1% of median except the interpreter's mandelbrot
  and float_div, which varied by 3-6%); the +-5% band of the plan applies to claims about small differences.
- Correctness: results match the interpreter exactly on all programs and self-tests; the oracle is not fuzzed.
- Encoder gap found: no `cmp m8, imm8` (used for every tag guard); the oracle hand-encodes it with
  `emit_bytes` (`80 /7 ib`, rbx base only). main has since gained `cmp_m8_i`; not adopted here, bytes are
  equivalent. No other form was missing. Tag guards could also be done as `cmp dword` since the upper tag-word
  bytes are always zero, but I used the byte compare the design specifies.
- Unsafe code is confined to `main.rs` (executable memory entry call, `snprintf`), with SAFETY comments.

## 5. Conclusion for the J0 checkpoint

- A plain template JIT roughly **triples integer-loop speed and gives 1.0-2.2x on float loops**; per-block
  counting and back-edge polling cost little except on the very shortest loops (one -31% case).
- Register allocation inside loops is worth 2-4x more again (tier 1.5 is justified by the data, mainly for floats).
- The J0 oracle criterion (projected geomean not above 0.75x PUC) passes: **0.62-0.72**.
- The tier-1 numeric-subset target (0.45x) is at the edge of what plain templates deliver even on ideal register
  loops, and should be treated as a stretch; the all-programs target (0.70x) is borderline. The tier-1.5
  targets (0.30x / 0.60x) are consistent with the data but not demonstrated until table access is measured.
  Recommended follow-up before J1 commits to the numbers: extend the oracle with array `GETTABLE/SETTABLE`
  fast paths (sieve, matmul) and a `fib`-style call, which are the missing buckets.

## 6. Extension: tables and calls (J0 follow-up)

Status: measured 2026-10-28/29 on the same machine with the same protocol. Section 5 recommended measuring the two
missing buckets before J1 commits to numbers: inline table access and Lua-to-Lua calls. This section does that and
restates the projection. Sections 1-5 are unchanged and still describe the register-only experiment.

### 6.1 What was added

- `crates/rn-core/src/jit_layout.rs` (registered in `lib.rs`): the **single source of layout truth** for the JIT
  (plan decision D2). `pub const` offsets computed with `offset_of!`/`size_of` for `Value`, `Table` (array pointer,
  `asize`, `lsizenode`, `flags`, node pointer, metatable), `Node`, the short-string hash, `LClosure.p`, `Proto`
  (`numparams`, `maxstacksize`, `gclist`; the `k`/`code` data pointers through `proto_layout_probe()` because the field order inside
  `RawBuf` is a Rust layout detail) and `CallInfo`. No private field was made public: fields private to `table.rs` and
  `value.rs` are derived from the `repr(C)` order and checked against real objects by the unit tests, which also pin
  `size_of::<Table>() == 48`, `Value == 16`, `Node == 24`. The module compiles and its tests pass under
  `--cfg rustynail_lua54`; the offsets are identical in both versions. What differs between versions is behaviour,
  exposed as constants: `ARRAY_ALIMIT_TOUCH` (5.4 updates the `alimit` hint on every array access, so a 5.4 template cannot
  inline the array path blindly) and `TABLE_ALL_HASH_HAVE_LASTFREE`.
- `crates/rn-jit/examples/oracle/`: `staterun.rs` (new, State-based harness), `decode.rs` (+GETTABLE, GETI, GETFIELD,
  SETTABLE, SETI, SETFIELD, TEST, LOADFALSE/LOADTRUE, CALL; CLOSE is decoded as the dead code it is after a `break`), `v1.rs` (templates,
  multi-function compile, CALL and RETURN1), `v2.rs` (table parameters and booleans). The register-only modes are
  unchanged and were re-verified against `rnlua` (7 programs, 2 self-tests, all three variants).
- Programs in `bench/jit-oracle/`: `sieve`, `matmul`, `heapsort`, `nbody` (tables), `fib`, `calloop` (calls), and the
  `while`-loop variants `sieve_w`, `dot_w` that fit the V2 allocator.

### 6.2 Method

**Harness.** Every new program has the shape: `local function f(...)` (the hot function, first), optional callee
functions, `local function setup(N)` (last), then `if ORACLE then return f, ..., setup end`, then an ordinary
`print(f(setup(N)))` so the same file runs under `rnlua` and PUC. The oracle creates a real `rn-std` state, runs the file
with the global `ORACLE` set (collector stopped), runs `setup(N)` in the interpreter (so **the tables and closures are real
heap objects**), decodes the hot functions from the State's own `Proto`s (the kcache is the real `Proto.k`, strings are real
short strings with real hashes), builds a frame (a private 1 MiB Value stack and 4096 `CallInfo` nodes with the real
struct layout, filled through `jit_layout` offsets) and runs the compiled code once. The result is the payload of the
returned Value, printed in `lua.c` format. **Every series printed the same value in `rnlua`, PUC 5.5.1, V1, V1o (and
V2 for the two programs that have it), at the measured N and at 12 smaller (program, N) pairs.** No guard failure (abort) occurred.

**Table templates (V1, "every bytecode through the stack", following the fast paths in `vm_unsafe.rs`):**

- GETTABLE/GETI: table tag guard; key tag guard (GETTABLE); `key-1 < asize` (unsigned, `asize` loaded from the table); `array`
  pointer load; element tag byte at `array+4+u`; nil (`tag & 0x0F == 0`) leaves compiled code (the interpreter's `get_int_arr`
  also treats nil as a miss: `__index` may have to run); payload at `array-8*key`; tag word and payload stored to the
  destination. About 18 instructions (GETI about 12).
- SETTABLE/SETI: same guards; the slot must be non-empty, or the table must have no metatable (`pset_existing_int_arr`); stores
  one tag byte and the payload. The write barrier is **not inlined**: a collectable value leaves compiled code (the
  programs store numbers and booleans, for which the interpreter's `barrier_back` is a no-op). About 20 instructions.
- GETFIELD/SETFIELD: key string from the kcache, its `short_hash`, `lsizenode`, `hash & (2^lsizenode - 1)`, node address (24-byte
  nodes), then the `hgetshortstr` loop (key tag and key pointer compare, signed `next` offset in nodes on a collision). The
  main-position hit is the first iteration; the chain walk is included because nbody's 7-key tables do collide. SETFIELD requires an
  existing non-nil value (`pset_existing_shortstr`). Missing key or nil value: exit. About 23 instructions for a main-position GETFIELD,
  25 for SETFIELD. The key hash is loaded from the string object each time, as the interpreter does; a real template can embed it
  (about 3 fewer instructions), not done here.
- TEST+JMP: falsy test (`false` tag or nil nibble), fused with the jump.

**Call protocol (E2 approximation of `JIT_DESIGN.md` section 4): the faithful version, not the narrow one.**

- CALL (about 37 instructions): Lua-closure tag guard; closure to proto; `st.top = ci.func + 1 + a + b`; `stack_size - top > maxstacksize`;
  spare `ci.next`; arity guard (the nil-fill loop is not emulated: fewer arguments than parameters leave compiled code); `Proto.jit`
  non-null (see below); the caller's `ci.savedpc` (an immediate, `code + 4*(pc+1)`); `st.ci = next`; the callee's `CallInfo` fields
  exactly as `pre_call_lua_fast` sets them (`func`, `callstatus = nresults+1`, `top`, `savedpc = code`, `kcache = k`); then `r12 = ci`,
  `rbx += 16*(a+1)`, `r14 = kcache`, entry loaded from the `JitProto`, indirect `jmp`. No native recursion.
- RETURN1 (about 31 instructions): expected-results guard (exactly one); result copied to the function slot (`base-16`);
  `st.ci = ci.previous`; `st.top = func+1`; `CIST_FRESH|CIST_MMRET` test (the outermost frame leaves the oracle); the caller's `base`
  is recomputed from `stack_base` and `prev.func` (the stack could have moved in a real run), `kcache` from `prev.kcache`; the resume
  address is `closure -> proto -> JitProto -> resume[(prev.savedpc - proto.code)/4]`, then an indirect `jmp`. The resume table has one
  8-byte entry per pc (the design says u32 offsets).
- `Proto.jit` emulation: the real design appends a field to `Proto`. The oracle stores the `JitProto` pointer in the `gclist` slot of
  the real `Proto`, which is dead while the collector is stopped (offset from `jit_layout`), so the cost is the same single load and
  null test. The State is never dropped (`process::exit`).
- V1o adds, in call mode, `cmp dword [ci+trap], 0; jne exit` at function entry and before RETURN1 (hook and call-hook poll), on top of
  the section 1 back-edge poll and per-block counter (a call return site is a block leader).

**V2 (tier 1.5 upper bound), only where it fits.** The V2 allocator of section 1 has no spilling, so a function must fit 9 GPRs and
12 xmm registers. A table parameter takes two registers (array pointer and `asize`), hoisted at entry together with a "no
metatable" guard; that is valid only because the compiled code makes no calls, no allocations and only in-bounds stores. The
element type comes from the table contents (all non-nil elements of a parameter table share one class: int, float or bool); loads
guard the tag against it, booleans live as the tag byte in a GPR, stores write the tag byte and the payload. A `for` loop needs three
registers, so the programs that fit are `while`-loop rewrites: `sieve_w` (same output as `sieve`) and `dot_w` (the inner loop of
matmul). `sieve`, `matmul`, `heapsort`, `nbody` do **not** fit and have no V2 number; `fib` and `calloop` have no V2.

**Measurement.** As in section 1: `perf stat -x, -e cycles:u,instructions:u`, `taskset -c 3`, 9 runs with the binaries interleaved inside
each run, under `systemd-run --user --scope -p MemoryMax=8G -p MemorySwapMax=0 timeout 300`, stdin `/dev/null`; binaries
`target/release/rnlua` (generational mode as in `lua.c`), `ref/55/build/lua`, and the oracle. Fixed cost: the oracle in compile-only
mode (state, parse, `setup`, assemble, mmap) is 2.5 M cycles and `rnlua prog 0` is 2.4 M, under 0.4% of the shortest run (fib V1,
631 M); not subtracted. The `setup` work (building the tables in Lua) is done by the interpreter in both and is part of both numbers.

### 6.3 Programs (N is argv[1]; the hot function takes the tables as parameters)

| program | N | what the hot function does |
|---|---|---|
| sieve | 500 | 100 000-element boolean array (about 1.2 MB with the 9 bytes per slot of the array part): refill (SETTABLE), test (GETTABLE+TEST), strided stores (`for j = i*i, n, i`) |
| matmul | 500 | 60x60 float matrices as nested tables: `ai[k] * b[k][j]` (3 GETTABLE, MUL, ADD per iteration), SETTABLE of the sum |
| heapsort | 100 | 20 000 ints: copy, heapify, sort down with swaps and data-dependent compares (GETTABLE, SETTABLE, GETI, SETI, ADDI, IDIV, MUL) |
| nbody | 500 000 | 5 tables with 7 float fields; per pair about 9 GETFIELD and 3 SETFIELD, no native call (no `sqrt`: "nbody-like") |
| fib | 35 | `self(n-1, self) + self(n-2, self)` (29.9 M calls); the function receives itself as a parameter (no upvalue or global access) |
| calloop | 1e8 | `for i = 1, n do s = s + g(i) end` with `g(x) = x + 1`; the callee is compiled too |
| sieve_w, dot_w | 500, 50 000 | while-loop forms for the V2 data point |

### 6.4 Results

Cycles:u in millions, median (min). Each series has 9 runs; heapsort, nbody, fib and calloop (interpreter, V1, V1o) were run a second time
and pooled to 18 samples because the first batch was disturbed (see 6.8). Speedup = interpreter / variant, **from the medians**; the speedup
from the minima is in the last column (it differs noticeably only for heapsort, see 6.8).

| program | interpreter | PUC 5.5 | V1 plain | V1o (+poll, counter, trap) | V2 regs | speedups from minima (V1 / V1o / V2) |
|---|---|---|---|---|---|---|
| sieve | 3162 (3149) | 3167 | 1331 (1322) **2.38x** | 1486 (1470) **2.13x** | not fit | 2.38 / 2.14 |
| matmul | 4405 (4313) | 4538 | 1782 (1778) **2.47x** | 1843 (1842) **2.39x** | not fit | 2.43 / 2.34 |
| heapsort | 3966 (3481) | 3639 | 1523 (1517) **2.60x** | 1542 (1535) **2.57x** | not fit | 2.29 / 2.27 |
| nbody | 2100 (2038) | 2209 | 1101 (1058) **1.91x** | 1096 (1065) **1.92x** | not fit | 1.93 / 1.91 |
| fib | 1752 (1706) | 1740 | 632 (616) **2.77x** | 668 (655) **2.62x** | n/a | 2.77 / 2.61 |
| calloop | 4784 (4721) | 5197 | 2511 (2492) **1.91x** | 2605 (2593) **1.84x** | n/a | 1.89 / 1.82 |
| sieve_w | 4508 (4490) | 4561 | 1510 (1507) **2.98x** | 1684 (1681) **2.68x** | 564 (556) **7.99x** | 2.98 / 2.67 / 8.08 |
| dot_w | 1665 (1642) | 1779 | 655 (653) **2.54x** | 698 (697) **2.39x** | 151 (151) **11.0x** | 2.51 / 2.36 / 10.9 |

V1o relative to PUC (median cycles; lower is better): sieve 0.47, matmul 0.41, heapsort 0.42, nbody 0.50, fib 0.38, calloop 0.50.

Instructions:u in millions (median): interpreter / V1 / V1o (/ V2): sieve 17550 / 6051 / 6947; matmul 24237 / 9300 / 9740; heapsort 16445 / 5366 / 5731;
nbody 11413 / 4473 / 4241; fib 7585 / 2778 / 3076; calloop 26301 / 10301 / 11501; sieve_w 27708 / 7155 / 8453 / 2546; dot_w 11608 / 3602 / 3903 / 1102.
V1 executes 2.6-3.1x fewer instructions than the interpreter; its IPC is 3.5 (heapsort) to 5.2 (matmul).

Per call: fib, interpreter 58.7 cycles and 254 instructions per call (15 bytecodes per call), V1 21.1 cycles / 93 instructions, V1o 22.4 / 103.
calloop (one call per iteration): interpreter 48.2 cycles / 263 instructions, V1 25.1 / 103, V1o 26.0 / 115.

Readings:

1. **Table access does not erode the template gain.** Array-part loops gain 2.1-2.6x with poll and counter (sieve 2.13, matmul 2.39, heapsort 2.57),
   a little below the register-only integer loops (2.85x, section 2) and well above the register-only float loops (1.5x). The float-array loops are the surprise:
   matmul 2.39x and dot_w 2.39x versus float_poly 1.02x in section 2. The loop bodies contain loads, address arithmetic and multiplies that overlap the
   store-to-load latency of the accumulator; float_poly is two pure recurrences. I did not analyse it further.
2. **GETFIELD/SETFIELD with the collision chain gains least (1.9x).** About 23-25 instructions per access, roughly half of them the hash, mask and node address
   computation, which the interpreter also does.
3. **Flat calls gain 1.8-2.6x, far above the plan's 1.15-1.3x.** The interpreter spends about 2.8x the template version's cycles per fib call
   (58.7 vs 21.1) and 254 instructions per call, 17 per bytecode, consistent with the `'frame` re-entry, dispatch and register reloads that
   a compiled call does not repeat. The call-heavy loop gains less (1.84x): about two thirds of its 103 instructions per iteration are the
   call sequence itself, which is the same work in both.
4. **The poll, counter and trap checks cost 1-10%** (V1 to V1o: sieve and sieve_w -10%, dot_w -6%, fib -5%, calloop -4%, matmul -3%, heapsort and nbody about 0): the same order as section 2.
5. **Registers plus hoisting are worth another 2.7-4.3x over V1** where they could be measured: sieve_w 7.99x and dot_w 11.0x against 2.98x and 2.54x for V1. These are upper bounds (see 6.5).

### 6.5 What is and is not representative

Representative (modelled at the instruction level of the interpreter's own fast paths): inline array and short-string hash access with the
interpreter's bounds, nil, metatable and empty-slot rules on **real** `Table` objects (the real `array` layout with payloads below the pointer
and a tag-byte array, real nodes, real hash values); the `CallInfo` push and pop of `pre_call_lua_fast` and `do_ret!` on the **real struct layout**, with the
`ci.next`/`ci.previous` list, `st.top`, the stack-size check, and the resume-address lookup through closure, proto and `JitProto`; no native recursion; correct results.

Not representative, with the direction of the bias:

- **Favourable:** all hot data in L1/L2 (the sieve array of about 1.2 MB is the largest); no GC safepoint, allocation or inline barrier (collectable stores leave
  compiled code instead of calling the helper; the programs store only numbers and booleans); no hook or count hook beyond the emulated word and counter; no `savedpc`
  store except at CALL; exits are aborts (a real exit stub is cold, but every guard site needs one); only fixed-arity, one-result calls; the callee is always compiled (no
  mixed interpreter/compiled frame transitions, no `FrameChanged` exit); at most two return sites per callee; no stack growth; no upvalue access (`fib` and `calloop` pass the function as
  a parameter; GETUPVAL would add a few instructions per call); no globals, varargs, SELF/metamethod dispatch, TBC or CLOSE.
- **Unfavourable:** the key hash is reloaded from the string each time; the block counter is a memory read-modify-write; the `JitProto` indirection adds one dependent load to CALL
  beyond the single `Proto.jit` load of the design; no forwarding of values between consecutive bytecodes (V1 by definition).
- **Not exercised, but J1 needs it:** the nil-fill loop (arguments < parameters), results other than 1, calls to native functions and to non-compiled Lua functions (exit `FrameChanged`),
  return into a non-compiled caller, vararg frames (`base` is shifted), hooks actually firing, a real stack reallocation (modelled only by recomputing `base` after RETURN),
  coroutine resume entering a compiled frame, GC and stack-shrink interaction.
- The proxies used for the projection below are my assignment, not measurements.

### 6.6 Updated projection

Same method as section 3 (per-program cycles ratio vs PUC from `bench/results/quiet-55.json`; interpreter geomean 0.859 over the 25 programs, 0.983 over the
numeric subset; the script reproduces the section 3 rows exactly). The buckets are now **per program** where a measured analogue exists:

- measured analogue: loops_arith = the register-only integer geomean of section 2 (V1o 2.85); mandelbrot = `mandelbrot_regs` (V1o 1.75); sieve, heapsort, matmul, nbody, fib = the programs above (V1o, medians).
- proxies (not measured): fannkuch and queens = geomean(sieve, heapsort); spectral_norm = geomean(matmul, calloop) (its inner loop calls `A(i,j)`); closures, oop_methods and varargs = calloop
  (upvalues, SELF with a metatable `__index`, and VARARG are not modelled: these three are the least trustworthy entries).
- the 12 runtime-bound programs keep the plan's x1.025 (not touched by this experiment).
- "exponent e" applies `s^e` to every measured and proxied speedup; "row B" applies the square root only to the six proxied programs.
- Basis: medians. The sensitivity with minima-based speedups is in parentheses (it moves numbers by at most 0.015, except that it makes heapsort look worse, see 6.8).

| scenario | numeric subset vs PUC (target <= 0.45) | all 25 geomean vs PUC (target <= 0.70) |
|---|---|---|
| plan model, low / high (section 3) | 0.595 / 0.493 | 0.701 / 0.628 |
| section 3, V1o at face value (register-only, calls x1.2) | 0.461 | 0.628 |
| new table numbers, calls kept at the plan's x1.2 (V1o) | 0.438 (0.452) | 0.617 (0.624) |
| **tier 1 V1, face value** | **0.419** (0.432) | **0.555** (0.562) |
| **tier 1 V1o, face value** | **0.438** (0.452) | **0.568** (0.575) |
| tier 1 V1o, row B (proxied programs square-rooted) | 0.502 (0.514) | 0.619 (0.624) |
| tier 1 V1o, exponent 0.75 | 0.536 (0.549) | 0.628 (0.634) |
| tier 1 V1o, exponent 0.5 (square root of everything) | 0.656 (0.666) | 0.694 (0.699) |
| tier 1.5: V2 where measured (loops_arith, mandelbrot, sieve, matmul), V1o elsewhere, face value | 0.248 (0.255) | 0.463 (0.468) |
| tier 1.5 same, row B | 0.284 (0.290) | 0.504 (0.508) |
| tier 1.5 same, exponent 0.75 | 0.350 (0.358) | 0.539 (0.543) |
| tier 1.5 same, exponent 0.5 | 0.494 (0.501) | 0.627 (0.631) |

The numeric subset (interpreter 0.983) needs a geomean speedup of **2.18x** to reach 0.45; V1o at face value delivers 2.24x (2.18x from minima).

### 6.7 Verdict

- **J0 oracle criterion (plan section 9: projected geomean over the 25 programs not above 0.75x PUC): holds, with more margin than before.** 0.57-0.62 at
  face value or row B, 0.69-0.70 in the all-square-root row. The register-only caveat is removed for the integer-array, float-array, field-access and call buckets; what remains
  unmeasured is the runtime-bound majority, which nothing here touches.
- **Tier 1 all-programs target (<= 0.70): holds in every row**, at 0.57 face value and 0.69-0.70 only if every measured gain is square-rooted. It is the more robust of the two tier-1
  targets because the 12 runtime-bound programs only have to stay at x1.0.
- **Tier 1 numeric target (<= 0.45): at the line, not demonstrated.** Tables and calls did not reduce the gain: the subset projects to 0.44 (0.45 from minima) at face value, a 3% margin that is inside
  the plan's +-5% band. Any derating fails it (row B 0.50, exponent 0.75 gives 0.54). The realistic range for J1 is **0.45-0.55**; keep 0.45 as the
  stretch value for tier 1 (reachable only if the template gains hold on the three proxied programs and the J1 bookkeeping costs nothing more than the emulated poll and counter).
- **Tier 1.5 numeric target (<= 0.30): plausible, not demonstrated.** With V2 numbers on four programs and V1o elsewhere it is 0.25 at face value, 0.28 with row B and 0.35 at exponent 0.75. The five
  programs without a V2 number (heapsort, nbody, fannkuch, queens, spectral_norm) are held at their V1o gain, which makes the row conservative for them; the four V2
  programs are upper bounds (no spilling, guards limited to tag checks, hoisting valid only because those functions have no calls or allocation). Tier 1.5 all-programs (<= 0.60):
  0.46-0.54 in the first three rows, 0.63 at exponent 0.5: holds except in the harshest row.

On derating: the section 3 rows were square-rooted because "tables are not measured". That reason no longer applies to the programs measured here (their gain survived table accesses and calls),
so face value is the better estimate for those. The remaining uncertainty is (a) the six proxied programs, (b) features the hot functions here do not exercise (6.5: exits, mixed frames, barriers,
varargs, native calls), (c) what J1 actually emits: `savedpc` and exit-stub bookkeeping, hook-aware counting, the 5.4 `alimit` rule. Row B captures (a); the exponent rows bound (b) and (c). My central
estimate for tier 1 is the row B to exponent-0.75 range: **numeric 0.50-0.54, all 0.62-0.63**.

Template-level improvements that these measurements point at (not implemented): embed the short-string hash and key address in GETFIELD (about 3-4 of 23 instructions); skip the key tag guard when
the key register is a for-loop control variable; cache the table pointer and `array` of a register inside a basic block (V1's rule forbids it across bytecodes, a block-local cache would not); a register-held
block counter in loops without calls. The gap between V1 and V1o is 4.5% of the numeric subset (0.419 vs 0.438), which bounds what the counter and poll improvements can recover.

### 6.8 Caveats and measurement quality

- **Noise.** The other builder's cargo jobs ran on cpus 0-2 during the runs (load average 1.4-2.0); `taskset -c 3` keeps the runs off those cpus but not off the shared L3, memory or clock domain.
  24 of the 34 series had a max/min spread under 5%. The exceptions were single disturbed runs (calloop interpreter 6504 M vs 4746 M, +37%; sieve V1 1655 vs 1322, +25%), the interpreter's nbody series (a
  continuous 2038-2229 M spread) and heapsort. **No run was dropped from the tables**: medians and minima are both shown; the four noisiest groups (heapsort, nbody, fib, calloop; interpreter, V1, V1o) were re-run once and
  pooled to 18 samples. Heapsort's interpreter series is the one place where median and minimum disagree: 16 of 18 samples lie at 3950-4030 M and two at 3481 and 3541 M, with V1 stable at 1517-1530 M
  in 16 of 18 samples. The median (3966 M) is the representative figure and the minimum an outlier, so heapsort is 2.6x, not the 2.29x the minima give. Instruction counts are exact and identical across runs.
- Cycles:u are compared, not times (the core boosts to 3.5 GHz). The SMT sibling of core 3 was not controlled.
- 5.5 only: the decoder and templates are Lua 5.5 (`FORLOOP`, `alimit_touch`, `lastfree` and the 5.4 opcodes differ); `jit_layout` is the part that builds for both versions.
- Oracle shortcuts with SAFETY comments in `staterun.rs`: `Proto.gclist` reuse, stopped collector, `process::exit` instead of dropping the State, a private stack instead of the thread's.
- **Encoder gaps found:** `movsxd r64, m32` (used for the hash chain `next` offset; hand-encoded as `49 63 41 0C`, base r9 only). `cmp m8, imm8`, `test m8, imm8` and `mov m8, r8` exist now and the new
  templates use them (the section 1 `cmp_tag` helper is unchanged). Nothing else was missing: SIB addressing with scale 2 and 8, `movzx`, `lea` with SIB, `mov r64, imm64` and `jmp r64` all encoded.
- Reproduce: `cargo build --release -p rn-cli -p rn-jit --example oracle`; for each program run
  `target/release/examples/oracle {v1|v1o|v2} bench/jit-oracle/<prog>.lua N` and `target/release/rnlua bench/jit-oracle/<prog>.lua N`
  (`dump` prints the decoded bytecode of every compiled function); measure with `perf stat` as above. The measurement driver (a loop around `perf stat`
  and a median/min aggregator) and the projection script (a dozen lines over `quiet-55.json`) are not committed.
