# Tables, session-m (subject 8cd1f06)

Ratios below 1: first column faster/fewer. Ours = min over all readings (2 quick, 5 full, 15-run serial reruns). Cached columns as stored.


# Aggregates, readings A: the fast pass only (min of 2 or 5 runs per program)

## Headline geomeans (cycles, ours/ref)

| build | /LuaJIT cyc (n) | /LuaJIT ins | /PUC same version cyc (223) | /PUC ins |
|---|---|---|---|---|
| 5.5 plain | 0.936 (219) | 1.290 | 0.214 | 0.211 |
| 5.5 PGO | 0.859 (219) | 1.192 | 0.196 | 0.195 |
| 5.4 plain | 0.996 (219) | 1.385 | 0.222 | 0.226 |
| 5.4 PGO | 0.916 (219) | 1.281 | 0.205 | 0.209 |

- 5.5 plain / LuaJIT -joff (219): 0.316
- 5.5 PGO / LuaJIT -joff (219): 0.290
- 5a064d7 cached (set B) (same programs): 5.5 plain/LJ 0.938, PGO/LJ 0.857, 5.4 plain/LJ 0.998, 5.4 PGO/LJ 0.914
- session-k published 5a064d7 (same programs): 5.5 plain/LJ 0.945, PGO/LJ 0.864, 5.4 plain/LJ 1.007, 5.4 PGO/LJ 0.921
- 7413bff (session-l subject) (same programs): 5.5 plain/LJ 0.936, PGO/LJ 0.856, 5.4 plain/LJ 0.999, 5.4 PGO/LJ 0.913
- our 5.4 build / our 5.5 build cycles: plain 1.063, PGO 1.066; PGO/plain: 5.5 0.919, 5.4 0.921

## Subject vs 5a064d7 (same session, set B), 5a064d7 set A, session-k published and b116447 (geomean, 223 programs)

| build | vs | cycles | instructions | cyc >3% better / within / >3% worse | ins >3% fewer / more |
|---|---|---|---|---|---|
| 5.5 plain | 5a064d7 cached (set B) | 0.999 | 0.961 | 36 / 149 / 38 | 70 / 3 |
| 5.5 plain | 7413bff same machine (session-l subject) | 1.000 | 0.965 | 33 / 155 / 35 | 68 / 5 |
| 5.5 plain | session-k published 5a064d7 | 0.990 | 0.961 | 42 / 160 / 21 | 70 / 3 |
| 5.5 PGO | 5a064d7 cached (set B) | 1.003 | 0.963 | 36 / 148 / 39 | 67 / 4 |
| 5.5 PGO | 7413bff same machine (session-l subject) | 1.004 | 0.967 | 29 / 160 / 34 | 64 / 6 |
| 5.5 PGO | session-k published 5a064d7 | 0.994 | 0.962 | 39 / 158 / 26 | 67 / 4 |
| 5.4 plain | 5a064d7 cached (set B) | 0.998 | 0.965 | 37 / 153 / 33 | 64 / 4 |
| 5.4 plain | 7413bff same machine (session-l subject) | 0.997 | 0.969 | 33 / 168 / 22 | 61 / 5 |
| 5.4 plain | session-k published 5a064d7 | 0.990 | 0.965 | 41 / 163 / 19 | 64 / 4 |
| 5.4 PGO | 5a064d7 cached (set B) | 1.002 | 0.967 | 35 / 151 / 37 | 62 / 5 |
| 5.4 PGO | 7413bff same machine (session-l subject) | 1.003 | 0.971 | 28 / 162 / 33 | 58 / 6 |
| 5.4 PGO | session-k published 5a064d7 | 0.994 | 0.967 | 42 / 157 / 24 | 62 / 5 |

### Per set, geomean subject / 5a064d7 and subject / b116447 (cycles / instructions)

| set | n | plain vs b cyc | ins | PGO vs b cyc | ins | 5.4 plain vs b cyc | ins | 5.4 PGO vs b cyc | ins | plain vs i cyc | ins | PGO vs 7413bff cyc | ins | 5.4 plain vs i cyc | ins | 5.4 PGO vs 7413bff cyc | ins |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| apps | 5 | 0.971 | 0.951 | 0.976 | 0.955 | 0.969 | 0.954 | 0.954 | 0.955 | 0.980 | 0.963 | 0.979 | 0.964 | 0.989 | 0.969 | 0.981 | 0.967 |
| awfy | 14 | 1.022 | 0.977 | 1.015 | 0.979 | 1.012 | 0.977 | 1.014 | 0.978 | 1.017 | 0.983 | 1.012 | 0.983 | 1.009 | 0.983 | 1.013 | 0.983 |
| corpus | 18 | 0.980 | 0.968 | 0.991 | 0.971 | 0.968 | 0.961 | 0.976 | 0.965 | 0.982 | 0.976 | 0.989 | 0.978 | 0.970 | 0.963 | 0.974 | 0.968 |
| corpus2/dev | 17 | 1.022 | 0.963 | 1.039 | 0.966 | 1.007 | 0.959 | 1.024 | 0.961 | 1.034 | 0.987 | 1.044 | 0.989 | 1.009 | 0.981 | 1.028 | 0.982 |
| corpus2/heldout | 11 | 1.027 | 0.994 | 1.035 | 0.994 | 1.014 | 0.997 | 1.008 | 0.998 | 1.013 | 1.001 | 1.019 | 1.001 | 1.004 | 1.012 | 1.002 | 1.013 |
| jit-trace | 29 | 0.979 | 0.878 | 0.980 | 0.878 | 0.997 | 0.900 | 0.996 | 0.900 | 0.981 | 0.883 | 0.983 | 0.883 | 0.998 | 0.905 | 0.998 | 0.905 |
| lib | 95 | 0.992 | 0.974 | 0.993 | 0.975 | 0.993 | 0.976 | 0.997 | 0.979 | 0.994 | 0.974 | 0.995 | 0.975 | 0.990 | 0.976 | 0.998 | 0.979 |
| programs | 25 | 1.016 | 0.971 | 1.025 | 0.975 | 1.026 | 0.977 | 1.029 | 0.980 | 1.017 | 0.974 | 1.028 | 0.978 | 1.024 | 0.977 | 1.027 | 0.981 |
| real | 5 | 1.005 | 0.992 | 1.013 | 0.993 | 0.997 | 0.993 | 1.002 | 0.995 | 1.007 | 0.993 | 1.013 | 0.994 | 1.000 | 0.994 | 1.004 | 0.996 |
| real_dis | 4 | 1.047 | 1.006 | 1.052 | 1.019 | 1.029 | 1.001 | 1.036 | 1.015 | 1.033 | 1.005 | 1.055 | 1.018 | 1.014 | 0.999 | 1.028 | 1.014 |
| ALL | 223 | 0.999 | 0.961 | 1.003 | 0.963 | 0.998 | 0.965 | 1.002 | 0.967 | 1.000 | 0.965 | 1.004 | 0.967 | 0.997 | 0.969 | 1.003 | 0.971 |

# Aggregates, readings B: with the 15-run serial reruns of 145 programs merged (min over all readings)

## Headline geomeans (cycles, ours/ref)

| build | /LuaJIT cyc (n) | /LuaJIT ins | /PUC same version cyc (223) | /PUC ins |
|---|---|---|---|---|
| 5.5 plain | 0.924 (219) | 1.290 | 0.211 | 0.211 |
| 5.5 PGO | 0.849 (219) | 1.192 | 0.194 | 0.195 |
| 5.4 plain | 0.984 (219) | 1.385 | 0.220 | 0.226 |
| 5.4 PGO | 0.904 (219) | 1.281 | 0.202 | 0.209 |

- 5.5 plain / LuaJIT -joff (219): 0.312
- 5.5 PGO / LuaJIT -joff (219): 0.287
- 5a064d7 cached (set B) (same programs): 5.5 plain/LJ 0.938, PGO/LJ 0.857, 5.4 plain/LJ 0.998, 5.4 PGO/LJ 0.914
- session-k published 5a064d7 (same programs): 5.5 plain/LJ 0.945, PGO/LJ 0.864, 5.4 plain/LJ 1.007, 5.4 PGO/LJ 0.921
- 7413bff (session-l subject) (same programs): 5.5 plain/LJ 0.936, PGO/LJ 0.856, 5.4 plain/LJ 0.999, 5.4 PGO/LJ 0.913
- our 5.4 build / our 5.5 build cycles: plain 1.064, PGO 1.065; PGO/plain: 5.5 0.920, 5.4 0.921

## Subject vs 5a064d7 (same session, set B), 5a064d7 set A, session-k published and b116447 (geomean, 223 programs)

| build | vs | cycles | instructions | cyc >3% better / within / >3% worse | ins >3% fewer / more |
|---|---|---|---|---|---|
| 5.5 plain | 5a064d7 cached (set B) | 0.986 | 0.961 | 43 / 163 / 17 | 70 / 3 |
| 5.5 plain | 7413bff same machine (session-l subject) | 0.987 | 0.965 | 43 / 167 / 13 | 68 / 5 |
| 5.5 plain | session-k published 5a064d7 | 0.978 | 0.961 | 52 / 163 / 8 | 70 / 3 |
| 5.5 PGO | 5a064d7 cached (set B) | 0.991 | 0.963 | 43 / 158 / 22 | 67 / 4 |
| 5.5 PGO | 7413bff same machine (session-l subject) | 0.992 | 0.967 | 35 / 173 / 15 | 64 / 6 |
| 5.5 PGO | session-k published 5a064d7 | 0.983 | 0.962 | 52 / 156 / 15 | 67 / 4 |
| 5.4 plain | 5a064d7 cached (set B) | 0.987 | 0.965 | 41 / 174 / 8 | 64 / 4 |
| 5.4 plain | 7413bff same machine (session-l subject) | 0.985 | 0.969 | 37 / 179 / 7 | 61 / 5 |
| 5.4 plain | session-k published 5a064d7 | 0.978 | 0.965 | 50 / 167 / 6 | 64 / 4 |
| 5.4 PGO | 5a064d7 cached (set B) | 0.990 | 0.967 | 38 / 173 / 12 | 62 / 5 |
| 5.4 PGO | 7413bff same machine (session-l subject) | 0.991 | 0.971 | 32 / 184 / 7 | 58 / 6 |
| 5.4 PGO | session-k published 5a064d7 | 0.982 | 0.967 | 49 / 168 / 6 | 62 / 5 |

### Per set, geomean subject / 5a064d7 and subject / b116447 (cycles / instructions)

| set | n | plain vs b cyc | ins | PGO vs b cyc | ins | 5.4 plain vs b cyc | ins | 5.4 PGO vs b cyc | ins | plain vs i cyc | ins | PGO vs 7413bff cyc | ins | 5.4 plain vs i cyc | ins | 5.4 PGO vs 7413bff cyc | ins |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| apps | 5 | 0.971 | 0.951 | 0.976 | 0.955 | 0.969 | 0.954 | 0.954 | 0.955 | 0.980 | 0.963 | 0.979 | 0.964 | 0.989 | 0.969 | 0.981 | 0.967 |
| awfy | 14 | 0.994 | 0.977 | 0.995 | 0.979 | 0.986 | 0.977 | 0.989 | 0.978 | 0.990 | 0.983 | 0.992 | 0.983 | 0.983 | 0.983 | 0.988 | 0.983 |
| corpus | 18 | 0.977 | 0.968 | 0.987 | 0.971 | 0.964 | 0.961 | 0.973 | 0.965 | 0.979 | 0.976 | 0.985 | 0.978 | 0.966 | 0.963 | 0.970 | 0.968 |
| corpus2/dev | 17 | 0.964 | 0.963 | 0.979 | 0.966 | 0.957 | 0.959 | 0.970 | 0.961 | 0.976 | 0.987 | 0.983 | 0.989 | 0.958 | 0.981 | 0.974 | 0.982 |
| corpus2/heldout | 11 | 1.009 | 0.994 | 1.018 | 0.994 | 0.995 | 0.997 | 0.991 | 0.998 | 0.995 | 1.001 | 1.001 | 1.001 | 0.986 | 1.012 | 0.986 | 1.013 |
| jit-trace | 29 | 0.975 | 0.878 | 0.977 | 0.878 | 0.993 | 0.900 | 0.994 | 0.900 | 0.978 | 0.883 | 0.979 | 0.883 | 0.994 | 0.905 | 0.995 | 0.905 |
| lib | 95 | 0.988 | 0.974 | 0.991 | 0.975 | 0.990 | 0.976 | 0.993 | 0.979 | 0.990 | 0.974 | 0.993 | 0.975 | 0.988 | 0.976 | 0.995 | 0.979 |
| programs | 25 | 0.990 | 0.971 | 1.000 | 0.975 | 0.997 | 0.977 | 1.000 | 0.980 | 0.990 | 0.974 | 1.002 | 0.978 | 0.995 | 0.977 | 0.997 | 0.980 |
| real | 5 | 1.005 | 0.992 | 1.013 | 0.993 | 0.997 | 0.993 | 1.002 | 0.995 | 1.007 | 0.993 | 1.013 | 0.994 | 1.000 | 0.994 | 1.004 | 0.996 |
| real_dis | 4 | 1.020 | 1.006 | 1.028 | 1.018 | 1.007 | 1.001 | 1.013 | 1.014 | 1.006 | 1.005 | 1.031 | 1.018 | 0.993 | 0.999 | 1.006 | 1.014 |
| ALL | 223 | 0.986 | 0.961 | 0.991 | 0.963 | 0.987 | 0.965 | 0.990 | 0.967 | 0.987 | 0.965 | 0.992 | 0.967 | 0.985 | 0.969 | 0.991 | 0.971 |

### 5.5 plain vs 5a064d7: 20 biggest improvements by cycles (>= 20 Mcyc)

| program | Mcyc | cyc ratio | ins ratio | cycle spread of readings | note |
|---|---|---|---|---|---|
| jit-trace/upval_acc | 1010 | 0.731 | 0.812 | 7.0% (20 runs) | instructions move the same way |
| corpus/lua_lexer | 821 | 0.761 | 0.756 | 25.8% (20 runs) | instructions move the same way |
| lib/co_yield | 183 | 0.765 | 0.919 | 7.8% (20 runs) | instructions move the same way |
| corpus2/dev/coro_generators | 979 | 0.781 | 0.945 | 2.7% (20 runs) | instructions move the same way |
| lib/tonumber_num | 169 | 0.784 | 0.839 | 7.5% (20 runs) | instructions move the same way |
| lib/find_pat | 203 | 0.789 | 0.795 | 40.5% (20 runs) | instructions move the same way |
| programs/coroutines | 1442 | 0.800 | 0.930 | 8.9% (20 runs) | instructions move the same way |
| corpus2/dev/oop_mixins | 1116 | 0.800 | 0.720 | 38.2% (20 runs) | instructions move the same way |
| apps/raytrace | 96 | 0.861 | 0.843 | 5.9% (20 runs) | instructions move the same way |
| jit-trace/rec_loop | 80 | 0.869 | 0.832 | 5.6% (20 runs) | instructions move the same way |
| lib/len_op | 42 | 0.873 | 0.920 | 33.0% (20 runs) | instructions move the same way |
| lib/str_lt_short | 101 | 0.892 | 0.963 | 13.4% (20 runs) | instructions move the same way |
| corpus2/dev/expr_compiler | 1183 | 0.896 | 0.925 | 20.5% (20 runs) | instructions move the same way |
| lib/match | 55 | 0.900 | 0.886 | 7.5% (20 runs) | instructions move the same way |
| jit-trace/inline_leaf | 410 | 0.904 | 0.876 | 2.2% (20 runs) | instructions move the same way |
| jit-trace/field_method | 516 | 0.910 | 0.936 | 12.6% (20 runs) | instructions move the same way |
| lib/pcall_ok | 28 | 0.914 | 0.875 | 4.3% (20 runs) | instructions move the same way |
| awfy/sieve | 114 | 0.924 | 0.865 | 5.8% (20 runs) | instructions move the same way |
| lib/m_min | 204 | 0.931 | 0.861 | 4.8% (20 runs) | instructions move the same way |
| corpus/csv_ini_yaml | 1461 | 0.936 | 0.916 | 12.6% (20 runs) | instructions move the same way |

### 5.5 PGO vs 5a064d7: 20 biggest improvements by cycles (>= 20 Mcyc)

| program | Mcyc | cyc ratio | ins ratio | cycle spread of readings | note |
|---|---|---|---|---|---|
| jit-trace/upval_acc | 1018 | 0.736 | 0.812 | 6.2% (20 runs) | instructions move the same way |
| corpus/lua_lexer | 722 | 0.753 | 0.752 | 6.7% (20 runs) | instructions move the same way |
| corpus2/dev/oop_mixins | 1050 | 0.773 | 0.708 | 47.9% (20 runs) | instructions move the same way |
| lib/find_pat | 180 | 0.786 | 0.766 | 8.2% (20 runs) | instructions move the same way |
| lib/tonumber_num | 169 | 0.797 | 0.839 | 33.2% (20 runs) | instructions move the same way |
| jit-trace/rec_loop | 79 | 0.867 | 0.832 | 23.3% (20 runs) | instructions move the same way |
| apps/raytrace | 91 | 0.875 | 0.847 | 4.4% (20 runs) | instructions move the same way |
| corpus2/dev/expr_compiler | 1000 | 0.888 | 0.910 | 47.3% (20 runs) | instructions move the same way |
| lib/m_min | 203 | 0.898 | 0.861 | 5.0% (20 runs) | instructions move the same way |
| lib/match | 46 | 0.899 | 0.895 | 7.4% (20 runs) | instructions move the same way |
| lib/len_op | 43 | 0.903 | 0.920 | 21.2% (20 runs) | instructions move the same way |
| lib/pcall_ok | 27 | 0.905 | 0.875 | 5.2% (20 runs) | instructions move the same way |
| lib/str_lt_short | 96 | 0.907 | 0.962 | 3.6% (20 runs) | instructions move the same way |
| jit-trace/inline_leaf | 413 | 0.911 | 0.878 | 3.0% (20 runs) | instructions move the same way |
| jit-trace/field_method | 513 | 0.911 | 0.936 | 16.7% (20 runs) | instructions move the same way |
| awfy/sieve | 113 | 0.921 | 0.865 | 4.2% (20 runs) | instructions move the same way |
| programs/matmul | 409 | 0.940 | 0.881 | 6.8% (20 runs) | instructions move the same way |
| corpus2/dev/coro_generators | 908 | 0.944 | 0.999 | 3.7% (20 runs) | instructions equal: layout / IPC |
| jit-trace/int_rec2 | 640 | 0.946 | 0.902 | 11.9% (20 runs) | instructions move the same way |
| corpus2/dev/lua_parser | 1059 | 0.950 | 0.876 | 8.6% (20 runs) | instructions move the same way |

### 5.4 plain vs 5a064d7: 20 biggest improvements by cycles (>= 20 Mcyc)

| program | Mcyc | cyc ratio | ins ratio | cycle spread of readings | note |
|---|---|---|---|---|---|
| lib/co_yield | 175 | 0.725 | 0.919 | 10.0% (20 runs) | instructions move the same way |
| programs/coroutines | 1392 | 0.750 | 0.934 | 6.6% (20 runs) | instructions move the same way |
| lib/find_pat | 208 | 0.768 | 0.799 | 4.3% (20 runs) | instructions move the same way |
| corpus/lua_lexer | 855 | 0.770 | 0.767 | 46.6% (20 runs) | instructions move the same way |
| lib/tonumber_num | 168 | 0.773 | 0.839 | 35.3% (20 runs) | instructions move the same way |
| corpus2/dev/coro_generators | 1006 | 0.775 | 0.937 | 3.1% (20 runs) | instructions move the same way |
| corpus2/dev/oop_mixins | 1124 | 0.786 | 0.731 | 34.9% (20 runs) | instructions move the same way |
| corpus2/dev/lua_parser | 1278 | 0.834 | 0.816 | 8.4% (20 runs) | instructions move the same way |
| corpus/csv_ini_yaml | 1688 | 0.899 | 0.894 | 2.2% (20 runs) | instructions move the same way |
| corpus/astar_jumper | 1660 | 0.899 | 0.844 | 6.0% (20 runs) | instructions move the same way |
| apps/raytrace | 106 | 0.900 | 0.884 | 6.5% (20 runs) | instructions move the same way |
| jit-trace/inline_leaf | 411 | 0.904 | 0.876 | 4.9% (20 runs) | instructions move the same way |
| lib/pcall_ok | 27 | 0.904 | 0.875 | 4.9% (20 runs) | instructions move the same way |
| lib/m_min | 202 | 0.909 | 0.861 | 4.4% (20 runs) | instructions move the same way |
| awfy/sieve | 115 | 0.909 | 0.886 | 3.9% (20 runs) | instructions move the same way |
| jit-trace/field_method | 520 | 0.910 | 0.935 | 13.8% (20 runs) | instructions move the same way |
| corpus2/dev/expr_compiler | 1232 | 0.911 | 0.920 | 30.7% (20 runs) | instructions move the same way |
| lib/match | 56 | 0.915 | 0.880 | 4.7% (20 runs) | instructions move the same way |
| apps/expr_lang | 146 | 0.917 | 0.906 | 1.1% (5 runs) | instructions move the same way |
| corpus2/heldout/vending_fsm | 1018 | 0.931 | 0.902 | 8.5% (20 runs) | instructions move the same way |

### 5.4 PGO vs 5a064d7: 20 biggest improvements by cycles (>= 20 Mcyc)

| program | Mcyc | cyc ratio | ins ratio | cycle spread of readings | note |
|---|---|---|---|---|---|
| corpus/lua_lexer | 752 | 0.757 | 0.766 | 24.4% (20 runs) | instructions move the same way |
| lib/find_pat | 185 | 0.783 | 0.770 | 5.1% (20 runs) | instructions move the same way |
| lib/tonumber_num | 167 | 0.790 | 0.839 | 33.8% (20 runs) | instructions move the same way |
| corpus2/dev/oop_mixins | 1052 | 0.796 | 0.723 | 35.5% (20 runs) | instructions move the same way |
| corpus2/dev/lua_parser | 1092 | 0.833 | 0.798 | 7.2% (20 runs) | instructions move the same way |
| lib/match | 46 | 0.883 | 0.886 | 4.2% (20 runs) | instructions move the same way |
| apps/expr_lang | 129 | 0.886 | 0.904 | 2.0% (5 runs) | instructions move the same way |
| awfy/sieve | 116 | 0.900 | 0.886 | 3.4% (20 runs) | instructions move the same way |
| jit-trace/inline_leaf | 410 | 0.902 | 0.877 | 4.4% (20 runs) | instructions move the same way |
| corpus2/dev/expr_compiler | 1055 | 0.903 | 0.906 | 12.0% (20 runs) | instructions move the same way |
| apps/raytrace | 99 | 0.906 | 0.885 | 4.2% (20 runs) | instructions move the same way |
| lib/pcall_ok | 27 | 0.911 | 0.875 | 8.7% (20 runs) | instructions move the same way |
| jit-trace/field_method | 515 | 0.911 | 0.937 | 43.5% (20 runs) | instructions move the same way |
| corpus/astar_jumper | 1617 | 0.914 | 0.839 | 5.0% (20 runs) | instructions move the same way |
| lib/m_min | 201 | 0.917 | 0.861 | 4.5% (20 runs) | instructions move the same way |
| lib/pcall_err | 102 | 0.920 | 1.005 | 10.1% (20 runs) | instructions equal: layout / IPC |
| jit-trace/rec_loop | 82 | 0.928 | 0.834 | 2.2% (20 runs) | instructions move the same way |
| corpus2/heldout/vending_fsm | 944 | 0.934 | 0.898 | 5.5% (20 runs) | instructions move the same way |
| programs/coroutines | 1322 | 0.935 | 1.035 | 5.1% (20 runs) |  |
| lib/error_tbl | 203 | 0.936 | 1.005 | 16.7% (20 runs) | instructions equal: layout / IPC |

### 5.5 plain vs 5a064d7: regressions (instructions > +2% or cycles > +3%, >= 20 Mcyc)

| program | Mcyc | cyc ratio | ins ratio | cycle spread of readings | note |
|---|---|---|---|---|---|
| lib/str_eq | 210 | 1.312 | 0.922 | 0.7% (20 runs) |  |
| corpus2/heldout/real_dis_ppc | 2237 | 1.074 | 1.054 | 4.9% (20 runs) | instructions move the same way |
| jit-trace/tfor_loops | 231 | 1.062 | 0.989 | 2.2% (5 runs) |  |
| lib/concat_op_str | 142 | 1.056 | 1.000 | 0.1% (2 runs) | instructions equal: layout / IPC |
| programs/string_ops | 1154 | 1.052 | 1.002 | 26.8% (20 runs) | instructions equal: layout / IPC |
| lib/tinsert_idx | 185 | 1.051 | 1.047 | 3.3% (20 runs) | instructions move the same way |
| corpus2/heldout/prototype_objects | 1450 | 1.048 | 1.007 | 11.2% (20 runs) | instructions equal: layout / IPC |
| lib/len_tbl | 143 | 1.042 | 0.987 | 3.1% (20 runs) |  |
| lib/type_call | 90 | 1.042 | 0.999 | 3.1% (20 runs) | instructions equal: layout / IPC |
| lib/error_tbl | 277 | 1.041 | 1.008 | 6.8% (20 runs) | instructions equal: layout / IPC |
| apps/markdown | 121 | 1.040 | 1.009 | 0.7% (5 runs) | instructions equal: layout / IPC |
| corpus/parser_combinator | 2364 | 1.037 | 1.031 | 5.0% (20 runs) | instructions move the same way |
| corpus2/heldout/roguelike_turns | 833 | 1.035 | 1.004 | 4.7% (17 runs) | instructions equal: layout / IPC |
| lib/str_eq_long | 37 | 1.032 | 0.961 | 11.6% (20 runs) |  |
| corpus2/dev/csv_report | 1168 | 1.032 | 1.018 | 26.8% (20 runs) | instructions move the same way |
| corpus2/dev/real_dis_x86 | 1572 | 1.031 | 1.003 | 19.1% (20 runs) | instructions equal: layout / IPC |
| awfy/storage | 102 | 1.010 | 1.024 | 6.1% (20 runs) |  |

### 5.5 PGO vs 5a064d7: regressions (instructions > +2% or cycles > +3%, >= 20 Mcyc)

| program | Mcyc | cyc ratio | ins ratio | cycle spread of readings | note |
|---|---|---|---|---|---|
| lib/str_eq | 210 | 1.312 | 0.922 | 0.6% (20 runs) |  |
| corpus2/heldout/real_dis_ppc | 2015 | 1.084 | 1.071 | 5.1% (20 runs) | instructions move the same way |
| corpus/parser_combinator | 2002 | 1.072 | 1.053 | 2.5% (20 runs) | instructions move the same way |
| programs/string_ops | 1097 | 1.072 | 1.004 | 35.2% (20 runs) | instructions equal: layout / IPC |
| corpus2/heldout/prototype_objects | 1388 | 1.072 | 1.010 | 14.6% (20 runs) | instructions move the same way |
| corpus2/heldout/log_analyzer | 1858 | 1.063 | 0.984 | 3.9% (20 runs) |  |
| lib/len_tbl | 143 | 1.054 | 0.987 | 2.5% (20 runs) |  |
| corpus2/dev/real_dis_x86 | 1411 | 1.051 | 1.012 | 8.3% (20 runs) | instructions move the same way |
| programs/gc_churn | 1853 | 1.048 | 1.007 | 36.1% (20 runs) | instructions equal: layout / IPC |
| lib/setmt | 167 | 1.047 | 1.000 | 15.1% (20 runs) | instructions equal: layout / IPC |
| apps/markdown | 106 | 1.045 | 1.023 | 1.7% (5 runs) | instructions move the same way |
| lib/sel_n | 337 | 1.045 | 1.000 | 10.9% (20 runs) | instructions equal: layout / IPC |
| corpus2/dev/template_gsub | 1467 | 1.043 | 1.008 | 1.9% (5 runs) | instructions equal: layout / IPC |
| jit-trace/tfor_loops | 230 | 1.043 | 0.989 | 1.6% (5 runs) |  |
| corpus2/heldout/roguelike_turns | 799 | 1.042 | 1.005 | 4.7% (17 runs) | instructions equal: layout / IPC |
| corpus2/dev/csv_report | 1052 | 1.039 | 1.020 | 51.7% (20 runs) | instructions move the same way |
| lib/str_eq_long | 36 | 1.036 | 0.961 | 12.4% (20 runs) |  |
| lib/type_call | 90 | 1.034 | 1.000 | 8.1% (20 runs) | instructions equal: layout / IPC |
| corpus/serpent_ser | 1785 | 1.033 | 0.983 | 3.6% (5 runs) |  |
| programs/table_hash | 491 | 1.031 | 1.004 | 68.7% (20 runs) | instructions equal: layout / IPC |
| lib/tremove_pop | 178 | 1.031 | 1.027 | 6.5% (20 runs) | instructions move the same way |
| lib/tonumber_base | 267 | 1.026 | 1.030 | 7.0% (20 runs) |  |
| lib/tinsert_idx | 155 | 1.020 | 1.046 | 3.5% (20 runs) |  |
| awfy/storage | 79 | 1.007 | 1.025 | 12.0% (20 runs) |  |
| programs/coroutines | 1304 | 1.003 | 1.034 | 7.0% (20 runs) |  |

### 5.4 plain vs 5a064d7: regressions (instructions > +2% or cycles > +3%, >= 20 Mcyc)

| program | Mcyc | cyc ratio | ins ratio | cycle spread of readings | note |
|---|---|---|---|---|---|
| lib/str_eq | 210 | 1.308 | 0.922 | 1.0% (20 runs) |  |
| programs/table_hash | 543 | 1.111 | 1.006 | 33.8% (20 runs) | instructions equal: layout / IPC |
| lib/fmt_q | 162 | 1.103 | 0.988 | 3.2% (20 runs) |  |
| lib/gsub_tbl | 165 | 1.087 | 0.988 | 4.8% (20 runs) |  |
| corpus2/heldout/prototype_objects | 1464 | 1.066 | 1.003 | 24.2% (20 runs) | instructions equal: layout / IPC |
| programs/string_ops | 1117 | 1.058 | 0.999 | 25.2% (20 runs) | instructions equal: layout / IPC |
| lib/type_call | 98 | 1.041 | 0.989 | 4.9% (20 runs) |  |
| programs/closures | 531 | 1.038 | 0.985 | 6.6% (20 runs) |  |
| lib/tinsert_append | 196 | 1.023 | 1.038 | 1.4% (5 runs) |  |
| corpus2/dev/ecs_world | 1015 | 1.021 | 1.031 | 5.9% (20 runs) |  |
| lib/tinsert_idx | 209 | 1.018 | 1.040 | 3.5% (20 runs) |  |
| corpus2/heldout/stream_closures | 827 | 0.973 | 1.127 | 2.0% (5 runs) |  |

### 5.4 PGO vs 5a064d7: regressions (instructions > +2% or cycles > +3%, >= 20 Mcyc)

| program | Mcyc | cyc ratio | ins ratio | cycle spread of readings | note |
|---|---|---|---|---|---|
| lib/str_eq | 210 | 1.312 | 0.922 | 1.9% (20 runs) |  |
| corpus2/heldout/prototype_objects | 1411 | 1.069 | 1.000 | 13.4% (20 runs) | instructions equal: layout / IPC |
| lib/load_small | 167 | 1.057 | 0.992 | 2.4% (20 runs) | instructions equal: layout / IPC |
| programs/string_ops | 1021 | 1.052 | 1.005 | 28.1% (20 runs) | instructions equal: layout / IPC |
| lib/tinsert_idx | 198 | 1.042 | 1.038 | 39.5% (20 runs) | instructions move the same way |
| lib/tonumber_base | 274 | 1.039 | 1.027 | 14.6% (20 runs) | instructions move the same way |
| lib/lower | 85 | 1.037 | 1.000 | 0.1% (2 runs) | instructions equal: layout / IPC |
| lib/type_call | 97 | 1.035 | 0.989 | 4.4% (20 runs) |  |
| corpus2/dev/real_dis_x86 | 1471 | 1.031 | 1.019 | 5.2% (20 runs) | instructions move the same way |
| lib/tinsert_append | 187 | 1.031 | 1.037 | 1.3% (5 runs) | instructions move the same way |
| lib/sel_n | 333 | 1.031 | 1.000 | 10.0% (20 runs) | instructions equal: layout / IPC |
| corpus2/heldout/real_dis_ppc | 1981 | 1.024 | 1.028 | 4.4% (20 runs) |  |
| corpus2/dev/ecs_world | 874 | 1.018 | 1.034 | 4.7% (20 runs) |  |
| corpus2/heldout/stream_closures | 765 | 0.981 | 1.138 | 1.9% (5 runs) |  |
| programs/coroutines | 1322 | 0.935 | 1.035 | 5.1% (20 runs) |  |

### 5.5 plain vs 7413bff: 20 biggest improvements by cycles (>= 20 Mcyc)

| program | Mcyc | cyc ratio | ins ratio | cycle spread of readings | note |
|---|---|---|---|---|---|
| jit-trace/upval_acc | 1010 | 0.731 | 0.812 | 7.0% (20 runs) | instructions move the same way |
| lib/co_yield | 183 | 0.769 | 0.919 | 7.8% (20 runs) | instructions move the same way |
| lib/find_pat | 203 | 0.774 | 0.796 | 40.5% (20 runs) | instructions move the same way |
| corpus2/dev/coro_generators | 979 | 0.779 | 0.947 | 2.7% (20 runs) | instructions move the same way |
| programs/coroutines | 1442 | 0.807 | 0.930 | 8.9% (20 runs) | instructions move the same way |
| corpus/lua_lexer | 821 | 0.841 | 0.845 | 25.8% (20 runs) | instructions move the same way |
| lib/tonumber_num | 169 | 0.856 | 0.839 | 7.5% (20 runs) | instructions move the same way |
| lib/len_op | 42 | 0.870 | 0.920 | 33.0% (20 runs) | instructions move the same way |
| apps/raytrace | 96 | 0.888 | 0.882 | 5.9% (20 runs) | instructions move the same way |
| jit-trace/inline_leaf | 410 | 0.897 | 0.876 | 2.2% (20 runs) | instructions move the same way |
| lib/m_min | 204 | 0.901 | 0.861 | 4.8% (20 runs) | instructions move the same way |
| corpus2/dev/expr_compiler | 1183 | 0.905 | 0.932 | 20.5% (20 runs) | instructions move the same way |
| jit-trace/field_method | 516 | 0.908 | 0.936 | 12.6% (20 runs) | instructions move the same way |
| lib/match | 55 | 0.911 | 0.887 | 7.5% (20 runs) | instructions move the same way |
| awfy/sieve | 114 | 0.914 | 0.865 | 5.8% (20 runs) | instructions move the same way |
| lib/pcall_ok | 28 | 0.914 | 0.876 | 4.3% (20 runs) | instructions move the same way |
| lib/co_wrap | 227 | 0.927 | 0.959 | 5.9% (20 runs) | instructions move the same way |
| corpus/coro_scheduler | 1405 | 0.928 | 0.972 | 2.1% (20 runs) | instructions move the same way |
| jit-trace/type_entry_stable | 1385 | 0.941 | 0.774 | 2.3% (5 runs) | instructions move the same way |
| lib/find_cap | 262 | 0.945 | 1.003 | 1.7% (5 runs) | instructions equal: layout / IPC |

### 5.5 PGO vs 7413bff: 20 biggest improvements by cycles (>= 20 Mcyc)

| program | Mcyc | cyc ratio | ins ratio | cycle spread of readings | note |
|---|---|---|---|---|---|
| jit-trace/upval_acc | 1018 | 0.737 | 0.812 | 6.2% (20 runs) | instructions move the same way |
| lib/find_pat | 180 | 0.781 | 0.766 | 8.2% (20 runs) | instructions move the same way |
| lib/tonumber_num | 169 | 0.828 | 0.839 | 33.2% (20 runs) | instructions move the same way |
| corpus/lua_lexer | 722 | 0.832 | 0.842 | 6.7% (20 runs) | instructions move the same way |
| corpus2/dev/expr_compiler | 1000 | 0.886 | 0.916 | 47.3% (20 runs) | instructions move the same way |
| lib/m_min | 203 | 0.889 | 0.861 | 5.0% (20 runs) | instructions move the same way |
| lib/match | 46 | 0.896 | 0.895 | 7.4% (20 runs) | instructions move the same way |
| lib/len_op | 43 | 0.897 | 0.920 | 21.2% (20 runs) | instructions move the same way |
| apps/raytrace | 91 | 0.902 | 0.877 | 4.4% (20 runs) | instructions move the same way |
| jit-trace/inline_leaf | 413 | 0.907 | 0.877 | 3.0% (20 runs) | instructions move the same way |
| jit-trace/field_method | 513 | 0.908 | 0.936 | 16.7% (20 runs) | instructions move the same way |
| awfy/sieve | 113 | 0.916 | 0.865 | 4.2% (20 runs) | instructions move the same way |
| lib/pcall_ok | 27 | 0.928 | 0.875 | 5.2% (20 runs) | instructions move the same way |
| corpus2/dev/coro_generators | 908 | 0.934 | 1.000 | 3.7% (20 runs) | instructions equal: layout / IPC |
| lib/co_yield | 173 | 0.934 | 1.003 | 7.6% (20 runs) | instructions equal: layout / IPC |
| corpus/csv_ini_yaml | 1278 | 0.937 | 0.919 | 6.5% (20 runs) | instructions move the same way |
| jit-trace/type_entry_stable | 1380 | 0.940 | 0.774 | 1.2% (5 runs) | instructions move the same way |
| jit-trace/int_rec2 | 640 | 0.945 | 0.902 | 11.9% (20 runs) | instructions move the same way |
| corpus2/dev/lua_parser | 1059 | 0.949 | 0.878 | 8.6% (20 runs) | instructions move the same way |
| lib/byte | 48 | 0.953 | 0.952 | 12.9% (20 runs) | instructions move the same way |

### 5.4 plain vs 7413bff: 20 biggest improvements by cycles (>= 20 Mcyc)

| program | Mcyc | cyc ratio | ins ratio | cycle spread of readings | note |
|---|---|---|---|---|---|
| programs/coroutines | 1392 | 0.754 | 0.934 | 6.6% (20 runs) | instructions move the same way |
| lib/co_yield | 175 | 0.767 | 0.919 | 10.0% (20 runs) | instructions move the same way |
| corpus/lua_lexer | 855 | 0.767 | 0.767 | 46.6% (20 runs) | instructions move the same way |
| corpus2/dev/coro_generators | 1006 | 0.778 | 0.938 | 3.1% (20 runs) | instructions move the same way |
| lib/find_pat | 208 | 0.788 | 0.799 | 4.3% (20 runs) | instructions move the same way |
| corpus2/dev/lua_parser | 1278 | 0.821 | 0.831 | 8.4% (20 runs) | instructions move the same way |
| lib/tonumber_num | 168 | 0.823 | 0.839 | 35.3% (20 runs) | instructions move the same way |
| corpus/astar_jumper | 1660 | 0.886 | 0.853 | 6.0% (20 runs) | instructions move the same way |
| jit-trace/field_method | 520 | 0.899 | 0.936 | 13.8% (20 runs) | instructions move the same way |
| lib/pcall_ok | 27 | 0.901 | 0.876 | 4.9% (20 runs) | instructions move the same way |
| lib/str_method_len | 51 | 0.902 | 0.956 | 18.6% (20 runs) | instructions move the same way |
| jit-trace/inline_leaf | 411 | 0.902 | 0.876 | 4.9% (20 runs) | instructions move the same way |
| awfy/sieve | 115 | 0.903 | 0.886 | 3.9% (20 runs) | instructions move the same way |
| corpus/csv_ini_yaml | 1688 | 0.908 | 0.908 | 2.2% (20 runs) | instructions move the same way |
| lib/match | 56 | 0.910 | 0.880 | 4.7% (20 runs) | instructions move the same way |
| corpus2/dev/expr_compiler | 1232 | 0.910 | 0.934 | 30.7% (20 runs) | instructions move the same way |
| lib/m_min | 202 | 0.911 | 0.861 | 4.4% (20 runs) | instructions move the same way |
| lib/len_op | 58 | 0.931 | 0.961 | 4.0% (20 runs) | instructions move the same way |
| corpus/coro_scheduler | 1507 | 0.934 | 0.971 | 3.3% (20 runs) | instructions move the same way |
| lib/byte | 48 | 0.934 | 0.952 | 5.8% (20 runs) | instructions move the same way |

### 5.4 PGO vs 7413bff: 20 biggest improvements by cycles (>= 20 Mcyc)

| program | Mcyc | cyc ratio | ins ratio | cycle spread of readings | note |
|---|---|---|---|---|---|
| corpus/lua_lexer | 752 | 0.764 | 0.765 | 24.4% (20 runs) | instructions move the same way |
| lib/find_pat | 185 | 0.788 | 0.770 | 5.1% (20 runs) | instructions move the same way |
| corpus2/dev/lua_parser | 1092 | 0.818 | 0.813 | 7.2% (20 runs) | instructions move the same way |
| lib/tonumber_num | 167 | 0.827 | 0.839 | 33.8% (20 runs) | instructions move the same way |
| lib/match | 46 | 0.879 | 0.886 | 4.2% (20 runs) | instructions move the same way |
| corpus/astar_jumper | 1617 | 0.897 | 0.848 | 5.0% (20 runs) | instructions move the same way |
| jit-trace/inline_leaf | 410 | 0.898 | 0.877 | 4.4% (20 runs) | instructions move the same way |
| corpus2/dev/expr_compiler | 1055 | 0.900 | 0.920 | 12.0% (20 runs) | instructions move the same way |
| awfy/sieve | 116 | 0.906 | 0.886 | 3.4% (20 runs) | instructions move the same way |
| lib/m_min | 201 | 0.909 | 0.861 | 4.5% (20 runs) | instructions move the same way |
| jit-trace/field_method | 515 | 0.912 | 0.937 | 43.5% (20 runs) | instructions move the same way |
| lib/pcall_ok | 27 | 0.919 | 0.875 | 8.7% (20 runs) | instructions move the same way |
| corpus/csv_ini_yaml | 1488 | 0.930 | 0.912 | 1.7% (20 runs) | instructions move the same way |
| apps/raytrace | 99 | 0.934 | 0.914 | 4.2% (20 runs) | instructions move the same way |
| lib/error_tbl | 203 | 0.937 | 1.008 | 16.7% (20 runs) | instructions equal: layout / IPC |
| lib/byte | 47 | 0.940 | 0.952 | 5.4% (20 runs) | instructions move the same way |
| lib/co_yield | 172 | 0.944 | 1.009 | 11.1% (20 runs) | instructions equal: layout / IPC |
| lib/pcall_err | 102 | 0.944 | 1.006 | 10.1% (20 runs) | instructions equal: layout / IPC |
| corpus2/dev/coro_generators | 936 | 0.946 | 0.987 | 3.9% (20 runs) | instructions move the same way |
| lib/co_wrap | 187 | 0.948 | 1.002 | 7.7% (20 runs) | instructions equal: layout / IPC |

### 5.5 plain vs 7413bff: regressions (instructions > +2% or cycles > +3%, >= 20 Mcyc)

| program | Mcyc | cyc ratio | ins ratio | cycle spread of readings | note |
|---|---|---|---|---|---|
| lib/str_eq | 210 | 1.312 | 0.922 | 0.7% (20 runs) |  |
| corpus2/dev/regex_nfa | 693 | 1.105 | 1.043 | 7.6% (20 runs) | instructions move the same way |
| lib/tinsert_idx | 185 | 1.063 | 1.048 | 3.3% (20 runs) | instructions move the same way |
| lib/error_tbl | 277 | 1.059 | 1.008 | 6.8% (20 runs) | instructions equal: layout / IPC |
| corpus2/heldout/real_dis_ppc | 2237 | 1.059 | 1.054 | 4.9% (20 runs) | instructions move the same way |
| lib/sel_n | 337 | 1.041 | 1.000 | 8.7% (20 runs) | instructions equal: layout / IPC |
| lib/str_eq_long | 37 | 1.036 | 0.961 | 11.6% (20 runs) |  |
| lib/type_call | 90 | 1.036 | 1.000 | 3.1% (20 runs) | instructions equal: layout / IPC |
| lib/len_tbl | 143 | 1.033 | 0.987 | 3.1% (20 runs) |  |
| programs/string_ops | 1154 | 1.032 | 1.002 | 26.8% (20 runs) | instructions equal: layout / IPC |
| lib/it_pairs | 90 | 1.032 | 1.000 | 6.9% (20 runs) | instructions equal: layout / IPC |
| corpus/classic_vector | 479 | 1.030 | 1.000 | 8.0% (20 runs) | instructions equal: layout / IPC |
| corpus/parser_combinator | 2364 | 1.022 | 1.031 | 5.0% (20 runs) |  |
| awfy/storage | 102 | 1.017 | 1.024 | 6.1% (20 runs) |  |
| corpus2/heldout/vending_fsm | 979 | 1.001 | 1.033 | 6.2% (20 runs) |  |

### 5.5 PGO vs 7413bff: regressions (instructions > +2% or cycles > +3%, >= 20 Mcyc)

| program | Mcyc | cyc ratio | ins ratio | cycle spread of readings | note |
|---|---|---|---|---|---|
| lib/str_eq | 210 | 1.315 | 0.922 | 0.6% (20 runs) |  |
| corpus2/heldout/real_dis_ppc | 2015 | 1.103 | 1.073 | 5.1% (20 runs) | instructions move the same way |
| lib/tonumber_bad | 120 | 1.074 | 0.993 | 9.7% (20 runs) | instructions equal: layout / IPC |
| corpus2/heldout/log_analyzer | 1858 | 1.053 | 0.985 | 3.9% (20 runs) |  |
| programs/string_ops | 1097 | 1.053 | 1.002 | 35.2% (20 runs) | instructions equal: layout / IPC |
| lib/load_small | 154 | 1.046 | 1.009 | 3.2% (20 runs) | instructions equal: layout / IPC |
| corpus2/dev/real_dis_x86 | 1411 | 1.043 | 1.014 | 8.3% (20 runs) | instructions move the same way |
| lib/setmt | 167 | 1.043 | 1.000 | 15.1% (20 runs) | instructions equal: layout / IPC |
| corpus/parser_combinator | 2002 | 1.043 | 1.053 | 2.5% (20 runs) | instructions move the same way |
| lib/upper | 93 | 1.040 | 1.000 | 1.8% (17 runs) | instructions equal: layout / IPC |
| lib/len_tbl | 143 | 1.037 | 0.987 | 2.5% (20 runs) |  |
| corpus2/dev/regex_nfa | 565 | 1.036 | 1.039 | 2.8% (20 runs) | instructions move the same way |
| lib/type_call | 90 | 1.033 | 1.000 | 8.1% (20 runs) | instructions equal: layout / IPC |
| lib/tremove_pop | 178 | 1.033 | 1.027 | 6.5% (20 runs) | instructions move the same way |
| apps/markdown | 106 | 1.029 | 1.024 | 1.7% (5 runs) |  |
| lib/tonumber_base | 267 | 1.029 | 1.030 | 7.0% (20 runs) |  |
| lib/tinsert_idx | 155 | 1.027 | 1.047 | 3.5% (20 runs) |  |
| corpus2/dev/csv_report | 1052 | 1.015 | 1.022 | 51.7% (20 runs) |  |
| awfy/storage | 79 | 1.011 | 1.025 | 12.0% (20 runs) |  |
| corpus2/heldout/vending_fsm | 903 | 1.009 | 1.034 | 8.1% (20 runs) |  |
| programs/coroutines | 1304 | 0.972 | 1.032 | 7.0% (20 runs) |  |

### 5.4 plain vs 7413bff: regressions (instructions > +2% or cycles > +3%, >= 20 Mcyc)

| program | Mcyc | cyc ratio | ins ratio | cycle spread of readings | note |
|---|---|---|---|---|---|
| lib/str_eq | 210 | 1.310 | 0.922 | 1.0% (20 runs) |  |
| lib/fmt_q | 162 | 1.109 | 0.988 | 3.2% (20 runs) |  |
| programs/table_hash | 543 | 1.107 | 1.006 | 33.8% (20 runs) | instructions equal: layout / IPC |
| lib/gsub_tbl | 165 | 1.073 | 0.988 | 4.8% (20 runs) |  |
| programs/string_ops | 1117 | 1.045 | 0.999 | 25.2% (20 runs) | instructions equal: layout / IPC |
| programs/pcall_error | 1519 | 1.041 | 0.996 | 7.0% (20 runs) | instructions equal: layout / IPC |
| lib/tonumber_flt | 202 | 1.034 | 0.998 | 2.8% (17 runs) | instructions equal: layout / IPC |
| corpus2/heldout/real_dis_ppc | 2253 | 1.028 | 1.032 | 5.0% (20 runs) |  |
| lib/tinsert_idx | 209 | 1.022 | 1.040 | 3.5% (20 runs) |  |
| corpus2/dev/ecs_world | 1015 | 1.017 | 1.032 | 5.9% (20 runs) |  |
| lib/tinsert_append | 196 | 1.012 | 1.038 | 1.4% (5 runs) |  |
| corpus2/heldout/stream_closures | 827 | 0.986 | 1.135 | 2.0% (5 runs) |  |

### 5.4 PGO vs 7413bff: regressions (instructions > +2% or cycles > +3%, >= 20 Mcyc)

| program | Mcyc | cyc ratio | ins ratio | cycle spread of readings | note |
|---|---|---|---|---|---|
| lib/str_eq | 210 | 1.310 | 0.922 | 1.9% (20 runs) |  |
| lib/tonumber_base | 274 | 1.052 | 1.027 | 14.6% (20 runs) | instructions move the same way |
| programs/string_ops | 1021 | 1.048 | 1.005 | 28.1% (20 runs) | instructions equal: layout / IPC |
| corpus2/heldout/real_dis_ppc | 1981 | 1.046 | 1.050 | 4.4% (20 runs) | instructions move the same way |
| lib/tinsert_idx | 198 | 1.033 | 1.038 | 39.5% (20 runs) | instructions move the same way |
| lib/utf8_len | 288 | 1.030 | 1.009 | 0.7% (20 runs) | instructions equal: layout / IPC |
| corpus2/dev/real_dis_x86 | 1471 | 1.029 | 1.022 | 5.2% (20 runs) |  |
| lib/tinsert_append | 187 | 1.028 | 1.037 | 1.3% (5 runs) |  |
| corpus2/dev/ecs_world | 874 | 1.017 | 1.034 | 4.7% (20 runs) |  |
| corpus2/heldout/stream_closures | 765 | 0.995 | 1.140 | 1.9% (5 runs) |  |
| programs/coroutines | 1322 | 0.956 | 1.035 | 5.1% (20 runs) |  |

### 5.5 plain: 15 biggest gaps to LuaJIT (ours/LJ cycles)

| program | Mcyc | cyc/LJ | ins/LJ |
|---|---|---|---|
| awfy/permute | 36 | 2.764 | 3.554 |
| lib/fmt_s | 8 | 2.406 | 2.603 |
| apps/raytrace | 96 | 2.370 | 1.827 |
| awfy/cd | 252 | 2.344 | 2.310 |
| corpus2/dev/text_adventure | 2611 | 2.200 | 2.177 |
| lib/byte_multi | 40 | 2.003 | 3.471 |
| corpus/classic_vector | 479 | 1.989 | 1.786 |
| awfy/towers | 46 | 1.978 | 1.695 |
| corpus2/heldout/stream_closures | 806 | 1.938 | 2.102 |
| lib/find_plain | 125 | 1.848 | 2.319 |
| awfy/json | 136 | 1.847 | 2.110 |
| corpus/lua_lexer | 821 | 1.832 | 1.748 |
| lib/co_yield | 183 | 1.789 | 1.730 |
| lib/tremove_pop | 244 | 1.787 | 2.125 |
| real/lua_interp | 106 | 1.780 | 2.213 |

### 5.5 plain: 15 biggest wins over LuaJIT (>= 20 Mcyc)

| program | Mcyc | cyc/LJ | ins/LJ |
|---|---|---|---|
| lib/sel_loop | 209 | 0.040 | 0.062 |
| lib/error_tbl | 277 | 0.091 | 0.080 |
| jit-trace/int_rec2 | 640 | 0.128 | 1.399 |
| lib/pcall_err | 120 | 0.157 | 0.136 |
| programs/pcall_error | 1483 | 0.178 | 0.185 |
| lib/tfront_1k | 79 | 0.193 | 0.076 |
| lib/it_next | 45 | 0.250 | 0.703 |
| lib/tonumber_flt | 195 | 0.251 | 0.393 |
| corpus/sha2_base64 | 379 | 0.269 | 0.307 |
| lib/match | 55 | 0.285 | 0.280 |
| lib/find_pat | 203 | 0.288 | 0.359 |
| jit-trace/int_sum | 876 | 0.349 | 1.107 |
| apps/markdown | 121 | 0.368 | 0.270 |
| programs/loops_arith | 455 | 0.411 | 0.962 |
| lib/gsub_str | 165 | 0.414 | 0.501 |

### 5.5 PGO: 15 biggest gaps to LuaJIT (ours/LJ cycles)

| program | Mcyc | cyc/LJ | ins/LJ |
|---|---|---|---|
| awfy/permute | 36 | 2.737 | 3.543 |
| apps/raytrace | 91 | 2.248 | 1.714 |
| lib/fmt_s | 7 | 2.241 | 2.507 |
| awfy/cd | 234 | 2.171 | 2.155 |
| lib/byte_multi | 40 | 1.985 | 3.462 |
| awfy/towers | 45 | 1.923 | 1.666 |
| corpus2/dev/text_adventure | 2151 | 1.813 | 1.952 |
| corpus/classic_vector | 429 | 1.784 | 1.691 |
| corpus2/heldout/stream_closures | 734 | 1.764 | 1.949 |
| lib/getmt | 15 | 1.733 | 2.292 |
| real/lua_interp | 103 | 1.730 | 2.185 |
| awfy/queens | 94 | 1.704 | 1.894 |
| lib/find_plain | 115 | 1.701 | 2.090 |
| jit-trace/type_entry | 1443 | 1.700 | 1.883 |
| lib/co_yield | 173 | 1.694 | 1.639 |

### 5.5 PGO: 15 biggest wins over LuaJIT (>= 20 Mcyc)

| program | Mcyc | cyc/LJ | ins/LJ |
|---|---|---|---|
| lib/sel_loop | 210 | 0.040 | 0.062 |
| lib/error_tbl | 211 | 0.069 | 0.063 |
| lib/pcall_err | 95 | 0.125 | 0.104 |
| jit-trace/int_rec2 | 640 | 0.127 | 1.399 |
| programs/pcall_error | 1241 | 0.149 | 0.152 |
| lib/tfront_1k | 76 | 0.188 | 0.070 |
| lib/tonumber_flt | 169 | 0.218 | 0.349 |
| lib/match | 46 | 0.236 | 0.252 |
| lib/it_next | 44 | 0.246 | 0.703 |
| lib/find_pat | 180 | 0.255 | 0.317 |
| corpus/sha2_base64 | 363 | 0.257 | 0.293 |
| apps/markdown | 106 | 0.322 | 0.249 |
| lib/gsub_str | 128 | 0.322 | 0.398 |
| jit-trace/int_sum | 874 | 0.348 | 1.107 |
| lib/tpack | 117 | 0.383 | 0.395 |

## Focus programs (cycles in M; ratios of cycles)

| program | plain Mcyc | PGO Mcyc | LJ Mcyc | PUC Mcyc | plain/LJ | PGO/LJ | plain/PUC | PGO/PUC | plain vs b cyc | ins | PGO vs b cyc | ins | plain vs 7413bff cyc | PGO vs 7413bff cyc | PGO/LJ 5a064d7 |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| programs/nbody | 345.4 | 344.5 | 345.6 | 4246.2 | 0.999 | 0.997 | 0.081 | 0.081 | 1.007 | 0.990 | 0.997 | 0.990 | 1.011 | 1.011 | 0.999 |
| programs/fib | 620.2 | 633.4 | 469.7 | 4139.5 | 1.320 | 1.348 | 0.150 | 0.153 | 0.983 | 0.984 | 1.004 | 0.984 | 0.986 | 1.009 | 1.343 |
| programs/binary_trees | 1155.4 | 999.0 | 2007.4 | 4389.1 | 0.576 | 0.498 | 0.263 | 0.228 | 0.950 | 0.984 | 0.970 | 0.990 | 0.978 | 1.014 | 0.513 |
| programs/queens | 1857.7 | 1859.0 | 1907.1 | 4368.7 | 0.974 | 0.975 | 0.425 | 0.426 | 0.998 | 0.955 | 1.007 | 0.955 | 1.005 | 1.008 | 0.968 |
| apps/raytrace | 96.2 | 91.2 | 40.6 | 262.7 | 2.370 | 2.248 | 0.366 | 0.347 | 0.861 | 0.843 | 0.875 | 0.847 | 0.888 | 0.902 | 2.569 |
| awfy/cd | 252.3 | 233.7 | 107.6 | 437.5 | 2.344 | 2.171 | 0.577 | 0.534 | 0.987 | 0.934 | 0.978 | 0.933 | 0.991 | 0.970 | 2.221 |
| awfy/list | 95.8 | 93.9 | 88.9 | 418.8 | 1.078 | 1.057 | 0.229 | 0.224 | 1.022 | 0.986 | 1.022 | 0.986 | 1.019 | 1.014 | 1.034 |
| awfy/richards | 59.7 | 57.4 | 34.7 | 197.7 | 1.719 | 1.653 | 0.302 | 0.290 | 1.001 | 0.976 | 1.002 | 0.975 | 0.969 | 0.968 | 1.650 |
| awfy/deltablue | 105.7 | 98.7 | 103.5 | 157.9 | 1.022 | 0.954 | 0.669 | 0.625 | 0.988 | 0.979 | 0.991 | 0.988 | 0.991 | 0.997 | 0.963 |
| awfy/havlak | 149.4 | 131.7 | 115.2 | 217.5 | 1.297 | 1.143 | 0.687 | 0.605 | 1.014 | 0.993 | 1.013 | 0.997 | 0.989 | 1.003 | 1.128 |
| awfy/permute | 36.2 | 35.9 | 13.1 | 246.3 | 2.764 | 2.737 | 0.147 | 0.146 | 1.001 | 0.990 | 1.005 | 0.990 | 0.994 | 0.997 | 2.724 |
| awfy/towers | 46.4 | 45.1 | 23.5 | 215.1 | 1.978 | 1.923 | 0.216 | 0.210 | 0.993 | 0.985 | 0.992 | 0.989 | 0.996 | 0.994 | 1.939 |
| corpus/lua_lexer | 821.0 | 722.1 | 448.2 | 3281.3 | 1.832 | 1.611 | 0.250 | 0.220 | 0.761 | 0.756 | 0.753 | 0.752 | 0.841 | 0.832 | 2.138 |
| corpus/classic_vector | 478.7 | 429.3 | 240.7 | 2945.5 | 1.989 | 1.784 | 0.163 | 0.146 | 1.016 | 0.993 | 0.993 | 0.993 | 1.030 | 0.995 | 1.797 |
| corpus2/heldout/dijkstra_routes | 1164.7 | 1136.5 | 1265.1 | 3142.9 | 0.921 | 0.898 | 0.371 | 0.362 | 0.993 | 0.975 | 0.998 | 0.975 | 0.993 | 0.995 | 0.900 |
| corpus2/dev/oop_mixins | 1115.7 | 1049.6 | 865.1 | 2464.3 | 1.290 | 1.213 | 0.453 | 0.426 | 0.800 | 0.720 | 0.773 | 0.708 | 0.994 | 0.980 | 1.570 |
| jit-trace/rec_loop | 80.1 | 79.2 | 141.9 | 1440.3 | 0.565 | 0.558 | 0.056 | 0.055 | 0.869 | 0.832 | 0.867 | 0.832 | 0.984 | 0.990 | 0.643 |
| corpus2/heldout/roguelike_turns | 833.4 | 799.5 | 623.3 | 3311.7 | 1.337 | 1.283 | 0.252 | 0.241 | 1.035 | 1.004 | 1.042 | 1.005 | 0.979 | 0.994 | 1.231 |
| corpus2/heldout/vending_fsm | 979.1 | 903.3 | 824.2 | 3290.3 | 1.188 | 1.096 | 0.298 | 0.275 | 0.961 | 0.974 | 0.958 | 0.974 | 1.001 | 1.009 | 1.144 |
| corpus2/heldout/real_dis_mips | 2114.1 | 1833.8 | 1786.6 | 3808.7 | 1.183 | 1.026 | 0.555 | 0.481 | 0.965 | 0.968 | 0.968 | 0.982 | 0.961 | 0.983 | 1.061 |
| awfy/queens | 95.4 | 93.7 | 55.0 | 401.8 | 1.734 | 1.704 | 0.237 | 0.233 | 0.989 | 0.992 | 1.000 | 0.993 | 0.997 | 1.002 | 1.704 |

## Per-program table (5.5 and 5.4; Mcyc, ratios vs 5a064d7 and LJ)

| program | n runs (plain) | plain | PGO | p54 | g54 | LJ | PUC55 | PUC54 | plain/LJ | PGO/LJ | plain vs b cyc | ins | PGO vs b cyc | ins | p54 vs b cyc | ins | g54 vs b cyc | ins | out/exp (all 4) |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| apps/events | 20 | 161.7 | 141.5 | 177.1 | 151.6 | 95.4 | 293.7 | 289.5 | 1.696 | 1.483 | 0.987 | 1.003 | 1.011 | 1.003 | 1.003 | 1.003 | 0.987 | 1.001 | ok |
| apps/expr_lang | 5 | 132.7 | 119.4 | 146.2 | 129.4 | 96.7 | 241.7 | 253.2 | 1.372 | 1.234 | 0.978 | 0.931 | 0.955 | 0.931 | 0.917 | 0.906 | 0.886 | 0.904 | ok |
| apps/lru_cache | 5 | 143.1 | 132.4 | 180.9 | 169.5 | 106.4 | 219.8 | 253.9 | 1.344 | 1.244 | 1.001 | 0.978 | 1.004 | 0.981 | 1.010 | 0.986 | 1.000 | 0.987 | ok |
| apps/markdown | 5 | 121.3 | 106.2 | 123.0 | 106.6 | 329.7 | 412.6 | 390.8 | 0.368 | 0.322 | 1.040 | 1.009 | 1.045 | 1.023 | 1.019 | 0.997 | 0.995 | 1.005 | ok |
| apps/raytrace | 20 | 96.2 | 91.2 | 105.9 | 99.0 | 40.6 | 262.7 | 281.7 | 2.370 | 2.248 | 0.861 | 0.843 | 0.875 | 0.847 | 0.900 | 0.884 | 0.906 | 0.885 | ok |
| awfy/bounce | 20 | 61.1 | 59.9 | 65.2 | 63.8 | 47.7 | 511.2 | 537.1 | 1.282 | 1.257 | 1.015 | 0.997 | 1.017 | 0.997 | 0.983 | 0.970 | 0.984 | 0.973 | ok |
| awfy/cd | 20 | 252.3 | 233.7 | 280.7 | 260.9 | 107.6 | 437.5 | 464.4 | 2.344 | 2.171 | 0.987 | 0.934 | 0.978 | 0.933 | 0.990 | 0.924 | 0.995 | 0.922 | ok |
| awfy/deltablue | 20 | 105.7 | 98.7 | 113.0 | 105.9 | 103.5 | 157.9 | 165.2 | 1.022 | 0.954 | 0.988 | 0.979 | 0.991 | 0.988 | 0.959 | 0.975 | 0.964 | 0.980 | ok |
| awfy/havlak | 20 | 149.4 | 131.7 | 172.4 | 155.0 | 115.2 | 217.5 | 225.5 | 1.297 | 1.143 | 1.014 | 0.993 | 1.013 | 0.997 | 0.996 | 0.994 | 1.002 | 0.996 | ok |
| awfy/json | 20 | 135.6 | 116.7 | 150.6 | 131.5 | 73.4 | 262.9 | 271.8 | 1.847 | 1.589 | 0.970 | 0.978 | 0.966 | 0.983 | 0.990 | 0.983 | 1.006 | 0.989 | ok |
| awfy/list | 20 | 95.8 | 93.9 | 96.3 | 94.9 | 88.9 | 418.8 | 477.9 | 1.078 | 1.057 | 1.022 | 0.986 | 1.022 | 0.986 | 0.996 | 0.992 | 0.998 | 0.993 | ok |
| awfy/mandelbrot | 2 | 275.0 | 274.2 | 274.3 | 274.0 | 273.1 | 1529.3 | 1379.3 | 1.007 | 1.004 | 1.008 | 1.000 | 1.007 | 1.000 | 1.006 | 1.000 | 1.006 | 1.000 | ok |
| awfy/nbody | 20 | 92.9 | 92.4 | 95.0 | 94.6 | 94.6 | 1504.3 | 1603.8 | 0.982 | 0.977 | 1.001 | 0.992 | 1.010 | 0.992 | 0.995 | 0.993 | 1.000 | 0.993 | ok |
| awfy/permute | 20 | 36.2 | 35.9 | 37.3 | 36.9 | 13.1 | 246.3 | 257.2 | 2.764 | 2.737 | 1.001 | 0.990 | 1.005 | 0.990 | 1.004 | 0.990 | 0.994 | 0.990 | ok |
| awfy/queens | 20 | 95.4 | 93.7 | 82.8 | 80.2 | 55.0 | 401.8 | 403.4 | 1.734 | 1.704 | 0.989 | 0.992 | 1.000 | 0.993 | 1.007 | 0.998 | 1.007 | 0.997 | ok |
| awfy/richards | 20 | 59.7 | 57.4 | 60.0 | 57.8 | 34.7 | 197.7 | 199.3 | 1.719 | 1.653 | 1.001 | 0.976 | 1.002 | 0.975 | 1.004 | 0.976 | 1.005 | 0.974 | ok |
| awfy/sieve | 20 | 113.5 | 112.8 | 115.4 | 115.8 | 156.4 | 1032.3 | 1085.2 | 0.726 | 0.722 | 0.924 | 0.865 | 0.921 | 0.865 | 0.909 | 0.886 | 0.900 | 0.886 | ok |
| awfy/storage | 20 | 102.3 | 78.7 | 146.4 | 115.5 | 154.0 | 255.9 | 262.0 | 0.664 | 0.511 | 1.010 | 1.024 | 1.007 | 1.025 | 1.002 | 1.016 | 1.022 | 1.015 | ok |
| awfy/towers | 20 | 46.4 | 45.1 | 46.3 | 45.2 | 23.5 | 215.1 | 213.9 | 1.978 | 1.923 | 0.993 | 0.985 | 0.992 | 0.989 | 0.965 | 0.985 | 0.976 | 0.989 | ok |
| corpus/astar_jumper | 20 | 1591.6 | 1535.6 | 1660.3 | 1616.6 | 997.4 | 3280.6 | 3358.7 | 1.596 | 1.540 | 0.962 | 0.916 | 0.969 | 0.908 | 0.899 | 0.844 | 0.914 | 0.839 | ok |
| corpus/class_middleclass | 2 | 478.5 | 475.2 | 502.4 | 497.6 | 319.5 | 3046.3 | 3098.9 | 1.498 | 1.487 | 0.998 | 0.998 | 1.016 | 0.999 | 0.990 | 0.998 | 1.006 | 0.998 | ok |
| corpus/classic_vector | 20 | 478.7 | 429.3 | 643.7 | 555.5 | 240.7 | 2945.5 | 2905.4 | 1.989 | 1.784 | 1.016 | 0.993 | 0.993 | 0.993 | 0.991 | 0.995 | 0.987 | 1.008 | ok |
| corpus/containers_lru_heap | 5 | 1292.4 | 1189.9 | 1652.3 | 1544.0 | 1417.3 | 2675.7 | 3035.3 | 0.912 | 0.840 | 1.010 | 0.992 | 0.992 | 0.991 | 0.993 | 0.996 | 0.993 | 0.995 | ok |
| corpus/coro_scheduler | 20 | 1405.1 | 1289.7 | 1506.8 | 1373.0 | 941.9 | 2772.8 | 2851.7 | 1.492 | 1.369 | 0.936 | 0.971 | 0.988 | 1.018 | 0.932 | 0.971 | 0.979 | 1.008 | ok |
| corpus/csv_ini_yaml | 20 | 1461.1 | 1278.2 | 1687.6 | 1487.7 | 1771.4 | 2948.4 | 3020.4 | 0.825 | 0.722 | 0.936 | 0.916 | 0.951 | 0.919 | 0.899 | 0.894 | 0.940 | 0.892 | ok |
| corpus/inspect_lua | 2 | 1561.6 | 1382.3 | 1681.5 | 1531.5 | 1056.4 | 2901.3 | 2968.0 | 1.478 | 1.308 | 0.999 | 0.996 | 0.995 | 0.997 | 1.004 | 1.001 | 1.018 | 1.003 | ok |
| corpus/json_rxi | 20 | 1797.6 | 1550.6 | 1887.5 | 1601.4 | 1222.9 | 3647.9 | 3646.8 | 1.470 | 1.268 | 0.990 | 1.003 | 1.023 | 1.009 | 0.990 | 1.001 | 1.012 | 1.006 | ok |
| corpus/lua_lexer | 20 | 821.0 | 722.1 | 854.6 | 751.9 | 448.2 | 3281.3 | 3358.7 | 1.832 | 1.611 | 0.761 | 0.756 | 0.753 | 0.752 | 0.770 | 0.767 | 0.757 | 0.766 | ok |
| corpus/markdown_conv | 20 | 1355.6 | 1245.1 | 1404.8 | 1268.2 | 3029.8 | 3746.9 | 3671.3 | 0.447 | 0.411 | 0.979 | 0.947 | 1.001 | 0.940 | 1.000 | 0.962 | 1.006 | 0.961 | ok |
| corpus/msgpack_pure | 20 | 2023.3 | 1737.8 | 2204.2 | 1878.1 | 4473.7 | 3436.5 | 3601.3 | 0.452 | 0.388 | 1.005 | 1.002 | 0.997 | 1.001 | 1.014 | 1.003 | 0.999 | 1.000 | ok |
| corpus/mustache_lustache | 20 | 1821.7 | 1462.0 | 1925.2 | 1549.5 | 1537.1 | 3296.2 | 3446.8 | 1.185 | 0.951 | 1.017 | 1.007 | 1.016 | 1.007 | 0.989 | 0.998 | 0.986 | 1.002 | ok |
| corpus/parser_combinator | 20 | 2364.2 | 2001.9 | 2401.9 | 1981.0 | 1563.1 | 3390.6 | 3391.8 | 1.512 | 1.281 | 1.037 | 1.031 | 1.072 | 1.053 | 0.954 | 0.981 | 0.949 | 0.991 | ok |
| corpus/penlight_mix | 5 | 2228.2 | 2113.9 | 2292.4 | 2183.6 | 2678.7 | 3992.1 | 3970.3 | 0.832 | 0.789 | 0.972 | 0.968 | 0.994 | 0.976 | 0.965 | 0.955 | 0.975 | 0.959 | ok |
| corpus/serpent_ser | 5 | 2053.7 | 1785.0 | 2087.0 | 1816.1 | 2426.7 | 3260.6 | 3254.8 | 0.846 | 0.736 | 1.028 | 0.987 | 1.033 | 0.983 | 0.999 | 0.981 | 1.010 | 0.985 | ok |
| corpus/sha2_base64 | 5 | 379.3 | 362.7 | 379.8 | 361.8 | 1411.3 | 3328.6 | 3384.1 | 0.269 | 0.257 | 0.980 | 0.993 | 0.988 | 0.992 | 0.982 | 0.993 | 0.977 | 0.993 | ok |
| corpus/template_etlua | 20 | 1585.4 | 1270.0 | 1574.1 | 1307.7 | 1826.0 | 2680.1 | 2700.4 | 0.868 | 0.696 | 1.006 | 1.009 | 1.028 | 1.009 | 1.004 | 0.996 | 1.025 | 1.004 | ok |
| corpus/testrunner_luaunit | 5 | 1454.3 | 1211.0 | 1633.5 | 1347.6 | 1390.7 | 2601.9 | 2628.8 | 1.046 | 0.871 | 0.987 | 0.970 | 0.990 | 0.969 | 1.010 | 1.004 | 1.010 | 1.006 | ok |
| corpus2/dev/bignum_arith | 5 | 797.1 | 733.6 | 880.5 | 819.5 | 660.2 | 2352.8 | 2412.4 | 1.207 | 1.111 | 1.015 | 1.014 | 1.011 | 1.013 | 1.019 | 1.009 | 1.019 | 1.007 | ok |
| corpus2/dev/coro_generators | 20 | 978.6 | 908.4 | 1006.2 | 936.3 | 713.4 | 1807.9 | 1836.1 | 1.372 | 1.273 | 0.781 | 0.945 | 0.944 | 0.999 | 0.775 | 0.937 | 0.943 | 0.986 | ok |
| corpus2/dev/csv_report | 20 | 1167.6 | 1052.5 | 1166.3 | 1079.5 | 1262.6 | 2165.3 | 2381.4 | 0.925 | 0.834 | 1.032 | 1.018 | 1.039 | 1.020 | 1.006 | 0.971 | 1.028 | 0.973 | ok |
| corpus2/dev/diff_levenshtein | 20 | 420.5 | 414.9 | 439.1 | 412.8 | 329.3 | 1776.3 | 1850.8 | 1.277 | 1.260 | 0.985 | 0.993 | 1.029 | 0.992 | 1.012 | 0.996 | 1.030 | 0.994 | ok |
| corpus2/dev/ecs_world | 20 | 939.6 | 840.8 | 1014.6 | 874.5 | 916.1 | 2115.3 | 2160.1 | 1.026 | 0.918 | 1.000 | 0.996 | 1.004 | 0.999 | 1.021 | 1.031 | 1.018 | 1.034 | ok |
| corpus2/dev/event_promise | 20 | 375.5 | 356.6 | 456.2 | 436.5 | 467.9 | 1906.6 | 1939.2 | 0.803 | 0.762 | 1.009 | 0.984 | 1.013 | 0.986 | 1.006 | 0.990 | 1.010 | 0.989 | ok |
| corpus2/dev/expr_compiler | 20 | 1182.6 | 999.6 | 1231.6 | 1054.8 | 1080.2 | 2081.3 | 2231.0 | 1.095 | 0.925 | 0.896 | 0.925 | 0.888 | 0.910 | 0.911 | 0.920 | 0.903 | 0.906 | ok |
| corpus2/dev/iter_pipeline | 20 | 173.5 | 166.8 | 182.4 | 175.8 | 132.2 | 2865.2 | 2874.5 | 1.313 | 1.262 | 0.999 | 0.989 | 1.009 | 0.992 | 1.003 | 0.990 | 1.005 | 0.991 | ok |
| corpus2/dev/lua_parser | 20 | 1232.6 | 1058.8 | 1277.9 | 1092.3 | 790.7 | 2461.5 | 2499.0 | 1.559 | 1.339 | 0.979 | 0.884 | 0.950 | 0.876 | 0.834 | 0.816 | 0.833 | 0.798 | ok |
| corpus2/dev/memo_cache | 20 | 1586.8 | 1452.3 | 1616.7 | 1501.7 | 1674.7 | 2669.5 | 2763.8 | 0.948 | 0.867 | 0.987 | 0.950 | 0.979 | 0.951 | 0.991 | 0.974 | 0.994 | 0.980 | ok |
| corpus2/dev/oop_mixins | 20 | 1115.7 | 1049.6 | 1124.2 | 1052.2 | 865.1 | 2464.3 | 2520.8 | 1.290 | 1.213 | 0.800 | 0.720 | 0.773 | 0.708 | 0.786 | 0.731 | 0.796 | 0.723 | ok |
| corpus2/dev/particles_physics | 20 | 394.5 | 371.0 | 503.7 | 468.7 | 379.7 | 1719.5 | 1811.4 | 1.039 | 0.977 | 0.983 | 0.991 | 0.995 | 0.993 | 0.996 | 0.992 | 0.989 | 0.993 | ok |
| corpus2/dev/real_dis_arm | 20 | 2459.2 | 2201.3 | 2430.5 | 2167.7 | 1947.2 | 4487.3 | 4484.7 | 1.263 | 1.130 | 1.013 | 1.003 | 1.012 | 1.010 | 0.998 | 1.003 | 1.003 | 1.012 | ok |
| corpus2/dev/real_dis_x86 | 20 | 1571.8 | 1411.3 | 1638.7 | 1471.1 | 1591.6 | 2708.6 | 2688.6 | 0.988 | 0.887 | 1.031 | 1.003 | 1.051 | 1.012 | 1.028 | 1.009 | 1.031 | 1.019 | ok |
| corpus2/dev/regex_nfa | 20 | 692.8 | 565.1 | 1342.5 | 1153.8 | 554.5 | 2510.1 | 2439.3 | 1.249 | 1.019 | 0.946 | 0.997 | 1.003 | 0.996 | 0.961 | 1.000 | 0.954 | 1.000 | ok |
| corpus2/dev/template_gsub | 5 | 1709.1 | 1466.6 | 1762.3 | 1488.6 | 2051.0 | 2892.7 | 2885.0 | 0.833 | 0.715 | 1.002 | 1.001 | 1.043 | 1.008 | 1.003 | 0.999 | 1.002 | 1.007 | ok |
| corpus2/dev/text_adventure | 5 | 2610.6 | 2151.4 | 2661.4 | 2209.1 | 1186.6 | 3375.1 | 3513.0 | 2.200 | 1.813 | 1.021 | 1.003 | 1.004 | 1.015 | 0.996 | 0.997 | 0.997 | 1.010 | ok |
| corpus2/dev/varargs_util | 5 | 1517.1 | 1235.5 | 1544.8 | 1266.0 | 1413.9 | 2284.8 | 2301.9 | 1.073 | 0.874 | 1.010 | 1.007 | 0.996 | 1.010 | 1.007 | 1.003 | 1.004 | 1.007 | ok |
| corpus2/dev/wordfreq_wrap | 20 | 597.6 | 490.4 | 647.7 | 548.4 | 658.1 | 1219.2 | 1225.4 | 0.908 | 0.745 | 0.995 | 1.001 | 0.999 | 1.004 | 1.006 | 1.003 | 1.006 | 1.007 | ok |
| corpus2/heldout/bigint_rsa | 5 | 1130.6 | 952.7 | 1374.6 | 1184.5 | 1074.5 | 3033.1 | 3149.9 | 1.052 | 0.887 | 1.006 | 1.013 | 1.013 | 1.013 | 1.004 | 1.005 | 0.993 | 1.003 | ok |
| corpus2/heldout/dijkstra_routes | 20 | 1164.7 | 1136.5 | 1448.5 | 1415.1 | 1265.1 | 3142.9 | 3226.1 | 0.921 | 0.898 | 0.993 | 0.975 | 0.998 | 0.975 | 0.999 | 0.998 | 1.010 | 0.997 | ok |
| corpus2/heldout/forth_vm | 20 | 808.2 | 804.9 | 1006.4 | 1116.2 | 696.1 | 2965.2 | 3269.3 | 1.161 | 1.156 | 1.008 | 0.995 | 1.006 | 0.995 | 0.958 | 0.960 | 0.957 | 0.959 | ok |
| corpus2/heldout/log_analyzer | 20 | 2139.6 | 1858.0 | 2165.8 | 1839.7 | 1652.2 | 3115.0 | 3178.7 | 1.295 | 1.125 | 1.018 | 0.983 | 1.063 | 0.984 | 1.004 | 0.989 | 0.983 | 0.994 | ok |
| corpus2/heldout/markov_text | 2 | 1128.5 | 1109.2 | 1187.6 | 1140.9 | 982.0 | 2862.9 | 2851.2 | 1.149 | 1.130 | 1.000 | 1.000 | 1.010 | 0.999 | 0.998 | 0.999 | 0.997 | 0.999 | ok |
| corpus2/heldout/prototype_objects | 20 | 1449.7 | 1387.5 | 1463.8 | 1410.9 | 1280.6 | 2532.8 | 2721.4 | 1.132 | 1.083 | 1.048 | 1.007 | 1.072 | 1.010 | 1.066 | 1.003 | 1.069 | 1.000 | ok |
| corpus2/heldout/real_dis_mips | 20 | 2114.1 | 1833.8 | 2151.6 | 1859.1 | 1786.6 | 3808.7 | 3869.0 | 1.183 | 1.026 | 0.965 | 0.968 | 0.968 | 0.982 | 0.987 | 0.983 | 0.995 | 0.999 | ok |
| corpus2/heldout/real_dis_ppc | 20 | 2236.9 | 2014.7 | 2253.5 | 1981.2 | 1747.6 | 3961.9 | 3962.8 | 1.280 | 1.153 | 1.074 | 1.054 | 1.084 | 1.071 | 1.016 | 1.011 | 1.024 | 1.028 | ok |
| corpus2/heldout/roguelike_turns | 17 | 833.4 | 799.5 | 857.4 | 827.9 | 623.3 | 3311.7 | 3473.6 | 1.337 | 1.283 | 1.035 | 1.004 | 1.042 | 1.005 | 1.007 | 1.002 | 1.001 | 1.001 | ok |
| corpus2/heldout/scheme_interp | 17 | 1440.6 | 1235.4 | 1519.0 | 1270.2 | 1045.1 | 3052.1 | 3021.0 | 1.378 | 1.182 | 0.998 | 1.001 | 1.022 | 1.001 | 1.005 | 1.001 | 0.974 | 1.001 | ok |
| corpus2/heldout/stream_closures | 5 | 806.3 | 733.6 | 827.1 | 764.9 | 416.0 | 3032.0 | 3163.1 | 1.938 | 1.764 | 1.026 | 0.981 | 1.017 | 0.983 | 0.973 | 1.127 | 0.981 | 1.138 | ok |
| corpus2/heldout/sudoku_solver | 17 | 1312.6 | 1170.5 | 1640.7 | 1494.4 | 1387.6 | 3013.0 | 3102.7 | 0.946 | 0.844 | 1.006 | 1.000 | 0.997 | 0.999 | 1.011 | 1.001 | 1.013 | 1.000 | ok |
| corpus2/heldout/vending_fsm | 20 | 979.1 | 903.3 | 1018.4 | 943.8 | 824.2 | 3290.3 | 3377.2 | 1.188 | 1.096 | 0.961 | 0.974 | 0.958 | 0.974 | 0.931 | 0.902 | 0.934 | 0.898 | ok |
| jit-trace/alloc_ctor | 20 | 184.4 | 160.0 | 259.0 | 199.4 | 276.5 | 1652.1 | 2197.8 | 0.667 | 0.579 | 0.986 | 0.997 | 1.015 | 0.997 | 1.005 | 0.998 | 0.989 | 0.989 | ok |
| jit-trace/array_rec | 5 | 1795.1 | 1795.4 | 1821.6 | 1822.1 | 1755.9 | 9671.5 | 10022.8 | 1.022 | 1.023 | 1.000 | 0.770 | 1.000 | 0.770 | 1.001 | 0.773 | 1.001 | 0.773 | ok |
| jit-trace/dot | 5 | 1228.4 | 1229.7 | 1229.7 | 1232.1 | 1166.0 | 11124.9 | 11807.8 | 1.054 | 1.055 | 1.009 | 0.700 | 1.008 | 0.700 | 1.006 | 0.701 | 1.007 | 0.701 | ok |
| jit-trace/field_global | 20 | 533.3 | 531.3 | 535.1 | 531.3 | 491.7 | 17948.4 | 17879.3 | 1.085 | 1.081 | 0.980 | 0.901 | 0.992 | 0.908 | 0.991 | 0.901 | 1.001 | 0.901 | ok |
| jit-trace/field_method | 20 | 516.3 | 512.7 | 519.7 | 515.0 | 423.1 | 20185.1 | 20271.4 | 1.220 | 1.212 | 0.910 | 0.936 | 0.911 | 0.936 | 0.910 | 0.935 | 0.911 | 0.937 | ok |
| jit-trace/field_rw | 5 | 954.3 | 954.1 | 1039.4 | 1037.8 | 929.3 | 19574.6 | 19882.6 | 1.027 | 1.027 | 1.006 | 0.932 | 1.006 | 0.932 | 1.005 | 0.939 | 1.005 | 0.940 | ok |
| jit-trace/float_loop | 2 | 2773.3 | 2775.8 | 2801.5 | 2799.2 | 2575.9 | 11381.5 | 13567.0 | 1.077 | 1.078 | 0.997 | 1.000 | 0.999 | 1.000 | 1.000 | 1.000 | 1.002 | 1.000 | ok |
| jit-trace/float_poly | 5 | 1785.6 | 1785.3 | 1826.1 | 1823.6 | 1768.0 | 7918.6 | 8940.5 | 1.010 | 1.010 | 1.002 | 0.881 | 1.002 | 0.882 | 1.004 | 0.888 | 1.003 | 0.887 | ok |
| jit-trace/inline_2 | 20 | 331.7 | 329.8 | 328.9 | 328.5 | 262.5 | 11057.3 | 10600.9 | 1.264 | 1.257 | 1.016 | 0.907 | 1.009 | 0.907 | 1.009 | 0.907 | 1.008 | 0.907 | ok |
| jit-trace/inline_leaf | 20 | 409.9 | 412.8 | 410.6 | 410.1 | 630.2 | 11902.9 | 12019.3 | 0.651 | 0.655 | 0.904 | 0.876 | 0.911 | 0.878 | 0.904 | 0.876 | 0.902 | 0.877 | ok |
| jit-trace/int_rec2 | 20 | 640.0 | 639.6 | 647.3 | 649.0 | 5016.4 | 12493.6 | 13006.3 | 0.128 | 0.127 | 0.945 | 0.902 | 0.946 | 0.902 | 0.945 | 0.902 | 0.949 | 0.902 | ok |
| jit-trace/int_sum | 5 | 876.1 | 873.5 | 900.3 | 900.5 | 2512.8 | 9858.5 | 12300.9 | 0.349 | 0.348 | 0.986 | 0.870 | 0.984 | 0.871 | 0.988 | 0.874 | 0.991 | 0.874 | ok |
| jit-trace/licm_expr | 5 | 466.5 | 465.4 | 470.7 | 470.8 | 442.6 | 12323.9 | 12827.3 | 1.054 | 1.052 | 1.001 | 0.729 | 0.998 | 0.729 | 1.010 | 0.805 | 1.009 | 0.805 | ok |
| jit-trace/multret | 5 | 151.0 | 150.2 | 151.3 | 150.2 | 156.7 | 5879.7 | 5782.8 | 0.964 | 0.959 | 1.002 | 0.966 | 1.003 | 0.966 | 1.008 | 0.966 | 1.004 | 0.966 | ok |
| jit-trace/oop_inherit | 2 | 58.1 | 57.8 | 49.7 | 49.2 | 112.8 | 2153.1 | 2201.0 | 0.515 | 0.513 | 1.012 | 1.000 | 1.014 | 1.000 | 1.017 | 1.000 | 1.014 | 1.000 | ok |
| jit-trace/oop_override | 20 | 106.1 | 105.6 | 110.2 | 110.0 | 136.7 | 645.6 | 666.3 | 0.776 | 0.773 | 0.991 | 0.995 | 0.990 | 0.995 | 0.994 | 0.995 | 0.997 | 0.995 | ok |
| jit-trace/rec_loop | 20 | 80.1 | 79.2 | 82.8 | 81.6 | 141.9 | 1440.3 | 1449.9 | 0.565 | 0.558 | 0.869 | 0.832 | 0.867 | 0.832 | 0.933 | 0.834 | 0.928 | 0.834 | ok |
| jit-trace/short_inner | 2 | 1105.8 | 1106.3 | 999.2 | 998.3 | 878.8 | 10393.4 | 11908.6 | 1.258 | 1.259 | 1.004 | 1.000 | 1.005 | 1.000 | 1.005 | 0.999 | 1.006 | 1.000 | ok |
| jit-trace/sink_tuple | 5 | 13.5 | 13.0 | 13.1 | 13.1 | 20.0 | 2743.7 | 3358.1 | 0.676 | 0.654 | 1.019 | 0.953 | 1.018 | 0.954 | 1.000 | 0.952 | 1.020 | 0.953 | ok |
| jit-trace/sink_vec | 5 | 15.4 | 15.2 | 15.4 | 15.2 | 20.0 | 1489.0 | 1832.3 | 0.771 | 0.760 | 1.002 | 0.910 | 1.018 | 0.911 | 1.015 | 0.910 | 1.020 | 0.910 | ok |
| jit-trace/str_concat | 17 | 529.6 | 510.1 | 600.4 | 557.0 | 501.6 | 1628.5 | 1630.2 | 1.056 | 1.017 | 0.997 | 1.000 | 1.001 | 1.000 | 1.010 | 1.000 | 1.004 | 1.003 | ok |
| jit-trace/tail_loop | 20 | 248.3 | 247.0 | 253.1 | 252.0 | 223.3 | 6724.0 | 6643.8 | 1.112 | 1.106 | 0.998 | 0.958 | 0.994 | 0.957 | 1.010 | 0.958 | 1.000 | 0.958 | ok |
| jit-trace/tfor_loops | 5 | 230.7 | 229.7 | 231.3 | 229.4 | 362.3 | 3623.0 | 3352.6 | 0.637 | 0.634 | 1.062 | 0.989 | 1.043 | 0.989 | 1.012 | 0.989 | 1.025 | 0.989 | ok |
| jit-trace/type_entry | 5 | 1437.8 | 1443.0 | 1580.2 | 1583.6 | 848.7 | 11725.8 | 12234.3 | 1.694 | 1.700 | 0.949 | 0.786 | 0.954 | 0.786 | 0.993 | 0.847 | 1.004 | 0.847 | ok |
| jit-trace/type_entry_stable | 5 | 1385.5 | 1380.1 | 1530.1 | 1527.0 | 856.9 | 11690.0 | 12248.4 | 1.617 | 1.611 | 0.955 | 0.774 | 0.952 | 0.774 | 1.014 | 0.838 | 1.003 | 0.838 | ok |
| jit-trace/type_late | 5 | 1530.3 | 1539.4 | 1549.9 | 1552.6 | 1461.5 | 10901.2 | 11418.6 | 1.047 | 1.053 | 1.006 | 0.803 | 1.011 | 0.803 | 1.013 | 0.804 | 1.014 | 0.804 | ok |
| jit-trace/type_late_stable | 5 | 1518.3 | 1520.2 | 1544.3 | 1539.8 | 1461.0 | 10905.6 | 11410.8 | 1.039 | 1.041 | 0.998 | 0.727 | 1.001 | 0.727 | 1.008 | 0.804 | 1.007 | 0.804 | ok |
| jit-trace/upval_acc | 20 | 1010.3 | 1017.6 | 1395.0 | 1391.0 | 1356.5 | 9970.0 | 10500.8 | 0.745 | 0.750 | 0.731 | 0.812 | 0.736 | 0.812 | 1.008 | 0.960 | 1.004 | 0.960 | ok |
| jit-trace/upval_inv | 5 | 2118.7 | 2118.2 | 2137.5 | 2137.8 | 1985.3 | 13886.5 | 14901.3 | 1.067 | 1.067 | 0.993 | 0.710 | 0.993 | 0.710 | 0.998 | 0.845 | 0.999 | 0.845 | ok |
| lib/assert_ok | 5 | 17.0 | 16.7 | 19.1 | 19.2 | 23.7 | 295.9 | 329.8 | 0.719 | 0.706 | 0.899 | 0.779 | 0.896 | 0.779 | 0.943 | 0.851 | 0.942 | 0.851 | ok |
| lib/byte | 20 | 47.9 | 47.7 | 48.0 | 47.4 | 33.0 | 776.0 | 845.1 | 1.449 | 1.444 | 0.951 | 0.952 | 0.956 | 0.952 | 0.943 | 0.952 | 0.941 | 0.952 | ok |
| lib/byte_multi | 20 | 40.3 | 39.9 | 40.4 | 39.7 | 20.1 | 336.9 | 366.2 | 2.003 | 1.985 | 0.982 | 0.972 | 0.967 | 0.972 | 0.985 | 0.972 | 0.974 | 0.972 | ok |
| lib/char | 20 | 153.3 | 139.2 | 157.0 | 147.0 | 104.8 | 383.6 | 387.1 | 1.463 | 1.328 | 1.015 | 1.000 | 1.006 | 1.000 | 1.010 | 1.000 | 1.024 | 1.000 | ok |
| lib/co_wrap | 20 | 227.4 | 192.8 | 226.8 | 186.7 | 228.7 | 381.5 | 338.9 | 0.994 | 0.843 | 0.942 | 0.954 | 0.969 | 0.990 | 0.944 | 0.955 | 0.956 | 1.001 | ok |
| lib/co_yield | 20 | 182.5 | 172.8 | 175.2 | 171.7 | 102.0 | 483.9 | 475.0 | 1.789 | 1.694 | 0.765 | 0.919 | 0.958 | 1.008 | 0.725 | 0.919 | 0.946 | 1.009 | ok |
| lib/concat_op | 5 | 140.3 | 129.6 | 144.3 | 135.4 | 212.9 | 419.4 | 445.3 | 0.659 | 0.609 | 1.001 | 1.000 | 1.003 | 1.000 | 1.014 | 1.000 | 1.015 | 1.005 | ok |
| lib/concat_op_str | 2 | 142.3 | 125.8 | 241.1 | 225.4 | 276.8 | 436.1 | 336.3 | 0.514 | 0.455 | 1.056 | 1.000 | 1.012 | 1.003 | 0.989 | 1.000 | 0.978 | 1.001 | ok |
| lib/error_tbl | 20 | 277.1 | 211.1 | 268.3 | 202.7 | 3042.9 | 619.9 | 625.1 | 0.091 | 0.069 | 1.041 | 1.008 | 0.958 | 1.009 | 1.014 | 1.010 | 0.936 | 1.005 | ok |
| lib/find_cap | 5 | 262.0 | 230.8 | 260.6 | 224.9 | 183.4 | 365.9 | 363.4 | 1.428 | 1.258 | 0.951 | 1.003 | 0.989 | 1.003 | 0.941 | 0.998 | 0.990 | 1.007 | ok |
| lib/find_pat | 20 | 202.8 | 179.6 | 208.0 | 184.8 | 705.2 | 1094.0 | 1092.2 | 0.288 | 0.255 | 0.789 | 0.795 | 0.786 | 0.766 | 0.768 | 0.799 | 0.783 | 0.770 | ok |
| lib/find_plain | 20 | 124.9 | 114.9 | 124.3 | 114.7 | 67.6 | 342.8 | 363.4 | 1.848 | 1.701 | 0.979 | 0.986 | 0.985 | 0.973 | 0.984 | 0.986 | 1.001 | 0.984 | ok |
| lib/fmt_d | 5 | 94.7 | 88.8 | 101.0 | 95.1 | 126.3 | 278.1 | 235.1 | 0.749 | 0.703 | 1.017 | 0.998 | 0.989 | 0.989 | 1.022 | 0.999 | 0.999 | 1.001 | ok |
| lib/fmt_f | 20 | 62.0 | 57.4 | 63.9 | 59.6 | 86.8 | 215.0 | 196.2 | 0.714 | 0.661 | 0.989 | 1.004 | 0.999 | 1.014 | 0.981 | 0.982 | 1.000 | 1.016 | ok |
| lib/fmt_g | 20 | 71.9 | 61.7 | 73.8 | 64.1 | 67.3 | 204.6 | 179.3 | 1.067 | 0.916 | 0.982 | 1.004 | 0.979 | 1.013 | 0.979 | 0.984 | 1.001 | 1.014 | ok |
| lib/fmt_mixed | 20 | 59.7 | 49.4 | 65.0 | 55.8 | 106.2 | 254.5 | 238.3 | 0.562 | 0.465 | 1.004 | 1.003 | 0.984 | 1.012 | 0.967 | 0.986 | 0.987 | 1.011 | ok |
| lib/fmt_q | 20 | 143.9 | 119.4 | 161.7 | 121.3 | 103.3 | 238.6 | 226.1 | 1.394 | 1.156 | 1.004 | 1.003 | 0.985 | 1.008 | 1.103 | 0.988 | 0.997 | 1.007 | ok |
| lib/fmt_s | 5 | 7.6 | 7.1 | 7.6 | 7.4 | 3.2 | 217.6 | 222.2 | 2.406 | 2.241 | 1.041 | 0.942 | 1.033 | 0.942 | 1.015 | 0.936 | 1.064 | 0.941 | ok |
| lib/fmt_x_pad | 20 | 57.2 | 48.5 | 60.0 | 51.7 | 54.0 | 177.1 | 177.5 | 1.060 | 0.898 | 1.015 | 1.016 | 1.015 | 1.007 | 1.017 | 0.999 | 1.016 | 1.000 | ok |
| lib/getmt | 20 | 15.6 | 15.3 | 20.0 | 20.0 | 8.8 | 255.7 | 264.8 | 1.770 | 1.733 | 0.973 | 0.949 | 0.979 | 0.950 | 1.008 | 0.979 | 1.028 | 0.980 | ok |
| lib/gmatch_words | 20 | 176.7 | 137.6 | 180.4 | 139.3 | 173.3 | 315.9 | 319.8 | 1.020 | 0.794 | 1.005 | 1.001 | 1.013 | 0.986 | 1.022 | 1.002 | 1.017 | 0.991 | ok |
| lib/gsub_fn | 20 | 177.9 | 125.2 | 177.7 | 128.3 | 200.2 | 323.4 | 312.5 | 0.888 | 0.625 | 1.018 | 1.001 | 0.973 | 0.965 | 0.993 | 0.993 | 0.972 | 0.966 | ok |
| lib/gsub_str | 20 | 164.9 | 128.5 | 161.9 | 129.3 | 398.5 | 491.6 | 515.6 | 0.414 | 0.322 | 0.998 | 1.002 | 1.016 | 1.008 | 0.973 | 0.987 | 1.025 | 1.008 | ok |
| lib/gsub_tbl | 20 | 165.2 | 107.5 | 165.3 | 108.2 | 187.9 | 321.9 | 321.7 | 0.880 | 0.572 | 1.012 | 1.002 | 0.993 | 0.992 | 1.087 | 0.988 | 0.989 | 0.995 | ok |
| lib/it_ipairs | 5 | 235.1 | 234.8 | 272.1 | 253.2 | 251.6 | 5628.0 | 4827.6 | 0.935 | 0.933 | 0.987 | 0.957 | 1.004 | 0.957 | 0.975 | 0.964 | 0.974 | 0.961 | ok |
| lib/it_next | 20 | 45.1 | 44.5 | 44.8 | 44.7 | 180.5 | 371.1 | 401.4 | 0.250 | 0.246 | 1.020 | 1.000 | 1.012 | 1.000 | 1.017 | 1.000 | 1.023 | 1.000 | ok |
| lib/it_next_empty | 20 | 52.3 | 52.2 | 52.7 | 52.7 | 72.5 | 393.0 | 455.4 | 0.721 | 0.720 | 1.010 | 0.966 | 1.018 | 0.967 | 1.017 | 0.984 | 1.024 | 0.984 | ok |
| lib/it_pairs | 20 | 89.6 | 88.3 | 95.0 | 91.5 | 127.9 | 687.0 | 646.3 | 0.701 | 0.691 | 1.025 | 1.000 | 1.011 | 1.000 | 1.023 | 1.000 | 1.023 | 0.999 | ok |
| lib/it_pairs_hash | 5 | 103.4 | 104.5 | 114.0 | 106.8 | 118.1 | 989.3 | 957.1 | 0.875 | 0.885 | 1.000 | 1.000 | 1.021 | 1.000 | 1.011 | 1.000 | 0.994 | 1.000 | ok |
| lib/len_op | 20 | 41.8 | 42.8 | 58.2 | 57.3 | 100.7 | 1384.7 | 1419.8 | 0.415 | 0.425 | 0.873 | 0.920 | 0.903 | 0.920 | 1.015 | 0.961 | 1.004 | 0.961 | ok |
| lib/len_tbl | 20 | 143.0 | 142.5 | 149.9 | 149.3 | 159.7 | 372.6 | 337.9 | 0.895 | 0.892 | 1.042 | 0.987 | 1.054 | 0.987 | 0.953 | 1.000 | 0.951 | 1.000 | ok |
| lib/load_small | 20 | 227.3 | 153.9 | 220.0 | 167.5 | 215.6 | 378.8 | 345.2 | 1.054 | 0.714 | 0.951 | 1.002 | 1.011 | 1.009 | 0.984 | 0.988 | 1.057 | 0.992 | ok |
| lib/lower | 2 | 90.5 | 82.4 | 91.2 | 85.4 | 110.1 | 289.0 | 294.8 | 0.822 | 0.748 | 1.025 | 1.000 | 1.020 | 1.000 | 1.006 | 1.000 | 1.037 | 1.000 | ok |
| lib/m_abs | 20 | 246.6 | 246.0 | 248.7 | 247.6 | 390.3 | 6061.9 | 6506.8 | 0.632 | 0.630 | 0.995 | 1.000 | 1.006 | 1.000 | 1.013 | 1.000 | 1.018 | 1.000 | ok |
| lib/m_floor | 20 | 226.5 | 226.2 | 227.9 | 227.5 | 206.5 | 3681.1 | 4104.1 | 1.096 | 1.095 | 1.002 | 1.000 | 1.002 | 1.000 | 1.005 | 1.000 | 1.006 | 1.000 | ok |
| lib/m_fmod | 20 | 219.1 | 221.0 | 221.2 | 229.4 | 235.1 | 329.2 | 361.2 | 0.932 | 0.940 | 1.001 | 1.020 | 1.015 | 1.019 | 1.000 | 1.020 | 1.017 | 1.015 | ok |
| lib/m_max | 2 | 256.7 | 259.2 | 260.6 | 256.7 | 401.3 | 8004.8 | 10137.5 | 0.640 | 0.646 | 1.003 | 1.000 | 1.014 | 1.000 | 1.015 | 1.000 | 1.002 | 1.000 | ok |
| lib/m_min | 20 | 203.9 | 202.6 | 201.7 | 201.1 | 387.7 | 6530.7 | 8066.5 | 0.526 | 0.523 | 0.931 | 0.861 | 0.898 | 0.861 | 0.909 | 0.861 | 0.917 | 0.861 | ok |
| lib/m_pow_op | 5 | 284.5 | 283.9 | 283.8 | 283.8 | 275.2 | 1923.5 | 1956.0 | 1.034 | 1.032 | 1.014 | 0.870 | 1.015 | 0.870 | 1.011 | 0.870 | 1.014 | 0.870 | ok |
| lib/m_random | 2 | 79.6 | 78.9 | 79.3 | 78.6 | 73.2 | 345.7 | 375.3 | 1.086 | 1.077 | 1.006 | 1.000 | 1.006 | 1.000 | 1.007 | 1.000 | 1.004 | 1.000 | ok |
| lib/m_random_f | 20 | 53.5 | 52.9 | 53.1 | 52.8 | 89.9 | 272.7 | 312.0 | 0.595 | 0.588 | 1.014 | 0.974 | 1.012 | 0.975 | 1.007 | 0.974 | 1.005 | 0.975 | ok |
| lib/m_sin | 20 | 269.4 | 258.1 | 269.2 | 258.1 | 210.5 | 563.9 | 589.3 | 1.280 | 1.226 | 0.992 | 0.988 | 0.992 | 0.987 | 0.992 | 0.988 | 0.991 | 0.987 | ok |
| lib/m_sqrt | 5 | 277.6 | 279.5 | 277.8 | 272.8 | 270.0 | 2022.6 | 2132.3 | 1.028 | 1.035 | 0.999 | 0.871 | 1.013 | 0.871 | 0.960 | 0.871 | 0.954 | 0.871 | ok |
| lib/m_tointeger | 20 | 223.4 | 223.4 | 224.5 | 236.8 | - | 3718.4 | 3972.3 | - | - | 1.002 | 1.000 | 1.003 | 1.000 | 1.005 | 1.000 | 1.002 | 1.000 | ok |
| lib/m_type | 2 | 32.2 | 32.0 | 32.3 | 32.2 | - | 419.3 | 429.1 | - | - | 1.010 | 1.000 | 1.014 | 1.001 | 1.009 | 1.000 | 1.017 | 1.000 | ok |
| lib/m_ult | 5 | 276.7 | 282.3 | 279.9 | 289.6 | - | 415.2 | 435.8 | - | - | 1.009 | 1.017 | 1.002 | 1.017 | 1.015 | 1.017 | 1.018 | 1.013 | ok |
| lib/match | 20 | 55.3 | 45.7 | 55.6 | 45.5 | 193.6 | 319.9 | 303.8 | 0.285 | 0.236 | 0.900 | 0.886 | 0.899 | 0.895 | 0.915 | 0.880 | 0.883 | 0.886 | ok |
| lib/os_clock | 20 | 114.4 | 120.5 | 119.5 | 120.9 | 100.7 | 119.3 | 118.5 | 1.136 | 1.197 | 0.996 | 1.018 | 0.996 | 1.017 | 1.008 | 1.018 | 0.998 | 1.014 | ok |
| lib/pcall_err | 20 | 119.8 | 95.2 | 124.8 | 101.6 | 764.5 | 308.4 | 309.0 | 0.157 | 0.125 | 0.986 | 1.005 | 0.955 | 1.008 | 0.947 | 0.997 | 0.920 | 1.005 | ok |
| lib/pcall_ok | 20 | 27.6 | 27.3 | 27.4 | 27.2 | 41.0 | 1524.0 | 1518.3 | 0.673 | 0.666 | 0.914 | 0.875 | 0.905 | 0.875 | 0.904 | 0.875 | 0.911 | 0.875 | ok |
| lib/rawget_set | 5 | 18.2 | 17.7 | 18.3 | 17.9 | 14.2 | 381.1 | 417.2 | 1.281 | 1.246 | 0.958 | 0.905 | 0.936 | 0.906 | 0.969 | 0.906 | 0.962 | 0.906 | ok |
| lib/rep | 5 | 108.7 | 99.5 | 118.0 | 109.8 | 89.6 | 347.4 | 333.3 | 1.214 | 1.110 | 1.009 | 0.999 | 0.996 | 0.999 | 0.973 | 1.000 | 0.995 | 1.002 | ok |
| lib/rep_sep | 2 | 89.5 | 81.3 | 116.6 | 109.6 | 103.8 | 329.6 | 311.2 | 0.862 | 0.784 | 1.014 | 0.999 | 1.006 | 0.999 | 0.995 | 1.000 | 1.014 | 1.000 | ok |
| lib/reverse | 2 | 98.5 | 93.3 | 103.1 | 99.1 | 115.3 | 309.8 | 314.7 | 0.854 | 0.809 | 0.989 | 1.000 | 0.999 | 1.000 | 0.998 | 1.000 | 1.012 | 1.000 | ok |
| lib/sel_count | 20 | 217.9 | 213.3 | 220.1 | 218.5 | 388.2 | 15509.5 | 14438.4 | 0.561 | 0.549 | 1.006 | 1.000 | 0.987 | 1.000 | 1.015 | 1.000 | 0.999 | 1.000 | ok |
| lib/sel_loop | 20 | 209.1 | 209.6 | 211.9 | 211.6 | 5263.6 | 53689.2 | 54662.9 | 0.040 | 0.040 | 0.992 | 0.929 | 1.001 | 0.929 | 0.992 | 0.930 | 0.999 | 0.930 | ok |
| lib/sel_n | 20 | 336.9 | 336.7 | 333.7 | 333.1 | 675.4 | 25345.4 | 23674.6 | 0.499 | 0.499 | 1.003 | 1.000 | 1.045 | 1.000 | 1.020 | 1.000 | 1.031 | 1.000 | ok |
| lib/setmt | 20 | 188.4 | 166.6 | 225.1 | 190.4 | 331.9 | 743.5 | 835.9 | 0.568 | 0.502 | 1.025 | 1.000 | 1.047 | 1.000 | 0.987 | 0.999 | 0.995 | 0.992 | ok |
| lib/str_eq | 20 | 209.9 | 209.8 | 209.8 | 209.6 | 182.5 | 1479.5 | 1529.4 | 1.150 | 1.150 | 1.312 | 0.922 | 1.312 | 0.922 | 1.308 | 0.922 | 1.312 | 0.922 | ok |
| lib/str_eq_dyn | 20 | 132.1 | 131.7 | 132.3 | 131.9 | 150.4 | 1277.3 | 1348.8 | 0.878 | 0.876 | 0.978 | 0.860 | 0.991 | 0.860 | 0.989 | 0.860 | 0.992 | 0.860 | ok |
| lib/str_eq_long | 20 | 36.8 | 36.3 | 35.8 | 35.8 | 21.5 | 215.5 | 222.1 | 1.715 | 1.691 | 1.032 | 0.961 | 1.036 | 0.961 | 1.003 | 0.961 | 1.011 | 0.961 | ok |
| lib/str_idx_concat_key | 5 | 204.9 | 190.3 | 206.7 | 192.5 | 166.6 | 464.0 | 461.2 | 1.230 | 1.143 | 0.998 | 1.000 | 0.999 | 1.000 | 1.004 | 1.000 | 0.999 | 1.000 | ok |
| lib/str_lt | 5 | 90.2 | 86.6 | 89.8 | 86.9 | 66.3 | 227.1 | 228.3 | 1.360 | 1.305 | 0.966 | 0.985 | 0.951 | 0.984 | 1.008 | 0.985 | 1.006 | 0.984 | ok |
| lib/str_lt_short | 20 | 101.3 | 96.4 | 106.3 | 99.4 | 62.5 | 249.8 | 255.3 | 1.621 | 1.541 | 0.892 | 0.963 | 0.907 | 0.962 | 1.016 | 0.988 | 1.013 | 0.987 | ok |
| lib/str_method_len | 20 | 43.8 | 43.6 | 50.9 | 50.6 | 55.3 | 1386.6 | 1429.7 | 0.792 | 0.788 | 0.962 | 0.904 | 0.962 | 0.904 | 0.999 | 0.956 | 0.990 | 0.956 | ok |
| lib/string_key_const | 5 | 173.8 | 173.5 | 202.6 | 203.0 | 176.9 | 1151.6 | 1165.1 | 0.982 | 0.981 | 0.983 | 0.874 | 0.985 | 0.874 | 1.013 | 0.977 | 1.017 | 0.977 | ok |
| lib/string_key_dyn | 5 | 228.7 | 228.2 | 229.7 | 229.2 | 268.0 | 1082.3 | 1101.0 | 0.853 | 0.851 | 0.997 | 0.983 | 0.998 | 0.983 | 1.006 | 0.983 | 1.004 | 0.983 | ok |
| lib/sub | 20 | 130.7 | 121.4 | 131.3 | 121.0 | 80.5 | 469.5 | 482.7 | 1.623 | 1.509 | 0.994 | 0.985 | 0.951 | 0.984 | 1.002 | 0.985 | 0.972 | 0.984 | ok |
| lib/sub_1char | 20 | 117.5 | 108.2 | 119.8 | 107.8 | 98.2 | 459.2 | 478.8 | 1.196 | 1.102 | 0.978 | 0.979 | 0.989 | 0.978 | 0.986 | 0.980 | 0.994 | 0.978 | ok |
| lib/tconcat | 20 | 144.4 | 100.7 | 167.3 | 118.5 | 133.4 | 346.9 | 322.3 | 1.083 | 0.755 | 1.000 | 1.001 | 1.009 | 1.005 | 0.988 | 0.993 | 0.984 | 1.003 | ok |
| lib/tconcat_num | 5 | 171.6 | 118.3 | 179.0 | 121.0 | 246.0 | 908.0 | 844.8 | 0.698 | 0.481 | 0.959 | 1.001 | 0.990 | 1.006 | 0.966 | 0.994 | 0.990 | 1.006 | ok |
| lib/tfront_1k | 5 | 78.5 | 76.4 | 81.7 | 77.1 | 406.3 | 4375.7 | 4241.0 | 0.193 | 0.188 | 0.994 | 1.005 | 1.003 | 1.017 | 0.999 | 1.005 | 1.014 | 1.015 | ok |
| lib/tinsert_append | 5 | 161.4 | 134.5 | 196.4 | 186.6 | 141.7 | 539.7 | 574.7 | 1.138 | 0.949 | 1.012 | 0.999 | 1.011 | 1.007 | 1.023 | 1.038 | 1.031 | 1.037 | ok |
| lib/tinsert_front | 20 | 223.6 | 179.9 | 262.4 | 210.6 | 272.9 | 455.4 | 493.5 | 0.819 | 0.659 | 0.999 | 1.003 | 1.009 | 1.010 | 0.999 | 1.002 | 1.007 | 1.005 | ok |
| lib/tinsert_idx | 20 | 184.6 | 155.2 | 208.9 | 197.7 | 154.6 | 328.8 | 383.3 | 1.194 | 1.004 | 1.051 | 1.047 | 1.020 | 1.046 | 1.018 | 1.040 | 1.042 | 1.038 | ok |
| lib/tmove | 2 | 103.9 | 103.3 | 123.8 | 123.8 | 105.8 | 700.5 | 683.1 | 0.983 | 0.977 | 1.009 | 1.000 | 1.006 | 1.000 | 0.994 | 1.000 | 0.991 | 1.000 | ok |
| lib/tmove_1k | 2 | 96.2 | 69.4 | 106.5 | 72.3 | 111.6 | 476.4 | 447.3 | 0.862 | 0.622 | 0.955 | 0.998 | 1.015 | 1.002 | 0.961 | 1.000 | 0.969 | 0.997 | ok |
| lib/tonumber_bad | 20 | 151.9 | 119.6 | 165.4 | 143.2 | 168.0 | 408.1 | 431.1 | 0.904 | 0.712 | 0.975 | 0.993 | 0.989 | 0.993 | 0.992 | 0.997 | 0.997 | 0.996 | ok |
| lib/tonumber_base | 20 | 268.4 | 266.9 | 274.1 | 274.1 | 229.2 | 432.1 | 451.7 | 1.171 | 1.164 | 1.012 | 1.011 | 1.026 | 1.030 | 1.009 | 1.011 | 1.039 | 1.027 | ok |
| lib/tonumber_flt | 17 | 195.2 | 169.4 | 202.4 | 167.3 | 776.7 | 577.4 | 596.1 | 0.251 | 0.218 | 1.004 | 0.997 | 0.988 | 0.998 | 1.027 | 0.998 | 0.998 | 0.998 | ok |
| lib/tonumber_int | 5 | 154.7 | 137.5 | 162.6 | 143.1 | 96.2 | 288.1 | 305.0 | 1.608 | 1.430 | 0.998 | 0.991 | 0.977 | 0.981 | 1.014 | 0.996 | 1.007 | 0.995 | ok |
| lib/tonumber_num | 20 | 168.5 | 168.7 | 168.3 | 167.2 | 383.1 | 6476.7 | 7506.3 | 0.440 | 0.440 | 0.784 | 0.839 | 0.797 | 0.839 | 0.773 | 0.839 | 0.790 | 0.839 | ok |
| lib/tostring_flt | 20 | 83.3 | 66.9 | 84.3 | 61.2 | 82.5 | 299.3 | 214.9 | 1.010 | 0.811 | 1.007 | 1.001 | 1.016 | 0.996 | 1.010 | 1.000 | 1.001 | 1.002 | ok |
| lib/tostring_int | 20 | 122.0 | 108.8 | 137.0 | 123.5 | 279.8 | 543.3 | 450.7 | 0.436 | 0.389 | 1.000 | 1.000 | 0.998 | 0.994 | 1.017 | 1.000 | 1.009 | 1.003 | ok |
| lib/tostring_tbl | 5 | 109.3 | 108.3 | 110.1 | 107.8 | 93.3 | 313.8 | 280.9 | 1.171 | 1.160 | 1.011 | 1.006 | 1.000 | 1.018 | 0.986 | 0.985 | 1.002 | 1.016 | ok |
| lib/tpack | 5 | 148.6 | 117.2 | 209.6 | 169.3 | 306.1 | 499.0 | 485.7 | 0.486 | 0.383 | 0.999 | 1.000 | 0.996 | 1.000 | 0.993 | 0.999 | 1.009 | 0.998 | ok |
| lib/tremove_front | 2 | 147.8 | 132.7 | 184.9 | 169.6 | 191.0 | 592.3 | 593.7 | 0.774 | 0.695 | 1.002 | 1.000 | 1.000 | 1.000 | 0.990 | 1.000 | 0.989 | 0.996 | ok |
| lib/tremove_pop | 20 | 244.4 | 178.0 | 227.4 | 209.3 | 136.7 | 483.9 | 552.9 | 1.787 | 1.302 | 1.016 | 1.007 | 1.031 | 1.027 | 1.014 | 1.003 | 0.998 | 1.009 | ok |
| lib/tsort_cmp | 20 | 261.1 | 245.2 | 279.2 | 248.2 | 285.5 | 339.9 | 336.1 | 0.914 | 0.859 | 0.984 | 1.001 | 0.953 | 0.941 | 1.019 | 0.999 | 0.952 | 0.937 | ok |
| lib/tsort_int | 5 | 268.6 | 216.9 | 290.8 | 223.5 | 294.0 | 382.4 | 406.0 | 0.913 | 0.738 | 1.001 | 0.993 | 1.010 | 0.999 | 0.993 | 0.998 | 0.998 | 0.997 | ok |
| lib/tsort_str | 20 | 260.0 | 151.8 | 296.0 | 160.3 | 244.8 | 364.6 | 401.0 | 1.062 | 0.620 | 1.014 | 0.989 | 0.997 | 1.003 | 1.011 | 1.000 | 1.001 | 1.000 | ok |
| lib/tunpack | 20 | 122.3 | 103.6 | 127.9 | 111.8 | 154.8 | 487.1 | 448.5 | 0.790 | 0.669 | 0.985 | 1.000 | 1.001 | 1.000 | 1.007 | 1.000 | 0.996 | 0.993 | ok |
| lib/type_call | 20 | 90.4 | 89.6 | 97.6 | 96.8 | 85.6 | 494.3 | 468.3 | 1.056 | 1.047 | 1.042 | 0.999 | 1.034 | 1.000 | 1.041 | 0.989 | 1.035 | 0.989 | ok |
| lib/upper | 17 | 97.7 | 92.6 | 97.9 | 91.8 | 120.7 | 307.4 | 312.6 | 0.809 | 0.767 | 1.018 | 1.000 | 1.020 | 1.000 | 0.979 | 1.000 | 0.968 | 1.000 | ok |
| lib/utf8_len | 20 | 276.5 | 278.6 | 277.7 | 288.0 | - | 442.2 | 456.7 | - | - | 1.017 | 1.011 | 1.014 | 1.011 | 1.013 | 1.011 | 1.030 | 1.008 | ok |
| programs/binary_trees | 20 | 1155.4 | 999.0 | 1437.8 | 1190.7 | 2007.4 | 4389.1 | 4447.3 | 0.576 | 0.498 | 0.950 | 0.984 | 0.970 | 0.990 | 1.004 | 1.008 | 1.023 | 0.998 | ok |
| programs/closures | 20 | 463.3 | 420.7 | 531.3 | 471.9 | 949.8 | 5063.2 | 4986.1 | 0.488 | 0.443 | 1.005 | 0.989 | 0.996 | 0.983 | 1.038 | 0.985 | 0.984 | 0.979 | ok |
| programs/coroutines | 20 | 1441.5 | 1303.6 | 1392.0 | 1322.0 | 871.7 | 3733.4 | 3656.9 | 1.654 | 1.495 | 0.800 | 0.930 | 1.003 | 1.034 | 0.750 | 0.934 | 0.935 | 1.035 | ok |
| programs/fannkuch | 20 | 1023.7 | 1026.1 | 1060.5 | 1067.2 | 1075.7 | 4075.1 | 4114.8 | 0.952 | 0.954 | 0.976 | 0.966 | 0.987 | 0.966 | 0.995 | 0.973 | 0.996 | 0.973 | ok |
| programs/fasta | 20 | 1274.9 | 1154.1 | 1364.2 | 1236.0 | 1495.7 | 3776.8 | 3842.3 | 0.852 | 0.772 | 1.002 | 1.000 | 1.000 | 1.000 | 1.009 | 0.999 | 1.005 | 1.000 | ok |
| programs/fib | 20 | 620.2 | 633.4 | 622.6 | 627.7 | 469.7 | 4139.5 | 4150.5 | 1.320 | 1.348 | 0.983 | 0.984 | 1.004 | 0.984 | 0.988 | 0.984 | 0.999 | 0.984 | ok |
| programs/gc_churn | 20 | 2185.8 | 1852.7 | 3229.4 | 2802.8 | 3608.7 | 4605.5 | 5988.9 | 0.606 | 0.513 | 1.024 | 1.002 | 1.048 | 1.007 | 1.006 | 1.000 | 1.015 | 1.002 | ok |
| programs/heapsort | 20 | 1058.5 | 1055.1 | 2195.2 | 2198.7 | 1286.6 | 4063.2 | 3957.0 | 0.823 | 0.820 | 1.011 | 0.952 | 1.006 | 0.952 | 0.992 | 0.994 | 0.996 | 0.995 | ok |
| programs/json | 5 | 1865.3 | 1397.4 | 1966.3 | 1514.3 | 1470.3 | 3127.6 | 3229.9 | 1.269 | 0.950 | 1.028 | 0.950 | 0.973 | 0.945 | 1.002 | 0.932 | 0.991 | 0.929 | ok |
| programs/knucleotide | 2 | 1061.7 | 1087.6 | 1073.8 | 1075.1 | 691.4 | 4061.5 | 4161.0 | 1.535 | 1.573 | 1.021 | 0.998 | 1.020 | 0.998 | 1.015 | 0.998 | 1.016 | 0.998 | ok |
| programs/load_big | 20 | 743.3 | 547.1 | 643.5 | 448.9 | 491.1 | 1787.3 | 620.0 | 1.514 | 1.114 | 0.985 | 0.999 | 1.007 | 0.998 | 0.973 | 0.999 | 1.023 | 0.999 | CHECK |
| programs/loops_arith | 20 | 455.4 | 455.2 | 459.4 | 459.1 | 1108.2 | 4344.5 | 4909.6 | 0.411 | 0.411 | 0.955 | 0.943 | 0.953 | 0.943 | 0.956 | 0.943 | 0.957 | 0.943 | CHECK |
| programs/mandelbrot | 5 | 534.0 | 533.6 | 534.9 | 534.5 | 521.6 | 4727.1 | 5530.2 | 1.024 | 1.023 | 1.002 | 1.000 | 1.003 | 1.000 | 1.002 | 1.000 | 1.002 | 1.000 | ok |
| programs/matmul | 20 | 413.5 | 409.0 | 438.8 | 439.4 | 272.0 | 3171.0 | 3693.4 | 1.520 | 1.504 | 0.939 | 0.881 | 0.940 | 0.881 | 0.987 | 0.893 | 0.990 | 0.893 | ok |
| programs/metamethods | 20 | 657.6 | 623.7 | 809.1 | 740.2 | 540.1 | 4614.3 | 4669.9 | 1.218 | 1.155 | 0.998 | 0.999 | 1.014 | 0.999 | 1.021 | 0.999 | 0.990 | 0.995 | ok |
| programs/nbody | 20 | 345.4 | 344.5 | 354.5 | 353.6 | 345.6 | 4246.2 | 4718.3 | 0.999 | 0.997 | 1.007 | 0.990 | 0.997 | 0.990 | 0.996 | 0.992 | 1.009 | 0.992 | ok |
| programs/oop_methods | 20 | 144.5 | 143.9 | 144.1 | 143.9 | 304.8 | 4567.3 | 4685.2 | 0.474 | 0.472 | 1.014 | 0.989 | 1.012 | 0.989 | 1.019 | 0.989 | 1.021 | 0.989 | ok |
| programs/pcall_error | 20 | 1483.2 | 1241.2 | 1518.8 | 1208.4 | 8346.6 | 4028.8 | 4363.2 | 0.178 | 0.149 | 0.995 | 1.001 | 0.999 | 1.009 | 1.022 | 0.996 | 0.979 | 0.993 | ok |
| programs/queens | 20 | 1857.7 | 1859.0 | 1911.8 | 1912.4 | 1907.1 | 4368.7 | 4120.8 | 0.974 | 0.975 | 0.998 | 0.955 | 1.007 | 0.955 | 0.993 | 0.962 | 0.994 | 0.962 | ok |
| programs/sieve | 5 | 578.3 | 574.0 | 597.3 | 668.9 | 761.7 | 2750.2 | 5029.1 | 0.759 | 0.754 | 0.998 | 0.898 | 0.984 | 0.898 | 0.975 | 0.904 | 0.985 | 0.917 | ok |
| programs/spectral_norm | 20 | 183.3 | 182.2 | 188.1 | 187.1 | 171.1 | 4573.7 | 4337.9 | 1.071 | 1.065 | 1.015 | 0.890 | 1.008 | 0.890 | 1.016 | 0.948 | 1.013 | 0.948 | ok |
| programs/string_ops | 20 | 1153.6 | 1096.5 | 1117.3 | 1020.9 | 2238.4 | 2828.0 | 2807.1 | 0.515 | 0.490 | 1.052 | 1.002 | 1.072 | 1.004 | 1.058 | 0.999 | 1.052 | 1.005 | ok |
| programs/table_array | 5 | 365.1 | 301.1 | 413.2 | 314.1 | 600.0 | 3400.3 | 3492.4 | 0.609 | 0.502 | 1.019 | 0.993 | 1.002 | 0.974 | 1.029 | 0.999 | 1.002 | 0.975 | ok |
| programs/table_hash | 20 | 501.8 | 491.2 | 543.1 | 495.8 | 394.4 | 1501.6 | 1831.0 | 1.272 | 1.246 | 1.030 | 1.005 | 1.031 | 1.004 | 1.111 | 1.006 | 1.026 | 1.005 | ok |
| programs/varargs | 20 | 1186.2 | 1063.4 | 1222.0 | 1094.4 | 2194.8 | 5366.5 | 5084.7 | 0.540 | 0.484 | 0.964 | 1.005 | 0.964 | 1.010 | 1.018 | 1.005 | 0.997 | 1.008 | ok |
| real/game_loop | 5 | 35.0 | 33.6 | 38.4 | 37.2 | 21.7 | 207.6 | 221.0 | 1.613 | 1.548 | 1.014 | 0.973 | 1.002 | 0.974 | 0.982 | 0.989 | 0.979 | 0.990 | ok |
| real/json_roundtrip | 5 | 551.6 | 464.7 | 583.2 | 484.0 | 469.1 | 796.8 | 802.8 | 1.176 | 0.991 | 1.010 | 0.989 | 1.017 | 0.993 | 1.004 | 0.985 | 1.027 | 0.993 | ok |
| real/lua_interp | 2 | 105.6 | 102.6 | 105.3 | 102.1 | 59.3 | 223.6 | 225.0 | 1.780 | 1.730 | 1.001 | 0.996 | 1.013 | 0.998 | 1.006 | 0.995 | 1.008 | 0.996 | ok |
| real/template_engine | 20 | 709.6 | 605.4 | 753.8 | 621.1 | 955.0 | 1225.1 | 1249.2 | 0.743 | 0.634 | 0.993 | 1.000 | 1.027 | 1.013 | 0.997 | 0.998 | 0.991 | 1.009 | ok |
| real/wordfreq | 5 | 113.9 | 96.8 | 113.8 | 97.3 | 116.4 | 212.8 | 210.2 | 0.979 | 0.831 | 1.006 | 1.000 | 1.006 | 0.988 | 0.997 | 0.998 | 1.005 | 0.989 | ok |

## Output / .expected check

- programs/load_big [p54]: out=diff exp=none err=
- programs/load_big [g54]: out=diff exp=none err=
- programs/loops_arith [p54]: out=diff exp=none err=
- programs/loops_arith [g54]: out=diff exp=none err=
- rerun programs/load_big [p54]: out=diff exp=none err=
- rerun programs/load_big [g54]: out=diff exp=none err=
- rerun programs/loops_arith [p54]: out=diff exp=none err=
- rerun programs/loops_arith [g54]: out=diff exp=none err=

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
