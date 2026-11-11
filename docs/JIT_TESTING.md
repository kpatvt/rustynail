# JIT differential testing harness

> Since BUNDLE W the `jit` feature is a *default* feature of the `rn-cli` package (`rnlua`). A plain `cargo build -p rn-cli`
> therefore has the JIT; where a script or a test means "without the JIT" (baselines, text-identity checks, the non-JIT PGO
> build) it passes `--no-default-features`, and J4-only builds use `--no-default-features --features jit_j4`. The `rn-jit` and
> `rustynail` crates keep their opt-in `jit` features. Binaries are `rnlua`/`rnluac` (older logs call them by their pre-rename names).

Source-level random program differential testing for the JIT (JIT_PLAN.md section 5). The harness
is independent of the JIT: until the JIT exists, `RUSTYNAIL_JIT=off` and `=always` are the same
interpreter, and the harness is validated against the reference Lua 5.5 build instead.

Files:

- `tests/jitdiff/gen.lua`: seeded program generator (runs under rnlua or the reference lua).
- `scripts/jit-diff.sh`: driver (generate, run in several modes, compare, minimise, summarise).
- `tests/jitdiff/failures/` (gitignored): kept failing programs and their outputs.

## Usage

    scripts/jit-diff.sh <seed-from> <seed-to> [bin]      # bin defaults to the main checkout's target/release/rnlua

    # the generator alone
    rnlua tests/jitdiff/gen.lua 42 > p.lua             # plain
    rnlua tests/jitdiff/gen.lua 42 count > p.lua       # hook variant: count | line | both
    rnlua tests/jitdiff/gen.lua 42 "" 2,5 > p.lua      # keep only snippets 2 and 5 (minimisation)

The driver re-executes itself under `systemd-run --user --scope -p MemoryMax=8G -p MemorySwapMax=0`, runs
workers with `ulimit -v 6000000`, `timeout $TMO` (default 20 s) and `</dev/null`. Temp files go under
`$TMPDIR` (default `/tmp`) and are deleted afterwards (`KEEP=1` keeps them). About 40 seeds per second
with 3 jobs on the three-mode configuration below.

Modes (each program runs once per mode; the first mode is the baseline; stdout, stderr and the exit
code are compared; `0x...` addresses and the `rnlua:`/`lua:` program-name prefix are normalised):

| mode    | environment                                           |
|---------|-------------------------------------------------------|
| off     | `RUSTYNAIL_JIT=off` (baseline)                        |
| always  | `RUSTYNAIL_JIT=always`                                |
| on2/on10| `RUSTYNAIL_JIT=on RUSTYNAIL_JIT_THRESHOLD=2` / `=10` (`JITDIFF_ON=1`) |
| stack   | `RUSTYNAIL_JIT=always RUSTYNAIL_JIT_STRESS=stackmove` (`JITDIFF_MODES="off always stack"`; every helper moves the stack) |
| ref     | baseline settings, binary `$JITDIFF_REF` (reference lua; `ref/55/build/lua` in the main checkout) |
| t15     | `RUSTYNAIL_JIT=always RUSTYNAIL_JIT_T15=always RUSTYNAIL_JIT_OPT=+force` (tier 1.5 regions for every hot nest) |
| t15flush, t15rand | `t15` with `+flushall` / `+randra=<seed>` |
| t15verify, t15verifyrand | `t15` with `+verify,+flushall` / `+verify,+randra=<seed>` (tier-1 type-knowledge checks at every region exit, see JIT_TIER15.md 17) |
| t15on | `RUSTYNAIL_JIT=on THRESHOLD=2` + T15 always `+force` |
| t15prof, t15profrand | `RUSTYNAIL_JIT=always RUSTYNAIL_JIT_T15=on RUSTYNAIL_JIT_T15_HOT=4` `+force` (`+randra=<seed>`): the warm-up profile `prof[pc]` exists when the nest is promoted, so the typed result paths of the table accesses (JIT_TIER15.md 19) are generated; `t15*` with `always` promote at the first back-edge and see mostly no profile |

(Exporting `RUSTYNAIL_JIT_T15=always RUSTYNAIL_JIT_OPT=+force` around the script has the same effect on the JIT modes `always`,
`on2`, `on10`, `stack`; the baseline `off` ignores it.)

**Counter assertion (`JITDIFF_ENTERS=1`).** A differential run proves nothing when the compiled mode never ran compiled code (that is what
happened with the original hook variants: the hook was installed in the prelude, so compiled code was never entered). With this
switch every non-baseline mode is run once more with `RUSTYNAIL_JIT_VERBOSE=1`, the `[jit] ... enters N` and `[jit] t15: promoted N
recompiled N` lines are summed per (hook variant, mode) and printed (`enters: hook=... mode=... enters=... t15_promoted=...
t15_recompiled=...`); the script fails with `NOENTERS` when a mode never entered compiled code and with `NOREGIONS` when a `t15*` mode
never generated a region. Use it for every gate run; e.g. `JITDIFF_ENTERS=1 JITDIFF_HOOK=all JITDIFF_MODES="off always t15" scripts/jit-diff.sh 1 400`.

Environment knobs: `JITDIFF_HOOK=none|count|line|both|all`, `JITDIFF_ON=1`, `JITDIFF_ENTERS=1`, `JITDIFF_REF=<lua>`,
`JITDIFF_MODES="off always verify"`, `JITDIFF_GEN=<bin>`, `JOBS` (default 3), `TMO`, `KEEP`, `MINIMIZE=0`.
The exit status is 0 iff every seed finished and no mode diverged. Generator problems are reported
separately (`GENFAIL`, `GENSYNTAX`, `TIMEOUT`).

On a divergence the driver stores `tests/jitdiff/failures/seed-<n>-<hook>-<mode>/` with the full program
(`p.lua`), all outputs (`out.*`, `err.*`, `rc.*`) and a minimised program `min.lua` (greedy removal of
whole snippets; `min.keep` lists the surviving snippet numbers; its outputs are in `min/`).

## What the programs contain

Each program is 5-9 snippets out of 17 templates, each run via `run(name, f, ...)` (a `pcall`), which
prints the running checksum after every snippet. The checksum `H` folds integers, floats (`%.17g`, so
`-0.0`, nan and inf are distinguished), strings, booleans and nil. Error messages (including
`file:line:` and variable names) are collected, sorted and printed (`ERRMSG` lines), so line info and
message text are compared, not only values. A third of the programs end with an uncaught error or
`os.exit(...)`, comparing stderr traceback and exit codes.

Templates: random integer/float/mixed expression trees evaluated under per-iteration `pcall`
(div/mod by zero, shifts, bitwise on non-integral floats, string coercions); float loops; numeric `for`
edge cases (overflowing bounds, float and zero steps, nan/inf limits, non-number operands); while,
repeat, goto, nested loops; array/hash/mixed int-float keys, `table.insert/remove/sort/concat`;
string library and number formatting; closures, upvalues, varargs, multiple returns, tail calls;
bounded recursion plus caught stack overflow; `error`/`pcall`/`xpcall` with non-string values and levels;
coroutines yielding in loops (also across `pcall`, `wrap`, `close`); metamethods (`__add`, `__index`,
`__newindex`, `__eq`, `__lt`, `__le`, `__concat`, `__call`, `__len`, `__close`, bitwise); bitwise and
shift edge cases; a values x operators matrix over special constants (minint, maxint, nan, -0.0, 2^53,
2^63, inf, strings, nil); tbc variables and iteration; float-key normalisation.

Loops run 200-5000 iterations, so they become hot with default thresholds; hook variants divide counts
by 5. Programs allocate well below 10 MB and finish in under 100 ms. The generator uses its own
xorshift PRNG (not `math.random`), no `pairs` order, addresses, `%p`, `os.time` or `os.clock`.

## Hook-semantics variant

`JITDIFF_HOOK=count|line|both|all` installs a prelude hook (`HF`) with a count hook (1-400 instructions), a
line hook, or both (count 50-3000). The hook records the number of count events, line events and a hash of
`getinfo(2,"l").currentline` / line numbers; they are printed after each snippet and at the end
(`hooks` line). Hooks are inherited by coroutines. The JIT must reproduce exact instruction counts and
line sequences. (The reference interpreter and rnlua agree on these, validated below.)

**Compiled code is never entered while a hook is installed** (JIT_DESIGN, "Hooks active"). Until T12 the prelude hook stayed
installed for the whole program, so the `hook=count|line|both|all` runs compared the interpreter with the interpreter
(`enters 0`); the "400 seeds `hook=all`" results quoted in JIT_TIER15.md 16.5 and above therefore tested hook *semantics* of
the interpreter only, nothing about compiled code with hooks. What makes the variants honest now:

* `run()` alternates: the prelude hook is installed for the odd snippets and removed (`debug.sethook()`) for the even ones, so every
  program has snippets that run compiled code (counted by `JITDIFF_ENTERS`) next to snippets whose hook counters are compared.
* the T12 templates install and remove hooks **in the middle of a hot loop**: from the loop body, from a callee, from a `__gc`
  finalizer running inside `collectgarbage()`; count / line / call / return masks; the snippet is run several times so the second
  run enters compiled code and installs the hook mid-run again. They restore the prelude hook with `rehook()`.

## Validation (before the JIT existed)

rnlua vs reference `ref/55/build/lua` (stdout, stderr, exit code): seeds 1..2000 plain, and seeds
1..2000 in each of the count/line/both hook variants: no divergences (see the commit message / task
report for the final numbers). `ref/54` is expected to differ, which the driver detects and minimises
(used as a self-test of the failure path).

## T12 templates (mid-run state changes)

Seven templates added for tier 1.5 (their bodies are in `tests/jitdiff/gen.lua`, section "T12"): hooks installed/removed mid-loop
(loop body, callee, finalizer); `debug.setlocal` from a callee or from a hook function changing a local's type (int, float, string,
nil, boolean, table, -0.0, 2^40, maxinteger) in the middle of a loop (`setl(lvl, name, v)` finds the local by name); `debug.upvaluejoin`
/ `debug.setupvalue` aliasing a captured local while its frame runs; `coroutine.create/resume/yield` inside hot loops with a type change
in the middle (yield period 1, 7, 1000); type-unstable accumulators alternating int/float/string/nil every K iterations
for K in 1, 7, 1000; numeric `for` loops whose bounds and step are variables of changing type (floats, numeric strings, zero step,
nan, inf, 2^63 neighbours) and integer-overflow loops.

## Template tests (J1b)

- `crates/rn-jit/tests/arith_diff.rs`: for every arithmetic / bitwise / comparison / test template a tiny Lua function is
  compiled, run by the interpreter and by the emulator (and the machine code) on pairs of special values (int min/max,
  2^53 neighbours, NaN, inf, -0.0, 2^63, nil, booleans, numeric and non-numeric strings). A template may exit (arguments
  untouched, `savedpc` inside the function) but when it completes it must give the interpreter's result; integer and float
  pairs of the plain operators are not allowed to exit.
- `crates/rn-jit/tests/jit_arith.rs`: whole programs in `always` mode against the jit-off twin, also with small
  instruction budgets (count exits at block heads), hooks, coroutines, metamethods and error messages.
- `crates/rn-jit/tests/emu_diff.rs`: Miri-runnable emulator checks (ADD, count refunds) and native vs emulator.
- `scripts/jit-diff.sh` with `JITDIFF_ON=1`: 3300+ seeds plain and 500 seeds in all hook variants.

## Hardening tests (J3c)

- `crates/rn-jit/tests/jit_j3c.rs`: each `tests/j3c/*.lua` (prefixed by `pre.lua`, which supplies `P`, `DEPTH`, `OVF`, `SMALL`) runs in
  fresh states with the JIT off and in `always` mode (plus count budgets 3/17/200); outputs must match. Debug interface (`dbg_*`), stack
  growth/shrink (`stack_move`), C-stack depth (`cstack`), coroutine stress (`coro_stress`, `coro_gc`), collector modes (`gc_modes`).
  Run the binary also with `RUSTYNAIL_GC_STRESS=1|2` and `RUSTYNAIL_JIT_STRESS=stackmove`; the test lowers depths/counts itself
  when those make deep recursion quadratic. `J3C_DUMP=<substring of test name>` prints the reference output.
- `crates/rn-jit/tests/jit_j3c_env.rs`: child-process runs under `PR_SET_MDWE`, a seccomp filter, `RLIMIT_AS`; code cap tests
  (`rn_jit::set_code_cap`).

## Fuzzing (J3a)

Targets in `fuzz/` (separate nightly workspace, ASan on; see CLAUDE.md). The JIT is behind the fuzz crate's cargo
feature `jit` (`scripts/jit-fuzz.sh build` = `cargo +nightly fuzz build --features jit`). The compiler is registered
once per process in `always` mode; a state is interpreted or compiled according to `rn_jit::set_mode`.

- `jit_diff`: input bytes -> Lua program -> run in two states of the same process (interpreter only, `always`),
  compare the `emit` log (type-tagged values, floats bit-exact, addresses normalised), the status and the error
  text. Config byte: bit 0 generational GC, bit 1 GC stress, bits 2-3 budget, bit 4 open `debug` (hooks, getinfo,
  traceback, getlocal), bit 7 raw source instead of the grammar. The grammar (`fuzz/src/gen.rs`) covers numeric and
  generic `for` (float/zero/overflowing steps, expression bounds), while/repeat/goto/break, arithmetic, bitwise,
  comparison, concat, tables, table library, closures/upvalues, varargs, multiple returns, tail calls, bounded
  recursion, pcall/xpcall/error, coroutines (wrap/create/resume/yield/close), all arithmetic/index/call/close
  metamethods, tbc variables, string library calls. Every input byte string is a valid description.
- `run_source --features jit`: unchanged sandbox; bit 7 of the config byte runs the state compiled.
- Seeds: `fuzz/make_jit_seeds.sh` (also called by `fuzz/make_seeds.sh`) writes `fuzz/corpus-jit/`: random bytes for
  the grammar, `tests/jitdiff/gen.lua` programs and `bench/programs` as raw seeds, the same plus
  `tests/vm`, `tests/lua/gen` for `run_source`.
- Driver: `scripts/jit-fuzz.sh build|run <target> [secs]|repro <target> <file>|lua <file.lua>` (memory capped,
  `</dev/null`); `scripts/jit-minimize.py` (line ddmin of a diverging raw program; `JIT_DIFF_TICKS=n` replays the
  grammar budget). Triage aids: `JIT_DIFF_DUMP=1` prints program, log and JIT stats; `JIT_DIFF_OUT=dir` writes
  `src.lua interp.log jit.log`.

### Budget: the plan's "JIT-aware budget"

`run_source`'s budget is a count hook, but **compiled code is never entered while a hook is installed**
(JIT_DESIGN, "Hooks active"): with the hook the JIT run silently degenerates to the interpreter (first version of
this target: `enters: 0`). So there is no instruction-count budget under compiled code that is comparable. Instead:

- grammar programs call `tick(site)` at function entry and in loop bodies (always in expression-bounded and generic
  loops); after N ticks the native raises an uncatchable error naming the site. N is deterministic and identical
  in both modes, so the point where the budget runs out is compared exactly (as program state, not as instruction
  counts); C1 exact counting (count hook inside compiled code) is therefore not exercised by this target. It is
  covered by `jit_skeleton.rs`/`jit_arith.rs` and `scripts/jit-diff.sh` hook variants.
- raw programs (and `run_source` with bit 7) cannot carry ticks: a watchdog thread sets the interpreter's
  `INTERRUPT` flag (polled by compiled back-edges as well) after 40-400 ms; in `jit_diff` runs it interrupted
  are not compared.

### Harness pitfalls found while fuzzing (not JIT bugs)

- `pairs`/`next` order depends on addresses when function/table keys are present, and assigning new keys during a
  traversal is undefined: the grammar traverses a sorted snapshot of the scalar keys.
- Strings containing addresses (`tostring(f)`) are normalised to `0xADDR`.
- Generated names must not collide with the prelude (`f1` recursing through `local function f1`).
- The tick budget must stay exhausted: a budget error swallowed at a coroutine boundary used to reset the counter to
  "unlimited", so the rest of the program (e.g. `for i = -1e308, 32, 2`, or `ipairs` over a table that grows) ran until
  the 20 s watchdog (slow units of 20 s, ~500 MB RSS, an `-rss_limit_mb` OOM after 30 min). Now a 500-tick grace, then
  the error is raised again.
- `run_source` (ASan, `-rss_limit_mb`): deep-stack overflow inside a message handler (`xpcall(nil, function() pcall(rec) end)`)
  takes 10-30 s compiled vs 0.4 s interpreted under the instrumented build, but 0.1 s in both modes with the release
  `rnlua`; instrumentation cost, not a JIT behaviour difference (outputs identical).

### Findings

See `tests/jit_fuzz_repros/` (each `NAME.lua` has `NAME.expected`, the interpreter/reference output).

- `tfor_cstack_limit.lua`: compiled generic `for` over an inlined `next`/`pairs` does not account C stack depth
  (`nCcalls`) like `OP_TFORCALL` (interpreter and reference: "C stack overflow" near 200 nested levels). JIT bug,
  low severity (observable only at the C-stack limit).

Note (T12): the `stack` mode (`RUSTYNAIL_JIT_STRESS=stackmove`) is quadratic on the old recursion template (`pcall(deep, 150000)` moves the
stack at every helper call): 25 of 300 seeds exceeded the default 20 s `TMO` there (rc 124, empty output; the programs are fine and
finish in 20 s to several minutes). Run that mode with `TMO=300` or accept those timeouts; this is unrelated to tier 1.5.

## T7 templates and policy modes

Two templates added for the tier 1.5 policy (JIT_TIER15.md 21): (1) *live-in flips between calls* - a function whose live-ins (a number, a table, a
key) change type every K calls for K in 1, 7, 1000, over numbers, numeric strings, array and hash tables and int / float / subnormal / string keys, so
that the entry speculation of a region fails until the policy retargets and widens it; (2) *thrash and give-up* - four live-ins change type in turn
(each phase invalidates the previous version) next to an array whose element types flip with period 1..9. All `t15*` modes run with
`+exithot=3` (a policy decision after three stub failures instead of 256), so respecialisations and give-ups happen inside these small programs;
`JITDIFF_ENTERS=1` prints `t15_respecialised` / `t15_given_up` per mode next to the promotion counters (a `t15` mode with zero respecialisations over
many seeds means the templates stopped exercising the policy).

## T6 templates (calls and intrinsics in regions)

Seven templates added for tier 1.5 task T6 (`tests/jitdiff/gen.lua`, section "T6"): every inline `math` function on the special operands (0, -0.0, +-0.5, 2^53, 2^62, 2^63, -2^63, huge, inf, NaN,
`mininteger`, `maxinteger`) with the callee passed as a parameter (one call site sees several functions); the callee replaced under the loop (an upvalue rebound, `math.floor` replaced
from the body, `math.min` rebound); call / count / line hooks installed in the middle of a loop of inline calls (the hook counters are compared: the interpreter must see every call);
operands that become strings, nil, tables, booleans mid-loop (coercions and error messages); Lua callees whose result type changes at iteration K (float, integer, string, nil) next to typed
arithmetic; a coroutine yielding from a loop of intrinsics; and one that compiles chunks with `load`, each calling the functions on *literal* operands (so that the facts at the call are typed even in the first call). Profiled modes (`t15prof`, `t15profrand`) are the ones that generate the inline forms (the call-site profile exists only after warm-up).


## T8 verification modes (shadow, exit storm, stress) 

Tier-1.5 option strings (`RUSTYNAIL_JIT_OPT`, docs/JIT_TIER15.md section 22): `+shadow` (tier-1 emulation of pure region blocks, aborts on a mismatch),
`+exitstorm=N` (N in {1,2,7,61}: pseudo-guards failing every N-th execution), `+randra=SEED`, `+flushall`, `+verify`.

- **jit-diff modes** (`JITDIFF_MODES="off t15shadow t15storm ..."`): `t15shadow`, `t15shadowrand` (+randra=seed), `t15storm1|2|7|61` (fixed N), `t15storm` (N and
  randra chosen from the seed), `t15stormshadow` (storm + shadow + verify), `t15stormprof` (T15=on, hot 4, storm + shadow). With `JITDIFF_ENTERS=1` a
  `*shadow*` mode that never shadow-verified a block fails (`NOSHADOW`).
- **`cargo test -p rn-jit --features jit --test t15_stress`** (opt-in, `test = false`): the whole twin corpus (about 300 programs: fixed, mid-run mutation,
  random int / float, table, cross-class, intrinsics) under ~25 option combinations, with and without a warm-up profile, and under count budgets; output
  and `Stats::insns` must equal the jit-off run. Also run it under `RUSTYNAIL_GC_STRESS=1|2` and `RUSTYNAIL_JIT_STRESS=stackmove`.
- **Fuzz** (`fuzz/fuzz_targets/jit_diff.rs`): config bits 5-6 select tier 1.5 (off / always / profiled / always+verify); a second byte then selects storm period,
  shadow and a randra seed. `fuzz/make_jit_seeds.sh` writes such seeds.
- **Mutation matrix:** `scripts/t15-mutants.sh` (which tool detects which mutant; table in JIT_TIER15.md section 22).
- **Scripts:** `scripts/ci-local.sh` has the `jit-t15-*` steps; `scripts/asan-jit.sh` runs the corpus with T15 (+shadow, +exitstorm=7); `scripts/miri.sh jit`
  also covers `t15::alloc` and the region generator in the emulator (`t15_core_emu`).
- Event-stream comparators (`regions_with_helper_calls_match_tier1_at_every_pc_and_exit`, table equivalent) cannot be combined with `+exitstorm`: the storm
  changes which pcs are reached in region code by design; they compare final state and helper logs only in `regions_match_tier1_emulated_and_native`.

## J5 trace tests (P8a)

The trace compiler (`crates/rn-jit/src/tr`, docs/JIT_TRACE.md) has four whole-system checks on top of the per-task unit
suites. All of them re-execute their own test binary as child processes (`TR_*` environment variables select the job), run
under the memory cap of CLAUDE.md, and share `tests/tr_p8a/common.rs` (corpus, child runner, state construction for the
trace modes, the interpreter fetch counter of `tr_identity_pre.rs`). Corpus: `tests/tr_oracle_progs` (review repros),
`tests/t15_oracle_progs`, `tests/vm`, `bench/programs` and `bench/jit-trace` at small N, the programs of
`tests/tr_core/progs.rs`, and jit-diff generator seeds (`TR_P8A_GEN`, default 8-12). `TR_P8A_ONLY=<substr>` selects programs,
`TR_P8A_JOBS` the number of parallel children, `TR_P8A_AS_MB` the address-space cap per child (default 2048),
`TR_P8A_TIMEOUT` the per-child timeout.

**The trace tests run the shipped configuration (K3a).** `TrLimits::default()`, the starting point of every test configuration, has the value of the process-default constant of every knob (`cache::*_PROCESS`),
so a test that names no limits runs what ships; a test that wants a rule's OFF state (a differential oracle, or a head count that native recursion would change) names the knob in its limits string (`fnpromo=0`, `recnat=0,recany=0`, ...).
The three exceptions are start gates and an oracle: `sidemin` and `wgate` / `warm` are 0 in tests (the twin programs are tiny; `apply_process_limits` sets the shipped values in a process) and `lockstep` is 1 in tests (0 in a process).
`TR_CORE_PROCESS=1` (formerly the only way to run `tr_core` with the shipped knobs, 13 failures) is an alias that differs from the default in `lockstep` only. Details and the list of adjusted tests: docs/JIT_TRACE_REF.md "K3a".
Run `tr_core` with `-- --skip mutants_are_caught_by_live_programs`: that test (24 mutants, sequential) is the long pole and runs alone (see the mutant paragraph below).

```
systemd-run --user --scope -q -p MemoryMax=8G -p MemorySwapMax=0 timeout 3000 \
  cargo test -p rn-jit --features tr_mutants,t15_oracle --test tr_identity --test tr_oracle --test tr_mutants -- --nocapture </dev/null
```

- **`tr_identity`** (9.5): `fetches(JIT off) == fetches(run) + Stats::insns(run)` for every program x trace mode (`always`, `on2` =
  hot 2/2, `on` = JIT on with default thresholds) x option variant (`-fold -cse`, `+exitstorm=2|7`, `+randra=3`, `+membudget`,
  `-loop -fwd -abc -sink`) x budget {none, 3, 17, 200}, plus a sweep of every budget 1..=40 on the twin programs; and the
  no-overshoot rule: no compiled stay ends with a negative budget (`rt::stay_stats`, test-only counters under `t15_oracle`).
  Exceptions are explicit: `NONDET` (knucleotide) and the `G4` iterator-error programs (|diff| <= 16).
- **`tr_oracle`** (9.4): the interpreter-vs-trace-run state oracle. Run 1 logs `(clock, proto, pc, digest)` at every trace exit that
  reaches the Rust handler (`+emuverify` forces that) and at the first interpreter fetch after every compiled stay; run 2 (JIT
  off) must be at the same `(proto, pc)` with the same digest at that clock. The digest covers the slots that are live at each
  frame's resume point (tier-1 and the trace restore leave dead scratch slots different on purpose), strings by content,
  NaN canonicalised, and the `CallInfo` chain. Debug aids: `TR_ORACLE_DEBUG=1` (dumps the raw live slots of both sides),
  `TR_ORACLE_RANGE=lo-hi` (prints the fetch clocks of both runs), `TR_ORACLE_LEFTOVER=1` (also compare the
  popped-callee window above the frame, TR3), `TR_ORACLE_V=1` (per-job counts).
- **`tr_mutants`** (9.7): the every-mutant runner. `TABLE` maps each name of `tr::mutants::NAMES` to detecting tests; the
  runner builds the other `tr_*` binaries with a nested `cargo test --no-run`, runs every detecting test without a mutant (must
  pass) and with `RUSTYNAIL_JIT_MUT=<name>` (at least one must fail), and fails for an injected mutant without a test or not caught.
  Mutants whose injection site does not exist yet are listed as `not injected` (`TR_MUT_STRICT=1` fails them: the F9 gate).
  When a task injects a mutant it adds its row (replacing the whole-binary placeholder of the P7 rows).
  **Running the full runner (529 mutants) under the 8 GB cap.** The OOM of the old runner was a mutated trace allocating without
  bound in a child (the pool ran up to 6 test binaries, each with 3 worker children, plus a nested `cargo` with all CPUs). Now:
  `TR_MUT_JOBS` (default 2) test binaries at a time; `TR_MUT_BUILD_JOBS` (default 4) caps the nested cargo; every test binary gets
  `RLIMIT_AS` `TR_MUT_MEM_MB` (default 4096) and every `tr_core` job child `TR_CORE_MEM_MB` (default 2048; `tr_core` applies that
  default itself to any job of a mutated run, 0 disables); `TR_CORE_JOBS` (default 3) caps the workers inside `tr_core`.
  `TR_MUT_ONLY=a,b` runs the rows whose name contains one of the comma-separated substrings; `TR_MUT_SHARD=i/n` runs the rows whose TABLE index is `i` modulo `n`; `TR_MUT_RESULTS=file` appends one line per finished
  mutant (`name<TAB>caught|SURVIVED<TAB>tests`) and `TR_MUT_RESUME=1` leaves out those already `caught` in it (an interrupted run
  continues; survivors are retried). Each finished mutant prints `[tr_mutants] done <name> caught|SURVIVED ...` at once. The
  baselines of the shard's tests run first (a failing baseline is a reported problem, not a survivor). Measured: about 20 mutants per
  10 minutes per shard with 2 jobs; 3 shards of one scope (`TR_MUT_SHARD=0/3`, `1/3`, `2/3`, each `TR_MUT_JOBS=2 TR_CORE_JOBS=2`)
  took about 3 hours for the 528 mutants, peak well under 8 GB:

      cargo test -p rn-jit --features tr_mutants,t15_oracle --no-run     # build once (CARGO_TARGET_DIR as you like)
      # then, inside ONE scope (the cap covers all shards):
      systemd-run --user --scope -q -p MemoryMax=8G -p MemorySwapMax=0 bash -c '
        for i in 0 1 2; do
          TR_MUT_SHARD=$i/3 TR_MUT_RESULTS=$HOME/mut/shard$i.res TR_MUT_RESUME=1 TR_MUT_JOBS=2 TR_CORE_JOBS=2 \
            timeout 40000 cargo test -p rn-jit --features tr_mutants,t15_oracle --test tr_mutants -- --nocapture every_registered \
            </dev/null > $HOME/mut/shard$i.log 2>&1 &
        done; wait'

  **Result of the first full run (2026-11-09, main 584ed677 + this change, 3 shards, 23 minutes wall):** 511 caught, 5 survived,
  7 reported `not injected yet`. Survivors: `side_stitch_never` (no test compared `sidestitch=1` with `=0`: `m1_interp_shapes` now
  asserts the side `Recursion` aborts drop 3x), `rawget_str_absent_skip` and `rawset_result_nil` (nothing ran an absent-then-present
  short-string `rawget`, nor used the result of `rawset`: new `tr_core::rawfn_programs`), `wgate_full_rearm` (STALE detector: the
  `b1_nest` entry counts are now 381901 / 381344 / 378746 without the mutant and 381901 / 361538 / 378108 with it; the 15x effect the
  test was written for no longer exists since the rearm and work-gate policies changed; the mutant is behaviourally equivalent on that
  program; not fixed), `rec_push_top_off` (the callee `CallInfo.top` of the native self-call is one slot low; observable only when a
  metamethod call or other `L->top = ci->top` op runs while the frame's last register is live and the frame was pushed natively; no
  program of `pm3_programs_run_on_traces` has that shape; not fixed, suspected weak detector rather than equivalent). The runner also
  never ran the four `rearm_*` mutants (R1): `injected()` scanned only `src/tr` and they are injected in `src/rt.rs`; it scans `src`
  now (all four caught by `r1_rearm_programs`). `abc_with_store` (P7) and `vbytes_no_credit` / `vbytes_no_limit_guard` (F-memory) have
  no injection site in the tree (registry placeholders). The new mutant `rand_storm_guard` (C43) makes the registry 529.

  **Run after K3a (2026-11-09, main ec084529, 3 shards, about 26 minutes wall under `taskset -c 3-7,11-15`):** 498 of 506 mutants ran (3 registry placeholders have no injection site: `abc_with_store`, `vbytes_no_credit`, `vbytes_no_limit_guard`),
  8 survivors, all stale detectors: the shipped configuration (`TrLimits::default()` since K3a) records different shapes than the OFF-by-default struct of before. Fixed without weakening any assertion (rows or programs added, nothing relaxed):
  `hrefv_mask_off_by_one`, `hrefv_meta_skip` (the `HREFV` mask form lives behind `dynmask=0`; rows `dynmask=0` in `dynamic_key_loops_do_not_exit_per_key` / `dynamic_key_programs`), `tnext_key_site_type` (with `typesides=1` the
  misclassified traversal end gets a side trace; row `typesides=0` in `gfor_nest_stays_in_traces`), `sink_mem_error_dropped` (the shipped inline stub materialises through `jit_tr_mat`; the handler path of the mutant needs `sinkstub=0`: four rows in
  `sink_materialise_alloc_failure_raises_memory_error`), `strmt_flush_skip` (with `hdrlib=2` the stale constant is not observable on `pv_string_methods`; `hdrlib=0` rows), `mm_num_mt_skip` (a number metatable without `__add` that gains one mid-loop:
  new program `n3_mm_nummt_late`; the old programs reach `mm_provider` first), `setmt_gc_skip` (`setmetatable({}, m)` of a fresh table is sunk under `sinkmt=1`, so the `SETMT` helper did not run: new program `n3b_setmt_gc_exit2` stores the table),
  `wgate_full_rearm` (the stale `b1_nest` detector of the 2026-11-09 run: the old bound `entered <= 2*e0+100` is kept and `entered >= 98% of e0` added: 381344 vs 361538 entries with the mutant). Each pair verified: passes without the mutant, fails with it.

  Do not edit sources while the shards start (the nested cargo would rebuild the binaries they exec). `cat shard*.res | grep
  SURVIVED` lists the survivors. `tr_core::mutants_are_caught_by_live_programs` (24 mutants, sequential, the unmutated family of ~500 jobs per mutant) runs alone with
  `TR_CORE_MEM_MB=2048 TR_CORE_JOBS=2` under the same scope; `TR_CORE_MUTANT=<substr>` selects some (run several substrings as parallel processes: 23 of the 24 were caught in about 20-70 s each that way; `rolled_no_writeback` hangs most of its `-loop` jobs to the 20 s leash and took over 30 minutes without finishing, it is caught by `tr_asm` and `mutant_probe_live_programs` in the sharded runner).
  `tr_core::adhoc_file` (`TR_ADHOC_FILE=x.lua`, `TR_ADHOC_TRACE`, `TR_ADHOC_OPTS`, `TR_ADHOC_LIMITS`) runs any Lua file as a harness
  program against the interpreter: the tool for minimising a miscompile (C43 was found with it).
- The `mutant_probe*` tests of `tr_identity`, `tr_oracle` and `tr_core` are small families that pass without a mutant and fail
  under a family of mutants; the runner drives them with the environment variable set (their children inherit it).

## J5 trace modes, generator templates, fuzzing (P8b)

Needs `rnlua` built with the trace feature: `cargo build --release -p rn-cli --features trace` (the driver checks
that `RUSTYNAIL_JIT_TRACE=always` makes the binary print a `[trace]` summary and exits 2 otherwise).

**Modes** (all with T15 off; compared with `off` on stdout, stderr and the exit code; `JITDIFF_REF=<lua>` adds the reference
interpreter as before):

| mode | environment |
|---|---|
| `tr` | `JIT=always TRACE=always` |
| `tron` | `JIT=on THRESHOLD=2 TRACE=on TRACE_HOT=2,2` |
| `trstack` | `tr` + `RUSTYNAIL_JIT_STRESS=stackmove` |
| `trstorm`, `trstormN` | `tr` + `TRACE_OPT=+exitstorm=3` (`N`) |
| `tremu` | `tr` + `+emurun` (the trace runs in the emulator; equals the native trace run, both equal `off`) |
| `trrand` | `tr` + `+randra=<seed>` |
| `trverify`, `trnoopt`, `trnosink` | `tr` + `+verify` / `-fold,-cse,-loop,-fwd,-abc` / `-sink` |
| `tr:<opts>`, `tron:<opts>` | any `TRACE_OPT` string, e.g. `JITDIFF_MODES="off tr:+exitstorm=2,+randra=5 tron:-loop"` |

    JITDIFF_ENTERS=1 JITDIFF_MODES="off tr tron trstack trstorm tremu trrand" scripts/jit-diff.sh 1 2000 target/release/rnlua
    JITDIFF_HOOK=all JITDIFF_ENTERS=1 JITDIFF_MODES="off tr tron" scripts/jit-diff.sh 1 400 target/release/rnlua

- **Template set.** A trace mode in the mode list selects the generator template set `tr` (`gen.lua` arg 4; `JITDIFF_TSET=tr|gm|none`
  overrides): 75% of the snippets of a seed come from the prototype-shaped templates, 25% from the general ones. Without a
  trace mode (and without arg 4) the generator stream is exactly the pre-P8b one: old seeds, saved failures and the T15 gates
  reproduce (checked: seeds 1-300 x plain/count/both byte-identical to the pre-change generator). Do not mix an old mode run and a
  trace run in one invocation if the old numbers must stay comparable with history.
- **Prototype-shaped templates** (`TT` in `gen.lua`): numeric `for`/`while` loops (step variations, descending, float loops) over
  int/float/mixed/wrapping arrays in local functions; upvalue accumulators; an invariant `math.sqrt(upvalue)` and
  `floor/ceil/abs/min/max` of elements; loop-carried recurrences `t[i] = t[i-k]*c + x` on exact, short (ragged: out-of-bounds read
  raises) and holed arrays; may-alias array pairs (`a == b` and distinct); 1-4 level call chains of small local functions with
  fixed result counts, with the callee rebound to a Lua function / `math.abs` mid-loop; the intrinsics over a table of special
  values and a replaced `math.sqrt`; `while`/`repeat` with rare branches and `break`; type flips of accumulator and element after
  iteration K; nested loops with inner trip counts 0..12; bubble passes (`t[j], t[j+1] = ...`), appends with `#t+1`; rare
  branches in a hot loop with upvalue counters written by a called closure. Values are folded with `mix` after the loop, errors
  with `err` (pcall), so every template ends in a printed hash.
- **Counters** (`JITDIFF_ENTERS=1`): the verbose run of every trace mode contributes `installed`, `entered` and `exits` of the
  `[trace]` summaries. Reported per (hook variant, mode): `traces: ... with_trace=K (P%)` = seeds that installed a trace and
  entered one (`entered + exits > 0`: `+exitstorm` flushes traces and with them the entry counters, the exits survive).
  `NOTRACES` fails when a trace mode never entered a trace in the whole range. `FEWTRACES` fails when fewer than 25% of the seeds
  in `tr` or `tron` (hook variant `none`; hooks make traces refuse in the hooked snippets) did. `NOTRACES_LIST=n` prints the first
  n seeds without a trace. The existing `NOENTERS` check (compiled code entered at all) still applies.
- **Stack stress.** `stackmove` moves the stack at every helper call: the `deep` template (150000 frames) is quadratic (40+ s, the
  J4 `stack` mode alone is as slow). `stack`-like modes run with 3x `TMO`; a timeout in a stack mode when the baseline finished is
  recorded as `SLOWSKIP` (counted in the summary line, not a diff).
- **Hooks.** `JITDIFF_HOOK=all` works with the trace modes: with a hook installed no trace is entered or recorded, and the snippets
  in between (`run()` alternates) are traced; the hook counters are part of the compared output.
- **ASan** needs `JITDIFF_NOULIMIT=1` (shadow memory); `scripts/asan-jit.sh` sets it.

### Fuzz targets (P8b)

Build/run: `FUZZ_FEATURES=trace scripts/jit-fuzz.sh build`, `fuzz/make_trace_seeds.sh` (corpus in `fuzz/corpus-trace/`),
`CORPUS_DIR=fuzz/corpus-trace scripts/jit-fuzz.sh run <trace_ir|jit_diff|run_source> 300`. Always under the caps of CLAUDE.md
(`-rss_limit_mb=2500 -timeout=30`, the script adds `systemd-run`).

- **`trace_ir`**: input = 8-byte seed + 3 config bytes. The P2 generator (`tr::interp::gen::random_case`) builds a well-typed trace
  (optionally with a `LOOP`/PHIs) and models; stages: fold/CSE vs the interpreter on every model; `opt_loop` (peeling, forwarding)
  on the rolled trace vs the interpreter; register allocation (default or `+randra`), exit building, assembly and the micro-ops in the
  emulator (inline or generic stubs, with or without `+exitstorm`, budget in a register or in memory) or natively, vs the
  interpreter: exit snapshot, restored frame, budget word, arrays, upvalue cells. The differential harness is not copied: `fuzz/build.rs`
  slices it out of `crates/rn-jit/tests/tr_asm.rs` (everything before the first `#[test]`), `fuzz/src/trace_harness_ext.rs` is the entry
  point, so the target cannot drift from the test suite. Byte layout in the header of `trace_harness_ext.rs`.
- **`jit_diff`** trace bits: first byte `0xFF` is an escape, then a trace byte `t` (mode `always`/`on` hot 2,2 / `on` hot 4,3, 8 option
  variants, `-loop`, `-fwd,-abc`, `+tracemove`), then the usual config byte (the config `0xFF` itself is given up). The compiled run
  has T15 off. **`run_source`** trace bits: bits 2-3 of the first byte (mode) and 4-6 (option variant) of the compiled runs (bit 7).
  Details in the headers of the two targets; `RUSTYNAIL_JIT_VERBOSE=1` on a repro prints the `[trace]` summary.


## BUNDLE-J tests (2026-11-09)
- `tr_hashd` (rn-jit, feature `trace`): `hashd_pairs_order_matches_the_reference` (golden `tests/data/hashd_pairs_order.expected` = `ref/55/build-seed0/refrun` on `tests/vm_seed0/lazy_hash_pairs_order.lua`; the file is NOT in `tests/vm` because its output depends on the seed: check it by hand with
  `RUSTYNAIL_SEED=0 rnlua tests/vm_seed0/lazy_hash_pairs_order.lua` against `ref/55/build-seed0/refrun`), `hashd_promoted_hash_is_the_puc_hash` (nonzero seeds), `hashd_string_table_stays_consistent`, `hashd_recorder_mainpos_uses_the_puc_hash`.
- `tr_core::{centry_programs_run_on_traces, centry_small_comparator_is_a_root, centry_blacklisted_comparator_leaves_c_entry}`; `Out.stats` gained `c_registered`, `c_light`, `c_entries` (the `[trace] centry:` line). jit-diff `JITDIFF_TSET=bj` (templates `bja`..`bjd`): `JITDIFF_TSET=bj JITDIFF_ENTERS=1 JITDIFF_MODES="off tr trdef trstorm tremu trstack" scripts/jit-diff.sh 1 200 BIN`.
- Mutant runner for the new families: `TR_MUT_ONLY=promote_skip,strtab_idx_raw,chain_cmp_hash_only,record_mainpos_raw,promote_seed0,centry_` (11 mutants, 18 s after the build). Tests that need the old function-root behaviour of a `gsub` callback pin `centry=0`.

- Follow-up: `tr_core::centry_registration_waits_for_hot_calls` (mutant `centry_wait_ignored`); mutant runner selection now also `centry_wait_ignored` (12 mutants of the bundle).

## BUNDLE-K (lazy CallInfo, `lazyci`)
- Observer programs `tests/lazyci/bk_obs1.lua` (traceback, getinfo, getlocal, error + pcall / xpcall, count and line hooks installed from a leaf, collector at depth, coroutine yield from a leaf, pcall inside the recursion) and `bk_obs2.lua` (roots entered from C: pcall, sort comparator,
  gsub callback, coroutine body; stack overflow after warm-up in the main thread, a coroutine and nested pcall; errors with traceback from depth; coroutines created in the recursion; mutual / vararg / multi-result / tail recursion; ackermann; floats and wrong types at depth).
  `scripts/lazyci-diff.sh [rnlua] [ref lua]` runs every program of `tests/lazyci` against the reference interpreter (`ref/55/build/lua` of the main checkout) in 8 modes (interpreter, trace with `lazyci=0` / `1`, shipped defaults with `lazyci=1`, `+exitstorm=3`, `+exitstorm=1`, `+verify`,
  GC stress 2), all capped; a clean run prints 16 `ok` lines and no `DIFF`.
- `tr_core::{bk_lazy_programs, bk_lazy_r2_programs, bk_lazy_programs_enter_traces}` (the observers, `pm3_rec`, `r2_rec`, `px_rec_landing` in `always` / `on2` with `+verify`, exit storms, budgets 17 / 1, GC stress 1 / 2, `unroll=4,maxdepth=6`, `fnroot=1`, `lazyci=0`); programs `bk_obs1`, `bk_obs2` in `tests/tr_core/progs.rs`.
  `tr_exit::lazy_leave_completes_the_chain_like_the_interpreter`: an interpreter-built recursion, the lazy fields of 1..5 frames (and the savedpc of a new frame) poisoned from a native function at the leaf, `lazy_leave` must restore exactly what the interpreter had built (top, kcache, callstatus,
  savedpc), `thread.ci`, `thread.top` and the root word.
- jit-diff template set `bk` (`tests/jitdiff/gen.lua`, 9 templates: fib with tracebacks from leaves, collector at depth, errors through pcall / xpcall, ackermann / tribonacci with float arguments, coroutine yields from leaves, stack overflow, recursion from sort / gsub callbacks, getlocal, mutual / vararg recursion):
  `RUSTYNAIL_JIT_TRACE_LIMITS=lockstep=1,lazyci=1 JITDIFF_TSET=bk JITDIFF_ENTERS=1 JITDIFF_MODES="off tr trdef trstorm tremu trstack" scripts/jit-diff.sh 1 200 BIN`.
- Mutant runner: `TR_MUT_ONLY=lazy_` (16 mutants; `lazy_fill_*` and `lazy_leave_root_zero` are detected by the `tr_exit` unit test, the others by `bk_lazy_programs`; about 3 minutes after the build). Registry 525 -> 541; `table_covers_the_registry_exactly_once` lists them in registry order.

## BUNDLE-L (array loops, `aidx` / `bhoist`)
- Observer programs `tests/bl/bl_obs1.lua` (loops that run past the array and use `or` defaults, `debug.getlocal` over all slots from a function called in the loop, errors with an `xpcall` handler that reads the frame, the array growing / shrinking in the loop, `__index` / `__newindex` arrays, an upvalue alias that changes the array mid-loop, coroutine yields inside the loop, a count hook)
  and `tests/bl/bl_oob.lua` (loads and stores with `a[k + c]` ending exactly at / past the array part, float arrays with a wrong element, nil holes, a `while` loop whose counter wraps, integer-limit loops, table.insert / remove / replacement in the loop, matrices with a ragged row). `scripts/bl-diff.sh [rnlua] [ref lua]` runs them against the reference
  interpreter in 8 modes (interpreter, trace with the knobs off / on, shipped defaults with the knobs on, `+exitstorm=3`, `+exitstorm=1`, `+verify`, GC stress 2); a clean run prints 16 `ok` lines. The reference leaves the error text in one more `(temporary)` slot than our interpreter: `locals_of` in `bl_obs1.lua` stops at the first string temporary.
- `tr_core::{bl_programs, bl_programs_enter_traces}` (the two observers and `bl_mat` with `aidx=1`, `bhoist=1`, both, in `always` / `on2` with `+verify`, exit storms, `+randra`, GC stress 2, budget 17); programs in `tests/tr_core/progs.rs`.
- `tr_loop::{bhoist_replaces_the_body_bounds_checks_by_pre_roll_guards, bhoist_keeps_the_checks_it_cannot_cover, aidx_makes_the_complemented_index_an_ir_value}` (IR-level: three guards per bound, none in the body, the `+c` of the last guard, a bound that changes in the loop, step 2; the displacement and sharing of the complemented index).
- jit-diff template set `bl` (`tests/jitdiff/gen.lua`, 6 templates: `a[k]`/`b[k + c]` loops that grow, matrix products, errors from inside loops with pcall, `__index` tables with insert / remove, coroutine yields, loops from/to the integer limits and with steps): `RUSTYNAIL_JIT_TRACE_LIMITS=lockstep=1,aidx=1,bhoist=1 JITDIFF_TSET=bl JITDIFF_ENTERS=1 JITDIFF_MODES="off tr trdef trstorm tremu trstack" scripts/jit-diff.sh 1 N BIN`.
- Mutants (9, registry 541 -> 550 in `src/tr/mutants.rs`; the `registry_is_the_frozen_list` assertion said 525 at the base and failed there, it now says 550): `aidx_disp_ignored`, `aidx_off8`, `aidx_body_reuse` (asm / optimiser; caught by `tr_core::bl_programs`), `bhoist_no_count_guard`, `bhoist_no_first_guard`, `bhoist_no_last_guard`, `bhoist_last_c_off`, `bhoist_bound_variant`, `bhoist_step_any`
  (caught by the `tr_loop` IR tests, the last two also named with `bl_programs`); `TR_MUT_ONLY=aidx_` and `TR_MUT_ONLY=bhoist_` (about 2 and 4 minutes after the build): 3 of 3 and 6 of 6 caught. A first `aidx_sub_sign` (wrong displacement of `k - c`) was dropped: the folder canonicalises `SUB k, #c` to `ADD k, #-c`, the branch is unreachable from the recorder.
  Base failures met on the way and fixed here: `tests/tr_asm.rs` did not compile (`AsmEnv::lazy_cs`, `lazy_nparams` from BUNDLE-K), the registry count assertion. Still failing at the base: `tr::tests::exit_area_layout_is_frozen` (2312 against 2304, BUNDLE-K grew the exit area).
- Runs of this bundle (binary with `aidx` and `bhoist` forced on through `lockstep=1,aidx=1,bhoist=1`): jit-diff seeds **1..100** (not 1..200: time) modes `off tr trdef trstorm tremu trstack`: set `bl` 0 diffs (74 of 100 seeds ran traces in `tr`), default `tr` set 0 diffs (15 slow skips); `scripts/bl-diff.sh` 16/16 ok; `tr_core` (170), `tr_policy`, `tr_asm` (41), `tr_loop` (27), `tr_record`, `tr_exit`, `tr_ra`, `tr_determinism`,
  `tr_runtime`, `tr_record_calls`, `tr_hook`, `tr_poll`, `tr_passes`, `tr_ir`, `tr_class_a_pb`, `tr_hashd`, `rn-core` + `rn-std --features jit` all pass; rn-jit `--lib` fails only `exit_area_layout_is_frozen` (base failure). The 5.4 build, the ASan scripts and the fuzz targets were not run.
- **A bug the first jit-diff run found** (bl seed 4, 16 of 100 seeds with `aidx=1`): the body copy split an index that was a pre-roll `ADD k, #1` carried into the body as a PHI (`NEG` of the pre-roll operand = the pre-roll iteration's `k`, not the previous iteration's). Fixed (a body copy splits only indices made in the body) and pinned by `sd4` in `tests/bl/bl_oob.lua`; the hand-written
  observers had not caught it, the generated programs did.

### BUNDLE-M M1: exit area layout test

`tr::tests::exit_area_layout_is_frozen` now asserts `LAZY_ROOT == ARGS + 32` and `SIZE == LAZY_ROOT + 8` (2312 bytes; the extra word is the intentional BUNDLE-K `lazy_root`). The only offset consumer of the new field is `asm.rs` (`exit_area::FROM_R15 + exit_area::LAZY_ROOT`); `exit.rs` and `tr_exit.rs` use the struct field; no hard-coded offsets exist elsewhere.

## BUNDLE-M (hygiene, 2026-11-09)

- **M1** `tr::tests::exit_area_layout_is_frozen` now asserts `LAZY_ROOT == ARGS + 32` and `SIZE == LAZY_ROOT + 8` (2312 bytes; the extra word is the intentional BUNDLE-K `lazy_root`). The only offset consumer of the new field is `asm.rs` (`exit_area::FROM_R15 + exit_area::LAZY_ROOT`); `exit.rs` and `tr_exit.rs` use the struct field; no hard-coded offsets elsewhere.
- **M2** `bhoist` ON: see docs/PERF.md "BUNDLE-M". `tr_core::type_stability` pins `bhoist=0`; new `tr_core::type_stability_hoisted`.
- **M3** strtab tag: `tr_hashd::hashd_string_table_beyond_the_tag` (530 K strings, past the 2^19-bucket tag width, shrink back), mutants `strtag_zero`, `strtag_shift`, `strtag_wide`; `Heap::verify` checks the tag (GC_VERIFY=1 / stress 1 and 2 run on a string churn program: clean). `rn-core` + `rn-std --features jit` (all pass), `rn-capi` differential against `ref/55` (4 pass), `tr_hashd` 5, `tr_runtime`, `tr_policy`, `tr_passes` pass.
- **M4 mutants.** Registry 550 -> 552: `abc_with_store` deleted (there is no ABC pass: `TraceOpts::abc` is parsed and unused, de-duplication of bounds checks is the generic CSE guarded by `header_safe`, covered by `newref_header_safe` / `hdrlib_*`); `vbytes_no_credit` and `vbytes_no_limit_guard` injected in `rt::recompile` (the refunded version bytes of a failed recompile; the ignored refused charge) with detectors `t15_runtime::a_failed_recompile_refunds_its_version_bytes` (a loop body of 3000 statements fills the first chunks, the code cap refuses the second version after the charge; the extension memory must grow by the same amount as for an unpromoted run of the identical chunk text) and `memory_limit_refuses_a_promotion_and_the_nest_retries_later` (now compares the charged extension memory of a refused promotion with that of a control run); `strtag_zero/shift/wide` added (M3). The 5 `V54_ONLY` rows have their sites only under `--cfg rustynail_lua54` (not run).
  `rec_push_top_off` (survived since the first full run): its effect is the `CallInfo.top` of a natively pushed frame, which nothing in a program observes except a metamethod pushed at `ci->top` by the interpreter, and no program had that shape. The tr_core harness now registers a native `__citop()` (the caller's `top - (func + 1)`, i.e. the prototype's `maxstacksize` for an interpreter-pushed frame); `tr_core::progs::bm_rec_top` calls it after a native self-call in every frame and sums the result; in `pm3_programs_run_on_traces` (always / on2, `+verify`, `+exitstorm=3`). Without the mutant 7 everywhere, with it 6 in the natively pushed frames.
- **Full sharded run** (3 shards of `every_registered_mutant_is_caught`, `TR_MUT_JOBS=2 TR_CORE_JOBS=2`, one scope with `MemoryMax=8G`, defaults `TR_MUT_MEM_MB`, 28 minutes wall, on the tree after M1-M4 code changes `dbab7a9c`): **547 ran, 543 caught, 4 survived** (552 registered, 5 skipped as 5.4-only). Survivors, all analysed:
  `aidx_disp_ignored` (stale: `bhoist` ON by default changes the shape of `aidx=1` alone; row `aidx=1,bhoist=0` and the program `bl_off` with `x[k + 1]`, `x[k - 2]` indices, whose displacement is non-zero, added; caught), `strself_tag_skip` (the string-receiver site never saw a non-string receiver: program `k_strself2` with numbers, booleans, a userdata and a table at one `s:sub()` site; caught), `t1i_floor_inexact` (no program floored a float that is out of the integer range: `t1i_floor2`, caught), `fside_room_ignored` (**still survives**, a real gap in the detector: the mutant skips the room check of a side trace entered with virtual frames (`siderooms=1` re-checks at the start, `siderooms=0` refuses); the S1 program has no side trace that needs more stack than its parent checked. A program with a 180-register callee behind a rare branch of an inlined helper, run at 200 recursion depths, was tried; its side traces were not "frameful" in the harness (the exit inside the inlined `f` produced an ordinary side trace), so the check never fires. Added rows `siderooms=0` / `siderooms=0,maxdepth=2` in `s1_frame_side_programs` as controls (pass). Left as a documented survivor).
  Caught/survived per family (owner column of the table): A1 6, A2 6, B1 8, B2 5, B4 4, B5 5, BB 7, BUNDLE-A 8, BUNDLE-B 3, BUNDLE-C 7, BUNDLE-D 8, BUNDLE-E 1, BUNDLE-F 2, BUNDLE-G 2, BUNDLE-G2 1, BUNDLE-GM 4, BUNDLE-H 14, BUNDLE-H1 10, BUNDLE-J 12, BUNDLE-K 16, BUNDLE-L 8 (+1 survived), BUNDLE-M 3, C2 3, C36 8, C38 1, C39 2, C43 1, DYNMASK 4, F-memory 2, F1 14, F2 12, F2a 15, F2b 7, F3 9, F3b 14, F4 8, F5 7, F8 2, G1 7, H2 7, H3 2, HDRSUB 3, J1 10, J2 6, K 2 (+1 survived), L 1, M 3, M1 4, N 1, N1 4, N2 1, N3 19, N3b 6, N5 1, N6 5, N7 9, O 3, P 3, P-A 8, P-B 14, P-E 10, P-E2 8, P-F 6, P-L 4, P-M3 7, P-M4 2, P-N 2, P-O 5, P-V 1, P-X 1, P1 1, P2 19, P3 9, P3a 7, P3b 6, P4 2, P4c 4, P5a 5, P5b 9, P6 2, P7 7, P7b 4, Q 1, Q1 1, Q2 1, Q3 1, R1 8, R2 9, S 2, S0 7, S1 9 (+1 survived), SINK-F 1, SINK-R 2, T1I 9 (+1 survived), T3 3.
  Re-run of the changed rows after the fixes (`TR_MUT_ONLY=aidx_disp_ignored,fside_room_ignored,strself_tag_skip,t1i_floor_inexact,vbytes_,strtag_,rec_push_top_off`): 10 caught, `fside_room_ignored` survived.
- **M5 (K4)**: not done, see docs/JIT_PARKED.md C57. **M6**: scratch and merged worktrees cleaned.


### BUNDLE-P (2026-11-14): `f2ifl`, `fast_hash_fresh`, `rearmside`

- jit-diff template set `bp` (`tests/jitdiff/gen.lua`, `JITDIFF_TSET=bp`; not part of TT / T, the other sets keep their seeds): floor / ceil of floats at the edges of the integer range (NaN, +-inf, -0.0, 2^53, 2^62, 2^63, -2^63, -2^63 - 3072, 1e300, ...) in a hot loop and repeated per value, a two-to-six-class dispatch loop (one side trace per class: the shape `rearmside` is about) and a vector class with a vararg `__call` constructor, `__add` / `__sub` / `__mul` / `__div` / `__unm` metamethods that end in a tail call or a plain return, `__eq` / `__lt` / `__le` / `__len` / `__concat`, metamethods replaced in the middle of the run. `JITDIFF_ENTERS=1 JITDIFF_TSET=bp JITDIFF_MODES="off tr trdef trstorm tremu trstack" scripts/jit-diff.sh 1 100 BIN`; a second pass with `RUSTYNAIL_JIT_TRACE_LIMITS=lockstep=1,rearmside=3`, and the default set with the same limits.
- `tr_asm::hand_f2ifl_floor_guard` (floor / non-floor operands, 15 edge values, emulator and native, `f2ifl` on and off), `tr_core::bp_f2ifl_programs` (programs `bp_floor`, `bp_floor_minint`: the output equals the interpreter's in every trace mode and, for the two programs checked in `ref`, the reference Lua's; the floor of exactly -2^63 exits with the knob and only with it), `tr_core::bp_rearmside_programs` (`bp_rearm_young_{clean,dirty,la}` since BUNDLE-C45), `tr_policy::{bp_process_defaults, bp_rearmside_rule}`, `tr_hashd::bp_fast_hash_equals_reference`, rn-core `string::tests::fast_hash_equals_reference_all_lengths`.
- Mutants (registry 552 to 557): `f2ifl_never`, `f2ifl_all`, `bytetail_hi_byte_skip`, `rearmside_never`, `rearmside_any` (all five caught; the 34 mutants of the string-allocation, hash, floor, `tnext` and rearm families re-run with `TR_MUT_ONLY=snew_,concat_,lib_bytes,lib_no_gc,promote_,strtab_,chain_cmp,record_mainpos,strtag_,floor,f2i,tostr,rearm_,tnext,m_floor`: 34 caught).
- A jit-diff `ref` column shows one pre-existing difference that has nothing to do with the JIT (seed 27: `collectgarbage("count")` deltas differ between rnlua and the reference build); every JIT mode equals `off`.

### BUNDLE-Q (2026-11-10): `t1q`

- Tier-1 tests need `--features trace` for rn-jit (without it 0 tests run and "pass"). `tr_core` jobs of the `t1` kind (JIT always, traces off) now apply the limits they name (before, they were ignored), which is how `t1q` is tested without traces.
- `tr_core::bq_t1q_programs` (programs `bq_eq`, `bq_mod`, in `tests/tr_core/progs.rs`): modes `t1`, `always`, `on2`, `on`; `t1q` 0 / 1 / 2 / 3; plain and `+verify`; exit storms, emulator, budget 17, GC stress 1 and 2, stack moves. It also asserts that compiled code executes more bytecodes (`Out::insns`) with the bit than without in `t1` mode. `bq_eq`: the same table, distinct tables with / without a metatable, `__eq` returning true / false / 1 / nil / "x", `__eq` on one operand only (both orders), added / removed / replaced after the absence flag was cached, `setmetatable` changing the answer, erroring and yielding `__eq`, functions, coroutines, io files, long strings, NaN, numbers. `bq_mod`: all sign combinations, zeros, `-0.0`, inf, NaN, `1e308`, `5e-324`, `2^53`, `2^63`, integer / float mixes, `% 0` of both kinds, `minint % -1`. Both programs also compared by hand against `ref/55/build/lua` in 16 modes (output files under `~/.cache/bunQ/t`, scripts `modes.sh`).
- `tr_policy::bq_process_default`. jit-diff template set `bq` (`JITDIFF_TSET=bq`; three templates: object equality matrix with `__eq` changes in the loop, float modulo matrix, a loop mixing `==` of fresh tables with float `%`): seeds 1..100, modes `off tr trdef trstorm tremu trstack`, default knob and `RUSTYNAIL_JIT_TRACE_LIMITS=t1q=3`: 0 diffs (85 % of the seeds ran traces); `off tr ref` with `t1q=3`: 0 diffs; the default set `tr` with `t1q=3`: 0 diffs (86 % ran traces, 15 slow skips).
- Mutants (registry 557 to 563): `t1q_eq_same_false`, `t1q_eq_nomm_true`, `t1q_eq_second_mm_skip`, `t1q_mod_sign_skip`, `t1q_mod_negzero`, `t1q_mod_inf_fix` (6 of 6 caught with `TR_MUT_ONLY=t1q_` on `cargo test -p rn-jit --features "trace tr_mutants" --test tr_mutants every_registered_mutant_is_caught`).
- Suites run on the final tree (all pass): `cargo test -p rn-core -p rn-std --features jit`, `tr_core` (174, `--skip mutants_are_caught_by_live_programs --test-threads=3`), `tr_policy` 46, `tr_asm`, `tr_loop`, `tr_record`, `tr_exit`, `tr_ra`, `tr_determinism`, `tr_runtime`, `tr_record_calls`, `tr_hook`, `tr_poll`, `tr_passes`, `tr_ir`, `tr_class_a_pb`, `tr_hashd`, the mutant registry and table tests, tier 1: `jit_arith`, `jit_table`, `jit_calls`, `jit_loops`, `jit_lifetime`, `jit_alloc`, `jit_stress`, `arith_diff`, `table_diff`, `emu_diff`, `emu_ops_diff`, `tr_bd`, `jit_review_{hooks,count}`, `jit_cstack`, `t15_analysis`, `t15_core_emu`, `t15_runtime`. Failing on the base commit too (same `debug_assert` in `rt.rs`, line 1618 there, 1637 here): `t15_stress::random_float_programs`, `t15_core::table_regions_are_generated` (and the known `mid_run_mutation_programs`, `t15_policy`). Not run: `t15_oracle`, `tr_oracle`, `tr_identity_pre` (known failures), the 5.4 build.


### BUNDLE-S (2026-11-10): `amt`, `t1q` bits 2 / 3

- Tier-1 and trace tests need `--features trace` for rn-jit (without it 0 tests run and "pass"); the instruction-count identity jobs need `trace,t15_oracle`.
- `tr_core::bs_amt_programs` (program `bs_amt`, in `tests/tr_core/progs.rs`): modes `always`, `on2`, `on`, `t1`; `amt` 0 / 1; plain, `+verify`, `+exitstorm=3 +verify`, `-loop +verify`, budget 17, GC stress 1 and 2, stack moves, emulator. Array reads / stores of tables with no `__index`, a Lua `__index`, a table `__index`; holes punched / refilled mid-loop; `__index` / `__newindex` added, removed, replaced after the absence flag was cached; `setmetatable(t, nil)`; one loop over tables with and without metatables; zero / negative / `2^53` / float / string / `maxinteger` / `mininteger` keys; stores over existing and nil slots with a `__newindex` that is added / removed / replaced, errors, `rawset`s, redirects; stores of changing types over existing elements; a yielding `__index` in a coroutine. It asserts that the `MetaTable` aborts of `amt=0` (43 in `always +verify`) are gone with `amt=1` (0). The first run found two bugs: the old-value guard of a store was evaluated against a lazily copied (already overwritten) model element (lockstep failure in `b2_programs`, `bb_tset_programs`, `twins_options`; fixed with `fill_elem` at record time; `bs_amt` gained the "stores of changing types" block that catches it), and the pre-existing `mm_index_int` float-key bug (the handler got the converted integer).
- `tr_core::bs_ixmm_programs` (program `bs_ixmm`): modes `t1`, `always`, `on2`, `on`; `t1q` 1 / 5 / 9 / 13 / 15; plain and `+verify`; exit storms, emulator, budgets 17 and 1000, GC stress 1 and 2, stack moves; with `t15_oracle` the instruction-count identity (`ident`) with budgets none / 3 / 17 / 200. Contents: every read form (`GETFIELD`, `GETTABLE` with string / integer keys, `GETI`, `SELF`, `GETTABUP` through a `load`ed environment) through a function `__index`; chains table -> table -> function (the handler must receive the level that holds it); handlers returning none / many values / calling a function that returns many; `pcall`-caught errors (string and table), a traceback from inside the handler (`metamethod 'index'`), `debug.getinfo` / `getlocal` / `setlocal` / `sethook` from the handler (a hook installed in the middle of a loop), the metamethod / the metatable / the table mutated mid-loop or by the handler itself (`rawset`, `setmetatable(t, nil)`), yields (resumed by the interpreter's `mm_return`; a coroutine that finishes inside the handler), nested frames and a runaway `__index` recursion (`C stack overflow` as in the reference), strings / numbers / nil / io files not affected, odd keys (`0`, `-1`, `2^53`, `1.5`, `2.0`, `maxinteger`, `true`), the four store forms through `__newindex` (shadow table, `rawset`, becoming `rawset`, removed and restored, erroring, yielding, table-valued chain, `SETTABUP` through an environment, float / negative keys), handlers whose general `RETURN` closes upvalues (closures per call, a to-be-closed variable, vararg handlers), and handlers that `debug.setlocal` the register of the caller that tier 1 holds typed (`s1_land_skip`). It asserts that compiled code executes more bytecodes (`Out::insns`) in `t1` mode with bits 4 / 8 / 12 than with neither.
- `r1_rearm_programs` pins `t1q=1`: its premise (tier 1 leaves at a Lua `__index` function and the interpreter finishes the call until the next tick) no longer exists with the process default.
- `tr_policy::{bq_process_default, bs_process_defaults}`. Output equal to `ref/55/build/lua` byte for byte, by hand, for `bs_amt` and `bs_ixmm` in 7 modes x `t1q` 0 / 4 / 8 / 12 / 15 (interpreter, tier 1 alone, default, traces always, GC stress 1 / 2, stack moves) with `~/.cache/bunS/cmp.sh`: all identical.
- jit-diff template set `bs` (`JITDIFF_TSET=bs`; four templates: array loops over a metatabled table with holes / `__index` / `__newindex` added, removed and replaced, `setmetatable(t, nil)`; reads through a function `__index` in a chain, a loaded environment, `SELF` and `pcall`, the handler replaced mid-run; the four store forms through `__newindex` with body variants; a yielding `__index` / `__newindex` in a coroutine). `JITDIFF_ENTERS=1 JITDIFF_TSET=bs JITDIFF_MODES="off tr trdef trstorm tremu trstack" scripts/jit-diff.sh 1 100 BIN`.
- Mutants (registry 563 to 574): `amt_get_nil_skip`, `amt_set_nil_skip`, `amt_set_old_skip` (`bs_amt_programs`), `s1_self_skip`, `s1_get_dst_off`, `s1_savedpc_skip`, `s1_count_skip`, `s1_pay_skip` (`t15_oracle`: the identity jobs), `s1_land_skip`, `s1_obj_orig`, `s1_close_skip` (`bs_ixmm_programs`): `TR_MUT_ONLY=s1_,amt_` 11 of 11 caught (`s1_land_skip` survived the first `bs_ixmm` and is caught since the straight-line `debug.setlocal` case `setlocal3` was added).
- Suites run on the final tree (`--features trace`, `--test-threads=3`; all pass): `cargo test -p rn-core -p rn-std --features jit` (106 + the doc and integration binaries), `tr_core` (176; `--skip mutants_are_caught_by_live_programs`), `tr_policy` 47, `tr_asm` 42, `tr_loop` 27, `tr_record` 34, `tr_exit` 34, `tr_ra` 19, `tr_determinism` 2, `tr_runtime` 43, `tr_record_calls` 37, `tr_hook` 13, `tr_poll` 3, `tr_passes` 14, `tr_ir` 16, `tr_class_a_pb` 19, `tr_hashd` 6, `tr_bd` 2, the mutant registry and table tests (`registry_is_the_frozen_list` 574, `table_covers_the_registry_exactly_once`), tier 1: `jit_arith`, `jit_table`, `jit_calls`, `jit_loops`, `jit_lifetime`, `jit_alloc`, `jit_stress`, `arith_diff`, `table_diff`, `emu_diff`, `emu_ops_diff`, `jit_review_{hooks,count}`, `jit_cstack`, `t15_analysis`, `t15_core_emu` (9), `t15_runtime`; with `trace,t15_oracle`: `tr_core` `bs_*` / `r1_rearm` / `bd_armm` / `bq_` / `bb_cmpmm` (7), `instruction_count_identity` (the four identity tests), `tr_identity` (5). Failing on the base commit too (not from this bundle): `t15_oracle::oracle_corpus`, `t15_core` (`mid_run_mutation_programs` aborts the binary at the `rt.rs` `debug_assert`, line 1642 here), `tr_oracle` (killed after the three known long-running failures `oracle_exits`, `oracle_options`, `oracle_trace_modes` had run for minutes). Not run: the 5.4 build, ASan, fuzz, the full 574-mutant runner (`TR_MUT_ONLY=s1_,amt_` only).

### BUNDLE-T (2026-11-10): fused coroutine resume

- Programs `tests/vm/co_11_switch_shapes.lua` (every wanted-result count at the `yield` call site, `return yield(...)`, argument / result counts 0..40 and 2500..5000 in both directions, a small-stack coroutine resumed with 3000 values and a small-stack resumer receiving 5000, `coroutine.yield` used directly as `__index` / `__add` / `__concat` / `__len` / `__unm` / `__lt` / `__le` / `__eq` / `__call` / `__newindex` and as a generic-`for` iterator, `local x = yield()` resumed with nothing), `co_12_errors_status.lua` (errors and error objects, dead / non-suspended / self resumes, `wrap` rethrow with position, `running` / `isyieldable` / `status` incl. `normal`, yield outside a coroutine and across `table.sort` / `gsub` / `__tostring`, library type errors), `co_13_close_tbc_hooks.lua` (`close` with pending to-be-closed variables, errors and yields in `__close`, `close` of the running coroutine, hooks `c` / `r` / `l` / count on a thread, a hook set from inside, a hook that yields, `debug.traceback` / `getinfo` / `getlocal` of suspended and dead coroutines), `co_14_cstack_overflow.lua` (chains of nested resumes against `LUAI_MAXCCALLS`, a coroutine suspended shallow and resumed deep, stack overflow inside a coroutine, GC of suspended coroutines, finalizers of objects held only by one, weak references to threads), `co_15_patterns.lua` (tree walk, permutations, producer / filter / consumer, scheduler, yields across `pcall` / `xpcall` / `pcall(yield)`, trampolines, many short-lived coroutines). Each (with `co_01` .. `co_10`) is byte-identical to `ref/55/build/lua` (stdout, exit status, first stderr line) under `RUSTYNAIL_JIT=0`, the default, `RUSTYNAIL_JIT=always`, `RUSTYNAIL_JIT_TRACE=always`, `RUSTYNAIL_GC_STRESS=1` and `=2` (and `=2 RUSTYNAIL_GC_VERIFY=1` on `co_02` .. `co_05` / `co_09`), and `RUSTYNAIL_JIT_STRESS=stackmove` (except `co_09_stack_and_args`, which times out at 300 s with the base binary too: its 5000-deep recursion is quadratic under stack moves), with `cmp_ref.py` (a scratch script comparing a `rnlua` with the reference by path); they are in `tests/vm`, so `vm_corpus` runs them against `refrun` as well (with the `ref` directory present: `vm_corpus` compares nothing in a worktree without it).
- `crates/rn-jit/tests/tr_coro.rs` (feature `trace`; the compiler is registered, `print` / `io.write` captured, chunk names as when run by path, 256 MB thread stack for the nested resumes): the five programs against `tests/data/co_NN_*.expected` (produced by `ref/55/build/lua` in `tests/vm`).
- Mutants (registry 574 to 581; the flag mask lives in rn-core `coro::set_mut_cofuse`, forwarded by `apply_vm_mutants`): `cofuse_room_skip` / `cofuse_results_room_skip` (the missing stack-room checks: the values that do not fit are dropped; `co_11`), `cofuse_hook_ignored` (`co_13`), `cofuse_ncalls_skip` (the resumer's C level is not propagated: the "suspended shallow, resumed deep" probe of `co_14`), `cofuse_finish_op_skip` (`co_11`: `coroutine.yield` as a metamethod), `cofuse_wanted_nil_skip` (`co_11`), `cofuse_prev_c_ignored` (`co_15`: `pcall(yield)`). `TR_MUT_ONLY=cofuse_ cargo test -p rn-jit --features tr_mutants,t15_oracle --test tr_mutants -- --nocapture every_registered`: 7 of 7 caught (the first two versions of the room mutants wrote past the stack and survived because the stack had spare capacity; they now model the bug as silent truncation).
- jit-diff template set `bt` (`JITDIFF_TSET=bt`; six templates: generators with changing argument / result counts and wanted counts, an explicit `resume` loop with `table.pack` / `unpack` of 0..40 values and `status`, `pcall` / `xpcall` / error / `error`-object between switches, `yield` as a C metamethod, nested resumes up to the C-stack limit, 50..2500 values, garbage coroutines with `<close>` variables and `coroutine.close`). Seeds 1..100, modes `off tr trdef trstorm tremu trstack` plus `ref` (`ref/55/build/lua`): 0 diffs.
- Suites run on the final tree (all pass): `cargo test -p rn-core -p rn-std` 184 and `--features jit` 186; with `--features trace`: `tr_core` 176 (`--skip mutants_are_caught_by_live_programs --test-threads=3`), `tr_policy` 47, `tr_asm` 42, `tr_loop` 27, `tr_record` 34, `tr_exit` 34, `tr_ra` 19, `tr_determinism` 2, `tr_runtime` 43, `tr_record_calls` 37, `tr_hook` 13, `tr_poll` 3, `tr_passes` 14, `tr_ir` 16, `tr_class_a_pb` 19, `tr_hashd` 6, `tr_coro` 5, `tr_tbl_move` 1, the registry test (581), `jit_arith` 11, `jit_table` 12, `jit_calls` 9, `jit_loops` 14, `jit_lifetime` 4, `jit_alloc` 2, `jit_stress` 2, `arith_diff` 7, `table_diff` 6, `emu_diff` 6, `emu_ops_diff` 45, `tr_bd` 2, `jit_review_hooks` 4, `jit_review_count` 3, `jit_cstack` 3, `t15_analysis` 46, `t15_core_emu` 9, `t15_runtime` 23; `cargo test -p rn-capi` 4; `cargo test --release -p rustynail -p rn-cli` (`vm_corpus` against `refrun`, with a temporary `ref` symlink in the worktree, removed afterwards) 56 pass.
- Build matrix of T0 (`cargo check -p rn-jit --all-targets --features X`): `jit`, `jit,trace`, `t15_oracle`, `t15_mutants`, `tr_mutants` all compile (the `vbytes_*` mutant calls of `rt::recompile` go through `vbytes_mutant`, which is `tr::mutants::active` with `trace` and `false` without); `RUSTFLAGS="--cfg rustynail_lua54" cargo check -p rn-core -p rn-std` compiles.


### BUNDLE-R (2026-11-10): more library calls on trace (`libcall` bits 3..9)

- Programs (`crates/rn-jit/tests/tr_core/progs.rs`): `br_lib` (`table.unpack` of fixed and changing sizes, holes, size growth in the middle of a loop, a metatable with `__index` / `__len` set in the middle of a loop, wanted counts fewer / more than the size, multiple results into calls, tail position, errors and `pcall`; `table.move` to another table, overlapping in both directions, nil destination, element kinds, destinations that grow, counts above the unrolled limit, arguments changing in the middle of a loop, a destination that becomes the source (`m12b` / `m12c` are the detectors of the identity guard: their results depend on the copy direction), metatables with `__index` / `__newindex` set at run time, errors including `table.move({}, 1, 2, math.maxinteger)` messages; `tonumber` of 25 non-numeral and 15 numeral forms, the success / failure flip without a branch (`tn11`: the detector of the missing failure guard), bases 2..36, `tonumber()` / `tonumber(nil)`; concatenated keys present / absent / stored / resized / long / numeric / with `__index` and `__newindex`), `bh_lib` (the `rep` part grew: counts across the 40-byte class limit with and without a separator, empty strings, single-byte units, counts of every sign, a count above the helper cap, `1 << 62` errors, float counts), `bh_rep_mixed_hot` (alternating result classes). The reference does not finish `table.move({}, 1, math.maxinteger, 1)`: do not add it.
- `tr_core::br_lib_matrix` (limits 39 / 71 / 135 / 263 / 511 / 7 over the whole `ba_jobs` matrix), `bh_lib_matrix` (now also `libcall=31`), `ba_find_matrix` (now also `libcall=519`), `br_rep_class_guard_gets_a_side_trace` (instruction count with bit 4 below the count without it), `tr_class_a_pb::lib_rep_follows_the_library` (single-byte units, every count 1..60 of a three-byte unit with and without a separator across the class limit).
- Comparison with the reference by path: `~/.cache/bunR/refcmp.py NAME LIMITS` (extracts a `tr_core` program and runs `ref/55/build/lua` and rnlua in off / tier-1 / on / always / `+verify` / `+exitstorm=3 +verify` / `+emurun +verify` / GC stress 1 and 2 / stack-move modes with `RUSTYNAIL_JIT=always`; note that `RUSTYNAIL_JIT_TRACE=always` alone records nothing in a small program: tier 1 must be on too). `br_lib` equals the reference in all of them with every bit on.
- **Three traps found on the way**: (1) a library function registered for the recorder but disabled by its bit must behave as an unknown C function (stitch), not as `CFunc`: `programs/varargs` (`select(2, table.unpack(t, 1, 5))`) became 3x slower when the unsupported shape returned `Nyi` instead of stitching (fixed by `libfn_enabled` / `lib_unrecorded`). (2) `tr_core` tests that use `string.rep` as "a C call that cannot be recorded" (`nest_stitch`, `side_untraced_head`) now pin `libcall=7`. (3) the TLEN ops emitted by a recorded library call need `Rec::pend_lens` or the lockstep reads the result of the VM instruction for them.
- Mutants (registry 581 to 590; `TR_MUT_ONLY=tunpack,tonum_fail,tmove,repcls,libk_lean_find` runs them): `repcls_guard_skip` (`br_rep_class_guard_gets_a_side_trace`), `tunpack_meta_skip` / `tunpack_len_guard_skip` / `tunpack_off_by_one`, `tonum_fail_guard_skip`, `tmove_args_guard_skip` / `tmove_same_guard_skip` / `tmove_order_flip` (`br_lib_matrix`), `libk_lean_find_swapped` (`ba_find_matrix`). Concatenated keys (bit 8) have no mutant of their own: the path is the existing `HREFV` machinery (its mutants `hrefv_*`, `dynmask_*` are live in `br_lib`'s `ck*` functions).
- jit-diff template set `br` (`JITDIFF_TSET=br`; five templates: `table.unpack` of small tables with `#` changes and `__len`, `table.move` ranges with changing arguments and a destination that sometimes is the source, `tonumber` of mixed forms, `string.rep` with changing classes and separators, concatenated keys with stores and deletions): seeds 1..100 in `off tr trdef trstorm tremu trstack` with `libcall=1023`: 0 diffs.

## BUNDLE-N (RETF infrastructure N2a-N2c)

- **Hand-built IR** (`tr_asm::hand_retf_*`, emulator): the pop and an exit in the caller (`r13`, `thread.ci`, `rbx`, `r14`, the exit store relative to the new base), every guard failing before the pop (`CIST_FRESH`, `CIST_MMRET`, `CIST_HOOKED`, wrong `nresults`, C caller, wrong site) and the bits that must NOT matter (`CIST_TAIL`, `CIST_JIT`), the `k` form (open upvalue below / at / above the base, a registered tbc), 200 random register allocations. `tr_ir::retf_cuts_the_uvrel_cse_chain`, `retf_in_the_model_rebases_slots_and_uvrel`; `tr_passes::optimize_keeps_retf_and_segments`; `tr_exit::n2a_segment_snapshots_use_the_segments_prototype_and_code`; `tr_hook::n2a_reroot_continues_the_recording_in_the_caller` (the VM-side protocol); `tr_policy::bn_retf_process_default`.
- **Programs** (`tests/tr_core/nr_progs.rs`, `n_retf_*`: basic, tail, multret, ccaller, two_sites, mm, getlocal, hook, vararg_caller, closeret, coro, deep, gc, results, upvalue_alias, hot, mr_hot): `tr_core::nr_matrix` runs the first fifteen with `retf=1` and without in every trace mode x {plain, `+verify`, exit storm 3 / 7 + randra, budgets 3 / 17, `-loop`, `+emurun +verify`, GC stress 1 and 2 in both collector modes, GC verify, stack moves}; `nr_retf_traces_are_installed` requires installed RETF traces (`[trace] sink: ... retf N`, printed only when non-zero) for `n_retf_hot`, `_basic`, `_tail`, `_two_sites`, `_mr_hot`.
- **jit-diff set `nr`** (`JITDIFF_TSET=nr`): callee / caller pairs, trip counts {0, 1, 2, random}, plain / tail / method / `__call` / `pcall` / `table.sort` comparator / `string.gsub` callback / coroutine, 0..3 results, optional closure capturing a caller local and `k` returns, vararg callers, recursion. `RUSTYNAIL_JIT_TRACE_LIMITS=lockstep=1,retf=1,retfmax=1 JITDIFF_TSET=nr JITDIFF_MODES="off tr trdef trstorm tremu trstack" scripts/jit-diff.sh 1 100 bin`; `trdef` ignores the limits (the shipped default, retf 0). Seeds 1..40: RETF traces installed in 25 of 40 seeds.
- **Mutants** (registry 606, all caught with `TR_MUT_ONLY=retf_,seg_`): `seg_root_code_stale`, `seg_land_proto_stale` (tr_exit), `retf_callstatus_unchecked`, `retf_cframe_unchecked`, `retf_site_unchecked` (`hand_retf_guards_exit_before_the_pop`), `retf_k_unchecked`, `retf_shift_off`, `retf_no_ci_store`, `retf_no_reloadk` (tr_asm), `retf_uvrel_cse`, `retf_model_uvrel_unshifted` (tr_ir), `retf_slot_remap_off`, `retf_results_dest_off`, `retf_multret_top_unmapped`, `retf_root_func_stale`, `retf_ci_stays` (`tr_core::nr_retf_traces_are_installed`: the lockstep aborts the recording or the segment oracle / output fails). NOT registered because equivalent while the recording stops right after the pop (the code is there, no mutant hook): `known` unshifted, `func_ref` stale, model slots unshifted, entry-guard hoisting past the pop, extent / max_ci unshifted: they come with N3a.
- `RUSTYNAIL_JIT_MUT` names that are not in `NAMES` panic when a mutant is selected: do not leave `mutants::active("x")` calls without a registry entry.

## BUNDLE-N-B (N3a, N3b, N4, N5)

- **Programs** (`tests/tr_core/nr_progs.rs`, now 30 `n_retf_*`): continue, untraced, relink, relink2, recursion, two_levels, after_exits (getlocal / traceback / error / GC after the pop), types (caller registers of other types than the callee's at the same positions), upvals (callee and caller closures with different upvalues), poly (the caller's register changes type after the recording), swap (callee-identity guard of a call made after the pop; the function changes mid-run), failafter (an unrecordable op after the pop). `tr_core::nr_matrix` runs all of them with `retf=1,retfmax=1`, `retf=1,retfmax=2`, `retf=3,retfmax=2`, `retf=7,retfmax=3` and with the knob off in every trace mode x {plain, +verify, exit storms, random allocation, budgets, `-loop`, emulator, GC stress 1/2 in both collector modes, GC verify, stack moves}; `tr_core::nr_n3_programs` (retf=7,retfmax=3: `always +verify`, exit storm 3 / 7 + randra, against the knob off, with "a RETF trace is installed" for the programs that must have one); `tr_core::n3b_relink_stub_programs` (`c38_side`, `c38_side2` with `sidenest=1,retf=2`: the stub is used, same output in every mode, the loop trace is entered as often as with the handler); `tr_policy::n3a_child_room_is_translated_by_the_segment`; `tr_exit::n3b_link_stub_inputs_equal_the_handlers`.
- **jit-diff set `nr`** gets three templates (two levels / the callee twice; a caller loop after the call + upvalue write + getlocal / error; recursion between two calls): `JITDIFF_TSET=nr` seeds 1..100 modes `off tr trdef trstorm tremu trstack` with `lockstep=1,retf=7,retfmax=2`: 0 diffs; defaults + reference build: 0; seeds 101..200 with `retf=3,retfmax=1`: 0; the general set `tr` seeds 1..150 with `retf=7,retfmax=2` forced: 0 diffs (17 slow skips); `tr` 201..330 with defaults + reference build: 7 diffs, all `ref` (our JIT-off interpreter vs the reference binary, not a trace mode) and none in any trace mode; `cc` (the `tfmt` templates) 1..80 with the reference build: 0.
- **Mutants** (registry 606 -> 617, all caught with `TR_MUT_ONLY=retf_,seg_,relink_`; the family run `retf_,seg_,relink_,side_,fn_,lazy_,sink_,snap_,fside_,sidenest`: 88 of 89 caught, the survivor is the known `fside_room_ignored`): `retf_known_unshifted`, `retf_func_ref_stale`, `retf_model_func_stale`, `retf_entry_hoist`, `retf_entry_checks_past`, `retf_untraced_aborts`, `retf_second_return_aborts` (`nr_n3_programs`, `nr_retf_traces_are_installed`), `retf_extent_unshifted`, `retf_maxci_unshifted` (`tr_policy::n3a_child_room_*`), `relink_jumps_to_land`, `relink_savedpc_off` (`tr_exit::n3b_link_stub_inputs_equal_the_handlers`). Not registered: the spec's `retf_alias_ignored` (the `RetfAlias` abort is conservative: the recording is consistent without it, see docs/JIT_TRACE_REF.md), `retf_sink_loaded_unscoped` (sink is off for traces with segments), the RETF-failure fallback (`cache::abort_retf`: only observable through a function-entry family, where a mutant that blacklists the exit instead changes no output). `fside_room_ignored` still survives: a frameful child with a larger extent than its parent needs the stack within a frame of its limit; none of the programs gets there (`child_room` itself is unit-tested).
- Builds: 5.4 (`RUSTFLAGS="--cfg rustynail_lua54"`): the 30 programs with `retf=7,retfmax=3` in `always` and `+verify` equal the JIT-off output, 0 RETF traces (inert). ASan (`-Zsanitizer=address -Zbuild-std`, `RUSTYNAIL_SYSTEM_MALLOC=1`): the 30 programs x {always, +verify, +exitstorm=3} with `retf=7,retfmax=3`: 0 AddressSanitizer errors, outputs equal; jit-diff `nr` 1..60 (`off tr tremu trstorm`, retf 7) and `tr` 1..40 (retf 3): the only diffs are LeakSanitizer reports (seeds 48, 55, 32, 37), which the same programs give with `retf=0,sidenest=0` (the pre-existing trace-memory leak at exit, not chased).

### BUNDLE-C45 (2026-11-10): `rearmside=2` + `rearmclean=1`

- `tr_core::bp_rearmside_programs` (programs `bp_rearm_young_clean`, `bp_rearm_young_dirty`, `bp_rearm_young_la`; `always` / `on2` / `on` x `rearmside` 0 / 1 / 2 / 64 and `rearmside=2,rearmclean=0`, plain and `+verify`, exit storms, budget 17, the emulator, GC stress 1 and 2, `stackmove`; same output as the interpreter everywhere). With the production tick period (`on`): the clean twin (a loop in a function called from a main-chunk loop; leaving the loop continues in compiled code, no failed side recording) is entered by young rearm ticks (`rearm_enters` above the `rearmside=0` run, no refusal); the dirty twin (a loop inside an outer `for`: the loop exit is a failed `SideUntraced` side recording) is entered exactly as often as with `rearmside=0` and refusal counter F > 0, and **is** entered with `rearmclean=0`; the `la` twin (a `while` loop in a callee method unrolled into the caller's trace; run with `wgate=30000` so that the callee loop is not traced as a root first) has refusal counter L > 0. Statistics line: `rearm: ... unclean F/L`.
- `tr_policy::bp_rearmside_rule` (pure rule over clean / unclean x `rearmclean` 0 / 1), `tr_policy::bp_process_defaults` (`REARMSIDE_PROCESS` = 2, `REARMCLEAN_PROCESS` = 1, named values win, `rearmclean` clamped to 1). `tr_determinism::rearm_leaves_formation_alone` names `rearmside=0` (the C45 young entries are a formation change by design).
- Mutants (registry 617 to 620): `rearmclean_ignores_faults` (programs), `rearmclean_ignores_la` (`bp_rearm_young_la`), `rearmclean_default_ignored` (`bp_process_defaults` and programs); with `rearmside_never` / `rearmside_any`: `TR_MUT_ONLY=rearmclean_,rearmside_` (5 of 5 caught, 8 s after the build). The stale `bp_rearm_young` program was split into the three twins above.

## RETF flip: `retf=3` is the process default (2026-11-10)

- **Tests that pin process defaults.** `tr_policy::bn_retf_process_default` asserts `RETF_PROCESS == 3`, `RETFMAX_PROCESS == 1`. `tr_core::nr_matrix` runs the 30 `n_retf_*` programs with `retf=1,retfmax=1`, `retf=1,retfmax=2`, `retf=3,retfmax=2`, `retf=7,retfmax=3`, `retf=0` (control: knob off) and `""` (the process default, now 3). Tests whose premise is the shape of the pre-RETF traces name `retf=2` explicitly: `c39_inner_programs`, `frame_links_and_stitches`, `p_laprune_programs`, `j1_sinkstub_alloc_failure_raises_memory_error` / `sink_materialise_alloc_failure_raises_memory_error` (GC stress 1 job), `side_traces_install_and_run` (`sidemin=1000`, a post-RETF stitching exit is not measured by `sidemin`).
- **Segment oracle fix.** `exit::seg_oracle_check` takes the entry function slot read BEFORE `restore` (`seg_oracle_entry_func`); with RETF on by default 15 `tr_core` tests (`bl_programs`, `dm_programs`, `class_a_helpers_run_on_traces`, `pf_variable_step_loops`, `px_typed_landing_labels`, `twins_options`, `s_loopany_guard_programs`, ...) tripped over the reused `CallInfo` node.
- **Runs** (`--features trace`, `--test-threads=3`, all pass; `tr_core`, `tr_exit`, `tr_asm`, `tr_policy`, `tr_hook`, `tr_passes`, `tr_ir`, `tr_record*`, `tr_runtime`, `tr_loop`, `tr_ra`, `tr_coro`, `tr_bd` and the registry tests were rerun on the final tree, the other binaries and the tier-1 suites ran on the tree with the new default before the oracle fix, which only touches the `+verify` oracle): `tr_core` 182 (`--skip mutants_are_caught_by_live_programs`), `tr_policy` 49, `tr_asm` 46, `tr_loop` 27, `tr_record` 34, `tr_exit` 36, `tr_ra` 19, `tr_determinism` 2, `tr_runtime` 43, `tr_record_calls` 37, `tr_hook` 14, `tr_poll` 3, `tr_passes` 15, `tr_ir` 18, `tr_class_a_pb` 19, `tr_hashd` 6, `tr_coro` 5, `tr_bd` 2, mutant registry tests; `cargo test -p rn-core -p rn-std --features jit`; the tier-1 suites (`jit_arith`, `jit_table`, `jit_calls`, `jit_loops`, `jit_lifetime`, `jit_alloc`, `jit_stress`, `arith_diff`, `table_diff`, `emu_diff`, `emu_ops_diff`, `jit_review_{hooks,count}`, `jit_cstack`, `t15_analysis`, `t15_core_emu`, `t15_runtime`); `cargo check -p rn-jit --all-targets` for `jit`, `jit,trace`, `t15_oracle`, `t15_mutants`, `tr_mutants`; 5.4: `RUSTFLAGS="--cfg rustynail_lua54" cargo check -p rn-core -p rn-std -p rn-jit --features rn-jit/trace` and `-p rn-cli --features jit`.
- **Mutants** (`TR_MUT_ONLY=retf_,seg_,relink_,side_,fn_,lazy_,sink_,snap_,fside_,sidenest,rearm`, 3 shards, 101 mutants matched, about 18 minutes wall): 100 caught, 1 survivor, the known `fside_room_ignored`. Per family (substring match, so `stitch_side_empty`, `frame_side_never`, `polyside_default_ignored` count under `side_`, `fcfn_*`, `libk_type_fn_skip` under `fn_`, `sinkside_dead_all` under `sink_`, `wgate_full_rearm` under `rearm`): `retf_` 23, `seg_` 2, `relink_` 2, `side_` 19, `fn_` 9, `lazy_` 16, `sink_` 9, `snap_` 3, `fside_` 7 (6 caught), `rearm` 11; `sidenest` has no mutant of its own (`side_frame_link_never` is its detector).
- **jit-diff** (seeds 1..150, modes `off tr trdef trstorm tremu trstack` + `ref` = the reference build `ref/55/build/lua`, `JITDIFF_ENTERS=1`, new defaults; `tr` / `tremu` / `trstorm` run with `lockstep=1`, `trdef` with the bare process defaults): default set 5 diffs, all in the `ref` column (seeds 27, 106, 110, 114, 128: the `GCOBS: count-delta` / `finalized` lines of `collectgarbage("count")` differ between rnlua and the reference, no JIT mode differs); `nr` 0, `bs` 0, `oop` 0 diffs.
- **ASan** (`ASAN_TRACE_ONLY=1 ASAN_TRACE_SEEDS=40 scripts/asan-jit.sh`): official suite (default, `JIT=always TRACE=always`, `JIT=on TRACE=on`), the tests/vm corpus (always, `+exitstorm=3`, `+emurun`, passes off), GC stress, stackmove and all `tr_*` test binaries (`tr_core` 182 passed in 26 minutes) clean. The script ends `ASAN-JIT FAILURES` only because of the jit-diff step: seeds 3, 17, 20, 32, 37 in `tr` / `tremu` / `tron` / `trstorm` are LeakSanitizer reports (leak at exit of trace memory), which the same programs give with `retf=0,sidenest=0` (seed 3: 360 KB leaked there against 397 KB with `retf=3`, seed 17: 319 / 340 KB, seed 32: 620 / 614 KB): the known pre-existing leak, not chased.
- **Not run:** fuzz targets, the full 622-mutant runner (only the families above), `t15_oracle` / `tr_oracle` test binaries.

## BUNDLE-V (tier-1 metamethod frames, `t1q` bits 4 / 5 / 6; recorded upvalue stores, `upbar`)

- **New tests.** `tr_core::bv_mm_programs` (program `bv_mm`: every arithmetic / bitwise / unary operator with a table on either side and with constants (`MMBINI` / `MMBINK`); handlers that tail call a Lua function / a vararg function / a native / a `__call` table / a `__call` chain / a native `__call`; handlers returning 0 / 1 / many / odd values; erroring handlers (message, tail callee errors, table errors, traceback line counts); `debug.getinfo` / `getlocal` / `traceback` / `sethook` from handlers; yields (also a tail-called `coroutine.yield` and coroutines sharing a table); the handler or the metatable mutated while the handler runs; deep and runaway recursion (`C stack overflow` message); `__lt` / `__le` / `__eq` / `__len` / `__concat` handlers with tail calls; the vararg `__call` constructor (the `oop_mixins` shape); closures created by handlers (open upvalues closed on return); native tail callees in `__index` / `__newindex` frames (refused: the count is given back)); modes `t1 always on2 on` x `t1q` 13 / 29 / 45 / 77 / 125 / 112 / 1, plain and `+verify`, `+exitstorm=3`, `+emurun`, budgets 17 / 1000, GC stress 1 / 2, stack moves; with `t15_oracle` the instruction-count identity jobs (budgets None / 3 / 17 / 200) at `t1q=125`; in `t1` the bytecodes executed by compiled code rise with the bits (229871 -> 242253 (bit 4) -> 286392 (all)). `tr_core::bv_upbar_programs` (program `bv_upbar`: minimal closed / open / swapped upvalue stores, an LRU list with head / tail upvalues, stored kinds (tables, strings, functions, nil, false, numbers, coroutines, files), an upvalue aliasing a register of the loop's frame, open upvalues shared between coroutines, closed upvalues, stores from metamethods) in `always on2 on` with `upbar` 0 / 1, `+verify`, exit storms, GC stress 1 / 2 in both collectors (also with `RUSTYNAIL_GC_VERIFY=1`), stack moves, the emulator. `tr_record::bv_upvalue_store_records_the_barrier`, `tr_asm::hand_upvalue_tbar_calls_the_barrier_exactly_for_black_upvalues_and_white_values`, `tr_policy` (`T1Q_PROCESS` 125, `UPBAR_PROCESS` 1, `t1q=99` clamps to 0xE3, 255 to 0xFF), jit-diff template set `bv` (`JITDIFF_TSET=bv`: arithmetic frames with tail calls / handler replacement mid-loop, erroring / yielding / debug handlers, an upvalue list with GC steps).
- **Lesson (test design): a loop that calls `collectgarbage("step")` aborts the recording (`CFunc`), so a "GC-driven" upvalue test with the call in the loop never ran a trace.** `bv_upbar` calls the collector between the runs only; the barrier itself is covered by the hand-built `tr_asm` test (all black / white mark combinations, `b` = 1, the store happens either way) and the `tr_record` test (the `TBAR` follows the `USTORE`); in real runs no program of the corpus had a black upvalue and a white value at a recorded store (the helper was never called in `bv_upbar` under any stress mode).
- **Mutants (registry 620 -> 624):** `bv_tail_nil` (the native tail callee's value is nil), `bv_ret_first_nil` (a multi-value return gives nil) (`bv_mm_programs`), `upbar_skip` (`tr_record`; `bv_upbar_programs` does not catch it, see above), `upbar_asm_skip` (`tr_asm`). `TR_MUT_ONLY=bv_,upbar_,armm_,s1_,amt_`: 21 of 21 caught (the `armm_*` rows now also exercise bit 4). A mutant that skips the vararg `func` shift (`bv_vararg_delta_skip`) was dropped: the wrong `top` after the pop is not observable by output.
- **Runs.** `tr_core` 184 (`--skip mutants_are_caught_by_live_programs`, `--test-threads=3`), `tr_policy` 49, `tr_asm` 47, `tr_loop` 27, `tr_record` 35, `tr_exit` 36, `tr_ra` 19, `tr_determinism` 2, `tr_runtime` 43, `tr_record_calls` 37, `tr_hook`, `tr_poll`, `tr_passes`, `tr_ir`, `tr_class_a_pb`, `tr_hashd`, `tr_coro`, `tr_bd`, the registry tests, the tier-1 suites (`jit_arith`, `jit_table`, `jit_calls`, `jit_loops`, `jit_lifetime`, `jit_alloc`, `jit_stress`, `arith_diff`, `table_diff`, `emu_diff`, `emu_ops_diff`, `tr_bd`, `jit_review_hooks`, `jit_review_count`, `jit_cstack`, `t15_analysis`, `t15_core_emu`, `t15_runtime`), `cargo test -p rn-core -p rn-std --features jit`, `cargo check -p rn-jit --all-targets` for `jit`, `jit,trace`, `t15_oracle`, `t15_mutants`, `tr_mutants`, the 5.4 check (`RUSTFLAGS="--cfg rustynail_lua54" cargo check -p rn-core -p rn-std -p rn-jit --features rn-jit/trace`): all pass. jit-diff seeds 1..100, modes `off tr trdef trstorm tremu trstack`: sets `bv`, `tr` (default), `bs`, `nr`: 0 diffs; `bv` with `off tr` + the reference lua (`JITDIFF_REF`): 0 diffs.
- **Not run:** the full 624-mutant runner (the families above only), ASan / fuzz, PGO.

## BUNDLE-X: library extensions (`table.new/clear`, `string.buffer`, `jit`)

Tests live in `tests/ext/` (see `docs/EXTENSIONS.md`): `scripts/ext-diff.sh` runs each program under LuaJIT 2.1 and `rnlua` and requires identical output
that equals the checked-in `.expected`; `scripts/ext-matrix.sh` repeats it under `RUSTYNAIL_JIT=off|always`, `RUSTYNAIL_JIT_TRACE=always`, `RUSTYNAIL_GC_MODE=inc`
and `RUSTYNAIL_GC_STRESS=1|2`; `cargo test -p rn-cli --test ext` is the LuaJIT-free version. The JIT-facing programs are `rn_clear_jit.lua` (`table.clear` in hot
loops, with traces/tier 1 caching array sizes and node positions, against a Lua reference implementation) and `rn_jit_ctl.lua` (`jit.flush()` / `jit.off()` / `jit.on()` inside
hot loops, recursion with compiled frames on the stack, metamethods and coroutines; checksums must not change). Under `RUSTYNAIL_JIT=verify` and `RUSTYNAIL_GC_VERIFY=1`
on the debug build both pass. Fuzz: `buffer_ops`, `string_lib`, `table_ops`, `run_source` (`fuzz/`).

## Release triage (BUNDLE Y, 2026-11-10)

Every test that was known to fail or survive on the base commit (975814d9) was reproduced, root-caused and either fixed or turned into a documented
expectation. The state afterwards, per item:

- **Y1 `t15_core::mid_run_mutation_programs`, `t15_core::table_regions_are_generated`, `t15_stress::random_float_programs`, `t15_policy::*`: fixed (production bug).**
  The `debug_assert!(same, "block structure differs between code versions")` in `rt::recompile` checks that every code version of a prototype has the
  same `BlockSig` (block heads, prepaid counts, resume entries). It was a genuine invariant violation, not a stale assertion: `lower()` appended every
  `cx.mm_resume` / `cx.s1_resume` continuation whose pc was not already a resume entry, but the set of known pcs was computed once, so an instruction that
  is lowered twice (the main and the warm-up variant of an arithmetic instruction) pushed its pc twice and the version's `resume_pcs` read `[0, 5, 9, 9]`
  against version 0's `[0, 5, 9]`. In a release build the new version was refused (`Fail::Permanent`): the tier-1.5 recompile was silently lost.
  Fix: the first continuation per pc wins (`HashSet::insert` as the filter). `t15_core` 63, `t15_policy` 26, `t15_stress` 88 tests pass.
- **Y2 oracle tests.** `t15_oracle::oracle_corpus`: *stale harness*. The model cache of the oracle is keyed by (prototype address, code address, length,
  linedefined, maxstacksize); in `gen:9` a freed prototype's memory was reused by a different `load`ed chunk of the same shape, so the oracle checked the
  effects of the wrong instructions (`Getfield: slot 3 changed ... not in defs` and `Loadi: slot 3 in defs but not written` on two different keys). The lookup
  now also compares the code words (230 programs, 139 enter a region, 0 violations). `tr_identity_pre::fetch_plus_insns_identity` passes since Y1 (it
  aborted at the same assertion in the middle of the run; not re-verified on the base binary). `tr_oracle::oracle_exits` found a *test-machinery* bug:
  with `+exitstorm=7 +randra=5` (also without `+emuverify`) `table.insert(u, i)` stored the element twice (`#u` 752 for 750; `bench/programs/table_array`
  printed 377 instead of 375). `table.insert` / `u[#u + 1] = v` is `TLEN` + `TSET`; the guard `NE tset, 0` fails only when the helper stored nothing, but the
  storm (`asm::guard_label`) failed it behind a successful store and the exit resumed at the `CALL`. The storm already exempted the same class of guards
  (`alloc_guard`: `TNEW` / `SNEW` / `CONCAT`, `GMATCH`, `RANDOM`); `TSET` is added. Not reachable without the storm option (a natural failure of that
  guard means nothing was stored). Regression test `tr_core::y2_tset_storm_programs` (program `y2_insert_storm`; 3 of its 18 jobs fail without the fix).
  After the fix `oracle_exits` (3438 jobs), `oracle_options` (3438) and `oracle_trace_modes` (4039) pass. **Documented non-failures of these tests:** the
  `inconclusive` jobs are not failures: `co_14_cstack_overflow`, `err_16_where_prefix`, `core/bk_obs2`, `core/m1_interp` hit the 300 s leash with tiny
  budgets (3, 17) or storms (millions of logged exits), 14-16 jobs per test; `gen:6` ends with `os.exit(3)` before the child prints its completion marker
  (7 jobs of `oracle_trace_modes`). Each of the three tests takes 32-35 minutes with 3 workers.
- **Y3 jit-diff `ref` column: harness change (documented expectation).** The diffs of seeds 27, 106, 110, 114, 128 (modes `off tr` + `ref`; the plain
  `off always ref` set is clean) were only `GCOBS:` lines (`count-delta`, `finalized`, `count-after-collect` of the F8 template): the reference lua's
  collector pacing and memory accounting differ by design. `same()` in `scripts/jit-diff.sh` now ignores `GCOBS:` lines when either mode is `ref` (it
  already did for the sinking trace modes). Program output (`mix` values, final hash) was identical. Seeds 1..130 `off tr ref` and `nr` 201..330 `off tr ref`:
  0 diffs; seeds 1..300 with `JITDIFF_ON=1`, modes `off always stack tr trdef trstorm tremu trstack` + `ref`: 0 diffs (80 slow skips of stack modes, by design).
- **Y4 `fside_room_ignored`: detector found (a table gap, no new program needed).** `tr_core::r2_side_room_programs` was never its listed detector: `r2_side_room`
  contains a frameful side trace (inside the inlined `B:step` frame; install log `extent 166 depth 2 (parent 15 2)`) that is entered right after a
  `collectgarbage()` shrank the stack; without the install-time room check (and with it, the start check of `siderooms=1`) the trace writes past the stack and
  the child aborts (`malloc(): unsorted double linked list corrupted`, 5 of 5 runs). The mutant row now lists that test; the test also asserts that
  `siderooms=0` refuses at least one side trace with `Compile(side-room)` (deterministic witness that the check fires on that program).
  `TR_MUT_ONLY=fside_room_ignored,side_room_`: 3 of 3 caught. The "still survives" statements above (BUNDLE-S / M / N-B) are superseded by this.
- **Y5 upvalue barrier coverage.** The premise was only partly true: `bv_upbar` already reached the `b = 1` path of `jit_tr_barrier` (4 calls, incremental
  collector). Real-run coverage is now measured: `asm::BARRIER_CALLS` counters, printed as `[trace] barrier helper calls: table T upvalue U` and parsed by
  `tr_core`; `tr_core::y5_upbar_black_programs` (programs `y5_upbar_black` and `y5_upbar_black_small` in `Y_PROGS`, which is not part of `selected()`): a
  40000-table heap and a traced loop that stores a fresh table into two closed upvalue cells (no `collectgarbage` in the loop; `collectgarbage("step")` only
  between runs). The test asserts upvalue barrier calls > 0 in the incremental run (8 calls) and in the generational run (41517 calls: old cell and closure,
  young table, the forward barrier of `OP_SETUPVAL` marks the table) and the interpreter's output; the small variant runs under `RUSTYNAIL_GC_STRESS=1|2`
  with `RUSTYNAIL_GC_VERIFY=1` (`Heap::verify` after every step) in both modes. Under GC stress the collector never leaves a black cell for a trace to hit
  (counter 0 there), so the barrier is exercised by the unstressed runs and verified by the stressed ones. Mutants `upbar_skip` and `upbar_asm_skip` abort
  these real runs and list the test.
- **Y6 5.4 `all.lua` / `gc.lua:487`: not reproducible, documented.** On this tree (and with `lower.rs` reverted to the base) `cargo xtask official
  --lua-version 5.4 --only all.lua,gc.lua` passes, the 5.4 release binary passes `all.lua` 40 of 40 times, the debug binary 90 of 90 times with six
  concurrent runs, and the JIT modes `off`, `always`, `on` (threshold 2), traces always, generational / incremental collector all pass. The failing
  assertion (`collectgarbage("count") <= m + 1`, 1 KB margin after the weak-key block) is a sensitive memory-accounting check; a probe that varies the
  number of live interned strings (0..3000) around the block shows no growth in either our 5.4 build or the reference 5.4 (no string-table resize effect).
  The failure of Bundle X's run could not be tied to a source change (the base commit does not fail either). If it appears again: run `cargo xtask
  official --lua-version 5.4 --only all.lua` and keep the stderr of the failing run. No code change, no status file change.
- **Y7.** `rnlua --version | head -1`, `rnlua --license`, `rnluac -v` and the `rnluac -l` listing with a closed stdout no longer panic (`license::quiet_println`,
  write errors ignored). `docs/RELEASE.md` is a release checklist (repository URL set to `https://github.com/kpatvt/rustynail`
  ). `cargo fmt --check`: 88 files / 1667 hunks differ: not applied (limit 15 files). `cargo clippy --release` (library crates): about
  190 style warnings (missing docs, safety-comment lint, precedence, `clone` on `Copy`), no correctness findings; none fixed.

### Known exceptions after BUNDLE Y

No test of the known-good list fails. Not passing or not decided: the `inconclusive` jobs of the three `tr_oracle` tests (leash timeouts, `os.exit`: above),
the Y6 failure that could not be reproduced, and the 88 files `cargo fmt` would touch.
