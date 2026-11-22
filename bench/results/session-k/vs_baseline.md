# Subject 5a064d7 vs b116447 (same session), session-j published, 22670a1 (set B, min over all readings incl. 15-run reruns)

## Subject vs b116447 (same session, set B), b116447 set A, session-j published, 22670a1, 8c39465, 1a5aad0 and 108b935 (geomean, 223 programs)

| build | vs | cycles | instructions | cyc >3% better / within / >3% worse | ins >3% fewer / more |
|---|---|---|---|---|---|
| 5.5 plain | b116447 | 0.997 | 0.996 | 6 / 213 / 4 | 3 / 1 |
| 5.5 plain | b116447 setA | 0.995 | 0.996 | 10 / 210 / 3 | 3 / 1 |
| 5.5 plain | session-j published | 0.993 | 0.995 | 13 / 204 / 6 | 3 / 1 |
| 5.5 plain | 22670a1 | 0.995 | 0.988 | 9 / 201 / 13 | 14 / 1 |
| 5.5 plain | 8c39465 | 0.990 | 0.982 | 17 / 194 / 12 | 24 / 2 |
| 5.5 plain | 1a5aad0 | 0.982 | 0.965 | 42 / 159 / 22 | 41 / 1 |
| 5.5 plain | 108b935 | 0.976 | 0.944 | 51 / 149 / 23 | 93 / 0 |
| 5.5 PGO | b116447 | 0.997 | 0.996 | 10 / 209 / 4 | 3 / 1 |
| 5.5 PGO | b116447 setA | 0.995 | 0.996 | 13 / 206 / 4 | 3 / 1 |
| 5.5 PGO | session-j published | 0.995 | 0.996 | 11 / 207 / 5 | 3 / 1 |
| 5.5 PGO | 22670a1 | 0.994 | 0.990 | 12 / 206 / 5 | 13 / 1 |
| 5.5 PGO | 8c39465 | 0.991 | 0.983 | 16 / 195 / 12 | 23 / 2 |
| 5.5 PGO | 1a5aad0 | 0.983 | 0.964 | 33 / 178 / 12 | 42 / 1 |
| 5.5 PGO | 108b935 | 0.979 | 0.943 | 48 / 150 / 25 | 95 / 1 |
| 5.4 plain | b116447 | 0.999 | 0.996 | 6 / 213 / 4 | 3 / 1 |
| 5.4 plain | b116447 setA | 0.997 | 0.996 | 10 / 210 / 3 | 3 / 1 |
| 5.4 plain | session-j published | 0.995 | 0.996 | 12 / 203 / 8 | 3 / 1 |
| 5.4 plain | 22670a1 | 0.997 | 0.993 | 13 / 197 / 13 | 10 / 1 |
| 5.4 plain | 8c39465 | 0.988 | 0.984 | 20 / 189 / 14 | 18 / 4 |
| 5.4 plain | 1a5aad0 | 0.987 | 0.971 | 32 / 168 / 23 | 34 / 3 |
| 5.4 PGO | b116447 | 0.998 | 0.996 | 6 / 212 / 5 | 3 / 1 |
| 5.4 PGO | b116447 setA | 0.996 | 0.996 | 9 / 210 / 4 | 3 / 1 |
| 5.4 PGO | session-j published | 0.996 | 0.996 | 11 / 206 / 6 | 3 / 1 |
| 5.4 PGO | 22670a1 | 0.996 | 0.993 | 12 / 206 / 5 | 9 / 1 |
| 5.4 PGO | 8c39465 | 0.992 | 0.985 | 16 / 197 / 10 | 17 / 4 |
| 5.4 PGO | 1a5aad0 | 0.987 | 0.970 | 26 / 182 / 15 | 35 / 3 |

### Per set, geomean subject / b116447 and subject / 22670a1 (cycles / instructions)

| set | n | plain vs b cyc | ins | PGO vs b cyc | ins | 5.4 plain vs b cyc | ins | 5.4 PGO vs b cyc | ins | plain vs i cyc | ins | PGO vs i cyc | ins | 5.4 plain vs i cyc | ins | 5.4 PGO vs i cyc | ins |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| apps | 5 | 1.008 | 1.001 | 1.012 | 1.003 | 1.009 | 1.001 | 1.009 | 1.000 | 1.002 | 0.975 | 1.001 | 0.982 | 1.000 | 0.977 | 0.997 | 0.980 |
| awfy | 14 | 0.993 | 0.984 | 0.990 | 0.983 | 0.999 | 0.986 | 0.995 | 0.985 | 0.983 | 0.972 | 0.984 | 0.973 | 0.989 | 0.974 | 0.986 | 0.976 |
| corpus | 18 | 1.003 | 1.000 | 1.014 | 1.003 | 1.013 | 1.000 | 1.012 | 1.000 | 0.965 | 0.968 | 0.972 | 0.977 | 0.990 | 0.986 | 0.991 | 0.989 |
| corpus2/dev | 17 | 1.006 | 0.999 | 1.012 | 1.000 | 1.009 | 0.999 | 1.005 | 0.999 | 1.011 | 0.994 | 1.006 | 0.995 | 1.003 | 0.998 | 1.002 | 0.998 |
| corpus2/heldout | 11 | 1.011 | 1.000 | 1.000 | 1.000 | 1.002 | 1.000 | 1.003 | 1.000 | 0.984 | 0.955 | 0.980 | 0.956 | 0.997 | 0.992 | 0.997 | 0.992 |
| jit-trace | 29 | 1.003 | 1.006 | 1.004 | 1.006 | 1.004 | 1.006 | 1.004 | 1.006 | 1.005 | 1.005 | 1.004 | 1.005 | 1.005 | 1.005 | 1.005 | 1.006 |
| lib | 95 | 0.999 | 1.000 | 0.997 | 1.001 | 1.000 | 1.000 | 0.998 | 1.000 | 1.004 | 1.000 | 1.002 | 1.001 | 1.002 | 1.000 | 1.002 | 1.000 |
| programs | 25 | 0.969 | 0.963 | 0.970 | 0.962 | 0.971 | 0.964 | 0.972 | 0.963 | 0.972 | 0.963 | 0.964 | 0.961 | 0.968 | 0.964 | 0.965 | 0.963 |
| real | 5 | 0.992 | 1.001 | 0.981 | 1.002 | 0.998 | 1.002 | 0.996 | 1.002 | 1.003 | 0.997 | 1.002 | 0.999 | 0.998 | 1.000 | 0.997 | 1.000 |
| real_dis | 4 | 0.999 | 1.001 | 0.977 | 1.003 | 1.012 | 1.000 | 1.000 | 1.000 | 0.987 | 0.985 | 0.996 | 0.985 | 1.008 | 1.000 | 1.007 | 1.000 |
| ALL | 223 | 0.997 | 0.996 | 0.997 | 0.996 | 0.999 | 0.996 | 0.998 | 0.996 | 0.995 | 0.988 | 0.994 | 0.990 | 0.997 | 0.993 | 0.996 | 0.993 |

### 5.5 plain vs b116447: 20 biggest improvements by cycles (>= 20 Mcyc)

| program | Mcyc | cyc ratio | ins ratio | cycle spread of readings | note |
|---|---|---|---|---|---|
| programs/fib | 634 | 0.510 | 0.505 | 4.7% (20 runs) | instructions move the same way |
| programs/binary_trees | 1244 | 0.811 | 0.762 | 15.0% (20 runs) | instructions move the same way |
| awfy/permute | 37 | 0.836 | 0.830 | 1.7% (20 runs) | instructions move the same way |
| lib/co_yield | 221 | 0.909 | 1.000 | 36.3% (20 runs) | instructions equal: layout / IPC |
| real/lua_interp | 107 | 0.958 | 1.004 | 1.9% (5 runs) | instructions equal: layout / IPC |
| corpus/csv_ini_yaml | 1559 | 0.967 | 1.000 | 14.6% (20 runs) | instructions equal: layout / IPC |
| apps/events | 165 | 0.973 | 1.000 | 1.8% (5 runs) |  |
| programs/fannkuch | 1037 | 0.973 | 1.000 | 0.5% (2 runs) |  |
| programs/coroutines | 1763 | 0.975 | 1.000 | 7.7% (20 runs) |  |
| awfy/towers | 48 | 0.975 | 0.978 | 6.2% (20 runs) |  |
| lib/tsort_str | 256 | 0.982 | 0.999 | 2.4% (5 runs) |  |
| corpus2/dev/oop_mixins | 1426 | 0.984 | 1.000 | 6.1% (17 runs) |  |
| lib/it_next | 45 | 0.985 | 1.000 | 8.4% (20 runs) |  |
| lib/char | 152 | 0.986 | 1.000 | 4.1% (20 runs) |  |
| lib/tonumber_flt | 193 | 0.986 | 1.000 | 2.0% (5 runs) |  |
| lib/tonumber_base | 265 | 0.986 | 1.000 | 0.1% (2 runs) |  |
| corpus/penlight_mix | 2274 | 0.986 | 1.000 | 4.8% (20 runs) |  |
| real/template_engine | 716 | 0.986 | 1.000 | 6.4% (20 runs) |  |
| corpus2/dev/diff_levenshtein | 431 | 0.987 | 1.000 | 3.9% (5 runs) |  |
| lib/fmt_mixed | 59 | 0.988 | 1.000 | 6.0% (20 runs) |  |

### 5.5 PGO vs b116447: 20 biggest improvements by cycles (>= 20 Mcyc)

| program | Mcyc | cyc ratio | ins ratio | cycle spread of readings | note |
|---|---|---|---|---|---|
| programs/fib | 633 | 0.509 | 0.505 | 54.7% (20 runs) | instructions move the same way |
| programs/binary_trees | 1059 | 0.813 | 0.738 | 5.2% (20 runs) | instructions move the same way |
| awfy/permute | 36 | 0.836 | 0.829 | 1.9% (20 runs) | instructions move the same way |
| lib/tonumber_bad | 114 | 0.895 | 1.004 | 4.1% (17 runs) | instructions equal: layout / IPC |
| real/lua_interp | 103 | 0.945 | 1.004 | 2.8% (5 runs) | instructions equal: layout / IPC |
| lib/load_small | 154 | 0.958 | 1.000 | 5.0% (20 runs) | instructions equal: layout / IPC |
| real/template_engine | 588 | 0.958 | 1.005 | 3.4% (20 runs) | instructions equal: layout / IPC |
| corpus2/heldout/real_dis_mips | 1894 | 0.963 | 1.003 | 1.4% (2 runs) | instructions equal: layout / IPC |
| corpus2/heldout/real_dis_ppc | 1819 | 0.965 | 1.004 | 3.3% (17 runs) | instructions equal: layout / IPC |
| awfy/towers | 46 | 0.968 | 0.976 | 2.0% (20 runs) | instructions move the same way |
| corpus2/heldout/log_analyzer | 1761 | 0.972 | 1.003 | 3.0% (5 runs) |  |
| lib/tonumber_flt | 168 | 0.972 | 1.002 | 0.2% (5 runs) |  |
| real/json_roundtrip | 453 | 0.977 | 1.001 | 0.2% (2 runs) |  |
| corpus2/dev/regex_nfa | 560 | 0.977 | 1.001 | 15.4% (20 runs) |  |
| lib/byte_multi | 40 | 0.980 | 1.000 | 1.2% (5 runs) |  |
| jit-trace/tfor_loops | 219 | 0.982 | 1.000 | 2.9% (5 runs) |  |
| lib/tsort_cmp | 256 | 0.982 | 1.000 | 0.2% (2 runs) |  |
| programs/string_ops | 1039 | 0.982 | 1.005 | 8.6% (20 runs) |  |
| lib/m_ult | 281 | 0.984 | 1.000 | 15.4% (20 runs) |  |
| lib/tostring_tbl | 105 | 0.985 | 0.999 | 0.9% (2 runs) |  |

### 5.4 plain vs b116447: 20 biggest improvements by cycles (>= 20 Mcyc)

| program | Mcyc | cyc ratio | ins ratio | cycle spread of readings | note |
|---|---|---|---|---|---|
| programs/fib | 632 | 0.509 | 0.505 | 4.0% (20 runs) | instructions move the same way |
| programs/binary_trees | 1454 | 0.835 | 0.789 | 3.9% (20 runs) | instructions move the same way |
| awfy/permute | 38 | 0.867 | 0.835 | 5.0% (20 runs) | instructions move the same way |
| lib/m_tointeger | 225 | 0.944 | 1.000 | 6.4% (20 runs) | instructions equal: layout / IPC |
| real/lua_interp | 106 | 0.951 | 1.010 | 0.2% (5 runs) |  |
| jit-trace/tfor_loops | 227 | 0.969 | 1.000 | 1.7% (5 runs) | instructions equal: layout / IPC |
| lib/gsub_str | 169 | 0.973 | 1.000 | 1.5% (2 runs) |  |
| corpus2/heldout/scheme_interp | 1514 | 0.975 | 1.000 | 6.6% (17 runs) |  |
| programs/table_hash | 524 | 0.977 | 1.000 | 31.0% (20 runs) |  |
| lib/tostring_int | 134 | 0.980 | 1.000 | 6.0% (20 runs) |  |
| corpus2/dev/varargs_util | 1519 | 0.981 | 1.000 | 3.1% (5 runs) |  |
| lib/tconcat | 167 | 0.983 | 0.999 | 10.8% (20 runs) |  |
| programs/table_array | 402 | 0.983 | 1.000 | 0.1% (2 runs) |  |
| lib/fmt_d | 100 | 0.986 | 1.000 | 2.0% (20 runs) |  |
| lib/find_pat | 263 | 0.986 | 1.000 | 11.4% (20 runs) |  |
| lib/byte_multi | 41 | 0.987 | 1.000 | 4.4% (5 runs) |  |
| lib/tonumber_bad | 167 | 0.987 | 1.000 | 1.3% (17 runs) |  |
| awfy/towers | 49 | 0.988 | 0.978 | 4.6% (20 runs) |  |
| lib/tostring_tbl | 112 | 0.988 | 1.000 | 0.2% (2 runs) |  |
| lib/tonumber_int | 160 | 0.988 | 1.000 | 135.5% (20 runs) |  |

### 5.4 PGO vs b116447: 20 biggest improvements by cycles (>= 20 Mcyc)

| program | Mcyc | cyc ratio | ins ratio | cycle spread of readings | note |
|---|---|---|---|---|---|
| programs/fib | 631 | 0.508 | 0.505 | 5.9% (20 runs) | instructions move the same way |
| programs/binary_trees | 1210 | 0.816 | 0.769 | 3.8% (20 runs) | instructions move the same way |
| lib/co_wrap | 196 | 0.857 | 0.999 | 5.7% (20 runs) | instructions equal: layout / IPC |
| awfy/permute | 38 | 0.866 | 0.834 | 1.8% (20 runs) | instructions move the same way |
| real/lua_interp | 102 | 0.954 | 1.011 | 1.1% (5 runs) |  |
| lib/tonumber_num | 204 | 0.959 | 1.000 | 11.2% (20 runs) | instructions equal: layout / IPC |
| lib/tostring_tbl | 109 | 0.978 | 1.000 | 0.4% (2 runs) |  |
| jit-trace/tfor_loops | 224 | 0.981 | 1.000 | 1.4% (5 runs) |  |
| lib/byte_multi | 41 | 0.983 | 1.000 | 2.4% (5 runs) |  |
| awfy/storage | 114 | 0.984 | 1.000 | 1.6% (5 runs) |  |
| lib/tpack | 170 | 0.985 | 1.001 | 2.2% (5 runs) |  |
| lib/tonumber_flt | 166 | 0.985 | 0.998 | 1.0% (5 runs) |  |
| apps/events | 152 | 0.986 | 1.001 | 2.1% (5 runs) |  |
| corpus2/dev/template_gsub | 1492 | 0.986 | 1.000 | 1.9% (5 runs) |  |
| programs/table_hash | 524 | 0.986 | 0.999 | 34.5% (20 runs) |  |
| lib/sub | 121 | 0.987 | 1.000 | 5.0% (20 runs) |  |
| awfy/towers | 47 | 0.987 | 0.977 | 7.7% (20 runs) |  |
| programs/json | 1537 | 0.988 | 1.000 | 0.0% (2 runs) |  |
| corpus/lua_lexer | 992 | 0.989 | 1.000 | 2.3% (17 runs) |  |
| lib/it_ipairs | 262 | 0.989 | 1.000 | 6.0% (20 runs) |  |

### 5.5 plain vs b116447: regressions (instructions > +2% or cycles > +3%, >= 20 Mcyc)

| program | Mcyc | cyc ratio | ins ratio | cycle spread of readings | note |
|---|---|---|---|---|---|
| jit-trace/rec_loop | 93 | 1.146 | 1.179 | 8.7% (20 runs) | instructions move the same way |
| lib/load_small | 241 | 1.044 | 1.000 | 3.3% (20 runs) | instructions equal: layout / IPC |
| lib/find_pat | 271 | 1.043 | 1.000 | 3.7% (20 runs) | instructions equal: layout / IPC |
| programs/load_big | 766 | 1.031 | 1.000 | 3.2% (17 runs) | instructions equal: layout / IPC |

### 5.5 PGO vs b116447: regressions (instructions > +2% or cycles > +3%, >= 20 Mcyc)

| program | Mcyc | cyc ratio | ins ratio | cycle spread of readings | note |
|---|---|---|---|---|---|
| jit-trace/rec_loop | 92 | 1.139 | 1.179 | 2.6% (20 runs) | instructions move the same way |
| corpus/sha2_base64 | 381 | 1.038 | 1.003 | 2.0% (17 runs) | instructions equal: layout / IPC |
| corpus/parser_combinator | 1878 | 1.038 | 1.001 | 4.7% (17 runs) | instructions equal: layout / IPC |
| corpus/mustache_lustache | 1448 | 1.031 | 1.001 | 2.5% (17 runs) | instructions equal: layout / IPC |
| corpus/penlight_mix | 2153 | 1.019 | 1.027 | 2.0% (20 runs) |  |

### 5.4 plain vs b116447: regressions (instructions > +2% or cycles > +3%, >= 20 Mcyc)

| program | Mcyc | cyc ratio | ins ratio | cycle spread of readings | note |
|---|---|---|---|---|---|
| jit-trace/rec_loop | 90 | 1.131 | 1.177 | 11.6% (20 runs) | instructions move the same way |
| corpus2/dev/regex_nfa | 1394 | 1.037 | 1.000 | 7.2% (20 runs) | instructions equal: layout / IPC |
| corpus/containers_lru_heap | 1672 | 1.033 | 1.001 | 8.2% (17 runs) | instructions equal: layout / IPC |
| corpus2/heldout/prototype_objects | 1517 | 1.031 | 1.000 | 11.1% (20 runs) | instructions equal: layout / IPC |

### 5.4 PGO vs b116447: regressions (instructions > +2% or cycles > +3%, >= 20 Mcyc)

| program | Mcyc | cyc ratio | ins ratio | cycle spread of readings | note |
|---|---|---|---|---|---|
| jit-trace/rec_loop | 89 | 1.133 | 1.176 | 1.3% (20 runs) | instructions move the same way |
| programs/coroutines | 1419 | 1.058 | 1.000 | 5.1% (20 runs) | instructions equal: layout / IPC |
| programs/metamethods | 748 | 1.036 | 1.004 | 2.5% (20 runs) | instructions equal: layout / IPC |
| jit-trace/alloc_ctor | 208 | 1.035 | 1.009 | 5.1% (20 runs) | instructions equal: layout / IPC |
| lib/co_yield | 188 | 1.033 | 1.000 | 18.7% (20 runs) | instructions equal: layout / IPC |

### 5.5 plain vs 22670a1: 20 biggest improvements by cycles (>= 20 Mcyc)

| program | Mcyc | cyc ratio | ins ratio | cycle spread of readings | note |
|---|---|---|---|---|---|
| programs/fib | 634 | 0.505 | 0.505 | 4.7% (20 runs) | instructions move the same way |
| corpus/classic_vector | 477 | 0.741 | 0.735 | 6.1% (20 runs) | instructions move the same way |
| programs/binary_trees | 1244 | 0.806 | 0.762 | 15.0% (20 runs) | instructions move the same way |
| corpus2/heldout/dijkstra_routes | 1186 | 0.819 | 0.641 | 2.4% (17 runs) | instructions move the same way |
| awfy/permute | 37 | 0.839 | 0.829 | 1.7% (20 runs) | instructions move the same way |
| corpus/lua_lexer | 1085 | 0.876 | 0.861 | 6.2% (17 runs) | instructions move the same way |
| corpus/sha2_base64 | 386 | 0.893 | 0.936 | 2.0% (17 runs) | instructions move the same way |
| lib/tconcat | 144 | 0.950 | 0.999 | 7.6% (20 runs) | instructions equal: layout / IPC |
| awfy/towers | 48 | 0.954 | 0.944 | 6.2% (20 runs) | instructions move the same way |
| corpus/template_etlua | 1567 | 0.970 | 0.999 | 0.1% (2 runs) |  |
| lib/match | 61 | 0.972 | 1.000 | 20.9% (20 runs) |  |
| corpus2/heldout/real_dis_ppc | 2090 | 0.972 | 0.979 | 4.8% (17 runs) |  |
| programs/pcall_error | 1483 | 0.976 | 1.000 | 3.2% (5 runs) |  |
| programs/string_ops | 1153 | 0.978 | 1.000 | 16.5% (20 runs) |  |
| lib/tinsert_idx | 175 | 0.979 | 1.001 | 6.9% (20 runs) |  |
| programs/fannkuch | 1037 | 0.979 | 1.000 | 0.5% (2 runs) |  |
| apps/expr_lang | 137 | 0.980 | 0.967 | 1.2% (2 runs) |  |
| lib/co_yield | 221 | 0.981 | 1.000 | 36.3% (20 runs) |  |
| awfy/json | 142 | 0.981 | 0.982 | 3.7% (17 runs) |  |
| lib/setmt | 185 | 0.981 | 1.000 | 3.7% (20 runs) |  |

### 5.5 PGO vs 22670a1: 20 biggest improvements by cycles (>= 20 Mcyc)

| program | Mcyc | cyc ratio | ins ratio | cycle spread of readings | note |
|---|---|---|---|---|---|
| programs/fib | 633 | 0.491 | 0.505 | 54.7% (20 runs) | instructions move the same way |
| corpus/classic_vector | 443 | 0.772 | 0.810 | 3.5% (20 runs) | instructions move the same way |
| programs/binary_trees | 1059 | 0.798 | 0.736 | 5.2% (20 runs) | instructions move the same way |
| corpus2/heldout/dijkstra_routes | 1150 | 0.812 | 0.639 | 8.1% (17 runs) | instructions move the same way |
| awfy/permute | 36 | 0.841 | 0.828 | 1.9% (20 runs) | instructions move the same way |
| corpus/lua_lexer | 949 | 0.866 | 0.851 | 3.9% (17 runs) | instructions move the same way |
| corpus/sha2_base64 | 381 | 0.929 | 0.933 | 2.0% (17 runs) | instructions move the same way |
| programs/string_ops | 1039 | 0.951 | 1.004 | 8.6% (20 runs) | instructions equal: layout / IPC |
| lib/tonumber_bad | 114 | 0.953 | 0.996 | 4.1% (17 runs) | instructions equal: layout / IPC |
| corpus2/dev/wordfreq_wrap | 499 | 0.965 | 0.951 | 1.9% (5 runs) | instructions move the same way |
| awfy/towers | 46 | 0.966 | 0.950 | 2.0% (20 runs) | instructions move the same way |
| lib/tsort_str | 151 | 0.970 | 0.991 | 1.9% (5 runs) | instructions equal: layout / IPC |
| lib/tonumber_flt | 168 | 0.973 | 0.996 | 0.2% (5 runs) |  |
| corpus/mustache_lustache | 1448 | 0.975 | 0.996 | 2.5% (17 runs) |  |
| apps/raytrace | 105 | 0.976 | 0.958 | 7.1% (20 runs) |  |
| awfy/deltablue | 101 | 0.977 | 0.968 | 2.7% (17 runs) |  |
| corpus/containers_lru_heap | 1201 | 0.977 | 0.998 | 2.4% (17 runs) |  |
| corpus2/heldout/markov_text | 1110 | 0.977 | 0.994 | 1.0% (2 runs) |  |
| lib/tsort_cmp | 256 | 0.979 | 0.993 | 0.2% (2 runs) |  |
| lib/tsort_int | 218 | 0.980 | 0.987 | 0.1% (2 runs) |  |

### 5.4 plain vs 22670a1: 20 biggest improvements by cycles (>= 20 Mcyc)

| program | Mcyc | cyc ratio | ins ratio | cycle spread of readings | note |
|---|---|---|---|---|---|
| programs/fib | 632 | 0.503 | 0.505 | 4.0% (20 runs) | instructions move the same way |
| corpus/classic_vector | 652 | 0.807 | 0.782 | 3.3% (20 runs) | instructions move the same way |
| programs/binary_trees | 1454 | 0.808 | 0.789 | 3.9% (20 runs) | instructions move the same way |
| awfy/permute | 38 | 0.865 | 0.834 | 5.0% (20 runs) | instructions move the same way |
| lib/m_min | 228 | 0.917 | 1.000 | 13.3% (20 runs) | instructions equal: layout / IPC |
| corpus2/dev/diff_levenshtein | 441 | 0.952 | 1.000 | 3.6% (5 runs) | instructions equal: layout / IPC |
| lib/tconcat | 167 | 0.955 | 1.000 | 10.8% (20 runs) | instructions equal: layout / IPC |
| corpus2/heldout/dijkstra_routes | 1459 | 0.957 | 0.935 | 4.2% (17 runs) | instructions move the same way |
| awfy/towers | 49 | 0.959 | 0.945 | 4.6% (20 runs) | instructions move the same way |
| lib/tonumber_flt | 196 | 0.961 | 1.000 | 1.6% (5 runs) | instructions equal: layout / IPC |
| real/lua_interp | 106 | 0.964 | 1.007 | 0.2% (5 runs) | instructions equal: layout / IPC |
| lib/find_pat | 263 | 0.966 | 1.000 | 11.4% (20 runs) | instructions equal: layout / IPC |
| corpus2/dev/text_adventure | 2668 | 0.970 | 1.000 | 2.4% (5 runs) | instructions equal: layout / IPC |
| programs/string_ops | 1091 | 0.971 | 1.000 | 9.5% (20 runs) |  |
| corpus2/dev/varargs_util | 1519 | 0.976 | 1.000 | 3.1% (5 runs) |  |
| corpus2/heldout/scheme_interp | 1514 | 0.977 | 1.000 | 6.6% (17 runs) |  |
| apps/raytrace | 118 | 0.979 | 0.950 | 3.1% (20 runs) |  |
| corpus/template_etlua | 1564 | 0.981 | 1.000 | 0.4% (2 runs) |  |
| lib/tostring_int | 134 | 0.982 | 1.000 | 6.0% (20 runs) |  |
| programs/fannkuch | 1070 | 0.983 | 1.000 | 0.4% (2 runs) |  |

### 5.4 PGO vs 22670a1: 20 biggest improvements by cycles (>= 20 Mcyc)

| program | Mcyc | cyc ratio | ins ratio | cycle spread of readings | note |
|---|---|---|---|---|---|
| programs/fib | 631 | 0.504 | 0.505 | 5.9% (20 runs) | instructions move the same way |
| programs/binary_trees | 1210 | 0.808 | 0.770 | 3.8% (20 runs) | instructions move the same way |
| corpus/classic_vector | 572 | 0.849 | 0.831 | 3.6% (20 runs) | instructions move the same way |
| awfy/permute | 38 | 0.865 | 0.834 | 1.8% (20 runs) | instructions move the same way |
| corpus2/dev/diff_levenshtein | 409 | 0.945 | 1.000 | 1.0% (5 runs) | instructions equal: layout / IPC |
| corpus2/heldout/dijkstra_routes | 1416 | 0.952 | 0.935 | 3.5% (17 runs) | instructions move the same way |
| lib/load_small | 160 | 0.953 | 1.014 | 2.5% (20 runs) |  |
| lib/tonumber_num | 204 | 0.955 | 1.000 | 11.2% (20 runs) | instructions equal: layout / IPC |
| awfy/towers | 47 | 0.963 | 0.951 | 7.7% (20 runs) | instructions move the same way |
| programs/string_ops | 996 | 0.966 | 1.000 | 8.5% (20 runs) | instructions equal: layout / IPC |
| lib/co_wrap | 196 | 0.966 | 1.000 | 5.7% (20 runs) | instructions equal: layout / IPC |
| real/lua_interp | 102 | 0.968 | 1.009 | 1.1% (5 runs) | instructions equal: layout / IPC |
| corpus/csv_ini_yaml | 1591 | 0.971 | 1.000 | 3.2% (20 runs) |  |
| awfy/deltablue | 111 | 0.978 | 0.973 | 2.3% (17 runs) |  |
| lib/m_random | 78 | 0.981 | 1.000 | 0.3% (2 runs) |  |
| corpus/template_etlua | 1265 | 0.982 | 1.000 | 1.0% (2 runs) |  |
| apps/raytrace | 110 | 0.983 | 0.957 | 3.4% (20 runs) |  |
| lib/sel_loop | 215 | 0.984 | 1.000 | 5.8% (17 runs) |  |
| corpus2/heldout/scheme_interp | 1288 | 0.985 | 1.001 | 4.4% (17 runs) |  |
| lib/byte | 50 | 0.987 | 1.000 | 1.0% (5 runs) |  |

### 5.5 plain vs 22670a1: regressions (instructions > +2% or cycles > +3%, >= 20 Mcyc)

| program | Mcyc | cyc ratio | ins ratio | cycle spread of readings | note |
|---|---|---|---|---|---|
| jit-trace/rec_loop | 93 | 1.140 | 1.173 | 8.7% (20 runs) | instructions move the same way |
| corpus2/dev/regex_nfa | 720 | 1.109 | 0.999 | 4.7% (20 runs) | instructions equal: layout / IPC |
| lib/tonumber_bad | 156 | 1.083 | 1.000 | 6.0% (17 runs) | instructions equal: layout / IPC |
| lib/load_small | 241 | 1.057 | 1.000 | 3.3% (20 runs) | instructions equal: layout / IPC |
| programs/matmul | 442 | 1.050 | 1.000 | 0.2% (2 runs) | instructions equal: layout / IPC |
| lib/tunpack | 124 | 1.049 | 1.000 | 0.8% (5 runs) | instructions equal: layout / IPC |
| programs/table_array | 374 | 1.046 | 1.000 | 0.0% (2 runs) | instructions equal: layout / IPC |
| lib/tsort_cmp | 276 | 1.042 | 1.001 | 0.3% (2 runs) | instructions equal: layout / IPC |
| programs/load_big | 766 | 1.041 | 1.000 | 3.2% (17 runs) | instructions equal: layout / IPC |
| lib/sel_n | 332 | 1.035 | 1.000 | 7.8% (20 runs) | instructions equal: layout / IPC |
| real/lua_interp | 107 | 1.035 | 1.000 | 1.9% (5 runs) | instructions equal: layout / IPC |
| lib/concat_op_str | 139 | 1.031 | 1.000 | 0.9% (2 runs) | instructions equal: layout / IPC |
| programs/gc_churn | 2223 | 1.030 | 1.000 | 4.4% (5 runs) | instructions equal: layout / IPC |

### 5.5 PGO vs 22670a1: regressions (instructions > +2% or cycles > +3%, >= 20 Mcyc)

| program | Mcyc | cyc ratio | ins ratio | cycle spread of readings | note |
|---|---|---|---|---|---|
| jit-trace/rec_loop | 92 | 1.133 | 1.174 | 2.6% (20 runs) | instructions move the same way |
| lib/concat_op_str | 127 | 1.078 | 1.017 | 0.2% (2 runs) | instructions move the same way |
| lib/error_tbl | 215 | 1.043 | 1.006 | 0.9% (5 runs) | instructions equal: layout / IPC |
| corpus2/dev/iter_pipeline | 171 | 1.042 | 0.995 | 0.9% (2 runs) | instructions equal: layout / IPC |
| lib/tpack | 118 | 1.035 | 1.023 | 0.5% (5 runs) | instructions move the same way |
| corpus/penlight_mix | 2153 | 0.997 | 1.028 | 2.0% (20 runs) |  |

### 5.4 plain vs 22670a1: regressions (instructions > +2% or cycles > +3%, >= 20 Mcyc)

| program | Mcyc | cyc ratio | ins ratio | cycle spread of readings | note |
|---|---|---|---|---|---|
| jit-trace/rec_loop | 90 | 1.118 | 1.171 | 11.6% (20 runs) | instructions move the same way |
| lib/co_yield | 244 | 1.084 | 1.000 | 8.5% (20 runs) | instructions equal: layout / IPC |
| lib/pcall_err | 135 | 1.054 | 1.000 | 6.4% (20 runs) | instructions equal: layout / IPC |
| corpus2/heldout/prototype_objects | 1517 | 1.047 | 0.995 | 11.1% (20 runs) | instructions equal: layout / IPC |
| programs/coroutines | 1878 | 1.044 | 1.000 | 3.6% (20 runs) | instructions equal: layout / IPC |
| corpus2/dev/regex_nfa | 1394 | 1.041 | 1.000 | 7.2% (20 runs) | instructions equal: layout / IPC |
| programs/matmul | 449 | 1.041 | 1.000 | 1.0% (2 runs) | instructions equal: layout / IPC |
| corpus2/dev/event_promise | 475 | 1.037 | 1.000 | 0.5% (2 runs) | instructions equal: layout / IPC |
| lib/sel_n | 332 | 1.035 | 1.000 | 22.6% (20 runs) | instructions equal: layout / IPC |
| programs/json | 1985 | 1.035 | 1.001 | 0.2% (2 runs) | instructions equal: layout / IPC |
| lib/tremove_front | 187 | 1.034 | 0.999 | 1.2% (5 runs) | instructions equal: layout / IPC |
| lib/error_tbl | 270 | 1.034 | 1.000 | 3.4% (5 runs) | instructions equal: layout / IPC |
| lib/concat_op_str | 247 | 1.032 | 1.000 | 0.5% (2 runs) | instructions equal: layout / IPC |

### 5.4 PGO vs 22670a1: regressions (instructions > +2% or cycles > +3%, >= 20 Mcyc)

| program | Mcyc | cyc ratio | ins ratio | cycle spread of readings | note |
|---|---|---|---|---|---|
| jit-trace/rec_loop | 89 | 1.122 | 1.172 | 1.3% (20 runs) | instructions move the same way |
| corpus2/heldout/prototype_objects | 1457 | 1.052 | 0.994 | 9.1% (20 runs) | instructions equal: layout / IPC |
| lib/m_min | 232 | 1.045 | 1.000 | 11.5% (20 runs) | instructions equal: layout / IPC |
| lib/co_yield | 188 | 1.043 | 1.000 | 18.7% (20 runs) | instructions equal: layout / IPC |
| lib/tpack | 170 | 1.030 | 1.004 | 2.2% (5 runs) | instructions equal: layout / IPC |

