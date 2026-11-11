# Benchmark report, November 2026 (BUNDLE-H3): rustynail against PUC Lua 5.5 and LuaJIT

Tree: BUNDLE-H3 (on top of BUNDLE-H2 684bc9e1). Same methodology as `docs/BENCH_REPORT_2026-11-final.md` (Bundle F): `cargo build --release -p rn-cli --features jit` ("plain") and
`scripts/pgo-build.sh` ("PGO", regenerated on this tree: 181 training runs, 0 failed), rustc 1.99.1, AMD Ryzen AI 7 350 (Zen 5), Linux 7.3.
PUC = `ref/55/build-seed0/lua` (Lua 5.5.1, fixed string-hash seed), LuaJIT = `/usr/bin/luajit` 2.1 (the `bench/lj` variants where a program has one).

Method: `scripts/bench-lj.sh` (`scripts/bench_lj.py`), `perf stat -e cycles:u,instructions:u`, `taskset -c 4`, every column interleaved per run, **RUNS 11 for every column**
(ours, PUC and LuaJIT), min and median of cycles:u, `RUSTYNAIL_SEED=0`, stdin `/dev/null`, memory capped, nothing else running (builds, tests and jit-diff ran before or after;
load average 1.0 for both passes). Columns: `ref` (PUC), `lj`, `plain` and `pgo` (this tree), `pgo2` (byte copy of `pgo`: the duplicate column), `bplain` / `bpgo` (the BASE tree
684bc9e1 = BUNDLE-H2, plain and PGO regenerated on the base, so before/after compares like with like). Binaries at equal-length paths (`~/.cache/hz{a..e}/rn-cli`).
All 173 programs (awfy 14, apps 5, programs 25, jit-trace 29, lib 95, real 5; 169 with a LuaJIT time). The driver is `scripts/bench_lj.py` with `~/.cache/finalbench.sh`'s column list
(see "Reproduce" at the end); the report tables come from `scripts/bench_h3_report.py` and `scripts/bench_bp_report.py` (`BENCH_PREFIX=h3`).
**Default mode** = what users get; **warm=0** = `RUSTYNAIL_JIT_TRACE_LIMITS=warm=0` (ours columns only, A/B convention; ratios to PUC / LuaJIT come from the default pass).
Every output of every ours column equals PUC's / `.expected` (bench_lj.py `same`/`expected` fields; no MISMATCH in the .err files).

**Contamination check.** The duplicate column `pgo2` agrees with `pgo` to a geomean of 0.9998 (cycles min, 173 programs), instructions 1.000 on every program. Individual lib rows of two
byte-identical binaries at different paths differ by 3-6% (code alignment / PGO layout: `lib/error_tbl` 0.939, `lib/upper` 0.945, `lib/tostring_tbl` 1.040, `lib/concat_op_str` 1.025,
`jit-trace/sink_vec` 1.029); that is the noise floor of a single lib row and is why the 5% rule below is judged on instructions as well. No pass was discarded. (During development a
first A/B of a `setmetatable` fast path moved `raytrace` by +14% cycles / +27% instructions through the trace set, see the H3 section of `docs/JIT_TRACE_REF.md`; that item was
dropped and none of its code is in the benchmarked binaries.)

## 0. What BUNDLE-H3 changed

Shipped (`docs/JIT_TRACE_REF.md`, "BUNDLE-H3 decisions and results"):

* **A. Blacklisted exits are not reported to the exit handler any more.** The frame-materialising inline stub (H2 `fmat`) of an unattached frameful exit diverted every 8th exit to the
  handler (~284 cycles per call), including exits whose side-trace state is `Black` (94% of the handler calls on queens). A blacklisted exit now carries a flag bit in its counter
  (`EXIT_BLACK_BIT`, masked in the stub's periodic test; every reader of the counters masks it out).
* **F. `HREFK` compares the 64-bit key address with one `cmp [node + KEY], reg`** instead of two 32-bit memory-immediate compares (`movabs v7, key; cmp [m], v7; jne`: 2 micro-ops instead of
  4); new micro-op `BrMR` (encoder, emulator, differential test), knob `hk64` (process default 1).

Sized and not shipped: B (link entry), C (late side traces; compact stubs), D (tier-1 `setmetatable` path), E (trace BUDGET compare), G (per-site sinking policy), the rest of F (lea folding,
back-edge moves, spills). Numbers in section 6 and in the decisions section of `docs/JIT_TRACE_REF.md`; patches in `docs/bench-h3-*.patch`.

## 1. Headline

Geomean of per-program cycle ratios (lower = ours faster), default mode, all 173 programs (169 for LuaJIT). In parentheses: Bundle F (`docs/BENCH_REPORT_2026-11-final.md`).

| build | ours/PUC (min) | ours/PUC (median) | ours/LuaJIT (min) | ours/LuaJIT (median) | median, without the 5 noisiest |
|---|---|---|---|---|---|
| plain release | 0.238 (0.250) | 0.237 (0.249) | **1.181** (1.241) | 1.154 (1.213) | 1.156 (1.188) |
| **PGO release** | **0.222** (0.233) | 0.221 (0.233) | **1.097** (1.157) | 1.072 (1.130) | 1.071 (1.106) |

Ours is **4.5x faster than the reference interpreter overall (PGO) and within 9.7% (geomean) of LuaJIT** (Bundle F: 4.3x, 15.7%).
**Ours is faster than LuaJIT on 62 of 169 programs by min and 62 by median (PGO); plain 54 by min, 55 by median** (Bundle F, PGO: 59 of 169).
(5 noisiest programs: awfy/permute 19.8 (LuaJIT is bimodal: min 14.1, median 270 Mcyc), lib/fmt_s 1.18, apps/lru_cache 1.13, apps/events 1.12, programs/fib 1.11.)

| faster than LuaJIT | awfy (14) | apps (5) | programs (25) | jit-trace (29) | lib (91) | real (5) | all (169) |
|---|---|---|---|---|---|---|---|
| PGO, min / median | 3 / 4 | 1 / 1 | 11 / 11 | 9 / 9 | 37 / 36 | 1 / 1 | **62 / 62** |
| plain, min / median | 2 / 3 | 1 / 1 | 11 / 11 | 9 / 9 | 30 / 30 | 1 / 1 | 54 / 55 |

### Per set, PGO build, default mode

| set (n) | ours/PUC min | ours/PUC median | ours/LJ min (n with LJ) | ours/LJ median | ours/LJ median w/o 5 noisiest | Bundle F: ours/LJ min | BUNDLE-H2 (awfy, apps rows): ours/LJ min / median |
|---|---|---|---|---|---|---|---|
| awfy (14) | 0.273 | 0.272 | 1.441 (14) | 1.156 | 1.329 | 1.660 | 1.495 / 1.189 |
| apps (5) | 0.524 | 0.527 | 1.268 (5) | 1.203 | 1.236 | 1.332 | 1.415 / 1.240 |
| programs (25) | 0.222 | 0.222 | 0.930 (25) | 0.929 | 0.881 | 0.943 | |
| jit-trace (29) | 0.085 | 0.084 | 1.066 (29) | 1.062 | 1.062 | 1.065 | |
| lib (95) | 0.265 | 0.264 | 1.093 (91) | 1.090 | 1.082 | 1.172 | |
| real (5) | 0.466 | 0.465 | 1.255 (5) | 1.238 | 1.238 | 1.293 | |
| **all** (173) | 0.222 | 0.221 | 1.097 (169) | 1.072 | 1.071 | 1.157 | |

(The apps min ratio moved 1.415 -> 1.268 mostly through the LuaJIT denominator: `events` LuaJIT min is 142 Mcyc here and 110 in the H2 run (bimodal), `lru_cache` likewise; ours moved -1.1%. The median column
is the comparable one: 1.240 -> 1.203.)

### Per set, plain build, default mode

| set (n) | ours/PUC min | ours/PUC median | ours/LJ min (n with LJ) | ours/LJ median | ours/LJ median w/o 5 noisiest | Bundle F: ours/LJ min |
|---|---|---|---|---|---|---|
| awfy (14) | 0.286 | 0.286 | 1.509 (14) | 1.212 | 1.398 | 1.734 |
| apps (5) | 0.586 | 0.590 | 1.420 (5) | 1.348 | 1.410 | 1.499 |
| programs (25) | 0.239 | 0.238 | 0.999 (25) | 0.995 | 0.995 | 1.014 |
| jit-trace (29) | 0.086 | 0.085 | 1.076 (29) | 1.073 | 1.073 | 1.077 |
| lib (95) | 0.290 | 0.289 | 1.203 (91) | 1.199 | 1.182 | 1.279 |
| real (5) | 0.523 | 0.522 | 1.409 (5) | 1.388 | 1.388 | 1.468 |
| **all** (173) | 0.238 | 0.237 | 1.181 (169) | 1.154 | 1.156 | 1.241 |

Most of the difference to Bundle F is not this bundle: BUNDLE-G1/G2/H1/H2 (exits, inlined frames, policy, `fmat`) landed in between. This bundle against its direct base (BUNDLE-H2, same run):

### Before / after: this tree against BUNDLE-H2 (684bc9e1), each built the same way, default mode, min of 11

| set (n) | plain: cycles min | median | instr | PGO: cycles min | median | instr | worst program (plain / PGO) | best program (plain / PGO) |
|---|---|---|---|---|---|---|---|---|
| awfy (14) | 0.968 | 0.968 | 0.981 | 0.966 | 0.964 | 0.980 | havlak 1.011 / sieve 1.002 | queens 0.923 / bounce 0.919 |
| apps (5) | 0.986 | 0.989 | 0.997 | 0.989 | 0.987 | 0.996 | lru_cache 1.012 / markdown 1.012 | markdown 0.959 / raytrace 0.967 |
| programs (25) | 0.998 | 0.999 | 0.997 | 0.998 | 1.001 | 0.997 | oop_methods 1.028 / pcall_error 1.054 | metamethods 0.949 / 0.925 |
| jit-trace (29) | 0.999 | 0.998 | 0.999 | 0.997 | 0.997 | 0.999 | sink_vec 1.020 / sink_tuple 1.012 | oop_override 0.950 / 0.962 |
| lib (95) | 1.000 | 1.000 | 1.000 | 0.998 | 0.999 | 1.000 | co_wrap 1.089 / load_small 1.071 | string_key_const 0.913 / 0.910 |
| real (5) | 0.982 | 0.982 | 0.993 | 0.987 | 0.988 | 0.993 | json_roundtrip 1.010 / template_engine 1.013 | game_loop 0.957 / 0.960 |
| **all (173)** | **0.996** | 0.996 | 0.997 | **0.995** | 0.995 | 0.997 | | |

warm=0 (A/B convention, same binaries): PGO awfy 0.969 / apps 0.989 / all 0.995; the plain numbers agree within 0.005 (`docs/bench-h3-tables-ba.md`).

**The 5% rule.** No program is more than 5% worse in instructions (worst instruction ratio on all 173: 1.001). Programs more than 5% worse in cycles with unchanged instructions (layout; the duplicate
column shows the same size of difference between identical binaries): PGO `programs/pcall_error` 1.054, `lib/error_tbl` 1.055, `lib/load_small` 1.071, `lib/upper` 1.054; plain `lib/co_wrap` 1.089,
`lib/m_type` 1.084. All have instruction ratios 0.999-1.001 and are library micro-rows (`pcall`, `error`, `load`, `string.upper`, coroutine wrap, `math.type`) that no code of this bundle executes.

### awfy + apps targets, per program, this tree against BUNDLE-H2 (cycles min / median, instructions)

| program | PGO min | PGO median | PGO Mcyc (H2 -> H3) | plain min | plain median | plain Mcyc (H2 -> H3) | instructions (PGO) |
|---|---|---|---|---|---|---|---|
| awfy/queens | 0.927 | 0.926 | 114.4 -> 106.1 | 0.923 | 0.920 | 116.8 -> 107.7 | 0.962 |
| awfy/towers | 0.935 | 0.936 | 47.5 -> 44.4 | 0.936 | 0.932 | 48.3 -> 45.2 | 0.957 |
| awfy/cd | 0.962 | 0.955 | 232.0 -> 223.1 | 0.966 | 0.963 | 250.5 -> 241.9 | 0.972 |
| apps/raytrace | 0.967 | 0.960 | 133.1 -> 128.7 | 0.984 | 0.985 | 154.9 -> 152.4 | 0.988 |
| awfy/havlak | 0.994 | 0.990 | 138.6 -> 137.7 | 1.011 | 1.010 | 154.7 -> 156.4 | 0.998 |
| awfy/json | 0.988 | 0.984 | 141.1 -> 139.4 | 0.997 | 1.002 | 161.0 -> 160.6 | 0.994 |
| awfy/richards | 0.931 | 0.933 | 58.3 -> 54.3 | 0.934 | 0.936 | 59.5 -> 55.5 | 0.960 |
| awfy/nbody | 0.955 | 0.954 | 137.3 -> 131.1 | 0.956 | 0.961 | 137.0 -> 131.0 | 0.960 |
| awfy/bounce | 0.919 | 0.912 | 73.0 -> 67.1 | 0.924 | 0.922 | 73.9 -> 68.3 | 0.963 |
| awfy/deltablue | 0.993 | 0.988 | 99.1 -> 98.4 | 0.990 | 0.993 | 105.7 -> 104.6 | 0.990 |
| awfy/list | 0.968 | 0.966 | 140.3 -> 135.8 | 0.966 | 0.963 | 143.8 -> 138.8 | 0.988 |
| awfy/permute | 0.965 | 0.963 | 54.0 -> 52.1 | 0.966 | 0.967 | 54.3 -> 52.4 | 0.985 |
| **geomean, A/B/C targets (queens towers cd raytrace havlak json richards)** | **0.957** | | | **0.964** | | | |
| **geomean, F targets (nbody deltablue json richards havlak bounce)** | **0.963** | | | **0.968** | | | |

(awfy geomean 0.966 PGO / 0.968 plain; apps 0.989 / 0.986; `docs/bench-h3-tables-ba.md` has every awfy + apps row for both builds and both modes.)

## 2. awfy and apps, per program (PGO build, default mode; Mcyc = cycles:u / 1e6, min of 11)

| program | ours Mcyc | PUC Mcyc | LJ Mcyc | ours/PUC | ours/LJ min | ours/LJ median |
|---|---|---|---|---|---|---|
| bounce | 67.1 | 519.9 | 47.9 | 0.129 | 1.40 | 1.40 |
| cd | 223.1 | 436.7 | 108.1 | 0.511 | 2.06 | 2.05 |
| deltablue | 98.4 | 158.2 | 102.8 | 0.622 | 0.96 | 0.95 |
| havlak | 137.7 | 218.2 | 116.0 | 0.631 | 1.19 | 1.18 |
| json | 139.4 | 265.0 | 73.6 | 0.526 | 1.89 | 1.87 |
| list | 135.8 | 422.9 | 88.7 | 0.321 | 1.53 | 1.52 |
| mandelbrot | 277.0 | 1326.8 | 272.6 | 0.209 | 1.02 | 1.02 |
| nbody | 131.1 | 1494.0 | 95.0 | 0.088 | 1.38 | 1.38 |
| permute | 52.1 | 252.5 | 14.1 | 0.206 | 3.70 | **0.19** |
| queens | 106.1 | 395.1 | 55.1 | 0.268 | 1.93 | 1.92 |
| richards | 54.3 | 199.6 | 35.0 | 0.272 | 1.55 | 1.54 |
| sieve | 133.7 | 1024.9 | 156.3 | 0.130 | 0.86 | 0.86 |
| storage | 107.3 | 254.1 | 154.3 | 0.422 | **0.70** | 0.69 |
| towers | 44.4 | 225.9 | 23.4 | 0.196 | 1.90 | 1.77 |
| events | 158.6 | 291.7 | 142.2 | 0.544 | 1.12 | **1.01** |
| expr_lang | 135.9 | 242.7 | 97.6 | 0.560 | 1.39 | 1.35 |
| lru_cache | 171.4 | 219.8 | 115.8 | 0.780 | 1.48 | 1.33 |
| markdown | 143.6 | 419.2 | 318.7 | 0.343 | 0.45 | 0.45 |
| raytrace | 128.7 | 265.8 | 40.6 | 0.484 | 3.17 | 3.14 |

Plain-build rows: `docs/bench-h3-tables-default-plain.md`; bench/real, programs rows: the same file. Against the BUNDLE-H2 PGO rows (docs/JIT_TRACE_REF.md): bounce 1.52 -> 1.40, queens 2.09 -> 1.93,
towers 2.01 -> 1.90, richards 1.67 -> 1.55, cd 2.15 -> 2.06, raytrace 3.24 -> 3.17, nbody 1.43 -> 1.38, list 1.59 -> 1.53 (the LuaJIT denominators of `permute`, `events`, `lru_cache` are bimodal).

## 3. Top remaining ratios (PGO, default mode) and why

29 of 169 programs have a min ratio over 2 (28 by median; plain 35 / 33; Bundle F: 33 / 31). Largest over all 169:

| rank | program | min | median | likely reason |
|---|---|---|---|---|
| 1 | jit-trace/rec_loop | 4.05 | 3.80 | trace recursion cost (frame push / pop per call; LuaJIT has up-recursion traces) |
| 2 | awfy/permute | 3.70 | 0.19 | measurement artefact: LuaJIT is bimodal (14 Mcyc fast mode, ~270 slow); against its slow mode we are 5x faster |
| 3 | lib/it_next_empty | 3.58 | 3.57 | `next` on an empty table: C-function call vs LuaJIT's inlined `next` |
| 4 | lib/find_plain | 3.51 | 3.59 | `string.find` plain: library call overhead vs LuaJIT's fast path |
| 5 | apps/raytrace | 3.17 | 3.14 | 47% of cycles in tier-1 territory (tier-1 code + its helpers) because loops with inner-loop calls abort (`InnerLoop`/`LeaveFrame`), plus allocation of short-lived vectors; sinking would give -13% (parked, see section 6) |
| 6 | programs/fib | 3.10 | 3.32 | recursion in trace code (backend cost per call), 100% in traces |
| 7 | lib/sub | 3.08 | 3.08 | `string.sub` call overhead (LuaJIT interns the result inline) |
| 8 | jit-trace/tail_loop | 2.70 | 2.71 | tail-call loop in traces |
| 9 | lib/tonumber_int | 2.65 | 2.65 | `tonumber` on an integer: C-call overhead |
| 10 | lib/m_random_f | 2.51 | 2.51 | `math.random()` float: C-call + xoshiro state through the C API path |
| ... | awfy/cd | 2.06 | 2.05 | tier-1 territory 39% (recursive red-black-tree insert), compile cost 16%, table allocation; sinking is worse on cd (+14%) |
| ... | awfy/queens | 1.93 | 1.92 | recursion through tier 1 (24%), exit stubs 12.8% (slot write-back before the side trace reloads), root re-entry prologue 5% |
| ... | awfy/towers | 1.90 | 1.77 | tier-1 call/return frame cost, integer key 0 in the hash part of `piles` |

Among real programs (awfy, apps, programs, real) the largest are raytrace 3.17, fib 3.10, cd 2.06, queens 1.93, towers 1.90, json 1.89 (Bundle F: towers 4.17, raytrace 3.25, fib 3.03, queens 2.44, cd 2.38).

## 4. Startup and memory

Startup (ms wall, `taskset -c 4`, min / median of 100; `docs/bench-h3-startup.md`):

| program | PUC | LuaJIT | H2 plain | H2 PGO | plain | PGO |
|---|---|---|---|---|---|---|
| hello.lua | 0.55 / 0.65 | 0.65 / 0.71 | 0.69 / 0.76 | 0.71 / 0.76 | 0.66 / 0.76 | 0.65 / 0.77 |
| ten_ms.lua | 1.10 / 1.26 | 1.04 / 1.17 | 1.22 / 1.40 | 1.14 / 1.31 | 1.20 / 1.39 | 1.15 / 1.32 |

Peak RSS (ru_maxrss min of 3, awfy + apps + real, `docs/bench-h3-rss.md`): geomean ratio to PUC: LuaJIT 1.34, H2 plain 2.11, H2 PGO 2.17, **this tree plain 2.11, PGO 2.17** (unchanged; the code added is
a micro-op and a flag bit). Examples (MiB, PUC / LuaJIT / plain / PGO): cd 3.1 / 6.1 / 9.1 / 9.2, queens 2.4 / 3.2 / 5.4 / 5.7, raytrace 2.5 / 3.3 / 7.0 / 7.3.

## 5. PGO against plain (this tree, default mode)

| set (n) | pgo/plain cycles min | median | instructions | worst program | best program |
|---|---|---|---|---|---|
| awfy (14) | 0.955 | 0.954 | 0.960 | sieve 1.001 | json 0.868 |
| apps (5) | 0.893 | 0.893 | 0.926 | events 0.916 | raytrace 0.844 |
| programs (25) | 0.931 | 0.934 | 0.925 | sieve 1.051 | gc_churn 0.790 |
| jit-trace (29) | 0.991 | 0.990 | 0.991 | inline_leaf 1.006 | alloc_ctor 0.888 |
| lib (95) | 0.913 | 0.914 | 0.916 | str_lt 1.153 | tsort_str 0.600 |
| real (5) | 0.890 | 0.892 | 0.901 | game_loop 0.977 | json_roundtrip 0.826 |
| all (173) | 0.931 | 0.931 | 0.933 | | |

(Bundle F: 0.934. `programs/sieve` +5% and `lib/str_lt` +15% are PGO layout regressions of the same kind as before.)

## 6. Items sized and not shipped (details in docs/JIT_TRACE_REF.md)

| item | measured ceiling / result | decision |
|---|---|---|
| B. link entry for side traces | queens root re-entry: prologue 2.6% + type checks 2.4% (S3 perf attribution; only the 2.6% is skippable without changing the entry-ok bookkeeping policy) -> <= 2.6% on one program, 0.4% on the 7 targets | not built (below the gate) |
| C. late side traces (`sidelate`) | cd 0.958 vs 0.960 (A+F only), raytrace 0.992 vs 0.984, deltablue 0.951 vs 0.986; awfy geomean 0.965 vs 0.971-0.976 (richards bimodal) | not shipped: patch `docs/bench-h3-G-C-sinkdemote-sidelate.patch` |
| C. compact exit stubs | stubs are 72-75% of trace code bytes; no experiment found a size-for-speed trade that moves a program | not built |
| D. tier-1 `setmetatable` | `CALL` fast path: raytrace 1.007 (no gain); adding `TAILCALL`: raytrace +14% cycles, +27% instructions (trace set changes) | dropped: patch `docs/bench-h3-D-setmetatable.patch` |
| E. drop the trace BUDGET compare | S2 pass 1 column `b`: geomean 0.994, sieve 0.93, the other programs 0.98-1.00 -> about 1.4% on the E targets (sieve fib list lru_cache richards) | not built (below the gate) |
| F. the rest (lea folding, PHI moves, spills) | not sized beyond the HREFK key compare | not built |
| G. sinking per site | raytrace 0.871 (vs 0.984 without), cd 1.145 -> 0.994 (`sinkdemote=1024`; the A+F baseline is 0.960), json 1.035 -> 1.024 (1.077 at 256), 3-target geomean 0.982 | not shipped: patch `docs/bench-h3-G-C-sinkdemote-sidelate.patch` |

## 7. Verification

* PGO build: `scripts/pgo-build.sh`, training 181 runs 0 failed. jit-diff seeds 1..40 (hook none, `off tr trdef`, `JITDIFF_ENTERS=1`) **0 diffs with the PGO binary**, hook count 0 diffs, seeds 100..150 0 diffs;
  `scripts/jit-official.sh` PGO binary `RUSTYNAIL_JIT=always` and default: `JIT-OFFICIAL OK`.
* Test suite, mutants, 5.4: see the BUNDLE-H3 section of `docs/JIT_TRACE_REF.md`.

## 8. What is not covered / reproduce

Lua 5.4 is not benchmarked; no full mutant runners, ASan / Miri, fuzzing. Medians over RUNS 11 of a single machine; `lib` rows are single-call micro-benchmarks that weigh as much as whole programs in the geomeans.
Reproduce: build the base (`git archive 684bc9e1`) and this tree plain and PGO, copy to equal-length paths, then per set
`SET=<set> OUT=docs/bench-h3-default-<set>.json RUNS=11 CPU=4 scripts/bench-lj.sh ref=<PUC> lj=/usr/bin/luajit plain="env RUSTYNAIL_SEED=0 <plain>" pgo="env RUSTYNAIL_SEED=0 <pgo>" pgo2="..." bplain="..." bpgo="..."`
(warm=0 pass: ours columns only with `RUSTYNAIL_JIT_TRACE_LIMITS=warm=0` in the env string). Raw data: `docs/bench-h3-{default,warm0}-<set>.{json,txt,err}`.
