# Benchmark report, November 2026 (post BUNDLE-J2): rustynail against PUC Lua 5.5 and LuaJIT

Tree: BUNDLE-J2 `a99f1d9e` (J1 + J2 on top of BUNDLE-H3 `8aef493b`). Measurement-only task, no source change. Same methodology as `docs/BENCH_REPORT_2026-11-h3.md`:
`cargo build --release -p rn-cli --features jit` ("plain") and `PGO_FEATURES=jit scripts/pgo-build.sh` ("PGO", regenerated on this tree: 181 training runs, 0 failed), rustc stable, AMD Ryzen AI 7 350 (Zen 5), Linux 7.3.
PUC = `ref/55/build-seed0/lua` (Lua 5.5.1, fixed string-hash seed), LuaJIT = `/usr/bin/luajit` 2.1 (the `bench/lj` variants where a program has one).

Method: `scripts/bench-lj.sh` (`scripts/bench_lj.py`), `perf stat -e cycles:u,instructions:u`, `taskset -c 4`, every column interleaved per run, **RUNS 11 for every column**, min and median of cycles:u,
`RUSTYNAIL_SEED=0`, stdin `/dev/null`, memory capped (8G, no swap), machine otherwise idle (nothing else ran during the passes; builds ran before, startup/RSS after).
Columns: `ref` (PUC), `lj`, `plain` and `pgo` (this tree), `pgo2` (byte copy of `pgo`: the duplicate column), `hplain` / `hpgo` (the **H3 binaries, commit 8aef493b**, built same-session
from `git archive` with the same recipes: plain and PGO regenerated on that tree, 181 training runs, 0 failed). Binaries at equal-length paths (`~/.cache/jz{a..e}/rn-cli`, deleted afterwards).
All 173 programs (awfy 14, apps 5, programs 25, jit-trace 29, lib 95, real 5; 169 with a LuaJIT time). Default mode = what users get; **warm=0** = `RUSTYNAIL_JIT_TRACE_LIMITS=warm=0` (ours columns only).
Every output of every ours column equals PUC's / `.expected` (no `MISMATCH` on any ours column). Two LuaJIT columns (`jit-trace/field_global`, `jit-trace/sink_vec`) differ from `.expected` text
by number formatting only (`numeq`); they do not affect our columns.
Data: `docs/bench-j2full-{default,warm0}-<set>.{json,txt,err}`, `docs/bench-j2full-tables-{pgo,plain}.md` (every per-program row, both builds), `docs/bench-j2full-startup.md`,
`docs/bench-j2full-rss.md`; report script `scripts/bench_j2full_report.py` (`geo`, `ahead`, `sets`, `worst`, `movers`, `rows`, `ba`, `top`, `dup`).

**Contamination check.** The duplicate column `pgo2` agrees with `pgo` to a geomean of 1.0004 (default) / 0.9998 (warm0) in cycles min over 173 programs, instructions 1.000 on every program.
Single rows of two byte-identical binaries differ by up to 8% (worst: `lib/pcall_err` 1.082, `programs/pcall_error` 1.047, `lib/co_yield` 1.045, `programs/coroutines` 1.042, `awfy/storage` 1.039):
that is the noise floor of a single library / coroutine row and is why every ">3%" below is judged on instructions as well. No pass was discarded.

## 1. Headline

Geomean of per-program cycle ratios (lower = ours faster), default mode, all 173 programs (169 for LuaJIT). In parentheses: the **H3 binary measured in the same session** (`hplain` / `hpgo`).

| build | ours/PUC (min) | ours/PUC (median) | ours/LuaJIT (min) | ours/LuaJIT (median) | LuaJIT median, without the 5 noisiest |
|---|---|---|---|---|---|
| plain release | 0.235 (0.238) | 0.234 (0.237) | **1.168** (1.184) | 1.140 (1.155) | 1.143 (1.158) |
| **PGO release** | **0.219** (0.221) | 0.218 (0.221) | **1.085** (1.100) | 1.059 (1.072) | 1.059 (1.074) |

(H3 report, other session: PGO 0.222 / 1.097 / 1.072, plain 0.238 / 1.181 / 1.154; the same-session H3 binary reproduces it within 0.3%.)
Ours is **4.6x faster than the reference interpreter overall (PGO) and within 8.5% (geomean, min) of LuaJIT.**
**Ours is faster than LuaJIT on 62 of 169 programs by min and 65 by median (PGO); plain 54 by min, 57 by median** (same-session H3 binary: PGO 62 / 62, plain 54 / 56; H3 report: 62 / 62, 54 / 55).
5 noisiest programs: awfy/permute 18.95 (LuaJIT bimodal: min 14.8, median 280 Mcyc), apps/events 1.54 (LuaJIT bimodal: 103.7 / 159.4 Mcyc), apps/lru_cache 1.27 (LuaJIT 102.0 / 129.8), jit-trace/type_entry 1.15, programs/fib 1.12.

| faster than LuaJIT | awfy (14) | apps (5) | programs (25) | jit-trace (29) | lib (91) | real (5) | all (169) |
|---|---|---|---|---|---|---|---|
| PGO, min / median | 3 / 4 | 1 / 2 | 11 / 11 | 9 / 10 | 37 / 37 | 1 / 1 | **62 / 65** |
| plain, min / median | 2 / 4 | 1 / 1 | 11 / 11 | 9 / 10 | 30 / 30 | 1 / 1 | 54 / 57 |

### Per set, PGO build, default mode (H3 report column: ours/LJ min)

| set (n) | ours/PUC min | ours/PUC median | ours/LJ min (n with LJ) | ours/LJ median | ours/LJ median w/o 5 noisiest | H3 report: ours/LJ min | same-session H3 binary: ours/LJ min |
|---|---|---|---|---|---|---|---|
| awfy (14) | 0.268 | 0.268 | 1.412 (14) | 1.135 | 1.310 | 1.441 | 1.442 |
| apps (5) | 0.529 | 0.528 | 1.392 (5) | 1.209 | 1.253 | 1.268 | 1.386 |
| programs (25) | 0.220 | 0.220 | 0.919 (25) | 0.921 | 0.874 | 0.930 | 0.927 |
| jit-trace (29) | 0.080 | 0.080 | 1.013 (29) | 1.009 | 0.987 | 1.066 | 1.070 |
| lib (95) | 0.264 | 0.264 | 1.091 (91) | 1.089 | 1.089 | 1.093 | 1.092 |
| real (5) | 0.460 | 0.460 | 1.250 (5) | 1.227 | 1.227 | 1.255 | 1.265 |
| **all** (173) | 0.219 | 0.218 | 1.085 (169) | 1.059 | 1.059 | 1.097 | 1.100 |

(The apps min ratio 1.268 in the H3 report vs 1.386-1.392 here is the LuaJIT denominator: `events` LuaJIT is bimodal, min 103.7 Mcyc here, 142.2 in the H3 run; the median column is
comparable: 1.203 -> 1.209, ours moved -0.3%.)

### Per set, plain build, default mode

| set (n) | ours/PUC min | ours/PUC median | ours/LJ min (n with LJ) | ours/LJ median | ours/LJ median w/o 5 noisiest | H3 report: ours/LJ min |
|---|---|---|---|---|---|---|
| awfy (14) | 0.280 | 0.281 | 1.479 (14) | 1.189 | 1.376 | 1.509 |
| apps (5) | 0.584 | 0.584 | 1.539 (5) | 1.338 | 1.395 | 1.420 |
| programs (25) | 0.237 | 0.236 | 0.989 (25) | 0.988 | 0.988 | 0.999 |
| jit-trace (29) | 0.081 | 0.081 | 1.022 (29) | 1.018 | 0.995 | 1.076 |
| lib (95) | 0.289 | 0.289 | 1.200 (91) | 1.199 | 1.189 | 1.203 |
| real (5) | 0.511 | 0.512 | 1.390 (5) | 1.365 | 1.365 | 1.409 |
| **all** (173) | 0.235 | 0.234 | 1.168 (169) | 1.140 | 1.143 | 1.181 |

warm=0 (ours columns, ratios against the default-pass PUC / LuaJIT): PGO all 0.219 / 0.219 PUC, 1.087 / 1.061 LuaJIT (min / median); plain 0.235 / 0.235, 1.170 / 1.144. Per-set rows:
`docs/bench-j2full-tables-{pgo,plain}.md`.

## 2. awfy and apps, per program (PGO build, default mode; Mcyc = cycles:u / 1e6, min of 11)

| program | ours Mcyc | PUC Mcyc | LJ Mcyc | ours/PUC | ours/LJ min | ours/LJ median | vs H3 binary (min / instr) |
|---|---|---|---|---|---|---|---|
| bounce | 62.5 | 520.3 | 47.9 | 0.120 | 1.30 | 1.31 | 0.929 / 0.937 |
| cd | 220.7 | 437.8 | 105.8 | 0.504 | 2.09 | 2.02 | 0.985 / 0.992 |
| deltablue | 97.1 | 158.9 | 103.2 | 0.611 | 0.94 | 0.91 | 0.987 / 0.991 |
| havlak | 138.3 | 218.5 | 115.7 | 0.633 | 1.20 | 1.19 | 0.999 / 0.997 |
| json | 138.5 | 265.6 | 73.7 | 0.521 | 1.88 | 1.88 | 0.990 / 0.995 |
| list | 135.2 | 422.5 | 88.7 | 0.320 | 1.52 | 1.53 | 0.997 / 0.970 |
| mandelbrot | 277.0 | 1325.8 | 272.5 | 0.209 | 1.02 | 1.02 | 0.999 / 1.000 |
| nbody | 122.2 | 1477.7 | 94.3 | 0.083 | 1.30 | 1.31 | 0.940 / 0.938 |
| permute | 48.8 | 252.4 | 14.8 | 0.193 | 3.31 | **0.18** | 0.934 / 0.975 |
| queens | 102.3 | 395.2 | 54.7 | 0.259 | 1.87 | 1.86 | 0.970 / 0.988 |
| richards | 54.4 | 199.3 | 35.1 | 0.273 | 1.55 | 1.53 | 1.001 / 0.989 |
| sieve | 133.7 | 1024.5 | 156.3 | 0.131 | 0.86 | 0.86 | 1.004 / 1.000 |
| storage | 107.8 | 253.6 | 153.6 | 0.425 | **0.70** | 0.70 | 1.003 / 0.996 |
| towers | 43.2 | 226.4 | 23.0 | 0.191 | 1.88 | 1.80 | 0.974 / 0.989 |
| events | 157.0 | 291.8 | 103.7 | 0.538 | 1.51 | **0.99** | 0.982 / 0.999 |
| expr_lang | 137.3 | 243.4 | 98.5 | 0.564 | 1.39 | 1.35 | 1.025 / 0.999 |
| lru_cache | 171.1 | 220.6 | 102.0 | 0.776 | 1.68 | 1.33 | 0.990 / 0.998 |
| markdown | 148.2 | 416.4 | 320.4 | 0.356 | **0.46** | 0.46 | 1.030 / 0.999 |
| raytrace | 129.6 | 263.4 | 40.7 | 0.492 | 3.18 | 3.15 | 0.994 / 0.993 |

(awfy geomean vs the H3 binary: PGO 0.979, plain 0.980; apps 1.004 / 0.991. `permute` ours/LJ min 3.31 vs median 0.18 is the bimodal LuaJIT: against its slow mode we are 5.7x faster.
`apps/markdown` +3.0% (PGO) and `expr_lang` +2.5% are cycles only, instructions 0.999: see section 5.)

## 3. Top remaining ratios (PGO, default mode)

29 of 169 programs have a min ratio over 2 (27 by median; plain 36 / 32; H3 report PGO: 29 / 28). Largest over all 169:

| rank | program | min | median | note |
|---|---|---|---|---|
| 1 | lib/it_next_empty | 3.57 | 3.58 | `next` on an empty table: C-function call vs LuaJIT inlined `next` |
| 2 | lib/find_plain | 3.51 | 3.50 | `string.find` plain: library call overhead |
| 3 | awfy/permute | 3.31 | 0.18 | measurement artefact: LuaJIT bimodal |
| 4 | apps/raytrace | 3.18 | 3.15 | short-lived vector allocation, tier-1 territory (unchanged by J1/J2: 0.994) |
| 5 | lib/sub | 3.09 | 3.08 | `string.sub` call overhead |
| 6 | programs/fib | 3.03 | 3.29 | recursion in trace code (1425 Mcyc, identical to the H3 binary: J2's deeper inlining does not touch it) |
| 7 | lib/tonumber_int | 2.67 | 2.65 | C-call overhead |
| 8 | lib/m_random_f | 2.50 | 2.51 | `math.random()` float through the C API path |
| 9 | jit-trace/tail_loop | 2.49 | 2.50 | tail-call loop in traces (H3: 2.70) |
| 10 | lib/reverse | 2.48 | 2.48 | string call overhead (layout: this row moved +8% on unchanged instructions, see section 5) |
| ... | awfy/cd | 2.09 | 2.02 | recursive red-black-tree insert in tier 1, compile cost |
| ... | awfy/queens | 1.87 | 1.86 | recursion through tier 1, exit stubs (H3: 1.93) |
| ... | awfy/towers | 1.88 | 1.80 | tier-1 call/return frame cost (H3: 1.90) |

`jit-trace/rec_loop` left the list: 4.05 (H3) -> 1.02 ours/LuaJIT (142.3 Mcyc against LuaJIT 139.7). Among real programs (awfy, apps, programs, real): raytrace 3.18, fib 3.03, cd 2.09,
json 1.88, towers 1.88, queens 1.87. Full list: `docs/bench-j2full-tables-pgo.md` ("top ours/LJ ratios").

## 4. Startup and memory

Startup (ms wall, `taskset -c 4`, min / median of 100, interleaved; `docs/bench-j2full-startup.md`):

| program | PUC | LuaJIT | H3 plain | H3 PGO | plain | PGO |
|---|---|---|---|---|---|---|
| hello.lua | 0.53 / 0.63 | 0.59 / 0.69 | 0.84 / 0.95 | 0.84 / 0.96 | 0.82 / 0.95 | 0.83 / 0.94 |
| ten_ms.lua | 0.87 / 1.25 | 0.82 / 1.18 | 1.10 / 1.59 | 1.04 / 1.53 | 1.12 / 1.62 | 1.04 / 1.52 |

No startup change versus the H3 binary (same session). The absolute numbers are 0.15-0.2 ms above the H3 report's table for our binaries (H3 report: 0.65 / 0.77 for hello PGO) while
PUC / LuaJIT are close to their H3 values; the H3 binary shows the same shift, so it is a session effect, not J1/J2 (cause not investigated).

Peak RSS (ru_maxrss min of 3, awfy + apps + real, `docs/bench-j2full-rss.md`): geomean ratio to PUC: LuaJIT 1.34, **H3 plain 2.11, H3 PGO 2.18, this tree plain 2.13, PGO 2.22**
(+1% / +2%: code added by the inlining / stub work). Absolute values per program are in the data file.

## 5. J1 + J2 against the H3 binary (same session, default mode, min of 11)

| set (n) | plain: cycles min | median | instr | PGO: cycles min | median | instr | worst program (plain / PGO) | best program (plain / PGO) |
|---|---|---|---|---|---|---|---|---|
| awfy (14) | 0.980 | 0.981 | 0.983 | 0.979 | 0.979 | 0.982 | list 1.017 / sieve 1.004 | bounce 0.934 / 0.929 |
| apps (5) | 0.991 | 0.991 | 0.998 | 1.004 | 1.002 | 0.998 | expr_lang 0.999 / markdown 1.030 | markdown 0.980 / events 0.982 |
| programs (25) | 0.990 | 0.994 | 0.995 | 0.992 | 0.998 | 0.995 | table_hash 1.009 / pcall_error 1.015 | nbody 0.918 / metamethods 0.943 |
| jit-trace (29) | 0.947 | 0.948 | 0.939 | 0.947 | 0.948 | 0.939 | licm_expr 1.004 / type_entry_stable 1.006 | rec_loop 0.236 / 0.243 |
| lib (95) | 0.998 | 0.999 | 0.999 | 0.999 | 0.999 | 0.999 | match 1.047 / reverse 1.080 | m_type 0.937 / string_key_const 0.903 |
| real (5) | 0.982 | 0.984 | 0.991 | 0.989 | 0.986 | 0.991 | wordfreq 1.008 / wordfreq 1.012 | game_loop 0.945 / 0.944 |
| **all (173)** | **0.986** | 0.987 | 0.987 | **0.987** | 0.988 | 0.987 | | |

warm=0 agrees: PGO all 0.987 (awfy 0.974, apps 1.005, jit-trace 0.947), plain 0.987. Without `rec_loop` (0.243) the all-programs PGO geomean is about 0.995.
The suite-wide effect of J1 + J2 is therefore small except on `rec_loop` (4.1x fewer cycles, 5.8x fewer instructions: `deep=8` per-site inlining) and the awfy / numeric group (-2%).

### Top 10 improvements (PGO, cycles min; plain in `docs/bench-j2full-tables-plain.md`)

| program | Mcyc H3 -> J2 | cycles min | cycles median | instructions |
|---|---|---|---|---|
| jit-trace/rec_loop | 586.1 -> 142.3 | 0.243 | 0.244 | 0.171 |
| lib/string_key_const | 244.6 -> 220.9 | 0.903 | 0.928 | 0.930 |
| jit-trace/tail_loop | 603.8 -> 556.6 | 0.922 | 0.924 | 0.955 |
| awfy/bounce | 67.3 -> 62.5 | 0.929 | 0.929 | 0.937 |
| awfy/permute | 52.3 -> 48.8 | 0.934 | 0.937 | 0.975 |
| lib/load_small | 183.7 -> 172.7 | 0.940 | 0.956 | 1.002 (layout) |
| awfy/nbody | 130.0 -> 122.2 | 0.940 | 0.942 | 0.938 |
| programs/metamethods | 753.1 -> 710.2 | 0.943 | 0.930 | 0.946 |
| real/game_loop | 37.7 -> 35.6 | 0.944 | 0.942 | 0.955 |
| programs/nbody | 569.4 -> 541.0 | 0.950 | 0.946 | 0.951 |

Plain top 10: rec_loop 0.236, programs/nbody 0.918, tail_loop 0.926, metamethods 0.932, bounce 0.934, lib/m_type 0.937 (instr 1.000: layout), permute 0.937, lib/fmt_mixed 0.943 (instr 1.000: layout), game_loop 0.945, awfy/nbody 0.946.

### Top 10 regressions (PGO, cycles min)

| program | Mcyc H3 -> J2 | cycles min | cycles median | instructions |
|---|---|---|---|---|
| lib/reverse | 262.2 -> 283.0 | 1.080 | 1.088 | 1.000 |
| lib/upper | 244.4 -> 255.7 | 1.046 | 1.050 | 0.999 |
| lib/tpack | 128.6 -> 132.8 | 1.033 | 1.048 | 0.997 |
| apps/markdown | 143.9 -> 148.2 | 1.030 | 1.026 | 0.999 |
| apps/expr_lang | 134.0 -> 137.3 | 1.025 | 1.020 | 0.999 |
| lib/co_wrap | 204.8 -> 209.6 | 1.024 | 1.024 | 1.001 |
| lib/setmt | 174.4 -> 178.0 | 1.021 | 1.020 | 0.996 |
| lib/pcall_err | 187.9 -> 191.8 | 1.020 | 1.030 | 0.991 |
| lib/tinsert_idx | 273.4 -> 278.6 | 1.019 | 1.028 | 1.000 |
| programs/pcall_error | 1821.1 -> 1847.8 | 1.015 | 1.037 | 1.011 |

Plain top 10: lib/match 1.047, tpack 1.037, pcall_err 1.030, fmt_s 1.018, awfy/list 1.017 (instr 0.970), tconcat 1.015, fmt_d 1.015, find_cap 1.012, upper 1.012, tonumber_int 1.010.

**The 3% rule.** No program is more than 1.1% worse in instructions (worst: `programs/pcall_error` PGO 1.011, a program whose own duplicate-column spread is 4.7%). Programs more than 3% worse in cycles
(`docs/bench-j2full-tables-*.md`, "regressions over 3%"): PGO `lib/reverse` 1.080, `lib/upper` 1.046, `lib/tpack` 1.033, `apps/markdown` 1.030; plain `lib/match` 1.047, `lib/tpack` 1.037, `lib/pcall_err` 1.030.
All have instruction ratios 0.993-1.000 and are string / library rows that J1/J2 code does not execute (the duplicate column shows 3-8% on such rows), except `apps/markdown` (PGO 1.030, plain 0.980)
and `apps/expr_lang` (PGO 1.025, plain 0.999): instructions are unchanged (0.999) and the plain build improved, so this is the PGO layout of the new binary, not a J2 trace change (trace counts were not
re-measured here). Not explained further.

## 6. PGO against plain (this tree, default mode)

| set (n) | pgo/plain cycles min | median | instructions | worst program | best program |
|---|---|---|---|---|---|
| awfy (14) | 0.955 | 0.954 | 0.959 | sieve 1.007 | json 0.869 |
| apps (5) | 0.904 | 0.904 | 0.927 | markdown 0.944 | raytrace 0.857 |
| programs (25) | 0.930 | 0.932 | 0.925 | sieve 1.055 | gc_churn 0.791 |
| jit-trace (29) | 0.991 | 0.991 | 0.991 | inline_leaf 1.007 | alloc_ctor 0.870 |
| lib (95) | 0.914 | 0.914 | 0.916 | str_lt 1.164 | tsort_str 0.598 |
| real (5) | 0.900 | 0.898 | 0.901 | game_loop 0.970 | json_roundtrip 0.847 |
| all (173) | 0.932 | 0.932 | 0.933 | | |

(H3 report: 0.931. `programs/sieve` +5.5% and `lib/str_lt` +16% are the same PGO layout regressions as in H3.)

## 7. Caveats

* Comparisons across sessions are unreliable for the rows listed in the task notes (LuaJIT bimodal on `permute`, `events`, `expr_lang`, `lru_cache`; `cd` and the raytrace / expr_lang / markdown group move with unrelated binary
  changes). Compare against the same-session H3 binary columns (`hplain` / `hpgo`) as done in section 5. `cd`'s trace set depends on the script path string; here all columns ran the same `bench/awfy/cd.lua` path.
* `richards` was not bimodal this session (54.4 Mcyc both binaries).
* Single-row noise floor 3-8% for library / coroutine rows (duplicate column). `lib` rows are single-call micro-benchmarks that weigh as much as whole programs in the geomeans.
* Lua 5.4 not benchmarked; no mutants, sanitizers or fuzzing in this task. Medians over RUNS 11 on a single machine.

## 8. Reproduce

Build this tree plain (`cargo build --release -p rn-cli --features jit`) and PGO (`PGO_FEATURES=jit scripts/pgo-build.sh <dir>`), and the same two from `git archive 8aef493b` into a scratch directory; copy the four
binaries and a byte copy of PGO to equal-length paths; then per set (default pass; the warm0 pass drops `ref` / `lj` and adds `RUSTYNAIL_JIT_TRACE_LIMITS=warm=0` to every env string):
`SET=<set> OUT=docs/bench-j2full-default-<set>.json RUNS=11 CPU=4 scripts/bench-lj.sh ref=<PUC> lj=/usr/bin/luajit plain="env RUSTYNAIL_SEED=0 <a>" pgo="env RUSTYNAIL_SEED=0 <b>" pgo2="..." hplain="..." hpgo="..."`.
Tables: `scripts/bench_j2full_report.py` (see its docstring). RSS: `scripts/bp_rss.py 3 puc=... lj=... -- awfy apps real`.
