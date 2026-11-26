# Tables, session-n (subject ab7d185)

Ratios below 1: first column faster/fewer. Ours = min over all readings (2 quick, 5 full, 15-run serial reruns). Cached columns as stored.


# Aggregates, readings A: the fast pass only (min of 2 or 5 runs per program)

## Headline geomeans (cycles, ours/ref)

| build | /LuaJIT cyc (n) | /LuaJIT ins | /PUC same version cyc (223) | /PUC ins |
|---|---|---|---|---|
| 5.5 plain | 0.905 (219) | 1.264 | 0.207 | 0.207 |
| 5.5 PGO | 0.833 (219) | 1.170 | 0.191 | 0.192 |
| 5.4 plain | 0.960 (219) | 1.355 | 0.214 | 0.221 |
| 5.4 PGO | 0.885 (219) | 1.254 | 0.198 | 0.205 |

- 5.5 plain / LuaJIT -joff (219): 0.306
- 5.5 PGO / LuaJIT -joff (219): 0.282
- 5a064d7 cached (set B) (same programs): 5.5 plain/LJ 0.938, PGO/LJ 0.857, 5.4 plain/LJ 0.998, 5.4 PGO/LJ 0.914
- session-m published 8cd1f06 (same programs): 5.5 plain/LJ 0.924, PGO/LJ 0.849, 5.4 plain/LJ 0.984, 5.4 PGO/LJ 0.904
- 7413bff (session-l subject) (same programs): 5.5 plain/LJ 0.936, PGO/LJ 0.856, 5.4 plain/LJ 0.999, 5.4 PGO/LJ 0.913
- our 5.4 build / our 5.5 build cycles: plain 1.060, PGO 1.062; PGO/plain: 5.5 0.922, 5.4 0.923

## Subject vs 5a064d7 (same session, set B), 5a064d7 set A, session-k published and b116447 (geomean, 223 programs)

| build | vs | cycles | instructions | cyc >3% better / within / >3% worse | ins >3% fewer / more |
|---|---|---|---|---|---|
| 5.5 plain | 5a064d7 cached (set B) | 0.966 | 0.942 | 83 / 124 / 16 | 100 / 3 |
| 5.5 plain | 7413bff same machine (session-l subject) | 0.967 | 0.947 | 74 / 137 / 12 | 96 / 3 |
| 5.5 plain | session-m published 8cd1f06 | 0.980 | 0.981 | 45 / 163 / 15 | 35 / 2 |
| 5.5 PGO | 5a064d7 cached (set B) | 0.973 | 0.945 | 73 / 133 / 17 | 98 / 5 |
| 5.5 PGO | 7413bff same machine (session-l subject) | 0.974 | 0.949 | 71 / 137 / 15 | 90 / 5 |
| 5.5 PGO | session-m published 8cd1f06 | 0.982 | 0.981 | 50 / 159 / 14 | 36 / 2 |
| 5.4 plain | 5a064d7 cached (set B) | 0.963 | 0.945 | 87 / 123 / 13 | 99 / 2 |
| 5.4 plain | 7413bff same machine (session-l subject) | 0.962 | 0.949 | 87 / 126 / 10 | 92 / 2 |
| 5.4 plain | session-m published 8cd1f06 | 0.976 | 0.979 | 53 / 157 / 13 | 39 / 1 |
| 5.4 PGO | 5a064d7 cached (set B) | 0.969 | 0.947 | 74 / 138 / 11 | 95 / 3 |
| 5.4 PGO | 7413bff same machine (session-l subject) | 0.970 | 0.951 | 71 / 141 / 11 | 89 / 3 |
| 5.4 PGO | session-m published 8cd1f06 | 0.979 | 0.979 | 41 / 170 / 12 | 40 / 2 |

### Per set, geomean subject / 5a064d7 and subject / b116447 (cycles / instructions)

| set | n | plain vs b cyc | ins | PGO vs b cyc | ins | 5.4 plain vs b cyc | ins | 5.4 PGO vs b cyc | ins | plain vs i cyc | ins | PGO vs 7413bff cyc | ins | 5.4 plain vs i cyc | ins | 5.4 PGO vs 7413bff cyc | ins |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| apps | 5 | 0.959 | 0.930 | 0.959 | 0.938 | 0.957 | 0.935 | 0.949 | 0.940 | 0.967 | 0.942 | 0.962 | 0.947 | 0.977 | 0.949 | 0.976 | 0.952 |
| awfy | 14 | 0.971 | 0.949 | 0.974 | 0.954 | 0.966 | 0.950 | 0.966 | 0.955 | 0.967 | 0.954 | 0.971 | 0.958 | 0.963 | 0.956 | 0.965 | 0.960 |
| corpus | 18 | 0.946 | 0.935 | 0.957 | 0.941 | 0.933 | 0.929 | 0.938 | 0.935 | 0.948 | 0.942 | 0.955 | 0.948 | 0.935 | 0.931 | 0.935 | 0.938 |
| corpus2/dev | 17 | 0.980 | 0.946 | 0.981 | 0.950 | 0.967 | 0.942 | 0.978 | 0.947 | 0.992 | 0.969 | 0.986 | 0.973 | 0.968 | 0.963 | 0.983 | 0.968 |
| corpus2/heldout | 11 | 1.004 | 0.973 | 1.020 | 0.973 | 1.002 | 0.979 | 0.998 | 0.980 | 0.990 | 0.980 | 1.003 | 0.979 | 0.992 | 0.993 | 0.992 | 0.995 |
| jit-trace | 29 | 0.961 | 0.865 | 0.964 | 0.866 | 0.976 | 0.882 | 0.980 | 0.882 | 0.964 | 0.870 | 0.966 | 0.871 | 0.977 | 0.887 | 0.981 | 0.887 |
| lib | 95 | 0.960 | 0.956 | 0.969 | 0.957 | 0.956 | 0.955 | 0.963 | 0.956 | 0.962 | 0.956 | 0.972 | 0.957 | 0.953 | 0.955 | 0.964 | 0.956 |
| programs | 25 | 0.967 | 0.957 | 0.977 | 0.960 | 0.969 | 0.965 | 0.978 | 0.967 | 0.968 | 0.960 | 0.980 | 0.962 | 0.967 | 0.965 | 0.976 | 0.967 |
| real | 5 | 0.998 | 0.984 | 1.005 | 0.985 | 0.984 | 0.986 | 0.987 | 0.986 | 1.000 | 0.985 | 1.005 | 0.986 | 0.987 | 0.987 | 0.989 | 0.987 |
| real_dis | 4 | 1.022 | 0.993 | 1.016 | 1.004 | 0.995 | 0.987 | 1.010 | 1.000 | 1.008 | 0.991 | 1.019 | 1.003 | 0.981 | 0.985 | 1.003 | 1.000 |
| ALL | 223 | 0.966 | 0.942 | 0.973 | 0.945 | 0.963 | 0.945 | 0.969 | 0.947 | 0.967 | 0.947 | 0.974 | 0.949 | 0.962 | 0.949 | 0.970 | 0.951 |

# Aggregates, readings B: with the 15-run serial reruns of 145 programs merged (min over all readings)

## Headline geomeans (cycles, ours/ref)

| build | /LuaJIT cyc (n) | /LuaJIT ins | /PUC same version cyc (223) | /PUC ins |
|---|---|---|---|---|
| 5.5 plain | 0.900 (219) | 1.264 | 0.206 | 0.207 |
| 5.5 PGO | 0.828 (219) | 1.170 | 0.190 | 0.192 |
| 5.4 plain | 0.955 (219) | 1.355 | 0.213 | 0.221 |
| 5.4 PGO | 0.881 (219) | 1.254 | 0.197 | 0.205 |

- 5.5 plain / LuaJIT -joff (219): 0.304
- 5.5 PGO / LuaJIT -joff (219): 0.280
- 5a064d7 cached (set B) (same programs): 5.5 plain/LJ 0.938, PGO/LJ 0.857, 5.4 plain/LJ 0.998, 5.4 PGO/LJ 0.914
- session-m published 8cd1f06 (same programs): 5.5 plain/LJ 0.924, PGO/LJ 0.849, 5.4 plain/LJ 0.984, 5.4 PGO/LJ 0.904
- 7413bff (session-l subject) (same programs): 5.5 plain/LJ 0.936, PGO/LJ 0.856, 5.4 plain/LJ 0.999, 5.4 PGO/LJ 0.913
- our 5.4 build / our 5.5 build cycles: plain 1.060, PGO 1.062; PGO/plain: 5.5 0.922, 5.4 0.924

## Subject vs 5a064d7 (same session, set B), 5a064d7 set A, session-k published and b116447 (geomean, 223 programs)

| build | vs | cycles | instructions | cyc >3% better / within / >3% worse | ins >3% fewer / more |
|---|---|---|---|---|---|
| 5.5 plain | 5a064d7 cached (set B) | 0.961 | 0.942 | 89 / 123 / 11 | 100 / 3 |
| 5.5 plain | 7413bff same machine (session-l subject) | 0.962 | 0.947 | 85 / 130 / 8 | 96 / 3 |
| 5.5 plain | session-m published 8cd1f06 | 0.974 | 0.981 | 56 / 160 / 7 | 35 / 2 |
| 5.5 PGO | 5a064d7 cached (set B) | 0.968 | 0.945 | 78 / 132 / 13 | 98 / 5 |
| 5.5 PGO | 7413bff same machine (session-l subject) | 0.969 | 0.949 | 77 / 136 / 10 | 90 / 5 |
| 5.5 PGO | session-m published 8cd1f06 | 0.977 | 0.981 | 56 / 160 / 7 | 36 / 2 |
| 5.4 plain | 5a064d7 cached (set B) | 0.958 | 0.945 | 90 / 128 / 5 | 99 / 2 |
| 5.4 plain | 7413bff same machine (session-l subject) | 0.956 | 0.949 | 97 / 122 / 4 | 92 / 2 |
| 5.4 plain | session-m published 8cd1f06 | 0.971 | 0.979 | 61 / 157 / 5 | 39 / 1 |
| 5.4 PGO | 5a064d7 cached (set B) | 0.964 | 0.947 | 76 / 143 / 4 | 95 / 3 |
| 5.4 PGO | 7413bff same machine (session-l subject) | 0.965 | 0.951 | 82 / 138 / 3 | 89 / 3 |
| 5.4 PGO | session-m published 8cd1f06 | 0.974 | 0.979 | 52 / 166 / 5 | 40 / 2 |

### Per set, geomean subject / 5a064d7 and subject / b116447 (cycles / instructions)

| set | n | plain vs b cyc | ins | PGO vs b cyc | ins | 5.4 plain vs b cyc | ins | 5.4 PGO vs b cyc | ins | plain vs i cyc | ins | PGO vs 7413bff cyc | ins | 5.4 plain vs i cyc | ins | 5.4 PGO vs 7413bff cyc | ins |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| apps | 5 | 0.957 | 0.930 | 0.956 | 0.938 | 0.954 | 0.935 | 0.948 | 0.940 | 0.965 | 0.942 | 0.959 | 0.947 | 0.974 | 0.949 | 0.975 | 0.952 |
| awfy | 14 | 0.966 | 0.949 | 0.967 | 0.954 | 0.962 | 0.950 | 0.961 | 0.955 | 0.961 | 0.954 | 0.965 | 0.958 | 0.959 | 0.956 | 0.960 | 0.960 |
| corpus | 18 | 0.941 | 0.935 | 0.951 | 0.940 | 0.927 | 0.929 | 0.935 | 0.935 | 0.943 | 0.942 | 0.949 | 0.947 | 0.930 | 0.931 | 0.932 | 0.938 |
| corpus2/dev | 17 | 0.953 | 0.946 | 0.961 | 0.950 | 0.942 | 0.942 | 0.957 | 0.947 | 0.964 | 0.969 | 0.966 | 0.973 | 0.943 | 0.963 | 0.961 | 0.968 |
| corpus2/heldout | 11 | 0.987 | 0.973 | 0.993 | 0.973 | 0.976 | 0.979 | 0.978 | 0.980 | 0.973 | 0.980 | 0.977 | 0.979 | 0.967 | 0.993 | 0.972 | 0.995 |
| jit-trace | 29 | 0.959 | 0.865 | 0.962 | 0.866 | 0.975 | 0.882 | 0.978 | 0.882 | 0.962 | 0.870 | 0.964 | 0.871 | 0.976 | 0.887 | 0.980 | 0.887 |
| lib | 95 | 0.957 | 0.956 | 0.967 | 0.957 | 0.953 | 0.955 | 0.961 | 0.956 | 0.958 | 0.956 | 0.969 | 0.957 | 0.951 | 0.955 | 0.962 | 0.956 |
| programs | 25 | 0.967 | 0.957 | 0.976 | 0.960 | 0.967 | 0.965 | 0.978 | 0.967 | 0.967 | 0.960 | 0.979 | 0.962 | 0.965 | 0.965 | 0.975 | 0.967 |
| real | 5 | 0.997 | 0.984 | 1.002 | 0.985 | 0.982 | 0.986 | 0.986 | 0.986 | 0.999 | 0.985 | 1.002 | 0.986 | 0.985 | 0.987 | 0.988 | 0.987 |
| real_dis | 4 | 1.018 | 0.993 | 1.012 | 1.004 | 0.990 | 0.987 | 1.006 | 1.000 | 1.003 | 0.991 | 1.015 | 1.003 | 0.976 | 0.985 | 0.999 | 1.000 |
| ALL | 223 | 0.961 | 0.942 | 0.968 | 0.945 | 0.958 | 0.945 | 0.964 | 0.947 | 0.962 | 0.947 | 0.969 | 0.949 | 0.956 | 0.949 | 0.965 | 0.951 |

### 5.5 plain vs 5a064d7: 20 biggest improvements by cycles (>= 20 Mcyc)

| program | Mcyc | cyc ratio | ins ratio | cycle spread of readings | note |
|---|---|---|---|---|---|
| lib/tremove_pop | 135 | 0.561 | 0.560 | 6.5% (5 runs) | instructions move the same way |
| corpus/classic_vector | 301 | 0.639 | 0.636 | 6.7% (14 runs) | instructions move the same way |
| lib/co_yield | 174 | 0.730 | 0.919 | 2.8% (14 runs) | instructions move the same way |
| jit-trace/upval_acc | 1022 | 0.740 | 0.812 | 15.5% (14 runs) | instructions move the same way |
| corpus/lua_lexer | 814 | 0.754 | 0.743 | 8.8% (14 runs) | instructions move the same way |
| programs/coroutines | 1365 | 0.757 | 0.934 | 7.8% (14 runs) | instructions move the same way |
| corpus2/dev/coro_generators | 963 | 0.769 | 0.939 | 3.7% (14 runs) | instructions move the same way |
| lib/tonumber_num | 166 | 0.773 | 0.838 | 5.8% (14 runs) | instructions move the same way |
| lib/match | 48 | 0.774 | 0.805 | 9.3% (14 runs) | instructions move the same way |
| corpus2/dev/oop_mixins | 1097 | 0.786 | 0.712 | 15.7% (14 runs) | instructions move the same way |
| lib/byte_multi | 33 | 0.804 | 0.820 | 4.3% (14 runs) | instructions move the same way |
| jit-trace/rec_loop | 76 | 0.828 | 0.802 | 1.8% (14 runs) | instructions move the same way |
| apps/raytrace | 93 | 0.833 | 0.815 | 5.8% (14 runs) | instructions move the same way |
| lib/str_eq | 135 | 0.845 | 0.921 | 3.4% (14 runs) | instructions move the same way |
| lib/find_pat | 218 | 0.847 | 0.822 | 7.6% (14 runs) | instructions move the same way |
| lib/str_lt_short | 97 | 0.853 | 0.944 | 1.3% (14 runs) | instructions move the same way |
| jit-trace/oop_inherit | 50 | 0.863 | 0.937 | 1.8% (14 runs) | instructions move the same way |
| corpus2/dev/expr_compiler | 1143 | 0.866 | 0.884 | 3.2% (14 runs) | instructions move the same way |
| lib/byte | 44 | 0.874 | 0.877 | 1.5% (14 runs) | instructions move the same way |
| programs/oop_methods | 125 | 0.874 | 0.894 | 0.7% (14 runs) | instructions move the same way |

### 5.5 PGO vs 5a064d7: 20 biggest improvements by cycles (>= 20 Mcyc)

| program | Mcyc | cyc ratio | ins ratio | cycle spread of readings | note |
|---|---|---|---|---|---|
| corpus/classic_vector | 288 | 0.666 | 0.651 | 6.6% (14 runs) | instructions move the same way |
| jit-trace/upval_acc | 1021 | 0.739 | 0.812 | 9.4% (14 runs) | instructions move the same way |
| corpus/lua_lexer | 709 | 0.739 | 0.736 | 3.0% (14 runs) | instructions move the same way |
| corpus2/dev/oop_mixins | 1031 | 0.759 | 0.701 | 14.8% (14 runs) | instructions move the same way |
| lib/tremove_pop | 135 | 0.780 | 0.720 | 3.2% (5 runs) | instructions move the same way |
| lib/tonumber_num | 166 | 0.786 | 0.838 | 33.1% (14 runs) | instructions move the same way |
| lib/byte_multi | 33 | 0.791 | 0.821 | 4.3% (14 runs) | instructions move the same way |
| jit-trace/rec_loop | 76 | 0.828 | 0.803 | 1.7% (14 runs) | instructions move the same way |
| lib/sub | 107 | 0.835 | 0.892 | 9.4% (14 runs) | instructions move the same way |
| apps/raytrace | 87 | 0.838 | 0.825 | 5.4% (14 runs) | instructions move the same way |
| lib/str_eq | 135 | 0.843 | 0.921 | 7.2% (14 runs) | instructions move the same way |
| lib/str_lt_short | 90 | 0.850 | 0.943 | 1.4% (14 runs) | instructions move the same way |
| lib/find_pat | 195 | 0.853 | 0.796 | 1.2% (14 runs) | instructions move the same way |
| lib/match | 43 | 0.854 | 0.834 | 3.4% (14 runs) | instructions move the same way |
| jit-trace/oop_inherit | 49 | 0.862 | 0.939 | 1.6% (14 runs) | instructions move the same way |
| corpus2/dev/expr_compiler | 976 | 0.867 | 0.879 | 3.7% (14 runs) | instructions move the same way |
| lib/byte | 43 | 0.871 | 0.878 | 2.0% (14 runs) | instructions move the same way |
| programs/oop_methods | 124 | 0.874 | 0.894 | 1.1% (14 runs) | instructions move the same way |
| corpus2/dev/lua_parser | 982 | 0.881 | 0.847 | 3.4% (14 runs) | instructions move the same way |
| lib/m_min | 199 | 0.882 | 0.860 | 13.8% (14 runs) | instructions move the same way |

### 5.4 plain vs 5a064d7: 20 biggest improvements by cycles (>= 20 Mcyc)

| program | Mcyc | cyc ratio | ins ratio | cycle spread of readings | note |
|---|---|---|---|---|---|
| lib/tremove_pop | 131 | 0.584 | 0.585 | 5.1% (5 runs) | instructions move the same way |
| corpus/classic_vector | 437 | 0.672 | 0.650 | 5.7% (14 runs) | instructions move the same way |
| lib/co_yield | 176 | 0.728 | 0.919 | 11.1% (14 runs) | instructions move the same way |
| programs/coroutines | 1410 | 0.759 | 0.938 | 3.5% (14 runs) | instructions move the same way |
| corpus/lua_lexer | 844 | 0.760 | 0.755 | 2.4% (14 runs) | instructions move the same way |
| corpus2/dev/coro_generators | 991 | 0.763 | 0.931 | 2.4% (14 runs) | instructions move the same way |
| lib/tonumber_num | 166 | 0.763 | 0.838 | 45.0% (14 runs) | instructions move the same way |
| corpus2/dev/oop_mixins | 1106 | 0.773 | 0.722 | 12.6% (14 runs) | instructions move the same way |
| lib/match | 48 | 0.789 | 0.799 | 9.2% (14 runs) | instructions move the same way |
| lib/find_pat | 218 | 0.806 | 0.819 | 0.9% (14 runs) | instructions move the same way |
| lib/byte_multi | 33 | 0.809 | 0.820 | 8.2% (14 runs) | instructions move the same way |
| corpus2/dev/lua_parser | 1259 | 0.822 | 0.800 | 3.9% (14 runs) | instructions move the same way |
| lib/len_op | 48 | 0.837 | 0.921 | 43.9% (14 runs) | instructions move the same way |
| lib/str_eq | 135 | 0.844 | 0.921 | 17.9% (14 runs) | instructions move the same way |
| jit-trace/upval_acc | 1174 | 0.848 | 0.803 | 5.0% (14 runs) | instructions move the same way |
| lib/tonumber_bad | 142 | 0.853 | 0.990 | 30.1% (14 runs) | instructions move the same way |
| corpus/astar_jumper | 1599 | 0.866 | 0.823 | 6.7% (14 runs) | instructions move the same way |
| lib/byte | 44 | 0.867 | 0.877 | 2.4% (14 runs) | instructions move the same way |
| apps/raytrace | 103 | 0.877 | 0.852 | 8.5% (14 runs) | instructions move the same way |
| jit-trace/field_method | 502 | 0.879 | 0.871 | 0.4% (14 runs) | instructions move the same way |

### 5.4 PGO vs 5a064d7: 20 biggest improvements by cycles (>= 20 Mcyc)

| program | Mcyc | cyc ratio | ins ratio | cycle spread of readings | note |
|---|---|---|---|---|---|
| corpus/classic_vector | 400 | 0.710 | 0.685 | 4.8% (14 runs) | instructions move the same way |
| corpus/lua_lexer | 737 | 0.743 | 0.751 | 6.1% (14 runs) | instructions move the same way |
| lib/tremove_pop | 158 | 0.755 | 0.685 | 2.9% (5 runs) | instructions move the same way |
| lib/tonumber_bad | 112 | 0.777 | 0.989 | 3.6% (14 runs) | instructions move the same way |
| corpus2/dev/oop_mixins | 1038 | 0.785 | 0.716 | 14.4% (14 runs) | instructions move the same way |
| lib/tonumber_num | 166 | 0.786 | 0.839 | 44.9% (14 runs) | instructions move the same way |
| lib/byte_multi | 33 | 0.801 | 0.821 | 6.0% (14 runs) | instructions move the same way |
| corpus2/dev/lua_parser | 1079 | 0.823 | 0.787 | 4.1% (14 runs) | instructions move the same way |
| lib/find_pat | 195 | 0.826 | 0.795 | 2.4% (14 runs) | instructions move the same way |
| lib/len_op | 48 | 0.845 | 0.921 | 6.4% (14 runs) | instructions move the same way |
| lib/str_eq | 135 | 0.845 | 0.921 | 17.7% (14 runs) | instructions move the same way |
| lib/match | 44 | 0.846 | 0.826 | 8.0% (14 runs) | instructions move the same way |
| jit-trace/upval_acc | 1172 | 0.846 | 0.803 | 4.7% (14 runs) | instructions move the same way |
| lib/sub | 107 | 0.862 | 0.892 | 1.5% (14 runs) | instructions move the same way |
| lib/byte | 44 | 0.866 | 0.878 | 2.2% (14 runs) | instructions move the same way |
| apps/raytrace | 95 | 0.869 | 0.861 | 2.2% (14 runs) | instructions move the same way |
| corpus/astar_jumper | 1537 | 0.869 | 0.819 | 5.2% (14 runs) | instructions move the same way |
| apps/expr_lang | 127 | 0.870 | 0.887 | 3.7% (14 runs) | instructions move the same way |
| corpus2/dev/expr_compiler | 1022 | 0.875 | 0.879 | 3.1% (14 runs) | instructions move the same way |
| programs/oop_methods | 124 | 0.880 | 0.894 | 0.5% (14 runs) | instructions move the same way |

### 5.5 plain vs 5a064d7: regressions (instructions > +2% or cycles > +3%, >= 20 Mcyc)

| program | Mcyc | cyc ratio | ins ratio | cycle spread of readings | note |
|---|---|---|---|---|---|
| lib/rep_sep | 100 | 1.133 | 1.144 | 1.0% (5 runs) | instructions move the same way |
| corpus2/heldout/real_dis_ppc | 2257 | 1.083 | 1.043 | 16.6% (14 runs) | instructions move the same way |
| corpus2/heldout/log_analyzer | 2225 | 1.058 | 0.978 | 8.6% (5 runs) |  |
| corpus2/dev/diff_levenshtein | 444 | 1.041 | 0.981 | 1.1% (5 runs) |  |
| lib/concat_op_str | 140 | 1.041 | 0.995 | 1.4% (5 runs) | instructions equal: layout / IPC |
| lib/len_tbl | 143 | 1.040 | 0.987 | 2.1% (14 runs) |  |
| apps/markdown | 121 | 1.036 | 1.006 | 10.3% (14 runs) | instructions equal: layout / IPC |
| corpus2/heldout/prototype_objects | 1431 | 1.034 | 0.999 | 52.3% (14 runs) | instructions equal: layout / IPC |
| lib/error_tbl | 275 | 1.034 | 1.009 | 5.5% (14 runs) | instructions equal: layout / IPC |
| real/json_roundtrip | 563 | 1.031 | 0.993 | 4.0% (5 runs) | instructions equal: layout / IPC |
| corpus2/dev/real_dis_x86 | 1570 | 1.030 | 0.993 | 1.6% (5 runs) | instructions equal: layout / IPC |
| corpus/parser_combinator | 2326 | 1.020 | 1.032 | 2.1% (14 runs) |  |
| lib/os_clock | 115 | 1.005 | 1.021 | 8.6% (5 runs) |  |
| lib/m_ult | 275 | 1.004 | 1.021 | 14.8% (14 runs) |  |
| lib/tinsert_idx | 175 | 0.997 | 1.027 | 2.7% (14 runs) |  |
| lib/m_fmod | 218 | 0.997 | 1.023 | 7.1% (5 runs) |  |

### 5.5 PGO vs 5a064d7: regressions (instructions > +2% or cycles > +3%, >= 20 Mcyc)

| program | Mcyc | cyc ratio | ins ratio | cycle spread of readings | note |
|---|---|---|---|---|---|
| lib/rep_sep | 94 | 1.167 | 1.196 | 0.4% (5 runs) | instructions move the same way |
| corpus2/heldout/log_analyzer | 1902 | 1.088 | 0.976 | 5.9% (5 runs) |  |
| corpus/parser_combinator | 2007 | 1.075 | 1.047 | 4.5% (14 runs) | instructions move the same way |
| real/template_engine | 621 | 1.053 | 1.020 | 4.6% (14 runs) | instructions move the same way |
| corpus2/heldout/real_dis_ppc | 1952 | 1.050 | 1.057 | 10.6% (14 runs) | instructions move the same way |
| lib/len_tbl | 142 | 1.048 | 0.987 | 2.4% (14 runs) |  |
| lib/load_small | 159 | 1.047 | 0.992 | 2.7% (14 runs) | instructions equal: layout / IPC |
| corpus/template_etlua | 1289 | 1.043 | 1.000 | 3.4% (14 runs) | instructions equal: layout / IPC |
| lib/utf8_len | 286 | 1.041 | 0.998 | 1.8% (14 runs) | instructions equal: layout / IPC |
| lib/sel_n | 334 | 1.037 | 0.998 | 14.2% (14 runs) | instructions equal: layout / IPC |
| corpus2/dev/real_dis_x86 | 1392 | 1.037 | 0.999 | 1.6% (5 runs) | instructions equal: layout / IPC |
| corpus2/heldout/prototype_objects | 1340 | 1.035 | 1.004 | 56.8% (14 runs) | instructions equal: layout / IPC |
| lib/gmatch_words | 140 | 1.032 | 1.000 | 11.1% (14 runs) | instructions equal: layout / IPC |
| apps/markdown | 104 | 1.026 | 1.023 | 4.5% (14 runs) |  |
| lib/tinsert_idx | 154 | 1.012 | 1.030 | 9.1% (14 runs) |  |
| lib/tfront_1k | 76 | 0.998 | 1.022 | 3.2% (14 runs) |  |
| programs/coroutines | 1289 | 0.992 | 1.038 | 9.9% (14 runs) |  |

### 5.4 plain vs 5a064d7: regressions (instructions > +2% or cycles > +3%, >= 20 Mcyc)

| program | Mcyc | cyc ratio | ins ratio | cycle spread of readings | note |
|---|---|---|---|---|---|
| lib/rep_sep | 130 | 1.113 | 1.116 | 3.6% (5 runs) | instructions move the same way |
| lib/fmt_q | 159 | 1.083 | 0.986 | 2.1% (14 runs) |  |
| lib/m_tointeger | 237 | 1.060 | 0.999 | 0.5% (5 runs) | instructions equal: layout / IPC |
| lib/gsub_tbl | 160 | 1.051 | 0.987 | 2.4% (14 runs) |  |
| lib/tsort_cmp | 283 | 1.031 | 0.999 | 17.1% (5 runs) | instructions equal: layout / IPC |
| lib/os_clock | 121 | 1.020 | 1.020 | 0.8% (5 runs) |  |
| lib/tinsert_append | 194 | 1.013 | 1.023 | 16.1% (14 runs) |  |
| lib/tinsert_idx | 207 | 1.010 | 1.023 | 20.7% (14 runs) |  |
| lib/m_fmod | 220 | 0.996 | 1.023 | 4.5% (5 runs) |  |
| lib/m_ult | 275 | 0.996 | 1.021 | 5.9% (14 runs) |  |
| corpus2/heldout/stream_closures | 800 | 0.941 | 1.120 | 2.0% (5 runs) |  |

### 5.4 PGO vs 5a064d7: regressions (instructions > +2% or cycles > +3%, >= 20 Mcyc)

| program | Mcyc | cyc ratio | ins ratio | cycle spread of readings | note |
|---|---|---|---|---|---|
| lib/rep_sep | 120 | 1.113 | 1.156 | 1.7% (5 runs) | instructions move the same way |
| lib/tonumber_base | 275 | 1.041 | 1.017 | 10.2% (14 runs) | instructions move the same way |
| lib/load_small | 164 | 1.038 | 0.972 | 2.0% (14 runs) |  |
| corpus2/dev/real_dis_x86 | 1470 | 1.031 | 1.005 | 2.4% (5 runs) | instructions equal: layout / IPC |
| lib/tinsert_idx | 194 | 1.023 | 1.025 | 8.0% (14 runs) |  |
| lib/tfront_1k | 77 | 1.015 | 1.024 | 3.8% (14 runs) |  |
| lib/tinsert_append | 182 | 1.006 | 1.025 | 9.6% (14 runs) |  |
| corpus2/heldout/stream_closures | 743 | 0.953 | 1.138 | 3.3% (5 runs) |  |
| programs/coroutines | 1317 | 0.932 | 1.030 | 5.2% (14 runs) |  |

### 5.5 plain vs 7413bff: 20 biggest improvements by cycles (>= 20 Mcyc)

| program | Mcyc | cyc ratio | ins ratio | cycle spread of readings | note |
|---|---|---|---|---|---|
| lib/tremove_pop | 135 | 0.561 | 0.560 | 6.5% (5 runs) | instructions move the same way |
| corpus/classic_vector | 301 | 0.648 | 0.641 | 6.7% (14 runs) | instructions move the same way |
| lib/co_yield | 174 | 0.734 | 0.919 | 2.8% (14 runs) | instructions move the same way |
| jit-trace/upval_acc | 1022 | 0.740 | 0.812 | 15.5% (14 runs) | instructions move the same way |
| programs/coroutines | 1365 | 0.764 | 0.934 | 7.8% (14 runs) | instructions move the same way |
| corpus2/dev/coro_generators | 963 | 0.767 | 0.941 | 3.7% (14 runs) | instructions move the same way |
| lib/match | 48 | 0.784 | 0.805 | 9.3% (14 runs) | instructions move the same way |
| lib/byte_multi | 33 | 0.805 | 0.820 | 4.3% (14 runs) | instructions move the same way |
| lib/find_pat | 218 | 0.831 | 0.822 | 7.6% (14 runs) | instructions move the same way |
| corpus/lua_lexer | 814 | 0.834 | 0.831 | 8.8% (14 runs) | instructions move the same way |
| lib/tonumber_num | 166 | 0.843 | 0.838 | 5.8% (14 runs) | instructions move the same way |
| lib/str_eq | 135 | 0.846 | 0.921 | 3.4% (14 runs) | instructions move the same way |
| apps/raytrace | 93 | 0.860 | 0.853 | 5.8% (14 runs) | instructions move the same way |
| jit-trace/oop_inherit | 50 | 0.867 | 0.938 | 1.8% (14 runs) | instructions move the same way |
| programs/oop_methods | 125 | 0.872 | 0.894 | 0.7% (14 runs) | instructions move the same way |
| lib/byte | 44 | 0.874 | 0.877 | 1.5% (14 runs) | instructions move the same way |
| corpus2/dev/expr_compiler | 1143 | 0.874 | 0.890 | 3.2% (14 runs) | instructions move the same way |
| lib/sub | 115 | 0.879 | 0.933 | 4.2% (14 runs) | instructions move the same way |
| jit-trace/field_method | 501 | 0.881 | 0.871 | 0.4% (14 runs) | instructions move the same way |
| lib/it_next_empty | 46 | 0.882 | 0.966 | 17.7% (14 runs) | instructions move the same way |

### 5.5 PGO vs 7413bff: 20 biggest improvements by cycles (>= 20 Mcyc)

| program | Mcyc | cyc ratio | ins ratio | cycle spread of readings | note |
|---|---|---|---|---|---|
| corpus/classic_vector | 288 | 0.668 | 0.654 | 6.6% (14 runs) | instructions move the same way |
| jit-trace/upval_acc | 1021 | 0.740 | 0.812 | 9.4% (14 runs) | instructions move the same way |
| lib/tremove_pop | 135 | 0.782 | 0.720 | 3.2% (5 runs) | instructions move the same way |
| lib/byte_multi | 33 | 0.804 | 0.821 | 4.3% (14 runs) | instructions move the same way |
| corpus/lua_lexer | 709 | 0.817 | 0.825 | 3.0% (14 runs) | instructions move the same way |
| lib/tonumber_num | 166 | 0.817 | 0.838 | 33.1% (14 runs) | instructions move the same way |
| lib/str_eq | 135 | 0.844 | 0.921 | 7.2% (14 runs) | instructions move the same way |
| lib/find_pat | 195 | 0.848 | 0.796 | 1.2% (14 runs) | instructions move the same way |
| lib/match | 43 | 0.850 | 0.834 | 3.4% (14 runs) | instructions move the same way |
| jit-trace/oop_inherit | 49 | 0.864 | 0.939 | 1.6% (14 runs) | instructions move the same way |
| apps/raytrace | 87 | 0.864 | 0.854 | 5.4% (14 runs) | instructions move the same way |
| corpus2/dev/expr_compiler | 976 | 0.865 | 0.885 | 3.7% (14 runs) | instructions move the same way |
| lib/byte | 43 | 0.868 | 0.878 | 2.0% (14 runs) | instructions move the same way |
| programs/oop_methods | 124 | 0.871 | 0.894 | 1.1% (14 runs) | instructions move the same way |
| lib/m_min | 199 | 0.873 | 0.860 | 13.8% (14 runs) | instructions move the same way |
| lib/sub | 107 | 0.875 | 0.892 | 9.4% (14 runs) | instructions move the same way |
| corpus2/dev/lua_parser | 982 | 0.880 | 0.849 | 3.4% (14 runs) | instructions move the same way |
| awfy/cd | 213 | 0.885 | 0.844 | 4.3% (14 runs) | instructions move the same way |
| jit-trace/inline_leaf | 406 | 0.892 | 0.877 | 2.3% (14 runs) | instructions move the same way |
| awfy/sieve | 111 | 0.902 | 0.850 | 3.9% (14 runs) | instructions move the same way |

### 5.4 plain vs 7413bff: 20 biggest improvements by cycles (>= 20 Mcyc)

| program | Mcyc | cyc ratio | ins ratio | cycle spread of readings | note |
|---|---|---|---|---|---|
| lib/tremove_pop | 131 | 0.587 | 0.585 | 5.1% (5 runs) | instructions move the same way |
| corpus/classic_vector | 437 | 0.693 | 0.653 | 5.7% (14 runs) | instructions move the same way |
| corpus/lua_lexer | 844 | 0.758 | 0.755 | 2.4% (14 runs) | instructions move the same way |
| programs/coroutines | 1410 | 0.764 | 0.938 | 3.5% (14 runs) | instructions move the same way |
| corpus2/dev/coro_generators | 991 | 0.766 | 0.932 | 2.4% (14 runs) | instructions move the same way |
| lib/len_op | 48 | 0.768 | 0.921 | 43.9% (14 runs) | instructions move the same way |
| lib/co_yield | 176 | 0.769 | 0.919 | 11.1% (14 runs) | instructions move the same way |
| lib/match | 48 | 0.784 | 0.799 | 9.2% (14 runs) | instructions move the same way |
| lib/byte_multi | 33 | 0.796 | 0.820 | 8.2% (14 runs) | instructions move the same way |
| corpus2/dev/lua_parser | 1259 | 0.808 | 0.815 | 3.9% (14 runs) | instructions move the same way |
| lib/tonumber_num | 166 | 0.813 | 0.838 | 45.0% (14 runs) | instructions move the same way |
| lib/find_pat | 218 | 0.827 | 0.819 | 0.9% (14 runs) | instructions move the same way |
| lib/str_eq | 135 | 0.845 | 0.921 | 17.9% (14 runs) | instructions move the same way |
| jit-trace/upval_acc | 1174 | 0.847 | 0.803 | 5.0% (14 runs) | instructions move the same way |
| corpus/astar_jumper | 1599 | 0.853 | 0.832 | 6.7% (14 runs) | instructions move the same way |
| lib/byte | 44 | 0.858 | 0.877 | 2.4% (14 runs) | instructions move the same way |
| lib/str_method_len | 49 | 0.865 | 0.955 | 6.4% (14 runs) | instructions move the same way |
| jit-trace/field_method | 502 | 0.868 | 0.871 | 0.4% (14 runs) | instructions move the same way |
| lib/tonumber_bad | 142 | 0.870 | 0.990 | 30.1% (14 runs) | instructions move the same way |
| lib/find_plain | 111 | 0.875 | 0.854 | 13.0% (14 runs) | instructions move the same way |

### 5.4 PGO vs 7413bff: 20 biggest improvements by cycles (>= 20 Mcyc)

| program | Mcyc | cyc ratio | ins ratio | cycle spread of readings | note |
|---|---|---|---|---|---|
| corpus/classic_vector | 400 | 0.715 | 0.687 | 4.8% (14 runs) | instructions move the same way |
| corpus/lua_lexer | 737 | 0.750 | 0.751 | 6.1% (14 runs) | instructions move the same way |
| lib/tremove_pop | 158 | 0.768 | 0.685 | 2.9% (5 runs) | instructions move the same way |
| lib/tonumber_bad | 112 | 0.787 | 0.989 | 3.6% (14 runs) | instructions move the same way |
| lib/byte_multi | 33 | 0.798 | 0.821 | 6.0% (14 runs) | instructions move the same way |
| corpus2/dev/lua_parser | 1079 | 0.808 | 0.803 | 4.1% (14 runs) | instructions move the same way |
| lib/tonumber_num | 166 | 0.823 | 0.839 | 44.9% (14 runs) | instructions move the same way |
| lib/find_pat | 195 | 0.831 | 0.795 | 2.4% (14 runs) | instructions move the same way |
| lib/match | 44 | 0.842 | 0.826 | 8.0% (14 runs) | instructions move the same way |
| lib/len_op | 48 | 0.843 | 0.921 | 6.4% (14 runs) | instructions move the same way |
| lib/str_eq | 135 | 0.844 | 0.921 | 17.7% (14 runs) | instructions move the same way |
| jit-trace/upval_acc | 1172 | 0.847 | 0.803 | 4.7% (14 runs) | instructions move the same way |
| corpus/astar_jumper | 1537 | 0.852 | 0.828 | 5.2% (14 runs) | instructions move the same way |
| lib/byte | 44 | 0.865 | 0.878 | 2.2% (14 runs) | instructions move the same way |
| corpus2/dev/expr_compiler | 1022 | 0.872 | 0.893 | 3.1% (14 runs) | instructions move the same way |
| programs/oop_methods | 124 | 0.872 | 0.894 | 0.5% (14 runs) | instructions move the same way |
| awfy/cd | 234 | 0.881 | 0.853 | 5.4% (14 runs) | instructions move the same way |
| awfy/sieve | 113 | 0.886 | 0.874 | 4.5% (14 runs) | instructions move the same way |
| jit-trace/field_method | 502 | 0.888 | 0.872 | 0.6% (14 runs) | instructions move the same way |
| lib/sub | 107 | 0.888 | 0.892 | 1.5% (14 runs) | instructions move the same way |

### 5.5 plain vs 7413bff: regressions (instructions > +2% or cycles > +3%, >= 20 Mcyc)

| program | Mcyc | cyc ratio | ins ratio | cycle spread of readings | note |
|---|---|---|---|---|---|
| lib/rep_sep | 100 | 1.116 | 1.144 | 1.0% (5 runs) | instructions move the same way |
| corpus2/dev/regex_nfa | 673 | 1.074 | 1.018 | 7.9% (14 runs) | instructions move the same way |
| corpus2/heldout/real_dis_ppc | 2257 | 1.068 | 1.043 | 16.6% (14 runs) | instructions move the same way |
| lib/error_tbl | 275 | 1.052 | 1.009 | 5.5% (14 runs) | instructions equal: layout / IPC |
| lib/sel_n | 336 | 1.037 | 0.998 | 12.0% (14 runs) | instructions equal: layout / IPC |
| corpus2/dev/diff_levenshtein | 444 | 1.037 | 0.982 | 1.1% (5 runs) |  |
| corpus2/heldout/log_analyzer | 2225 | 1.034 | 0.978 | 8.6% (5 runs) |  |
| lib/len_tbl | 143 | 1.031 | 0.987 | 2.1% (14 runs) |  |
| lib/tinsert_idx | 175 | 1.008 | 1.028 | 2.7% (14 runs) |  |
| lib/m_ult | 275 | 1.008 | 1.021 | 14.8% (14 runs) |  |
| lib/os_clock | 115 | 1.006 | 1.021 | 8.6% (5 runs) |  |
| corpus/parser_combinator | 2326 | 1.005 | 1.032 | 2.1% (14 runs) |  |
| lib/m_fmod | 218 | 0.995 | 1.023 | 7.1% (5 runs) |  |

### 5.5 PGO vs 7413bff: regressions (instructions > +2% or cycles > +3%, >= 20 Mcyc)

| program | Mcyc | cyc ratio | ins ratio | cycle spread of readings | note |
|---|---|---|---|---|---|
| lib/rep_sep | 94 | 1.162 | 1.196 | 0.4% (5 runs) | instructions move the same way |
| lib/load_small | 159 | 1.083 | 0.992 | 2.7% (14 runs) | instructions equal: layout / IPC |
| corpus2/heldout/log_analyzer | 1902 | 1.078 | 0.977 | 5.9% (5 runs) |  |
| corpus2/heldout/real_dis_ppc | 1952 | 1.069 | 1.059 | 10.6% (14 runs) | instructions move the same way |
| corpus/parser_combinator | 2007 | 1.046 | 1.047 | 4.5% (14 runs) | instructions move the same way |
| real/template_engine | 621 | 1.042 | 1.019 | 4.6% (14 runs) | instructions move the same way |
| lib/utf8_len | 286 | 1.037 | 0.998 | 1.8% (14 runs) | instructions equal: layout / IPC |
| lib/gmatch_words | 140 | 1.032 | 1.000 | 11.1% (14 runs) | instructions equal: layout / IPC |
| corpus/template_etlua | 1289 | 1.032 | 1.001 | 3.4% (14 runs) | instructions equal: layout / IPC |
| lib/len_tbl | 142 | 1.030 | 0.987 | 2.4% (14 runs) |  |
| lib/tinsert_idx | 154 | 1.019 | 1.031 | 9.1% (14 runs) |  |
| apps/markdown | 104 | 1.010 | 1.024 | 4.5% (14 runs) |  |
| lib/tfront_1k | 76 | 1.001 | 1.022 | 3.2% (14 runs) |  |
| programs/coroutines | 1289 | 0.961 | 1.036 | 9.9% (14 runs) |  |

### 5.4 plain vs 7413bff: regressions (instructions > +2% or cycles > +3%, >= 20 Mcyc)

| program | Mcyc | cyc ratio | ins ratio | cycle spread of readings | note |
|---|---|---|---|---|---|
| lib/rep_sep | 130 | 1.110 | 1.116 | 3.6% (5 runs) | instructions move the same way |
| lib/fmt_q | 159 | 1.089 | 0.986 | 2.1% (14 runs) |  |
| real/template_engine | 773 | 1.046 | 1.004 | 8.3% (14 runs) | instructions equal: layout / IPC |
| lib/gsub_tbl | 160 | 1.038 | 0.987 | 2.4% (14 runs) |  |
| lib/os_clock | 121 | 1.019 | 1.020 | 0.8% (5 runs) |  |
| lib/tinsert_idx | 207 | 1.014 | 1.023 | 20.7% (14 runs) |  |
| lib/tinsert_append | 194 | 1.002 | 1.023 | 16.1% (14 runs) |  |
| lib/m_fmod | 220 | 0.995 | 1.023 | 4.5% (5 runs) |  |
| lib/m_ult | 275 | 0.994 | 1.021 | 5.9% (14 runs) |  |
| corpus2/heldout/stream_closures | 800 | 0.954 | 1.128 | 2.0% (5 runs) |  |

### 5.4 PGO vs 7413bff: regressions (instructions > +2% or cycles > +3%, >= 20 Mcyc)

| program | Mcyc | cyc ratio | ins ratio | cycle spread of readings | note |
|---|---|---|---|---|---|
| lib/rep_sep | 120 | 1.103 | 1.156 | 1.7% (5 runs) | instructions move the same way |
| apps/events | 157 | 1.056 | 1.001 | 3.6% (14 runs) | instructions equal: layout / IPC |
| lib/tonumber_base | 275 | 1.054 | 1.017 | 10.2% (14 runs) | instructions move the same way |
| corpus2/heldout/real_dis_ppc | 1943 | 1.026 | 1.037 | 12.3% (14 runs) |  |
| lib/tinsert_idx | 194 | 1.015 | 1.025 | 8.0% (14 runs) |  |
| lib/tinsert_append | 182 | 1.003 | 1.025 | 9.6% (14 runs) |  |
| lib/tfront_1k | 77 | 1.001 | 1.024 | 3.8% (14 runs) |  |
| corpus2/heldout/stream_closures | 743 | 0.967 | 1.140 | 3.3% (5 runs) |  |
| programs/coroutines | 1317 | 0.953 | 1.030 | 5.2% (14 runs) |  |

### 5.5 plain: 15 biggest gaps to LuaJIT (ours/LJ cycles)

| program | Mcyc | cyc/LJ | ins/LJ |
|---|---|---|---|
| awfy/permute | 36 | 2.742 | 3.519 |
| apps/raytrace | 93 | 2.294 | 1.767 |
| corpus2/dev/text_adventure | 2620 | 2.208 | 2.164 |
| lib/fmt_s | 7 | 2.132 | 2.295 |
| awfy/cd | 229 | 2.126 | 2.051 |
| awfy/towers | 44 | 1.876 | 1.648 |
| awfy/json | 135 | 1.834 | 2.075 |
| corpus2/heldout/stream_closures | 760 | 1.827 | 2.059 |
| corpus/lua_lexer | 814 | 1.816 | 1.719 |
| lib/co_yield | 174 | 1.707 | 1.729 |
| awfy/queens | 93 | 1.697 | 1.878 |
| real/lua_interp | 100 | 1.690 | 2.164 |
| lib/getmt | 15 | 1.676 | 2.294 |
| lib/find_plain | 113 | 1.669 | 2.008 |
| jit-trace/type_entry | 1414 | 1.666 | 1.867 |

### 5.5 plain: 15 biggest wins over LuaJIT (>= 20 Mcyc)

| program | Mcyc | cyc/LJ | ins/LJ |
|---|---|---|---|
| lib/sel_loop | 207 | 0.039 | 0.062 |
| lib/error_tbl | 275 | 0.090 | 0.080 |
| jit-trace/int_rec2 | 640 | 0.128 | 1.398 |
| lib/pcall_err | 118 | 0.154 | 0.136 |
| programs/pcall_error | 1460 | 0.175 | 0.185 |
| lib/tfront_1k | 78 | 0.191 | 0.076 |
| lib/it_next | 44 | 0.245 | 0.676 |
| lib/match | 48 | 0.246 | 0.255 |
| lib/tonumber_flt | 199 | 0.256 | 0.393 |
| corpus/sha2_base64 | 367 | 0.260 | 0.300 |
| lib/find_pat | 218 | 0.309 | 0.371 |
| jit-trace/int_sum | 872 | 0.347 | 1.107 |
| apps/markdown | 121 | 0.366 | 0.269 |
| programs/oop_methods | 125 | 0.409 | 0.794 |
| lib/gsub_str | 163 | 0.409 | 0.502 |

### 5.5 PGO: 15 biggest gaps to LuaJIT (ours/LJ cycles)

| program | Mcyc | cyc/LJ | ins/LJ |
|---|---|---|---|
| awfy/permute | 36 | 2.715 | 3.512 |
| apps/raytrace | 87 | 2.153 | 1.669 |
| awfy/cd | 213 | 1.982 | 1.937 |
| lib/fmt_s | 6 | 1.980 | 2.215 |
| corpus2/dev/text_adventure | 2182 | 1.839 | 1.943 |
| awfy/towers | 43 | 1.819 | 1.627 |
| lib/find_plain | 117 | 1.739 | 1.772 |
| corpus2/heldout/stream_closures | 700 | 1.682 | 1.911 |
| lib/co_yield | 171 | 1.677 | 1.634 |
| real/lua_interp | 99 | 1.676 | 2.141 |
| jit-trace/type_entry | 1417 | 1.670 | 1.867 |
| awfy/queens | 91 | 1.661 | 1.856 |
| lib/getmt | 14 | 1.630 | 2.286 |
| lib/byte_multi | 33 | 1.623 | 2.922 |
| jit-trace/type_entry_stable | 1358 | 1.584 | 1.778 |

### 5.5 PGO: 15 biggest wins over LuaJIT (>= 20 Mcyc)

| program | Mcyc | cyc/LJ | ins/LJ |
|---|---|---|---|
| lib/sel_loop | 206 | 0.039 | 0.062 |
| lib/error_tbl | 216 | 0.071 | 0.062 |
| lib/pcall_err | 95 | 0.124 | 0.103 |
| jit-trace/int_rec2 | 639 | 0.127 | 1.398 |
| programs/pcall_error | 1229 | 0.147 | 0.150 |
| lib/tfront_1k | 76 | 0.187 | 0.071 |
| lib/tonumber_flt | 169 | 0.217 | 0.349 |
| lib/match | 43 | 0.224 | 0.235 |
| lib/it_next | 44 | 0.245 | 0.676 |
| corpus/sha2_base64 | 355 | 0.252 | 0.287 |
| lib/find_pat | 195 | 0.276 | 0.329 |
| apps/markdown | 104 | 0.316 | 0.249 |
| lib/gsub_str | 127 | 0.319 | 0.395 |
| jit-trace/int_sum | 871 | 0.347 | 1.107 |
| lib/tpack | 117 | 0.381 | 0.392 |

## Focus programs (cycles in M; ratios of cycles)

| program | plain Mcyc | PGO Mcyc | LJ Mcyc | PUC Mcyc | plain/LJ | PGO/LJ | plain/PUC | PGO/PUC | plain vs b cyc | ins | PGO vs b cyc | ins | plain vs 7413bff cyc | PGO vs 7413bff cyc | PGO/LJ 5a064d7 |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| programs/nbody | 340.7 | 340.1 | 345.6 | 4246.2 | 0.986 | 0.984 | 0.080 | 0.080 | 0.993 | 0.973 | 0.985 | 0.974 | 0.997 | 0.999 | 0.999 |
| programs/fib | 623.0 | 624.4 | 469.7 | 4139.5 | 1.326 | 1.329 | 0.151 | 0.151 | 0.987 | 0.984 | 0.989 | 0.984 | 0.991 | 0.995 | 1.343 |
| programs/binary_trees | 1170.7 | 979.4 | 2007.4 | 4389.1 | 0.583 | 0.488 | 0.267 | 0.223 | 0.962 | 0.983 | 0.951 | 0.989 | 0.991 | 0.994 | 0.513 |
| programs/queens | 1810.2 | 1809.8 | 1907.1 | 4368.7 | 0.949 | 0.949 | 0.414 | 0.414 | 0.972 | 0.922 | 0.980 | 0.922 | 0.979 | 0.981 | 0.968 |
| apps/raytrace | 93.1 | 87.4 | 40.6 | 262.7 | 2.294 | 2.153 | 0.354 | 0.332 | 0.833 | 0.815 | 0.838 | 0.825 | 0.860 | 0.864 | 2.569 |
| awfy/cd | 228.9 | 213.3 | 107.6 | 437.5 | 2.126 | 1.982 | 0.523 | 0.488 | 0.895 | 0.829 | 0.892 | 0.839 | 0.899 | 0.885 | 2.221 |
| awfy/list | 93.6 | 91.8 | 88.9 | 418.8 | 1.053 | 1.033 | 0.223 | 0.219 | 0.998 | 0.949 | 0.999 | 0.950 | 0.995 | 0.991 | 1.034 |
| awfy/richards | 56.4 | 54.5 | 34.7 | 197.7 | 1.624 | 1.570 | 0.285 | 0.276 | 0.946 | 0.931 | 0.952 | 0.939 | 0.916 | 0.919 | 1.650 |
| awfy/deltablue | 101.5 | 96.1 | 103.5 | 157.9 | 0.981 | 0.929 | 0.643 | 0.608 | 0.949 | 0.945 | 0.964 | 0.966 | 0.951 | 0.971 | 0.963 |
| awfy/havlak | 145.6 | 129.0 | 115.2 | 217.5 | 1.264 | 1.120 | 0.670 | 0.593 | 0.988 | 0.983 | 0.993 | 0.991 | 0.964 | 0.983 | 1.128 |
| awfy/permute | 36.0 | 35.6 | 13.1 | 246.3 | 2.742 | 2.715 | 0.146 | 0.145 | 0.993 | 0.980 | 0.997 | 0.982 | 0.986 | 0.989 | 2.724 |
| awfy/towers | 44.0 | 42.7 | 23.5 | 215.1 | 1.876 | 1.819 | 0.205 | 0.198 | 0.942 | 0.958 | 0.938 | 0.966 | 0.945 | 0.940 | 1.939 |
| corpus/lua_lexer | 814.1 | 708.7 | 448.2 | 3281.3 | 1.816 | 1.581 | 0.248 | 0.216 | 0.754 | 0.743 | 0.739 | 0.736 | 0.834 | 0.817 | 2.138 |
| corpus/classic_vector | 301.2 | 288.3 | 240.7 | 2945.5 | 1.252 | 1.198 | 0.102 | 0.098 | 0.639 | 0.636 | 0.666 | 0.651 | 0.648 | 0.668 | 1.797 |
| corpus2/heldout/dijkstra_routes | 1123.3 | 1097.5 | 1265.1 | 3142.9 | 0.888 | 0.868 | 0.357 | 0.349 | 0.958 | 0.937 | 0.964 | 0.937 | 0.958 | 0.961 | 0.900 |
| corpus2/dev/oop_mixins | 1096.7 | 1030.8 | 865.1 | 2464.3 | 1.268 | 1.192 | 0.445 | 0.418 | 0.786 | 0.712 | 0.759 | 0.701 | 0.977 | 0.962 | 1.570 |
| jit-trace/rec_loop | 76.3 | 75.6 | 141.9 | 1440.3 | 0.538 | 0.532 | 0.053 | 0.052 | 0.828 | 0.802 | 0.828 | 0.803 | 0.937 | 0.945 | 0.643 |
| corpus2/heldout/roguelike_turns | 799.2 | 756.7 | 623.3 | 3311.7 | 1.282 | 1.214 | 0.241 | 0.229 | 0.993 | 0.966 | 0.986 | 0.968 | 0.939 | 0.941 | 1.231 |
| corpus2/heldout/vending_fsm | 964.9 | 879.7 | 824.2 | 3290.3 | 1.171 | 1.067 | 0.293 | 0.267 | 0.947 | 0.953 | 0.933 | 0.951 | 0.987 | 0.982 | 1.144 |
| corpus2/heldout/real_dis_mips | 2067.2 | 1810.1 | 1786.6 | 3808.7 | 1.157 | 1.013 | 0.543 | 0.475 | 0.944 | 0.952 | 0.955 | 0.966 | 0.940 | 0.970 | 1.061 |
| awfy/queens | 93.4 | 91.4 | 55.0 | 401.8 | 1.697 | 1.661 | 0.232 | 0.227 | 0.968 | 0.971 | 0.975 | 0.973 | 0.976 | 0.977 | 1.704 |

## Per-program table (5.5 and 5.4; Mcyc, ratios vs 5a064d7 and LJ)

| program | n runs (plain) | plain | PGO | p54 | g54 | LJ | PUC55 | PUC54 | plain/LJ | PGO/LJ | plain vs b cyc | ins | PGO vs b cyc | ins | p54 vs b cyc | ins | g54 vs b cyc | ins | out/exp (all 4) |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| apps/events | 14 | 156.9 | 138.7 | 177.8 | 156.9 | 95.4 | 293.7 | 289.5 | 1.645 | 1.454 | 0.958 | 0.999 | 0.991 | 1.002 | 1.007 | 0.998 | 1.022 | 1.000 | ok |
| apps/expr_lang | 14 | 131.2 | 117.3 | 141.9 | 127.0 | 96.7 | 241.7 | 253.2 | 1.356 | 1.213 | 0.967 | 0.908 | 0.939 | 0.912 | 0.890 | 0.884 | 0.870 | 0.887 | ok |
| apps/lru_cache | 5 | 143.4 | 131.7 | 176.8 | 169.3 | 106.4 | 219.8 | 253.9 | 1.347 | 1.238 | 1.003 | 0.936 | 0.999 | 0.943 | 0.987 | 0.953 | 0.999 | 0.958 | ok |
| apps/markdown | 14 | 120.8 | 104.2 | 123.2 | 106.2 | 329.7 | 412.6 | 390.8 | 0.366 | 0.316 | 1.036 | 1.006 | 1.026 | 1.023 | 1.020 | 0.995 | 0.992 | 1.002 | ok |
| apps/raytrace | 14 | 93.1 | 87.4 | 103.2 | 95.0 | 40.6 | 262.7 | 281.7 | 2.294 | 2.153 | 0.833 | 0.815 | 0.838 | 0.825 | 0.877 | 0.852 | 0.869 | 0.861 | ok |
| awfy/bounce | 5 | 60.0 | 59.1 | 63.9 | 62.3 | 47.7 | 511.2 | 537.1 | 1.259 | 1.239 | 0.997 | 0.964 | 1.003 | 0.966 | 0.964 | 0.947 | 0.961 | 0.951 | ok |
| awfy/cd | 14 | 228.9 | 213.3 | 251.5 | 233.9 | 107.6 | 437.5 | 464.4 | 2.126 | 1.982 | 0.895 | 0.829 | 0.892 | 0.839 | 0.887 | 0.837 | 0.892 | 0.847 | ok |
| awfy/deltablue | 14 | 101.5 | 96.1 | 109.8 | 102.7 | 103.5 | 157.9 | 165.2 | 0.981 | 0.929 | 0.949 | 0.945 | 0.964 | 0.966 | 0.933 | 0.944 | 0.935 | 0.959 | ok |
| awfy/havlak | 14 | 145.6 | 129.0 | 171.9 | 154.1 | 115.2 | 217.5 | 225.5 | 1.264 | 1.120 | 0.988 | 0.983 | 0.993 | 0.991 | 0.993 | 0.985 | 0.996 | 0.992 | ok |
| awfy/json | 5 | 134.6 | 113.9 | 150.2 | 129.1 | 73.4 | 262.9 | 271.8 | 1.834 | 1.552 | 0.962 | 0.962 | 0.943 | 0.968 | 0.987 | 0.967 | 0.987 | 0.975 | ok |
| awfy/list | 14 | 93.6 | 91.8 | 94.5 | 92.4 | 88.9 | 418.8 | 477.9 | 1.053 | 1.033 | 0.998 | 0.949 | 0.999 | 0.950 | 0.977 | 0.954 | 0.973 | 0.956 | ok |
| awfy/mandelbrot | 5 | 272.7 | 272.1 | 272.4 | 272.2 | 273.1 | 1529.3 | 1379.3 | 0.999 | 0.996 | 1.000 | 0.991 | 0.999 | 0.991 | 0.999 | 0.990 | 0.999 | 0.991 | ok |
| awfy/nbody | 5 | 91.1 | 90.7 | 94.0 | 93.5 | 94.6 | 1504.3 | 1603.8 | 0.963 | 0.959 | 0.982 | 0.978 | 0.991 | 0.979 | 0.984 | 0.977 | 0.988 | 0.978 | ok |
| awfy/permute | 14 | 36.0 | 35.6 | 37.0 | 36.5 | 13.1 | 246.3 | 257.2 | 2.742 | 2.715 | 0.993 | 0.980 | 0.997 | 0.982 | 0.998 | 0.980 | 0.984 | 0.982 | ok |
| awfy/queens | 14 | 93.4 | 91.4 | 78.7 | 76.4 | 55.0 | 401.8 | 403.4 | 1.697 | 1.661 | 0.968 | 0.971 | 0.975 | 0.973 | 0.957 | 0.957 | 0.960 | 0.958 | ok |
| awfy/richards | 14 | 56.4 | 54.5 | 57.0 | 55.0 | 34.7 | 197.7 | 199.3 | 1.624 | 1.570 | 0.946 | 0.931 | 0.952 | 0.939 | 0.955 | 0.932 | 0.957 | 0.939 | ok |
| awfy/sieve | 14 | 110.5 | 111.2 | 114.5 | 113.2 | 156.4 | 1032.3 | 1085.2 | 0.707 | 0.711 | 0.900 | 0.850 | 0.908 | 0.850 | 0.901 | 0.874 | 0.880 | 0.874 | ok |
| awfy/storage | 5 | 102.3 | 78.2 | 146.4 | 114.8 | 154.0 | 255.9 | 262.0 | 0.664 | 0.508 | 1.010 | 1.010 | 1.001 | 1.014 | 1.002 | 1.007 | 1.015 | 1.008 | ok |
| awfy/towers | 14 | 44.0 | 42.7 | 44.9 | 43.3 | 23.5 | 215.1 | 213.9 | 1.876 | 1.819 | 0.942 | 0.958 | 0.938 | 0.966 | 0.937 | 0.959 | 0.936 | 0.966 | ok |
| corpus/astar_jumper | 14 | 1558.7 | 1484.0 | 1599.3 | 1536.9 | 997.4 | 3280.6 | 3358.7 | 1.563 | 1.488 | 0.942 | 0.894 | 0.937 | 0.882 | 0.866 | 0.823 | 0.869 | 0.819 | ok |
| corpus/class_middleclass | 14 | 463.4 | 455.5 | 490.3 | 481.3 | 319.5 | 3046.3 | 3098.9 | 1.450 | 1.426 | 0.966 | 0.971 | 0.974 | 0.973 | 0.966 | 0.972 | 0.973 | 0.974 | ok |
| corpus/classic_vector | 14 | 301.2 | 288.3 | 436.8 | 399.6 | 240.7 | 2945.5 | 2905.4 | 1.252 | 1.198 | 0.639 | 0.636 | 0.666 | 0.651 | 0.672 | 0.650 | 0.710 | 0.685 | ok |
| corpus/containers_lru_heap | 5 | 1292.9 | 1212.9 | 1653.8 | 1562.5 | 1417.3 | 2675.7 | 3035.3 | 0.912 | 0.856 | 1.010 | 0.989 | 1.011 | 0.989 | 0.994 | 0.990 | 1.005 | 0.989 | ok |
| corpus/coro_scheduler | 14 | 1408.3 | 1286.0 | 1504.6 | 1376.3 | 941.9 | 2772.8 | 2851.7 | 1.495 | 1.365 | 0.938 | 0.970 | 0.985 | 1.019 | 0.931 | 0.970 | 0.982 | 1.001 | ok |
| corpus/csv_ini_yaml | 14 | 1476.8 | 1261.8 | 1669.3 | 1440.0 | 1771.4 | 2948.4 | 3020.4 | 0.834 | 0.712 | 0.946 | 0.914 | 0.939 | 0.912 | 0.890 | 0.891 | 0.910 | 0.886 | ok |
| corpus/inspect_lua | 14 | 1470.3 | 1310.4 | 1593.0 | 1410.5 | 1056.4 | 2901.3 | 2968.0 | 1.392 | 1.240 | 0.941 | 0.986 | 0.943 | 0.987 | 0.951 | 0.988 | 0.938 | 0.983 | ok |
| corpus/json_rxi | 5 | 1816.8 | 1542.4 | 1885.2 | 1597.5 | 1222.9 | 3647.9 | 3646.8 | 1.486 | 1.261 | 1.000 | 0.999 | 1.018 | 1.001 | 0.989 | 0.995 | 1.010 | 0.996 | ok |
| corpus/lua_lexer | 14 | 814.1 | 708.7 | 844.3 | 737.5 | 448.2 | 3281.3 | 3358.7 | 1.816 | 1.581 | 0.754 | 0.743 | 0.739 | 0.736 | 0.760 | 0.755 | 0.743 | 0.751 | ok |
| corpus/markdown_conv | 14 | 1323.0 | 1184.1 | 1344.1 | 1228.8 | 3029.8 | 3746.9 | 3671.3 | 0.437 | 0.391 | 0.955 | 0.945 | 0.952 | 0.939 | 0.957 | 0.958 | 0.975 | 0.955 | ok |
| corpus/msgpack_pure | 5 | 2012.2 | 1791.1 | 2167.5 | 1857.7 | 4473.7 | 3436.5 | 3601.3 | 0.450 | 0.400 | 0.999 | 0.989 | 1.028 | 1.000 | 0.997 | 0.987 | 0.988 | 0.984 | ok |
| corpus/mustache_lustache | 14 | 1826.8 | 1424.8 | 1903.0 | 1535.2 | 1537.1 | 3296.2 | 3446.8 | 1.188 | 0.927 | 1.020 | 0.990 | 0.991 | 0.998 | 0.977 | 0.980 | 0.977 | 0.992 | ok |
| corpus/parser_combinator | 14 | 2325.6 | 2007.4 | 2349.6 | 1891.9 | 1563.1 | 3390.6 | 3391.8 | 1.488 | 1.284 | 1.020 | 1.032 | 1.075 | 1.047 | 0.934 | 0.985 | 0.906 | 0.989 | ok |
| corpus/penlight_mix | 5 | 2216.6 | 2108.1 | 2309.6 | 2175.4 | 2678.7 | 3992.1 | 3970.3 | 0.827 | 0.787 | 0.967 | 0.968 | 0.991 | 0.978 | 0.972 | 0.966 | 0.972 | 0.972 | ok |
| corpus/serpent_ser | 5 | 2020.8 | 1745.2 | 2057.9 | 1792.5 | 2426.7 | 3260.6 | 3254.8 | 0.833 | 0.719 | 1.011 | 0.979 | 1.010 | 0.978 | 0.985 | 0.977 | 0.997 | 0.980 | ok |
| corpus/sha2_base64 | 5 | 367.3 | 355.4 | 367.1 | 350.9 | 1411.3 | 3328.6 | 3384.1 | 0.260 | 0.252 | 0.949 | 0.969 | 0.968 | 0.973 | 0.949 | 0.969 | 0.948 | 0.973 | ok |
| corpus/template_etlua | 14 | 1581.2 | 1288.7 | 1592.0 | 1298.9 | 1826.0 | 2680.1 | 2700.4 | 0.866 | 0.706 | 1.003 | 0.995 | 1.043 | 1.000 | 1.015 | 0.984 | 1.018 | 0.993 | ok |
| corpus/testrunner_luaunit | 5 | 1435.1 | 1170.7 | 1569.9 | 1315.4 | 1390.7 | 2601.9 | 2628.8 | 1.032 | 0.842 | 0.974 | 0.955 | 0.957 | 0.961 | 0.970 | 0.981 | 0.986 | 0.996 | ok |
| corpus2/dev/bignum_arith | 5 | 776.4 | 714.2 | 860.8 | 800.4 | 660.2 | 2352.8 | 2412.4 | 1.176 | 1.082 | 0.988 | 0.968 | 0.985 | 0.967 | 0.996 | 0.967 | 0.995 | 0.965 | ok |
| corpus2/dev/coro_generators | 14 | 963.1 | 908.7 | 990.6 | 929.8 | 713.4 | 1807.9 | 1836.1 | 1.350 | 1.274 | 0.769 | 0.939 | 0.944 | 0.996 | 0.763 | 0.931 | 0.937 | 0.970 | ok |
| corpus2/dev/csv_report | 14 | 1148.2 | 1009.3 | 1152.1 | 1015.9 | 1262.6 | 2165.3 | 2381.4 | 0.909 | 0.799 | 1.015 | 1.016 | 0.997 | 1.013 | 0.993 | 0.968 | 0.967 | 0.965 | ok |
| corpus2/dev/diff_levenshtein | 5 | 444.4 | 396.2 | 435.3 | 407.1 | 329.3 | 1776.3 | 1850.8 | 1.349 | 1.203 | 1.041 | 0.981 | 0.982 | 0.982 | 1.003 | 0.984 | 1.015 | 0.984 | ok |
| corpus2/dev/ecs_world | 5 | 922.8 | 831.5 | 984.2 | 858.6 | 916.1 | 2115.3 | 2160.1 | 1.007 | 0.908 | 0.982 | 0.975 | 0.993 | 0.979 | 0.990 | 0.990 | 0.999 | 1.002 | ok |
| corpus2/dev/event_promise | 14 | 371.6 | 356.0 | 447.1 | 434.9 | 467.9 | 1906.6 | 1939.2 | 0.794 | 0.761 | 0.998 | 0.965 | 1.011 | 0.968 | 0.986 | 0.968 | 1.006 | 0.969 | ok |
| corpus2/dev/expr_compiler | 14 | 1142.7 | 976.5 | 1189.2 | 1022.1 | 1080.2 | 2081.3 | 2231.0 | 1.058 | 0.904 | 0.866 | 0.884 | 0.867 | 0.879 | 0.880 | 0.883 | 0.875 | 0.879 | ok |
| corpus2/dev/iter_pipeline | 14 | 172.4 | 164.5 | 180.0 | 170.0 | 132.2 | 2865.2 | 2874.5 | 1.304 | 1.244 | 0.993 | 0.980 | 0.995 | 0.986 | 0.990 | 0.980 | 0.972 | 0.983 | ok |
| corpus2/dev/lua_parser | 14 | 1159.0 | 982.4 | 1259.1 | 1079.2 | 790.7 | 2461.5 | 2499.0 | 1.466 | 1.243 | 0.920 | 0.855 | 0.881 | 0.847 | 0.822 | 0.800 | 0.823 | 0.787 | ok |
| corpus2/dev/memo_cache | 14 | 1561.4 | 1444.5 | 1608.4 | 1485.3 | 1674.7 | 2669.5 | 2763.8 | 0.932 | 0.863 | 0.972 | 0.941 | 0.974 | 0.935 | 0.986 | 0.964 | 0.983 | 0.966 | ok |
| corpus2/dev/oop_mixins | 14 | 1096.7 | 1030.8 | 1105.5 | 1038.2 | 865.1 | 2464.3 | 2520.8 | 1.268 | 1.192 | 0.786 | 0.712 | 0.759 | 0.701 | 0.773 | 0.722 | 0.785 | 0.716 | ok |
| corpus2/dev/particles_physics | 5 | 387.4 | 364.1 | 500.7 | 463.7 | 379.7 | 1719.5 | 1811.4 | 1.020 | 0.959 | 0.966 | 0.961 | 0.977 | 0.967 | 0.990 | 0.969 | 0.978 | 0.973 | ok |
| corpus2/dev/real_dis_arm | 5 | 2472.6 | 2196.0 | 2412.0 | 2162.6 | 1947.2 | 4487.3 | 4484.7 | 1.270 | 1.128 | 1.019 | 0.985 | 1.009 | 0.994 | 0.991 | 0.985 | 1.000 | 0.996 | ok |
| corpus2/dev/real_dis_x86 | 5 | 1570.2 | 1392.0 | 1614.1 | 1470.0 | 1591.6 | 2708.6 | 2688.6 | 0.987 | 0.875 | 1.030 | 0.993 | 1.037 | 0.999 | 1.012 | 0.998 | 1.031 | 1.005 | ok |
| corpus2/dev/regex_nfa | 14 | 673.3 | 550.9 | 1294.4 | 1151.4 | 554.5 | 2510.1 | 2439.3 | 1.214 | 0.994 | 0.919 | 0.974 | 0.977 | 0.975 | 0.927 | 0.985 | 0.952 | 0.994 | ok |
| corpus2/dev/template_gsub | 5 | 1684.5 | 1419.5 | 1737.1 | 1480.0 | 2051.0 | 2892.7 | 2885.0 | 0.821 | 0.692 | 0.987 | 0.983 | 1.010 | 1.002 | 0.989 | 0.981 | 0.996 | 0.997 | ok |
| corpus2/dev/text_adventure | 5 | 2619.8 | 2181.7 | 2615.4 | 2202.6 | 1186.6 | 3375.1 | 3513.0 | 2.208 | 1.839 | 1.025 | 0.997 | 1.018 | 1.011 | 0.979 | 0.990 | 0.994 | 1.005 | ok |
| corpus2/dev/varargs_util | 5 | 1527.2 | 1246.6 | 1550.9 | 1286.7 | 1413.9 | 2284.8 | 2301.9 | 1.080 | 0.882 | 1.017 | 0.995 | 1.005 | 0.999 | 1.011 | 0.991 | 1.020 | 1.000 | ok |
| corpus2/dev/wordfreq_wrap | 14 | 606.2 | 494.6 | 641.8 | 551.5 | 658.1 | 1219.2 | 1225.4 | 0.921 | 0.752 | 1.010 | 1.000 | 1.007 | 1.005 | 0.997 | 0.999 | 1.012 | 1.006 | ok |
| corpus2/heldout/bigint_rsa | 14 | 1112.1 | 928.7 | 1362.1 | 1159.7 | 1074.5 | 3033.1 | 3149.9 | 1.035 | 0.864 | 0.990 | 1.000 | 0.987 | 1.001 | 0.995 | 0.993 | 0.973 | 0.993 | ok |
| corpus2/heldout/dijkstra_routes | 14 | 1123.3 | 1097.5 | 1422.3 | 1390.0 | 1265.1 | 3142.9 | 3226.1 | 0.888 | 0.868 | 0.958 | 0.937 | 0.964 | 0.937 | 0.981 | 0.961 | 0.992 | 0.963 | ok |
| corpus2/heldout/forth_vm | 5 | 798.1 | 788.6 | 996.4 | 1102.3 | 696.1 | 2965.2 | 3269.3 | 1.147 | 1.133 | 0.996 | 0.971 | 0.986 | 0.971 | 0.948 | 0.940 | 0.945 | 0.939 | ok |
| corpus2/heldout/log_analyzer | 5 | 2224.7 | 1902.0 | 2195.8 | 1898.1 | 1652.2 | 3115.0 | 3178.7 | 1.346 | 1.151 | 1.058 | 0.978 | 1.088 | 0.976 | 1.017 | 0.983 | 1.014 | 0.983 | ok |
| corpus2/heldout/markov_text | 5 | 1101.7 | 1091.9 | 1163.5 | 1133.6 | 982.0 | 2862.9 | 2851.2 | 1.122 | 1.112 | 0.976 | 0.985 | 0.994 | 0.966 | 0.978 | 0.983 | 0.990 | 0.967 | ok |
| corpus2/heldout/prototype_objects | 14 | 1431.0 | 1340.3 | 1375.0 | 1348.0 | 1280.6 | 2532.8 | 2721.4 | 1.117 | 1.047 | 1.034 | 0.999 | 1.035 | 1.004 | 1.001 | 0.983 | 1.022 | 0.986 | ok |
| corpus2/heldout/real_dis_mips | 14 | 2067.2 | 1810.1 | 2092.1 | 1846.9 | 1786.6 | 3808.7 | 3869.0 | 1.157 | 1.013 | 0.944 | 0.952 | 0.955 | 0.966 | 0.960 | 0.968 | 0.988 | 0.985 | ok |
| corpus2/heldout/real_dis_ppc | 14 | 2256.7 | 1952.2 | 2213.3 | 1942.8 | 1747.6 | 3961.9 | 3962.8 | 1.291 | 1.117 | 1.083 | 1.043 | 1.050 | 1.057 | 0.998 | 0.999 | 1.004 | 1.015 | ok |
| corpus2/heldout/roguelike_turns | 14 | 799.2 | 756.7 | 830.3 | 801.1 | 623.3 | 3311.7 | 3473.6 | 1.282 | 1.214 | 0.993 | 0.966 | 0.986 | 0.968 | 0.975 | 0.969 | 0.969 | 0.972 | ok |
| corpus2/heldout/scheme_interp | 14 | 1389.5 | 1218.0 | 1496.9 | 1292.7 | 1045.1 | 3052.1 | 3021.0 | 1.330 | 1.165 | 0.962 | 0.984 | 1.008 | 0.989 | 0.990 | 0.991 | 0.991 | 0.997 | ok |
| corpus2/heldout/stream_closures | 5 | 760.1 | 699.5 | 799.7 | 743.2 | 416.0 | 3032.0 | 3163.1 | 1.827 | 1.682 | 0.967 | 0.960 | 0.970 | 0.965 | 0.941 | 1.120 | 0.953 | 1.138 | ok |
| corpus2/heldout/sudoku_solver | 5 | 1279.0 | 1150.1 | 1613.7 | 1471.1 | 1387.6 | 3013.0 | 3102.7 | 0.922 | 0.829 | 0.980 | 0.973 | 0.980 | 0.972 | 0.994 | 0.976 | 0.997 | 0.981 | ok |
| corpus2/heldout/vending_fsm | 14 | 964.9 | 879.7 | 1006.7 | 928.0 | 824.2 | 3290.3 | 3377.2 | 1.171 | 1.067 | 0.947 | 0.953 | 0.933 | 0.951 | 0.921 | 0.882 | 0.918 | 0.877 | ok |
| jit-trace/alloc_ctor | 14 | 185.5 | 155.2 | 240.0 | 200.1 | 276.5 | 1652.1 | 2197.8 | 0.671 | 0.561 | 0.992 | 0.990 | 0.985 | 0.989 | 0.931 | 0.990 | 0.992 | 0.986 | ok |
| jit-trace/array_rec | 5 | 1792.9 | 1792.5 | 1821.2 | 1820.5 | 1755.9 | 9671.5 | 10022.8 | 1.021 | 1.021 | 0.998 | 0.769 | 0.998 | 0.769 | 1.000 | 0.773 | 1.000 | 0.773 | ok |
| jit-trace/dot | 14 | 1221.1 | 1222.9 | 1224.6 | 1222.6 | 1166.0 | 11124.9 | 11807.8 | 1.047 | 1.049 | 1.003 | 0.699 | 1.002 | 0.699 | 1.002 | 0.701 | 0.999 | 0.701 | ok |
| jit-trace/field_global | 5 | 530.0 | 528.3 | 532.4 | 527.5 | 491.7 | 17948.4 | 17879.3 | 1.078 | 1.074 | 0.974 | 0.901 | 0.986 | 0.902 | 0.986 | 0.901 | 0.994 | 0.900 | ok |
| jit-trace/field_method | 14 | 500.7 | 537.2 | 501.9 | 501.7 | 423.1 | 20185.1 | 20271.4 | 1.183 | 1.270 | 0.882 | 0.871 | 0.955 | 0.872 | 0.879 | 0.871 | 0.888 | 0.872 | ok |
| jit-trace/field_rw | 5 | 949.5 | 949.1 | 1031.6 | 1048.5 | 929.3 | 19574.6 | 19882.6 | 1.022 | 1.021 | 1.001 | 0.931 | 1.001 | 0.931 | 0.998 | 0.939 | 1.015 | 0.939 | ok |
| jit-trace/float_loop | 2 | 2772.0 | 2772.3 | 2793.8 | 2796.6 | 2575.9 | 11381.5 | 13567.0 | 1.076 | 1.076 | 0.997 | 0.999 | 0.998 | 1.000 | 0.998 | 1.000 | 1.001 | 1.000 | ok |
| jit-trace/float_poly | 5 | 1783.2 | 1783.0 | 1818.3 | 1817.4 | 1768.0 | 7918.6 | 8940.5 | 1.009 | 1.009 | 1.000 | 0.880 | 1.000 | 0.882 | 1.000 | 0.888 | 1.000 | 0.887 | ok |
| jit-trace/inline_2 | 5 | 326.8 | 325.6 | 326.8 | 325.6 | 262.5 | 11057.3 | 10600.9 | 1.245 | 1.241 | 1.001 | 0.906 | 0.996 | 0.906 | 1.002 | 0.906 | 0.999 | 0.906 | ok |
| jit-trace/inline_leaf | 14 | 406.6 | 406.1 | 407.6 | 407.6 | 630.2 | 11902.9 | 12019.3 | 0.645 | 0.644 | 0.897 | 0.876 | 0.896 | 0.877 | 0.898 | 0.876 | 0.897 | 0.877 | ok |
| jit-trace/int_rec2 | 14 | 640.1 | 638.8 | 644.9 | 645.0 | 5016.4 | 12493.6 | 13006.3 | 0.128 | 0.127 | 0.945 | 0.901 | 0.945 | 0.901 | 0.941 | 0.902 | 0.943 | 0.902 | ok |
| jit-trace/int_sum | 5 | 872.2 | 871.4 | 896.8 | 896.7 | 2512.8 | 9858.5 | 12300.9 | 0.347 | 0.347 | 0.982 | 0.870 | 0.981 | 0.870 | 0.984 | 0.874 | 0.987 | 0.874 | ok |
| jit-trace/licm_expr | 5 | 463.0 | 463.0 | 466.1 | 466.4 | 442.6 | 12323.9 | 12827.3 | 1.046 | 1.046 | 0.993 | 0.729 | 0.993 | 0.729 | 1.000 | 0.804 | 0.999 | 0.804 | ok |
| jit-trace/multret | 5 | 148.9 | 148.2 | 149.0 | 148.4 | 156.7 | 5879.7 | 5782.8 | 0.950 | 0.946 | 0.988 | 0.856 | 0.990 | 0.856 | 0.992 | 0.856 | 0.991 | 0.856 | ok |
| jit-trace/oop_inherit | 14 | 49.5 | 49.2 | 48.8 | 48.4 | 112.8 | 2153.1 | 2201.0 | 0.439 | 0.436 | 0.863 | 0.937 | 0.862 | 0.939 | 0.998 | 0.910 | 0.996 | 0.911 | ok |
| jit-trace/oop_override | 14 | 103.3 | 102.9 | 106.3 | 105.7 | 136.7 | 645.6 | 666.3 | 0.756 | 0.753 | 0.965 | 0.969 | 0.964 | 0.970 | 0.959 | 0.970 | 0.957 | 0.971 | ok |
| jit-trace/rec_loop | 14 | 76.3 | 75.6 | 79.6 | 78.7 | 141.9 | 1440.3 | 1449.9 | 0.538 | 0.532 | 0.828 | 0.802 | 0.828 | 0.803 | 0.897 | 0.806 | 0.894 | 0.807 | ok |
| jit-trace/short_inner | 5 | 1089.4 | 1086.4 | 994.4 | 994.1 | 878.8 | 10393.4 | 11908.6 | 1.240 | 1.236 | 0.989 | 0.984 | 0.987 | 0.984 | 1.000 | 0.998 | 1.002 | 0.999 | ok |
| jit-trace/sink_tuple | 5 | 13.5 | 13.0 | 13.4 | 13.0 | 20.0 | 2743.7 | 3358.1 | 0.674 | 0.652 | 1.016 | 0.950 | 1.015 | 0.953 | 1.021 | 0.949 | 1.011 | 0.952 | ok |
| jit-trace/sink_vec | 5 | 15.4 | 15.1 | 15.4 | 15.1 | 20.0 | 1489.0 | 1832.3 | 0.772 | 0.758 | 1.003 | 0.909 | 1.014 | 0.910 | 1.014 | 0.909 | 1.016 | 0.910 | ok |
| jit-trace/str_concat | 5 | 538.2 | 512.3 | 592.6 | 551.7 | 501.6 | 1628.5 | 1630.2 | 1.073 | 1.021 | 1.013 | 0.997 | 1.006 | 1.012 | 0.997 | 0.997 | 0.994 | 1.011 | ok |
| jit-trace/tail_loop | 5 | 236.3 | 235.6 | 242.9 | 242.0 | 223.3 | 6724.0 | 6643.8 | 1.058 | 1.055 | 0.950 | 0.919 | 0.948 | 0.919 | 0.969 | 0.920 | 0.960 | 0.920 | ok |
| jit-trace/tfor_loops | 14 | 222.0 | 222.4 | 227.3 | 227.4 | 362.3 | 3623.0 | 3352.6 | 0.613 | 0.614 | 1.022 | 0.988 | 1.009 | 0.988 | 0.995 | 0.986 | 1.016 | 0.987 | ok |
| jit-trace/type_entry | 5 | 1413.9 | 1417.5 | 1579.1 | 1575.4 | 848.7 | 11725.8 | 12234.3 | 1.666 | 1.670 | 0.933 | 0.779 | 0.937 | 0.779 | 0.992 | 0.847 | 0.998 | 0.847 | ok |
| jit-trace/type_entry_stable | 5 | 1357.7 | 1357.7 | 1492.9 | 1506.1 | 856.9 | 11690.0 | 12248.4 | 1.584 | 1.584 | 0.936 | 0.768 | 0.936 | 0.768 | 0.989 | 0.838 | 0.989 | 0.838 | ok |
| jit-trace/type_late | 5 | 1518.4 | 1520.4 | 1537.3 | 1533.3 | 1461.5 | 10901.2 | 11418.6 | 1.039 | 1.040 | 0.998 | 0.802 | 0.999 | 0.803 | 1.005 | 0.804 | 1.002 | 0.804 | ok |
| jit-trace/type_late_stable | 5 | 1507.3 | 1506.7 | 1533.5 | 1533.8 | 1461.0 | 10905.6 | 11410.8 | 1.032 | 1.031 | 0.991 | 0.726 | 0.992 | 0.726 | 1.001 | 0.804 | 1.003 | 0.804 | ok |
| jit-trace/upval_acc | 14 | 1022.3 | 1021.3 | 1174.1 | 1172.4 | 1356.5 | 9970.0 | 10500.8 | 0.754 | 0.753 | 0.740 | 0.812 | 0.739 | 0.812 | 0.848 | 0.803 | 0.846 | 0.803 | ok |
| jit-trace/upval_inv | 5 | 2115.0 | 2115.1 | 2130.4 | 2135.6 | 1985.3 | 13886.5 | 14901.3 | 1.065 | 1.065 | 0.991 | 0.710 | 0.992 | 0.710 | 0.995 | 0.845 | 0.998 | 0.845 | ok |
| lib/assert_ok | 5 | 16.9 | 17.4 | 19.6 | 19.3 | 23.7 | 295.9 | 329.8 | 0.713 | 0.734 | 0.892 | 0.777 | 0.932 | 0.778 | 0.967 | 0.849 | 0.948 | 0.850 | ok |
| lib/byte | 14 | 44.0 | 43.4 | 44.1 | 43.6 | 33.0 | 776.0 | 845.1 | 1.332 | 1.315 | 0.874 | 0.877 | 0.871 | 0.878 | 0.867 | 0.877 | 0.866 | 0.878 | ok |
| lib/byte_multi | 14 | 33.0 | 32.6 | 33.2 | 32.7 | 20.1 | 336.9 | 366.2 | 1.640 | 1.623 | 0.804 | 0.820 | 0.791 | 0.821 | 0.809 | 0.820 | 0.801 | 0.821 | ok |
| lib/char | 5 | 149.4 | 136.8 | 158.7 | 146.2 | 104.8 | 383.6 | 387.1 | 1.426 | 1.306 | 0.989 | 0.986 | 0.989 | 0.982 | 1.021 | 0.996 | 1.018 | 0.993 | ok |
| lib/co_wrap | 14 | 226.6 | 196.3 | 213.2 | 189.9 | 228.7 | 381.5 | 338.9 | 0.991 | 0.858 | 0.939 | 0.955 | 0.987 | 0.990 | 0.887 | 0.955 | 0.973 | 0.997 | ok |
| lib/co_yield | 14 | 174.1 | 171.0 | 175.8 | 172.5 | 102.0 | 483.9 | 475.0 | 1.707 | 1.677 | 0.730 | 0.919 | 0.949 | 1.004 | 0.728 | 0.919 | 0.951 | 0.997 | ok |
| lib/concat_op | 5 | 140.4 | 129.5 | 143.0 | 136.0 | 212.9 | 419.4 | 445.3 | 0.659 | 0.608 | 1.002 | 0.994 | 1.002 | 1.009 | 1.005 | 0.993 | 1.019 | 1.007 | ok |
| lib/concat_op_str | 5 | 140.2 | 123.4 | 240.4 | 224.7 | 276.8 | 436.1 | 336.3 | 0.506 | 0.446 | 1.041 | 0.995 | 0.993 | 0.997 | 0.986 | 0.980 | 0.975 | 0.979 | ok |
| lib/error_tbl | 14 | 275.2 | 216.4 | 267.2 | 206.6 | 3042.9 | 619.9 | 625.1 | 0.090 | 0.071 | 1.034 | 1.009 | 0.982 | 0.990 | 1.010 | 1.011 | 0.954 | 0.986 | ok |
| lib/find_cap | 5 | 265.1 | 223.6 | 264.7 | 227.8 | 183.4 | 365.9 | 363.4 | 1.445 | 1.219 | 0.962 | 1.005 | 0.958 | 0.983 | 0.956 | 1.000 | 1.002 | 0.985 | ok |
| lib/find_pat | 14 | 217.8 | 194.9 | 218.2 | 194.9 | 705.2 | 1094.0 | 1092.2 | 0.309 | 0.276 | 0.847 | 0.822 | 0.853 | 0.796 | 0.806 | 0.819 | 0.826 | 0.795 | ok |
| lib/find_plain | 14 | 112.8 | 117.5 | 111.4 | 116.6 | 67.6 | 342.8 | 363.4 | 1.669 | 1.739 | 0.884 | 0.854 | 1.007 | 0.825 | 0.883 | 0.854 | 1.018 | 0.834 | ok |
| lib/fmt_d | 5 | 93.4 | 88.6 | 98.5 | 95.3 | 126.3 | 278.1 | 235.1 | 0.739 | 0.702 | 1.004 | 0.993 | 0.986 | 0.986 | 0.997 | 0.991 | 1.001 | 0.988 | ok |
| lib/fmt_f | 5 | 62.1 | 58.0 | 63.4 | 59.7 | 86.8 | 215.0 | 196.2 | 0.716 | 0.668 | 0.991 | 1.004 | 1.009 | 1.009 | 0.973 | 0.982 | 1.002 | 1.008 | ok |
| lib/fmt_g | 14 | 71.7 | 62.2 | 73.3 | 63.8 | 67.3 | 204.6 | 179.3 | 1.065 | 0.923 | 0.980 | 1.005 | 0.986 | 1.008 | 0.973 | 0.984 | 0.996 | 1.007 | ok |
| lib/fmt_mixed | 14 | 59.8 | 50.6 | 66.4 | 56.2 | 106.2 | 254.5 | 238.3 | 0.563 | 0.476 | 1.005 | 1.002 | 1.007 | 1.005 | 0.988 | 0.985 | 0.994 | 1.006 | ok |
| lib/fmt_q | 14 | 142.1 | 119.2 | 158.8 | 119.9 | 103.3 | 238.6 | 226.1 | 1.376 | 1.155 | 0.991 | 1.001 | 0.984 | 1.002 | 1.083 | 0.986 | 0.986 | 1.003 | ok |
| lib/fmt_s | 5 | 6.8 | 6.3 | 6.8 | 6.3 | 3.2 | 217.6 | 222.2 | 2.132 | 1.980 | 0.923 | 0.830 | 0.913 | 0.832 | 0.911 | 0.826 | 0.903 | 0.833 | ok |
| lib/fmt_x_pad | 14 | 56.9 | 48.9 | 59.2 | 50.7 | 54.0 | 177.1 | 177.5 | 1.054 | 0.906 | 1.010 | 0.995 | 1.024 | 0.990 | 1.004 | 0.995 | 0.997 | 0.993 | ok |
| lib/getmt | 5 | 14.8 | 14.4 | 16.1 | 15.8 | 8.8 | 255.7 | 264.8 | 1.676 | 1.630 | 0.921 | 0.945 | 0.921 | 0.947 | 0.813 | 0.828 | 0.812 | 0.828 | ok |
| lib/gmatch_words | 14 | 178.5 | 140.2 | 180.2 | 138.3 | 173.3 | 315.9 | 319.8 | 1.030 | 0.809 | 1.015 | 1.014 | 1.032 | 1.000 | 1.021 | 1.015 | 1.009 | 1.003 | ok |
| lib/gsub_fn | 14 | 173.1 | 124.1 | 172.0 | 125.0 | 200.2 | 323.4 | 312.5 | 0.864 | 0.620 | 0.991 | 1.000 | 0.965 | 0.959 | 0.961 | 0.993 | 0.947 | 0.960 | ok |
| lib/gsub_str | 5 | 162.9 | 126.9 | 159.1 | 126.4 | 398.5 | 491.6 | 515.6 | 0.409 | 0.319 | 0.986 | 1.005 | 1.004 | 1.002 | 0.956 | 0.990 | 1.002 | 0.998 | ok |
| lib/gsub_tbl | 14 | 161.2 | 105.3 | 160.0 | 105.1 | 187.9 | 321.9 | 321.7 | 0.858 | 0.561 | 0.987 | 1.001 | 0.973 | 0.972 | 1.051 | 0.987 | 0.961 | 0.974 | ok |
| lib/it_ipairs | 14 | 230.2 | 228.0 | 248.7 | 240.2 | 251.6 | 5628.0 | 4827.6 | 0.915 | 0.906 | 0.966 | 0.949 | 0.975 | 0.948 | 0.891 | 0.915 | 0.924 | 0.929 | ok |
| lib/it_next | 14 | 44.3 | 44.3 | 44.1 | 44.1 | 180.5 | 371.1 | 401.4 | 0.245 | 0.245 | 1.000 | 0.962 | 1.007 | 0.962 | 1.002 | 0.962 | 1.009 | 0.962 | ok |
| lib/it_next_empty | 14 | 46.0 | 46.5 | 52.1 | 51.8 | 72.5 | 393.0 | 455.4 | 0.634 | 0.642 | 0.888 | 0.966 | 0.908 | 0.966 | 1.005 | 0.983 | 1.007 | 0.983 | ok |
| lib/it_pairs | 14 | 85.9 | 86.0 | 89.8 | 88.6 | 127.9 | 687.0 | 646.3 | 0.672 | 0.672 | 0.983 | 0.997 | 0.984 | 0.997 | 0.967 | 0.967 | 0.990 | 0.975 | ok |
| lib/it_pairs_hash | 14 | 101.7 | 101.1 | 107.2 | 106.5 | 118.1 | 989.3 | 957.1 | 0.861 | 0.856 | 0.983 | 0.998 | 0.988 | 0.998 | 0.950 | 0.976 | 0.990 | 0.982 | ok |
| lib/len_op | 14 | 45.6 | 45.0 | 48.0 | 48.2 | 100.7 | 1384.7 | 1419.8 | 0.452 | 0.447 | 0.952 | 0.838 | 0.949 | 0.838 | 0.837 | 0.921 | 0.845 | 0.921 | ok |
| lib/len_tbl | 14 | 142.6 | 141.6 | 149.3 | 148.7 | 159.7 | 372.6 | 337.9 | 0.893 | 0.887 | 1.040 | 0.987 | 1.048 | 0.987 | 0.949 | 1.000 | 0.947 | 1.000 | ok |
| lib/load_small | 14 | 231.2 | 159.4 | 228.3 | 164.5 | 215.6 | 378.8 | 345.2 | 1.072 | 0.739 | 0.967 | 1.000 | 1.047 | 0.992 | 1.021 | 0.985 | 1.038 | 0.972 | ok |
| lib/lower | 14 | 83.8 | 75.8 | 84.3 | 76.8 | 110.1 | 289.0 | 294.8 | 0.762 | 0.688 | 0.949 | 0.989 | 0.938 | 0.976 | 0.930 | 0.979 | 0.932 | 0.963 | ok |
| lib/m_abs | 11 | 247.3 | 247.0 | 246.8 | 246.6 | 390.3 | 6061.9 | 6506.8 | 0.634 | 0.633 | 0.998 | 0.999 | 1.010 | 0.999 | 1.006 | 0.999 | 1.014 | 0.999 | ok |
| lib/m_floor | 2 | 226.3 | 225.6 | 227.7 | 227.1 | 206.5 | 3681.1 | 4104.1 | 1.096 | 1.092 | 1.001 | 0.999 | 1.000 | 1.000 | 1.004 | 0.999 | 1.004 | 0.999 | ok |
| lib/m_fmod | 5 | 218.3 | 222.2 | 220.2 | 228.8 | 235.1 | 329.2 | 361.2 | 0.928 | 0.945 | 0.997 | 1.023 | 1.020 | 0.996 | 0.996 | 1.023 | 1.014 | 0.996 | ok |
| lib/m_max | 2 | 256.4 | 256.0 | 255.9 | 256.9 | 401.3 | 8004.8 | 10137.5 | 0.639 | 0.638 | 1.002 | 0.999 | 1.002 | 0.999 | 0.996 | 0.999 | 1.002 | 0.999 | ok |
| lib/m_min | 14 | 204.5 | 198.9 | 200.9 | 200.3 | 387.7 | 6530.7 | 8066.5 | 0.527 | 0.513 | 0.933 | 0.860 | 0.882 | 0.860 | 0.905 | 0.860 | 0.914 | 0.861 | ok |
| lib/m_pow_op | 5 | 284.6 | 283.6 | 284.5 | 284.1 | 275.2 | 1923.5 | 1956.0 | 1.034 | 1.031 | 1.015 | 0.870 | 1.014 | 0.870 | 1.014 | 0.870 | 1.015 | 0.870 | ok |
| lib/m_random | 2 | 78.6 | 78.9 | 79.1 | 78.3 | 73.2 | 345.7 | 375.3 | 1.073 | 1.077 | 0.993 | 0.999 | 1.006 | 1.000 | 1.005 | 0.999 | 1.000 | 1.000 | ok |
| lib/m_random_f | 5 | 53.0 | 52.7 | 53.0 | 52.6 | 89.9 | 272.7 | 312.0 | 0.589 | 0.586 | 1.003 | 0.974 | 1.010 | 0.974 | 1.005 | 0.974 | 1.002 | 0.974 | ok |
| lib/m_sin | 5 | 267.0 | 256.2 | 267.5 | 256.1 | 210.5 | 563.9 | 589.3 | 1.268 | 1.217 | 0.983 | 0.975 | 0.984 | 0.974 | 0.986 | 0.975 | 0.984 | 0.974 | ok |
| lib/m_sqrt | 5 | 277.9 | 278.0 | 277.7 | 278.4 | 270.0 | 2022.6 | 2132.3 | 1.030 | 1.030 | 1.000 | 0.870 | 1.008 | 0.870 | 0.959 | 0.870 | 0.974 | 0.870 | ok |
| lib/m_tointeger | 5 | 228.7 | 228.8 | 236.9 | 236.4 | - | 3718.4 | 3972.3 | - | - | 1.026 | 0.999 | 1.027 | 1.000 | 1.060 | 0.999 | 1.001 | 1.000 | ok |
| lib/m_type | 5 | 32.1 | 31.7 | 32.0 | 31.7 | - | 419.3 | 429.1 | - | - | 1.004 | 0.998 | 1.003 | 0.998 | 1.000 | 0.997 | 1.002 | 0.998 | ok |
| lib/m_ult | 14 | 275.3 | 280.7 | 274.6 | 284.3 | - | 415.2 | 435.8 | - | - | 1.004 | 1.021 | 0.996 | 0.997 | 0.996 | 1.021 | 1.000 | 0.997 | ok |
| lib/match | 14 | 47.5 | 43.4 | 47.9 | 43.6 | 193.6 | 319.9 | 303.8 | 0.246 | 0.224 | 0.774 | 0.805 | 0.854 | 0.834 | 0.789 | 0.799 | 0.846 | 0.826 | ok |
| lib/os_clock | 5 | 115.5 | 120.9 | 120.9 | 119.9 | 100.7 | 119.3 | 118.5 | 1.147 | 1.200 | 1.005 | 1.021 | 0.999 | 0.997 | 1.020 | 1.020 | 0.990 | 0.997 | ok |
| lib/pcall_err | 14 | 118.0 | 94.8 | 124.3 | 104.6 | 764.5 | 308.4 | 309.0 | 0.154 | 0.124 | 0.971 | 1.005 | 0.951 | 0.992 | 0.943 | 0.997 | 0.947 | 0.991 | ok |
| lib/pcall_ok | 14 | 27.4 | 27.0 | 27.3 | 27.0 | 41.0 | 1524.0 | 1518.3 | 0.669 | 0.658 | 0.909 | 0.874 | 0.894 | 0.874 | 0.901 | 0.874 | 0.905 | 0.873 | ok |
| lib/rawget_set | 5 | 16.1 | 15.9 | 16.3 | 16.0 | 14.2 | 381.1 | 417.2 | 1.132 | 1.114 | 0.847 | 0.808 | 0.837 | 0.809 | 0.865 | 0.808 | 0.859 | 0.809 | ok |
| lib/rep | 5 | 106.9 | 96.5 | 116.6 | 105.9 | 89.6 | 347.4 | 333.3 | 1.193 | 1.077 | 0.992 | 0.984 | 0.966 | 0.976 | 0.962 | 0.987 | 0.960 | 0.981 | ok |
| lib/rep_sep | 5 | 100.0 | 94.4 | 130.4 | 120.3 | 103.8 | 329.6 | 311.2 | 0.964 | 0.910 | 1.133 | 1.144 | 1.167 | 1.196 | 1.113 | 1.116 | 1.113 | 1.156 | ok |
| lib/reverse | 14 | 94.4 | 87.8 | 95.1 | 88.9 | 115.3 | 309.8 | 314.7 | 0.819 | 0.762 | 0.948 | 0.989 | 0.941 | 0.976 | 0.920 | 0.979 | 0.907 | 0.966 | ok |
| lib/sel_count | 14 | 215.3 | 216.2 | 223.4 | 219.0 | 388.2 | 15509.5 | 14438.4 | 0.555 | 0.557 | 0.994 | 0.998 | 1.000 | 0.998 | 1.030 | 0.998 | 1.001 | 0.998 | ok |
| lib/sel_loop | 14 | 207.2 | 206.4 | 208.4 | 208.2 | 5263.6 | 53689.2 | 54662.9 | 0.039 | 0.039 | 0.983 | 0.924 | 0.985 | 0.924 | 0.976 | 0.924 | 0.983 | 0.924 | ok |
| lib/sel_n | 14 | 335.6 | 334.1 | 331.7 | 329.2 | 675.4 | 25345.4 | 23674.6 | 0.497 | 0.495 | 0.999 | 0.998 | 1.037 | 0.998 | 1.014 | 0.998 | 1.019 | 0.998 | ok |
| lib/setmt | 14 | 181.3 | 159.8 | 218.1 | 188.8 | 331.9 | 743.5 | 835.9 | 0.546 | 0.481 | 0.986 | 0.993 | 1.004 | 0.993 | 0.956 | 0.988 | 0.986 | 0.983 | ok |
| lib/str_eq | 14 | 135.2 | 134.7 | 135.4 | 135.1 | 182.5 | 1479.5 | 1529.4 | 0.741 | 0.738 | 0.845 | 0.921 | 0.843 | 0.921 | 0.844 | 0.921 | 0.845 | 0.921 | ok |
| lib/str_eq_dyn | 5 | 131.7 | 131.0 | 131.8 | 131.7 | 150.4 | 1277.3 | 1348.8 | 0.876 | 0.871 | 0.975 | 0.859 | 0.986 | 0.859 | 0.985 | 0.859 | 0.991 | 0.859 | ok |
| lib/str_eq_long | 14 | 34.4 | 34.0 | 34.1 | 33.8 | 21.5 | 215.5 | 222.1 | 1.603 | 1.583 | 0.965 | 0.960 | 0.969 | 0.961 | 0.957 | 0.960 | 0.954 | 0.961 | ok |
| lib/str_idx_concat_key | 5 | 203.5 | 190.9 | 203.5 | 192.8 | 166.6 | 464.0 | 461.2 | 1.222 | 1.146 | 0.991 | 0.996 | 1.002 | 1.008 | 0.989 | 0.996 | 1.001 | 1.007 | ok |
| lib/str_lt | 14 | 86.6 | 84.4 | 87.1 | 84.9 | 66.3 | 227.1 | 228.3 | 1.305 | 1.272 | 0.928 | 0.983 | 0.927 | 0.984 | 0.979 | 0.983 | 0.982 | 0.983 | ok |
| lib/str_lt_short | 14 | 96.9 | 90.3 | 97.8 | 93.1 | 62.5 | 249.8 | 255.3 | 1.550 | 1.445 | 0.853 | 0.944 | 0.850 | 0.943 | 0.935 | 0.951 | 0.949 | 0.950 | ok |
| lib/str_method_len | 14 | 43.5 | 43.2 | 48.8 | 48.3 | 55.3 | 1386.6 | 1429.7 | 0.788 | 0.782 | 0.957 | 0.903 | 0.954 | 0.903 | 0.958 | 0.955 | 0.945 | 0.955 | ok |
| lib/string_key_const | 14 | 168.3 | 168.1 | 182.6 | 182.1 | 176.9 | 1151.6 | 1165.1 | 0.951 | 0.950 | 0.952 | 0.898 | 0.954 | 0.898 | 0.913 | 0.909 | 0.912 | 0.910 | ok |
| lib/string_key_dyn | 5 | 215.3 | 215.2 | 215.6 | 215.1 | 268.0 | 1082.3 | 1101.0 | 0.803 | 0.803 | 0.939 | 0.933 | 0.941 | 0.933 | 0.944 | 0.933 | 0.942 | 0.934 | ok |
| lib/sub | 14 | 115.1 | 106.7 | 118.5 | 107.3 | 80.5 | 469.5 | 482.7 | 1.430 | 1.325 | 0.875 | 0.933 | 0.835 | 0.892 | 0.904 | 0.933 | 0.862 | 0.892 | ok |
| lib/sub_1char | 14 | 111.6 | 99.2 | 112.4 | 99.9 | 98.2 | 459.2 | 478.8 | 1.136 | 1.010 | 0.929 | 0.958 | 0.907 | 0.938 | 0.925 | 0.959 | 0.921 | 0.939 | ok |
| lib/tconcat | 14 | 142.5 | 99.9 | 173.2 | 119.0 | 133.4 | 346.9 | 322.3 | 1.068 | 0.749 | 0.987 | 1.000 | 1.000 | 1.002 | 1.024 | 0.994 | 0.988 | 1.003 | ok |
| lib/tconcat_num | 14 | 174.7 | 117.4 | 180.5 | 121.2 | 246.0 | 908.0 | 844.8 | 0.710 | 0.477 | 0.977 | 1.001 | 0.982 | 1.003 | 0.974 | 0.993 | 0.991 | 1.003 | ok |
| lib/tfront_1k | 14 | 77.8 | 76.0 | 80.5 | 77.2 | 406.3 | 4375.7 | 4241.0 | 0.191 | 0.187 | 0.984 | 1.008 | 0.998 | 1.022 | 0.984 | 1.008 | 1.015 | 1.024 | ok |
| lib/tinsert_append | 14 | 152.7 | 128.9 | 194.5 | 182.2 | 141.7 | 539.7 | 574.7 | 1.077 | 0.910 | 0.958 | 0.990 | 0.970 | 0.991 | 1.013 | 1.023 | 1.006 | 1.025 | ok |
| lib/tinsert_front | 14 | 224.6 | 176.6 | 261.7 | 209.6 | 272.9 | 455.4 | 493.5 | 0.823 | 0.647 | 1.003 | 1.003 | 0.991 | 1.006 | 0.997 | 1.003 | 1.002 | 1.005 | ok |
| lib/tinsert_idx | 14 | 175.1 | 153.9 | 207.2 | 194.1 | 154.6 | 328.8 | 383.3 | 1.133 | 0.996 | 0.997 | 1.027 | 1.012 | 1.030 | 1.010 | 1.023 | 1.023 | 1.025 | ok |
| lib/tmove | 5 | 101.7 | 101.1 | 120.7 | 120.3 | 105.8 | 700.5 | 683.1 | 0.962 | 0.956 | 0.988 | 0.985 | 0.984 | 0.985 | 0.968 | 0.994 | 0.963 | 0.994 | ok |
| lib/tmove_1k | 14 | 97.9 | 69.6 | 107.9 | 72.2 | 111.6 | 476.4 | 447.3 | 0.877 | 0.623 | 0.971 | 1.002 | 1.017 | 1.004 | 0.973 | 1.003 | 0.967 | 0.993 | ok |
| lib/tonumber_bad | 14 | 147.1 | 112.1 | 142.2 | 111.6 | 168.0 | 408.1 | 431.1 | 0.876 | 0.667 | 0.944 | 0.993 | 0.928 | 0.992 | 0.853 | 0.990 | 0.777 | 0.989 | ok |
| lib/tonumber_base | 14 | 267.0 | 262.8 | 268.5 | 274.7 | 229.2 | 432.1 | 451.7 | 1.165 | 1.146 | 1.007 | 1.013 | 1.011 | 1.017 | 0.989 | 1.012 | 1.041 | 1.017 | ok |
| lib/tonumber_flt | 14 | 198.5 | 168.7 | 189.6 | 165.0 | 776.7 | 577.4 | 596.1 | 0.256 | 0.217 | 1.022 | 0.996 | 0.983 | 0.998 | 0.963 | 0.995 | 0.984 | 0.994 | ok |
| lib/tonumber_int | 14 | 150.8 | 136.9 | 160.4 | 140.8 | 96.2 | 288.1 | 305.0 | 1.568 | 1.423 | 0.973 | 0.991 | 0.972 | 0.981 | 1.000 | 0.996 | 0.991 | 0.995 | ok |
| lib/tonumber_num | 14 | 166.0 | 166.4 | 166.2 | 166.4 | 383.1 | 6476.7 | 7506.3 | 0.433 | 0.434 | 0.773 | 0.838 | 0.786 | 0.838 | 0.763 | 0.838 | 0.786 | 0.839 | ok |
| lib/tostring_flt | 14 | 82.4 | 65.9 | 83.6 | 61.4 | 82.5 | 299.3 | 214.9 | 0.999 | 0.799 | 0.996 | 0.995 | 1.001 | 0.991 | 1.001 | 0.996 | 1.004 | 0.995 | ok |
| lib/tostring_int | 5 | 119.2 | 107.5 | 135.7 | 123.2 | 279.8 | 543.3 | 450.7 | 0.426 | 0.384 | 0.977 | 0.988 | 0.986 | 0.982 | 1.007 | 0.992 | 1.006 | 0.989 | ok |
| lib/tostring_tbl | 14 | 107.1 | 103.8 | 107.5 | 106.1 | 93.3 | 313.8 | 280.9 | 1.148 | 1.112 | 0.991 | 0.997 | 0.959 | 0.994 | 0.963 | 0.975 | 0.987 | 0.971 | ok |
| lib/tpack | 14 | 143.6 | 116.6 | 214.6 | 169.6 | 306.1 | 499.0 | 485.7 | 0.469 | 0.381 | 0.965 | 0.995 | 0.990 | 0.994 | 1.017 | 0.995 | 1.011 | 0.991 | ok |
| lib/tremove_front | 5 | 147.1 | 132.1 | 183.8 | 168.0 | 191.0 | 592.3 | 593.7 | 0.770 | 0.692 | 0.997 | 0.997 | 0.996 | 0.997 | 0.984 | 0.996 | 0.979 | 0.996 | ok |
| lib/tremove_pop | 5 | 134.9 | 134.8 | 131.0 | 158.5 | 136.7 | 483.9 | 552.9 | 0.986 | 0.986 | 0.561 | 0.560 | 0.780 | 0.720 | 0.584 | 0.585 | 0.755 | 0.685 | ok |
| lib/tsort_cmp | 5 | 270.8 | 251.3 | 282.6 | 256.9 | 285.5 | 339.9 | 336.1 | 0.949 | 0.880 | 1.021 | 0.999 | 0.976 | 0.954 | 1.031 | 0.999 | 0.985 | 0.942 | ok |
| lib/tsort_int | 5 | 267.3 | 221.2 | 293.6 | 222.6 | 294.0 | 382.4 | 406.0 | 0.909 | 0.752 | 0.996 | 0.993 | 1.029 | 1.010 | 1.002 | 0.998 | 0.994 | 0.997 | ok |
| lib/tsort_str | 5 | 243.4 | 147.8 | 276.8 | 151.4 | 244.8 | 364.6 | 401.0 | 0.995 | 0.604 | 0.950 | 0.908 | 0.971 | 0.945 | 0.945 | 0.921 | 0.945 | 0.921 | ok |
| lib/tunpack | 5 | 123.7 | 106.1 | 129.1 | 112.3 | 154.8 | 487.1 | 448.5 | 0.799 | 0.686 | 0.996 | 0.996 | 1.025 | 0.996 | 1.017 | 0.996 | 1.000 | 0.995 | ok |
| lib/type_call | 14 | 84.4 | 84.0 | 91.2 | 90.8 | 85.6 | 494.3 | 468.3 | 0.986 | 0.981 | 0.972 | 0.947 | 0.969 | 0.948 | 0.972 | 0.923 | 0.971 | 0.923 | ok |
| lib/upper | 14 | 91.1 | 85.3 | 91.0 | 84.8 | 120.7 | 307.4 | 312.6 | 0.755 | 0.706 | 0.950 | 0.989 | 0.939 | 0.976 | 0.911 | 0.979 | 0.895 | 0.963 | ok |
| lib/utf8_len | 14 | 273.3 | 285.8 | 273.9 | 287.8 | - | 442.2 | 456.7 | - | - | 1.005 | 1.013 | 1.041 | 0.998 | 0.999 | 1.013 | 1.029 | 0.998 | ok |
| programs/binary_trees | 14 | 1170.7 | 979.4 | 1421.4 | 1189.0 | 2007.4 | 4389.1 | 4447.3 | 0.583 | 0.488 | 0.962 | 0.983 | 0.951 | 0.989 | 0.992 | 1.007 | 1.021 | 1.004 | ok |
| programs/closures | 14 | 456.1 | 420.2 | 518.6 | 473.0 | 949.8 | 5063.2 | 4986.1 | 0.480 | 0.442 | 0.989 | 0.988 | 0.995 | 0.983 | 1.013 | 0.985 | 0.986 | 0.981 | ok |
| programs/coroutines | 14 | 1365.0 | 1288.8 | 1409.7 | 1317.5 | 871.7 | 3733.4 | 3656.9 | 1.566 | 1.478 | 0.757 | 0.934 | 0.992 | 1.038 | 0.759 | 0.938 | 0.932 | 1.030 | ok |
| programs/fannkuch | 5 | 1010.9 | 1011.4 | 1032.6 | 1041.1 | 1075.7 | 4075.1 | 4114.8 | 0.940 | 0.940 | 0.964 | 0.942 | 0.973 | 0.942 | 0.969 | 0.952 | 0.971 | 0.953 | ok |
| programs/fasta | 5 | 1254.5 | 1140.5 | 1361.6 | 1232.4 | 1495.7 | 3776.8 | 3842.3 | 0.839 | 0.763 | 0.986 | 0.978 | 0.988 | 0.974 | 1.007 | 0.979 | 1.002 | 0.977 | ok |
| programs/fib | 14 | 623.0 | 624.4 | 620.7 | 621.0 | 469.7 | 4139.5 | 4150.5 | 1.326 | 1.329 | 0.987 | 0.984 | 0.989 | 0.984 | 0.985 | 0.984 | 0.988 | 0.984 | ok |
| programs/gc_churn | 14 | 2186.6 | 1791.1 | 3103.3 | 2609.2 | 3608.7 | 4605.5 | 5988.9 | 0.606 | 0.496 | 1.024 | 1.004 | 1.013 | 1.006 | 0.967 | 1.002 | 0.945 | 0.998 | ok |
| programs/heapsort | 5 | 1032.3 | 1029.6 | 2192.5 | 2187.8 | 1286.6 | 4063.2 | 3957.0 | 0.802 | 0.800 | 0.986 | 0.899 | 0.982 | 0.899 | 0.991 | 0.994 | 0.991 | 0.994 | ok |
| programs/json | 5 | 1814.7 | 1358.4 | 1879.5 | 1487.0 | 1470.3 | 3127.6 | 3229.9 | 1.234 | 0.924 | 1.000 | 0.944 | 0.946 | 0.946 | 0.958 | 0.924 | 0.973 | 0.928 | ok |
| programs/knucleotide | 14 | 990.9 | 1012.7 | 1015.9 | 999.6 | 691.4 | 4061.5 | 4161.0 | 1.433 | 1.465 | 0.953 | 0.976 | 0.950 | 0.951 | 0.960 | 0.977 | 0.945 | 0.952 | ok |
| programs/load_big | 5 | 731.2 | 551.5 | 624.8 | 443.8 | 491.1 | 1787.3 | 620.0 | 1.489 | 1.123 | 0.969 | 1.000 | 1.015 | 0.994 | 0.944 | 1.000 | 1.011 | 0.993 | CHECK |
| programs/loops_arith | 5 | 453.8 | 454.2 | 459.0 | 458.7 | 1108.2 | 4344.5 | 4909.6 | 0.410 | 0.410 | 0.952 | 0.943 | 0.951 | 0.943 | 0.955 | 0.943 | 0.956 | 0.943 | CHECK |
| programs/mandelbrot | 2 | 533.0 | 532.6 | 533.8 | 533.4 | 521.6 | 4727.1 | 5530.2 | 1.022 | 1.021 | 1.001 | 0.996 | 1.001 | 0.996 | 1.000 | 0.996 | 1.000 | 0.996 | ok |
| programs/matmul | 5 | 427.3 | 412.6 | 436.3 | 433.8 | 272.0 | 3171.0 | 3693.4 | 1.571 | 1.517 | 0.970 | 0.880 | 0.948 | 0.880 | 0.981 | 0.892 | 0.977 | 0.892 | ok |
| programs/metamethods | 14 | 653.7 | 608.9 | 796.0 | 734.1 | 540.1 | 4614.3 | 4669.9 | 1.210 | 1.127 | 0.992 | 0.995 | 0.990 | 0.995 | 1.004 | 0.993 | 0.981 | 0.992 | ok |
| programs/nbody | 14 | 340.7 | 340.1 | 346.5 | 346.3 | 345.6 | 4246.2 | 4718.3 | 0.986 | 0.984 | 0.993 | 0.973 | 0.985 | 0.974 | 0.974 | 0.976 | 0.988 | 0.976 | ok |
| programs/oop_methods | 14 | 124.6 | 124.2 | 124.6 | 124.1 | 304.8 | 4567.3 | 4685.2 | 0.409 | 0.408 | 0.874 | 0.894 | 0.874 | 0.894 | 0.881 | 0.894 | 0.880 | 0.894 | ok |
| programs/pcall_error | 14 | 1460.5 | 1229.2 | 1438.5 | 1206.0 | 8346.6 | 4028.8 | 4363.2 | 0.175 | 0.147 | 0.980 | 1.002 | 0.990 | 0.995 | 0.968 | 0.997 | 0.977 | 0.985 | ok |
| programs/queens | 14 | 1810.2 | 1809.8 | 1888.7 | 1889.5 | 1907.1 | 4368.7 | 4120.8 | 0.949 | 0.949 | 0.972 | 0.922 | 0.980 | 0.922 | 0.981 | 0.939 | 0.982 | 0.939 | ok |
| programs/sieve | 5 | 549.2 | 549.8 | 587.2 | 651.3 | 761.7 | 2750.2 | 5029.1 | 0.721 | 0.722 | 0.948 | 0.892 | 0.943 | 0.892 | 0.958 | 0.898 | 0.959 | 0.912 | ok |
| programs/spectral_norm | 5 | 178.5 | 177.9 | 182.4 | 183.0 | 171.1 | 4573.7 | 4337.9 | 1.043 | 1.040 | 0.989 | 0.859 | 0.984 | 0.860 | 0.985 | 0.896 | 0.991 | 0.896 | ok |
| programs/string_ops | 5 | 1114.9 | 1029.4 | 1060.1 | 982.7 | 2238.4 | 2828.0 | 2807.1 | 0.498 | 0.460 | 1.017 | 1.003 | 1.006 | 1.000 | 1.004 | 1.000 | 1.013 | 1.000 | ok |
| programs/table_array | 5 | 357.4 | 301.7 | 398.7 | 311.6 | 600.0 | 3400.3 | 3492.4 | 0.596 | 0.503 | 0.997 | 0.993 | 1.004 | 0.989 | 0.993 | 0.997 | 0.994 | 0.975 | ok |
| programs/table_hash | 14 | 487.2 | 479.8 | 487.3 | 483.7 | 394.4 | 1501.6 | 1831.0 | 1.235 | 1.217 | 1.000 | 0.993 | 1.007 | 0.998 | 0.997 | 0.992 | 1.001 | 0.998 | ok |
| programs/varargs | 14 | 1163.5 | 1057.2 | 1190.6 | 1087.1 | 2194.8 | 5366.5 | 5084.7 | 0.530 | 0.482 | 0.946 | 0.982 | 0.958 | 0.987 | 0.992 | 0.981 | 0.991 | 0.990 | ok |
| real/game_loop | 14 | 33.4 | 32.1 | 36.3 | 35.0 | 21.7 | 207.6 | 221.0 | 1.537 | 1.478 | 0.967 | 0.945 | 0.956 | 0.953 | 0.929 | 0.961 | 0.922 | 0.968 | ok |
| real/json_roundtrip | 5 | 563.1 | 459.0 | 572.6 | 476.6 | 469.1 | 796.8 | 802.8 | 1.200 | 0.978 | 1.031 | 0.993 | 1.005 | 0.992 | 0.986 | 0.988 | 1.012 | 0.987 | ok |
| real/lua_interp | 5 | 100.3 | 99.4 | 102.6 | 99.8 | 59.3 | 223.6 | 225.0 | 1.690 | 1.676 | 0.950 | 0.974 | 0.981 | 0.978 | 0.981 | 0.976 | 0.986 | 0.980 | ok |
| real/template_engine | 14 | 730.5 | 620.8 | 772.6 | 619.5 | 955.0 | 1225.1 | 1249.2 | 0.765 | 0.650 | 1.022 | 1.006 | 1.053 | 1.020 | 1.022 | 1.004 | 0.989 | 1.014 | ok |
| real/wordfreq | 5 | 115.5 | 98.1 | 113.7 | 99.4 | 116.4 | 212.8 | 210.2 | 0.992 | 0.843 | 1.020 | 1.004 | 1.019 | 0.985 | 0.996 | 1.002 | 1.027 | 0.984 | ok |

## Output / .expected check

- programs/load_big [p54]: out=diff exp=none err=
- programs/load_big [g54]: out=diff exp=none err=
- programs/loops_arith [p54]: out=diff exp=none err=
- programs/loops_arith [g54]: out=diff exp=none err=

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
