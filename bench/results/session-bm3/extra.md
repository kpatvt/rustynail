## Ours/LuaJIT geomean cycles (pass B), three sessions, programs present in all three with a LuaJIT variant

| set | n | 79e7441c plain | 80f1d96e plain (bm2) | 1e6ec4ca plain (bm3) | 79e7441c PGO | 80f1d96e PGO (bm2) | 1e6ec4ca PGO (bm3) |
|---|---|---|---|---|---|---|---|
| ALL | 219 | 1.109 | 1.040 | 1.010 | 1.010 | 0.943 | 0.918 |
| ALL excl real/real_dis | 210 | 1.103 | 1.033 | 1.002 | 1.007 | 0.939 | 0.913 |
| corpus (old) | 18 | 1.230 | 1.203 | 1.157 | 1.082 | 1.058 | 1.009 |
| corpus2 dev | 19 | 1.251 | 1.225 | 1.211 | 1.088 | 1.073 | 1.059 |
| corpus2 heldout | 13 | 1.294 | 1.260 | 1.231 | 1.169 | 1.138 | 1.123 |
| awfy | 14 | 1.464 | 1.427 | 1.464 | 1.371 | 1.339 | 1.374 |
| programs | 25 | 0.928 | 0.907 | 0.903 | 0.854 | 0.838 | 0.831 |
| jit-trace | 29 | 0.995 | 0.965 | 0.964 | 0.981 | 0.957 | 0.952 |
| lib | 91 | 1.057 | 0.944 | 0.889 | 0.953 | 0.838 | 0.796 |
| apps | 5 | 1.405 | 1.246 | 1.251 | 1.214 | 1.091 | 1.091 |
| real (5) | 5 | 1.288 | 1.242 | 1.236 | 1.148 | 1.095 | 1.086 |
| real_dis (4) | 4 | 1.213 | 1.175 | 1.147 | 1.034 | 0.995 | 0.985 |

## LuaJIT-column drift bm3 vs bm2 (pass B cycles, ratio now/then), programs moving more than 8%

- apps/lru_cache: LJ 0.915 (cores 4 vs 6); ours plain bm3/bm2 0.911
- awfy/permute: LJ 0.643 (cores 4 vs 6); ours plain bm3/bm2 1.005
- programs/heapsort: LJ 0.833 (cores 4 vs 4); ours plain bm3/bm2 0.762

## Knob attribution (CPU 4, 15 runs, min cycles in Mcyc; instructions in Minstr in the second line group)

base = 1e6ec4ca plain default; libcall7 = libcall=7; t1q1amt0 = t1q=1,amt=0; retf0 = retf=0,sidenest=0,tfmt=0; rearm0 = rearmside=0; alloff = all of these; prev = 80f1d96e plain; pbase / palloff = 1e6ec4ca PGO default / all knobs off; pprev = 80f1d96e PGO

| program | base | base2 | libcall7 | t1q1amt0 | retf0 | rearm0 | alloff | prev | pbase | palloff | pprev | base/prev | alloff/prev | pbase/pprev | palloff/pprev |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| corpus/csv_ini_yaml | 1674 | 1673 | 1667 | 1667 | 1669 | 1667 | 1676 | 1633 | 1382 | 1387 | 1397 | 1.025 | 1.027 | 0.989 | 0.992 |
| corpus/parser_combinator | 2419 | 2422 | 2422 | 2416 | 2430 | 2418 | 2413 | 2226 | 1858 | 1834 | 1842 | 1.087 | 1.084 | 1.009 | 0.996 |
| corpus2/dev/regex_nfa | 742 | 737 | 741 | 684 | 743 | 742 | 684 | 706 | 592 | 574 | 565 | 1.051 | 0.970 | 1.047 | 1.015 |
| corpus2/dev/varargs_util | 1548 | 1547 | 1542 | 1523 | 1545 | 1545 | 1525 | 1501 | 1257 | 1234 | 1240 | 1.032 | 1.016 | 1.014 | 0.996 |
| lib/char | 177 | 177 | 176 | 176 | 176 | 177 | 176 | 176 | 262 | 263 | 243 | 1.001 | 0.999 | 1.078 | 1.080 |
| lib/concat_op_str | 145 | 145 | 144 | 145 | 142 | 145 | 145 | 142 | 132 | 132 | 126 | 1.021 | 1.020 | 1.044 | 1.044 |
| lib/error_tbl | 270 | 270 | 272 | 271 | 270 | 271 | 270 | 264 | 212 | 212 | 209 | 1.022 | 1.021 | 1.011 | 1.013 |
| lib/gsub_tbl | 164 | 164 | 164 | 163 | 164 | 164 | 164 | 151 | 108 | 108 | 108 | 1.085 | 1.086 | 1.003 | 1.002 |
| lib/pcall_err | 116 | 116 | 116 | 116 | 116 | 116 | 116 | 115 | 100 | 100 | 97 | 1.011 | 1.009 | 1.031 | 1.031 |
| lib/sel_loop | 258 | 252 | 254 | 232 | 252 | 257 | 230 | 236 | 257 | 233 | 234 | 1.096 | 0.976 | 1.097 | 0.995 |
| lib/tonumber_int | 137 | 137 | 137 | 137 | 137 | 137 | 137 | 134 | 141 | 141 | 142 | 1.022 | 1.024 | 0.998 | 0.998 |
| lib/tonumber_num | 213 | 214 | 212 | 194 | 214 | 214 | 194 | 197 | 213 | 197 | 194 | 1.084 | 0.985 | 1.101 | 1.015 |

Instruction ratios (base/prev, alloff/prev, pbase/pprev, palloff/pprev):

- corpus/csv_ini_yaml: 0.997 0.998 1.000 1.002
- corpus/parser_combinator: 1.002 1.002 0.998 0.998
- corpus2/dev/regex_nfa: 1.001 1.001 1.001 1.001
- corpus2/dev/varargs_util: 1.004 1.003 0.999 0.999
- lib/char: 1.000 1.000 1.004 1.004
- lib/concat_op_str: 1.000 1.000 1.003 1.003
- lib/error_tbl: 1.000 1.000 1.000 1.000
- lib/gsub_tbl: 0.991 0.991 0.998 0.998
- lib/pcall_err: 1.013 1.013 1.000 1.000
- lib/sel_loop: 1.000 1.000 1.000 1.000
- lib/tonumber_int: 1.000 1.000 0.996 0.996
- lib/tonumber_num: 1.000 1.000 1.000 1.000

## Knob split: t1q bits vs amt (CPU 4, 15 runs, min Mcyc). t1q=13 default; t1q=1 bits 4/8 off; t1q=5 = bit 4 on; t1q=9 = bit 8 on (bit 4 off)

base = 1e6ec4ca plain; amt0 = amt=0; t1qN = t1q=N; prev = 80f1d96e plain; pbase/pamt0/pt1q1 = PGO; pprev = 80f1d96e PGO

| program | base | amt0 | t1q1 | t1q5 | t1q9 | prev | pbase | pamt0 | pt1q1 | pprev |
|---|---|---|---|---|---|---|---|---|---|---|
| corpus2/dev/regex_nfa | 742 | 738 | 685 | 687 | 733 | 702 | 592 | 590 | 575 | 566 |
| lib/sel_loop | 251 | 260 | 233 | 258 | 237 | 237 | 257 | 258 | 230 | 229 |
| lib/tonumber_num | 215 | 213 | 193 | 213 | 212 | 202 | 213 | 214 | 209 | 200 |
