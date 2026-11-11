# Final benchmark report, November 2026 (BUNDLE-F): rustynail against PUC Lua 5.5 and LuaJIT

Tree: BUNDLE-F (this branch, on top of BUNDLE-E cb3288c0). Same methodology as `docs/BENCH_REPORT_2026-11.md` (Bundle P): `cargo build --release -p rn-cli --features jit`
("plain") and `scripts/pgo-build.sh` ("PGO", regenerated on this tree: 181 training runs, 0 failed), rustc 1.99.1, AMD Ryzen AI 7 350 (Zen 5), Linux 7.3, THP `always`.
PUC = `ref/55/build-seed0/lua` (Lua 5.5.1, fixed string-hash seed), LuaJIT = `/usr/bin/luajit` 2.1 (the `bench/lj` variants where a program has one).

Method (unchanged): `scripts/bench-lj.sh` (`scripts/bench_lj.py`), `perf stat -e cycles:u,instructions:u`, `taskset -c 4`, every column interleaved per run,
**RUNS 11 for every column** (ours, PUC and LuaJIT), min and median of cycles:u, `RUSTYNAIL_SEED=0`, stdin `/dev/null`, memory capped.
Columns: `ref` (PUC), `lj`, `plain` and `pgo` (this tree), `pgo2` (byte copy of `pgo`: the duplicate column), `bplain` / `bpgo` (the BASE tree cb3288c0 = Bundle E,
plain and PGO-regenerated on the base, so before/after compares like with like; the Bundle P numbers come from the older tree 8c5f4603 and are shown for reference).
Binaries at equal-length paths (`~/.cache/bf{a..e}/rn-cli`). All 173 programs (awfy 14, apps 5, programs 25, jit-trace 29, lib 95, real 5; 169 with a LuaJIT time).
Every output of every `ours` column equals PUC's and `.expected` on all 173 programs (0 differences, plain, PGO, base).
**Default mode** = what users get; **warm=0** = `RUSTYNAIL_JIT_TRACE_LIMITS=warm=0` (ours columns only; ratios to PUC / LuaJIT come from the default pass).
Machine: quiet, load average 0.9-1.1 (the benchmark itself) for the whole of both passes, nothing else started (builds, tests and jit-diff all ran before or after).
The duplicate column agrees to 1.000 (cycles min and median, all 173), so there was no contamination. **One earlier complete pass was discarded and re-run**:
its numbers showed a +4..8% instruction regression on `lib/tremove_front`, `lib/tunpack`, `programs/table_array` (plain and PGO) which was a real code effect of the first
version of the change (`set_int` grew past the inlining threshold, so `jit_setlist` called it out of line); it was fixed (the hash-part half of `set_int` is out of line,
commit "keep set_int's array fast path inlinable"), the PGO build regenerated and the whole benchmark repeated. The numbers below are the second pass only.

## 0. What BUNDLE-F changed (exact: same node placement, sizes, `pairs` order and `#` borders as PUC)

`crates/rn-core/src/table.rs` only: (1) `resizearray` reallocs only blocks > 512 B, smaller arrays allocate + one copy + free (like PUC); (2) the empty dummy hash is
skipped: `set_int` does not probe it, `newkey` does not `insertkey` into it, `rehash` has a fast path for an integer key into the array range of a table without hash part
(`rehash_dummy_int`, the same counts as the general path), and `resize` has a fast path for pure array growth (5.5 only); (3) the `idiv` of `hashint`/`hashmod` is
a multiply-based exact modulo (Lemire fastmod, unit-tested against `%` for every node-size exponent, edge values and random inputs).

## 1. Headline

Geomean of per-program cycle ratios (lower = ours faster), default mode, all 173 programs (169 for LuaJIT). In parentheses: Bundle P.

| build | ours/PUC (min) | ours/PUC (median) | ours/LuaJIT (min) | ours/LuaJIT (median) | median, without the 5 noisiest |
|---|---|---|---|---|---|
| plain release | 0.250 (0.251) | 0.249 (0.251) | **1.241** (1.249) | 1.213 (1.229) | 1.188 (1.219) |
| **PGO release** | **0.233** (0.236) | 0.233 (0.236) | **1.157** (1.170) | 1.130 (1.150) | 1.106 (1.140) |

So ours is **4.3x faster than the reference interpreter overall (PGO) and within 16% (geomean) of LuaJIT** (Bundle P: 4.2x, 17%); ours is faster than LuaJIT on 59 of 169 programs.
(5 noisiest programs: awfy/permute 19.2 (LuaJIT is bimodal: min 13.5, median 260 Mcyc this time), apps/events 1.49, lib/fmt_s 1.10, awfy/queens 1.09, lib/char 1.09.)

### Per set, PGO build, default mode

| set (n) | ours/PUC min | ours/PUC median | ours/LJ min (n with LJ) | ours/LJ median | ours/LJ median w/o 5 noisiest | Bundle P: ours/LJ min |
|---|---|---|---|---|---|---|
| awfy (14) | 0.317 | 0.316 | 1.660 (14) | 1.327 | 1.456 | 1.681 |
| apps (5) | 0.523 | 0.526 | 1.332 (5) | 1.204 | 1.268 | 1.426 |
| programs (25) | 0.227 | 0.227 | 0.943 (25) | 0.942 | 0.942 | 0.948 |
| jit-trace (29) | 0.085 | 0.084 | 1.065 (29) | 1.064 | 1.064 | 1.071 |
| lib (95) | 0.282 | 0.281 | 1.172 (91) | 1.169 | 1.114 | 1.186 |
| real (5) | 0.477 | 0.478 | 1.293 (5) | 1.272 | 1.272 | 1.298 |
| **all** (173) | 0.233 | 0.233 | 1.157 (169) | 1.130 | 1.106 | 1.170 |

### Per set, plain build, default mode

| set (n) | ours/PUC min | ours/PUC median | ours/LJ min (n with LJ) | ours/LJ median | ours/LJ median w/o 5 noisiest | Bundle P: ours/LJ min |
|---|---|---|---|---|---|---|
| awfy (14) | 0.331 | 0.331 | 1.734 (14) | 1.389 | 1.531 | 1.752 |
| apps (5) | 0.589 | 0.591 | 1.499 (5) | 1.352 | 1.416 | 1.587 |
| programs (25) | 0.244 | 0.244 | 1.014 (25) | 1.012 | 1.012 | 1.018 |
| jit-trace (29) | 0.086 | 0.085 | 1.077 (29) | 1.075 | 1.075 | 1.083 |
| lib (95) | 0.307 | 0.306 | 1.279 (91) | 1.277 | 1.218 | 1.284 |
| real (5) | 0.542 | 0.543 | 1.468 (5) | 1.444 | 1.444 | 1.465 |
| **all** (173) | 0.250 | 0.249 | 1.241 (169) | 1.213 | 1.188 | 1.249 |

The apps ratio moves more than the ours-side change explains (1.587 -> 1.499 plain): LuaJIT's `events` / `lru_cache` times are noisy (events LJ 110.3 Mcyc here, 95.3 in Bundle P).
Compare ours against ours (section 2) for the effect of the change; compare against LuaJIT only with the median columns for permute / events / lru_cache.

### warm=0 (A/B convention), geomeans, this tree

| build | awfy ours/PUC, /LJ min | apps | programs | jit-trace | lib | real | all, /LJ min |
|---|---|---|---|---|---|---|---|
| plain | 0.332, 1.738 | 0.582, 1.482 | 0.245, 1.019 | 0.086, 1.077 | 0.307, 1.283 | 0.527, 1.429 | 0.250, 1.243 |
| PGO | 0.317, 1.661 | 0.523, 1.333 | 0.228, 0.948 | 0.085, 1.065 | 0.283, 1.175 | 0.468, 1.270 | 0.234, 1.159 |

warm=0 and default mode agree within 0.005 per set. Full tables: `docs/bench-bf-tables-{default,warm0}-{plain,pgo}.md`; raw data `docs/bench-bf-{default,warm0}-<set>.{json,txt,err}`.

## 2. Before / after (this tree against the base cb3288c0, each built the same way, default mode, min of 11)

Geomean over each set (ratio < 1 = faster than the base), cycles min / cycles median / instructions:

| set (n) | plain: cycles min | median | instr | PGO: cycles min | median | instr | worst program (plain / PGO, cycles min) | best program (plain / PGO) |
|---|---|---|---|---|---|---|---|---|
| awfy (14) | 0.982 | 0.982 | 0.979 | 0.983 | 0.982 | 0.980 | mandelbrot 1.000 / nbody 1.007 | storage 0.863 / 0.857 |
| apps (5) | 0.983 | 0.982 | 0.982 | 0.990 | 0.994 | 0.989 | raytrace 0.991 / lru_cache 1.013 | expr_lang 0.969 / events 0.972 |
| programs (25) | 0.994 | 0.995 | 0.994 | 0.996 | 0.993 | 0.993 | binary_trees 1.031 / closures 1.032 | pcall_error 0.927 / gc_churn 0.959 |
| jit-trace (29) | 0.999 | 1.002 | 0.999 | 0.998 | 1.001 | 1.000 | alloc_ctor 1.019 / rec_loop 1.007 | field_global 0.988 / 0.962 |
| lib (95) | 0.995 | 0.994 | 0.994 | 0.986 | 0.986 | 0.993 | load_small 1.043 / pcall_err 1.088 | tinsert_idx 0.914 / pcall_ok 0.859 |
| real (5) | 0.998 | 1.001 | 0.995 | 0.989 | 0.992 | 0.998 | template_engine 1.029 / wordfreq 1.004 | lua_interp 0.984 / template_engine 0.966 |
| **all (173)** | **0.994** | 0.994 | 0.994 | **0.990** | 0.989 | 0.993 | | |

(duplicate column pgo2 / pgo: 1.000 cycles min on all 173 programs, 1.000 median, instructions 1.000.)
**awfy -1.8% / apps -1.7% (plain), awfy -1.7% / apps -1.0% (PGO)**; all 173: -0.6% plain, -1.0% PGO. The prototype measured -2.2% / -1.6% on awfy+apps; the difference is `set_int`
inlining (see above) and the PGO profile regenerated on both sides. The win is concentrated where a program grows many small arrays: **storage -14%**, havlak -3..-4.5%, json -2..-3%,
events -3%, expr_lang -2..-3%; everything else within +-1%.

### awfy + apps, per program: before (base) -> after, cycles:u min of 11 (Mcyc), and ratio after/before for cycles min, median and instructions

PGO build:

| program | base Mcyc | new Mcyc | cycles min | cycles median | instructions |
|---|---|---|---|---|---|
| awfy/bounce | 75.5 | 75.3 | 0.997 | 0.992 | 0.999 |
| awfy/cd | 256.7 | 255.9 | 0.997 | 0.995 | 0.998 |
| awfy/deltablue | 101.5 | 101.0 | 0.995 | 0.995 | 0.995 |
| awfy/havlak | 143.7 | 137.2 | 0.955 | 0.957 | 0.970 |
| awfy/json | 143.3 | 138.8 | 0.968 | 0.967 | 0.967 |
| awfy/list | 140.8 | 140.6 | 0.999 | 1.000 | 1.000 |
| awfy/mandelbrot | 276.9 | 277.0 | 1.000 | 1.001 | 1.000 |
| awfy/nbody | 150.6 | 151.6 | 1.007 | 1.004 | 1.000 |
| awfy/permute | 66.7 | 66.5 | 0.998 | 0.995 | 1.000 |
| awfy/queens | 132.9 | 133.7 | 1.006 | 1.004 | 0.992 |
| awfy/richards | 67.2 | 67.2 | 1.000 | 0.998 | 1.000 |
| awfy/sieve | 134.2 | 133.8 | 0.997 | 0.997 | 1.000 |
| awfy/storage | 126.0 | 108.1 | **0.857** | 0.858 | 0.817 |
| awfy/towers | 97.7 | 97.9 | 1.002 | 1.001 | 1.002 |
| **awfy geomean (14)** | | | **0.983** | 0.982 | 0.980 |
| apps/events | 165.6 | 160.9 | 0.972 | 0.966 | 0.960 |
| apps/expr_lang | 133.8 | 130.6 | 0.976 | 0.981 | 0.983 |
| apps/lru_cache | 166.7 | 168.9 | 1.013 | 1.015 | 1.003 |
| apps/markdown | 146.5 | 145.0 | 0.989 | 1.006 | 0.997 |
| apps/raytrace | 131.9 | 131.8 | 0.999 | 1.001 | 1.002 |
| **apps geomean (5)** | | | **0.990** | 0.994 | 0.989 |

Plain build:

| program | base Mcyc | new Mcyc | cycles min | cycles median | instructions |
|---|---|---|---|---|---|
| awfy/bounce | 76.5 | 76.4 | 0.999 | 0.991 | 0.999 |
| awfy/cd | 277.9 | 276.0 | 0.993 | 0.995 | 0.997 |
| awfy/deltablue | 107.6 | 107.3 | 0.997 | 0.995 | 0.992 |
| awfy/havlak | 159.7 | 155.6 | 0.974 | 0.973 | 0.962 |
| awfy/json | 161.9 | 158.4 | 0.978 | 0.976 | 0.967 |
| awfy/list | 144.4 | 143.9 | 0.997 | 0.999 | 1.000 |
| awfy/mandelbrot | 277.2 | 277.2 | 1.000 | 1.000 | 1.000 |
| awfy/nbody | 152.1 | 151.9 | 0.999 | 1.005 | 1.000 |
| awfy/permute | 66.8 | 66.6 | 0.996 | 0.996 | 1.000 |
| awfy/queens | 138.5 | 136.0 | 0.982 | 0.988 | 0.991 |
| awfy/richards | 68.5 | 68.3 | 0.997 | 0.990 | 1.000 |
| awfy/sieve | 134.5 | 134.1 | 0.997 | 1.000 | 1.000 |
| awfy/storage | 142.8 | 123.2 | **0.863** | 0.864 | 0.817 |
| awfy/towers | 100.2 | 98.8 | 0.986 | 0.985 | 1.002 |
| **awfy geomean (14)** | | | **0.982** | 0.982 | 0.979 |
| apps/events | 187.7 | 182.8 | 0.974 | 0.974 | 0.940 |
| apps/expr_lang | 152.2 | 147.6 | 0.969 | 0.973 | 0.976 |
| apps/lru_cache | 188.6 | 186.5 | 0.989 | 0.999 | 1.000 |
| apps/markdown | 160.0 | 158.3 | 0.990 | 0.985 | 0.996 |
| apps/raytrace | 155.1 | 153.8 | 0.991 | 0.978 | 0.999 |
| **apps geomean (5)** | | | **0.983** | 0.982 | 0.982 |

### The 5% rule (per program, all 173)

**No program is more than 5% worse in instructions** (the worst instruction ratios on all 173 programs: plain `programs/table_hash` +0.7%, `lib/tunpack` +0.7%; PGO `lib/tmove` +1.3%, `lib/char` +1.2%, `lib/len_tbl` +1.1%). Programs more than 5% worse in cycles min:
plain: none (worst `lib/load_small` +4.3%, `lib/fmt_mixed` +4.0%, `programs/binary_trees` +3.1%); PGO: `lib/pcall_err` +8.8%, `lib/rep_sep` +6.5%, `lib/load_small` +6.2%
(also `programs/pcall_error` +5.2% at warm=0). All of them have instructions within +-0.3% and unchanged branch-miss counts; they are not table code. Investigated: `lib/pcall_err` PGO,
base vs new, `perf stat` min of 7: cycles 181.7 -> 194.8 M, instructions 739.6 -> 738.8 M (equal), branch-misses 71 k -> 68 k, **L1 icache load misses 18 k -> 36 k**
(`lib/load_small`: 388 k -> 883 k). A front-end layout effect of the regenerated PGO profile (code placement of the call / pcall / load machinery), the same class as Bundle P's
`pcall_ok` +35% PGO artefact; the plain build (same source) shows none of them. Not a table-runtime effect; left alone (no knob to pin PGO layout). The same profile regeneration also
gave `lib/pcall_ok` -14% and `lib/tinsert_front` -13.5%.

## 3. awfy and apps, per program (PGO build, default mode; Mcyc = cycles:u / 1e6, min of 11)

| program | ours Mcyc | PUC Mcyc | LJ Mcyc | ours/PUC | ours/LJ min | ours/LJ median |
|---|---|---|---|---|---|---|
| bounce | 75.3 | 515.7 | 47.9 | 0.146 | 1.57 | 1.56 |
| cd | 255.9 | 436.1 | 107.6 | 0.587 | 2.38 | 2.32 |
| deltablue | 101.0 | 158.0 | 103.5 | 0.639 | 0.98 | 0.97 |
| havlak | 137.2 | 218.2 | 116.3 | 0.629 | 1.18 | 1.18 |
| json | 138.8 | 264.7 | 73.7 | 0.524 | 1.88 | 1.87 |
| list | 140.6 | 418.6 | 88.6 | 0.336 | 1.59 | 1.58 |
| mandelbrot | 277.0 | 1313.3 | 272.5 | 0.211 | 1.02 | 1.02 |
| nbody | 151.6 | 1517.2 | 95.1 | 0.100 | 1.59 | 1.60 |
| permute | 66.5 | 246.4 | 13.5 | 0.270 | 4.94 | **0.26** |
| queens | 133.7 | 393.9 | 54.8 | 0.339 | 2.44 | 2.25 |
| richards | 67.2 | 195.7 | 35.1 | 0.343 | 1.91 | 1.91 |
| sieve | 133.8 | 1029.6 | 156.5 | 0.130 | 0.85 | 0.86 |
| storage | 108.1 | 250.5 | 153.5 | 0.431 | **0.70** | 0.70 |
| towers | 97.9 | 215.0 | 23.5 | 0.455 | 4.17 | 3.96 |
| events | 160.9 | 293.7 | 110.3 | 0.548 | 1.46 | **0.98** |
| expr_lang | 130.6 | 243.6 | 96.1 | 0.536 | 1.36 | 1.31 |
| lru_cache | 168.9 | 220.7 | 117.5 | 0.765 | 1.44 | 1.35 |
| markdown | 145.0 | 418.1 | 320.0 | 0.347 | 0.45 | 0.46 |
| raytrace | 131.8 | 262.6 | 40.6 | 0.502 | 3.25 | 3.20 |

Bold = min and median differ a lot (LuaJIT `permute` is bimodal: 13.5 Mcyc fast mode, ~260 slow mode; `events` has a noisy LuaJIT). The plain-build rows are in
`docs/bench-bf-tables-default-plain.md` / `scripts/bench_bp_report.py rows default <set> plain` (BENCH_PREFIX=bf), bench/real rows (PGO, ours/PUC, ours/LJ min): game_loop 0.187 / 1.79,
json_roundtrip 0.648 / 1.10, lua_interp 0.542 / 2.05, template_engine 0.571 / 0.74, wordfreq 0.660 / 1.21.

Comparison with Bundle P (PGO, ours/LJ min, awfy + apps programs that moved): storage 0.83 -> **0.70**, havlak 1.24 -> 1.18, json 1.98 -> 1.88, events 1.75 -> 1.46 (noisy), expr_lang 1.40 -> 1.36;
the others are inside +-2% (the LuaJIT denominators moved by the same amount between the two runs).

## 4. Top remaining ratios (PGO, default mode) and why

33 of 169 programs have a min ratio over 2 (31 by median; plain: 39 / 38), almost all `lib` micro-rows (single-library-call loops). Largest over all 169:

| rank | program | min | median | likely reason |
|---|---|---|---|---|
| 1 | lib/fmt_s | 51.6 | 47.1 | not a real gap: LuaJIT folds `string.format("%s", s)` to `s`; ours pays the C-function call |
| 2 | lib/str_eq_long | 7.2 | 7.2 | string representation: LuaJIT interns all strings (pointer compare), PUC-style long strings compare bytes (by design) |
| 3 | awfy/permute | 4.9 | 0.26 | measurement artefact of LuaJIT's bimodality; against its slow mode we are 4x faster |
| 4 | lib/pcall_ok | 4.5 | 4.5 | `pcall` is a rolled loop in LuaJIT and a call-machinery path here, never in a trace (PGO is now 14% better than Bundle P's profile on it: 4.54 vs 5.3) |
| 5 | awfy/towers | 4.2 | 4.0 | tier-1 call/return frame cost (Bundle D: ceiling under the gate) plus integer key 0 in the hash part of `piles` (cannot be fixed by array sizing: LuaJIT has slot 0, a 1-based array part cannot) |
| 6 | jit-trace/rec_loop | 4.0 | 3.8 | trace recursion cost (6 vs 18-20 cycles per call) |
| 7 | lib/it_next_empty | 3.6 | 3.6 | `next` on an empty table: C-function call vs LuaJIT inlined `next` |
| 8 | lib/find_plain | 3.5 | 3.5 | `string.find` plain: library call vs LuaJIT's fast path |
| 9 | apps/raytrace | 3.25 | 3.20 | allocation / GC of short-lived vectors and compile cost (16%); sinking is built but default off (cd +7%) |
| 10 | lib/sub | 3.1 | 3.1 | `string.sub` call overhead (LuaJIT interns the result inline) |
| 11 | programs/fib | 3.0 | 3.0 | trace-code recursion cost (backend), 100% in traces |
| 12 | jit-trace/tail_loop | 2.7 | 2.7 | tail-call loop in traces |
| 15 | awfy/queens | 2.4 | 2.3 | exits and tier-1 share (side traces capped) |
| - | awfy/cd | 2.4 | 2.3 | compile cost 13%, table-allocation runtime |

Among real programs (awfy, apps, programs, real) the largest are towers 4.17, raytrace 3.25, fib 3.03, queens 2.44, cd 2.38, lua_interp 2.05 (unchanged from Bundle P: BUNDLE-F touched table runtime only).

## 5. PGO against plain (all 173 programs, default mode, this tree)

| set (n) | pgo/plain cycles min | median | instructions | worst program | best program |
|---|---|---|---|---|---|
| awfy (14) | 0.957 | 0.956 | 0.961 | mandelbrot 0.999 | json 0.876 |
| apps (5) | 0.889 | 0.890 | 0.928 | markdown 0.915 | raytrace 0.857 |
| programs (25) | 0.930 | 0.930 | 0.927 | sieve 1.057 | json 0.786 |
| jit-trace (29) | 0.989 | 0.990 | 0.991 | inline_leaf 1.006 | alloc_ctor 0.863 |
| lib (95) | 0.921 | 0.920 | 0.916 | str_eq_long 1.200 | tsort_str 0.600 |
| real (5) | 0.881 | 0.881 | 0.902 | game_loop 0.979 | template_engine 0.811 |
| all (173) | 0.934 | 0.934 | 0.933 | | |

(Bundle P: 0.939. `programs/sieve` +5.7% and `lib/str_eq_long` +20% are PGO layout regressions, as before.)

## 6. Verification

* PGO build: `scripts/pgo-build.sh ~/.cache/bf-pgo-new`, training 181 runs 0 failed. jit-diff seeds 1..40, hook none, modes `off tr trdef`, `JITDIFF_ENTERS=1`, **with the PGO binary: 0 diffs**
  (`docs/bench-bf-jitdiff-pgo.txt`); `scripts/jit-official.sh` with `RUSTYNAIL_JIT=always` and default: `JIT-OFFICIAL OK` (also for the 5.4 build).
* Test suite, jit-diff, 5.4: see the BUNDLE-F section of `docs/JIT_TRACE_REF.md`.

## 7. What is not covered

Lua 5.4 is not benchmarked (its table code is untouched: the new fast paths are `cfg`-gated to 5.5), full mutant runners, ASan / Miri, fuzzing. Medians over RUNS 11 of a single machine; `lib` rows
are single-call micro-benchmarks that weigh as much as whole programs in the geomeans. PGO speedups are on benchmark sets that are in the training list.
