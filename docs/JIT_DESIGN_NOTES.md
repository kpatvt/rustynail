# JIT design notes: what LuaJIT teaches us (Phase 10, J0 item 6)

Status: study notes, 2026-10-28. Inputs: `docs/JIT_PLAN.md` (v2), `CLAUDE.md`, the LuaJIT source and the
installed `/usr/bin/luajit`. Nothing here changes the plan by itself; section 12 lists proposed plan changes
for the orchestrator to accept or reject.

## 0. Method, rules, evidence tags

- Source studied: `github.com/LuaJIT/LuaJIT`, shallow clone at commit `a4c0645` (2026 head of the v2.1 branch),
  in `ref/luajit/` (ignored by the existing `/ref` rule in `.gitignore`; `git check-ignore ref/luajit` confirms).
  The installed binary is `LuaJIT 2.1.1792588164` (SSE3, SSE4.1, BMI2; all optimisations on).
- License rule from the plan: LuaJIT is MIT. This is **study and reimplement only**: these notes contain
  paraphrases, short identifiers and file/function names, no code and no copied tables. Constants quoted
  (thresholds, limits) are facts about behaviour, not code.
- Every claim carries a tag:
  - **[V]** verified by reading the source (file and function named).
  - **[M]** measured here with the installed `luajit` (commands in section 11).
  - **[R]** recalled from memory or general knowledge of LuaJIT's history; not verified in this checkout.
  - **[D]** derived by me for rustynail (a proposal, not a fact about LuaJIT).
- Honest scope: I read `lj_trace.c`, `lj_snap.c`, `lj_jit.h`, `lj_ir.h`, `lj_gc.c` (trace parts),
  `lj_dispatch.c/h`, `lj_mcode.c` (protection), `lj_opt_loop.c` (header, PHI logic), `lj_opt_mem.c` (alias
  analysis and forwarding core), `lj_opt_fold.c` (engine description and CSE only), `lj_opt_narrow.c` and
  `lj_opt_sink.c` (headers), `lj_record.c` (loops, calls, returns, root setup, table index, dispatch switch),
  `lj_ffrecord.c` (NYI and stitch), `lj_asm.c` (register allocator core, snapshot allocation, assembly
  driver) and from `lj_asm_x86.h` only the exit stubs, GC check, barriers, SLOAD and HREFK. Also
  `vm_x64.dasc` hot-count macros and the exit handler. Not read in depth: most of `lj_asm_x86.h`, most of
  `lj_opt_fold.c`'s 2600 lines of rules, `lj_opt_split.c` (32-bit only), FFI/cdata recording, the profiler,
  `lj_crecord.c`. There is no NYI document in the checkout (`doc/ext_jit.html` only documents the `jit.*`
  API); the NYI map below comes from the trace-error list and the recorder.

## 1. How LuaJIT decides what to compile, and what we copy (answers (a))

### 1.1 Mechanism [V unless tagged]

- **Counters.** A table of 64 `u16` counters indexed by `(pc >> 2) & 63` (`lj_dispatch.h`:
  `hotcount_get`). It is shared by every bytecode in every prototype: collisions merge counts. The cost of a
  collision is a spurious recording attempt, which is cheap to abort. The source comment says the counter
  overhead is negligible even at maximum penalty.
- **What is counted.** Loop instructions (`FORL`, `ITERL`, `LOOP`; `ITERN` is special) decrement by 2, function
  headers (`FUNCF`) by 1 (`HOTCOUNT_LOOP`/`HOTCOUNT_CALL`). Vararg function headers (`FUNCV`) are **not** counted
  (comment in the `lj_record.c` dispatch: "no hotcall counting for vararg funcs").
  The counters are armed at `hotloop * 2 - 1` with `hotloop = 56`, and the check is "borrow on subtract", so a
  loop triggers on about the 56th back edge and a function on about the 112th call. After a trigger the
  counter is reset to `hotloop * 2` (`lj_trace_hot`).
- **Counting is done by swapping the bytecode implementations.** While JIT is off or a trace is being
  recorded, the dispatch table points at non-counting variants (`IFORL`, `ILOOP`, ...). No per-instruction
  branch is added when the JIT is off. (`lj_dispatch_update`.)
- **Where traces start.** Root traces start at a hot loop instruction, a hot function header, or a hot
  return (down-recursion / return-to-lower-frame). Side traces start at a hot exit: each snapshot has a
  counter, compared with `hotexit = 10` (`trace_hotside`); `tryside = 4` further failed attempts, or
  `maxside = 100` side traces on one root, stop further attempts (`lj_record_setup`, "sidecheck").
- **Entering a trace** is done by **patching the bytecode** of the loop head to `JLOOP`/`JFORL` with the trace
  number (`trace_stop`). Effectively this is OSR at a loop head: the interpreter state is canonical, and the
  trace's entry loads the live slots and type-guards them (`SLOAD` with the typecheck flag).
- **Penalties and blacklisting** (`penalty_pc`, `blacklist_pc`). A failed root recording is remembered in a
  64-slot round-robin cache keyed by the loop pc. First failure sets a penalty of 72, every repeat doubles it
  plus 4 random bits, and the penalty is written back as the new counter value (so retrying waits
  `penalty / 2` iterations, 36 up to about 30,000). Above 60,000 the instruction is blacklisted by patching the
  opcode to its non-counting `I*` form and flagging the prototype. That is about 11 failures. Only 64 pcs are
  remembered, so with many failing loops the history is evicted and penalties restart. An immediate-retry
  result (`TRERR_RETRY`) re-arms the counter to 1. A side-exit failure is blacklisted by self-linking the
  parent exit instead.
- **Limits** (`lj_jit.h` parameter table): traces in cache 1000, IR instructions recorded per trace 4000, IR
  constants 500, snapshots per trace 500, side traces per root 100, trace slots 250 (`LJ_MAX_JSLOTS`),
  PHIs 64, exit stubs 512 (16 groups of 32). Machine-code areas are 64 KB chunks up to 2 MB in total. When
  the trace table or the code area is full, **all traces are flushed** (`lj_trace_flushall`).
- **Unrolling limits** that act as "abort reasons": loop unroll 15 (side traces), call unroll 3, recursion
  unroll 2, unstable-loop unroll 4.

### 1.2 Abort reasons (from `lj_traceerr.h`, and what we saw)

The full list has 35 entries. The ones that matter as a map of "hard" constructs:
leaving a loop in a root trace (the loop exited, or the trace hit the exit before looping back), inner loop in a
root trace (let the inner loop trace first), loop/call unroll limit, down-recursion, "NYI: bytecode %s"
(only `UCLO` and `FNEW` are unrecorded bytecodes in this checkout [V], dispatch switch in `lj_record.c`),
"NYI: return to lower frame", NYI fast-function variants, store with nil/NaN key, missing metamethod, looping
index lookup, persistent type instability, too many PHIs, trace too long/short/deep, too many snapshots,
code-memory exhaustion, too many spill slots, "inconsistent register allocation" (an internal-consistency
abort) and "cannot assemble IR instruction". [M] On our benchmark programs the observed reasons were almost
only: leaving loop in root trace, call unroll limit reached, inner loop in root trace, and NYI FNEW (section 11).

### 1.3 What we copy, tier by tier [D]

Tier 1 (templates + OSR):
- Counters **per prototype** (fields next to the proto's other JIT state), not a shared hashed table: we have
  a proto object, and exactness helps tests. Two counters, back-edge and call, with the same decrement-to-
  underflow idea so the interpreter hot path does one subtract and one branch.
- Thresholds are **not** LuaJIT's. LuaJIT's 56/112 are tuned for a recorder plus optimiser plus assembler
  (tens to hundreds of microseconds per trace). A template compiler is far cheaper, so tier-1 thresholds
  should be lower and measured (J1); the plan already says "thresholds by measurement".
- Keep LuaJIT's structure of **entry points only at loop heads, function entries and return sites**; ours
  adds resume sites (E1). The entry "type check of live slots" does not apply to templates (every template
  guards its own operands).
- Blacklisting must **not patch bytecode** (we keep bytecode intact for `string.dump`/`getobjname`, plan J1b
  note). Use a per-proto side table: state per loop head (never compiled / compiled / failed N times / off).
- Failure feedback loop: copy the exponential backoff with randomness and a hard cap, but key it by (proto,
  loop head) in a small per-proto map instead of a 64-entry global cache that evicts history.

Tier 1.5 and 2:
- The big thing LuaJIT gets for free and we do not: **its recorder is also its type profiler.** At trigger
  time it records what actually executed, so every guard and every unboxed value comes from observed types.
  Our tier 1.5 plan says "local type inference from control variables, constants and guards"; that alone will
  leave most loop live-ins (`local x = t.field` before the loop, function parameters) as unknown. Proposal:
  at the moment a back-edge counter fires, the compiler **reads the tags of the live-in registers from the
  frame** (this is the information a LuaJIT `SLOAD` guard is built from), specialises the loop on them with
  entry guards, and infers the types of registers assigned inside the loop by a bytecode fixpoint (anything
  not provably int/float/bool/table/string becomes "boxed, lives in its slot"). On entry-guard failure: run in
  the interpreter and apply the backoff above. Plan change P1 in section 12.
- Tier 1.5 failure counters: count exits per exit site (LuaJIT counts per snapshot); an exit taken more than
  a threshold (LuaJIT: 10) means the specialisation is wrong. LuaJIT answers with a side trace; tier 1.5 has no
  side traces, so the answer is "recompile the loop with that slot generalised to boxed" once, then
  blacklist (with the shared backoff).
- Tier 2 (traces): the full LuaJIT hot-exit/side-trace/linking machinery would apply; the plan keeps it out of
  scope. Note LuaJIT keeps loops in **one root trace per loop with inner loops handled by separate traces
  linked together** ("inner loop in root trace" abort = wait for the inner loop to compile first). For any
  nested loop in tier 1.5, compile **innermost loop first**, then the outer loop with the inner as a call-like
  region (OSR entry at the inner head), the same ordering rule.

## 2. IR design (answers (b))

### 2.1 LuaJIT's IR [V: `lj_ir.h`]

- **Layout.** A flat array of 8-byte instructions: two 16-bit operand references, an 8-bit type byte, an 8-bit
  opcode, and two bytes that the assembler later reuses for register/spill (and the CSE chain link before
  that). References are biased: constants have references below a fixed bias and are allocated downwards;
  instructions above it, so "defined before" is a plain integer compare. 64-bit constants take two slots.
- **Types** are in the instruction: nil, false, true, light userdata, string, table, function, thread, udata,
  cdata, proto, pointers (32/64), number (double), int (plus i8/u8/i16/u16/u32/i64/u64 for the FFI). Two flag
  bits live in the type byte: **guard** and **phi**. Integers exist in the IR only as a narrowing result and
  never escape to a stack slot without conversion (in LuaJIT's non-DUALNUM build).
- **Instruction families** (the families are the useful thing to take; there are about 100 opcodes):
  1. Guarded assertions: `LT GE LE GT`, unsigned variants, `EQ NE`, `ABC` (array bounds), `RETF` (return-frame
     guard). The opcodes are laid out so negation and ordered/unordered flips are xor-with-constant on the
     opcode (static asserts check this). They have **no result**: they exist to be guards, and the assembler
     fuses them with the preceding compare into compare + conditional jump to an exit stub.
  2. Constants: `KPRI KINT KGC KPTR KKPTR KNULL KNUM KINT64 KSLOT` (KSLOT = constant key plus predicted hash
     node index).
  3. Bit ops, arithmetic (`ADD SUB MUL DIV MOD POW NEG ABS MIN MAX LDEXP FPMATH`), overflow-checking variants
     (`ADDOV SUBOV MULOV`: used when integers are narrowed; Lua 5.3+ integers wrap, so we would not need them).
  4. Memory references vs loads/stores: `AREF HREF HREFK UREFO UREFC FREF STRREF` produce addresses;
     `ALOAD HLOAD ULOAD FLOAD XLOAD SLOAD VLOAD ALEN` load; `ASTORE HSTORE USTORE FSTORE XSTORE` store.
     Splitting address computation from access is what makes alias analysis and CSE of addresses simple.
     `SLOAD` reads a Lua stack slot (with flags: type check, convert, read-only, inherited, parent).
  5. Allocation: `SNEW TNEW TDUP CNEW CNEWI`; barriers: `TBAR OBAR XBAR`.
  6. Conversions: `CONV TOBIT TOSTR STRTO`.
  7. Calls: `CALLN` (pure), `CALLA` (allocating), `CALLL` (load: reads memory), `CALLS` (store: has side effects),
     `CARG` argument chain. The class letter drives what the optimiser may move across the call.
  8. Control/misc: `LOOP` (marks the start of the loop body), `PHI`, `RENAME`, `NOP`, `BASE`, `GCSTEP`, `USE`.
- **Per-opcode mode bytes** (N normal, C commutative, R reference, L load, S store, A allocation, W weak) control
  CSE, DCE, and ordering. The assembler skips any instruction with no register use and no side effect (dead
  code elimination is done during backward assembly, not as a pass).
- **Guards are not separate type-test instructions**: a load that must have a given type is a load with the
  guard flag and the expected type (`SLOAD`/`ALOAD`/`HLOAD` with type). The tag compare is generated inside
  the load. [V: `asm_sload`]
- **Fold engine.** Each new instruction is looked up in a hash of rules keyed by (opcode, opcode of left
  operand, opcode of right operand) with wildcards, most-specific first; a rule can return "no change, try a
  less specific rule", "modified in place, retry", a constant, one of the operands, or "emit via CSE".
  Falling through reaches CSE, which walks a per-opcode chain (each instruction stores the previous
  instruction of the same opcode) and stops at the larger operand reference (an instruction cannot be
  defined before its operands). Load/store forwarding uses the same chains of stores and loads
  (`lj_opt_mem.c`). The result is that **every optimisation is a backward look at already-emitted IR**, which
  is why the loop optimisation can reuse them unchanged (section 5).
- **Code size/cost.** 4000-instruction cap, 8 bytes each; the whole IR of a trace is a few tens of KB.

### 2.2 Proposal: a much smaller IR for tier 1.5 [D]

Why not copy LuaJIT's shape: it is SSA over a straight-line trace, with inlined frames and PHIs. Tier 1.5 is a
**method-level loop region with real control flow** (if/else inside the loop, early `break`), frames identical
to the interpreter's, and exits that go to the interpreter at a bytecode pc. So:

- **Form:** basic blocks of three-address instructions over **mutable virtual registers** (not SSA). Liveness
  and intervals computed over the block order (section 4). No fold-engine rule table; a few hand-written
  peepholes and local value numbering inside a block.
- **Value types (static per vreg):** `I` (i64), `F` (f64), `B` (bool/flag, only for conditions), `Tab` (pointer to
  a table header, known to be a table), `Str` (pointer to a string header), `Any` (a boxed `Value` that lives
  only in a Lua stack slot, never in a machine register). Anything else (functions, userdata, threads, nil,
  tables of unknown shape) is `Any`, or `Ptr` as an opaque collectable pointer when only passed to helpers.
- **Mapping to our `Value`** (16 bytes, tag word `u64` of which only the low byte is ever written; payload at
  offset 8; loads/stores are split into a byte store for the tag and a qword store for the payload because
  16-byte loads after split stores stall store forwarding, `value.rs` `Value::load/store`):
  - Type guard = `cmp byte [slot], tag_imm8` + `jne exit`. The tag byte includes the variant bits and the
    collectable bit (`LUA_VNUMINT`, `LUA_VNUMFLT`, `ctb(LUA_VTABLE)`, `LUA_VSHRSTR` ...). The guard compares the
    whole byte, never the type nibble only (otherwise int vs float or short vs long string are confused).
  - Unbox = `mov r64, [slot+8]` (I), `movsd xmm, [slot+8]` (F), `mov r64, [slot+8]` (Tab/Str).
  - Box (write-back) = byte store of the tag, then qword store of the payload, never a 16-byte combined
    store. A write-back can skip the tag store only when the slot is already known to hold that tag **and**
    nothing could have changed it (no call or hook point since): LuaJIT's analogue is the "no need to
    restore" flag on unmodified slots in snapshots [V `snapshot_slots`].
- **Ops (about 45):**
  - Constants and moves: `ICONST FCONST KLOAD(proto k index, boxed)`, `MOV`.
  - Slot access: `SLOT_TAG_GUARD(slot, tag, exit)`, `SLOT_LOAD_I/F/P(slot)` (after a guard), `SLOT_STORE_I/F/P/ANY
    (slot, v)` (tag+payload), `SLOT_COPY(dst, src)` (boxed move, copies both words).
  - Integer: `IADD ISUB IMUL INEG IAND IOR IXOR ISHL ISHR ISAR INOT` (wrapping; shift counts as the manual
    defines), `IDIV IMOD` (floor semantics; guard divisor != 0 and != -1/min-int edge via exit),
    `ICMP(cc)`. Float: `FADD FSUB FMUL FDIV FNEG FSQRT FFLOOR` (only when the call site is a known `math.*`
    builtin and the guard on the callee identity passed), `FCMP(cc)` with explicit NaN behaviour (a compare
    with NaN is false for everything except `~=`; the Lua `a < b` branch inversion must not be done by
    negating the condition code). Conversions: `I2F`, `F2I_EXACT(exit)` (float with an exact integer value
    -> int, else exit; used for float keys, `//` with ints, for-loop prep), `ADD_FLOAT_INT_MIX` is just `I2F`
    then `FADD`.
  - Control: `JMP BR(cond)`, `LOOP_BACKEDGE(trap/INTERRUPT poll, hook-count decrement)`, `EXIT(exit_id)`.
  - Tables (all guarded, all exit on any miss): `TAB_ARRAY_GET(tab, idx_I, exit)` (bounds + non-empty slot
    check; yields a tag + payload pair, then guarded to a type), `TAB_ARRAY_SET`, `TAB_HASH_GETSTR(tab,
    kidx, exit)` with the speculated node index (section 6), `TAB_HASH_SETSTR` (only when the key is present
    and the old value is non-nil, so no insertion, no metamethod check beyond the table flags), `TAB_LEN`
    (only when the array part is full to the limit, else exit), `TAB_META_NULL_GUARD`.
  - GC: `BARRIER_BACK(tab, value_tag, value_payload)` (inline black test, cold call), `BARRIER_FWD_UV(uv, value)`,
    `GC_POLL(exit)` (debt compare, call, exit if the collector needs the interpreter, section 7).
  - Calls to runtime helpers: `CALL_HELPER(fn, args, class)` with a class tag `Pure`, `MayAlloc`, `MayExit`;
    helpers are `extern "C"` and infallible (return a status; errors only by exiting, P1).
  - Exits: each `EXIT` or guard refers to an **exit map**: list of `(slot, vreg, rep)` entries for slots whose
    vreg value is newer than the slot content, plus the target pc, plus "kind" (re-execute pc vs continue
    after pc).
- **What the IR deliberately lacks:** SSA/PHIs, frames, continuation frames, allocation instructions (`NEWTABLE`
  stays a helper call), upvalue arithmetic across calls, string operations, and any op that can run Lua code.
  A function call, `CONCAT`, `CLOSURE`, `VARARG`, `TFORCALL`, a metamethod-capable op whose guard failed: all
  end the compiled region or exit.
- **Emulator** (plan section 5): this op list is small enough to interpret in Rust under Miri, as the plan
  wants; the x86 emitter compares against it. That argues for keeping the IR typed and total (no op whose
  semantics are "whatever the emitter does").

## 3. Snapshots and exits (answers (c))

### 3.1 LuaJIT [V: `lj_snap.c`, `lj_trace.c`, `vm_x64.dasc`]

- **What a snapshot is.** A record taken before a guard that can fail, with: the bytecode pc to resume at,
  the number of valid slots, the maximum frame extent (`topslot`), and a list of (slot, IR reference) pairs.
  It is **sparse**: slots whose current IR value is just the unmodified `SLOAD` of that same slot are omitted
  (or marked "no restore"); dead slots are removed by a bytecode def-use scan that follows the code after the
  pc (`snap_usedef`), treating open upvalues as live. After the slot list come **frame links**: one entry per
  inlined Lua frame (its return pc) and per continuation/C frame. Snapshots are merged when there is no
  instruction between them or when no guard was emitted since the previous one (`lj_snap_add`).
- **How a guard exits.** A guard compiles to compare + conditional jump to **an exit stub**. Stubs are
  generated in groups of 32 (`push exit-number; jmp common`); the common handler pushes all GPRs and all 16
  XMM registers into an `ExitState` on the machine stack, stores `jit_base` (the current Lua base), clears
  `jit_base` ("not on trace"), then calls the C function `lj_trace_exit` (`vm_exit_handler`).
- **How state is restored.** `lj_trace_exit` calls `lj_snap_restore` **inside a protected call** (restoration
  can grow the stack and so can throw). Restore walks the snapshot entries and for each slot reads the value
  from the saved register, or a spill slot, or a constant, or re-creates a sunk allocation, converting
  unboxed ints/doubles back to Lua values; rebuilds frame links on the Lua stack; sets the interpreter's
  saved pc; computes `L->top`; returns the pc to the interpreter dispatch (`vm_exit_interp`).
- **The saved pc is "pc + 1" for ordinary instructions** ("to get correct error messages"; except returns and
  tail calls, where pc+1 may be out of range). The interpreter is re-entered at the snapshot pc, while the
  saved pc used for error positions is the next one. [V `lj_snap_restore`]
- **Exit counting and side traces:** `lj_trace_exit` bumps the snapshot's counter; at 10 it starts recording a
  side trace; once a side trace is linked, the parent's exit jump is patched to jump straight to it
  (`lj_asm_patchexit`).
- **GC and finalizers interplay at exit:** if the collector is in atomic/finalize state when the trace exits,
  `lj_trace_exit` drives a GC step (the trace left precisely because of this).
- **Exit consistency rule.** All guards that share a snapshot must exit with the **same** register mapping,
  because the stub only knows the exit number. If a register was renamed after an earlier guard of the same
  snapshot, the assembler forces a spill slot instead (`asm_snap_checkrename`). [V]
- **Snapshot values are "weak" in register allocation:** a value that is referenced only by a snapshot gets a
  register only if one is free, and is the first thing evicted (it then lives in a spill slot). Hot values do
  not lose registers to exit state. [V `asm_snap_alloc1`, `ra_weak`]

### 3.2 Mapping to our frame/`CallInfo` model and E1-E4 [D]

- Our frames are the interpreter's own: same stack slots, `CallInfo`, `savedpc`, upvalue objects. So a
  tier-1/1.5 exit never rebuilds frames. The LuaJIT frame-link part of a snapshot has **no counterpart**; what
  remains is the slot map. That is why tier 1.5 exits can be simple (section 2.2 exit map).
- **E3 (slow paths exit before side effects)** corresponds to LuaJIT's discipline "a snapshot is taken before
  the guard, so the state at the exit is the state before the guarded operation". LuaJIT's guarantee is weaker
  (guards are moved and merged); we want the strong form: an exit for bytecode `pc` has the architectural state
  of "just before executing `pc`", so the interpreter re-executes the instruction.
- **E4 (`savedpc` exactness).** The interpreter decodes `savedpc-1` (and `-2` for MMBIN) after the fact. For
  an exit-before-execution at instruction `P`, set `ci.savedpc` so that the interpreter restarts and fetches
  `P` (that is `savedpc = P`, in our convention the pointer to the *next* instruction to fetch), not `P + 1`.
  LuaJIT stores `P + 1` only because its interpreter entry is separate from the error-position pc; our `'frame`
  loop has one `savedpc`. The J0 design doc must state this per exit kind (re-execute `P`, continue after `P`
  for a half-done instruction, return-site exits). Also every exit must leave `top` as the interpreter expects
  at `P` (fixed `ci.top` for ordinary instructions; the dynamic top for the `*M`-style instructions with
  variable results, like LuaJIT's special-casing of `CALLM/RETM/TSETM` in `lj_snap_restore`).
- **E1 (single entry mechanism).** LuaJIT enters traces via patched bytecode; we use a `pc -> native` table.
  Exits correspond to LuaJIT's "return to interpreter dispatch with pc": we return to the `'frame` loop with
  `ci.savedpc` set. Entry after a call returns to a compiled caller uses the same table (LuaJIT instead
  inlines the callee into the caller's trace and guards the return with `RETF`).
- **Restore mechanism choice.** Two options, to be decided in `JIT_DESIGN.md`:
  1. Per-exit inline write-back code (fast exits, big code, hard to test).
  2. LuaJIT's way: one shared trampoline saves all registers, calls a Rust `extern "C"` restore function
     that interprets a **table-driven exit map** (slot, location class, tag). Slower per exit (about a
     function call plus a loop over live slots; LuaJIT accepts this, [R] reported at hundreds of cycles) but
     the restore logic is then plain Rust that Miri and the emulator test directly, and code size per guard
     is two instructions. Recommended for tier 1.5; tier 1 per-instruction exits (nothing live in registers)
     are just "store savedpc, jump to common exit".
- **Stack growth in restore.** LuaJIT must handle it because inlined frames may need slots that were never
  checked. Ours cannot grow: frames have `maxstacksize` slots checked at call time. The exit map therefore
  never needs a grow check [D, relies on E2 flat frames].
- **Dead-slot pruning is a GC-safety feature, not only an optimisation.** If an exit writes a stale register
  value into a slot that bytecode no longer reads, and that value is a collectable pointer whose object was
  freed since (no GC root held it), the interpreter's next GC would trace a dangling pointer. LuaJIT prunes by
  a forward def-use scan; we must either prune the same way or guarantee that **no collectable vreg value is
  ever newer than its slot** (rule G in section 7, which makes pruning unnecessary for pointers).

## 4. Register allocation (answers (d))

### 4.1 LuaJIT [V: `lj_asm.c`]

- **Reverse linear scan on straight-line SSA.** The assembler visits IR instructions from last to first and
  emits machine code from the end of the buffer backwards. A register is allocated to a value when its first
  *use* is seen (which in backward order is the last use); it is freed when the defining instruction is
  reached. Live ranges therefore need no explicit liveness analysis. Instruction fusion (memory operands,
  address modes) is decided at the same time because the consumer is seen before the producer.
- **Spilling.** When no register is free the allocator evicts the register whose value has the lowest
  "cost", where the cost is the defining reference number (the earliest definition = longest remaining range
  in backward order = the Belady-like victim) plus a large weight for PHI values so loop-carried values stay.
  Evicted values get a spill slot (allocated lazily; 64-bit values take two 32-bit slots; more than 256 slots
  aborts the trace), and the spill store is emitted at the definition. Constants and bases are **rematerialised**
  instead of spilled. Calls evict the caller-saved set.
- **Hints.** A value that is the left operand of a two-address destructive instruction prefers the destination
  register of that instruction (`ra_left`, `ra_dest`), a PHI's two ends hint each other, and values defined
  before the loop (invariant) prefer registers that the loop body does not modify (`modset`), to avoid
  shuffles at the back edge.
- **Loop back edge.** PHIs need a register shuffle (`asm_phi_shuffle`, parallel-move resolution with cycle
  breaking) and may need spill copies; two abort reasons exist for this ("PHI shuffling too complex", "register
  coalescing too complex") [V in the error list]. This is the most intricate part of the allocator.
- **Debug aids:** a compile-time `LUAJIT_RANDOM_RA` mode randomises register choices to flush allocator bugs
  [V seen in `lj_asm_trace`]; an assert pass checks that all registers are free at the trace start
  ("inconsistent register allocation" abort otherwise).
- **x86-64 specifics:** all XMM registers are caller-saved in the SysV ABI, so any float value live across a
  helper call is spilled and reloaded.

### 4.2 Proposal: forward linear scan for loop bodies [D]

- Input: the tier-1.5 IR of one loop region in reverse-post-order block order (blocks in bytecode order; loop
  body contiguous). Compute **live intervals** by a backward dataflow over blocks (bit sets per vreg;
  region sizes are small, capped), then extend any vreg live at the back edge to cover the whole loop
  (`[loop_start, loop_end]`). This is where we pay for not having traces: LuaJIT never computes liveness.
- Poletto-style allocation: sort intervals by start, keep an active list, expire by end; when out of
  registers spill the interval with the **furthest end** (or, among equals, one that is not loop-carried and
  not used inside the innermost loop). No interval splitting, no second chance: a spilled vreg stays in memory
  for its whole life. The memory home of a vreg that corresponds to a Lua local is **its Lua stack slot**
  (box/unbox on access); temporaries get machine-stack slots.
- **Loop-carried vregs get one fixed location for the entire loop**, so there is no PHI shuffle and no parallel
  move at the back edge. If that costs a spill the loop is slower, not wrong; we accept this to avoid the most
  intricate piece of LuaJIT's allocator.
- Register budget on x86-64 (SysV): 16 GPRs minus `rsp`, `rbp` (frame-pointer chain, plan section 3), the
  `State`, `base` and one scratch pair gives about 10 allocatable for `I`/`Tab`/`Str`; 16 XMM allocatable (all
  caller-saved, so across `CALL_HELPER` they are saved/restored around the call by the emitter or the
  vreg is spilled for the call's duration).
- **Hints:** two-address forms (`add r, r2` destroys its first operand) get destination hints from the first
  source when that source dies there; loop-invariant vregs avoid registers written in the loop.
- **Exit maps and registers:** a vreg needed only by exit maps is not given a register in preference to a
  computational use (LuaJIT's "weak" idea, 3.1); exits read it from its spill/home slot if it was evicted.
  Because exit maps are allocation-time facts, the register mapping at each exit is fixed at the moment the
  guard is emitted and never altered by later renames (the consistency rule in 3.1).
- Test aids to copy: a random-register-choice mode under a debug env var (`RUSTYNAIL_JIT_RANDOM_RA`), and an
  "all registers free at region end" assertion.

## 5. Loop optimisation (answers (e))

### 5.1 LuaJIT [V: header of `lj_opt_loop.c`]

- LuaJIT explicitly rejects classic LICM for dynamic languages: nearly every instruction is control-dependent
  on an earlier guard, so the first non-hoistable guard blocks everything after it.
- Instead it **peels by copy-substitution**: after recording one iteration (the *pre-roll*), the recorded
  instructions are re-emitted through the whole pipeline (fold, CSE, forwarding, narrowing) with operands
  substituted by the already-emitted equivalents. Invariant instructions, including invariant **guards**,
  and loads that were forwarded fold to the pre-roll's results; only variant instructions remain in the
  second copy, the loop body, after a `LOOP` marker. Loop-carried values get **PHIs**, emitted below the body.
- Consequences: type guards on invariants run once; load/store forwarding works across iterations (a
  `self.count = self.count + 1` pattern becomes a register recurrence); PHIs whose operands turn out
  invariant are removed; **type instability** across the back edge aborts or re-unrolls: int vs double mismatch
  is repaired with a conversion, anything else is "persistent type instability", allowed up to 4 extra
  unrolls. A trace is a loop only if it returns to its own start (`LLEAVE` abort otherwise).
- [M] Example (section 11): the mandelbrot inner loop trace has a pre-roll of 18 IR instructions (5
  type-guarded `SLOAD`s, 11 float operations, 2 compare-guards) and, after `LOOP`, a body of 11 instructions
  (9 float operations and 2 compare-guards), then 5 PHIs and some renames. All five `SLOAD` type guards run
  once; the loop-carried floats are PHIs that stay in XMM registers.

### 5.2 What is safe for tier 1.5 [D]

Tier 1.5 is not a trace, so peeling-by-substitution does not apply directly. The subset that is safe and
captures most of the same value:
1. **Entry specialisation** (the equivalent of the pre-roll's `SLOAD` guards): tag-guard each live-in slot once
   in the preheader; inside the loop assume the tag for slots no instruction in the loop (and no callee,
   see 5.3) can write with another type.
2. **Registers across iterations:** `I`/`F` vregs for loop-carried locals with a fixed location (4.2). This is
   the mandelbrot case (floats in registers) and the plan's main float win.
3. **Invariant hoisting limited to guards and unboxes of slots not written in the loop**, and constants.
4. **Guard elimination:** a repeated identical guard on a vreg with no intervening redefinition is dropped
   (dominator-based within the region, simple version: same block or the guard dominates in the CFG).
5. **For-loop induction variable:** the control variables of a numeric `for` are known ints (or floats) from
   the `FORPREP` form, with the iteration count precomputed in 5.4+/5.5 style prep; no overflow check needed
   (Lua integers wrap, and `FORLOOP` counts iterations). No scalar-evolution machinery.
6. **Table-array bounds-check hoisting** (`t[i]`, `i` the induction variable, `t` invariant): allowed **only
   in a "pure" loop**: no calls, no metamethod-capable ops with failed guards, no stores that can insert a key
   into any table (a store with a new key can trigger a rehash, which moves the array part). The same
   classification gives the plan's shadow-execution classes.
Unsafe or not worth it for tier 1.5: allocation sinking, store-to-load forwarding of table fields across
possible aliasing stores (6.2 rules), loop unrolling, hoisting across any `CALL_HELPER` that may allocate or
run Lua code, speculation that needs a snapshot with inlined frames.

### 5.3 Why LuaJIT's invariants are cheaper than ours [V/D]
LuaJIT inlines calls and knows what the callee does (or aborts), so within a trace the only writers to a slot
are visible instructions. In tier 1.5 any call in the loop is opaque. Slots that a callee can modify without
the loop writing them: locals captured by a closure (via the open upvalue, which points **into the stack**),
`debug.setlocal` targets, and (for `Any` slots) everything. So: **a slot captured by any `CLOSURE` in the
function (instack upvalue descriptors) is "escaped" and is never register-cached across a call, a hook point or
a GC point**; other slots are cached normally. LuaJIT's analogue is treating open upvalues as live slots in
snapshot pruning [V `snap_usedef`] and its deliberately simple upvalue alias analysis (only distinct upvalues
of the same function are disambiguated, [V `lj_opt_mem.c` comment]).

## 6. Memory and alias analysis (answers (f))

### 6.1 LuaJIT's rules [V: `lj_opt_mem.c`]

- **Separate store/load chains per access class**: array (`ALOAD/ASTORE`), hash (`HLOAD/HSTORE`), upvalue,
  table header fields (`FLOAD/FSTORE`: array pointer, hash mask, node pointer, metatable, flags) and raw
  memory. Forwarding walks the store chain backwards from a load and asks "same, different, or maybe".
- **Array references** are compared by key arithmetic: different constants never alias; `t[i]` vs `t[i+k]` vs
  `t[i+m]` are disambiguated by offsets from a common base; unrelated index expressions "may alias" if the
  tables may be the same.
- **Hash references** are compared by key: same constant key and same table (or same address) must alias;
  different constant keys never alias; non-constant keys of different IR types never alias.
- **Different tables** may alias unless one of them is a fresh allocation (`TNEW`/`TDUP`) that has not been
  stored anywhere in between (a small escape check: scan for any store whose value operand is that table).
- **A NEWREF (key insertion) or a table clear invalidates forwarding** for any table that may be the same one,
  because inserting may rehash and move array entries ("may end up pointing to the array part", comments in
  `fwd_ahload`). Loads from a freshly allocated, never-written table fold to nil.
- **Table-field loads** are forwarded/CSE'd across anything except an aliasing field store or an insertion;
  `HREFK` speculates the **hash-mask value and the node index** (recorded when the trace was made) and
  guards both; loads then reuse that address.
- **Previous-value analysis** for stores: a store can skip the "key absent / metatable present" slow path when
  an earlier load or store of the same slot proves the old value was non-nil.
- **Dead store elimination** of array/hash/field stores overwritten before any intervening load or possible exit.
- **Metamethod absence flags** on the table are cached; a store with a metamethod-name string key must clear
  the negative cache (the recorder emits an explicit field store for it) [V `lj_record.c` near TBAR].
- Calls: `CALLS`/`CALLL` classes keep loads from being forwarded across them (and the `table.clear`-style call
  is special-cased); `CALLN` is pure.

### 6.2 Rules for tier 1.5 [D]
Define five location classes: **stack slot** (own frame), **table array element**, **table hash value**,
**table header word** (array pointer, array size, node pointer, node mask, flags/metatable), **upvalue**.
- Stack slots of the current frame: tracked exactly, except for escaped slots (5.3).
- Header words and element values may be cached in a vreg only until the first of: any store that can insert a
  key (`SETTABLE/SETFIELD/SETI` whose old value may be nil, or `SETLIST`, `NEWTABLE` of that table), any
  `CALL_HELPER` of class `MayAlloc`/`MayExit`, any call, any GC point (the stack and the tables may be
  rehashed or shrunk, objects freed), any `setmetatable`-capable op. Never across them.
- Two table vregs may alias unless one is the result of `NEWTABLE` in this region with no store of it so far.
- Key aliasing uses **normalised keys**: a float key with an exact integer value is the integer key. Constants
  that differ only as `2` vs `2.0` alias (a real difference from LuaJIT, whose numbers are all doubles).
- 5.4 only: integer-key reads call `alimit_touch`, which mutates the table on read (plan section 3). A read
  fast path that skips it changes later `#t`. For 5.4 builds, tier-1.5 table access is restricted to the cases
  that need no touch, or J2b must do the touch.
- A "pure region" (no calls, no inserts, no metamethods) allows full forwarding and hoisting of header words.
  This is the common case for `sieve`, `matmul`, `fannkuch`, `heapsort` inner loops and is where the table
  win lives; `nbody` (field loads/stores on bodies) needs field forwarding between stores to *different
  constant keys*, which the rules above allow.

## 7. GC and write barriers in JIT code (answers (g))

### 7.1 LuaJIT [V unless tagged]

- **Collector:** incremental mark-sweep, non-moving, non-generational (`lj_gc.c`). Traces are GC objects:
  a trace is marked via its prototype's trace list, traversal marks every `KGC` constant in its IR plus its
  linked traces and its start prototype; machine code is freed when the trace object is swept. If a trace is
  saved during the propagate phase it gets a barrier (`lj_gc_barriertrace`).
- **Table barrier (`TBAR`):** an inline test "table is black" then re-link the table onto the *gray-again*
  list (a **back barrier**, set to gray and revisited in atomic). The recorder emits it only when the stored
  value is a collectable or the key may be (`lj_record.c`: `keybarrier || tref_isgcv(val)`). Storing numbers
  or booleans needs no barrier.
- **Upvalue barrier (`OBAR`):** for closed upvalues, an inline "upvalue black and value white" test, calling
  the runtime barrier function only in the rare case (a forward barrier).
- **Allocation:** allocation calls are `CALLA`; the trace counts allocations and emits one **aggregated GC
  check**: compare total allocated against the GC threshold, and if above, call `lj_gc_step_jit` with the step
  count. If that returns "collector entered atomic or finalize phase", the trace **exits** (so that atomic
  work, finalizers and stack shrinking happen in the interpreter). The check is a guard with a snapshot.
  The step function sets `L->base` from `jit_base` and `L->top` before running. The atomic phase and the
  finalize phase refuse to run while a trace is on the C stack (`jit_base` non-null) and return a large cost
  instead. [V `gc_onestep`, `lj_gc_step_jit`]
- **Stack:** the Lua stack is always scanned in the atomic phase, so stack stores need no barrier. Stack
  *growth* during a trace must rebase `jit_base` (`lj_state.c` stack resize does this) and the **stack is not
  shrunk while a trace of that thread is live** (explicit condition in `lj_state_shrinkstack`).
- **Finalizers never run on a trace.** (`lj_assertG(jit_base == NULL)` in the finalizer caller.)
- **Hooks:** the GC hook and VM-event hook flags block new recordings while a finalizer runs.
- [R] Held-in-register-only objects at the GC check: I did not verify how LuaJIT guarantees that a collectable
  value held only in a register is not freed by the step (the check sits at the start of the trace/loop where
  live data is in stack slots or snapshot-visible, and evicts the scratch set around the call). Our design must
  not rely on this; see rule G.

### 7.2 Our collector and what the JIT must do [D, from `crates/rn-core/src/gc/mod.rs`]

Our collector is Lua 5.5's: incremental **and** generational (ages `G_NEW .. G_OLD`, `G_TOUCHED1/2`),
non-moving, with `barrier` (forward: mark the new target, and in generational mode age the target to
`G_OLD0`), `barrier_back` (turn the black table gray again; if its age is `G_TOUCHED2` just gray it, else link
into the gray-again list; an old table becomes `G_TOUCHED1`), and tables/userdata as the only back-barrier
objects. Both barriers are only needed when `o` is black and `v` is white.

Rules for JIT code:
1. **Emit exactly the interpreter's barriers**, inline only the cheap test and call the same Rust slow path:
   `table.marked & BLACKBIT` and (for the back barrier) `value.marked` white test, then `call barrier_back_slow`
   (cold, `extern "C"`). The slow path contains the generational age bookkeeping; never duplicate it in
   machine code. Same for the upvalue forward barrier.
2. **Skip barriers by static type:** a store of a value statically known `I`, `F` or `B` (or nil) needs none.
   This is the same saving LuaJIT gets and is large for numeric arrays. A store of an `Any`/`Str`/`Tab` value
   needs it (strings are collectable; short strings are not exempt).
3. **Rule G (roots):** the collector's roots are the Lua stack (plus registry etc.), never machine registers
   or the machine stack. Therefore a collectable value is **never kept only in a machine register across a
   GC point**; it is also present in a stack slot (every `Tab`/`Str`/`Any` vreg that outlives its defining
   instruction is stored to its slot at definition, deferred write-back is allowed only for `I`/`F`/`B`).
   A newly allocated object from a helper is stored to its destination slot before the next GC point.
4. **GC points** are exactly the interpreter's `check_gc` sites plus every helper that allocates. At each: set
   `ci.savedpc`, set `top` (exactly, plan section 3), call, then **reload `base`** and every pointer derived
   from it, re-guard assumptions invalidated by finalizers (plan section 3 invalidation list). The stack can
   be shrunk and *moved* at the atomic phase (gc/obj.rs), unlike LuaJIT where stack shrink is skipped for a
   live trace thread. Option for the plan: also skip shrink while compiled frames are live on that thread.
   Caveat: shrink affects `collectgarbage("count")`-visible memory, so that is observable behaviour; measure
   before adopting.
5. **GC step pacing:** do not duplicate LuaJIT's aggregated check; the interpreter's debt check at every
   allocating instruction must stay observable to the GC tests (`collectgarbage("count")` sequences, finalizer
   order). A helper that allocates calls the same `check_gc` logic as the interpreter.
6. **No embedded GC pointers in machine code** other than a prototype's own constants (plan): the compiled code
   is owned by the prototype and freed with it. LuaJIT marks its KGC constants; we avoid the problem by
   loading constants from the proto's `k` array via the closure at each use or at region entry.
7. **Generational-specific risks** not present in LuaJIT: an old table receiving a young value in compiled code
   without the barrier is silent heap corruption that shows up only in a later minor collection. The
   generational GC stress modes (`RUSTYNAIL_GC_STRESS=2` in generational mode) are the test that matters, plus
   a JIT-specific "barrier audit" mode that forces the slow path at every barrier site.

## 8. Calls, returns, varargs, metamethods, coroutines, hooks, errors (answers (h))

| Area | What LuaJIT does | NYI / limits (from source) | Implication for us |
|---|---|---|---|
| Lua calls | Recorded **inline**: caller and callee are one trace; frame depth tracked; frames appear only in snapshots. Return to a frame that is part of the trace is a slot shuffle; return **below** the trace start is guarded by a return-frame check and counted as `retdepth`. Tail calls re-use the frame; they count toward the loop-unroll limit. | Recursion unrolled up to `callunroll` (3) / `recunroll` (2); down-recursion restarts a root trace at the return; "NYI: return to lower frame" in several shapes (return to C frame, return from vararg to lower frame). Trace stack depth is capped at 250 slots. | E2 (flat frames, no native recursion) and tier 1.5 (no inlining) mean none of this applies. Tier 2 should start by **not** inlining calls (exit at calls); inlining needs snapshot frame links and a return guard, i.e. LuaJIT's design in full. |
| Returns | `RET*` recorded; the return pc is checked when it leaves the trace. | as above | Return into a compiled caller goes through the E1 resume table, with no guard needed (the CallInfo already says where we return). |
| `pcall` | A `pcall` of a Lua function is inlined; returning through a pcall frame prepends `true`; an error thrown on trace is "caught" by exiting. | NYI when the frame structure is unusual. | Tier 1/1.5: `pcall` is a native call; exit. Do not special-case. |
| Varargs | Vararg functions are recorded after their header (`rec_func_vararg`); the function header is **not** hot-counted; loops inside them trace normally. Specialisation is on the argument count seen. | "NYI: return of vararg func to lower frame". | `VARARGPREP/VARARG`: plan J2 puts them in tier 1 templates; tier 1.5 regions with varargs in the loop: reject (exit). |
| Metamethods | Looked up at record time; `__index` chains are walked and guarded on the table identity / metatable identity / key absence; arithmetic, comparison and concat metamethods are **calls recorded inline** with **continuation frames** (result-to-register, nop, concat-resume, conditional) in `lj_record_ret`. | "missing metamethod", "looping index lookup", a number of fast-function variants; concat continuations in tail calls are NYI. | E3: exit. A table access whose metatable is non-nil and whose raw lookup misses is always an exit in tier 1/1.5. The continuation-frame machinery is exactly what we avoid; in the interpreter `CIST_MMRET` frames continue it (plan E3). |
| Calls to C / builtins | Common builtins have dedicated recorders (hundreds, `lj_ffrecord.c`), the rest are unsupported and use **trace stitching**: the trace ends before the call with a continuation frame, the interpreter runs the C function, and a new trace starts at the continuation (`recff_nyi`, `recff_stitch`). | Not stitchable: `error`, `debug.sethook`, `jit.flush`. Stitching cannot start a trace at `*M` ops. | Tier 2 equivalent: end the region at a native call, run it in the interpreter, resume at the call-return site via E1 (our resume table is the continuation). |
| Coroutines | I found no recorders for `coroutine.*` in `lj_ffrecord.c` (grep for the name), so they take the generic NYI/stitch path. [V partially; exact classification not verified] | | A yield/resume crosses frames and native stacks; `coro.rs` resume goes `unroll -> finish_op -> executor(ci)`. Compiled code never switches coroutines; resume re-enters through E1. |
| Hooks / count / line | Compiled traces **do not check hooks** unless built with `LUAJIT_ENABLE_CHECKHOOK` (off by default; the comment says checks may be expensive in tight loops). The interpreter dispatch table is switched when hooks are set. A hook set while code is compiled has no effect on that code until it exits. [V `lj_record.c` end of `lj_record_setup`] | Debug hooks are not exact under LuaJIT. | Deliberate difference: we require exactness (C1 block counting + hook mode runs the interpreter). The cost model of LuaJIT's optional hook check (a volatile byte load and compare at trace entry) is a hint for the cost of our trap/INTERRUPT polls. |
| Errors | An error thrown from a C helper called by a trace is unwound by the C++-style unwinder **to the trace's exit stub** (`lj_err.c` personality routine: finds the exit via `lj_trace_unwind`, jumps to the stub with an error code), so the trace state is restored through the normal snapshot path and the error is then rethrown from the interpreter. Requires unwind tables for JIT code. | Not available on every OS/ABI combination; falls back to different behaviour. | Not our approach: P1 says helpers are `extern "C"`, infallible, and abort on panic; errors can only occur after an exit when the interpreter re-executes the instruction. For helpers that may fail (allocation limit), return a failure status and exit at the instruction. |
| Stack overflow | Trace slot cap (250), stack-check guard at trace entry for side traces. | "trace too deep" abort | E2 keeps interpreter frames; each compiled call still does the interpreter's stack check. |
| GC | See section 7. | | |

Invariants E1/E2/E3/P1 survive intact: LuaJIT's solutions for each row above require the machinery we chose
not to build (inlined frames and snapshots for them, unwinding through JIT frames, patched bytecode). The one
place LuaJIT is *simpler* than us: it never needs to run a hook, a count limit or an interrupt inside compiled
code. C1 is entirely our own design problem.

## 9. Mechanism, decision, difference, risk (answers (i))

| # | LuaJIT mechanism | Our design decision | Deliberate difference | Risk |
|---|---|---|---|---|
| 1 | Hashed 64-entry `u16` hot counters, shared by all pcs | Per-proto back-edge and call counters | Exact, no collisions | Counter update in interpreter loop: F1 feature gate, instruction budget 0.5% |
| 2 | Loop counts 2, call counts 1, trigger ~56 iterations / ~112 calls | Thresholds measured per tier; tier 1 lower, tier 1.5 higher | LuaJIT tuned for expensive compile | Wrong thresholds hide gains or add compile cost; measure on short-script suite |
| 3 | Counting via alternate interpreter instruction set (dispatch swap) | Cargo feature `jit`; counters compiled into the one interpreter | No dispatch table swap | Feature-off must stay byte-identical (F1) |
| 4 | Exponential penalty (72, doubling, random bits) and blacklist at 60000 | Same backoff per (proto, loop head), side table | Not patching bytecode; no 64-slot eviction | Thrash if the table is lost on GC of the proto |
| 5 | Blacklisting by rewriting the opcode to a non-counting form | Flag in side table | Bytecode immutable | A stale flag after `setmode`-like events; clear on proto replacement |
| 6 | Entry via patched `JLOOP` (OSR at loop heads) | `pc -> native` resume table (E1) | One mechanism also serves returns and coroutines | Check on `'frame` restart costs interpreter instructions |
| 7 | Recorder doubles as type profiler | At OSR trigger read tags of live-in slots; entry guards | No recording | Entry guards fail when types vary between entries: need backoff and generalisation |
| 8 | Root traces + side traces from hot exits (10 exits) | Tier 1.5: one compile per loop; exits above a threshold trigger one generalising recompile | No linking, no side traces | Exit storm if the generalisation is wrong |
| 9 | Flat SSA IR, 8-byte instructions, 16-bit refs, constants below bias | Mutable-vreg basic-block IR, ~45 ops | Control flow in the IR; smaller | Liveness analysis cost; must stay capped |
| 10 | Types inside instructions; guard is a flag; type check fused in the load | vreg static types `I F B Tab Str Any`; explicit `SLOT_TAG_GUARD` | Not a general type lattice | Guard on the wrong tag byte (variant/collectable bit) |
| 11 | Table-driven fold rules by (op, left-op, right-op) | A few peepholes and local value numbering | No rule engine | Missed folds only, not correctness; keep Lua semantic edge cases (NaN, -0, int/float mix) in tests |
| 12 | CSE by per-opcode chain, stops at max operand ref | LVN within a block | No cross-block CSE | Performance only |
| 13 | `SLOAD` type check at entry; unmodified slots skipped in snapshots | Entry guards in preheader; dirty-slot write-back | | Writing back a stale value into a slot another party changed (debug.setlocal, upvalue) |
| 14 | Sparse snapshot with frame links; merged when no guard between | Exit map per guard: slots only, no frames | Frames are the interpreter's own | Map wrong for one exit class = wrong program state; per-exit-kind tests |
| 15 | Exit stubs in groups of 32, shared handler saves all registers, C restore under protected call | Shared trampoline, Rust table-driven restore | Restore in Rust, testable; no protected call needed (no stack growth) | Slow exits; needs a hot-exit cap (tier 1.5 recompile) |
| 16 | Interpreter resumes with saved pc = pc + 1 | Exit sets `savedpc` to re-execute the instruction (E3/E4) | Single `savedpc` in our loop | Off-by-one in a rare opcode class: per-opcode test is mandatory (plan) |
| 17 | Snapshot slot pruning by forward def-use scan | Rule G (collectables always in slot) plus optional pruning | | Dangling pointer if a stale vreg is written back |
| 18 | Exit consistency: all guards of a snapshot share one register mapping, renames force spills | One exit map per guard, fixed at emission | Larger metadata | Register moved after the map was fixed |
| 19 | Reverse linear scan over SSA, lowest-ref eviction, PHI weight | Forward linear scan over intervals, furthest-end eviction | Needs liveness; no splitting | Spill-heavy loops are slower, never wrong |
| 20 | PHI shuffle with cycle breaking at the back edge | Loop-carried vregs fixed location, no shuffle | Possible extra spills | None for correctness |
| 21 | Weak register refs for snapshot-only values | Exit-only vregs read from home slot after eviction | Same idea | Exit reads a clobbered register if the weak flag is wrong |
| 22 | Loop optimisation by copy-substitution peeling | Preheader hoist of guards/unboxes; registers across iterations | No peeling | Hoisting a guard past a write to the guarded slot: the "pure region" test must be exact |
| 23 | Alias analysis by key arithmetic and key types; fresh allocation escape check | Five location classes, conservative; normalised numeric keys | int/float keys unify (`2` and `2.0`) | Treating `t[2]` and `t[2.0]` as distinct (LuaJIT never had to) |
| 24 | `HREFK`: speculate hash mask + node index, guard both | Same for constant short-string keys; guard key identity at the node | Our node layout (T2) | Node array reallocated by rehash invalidates; header words never cached across inserting stores |
| 25 | `TBAR`: inline black test, back barrier onto gray-again list, only for collectable values | Inline test, call Rust `barrier_back` slow path, skipped for static I/F/B | Generational ages live in the slow path | A missed barrier on an old table = rare heap corruption: generational stress and barrier audit |
| 26 | `OBAR` for closed upvalues | Same condition, Rust forward barrier | | Open vs closed upvalue distinction: copy the interpreter's logic exactly |
| 27 | One aggregated GC check, trace exits if the collector needs atomic/finalize | Exact per-allocation `check_gc` equivalents; reload `base` after | GC tests observe allocation pacing | Pacing drift changes `collectgarbage("count")` output |
| 28 | Stack not shrunk while a trace of the thread is live; `jit_base` rebased on growth | `base` reloaded at every GC point (optionally skip shrink) | Our atomic phase shrinks and moves | Stale `base` after a helper: a plain read-after-free |
| 29 | Atomic phase and finalizers refuse to run on trace | Any GC point that may run finalizers exits to the interpreter | | A finalizer changes a cached slot or table (plan invalidation list) |
| 30 | Traces are GC objects marking their KGC constants | Code owned by proto, no embedded object pointers | Simplest possible ownership | Freed code still running: free only when no compiled frame is active |
| 31 | Calls inlined into traces; return-frame guard | Flat frames, no inlining (E2) | No cross-function optimisation | Call-heavy programs gain little (fib-like 1.15-1.3x per the plan) |
| 32 | Metamethods recorded inline with continuation frames | Exit and let the interpreter run them (E3) | | Frequent metamethod loops (oop-style) stay at interpreter speed: J1b inline caches |
| 33 | Trace stitching around unsupported C functions | Resume table re-entry (E1) after the native call | | Cost of the re-entry check on each return |
| 34 | Hooks not honoured in compiled code (optional check) | Exact C1 block counting; hook mode runs interpreter | Stricter than LuaJIT | Overhead of polls; budget measured |
| 35 | C errors in traces unwound to the exit stub via the unwinder | Infallible `extern "C"` helpers; panic aborts (P1) | No unwind tables for JIT code | A helper that can fail needs a status path |
| 36 | Narrowing doubles to ints with overflow guards; DUALNUM off | Lua 5.3+ ints are a type; no narrowing; `F2I_EXACT` only at key/for-prep sites | No overflow checks | int vs float mixing rules must be exactly the VM's |
| 37 | Allocation sinking (escape analysis for tables) | Not in 1.5 | | Allocation-heavy loops (binary_trees) gain nothing from the JIT |
| 38 | Trace/IR/snapshot/PHI caps; flush all when full | Per-state code cap, per-proto eviction | Never "flush all running code" | Eviction while a compiled frame is live: reference-count active frames |
| 39 | W^X: RW while generating, RX at commit, per-area flips; optional RWX build | RW then RX per compile unit, batched, probe + fallback | Hardened-host fallback (D9) | `mprotect` cost on small functions |
| 40 | Random register allocation mode, assertions, opt-level flags (`-O`) for bisection | Same knobs: `RUSTYNAIL_JIT_RANDOM_RA`, per-optimisation switches | | Switches must be exercised in CI or they rot |

## 10. Ten lessons and pitfalls (answers (j))

Tags: **V** = I saw it in this checkout, **R** = recalled from history/community knowledge, not verified.

1. **Stack movement and shrinking under running JIT code is a first-class hazard.** Evidence **V**: the
   resize routine rebases the saved JIT base; `lj_state_shrinkstack` explicitly refuses to shrink a thread with
   a live trace; GC step functions set `L->base` from the saved JIT base. We have an atomic-phase shrink that
   moves the stack at every GC point (plan section 3). Guard: reload `base` after every GC point; a test mode
   that **forces a stack move at every GC point** (not only on growth) under the `always` mode.
2. **Some collector phases must not run while compiled code is active.** **V**: atomic and finalize phases
   return without work if a trace is on the stack; the finalizer caller asserts it; GC step from a trace
   returns "exit now" when the collector needs the interpreter. We must define which phases/finalizers run
   at a JIT GC point (everything the interpreter's `check_gc` runs, with `top`/`savedpc` exact), and make
   every `__gc` run an exit/invalidation boundary.
3. **Exit state must not depend on which guard fired.** **V**: all guards of a snapshot share one exit number,
   so register renames after an earlier guard force a spill. Our exit maps are per guard, but any shared exit
   stub (merged to save space) must have identical maps. Test: for each exit, compare the full frame after
   exit against the interpreter's frame at the same pc in `verify` mode.
4. **Dead-slot pruning errors turn into GC crashes.** **R** (many snapshot-related GC bugs in LuaJIT's
   history were stale references in unused slots), **V** that the pruning exists and treats open upvalues as
   live. Rule G (7.2) removes the class for us.
5. **Open upvalues alias the stack.** **V** (open upvalues treated as used in `snap_usedef`; upvalue alias
   analysis is deliberately minimal). A closure can write a local of a running frame through its open
   upvalue. Mark captured registers escaped (5.3) and never register-cache them across calls.
6. **Table insertion invalidates everything cached about the table's memory.** **V** (comments in
   `fwd_ahload`, the hash-mask guard in `HREFK`). Rehash moves the array and the nodes; cached `asize`/node
   pointers/bounds checks must be dropped at any key-inserting store, any call, any GC point. A 5.4-specific
   twist: `alimit_touch` changes `#t` on a read.
7. **Type instability across the back edge is the normal failure mode, not an exception.** **V** (the loop
   optimiser repairs int/number mixes and aborts the rest; `instunroll` bounds retries). For tier 1.5: a local
   assigned a different type in the loop must be boxed at the back edge or the loop generalised; add a
   test with a variable that changes type on iteration N for N in {1, 2, 3, 100}.
8. **Negative metamethod caches need invalidation by stores.** **V** (recorder emits a flag store on stores
   with metamethod-name keys). If our tables cache "no `__index`" style flags, an inline store that skips the
   interpreter's flag reset is a silent miscompile. Inline stores must reproduce the interpreter's flag
   invalidation, or only handle keys known not to be metamethod names.
9. **Hot-exit and abort storms need a global brake.** **V** (penalty doubling, blacklisting, self-link
   blacklist for exits, flush on code or trace exhaustion). Without the brake a bad speculation costs a
   compile per few iterations. Make the brake observable: counters by reason in `RUSTYNAIL_JIT_VERBOSE`.
10. **Allocator and optimiser bugs need dedicated stress knobs.** **V** that LuaJIT ships a random-register
    mode, assertion macros inside the assembler (`lj_assertA` family), per-pass switches (`-O` flags: fold,
    cse, dce, fwd, dse, narrow, loop, abc, sink, fuse, fma) usable for bisection, and the `-jdump` dump of
    every stage. **R**: the project's recurring bug classes were register allocation around loop PHIs and
    snapshot restore of sunk allocations, plus rarely-hit trace-exit paths. We should add an "exit
    storm" test mode that takes every guard exit once (force-fail each guard N times in turn).
11. (extra, **R**) **Concurrency of "tracing hooks" and the GC**: LuaJIT blocks recording during finalizers and
    VM-event hooks (**V** flags tested in `lj_trace_hot`). If a compile is triggered from inside a finalizer, a
    hook, or on a coroutine mid-resume, compile state (our synchronous compile on the executing thread, D7)
    must be re-entrancy-safe. Decide: no compile while `gc.stp` has GC stopped internally or while a hook is
    active.
12. (extra, **V**) **Do not trust a unit test of the compiler alone**: LuaJIT's own fast paths (e.g. `HREFK`
    needing the `hmask` guard, the sunk-store restore, `RENAME`) exist because correctness depends on global
    invariants (table layout, GC state). Our equivalent is the layout constants in `rn-core` with `offset_of!`
    tests (plan section 3, Layout).

## 11. Quality oracles on `bench/programs` (answers (k))

### 11.1 Commands

Run under the memory cap/timeout of CLAUDE.md (`systemd-run --user --scope -q -p MemoryMax=8G -p
MemorySwapMax=0 timeout 300 ...`, stdin from `/dev/null`). Replace `PROG` by a file under `bench/programs/`.

| Purpose | Command | What to read |
|---|---|---|
| Which loops/functions compile, which abort and why | `luajit -jv=out.txt PROG` | One line per event: `[TRACE n file:line loop]` root; `[TRACE n (parent/exit) file:line -> m]` side trace linked to `m`; `[TRACE --- file:line -- reason]` abort |
| Full pipeline for the hot trace (bytecode, IR, snapshots, machine code) | `luajit -jdump=tbimsr,out.txt PROG` | IR with registers/spill (`r`), snapshots (`s`), machine code (`m`); append `a` to see aborted traces' IR |
| Exits actually taken, by trace and exit number | `luajit -jdump=x,out.txt PROG` | Count lines per `TRACE n exit k`: hot exits are speculation misses |
| Same plus register contents at each exit | `luajit -jdump=X,out.txt PROG` | verbose; for diagnosing one exit |
| Bytecode of the program (what both VMs run) | `luajit -jbc PROG` | LuaJIT bytecode is 5.1-flavoured, so it is a rough guide only |
| Interpreter vs JIT speed | `luajit -joff PROG` vs `luajit -jon PROG` | single-shot wall time is noisy; context only |
| Contribution of one optimisation (ablation) | `luajit -O-fwd PROG`, `-O-loop`, `-O-sink`, `-O-narrow`, `-O-dse`, `-O-abc`, `-O-fuse` | which optimisation matters on which program |
| Trigger threshold sensitivity | `luajit -Ohotloop=10 -jv=out.txt PROG` | number of traces and timing vs default 56 |
| Engine state | `luajit -e "print(jit.status())"` | confirms features |
| Sampling profiler | `luajit -jp=vl PROG` ([R] from `doc/ext_profiler.html`, not run here) | where time goes in compiled code |

How we use each as an oracle (tier 1.5/2), per program in the numeric subset:
- **Coverage oracle (`-jv`):** the set of loop-head lines LuaJIT roots a trace at is the set of loops our
  tier 1.5 should compile; "should compile, did not" is a bug or an explicit documented rejection. Abort
  reasons tell us which constructs LuaJIT also gives up on (so we need not try).
- **Code-quality oracle (`-jdump=tim` plus `r`):** the loop body IR after the `LOOP` marker is a lower bound
  on work per iteration (guards in the body, loads, arithmetic). Compare our per-iteration instruction count
  (`perf stat` instructions per loop iteration at a fixed seed) with LuaJIT's machine-code line count for
  the same loop; investigate any program where ours exceeds about 2x LuaJIT's body (type guards left in the body
  that LuaJIT hoisted, missing register residency, repeated loads of invariant header words).
- **Exit oracle (`-jdump=x`):** programs where LuaJIT takes many exits (fannkuch: 436 exit events in a
  run, nbody 42, mandelbrot 31) are the ones where speculation matters; use them to test our exit paths.
- **Ablation oracle (`-O-flag`):** tells what to build first. [M] on `nbody` (single runs, noisy):
  `-joff` 706 ms, JIT 84 ms, `-O-fwd` 107 ms (forwarding is worth about 25%), `-O-loop` 82 ms and `-O-sink`
  87 ms (no visible effect). Mandelbrot: 702 ms vs 106 ms. So for float-field programs table-field load/store
  forwarding (our 6.2) matters more than loop peeling.

### 11.2 Programs that run on LuaJIT

LuaJIT parses Lua 5.1 syntax only. [M] **15 of the 25 programs run unmodified**: binary_trees, fannkuch, fib,
gc_churn, json, knucleotide, load_big, mandelbrot, matmul, nbody, queens, sieve, spectral_norm, string_ops,
table_hash. **10 fail to parse** (`unexpected symbol near '/'`, the `//` operator): closures, coroutines,
fasta, heapsort, loops_arith, metamethods, oop_methods, pcall_error, table_array, varargs. Two of these
(`loops_arith`, `heapsort`) are in the plan's numeric gate subset, so J0 item 2 needs 5.1-compatible variants
of at least those two (e.g. `math.floor(a / b)`) before they have a LuaJIT oracle.

### 11.3 Example output summaries [M]

Run on this machine, default options, one run each, `bench/programs` unmodified (stdout discarded):

| Program | Traces | Root loops / side | Aborts and reasons | Wall `-joff` -> `-jon` |
|---|---|---|---|---|
| mandelbrot | 4 | 1 / 3 | 1: leaving loop in root trace (first attempt at the inner loop, then it compiled) | 702 ms -> 106 ms |
| nbody | 5 | 1 / 4 | 2: leaving loop in root trace | 706 ms -> 84 ms |
| fannkuch | 48 | 4 / 44 | 6: leaving loop in root trace (4 plain, 2 with a different note) | not timed |
| fib | 27 | 8 / 19 | 7: all "call unroll limit reached" (recursion) | not timed |
| sieve | 11 | 4 / 7 | 0 | not timed |
| queens | 51 | 2 / 49 | 0 | not timed |
| spectral_norm | 9 | 5 / 4 | 0 | not timed |
| matmul | 15 | 3 / 12 | 0 | not timed |
| binary_trees | 78 | 29 / 49 | 26: 21+5 "call unroll limit reached" | not timed |
| json | 125 | 7 / 118 | 57: 46 "NYI: bytecode FNEW", 5 inner loop in root trace, 6 leaving loop | not timed |

Observations:
- The mandelbrot root trace: the first attempt was aborted (the recorder started at the inner `while` loop and
  the trace left it), then the loop compiled as: pre-roll with 5 type-guarded loads of the float locals, 11
  float operations and 2 compare-guards; after `LOOP` a body of 9 float operations and 2 compare-guards; 5 PHIs;
  every value in an XMM register. The two body guards (`i < 50`, `zr*zr+zi*zi < 4`) are the exits that became
  side traces 2 and 4. About 60 disassembly lines for the root trace. That is the target shape for our tier
  1.5 inner loop: type guards once, floats in registers, two conditional exits.
- `fib` and `binary_trees` show that LuaJIT also fails to trace deep recursion and relies on its fast
  interpreter; this supports the plan's modest expectation for call-heavy programs.
- `json` aborts 46 times on `FNEW`, the closure-creation bytecode, confirming that closure creation is a hard
  construct even there. The plan's J2 treats `CLOSURE` as helper/exit.
- Side traces dominate counts (fannkuch 44 of 48, queens 49 of 51): speculation exits are common even on
  tame programs, which is why exit cost and exit-count policy (section 3.2, 1.3) matter for tier 1.5.

## 12. Proposed changes to `docs/JIT_PLAN.md` (for the orchestrator to decide; not applied)

1. **P1 (J4, section 2 tier 1.5):** replace "local type inference from control variables, constants and
   guards" by "observed live-in types read from the frame at the trigger, plus bytecode fixpoint inference
   inside the loop, with entry guards". This is the information LuaJIT's recorder provides for free and
   is probably the difference between tier 1.5 reaching the 0.30x target or not. Add an entry-guard
   failure/backoff policy to J4 and a "generalise once then blacklist" rule on hot exits (section 1.3).
2. **P2 (J0 item 3, section 5 "small IR"):** specify the IR shape now: basic-block, mutable-vreg, ~45 ops,
   static types `I F B Tab Str Any` (section 2.2). The Miri emulator should interpret exactly this IR and tier 1.5
   should compile it; tier 1 templates can be expressed in the same IR so one emulator serves both.
3. **P3 (J0 item 3 design doc):** decide the exit mechanism: shared trampoline plus Rust table-driven restore
   (LuaJIT style) for tier 1.5; tier-1 exits stay "store savedpc, jump to common exit". State per exit kind
   the `savedpc` value (re-execute `P` means `savedpc = P`; LuaJIT's `pc + 1` convention does **not** transfer)
   and the `top` rule (section 3.2).
4. **P4 (section 3, GC):** add **Rule G**: a collectable value is never held only in a machine register across a
   GC point; deferred write-back only for int/float/bool. Add the barrier rules of 7.2 (inline test + Rust slow
   path; skip for static I/F/B; barrier audit test mode).
5. **P5 (section 3 invalidation sources):** add "locals captured by a closure (open upvalues alias the stack)":
   such registers are escaped and never register-cached across calls, hook points or GC points. The plan lists
   `debug.setlocal`/`upvaluejoin` but not the ordinary closure-assignment case.
6. **P6 (section 5 testing):** add stress modes inspired by LuaJIT: random register allocation, per-pass
   switches for bisection, force-fail-each-guard exit storm, and "force a stack move at every GC point" under
   `always` mode.
7. **P7 (J1/J4 hotness):** keep counters per proto (not LuaJIT's shared hash), keep blacklisting in a side table
   (no bytecode patching), reuse the exponential-backoff-with-cap scheme for failed compiles and entry-guard
   failures. Compile innermost loops first (section 1.3).
8. **P8 (J0 item 2):** LuaJIT cannot run 10 of the 25 programs (all because of `//`); two of them are gate
   programs (`loops_arith`, `heapsort`). The quiet-machine LuaJIT comparison needs variants of those two at
   minimum. The 15 runnable programs include 7 of the 9 numeric-subset programs.
9. **P9 (section 11 of the plan):** source is now cloned (gitignored under the existing `/ref` rule); update the
   text ("Source is not on this machine"). Add the oracle commands of section 11.1 to section 6 (Measurement).
10. **P10 (priorities):** the ablation numbers suggest the order for tier 1.5 work: registers for floats and
    ints first (mandelbrot-style), then table-field/array forwarding in pure regions (nbody, matmul), then
    guard hoisting; peeling and sinking last or never.
11. **P11 (hooks):** record that LuaJIT does **not** make compiled code hook-exact (optional check, off by
    default). Our C1 requirement is therefore stricter than the reference and has no LuaJIT precedent to
    copy; budget design time accordingly.
12. **P12 (optional, GC):** evaluate "skip stack shrink while compiled frames are live on the thread" (LuaJIT
    precedent) as a simplification, after measuring whether it changes observable `collectgarbage("count")`
    sequences.

## 13. What I could not read or verify

- No NYI list document exists in the repository; the NYI map is from the trace-error list and the recorder
  switch, not from LuaJIT's wiki page.
- Not read: most of `lj_asm_x86.h` (3170 lines), most fold rules in `lj_opt_fold.c`, `lj_opt_split.c`,
  `lj_opt_narrow.c` bodies (only the rationale), `lj_opt_sink.c` bodies, FFI recorders, the profiler, `lj_err.c`
  beyond the JIT-unwind routine. The `vm_*.dasc` interpreter was read only at hot-count and exit-handler sites.
- Not verified: the exact cost of a trace exit in cycles; how LuaJIT keeps a collectable value alive that is only
  in a register at its GC check; the classification of `coroutine.*` fast functions; the bug-tracker themes in
  lesson 10 and lesson 4 (recalled).
- The timing numbers are single, unpinned runs for orientation, not benchmarks (the plan's method requires
  cycles medians over N >= 7); trace and abort counts are exact for the runs made but the first-run/abort
  sequence can vary with the string-hash seed.
- I did not read the rustynail code beyond `value.rs` (Value layout), `state.rs` (`CallInfo`) and
  `gc/mod.rs` (barriers); statements about E1-E4 rely on the plan's text, not on a fresh read of
  `vm_unsafe.rs`.
