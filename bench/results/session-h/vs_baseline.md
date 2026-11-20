# vs previous subject 1a5aad0 and vs 108b935, session-h (subject 8c39465)

Subject columns: 5.5 plain, 5.5 PGO, 5.4 plain, 5.4 PGO (scripts/bench-fast.sh). 1a5aad0 and 108b935 are the cached columns of bench/results/baselines (measured in session-nbody on this machine, min of 5). Reading set A = this session's fast pass only (min of 2 runs, or 5 for programs flagged for full runs); set B adds the 15-run serial reruns of 101 programs (min over all readings: a min of 20 readings is lower than the cached min of 5 by about 1% on average, so B flatters the subject slightly; A is slightly pessimistic for quick-only programs).

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

