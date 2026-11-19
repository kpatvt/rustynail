# Extras, session-nbody (HEAD 1a5aad0)

Data: `tables.md` (pass B/A/C tables, per-program, re-runs), `vs_baseline.md` (vs 108b935 same session and vs stored session-final, improvements, regressions, nbody), `per_program_pass{A,B,54}.csv`, `extra_calc_output.txt`, raw JSON/logs, `js_vs_node.{json,log}`, `machine_state.txt`, `build_logs/` (scripts, build-log tails).
Method as in session-final (see `machine_state.txt` for the deviations). Baseline = 108b935 (the session-final subject) rebuilt in this session (columns b1p/b1g); SF = session-final's stored numbers.
Outputs: every ours column (5.5 plain, PGO, JIT off, nojit build, b1 plain/PGO; 5.4 plain, PGO, JIT off, nojit build) equals PUC (same version) and `.expected` on all 223 programs. LuaJIT differs from PUC only on the same programs as in session-final (corpus/testrunner_luaunit, corpus2/dev/real_dis_arm, ... variants print differently; 4 programs have no LuaJIT variant: 219 with LuaJIT).

## Headline (geomean cycles, below 1 = ours faster; pass B for LuaJIT; pass C for 5.4)

| column | ours/LuaJIT (219) now | SF | ours/PUC same version (223) now | SF | ours/LuaJIT instr now | SF |
|---|---|---|---|---|---|---|
| 5.5 plain | 0.958 | 0.965 | 0.220 | 0.222 | 1.385 | 1.418 |
| 5.5 PGO | 0.874 | 0.880 | 0.201 | 0.202 | 1.279 | 1.310 |
| 108b935 plain / PGO rebuilt now | 0.964 / 0.878 | 0.965 / 0.880 | 0.221 / 0.202 | 0.222 / 0.202 | 1.416 / 1.307 | |
| 5.4 plain (pass C) | 1.017 | 1.021 | 0.228 | 0.229 | | |
| 5.4 PGO (pass C) | 0.931 | 0.934 | 0.209 | 0.210 | | |

- The same-session rebuild of 108b935 reproduces session-final within 0.001 on every geomean (0.964 vs 0.965, 0.878 vs 0.880): the data are comparable across the two sessions. Instruction counts per program match too (subject/b1 and subject/stored-SF instruction ratios agree to 3 digits on all flagged programs).
- Pass A (CPUs 3-7): plain/LJ 0.962, PGO/LJ 0.877. Without real and real_dis (210): 0.948 plain, 0.867 PGO against LuaJIT (SF 0.956, 0.874).
- vs LuaJIT -joff (219): our plain 0.326, PGO 0.298, our JIT-off interpreter 1.084 (SF 0.329, 0.300, 1.090).
- JIT gain (plain / JIT off): 0.300 on 5.5; 0.305 on 5.4 (SF 0.300 / 0.309). Our 5.4 build costs 1.057x our 5.5 build in cycles (1.061 instructions); relative to the references 1.033 (SF 1.034).
- Pass A vs pass B, same column: geomean 1.0005-1.0012 for ours (SF had +1.0%: no CPU-set effect this time), up to 6 programs >= 30 Mcyc differ by more than 5% between the passes per column.
- Dup column (plain run twice, pass A): geomean 1.000, 5 programs differ by more than 3% (max 1.035, min 0.954).

## vs session-final (same-session rebuild of 108b935, pass A, 223 programs)

| | cycles | instructions | cycles >3% faster / within / >3% slower | instructions >3% fewer / more |
|---|---|---|---|---|
| plain / b1p | 0.994 | 0.979 | 39 / 158 / 26 | 54 / 12 |
| PGO / b1g | 0.996 | 0.979 | 35 / 168 / 20 | 57 / 13 |

Per set (plain cycles / instructions vs b1): jit-trace 0.992 / 0.910, programs 0.971 / 0.955, lib 0.985 / 0.983, awfy 1.004 / 0.984, corpus2 dev 1.003 / 1.002, corpus 1.019 / 1.009, corpus2 heldout 1.028 / 1.050, apps 1.035 / 1.019, real 1.045 / 1.024, real_dis 1.014 / 1.015. So: the overall geomean moved only 0.6% in cycles (2.1% in instructions) and the gains are concentrated in float/field-heavy kernels (nbody, jit-trace, sel_*), while about a dozen object/method-oriented programs got worse (below).

### 15 biggest improvements (plain, cycles vs b1, programs >= 20 Mcyc; PGO is nearly identical)

| program | cycles | instructions |
|---|---|---|
| programs/nbody | 0.662 | 0.684 |
| awfy/nbody | 0.783 | 0.769 |
| lib/type_call | 0.805 | 0.826 |
| jit-trace/field_method | 0.839 | 0.739 |
| lib/sel_n | 0.857 | 0.791 |
| lib/sel_loop | 0.883 | 0.829 |
| lib/len_op | 0.884 | 0.961 |
| jit-trace/oop_override | 0.889 | 0.871 |
| lib/sel_count | 0.907 | 0.792 |
| corpus2/dev/diff_levenshtein | 0.911 | 0.931 |
| lib/string_key_const | 0.923 | 0.929 |
| jit-trace/inline_leaf | 0.925 | 0.729 |
| lib/byte | 0.929 | 0.932 |
| programs/fannkuch | 0.931 | 0.935 |
| jit-trace/int_sum | 0.935 | 0.754 |

By instructions the largest drops are programs/nbody 0.684, jit-trace/field_rw 0.726 (cycles 0.997: no cycle gain), jit-trace/inline_leaf 0.729, jit-trace/field_method 0.739, jit-trace/int_sum 0.754, awfy/nbody 0.769, jit-trace/float_poly 0.770 (cycles 0.994), lib/sel_n 0.791, lib/sel_count 0.792. Several large instruction drops did not turn into cycles (field_rw, float_poly, int_sum -6.5% only): IPC fell.

### Regressions: programs more than 3% worse in instructions (deterministic) vs 108b935, 5.5 plain (PGO equal within 1%)

All are "real" (cycles move with instructions); 15-run serial CPU-4 values:

| program | Mcyc | instr plain / PGO | cycles 15-run plain / PGO |
|---|---|---|---|
| corpus2/heldout/roguelike_turns | 1199 | 1.873 / 1.897 | 1.466 / 1.472 |
| corpus2/dev/oop_mixins | 1743 | 1.163 / 1.171 | 1.131 / 1.127 |
| corpus/classic_vector | 714 | 1.130 / 1.129 | 1.116 / 1.117 |
| real/game_loop | 43 | 1.117 / 1.117 | 1.254 / 1.257 |
| corpus/astar_jumper | 1826 | 1.115 / 1.122 | 1.068 / 1.072 |
| awfy/cd | 322 | 1.107 / 1.113 | 1.191 / 1.198 |
| awfy/deltablue | 117 | 1.078 / 1.083 | 1.120 / 1.126 |
| awfy/richards | 61 | 1.066 / 1.071 | 1.066 / 1.062 |
| corpus2/heldout/prototype_objects | 1650 | 1.065 / 1.070 | 1.045 / 1.080 |
| corpus2/dev/real_dis_x86 | 1624 | 1.047 / 1.049 | 1.038 / 1.022 |
| apps/expr_lang | 205 | 1.041 / 1.046 | 1.060 / 1.062 |
| apps/lru_cache | 156 | 1.033 / 1.035 | 1.028 / 1.023 |
| lib/tsort_cmp (PGO only) | 256 | 0.998 / 1.031 | 0.963 / 0.975 |

Smaller instruction increases (1.5% to 2.5%), cycles +3 to +4%: awfy/havlak (1.022), awfy/json (1.018), corpus2/dev/coro_generators (1.017), apps/raytrace (1.020; cycles 1.038 / PGO 1.046). The ones that matter most are roguelike_turns (+87% instructions, +47% cycles; also ours/LuaJIT went from 1.22 to 1.87), game_loop (ours/LuaJIT 1.47 -> 1.85), awfy/cd (2.23 -> 2.59), classic_vector (2.35 -> 2.60). These are all object/field-store/method-call programs: consistent with guard additions (S4 shape guards, HSTORE guard) costing instructions where they do not pay back.
Cycles >3% worse at equal or lower instructions (IPC loss, 15-run values): jit-trace/sink_vec (15 Mcyc, plain +33% cycles, instructions -12%), jit-trace/inline_2 (+8% cycles, instructions -16%), corpus/class_middleclass (+7% cycles, instructions -7%): the instruction count dropped but cycles rose.
Layout lottery (instructions identical; sign/size varies with binary path length, sweep below): lib/fmt_q plain +11% to +14% at every path length (PGO flat 0.98-1.00; instructions 1.000), corpus/parser_combinator plain +4 to +5% at every path length (PGO +1 to +3%), lib/tunpack plain +3.5 to +4.4% at every path length (instructions -0.5%), lib/tonumber_bad PGO +4 to +8% at every path length (instructions +0.4%). The other swept programs (find_cap, fmt_g) flip sign with the path length.

Path-length sweep (min of 11, CPU 4; plain/b1p, PGO/b1g cycles; instruction ratios in brackets):

| program | 15-run (pad 0) | +1 | +5 | +12 | +29 |
|---|---|---|---|---|---|
| corpus/parser_combinator | 1.056, 1.006 | 1.045, 1.011 | 1.048, 1.026 | 1.043, 1.007 | 1.054, 1.013 |
| lib/find_cap | 1.041, 1.014 | 1.005, 1.004 | 1.003, 1.028 | 0.997, 0.997 | 0.955, 0.985 |
| lib/fmt_g | 1.007, 1.075 | 1.013, 1.033 | 0.990, 0.989 | 0.984, 0.989 | 0.981, 0.984 |
| lib/fmt_q | 1.140, 1.002 | 1.114, 0.999 | 1.133, 0.993 | 1.138, 0.989 | 1.119, 0.983 |
| lib/tonumber_bad | 1.015, 1.054 | 1.016, 1.077 | 1.005, 1.061 | 1.011, 1.037 | 1.017, 1.054 |
| lib/tunpack | 1.042, 0.981 | 1.035, 0.983 | 1.042, 0.980 | 1.044, 0.983 | 1.036, 0.981 |

(Instruction ratios: all 1.000 except tonumber_bad 1.000/1.004 and tunpack 0.995/0.994.) fmt_q plain, parser_combinator plain and tunpack plain are persistent at equal instructions, so they are code-layout effects of the plain build that this method cannot tell from a regression; they have no instruction cause.

## nbody (pass A 5-run min; 5.5)

| program | build | Mcyc | Minstr | ours/LuaJIT cyc | ours/LuaJIT ins | ours/PUC cyc | vs b1 cyc | vs b1 ins | SF Mcyc | SF ours/LJ |
|---|---|---|---|---|---|---|---|---|---|---|
| programs/nbody | plain | 344.8 | 2029 | 0.998 | 1.194 | 0.081 | 0.662 | 0.684 | 523.4 | 1.501 |
| programs/nbody | PGO | 344.9 | 2027 | 0.998 | 1.193 | 0.081 | 0.664 | 0.684 | 521.2 | 1.493 |
| awfy/nbody | plain | 92.3 | 557 | 0.976 | 1.193 | 0.061 | 0.783 | 0.769 | 118.3 | 1.240 |
| awfy/nbody | PGO | 91.6 | 556 | 0.969 | 1.191 | 0.061 | 0.780 | 0.769 | 117.5 | 1.235 |

nbody went from 1.50x LuaJIT's cycles (session-final) to on par (0.998; awfy/nbody 1.24 -> 0.97), ours/PUC cycles 0.081 for programs/nbody (b1 0.123). PGO adds nothing on nbody (the JIT code dominates).

## Our JIT vs node on the 10 bench/js ports (new; `js_vs_node.json`)

min of 5 interleaved runs pinned to CPU 4, perf stat cycles:u summed over all threads (node runs GC/compiler threads, all pinned to CPU 4 too), default sizes of the ports, node v26.13.1. Ratios above 1 mean ours is slower than node. Outputs equal node's on every port (heapsort differs only by tab vs space in the print). LuaJIT runs the same Lua file (bench/luajit/heapsort.lua for heapsort, LuaJIT cycles from `js_vs_node_luajit_heapsort.txt`).

| program | node Mcyc | plain Mcyc | PGO Mcyc | LuaJIT Mcyc | plain/node cyc | PGO/node cyc | plain/node wall | LuaJIT/node wall |
|---|---|---|---|---|---|---|---|---|
| binary_trees | 850 | 1947 | 1596 | 2131 | 2.29 | 1.88 | 1.78 | 1.94 |
| fannkuch | 762 | 1048 | 1062 | 1078 | 1.38 | 1.39 | 1.29 | 1.32 |
| fib | 702 | 1250 | 1251 | 472 | 1.78 | 1.78 | 1.65 | 0.67 |
| heapsort | 751 | 1039 | 1039 | 1230 | 1.38 | 1.38 | 1.33 | - |
| mandelbrot | 572 | 532 | 532 | 522 | 0.93 | 0.93 | 0.88 | 0.89 |
| matmul | 389 | 427 | 433 | 273 | 1.10 | 1.12 | 0.98 | 0.68 |
| nbody | 275 | 352 | 345 | 348 | 1.28 | 1.25 | 1.14 | 1.10 |
| queens | 1183 | 1846 | 1846 | 1896 | 1.56 | 1.56 | 1.50 | 1.52 |
| sieve | 2311 | 569 | 572 | 1002 | 0.25 | 0.25 | 0.28 | 0.43 |
| spectral_norm | 378 | 180 | 180 | 171 | 0.48 | 0.48 | 0.48 | 0.46 |

Wall times are in `tables`-style form in `extra_calc_output.txt` (nbody: node 71 ms, ours 81 ms plain / 80 ms PGO, LuaJIT 78 ms). Geomean ours/node (cycles): plain 1.07 over the ten (wall 1.00), node wins on binary_trees (GC), fib, queens, heapsort, fannkuch; ours wins on sieve (4x), spectral_norm (2x) and mandelbrot. All of these are short runs (0.05 to 0.5 s) so node's compile and startup are included; they are not steady-state numbers.

## Start-up and size (empty script; cycles:u min of 50, CPU 4)

rn55 plain 909k cycles (b1 896k), PGO 889k (b1 887k), JIT off 876k, nojit build 882k, 5.4 plain 884k / PGO 856k / nojit 852k, PUC 5.5 709k, PUC 5.4 712k, LuaJIT 804k. So 5.5 plain starts at 1.28x PUC 5.5 and 1.13x LuaJIT (cycles); HEAD is +1.4% over 108b935 plain (instructions identical, 982k).
Wall clock (hyperfine -N, 200 runs, CPU 4): this run's first measurement gave rn55 plain 0.464 ms min, PGO 0.472, nojit 0.445, 108b935 0.467 / 0.473, PUC 5.5 0.321, LuaJIT 0.365; but the three alternating JIT on/off re-measurements right after read 0.327 to 0.330 ms (on) and 0.322 to 0.361 (off) for the same binary. So the first (main) hyperfine run was inflated about 40% (the same kind of anomaly as in bm5 and session-final, there seen on the first JIT-off run), and the wall figures carry large noise; use the cycle counts. The PUC and LuaJIT numbers come from the inflated run too.
VmHWM empty state: rn55 plain 4.06 MB (b1 4.12), PGO 4.70, PUC 5.5 2.36, LuaJIT 2.93.
Stripped sizes: rnlua 5.5 plain 3,083,576 B (b1 / session-final 2,976,768: +3.6%), PGO 3,474,048 (b1 3,352,944: +3.6%), 5.4 plain 2,899,120, PGO 3,284,928, nojit 5.5 / 5.4 1,208,424 / 1,187,904; rnluac 554,872 (5.5), 534,072 (5.4); PUC 5.5 / 5.4 `lua` 341,088 / 327,536.

## Notes and what weakens the numbers

- Live desktop machine, see `machine_state.txt`; idle checks clean at every pass boundary.
- Single-program 5-run noise about +-3% (dup column); the 15-run values for 33 programs are used where available.
- The 15-run re-run list: every program whose pass-A cycles or instructions were more than 3% above b1 (plain or PGO): 33 programs. The 5.4 list (19 programs) is the 20 largest relative-5.4 costs by session-final's rule; 5.4 re-run values are in `tables.md`.
- No 9a9b8ed / 8a18eb3 columns this session.
