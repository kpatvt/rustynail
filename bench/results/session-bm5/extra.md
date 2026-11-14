# Extras, session-bm5

## Baseline identity
session-release reported HEAD 8f422153, which is not a commit of this repository any more (history was squashed into 8a18eb3 on 2026-11-11, with later commits on top). The same-session baseline column is therefore 8a18eb3, the squashed first commit, rebuilt with the same release script (plain and PGO, 5.5). It is the closest equivalent that exists; it may contain small differences from 8f422153 (the squash is dated after the session-release run). Cross-session numbers against session-release are indicative only (also LuaJIT was updated between the sessions: 2.1.1793383622 -> 2.1.1794386141).

## lib/char under PGO: fixed
session-release: PGO 263 Mcyc vs plain 178 (store-forwarding stall in the string.char helper). Commit 0993a47 (helper reads its fresh bytes byte-wise): this session plain 154 Mcyc, PGO 139 Mcyc in pass A (PGO/plain no longer in the regression list; PGO/baseline 0.523, plain/baseline 0.870).

## apps/expr_lang: a real regression vs the baseline
15 serial runs on CPU 4: plain 192 vs 150 Mcyc (1.280x), PGO 169 vs 134 (1.256x); instructions +13.4% (plain) / +12.8% (PGO) on both, dup 1.001. Not layout lottery. Not bisected here (the commits between 8a18eb3 and 9a9b8ed number 59; candidates are the trace-compiler changes of 2026-11-12/13: loopanymax 4 -> 48, sinkside=2, cloreuse, randinl, TNEXT inline). It was already the largest regression in session-release (+2.0% instructions then). Worth a bisect.

## Other vs-baseline observations (details in vs_baseline.md)
- Layout lottery (instructions equal within 0.1%, cycles >3% off in plain, 15-run confirmed): lib/fmt_q +13.7%, programs/coroutines +7.3%, lib/str_lt_short +7.8% (PGO +8.9%), jit-trace/alloc_ctor +6.6% (PGO -11%: same instruction count), lib/sel_n +4.9% (PGO +6.5%), lib/co_yield +2.7%.
- corpus2/heldout/stream_closures: cycles +6.7% plain / +6.0% PGO while instructions are -3.7% / -4.9%: an IPC drop, not diagnosed (the layout-lottery marker does not apply since instructions moved).
- awfy/cd +3.3% cycles, +4.0% instructions (same as session-release, 3.8%): slightly real.
- Biggest wins vs baseline (instructions move with cycles): lib/m_type 0.19 (math.type constants, 4366af4), lib/m_random 0.34 and m_random_f 0.38 (randinl), lib/it_pairs / it_next / it_next_empty 0.40 (inline TNEXT), jit-trace/tail_loop 0.46, lib/it_pairs_hash 0.48, jit-trace/tfor_loops 0.51, rec_loop 0.67.
- Suite totals: plain/baseline 0.958 cycles (0.959 instructions), PGO/baseline 0.959 (0.966); jit-trace 0.937 and lib 0.925 gain most, apps (expr_lang) +3.6% plain is the only set above 1.

## 5.4 build: relative cost table (relative = (p54/PUC54)/(p55/PUC55), same programs)
Net: ALL (223) p54/PUC54 0.232 vs p55/PUC55 0.221, so 5.4 is now 1.048x worse relative to its reference (session-release: 1.157). By set: lib 1.055 (was 1.185), programs 1.048 (1.229), corpus 1.044 (1.185), corpus2 dev 1.069 (1.121), heldout 1.074 (1.131), awfy 1.025 (1.081), jit-trace 1.020 (1.089), apps 1.015 (1.090), real 1.033 (1.058), real_dis 1.027 (1.050). PUC 5.4 is 1.023x slower than PUC 5.5 on these programs. Our 5.4 build costs 1.071x our 5.5 build (cycles; 1.076x instructions); PGO 1.077x.
JIT gain: plain/JIT-off 0.301 on 5.5 (pass A) and 0.312 on 5.4 (pass C), geomean over all 223 (3.3x / 3.2x).

| item | session-release rel | now rel | now p54/PUC54 | cause (commits of 2026-11-12) |
|---|---|---|---|---|
| lib/rawget_set | 11.5 | 0.95 | 0.053 | rawget/rawset recorded on 5.4 (e386626) |
| lib/it_next | 2.2 | 0.95 | 0.112 | next(t,k) recorded on 5.4 (1b4d168) |
| lib/len_tbl | 1.8 | 1.31 | 0.509 | inline #t on 5.4 (d585226); remainder open |
| lib/tinsert_append | 1.5 | 1.13 | 0.333 | same |
| lib/tpack | 1.4 | 1.46 | 0.456 | unchanged |
| lib/tfront_1k | 8.6 | 1.09 | 0.019 | integer keys of the hash part on 5.4 (9dc4279) |
| jit-trace/int_rec2 | 3.4 | 2.11 | 0.115 | partly fixed; still 2x |
| programs/sieve | 3.0 | 0.64 | 0.122 | empty array slot stores on 5.4 (9dc4279) |
| lib/tmove / tunpack | 2.7 / 2.3 | 1.21 / 1.23 | 0.183 / 0.312 | table.move / unpack recorded on 5.4 (542a3f8) |
| jit-trace/array_rec | 2.5 | 2.43 | 0.451 | unchanged (instructions p54/PUC 0.129 vs 0.080) |
| programs/table_array | 2.1 | 1.11 | 0.111 | 9dc4279 |
| corpus/class_middleclass | 6.5 | 1.04 | 0.164 | metamethod / int-key __index paths (9dc4279, 83e658d) |
| programs/metamethods | 6.1 | 1.17 | 0.179 | same |
| programs/load_big | 2.4 | 2.45 | 1.032 | unchanged, not root-caused (p54/PUC54 1.03: nothing gained; g54 0.75) |

Remaining largest 5.4 gaps (pass C, rel >= 1.8): programs/load_big 2.45, jit-trace/array_rec 2.43, lib/concat_op_str 2.21, jit-trace/int_rec2 2.11, programs/heapsort 1.83, corpus2/dev/regex_nfa 1.80. Relatively faster on 5.4: programs/sieve 0.64, lib/m_max 0.78, jit-trace/short_inner 0.79, apps/expr_lang 0.80, lib/m_min 0.83, awfy/queens 0.83, int_sum 0.83, mandelbrot 0.85.
The causes in the last column are inferred from the commit titles and the items they target, not re-verified in the sources this session.

## Outputs
Every column of passes A, B and C (5.5 plain, PGO, JIT off, nojit build, baseline plain and PGO; 5.4 plain, PGO, JIT off, nojit build) equals PUC (same version) and `.expected` on all 223 programs. Differences only for LuaJIT (corpus/testrunner_luaunit, corpus2/dev/real_dis_arm differ from PUC; lib/m_tointeger and a few others have no LuaJIT variant).
