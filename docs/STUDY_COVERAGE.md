# Study: trace coverage (H8 diagnostics, the tier-1 residue, permute) (2026-11-20, base 9e10538)

Question: why do `apps/raytrace`, `awfy/cd`, `awfy/deltablue`, `awfy/richards`, `awfy/list` (and `awfy/permute`) still spend 17-45 % of
their cycles in tier 1, which heads are refused or abort and why, what is each class worth, and where does `permute` lose against
LuaJIT. Study plus diagnostics: production behaviour is unchanged (checked below); the prototypes quoted are scratch patches or
source rewrites, measured and reverted.

Method. Plain release `rn-cli` (JIT on, traces on, generational), `RUSTYNAIL_SEED=0`, `taskset -c 4`, `perf stat -e
cycles:u,instructions:u`, one run each (instruction counts exact, cycles +-1-3 %). Workloads larger than the defaults:
`raytrace 40/100`, `cd 20/30`, `deltablue 400/600`, `richards 500/800`, `list 7000`, `permute 3000`. Shares come from
`perf record -e cycles:u -F 5000` with `RUSTYNAIL_JIT_PERFMAP=2` joined with the trace log by `scripts/heads-perf.py`
(finer than the in-process sampler, which is bound by the kernel timer tick: about 250 samples per second here).

## 1. The diagnostics (what was added)

All off by default; counters only on the slow paths of the policy (recording start, recording end, flush, prototype free),
nothing on the dispatch path or per executed instruction.

| knob | effect |
|---|---|
| `RUSTYNAIL_JIT_VERBOSE=1` | the old summary plus the **head table** (`[heads]` lines, below) printed when the state closes |
| `RUSTYNAIL_JIT_SAMPLE=on` (or `<hz>`, needs `VERBOSE>=1`) | an `ITIMER_PROF` signal stores the interrupted pc; at close each sample is classified against the code ranges of the installed traces (per head) and of every prototype's tier-1 code; the rest is `runtime` (Rust helpers, GC, compiler). x86-64 / aarch64 Linux. Adds `[heads] samples` and `[heads] proto` lines |
| `RUSTYNAIL_JIT_PERFMAP=2` | like `=1`, plus every installed trace is named `TRACE-<n>` and tier-1 blocks `lua_proto:<line>@<ptr>` in `/tmp/perf-<pid>.map` (`mem.rs`: `PerfMap::names`, `CodeArena::register_range`, `perf_names`) |
| `RUSTYNAIL_JIT_VERBOSE=2` | also: `[trace] <n> side ... head proto:<line> pc <pc>` names the head a side trace belongs to (the join key of `scripts/heads-perf.py`) |

Head table, one row per head (prototype `source:line`, pc) that is not `Traced` or had an abort, refusal or flush:

```
[heads] raytrace.lua:63 pc 11 loop Traced: rec 1 inst 1 side-aborts {TooLong:3,LeaveFrame:2} at {LeaveFrame@L63:43:Tailcall:2,TooLong@L15:1:Getfield[sideins]:2} entries 3844440 (fail 42) trace 15.0%
[heads] richards.lua:101 pc 0 fn Black: rec 6 inst 0 aborts {FnShort:6} at {FnShort@L101:9:Return1:6} entries 0 (fail 0) black@rec#76/8ms(FnShort) trace 0.0%
[heads] list.lua:31 pc 17 cont Black: rec 3 inst 0 aborts {TailCall:2,StartRefused:1} start-refused {ContNotRecursive:1} at {TailCall@L31:22:Tailcall:2} entries 0 (fail 0) black@rec#19/0ms(TailCall)
[heads] samples 759: tier1 48.0% traces 29.8% runtime (helpers, gc, compile, other) 22.3%
[heads] proto raytrace.lua:63: tier1 12.4% traces 27.5%
```

- `<name> pc <pc> <fn|loop|cont> <state>`: kind of head (function entry, loop, continuation after a self-call) and final state.
- `rec` recordings started, `inst` root traces installed, `aborts {reason:n}` root aborts, `side-aborts` aborts of side recordings of this
  head's traces, `start-refused {why:n}` the sub-reason when the recorder refused to start (`ContNotRecursive`, `HookBusy`,
  `NoLanding`, `DeadLabel`, `SeedFrames`, `NoLuaFrame`; `unspecified` if none was set), `flushes {cause:n}` (`Rerec`, `Ineffective`,
  `Dep`, `Dom`, ...), `entries` successful entries (fail: entry failures), `black@rec#<n>/<ms>(<reason>)` when the head was
  blacklisted (the n-th recording of the state, milliseconds since the first event, the reason; `flush` = by a flush rule).
- `at {<reason>@L<line>:<pc>:<op>[limit][record.rs:<n>]:<count>}` the sites of the aborts: the frame the recorder was in (an inlined
  callee has its own line), the pc and opcode; for `TooLong` the limit (`sideins` 1000 IR of a side trace, `ins`, `path`,
  `deepins`, `extent`, `snaps`, `ir`), for `Nyi(op)` the line of `record.rs` that refused (the 60-odd `nyi()` sites were
  indistinguishable before: `Nyi(Call)` at `record.rs:7751` is `math.max(int, float)`, at `10081` it is `table.remove(t, i)`).
- `trace x%` (with the sampler) the samples inside the head's trace family (root and side traces); tier-1 samples are per
  prototype (`[heads] proto` lines), tier 1 has no per-head code.

`scripts/heads-perf.py rep.txt log.txt` joins a `perf report --sort sym` with the verbose-2 log: traces per head, tier 1 per
prototype line, helpers by family (alloc, gc, materialise, table helpers, call glue, lib functions, handlers).
Tests: `crates/rn-cli/tests/jit_heads.rs` (format only: header and row syntax, sampler lines, perf-map levels, abort-site syntax,
off by default). No mutants or knobs were added (registry 732 unchanged).

Unchanged behaviour (instructions at a fixed binary path, base 9e10538 vs this branch, `RUSTYNAIL_SEED=0`): nbody 1000000
2161316025 / 2161318904, fib 32 703563112 / 703566177, binary_trees 14 1563416148 / 1563418496, richards 100 602915610 /
602887585 (differences are timing noise of the GC / hook polls, 3e-6 or less). Note: instruction counts of the allocation-heavy
programs depend on the *path of the binary and the script* (the GC pacing hashes addresses): comparing two binaries at different
paths shows a spurious +0.28 % on `binary_trees`; always copy both binaries to the same path.

## 2. Per-program tables

Weights are % of the program's cycles (perf shares; `T1` = tier-1 code of that prototype, helpers called from tier 1 are in the
`runtime` rows). Classes are defined in section 3.

Totals (large runs): raytrace traces 24 / tier 1 43.5 / runtime 32 (alloc 19.6, gc 3.1, lib 3.1, glue 1.6, mat 1.0); cd 24 / 27 / 49
(materialise 19.7, alloc 10.5, gc 6.3); deltablue 32 / 35 / 33 (alloc 9, lib 5.4, table helpers 3, glue 2.5); richards 74 / 17.5 / 8;
list 55 / 43.5 / 2; permute 99 / 0.1 / 1.

### raytrace (1317 Mcyc for `raytrace 40`)

| head / proto | state, reason | weight | class |
|---|---|---:|---|
| `vec` (L11) T1 | no head (only `TAILCALL`/`SETMETATABLE`), called from tier-1 code | 14.5 | C1 |
| `V:add/sub/mul/dot` (L12-15) T1 | no head (leaf or tail-call-only), called from tier 1 | 9.7 | C1 |
| `trace` (L63) T1 (code outside its two loop roots) | loop head pc 11 `Traced`, side aborts `TooLong[sideins]` 3 (`L15:1`, `L11:0`), `LeaveFrame` 2 (`L63:43 Tailcall`); pc 84 `Traced`, side aborts `InnerLoop` 4, `Nyi(Call)` 2 (`L63:105`, `math.max(int,float)`), `SideShort` 2 | 8.7 | C1, C7 |
| `trace` loops pc 11 / 84 / 62, pc 0 (traces) | traced, 24 % of the cycles in total | 24 | - |
| `render` (L97) T1 | loop heads pc 12, 18 `Warm`/`Black`: `InnerLoop` 8 (`L63:10 Forprep`: the call of `trace` has loops) | 4.4 | C3 |
| `Plane:hit` (L41) T1 | has a `CALL`-free body: no head | 4.0 | C1 |
| `Plane:color/normal`, `Sphere:*` (L47, L48, L16, L23, L36) | `FnShort` 6 -> `Black` (L48), `FnShort` 4 (L16, L23), `ContNotRecursive` refusals on 11 cont heads (weight 0, 117 refused starts) | 2 | C2, C8 |
| helpers: `jit_newtable`, `resize`, `setmetatable` C glue, pool | tier-1 constructors (alloc 19.6 % + glue 1.6) | ~21 | C1 (the constructors run in tier 1) |

Prototype (source rewrite, no recorder change): `max(0, x)` -> `max(0.0, x)`, `min(1, c.x)` -> `min(1.0, ..)` in `raytrace.lua`
(the recorder refuses mixed integer / float `math.min/max`, `record.rs:7751`): **cycles 1306 -> 1166 M (-10.7 %), instructions 5328 ->
4414 M (-17 %)**; the lights loop (pc 84) then runs with 2 more traces and the `Nyi(Call)` side aborts disappear. With
`MAX_SIDE_INS` 1000 -> 4000 the `TooLong[sideins]` aborts disappear but the side recording then dies at the snapshot limit
(`snaps`, 500) and cycles get worse (1244 M with the source rewrite, 1490 M without it): the recording inlines the whole shading section (two light loops unrolled over four
objects, each `hit` a polymorphic callee chain), so the real fix is to end a side recording at the head of a loop of the same frame
that already has a root trace, not to raise limits.

### cd (3000 Mcyc for `cd 20`)

| head / proto | state, reason | weight | class |
|---|---|---:|---|
| materialisation of sunk tables, `exit_entry`, `restore` | not coverage: 3.9 M tables materialised at exits | 20 + 3 | C10 (H5 of STUDY_ALLOC_CALL) |
| `put` (L97, recursive local) T1 | fn head pc 0 `Black` (`FnShort` 4, `L97:13`, `L97:82` Return1), cont heads pc 17 / 48 `Black` after `Rerec` flushes (entry failures 80 / 64) | 9.9 | C2, C9 |
| `recurse` (L239) loop pc 34 | traced (9.4 %); side aborts `Depth` 13 (`L97:16 Call`: `put` is the fifth frame), `LeaveLoop` 1 | 9.4 | C4 |
| `isRed` (L51), `CallSign:compareTo` (L41), `Vector2D:compareTo` (L20), `rotateLeft/flipColors` (L52, L68) T1 | leaf functions called from `put` in tier 1 | 6.1 + 3.2 + 1.4 + 0.8 | C1 |
| `handleNewFrame` (L263) loops pc 63 / 8 / 58 | traced, side aborts `SideShort` 36, `Recursion` 12 (`L97:34`), `LeaveFrame` 2 | 4.7 | C5, C4 |
| `RBT:get` (L122) loop pc 1 | traced, side aborts `LeaveFrame` 8 (`Return1`), `SideShort` 7 | 2.4 | C5 |
| `reduceCollisionSet` (L251) | fn head `Black` (`TooLong` 2 at `L210:82`), loop pc 6 traced, `InnerLoop` 3 | 2.4 + 0.1 | C3 |
| `delete` (L140), `forEach` (L168), `balance` (L73) | `TailCall` abort on cont head, `Recursion` 2, `LeaveFrame` 18 | < 1 | C6, C4 |

### deltablue (1255 Mcyc for `deltablue 400`)

| head / proto | state, reason | weight | class |
|---|---|---:|---|
| `Plan:execute` (L216) loop pc 5 | traced (23.6 %), side aborts `LeaveFrame` 16 (`L216:9 Return0`: the callee variants return into the loop) | 23.6 | C5 |
| `Constraint:satisfy` (L58) T1 | fn head `Warm`: `InnerLoop` 6 (`L303:5 Forprep`) | 7.5 | C3 |
| `Planner:addPropagate` (L267) T1 | fn head `Warm` (`InnerLoop` 6), loop pc 4 `Black` (`InnerLoop` 5, `LeaveFrame` 4) | 4.6 | C3 |
| `BinaryConstraint:output/input/isSatisfied/markInputs/recalculate` (L154-158) T1 | 8 fn heads `Black` by `FnShort` (6 each, `Return0`/`Return1` leaf samples) | 4.3 + 1.7 + 1.5 + 1.4 + 1.2 | C2, C9 |
| `Planner:incrementalAdd` (L223), `makePlan` (L245) T1 | `InnerLoop` 2; `makePlan` heads pc 0 / 11 `Black`: `Nyi(Call)` 2 at `record.rs:10081` (`table.remove(t, i)`: only the one-argument form is recorded) | 1.5 + 0.8 | C3, C7 |
| `Planner:removePropagateFrom` (L280) loops pc 21 / 36 | `Black` after `Ineffective` / `Rerec` flushes, `LeaveLoop` 10, side `TooLong` 4 (`L198:5`) | 2.8 | C9 |
| `addConstraintsConsumingTo` (L303) loop pc 6 / fn 0 | traced; `LeaveLoop`, side `LeaveFrame` 12, `SideShort` 4 | 3.1 | C5 |
| `chainTest` / `projectionTest` (L313, L344), `Depth` side aborts | `Depth@L149:3`, `L86:3`, `L96:5`, `L54:1` (method chains deeper than `maxdepth` 4) | 1.7 | C4 |
| runtime: alloc 9, lib functions (`tinsert`, `tremove`, `checktab`) 5.4, table helpers 3, glue 2.5 | helpers called from tier 1 | ~20 | C1, C7 |

### richards (812 Mcyc for `richards 500`)

| head / proto | state, reason | weight | class |
|---|---|---:|---|
| `Scheduler:schedule` (L83) loop pc 2 | traced (73-78 %), side aborts `InnerLoop` 3 (`L157:21`: the task body has a loop) | 74 | - |
| `Scheduler:queue` (L110), `WorkerTask:run` (L157), `TCB:addInputAndCheckPriority` (L60), `HandlerTask:run` (L172), `TCB:run` (L47) T1 | task bodies reached by tail calls: fn heads `Black` by `FnShort` (L47 `Tailcall`, L60 `Return1`), `FnShort` at `Tailcall` (L172, entries 35216, fail 6250), side `LeaveFrame` 20, side `TailCall` 4 at `L157:37` | 3.2 + 2.6 + 2.3 + 2.3 + 1.3 | C2, C6 |
| `TCB:markAs*` (L42, L43), `Scheduler:holdCurrent/waitCurrent` (L101, L106), `createTask` (L77), `newTCB` (L34) | `FnShort` leaves `Black` | 2.5 | C2, C9 |

### list (2042 Mcyc for `list 7000`)

| head / proto | state, reason | weight | class |
|---|---|---:|---|
| `isShorterThan` (L21) T1 | loop pc 2 `Black` (`LeaveFrame` 6, `Unstable` 3, side `SideShort` 8 at `L21:10 Jmp`); the unrolled copy inside `tail`'s root trace has five frame exits (14-18) whose side recordings die `SideShort` (`L21:10`), so the rest of every loop runs in tier 1 | **34.5** | C5 |
| `tail` (L31) fn pc 0 | traced (50.6 %), cont pc 4 / 12 traced, pc 17 / 22 `Black` by `TailCall` (`L31:22`), `FnShort` 7 | 9 (T1) | C6 |

`sidemin=0` (side traces of fewer than 4 bytecodes allowed): cycles 2042 -> 1990 M (-2.5 %), instructions -6.5 %: the short side traces
are not enough, the loop has to continue in the side trace or link back to the loop head.

### expr_lang (not interesting at this size)

The default run is 25 samples long (55 % runtime: compile and start-up). Its table is the same shape as the others (cont heads
`TailCall` / `ContNotRecursive`, `FnShort` on `L171`, `Recursion` on `L93`, `Nyi(Call)` at `L49:10`); no class has a visible weight.

## 3. Classes of the tier-1 residue

| class | what | evidence (weights above) | prototype / arithmetic | potential |
|---|---|---|---|---|
| **C1 headless callees behind a tier-1 caller** | functions without a `CALL` (leaves, `TAILCALL`-only bodies like `vec`, `V:add`) have no function-entry head; they run in traces only when inlined into a root. When the caller region is in tier 1 (the straight-line code after a loop exit, see C7 / the `TooLong` side aborts) they and the constructors they call run in tier 1: ~100 instructions per call, ~1700 per `vec` | raytrace 14.5 + 9.7 + 4.0 + 8.7 (`trace` own) + ~21 helpers; cd 11.5; deltablue 10 | the fix is the *caller's* trace (C4/C7/C5); tier-1 intrinsics for these callees were done in STUDY_ALLOC_CALL H2 | raytrace -25..40 % if the shading section traced (arithmetic: 43 % tier 1 x 60 % recoverable, minus extra snapshot/exit cost); cd -4..6 %; deltablue -3..5 % |
| **C2 `FnShort` leaf samples blacklist a function-entry head** | a recording started at a function entry samples *one invocation*; a leaf invocation is shorter than `fnmin`, the head goes `Black` after 4-6 samples. K6's `selfref` exception covers recursion through an upvalue only: method recursion (`self:permute`), `obj:method` chains and tail-called bodies are not covered | permute without the `count` store: **1126 -> 411 Mcyc (2.7x) with `fnmin=0`** (the recursion stays in tier 1: tier 1 99.6 % of the samples, fn head `Black` after `FnShort` 6, `L14:23 Return0`); deltablue 8 heads, richards 7, raytrace 1, cd 2 | `fnmin=0` on the awfy set: raytrace +1 %, cd +1.4 %, deltablue -0.9 %, richards +5.5 %, list -0.7 %: the cliff exists, but in these programs the heads are not the bottleneck | cliff programs (any method-recursive function with a cheap leaf path) up to 2.7x; awfy 0..-1 %; effort S |
| **C3 `InnerLoop`** | a recording that reaches a callee with a loop aborts `InnerLoop`; heads of callers of loop functions stay in tier 1 (`innerretry` only retries) | deltablue 7.5 + 4.6 + 1.5; raytrace `render` 4.4; cd `reduceCollisionSet` 2.4 | `innerretry=8`: 0 .. +4 % (no effect: the structure, not the budget) | deltablue -5..8 %, raytrace -3..4 %; L effort (nest through a call: outer trace + linked inner root) |
| **C4 `Depth` / `Recursion` side aborts** | side recordings beyond `maxdepth` 4 (`Depth`) or recursion beyond `unroll` | cd 13 + 2 + 12 (`L97:16`, `L97:34`), deltablue 12 | `maxdepth=8`: cd **+7 %** (3217 vs 3000 M), others 0: deeper inlining is not the cure | small; needs stitching at the limit instead of aborting (`depthstitch` exists for roots) |
| **C5 `SideShort` / `LeaveFrame` side aborts** | the exit of a root (a loop exit inside an inlined frame, a polymorphic callee) has no side trace: the side recording returns into the caller frame (`LeaveFrame`) or ends before `sidemin` (`SideShort`); the exit then lands in tier 1 | **list 34.5 %** (+ 9 `tail` T1), deltablue 216 (23.6 % traced, 16 `LeaveFrame`), cd 4.7 + 2.4 | `sidemin=0`: list -2.5 %, deltablue +2 %, others 0; arithmetic for list: 34.5 % x (1 - 1/2.5) = -20 % | list -15..25 %, deltablue -3..6 %, cd -2..4 %; M-L (frame-exit side traces: record through the return, `RETF`) |
| **C6 root-frame `TAILCALL`** | `TailCall` aborts / `FnShort at Tailcall` for task bodies and `return f(x)` continuations | richards 11.7 (L47, L157, L172, L60, L110 T1), cd cont pc 24 `Black`, list cont pc 17 / 22 `Black` (weight now ~0 after K6), expr_lang | docs/JIT_TRACE.md section 20 measured the H6 ceiling; richards T1 11.7 % x 50 % | richards -4..6 %, others < 2 % (H6, high risk) |
| **C7 library calls with an unsupported argument shape** | `math.min/max` with mixed integer and float (`record.rs:7751`), `table.remove(t, i)` (`10081`), ... : the side recording of the whole region dies `Nyi(Call)` | raytrace: the lights loop and the shading section; deltablue `makePlan` | **raytrace source rewrite: -10.7 % cycles / -17 % instructions**; deltablue (arithmetic): `tremove` + `makePlan` 0.8 + part of 5.4 % lib helpers | raytrace -10..11 %, deltablue -1..2 %; **S** (guard the compare direction; the result type is known per branch) |
| **C8 `ContNotRecursive` refusals** | cont heads of functions whose self-call is no recursion are armed, refused, and blacklisted (raytrace: 11 heads, 117 refused starts) | weight 0 | - | cleanliness: stops 40-700 wasted refusals per program from showing up as `StartRefused`; no cycle gain |
| **C9 blacklist after budget** | the final state of C2/C3/C5 heads (`black@rec#n`): every one of them stopped after 2-6 recordings | all tables | - | the rows say which reason closed each head |
| **C10 allocation, materialisation** | not coverage: cd materialise 19.7 %, raytrace alloc 19.6 % | STUDY_ALLOC_CALL H3 / H5 | - | see there |

## 4. awfy/permute

Workload `permute 3000` (25.98 M `permute` calls, 30.2 M swaps; 8660 calls per round: 5040 leaves, 3620 internal), plain release,
`taskset -c 4`, `RUSTYNAIL_SEED=0`:

| | cycles M | instr M | cyc / call | instr / call | branch miss M |
|---|---:|---:|---:|---:|---:|
| rustynail | 1177 | 7036 | 45 | 271 | 1 |
| rustynail `TRACE=off` (tier 1) | 5315 | 30106 | 205 | 1159 | 10 |
| LuaJIT 2.1 | 285 | 1376 | 11 | 53 | 0 |
| ratio | 4.1x | 5.1x | | | |
| `fib 37` (78.2 M calls) | 1260 | 7781 | 16 | 99.5 | 2 |
| `towers 900` (29.5 M calls) | 664 | 2898 | 22.5 | 98 | 1 |

**Coverage is not the problem here**: 99 % of the cycles are in five traces, tier 1 0.1 %, runtime 0.9 % (the head table shows four heads:
fn pc 0 and cont pc 11 `Traced`, the rest compile noise; 19.4 M exits are cheap linked side-trace transitions: `exit_entry` is 0.2 %).
By trace (`perf record -e instructions:u` with `RUSTYNAIL_JIT_PERFMAP=2`): function-entry root (pc 0) 40 %, loop continuation
(pc 22) 22 %, continuation after the first self-call (pc 11) 19 %, side of the entry (loop, exit 13) 13 %, leaf side trace 5.5 %
(26 instructions per leaf call).

Per-call decomposition from instruction counts (variants of the program, same call tree; 3000 rounds):

| variant | instr M | cycles M | derived |
|---|---:|---:|---|
| local-function recursion, no table, no swap, no count (`p_plain`) | 3530 | 581 | **136 instr / 22 cyc per call**: the recursion protocol plus the `for` loop (fib is 99.5 / 16) |
| method recursion `self:permute(n1)`, no swap, no count (`p_rec`) | 4481 | 809 | +36 instr / +9 cyc per call: `SELF` on the instance (HMISS), metatable guard, `__index` HREFK/HLOAD, method HREFK/HLOAD, `EQREF` on the proto (12 IR ops, repeated at every call) |
| + `self.count = self.count + 1` (`p_noswap`) | 4750 | 834 | +10 instr per call |
| + the two `swap` calls (full program) | 7036 | 1177 | +76 instr / +11.4 cyc **per swap** (88 per call at 1.16 swaps per call); 67 of them remain with the swap written inline (`p_inlineswap` 6775) |

The swap is 2 array loads and 2 array stores of integers but costs ~67 instructions: in the dumped IR of the loop-continuation trace
(`RUSTYNAIL_JIT_DUMP`) each swap redoes the 12-op method lookup chain, reloads `self.v` (HREFK + HLOAD), and each element access is
`ADD` index, `ULT` bound (against a reloaded `asize`), `AREF`, `ALOAD` with a type guard (S19/S22 snapshots), then two `ASTORE`
(no barrier: integer values). LuaJIT keeps the table and bounds in registers and folds the method lookup into one constant guard.

Where the gap to LuaJIT (218 instructions per call) sits: recursion protocol ~110 (136 vs ~25), array stores / loads ~68 (88 vs ~20),
method lookups ~28 (36 vs ~8), `count` ~7. **The recursion protocol dominates (50 %), array element traffic is second (31 %), the
method lookups third (13 %); aborts and GC barriers do not appear** (no barrier executes: the stored values are integers; no abort
matters: 7 recordings in total). The cliff of C2 is the one coverage risk of this program family (`count` removed: 2.7x slower).

Handles for permute (estimates, cycles): protocol diet continuation of H7 (identity hoist, POLL only at entry): -8..12 %;
redundant method-lookup chain removal within a trace (same receiver, no intervening store to the instance / metatable, one chain per
trace segment, plus `self.v` CSE across the swaps): -10..14 %; array element access (bound reloads, `ALOAD` type-guard folding after an
`ASTORE` of the same slot type, keep `asize` in a register): -6..9 %; C2 fix: 0 % here, 2.7x for the cliff variants. Together -25..35 %
(45 -> ~31 cycles per call, still 2.8x LuaJIT: the rest is the call protocol).

## 5. Ranked handles

Gains are cycles of the named program; "measured" = prototype above, others arithmetic from the shares. Risk: soundness / coverage cost.

| # | handle | expected gain | risk | effort | files |
|---|---|---|---|---|---|
| K1 | **C7: mixed integer / float `math.min` / `math.max`** (and `table.remove(t, i)`, other `Nyi(Call)` sites that the abort sites now name) in the recorder | raytrace **-10.7 % cycles / -17 % instr (measured by source rewrite)**, deltablue -1..2 % | low: the branch direction becomes a guard (`LT` with the compare result as the type selector); two mutants (wrong type after the branch, missing guard) | S | `rn-jit/src/tr/record.rs` (`lib_minmax`, `lib_tremove`), `interp.rs` (model), `tests/tr_core`, `mutants.rs` |
| K2 | **C2: `FnShort` leaf samples must not blacklist a recursive function-entry head**: extend `selfref` to method recursion (`SELF` + same proto in the callee slot) or count leaf samples as probes (no penalty) until a non-leaf sample was seen | the 2.7x cliff of `p_nocount` (any method-recursive function with a cheap leaf); awfy 0 .. -1 %; protects future builders from a lottery | low-medium: more heads record longer; check the `FnShort` tests (`fn_root_short_recordings_blacklist_the_head`) and list / fib / towers instruction counts | S | `rn-jit/src/tr/cache.rs` (`on_abort_with`, `mark_selfref`), `mod.rs` (`driver_start`), `tests/tr_core` |
| K3 | **C5: side traces for loop exits inside inlined frames** (list `isShorterThan`: five exits of the unrolled loop, `SideShort` at `L21:10`; deltablue `Plan:execute` `LeaveFrame`): let the side recording run through the callee's `RETURN` into the caller (`RETF`) or link back to the loop head | list **-15..25 %**, deltablue -3..6 %, cd -2..4 %, richards -2 % | medium: frame restore at exits with `lazyci` frames; reuses N3a `RETF` / `noretf` | M-L | `rn-jit/src/tr/record.rs` (`SideShort`, frame exits), `cache.rs` (`side_attempt`), `exit.rs` |
| K4 | **C1/C7: end a side recording at a loop head of the same frame that has a root trace** instead of unrolling the loop (raytrace `trace()` shading: `TooLong[sideins]`, then `snaps`) | raytrace a further -10..15 % after K1; cd -2..3 % | medium: the side must link into the loop's entry (nest links exist for callee heads: `nest_frame`) | M | `rn-jit/src/tr/record.rs` (`head_state`, `side_stitch`), `cache.rs` |
| K5 | **permute / recursion: method-lookup chain CSE and array traffic**: dedupe the 12-op `SELF` chain and the `self.v` HREFK within a trace segment, `asize` reload and bound guards for consecutive array ops | permute -16..23 % (arithmetic: 36 + 30 instr per call of 271), fib / towers 0 (no method chain), richards / deltablue -2..4 % | medium: invalidation across stores (an `HSTORE` / call can change the metatable): needs the alias rules of `opt_mem.rs` | M | `rn-jit/src/tr/opt_mem.rs`, `fold.rs`, `record.rs` (`SELF`) |
| K6 | **C3: outer loops through calls with loops** (raytrace `render`, deltablue `Planner` nest) | deltablue -5..8 %, raytrace -3..4 % | high (nest model, exits) | L | `record.rs`, `cache.rs` |
| K7 | **C6 root-frame `TAILCALL`** (H6) | richards -4..6 %, others < 2 % | high (docs/JIT_TRACE.md section 20) | L | see section 20 |
| K8 | **H7 recursion protocol diet, continued** | permute -8..12 %, fib -10..15 %, towers -8..12 % | high (family invariant, CP3 lost cycles once) | L | `asm.rs`, `cache.rs` |
| K9 | **C8 cleanup**: do not arm continuation heads of a prototype that is not recursive (`ContNotRecursive`) | 0 cycles; removes 100-700 refused starts per program from the statistics and the `Black` marks of cont heads | low | S | `rn-jit/src/tr/cache.rs` (`hot`), `mod.rs` |
| H3, H5 | allocation / materialisation (STUDY_ALLOC_CALL): cd 20 % materialise, raytrace 20 % alloc | cd -6..10 %, raytrace -5..10 %, binary_trees -12..18 % | see that study | L / M | see that study |

Not handles (measured here): `fnmin=0` (awfy 0 .. +5.5 %), `sidemin=0` (list -2.5 %, deltablue +2 %), `maxdepth=8` (cd +7 %),
`innerretry=8` (0 .. +4 %), `MAX_SIDE_INS` 4000 (raytrace +5 .. +14 %, hits `snaps`), `deep=0` / `deepins=6000` (no change).

## 6. Recommended order for later builders

1. **K1** (S, measured -10.7 % raytrace): `lib_minmax` mixed shapes; then grep the verbose logs of the whole benchmark set for the other
   `Nyi(...)@record.rs:<line>` sites (the diagnostics now name them: `scripts/heads-perf.py` is not needed, `VERBOSE=1` is enough) and take
   the ones with weight; `table.remove(t, i)` is the next one (deltablue).
2. **K2** (S): extend the `FnShort` leaf rule to method recursion; gate `permute` without the `count` store (411 vs 1126 Mcyc) and
   keep `fib`, `towers`, `list`, `binary_trees` instruction counts.
3. **K3** (M-L): `list` / `deltablue` loop-exit side traces: gate `list 7000` (isShorterThan T1 34.5 %), `deltablue 400`.
4. **K4** (M): side recordings end at a traced loop head of the same frame; gate `raytrace` shading (`TooLong` rows disappear).
5. **K5** (M): method-lookup / array CSE for permute; gate `permute 3000` instructions (7036 M today) and the oracle.
6. K8, K6, K7 (L, high risk) only when 1-5 are in and measured; every number above moves with the formation lottery: measure with
   several script paths (different binary path = different GC pacing: see section 1) and `RUSTYNAIL_JIT_VERBOSE=1` heads before and
   after (the `[heads]` rows are the acceptance test: the row of the head must change state or lose its abort).

Using the diagnostics for a new program: `RUSTYNAIL_JIT_VERBOSE=1 RUSTYNAIL_JIT_SAMPLE=on rnlua prog.lua 2>&1 | grep '^\[heads\]'`
for the table, `RUSTYNAIL_JIT_PERFMAP=2 RUSTYNAIL_JIT_VERBOSE=2 perf record -e cycles:u ...` plus `scripts/heads-perf.py` for per-head
cycle shares and helper families; read the rows top-down: `Traced` with side aborts (C4/C5/C7), `Black` with `FnShort` (C2), `Warm` /
`Black` with `InnerLoop` (C3), tier-1 protos with no row at all (C1: headless callees).
