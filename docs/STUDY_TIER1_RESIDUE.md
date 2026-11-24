# Study: the tier-1 residue and the remaining LuaJIT gap (2026-11-22, base 2222703)

Question: why do the programs that session-k still lists as weak against LuaJIT (plain/LuaJIT cycles: `awfy/cd` 2.4, `apps/raytrace` 2.8,
`corpus/lua_lexer` 2.4, `awfy/richards` 1.7, `awfy/permute` 2.8, `awfy/towers` 2.0, `corpus/classic_vector` 2.0; plus `awfy/deltablue`,
`havlak`, `list`) still run 16-47 % of their instructions outside traces, which reasons put that code there, what LuaJIT does at the same
spots, what each reason is worth, and how the instruction gap to LuaJIT splits into categories. Study only: no production behaviour change
(one diagnostic field pair added to an existing diagnostic, below); every ceiling quoted as [M] is an existing knob or a scratch build that was
measured and reverted. [M] = measured, [E] = estimate (arithmetic or attribution rule stated next to it).

Method. Plain release `rn-cli` of 2222703 (`cargo build --release -p rn-cli`; JIT and traces on, generational GC), `RUSTYNAIL_SEED=0`,
binaries at one fixed path, every process pinned to CPUs 10-15 under the memory cap (another builder times cycles on CPU 4: **no cycle
numbers in this study, only instruction counts and instruction-sampled profiles**). Profiles: `perf record -e instructions:u -c 1000000`
(a smaller period is throttled by `perf_event_max_sample_rate` 32000 at the 6+ IPC of the traces and biases the shares against trace code:
checked with `perf report --stats`, 403 THROTTLE events at `-c 300000` on `cd 10`, none at `-c 1000000`), `RUSTYNAIL_JIT_PERFMAP=2`, 1-10
runs merged per program for about 6000 samples at the default size; a second run with `RUSTYNAIL_JIT_VERBOSE=2 RUSTYNAIL_JIT_EXITDUMP=1`
gives the head table, the trace -> head map and the per-exit counts. Two sizes: the bench default (the size of the session ratios) and a
larger one for the per-site ranking (`cd 10`, `raytrace 20`, `richards 250`, `permute 1000`, `towers 450`, `deltablue 200`, `havlak 30`,
`list 3500`; `lua_lexer` and `classic_vector` are 4.4 G / 2.1 G instructions at the default already). LuaJIT 2.1 (`/usr/bin/luajit`) on the
same files: `perf stat` (min of 5, matches session-k's `lj_ins_min`), `perf record` by DSO (trace code is `[JIT]`, the stripped
`libluajit` holds interpreter, GC, compiler and the helpers called from traces), `-jp=vi1` VM-state profiles (8 runs averaged; time shares),
`-jv` trace logs. The whole suite (223 programs) once at the default size with the same profile and the head table, plus three knob
columns.

Tools added (scripts only):

* `scripts/tier1-residue.py rep log map`: category shares of one profile (traces, tier 1, interpreter, exits / materialisation, trace
  helpers, tier-1 helpers, allocation, GC, library, strings, table ops, compiler, libc, other), trace share per head, tier-1 share per
  prototype, the trace exits that land in tier 1 grouped by landing prototype with their counts and closing abort, the head rows of the
  top tier-1 prototypes.
* `scripts/residue-aggregate.py DIR... [--suite]`: classes each prototype's tier-1 share by the reason the code is there (rules in the
  script header: split over the landing exits by exit count, else the prototype's own head rows) and sums over programs.
* Diagnostic (behind the existing `RUSTYNAIL_JIT_EXITDUMP=1`): every `[exitdump]` line now ends with `land <addr> link <bool>` (the tier-1
  landing address, which `RUSTYNAIL_JIT_PERFMAP=2`'s map turns into the landing prototype, and whether the exit is an F2b frame-linking exit
  that continues in a callee loop's trace instead of tier 1). Format-only; `scripts/bench_o_o0.py`'s parser still matches.

## 1. Where the instructions go

### 1.1 Category shares per program [M]

Instruction shares in %. `tier 1` = method-JIT code (`lua_proto:` blocks); `tier-1 helpers` = `jit_get/set/call/newtable/...` and the call
glue; `trace helpers` = `jit_tr_*` (allocation, `string.*` arms, `NEWREF`); `exits` = materialisation of sunk tables, exit entry and
restore; `compiler + libc` = the trace recorder and pipeline, the tier-1 lowering, and libc (malloc / free / memcpy: it scales with the
compiler share between the two sizes, cd 6.5 -> 1.5 while the compiler goes 20.0 -> 3.7, so it is counted as compile cost [E]).

| program (size) | Minstr | traces | tier 1 | interp | tier-1 helpers | trace helpers | lib + strings + table ops | exits / materialise | alloc | GC | compiler + libc | other |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| cd (default) | 774 | 19.7 | 25.9 | 1.3 | 1.0 | 0.8 | 1.3 | 13.4 | 5.5 | 3.6 | 20.0 + 6.5 | 1.0 |
| cd (10) | 5562 | 29.5 | 29.7 | 0.2 | 2.2 | 1.1 | 1.5 | 16.2 | 8.7 | 4.9 | 3.7 + 1.5 | 0.8 |
| raytrace (default) | 366 | 23.7 | 20.4 | 1.3 | 2.2 | 2.8 | 1.3 | 0.7 | 6.1 | 3.5 | 28.1 + 8.6 | 1.3 |
| raytrace (20) | 2377 | 35.8 | 32.9 | 0.1 | 4.8 | 2.8 | 1.4 | 1.4 | 9.2 | 5.0 | 4.4 + 1.3 | 1.0 |
| lua_lexer (default) | 4415 | 35.1 | 14.0 | 0.2 | 4.8 | 5.4 | 25.6 | 2.6 | 1.7 | 3.4 | 1.2 + 4.7 | 1.3 |
| richards (default) | 177 | 57.2 | 14.1 | 2.4 | 1.0 | 0.1 | 0.4 | 1.1 | 0.1 | 0.4 | 17.7 + 5.0 | 0.4 |
| richards (250) | 1422 | 76.2 | 17.7 | 0.7 | 1.3 | 0.1 | 0.4 | 0.1 | 0.1 | 0.1 | 2.7 + 0.6 | 0.0 |
| permute (default) | 195 | 96.6 | 1.0 | 0.6 | 0.0 | 0.1 | 0.0 | 0.0 | 0.0 | 0.0 | 1.4 + 0.2 | 0.1 |
| permute (1000) | 1892 | 99.2 | 0.3 | 0.1 | 0.0 | 0.1 | 0.0 | 0.0 | 0.0 | 0.0 | 0.2 + 0.1 | 0.0 |
| towers (default) | 175 | 75.2 | 6.2 | 0.7 | 0.3 | 0.2 | 0.1 | 1.3 | 0.1 | 0.0 | 11.5 + 4.0 | 0.3 |
| towers (450) | 1409 | 93.6 | 2.7 | 0.1 | 0.0 | 0.0 | 0.1 | 0.3 | 0.1 | 0.0 | 2.2 + 0.8 | 0.1 |
| classic_vector (default) | 2054 | 34.3 | 0.4 | 0.7 | 0.3 | 8.1 | 8.7 | 15.7 | 19.6 | 8.0 | 3.2 + 0.7 | 0.4 |
| deltablue (default) | 324 | 22.5 | 25.3 | 11.5 | 2.7 | 0.7 | 5.2 | 0.2 | 3.0 | 2.3 | 18.7 + 6.2 | 1.6 |
| deltablue (200) | 2291 | 35.9 | 37.5 | 3.3 | 3.8 | 1.5 | 5.3 | 0.2 | 3.7 | 1.5 | 4.8 + 1.5 | 0.9 |
| havlak (default) | 516 | 7.0 | 21.9 | 3.1 | 6.2 | 5.2 | 16.6 | 0.5 | 13.7 | 12.7 | 8.9 + 2.5 | 1.7 |
| havlak (30) | 2065 | 12.6 | 21.6 | 1.6 | 7.4 | 6.5 | 19.5 | 0.5 | 15.7 | 9.7 | 2.6 + 0.6 | 1.7 |
| list (default) | 395 | 80.7 | 11.1 | 0.4 | 0.1 | 0.2 | 0.1 | 0.3 | 0.8 | 0.0 | 4.6 + 1.6 | 0.2 |
| list (3500) | 3681 | 88.4 | 9.4 | 0.0 | 0.0 | 0.4 | 0.0 | 0.0 | 0.7 | 0.2 | 0.5 + 0.2 | 0.0 |

Two things the earlier studies did not show:

1. **At the bench default size the trace compiler is 15-37 % of the instructions** of raytrace (36.7), cd (26.5), deltablue (24.9), richards
   (22.7) and towers (15.5) [M]. Split for raytrace (default): trace pipeline (assembler, register allocator, exits, optimiser passes, cache
   policy) 16.4, libc 8.6, recorder 6.1, tier-1 lowering 1.8, other `rn_jit` 2.2 [M]. raytrace compiles 23 root recordings (7 installed) and 45
   pipelines for 134 M instructions: ~2.0 M per recording + pipeline [M]; its 12 k recorded bytecodes cost `rec_step` 9.7 Mtsc of the 25.8
   Mtsc of trace compile time (`[trace] phases`), most of it in recordings that abort (`TooLong`, `snaps`). The interpreter is the other
   warm-up cost: deltablue 11.5 % at the default size (3.3 % at 200), richards 2.4 %.
2. **The interpreter proper is small once warm** (0.0-3.3 % at the large sizes); "outside traces" is tier 1 plus its helpers, as before.

### 1.2 Suite (223 programs, default size) [M]

Mean shares: traces 46.3, library 10.4, tier 1 8.5, GC 4.5, libc 4.5, trace helpers 4.2, alloc 3.8, tier-1 helpers 3.6, interpreter 3.0,
strings 2.9, compiler 2.3, table ops 1.4, exits 0.3, other 4.5. Medians: traces 35.5, tier 1 + tier-1 helpers + interpreter 5.3, compiler +
libc 2.5. The programs with the largest tier-1 + helpers + interpreter share are the library micro programs whose loop calls a C function the
recorder does not take (`lib/m_ult` 94.6, `lib/m_fmod` 85.9, `lib/utf8_len` 80.4, `lib/tonumber_base` 73.2, `lib/tsort_cmp` 67.9,
`lib/co_yield` 59.5, `lib/error_tbl` 57.0), coroutine programs (`corpus/coro_scheduler` 71.1, `programs/coroutines` 58.4), and
interpreter-shaped code (`real/lua_interp` 60.8, `corpus/parser_combinator` 57.3, `programs/closures` 56.8, `apps/events` 55.3).

Head-table totals over the suite [M]: root aborts `StartRefused` 8412 (all `ContNotRecursive`: weight 0, as C8 of STUDY_COVERAGE), `FnShort`
1576, `InnerLoop` 415, `LeaveLoop` 322, `LeaveFrame` 177, `CFunc` 160, `Nyi(Closure)` 137, `Nyi(Call)` 127, `TableHash` 70; side aborts
`LeaveFrame` 1143, `SideShort` 804, `CFunc` 291, `Recursion` 288, `SideUntraced` 149, `Depth` 129, `InnerLoop` 114, `Metamethod` 69; heads
blacklisted by `StartRefused` 325, `FnShort` 149, `CFunc` 71, `Nyi(Closure)` 63, `Nyi(Call)` 61, `TableHash` 33, `MultRet` 26, flush 21,
`TailCall` 20. The `Nyi(Call)` sites by `record.rs` line: 54 `string.find` with a real pattern (10960), 29 `tostring` of a non-number
(10386), 22 `table.insert(t, pos, v)` (10679), 22 `string.char` with more than 3 arguments (10175), 10 `string.byte(s, i, j)` beyond the
range limit (10124), 8 one-argument numeric identities called with more (10855). The `CFunc` callees (named from the bytecode at the abort
pc): string methods through `SELF` (86 sites in 19 programs), `table.concat` (117 sites), `string.match` (45), `table.sort` (34),
`string.format` (29), `unpack` (14), coroutine `yield` / `resume` (22), `string.gsub` (11).

Suite residue by class (mean over the 223 programs of the tier-1 share each class explains; tier 1 alone is 8.48 % on average) [E: attribution
rule of `residue-aggregate.py`, M: shares]: a head that ends `CFunc` 1.19, a headless callee 1.18, `Nyi(Call)` 1.08, `FnShort` 0.88, exits at a
root `RETURN` closed by `LeaveFrame` 0.67, `SideShort` exits 0.58, side `Nyi` / `CFunc` 0.33, `MaxSide` 0.30, `Nyi(Closure)` heads 0.30,
down-recursion start exits 0.28, `InnerLoop` 0.28 + 0.19 (heads + exits), `TableHash` 0.15, root `TAILCALL` 0.16. **Across the suite the
largest single reason is unsupported library calls (CFunc + Nyi(Call) + side Nyi: 2.6 of 8.5 points)**; in the ten focus programs it is
not (next section).

## 2. The residue of the ten programs

### 2.1 Reason classes [E attribution, M shares]

Tier-1 instruction share (% of all instructions) by reason, large size where there is one (default for lua_lexer / classic_vector). A
"headless callee" is a function without a `CALL` (no function-entry head) that runs in tier 1 because its caller does: it is the symptom of
the caller's reason, listed separately because it is the biggest block.

| class | cd | raytrace | lua_lexer | richards | permute | towers | classic_vector | deltablue | havlak | list | sum |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| tier 1 total | 29.8 | 32.8 | 14.0 | 17.7 | 0.3 | 2.7 | 0.4 | 37.4 | 21.6 | 9.4 | 166.1 |
| headless callee under a tier-1 caller | 10.3 | 20.3 | - | 5.0 | 0.2 | - | 0.3 | 6.6 | 3.7 | - | 46.4 |
| `InnerLoop`: the head's recording reaches a callee with a loop | 0.1 | 6.0 | - | 1.1 | - | - | - | 14.9 | 5.9 | 3.8 | 31.8 |
| `FnShort` leaf samples blacklist a function-entry head | 1.5 | 0.7 | - | 4.6 | - | 0.1 | - | 10.5 | - | - | 17.4 |
| down-recursion: start exit of a `rec_ret` trace (no side trace allowed) | 9.9 | - | - | - | - | - | - | - | - | 2.5 | 12.4 |
| exit at the root frame's `RETURN`, side closed `LeaveFrame` | 0.2 | - | 5.1 | - | - | - | - | 3.5 | 1.2 | - | 10.0 |
| side recording cut by `Depth` / `Recursion` | 2.2 | - | - | - | - | 2.1 | - | 0.9 | - | 2.9 | 8.1 |
| `SideShort` | 0.9 | - | 0.6 | - | - | - | - | 0.2 | 5.9 | - | 7.6 |
| `MaxSide`: the family is full | 3.5 | - | 0.2 | 3.0 | - | 0.3 | - | - | - | - | 7.0 |
| exit at a root `TAILCALL` (`LeaveFrame` / `TailCall`) | - | 4.8 | 0.6 | 1.6 | - | - | - | - | - | - | 7.0 |
| unsupported: `CFunc`, `Nyi(Call)`, `TableHash`, `Closure` | - | - | 1.9 | - | - | - | - | - | 4.9 | - | 6.8 |
| `TooLong` | 0.5 | 0.9 | - | - | - | - | - | - | - | - | 1.4 |
| flushed head / other | 0.8 | - | 5.7 | 2.4 | - | 0.1 | - | 0.6 | - | - | 9.6 |
| (tier-1 helpers, not attributed) | 2.2 | 4.8 | 4.8 | 1.3 | 0.0 | 0.0 | 0.3 | 3.8 | 7.4 | 0.0 | 24.6 |

At the default size the same table has tier 1 26.0 / 20.4 / 14.0 / 14.1 / 1.0 / 6.2 / 0.4 / 25.3 / 21.9 / 11.1 and the same ranking
(headless 38.8, InnerLoop 24.7, FnShort 15.0, down-recursion 11.2, LeaveFrame 10.2); the interpreter adds 11.5 points on deltablue.

### 2.2 Top residue sites per program [M]

Weights are % of the program's instructions at the large size; exits are the `[exitdump]` counts of the tier-1-landing exits.

**cd (10)**: tier 1 29.7, exits / materialisation 16.2 (H5 of STUDY_ALLOC_CALL).
* `put` (L97, recursive red-black insert) 12.3 % tier 1 + its callees `isRed` (L51) 5.2, `CallSign:compareTo` (L41) 2.5, `rotateLeft`
  / `flipColors` 1.4: the down-recursion. E1/E4 made `put`'s returns into two `rec_ret` loop traces (heads pc 35 and pc 43: the return into
  `h.left = put(..)` and into `h.right = put(..)`); each is entered when a return lands at its pc and exits at its **start snapshot** when
  the next return goes to the other site (left/right alternation). Those exits are blacklisted at install (`no_start_side`, `attach_exits`):
  1.32 M exits land in `put`'s tier-1 code (traces 19 and 16: 328 k + 210 k + 137 k + 137 k ...), and the rest of the return chain, with
  `isRed` and the rotations, runs in tier 1.
* `MaxSide`: the `recurse` loop family (L239 pc 34) is full: exits at L210 pc 92 / 87 (`Vector2D` comparisons, 41 k `MaxSide` + 203 k with no attempt recorded), L20
  (101 k), L239 pc 44 `Return0` (141 k).
* `Depth` / `Recursion` side aborts at `L97:16` (the fifth frame), `SideShort` at L263 pc 71 (`Forloop`, 97 k exits).
* Compile at the default size: 26.5 %.

**raytrace (20)**: tier 1 32.9 + helpers 4.8; the shading section of `trace()` and everything below `render` run in tier 1.
* `vec` (L11) 16.0 %, `V:add/mul/sub` 4 %: headless constructors called from tier-1 code (each `vec` in tier 1 is a `NEWTABLE`, three
  new-key `SETFIELD`s and the `setmetatable` C call).
* `render` (L97) 6.0 %: both loop heads `InnerLoop` (`trace()` has loops, `L63:10 Forprep`), so `render`'s calls of `vec(..):norm()` and
  `trace()` start in tier 1. `V:norm` (L16) has its own function-entry trace (136 k entries), whose final exit is the root `TAILCALL`
  `return vec(..)` (pc 15): 136.7 k exits, side closed `LeaveFrame` (a side trace that would start at the `TAILCALL` itself).
* `trace` (L63) 4.7 %: the "no hit" path `return vec(0.6 + ..)` (pc 43, root `TAILCALL` of the loop root's side trace 13): 137 k exits,
  `LeaveFrame`; the shading path leaves through L41 pc 2 (`Plane:hit`, 167 k exits, side closed `TooLong[sideins]`).
* Compile at the default size: 36.7 %.

**lua_lexer (default)**: tier 1 14.0 + helpers 4.8 + library 16.2 + strings 8.9.
* `Lexer:next` (L80) 6.6 %: 963 k tier-1-landing exits, 599 k of them at the root frame's `RETURN` of side traces of the loop root L80
  pc 99 (`Return` pc 128 / 131, side closed `LeaveFrame`): `Lexer:next` is called once per token from the main loop, its loop traces end
  by returning, and the return cannot be recorded: every side recording whose root prototype has a function-entry head
  (`head_is_fn(pr, 0)`, `driver_start_side`) is `fn_root`, and `Rec::ret` pops a `fn_root` frame only with RETF bit 2, which is off
  (C38). So bit 2 governs the loop roots of such prototypes too, not only function-entry traces. (`retf=7` removes 17.1 % of
  lua_lexer's instructions, but the scratch selector of section 3 that sets bit 2 only for loop-root families gains nothing: the paying
  sides are in function-entry families; the exact family is not identified.) (1.44 M other
  exits are F2b frame links into `Lexer:next`'s loop trace, not tier 1.)
* main chunk (L0) 4.4 %: its loop head pc 127 was flushed (`Dom`).
* `is_digit` (L16) 1.6 %: 750 k exits at its `b <= BYTE_9` guard (side of L80 pc 1), the side recording dies `Nyi(Call)` at `L80:158`
  (`string.find` with a real pattern, `record.rs:10960`: the number branch).
* `Lexer:read_string` (L49) 0.8 %: `SideShort` at pc 97 `Settable` (195 k).

**richards (250)**: tier 1 17.7.
* `Scheduler:queue` (L110) 3.4 %, `TCB:addInputAndCheckPriority` (L60) 2.6 % (`FnShort`), `Scheduler:holdCurrent` / `waitCurrent`
  (`FnShort`): leaf functions called from tier-1 task bodies.
* `HandlerTask:run` (L172) 2.5 %, `TCB:run` (L47) 1.7 %, `WorkerTask:run` (L157) 1.4 %: the scheduler loop family (L83 pc 2, 75 % of
  the instructions in traces) is **full** (`MaxSide`): exits at L172 pc 23 / 12 (30 k + 21 k + 21 k + 20 k), L47 pc 11, L44 pc 6 (34 k)
  have no side trace; L47 pc 25 `Tailcall` 115 k exits (side `InnerLoop`); L157 pc 33 `Forloop` 58 k exits (side `TailCall`).

**permute / towers**: coverage is not the issue (traces 99.2 / 93.6 % at the large size). towers: 637 k exits at L32 pc 17 (`Call`, frames
2, side cut by `Recursion`), 2.1 % tier 1. Their gap is the trace code itself (section 4).

**classic_vector**: tier 1 0.4 %; the gap is allocation (`newref_presize_core`, `insert_small`, `setnodevector` 19.6 % alloc), GC 8.0, and
materialisation of sunk tables 15.7 % (`materialise` 8.2, `restore` 2.9).

**deltablue (200)**: tier 1 37.5 + interpreter 3.3 + helpers 3.8.
* `Planner:addPropagate` (L267) 5.9 %, `Constraint:satisfy` (L58) 5.5 %, `Planner:makePlan` (L245) 3.2 %, `incrementalAdd` (L223) 2.0 %:
  heads `InnerLoop` (their callees `addConstraintsConsumingTo` L303 / `Plan:execute` have loops) and `LeaveFrame`.
* `BinaryConstraint:*` (L154-158) 6.3 % together, `Strength` helpers: `FnShort` heads and headless leaves called from those tier-1 callers.
* Exits at a root `RETURN`, side closed `LeaveFrame`: `addConstraintsConsumingTo` (L303 pc 18, 162 k + 20 k + 19 k), `removePropagateFrom`
  / `Planner:*` function roots L198 (54 k + 20 k + 20 k), L126 (4 x 19 k), L211 (2 x 19 k), `Plan:execute` L216 (17 k).
* `Depth` at `L86:3` / `L149:3` (36 k + 38 k: the method chains of `chainTest` / `projectionTest`).

**havlak (30)**: tier 1 21.6 + helpers 7.4 + table ops 16.3 + alloc 15.7 + GC 9.7.
* `findLoops` (L117) 11.9 %: loop root pc 106's exit at the inner `Forprep` (pc 122, 48 k exits, `InnerLoop`), loop root pc 240's `Forloop`
  exit (30 k + 18 k, `SideShort`).
* `dfs` (L102) 4.9 %: fn head and loop head `TableHash` (`number[cur]`: a table keyed by node objects, `GETTABLE` / `SETTABLE` with a table
  key is not recorded) -> blacklisted; it is recursive and runs entirely in tier 1 with `jit_get` / `getgeneric` / `mainposition_slow`.
* `UnionFind:find` (L87): 233 k exits at its root `RETURN` (`LeaveFrame`).

**list (3500)**: tier 1 9.4. `tail` (L31) 8.7 %: 276 k exits at pc 11 (`Call`, side cut by `Recursion`), 235 k at the start of the
continuation trace pc 4 (`no-side`), 137 k + 135 k `InnerLoop` (the inlined `isShorterThan` loop).

### 2.3 What LuaJIT does on the same programs [M]

LuaJIT's own coverage is not complete either (`-jp=v`, time shares, 8 runs; `-jv`): raytrace **32-41 % interpreted** (35 blacklisted
starts, 59 "leaving loop in root trace", 9 "NYI: return to lower frame"), cd 8-12 % interpreted (28 "NYI: bytecode UCLO", 20 "NYI: bytecode
FNEW", 22 "call unroll limit reached"), lua_lexer 5 % interpreted + 18 % C code, permute 17-31 % interpreted with a trace-formation lottery
(1939 "call unroll limit reached" aborts in one run; instructions vary 55-2708 M between identical runs, session-k takes the minimum),
deltablue 51 "loop unroll limit reached", havlak 54. Its trace code (`[JIT]` DSO) is 10-30 % of its instructions on raytrace / havlak /
deltablue / classic_vector at the default size (the rest is `libluajit`: interpreter, GC, compiler and the helpers its traces call). **The difference is that LuaJIT's fallback (its interpreter, ~1 instruction per bytecode
handler step plus dispatch) is far cheaper than our tier 1 plus helpers**: raytrace (20) runs about 500 M instructions interpreted in LuaJIT
[E: jp time share x instructions] against our 898 M in tier 1 + tier-1 helpers + interpreter [M] for (by the head tables) a similar code
region. Where LuaJIT does trace what we do not, the mechanisms are: side traces that return to lower frames (a guard on the return pc,
`lj_record_ret`), links from an outer trace into an inner loop's root trace (`LJ_TRLINK_ROOT`), stitching around unrecorded C functions
(2.1), up to 100 side traces per trace (`maxside` 100), no minimum length for side or function traces, `HREF` for any key type.

## 3. The top reasons: LuaJIT, handle, soundness, effort, ceiling

Knob ceilings are instruction deltas of the shipped binary under `RUSTYNAIL_JIT_TRACE_LIMITS` (env length padded equal), default size and
large size; scratch ceilings are builds with an environment switch, measured against the same binary with the switch off, then reverted.

Knob ceilings on the ten programs [M] (instructions, % against the default knobs; `def` = default size, `L` = large size):

| program | maxside=64 | retf=7 | lazyci=0 | sidemin=0, framemin=0 | maxdepth=8 | fnmin=0 | nest=2 | innerside=0 |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| cd def / L | +5.1 / +0.6 | +0.4 / -0.9 | -1.5 / -1.8 | -0.1 / +0.2 | +3.7 / +0.1 | 0 / 0 | -1.5 / -2.3 | 0 / 0 |
| raytrace def / L | +0.6 / +0.1 | 0 / 0 | 0 / 0 | +0.1 / 0 | +0.1 / 0 | 0 / -0.3 | +0.5 / +0.1 | +2.0 / +0.2 |
| lua_lexer def | -3.2 | **-17.1** | 0 | -2.0 | 0 | 0 | +3.1 | 0 |
| richards def / L | +0.9 / **-7.7** | 0 / 0 | 0 / 0 | 0 / 0 | 0 / 0 | +1.9 / -1.4 | 0 / 0 | 0 / 0 |
| permute def | 0 | 0 | +20.6 | 0 | 0 | 0 | 0 | 0 |
| towers def | 0 | 0 | +2.2 | 0 | 0 | +4.4 | 0 | 0 |
| classic_vector def | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| deltablue def / L | 0 / 0 | +3.1 / -2.0 | -0.1 / -0.1 | +0.1 / 0 | +5.8 / +1.6 | -0.6 / -2.0 | +0.3 / 0 | 0 / 0 |
| havlak def / L | -0.1 / 0 | +0.3 / -0.8 | 0 / 0 | 0 / 0 | +0.7 / +0.2 | -0.5 / +0.6 | -0.7 / -1.0 | 0 / +0.1 |
| list def / L | 0 / 0 | +55.6 / +64.3 | -0.2 / +4.8 | +24.1 / +29.2 | 0 / 0 | 0 / 0 | +64.4 / +73.9 | +0.5 / -0.1 |

(`retfmax=4`, `lazyloop=0`, `innerretry=8`, `loopunroll=8`, `maxside=256,polyside=256` were measured too: nothing beyond +-0.1 except
list +2.7..3.0 (`retfmax`, `loopunroll`), permute +20.6 (`lazyloop=0`), cd def +7.9 / L +1.4 (`maxside=256`).)

Suite columns [M] (223 programs, default size): `retf=7` geomean 1.0054 instructions, 13 programs better than -2 %, 11 worse than +2 %: lua_lexer
-17.1, awfy/queens -16.5, scheme_interp -10.6, expr_compiler -9.5, field_method -7.9, field_global -6.5, json_rxi -5.9; roguelike_turns
**+112**, list +56, real/lua_interp +35, regex_nfa +20, sudoku_solver +16.5, astar_jumper +7, nbody +5.3 / +7.8, deltablue +3.1.
`maxside=64` geomean 0.9999: oop_mixins **-30.8**, vending_fsm -5.3, heapsort -4.8, regex_nfa -4.2, lua_lexer -3.2; expr_compiler +17.7,
astar_jumper +10.6, cd +5.1, lru_cache +5.1, particles_physics +3.5.

Scratch ceilings [M] (suite, default size):
* **S-A, RETF for function-entry families of non-recursive prototypes** (bit 2 only when neither `proto_recursive` nor `selfref` is set at
  the side start): geomean 1.0043; the wins stay (lua_lexer -17.1, scheme_interp -10.6, expr_compiler -9.0), queens' -16.5 is lost (-0.4),
  lua_interp (+34.8 -> 0.0) and sudoku_solver (+16.5 -> 0.0) are fixed, but the biggest regressions stay (roguelike +112, list +56,
  regex_nfa +20, astar +16, nbody +5..8, deltablue +3.1). The recursion flag removes part of them, it is not the selector.
  A second selector, **bit 2 only for side recordings whose family root is a loop trace** (head pc != 0), with and without the recursion
  condition: geomean 0.9998 / 0.9992, astar_jumper -3.2 / -13.7, real_dis_x86 -2.1, **everything else 0.0, lua_lexer included**. So the
  lua_lexer / scheme_interp / expr_compiler gain of `retf=7` comes from sides of function-entry families (or from RETF changing which
  families form), not from the loop roots of section 2.2's reading; finding which families pay is the first step of T1 (per-family
  bisection with the knob, not done here). roguelike with
  `retf=7` [M]: tier 1 15.9 -> 69.8 %: its hottest loop head (L234 pc 96, 31.5 % of the instructions in traces) ends up without a trace; a
  formation effect of the RETF side traces (more flushes: 35 -> 33 installed, 6 -> 12 blacklisted, `Unstable` 4, `Nyi(Close)` 4).
* **S-B, side recordings stitch at a `CFunc`** (the M1 side-stitch path, now also for `CFunc`): geomean 0.9998, text_adventure -4.1,
  real_dis_x86 -2.3, every focus program 0.0. Stitching a side trace around a C call does not bring the code after it back into a trace.
* **S-C, side traces at the start snapshot of `rec_ret` traces** (`no_start_side` off): geomean 1.0012; astar_jumper -8.0, memo_cache -1.7;
  **cd +9.8**, expr_lang +11.0, json +6.3, list +1.6. The down-recursion exits need a link to the other return trace, not a new side trace
  that re-records the return chain.

The ten reasons, largest first (sum of the tier-1 share over the ten programs, section 2.1):

| # | reason | weight (10 programs, % sum) | LuaJIT there | handle | soundness concerns | effort | ceiling |
|---|---|---:|---|---|---|---|---|
| 1 | headless callee under a tier-1 caller (`vec`, leaves) | 46.4 | interprets them too when the caller is interpreted (raytrace: 32-41 % interpreted), at a far lower cost per call | fix the caller (rows 2-5); and the per-call cost of tier 1 for the idioms that stay (`NEWTABLE` + new-key `SETFIELD` + `setmetatable` C call: ~1700 instructions per `vec` in tier 1, STUDY_ALLOC_CALL 3) | tier-1 templates, `__metatable` / `__gc` / hooks keep the slow path | M | raytrace (20): `vec` + `V:*` 20 % [M share]; halving tier-1 cost per call = -10 % [E] |
| 2 | `InnerLoop` (outer head reaches a callee with a loop) | 31.8 | ends the outer trace with a link to the inner loop's root trace; the inner trace's loop exit gets a side trace that continues in the callee, returns to the lower frame and reaches the outer loop's head | outer trace links into the inner root (K4 `preplink` did that for a `FORPREP`) **plus** side traces of the inner loop's exit that return into the caller (RETF from a loop root's side: exists, `retf` bit 0) and end at the outer head | the outer frame lives in the inner trace's snapshots (frame link), lazy frames (`retf_nolazy` refuses lazy-aware callers) | L | `nest=2`: deltablue 0, raytrace +0.1..0.5, cd -1.5..-2.3, list +64..74 [M]; K4 `preplink` 0 [M, section 27.4]: **not reachable with the existing pieces** |
| 3 | `FnShort` leaf samples | 17.4 | no minimum length for function traces | probe-not-penalty for leaf samples (STUDY_COVERAGE K2 extended `selfref`; non-recursive leaves still count) | more short traces, more compile | S | `fnmin=0`: deltablue L -2.0, richards L -1.4, havlak L +0.6, richards def +1.9, towers def +4.4 [M] |
| 4 | down-recursion `rec_ret` start exits | 12.4 | return traces chained by `RETF` guards; a guard failure gets a side trace that links to the other return trace | a link from the start exit of one `rec_ret` trace to the sibling trace of the other return pc (a chained cell, as E6 `dlink` does for exits of recursion families) | the frame below must be complete (lazy frames), the return value / slot layout of both traces must agree | M-L | cd: 9.9 % tier 1 + callees [M share]; S-C naive sides **+9.8 %** [M] |
| 5 | exit at the root `RETURN`, side `LeaveFrame` | 10.0 | side traces return to lower frames with a return-pc guard | `retf` bit 2 for function-entry families with a selection rule that keeps the formation of the regressing programs | C38 (native recursion families), formation (roguelike) | M | `retf=7`: lua_lexer **-17.1**, scheme_interp -10.6, expr_compiler -9.5, deltablue L -2.0; regressions up to +112 [M]; S-A shows `proto_recursive` is not the selector [M] |
| 6 | `Depth` / `Recursion` in side recordings | 8.1 | also limited ("call unroll limit reached": towers 10, list 5, cd 22, permute 1939 aborts) | stitch at the limit (exists for `Recursion` / `Depth` via `side_stitch`); deeper limits | - | - | `maxdepth=8`: cd def +3.7, deltablue def +5.8 / L +1.6 [M]: not a handle |
| 7 | `SideShort` | 7.6 | no minimum side length | `fretry` (K3b) already | - | - | `sidemin=0,framemin=0`: lua_lexer -2.0, list +24..29, rest 0 [M]: not a handle |
| 8 | `MaxSide` | 7.0 | `maxside` 100 per trace | a family cap that grows with exit evidence (hot exits of a full family beyond `polyside`, which today only covers dispatch guards) | more side traces, more compile at small sizes, formation | S | `maxside=64`: richards L **-7.7**, oop_mixins -30.8, lua_lexer -3.2; cd def +5.1, expr_compiler +17.7, astar +10.6 [M] |
| 9 | exit at a root `TAILCALL` | 7.0 | records the tail call in any frame | H6 (root frame replaced by the callee) | section 20: large surface | L | section 20 proxy: raytrace -1.3, richards +0.5 [M, earlier]; the cost is the tier-1 callee behind it (row 1), not the transition |
| 10 | unsupported library calls / ops (`CFunc`, `Nyi(Call)`, `TableHash`, `Closure`) | 6.8 (suite: 2.6 of 8.5 points, the largest class there) | records most string / table library calls, stitches around the rest; `HREF` on any key; FNEW is NYI in LuaJIT too | per-builtin recording: `TableHash` for table keys (havlak `dfs`), `string.find` with a pattern (L3), `tostring` of strings / tables, `table.insert(t, pos, v)`, `string.char` n > 3, `table.concat` | per builtin: argument shapes, metamethods (`__tostring`, `__index` on key tables), aliasing for object keys | M each | S-B (side stitch at `CFunc`): 0.0 on all ten [M]; havlak `dfs` 4.9 % tier 1 + helpers [M share]; lua_lexer `is_digit` / number branch 1.9 % [M share] |

Not in the table but measured: `lazyci=0` (permute +20.6, towers +2.2, cd -1.5..-1.8): the lazy frames are worth far more than the RETF they
block.

## 4. How much of the gap each category explains

Instructions, default size (the size of the session ratios), ours [M] against LuaJIT's min-of-5 run [M] with LuaJIT split by its `-jp=v` time
shares [E: time share used as instruction share; LuaJIT's `Compiled` state includes the runtime helpers its traces call, which is why
classic_vector / havlak / lua_lexer show a negative or small "trace code" cell and a large "other" one: read trace code + allocation + other
together for those]. Each cell is M instructions and, in brackets, points of the instruction ratio (ratio - 1 = sum of the cells / LuaJIT).

| program | ratio | gap M | trace code (protocol + guards) | exits / snapshots (materialise) | allocation + GC | coverage / tier-1 residue (tier 1 + helpers + interpreter) | compile time (compiler + libc) | other (library, strings, table ops) |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| awfy/cd | 2.52 | 467 | +55 (+0.18) | +103 (+0.34) | -9 (-0.03) | +195 (+0.63) | +110 (+0.36) | +13 (+0.04) |
| apps/raytrace | 2.22 | 201 | +60 (+0.36) | +3 (+0.02) | -7 (-0.04) | +20 (+0.12) | +116 (+0.70) | +10 (+0.06) |
| corpus/lua_lexer | 2.32 | 2510 | +597 (+0.31) | +116 (+0.06) | -61 (-0.03) | +748 (+0.39) | +262 (+0.14) | +848 (+0.45) |
| awfy/richards | 1.46 | 56 | +2 (+0.02) | +2 (+0.02) | +1 (+0.01) | +30 (+0.25) | +20 (+0.16) | +1 (+0.01) |
| awfy/permute | 3.55 | 140 | +153 (+2.78) | 0 | 0 | -13 (-0.24): our tier 1 + compile 6 M against LuaJIT's 19 M outside `[JIT]` (its jp split is a lottery here) | (in the previous cell) | 0 |
| awfy/towers | 1.73 | 74 | +43 (+0.43) | +2 (+0.02) | 0 | +13 (+0.13) | +15 (+0.15) | +1 (+0.01) |
| corpus/classic_vector | 1.80 | 911 | +158 (+0.14) | +322 (+0.28) | +141 (+0.12) | +29 (+0.03) | +77 (+0.07) | +185 (+0.16) |
| awfy/deltablue | 1.48 | 105 | +25 (+0.11) | +1 (0.00) | +10 (+0.04) | +125 (+0.57) | -77 (-0.35) | +22 (+0.10) |
| awfy/havlak | 1.65 | 203 | -116 (-0.37) | +3 (+0.01) | +64 (+0.21) | +154 (+0.49) | +4 (+0.01) | +93 (+0.30) |
| awfy/list | 1.26 | 80 | +7 (+0.02) | +1 (0.00) | +1 (0.00) | +46 (+0.15) | +25 (+0.08) | +1 (0.00) |

The same at the large size (steady state; compile and warm-up amortised):

| program | ratio | trace code | exits | allocation + GC | coverage / tier 1 | compile | other |
|---|---:|---:|---:|---:|---:|---:|---:|
| cd (10) | 2.08 | +0.09 | +0.34 | +0.02 | +0.55 | +0.04 | +0.04 |
| raytrace (20) | 1.56 | +0.19 | +0.02 | -0.04 | +0.26 | +0.09 | +0.04 |
| richards (250) | 1.38 | +0.05 | 0.00 | 0.00 | +0.27 | +0.05 | +0.01 |
| permute (1000) | 4.07 | +3.64 | 0.00 | 0.00 | -0.18 | -0.39 | 0.00 |
| towers (450) | 1.59 | +0.59 | 0.00 | -0.10 | +0.05 | +0.05 | 0.00 |
| deltablue (200) | 1.93 | +0.12 | 0.00 | -0.05 | +0.86 | -0.12 | +0.12 |
| havlak (30) | 1.73 | -0.26 | +0.01 | +0.10 | +0.53 | -0.01 | +0.37 |
| list (3500) | 1.18 | +0.04 | 0.00 | +0.01 | +0.11 | +0.01 | 0.00 |

Reading (the dominant category per program):

* **raytrace: compile time** at the bench size (+0.70 of 1.22; 36.7 % of our instructions compile), coverage at steady state (+0.26).
  Note LuaJIT interprets 32-41 % of raytrace and its interpreter does that work in fewer instructions than our tier 1 does the residue.
* **cd: coverage** (+0.63: the down-recursion of `put` and the `MaxSide` family) and **materialisation** (+0.34, steady at both sizes:
  H5), then compile at the bench size (+0.36).
* **lua_lexer**: coverage (+0.39; RETF of `Lexer:next`'s returns, `retf=7` -17.1 % [M]), trace code + library + strings together (+0.76;
  LuaJIT's library / string helpers sit in its `Compiled` bin), compile +0.14.
* **richards**: coverage (+0.25..0.27: `MaxSide`, `FnShort`, tail-called task bodies) and compile at the bench size (+0.16).
* **permute / towers: trace code** (recursion protocol, STUDY_COVERAGE 4 and STUDY_ALLOC_CALL fib: protocol ~50 %, array element traffic
  ~31 %, method lookups ~13 % of permute's per-call excess [M earlier]); towers' small tier-1 and compile shares add +0.13 / +0.15 at the bench
  size.
* **classic_vector: allocation and materialisation** (exits +0.28, allocation + GC and LuaJIT's helper-inclusive `Compiled` bin together
  +0.42): not coverage.
* **deltablue, havlak: coverage** (+0.49..0.86: `InnerLoop`, `FnShort`, `TableHash`), deltablue's compile at the bench size is cheaper than
  LuaJIT's (LuaJIT spends 70 % of its time compiling deltablue at the default size and its min run varies 219-296 M instructions).
* **list**: coverage (+0.15: recursion limits and the inlined `isShorterThan` loop) and compile (+0.08).

Guards / snapshots inside the trace code (as opposed to exits) are not separable from the protocol with these profiles: the trace-code
column holds both. permute's split from the earlier per-call decomposition is the best available [M earlier].

## 5. Ranked handles

Gains are instructions of the named program (the study measured no cycles); "ceiling" = a knob / scratch measurement, otherwise
arithmetic from the shares. Previously tried items are flagged so they are not re-proposed without new evidence.

| # | handle | expected gain (instructions) | risk | effort | status / evidence |
|---|---|---|---|---|---|
| T1 | **RETF for function-entry families, with a selector** (**built**, docs/JIT_TRACE.md 39.1: `retfsel`, default on; lua_lexer -7.6 % instructions / -6.7 % cycles; the paying families are loop families of prototypes with a function-entry head; selector = caller's loop around the call traced and no recursion popped) (`retf` bit 2 per family instead of global): the side trace at a function root's `RETURN` pops the frame and goes on in the caller | lua_lexer **-17 % (ceiling M)**, scheme_interp -11, expr_compiler -9, queens -16.5, json_rxi -6, deltablue L -2, havlak L -0.8, cd L -0.9 | medium-high: ungated regressions up to +112 % (roguelike, formation), list +56 (C38), lua_interp +35, regex_nfa +20; `proto_recursive` / `selfref` is not the selector, and "loop-root families only" keeps none of the gain (S-A, M). Candidate selectors to measure next: refuse when the caller's loop head has a root trace whose family would be re-formed; per-exit retry (`fretry`-like) with a revert when the family's entries drop | M | new; `retf` bit 2 was turned off in C38 for native recursion, this study shows the non-recursive half is worth -6..-17 % on 7 programs |
| T2 | **Compile cost at small sizes** (**partly built**, docs/JIT_TRACE.md 39.3: four allocation / scan fixes with identical code; raytrace -5.0 %, deltablue -1.8, cd -1.4 % instructions; the recorder per-bytecode path, the back end and the tier-1 encoder remain): fewer instructions per recording / pipeline (raytrace ~2 M per trace) and less work on recordings that abort (`TooLong` / `snaps`: raytrace records 12 k bytecodes for 7 installed roots) | ceiling = the compile share at the bench size: raytrace 36.7 %, cd 26.5, deltablue 24.9, richards 22.7, towers 15.5 [M]; halving it: -8..-18 % on those five [E] | low (no generated-code change if done in the compiler) / medium (if it changes which recordings are started) | M-L | new as a target; S1 `lazystub` (built) cut cd's stub assembly only (-4.6 % instructions); profile split: pipeline 16.4 / libc 8.6 / recorder 6.1 / tier-1 lowering 1.8 % (raytrace default) |
| T3 | **`MaxSide` grown by evidence** (**built**, docs/JIT_TRACE.md 39.2: `heatside`, default on; oop_mixins -28.8 %, vending_fsm -5.8, heapsort -4.6, regex_nfa -4.3, lua_lexer -3.1, richards 250 -7.4 %; richards default size +4.7 % cycles): a full family may attach a side trace to an exit that kept coming back (count threshold), not only to dispatch guards (`polyside`) | richards L **-7.7 % (ceiling M)**, oop_mixins -30.8, vending_fsm -5.3, heapsort -4.8, lua_lexer -3.2 | medium: `maxside=64` costs at the bench size (cd +5.1, expr_compiler +17.7, astar +10.6, lru_cache +5.1): the extra side traces must be paid by exits, which a count threshold expresses | S | new |
| T4 | **Down-recursion links** (**built differently**, docs/JIT_TRACE.md section 46: the sibling is already linked through the `rec` cells; trace 19's head pc 43 is the continuation after `isRed(h.right)`, not the left return (pc 25); the start exits are the bottom frame's return into `RBT:put`, a caller outside the recursion. Built: the native-return guards at the end of the trace (`t4` bit 0) and side traces at the start snapshot (bit 1); cd 10 -13.2 % instructions / -12.3 % cycles, default off: bar missed on 5.4 expr_lang (lottery) and two cycle readings): the start exit of a `rec_ret` trace links to the sibling return trace (left/right return sites of `put`) | cd: up to the 9.9 % tier-1 share of `put` plus callees (~15 % of cd's instructions at the large size [E]) | medium-high: lazy frames, slot layouts of the two traces | M-L | the naive form (side traces there, S-C) is **+9.8 %** on cd [M] |
| T5 | **`TableHash`: `GETTABLE` / `SETTABLE` with a table (object) key** in traces (pointer hash `HREF`) | havlak: `dfs` 4.9 % tier 1 + part of 7.4 % helpers / 16 % table ops -> -4..-8 % [E]; suite `TableHash` 0.15 points | medium: key identity, `__index` / `__newindex` on the keyed table, rehash | M | new |
| T6 | **Tier-1 per-call cost of the residue that stays** (constructor idiom, `setmetatable` C call, new-key `SETFIELD`, leaf calls) | raytrace: tier 1 + helpers 37.7 % at L; LuaJIT does the same interpreted region in ~0.55x [E]; -10 % raytrace if halved [E] | low-medium (template fast paths with the helper as fallback) | M | partly done (STUDY_ALLOC_CALL H2); E9 items 2/3 below 1 % (not re-proposed) |
| T7 | `FnShort` probes for non-recursive leaves (a leaf sample does not count toward the blacklist until a non-leaf sample was seen or N samples passed) | deltablue L -2.0, richards L -1.4 (ceiling `fnmin=0` M); towers def +4.4 with `fnmin=0` | low-medium | S | K2 did method recursion; `fnmin=0` measured in STUDY_COVERAGE too |
| T8 | Library calls: `string.find` with a pattern (L3), `tostring` of strings / tables, `table.insert(t, pos, v)`, `string.char` n > 3, `table.concat` | lua_lexer -2..-4 [E, earlier]; suite: CFunc + Nyi(Call) 2.6 of the mean 8.5 tier-1 points, concentrated in the `lib/` micro programs | low each | M (sum) | L3 estimated earlier; side stitching instead is worth 0 (S-B, M) |
| T9 | `InnerLoop`: outer trace linked into the inner root + inner-exit sides that return to the caller and end at the outer head | deltablue 15 % tier 1, raytrace 6, havlak 6, list 4 [M shares] | high | L | **tried**: K4 `preplink` (0), `nest=2` (0 / list +74 %): needs both halves at once; not before T1 (which supplies the return half) |
| - | root `TAILCALL` (H6) | 0..3 % | high | L | **tried** (section 20) |
| - | side `sidecut` / `preplink` | 0 / +2.8 % | - | - | **tried** (K4, section 27.4) |
| - | `lazystub` | -2..3 % cycles cd | - | - | **built** (S1: 1.6-2.6 %, not the 6-10 % estimated) |
| - | E9 items 2 / 3 (call-site IC, native fn_end return) | < 1 % | - | - | **measured, not built** (section 37) |
| - | `sidemin=0`, `maxdepth=8`, `nest=2`, `lazyci=0`, side stitch at `CFunc`, naive `rec_ret` sides | 0 or worse | - | - | **measured here**, not handles |

Recommended order: **T1** first (largest measured ceiling, M effort, a knob exists: the work is the selector and its regression set:
roguelike_turns, list, lua_interp, regex_nfa, sudoku_solver, astar_jumper, nbody, deltablue), then **T3** (S, measured -7.7 % on richards and
-31 % on oop_mixins; the same "pay with evidence" rule as T1's retry), then **T2** (the bench-size compile share is the largest single
category for raytrace, and a profile of the pipeline per trace is the first step; no formation risk if it only makes the compiler cheaper),
then **T5** (havlak) and **T7** (S). T4 and T9 are L and should wait until T1 has shown how RETF behaves for function roots; T6 / T8 are
independent and can be done by a second builder. Expected position after T1-T3 [E]: lua_lexer 2.32 -> ~1.95, richards (250) 1.38 -> ~1.27,
raytrace / cd / deltablue at the bench size -5..-15 % if T2 halves compile; permute / towers / classic_vector unchanged (trace code and
allocation, not coverage).

Gate: `gate-light.sh` (CPUs 10-15) on the diagnostic change: fmt, clippy x3, core x2, trace suites 5.5 / 5.4, `vm_corpus`, jit-diff 1-50,
mutant group PASS; `mutant-map` failed only on the shifted line numbers of 11 injection sites below `exit_dump` (2 lines added), the map is
regenerated (`gen-mutant-map.py --check`: 792 mutants ok). Scratch builds S-A / S-B / S-C and the loop-root selector were built in the same
worktree, measured from copies of the binary, and reverted; none is in the commit.

## 6. Reproducing

```
# one program: clean instruction count, merged clean profiles, one diag profile -> clean.txt / diag.txt / a.json
RUSTYNAIL_SEED=0 RUSTYNAIL_JIT_PERFMAP=2 perf record -e instructions:u -c 1000000 -o a.data rnlua prog.lua
RUSTYNAIL_SEED=0 RUSTYNAIL_JIT_PERFMAP=2 RUSTYNAIL_JIT_VERBOSE=2 RUSTYNAIL_JIT_EXITDUMP=1 perf record -e instructions:u -c 1000000 -o d.data rnlua prog.lua 2> d.log
perf report -i a.data --no-children --sort dso,sym --stdio > a.rep     # (keep /tmp/perf-<pid>.map of the d run as d.map before deleting it)
scripts/tier1-residue.py a.rep d.log d.map --json a.json
scripts/residue-aggregate.py DIR...            # per program; --suite for the mean over many program directories
```

Pin to CPUs away from timing runs and keep `-c` at 1000000 or more (throttling, section "Method"). Instruction counts depend on the binary
path and the environment length (pad knob strings to equal length, as the knob tables here did).

## 7. Handle status (2026-11-22)

T1, T3 and T2 were built in that order (docs/JIT_TRACE.md section 39, commits on top of 23d1692): T1 `retfsel` and T3 `heatside` ship on by
default, T2 is four compile-cost fixes that keep the emitted code identical. The open question of T1 is answered there (39.1): `retf=7`'s
lua_lexer gain comes from `Lexer:next`'s **loop** families (pc 1 and 99), which count as function-entry families because the prototype also has a
function-entry head; the study's loop-root scratch selector should have kept them. The losers of `retf=7` are pops inside a recursion, pops into a
caller loop without a root trace and a first-iteration `Unstable` lottery (roguelike_turns, cured in a scratch build by TR24c in-place restarts).
T4-T9 are unchanged.

T6 and T8 were built on 2026-11-23 (docs/JIT_TRACE.md section 44, on top of fdc8eda): T6 is the fused tier-1 constructor (`t6` bit 0, `vec(i, i, i)` in tier 1 935 -> 798 instructions,
the 1700 of this study being the J4 `RUSTYNAIL_JIT_TRACE=off` figure) and the mixed integer / float `math.min` / `math.max` (`t6` bit 1); together raytrace -6.6 % instructions. T8 turned out to be the pattern `string.find` /
`string.match` (the recorder aborted `Nyi(Call)` at them: lua_lexer -16 %, lua_parser -13 %, expr_compiler -6.5 %, expr_lang -9.6 % on 5.5), the other library calls of the handle were measured and not built. The "wasted
recordings" of section 43 became the knob `blfirst` (raytrace -4 %).
