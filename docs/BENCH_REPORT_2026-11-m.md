# Benchmark report, November 2026 (post BUNDLE-M, 4-way parallel run): rustynail against PUC Lua 5.5 and LuaJIT

Tree: main `e6103dc3` (BUNDLE-M merged, C1 lockstep off shipped; since the post-J2 full run at `cde96490`: K item B (tier-1 inline SELF on strings), L (all default-off), M (C1 lockstep off in production, C2 / C4 off)).
Measurement-only task, no source change under `crates/`. Same recipes as `docs/BENCH_REPORT_2026-11-j2.md`: "plain" = `cargo build --release -p rn-cli --features jit`, "PGO" = `PGO_FEATURES=jit scripts/pgo-build.sh`
(regenerated on this tree, 181 training runs, 0 failed), rustc stable, AMD Ryzen AI 7 350 (Zen 5), Linux 7.3. PUC = `ref/55/build-seed0/lua` (Lua 5.5.1, fixed hash seed), LuaJIT = `/usr/bin/luajit` 2.1 (the `bench/lj` variants where a program has one).

**The run is 4-way parallel and its absolute cycles are NOT comparable to the J2 / H3 reports** (different session, four neighbours on the chip). Compare only columns of this session; the J2 headline is quoted below for orientation only.
The calibration (section 8) shows the 4-way pass reproduces a single-core pass to within +0.1% (geomean cycles min over 12 programs, no program above +1.1%, instructions identical), so the contention shift is negligible; the 4-way numbers can be trusted as if single-core.

Method: `scripts/bench-mfull.sh` -> `scripts/bench_mfull.py` (a work queue of programs over 4 workers pinned with `taskset -c 4/5/6/7` = four distinct physical cores; SMT siblings 12-15 idle; before the run `top`/`ps`
showed CPUs 4-7 and 12-15 idle apart from <1% desktop noise). Every column of one program runs interleaved on the same worker / core: per program a warm-up run per column (output capture / `.expected` compare), then
RUNS = 11 rounds of all columns, **the column order rotated by the round index** (the J2 harness used a fixed order), `perf stat -e cycles:u,instructions:u`, min and median over 11,
`RUSTYNAIL_SEED=0`, stdin `/dev/null`, whole run under `systemd-run ... MemoryMax=12G MemorySwapMax=0`. Absolute script paths, identical across columns (cd's trace set depends on the path).
Columns: `ref` (PUC), `lj`, `plain`, `pgo`, `pgo2` (byte copy of `pgo`, the duplicate), `bplain` / `bpgo` (**the `cde96490` binaries**, built same-session from `git archive`, plain and PGO regenerated on that tree, 181 training runs, 0 failed).
Binaries at equal-length paths (`~/.cache/mf-bin/{a..e}/rn-cli`, deleted afterwards); builds finished before measuring (cargo jobs capped at 3). All 173 programs (awfy 14, apps 5, programs 25, jit-trace 29, lib 95, real 5; 169 with a LuaJIT time).
Default mode = what users get; **warm0** = `RUSTYNAIL_JIT_TRACE_LIMITS=warm=0` (ours columns only). Wall time: default pass 13.5 min, warm0 pass 8.5 min, calibration 1 min.
Every output of every ours column equals PUC's / `.expected` (no `MISMATCH` on any ours column; LuaJIT `MISMATCH` on `jit-trace/field_global` and `sink_vec` is number formatting, as in J2).
Data: `docs/bench-mfull-{default,warm0,single}-<set>.{json,txt}` (+ `.err` progress logs), `docs/bench-mfull-tables-{pgo,plain}.md` (every per-program row, both builds, regressions over 3%, movers),
`docs/bench-mfull-calibration.md`, `docs/bench-mfull-startup.md`, `docs/bench-mfull-rss.md`; scripts `scripts/bench_mfull.py`, `bench-mfull.sh`, `bench-mfull-all.sh` (driver), `bench_mfull_report.py` (`geo`, `ahead`, `sets`, `worst`, `movers`, `rows`, `ba`, `top`, `dup`, `cal`), `bench_mfull_startup.py`.

**Contamination check.** The duplicate column `pgo2` agrees with `pgo` to a geomean of 0.9985 (default) / 0.9993 (warm0) in cycles min, instructions exactly 1.000 on every program. Worst single rows of two byte-identical binaries:
`lib/load_small` 1.110 (default; 1.102 warm0: 194.9 vs 175.6 Mcyc, reproducible, a code-alignment effect of the identical binary at its different path/inode layout, not noise), `programs/table_hash` 1.034, `programs/coroutines` 1.027,
`programs/gc_churn` 1.023, `programs/nbody` 1.020. That is the noise floor of a single row; every ">3%" below is judged on instructions as well.

## 1. Headline

Geomean of per-program cycle ratios (lower = ours faster), default mode, all 173 programs (169 for LuaJIT). In parentheses: the `cde96490` binary measured in the same session (`bplain` / `bpgo`).

| build | ours/PUC (min) | ours/PUC (median) | ours/LuaJIT (min) | ours/LuaJIT (median) | ours/LuaJIT median, without the 5 noisiest |
|---|---|---|---|---|---|
| plain release | 0.233 | 0.233 | **1.155** | 1.138 | 1.142 |
| **PGO release** | **0.218** | 0.218 | **1.074** | 1.058 | 1.062 |

(Without `awfy/permute`, whose LuaJIT time is bimodal: PGO 0.218 / 0.218 / 1.077 / 1.070, plain 0.234 / 0.234 / 1.158 / 1.151.)
For orientation, J2 report (other session, single core): PGO 0.219 / 0.218 / 1.085 / 1.059, plain 0.235 / 0.234 / 1.168 / 1.140. The ours/LuaJIT *min* difference (1.085 -> 1.074) is a LuaJIT denominator effect, not ours:
`permute` LuaJIT was in its fast mode in the J2 session (min 14.8 Mcyc, ratio 3.31) and in its slow mode here (min 70.8, median 293 Mcyc, ratio 0.69; ours 48.6 Mcyc both times). With `permute` removed the J2 PGO min geomean is 1.078, here 1.077;
the medians (1.059 -> 1.058) agree. Same-session ours-over-ours movement is in section 5 (PGO -0.5%, plain -0.7%).
Ours is **4.6x faster than the reference interpreter overall (PGO) and within 7.4% (min) / 5.8% (median) of LuaJIT.**
**Ours is faster than LuaJIT on 63 of 169 programs by min and 64 by median (PGO); plain 56 by min, 57 by median** (J2: PGO 62 / 65, plain 54 / 57).
5 noisiest programs: awfy/permute 4.14 (LuaJIT bimodal: 70.8 / 293.2 Mcyc), apps/events 1.65 (LuaJIT 96.9 / 160.3), programs/table_hash 1.51 (ours and PUC bimodal: PGO min 671.7, median 1013.8 Mcyc, also in the base binary), apps/lru_cache 1.29 (LuaJIT 110.5 / 142.6), awfy/cd 1.17 (LuaJIT 107.4 / 125.6).

| faster than LuaJIT | awfy (14) | apps (5) | programs (25) | jit-trace (29) | lib (91) | real (5) | all (169) |
|---|---|---|---|---|---|---|---|
| PGO, min / median | 4 / 4 | 1 / 2 | 11 / 11 | 9 / 10 | 37 / 36 | 1 / 1 | **63 / 64** |
| plain, min / median | 4 / 4 | 1 / 1 | 11 / 11 | 9 / 10 | 30 / 30 | 1 / 1 | 56 / 57 |

(awfy 4 instead of J2's 3 is `permute`, again the LuaJIT slow mode.)

### Per set, PGO build, default mode

| set (n) | ours/PUC min | ours/PUC median | ours/LJ min (n with LJ) | ours/LJ median | ours/LJ median w/o 5 noisiest |
|---|---|---|---|---|---|
| awfy (14) | 0.264 | 0.264 | 1.248 (14) | 1.104 | 1.244 |
| apps (5) | 0.522 | 0.527 | 1.379 (5) | 1.193 | 1.238 |
| programs (25) | 0.220 | 0.222 | 0.931 (25) | 0.941 | 0.912 |
| jit-trace (29) | 0.080 | 0.080 | 1.014 (29) | 1.006 | 1.006 |
| lib (95) | 0.263 | 0.262 | 1.088 (91) | 1.087 | 1.087 |
| real (5) | 0.451 | 0.451 | 1.233 (5) | 1.210 | 1.210 |
| **all** (173) | 0.218 | 0.218 | 1.074 (169) | 1.058 | 1.062 |

### Per set, plain build, default mode

| set (n) | ours/PUC min | ours/PUC median | ours/LJ min (n with LJ) | ours/LJ median | ours/LJ median w/o 5 noisiest |
|---|---|---|---|---|---|
| awfy (14) | 0.277 | 0.277 | 1.308 (14) | 1.158 | 1.305 |
| apps (5) | 0.589 | 0.592 | 1.557 (5) | 1.341 | 1.418 |
| programs (25) | 0.237 | 0.238 | 1.000 (25) | 1.010 | 0.981 |
| jit-trace (29) | 0.081 | 0.080 | 1.021 (29) | 1.015 | 1.015 |
| lib (95) | 0.287 | 0.286 | 1.194 (91) | 1.193 | 1.193 |
| real (5) | 0.504 | 0.506 | 1.380 (5) | 1.356 | 1.356 |
| **all** (173) | 0.233 | 0.233 | 1.155 (169) | 1.138 | 1.142 |

warm=0 (ours columns, ratios against the default-pass PUC / LuaJIT columns): PGO all 0.218 / 0.217 PUC, 1.075 / 1.057 LuaJIT (min / median); plain 0.234 / 0.234, 1.157 / 1.139.

## 2. awfy and apps, per program (PGO build, default mode; Mcyc = cycles:u / 1e6, min of 11)

| program | ours Mcyc | PUC Mcyc | LJ Mcyc | ours/PUC | ours/LJ min | ours/LJ median |
|---|---|---|---|---|---|---|
| bounce | 62.2 | 520.2 | 47.9 | 0.120 | 1.30 | 1.30 |
| cd | 218.5 | 438.5 | 107.4 | 0.498 | 2.03 | 1.74 |
| deltablue | 93.1 | 158.0 | 103.1 | 0.589 | 0.90 | 0.90 |
| havlak | 140.6 | 225.8 | 116.3 | 0.623 | 1.21 | 1.15 |
| json | 137.5 | 265.7 | 73.3 | 0.517 | 1.88 | 1.87 |
| list | 134.8 | 420.7 | 88.8 | 0.321 | 1.52 | 1.52 |
| mandelbrot | 276.5 | 1325.5 | 272.6 | 0.209 | 1.01 | 1.01 |
| nbody | 123.2 | 1510.1 | 95.0 | 0.082 | 1.30 | 1.30 |
| permute | 48.6 | 253.0 | 70.8 | 0.192 | **0.69** | **0.17** |
| queens | 100.2 | 396.0 | 54.6 | 0.253 | 1.84 | 1.77 |
| richards | 53.2 | 198.5 | 34.9 | 0.268 | 1.52 | 1.51 |
| sieve | 132.8 | 1029.1 | 156.3 | 0.129 | 0.85 | 0.85 |
| storage | 107.0 | 254.7 | 153.8 | 0.420 | **0.70** | 0.69 |
| towers | 42.0 | 226.2 | 23.3 | 0.186 | 1.80 | 1.72 |
| events | 157.7 | 292.4 | 96.9 | 0.539 | 1.63 | **1.00** |
| expr_lang | 131.5 | 243.9 | 97.3 | 0.539 | 1.35 | 1.33 |
| lru_cache | 173.3 | 221.1 | 110.5 | 0.784 | 1.57 | 1.28 |
| markdown | 148.7 | 417.9 | 316.7 | 0.356 | **0.47** | 0.46 |
| raytrace | 125.0 | 262.5 | 40.6 | 0.476 | 3.08 | 3.09 |

`richards` was not bimodal in either binary this session (53.2 / 54.0 Mcyc). `cd` 218.5 Mcyc (J2 220.7; the cross-session 254-270 spread did not appear).

## 3. Top remaining ratios (PGO, default mode)

27 of 169 programs have a min ratio over 2 (26 by median; J2: 29 / 27). Largest:

| rank | program | min | median | note |
|---|---|---|---|---|
| 1 | lib/it_next_empty | 3.58 | 3.59 | `next` on an empty table: C call vs LuaJIT inlined `next` |
| 2 | lib/find_plain | 3.53 | 3.57 | `string.find` plain: library call overhead |
| 3 | programs/fib | 3.10 | 3.38 | recursion in trace code (LuaJIT 4-way min is 2.5% lower than single-core, see section 8) |
| 4 | lib/sub | 3.09 | 3.07 | `string.sub` call overhead |
| 5 | apps/raytrace | 3.08 | 3.09 | short-lived vector allocation, tier-1 territory (-4.9% against the cde96490 binary, instructions 0.970) |
| 6 | lib/tonumber_int | 2.65 | 2.66 | C-call overhead |
| 7 | lib/m_random_f | 2.50 | 2.51 | `math.random()` float |
| 8 | jit-trace/tail_loop | 2.49 | 2.49 | tail-call loop in traces |
| 9 | lib/fmt_s | 2.42 | 2.33 | |
| 10 | lib/tostring_tbl | 2.35 | 2.33 | |
| ... | awfy/cd | 2.03 | 1.74 | |
| ... | awfy/json, towers, queens | 1.88 / 1.80 / 1.84 | | |

`awfy/permute` left the list only because LuaJIT was in its slow mode (see above). Full list: `docs/bench-mfull-tables-pgo.md`.

## 4. Startup and memory

Startup (ms wall, `taskset -c 4`, single core, min / median of 100, interleaved; `docs/bench-mfull-startup.md`):

| program | PUC | LuaJIT | base plain | base PGO | plain | PGO |
|---|---|---|---|---|---|---|
| hello.lua | 0.54 / 0.64 | 0.56 / 0.69 | 0.79 / 0.95 | 0.78 / 0.96 | 0.77 / 0.94 | 0.76 / 0.93 |
| ten_ms.lua | 1.03 / 1.22 | 0.98 / 1.17 | 1.35 / 1.57 | 1.27 / 1.48 | 1.35 / 1.57 | 1.22 / 1.48 |

No startup change versus the cde96490 binary (ten_ms PGO min 1.27 -> 1.22, within noise).

Peak RSS (ru_maxrss min of 3, awfy + apps + real, `docs/bench-mfull-rss.md`, single core): geomean ratio to PUC: LuaJIT 1.33, **base plain 2.13, base PGO 2.20, this tree plain 2.14, PGO 2.21** (+0.5% / +0.5%).

## 5. K + L + M against the cde96490 binary (same session, default mode, min of 11)

| set (n) | plain: cycles min | median | instr | PGO: cycles min | median | instr | worst program (plain / PGO) | best program (plain / PGO) |
|---|---|---|---|---|---|---|---|---|
| awfy (14) | 0.990 | 0.989 | 0.994 | 0.989 | 0.987 | 0.993 | sieve 1.011 / sieve 1.008 | deltablue 0.957 / 0.954 |
| apps (5) | 1.004 | 1.005 | 0.993 | 0.991 | 0.990 | 0.992 | markdown 1.060 / 1.042 | expr_lang 0.972 / raytrace 0.951 |
| programs (25) | 1.000 | 0.996 | 0.997 | 0.996 | 1.007 | 0.997 | table_hash 1.135 / closures 1.016 | string_ops 0.931 / pcall_error 0.944 |
| jit-trace (29) | 0.998 | 0.999 | 0.999 | 1.000 | 0.999 | 0.999 | type_entry_stable 1.007 / field_global 1.019 | sink_vec 0.975 / rec_loop 0.986 |
| lib (95) | 0.989 | 0.989 | 0.997 | 0.996 | 0.995 | 0.997 | find_plain 1.051 / error_tbl 1.092 | reverse 0.837 / pcall_err 0.880 |
| real (5) | 0.988 | 0.989 | 0.979 | 0.985 | 0.985 | 0.975 | lua_interp 1.017 / template_engine 1.030 | wordfreq 0.964 / json_roundtrip 0.940 |
| **all (173)** | **0.993** | 0.992 | 0.997 | **0.995** | 0.997 | 0.996 | | |

warm0 (the mode where C1 lockstep matters most, every trace is compiled early): PGO all 0.995 (awfy 0.987, apps 0.990, real 0.987, programs 0.995, lib 0.996, jit-trace 1.000), plain 0.992. This is K + L + M together; the
M claim (PGO warm0 awfy 0.986, apps 0.984, real 0.993) is reproduced for awfy and apps and real within about 1% (0.987 / 0.990 / 0.987). K item B is visible in the library string rows (below); M's C1 is visible as the awfy / real / apps group (-1%),
whose instruction counts also drop (0.993 / 0.992 / 0.975).

### Top 10 improvements (PGO, cycles min; plain in `docs/bench-mfull-tables-plain.md`)

| program | Mcyc base -> now | cycles min | cycles median | instructions |
|---|---|---|---|---|
| lib/pcall_err | 204.7 -> 180.1 | 0.880 | 0.879 | 1.007 |
| lib/reverse | 282.8 -> 252.8 | 0.894 | 0.871 | 0.924 |
| lib/lower | 237.6 -> 217.4 | 0.915 | 0.927 | 0.924 |
| lib/upper | 258.4 -> 241.5 | 0.935 | 0.937 | 0.921 |
| real/json_roundtrip | 515.2 -> 484.5 | 0.940 | 0.944 | 0.928 |
| lib/co_wrap | 219.2 -> 206.5 | 0.942 | 0.947 | 1.002 |
| programs/pcall_error | 1913.3 -> 1806.8 | 0.944 | 0.955 | 0.991 |
| programs/json | 1713.3 -> 1620.5 | 0.946 | 0.945 | 0.955 |
| apps/raytrace | 131.4 -> 125.0 | 0.951 | 0.963 | 0.970 |
| awfy/deltablue | 97.5 -> 93.1 | 0.954 | 0.956 | 0.976 |

Plain top 10: lib/reverse 0.837 (instr 0.925), fmt_mixed 0.860 (instr 1.000: layout), upper 0.879 (0.925), fmt_f 0.889 (layout), fmt_g 0.893 (layout), match 0.908, lower 0.926 (0.925), gmatch_words 0.928, programs/string_ops 0.931, fmt_x_pad 0.932.
The real instruction drops (reverse / upper / lower -7.5%, json_roundtrip -7.2%, programs/json -4.5%, raytrace -3%) are K item B (inline SELF on strings: `s:upper()` etc.). `pcall_err`, `co_wrap` and the `fmt_*` rows move on unchanged instructions: layout, not a code change.

### Top 10 regressions (PGO, cycles min)

| program | Mcyc base -> now | cycles min | cycles median | instructions |
|---|---|---|---|---|
| lib/error_tbl | 213.3 -> 232.9 | 1.092 | 1.022 | 1.001 |
| lib/load_small | 183.3 -> 194.9 | 1.063 | 1.065 | 1.000 |
| apps/markdown | 142.7 -> 148.7 | 1.042 | 1.030 | 0.999 |
| real/template_engine | 705.0 -> 725.9 | 1.030 | 1.026 | 0.995 |
| jit-trace/field_global | 526.5 -> 536.7 | 1.019 | 1.000 | 1.000 |
| lib/m_random | 146.3 -> 149.0 | 1.018 | 1.017 | 1.000 |
| programs/closures | 455.3 -> 462.7 | 1.016 | 1.016 | 1.000 |
| lib/fmt_x_pad | 105.9 -> 107.6 | 1.016 | 1.020 | 1.000 |
| programs/string_ops | 1319.0 -> 1335.5 | 1.013 | 1.086 | 0.984 |
| lib/tsort_cmp | 257.6 -> 260.7 | 1.012 | 1.011 | 1.000 |

Plain top 10: programs/table_hash 1.135 (median 0.919: the bimodal row, see section 1), programs/pcall_error 1.064 (instr 0.992), apps/markdown 1.060, lib/find_plain 1.051, tonumber_bad 1.032, string_key_const 1.027, concat_op_str 1.021, tostring_tbl 1.018, apps/events 1.018, real/lua_interp 1.017.

**The 3% rule.** No program is more than 0.7% worse in instructions in default mode (1.0% in warm0, `programs/pcall_error`; instruction ratios of all regressions listed are 0.984 to 1.007). Programs more than 3% worse in cycles min (`docs/bench-mfull-tables-*.md`, "regressions over 3%"):
PGO `lib/error_tbl` 1.092, `lib/load_small` 1.063, `apps/markdown` 1.042 (median 1.030), `real/template_engine` 1.030 (instr 0.995); plain `programs/table_hash` 1.135, `programs/pcall_error` 1.064, `apps/markdown` 1.060, `lib/find_plain` 1.051, `lib/tonumber_bad` 1.032.
Judgement: `lib/load_small` is the duplicate-column outlier (pgo2 175.6 vs pgo 194.9 Mcyc with identical bytes), `lib/error_tbl` has a base spread of 7% between min and median (213.3 / 229.6), `programs/table_hash` is bimodal (min 621-705, median ~1000 in every column, both binaries),
`lib/find_plain` / `tonumber_bad` / `pcall_error` are library / error rows with 0 instruction change. The one that repeats across sessions is **`apps/markdown`**: PGO +4.2% (+3.0% in J2 against H3), plain +6.0%, warm0 +2.8% / +5.9%, with instructions 0.999 each time.
It is not explained by an instruction or trace change (trace counts were not re-measured here); it has now regressed in two consecutive steps on unchanged instruction counts. Recommend a trace-count / perf comparison of `markdown` on the next item that touches the tier-1 / recorder code. `real/template_engine` +3.0% (also +3.2% in warm0, instructions 0.995) is the only other program that appears in both modes.

## 6. PGO against plain (this tree, default mode)

| set (n) | pgo/plain cycles min | median | instructions | worst program | best program |
|---|---|---|---|---|---|
| awfy (14) | 0.954 | 0.953 | 0.959 | mandelbrot 0.999 | json 0.869 |
| apps (5) | 0.886 | 0.890 | 0.926 | events 0.908 | raytrace 0.841 |
| programs (25) | 0.931 | 0.932 | 0.925 | sieve 1.056 | gc_churn 0.799 |
| jit-trace (29) | 0.993 | 0.992 | 0.992 | inline_leaf 1.009 | alloc_ctor 0.905 |
| lib (95) | 0.917 | 0.916 | 0.916 | str_lt 1.156 | tsort_str 0.613 |
| real (5) | 0.894 | 0.893 | 0.899 | game_loop 0.974 | json_roundtrip 0.823 |
| all (173) | 0.932 | 0.932 | 0.932 | | |

(J2: 0.932; `programs/sieve` +5.6% and `lib/str_lt` +15.6% are the same PGO layout regressions as in H3 / J2.)

## 7. Duplicate-column and bimodality notes

* `pgo2` / `pgo`: see the contamination check at the top (geomean 0.9985; `lib/load_small` 1.110 byte-identical).
* LuaJIT bimodal this session: `awfy/permute` (slow mode: min 70.8, median 293.2), `apps/events` (96.9 / 160.3), `apps/lru_cache` (110.5 / 142.6), `awfy/cd` (107.4 / 125.6). Ours: `programs/table_hash` bimodal in all ours columns and in the base binary.
* `richards` not bimodal in the base this session. `raytrace` -4.9% and `expr_lang` -4.0% (PGO) against the base with instruction ratios 0.970 / 0.995: raytrace is a real (K item B or C1) saving, `expr_lang` is the layout-sensitive group
  (plain 0.972, instructions 0.995): treat as unexplained, small.

## 8. Calibration: 4-way against single core (12 programs, plain + PGO + base PGO, same RUNS 11)

Programs: awfy/cd, deltablue, json, richards, towers; apps/raytrace, expr_lang; programs/fib; jit-trace/oop_override; lib/find_plain (string-call-heavy), lib/gsub_str (string-heavy); real/lua_interp.
Single-core = `CPUS=4` only, nothing else running, all seven columns exactly as in the 4-way pass (`docs/bench-mfull-single-*.json`); 4-way = the main default pass. Ratio = 4-way / single (> 1: contention made it slower). Full table: `docs/bench-mfull-calibration.md`.

| column | cycles min geomean (range) | cycles median geomean (range) | instructions |
|---|---|---|---|
| plain | **1.001** (0.990 towers .. 1.010 raytrace) | 1.013 (0.995 .. 1.117 fib, a bimodal median) | 1.0000 every program |
| PGO | **1.001** (0.992 towers .. 1.010 cd) | 1.002 (0.995 .. 1.011) | 1.0000 |
| base PGO | **1.002** (0.992 richards .. 1.011 raytrace) | 0.999 (0.992 .. 1.007) | 1.0000 |
| LuaJIT | per program 0.975 (fib) .. 1.011 (cd) | cd 1.066, expr_lang 0.981, lua_interp 0.965 | |

ours/LuaJIT geomean over these 12 (min / median):

| setup | plain | PGO | base PGO |
|---|---|---|---|
| single core | 1.751 / 1.707 | 1.620 / 1.596 | 1.647 / 1.609 |
| 4-way | 1.756 / 1.724 | 1.625 / 1.595 | 1.652 / 1.603 |

**Conclusion.** The contention shift is +0.1% (min) on every column, at most 1.1% for any program, and no program differs by more than 1.1% in min; instructions are bit-identical. This is far below the 3% geomean / 8% per-program thresholds, so there is
no contention-induced bias and **all 4-way numbers can be trusted at the single-core level** (the Zen 5 cores with idle SMT siblings, separate L2, share only L3 / memory bandwidth, which these small-footprint programs do not stress).
The medians of `fib` (plain 1.117: the bimodal median of that row) and `cd` LuaJIT (1.066) are the only visible differences and are bimodality, not contention. The ours/LuaJIT geomean over the 12 shifts by +0.3% (min, plain and PGO) and -0.1% .. +1.0% (median), the LuaJIT column
moving by -2.5% (fib) .. +1.1% (cd) per row, which is within the per-row noise floor.

## 9. Caveats

* Absolute cycles are not comparable to the J2 / H3 reports (different session, 4-way run). Compare same-session columns only (`bplain` / `bpgo`).
* Cross-session unreliable rows: LuaJIT permute / events / expr_lang / lru_cache / cd bimodal; `lib/load_small`, `lib/error_tbl`, `programs/table_hash`: single-row noise floor up to 11% on identical binaries (duplicate column). `lib` rows are single-call micro-benchmarks that weigh as much as whole programs in the geomeans.
* The column order is rotated per round here (fixed order in the J2 harness): a possible, small difference in what a single row's min picks up, with no visible effect on the duplicate column (geomean 0.9985).
* K, L and M are measured together against the base binary; K (string SELF) and M (C1) are not separated (L is default-off and affects nothing). Trace counts were not re-measured.
* Lua 5.4 not benchmarked; no mutants, sanitizers or fuzzing in this task. Medians over RUNS 11 on a single machine.

## 10. Reproduce

Build this tree plain (`cargo build --release -p rn-cli --features jit`) and PGO (`PGO_FEATURES=jit scripts/pgo-build.sh <dir>`), and the same two from `git archive cde96490` into a scratch directory (all builds with `CARGO_BUILD_JOBS=3`,
under `systemd-run --user --scope -p MemoryMax=8G -p MemorySwapMax=0`); copy the four binaries and a byte copy of PGO to `~/.cache/mf-bin/{a,b,c,d,e}/rn-cli` (plain, PGO, PGO copy, base plain, base PGO);
then `scripts/bench-mfull-all.sh` (stage 1 calibration on CPU 4, stage 2 default and stage 3 warm0 on CPUs 4-7, 11 runs). Tables: `scripts/bench_mfull_report.py` (see its docstring; `BENCH_PREFIX=mfull` for `geo` / `top`).
Startup: `scripts/bench_mfull_startup.py 100`. RSS: `scripts/bp_rss.py 3 puc=... lj=... bplain=... bpgo=... plain=... pgo=... -- awfy apps real`.
