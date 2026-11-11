## Headline geomeans (cycles:u min of 5 interleaved runs; ratio < 1 means ours is faster / the change is a win)

Pass A = CPUs 3-7 (one program per core, all columns on the same core). Columns: plain = default release build with jit; pgo = PGO build (scripts/pgo-build.sh); interp = plain binary with RUSTYNAIL_JIT=off; cb1 = cb1af288 plain, cb1pgo = cb1af288 PGO.

| set | n | plain/PUC | pgo/PUC | plain/interp | plain/LJ (n) | pgo/LJ | plain vs cb1 | pgo vs cb1pgo | dup noise (plain/plain2) |
|---|---|---|---|---|---|---|---|---|---|
| corpus | 18 | 0.569 | 0.497 | 0.689 | 1.421 (18) | 1.242 | 0.986 | 0.990 | 1.000 |
| corpus2/dev | 19 | 0.517 | 0.448 | 0.640 | 1.452 (19) | 1.259 | 0.893 | 0.892 | 1.001 |
| corpus2/heldout | 13 | 0.493 | 0.440 | 0.552 | 1.457 (13) | 1.302 | 0.918 | 0.900 | 1.000 |
| awfy | 14 | 0.283 | 0.265 | 0.300 | 1.421 (14) | 1.330 | 1.014 | 1.002 | 1.003 |
| programs | 25 | 0.239 | 0.221 | 0.310 | 1.006 (25) | 0.928 | 0.995 | 1.002 | 0.999 |
| jit-trace | 29 | 0.080 | 0.080 | 0.086 | 1.027 (29) | 1.018 | 1.000 | 1.000 | 1.001 |
| lib | 95 | 0.288 | 0.261 | 0.406 | 1.198 (91) | 1.077 | 1.000 | 0.998 | 1.000 |
| apps | 5 | 0.586 | 0.522 | 0.791 | 1.440 (5) | 1.282 | 1.005 | 1.002 | 1.001 |
| real | 5 | 0.516 | 0.454 | 0.592 | 1.410 (5) | 1.241 | 1.012 | 1.010 | 1.001 |
| real_dis dev (2) | 2 | 0.698 | 0.603 | 0.887 | 1.329 (2) | 1.148 | 1.011 | 1.016 | 1.001 |
| real_dis heldout (2) | 2 | 0.755 | 0.631 | 0.916 | 1.613 (2) | 1.348 | 1.012 | 0.987 | 1.010 |
| real+real_dis (9) | 9 | 0.601 | 0.520 | 0.714 | 1.434 (9) | 1.242 | 1.011 | 1.006 | 1.003 |
| ALL | 223 | 0.282 | 0.257 | 0.357 | 1.223 (219) | 1.113 | 0.985 | 0.983 | 1.000 |

## ours/LuaJIT on fast Zen5 cores only (pass B: CPUs 4, 6; all columns of a program on the same core)

| set | n with LJ | plain/LJ | pgo/LJ | cb1/LJ | cb1pgo/LJ | plain/PUC | plain vs cb1 | pgo vs cb1pgo |
|---|---|---|---|---|---|---|---|---|
| corpus | 18 | 1.398 | 1.224 | 1.414 | 1.233 | 0.568 | 0.988 | 0.993 |
| corpus2/dev | 19 | 1.450 | 1.257 | 1.630 | 1.411 | 0.515 | 0.890 | 0.891 |
| corpus2/heldout | 13 | 1.472 | 1.316 | 1.585 | 1.448 | 0.498 | 0.929 | 0.909 |
| awfy | 14 | 1.210 | 1.144 | 1.202 | 1.135 | 0.283 | 1.006 | 1.008 |
| programs | 25 | 0.989 | 0.907 | 1.000 | 0.904 | 0.238 | 0.989 | 1.004 |
| jit-trace | 29 | 1.030 | 1.018 | 1.028 | 1.018 | 0.081 | 1.002 | 1.000 |
| lib | 91 | 1.196 | 1.076 | 1.195 | 1.078 | 0.289 | 1.001 | 0.998 |
| apps | 5 | 1.408 | 1.260 | 1.401 | 1.249 | 0.589 | 1.005 | 1.009 |
| real | 5 | 1.420 | 1.244 | 1.396 | 1.229 | 0.518 | 1.017 | 1.012 |
| real_dis dev (2) | 2 | 1.333 | 1.144 | 1.328 | 1.140 | 0.698 | 1.004 | 1.004 |
| real_dis heldout (2) | 2 | 1.620 | 1.358 | 1.606 | 1.368 | 0.752 | 1.008 | 0.993 |
| real+real_dis (9) | 9 | 1.441 | 1.245 | 1.424 | 1.238 | 0.601 | 1.012 | 1.006 |
| ALL | 219 | 1.207 | 1.098 | 1.225 | 1.115 | 0.282 | 0.985 | 0.985 |

### corpus

Mcyc min; ratios are cycles (ours over the other column). `chg` = plain/cb1, `chg pgo` = pgo/cb1pgo. `!` = see flags.

| program | plain | pgo | interp | PUC | LJ | plain/PUC | plain/interp | plain/LJ | pgo/LJ | chg | chg pgo | core |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| astar_jumper | 2049 | 1957 | 3461 | 3344 | 1027 | 0.613 | 0.592 | 1.995 | 1.905 | 1.005 | 1.008 | 3 |
| class_middleclass | 1018 | 1000 | 3437 | 3016 | 318 | 0.338 | 0.296 | 3.204 | 3.148 | 0.989 | 1.006 | 4 |
| classic_vector | 1592 | 1422 | 2717 | 2950 | 244 | 0.540 | 0.586 | 6.533 | 5.836 | 0.936 | 0.991 | 5 |
| containers_lru_heap | 1670 | 1624 | 2706 | 2656 | 1562 | 0.629 | 0.617 | 1.069 | 1.040 | 0.920 | 0.934 | 6 |
| coro_scheduler | 1621 | 1383 | 1884 | 2761 | 920 | 0.587 | 0.860 | 1.762 | 1.503 | 0.994 | 0.986 | 7 |
| csv_ini_yaml | 2043 | 1663 | 2170 | 2954 | 1780 | 0.691 | 0.941 | 1.147 | 0.934 | 0.997 | 0.967 | 4 |
| inspect_lua | 1890 | 1695 | 2320 | 2893 | 1055 | 0.653 | 0.815 | 1.791 | 1.606 | 0.998 | 0.999 | 6 |
| json_rxi | 2109 | 1740 | 3217 | 3660 | 1251 | 0.576 | 0.655 | 1.685 | 1.390 | 0.940 | 0.905 | 7 |
| lua_lexer | 1392 | 1243 | 3139 | 3294 | 458 | 0.422 | 0.443 | 3.037 | 2.713 | 0.988 | 1.009 | 5 |
| markdown_conv | 1597 | 1343 | 1567 | 3757 | 2981 | 0.425 | 1.020 | 0.536 | 0.450 | 1.023 | 1.003 | 3 |
| msgpack_pure | 2688 | 2226 | 2915 | 3466 | 3204 | 0.776 | 0.922 | 0.839 | 0.695 | 1.042 | 1.002 | 4 |
| mustache_lustache | 2249 | 1808 | 2653 | 3303 | 1561 | 0.681 | 0.848 | 1.441 | 1.158 | 1.007 | 1.019 | 6 |
| parser_combinator | 2376 | 1986 | 3037 | 3462 | 1592 | 0.686 | 0.782 | 1.493 | 1.248 | 0.994 | 1.008 | 5 |
| penlight_mix | 2988 | 2543 | 3004 | 4006 | 2759 | 0.746 | 0.995 | 1.083 | 0.922 | 0.997 | 0.988 | 7 |
| serpent_ser | 2599 | 2315 | 2759 | 3292 | 2456 | 0.790 | 0.942 | 1.058 | 0.943 | 0.994 | 1.014 | 3 |
| sha2_base64 | 506 | 478 | 3298 | 3367 | 1406 | 0.150 | 0.154 | 0.360 | 0.340 | 0.997 | 1.001 | 6 |
| template_etlua | 2414 | 2094 | 2253 | 2655 | 1841 | 0.909 | 1.072 | 1.311 | 1.137 | 0.949 | 1.005 | 4 |
| testrunner_luaunit | 1794 | 1464 | 1731 | 2607 | 1393 | 0.688 | 1.036 | 1.287 | 1.050 | 0.991 | 0.988 | 6 |

### corpus2/dev

Mcyc min; ratios are cycles (ours over the other column). `chg` = plain/cb1, `chg pgo` = pgo/cb1pgo. `!` = see flags.

| program | plain | pgo | interp | PUC | LJ | plain/PUC | plain/interp | plain/LJ | pgo/LJ | chg | chg pgo | core |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| dev/bignum_arith | 1063 | 988 | 2359 | 2409 | 668 | 0.441 | 0.451 | 1.590 | 1.477 | 1.008 | 1.001 | 5 |
| dev/coro_generators | 1398 | 970 | 1574 | 1851 | 727 | 0.756 | 0.888 | 1.925 | 1.336 | 1.000 | 0.997 | 4 |
| dev/csv_report | 1518 | 1453 | 1649 | 2598 | 1435 | 0.584 | 0.920 | 1.058 | 1.013 | 0.950 | 1.033 | 6 |
| dev/diff_levenshtein | 753 | 684 | 1571 | 1819 | 335 | 0.414 | 0.480 | 2.248 | 2.040 | 1.041 | 0.996 | 3 |
| dev/ecs_world | 1113 | 963 | 1775 | 2159 | 950 | 0.516 | 0.627 | 1.172 | 1.014 | 1.005 | 0.989 | 7 |
| dev/event_promise | 431 | 438 | 1391 | 2014 | 622 | 0.214 | 0.310 | 0.692 | 0.703 | 0.961 | 1.054 | 4 |
| dev/expr_compiler | 1574 | 1325 | 1895 | 2153 | 1186 | 0.731 | 0.830 | 1.327 | 1.117 | 1.048 | 1.047 | 5 |
| dev/iter_pipeline | 321 | 318 | 3196 | 3352 | 140 | 0.096 | 0.100 | 2.294 | 2.276 | 0.114 | 0.118 | 4 |
| dev/lua_parser | 1510 | 1260 | 1822 | 2520 | 840 | 0.599 | 0.828 | 1.797 | 1.500 | 1.041 | 1.010 | 3 |
| dev/memo_cache | 1804 | 1620 | 2144 | 2860 | 1737 | 0.631 | 0.842 | 1.039 | 0.933 | 0.973 | 0.981 | 6 |
| dev/oop_mixins | 2207 | 2092 | 2488 | 2605 | 967 | 0.847 | 0.887 | 2.283 | 2.164 | 1.007 | 0.994 | 7 |
| dev/particles_physics | 483 | 418 | 1284 | 1754 | 396 | 0.275 | 0.376 | 1.221 | 1.057 | 0.942 | 0.875 | 5 |
| dev/real_dis_arm | 3318 | 2833 | 3649 | 4475 | 2060 | 0.742 | 0.909 | 1.611 | 1.376 | 1.022 | 1.016 | 4 |
| dev/real_dis_x86 | 1780 | 1555 | 2057 | 2710 | 1623 | 0.657 | 0.866 | 1.097 | 0.958 | 1.000 | 1.017 | 6 |
| dev/regex_nfa | 1204 | 953 | 2316 | 2511 | 556 | 0.479 | 0.520 | 2.165 | 1.714 | 0.965 | 0.971 | 3 |
| dev/template_gsub | 2053 | 1629 | 1978 | 2899 | 2080 | 0.708 | 1.038 | 0.987 | 0.783 | 1.013 | 0.991 | 5 |
| dev/text_adventure | 2911 | 2422 | 3162 | 3358 | 1195 | 0.867 | 0.921 | 2.437 | 2.027 | 1.003 | 1.005 | 7 |
| dev/varargs_util | 1640 | 1371 | 1651 | 2258 | 1407 | 0.726 | 0.993 | 1.166 | 0.974 | 1.008 | 0.991 | 6 |
| dev/wordfreq_wrap | 757 | 613 | 938 | 1216 | 639 | 0.623 | 0.808 | 1.185 | 0.960 | 1.047 | 1.015 | 3 |

### corpus2/heldout

Mcyc min; ratios are cycles (ours over the other column). `chg` = plain/cb1, `chg pgo` = pgo/cb1pgo. `!` = see flags.

| program | plain | pgo | interp | PUC | LJ | plain/PUC | plain/interp | plain/LJ | pgo/LJ | chg | chg pgo | core |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| heldout/bigint_rsa | 1454 | 1311 | 2805 | 3043 | 1047 | 0.478 | 0.518 | 1.389 | 1.253 | 1.000 | 0.998 | 4 |
| heldout/dijkstra_routes | 1372 | 1334 | 3121 | 3175 | 1291 | 0.432 | 0.440 | 1.063 | 1.033 | 0.987 | 1.010 | 3 |
| heldout/forth_vm | 1211 | 1199 | 3232 | 2936 | 704 | 0.413 | 0.375 | 1.721 | 1.703 | 1.025 | 1.026 | 6 |
| heldout/log_analyzer | 2503 | 1988 | 2562 | 3206 | 1791 | 0.781 | 0.977 | 1.398 | 1.110 | 1.023 | 0.967 | 5 |
| heldout/markov_text | 1376 | 1268 | 2649 | 2845 | 991 | 0.484 | 0.520 | 1.388 | 1.279 | 0.783 | 0.763 | 4 |
| heldout/prototype_objects | 1990 | 1696 | 2658 | 3736 | 1885 | 0.533 | 0.749 | 1.056 | 0.900 | 0.966 | 0.842 | 6 |
| heldout/real_dis_mips | 2927 | 2442 | 3220 | 3847 | 1802 | 0.761 | 0.909 | 1.624 | 1.355 | 1.031 | 1.017 | 7 |
| heldout/real_dis_ppc | 2986 | 2497 | 3237 | 3987 | 1863 | 0.749 | 0.923 | 1.603 | 1.340 | 0.992 | 0.958 | 3 |
| heldout/roguelike_turns | 868 | 829 | 2796 | 3325 | 642 | 0.261 | 0.311 | 1.352 | 1.291 | 1.032 | 1.047 | 4 |
| heldout/scheme_interp | 2087 | 1790 | 2683 | 3112 | 1072 | 0.671 | 0.778 | 1.947 | 1.670 | 0.775 | 0.755 | 5 |
| heldout/stream_closures | 944 | 856 | 2784 | 3062 | 415 | 0.308 | 0.339 | 2.276 | 2.064 | 0.497 | 0.498 | 6 |
| heldout/sudoku_solver | 1685 | 1507 | 2854 | 3013 | 1391 | 0.559 | 0.590 | 1.211 | 1.083 | 1.003 | 1.003 | 4 |
| heldout/vending_fsm | 1131 | 1048 | 3384 | 3302 | 835 | 0.343 | 0.334 | 1.355 | 1.255 | 1.028 | 1.026 | 7 |

### awfy

Mcyc min; ratios are cycles (ours over the other column). `chg` = plain/cb1, `chg pgo` = pgo/cb1pgo. `!` = see flags.

| program | plain | pgo | interp | PUC | LJ | plain/PUC | plain/interp | plain/LJ | pgo/LJ | chg | chg pgo | core |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| bounce | 64 | 62 | 509 | 528 | 48 | 0.121 | 0.125 | 1.325 | 1.296 | 1.003 | 1.005 | 6 |
| cd | 266 | 235 | 364 | 441 | 108 | 0.603 | 0.730 | 2.458 | 2.174 | 1.087 | 1.044 | 6 |
| deltablue | 101 | 94 | 147 | 158 | 104 | 0.637 | 0.684 | 0.971 | 0.903 | 1.007 | 0.988 | 4 |
| havlak | 172 | 134 | 177 | 225 | 128 | 0.763 | 0.970 | 1.345 | 1.049 | 1.083 | 0.962 | 4 |
| json | 163 | 141 | 197 | 268 | 74 | 0.609 | 0.830 | 2.208 | 1.913 | 1.022 | 1.046 | 6 |
| list | 142 | 137 | 474 | 434 | 89 | 0.327 | 0.299 | 1.593 | 1.537 | 1.005 | 0.998 | 3 |
| mandelbrot | 277 | 277 | 1372 | 1515 | 274 | 0.183 | 0.202 | 1.013 | 1.014 | 1.000 | 0.999 | 4 |
| nbody | 122 | 121 | 1481 | 1483 | 95 | 0.082 | 0.083 | 1.281 | 1.273 | 0.999 | 0.998 | 6 |
| permute | 51 | 51 | 253 | 247 | 29 | 0.205 | 0.201 | 1.732 | 1.733 | 0.995 | 1.004 | 3 |
| queens | 111 | 108 | 410 | 397 | 56 | 0.281 | 0.272 | 2.008 | 1.950 | 1.002 | 0.996 | 3 |
| richards | 56 | 55 | 208 | 202 | 35 | 0.276 | 0.269 | 1.577 | 1.546 | 0.999 | 0.993 | 5 |
| sieve | 137 | 135 | 1109 | 1032 | 156 | 0.132 | 0.123 | 0.874 | 0.866 | 0.998 | 0.997 | 6 |
| storage | 125 | 104 | 213 | 256 | 158 | 0.489 | 0.589 | 0.790 | 0.657 | 1.003 | 1.007 | 5 |
| towers | 44 | 43 | 247 | 224 | 24 | 0.196 | 0.178 | 1.853 | 1.806 | 1.006 | 0.998 | 4 |

### real

Mcyc min; ratios are cycles (ours over the other column). `chg` = plain/cb1, `chg pgo` = pgo/cb1pgo. `!` = see flags.

| program | plain | pgo | interp | PUC | LJ | plain/PUC | plain/interp | plain/LJ | pgo/LJ | chg | chg pgo | core |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| game_loop | 36 | 34 | 211 | 214 | 22 | 0.167 | 0.170 | 1.607 | 1.543 | 0.999 | 0.984 | 4 |
| json_roundtrip | 586 | 491 | 636 | 793 | 463 | 0.739 | 0.922 | 1.267 | 1.062 | 0.981 | 1.005 | 6 |
| lua_interp | 126 | 118 | 269 | 222 | 58 | 0.569 | 0.468 | 2.164 | 2.020 | 1.060 | 1.072 | 4 |
| template_engine | 871 | 706 | 832 | 1228 | 943 | 0.709 | 1.047 | 0.923 | 0.748 | 1.020 | 0.992 | 7 |
| wordfreq | 157 | 136 | 165 | 213 | 114 | 0.735 | 0.950 | 1.371 | 1.187 | 1.000 | 1.002 | 4 |

### real_dis_* ports (corpus2)

Mcyc min; ratios are cycles (ours over the other column). `chg` = plain/cb1, `chg pgo` = pgo/cb1pgo. `!` = see flags.

| program | plain | pgo | interp | PUC | LJ | plain/PUC | plain/interp | plain/LJ | pgo/LJ | chg | chg pgo | core |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| dev/real_dis_arm | 3318 | 2833 | 3649 | 4475 | 2060 | 0.742 | 0.909 | 1.611 | 1.376 | 1.022 | 1.016 | 4 |
| dev/real_dis_x86 | 1780 | 1555 | 2057 | 2710 | 1623 | 0.657 | 0.866 | 1.097 | 0.958 | 1.000 | 1.017 | 6 |
| heldout/real_dis_mips | 2927 | 2442 | 3220 | 3847 | 1802 | 0.761 | 0.909 | 1.624 | 1.355 | 1.031 | 1.017 | 7 |
| heldout/real_dis_ppc | 2986 | 2497 | 3237 | 3987 | 1863 | 0.749 | 0.923 | 1.603 | 1.340 | 0.992 | 0.958 | 3 |

## Top 10 winners vs cb1af288 (plain/cb1 cycles, lowest first)

| program | plain/cb1 | pgo/cb1pgo | plain Mcyc | instr ratio plain/cb1 |
|---|---|---|---|---|
| corpus2/dev/iter_pipeline | 0.114 | 0.118 | 321 | 0.107 |
| corpus2/heldout/stream_closures | 0.497 | 0.498 | 944 | 0.422 |
| corpus2/heldout/scheme_interp | 0.775 | 0.755 | 2087 | 0.833 |
| programs/table_hash | 0.777 | 0.946 | 745 | 1.000 |
| corpus2/heldout/markov_text | 0.783 | 0.763 | 1376 | 0.693 |
| lib/fmt_q | 0.915 | 0.967 | 185 | 1.000 |
| corpus/containers_lru_heap | 0.920 | 0.934 | 1670 | 0.906 |
| lib/tmove_1k | 0.931 | 0.994 | 95 | 1.000 |
| corpus/classic_vector | 0.936 | 0.991 | 1592 | 0.903 |
| corpus/json_rxi | 0.940 | 0.905 | 2109 | 0.945 |

## Top 10 losers vs cb1af288 (plain/cb1 cycles, highest first)

| program | plain/cb1 | pgo/cb1pgo | plain Mcyc | instr ratio plain/cb1 | tiny (<100 Mcyc) |
|---|---|---|---|---|---|
| lib/match | 1.112 | 1.010 | 263 | 1.000 |  |
| programs/heapsort | 1.090 | 1.062 | 1230 | 1.001 |  |
| awfy/cd | 1.087 | 1.044 | 266 | 1.068 |  |
| awfy/havlak | 1.083 | 0.962 | 172 | 0.991 |  |
| programs/string_ops | 1.074 | 0.987 | 1666 | 1.000 |  |
| real/lua_interp | 1.060 | 1.072 | 126 | 0.980 |  |
| lib/tunpack | 1.054 | 0.988 | 272 | 1.000 |  |
| corpus2/dev/expr_compiler | 1.048 | 1.047 | 1574 | 1.006 |  |
| corpus2/dev/wordfreq_wrap | 1.047 | 1.015 | 757 | 1.000 |  |
| corpus/msgpack_pure | 1.042 | 1.002 | 2688 | 0.998 |  |

## Programs > 5% slower than cb1af288 (plain or pgo), with instruction ratios

| program | plain/cb1 cyc | plain/cb1 instr | pgo/cb1pgo cyc | pgo/cb1pgo instr | plain Mcyc |
|---|---|---|---|---|---|
| lib/match | 1.112 | 1.000 | 1.010 | 1.000 | 263 |
| programs/heapsort | 1.090 | 1.001 | 1.062 | 1.001 | 1230 |
| awfy/cd | 1.087 | 1.068 | 1.044 | 1.034 | 266 |
| awfy/havlak | 1.083 | 0.991 | 0.962 | 0.991 | 172 |
| programs/string_ops | 1.074 | 1.000 | 0.987 | 1.000 | 1666 |
| real/lua_interp | 1.060 | 0.980 | 1.072 | 0.979 | 126 |
| lib/tunpack | 1.054 | 1.000 | 0.988 | 1.000 | 272 |
| corpus2/dev/event_promise | 0.961 | 1.000 | 1.054 | 1.000 | 431 |

## Duplicate-column noise: programs where plain/plain2 differs by >3%

- awfy/havlak: 1.060 (plain 172 Mcyc)
- corpus2/dev/csv_report: 0.949 (plain 1518 Mcyc)
- corpus2/heldout/prototype_objects: 0.958 (plain 1990 Mcyc)
- jit-trace/sink_tuple: 1.033 (plain 13 Mcyc)
- programs/heapsort: 1.051 (plain 1230 Mcyc)
- programs/table_hash: 0.907 (plain 745 Mcyc)

## Output / .expected check

All ours/cb1 columns equal PUC and .expected on every program.

LuaJIT problems (not runnable / output differs from PUC beyond numeric tolerance):

- corpus/testrunner_luaunit: output differs from PUC
- corpus2/dev/real_dis_arm: output differs from PUC
- lib/m_tointeger: skipped (no LuaJIT equivalent)
- lib/m_type: skipped (no LuaJIT equivalent)
- lib/m_ult: skipped (no LuaJIT equivalent)
- lib/utf8_len: skipped (no LuaJIT equivalent)
- programs/coroutines: output differs from PUC
- programs/load_big: output differs from PUC
- programs/oop_methods: output differs from PUC
