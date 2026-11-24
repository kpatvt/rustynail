# Study: causal attribution of the LuaJIT gap (2026-11-24, base c0bf0ce, LuaJIT 2.1.1795119671)

Question: the previous studies sorted sampled instructions into buckets and then guessed a mechanism for each bucket. Handle after
handle realised 0.1-0.5 of its estimate. This study measures causes directly. It switches a mechanism off (ablation), runs it twice
(doubling) or deletes it in an unsound scratch build (ceiling), on BOTH engines, and counts **cycles**. Nothing below is a bucket share
turned into a gain.

Tags: **[M]** measured here (scripts and data below), **[D]** derived from [M] numbers by the arithmetic stated. Backtest data comes
from docs/JIT_TRACE.md and the docs/STUDY_*.md files, cited by section.

## 0. Answer in one page

1. **Every program except text_adventure is 0.9-1.3x LuaJIT in steady state.** The large ratios (1.8-2.3x) are mostly
   warm-up. That warm-up is mostly NOT the trace back end. [M, section 3]

   | | raytrace | cd | richards | fmt_s | towers | deltablue |
   |---|---:|---:|---:|---:|---:|---:|
   | ours, first run (Mcyc) | 100.5 | 250.8 | 58.2 | 6.7 | 47.6 | 105.8 |
   | ours, steady run | 51.8 | 132.2 | 34.5 | 2.0 | 36.7 | 67.1 |
   | LuaJIT, whole program | 41.7 | 113.0 | 35.6 | 3.5 | 24.9 | 115.1 |
   | warm-up share of the cycle gap | ~85 % | ~80 % | ~95 % | ~95 % | ~40 % | (we win) |

   Doubling the trace back end plus the tier-1 compile costs only 16 / 48 / 8 / 0.7 / 4.4 / 13 Mcyc, about a third of each warm-up.
   Recording adds 7 / 14 / 4 / 0.3 / 1.5 / 8. **The largest part of warm-up (24 / 50 / 11 / 3.8 / 4.5 / 17 Mcyc) is the program
   running slowly in tier 1 and the interpreter before its traces exist, plus the exits during formation.** No previous study
   named this as a cause. Lower hot thresholds do not buy it back: earlier side traces make us 2-22 % SLOWER, while LuaJIT barely
   notices our thresholds (3.3). Formation latency and per-formation cost are one problem.
2. **The steady-state gap of the object-allocating programs is allocation sinking, and specifically re-sinking in side traces.**
   LuaJIT ablation [M]: `-O-sink` costs LuaJIT classic_vector +190 %, raytrace +63 %, cd +42 % and havlak +17 %. Our sinking off
   costs us +16 %, +14 %, 0 % and -2 %. A scratch LuaJIT with sinking switched off **only in side traces** loses +55 % / +38 % / +16 %
   / +11 %. Without sinking, LuaJIT runs classic_vector at 718 Mcyc, which is slower than our 480. Doubling our materialisation
   costs +107 Mcyc on classic_vector and +37 Mcyc on cd. The cheapest causal story that fits these numbers: classic_vector's steady
   gap (~210 Mcyc) and most of cd's and raytrace's steady gap are allocations that LuaJIT keeps sunk across side-trace boundaries
   (`snap_replay` plus sinking in the side trace) and we materialise. This is (B) of docs/JIT_TRACE.md 50.6, which was "not built".
3. **text_adventure is not a trace-formation problem.** LuaJIT's interpreter alone (`-joff`, 2224 Mcyc) beats our complete JIT
   (2630 Mcyc). Our traces are worth only 9 % there; LuaJIT's are worth 41 %. LuaJIT's function-entry roots are worth 5 % of its time
   there, so the T7 / `fnroot` direction cannot close it (measured on our side: 0 to -1 %). C-call stitching is worth 40 % to LuaJIT.
   **The rest is the speed of our runtime and library code outside traces:** our interpreter 2994 vs LuaJIT's 2224, our tier 1 2866.
4. **Frame model and value layout are real but small ceilings** [M, ceiling and doubling builds]. Deleting the native-return and
   room checks saves fib 4.4 % and permute 1.7 %. Doubling every tag guard costs 10-14 % on the recursion and method programs
   (permute, towers, richards, fib), 4-6 % on classic_vector / lua_lexer, and 1-4 % elsewhere. That puts an upper bound on what
   NaN-boxing could save in guards: LuaJIT also checks types, so removing guards entirely is the bound, not the saving.
5. **Why the old method failed** (section 4): bucket share x assumed removal fraction realised a median of **0.12**. Same-binary
   knob ceilings realised **~0.95**. The ablation, doubling and ceiling methods used here are all of the second kind. The two
   ceilings of this study that overlap a built handle (`slink` link cost, the compile back end) retrodict the realised gains within
   a factor of ~1.5.
6. **Recommendation** (section 6): (1) build sunk-allocation replay into side traces: largest measured ceiling, and LuaJIT depends on
   it. (2) Warm-up is the largest item on raytrace / cd / richards / fmt_s, but it is not thresholds. Before any builder, run a study
   of the cost of one formation step: exits through the handler and tier 1 while a side trace is pending, failed side recordings, and
   tier-1 code size. Do not run another compile-speed diet. (3) Runtime / library speed for interpreter-shaped programs. Do not build
   another allocator, link, guard or frame diet: the measured ceilings are 2-15 % of the program before realisation.

## 1. Method

* Binaries: `cargo build --release -p rn-cli` at c0bf0ce (plain, not PGO), copied out of the target dir. LuaJIT: `/usr/bin/luajit`
  (2.1.1795119671). Scratch LuaJIT: the same upstream commit (96f7b29, built from source, never committed) with two environment
  switches, `LJHACK_NOFUNCROOT` (no root traces at function entries) and `LJHACK_NOSIDESINK` (`lj_opt_sink` returns at once when
  `J->parent != 0`). It is always compared with itself without the switch.
* Scratch builds of ours (never committed, separate target dir, removed): one binary with `RNHACK` bits. The same binary with
  `RNHACK=0` is the baseline, so code layout is identical.
  * 1 = the trace back end runs twice on the same recording (`pipeline_front` on a copy, `ra::allocate_hinted`,
    `exit::build_exits_ex`, `asm::assemble_dl`, `x86::emit_ex`; results discarded through `black_box`).
  * 2 = tier-1 lowering + encoding twice.
  * 4 = every sunk table is also built as a garbage duplicate in `exit::materialise` (allocate, size, insert fields).
  * 8 / 16 = tag guards / every memory-compare guard deleted (unsound).
  * 32 = native-return guards, room checks and RETF guards deleted (unsound).
  * 64 = the same but keeping the return-site compare.
  * 128 / 256 = every tag guard / memory-compare guard emitted twice.
  * 512 = native-return guards and room checks emitted twice.
* Measurement (scripts/causal-grid.py): `taskset -c 4`, a 300 ms spin of CPU 4 before every run, `RUSTYNAIL_SEED=0`, stdin from
  /dev/null, the environment padded to equal length, configurations rotated per run, `perf stat -e` with at most 6 events per run (no
  multiplexing: every count at 100 %). 9 runs (permute 15), **median** unless stated. Before every series I checked that no other perf,
  benchmark or build job was running; nothing else ran during the measurements. Output was checked against `bench/<p>.expected` (or the
  first configuration): `(diff)` / `(err)` in a table means that configuration broke the program, and its number is not used.
* Warm-up by construction (scripts/causal-reps.lua): the benchmark chunk is loaded ONCE and called N = 1, 2, 3 times in one process
  (same prototypes, so traces survive). cost(rep k) = cycles(N=k) - cycles(N=k-1). warm-up = rep 1 - rep 3.
* Phase timers: `RUSTYNAIL_JIT_VERBOSE=1` (`[trace] phases`, Mtsc). Converted to cycles with the ratio measured against the
  doubling build (back end + tier-1 Mtsc vs `RNHACK=3` cycles: 1.2-1.5, so 1.4 is used) [D].
* Data: all JSON is in the scratchpad of this session (not kept in the repo). The tables below are complete copies; they are regenerated
  with scripts/causal-table.py.

Totals at this base [M] (median Mcyc / Minstr; LuaJIT's minimum in brackets, it is a lottery on two programs):

| program | ours Mcyc | LJ Mcyc (min) | cyc ratio | ours Minstr | LJ Minstr | ins ratio |
|---|---:|---:|---:|---:|---:|---:|
| permute | 35.9 | 271.8 (14.3) | 0.13 | 191 | 1156 | 0.17 |
| raytrace | 94.2 | 41.7 | 2.26 | 294 | 166 | 1.77 |
| cd | 246.0 | 113.0 | 2.18 | 690 | 318 | 2.17 |
| towers | 45.4 | 24.9 | 1.83 | 169 | 105 | 1.61 |
| classic_vector | 480.2 | 247.7 | 1.94 | 2022 | 1143 | 1.77 |
| lua_lexer | 829.5 | 458.4 | 1.81 | 3291 | 1908 | 1.72 |
| richards | 58.8 | 35.6 | 1.65 | 166 | 121 | 1.37 |
| fib | 645.8 | 479.2 | 1.35 | 3864 | 2576 | 1.50 |
| byte_multi | 38.5 | 20.3 | 1.90 | 268 | 85 | 3.16 |
| fmt_s | 6.7 | 3.5 | 1.93 | 23 | 10 | 2.27 |
| text_adventure | 2629.7 | 1308.0 | 2.01 | 8075 | 3979 | 2.03 |
| deltablue | 105.7 | 115.1 (79.2) | 0.92 | 308 | 303 | 1.01 |
| havlak | 169.0 (151 in the hack series) | 121.3 | 1.39 | 503 | 321 | 1.57 |

## 2. Deliverable 1: ablation grids

### 2.1 LuaJIT [M] (cycles, % against `lj`; 9 runs median, permute 15)

| program | lj Mcyc | joff | O0 | nofold | nocse | nodce | nonarrow | noloop | nofwd | nodse | noabc | nosink | nofuse | nostitch | noside | recunroll0 | callunroll0 |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| permute | 271.8 | -24 | -59 | -50 | +2 | -85 | -0 | +3 | -53 | +8 | +73 | +2 | +100 | +4 | -87 | -89 | -90 |
| raytrace | 41.7 | +303 | +94 | +94 | +86 | +69 | -3 | +2 | +85 | +67 | -1 | +63 | +1 | -1 | +45 | -1 | +17 |
| cd | 113.0 | +171 | +55 | +56 | +56 | +46 | -0 | -3 | +54 | +44 | +0 | +42 | +9 | -1 | +77 | +2 | +17 |
| towers | 24.9 | +515 | +42 | +35 | +43 | +5 | +9 | -1 | +40 | +8 | -3 | -3 | +2 | -2 | +191 | +3 | -10 |
| classic_vector | 247.7 | +593 | +222 | +224 | +211 | +189 | -0 | +2 | +213 | +191 | -0 | +190 | -0 | -0 | +66 | -1 | +0 |
| lua_lexer | 458.4 | +376 | +23 | +20 | +17 | +2 | +0 | +3 | +16 | -0 | +0 | +1 | -0 | +10 | +125 | -1 | +0 |
| richards | 35.6 | +318 | +37 | +44 | +39 | +4 | +1 | -1 | +40 | +0 | -0 | +0 | +5 | +1 | +138 | +1 | +1 |
| fib | 479.2 | +491 | +1 | -4 | -0 | -1 | -0 | -0 | -1 | -1 | -0 | -1 | -1 | -0 | +208 | +41 | +2 |
| byte_multi | 20.3 | +1140 | +197 | +1 | +1 | +24 | +29 | +238 | +1 | -0 | -1 | -1 | +1 | -0 | +0 | -0 | -0 |
| fmt_s | 3.5 | +2547 | +1782 | +1523 | +10 | +152 | +40 | +336 | +15 | +3 | -0 | +0 | +9 | -1 | +1 | -0 | -0 |
| text_adventure | 1308.0 | +70 | +13 | +10 | +8 | +4 | -6 | +4 | +9 | +2 | +4 | +3 | +2 | +40 | +35 | -5 | -1 |
| deltablue | 115.1 | +9 | -29 | -32 | -28 | +11 | -5 | +16 | -28 | +10 | +10 | +9 | +10 | -10 | -53 | +7 | +8 |
| havlak | 121.3 | +34 | +11 | +12 | +12 | +17 | -1 | -3 | +10 | -2 | -2 | +17 | -1 | -0 | +6 | -2 | +1 |

(`nostitch` = `-Ominstitch=1000000`, `noside` = `-Omaxside=0`. In LuaJIT `lj_opt_sink` needs SINK, FWD, DCE, CSE and FOLD, so `nofold`,
`nocse`, `nodce` and `nofwd` also switch sinking off. Their numbers on raytrace / cd / classic_vector / havlak include the sinking loss.
The clean readings are on the programs where `nosink` is ~0: towers, richards, lua_lexer, fib, byte_multi, fmt_s.)

Scratch LuaJIT [M] (same binary with and without the switch):

| program | ljh Mcyc | no function-entry roots | no sinking in side traces | no function roots + no stitching |
|---|---:|---:|---:|---:|
| permute | 280.1 | -92 (lottery) | +6 | -24 |
| raytrace | 41.5 | +24 | **+16** | +23 |
| cd | 112.6 | +24 | **+38** | +33 |
| towers | 25.2 | **+572** | -1 | +578 |
| classic_vector | 250.2 | +1 | **+55** | +1 |
| lua_lexer | 481.4 | +0 | +1 | +11 |
| richards | 34.4 | -1 | -1 | +2 |
| fib | 485.7 | **+502** | -1 | +501 |
| byte_multi / fmt_s | 20.1 / 3.3 | 0 / 0 | 0 / 0 | 0 / 0 |
| text_adventure | 1251.1 | +5 | -1 | **+67** |
| deltablue | 121.7 | -63 (lottery) | -1 | -64 |
| havlak | 120.3 | -6 | **+11** | -7 |

### 2.2 Ours [M] (cycles, % against `rn`)

| program | rn Mcyc | interp | tier1 | nofold | nocse | noloop | nofwd | noabc | nosink | noixfuse | noleaadd | noside | recunroll0 | slink0 | rapol0 | cspeed0 | dlink0 | lazyci0 | t7on | guarddiet0 | midfold0 |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| permute | 35.9 | +598 | +383 | +2 | +16 | +0 | +27 | -0 | +0 | -0 | -0 | +100 | +0 | +0 | +0 | +1 | +2 | +16 | -0 | -0 | +12 |
| raytrace | 94.2 | +133 | +122 | +2 | +8 | -1 | +3 | +0 | +14 | +1 | +1 | +37 | -0 | +2 | +1 | +12 | +0 | +1 | +1 | +2 | -0 |
| cd | 246.0 | +48 | +27 | -1 | +9 | -2 | +2 | -0 | -0 | -1 | -0 | +7 | -1 | -0 | +0 | +15 | +0 | -2 | +0 | -0 | -0 |
| towers | 45.4 | +453 | +304 | -2 | +40 | -1 | +38 | -0 | -1 | +0 | -1 | +114 | +0 | -0 | +1 | +1 | +5 | +2 | +6 | +2 | -1 |
| classic_vector | 480.2 | +415 | +420 | -1 | -1 | -1 | +1 | +1 | +16 | -0 | -0 | +295 | +0 | +1 | +1 | +1 | -0 | +1 | +1 | -0 | -1 |
| lua_lexer | 829.5 | +272 | +152 | -0 | +7 | -4 | +56 | +1 | +0 | +1 | +1 | +100 | +2 | +1 | +1 | +1 | +0 | +0 | +0 | +1 | +1 |
| richards | 58.8 | +254 | +181 | -1 | +53 | -4 | +14 | +0 | +0 | -1 | -0 | +114 | -0 | +0 | +3 | +4 | -0 | +0 | +2 | -0 | -1 |
| fib | 645.8 | +586 | +240 | +0 | +0 | +0 | -0 | -1 | -0 | +0 | -1 | +219 | +58 | -0 | -0 | +0 | +7 | +128 | -1 | +0 | +14 |
| byte_multi | 38.5 | +654 | +596 | +7 | +11 | +70 | +15 | -1 | -1 | +2 | +1 | -1 | -0 | +0 | +5 | +0 | -0 | -0 | -1 | -0 | -0 |
| fmt_s | 6.7 | +1778 | +1577 | +6 | +16 | +65 | +8 | -1 | -2 | -1 | -0 | -11 | -1 | -0 | +10 | +1 | -2 | -1 | +1 | +1 | -1 |
| text_adventure | 2629.7 | +14 | +9 | -0 | +0 | +0 | +0 | -1 | -0 | -0 | +0 | +3 | -0 | +0 | +2 | +0 | +0 | +0 | -0 | +0 | +2 |
| deltablue | 105.7 | +42 | +23 | -1 | +3 | +1 | +1 | -0 | -0 | -0 | -1 | +13 | -0 | -0 | +1 | +5 | +0 | +0 | +1 | +0 | +0 |
| havlak | 169.0 | +9 | +7 | -4 | -4 | -1 | -2 | -3 | -2 | -3 | -6 | -3 | -6 | -3 | -3 | -2 | -2 | -0 | -3 | -0 | -1 |

(`interp` = `RUSTYNAIL_JIT=off`, `tier1` = `RUSTYNAIL_JIT_TRACE=off`, `no*` of the first block = `RUSTYNAIL_JIT_TRACE_OPT=-*`, the rest
`RUSTYNAIL_JIT_TRACE_LIMITS`: `noside` = `maxside=0,heatside=0`, `midfold0` = `midfold=0,idfold=0`, `t7on` = `t7=1`. havlak's `rn`
reading was a slow draw: every other column is -2..-6 %, so read havlak as flat. `cspeed0` costs raytrace +12 % and cd +15 % cycles
here (+14 / +18 % instructions), while section 49 measured the levers at -2.3 / -3.8 % instructions. At this base the old paths (`cspeed=0`)
have become much slower, probably through code added after section 49 that has a slow path under `cspeed=0`. The shipped default is not
affected; noted because `cspeed=0` is no longer a valid "before" for compile-cost measurements. Coverage knobs on their own [M]: text_adventure
`t7=1` -0.4 %, `t7=1,fnpromow=0` -0.6 %, `fnroot=1` +0.8 %, `fnmin=0` -0.9 %; richards / deltablue / raytrace / towers 0 to +8 %.)

### 2.3 The matrix: what each engine's speed depends on (cycle delta when the feature is off, LuaJIT / ours)

| program | sinking | side-trace re-sinking (LJ only) | CSE | forwarding | loop opt | side traces | recursion unroll | function roots (LJ) | stitching (LJ) | JIT off: LJ interp / our tier 1 / our interp (Mcyc) |
|---|---|---|---|---|---|---|---|---|---|---|
| raytrace | **+63 / +14** | **+16** | (sink) / +8 | (sink) / +3 | +2 / -1 | +45 / +37 | 0 / 0 | +24 | -1 | 168 / 210 / 220 |
| cd | **+42 / 0** | **+38** | (sink) / +9 | (sink) / +2 | -3 / -2 | +77 / +7 | 0 / 0 | +24 | -1 | 306 / 312 / 365 |
| classic_vector | **+190 / +16** | **+55** | (sink) / -1 | (sink) / +1 | +2 / -1 | +66 / +295 | 0 / 0 | +1 | 0 | 1715 / 2499 / 2473 |
| havlak | **+17 / -2** | **+11** | (sink) / -4 | (sink) / -2 | -3 / -1 | +6 / -3 | 0 / -6 | -6 | 0 | 163 / 182 / 184 |
| towers | -3 / -1 | -1 | +43 / +40 | +40 / +38 | -1 / -1 | +191 / +114 | +3 / 0 | **+572** | -2 | 153 / 184 / 251 |
| richards | 0 / 0 | -1 | +39 / +53 | +40 / +14 | -1 / -4 | +138 / +114 | +1 / 0 | -1 | +1 | 149 / 165 / 208 |
| lua_lexer | +1 / 0 | +1 | +17 / +7 | +16 / **+56** | +3 / -4 | +125 / +100 | -1 / +2 | 0 | +10 | 2182 / 2092 / 3089 |
| fib | -1 / 0 | -1 | 0 / 0 | -1 / 0 | 0 / 0 | +208 / +219 | +41 / +58 | **+502** | 0 | 2833 / 2198 / 4428 |
| byte_multi | -1 / -1 | 0 | +1 / +11 | +1 / +15 | **+238 / +70** | 0 / -1 | 0 / 0 | 0 | 0 | 252 / 268 / 291 |
| fmt_s | 0 / -2 | 0 | +10 / +16 | +15 / +8 | **+336 / +65** | +1 / -11 | 0 / -1 | 0 | -1 | 92 / 112 / 126 |
| text_adventure | +3 / 0 | -1 | +8 / 0 | +9 / 0 | +4 / 0 | +35 / +3 | -5 / 0 | +5 | **+40** | **2224** / 2866 / 2994 |

`abc`, `fuse` / `ixfuse` / `leaadd`, `narrow` and `dse` (outside the sinking programs) are within +-10 % for LuaJIT on all programs but
byte_multi (`narrow` +29 %) and permute / deltablue (lotteries). They cannot explain any gap.

**Where LuaJIT relies heavily on a feature and our reliance is absent or weak: real causal candidates**

* **Allocation sinking** (raytrace, cd, classic_vector, havlak): LuaJIT +17 to +190 %, ours -2 to +16 %. Half of LuaJIT's reliance
  survives when only side-trace sinking is switched off. LuaJIT without sinking: raytrace 68, cd 161, classic_vector 718, havlak 142 Mcyc,
  against our 94 / 246 / 480 / 151-169 with sinking. On classic_vector we are faster than LuaJIT-without-sinking, so **its whole gap is
  sinking**. On raytrace / cd, sinking is the steady-state part of the gap; the rest is warm-up (section 3).
* **Loop optimisation** (byte_multi, fmt_s): LuaJIT +238 / +336 %, ours +70 / +65 %. byte_multi's gap is steady state (section 3); in
  the same loop LuaJIT's body is 29.6 instructions and ours 94 (docs/JIT_TRACE.md 51.4). fmt_s's gap is warm-up, not the loop.
* **Stitching** (text_adventure): LuaJIT +40 %, ours has no equivalent.

**Features we lack or under-use that matter little to LuaJIT: these cannot explain the gap**

* **Function-entry roots on text_adventure, richards, lua_lexer**: +5 / -1 / 0 % for LuaJIT. C4's "function-entry roots" in
  STUDY_WHY_GAP has no effect on these programs. Function roots matter enormously only for recursion (towers, fib), and we already have
  them there (our `noside` / `recunroll0` profile on fib matches LuaJIT's: +219 / +58 vs +208 / +41).
* **ABC, fusion, narrowing**: ~0 for LuaJIT.
* **CSE / forwarding**: towers and richards rely on them equally in both engines. On lua_lexer we rely on forwarding MORE (+56 vs +16):
  our code has more reloads to forward (fragment boundaries), which is a symptom, not a missing feature.
* **Side traces in general**: both engines rely on them about equally (towers, richards, lua_lexer, fib). classic_vector relies on them
  far more on our side (+295 vs +66), because without side traces its exits fall into materialisation and tier 1.

## 3. Deliverables 2 and 3: where the extra cycles go, and warm-up by construction

### 3.1 In-process repetitions [M] (Mcyc / Minstr; median of 9)

| program | eng | 1 rep Mcyc | 2nd rep | 3rd rep | warm-up = 1st - 3rd | warm-up share | 1 rep Minstr | 2nd rep | 3rd rep |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|
| permute | lj | 68.8 | 217.1 | 85.2 | -16.4 | -24 % | 284.7 | 938.8 | 453.3 |
| permute | rn | 36.7 | 31.9 | 31.6 | 5.1 | 14 % | 191.5 | 186.2 | 185.9 |
| raytrace | lj | 42.5 | 41.3 | 46.2 | -3.7 | -9 % | 167.1 | 168.1 | 184.6 |
| raytrace | rn | 100.5 | 53.4 | 51.8 | 48.7 | 48 % | 294.8 | 201.1 | 201.0 |
| cd | lj | 135.9 | 142.0 | 171.1 | -35.2 | -26 % | 381.0 | 470.9 | 498.9 |
| cd | rn | 250.8 | 128.5 | 132.2 | 118.6 | 47 % | 666.9 | 447.6 | 441.1 |
| towers | lj | 25.8 | 25.3 | 28.9 | -3.1 | -12 % | 106.7 | 124.5 | 145.5 |
| towers | rn | 47.6 | 35.4 | 36.7 | 10.9 | 23 % | 173.3 | 156.4 | 158.5 |
| classic_vector | lj | 251.1 | 269.7 | 263.8 | -12.7 | -5 % | 1143.0 | 1175.2 | 1199.6 |
| classic_vector | rn | 482.7 | 452.2 | 457.6 | 25.2 | 5 % | 2021.9 | 1939.9 | 1937.0 |
| lua_lexer | lj | 510.0 | 540.4 | 625.7 | -115.6 | -23 % | 2004.8 | 2066.4 | 2714.9 |
| lua_lexer | rn | 829.6 | 779.0 | 819.1 | 10.4 | 1 % | 3290.4 | 3212.6 | 3242.0 |
| richards | lj | 35.7 | 41.1 | 94.7 | -59.1 | -165 % | 117.2 | 147.2 | 392.9 |
| richards | rn | 58.2 | 36.8 | 34.5 | 23.6 | 41 % | 166.4 | 125.3 | 123.9 |
| fib | lj | 476.0 | 836.2 | 1387.9 | -911.9 | -192 % | 2535.2 | 4481.1 | 6011.3 |
| fib | rn | 632.6 | 632.2 | 635.0 | -2.4 | -0 % | 3863.7 | 3859.9 | 3859.9 |
| byte_multi | lj | 20.4 | 18.5 | 18.7 | 1.7 | 8 % | 84.9 | 83.6 | 83.5 |
| byte_multi | rn | 38.5 | 34.0 | 34.6 | 3.9 | 10 % | 268.4 | 261.7 | 261.7 |
| fmt_s | lj | 3.6 | 1.8 | 1.9 | 1.7 | 48 % | 10.3 | 8.9 | 8.9 |
| fmt_s | rn | 6.7 | 2.1 | 2.0 | 4.7 | 69 % | 23.3 | 15.4 | 15.4 |
| text_adventure | lj | 1227.3 | 1517.0 | 1180.8 | 46.4 | 4 % | 3729.1 | 4783.1 | 3636.5 |
| text_adventure | rn | 2596.6 | 2588.7 | 2600.3 | -3.7 | -0 % | 8067.1 | 8024.8 | 8024.5 |
| deltablue | lj | 117.2 | 39.2 | 44.1 | 73.1 | 62 % | 314.4 | 121.2 | 140.8 |
| deltablue | rn | 105.8 | 65.3 | 67.1 | 38.8 | 37 % | 308.2 | 224.3 | 223.6 |
| havlak | lj | 143.9 | 172.2 | 217.7 | -73.8 | -51 % | 327.1 | 348.8 | 351.0 |
| havlak | rn | 174.1 | 195.7 | 143.6 | 30.5 | 18 % | 503.1 | 440.8 | 427.1 |

* **Ours is clean**: the 2nd and 3rd repetitions agree within 1-4 % everywhere except havlak, a known layout / formation lottery.
* **LuaJIT is not**: its later repetitions often get *slower* (fib 2.9x, richards 2.7x, lua_lexer +23 %). The reloaded chunk makes new
  closures, so LuaJIT's function-identity specialisation fails and traces are re-formed or blacklisted against the penalised counters.
  For LuaJIT the method gives only an upper bound on warm-up where rep 2 is not slower: raytrace ~1, cd <0, towers ~0.5,
  classic_vector <0, byte_multi 1.7, fmt_s 1.7 Mcyc. STUDY_WHY_GAP's two-size intercept for LuaJIT (raytrace 6.7, cd 31, richards 6.7,
  towers 10) is the other reading. Both say LuaJIT's warm-up is small.
* **Share of the cycle gap that is warm-up** [D] = (our warm-up - LuaJIT warm-up 0..intercept) / gap: raytrace (48.7 - 0..6.7) / 52.5
  = **80-93 %**; cd (118.6 - 0..31) / 133 = **66-89 %**; richards (23.6 - 0..6.7) / 23.1 = **73-100 %**; fmt_s (4.7 - 1.7) / 3.2 =
  **94 %**; towers (10.9 - 0..10) / 20.6 = 5-53 %; classic_vector 9-11 %; byte_multi 12 %; lua_lexer, fib, text_adventure ~0 %.
* **Steady-state ratio** (our 3rd repetition / LuaJIT's whole program, which includes its warm-up and therefore favours LuaJIT):
  raytrace 1.24, cd 1.17, richards **0.97**, fmt_s **1.05** (0.57 against its run total), towers 1.47, classic_vector 1.85,
  lua_lexer 1.79, fib 1.33, byte_multi 1.70, deltablue 1.52 (against LuaJIT's 44 Mcyc steady repetition), text_adventure 2.0.

**The old claim** "compile is 36-56 % of the cycle gap" (raytrace 54 %, cd 43 %, richards 56 %, towers 36 %, fmt_s 47 %) gets the
size of warm-up roughly right, or too small. It names the wrong mechanism, as the next table shows.

### 3.2 What the warm-up consists of [M + D]

| program | warm-up (3.1) | back end + tier-1 compile, doubled (`RNHACK=3`) | lazy stubs (phase timer) | recording (phase timer x 1.4) | **remainder: execution before traces + formation exits + cold caches** |
|---|---:|---:|---:|---:|---:|
| raytrace | 48.7 | 16.1 | 0.9 | 7.4 | **24.3 (50 %)** |
| cd | 118.6 | 48.0 | 6.5 | 13.7 | **50.4 (42 %)** |
| richards | 23.6 | 8.4 | 0.7 | 3.6 | **10.9 (46 %)** |
| towers | 10.9 | 4.4 | 0.5 | 1.5 | **4.5 (41 %)** |
| fmt_s | 4.7 | 0.6 | 0.03 | 0.25 | **3.8 (81 %)** |
| deltablue | 38.8 | 13.0 | 1.1 | 8.1 | **16.6 (43 %)** |

The doubling costs the second compile with warm caches, so it slightly understates. The remainder is computed as the difference
[D]. The phase timers say the same: `pipeline + tier1` raytrace 11.0 Mtsc, cd 31.5, richards 6.9, towers 3.8, fmt_s 0.9. The
remainder is at least as large as compile + recording in every program, and in fmt_s (one loop trace, two side traces that fail
`SideShort`) it is 81 %. Our hot thresholds are 256 back-edges / 16 exits; LuaJIT's are 56 / 10. Section 3.3 measures the sensitivity.

### 3.3 Hot thresholds (formation latency) [M]

`RUSTYNAIL_JIT_TRACE_HOT=loop,exit` on ours (default 256,16), `-Ohotloop / -Ohotexit` on LuaJIT (default 56,10), cycles % against
each engine's default (9 runs median):

| program | ours Mcyc | ours 56,10 | 112,10 | 256,10 | 128,16 | LJ Mcyc | LJ 256,16 | LJ hotloop 256 | LJ hotexit 16 |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| permute | 36.3 | -1 | +1 | +0 | +1 | 311.2 | +52 | -9 | +47 (lottery) |
| raytrace | 96.5 | +15 | +8 | +2 | +9 | 42.1 | +1 | +29 | -0 |
| cd | 248.0 | +22 | +23 | +22 | -3 | 118.3 | +5 | -6 | +9 |
| towers | 45.3 | +19 | +34 | +19 | +5 | 26.0 | -2 | -3 | +3 |
| classic_vector | 474.0 | +11 | +3 | +3 | +6 | 247.8 | +6 | +5 | +0 |
| lua_lexer | 841.9 | +5 | +62 | +4 | -3 | 465.1 | +10 | +9 | +1 |
| richards | 58.7 | +4 | +6 | +6 | -1 | 36.0 | +18 | -4 | -2 |
| fib | 645.3 | -3 | -2 | -2 | -0 | 480.0 | -1 | +0 | -1 |
| byte_multi | 38.4 | 0 | 0 | 0 | 0 | 20.3 | +1 | +0 | -0 |
| fmt_s | 7.0 | -4 | -5 | -6 | -6 | 3.5 | +5 | +6 | +1 |
| text_adventure | 2622.5 | +7 | +7 | +7 | -3 | 1237.0 | +10 | +9 | +5 |
| deltablue | 105.1 | +6 | +3 | -1 | -3 | 116.2 | -30 | -32 | -11 (lottery) |
| havlak | 150.8 | +1 | +1 | +2 | +0 | 122.0 | -3 | -0 | -2 |

* **Earlier side traces make us slower**: exit threshold 10 costs cd +22 %, towers +19 %, raytrace +2..+15 %, text_adventure +7 %,
  with any loop threshold. An earlier loop threshold alone (128,16) is mixed (-6..+9 %). LuaJIT at OUR thresholds loses 0-10 % on most
  programs (raytrace +1 % at 256,16).
* So the warm-up remainder of 3.2 is **not** waiting time that lower thresholds would buy back. Every additional or earlier recording
  costs us more than it saves. LuaJIT's formation is cheap enough that its thresholds hardly matter. Formation latency and compile
  throughput are one problem: the remainder shrinks only if each formation step (recording, compile, the exits that run through the
  handler and tier 1 until a side trace attaches) gets cheaper.
* Related [M, `[jit]` statistics of the same runs]: tier-1 code per program is large. cd 489 KB in 40 compiles, deltablue 407 KB, raytrace
  237 KB, lua_lexer 242 KB, classic_vector 232 KB, richards 205 KB; fmt_s is 28 KB for its one main chunk. That is consistent with the
  2-3x instruction-cache miss rates of 3.4 during warm-up (not ablated: a lead, not a measured cause).

### 3.4 Cycle causes: instructions or IPC, and of which kind [M]

Whole runs (7 runs median; Zen 5, 8 dispatch slots per cycle; `frontend% / backend% / retire% / badspec%` of slots; `fe-lat%` =
cycles with all 8 slots empty for frontend reasons; per-kilo-instruction event rates):

| program | eng | Mcyc | Minstr | IPC | retire% | frontend% | fe-lat% | backend% | badspec% | brmisp/ki | indmisp/ki | icmiss/ki | stli/ki | ldwait% | l2miss/ki |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| permute | lj | 274.5 | 1171.1 | 4.27 | 47.3 | 24.8 | 7.8 | 23.2 | 4.39 | 0.90 | 0.06 | 33.3 | 2.42 | 8.9 | 0.03 |
| permute | rn | 36.7 | 191.5 | 5.21 | 49.6 | 12.5 | 8.9 | 36.0 | 1.43 | 0.43 | 0.04 | 6.25 | 4.38 | 20.2 | 0.20 |
| raytrace | lj | 42.0 | 166.0 | 3.95 | 44.0 | 31.8 | 15.8 | 19.8 | 4.44 | 1.03 | 0.14 | 19.4 | 1.79 | 17.4 | 0.18 |
| raytrace | rn | 97.5 | 294.4 | 3.02 | 33.1 | 47.4 | 22.3 | 14.2 | 4.75 | 1.93 | 0.30 | 58.7 | 3.16 | 23.2 | 1.33 |
| cd | lj | 115.1 | 317.0 | 2.76 | 30.3 | 43.6 | 27.0 | 16.7 | 8.73 | 3.05 | 0.29 | 41.6 | 1.19 | 25.5 | 2.32 |
| cd | rn | 252.3 | 689.7 | 2.73 | 29.8 | 46.2 | 27.3 | 17.2 | 6.34 | 2.85 | 0.40 | 58.2 | 4.02 | 28.3 | 3.65 |
| towers | lj | 25.2 | 105.1 | 4.16 | 43.0 | 19.0 | 12.4 | 34.8 | 3.15 | 0.69 | 0.06 | 10.1 | 0.55 | 13.7 | 0.21 |
| towers | rn | 46.4 | 169.0 | 3.64 | 36.3 | 40.9 | 21.2 | 18.4 | 3.98 | 1.41 | 0.21 | 31.0 | 1.22 | 25.3 | 0.75 |
| classic_vector | lj | 253.7 | 1143.7 | 4.51 | 51.5 | 19.6 | 10.8 | 24.0 | 5.00 | 0.83 | 0.02 | 16.2 | 3.19 | 16.8 | 0.25 |
| classic_vector | rn | 491.3 | 2021.5 | 4.11 | 44.7 | 30.2 | 10.1 | 21.5 | 1.73 | 0.38 | 0.04 | 26.8 | 5.65 | 26.4 | 0.50 |
| lua_lexer | lj | 462.6 | 1909.2 | 4.13 | 45.7 | 33.7 | 17.9 | 15.6 | 4.18 | 0.71 | 0.02 | 29.3 | 1.41 | 19.2 | 0.32 |
| lua_lexer | rn | 832.6 | 3291.1 | 3.95 | 42.2 | 47.1 | 15.6 | 9.2 | 1.56 | 0.34 | 0.03 | 55.8 | 0.48 | 22.0 | 0.42 |
| richards | lj | 36.1 | 121.6 | 3.37 | 32.8 | 34.6 | 13.2 | 28.7 | 3.29 | 0.94 | 0.11 | 22.8 | 0.36 | 13.6 | 0.21 |
| richards | rn | 58.2 | 166.4 | 2.86 | 29.2 | 50.5 | 24.0 | 14.9 | 5.10 | 2.21 | 0.33 | 52.1 | 3.13 | 22.2 | 1.29 |
| fib | lj | 482.7 | 2581.5 | 5.35 | 60.6 | 2.6 | 2.0 | 35.4 | 1.65 | 0.21 | 0.00 | 0.13 | 15.6 | 3.6 | 0.01 |
| fib | rn | 648.0 | 3864.5 | 5.96 | 59.2 | 16.5 | 5.6 | 21.2 | 2.81 | 0.36 | 0.00 | 2.09 | 4.19 | 23.1 | 0.01 |
| byte_multi | lj | 20.3 | 84.9 | 4.18 | 46.9 | 6.3 | 5.2 | 44.9 | 1.04 | 0.33 | 0.02 | 2.16 | 0.04 | 2.8 | 0.20 |
| byte_multi | rn | 38.4 | 268.3 | 6.98 | 65.3 | 7.9 | 6.4 | 25.4 | 0.83 | 0.17 | 0.02 | 1.92 | 0.18 | 8.6 | 0.15 |
| fmt_s | lj | 3.47 | 10.2 | 2.95 | 30.4 | 36.0 | 28.3 | 30.9 | 5.43 | 2.23 | 0.13 | 17.4 | 0.30 | 16.0 | 1.53 |
| fmt_s | rn | 6.49 | 23.3 | 3.59 | 35.5 | 41.5 | 33.5 | 17.8 | 5.12 | 1.75 | 0.21 | 24.2 | 2.97 | 22.4 | 1.60 |
| text_adventure | lj | 1351.6 | 4061.4 | 3.00 | 34.8 | 47.9 | 21.8 | 9.6 | 8.21 | 1.68 | 0.10 | 53.4 | 2.32 | 18.2 | 0.27 |
| text_adventure | rn | 2630.6 | 8075.0 | 3.07 | 34.3 | 60.2 | 20.6 | 2.9 | 2.28 | 0.69 | 0.11 | 123.4 | 0.58 | 23.2 | 0.15 |
| deltablue | lj | 116.8 | 304.2 | 2.60 | 27.0 | 27.1 | 17.4 | 38.0 | 6.95 | 2.29 | 0.21 | 26.2 | 1.27 | 33.3 | 0.56 |
| deltablue | rn | 105.3 | 307.7 | 2.92 | 31.2 | 46.0 | 24.5 | 17.8 | 4.59 | 2.07 | 0.35 | 69.7 | 2.61 | 26.8 | 2.43 |
| havlak | lj | 126.1 | 322.8 | 2.56 | 28.8 | 31.6 | 21.0 | 30.3 | 9.06 | 2.60 | 0.15 | 29.3 | 2.75 | 39.2 | 4.37 |
| havlak | rn | 152.8 | 502.8 | 3.29 | 36.4 | 34.0 | 19.8 | 24.3 | 5.03 | 1.19 | 0.14 | 47.7 | 2.40 | 33.2 | 3.36 |

Steady state only (the 3rd repetition minus the 1st, halved; scripts/causal-reps.lua with the same event groups, 7 runs) [M]:

| program | eng | Mcyc | Minstr | IPC | frontend% | backend% | badspec% | icmiss/ki | brmisp/ki | stli/ki |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| raytrace | lj / rn | 42.4 / 51.9 | 176 / 201 | 4.15 / 3.87 | 30 / 43 | 21 / 14 | 3.6 / 0.4 | 21.6 / 56.5 | 0.81 / 0.13 | 1.40 / 1.79 |
| cd | lj / rn | 146.2 / 126.9 | 452 / 444 | 3.09 / 3.50 | 38 / 39 | 19 / 21 | 9.1 / 2.2 | 36.5 / 60.8 | 2.44 / 0.57 | 1.47 / 3.45 |
| richards | lj / rn | 67.6 / 35.1 | 270 / 125 | 4.00 / 3.55 | 29 / 49 | 29 / 16 | 1.5 / 0.9 | 27.6 / 45.8 | 0.36 / 0.35 | 0.66 / 1.27 |
| towers | lj / rn | 28.6 / 35.8 | 139 / 157 | 4.86 / 4.40 | 7 / 38 | 41 / 17 | 1.2 / 1.0 | 4.8 / 29.7 | 0.24 / 0.30 | 1.99 / 0.30 |
| fmt_s | lj / rn | 1.8 / 2.0 | 8.9 / 15.4 | 4.91 / 7.74 | 46 / 14 | 4 / 15 | 1.1 / 1.2 | 2.1 / 2.3 | 0.25 / 0.11 | 0.45 / 0.62 |
| classic_vector | lj / rn | 261.0 / 449.6 | 1186 / 1939 | 4.55 / 4.31 | 18 / 27 | 26 / 25 | 5.1 / 1.4 | 16.6 / 24.6 | 0.91 / 0.25 | 2.43 / **6.36** |
| lua_lexer | lj / rn | 580.8 / 782.4 | 2384 / 3227 | 4.10 / 4.13 | 34 / 46 | 18 / 9 | 3.3 / 1.1 | 34.3 / 54.1 | 0.55 / 0.18 | 1.08 / 0.40 |
| fib | lj / rn | 1101.8 / 630.3 | 5151 / 3860 | 4.67 / 6.12 | 2 / 15 | 45 / 22 | 1.2 / 2.5 | 8.5 / 1.6 | 0.21 / 0.35 | 15.9 / 4.88 |
| deltablue | lj / rn | 35.9 / 63.2 | 124 / 224 | 3.44 / 3.54 | 33 / 39 | 24 / 22 | 7.4 / 1.7 | 21.7 / 64.5 | 1.94 / 0.64 | 0.86 / 1.07 |
| text_adventure | lj / rn | 1509.4 / 2571.5 | 4823 / 8025 | 3.19 / 3.12 | 48 / 60 | 9 / 3 | 6.0 / 2.3 | 60.1 / 125.0 | 1.31 / 0.67 | 2.02 / 0.92 |

(LuaJIT's repetitions re-form traces for fib / richards / cd: use its whole-run row for those.)

Per program, the cycle ratio splits as instruction ratio x IPC ratio (whole runs; log shares) [D]:

| program | cycle ratio | = instructions | x IPC (LJ / ours) | gap from instructions | gap from IPC | kind of the IPC part |
|---|---:|---:|---:|---:|---:|---|
| raytrace | 2.32 | 1.77 | 1.31 | 68 % | 32 % | warm-up: frontend 47 % vs 32 %, icache misses 3x. Steady state IPC 3.87 vs 4.15 |
| cd | 2.19 | 2.18 | 1.01 | ~100 % | 0 | none in total; steady state our IPC is higher |
| towers | 1.84 | 1.61 | 1.14 | 78 % | 22 % | frontend 41 % vs 19 % and icache 3x, **also in steady state** (38 % vs 7 %) |
| classic_vector | 1.94 | 1.77 | 1.10 | 86 % | 14 % | store-to-load conflicts 2.6x (6.36 vs 2.43 /ki steady), frontend 27 % vs 18 % |
| lua_lexer | 1.80 | 1.72 | 1.05 | 92 % | 8 % | frontend (icache 1.6x) |
| richards | 1.61 | 1.37 | 1.18 | 66 % | 34 % | warm-up frontend; steady state icache 1.7x |
| fib | 1.34 | 1.50 | 0.90 | >100 % | <0 (we retire faster) | the extra instructions are cheap; frontend 16.5 % vs 2.6 % and load-wait 23 % vs 4 % (the return chain) |
| byte_multi | 1.89 | 3.16 | 0.60 | >100 % | <0 | IPC 7.0: the extra instructions run nearly free; 1.9x cycles for 3.2x instructions |
| fmt_s | 1.87 | 2.27 | 0.82 | >100 % | <0 | warm-up |
| text_adventure | 1.95 | 1.99 | 0.98 | ~100 % | 0 | frontend 60 % vs 48 %, icache 2.1x |
| havlak | 1.21 | 1.56 | 0.78 | >100 % | <0 | |

**Verdict.** The cycle gap is an **instruction-count gap** for every program except raytrace / richards / towers, which add 20-34 %
from IPC. That IPC deficit is a frontend one: icache misses are 2-3x LuaJIT's, mostly during warm-up (compiler code, fresh code), and
in steady state on towers / richards / deltablue / lua_lexer (bigger traces and stubs; docs/STUDY_WHY_GAP.md C1 notes a 2152-byte
machine-code trace for 4 IR ops). Branch mispredictions and bad speculation are *lower* on our side everywhere. classic_vector's
store-to-load conflicts (12 M per steady repetition) are the one backend item worth a look: they sit on the materialisation and Rust
table paths (the same program and paths as section 2's sinking finding). They were not ablated separately, so this stays a lead.
"Instruction diets convert at 1/3" is true at the margin, inside high-IPC trace code (byte_multi runs at IPC 7). It is false for the
gap as a whole: the gap is instructions, and most of those are not in the hot trace loop. They are in warm-up, materialisation,
tier 1 and the runtime.

## 4. Deliverable 4: backtest of the prediction methods

Predicted vs realised, with the method used for the prediction (sources: the sections of docs/JIT_TRACE.md and the studies;
ratio = realised / predicted, range midpoints):

| handle (§) | program | predicted | realised | ratio | prediction method |
|---|---|---|---|---:|---|
| S1 lazystub (31) | cd | -6..-10 % c (STUDY_LUAJIT_GAP) | -1.6..-2.6 % c | 0.26 | bucket share x fraction |
| P1 presize (32) | classic_vector | -19 % c | -14.8 % c | 0.78 | source-rewrite ceiling |
| E1+E2 (34) | fib / binary_trees / sumrec | -41.6 / -21.1 / -33.5 % i | -44.5 / -22.8 / -37.5 % i | 1.07-1.12 | measured prototype |
| E4 (35) | permute | -5.6 % i | -4.8 % i | 0.86 | piece count |
| E5 (35) | permute / towers | -13.5 / -12 % i | -9.9 / 0 % i | 0.73 / 0 | piece count |
| E6 dlink (36) | fib / binary_trees / permute / queens | -5.5 / -3 / -2 / -4.5 % i | -8.3 / -2.2 / -3.7 / 0 % i | 1.5 / 0.7 / 1.9 / 0 | piece count |
| E9 (37) | five programs | 0 % c | 0 | exact | same-binary measurement |
| deepprobe (38) | rec_loop | ceiling 438 M i | 444.5 M i | 0.92 | knob ceiling |
| T1 retfsel (39.1) | lua_lexer / queens / scheme / expr_compiler | -17 / -16.5 / -11 / -9 % i | -7.6 / 0 / 0 / 0 % i | 0.45 / 0 | knob ceiling restricted by a selector |
| T3 heatside (39.2) | six programs | -3.2..-30.8 % i | -3.1..-28.8 % i | 0.94-1.09 | same-binary knob ceiling |
| T2 (39.3) | raytrace, cd, deltablue, richards, towers | -18, -13, -12, -11, -8 % i | -5.0, -1.4, -1.8, -0.8, -1.0 % i | 0.07-0.28 | bucket share x 1/2 |
| N1 (41) | type_entry_stable / matmul / permute-queens-towers | -20 / -10 / -5..-8 % i | -22.6 / -11.9 / ~-0.5 % i (cycles -9.6 / -5.7) | 1.13 / 1.19 / 0.1 (cycles 0.55) | static instruction count |
| N2 (42) | co_yield | -30 % c | -8.8 % c | 0.29 | piece count on skidded shares |
| N2b (45) | co_yield | ~-3 % | -17 % c | 5.7 | piece estimate (under) |
| TR24c (43) | raytrace | -3.9 % | -5.6 % c (lottery) | ~0 systematic | scratch build at one threshold |
| T2b (43) | raytrace / cd / richards | -6 / -4 / -5 % | -0.74 / -0.52 / -0.40 % i | 0.08-0.13 | phase share x 1/2 |
| blfirst (44) | raytrace | ~-1.5 % | -4.1 % i | 2.7 | count of removable recordings (under) |
| T6 (44) | raytrace / lua_parser / events | -10 / -4 / -4 % | -6.6 / -1.0 / ~0 % i | 0.66 / 0.25 / 0 | tier-1 share x fraction |
| T8 (44) | lua_lexer | -3 % | -17.1 % c | 5.3 | token-share arithmetic (under) |
| T4 (46) | cd 10 / cd default | -12 % | -12.3 / -1.7 % c | 0.88 / 0.49 | tier-1 share as ceiling |
| T9 (47) | deltablue 400 / default | -15 % share | -13.0 % c / +6.6 % c | 0.82 / <0 | share as ceiling |
| T5 (48) | havlak | -6 % i | -7.6 % i, -0.8 % c | 1.27 i / 0.13 c | tier-1 + helper share |
| T7 (48) | deltablue L / richards L / towers | -2.0 / -1.4 / +4.4 % i | -2.1 / -0.06 / +4.6 % i | 1.05 / 0.04 / 1.05 | knob ceiling (`fnmin=0`) |
| C1 cspeed (49) | raytrace, richards, cd, towers, fmt_s | -21, -15, -16, -11, -22 % c | -1.8, -2.3, -4.2, -2.4, -1.5 % c | 0.07-0.26 | IBS compile bucket x 2/3 |
| slink (50) | classic_vector, cd, lua_lexer, raytrace | -13, -13, -7, -6 % c | +1.0, -0.2, -0.2, +0.4 % c | ~0 | IBS exit bucket x 80 % |
| rapol (51) | byte_multi, towers, lua_lexer, classic_vector | -25, -9, -5, -3 % i | -8.7, -0.9, -0.4, -0.5 % i | 0.08-0.35 | spill bucket by operand pattern |

Median realisation by method [D]:

| method | n | median | range | verdict |
|---|---:|---:|---|---|
| bucket share x assumed removal fraction (S1, T2, T2b, T6, C1, slink, rapol) | ~20 | **0.12** | 0-0.66 | discard: over-estimates ~8x |
| static piece / instruction count per call or iteration (E4, E5, E6, N1, N2) | 9 | 0.73 in instructions, ~0.4 in cycles | 0-1.9 | instructions OK on the target shape only; wrong in cycles |
| share-based coverage estimates (T8, blfirst, N2b) | 3 | 2.7-5.7 (under) | | discard: a removed abort brings back a whole region |
| tier-1 share as a ceiling (T4, T5, T9) | 4 | 0.85 at large size, <0.5 at default | | usable only with the size the benchmark runs at |
| **same-binary knob ceiling** (T3, T7, deepprobe, `xregs=3` vs rapol, E9) | 12 | **0.95** | 0.04-1.09 | keep |
| knob ceiling the shipped selector must restrict (T1) | 4 | 0.45 / 0 | | keep, with the selector applied before the estimate |
| measured prototype / scratch build (E1+E2, P1) | 4 | 1.0 | 0.78-1.12 | keep |

**Would the methods of this study have retrodicted the realised numbers?**

* **Ablation of our own knob** (2.2), at this base, measures the realised value of every built-and-knobbed handle by definition:
  `slink0` +0..+2 % (realised -0.2..-1 %), `rapol0` +0..+10 % (realised fmt_s -9 %, byte_multi -5 %, others ~1 %), `dlink0` fib +7 %
  (realised -8.3 % i), `recunroll0` fib +58 %, `midfold0` permute +12 % / fib +14 %. These agree with the sections that built them.
* **LuaJIT ablation as a prior.** Before slink: LuaJIT `nosidesink` = 0 on lua_lexer / richards / towers, and 16-55 % on raytrace / cd /
  classic_vector. That says side-trace cost there is about sunk allocations, not the link. slink was predicted -7..-13 % and realised ~0:
  the LuaJIT reading would have pointed at (B), not (A). Before C1: LuaJIT shows no feature that compile speed would buy. Our doubling ceiling
  (3.2) gives raytrace 16 / cd 48 / richards 8 / towers 4.4 Mcyc for back end + tier 1 together. A 7-11 % cheaper compile (C1's actual
  lever) x those ceilings = raytrace -1.3 %, cd -1.6..-2.2 %, richards -1.2 %, towers -0.8 %, against the realised -1.8 / -4.2 / -2.3 / -2.4 %.
  That is the right order of magnitude (the predicted -11..-22 % was 5-10x off). The study's estimate assumed a 3x cheaper compile, which
  nobody knew how to build; a ceiling x a demonstrated lever fraction is the honest form.
* **Doubling as a ceiling for exits** (`RNHACK=4`): materialisation costs classic_vector +107 Mcyc (22 %), cd +37 (15 %). The stub part
  that slink removed is not in that number. Read together with LuaJIT's `nosidesink`, this would have said "the link is the small part"
  before slink was built.
* **Warm-up by construction** would have bounded C1 from above: the whole warm-up of raytrace is 48.7 Mcyc, and the compile is a
  third of it.
* **Ceiling for the frame model**: deleting the return checks gives fib -4.4 %. The E-series and N-series frame diets realised fib -8.3 %
  (dlink) and none of the rest moved fib in cycles. Consistent.

Methods discarded: bucket share x fraction, and share-of-trace-instructions x assumed IPC conversion. Kept: same-binary ablation, LuaJIT
ablation as a prior on which mechanism matters, doubling and deletion ceilings, and in-process repetitions.

## 5. Deliverable 5: ceilings [M]

Same scratch binary, `RNHACK=0` baseline (ceilings marked "delete" are unsound and valid only where the output stayed equal).
Δ in Mcyc and % of the program (cycles / instructions):

| ceiling | how | permute | raytrace | cd | towers | classic_vector | lua_lexer | richards | fib | byte_multi | fmt_s | text_adv. | deltablue | havlak |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| (d) trace back end | double | +0.5 (+1/+1) | +13.1 (+14/+13) | +41.5 (+17/+16) | +3.7 (+8/+7) | +12.4 (+3/+2) | +6.8 (+1/+1) | +6.7 (+12/+11) | +1.1 (0/0) | +0.3 (+1/0) | +0.3 (+5/+2) | noise | +8.4 (+8/+7) | +3.2 (+2/+2) |
| (d) tier-1 lowering + encode | double | +0.2 | +3.5 (+4/+3) | +5.3 (+2/+3) | +0.2 (+1/+1) | +2.6 (+1/0) | -2.0 (0/0) | +3.0 (+5/+5) | 0 | +0.4 | +0.4 (+6/+5) | noise | +4.7 (+5/+5) | +6.0 (+4/+2) |
| (d) both | double | +0.7 (+2/+1) | **+16.1 (+17/+16)** | **+48.0 (+20/+19)** | +4.4 (+10/+8) | +13.6 (+3/+2) | +8.5 (+1/+1) | **+8.4 (+15/+16)** | 0 | +0.7 (+2/+1) | +0.6 (+10/+7) | +5 (0/0) | +13.0 (+13/+12) | +7.8 (+5/+4) |
| (e) sunk-table materialisation | double | 0 | +3.8 (+4/0) | **+36.8 (+15/+16)** | 0 | **+107.4 (+23/+25)** | 0 | 0 | 0 | 0 | 0 | +10 (0) | 0 | +4.8 (+3/+2) |
| (a) native-return + room checks, site compare kept | delete | -0.6 (-2/-1) | (0) | (0) | (err) | (0) | 0 | 0 | **-29.0 (-4.4/-3)** | 0 | 0 | 0 | 0 | 0 |
| (a) the same, all return guards | delete | (diff) | (err) | (err) | (err) | -2.5 (0) | +4.6 | 0 | (err) | 0 | 0 | 0 | 0 | 0 |
| (a) native-return + room checks | double | +1.9 (+5/+7) | 0 | +3.2 (+1/0) | +1.5 (+3/+3) | 0 | 0 | 0 | +85.3 (+13/+14) | 0 | 0 | 0 | 0 | 0 |
| (c) tag guards | double | +4.9 (+14/+13) | +1.6 (+2/+4) | +8.1 (+3/+4) | +5.5 (+12/+11) | +25.3 (+5/+4) | +48.2 (+6/+5) | +6.5 (+11/+11) | +79.4 (+12/+10) | 0 (+1/+2) | +0.2 (+3/+5) | 0 | +2.5 (+2/+4) | +1.3 (+1/+1) |
| (c) every memory-compare guard | double | +5.9 (+16/+14) | 0 (+4) | +8.6 (+4/+4) | +7.1 (+16/+14) | +28.8 (+6/+4) | +53.5 (+6/+5) | +10.2 (+17/+13) | +78.9 (+12/+10) | +0.6 (+2/+2) | 0 (+5) | +12 (0) | +3.7 (+4/+5) | +1.5 (+1/+1) |
| (c) tag guards | delete | **-4.5 (-12/-13)** | (diff) | (err) | (err) | (diff) | (diff) | (err) | **-29.3 (-5/-10)** | +0.5 (+1/-2) | 0 (-1/-5) | (err) | +6 (+6/+5) | (err) |
| (b) function-entry roots | knobs `t7`, `fnroot`, `fnmin=0` | | 0..+7 | | 0..+8 | | -1..-1.3 | 0..+6 | | | | **-0.4..-0.9** | -1..+8 | |
| (b) function roots on LuaJIT | scratch LJ | | +24 | +24 | +572 | +1 | 0 | -1 | +502 | 0 | 0 | +5 | lottery | -6 |
| (b) stitching on LuaJIT | `-Ominstitch` | | -1 | -1 | -2 | 0 | +10 | +1 | 0 | 0 | -1 | **+40** | -10 | 0 |
| (f) side-trace re-sinking on LuaJIT | scratch LJ | +6 | **+16** | **+38** | -1 | **+55** | +1 | -1 | -1 | 0 | 0 | -1 | -1 | +11 |

Notes [M]:
* The deletion of the return guards fails on most programs. Its "diff" / "err" readings show that those guards do fail in real runs:
  the return site differs from the trace's assumption, so the frame-model ceiling is only measurable where the output stays equal (fib,
  permute). fib's whole return-check cost is 29 Mcyc of its 167 Mcyc gap (17 %). The rest of the fib gap is the protocol itself:
  `CallInfo` link, savedpc, base adjust, the dispatch cell load and `jmp *`. It shows up as frontend 16.5 % vs 2.6 % and load-wait 23 % vs
  3.6 % (3.4), a chain of dependent loads at every return. It is not removable short of a frame-model change (docs/STUDY_WHY_GAP.md item 6).
* Doubling the guards measures their marginal cost; deleting them (where valid) gives a similar figure: permute -12 % / +14 %, fib -5 %
  / +12 %. The guard ceilings are the cost of ALL type checks. LuaJIT also checks types (one compare of the NaN-box tag), so a value-layout
  change would save only the second word of each check and the second store per slot. On these numbers that is a fraction of 10-14 % on
  the method / recursion programs and of 1-6 % elsewhere.
* The doubled back end runs with warm caches and the same allocation decisions. So the ceiling of "free compile" is a little above
  these numbers, while a real rewrite saves a fraction of them. Tier-1 lowering for traced prototypes is part of the tier-1 row (+0.2 to
  +6 Mcyc): even deleted, it saves at most 5 % (richards, deltablue, fmt_s).
* (e): the duplicate also allocates garbage that the collector must sweep later. The real materialised tables also become garbage, so the
  doubled cost includes their GC share. The exit-handler part (`exit_entry`, `restore`, `restore_chain`, ~6 % of classic_vector's
  instructions per docs/JIT_TRACE.md 50.6) is not in the number.

## 6. Deliverable 6: ranked candidates and recommendation

Realisation factors from section 4: a handle built on a measured ceiling of the same mechanism realised 0.8-1.0 when its scope matched
the ceiling (T3, P1, E1/E2, deepprobe), and ~0.45 when the shipped version had to be restricted (T1). Below, **factor 0.5** is used
for every ceiling-based candidate whose implementation must be restricted for soundness, and 0.8 where the ceiling is the
mechanism's own knob.

| # | candidate | measured ceiling (cycles, % of ours) | evidence it is causal | factor | expected | recommendation |
|---|---|---|---|---:|---|---|
| 1 | **Sunk-allocation replay into side traces** (LuaJIT `snap_replay` + sinking in side traces; docs/JIT_TRACE.md 50.6 (B)), plus a sunk-frame stub for metamethod frames | classic_vector 107 Mcyc (22 %), cd 37 (15 %), raytrace 3.8 (4 %), havlak 4.8 (3 %); LuaJIT loses +55 / +38 / +16 / +11 % without it | LuaJIT ablation (2.1, scratch), our doubling (5), and classic_vector beats LuaJIT-without-sinking | 0.5 | classic_vector **-11 %**, cd **-7 %**, raytrace -2 %, havlak -1.5 % | **build** (largest measured, mechanism proven on LuaJIT) |
| 2 | **Warm-up / formation cost** (excluding the back end of #3): exits through the handler and tier 1 while side traces are pending, failed side recordings (cd 129 of 282, deltablue 43 of 76, richards 19 of 38 side recordings fail), recording, tier-1 code size | remainder of warm-up + recording: raytrace 32, cd 64, richards 15, fmt_s 4.0, towers 6, deltablue 25 Mcyc (13-60 % of the program) | warm-up by construction (3.1-3.2). Thresholds are NOT the lever (3.3: earlier = slower) | unknown: no handle has attacked it; use 0.3 | raytrace ~-10 %, cd ~-8 %, richards ~-8 %, fmt_s ~-18 % | **next study** (per-formation-step cost, same doubling method), then builders |
| 3 | Compile back end rewrite (single pass RA+emit, lazy tier 1 for traced protos) | back end + tier 1 doubled: cd 48 (20 %), raytrace 16 (17 %), richards 8.4 (15 %), deltablue 13 (13 %), towers 4.4 (10 %), fmt_s 0.6 (10 %) | doubling | 0.3 (a 2x cheaper compile realises half the ceiling; C1 got 0.1) | cd -6 %, raytrace -5 %, richards -4.5 %, deltablue -4 % | only after 1-2; a rewrite for ~5 % |
| 4 | Runtime / library speed outside traces (text_adventure, lua_lexer helpers) | text_adventure: LuaJIT's interpreter is 0.85x our full JIT; our interpreter is 1.35x LuaJIT's | ablation (`interp`, `tier1` vs `joff`) | n/a (per function) | profile-driven, per helper | **build per function** for interpreter-shaped programs |
| 5 | C-call stitching | LuaJIT relies on it +40 % on text_adventure, +10 % lua_lexer; nothing else | LuaJIT ablation | 0.5 x (LuaJIT's share of our time) | text_adventure ~-10 % at best | after 4; our traces are worth only 9 % on that program, so stitching alone cannot repeat LuaJIT's 40 % |
| 6 | Loop optimisation quality (byte_multi) | LuaJIT relies +238 %, ours +70 %; byte_multi steady gap 16 Mcyc (42 %) | LuaJIT ablation | 0.5 | byte_multi up to -20 % | small program set; low priority |
| 7 | Value layout (NaN-boxing / one-word tag) | all type guards: 10-14 % on permute / towers / richards / fib, 1-6 % elsewhere; only part removable | doubling and deletion | 0.3 | 3-4 % on recursion / method programs | **do not build** (architectural, small) |
| 8 | Frame model | return / room checks: fib 4.4 %, permute 1.7 %; the dependent-load return chain is the rest of fib's gap but not ablatable | deletion | 0.8 for the checks | fib ~-3.5 % from checks | **do not build** further diets; the rest needs the architectural change |
| 9 | Function-entry roots (T7 / `fnroot`) | 0 to -1 % on text_adventure; LuaJIT relies +5 % there | ablation both sides | | 0 | **do not build** |
| 10 | Side-link register passing, allocator policy, guard diets, compile-speed levers at identical code | our knobs (`slink0`, `rapol0`, `guarddiet0`) +0..+2 % except rapol on fmt_s / byte_multi | ablation | | ≤1 % | **stop** |

**What the data says the worst gaps actually are (blunt):**

* **raytrace, cd, richards, fmt_s: warm-up.** 66-100 % of the cycle gap is paid before steady state. In steady state, richards and
  fmt_s are at parity with LuaJIT, and raytrace / cd are 1.2x. Two thirds of that warm-up is NOT compiling: it is running in tier 1 /
  the interpreter while traces form, plus recording. Starting traces earlier does not help (3.3). The steady part of raytrace / cd
  is allocation sinking (candidate 1).
* **classic_vector: allocation sinking, nothing else.** LuaJIT without sinking is slower than us (718 vs 480 Mcyc). Its side-trace
  re-sinking alone is worth +55 %. Our materialisation costs at least 107 Mcyc.
* **text_adventure: the runtime, not the tracer.** LuaJIT's *interpreter* beats our whole JIT. Neither function-entry roots (LuaJIT +5 %,
  ours 0) nor more traces will close a gap of 1.3 G cycles whose base is library and runtime code speed.
* **lua_lexer: steady state, instruction count** (1.72x instructions, IPC equal, warm-up ~0). Its ablation profile matches LuaJIT's
  except forwarding (+56 vs +16 %). Neither engine depends on sinking there. Not explained by any candidate here. The previous study's
  C3 (code quality: guards 6 %, spills, slot reloads across fragments) is the remaining hypothesis. Tag-guard doubling gives +6 %, so the
  guards are not it either. **This gap has no measured cause yet.**
* **fib, permute, towers: the call/return protocol** (frontend and load-latency stalls of the return chain). Measurable only partly
  (checks 4.4 % of fib); the rest needs the frame-model change that the call-protocol design rejected. towers additionally shows
  steady-state frontend stalls 38 % vs 7 % (icache misses 6x), which points at code size.
* **byte_multi: loop-body instruction count** (3.2x instructions at IPC 7.0); LuaJIT's loop optimisation does far more.
* **deltablue and permute: LuaJIT lotteries.** LuaJIT's median loses to us. Its lucky runs (and its `noside` / scratch no-function-root
  runs) are 2-4x faster. Not a gap to chase without fixing the baseline.

## 7. Caveats

* LuaJIT's in-process repetitions are not a clean warm-up measure (new closures re-specialise). LuaJIT's warm-up is taken from the
  upper bound where rep 2 is not slower, and from STUDY_WHY_GAP's two-size intercept.
* Phase timers are TSC, converted with a ratio fitted against the doubling build (1.2-1.5). The recording column is therefore +-20 %.
* The `(diff)` / `(err)` columns of the deletion hacks are not used. A deletion that kept the output on one program may still delete a
  guard that never fails there but would elsewhere: those numbers are ceilings, not designs.
* havlak and deltablue move +-10 % between series (formation / layout). Use the hack series' baselines for their ceilings.
* The scratch builds and the scratch LuaJIT are not committed. Their patches are described in section 1; the hack points are the
  functions named there.

## 8. Reproduction

```
scripts/causal-grid.py SPEC.json OUT.json      # spec format in the script header; run under systemd-run MemoryMax + timeout
scripts/causal-table.py grid OUT.json [base] [event]   |  ev OUT.json...  |  reps OUT.json
<lua> scripts/causal-reps.lua bench/<p>.lua    # CAUSAL_REPS=N repetitions in one process
```
