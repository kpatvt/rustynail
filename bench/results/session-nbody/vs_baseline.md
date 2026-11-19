# vs session-final (108b935): same-session rebuild (b1) and stored session-final numbers

Subject = HEAD 1a5aad0. b1 = 108b935 rebuilt with scripts/release-build.sh in this session (pass A, 5-run min; 15-run serial re-run values where available). SF = stored session-final pass A (script path differs, so instruction counts differ slightly).

- plain/b1 geomean over 223 programs: cycles 0.994, instructions 0.979; cycles >3% faster 39, within 3% 158, >3% slower 26; instructions >3% fewer 54, >3% more 12
- plain vs stored SF pass A: instructions geomean 0.979, cycles geomean 0.985 over 223 programs
- PGO/b1 geomean over 223 programs: cycles 0.996, instructions 0.979; cycles >3% faster 35, within 3% 168, >3% slower 20; instructions >3% fewer 57, >3% more 13
- PGO vs stored SF pass A: instructions geomean 0.979, cycles geomean 0.984 over 223 programs

## Per set, geomean subject/b1 (pass A)

| set | n | plain cyc | plain ins | PGO cyc | PGO ins |
|---|---|---|---|---|---|
| apps | 5 | 1.035 | 1.019 | 1.032 | 1.021 |
| awfy | 14 | 1.004 | 0.984 | 1.004 | 0.984 |
| corpus | 18 | 1.019 | 1.009 | 1.019 | 1.009 |
| corpus2/dev | 17 | 1.003 | 1.002 | 1.005 | 1.003 |
| corpus2/heldout | 11 | 1.028 | 1.050 | 1.029 | 1.051 |
| jit-trace | 29 | 0.992 | 0.910 | 0.993 | 0.910 |
| lib | 95 | 0.985 | 0.983 | 0.988 | 0.982 |
| programs | 25 | 0.971 | 0.955 | 0.967 | 0.954 |
| real | 5 | 1.045 | 1.024 | 1.050 | 1.025 |
| real_dis | 4 | 1.014 | 1.015 | 1.013 | 1.016 |

## plain/b1: 15 biggest improvements by cycles (programs >= 20 Mcyc)

| program | Mcyc | cyc A | ins A | cyc 15-run | ins 15-run | ins vs stored SF | note |
|---|---|---|---|---|---|---|---|
| programs/nbody | 345 | 0.662 | 0.684 | - | - | 0.684 | real (instructions move the same way) |
| awfy/nbody | 92 | 0.783 | 0.769 | - | - | 0.769 | real (instructions move the same way) |
| lib/type_call | 88 | 0.805 | 0.826 | - | - | 0.826 | real (instructions move the same way) |
| jit-trace/field_method | 568 | 0.839 | 0.739 | - | - | 0.739 | real (instructions move the same way) |
| lib/sel_n | 329 | 0.857 | 0.791 | - | - | 0.791 | real (instructions move the same way) |
| lib/sel_loop | 210 | 0.883 | 0.829 | - | - | 0.829 | real (instructions move the same way) |
| lib/len_op | 49 | 0.884 | 0.961 | - | - | 0.961 | real (instructions move the same way) |
| jit-trace/oop_override | 107 | 0.889 | 0.871 | - | - | 0.871 | real (instructions move the same way) |
| lib/sel_count | 217 | 0.907 | 0.792 | - | - | 0.792 | real (instructions move the same way) |
| corpus2/dev/diff_levenshtein | 425 | 0.911 | 0.931 | - | - | 0.931 | real (instructions move the same way) |
| lib/string_key_const | 177 | 0.923 | 0.929 | - | - | 0.929 | real (instructions move the same way) |
| jit-trace/inline_leaf | 454 | 0.925 | 0.729 | - | - | 0.729 | real (instructions move the same way) |
| lib/byte | 50 | 0.929 | 0.932 | - | - | 0.932 | real (instructions move the same way) |
| programs/fannkuch | 1054 | 0.931 | 0.935 | - | - | 0.935 | real (instructions move the same way) |
| jit-trace/int_sum | 888 | 0.935 | 0.754 | - | - | 0.754 | real (instructions move the same way) |

## plain/b1: 15 biggest improvements by instructions (programs >= 20 Mcyc)

| program | Mcyc | cyc A | ins A | cyc 15-run | ins 15-run | ins vs stored SF | note |
|---|---|---|---|---|---|---|---|
| programs/nbody | 345 | 0.662 | 0.684 | - | - | 0.684 | real (instructions move the same way) |
| jit-trace/field_rw | 948 | 0.997 | 0.726 | - | - | 0.726 |  |
| jit-trace/inline_leaf | 454 | 0.925 | 0.729 | - | - | 0.729 | real (instructions move the same way) |
| jit-trace/field_method | 568 | 0.839 | 0.739 | - | - | 0.739 | real (instructions move the same way) |
| jit-trace/int_sum | 888 | 0.935 | 0.754 | - | - | 0.754 | real (instructions move the same way) |
| awfy/nbody | 92 | 0.783 | 0.769 | - | - | 0.769 | real (instructions move the same way) |
| jit-trace/float_poly | 1783 | 0.994 | 0.770 | - | - | 0.770 |  |
| lib/sel_n | 329 | 0.857 | 0.791 | - | - | 0.791 | real (instructions move the same way) |
| lib/sel_count | 217 | 0.907 | 0.792 | - | - | 0.792 | real (instructions move the same way) |
| lib/type_call | 88 | 0.805 | 0.826 | - | - | 0.826 | real (instructions move the same way) |
| lib/sel_loop | 210 | 0.883 | 0.829 | - | - | 0.829 | real (instructions move the same way) |
| jit-trace/inline_2 | 325 | 1.082 | 0.843 | 1.079 | 0.843 | 0.843 |  |
| programs/spectral_norm | 181 | 0.958 | 0.868 | - | - | 0.868 | real (instructions move the same way) |
| jit-trace/oop_override | 107 | 0.889 | 0.871 | - | - | 0.871 | real (instructions move the same way) |
| programs/sieve | 552 | 0.958 | 0.899 | - | - | 0.899 | real (instructions move the same way) |

## plain/b1: programs with instructions >3% above b1 (same session) or above stored SF

| program | Mcyc | cyc A | ins A | cyc 15-run | ins 15-run | ins vs stored SF | note |
|---|---|---|---|---|---|---|---|
| corpus2/heldout/roguelike_turns | 1199 | 1.467 | 1.873 | 1.466 | 1.873 | 1.873 | real (instructions move the same way) |
| corpus2/dev/oop_mixins | 1743 | 1.169 | 1.163 | 1.131 | 1.163 | 1.163 | real (instructions move the same way) |
| corpus/classic_vector | 714 | 1.113 | 1.130 | 1.116 | 1.130 | 1.130 | real (instructions move the same way) |
| real/game_loop | 43 | 1.253 | 1.117 | 1.254 | 1.117 | 1.117 | real (instructions move the same way) |
| corpus/astar_jumper | 1826 | 1.066 | 1.115 | 1.068 | 1.115 | 1.118 | real (instructions move the same way) |
| awfy/cd | 322 | 1.192 | 1.107 | 1.191 | 1.107 | 1.107 | real (instructions move the same way) |
| awfy/deltablue | 117 | 1.116 | 1.078 | 1.120 | 1.078 | 1.078 | real (instructions move the same way) |
| awfy/richards | 61 | 1.066 | 1.065 | 1.066 | 1.066 | 1.065 | real (instructions move the same way) |
| corpus2/heldout/prototype_objects | 1650 | 1.052 | 1.065 | 1.045 | 1.065 | 1.065 | real (instructions move the same way) |
| corpus2/dev/real_dis_x86 | 1624 | 1.036 | 1.047 | 1.038 | 1.047 | 1.046 | real (instructions move the same way) |
| apps/expr_lang | 205 | 1.063 | 1.041 | 1.060 | 1.041 | 1.041 | real (instructions move the same way) |
| apps/lru_cache | 156 | 1.034 | 1.033 | 1.028 | 1.033 | 1.033 |  |

## plain/b1: programs with cycles >3% above b1 in pass A

| program | Mcyc | cyc A | ins A | cyc 15-run | ins 15-run | ins vs stored SF | note |
|---|---|---|---|---|---|---|---|
| corpus2/heldout/roguelike_turns | 1199 | 1.467 | 1.873 | 1.466 | 1.873 | 1.873 | real (instructions move the same way) |
| jit-trace/sink_vec | 15 | 1.323 | 0.884 | 1.329 | 0.884 | 0.884 |  |
| real/game_loop | 43 | 1.253 | 1.117 | 1.254 | 1.117 | 1.117 | real (instructions move the same way) |
| awfy/cd | 322 | 1.192 | 1.107 | 1.191 | 1.107 | 1.107 | real (instructions move the same way) |
| corpus2/dev/oop_mixins | 1743 | 1.169 | 1.163 | 1.131 | 1.163 | 1.163 | real (instructions move the same way) |
| lib/fmt_q | 163 | 1.122 | 1.000 | 1.140 | 1.000 | 1.000 | layout lottery (instructions equal) |
| awfy/deltablue | 117 | 1.116 | 1.078 | 1.120 | 1.078 | 1.078 | real (instructions move the same way) |
| corpus/classic_vector | 714 | 1.113 | 1.130 | 1.116 | 1.130 | 1.130 | real (instructions move the same way) |
| jit-trace/inline_2 | 325 | 1.082 | 0.843 | 1.079 | 0.843 | 0.843 |  |
| corpus/class_middleclass | 518 | 1.076 | 0.934 | 1.073 | 0.934 | 0.934 |  |
| corpus/astar_jumper | 1826 | 1.066 | 1.115 | 1.068 | 1.115 | 1.118 | real (instructions move the same way) |
| awfy/richards | 61 | 1.066 | 1.065 | 1.066 | 1.066 | 1.065 | real (instructions move the same way) |
| apps/expr_lang | 205 | 1.063 | 1.041 | 1.060 | 1.041 | 1.041 | real (instructions move the same way) |
| corpus2/dev/event_promise | 365 | 1.057 | 0.989 | 1.004 | 0.989 | 0.989 |  |
| corpus/parser_combinator | 2406 | 1.054 | 1.001 | 1.056 | 1.001 | 1.000 | layout lottery (instructions equal) |
| corpus2/heldout/prototype_objects | 1650 | 1.052 | 1.065 | 1.045 | 1.065 | 1.065 | real (instructions move the same way) |
| lib/find_cap | 273 | 1.043 | 1.000 | 1.041 | 1.000 | 1.000 | layout lottery (instructions equal) |
| lib/tmove_1k | 99 | 1.039 | 1.000 | 1.000 | 1.000 | 1.000 |  |
| awfy/havlak | 154 | 1.039 | 1.022 | 1.038 | 1.022 | 1.023 | real (instructions move the same way) |
| lib/tunpack | 130 | 1.038 | 0.995 | 1.042 | 0.995 | 0.995 | layout lottery (instructions equal) |
| lib/tonumber_flt | 203 | 1.037 | 1.000 | 1.015 | 1.000 | 1.000 |  |
| corpus2/dev/real_dis_x86 | 1624 | 1.036 | 1.047 | 1.038 | 1.047 | 1.046 | real (instructions move the same way) |
| apps/lru_cache | 156 | 1.034 | 1.033 | 1.028 | 1.033 | 1.033 |  |
| awfy/json | 145 | 1.032 | 1.018 | 1.028 | 1.018 | 1.018 |  |
| apps/events | 169 | 1.031 | 1.000 | 1.028 | 1.000 | 1.000 |  |
| corpus2/dev/coro_generators | 1296 | 1.031 | 1.017 | 1.027 | 1.017 | 1.017 |  |

## plain vs stored SF (cross-session): 10 largest instruction increases / decreases (programs >= 20 Mcyc)

| program | Mcyc | ins ratio vs SF | cyc ratio vs SF |
|---|---|---|---|
| corpus2/heldout/roguelike_turns | 1199 | 1.873 | 1.451 |
| corpus2/dev/oop_mixins | 1743 | 1.163 | 1.081 |
| corpus/classic_vector | 714 | 1.130 | 1.117 |
| corpus/astar_jumper | 1826 | 1.118 | 1.071 |
| real/game_loop | 43 | 1.117 | 1.252 |
| awfy/cd | 322 | 1.107 | 1.178 |
| awfy/deltablue | 117 | 1.078 | 1.109 |
| awfy/richards | 61 | 1.065 | 1.058 |
| corpus2/heldout/prototype_objects | 1650 | 1.065 | 0.833 |
| corpus2/dev/real_dis_x86 | 1624 | 1.046 | 1.023 |
| ... | | | |
| programs/nbody | 345 | 0.684 | 0.659 |
| jit-trace/field_rw | 948 | 0.726 | 0.996 |
| jit-trace/inline_leaf | 454 | 0.729 | 0.941 |
| jit-trace/field_method | 568 | 0.739 | 0.840 |
| jit-trace/int_sum | 888 | 0.754 | 0.935 |
| awfy/nbody | 92 | 0.769 | 0.781 |
| jit-trace/float_poly | 1783 | 0.770 | 0.993 |
| lib/sel_n | 329 | 0.791 | 0.913 |
| lib/sel_count | 217 | 0.792 | 0.916 |
| lib/type_call | 88 | 0.826 | 0.814 |

## PGO/b1: 15 biggest improvements by cycles (programs >= 20 Mcyc)

| program | Mcyc | cyc A | ins A | cyc 15-run | ins 15-run | ins vs stored SF | note |
|---|---|---|---|---|---|---|---|
| programs/nbody | 345 | 0.664 | 0.684 | - | - | 0.684 | real (instructions move the same way) |
| awfy/nbody | 92 | 0.780 | 0.769 | - | - | 0.769 | real (instructions move the same way) |
| lib/type_call | 88 | 0.806 | 0.826 | - | - | 0.826 | real (instructions move the same way) |
| jit-trace/field_method | 561 | 0.831 | 0.738 | - | - | 0.738 | real (instructions move the same way) |
| jit-trace/oop_override | 107 | 0.884 | 0.871 | - | - | 0.871 | real (instructions move the same way) |
| lib/len_op | 48 | 0.887 | 0.961 | - | - | 0.961 | real (instructions move the same way) |
| lib/sel_loop | 211 | 0.902 | 0.829 | - | - | 0.829 | real (instructions move the same way) |
| lib/sel_n | 325 | 0.906 | 0.791 | - | - | 0.791 | real (instructions move the same way) |
| programs/heapsort | 1058 | 0.909 | 0.938 | - | - | 0.938 | real (instructions move the same way) |
| corpus2/dev/diff_levenshtein | 400 | 0.911 | 0.930 | - | - | 0.930 | real (instructions move the same way) |
| jit-trace/inline_leaf | 453 | 0.915 | 0.728 | - | - | 0.728 | real (instructions move the same way) |
| lib/sel_count | 217 | 0.926 | 0.792 | - | - | 0.792 | real (instructions move the same way) |
| lib/byte | 50 | 0.926 | 0.932 | - | - | 0.932 | real (instructions move the same way) |
| lib/string_key_const | 176 | 0.927 | 0.929 | - | - | 0.929 | real (instructions move the same way) |
| lib/len_tbl | 135 | 0.938 | 0.963 | - | - | 0.963 | real (instructions move the same way) |

## PGO/b1: 15 biggest improvements by instructions (programs >= 20 Mcyc)

| program | Mcyc | cyc A | ins A | cyc 15-run | ins 15-run | ins vs stored SF | note |
|---|---|---|---|---|---|---|---|
| programs/nbody | 345 | 0.664 | 0.684 | - | - | 0.684 | real (instructions move the same way) |
| jit-trace/field_rw | 947 | 0.994 | 0.726 | - | - | 0.726 |  |
| jit-trace/inline_leaf | 453 | 0.915 | 0.728 | - | - | 0.728 | real (instructions move the same way) |
| jit-trace/field_method | 561 | 0.831 | 0.738 | - | - | 0.738 | real (instructions move the same way) |
| jit-trace/int_sum | 888 | 0.964 | 0.754 | - | - | 0.754 | real (instructions move the same way) |
| awfy/nbody | 92 | 0.780 | 0.769 | - | - | 0.769 | real (instructions move the same way) |
| jit-trace/float_poly | 1783 | 0.994 | 0.769 | - | - | 0.769 |  |
| lib/sel_n | 325 | 0.906 | 0.791 | - | - | 0.791 | real (instructions move the same way) |
| lib/sel_count | 217 | 0.926 | 0.792 | - | - | 0.792 | real (instructions move the same way) |
| lib/type_call | 88 | 0.806 | 0.826 | - | - | 0.826 | real (instructions move the same way) |
| lib/sel_loop | 211 | 0.902 | 0.829 | - | - | 0.829 | real (instructions move the same way) |
| jit-trace/inline_2 | 324 | 1.078 | 0.843 | 1.086 | 0.843 | 0.843 |  |
| programs/spectral_norm | 182 | 0.965 | 0.868 | - | - | 0.868 | real (instructions move the same way) |
| jit-trace/oop_override | 107 | 0.884 | 0.871 | - | - | 0.871 | real (instructions move the same way) |
| programs/sieve | 556 | 0.981 | 0.899 | - | - | 0.899 |  |

## PGO/b1: programs with instructions >3% above b1 (same session) or above stored SF

| program | Mcyc | cyc A | ins A | cyc 15-run | ins 15-run | ins vs stored SF | note |
|---|---|---|---|---|---|---|---|
| corpus2/heldout/roguelike_turns | 1165 | 1.472 | 1.897 | 1.472 | 1.897 | 1.898 | real (instructions move the same way) |
| corpus2/dev/oop_mixins | 1622 | 1.141 | 1.171 | 1.127 | 1.171 | 1.170 | real (instructions move the same way) |
| corpus/classic_vector | 624 | 1.111 | 1.129 | 1.117 | 1.129 | 1.129 | real (instructions move the same way) |
| corpus/astar_jumper | 1748 | 1.044 | 1.127 | 1.072 | 1.122 | 1.120 | real (instructions move the same way) |
| real/game_loop | 42 | 1.250 | 1.117 | 1.257 | 1.117 | 1.117 | real (instructions move the same way) |
| awfy/cd | 293 | 1.205 | 1.113 | 1.198 | 1.113 | 1.113 | real (instructions move the same way) |
| awfy/deltablue | 110 | 1.125 | 1.083 | 1.126 | 1.083 | 1.083 | real (instructions move the same way) |
| awfy/richards | 59 | 1.067 | 1.071 | 1.062 | 1.071 | 1.071 | real (instructions move the same way) |
| corpus2/heldout/prototype_objects | 1527 | 1.030 | 1.070 | 1.080 | 1.070 | 1.070 | real (instructions move the same way) |
| corpus2/dev/real_dis_x86 | 1382 | 1.014 | 1.049 | 1.022 | 1.049 | 1.049 |  |
| apps/expr_lang | 181 | 1.058 | 1.046 | 1.062 | 1.046 | 1.046 | real (instructions move the same way) |
| apps/lru_cache | 146 | 1.033 | 1.035 | 1.023 | 1.035 | 1.036 |  |
| lib/tsort_cmp | 256 | 0.985 | 1.031 | 0.975 | 1.031 | 1.031 |  |

## PGO/b1: programs with cycles >3% above b1 in pass A

| program | Mcyc | cyc A | ins A | cyc 15-run | ins 15-run | ins vs stored SF | note |
|---|---|---|---|---|---|---|---|
| corpus2/heldout/roguelike_turns | 1165 | 1.472 | 1.897 | 1.472 | 1.897 | 1.898 | real (instructions move the same way) |
| jit-trace/sink_vec | 15 | 1.342 | 0.883 | 1.341 | 0.883 | 0.883 |  |
| real/game_loop | 42 | 1.250 | 1.117 | 1.257 | 1.117 | 1.117 | real (instructions move the same way) |
| awfy/cd | 293 | 1.205 | 1.113 | 1.198 | 1.113 | 1.113 | real (instructions move the same way) |
| corpus2/dev/oop_mixins | 1622 | 1.141 | 1.171 | 1.127 | 1.171 | 1.170 | real (instructions move the same way) |
| awfy/deltablue | 110 | 1.125 | 1.083 | 1.126 | 1.083 | 1.083 | real (instructions move the same way) |
| corpus/classic_vector | 624 | 1.111 | 1.129 | 1.117 | 1.129 | 1.129 | real (instructions move the same way) |
| lib/fmt_g | 67 | 1.080 | 1.000 | 1.075 | 1.000 | 1.000 | layout lottery (instructions equal) |
| jit-trace/inline_2 | 324 | 1.078 | 0.843 | 1.086 | 0.843 | 0.843 |  |
| awfy/richards | 59 | 1.067 | 1.071 | 1.062 | 1.071 | 1.071 | real (instructions move the same way) |
| corpus/class_middleclass | 507 | 1.059 | 0.933 | 1.055 | 0.933 | 0.933 |  |
| apps/expr_lang | 181 | 1.058 | 1.046 | 1.062 | 1.046 | 1.046 | real (instructions move the same way) |
| apps/raytrace | 117 | 1.052 | 1.022 | 1.046 | 1.022 | 1.022 | real (instructions move the same way) |
| lib/tonumber_bad | 115 | 1.049 | 1.004 | 1.054 | 1.004 | 1.000 | layout lottery (instructions equal) |
| corpus/astar_jumper | 1748 | 1.044 | 1.127 | 1.072 | 1.122 | 1.120 | real (instructions move the same way) |
| corpus2/dev/csv_report | 1070 | 1.039 | 1.001 | 1.005 | 1.001 | 1.001 |  |
| lib/fmt_s | 7 | 1.037 | 0.988 | 1.016 | 0.988 | 0.988 |  |
| awfy/json | 124 | 1.035 | 1.025 | 1.049 | 1.025 | 1.025 | real (instructions move the same way) |
| apps/lru_cache | 146 | 1.033 | 1.035 | 1.023 | 1.035 | 1.036 |  |
| corpus/mustache_lustache | 1451 | 1.032 | 1.003 | 1.023 | 1.003 | 1.003 |  |

## PGO vs stored SF (cross-session): 10 largest instruction increases / decreases (programs >= 20 Mcyc)

| program | Mcyc | ins ratio vs SF | cyc ratio vs SF |
|---|---|---|---|
| corpus2/heldout/roguelike_turns | 1165 | 1.898 | 1.476 |
| corpus2/dev/oop_mixins | 1622 | 1.170 | 1.044 |
| corpus/classic_vector | 624 | 1.129 | 1.080 |
| corpus/astar_jumper | 1748 | 1.120 | 1.030 |
| real/game_loop | 42 | 1.117 | 1.258 |
| awfy/cd | 293 | 1.113 | 1.184 |
| awfy/deltablue | 110 | 1.083 | 1.116 |
| awfy/richards | 59 | 1.071 | 1.063 |
| corpus2/heldout/prototype_objects | 1527 | 1.070 | 0.814 |
| corpus2/dev/real_dis_x86 | 1382 | 1.049 | 0.999 |
| ... | | | |
| programs/nbody | 345 | 0.684 | 0.662 |
| jit-trace/field_rw | 947 | 0.726 | 0.994 |
| jit-trace/inline_leaf | 453 | 0.728 | 0.887 |
| jit-trace/field_method | 561 | 0.738 | 0.831 |
| jit-trace/int_sum | 888 | 0.754 | 0.950 |
| awfy/nbody | 92 | 0.769 | 0.780 |
| jit-trace/float_poly | 1783 | 0.769 | 0.994 |
| lib/sel_n | 325 | 0.791 | 0.888 |
| lib/sel_count | 217 | 0.792 | 0.945 |
| lib/type_call | 88 | 0.826 | 0.806 |

# nbody focus (pass A 5-run min, CPUs 3-7; 15-run re-run where available)

| program | build | Mcyc | Mins | cyc/LJ | ins/LJ | cyc/PUC | cyc vs b1 | ins vs b1 | SF Mcyc | SF cyc/LJ(passB) |
|---|---|---|---|---|---|---|---|---|---|---|
| programs/nbody | plain | 344.8 | 2029 | 0.998 | 1.194 | 0.081 | 0.662 | 0.684 | 523.4 | 1.501 |
| programs/nbody | PGO | 344.9 | 2027 | 0.998 | 1.193 | 0.081 | 0.664 | 0.684 | 521.2 | 1.493 |
| programs/nbody | b1 plain | 520.7 | 2966 | 1.507 | 1.746 | 0.123 | - | - | 523.4 | 1.501 |
| programs/nbody | b1 PGO | 519.6 | 2965 | 1.504 | 1.745 | 0.122 | - | - | 521.2 | 1.493 |
| awfy/nbody | plain | 92.3 | 557 | 0.976 | 1.193 | 0.061 | 0.783 | 0.769 | 118.3 | 1.240 |
| awfy/nbody | PGO | 91.6 | 556 | 0.969 | 1.191 | 0.061 | 0.780 | 0.769 | 117.5 | 1.235 |
| awfy/nbody | b1 plain | 118.0 | 724 | 1.247 | 1.552 | 0.078 | - | - | 118.3 | 1.240 |
| awfy/nbody | b1 PGO | 117.5 | 723 | 1.242 | 1.549 | 0.078 | - | - | 117.5 | 1.235 |
