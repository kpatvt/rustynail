# Extras, session-final (HEAD 108b935)

Data: `tables.md` (all per-set/per-program tables, pass A/B/C, re-runs, start-up), `vs_baseline.md` (vs 9a9b8ed and vs 8a18eb3), `per_program_pass{A,B,54}.csv`, raw JSON/logs, `machine_state.txt`, `build_logs/` (scripts and build-log tails).
Outputs: every ours column (5.5 plain, PGO, JIT off, nojit build, both baselines' plain and PGO; 5.4 plain, PGO, JIT off, nojit build) equals PUC (same version) and `.expected` on all 223 programs. No mismatches. LuaJIT (and LuaJIT -joff) differ from PUC only for corpus/testrunner_luaunit, corpus2/dev/real_dis_arm, programs/coroutines, programs/load_big, programs/oop_methods (LuaJIT variants print differently); lib/m_tointeger, m_type, m_ult, utf8_len have no LuaJIT variant (4 programs: LuaJIT columns over 219).

## Headline (geomean cycles, below 1 = ours faster; pass B for LuaJIT, 5 interleaved runs)

| column | ours/LuaJIT (219) | ours/PUC 5.5 (223) |
|---|---|---|
| 5.5 plain | 0.965 | 0.222 |
| 5.5 PGO | 0.880 | 0.202 |
| 9a9b8ed plain / PGO | 0.966 / 0.881 | 0.222 / 0.203 |
| 8a18eb3 plain / PGO | 1.001 / 0.911 | 0.231 / 0.211 |
| 5.4 plain (pass C) | 1.021 | 0.229 (vs PUC 5.4) |
| 5.4 PGO (pass C) | 0.934 | 0.210 (vs PUC 5.4) |

- Pass A (CPUs 3-7) agrees: plain/LJ 0.969, PGO/LJ 0.885.
- Without real and real_dis (210): plain 0.956, PGO 0.874 vs LuaJIT.
- vs LuaJIT -joff (interpreter-only LuaJIT, pass A, 219 programs): our plain JIT build / LuaJIT-joff 0.329, our JIT-off interpreter / LuaJIT-joff 1.090 (our interpreter is 9% slower than LuaJIT's hand-written interpreter), PGO / LuaJIT-joff 0.300, LuaJIT (JIT on) / LuaJIT-joff 0.339.
- JIT gain (plain/JIT-off, pass A): 0.300 on 5.5 (3.3x), 0.309 on 5.4 (pass C). Nojit build vs JIT-off env: plain/nj 0.304.
- 5.4 relative cost: (p54/PUC54)/(p55/PUC55) = 1.034 (session-bm5: 1.048); PGO 1.038. Our 5.4 build costs 1.058x our 5.5 build (cycles; 1.061 instructions); PUC 5.4 is 1.023x slower than PUC 5.5. Remaining largest 5.4 gaps (rel): programs/load_big 2.32, lib/concat_op_str 2.20, programs/heapsort 2.06, corpus2/dev/regex_nfa 1.88 (full list in tables.md).

## vs previous (pass A, 223 programs; details in vs_baseline.md)

| subject / baseline | plain cycles | plain instr | PGO cycles | PGO instr |
|---|---|---|---|---|
| 108b935 / 9a9b8ed | 0.998 | 1.000 | 0.999 | 1.000 |
| 108b935 / 8a18eb3 | 0.956 | 0.959 | 0.959 | 0.966 |

(bm5 measured 9a9b8ed/8a18eb3 as 0.958 / 0.959 plain, 0.959 / 0.966 PGO: the same numbers, so HEAD and 9a9b8ed are performance-identical on this suite.)
Per set (plain cycles vs 9a9b8ed): all sets 0.99-1.01 (apps 1.008, awfy 1.000, corpus 0.997, corpus2 dev 0.992, heldout 1.007, jit-trace 0.998, lib 0.998, programs 0.997, real 1.002). vs 8a18eb3: jit-trace 0.935, lib 0.924, apps 1.044 (expr_lang), the rest 0.99-1.00.
Only real change vs 9a9b8ed: programs/fib 0.889 (instructions 0.890).

## Programs more than 3% worse than 9a9b8ed: re-run verdicts

Pass A (single 5-run) flagged 7 programs (plain) and 5 (PGO) against 9a9b8ed (11 distinct). m_tointeger and sel_n (flagged via the 8a18eb3 list) are listed below because they showed >3% in the re-run. All flagged programs, plus those flagged against 8a18eb3 (25 programs in total), were re-run 15 times serially on CPU 4 (`rerun55_serial_cpu4_15runs.json`), and the programs still more than 3% worse were swept over four binary-directory path lengths (+1, +5, +12, +29 characters, identical for every column; 11 runs, CPU 4; `sweep_pad*.json`). Cells: plain/9a9b8ed, PGO/9a9b8ed (instruction ratio plain/PGO).

| program | pass A plain, PGO | 15-run plain, PGO | pad +1 | pad +5 | pad +12 | pad +29 |
|---|---|---|---|---|---|---|
| awfy/towers | 1.036, 1.042 | 1.033, 1.041 (ins 1.012/1.009) | 1.041, 1.034 | 1.040, 1.051 | 1.051, 1.050 | 1.040, 1.033 |
| lib/co_yield | 0.995, 1.053 | 0.992, 1.051 (ins 1.000) | 1.007, 1.002 | 0.933, 1.062 | 1.085, 1.003 | 1.102, 1.052 |
| lib/m_tointeger | 0.998, 1.015 | 1.039, 1.052 (ins 1.000) | 0.956, 0.999 | 1.042, 0.974 | 0.970, 1.052 | 0.950, 1.057 |
| lib/sel_n | 0.949, 0.971 | 1.105, 0.973 (dup 1.047) | 0.985, 0.974 | 1.005, 0.967 | 0.909, 1.050 | 0.941, 0.918 |
| lib/tonumber_base | 1.039, 0.995 | 1.036, 0.996 (ins 1.000) | 0.998, 0.995 | 0.997, 0.996 | 0.998, 1.004 | 0.996, 0.997 |
| real/template_engine | 1.039, 1.001 | 1.036, 1.003 (ins 1.000) | 1.009, 1.001 | 1.034, 1.008 | 1.043, 1.004 | 1.043, 0.918 |
| lib/gsub_fn | 0.993, 1.030 | 0.996, 1.030 | 0.984, 1.007 | 0.987, 0.999 | 0.991, 0.995 | 0.999, 1.002 |

Verdicts:
- awfy/towers (51 Mcyc): small real regression, +3 to +5% in every run and at every path length, instructions +1.2% plain / +0.9% PGO. The only program that stays >3% worse across the sweep with an instruction increase (a ~50 Mcyc program, so geomean effect is nil). Same size vs 8a18eb3 (1.037/1.038, instructions ~equal), so it is not new since 9a9b8ed's era either.
- lib/co_yield, lib/m_tointeger, lib/sel_n, lib/tonumber_base, lib/gsub_fn: layout lottery (instructions identical to 0.0%; the sign and size of the difference flip with the path length, between -9% and +10%).
- real/template_engine plain: instructions identical, +3.4% to +4.3% at 4 of 5 path lengths (+0.9% at +1), PGO equal: most likely a persistent code-layout effect of the plain build (not explained by instructions); PGO unaffected.
- Others flagged in pass A (corpus2/heldout/prototype_objects, programs/binary_trees, lib/sel_count, lib/gsub_str, jit-trace/inline_leaf, corpus/astar_jumper, programs/load_big): noise, 15-run values within 2.3% (instructions equal).
- No regression vs 9a9b8ed that is real beyond awfy/towers.

vs 8a18eb3 (re-run values): apps/expr_lang is still the one real regression, plain 1.291 / PGO 1.261 cycles, instructions +13.4% / +12.8% (identical to session-bm5; equal to 9a9b8ed: 1.007 / 1.002, so it did not move since bm5 and was not fixed). awfy/cd +5.4% / +4.1% cycles, +5.0% / +5.7% instructions (real, small). corpus2/heldout/stream_closures +7.5% / +5.8% cycles with instructions -3.4% / -4.6% (IPC drop, as in bm5, not diagnosed). lib/str_lt_short +8.2% / +8.8%, lib/m_tointeger +6.1% / +5.6%, lib/co_yield +5.1% (PGO), programs/closures +3.5% (plain): instructions equal, layout lottery (str_lt_short and m_tointeger not swept for 8a18eb3). Biggest wins vs 8a18eb3 are the bm5 list (m_type, m_random, it_pairs/next, tail_loop, ...), see vs_baseline.md.

## Start-up and size

Empty script, cycles:u (min of 50, CPU 4; tables.md has more scripts and memory): rn55 plain 904k, PGO 880k, JIT off 878k, nojit build 865k; 9a9b8ed plain 941k / PGO 910k; 8a18eb3 plain 928k / PGO 915k; 5.4 plain 881k / PGO 851k; PUC 5.5 696k; PUC 5.4 703k; LuaJIT 801k. So 5.5 plain starts at 1.30x PUC 5.5 and 1.13x LuaJIT (cycles); 4% better than 9a9b8ed.
Wall clock (hyperfine -N, 200 runs, CPU 4, empty script, min / median): rn55 plain 0.465 / 0.483 ms, PGO 0.469 / 0.493, JIT off 0.462 / 0.478 (re-measured, see machine_state.txt), nojit build 0.447 / 0.464, 5.4 plain 0.464 / 0.481, PGO 0.468 / 0.486, 9a9b8ed plain 0.491 / 0.512, PGO 0.498 / 0.519, 8a18eb3 plain 0.491 / 0.509, PGO 0.456 / 0.508, PUC 5.5 0.266 / 0.290, PUC 5.4 0.236 / 0.256, LuaJIT 0.273 / 0.285 (PUC/LuaJIT wall numbers vary by about 20% between runs, 0.324 ms min for PUC 5.5 in the re-check).
Memory of an empty state (VmHWM): rn55 plain 4.06 MB (PUC 5.5 2.35 MB, LuaJIT 2.96 MB); collectgarbage('count') 29.3 KB (PUC 20.2, LuaJIT 48.2).

Stripped binary sizes (shipped, `scripts/release-build.sh`, bytes):

| build | rnlua | rnluac |
|---|---|---|
| 5.5 plain (108b935) | 2,976,768 | 554,856 |
| 5.5 PGO | 3,346,680 | 554,856 |
| 5.4 plain | 2,792,536 | 534,056 |
| 5.4 PGO | 3,134,200 | 534,056 |
| 9a9b8ed plain / PGO | 2,973,776 / 3,336,504 | 554,856 |
| 8a18eb3 plain / PGO | 2,939,304 / 3,303,912 | 554,632 |
| nojit build 5.5 / 5.4 (stripped copies) | 1,207,992 / 1,187,136 | - |
| PUC 5.5 / 5.4 `lua` (ref build) | 341,088 / 327,536 | - |
| LuaJIT (system package) | 22,632 (launcher; libluajit is a shared library) | - |

PGO adds 12.4% (5.5) / 12.2% (5.4) to rnlua; the JIT is about 1.77 MB of the 2.98 MB plain 5.5 binary (plain vs nojit build).

## Build/PGO notes

All eight release-script builds succeeded; `sha256sum -c` OK; PGO training 181/181/179/181 runs, 0 failed (HEAD 5.5, HEAD 5.4, 9a9b8ed, 8a18eb3). Pass A/B A/B consistency: the same column differs between pass A and pass B by +1.0% in geomean (CPU set effect), at most ~5% on single programs >= 30 Mcyc (9-18 programs per column); trust the within-pass ratios.
