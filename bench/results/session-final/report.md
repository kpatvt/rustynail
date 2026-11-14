# Final benchmark (session-final): HEAD 108b935, Lua 5.5 and 5.4, plain and PGO

Data: `tables.md`, `vs_baseline.md`, `extra.md`, `per_program_pass{A,B,54}.csv`, raw JSONs, logs, `build_logs/`, `release_script_check.txt`, `machine_state.txt`.
Method as in session-bm5: min of 5 interleaved runs, `perf stat cycles:u,instructions:u`, `RUSTYNAIL_SEED=0`, equal-length binary paths, one script path for every column, 15-run serial re-runs on CPU 4 of every program flagged more than 3% worse, then a path-length sweep (+1, +5, +12, +29 characters) for those that stayed flagged.
Pass A: 223 programs, 12 columns, CPUs 3-7. Pass B (5.5 vs LuaJIT, 219 programs with a LuaJIT variant, plus PUC and the baselines) and pass C (5.4, 9 columns): CPUs 4 and 6.
All our outputs (5.5 and 5.4; plain, PGO, JIT off, nojit build; both baseline builds) equal PUC and `.expected` on all 223 programs. LuaJIT differs from PUC only on 5 programs whose LuaJIT variants print differently; 4 programs have no LuaJIT variant.
`scripts/release-build.sh` produced all eight shipped builds (`sha256sum -c` OK, stripped). PGO was re-made fresh for every PGO column; training had 0 failed runs (181 / 181 / 179 / 181 runs).
Baselines rebuilt the same session: 9a9b8ed (the bm5 subject) and 8a18eb3 (the commit of the previous release table), plain and PGO.

## Headline: geomean cycles, below 1 means ours is faster

| build | ours/LuaJIT 2.1 (219) | ours/PUC, same version (223) |
|---|---|---|
| 5.5 plain | 0.965 | 0.222 |
| 5.5 PGO (the release build) | 0.880 | 0.202 |
| 5.4 plain | 1.021 | 0.229 |
| 5.4 PGO | 0.934 | 0.210 |

- Without real and real_dis (210 programs): 0.956 plain and 0.874 PGO against LuaJIT.
- JIT gain (plain / JIT off): 0.300 on 5.5 (3.3x faster), 0.309 on 5.4 (3.2x).
- Against LuaJIT with its JIT off (`-joff`, pass A): our plain JIT build 0.329, our PGO build 0.300; our interpreter alone (JIT off) 1.090.
- 5.4 costs about 1.06x the cycles of our 5.5 build on the same programs; its cost relative to its own reference is 1.034 (bm5: 1.048).
- Pass A agrees with pass B: plain/LuaJIT 0.969, PGO/LuaJIT 0.885. Pass A runs five CPUs in parallel and reads about 1% slower for every column alike.
- Every 5.5 program is faster than PUC 5.5 in the plain build; the slowest, `lib/os_clock`, is at 0.98. With PGO `lib/os_clock` is 1.009 (on par); no other program is at or above 1.

## Per-group geomeans (5.5, pass B)

| group | n | plain/LuaJIT | PGO/LuaJIT | plain/PUC | PGO/PUC |
|---|---|---|---|---|---|
| apps | 5 | 1.266 | 1.098 | 0.538 | 0.467 |
| awfy | 14 | 1.433 | 1.342 | 0.273 | 0.256 |
| corpus (old) | 18 | 1.084 | 0.951 | 0.438 | 0.384 |
| corpus2 dev | 19 | 1.194 | 1.050 | 0.423 | 0.372 |
| corpus2 heldout | 13 | 1.254 | 1.137 | 0.419 | 0.380 |
| jit-trace | 29 | 0.881 | 0.869 | 0.070 | 0.069 |
| lib | 91 | 0.835 | 0.754 | 0.200 | 0.182 |
| programs | 25 | 0.896 | 0.825 | 0.215 | 0.198 |
| real | 5 | 1.212 | 1.054 | 0.451 | 0.393 |
| real_dis | 4 | 1.149 | 0.985 | 0.566 | 0.485 |

n counts programs with a LuaJIT variant; the PUC columns use every program of the group. corpus2 rows include their real_dis ports; `tables.md` has the rows without them. corpus and corpus2 are not in the PGO training set.

## vs previous builds (pass A, 223 programs)

| subject vs | plain cycles | plain instr | PGO cycles | PGO instr |
|---|---|---|---|---|
| 9a9b8ed (bm5) | 0.998 | 1.000 | 0.999 | 1.000 |
| 8a18eb3 (previous release table) | 0.956 | 0.959 | 0.959 | 0.966 |

- HEAD and 9a9b8ed perform the same on this suite; every set is within 0.99-1.01. The one real gain since bm5 is `programs/fib` at 0.889 (lazy CallInfo, C70).
- Against 8a18eb3: jit-trace 0.935, lib 0.924, apps 1.044 (because of `expr_lang`).
- 5.4 gained from the loop-variable pairing (C71): `array_rec` and `int_rec2` are now level with 5.5 (see `docs/JIT_PARKED.md`).

## Regressions (15-run values, path sweep where flagged)

| program | verdict |
|---|---|
| apps/expr_lang | +29% cycles plain, +26% PGO, +13% instructions vs 8a18eb3; identical to 9a9b8ed. Not a regression: the script-path trace-formation lottery (C69), HEAD is at or below the baseline in both states |
| corpus2/heldout/stream_closures | +7.5% cycles at -3.4% instructions vs 8a18eb3: the same lottery on HEAD only (C69) |
| awfy/towers | +3 to +5% cycles at every path length (instructions +1.2% plain, +0.9% PGO): the one small real regression vs 9a9b8ed; no effect on the geomean |
| awfy/cd | +5% cycles and +5% instructions vs 8a18eb3 (also +3.8% in session-release): slightly real |
| real/template_engine (plain) | +3.4 to +4.3% at 4 of 5 path lengths at identical instructions, PGO unaffected: probably a plain-build code-layout effect, not explained |
| lib/co_yield (PGO), m_tointeger, sel_n, tonumber_base, gsub_fn | layout lottery: instructions identical, the sign flips with path length (-9% to +10%) |

The rest of the programs pass A flagged are within 2.3% at 15 runs.

## 10 worst gaps vs LuaJIT, 5.5 PGO (ours/LuaJIT cycles)

awfy/permute 2.97, apps/raytrace 2.75, programs/fib 2.65, corpus/lua_lexer 2.46, corpus/classic_vector 2.35, awfy/cd 2.23, lib/fmt_s 2.16, lib/byte_multi 2.13, awfy/towers 2.11, corpus2/heldout/stream_closures 2.06.

## 10 biggest wins vs LuaJIT, 5.5 PGO (programs of at least 20 Mcyc)

lib/sel_loop 0.046, lib/error_tbl 0.071, lib/pcall_err 0.128, jit-trace/int_rec2 0.135, programs/pcall_error 0.146, lib/tfront_1k 0.186, lib/tonumber_flt 0.217, lib/it_next 0.242, lib/match 0.275, corpus/sha2_base64 0.293.

## Startup and size (empty script)

| | cycles:u | wall (hyperfine, min) | stripped `rnlua` |
|---|---|---|---|
| 5.5 plain | 904k | 0.465 ms | 2,976,768 B |
| 5.5 PGO | 880k | 0.469 ms | 3,346,680 B |
| 5.4 plain | | 0.465 ms | 2,792,536 B |
| 5.4 PGO | | | 3,134,200 B |
| JIT off (5.5) / nojit build | 878k / 865k | | 1,207,992 B (nojit) |
| PUC 5.5 | 696k | 0.266 ms | `lua` 341,088 B |
| LuaJIT | 801k | 0.273 ms | |

- 9a9b8ed started in 941k cycles (0.491 ms): the lazy pattern cache (d56d844) gave 4%.
- Wall-clock startup of PUC and LuaJIT varied by about 20% between runs (PUC 5.5 minimum 0.266 ms in one run, 0.324 ms in another): the cycle counts are the reliable figures.
- `rnluac` is 554,856 B on 5.5. Empty-state memory (VmHWM): 4.06 MB for our 5.5 plain build, 2.35 MB for PUC 5.5.

## What weakens the numbers

- The machine is a live desktop (browser, 3D app, KDE, other agent sessions). Idle checks at the start and end of every pass, re-run and sweep showed every pinned CPU and its sibling under 3.1% busy, but those processes still share caches and the power budget.
- Single-program 5-run noise is about +-3%: 7 programs differ by more than 3% between the two identical `dup` columns.
- The first JIT-off hyperfine startup reading (0.329 ms) is the same anomaly as in bm5; it did not reproduce in three alternating re-measurements (0.49 ms with the JIT on and off). The original file is kept.
- The script path differs from bm5's, so absolute counts differ slightly from that run.
- LuaJIT is the version installed on the machine (see `machine_state.txt`).
