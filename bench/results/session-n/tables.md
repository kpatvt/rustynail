# Tables, session-n (subject ab7d185)

Ratios below 1: first column faster/fewer. Ours = min over all readings (2 quick, 5 full, 15-run serial reruns). Cached columns as stored.


# Aggregates, readings A: the fast pass only (min of 2 or 5 runs per program)

## Headline geomeans (cycles, ours/ref)

| build | /LuaJIT cyc (n) | /LuaJIT ins | /PUC same version cyc (223) | /PUC ins |
|---|---|---|---|---|
| 5.5 plain | 0.910 (219) | 1.268 | 0.208 | 0.208 |
| 5.5 PGO | 0.832 (219) | 1.172 | 0.190 | 0.192 |
| 5.4 plain | 0.959 (219) | 1.359 | 0.214 | 0.222 |
| 5.4 PGO | 0.883 (219) | 1.257 | 0.198 | 0.206 |

- 5.5 plain / LuaJIT -joff (219): 0.307
- 5.5 PGO / LuaJIT -joff (219): 0.281
- 5a064d7 cached (set B) (same programs): 5.5 plain/LJ 0.938, PGO/LJ 0.857, 5.4 plain/LJ 0.998, 5.4 PGO/LJ 0.914
- session-m published 8cd1f06 (same programs): 5.5 plain/LJ 0.924, PGO/LJ 0.849, 5.4 plain/LJ 0.984, 5.4 PGO/LJ 0.904
- 7413bff (session-l subject) (same programs): 5.5 plain/LJ 0.936, PGO/LJ 0.856, 5.4 plain/LJ 0.999, 5.4 PGO/LJ 0.913
- our 5.4 build / our 5.5 build cycles: plain 1.054, PGO 1.061; PGO/plain: 5.5 0.916, 5.4 0.923

## Subject vs 5a064d7 (same session, set B), 5a064d7 set A, session-k published and b116447 (geomean, 223 programs)

| build | vs | cycles | instructions | cyc >3% better / within / >3% worse | ins >3% fewer / more |
|---|---|---|---|---|---|
| 5.5 plain | 5a064d7 cached (set B) | 0.971 | 0.945 | 75 / 133 / 15 | 97 / 2 |
| 5.5 plain | 7413bff same machine (session-l subject) | 0.972 | 0.950 | 82 / 124 / 17 | 93 / 2 |
| 5.5 plain | session-m published 8cd1f06 | 0.985 | 0.984 | 52 / 158 / 13 | 32 / 1 |
| 5.5 PGO | 5a064d7 cached (set B) | 0.972 | 0.946 | 76 / 134 / 13 | 96 / 4 |
| 5.5 PGO | 7413bff same machine (session-l subject) | 0.973 | 0.950 | 79 / 132 / 12 | 89 / 4 |
| 5.5 PGO | session-m published 8cd1f06 | 0.981 | 0.983 | 62 / 154 / 7 | 32 / 1 |
| 5.4 plain | 5a064d7 cached (set B) | 0.962 | 0.948 | 93 / 121 / 9 | 96 / 1 |
| 5.4 plain | 7413bff same machine (session-l subject) | 0.960 | 0.952 | 94 / 121 / 8 | 88 / 1 |
| 5.4 plain | session-m published 8cd1f06 | 0.975 | 0.982 | 68 / 150 / 5 | 34 / 1 |
| 5.4 PGO | 5a064d7 cached (set B) | 0.967 | 0.949 | 81 / 138 / 4 | 92 / 1 |
| 5.4 PGO | 7413bff same machine (session-l subject) | 0.968 | 0.953 | 84 / 133 / 6 | 86 / 2 |
| 5.4 PGO | session-m published 8cd1f06 | 0.977 | 0.981 | 51 / 166 / 6 | 37 / 2 |

### Per set, geomean subject / 5a064d7 and subject / b116447 (cycles / instructions)

| set | n | plain vs b cyc | ins | PGO vs b cyc | ins | 5.4 plain vs b cyc | ins | 5.4 PGO vs b cyc | ins | plain vs i cyc | ins | PGO vs 7413bff cyc | ins | 5.4 plain vs i cyc | ins | 5.4 PGO vs 7413bff cyc | ins |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| apps | 5 | 0.956 | 0.930 | 0.951 | 0.938 | 0.943 | 0.935 | 0.939 | 0.940 | 0.965 | 0.942 | 0.954 | 0.947 | 0.963 | 0.949 | 0.966 | 0.952 |
| awfy | 14 | 0.970 | 0.949 | 0.969 | 0.954 | 0.961 | 0.950 | 0.962 | 0.954 | 0.966 | 0.954 | 0.966 | 0.958 | 0.959 | 0.956 | 0.960 | 0.960 |
| corpus | 18 | 0.948 | 0.935 | 0.945 | 0.940 | 0.929 | 0.929 | 0.934 | 0.935 | 0.950 | 0.943 | 0.944 | 0.948 | 0.931 | 0.931 | 0.931 | 0.938 |
| corpus2/dev | 17 | 0.950 | 0.945 | 0.952 | 0.950 | 0.937 | 0.942 | 0.952 | 0.947 | 0.961 | 0.969 | 0.956 | 0.973 | 0.939 | 0.963 | 0.956 | 0.968 |
| corpus2/heldout | 11 | 0.977 | 0.973 | 0.979 | 0.972 | 0.967 | 0.979 | 0.965 | 0.980 | 0.963 | 0.979 | 0.963 | 0.979 | 0.957 | 0.993 | 0.960 | 0.995 |
| jit-trace | 29 | 0.993 | 0.864 | 0.992 | 0.865 | 0.987 | 0.882 | 0.988 | 0.882 | 0.996 | 0.869 | 0.995 | 0.870 | 0.988 | 0.887 | 0.989 | 0.887 |
| lib | 95 | 0.968 | 0.963 | 0.972 | 0.961 | 0.961 | 0.962 | 0.967 | 0.961 | 0.970 | 0.963 | 0.974 | 0.961 | 0.958 | 0.962 | 0.968 | 0.961 |
| programs | 25 | 0.974 | 0.957 | 0.976 | 0.960 | 0.971 | 0.965 | 0.980 | 0.967 | 0.974 | 0.960 | 0.978 | 0.962 | 0.969 | 0.965 | 0.977 | 0.967 |
| real | 5 | 1.001 | 0.984 | 0.990 | 0.985 | 0.981 | 0.986 | 0.976 | 0.986 | 1.003 | 0.985 | 0.990 | 0.986 | 0.984 | 0.987 | 0.978 | 0.987 |
| real_dis | 4 | 1.024 | 0.993 | 1.006 | 1.004 | 0.990 | 0.988 | 1.011 | 1.001 | 1.009 | 0.992 | 1.009 | 1.004 | 0.975 | 0.986 | 1.004 | 1.000 |
| ALL | 223 | 0.971 | 0.945 | 0.972 | 0.946 | 0.962 | 0.948 | 0.967 | 0.949 | 0.972 | 0.950 | 0.973 | 0.950 | 0.960 | 0.952 | 0.968 | 0.953 |

# Aggregates, readings B: with the 15-run serial reruns of 145 programs merged (min over all readings)

## Headline geomeans (cycles, ours/ref)

| build | /LuaJIT cyc (n) | /LuaJIT ins | /PUC same version cyc (223) | /PUC ins |
|---|---|---|---|---|
| 5.5 plain | 0.908 (219) | 1.268 | 0.208 | 0.208 |
| 5.5 PGO | 0.830 (219) | 1.172 | 0.190 | 0.192 |
| 5.4 plain | 0.958 (219) | 1.359 | 0.214 | 0.222 |
| 5.4 PGO | 0.882 (219) | 1.257 | 0.197 | 0.206 |

- 5.5 plain / LuaJIT -joff (219): 0.307
- 5.5 PGO / LuaJIT -joff (219): 0.281
- 5a064d7 cached (set B) (same programs): 5.5 plain/LJ 0.938, PGO/LJ 0.857, 5.4 plain/LJ 0.998, 5.4 PGO/LJ 0.914
- session-m published 8cd1f06 (same programs): 5.5 plain/LJ 0.924, PGO/LJ 0.849, 5.4 plain/LJ 0.984, 5.4 PGO/LJ 0.904
- 7413bff (session-l subject) (same programs): 5.5 plain/LJ 0.936, PGO/LJ 0.856, 5.4 plain/LJ 0.999, 5.4 PGO/LJ 0.913
- our 5.4 build / our 5.5 build cycles: plain 1.053, PGO 1.061; PGO/plain: 5.5 0.916, 5.4 0.923

## Subject vs 5a064d7 (same session, set B), 5a064d7 set A, session-k published and b116447 (geomean, 223 programs)

| build | vs | cycles | instructions | cyc >3% better / within / >3% worse | ins >3% fewer / more |
|---|---|---|---|---|---|
| 5.5 plain | 5a064d7 cached (set B) | 0.969 | 0.945 | 76 / 133 / 14 | 97 / 2 |
| 5.5 plain | 7413bff same machine (session-l subject) | 0.971 | 0.950 | 84 / 123 / 16 | 93 / 2 |
| 5.5 plain | session-m published 8cd1f06 | 0.983 | 0.984 | 55 / 155 / 13 | 32 / 1 |
| 5.5 PGO | 5a064d7 cached (set B) | 0.970 | 0.946 | 78 / 132 / 13 | 96 / 4 |
| 5.5 PGO | 7413bff same machine (session-l subject) | 0.971 | 0.950 | 83 / 130 / 10 | 89 / 4 |
| 5.5 PGO | session-m published 8cd1f06 | 0.979 | 0.983 | 69 / 147 / 7 | 32 / 1 |
| 5.4 plain | 5a064d7 cached (set B) | 0.960 | 0.948 | 94 / 122 / 7 | 96 / 1 |
| 5.4 plain | 7413bff same machine (session-l subject) | 0.959 | 0.952 | 97 / 121 / 5 | 88 / 1 |
| 5.4 plain | session-m published 8cd1f06 | 0.973 | 0.982 | 70 / 148 / 5 | 34 / 1 |
| 5.4 PGO | 5a064d7 cached (set B) | 0.965 | 0.949 | 83 / 138 / 2 | 92 / 1 |
| 5.4 PGO | 7413bff same machine (session-l subject) | 0.966 | 0.953 | 88 / 131 / 4 | 86 / 2 |
| 5.4 PGO | session-m published 8cd1f06 | 0.975 | 0.981 | 53 / 164 / 6 | 37 / 2 |

### Per set, geomean subject / 5a064d7 and subject / b116447 (cycles / instructions)

| set | n | plain vs b cyc | ins | PGO vs b cyc | ins | 5.4 plain vs b cyc | ins | 5.4 PGO vs b cyc | ins | plain vs i cyc | ins | PGO vs 7413bff cyc | ins | 5.4 plain vs i cyc | ins | 5.4 PGO vs 7413bff cyc | ins |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| apps | 5 | 0.955 | 0.930 | 0.947 | 0.938 | 0.942 | 0.935 | 0.936 | 0.940 | 0.963 | 0.942 | 0.950 | 0.947 | 0.961 | 0.949 | 0.963 | 0.952 |
| awfy | 14 | 0.969 | 0.949 | 0.968 | 0.954 | 0.960 | 0.950 | 0.960 | 0.954 | 0.964 | 0.954 | 0.965 | 0.958 | 0.957 | 0.956 | 0.959 | 0.959 |
| corpus | 18 | 0.947 | 0.935 | 0.944 | 0.940 | 0.927 | 0.929 | 0.932 | 0.935 | 0.949 | 0.943 | 0.942 | 0.948 | 0.930 | 0.931 | 0.930 | 0.938 |
| corpus2/dev | 17 | 0.949 | 0.945 | 0.949 | 0.950 | 0.936 | 0.942 | 0.950 | 0.947 | 0.960 | 0.969 | 0.954 | 0.973 | 0.937 | 0.963 | 0.954 | 0.968 |
| corpus2/heldout | 11 | 0.976 | 0.973 | 0.977 | 0.972 | 0.966 | 0.979 | 0.965 | 0.980 | 0.963 | 0.979 | 0.961 | 0.979 | 0.957 | 0.993 | 0.959 | 0.995 |
| jit-trace | 29 | 0.992 | 0.864 | 0.991 | 0.865 | 0.987 | 0.882 | 0.987 | 0.882 | 0.995 | 0.869 | 0.994 | 0.870 | 0.988 | 0.887 | 0.989 | 0.887 |
| lib | 95 | 0.966 | 0.963 | 0.970 | 0.961 | 0.959 | 0.962 | 0.965 | 0.961 | 0.968 | 0.963 | 0.973 | 0.961 | 0.956 | 0.962 | 0.966 | 0.961 |
| programs | 25 | 0.972 | 0.957 | 0.975 | 0.960 | 0.968 | 0.965 | 0.978 | 0.967 | 0.973 | 0.960 | 0.978 | 0.962 | 0.966 | 0.965 | 0.975 | 0.967 |
| real | 5 | 1.001 | 0.984 | 0.990 | 0.985 | 0.981 | 0.986 | 0.973 | 0.986 | 1.003 | 0.985 | 0.990 | 0.986 | 0.984 | 0.987 | 0.975 | 0.987 |
| real_dis | 4 | 1.023 | 0.993 | 1.004 | 1.004 | 0.990 | 0.988 | 1.009 | 1.001 | 1.008 | 0.992 | 1.007 | 1.004 | 0.975 | 0.986 | 1.002 | 1.000 |
| ALL | 223 | 0.969 | 0.945 | 0.970 | 0.946 | 0.960 | 0.948 | 0.965 | 0.949 | 0.971 | 0.950 | 0.971 | 0.950 | 0.959 | 0.952 | 0.966 | 0.953 |

### 5.5 plain vs 5a064d7: 20 biggest improvements by cycles (>= 20 Mcyc)

| program | Mcyc | cyc ratio | ins ratio | cycle spread of readings | note |
|---|---|---|---|---|---|
| corpus/classic_vector | 319 | 0.676 | 0.636 | 6.5% (14 runs) | instructions move the same way |
| jit-trace/upval_acc | 1010 | 0.731 | 0.812 | 9.5% (14 runs) | instructions move the same way |
| lib/co_yield | 175 | 0.732 | 0.919 | 9.4% (14 runs) | instructions move the same way |
| corpus/lua_lexer | 821 | 0.760 | 0.743 | 6.0% (14 runs) | instructions move the same way |
| programs/coroutines | 1374 | 0.762 | 0.934 | 63.1% (14 runs) | instructions move the same way |
| corpus2/dev/coro_generators | 961 | 0.768 | 0.939 | 2.4% (14 runs) | instructions move the same way |
| corpus2/dev/oop_mixins | 1079 | 0.774 | 0.712 | 2.7% (14 runs) | instructions move the same way |
| lib/match | 48 | 0.775 | 0.805 | 8.8% (14 runs) | instructions move the same way |
| lib/tonumber_num | 167 | 0.779 | 0.839 | 8.3% (14 runs) | instructions move the same way |
| lib/tonumber_int | 129 | 0.830 | 0.983 | 1.7% (14 runs) | instructions move the same way |
| apps/raytrace | 93 | 0.832 | 0.816 | 4.2% (14 runs) | instructions move the same way |
| jit-trace/rec_loop | 77 | 0.834 | 0.801 | 1.4% (14 runs) | instructions move the same way |
| lib/find_pat | 217 | 0.844 | 0.822 | 1.6% (14 runs) | instructions move the same way |
| lib/str_eq | 135 | 0.846 | 0.921 | 7.7% (14 runs) | instructions move the same way |
| lib/str_lt_short | 96 | 0.849 | 0.944 | 2.3% (14 runs) | instructions move the same way |
| lib/byte | 44 | 0.866 | 0.877 | 3.3% (14 runs) | instructions move the same way |
| corpus2/dev/expr_compiler | 1150 | 0.872 | 0.884 | 2.3% (14 runs) | instructions move the same way |
| programs/oop_methods | 125 | 0.875 | 0.894 | 1.4% (14 runs) | instructions move the same way |
| lib/find_plain | 112 | 0.881 | 0.854 | 3.7% (14 runs) | instructions move the same way |
| jit-trace/field_method | 500 | 0.881 | 0.871 | 0.4% (14 runs) | instructions move the same way |

### 5.5 PGO vs 5a064d7: 20 biggest improvements by cycles (>= 20 Mcyc)

| program | Mcyc | cyc ratio | ins ratio | cycle spread of readings | note |
|---|---|---|---|---|---|
| corpus/classic_vector | 285 | 0.660 | 0.651 | 7.9% (14 runs) | instructions move the same way |
| jit-trace/upval_acc | 997 | 0.721 | 0.812 | 23.9% (14 runs) | instructions move the same way |
| corpus/lua_lexer | 700 | 0.730 | 0.736 | 4.3% (14 runs) | instructions move the same way |
| corpus2/dev/oop_mixins | 1014 | 0.747 | 0.701 | 3.1% (14 runs) | instructions move the same way |
| lib/tonumber_num | 167 | 0.788 | 0.839 | 46.6% (14 runs) | instructions move the same way |
| jit-trace/rec_loop | 76 | 0.831 | 0.802 | 2.1% (14 runs) | instructions move the same way |
| lib/sub | 106 | 0.833 | 0.892 | 3.1% (14 runs) | instructions move the same way |
| apps/raytrace | 87 | 0.837 | 0.825 | 6.9% (14 runs) | instructions move the same way |
| lib/str_lt_short | 89 | 0.838 | 0.943 | 1.7% (14 runs) | instructions move the same way |
| lib/str_eq | 135 | 0.842 | 0.921 | 12.7% (14 runs) | instructions move the same way |
| lib/tonumber_int | 119 | 0.846 | 0.972 | 7.4% (14 runs) | instructions move the same way |
| lib/match | 43 | 0.848 | 0.834 | 5.4% (14 runs) | instructions move the same way |
| lib/find_pat | 196 | 0.857 | 0.796 | 1.1% (14 runs) | instructions move the same way |
| corpus2/dev/expr_compiler | 965 | 0.858 | 0.879 | 1.7% (14 runs) | instructions move the same way |
| lib/byte | 43 | 0.868 | 0.878 | 4.7% (14 runs) | instructions move the same way |
| programs/oop_methods | 124 | 0.872 | 0.894 | 1.0% (14 runs) | instructions move the same way |
| jit-trace/field_method | 497 | 0.884 | 0.873 | 1.0% (14 runs) | instructions move the same way |
| corpus2/dev/lua_parser | 991 | 0.889 | 0.847 | 2.0% (14 runs) | instructions move the same way |
| awfy/cd | 214 | 0.893 | 0.838 | 3.1% (14 runs) | instructions move the same way |
| lib/pcall_ok | 27 | 0.897 | 0.874 | 5.0% (14 runs) | instructions move the same way |

### 5.4 plain vs 5a064d7: 20 biggest improvements by cycles (>= 20 Mcyc)

| program | Mcyc | cyc ratio | ins ratio | cycle spread of readings | note |
|---|---|---|---|---|---|
| corpus/classic_vector | 428 | 0.659 | 0.650 | 6.1% (14 runs) | instructions move the same way |
| lib/co_yield | 176 | 0.729 | 0.919 | 3.1% (14 runs) | instructions move the same way |
| corpus/lua_lexer | 828 | 0.746 | 0.755 | 3.6% (14 runs) | instructions move the same way |
| programs/coroutines | 1398 | 0.753 | 0.938 | 65.8% (14 runs) | instructions move the same way |
| corpus2/dev/coro_generators | 990 | 0.763 | 0.931 | 1.9% (14 runs) | instructions move the same way |
| lib/tonumber_num | 167 | 0.767 | 0.839 | 52.1% (14 runs) | instructions move the same way |
| corpus2/dev/oop_mixins | 1099 | 0.768 | 0.722 | 1.6% (14 runs) | instructions move the same way |
| lib/match | 48 | 0.791 | 0.799 | 9.4% (14 runs) | instructions move the same way |
| lib/find_pat | 218 | 0.806 | 0.819 | 7.5% (14 runs) | instructions move the same way |
| corpus2/dev/lua_parser | 1254 | 0.819 | 0.800 | 0.8% (14 runs) | instructions move the same way |
| lib/str_eq | 135 | 0.843 | 0.921 | 5.7% (14 runs) | instructions move the same way |
| jit-trace/upval_acc | 1173 | 0.847 | 0.803 | 6.1% (14 runs) | instructions move the same way |
| lib/tonumber_bad | 142 | 0.851 | 0.990 | 1.9% (14 runs) | instructions move the same way |
| lib/byte | 44 | 0.858 | 0.877 | 10.1% (14 runs) | instructions move the same way |
| apps/raytrace | 101 | 0.862 | 0.853 | 4.5% (14 runs) | instructions move the same way |
| corpus/astar_jumper | 1598 | 0.866 | 0.823 | 2.4% (14 runs) | instructions move the same way |
| corpus2/dev/expr_compiler | 1183 | 0.875 | 0.883 | 1.4% (14 runs) | instructions move the same way |
| lib/len_op | 50 | 0.876 | 0.999 | 6.6% (14 runs) | instructions equal: layout / IPC |
| lib/find_plain | 111 | 0.876 | 0.854 | 2.0% (14 runs) | instructions move the same way |
| corpus/csv_ini_yaml | 1648 | 0.878 | 0.891 | 2.0% (14 runs) | instructions move the same way |

### 5.4 PGO vs 5a064d7: 20 biggest improvements by cycles (>= 20 Mcyc)

| program | Mcyc | cyc ratio | ins ratio | cycle spread of readings | note |
|---|---|---|---|---|---|
| corpus/classic_vector | 393 | 0.699 | 0.685 | 1.7% (14 runs) | instructions move the same way |
| corpus/lua_lexer | 732 | 0.738 | 0.751 | 2.1% (14 runs) | instructions move the same way |
| lib/tonumber_bad | 113 | 0.783 | 0.989 | 9.2% (14 runs) | instructions move the same way |
| corpus2/dev/oop_mixins | 1038 | 0.785 | 0.716 | 3.0% (14 runs) | instructions move the same way |
| lib/tonumber_num | 167 | 0.786 | 0.839 | 40.1% (14 runs) | instructions move the same way |
| corpus2/dev/lua_parser | 1073 | 0.818 | 0.787 | 5.5% (14 runs) | instructions move the same way |
| lib/find_pat | 196 | 0.829 | 0.795 | 5.2% (14 runs) | instructions move the same way |
| jit-trace/upval_acc | 1168 | 0.843 | 0.803 | 6.9% (14 runs) | instructions move the same way |
| lib/str_eq | 135 | 0.845 | 0.921 | 5.4% (14 runs) | instructions move the same way |
| lib/match | 44 | 0.848 | 0.826 | 7.4% (14 runs) | instructions move the same way |
| lib/sub | 106 | 0.849 | 0.892 | 2.9% (14 runs) | instructions move the same way |
| lib/byte | 43 | 0.860 | 0.878 | 11.8% (14 runs) | instructions move the same way |
| apps/expr_lang | 126 | 0.865 | 0.887 | 2.8% (14 runs) | instructions move the same way |
| corpus/astar_jumper | 1530 | 0.865 | 0.819 | 3.5% (14 runs) | instructions move the same way |
| corpus2/dev/expr_compiler | 1015 | 0.869 | 0.880 | 4.1% (14 runs) | instructions move the same way |
| apps/raytrace | 95 | 0.870 | 0.862 | 6.7% (14 runs) | instructions move the same way |
| lib/len_op | 50 | 0.872 | 0.999 | 14.4% (14 runs) | instructions equal: layout / IPC |
| programs/oop_methods | 124 | 0.881 | 0.894 | 0.6% (14 runs) | instructions move the same way |
| jit-trace/field_method | 499 | 0.882 | 0.872 | 0.4% (14 runs) | instructions move the same way |
| awfy/sieve | 114 | 0.883 | 0.874 | 11.3% (14 runs) | instructions move the same way |

### 5.5 plain vs 5a064d7: regressions (instructions > +2% or cycles > +3%, >= 20 Mcyc)

| program | Mcyc | cyc ratio | ins ratio | cycle spread of readings | note |
|---|---|---|---|---|---|
| jit-trace/oop_inherit | 96 | 1.666 | 0.913 | 1.2% (14 runs) |  |
| jit-trace/dot | 1632 | 1.340 | 0.699 | 0.7% (14 runs) |  |
| lib/it_next | 49 | 1.112 | 0.997 | 5.6% (11 runs) | instructions equal: layout / IPC |
| corpus2/heldout/real_dis_ppc | 2275 | 1.092 | 1.043 | 2.7% (14 runs) | instructions move the same way |
| real/template_engine | 765 | 1.070 | 1.006 | 9.3% (14 runs) | instructions equal: layout / IPC |
| programs/metamethods | 703 | 1.068 | 0.995 | 1.4% (14 runs) | instructions equal: layout / IPC |
| lib/tonumber_flt | 206 | 1.058 | 0.993 | 2.8% (14 runs) | instructions equal: layout / IPC |
| corpus/parser_combinator | 2410 | 1.057 | 1.032 | 3.4% (14 runs) | instructions move the same way |
| lib/concat_op_str | 141 | 1.047 | 0.995 | 1.8% (5 runs) | instructions equal: layout / IPC |
| apps/markdown | 122 | 1.045 | 1.006 | 1.8% (14 runs) | instructions equal: layout / IPC |
| jit-trace/alloc_ctor | 194 | 1.037 | 0.990 | 8.4% (14 runs) | instructions equal: layout / IPC |
| lib/os_clock | 119 | 1.035 | 1.021 | 1.9% (5 runs) | instructions move the same way |
| lib/len_tbl | 142 | 1.035 | 0.987 | 2.0% (14 runs) |  |
| lib/setmt | 189 | 1.031 | 0.993 | 5.8% (14 runs) | instructions equal: layout / IPC |
| lib/tinsert_idx | 177 | 1.009 | 1.028 | 4.2% (14 runs) |  |
| lib/m_fmod | 218 | 0.997 | 1.023 | 0.7% (5 runs) |  |
| lib/m_ult | 271 | 0.988 | 1.021 | 19.0% (14 runs) |  |

### 5.5 PGO vs 5a064d7: regressions (instructions > +2% or cycles > +3%, >= 20 Mcyc)

| program | Mcyc | cyc ratio | ins ratio | cycle spread of readings | note |
|---|---|---|---|---|---|
| jit-trace/oop_inherit | 95 | 1.672 | 0.914 | 1.0% (14 runs) |  |
| jit-trace/dot | 1630 | 1.336 | 0.699 | 0.9% (14 runs) |  |
| lib/it_next | 49 | 1.108 | 0.997 | 5.6% (11 runs) | instructions equal: layout / IPC |
| lib/load_small | 168 | 1.103 | 1.000 | 2.1% (14 runs) | instructions equal: layout / IPC |
| corpus2/heldout/log_analyzer | 1835 | 1.050 | 0.976 | 2.9% (5 runs) |  |
| corpus/parser_combinator | 1959 | 1.050 | 1.046 | 7.0% (14 runs) | instructions move the same way |
| corpus2/heldout/real_dis_ppc | 1944 | 1.046 | 1.058 | 2.2% (14 runs) | instructions move the same way |
| lib/len_tbl | 141 | 1.043 | 0.987 | 4.2% (14 runs) |  |
| lib/utf8_len | 286 | 1.040 | 0.998 | 9.7% (14 runs) | instructions equal: layout / IPC |
| lib/sel_n | 335 | 1.038 | 1.000 | 10.8% (14 runs) | instructions equal: layout / IPC |
| lib/gmatch_words | 141 | 1.035 | 1.000 | 6.9% (14 runs) | instructions equal: layout / IPC |
| lib/m_tointeger | 230 | 1.032 | 1.000 | 0.4% (5 runs) | instructions equal: layout / IPC |
| real/template_engine | 608 | 1.031 | 1.017 | 1.2% (14 runs) | instructions move the same way |
| apps/markdown | 104 | 1.021 | 1.023 | 2.0% (14 runs) |  |
| lib/tinsert_idx | 155 | 1.017 | 1.031 | 6.0% (14 runs) |  |
| programs/coroutines | 1291 | 0.994 | 1.039 | 62.7% (14 runs) |  |

### 5.4 plain vs 5a064d7: regressions (instructions > +2% or cycles > +3%, >= 20 Mcyc)

| program | Mcyc | cyc ratio | ins ratio | cycle spread of readings | note |
|---|---|---|---|---|---|
| jit-trace/dot | 1637 | 1.340 | 0.701 | 0.3% (14 runs) |  |
| lib/it_next | 49 | 1.116 | 0.997 | 8.8% (11 runs) | instructions equal: layout / IPC |
| lib/m_tointeger | 237 | 1.060 | 1.000 | 0.6% (5 runs) | instructions equal: layout / IPC |
| lib/gsub_tbl | 160 | 1.052 | 0.987 | 6.0% (14 runs) |  |
| lib/fmt_x_pad | 62 | 1.045 | 0.996 | 2.5% (14 runs) | instructions equal: layout / IPC |
| programs/closures | 532 | 1.039 | 0.985 | 5.5% (14 runs) |  |
| lib/sel_count | 224 | 1.034 | 0.999 | 8.0% (14 runs) | instructions equal: layout / IPC |
| lib/tinsert_append | 197 | 1.024 | 1.023 | 17.3% (14 runs) |  |
| lib/os_clock | 121 | 1.023 | 1.020 | 0.5% (5 runs) |  |
| lib/tinsert_idx | 208 | 1.014 | 1.024 | 18.8% (14 runs) |  |
| lib/m_fmod | 221 | 1.001 | 1.023 | 4.7% (5 runs) |  |
| lib/m_ult | 275 | 0.997 | 1.021 | 3.1% (14 runs) |  |
| corpus2/heldout/stream_closures | 795 | 0.935 | 1.120 | 1.2% (5 runs) |  |

### 5.4 PGO vs 5a064d7: regressions (instructions > +2% or cycles > +3%, >= 20 Mcyc)

| program | Mcyc | cyc ratio | ins ratio | cycle spread of readings | note |
|---|---|---|---|---|---|
| jit-trace/dot | 1635 | 1.336 | 0.701 | 0.3% (14 runs) |  |
| lib/it_next | 49 | 1.126 | 0.997 | 13.8% (11 runs) | instructions equal: layout / IPC |
| lib/tinsert_idx | 194 | 1.024 | 1.026 | 8.3% (14 runs) |  |
| lib/tinsert_append | 184 | 1.017 | 1.025 | 18.6% (14 runs) |  |
| corpus2/heldout/stream_closures | 743 | 0.953 | 1.139 | 0.9% (5 runs) |  |
| programs/coroutines | 1321 | 0.935 | 1.027 | 55.9% (14 runs) |  |

### 5.5 plain vs 7413bff: 20 biggest improvements by cycles (>= 20 Mcyc)

| program | Mcyc | cyc ratio | ins ratio | cycle spread of readings | note |
|---|---|---|---|---|---|
| corpus/classic_vector | 319 | 0.686 | 0.641 | 6.5% (14 runs) | instructions move the same way |
| jit-trace/upval_acc | 1010 | 0.731 | 0.812 | 9.5% (14 runs) | instructions move the same way |
| lib/co_yield | 175 | 0.736 | 0.919 | 9.4% (14 runs) | instructions move the same way |
| corpus2/dev/coro_generators | 961 | 0.765 | 0.941 | 2.4% (14 runs) | instructions move the same way |
| programs/coroutines | 1374 | 0.769 | 0.934 | 63.1% (14 runs) | instructions move the same way |
| lib/match | 48 | 0.784 | 0.805 | 8.8% (14 runs) | instructions move the same way |
| lib/find_pat | 217 | 0.828 | 0.822 | 1.6% (14 runs) | instructions move the same way |
| corpus/lua_lexer | 821 | 0.841 | 0.831 | 6.0% (14 runs) | instructions move the same way |
| lib/tonumber_int | 129 | 0.841 | 0.983 | 1.7% (14 runs) | instructions move the same way |
| lib/str_eq | 135 | 0.846 | 0.921 | 7.7% (14 runs) | instructions move the same way |
| lib/tonumber_num | 167 | 0.850 | 0.839 | 8.3% (14 runs) | instructions move the same way |
| apps/raytrace | 93 | 0.859 | 0.853 | 4.2% (14 runs) | instructions move the same way |
| lib/byte | 44 | 0.866 | 0.877 | 3.3% (14 runs) | instructions move the same way |
| programs/oop_methods | 125 | 0.873 | 0.894 | 1.4% (14 runs) | instructions move the same way |
| lib/find_plain | 112 | 0.880 | 0.854 | 3.7% (14 runs) | instructions move the same way |
| jit-trace/field_method | 500 | 0.880 | 0.871 | 0.4% (14 runs) | instructions move the same way |
| corpus2/dev/expr_compiler | 1150 | 0.880 | 0.891 | 2.3% (14 runs) | instructions move the same way |
| corpus2/heldout/prototype_objects | 1338 | 0.885 | 0.994 | 3.8% (14 runs) | instructions equal: layout / IPC |
| jit-trace/inline_leaf | 405 | 0.887 | 0.876 | 2.5% (14 runs) | instructions move the same way |
| awfy/sieve | 111 | 0.895 | 0.850 | 2.8% (14 runs) | instructions move the same way |

### 5.5 PGO vs 7413bff: 20 biggest improvements by cycles (>= 20 Mcyc)

| program | Mcyc | cyc ratio | ins ratio | cycle spread of readings | note |
|---|---|---|---|---|---|
| corpus/classic_vector | 285 | 0.661 | 0.654 | 7.9% (14 runs) | instructions move the same way |
| jit-trace/upval_acc | 997 | 0.722 | 0.812 | 23.9% (14 runs) | instructions move the same way |
| corpus/lua_lexer | 700 | 0.807 | 0.824 | 4.3% (14 runs) | instructions move the same way |
| lib/tonumber_num | 167 | 0.819 | 0.839 | 46.6% (14 runs) | instructions move the same way |
| lib/str_eq | 135 | 0.843 | 0.921 | 12.7% (14 runs) | instructions move the same way |
| lib/match | 43 | 0.844 | 0.834 | 5.4% (14 runs) | instructions move the same way |
| lib/tonumber_int | 119 | 0.848 | 0.972 | 7.4% (14 runs) | instructions move the same way |
| lib/find_pat | 196 | 0.851 | 0.796 | 1.1% (14 runs) | instructions move the same way |
| corpus2/dev/expr_compiler | 965 | 0.855 | 0.885 | 1.7% (14 runs) | instructions move the same way |
| apps/raytrace | 87 | 0.863 | 0.854 | 6.9% (14 runs) | instructions move the same way |
| lib/byte | 43 | 0.865 | 0.878 | 4.7% (14 runs) | instructions move the same way |
| programs/oop_methods | 124 | 0.869 | 0.894 | 1.0% (14 runs) | instructions move the same way |
| lib/sub | 106 | 0.872 | 0.892 | 3.1% (14 runs) | instructions move the same way |
| jit-trace/field_method | 497 | 0.881 | 0.873 | 1.0% (14 runs) | instructions move the same way |
| corpus2/heldout/prototype_objects | 1276 | 0.884 | 0.994 | 5.8% (14 runs) | instructions equal: layout / IPC |
| awfy/cd | 214 | 0.886 | 0.843 | 3.1% (14 runs) | instructions move the same way |
| corpus2/dev/lua_parser | 991 | 0.888 | 0.849 | 2.0% (14 runs) | instructions move the same way |
| lib/m_min | 203 | 0.888 | 0.861 | 7.4% (14 runs) | instructions move the same way |
| lib/sub_1char | 99 | 0.903 | 0.938 | 13.4% (14 runs) | instructions move the same way |
| corpus/csv_ini_yaml | 1233 | 0.904 | 0.912 | 2.6% (14 runs) | instructions move the same way |

### 5.4 plain vs 7413bff: 20 biggest improvements by cycles (>= 20 Mcyc)

| program | Mcyc | cyc ratio | ins ratio | cycle spread of readings | note |
|---|---|---|---|---|---|
| corpus/classic_vector | 428 | 0.679 | 0.653 | 6.1% (14 runs) | instructions move the same way |
| corpus/lua_lexer | 828 | 0.744 | 0.755 | 3.6% (14 runs) | instructions move the same way |
| programs/coroutines | 1398 | 0.757 | 0.938 | 65.8% (14 runs) | instructions move the same way |
| corpus2/dev/coro_generators | 990 | 0.765 | 0.932 | 1.9% (14 runs) | instructions move the same way |
| lib/co_yield | 176 | 0.770 | 0.919 | 3.1% (14 runs) | instructions move the same way |
| lib/match | 48 | 0.787 | 0.799 | 9.4% (14 runs) | instructions move the same way |
| lib/len_op | 50 | 0.804 | 0.999 | 6.6% (14 runs) | instructions equal: layout / IPC |
| corpus2/dev/lua_parser | 1254 | 0.805 | 0.815 | 0.8% (14 runs) | instructions move the same way |
| lib/tonumber_num | 167 | 0.817 | 0.839 | 52.1% (14 runs) | instructions move the same way |
| lib/find_pat | 218 | 0.827 | 0.819 | 7.5% (14 runs) | instructions move the same way |
| lib/str_eq | 135 | 0.845 | 0.921 | 5.7% (14 runs) | instructions move the same way |
| jit-trace/upval_acc | 1173 | 0.846 | 0.803 | 6.1% (14 runs) | instructions move the same way |
| lib/byte | 44 | 0.850 | 0.878 | 10.1% (14 runs) | instructions move the same way |
| corpus/astar_jumper | 1598 | 0.853 | 0.832 | 2.4% (14 runs) | instructions move the same way |
| corpus2/heldout/prototype_objects | 1312 | 0.857 | 0.986 | 3.7% (14 runs) | instructions move the same way |
| lib/tonumber_bad | 142 | 0.867 | 0.990 | 1.9% (14 runs) | instructions move the same way |
| jit-trace/field_method | 501 | 0.867 | 0.871 | 0.6% (14 runs) | instructions move the same way |
| lib/find_plain | 111 | 0.869 | 0.854 | 2.0% (14 runs) | instructions move the same way |
| corpus2/dev/expr_compiler | 1183 | 0.874 | 0.896 | 1.4% (14 runs) | instructions move the same way |
| programs/oop_methods | 124 | 0.875 | 0.894 | 0.9% (14 runs) | instructions move the same way |

### 5.4 PGO vs 7413bff: 20 biggest improvements by cycles (>= 20 Mcyc)

| program | Mcyc | cyc ratio | ins ratio | cycle spread of readings | note |
|---|---|---|---|---|---|
| corpus/classic_vector | 393 | 0.703 | 0.687 | 1.7% (14 runs) | instructions move the same way |
| corpus/lua_lexer | 732 | 0.744 | 0.751 | 2.1% (14 runs) | instructions move the same way |
| lib/tonumber_bad | 113 | 0.794 | 0.989 | 9.2% (14 runs) | instructions move the same way |
| corpus2/dev/lua_parser | 1073 | 0.803 | 0.803 | 5.5% (14 runs) | instructions move the same way |
| lib/tonumber_num | 167 | 0.824 | 0.839 | 40.1% (14 runs) | instructions move the same way |
| lib/find_pat | 196 | 0.833 | 0.795 | 5.2% (14 runs) | instructions move the same way |
| jit-trace/upval_acc | 1168 | 0.844 | 0.803 | 6.9% (14 runs) | instructions move the same way |
| lib/match | 44 | 0.844 | 0.826 | 7.4% (14 runs) | instructions move the same way |
| lib/str_eq | 135 | 0.844 | 0.921 | 5.4% (14 runs) | instructions move the same way |
| corpus/astar_jumper | 1530 | 0.849 | 0.828 | 3.5% (14 runs) | instructions move the same way |
| lib/byte | 43 | 0.859 | 0.878 | 11.8% (14 runs) | instructions move the same way |
| corpus2/heldout/prototype_objects | 1276 | 0.863 | 0.991 | 3.4% (14 runs) | instructions equal: layout / IPC |
| corpus2/dev/expr_compiler | 1015 | 0.866 | 0.893 | 4.1% (14 runs) | instructions move the same way |
| lib/len_op | 50 | 0.871 | 0.999 | 14.4% (14 runs) | instructions equal: layout / IPC |
| programs/oop_methods | 124 | 0.873 | 0.894 | 0.6% (14 runs) | instructions move the same way |
| lib/sub | 106 | 0.875 | 0.892 | 2.9% (14 runs) | instructions move the same way |
| awfy/cd | 233 | 0.878 | 0.852 | 1.5% (14 runs) | instructions move the same way |
| jit-trace/field_method | 499 | 0.883 | 0.872 | 0.4% (14 runs) | instructions move the same way |
| awfy/sieve | 114 | 0.889 | 0.874 | 11.3% (14 runs) | instructions move the same way |
| jit-trace/inline_leaf | 407 | 0.892 | 0.877 | 3.2% (14 runs) | instructions move the same way |

### 5.5 plain vs 7413bff: regressions (instructions > +2% or cycles > +3%, >= 20 Mcyc)

| program | Mcyc | cyc ratio | ins ratio | cycle spread of readings | note |
|---|---|---|---|---|---|
| jit-trace/oop_inherit | 96 | 1.674 | 0.913 | 1.2% (14 runs) |  |
| jit-trace/dot | 1632 | 1.337 | 0.699 | 0.7% (14 runs) |  |
| corpus2/dev/regex_nfa | 697 | 1.111 | 1.018 | 7.8% (14 runs) | instructions move the same way |
| lib/it_next | 49 | 1.104 | 0.997 | 5.6% (11 runs) | instructions equal: layout / IPC |
| corpus2/heldout/real_dis_ppc | 2275 | 1.077 | 1.043 | 2.7% (14 runs) | instructions move the same way |
| real/template_engine | 765 | 1.066 | 1.006 | 9.3% (14 runs) | instructions equal: layout / IPC |
| programs/metamethods | 703 | 1.065 | 0.995 | 1.4% (14 runs) | instructions equal: layout / IPC |
| lib/tonumber_flt | 206 | 1.056 | 0.993 | 2.8% (14 runs) | instructions equal: layout / IPC |
| lib/tonumber_bad | 156 | 1.049 | 0.987 | 2.4% (14 runs) |  |
| lib/sel_n | 339 | 1.049 | 1.000 | 7.2% (14 runs) | instructions equal: layout / IPC |
| lib/error_tbl | 274 | 1.046 | 1.009 | 4.1% (14 runs) | instructions equal: layout / IPC |
| lib/load_small | 236 | 1.045 | 1.001 | 1.9% (14 runs) | instructions equal: layout / IPC |
| corpus/parser_combinator | 2410 | 1.042 | 1.032 | 3.4% (14 runs) | instructions move the same way |
| lib/os_clock | 119 | 1.036 | 1.021 | 1.9% (5 runs) | instructions move the same way |
| apps/markdown | 122 | 1.034 | 1.008 | 1.8% (14 runs) | instructions equal: layout / IPC |
| apps/events | 168 | 1.032 | 1.000 | 9.8% (11 runs) | instructions equal: layout / IPC |
| lib/tinsert_idx | 177 | 1.020 | 1.029 | 4.2% (14 runs) |  |
| lib/m_fmod | 218 | 0.996 | 1.023 | 0.7% (5 runs) |  |
| lib/m_ult | 271 | 0.992 | 1.021 | 19.0% (14 runs) |  |

### 5.5 PGO vs 7413bff: regressions (instructions > +2% or cycles > +3%, >= 20 Mcyc)

| program | Mcyc | cyc ratio | ins ratio | cycle spread of readings | note |
|---|---|---|---|---|---|
| jit-trace/oop_inherit | 95 | 1.675 | 0.915 | 1.0% (14 runs) |  |
| jit-trace/dot | 1630 | 1.336 | 0.699 | 0.9% (14 runs) |  |
| lib/load_small | 168 | 1.141 | 1.000 | 2.1% (14 runs) | instructions equal: layout / IPC |
| lib/it_next | 49 | 1.104 | 0.997 | 5.6% (11 runs) | instructions equal: layout / IPC |
| lib/tonumber_bad | 122 | 1.095 | 0.985 | 8.4% (14 runs) |  |
| corpus2/heldout/real_dis_ppc | 1944 | 1.064 | 1.060 | 2.2% (14 runs) | instructions move the same way |
| corpus2/heldout/log_analyzer | 1835 | 1.040 | 0.977 | 2.9% (5 runs) |  |
| lib/utf8_len | 286 | 1.037 | 0.998 | 9.7% (14 runs) | instructions equal: layout / IPC |
| lib/gmatch_words | 141 | 1.035 | 1.000 | 6.9% (14 runs) | instructions equal: layout / IPC |
| lib/m_tointeger | 230 | 1.032 | 1.000 | 0.4% (5 runs) | instructions equal: layout / IPC |
| lib/tinsert_idx | 155 | 1.024 | 1.032 | 6.0% (14 runs) |  |
| corpus/parser_combinator | 1959 | 1.021 | 1.047 | 7.0% (14 runs) |  |
| apps/markdown | 104 | 1.006 | 1.024 | 2.0% (14 runs) |  |
| programs/coroutines | 1291 | 0.963 | 1.036 | 62.7% (14 runs) |  |

### 5.4 plain vs 7413bff: regressions (instructions > +2% or cycles > +3%, >= 20 Mcyc)

| program | Mcyc | cyc ratio | ins ratio | cycle spread of readings | note |
|---|---|---|---|---|---|
| jit-trace/dot | 1637 | 1.338 | 0.701 | 0.3% (14 runs) |  |
| lib/it_next | 49 | 1.107 | 0.997 | 8.8% (11 runs) | instructions equal: layout / IPC |
| real/template_engine | 773 | 1.046 | 1.004 | 1.9% (14 runs) | instructions equal: layout / IPC |
| lib/gsub_tbl | 160 | 1.039 | 0.987 | 6.0% (14 runs) |  |
| lib/fmt_x_pad | 62 | 1.031 | 0.996 | 2.5% (14 runs) | instructions equal: layout / IPC |
| lib/os_clock | 121 | 1.022 | 1.020 | 0.5% (5 runs) |  |
| lib/tinsert_idx | 208 | 1.018 | 1.024 | 18.8% (14 runs) |  |
| lib/tinsert_append | 197 | 1.013 | 1.023 | 17.3% (14 runs) |  |
| lib/m_fmod | 221 | 1.001 | 1.023 | 4.7% (5 runs) |  |
| lib/m_ult | 275 | 0.996 | 1.021 | 3.1% (14 runs) |  |
| corpus2/heldout/stream_closures | 795 | 0.948 | 1.128 | 1.2% (5 runs) |  |

### 5.4 PGO vs 7413bff: regressions (instructions > +2% or cycles > +3%, >= 20 Mcyc)

| program | Mcyc | cyc ratio | ins ratio | cycle spread of readings | note |
|---|---|---|---|---|---|
| jit-trace/dot | 1635 | 1.338 | 0.701 | 0.3% (14 runs) |  |
| lib/it_next | 49 | 1.110 | 0.997 | 13.8% (11 runs) | instructions equal: layout / IPC |
| lib/tonumber_base | 271 | 1.040 | 1.017 | 15.0% (14 runs) | instructions move the same way |
| corpus2/heldout/real_dis_ppc | 1970 | 1.040 | 1.037 | 6.3% (14 runs) | instructions move the same way |
| lib/tinsert_idx | 194 | 1.016 | 1.026 | 8.3% (14 runs) |  |
| lib/tinsert_append | 184 | 1.014 | 1.025 | 18.6% (14 runs) |  |
| corpus2/heldout/stream_closures | 743 | 0.967 | 1.141 | 0.9% (5 runs) |  |
| programs/coroutines | 1321 | 0.955 | 1.027 | 55.9% (14 runs) |  |

### 5.5 plain: 15 biggest gaps to LuaJIT (ours/LJ cycles)

| program | Mcyc | cyc/LJ | ins/LJ |
|---|---|---|---|
| awfy/permute | 36 | 2.739 | 3.519 |
| apps/raytrace | 93 | 2.291 | 1.767 |
| corpus2/dev/text_adventure | 2583 | 2.177 | 2.163 |
| lib/fmt_s | 7 | 2.140 | 2.296 |
| awfy/cd | 230 | 2.135 | 2.050 |
| lib/byte_multi | 38 | 1.889 | 3.163 |
| awfy/towers | 44 | 1.875 | 1.648 |
| awfy/json | 136 | 1.848 | 2.075 |
| corpus2/heldout/stream_closures | 767 | 1.844 | 2.059 |
| corpus/lua_lexer | 821 | 1.831 | 1.719 |
| lib/tremove_pop | 244 | 1.781 | 2.122 |
| apps/events | 168 | 1.762 | 2.041 |
| awfy/queens | 96 | 1.744 | 1.877 |
| lib/co_yield | 175 | 1.712 | 1.729 |
| jit-trace/type_entry | 1442 | 1.699 | 1.867 |

### 5.5 plain: 15 biggest wins over LuaJIT (>= 20 Mcyc)

| program | Mcyc | cyc/LJ | ins/LJ |
|---|---|---|---|
| lib/sel_loop | 208 | 0.040 | 0.062 |
| lib/error_tbl | 274 | 0.090 | 0.080 |
| jit-trace/int_rec2 | 642 | 0.128 | 1.398 |
| lib/pcall_err | 119 | 0.156 | 0.136 |
| programs/pcall_error | 1458 | 0.175 | 0.185 |
| lib/tfront_1k | 78 | 0.192 | 0.076 |
| lib/match | 48 | 0.246 | 0.255 |
| lib/tonumber_flt | 206 | 0.265 | 0.392 |
| lib/it_next | 49 | 0.272 | 0.701 |
| corpus/sha2_base64 | 385 | 0.273 | 0.301 |
| lib/find_pat | 217 | 0.308 | 0.371 |
| jit-trace/int_sum | 871 | 0.346 | 1.107 |
| apps/markdown | 122 | 0.370 | 0.269 |
| lib/gsub_str | 162 | 0.406 | 0.502 |
| programs/oop_methods | 125 | 0.409 | 0.794 |

### 5.5 PGO: 15 biggest gaps to LuaJIT (ours/LJ cycles)

| program | Mcyc | cyc/LJ | ins/LJ |
|---|---|---|---|
| awfy/permute | 36 | 2.719 | 3.512 |
| apps/raytrace | 87 | 2.151 | 1.670 |
| lib/fmt_s | 6 | 2.017 | 2.216 |
| awfy/cd | 214 | 1.984 | 1.935 |
| lib/byte_multi | 38 | 1.888 | 3.158 |
| awfy/towers | 43 | 1.828 | 1.627 |
| corpus2/dev/text_adventure | 2130 | 1.795 | 1.938 |
| lib/find_plain | 117 | 1.733 | 1.772 |
| awfy/queens | 94 | 1.700 | 1.856 |
| jit-trace/type_entry | 1442 | 1.699 | 1.867 |
| lib/co_yield | 171 | 1.681 | 1.634 |
| corpus2/heldout/stream_closures | 690 | 1.660 | 1.912 |
| real/lua_interp | 97 | 1.635 | 2.141 |
| lib/getmt | 14 | 1.626 | 2.286 |
| jit-trace/type_entry_stable | 1375 | 1.604 | 1.778 |

### 5.5 PGO: 15 biggest wins over LuaJIT (>= 20 Mcyc)

| program | Mcyc | cyc/LJ | ins/LJ |
|---|---|---|---|
| lib/sel_loop | 208 | 0.040 | 0.062 |
| lib/error_tbl | 213 | 0.070 | 0.062 |
| lib/pcall_err | 96 | 0.126 | 0.103 |
| jit-trace/int_rec2 | 640 | 0.127 | 1.398 |
| programs/pcall_error | 1218 | 0.146 | 0.150 |
| lib/tfront_1k | 75 | 0.185 | 0.070 |
| lib/tonumber_flt | 167 | 0.215 | 0.348 |
| lib/match | 43 | 0.223 | 0.235 |
| corpus/sha2_base64 | 370 | 0.262 | 0.289 |
| lib/it_next | 49 | 0.270 | 0.700 |
| lib/find_pat | 196 | 0.278 | 0.329 |
| apps/markdown | 104 | 0.315 | 0.249 |
| lib/gsub_str | 126 | 0.316 | 0.395 |
| jit-trace/int_sum | 872 | 0.347 | 1.107 |
| lib/tpack | 116 | 0.378 | 0.392 |

## Focus programs (cycles in M; ratios of cycles)

| program | plain Mcyc | PGO Mcyc | LJ Mcyc | PUC Mcyc | plain/LJ | PGO/LJ | plain/PUC | PGO/PUC | plain vs b cyc | ins | PGO vs b cyc | ins | plain vs 7413bff cyc | PGO vs 7413bff cyc | PGO/LJ 5a064d7 |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| programs/nbody | 336.1 | 335.5 | 345.6 | 4246.2 | 0.973 | 0.971 | 0.079 | 0.079 | 0.980 | 0.973 | 0.971 | 0.974 | 0.984 | 0.985 | 0.999 |
| programs/fib | 621.6 | 623.1 | 469.7 | 4139.5 | 1.323 | 1.327 | 0.150 | 0.151 | 0.985 | 0.984 | 0.987 | 0.984 | 0.989 | 0.993 | 1.343 |
| programs/binary_trees | 1204.4 | 979.2 | 2007.4 | 4389.1 | 0.600 | 0.488 | 0.274 | 0.223 | 0.990 | 0.983 | 0.950 | 0.989 | 1.020 | 0.994 | 0.513 |
| programs/queens | 1807.7 | 1808.4 | 1907.1 | 4368.7 | 0.948 | 0.948 | 0.414 | 0.414 | 0.971 | 0.922 | 0.980 | 0.922 | 0.978 | 0.980 | 0.968 |
| apps/raytrace | 92.9 | 87.3 | 40.6 | 262.7 | 2.291 | 2.151 | 0.354 | 0.332 | 0.832 | 0.816 | 0.837 | 0.825 | 0.859 | 0.863 | 2.569 |
| awfy/cd | 229.9 | 213.5 | 107.6 | 437.5 | 2.135 | 1.984 | 0.525 | 0.488 | 0.899 | 0.828 | 0.893 | 0.838 | 0.903 | 0.886 | 2.221 |
| awfy/list | 93.8 | 92.2 | 88.9 | 418.8 | 1.055 | 1.037 | 0.224 | 0.220 | 1.001 | 0.949 | 1.003 | 0.950 | 0.997 | 0.995 | 1.034 |
| awfy/richards | 55.9 | 54.6 | 34.7 | 197.7 | 1.611 | 1.574 | 0.283 | 0.276 | 0.938 | 0.931 | 0.954 | 0.939 | 0.908 | 0.922 | 1.650 |
| awfy/deltablue | 101.8 | 96.0 | 103.5 | 157.9 | 0.984 | 0.928 | 0.645 | 0.608 | 0.952 | 0.949 | 0.963 | 0.968 | 0.954 | 0.970 | 0.963 |
| awfy/havlak | 146.4 | 127.1 | 115.2 | 217.5 | 1.271 | 1.103 | 0.673 | 0.584 | 0.994 | 0.982 | 0.978 | 0.990 | 0.969 | 0.968 | 1.128 |
| awfy/permute | 35.9 | 35.7 | 13.1 | 246.3 | 2.739 | 2.719 | 0.146 | 0.145 | 0.992 | 0.980 | 0.998 | 0.982 | 0.985 | 0.991 | 2.724 |
| awfy/towers | 44.0 | 42.9 | 23.5 | 215.1 | 1.875 | 1.828 | 0.205 | 0.199 | 0.941 | 0.958 | 0.943 | 0.966 | 0.944 | 0.944 | 1.939 |
| corpus/lua_lexer | 820.8 | 700.0 | 448.2 | 3281.3 | 1.831 | 1.562 | 0.250 | 0.213 | 0.760 | 0.743 | 0.730 | 0.736 | 0.841 | 0.807 | 2.138 |
| corpus/classic_vector | 318.5 | 285.4 | 240.7 | 2945.5 | 1.324 | 1.186 | 0.108 | 0.097 | 0.676 | 0.636 | 0.660 | 0.651 | 0.686 | 0.661 | 1.797 |
| corpus2/heldout/dijkstra_routes | 1124.1 | 1087.3 | 1265.1 | 3142.9 | 0.889 | 0.859 | 0.358 | 0.346 | 0.958 | 0.936 | 0.955 | 0.937 | 0.958 | 0.952 | 0.900 |
| corpus2/dev/oop_mixins | 1079.2 | 1014.3 | 865.1 | 2464.3 | 1.248 | 1.173 | 0.438 | 0.412 | 0.774 | 0.712 | 0.747 | 0.701 | 0.961 | 0.947 | 1.570 |
| jit-trace/rec_loop | 76.8 | 75.8 | 141.9 | 1440.3 | 0.541 | 0.534 | 0.053 | 0.053 | 0.834 | 0.801 | 0.831 | 0.802 | 0.943 | 0.948 | 0.643 |
| corpus2/heldout/roguelike_turns | 798.3 | 761.5 | 623.3 | 3311.7 | 1.281 | 1.222 | 0.241 | 0.230 | 0.992 | 0.966 | 0.992 | 0.968 | 0.938 | 0.947 | 1.231 |
| corpus2/heldout/vending_fsm | 961.5 | 879.0 | 824.2 | 3290.3 | 1.167 | 1.066 | 0.292 | 0.267 | 0.944 | 0.953 | 0.932 | 0.951 | 0.983 | 0.982 | 1.144 |
| corpus2/heldout/real_dis_mips | 2099.0 | 1801.7 | 1786.6 | 3808.7 | 1.175 | 1.008 | 0.551 | 0.473 | 0.958 | 0.952 | 0.951 | 0.966 | 0.954 | 0.966 | 1.061 |
| awfy/queens | 96.0 | 93.5 | 55.0 | 401.8 | 1.744 | 1.700 | 0.239 | 0.233 | 0.995 | 0.971 | 0.997 | 0.973 | 1.003 | 0.999 | 1.704 |

## Per-program table (5.5 and 5.4; Mcyc, ratios vs 5a064d7 and LJ)

| program | n runs (plain) | plain | PGO | p54 | g54 | LJ | PUC55 | PUC54 | plain/LJ | PGO/LJ | plain vs b cyc | ins | PGO vs b cyc | ins | p54 vs b cyc | ins | g54 vs b cyc | ins | out/exp (all 4) |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| apps/events | 11 | 168.1 | 143.1 | 175.2 | 151.9 | 95.4 | 293.7 | 289.5 | 1.762 | 1.500 | 1.026 | 0.999 | 1.022 | 1.002 | 0.992 | 0.998 | 0.989 | 1.000 | ok |
| apps/expr_lang | 14 | 129.7 | 116.8 | 140.4 | 126.3 | 96.7 | 241.7 | 253.2 | 1.341 | 1.208 | 0.956 | 0.908 | 0.934 | 0.911 | 0.881 | 0.884 | 0.865 | 0.887 | ok |
| apps/lru_cache | 5 | 133.2 | 123.3 | 172.5 | 164.0 | 106.4 | 219.8 | 253.9 | 1.252 | 1.158 | 0.932 | 0.936 | 0.935 | 0.943 | 0.963 | 0.952 | 0.968 | 0.958 | ok |
| apps/markdown | 14 | 121.9 | 103.8 | 123.3 | 107.0 | 329.7 | 412.6 | 390.8 | 0.370 | 0.315 | 1.045 | 1.006 | 1.021 | 1.023 | 1.021 | 0.994 | 0.999 | 1.002 | ok |
| apps/raytrace | 14 | 92.9 | 87.3 | 101.4 | 95.0 | 40.6 | 262.7 | 281.7 | 2.291 | 2.151 | 0.832 | 0.816 | 0.837 | 0.825 | 0.862 | 0.853 | 0.870 | 0.862 | ok |
| awfy/bounce | 5 | 59.1 | 57.5 | 63.2 | 61.4 | 47.7 | 511.2 | 537.1 | 1.239 | 1.207 | 0.981 | 0.964 | 0.976 | 0.966 | 0.952 | 0.947 | 0.947 | 0.951 | ok |
| awfy/cd | 14 | 229.9 | 213.5 | 250.1 | 232.9 | 107.6 | 437.5 | 464.4 | 2.135 | 1.984 | 0.899 | 0.828 | 0.893 | 0.838 | 0.882 | 0.836 | 0.888 | 0.846 | ok |
| awfy/deltablue | 14 | 101.8 | 96.0 | 110.1 | 102.8 | 103.5 | 157.9 | 165.2 | 0.984 | 0.928 | 0.952 | 0.949 | 0.963 | 0.968 | 0.935 | 0.947 | 0.936 | 0.960 | ok |
| awfy/havlak | 14 | 146.4 | 127.1 | 169.6 | 151.8 | 115.2 | 217.5 | 225.5 | 1.271 | 1.103 | 0.994 | 0.982 | 0.978 | 0.990 | 0.980 | 0.985 | 0.981 | 0.992 | ok |
| awfy/json | 5 | 135.7 | 114.4 | 147.7 | 128.1 | 73.4 | 262.9 | 271.8 | 1.848 | 1.558 | 0.970 | 0.961 | 0.947 | 0.967 | 0.971 | 0.967 | 0.980 | 0.975 | ok |
| awfy/list | 14 | 93.8 | 92.2 | 94.5 | 92.7 | 88.9 | 418.8 | 477.9 | 1.055 | 1.037 | 1.001 | 0.949 | 1.003 | 0.950 | 0.977 | 0.954 | 0.976 | 0.956 | ok |
| awfy/mandelbrot | 5 | 272.1 | 271.8 | 272.0 | 271.6 | 273.1 | 1529.3 | 1379.3 | 0.996 | 0.995 | 0.998 | 0.991 | 0.998 | 0.991 | 0.998 | 0.990 | 0.997 | 0.991 | ok |
| awfy/nbody | 5 | 91.2 | 90.7 | 95.7 | 93.2 | 94.6 | 1504.3 | 1603.8 | 0.964 | 0.958 | 0.983 | 0.978 | 0.990 | 0.979 | 1.002 | 0.977 | 0.986 | 0.978 | ok |
| awfy/permute | 14 | 35.9 | 35.7 | 36.9 | 36.5 | 13.1 | 246.3 | 257.2 | 2.739 | 2.719 | 0.992 | 0.980 | 0.998 | 0.982 | 0.993 | 0.980 | 0.985 | 0.982 | ok |
| awfy/queens | 14 | 96.0 | 93.5 | 79.3 | 76.6 | 55.0 | 401.8 | 403.4 | 1.744 | 1.700 | 0.995 | 0.971 | 0.997 | 0.973 | 0.963 | 0.957 | 0.962 | 0.958 | ok |
| awfy/richards | 14 | 55.9 | 54.6 | 57.1 | 55.4 | 34.7 | 197.7 | 199.3 | 1.611 | 1.574 | 0.938 | 0.931 | 0.954 | 0.939 | 0.957 | 0.931 | 0.963 | 0.939 | ok |
| awfy/sieve | 14 | 111.3 | 111.8 | 114.6 | 113.7 | 156.4 | 1032.3 | 1085.2 | 0.712 | 0.715 | 0.906 | 0.850 | 0.913 | 0.850 | 0.902 | 0.874 | 0.883 | 0.874 | ok |
| awfy/storage | 5 | 103.5 | 78.3 | 145.1 | 115.5 | 154.0 | 255.9 | 262.0 | 0.672 | 0.509 | 1.022 | 1.010 | 1.002 | 1.013 | 0.993 | 1.007 | 1.022 | 1.009 | ok |
| awfy/towers | 14 | 44.0 | 42.9 | 45.0 | 43.6 | 23.5 | 215.1 | 213.9 | 1.875 | 1.828 | 0.941 | 0.958 | 0.943 | 0.966 | 0.938 | 0.959 | 0.942 | 0.966 | ok |
| corpus/astar_jumper | 14 | 1553.4 | 1485.9 | 1597.8 | 1530.4 | 997.4 | 3280.6 | 3358.7 | 1.557 | 1.490 | 0.939 | 0.896 | 0.938 | 0.885 | 0.866 | 0.823 | 0.865 | 0.819 | ok |
| corpus/class_middleclass | 14 | 466.6 | 457.1 | 483.7 | 474.5 | 319.5 | 3046.3 | 3098.9 | 1.460 | 1.431 | 0.973 | 0.972 | 0.977 | 0.974 | 0.953 | 0.973 | 0.960 | 0.974 | ok |
| corpus/classic_vector | 14 | 318.5 | 285.4 | 428.1 | 393.1 | 240.7 | 2945.5 | 2905.4 | 1.324 | 1.186 | 0.676 | 0.636 | 0.660 | 0.651 | 0.659 | 0.650 | 0.699 | 0.685 | ok |
| corpus/containers_lru_heap | 5 | 1278.4 | 1210.5 | 1633.2 | 1525.1 | 1417.3 | 2675.7 | 3035.3 | 0.902 | 0.854 | 0.999 | 0.989 | 1.009 | 0.989 | 0.981 | 0.990 | 0.980 | 0.989 | ok |
| corpus/coro_scheduler | 14 | 1410.3 | 1285.2 | 1504.4 | 1372.4 | 941.9 | 2772.8 | 2851.7 | 1.497 | 1.365 | 0.940 | 0.970 | 0.984 | 1.018 | 0.931 | 0.970 | 0.979 | 1.000 | ok |
| corpus/csv_ini_yaml | 14 | 1454.7 | 1233.1 | 1647.6 | 1446.0 | 1771.4 | 2948.4 | 3020.4 | 0.821 | 0.696 | 0.932 | 0.914 | 0.917 | 0.912 | 0.878 | 0.891 | 0.914 | 0.885 | ok |
| corpus/inspect_lua | 14 | 1467.0 | 1300.9 | 1575.2 | 1406.4 | 1056.4 | 2901.3 | 2968.0 | 1.389 | 1.231 | 0.939 | 0.986 | 0.936 | 0.987 | 0.941 | 0.988 | 0.935 | 0.984 | ok |
| corpus/json_rxi | 5 | 1809.7 | 1507.7 | 1881.8 | 1561.0 | 1222.9 | 3647.9 | 3646.8 | 1.480 | 1.233 | 0.997 | 0.999 | 0.995 | 1.001 | 0.987 | 0.995 | 0.987 | 0.996 | ok |
| corpus/lua_lexer | 14 | 820.8 | 700.0 | 828.2 | 732.3 | 448.2 | 3281.3 | 3358.7 | 1.831 | 1.562 | 0.760 | 0.743 | 0.730 | 0.736 | 0.746 | 0.755 | 0.738 | 0.751 | ok |
| corpus/markdown_conv | 14 | 1337.3 | 1177.2 | 1375.5 | 1228.2 | 3029.8 | 3746.9 | 3671.3 | 0.441 | 0.389 | 0.966 | 0.945 | 0.947 | 0.939 | 0.979 | 0.958 | 0.974 | 0.955 | ok |
| corpus/msgpack_pure | 5 | 2020.3 | 1742.3 | 2133.8 | 1851.9 | 4473.7 | 3436.5 | 3601.3 | 0.452 | 0.389 | 1.003 | 0.989 | 1.000 | 0.999 | 0.981 | 0.987 | 0.985 | 0.984 | ok |
| corpus/mustache_lustache | 14 | 1792.6 | 1434.4 | 1861.1 | 1533.1 | 1537.1 | 3296.2 | 3446.8 | 1.166 | 0.933 | 1.001 | 0.990 | 0.997 | 0.999 | 0.956 | 0.980 | 0.975 | 0.991 | ok |
| corpus/parser_combinator | 14 | 2410.3 | 1959.4 | 2518.9 | 1923.3 | 1563.1 | 3390.6 | 3391.8 | 1.542 | 1.253 | 1.057 | 1.032 | 1.050 | 1.046 | 1.001 | 0.985 | 0.921 | 0.989 | ok |
| corpus/penlight_mix | 5 | 2214.5 | 2074.8 | 2314.0 | 2179.9 | 2678.7 | 3992.1 | 3970.3 | 0.827 | 0.775 | 0.966 | 0.968 | 0.976 | 0.978 | 0.974 | 0.966 | 0.974 | 0.972 | ok |
| corpus/serpent_ser | 5 | 2011.7 | 1728.2 | 2056.3 | 1770.4 | 2426.7 | 3260.6 | 3254.8 | 0.829 | 0.712 | 1.007 | 0.979 | 1.000 | 0.977 | 0.984 | 0.977 | 0.985 | 0.980 | ok |
| corpus/sha2_base64 | 5 | 384.7 | 370.2 | 382.5 | 368.7 | 1411.3 | 3328.6 | 3384.1 | 0.273 | 0.262 | 0.993 | 0.973 | 1.009 | 0.978 | 0.989 | 0.973 | 0.996 | 0.977 | ok |
| corpus/template_etlua | 14 | 1574.8 | 1250.8 | 1558.8 | 1287.4 | 1826.0 | 2680.1 | 2700.4 | 0.862 | 0.685 | 0.999 | 0.995 | 1.012 | 1.000 | 0.994 | 0.984 | 1.009 | 0.993 | ok |
| corpus/testrunner_luaunit | 5 | 1454.7 | 1174.9 | 1596.6 | 1314.6 | 1390.7 | 2601.9 | 2628.8 | 1.046 | 0.845 | 0.987 | 0.955 | 0.961 | 0.961 | 0.987 | 0.981 | 0.985 | 0.996 | ok |
| corpus2/dev/bignum_arith | 5 | 773.2 | 708.4 | 861.5 | 803.7 | 660.2 | 2352.8 | 2412.4 | 1.171 | 1.073 | 0.984 | 0.968 | 0.977 | 0.967 | 0.997 | 0.967 | 1.000 | 0.965 | ok |
| corpus2/dev/coro_generators | 14 | 961.3 | 894.8 | 990.2 | 918.4 | 713.4 | 1807.9 | 1836.1 | 1.347 | 1.254 | 0.768 | 0.939 | 0.930 | 0.997 | 0.763 | 0.931 | 0.925 | 0.972 | ok |
| corpus2/dev/csv_report | 14 | 1102.6 | 978.5 | 1103.2 | 992.3 | 1262.6 | 2165.3 | 2381.4 | 0.873 | 0.775 | 0.974 | 1.016 | 0.966 | 1.013 | 0.951 | 0.968 | 0.945 | 0.965 | ok |
| corpus2/dev/diff_levenshtein | 5 | 420.9 | 396.1 | 433.4 | 410.3 | 329.3 | 1776.3 | 1850.8 | 1.278 | 1.203 | 0.986 | 0.981 | 0.982 | 0.981 | 0.999 | 0.984 | 1.023 | 0.984 | ok |
| corpus2/dev/ecs_world | 5 | 927.0 | 829.4 | 981.2 | 854.0 | 916.1 | 2115.3 | 2160.1 | 1.012 | 0.905 | 0.986 | 0.975 | 0.990 | 0.979 | 0.987 | 0.990 | 0.994 | 1.002 | ok |
| corpus2/dev/event_promise | 14 | 366.6 | 347.6 | 440.3 | 424.1 | 467.9 | 1906.6 | 1939.2 | 0.783 | 0.743 | 0.985 | 0.965 | 0.987 | 0.968 | 0.971 | 0.968 | 0.981 | 0.968 | ok |
| corpus2/dev/expr_compiler | 14 | 1150.2 | 965.5 | 1182.6 | 1015.2 | 1080.2 | 2081.3 | 2231.0 | 1.065 | 0.894 | 0.872 | 0.884 | 0.858 | 0.879 | 0.875 | 0.883 | 0.869 | 0.880 | ok |
| corpus2/dev/iter_pipeline | 14 | 169.9 | 162.3 | 175.7 | 169.4 | 132.2 | 2865.2 | 2874.5 | 1.285 | 1.228 | 0.979 | 0.980 | 0.981 | 0.986 | 0.966 | 0.980 | 0.969 | 0.983 | ok |
| corpus2/dev/lua_parser | 14 | 1192.9 | 991.2 | 1254.3 | 1072.5 | 790.7 | 2461.5 | 2499.0 | 1.509 | 1.254 | 0.947 | 0.855 | 0.889 | 0.847 | 0.819 | 0.800 | 0.818 | 0.787 | ok |
| corpus2/dev/memo_cache | 14 | 1544.4 | 1408.9 | 1579.3 | 1454.6 | 1674.7 | 2669.5 | 2763.8 | 0.922 | 0.841 | 0.961 | 0.941 | 0.950 | 0.935 | 0.968 | 0.964 | 0.963 | 0.966 | ok |
| corpus2/dev/oop_mixins | 14 | 1079.2 | 1014.3 | 1098.7 | 1038.3 | 865.1 | 2464.3 | 2520.8 | 1.248 | 1.173 | 0.774 | 0.712 | 0.747 | 0.701 | 0.768 | 0.722 | 0.785 | 0.716 | ok |
| corpus2/dev/particles_physics | 5 | 399.6 | 357.2 | 490.9 | 459.6 | 379.7 | 1719.5 | 1811.4 | 1.052 | 0.941 | 0.996 | 0.961 | 0.959 | 0.967 | 0.971 | 0.969 | 0.970 | 0.974 | ok |
| corpus2/dev/real_dis_arm | 5 | 2492.5 | 2177.0 | 2359.0 | 2144.7 | 1947.2 | 4487.3 | 4484.7 | 1.280 | 1.118 | 1.027 | 0.985 | 1.001 | 0.995 | 0.969 | 0.986 | 0.992 | 0.997 | ok |
| corpus2/dev/real_dis_x86 | 5 | 1549.9 | 1370.9 | 1620.9 | 1460.4 | 1591.6 | 2708.6 | 2688.6 | 0.974 | 0.861 | 1.017 | 0.993 | 1.021 | 0.999 | 1.016 | 0.998 | 1.024 | 1.006 | ok |
| corpus2/dev/regex_nfa | 14 | 696.7 | 552.7 | 1331.5 | 1154.1 | 554.5 | 2510.1 | 2439.3 | 1.256 | 0.997 | 0.951 | 0.974 | 0.981 | 0.975 | 0.953 | 0.985 | 0.954 | 0.994 | ok |
| corpus2/dev/template_gsub | 5 | 1694.6 | 1411.4 | 1733.3 | 1502.0 | 2051.0 | 2892.7 | 2885.0 | 0.826 | 0.688 | 0.993 | 0.982 | 1.004 | 1.002 | 0.987 | 0.981 | 1.011 | 0.996 | ok |
| corpus2/dev/text_adventure | 5 | 2582.9 | 2130.0 | 2647.9 | 2223.7 | 1186.6 | 3375.1 | 3513.0 | 2.177 | 1.795 | 1.010 | 0.997 | 0.994 | 1.008 | 0.991 | 0.989 | 1.004 | 1.003 | ok |
| corpus2/dev/varargs_util | 5 | 1532.2 | 1225.3 | 1546.2 | 1268.2 | 1413.9 | 2284.8 | 2301.9 | 1.084 | 0.867 | 1.020 | 0.995 | 0.987 | 0.999 | 1.008 | 0.991 | 1.005 | 1.000 | ok |
| corpus2/dev/wordfreq_wrap | 14 | 597.7 | 490.5 | 643.6 | 535.3 | 658.1 | 1219.2 | 1225.4 | 0.908 | 0.745 | 0.996 | 1.000 | 0.999 | 1.005 | 1.000 | 0.999 | 0.982 | 1.006 | ok |
| corpus2/heldout/bigint_rsa | 14 | 1112.1 | 928.3 | 1358.2 | 1168.4 | 1074.5 | 3033.1 | 3149.9 | 1.035 | 0.864 | 0.990 | 1.000 | 0.987 | 1.000 | 0.992 | 0.993 | 0.980 | 0.994 | ok |
| corpus2/heldout/dijkstra_routes | 14 | 1124.1 | 1087.3 | 1430.0 | 1384.0 | 1265.1 | 3142.9 | 3226.1 | 0.889 | 0.859 | 0.958 | 0.936 | 0.955 | 0.937 | 0.986 | 0.961 | 0.988 | 0.963 | ok |
| corpus2/heldout/forth_vm | 5 | 786.5 | 781.9 | 992.6 | 1094.1 | 696.1 | 2965.2 | 3269.3 | 1.130 | 1.123 | 0.981 | 0.971 | 0.978 | 0.971 | 0.944 | 0.940 | 0.938 | 0.939 | ok |
| corpus2/heldout/log_analyzer | 5 | 2125.4 | 1835.1 | 2149.7 | 1849.5 | 1652.2 | 3115.0 | 3178.7 | 1.286 | 1.111 | 1.011 | 0.978 | 1.050 | 0.976 | 0.996 | 0.983 | 0.988 | 0.983 | ok |
| corpus2/heldout/markov_text | 5 | 1096.4 | 1068.8 | 1141.2 | 1104.4 | 982.0 | 2862.9 | 2851.2 | 1.117 | 1.088 | 0.971 | 0.985 | 0.973 | 0.967 | 0.959 | 0.983 | 0.965 | 0.967 | ok |
| corpus2/heldout/prototype_objects | 14 | 1337.9 | 1275.6 | 1311.9 | 1275.9 | 1280.6 | 2532.8 | 2721.4 | 1.045 | 0.996 | 0.967 | 0.998 | 0.985 | 1.000 | 0.955 | 0.983 | 0.967 | 0.986 | ok |
| corpus2/heldout/real_dis_mips | 14 | 2099.0 | 1801.7 | 2110.6 | 1872.9 | 1786.6 | 3808.7 | 3869.0 | 1.175 | 1.008 | 0.958 | 0.952 | 0.951 | 0.966 | 0.968 | 0.968 | 1.002 | 0.985 | ok |
| corpus2/heldout/real_dis_ppc | 14 | 2275.0 | 1944.1 | 2231.5 | 1969.7 | 1747.6 | 3961.9 | 3962.8 | 1.302 | 1.112 | 1.092 | 1.043 | 1.046 | 1.058 | 1.007 | 1.000 | 1.018 | 1.015 | ok |
| corpus2/heldout/roguelike_turns | 14 | 798.3 | 761.5 | 830.2 | 799.8 | 623.3 | 3311.7 | 3473.6 | 1.281 | 1.222 | 0.992 | 0.966 | 0.992 | 0.968 | 0.975 | 0.969 | 0.967 | 0.972 | ok |
| corpus2/heldout/scheme_interp | 14 | 1404.9 | 1174.3 | 1478.0 | 1256.8 | 1045.1 | 3052.1 | 3021.0 | 1.344 | 1.124 | 0.973 | 0.984 | 0.971 | 0.989 | 0.978 | 0.991 | 0.963 | 0.997 | ok |
| corpus2/heldout/stream_closures | 5 | 767.2 | 690.3 | 794.6 | 743.1 | 416.0 | 3032.0 | 3163.1 | 1.844 | 1.660 | 0.976 | 0.960 | 0.957 | 0.965 | 0.935 | 1.120 | 0.953 | 1.139 | ok |
| corpus2/heldout/sudoku_solver | 5 | 1276.1 | 1142.3 | 1592.2 | 1461.1 | 1387.6 | 3013.0 | 3102.7 | 0.920 | 0.823 | 0.978 | 0.973 | 0.973 | 0.972 | 0.981 | 0.976 | 0.990 | 0.980 | ok |
| corpus2/heldout/vending_fsm | 14 | 961.5 | 879.0 | 1012.3 | 926.0 | 824.2 | 3290.3 | 3377.2 | 1.167 | 1.066 | 0.944 | 0.953 | 0.932 | 0.951 | 0.926 | 0.882 | 0.916 | 0.877 | ok |
| jit-trace/alloc_ctor | 14 | 193.9 | 155.0 | 244.6 | 197.7 | 276.5 | 1652.1 | 2197.8 | 0.701 | 0.561 | 1.037 | 0.990 | 0.983 | 0.989 | 0.949 | 0.990 | 0.981 | 0.988 | ok |
| jit-trace/array_rec | 5 | 1793.5 | 1792.8 | 1820.2 | 1820.0 | 1755.9 | 9671.5 | 10022.8 | 1.021 | 1.021 | 0.999 | 0.769 | 0.998 | 0.769 | 1.000 | 0.773 | 1.000 | 0.773 | ok |
| jit-trace/dot | 14 | 1631.6 | 1629.8 | 1637.4 | 1635.4 | 1166.0 | 11124.9 | 11807.8 | 1.399 | 1.398 | 1.340 | 0.699 | 1.336 | 0.699 | 1.340 | 0.701 | 1.336 | 0.701 | ok |
| jit-trace/field_global | 5 | 531.4 | 528.1 | 536.0 | 529.9 | 491.7 | 17948.4 | 17879.3 | 1.081 | 1.074 | 0.977 | 0.901 | 0.986 | 0.902 | 0.993 | 0.901 | 0.998 | 0.900 | ok |
| jit-trace/field_method | 14 | 500.1 | 497.4 | 501.4 | 498.5 | 423.1 | 20185.1 | 20271.4 | 1.182 | 1.176 | 0.881 | 0.871 | 0.884 | 0.873 | 0.878 | 0.871 | 0.882 | 0.872 | ok |
| jit-trace/field_rw | 5 | 948.8 | 951.9 | 1032.2 | 1032.1 | 929.3 | 19574.6 | 19882.6 | 1.021 | 1.024 | 1.000 | 0.931 | 1.004 | 0.931 | 0.998 | 0.939 | 0.999 | 0.939 | ok |
| jit-trace/float_loop | 2 | 2771.3 | 2773.0 | 2793.8 | 2798.1 | 2575.9 | 11381.5 | 13567.0 | 1.076 | 1.077 | 0.997 | 0.999 | 0.998 | 1.000 | 0.998 | 1.000 | 1.002 | 1.000 | ok |
| jit-trace/float_poly | 5 | 1782.7 | 1783.2 | 1817.0 | 1817.8 | 1768.0 | 7918.6 | 8940.5 | 1.008 | 1.009 | 1.000 | 0.880 | 1.001 | 0.882 | 0.999 | 0.888 | 1.000 | 0.887 | ok |
| jit-trace/inline_2 | 5 | 327.0 | 324.7 | 326.2 | 326.8 | 262.5 | 11057.3 | 10600.9 | 1.246 | 1.237 | 1.002 | 0.906 | 0.993 | 0.906 | 1.001 | 0.906 | 1.003 | 0.906 | ok |
| jit-trace/inline_leaf | 14 | 405.4 | 411.8 | 406.5 | 407.1 | 630.2 | 11902.9 | 12019.3 | 0.643 | 0.653 | 0.894 | 0.876 | 0.908 | 0.878 | 0.895 | 0.876 | 0.896 | 0.876 | ok |
| jit-trace/int_rec2 | 14 | 642.2 | 639.5 | 644.2 | 643.1 | 5016.4 | 12493.6 | 13006.3 | 0.128 | 0.127 | 0.948 | 0.901 | 0.946 | 0.901 | 0.941 | 0.902 | 0.940 | 0.902 | ok |
| jit-trace/int_sum | 5 | 870.6 | 871.7 | 894.7 | 894.7 | 2512.8 | 9858.5 | 12300.9 | 0.346 | 0.347 | 0.980 | 0.870 | 0.982 | 0.870 | 0.982 | 0.874 | 0.985 | 0.874 | ok |
| jit-trace/licm_expr | 5 | 463.7 | 463.0 | 465.9 | 465.9 | 442.6 | 12323.9 | 12827.3 | 1.048 | 1.046 | 0.995 | 0.729 | 0.993 | 0.729 | 1.000 | 0.804 | 0.998 | 0.804 | ok |
| jit-trace/multret | 5 | 148.7 | 148.2 | 148.8 | 148.2 | 156.7 | 5879.7 | 5782.8 | 0.949 | 0.946 | 0.987 | 0.856 | 0.990 | 0.856 | 0.991 | 0.856 | 0.990 | 0.856 | ok |
| jit-trace/oop_inherit | 14 | 95.7 | 95.4 | 48.5 | 48.4 | 112.8 | 2153.1 | 2201.0 | 0.848 | 0.846 | 1.666 | 0.913 | 1.672 | 0.914 | 0.991 | 0.910 | 0.997 | 0.911 | ok |
| jit-trace/oop_override | 14 | 103.3 | 102.7 | 106.0 | 105.5 | 136.7 | 645.6 | 666.3 | 0.756 | 0.752 | 0.964 | 0.969 | 0.962 | 0.969 | 0.956 | 0.970 | 0.956 | 0.971 | ok |
| jit-trace/rec_loop | 14 | 76.8 | 75.8 | 79.3 | 78.5 | 141.9 | 1440.3 | 1449.9 | 0.541 | 0.534 | 0.834 | 0.801 | 0.831 | 0.802 | 0.893 | 0.805 | 0.892 | 0.806 | ok |
| jit-trace/short_inner | 5 | 1087.2 | 1087.8 | 993.4 | 991.7 | 878.8 | 10393.4 | 11908.6 | 1.237 | 1.238 | 0.987 | 0.984 | 0.988 | 0.984 | 1.000 | 0.998 | 0.999 | 0.999 | ok |
| jit-trace/sink_tuple | 5 | 13.3 | 12.9 | 13.5 | 12.9 | 20.0 | 2743.7 | 3358.1 | 0.665 | 0.646 | 1.002 | 0.950 | 1.006 | 0.952 | 1.030 | 0.949 | 1.003 | 0.952 | ok |
| jit-trace/sink_vec | 5 | 15.4 | 15.0 | 15.4 | 15.1 | 20.0 | 1489.0 | 1832.3 | 0.772 | 0.750 | 1.003 | 0.908 | 1.003 | 0.910 | 1.013 | 0.909 | 1.018 | 0.910 | ok |
| jit-trace/str_concat | 5 | 524.9 | 512.3 | 599.4 | 550.8 | 501.6 | 1628.5 | 1630.2 | 1.046 | 1.021 | 0.988 | 0.997 | 1.006 | 1.012 | 1.009 | 0.997 | 0.993 | 1.011 | ok |
| jit-trace/tail_loop | 5 | 235.5 | 235.0 | 257.0 | 255.2 | 223.3 | 6724.0 | 6643.8 | 1.055 | 1.052 | 0.947 | 0.919 | 0.946 | 0.919 | 1.025 | 0.920 | 1.013 | 0.920 | ok |
| jit-trace/tfor_loops | 14 | 222.3 | 222.0 | 226.6 | 227.3 | 362.3 | 3623.0 | 3352.6 | 0.614 | 0.613 | 1.023 | 0.988 | 1.007 | 0.988 | 0.992 | 0.986 | 1.015 | 0.987 | ok |
| jit-trace/type_entry | 5 | 1442.3 | 1442.1 | 1561.7 | 1548.1 | 848.7 | 11725.8 | 12234.3 | 1.699 | 1.699 | 0.952 | 0.779 | 0.953 | 0.779 | 0.981 | 0.847 | 0.981 | 0.847 | ok |
| jit-trace/type_entry_stable | 5 | 1376.0 | 1374.7 | 1500.2 | 1489.4 | 856.9 | 11690.0 | 12248.4 | 1.606 | 1.604 | 0.949 | 0.768 | 0.948 | 0.768 | 0.994 | 0.838 | 0.978 | 0.838 | ok |
| jit-trace/type_late | 5 | 1519.7 | 1522.8 | 1534.9 | 1533.8 | 1461.5 | 10901.2 | 11418.6 | 1.040 | 1.042 | 0.999 | 0.802 | 1.000 | 0.803 | 1.003 | 0.804 | 1.002 | 0.804 | ok |
| jit-trace/type_late_stable | 5 | 1507.3 | 1506.8 | 1532.9 | 1533.4 | 1461.0 | 10905.6 | 11410.8 | 1.032 | 1.031 | 0.991 | 0.726 | 0.992 | 0.726 | 1.000 | 0.804 | 1.002 | 0.804 | ok |
| jit-trace/upval_acc | 14 | 1010.4 | 996.5 | 1172.6 | 1167.7 | 1356.5 | 9970.0 | 10500.8 | 0.745 | 0.735 | 0.731 | 0.812 | 0.721 | 0.812 | 0.847 | 0.803 | 0.843 | 0.803 | ok |
| jit-trace/upval_inv | 5 | 2116.1 | 2116.6 | 2133.2 | 2133.2 | 1985.3 | 13886.5 | 14901.3 | 1.066 | 1.066 | 0.992 | 0.710 | 0.992 | 0.710 | 0.996 | 0.845 | 0.997 | 0.845 | ok |
| lib/assert_ok | 5 | 16.9 | 17.2 | 19.4 | 19.0 | 23.7 | 295.9 | 329.8 | 0.714 | 0.724 | 0.893 | 0.777 | 0.919 | 0.778 | 0.955 | 0.850 | 0.930 | 0.850 | ok |
| lib/byte | 14 | 43.6 | 43.3 | 43.7 | 43.3 | 33.0 | 776.0 | 845.1 | 1.320 | 1.310 | 0.866 | 0.877 | 0.868 | 0.878 | 0.858 | 0.877 | 0.860 | 0.878 | ok |
| lib/byte_multi | 14 | 38.0 | 37.9 | 38.3 | 38.0 | 20.1 | 336.9 | 366.2 | 1.889 | 1.888 | 0.926 | 0.886 | 0.920 | 0.887 | 0.932 | 0.886 | 0.932 | 0.887 | ok |
| lib/char | 5 | 149.6 | 135.2 | 157.6 | 145.6 | 104.8 | 383.6 | 387.1 | 1.427 | 1.290 | 0.990 | 0.986 | 0.977 | 0.982 | 1.014 | 0.996 | 1.014 | 0.993 | ok |
| lib/co_wrap | 14 | 233.9 | 192.1 | 217.1 | 188.2 | 228.7 | 381.5 | 338.9 | 1.023 | 0.840 | 0.969 | 0.955 | 0.966 | 0.991 | 0.903 | 0.955 | 0.964 | 0.996 | ok |
| lib/co_yield | 14 | 174.6 | 171.4 | 176.0 | 170.3 | 102.0 | 483.9 | 475.0 | 1.712 | 1.681 | 0.732 | 0.919 | 0.951 | 1.004 | 0.729 | 0.919 | 0.939 | 0.994 | ok |
| lib/concat_op | 5 | 138.5 | 128.9 | 144.0 | 134.6 | 212.9 | 419.4 | 445.3 | 0.651 | 0.606 | 0.989 | 0.994 | 0.998 | 1.009 | 1.012 | 0.993 | 1.008 | 1.007 | ok |
| lib/concat_op_str | 5 | 141.1 | 125.0 | 236.1 | 223.2 | 276.8 | 436.1 | 336.3 | 0.510 | 0.452 | 1.047 | 0.995 | 1.005 | 0.997 | 0.968 | 0.980 | 0.969 | 0.979 | ok |
| lib/error_tbl | 14 | 273.5 | 212.8 | 260.6 | 216.4 | 3042.9 | 619.9 | 625.1 | 0.090 | 0.070 | 1.028 | 1.009 | 0.966 | 0.991 | 0.985 | 1.011 | 1.000 | 0.986 | ok |
| lib/find_cap | 5 | 267.3 | 224.3 | 266.8 | 227.6 | 183.4 | 365.9 | 363.4 | 1.457 | 1.223 | 0.970 | 1.005 | 0.961 | 0.983 | 0.964 | 1.000 | 1.002 | 0.985 | ok |
| lib/find_pat | 14 | 217.2 | 195.8 | 218.1 | 195.6 | 705.2 | 1094.0 | 1092.2 | 0.308 | 0.278 | 0.844 | 0.822 | 0.857 | 0.796 | 0.806 | 0.819 | 0.829 | 0.795 | ok |
| lib/find_plain | 14 | 112.4 | 117.1 | 110.6 | 116.9 | 67.6 | 342.8 | 363.4 | 1.664 | 1.733 | 0.881 | 0.854 | 1.004 | 0.825 | 0.876 | 0.854 | 1.020 | 0.834 | ok |
| lib/fmt_d | 5 | 94.3 | 88.9 | 101.3 | 94.9 | 126.3 | 278.1 | 235.1 | 0.747 | 0.704 | 1.013 | 0.994 | 0.990 | 0.988 | 1.025 | 0.992 | 0.997 | 0.989 | ok |
| lib/fmt_f | 5 | 62.1 | 57.4 | 63.2 | 59.0 | 86.8 | 215.0 | 196.2 | 0.715 | 0.661 | 0.990 | 1.004 | 0.998 | 1.009 | 0.970 | 0.982 | 0.990 | 1.008 | ok |
| lib/fmt_g | 14 | 72.2 | 61.3 | 73.7 | 64.1 | 67.3 | 204.6 | 179.3 | 1.072 | 0.910 | 0.987 | 1.005 | 0.972 | 1.008 | 0.978 | 0.984 | 1.000 | 1.007 | ok |
| lib/fmt_mixed | 14 | 60.0 | 49.3 | 66.1 | 57.0 | 106.2 | 254.5 | 238.3 | 0.565 | 0.464 | 1.009 | 1.002 | 0.982 | 1.005 | 0.984 | 0.985 | 1.008 | 1.006 | ok |
| lib/fmt_q | 14 | 140.6 | 118.4 | 142.5 | 121.1 | 103.3 | 238.6 | 226.1 | 1.361 | 1.147 | 0.980 | 1.001 | 0.977 | 1.002 | 0.972 | 0.986 | 0.996 | 1.003 | ok |
| lib/fmt_s | 5 | 6.8 | 6.4 | 6.9 | 6.3 | 3.2 | 217.6 | 222.2 | 2.140 | 2.017 | 0.926 | 0.830 | 0.930 | 0.833 | 0.923 | 0.826 | 0.909 | 0.833 | ok |
| lib/fmt_x_pad | 14 | 56.7 | 49.0 | 61.6 | 50.5 | 54.0 | 177.1 | 177.5 | 1.050 | 0.908 | 1.007 | 0.996 | 1.026 | 0.991 | 1.045 | 0.996 | 0.993 | 0.994 | ok |
| lib/getmt | 5 | 14.6 | 14.4 | 16.0 | 15.6 | 8.8 | 255.7 | 264.8 | 1.655 | 1.626 | 0.910 | 0.945 | 0.918 | 0.947 | 0.809 | 0.828 | 0.800 | 0.829 | ok |
| lib/gmatch_words | 14 | 178.4 | 140.6 | 179.4 | 137.8 | 173.3 | 315.9 | 319.8 | 1.030 | 0.811 | 1.015 | 1.014 | 1.035 | 1.000 | 1.016 | 1.015 | 1.006 | 1.003 | ok |
| lib/gsub_fn | 14 | 170.7 | 125.3 | 171.6 | 124.9 | 200.2 | 323.4 | 312.5 | 0.852 | 0.626 | 0.977 | 1.000 | 0.974 | 0.960 | 0.959 | 0.993 | 0.947 | 0.960 | ok |
| lib/gsub_str | 5 | 162.0 | 125.9 | 160.3 | 127.4 | 398.5 | 491.6 | 515.6 | 0.406 | 0.316 | 0.981 | 1.005 | 0.996 | 1.002 | 0.963 | 0.990 | 1.010 | 0.998 | ok |
| lib/gsub_tbl | 14 | 160.5 | 103.9 | 160.1 | 106.2 | 187.9 | 321.9 | 321.7 | 0.854 | 0.553 | 0.983 | 1.001 | 0.960 | 0.972 | 1.052 | 0.987 | 0.971 | 0.974 | ok |
| lib/it_ipairs | 14 | 233.0 | 228.9 | 250.9 | 240.3 | 251.6 | 5628.0 | 4827.6 | 0.926 | 0.910 | 0.978 | 0.949 | 0.979 | 0.948 | 0.899 | 0.915 | 0.924 | 0.929 | ok |
| lib/it_next | 11 | 49.2 | 48.7 | 49.2 | 49.2 | 180.5 | 371.1 | 401.4 | 0.272 | 0.270 | 1.112 | 0.997 | 1.108 | 0.997 | 1.116 | 0.997 | 1.126 | 0.997 | ok |
| lib/it_next_empty | 14 | 51.8 | 51.7 | 52.2 | 51.4 | 72.5 | 393.0 | 455.4 | 0.714 | 0.713 | 1.000 | 0.966 | 1.009 | 0.966 | 1.008 | 0.983 | 0.999 | 0.983 | ok |
| lib/it_pairs | 14 | 86.4 | 86.2 | 90.5 | 87.9 | 127.9 | 687.0 | 646.3 | 0.676 | 0.674 | 0.989 | 0.997 | 0.987 | 0.997 | 0.974 | 0.967 | 0.983 | 0.975 | ok |
| lib/it_pairs_hash | 14 | 101.7 | 101.0 | 107.9 | 104.2 | 118.1 | 989.3 | 957.1 | 0.861 | 0.855 | 0.983 | 0.998 | 0.987 | 0.998 | 0.957 | 0.976 | 0.969 | 0.982 | ok |
| lib/len_op | 14 | 47.4 | 47.2 | 50.2 | 49.8 | 100.7 | 1384.7 | 1419.8 | 0.471 | 0.468 | 0.990 | 0.919 | 0.995 | 0.919 | 0.876 | 0.999 | 0.872 | 0.999 | ok |
| lib/len_tbl | 14 | 142.0 | 141.1 | 149.0 | 148.7 | 159.7 | 372.6 | 337.9 | 0.889 | 0.883 | 1.035 | 0.987 | 1.043 | 0.987 | 0.947 | 1.000 | 0.947 | 1.000 | ok |
| lib/load_small | 14 | 236.2 | 167.9 | 222.5 | 162.6 | 215.6 | 378.8 | 345.2 | 1.096 | 0.779 | 0.988 | 1.001 | 1.103 | 1.000 | 0.995 | 0.988 | 1.026 | 0.978 | ok |
| lib/lower | 14 | 83.9 | 74.8 | 84.1 | 76.5 | 110.1 | 289.0 | 294.8 | 0.762 | 0.680 | 0.950 | 0.989 | 0.926 | 0.973 | 0.928 | 0.979 | 0.929 | 0.963 | ok |
| lib/m_abs | 11 | 247.0 | 246.9 | 245.9 | 245.2 | 390.3 | 6061.9 | 6506.8 | 0.633 | 0.633 | 0.997 | 1.000 | 1.010 | 1.000 | 1.002 | 1.000 | 1.008 | 1.000 | ok |
| lib/m_floor | 2 | 225.9 | 225.8 | 227.7 | 227.2 | 206.5 | 3681.1 | 4104.1 | 1.094 | 1.093 | 1.000 | 1.000 | 1.001 | 1.000 | 1.004 | 1.000 | 1.004 | 1.000 | ok |
| lib/m_fmod | 5 | 218.3 | 221.1 | 221.5 | 228.6 | 235.1 | 329.2 | 361.2 | 0.929 | 0.940 | 0.997 | 1.023 | 1.015 | 0.996 | 1.001 | 1.023 | 1.013 | 0.996 | ok |
| lib/m_max | 2 | 257.5 | 255.9 | 256.6 | 256.5 | 401.3 | 8004.8 | 10137.5 | 0.642 | 0.638 | 1.007 | 1.000 | 1.002 | 1.000 | 0.999 | 1.000 | 1.001 | 1.000 | ok |
| lib/m_min | 14 | 205.4 | 202.5 | 201.5 | 200.2 | 387.7 | 6530.7 | 8066.5 | 0.530 | 0.522 | 0.938 | 0.861 | 0.898 | 0.861 | 0.908 | 0.861 | 0.913 | 0.861 | ok |
| lib/m_pow_op | 5 | 283.1 | 282.8 | 283.7 | 283.1 | 275.2 | 1923.5 | 1956.0 | 1.029 | 1.028 | 1.009 | 0.870 | 1.011 | 0.870 | 1.011 | 0.870 | 1.012 | 0.870 | ok |
| lib/m_random | 2 | 78.7 | 78.5 | 79.2 | 78.2 | 73.2 | 345.7 | 375.3 | 1.074 | 1.071 | 0.995 | 0.999 | 1.001 | 1.000 | 1.006 | 0.999 | 0.999 | 1.000 | ok |
| lib/m_random_f | 5 | 53.0 | 52.5 | 53.0 | 52.6 | 89.9 | 272.7 | 312.0 | 0.589 | 0.583 | 1.004 | 0.974 | 1.005 | 0.974 | 1.005 | 0.974 | 1.001 | 0.974 | ok |
| lib/m_sin | 5 | 267.6 | 256.3 | 267.5 | 256.2 | 210.5 | 563.9 | 589.3 | 1.271 | 1.218 | 0.985 | 0.975 | 0.985 | 0.974 | 0.986 | 0.975 | 0.984 | 0.974 | ok |
| lib/m_sqrt | 5 | 278.6 | 278.4 | 276.2 | 273.5 | 270.0 | 2022.6 | 2132.3 | 1.032 | 1.031 | 1.003 | 0.871 | 1.009 | 0.871 | 0.954 | 0.871 | 0.957 | 0.871 | ok |
| lib/m_tointeger | 5 | 224.8 | 229.9 | 236.8 | 236.5 | - | 3718.4 | 3972.3 | - | - | 1.008 | 1.000 | 1.032 | 1.000 | 1.060 | 1.000 | 1.001 | 1.000 | ok |
| lib/m_type | 5 | 32.1 | 31.8 | 32.1 | 31.5 | - | 419.3 | 429.1 | - | - | 1.004 | 0.998 | 1.005 | 0.998 | 1.002 | 0.997 | 0.996 | 0.998 | ok |
| lib/m_ult | 14 | 271.1 | 280.5 | 275.0 | 284.1 | - | 415.2 | 435.8 | - | - | 0.988 | 1.021 | 0.995 | 0.997 | 0.997 | 1.021 | 0.999 | 0.997 | ok |
| lib/match | 14 | 47.6 | 43.1 | 48.1 | 43.7 | 193.6 | 319.9 | 303.8 | 0.246 | 0.223 | 0.775 | 0.805 | 0.848 | 0.834 | 0.791 | 0.799 | 0.848 | 0.826 | ok |
| lib/os_clock | 5 | 118.9 | 120.5 | 121.3 | 120.5 | 100.7 | 119.3 | 118.5 | 1.181 | 1.197 | 1.035 | 1.021 | 0.996 | 0.997 | 1.023 | 1.020 | 0.995 | 0.997 | ok |
| lib/pcall_err | 14 | 119.2 | 96.1 | 123.3 | 104.8 | 764.5 | 308.4 | 309.0 | 0.156 | 0.126 | 0.981 | 1.005 | 0.964 | 0.993 | 0.936 | 0.997 | 0.949 | 0.991 | ok |
| lib/pcall_ok | 14 | 27.5 | 27.0 | 27.5 | 27.3 | 41.0 | 1524.0 | 1518.3 | 0.672 | 0.660 | 0.913 | 0.874 | 0.897 | 0.874 | 0.907 | 0.874 | 0.912 | 0.874 | ok |
| lib/rawget_set | 5 | 16.1 | 15.9 | 16.1 | 15.9 | 14.2 | 381.1 | 417.2 | 1.130 | 1.114 | 0.845 | 0.808 | 0.838 | 0.809 | 0.854 | 0.808 | 0.853 | 0.809 | ok |
| lib/rep | 5 | 106.1 | 96.4 | 115.9 | 106.4 | 89.6 | 347.4 | 333.3 | 1.184 | 1.076 | 0.984 | 0.984 | 0.965 | 0.975 | 0.956 | 0.987 | 0.964 | 0.981 | ok |
| lib/rep_sep | 5 | 87.0 | 78.9 | 116.3 | 107.5 | 103.8 | 329.6 | 311.2 | 0.839 | 0.760 | 0.986 | 0.989 | 0.975 | 0.984 | 0.993 | 0.991 | 0.995 | 0.987 | ok |
| lib/reverse | 14 | 94.3 | 87.7 | 94.3 | 88.7 | 115.3 | 309.8 | 314.7 | 0.818 | 0.760 | 0.947 | 0.989 | 0.939 | 0.976 | 0.913 | 0.979 | 0.906 | 0.966 | ok |
| lib/sel_count | 14 | 215.7 | 215.9 | 224.3 | 219.9 | 388.2 | 15509.5 | 14438.4 | 0.556 | 0.556 | 0.996 | 1.000 | 0.999 | 1.000 | 1.034 | 0.999 | 1.005 | 1.000 | ok |
| lib/sel_loop | 14 | 208.3 | 208.0 | 211.2 | 211.1 | 5263.6 | 53689.2 | 54662.9 | 0.040 | 0.040 | 0.988 | 0.929 | 0.993 | 0.929 | 0.989 | 0.930 | 0.996 | 0.930 | ok |
| lib/sel_n | 14 | 339.4 | 334.6 | 329.0 | 332.7 | 675.4 | 25345.4 | 23674.6 | 0.502 | 0.495 | 1.010 | 1.000 | 1.038 | 1.000 | 1.006 | 1.000 | 1.030 | 1.000 | ok |
| lib/setmt | 14 | 189.5 | 159.3 | 216.4 | 190.5 | 331.9 | 743.5 | 835.9 | 0.571 | 0.480 | 1.031 | 0.993 | 1.001 | 0.993 | 0.949 | 0.988 | 0.995 | 0.987 | ok |
| lib/str_eq | 14 | 135.3 | 134.6 | 135.3 | 135.0 | 182.5 | 1479.5 | 1529.4 | 0.741 | 0.737 | 0.846 | 0.921 | 0.842 | 0.921 | 0.843 | 0.921 | 0.845 | 0.921 | ok |
| lib/str_eq_dyn | 5 | 131.7 | 131.1 | 132.1 | 131.8 | 150.4 | 1277.3 | 1348.8 | 0.876 | 0.872 | 0.975 | 0.859 | 0.987 | 0.859 | 0.988 | 0.859 | 0.992 | 0.859 | ok |
| lib/str_eq_long | 14 | 33.9 | 34.0 | 34.0 | 33.9 | 21.5 | 215.5 | 222.1 | 1.577 | 1.581 | 0.949 | 0.960 | 0.969 | 0.961 | 0.953 | 0.960 | 0.955 | 0.961 | ok |
| lib/str_idx_concat_key | 5 | 203.4 | 191.1 | 203.2 | 191.8 | 166.6 | 464.0 | 461.2 | 1.221 | 1.147 | 0.991 | 0.996 | 1.003 | 1.008 | 0.987 | 0.996 | 0.995 | 1.007 | ok |
| lib/str_lt | 14 | 86.1 | 84.0 | 86.3 | 84.3 | 66.3 | 227.1 | 228.3 | 1.297 | 1.266 | 0.922 | 0.968 | 0.922 | 0.968 | 0.970 | 0.968 | 0.975 | 0.968 | ok |
| lib/str_lt_short | 14 | 96.5 | 89.0 | 97.9 | 94.0 | 62.5 | 249.8 | 255.3 | 1.543 | 1.423 | 0.849 | 0.944 | 0.838 | 0.943 | 0.936 | 0.951 | 0.958 | 0.950 | ok |
| lib/str_method_len | 14 | 43.5 | 43.2 | 50.9 | 50.5 | 55.3 | 1386.6 | 1429.7 | 0.786 | 0.781 | 0.955 | 0.903 | 0.953 | 0.903 | 0.998 | 0.955 | 0.987 | 0.955 | ok |
| lib/string_key_const | 14 | 167.3 | 167.3 | 183.0 | 182.3 | 176.9 | 1151.6 | 1165.1 | 0.946 | 0.946 | 0.946 | 0.899 | 0.950 | 0.899 | 0.915 | 0.910 | 0.913 | 0.910 | ok |
| lib/string_key_dyn | 5 | 215.3 | 214.9 | 215.9 | 215.2 | 268.0 | 1082.3 | 1101.0 | 0.803 | 0.802 | 0.939 | 0.933 | 0.940 | 0.934 | 0.946 | 0.934 | 0.943 | 0.934 | ok |
| lib/sub | 14 | 117.9 | 106.3 | 117.9 | 105.7 | 80.5 | 469.5 | 482.7 | 1.465 | 1.321 | 0.897 | 0.933 | 0.833 | 0.892 | 0.900 | 0.933 | 0.849 | 0.892 | ok |
| lib/sub_1char | 14 | 112.3 | 98.6 | 111.9 | 99.3 | 98.2 | 459.2 | 478.8 | 1.143 | 1.004 | 0.934 | 0.958 | 0.902 | 0.938 | 0.921 | 0.959 | 0.915 | 0.939 | ok |
| lib/tconcat | 14 | 147.2 | 100.0 | 168.6 | 117.8 | 133.4 | 346.9 | 322.3 | 1.104 | 0.750 | 1.020 | 1.001 | 1.002 | 1.003 | 0.996 | 0.993 | 0.978 | 1.002 | ok |
| lib/tconcat_num | 14 | 172.3 | 117.7 | 175.5 | 121.2 | 246.0 | 908.0 | 844.8 | 0.700 | 0.478 | 0.963 | 1.001 | 0.985 | 1.003 | 0.947 | 0.993 | 0.992 | 1.003 | ok |
| lib/tfront_1k | 14 | 77.9 | 75.4 | 81.0 | 77.0 | 406.3 | 4375.7 | 4241.0 | 0.192 | 0.185 | 0.985 | 1.006 | 0.990 | 1.011 | 0.990 | 1.006 | 1.013 | 1.010 | ok |
| lib/tinsert_append | 14 | 152.4 | 129.1 | 196.6 | 184.1 | 141.7 | 539.7 | 574.7 | 1.075 | 0.911 | 0.956 | 0.990 | 0.971 | 0.992 | 1.024 | 1.023 | 1.017 | 1.025 | ok |
| lib/tinsert_front | 14 | 227.1 | 179.2 | 265.0 | 209.1 | 272.9 | 455.4 | 493.5 | 0.832 | 0.657 | 1.014 | 1.003 | 1.005 | 1.007 | 1.010 | 1.003 | 1.000 | 1.006 | ok |
| lib/tinsert_idx | 14 | 177.1 | 154.8 | 208.0 | 194.3 | 154.6 | 328.8 | 383.3 | 1.146 | 1.001 | 1.009 | 1.028 | 1.017 | 1.031 | 1.014 | 1.024 | 1.024 | 1.026 | ok |
| lib/tmove | 5 | 101.5 | 100.9 | 120.8 | 120.4 | 105.8 | 700.5 | 683.1 | 0.960 | 0.954 | 0.986 | 0.984 | 0.982 | 0.985 | 0.969 | 0.994 | 0.963 | 0.994 | ok |
| lib/tmove_1k | 14 | 97.3 | 70.2 | 106.7 | 71.8 | 111.6 | 476.4 | 447.3 | 0.872 | 0.629 | 0.965 | 1.001 | 1.026 | 1.004 | 0.963 | 1.003 | 0.962 | 0.993 | ok |
| lib/tonumber_bad | 14 | 156.0 | 121.9 | 141.8 | 112.5 | 168.0 | 408.1 | 431.1 | 0.928 | 0.726 | 1.000 | 0.987 | 1.009 | 0.985 | 0.851 | 0.990 | 0.783 | 0.989 | ok |
| lib/tonumber_base | 14 | 266.2 | 263.1 | 268.6 | 271.2 | 229.2 | 432.1 | 451.7 | 1.161 | 1.148 | 1.004 | 1.013 | 1.012 | 1.017 | 0.989 | 1.012 | 1.027 | 1.017 | ok |
| lib/tonumber_flt | 14 | 205.6 | 166.7 | 191.1 | 163.7 | 776.7 | 577.4 | 596.1 | 0.265 | 0.215 | 1.058 | 0.993 | 0.972 | 0.994 | 0.970 | 0.995 | 0.976 | 0.994 | ok |
| lib/tonumber_int | 14 | 128.7 | 119.0 | 158.7 | 141.2 | 96.2 | 288.1 | 305.0 | 1.338 | 1.238 | 0.830 | 0.983 | 0.846 | 0.972 | 0.989 | 0.996 | 0.993 | 0.995 | ok |
| lib/tonumber_num | 14 | 167.4 | 166.8 | 166.9 | 166.5 | 383.1 | 6476.7 | 7506.3 | 0.437 | 0.435 | 0.779 | 0.839 | 0.788 | 0.839 | 0.767 | 0.839 | 0.786 | 0.839 | ok |
| lib/tostring_flt | 14 | 82.4 | 65.7 | 84.1 | 61.2 | 82.5 | 299.3 | 214.9 | 0.999 | 0.797 | 0.996 | 0.995 | 0.998 | 0.991 | 1.007 | 0.996 | 1.000 | 0.995 | ok |
| lib/tostring_int | 5 | 119.9 | 107.8 | 138.4 | 123.2 | 279.8 | 543.3 | 450.7 | 0.429 | 0.385 | 0.983 | 0.988 | 0.989 | 0.982 | 1.027 | 0.992 | 1.006 | 0.989 | ok |
| lib/tostring_tbl | 14 | 105.6 | 104.1 | 106.9 | 105.3 | 93.3 | 313.8 | 280.9 | 1.132 | 1.115 | 0.978 | 0.997 | 0.961 | 0.995 | 0.958 | 0.975 | 0.980 | 0.970 | ok |
| lib/tpack | 14 | 146.1 | 115.7 | 206.2 | 165.0 | 306.1 | 499.0 | 485.7 | 0.477 | 0.378 | 0.982 | 0.995 | 0.983 | 0.994 | 0.977 | 0.995 | 0.984 | 0.993 | ok |
| lib/tremove_front | 2 | 149.4 | 130.6 | 186.6 | 170.8 | 191.0 | 592.3 | 593.7 | 0.782 | 0.684 | 1.013 | 0.997 | 0.985 | 0.997 | 0.999 | 0.996 | 0.996 | 0.996 | ok |
| lib/tremove_pop | 5 | 243.5 | 176.3 | 224.6 | 206.7 | 136.7 | 483.9 | 552.9 | 1.781 | 1.289 | 1.013 | 1.006 | 1.021 | 1.012 | 1.002 | 0.998 | 0.985 | 0.999 | ok |
| lib/tsort_cmp | 5 | 265.0 | 255.0 | 265.4 | 262.4 | 285.5 | 339.9 | 336.1 | 0.928 | 0.893 | 0.999 | 1.000 | 0.991 | 0.956 | 0.969 | 0.999 | 1.007 | 0.942 | ok |
| lib/tsort_int | 5 | 266.5 | 221.2 | 293.2 | 223.5 | 294.0 | 382.4 | 406.0 | 0.907 | 0.752 | 0.993 | 0.992 | 1.029 | 1.010 | 1.001 | 0.998 | 0.998 | 0.996 | ok |
| lib/tsort_str | 5 | 255.9 | 153.4 | 293.6 | 159.7 | 244.8 | 364.6 | 401.0 | 1.045 | 0.627 | 0.998 | 0.988 | 1.007 | 1.010 | 1.002 | 1.000 | 0.998 | 1.000 | ok |
| lib/tunpack | 5 | 122.1 | 105.7 | 130.5 | 110.6 | 154.8 | 487.1 | 448.5 | 0.789 | 0.683 | 0.983 | 0.996 | 1.021 | 0.996 | 1.028 | 0.996 | 0.985 | 0.997 | ok |
| lib/type_call | 14 | 82.9 | 82.5 | 88.3 | 88.2 | 85.6 | 494.3 | 468.3 | 0.968 | 0.964 | 0.955 | 0.947 | 0.952 | 0.947 | 0.941 | 0.923 | 0.943 | 0.923 | ok |
| lib/upper | 14 | 89.2 | 82.6 | 90.6 | 85.0 | 120.7 | 307.4 | 312.6 | 0.739 | 0.684 | 0.930 | 0.989 | 0.910 | 0.974 | 0.907 | 0.979 | 0.897 | 0.963 | ok |
| lib/utf8_len | 14 | 275.3 | 285.6 | 272.5 | 286.9 | - | 442.2 | 456.7 | - | - | 1.012 | 1.013 | 1.040 | 0.998 | 0.994 | 1.013 | 1.026 | 0.998 | ok |
| programs/binary_trees | 14 | 1204.4 | 979.2 | 1408.6 | 1183.3 | 2007.4 | 4389.1 | 4447.3 | 0.600 | 0.488 | 0.990 | 0.983 | 0.950 | 0.989 | 0.983 | 1.007 | 1.017 | 1.006 | ok |
| programs/closures | 14 | 464.7 | 420.6 | 531.6 | 473.4 | 949.8 | 5063.2 | 4986.1 | 0.489 | 0.443 | 1.008 | 0.988 | 0.996 | 0.983 | 1.039 | 0.985 | 0.987 | 0.981 | ok |
| programs/coroutines | 14 | 1374.3 | 1291.1 | 1398.1 | 1320.9 | 871.7 | 3733.4 | 3656.9 | 1.577 | 1.481 | 0.762 | 0.934 | 0.994 | 1.039 | 0.753 | 0.938 | 0.935 | 1.027 | ok |
| programs/fannkuch | 5 | 1021.4 | 1024.8 | 1046.6 | 1042.5 | 1075.7 | 4075.1 | 4114.8 | 0.950 | 0.953 | 0.974 | 0.942 | 0.985 | 0.942 | 0.982 | 0.952 | 0.973 | 0.953 | ok |
| programs/fasta | 5 | 1271.7 | 1141.8 | 1389.5 | 1242.3 | 1495.7 | 3776.8 | 3842.3 | 0.850 | 0.763 | 1.000 | 0.978 | 0.989 | 0.974 | 1.027 | 0.979 | 1.010 | 0.977 | ok |
| programs/fib | 14 | 621.6 | 623.1 | 621.7 | 620.5 | 469.7 | 4139.5 | 4150.5 | 1.323 | 1.327 | 0.985 | 0.984 | 0.987 | 0.984 | 0.987 | 0.984 | 0.987 | 0.984 | ok |
| programs/gc_churn | 14 | 2105.6 | 1767.4 | 3067.3 | 2620.4 | 3608.7 | 4605.5 | 5988.9 | 0.583 | 0.490 | 0.986 | 1.004 | 1.000 | 1.005 | 0.956 | 1.002 | 0.949 | 0.997 | ok |
| programs/heapsort | 5 | 1034.5 | 1040.0 | 2186.5 | 2193.2 | 1286.6 | 4063.2 | 3957.0 | 0.804 | 0.808 | 0.988 | 0.899 | 0.992 | 0.899 | 0.988 | 0.994 | 0.994 | 0.994 | ok |
| programs/json | 5 | 1829.5 | 1355.0 | 1883.3 | 1481.0 | 1470.3 | 3127.6 | 3229.9 | 1.244 | 0.922 | 1.008 | 0.944 | 0.944 | 0.946 | 0.959 | 0.924 | 0.969 | 0.928 | ok |
| programs/knucleotide | 14 | 995.3 | 1015.2 | 1019.9 | 992.9 | 691.4 | 4061.5 | 4161.0 | 1.439 | 1.468 | 0.957 | 0.976 | 0.953 | 0.951 | 0.964 | 0.977 | 0.938 | 0.952 | ok |
| programs/load_big | 2 | 750.8 | 539.6 | 618.7 | 438.8 | 491.1 | 1787.3 | 620.0 | 1.529 | 1.099 | 0.995 | 1.000 | 0.993 | 0.998 | 0.935 | 1.001 | 1.000 | 0.996 | CHECK |
| programs/loops_arith | 5 | 454.8 | 454.0 | 458.8 | 458.8 | 1108.2 | 4344.5 | 4909.6 | 0.410 | 0.410 | 0.954 | 0.943 | 0.951 | 0.943 | 0.955 | 0.943 | 0.956 | 0.943 | CHECK |
| programs/mandelbrot | 2 | 532.5 | 532.4 | 533.6 | 533.0 | 521.6 | 4727.1 | 5530.2 | 1.021 | 1.021 | 1.000 | 0.996 | 1.001 | 0.996 | 1.000 | 0.996 | 0.999 | 0.996 | ok |
| programs/matmul | 5 | 422.6 | 410.6 | 434.3 | 434.9 | 272.0 | 3171.0 | 3693.4 | 1.554 | 1.510 | 0.959 | 0.880 | 0.944 | 0.880 | 0.977 | 0.893 | 0.980 | 0.893 | ok |
| programs/metamethods | 14 | 703.3 | 608.5 | 791.9 | 731.8 | 540.1 | 4614.3 | 4669.9 | 1.302 | 1.127 | 1.068 | 0.995 | 0.989 | 0.995 | 0.999 | 0.993 | 0.978 | 0.993 | ok |
| programs/nbody | 14 | 336.1 | 335.5 | 345.9 | 345.2 | 345.6 | 4246.2 | 4718.3 | 0.973 | 0.971 | 0.980 | 0.973 | 0.971 | 0.974 | 0.972 | 0.976 | 0.985 | 0.976 | ok |
| programs/oop_methods | 14 | 124.7 | 124.0 | 124.3 | 124.2 | 304.8 | 4567.3 | 4685.2 | 0.409 | 0.407 | 0.875 | 0.894 | 0.872 | 0.894 | 0.878 | 0.893 | 0.881 | 0.894 | ok |
| programs/pcall_error | 14 | 1457.9 | 1217.9 | 1443.6 | 1223.5 | 8346.6 | 4028.8 | 4363.2 | 0.175 | 0.146 | 0.978 | 1.002 | 0.980 | 0.990 | 0.971 | 0.997 | 0.992 | 0.985 | ok |
| programs/queens | 14 | 1807.7 | 1808.4 | 1889.8 | 1892.4 | 1907.1 | 4368.7 | 4120.8 | 0.948 | 0.948 | 0.971 | 0.922 | 0.980 | 0.922 | 0.982 | 0.939 | 0.984 | 0.939 | ok |
| programs/sieve | 5 | 552.9 | 551.0 | 584.5 | 651.8 | 761.7 | 2750.2 | 5029.1 | 0.726 | 0.723 | 0.954 | 0.892 | 0.945 | 0.892 | 0.954 | 0.898 | 0.960 | 0.912 | ok |
| programs/spectral_norm | 5 | 178.4 | 177.9 | 182.1 | 181.2 | 171.1 | 4573.7 | 4337.9 | 1.043 | 1.039 | 0.988 | 0.859 | 0.983 | 0.861 | 0.983 | 0.896 | 0.981 | 0.896 | ok |
| programs/string_ops | 5 | 1124.7 | 1045.1 | 1058.4 | 985.7 | 2238.4 | 2828.0 | 2807.1 | 0.502 | 0.467 | 1.026 | 1.003 | 1.022 | 1.000 | 1.003 | 1.000 | 1.016 | 1.000 | ok |
| programs/table_array | 5 | 357.1 | 302.0 | 394.3 | 317.3 | 600.0 | 3400.3 | 3492.4 | 0.595 | 0.503 | 0.996 | 0.993 | 1.005 | 0.989 | 0.982 | 0.997 | 1.012 | 0.975 | ok |
| programs/table_hash | 14 | 485.7 | 475.4 | 500.5 | 480.4 | 394.4 | 1501.6 | 1831.0 | 1.232 | 1.205 | 0.997 | 0.993 | 0.998 | 0.998 | 1.024 | 0.992 | 0.994 | 0.997 | ok |
| programs/varargs | 14 | 1173.7 | 1058.1 | 1189.3 | 1084.7 | 2194.8 | 5366.5 | 5084.7 | 0.535 | 0.482 | 0.954 | 0.982 | 0.959 | 0.987 | 0.991 | 0.981 | 0.988 | 0.991 | ok |
| real/game_loop | 14 | 33.3 | 32.6 | 36.0 | 34.4 | 21.7 | 207.6 | 221.0 | 1.535 | 1.500 | 0.965 | 0.945 | 0.971 | 0.953 | 0.920 | 0.961 | 0.905 | 0.968 | ok |
| real/json_roundtrip | 5 | 562.2 | 456.5 | 584.1 | 477.5 | 469.1 | 796.8 | 802.8 | 1.198 | 0.973 | 1.029 | 0.993 | 0.999 | 0.992 | 1.006 | 0.988 | 1.013 | 0.987 | ok |
| real/lua_interp | 5 | 100.6 | 97.0 | 102.9 | 99.7 | 59.3 | 223.6 | 225.0 | 1.697 | 1.635 | 0.954 | 0.974 | 0.958 | 0.978 | 0.983 | 0.976 | 0.985 | 0.980 | ok |
| real/template_engine | 14 | 764.8 | 608.0 | 773.1 | 611.8 | 955.0 | 1225.1 | 1249.2 | 0.801 | 0.637 | 1.070 | 1.006 | 1.031 | 1.017 | 1.023 | 1.004 | 0.976 | 1.014 | ok |
| real/wordfreq | 5 | 112.3 | 95.6 | 111.3 | 95.6 | 116.4 | 212.8 | 210.2 | 0.965 | 0.821 | 0.992 | 1.004 | 0.993 | 0.986 | 0.975 | 1.002 | 0.987 | 0.984 | ok |

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
