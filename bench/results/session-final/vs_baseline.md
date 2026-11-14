# vs baselines (same session, all rebuilt with scripts/release-build.sh): pass A min of 5, 15-run serial CPU-4 re-run where available

Subject = HEAD 108b935. Baseline 9a9b8ed = the session-bm5 subject; baseline 8a18eb3 = the squashed first commit, the baseline of the previous release table. 5.5 builds, plain and PGO.


# Baseline 9a9b8ed

- plain/9a9b8ed geomean over 223 programs: cycles 0.998, instructions 1.000; >3% faster: 11, within 3%: 205, >3% slower: 7
- PGO/9a9b8ed geomean over 223 programs: cycles 0.999, instructions 1.000; >3% faster: 9, within 3%: 209, >3% slower: 5

## plain/9a9b8ed: top-20 improvements (programs >= 30 Mcyc)

| program | Mcyc | cyc A | ins A | cyc 15-run | ins 15-run | note |
|---|---|---|---|---|---|---|
| programs/fib | 1240 | 0.889 | 0.890 | - | - | real (instructions move the same way) |
| lib/fmt_q | 146 | 0.898 | 1.000 | - | - | layout lottery (instructions equal) |
| jit-trace/alloc_ctor | 212 | 0.942 | 1.000 | - | - | layout lottery (instructions equal) |
| corpus2/dev/event_promise | 392 | 0.944 | 1.001 | - | - | layout lottery (instructions equal) |
| lib/sel_n | 361 | 0.949 | 1.000 | 1.105 | 1.000 | layout lottery (instructions equal) |
| lib/tonumber_bad | 147 | 0.951 | 1.000 | - | - | layout lottery (instructions equal) |
| programs/heapsort | 1225 | 0.960 | 1.000 | - | - | layout lottery (instructions equal) |
| corpus2/dev/oop_mixins | 1613 | 0.960 | 1.000 | - | - | layout lottery (instructions equal) |
| programs/load_big | 757 | 0.961 | 1.000 | 0.992 | 1.000 |  |
| lib/find_pat | 265 | 0.963 | 1.001 | 0.967 | 1.001 | layout lottery (instructions equal) |
| corpus/parser_combinator | 2302 | 0.967 | 1.000 | - | - | layout lottery (instructions equal) |
| lib/m_min | 227 | 0.972 | 1.000 | - | - |  |
| corpus/classic_vector | 639 | 0.976 | 1.000 | - | - |  |
| corpus2/dev/regex_nfa | 680 | 0.976 | 1.000 | - | - |  |
| jit-trace/tfor_loops | 220 | 0.978 | 1.000 | - | - |  |
| awfy/havlak | 162 | 0.978 | 1.000 | - | - |  |
| corpus2/dev/text_adventure | 2602 | 0.979 | 1.000 | - | - |  |
| lib/str_eq_long | 35 | 0.982 | 1.000 | - | - |  |
| awfy/sieve | 131 | 0.983 | 1.000 | - | - |  |
| lib/sub_1char | 125 | 0.984 | 1.000 | - | - |  |

## plain/9a9b8ed: programs >3% worse in pass A (all sizes)

| program | Mcyc | cyc A | ins A | cyc 15-run | ins 15-run | note |
|---|---|---|---|---|---|---|
| corpus2/heldout/prototype_objects | 1982 | 1.052 | 1.001 | 1.023 | 1.001 |  |
| programs/binary_trees | 2020 | 1.040 | 1.003 | 1.015 | 1.003 |  |
| real/template_engine | 756 | 1.039 | 1.000 | 1.036 | 1.000 | layout lottery (instructions equal) |
| lib/tonumber_base | 275 | 1.039 | 1.000 | 1.036 | 1.000 | layout lottery (instructions equal) |
| awfy/towers | 51 | 1.036 | 1.012 | 1.033 | 1.012 | real (instructions move the same way) |
| lib/sel_count | 237 | 1.035 | 1.000 | 0.978 | 1.000 |  |
| lib/gsub_str | 171 | 1.030 | 1.000 | 1.005 | 1.000 |  |

## PGO/9a9b8ed: top-20 improvements (programs >= 30 Mcyc)

| program | Mcyc | cyc A | ins A | cyc 15-run | ins 15-run | note |
|---|---|---|---|---|---|---|
| programs/fib | 1236 | 0.890 | 0.890 | - | - | real (instructions move the same way) |
| programs/heapsort | 1273 | 0.893 | 1.000 | - | - | layout lottery (instructions equal) |
| awfy/storage | 77 | 0.960 | 1.000 | 1.003 | 1.000 |  |
| programs/table_hash | 606 | 0.961 | 1.000 | - | - | layout lottery (instructions equal) |
| corpus2/heldout/prototype_objects | 1876 | 0.964 | 1.001 | 0.987 | 1.001 |  |
| awfy/havlak | 143 | 0.965 | 1.001 | - | - | layout lottery (instructions equal) |
| programs/string_ops | 1181 | 0.966 | 1.000 | 0.999 | 1.000 |  |
| corpus/json_rxi | 1541 | 0.969 | 1.000 | - | - | layout lottery (instructions equal) |
| corpus2/dev/csv_report | 1062 | 0.969 | 1.000 | - | - | layout lottery (instructions equal) |
| lib/sel_n | 366 | 0.971 | 1.000 | 0.973 | 1.000 |  |
| corpus2/dev/template_gsub | 1412 | 0.973 | 1.001 | - | - |  |
| lib/load_small | 158 | 0.976 | 1.000 | - | - |  |
| lib/sel_loop | 233 | 0.977 | 1.000 | - | - |  |
| lib/sel_count | 229 | 0.977 | 1.000 | 0.982 | 1.000 |  |
| corpus2/dev/varargs_util | 1241 | 0.978 | 0.992 | - | - |  |
| lib/tmove_1k | 68 | 0.979 | 1.000 | - | - |  |
| corpus2/dev/diff_levenshtein | 443 | 0.979 | 1.000 | - | - |  |
| apps/events | 139 | 0.979 | 0.998 | - | - |  |
| lib/find_cap | 226 | 0.981 | 1.001 | - | - |  |
| corpus2/dev/regex_nfa | 560 | 0.981 | 1.000 | - | - |  |

## PGO/9a9b8ed: programs >3% worse in pass A (all sizes)

| program | Mcyc | cyc A | ins A | cyc 15-run | ins 15-run | note |
|---|---|---|---|---|---|---|
| jit-trace/inline_leaf | 511 | 1.061 | 1.000 | 1.001 | 1.000 |  |
| lib/co_yield | 192 | 1.053 | 1.000 | 1.051 | 1.000 | layout lottery (instructions equal) |
| awfy/towers | 49 | 1.042 | 1.009 | 1.041 | 1.009 | layout lottery (instructions equal) |
| corpus/astar_jumper | 1697 | 1.039 | 1.003 | 1.005 | 0.998 |  |
| programs/load_big | 587 | 1.033 | 1.000 | 1.019 | 1.000 |  |

## Per set, geomean plain/9a9b8ed and PGO/9a9b8ed (cycles, instructions), pass A

| set | n | plain cyc | plain ins | PGO cyc | PGO ins |
|---|---|---|---|---|---|
| apps | 5 | 1.008 | 1.000 | 1.000 | 1.000 |
| awfy | 14 | 1.000 | 1.002 | 0.996 | 1.002 |
| corpus | 18 | 0.997 | 1.001 | 1.002 | 1.000 |
| corpus2/dev | 17 | 0.990 | 1.001 | 0.996 | 1.001 |
| corpus2/heldout | 11 | 1.008 | 1.000 | 0.998 | 1.001 |
| jit-trace | 29 | 0.998 | 1.000 | 1.003 | 1.000 |
| lib | 95 | 0.998 | 1.000 | 1.000 | 1.000 |
| programs | 25 | 0.997 | 0.995 | 0.990 | 0.996 |
| real | 5 | 1.002 | 1.000 | 0.999 | 1.000 |
| real_dis | 4 | 1.005 | 1.001 | 0.996 | 1.001 |

# Baseline 8a18eb3

- plain/8a18eb3 geomean over 223 programs: cycles 0.956, instructions 0.959; >3% faster: 27, within 3%: 186, >3% slower: 10
- PGO/8a18eb3 geomean over 223 programs: cycles 0.959, instructions 0.966; >3% faster: 28, within 3%: 181, >3% slower: 14

## plain/8a18eb3: top-20 improvements (programs >= 30 Mcyc)

| program | Mcyc | cyc A | ins A | cyc 15-run | ins 15-run | note |
|---|---|---|---|---|---|---|
| lib/m_type | 32 | 0.187 | 0.178 | - | - | real (instructions move the same way) |
| lib/m_random | 78 | 0.338 | 0.371 | - | - | real (instructions move the same way) |
| lib/m_random_f | 52 | 0.374 | 0.457 | - | - | real (instructions move the same way) |
| lib/it_pairs | 88 | 0.395 | 0.410 | - | - | real (instructions move the same way) |
| lib/it_next_empty | 51 | 0.403 | 0.431 | - | - | real (instructions move the same way) |
| lib/it_next | 46 | 0.421 | 0.409 | - | - | real (instructions move the same way) |
| jit-trace/tail_loop | 253 | 0.456 | 0.705 | - | - | real (instructions move the same way) |
| lib/it_pairs_hash | 107 | 0.491 | 0.556 | - | - | real (instructions move the same way) |
| jit-trace/tfor_loops | 220 | 0.498 | 0.610 | - | - | real (instructions move the same way) |
| jit-trace/rec_loop | 93 | 0.668 | 0.805 | - | - | real (instructions move the same way) |
| programs/fib | 1240 | 0.856 | 0.778 | - | - | real (instructions move the same way) |
| lib/char | 152 | 0.856 | 1.000 | - | - | layout lottery (instructions equal) |
| awfy/permute | 45 | 0.881 | 0.842 | - | - | real (instructions move the same way) |
| lib/tonumber_flt | 198 | 0.910 | 1.000 | - | - | layout lottery (instructions equal) |
| corpus2/dev/oop_mixins | 1613 | 0.925 | 0.930 | - | - | real (instructions move the same way) |
| lib/tonumber_bad | 147 | 0.926 | 1.000 | - | - | layout lottery (instructions equal) |
| corpus/classic_vector | 639 | 0.931 | 0.939 | - | - | real (instructions move the same way) |
| lib/type_call | 108 | 0.943 | 0.934 | - | - | real (instructions move the same way) |
| apps/raytrace | 138 | 0.949 | 0.949 | - | - | real (instructions move the same way) |
| corpus2/heldout/prototype_objects | 1982 | 0.952 | 0.997 | 1.010 | 0.997 |  |

## plain/8a18eb3: programs >3% worse in pass A (all sizes)

| program | Mcyc | cyc A | ins A | cyc 15-run | ins 15-run | note |
|---|---|---|---|---|---|---|
| apps/expr_lang | 195 | 1.294 | 1.134 | 1.291 | 1.134 | real (instructions move the same way) |
| lib/str_lt_short | 113 | 1.074 | 1.000 | 1.082 | 1.000 | layout lottery (instructions equal) |
| corpus2/heldout/stream_closures | 958 | 1.073 | 0.966 | 1.075 | 0.966 |  |
| lib/m_tointeger | 236 | 1.059 | 0.999 | 1.061 | 0.999 | layout lottery (instructions equal) |
| awfy/cd | 273 | 1.049 | 1.050 | 1.054 | 1.050 | real (instructions move the same way) |
| programs/string_ops | 1296 | 1.044 | 1.000 | 1.005 | 1.000 |  |
| awfy/storage | 104 | 1.034 | 0.988 | 1.006 | 0.988 |  |
| programs/closures | 505 | 1.034 | 0.988 | 1.035 | 0.988 |  |
| lib/tonumber_base | 275 | 1.033 | 1.000 | 1.036 | 1.000 | layout lottery (instructions equal) |
| lib/sel_count | 237 | 1.032 | 0.999 | 1.006 | 0.999 |  |

## PGO/8a18eb3: top-20 improvements (programs >= 30 Mcyc)

| program | Mcyc | cyc A | ins A | cyc 15-run | ins 15-run | note |
|---|---|---|---|---|---|---|
| lib/m_type | 31 | 0.171 | 0.181 | - | - | real (instructions move the same way) |
| lib/it_next | 44 | 0.427 | 0.468 | - | - | real (instructions move the same way) |
| lib/it_pairs | 88 | 0.429 | 0.469 | - | - | real (instructions move the same way) |
| lib/it_next_empty | 52 | 0.433 | 0.488 | - | - | real (instructions move the same way) |
| jit-trace/tail_loop | 252 | 0.456 | 0.705 | - | - | real (instructions move the same way) |
| lib/it_pairs_hash | 105 | 0.486 | 0.570 | - | - | real (instructions move the same way) |
| jit-trace/tfor_loops | 221 | 0.502 | 0.623 | - | - | real (instructions move the same way) |
| lib/char | 139 | 0.528 | 1.025 | - | - |  |
| lib/m_random_f | 52 | 0.622 | 0.714 | - | - | real (instructions move the same way) |
| jit-trace/rec_loop | 92 | 0.670 | 0.805 | - | - | real (instructions move the same way) |
| lib/m_random | 79 | 0.746 | 0.632 | - | - | real (instructions move the same way) |
| lib/tonumber_bad | 112 | 0.779 | 0.996 | - | - | layout lottery (instructions equal) |
| programs/fib | 1236 | 0.849 | 0.778 | - | - | real (instructions move the same way) |
| awfy/permute | 44 | 0.882 | 0.842 | - | - | real (instructions move the same way) |
| jit-trace/alloc_ctor | 164 | 0.889 | 0.989 | - | - | real (instructions move the same way) |
| apps/raytrace | 111 | 0.899 | 0.951 | - | - | real (instructions move the same way) |
| corpus2/dev/regex_nfa | 560 | 0.938 | 0.983 | - | - | real (instructions move the same way) |
| corpus/mustache_lustache | 1430 | 0.947 | 0.969 | - | - | real (instructions move the same way) |
| apps/events | 139 | 0.957 | 0.994 | - | - | layout lottery (instructions equal) |
| lib/type_call | 109 | 0.957 | 0.934 | - | - | real (instructions move the same way) |

## PGO/8a18eb3: programs >3% worse in pass A (all sizes)

| program | Mcyc | cyc A | ins A | cyc 15-run | ins 15-run | note |
|---|---|---|---|---|---|---|
| apps/expr_lang | 169 | 1.256 | 1.128 | 1.261 | 1.128 | real (instructions move the same way) |
| lib/str_lt_short | 105 | 1.089 | 1.000 | 1.088 | 1.000 | layout lottery (instructions equal) |
| jit-trace/inline_leaf | 511 | 1.079 | 0.999 | 1.000 | 0.999 |  |
| lib/co_yield | 192 | 1.063 | 1.000 | 1.051 | 1.000 | layout lottery (instructions equal) |
| corpus2/heldout/stream_closures | 868 | 1.059 | 0.954 | 1.058 | 0.954 |  |
| lib/m_tointeger | 235 | 1.057 | 0.999 | 1.056 | 0.999 | layout lottery (instructions equal) |
| awfy/cd | 248 | 1.048 | 1.057 | 1.041 | 1.057 | real (instructions move the same way) |
| corpus/csv_ini_yaml | 1436 | 1.047 | 1.013 | 1.014 | 1.013 |  |
| lib/gsub_fn | 136 | 1.042 | 1.000 | 1.026 | 1.000 |  |
| awfy/towers | 49 | 1.039 | 1.000 | 1.038 | 1.000 | layout lottery (instructions equal) |
| corpus2/heldout/scheme_interp | 1324 | 1.033 | 0.994 | 1.029 | 0.994 |  |
| corpus2/dev/memo_cache | 1721 | 1.033 | 0.997 | 1.009 | 0.997 |  |
| lib/sel_n | 366 | 1.032 | 0.999 | 1.071 | 0.999 | layout lottery (instructions equal) |
| lib/find_pat | 236 | 1.030 | 1.015 | 1.003 | 1.015 |  |

## Per set, geomean plain/8a18eb3 and PGO/8a18eb3 (cycles, instructions), pass A

| set | n | plain cyc | plain ins | PGO cyc | PGO ins |
|---|---|---|---|---|---|
| apps | 5 | 1.044 | 1.014 | 1.014 | 1.019 |
| awfy | 14 | 0.998 | 0.979 | 0.994 | 0.980 |
| corpus | 18 | 0.986 | 0.986 | 0.993 | 0.991 |
| corpus2/dev | 17 | 0.987 | 0.990 | 0.995 | 0.992 |
| corpus2/heldout | 11 | 1.002 | 0.988 | 1.010 | 0.987 |
| jit-trace | 29 | 0.935 | 0.961 | 0.934 | 0.961 |
| lib | 95 | 0.924 | 0.931 | 0.931 | 0.945 |
| programs | 25 | 0.993 | 0.979 | 0.985 | 0.978 |
| real | 5 | 1.001 | 0.991 | 0.992 | 0.993 |
| real_dis | 4 | 1.002 | 0.998 | 0.981 | 1.002 |
