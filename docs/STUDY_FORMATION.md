# Study: the cost of one trace formation step (2026-11-24, base e9a66cc, LuaJIT 2.1.1795119671)

Question (from docs/STUDY_CAUSAL_GAP.md section 6, candidate 2): on raytrace, cd, richards and fmt_s most of the cycle gap to LuaJIT is warm-up.
That study split the warm-up into "compile" (16 / 48 / 8 / 0.6 Mcyc, measured by doubling the back end) and a "remainder" of 24 / 50 / 11 / 3.8 Mcyc,
which it read as execution in tier 1 and the interpreter while traces form. What does one formation step cost, how many steps do we take, where
do the extra warm-up cycles really go, and which builder would remove them?

Tags: **[M]** measured here (scripts and data below), **[D]** derived from [M] numbers by the arithmetic stated. Nothing is a bucket share
turned into a gain.

## 0. Answer in one page

1. **The warm-up is formation, not execution.** Sampled phase by phase with the run-it-twice method (rep 1 minus the steady repetition, both
   measured in the same binary; section 2.2), recording + trace back end + tier-1 compile are **72-81 % of the warm-up** on raytrace, cd,
   richards and towers [M]:

   | Mcyc | raytrace | cd | richards | towers | fmt_s | deltablue | classic_vector |
   |---|---:|---:|---:|---:|---:|---:|---:|
   | warm-up (rep 1 - steady rep) | 43.9 | 125.2 | 23.3 | 14.7 | 4.7 | 41.1 | 29.5 |
   | of which recording + back end + tier-1 compile | **35.7** | **93.8** | **17.9** | **10.6** | 1.7 | 24.4 | 19.4 |
   | of which interpreter + tier-1 + traces + exits running longer | 9.8 | 26.6 | 2.9 | 2.0 | 0.5 | 13.7 | 6.7 |
   | LuaJIT: all formation attempts (start to stop/abort, section 3) | 5.6 | 38.7 | 6.3 | 5.8 | 0.2 | 71.6 (lottery) | 5.2 |

   The previous study's "remainder" was mostly compile. Two measurement errors caused it: (a) its TSC-to-cycle factor 1.4 is wrong; on this
   machine one TSC tick is **2.52 core cycles** [M, busy loop under `perf stat`], so every phase-timer figure was 1.8x too small; (b) a
   doubled compile runs with warm caches and costs about 0.6 of the first one (raytrace: doubling 16.1 Mcyc vs 26.3 sampled for the same
   back end + tier 1).
2. **The number of steps is not the problem; the cost of each step is.** We take about as many formation steps as LuaJIT, often fewer
   (cd: 238 installed traces vs LuaJIT's 572), but a successful side trace costs **216-957 kcyc against LuaJIT's 34-122 (5.5-10x)** and a
   root trace **340-3600 kcyc against 34-322 (10-20x)** [M, median per attempt, section 3.1]. Formation therefore also finishes late: half
   of raytrace's traces are installed at 31.8 Mcyc (LuaJIT 7.9), richards' at 18.7 (4.3), towers' at 10.4 (4.4).
3. **The other warm-up mechanisms are small** [M]. Failed recordings cost 2-16 Mcyc of attempt time; deleting them a priori (scratch
   blacklist from a pre-pass) gains -2 / -4 / -1 / -8 / -10 / -4 % on raytrace / cd / richards / towers / deltablue / havlak, but
   +22 % on lua_lexer (a blacklisted head was needed). Tier-1 code +64 % larger costs only +1-2 % (so a diet saves at most that). Our
   interpreter is 1.17-1.66x LuaJIT's `-joff` and our tier 1 0.93-1.41x, but the interpreter/tier-1 excess during warm-up is 1-18 Mcyc,
   of which a LuaJIT-speed interpreter would save under 2 Mcyc. Formation exits through the handler add 1-2 Mcyc. Hot thresholds,
   side-exit counts, `sidemin`, `heatside`, `maxside`, `wgate`, tier-1 thresholds: every knob is within +-5 % on most programs, with
   mixed signs (best single knob per program -2 to -9 %, none good everywhere). A faster malloc (tcmalloc / jemalloc preloaded) is slower.
4. **Builders, ranked** (section 4): (1) **a cheaper formation step**: side traces first (70-94 % of the successful steps on most programs), cut the passes /
   snapshot work per step and the cold-start cost of the compile path; measured ceiling 23-39 % of the program on raytrace / cd / richards /
   towers / deltablue, expected **-6 to -10 %** there at factor 0.5 for a 2x cheaper step. (2) **Fewer failed recordings by a cheap
   predictor** (FnShort / TooLong / InnerLoop heads): ceiling 2-10 %, expected -1 to -5 %, needs a guard against lua_lexer's loss.
   Drop: tier-1 code size diet, interpreter speed for warm-up, threshold / policy tuning, allocator swap, formation-exit path.
5. **Previously queued handles** (section 5): code-changing compile levers are now the top item (they ARE the formation step). Frame model
   (fib 4.4 %), function-entry roots (0 to -1 %), NaN-boxing (at most 3-4 % on recursion programs): drop.

## 1. Method

* Binaries: `cargo build --release -p rn-cli` at e9a66cc (plain), copied out. A scratch build of the same commit (never committed, removed)
  adds `RNHACK` switches: bit 1 = blacklist the root heads / side exits listed in `RNHACK_BLIN` at their first hot event; bits 2 / 4 / 8 =
  tier 1 emits `jmp L; <1 | 120 | 250 NOP bytes>; L:` in front of every bytecode (same instructions executed, different code size);
  `RNHACK_BLOUT` = append one line per formation event (`rdtsc`, `START R|S`, `OK`, `F <reason>`) with a stable key (root: proto line + head
  pc; side: root line, head pc, parent-is-side, parent snapshot count, exit). Perf map names `TRACE-<n>r` / `TRACE-<n>s` split root and side
  trace code. The scratch binary with `RNHACK=0` is the baseline of every hack comparison; the phase profiles used it too (its default
  behaviour is that of e9a66cc). LuaJIT: `/usr/bin/luajit`, stripped (building an unstripped one was not possible in this session).
* Totals: scripts/causal-grid.py (`taskset -c 4`, 300 ms spin before each run, `RUSTYNAIL_SEED=0`, padded environment, stdin /dev/null,
  `cycles:u,instructions:u`, 9 runs, median, configurations rotated). Nothing else ran during any series (checked before each).
* Phase profiles (scripts/formation-phases.py): `perf record -e cycles:u -c 100003` (period above the 100 k/s throttle: a 20 000 period lost
  half the samples), leaf symbol per sample, classified into interpreter / recording (`run_mode<true>`, the recorder) / tier-1 code / root
  trace / side trace / exit stubs / exit handler / trace back end / x86 encoder / tier-1 compile / compile support (`tr::ir`, `hashbrown`,
  `alloc`) / JIT helpers / GC / core runtime / library / libc. Dwarf call chains through JIT frames came back empty, so libc and Rust support
  samples are attributed to the phase of the nearest preceding primary sample (`--ctx`; without it they are a separate column, section 2.2
  shows both). 5-41 runs per program and repetition count (`CAUSAL_REPS` = 1 and 3 of scripts/causal-reps.lua), means.
* Warm-up by phase = phase(rep 1) - (phase(3 reps) - phase(rep 1)) / 2. The totals agree with STUDY_CAUSAL_GAP 3.1 within 5 %.
* Formation events: ours from the scratch event log (5 runs per program), LuaJIT from scripts/formation-lj.lua (`jit.attach` trace events
  stamped with `os.clock()`, 9 runs). An attempt runs from its start to its install / abort and includes the interpreter executing the
  recorded bytecodes. Cycles: ours `rdtsc` x 2.52 (TSC ratio, [M] `tscratio` busy loop: 402.6 M cycles in 160.0 M ticks), LuaJIT seconds x the
  cycles / task-clock of the same run (4.15-4.87 GHz).
* LuaJIT phases: perf samples in JIT machine code vs libluajit vs libc (`formation-phases.py lj`), and `-jp=vi1` VM states averaged over 30
  runs (Interpreted / Compiled / JIT Compiler / GC / C).
* Startup [M]: `rnlua -e ''` 1.17 Mcyc, `luajit -e ''` 1.00 Mcyc: not a factor.

## 2. Deliverable 1: timelines

### 2.1 Ours: phase shares per window of the first run [M] (% of the window's cycles; `--ctx`; compile = back end + encoder + tier-1 compile + support)

raytrace (97 Mcyc; 10 Mcyc windows):

| window | interp | record | tier1 | trace | exit | compile | helpers | gc+rt | other |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| 0-10 | 6 | 6 | 4 | 0 | 1 | 51 | 1 | 28 | 4 |
| 10-20 | 1 | 13 | 17 | 5 | 1 | 38 | 2 | 19 | 0 |
| 20-30 | 0 | 13 | 5 | 2 | 2 | 58 | 1 | 16 | 1 |
| 30-40 | 0 | 21 | 3 | 2 | 2 | 55 | 1 | 14 | 1 |
| 40-50 | 0 | 8 | 17 | 8 | 3 | 32 | 3 | 18 | 1 |
| 50-60 | 0 | 2 | 26 | 17 | 3 | 11 | 5 | 16 | 0 |
| 60-90 | 0 | 0 | 33 | 22 | 3 | 3 | 4 | 13 | 0 |

cd (260 Mcyc; 20 Mcyc windows): compile 49-55 % of every window up to 80 Mcyc, 40 % at 80-100, 29-32 % at 100-140, 9-14 % after; exits 2-5 %
early, 15-19 % from 140 Mcyc on (steady: cd's exits are a steady-state item, 31 Mcyc per repetition in stubs + handler).

richards (60 Mcyc; 5 Mcyc windows): compile 33 / 55 / 75 / 42 / 34 / 36 % in the first six windows (record another 0-11 %), trace code 0 % until
15 Mcyc, 9-13 % at 15-30, 22-30 % after 30 Mcyc (the rest of the steady state is tier 1 and side traces in stubs).

towers (48 Mcyc; 10 Mcyc windows): compile 43 / 26 / 11 / 5 %, trace 3 / 16 / 26 / 26 %. fmt_s (7 Mcyc; 1 Mcyc windows): startup + library
(gc+rt 80 / 61 %) for 2 Mcyc, compile 38-60 % at 2-4 Mcyc, the one loop trace from 4.1 Mcyc. lua_lexer: compile 47 % of the first 40 Mcyc, then
<= 1 %. classic_vector: compile 33 / 58 % in the first 40 Mcyc, then trace 33-40 %. havlak: compile 28 / 25 / 18 % in the first 60 Mcyc.
deltablue: compile 20-40 % of each 10 Mcyc window up to 70 Mcyc, interpreter 6-13 % (the largest interpreter share of the set).

### 2.2 Warm-up by phase, the run-it-twice ground truth [M] (Mcyc; rep 1 - steady repetition)

With libc / Rust support attributed by context (`--ctx`):

| program | rep 1 | steady | warm-up | interp | tier-1 code | root traces | side traces | stubs + exit handler | helpers+GC+runtime+lib | **recording** | **back end + encoder** | **tier-1 compile** | other |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| raytrace | 96.8 | 53.0 | 43.9 | +1.1 | +2.7 | +4.6 | -0.3 | +1.7 | -2.8 | +9.4 | +24.0 | +2.3 | +1.3 |
| cd | 260.0 | 134.7 | 125.2 | +3.6 | +14.2 | +2.6 | +4.2 | +2.0 | +1.3 | +15.7 | +73.2 | +4.9 | +3.4 |
| richards | 60.0 | 36.7 | 23.3 | +1.7 | +2.2 | -0.1 | -1.8 | +0.9 | +1.5 | +2.9 | +13.3 | +1.7 | +1.1 |
| towers | 48.3 | 33.6 | 14.7 | +0.3 | -0.5 | -1.1 | +2.3 | +1.0 | +1.0 | +1.9 | +8.3 | +0.4 | +1.0 |
| fmt_s | 6.8 | 2.2 | 4.7 | +0.2 | +0.1 | +0.1 | 0.0 | +0.1 | +1.5 | +0.3 | +1.1 | +0.3 | +1.0 |
| lua_lexer | 838.9 | 815.6 | 23.4 | +1.0 | +2.3 | -22.4 | -6.3 | +3.7 | +13.7 | +5.2 | +21.5 | +2.9 | +1.8 |
| deltablue | 106.8 | 65.7 | 41.1 | +7.7 | +4.9 | 0.0 | +0.1 | +1.0 | +1.7 | +7.3 | +14.2 | +2.9 | +1.3 |
| havlak | 151.6 | 146.5 | 5.1 | +3.6 | -7.6 | -3.9 | +0.2 | 0.0 | -6.3 | +4.7 | +8.6 | +3.3 | +2.3 |
| classic_vector | 325.7 | 296.2 | 29.5 | +2.0 | +2.5 | +3.3 | +0.7 | -1.8 | +1.5 | +2.4 | +15.2 | +1.8 | +1.9 |

Without context attribution (libc and Rust support as their own columns) the formation columns shrink and the libc column takes the rest:
raytrace recording 6.1 / back end + encoder 18.4 / tier-1 compile 1.5 / support + libc 13.1; cd 10.8 / 50.5 / 3.5 / 40.1; richards 1.8 / 9.8 /
1.4 / 8.4. libc's warm-up excess is 7-26 Mcyc against a steady 0-3 Mcyc per repetition: it is the compile's malloc / free (the unnamed libc
samples are `_int_malloc` / `_int_free`).

Cross-checks [M]: (a) the phase timers of a `RUSTYNAIL_JIT_VERBOSE=1` run (raytrace: rec_step 5.38 + pipeline 9.55 + tier1 1.90 + lazystub 0.82 =
17.65 Mtsc) x 2.52 = 44.5 Mcyc, against 35.7 sampled (the timers add their own cost); (b) the sum of attempt durations of section 3.1 (raytrace
38.9 Mcyc, cd 110.5, richards 19.5, towers 12.4) brackets the sampled formation cycles (35.7 / 93.8 / 17.9 / 10.6) from above, as it should
(attempts include the interpreter running the recorded bytecodes).

lua_lexer and havlak have tiny or negative warm-up: their repetitions run 3-10 % differently in trace code (formation lottery). Read their
formation columns as "formation costs 30 / 20 Mcyc of rep 1", not as warm-up.

### 2.3 LuaJIT [M]

| program | LJ Mcyc | in trace mcode (perf) | libluajit (perf) | `-jp` Compiled / Interpreted / JIT Compiler / GC | formation attempts (Mcyc, section 3) | 50 % / 90 % of installs at (Mcyc) | ours: 50 % / 90 % at |
|---|---:|---:|---:|---|---:|---|---|
| raytrace | 42.6 | 9.2 | 37.8 | 34 / 36 / 8 / 23 % | 5.6 | 7.9 / 13.8 | 31.8 / 51.7 |
| cd | 114.9 | 36.3 | 105.1 | 34 / 13 / 29 / 22 % | 38.7 | 36.4 / 108.9 | 62.2 / 161.5 |
| richards | 36.1 | 30.2 | 7.4 | 90 / 1 / 9 / 1 % | 6.3 | 4.3 / 9.6 | 18.7 / 31.7 |
| towers | 26.4 | 18.5 | 7.4 | 91 / 1 / 7 / 0 % | 5.8 | 4.4 / 8.7 | 10.4 / 31.6 |
| fmt_s | 3.6 | 1.9 | 1.0 | (run too short) | 0.2 | 0.1 / 0.3 | 2.9 / 3.8 |
| lua_lexer | 469.1 | 122.7 | 284.5 | 65 / 4 / 0 / 11 %, C 19 % | 5.3 | 5.3 / 99 | 26.0 / 45.1 |
| deltablue | 116.2 | 17.2 | 112.8 | 23 / 2 / 70 / 4 % | 71.6 | 70 / 89 | 47.1 / 103.5 |
| havlak | 137.2 | 26.0 | 119.3 | 56 / 1 / 19 / 22 % | 26.4 | 33.5 / 96.5 | 53.1 / 70.1 |
| classic_vector | 250.6 | 49.3 | 213.3 | 63 / 0 / 0 / 36 % | 5.2 | 5.0 / 252 | 29.7 / 46.1 |

(The perf columns are of runs under perf with the repetition wrapper, 3-15 % above the grid totals. "libluajit" holds the interpreter, the
compiler, GC and the helpers that traces call; `-jp`'s "Compiled" includes those helpers, which is why it exceeds the mcode column.)
richards in LuaJIT: 58 % of the 5-10 Mcyc window and 92 % of 10-15 Mcyc is trace machine code; ours reaches 9-13 % trace code (+ 11 % tier 1)
at 15-30 Mcyc. LuaJIT also interprets a lot where it does not trace (raytrace 36 %): it does not need full coverage early, because its
interpreter is cheap and its formation costs 5.6 Mcyc in total.

**Where our extra warm-up cycles go** [D, from 2.2 and 2.3]: our formation minus LuaJIT's formation attempts = raytrace 30.5, cd 63.5,
richards 11.7, towers 5.5, fmt_s 1.5, lua_lexer 25, classic_vector 21 Mcyc. Against the whole cycle gap (grid: 53.9 / 129 / 22.5 / 19.3 / 3.1 /
366 / 77) that is **57 / 49 / 52 / 28 / 48 / 7 / 27 %**. The remaining warm-up (execution excess, 2-27 Mcyc) is 10-21 % of the gap on raytrace /
cd and under 10 % elsewhere.

## 3. Deliverable 2: the cost of a formation step

### 3.1 Events, outcomes and cost per attempt [M] (per run; kcyc = median per attempt)

| program | ours: root ok / fail | side ok / fail | median kcyc root ok / side ok / failed side | sum of attempts (Mcyc) | LuaJIT: root ok / fail | side ok / fail | median kcyc root ok / side ok | sum (Mcyc) |
|---|---|---|---|---:|---|---|---|---:|
| raytrace | 6 / 105 | 35 / 11 | 1547 / 560 / 228 | 38.9 | 9.4 / 40 | 33 / 11 | 79 / 102 | 5.6 |
| cd | 27 / 207 | 211 / 92 | 339 / 216 / 37 | 110.5 | 29 / 51 | 543 / 72 | 39 / 34 | 38.7 |
| richards | 2 / 44 | 31 / 7 | 1086 / 407 / 41 | 19.5 | 9 / 0 | 76 / 2 | 34 / 56 | 6.3 |
| towers | 6 / 48 | 21 / 14 | 434 / 321 / 23 | 12.4 | 12 / 1 | 48 / 13 | 66 / 37 | 5.8 |
| fmt_s | 2 / 0 | 0 / 3 | 1049 / - / 24 | 1.4 | 2 / 0 | 1 / 0 | 93 / 18 | 0.2 |
| lua_lexer | 8 / 82 | 58 / 12 | 327 / 395 / 35 | 30.6 | 7 / 3 | 81 / 0 | 48 / 38 | 5.3 |
| deltablue | 14 / 462 | 33 / 43 | 743 / 256 / 23 | 35.3 | 35 / 15 | 122 / 49 | 41 / 74 | 71.6 |
| havlak | 15 / 202 | 13 / 22 | 538 / 195 / 22 | 21.7 | 24 / 33 | 75 / 33 | 69 / 56 | 26.4 |
| classic_vector | 3 / 27 | 14 / 3 | 3589 / 957 / 50 | 27.5 | 9 / 9 | 15 / 2 | 322 / 122 | 5.2 |

Our root "failures" are mostly cheap probes: `StartRefused` (81 / 160 / 24 / 33 / 66 / 354 / 127 per run, 2-8 kcyc each) and `FnShort`.
The costly failures are elsewhere [M, Mcyc per run]: raytrace `TooLong` 4.5 + `Compile(pass)` 2.3 + `InnerLoop` 1.3 + `FnShort` 1.6; cd `FnShort`
3.4, `Depth` 1.7, `SideShort` 1.2; deltablue `FnShort` 8.8, `InnerLoop` 3.4; havlak `FnShort` 5.0, `LeaveLoop` 2.0. All failures together:
raytrace 10.3, cd 9.6, richards 2.1, towers 1.6, deltablue 16.0, havlak 10.4 Mcyc. LuaJIT's are 1.4 / 7.3 / 0.2 / 2.8 / 55 (deltablue's
loop-unroll aborts, its known lottery) / 19.5.

**Which events dominate the warm-up** [M]: successful side traces (raytrace 19.7 of 38.9 Mcyc, cd 72.2 of 110.5, richards 15.9 of 19.5,
towers 8.8 of 12.4, lua_lexer 23.1 of 30.6, classic_vector 17.2 of 27.5), then successful roots (8.8 / 28.7 / 1.6 / 2.0 / 4.6 / 7.5), then
failures. On deltablue and havlak the failed roots are the largest single item (14.8, 9.8 Mcyc).

What one step consists of [M, phase samples of 2.2 divided over the events]: back end + encoder 65-75 %, recording 15-26 %, tier-1 compile
5-8 %. Inside the back end no function exceeds 2 % of the run (raytrace: `ra::allocate_hinted` 1.8, `x86::emit_ex` 1.6, `exit::build_exits_ex`
1.4, `Ra::run` 1.3, `shape_hash_ir` 1.1, `sink::rebuild` 1.0 Mcyc): the cost is the number of passes over the IR and its snapshots, plus malloc /
free (7-26 Mcyc of libc in the warm-up) and the cold start (the first compile costs ~1.6x a repeated compile of the same recording).

### 3.2 (i) Pre-formed traces: the run-it-twice split

The warm-up of section 2.2 is this measurement: rep 2 and 3 run on pre-formed traces in the same process. Split by events: section 3.1. Split by
phase: formation 72-81 % (raytrace, cd, richards, towers), 59 % deltablue, 66 % classic_vector, 36 % fmt_s (whose warm-up is mostly startup,
library and first-GC work: 2.5 of 4.7 Mcyc).

### 3.3 (ii) Earlier / later formation: response curves [M] (cycles % against the default; 9 runs median)

| program | rn Mcyc | hot 128 | hot 512 | exit 8 | exit 32 | exit 64 | sidemin 0 | sidemin 12 | heatside 0 | heatside 48 | wgate 0 | wgate 100k | tier-1 early (50,250) | tier-1 late (800,4000) | blfirst 31 | maxside 4 | maxside 64 |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| raytrace | 96.5 | +3 | +5 | -2 | +3 | +1 | -0 | +1 | +0 | +0 | +2 | +3 | +7 | +8 | -2 | -4 | +4 |
| cd | 243.9 | -6 | -1 | -6 | -2 | -2 | -3 | +15 | -0 | +1 | -1 | -1 | -3 | +0 | -3 | -5 | +14 |
| richards | 58.6 | +1 | -0 | +1 | -0 | +35 | -0 | +4 | -0 | +3 | +21 | +0 | +0 | +3 | +36 | -1 | +4 |
| towers | 45.7 | +5 | +13 | +10 | +20 | +6 | +0 | -0 | +0 | -1 | +7 | +3 | +5 | +4 | -8 | +7 | -1 |
| fmt_s | 6.7 | +0 | +0 | +0 | -2 | +1 | -5 | +4 | -1 | -1 | -9 | +21 | -8 | +30 | -5 | -2 | -1 |
| lua_lexer | 835.1 | +0 | +2 | +1 | +3 | +4 | +1 | +23 | +9 | -3 | +1 | +2 | +26 | +16 | +6 | +11 | -1 |
| deltablue | 106.1 | -0 | +2 | -1 | +2 | +0 | -1 | +0 | -1 | +1 | +0 | -0 | -1 | +7 | -1 | -2 | +1 |
| havlak | 171.9 | -0 | -6 | -1 | +3 | -0 | +1 | -0 | +1 | -0 | -1 | -1 | +0 | +1 | +1 | +2 | +1 |
| classic_vector | 327.4 | -1 | +0 | -2 | -3 | -1 | -1 | -1 | -0 | -1 | -0 | +1 | -7 | +4 | -1 | +1 | -1 |

LuaJIT, same programs (`-Ohotloop` / `-Ohotexit`, default 56 / 10): raytrace hotloop 112 / 256 **+29 / +30 %**, everything else -8..+18 %
(deltablue -21..-29 %: lottery). [M]

Reading: with a step costing 5-10x LuaJIT's, earlier formation buys nothing on most programs (more steps cost as much as they save), while
LuaJIT loses 30 % on raytrace when it forms later. The big single cells are formation landings, not trends (richards `exit 64` +35 % and
`blfirst 31` +36 %, towers `exit 32` +20 %, lua_lexer `sidemin 12` +23 %, tier-1 early +26 %). The best knob per program is -2..-9 % and differs
by program; no setting is a global gain. **The lever is the step cost; the policy follows from it** (a step 5x cheaper would make LuaJIT's
thresholds affordable).

### 3.4 (iii) Failed recordings deleted a priori [M] (scratch blacklist of every key that never installed in five pre-pass runs)

Keys never installed / their share of failed attempts: raytrace 26 (540 of 580 attempts in 5 runs), cd 54 (1150 / 1495), richards 14 (245 / 255),
towers 11 (235 / 310), fmt_s 1, lua_lexer 24 (455 / 470), deltablue 112 (2445 / 2525), havlak 54 (960 / 1120), classic_vector 12 (145 / 150).

| program | h0 Mcyc | blacklist | instr | rep 1 / steady / warm-up, h0 | the same, blacklist | failed-attempt Mcyc / run: before -> after |
|---|---:|---:|---:|---|---|---|
| raytrace | 101.1 | **-2 %** | +0 % | 98.7 / 51.4 / 47.3 | 93.3 / 54.2 / 39.2 | 10.3 -> 2.7 |
| cd | 254.7 | **-4 %** | -4 % | 263.2 / 134.3 / 128.8 | 246.9 / 111.5 / 135.5 | 9.6 -> 4.1 |
| richards | 62.2 | -1 % | -1 % | 61.5 / 37.7 / 23.7 | 60.1 / 36.4 / 23.7 | 2.1 -> 0.9 |
| towers | 49.4 | **-8 %** | +5 % | 50.9 / 33.4 / 17.5 | 48.2 / 34.1 / 14.1 | 1.6 -> 0.7 |
| fmt_s | 9.3 (wrapper) | -8 % | -11 % | | | 0.1 -> 0 |
| lua_lexer | 847.0 | **+22 %** | +23 % | 832.9 / 808.1 / 24.8 | 1019 / 941 / 78 | 2.9 -> 3.4 (115 side traces instead of 58) |
| deltablue | 110.2 | **-10 %** | -6 % | 108.9 / 66.4 / 42.4 | 98.2 / 60.5 / 37.8 | 16.0 -> 3.4 |
| havlak | 168.2 | -4 % | -4 % | | | 10.4 -> 2.1 |
| classic_vector | 314.2 | -0 % | -0 % | 319.7 / 286.9 / 32.7 | 316.2 / 292.0 / 24.2 | 2.8 -> 0.8 |

(`h0` = the scratch binary through the same wrapper script, so its totals sit 0.5-2.5 Mcyc above the grid's.) The ceiling is 2-10 % where the
deleted attempts were pure waste, and negative where a failing head's attempts shaped a later formation (lua_lexer). Removing 7.6 Mcyc of failed
attempts on raytrace saved 2.1 Mcyc whole-run (8 Mcyc of warm-up in the repetition series): deleted recordings partly come back as other
work.

### 3.5 (iv) Tier-1 code size [M]

Tier-1 code per program: cd 489 KB (40 prototypes), deltablue 417, havlak 424, lua_lexer 242, raytrace 237, classic_vector 232, richards 205, towers
40, fmt_s 28 (`[jit]` statistics). Padding every bytecode's template by 250 bytes (about +64 % code at ~390 bytes per bytecode) against 1 byte,
same instructions executed: raytrace +1 %, cd -1 %, richards +1 %, towers +1 %, lua_lexer +2 %, deltablue -1 %, havlak 0 %, classic_vector +1 %
(cycles; instructions +0 %). In the repetition series the padding moves the warm-up by -1..+2 Mcyc. **A tier-1 code diet that removed 40 % of the
bytes would save about 1-2 %: below the bar.** The 2-3x instruction-cache miss rate of STUDY_CAUSAL_GAP 3.4 is the compile path's own code (cold
on first use, see 3.1), not tier-1 code density.

### 3.6 (v) Our interpreter and tier 1 against LuaJIT's interpreter [M] (whole runs, Mcyc)

| program | ours (JIT) | LuaJIT | ours interp | ours tier 1 only | LuaJIT `-joff` | interp / LJ interp | tier 1 / LJ interp |
|---|---:|---:|---:|---:|---:|---:|---:|
| raytrace | 96.5 | 42.6 | 221.9 | 214.0 | 168.1 | 1.32 | 1.27 |
| cd | 243.9 | 114.9 | 367.8 | 308.1 | 311.8 | 1.18 | 0.99 |
| richards | 58.6 | 36.1 | 208.4 | 167.4 | 149.6 | 1.39 | 1.12 |
| towers | 45.7 | 26.4 | 252.7 | 182.5 | 152.0 | 1.66 | 1.20 |
| fmt_s | 6.7 | 3.6 | 119.1 | 112.7 | 93.4 | 1.28 | 1.21 |
| lua_lexer | 835.1 | 469.1 | 3117.5 | 2094.3 | 2225.5 | 1.40 | 0.94 |
| deltablue | 106.1 | 116.2 | 150.1 | 129.2 | 128.5 | 1.17 | 1.01 |
| havlak | 171.9 | 137.2 | 183.7 | 176.4 | 189.4 | 0.97 | 0.93 |
| classic_vector | 327.4 | 250.6 | 2451.2 | 2470.3 | 1749.0 | 1.40 | 1.41 |

Interpreter + tier-1 excess in the warm-up (2.2): raytrace 3.8, cd 17.8, richards 3.9, towers -0.2, deltablue 12.6, classic_vector 4.5 Mcyc. At
LuaJIT's interpreter speed these would cost (1 - 1 / ratio) less [D]: raytrace 0.8, cd 0.6-2.7, richards 0.5-1.1, deltablue 0.1-1.8,
classic_vector 1.3 Mcyc, **at most 2 % of any program**. A slower interpreter is not why warm-up is long. (Tier 1 runs 17-44 Mcyc per steady
repetition on raytrace / cd / deltablue / havlak / lua_lexer: that is a steady-state coverage item, outside this study.)

### 3.7 Other mechanisms [M]

* Exits during formation (stubs + handler, warm-up excess): raytrace 1.7, cd 2.0, richards 0.9, towers 1.0 Mcyc (1-2 %). A cheaper early-exit
  path cannot buy more than that. In steady state cd spends 31 Mcyc per repetition (23 %) in stubs and the handler: steady-state item.
* Allocator: preloading tcmalloc / jemalloc changes cycles by raytrace +6 / +4 %, cd -3 / -4 %, richards +15 / +10 %, towers +17 / +14 %, others
  -0..+7 %. glibc malloc is not the cost; the number of allocations per step is (section 4).
* Startup: 1.17 vs 1.00 Mcyc: not a factor.

## 4. Deliverable 3: ceilings and builders

Ceiling = cycles that disappear if the mechanism were free (measured as stated); expected = ceiling x the fraction a concrete design removes x
realisation 0.5 (structural changes; same-binary knob ceilings realised 0.95 in the backtest of STUDY_CAUSAL_GAP section 4, structural ones
less). Check of the method on a built handle: C1 (docs/JIT_TRACE.md 49) made the compile 7-11 % cheaper and realised -1.8 / -4.2 / -2.3 / -2.4 %
cycles on raytrace / cd / richards / towers; 9 % of the formation share below (37 / 39 / 30 / 23 %) predicts -3.3 / -3.5 / -2.7 / -2.1 %. Agreement
within 1.5x.

| # | mechanism | measured ceiling (Mcyc, % of the program) | what a builder must change | expected | verdict |
|---|---|---|---|---|---|
| 1 | **Formation step cost** (recording + back end + tier-1 compile, first run) | raytrace 36.1 (37 %), cd 102.2 (39 %), richards 18.0 (30 %), towers 11.3 (23 %), deltablue 31.1 (29 %), classic_vector 26.1 (8 %), lua_lexer 30.3 (3.6 %), havlak 20.3 (13 %), fmt_s 1.7 (25 %) | make a step 2x cheaper (LuaJIT's is 5-10x cheaper) | 2x cheaper x 0.5: raytrace **-9 %**, cd **-10 %**, richards **-8 %**, towers **-6 %**, deltablue -7 %, fmt_s -6 %, havlak -3 %, classic_vector -2 % | **build** |
| 2 | Failed recordings | blacklist ceiling: -2 / -4 / -1 / -8 / -10 / -4 % (raytrace / cd / richards / towers / deltablue / havlak), +22 % lua_lexer | predict failing heads cheaply (FnShort / TooLong / InnerLoop shapes) before recording, without blacklisting heads that later matter | x 0.5: -1 to -5 % on those programs | build after 1, small |
| 3 | Thresholds / formation policy | best knob per program -2..-9 %, signs differ | per-shape policy | <= 2 % net | drop until 1 is built (then re-measure: cheaper steps make earlier formation pay) |
| 4 | Tier-1 code size | +64 % size = +1-2 % | template diet | <= 1 % | **drop** |
| 5 | Interpreter / tier-1 speed during warm-up | <= 2.7 Mcyc (<= 2 %) | faster dispatch | <= 1 % warm-up | **drop for warm-up** (keep for steady interpreter-shaped programs) |
| 6 | Exits through the handler during formation | 1-2 Mcyc (1-2 %) | cheaper exit / re-entry while a side trace is pending | <= 1 % | **drop** (cd's steady exits are a separate item) |
| 7 | Allocator | swap: -4..+17 % | - | 0 | **drop** (allocation count is part of 1) |

**Builder 1, concrete designs** (in order of measured weight; all inside a single run, no cross-run caching):

* **(1a) A side-trace compile path.** Side traces are 70-94 % of the successful steps on all programs but havlak (46 %) and 60-87 % of the attempt time on raytrace / cd / richards / towers / lua_lexer / classic_vector (3.1). A side trace has no
  loop pass, usually nothing to sink (except a `resink` replay), and starts from its parent's snapshot. Build it with: one pass that folds,
  CSEs and allocates while emitting (no separate `opt_loop`, `sink` only when the parent exit has sunk tables, no shape hash for traces that are
  never compared), snapshots stored as deltas of the parent exit's snapshot (the snapshot volume drives the recorder, allocator, exit builder and
  shape hash, 49.1), and exit stubs built lazily for every exit (S1 already does it for some). Measure first with a scratch build that times the
  side-trace pipeline per pass (phase timers x 2.52) to choose the passes; target 2x on side traces = (side share of the attempt time x formation x 1/2 x 0.5) cd -7 %, raytrace -6 %,
  richards -7 %, towers -4 %.
* **(1b) Compile-data allocation.** malloc / free of the compile are 7-26 Mcyc of the warm-up (20-30 % of the formation cycles, 2.2). A per-compile
  bump arena reused across compiles (IR, snapshot maps, allocation vectors, hash tables reset not freed) removes most of it; the allocator swap
  shows it is the count, not glibc. Ceiling raytrace 7.5, cd 25.7, richards 4.8, towers 3.0 Mcyc; expected (0.5) -4 / -5 / -4 / -3 %.
* **(1c) Cold start of the compile path.** The first compile of a run costs ~1.6x a repeated one (16.1 Mcyc doubled vs 26.3 sampled on raytrace;
  48 vs 78 on cd). Shrinking the code the compile touches (fewer generic paths, less monomorphisation of the passes) helps every program's first
  traces; no ceiling of its own beyond 1; measure with a doubled-first-compile scratch run before building.
* **(1d) Recording.** 15-26 % of a step (raytrace 9.4, cd 15.7 Mcyc). T2b already took the cheap part; the remaining cost is per bytecode
  (`record::step`, `snap::take`, `SlotMap::entries_into`): snapshot-per-guard volume again, shared with (1a).

**Candidates with ceilings below ~3 % (drop):** tier-1 code diet, interpreter dispatch for warm-up, threshold / side-count tuning as a global
change, formation-exit fast path, allocator swap, a-priori blacklisting on its own for lua_lexer-like programs.

## 5. Deliverable 4: the previously queued handles, re-ranked

| handle | measured ceiling | verdict |
|---|---|---|
| Code-changing compile levers (C1 follow-ups, single-pass RA + emit, lazy tier 1 for traced prototypes) | they are builder 1: formation is 23-39 % of raytrace / cd / richards / towers / deltablue (section 4), not "a third of warm-up" | **keep, first** (as 1a-1d, side traces first) |
| Frame model (return / room checks, CallInfo protocol) | checks: fib 4.4 %, permute 1.7 % (STUDY_CAUSAL_GAP 5); the rest of fib's gap is the protocol, not ablatable | drop as a builder; architectural only |
| Function-entry roots (T7 / `fnroot`) | 0 to -1 % on text_adventure, 0..+8 % elsewhere (STUDY_CAUSAL_GAP 2.2); LuaJIT relies on them only for recursion, which we have | **drop** |
| NaN-boxing / one-word values | all tag guards 10-14 % on permute / towers / richards / fib, 1-6 % elsewhere; only part removable (LuaJIT checks tags too) | **drop** (3-4 % at best on recursion programs) |
| Sunk-allocation replay | built (52.3); classic_vector -34 % | done |

## 6. Correction to docs/STUDY_CAUSAL_GAP.md section 3.2

Its "remainder" (raytrace 24.3, cd 50.4, richards 10.9, towers 4.5, fmt_s 3.8, deltablue 16.6 Mcyc) is mostly compile: the phase timers were
converted with 1.4 cycles per TSC tick instead of the measured 2.52, and the doubled compile (warm caches) costs ~0.6 of the first. The measured
execution excess is raytrace 9.8, cd 26.6, richards 2.9, towers 2.0, fmt_s 0.5 (+2.5 startup / library / first GC), deltablue 13.7 Mcyc. Its
conclusion that thresholds are not the lever stands (3.3 here confirms it); its conclusion that a compile-speed diet should not be run is reversed:
the compile, including recording, allocation and cold start, is the warm-up.

## 7. Caveats

* Leaf-symbol sampling: helper code shared by phases (libc, `hashbrown`, `alloc`) is attributed by context; section 2.2 gives both readings. The
  formation share without context attribution is 52-59 % of the warm-up on raytrace / cd / richards (with the libc column, which is compile-side
  by its warm-up / steady profile, it is 72-81 %).
* LuaJIT is stripped: its compiler share comes from `-jp` (1 ms samples, 30 runs) and the attempt durations; its repetition series is not clean
  (STUDY_CAUSAL_GAP 3.1), so LuaJIT has no run-it-twice split here.
* LuaJIT's attempt durations include a `jit.attach` callback per event (one table store per field).
* The blacklist keys use the parent's snapshot count; when the blacklist changes formation, later side keys can differ (remaining failures are
  listed in 3.4). Its result is a ceiling for "these attempts never happen", not a design.
* havlak and lua_lexer warm-up values are within their repetition noise (formation lotteries of 3-10 %).
* The scratch builds are not committed; their switches are described in section 1.

## 8. Reproduction

```
scripts/formation-phases.py record OUT.json 4 REPS N -- <rnlua|luajit> scripts/causal-reps.lua bench/<p>.lua   # FP_PERIOD=100003
scripts/formation-phases.py warm ONE.json THREE.json [--ctx]  |  timeline OUT.json W [--ctx]  |  lj OUT.json [W]
scripts/formation-events.py ours LOG... --ratio 2.52  |  lj ERR... --ghz G  |  blin LOG OUT
luajit scripts/formation-lj.lua bench/<p>.lua 2>events.txt
scripts/causal-grid.py SPEC.json OUT.json ; scripts/causal-table.py grid OUT.json [base] [event]
```
All under `systemd-run --user --scope -p MemoryMax=8G -p MemorySwapMax=0 timeout ...`, stdin from /dev/null. Data (JSON, logs) stayed in the session
scratchpad; the tables above are complete copies.
