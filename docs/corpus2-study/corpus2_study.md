# corpus2 study: 32 more programs, re-measured blockers, parked-ideas ledger (rustynail main 65e69653, 2026-11-07)

**Setup.**
- Binary: plain release `cargo build --release -p rn-cli --features jit,trace` of `git archive HEAD` in `~/.cache/corpus2`
  (deleted afterwards). The main checkout's uncommitted `POLYSIDE_PROCESS = 48` is therefore **not** in this binary: it has 0.
- PUC = `ref/55/build-seed0/lua`. LuaJIT = `/usr/bin/luajit` 2.1.1793383622.
- Every run: `systemd-run ... MemoryMax -p MemorySwapMax=0 timeout`, stdin `/dev/null`, `RUSTYNAIL_SEED=0`.

**Where things are (scratchpad `corpus2/`).**
- `bench/corpus2/`: programs, `.expected`, MANIFEST.
- `docs/JIT_PARKED.md`: the ledger.
- `study/`: tools, data, micro repros, ablation variants.

## 0. Things that change how earlier results should be read (found on the way)

1. **This machine has two core types.** CPUs 0, 2, 4, 6 (+8, 10, 12, 14) are Zen5 at 5.1 GHz. CPUs 1, 3, 5, 7 (+9, 11, 13, 15) are
   Zen5c at 3.5 GHz, with less L3.
   - The "CPUs 3-7" worker pool mixes them. The same program costs up to 1.6x more PUC cycles on a Zen5c core.
   - The ours/LJ ratio of one program also moves with core type: class_middleclass 6.5 vs 8.2, containers_lru_heap 1.42 vs 1.21.
   - Geomeans are stable (dev 1.624 vs 1.640, heldout 1.595 vs 1.617, corpus 1.494 vs 1.580).
   - A/B pairs that run as separate queue items, rather than as columns of one item, can land on different core types. My first
     ablation pass had exactly that problem and was redone pinned to CPUs 4 and 6.
   - Headline numbers below are Zen5 (CPUs 4, 6). Zen5c tables are in `study/data/mainZ5c_table.md`.
2. **The default trace policy is not deterministic.** The hop-rate rule (`hopcyc`, BUNDLE-5) compares time-stamp-counter ticks per entry.
   - Repro (`study/var/diff_levenshtein__flatloop.lua`), all with `RUSTYNAIL_SEED=0`, also under `setarch -R`:
     - default mode gives 1502-1568 Mcyc or 495-560 Mcyc, about 3x, run to run;
     - the two modes install different trace sets: entered 0.73 M vs 4.8 M;
     - `hopcyc=0` gives 8/8 fast runs; `warm=0` gives 8/8 runs at about 497.
   - `hopcyc=0` is neutral on the other 37 programs (geomean 1.003 dev, 1.000 corpus).
   - TSC ticks per cycle also differ between the two core types.
   - So any single-program verdict near the rule's threshold may be a coin flip. CONFIRMED (repro + knob ablation).
3. **An ablation can create the next blocker.** My first `oop_mixins` "no `table.remove`" variant replaced it with a shift loop.
   That loop then aborted `InnerLoop` at depth 2 and hid the effect. The loop-free variant (`__noremoveb`) shows the real (small) effect.
   Every ablation below was checked with VERBOSE=3 for this pattern only where noted.

## 1. Programs (32 new; MANIFEST has provenance and the split)

**(a) Real code: 4 programs.** The whole filesystem holds 151 `.lua` files outside the repo and `ref/`.
- Almost all are host-API scripts that cannot run without their host: wireplumber, gegl FFI, mpv, vlc, gimp.
- The only self-contained algorithmic Lua is the system LuaJIT's `jit/dis_*.lua` disassemblers (MIT).
- That LuaJIT ships them in an **extended dialect** (`!=`, `+=`, `?:`, `??`, 32-bit `~>>`), so none run unmodified on 5.5.
- They were **ported syntax-only**: mechanical desugaring, a `sar32` helper for `~>>`, one masked shift count.
- Equivalence evidence: the original dialect files under the system LuaJIT and the ports under PUC give **byte-identical** output
  for x86/x64, MIPS and PPC. ARM differs in 3 of 400 000 lines, LuaJIT's `-0`.
- So: "real, unmodified" is not available on this machine; "real, ported 1:1" is what was used.
- Programs: real_dis_x86 (x86 + x64), real_dis_arm (dev); real_dis_mips, real_dis_ppc (heldout). All four share one author and one
  driver pattern, so the heldout pair is weak held-out evidence.

**(b) Written: 28 programs.** 17 dev and 11 heldout, by two separate authors (subagents) with different style briefs. Neither author
saw the JIT or its docs.
- Dev topics: ECS game loop, particle physics, text adventure FSM, CSV report, word frequency and wrap, Levenshtein/LCS diff, regex NFA,
  Pratt expression compiler, luafun-style iterators, coroutine generators, class library with mixins, event emitter + promises,
  vararg utilities, gsub/format templating, bignum, Lua-subset parser, memoize/LRU.
- Heldout topics, in a procedural / closure-object / table-driven / prototype style: roguelike, Dijkstra, FSMs, log analyzer, Markov
  text, Scheme interpreter, Forth VM, bigint RSA, prototype OOP, closure streams, sudoku/queens/knapsack.
- Each program: 160-300 lines, deterministic LCG, 0.3-0.75 s on PUC, small-N mode for GC stress, <= 50 MB.
- All 32 match LuaJIT's output except real_dis_arm (the `-0` lines).

**Split.** dev = 19 (17 written + 2 real). heldout = 13 (11 written + 2 real), 59/41.
- Heldout was used only for timing, for one confirmation of a dev claim (stream_closures), and for confirming knob results.
- Its abort statistics went into the cross-program signature matrix (section 3) but did not drive any ablation choice.

## 2. Correctness: no divergence found

- **Mode matrix.** Every one of the 50 programs (18 corpus + 19 dev + 13 heldout) was compared with PUC under these modes:
  - JIT off;
  - tier-1 only (`TRACE=off T15=off`);
  - J4 (`TRACE=off`);
  - trace default;
  - `JIT=always TRACE=always`;
  - hot2 (`THRESHOLD=2 TRACE_HOT=2,2`);
  - no seed;
  - GC stress 1 (default JIT), stress 1 + always, and stress 2 + always, the stress modes at each program's small N.
- **Result: 500 runs, 0 diffs, 0 crashes, 0 timeouts** (`study/data/check_*.txt`).
- **Every timed run was also checked.** Each run's stdout was compared with `.expected`:
  - main tables: 3 x 74 programs x 7 runs, two core types;
  - 33 knob strings x 20 programs x 7 runs;
  - 10 knob strings x 35 programs;
  - all ablation variants.
- Ours always matched. Only LuaJIT differs, on msgpack_pure, testrunner_luaunit (as before) and real_dis_arm.
- The 4 real programs were also checked under the full mode matrix.

## 3. Timing (cycles:u min of 7 interleaved runs, Zen5 CPUs 4, 6; `study/data/mainZ5_table.md`, Zen5c in `mainZ5c_table.md`)

| set | n | ours/LJ geomean (min) | ours/LJ (median) | ours/PUC | programs > 1.5x LJ |
|---|---|---|---|---|---|
| **dev** (17 new + 2 real) | 19 | **1.624** (Zen5c 1.640) | 1.61 | 0.573 | 9 |
| **heldout** (11 new + 2 real) | 13 | **1.595** (Zen5c 1.617) | 1.60 | 0.533 | 6 |
| corpus (old 18, reference) | 18 | 1.494 (Zen5c 1.580) | 1.47 | 0.606 | 8 |
| dev-real / heldout-real | 2 / 2 | 1.357 / 1.625 | | 0.69 / 0.75 | 1 / 2 |

- **The new programs confirm the corpus picture.** About 1.6x LuaJIT and only about 1.8x faster than PUC, against our own sets'
  1.07-1.15x and 4.3-4.6x. Heldout is no better than dev.
- Worst programs:

  | program | ours/LJ | ours/PUC |
  |---|---|---|
  | dev iter_pipeline | 20.2 | 0.97 |
  | corpus class_middleclass | 8.2 | |
  | corpus classic_vector | 6.9 | |
  | heldout stream_closures | 4.6 | |
  | corpus lua_lexer | 3.1 | |
  | heldout scheme_interp | 2.5 | |
  | dev text_adventure | 2.4 | |
  | dev oop_mixins | 2.3 | |
  | dev diff_levenshtein | 2.2 | |
  | dev regex_nfa | 2.2 | |

- Faster than LuaJIT: event_promise 0.71, template_gsub 0.97, markdown_conv 0.52, sha2_base64 0.38.

Where the cycles go (perf, cycle-weighted; `study/data/cov_default.md`):

| set | trace | tier-1 | interp | helpers+exit | compile | other (library, alloc, tables, GC) |
|---|---|---|---|---|---|---|
| dev | 5.5% | 20.8% | 13.3% | 8.3% | 1.0% | 51.2% |
| heldout | 18.1% | 24.4% | 1.7% | 7.6% | 0.8% | 47.3% |
| corpus | 3.0% | 20.0% | 14.4% | 7.2% | 1.1% | 54.2% |

Runtime components over corpus + dev (`study/data/sym_groups.md`; regex classification of symbols, approximate):

| component | share of cycles |
|---|---|
| pattern matching | 12.2% |
| interpreter | 11.8% |
| tier-1/trace call helpers (`jit_call`, `ccall`, ...) | 5.3% |
| table growth (`newkey`, `resize`) | 4.8% |
| allocation | 4.7% |
| string creation | 4.6% |
| table-set slow path | 2.9% |
| metamethod paths | 2.0% |

Half of all cycles are outside JIT code. Trace coverage is the smaller lever on most of these programs.

## 4. Blocker re-ranking

**Method.**
- Abort and exit statistics: VERBOSE=3 + EXITDUMP for all 50 programs (`study/data/classes.md`: signature counts per program).
- Perf coverage per program.
- Then, per class, a minimal repro (`study/micro/`) and a source ablation with identical output (`study/var/`) or a knob ablation.
- Cycles: Zen5 CPUs 4/6, RUNS 9 (ablations) / 7 (knobs).
- **CONFIRMED** = repro + program-level ablation both show it. **UNVERIFIED** = signature or repro only.
- "Gap" = ours - LJ Mcyc. Totals: dev 10 471, corpus 7 625, heldout 8 514.

| rank | class | status | programs where an ablation moved it (cycles saved, share of that set's gap) | repro (ours var/orig; ours/LJ) | notes / what the claim rests on |
|---|---|---|---|---|---|
| 1 | **Class-library idioms in OO corpus code:** super calls through class-level `__index` functions + method families needing more than 16 side traces | CONFIRMED (corpus) | class_middleclass: no-super-index variant 0.337 (-1711 Mcyc); `maxside=24` 0.398 (-1564) = **20% of corpus gap**; also `polyside` 0.397, `fnroot=1` 0.415 | (poly_site micro: only 1.24x LJ at 4 classes) | One program. The same "MaxSide-blacklisted exits" signature appears in lua_lexer (2.3 M), regex_nfa, astar, real_dis_arm, scheme, and **none of them gains** from lifting the cap. dev oop_mixins does not gain either: the signature is not predictive |
| 2 | **Generic `for` over a Lua-closure iterator** (`Nyi(TFORCALL/TFORPREP)`; tier-1 calls the iterator through `jit_tforcall` -> `ccall` -> interpreter) | CONFIRMED | dev iter_pipeline: `for v in it` -> `while` loop **0.457 (-1489 Mcyc, 14% of dev gap**; ours/LJ 19.8 -> 9.5); heldout stream_closures (confirmation): 0.92 (-152, 2% of heldout gap) | tfor_lua 0.055 (ours/LJ 21.9 -> 1.19) | NEW. Huge on the dev program, small on the heldout one: the cause generalises, the magnitude does not. After the fix, iter_pipeline runs 93% in tier-1. The remainder is class 6 (`fnroot=1` 0.742 on the variant) |
| 3 | **Constructor idiom** `Class(...)` -> `__call(...)` -> `new` -> `init`, fresh table + new-key SETFIELDs | CONFIRMED (corpus) | classic_vector: table-constructor variant 0.350 (-1127, **15% of corpus gap**; ours/LJ 6.97 -> 3.27) | (alloc_short micro: short-lived plain tables already near LJ, 1.29x of a tiny loop) | One program; BUNDLE-N's N2 remainder. sinkpol=1 makes classic_vector +50% |
| 4 | **Coroutine bodies run interpreted** after resume (82% interpreter in the repro) | CONFIRMED | dev coro_generators: sieve as closures 0.598 (-539; LJ also -206: net gap -333 = **3% of dev gap**; ours/LJ 1.84 -> 1.54) | coro_body 0.287 (ours/LJ 4.77 -> 2.45) | Corpus coro_scheduler has the same signature (27% interp), not ablated. The ablation removes coroutines, so it also removes resume cost: an upper bound for "re-enter compiled code" |
| 5 | **Recursion unroll limit** (`unroll` 2) | CONFIRMED (knob ablation) | dev regex_nfa `unroll=3` 0.711 (-346, 3% of dev gap); heldout scheme_interp 0.862 (-356, 4% of heldout gap) | - | Hurts corpus astar_jumper / class_middleclass +7% (`unroll=8,maxdepth=8`: astar +37%). The fix is per site, not a global value |
| 6 | **Loops inside called functions / closures** (`InnerLoop` at depth >= 1; function-entry roots off) | CONFIRMED by knob on heldout and corpus, NOT on dev originals | `fnroot=1`: heldout stream_closures **0.501 (-941, 11% of heldout gap)**, corpus class_middleclass 0.415 (overlaps rank 1), iter_pipeline variant 0.742; dev programs: 0.992 on affected | callee_loop: inlined 0.677, but ours/LJ goes 2.60 -> 4.78 (LuaJIT gains more); nested_short 0.589 (ours/LJ 1.26 -> 0.76) | The micro says the callee loop costs us 1.5x but costs LuaJIT 2.7x, so it is not a relative gap by itself. The program-level wins are through function-entry roots. `fnroot` was rejected on own sets (ledger A1): needs a full-set A/B |
| 7 | **`__lt` / `__eq` metamethod comparisons** from compiled code (tier-1 exits at LT/EQ, N1(c)) | CONFIRMED (corpus), not on dev | astar_jumper: field compare 0.914 (-178); class_middleclass: no-`__lt` on top of rank 1, 871 -> 604 (-267); together **6% of corpus gap**; dev oop_mixins explicit-metamethod variant 1.019 (no gain) | mm_lt_loop 0.465 (ours/LJ 1.84 -> 0.98); sort_lt (comparator using `__lt`) 0.484 (2.15 -> 1.06) | The tier-1 exit "interpreter stay" numbers (astar: 864 Mcyc) overstate it about 5x: stays include nested work |
| 8 | **Library calls not recorded** (`table.remove(t,pos)`, `table.insert(t,pos,v)`, multi-arg `string.char`, `tonumber(s)`: Nyi CALL; find/match/gsub/format(non-`%s`)/rep/upper/lower/concat/unpack/sort/next/rawequal/gmatch: CFunc; probe table `study/probe/`) | CONFIRMED but small | dev oop_mixins loop-free no-`table.remove` 0.964 (-75, 0.7% of dev gap) | tremove1 0.643; strfind micro (find/match/gsub/format/tonumber) is **1.04x LJ**: no gap | The previous study's "5-15% on string-heavy programs" is not supported. The string-heavy and library-heavy programs (real_dis_*, csv, template, wordfreq) are 1.0-1.7x and spend about 70% in library code. What is left there is library speed (new_str, pattern matching), not trace coverage: UNVERIFIED as a JIT blocker |
| 9 | **Varargs** (root-frame `VARARG`, tail call into a vararg callee with `select`) | CONFIRMED in micro, small in programs | dev oop_mixins fixed-arity constructors 0.976 (-55) | tail_vararg: ours = PUC speed, 12.6x LJ | varargs_util is only 1.16x LJ; no slow program is dominated by it |
| - | **Closures created in loops** (`Nyi(CLOSURE)`) | REJECTED as a blocker | - | closure_loop: ours 0.81x LJ | Ours is faster than LuaJIT at it |
| - | **Polymorphic call sites at small arity** | not a blocker by itself | - | poly_site (4 classes): 1.24x LJ | Matters only through the family cap (rank 1) |
| - | **Short-lived allocation / sinking** | UNVERIFIED beyond rank 3 | sinkmt knobs: dev +-2% | alloc_short: 1.29x of a tiny loop | |
| - | **Unexplained** | | lua_lexer 3.1x, text_adventure 2.4x, oop_mixins 2.3x (56% interpreter; all edits together + maxside64: -11%), json_rxi 1.8x, lua_parser 1.8x, real_dis_* 1.6-1.7x | | |

**Previous study's hypotheses, as now judged.**

| previous claim | verdict | evidence |
|---|---|---|
| B1 metamethods | partly | the arithmetic/index part was fixed by N; `__lt` compare remains (rank 7) |
| B2 constructor | confirmed | rank 3 |
| B3 callee loops | real cost, not a relative gap in the micro | program wins only via `fnroot` (rank 6) |
| B4 coroutines | confirmed | rank 4 |
| B5 string library | not supported as ranked | strfind micro at parity |
| B6 closures | rejected | ours faster than LuaJIT |
| B7 tail calls / varargs | confirmed micro, small in programs | rank 9 |

**New classes:** generic for over a Lua iterator (rank 2), the recursion unroll limit (rank 5), non-deterministic TSC policy (section 0.2).

## 5. Ledger and retest (`docs/JIT_PARKED.md`)

**Ledger.** 76 entries plus one new one (`hopcyc`).
- A1-A29 are knob-gated; B1-B13 were built and reverted; C1-C34 were not built.
- Each entry gives what was tried, the sets and numbers, why it was rejected, an evidence-quality flag, a retest trigger and
  whether a cheap retest exists. The ledger was drafted by a subagent from docs + git; I spot-checked A5 and the `vargtr` claim.
- The ledger agent also found:
  - commit `fec75e2b` (BUNDLE-G1) deleted about 2100 lines of `docs/JIT_TRACE_REF.md` decision sections (P-J ... BUNDLE-F).
    They now exist only in git history; the ledger cites them as `REF0:`;
  - `vargtr` / `vargt1` are no-op placeholders: I confirmed `RecCfg::vargtr` is never read;
  - `fnloops=1` is a no-op while `recany=1`;
  - the docs are stale on lockstep (shipped off) and on BUNDLE-K item A (reverted).

**Retest.** 33 knob strings on dev, alone, against default.
- Positive on dev (>= 3% geomean on affected programs): `maxside=24` 0.923, `unroll=3` 0.909, `unroll=8,maxdepth=8` 0.915.
  Each is driven by **one** dev program.
- On heldout:
  - `unroll=3` confirms (scheme_interp 0.862, forth_vm 0.945) but regresses 2 corpus programs by 7%;
  - `maxside=24` does not move heldout (1.001; scheme_interp +5.5%).
- Not dev-positive, but big elsewhere:
  - `fnroot=1`: heldout 0.946 geomean (stream_closures 0.50), corpus 0.929 (middleclass 0.42);
  - `sinkpol=1`: stream_closures 0.52, but classic_vector +50%.
- Neutral or negative on dev: `polyside` (+2% on coro_generators). The uncommitted `POLYSIDE_PROCESS = 48` helps only middleclass.
- Overfitting risk is high: with 19 dev programs, single-program effects decide both "positive" and "neutral". No knob is a clean win.
  Any default change needs the full own sets (173) + 50 programs.

## 6. What each claim rests on, and what was not done

- **Timing:** one plain binary; no PGO. 7 interleaved runs; mins and medians agree within a few percent except prototype_objects (1.06 min / 1.12 median). Two core types, reported separately.
- **Ablations:** a single variant each, written by me. A variant can change more than the targeted pattern; the coroutine and
  `while`-loop variants also change LuaJIT's time, so the ours/LJ columns are reported next to ours alone. Only oop_mixins was
  re-checked for "next blocker" effects (section 0.3).
- **Program counts per class are small:** 1-2 programs per CONFIRMED class. The ranking by cycles at stake is dominated by
  middleclass, iter_pipeline, classic_vector and stream_closures.
- **Not done:**
  - a full-set (173 own programs) A/B of any knob;
  - PGO;
  - ablations for lua_lexer, text_adventure, json_rxi, lua_parser and real_dis_*;
  - Lua 5.4.
- **Bench data quality:** the first main run overlapped with a correctness check on CPUs 0-2 and 8-10, which share L3 with CPUs 4 and 6.
  It was discarded (`main_contaminated.json`) and rerun.
