# Tables, session-h (subject 8c39465)

Ratios below 1: first column faster/fewer. Ours = min over all readings (2 quick, 5 full, 15-run serial reruns). Cached columns as stored.


# Aggregates, readings A: the fast pass only (min of 2 or 5 runs per program)

## Headline geomeans (cycles, ours/ref)

| build | /LuaJIT cyc (n) | /LuaJIT ins | /PUC same version cyc (223) | /PUC ins |
|---|---|---|---|---|
| 5.5 plain | 0.963 (219) | 1.369 | 0.220 | 0.224 |
| 5.5 PGO | 0.878 (219) | 1.261 | 0.201 | 0.206 |
| 5.4 plain | 1.027 (219) | 1.460 | 0.229 | 0.238 |
| 5.4 PGO | 0.936 (219) | 1.346 | 0.209 | 0.220 |

- 5.5 plain / LuaJIT -joff (219): 0.325
- 5.5 PGO / LuaJIT -joff (219): 0.297
- cached prev subject 1a5aad0 (same programs): 5.5 plain/LJ 0.964, PGO/LJ 0.879, 5.4 plain/LJ 1.020, 5.4 PGO/LJ 0.934
- cached 108b935 (same programs): plain/LJ 0.969, PGO/LJ 0.883
- our 5.4 build / our 5.5 build cycles: plain 1.066, PGO 1.066; PGO/plain: 5.5 0.913, 5.4 0.913

## Subject vs previous subject 1a5aad0 and vs 108b935 (geomean, 223 programs)

| build | vs | cycles | instructions | cyc >3% better / within / >3% worse | ins >3% fewer / more |
|---|---|---|---|---|---|
| 5.5 plain | 1a5aad0 | 0.999 | 0.983 | 27 / 165 / 31 | 28 / 2 |
| 5.5 plain | 108b935 | 0.993 | 0.962 | 43 / 148 / 32 | 79 / 1 |
| 5.5 PGO | 1a5aad0 | 0.999 | 0.980 | 23 / 178 / 22 | 31 / 1 |
| 5.5 PGO | 108b935 | 0.994 | 0.960 | 40 / 150 / 33 | 83 / 1 |
| 5.4 plain | 1a5aad0 | 1.007 | 0.987 | 23 / 165 / 35 | 25 / 4 |
| 5.4 PGO | 1a5aad0 | 1.003 | 0.985 | 22 / 179 / 22 | 27 / 2 |

### Per set, geomean subject / 1a5aad0 and subject / 108b935 (cycles / instructions)

| set | n | plain vs 1a5 cyc | ins | PGO vs 1a5 cyc | ins | 5.4 plain vs 1a5 cyc | ins | 5.4 PGO vs 1a5 cyc | ins | plain vs 108 cyc | ins | PGO vs 108 cyc | ins |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| apps | 5 | 0.914 | 0.894 | 0.955 | 0.889 | 1.087 | 0.995 | 1.117 | 0.993 | 0.946 | 0.911 | 0.984 | 0.908 |
| awfy | 14 | 0.984 | 0.966 | 0.985 | 0.966 | 0.982 | 0.969 | 0.984 | 0.967 | 0.988 | 0.950 | 0.990 | 0.951 |
| corpus | 18 | 0.991 | 0.975 | 0.993 | 0.969 | 1.008 | 0.982 | 0.989 | 0.975 | 1.010 | 0.984 | 1.012 | 0.978 |
| corpus2/dev | 17 | 1.003 | 0.970 | 0.999 | 0.967 | 1.008 | 0.970 | 0.992 | 0.966 | 1.004 | 0.973 | 1.001 | 0.969 |
| corpus2/heldout | 11 | 0.966 | 0.915 | 0.962 | 0.913 | 0.969 | 0.916 | 0.968 | 0.913 | 0.992 | 0.960 | 0.994 | 0.960 |
| jit-trace | 29 | 1.000 | 0.991 | 1.002 | 0.993 | 1.000 | 0.994 | 1.002 | 0.993 | 0.992 | 0.902 | 0.995 | 0.903 |
| lib | 95 | 1.012 | 1.002 | 1.008 | 0.999 | 1.016 | 1.001 | 1.009 | 1.000 | 0.997 | 0.984 | 0.995 | 0.981 |
| programs | 25 | 1.005 | 0.983 | 1.007 | 0.983 | 1.001 | 0.986 | 1.010 | 0.985 | 0.976 | 0.939 | 0.974 | 0.938 |
| real | 5 | 0.939 | 0.937 | 0.947 | 0.939 | 0.956 | 0.940 | 0.946 | 0.945 | 0.982 | 0.959 | 0.996 | 0.962 |
| real_dis | 4 | 0.990 | 0.985 | 1.001 | 0.975 | 1.050 | 1.013 | 1.016 | 1.006 | 1.005 | 1.000 | 1.016 | 0.991 |
| ALL | 223 | 0.999 | 0.983 | 0.999 | 0.980 | 1.007 | 0.987 | 1.003 | 0.985 | 0.993 | 0.962 | 0.994 | 0.960 |

# Aggregates, readings B: with the 15-run serial reruns of 101 programs merged (min over all readings)

## Headline geomeans (cycles, ours/ref)

| build | /LuaJIT cyc (n) | /LuaJIT ins | /PUC same version cyc (223) | /PUC ins |
|---|---|---|---|---|
| 5.5 plain | 0.955 (219) | 1.369 | 0.218 | 0.224 |
| 5.5 PGO | 0.871 (219) | 1.261 | 0.199 | 0.206 |
| 5.4 plain | 1.020 (219) | 1.459 | 0.227 | 0.238 |
| 5.4 PGO | 0.929 (219) | 1.346 | 0.208 | 0.220 |

- 5.5 plain / LuaJIT -joff (219): 0.323
- 5.5 PGO / LuaJIT -joff (219): 0.294
- cached prev subject 1a5aad0 (same programs): 5.5 plain/LJ 0.964, PGO/LJ 0.879, 5.4 plain/LJ 1.020, 5.4 PGO/LJ 0.934
- cached 108b935 (same programs): plain/LJ 0.969, PGO/LJ 0.883
- our 5.4 build / our 5.5 build cycles: plain 1.067, PGO 1.066; PGO/plain: 5.5 0.914, 5.4 0.913

## Subject vs previous subject 1a5aad0 and vs 108b935 (geomean, 223 programs)

| build | vs | cycles | instructions | cyc >3% better / within / >3% worse | ins >3% fewer / more |
|---|---|---|---|---|---|
| 5.5 plain | 1a5aad0 | 0.991 | 0.983 | 31 / 175 / 17 | 28 / 2 |
| 5.5 plain | 108b935 | 0.985 | 0.962 | 48 / 159 / 16 | 79 / 1 |
| 5.5 PGO | 1a5aad0 | 0.991 | 0.980 | 29 / 186 / 8 | 31 / 1 |
| 5.5 PGO | 108b935 | 0.987 | 0.960 | 47 / 160 / 16 | 83 / 1 |
| 5.4 plain | 1a5aad0 | 1.000 | 0.987 | 27 / 179 / 17 | 25 / 4 |
| 5.4 PGO | 1a5aad0 | 0.995 | 0.985 | 26 / 190 / 7 | 27 / 2 |

### Per set, geomean subject / 1a5aad0 and subject / 108b935 (cycles / instructions)

| set | n | plain vs 1a5 cyc | ins | PGO vs 1a5 cyc | ins | 5.4 plain vs 1a5 cyc | ins | 5.4 PGO vs 1a5 cyc | ins | plain vs 108 cyc | ins | PGO vs 108 cyc | ins |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| apps | 5 | 0.901 | 0.894 | 0.941 | 0.889 | 1.071 | 0.995 | 1.104 | 0.993 | 0.932 | 0.911 | 0.969 | 0.908 |
| awfy | 14 | 0.976 | 0.966 | 0.978 | 0.966 | 0.974 | 0.969 | 0.975 | 0.967 | 0.980 | 0.950 | 0.982 | 0.951 |
| corpus | 18 | 0.982 | 0.975 | 0.983 | 0.969 | 0.993 | 0.978 | 0.979 | 0.975 | 1.002 | 0.984 | 1.002 | 0.978 |
| corpus2/dev | 17 | 0.991 | 0.970 | 0.989 | 0.967 | 0.997 | 0.970 | 0.981 | 0.966 | 0.992 | 0.973 | 0.991 | 0.969 |
| corpus2/heldout | 11 | 0.959 | 0.915 | 0.956 | 0.913 | 0.960 | 0.916 | 0.960 | 0.913 | 0.986 | 0.960 | 0.987 | 0.960 |
| jit-trace | 29 | 0.999 | 0.991 | 1.001 | 0.993 | 0.998 | 0.994 | 1.001 | 0.993 | 0.990 | 0.902 | 0.994 | 0.903 |
| lib | 95 | 1.005 | 1.002 | 1.001 | 0.999 | 1.010 | 1.001 | 1.002 | 1.000 | 0.989 | 0.984 | 0.988 | 0.981 |
| programs | 25 | 0.994 | 0.983 | 0.998 | 0.983 | 0.992 | 0.986 | 0.998 | 0.985 | 0.965 | 0.939 | 0.965 | 0.938 |
| real | 5 | 0.932 | 0.936 | 0.940 | 0.939 | 0.949 | 0.940 | 0.940 | 0.945 | 0.975 | 0.959 | 0.989 | 0.962 |
| real_dis | 4 | 0.975 | 0.985 | 0.988 | 0.975 | 1.032 | 1.013 | 1.003 | 1.006 | 0.990 | 1.000 | 1.002 | 0.991 |
| ALL | 223 | 0.991 | 0.983 | 0.991 | 0.980 | 1.000 | 0.987 | 0.995 | 0.985 | 0.985 | 0.962 | 0.987 | 0.960 |

### 5.5 plain vs 1a5aad0: 20 biggest improvements by cycles (>= 20 Mcyc)

| program | Mcyc | cyc ratio | ins ratio | cycle spread of readings | note |
|---|---|---|---|---|---|
| corpus2/heldout/roguelike_turns | 811 | 0.680 | 0.471 | 3.9% (20 runs) | instructions move the same way |
| apps/expr_lang | 139 | 0.684 | 0.767 | 4.1% (20 runs) | instructions move the same way |
| programs/binary_trees | 1509 | 0.783 | 0.745 | 6.3% (20 runs) | instructions move the same way |
| real/game_loop | 34 | 0.790 | 0.824 | 5.9% (20 runs) | instructions move the same way |
| awfy/cd | 258 | 0.809 | 0.824 | 2.2% (20 runs) | instructions move the same way |
| real/lua_interp | 103 | 0.883 | 0.882 | 2.6% (20 runs) | instructions move the same way |
| corpus2/dev/oop_mixins | 1471 | 0.884 | 0.857 | 5.2% (20 runs) | instructions move the same way |
| corpus/classic_vector | 629 | 0.891 | 0.872 | 3.0% (20 runs) | instructions move the same way |
| lib/fmt_q | 146 | 0.892 | 1.000 | 3.8% (17 runs) | instructions equal: layout / IPC |
| lib/setmt | 184 | 0.897 | 0.956 | 2.7% (20 runs) | instructions move the same way |
| lib/tunpack | 118 | 0.904 | 0.996 | 11.8% (20 runs) | instructions equal: layout / IPC |
| corpus/astar_jumper | 1613 | 0.906 | 0.808 | 14.4% (20 runs) | instructions move the same way |
| jit-trace/rec_loop | 81 | 0.915 | 0.877 | 8.4% (20 runs) | instructions move the same way |
| jit-trace/alloc_ctor | 187 | 0.917 | 0.932 | 9.0% (20 runs) | instructions move the same way |
| awfy/deltablue | 108 | 0.921 | 0.918 | 3.9% (20 runs) | instructions move the same way |
| awfy/list | 124 | 0.935 | 0.972 | 1.3% (5 runs) | instructions move the same way |
| corpus2/dev/particles_physics | 422 | 0.936 | 0.900 | 1.8% (5 runs) | instructions move the same way |
| corpus2/heldout/scheme_interp | 1463 | 0.940 | 0.949 | 0.7% (5 runs) | instructions move the same way |
| corpus2/heldout/log_analyzer | 2112 | 0.942 | 0.975 | 4.0% (20 runs) | instructions move the same way |
| apps/events | 159 | 0.943 | 0.993 | 3.8% (20 runs) | instructions equal: layout / IPC |

### 5.5 PGO vs 1a5aad0: 20 biggest improvements by cycles (>= 20 Mcyc)

| program | Mcyc | cyc ratio | ins ratio | cycle spread of readings | note |
|---|---|---|---|---|---|
| corpus2/heldout/roguelike_turns | 782 | 0.674 | 0.462 | 3.0% (20 runs) | instructions move the same way |
| apps/expr_lang | 128 | 0.707 | 0.777 | 9.1% (20 runs) | instructions move the same way |
| real/game_loop | 33 | 0.785 | 0.821 | 3.6% (20 runs) | instructions move the same way |
| awfy/cd | 238 | 0.819 | 0.826 | 3.3% (20 runs) | instructions move the same way |
| programs/binary_trees | 1298 | 0.820 | 0.763 | 36.1% (20 runs) | instructions move the same way |
| corpus2/dev/oop_mixins | 1393 | 0.892 | 0.856 | 8.2% (20 runs) | instructions move the same way |
| corpus/astar_jumper | 1522 | 0.892 | 0.802 | 17.6% (20 runs) | instructions move the same way |
| corpus/classic_vector | 565 | 0.905 | 0.871 | 4.4% (20 runs) | instructions move the same way |
| corpus2/heldout/log_analyzer | 1758 | 0.910 | 0.972 | 3.9% (20 runs) | instructions move the same way |
| jit-trace/rec_loop | 81 | 0.912 | 0.877 | 2.3% (20 runs) | instructions move the same way |
| real/lua_interp | 100 | 0.921 | 0.909 | 18.9% (20 runs) | instructions move the same way |
| awfy/deltablue | 101 | 0.925 | 0.914 | 2.7% (20 runs) | instructions move the same way |
| lib/fmt_g | 62 | 0.933 | 1.000 | 5.0% (20 runs) | instructions equal: layout / IPC |
| apps/lru_cache | 136 | 0.937 | 0.778 | 1.6% (5 runs) | instructions move the same way |
| corpus2/dev/particles_physics | 394 | 0.942 | 0.891 | 1.4% (5 runs) | instructions move the same way |
| corpus2/dev/expr_compiler | 1100 | 0.945 | 0.941 | 4.6% (20 runs) | instructions move the same way |
| awfy/list | 123 | 0.948 | 0.984 | 1.6% (5 runs) | instructions move the same way |
| corpus2/dev/memo_cache | 1562 | 0.954 | 0.909 | 4.4% (5 runs) | instructions move the same way |
| lib/tpack | 113 | 0.960 | 0.963 | 2.7% (5 runs) | instructions move the same way |
| lib/co_wrap | 202 | 0.960 | 1.001 | 3.9% (5 runs) | instructions equal: layout / IPC |

### 5.4 plain vs 1a5aad0: 20 biggest improvements by cycles (>= 20 Mcyc)

| program | Mcyc | cyc ratio | ins ratio | cycle spread of readings | note |
|---|---|---|---|---|---|
| corpus2/heldout/roguelike_turns | 852 | 0.657 | 0.486 | 3.9% (20 runs) | instructions move the same way |
| real/game_loop | 39 | 0.797 | 0.841 | 5.3% (20 runs) | instructions move the same way |
| programs/binary_trees | 1778 | 0.808 | 0.779 | 3.4% (20 runs) | instructions move the same way |
| awfy/cd | 288 | 0.811 | 0.829 | 2.9% (20 runs) | instructions move the same way |
| corpus2/dev/oop_mixins | 1483 | 0.885 | 0.860 | 3.5% (20 runs) | instructions move the same way |
| apps/events | 175 | 0.890 | 0.889 | 3.7% (20 runs) | instructions move the same way |
| real/lua_interp | 107 | 0.895 | 0.879 | 3.9% (20 runs) | instructions move the same way |
| corpus2/dev/ecs_world | 990 | 0.895 | 0.864 | 5.5% (20 runs) | instructions move the same way |
| corpus/astar_jumper | 1645 | 0.896 | 0.851 | 16.7% (20 runs) | instructions move the same way |
| jit-trace/rec_loop | 79 | 0.902 | 0.878 | 2.1% (20 runs) | instructions move the same way |
| lib/sel_loop | 217 | 0.908 | 0.998 | 8.6% (20 runs) | instructions equal: layout / IPC |
| lib/setmt | 225 | 0.910 | 0.966 | 4.8% (20 runs) | instructions move the same way |
| lib/tunpack | 127 | 0.912 | 0.995 | 3.5% (20 runs) | instructions equal: layout / IPC |
| jit-trace/alloc_ctor | 254 | 0.917 | 0.989 | 9.1% (20 runs) | instructions move the same way |
| corpus/classic_vector | 816 | 0.921 | 0.899 | 9.9% (20 runs) | instructions move the same way |
| lib/load_small | 217 | 0.926 | 0.987 | 6.5% (20 runs) | instructions move the same way |
| awfy/list | 130 | 0.932 | 0.954 | 1.6% (5 runs) | instructions move the same way |
| corpus2/dev/expr_compiler | 1348 | 0.936 | 0.937 | 3.2% (20 runs) | instructions move the same way |
| corpus2/heldout/prototype_objects | 1524 | 0.947 | 0.858 | 11.8% (20 runs) | instructions move the same way |
| apps/lru_cache | 185 | 0.949 | 0.839 | 2.2% (5 runs) | instructions move the same way |

### 5.4 PGO vs 1a5aad0: 20 biggest improvements by cycles (>= 20 Mcyc)

| program | Mcyc | cyc ratio | ins ratio | cycle spread of readings | note |
|---|---|---|---|---|---|
| corpus2/heldout/roguelike_turns | 824 | 0.657 | 0.478 | 2.7% (20 runs) | instructions move the same way |
| real/game_loop | 37 | 0.797 | 0.839 | 5.7% (20 runs) | instructions move the same way |
| programs/binary_trees | 1469 | 0.809 | 0.775 | 3.7% (20 runs) | instructions move the same way |
| awfy/cd | 264 | 0.814 | 0.830 | 4.2% (20 runs) | instructions move the same way |
| corpus2/dev/oop_mixins | 1360 | 0.860 | 0.852 | 3.8% (20 runs) | instructions move the same way |
| lib/load_small | 166 | 0.873 | 0.996 | 4.0% (20 runs) | instructions equal: layout / IPC |
| corpus/classic_vector | 670 | 0.881 | 0.884 | 15.7% (20 runs) | instructions move the same way |
| lib/sel_loop | 215 | 0.897 | 0.998 | 9.3% (20 runs) | instructions equal: layout / IPC |
| jit-trace/rec_loop | 78 | 0.900 | 0.878 | 2.2% (20 runs) | instructions move the same way |
| corpus2/dev/ecs_world | 857 | 0.904 | 0.854 | 2.0% (20 runs) | instructions move the same way |
| corpus/astar_jumper | 1592 | 0.914 | 0.846 | 13.5% (20 runs) | instructions move the same way |
| corpus/parser_combinator | 1977 | 0.917 | 0.994 | 13.2% (20 runs) | instructions equal: layout / IPC |
| real/lua_interp | 104 | 0.921 | 0.905 | 6.2% (20 runs) | instructions move the same way |
| apps/lru_cache | 174 | 0.933 | 0.810 | 1.9% (5 runs) | instructions move the same way |
| corpus/mustache_lustache | 1532 | 0.937 | 0.989 | 7.8% (20 runs) | instructions move the same way |
| apps/events | 152 | 0.939 | 0.896 | 3.7% (20 runs) | instructions move the same way |
| awfy/deltablue | 110 | 0.942 | 0.936 | 3.7% (20 runs) | instructions move the same way |
| corpus2/dev/expr_compiler | 1152 | 0.943 | 0.933 | 2.9% (20 runs) | instructions move the same way |
| corpus/testrunner_luaunit | 1337 | 0.951 | 0.963 | 3.0% (5 runs) | instructions move the same way |
| corpus/class_middleclass | 507 | 0.953 | 0.984 | 9.9% (20 runs) | instructions move the same way |

### 5.5 plain vs 1a5aad0: regressions (instructions > +2% or cycles > +3%, >= 20 Mcyc)

| program | Mcyc | cyc ratio | ins ratio | cycle spread of readings | note |
|---|---|---|---|---|---|
| lib/tonumber_int | 155 | 1.138 | 1.013 | 2.5% (20 runs) | instructions move the same way |
| lib/tsort_int | 277 | 1.068 | 1.015 | 4.1% (20 runs) | instructions move the same way |
| programs/table_hash | 503 | 1.065 | 1.007 | 26.3% (20 runs) | instructions equal: layout / IPC |
| programs/load_big | 810 | 1.063 | 0.996 | 3.3% (17 runs) | instructions equal: layout / IPC |
| lib/tinsert_idx | 174 | 1.058 | 1.034 | 4.0% (20 runs) | instructions move the same way |
| corpus2/dev/regex_nfa | 692 | 1.057 | 0.990 | 16.0% (20 runs) |  |
| lib/load_small | 241 | 1.054 | 0.972 | 5.1% (20 runs) |  |
| programs/sieve | 579 | 1.049 | 1.000 | 1.8% (17 runs) | instructions equal: layout / IPC |
| lib/tconcat | 148 | 1.047 | 1.007 | 10.1% (20 runs) | instructions equal: layout / IPC |
| lib/tostring_int | 121 | 1.044 | 1.029 | 3.1% (20 runs) | instructions move the same way |
| programs/string_ops | 1151 | 1.042 | 1.008 | 23.0% (20 runs) | instructions equal: layout / IPC |
| lib/tsort_str | 261 | 1.038 | 1.011 | 2.6% (20 runs) | instructions move the same way |
| corpus2/heldout/dijkstra_routes | 1273 | 1.024 | 1.029 | 3.6% (20 runs) |  |
| corpus2/heldout/real_dis_mips | 2188 | 1.016 | 1.031 | 3.0% (20 runs) |  |
| lib/tinsert_front | 223 | 1.007 | 1.027 | 5.5% (20 runs) |  |
| lib/fmt_d | 94 | 1.000 | 1.023 | 3.8% (20 runs) |  |
| lib/concat_op | 140 | 0.992 | 1.021 | 3.8% (20 runs) |  |

### 5.5 PGO vs 1a5aad0: regressions (instructions > +2% or cycles > +3%, >= 20 Mcyc)

| program | Mcyc | cyc ratio | ins ratio | cycle spread of readings | note |
|---|---|---|---|---|---|
| apps/raytrace | 130 | 1.125 | 0.945 | 4.5% (20 runs) |  |
| corpus2/dev/event_promise | 361 | 1.118 | 0.986 | 7.9% (20 runs) |  |
| programs/table_hash | 492 | 1.101 | 1.003 | 30.5% (20 runs) | instructions equal: layout / IPC |
| lib/load_small | 171 | 1.090 | 0.982 | 2.4% (20 runs) |  |
| awfy/storage | 78 | 1.035 | 1.012 | 3.3% (20 runs) | instructions move the same way |
| programs/sieve | 574 | 1.032 | 1.000 | 3.2% (17 runs) | instructions equal: layout / IPC |
| corpus2/heldout/dijkstra_routes | 1241 | 1.026 | 1.037 | 3.1% (20 runs) |  |
| lib/tinsert_idx | 151 | 1.024 | 1.028 | 9.5% (20 runs) |  |

### 5.4 plain vs 1a5aad0: regressions (instructions > +2% or cycles > +3%, >= 20 Mcyc)

| program | Mcyc | cyc ratio | ins ratio | cycle spread of readings | note |
|---|---|---|---|---|---|
| apps/expr_lang | 239 | 1.500 | 1.219 | 2.9% (20 runs) | instructions move the same way |
| lib/gsub_fn | 227 | 1.260 | 1.002 | 2.7% (17 runs) | instructions equal: layout / IPC |
| lib/tonumber_bad | 165 | 1.204 | 1.010 | 13.5% (20 runs) | instructions move the same way |
| corpus2/dev/diff_levenshtein | 516 | 1.162 | 1.012 | 2.2% (20 runs) | instructions move the same way |
| lib/tonumber_int | 161 | 1.102 | 1.012 | 2.4% (20 runs) | instructions move the same way |
| apps/raytrace | 159 | 1.075 | 1.068 | 5.4% (20 runs) | instructions move the same way |
| lib/tostring_int | 138 | 1.067 | 1.026 | 3.4% (20 runs) | instructions move the same way |
| lib/pcall_err | 134 | 1.050 | 0.999 | 8.8% (20 runs) | instructions equal: layout / IPC |
| lib/co_yield | 243 | 1.046 | 1.000 | 5.3% (17 runs) | instructions equal: layout / IPC |
| corpus2/heldout/real_dis_mips | 2259 | 1.046 | 1.028 | 3.0% (20 runs) | instructions move the same way |
| lib/gmatch_words | 187 | 1.041 | 1.001 | 21.5% (20 runs) | instructions equal: layout / IPC |
| corpus2/dev/wordfreq_wrap | 672 | 1.037 | 1.004 | 9.6% (20 runs) | instructions equal: layout / IPC |
| lib/it_ipairs | 280 | 1.036 | 0.997 | 6.5% (20 runs) | instructions equal: layout / IPC |
| apps/markdown | 123 | 1.034 | 1.006 | 3.3% (20 runs) | instructions equal: layout / IPC |
| lib/tonumber_flt | 201 | 1.033 | 1.005 | 2.3% (17 runs) | instructions equal: layout / IPC |
| corpus2/heldout/real_dis_ppc | 2289 | 1.030 | 1.006 | 4.2% (20 runs) | instructions equal: layout / IPC |
| corpus2/dev/real_dis_arm | 2579 | 1.029 | 1.023 | 4.0% (20 runs) |  |
| lib/tinsert_idx | 201 | 1.014 | 1.032 | 5.2% (20 runs) |  |
| awfy/storage | 146 | 1.007 | 1.024 | 4.7% (20 runs) |  |
| lib/tinsert_append | 191 | 1.003 | 1.030 | 5.2% (20 runs) |  |
| lib/fmt_d | 101 | 0.996 | 1.021 | 7.4% (20 runs) |  |

### 5.4 PGO vs 1a5aad0: regressions (instructions > +2% or cycles > +3%, >= 20 Mcyc)

| program | Mcyc | cyc ratio | ins ratio | cycle spread of readings | note |
|---|---|---|---|---|---|
| apps/expr_lang | 214 | 1.531 | 1.208 | 5.2% (20 runs) | instructions move the same way |
| apps/raytrace | 148 | 1.217 | 1.105 | 6.3% (20 runs) | instructions move the same way |
| lib/tonumber_num | 214 | 1.087 | 0.999 | 7.7% (20 runs) | instructions equal: layout / IPC |
| programs/string_ops | 1020 | 1.043 | 0.999 | 8.5% (20 runs) | instructions equal: layout / IPC |
| corpus/csv_ini_yaml | 1677 | 1.034 | 0.993 | 2.0% (20 runs) | instructions equal: layout / IPC |
| awfy/storage | 114 | 1.031 | 1.013 | 9.5% (20 runs) | instructions move the same way |
| corpus/msgpack_pure | 1940 | 1.030 | 1.001 | 3.9% (20 runs) | instructions equal: layout / IPC |
| lib/tinsert_append | 182 | 1.029 | 1.025 | 4.9% (20 runs) |  |
| lib/tinsert_idx | 190 | 1.023 | 1.026 | 3.5% (20 runs) |  |

### 5.5 plain vs 108b935: 15 biggest improvements by cycles (>= 20 Mcyc)

| program | Mcyc | cyc ratio | ins ratio | cycle spread of readings | note |
|---|---|---|---|---|---|
| programs/nbody | 348 | 0.669 | 0.681 | 0.3% (2 runs) | instructions move the same way |
| apps/expr_lang | 139 | 0.725 | 0.799 | 4.1% (20 runs) | instructions move the same way |
| programs/binary_trees | 1509 | 0.779 | 0.745 | 6.3% (20 runs) | instructions move the same way |
| awfy/nbody | 92 | 0.784 | 0.765 | 0.9% (5 runs) | instructions move the same way |
| lib/type_call | 87 | 0.794 | 0.825 | 63.0% (20 runs) | instructions move the same way |
| lib/sel_n | 320 | 0.834 | 0.791 | 35.1% (20 runs) | instructions move the same way |
| jit-trace/field_method | 570 | 0.843 | 0.739 | 0.1% (2 runs) | instructions move the same way |
| jit-trace/rec_loop | 81 | 0.877 | 0.843 | 8.4% (20 runs) | instructions move the same way |
| lib/sel_loop | 210 | 0.880 | 0.827 | 11.6% (20 runs) | instructions move the same way |
| lib/setmt | 184 | 0.886 | 0.958 | 2.7% (20 runs) | instructions move the same way |
| jit-trace/alloc_ctor | 187 | 0.891 | 0.926 | 9.0% (20 runs) | instructions move the same way |
| lib/len_op | 49 | 0.891 | 0.961 | 6.1% (5 runs) | instructions move the same way |
| jit-trace/oop_override | 108 | 0.897 | 0.871 | 0.7% (2 runs) | instructions move the same way |
| real/lua_interp | 103 | 0.901 | 0.884 | 2.6% (20 runs) | instructions move the same way |
| lib/sel_count | 218 | 0.912 | 0.791 | 5.4% (5 runs) | instructions move the same way |

### 5.5 plain vs 108b935: regressions (instructions > +2% or cycles > +3%, >= 20 Mcyc)

| program | Mcyc | cyc ratio | ins ratio | cycle spread of readings | note |
|---|---|---|---|---|---|
| lib/tonumber_int | 155 | 1.111 | 1.013 | 2.5% (20 runs) | instructions move the same way |
| corpus2/dev/event_promise | 375 | 1.084 | 0.975 | 6.3% (20 runs) |  |
| jit-trace/inline_2 | 324 | 1.081 | 0.843 | 2.4% (17 runs) |  |
| programs/table_hash | 503 | 1.069 | 1.009 | 26.3% (20 runs) | instructions equal: layout / IPC |
| programs/load_big | 810 | 1.062 | 0.996 | 3.3% (17 runs) | instructions equal: layout / IPC |
| corpus/parser_combinator | 2410 | 1.062 | 1.008 | 11.2% (20 runs) | instructions equal: layout / IPC |
| lib/tsort_int | 277 | 1.051 | 1.008 | 4.1% (20 runs) | instructions equal: layout / IPC |
| lib/find_cap | 274 | 1.048 | 0.999 | 4.9% (20 runs) | instructions equal: layout / IPC |
| corpus2/heldout/prototype_objects | 1511 | 1.047 | 0.971 | 15.0% (20 runs) |  |
| awfy/richards | 60 | 1.042 | 1.006 | 4.7% (20 runs) | instructions equal: layout / IPC |
| corpus/class_middleclass | 497 | 1.036 | 0.917 | 8.9% (20 runs) |  |
| lib/tostring_int | 121 | 1.034 | 1.020 | 3.1% (20 runs) | instructions move the same way |
| corpus2/heldout/real_dis_mips | 2188 | 1.026 | 1.031 | 3.0% (20 runs) |  |
| lib/tinsert_idx | 174 | 1.011 | 1.022 | 4.0% (20 runs) |  |
| lib/tinsert_front | 223 | 1.002 | 1.026 | 5.5% (20 runs) |  |

### 5.5 PGO vs 108b935: 15 biggest improvements by cycles (>= 20 Mcyc)

| program | Mcyc | cyc ratio | ins ratio | cycle spread of readings | note |
|---|---|---|---|---|---|
| programs/nbody | 348 | 0.670 | 0.681 | 0.2% (2 runs) | instructions move the same way |
| apps/expr_lang | 128 | 0.751 | 0.813 | 9.1% (20 runs) | instructions move the same way |
| awfy/nbody | 92 | 0.781 | 0.765 | 1.7% (5 runs) | instructions move the same way |
| lib/type_call | 87 | 0.795 | 0.825 | 2.3% (20 runs) | instructions move the same way |
| programs/binary_trees | 1298 | 0.799 | 0.765 | 36.1% (20 runs) | instructions move the same way |
| jit-trace/field_method | 565 | 0.837 | 0.738 | 0.1% (2 runs) | instructions move the same way |
| jit-trace/rec_loop | 81 | 0.878 | 0.843 | 2.3% (20 runs) | instructions move the same way |
| lib/len_op | 48 | 0.882 | 0.961 | 7.8% (5 runs) | instructions move the same way |
| lib/sel_loop | 208 | 0.889 | 0.827 | 9.0% (20 runs) | instructions move the same way |
| lib/sel_n | 320 | 0.893 | 0.791 | 23.9% (20 runs) | instructions move the same way |
| jit-trace/oop_override | 108 | 0.897 | 0.870 | 0.2% (2 runs) | instructions move the same way |
| programs/heapsort | 1070 | 0.919 | 0.928 | 6.8% (5 runs) | instructions move the same way |
| corpus2/heldout/log_analyzer | 1758 | 0.920 | 0.974 | 3.9% (20 runs) | instructions move the same way |
| jit-trace/inline_leaf | 456 | 0.921 | 0.727 | 1.3% (2 runs) | instructions move the same way |
| corpus2/dev/diff_levenshtein | 406 | 0.926 | 0.929 | 4.8% (20 runs) | instructions move the same way |

### 5.5 PGO vs 108b935: regressions (instructions > +2% or cycles > +3%, >= 20 Mcyc)

| program | Mcyc | cyc ratio | ins ratio | cycle spread of readings | note |
|---|---|---|---|---|---|
| apps/raytrace | 130 | 1.176 | 0.966 | 4.5% (20 runs) |  |
| lib/load_small | 171 | 1.101 | 0.992 | 2.4% (20 runs) | instructions equal: layout / IPC |
| jit-trace/inline_2 | 323 | 1.086 | 0.843 | 2.8% (17 runs) |  |
| corpus2/heldout/prototype_objects | 1435 | 1.073 | 0.978 | 12.2% (20 runs) |  |
| programs/table_hash | 492 | 1.073 | 1.006 | 30.5% (20 runs) | instructions equal: layout / IPC |
| corpus2/dev/event_promise | 361 | 1.063 | 0.974 | 7.9% (20 runs) |  |
| awfy/deltablue | 101 | 1.041 | 0.990 | 2.7% (20 runs) |  |
| awfy/richards | 57 | 1.037 | 1.003 | 10.6% (20 runs) | instructions equal: layout / IPC |
| corpus/mustache_lustache | 1457 | 1.037 | 0.994 | 3.6% (20 runs) | instructions equal: layout / IPC |
| awfy/storage | 78 | 1.034 | 1.002 | 3.3% (20 runs) | instructions equal: layout / IPC |
| programs/string_ops | 1080 | 1.032 | 0.997 | 7.5% (20 runs) | instructions equal: layout / IPC |
| corpus2/dev/csv_report | 1040 | 1.030 | 0.987 | 8.4% (20 runs) |  |
| lib/tsort_cmp | 257 | 0.994 | 1.032 | 1.0% (5 runs) |  |

### 5.5 plain: 15 biggest gaps to LuaJIT (ours/LJ cycles)

| program | Mcyc | cyc/LJ | ins/LJ |
|---|---|---|---|
| apps/raytrace | 139 | 3.431 | 2.714 |
| awfy/permute | 44 | 3.368 | 4.442 |
| corpus/lua_lexer | 1264 | 2.819 | 2.687 |
| programs/fib | 1254 | 2.670 | 3.123 |
| corpus/classic_vector | 629 | 2.615 | 2.452 |
| awfy/cd | 258 | 2.395 | 2.565 |
| lib/co_yield | 240 | 2.355 | 1.882 |
| lib/fmt_s | 7 | 2.344 | 2.770 |
| corpus2/heldout/stream_closures | 935 | 2.247 | 2.580 |
| awfy/towers | 51 | 2.173 | 1.839 |
| corpus2/dev/text_adventure | 2572 | 2.168 | 2.170 |
| programs/coroutines | 1802 | 2.067 | 1.789 |
| lib/byte_multi | 41 | 2.028 | 3.572 |
| awfy/json | 143 | 1.952 | 2.231 |
| lib/find_plain | 128 | 1.889 | 2.353 |

### 5.5 plain: 15 biggest wins over LuaJIT (>= 20 Mcyc)

| program | Mcyc | cyc/LJ | ins/LJ |
|---|---|---|---|
| lib/sel_loop | 210 | 0.040 | 0.067 |
| lib/error_tbl | 270 | 0.089 | 0.079 |
| jit-trace/int_rec2 | 681 | 0.136 | 1.551 |
| lib/pcall_err | 119 | 0.156 | 0.136 |
| programs/pcall_error | 1470 | 0.176 | 0.184 |
| lib/tfront_1k | 79 | 0.195 | 0.076 |
| lib/it_next | 45 | 0.250 | 0.704 |
| lib/tonumber_flt | 196 | 0.253 | 0.395 |
| corpus/sha2_base64 | 426 | 0.302 | 0.331 |
| lib/match | 61 | 0.316 | 0.316 |
| apps/markdown | 116 | 0.353 | 0.268 |
| jit-trace/int_sum | 889 | 0.354 | 1.272 |
| lib/find_pat | 259 | 0.367 | 0.451 |
| lib/gsub_str | 167 | 0.420 | 0.499 |
| lib/tostring_int | 121 | 0.432 | 0.800 |

### 5.5 PGO: 15 biggest gaps to LuaJIT (ours/LJ cycles)

| program | Mcyc | cyc/LJ | ins/LJ |
|---|---|---|---|
| awfy/permute | 44 | 3.331 | 4.427 |
| apps/raytrace | 130 | 3.209 | 2.536 |
| programs/fib | 1255 | 2.672 | 3.123 |
| corpus/lua_lexer | 1119 | 2.496 | 2.549 |
| corpus/classic_vector | 565 | 2.349 | 2.106 |
| lib/fmt_s | 7 | 2.233 | 2.668 |
| awfy/cd | 238 | 2.213 | 2.383 |
| awfy/towers | 49 | 2.092 | 1.791 |
| corpus2/heldout/stream_closures | 858 | 2.062 | 2.372 |
| lib/byte_multi | 40 | 2.003 | 3.563 |
| lib/getmt | 16 | 1.790 | 2.416 |
| jit-trace/type_entry | 1514 | 1.785 | 2.396 |
| lib/co_yield | 182 | 1.780 | 1.626 |
| corpus2/dev/text_adventure | 2111 | 1.779 | 1.919 |
| awfy/queens | 95 | 1.721 | 1.909 |

### 5.5 PGO: 15 biggest wins over LuaJIT (>= 20 Mcyc)

| program | Mcyc | cyc/LJ | ins/LJ |
|---|---|---|---|
| lib/sel_loop | 208 | 0.040 | 0.067 |
| lib/error_tbl | 218 | 0.071 | 0.062 |
| lib/pcall_err | 99 | 0.129 | 0.103 |
| jit-trace/int_rec2 | 678 | 0.135 | 1.551 |
| programs/pcall_error | 1220 | 0.146 | 0.151 |
| lib/tfront_1k | 76 | 0.188 | 0.069 |
| lib/tonumber_flt | 170 | 0.219 | 0.352 |
| lib/it_next | 45 | 0.249 | 0.702 |
| lib/match | 51 | 0.264 | 0.279 |
| corpus/sha2_base64 | 406 | 0.287 | 0.317 |
| apps/markdown | 101 | 0.307 | 0.240 |
| lib/gsub_str | 128 | 0.322 | 0.394 |
| lib/find_pat | 235 | 0.333 | 0.408 |
| jit-trace/int_sum | 891 | 0.354 | 1.272 |
| lib/tpack | 113 | 0.370 | 0.386 |

## Focus programs (cycles in M; ratios of cycles)

| program | plain Mcyc | PGO Mcyc | LJ Mcyc | PUC Mcyc | plain/LJ | PGO/LJ | plain/PUC | PGO/PUC | plain vs 1a5 cyc | ins | PGO vs 1a5 cyc | ins | plain vs 108 cyc | PGO vs 108 cyc | PGO/LJ 1a5aad0 |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| programs/nbody | 348.4 | 347.9 | 345.6 | 4246.2 | 1.008 | 1.007 | 0.082 | 0.082 | 1.010 | 0.996 | 1.009 | 0.996 | 0.669 | 0.670 | 0.998 |
| programs/fib | 1254.1 | 1255.3 | 469.7 | 4139.5 | 2.670 | 2.672 | 0.303 | 0.303 | 0.999 | 0.948 | 1.007 | 0.948 | 1.015 | 1.011 | 2.654 |
| programs/binary_trees | 1508.7 | 1298.4 | 2007.4 | 4389.1 | 0.752 | 0.647 | 0.344 | 0.296 | 0.783 | 0.745 | 0.820 | 0.763 | 0.779 | 0.799 | 0.789 |
| programs/queens | 1842.5 | 1842.0 | 1907.1 | 4368.7 | 0.966 | 0.966 | 0.422 | 0.422 | 0.993 | 0.975 | 0.997 | 0.975 | 0.981 | 0.981 | 0.969 |
| apps/raytrace | 139.2 | 130.2 | 40.6 | 262.7 | 3.431 | 3.209 | 0.530 | 0.495 | 0.980 | 0.918 | 1.125 | 0.945 | 1.017 | 1.176 | 2.852 |
| awfy/cd | 257.8 | 238.2 | 107.6 | 437.5 | 2.395 | 2.213 | 0.589 | 0.545 | 0.809 | 0.824 | 0.819 | 0.826 | 0.963 | 0.982 | 2.702 |
| awfy/list | 124.3 | 123.2 | 88.9 | 418.8 | 1.399 | 1.387 | 0.297 | 0.294 | 0.935 | 0.972 | 0.948 | 0.984 | 0.941 | 0.952 | 1.463 |
| awfy/richards | 59.5 | 57.3 | 34.7 | 197.7 | 1.715 | 1.650 | 0.301 | 0.290 | 0.978 | 0.944 | 0.975 | 0.936 | 1.042 | 1.037 | 1.692 |
| awfy/deltablue | 108.0 | 101.2 | 103.5 | 157.9 | 1.044 | 0.978 | 0.684 | 0.641 | 0.921 | 0.918 | 0.925 | 0.914 | 1.029 | 1.041 | 1.058 |
| awfy/havlak | 152.7 | 133.1 | 115.2 | 217.5 | 1.325 | 1.155 | 0.702 | 0.612 | 0.990 | 0.951 | 0.984 | 0.949 | 1.028 | 1.013 | 1.174 |

## Per-program table (5.5 and 5.4; Mcyc, ratios vs 1a5aad0 and LJ)

| program | n runs (plain) | plain | PGO | p54 | g54 | LJ | PUC55 | PUC54 | plain/LJ | PGO/LJ | plain vs 1a5 cyc | ins | PGO vs 1a5 cyc | ins | p54 vs 1a5 cyc | ins | g54 vs 1a5 cyc | ins | out/exp (all 4) |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| apps/events | 20 | 158.9 | 140.5 | 175.4 | 151.7 | 95.4 | 293.7 | 289.5 | 1.665 | 1.472 | 0.943 | 0.993 | 0.992 | 1.000 | 0.890 | 0.889 | 0.939 | 0.896 | ok |
| apps/expr_lang | 20 | 139.3 | 127.9 | 238.6 | 213.8 | 96.7 | 241.7 | 253.2 | 1.441 | 1.323 | 0.684 | 0.767 | 0.707 | 0.777 | 1.500 | 1.219 | 1.531 | 1.208 | ok |
| apps/lru_cache | 5 | 146.7 | 135.5 | 185.5 | 173.6 | 106.4 | 219.8 | 253.9 | 1.378 | 1.274 | 0.947 | 0.810 | 0.937 | 0.778 | 0.949 | 0.839 | 0.933 | 0.810 | ok |
| apps/markdown | 20 | 116.5 | 101.2 | 123.4 | 105.3 | 329.7 | 412.6 | 390.8 | 0.353 | 0.307 | 0.991 | 1.006 | 0.996 | 0.972 | 1.034 | 1.006 | 1.003 | 0.999 | ok |
| apps/raytrace | 20 | 139.2 | 130.2 | 159.2 | 147.7 | 40.6 | 262.7 | 281.7 | 3.431 | 3.209 | 0.980 | 0.918 | 1.125 | 0.945 | 1.075 | 1.068 | 1.217 | 1.105 | ok |
| awfy/bounce | 5 | 61.1 | 59.5 | 67.5 | 65.4 | 47.7 | 511.2 | 537.1 | 1.281 | 1.249 | 0.992 | 0.983 | 0.991 | 0.983 | 0.987 | 0.983 | 0.989 | 0.985 | ok |
| awfy/cd | 20 | 257.8 | 238.2 | 288.3 | 264.4 | 107.6 | 437.5 | 464.4 | 2.395 | 2.213 | 0.809 | 0.824 | 0.819 | 0.826 | 0.811 | 0.829 | 0.814 | 0.830 | ok |
| awfy/deltablue | 20 | 108.0 | 101.2 | 119.8 | 110.5 | 103.5 | 157.9 | 165.2 | 1.044 | 0.978 | 0.921 | 0.918 | 0.925 | 0.914 | 0.954 | 0.943 | 0.942 | 0.936 | ok |
| awfy/havlak | 5 | 152.7 | 133.1 | 177.4 | 157.7 | 115.2 | 217.5 | 225.5 | 1.325 | 1.155 | 0.990 | 0.951 | 0.984 | 0.949 | 1.007 | 0.966 | 0.996 | 0.960 | ok |
| awfy/json | 20 | 143.3 | 119.9 | 157.2 | 133.4 | 73.4 | 262.9 | 271.8 | 1.952 | 1.633 | 0.989 | 0.979 | 0.968 | 0.974 | 0.989 | 0.979 | 0.969 | 0.970 | ok |
| awfy/list | 5 | 124.3 | 123.2 | 130.2 | 128.8 | 88.9 | 418.8 | 477.9 | 1.399 | 1.387 | 0.935 | 0.972 | 0.948 | 0.984 | 0.932 | 0.954 | 0.955 | 0.968 | ok |
| awfy/mandelbrot | 2 | 272.8 | 272.6 | 272.9 | 272.4 | 273.1 | 1529.3 | 1379.3 | 0.999 | 0.998 | 1.001 | 0.997 | 1.002 | 0.997 | 1.001 | 0.996 | 1.000 | 0.996 | ok |
| awfy/nbody | 5 | 92.4 | 91.7 | 95.8 | 95.1 | 94.6 | 1504.3 | 1603.8 | 0.977 | 0.970 | 1.001 | 0.995 | 1.001 | 0.995 | 0.985 | 0.995 | 0.992 | 0.995 | ok |
| awfy/permute | 5 | 44.2 | 43.7 | 45.0 | 44.7 | 13.1 | 246.3 | 257.2 | 3.368 | 3.331 | 1.017 | 0.981 | 1.021 | 0.981 | 1.003 | 0.981 | 1.008 | 0.981 | ok |
| awfy/queens | 5 | 96.9 | 94.7 | 81.9 | 79.6 | 55.0 | 401.8 | 403.4 | 1.760 | 1.721 | 1.010 | 0.992 | 1.011 | 0.991 | 1.006 | 0.992 | 1.006 | 0.990 | ok |
| awfy/richards | 20 | 59.5 | 57.3 | 60.0 | 57.6 | 34.7 | 197.7 | 199.3 | 1.715 | 1.650 | 0.978 | 0.944 | 0.975 | 0.936 | 0.977 | 0.944 | 0.970 | 0.937 | ok |
| awfy/sieve | 20 | 122.3 | 122.4 | 127.4 | 126.5 | 156.4 | 1032.3 | 1085.2 | 0.782 | 0.783 | 1.013 | 0.997 | 1.009 | 0.997 | 0.993 | 0.997 | 1.001 | 0.997 | ok |
| awfy/storage | 20 | 101.6 | 78.1 | 145.7 | 114.2 | 154.0 | 255.9 | 262.0 | 0.659 | 0.507 | 1.009 | 1.005 | 1.035 | 1.012 | 1.007 | 1.024 | 1.031 | 1.013 | ok |
| awfy/towers | 17 | 51.0 | 49.1 | 52.0 | 49.8 | 23.5 | 215.1 | 213.9 | 2.173 | 2.092 | 1.025 | 0.999 | 1.019 | 0.998 | 1.010 | 1.000 | 1.002 | 0.998 | ok |
| corpus/astar_jumper | 20 | 1612.7 | 1521.8 | 1645.3 | 1592.5 | 997.4 | 3280.6 | 3358.7 | 1.617 | 1.526 | 0.906 | 0.808 | 0.892 | 0.802 | 0.896 | 0.851 | 0.914 | 0.846 | ok |
| corpus/class_middleclass | 20 | 496.7 | 486.5 | 525.1 | 506.8 | 319.5 | 3046.3 | 3098.9 | 1.555 | 1.523 | 0.965 | 0.981 | 0.964 | 0.981 | 0.966 | 0.984 | 0.953 | 0.984 | ok |
| corpus/classic_vector | 20 | 629.3 | 565.3 | 815.8 | 670.3 | 240.7 | 2945.5 | 2905.4 | 2.615 | 2.349 | 0.891 | 0.872 | 0.905 | 0.871 | 0.921 | 0.899 | 0.881 | 0.884 | ok |
| corpus/containers_lru_heap | 5 | 1272.0 | 1208.0 | 1671.5 | 1538.0 | 1417.3 | 2675.7 | 3035.3 | 0.897 | 0.852 | 0.971 | 0.991 | 0.972 | 0.978 | 1.025 | 0.993 | 0.997 | 0.989 | ok |
| corpus/coro_scheduler | 5 | 1506.1 | 1311.2 | 1643.1 | 1412.4 | 941.9 | 2772.8 | 2851.7 | 1.599 | 1.392 | 0.978 | 0.977 | 0.974 | 0.981 | 1.002 | 0.983 | 1.000 | 0.987 | ok |
| corpus/csv_ini_yaml | 20 | 1559.0 | 1344.0 | 1903.0 | 1677.5 | 1771.4 | 2948.4 | 3020.4 | 0.880 | 0.759 | 0.981 | 0.991 | 0.970 | 0.976 | 1.024 | 0.996 | 1.034 | 0.993 | ok |
| corpus/inspect_lua | 5 | 1578.8 | 1400.1 | 1706.1 | 1534.2 | 1056.4 | 2901.3 | 2968.0 | 1.495 | 1.325 | 0.990 | 0.975 | 0.998 | 0.969 | 0.998 | 0.977 | 1.007 | 0.975 | ok |
| corpus/json_rxi | 20 | 1832.1 | 1510.3 | 1892.4 | 1590.8 | 1222.9 | 3647.9 | 3646.8 | 1.498 | 1.235 | 1.000 | 0.998 | 0.984 | 0.998 | 1.010 | 1.002 | 1.011 | 0.999 | ok |
| corpus/lua_lexer | 5 | 1263.8 | 1118.6 | 1128.2 | 994.9 | 448.2 | 3281.3 | 3358.7 | 2.819 | 2.496 | 1.016 | 0.990 | 0.985 | 0.977 | 1.008 | 0.984 | 1.009 | 0.982 | ok |
| corpus/markdown_conv | 20 | 1405.0 | 1260.8 | 1433.1 | 1268.9 | 3029.8 | 3746.9 | 3671.3 | 0.464 | 0.416 | 0.992 | 1.001 | 1.006 | 0.989 | 1.011 | 1.003 | 0.991 | 0.999 | ok |
| corpus/msgpack_pure | 20 | 2029.2 | 1769.7 | 2202.4 | 1939.7 | 4473.7 | 3436.5 | 3601.3 | 0.454 | 0.396 | 0.992 | 1.006 | 0.999 | 1.003 | 1.011 | 1.004 | 1.030 | 1.001 | ok |
| corpus/mustache_lustache | 20 | 1802.6 | 1457.2 | 1917.5 | 1532.4 | 1537.1 | 3296.2 | 3446.8 | 1.173 | 0.948 | 0.990 | 0.993 | 1.013 | 0.991 | 0.998 | 0.989 | 0.937 | 0.989 | ok |
| corpus/parser_combinator | 20 | 2410.4 | 1904.2 | 2428.0 | 1976.7 | 1563.1 | 3390.6 | 3391.8 | 1.542 | 1.218 | 1.005 | 1.007 | 1.006 | 1.012 | 0.985 | 0.999 | 0.917 | 0.994 | ok |
| corpus/penlight_mix | 5 | 2308.1 | 2157.8 | 2424.9 | 2293.0 | 2678.7 | 3992.1 | 3970.3 | 0.862 | 0.806 | 1.011 | 0.995 | 1.004 | 0.963 | 1.025 | 0.997 | 1.010 | 0.996 | ok |
| corpus/serpent_ser | 5 | 2039.8 | 1766.2 | 2132.2 | 1840.9 | 2426.7 | 3260.6 | 3254.8 | 0.841 | 0.728 | 1.007 | 0.999 | 0.990 | 0.992 | 1.026 | 0.997 | 1.005 | 0.993 | ok |
| corpus/sha2_base64 | 5 | 425.8 | 405.7 | 391.7 | 371.5 | 1411.3 | 3328.6 | 3384.1 | 0.302 | 0.287 | 1.008 | 1.000 | 1.004 | 0.996 | 1.005 | 0.998 | 1.006 | 0.998 | ok |
| corpus/template_etlua | 17 | 1572.1 | 1273.6 | 1560.4 | 1273.8 | 1826.0 | 2680.1 | 2700.4 | 0.861 | 0.697 | 0.980 | 1.000 | 1.013 | 0.997 | 0.999 | 0.998 | 0.981 | 0.996 | ok |
| corpus/testrunner_luaunit | 5 | 1529.2 | 1251.6 | 1657.6 | 1336.6 | 1390.7 | 2601.9 | 2628.8 | 1.100 | 0.900 | 1.009 | 1.001 | 1.019 | 0.992 | 0.976 | 0.964 | 0.951 | 0.963 | ok |
| corpus2/dev/bignum_arith | 5 | 789.9 | 725.5 | 879.4 | 821.0 | 660.2 | 2352.8 | 2412.4 | 1.196 | 1.099 | 1.019 | 1.011 | 1.010 | 1.010 | 1.024 | 1.010 | 1.023 | 1.008 | ok |
| corpus2/dev/coro_generators | 5 | 1253.4 | 960.1 | 1281.8 | 987.0 | 713.4 | 1807.9 | 1836.1 | 1.757 | 1.346 | 0.980 | 0.977 | 0.997 | 0.978 | 0.996 | 0.977 | 0.982 | 0.978 | ok |
| corpus2/dev/csv_report | 20 | 1143.2 | 1039.6 | 1200.2 | 1069.8 | 1262.6 | 2165.3 | 2381.4 | 0.905 | 0.823 | 1.021 | 1.000 | 1.025 | 0.985 | 1.026 | 1.007 | 1.002 | 1.003 | ok |
| corpus2/dev/diff_levenshtein | 20 | 437.5 | 406.3 | 516.2 | 404.0 | 329.3 | 1776.3 | 1850.8 | 1.328 | 1.234 | 1.030 | 1.014 | 1.016 | 1.000 | 1.162 | 1.012 | 0.978 | 1.003 | ok |
| corpus2/dev/ecs_world | 20 | 940.0 | 835.5 | 989.6 | 857.1 | 916.1 | 2115.3 | 2160.1 | 1.026 | 0.912 | 0.980 | 0.962 | 0.969 | 0.957 | 0.895 | 0.864 | 0.904 | 0.854 | ok |
| corpus2/dev/event_promise | 20 | 375.0 | 361.1 | 455.3 | 435.4 | 467.9 | 1906.6 | 1939.2 | 0.801 | 0.772 | 1.026 | 0.986 | 1.118 | 0.986 | 1.007 | 0.989 | 1.022 | 0.991 | ok |
| corpus2/dev/expr_compiler | 20 | 1289.9 | 1100.2 | 1348.4 | 1152.1 | 1080.2 | 2081.3 | 2231.0 | 1.194 | 1.018 | 0.945 | 0.938 | 0.945 | 0.941 | 0.936 | 0.937 | 0.943 | 0.933 | ok |
| corpus2/dev/iter_pipeline | 20 | 173.7 | 165.6 | 183.3 | 176.0 | 132.2 | 2865.2 | 2874.5 | 1.314 | 1.253 | 1.021 | 0.987 | 1.018 | 0.989 | 0.989 | 0.992 | 1.005 | 0.993 | ok |
| corpus2/dev/lua_parser | 5 | 1296.3 | 1132.3 | 1599.7 | 1369.8 | 790.7 | 2461.5 | 2499.0 | 1.640 | 1.432 | 1.009 | 0.980 | 1.010 | 0.978 | 1.006 | 0.983 | 1.010 | 0.979 | ok |
| corpus2/dev/memo_cache | 5 | 1677.5 | 1561.7 | 1700.2 | 1561.6 | 1674.7 | 2669.5 | 2763.8 | 1.002 | 0.933 | 0.972 | 0.921 | 0.954 | 0.909 | 1.002 | 0.955 | 0.997 | 0.944 | ok |
| corpus2/dev/oop_mixins | 20 | 1471.1 | 1392.9 | 1483.4 | 1360.2 | 865.1 | 2464.3 | 2520.8 | 1.701 | 1.610 | 0.884 | 0.857 | 0.892 | 0.856 | 0.885 | 0.860 | 0.860 | 0.852 | ok |
| corpus2/dev/particles_physics | 5 | 422.3 | 393.6 | 527.2 | 483.7 | 379.7 | 1719.5 | 1811.4 | 1.112 | 1.037 | 0.936 | 0.900 | 0.942 | 0.891 | 0.980 | 0.927 | 0.987 | 0.924 | ok |
| corpus2/dev/real_dis_arm | 20 | 2483.5 | 2183.4 | 2579.3 | 2237.5 | 1947.2 | 4487.3 | 4484.7 | 1.275 | 1.121 | 0.974 | 0.977 | 0.960 | 0.967 | 1.029 | 1.023 | 1.000 | 1.010 | ok |
| corpus2/dev/real_dis_x86 | 20 | 1534.4 | 1361.7 | 1635.9 | 1422.7 | 1591.6 | 2708.6 | 2688.6 | 0.964 | 0.856 | 0.951 | 0.951 | 0.990 | 0.942 | 1.025 | 0.996 | 1.003 | 0.991 | ok |
| corpus2/dev/regex_nfa | 20 | 691.7 | 551.3 | 1201.2 | 1037.1 | 554.5 | 2510.1 | 2439.3 | 1.247 | 0.994 | 1.057 | 0.990 | 0.993 | 0.997 | 0.992 | 1.001 | 1.003 | 0.996 | ok |
| corpus2/dev/template_gsub | 20 | 1690.4 | 1390.2 | 1825.3 | 1478.7 | 2051.0 | 2892.7 | 2885.0 | 0.824 | 0.678 | 0.992 | 0.994 | 0.996 | 0.982 | 1.023 | 0.995 | 0.981 | 0.991 | ok |
| corpus2/dev/text_adventure | 20 | 2571.9 | 2111.5 | 2748.0 | 2192.6 | 1186.6 | 3375.1 | 3513.0 | 2.168 | 1.779 | 0.993 | 0.995 | 0.983 | 0.996 | 1.020 | 0.999 | 0.982 | 0.996 | ok |
| corpus2/dev/varargs_util | 5 | 1520.8 | 1219.7 | 1550.2 | 1275.9 | 1413.9 | 2284.8 | 2301.9 | 1.076 | 0.863 | 1.004 | 0.997 | 0.979 | 0.998 | 0.995 | 1.002 | 1.003 | 0.997 | ok |
| corpus2/dev/wordfreq_wrap | 20 | 616.3 | 512.6 | 672.3 | 543.8 | 658.1 | 1219.2 | 1225.4 | 0.936 | 0.779 | 0.994 | 1.004 | 0.984 | 0.995 | 1.037 | 1.004 | 1.017 | 1.001 | ok |
| corpus2/heldout/bigint_rsa | 5 | 1118.9 | 943.3 | 1383.1 | 1183.1 | 1074.5 | 3033.1 | 3149.9 | 1.041 | 0.878 | 1.010 | 1.006 | 1.014 | 1.007 | 1.019 | 1.019 | 1.025 | 1.008 | ok |
| corpus2/heldout/dijkstra_routes | 20 | 1272.7 | 1241.4 | 1553.4 | 1507.6 | 1265.1 | 3142.9 | 3226.1 | 1.006 | 0.981 | 1.024 | 1.029 | 1.026 | 1.037 | 1.014 | 1.016 | 1.018 | 1.016 | ok |
| corpus2/heldout/forth_vm | 5 | 813.1 | 808.3 | 1054.1 | 1161.9 | 696.1 | 2965.2 | 3269.3 | 1.168 | 1.161 | 1.015 | 0.980 | 1.001 | 0.980 | 1.002 | 0.986 | 1.009 | 0.986 | ok |
| corpus2/heldout/log_analyzer | 20 | 2112.0 | 1757.7 | 2183.8 | 1846.6 | 1652.2 | 3115.0 | 3178.7 | 1.278 | 1.064 | 0.942 | 0.975 | 0.910 | 0.972 | 1.000 | 0.975 | 0.973 | 0.969 | ok |
| corpus2/heldout/markov_text | 5 | 1147.0 | 1112.4 | 1201.5 | 1158.6 | 982.0 | 2862.9 | 2851.2 | 1.168 | 1.133 | 1.023 | 0.996 | 1.003 | 1.002 | 1.007 | 0.987 | 1.003 | 0.993 | ok |
| corpus2/heldout/prototype_objects | 20 | 1511.3 | 1434.8 | 1523.8 | 1463.5 | 1280.6 | 2532.8 | 2721.4 | 1.180 | 1.120 | 1.003 | 0.911 | 0.994 | 0.914 | 0.947 | 0.858 | 0.976 | 0.860 | ok |
| corpus2/heldout/real_dis_mips | 20 | 2187.8 | 1874.3 | 2258.8 | 1893.8 | 1786.6 | 3808.7 | 3869.0 | 1.225 | 1.049 | 1.016 | 1.031 | 1.025 | 1.018 | 1.046 | 1.028 | 1.019 | 1.019 | ok |
| corpus2/heldout/real_dis_ppc | 20 | 2119.7 | 1832.3 | 2288.8 | 1924.0 | 1747.6 | 3961.9 | 3962.8 | 1.213 | 1.048 | 0.961 | 0.982 | 0.977 | 0.975 | 1.030 | 1.006 | 0.991 | 1.001 | ok |
| corpus2/heldout/roguelike_turns | 20 | 810.7 | 782.1 | 852.4 | 824.1 | 623.3 | 3311.7 | 3473.6 | 1.301 | 1.255 | 0.680 | 0.471 | 0.674 | 0.462 | 0.657 | 0.486 | 0.657 | 0.478 | ok |
| corpus2/heldout/scheme_interp | 5 | 1463.3 | 1244.5 | 1611.5 | 1355.2 | 1045.1 | 3052.1 | 3021.0 | 1.400 | 1.191 | 0.940 | 0.949 | 0.981 | 0.954 | 0.967 | 0.955 | 0.962 | 0.956 | ok |
| corpus2/heldout/stream_closures | 5 | 934.7 | 857.7 | 1031.9 | 941.7 | 416.0 | 3032.0 | 3163.1 | 2.247 | 2.062 | 0.976 | 0.965 | 0.992 | 0.959 | 0.995 | 0.972 | 0.988 | 0.969 | ok |
| corpus2/heldout/sudoku_solver | 5 | 1317.4 | 1174.4 | 1644.6 | 1501.9 | 1387.6 | 3013.0 | 3102.7 | 0.949 | 0.846 | 1.019 | 1.005 | 1.009 | 1.000 | 1.021 | 1.008 | 1.023 | 1.004 | ok |
| corpus2/heldout/vending_fsm | 5 | 1034.9 | 946.9 | 1120.7 | 1020.4 | 824.2 | 3290.3 | 3377.2 | 1.256 | 1.149 | 0.980 | 0.963 | 0.970 | 0.959 | 1.006 | 0.990 | 0.992 | 0.986 | ok |
| jit-trace/alloc_ctor | 20 | 187.0 | 158.3 | 254.0 | 203.3 | 276.5 | 1652.1 | 2197.8 | 0.676 | 0.572 | 0.917 | 0.932 | 0.980 | 0.994 | 0.917 | 0.989 | 0.996 | 0.984 | ok |
| jit-trace/array_rec | 2 | 1796.5 | 1796.2 | 1822.2 | 1821.6 | 1755.9 | 9671.5 | 10022.8 | 1.023 | 1.023 | 1.001 | 1.000 | 1.001 | 1.000 | 1.001 | 1.000 | 1.001 | 1.000 | ok |
| jit-trace/dot | 2 | 1223.1 | 1224.2 | 1230.4 | 1228.6 | 1166.0 | 11124.9 | 11807.8 | 1.049 | 1.050 | 1.004 | 1.000 | 1.005 | 1.000 | 1.005 | 1.000 | 1.006 | 1.000 | ok |
| jit-trace/field_global | 2 | 545.6 | 541.5 | 547.2 | 542.5 | 491.7 | 17948.4 | 17879.3 | 1.110 | 1.101 | 1.009 | 1.000 | 1.007 | 1.000 | 1.005 | 1.000 | 1.004 | 1.001 | ok |
| jit-trace/field_method | 2 | 570.4 | 564.9 | 572.0 | 566.3 | 423.1 | 20185.1 | 20271.4 | 1.348 | 1.335 | 1.004 | 1.000 | 1.007 | 1.000 | 1.005 | 1.000 | 1.004 | 1.001 | ok |
| jit-trace/field_rw | 2 | 949.7 | 948.4 | 1037.0 | 1035.3 | 929.3 | 19574.6 | 19882.6 | 1.022 | 1.021 | 1.002 | 1.000 | 1.001 | 1.000 | 1.002 | 1.000 | 1.003 | 1.001 | ok |
| jit-trace/float_loop | 2 | 2778.2 | 2775.3 | 2797.6 | 2798.5 | 2575.9 | 11381.5 | 13567.0 | 1.079 | 1.077 | 1.001 | 1.000 | 1.000 | 1.000 | 1.001 | 1.000 | 1.003 | 1.000 | ok |
| jit-trace/float_poly | 2 | 1784.1 | 1783.8 | 1822.1 | 1819.5 | 1768.0 | 7918.6 | 8940.5 | 1.009 | 1.009 | 1.001 | 1.000 | 1.001 | 1.000 | 1.000 | 1.000 | 1.000 | 1.002 | ok |
| jit-trace/inline_2 | 17 | 324.4 | 323.3 | 324.0 | 324.0 | 262.5 | 11057.3 | 10600.9 | 1.236 | 1.232 | 1.002 | 1.000 | 1.000 | 1.000 | 1.001 | 1.000 | 1.003 | 1.000 | ok |
| jit-trace/inline_leaf | 2 | 457.1 | 456.1 | 457.8 | 456.1 | 630.2 | 11902.9 | 12019.3 | 0.725 | 0.724 | 1.006 | 1.000 | 1.006 | 1.000 | 1.007 | 1.000 | 1.002 | 1.000 | ok |
| jit-trace/int_rec2 | 5 | 680.5 | 677.9 | 687.8 | 693.4 | 5016.4 | 12493.6 | 13006.3 | 0.136 | 0.135 | 1.000 | 1.000 | 1.001 | 1.000 | 0.986 | 1.000 | 1.006 | 1.000 | ok |
| jit-trace/int_sum | 2 | 888.9 | 890.7 | 909.2 | 909.9 | 2512.8 | 9858.5 | 12300.9 | 0.354 | 0.354 | 1.001 | 1.000 | 1.003 | 1.000 | 1.001 | 1.000 | 1.000 | 1.000 | ok |
| jit-trace/licm_expr | 2 | 467.4 | 466.9 | 467.5 | 467.2 | 442.6 | 12323.9 | 12827.3 | 1.056 | 1.055 | 1.004 | 1.000 | 1.002 | 1.000 | 1.002 | 1.000 | 1.001 | 1.000 | ok |
| jit-trace/multret | 2 | 150.7 | 150.1 | 150.9 | 150.1 | 156.7 | 5879.7 | 5782.8 | 0.961 | 0.958 | 1.004 | 1.000 | 1.005 | 1.000 | 1.005 | 1.000 | 1.004 | 1.000 | ok |
| jit-trace/oop_inherit | 2 | 57.6 | 57.2 | 49.2 | 48.8 | 112.8 | 2153.1 | 2201.0 | 0.511 | 0.507 | 1.002 | 1.000 | 1.002 | 1.000 | 1.007 | 1.000 | 1.005 | 1.000 | ok |
| jit-trace/oop_override | 2 | 108.0 | 108.1 | 112.1 | 111.1 | 136.7 | 645.6 | 666.3 | 0.790 | 0.791 | 1.009 | 1.000 | 1.014 | 0.999 | 1.007 | 1.000 | 1.005 | 0.999 | ok |
| jit-trace/rec_loop | 20 | 81.5 | 80.5 | 79.3 | 78.2 | 141.9 | 1440.3 | 1449.9 | 0.574 | 0.567 | 0.915 | 0.877 | 0.912 | 0.877 | 0.902 | 0.878 | 0.900 | 0.878 | ok |
| jit-trace/short_inner | 5 | 1095.7 | 1098.0 | 995.7 | 994.5 | 878.8 | 10393.4 | 11908.6 | 1.247 | 1.249 | 1.001 | 0.989 | 1.002 | 0.989 | 1.000 | 0.991 | 1.000 | 0.991 | ok |
| jit-trace/sink_tuple | 5 | 13.4 | 13.0 | 13.4 | 13.0 | 20.0 | 2743.7 | 3358.1 | 0.669 | 0.650 | 1.034 | 0.993 | 1.023 | 0.993 | 1.022 | 0.994 | 1.004 | 0.994 | ok |
| jit-trace/sink_vec | 5 | 15.3 | 15.1 | 15.5 | 15.1 | 20.0 | 1489.0 | 1832.3 | 0.767 | 0.757 | 1.030 | 0.998 | 1.029 | 0.997 | 1.022 | 0.998 | 1.027 | 0.998 | ok |
| jit-trace/str_concat | 5 | 532.5 | 507.4 | 602.0 | 555.8 | 501.6 | 1628.5 | 1630.2 | 1.062 | 1.012 | 0.983 | 1.015 | 0.995 | 0.999 | 1.016 | 1.020 | 1.008 | 0.998 | ok |
| jit-trace/tail_loop | 5 | 247.3 | 247.3 | 251.3 | 251.0 | 223.3 | 6724.0 | 6643.8 | 1.107 | 1.107 | 1.006 | 0.963 | 1.005 | 0.963 | 1.005 | 0.963 | 1.015 | 0.963 | ok |
| jit-trace/tfor_loops | 2 | 219.2 | 218.5 | 227.4 | 227.1 | 362.3 | 3623.0 | 3352.6 | 0.605 | 0.603 | 1.006 | 1.000 | 0.999 | 1.000 | 0.994 | 1.000 | 0.989 | 1.000 | ok |
| jit-trace/type_entry | 2 | 1513.2 | 1514.5 | 1581.3 | 1578.7 | 848.7 | 11725.8 | 12234.3 | 1.783 | 1.785 | 1.002 | 0.997 | 1.001 | 0.997 | 1.013 | 0.998 | 0.998 | 0.998 | ok |
| jit-trace/type_entry_stable | 2 | 1453.6 | 1458.7 | 1528.9 | 1528.6 | 856.9 | 11690.0 | 12248.4 | 1.696 | 1.702 | 1.003 | 0.998 | 1.008 | 0.998 | 1.007 | 0.998 | 1.020 | 0.998 | ok |
| jit-trace/type_late | 2 | 1528.3 | 1528.8 | 1540.9 | 1542.2 | 1461.5 | 10901.2 | 11418.6 | 1.046 | 1.046 | 1.005 | 1.000 | 1.006 | 1.000 | 1.011 | 1.000 | 1.009 | 1.000 | ok |
| jit-trace/type_late_stable | 2 | 1530.0 | 1531.4 | 1539.8 | 1540.0 | 1461.0 | 10905.6 | 11410.8 | 1.047 | 1.048 | 1.006 | 1.000 | 1.007 | 1.000 | 1.006 | 1.000 | 1.005 | 1.000 | ok |
| jit-trace/upval_acc | 2 | 1393.3 | 1392.8 | 1396.1 | 1396.4 | 1356.5 | 9970.0 | 10500.8 | 1.027 | 1.027 | 1.008 | 1.000 | 1.008 | 1.000 | 1.000 | 1.000 | 1.001 | 1.000 | ok |
| jit-trace/upval_inv | 2 | 2144.6 | 2148.6 | 2154.8 | 2148.4 | 1985.3 | 13886.5 | 14901.3 | 1.080 | 1.082 | 1.005 | 1.000 | 1.007 | 1.000 | 1.007 | 1.000 | 1.004 | 1.000 | ok |
| lib/assert_ok | 20 | 19.0 | 18.7 | 20.4 | 20.1 | 23.7 | 295.9 | 329.8 | 0.803 | 0.789 | 0.999 | 0.999 | 0.998 | 0.999 | 1.009 | 0.999 | 1.002 | 0.999 | ok |
| lib/byte | 2 | 51.0 | 50.5 | 51.8 | 51.5 | 33.0 | 776.0 | 845.1 | 1.544 | 1.530 | 1.016 | 1.000 | 1.015 | 1.000 | 1.019 | 1.000 | 1.009 | 1.000 | ok |
| lib/byte_multi | 17 | 40.7 | 40.2 | 40.9 | 40.8 | 20.1 | 336.9 | 366.2 | 2.028 | 2.003 | 1.004 | 1.000 | 1.003 | 0.999 | 0.985 | 1.000 | 1.000 | 0.999 | ok |
| lib/char | 5 | 153.6 | 139.8 | 157.4 | 145.1 | 104.8 | 383.6 | 387.1 | 1.466 | 1.334 | 1.013 | 0.990 | 1.011 | 1.000 | 1.018 | 0.991 | 1.006 | 1.000 | ok |
| lib/co_wrap | 5 | 247.5 | 201.8 | 234.3 | 199.0 | 228.7 | 381.5 | 338.9 | 1.082 | 0.883 | 0.999 | 0.982 | 0.960 | 1.001 | 0.974 | 0.978 | 0.994 | 1.012 | ok |
| lib/co_yield | 17 | 240.2 | 181.6 | 242.9 | 179.9 | 102.0 | 483.9 | 475.0 | 2.355 | 1.780 | 1.007 | 1.000 | 1.013 | 1.000 | 1.046 | 1.000 | 0.954 | 1.002 | ok |
| lib/concat_op | 20 | 139.8 | 127.8 | 144.4 | 131.9 | 212.9 | 419.4 | 445.3 | 0.657 | 0.600 | 0.992 | 1.021 | 0.993 | 0.998 | 1.000 | 1.019 | 0.989 | 0.995 | ok |
| lib/concat_op_str | 5 | 140.3 | 120.3 | 241.4 | 229.5 | 276.8 | 436.1 | 336.3 | 0.507 | 0.435 | 1.005 | 1.004 | 0.962 | 0.978 | 0.992 | 1.002 | 1.019 | 1.003 | ok |
| lib/error_tbl | 5 | 269.6 | 217.5 | 261.7 | 213.6 | 3042.9 | 619.9 | 625.1 | 0.089 | 0.071 | 0.985 | 0.979 | 0.981 | 1.005 | 0.995 | 0.979 | 1.005 | 1.003 | ok |
| lib/find_cap | 20 | 274.0 | 233.7 | 270.3 | 227.2 | 183.4 | 365.9 | 363.4 | 1.494 | 1.274 | 1.006 | 0.999 | 0.999 | 0.987 | 0.988 | 0.999 | 0.976 | 0.999 | ok |
| lib/find_pat | 20 | 258.7 | 234.5 | 279.5 | 236.0 | 705.2 | 1094.0 | 1092.2 | 0.367 | 0.333 | 0.951 | 1.000 | 1.018 | 0.988 | 0.997 | 1.000 | 1.007 | 1.000 | ok |
| lib/find_plain | 20 | 127.7 | 115.8 | 126.6 | 114.8 | 67.6 | 342.8 | 363.4 | 1.889 | 1.714 | 1.000 | 1.000 | 0.997 | 1.000 | 1.002 | 1.000 | 0.995 | 0.989 | ok |
| lib/fmt_d | 20 | 93.6 | 89.0 | 100.6 | 94.3 | 126.3 | 278.1 | 235.1 | 0.741 | 0.704 | 1.000 | 1.023 | 0.992 | 1.000 | 0.996 | 1.021 | 0.995 | 0.989 | ok |
| lib/fmt_f | 20 | 62.7 | 56.8 | 65.2 | 58.4 | 86.8 | 215.0 | 196.2 | 0.723 | 0.655 | 0.996 | 1.017 | 0.982 | 1.000 | 1.015 | 1.016 | 0.986 | 0.997 | ok |
| lib/fmt_g | 20 | 73.4 | 61.8 | 75.7 | 62.9 | 67.3 | 204.6 | 179.3 | 1.090 | 0.918 | 1.011 | 1.015 | 0.933 | 1.000 | 1.007 | 1.014 | 0.992 | 0.997 | ok |
| lib/fmt_mixed | 5 | 59.7 | 49.7 | 66.4 | 56.5 | 106.2 | 254.5 | 238.3 | 0.562 | 0.468 | 1.019 | 1.010 | 0.999 | 0.999 | 1.023 | 1.009 | 1.016 | 0.997 | ok |
| lib/fmt_q | 17 | 145.6 | 119.8 | 144.7 | 119.2 | 103.3 | 238.6 | 226.1 | 1.410 | 1.160 | 0.892 | 1.000 | 0.986 | 1.000 | 0.997 | 1.000 | 0.988 | 1.000 | ok |
| lib/fmt_s | 5 | 7.4 | 7.1 | 7.5 | 6.9 | 3.2 | 217.6 | 222.2 | 2.344 | 2.233 | 1.063 | 0.997 | 1.036 | 0.996 | 1.035 | 0.997 | 0.978 | 0.996 | ok |
| lib/fmt_x_pad | 20 | 56.2 | 48.1 | 59.2 | 50.1 | 54.0 | 177.1 | 177.5 | 1.041 | 0.890 | 0.991 | 1.006 | 0.975 | 0.994 | 0.997 | 1.011 | 0.987 | 0.992 | ok |
| lib/getmt | 5 | 16.1 | 15.8 | 20.1 | 19.8 | 8.8 | 255.7 | 264.8 | 1.822 | 1.790 | 1.032 | 0.999 | 1.046 | 0.999 | 1.016 | 0.999 | 1.021 | 0.999 | ok |
| lib/gmatch_words | 20 | 175.4 | 136.8 | 187.4 | 136.1 | 173.3 | 315.9 | 319.8 | 1.012 | 0.790 | 0.977 | 1.001 | 1.001 | 0.998 | 1.041 | 1.001 | 0.985 | 0.999 | ok |
| lib/gsub_fn | 17 | 173.9 | 128.7 | 226.6 | 130.7 | 200.2 | 323.4 | 312.5 | 0.868 | 0.643 | 0.994 | 1.001 | 0.997 | 1.001 | 1.260 | 1.002 | 0.994 | 1.000 | ok |
| lib/gsub_str | 5 | 167.4 | 128.2 | 164.3 | 126.8 | 398.5 | 491.6 | 515.6 | 0.420 | 0.322 | 1.023 | 1.000 | 1.007 | 0.997 | 1.005 | 1.000 | 1.004 | 1.000 | ok |
| lib/gsub_tbl | 5 | 164.6 | 106.9 | 155.5 | 109.7 | 187.9 | 321.9 | 321.7 | 0.876 | 0.569 | 1.007 | 0.999 | 0.980 | 0.990 | 1.015 | 0.999 | 0.979 | 0.990 | ok |
| lib/it_ipairs | 20 | 237.6 | 233.8 | 280.5 | 259.2 | 251.6 | 5628.0 | 4827.6 | 0.945 | 0.929 | 1.002 | 1.000 | 0.983 | 0.994 | 1.036 | 0.997 | 1.017 | 0.997 | ok |
| lib/it_next | 5 | 45.1 | 45.0 | 45.6 | 44.8 | 180.5 | 371.1 | 401.4 | 0.250 | 0.249 | 1.025 | 0.997 | 1.011 | 0.997 | 1.030 | 0.997 | 1.028 | 0.997 | ok |
| lib/it_next_empty | 5 | 51.9 | 51.6 | 51.9 | 51.6 | 72.5 | 393.0 | 455.4 | 0.716 | 0.712 | 1.006 | 1.000 | 1.009 | 1.000 | 1.003 | 1.000 | 1.002 | 1.000 | ok |
| lib/it_pairs | 20 | 86.7 | 87.8 | 94.5 | 90.9 | 127.9 | 687.0 | 646.3 | 0.678 | 0.687 | 1.004 | 0.998 | 0.999 | 0.998 | 1.006 | 0.998 | 1.009 | 0.998 | ok |
| lib/it_pairs_hash | 2 | 106.6 | 108.1 | 116.1 | 110.4 | 118.1 | 989.3 | 957.1 | 0.903 | 0.915 | 1.018 | 1.000 | 1.030 | 0.999 | 1.011 | 1.000 | 1.004 | 1.000 | ok |
| lib/len_op | 5 | 48.9 | 48.1 | 58.2 | 58.1 | 100.7 | 1384.7 | 1419.8 | 0.486 | 0.477 | 1.008 | 1.000 | 0.995 | 1.000 | 1.015 | 1.000 | 1.017 | 1.000 | ok |
| lib/len_tbl | 5 | 137.5 | 138.4 | 158.9 | 158.4 | 159.7 | 372.6 | 337.9 | 0.861 | 0.866 | 1.000 | 1.000 | 1.026 | 1.000 | 1.010 | 1.000 | 1.011 | 1.000 | ok |
| lib/load_small | 20 | 240.7 | 170.8 | 216.9 | 166.2 | 215.6 | 378.8 | 345.2 | 1.116 | 0.792 | 1.054 | 0.972 | 1.090 | 0.982 | 0.926 | 0.987 | 0.873 | 0.996 | ok |
| lib/lower | 2 | 89.4 | 81.4 | 90.2 | 83.2 | 110.1 | 289.0 | 294.8 | 0.812 | 0.739 | 1.009 | 1.002 | 1.010 | 1.004 | 1.007 | 1.002 | 1.011 | 1.004 | ok |
| lib/m_abs | 17 | 241.4 | 241.3 | 241.4 | 241.5 | 390.3 | 6061.9 | 6506.8 | 0.618 | 0.618 | 0.993 | 1.000 | 0.995 | 1.000 | 0.999 | 1.000 | 1.004 | 1.000 | ok |
| lib/m_floor | 2 | 227.1 | 226.7 | 227.6 | 227.2 | 206.5 | 3681.1 | 4104.1 | 1.099 | 1.098 | 1.006 | 1.000 | 1.005 | 1.000 | 1.006 | 1.000 | 1.004 | 1.000 | ok |
| lib/m_fmod | 5 | 219.7 | 220.3 | 223.0 | 229.1 | 235.1 | 329.2 | 361.2 | 0.935 | 0.937 | 0.998 | 1.000 | 1.014 | 1.000 | 1.011 | 1.000 | 1.012 | 1.000 | ok |
| lib/m_max | 2 | 258.2 | 257.7 | 259.2 | 259.4 | 401.3 | 8004.8 | 10137.5 | 0.643 | 0.642 | 1.013 | 1.000 | 1.011 | 1.000 | 1.013 | 1.000 | 1.017 | 1.000 | ok |
| lib/m_min | 20 | 223.6 | 223.8 | 223.2 | 229.2 | 387.7 | 6530.7 | 8066.5 | 0.577 | 0.577 | 0.997 | 1.000 | 0.995 | 1.000 | 1.008 | 1.000 | 0.991 | 1.000 | ok |
| lib/m_pow_op | 2 | 280.2 | 280.4 | 280.8 | 280.1 | 275.2 | 1923.5 | 1956.0 | 1.018 | 1.019 | 1.000 | 1.000 | 1.002 | 1.000 | 1.003 | 1.000 | 1.001 | 1.000 | ok |
| lib/m_random | 2 | 79.4 | 79.3 | 79.5 | 79.1 | 73.2 | 345.7 | 375.3 | 1.084 | 1.083 | 1.010 | 1.000 | 1.016 | 1.000 | 1.012 | 1.000 | 1.009 | 1.000 | ok |
| lib/m_random_f | 2 | 53.6 | 53.1 | 53.5 | 53.1 | 89.9 | 272.7 | 312.0 | 0.595 | 0.590 | 1.019 | 1.000 | 1.015 | 1.000 | 1.014 | 1.000 | 1.014 | 1.000 | ok |
| lib/m_sin | 2 | 272.9 | 262.0 | 273.7 | 261.8 | 210.5 | 563.9 | 589.3 | 1.296 | 1.245 | 1.003 | 1.000 | 1.007 | 1.000 | 1.008 | 1.000 | 1.006 | 1.000 | ok |
| lib/m_sqrt | 17 | 272.8 | 271.1 | 277.4 | 278.9 | 270.0 | 2022.6 | 2132.3 | 1.011 | 1.004 | 0.991 | 1.000 | 0.983 | 1.000 | 1.017 | 1.000 | 1.024 | 1.000 | ok |
| lib/m_tointeger | 2 | 224.9 | 224.9 | 238.5 | 238.1 | - | 3718.4 | 3972.3 | - | - | 1.010 | 1.000 | 1.011 | 1.000 | 1.008 | 1.000 | 1.008 | 1.000 | ok |
| lib/m_type | 2 | 32.5 | 32.1 | 32.5 | 32.1 | - | 419.3 | 429.1 | - | - | 1.023 | 1.000 | 1.021 | 0.999 | 1.022 | 0.999 | 1.019 | 0.999 | ok |
| lib/m_ult | 20 | 271.3 | 280.3 | 271.9 | 282.3 | - | 415.2 | 435.8 | - | - | 0.995 | 1.000 | 0.996 | 1.000 | 0.994 | 1.000 | 0.990 | 1.000 | ok |
| lib/match | 20 | 61.2 | 51.2 | 61.8 | 51.6 | 193.6 | 319.9 | 303.8 | 0.316 | 0.264 | 1.001 | 1.000 | 1.000 | 0.987 | 1.018 | 1.000 | 1.008 | 0.996 | ok |
| lib/os_clock | 17 | 115.5 | 121.4 | 119.6 | 120.9 | 100.7 | 119.3 | 118.5 | 1.147 | 1.206 | 1.012 | 1.000 | 1.001 | 1.000 | 1.003 | 1.000 | 0.996 | 1.000 | ok |
| lib/pcall_err | 20 | 119.4 | 98.7 | 133.8 | 112.1 | 764.5 | 308.4 | 309.0 | 0.156 | 0.129 | 1.020 | 0.998 | 0.996 | 1.005 | 1.050 | 0.999 | 1.026 | 1.003 | ok |
| lib/pcall_ok | 17 | 29.8 | 29.4 | 29.7 | 29.4 | 41.0 | 1524.0 | 1518.3 | 0.727 | 0.718 | 1.002 | 1.000 | 1.004 | 0.999 | 0.994 | 1.000 | 0.997 | 0.999 | ok |
| lib/rawget_set | 2 | 19.5 | 18.9 | 19.3 | 19.1 | 14.2 | 381.1 | 417.2 | 1.366 | 1.329 | 1.036 | 0.999 | 1.024 | 0.999 | 1.012 | 0.999 | 1.025 | 0.999 | ok |
| lib/rep | 5 | 109.6 | 100.2 | 119.8 | 110.1 | 89.6 | 347.4 | 333.3 | 1.223 | 1.118 | 1.019 | 1.009 | 1.010 | 0.994 | 1.003 | 1.006 | 0.995 | 0.988 | ok |
| lib/rep_sep | 5 | 88.9 | 81.3 | 118.2 | 110.3 | 103.8 | 329.6 | 311.2 | 0.857 | 0.784 | 1.000 | 1.014 | 0.998 | 0.993 | 1.012 | 1.009 | 1.017 | 0.998 | ok |
| lib/reverse | 2 | 99.8 | 94.2 | 104.9 | 99.7 | 115.3 | 309.8 | 314.7 | 0.865 | 0.817 | 1.007 | 1.002 | 1.011 | 1.004 | 1.011 | 1.002 | 1.010 | 1.004 | ok |
| lib/sel_count | 5 | 218.4 | 218.2 | 217.8 | 218.5 | 388.2 | 15509.5 | 14438.4 | 0.563 | 0.562 | 1.006 | 0.999 | 1.008 | 0.999 | 1.009 | 0.999 | 1.014 | 0.999 | ok |
| lib/sel_loop | 20 | 209.7 | 208.2 | 216.6 | 215.2 | 5263.6 | 53689.2 | 54662.9 | 0.040 | 0.040 | 0.997 | 0.998 | 0.986 | 0.998 | 0.908 | 0.998 | 0.897 | 0.998 | ok |
| lib/sel_n | 20 | 320.4 | 320.0 | 321.5 | 321.8 | 675.4 | 25345.4 | 23674.6 | 0.474 | 0.474 | 0.973 | 0.999 | 0.985 | 0.999 | 1.005 | 0.999 | 1.004 | 0.999 | ok |
| lib/setmt | 20 | 183.7 | 160.4 | 225.4 | 187.2 | 331.9 | 743.5 | 835.9 | 0.553 | 0.483 | 0.897 | 0.956 | 0.980 | 0.959 | 0.910 | 0.966 | 0.964 | 0.969 | ok |
| lib/str_eq | 2 | 160.7 | 160.4 | 161.3 | 160.7 | 182.5 | 1479.5 | 1529.4 | 0.881 | 0.879 | 1.008 | 1.000 | 1.008 | 1.000 | 1.009 | 1.000 | 1.008 | 1.000 | ok |
| lib/str_eq_dyn | 2 | 135.6 | 135.3 | 135.9 | 136.3 | 150.4 | 1277.3 | 1348.8 | 0.901 | 0.900 | 1.017 | 1.000 | 1.002 | 1.000 | 1.017 | 1.000 | 1.023 | 1.000 | ok |
| lib/str_eq_long | 20 | 35.6 | 35.3 | 35.8 | 34.9 | 21.5 | 215.5 | 222.1 | 1.659 | 1.644 | 1.012 | 1.000 | 1.006 | 1.000 | 1.014 | 1.000 | 0.997 | 1.000 | ok |
| lib/str_idx_concat_key | 2 | 207.5 | 191.8 | 208.2 | 195.5 | 166.6 | 464.0 | 461.2 | 1.246 | 1.151 | 1.015 | 1.002 | 1.009 | 1.000 | 1.016 | 1.002 | 1.016 | 1.000 | ok |
| lib/str_lt | 2 | 94.9 | 92.1 | 90.6 | 87.5 | 66.3 | 227.1 | 228.3 | 1.431 | 1.389 | 1.011 | 1.000 | 1.015 | 1.000 | 1.015 | 1.000 | 1.013 | 1.000 | ok |
| lib/str_lt_short | 5 | 114.0 | 106.4 | 105.6 | 98.9 | 62.5 | 249.8 | 255.3 | 1.824 | 1.702 | 1.012 | 1.000 | 1.008 | 1.000 | 1.011 | 1.000 | 1.007 | 1.000 | ok |
| lib/str_method_len | 20 | 45.2 | 44.9 | 51.1 | 50.4 | 55.3 | 1386.6 | 1429.7 | 0.818 | 0.812 | 0.994 | 1.000 | 1.003 | 1.000 | 1.003 | 1.000 | 0.994 | 1.000 | ok |
| lib/string_key_const | 2 | 178.2 | 178.2 | 201.9 | 201.5 | 176.9 | 1151.6 | 1165.1 | 1.008 | 1.007 | 1.009 | 1.000 | 1.010 | 1.000 | 1.005 | 1.000 | 1.005 | 1.000 | ok |
| lib/string_key_dyn | 2 | 229.5 | 229.0 | 228.3 | 228.1 | 268.0 | 1082.3 | 1101.0 | 0.856 | 0.854 | 1.008 | 1.000 | 1.006 | 1.000 | 1.003 | 1.000 | 1.006 | 1.000 | ok |
| lib/sub | 2 | 132.0 | 123.5 | 131.5 | 121.8 | 80.5 | 469.5 | 482.7 | 1.640 | 1.534 | 1.013 | 0.996 | 1.014 | 1.000 | 1.010 | 0.996 | 1.009 | 1.000 | ok |
| lib/sub_1char | 20 | 118.4 | 108.8 | 120.5 | 107.9 | 98.2 | 459.2 | 478.8 | 1.206 | 1.108 | 0.997 | 0.984 | 0.999 | 1.000 | 0.999 | 0.985 | 0.998 | 1.000 | ok |
| lib/tconcat | 20 | 147.7 | 100.0 | 171.1 | 119.6 | 133.4 | 346.9 | 322.3 | 1.107 | 0.750 | 1.047 | 1.007 | 0.983 | 1.001 | 1.023 | 1.003 | 0.992 | 1.003 | ok |
| lib/tconcat_num | 2 | 175.1 | 118.1 | 180.5 | 121.8 | 246.0 | 908.0 | 844.8 | 0.712 | 0.480 | 1.021 | 1.004 | 1.003 | 0.999 | 1.012 | 1.004 | 1.011 | 0.998 | ok |
| lib/tfront_1k | 5 | 79.3 | 76.5 | 80.8 | 76.9 | 406.3 | 4375.7 | 4241.0 | 0.195 | 0.188 | 0.975 | 1.006 | 0.966 | 1.001 | 1.010 | 1.000 | 1.012 | 1.002 | ok |
| lib/tinsert_append | 20 | 156.8 | 133.7 | 190.9 | 182.0 | 141.7 | 539.7 | 574.7 | 1.107 | 0.943 | 0.996 | 1.007 | 0.990 | 0.999 | 1.003 | 1.030 | 1.029 | 1.025 | ok |
| lib/tinsert_front | 20 | 222.7 | 177.9 | 260.9 | 211.8 | 272.9 | 455.4 | 493.5 | 0.816 | 0.652 | 1.007 | 1.027 | 1.016 | 1.006 | 0.994 | 1.016 | 1.017 | 1.011 | ok |
| lib/tinsert_idx | 20 | 174.1 | 151.2 | 201.0 | 189.7 | 154.6 | 328.8 | 383.3 | 1.127 | 0.978 | 1.058 | 1.034 | 1.024 | 1.028 | 1.014 | 1.032 | 1.023 | 1.026 | ok |
| lib/tmove | 2 | 103.8 | 103.3 | 125.1 | 124.8 | 105.8 | 700.5 | 683.1 | 0.982 | 0.977 | 1.017 | 1.000 | 1.015 | 1.000 | 1.011 | 1.000 | 1.011 | 1.000 | ok |
| lib/tmove_1k | 20 | 96.9 | 68.1 | 103.9 | 70.6 | 111.6 | 476.4 | 447.3 | 0.868 | 0.610 | 1.018 | 1.005 | 1.006 | 1.002 | 1.007 | 1.001 | 1.012 | 1.004 | ok |
| lib/tonumber_bad | 20 | 150.0 | 111.6 | 165.2 | 142.4 | 168.0 | 408.1 | 431.1 | 0.893 | 0.665 | 0.998 | 1.010 | 0.969 | 1.004 | 1.204 | 1.010 | 1.002 | 1.000 | ok |
| lib/tonumber_base | 5 | 266.2 | 261.1 | 274.6 | 262.4 | 229.2 | 432.1 | 451.7 | 1.161 | 1.139 | 1.008 | 1.000 | 1.003 | 1.000 | 1.016 | 1.000 | 1.009 | 1.000 | ok |
| lib/tonumber_flt | 17 | 196.2 | 170.4 | 200.9 | 164.5 | 776.7 | 577.4 | 596.1 | 0.253 | 0.219 | 0.986 | 1.005 | 0.999 | 1.002 | 1.033 | 1.005 | 1.004 | 1.000 | ok |
| lib/tonumber_int | 20 | 154.6 | 139.6 | 160.6 | 142.1 | 96.2 | 288.1 | 305.0 | 1.608 | 1.452 | 1.138 | 1.013 | 1.004 | 1.000 | 1.102 | 1.012 | 1.003 | 1.000 | ok |
| lib/tonumber_num | 20 | 214.4 | 211.3 | 208.9 | 214.1 | 383.1 | 6476.7 | 7506.3 | 0.560 | 0.552 | 0.976 | 0.999 | 0.982 | 0.999 | 0.964 | 0.999 | 1.087 | 0.999 | ok |
| lib/tostring_flt | 5 | 83.0 | 65.1 | 84.7 | 61.1 | 82.5 | 299.3 | 214.9 | 1.006 | 0.789 | 1.003 | 1.012 | 0.991 | 0.997 | 1.010 | 1.012 | 0.993 | 0.996 | ok |
| lib/tostring_int | 20 | 121.0 | 108.3 | 138.0 | 120.5 | 279.8 | 543.3 | 450.7 | 0.432 | 0.387 | 1.044 | 1.029 | 0.987 | 0.993 | 1.067 | 1.026 | 0.974 | 0.993 | ok |
| lib/tostring_tbl | 5 | 109.5 | 105.5 | 113.1 | 108.5 | 93.3 | 313.8 | 280.9 | 1.173 | 1.131 | 1.008 | 0.999 | 0.991 | 1.001 | 1.012 | 0.999 | 1.007 | 1.000 | ok |
| lib/tpack | 5 | 148.6 | 113.2 | 211.2 | 172.3 | 306.1 | 499.0 | 485.7 | 0.485 | 0.370 | 0.965 | 0.987 | 0.960 | 0.963 | 0.965 | 0.991 | 0.996 | 0.990 | ok |
| lib/tremove_front | 5 | 251.7 | 221.7 | 285.1 | 229.8 | 191.0 | 592.3 | 593.7 | 1.318 | 1.161 | 0.982 | 1.013 | 1.025 | 1.006 | 1.013 | 1.009 | 1.011 | 1.010 | ok |
| lib/tremove_pop | 5 | 241.3 | 174.5 | 222.4 | 206.8 | 136.7 | 483.9 | 552.9 | 1.765 | 1.276 | 1.013 | 1.016 | 1.007 | 1.012 | 0.981 | 1.006 | 1.013 | 1.008 | ok |
| lib/tsort_cmp | 5 | 270.7 | 256.5 | 275.4 | 257.8 | 285.5 | 339.9 | 336.1 | 0.948 | 0.898 | 0.995 | 1.007 | 1.019 | 1.001 | 0.999 | 1.001 | 1.019 | 1.005 | ok |
| lib/tsort_int | 20 | 276.8 | 218.1 | 287.5 | 220.9 | 294.0 | 382.4 | 406.0 | 0.941 | 0.742 | 1.068 | 1.015 | 0.992 | 1.009 | 0.998 | 1.002 | 0.997 | 1.014 | ok |
| lib/tsort_str | 20 | 260.9 | 152.9 | 291.7 | 157.9 | 244.8 | 364.6 | 401.0 | 1.066 | 0.625 | 1.038 | 1.011 | 1.018 | 1.007 | 1.007 | 0.998 | 1.005 | 1.009 | ok |
| lib/tunpack | 20 | 117.5 | 103.8 | 127.0 | 111.7 | 154.8 | 487.1 | 448.5 | 0.759 | 0.670 | 0.904 | 0.996 | 0.997 | 0.991 | 0.912 | 0.995 | 0.998 | 0.989 | ok |
| lib/type_call | 20 | 86.7 | 86.5 | 93.8 | 93.4 | 85.6 | 494.3 | 468.3 | 1.013 | 1.011 | 0.987 | 1.000 | 0.986 | 1.000 | 1.002 | 1.000 | 1.006 | 0.999 | ok |
| lib/upper | 2 | 95.2 | 88.8 | 101.4 | 95.7 | 120.7 | 307.4 | 312.6 | 0.789 | 0.736 | 0.995 | 1.002 | 0.987 | 1.004 | 1.022 | 1.002 | 1.014 | 1.004 | ok |
| lib/utf8_len | 2 | 271.9 | 277.2 | 274.8 | 281.7 | - | 442.2 | 456.7 | - | - | 1.011 | 1.000 | 1.009 | 1.000 | 0.993 | 1.000 | 1.010 | 1.000 | ok |
| programs/binary_trees | 20 | 1508.7 | 1298.4 | 1778.0 | 1469.1 | 2007.4 | 4389.1 | 4447.3 | 0.752 | 0.647 | 0.783 | 0.745 | 0.820 | 0.763 | 0.808 | 0.779 | 0.809 | 0.775 | ok |
| programs/closures | 5 | 464.3 | 424.1 | 517.4 | 481.0 | 949.8 | 5063.2 | 4986.1 | 0.489 | 0.447 | 0.958 | 0.997 | 0.993 | 0.984 | 0.967 | 0.994 | 0.998 | 0.980 | ok |
| programs/coroutines | 17 | 1801.8 | 1302.0 | 1842.3 | 1392.3 | 871.7 | 3733.4 | 3656.9 | 2.067 | 1.494 | 0.982 | 0.999 | 0.980 | 1.000 | 1.025 | 0.999 | 0.986 | 1.002 | ok |
| programs/fannkuch | 5 | 1060.9 | 1065.8 | 1095.9 | 1093.8 | 1075.7 | 4075.1 | 4114.8 | 0.986 | 0.991 | 1.007 | 0.994 | 1.003 | 0.994 | 1.001 | 0.992 | 1.003 | 0.992 | ok |
| programs/fasta | 5 | 1287.6 | 1169.2 | 1392.3 | 1257.9 | 1495.7 | 3776.8 | 3842.3 | 0.861 | 0.782 | 1.013 | 0.992 | 1.008 | 0.987 | 1.019 | 0.994 | 1.009 | 0.990 | ok |
| programs/fib | 5 | 1254.1 | 1255.3 | 1247.1 | 1249.5 | 469.7 | 4139.5 | 4150.5 | 2.670 | 2.672 | 0.999 | 0.948 | 1.007 | 0.948 | 1.002 | 0.948 | 1.006 | 0.948 | ok |
| programs/gc_churn | 20 | 2189.7 | 1825.2 | 3281.2 | 2843.3 | 3608.7 | 4605.5 | 5988.9 | 0.607 | 0.506 | 0.984 | 0.999 | 0.997 | 0.993 | 1.011 | 1.004 | 1.014 | 1.007 | ok |
| programs/heapsort | 5 | 1072.4 | 1069.8 | 2262.4 | 2245.4 | 1286.6 | 4063.2 | 3957.0 | 0.834 | 0.832 | 1.008 | 0.990 | 1.011 | 0.990 | 1.025 | 0.999 | 1.024 | 0.999 | ok |
| programs/json | 20 | 1796.2 | 1381.2 | 1904.0 | 1539.2 | 1470.3 | 3127.6 | 3229.9 | 1.222 | 0.939 | 0.961 | 0.999 | 0.972 | 1.000 | 0.989 | 1.004 | 1.012 | 1.002 | ok |
| programs/knucleotide | 2 | 1053.3 | 1069.6 | 1071.6 | 1071.4 | 691.4 | 4061.5 | 4161.0 | 1.523 | 1.547 | 1.020 | 0.995 | 1.014 | 1.003 | 1.018 | 0.995 | 1.009 | 1.003 | ok |
| programs/load_big | 17 | 809.8 | 544.4 | 621.9 | 446.7 | 491.1 | 1787.3 | 620.0 | 1.649 | 1.109 | 1.063 | 0.996 | 0.994 | 1.000 | 1.001 | 0.995 | 0.998 | 0.996 | CHECK |
| programs/loops_arith | 5 | 479.3 | 478.8 | 482.9 | 482.6 | 1108.2 | 4344.5 | 4909.6 | 0.433 | 0.432 | 1.004 | 0.998 | 1.008 | 0.998 | 1.011 | 0.998 | 1.012 | 0.998 | CHECK |
| programs/mandelbrot | 2 | 533.5 | 533.2 | 535.3 | 535.3 | 521.6 | 4727.1 | 5530.2 | 1.023 | 1.022 | 1.002 | 0.997 | 1.002 | 0.997 | 1.003 | 0.997 | 1.003 | 0.997 | ok |
| programs/matmul | 17 | 417.9 | 420.5 | 435.6 | 432.2 | 272.0 | 3171.0 | 3693.4 | 1.537 | 1.546 | 0.999 | 1.000 | 0.976 | 1.000 | 1.002 | 1.000 | 1.010 | 1.000 | ok |
| programs/metamethods | 5 | 666.4 | 627.3 | 795.0 | 751.7 | 540.1 | 4614.3 | 4669.9 | 1.234 | 1.161 | 0.963 | 0.983 | 1.001 | 0.987 | 0.958 | 0.987 | 1.011 | 0.991 | ok |
| programs/nbody | 2 | 348.4 | 347.9 | 356.8 | 356.2 | 345.6 | 4246.2 | 4718.3 | 1.008 | 1.007 | 1.010 | 0.996 | 1.009 | 0.996 | 1.013 | 0.996 | 1.011 | 0.996 | ok |
| programs/oop_methods | 2 | 141.6 | 143.6 | 143.3 | 143.6 | 304.8 | 4567.3 | 4685.2 | 0.465 | 0.471 | 0.991 | 1.000 | 1.011 | 1.000 | 1.003 | 1.000 | 1.011 | 1.000 | ok |
| programs/pcall_error | 20 | 1469.6 | 1220.0 | 1464.6 | 1230.4 | 8346.6 | 4028.8 | 4363.2 | 0.176 | 0.146 | 1.008 | 1.000 | 0.992 | 0.997 | 0.994 | 1.002 | 1.020 | 0.998 | ok |
| programs/queens | 5 | 1842.5 | 1842.0 | 1933.7 | 1930.5 | 1907.1 | 4368.7 | 4120.8 | 0.966 | 0.966 | 0.993 | 0.975 | 0.997 | 0.975 | 1.004 | 0.977 | 1.008 | 0.977 | ok |
| programs/sieve | 17 | 579.4 | 574.2 | 599.4 | 668.2 | 761.7 | 2750.2 | 5029.1 | 0.761 | 0.754 | 1.049 | 1.000 | 1.032 | 1.000 | 0.996 | 1.000 | 0.984 | 1.000 | ok |
| programs/spectral_norm | 2 | 181.8 | 183.2 | 187.4 | 186.8 | 171.1 | 4573.7 | 4337.9 | 1.063 | 1.071 | 1.004 | 1.000 | 1.008 | 1.000 | 1.011 | 1.000 | 1.003 | 1.000 | ok |
| programs/string_ops | 20 | 1151.0 | 1080.0 | 1098.5 | 1020.0 | 2238.4 | 2828.0 | 2807.1 | 0.514 | 0.482 | 1.042 | 1.008 | 1.026 | 0.997 | 1.010 | 1.007 | 1.043 | 0.999 | ok |
| programs/table_array | 5 | 367.6 | 303.2 | 396.2 | 312.5 | 600.0 | 3400.3 | 3492.4 | 0.613 | 0.505 | 1.022 | 1.011 | 1.009 | 1.003 | 0.997 | 0.999 | 1.011 | 1.006 | ok |
| programs/table_hash | 20 | 502.8 | 491.9 | 515.1 | 504.9 | 394.4 | 1501.6 | 1831.0 | 1.275 | 1.247 | 1.065 | 1.007 | 1.101 | 1.003 | 0.996 | 1.006 | 0.988 | 1.002 | ok |
| programs/varargs | 20 | 1214.4 | 1091.6 | 1196.7 | 1094.7 | 2194.8 | 5366.5 | 5084.7 | 0.553 | 0.497 | 0.951 | 0.983 | 0.994 | 0.990 | 0.970 | 0.999 | 0.992 | 0.991 | ok |
| real/game_loop | 20 | 34.1 | 32.6 | 38.7 | 37.2 | 21.7 | 207.6 | 221.0 | 1.570 | 1.503 | 0.790 | 0.824 | 0.785 | 0.821 | 0.797 | 0.841 | 0.797 | 0.839 | ok |
| real/json_roundtrip | 2 | 556.8 | 456.0 | 584.2 | 479.6 | 469.1 | 796.8 | 802.8 | 1.187 | 0.972 | 1.004 | 1.000 | 1.010 | 0.997 | 1.023 | 1.002 | 1.012 | 1.000 | ok |
| real/lua_interp | 20 | 102.9 | 99.6 | 106.9 | 103.7 | 59.3 | 223.6 | 225.0 | 1.736 | 1.679 | 0.883 | 0.882 | 0.921 | 0.909 | 0.895 | 0.879 | 0.921 | 0.905 | ok |
| real/template_engine | 5 | 717.9 | 578.1 | 785.9 | 624.2 | 955.0 | 1225.1 | 1249.2 | 0.752 | 0.605 | 1.000 | 0.993 | 1.002 | 0.986 | 1.025 | 0.995 | 0.981 | 0.992 | ok |
| real/wordfreq | 5 | 114.4 | 97.8 | 116.3 | 99.1 | 116.4 | 212.8 | 210.2 | 0.983 | 0.840 | 1.007 | 0.999 | 1.004 | 0.996 | 1.028 | 0.999 | 1.006 | 0.998 | ok |

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
