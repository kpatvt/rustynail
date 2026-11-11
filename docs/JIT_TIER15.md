# JIT tier 1.5: typed loop regions (J4 design)

Status: DESIGN v1 (2026-10-29), for review before any J4 builder starts. Scope: `docs/JIT_PLAN.md` J4 ("Tier 1.5: local
type inference, values in registers"), with plan sections 3 (E1-E4, P1, C1, G1, invalidation sources), 9 (checkpoints), 13
(Sonnet builders), 14 (LuaJIT study changes) as constraints. Builds on the tier-1 code as it is on `main` at `09b5917`
(`crates/rn-jit/src`: `lower.rs`, `ir.rs`, `emu.rs`, `x86.rs`, `templates_*.rs`, `calls.rs`, `rt.rs`).

Evidence tags as in `docs/JIT_DESIGN_NOTES.md`: **[M]** measured here (section 1), **[V]** verified in the code, **[D]** design
proposal, **[E]** estimate (with the method stated).

## 0. Summary

- **What the profile says [M].** In the 10 programs studied, tier-1 code almost never exits to the interpreter (0
  `Continue` exits on the hot paths), so tier 1.5 can only gain on code quality inside compiled loops. Of the instructions
  executed in JIT code, **tag loads and tag guards are 18-46%** (mandelbrot 46%), **payload loads/stores to the VM stack
  plus tag stores 14-27%**, GPR<->XMM shuffles 0-15% (mandelbrot 15%), back-edge polls 1-14% and block counters 1-6%. A
  float multiply costs 17 executed instructions in tier 1 (mandelbrot), against 1 instruction in ideal code. Float
  recurrences run through the VM stack, so they are latency-bound on store-to-load forwarding: mandelbrot runs 212
  instructions and 50 cycles per inner iteration, against about 9 cycles for LuaJIT. nbody spends 23% of its cycles in
  the `math.sqrt` native and its call helper.
- **Design in one paragraph [D].** A hot loop nest of a tier-1 function is *promoted*. The whole prototype is recompiled
  into one program that contains the unchanged tier-1 code (the deopt target) plus a **typed region** for that nest.
  Inside the region every Lua register has a static type fact. The facts come from constants and transfer functions,
  from the frame tags observed at promotion, and from per-pc "last tag" bytes that the tier-1 warm-up code records for
  loads. Values live in machine registers (ints, booleans and table pointers in 5 GPRs, floats in 14 XMMs), and the VM
  stack slot of each Lua register is its only spill home. **Every exit writes the dirty registers back inline and jumps
  into the tier-1 code of the same instruction**: no table-driven restore, no interpreter round trip, and tier-1 frames
  are identical to the interpreter's. Any instruction the region does not specialise runs as its tier-1 template
  between a *flush* (write back everything dirty) and a *resume stub* (reload, and re-guard when Lua code may have run).
  Calls, GC points, `debug.setlocal`, hooks, errors and coroutine switches are therefore all observed with the exact VM
  stack, by construction.
- **Projection [E].** Numeric gate subset (the 7 programs of `docs/JIT_PLAN.md` s21): **0.524 now -> 0.29 central (range
  0.22-0.36)**; the plan's 9-program subset: 0.53 -> 0.31 (0.23-0.38); all 25 programs: 0.627 -> 0.51 (0.47-0.56). The
  0.30 numeric target is **reachable but at the line**. It depends on the float loops (mandelbrot, loops_arith, nbody,
  matmul) reaching 0.17-0.25 of the reference and on two side passes (the `math.sqrt` intrinsic, field-probe caching).
  sieve and spectral_norm stay around 0.5 (allocation helpers; a Lua call in the inner loop) and fib does not change (no
  loops). The 0.60 all-programs target is met in every scenario.
- **Tasks.** 9 Sonnet-builder tasks (section 8), 6 of them on disjoint files. Opus review is required on T2 (analysis),
  T3 (runtime and code lifetime), T4 (region codegen core) and T7 (policy). Every pass can be switched off with
  `RUSTYNAIL_JIT_OPT`, and the whole tier with `RUSTYNAIL_JIT_T15=off`.

## 1. Profile evidence

### 1.1 Method

- Build: `cargo build --release -p rn-cli --features jit` in a private target dir, niced, on CPUs 0-2, under
  `systemd-run --user --scope -p MemoryMax=8G -p MemorySwapMax=0 timeout`.
- Counts: `perf stat -x, -e cycles:u,instructions:u`, `taskset -c 4`, min of 3 runs, `RUSTYNAIL_SEED=0`, default N,
  stdin `/dev/null`. The comparison binary is the reference `ref/55/build/lua`. This is not the gate protocol
  (median of 7 or more). The ratios below agree with `bench/results/jit-j2.json` to within ±0.02.
- Instruction mix of JIT code: `RUSTYNAIL_JIT_PERFMAP=1 RUSTYNAIL_JIT_DUMP=dir`, `perf record -b -e cycles:u` on CPU 5
  (Zen 5 LBR branch stacks), `perf script -F brstacksym`. Each pair of consecutive branch records gives one straight-line
  run of executed instructions; mapping the runs onto `objdump` of the dumped code gives **exact relative execution
  counts per instruction** (100k-140k runs per program). Plain sampling of `instructions:u` was tried first and
  discarded, because its skid piles samples onto instructions that follow long-latency ones (one `movq` drew 18% of all
  samples). IBS precise sampling is unavailable to unprivileged users here.
- Classifier (heuristic, by addressing mode and context): stack tag load `movzx r,[rbx+16k]`; tag guard = the `cmp r,imm`
  plus `jcc` on a register just loaded with a tag; tag store `mov byte [rbx+16k]`; slot payload load/store
  `[rbx+16k+8]`; heap access through any other base register (table header, array, node, upvalue, closure); `[r15]` =
  block counter; `[r13+trap]` plus the `INTERRUPT` byte load = poll; `movq` = GPR<->XMM; `*sd` = FP op. Comparisons whose
  tag register was renamed fall into "alu", so the tag share is a lower bound. The scripts (`stat.sh`, `prof.sh`,
  `classify.py`, `project.py`) were scratch files and are not committed. The numbers are reproducible with the commands
  above.

### 1.2 Results

Cycles relative to the reference, and the share of tier-1 instruction executions by class (in % of the instructions
executed in JIT code). "cyc in JIT" = cycles samples whose IP is in compiled code.

| program | on/ref cyc | Ginstr | IPC | cyc in JIT | tag ld+guard | tag st | slot pay ld | slot pay st | gpr<->xmm | fp op | poll | count | heap ld/st/guard | K/ci/thread | alu+br+jmp |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| loops_arith | 0.479 | 9.20 | 4.4 | 100% | 23.9 | 5.3 | 13.1 | 9.0 | 5.1 | 2.3 | 6.9 | 2.6 | 0.0 | 0.0 | 31.8 |
| mandelbrot | 0.587 | 11.66 | 4.3 | 100% | 45.7 | 5.4 | 11.2 | 6.1 | 15.2 | 5.2 | 4.0 | 2.9 | 0.0 | 0.0 | 4.3 |
| nbody | 0.549 | 12.29 | 4.9 | 76% | 28.8 | 3.8 | 7.8 | 4.3 | 7.4 | 2.5 | 1.3 | 1.1 | 17.1 | 2.7 | 23.3 |
| spectral_norm | 0.599 | 12.03 | 4.4 | 100% | 21.1 | 4.4 | 8.3 | 5.6 | 5.6 | 2.4 | 5.8 | 3.2 | 12.8 | 4.5 | 26.4 |
| matmul | 0.369 | 9.20 | 7.5 | 99% | 29.5 | 3.5 | 8.9 | 4.8 | 4.1 | 1.4 | 4.5 | 1.5 | 8.6 | 0.0 | 33.3 |
| fannkuch | 0.421 | 10.38 | 6.1 | 100% | 23.7 | 2.9 | 9.4 | 3.7 | 0.0 | 0.0 | 4.9 | 3.3 | 11.7 | 0.0 | 40.4 |
| sieve | 0.759 | 8.26 | 3.5 | 94% | 18.3 | 0.2 | 9.9 | 4.2 | 0.0 | 0.0 | 14.0 | 4.3 | 10.2 | 3.6 | 35.2 |
| heapsort | 0.563 | 8.95 | 4.2 | 100% | 22.3 | 4.4 | 10.0 | 4.5 | 0.0 | 0.0 | 2.7 | 3.3 | 17.2 | 0.3 | 35.1 |
| queens | 0.571 | 7.75 | 3.2 | 100% | 21.0 | 4.1 | 9.4 | 5.9 | 0.0 | 0.0 | 6.1 | 4.6 | 15.3 | 0.9 | 32.8 |
| fib | 0.527 | 11.81 | 5.4 | 100% | 8.9 | 2.5 | 4.1 | 2.5 | 0.0 | 0.0 | 8.6 | 6.0 | 20.5 | 8.9 | 37.8 |

Further facts [M]:
- **Exits:** `RUSTYNAIL_JIT_VERBOSE=1` reports 0 `Continue` exits on all 10 programs (1-810 entries, all OSR or frame
  changes). Tier 1 never bounces to the interpreter here, so guard-failure exits are not a cost today and all of tier
  1.5's gain has to come from code quality.
- **Outside JIT code:** nbody 16.9% of cycles in `lib_math::math_sqrt` and 6.5% in `jit_call` (the native call path,
  once per inner iteration). sieve 6% in `table::resize`/`newkey`/libc (the insertion loop `flags[i] = true`).
  spectral_norm: 47% of JIT instructions are in the loop-free leaf `A(i, j)` and the flat call/return around it. Every
  other program has 99% or more of its cycles in JIT code.
- **Anatomy of a float op (mandelbrot `MUL 13 10 10`, i.e. `zr*zr`).** Tier 1 has no type knowledge across blocks and
  the int/float dispatch merges result types to "unknown", so every arithmetic op re-dispatches. The executed path for
  float operands is:
  ```
  movzx esi,[rbx+0xa0] ; movzx edi,[rbx+0xa0]      two tag loads (same slot twice)
  cmp rsi,3 ; jne ; ...  cmp rsi,0x13 ; jne ; cmp rdi,3 ; jne ; cmp rdi,0x13 ; jne      int test fails, float test
  mov r8,[rbx+0xa8] ; mov r9,[rbx+0xa8]             two payload loads
  movq xmm0,r8 ; movq xmm1,r9 ; mulsd xmm0,xmm1 ; movq r8,xmm0
  mov byte [rbx+0xd0],0x13 ; mov [rbx+0xd8],r8       tag and payload store; the next op reloads them
  ```
  That is 17 instructions instead of `mulsd xmm2,xmm2`. The inner `while` loop executes **212 instructions and 50 cycles
  per iteration** (54.9 M iterations; reference 85 cycles, LuaJIT about 9.4 estimated from its 0.11 ratio). The
  register-allocated equivalent is about 35 instructions with a 9-10 cycle FP dependency chain [E].
- **matmul inner loop:** 144 instructions and 19.2 cycles per iteration at IPC 7.5 (reference 52 cycles, LuaJIT about
  4.3). The loop is throughput-bound, so instruction cuts translate almost directly into cycles.
- **loops_arith:** the hottest cycle samples sit on the `f = f + i * 0.5` store-reload chain (`[rbx+0x28]`), not on the
  `idiv` of `i % 7` (which overlaps). That is a register-allocation win, not a strength-reduction win.

### 1.3 What each optimisation can remove (cap-ex, share of tier-1 instructions in JIT code)

| optimisation | classes removed | share available (range over the 9 numeric programs) | notes |
|---|---|---|---|
| O1 type facts (no tag guards on stack slots) | tag ld+guard | 18-46% | heap tag checks (array/node element) stay, merged with the nil check (one `cmp byte` against the expected tag) |
| O2 register residency + write-back only at exits | slot payload ld/st, tag st | 14-27% | write-through stays for collectables (G1) |
| O3 floats in XMM | gpr<->xmm | 0-15% | mandelbrot 15%, nbody 7%, loops_arith 5% |
| O4 single-path code (no int/float dispatch joins) | part of jmp/alu | 2-8% [E] | |
| O5 builtin intrinsics (`math.sqrt` ...) | native call + helper | nbody 23% of cycles | only nbody among the 10 |
| O6 hoisted upvalue loads, field-probe caches | part of heap ld | 3-10% [E] | nbody/heapsort/queens: 15-17% heap ld |
| O7 counter in a register, shorter poll | count/poll latency | 0-31% of cycles on tight loops (oracle V1 vs V1o) | instruction count unchanged; the store-forwarding chain through `[r15]` goes away |

### 1.4 Expected gains per benchmark [E]

Two models, then a judgement:
- *Instruction model* (`project.py`): within the region-eligible fraction of JIT code, remove 75-95% of tag
  loads/guards, 60-90% of tag stores, 50-85% of slot payload loads/stores, 85-100% of GPR<->XMM moves, 40-80% of jumps,
  5-25% of ALU, 5-30% of heap loads, 0% of polls/counts. Assume cycles scale with instructions at constant IPC. This
  **under-states** float loops (the store-forwarding chains disappear) and **over-states** throughput loops whose IPC
  falls once the cheap instructions are gone.
- *Latency anchors:* the J0 oracle's register-allocated V2 reached 0.12 of PUC on `mandelbrot_regs` and 0.09-0.25 on the
  register-only loops (`docs/JIT_ORACLE.md` s2). LuaJIT's ratios (s21 / `jit-j2.json`) are a floor nobody here expects to
  reach.

| program | now | eligible | pessimistic | **central** | optimistic | LuaJIT | main levers |
|---|---|---|---|---|---|---|---|
| loops_arith | 0.479 | 100% | 0.32 | **0.25** | 0.17 | 0.42 | O1-O3, O7 (float chain f, int chain x) |
| mandelbrot | 0.587 | 100% | 0.26 | **0.17** | 0.12 | 0.11 | O1-O3 (pure float nest) |
| nbody | 0.549 | 97% | 0.33 | **0.25** | 0.19 | 0.09 | O5 sqrt, O1-O3, O6 field probes |
| spectral_norm | 0.599 | 35% | 0.55 | **0.48** | 0.40 | 0.04 | O1/O2 in Av/Atv only; A() is loop-free (T9 adds about 0.05) |
| matmul | 0.369 | 100% | 0.26 | **0.20** | 0.15 | 0.08 | O1, typed array loads, O2 |
| fannkuch | 0.421 | 95% | 0.34 | **0.30** | 0.22 | 0.26 | O1/O2 on int tables; short inner loops |
| sieve | 0.759 | 90% | 0.63 | **0.55** | 0.45 | 0.36 | insertion loop stays helper-bound; polls 14% |
| heapsort | 0.563 | 95% | 0.43 | **0.36** | 0.28 | 0.33 | O1/O2, O6 upvalue `a` hoist |
| queens | 0.571 | 80% | 0.47 | **0.40** | 0.33 | 0.41 | O1/O2, O6; recursive call on hit paths |
| fib | 0.527 | 0% | 0.53 | **0.52** | 0.50 | 0.11 | no loop; only T9 (function regions) touches it |
| **numeric-7 geomean** (s21 gate set) | **0.524** | | 0.36 | **0.29** | 0.22 | 0.14 | |
| numeric-9 geomean (plan s1 set) | 0.534 | | 0.38 | **0.31** | 0.23 | | |
| all-25 geomean (others unchanged) | 0.627 | | 0.56 | **0.51** | 0.47 | 0.24 | |

Reading, honestly:
- **0.30 numeric is reachable but not safe.** The central estimate sits on the line (0.29 for the 7-program gate set,
  0.31 for the 9-program set). Below about 0.33 it needs all of: floats in XMM across the whole loop nest (not only the
  innermost loop), the sqrt intrinsic, typed array loads with the element-type guard folded into the nil check, and the
  counter out of memory. If only O1-O3 land, expect 0.33-0.36. A further 0.02-0.03 would come from T9 (function regions
  for spectral_norm's `A`) and from tighter table probes. I put the chance of reaching 0.30 at about 50%.
- **The all-programs target (0.60) is met in every scenario** (0.47-0.56); tier 1 is already at 0.627.
- Risks to the estimate: IPC collapse on throughput-bound table loops (matmul is at IPC 7.5 today); register pressure
  in GPR-heavy loops (5 allocatable GPRs, section 4.1); entry/exit cost of very short inner loops (fannkuch's
  `n = 10` loops, nbody's 2.5-iteration inner loop). The last is why regions cover whole loop nests (section 3.1).

## 2. Architecture

```
tier-1 compile (unchanged)          promotion (hot nest)                      combined program (recompile)
+--------------------+   counter    +-----------------------------+          +-------------------------------+
| tier-1 code        | -----------> | jit_t15_promote helper:     | -------> | tier-1 code of all pcs        |
| + warm-up profiling|  at nest     | read frame tags + prof[pc], |          |   (deopt target, unchanged)   |
|   (prof[pc] tags)  |  back-edges  | analyse, allocate, lower,   |          | region code for nest [H, B]   |
+--------------------+              | emit, commit, swap code     |          | stubs: resume/entry, exit,    |
                                    +-----------------------------+          |   fallback reload             |
                                                                             +-------------------------------+
```

- **Region** = one natural loop nest `[H, B]` of a prototype (H = loop head, B = its back-edge instruction), single
  entry. All nested loops are inside it. Several disjoint nests of one function can each be promoted. A promotion
  recompiles the prototype once with all of its promoted regions.
- **Deopt target = the tier-1 code in the same program.** The interpreter's frame layout is used by tier 1 at every
  instruction (nothing lives in registers between instructions), and region code keeps the VM stack exact at every
  *observation point*. A region exit at instruction `P` therefore only writes back its dirty registers and jumps to
  tier-1's label for `P`, which re-executes `P` (E3). Block boundaries and pre-paid counts are identical in both
  copies (section 4.9), so the jump lands in a consistent C1 state.
- **Entry:** edges into `H` from tier-1 code (the instruction before the loop, the tier-1 copy's own back-edges, the
  resume table at loop heads and after calls) go through `stub(H)`, which checks the facts the region assumes and loads
  the registers. If any check fails it goes to tier-1 `H`. The tier-1 copy's back-edges also go through `stub(H)`, so a
  region that was left for one iteration is re-entered at the next back-edge. A loop whose first iteration has a
  different type (for example `local a = 0` followed by `a = a + float`) runs that iteration in tier 1 and enters the
  region at the back-edge: a free one-iteration *peel*.
- **No new mid-level IR.** The analysis runs on bytecode; the code generator emits the existing micro-op IR, extended by
  XMM registers and a few ops (section 4.2). The Miri emulator and the x86 backend keep running the same `Program`.

## 3. Analysis (crate-internal module `t15::analysis`, pure functions)

### 3.1 Region selection

```
fn select_region(code, loops, hot_backedge_pc) -> Option<RegionPlanSeed>
  L = innermost natural loop whose back-edge instruction is hot_backedge_pc
  // outermost nest within the size cap: per-entry preheader cost is paid once per nest entry, not per inner entry
  while let Some(P) = parent(L) { if size(P) > MAX_REGION (512 instructions) || !single_entry(P) { break }; L = P }
  if !single_entry(L) || size(L) > MAX_REGION || uses_named_vararg_ops(L) /* GETVARG, ERRNNIL */ { return None }
  Some(L)
```

- Natural loops [V: `lower.rs` already finds them]: back-edges are `FORLOOP` (target `pc + 1 - Bx`), `TFORLOOP`,
  backward `JMP`, and conditionals whose `JMP` follower jumps backward (`repeat ... until`). The body is `[t, p]`, which
  is contiguous in Lua bytecode. Nesting = interval containment.
- `single_entry(L)`: no jump from outside `[H, B]` targets a pc in `(H, B]`. Lua's scoping forbids `goto` into a
  nested block, but goto-built loops in a single block can have side entries. v1 rejects them. A later version can route
  side entries through `stub(P)`, which is the same mechanism.
- Deviation from plan s14 item 2 ("innermost loops compile first", the LuaJIT trace rule): LuaJIT needs it because a
  trace cannot contain an inner loop. A method region can, and taking the whole nest avoids paying entry guards on
  every entry of a short inner loop (nbody's inner loop averages 2.5 iterations; fannkuch's are at most 10).

### 3.2 Type lattice

`TagSet = u16`, a bitmask over tag classes: `NIL FALSE TRUE INT FLT SHRSTR LNGSTR TABLE LCL LCF CCL UDATA LUDATA THREAD`
(one bit per exact tag byte the VM uses; the mapping `tag byte -> class` is a table generated from `rn_core::value`
constants, so variant and collectable bits are never confused, see `JIT_DESIGN_NOTES.md` s2.2). The lattice:

- `0` = bottom (unreached); join = bitwise OR; top = all bits ("Any").
- A **singleton** set is a type fact that removes all guards on that slot. `INT|FLT` ("Num") gets dual-path code, and
  the value stays in its slot. `FALSE|TRUE` ("Bool") is enough for `TEST` (`cmp tag, FALSE`). Any other set means the
  slot is boxed and treated as in tier 1.
- Facts are per **(pc, Lua register)** and describe the state *before* the instruction at `pc`. Slots `>= maxstacksize`
  never appear.

Other facts:
- `nonnil(s)`: implied by the tag set.
- `invariant(s)`: `s` is not written by any instruction in the region (computed once), and it is not `escaped`.
- `escaped(s)`: some `CLOSURE` in the function captures slot `s` (an `instack` upvalue description of a nested proto).
  Callees can write such a slot through the open upvalue (plan s14 item 6).
- Table facts (section 5): `tabhdr(s)` (array pointer/size of the table in `s` are cached), `node(s, k)` (node address
  of constant key `k` in the table in `s`). Both are *versioned by barrier*: killed at every barrier op (3.6) and at every
  instruction that redefines `s`.
- Constant facts: `k(s) = value` for slots loaded with `LOADI/LOADF/LOADK` of a number and not redefined (immediates).
- Speculated values (not only types): the step of each numeric `for` loop in the region (`step(A) = c` when observed
  as a small integer constant; one guard in `stub(H)`), and builtin identity for upvalues/globals that are called (5.2).

### 3.3 Sources of facts

1. **Static** (no guard): `LOADI` -> INT, `LOADF` -> FLT, `LOADK` -> the constant's tag, `LOADTRUE/LOADFALSE/LOADNIL`,
   transfer functions of arithmetic (3.4), `LEN` of a TABLE without metatable or of a string -> INT (the helper
   decides), comparisons and `NOT` (-> Bool), numeric `for` control variables (from FORPREP's typed path).
2. **Frame snapshot at promotion** (`jit_t15_promote` runs with the frame consistent, because tier 1 keeps everything in
   slots): `obs[s] = tag of slot s` for every `s < maxstacksize`. Used for the region's live-ins at `H` (entry
   speculation, checked in `stub(H)`). [D; plan s14 item 1 / P1]
3. **Warm-up profile bytes.** While a prototype is tier-1 compiled and has candidate nests, the tier-1 templates of
   *loads inside candidate nests* (`GETTABLE GETI GETFIELD GETTABUP GETUPVAL SELF`) store the tag of their result into
   `prof[pc]` (`mov rax, imm64(&prof[pc]); mov byte [rax], tagreg`: 2 instructions in warm-up code only; the recompiled
   tier-1 copy omits them). Used for def-site speculation (guard right after the load, before committing the result).
   `prof[pc] == 0xFF` (never executed) -> Any. Call results are Any in v1 (except intrinsics).
4. **Exit feedback**: a type-guard exit stub stores the tag it saw into `prof[pc]` (cold path). The next
   respecialisation joins it with the old speculation (section 3.8).

### 3.4 Fixpoint and transfer functions

Forward dataflow over the region's basic blocks (blocks = tier-1 blocks, `lower.rs` `head[]`), state `[TagSet;
maxstacksize]`.

```
entry(H) = { s live at H: obs[s] (singleton) ; others: Any }      // speculation
repeat until stable:                                              // height <= 14 bits per slot: converges in a few rounds
  for B in reverse post-order: in(B) = join(out(pred)) (preds outside the region: entry(H)); out(B) = transfer(B, in(B))
then for each slot s live at H with in(H)[s] not a singleton:
  t = join of the back-edge outs for s
  if t is a singleton: spec(H)[s] = t, mark s "peel" (stub(H) fails on the first entry: expected, not counted)
  else: s is Any at H (boxed)
re-run the fixpoint with spec(H) as entry(H) until stable
```

Transfer per opcode (5.5 bytecode; "num" = `{INT, FLT}`; `x ∩ num = {}` means the template always exits there, so the
result is irrelevant):

| op | result type | notes |
|---|---|---|
| `ADD SUB MUL` (+`I/K` forms) | INT if both INT; FLT if both ⊆ num and either has FLT and both singleton; else num | the int/int case wraps |
| `DIV`, `DIVK`, `POW`, `POWK` | FLT | |
| `IDIV MOD` (+K) | INT if both INT; FLT if both FLT/mixed (tier 1 exits there: keep the exit) | constant-zero divisor: exits (interpreter raises) |
| `BAND BOR BXOR SHL SHR` (+K/I), `BNOT` | INT if operands INT; float operands -> exit (tier-1 rule) | |
| `UNM` | type of the operand ∩ num | |
| `NOT`, `EQ.. GEI`, `TEST` | `NOT` -> Bool; compares define nothing | |
| `TESTSET` | operand's set | |
| `MOVE` | source set | |
| `LOAD*` | static | |
| `GETTABLE GETI GETFIELD GETTABUP GETUPVAL SELF` | `prof[pc]` singleton if present, else Any | a guard is emitted at the def (3.5) |
| `FORPREP` (5.5) | ctl/count INT if init and step INT (from facts or obs) and limit num; FLT loop if any FLT | the typed FORPREP exits for unusual operands |
| `FORLOOP` | ctl as FORPREP | |
| `LEN` | INT | through the helper (class N) |
| `CALL A B C` | slots `A..A+C-2` = Any (all `>= A` if `C = 0`) | barrier op |
| `TFORCALL A` | loop variables Any | barrier op |
| `CONCAT A B` | `A` = SHRSTR or LNGSTR -> Any | barrier op |
| `NEWTABLE A` | TABLE | barrier op |
| `CLOSURE A` | LCL | barrier op |
| everything else | defs Any (as computed by the opcode's def set) | |

**Barrier kill:** at a barrier op (3.6), facts of `escaped` slots become Any in the out-state. All other facts are
kept, but `stub(P+1)` after the op re-checks them (they are speculation again).

Def/use sets per opcode are needed for liveness and for the transfer function. They are a table in `t15::analysis`
with explicit variable-range rules (`CALL B=0` reads up to `top`, `C=0` defines everything from `A`, `RETURN B=0`,
`VARARG C=0`, `SETLIST B=0`, `CONCAT A..A+B-1`, `TFORCALL A+4..A+3+C` for 5.5 ...). The table is unit-tested against
`luac -l` listings of generated functions.

### 3.5 Speculation and guards

- **Entry guards (`stub(H)`):** for each slot `s` live at `H` with a singleton fact: `cmp byte [rbx+16s], tag(T); jne
  fail`. For each speculated for-loop step: `cmp qword [rbx+16(A+1)+8], c; jne fail`. For each hoisted builtin:
  identity compare (5.2). `fail`: if `s` is a peel slot, no counting; else increment `entry_fail[region]`. Then jump to
  tier-1 `H` (nothing was modified).
- **Def-site guards (loads with a speculated result):** the tag of the loaded value is compared against the speculated
  tag *before the result is committed* (to a register or the slot). On failure the exit goes to tier-1 at `pc` (the load
  re-executes there; reads are idempotent, including the 5.4 `alimit_touch`, [V] `JIT_DESIGN.md` s18). For array
  loads the element tag check (`!= nil`) and the type guard are one compare.
- **Static facts need no guard.**
- **Resume stubs `stub(P)`** for `P` in {loop heads in the region, the instruction after each CALL/TFORCALL in the
  region, the instruction after each fallback of class G/C}: the same code as entry guards but for the facts at `P`.
  Failure -> tier-1 `P`.

### 3.6 Instruction classes inside a region

| class | which | code | after it |
|---|---|---|---|
| **Typed** | arithmetic, compares, TEST/TESTSET/NOT, MOVE/LOAD*, JMP, FORPREP/FORLOOP, typed table ops (section 5), GETUPVAL (5.1), intrinsics (5.2), `LEN` of a statically known string | region templates on registers | nothing |
| **Local fallback** | a templated instruction whose operands are not typed enough (Num, Any) and whose tier-1 template calls no helper on its fast path | flush dirty **GPR** values + budget, tier-1 template, reload GPR values | XMM values survive (tier-1 templates clobber only `xmm0/xmm1` outside `CallH`) |
| **Fallback N** | tier-1 template with a helper that runs no Lua code and allocates nothing: `jit_get` (raw `__index` table chains), `jit_len`, `jit_pow` | flush all, template, *reload without guards* | registers reloaded (all caller-saved) |
| **Fallback G** (barrier) | helper that may allocate (GC step, finalizers = Lua code): `jit_set` (insertion), NEWTABLE, SETLIST, CONCAT, CLOSURE, VARARG, CLOSE/TBC, TFORCALL | flush all, template, `stub(P+1)` (full re-guard) | |
| **Call** (barrier) | CALL, TAILCALL, RETURN* (leave the frame), TFORCALL with a Lua iterator | flush all, tier-1 template (flat call / helper / return); the return comes back through the resume table at `P+1` -> `stub(P+1)` | |
| **Exit** | stubs in tier 1 (MMBIN followers as dead code, LOADKX, GETVARG ...) | flush all, jump to tier-1 `P` | |

A **barrier op** is any Fallback G or Call instruction: Lua code (callees, finalizers, `__close`, iterators) may run
inside it, and so may `debug.setlocal`, `debug.sethook`, `setmetatable`, table rehash, GC, stack reallocation, yields
and errors.

### 3.7 Validity of facts: the exact rules

- **R-OBS (observation points).** Only code that runs inside a helper call can observe or modify the frame (callee,
  native, finalizer, `__close`, iterator, error handler). Hooks never run while compiled code is entered (plan s3; [V]
  J-ENTRY in `docs/UNSAFE.md`: `jit_hooks::enter` only enters when `hook_mask() == 0`). Region code itself runs no Lua code. Observation points are therefore
  exactly: every helper call (inside fallbacks and calls), every region exit (to tier 1, which may call helpers), and
  leaving native code (flat call, return, thunk exit).
- **R-EXACT.** At every observation point the VM stack slots `[0, maxstacksize)` hold exactly what the interpreter would
  hold: every *dirty* register value (newer than its slot) is written back before the point. A slot whose value is
  dead by liveness but dirty is also written back, because `debug.getlocal` can read temporaries ("(temporary)" names,
  up to `ci.top`), and so can error handlers and tracebacks. No dead-slot pruning.
- **R-REGUARD.** After a barrier op, every fact the region relies on is re-established by `stub(P+1)` guards and every
  register is reloaded from its slot. This covers `debug.setlocal` by a callee or finalizer, open-upvalue writes to
  escaped slots, and coroutine switches (a yield is an error exit; the resume enters through `stub`). Optional
  optimisation T10a: a per-thread `setlocal_epoch` bumped by `debug.setlocal`/`lua_setlocal`. If it is unchanged across
  the helper, the guards on non-escaped slots are skipped (loads only).
- **R-G1 (roots).** A collectable value (TABLE, strings, functions, userdata, thread) is **write-through**: the
  instruction that defines it stores tag and payload to its slot immediately. The register copy is a cache that is
  valid until the next barrier op (objects do not move [V `gc/`]; the slot keeps the object alive). Only INT, FLT and
  Bool values may be dirty.
- **R-DERIVED.** Pointers derived from a table (array pointer, `asize`, node pointer) are valid until the next barrier op
  or region exit, and until the slot holding the table is redefined. Inline stores never resize: they only write an
  existing array slot or an existing node value ([V] `templates_table.rs` `set_arr`/`set_node`). Insertion is a
  `jit_set` fallback, which is class G. The GC never resizes tables ([V]: the only `resize` in `gc/` is the string
  table); weak-table clearing changes slot *contents*, at GC points only.
- **R-STACK.** `rbx` is reloaded after every helper (the tier-1 templates already `ReloadFrame`). Region code keeps no
  stack address except through `rbx`.

### 3.8 Anti-thrash policy (plan s14 items 2 and 4)

Per region (side table in `T15Data`, section 6.2; never in bytecode):

| event | counter | action |
|---|---|---|
| type-guard exit at site `k` | `exit_ctr[k]` (+1 in the exit stub) | at `EXIT_HOT` (256, doubled after each respecialisation): call `jit_t15_event(Respecialize)` |
| entry-guard failure (non-peel) | `entry_fail[r]`, with `entry_ok[r]` (+1 per successful `stub(H)` entry, not per iteration) | `entry_fail >= 64 && entry_fail * 4 > entry_ok`: respecialise |
| miss exit (array index out of bounds, nil element, key not found, barrier needed) | `miss_ctr[k]` | not a speculation failure. At 1024 the site switches to the in-region fallback protocol on the next recompile (flush + tier-1 slow path + reload) instead of exiting |
| compile failure (register budget impossible, cap reached, unsupported) | | region `Off` for this nest (permanent; tier-1 code stays) |

**Respecialisation:** recompile the prototype with, for the failing def site, `spec = join(old spec, prof[pc])`
(possibly no longer a singleton, so the value is boxed), or for the failing live-in `entry(H)[s] = join(old,
observed)`. Each respecialisation increments `gen[r]`. At `gen[r] == 3` a further trigger sets the region `Off` and
recompiles without it (blacklist). Total compiles per prototype are capped at 6 (tier 1 + 5); afterwards no more
promotions in that prototype. A region turned `Off` stays off; there is no re-trial. The exponential back-off of the
plan applies to *promotion attempts* that fail transiently (code cap or memory limit refused: retry after 2^k * 4096
back-edges, k <= 10, plus 4 random bits as in LuaJIT, then `Off`).

Promotion trigger: `T15_HOT` = 4096 tier-1 back-edges summed over the nest (one counter per nest, decremented at every
back-edge of the nest in the warm-up tier-1 code: `mov rax, imm64; sub dword [rax], 1; js promote`). In
`RUSTYNAIL_JIT=always` and `RUSTYNAIL_JIT_T15=always` the threshold is 1 and every eligible nest is promoted at its first
back-edge (deterministic tests). `on` mode only.

## 4. Code generation (`t15::codegen`, `t15::alloc`, `t15::stubs`)

### 4.1 Registers

- Pinned (unchanged, callee-saved, never allocated): `rbx` base, `r12` thread, `r13` ci, `r14` kcache, `r15` &budget,
  `rbp` frame pointer, `rsp`.
- Free in compiled code: the 9 caller-saved GPRs `rax rcx rdx rsi rdi r8 r9 r10 r11` and `xmm0-xmm15`. Every one of
  them is clobbered by `CallH` (SysV).
- Region assignment [D]:
  - allocatable GPRs: `rsi rdi r8 r9 r10` (IR `V0..V4`) for INT, Bool (tag byte) and TABLE/collectable pointers;
  - `r11` (`V5`) = **budget counter register** (`[r15]` cached; written back at every flush);
  - scratch GPRs: `rcx` (`V6`, variable shift count), `rax` (`V7`), `rdx` (`V8`);
  - allocatable XMM: `xmm2-xmm15` (14) for FLT; scratch `xmm0`, `xmm1`.
  - Float constants are memory operands from `K` (`mulsd xmmN, [r14 + 16k + 8]`), or materialised in scratch for
    `LOADF`-style immediates. They are not allocated.
- Not done in v1 (decision 9.6): taking `r14` (callee-saved, so it would survive helpers) into the allocatable set.

### 4.2 IR changes (`ir.rs`, `emu.rs`, `x86.rs`, encoder)

Must have:
```rust
pub const NV: usize = 9;          // V7 = rax, V8 = rdx: allocatable by region code only, clobbered by SDivRem/UDiv
                                  // (rax, rdx), ReloadFrame (uses V7/V8 explicitly), CallH (all), Ret (rax, rdx)
pub type X = u8; pub const NX: usize = 16;   // xmm0..xmm15; X0/X1 are the scratch of FAdd/FSub/... (unchanged)
Op::LdF { dst: X, m: M }          // movsd xmm, [m]
Op::StF { m: M, src: X }          // movsd [m], xmm
Op::MovF(X, X)                    // movaps
Op::FBin { op: FOp, dst: X, src: X }      // FOp = Add Sub Mul Div Min Max (addsd ... ; Min/Max only for intrinsics)
Op::FBinM { op: FOp, dst: X, m: M }       // second operand in memory (slot payload or K)
Op::FSqrt(X, X)                   // sqrtsd
Op::FNeg(X)                       // xorpd with sign mask (built in scratch via V7)
Op::GtoF(X, V) / Op::FtoG(V, X)   // movq
Op::I2Fx(X, V)                    // xorps x,x; cvtsi2sd x, r   (dependency break)
Op::FBrX { cc, a: X, b: X, to }   // ucomisd + jcc with the NaN rules of FBr
Op::BrM { cc, w, m: M, imm: i32, to }     // cmp m8/m32/m64, imm ; jcc   (tag guards on slots, polls)
```
Nice to have (T1b): an indexed memory operand `Mi { base, idx: V, scale: 1|8, off }` for array element access (saves 2-3
ALU ops per access), and `cvttsd2si` + range check for `F2I_EXACT` (float keys, `math.floor`).

Emulator: `x: [u64; NX]` bit patterns; each new op gets the obvious semantics, with IEEE behaviour copied from the
existing `fop`/`FBr` code. `CallH` stays `Stop::Unsupported`. New: `emu::run_until(prog, start, stop: &[L], env, steps)`,
which stops at the first `Bind` of a label in `stop` (shadow verification, section 7). Native vs emulator differential
tests are extended with random XMM/GPR states (`tests/emu_diff.rs` pattern).

Tier-1 templates are not changed by T1 (they keep using `V0..V6`). That keeps tier-1 machine code byte-identical, which
is checked by comparing `RUSTYNAIL_JIT_DUMP` outputs before and after T1 on the 25 programs.

### 4.3 Register allocation: linear scan over the region

Input: blocks of the region in bytecode order (= a linear order in which every loop is contiguous), the facts, liveness
(backward dataflow over slot bitsets `[u64; 4]`; region exits are live-out for all slots), and the instruction classes.

```
values: for each (slot s, class c) with c ∈ {G (INT/Bool/TAB/...), X (FLT)} where s has a singleton fact of class c
        at some point where it is live: one interval [first point, last point] in linear order (holes merged);
        if the interval intersects a loop L and s is live at L's head with class c, extend it to cover L entirely
        (loop-carried: one fixed location for the whole loop, so no shuffles on back-edges)
weight(v) = Σ over uses/defs of v: 10^(loop depth at the use)   (+ 50% for loop-carried values)
order: by interval start; active list; on overflow evict the active-or-new interval with the lowest weight per
       remaining length (Poletto with a weight tie-break); an evicted value is SLOT-RESIDENT for its whole interval
hints: a def whose first operand dies at it gets that operand's register (two-address x86 forms)
constraint: across a barrier op every register value is dead in the machine sense (CallH clobbers everything), but it
       keeps its register: the flush stores it, the stub reloads it. The assignment stays fixed.
```

- **Spill = slot-resident.** A value without a register is read and written through its VM slot with its type fact (no
  tag guards). An unallocated FLT costs `movsd`/`addsd [m]` memory operands. No machine-stack spill slots exist: the slot
  is the home, so exits need nothing extra.
- **Write-back on release (rule W).** When a dirty value's interval ends (its last use), the allocator emits its
  write-back (tag byte + payload) at that point, unless the slot is redefined before the next observation point within
  the same block. v1 implements only the first part (always store at release). The "unless" part is pass `+dse` (later).
- **`RUSTYNAIL_JIT_OPT=+randra=SEED`:** random eviction order and random refusal of up to half of the registers (stress
  mode, section 7). `-ra`: no registers at all (typed and slot-resident only; isolates allocator bugs from typing bugs).

### 4.4 Locations, dirty sets, write-back

During emission the code generator keeps the **point state** for the current pc:
```rust
struct PointState { loc: [Loc; 256], dirty: SlotSet, memtag: [u8; 256] /* tag known to be in the slot, 0xFF = unknown */ }
enum Loc { Slot, G(V), X(X) }
```
- A typed def of a non-collectable value into an allocated register marks the slot dirty and does not store it.
  Collectables are write-through (R-G1) and never dirty.
- **flush(P)** = for each dirty slot `s`: store its tag (skipped when `memtag[s]` already equals it) and payload;
  `mov [r15], r11d`; then clear dirty. Used before every barrier op, fallback and exit.
- **At joins** the predecessor states must agree on `loc` (guaranteed by the fixed assignment). `dirty` at a join = the
  union (writing back a clean value is idempotent). `memtag` = per-slot equality or unknown.
- **Back-edges** carry dirty sets around the loop (loop-carried values stay dirty for the whole loop and are written
  back at its exits).

### 4.5 Exit stubs

Every guard or miss in region code jumps to an exit stub built from the exit's map `Vec<(slot, Loc, tag)>` (= the dirty
set at that point) and its target:
```
exit_k:   [type exits only] mov rax, imm64(&prof[pc]); mov byte [rax], <observed tag reg>;
                            mov rax, imm64(&exit_ctr[k]); add dword [rax], 1; cmp dword [rax], EXIT_HOT; jae event_k
          for (s, loc, tag) in map: mov byte [rbx+16s], tag ; mov/movsd [rbx+16s+8], loc
          mov [r15], r11d
          jmp tier1_label(target)          // target = pc (re-execute) or tier-1 exit_trap(pc) / exit_at(pc) labels
event_k:  flush as above; CallH jit_t15_event(k); JmpR(rax = continuation address in the new code, or tier1_label)
```
- Stubs are deduplicated by `(target, map, kind)` and emitted out of line after the region (like tier-1's lazily
  referenced exits).
- **Why not LuaJIT's shared trampoline with a Rust table-driven restore (plan s14 item 4)?** Here the deopt target is
  machine code in the same frame, not the interpreter. An inline stub costs about 2 stores per dirty slot plus a jump
  (5-20 cycles). A trampoline would cost a register save, a Rust call, a map walk and an exit to the interpreter
  (hundreds of cycles plus interpreter time until the next entry). The inline stub is also plain IR, so the emulator
  runs it under Miri, which a native trampoline would not allow. Code size: typical maps hold 2-6 entries (about 11
  bytes each), with 20-60 guards per region, so a few KB per region; it is charged to the code cap. Revisit if a region
  needs more than 16 KB of stubs (open question 10.8).
- "Snapshot compression": the dedup above plus the dirty-only maps are the analogue of LuaJIT's sparse and merged
  snapshots. Nothing more is needed because frames are never reconstructed.

### 4.6 Resume stubs and the preheader

```
stub_P:   // entered with all values in their slots and rbx/r13/r14 valid (from tier 1, the resume table, after a helper)
          for s in typed_live(P):   cmp byte [rbx+16s], tag(fact(P,s)); jne fail_P        // BrM
          for step speculation:     cmp qword [rbx+16(A+1)+8], c; jne fail_P
          for hoisted builtins:     <load upvalue/global value>; cmp tag; cmp payload, imm64(fn addr); jne fail_P
          for v in registers(P):    mov/movsd/movzx reg(v), [rbx+16s+8 or +0 for Bool]
          rematerialise R-DERIVED values cached at P (table headers, upvalue table pointers)
          mov r11d, [r15]
          jmp region_P
fail_P:   (count unless peel) ; jmp tier1_label(P)
```
- The **preheader is `stub(H)`**. Every edge from tier-1 code into `H` (fallthrough from FORPREP, the tier-1 copy's
  back-edges, the resume table entry of `H`) is retargeted to it.
- Resume-table entries inside the region (loop heads, after-CALL pcs that are templates) point at `stub(P)`; all other
  resume entries keep pointing at tier-1 labels.
- Fallback N reload = the load part of `stub(P+1)` without guards.

### 4.7 Typed templates (examples; `[x]` = slot-resident memory operand)

- `ADD a b c`, all FLT: `a==b`: `addsd Xa, Xc|[c]`; `a==c` (commutative): `addsd Xa, Xb`; else `movaps Xa, Xb;
  addsd Xa, Xc`. `SUB`/`DIV` with `a==c`: compute in `xmm0`, then move. INT: `lea`/`add`/`imul` likewise; `MULK` with a
  constant outside imm32 goes through `V7`.
- `ADDK a b k` FLT: `addsd Xa, [r14+16k+8]`.
- `LT a b` (+ JMP follower) FLT: `ucomisd`, then branch with the exact NaN rules of `FBr` (never by negating the
  condition code). INT vs FLT mixed: as tier 1 (exact within ±2^53, else exit).
- `FORLOOP A` (5.5, INT): `test Cnt,Cnt; je fall; sub Cnt,1; add Ctl, Step|imm; poll(H); jmp region_H` (`Cnt`/`Ctl`
  dirty; `Step` is a register, an immediate (speculated), or `[A+1]`). FLT loop: `addsd Ctl, Step`, then compare with the
  limit using the sign of the step (known statically when speculated).
- `FORPREP` inside a region: the typed version of `t_forprep`; with a speculated step `1` the count is `limit - init`
  (no `div`); unusual operands exit to tier-1 FORPREP.
- `TEST a k` Bool in a GPR: `cmp Ra8, FALSE` (nil has its own tag: a Bool fact excludes it). Any: tier-1 rule on the
  slot tag.
- `MOVE a b`: register copy (or a new location binding when `b` dies); collectable: also the slot store (write-through).
- `GETTABLE` / `SETTABLE`: section 5.
- Template contract: pure until committed. Guards come first; results go to registers or slots only after the last
  guard (E3). A guard failure leaves registers and slots as the exit map says.

### 4.8 The fallback protocol (reusing tier-1 templates verbatim)

```
fallback(P, class):
    flush(P)                                          // class Local: GPR values + budget only
    cx.ty = [Unk; 256]                                // tier-1 block-local knowledge is not used
    with cx.pcl remapped: pcl[P+1] -> after_P         // fallthrough continues in the region
        cx.template(P, ins[P], op)                    // the tier-1 code: may exit to the interpreter (its own
                                                      //   exit_r labels: refunds as in tier 1), flat-call, call helpers
    bind after_P: class Local/N: reload registers ; class G/Call: stub(P+1) ; then region_{P+1}
```
- Only straight-line instructions (falling through to `P+1`, or leaving the frame) may be fallbacks. Control
  instructions (compares, TEST, FORLOOP, TFORLOOP, JMP) are always region templates: a typed form, or the untyped
  dual-path form on slots with scratch registers.
- The tier-1 template's own exit labels (`exit_r`, `exit_trap`, `slow_label`) are shared with the tier-1 copy in the same
  `Cx`. Because the state was flushed, those exits are exactly tier 1's.

### 4.9 Counting (C1) and polls

- Region blocks are exactly the tier-1 blocks (`head[]`, `cnt_at[]`, `rem[]` from `lower()` are computed once and
  shared). Each region block head emits `sub r11d, cnt_at[P]; js cexit_P`, where `cexit_P` = flush, then `jmp` to
  tier-1's `exit_at(P)` label (savedpc = P, `Continue`; no refund, as tier 1). An exit into tier-1 `P` mid-block
  therefore finds the same pre-paid state that tier 1 would have.
- Back-edge poll: `cmp dword [r13+TRAP], 0; jne t; mov rax, imm64(INTERRUPT); cmp byte [rax], 0; jne t` (5 instructions
  instead of 7). `t` = flush + tier-1 `exit_trap(H)` label.
- `RUSTYNAIL_JIT_OPT=-cntreg` keeps tier-1's `sub [r15]` in regions (bisection and measurement of O7).

### 4.10 Invariants of region code (the review checklist)

1. **T-EXACT:** at every observation point (R-OBS) the VM stack equals the interpreter's (R-EXACT); the budget word
   `[r15]` equals tier 1's at the same point.
2. **T-DEOPT:** every exit lands on a tier-1 label of the same program, with flushed state, at a pc whose tier-1 code
   re-executes the instruction (or on tier-1 `exit_at`/`exit_trap` labels with tier 1's own `savedpc`/`trap` rules).
3. **T-ENTRY:** region code is reached only through `stub(P)`; a stub reads only slots and checks every fact the code
   after `P` relies on.
4. **T-G1:** collectables are write-through; register copies of collectables and derived pointers die at barrier ops.
5. **T-BARRIER:** every barrier op is bracketed by flush and `stub(P+1)`; every helper call is inside a tier-1 template
   emitted by the fallback protocol (region code itself emits no `CallH` except `jit_t15_event`/`promote`, which are
   preceded by a flush).
6. **T-PURE:** region templates are pure until committed (E3); inline table stores never insert or resize.
7. **T-COUNT:** same block heads and counts as tier 1 (C1).
8. **T-FLOAT:** no FMA, no reassociation, no `-ffast-math` style folding; NaN/-0.0 behaviour as `FBr`/`fop`.
9. **T-5.5:** region lowering refuses 5.4 prototypes until T9b (5.4 FORLOOP layout, `alimit_touch` in typed array ops).

### 4.11 Interactions, one by one

- **`debug.getlocal`** (callee, error handler, traceback): exact by T-EXACT; temporaries included.
- **`debug.setlocal` of a register-allocated local:** possible only inside a barrier op (R-OBS), so it is seen by
  `stub(P+1)`. If the type changed, the guard fails and the frame continues in tier 1. Allocation is never refused
  because a function might be debugged.
- **Hooks:** compiled code (tier 1 or region) is never entered with a hook active. A hook installed during a helper (a
  finalizer, another coroutine) is noticed by the tier-1 template's `hookmask` test after the helper (inside the
  fallback), or by the next back-edge poll (`trap`). Both leave with flushed state.
- **GC:** GC runs only inside helpers, hence inside barrier ops (flushed; collectables were in slots anyway, R-G1). `top`
  is set by the tier-1 template exactly as in tier 1. Stack shrink/move: `ReloadFrame` in the template, and `stub`
  addresses everything through `rbx`.
- **Stack reallocation:** as GC (R-STACK). `RUSTYNAIL_JIT_STRESS=stackmove` applies unchanged.
- **Errors from helpers:** the error exit happens inside the tier-1 template after the flush, so handlers see the exact
  frame. Inline region code never raises.
- **Coroutines:** a yield is an error exit inside a helper (flushed). Resume goes through `finish_op` -> E1 resume table
  -> `stub(P+1)` with full guards. `debug.setlocal(co, ...)` on the suspended coroutine is covered by those guards.
- **Calls:** a flat call leaves the region's native code (`JmpR`). The callee's return comes back through the resume
  table into `stub(P+1)`. Natives return into the fallback's `after_P`.
- **Code lifetime:** suspended native activations of an older version of the code (a helper deeper on the native stack
  that was called from the old code) continue in the retired block, which stays mapped until the prototype is freed
  (section 6.1).

## 5. Tables and builtins

### 5.1 Array and field access

- **Typed array read** (`GETTABLE a t k` / `GETI`, `t` TABLE, `k` INT, element speculated `T`):
  ```
  lea rax,[Rk-1]           ; u (Rk = register or [k] load)
  mov edx,[Rt+ASIZE] ; cmp rax,rdx ; jae miss          ; one unsigned compare: k < 1 and k > asize
  mov rdx,[Rt+ARRAY]
  cmp byte [rdx+4+rax], tag(T) ; jne type_or_miss     ; element present AND of the speculated type
  shl rax,3 ; sub rdx,rax ; mov/movsd Ra,[rdx-8]
  ```
  About 9 instructions, against about 18 in tier 1 plus the tag/payload stores of the result and the reload by the next
  op. `type_or_miss` checks `tag & 0x0F == 0` and goes to the miss exit if so (nil: maybe `__index`, a tier-1 matter);
  otherwise it is a type exit (feedback). Tables without a metatable fact are not needed for reads (nil always exits).
- **Typed array write** (`SETTABLE t k v` / `SETI`): bounds as above; slot non-empty, *or* `[Rt+METATABLE] == 0`
  (tier-1 rule); **no barrier test when `v` is statically INT/FLT/Bool/nil** (T-G1: the interpreter's `barrier_back`
  is a no-op for non-collectables); collectable values use tier-1's `barrier_exit` condition (exit when the table is
  black and the value white). The element's tag byte is always stored, because the old element tag is not known
  statically.
- **5.4:** every inline integer access performs `alimit_touch` exactly as `templates_table.rs` does (T9b).
- **Header hoisting (`tabhdr`, pass `hoist`):** for an *invariant* table slot in a region, `ARRAY` and `ASIZE` are loaded
  once per `stub` entry and kept in registers *only if* registers are left after the value allocation (weight below
  every value). Otherwise they are memory operands (`cmp rax,[Rt+ASIZE]`): an L1 hit with no store chain, so the
  saving is about one load per access. Valid per R-DERIVED (killed at barrier ops, reloaded by `stub`).
- **Bounds-check hoisting: not in J4** (decision 9.2). The per-access check is 1 compare + 1 predicted branch.
- **Field probes (`GETFIELD/SETFIELD/GETTABUP/SETTABUP`, constant short-string key, pass `fcache`):**
  - Main-position speculation: guard `byte [Rt+LSIZENODE] == L` (`L` observed at promotion from the table in the frame
    or `prof`; otherwise the generic walk), then the node of the key's main position is `[Rt+NODE] + (hash & (2^L-1)) *
    24` with a compile-time constant offset. Then `cmp byte [node+KEY_TT], SHRSTR; jne walk; cmp [node+KEY], Rk; jne
    walk` (key pointer from `K`, never embedded: G1). `walk` = tier-1's chain walk (rare for small tables). This saves
    the shift/mask/multiply (5-6 instructions) per access.
  - **Probe CSE:** within a barrier-free stretch, the same (table value, constant key) reuses the node pointer, so
    `bj.vx = bj.vx + ...` probes once for the get and the set. The node is valid per R-DERIVED (no insertion without a
    barrier op).
  - Values: speculated type from `prof[pc]`. SETFIELD of a non-collectable value needs no barrier test (the node is
    known to hold a non-nil value, so there is no `__newindex` question; [V] tier-1 rule).
- **Upvalue loads (`GETUPVAL`, pass `hoist`):** if no `SETUPVAL` of the same index is in the region, the upvalue's value
  is invariant between barrier ops. It is loaded in `stub` (closure -> `UpVal` -> `v`), kept in a register or re-read,
  and its type is guarded there. heapsort's `a`, queens' `cols/d1/d2`, nbody's `sqrt`.
- **Not done:** load/store forwarding of table values across iterations or across stores to possibly aliasing tables
  (LuaJIT `-O-fwd`, worth about 25% on nbody there); alias analysis; allocation sinking (decision 9.3).

### 5.2 Builtin intrinsics (pass `intr`)

`CALL` whose callee slot comes from `GETUPVAL`/`GETTABUP`/`GETFIELD` and whose value was observed to be one of the
registered builtins (light C function payload == address): `math.sqrt` (`sqrtsd`), `math.abs` (INT: `neg`+`cmov`; FLT:
`andpd`), `math.floor`/`math.ceil` on FLT (result INT when representable, else exit; needs `roundsd` or the
`cvttsd2si` sequence), `math.min`/`math.max` with 2 arguments of the same type. Guard: the callee value's tag and
payload equal the builtin's (`LCF` tag + address), checked in `stub` when the callee is invariant (hoisted upvalue) or at
the call otherwise. Argument and result counts must match exactly (`B`, `C`). Anything else falls back to the CALL
protocol. Builtin addresses are registered like `next`/`ipairs` today ([V] `jit_hooks::set_builtins`,
`rn_std::lib_base::jit_builtins`): add `lib_math::jit_builtins()` -> (sqrt, abs, floor, ceil, min, max). The
intrinsic's result is a static type (FLT for sqrt).

## 6. Runtime (`rt.rs`, `calls.rs` header, new `t15/mod.rs`)

### 6.1 Promotion and recompilation

```
extern "C" fn jit_t15_promote(thread, ci, nest_id, _) -> u64    // called from warm-up tier-1 code at a nest back-edge
    st = State::from_th_ptr(thread); jp = descriptor of ci's proto (live: the frame holds the closure)
    obs = tags of slots [0, maxstacksize) of the frame;  prof = jp.t15.prof
    plan = analysis::plan(code, consts, nest, obs, prof, policy state)      // None -> nest Off, return 0
    low = lower_with_regions(code, cfg, &[plans of all promoted nests])     // tier-1 lowering + region code
    em = x86::emit(low.prog); charge; reopen_tail/alloc/write/commit as compile_inner does
    retire old block (push to jp.retired); jp.block = new; rebuild jp.resume; jp.hdr = {entry0, base_m1, resume}
    jp.pcmap = native offset of tier-1 label per pc (for continuations)
    return address of stub(back-edge target) in the new code, or 0 = "continue in the old code"
```
- The warm-up code calls it after `CountSub`, with the frame consistent. It does `JmpR(rax)` when the result is nonzero
  (same `rbx/r13/r14`, same frame), else continues. Re-entrancy: like `jit_call`'s compiles today; the arena is resealed
  before the helper returns (J-PACK holds: no generated code runs during the compile).
- `jit_t15_event(thread, ci, site, kind)` (from exit stubs and stubs, after a flush): updates counters and policy,
  possibly recompiles, and returns the continuation = new code's tier-1 label for the target pc (from `pcmap`), or 0.
- **Retired blocks** stay mapped and charged until `free_code` frees the prototype (all blocks). With at most 6 compiles
  per prototype the waste is bounded. In `verify` mode retired blocks are not quarantined, because a helper deeper on
  the native stack may still return into them.
- **Descriptor:** `JitProto` keeps its address for life (compiled callers and returns read `hdr` at use time; [V]
  `calls.rs`). A recompile only swaps `block`, `resume`, `hdr.*`, `pcmap`. `T15Data` is allocated *before* the first
  `lower()`, because warm-up code embeds the addresses of its counters/profile bytes, and it lives as long as the
  descriptor.

### 6.2 Data

```rust
struct T15Data {                       // Box, address embedded in machine code (not a GC object: G1 unaffected)
    prof: Box<[u8]>,                   // last observed result tag per pc (0xFF = never)
    nests: Vec<NestState>,             // candidate nests found at tier-1 compile time
    exit_ctr: Box<[u32]>, miss_ctr: Box<[u32]>,      // per exit site of the current code version
    compiles: u8,
}
struct NestState { head: u32, end: u32, promote_ctr: i32, state: Warm | Promoted { gen: u8 } | Off,
                   entry_fail: u32, entry_ok: u32, spec_overrides: Vec<(u32 /*pc or slot*/, TagSet)>, backoff: u8 }
```
Memory is charged with `charge_code` (descriptor bytes) like the existing descriptors ([V] `rt::descriptor_bytes`).

### 6.3 Knobs

| variable | values | meaning |
|---|---|---|
| `RUSTYNAIL_JIT_T15` | `off` (default until the J4 gate), `on`, `always` | tier 1.5 on/off; `always` = promote every eligible nest at its first back-edge (tests) |
| `RUSTYNAIL_JIT_T15_HOT` | n | promotion threshold (default 4096 back-edges per nest) |
| `RUSTYNAIL_JIT_OPT` | comma list | `-ra` (no registers), `-xmm` (floats in GPR/slots), `-hoist`, `-fcache`, `-intr`, `-cntreg`, `-calls` (no regions containing Call-class ops), `-nest` (innermost loop only), `-peel` (count peel failures as failures), `+randra=SEED`, `+exitstorm=N`, `+flushall`, `+verify` |
| `RUSTYNAIL_JIT_VERBOSE=1` | | adds per region: pcs, slots typed/allocated, stub/exit counters, respecialisations, Off reasons |

## 7. Test strategy

1. **Emulator/native differential (T1):** every new IR op against the emulator on random register states;
   x86 encodings against iced-x86/objdump ([V] existing oracle tests in `arch/x86_64/tests.rs`).
2. **Analysis unit tests (T2):** def/use table vs `luac -l` listings; fixpoint results on hand-written functions
   (type-stable loops, int-to-float accumulators -> peel, polymorphic loads -> Any, escaped slots); region selection
   (nests, side entries rejected, size cap). Pure Rust, Miri-run.
3. **Region-vs-interpreter twin programs (T4-T7, `crates/rn-jit/tests/t15_*.rs`, harness like `jit_j3c.rs`):** every
   program runs with `RUSTYNAIL_JIT=off` and with `always + T15=always` (plus `on`), with count budgets 3/17/200 ([V]
   existing pattern), and is compared. Required programs:
   - type instability at iteration N for N in {1, 2, 3, 100} for each of: a live-in, a loaded value, a loop-carried
     accumulator, a table element; int/float/string/nil/table mixes;
   - `debug.setlocal` from a callee in the loop, from a `__gc` finalizer at an allocation in the loop, and from another
     coroutine while this one is suspended in the loop; `debug.getlocal` of every slot including temporaries from a
     callee and from an `xpcall` handler after an error inside the loop;
   - closures capturing loop locals and callees writing them (escaped slots); `debug.upvaluejoin`/`setupvalue` on
     running closures;
   - hooks installed by finalizers and by callees while a region runs; Ctrl-C (`INTERRUPT`) inside regions;
   - coroutine yield inside region loops (Lua and native callees); resume after `collectgarbage()` shrinks the stack;
   - deep recursion from region loops (stack growth, `stackmove`); GC stress 1/2/verify; both collectors;
   - array loads with holes, out-of-bounds, metatables with `__index`/`__newindex`, insertion/rehash in the loop
     (sieve pattern), shape changes between calls; field probes with collisions and `lsizenode` changes;
   - intrinsics with a replaced `math.sqrt` (`math.sqrt = function ... end` mid-run), wrong argument counts, ints vs
     floats, NaN/-0.0/inf.
4. **Shadow verification of side-effect-free blocks (`RUSTYNAIL_JIT_OPT=+verify`, T8).** For each region block whose
   instructions are all Typed and store nothing to the heap, the compiler emits: `flush; CallH verify_pre(block)` (copy
   slots `[0, maxstacksize)` and the budget to a shadow buffer); the region block; at each successor edge or exit
   `flush; CallH verify_post(block, succ)`, which runs the **tier-1 IR of the same block in the emulator**
   (`emu::run_until` from `pcl[block start]` to the next block-head label or `Ret`) on the shadow copy and compares all
   slots (tag byte, and payload bits for numbers) and the successor. Mismatch: abort with pc, slot and both values.
   Reloads follow each `CallH`. This checks per-block semantics and every write-back map; register persistence *across*
   blocks is covered by 5 and 6. (The emulator reads the real tables; a tier-1 slow path `CallH` in the emulated block
   makes the check inconclusive and it is skipped.)
5. **Random register assignment (`+randra=SEED`)** over all twin programs and the corpora, with several seeds per CI run.
6. **Exit storm (`+exitstorm=N`):** every guard and every instruction boundary of region code gets a pseudo-guard
   that fails every N-th execution (per-site counter in `T15Data`; N in {1, 2, 7, 61}). Results must equal `off`. This
   exercises every exit map, every tier-1 re-entry mid-block, and the counting of C1. `+flushall`: flush after every
   instruction (isolates dirty-tracking bugs).
7. **Corpora and suites:** official 5.5 suite with `always + T15=always`; `tests/vm` corpus; jit-diff (`scripts/jit-diff.sh`
   gains `JITDIFF_T15=1` and an OPT variant list); the `jit_diff` fuzz target's first byte also selects T15 mode, randra
   seed and exitstorm period; ASan (`scripts/asan-jit.sh` + T15); Miri on analysis, alloc, emulator, stub generation.
8. **Mutation checks** (each must make a test fail; recorded in the task report): drop the flush before a barrier op;
   drop one entry of an exit map; skip `stub` re-guards after a fallback G; keep a table pointer across a barrier;
   remove the peel exemption (the policy test must see a respecialisation).
9. **Performance gates** per task (section 8) with `scripts/bench-cycles.sh` (median of 7, ±5% band), and a check that
   `RUSTYNAIL_JIT_T15=off` code is identical to tier-1 code (dump compare) and costs nothing.

## 8. Task breakdown (Sonnet builders; at most 2 in parallel, plan s13)

Ordering: **T1 ∥ T2 -> T3 -> T4 -> (T5 ∥ T8) -> T6 -> T7 -> (T9, T10 optional)**. Each task: its file list, invariants
from 4.10 quoted verbatim, the gates below, CLAUDE.md memory caps. The builder stops and reports on any deviation.

| # | task | files (disjoint where possible) | acceptance | review |
|---|---|---|---|---|
| **T1** | IR + backends: `NV=9` (V7/V8), XMM file `X0..X15`, ops of 4.2 (must-have), encoder forms (`movsd m`, `movaps`, `sqrtsd`, `andpd/xorpd`, `cmp m8/m32/m64, imm`, `cvttsd2si`, optionally `roundsd`), emulator ops + `run_until` | `ir.rs`, `emu.rs`, `x86.rs`, `arch/x86_64/{asm,regs,tests}.rs`, `tests/emu_diff.rs` | all rn-jit tests; new op diff tests (emu vs native, 10k random states), encoder oracle tests; tier-1 dumps byte-identical on the 25 programs | light (mechanical) |
| **T2** | analysis: CFG/loops/nests, single-entry, def/use table, liveness, TagSet lattice, fixpoint with spec inputs and peel detection, instruction classes, escaped slots, `RegionPlan` | new `src/t15/analysis.rs`, `src/t15/mod.rs` (types only), `tests/t15_analysis.rs` | unit tests of section 7.2 incl. Miri; plans for the 10 benchmark hot nests printed and checked by hand against `luac -l` | **Opus** (soundness of transfer functions and barrier kills) |
| **T3** | runtime plumbing: `T15Data`, warm-up instrumentation (`LowerCfg.t15_warmup`: promote counters at nest back-edges, `prof[pc]` stores in loads), `jit_t15_promote`/`jit_t15_event` helpers, recompile path (retired blocks, resume/hdr/pcmap swap, charging), knobs, VERBOSE; `lower_with_regions` that for now emits *no region* (the recompiled program = tier 1 again) | `rt.rs`, `calls.rs` (header doc only), `lower.rs` (cfg flag, label retargeting hook), new `src/t15/runtime.rs`, `tests/t15_runtime.rs` | everything green with `T15=always` (recompiles happen, behaviour unchanged); code lifetime tests (proto freed while a helper is still inside retired code: `tests/jit_review_uaf.rs` pattern); memory-limit charging; warm-up cost ≤ 1% cycles on the 25 programs with `T15=on` | **Opus** (lifetime, re-entrancy) |
| **T4** | region codegen core: allocator (4.3), point state/flush/exit and resume stubs (4.4-4.6), fallback protocol (4.8), counting/polls (4.9), typed arithmetic/compare/TEST/MOVE/LOAD/JMP/FORPREP/FORLOOP (INT, FLT), dual-path untyped forms; `-ra`, `+flushall` | new `src/t15/{codegen,alloc,stubs}.rs`, `tests/t15_core.rs` | twin programs of 7.3 (arith/loop subset) in all modes incl. GC stress, stackmove, count budgets; official suite with `T15=always`; **loops_arith ≤ 0.32, mandelbrot ≤ 0.26** (pessimistic column); no program slower than tier 1 beyond the band | **Opus** writes the reference for one op class end to end (plan s13: "the hardest piece") and reviews |
| **T5** | tables in regions: typed GETTABLE/GETI/SETTABLE/SETI, field probes with main-position speculation + probe CSE, GETUPVAL/GETTABUP hoisting, miss vs type exits | new `src/t15/tables.rs`, `tests/t15_tables.rs` (+ `table_diff`-style region variant) | table twins of 7.3 (holes, metatables, rehash, collisions, both collectors, barrier matrix for collectable stores); **matmul ≤ 0.26, fannkuch ≤ 0.34, heapsort ≤ 0.45, sieve ≤ 0.68** | Opus review of the barrier/metatable conditions |
| **T6** | calls and intrinsics in regions: CALL/TAILCALL/RETURN/TFORCALL via the fallback protocol with `stub(P+1)` resume entries; builtin registration for `lib_math`; intrinsics of 5.2 | new `src/t15/calls.rs`; `rn-std/src/lib_math.rs` (+`jit_builtins`), `rn-core/src/jit_hooks.rs` (builtin table) | call/coroutine/hook/setlocal twins of 7.3; **nbody ≤ 0.33, spectral_norm ≤ 0.55, queens ≤ 0.47** | review: yes (call protocol) |
| **T7** | policy: exit/miss/entry counters, respecialisation, `gen` cap, Off/blacklist, promotion back-off, peel exemption; VERBOSE report | `src/t15/policy.rs`, `src/t15/runtime.rs` (event handling), `tests/t15_policy.rs` | thrash tests (a loop alternating types every K iterations for K in {1, 7, 1000}: bounded compiles, no slowdown beyond the band vs tier 1); respecialisation tests | **Opus** |
| **T8** | verification tooling: `+verify` shadow mode (helpers + emitted brackets), `+randra`, `+exitstorm`, jit-diff/fuzz knobs, CI steps | `src/t15/verify.rs`, `scripts/jit-diff.sh`, `fuzz/fuzz_targets/jit_diff.rs`, `scripts/ci-local.sh`, `tests/t15_stress.rs` | the stress modes run green on all t15 twins and the official suite; each mutation of 7.8 is caught | review: yes |
| T9 (opt) **[SKIPPED 2026-10-30, user decision: (a) is superseded by J5 call inlining, (b) 5.4 gets traces in J5 F7]** | (a) function regions: loop-free functions specialised at pc 0 on observed parameter tags (spectral `A`, fib); (b) 5.4 regions (FORLOOP54 layout, `alimit_touch` in typed array ops) | `src/t15/*` | (a) spectral_norm -0.05, fib no regression; (b) the 5.4 suite + twins under `--cfg rustynail_lua54` | Opus for (a) |
| T10 (opt) **[SKIPPED 2026-10-30, user decision; revisit only (c) `MODK/IDIVK` multiply-high if T13 shows loops3 holding the numeric geomean above 0.30]** | (a) `setlocal_epoch` re-guard elision (rn-core `lib_debug`/`lapi_debug` bump under `cfg(feature="jit")`, F1 `.text` identity check); (b) `+dse` delayed temp stores; (c) `MODK/IDIVK` by constants as multiply-high (tier 1 and regions) | small, separate | measured gain ≥ 3% on at least two programs or dropped | review |

Notes for the briefs:
- T4 is the large one. Split it at the template boundary if needed: T4a (allocator, stubs, flush, fallback, FORLOOP
  INT, ADD/SUB/MUL INT, compares INT; gate on loops_arith loop 2 and 3), then T4b (FLT/XMM, FORLOOP FLT, dual-path forms;
  gate on mandelbrot).
- Every task keeps `RUSTYNAIL_JIT_T15=off` code identical to tier 1 (dump compare) and runs the whole rn-jit suite plus
  `cargo test -p rn-jit --features jit` with `RUSTYNAIL_JIT_T15=always`.
- Rollback: every pass has its `RUSTYNAIL_JIT_OPT` switch; the tier has `RUSTYNAIL_JIT_T15=off` (the default until the J4
  gate is accepted).

## 9. Decision points: recommended NOT to do (in J4)

1. **No separate tier-1.5 SSA/mid-level IR** (plan s14 item 3 / P3 proposed a ~45-op IR). The bytecode is already a
   register IR with explicit control flow. The analysis runs on it, and the code generator emits the existing micro-op
   IR. A second IR would double the emulator surface and the test burden for no measured gain.
2. **No bounds-check hoisting (ABC).** Saving 2 instructions per access needs proofs over `asize` and the loop bounds.
   The rule itself is easy (tables only grow, and only through barrier ops), but the gain is under 2% on the measured
   loops.
3. **No allocation sinking, no loop unrolling, no LuaJIT-style copy-substitution peeling.** The tier-1 copy already
   gives a one-iteration peel. LuaJIT's own ablations showed `-O-loop`/`-O-sink` invisible on these programs (design
   notes s11.1).
4. **No table value forwarding across iterations / alias analysis.** About 25% on nbody in LuaJIT, but it needs an
   alias model for every table store. Probe CSE (section 5) covers the same-iteration case cheaply. Revisit after J4 if
   nbody misses its gate.
5. **No table-driven exit restore / shared trampoline** (plan s14 item 4): replaced by inline write-back plus a jump
   into tier 1, which is faster, simpler and Miri-testable (4.5). This is a deliberate deviation from the plan text.
6. **No `r14` freeing, no machine-stack spill slots** in v1. GPR pressure is real (5 registers), but slot-resident
   values are cheap once their tags are known.
7. **No inlining of Lua calls**, and no regions for loop-free functions in the core tasks (T9 is optional). fib and
   spectral_norm's `A` are call-bound; LuaJIT's 0.11/0.04 there comes from inlining, which E2 and the design rule out.
8. **No change of C1 semantics.** Keep exact counting (in a register). Dropping it inside regions (compiled code is never
   entered with a count hook, so the budget only serves `set_count_budget` tests and the 2^31 reseed) would save a few
   percent at most and would need a plan change (open question 10.4).
9. **No promotion at the first OSR compile.** Promotion after a warm-up gives `prof[]` data and stable types. The cost
   is one extra compile per hot nest.
10. **5.4 regions are not part of the J4 gate** (T9b after 5.5 passes), as J2b followed J2.

## 10. Open questions (for the orchestrator / reviewer)

1. **Jumping into the middle of a tier-1 block.** Tier 1 uses block-local `ty` facts and skips tag stores when a slot
   "is known" to be INT/FLT ([V] `st_int`/`st_flt`). The argument that this is safe: after a flush the VM state equals
   the interpreter's at `P`, and tier-1's facts are sound statements about that state, whichever code produced it. This
   needs a reviewer's confirmation and the exit-storm test (7.6) as evidence. A safe fallback, if it is doubted: make
   tier-1 `ty` knowledge reset at every pc that is an exit target (costs some tier-1 tag stores on the deopt copy only).
2. Promotion threshold (4096 per nest) and `EXIT_HOT`/entry-failure ratios: initial values, to be tuned by T7
   measurements.
3. T10a (`setlocal_epoch`) touches rn-core and rn-capi; is a JIT-feature-only field in `Thread` acceptable under F1?
4. Keep exact C1 counting in regions, or allow per-iteration aggregated counting? (Decision 9.8 says keep.)
5. Default of `RUSTYNAIL_JIT_T15` at the J4 gate (`on` with `RUSTYNAIL_JIT=on`, or opt-in for one release).
6. Should regions refuse nests whose barrier ops run on the hot path every iteration (spectral's `A(i,j)` call,
   sieve's insertion loop)? The data says they still gain a little (5-15%) but pay re-guard stubs; `-calls` makes this
   measurable. Decide on T6 numbers.
7. The 5-register GPR budget: if T5 shows heavy slot residency on heapsort/queens/fannkuch, evaluate `r14` as an
   allocatable callee-saved register (decision 9.6) before anything else.
8. Code size: region + stubs + retired blocks per hot proto. Measure in T4/T5 (`code_bytes` in `jit_stats`) against
   the code cap and the short-script budget.
9. Measurements here are one Zen 5 laptop part; store-forwarding behaviour differs on Intel and on older Zen. The float
   gains in particular could shift either way.
10. `prof[pc]` keeps only the last tag. Is a 2-bit "monomorphic/polymorphic" state worth one more instruction in warm-up
   code? (Exit feedback already widens the speculation after misses.)

## 11. Risks

| risk | mitigation |
|---|---|
| Miscompile through a stale register after a barrier op | T-BARRIER rule, `+flushall`, `+verify`, mutation checks 7.8, Opus review of T4/T6 |
| Exit maps missing a dirty slot | maps derived from one `PointState` (no separate bookkeeping), exit storm at every instruction boundary |
| Thrash (exit/entry storms) | policy of 3.8, VERBOSE counters, `-peel` switch, thrash tests in T7 |
| GPR pressure (5 regs) makes table loops slot-resident | weights by loop depth; memory operands; open question 10.7 |
| Code-size growth (two copies + stubs + retired blocks) | compile cap of 6 per proto, region cap of 512 instructions, code cap charging (existing) |
| IPC collapse on throughput-bound loops (gain below the model) | pessimistic gates per task; stop and re-plan per plan s9 if T4+T5 miss the pessimistic gates |
| Projection error | numbers are [E]; the J4 checkpoint judges by measured cycles only |

## 12. Review amendments (binding; Opus adversarial review of 2026-10-29)

Verdict: architecture holds (tier-1 code in the same program is the deopt target; inline write-back; fallbacks bracketed by
flush and `stub`; whole-nest regions). Jumping into the middle of a tier-1 block is safe under three conditions: (1) every dirty
value, XMM ones included, is flushed before the jump; (2) the region reached P by running the same block prefix with
interpreter semantics, so a region entry never sits mid-block except after its own fallback; (3) block structure
(`head`/`cnt_at`/`rem`) is identical in every code version (assert when `jit_t15_event` continues into new code). Do NOT
reset tier-1's `Cx.ty` at exit targets; instead add a `+verify` mode that emits tier-1 `ty` assertions (memory tag checks) at
every exit-target label and run it with the exit storm. These amendments override earlier sections where they conflict.

**Design-blocking fixes (must be reflected before T3/T4):**
- **B1** Local fallbacks lose dirty XMM values (a tier-1 template can still `exit_r` to the interpreter for strings/MMBIN/zero
  divisor; cold table paths call `jit_get`/`jit_set`, clobbering all XMM). Every fallback flushes everything; the only saving of
  class Local is skipping the XMM reload. Define Local as "no CallH on any path, cold paths included". Reload `r11` afterwards
  (tier-1 templates use V5 = r11 as scratch).
- **B2** Cold slow paths rejoin through `pcl[next]` read at `emit_cold` time (templates_alloc.rs ~185), so the temporary label
  remap is not seen: store the rejoin label at template time in `HCall`/`Cold::Slow` (tier-1 bytes unchanged). T4 may touch
  `lower.rs` and `templates_alloc.rs`/`templates_table.rs` for this.
- **B3** C1 double count: a region guard failing at the first instruction of a block head P, after the region's own `sub r11d`,
  jumps to tier-1 `pcl[P]` which counts again. Add a post-count label per head (or refund `rem[P]` in the stub).
- **B4** Per-version data reached from retired code: site counters/tables are owned by the code block (retired with it), the event
  helper takes a version id, `promote` is idempotent when the nest is already promoted, `NestState` has stable addresses (no
  growable Vec). Old stubs must never write into freed memory.
- **B5** Guard set per stub/entry = typed uses read before redefinition (upward-exposed) plus slots loaded into registers; the
  write-back set of dead dirty values (R-EXACT) is separate. Do not guard every live-out slot.

**Must-fix items for the briefs:**
- M1 `memtag` becomes Unknown after every barrier op or helper for every slot not re-guarded (callee may `debug.setlocal` a dead
  temporary); flush must not skip the tag store on stale memtag.
- M2 class-N fallbacks guard what they define (or define Any): `jit_get/jit_len/jit_pow` write results via `st.set`.
- M3 upvalue hoisting only when the region has no SETUPVAL at all; escaped slots are write-through and always re-guarded; `stub`
  checks that `UpVal.v` is outside the current frame (debug.upvaluejoin).
- M4 intrinsics copy CALL exactly: `CountSub{1}` after the guards, a guard failure exits to tier-1 CALL uncounted and P+1 stays a
  head; Lua `min(a,b)` = `minsd dst=b, src=a`, `max` = `maxsd dst=b, src=a`; document the stale-temporary visibility divergence.
- M5 region IDIV/MOD follow `divmod_leaf` (templates_arith.rs): exit on zero divisor, special-case -1 (`mininteger // -1` must not SIGFPE).
- M6 region code does not call tier-1 `Cx` emitters (`copy_value`, `poll`, `set_const`, `ld_f`) outside the fallback protocol (they clobber V0-V2,V5,V6).
- M7 promotion happens on the back-edge before H's `CountSub` (not after), so `stub(H)`/`region_H` do not pay the block twice.
- M8 V7/V8 (rax/rdx) are allocatable by region code only; emulator poisons them on SDivRem/UDiv/ReloadFrame/CallH/Ret; `BrM W::B1` limited to Eq/Ne or 8-bit flag semantics specified.
- M9 continuation lookups refuse dead pcs (JMP-follower, EXTRAARG, MMBIN followers).
- M10 exit-stub dedup keys include the observed-tag register and the site counter; the observed tag must not live in `rax`.
- M11 task files: T4 needs `lower.rs` (`template`, `exit_at`, `exit_trap` private -> pub(crate)) and `templates_alloc.rs`; T5 and T8 are sequenced (both edit codegen.rs); T6 touches rn-core so it needs the F1 `.text` identity check. Added tasks: T11 docs (UNSAFE.md J4 section for new embedded addresses `prof`/counters/`NestState`, `JmpR` continuations, retired blocks, cached table/upvalue pointers; JIT_DESIGN.md update), T12 jit-diff generator for type-unstable loops and setlocal/upvaluejoin, T13 final gate measurement.

**Advisory:** A1 the projection is optimistic: honest chance of reaching 0.30 numeric is about 30% (0.344 if only the four float loops reach their pessimistic column); mandelbrot at 0.17 means 14.5 cycles/iteration vs LuaJIT 9.4; loops_arith has a divider-throughput floor (one `idiv` per iteration in loop 1, two in loop 3) unless T10c (multiply-high division by constants) is done. A2 code size: each recompile copies tier-1 code, up to 6 copies stay alive: consider region-only blocks reached via a per-nest indirect cell. A3 inline float MOD/IDIV in regions. A4 write payload 0 when writing back Bool. A5 warm-up instrumentation stays when a nest turns Off: re-arm or recompile clean. A6 FORLOOP control variable payload is read regardless of tag: it belongs in the guard set after barrier ops.

**Answers to section 10:** thresholds are fine as starting points; a feature-only field in `Thread` is acceptable under F1 (bump the epoch from `lua_setlocal` too; escaped slots are always re-guarded); keep exact C1; T15 stays opt-in (off) until a fuzz campaign is clean [SUPERSEDED 2026-10-29 by user decision: regions and traces default ON at runtime in `jit` builds (cargo feature stays off), the user runs the long fuzz campaign after J5; see JIT_TRACE.md D5]; decide T6's numeric targets after B5; `r14` can be freed via `ci.kcache` (evaluate only if T5 shows heavy slot residency); no 2-bit nest state.

**Ordering after review:** T1 (with M8) and T2 (with B5, M1-M3, M9) run first. T3 waits for B4/M7 text; T4 waits for B1-B3, M1, M2, M6 and an orchestrator-written reference for one op class end to end (allocator, point state, flush, exit and resume stubs, fallback) — the orchestrator writes that reference (plan s13) as `docs/JIT_TIER15_REF.md` + reference code before T4 starts.

## 13. As built: T2 (region analysis, `crates/rn-jit/src/t15/{mod,effects,analysis,print}.rs`)

Status: implemented and tested (`tests/t15_analysis.rs`, Miri-runnable). The module docs of `t15/mod.rs` list every
decision; this section records where the code differs from sections 3-4 above.

- **Pure inputs.** `FuncInfo` (code, constant tag classes and int values, `max_stack`, escaped slots from the nested
  protos' `instack` upvalues, optional tier-1 `head[]`), `Speculation` (frame tags `obs`, per-pc `prof`, entry/def
  widenings from exit feedback) and `Policy` (`max_region`, `fallback_g` sites, `guard_n_defs`, `EscapedMode`,
  `refuse_named_vararg`). Entry points: `Analysis::new(&info)`, `select_region(hot_backedge_pc, &policy)`,
  `plan(loop, &spec, &policy) -> RegionPlan`, `print::dump`.
- **5.5 generic `for` layout follows the VM** (`TFORCALL A C` reads `A, A+1, A+3`, defines `A+3..=A+2+C`, scratch
  `A+4, A+5`; `TFORLOOP` reads `A+3`), not the stale comments in `lopcodes.h` or section 3.4.
- **Guard set vs write-back set (amendment B5).** `InsnPlan::guard_live_in` is the upward-exposed typed-use set where
  every barrier op and exit *cuts* the exposure (the stub after a barrier re-guards what is read later);
  `RegionPlan::stub_guards(pc)` = those slots with a singleton fact. Write-back is `RegionPlan::writeback_all` (every
  slot, R-EXACT). The entry *speculation* (`EntryFact`) additionally covers slots read only after the first barrier
  (`guarded = false`): the observed tag is assumed, and verified by the stub after that barrier before any use.
- **Memory-tag knowledge (M1).** `InsnPlan::mem_known`: slots whose VM-stack tag byte equals the fact. Set by stub
  guards and by write-through collectables and helper results, reset after every barrier and class-N helper to the
  slots the next stub re-guards (or the guarded helper result).
- **Class N (M2).** `LEN`/`POW`/`POWK` define their static type behind a *mandatory* guard (`InsnPlan::guard_def`) or
  `Any` (`Policy::guard_n_defs = false`). Loads (`GET*`, `SELF`) carry the speculated `guard_def` from `prof`.
- **Escaped slots (M3, A6).** Default `EscapedMode::Reguard`: the fact is kept across barriers and re-checked by the
  stub (the `FORLOOP` control slot is a use, hence in the guard set); `ToAny` is the doc 3.4 wording.
  `RegionPlan::has_setupval` tells T5 whether upvalue hoisting is allowed.
- **Followers (M9).** `Role::{Mmbin, Extra, JmpFollower}`; `RegionPlan::insn(pc)` returns `None` for them, they are
  never resume points (the continuation of `NEWTABLE`/`SETLIST k` is `pc+2`).
- **Region = natural loop by dominance.** A retreating edge whose target does not dominate its source (goto into a
  loop) is recorded in `Analysis::irreducible` and no loop is formed; `select_region` also stops growing before a
  parent loop that is not single-entry, over the size cap or contains `GETVARG`/`ERRNNIL`.
- **Not implemented (left to T4-T6):** constant facts `k(s)`, step *value* speculation (only static
  `ForInfo::step_const`), intrinsic result types, probe CSE facts, `tabhdr`/`node` facts. 5.4 builds the same tables
  (`cfg(rustynail_lua54)` register windows) but is not part of the gate (T-5.5).

## 14. As built: T3 runtime plumbing (no region code yet)

Files: `crates/rn-jit/src/t15/runtime.rs` (new), `rt.rs`, `lower.rs`, `lib.rs` (`pub mod t15`, `set_t15`), `t15/mod.rs` (only
`pub mod runtime;`), `rn-core/src/jit_hooks.rs` (`state_from_thread`, see deviations), tests `tests/t15_runtime.rs`.

**Knobs.** `RUSTYNAIL_JIT_T15=off|on|always` (default off), `RUSTYNAIL_JIT_T15_HOT=n` (default 4096; `always` forces 1),
`RUSTYNAIL_JIT_OPT` parsed into `OptFlags` (nothing consults it yet; unknown items are listed by VERBOSE). Process-wide values
are read once (`T15Config::process`); each state copies them and `rn_jit::set_t15(&mut st, cfg)` overrides them for prototypes compiled
afterwards (tests). VERBOSE adds `[t15] promoted nest ...` lines and a `[jit] t15: promoted/recompiled/refused/off` summary;
`Stats` gained `t15_promoted t15_recompiled t15_refused t15_off`.

**Data.** `T15Data` (boxed, address-stable, interior-mutable cells only): `prof: [Cell<u8>]` (0xFF = never), `nests:
Box<[NestState]>` (fixed at the first tier-1 compile, B4), `cont` (continuation cell), `sig` (version-0 block signature),
`compiles`. `T15Proto` (in `JitProto.t15`): the box, `versions: Vec<Box<VersionData>>` (per-version exit/miss counters, sized by the
number of exit sites; 0 in T3), `retired: Vec<CodeBlock>`, `pcmap`, `cur`. A prototype gets one only if the knob is on and
`scan_nests` finds a loop. `scan_nests` is the T3 placeholder of nest discovery (outermost loops, overlapping loops merged, by
the back-edge rules of `lower`); T7 replaces it with `t15::analysis`.

**Warm-up code (`LowerCfg.t15_warmup`, `Warmup` in runtime.rs).**
- Counter: every jump of a back-edge instruction to its loop head is retargeted (`lower.rs::retarget`, applied to the ops the
  template emitted, so the templates in `templates_*.rs` are untouched) to an out-of-line stub per (nest, head): `ctr -= 1;
  if ctr < 0 { CallH jit_t15_promote(a = nest | ver << 16, b = head); ReloadFrame; if result != 0 { JmpR [cont] } }; jmp head`.
  It runs **before** the head's `CountSub` (M7): the head block is then counted once, by whichever code version continues at it.
  All back-edges of all loops inside a nest share the nest's counter. The counter is compared as a signed 64-bit value after the
  decrement of a zero-extended 32-bit load: it fires at -1 and the helper always re-arms it to a non-negative value.
- `prof[pc]`: after a `GETTABLE GETI GETFIELD GETTABUP GETUPVAL SELF` inside a warm nest, `Mov r0, &prof[pc]; Ld r1, tag(R[A]);
  St8 [r0], r1` (3 micro-ops, not the 2 of the sketch: the result tag register is private to the template). The template's `pcl[pc+1]` is
  remapped to a label in front of the store, so the fast path and all inline jumps record the tag; the out-of-line slow paths of
  `templates_alloc` (`Cold::Slow` rejoins through `pcl[next]` read at `emit_cold` time, B2) rejoin behind it and record nothing.
- Only nests that are still `Warm` get instrumentation. A promoted nest is dropped from the next version's warm-up code
  (A5: no instrumentation is left behind for it). A nest that goes `Off` keeps its (now dead: counter = `i32::MAX`) stub in the running
  version and loses it at the next recompile of the prototype; no recompile is done just for that.
- With `t15_warmup: None` the emission path is the old one (`lower` only gained collection of the back-edge list and the data
  of `Lowered`): `jit-dump-identity.sh` against main is the check.

**Helpers.** `jit_t15_promote(thread, ci, a = nest | ver << 16, b = head) -> 0|1` and `jit_t15_event(thread, ci, a = site | ver << 24,
b = kind << 16 | target_pc) -> 0|1`. 1 = continue at the absolute address in `T15Data::cont` (`CallH` results are 32 bits wide); the
address is the tier-1 label of the pc in the *current* code, taken from `pcmap`, which has 0 for the dead pcs (M9: operand word of
`LOADKX`/`NEWTABLE`/..., the `JMP` a compare consumes, `MMBIN*`; such pcs, pcs out of range and nonexistent versions give 0).
- promote: nest `Promoted` and caller older than the current version -> idempotent: continuation at `head` in the current code;
  caller is current -> 0. `Off` -> counter parked, 0. `Warm` -> recompile with all promoted nests plus this one; success marks the
  nest `Promoted` and returns the continuation of `head`; `Retry` (charge refused, code cap): `ctr = (hot << k) + rand4`, `k <= 10`,
  then the nest is `Off`; `Permanent` (or the 6-versions cap): `Off`.
- event: counters are those of `versions[ver]` (the version the caller belongs to); only the current version can trigger a
  recompile (`Respecialize`; T7 will change the plans first). It returns 0 when the caller's own tier-1 label is right (same version, no
  recompile) and the current code's label otherwise.
- Test hooks: `t15::runtime::{test_event, test_info}` (`#[doc(hidden)]`): the tests call them from native functions to simulate
  exit-stub events with chosen version ids.

**Recompile (`rt::recompile`).** Charge transient buffers and a `VersionData`; lower (`t15::runtime::lower_with_regions`, which for now
emits no region); require `Lowered::sig == version-0 sig` (hard `debug_assert!`, refusal in release); emit; `place_code` (the code
placement of `compile_inner`, factored out unchanged: charge pages, `reopen_tail`, write, commit, refund the unused part); then in one
infallible sequence: push the old block to `retired`, `block`, `resume`, `hdr`, `pcmap`, `versions`, `cur`, `charged`. Every failure
before that point leaves the old version current. `d.deny` is not consulted by recompiles (the nest's own back-off paces them).

**Deviations / decisions to review.**
1. `rn-core/src/jit_hooks.rs` gained `pub unsafe fn state_from_thread(*mut u8) -> State` (feature `jit` only; `State::from_th_ptr` is
   crate-private and the helpers live in rn-jit). Not in the T3 file list; T6 also edits this file (trivial merge).
2. `LowerCfg` gained a field and `Lowered` gained `pc_labels`, `sig`, `backedges`; the four test files and the one `Lowered` literal
   in `emu_diff.rs` that build them were edited (`t15_warmup: None` etc.).
3. 5.4 builds get the same plumbing (nothing in it depends on the FORLOOP layout); region lowering will refuse 5.4 (T-5.5).
4. `tests/jit_j3c_env.rs::code_cap_with_retained_functions_refuses_but_stays_correct` expects fewer recovered compiles when the T15
   env knob is on (every promoted prototype holds a retired block next to the current one, so the same cap holds half as many).
5. Memory cost of retiring is real: a promoted prototype holds up to 6 copies of its tier-1 code (A2 in section 12 suggests region-only
   blocks via an indirect cell; not done here).

## 15. As built: T2b (soundness review of the region analysis + the VM-truth oracle)

An Opus adversarial review of T2 found that the analysis modelled only in-region dataflow while *resume points* can be
entered from outside the region: the `resume[]` table returns every CALL the interpreter or tier 1 runs into `stub(P+1)`,
and OSR enters inner-loop heads through `stub(J)`. `stub(P)` checks only `stub_guards(P)` (the upward-exposed typed uses
with a singleton fact); every other fact at `P`, and `mem_known`, was in-region-derived and could be false there. The
fixes below are in `t15/{effects,analysis,mod}.rs`; `tests/t15_analysis.rs` (`t2b_*`, `check_resume_invariants`) pins them
and `tests/t15_oracle.rs` checks them against the interpreter.

**Resume-point rule (findings 1-3).** At every resume point `P` (region head, inner loop heads, the continuation of every
barrier op, whether it starts a block or not) the state is *clamped* to the stub: slots of `guard_live_in(P)` keep their
fact only if it is a singleton (the stub checks it); non-singleton facts (Bool, Num, any join) and bottom become `Any`;
`mem_known(P)` is intersected with the stub's checked set. Slots outside the guard set keep their fact but it is
*unreliable*: nothing reads it before a later stub re-guards it (typed uses are exposed by construction), so the
peel/speculation machinery is unchanged. Consequences for T4/T5: `stub_guards` only ever contains singleton checks,
a `TEST` on a Bool fact no longer exists across a resume point, a Num-typed accumulator that is `INT|FLT` at an inner
loop head or after a call is boxed (`Any`) there. Implementation: `State::clamp_to_guards`, applied inside
`effects::finish` (barrier continuation, mid-block), and in `Analysis::solve` after the join at inner-loop heads and at
block starts that are barrier continuations (`Solved::lastbar`).

**Bottom (finding 3).** Any typed/table instruction with a bottom operand fact is `Disp::Dead` (MOVE, TEST, SETUPVAL ...,
not only arithmetic). `POW`/`POWK` (class N) define `FLT` unconditionally behind the mandatory guard (the operands are not
typed uses, their facts are unreliable) and are never `Dead`; `LEN` is never `Dead` either. `SELF` in class G and
`TFORPREP` no longer copy operand facts (`Any`). A region whose first instruction is `Dead` under the entry speculation is
refused (`Reject::DeadEntry`). Loops whose latch precedes their header are no longer listed in `RegionPlan::loops` (they
produced a resume point outside the region).

**`TEST` / `TESTSET`.** `Disp::Specialised` on a singleton fact, `Disp::Dual` on a Bool fact (two-way flag test), `Local` on
anything else.

**SETUPVAL (finding 4).** `SETUPVAL` is a may-def of every escaped slot (`debug.upvaluejoin(f, k, g, 1)` can make upvalue
`k` alias a slot of the running frame): their facts become `Any`, `mem_known` is cleared. `GETUPVAL` results remain
`prof`-speculated behind a def guard. Tier 1 had the same hole in `Cx.ty` (block-local register types): `t_setupval`
now drops all type knowledge after the store (`tests/t15_upvaluejoin_tier1.rs`; jit-on printed `2.3e19` instead of
`81.0` before the fix).

**Table ops with `Disp::Local` (finding 5).** Renamed to `Disp::LocalBarrier`: the tier-1 template calls `jit_get` /
`jit_set` on its cold paths (insert, rehash, GC step, finalizers), so it violates B1's "Local = no CallH". The plan marks
such an instruction `InsnPlan::barrier`, its continuation is a resume point, and the transfer function applies the barrier
rules (clamp to `stub(P+1)`'s guard set, `mem_known` reset, `ToAny` drops escaped facts). T4 must emit the class-G protocol
(flush everything, template, stub) for it. `Disp::Local` now means arithmetic / compare / `TEST` templates only.
`GETTABUP` / `SETTABUP` stay `Specialised`: T5 must verify at `stub`/hoist time that the upvalue is a table (no fact exists).

**Other.** `State::join_from` clears `mem` on slots whose joined fact is not a singleton. 5.4: the `FORPREP` skip edge sees
`A+3` as `pre | INT` (the integer path stores the control copy before it decides to skip).

**Item 6, generic `for` nests: not implemented.** `TFORPREP` jumps *into* the `TFORCALL` block, so the loop header (the body
start) does not dominate the latch and the nest is classified `Irreducible`. Making them regions needs a second region entry
(the `TFORCALL` pc) with its own entry facts and a stub for it in T4 (the header is reached only from `TFORLOOP`); the
analysis side is a small change (start the solve at both the `TFORPREP` fall-in and the header, treat `TFORCALL` as a
resume point) but T4's preheader model assumes entry at `H`. Deferred until T4 exists.

### The oracle (`crates/rn-jit/tests/t15_oracle.rs`, feature `t15_oracle`)

`rn-core` feature `t15_oracle` (OFF by default; `scripts/jit-text-identity.sh` shows the default `.text` is unchanged)
installs a `fn(frame, proto, pc, base, maxstack)` callback before every instruction fetch of the interpreter
(`crates/rn-core/src/oracle.rs`, one cfg-gated block in `vm_unsafe.rs`); `rn-jit/t15_oracle` passes it through.
The "after" state of an instruction is the state at the next fetch of the same frame (calls: when the callee returned).
Pass 1 records `prof`/`obs`; pass 2 plans every loop of every prototype under four speculation variants (observed;
`ToAny`; random and wrong `obs`/`prof` with random `fallback_g`; perturbed `obs` without `prof`), then runs a per-frame,
per-plan "inside the region" state machine: **enter** when the interpreter reaches any resume point of the plan and all
`stub_guards` hold; **while inside** assert `actual tag in fact` for `guard_live_in | uses`, `actual == fact` for
`mem_known`, no unreached pc; **leave** at barriers, `LocalBarrier`, `Dead`, exits, failed def guards, operands the
region template cannot take (non-number arithmetic, non-integer bit ops), nested Lua frames, and on leaving `[H, B]`.
Independently, slots in `defs \ uses` are poisoned with a light userdata before each instruction and must be overwritten;
no slot outside `defs | may_defs` may change (skipped for calls, `VARARGPREP`, and when nested frames ran).
Corpus: `tests/vm`, `bench/programs` (tiny `arg[1]`), `tests/jitdiff/gen.lua` seeds (`T15_ORACLE_GEN`, default 40),
`tests/t15_oracle_progs/*.lua` (review repros). Each program runs in a child process (the test binary re-executed).
Run: `systemd-run --user --scope -q -p MemoryMax=8G -p MemorySwapMax=0 timeout 1700 cargo test -p rn-jit --features
t15_oracle --test t15_oracle -- --nocapture </dev/null` (`T15_ORACLE_ONLY=<substr>`, `T15_ORACLE_GEN=<n>`,
`T15_ORACLE_V=1`). On the unfixed T2 analysis it failed on all four repro programs (and 106 of 266 corpus programs).

## 16. Reference decisions (T4a: region code generator, integer slice) and briefing for T4b/T5/T6/T7/T8

Files: `t15/codegen.rs` (acts, emission, driver), `t15/alloc.rs` (register choice, register-resident dataflow; pure, Miri),
`t15/stubs.rs` (flush, edges/pads, budget exits, `stub(P)`), `lower.rs` (`lower_ex` + `RegionHook`), `templates_alloc.rs` (B2),
`t15/runtime.rs` (`FuncMeta`, `lower_with_regions`, frame tags `obs` saved in `NestState`), `rt.rs` (builds `FuncMeta`).
Tests: `tests/t15_core.rs` (twin programs x allocation modes x budgets), `tests/t15_core_emu.rs` (emulator + native vs tier 1
on synthetic frames, Miri-runnable). Region lowering is skipped for 5.4 builds (T-5.5) and when the nest has no typed instruction.

### 16.1 Decisions where the text above was wrong, silent or changed

1. **One `Program`, a hook in `lower`.** `lower_ex(code, cfg, Some(hook))` runs the hook after the tier-1 code of every pc and
   before the cold/exit sweeps, with the open `Cx`. The hook (a) builds the analysis there because `FuncInfo::extra_heads` is
   tier 1's `head[]` (only known inside `lower`), (b) emits the regions, (c) returns `heads` (loop head, stub label) and
   `overrides` (resume pcs inside the region). `lower_ex` then redirects **every** tier-1 jump to `pcl[H]` (entries and the tier-1
   copy's back-edges) to `stub(H)`, inserts `Jmp stub(H)` in front of the fall-through into `H`, and replaces the resume-table entry
   and the `pc_labels` entry (continuations of `jit_t15_promote`/`event`) of `H` and of every region pc with a resume entry
   (inner loop heads, pcs after CALL) by their stubs. Region code's own jumps to `pcl[...]` keep the tier-1 labels (they were added
   after the redirect). Tier-1 bytes without a hook are unchanged (checked by the dump-identity scripts).
2. **Register-resident set `RR(pc)` instead of `PointState{loc,dirty,memtag}`.** Each Lua register is *register-resident*
   (value in its machine register, VM slot stale) or *slot-resident* (slot exact). `RR(pc)` is a pure function of the code:
   forward must-dataflow (`alloc::solve_rr`, join = intersection, entries = `stub(P)` loading `load(P)` = assigned slots in
   `stub_guards(P)` with an INT fact, fallback successors enter through their stub). Consequences: every exit/edge/flush is
   generated from `RR` alone (no separate maps, so no map can be forgotten); a slot that is register-resident on one
   predecessor of a join but not on another is flushed on the edge (`Pad`). **No dirty bit and no `memtag` in v1**: everything in
   `RR` is written back (tag byte + payload) at every flush; stores are redundant for clean values but never wrong.
   This is the cheapest correct reading of "flush must not skip the tag store on stale memtag" (M1). Dirty tracking and tag
   elision (`InsnPlan::mem_known` is the ready-made input) are pure optimisations of `flush_stores`.
3. **One register per slot for the whole region, chosen by loop-depth weight** (`alloc::choose`), not interval linear scan. Slots
   without a register are slot-resident everywhere. Interval splitting is a later optimisation; the interface (`Assign`) would not change.
   `V0..V4` are the only assignable registers; `V5` budget; `V6 V7 V8` (`rcx rax rdx`) scratch of region code. `MODK`/`IDIVK` by a
   constant integer other than 0/-1 are typed (`Act::DivK`): `SDivRem` needs operands in `V0..V6` and clobbers `rax/rdx`, so a region
   that contains one assigns only `V0..V3` and uses `V4` (divisor/remainder) and `V6` (dividend/quotient) as scratch; floor correction
   follows `divmod_leaf`. Register divisors, `//`/`%` by 0 and -1 stay fallbacks (M5).
4. **Fallback = successor stubs.** The tier-1 template runs with `pcl[q]` of every pc `q` in the region replaced by `stub(q)`. Any
   control instruction can therefore be a fallback (compares on non-integers, FORPREP, TFORLOOP ...): the template's own jumps
   land on a stub that re-guards and reloads. After the template an explicit `Jmp stub(next)` is appended (the template "falls off
   its end" into the next instruction in tier 1; `follow(pc)` skips MMBIN/EXTRAARG followers). Which stubs the template refers to
   is read back from the emitted ops and the cold list; the register dataflow assumed the superset `succs + {pc+1, pc+2}`
   (`fall_targets`), and `emit_fall` asserts it. A stub **always guards everything in `stub_guards(P)`** (also after non-barrier
   fallbacks): simpler, and it makes transfer-function mistakes about fallback results harmless wherever the value is used later.
5. **B2 fixed as `HCall::rejoin: Option<L>`**, captured only while `Cx::capture_rejoin` is set (region fallbacks). Plain tier 1 and
   the T3 warm-up code still read `pcl[next]` late, so their bytes and recorded profiles are unchanged.
6. **B3:** `Cx::pcl_post[pc]` (label behind the `CountSub`; equal to `pcl` for heads without a count and without hook) exists for
   region exits taken *after* the region's own `Sub r11` at a head. **The INT slice has no such exit** (typed integer templates
   never exit; the budget exit is tier 1's own `exit_at`; stub failures happen before the count), so nothing uses it yet.
   T4b/T5 must use it for the first guard that can fail in the first instruction of a block.
7. **Budget register.** `Sub r11, n; (cmp r11,0); jl cexit` at the region label of each tier-1 head with a count (same heads and
   counts as tier 1, `RegionEnv`). `r11` is loaded by every stub and stored to `[r15]` by every flush, edge-leaving pad and exit.
   Valid because the budget is non-negative whenever compiled code runs (the 64-bit compare then equals the 32-bit one).
8. **Back-edge poll = 5 ops in the edge pad** (`BrM TRAP; Mov V7, INTERRUPT; BrM byte`); the trap exit flushes `RR` and the budget and
   jumps to tier 1's `exit_trap(target)`. `-cntreg` (keep `[r15]` counting) is not implemented.
9. **Profitability gate.** A nest gets no region if it has no typed instruction or if its loop-depth-weighted typed work
   (`10^depth` per instruction) is less than twice the weighted fallback work (each fallback costs a flush, a reload and the stub
   guards). Without it fannkuch (every table access a fallback) ran 2x slower than tier 1 and loops_arith loop 3 1.5x slower.
   `RUSTYNAIL_JIT_OPT=+force` bypasses the gate (the tests and the official suite run with it). T4b/T5 should revisit the factor.
10. **`obs`** (frame tags at promotion) is stored per nest in `NestState::obs` by `promote`, so later re-lowerings (other nests,
    respecialisation) rebuild the same plan. `Speculation::{entry_widen,def_widen}` are empty until T7.
11. Not implemented from the T4 row: `+exitstorm`, `-cntreg`, XMM, `mem_known` tag elision, dirty bits, peel counting,
    the event helper from exit stubs (region code emits no `CallH` at all, except the debug-assertion/`+verify` failure report;
    the emulator can run all of it). `+verify` was added by T12 (section 17).

### 16.2 How region code is shaped (read this before touching `codegen.rs`)

* `Act` per pc (`classify`): integer templates, memory-pure definitions, or `Fall`. An integer template is chosen only when
  the **facts** at the pc say its operands are `INT`; it then reads registers or slot payloads without any tag check. This is sound
  because every typed use is in `guard_live_in` and so is guarded by the stub that most recently dominated it (or defined by
  region code with a known result type).
* `emit_all`: for each reached pc: `rr := rr_in[pc]`, bind `reg_l[pc]`, block count at tier-1 heads, the act. **Every outgoing edge
  goes through `Rg::edge(rr, target, poll)`**, which returns the region label itself (nothing to do) or a pad that flushes
  slots that leave `RR`, polls, or leaves the region for tier 1 (`Leave`: flush all, `St budget`, `Jmp pcl[target]`).
* Reading an integer slot: `src_reg` (its register, loading it first if it is not register-resident, which adds it to `RR`) or a
  scratch register loaded from the slot payload. Defining one: `commit_int` (register-resident: add to `RR`; else store tag+payload).
  `RR` effects of an act are declared once in `Rg::effects` and used by both the dataflow and the emission; `emit_all`
  `debug_assert`s that they agree.
* `emit_fall`: flush `RR`, store the budget, run the tier-1 template, append `Jmp stub(next)`. Stubs are emitted at the end
  (`finish`) for the pcs that are referenced.

### 16.3 How to extend (briefing for T4b, T5, T6, T7, T8)

**Adding an op class** (e.g. float arithmetic). (1) Add an `Act` variant and its `classify` rule on `plan.fact(pc, s)`. (2) Declare its
`(ruse, rdef, mdef)` in `Rg::effects`. (3) Emit it in `emit_act`; end straight-line acts with `end_straight(pc)`, branches with
`edge()` for each successor. (4) If it can *fail* (guard exit), the exit must be a pad/stub that flushes `RR` of the state *at the
failing point*, stores the budget and jumps to a tier-1 label: `pcl[pc]` before the region counted the block, **`pcl_post[pc]` after**
(B3). (5) Make sure the new value kind is covered by `RR` (see below), or is slot-resident. (6) Extend `tests/t15_core.rs` programs and
`t15_core_emu.rs` programs; run the mutation checks of 16.5.

**XMM values (T4b).** Add `Loc` classes to `Assign`/`RR`: a second set `RX` with the same dataflow (it is the same lattice, one more
`SlotSet` in `Node`/`solve_rr`), `load` sets from `stub_guards` with a FLT fact, flush stores `StF` + tag `FLT` (the tag store is
skipped only if you track it). **Every fallback clobbers all XMM (B1)**, so `RX` is flushed and emptied exactly like `RR`. Scratch is
`X0/X1`; `FNeg/FAbs` also clobber `V7`. Constants come from `K` as memory operands. NaN/-0.0 rules: `FBrX` only, never negated
condition codes (T-FLOAT). The profitability gate then changes (floats stop being fallbacks).

**Table ops (T5).** Typed `GETTABLE/SETTABLE` keep tables write-through (R-G1): register copies of table pointers and `ARRAY/ASIZE`
die at every barrier op (any `Fall` of class G/Call) and are reloaded by the stub (put them into `load()`, guarded by the TABLE
fact). Misses and type exits need exits with feedback (T7). Inline stores never insert (`jit_set` is a class G fallback).
5.4 needs `alimit_touch`. Table helpers that reallocate move the array: never cache a pointer across a `Fall`.

**Calls/intrinsics (T6).** CALL stays a `Fall`; the continuation is already `stub(pc+1)` for both the inline-return (native callee)
and the resume-table return (Lua callee). Intrinsics must emit `CountSub{1}` after their guards exactly like `tpl_call` (M4).

**Policy/events (T7).** Region code has no counters and no `jit_t15_event` calls yet. Add them in the *pads* (they already carry the
exact `RR`), keyed by a per-version site id (`VersionData`, B4). Do not store addresses of per-version data in shared state.

**Verification (T8).** `+flushall` already exists (stores the whole `RR` and the budget before every instruction). `+verify` can run the
tier-1 IR of a block in the emulator between two `Bind`s of region labels: `reg_l[pc]` are the block-boundary labels.

### 16.4 Invariant review checklist (per change to region code)

1. Every observation point (helper call, exit, leaving native code) is preceded by `flush_stores(RR)` and `store_budget`; `RR` is empty
   afterwards. (`emit_fall`, `Leave` pads, trap and budget exits.)
2. Every register that a stub loads is guarded by that same stub (`load(P) ⊆ stub_guards(P)`); `solve_rr` is seeded with `load(P)` for
   *all* stub-entered pcs, and `emit_fall` asserts that the template references only registered stubs.
3. A template never exits from the middle of an instruction after writing a register or a slot (E3); exits go to tier-1 labels whose
   count state matches (pre-count `pcl[P]`, post-count `pcl_post[P]`).
4. Block counts: region heads == tier 1 heads; `BlockSig` equality is asserted by `recompile`.
5. Nothing in region code calls `Cx::copy_value/poll/set_const/ld_f` (they clobber `V0-V2,V5,V6`); the tier-1 `template()` is called only
   through `emit_fall` with `capture_rejoin` set and the labels remapped.
6. Scratch discipline: `V6 V7 V8` only; `V5` is the budget; `V0..V4` only through `Assign`.
7. After any edit: `cargo test -p rn-jit --features jit` with and without `RUSTYNAIL_JIT_T15=always`, `t15_core` under
   `RUSTYNAIL_GC_STRESS=1|2` and `RUSTYNAIL_JIT_STRESS=stackmove`, jit-diff with `RUSTYNAIL_JIT_T15=always`, and the mutation checks.

### 16.5 Test matrix, mutation checks and measurements (as run for T4a)

`t15_core`: 35 fixed programs + type-change matrix (N in {1,2,3,100}) + 60 random integer programs, each against the jit-off twin under
`default`, `-ra`, `+randra=1,2,3,7`, `+flushall`; programs also under budgets 3/17/200 for `default`, `-ra`, `+randra=1`. Addresses are normalised.
`t15_core_emu`: 6 loops x 5 allocation modes x 150 random frames, emulator and native vs tier 1 on all frame slots, budget, `savedpc`,
`trap`. Gates run green: rn-jit tests (5.5 and 5.4 cfg) with and without `T15=always`, `t15_core` under `RUSTYNAIL_GC_STRESS=1|2` and
`RUSTYNAIL_JIT_STRESS=stackmove`, workspace tests with jit, clippy 5.5/5.4/jit, the T2b oracle, text and dump identity, the official
suite with `T15=always` (with and without `+force`), jit-diff 1500 seeds (`JITDIFF_ON=1`) and `hook=all` 400 seeds with `T15=always,+force`.
**Correction (T12):** the `hook=all` runs compared the interpreter with itself - the prelude hook stayed installed, and compiled code is
never entered while a hook is installed - so they never exercised region code. See JIT_TESTING.md ("Hook-semantics variant"); T12
changed the generator (hook alternates with hook-free snippets, mid-run hook templates) and added the `JITDIFF_ENTERS=1` counter
assertion that fails when a mode never enters compiled code.
Measured (release, cpu 4, min of 7, `RUSTYNAIL_JIT=on` T15 off -> always, seconds): loops_arith 0.425 -> 0.388 (interpreter 0.904);
loop 1 0.327 -> 0.313 (float ops are fallbacks), loop 2 0.103 -> 0.085, loop 3 0.067 -> 0.066 (an `idiv` per iteration), fannkuch 0.364 -> 0.366
(no region: gate). The integer slice alone does not reach loops_arith <= 0.40 of the reference; it needs T4b (floats in XMM) and the
store reductions of 16.6.
Mutation checks (each must fail the suite; all seven were caught): skip the fallback flush, skip the edge flush of leaving
slots, drop the stub guards, drop the trap-exit flush, drop the budget-exit flush, skip `store_budget`, skip the budget reload in stubs.

### 16.6 Pitfalls found or avoided

* `oomd`/memory: tests only use tiny sizes. Tests that print `tostring` of functions must normalise `0x...` (they differ per state).
* A stub that loads a register without guarding it would write garbage back at the next flush (tag INT over a non-integer). The
  register dataflow must therefore be seeded by *every* possible stub entry; adding a new way into region code (a new resume
  entry, a new fallback shape) means adding it to `stub_entered`.
* Dead temporaries that are register-resident at a back-edge but not at the loop head are stored on the edge every iteration
  (R-EXACT forbids pruning). This is why loops with many dead temporaries gain less than the model: tag elision (`mem_known`) halves it.
* `Op::Br` compares 64 bits, the budget is a 32-bit word: the compare is only valid because the register holds a zero-extended
  non-negative value on entry (stub load `Ld B4`).
* The tier-1 template for a `Stub`-kind pc must not be called; `emit_fall` jumps to its exit stub instead.
* Do not edit the generator while a mutation run is in flight (the mutation script patches and restores the same files).

## 17. As built: T12 (T4a hardening before the float/XMM regions)

Findings of the Opus review of T4a: no unsound code, but tests that could not catch several classes of bug. T12 changes tests, the
differential harness and three robustness points; default code is unchanged (checked by the identity scripts).

1. **Refusals instead of panics.** `Rg::refusal` (set by `Rg::refuse`) replaces the `assert!` in `emit_fall` ("template refers to the
   stub of pc ...") and the `expect`s of `stubs.rs` (register of a register-resident slot, singleton guard tag, missing `rr_in`).
   `emit_region` remembers the program length, the cold list and the lazily created exit sequences before emitting; on a refusal it
   truncates them and returns `None`: the nest stays in tier 1 and `RUSTYNAIL_JIT_VERBOSE` prints `[t15] region [h, e] refused by the
   code generator (why)`. The graceful mutant `bad_fall_targets` (below) exercises this path in the test suite.
   `debug_assert!`s of the dataflow (`emit_all`) remain.
2. **Budget invariant check.** Stubs end with `BrM Lt [r15] 0 -> jit_t15_verify_fail` when the crate is built with `debug_assertions`
   or the region is compiled with `+verify` (release bytes without `+verify` are unchanged). It checks the invariant of 16.1 item 7
   (the budget word is non-negative whenever compiled code runs, which makes the 64-bit compare of `r11` equal to tier 1's 32-bit
   one). The word, not `r11`, is tested: `r11` is zero-extended and could never look negative. Never fired in any test.
3. **`+verify`** (`OptFlags::verify`; `Rg::tier1_label`, `Rg::emit_checks`). `lower_ex(.., record_ty)` records, for every pc, the
   slots whose `Cx::ty` is `Int`/`Flt` when tier 1 emits that pc (`RegionEnv::ty_at`). Every jump of region code into tier-1 code
   (stub guard failure, leaving pads, the continuation of a fallback outside the region) goes to a check block instead of
   `pcl[pc]`: `BrM Ne tag` for each known slot, then `Jmp pcl[pc]`; a mismatch calls `jit_t15_verify_fail`, which prints the pc and the
   slot and aborts the process. Exits to the interpreter (`exit_at`/`exit_trap`) need no check. Run with `+flushall`/`+randra`
   (`t15_core` OPTS, `t15_core_emu`, jit-diff modes `t15verify`, `t15verifyrand`). No violation was found.
4. **Emulator helper model** (`emu::Emu`, `emu::Helpers`). `CallH` calls a `Helpers` model (all virtual and `X` registers are
   poisoned afterwards; the result is `V0`); a model may return a new frame base (the Lua stack moved): the emulator makes the old base
   unusable (a poisoned address) until the next `ReloadFrame`, so a missing reload crashes. `Emu::run` can be resumed behind a stop
   label and exposes the registers. `run_until` is unchanged for existing users.
5. **`t15_core_emu::regions_with_helper_calls_match_tier1_at_every_pc_and_exit`** (`calls: Some`): six loops with `CALL`, `LEN`,
   `GETTABLE`, `SETTABLE`, `POW`, `CONCAT`, `NEWTABLE` fallbacks. The seeded model (`Model`) answers each helper call with done / exit
   (`H_EXIT`) / error, writes the result with the type tier 1 assumes, and (for `CALL` and the allocating helpers) overwrites a random
   slot with a random-typed value and moves the stack. `Lowered::points` (`RegionPoint`: tier-1 label, region label and the
   register-resident slots of every region pc) lets the test trace both programs and compare, at **every arrival at a region pc**
   (region label with the registers overlaid on the frame, or tier-1 label): the whole frame, the budget (`r11` or `[r15]`), the
   `CallInfo` and the thread; plus the final state and exit code. About 25000 helper calls, 70000 region-label and 130000 tier-1-label
   events per run; no divergence. Native execution is not compared here (helpers are models).
6. **Instruction-count twin** (`t15_core::instruction_counts_are_identical_with_and_without_regions`): `Stats::insns` with T15 off and
   with `always,+force` (opts default, `-ra`, `+randra=3`, `+flushall`, `+verify`) must be equal for every program of the corpus
   (fixed, mid-run mutation programs, 60 random) under count budgets none / 5 / 37. Equal everywhere.
7. **Mutation testing** (cargo feature `t15_mutants`, off by default; `src/t15/mutants.rs`, `RUSTYNAIL_JIT_MUT=name`; without the
   feature `mutants::active` is a `const false`). `cargo test -p rn-jit --features t15_mutants --test t15_core every_mutant` re-runs
   the binary (all of t15_core and the emulator tests of `t15_core_emu.rs`, included as a module) once per mutant in a child process
   and requires failure (a crash or a 60 s hang counts); the unmutated child and the graceful mutant must pass. Mutants and who catches
   them: `count_after_fall` (block count +1 at heads after a fallback; insns twin), `no_budget_reload_after_call` (insns twin),
   `drop_guard_first/last/after_fall` (emulator tests, `mid_run_mutation_programs`), `flush_skip_last`, `flush_skip_tag`,
   `leave_no_budget`, `pad_no_irq_poll`, `fall_no_budget`, `stub_no_load_last` (emulator tests; the last one hangs natively), and
   the graceful `bad_fall_targets` (generator refuses every nest with a fallback).
8. **Harness** (docs/JIT_TESTING.md): generator templates for mid-run hooks (loop / callee / finalizer), `setlocal` type changes from
   callees and hook functions, `upvaluejoin`/`setupvalue` aliasing, coroutines in hot loops, type-unstable accumulators (K = 1, 7,
   1000), variable-typed loop bounds and overflow loops; the hook variants alternate hook and hook-free snippets; the new modes
   `t15*` and the `JITDIFF_ENTERS=1` counter assertion. The earlier `hook=all` results did not exercise compiled code (see 16.5).

## 18. As built: T4b (float slots in XMM registers)

Files: `t15/codegen.rs` (new acts, float operand helpers, `Raw`/`eff`), `t15/alloc.rs` (`Assign::xreg`, `choose_all`), `t15/stubs.rs`
(class-aware flush and stub loads), `lower.rs` (`RegionPoint::rx`), `t15/mutants.rs`. Tests: `tests/t15_core.rs` (float twin programs,
random float programs, 7 new mutants), `tests/t15_core_emu.rs` (float programs, entry-speculation variants), `tests/jitdiff/gen.lua` (four
float templates). No tier-1 byte changed, no `unsafe` added (UNSAFE.md untouched); the identity scripts pass.

### 18.1 Design as built

* **One register class per slot.** `Assign` has `reg` (V0..V4) and `xreg` (X2..X15, `alloc::NALLOC_X` = 14). `choose_all(wi, wf, ..)` gives a
  slot the class with the larger operand weight (ties: integer); the other class operates on that slot through memory. The register-resident
  set `RR` stays one `SlotSet`: a slot in `RR` holds a value of *its* class (same invariant as T4a: it entered `RR` through an act that
  required that fact, or through a stub that guarded the tag byte). A def of the other class is a memory def (`Rg::eff` puts it into `mdef`,
  the emitters call `rr.remove`). `solve_rr`, `edge`/pads and `emit_all` needed no structural change; `Rg::raw_effects` has the class-free
  effects of an act (used for the weights), `Rg::eff` the ones under the current assignment (used for the dataflow and the debug assertion).
* **Flush/stub.** `flush_stores`: integer slot = tag `INT` + `mov`, float slot = tag `FLT` + `movsd` (`StF`: the bits are copied, so -0.0 and
  NaN payloads survive). Stubs load float slots with `LdF` after guarding the `FLT` tag byte (`load_set`: the guard tag matches the class of the
  slot's register). Every fallback still flushes everything (B1): XMM registers are never live across a `CallH` or a tier-1 template.
* **Acts** (`classify_float`, chosen only on singleton facts, i.e. the same typing discipline as the integer acts, no guards of their own):
  `LoadFc` (`LOADF`, float `LOADK`), `MoveF`, `FBin` (`ADD SUB MUL DIV`, `ADDI`, `ADDK SUBK MULK DIVK`; operands: `Flt` slot, `Int` slot
  converted with `cvtsi2sd`, float constant as a memory operand from `K`, integer constant/immediate as a materialised double; at least one
  float operand, or `DIV` of any numbers), `FNeg`, `FCmp` (`EQ LT LE`, `EQI LTI LEI GTI GEI`, `EQK`: float vs float, or float vs a small
  integer immediate / constant with |k| <= 2^53; `FBrX`, never a negated condition), `FForLoop`.
* **Operand order is never changed** (which NaN wins depends on it): if the destination register is also the second operand, the operation is
  done in `X0` and moved. The float `FORLOOP` mirrors the integer one: `idx + step` is computed in `X0`; the slot keeps its old value on
  the exit path (the interpreter does not store it) and is updated only when the loop continues.
* **Float `FORLOOP` sign test** is exact: `0 < step` is decided on the bits of the step (`0 < bits <= 0x7FF0000000000000` as signed
  integers), so a zero step (reachable only through `debug.setlocal`) or a NaN step behaves as `luai_numlt(0, step)` does.
* **FORPREP stays a fallback** (it runs once per loop entry; its successor stub loads the float registers).
* **Profitability gate.** Unchanged (depth-weighted typed work >= 2x fallback work; `+force` bypasses it). Float acts count as typed work,
  so float loops now pass it (loops_arith loop 1 had a fallback at its float statement before).

### 18.2 Deviations from sections 3-4 / 16

1. **No dual-path code for `INT|FLT` facts.** Section 3.2 proposed dual-path templates for "Num" slots. They are not generated: an operand
   that is not a singleton keeps the fallback (tier-1 template between a flush and the stub). The analysis clamps such facts to `Any` at
   resume points anyway (T2b), and the entry speculation (peel, `spec(H)`) turns loop-carried `INT -> FLT` transitions into a singleton at the
   head, which is what the benchmarks need. A guard-and-stub alternative for a changing accumulator needs T7's exit feedback.
2. **Mixed int/float compares, float `//` and `%`, `^` stay fallbacks.** Tier 1 itself exits to the interpreter on float `//` and `%` and on
   mixed compares outside +-2^53, so loops with them do not get hot in compiled code anyway (the tests that need a region leave them out).
3. **Tag stores are not elided** (as in 16.1.2): float slots are stored with their tag at every flush.
4. **Emulator tests needed the constants array.** Region code reads float constants from `K` (`[r14 + 16k + 8]`); the synthetic frames of
   `t15_core_emu.rs` had an all-zero `K` (integer constants are immediates in tier 1). `Mem::set_k` fills it from the compiled constants.
   The entry speculation of those tests (`obs`) used to be "every slot is an integer"; `obs_for(variant, ..)` adds all-float, odd-slots-float
   and high-slots-float variants for the `fl_*` programs (otherwise float loop heads always failed their stub guards).

### 18.3 Tests and gates

* `t15_core`: 11 float twin programs (arithmetic with `K`/`I` forms and mixed int/float, NaN/inf/-0.0, compares of every form incl. NaN,
  `EQK` beyond 2^53, `for` bounds cases with accumulating steps / NaN / inf / 2^53 / overflow / string bounds, int<->float conversions near
  2^53, `-0.0` through unary minus, string-to-number coercion at iteration N, float `%` `//` `^` fallbacks, type flips, a count hook in a
  float loop) and 60 random float programs; all under the nine allocation/verify modes and budgets 3/17/200, plus the instruction-count twin
  over the whole corpus. Floats are compared by their exact bit pattern (`H`).
* `t15_core_emu`: 11 `fl_*` programs x 7 options x 4 entry speculations x chaos, compared with tier 1 at **every region pc** (the
  xmm registers are overlaid on the frame via `RegionPoint::rx`); native-vs-emulator-vs-tier-1 frame comparisons with float and special-value
  inputs (`float_programs_generate_xmm_region_code` checks that XMM code is really produced).
* Mutants (feature `t15_mutants`; all detected): `flt_flush_int_tag`, `fcmp_negated`, `i2f_missing`, `fneg_missing`,
  `fforloop_always_pos`, `fforloop_clobber_idx`, `fbin_alias_commute` (plus the eleven of section 17).
* jit-diff: templates for NaN/inf/-0.0/2^53 float loops, float `for` bounds (accumulating step, NaN/inf bounds, overflow, string bounds),
  int<->float conversion near 2^53, and a float operand that turns into a string/table/nil/integer at iteration K.

### 18.4 Results

Measured (release `--features jit`, `RUSTYNAIL_JIT=on`, cpu 4, min of 7, seconds, T15 off -> always; one process at a time):
loops_arith 0.416 -> **0.160** (T4a: 0.388; interpreter 0.97); its loops separately: loop 1 (int + float accumulator `f + i * 0.5`)
0.317 -> 0.086, loop 2 0.037 -> 0.023, loop 3 (while, `%`) 0.057 -> 0.059 (divider floor, unchanged); mandelbrot 0.546 -> **0.115**;
nbody 0.497 -> 0.500 and spectral_norm 0.545 -> 0.544 (table accesses are fallbacks: the gate keeps them in tier 1, region work needs T5).
Gates run green: fmt; clippy -D warnings (5.5 default, workspace+jit, 5.4+jit, t15_mutants, t15_oracle); rn-jit tests 5.5 (also
`T15=always`) and 5.4; workspace tests with jit; T2b oracle; `jit-text-identity.sh main` and `jit-dump-identity.sh HEAD~1` identical;
jit-diff seeds 1..1100 (`JITDIFF_ON=1`, `JITDIFF_ENTERS=1`, modes off/always/on2/on10/t15/t15flush/t15rand/t15verify/t15verifyrand)
0 diffs (14.9M entries, 18471 region recompiles); official suite with `T15=always` `+force` and `+force,+verify,+flushall`; all
nineteen mutants detected.

## 19. As built: T5 (inline table accesses in regions)

Files: `t15/codegen/tables.rs` (new; classification, emission, slow paths), `t15/codegen.rs` (four `Act` variants, effects, the
register limit and the stub entries of table accesses, the profitability gate), `t15/stubs.rs` (`finish` emits the slow paths),
`t15/runtime.rs` (`-tables`), `t15/mutants.rs` (seven mutants). No tier-1 byte changed, no `unsafe` added to the library (UNSAFE.md has a
J4-T5 entry saying so); the identity scripts pass. Tests: `tests/t15_core.rs` (table twin programs, profiled twins, cross-class and XMM
exhaustion programs, mutants), `tests/t15_core_emu.rs` (synthetic tables in the emulator), `tests/jitdiff/gen.lua` (seven templates),
`scripts/jit-diff.sh` (modes `t15prof`, `t15profrand`).

### 19.1 Design as built

* **Shape.** `GETI GETTABLE GETFIELD` -> `Act::GetArr/GetFld`; `SETI SETTABLE SETFIELD` -> `Act::SetArr/SetFld` (`classify_table`). An
  access is inlined when the table slot's fact *contains* `TABLE` (`tp` = the fact is exactly `TABLE`: no check; otherwise the tag byte
  is compared at run time) and, for `GETTABLE/SETTABLE`, the key slot's fact contains `INT` (`TKey::Reg` = exactly `INT`, read from its
  register; `TKey::RegU` = checked at run time, read from memory or from the register of a register-resident slot, which holds an
  integer by the class invariant of `RR`). A singleton fact that excludes the class (a string key, a number as table) keeps the
  fallback. `Disp::Dead` accesses stay fallbacks. The run-time check exists because `prof[pc]`/`obs` are missing for loads executed
  before the nest was instrumented (an OSR compile inside the first iteration of an outer loop: matmul's `a[i]`).
* **Everything uncommon is one slow path per instruction.** All misses jump to a label whose code, emitted out of line by `finish`
  (`emit_slow_paths`), is exactly the fallback protocol of 16 (`emit_fall`): flush the register-resident set `rr` that held at the jump, store
  the budget, run the **tier-1 template of the same instruction** (so nil results, `__index`/`__newindex`, insertion, rehash, GC barrier
  work, errors and their messages stay in one place) and continue in `stub(pc+1)`. The fast path is pure until its last check (E3):
  every operand register is loaded *before* the slow label is created (so all jumps to it agree on `rr`), nothing is written before the last
  jump. Because the slow path re-executes the instruction, a speculated result type needs no exit stub: a mismatching element tag
  takes the slow path, whose `stub(pc+1)` guard on the result slot then fails to tier 1 at `pc+1`.
* **Array part.** `GETTABLE`: `S1 = key-1` (`lea`), `S1 < 0 -> slow`, `S0 = table`, `S1 >= asize -> slow`, `S2 = array`; the element tag is
  compared with the speculated one (`Act::GetArr::res`: `Int`, `Flt`, `Mem(Some(tag))` from `InsnPlan::guard_def`, else `Mem(None)` = "not
  nil", the only check) with an indexed `cmp byte [S2 + S1 + 4]`; the payload is `[S2 + (-S1)*8 - 8]` into the slot's integer or double
  register (`LdI/LdFi`) or copied with its tag to the slot (`Mem`). `GETI` has constant offsets (`asize <= u -> slow`). Stores: bounds as
  above; a **non-empty slot, or a table without metatable** (tier 1's rule: otherwise `__newindex`); barrier check (below); tag byte, then
  payload (`StI/StFi`, or a copy through `S3`).
* **Hash part.** The chain walk of tier 1's `node_lookup` on the region scratch registers: main position from `lsizenode` and the compile-time
  hash of the constant (`1 << lsz` via `ShlV` with the count in `rcx`), `key tag == SHRSTR && key == K[kx]` per node (the key pointer comes from
  `K`, never embedded: G1), `next` sign-extended as in tier 1; an absent key takes the slow path. `GETFIELD` then checks the value tag as for
  arrays; `SETFIELD` requires an **existing non-nil value** (no `__newindex`, no insertion, no `invalidate_tm_cache`: tier 1's
  `pset_existing_shortstr_slot`). There is no main-position speculation or probe CSE (the doc 5.1 `fcache` pass): every access walks.
* **GC barrier (review point).** Exactly tier 1's `barrier_exit`, evaluated before the store: `table.marked & BLACKBIT != 0` and (for a
  value that may be collectable) `value.tt & BIT_ISCOLLECTABLE != 0` and `value.marked & WHITEBITS != 0` -> slow path (the tier-1
  template exits to the helper, which stores with `barrier_back`). No barrier code exists in region code. The test is skipped only for
  values of a statically non-collectable kind: a slot with a register (`INT`/`FLT`), a slot whose fact is `nil`/`false`/`true`/`INT`/`FLT`/
  light C function/light userdata, a constant that is not collectable. A collectable *fact* is not trusted: it still tests the collectable
  bit at run time, so the dereference of the payload never rests on a fact (`Coll::Maybe`); only constants are `Coll::Yes`. The value is
  read from memory (collectables are write-through, R-G1; a register-resident `INT`/`FLT` slot is `spill`ed or read from its register).
  Mutant `tbl_no_barrier` removes the test; `tb_gc_barrier_gen/inc`, `tb_weak`, `tb_finalizer_store` and the jit-diff GC template catch it.
* **Registers.** Scratch: `S0 rax`, `S1 rdx`, `S2 rcx`, `S3 = V4 (r10)`. A region with an inline table access (or a `DivK`) assigns only
  `V0..V3`. The profitability gate counts an access with an unproven table fact as a fallback.

### 19.2 Invariants (the review checklist)

1. **G1 / R-DERIVED.** The table pointer, the array base, the index and the node live in `S0..S3` from their load to the end of the instruction. They
   are loaded from the table's VM stack slot (write-through, so the object is alive); nothing is cached across an instruction, a barrier op or a
   helper. There is no GC point inside the fast path (no helper call, no allocation), so no object can die or move while a derived pointer is held.
   Array and node memory are never resized by the fast path (stores never insert). Re-derivation after anything that can move the Lua stack or run
   a helper is trivial: it is done at every access. `rbx` is only used through `fr(slot)` as everywhere in region code.
2. **A table slot is never register-resident.** A table slot found in `RR` refuses the region (graceful `Rg::refuse`); a register-resident key
   with a float register refuses likewise.
3. **E3.** All checks before the first store; the register-resident set at the slow label is the one after all operand loads; the dataflow
   (`solve_rr`) enters `follow(pc)` additionally through its stub (`stub_entered`), so the register state at `pc+1` is the intersection of the
   fast path's and the stub's load set; edges that leave registers pay a pad as everywhere.
4. **C1.** No count or poll in the fast path; the slow path is the template mid-block exactly as for any fallback (the instruction-count twin test
   covers it); the stub after it never double counts (it is not a block head).
5. **Facts.** An inline access relies only on facts of its own operands (they are in `guard_live_in` because table-class instructions expose all
   their uses) and writes a result whose tag it has just compared. The result type is the analysis' own `guard_def`; an **absent** speculation gives the untyped `Mem` form
   (slot-resident result, tag and payload copied, no fact). *Corrected in T7 (21.2):* a non-singleton or nil speculation used to take the same
   unchecked form while the analysis still assumed the speculated set for the destination (a nil fact makes a later store skip the GC barrier); such
   loads now keep the fallback, whose stub re-checks the fact.
6. **Lua 5.5 only.** `ARRAY_ALIMIT_TOUCH` (5.4) is not emitted; 5.4 regions are refused as before.

### 19.3 Tests

* `t15_core.rs`: 29 table programs (arrays with holes and out-of-range / zero / negative / huge / float / NaN keys, fields in tables of 4..40 keys
  (collision chains) with absent fields, insertion and removal mid-loop, field type flips, `__index/__newindex` installed by the loop, a callee, a
  count hook function and a finalizer, `debug.setlocal` turning the table into a number/string/nil/other table, callee-rebuilt tables,
  `table.remove/insert`, growth by `t[#t+1]`, nil stores, generational and incremental barrier loops, weak tables, `__gc` stores, tables shared by
  coroutines, special float values, stack growth, sparse integer keys, matrix product / n-body / fannkuch / sieve kernels, error messages) run
  against the jit-off twin in all nine allocation modes and three budgets (`twin`) **and** with a warm-up profile (`twin_prof`: `T15=on`, hot 2 and 9,
  seven option sets including `-tables`, budgets 3 and 17) so that the typed result paths exist; the instruction-count twin covers the corpus.
* `t15_core_emu.rs`: `inline_table_accesses_match_tier1_at_every_pc_on_synthetic_tables`: five loops over a pool of hand-built tables (array parts with
  random element tags, hash parts with real interned keys in main positions and collision chains, random `marked` colours, metatable pointers), seven
  allocation modes, five (entry speculation, profile) combinations, clean/dirty values, with and without chaos; compared with tier 1 at every region
  pc, including **the table memory itself** and the number of helper calls (a fast path that decides differently from tier 1 shows up).
* Mutants (feature `t15_mutants`, all detected): `tbl_no_lower_bound`, `tbl_no_upper_bound`, `tbl_no_tag_check`, `tbl_no_nil_check`,
  `tbl_no_meta_check`, `tbl_no_nonnil_check`, `tbl_no_barrier`.
* From the T4b review: `cross_int_slot_float_def` (an integer-class slot written by a float act and read through a memory operand `fy`),
  `cross_for_control_float_class` (the control slots of an integer `for` that is float-class elsewhere in the nest: payload-only `FORLOOP` stores),
  `xmm_exhaustion` (18 live floats, 14 XMM registers) and their emulator twins `fl_cross_class`, `fl_for_xreg_slot`, `fl_xmm_exhaust`.
* jit-diff: seven table templates (array/hash/meta/non-table/GC/shared/kernels) and modes `t15prof`, `t15profrand` (`T15=on`, `HOT=4`).

### 19.4 Results

Measured on the final tree (release `--features jit`, `RUSTYNAIL_JIT=on`, `taskset -c 4`, min of 7, seconds, one process at a time; "ref" =
`ref/55/build-seed0/lua`; T15 `off` = tier 1, `always` = every nest promoted at its first back-edge, `on` = promotion after 4096 back-edges so the
warm-up profile exists):

| program | ref | tier 1 (T15 off) | T15 always | T15 on | on / ref (tier 1 / ref) |
|---|---|---|---|---|---|
| nbody | 0.860 | 0.494 | **0.355** | **0.356** | 0.41 (0.57) |
| matmul | 0.631 | 0.252 | **0.134** | **0.135** | 0.21 (0.40) |
| fannkuch | 0.805 | 0.342 | 0.342 (no region) | **0.244** | 0.30 (0.42) |
| sieve | 0.648 | 0.493 | **0.354** | **0.356** | 0.55 (0.76) |
| spectral_norm | 0.868 | 0.546 | 0.544 | 0.546 | not promoted (gate) |
| heapsort | 0.808 | 0.429 | 0.430 | 0.427 | unchanged |
| queens | 0.875 | 0.490 | 0.493 | 0.493 | unchanged |
| table_array / table_hash | 0.686 / 0.340 | 0.449 / 0.279 | 0.450 / 0.287 | 0.450 / 0.287 | unchanged (noise) |

All 25 programs were also run `off` vs `on`: nothing else moved by more than the noise (loops_arith 0.417 -> 0.161 and mandelbrot 0.550 -> 0.115 are
T4b). Targets of the T5 row of section 8 (matmul <= 0.26, fannkuch <= 0.34, sieve <= 0.68, heapsort <= 0.45 of the reference): matmul 0.21, fannkuch
0.30, sieve 0.55 met; heapsort stays in tier 1 (0.53 of the reference, its 0.45 target is not met): the hot loop is the `while true ... break` loop of the `sift` closure, for which the promotion reports `no region (NoSuchLoop)` (nest selection of `scan_nests` / `select_region`, T2/T7), and it reads the array through an upvalue (`GETUPVAL` is a fallback, hoisting is the `hoist` pass).

### 19.5 Deviations, decisions, open items

1. **`always` mode and the profile.** `T15=always` promotes at the first back-edge, so loads that were not executed in the instrumented first
   iteration have no profile (and none if the OSR compile happened in the middle of an outer iteration). An unprofiled load stores an untyped
   result (`TRes::Mem(None)`), and its consumers (`EQI`, `ADD` on an `Any` slot ...) are fallbacks. The gate therefore counts such loads as fallback work
   (first version: fannkuch in `always` mode was 13% *slower* than tier 1); with the profile (`on`, or `t15prof`) the same loop is 29% faster. Speculating
   the result tag from the frame (`obs[a]`) was considered and not done (wrong guesses make every access a slow path and there is no respecialisation until T7).
2. **spectral_norm stays in tier 1.** The inner loop calls `A(i, j)` every iteration: `GETUPVAL`, `CALL` and the two arithmetic operations on the call
   result (`Any`: call results are not speculated in v1) are fallbacks, so the gate refuses the nest; with `+force` the region is 14% slower than
   tier 1 (0.62 vs 0.55: a flush and a stub with its guards around every call). It needs T6 (call-result speculation, `GETUPVAL` hoisting).
   nbody similarly keeps the `sqrt` call as a barrier per inner iteration (T6 intrinsic).
3. **Not inlined (still fallbacks):** `GETTABUP/SETTABUP` (upvalue table), `SELF`, `LEN`, `GETTABLE/SETTABLE` with a string/float key register, accesses whose
   table fact excludes `TABLE`. 5.4 regions remain refused (T-5.5).
4. **No main-position speculation, probe CSE, header hoisting or value forwarding** (doc 5.1 passes `fcache`/`hoist`): every access reloads the table
   pointer, `asize`/`array` or `lsizenode`/`node` from memory (L1 hits) and the hash walk costs about 15 instructions. Registers: regions with table
   accesses assign four integer registers instead of five.
5. **No exit stubs for table misses**: slow path = tier-1 template (one per instruction, out of line). `miss_ctr`/`exit_ctr`, the 1024-miss switch to
   `Policy::fallback_g` and the event helper are T7.
6. **Dead temporaries** (a loaded value that is stored back to its slot at every back-edge because the loop head does not keep it register-resident) still
   cost two stores per iteration (matmul: 4 of ~35 instructions); fixing it needs dirty tracking (16.1.2).

### 19.6 Gates run

`cargo fmt`; `clippy -D warnings` (5.5 default workspace all-targets, workspace + `rn-cli/jit,rustynail/jit`, `t15_mutants`, `t15_oracle`, 5.4 workspace + jit);
`rn-jit` tests with `--features jit` (also `RUSTYNAIL_JIT_T15=always`, `t15_core` under `RUSTYNAIL_GC_STRESS=1|2` and `RUSTYNAIL_JIT_STRESS=stackmove`), 5.4 build
(`--cfg rustynail_lua54`, `CARGO_TARGET_DIR=target-54`), workspace tests with `rn-cli/jit,rustynail/jit`, the T2b oracle; all 27 mutants detected (the seven new ones
by `table_programs_match_the_interpreter`, the emulator table test or a crash); `scripts/jit-text-identity.sh main` (identical, 1014184 bytes) and
`scripts/jit-dump-identity.sh da7d645` (tier-1 dumps identical with T15 off); jit-diff seeds 1..1100 (`JITDIFF_ON=1 JITDIFF_ENTERS=1`, modes off always on2 on10 t15 t15flush
t15rand t15verify t15verifyrand t15prof t15profrand; 18.3M entries per compiled mode, 20806 region recompiles in `t15`, 20183 in `t15prof`) 0 diffs; `RUSTYNAIL_GC_STRESS=2`
seeds 2000..2220 (off always t15 t15prof t15rand), `RUSTYNAIL_GC_STRESS=1` seeds 3000..3040, `RUSTYNAIL_GC_MODE=inc` seeds 4000..4200, `JITDIFF_HOOK=all` seeds 6000..6400:
0 diffs; stack mode (`TMO=300`, about 4 minutes per seed) only for a handful of seeds; `jit-official.sh` with `T15=always` `+force` and
`+force,+verify,+flushall`: OK; `gc.lua` and `gengc.lua` under `T15=always +force` `RUSTYNAIL_GC_STRESS=2`: OK; `RUSTYNAIL_GC_VERIFY=1` with `RUSTYNAIL_GC_STRESS=2` on the table programs
(test harness and a tiny script through rnlua): OK.

## 20. As built: T6 (calls and intrinsics in regions)

Files: `t15/intrinsics.rs` (new; the `Intr` enum and the pure result-type rule), `t15/codegen/calls.rs` (new; guards, inline forms, the libm helper,
inline `GETUPVAL`), `t15/effects.rs` + `t15/analysis.rs` + `t15/mod.rs` (class `InsClass::Intr`, `Policy::intr`, `RegionPlan::intr`, speculated call
results), `t15/codegen.rs` (three `Act` variants, effects, dataflow hooks, the gate), `t15/runtime.rs` (`T15Data::cprof`, `FuncMeta::intr`, `Warmup::cprof_addr`),
`lower.rs` (warm-up instrumentation of `CALL`), `rn-core/src/jit_hooks.rs` and `rn-std/src/lib_math.rs` (the builtin registry; JIT feature only). Tests:
`tests/t15_core.rs` (`in_*` programs, 12 mutants), `tests/t15_core_emu.rs` (`intrinsic_calls_match_tier1_at_every_pc`), `tests/t15_oracle.rs` (+ `tests/t15_oracle_progs/p10_math_calls.lua`),
`tests/jitdiff/gen.lua` (seven templates). No tier-1 byte changed with `T15` off; `unsafe`: one `extern "C"` helper (docs/UNSAFE.md, "J4 additions: T6").

### 20.1 Design as built

* **Recognising the callee.** The warm-up code of every `CALL` in a warm nest stores the payload of a *light C function* found in the callee slot (0 otherwise) into
  `T15Data::cprof[pc]` (7 micro-ops before the call template) and, for calls with results (`C >= 2`), the tag of `R[A]` after it into `prof[pc]` (the result
  profile; the resume-table entry behind a `CALL` is aliased to the label in front of that store, so a compiled Lua callee returning into the caller records it too).
  `T15Data::intr_sites` maps `cprof` to the registered `math` functions (`jit_hooks::set_intrinsics`, filled by `rn_std::lib_math::jit_builtins()` at `register_with`):
  `sqrt abs floor ceil min max sin cos exp log` (`fmod` is not inlined: integer `fmod` raises on zero and C `%` semantics differ). The sites travel as
  `FuncMeta::intr` -> `Policy::intr` -> `RegionPlan::intr`. There is no compile-time identity: only the *run-time* identity guard below makes the inline form valid.
* **Analysis.** A `CALL` at a site is `InsClass::Intr`: not a barrier, argument slots `A+1 ..` are typed uses (exposed to the stub, like table accesses; the callee slot
  is compared at run time, never typed). `Disp::Specialised` when `B`/`C` fit (`B == nargs + 1`, `C == 2`) and the argument facts are `INT`/`FLT` singletons
  (`intrinsics::result_set`: sqrt/sin/cos/exp/log -> `FLT`; abs/min/max -> the argument class (both arguments the same); floor/ceil -> `INT`, for a float argument only
  when the call-result profile says `INT`). Anything else is `Disp::LocalBarrier` = an ordinary `CALL` barrier. The continuation of an `Intr` is a resume point
  (its slow path may be a flat Lua call returning through the resume table), so its facts are clamped to the guard set of `stub(P+1)` exactly like after a barrier.
  **Call results** (all `CALL`s with `C >= 2`, intrinsic or not): the profile tag is the speculated fact of `R[A]` (`InsnPlan::guard_def`); the continuation stub guards it
  when it is read as a typed operand. A wrong speculation fails that stub and the frame continues in tier 1 (no exit counters until T7).
* **Code.** `Act::Intr` (inline), `Act::IntrH` (libm helper), `Act::GetUp` (inline upvalue load). The guards of an inline call, every failure -> the *slow path* =
  the tier-1 `CALL` template (`emit_fall` with the register-resident set after the operand loads, continuing in `stub(P+1)`):
  `thread.hookmask == 0`; no pending interrupt; callee slot tag `LCF` and payload == the registered address; `stack_size - func > a + b + 1 + LUA_MINSTACK`
  and `ci.next != 0` (the conditions of `pre_call_native_fast`: otherwise `precall_c` grows the stack / allocates a `CallInfo`, which is observable through memory
  accounting; the inline form never skips them). After the guards: the representability test (floor/ceil), then the instruction count (`Sub r11, 1; js cexit` = tier 1's
  `CountSub{1}` after the guards, M4; the exit is the instruction's budget exit, no refund), then the result, `ci.savedpc = &code[pc+1]` and `thread.top = func + a + 2`
  (the state `pre_call_native_fast` leaves for one result). Forms: `sqrtsd` (integer argument converted with `cvtsi2sd`); `abs`: `neg` for integers (wrapping,
  `mininteger` stays), sign-bit clear for floats; `floor`/`ceil` of an integer is the integer, of a float `F2IExact` (exact integral values) else the truncation
  corrected by one according to the sign (no `roundsd`: SSE4.1 is not assumed) and `NaN/inf/|x| >= 2^63` -> slow path; `min`/`max` of integers by compare, of floats
  `minsd`/`maxsd` with the *second* Lua argument as destination (`min(a,b) = b < a ? b : a` is exactly `minsd dst=b, src=a`, also for NaN and signed zeros).
  `sin cos exp log` flush the register-resident set, call `jit_t15_math` (same `f64::sin/cos/exp/ln` as `lib_math.rs`, so results are bit-identical) and continue in
  `stub(P+1)` (`Act::IntrH` is a `Fall` for the register dataflow; `ln` only for `log(x)` with one argument).
* **Inline `GETUPVAL`.** `closure -> upvals[b] -> v` -> copy (tag compared with the speculated tag when the fact is a singleton; mismatch -> the tier-1 template).
  Register-resident *escaped* slots are written back first (`spill`, registers stay valid): an open upvalue of this frame may alias a stack slot after
  `debug.upvaluejoin` (mutant `up_no_spill`, program `in_upvaluejoin`). Removes the flush/stub pair around every upvalue load (nbody's `sqrt`, spectral's `A`).
* **Profitability gate.** A fallback `CALL` weighs 4 ordinary fallbacks (`CALL_FALL_WEIGHT`): flush, the tier-1 call sequence, a stub with its guards and a reload. Inline
  intrinsics and `GETUPVAL` are typed work. `Act::IntrH` counts as a (cheap) fallback.
* **Flush at Lua calls (item 3 of the task).** Not changed: R-EXACT makes every register-resident value part of the frame at a call (the callee can read it with
  `debug.getlocal`, an error handler can traceback), and without dirty bits a flush stores all of them. What was done instead is to avoid the call being a fallback
  where possible (intrinsics, inline `GETUPVAL`) and to give its result a type. Clean-slot tracking (skip the stores of slots loaded by the stub and not redefined)
  is the remaining optimisation of `flush_stores`; it would save two stores per invariant slot per call and was not worth its dataflow in this task.

### 20.2 Invariants (review checklist)

1. **Guards first, then count, then commit (E3).** Every jump to the slow path precedes `Sub r11, 1`; registers of the result slot are written before the count but
   that slot is the callee slot (never register-resident: `Rg::emit_intr_*` refuses the region otherwise) and dead until the commit. Found by `intrinsic_calls_match_tier1_at_every_pc`:
   a `floor` overflow check *after* the count double-counted on the slow path.
2. **The slow path is tier 1's `CALL`** (hooks, interrupt, `CountSub`, stack growth, `extend_ci`, errors, `__call`, a Lua callee replacing `math.sqrt`): the inline form
   adds no semantics. `savedpc`, `top`, the frame and the budget equal tier 1's after the call (compared at every region pc by the emulator test: frame, `ci`, `thread`).
3. **No derived pointers survive the instruction.** `GetUp` uses `rax` only; the helper `jit_t15_math` allocates nothing and cannot move the stack.
4. **Facts.** The result fact of an inline call is the type the fast path produced; a mismatching profile takes the slow path whose `stub(P+1)` rejects it.
   Facts after an `Intr` are clamped like after a barrier (resume point), so the Lua-callee slow path cannot make them wrong.

### 20.3 Deviations and divergences

* **Stale temporary.** The interpreter's native call leaves a copy of the result at slot `A+B` (the C function's `push`); the inline form does not write it. It is a dead
  temporary above the call (observable only through `debug.getlocal` on `(temporary)` names of a frame that is not executing the call, and in `RUSTYNAIL_JIT_STRESS=stackmove`
  runs the stack is not moved by an inline call).
* The native `CallInfo` is not pushed/popped (no observer exists: no hook, no Lua code runs, no error is possible inside).
* `fmod`, `tan`, `atan`, `log(x, b)`, `math.type`, `tointeger`, `ult`, `random` stay ordinary calls. Mixed `min/max(int, float)` stays an ordinary call.
* Call-result speculation costs a stub failure per wrong guess until T7 adds exit counters; `fallback_g`-style demotion of intrinsic sites is T7's.

### 20.4 Results

Measured on the final tree (release `--features jit`, `RUSTYNAIL_JIT=on`, `taskset -c 5`, min of 5, seconds, one process at a time; the machine ran about 1.45x
slower than during T5 (reference nbody 1.27 s against 0.86 s), so only the ratios are comparable; "ref" = `ref/55/build-seed0/lua`; T15 `off` = tier 1, `on` =
promotion after 4096 back-edges, `always` = promotion at the first back-edge):

| program | ref | tier 1 (off) | T15 on | T15 always | off / ref | on / ref |
|---|---|---|---|---|---|---|
| nbody | 1.266 | 0.765 | **0.364** | 0.364 | 0.60 | **0.29** (T5: 0.41) |
| spectral_norm | 1.266 | 0.815 | 0.815 | 0.815 | 0.64 | 0.64 (not promoted: gate) |
| queens | 1.317 | 0.715 | 0.715 | 0.715 | 0.54 | 0.54 (a region is generated, break-even) |
| heapsort | 1.366 | 0.665 | 0.665 | 0.715 | 0.49 | 0.49 (the 0.715 of the 5-run table is noise; 9 runs: 0.665 for off/on/-hoist) |
| fannkuch / matmul / sieve | 1.216 / 0.965 / 0.865 | 0.515 / 0.364 / 0.615 | 0.365 / 0.214 / 0.465 | | 0.42 / 0.38 / 0.71 | 0.30 / 0.22 / 0.54 |
| mandelbrot / loops_arith | 1.366 / 1.266 | 0.815 / 0.615 | 0.214 / 0.264 | | 0.60 / 0.49 | 0.16 / 0.21 |

All 25 programs, `off` vs `on` (ratio on/off): binary_trees 1.00, closures 1.00, coroutines 1.00, fannkuch 0.71, fasta 1.00, fib 1.00, gc_churn 1.00, heapsort 1.00 (9 runs), json 1.00,
knucleotide 1.00, load_big 1.00, loops_arith 0.43, mandelbrot 0.26, matmul 0.59, metamethods 1.00, nbody 0.48, oop_methods 1.00, pcall_error 1.00, queens 1.00, sieve 0.76,
spectral_norm 1.00, string_ops 1.00, table_array 1.00, table_hash 1.00, varargs 1.00. Nothing moved by more than the (large, 50 ms quantised) noise; the earlier bench run
before the final refactor showed the same picture. The T6 targets of the section 8 row: **nbody <= 0.33: met (0.29)**; spectral_norm <= 0.55: **not met** (0.64); queens <= 0.47: **not met**
(0.54).

**Why spectral_norm and queens do not move.** spectral_norm's inner loop is dominated by the call `A(i, j)` (about 300 machine instructions per iteration: the flat call,
the callee in tier 1 and the return); `perf stat` shows 12.0 G instructions in tier 1 and 13.3 G with the region forced (`+force`): the region adds a flush and a stub with guards
around the call and runs `a = a + x[j] * A(i, j)` with an `INT|FLT` accumulator (initialised with the integer `0`, a float afterwards) that is an `Any` fact at the resume point behind the
call, so that `ADD` is a fallback too. The speculated float call result and the inline `GETUPVAL` make the multiplication and the upvalue load typed, but the call and the accumulator
remain; the gate (fallback `CALL` = 4 fallbacks) keeps the loop in tier 1, which is the right decision (forced with `+force`, or with the gate before T6, the region is 6% slower). A
real gain needs inlining the Lua callee (the J5 trace compiler) or a typed `INT|FLT` accumulator (peeling the inner loop, T7). queens recurses: its loop is entered once per call of `place` (a few iterations each), so the entry stub and the
guards of every entry eat what the inline `GETUPVAL`s save.

**nbody** gains from the inline `sqrt` (no flush, no tier-1 call sequence, no stub reload), the inline upvalue load of `sqrt` and the typed float arithmetic around it: the inner loop is one region
without a single fallback except `FORPREP`.

### 20.5 Tests and gates

* `tests/t15_core_emu.rs::intrinsic_calls_match_tier1_at_every_pc`: 15 programs (every intrinsic, integer and float operands) x 5 allocation/verify options x 4 entry speculations x 7
  scenarios (normal; a hook set; an interrupt pending; stack full; no spare `CallInfo`; wrong payload; wrong tag) on synthetic frames, emulator and native against tier 1 at **every region pc**
  (frame, budget, `ci` incl. `savedpc`, `thread` incl. `top`) with a helper model that implements the real `lmathlib` results (an independent oracle, `Model::try_math`); the broken-guard scenarios additionally
  require the region run to call the helper exactly as often as tier 1 (so an inline form that runs past a failed guard shows up even where the result is identical). About 11 400 inline
  calls and 136 000 slow-path calls per run; 400 000 region events.
* `tests/t15_core.rs`: 12 programs (`in_mix`, `in_special` (29 values x every function, 1 500 generated chunks), `in_shadow`, `in_hook`, `in_errors`, `in_edges`, `in_lua_calls`, `in_coroutine`, `in_gc`,
  `in_upvaluejoin`, `in_call_result`, `in_call_multi`) against the jit-off twin in all nine allocation modes, three budgets, with a warm-up profile (`twin_prof`, the mode that generates the
  inline forms), plus the instruction-count twin (T15 off vs `always,+force`, budgets none/5/37) over the same programs: equal.
* Mutants (feature `t15_mutants`; all detected by the full harness, 40 bugs + 1 graceful): `intr_no_hook_check`, `intr_no_identity`, `intr_wrong_identity`, `intr_no_stack_check`, `intr_no_ci_check`,
  `intr_no_count`, `intr_abs_no_clear`, `intr_minmax_swap`, `intr_minmax_order` (NaN / signed zeros), `intr_floor_ceil_swap`, `intr_floor_no_range`, `up_no_spill`. (`intr_no_stack_check` / `intr_no_ci_check` are observable
  only as a missing helper call: the emulator test is the only place that sees them.)
* The oracle (`t15_oracle`, 206 programs + `p10_math_calls.lua`: 12 plans, 4 711 entries, 175 000 checks): 0 violations; pass 1 records the callee of every `CALL` so that intrinsic sites are planned.
* jit-diff (`tests/jitdiff/gen.lua`: seven T6 templates): seeds 1..1000 (`JITDIFF_ON=1 JITDIFF_ENTERS=1`, `RUSTYNAIL_JIT_T15=always RUSTYNAIL_JIT_OPT=+force`, modes off always on2 on10 t15 t15flush t15rand
  t15verify t15verifyrand t15prof t15profrand; 17.2 M entries per compiled mode, 19 058 region recompiles): 0 diffs; seeds 5000..5500 (with the literal-operand template, same modes): 0 diffs;
  `RUSTYNAIL_GC_STRESS=2` seeds 2000..2220 (off always t15 t15rand t15prof t15profrand + on2/on10): 0 diffs; `JITDIFF_HOOK=all` seeds 6000..6250: 0 diffs; stack mode (`TMO=300`) seeds 3..4.
* `cargo fmt`; `clippy -D warnings` (5.5 default workspace all-targets, workspace + `rn-cli/jit,rustynail/jit`, `t15_mutants`, `t15_oracle`, 5.4 + jit); `rn-jit` tests 5.5 (also with
  `RUSTYNAIL_JIT_T15=always`; `t15_core` under `RUSTYNAIL_GC_STRESS=2` and `RUSTYNAIL_JIT_STRESS=stackmove`), 5.4 (`--cfg rustynail_lua54`, `target-54`), workspace tests with `rn-cli/jit,rustynail/jit`;
  `scripts/jit-text-identity.sh main` identical (1 014 184 bytes), `scripts/jit-dump-identity.sh 0c4156a` (tier-1 dumps) identical; `scripts/jit-official.sh` with `T15=always +force`, `+force,+verify,+flushall`
  and `T15=on hot=4 +force`: OK.

### 20.6 Open items for the reviewer / T7

1. Soundness doubts: none found. Points that deserve a second look: (a) the `Intr` continuation is clamped like a barrier because the slow path can be a flat Lua call; (b) the inline form does not write the stale
   result copy at `A+B` and does not push a native `CallInfo` (20.3); (c) `GetUp` spills escaped register-resident slots rather than proving that no `upvaluejoin` happened.
2. T7: intrinsic sites and call-result speculation have no exit counters (a wrong speculation fails the continuation's stub on every iteration until the nest is recompiled); `Policy::intr` is the place
   to demote a site (like `fallback_g`); a typed `INT|FLT` accumulator across a call (spectral_norm) needs the peel of inner loops.
3. Not done: clean-slot tracking in `flush_stores`; `math.fmod`/`tan`/two-argument `log`; `min`/`max` of mixed integer and float operands; inlining of Lua callees (J5).
## 21. As built: T7 (policy, exit feedback, respecialisation, nest selection)

Files: `t15/runtime.rs` (policy: `stub_fail`, `give_up`, `NestState` additions, `lower_with_regions_ex`, knobs), `t15/stubs.rs` (failure pads, entry
counter), `t15/codegen.rs` (parameter `fb`, upvalue weight in the gate), `t15/codegen/tables.rs` (soundness fix, six mutants), `t15/analysis.rs`
(`select_nest`), `t15/mutants.rs` (twelve mutants), `rt.rs` (policy inputs/outputs of a recompile, `recompile_with`, `Stats`), `lower.rs`
(`RegionOut::sites`). Tests: `tests/t15_policy.rs` (new), `t15_core.rs`, `t15_core_emu.rs`, `t15_analysis.rs`; `tests/jitdiff/gen.lua` (two
templates), `scripts/jit-diff.sh` (`+exithot=3` in the `t15*` modes, respecialisation counters in the `JITDIFF_ENTERS` report). No tier-1 byte
changed; `UNSAFE.md` has a J4-T7 section.

### 21.1 Policy as built

The region code has no type-guard exits of its own (T4/T5: a speculation that fails at run time ends in the stub of the next instruction and
therefore in tier 1), so **the stub failure is the one feedback channel**. `emit_stub` splits the guards of a stub into peel and non-peel
guards; a non-peel failure jumps to the stub's *failure pad*; a peel guard (the by-design first-entry failure of a slot whose entry type
differs from its loop-carried type, `EntryFact::peel`) goes straight to tier 1 uncounted (A: the peel exemption).

* **Failure pad** (cold, nothing is live: every value is in its VM slot): `fail = [nest.fail]; fail -= 1; if fail < 0 { CallH jit_t15_event(nest | ver << 24,
  StubFail << 16 | pc) ; ReloadFrame; if result { JmpR [cont] } }; jmp tier-1 label of pc`. The countdown is per **nest** (`NestState::fail`, shared
  by all code versions: B4 - retired code only writes into address-stable data that outlives every block) and is armed with `exit_hot`
  (`RUSTYNAIL_JIT_OPT=+exithot=N`, default 256) at promotion, `exit_hot << gen` after a respecialisation.
* **Head stub entry counter**: a successful `stub(H)` increments `NestState::entry_ok` (4 instructions, once per nest entry from tier 1; a
  region that loops internally does not pass it per iteration).
* **Decision** (`stub_fail`, current version only; a stale version just moves the activation to the current code):
  1. *Benign rate* (thrash detection, enter/exit ratio): at the head stub, if `fails * 4 <= entry_ok` (fewer than 20% of the entries since the last
     respecialisation fail) nothing changes but the penalty: the next decision needs twice as many failures (`armed *= 2`, capped).
  2. *Give up*: `gen == GEN_CAP` (3 respecialisations done): the nest goes `Off` and the prototype is recompiled without its region
     (`recompile_with(.., give_up = true)`, which may use `GIVEUP_RESERVE = 2` compile slots beyond the cap of 6, so that a nest that
     keeps failing can always be removed). `Off` is permanent.
  3. *Respecialise*: the helper reads the frame (exact at a stub) and finds the guards of the failing stub that fail (the stub table of the
     current version, `NestState::fb.sites`: slot, expected tag, peel, and the load whose `guard_def` produced the fact, if the previous
     instruction defines the slot). `gen == 0`: **retarget** - `obs[slot]` / `prof[def pc]` become the observed tag (a one-shot type change
     and the peel form need exactly this: promotion happens at a back-edge, where an accumulator already has its loop-carried type, so the
     first version often fails at every entry and the retargeted one is the peel form). `gen >= 1`: **widen** - the observed tag is joined into
     `entry_widen` / `def_widen` (`Speculation`); a non-singleton entry fact is `Any` (boxed) and a non-singleton load result keeps the
     fallback, so a flip-flop slot stops being speculated and the failures stop. Then `recompile`.
  4. A refused recompile: a transient refusal (memory limit / code cap, `Fail::Retry`) keeps the running version and re-arms the countdown; when
     the normal compile budget is spent (`Fail::Permanent`) the nest is **given up with a reserved slot** (review fix 2), so a failing region can
     always be removed.
* **Bounds**: respecialisations per nest <= 3, compiles per prototype <= 6 (+2 reserved for give-ups), so at most 8 blocks are kept per
  prototype. The promotion back-off of T3 is unchanged. Retired blocks stay mapped and charged until the prototype is freed.
* **Observability**: `RUSTYNAIL_JIT_VERBOSE` prints one line per decision (benign / retarget / widen with slot, expected and observed tag set /
  respecialised / given up / refused) and `[jit] t15 policy: regions N stub_events N respecialised N given_up N penalties N` at exit;
  `Stats` has `t15_stub_events t15_respec t15_giveups t15_penalties`; `scripts/jit-diff.sh` with `JITDIFF_ENTERS=1` sums respecialisations and
  give-ups per mode. Knobs: `+exithot=N`, `-respec` (no pads, regions are never revised: code identical to T5).
* **Type-changing accumulators.** No dual-path (`INT|FLT`) code was written (18.2.1 still holds). A loop-carried int/float accumulator that
  changes type at the first iteration is a peel slot (analysis); one that flips at run time is joined to `Any` by the analysis and handled by
  fallbacks; a live-in whose type changes between calls is retargeted, then widened (this section). All three settle without thrash
  (`flip_flop_live_ins_are_retargeted_then_widened_and_settle`, K = 1, 7, 1000).

### 21.2 Soundness fix (T5 review) and the gaps of the first mutation round

`classify_table` chose `TRes::Mem(None)` (commit any non-nil element, no tag check) for *every* speculated load result that is not
`INT`/`FLT`/a non-nil singleton, including a **nil** speculation and a **non-singleton** one (`INT|FLT`), while the analysis sets the fact of the
destination to that speculation. Downstream code relied on an unchecked fact (a store whose value fact is `nil` skips the GC barrier,
`Coll::No`). It was latent until exit feedback can produce such speculations (a retargeted `prof` byte `nil`, a widened `INT|FLT`). The inline
form is now generated only for *no* speculation (`Mem(None)`, no fact) or a non-nil singleton; anything else keeps the fallback (the tier-1
template and the stub after it, which re-checks the fact). Doc 19.2 item 5 is corrected accordingly (the fact of the destination is the
analysis' `guard_def`, which the inline form honours only when it checks it).
Tests: `NIL_RETARGET` (profile `INT`, then holes: the first decision retargets the speculation to `nil`, then young tables stored into an old table
in generational mode; the mutant `tbl_widen_unchecked` restores the old behaviour and is detected), `widened_load_result_is_not_inlined_unchecked`,
the flip programs with element-type flips (`def_flip*`).
The reviewer's list of mutation gaps (all six detected by the suite): `tbl_no_key_tag` (RegU key tag, `TB_SUBNORMAL_KEYS`: keys `k * 5e-324`),
`tbl_no_table_tag` (unproven table fact; reachable only after a widening, `TB_TABLE_TAG` and `table_and_key_flips`),
`tbl_no_coll_test` (collectable-bit test of a value of unproven class, `TB_MAYBE_COLL`), `tbl_no_s0_reload` (`SETFIELD` pointer reload),
`tbl_next_zext` (chain-offset sign extension, `tb_chain7/20/45` touch every node of every collision chain), `tbl_const_off` (payload offset of constant
indices, `TB_CONST_INDEX`). Of these, `tbl_no_table_tag`, `tbl_next_zext` and `tbl_no_s0_reload` were *already* caught by older table programs in the
whole-suite run (crashes); the new programs make the detection specific.

### 21.3 Regions and hooks

Established by `regions_do_not_run_while_a_hook_is_installed`: **compiled code, and so region code, is never entered while a count, line, call or
return hook is installed** (function entry and OSR both check `hook_mask() == 0`: with a hook set before the chunk runs, `Stats::enters`, `osr_enters`
and `t15_regions` do not move; without it they do). A hook installed *inside* a running region loop (by a callee, a finalizer, a hook function) is seen
at the next back-edge poll / after the helper, flushes and leaves, and the rest of the run is interpreted with exact hook counts (`mid` program of the
test, jit-diff hook templates). So the T12 `hook=all` result stands: hook semantics are those of the interpreter, region code is only exercised in the
hook-free snippets.

### 21.4 Nest selection

`scan_nests` reports `(head, end)` with `end` = the last backward `JMP` of the nest. A `while true do ... break ... end` loop compiles to a body whose
`break`s jump forward out of the loop and a closing `JMP` back to the head that is **dead code** (it follows the last `break`); the nest ends at that
dead instruction, which is no latch of any reachable loop, so `select_region(end)` answered `NoSuchLoop` (heapsort's `sift`; also any `while true`
loop whose body ends in `if .. then .. else break end`). `Analysis::select_nest(head, end)` falls back to the loop headed at `head` whose last latch is
inside the nest and then grows like `select_region`. `lower_with_regions` uses it. Unit test: `while_true_loops_with_break_are_selected_as_nests`
(`while true` with breaks, `repeat`, goto exit); twin program `WHILE_TRUE_SIFT` (the heapsort kernel) in `t15_policy.rs`.
Result (see 21.6): heapsort's `sift` now *gets* a region (18 typed, 8 fallback instructions) but the region is **10% slower** than tier 1
(0.515 s vs 0.465 s): its eight `GETUPVAL` instructions (the array is an upvalue) are fallbacks, each a flush + tier-1 template + stub round trip.
The profitability gate counted them at weight 1 and let it through; `GETUPVAL` fallbacks now weigh 2 in the gate, so heapsort and queens (which
had weighted ratios 2.25 and 1.93 against the threshold 2) stay in tier 1. They need an **inline `GETUPVAL`** (a typed load of the upvalue's
value into the slot, with the tag from `prof`) in the code generator; that is the single change that would make both profitable and was left out of T7
(it touches `codegen.rs` next to the T6 work).
Other remaining `NoSuchLoop` reports (json, string_ops) are not dead-back-edge loops (jumping into loops, irreducible shapes); generic `for` nests stay
`Irreducible` (15, item 6).

### 21.5 Tests and mutants

* `tests/t15_policy.rs` (16 tests + the mutant driver): twin programs against the jit-off interpreter in eight option sets (`+exithot=1/2/5`, `-ra`,
  `+randra`, `+flushall`, `+verify`), with the warm-up profile (`T15=on`, hot 2/9) and under count budgets 3/17: live-in flips for K = 1, 7, 1000, a
  one-time change, many types (numbers, numeric strings, `__add` tables, errors), element-type flips (def-site feedback) for K = 1, 7, 1000, table and
  key live-ins that are sometimes not a table / a float / subnormal / a string, the peel program, a rare failure, four phases (give-up), six loops
  with flips (compile cap), recursion/`pcall`/coroutines suspended in older versions, errors and `debug.setlocal` from callees, accumulators at K =
  1, 7, 1000, the nil retarget, the `while true` kernel. Policy assertions on `Stats`. Instruction-count twin (output and `Stats::insns` equal across
  respecialised versions, budgets none/5/37, opts default/`-ra`/`+randra`/`+flushall`/`+verify`). Lifetime: recursion and coroutines with versions
  retired under them (also in `Verify` mode, where freed chunks fault), 40 prototypes freed with live versions, code cap during a respecialisation,
  `pq` queries of the retired-block count. Hooks (21.3).
* `t15_core_emu.rs::failure_pads_count_fire_the_event_and_continue_in_tier_1`: the pads in the emulator (countdown, event arguments, entry counter,
  result identical to tier 1).
* Mutants (feature `t15_mutants`; `every_mutant_is_detected` in `t15_core.rs` and `every_policy_mutant_is_detected` in `t15_policy.rs`; all detected):
  `tbl_no_key_tag tbl_no_table_tag tbl_no_coll_test tbl_no_s0_reload tbl_next_zext tbl_const_off` (T5 gaps), `tbl_widen_unchecked`,
  `pol_no_widen` (widened facts not applied), `pol_no_peel_exempt`, `pol_no_penalty`, `pol_no_giveup`, `pol_no_fail_count`, `pol_drop_old_block`
  (retire-while-active: the old block is freed).
* jit-diff: `T7` templates (call-level live-in flips with K = 1, 7, 1000 over numbers, strings, array/hash tables and int/float/subnormal/string keys;
  four-phase thrash with an element-type flip) and `+exithot=3` in all `t15*` modes, so that decisions happen inside the generated programs.

### 21.6 Results and gates

Measured (release `--features jit`, `RUSTYNAIL_JIT=on`, `taskset -c 4`, min of 3, seconds; coarse timer, noise ~0.05 s): T15 off -> on:
loops_arith 0.465 -> 0.164, mandelbrot 0.566 -> 0.164, matmul 0.265 -> 0.165, fannkuch 0.364 -> 0.264, nbody 0.515 -> 0.365, sieve 0.516 -> 0.365,
heapsort 0.465 -> 0.466 and queens 0.515 -> 0.516 (no region: 21.4), spectral_norm 0.565 -> 0.566, the other programs unchanged within noise
(table_array/table_hash/varargs wobble +-0.05 as in 19.4). The failure pads and the entry counter cost nothing measurable on the programs with regions
(`stub_events 0` on all 25 with the default `EXIT_HOT` 256: no program of the corpus fails its stubs often enough to trigger a decision). Regions exist
for: fannkuch, loops_arith, mandelbrot, matmul, nbody, sieve, table_array, string_ops (small), json (small); heapsort's `sift` is selected but refused by the gate.
Gates (all green): fmt; clippy -D warnings (5.5 default workspace, workspace + `rn-cli/jit,rustynail/jit`, 5.4 + jit, `t15_mutants`, `t15_oracle`);
rn-jit tests 5.5 (also `RUSTYNAIL_JIT_T15=always`; `t15_core`/`t15_policy` under `RUSTYNAIL_GC_STRESS=1|2`, `RUSTYNAIL_JIT_STRESS=stackmove`,
`RUSTYNAIL_GC_MODE=inc`) and 5.4 (`--cfg rustynail_lua54`); workspace tests with `rn-cli/jit,rustynail/jit` (81 binaries); `jit-text-identity.sh main`
(identical, 1014184 bytes) and `jit-dump-identity.sh` (tier-1 dumps identical); jit-diff seeds 1..1100 (`JITDIFF_ON=1 JITDIFF_ENTERS=1`, modes off always on2
on10 t15 t15flush t15rand t15verify t15verifyrand t15prof t15profrand, `RUSTYNAIL_JIT_T15=always +force`) 0 diffs (17.9M entries per compiled mode; in
`t15`: 21204 promotions, 29039 recompiles, 7034 respecialisations, 801 give-ups), seeds 3000..3250 on the final binary 0 diffs, `RUSTYNAIL_GC_STRESS=2`
seeds 2000..2220 (off always t15 t15prof t15rand) 0 diffs (1299 respecialisations, 163 give-ups); `jit-official.sh` with `T15=always` `+force`,
`+force,+exithot=3` and `+force,+verify,+flushall,+exithot=2`: OK; all mutants of `t15_core` (33) and `t15_policy` (8) detected.
Open items: (1) inline `GETUPVAL` (heapsort, queens, spectral_norm); (2) no dual-path `INT|FLT` code; (3) retarget/widen are decided from one sampled frame
per decision (three-way type mixes need a further round; bounded by the gen cap); (4) `tight_limits_fall_back_to_the_interpreter_consistently`
(`jit_memlimit.rs`) is sensitive to a few hundred bytes of descriptor memory under `T15=always` (the per-nest feedback record is allocated lazily to keep
`NestState` small for that reason); (5) the unreachable-latch fallback of `select_nest` does not cover json's/string_ops' remaining `NoSuchLoop` nests.

### 21.7 Review fixes (independent Opus review of T7)

1. **Dead countdown.** The failure pad compared the zero-extended 32-bit countdown as a 64-bit value: it fired only on the exact `0 -> -1` step and
   stored `0xFFFFFFFF` (read back as +4294967295: never again). The pad now ends in a 32-bit signed memory compare (`BrM Lt B4`). The stale-version
   branch of the event helper skipped the decision *without re-arming*, so after a stale fire the current version's failures never reached the policy;
   it now re-arms (`armed - 1`) and counts `Stats::t15_stale_events`. Tests: emulator countdown at -1 and -5 fires; `a_stale_fire_does_not_kill_the_countdown`;
   mutants `pol_countdown_64bit` (`t15_core` driver) and `pol_stale_no_rearm`.
2. **Give-up reserve.** A respecialisation refused for lack of a compile slot (`Fail::Permanent`) used to park the nest with its failing region; it
   now gives the nest up with the reserved slots (whatever `gen` is); a transient `Fail::Retry` re-arms instead of stopping decisions. Test
   `an_exhausted_compile_cap_still_gives_the_nest_up` (six promoted loops, then flips), mutant `pol_cap_no_giveup`.
3. **Guards feedback cannot change.** `StubGuard::def_pc` is set only for table/upvalue loads and the method result of `SELF` (not its receiver slot);
   a fact from `LEN`/`POW` is `fixed`; a decision that finds a fixed guard, or no guard that differs by tag class (a variant-only mismatch), gives
   the nest up at once instead of recompiling the same speculation three times (`unchangeable_guards_are_given_up_at_once`).
4. **Tests.** Mutant `gate_upval_single` + `the_gate_refuses_the_upvalue_heavy_sift` (a heapsort kernel without `+force`); `select_nest` over two loops
   sharing a head (backward gotos) and reachable code between the chosen latch and the dead closing `JMP`; `outer_activations_really_run_in_retired_blocks`:
   recursion where inner calls respecialise the nest, then the callee changes an outer local (`debug.setlocal`) with the countdown at -1, so the stub
   fails in the *retired* version's pad - `t15_stale_events >= 1` proves that code in retired blocks ran (also in `Verify` mode). The earlier lifetime
   tests could be satisfied by interpreted resumes of suspended coroutines; this one cannot.

### 21.8 After the T6 merge: policy against intrinsics, typed call results and inline GETUPVAL

On the merged tree seven policy tests failed. Classification:

* **(b) real interaction bug, fixed:** `a_one_time_type_change_is_retargeted_once`, `a_nest_at_the_respecialisation_cap_is_given_up` (2 give-ups instead
  of 1), `an_exhausted_compile_cap_still_gives_the_nest_up`. T6 speculates the result of every `CALL` with results (`prof[pc]` of the call, guard on the
  continuation stub). `emit_stub` computed `def_pc` only for table/upvalue loads and marked every other defining instruction `fixed`, so a wrong call-result
  guess made the caller's loop nest give up at the first decision. `CALL` is now a retargetable/widenable def (`load_spec` already joins `def_widen` for
  call results); `LEN`/`POW` stay `fixed`.
* **(a) intended T6 change, tests adapted:** `flip_flop_live_ins_*` (a second nest, the caller's loop, now has a speculated call result: up to `2 * GEN_CAP`
  respecialisations and `2 + 6` recompiles), `compiles_per_prototype_are_bounded` and `an_exhausted_compile_cap_*` (the bound is per prototype and two prototypes
  have loops: `2 * (5 + GIVEUP_RESERVE)`), `the_gate_refuses_the_upvalue_heavy_sift` (GETUPVAL is inline now, so the kernel's region is generated; the test runs
  with `-hoist`, which keeps the fallback form that the gate weight and the mutant `gate_upval_single` are about), `a_benign_failure_rate_is_penalised_*` (the
  rare type difference was in the *call result* of the caller's loop, an interior stub that has no entry counter and therefore no benign-rate test; the program now
  returns a stable type. Interior stubs without a rate are a known limitation: a call result that differs once in 256 stub failures costs one respecialisation).
* **Intrinsic demotion (new).** An inline `math` call has two failure channels: (1) its slow path (callee identity mismatch, stack / `CallInfo` shortage) - the
  slow path now starts with the nest countdown (`emit_intr_slow_count`: flush, `fail -= 1`, event helper when it fires, then the unchanged tier-1 `CALL`
  template); (2) the stub after the call failing on the speculated result (a replaced `math.sqrt` returning another type). Both reach `stub_fail`, which
  *demotes* the site (`T15Data::intr_demoted`, filtered out of `intr_sites()` for every later lowering: the `CALL` is an ordinary barrier call), counts a
  respecialisation (so demotions are bounded by the gen cap and the compile cap) and recompiles. `Stats::t15_demotions`. Test
  `failing_intrinsic_sites_are_demoted_to_ordinary_calls` (replacement of the same type, of another type, returning a string; option sets incl. `+verify,+flushall`);
  mutant `pol_no_demote`.
* **Boolean flips.** queens ran into the respecialisation cap on the default configuration (4 stub events, 3 respecialisations, a give-up): its `cols[c]`,
  `d1[..]`, `d2[..]` loads flip between `false` and `true`, and each load cost one decision. A mismatch between two boolean tags now widens at once (no retarget
  step) and widens every load of the nest whose profile is boolean: one decision, no give-up. Test `boolean_flips_at_several_loads_settle_in_one_decision`
  (queens N = 12; mutant `pol_no_bool_widen`).
* **Measured** (release, `taskset -c 4`, min of 5, seconds, `RUSTYNAIL_JIT=on`, T15 off -> on): **heapsort 0.435 -> 0.346** (the region is profitable now:
  inline `GETUPVAL`), nbody 0.50 -> 0.23-0.25, queens 0.495 -> 0.498 and spectral_norm 0.549 -> 0.551 (no gain, no loss).


### 21.9 T6 review fixes (independent Opus review of T6)

1. **Stay path of the failure pad (accounting).** `emit_intr_slow_count` calls the policy helper (`CallH` clobbers `r11`); when the helper returns 0 (refused or
   deferred recompile, deny counter, nest no longer promoted) control continues at `go` into `emit_fall`, whose `store_budget` wrote the garbage in `r11` to `[r15]`.
   Fix: `go` is followed by `load_budget()` (on the not-fired path `[r15]` equals `r11`, which the pad stored, so the reload is a no-op there). Mutant
   `pol_no_budget_reload_stay`; tests: `intrinsic_failure_pad_counts_demotable_failures_only_and_stays_exactly` (emulator, helper model answering 0, budgets
   MAX/40/7, frame/budget/`ci`/`thread` equal to tier 1's) and `the_stay_path_of_the_failure_pad_keeps_the_budget` (runtime: the recompile refused by the memory
   limit, instruction-count twin against tier 1, budgets none / 100000 / 37).
2. **Only demotable failures feed the countdown.** The guards of an inline intrinsic now jump to two slow labels: hook and interrupt (and the floor/ceil
   representability miss) to the plain one, callee identity / stack room / spare `CallInfo` to a *counted* one (`Rg::slow_counted`; the pad of 21.1 is emitted only
   in front of the counted copy of the slow path; the tier-1 `CALL` template is emitted once per label). Mutant `pol_count_all_slow` (every slow path counts);
   test: the same emulator test (`Hook` scenario: no event; identity / stack / `CallInfo` scenarios: at least one) and `hook_misses_of_inline_calls_do_not_demote`
   (runtime; it cannot reach the guard with a hook set in-process, which is why the emulator test is the one that detects the mutant). A floor/ceil range
   miss is always followed by a result-guard failure of the continuation stub (the result is a float where the profile said integer), which demotes the site
   legitimately; it no longer also counts at the pad.
3. **Demotion takes effect only when the recompile succeeds.** `intr_demoted` is pushed before the recompile (the recompile reads it), and rolled back on
   `Err(Retry | Permanent)` (the old code with its inline intrinsics keeps running, so `intr_sites()` must still list the pc); on `Retry` the generation is
   restored too and the countdown re-armed for the old generation. Before: a refused demotion (code cap / memory limit) became a permanent give-up at the next fire
   ("cannot be changed by feedback"). Mutant `pol_demote_no_rollback`; test `a_refused_demotion_is_retried_not_given_up` (limit, then room: `t15_giveups == 0` and a
   respecialisation happens).

## 22. As built: T8 (verification tooling: shadow verification, exit storm, stress, mutation matrix)

Files: `t15/verify.rs` (registry, helpers, storm counters), `t15/codegen/shadow.rs` (code emitted for both options), `t15/stubs.rs` (hooks in
`edge`/`finish`/`emit_stub`), `emu.rs` (`run_until_shared`), `tests/t15_stress.rs`, `scripts/t15-mutants.sh`, jit-diff modes, the `jit_diff` fuzz
target, `scripts/{ci-local,asan-jit,miri}.sh`. Both options are runtime-only (`RUSTYNAIL_JIT_OPT`), never default; without them region bytes are
unchanged (checked by the dump identity script), and the JIT-off `.text` is untouched (no non-`jit` code changed).

### 22.1 `+shadow` (section 7 item 4, as built)

* **Blocks.** A tier-1 block (head to next head, inside the region) is verified when every non-follower instruction is one of the *pure* region
  templates (`codegen/shadow.rs::pure_act`: integer / float arithmetic, moves, loads of constants, compares, `TEST`, `JMP`, `FORLOOP`/float
  `FORLOOP`, `DivK`, shifts). Table accesses, calls, intrinsics, `GETUPVAL` and fallbacks are excluded (they have helper calls, exits mid-instruction
  or heap effects).
* **Code.** At the region label of the block start (behind the exit-storm guard, before the block count): flush the register-resident set with the
  *unmutated* flush, store the budget, `CallH jit_t15_shadow_pre(block)`, reload the registers and `r11`. Every edge that leaves the block (target is
  a tier-1 head) is rewritten to a verification pad: flush, store budget, `CallH jit_t15_shadow_post(block, successor pc)`, reload, jump to the ordinary
  edge label (so edge pads, polls and `Leave` exits are unchanged behind it).
* **Check.** `post` runs the tier-1 `Program` of the same lowering (cloned when the lowering has finished, because the region hook runs before the
  out-of-line tier-1 code exists) with `emu::run_until_shared` from `pcl[block start]` on the shadow copy of the frame until it reaches the `Bind` of a
  tier-1 block head (or the entry stub of a region head, to which tier-1 jumps to a head are redirected). Compared: the successor pc, the budget word
  after the block's count, and every slot `[0, maxstacksize)`: tag byte always, payload bits for `INT`, `FLT` and collectable tags (exact bit
  patterns, NaN payloads and -0.0 included; both sides are hardware-semantics: the emulator's `fbin` copies SSE rules). Mismatch: stderr
  `[t15] SHADOW VERIFICATION FAILED in block [a, b] (successor s): slot n: region ..., tier 1 ...`, abort.
* **Inconclusive** (counted, not an error): the emulated block meets a helper `CallH` (slow path), a trap/budget exit, or 20000 steps.
  `RUSTYNAIL_JIT_VERBOSE=1` prints `[jit] t15 shadow: checked N inconclusive M`; jit-diff `JITDIFF_ENTERS=1` fails a `*shadow*` mode that checked 0.
* **What it does not check:** register persistence across blocks, exit maps and stub guards (covered by `+flushall`, `+exitstorm`, `+verify`, the twins);
  blocks with fallbacks / table ops / calls.

### 22.2 `+exitstorm=N` (item 6, as built)

Pseudo-guards (a per-site 32-bit counter in a process-wide static table, `verify::storm_counter`; a site fires and resets when the counter reaches
N) at these sites:

| site | fires -> |
|---|---|
| every instruction boundary of region code (`reg_l[pc]`, so also every fall point, every block head *before* its count, every arrival from an edge) | write back `RR(pc)`, store budget, jump to tier-1 `pc` (mid-block re-entry, the exit map of every pc) |
| every stub entry (`stub(P)`, period `max(N, 2)`: with N = 1 the region would never run) | the stub's plain failure path: tier 1 at `P` |
| every edge pad that is not already a `Leave` | full write-back of the source state, tier 1 at the target |
| every back-edge poll | the real trap exit (`exit_trap`: the interpreter continues, `ci.trap = 1`) |

Stub entries jump behind the boundary guard of their pc, so a region visit always executes at least one instruction (progress for N = 1). Not covered:
real guard exits inside table / intrinsic fast paths (their slow paths are ordinary fallbacks, reached on real misses), the budget exit (`cexit`, tier 1's
own exit; the budget-limited runs of `t15_stress` cover it). Results must equal the jit-off run and `Stats::insns` must equal the run without regions
(`t15_stress`). Event-stream comparators of `t15_core_emu` cannot be used with the storm (the arrival sequence changes by design).

### 22.3 Tooling

* `tests/t15_stress.rs` (opt-in: `cargo test -p rn-jit --features jit --test t15_stress`): the `t15_core` corpus plus table / cross-class programs
  (about 300) under 23 option strings x {T15=always, profiled hot 2 and 9} and two budget groups. Also run with `RUSTYNAIL_GC_STRESS=1|2` and
  `RUSTYNAIL_JIT_STRESS=stackmove`.
* jit-diff modes `t15shadow t15shadowrand t15storm1/2/7/61 t15storm t15stormshadow t15stormprof` (see JIT_TESTING.md).
* `fuzz/fuzz_targets/jit_diff.rs`: first byte bits 5-6 = T15 mode, then one byte = storm / shadow / randra.
* `scripts/ci-local.sh` (`jit-t15-*` steps), `scripts/asan-jit.sh` (T15 corpus, `t15_stress` subset), `scripts/miri.sh jit` (`t15::alloc`, `t15_core_emu`).
* Mutation matrix: `scripts/t15-mutants.sh`.

### 22.4 Which tool catches which mutant

`scripts/t15-mutants.sh`, run on the as-built tree (every mutant of `src/t15/mutants.rs` except the graceful `bad_fall_targets`). Columns: **core** =
`tests/t15_core.rs` (twin corpus, insns twin, emulator tests); **policy** = `tests/t15_policy.rs` (run for `pol_*`/`gate_*` only, `.` = not run);
**shadow** = `t15_stress` `s_shadow*` (**S** = the shadow check itself aborted, **D** = only the twin / instruction-count comparison of the shadow-mode
runs failed, `-` = survived); **storm** = `s_storm*` + `s_budget_storm`; **misc** = `s_randra* s_flushall s_verify*`. `D` = detected (assertion, abort, signal or
100 s timeout), `-` = survived that tool.

| mutant | core | policy | shadow | storm | misc |
|---|---|---|---|---|---|
| `count_after_fall` | D | . | S | D | D |
| `no_budget_reload_after_call` | D | . | D | D | D |
| `drop_guard_first` | D | . | S | D | D |
| `drop_guard_last` | D | . | S | D | D |
| `drop_guard_after_fall` | D | . | S | D | D |
| `flush_skip_last` | D | . | D | D | D |
| `flush_skip_tag` | D | . | D | D | D |
| `leave_no_budget` | D | . | D | D | D |
| `pad_no_irq_poll` | D | . | - | - | - |
| `fall_no_budget` | D | . | D | D | D |
| `stub_no_load_last` | D | . | D | D | D |
| `flt_flush_int_tag` | D | . | D | D | D |
| `fcmp_negated` | D | . | S | D | D |
| `i2f_missing` | D | . | S | D | D |
| `fneg_missing` | D | . | S | D | D |
| `fforloop_always_pos` | D | . | S | D | D |
| `fforloop_clobber_idx` | D | . | S | D | - |
| `fbin_alias_commute` | D | . | S | D | D |
| `tbl_no_lower_bound` | D | . | D | D | D |
| `tbl_no_upper_bound` | D | . | D | D | D |
| `tbl_no_tag_check` | D | . | D | D | D |
| `tbl_no_nil_check` | D | . | D | D | D |
| `tbl_no_meta_check` | D | . | D | D | D |
| `tbl_no_nonnil_check` | D | . | D | D | D |
| `tbl_no_barrier` | D | . | D | D | D |
| `tbl_no_key_tag` | D | . | D | D | D |
| `tbl_no_table_tag` | D | . | D | D | D |
| `tbl_no_coll_test` | D | . | D | D | D |
| `tbl_no_s0_reload` | D | . | D | D | D |
| `tbl_next_zext` | D | . | D | D | D |
| `tbl_const_off` | D | . | D | D | D |
| `pol_no_widen` | - | D | - | - | - |
| `pol_no_peel_exempt` | - | D | - | - | - |
| `pol_no_penalty` | - | D | - | - | - |
| `pol_no_giveup` | - | D | - | - | - |
| `pol_no_fail_count` | D | D | - | - | - |
| `pol_drop_old_block` | D | D | D | D | D |
| `tbl_widen_unchecked` | D | . | D | D | - |
| `pol_countdown_64bit` | D | D | - | - | - |
| `pol_stale_no_rearm` | - | D | - | - | - |
| `pol_cap_no_giveup` | - | D | - | - | - |
| `gate_upval_single` | - | D | - | - | - |
| `intr_no_hook_check` | D | . | - | - | - |
| `intr_no_identity` | D | . | D | D | D |
| `intr_wrong_identity` | D | . | - | - | - |
| `intr_no_stack_check` | D | . | - | - | - |
| `intr_no_ci_check` | D | . | - | - | - |
| `intr_no_count` | D | . | D | D | D |
| `intr_abs_no_clear` | D | . | D | D | D |
| `intr_minmax_swap` | D | . | D | D | D |
| `intr_minmax_order` | D | . | D | D | D |
| `intr_floor_ceil_swap` | D | . | D | D | D |
| `intr_floor_no_range` | D | . | D | D | D |
| `up_no_spill` | D | . | - | D | D |
| `pol_no_demote` | - | D | - | - | - |
| `pol_no_bool_widen` | - | D | - | - | - |
| `exitmap_wrong_reg` | D | . | D | D | D |
| `leave_drop_entry` | D | . | D | D | D |
| `fall_succ_no_guards` | D | . | S | D | D |
| `rr_join_union` | D | . | D | D | D |

Reading the table:

* Every non-policy mutant is detected by `core`, and every one that the stress runs survive is caught by `core`; the policy mutants are caught by
  `t15_policy` (a few also by `core`). The new tools therefore found **no gap in the old suite for the present mutant set**; their value is breadth
  (new corpora, mid-block exits, randomised allocation) and independence (the shadow check does not use the twin comparison).
* The shadow check (**S**) fires on mutants that corrupt a pure block's result, budget or facts: `i2f_missing`, `fneg_missing`, `fcmp_negated`,
  `fforloop_*`, `fbin_alias_commute`, `count_after_fall` (budget word), `drop_guard_*`, `fall_succ_no_guards`. It is blind, as designed, to exit maps and
  write-back (`flush_*`, `exitmap_wrong_reg`, `leave_*`: caught by storm / flushall / twins), to blocks with table ops, calls or intrinsics (`tbl_*`, `intr_*`,
  `up_no_spill`) and to the interrupt poll (`pad_no_irq_poll`: only `core` sets the interrupt flag).
* `rr_join_union` (dataflow bug: wrong register-resident set at joins), `exitmap_wrong_reg` (wrong entry of a write-back map), `leave_drop_entry` (dropped exit-map
  entry) and `fall_succ_no_guards` (no re-guard after a fallback) are the T8 additions for the section 7 item 8 list. "Keep a table pointer across a barrier"
  has no code to mutate: region code caches no table pointer across instructions (section 19.2 item 1, every access re-derives); the nearest mutant is
  `tbl_no_s0_reload` (a derived table pointer survives a clobbering step), detected by all tools. "Drop the flush before a barrier op" is `fall_no_budget` /
  `flush_skip_*`; "remove the peel exemption" is `pol_no_peel_exempt` (policy suite).

### 22.5 Results and gates (as run for T8)

* **Divergences / bugs found in region code: none.** All stress modes over the whole corpus were green on the first run (the only failures during
  development were my own: the event-stream comparators of `t15_core_emu` cannot take `+exitstorm`, see 22.2). Shadow verification checked 227 million
  block executions in the 800-seed jit-diff run (`t15shadow`), 68 million combined with the storm, with 0 mismatches.
* Unit/integration: `cargo test -p rn-jit --features jit` (5.5 default, `RUSTYNAIL_JIT_T15=always`, 5.4 cfg), workspace with `rn-cli/jit,rustynail/jit`, and
  `--test t15_stress` (35 tests, also under `RUSTYNAIL_GC_STRESS=2` and `RUSTYNAIL_JIT_STRESS=stackmove` subsets): all pass. clippy `-D warnings`: 5.5 default,
  5.4+jit, workspace+jit, `t15_mutants`, `t15_oracle`: clean.
* Identity: `jit-text-identity.sh 8667648`: `.text` identical (1014184 bytes); `jit-dump-identity.sh 8667648`: 25 programs / 69 protos identical.
* jit-diff (`JITDIFF_ON=1 JITDIFF_ENTERS=1`, T15=always +force): 800 seeds x {off always t15 t15flush t15rand t15verify t15verifyrand t15prof t15profrand
  t15on on2 on10 t15shadow t15shadowrand t15storm1/2/7/61 t15storm t15stormshadow t15stormprof}: 0 diffs; GC stress 2: 160 seeds x 8 modes, 0 diffs;
  `hook=all`: 150 seeds x 6 modes, 0 diffs; `RUSTYNAIL_JIT_STRESS=stackmove` with `t15storm`: seeds 1-30 except 10 and 15 (those two take more than 300 s
  per run under stackmove already in plain tier 1: quadratic stack moves, unrelated), 0 diffs.
* Official suite (`jit-official.sh`, `RUSTYNAIL_JIT=always RUSTYNAIL_JIT_T15=always`): `+force`, `+force,+shadow`, `+force,+exitstorm=7`: all OK.
* Fuzz: `cd fuzz && cargo +nightly fuzz build --features jit jit_diff` builds; a 120 s smoke run with 30 T15 seeds: 1801 runs, no finding.
* ASan (`scripts/asan-jit.sh`, nightly build-std): official suite (always and on), tests/vm corpus, GC-stress and stackmove corpora, **and the new T15
  corpus runs (`+force`, `+shadow`, `+exitstorm=7`)** and the `t15_stress` subset: clean. Two `cargo test -p rn-jit` failures under ASan, both independent
  of T8: `jit_j3c_env::failing_mmap_of_code_chunks_falls_back_silently` (ASan cannot run under the address-space limit the test sets) and
  `t15_core_emu::inline_table_accesses_match_tier1_at_every_pc_on_synthetic_tables` (`ptr::add` overflow precondition in the test's synthetic-table
  helper `wr`; reproduced identically at the base commit 8667648).
* Miri (`scripts/miri.sh jit`): 71 tests pass (546 s): `t15::alloc`, `t15_analysis`, emulator diff tests, `t15_core_emu` float and the new
  `exit_storm_regions_match_tier1_emulated`. `t15_core_emu` also contains tests that fail under Miri at the base commit as well (integer-to-pointer
  casts / protector violations in the synthetic-table and helper-model harness: `inline_table_accesses_*`, `intrinsic_calls_*`, `failure_pads_*`,
  `region_code_is_generated_without_helper_calls`) and two that need over an hour (`regions_with_helper_calls_*`, `regions_match_tier1_emulated_and_native`);
  the Miri suite filters those out. Open item: make the harness Miri-clean.

## 23. T13: J4 measurement (quiet machine, 2026-10-30, main 43ade31)

`scripts/bench-cycles.sh`, 25 programs, 9 interleaved runs, CPU 4, cycles (min), ratios to the reference Lua 5.5 (`ref/55/build-seed0`, `RUSTYNAIL_SEED=0`);
`jit` release build, `RUSTYNAIL_JIT=off|on`, `RUSTYNAIL_JIT_T15=off|on`; LuaJIT 2.1 uses `bench/luajit` variants where present.

| geomean of cycles | interpreter | tier 1 | tier 1 + regions | LuaJIT |
|---|---|---|---|---|
| all 25 | 0.877 | 0.632 | **0.521** | 0.252 |
| numeric | 1.016 | 0.531 | **0.275** | 0.143 |

J4 targets (all <= 0.60, numeric <= 0.30) are met. Regions gain most on loops_arith (0.479 -> 0.184), mandelbrot (0.591 -> 0.123), matmul (0.372 -> 0.196),
nbody (0.588 -> 0.266); none on call/closure/method-heavy programs (fib 0.532, spectral_norm 0.606, oop_methods 0.770, metamethods 0.963, closures 0.644,
knucleotide 0.875), which are the J5 targets. This is also the J5 prototype-gate baseline (same method). Raw table: `docs/bench-t13-j4.txt`.
