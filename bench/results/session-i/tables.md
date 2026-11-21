# Tables, session-i (subject 22670a1)

Ratios below 1: first column faster/fewer. Ours = min over all readings (2 quick, 5 full, 15-run serial reruns). Cached columns as stored.


# Aggregates, readings A: the fast pass only (min of 2 or 5 runs per program)

## Headline geomeans (cycles, ours/ref)

| build | /LuaJIT cyc (n) | /LuaJIT ins | /PUC same version cyc (223) | /PUC ins |
|---|---|---|---|---|
| 5.5 plain | 0.955 (219) | 1.360 | 0.218 | 0.222 |
| 5.5 PGO | 0.873 (219) | 1.253 | 0.200 | 0.205 |
| 5.4 plain | 1.016 (219) | 1.447 | 0.227 | 0.236 |
| 5.4 PGO | 0.929 (219) | 1.334 | 0.208 | 0.218 |

- 5.5 plain / LuaJIT -joff (219): 0.323
- 5.5 PGO / LuaJIT -joff (219): 0.295
- cached session-h 8c39465 (same programs): 5.5 plain/LJ 0.955, PGO/LJ 0.871, 5.4 plain/LJ 1.020, 5.4 PGO/LJ 0.929
- cached 1a5aad0 (same programs): 5.5 plain/LJ 0.964, PGO/LJ 0.879, 5.4 plain/LJ 1.020, 5.4 PGO/LJ 0.934
- cached 108b935 (same programs): plain/LJ 0.969, PGO/LJ 0.883
- our 5.4 build / our 5.5 build cycles: plain 1.063, PGO 1.063; PGO/plain: 5.5 0.916, 5.4 0.916

## Subject vs session-h 8c39465 (set B merged), session-h set A, 1a5aad0 and 108b935 (geomean, 223 programs)

| build | vs | cycles | instructions | cyc >3% better / within / >3% worse | ins >3% fewer / more |
|---|---|---|---|---|---|
| 5.5 plain | 8c39465 | 1.000 | 0.993 | 12 / 198 / 13 | 8 / 2 |
| 5.5 plain | 8c39465 setA | 0.992 | 0.993 | 23 / 196 / 4 | 8 / 2 |
| 5.5 plain | 1a5aad0 | 0.991 | 0.976 | 36 / 168 / 19 | 33 / 3 |
| 5.5 plain | 108b935 | 0.985 | 0.955 | 46 / 152 / 25 | 84 / 2 |
| 5.5 PGO | 8c39465 | 1.002 | 0.993 | 8 / 198 / 17 | 8 / 2 |
| 5.5 PGO | 8c39465 setA | 0.995 | 0.993 | 12 / 206 / 5 | 8 / 2 |
| 5.5 PGO | 1a5aad0 | 0.994 | 0.974 | 28 / 173 / 22 | 37 / 1 |
| 5.5 PGO | 108b935 | 0.989 | 0.953 | 44 / 148 / 31 | 87 / 2 |
| 5.4 plain | 8c39465 | 0.997 | 0.991 | 16 / 191 / 16 | 9 / 3 |
| 5.4 plain | 8c39465 setA | 0.989 | 0.991 | 21 / 194 / 8 | 9 / 3 |
| 5.4 plain | 1a5aad0 | 0.996 | 0.978 | 28 / 170 / 25 | 30 / 3 |
| 5.4 PGO | 8c39465 | 1.000 | 0.991 | 8 / 202 / 13 | 9 / 3 |
| 5.4 PGO | 8c39465 setA | 0.992 | 0.991 | 13 / 204 / 6 | 9 / 3 |
| 5.4 PGO | 1a5aad0 | 0.995 | 0.976 | 28 / 175 / 20 | 33 / 1 |

### Per set, geomean subject / 8c39465 and subject / 1a5aad0 (cycles / instructions)

| set | n | plain vs h cyc | ins | PGO vs h cyc | ins | 5.4 plain vs h cyc | ins | 5.4 PGO vs h cyc | ins | plain vs 1a5 cyc | ins | PGO vs 1a5 cyc | ins | 5.4 plain vs 1a5 cyc | ins | 5.4 PGO vs 1a5 cyc | ins |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| apps | 5 | 0.982 | 0.963 | 0.977 | 0.962 | 0.882 | 0.888 | 0.889 | 0.887 | 0.885 | 0.861 | 0.919 | 0.856 | 0.944 | 0.883 | 0.982 | 0.881 |
| awfy | 14 | 0.989 | 0.965 | 0.989 | 0.964 | 0.982 | 0.961 | 0.985 | 0.959 | 0.966 | 0.931 | 0.967 | 0.931 | 0.957 | 0.931 | 0.961 | 0.928 |
| corpus | 18 | 1.007 | 0.999 | 1.008 | 1.002 | 1.008 | 1.006 | 1.013 | 1.006 | 0.989 | 0.975 | 0.991 | 0.971 | 1.001 | 0.984 | 0.992 | 0.981 |
| corpus2/dev | 17 | 0.994 | 0.995 | 1.001 | 0.995 | 0.994 | 1.003 | 1.010 | 1.002 | 0.985 | 0.965 | 0.990 | 0.961 | 0.991 | 0.973 | 0.991 | 0.968 |
| corpus2/heldout | 11 | 1.002 | 0.991 | 1.000 | 0.991 | 0.981 | 0.967 | 0.987 | 0.968 | 0.961 | 0.906 | 0.956 | 0.905 | 0.942 | 0.886 | 0.948 | 0.883 |
| jit-trace | 29 | 1.003 | 1.000 | 1.003 | 1.000 | 1.000 | 1.000 | 1.001 | 1.000 | 1.001 | 0.991 | 1.004 | 0.993 | 0.999 | 0.994 | 1.001 | 0.993 |
| lib | 95 | 1.000 | 0.994 | 1.002 | 0.994 | 1.001 | 0.995 | 1.003 | 0.995 | 1.004 | 0.995 | 1.003 | 0.993 | 1.011 | 0.996 | 1.005 | 0.995 |
| programs | 25 | 1.003 | 0.998 | 1.011 | 0.998 | 1.012 | 0.998 | 1.008 | 0.998 | 0.997 | 0.981 | 1.008 | 0.981 | 1.005 | 0.984 | 1.006 | 0.983 |
| real | 5 | 1.014 | 0.999 | 1.018 | 0.999 | 1.003 | 1.000 | 1.013 | 0.999 | 0.945 | 0.936 | 0.957 | 0.939 | 0.951 | 0.940 | 0.952 | 0.944 |
| real_dis | 4 | 1.010 | 0.999 | 1.006 | 1.000 | 0.969 | 0.987 | 0.988 | 0.989 | 0.985 | 0.984 | 0.993 | 0.975 | 1.000 | 1.000 | 0.991 | 0.994 |
| ALL | 223 | 1.000 | 0.993 | 1.002 | 0.993 | 0.997 | 0.991 | 1.000 | 0.991 | 0.991 | 0.976 | 0.994 | 0.974 | 0.996 | 0.978 | 0.995 | 0.976 |

# Aggregates, readings B: with the 15-run serial reruns of 83 programs merged (min over all readings)

## Headline geomeans (cycles, ours/ref)

| build | /LuaJIT cyc (n) | /LuaJIT ins | /PUC same version cyc (223) | /PUC ins |
|---|---|---|---|---|
| 5.5 plain | 0.950 (219) | 1.360 | 0.217 | 0.222 |
| 5.5 PGO | 0.869 (219) | 1.253 | 0.199 | 0.205 |
| 5.4 plain | 1.010 (219) | 1.447 | 0.225 | 0.236 |
| 5.4 PGO | 0.925 (219) | 1.334 | 0.207 | 0.218 |

- 5.5 plain / LuaJIT -joff (219): 0.321
- 5.5 PGO / LuaJIT -joff (219): 0.294
- cached session-h 8c39465 (same programs): 5.5 plain/LJ 0.955, PGO/LJ 0.871, 5.4 plain/LJ 1.020, 5.4 PGO/LJ 0.929
- cached 1a5aad0 (same programs): 5.5 plain/LJ 0.964, PGO/LJ 0.879, 5.4 plain/LJ 1.020, 5.4 PGO/LJ 0.934
- cached 108b935 (same programs): plain/LJ 0.969, PGO/LJ 0.883
- our 5.4 build / our 5.5 build cycles: plain 1.062, PGO 1.064; PGO/plain: 5.5 0.916, 5.4 0.918

## Subject vs session-h 8c39465 (set B merged), session-h set A, 1a5aad0 and 108b935 (geomean, 223 programs)

| build | vs | cycles | instructions | cyc >3% better / within / >3% worse | ins >3% fewer / more |
|---|---|---|---|---|---|
| 5.5 plain | 8c39465 | 0.995 | 0.993 | 19 / 199 / 5 | 8 / 2 |
| 5.5 plain | 8c39465 setA | 0.987 | 0.993 | 31 / 189 / 3 | 8 / 2 |
| 5.5 plain | 1a5aad0 | 0.986 | 0.976 | 45 / 163 / 15 | 33 / 3 |
| 5.5 plain | 108b935 | 0.981 | 0.955 | 56 / 150 / 17 | 84 / 2 |
| 5.5 PGO | 8c39465 | 0.998 | 0.993 | 11 / 208 / 4 | 8 / 2 |
| 5.5 PGO | 8c39465 setA | 0.991 | 0.993 | 20 / 201 / 2 | 8 / 2 |
| 5.5 PGO | 1a5aad0 | 0.989 | 0.974 | 32 / 175 / 16 | 37 / 1 |
| 5.5 PGO | 108b935 | 0.985 | 0.953 | 47 / 155 / 21 | 87 / 2 |
| 5.4 plain | 8c39465 | 0.991 | 0.991 | 21 / 196 / 6 | 9 / 3 |
| 5.4 plain | 8c39465 setA | 0.983 | 0.991 | 32 / 188 / 3 | 9 / 3 |
| 5.4 plain | 1a5aad0 | 0.990 | 0.978 | 33 / 178 / 12 | 30 / 3 |
| 5.4 PGO | 8c39465 | 0.996 | 0.991 | 11 / 208 / 4 | 9 / 3 |
| 5.4 PGO | 8c39465 setA | 0.988 | 0.991 | 20 / 199 / 4 | 9 / 3 |
| 5.4 PGO | 1a5aad0 | 0.991 | 0.976 | 31 / 183 / 9 | 33 / 1 |

### Per set, geomean subject / 8c39465 and subject / 1a5aad0 (cycles / instructions)

| set | n | plain vs h cyc | ins | PGO vs h cyc | ins | 5.4 plain vs h cyc | ins | 5.4 PGO vs h cyc | ins | plain vs 1a5 cyc | ins | PGO vs 1a5 cyc | ins | 5.4 plain vs 1a5 cyc | ins | 5.4 PGO vs 1a5 cyc | ins |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| apps | 5 | 0.968 | 0.963 | 0.966 | 0.962 | 0.870 | 0.888 | 0.879 | 0.887 | 0.872 | 0.861 | 0.908 | 0.856 | 0.931 | 0.883 | 0.970 | 0.881 |
| awfy | 14 | 0.986 | 0.965 | 0.986 | 0.964 | 0.978 | 0.961 | 0.980 | 0.959 | 0.962 | 0.931 | 0.964 | 0.931 | 0.953 | 0.931 | 0.956 | 0.928 |
| corpus | 18 | 1.003 | 0.999 | 1.004 | 1.000 | 1.004 | 1.006 | 1.009 | 1.006 | 0.986 | 0.975 | 0.987 | 0.969 | 0.997 | 0.984 | 0.988 | 0.981 |
| corpus2/dev | 17 | 0.992 | 0.995 | 0.999 | 0.995 | 0.991 | 1.003 | 1.007 | 1.002 | 0.983 | 0.965 | 0.988 | 0.961 | 0.988 | 0.973 | 0.988 | 0.968 |
| corpus2/heldout | 11 | 0.997 | 0.991 | 0.997 | 0.991 | 0.974 | 0.967 | 0.982 | 0.968 | 0.956 | 0.906 | 0.953 | 0.905 | 0.936 | 0.886 | 0.943 | 0.883 |
| jit-trace | 29 | 1.002 | 1.000 | 1.003 | 1.000 | 1.000 | 1.000 | 1.001 | 1.000 | 1.001 | 0.991 | 1.004 | 0.993 | 0.998 | 0.994 | 1.001 | 0.993 |
| lib | 95 | 0.993 | 0.994 | 0.995 | 0.994 | 0.993 | 0.995 | 0.998 | 0.995 | 0.998 | 0.995 | 0.996 | 0.993 | 1.003 | 0.996 | 1.000 | 0.995 |
| programs | 25 | 0.997 | 0.998 | 1.005 | 0.998 | 1.005 | 0.998 | 1.003 | 0.998 | 0.991 | 0.981 | 1.002 | 0.981 | 0.997 | 0.984 | 1.001 | 0.983 |
| real | 5 | 1.007 | 0.999 | 1.012 | 0.999 | 0.997 | 1.000 | 1.005 | 0.999 | 0.939 | 0.936 | 0.952 | 0.939 | 0.946 | 0.940 | 0.945 | 0.944 |
| real_dis | 4 | 1.010 | 0.999 | 1.006 | 1.000 | 0.969 | 0.987 | 0.988 | 0.989 | 0.985 | 0.984 | 0.993 | 0.975 | 1.000 | 1.000 | 0.991 | 0.994 |
| ALL | 223 | 0.995 | 0.993 | 0.998 | 0.993 | 0.991 | 0.991 | 0.996 | 0.991 | 0.986 | 0.976 | 0.989 | 0.974 | 0.990 | 0.978 | 0.991 | 0.976 |

### 5.5 plain vs 8c39465: 20 biggest improvements by cycles (>= 20 Mcyc)

| program | Mcyc | cyc ratio | ins ratio | cycle spread of readings | note |
|---|---|---|---|---|---|
| lib/tremove_front | 149 | 0.591 | 0.547 | 6.1% (20 runs) | instructions move the same way |
| awfy/list | 97 | 0.777 | 0.639 | 2.0% (20 runs) | instructions move the same way |
| apps/raytrace | 115 | 0.827 | 0.837 | 4.6% (20 runs) | instructions move the same way |
| corpus2/heldout/stream_closures | 813 | 0.870 | 0.849 | 3.7% (20 runs) | instructions move the same way |
| programs/load_big | 736 | 0.908 | 1.000 | 2.4% (20 runs) | instructions equal: layout / IPC |
| lib/co_yield | 225 | 0.939 | 1.000 | 15.5% (20 runs) | instructions equal: layout / IPC |
| corpus2/dev/regex_nfa | 650 | 0.939 | 1.017 | 12.2% (20 runs) |  |
| corpus2/dev/particles_physics | 401 | 0.948 | 0.959 | 1.9% (5 runs) | instructions move the same way |
| lib/load_small | 228 | 0.949 | 1.000 | 2.7% (17 runs) | instructions equal: layout / IPC |
| corpus/parser_combinator | 2293 | 0.951 | 0.983 | 10.1% (20 runs) | instructions move the same way |
| programs/coroutines | 1716 | 0.952 | 1.000 | 7.7% (20 runs) | instructions equal: layout / IPC |
| lib/tonumber_bad | 144 | 0.961 | 1.000 | 6.6% (20 runs) | instructions equal: layout / IPC |
| lib/concat_op_str | 135 | 0.961 | 1.000 | 6.6% (20 runs) | instructions equal: layout / IPC |
| corpus2/heldout/prototype_objects | 1455 | 0.963 | 0.915 | 6.8% (20 runs) | instructions move the same way |
| corpus/class_middleclass | 480 | 0.966 | 0.983 | 9.7% (20 runs) | instructions move the same way |
| corpus/testrunner_luaunit | 1481 | 0.969 | 0.976 | 1.1% (5 runs) | instructions move the same way |
| corpus2/dev/oop_mixins | 1426 | 0.969 | 0.952 | 6.3% (20 runs) | instructions move the same way |
| lib/error_tbl | 261 | 0.970 | 1.000 | 5.4% (17 runs) | instructions equal: layout / IPC |
| lib/it_pairs_hash | 103 | 0.970 | 1.000 | 5.1% (20 runs) |  |
| programs/table_array | 357 | 0.972 | 1.000 | 0.3% (2 runs) |  |

### 5.5 PGO vs 8c39465: 20 biggest improvements by cycles (>= 20 Mcyc)

| program | Mcyc | cyc ratio | ins ratio | cycle spread of readings | note |
|---|---|---|---|---|---|
| lib/tremove_front | 132 | 0.595 | 0.568 | 3.1% (20 runs) | instructions move the same way |
| awfy/list | 94 | 0.766 | 0.635 | 1.6% (20 runs) | instructions move the same way |
| apps/raytrace | 107 | 0.825 | 0.833 | 3.9% (20 runs) | instructions move the same way |
| corpus2/heldout/stream_closures | 745 | 0.869 | 0.851 | 2.0% (20 runs) | instructions move the same way |
| lib/load_small | 156 | 0.913 | 0.997 | 2.6% (17 runs) | instructions equal: layout / IPC |
| lib/error_tbl | 206 | 0.948 | 1.001 | 5.2% (17 runs) | instructions equal: layout / IPC |
| corpus2/dev/particles_physics | 375 | 0.954 | 0.958 | 2.0% (5 runs) | instructions move the same way |
| corpus2/dev/memo_cache | 1503 | 0.962 | 0.978 | 2.8% (5 runs) | instructions move the same way |
| lib/it_pairs_hash | 104 | 0.964 | 1.000 | 3.8% (20 runs) | instructions equal: layout / IPC |
| corpus/class_middleclass | 472 | 0.969 | 0.983 | 5.9% (20 runs) | instructions move the same way |
| corpus2/dev/oop_mixins | 1350 | 0.969 | 0.954 | 4.9% (20 runs) | instructions move the same way |
| awfy/towers | 48 | 0.972 | 0.991 | 0.6% (5 runs) |  |
| corpus2/dev/lua_parser | 1104 | 0.975 | 1.008 | 5.2% (20 runs) |  |
| corpus/parser_combinator | 1857 | 0.975 | 0.981 | 1.3% (20 runs) |  |
| corpus2/heldout/prototype_objects | 1403 | 0.978 | 0.914 | 8.6% (20 runs) |  |
| corpus/lua_lexer | 1096 | 0.980 | 1.000 | 3.2% (20 runs) |  |
| lib/concat_op_str | 118 | 0.980 | 1.000 | 22.5% (20 runs) |  |
| corpus2/heldout/scheme_interp | 1221 | 0.981 | 0.901 | 3.8% (5 runs) |  |
| lib/tremove_pop | 172 | 0.983 | 0.989 | 2.4% (20 runs) |  |
| programs/fannkuch | 1048 | 0.983 | 0.976 | 1.8% (5 runs) |  |

### 5.4 plain vs 8c39465: 20 biggest improvements by cycles (>= 20 Mcyc)

| program | Mcyc | cyc ratio | ins ratio | cycle spread of readings | note |
|---|---|---|---|---|---|
| lib/tremove_front | 181 | 0.634 | 0.629 | 4.6% (20 runs) | instructions move the same way |
| apps/expr_lang | 161 | 0.673 | 0.798 | 3.2% (20 runs) | instructions move the same way |
| awfy/list | 98 | 0.755 | 0.613 | 4.5% (20 runs) | instructions move the same way |
| apps/raytrace | 120 | 0.757 | 0.692 | 3.1% (20 runs) | instructions move the same way |
| lib/gsub_fn | 178 | 0.786 | 1.000 | 3.2% (20 runs) | instructions equal: layout / IPC |
| corpus2/heldout/stream_closures | 871 | 0.844 | 0.796 | 2.0% (20 runs) | instructions move the same way |
| corpus2/dev/diff_levenshtein | 463 | 0.897 | 1.000 | 3.0% (17 runs) | instructions equal: layout / IPC |
| lib/co_yield | 225 | 0.927 | 1.000 | 6.4% (20 runs) | instructions equal: layout / IPC |
| lib/m_tointeger | 224 | 0.938 | 1.000 | 6.7% (17 runs) | instructions equal: layout / IPC |
| corpus2/dev/real_dis_arm | 2419 | 0.938 | 0.951 | 2.2% (5 runs) | instructions move the same way |
| real/template_engine | 744 | 0.946 | 1.000 | 10.1% (17 runs) | instructions equal: layout / IPC |
| corpus2/dev/oop_mixins | 1410 | 0.951 | 0.961 | 5.2% (20 runs) | instructions move the same way |
| corpus2/heldout/prototype_objects | 1449 | 0.951 | 0.847 | 7.9% (20 runs) | instructions move the same way |
| corpus2/dev/lua_parser | 1531 | 0.957 | 0.987 | 2.0% (20 runs) | instructions move the same way |
| lib/gmatch_words | 180 | 0.958 | 1.000 | 0.7% (2 runs) | instructions equal: layout / IPC |
| lib/pcall_err | 129 | 0.961 | 1.000 | 8.3% (20 runs) | instructions equal: layout / IPC |
| corpus2/heldout/scheme_interp | 1549 | 0.961 | 1.067 | 3.0% (5 runs) |  |
| lib/it_pairs_hash | 112 | 0.963 | 1.000 | 9.7% (20 runs) | instructions equal: layout / IPC |
| corpus2/dev/template_gsub | 1760 | 0.964 | 1.000 | 0.2% (2 runs) | instructions equal: layout / IPC |
| corpus/class_middleclass | 507 | 0.965 | 0.984 | 6.3% (20 runs) | instructions move the same way |

### 5.4 PGO vs 8c39465: 20 biggest improvements by cycles (>= 20 Mcyc)

| program | Mcyc | cyc ratio | ins ratio | cycle spread of readings | note |
|---|---|---|---|---|---|
| apps/expr_lang | 146 | 0.684 | 0.809 | 4.3% (20 runs) | instructions move the same way |
| lib/tremove_front | 166 | 0.723 | 0.674 | 4.8% (20 runs) | instructions move the same way |
| awfy/list | 97 | 0.749 | 0.608 | 10.0% (20 runs) | instructions move the same way |
| apps/raytrace | 112 | 0.758 | 0.682 | 5.5% (20 runs) | instructions move the same way |
| corpus2/heldout/stream_closures | 810 | 0.860 | 0.792 | 2.1% (20 runs) | instructions move the same way |
| corpus2/heldout/prototype_objects | 1386 | 0.947 | 0.860 | 10.3% (20 runs) | instructions move the same way |
| corpus2/dev/real_dis_arm | 2140 | 0.957 | 0.956 | 2.5% (5 runs) | instructions move the same way |
| lib/tpack | 165 | 0.957 | 0.989 | 10.9% (20 runs) | instructions move the same way |
| corpus2/heldout/scheme_interp | 1308 | 0.965 | 1.063 | 1.2% (5 runs) |  |
| lib/m_min | 222 | 0.968 | 1.000 | 16.4% (20 runs) | instructions equal: layout / IPC |
| corpus2/dev/lua_parser | 1327 | 0.969 | 0.985 | 3.4% (20 runs) | instructions move the same way |
| awfy/permute | 43 | 0.972 | 0.976 | 0.7% (5 runs) |  |
| corpus/msgpack_pure | 1888 | 0.973 | 0.998 | 3.0% (20 runs) |  |
| corpus2/dev/memo_cache | 1521 | 0.974 | 0.978 | 1.7% (5 runs) |  |
| corpus/csv_ini_yaml | 1638 | 0.977 | 1.000 | 1.7% (17 runs) |  |
| awfy/towers | 49 | 0.977 | 0.990 | 1.0% (5 runs) |  |
| lib/tostring_tbl | 106 | 0.977 | 1.000 | 5.6% (20 runs) |  |
| lib/fmt_mixed | 56 | 0.983 | 0.999 | 7.5% (20 runs) |  |
| corpus/class_middleclass | 498 | 0.984 | 0.984 | 23.6% (20 runs) |  |
| lib/tconcat_num | 120 | 0.985 | 1.000 | 2.4% (20 runs) |  |

### 5.5 plain vs 8c39465: regressions (instructions > +2% or cycles > +3%, >= 20 Mcyc)

| program | Mcyc | cyc ratio | ins ratio | cycle spread of readings | note |
|---|---|---|---|---|---|
| corpus2/heldout/dijkstra_routes | 1449 | 1.138 | 1.291 | 2.1% (20 runs) | instructions move the same way |
| lib/find_pat | 272 | 1.050 | 1.000 | 31.8% (20 runs) | instructions equal: layout / IPC |
| programs/table_hash | 524 | 1.043 | 1.000 | 9.9% (20 runs) | instructions equal: layout / IPC |
| corpus/astar_jumper | 1680 | 1.042 | 1.073 | 14.5% (20 runs) | instructions move the same way |
| programs/pcall_error | 1520 | 1.034 | 1.000 | 3.4% (20 runs) | instructions equal: layout / IPC |

### 5.5 PGO vs 8c39465: regressions (instructions > +2% or cycles > +3%, >= 20 Mcyc)

| program | Mcyc | cyc ratio | ins ratio | cycle spread of readings | note |
|---|---|---|---|---|---|
| corpus2/heldout/dijkstra_routes | 1416 | 1.140 | 1.296 | 1.3% (20 runs) | instructions move the same way |
| lib/tonumber_bad | 119 | 1.070 | 0.996 | 6.1% (20 runs) | instructions equal: layout / IPC |
| corpus/astar_jumper | 1605 | 1.055 | 1.078 | 13.6% (20 runs) | instructions move the same way |
| programs/table_hash | 512 | 1.041 | 1.000 | 11.9% (20 runs) | instructions equal: layout / IPC |

### 5.4 plain vs 8c39465: regressions (instructions > +2% or cycles > +3%, >= 20 Mcyc)

| program | Mcyc | cyc ratio | ins ratio | cycle spread of readings | note |
|---|---|---|---|---|---|
| corpus/astar_jumper | 1877 | 1.141 | 1.149 | 5.4% (20 runs) | instructions move the same way |
| lib/m_min | 249 | 1.115 | 1.000 | 3.8% (20 runs) | instructions equal: layout / IPC |
| corpus2/dev/regex_nfa | 1339 | 1.115 | 1.130 | 6.6% (20 runs) | instructions move the same way |
| lib/load_small | 227 | 1.047 | 1.000 | 3.0% (17 runs) | instructions equal: layout / IPC |
| real/lua_interp | 110 | 1.032 | 1.002 | 7.5% (20 runs) | instructions equal: layout / IPC |
| lib/tmove_1k | 107 | 1.031 | 1.000 | 10.5% (17 runs) | instructions equal: layout / IPC |
| corpus2/heldout/scheme_interp | 1549 | 0.961 | 1.067 | 3.0% (5 runs) |  |

### 5.4 PGO vs 8c39465: regressions (instructions > +2% or cycles > +3%, >= 20 Mcyc)

| program | Mcyc | cyc ratio | ins ratio | cycle spread of readings | note |
|---|---|---|---|---|---|
| corpus/astar_jumper | 1793 | 1.126 | 1.157 | 6.9% (20 runs) | instructions move the same way |
| corpus2/dev/regex_nfa | 1153 | 1.112 | 1.127 | 4.8% (20 runs) | instructions move the same way |
| corpus2/dev/diff_levenshtein | 433 | 1.072 | 1.000 | 9.0% (17 runs) | instructions equal: layout / IPC |
| corpus/parser_combinator | 2071 | 1.048 | 1.000 | 2.4% (20 runs) | instructions equal: layout / IPC |
| corpus2/heldout/scheme_interp | 1308 | 0.965 | 1.063 | 1.2% (5 runs) |  |

### 5.5 plain vs 1a5aad0: 20 biggest improvements by cycles (>= 20 Mcyc)

| program | Mcyc | cyc ratio | ins ratio | cycle spread of readings | note |
|---|---|---|---|---|---|
| lib/tremove_front | 149 | 0.580 | 0.554 | 6.1% (20 runs) | instructions move the same way |
| apps/expr_lang | 140 | 0.687 | 0.762 | 7.7% (20 runs) | instructions move the same way |
| corpus2/heldout/roguelike_turns | 833 | 0.699 | 0.471 | 1.0% (2 runs) | instructions move the same way |
| awfy/list | 97 | 0.726 | 0.621 | 2.0% (20 runs) | instructions move the same way |
| programs/binary_trees | 1543 | 0.801 | 0.745 | 1.2% (2 runs) | instructions move the same way |
| apps/raytrace | 115 | 0.810 | 0.769 | 4.6% (20 runs) | instructions move the same way |
| real/game_loop | 35 | 0.813 | 0.820 | 0.1% (2 runs) | instructions move the same way |
| awfy/cd | 263 | 0.824 | 0.826 | 2.7% (17 runs) | instructions move the same way |
| corpus2/heldout/stream_closures | 813 | 0.850 | 0.819 | 3.7% (20 runs) | instructions move the same way |
| corpus2/dev/oop_mixins | 1426 | 0.857 | 0.817 | 6.3% (20 runs) | instructions move the same way |
| real/lua_interp | 103 | 0.883 | 0.882 | 18.2% (20 runs) | instructions move the same way |
| corpus2/dev/particles_physics | 401 | 0.888 | 0.863 | 1.9% (5 runs) | instructions move the same way |
| lib/fmt_q | 146 | 0.893 | 1.000 | 6.2% (20 runs) | instructions equal: layout / IPC |
| lib/tunpack | 118 | 0.910 | 0.996 | 0.3% (2 runs) | instructions equal: layout / IPC |
| corpus/classic_vector | 644 | 0.911 | 0.870 | 2.3% (20 runs) | instructions move the same way |
| lib/setmt | 188 | 0.918 | 0.956 | 1.0% (2 runs) | instructions move the same way |
| jit-trace/rec_loop | 82 | 0.920 | 0.877 | 0.0% (2 runs) | instructions move the same way |
| corpus2/heldout/scheme_interp | 1446 | 0.928 | 0.856 | 4.1% (5 runs) | instructions move the same way |
| corpus/class_middleclass | 480 | 0.932 | 0.965 | 9.7% (20 runs) | instructions move the same way |
| jit-trace/alloc_ctor | 190 | 0.933 | 0.932 | 4.6% (5 runs) | instructions move the same way |

### 5.5 PGO vs 1a5aad0: 20 biggest improvements by cycles (>= 20 Mcyc)

| program | Mcyc | cyc ratio | ins ratio | cycle spread of readings | note |
|---|---|---|---|---|---|
| lib/tremove_front | 132 | 0.610 | 0.572 | 3.1% (20 runs) | instructions move the same way |
| corpus2/heldout/roguelike_turns | 791 | 0.682 | 0.462 | 0.1% (2 runs) | instructions move the same way |
| apps/expr_lang | 128 | 0.707 | 0.772 | 2.8% (20 runs) | instructions move the same way |
| awfy/list | 94 | 0.727 | 0.624 | 1.6% (20 runs) | instructions move the same way |
| real/game_loop | 34 | 0.807 | 0.817 | 0.6% (2 runs) | instructions move the same way |
| programs/binary_trees | 1327 | 0.838 | 0.763 | 1.1% (2 runs) | instructions move the same way |
| awfy/cd | 244 | 0.839 | 0.828 | 1.6% (17 runs) | instructions move the same way |
| corpus2/heldout/stream_closures | 745 | 0.862 | 0.817 | 2.0% (20 runs) | instructions move the same way |
| corpus2/dev/oop_mixins | 1350 | 0.865 | 0.817 | 4.9% (20 runs) | instructions move the same way |
| corpus2/dev/particles_physics | 375 | 0.899 | 0.854 | 2.0% (5 runs) | instructions move the same way |
| corpus2/heldout/log_analyzer | 1763 | 0.913 | 0.972 | 2.9% (20 runs) | instructions move the same way |
| corpus2/dev/memo_cache | 1503 | 0.918 | 0.889 | 2.8% (5 runs) | instructions move the same way |
| jit-trace/rec_loop | 81 | 0.919 | 0.877 | 0.2% (2 runs) | instructions move the same way |
| corpus/classic_vector | 574 | 0.919 | 0.869 | 9.8% (20 runs) | instructions move the same way |
| real/lua_interp | 100 | 0.928 | 0.909 | 3.0% (20 runs) | instructions move the same way |
| apps/raytrace | 107 | 0.929 | 0.787 | 3.9% (20 runs) | instructions move the same way |
| lib/error_tbl | 206 | 0.930 | 1.007 | 5.2% (17 runs) | instructions equal: layout / IPC |
| corpus/class_middleclass | 472 | 0.935 | 0.965 | 5.9% (20 runs) | instructions move the same way |
| lib/fmt_g | 62 | 0.939 | 1.001 | 5.7% (20 runs) | instructions equal: layout / IPC |
| corpus/astar_jumper | 1605 | 0.941 | 0.864 | 13.6% (20 runs) | instructions move the same way |

### 5.4 plain vs 1a5aad0: 20 biggest improvements by cycles (>= 20 Mcyc)

| program | Mcyc | cyc ratio | ins ratio | cycle spread of readings | note |
|---|---|---|---|---|---|
| lib/tremove_front | 181 | 0.642 | 0.635 | 4.6% (20 runs) | instructions move the same way |
| corpus2/heldout/roguelike_turns | 872 | 0.673 | 0.487 | 0.2% (2 runs) | instructions move the same way |
| awfy/list | 98 | 0.704 | 0.585 | 4.5% (20 runs) | instructions move the same way |
| apps/raytrace | 120 | 0.813 | 0.738 | 3.1% (20 runs) | instructions move the same way |
| real/game_loop | 40 | 0.816 | 0.838 | 0.1% (2 runs) | instructions move the same way |
| awfy/cd | 291 | 0.817 | 0.826 | 1.5% (17 runs) | instructions move the same way |
| programs/binary_trees | 1800 | 0.818 | 0.779 | 0.1% (2 runs) | instructions move the same way |
| corpus2/heldout/stream_closures | 871 | 0.840 | 0.774 | 2.0% (20 runs) | instructions move the same way |
| corpus2/dev/oop_mixins | 1410 | 0.841 | 0.826 | 5.2% (20 runs) | instructions move the same way |
| corpus2/heldout/prototype_objects | 1449 | 0.900 | 0.727 | 7.9% (20 runs) | instructions move the same way |
| apps/events | 178 | 0.900 | 0.887 | 0.2% (2 runs) | instructions move the same way |
| corpus2/dev/ecs_world | 1006 | 0.910 | 0.863 | 0.1% (2 runs) | instructions move the same way |
| corpus/classic_vector | 807 | 0.911 | 0.890 | 2.6% (20 runs) | instructions move the same way |
| lib/sel_loop | 218 | 0.914 | 0.998 | 3.8% (5 runs) | instructions equal: layout / IPC |
| jit-trace/rec_loop | 80 | 0.914 | 0.878 | 0.1% (2 runs) | instructions move the same way |
| lib/tunpack | 127 | 0.914 | 0.995 | 1.3% (2 runs) | instructions equal: layout / IPC |
| lib/setmt | 229 | 0.924 | 0.966 | 0.4% (2 runs) | instructions move the same way |
| real/lua_interp | 110 | 0.924 | 0.880 | 7.5% (20 runs) | instructions move the same way |
| corpus2/heldout/scheme_interp | 1549 | 0.929 | 1.019 | 3.0% (5 runs) |  |
| jit-trace/alloc_ctor | 258 | 0.929 | 0.989 | 1.8% (5 runs) | instructions move the same way |

### 5.4 PGO vs 1a5aad0: 20 biggest improvements by cycles (>= 20 Mcyc)

| program | Mcyc | cyc ratio | ins ratio | cycle spread of readings | note |
|---|---|---|---|---|---|
| corpus2/heldout/roguelike_turns | 839 | 0.669 | 0.479 | 0.2% (2 runs) | instructions move the same way |
| awfy/list | 97 | 0.715 | 0.589 | 10.0% (20 runs) | instructions move the same way |
| lib/tremove_front | 166 | 0.731 | 0.681 | 4.8% (20 runs) | instructions move the same way |
| real/game_loop | 38 | 0.817 | 0.836 | 1.8% (2 runs) | instructions move the same way |
| programs/binary_trees | 1497 | 0.825 | 0.776 | 0.0% (2 runs) | instructions move the same way |
| awfy/cd | 270 | 0.831 | 0.825 | 4.3% (17 runs) | instructions move the same way |
| corpus2/dev/oop_mixins | 1340 | 0.848 | 0.821 | 3.0% (20 runs) | instructions move the same way |
| corpus2/heldout/stream_closures | 810 | 0.850 | 0.767 | 2.1% (20 runs) | instructions move the same way |
| lib/load_small | 167 | 0.880 | 0.997 | 2.8% (17 runs) | instructions equal: layout / IPC |
| corpus/classic_vector | 673 | 0.885 | 0.872 | 3.9% (20 runs) | instructions move the same way |
| lib/sel_loop | 218 | 0.909 | 0.998 | 4.8% (5 runs) | instructions equal: layout / IPC |
| jit-trace/rec_loop | 79 | 0.913 | 0.878 | 0.2% (2 runs) | instructions move the same way |
| corpus2/dev/ecs_world | 867 | 0.914 | 0.853 | 0.1% (2 runs) | instructions move the same way |
| apps/raytrace | 112 | 0.922 | 0.753 | 5.5% (20 runs) | instructions move the same way |
| corpus2/heldout/prototype_objects | 1386 | 0.924 | 0.739 | 10.3% (20 runs) | instructions move the same way |
| corpus2/heldout/scheme_interp | 1308 | 0.928 | 1.016 | 1.2% (5 runs) |  |
| corpus/class_middleclass | 498 | 0.938 | 0.969 | 23.6% (20 runs) | instructions move the same way |
| real/lua_interp | 106 | 0.939 | 0.906 | 3.4% (20 runs) | instructions move the same way |
| apps/lru_cache | 175 | 0.941 | 0.808 | 0.1% (2 runs) | instructions move the same way |
| apps/events | 154 | 0.951 | 0.894 | 0.7% (2 runs) | instructions move the same way |

### 5.5 plain vs 1a5aad0: regressions (instructions > +2% or cycles > +3%, >= 20 Mcyc)

| program | Mcyc | cyc ratio | ins ratio | cycle spread of readings | note |
|---|---|---|---|---|---|
| corpus2/heldout/dijkstra_routes | 1449 | 1.166 | 1.329 | 2.1% (20 runs) | instructions move the same way |
| lib/tonumber_int | 153 | 1.123 | 1.013 | 16.9% (20 runs) | instructions move the same way |
| programs/table_hash | 524 | 1.110 | 1.007 | 9.9% (20 runs) | instructions equal: layout / IPC |
| lib/tinsert_idx | 179 | 1.089 | 1.034 | 1.0% (2 runs) | instructions move the same way |
| lib/tconcat | 151 | 1.074 | 1.007 | 1.0% (2 runs) | instructions equal: layout / IPC |
| programs/string_ops | 1179 | 1.068 | 1.008 | 1.7% (5 runs) | instructions equal: layout / IPC |
| corpus2/dev/event_promise | 382 | 1.046 | 0.986 | 1.3% (5 runs) |  |
| lib/tostring_int | 121 | 1.045 | 1.029 | 8.8% (20 runs) | instructions move the same way |
| programs/sieve | 577 | 1.044 | 1.000 | 2.5% (17 runs) | instructions equal: layout / IPC |
| programs/pcall_error | 1520 | 1.042 | 1.000 | 3.4% (20 runs) | instructions equal: layout / IPC |
| lib/tsort_int | 269 | 1.039 | 1.015 | 0.2% (2 runs) | instructions move the same way |
| lib/pcall_err | 121 | 1.037 | 0.998 | 5.6% (20 runs) | instructions equal: layout / IPC |
| corpus2/heldout/real_dis_mips | 2234 | 1.037 | 1.031 | 1.0% (2 runs) | instructions move the same way |
| lib/tinsert_front | 225 | 1.016 | 1.027 | 6.6% (20 runs) |  |
| lib/fmt_d | 93 | 0.993 | 1.023 | 4.2% (20 runs) |  |
| lib/concat_op | 139 | 0.988 | 1.021 | 2.9% (17 runs) |  |

### 5.5 PGO vs 1a5aad0: regressions (instructions > +2% or cycles > +3%, >= 20 Mcyc)

| program | Mcyc | cyc ratio | ins ratio | cycle spread of readings | note |
|---|---|---|---|---|---|
| corpus2/heldout/dijkstra_routes | 1416 | 1.170 | 1.344 | 1.3% (20 runs) | instructions move the same way |
| programs/table_hash | 512 | 1.146 | 1.003 | 11.9% (20 runs) | instructions equal: layout / IPC |
| corpus2/dev/event_promise | 365 | 1.131 | 0.986 | 2.7% (5 runs) |  |
| lib/len_tbl | 142 | 1.051 | 1.000 | 0.2% (2 runs) | instructions equal: layout / IPC |
| corpus2/heldout/real_dis_mips | 1906 | 1.042 | 1.019 | 1.4% (2 runs) | instructions move the same way |
| programs/string_ops | 1093 | 1.038 | 0.997 | 2.0% (5 runs) | instructions equal: layout / IPC |
| lib/tsort_cmp | 261 | 1.038 | 1.009 | 1.1% (5 runs) | instructions equal: layout / IPC |
| lib/tonumber_bad | 119 | 1.037 | 1.000 | 6.1% (20 runs) | instructions equal: layout / IPC |
| programs/fib | 1291 | 1.035 | 0.948 | 2.4% (5 runs) |  |
| awfy/storage | 78 | 1.035 | 1.010 | 34.7% (20 runs) | instructions equal: layout / IPC |
| programs/sieve | 576 | 1.035 | 1.000 | 2.6% (17 runs) | instructions equal: layout / IPC |
| lib/tinsert_idx | 153 | 1.034 | 1.028 | 0.4% (2 runs) | instructions move the same way |
| corpus/mustache_lustache | 1486 | 1.033 | 0.991 | 1.2% (2 runs) | instructions equal: layout / IPC |
| lib/tsort_str | 155 | 1.033 | 1.007 | 0.2% (2 runs) | instructions equal: layout / IPC |

### 5.4 plain vs 1a5aad0: regressions (instructions > +2% or cycles > +3%, >= 20 Mcyc)

| program | Mcyc | cyc ratio | ins ratio | cycle spread of readings | note |
|---|---|---|---|---|---|
| lib/tonumber_bad | 165 | 1.204 | 1.010 | 1.7% (20 runs) | instructions move the same way |
| lib/m_min | 249 | 1.124 | 1.000 | 3.8% (20 runs) | instructions equal: layout / IPC |
| corpus2/dev/regex_nfa | 1339 | 1.106 | 1.131 | 6.6% (20 runs) | instructions move the same way |
| lib/tonumber_int | 160 | 1.097 | 1.012 | 1.4% (20 runs) | instructions move the same way |
| lib/tostring_int | 137 | 1.057 | 1.026 | 2.3% (20 runs) | instructions move the same way |
| lib/tonumber_flt | 204 | 1.049 | 1.005 | 0.9% (2 runs) | instructions equal: layout / IPC |
| lib/tconcat | 174 | 1.043 | 1.003 | 0.1% (2 runs) | instructions equal: layout / IPC |
| corpus2/dev/diff_levenshtein | 463 | 1.042 | 1.012 | 3.0% (17 runs) | instructions move the same way |
| lib/tmove_1k | 107 | 1.038 | 1.001 | 10.5% (17 runs) | instructions equal: layout / IPC |
| programs/string_ops | 1124 | 1.033 | 1.007 | 3.0% (5 runs) | instructions equal: layout / IPC |
| lib/m_sqrt | 281 | 1.032 | 1.000 | 1.0% (5 runs) | instructions equal: layout / IPC |
| corpus2/heldout/real_dis_mips | 2223 | 1.029 | 1.027 | 1.0% (2 runs) |  |
| lib/tinsert_idx | 203 | 1.025 | 1.032 | 1.9% (2 runs) |  |
| lib/tinsert_append | 193 | 1.014 | 1.031 | 3.5% (5 runs) |  |
| corpus2/heldout/bigint_rsa | 1373 | 1.011 | 1.026 | 6.4% (20 runs) |  |
| awfy/storage | 146 | 1.008 | 1.023 | 6.4% (20 runs) |  |
| lib/fmt_d | 100 | 0.992 | 1.021 | 7.5% (20 runs) |  |

### 5.4 PGO vs 1a5aad0: regressions (instructions > +2% or cycles > +3%, >= 20 Mcyc)

| program | Mcyc | cyc ratio | ins ratio | cycle spread of readings | note |
|---|---|---|---|---|---|
| corpus2/dev/regex_nfa | 1153 | 1.115 | 1.123 | 4.8% (20 runs) | instructions move the same way |
| lib/tonumber_num | 214 | 1.085 | 0.999 | 7.6% (20 runs) | instructions equal: layout / IPC |
| programs/string_ops | 1031 | 1.055 | 1.000 | 3.0% (5 runs) | instructions equal: layout / IPC |
| corpus2/dev/diff_levenshtein | 433 | 1.048 | 1.003 | 9.0% (17 runs) | instructions equal: layout / IPC |
| apps/expr_lang | 146 | 1.048 | 0.977 | 4.3% (20 runs) |  |
| corpus2/dev/wordfreq_wrap | 552 | 1.033 | 1.001 | 0.4% (2 runs) | instructions equal: layout / IPC |
| programs/pcall_error | 1245 | 1.032 | 0.999 | 5.1% (20 runs) | instructions equal: layout / IPC |
| lib/tinsert_idx | 191 | 1.032 | 1.024 | 0.8% (2 runs) | instructions move the same way |
| corpus2/dev/event_promise | 439 | 1.032 | 0.991 | 2.2% (5 runs) | instructions equal: layout / IPC |
| lib/tinsert_append | 182 | 1.028 | 1.023 | 3.1% (5 runs) |  |

### 5.5 plain: 15 biggest gaps to LuaJIT (ours/LJ cycles)

| program | Mcyc | cyc/LJ | ins/LJ |
|---|---|---|---|
| awfy/permute | 44 | 3.336 | 4.335 |
| apps/raytrace | 115 | 2.837 | 2.273 |
| corpus/lua_lexer | 1238 | 2.762 | 2.686 |
| programs/fib | 1257 | 2.677 | 3.123 |
| corpus/classic_vector | 644 | 2.674 | 2.448 |
| awfy/cd | 263 | 2.439 | 2.574 |
| lib/fmt_s | 7 | 2.263 | 2.771 |
| lib/co_yield | 225 | 2.211 | 1.882 |
| corpus2/dev/text_adventure | 2594 | 2.186 | 2.170 |
| awfy/towers | 50 | 2.127 | 1.822 |
| lib/byte_multi | 41 | 2.063 | 3.572 |
| awfy/json | 145 | 1.974 | 2.198 |
| programs/coroutines | 1716 | 1.968 | 1.789 |
| corpus2/heldout/stream_closures | 813 | 1.955 | 2.190 |
| lib/find_plain | 129 | 1.905 | 2.353 |

### 5.5 plain: 15 biggest wins over LuaJIT (>= 20 Mcyc)

| program | Mcyc | cyc/LJ | ins/LJ |
|---|---|---|---|
| lib/sel_loop | 214 | 0.041 | 0.067 |
| lib/error_tbl | 261 | 0.086 | 0.079 |
| jit-trace/int_rec2 | 684 | 0.136 | 1.551 |
| lib/pcall_err | 121 | 0.159 | 0.136 |
| programs/pcall_error | 1520 | 0.182 | 0.184 |
| lib/tfront_1k | 78 | 0.193 | 0.076 |
| lib/it_next | 45 | 0.248 | 0.704 |
| lib/tonumber_flt | 196 | 0.253 | 0.395 |
| corpus/sha2_base64 | 433 | 0.307 | 0.331 |
| lib/match | 63 | 0.325 | 0.316 |
| jit-trace/int_sum | 889 | 0.354 | 1.272 |
| apps/markdown | 118 | 0.357 | 0.268 |
| lib/find_pat | 272 | 0.385 | 0.451 |
| lib/gsub_str | 164 | 0.411 | 0.499 |
| lib/tostring_int | 121 | 0.433 | 0.800 |

### 5.5 PGO: 15 biggest gaps to LuaJIT (ours/LJ cycles)

| program | Mcyc | cyc/LJ | ins/LJ |
|---|---|---|---|
| awfy/permute | 43 | 3.289 | 4.320 |
| programs/fib | 1291 | 2.748 | 3.123 |
| apps/raytrace | 107 | 2.649 | 2.113 |
| corpus/lua_lexer | 1096 | 2.445 | 2.548 |
| corpus/classic_vector | 574 | 2.383 | 2.103 |
| awfy/cd | 244 | 2.268 | 2.391 |
| lib/fmt_s | 7 | 2.205 | 2.667 |
| lib/byte_multi | 41 | 2.040 | 3.563 |
| awfy/towers | 48 | 2.033 | 1.774 |
| corpus2/dev/text_adventure | 2147 | 1.809 | 1.918 |
| corpus2/heldout/stream_closures | 745 | 1.792 | 2.019 |
| lib/getmt | 16 | 1.790 | 2.416 |
| jit-trace/type_entry | 1515 | 1.785 | 2.396 |
| lib/co_yield | 179 | 1.759 | 1.629 |
| lib/find_plain | 117 | 1.728 | 2.124 |

### 5.5 PGO: 15 biggest wins over LuaJIT (>= 20 Mcyc)

| program | Mcyc | cyc/LJ | ins/LJ |
|---|---|---|---|
| lib/sel_loop | 214 | 0.041 | 0.067 |
| lib/error_tbl | 206 | 0.068 | 0.062 |
| lib/pcall_err | 98 | 0.128 | 0.103 |
| jit-trace/int_rec2 | 681 | 0.136 | 1.551 |
| programs/pcall_error | 1213 | 0.145 | 0.151 |
| lib/tfront_1k | 76 | 0.186 | 0.069 |
| lib/tonumber_flt | 173 | 0.223 | 0.351 |
| lib/it_next | 45 | 0.248 | 0.702 |
| lib/match | 52 | 0.268 | 0.279 |
| corpus/sha2_base64 | 410 | 0.290 | 0.317 |
| apps/markdown | 103 | 0.312 | 0.240 |
| lib/gsub_str | 128 | 0.321 | 0.394 |
| lib/find_pat | 237 | 0.336 | 0.408 |
| jit-trace/int_sum | 888 | 0.354 | 1.272 |
| lib/tpack | 114 | 0.371 | 0.386 |

## Focus programs (cycles in M; ratios of cycles)

| program | plain Mcyc | PGO Mcyc | LJ Mcyc | PUC Mcyc | plain/LJ | PGO/LJ | plain/PUC | PGO/PUC | plain vs h cyc | ins | PGO vs h cyc | ins | plain vs 1a5 cyc | PGO vs 1a5 cyc | PGO/LJ session-h |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| programs/nbody | 345.0 | 343.9 | 345.6 | 4246.2 | 0.998 | 0.995 | 0.081 | 0.081 | 0.990 | 0.984 | 0.988 | 0.984 | 1.000 | 0.997 | 1.007 |
| programs/fib | 1257.5 | 1290.7 | 469.7 | 4139.5 | 2.677 | 2.748 | 0.304 | 0.312 | 1.003 | 1.000 | 1.028 | 1.000 | 1.002 | 1.035 | 2.672 |
| programs/binary_trees | 1543.3 | 1326.8 | 2007.4 | 4389.1 | 0.769 | 0.661 | 0.352 | 0.302 | 1.023 | 1.000 | 1.022 | 1.000 | 0.801 | 0.838 | 0.647 |
| programs/queens | 1838.1 | 1840.0 | 1907.1 | 4368.7 | 0.964 | 0.965 | 0.421 | 0.421 | 0.998 | 1.000 | 0.999 | 1.000 | 0.991 | 0.995 | 0.966 |
| apps/raytrace | 115.1 | 107.5 | 40.6 | 262.7 | 2.837 | 2.649 | 0.438 | 0.409 | 0.827 | 0.837 | 0.825 | 0.833 | 0.810 | 0.929 | 3.209 |
| awfy/cd | 262.6 | 244.2 | 107.6 | 437.5 | 2.439 | 2.268 | 0.600 | 0.558 | 1.018 | 1.003 | 1.025 | 1.003 | 0.824 | 0.839 | 2.213 |
| awfy/list | 96.6 | 94.4 | 88.9 | 418.8 | 1.087 | 1.063 | 0.231 | 0.225 | 0.777 | 0.639 | 0.766 | 0.635 | 0.726 | 0.727 | 1.387 |
| awfy/richards | 59.2 | 56.9 | 34.7 | 197.7 | 1.707 | 1.640 | 0.300 | 0.288 | 0.995 | 0.991 | 0.994 | 0.990 | 0.973 | 0.969 | 1.650 |
| awfy/deltablue | 110.9 | 103.1 | 103.5 | 157.9 | 1.072 | 0.997 | 0.702 | 0.653 | 1.026 | 1.002 | 1.019 | 1.002 | 0.946 | 0.942 | 0.978 |
| awfy/havlak | 152.9 | 135.6 | 115.2 | 217.5 | 1.327 | 1.177 | 0.703 | 0.623 | 1.001 | 0.997 | 1.019 | 0.996 | 0.991 | 1.003 | 1.155 |
| awfy/permute | 43.7 | 43.1 | 13.1 | 246.3 | 3.336 | 3.289 | 0.178 | 0.175 | 0.990 | 0.976 | 0.988 | 0.976 | 1.007 | 1.009 | 3.331 |
| programs/fannkuch | 1059.6 | 1048.0 | 1075.7 | 4075.1 | 0.985 | 0.974 | 0.260 | 0.257 | 0.999 | 0.976 | 0.983 | 0.976 | 1.005 | 0.986 | 0.991 |
| awfy/towers | 49.9 | 47.7 | 23.5 | 215.1 | 2.127 | 2.033 | 0.232 | 0.222 | 0.979 | 0.991 | 0.972 | 0.991 | 1.003 | 0.990 | 2.092 |

## Per-program table (5.5 and 5.4; Mcyc, ratios vs session-h and LJ)

| program | n runs (plain) | plain | PGO | p54 | g54 | LJ | PUC55 | PUC54 | plain/LJ | PGO/LJ | plain vs h cyc | ins | PGO vs h cyc | ins | p54 vs h cyc | ins | g54 vs h cyc | ins | out/exp (all 4) |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| apps/events | 2 | 162.0 | 140.1 | 177.5 | 153.6 | 95.4 | 293.7 | 289.5 | 1.698 | 1.468 | 1.020 | 0.996 | 0.997 | 0.997 | 1.012 | 0.997 | 1.012 | 0.998 | ok |
| apps/expr_lang | 20 | 140.0 | 127.9 | 160.6 | 146.3 | 96.7 | 241.7 | 253.2 | 1.448 | 1.323 | 1.005 | 0.993 | 1.000 | 0.994 | 0.673 | 0.798 | 0.684 | 0.809 | ok |
| apps/lru_cache | 2 | 145.6 | 136.3 | 184.8 | 175.2 | 106.4 | 219.8 | 253.9 | 1.368 | 1.281 | 0.993 | 1.000 | 1.006 | 1.001 | 0.996 | 1.000 | 1.009 | 0.998 | ok |
| apps/markdown | 17 | 117.8 | 102.7 | 119.8 | 104.5 | 329.7 | 412.6 | 390.8 | 0.357 | 0.312 | 1.011 | 1.000 | 1.016 | 1.000 | 0.970 | 1.000 | 0.992 | 1.000 | ok |
| apps/raytrace | 20 | 115.1 | 107.5 | 120.5 | 112.0 | 40.6 | 262.7 | 281.7 | 2.837 | 2.649 | 0.827 | 0.837 | 0.825 | 0.833 | 0.757 | 0.692 | 0.758 | 0.682 | ok |
| awfy/bounce | 2 | 62.0 | 60.3 | 68.2 | 66.0 | 47.7 | 511.2 | 537.1 | 1.300 | 1.266 | 1.015 | 0.998 | 1.014 | 0.998 | 1.010 | 0.999 | 1.009 | 0.997 | ok |
| awfy/cd | 17 | 262.6 | 244.2 | 290.6 | 270.0 | 107.6 | 437.5 | 464.4 | 2.439 | 2.268 | 1.018 | 1.003 | 1.025 | 1.003 | 1.008 | 0.997 | 1.021 | 0.995 | ok |
| awfy/deltablue | 17 | 110.9 | 103.1 | 121.2 | 113.7 | 103.5 | 157.9 | 165.2 | 1.072 | 0.997 | 1.026 | 1.002 | 1.019 | 1.002 | 1.012 | 0.999 | 1.029 | 0.998 | ok |
| awfy/havlak | 17 | 152.9 | 135.6 | 175.7 | 158.4 | 115.2 | 217.5 | 225.5 | 1.327 | 1.177 | 1.001 | 0.997 | 1.019 | 0.996 | 0.990 | 1.000 | 1.004 | 0.996 | ok |
| awfy/json | 5 | 144.9 | 122.6 | 155.0 | 133.3 | 73.4 | 262.9 | 271.8 | 1.974 | 1.670 | 1.011 | 0.985 | 1.022 | 0.984 | 0.986 | 0.979 | 0.999 | 0.977 | ok |
| awfy/list | 20 | 96.6 | 94.4 | 98.3 | 96.5 | 88.9 | 418.8 | 477.9 | 1.087 | 1.063 | 0.777 | 0.639 | 0.766 | 0.635 | 0.755 | 0.613 | 0.749 | 0.608 | ok |
| awfy/mandelbrot | 2 | 273.6 | 273.1 | 273.2 | 272.8 | 273.1 | 1529.3 | 1379.3 | 1.002 | 1.000 | 1.003 | 1.000 | 1.002 | 1.000 | 1.001 | 1.000 | 1.002 | 1.000 | ok |
| awfy/nbody | 2 | 92.9 | 91.9 | 97.4 | 95.7 | 94.6 | 1504.3 | 1603.8 | 0.982 | 0.971 | 1.005 | 1.000 | 1.002 | 1.000 | 1.016 | 1.000 | 1.006 | 1.000 | ok |
| awfy/permute | 5 | 43.7 | 43.1 | 43.9 | 43.5 | 13.1 | 246.3 | 257.2 | 3.336 | 3.289 | 0.990 | 0.976 | 0.988 | 0.976 | 0.975 | 0.977 | 0.972 | 0.976 | ok |
| awfy/queens | 2 | 97.0 | 94.1 | 81.6 | 79.4 | 55.0 | 401.8 | 403.4 | 1.763 | 1.711 | 1.002 | 1.000 | 0.994 | 1.000 | 0.996 | 0.999 | 0.998 | 0.999 | ok |
| awfy/richards | 20 | 59.2 | 56.9 | 59.5 | 57.6 | 34.7 | 197.7 | 199.3 | 1.707 | 1.640 | 0.995 | 0.991 | 0.994 | 0.990 | 0.992 | 0.991 | 0.999 | 0.990 | ok |
| awfy/sieve | 2 | 124.1 | 124.5 | 127.9 | 126.6 | 156.4 | 1032.3 | 1085.2 | 0.793 | 0.796 | 1.015 | 1.000 | 1.018 | 1.000 | 1.004 | 1.000 | 1.001 | 1.000 | ok |
| awfy/storage | 20 | 100.9 | 78.2 | 145.8 | 114.0 | 154.0 | 255.9 | 262.0 | 0.655 | 0.508 | 0.993 | 0.999 | 1.001 | 0.998 | 1.001 | 0.999 | 0.998 | 0.997 | ok |
| awfy/towers | 5 | 49.9 | 47.7 | 50.8 | 48.7 | 23.5 | 215.1 | 213.9 | 2.127 | 2.033 | 0.979 | 0.991 | 0.972 | 0.991 | 0.977 | 0.991 | 0.977 | 0.990 | ok |
| corpus/astar_jumper | 20 | 1680.2 | 1605.1 | 1876.6 | 1793.0 | 997.4 | 3280.6 | 3358.7 | 1.685 | 1.609 | 1.042 | 1.073 | 1.055 | 1.078 | 1.141 | 1.149 | 1.126 | 1.157 | ok |
| corpus/class_middleclass | 20 | 479.7 | 471.6 | 506.7 | 498.4 | 319.5 | 3046.3 | 3098.9 | 1.501 | 1.476 | 0.966 | 0.983 | 0.969 | 0.983 | 0.965 | 0.984 | 0.984 | 0.984 | ok |
| corpus/classic_vector | 20 | 643.6 | 573.6 | 807.1 | 673.2 | 240.7 | 2945.5 | 2905.4 | 2.674 | 2.383 | 1.023 | 0.999 | 1.015 | 0.998 | 0.989 | 0.991 | 1.004 | 0.986 | ok |
| corpus/containers_lru_heap | 2 | 1309.3 | 1229.7 | 1665.6 | 1549.7 | 1417.3 | 2675.7 | 3035.3 | 0.924 | 0.868 | 1.029 | 1.001 | 1.018 | 1.001 | 0.996 | 1.000 | 1.008 | 1.000 | ok |
| corpus/coro_scheduler | 2 | 1521.8 | 1318.5 | 1634.1 | 1420.9 | 941.9 | 2772.8 | 2851.7 | 1.616 | 1.400 | 1.010 | 1.000 | 1.006 | 1.002 | 0.995 | 1.000 | 1.006 | 1.001 | ok |
| corpus/csv_ini_yaml | 17 | 1579.3 | 1370.8 | 1864.0 | 1638.1 | 1771.4 | 2948.4 | 3020.4 | 0.892 | 0.774 | 1.013 | 1.001 | 1.020 | 1.001 | 0.979 | 1.000 | 0.977 | 1.000 | ok |
| corpus/inspect_lua | 2 | 1588.4 | 1406.2 | 1712.0 | 1536.9 | 1056.4 | 2901.3 | 2968.0 | 1.504 | 1.331 | 1.006 | 1.000 | 1.004 | 1.000 | 1.003 | 1.000 | 1.002 | 0.999 | ok |
| corpus/json_rxi | 2 | 1847.4 | 1524.2 | 1917.9 | 1604.9 | 1222.9 | 3647.9 | 3646.8 | 1.511 | 1.246 | 1.008 | 1.000 | 1.009 | 1.000 | 1.013 | 1.000 | 1.009 | 1.000 | ok |
| corpus/lua_lexer | 20 | 1238.0 | 1095.9 | 1118.1 | 981.3 | 448.2 | 3281.3 | 3358.7 | 2.762 | 2.445 | 0.980 | 1.000 | 0.980 | 1.000 | 0.991 | 1.000 | 0.986 | 0.999 | ok |
| corpus/markdown_conv | 5 | 1429.0 | 1271.1 | 1430.9 | 1284.5 | 3029.8 | 3746.9 | 3671.3 | 0.472 | 0.420 | 1.017 | 0.999 | 1.008 | 0.999 | 0.999 | 0.999 | 1.012 | 0.999 | ok |
| corpus/msgpack_pure | 20 | 2016.6 | 1780.7 | 2175.8 | 1888.3 | 4473.7 | 3436.5 | 3601.3 | 0.451 | 0.398 | 0.994 | 0.978 | 1.006 | 0.980 | 0.988 | 0.999 | 0.973 | 0.998 | ok |
| corpus/mustache_lustache | 2 | 1815.1 | 1485.8 | 1954.3 | 1561.2 | 1537.1 | 3296.2 | 3446.8 | 1.181 | 0.967 | 1.007 | 1.000 | 1.020 | 1.000 | 1.019 | 1.000 | 1.019 | 1.000 | ok |
| corpus/parser_combinator | 20 | 2292.8 | 1856.9 | 2491.6 | 2071.5 | 1563.1 | 3390.6 | 3391.8 | 1.467 | 1.188 | 0.951 | 0.983 | 0.975 | 0.981 | 1.026 | 1.000 | 1.048 | 1.000 | ok |
| corpus/penlight_mix | 2 | 2298.6 | 2158.5 | 2397.2 | 2279.0 | 2678.7 | 3992.1 | 3970.3 | 0.858 | 0.806 | 0.996 | 1.000 | 1.000 | 1.000 | 0.989 | 1.000 | 0.994 | 1.000 | ok |
| corpus/serpent_ser | 5 | 2062.4 | 1762.5 | 2111.0 | 1824.1 | 2426.7 | 3260.6 | 3254.8 | 0.850 | 0.726 | 1.011 | 1.000 | 0.998 | 1.000 | 0.990 | 1.002 | 0.991 | 0.999 | ok |
| corpus/sha2_base64 | 2 | 432.7 | 409.9 | 390.6 | 373.7 | 1411.3 | 3328.6 | 3384.1 | 0.307 | 0.290 | 1.016 | 1.000 | 1.010 | 1.001 | 0.997 | 1.000 | 1.006 | 1.000 | ok |
| corpus/template_etlua | 2 | 1615.1 | 1271.4 | 1593.9 | 1287.5 | 1826.0 | 2680.1 | 2700.4 | 0.885 | 0.696 | 1.027 | 1.000 | 0.998 | 0.999 | 1.021 | 1.000 | 1.011 | 1.000 | ok |
| corpus/testrunner_luaunit | 5 | 1481.2 | 1235.4 | 1632.5 | 1368.8 | 1390.7 | 2601.9 | 2628.8 | 1.065 | 0.888 | 0.969 | 0.976 | 0.987 | 0.975 | 0.985 | 0.994 | 1.024 | 0.992 | ok |
| corpus2/dev/bignum_arith | 5 | 796.0 | 731.7 | 870.4 | 813.1 | 660.2 | 2352.8 | 2412.4 | 1.206 | 1.108 | 1.008 | 1.001 | 1.008 | 1.001 | 0.990 | 1.000 | 0.990 | 1.000 | ok |
| corpus2/dev/coro_generators | 2 | 1258.7 | 970.1 | 1284.6 | 986.5 | 713.4 | 1807.9 | 1836.1 | 1.764 | 1.360 | 1.004 | 0.996 | 1.010 | 0.998 | 1.002 | 0.997 | 1.000 | 0.998 | ok |
| corpus2/dev/csv_report | 20 | 1149.5 | 1024.9 | 1187.3 | 1073.8 | 1262.6 | 2165.3 | 2381.4 | 0.910 | 0.812 | 1.005 | 1.000 | 0.986 | 0.999 | 0.989 | 1.004 | 1.004 | 1.003 | ok |
| corpus2/dev/diff_levenshtein | 17 | 429.0 | 407.0 | 463.0 | 433.1 | 329.3 | 1776.3 | 1850.8 | 1.303 | 1.236 | 0.981 | 1.000 | 1.002 | 0.999 | 0.897 | 1.000 | 1.072 | 1.000 | ok |
| corpus2/dev/ecs_world | 2 | 941.6 | 852.0 | 1006.5 | 867.3 | 916.1 | 2115.3 | 2160.1 | 1.028 | 0.930 | 1.002 | 0.998 | 1.020 | 0.998 | 1.017 | 0.999 | 1.012 | 0.999 | ok |
| corpus2/dev/event_promise | 5 | 382.2 | 365.3 | 458.2 | 439.4 | 467.9 | 1906.6 | 1939.2 | 0.817 | 0.781 | 1.019 | 1.000 | 1.012 | 1.000 | 1.006 | 1.000 | 1.009 | 1.000 | ok |
| corpus2/dev/expr_compiler | 5 | 1300.7 | 1115.2 | 1355.8 | 1173.8 | 1080.2 | 2081.3 | 2231.0 | 1.204 | 1.032 | 1.008 | 1.000 | 1.014 | 1.000 | 1.005 | 1.007 | 1.019 | 1.009 | ok |
| corpus2/dev/iter_pipeline | 20 | 173.3 | 164.5 | 183.7 | 174.4 | 132.2 | 2865.2 | 2874.5 | 1.311 | 1.244 | 0.998 | 1.000 | 0.993 | 1.000 | 1.002 | 0.999 | 0.990 | 0.999 | ok |
| corpus2/dev/lua_parser | 20 | 1274.5 | 1104.0 | 1531.0 | 1326.7 | 790.7 | 2461.5 | 2499.0 | 1.612 | 1.396 | 0.983 | 1.009 | 0.975 | 1.008 | 0.957 | 0.987 | 0.969 | 0.985 | ok |
| corpus2/dev/memo_cache | 5 | 1662.5 | 1503.0 | 1677.0 | 1520.7 | 1674.7 | 2669.5 | 2763.8 | 0.993 | 0.897 | 0.991 | 0.979 | 0.962 | 0.978 | 0.986 | 0.980 | 0.974 | 0.978 | ok |
| corpus2/dev/oop_mixins | 20 | 1425.7 | 1350.3 | 1410.1 | 1340.2 | 865.1 | 2464.3 | 2520.8 | 1.648 | 1.561 | 0.969 | 0.952 | 0.969 | 0.954 | 0.951 | 0.961 | 0.985 | 0.964 | ok |
| corpus2/dev/particles_physics | 5 | 400.6 | 375.4 | 515.6 | 478.1 | 379.7 | 1719.5 | 1811.4 | 1.055 | 0.989 | 0.948 | 0.959 | 0.954 | 0.958 | 0.978 | 0.992 | 0.988 | 0.988 | ok |
| corpus2/dev/real_dis_arm | 5 | 2464.9 | 2165.2 | 2418.7 | 2140.4 | 1947.2 | 4487.3 | 4484.7 | 1.266 | 1.112 | 0.993 | 0.998 | 0.992 | 0.998 | 0.938 | 0.951 | 0.957 | 0.956 | ok |
| corpus2/dev/real_dis_x86 | 5 | 1552.2 | 1368.5 | 1586.0 | 1414.3 | 1591.6 | 2708.6 | 2688.6 | 0.975 | 0.860 | 1.012 | 1.000 | 1.005 | 1.000 | 0.969 | 1.000 | 0.994 | 1.000 | ok |
| corpus2/dev/regex_nfa | 20 | 649.7 | 565.1 | 1339.0 | 1153.2 | 554.5 | 2510.1 | 2439.3 | 1.172 | 1.019 | 0.939 | 1.017 | 1.025 | 1.018 | 1.115 | 1.130 | 1.112 | 1.127 | ok |
| corpus2/dev/template_gsub | 2 | 1688.6 | 1404.6 | 1759.8 | 1491.1 | 2051.0 | 2892.7 | 2885.0 | 0.823 | 0.685 | 0.999 | 1.000 | 1.010 | 1.000 | 0.964 | 1.000 | 1.008 | 1.000 | ok |
| corpus2/dev/text_adventure | 2 | 2593.7 | 2146.9 | 2752.0 | 2183.9 | 1186.6 | 3375.1 | 3513.0 | 2.186 | 1.809 | 1.008 | 1.000 | 1.017 | 0.999 | 1.001 | 1.000 | 0.996 | 1.000 | ok |
| corpus2/dev/varargs_util | 2 | 1511.9 | 1240.8 | 1557.0 | 1261.3 | 1413.9 | 2284.8 | 2301.9 | 1.069 | 0.878 | 0.994 | 1.000 | 1.017 | 1.000 | 1.004 | 1.000 | 0.989 | 1.000 | ok |
| corpus2/dev/wordfreq_wrap | 2 | 619.2 | 516.4 | 664.7 | 552.3 | 658.1 | 1219.2 | 1225.4 | 0.941 | 0.785 | 1.005 | 1.000 | 1.007 | 1.000 | 0.989 | 1.000 | 1.016 | 1.000 | ok |
| corpus2/heldout/bigint_rsa | 20 | 1129.6 | 935.5 | 1372.5 | 1188.5 | 1074.5 | 3033.1 | 3149.9 | 1.051 | 0.871 | 1.010 | 0.999 | 0.992 | 0.999 | 0.992 | 1.007 | 1.005 | 1.007 | ok |
| corpus2/heldout/dijkstra_routes | 20 | 1448.6 | 1415.7 | 1525.3 | 1487.2 | 1265.1 | 3142.9 | 3226.1 | 1.145 | 1.119 | 1.138 | 1.291 | 1.140 | 1.296 | 0.982 | 0.955 | 0.986 | 0.956 | ok |
| corpus2/heldout/forth_vm | 20 | 810.6 | 806.6 | 1058.8 | 1156.6 | 696.1 | 2965.2 | 3269.3 | 1.165 | 1.159 | 0.997 | 1.000 | 0.998 | 1.000 | 1.004 | 1.000 | 0.995 | 0.998 | ok |
| corpus2/heldout/log_analyzer | 20 | 2152.3 | 1762.9 | 2149.0 | 1894.7 | 1652.2 | 3115.0 | 3178.7 | 1.303 | 1.067 | 1.019 | 1.000 | 1.003 | 1.000 | 0.984 | 1.000 | 1.026 | 1.000 | ok |
| corpus2/heldout/markov_text | 2 | 1141.8 | 1136.8 | 1206.7 | 1160.4 | 982.0 | 2862.9 | 2851.2 | 1.163 | 1.158 | 0.996 | 1.000 | 1.022 | 1.000 | 1.004 | 1.000 | 1.002 | 1.000 | ok |
| corpus2/heldout/prototype_objects | 20 | 1455.2 | 1403.3 | 1448.7 | 1385.7 | 1280.6 | 2532.8 | 2721.4 | 1.136 | 1.096 | 0.963 | 0.915 | 0.978 | 0.914 | 0.951 | 0.847 | 0.947 | 0.860 | ok |
| corpus2/heldout/real_dis_mips | 2 | 2233.6 | 1906.1 | 2222.9 | 1900.6 | 1786.6 | 3808.7 | 3869.0 | 1.250 | 1.067 | 1.021 | 1.000 | 1.017 | 1.000 | 0.984 | 0.999 | 1.004 | 0.999 | ok |
| corpus2/heldout/real_dis_ppc | 2 | 2149.6 | 1848.2 | 2252.8 | 1921.7 | 1747.6 | 3961.9 | 3962.8 | 1.230 | 1.058 | 1.014 | 1.000 | 1.009 | 1.001 | 0.984 | 1.000 | 0.999 | 1.000 | ok |
| corpus2/heldout/roguelike_turns | 2 | 832.7 | 790.9 | 872.4 | 838.6 | 623.3 | 3311.7 | 3473.6 | 1.336 | 1.269 | 1.027 | 1.001 | 1.011 | 1.001 | 1.023 | 1.003 | 1.018 | 1.002 | ok |
| corpus2/heldout/scheme_interp | 5 | 1445.5 | 1221.1 | 1548.8 | 1307.6 | 1045.1 | 3052.1 | 3021.0 | 1.383 | 1.168 | 0.988 | 0.902 | 0.981 | 0.901 | 0.961 | 1.067 | 0.965 | 1.063 | ok |
| corpus2/heldout/stream_closures | 20 | 813.4 | 745.3 | 871.1 | 809.9 | 416.0 | 3032.0 | 3163.1 | 1.955 | 1.792 | 0.870 | 0.849 | 0.869 | 0.851 | 0.844 | 0.796 | 0.860 | 0.792 | ok |
| corpus2/heldout/sudoku_solver | 2 | 1308.4 | 1175.7 | 1641.9 | 1509.8 | 1387.6 | 3013.0 | 3102.7 | 0.943 | 0.847 | 0.993 | 0.998 | 1.001 | 0.998 | 0.998 | 1.001 | 1.005 | 1.001 | ok |
| corpus2/heldout/vending_fsm | 2 | 1021.8 | 936.5 | 1103.8 | 1031.0 | 824.2 | 3290.3 | 3377.2 | 1.240 | 1.136 | 0.987 | 0.999 | 0.989 | 0.999 | 0.985 | 0.998 | 1.010 | 0.998 | ok |
| jit-trace/alloc_ctor | 5 | 190.2 | 160.6 | 257.5 | 203.3 | 276.5 | 1652.1 | 2197.8 | 0.688 | 0.581 | 1.018 | 1.000 | 1.015 | 1.000 | 1.014 | 1.000 | 1.000 | 0.994 | ok |
| jit-trace/array_rec | 2 | 1797.0 | 1796.1 | 1822.3 | 1821.4 | 1755.9 | 9671.5 | 10022.8 | 1.023 | 1.023 | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | ok |
| jit-trace/dot | 2 | 1225.8 | 1225.3 | 1231.2 | 1233.3 | 1166.0 | 11124.9 | 11807.8 | 1.051 | 1.051 | 1.002 | 1.000 | 1.001 | 1.000 | 1.001 | 1.000 | 1.004 | 1.000 | ok |
| jit-trace/field_global | 2 | 546.2 | 540.9 | 546.4 | 542.2 | 491.7 | 17948.4 | 17879.3 | 1.111 | 1.100 | 1.001 | 1.000 | 0.999 | 1.000 | 0.999 | 1.000 | 0.999 | 1.000 | ok |
| jit-trace/field_method | 2 | 571.4 | 565.4 | 571.6 | 567.3 | 423.1 | 20185.1 | 20271.4 | 1.351 | 1.336 | 1.002 | 1.000 | 1.001 | 1.001 | 0.999 | 1.000 | 1.002 | 1.000 | ok |
| jit-trace/field_rw | 2 | 950.4 | 949.1 | 1036.8 | 1034.7 | 929.3 | 19574.6 | 19882.6 | 1.023 | 1.021 | 1.001 | 1.000 | 1.001 | 1.000 | 1.000 | 1.000 | 0.999 | 1.001 | ok |
| jit-trace/float_loop | 2 | 2781.5 | 2777.9 | 2793.6 | 2795.8 | 2575.9 | 11381.5 | 13567.0 | 1.080 | 1.078 | 1.001 | 1.000 | 1.001 | 1.000 | 0.999 | 1.000 | 0.999 | 1.000 | ok |
| jit-trace/float_poly | 2 | 1787.0 | 1784.4 | 1819.6 | 1820.8 | 1768.0 | 7918.6 | 8940.5 | 1.011 | 1.009 | 1.002 | 1.000 | 1.000 | 1.000 | 0.999 | 1.000 | 1.001 | 1.000 | ok |
| jit-trace/inline_2 | 2 | 333.0 | 332.4 | 333.2 | 332.4 | 262.5 | 11057.3 | 10600.9 | 1.269 | 1.267 | 1.026 | 1.000 | 1.028 | 1.000 | 1.028 | 1.000 | 1.026 | 1.000 | ok |
| jit-trace/inline_leaf | 2 | 457.1 | 454.3 | 454.9 | 457.1 | 630.2 | 11902.9 | 12019.3 | 0.725 | 0.721 | 1.000 | 1.000 | 0.996 | 1.000 | 0.994 | 1.000 | 1.002 | 1.000 | ok |
| jit-trace/int_rec2 | 5 | 683.6 | 680.7 | 683.6 | 691.2 | 5016.4 | 12493.6 | 13006.3 | 0.136 | 0.136 | 1.004 | 1.000 | 1.004 | 1.000 | 0.994 | 1.000 | 0.997 | 1.000 | ok |
| jit-trace/int_sum | 2 | 889.3 | 888.5 | 910.7 | 911.1 | 2512.8 | 9858.5 | 12300.9 | 0.354 | 0.354 | 1.000 | 1.000 | 0.997 | 1.000 | 1.002 | 1.000 | 1.001 | 1.000 | ok |
| jit-trace/licm_expr | 2 | 466.6 | 467.2 | 466.6 | 466.2 | 442.6 | 12323.9 | 12827.3 | 1.054 | 1.056 | 0.998 | 1.000 | 1.001 | 1.000 | 0.998 | 1.000 | 0.998 | 1.000 | ok |
| jit-trace/multret | 2 | 150.7 | 150.0 | 150.5 | 150.2 | 156.7 | 5879.7 | 5782.8 | 0.962 | 0.957 | 1.000 | 1.000 | 0.999 | 1.000 | 0.997 | 1.000 | 1.001 | 1.000 | ok |
| jit-trace/oop_inherit | 2 | 57.7 | 57.3 | 49.1 | 48.7 | 112.8 | 2153.1 | 2201.0 | 0.512 | 0.508 | 1.002 | 1.000 | 1.001 | 1.000 | 0.997 | 1.000 | 0.998 | 1.000 | ok |
| jit-trace/oop_override | 20 | 106.8 | 106.5 | 110.6 | 110.0 | 136.7 | 645.6 | 666.3 | 0.782 | 0.779 | 0.989 | 1.000 | 0.986 | 1.000 | 0.987 | 1.000 | 0.990 | 1.000 | ok |
| jit-trace/rec_loop | 2 | 82.0 | 81.1 | 80.4 | 79.3 | 141.9 | 1440.3 | 1449.9 | 0.578 | 0.571 | 1.006 | 1.000 | 1.007 | 1.000 | 1.013 | 1.000 | 1.014 | 1.000 | ok |
| jit-trace/short_inner | 2 | 1101.2 | 1101.4 | 1000.2 | 998.5 | 878.8 | 10393.4 | 11908.6 | 1.253 | 1.253 | 1.005 | 1.000 | 1.003 | 1.000 | 1.005 | 1.000 | 1.004 | 1.000 | ok |
| jit-trace/sink_tuple | 5 | 13.3 | 13.0 | 13.3 | 13.0 | 20.0 | 2743.7 | 3358.1 | 0.665 | 0.650 | 0.993 | 1.000 | 1.000 | 1.000 | 0.995 | 1.000 | 1.000 | 1.000 | ok |
| jit-trace/sink_vec | 2 | 15.3 | 15.3 | 15.4 | 15.1 | 20.0 | 1489.0 | 1832.3 | 0.767 | 0.767 | 1.001 | 1.000 | 1.012 | 1.000 | 0.993 | 1.000 | 0.999 | 1.000 | ok |
| jit-trace/str_concat | 2 | 537.9 | 512.2 | 597.6 | 554.4 | 501.6 | 1628.5 | 1630.2 | 1.072 | 1.021 | 1.010 | 1.000 | 1.009 | 1.000 | 0.993 | 1.000 | 0.997 | 1.001 | ok |
| jit-trace/tail_loop | 2 | 248.2 | 248.6 | 252.9 | 251.3 | 223.3 | 6724.0 | 6643.8 | 1.111 | 1.113 | 1.003 | 1.000 | 1.005 | 1.000 | 1.006 | 1.000 | 1.001 | 1.000 | ok |
| jit-trace/tfor_loops | 5 | 220.2 | 219.2 | 227.6 | 224.1 | 362.3 | 3623.0 | 3352.6 | 0.608 | 0.605 | 1.005 | 1.000 | 1.003 | 1.000 | 1.001 | 1.000 | 0.987 | 1.000 | ok |
| jit-trace/type_entry | 2 | 1511.8 | 1514.8 | 1577.9 | 1584.7 | 848.7 | 11725.8 | 12234.3 | 1.781 | 1.785 | 0.999 | 1.000 | 1.000 | 1.000 | 0.998 | 1.000 | 1.004 | 1.000 | ok |
| jit-trace/type_entry_stable | 2 | 1462.2 | 1465.7 | 1530.3 | 1533.4 | 856.9 | 11690.0 | 12248.4 | 1.706 | 1.710 | 1.006 | 1.000 | 1.005 | 1.000 | 1.001 | 1.000 | 1.003 | 1.000 | ok |
| jit-trace/type_late | 2 | 1528.0 | 1528.3 | 1539.9 | 1539.9 | 1461.5 | 10901.2 | 11418.6 | 1.046 | 1.046 | 1.000 | 1.000 | 1.000 | 1.000 | 0.999 | 1.000 | 0.999 | 1.000 | ok |
| jit-trace/type_late_stable | 2 | 1530.1 | 1531.2 | 1532.5 | 1532.0 | 1461.0 | 10905.6 | 11410.8 | 1.047 | 1.048 | 1.000 | 1.000 | 1.000 | 1.000 | 0.995 | 1.000 | 0.995 | 1.000 | ok |
| jit-trace/upval_acc | 2 | 1392.0 | 1392.9 | 1398.4 | 1398.4 | 1356.5 | 9970.0 | 10500.8 | 1.026 | 1.027 | 0.999 | 1.000 | 1.000 | 1.000 | 1.002 | 1.000 | 1.001 | 1.000 | ok |
| jit-trace/upval_inv | 2 | 2135.0 | 2144.2 | 2153.6 | 2139.8 | 1985.3 | 13886.5 | 14901.3 | 1.075 | 1.080 | 0.996 | 1.000 | 0.998 | 1.000 | 0.999 | 1.000 | 0.996 | 1.000 | ok |
| lib/assert_ok | 2 | 19.2 | 18.7 | 20.4 | 20.1 | 23.7 | 295.9 | 329.8 | 0.813 | 0.789 | 1.012 | 1.000 | 1.000 | 1.000 | 1.001 | 1.000 | 1.001 | 1.000 | ok |
| lib/byte | 2 | 50.3 | 50.0 | 51.2 | 51.1 | 33.0 | 776.0 | 845.1 | 1.522 | 1.516 | 0.986 | 1.000 | 0.991 | 1.000 | 0.988 | 1.000 | 0.992 | 1.000 | ok |
| lib/byte_multi | 5 | 41.5 | 41.0 | 41.7 | 41.1 | 20.1 | 336.9 | 366.2 | 2.063 | 2.040 | 1.017 | 1.000 | 1.018 | 1.000 | 1.019 | 1.000 | 1.008 | 1.000 | ok |
| lib/char | 5 | 154.2 | 139.6 | 156.3 | 145.2 | 104.8 | 383.6 | 387.1 | 1.471 | 1.332 | 1.004 | 1.000 | 0.999 | 1.000 | 0.993 | 1.000 | 1.001 | 1.000 | ok |
| lib/co_wrap | 20 | 249.3 | 202.9 | 239.9 | 202.7 | 228.7 | 381.5 | 338.9 | 1.090 | 0.887 | 1.007 | 0.999 | 1.005 | 1.001 | 1.024 | 1.000 | 1.018 | 0.988 | ok |
| lib/co_yield | 20 | 225.5 | 179.4 | 225.3 | 180.0 | 102.0 | 483.9 | 475.0 | 2.211 | 1.759 | 0.939 | 1.000 | 0.988 | 1.002 | 0.927 | 1.000 | 1.001 | 1.000 | ok |
| lib/concat_op | 17 | 139.3 | 128.3 | 142.8 | 134.1 | 212.9 | 419.4 | 445.3 | 0.654 | 0.603 | 0.996 | 1.000 | 1.004 | 1.000 | 0.989 | 1.000 | 1.017 | 1.002 | ok |
| lib/concat_op_str | 20 | 134.8 | 118.0 | 239.1 | 226.8 | 276.8 | 436.1 | 336.3 | 0.487 | 0.426 | 0.961 | 1.000 | 0.980 | 1.000 | 0.990 | 1.000 | 0.988 | 0.990 | ok |
| lib/error_tbl | 17 | 261.5 | 206.1 | 261.0 | 217.9 | 3042.9 | 619.9 | 625.1 | 0.086 | 0.068 | 0.970 | 1.000 | 0.948 | 1.001 | 0.997 | 1.000 | 1.020 | 1.000 | ok |
| lib/find_cap | 17 | 272.8 | 236.1 | 276.7 | 226.1 | 183.4 | 365.9 | 363.4 | 1.487 | 1.287 | 0.996 | 1.000 | 1.010 | 1.000 | 1.024 | 1.000 | 0.995 | 1.000 | ok |
| lib/find_pat | 20 | 271.6 | 236.7 | 272.0 | 234.2 | 705.2 | 1094.0 | 1092.2 | 0.385 | 0.336 | 1.050 | 1.000 | 1.010 | 1.000 | 0.973 | 1.000 | 0.992 | 1.000 | ok |
| lib/find_plain | 2 | 128.7 | 116.7 | 127.9 | 116.1 | 67.6 | 342.8 | 363.4 | 1.905 | 1.728 | 1.008 | 1.000 | 1.008 | 1.000 | 1.010 | 1.000 | 1.012 | 1.000 | ok |
| lib/fmt_d | 20 | 92.9 | 88.3 | 100.2 | 94.7 | 126.3 | 278.1 | 235.1 | 0.735 | 0.699 | 0.993 | 1.000 | 0.992 | 1.000 | 0.996 | 1.000 | 1.005 | 1.002 | ok |
| lib/fmt_f | 20 | 62.3 | 57.4 | 66.0 | 59.0 | 86.8 | 215.0 | 196.2 | 0.718 | 0.662 | 0.994 | 1.000 | 1.011 | 1.001 | 1.012 | 1.000 | 1.009 | 1.001 | ok |
| lib/fmt_g | 20 | 72.6 | 62.2 | 75.5 | 63.8 | 67.3 | 204.6 | 179.3 | 1.078 | 0.924 | 0.989 | 1.000 | 1.006 | 1.001 | 0.996 | 1.000 | 1.014 | 1.001 | ok |
| lib/fmt_mixed | 20 | 58.7 | 49.0 | 66.9 | 55.5 | 106.2 | 254.5 | 238.3 | 0.553 | 0.462 | 0.984 | 1.000 | 0.987 | 1.000 | 1.007 | 1.000 | 0.983 | 0.999 | ok |
| lib/fmt_q | 20 | 145.8 | 121.2 | 144.1 | 119.1 | 103.3 | 238.6 | 226.1 | 1.412 | 1.173 | 1.001 | 1.000 | 1.011 | 1.000 | 0.996 | 1.000 | 0.999 | 1.000 | ok |
| lib/fmt_s | 5 | 7.2 | 7.0 | 7.6 | 7.1 | 3.2 | 217.6 | 222.2 | 2.263 | 2.205 | 0.966 | 1.000 | 0.988 | 1.000 | 1.008 | 1.000 | 1.025 | 1.000 | ok |
| lib/fmt_x_pad | 2 | 56.9 | 48.5 | 59.6 | 51.4 | 54.0 | 177.1 | 177.5 | 1.053 | 0.898 | 1.012 | 1.000 | 1.009 | 1.000 | 1.007 | 1.000 | 1.026 | 1.001 | ok |
| lib/getmt | 5 | 16.1 | 15.8 | 19.8 | 19.8 | 8.8 | 255.7 | 264.8 | 1.824 | 1.790 | 1.001 | 1.000 | 1.000 | 1.000 | 0.985 | 1.000 | 1.000 | 1.000 | ok |
| lib/gmatch_words | 2 | 178.2 | 138.4 | 179.5 | 138.2 | 173.3 | 315.9 | 319.8 | 1.029 | 0.799 | 1.016 | 1.000 | 1.012 | 1.000 | 0.958 | 1.000 | 1.015 | 1.000 | ok |
| lib/gsub_fn | 20 | 177.5 | 129.3 | 178.1 | 130.6 | 200.2 | 323.4 | 312.5 | 0.887 | 0.646 | 1.021 | 1.000 | 1.004 | 1.004 | 0.786 | 1.000 | 0.999 | 1.000 | ok |
| lib/gsub_str | 5 | 163.9 | 128.1 | 167.0 | 126.8 | 398.5 | 491.6 | 515.6 | 0.411 | 0.321 | 0.979 | 1.000 | 0.999 | 1.000 | 1.016 | 1.000 | 1.000 | 1.000 | ok |
| lib/gsub_tbl | 5 | 165.1 | 106.3 | 152.6 | 110.3 | 187.9 | 321.9 | 321.7 | 0.879 | 0.566 | 1.003 | 1.000 | 0.994 | 1.000 | 0.982 | 1.000 | 1.006 | 1.000 | ok |
| lib/it_ipairs | 20 | 235.8 | 232.3 | 277.4 | 258.6 | 251.6 | 5628.0 | 4827.6 | 0.937 | 0.924 | 0.992 | 1.000 | 0.994 | 1.000 | 0.989 | 1.000 | 0.998 | 1.000 | ok |
| lib/it_next | 5 | 44.9 | 44.8 | 44.8 | 44.4 | 180.5 | 371.1 | 401.4 | 0.248 | 0.248 | 0.994 | 1.000 | 0.997 | 1.000 | 0.983 | 1.000 | 0.992 | 1.000 | ok |
| lib/it_next_empty | 20 | 51.6 | 51.5 | 51.8 | 51.4 | 72.5 | 393.0 | 455.4 | 0.711 | 0.710 | 0.994 | 1.000 | 0.997 | 1.000 | 0.997 | 1.000 | 0.996 | 1.000 | ok |
| lib/it_pairs | 20 | 88.0 | 86.9 | 93.5 | 92.2 | 127.9 | 687.0 | 646.3 | 0.688 | 0.679 | 1.015 | 1.000 | 0.989 | 1.000 | 0.990 | 1.000 | 1.014 | 1.000 | ok |
| lib/it_pairs_hash | 20 | 103.4 | 104.2 | 111.9 | 109.6 | 118.1 | 989.3 | 957.1 | 0.876 | 0.882 | 0.970 | 1.000 | 0.964 | 1.000 | 0.963 | 1.000 | 0.993 | 1.000 | ok |
| lib/len_op | 20 | 47.7 | 47.5 | 57.6 | 57.2 | 100.7 | 1384.7 | 1419.8 | 0.473 | 0.471 | 0.974 | 1.000 | 0.988 | 1.000 | 0.989 | 1.000 | 0.986 | 1.000 | ok |
| lib/len_tbl | 2 | 140.6 | 141.8 | 159.3 | 158.4 | 159.7 | 372.6 | 337.9 | 0.880 | 0.888 | 1.023 | 1.000 | 1.025 | 1.000 | 1.003 | 1.000 | 1.000 | 1.000 | ok |
| lib/load_small | 17 | 228.3 | 155.9 | 227.2 | 167.4 | 215.6 | 378.8 | 345.2 | 1.059 | 0.723 | 0.949 | 1.000 | 0.913 | 0.997 | 1.047 | 1.000 | 1.007 | 1.001 | ok |
| lib/lower | 2 | 89.8 | 81.1 | 91.1 | 82.9 | 110.1 | 289.0 | 294.8 | 0.816 | 0.736 | 1.005 | 1.000 | 0.997 | 1.000 | 1.010 | 1.000 | 0.996 | 1.000 | ok |
| lib/m_abs | 20 | 241.5 | 240.3 | 241.4 | 243.6 | 390.3 | 6061.9 | 6506.8 | 0.619 | 0.616 | 1.001 | 1.000 | 0.996 | 1.000 | 1.000 | 1.000 | 1.009 | 1.000 | ok |
| lib/m_floor | 2 | 227.0 | 226.6 | 227.7 | 227.3 | 206.5 | 3681.1 | 4104.1 | 1.099 | 1.097 | 1.000 | 1.000 | 1.000 | 1.000 | 1.001 | 1.000 | 1.001 | 1.000 | ok |
| lib/m_fmod | 2 | 220.5 | 222.3 | 223.7 | 227.9 | 235.1 | 329.2 | 361.2 | 0.938 | 0.946 | 1.004 | 1.000 | 1.009 | 1.000 | 1.003 | 1.000 | 0.995 | 1.000 | ok |
| lib/m_max | 2 | 258.1 | 257.7 | 259.6 | 259.6 | 401.3 | 8004.8 | 10137.5 | 0.643 | 0.642 | 0.999 | 1.000 | 1.000 | 1.000 | 1.001 | 1.000 | 1.001 | 1.000 | ok |
| lib/m_min | 20 | 226.7 | 225.6 | 248.9 | 221.7 | 387.7 | 6530.7 | 8066.5 | 0.585 | 0.582 | 1.014 | 1.000 | 1.008 | 1.000 | 1.115 | 1.000 | 0.968 | 1.000 | ok |
| lib/m_pow_op | 2 | 280.4 | 280.1 | 280.4 | 280.1 | 275.2 | 1923.5 | 1956.0 | 1.019 | 1.018 | 1.000 | 1.000 | 0.999 | 1.000 | 0.999 | 1.000 | 1.000 | 1.000 | ok |
| lib/m_random | 2 | 79.2 | 78.7 | 79.4 | 79.9 | 73.2 | 345.7 | 375.3 | 1.082 | 1.074 | 0.998 | 1.000 | 0.992 | 1.000 | 1.000 | 1.000 | 1.011 | 1.000 | ok |
| lib/m_random_f | 2 | 53.6 | 53.2 | 53.5 | 53.0 | 89.9 | 272.7 | 312.0 | 0.596 | 0.591 | 1.001 | 1.000 | 1.002 | 1.000 | 1.000 | 1.000 | 0.999 | 1.000 | ok |
| lib/m_sin | 2 | 273.2 | 262.1 | 274.0 | 261.8 | 210.5 | 563.9 | 589.3 | 1.298 | 1.245 | 1.001 | 1.000 | 1.000 | 1.000 | 1.001 | 1.000 | 1.000 | 1.000 | ok |
| lib/m_sqrt | 5 | 275.8 | 274.9 | 281.3 | 278.3 | 270.0 | 2022.6 | 2132.3 | 1.022 | 1.018 | 1.011 | 1.000 | 1.014 | 1.000 | 1.014 | 1.000 | 0.998 | 1.000 | ok |
| lib/m_tointeger | 17 | 227.4 | 228.4 | 223.6 | 236.3 | - | 3718.4 | 3972.3 | - | - | 1.011 | 1.000 | 1.016 | 1.000 | 0.938 | 1.000 | 0.992 | 1.000 | ok |
| lib/m_type | 2 | 32.6 | 32.1 | 32.6 | 32.2 | - | 419.3 | 429.1 | - | - | 1.005 | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | 1.001 | 1.000 | ok |
| lib/m_ult | 20 | 272.2 | 280.2 | 275.8 | 281.0 | - | 415.2 | 435.8 | - | - | 1.003 | 1.000 | 1.000 | 1.000 | 1.015 | 1.000 | 0.995 | 1.000 | ok |
| lib/match | 2 | 62.9 | 51.9 | 61.8 | 52.7 | 193.6 | 319.9 | 303.8 | 0.325 | 0.268 | 1.028 | 1.000 | 1.014 | 1.000 | 0.999 | 1.000 | 1.021 | 1.000 | ok |
| lib/os_clock | 20 | 116.0 | 120.7 | 119.1 | 121.4 | 100.7 | 119.3 | 118.5 | 1.152 | 1.199 | 1.005 | 1.000 | 0.994 | 1.000 | 0.995 | 1.000 | 1.004 | 1.000 | ok |
| lib/pcall_err | 20 | 121.4 | 97.7 | 128.6 | 112.5 | 764.5 | 308.4 | 309.0 | 0.159 | 0.128 | 1.017 | 1.000 | 0.990 | 1.001 | 0.961 | 1.000 | 1.003 | 1.000 | ok |
| lib/pcall_ok | 2 | 30.3 | 30.1 | 30.5 | 30.1 | 41.0 | 1524.0 | 1518.3 | 0.741 | 0.734 | 1.019 | 1.000 | 1.022 | 1.000 | 1.025 | 1.000 | 1.024 | 1.000 | ok |
| lib/rawget_set | 5 | 19.0 | 18.8 | 19.4 | 19.0 | 14.2 | 381.1 | 417.2 | 1.336 | 1.322 | 0.978 | 1.000 | 0.995 | 1.000 | 1.008 | 1.000 | 0.995 | 1.000 | ok |
| lib/rep | 2 | 108.5 | 100.7 | 122.2 | 111.9 | 89.6 | 347.4 | 333.3 | 1.211 | 1.123 | 0.991 | 1.000 | 1.005 | 1.000 | 1.020 | 1.000 | 1.016 | 1.001 | ok |
| lib/rep_sep | 5 | 89.3 | 81.3 | 118.6 | 109.4 | 103.8 | 329.6 | 311.2 | 0.861 | 0.784 | 1.005 | 1.000 | 1.000 | 1.000 | 1.003 | 1.000 | 0.992 | 0.992 | ok |
| lib/reverse | 2 | 100.3 | 94.0 | 104.0 | 99.8 | 115.3 | 309.8 | 314.7 | 0.870 | 0.815 | 1.005 | 1.000 | 0.998 | 1.000 | 0.992 | 1.000 | 1.001 | 1.000 | ok |
| lib/sel_count | 20 | 217.9 | 216.2 | 216.4 | 215.9 | 388.2 | 15509.5 | 14438.4 | 0.561 | 0.557 | 0.998 | 1.000 | 0.991 | 1.000 | 0.993 | 1.000 | 0.988 | 1.000 | ok |
| lib/sel_loop | 5 | 213.6 | 214.0 | 217.8 | 218.2 | 5263.6 | 53689.2 | 54662.9 | 0.041 | 0.041 | 1.019 | 1.000 | 1.028 | 1.000 | 1.006 | 1.000 | 1.014 | 1.000 | ok |
| lib/sel_n | 20 | 320.6 | 321.3 | 320.4 | 322.7 | 675.4 | 25345.4 | 23674.6 | 0.475 | 0.476 | 1.001 | 1.000 | 1.004 | 1.000 | 0.996 | 1.000 | 1.003 | 1.000 | ok |
| lib/setmt | 2 | 188.1 | 162.9 | 228.9 | 192.3 | 331.9 | 743.5 | 835.9 | 0.567 | 0.491 | 1.024 | 1.000 | 1.015 | 1.000 | 1.015 | 1.000 | 1.027 | 1.000 | ok |
| lib/str_eq | 2 | 160.8 | 161.0 | 161.2 | 160.7 | 182.5 | 1479.5 | 1529.4 | 0.881 | 0.882 | 1.000 | 1.000 | 1.004 | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | ok |
| lib/str_eq_dyn | 2 | 134.7 | 135.7 | 137.1 | 135.4 | 150.4 | 1277.3 | 1348.8 | 0.896 | 0.902 | 0.993 | 1.000 | 1.003 | 1.000 | 1.009 | 1.000 | 0.993 | 1.000 | ok |
| lib/str_eq_long | 20 | 35.7 | 35.0 | 35.0 | 35.8 | 21.5 | 215.5 | 222.1 | 1.663 | 1.628 | 1.003 | 1.000 | 0.990 | 1.000 | 0.980 | 1.000 | 1.026 | 1.000 | ok |
| lib/str_idx_concat_key | 5 | 206.9 | 191.2 | 208.3 | 194.0 | 166.6 | 464.0 | 461.2 | 1.242 | 1.148 | 0.997 | 1.000 | 0.997 | 1.000 | 1.000 | 1.000 | 0.993 | 1.000 | ok |
| lib/str_lt | 2 | 94.9 | 92.2 | 91.3 | 87.6 | 66.3 | 227.1 | 228.3 | 1.430 | 1.389 | 1.000 | 1.000 | 1.000 | 1.000 | 1.007 | 1.000 | 1.001 | 1.000 | ok |
| lib/str_lt_short | 5 | 114.2 | 106.9 | 106.6 | 99.2 | 62.5 | 249.8 | 255.3 | 1.826 | 1.710 | 1.001 | 1.000 | 1.005 | 1.000 | 1.009 | 1.000 | 1.003 | 1.000 | ok |
| lib/str_method_len | 20 | 45.3 | 44.7 | 50.7 | 50.1 | 55.3 | 1386.6 | 1429.7 | 0.820 | 0.808 | 1.002 | 1.000 | 0.995 | 1.000 | 0.993 | 1.000 | 0.994 | 1.000 | ok |
| lib/string_key_const | 2 | 178.3 | 177.8 | 202.3 | 201.1 | 176.9 | 1151.6 | 1165.1 | 1.008 | 1.005 | 1.000 | 1.000 | 0.998 | 1.000 | 1.002 | 1.000 | 0.998 | 1.000 | ok |
| lib/string_key_dyn | 2 | 229.5 | 229.3 | 228.5 | 228.2 | 268.0 | 1082.3 | 1101.0 | 0.856 | 0.856 | 1.000 | 1.000 | 1.001 | 1.000 | 1.001 | 1.000 | 1.000 | 1.000 | ok |
| lib/sub | 20 | 131.0 | 122.1 | 130.1 | 120.7 | 80.5 | 469.5 | 482.7 | 1.627 | 1.516 | 0.992 | 1.000 | 0.989 | 1.000 | 0.989 | 1.000 | 0.991 | 1.000 | ok |
| lib/sub_1char | 5 | 119.7 | 109.9 | 122.0 | 108.9 | 98.2 | 459.2 | 478.8 | 1.219 | 1.119 | 1.011 | 1.000 | 1.011 | 1.000 | 1.013 | 1.000 | 1.009 | 1.000 | ok |
| lib/tconcat | 2 | 151.5 | 101.3 | 174.4 | 121.1 | 133.4 | 346.9 | 322.3 | 1.136 | 0.759 | 1.026 | 1.000 | 1.013 | 0.999 | 1.019 | 1.000 | 1.012 | 0.995 | ok |
| lib/tconcat_num | 20 | 172.5 | 117.3 | 182.6 | 120.0 | 246.0 | 908.0 | 844.8 | 0.701 | 0.477 | 0.985 | 1.000 | 0.994 | 1.000 | 1.011 | 1.000 | 0.985 | 1.000 | ok |
| lib/tfront_1k | 20 | 78.2 | 75.7 | 80.0 | 75.7 | 406.3 | 4375.7 | 4241.0 | 0.193 | 0.186 | 0.986 | 1.000 | 0.990 | 0.998 | 0.989 | 1.000 | 0.985 | 0.998 | ok |
| lib/tinsert_append | 5 | 159.5 | 133.7 | 193.0 | 181.8 | 141.7 | 539.7 | 574.7 | 1.125 | 0.944 | 1.017 | 0.999 | 1.001 | 1.000 | 1.011 | 1.000 | 0.999 | 0.998 | ok |
| lib/tinsert_front | 20 | 224.7 | 178.0 | 264.0 | 211.1 | 272.9 | 455.4 | 493.5 | 0.823 | 0.652 | 1.009 | 1.000 | 1.000 | 1.008 | 1.012 | 1.000 | 0.997 | 0.998 | ok |
| lib/tinsert_idx | 2 | 179.2 | 152.7 | 203.4 | 191.3 | 154.6 | 328.8 | 383.3 | 1.159 | 0.988 | 1.029 | 1.000 | 1.010 | 1.000 | 1.012 | 1.000 | 1.008 | 0.998 | ok |
| lib/tmove | 2 | 103.8 | 103.3 | 125.2 | 125.0 | 105.8 | 700.5 | 683.1 | 0.982 | 0.977 | 1.000 | 1.000 | 1.000 | 1.000 | 1.001 | 1.000 | 1.002 | 1.000 | ok |
| lib/tmove_1k | 17 | 95.5 | 68.8 | 107.1 | 70.8 | 111.6 | 476.4 | 447.3 | 0.856 | 0.616 | 0.986 | 1.000 | 1.010 | 1.000 | 1.031 | 1.000 | 1.002 | 1.000 | ok |
| lib/tonumber_bad | 20 | 144.1 | 119.4 | 165.2 | 143.4 | 168.0 | 408.1 | 431.1 | 0.858 | 0.711 | 0.961 | 1.000 | 1.070 | 0.996 | 1.000 | 1.000 | 1.007 | 1.000 | ok |
| lib/tonumber_base | 2 | 266.7 | 262.2 | 270.9 | 262.6 | 229.2 | 432.1 | 451.7 | 1.163 | 1.144 | 1.002 | 1.000 | 1.004 | 1.000 | 0.987 | 1.000 | 1.000 | 1.000 | ok |
| lib/tonumber_flt | 2 | 196.1 | 173.1 | 203.9 | 166.7 | 776.7 | 577.4 | 596.1 | 0.253 | 0.223 | 1.000 | 1.000 | 1.016 | 0.998 | 1.015 | 1.000 | 1.013 | 1.000 | ok |
| lib/tonumber_int | 20 | 152.6 | 139.1 | 159.9 | 141.9 | 96.2 | 288.1 | 305.0 | 1.587 | 1.447 | 0.987 | 1.000 | 0.996 | 1.000 | 0.995 | 1.000 | 0.998 | 1.000 | ok |
| lib/tonumber_num | 20 | 216.6 | 214.5 | 214.5 | 213.8 | 383.1 | 6476.7 | 7506.3 | 0.565 | 0.560 | 1.010 | 1.000 | 1.015 | 1.000 | 1.027 | 1.000 | 0.998 | 1.000 | ok |
| lib/tostring_flt | 2 | 84.4 | 65.8 | 83.8 | 61.5 | 82.5 | 299.3 | 214.9 | 1.023 | 0.797 | 1.017 | 1.000 | 1.010 | 1.000 | 0.990 | 1.000 | 1.007 | 1.002 | ok |
| lib/tostring_int | 20 | 121.0 | 108.2 | 136.6 | 122.0 | 279.8 | 543.3 | 450.7 | 0.433 | 0.387 | 1.001 | 1.000 | 1.000 | 1.000 | 0.990 | 1.000 | 1.013 | 1.003 | ok |
| lib/tostring_tbl | 20 | 107.6 | 105.9 | 109.7 | 106.0 | 93.3 | 313.8 | 280.9 | 1.153 | 1.135 | 0.982 | 1.000 | 1.003 | 1.000 | 0.970 | 1.000 | 0.977 | 1.000 | ok |
| lib/tpack | 20 | 146.8 | 113.5 | 214.0 | 164.9 | 306.1 | 499.0 | 485.7 | 0.479 | 0.371 | 0.988 | 1.000 | 1.002 | 1.000 | 1.013 | 1.000 | 0.957 | 0.989 | ok |
| lib/tremove_front | 20 | 148.7 | 132.0 | 180.8 | 166.1 | 191.0 | 592.3 | 593.7 | 0.779 | 0.691 | 0.591 | 0.547 | 0.595 | 0.568 | 0.634 | 0.629 | 0.723 | 0.674 | ok |
| lib/tremove_pop | 20 | 241.0 | 171.5 | 220.5 | 205.8 | 136.7 | 483.9 | 552.9 | 1.763 | 1.254 | 0.999 | 0.997 | 0.983 | 0.989 | 0.991 | 0.993 | 0.995 | 0.987 | ok |
| lib/tsort_cmp | 5 | 264.7 | 261.3 | 279.6 | 259.4 | 285.5 | 339.9 | 336.1 | 0.927 | 0.915 | 0.978 | 1.000 | 1.019 | 1.008 | 1.015 | 1.000 | 1.006 | 1.000 | ok |
| lib/tsort_int | 2 | 269.1 | 222.9 | 290.4 | 224.8 | 294.0 | 382.4 | 406.0 | 0.915 | 0.758 | 0.972 | 1.000 | 1.022 | 1.000 | 1.010 | 1.000 | 1.018 | 1.000 | ok |
| lib/tsort_str | 2 | 258.0 | 155.2 | 291.8 | 160.0 | 244.8 | 364.6 | 401.0 | 1.054 | 0.634 | 0.989 | 1.000 | 1.015 | 1.000 | 1.000 | 1.000 | 1.014 | 1.000 | ok |
| lib/tunpack | 2 | 118.4 | 104.4 | 127.2 | 111.7 | 154.8 | 487.1 | 448.5 | 0.765 | 0.675 | 1.007 | 1.000 | 1.006 | 1.000 | 1.002 | 1.000 | 1.000 | 0.998 | ok |
| lib/type_call | 2 | 87.8 | 87.5 | 94.8 | 94.2 | 85.6 | 494.3 | 468.3 | 1.026 | 1.022 | 1.013 | 1.000 | 1.012 | 1.000 | 1.011 | 1.000 | 1.009 | 1.000 | ok |
| lib/upper | 2 | 95.1 | 88.8 | 101.1 | 95.7 | 120.7 | 307.4 | 312.6 | 0.788 | 0.735 | 0.999 | 1.000 | 1.000 | 1.000 | 0.998 | 1.000 | 1.000 | 1.000 | ok |
| lib/utf8_len | 20 | 270.7 | 274.7 | 273.5 | 279.1 | - | 442.2 | 456.7 | - | - | 0.996 | 1.000 | 0.991 | 1.000 | 0.995 | 1.000 | 0.991 | 1.000 | ok |
| programs/binary_trees | 2 | 1543.3 | 1326.8 | 1799.7 | 1497.3 | 2007.4 | 4389.1 | 4447.3 | 0.769 | 0.661 | 1.023 | 1.000 | 1.022 | 1.000 | 1.012 | 1.000 | 1.019 | 1.002 | ok |
| programs/closures | 20 | 459.2 | 420.5 | 513.9 | 476.6 | 949.8 | 5063.2 | 4986.1 | 0.483 | 0.443 | 0.989 | 1.000 | 0.992 | 1.000 | 0.993 | 1.000 | 0.991 | 1.003 | ok |
| programs/coroutines | 20 | 1715.7 | 1321.4 | 1798.9 | 1396.5 | 871.7 | 3733.4 | 3656.9 | 1.968 | 1.516 | 0.952 | 1.000 | 1.015 | 1.001 | 0.976 | 1.000 | 1.003 | 0.999 | ok |
| programs/fannkuch | 5 | 1059.6 | 1048.0 | 1088.3 | 1079.8 | 1075.7 | 4075.1 | 4114.8 | 0.985 | 0.974 | 0.999 | 0.976 | 0.983 | 0.976 | 0.993 | 0.973 | 0.987 | 0.973 | ok |
| programs/fasta | 2 | 1292.2 | 1180.2 | 1396.2 | 1271.0 | 1495.7 | 3776.8 | 3842.3 | 0.864 | 0.789 | 1.004 | 1.000 | 1.009 | 1.000 | 1.003 | 1.000 | 1.010 | 0.999 | ok |
| programs/fib | 5 | 1257.5 | 1290.7 | 1255.7 | 1251.0 | 469.7 | 4139.5 | 4150.5 | 2.677 | 2.748 | 1.003 | 1.000 | 1.028 | 1.000 | 1.007 | 1.000 | 1.001 | 1.000 | ok |
| programs/gc_churn | 20 | 2158.0 | 1845.6 | 3315.9 | 2806.2 | 3608.7 | 4605.5 | 5988.9 | 0.598 | 0.511 | 0.986 | 1.000 | 1.011 | 0.999 | 1.011 | 1.000 | 0.987 | 0.992 | ok |
| programs/heapsort | 2 | 1055.7 | 1060.0 | 2262.9 | 2255.3 | 1286.6 | 4063.2 | 3957.0 | 0.821 | 0.824 | 0.984 | 1.000 | 0.991 | 1.000 | 1.000 | 1.003 | 1.004 | 1.002 | ok |
| programs/json | 17 | 1809.7 | 1404.5 | 1918.4 | 1540.8 | 1470.3 | 3127.6 | 3229.9 | 1.231 | 0.955 | 1.008 | 1.000 | 1.017 | 1.000 | 1.008 | 1.000 | 1.001 | 1.000 | ok |
| programs/knucleotide | 2 | 1051.8 | 1072.6 | 1071.0 | 1074.7 | 691.4 | 4061.5 | 4161.0 | 1.521 | 1.551 | 0.999 | 1.000 | 1.003 | 1.000 | 0.999 | 1.000 | 1.003 | 1.000 | ok |
| programs/load_big | 20 | 735.7 | 542.8 | 637.4 | 451.5 | 491.1 | 1787.3 | 620.0 | 1.498 | 1.105 | 0.908 | 1.000 | 0.997 | 0.995 | 1.025 | 1.000 | 1.011 | 0.999 | CHECK |
| programs/loops_arith | 2 | 479.4 | 479.8 | 484.4 | 483.7 | 1108.2 | 4344.5 | 4909.6 | 0.433 | 0.433 | 1.000 | 1.000 | 1.002 | 1.000 | 1.003 | 1.000 | 1.002 | 1.000 | CHECK |
| programs/mandelbrot | 2 | 533.6 | 533.5 | 535.2 | 534.5 | 521.6 | 4727.1 | 5530.2 | 1.023 | 1.023 | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | 0.999 | 1.000 | ok |
| programs/matmul | 20 | 421.2 | 429.8 | 431.7 | 433.2 | 272.0 | 3171.0 | 3693.4 | 1.549 | 1.580 | 1.008 | 1.000 | 1.022 | 1.000 | 0.991 | 1.000 | 1.002 | 1.000 | ok |
| programs/metamethods | 5 | 663.6 | 629.2 | 798.6 | 746.2 | 540.1 | 4614.3 | 4669.9 | 1.229 | 1.165 | 0.996 | 1.000 | 1.003 | 1.000 | 1.005 | 1.000 | 0.993 | 0.998 | ok |
| programs/nbody | 5 | 345.0 | 343.9 | 354.6 | 354.4 | 345.6 | 4246.2 | 4718.3 | 0.998 | 0.995 | 0.990 | 0.984 | 0.988 | 0.984 | 0.994 | 0.984 | 0.995 | 0.984 | ok |
| programs/oop_methods | 2 | 142.7 | 141.3 | 143.9 | 142.7 | 304.8 | 4567.3 | 4685.2 | 0.468 | 0.464 | 1.008 | 0.999 | 0.984 | 0.999 | 1.004 | 0.999 | 0.994 | 0.999 | ok |
| programs/pcall_error | 20 | 1520.0 | 1213.2 | 1480.0 | 1244.9 | 8346.6 | 4028.8 | 4363.2 | 0.182 | 0.145 | 1.034 | 1.000 | 0.994 | 1.001 | 1.011 | 1.000 | 1.012 | 1.000 | ok |
| programs/queens | 2 | 1838.1 | 1840.0 | 1939.8 | 1939.2 | 1907.1 | 4368.7 | 4120.8 | 0.964 | 0.965 | 0.998 | 1.000 | 0.999 | 1.000 | 1.003 | 1.000 | 1.005 | 1.000 | ok |
| programs/sieve | 17 | 576.6 | 576.0 | 608.8 | 671.1 | 761.7 | 2750.2 | 5029.1 | 0.757 | 0.756 | 0.995 | 1.000 | 1.003 | 1.000 | 1.016 | 1.000 | 1.004 | 1.000 | ok |
| programs/spectral_norm | 2 | 182.2 | 183.0 | 188.5 | 186.2 | 171.1 | 4573.7 | 4337.9 | 1.065 | 1.070 | 1.002 | 1.000 | 0.999 | 1.000 | 1.006 | 1.000 | 0.997 | 1.000 | ok |
| programs/string_ops | 5 | 1179.1 | 1092.7 | 1123.5 | 1031.5 | 2238.4 | 2828.0 | 2807.1 | 0.527 | 0.488 | 1.024 | 1.000 | 1.012 | 1.000 | 1.023 | 1.000 | 1.011 | 1.000 | ok |
| programs/table_array | 2 | 357.3 | 304.8 | 400.6 | 314.1 | 600.0 | 3400.3 | 3492.4 | 0.596 | 0.508 | 0.972 | 1.000 | 1.005 | 1.003 | 1.011 | 1.000 | 1.005 | 1.000 | ok |
| programs/table_hash | 20 | 524.2 | 512.0 | 525.7 | 515.3 | 394.4 | 1501.6 | 1831.0 | 1.329 | 1.298 | 1.043 | 1.000 | 1.041 | 1.000 | 1.021 | 1.000 | 1.020 | 1.000 | ok |
| programs/varargs | 5 | 1230.4 | 1095.2 | 1204.0 | 1103.6 | 2194.8 | 5366.5 | 5084.7 | 0.561 | 0.499 | 1.013 | 1.000 | 1.003 | 1.000 | 1.006 | 1.000 | 1.008 | 0.999 | ok |
| real/game_loop | 2 | 35.1 | 33.5 | 39.6 | 38.1 | 21.7 | 207.6 | 221.0 | 1.614 | 1.544 | 1.029 | 0.995 | 1.028 | 0.995 | 1.024 | 0.996 | 1.026 | 0.996 | ok |
| real/json_roundtrip | 5 | 557.9 | 456.6 | 581.4 | 476.6 | 469.1 | 796.8 | 802.8 | 1.189 | 0.973 | 1.002 | 1.000 | 1.001 | 1.000 | 0.995 | 1.000 | 0.994 | 1.000 | ok |
| real/lua_interp | 20 | 103.0 | 100.3 | 110.4 | 105.8 | 59.3 | 223.6 | 225.0 | 1.737 | 1.691 | 1.001 | 1.000 | 1.007 | 1.000 | 1.032 | 1.002 | 1.020 | 1.001 | ok |
| real/template_engine | 17 | 727.1 | 594.0 | 743.6 | 617.2 | 955.0 | 1225.1 | 1249.2 | 0.761 | 0.622 | 1.013 | 1.000 | 1.028 | 1.001 | 0.946 | 1.000 | 0.989 | 1.000 | ok |
| real/wordfreq | 2 | 113.4 | 97.4 | 115.1 | 99.0 | 116.4 | 212.8 | 210.2 | 0.975 | 0.837 | 0.992 | 1.000 | 0.997 | 1.000 | 0.989 | 1.000 | 1.000 | 1.000 | ok |

## Output / .expected check

- programs/load_big [p54]: out=diff exp=none err=
- programs/load_big [g54]: out=diff exp=none err=
- programs/loops_arith [p54]: out=diff exp=none err=
- programs/loops_arith [g54]: out=diff exp=none err=
- rerun programs/load_big [p54]: out=diff exp=none err=
- rerun programs/load_big [g54]: out=diff exp=none err=

LuaJIT: programs without a LuaJIT variant or whose LuaJIT output differs:

- corpus/testrunner_luaunit: output differs
- corpus2/dev/real_dis_arm: output differs
- lib/m_tointeger: skipped (no LuaJIT equivalent)
- lib/m_type: skipped (no LuaJIT equivalent)
- lib/m_ult: skipped (no LuaJIT equivalent)
- lib/utf8_len: skipped (no LuaJIT equivalent)
- programs/coroutines: output differs
- programs/load_big: output differs
- programs/oop_methods: output differs
