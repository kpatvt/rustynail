# vs baseline (same session): pass A min of 5, 15-run serial CPU-4 re-run where available

Baseline = the session-release source tree (squashed commit 8a18eb3, equivalent of 8f422153), rebuilt in this session, plain and PGO, 5.5.

- plain/baseline geomean over 223 programs: cycles 0.958, instructions 0.959; >3% faster: 31, within 3%: 178, >3% slower: 14
- PGO/baseline geomean over 223 programs: cycles 0.959, instructions 0.966; >3% faster: 30, within 3%: 184, >3% slower: 9

## plain: top-20 improvements (programs >= 30 Mcyc), plain/baseline

| program | Mcyc | cyc A | ins A | cyc 15-run | ins 15-run | note |
|---|---|---|---|---|---|---|
| lib/m_type | 32 | 0.190 | 0.178 | - | - | real (instructions move the same way) |
| lib/m_random | 79 | 0.338 | 0.371 | - | - | real (instructions move the same way) |
| lib/m_random_f | 53 | 0.375 | 0.457 | - | - | real (instructions move the same way) |
| lib/it_pairs | 90 | 0.396 | 0.410 | - | - | real (instructions move the same way) |
| lib/it_next | 45 | 0.403 | 0.409 | - | - | real (instructions move the same way) |
| lib/it_next_empty | 52 | 0.405 | 0.431 | - | - | real (instructions move the same way) |
| jit-trace/tail_loop | 251 | 0.456 | 0.705 | - | - | real (instructions move the same way) |
| lib/it_pairs_hash | 106 | 0.484 | 0.556 | - | - | real (instructions move the same way) |
| jit-trace/tfor_loops | 223 | 0.505 | 0.610 | - | - | real (instructions move the same way) |
| jit-trace/rec_loop | 93 | 0.668 | 0.805 | - | - | real (instructions move the same way) |
| lib/char | 154 | 0.870 | 1.000 | - | - | layout lottery (instructions equal) |
| awfy/permute | 44 | 0.878 | 0.842 | - | - | real (instructions move the same way) |
| corpus2/dev/oop_mixins | 1495 | 0.892 | 0.930 | - | - | real (instructions move the same way) |
| lib/tonumber_flt | 196 | 0.899 | 1.000 | - | - | layout lottery (instructions equal) |
| apps/raytrace | 137 | 0.921 | 0.949 | - | - | real (instructions move the same way) |
| lib/tinsert_idx | 173 | 0.935 | 0.999 | 1.023 | 0.999 |  |
| lib/type_call | 109 | 0.941 | 0.934 | - | - | real (instructions move the same way) |
| corpus/classic_vector | 647 | 0.944 | 0.939 | - | - | real (instructions move the same way) |
| corpus2/heldout/log_analyzer | 2180 | 0.950 | 0.998 | - | - | layout lottery (instructions equal) |
| lib/load_small | 234 | 0.953 | 0.985 | 0.965 | 0.985 | real (instructions move the same way) |

## plain: programs >3% worse than baseline in pass A (all sizes)

| program | Mcyc | cyc A | ins A | cyc 15-run | ins 15-run | note |
|---|---|---|---|---|---|---|
| apps/expr_lang | 197 | 1.293 | 1.134 | 1.280 | 1.134 | real (instructions move the same way) |
| lib/fmt_q | 167 | 1.145 | 1.000 | 1.137 | 1.000 | layout lottery (instructions equal) |
| lib/sel_n | 402 | 1.116 | 0.999 | 1.049 | 0.999 | layout lottery (instructions equal) |
| lib/str_lt_short | 114 | 1.079 | 1.000 | 1.078 | 1.000 | layout lottery (instructions equal) |
| jit-trace/alloc_ctor | 226 | 1.063 | 1.000 | 1.066 | 1.000 | layout lottery (instructions equal) |
| corpus2/heldout/stream_closures | 943 | 1.057 | 0.963 | 1.067 | 0.963 |  |
| programs/coroutines | 1850 | 1.053 | 1.000 | 1.073 | 1.000 | layout lottery (instructions equal) |
| lib/str_method_len | 47 | 1.050 | 0.999 | 1.005 | 0.999 |  |
| programs/load_big | 770 | 1.049 | 1.000 | 1.011 | 1.000 |  |
| programs/table_hash | 517 | 1.042 | 1.005 | 0.987 | 1.005 |  |
| programs/string_ops | 1153 | 1.039 | 1.000 | 1.001 | 1.000 |  |
| awfy/cd | 266 | 1.038 | 1.040 | 1.033 | 1.040 | real (instructions move the same way) |
| lib/tonumber_num | 224 | 1.036 | 0.998 | 0.991 | 0.998 |  |
| lib/co_yield | 234 | 1.032 | 1.000 | 1.027 | 1.000 |  |

## PGO: top-20 improvements (programs >= 30 Mcyc), PGO/baseline

| program | Mcyc | cyc A | ins A | cyc 15-run | ins 15-run | note |
|---|---|---|---|---|---|---|
| lib/m_type | 32 | 0.174 | 0.181 | - | - | real (instructions move the same way) |
| lib/it_next_empty | 52 | 0.428 | 0.488 | - | - | real (instructions move the same way) |
| lib/it_next | 45 | 0.431 | 0.468 | - | - | real (instructions move the same way) |
| lib/it_pairs | 91 | 0.452 | 0.469 | - | - | real (instructions move the same way) |
| jit-trace/tail_loop | 252 | 0.455 | 0.705 | - | - | real (instructions move the same way) |
| lib/it_pairs_hash | 106 | 0.483 | 0.570 | - | - | real (instructions move the same way) |
| jit-trace/tfor_loops | 222 | 0.503 | 0.623 | - | - | real (instructions move the same way) |
| lib/char | 139 | 0.523 | 1.025 | - | - |  |
| lib/m_random_f | 52 | 0.624 | 0.714 | - | - | real (instructions move the same way) |
| jit-trace/rec_loop | 92 | 0.671 | 0.805 | - | - | real (instructions move the same way) |
| lib/m_random | 79 | 0.758 | 0.633 | - | - | real (instructions move the same way) |
| lib/tonumber_bad | 112 | 0.772 | 0.996 | - | - | layout lottery (instructions equal) |
| awfy/permute | 44 | 0.878 | 0.842 | - | - | real (instructions move the same way) |
| apps/raytrace | 114 | 0.882 | 0.950 | - | - | real (instructions move the same way) |
| jit-trace/alloc_ctor | 163 | 0.886 | 0.989 | 0.889 | 0.989 | real (instructions move the same way) |
| corpus/classic_vector | 559 | 0.942 | 0.947 | - | - | real (instructions move the same way) |
| corpus2/dev/regex_nfa | 559 | 0.945 | 0.983 | - | - | real (instructions move the same way) |
| programs/coroutines | 1353 | 0.946 | 0.990 | 0.947 | 0.990 | real (instructions move the same way) |
| programs/heapsort | 1092 | 0.954 | 0.969 | - | - | real (instructions move the same way) |
| lib/type_call | 109 | 0.955 | 0.934 | - | - | real (instructions move the same way) |

## PGO: programs >3% worse than baseline in pass A (all sizes)

| program | Mcyc | cyc A | ins A | cyc 15-run | ins 15-run | note |
|---|---|---|---|---|---|---|
| apps/expr_lang | 171 | 1.250 | 1.128 | 1.256 | 1.128 | real (instructions move the same way) |
| lib/str_lt_short | 106 | 1.085 | 1.000 | 1.089 | 1.000 | layout lottery (instructions equal) |
| corpus2/heldout/stream_closures | 854 | 1.064 | 0.951 | 1.060 | 0.951 |  |
| lib/sel_n | 381 | 1.062 | 0.999 | 1.065 | 0.999 | layout lottery (instructions equal) |
| lib/m_tointeger | 235 | 1.056 | 0.999 | 1.004 | 0.999 |  |
| lib/tinsert_idx | 157 | 1.045 | 0.999 | 1.006 | 0.999 |  |
| corpus2/heldout/prototype_objects | 1531 | 1.040 | 0.997 | 1.012 | 0.997 |  |
| lib/load_small | 168 | 1.037 | 0.982 | 1.032 | 0.982 |  |
| awfy/cd | 241 | 1.030 | 1.047 | 1.026 | 1.047 |  |

## Per set, geomean plain/baseline and PGO/baseline (cycles, instructions), pass A

| set | n | plain cyc | plain ins | PGO cyc | PGO ins |
|---|---|---|---|---|---|
| apps | 5 | 1.036 | 1.013 | 1.010 | 1.018 |
| awfy | 14 | 0.991 | 0.977 | 0.991 | 0.978 |
| corpus | 18 | 0.987 | 0.986 | 0.991 | 0.991 |
| corpus2/dev | 19 | 0.992 | 0.989 | 0.993 | 0.991 |
| corpus2/heldout | 13 | 0.998 | 0.990 | 1.013 | 0.989 |
| jit-trace | 29 | 0.937 | 0.961 | 0.929 | 0.961 |
| lib | 95 | 0.925 | 0.931 | 0.932 | 0.945 |
| programs | 25 | 1.003 | 0.983 | 0.987 | 0.983 |
| real | 5 | 0.998 | 0.991 | 0.995 | 0.993 |
