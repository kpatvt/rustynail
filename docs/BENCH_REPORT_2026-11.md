# Benchmark report, November 2026 (BUNDLE-P): where rustynail stands against PUC Lua 5.5 and LuaJIT

Tree: main at 8c5f4603 (Bundles 1..D merged), `cargo build --release -p rn-cli --features jit` ("plain") and
`scripts/pgo-build.sh` ("PGO", the recommended release build), rustc 1.99.1, AMD Ryzen AI 7 350 (Zen 5), Linux 7.3, THP `always`.
PUC = `ref/55/build-seed0/lua` (Lua 5.5.1 built from `vendor/lua-5.5`, fixed string-hash seed), LuaJIT = `/usr/bin/luajit`
(2.1, the `bench/lj` variants where a program has one). Lua 5.4 is not benchmarked.

Method: `scripts/bench-lj.sh` (`scripts/bench_lj.py`), `perf stat -e cycles:u,instructions:u`, `taskset -c 4` (sibling 12 hosts the
compositor), all columns interleaved per run, **RUNS 11 for every column** (ours and LuaJIT; the brief asked for >= 9 and >= 11),
min and median of cycles:u per program, `RUSTYNAIL_SEED=0`, stdin `/dev/null`, memory-capped. Binaries at equal-length paths
(`~/.cache/bp{a,b,e}/rn-cli`; `pgo2` is a byte copy of `pgo` = the duplicate column, mean 1.002 on 173 programs). Sets: awfy (14), apps (5),
programs (25), jit-trace (29), lib (95), bench/real (5) = 173 programs, 169 with a LuaJIT time (4 `lib` rows have no
LuaJIT equivalent, `bench/lj/SKIP`). Every output of every `ours` column equals PUC's (and `.expected`) on all 173 programs; the only
output differences are 3 LuaJIT variants (`coroutines`, `load_big`, `oop_methods`, number formatting) and 2 LuaJIT `.expected`
mismatches (`field_global`, `sink_vec`, numerically equal).
**Default mode** = what users get (`RUSTYNAIL_SEED=0`, no `warm=0`); **warm=0** = `RUSTYNAIL_JIT_TRACE_LIMITS=warm=0` (the A/B
convention of Bundles 1..D). Load average 0.4-1.5 during the runs, no foreign CPU-bound process except an interactive application opened at
06:38 (during the tail of jit-trace and the lib set, off CPU 4; the duplicate column shows no effect). **One measurement
artefact was found and repeated**: the first warm=0 pass of `apps` and `programs` ran 20-40% slow on all three columns (gc_churn 3035 instead
of 2270 Mcyc, lru_cache 280 instead of 186) for an unidentified outside reason; both sets were re-measured on the quiet machine and the first
pass is kept as `docs/bench-bp-warm0-CONTAMINATED-*` (not used).

## 1. Headline

Geomean of the per-program cycle ratios (ours / other, lower = ours faster), default mode, all 173 programs (169 for LuaJIT):

| build | ours/PUC (min) | ours/PUC (median) | ours/LuaJIT (min) | ours/LuaJIT (median) | median, without the 5 noisiest |
|---|---|---|---|---|---|
| plain release | 0.251 | 0.251 | 1.249 | 1.229 | 1.219 |
| **PGO release** | **0.236** | 0.236 | **1.170** | 1.150 | 1.140 |

(The 5 noisiest programs, by max(median/min) of ours or LuaJIT: awfy/permute 4.66 (LuaJIT is bimodal), apps/events 1.55, apps/lru_cache 1.38,
jit-trace/type_entry 1.15, programs/fib 1.12.)
So: **ours is 4.2x faster than the reference interpreter overall (PGO), and within 17% (geomean) of LuaJIT's JIT**; ours is faster than
LuaJIT on 59 of 169 programs (it wins big on GC- and string-heavy code: gc_churn 0.53, pcall_error 0.22, markdown 0.46, closures 0.48, varargs 0.53).

### Per set, PGO build, default mode

| set (n) | ours/PUC min | ours/PUC median | ours/LJ min (n with LJ) | ours/LJ median | ours/LJ median w/o 5 noisiest |
|---|---|---|---|---|---|
| awfy (14) | 0.317 | 0.317 | 1.681 (14) | 1.491 | 1.539 |
| apps (5) | 0.531 | 0.531 | 1.426 (5) | 1.211 | 1.254 |
| programs (25) | 0.228 | 0.228 | 0.948 (25) | 0.950 | 0.901 |
| jit-trace (29) | 0.085 | 0.085 | 1.071 (29) | 1.066 | 1.045 |
| lib (95) | 0.288 | 0.287 | 1.186 (91) | 1.183 | 1.183 |
| real (5) | 0.476 | 0.475 | 1.298 (5) | 1.271 | 1.271 |
| **all** (173) | 0.236 | 0.236 | 1.170 (169) | 1.150 | 1.140 |

### Per set, plain build, default mode

| set (n) | ours/PUC min | ours/PUC median | ours/LJ min (n with LJ) | ours/LJ median | ours/LJ median w/o 5 noisiest |
|---|---|---|---|---|---|
| awfy (14) | 0.330 | 0.330 | 1.752 (14) | 1.553 | 1.607 |
| apps (5) | 0.591 | 0.589 | 1.587 (5) | 1.345 | 1.399 |
| programs (25) | 0.245 | 0.245 | 1.018 (25) | 1.020 | 0.971 |
| jit-trace (29) | 0.086 | 0.085 | 1.083 (29) | 1.076 | 1.056 |
| lib (95) | 0.310 | 0.310 | 1.284 (91) | 1.284 | 1.284 |
| real (5) | 0.537 | 0.535 | 1.465 (5) | 1.433 | 1.433 |
| **all** (173) | 0.251 | 0.251 | 1.249 (169) | 1.229 | 1.219 |

### warm=0 (A/B convention), geomeans

| build | awfy ours/PUC, /LJ min | apps | programs | jit-trace | lib | real | all, /LJ min |
|---|---|---|---|---|---|---|---|
| plain | 0.331, 1.758 | 0.590, 1.584 | 0.246, 1.023 | 0.086, 1.090 | 0.311, 1.288 | 0.526, 1.436 | 0.252, 1.253 |
| PGO | 0.318, 1.688 | 0.528, 1.419 | 0.229, 0.950 | 0.085, 1.079 | 0.289, 1.188 | 0.464, 1.267 | 0.237, 1.172 |

warm=0 and default mode are the same within noise now (the deterministic work gate of Bundle 1 made the time gate irrelevant; compare
the two blocks above: <= 0.003 per set). Full tables: `docs/bench-bp-tables-{default,warm0}-{plain,pgo}.md`.

### Where the earlier docs stood (for orientation; other binaries, `warm=0`, RUNS 5-9)

BUNDLE-C/D plain build: awfy 0.332 / 1.782, apps 0.591 / 1.419, programs 0.245 / 1.017, jit-trace 0.085 / 1.078, lib 0.309 / 1.289.
This run, plain, warm=0: awfy 0.331 / 1.758, apps 0.590 / 1.584 (the Bundle C figure is lower: it used `bench-bc` RUNS 5 and
the older `lj` columns; the apps ratio is dominated by raytrace 3.8x and events/lru_cache whose LuaJIT time is noisy), programs 0.246 / 1.023,
jit-trace 0.090 / 1.090, lib 0.311 / 1.288. No set moved by more than the noise of the LuaJIT denominators, as expected: Bundles C and D
changed compile cost only (-1.4% cycles on awfy) or nothing.

## 2. awfy and apps, per program (PGO build, default mode; Mcyc = cycles:u / 1e6, min of 11)

| program | ours Mcyc | PUC Mcyc | LJ Mcyc | ours/PUC | ours/LJ min | ours/LJ median |
|---|---|---|---|---|---|---|
| bounce | 75.4 | 522.6 | 48.0 | 0.144 | 1.57 | 1.58 |
| cd | 257.4 | 438.5 | 108.0 | 0.587 | 2.38 | 2.27 |
| deltablue | 101.0 | 158.3 | 102.9 | 0.638 | 0.98 | 0.99 |
| havlak | 144.5 | 218.3 | 116.7 | 0.662 | 1.24 | 1.23 |
| json | 144.5 | 263.3 | 72.9 | 0.549 | 1.98 | 1.97 |
| list | 141.0 | 421.4 | 88.7 | 0.335 | 1.59 | 1.59 |
| mandelbrot | 277.1 | 1314.5 | 272.6 | 0.211 | 1.02 | 1.02 |
| nbody | 150.9 | 1525.8 | 95.0 | 0.099 | 1.59 | 1.58 |
| permute | 66.4 | 271.6 | 14.5 | 0.244 | 4.59 | **0.99** |
| queens | 133.4 | 415.7 | 55.0 | 0.321 | 2.43 | 2.28 |
| richards | 66.9 | 198.0 | 35.3 | 0.338 | 1.90 | 1.90 |
| sieve | 133.8 | 1029.2 | 156.5 | 0.130 | 0.86 | 0.86 |
| storage | 127.1 | 251.6 | 154.0 | 0.505 | 0.83 | 0.82 |
| towers | 97.4 | 228.9 | 23.5 | 0.425 | 4.15 | 4.03 |
| events | 167.0 | 291.3 | 95.3 | 0.573 | 1.75 | **1.14** |
| expr_lang | 135.5 | 244.3 | 96.8 | 0.555 | 1.40 | 1.32 |
| lru_cache | 165.6 | 220.3 | 103.8 | 0.752 | 1.60 | **1.16** |
| markdown | 147.5 | 414.4 | 318.2 | 0.356 | 0.46 | 0.46 |
| raytrace | 131.6 | 265.3 | 40.4 | 0.496 | 3.25 | 3.22 |

Bold = min and median differ a lot: LuaJIT `permute` is bimodal (14.5 Mcyc fast mode vs ~66 Mcyc in the slow mode; the median shows the slow
mode, the min the fast one), and `events` / `lru_cache` have a noisy LuaJIT (and ours: events 167-259 Mcyc). Use the median column for those three.
Plain build, warm=0 and bench/real tables: `docs/bench-bp-tables-*.md`.

## 3. Programs still slower than 2x LuaJIT, and the five largest ratios

36 of 169 programs have a min ratio over 2 (33 by median); they are almost all `lib` micro-rows (single-library-call loops) plus:
awfy cd 2.38, queens 2.43, towers 4.15 (permute only by min), apps raytrace 3.25, programs fib 3.10, real lua_interp 2.09,
jit-trace rec_loop 4.26 and tail_loop 2.71. Largest ratios over the whole 169 (PGO, default):

| rank | program | min | median | likely reason (categories of docs/PROFILE_STUDY.md) |
|---|---|---|---|---|
| 1 | lib/fmt_s | 53.4 | 49.3 | not a real gap: LuaJIT folds `string.format("%s", s)` to `s` (the loop is an array load + store, DIFF_STUDY / H1 notes); ours pays the C-function call |
| 2 | lib/str_eq_long | 7.9 | 7.9 | string representation: LuaJIT interns all strings (pointer compare), PUC-style long strings compare bytes (DIFF_STUDY; ~5x stays by design) |
| 3 | lib/pcall_ok | 5.3 | 5.3 | `pcall` is a rolled loop in LuaJIT (E7) and a call-machinery path here, never in a trace; **and PGO makes it worse: plain 160 Mcyc, PGO 217** (see 5) |
| 4 | awfy/permute | 4.6 | 1.0 | measurement artefact of LuaJIT's bimodality; against LuaJIT's slow mode we are level |
| 5 | jit-trace/rec_loop | 4.3 | 4.1 | trace recursion cost (6 vs 18-20 cycles per call, PROFILE_STUDY item 8) |

Among the real programs (awfy, apps, programs, real) the five largest are:

| rank | program | min | median | coverage (trace / tier-1 / interp / helpers+exit / compile / other, %) | likely reason |
|---|---|---|---|---|---|
| 1 | awfy/towers | 4.15 | 4.03 | 19 / 47 / 1 / 26 / 3 / 5 | tier-1 call/return frame cost (LuaJIT ~11 cycles per call vs ~21; PROFILE_STUDY item 6, Bundle D measured the ceiling under the 3% gate) plus integer keys in the hash part of `piles` (helper time 26%) |
| 2 | apps/raytrace | 3.25 | 3.22 | 28 / 17 / 1 / 12 / 16 / 26 | allocation and GC of short-lived vectors (constructor cost, item 3; sinking of tables with metatables is built but default off: raytrace -21%, cd +11%) and compile cost (16% of cycles, item 4) |
| 3 | programs/fib | 3.10 | 3.37 | 100 / 0 / 0 / 0 / 0 / 0 | pure trace code: recursion cost in traces (6.0 vs 17.6-19.8 cycles/call, item 8), i.e. the backend, not coverage |
| 4 | awfy/queens | 2.43 | 2.28 | 49 / 21 / 1 / 16 / 5 / 7 | exits and helpers (16%) and tier-1 code (21%): coverage; side traces were capped in Bundle B (queens 0.85) |
| 5 | awfy/cd | 2.38 | 2.27 | 20 / 30 / 1 / 12 / 13 / 23 | compile cost (13%), tier-1 share (30%), table-allocation runtime (other 23%: GC/allocator/strings/hash) |

Next after those: awfy json 1.98 (tier-1 19%, other 54%: strings, tables), richards 1.90 (compile 13%), programs knucleotide 1.98 (string/table runtime: `other`
53%), coroutines 1.82 (35% interpreter + 14% helpers: coroutine resume/yield runs in the interpreter and call machinery).

## 4. Trace coverage (shares of cycles, perf `-c 150001`, `RUSTYNAIL_JIT_PERFMAP=1`, PGO build, default mode, CPU 2)

Classifier: `scripts/pprof-classify.py` categories mapped to the columns of `docs/bench-b1-coverage.md` by `scripts/bp_coverage.py`:
trace = unsymbolised JIT code, tier-1 = `lua_proto@` code, interp = interpreter dispatch, helpers+exit = `jit_helpers::` + trace exit Rust code,
compile = `rn_jit::` compile, other = everything else (Lua compiler, GC, allocator, tables, strings, libc, library). All per-program rows:
`docs/bench-bp-coverage-tables.md`. Cycle-weighted by set (B1 = `docs/bench-b1-coverage.md`, base of Bundle 1, `warm=0`, plain):

| set | trace | tier-1 | interp | helpers+exit | compile | other | Mcyc total |
|---|---|---|---|---|---|---|---|
| awfy now (PGO) | 47.2 | 19.2 | 1.4 | 6.7 | 6.0 | 19.5 | 1944 |
| awfy B1 | 43.3 | 16.8 | 1.5 | 4.5 | 7.0 | 26.9 | 2247 |
| apps now | 11.5 | 27.1 | 4.8 | 9.0 | 6.6 | 40.9 | 760 |
| apps B1 | 10.4 | 20.5 | 4.2 | 7.4 | 6.9 | 50.6 | 965 |
| programs now | 42.0 | 5.6 | 3.8 | 7.7 | 0.2 | 40.6 | 26694 |
| programs B1 | 36.9 | 4.9 | 4.5 | 4.4 | 0.2 | 49.1 | 33297 |
| jit-trace now | 89.6 | 4.5 | 1.4 | 0.1 | 2.5 | 1.9 | 28863 |
| lib now | 28.3 | 5.6 | 2.4 | 11.0 | 2.7 | 50.0 | 19293 |
| real now | 3.3 | 10.8 | 12.7 | 11.6 | 1.6 | 60.1 | 1541 |

Reading: trace coverage grew 3-5 points on awfy / apps / programs; compile share fell a point on awfy (Bundle C), tier-1 and helpers+exit shares
rose a little because they are measured on a binary whose other categories got cheaper (PGO, GC pacing: `other` fell 5-9 points and the total
cycles fell 14-21%). Caveat: B1 was a plain `warm=0` profile with the symbol set of that day; this one is a PGO binary (inlining moves symbols
between categories, e.g. helpers are partly inlined), so shifts of 1-3 points are not significant. Still interpreted: apps 4.8%
(lru_cache 13%), real 12.7% (template_engine 21%), awfy 1.4%. `other` is where the remaining gap is: awfy storage 79%, havlak 51%, json 54%,
apps markdown 85%, real template_engine 72% / json_roundtrip 57% are allocation, GC, string and table-runtime time, not JIT code.

## 5. Post-link and layout trials on top of PGO (all 173 programs, default mode, RUNS 11, ratio to the plain PGO binary)

| variant | geomean cycles min | median | instructions | worst set | worst program | best program | verdict |
|---|---|---|---|---|---|---|---|
| pgo2 (byte copy of pgo) | 1.002 | 1.002 | 1.000 | | | | noise floor |
| BOLT (`BOLT_BASE`, period 200003 x3, 148 M executed instr. in the profile) | 1.000 | 0.999 | 0.999 | jit-trace 1.002 | pcall_error 1.048, lib pcall_err 1.131 | apps markdown 0.948 | no gain |
| BOLT (period 100003 x6, 604 M instr., 4x denser) | 0.999 | 0.997 | 0.999 | programs 1.001 | find_plain 1.076 | apps markdown 0.945 | no gain (apps 0.981, the rest 0.995-1.001) |
| `-C target-cpu=native` (ceiling, NOT portable) | 1.005 | 1.004 | 1.001 | lib 1.008 | m_random 1.138 | sieve 0.936 | no gain (slower) |
| transparent huge pages | not built; see below | | | | | | ceiling < 0.5% |

Gate (>= 3% geomean, no set worse than +2%): **no variant qualifies; nothing shipped**. Per-set tables: `docs/bench-bp-variant-*.md`.
* **BOLT**: `llvm-bolt`/`perf2bolt`/LBR (`perf record -j any,u`) are installed and work (LLVM 23.1.4). `scripts/bolt.sh` with `BOLT_PGO=1` builds the
  PGO base without the JIT, so the trial used the PGO profile re-linked with `--emit-relocs` and `BOLT_BASE`. The BOLT binaries pass jit-diff (seeds 1..40,
  hook none, modes `off tr trdef`: 0 diffs, 31 of 40 seeds with traces, 543-848 traces installed) and the official suite
  (`scripts/jit-official.sh`, `RUSTYNAIL_JIT=always`: OK). Reason for no gain: PGO already lays out hot/cold code per function, the hot loop is JIT code
  (not in the ELF), and the interpreter is cold for the sets that matter. `scripts/bolt.sh` keeps working; only its header comment was extended.
* **target-cpu=native**: same instructions (1.001), cycles +0.5%: the Rust runtime has nothing AVX2/Zen 5 can use; the JIT emits its own x86-64 code.
* **Huge pages**: `/sys/kernel/mm/transparent_hugepage/enabled` is `always` here, so large anonymous heap arenas already get THP wherever they are 2 MiB aligned.
  The JIT code arena cannot use them: it is 1 MiB `mmap` chunks with per-page `mprotect` (W^X toggling), which splits a huge page by design. Instead of a
  scratch patch the ceiling was measured: user-mode `iTLB-load-misses` 0.9-2.6 k and `dTLB-load-misses` 3.6-115 k per run (havlak 1.7 k / 19.7 k in
  146 Mcyc; raytrace 1.2 k / 3.6 k in 141 Mcyc; cd 2.6 k / 8.1 k; lru_cache 0.9 k / 8.8 k; binary_trees 1.7 k / 115 k in 1883 Mcyc). Even at 100 cycles
  per page walk that is at most 0.6% of the cycles (binary_trees), 0.1-0.3% elsewhere, before any hit-rate benefit; far under the 3% gate. Not built (a patch
  would add an env knob and a 2 MiB-aligned arena for < 0.5%).

### PGO against plain (all 173 programs, default mode; `docs/bench-bp-pgo-vs-plain.md`)

| set (n) | pgo/plain cycles min | median | instructions | worst program | best program |
|---|---|---|---|---|---|
| awfy (14) | 0.959 | 0.960 | 0.960 | mandelbrot 0.999 | json 0.891 |
| apps (5) | 0.899 | 0.900 | 0.922 | markdown 0.928 | raytrace 0.855 |
| programs (25) | 0.931 | 0.931 | 0.927 | **sieve 1.056** | json 0.804 |
| jit-trace (29) | 0.989 | 0.991 | 0.990 | tfor_loops 1.002 | alloc_ctor 0.895 |
| lib (95) | 0.928 | 0.926 | 0.917 | **pcall_ok 1.355** | tsort_str 0.641 |
| real (5) | 0.886 | 0.888 | 0.900 | game_loop 0.979 | template_engine 0.818 |
| all (173) | **0.939** | 0.938 | 0.933 | | |

This reproduces BUNDLE-A's numbers (awfy 0.953, apps 0.877, programs 0.929, lib 0.927, jit-trace 0.991) on the current tree, and shows the two
regressions of PGO: `lib/pcall_ok` (160 -> 217 Mcyc, +35%, but instructions only +3%: 1176 -> 1212 M, i.e. an IPC effect of code layout / branch prediction, not extra work) and `programs/sieve` (+5.6% cycles, +6.6% instructions). Next in `lib`: `str_eq_long` +30%, `str_lt` +30%, `m_type` +12% (all
micro-rows; the worst program of every other set is within +0.2%). `bench/lib` is in the training list, so these rows were trained: it is a code-generation/layout effect of the profile, cause not investigated.

## 6. Startup (ms wall, `taskset -c 4`, min / median of 100; `docs/bench-bp-startup.md`)

| program | PUC | LuaJIT | plain | PGO |
|---|---|---|---|---|
| hello.lua | 0.65 / 0.71 | 0.69 / 0.77 | 0.75 / 0.82 | 0.73 / 0.85 |
| ten_ms.lua | 1.21 / 1.29 | 1.14 / 1.21 | 1.34 / 1.45 | 1.30 / 1.45 |

Our startup is ~0.1 ms (15%) behind PUC and ~0.06 ms behind LuaJIT; not a user-visible difference.

## 7. Memory (peak RSS = ru_maxrss min of 3 (awfy, apps, real) / 2 (programs); heap = `collectgarbage("count")` at the end of the program)

Raw: `docs/bench-bp-rss.md`, `docs/bench-bp-rss-programs.md` (15 of 25 `programs` rows; ten rows with a failing LuaJIT variant are dropped),
`docs/bench-bp-rss-pacing.md`. Geomean RSS ratio to PUC: awfy + apps + real: **LuaJIT 1.34, ours (plain) 2.09, ours (PGO) 2.16**;
programs: LuaJIT 1.21, ours 1.44 / 1.46. Selected programs (MiB RSS):

| program | PUC | LuaJIT | ours plain | ours PGO | ours with the reference's young-GC pacing (`minormul=20`) |
|---|---|---|---|---|---|
| awfy/bounce (small) | 2.4 | 3.2 | 5.2 | 5.4 | |
| awfy/havlak | 16.0 | 19.3 | 16.8 | 17.0 | 16.6 |
| awfy/storage | 4.6 | 5.8 | 8.2 | 8.4 | 7.0 |
| apps/lru_cache | 4.6 | 6.5 | 11.4 | 12.0 | 7.3 |
| apps/raytrace | 2.6 | 3.4 | 6.8 | 7.0 | 7.2 |
| apps/markdown | 3.2 | 4.3 | 7.1 | 7.3 | 6.4 |
| programs/binary_trees | 43.0 | 55.3 | 46.6 | 46.8 | 44.2 |
| programs/gc_churn | 42.3 | 54.1 | 45.7 | 46.1 | 42.3 |
| programs/string_ops | 79.2 | 171.8 | 97.0 | 97.5 | 93.8 |
| programs/sieve | 148.4 | 387.1 | 150.9 | 151.1 | |
| programs/knucleotide | 76.4 | 76.7 | 81.6 | 81.9 | |

Honest reading: (1) a fixed ~2.5 MiB floor over PUC (the binary, JIT code arena chunks, trace metadata) dominates the small programs and makes the
geomean 2.1x; on programs with a real heap we are +1..10% over PUC (binary_trees +9%, gc_churn +9%, knucleotide +7%, string_ops +23%) and below LuaJIT.
(2) The Bundle A GC pacing (nursery = up to 20x `minormul`% of the live heap, 4 MiB floor on small heaps) costs +0.4..4.7 MiB of peak RSS on programs with a
small live set that allocate heavily (lru_cache +4.7 MiB = +64%, storage +2.9 MiB, binary_trees +2.6, gc_churn +3.8, string_ops +3.7) for -4..-7% of the cycles; incremental mode
(`RUSTYNAIL_GC_MODE=inc`) is not smaller (havlak 22.2, string_ops 135.9 MiB). The end-of-run `collectgarbage("count")` (heap columns of the raw files) is within a few MiB of PUC's (binary_trees 14.8 both, havlak 7.8 vs 11.2, storage 2.3 vs 0.6, lru_cache 3.2 vs 1.2). (3) A user who cares about memory over speed can pin the parameter: `collectgarbage("param","minormul",20)` restores the
reference pacing exactly.

## 8. Training run, PGO regeneration and verification (task 1)

`scripts/pgo-build.sh ~/.cache/bp-pgo` on main 8c5f4603: **1 min 18 s wall in total** (instrumented build 27 s, training ~26 s, final build 25 s; the
README's "about 10 minutes" is a clean-checkout figure with a cold dependency build), **training: 181 runs, 0 failed** (re-timed alone: 26.0 s wall, same count);
merged profile ~2 MB, binary 18.3 MB (debug info included; stripped BOLT binary 9.2 MB). Log: `docs/bench-bp-pgo-build.log`.
Verification of the PGO binary: jit-diff seeds 1..40, hook none, modes `off tr trdef`, `JITDIFF_ENTERS=1`: **0 diffs** (31 of 40 seeds with traces, 844 / 545 traces
installed, 668 904 / 559 259 trace entries; `docs/bench-bp-jitdiff-pgo.txt`); official suite through `scripts/jit-official.sh` (`RUSTYNAIL_JIT=always`, every file
except `all` / `heavy`): `JIT-OFFICIAL OK`. The BOLT binary passes the same two checks.

## 9. What is not covered

Lua 5.4 (not benchmarked), full mutant runners, ASan/Miri, fmt/clippy, 24 h fuzzing (the release phase, out of scope here). Medians over RUNS 11 of a
single machine; `lib` rows are micro-benchmarks (one library call per iteration) and dominate the count of programs over 2x LuaJIT; the geomeans over all
173 programs weigh them like whole programs. The PGO speedups are on benchmark sets that are in the training list (BUNDLE-A: hold-out training gives ~0.96 on awfy / programs instead of 0.953 / 0.929).

## 10. Product-code suggestions (nothing was changed)

* `programs/sieve` (+5.6%) and `lib/pcall_ok` (+35%) are PGO regressions at nearly equal instruction counts (also `str_eq_long`, `str_lt` +30%): compare branch-misses / front-end stalls of the plain and PGO binaries with `perf stat` before touching code; a PGO regeneration with `PGO_REPEAT=2` is the cheap first experiment.
* Biggest remaining levers by the numbers above: tier-1 call/return frame (towers, events, cd; ceiling under the gate in Bundle D, i.e. only worth it with other call-path work), allocation + compile cost in raytrace/cd (`other` 23-26% + compile 13-16%), recursion cost in traces (fib, rec_loop), and `pcall`/`string.format('%s')`-style intrinsics in traces.
