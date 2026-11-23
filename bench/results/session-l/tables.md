# Tables, session-l (subject 7413bff)

Ratios below 1: first column faster/fewer. Ours = min over all readings (2 quick, 5 full, 15-run serial reruns). Cached columns as stored.


# Aggregates, readings A: the fast pass only (min of 2 or 5 runs per program)

## Headline geomeans (cycles, ours/ref)

| build | /LuaJIT cyc (n) | /LuaJIT ins | /PUC same version cyc (223) | /PUC ins |
|---|---|---|---|---|
| 5.5 plain | 0.943 (219) | 1.338 | 0.215 | 0.219 |
| 5.5 PGO | 0.861 (219) | 1.234 | 0.197 | 0.202 |
| 5.4 plain | 1.007 (219) | 1.431 | 0.225 | 0.233 |
| 5.4 PGO | 0.920 (219) | 1.321 | 0.206 | 0.216 |

- 5.5 plain / LuaJIT -joff (219): 0.319
- 5.5 PGO / LuaJIT -joff (219): 0.291
- 5a064d7 same-session (set B) (same programs): 5.5 plain/LJ 0.938, PGO/LJ 0.857, 5.4 plain/LJ 0.998, 5.4 PGO/LJ 0.914
- session-k published 5a064d7 (same programs): 5.5 plain/LJ 0.945, PGO/LJ 0.864, 5.4 plain/LJ 1.007, 5.4 PGO/LJ 0.921
- b116447 cached (session-k set B) (same programs): 5.5 plain/LJ 0.948, PGO/LJ 0.866, 5.4 plain/LJ 1.007, 5.4 PGO/LJ 0.923
- our 5.4 build / our 5.5 build cycles: plain 1.068, PGO 1.067; PGO/plain: 5.5 0.915, 5.4 0.915

## Subject vs 5a064d7 (same session, set B), 5a064d7 set A, session-k published and b116447 (geomean, 223 programs)

| build | vs | cycles | instructions | cyc >3% better / within / >3% worse | ins >3% fewer / more |
|---|---|---|---|---|---|
| 5.5 plain | 5a064d7 | 1.006 | 0.996 | 10 / 191 / 22 | 7 / 0 |
| 5.5 plain | 5a064d7 setA | 1.004 | 0.996 | 11 / 192 / 20 | 7 / 0 |
| 5.5 plain | session-k published | 0.997 | 0.996 | 13 / 202 / 8 | 7 / 0 |
| 5.5 plain | b116447 | 0.994 | 0.991 | 12 / 201 / 10 | 10 / 0 |
| 5.5 PGO | 5a064d7 | 1.006 | 0.996 | 8 / 193 / 22 | 7 / 0 |
| 5.5 PGO | 5a064d7 setA | 1.004 | 0.996 | 9 / 192 / 22 | 7 / 0 |
| 5.5 PGO | session-k published | 0.997 | 0.996 | 10 / 205 / 8 | 7 / 0 |
| 5.5 PGO | b116447 | 0.994 | 0.992 | 17 / 199 / 7 | 10 / 0 |
| 5.4 plain | 5a064d7 | 1.009 | 0.996 | 8 / 188 / 27 | 5 / 0 |
| 5.4 plain | 5a064d7 setA | 1.008 | 0.996 | 8 / 191 / 24 | 5 / 0 |
| 5.4 plain | session-k published | 1.001 | 0.996 | 12 / 194 / 17 | 5 / 0 |
| 5.4 plain | b116447 | 1.000 | 0.992 | 13 / 188 / 22 | 8 / 0 |
| 5.4 PGO | 5a064d7 | 1.006 | 0.996 | 10 / 196 / 17 | 5 / 0 |
| 5.4 PGO | 5a064d7 setA | 1.004 | 0.996 | 13 / 197 / 13 | 5 / 0 |
| 5.4 PGO | session-k published | 0.998 | 0.997 | 11 / 200 / 12 | 5 / 0 |
| 5.4 PGO | b116447 | 0.996 | 0.992 | 13 / 195 / 15 | 7 / 0 |

### Per set, geomean subject / 5a064d7 and subject / b116447 (cycles / instructions)

| set | n | plain vs b cyc | ins | PGO vs b cyc | ins | 5.4 plain vs b cyc | ins | 5.4 PGO vs b cyc | ins | plain vs i cyc | ins | PGO vs i cyc | ins | 5.4 plain vs i cyc | ins | 5.4 PGO vs i cyc | ins |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| apps | 5 | 0.991 | 0.988 | 0.997 | 0.990 | 0.980 | 0.985 | 0.972 | 0.988 | 0.982 | 0.988 | 0.988 | 0.993 | 0.977 | 0.985 | 0.977 | 0.988 |
| awfy | 14 | 1.009 | 0.995 | 1.005 | 0.996 | 1.008 | 0.996 | 1.005 | 0.997 | 0.986 | 0.979 | 0.981 | 0.979 | 0.991 | 0.982 | 0.989 | 0.982 |
| corpus | 18 | 1.012 | 0.993 | 1.015 | 0.993 | 1.006 | 0.999 | 1.010 | 0.998 | 1.007 | 0.993 | 1.016 | 0.997 | 1.010 | 0.999 | 1.011 | 0.999 |
| corpus2/dev | 17 | 1.006 | 0.976 | 1.012 | 0.976 | 1.018 | 0.979 | 1.025 | 0.980 | 0.998 | 0.975 | 1.010 | 0.976 | 1.012 | 0.978 | 1.020 | 0.979 |
| corpus2/heldout | 11 | 1.034 | 0.994 | 1.033 | 0.994 | 1.024 | 0.986 | 1.026 | 0.985 | 1.025 | 0.994 | 1.016 | 0.994 | 1.009 | 0.986 | 1.012 | 0.985 |
| jit-trace | 29 | 0.999 | 0.994 | 0.998 | 0.994 | 1.002 | 0.995 | 0.999 | 0.995 | 0.997 | 1.000 | 0.997 | 1.000 | 0.999 | 1.000 | 0.997 | 1.000 |
| lib | 95 | 1.003 | 1.000 | 1.002 | 1.000 | 1.012 | 1.000 | 1.003 | 1.000 | 0.997 | 1.000 | 0.995 | 1.001 | 1.007 | 1.000 | 0.997 | 1.000 |
| programs | 25 | 1.007 | 0.998 | 1.005 | 0.998 | 1.006 | 1.000 | 1.008 | 1.000 | 0.962 | 0.961 | 0.964 | 0.960 | 0.966 | 0.964 | 0.967 | 0.963 |
| real | 5 | 1.002 | 0.999 | 1.003 | 1.000 | 0.997 | 0.999 | 1.002 | 0.999 | 0.986 | 1.000 | 0.981 | 1.001 | 0.986 | 1.001 | 0.988 | 1.001 |
| real_dis | 4 | 1.018 | 1.001 | 1.004 | 1.000 | 1.022 | 1.007 | 1.021 | 1.007 | 1.009 | 1.001 | 0.982 | 1.004 | 1.020 | 1.007 | 1.017 | 1.007 |
| ALL | 223 | 1.006 | 0.996 | 1.006 | 0.996 | 1.009 | 0.996 | 1.006 | 0.996 | 0.994 | 0.991 | 0.994 | 0.992 | 1.000 | 0.992 | 0.996 | 0.992 |

# Aggregates, readings B: with the 15-run serial reruns of 119 programs merged (min over all readings)

## Headline geomeans (cycles, ours/ref)

| build | /LuaJIT cyc (n) | /LuaJIT ins | /PUC same version cyc (223) | /PUC ins |
|---|---|---|---|---|
| 5.5 plain | 0.939 (219) | 1.338 | 0.214 | 0.219 |
| 5.5 PGO | 0.858 (219) | 1.234 | 0.196 | 0.202 |
| 5.4 plain | 1.002 (219) | 1.431 | 0.224 | 0.233 |
| 5.4 PGO | 0.915 (219) | 1.321 | 0.205 | 0.216 |

- 5.5 plain / LuaJIT -joff (219): 0.317
- 5.5 PGO / LuaJIT -joff (219): 0.290
- 5a064d7 same-session (set B) (same programs): 5.5 plain/LJ 0.938, PGO/LJ 0.857, 5.4 plain/LJ 0.998, 5.4 PGO/LJ 0.914
- session-k published 5a064d7 (same programs): 5.5 plain/LJ 0.945, PGO/LJ 0.864, 5.4 plain/LJ 1.007, 5.4 PGO/LJ 0.921
- b116447 cached (session-k set B) (same programs): 5.5 plain/LJ 0.948, PGO/LJ 0.866, 5.4 plain/LJ 1.007, 5.4 PGO/LJ 0.923
- our 5.4 build / our 5.5 build cycles: plain 1.066, PGO 1.066; PGO/plain: 5.5 0.915, 5.4 0.915

## Subject vs 5a064d7 (same session, set B), 5a064d7 set A, session-k published and b116447 (geomean, 223 programs)

| build | vs | cycles | instructions | cyc >3% better / within / >3% worse | ins >3% fewer / more |
|---|---|---|---|---|---|
| 5.5 plain | 5a064d7 | 1.001 | 0.996 | 10 / 200 / 13 | 7 / 0 |
| 5.5 plain | 5a064d7 setA | 1.000 | 0.996 | 12 / 199 / 12 | 7 / 0 |
| 5.5 plain | session-k published | 0.993 | 0.996 | 14 / 205 / 4 | 7 / 0 |
| 5.5 plain | b116447 | 0.990 | 0.991 | 13 / 206 / 4 | 10 / 0 |
| 5.5 PGO | 5a064d7 | 1.001 | 0.996 | 10 / 201 / 12 | 7 / 0 |
| 5.5 PGO | 5a064d7 setA | 1.000 | 0.996 | 13 / 198 / 12 | 7 / 0 |
| 5.5 PGO | session-k published | 0.993 | 0.996 | 12 / 208 / 3 | 7 / 0 |
| 5.5 PGO | b116447 | 0.990 | 0.992 | 17 / 203 / 3 | 10 / 0 |
| 5.4 plain | 5a064d7 | 1.004 | 0.996 | 8 / 202 / 13 | 5 / 0 |
| 5.4 plain | 5a064d7 setA | 1.003 | 0.996 | 10 / 202 / 11 | 5 / 0 |
| 5.4 plain | session-k published | 0.995 | 0.996 | 13 / 201 / 9 | 5 / 0 |
| 5.4 plain | b116447 | 0.995 | 0.992 | 15 / 197 / 11 | 8 / 0 |
| 5.4 PGO | 5a064d7 | 1.001 | 0.996 | 10 / 207 / 6 | 5 / 0 |
| 5.4 PGO | 5a064d7 setA | 0.999 | 0.996 | 15 / 203 / 5 | 5 / 0 |
| 5.4 PGO | session-k published | 0.993 | 0.997 | 13 / 207 / 3 | 5 / 0 |
| 5.4 PGO | b116447 | 0.991 | 0.992 | 13 / 205 / 5 | 7 / 0 |

### Per set, geomean subject / 5a064d7 and subject / b116447 (cycles / instructions)

| set | n | plain vs b cyc | ins | PGO vs b cyc | ins | 5.4 plain vs b cyc | ins | 5.4 PGO vs b cyc | ins | plain vs i cyc | ins | PGO vs i cyc | ins | 5.4 plain vs i cyc | ins | 5.4 PGO vs i cyc | ins |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| apps | 5 | 0.991 | 0.987 | 0.997 | 0.990 | 0.980 | 0.985 | 0.972 | 0.988 | 0.982 | 0.988 | 0.988 | 0.993 | 0.977 | 0.985 | 0.977 | 0.988 |
| awfy | 14 | 1.006 | 0.995 | 1.003 | 0.996 | 1.005 | 0.996 | 1.003 | 0.997 | 0.983 | 0.979 | 0.979 | 0.979 | 0.988 | 0.982 | 0.987 | 0.982 |
| corpus | 18 | 0.999 | 0.992 | 1.003 | 0.993 | 0.999 | 0.999 | 1.003 | 0.998 | 0.994 | 0.993 | 1.004 | 0.997 | 1.003 | 0.999 | 1.004 | 0.999 |
| corpus2/dev | 17 | 0.991 | 0.976 | 0.997 | 0.976 | 1.002 | 0.979 | 0.998 | 0.980 | 0.984 | 0.975 | 0.995 | 0.976 | 0.997 | 0.978 | 0.994 | 0.979 |
| corpus2/heldout | 11 | 1.017 | 0.994 | 1.021 | 0.994 | 1.015 | 0.986 | 1.009 | 0.985 | 1.009 | 0.994 | 1.004 | 0.994 | 1.000 | 0.986 | 0.995 | 0.985 |
| jit-trace | 29 | 0.998 | 0.994 | 0.998 | 0.994 | 0.999 | 0.995 | 0.998 | 0.995 | 0.996 | 1.000 | 0.997 | 1.000 | 0.997 | 1.000 | 0.996 | 1.000 |
| lib | 95 | 1.001 | 1.000 | 1.000 | 1.000 | 1.006 | 1.000 | 1.001 | 1.000 | 0.995 | 1.000 | 0.992 | 1.001 | 1.000 | 1.000 | 0.994 | 1.000 |
| programs | 25 | 1.005 | 0.998 | 1.003 | 0.998 | 1.005 | 1.000 | 1.006 | 1.000 | 0.961 | 0.961 | 0.962 | 0.960 | 0.965 | 0.964 | 0.966 | 0.963 |
| real | 5 | 0.998 | 0.999 | 1.000 | 0.999 | 0.997 | 0.999 | 0.998 | 0.999 | 0.982 | 1.000 | 0.978 | 1.001 | 0.985 | 1.001 | 0.983 | 1.001 |
| real_dis | 4 | 1.014 | 1.001 | 1.000 | 1.000 | 1.021 | 1.007 | 1.016 | 1.006 | 1.005 | 1.001 | 0.978 | 1.004 | 1.020 | 1.007 | 1.011 | 1.006 |
| ALL | 223 | 1.001 | 0.996 | 1.001 | 0.996 | 1.004 | 0.996 | 1.001 | 0.996 | 0.990 | 0.991 | 0.990 | 0.992 | 0.995 | 0.992 | 0.991 | 0.992 |

### 5.5 plain vs 5a064d7: 20 biggest improvements by cycles (>= 20 Mcyc)

| program | Mcyc | cyc ratio | ins ratio | cycle spread of readings | note |
|---|---|---|---|---|---|
| corpus2/dev/oop_mixins | 1122 | 0.805 | 0.710 | 8.8% (20 runs) | instructions move the same way |
| corpus2/dev/regex_nfa | 643 | 0.878 | 0.956 | 4.5% (20 runs) | instructions move the same way |
| jit-trace/rec_loop | 83 | 0.897 | 0.856 | 14.2% (20 runs) | instructions move the same way |
| corpus/lua_lexer | 982 | 0.910 | 0.894 | 16.0% (20 runs) | instructions move the same way |
| lib/load_small | 226 | 0.946 | 1.000 | 4.2% (20 runs) | instructions equal: layout / IPC |
| lib/tmove_1k | 96 | 0.950 | 1.000 | 0.6% (2 runs) | instructions equal: layout / IPC |
| lib/tonumber_bad | 149 | 0.954 | 1.000 | 7.6% (20 runs) | instructions equal: layout / IPC |
| corpus2/heldout/vending_fsm | 978 | 0.960 | 0.946 | 4.7% (20 runs) | instructions move the same way |
| lib/sel_n | 324 | 0.963 | 1.000 | 18.9% (20 runs) | instructions equal: layout / IPC |
| apps/raytrace | 108 | 0.970 | 0.956 | 4.6% (20 runs) | instructions move the same way |
| lib/pcall_err | 119 | 0.979 | 1.000 | 19.7% (20 runs) |  |
| corpus/json_rxi | 1779 | 0.980 | 1.000 | 4.1% (20 runs) |  |
| lib/tpack | 146 | 0.981 | 1.000 | 2.9% (5 runs) |  |
| lib/error_tbl | 262 | 0.983 | 1.000 | 10.5% (20 runs) |  |
| lib/tonumber_num | 211 | 0.983 | 1.000 | 3.6% (5 runs) |  |
| lib/sub_1char | 118 | 0.985 | 1.000 | 15.5% (20 runs) |  |
| programs/closures | 455 | 0.986 | 1.000 | 10.1% (20 runs) |  |
| corpus/classic_vector | 465 | 0.986 | 0.993 | 6.7% (20 runs) |  |
| apps/expr_lang | 134 | 0.986 | 0.996 | 2.2% (5 runs) |  |
| programs/load_big | 744 | 0.987 | 1.000 | 0.9% (2 runs) |  |

### 5.5 PGO vs 5a064d7: 20 biggest improvements by cycles (>= 20 Mcyc)

| program | Mcyc | cyc ratio | ins ratio | cycle spread of readings | note |
|---|---|---|---|---|---|
| corpus2/dev/oop_mixins | 1074 | 0.791 | 0.707 | 9.4% (20 runs) | instructions move the same way |
| jit-trace/rec_loop | 82 | 0.893 | 0.855 | 8.8% (20 runs) | instructions move the same way |
| corpus/lua_lexer | 877 | 0.915 | 0.892 | 37.4% (20 runs) | instructions move the same way |
| lib/tonumber_bad | 111 | 0.921 | 1.000 | 20.6% (20 runs) | instructions equal: layout / IPC |
| corpus2/heldout/vending_fsm | 895 | 0.950 | 0.945 | 7.4% (20 runs) | instructions move the same way |
| lib/sub | 122 | 0.955 | 1.000 | 0.1% (2 runs) | instructions equal: layout / IPC |
| programs/matmul | 420 | 0.965 | 1.000 | 4.5% (5 runs) | instructions equal: layout / IPC |
| lib/load_small | 147 | 0.967 | 1.000 | 16.2% (20 runs) | instructions equal: layout / IPC |
| programs/json | 1389 | 0.968 | 1.000 | 6.8% (20 runs) | instructions equal: layout / IPC |
| corpus2/dev/regex_nfa | 546 | 0.969 | 0.958 | 11.8% (20 runs) | instructions move the same way |
| apps/raytrace | 101 | 0.970 | 0.966 | 7.7% (20 runs) |  |
| awfy/json | 118 | 0.974 | 0.987 | 3.9% (5 runs) |  |
| lib/pcall_ok | 29 | 0.975 | 1.000 | 0.6% (2 runs) |  |
| lib/error_tbl | 215 | 0.977 | 1.001 | 9.4% (20 runs) |  |
| apps/expr_lang | 122 | 0.979 | 0.997 | 1.2% (5 runs) |  |
| lib/pcall_err | 98 | 0.981 | 1.001 | 19.4% (20 runs) |  |
| lib/upper | 89 | 0.981 | 1.000 | 3.1% (5 runs) |  |
| lib/tconcat_num | 117 | 0.981 | 1.000 | 1.8% (2 runs) |  |
| corpus2/heldout/real_dis_ppc | 1827 | 0.983 | 0.999 | 0.6% (2 runs) |  |
| lib/byte_multi | 41 | 0.983 | 1.000 | 2.5% (5 runs) |  |

### 5.4 plain vs 5a064d7: 20 biggest improvements by cycles (>= 20 Mcyc)

| program | Mcyc | cyc ratio | ins ratio | cycle spread of readings | note |
|---|---|---|---|---|---|
| corpus2/dev/oop_mixins | 1135 | 0.793 | 0.722 | 17.5% (20 runs) | instructions move the same way |
| corpus2/heldout/vending_fsm | 1019 | 0.932 | 0.887 | 2.9% (20 runs) | instructions move the same way |
| jit-trace/rec_loop | 83 | 0.934 | 0.857 | 62.1% (20 runs) | instructions move the same way |
| apps/expr_lang | 150 | 0.940 | 0.976 | 2.0% (5 runs) | instructions move the same way |
| apps/raytrace | 113 | 0.958 | 0.960 | 10.1% (20 runs) | instructions move the same way |
| corpus2/heldout/forth_vm | 1012 | 0.963 | 0.963 | 2.1% (5 runs) | instructions move the same way |
| corpus2/dev/regex_nfa | 1345 | 0.963 | 1.000 | 3.1% (20 runs) | instructions equal: layout / IPC |
| programs/load_big | 639 | 0.965 | 1.000 | 0.2% (2 runs) | instructions equal: layout / IPC |
| corpus/classic_vector | 630 | 0.970 | 0.995 | 6.4% (20 runs) |  |
| lib/tmove_1k | 108 | 0.972 | 1.000 | 2.0% (2 runs) |  |
| jit-trace/field_global | 525 | 0.972 | 1.000 | 8.1% (20 runs) |  |
| lib/pcall_err | 128 | 0.973 | 1.000 | 8.5% (20 runs) |  |
| lib/find_pat | 264 | 0.975 | 1.000 | 11.3% (20 runs) |  |
| lib/tconcat_num | 181 | 0.975 | 1.000 | 1.1% (2 runs) |  |
| programs/matmul | 435 | 0.977 | 1.000 | 2.6% (5 runs) |  |
| real/template_engine | 739 | 0.978 | 1.000 | 5.3% (20 runs) |  |
| lib/rep | 119 | 0.980 | 1.000 | 0.0% (2 runs) |  |
| corpus/mustache_lustache | 1909 | 0.980 | 1.000 | 12.2% (20 runs) |  |
| lib/tonumber_bad | 163 | 0.981 | 1.000 | 13.5% (20 runs) |  |
| lib/tonumber_num | 214 | 0.985 | 1.000 | 4.3% (5 runs) |  |

### 5.4 PGO vs 5a064d7: 20 biggest improvements by cycles (>= 20 Mcyc)

| program | Mcyc | cyc ratio | ins ratio | cycle spread of readings | note |
|---|---|---|---|---|---|
| corpus2/dev/oop_mixins | 1071 | 0.810 | 0.722 | 16.0% (20 runs) | instructions move the same way |
| jit-trace/rec_loop | 82 | 0.931 | 0.857 | 22.8% (20 runs) | instructions move the same way |
| apps/expr_lang | 136 | 0.933 | 0.978 | 0.7% (5 runs) | instructions move the same way |
| corpus2/heldout/vending_fsm | 948 | 0.938 | 0.881 | 2.4% (20 runs) | instructions move the same way |
| lib/tmove_1k | 72 | 0.964 | 1.000 | 0.4% (2 runs) | instructions equal: layout / IPC |
| corpus2/dev/regex_nfa | 1167 | 0.965 | 1.000 | 2.5% (20 runs) | instructions equal: layout / IPC |
| corpus2/heldout/forth_vm | 1127 | 0.967 | 0.962 | 2.4% (5 runs) | instructions move the same way |
| apps/events | 149 | 0.967 | 0.999 | 5.7% (20 runs) | instructions equal: layout / IPC |
| apps/raytrace | 106 | 0.970 | 0.969 | 4.1% (20 runs) | instructions move the same way |
| lib/sub | 121 | 0.970 | 1.000 | 0.3% (2 runs) | instructions equal: layout / IPC |
| corpus/parser_combinator | 2039 | 0.977 | 1.000 | 21.2% (20 runs) |  |
| lib/tremove_front | 168 | 0.982 | 1.000 | 4.2% (5 runs) |  |
| lib/pcall_err | 108 | 0.982 | 0.999 | 9.5% (20 runs) |  |
| lib/str_method_len | 50 | 0.982 | 1.000 | 28.2% (20 runs) |  |
| lib/tremove_pop | 206 | 0.984 | 1.000 | 5.6% (20 runs) |  |
| corpus2/heldout/stream_closures | 768 | 0.985 | 0.998 | 1.3% (5 runs) |  |
| lib/tpack | 165 | 0.986 | 1.000 | 4.9% (5 runs) |  |
| lib/tonumber_bad | 142 | 0.987 | 1.000 | 1.5% (20 runs) |  |
| lib/str_eq_long | 35 | 0.987 | 1.000 | 9.5% (20 runs) |  |
| jit-trace/type_entry | 1557 | 0.987 | 1.000 | 8.1% (20 runs) |  |

### 5.5 plain vs 5a064d7: regressions (instructions > +2% or cycles > +3%, >= 20 Mcyc)

| program | Mcyc | cyc ratio | ins ratio | cycle spread of readings | note |
|---|---|---|---|---|---|
| corpus2/heldout/prototype_objects | 1568 | 1.133 | 1.004 | 89.6% (20 runs) | instructions equal: layout / IPC |
| programs/gc_churn | 2263 | 1.060 | 1.000 | 26.4% (20 runs) | instructions equal: layout / IPC |
| corpus2/heldout/roguelike_turns | 851 | 1.057 | 1.017 | 3.7% (20 runs) | instructions move the same way |
| lib/gmatch_words | 185 | 1.053 | 1.000 | 12.1% (20 runs) | instructions equal: layout / IPC |
| lib/concat_op_str | 140 | 1.041 | 1.000 | 6.9% (17 runs) | instructions equal: layout / IPC |
| jit-trace/tfor_loops | 226 | 1.040 | 1.000 | 5.9% (20 runs) | instructions equal: layout / IPC |
| programs/binary_trees | 1265 | 1.040 | 1.000 | 7.9% (20 runs) | instructions equal: layout / IPC |
| awfy/havlak | 152 | 1.034 | 0.993 | 17.3% (20 runs) | instructions equal: layout / IPC |
| corpus2/dev/event_promise | 384 | 1.033 | 0.999 | 6.9% (20 runs) | instructions equal: layout / IPC |
| awfy/richards | 62 | 1.033 | 0.995 | 4.1% (20 runs) | instructions equal: layout / IPC |
| lib/m_min | 226 | 1.032 | 1.000 | 9.7% (20 runs) | instructions equal: layout / IPC |
| corpus2/dev/lua_parser | 1299 | 1.031 | 0.998 | 2.8% (17 runs) | instructions equal: layout / IPC |
| corpus2/dev/iter_pipeline | 179 | 1.030 | 0.998 | 3.4% (17 runs) | instructions equal: layout / IPC |

### 5.5 PGO vs 5a064d7: regressions (instructions > +2% or cycles > +3%, >= 20 Mcyc)

| program | Mcyc | cyc ratio | ins ratio | cycle spread of readings | note |
|---|---|---|---|---|---|
| corpus2/heldout/prototype_objects | 1492 | 1.152 | 1.006 | 44.6% (20 runs) | instructions equal: layout / IPC |
| lib/it_pairs_hash | 107 | 1.049 | 1.000 | 17.7% (20 runs) | instructions equal: layout / IPC |
| programs/gc_churn | 1854 | 1.049 | 1.000 | 22.7% (20 runs) | instructions equal: layout / IPC |
| corpus2/heldout/roguelike_turns | 804 | 1.048 | 1.018 | 3.6% (20 runs) | instructions move the same way |
| lib/sel_n | 338 | 1.047 | 1.000 | 21.1% (20 runs) | instructions equal: layout / IPC |
| corpus2/dev/event_promise | 367 | 1.043 | 0.999 | 10.8% (20 runs) | instructions equal: layout / IPC |
| awfy/richards | 59 | 1.035 | 0.993 | 4.1% (20 runs) | instructions equal: layout / IPC |
| corpus/parser_combinator | 1927 | 1.032 | 1.000 | 39.5% (20 runs) | instructions equal: layout / IPC |
| corpus2/dev/wordfreq_wrap | 506 | 1.032 | 1.001 | 29.6% (20 runs) | instructions equal: layout / IPC |
| programs/coroutines | 1340 | 1.032 | 1.003 | 7.6% (20 runs) | instructions equal: layout / IPC |
| programs/binary_trees | 1062 | 1.031 | 1.000 | 20.3% (20 runs) | instructions equal: layout / IPC |
| apps/events | 144 | 1.030 | 0.999 | 7.4% (20 runs) | instructions equal: layout / IPC |

### 5.4 plain vs 5a064d7: regressions (instructions > +2% or cycles > +3%, >= 20 Mcyc)

| program | Mcyc | cyc ratio | ins ratio | cycle spread of readings | note |
|---|---|---|---|---|---|
| corpus2/heldout/prototype_objects | 1614 | 1.175 | 0.997 | 100.0% (20 runs) | instructions equal: layout / IPC |
| lib/str_method_len | 56 | 1.108 | 1.000 | 27.1% (20 runs) | instructions equal: layout / IPC |
| lib/len_op | 63 | 1.090 | 1.000 | 68.4% (20 runs) | instructions equal: layout / IPC |
| lib/it_pairs_hash | 121 | 1.071 | 1.000 | 7.9% (20 runs) | instructions equal: layout / IPC |
| corpus2/dev/wordfreq_wrap | 685 | 1.064 | 1.000 | 32.6% (20 runs) | instructions equal: layout / IPC |
| lib/m_tointeger | 237 | 1.061 | 1.000 | 1.5% (20 runs) | instructions equal: layout / IPC |
| programs/gc_churn | 3392 | 1.057 | 1.000 | 8.3% (20 runs) | instructions equal: layout / IPC |
| corpus2/heldout/real_dis_mips | 2278 | 1.045 | 1.022 | 3.3% (20 runs) | instructions move the same way |
| lib/str_eq_dyn | 140 | 1.045 | 1.000 | 23.9% (20 runs) | instructions equal: layout / IPC |
| corpus2/dev/diff_levenshtein | 450 | 1.036 | 1.000 | 4.1% (20 runs) | instructions equal: layout / IPC |
| lib/it_pairs | 96 | 1.035 | 1.000 | 11.9% (20 runs) | instructions equal: layout / IPC |
| corpus2/dev/iter_pipeline | 188 | 1.034 | 0.998 | 3.0% (17 runs) | instructions equal: layout / IPC |
| lib/getmt | 20 | 1.032 | 1.000 | 2.5% (17 runs) | instructions equal: layout / IPC |

### 5.4 PGO vs 5a064d7: regressions (instructions > +2% or cycles > +3%, >= 20 Mcyc)

| program | Mcyc | cyc ratio | ins ratio | cycle spread of readings | note |
|---|---|---|---|---|---|
| corpus2/heldout/prototype_objects | 1521 | 1.153 | 0.994 | 74.2% (20 runs) | instructions equal: layout / IPC |
| lib/load_small | 169 | 1.066 | 1.000 | 5.5% (20 runs) | instructions equal: layout / IPC |
| programs/gc_churn | 2885 | 1.045 | 1.000 | 14.5% (20 runs) | instructions equal: layout / IPC |
| corpus2/dev/csv_report | 1088 | 1.036 | 0.999 | 44.1% (20 runs) | instructions equal: layout / IPC |
| corpus2/heldout/real_dis_mips | 1931 | 1.033 | 1.021 | 5.6% (20 runs) | instructions move the same way |
| corpus2/dev/wordfreq_wrap | 563 | 1.032 | 0.999 | 25.7% (20 runs) | instructions equal: layout / IPC |

### 5.5 plain vs b116447: 20 biggest improvements by cycles (>= 20 Mcyc)

| program | Mcyc | cyc ratio | ins ratio | cycle spread of readings | note |
|---|---|---|---|---|---|
| programs/fib | 631 | 0.507 | 0.505 | 9.3% (20 runs) | instructions move the same way |
| corpus2/dev/oop_mixins | 1122 | 0.774 | 0.710 | 8.8% (20 runs) | instructions move the same way |
| programs/binary_trees | 1265 | 0.824 | 0.762 | 7.9% (20 runs) | instructions move the same way |
| awfy/permute | 36 | 0.831 | 0.829 | 3.6% (17 runs) | instructions move the same way |
| corpus/lua_lexer | 982 | 0.910 | 0.894 | 16.0% (20 runs) | instructions move the same way |
| corpus2/dev/regex_nfa | 643 | 0.910 | 0.956 | 4.5% (20 runs) | instructions move the same way |
| programs/table_hash | 495 | 0.926 | 1.000 | 64.5% (20 runs) | instructions equal: layout / IPC |
| real/lua_interp | 105 | 0.943 | 1.002 | 3.5% (5 runs) | instructions equal: layout / IPC |
| awfy/towers | 47 | 0.953 | 0.969 | 4.2% (20 runs) | instructions move the same way |
| corpus2/heldout/vending_fsm | 978 | 0.957 | 0.947 | 4.7% (20 runs) | instructions move the same way |
| corpus/csv_ini_yaml | 1545 | 0.958 | 1.000 | 16.1% (17 runs) | instructions equal: layout / IPC |
| lib/tpack | 146 | 0.961 | 1.000 | 2.9% (5 runs) | instructions equal: layout / IPC |
| apps/events | 163 | 0.961 | 1.000 | 12.7% (20 runs) | instructions equal: layout / IPC |
| lib/tsort_cmp | 265 | 0.971 | 1.000 | 30.1% (20 runs) |  |
| corpus/classic_vector | 465 | 0.971 | 0.994 | 6.7% (20 runs) |  |
| lib/tonumber_bad | 149 | 0.971 | 1.000 | 7.6% (20 runs) |  |
| apps/raytrace | 108 | 0.972 | 0.956 | 4.6% (20 runs) |  |
| apps/lru_cache | 143 | 0.974 | 0.989 | 16.0% (20 runs) |  |
| lib/pcall_err | 119 | 0.974 | 1.000 | 19.7% (20 runs) |  |
| lib/co_yield | 237 | 0.975 | 1.000 | 13.5% (20 runs) |  |

### 5.5 PGO vs b116447: 20 biggest improvements by cycles (>= 20 Mcyc)

| program | Mcyc | cyc ratio | ins ratio | cycle spread of readings | note |
|---|---|---|---|---|---|
| programs/fib | 628 | 0.504 | 0.505 | 12.8% (20 runs) | instructions move the same way |
| corpus2/dev/oop_mixins | 1074 | 0.791 | 0.704 | 9.4% (20 runs) | instructions move the same way |
| programs/binary_trees | 1062 | 0.815 | 0.738 | 20.3% (20 runs) | instructions move the same way |
| awfy/permute | 36 | 0.829 | 0.829 | 10.9% (17 runs) | instructions move the same way |
| lib/tonumber_bad | 111 | 0.875 | 1.004 | 20.6% (20 runs) | instructions equal: layout / IPC |
| lib/load_small | 147 | 0.915 | 1.000 | 16.2% (20 runs) | instructions equal: layout / IPC |
| corpus/lua_lexer | 877 | 0.917 | 0.894 | 37.4% (20 runs) | instructions move the same way |
| real/lua_interp | 102 | 0.940 | 1.003 | 2.9% (5 runs) | instructions equal: layout / IPC |
| programs/table_hash | 479 | 0.947 | 1.000 | 73.3% (20 runs) | instructions equal: layout / IPC |
| corpus2/heldout/real_dis_mips | 1866 | 0.949 | 1.002 | 7.1% (20 runs) | instructions equal: layout / IPC |
| corpus2/dev/regex_nfa | 546 | 0.953 | 0.959 | 11.8% (20 runs) | instructions move the same way |
| awfy/towers | 45 | 0.954 | 0.969 | 9.1% (20 runs) | instructions move the same way |
| corpus2/heldout/vending_fsm | 895 | 0.958 | 0.945 | 7.4% (20 runs) | instructions move the same way |
| apps/lru_cache | 131 | 0.966 | 0.989 | 16.3% (20 runs) | instructions move the same way |
| apps/raytrace | 101 | 0.968 | 0.964 | 7.7% (20 runs) | instructions move the same way |
| programs/matmul | 420 | 0.969 | 1.000 | 4.5% (5 runs) | instructions equal: layout / IPC |
| corpus2/heldout/real_dis_ppc | 1827 | 0.969 | 1.003 | 0.6% (2 runs) | instructions equal: layout / IPC |
| lib/find_pat | 230 | 0.971 | 1.012 | 17.7% (20 runs) |  |
| real/template_engine | 596 | 0.972 | 1.006 | 10.0% (20 runs) |  |
| real/json_roundtrip | 451 | 0.972 | 1.001 | 30.0% (20 runs) |  |

### 5.4 plain vs b116447: 20 biggest improvements by cycles (>= 20 Mcyc)

| program | Mcyc | cyc ratio | ins ratio | cycle spread of readings | note |
|---|---|---|---|---|---|
| programs/fib | 627 | 0.506 | 0.505 | 14.9% (20 runs) | instructions move the same way |
| corpus2/dev/oop_mixins | 1135 | 0.790 | 0.721 | 17.5% (20 runs) | instructions move the same way |
| programs/binary_trees | 1459 | 0.838 | 0.789 | 17.0% (20 runs) | instructions move the same way |
| awfy/permute | 38 | 0.865 | 0.835 | 15.8% (17 runs) | instructions move the same way |
| programs/table_hash | 491 | 0.915 | 1.000 | 79.2% (20 runs) | instructions equal: layout / IPC |
| corpus2/heldout/vending_fsm | 1019 | 0.928 | 0.887 | 2.9% (20 runs) | instructions move the same way |
| real/lua_interp | 105 | 0.937 | 1.008 | 2.4% (5 runs) | instructions equal: layout / IPC |
| apps/expr_lang | 150 | 0.950 | 0.977 | 2.0% (5 runs) | instructions move the same way |
| lib/gsub_str | 165 | 0.952 | 1.000 | 0.0% (2 runs) | instructions equal: layout / IPC |
| awfy/towers | 47 | 0.958 | 0.969 | 16.7% (20 runs) | instructions move the same way |
| jit-trace/field_global | 525 | 0.959 | 1.000 | 8.1% (20 runs) | instructions equal: layout / IPC |
| corpus2/heldout/forth_vm | 1012 | 0.962 | 0.963 | 2.1% (5 runs) | instructions move the same way |
| lib/tonumber_bad | 163 | 0.968 | 1.000 | 13.5% (20 runs) | instructions equal: layout / IPC |
| apps/raytrace | 113 | 0.968 | 0.960 | 10.1% (20 runs) | instructions move the same way |
| lib/pcall_err | 128 | 0.969 | 1.000 | 8.5% (20 runs) | instructions equal: layout / IPC |
| programs/load_big | 639 | 0.972 | 1.000 | 0.2% (2 runs) |  |
| lib/co_wrap | 237 | 0.978 | 1.000 | 13.2% (20 runs) |  |
| corpus/classic_vector | 630 | 0.979 | 0.995 | 6.4% (20 runs) |  |
| jit-trace/str_concat | 592 | 0.979 | 1.000 | 2.4% (20 runs) |  |
| lib/m_min | 221 | 0.981 | 1.000 | 17.1% (20 runs) |  |

### 5.4 PGO vs b116447: 20 biggest improvements by cycles (>= 20 Mcyc)

| program | Mcyc | cyc ratio | ins ratio | cycle spread of readings | note |
|---|---|---|---|---|---|
| programs/fib | 626 | 0.505 | 0.505 | 53.5% (20 runs) | instructions move the same way |
| corpus2/dev/oop_mixins | 1071 | 0.801 | 0.721 | 16.0% (20 runs) | instructions move the same way |
| programs/binary_trees | 1199 | 0.808 | 0.769 | 18.1% (20 runs) | instructions move the same way |
| awfy/permute | 37 | 0.861 | 0.834 | 4.3% (17 runs) | instructions move the same way |
| lib/co_wrap | 197 | 0.862 | 0.998 | 10.1% (20 runs) | instructions equal: layout / IPC |
| programs/table_hash | 491 | 0.924 | 0.999 | 74.4% (20 runs) | instructions equal: layout / IPC |
| corpus2/heldout/vending_fsm | 948 | 0.932 | 0.882 | 2.4% (20 runs) | instructions move the same way |
| real/lua_interp | 101 | 0.942 | 1.009 | 3.5% (5 runs) | instructions equal: layout / IPC |
| apps/expr_lang | 136 | 0.949 | 0.978 | 0.7% (5 runs) | instructions move the same way |
| lib/tpack | 165 | 0.959 | 1.001 | 4.9% (5 runs) | instructions equal: layout / IPC |
| awfy/towers | 46 | 0.963 | 0.970 | 23.9% (20 runs) | instructions move the same way |
| lib/pcall_err | 108 | 0.965 | 0.999 | 9.5% (20 runs) | instructions equal: layout / IPC |
| apps/events | 149 | 0.966 | 1.001 | 5.7% (20 runs) | instructions equal: layout / IPC |
| lib/it_pairs_hash | 107 | 0.970 | 1.000 | 17.2% (20 runs) |  |
| corpus2/heldout/stream_closures | 768 | 0.971 | 1.000 | 1.3% (5 runs) |  |
| lib/m_min | 221 | 0.974 | 1.000 | 20.2% (20 runs) |  |
| lib/it_pairs | 90 | 0.978 | 1.000 | 18.2% (20 runs) |  |
| lib/error_tbl | 216 | 0.978 | 0.997 | 6.1% (20 runs) |  |
| awfy/storage | 114 | 0.978 | 1.000 | 1.4% (5 runs) |  |
| programs/load_big | 441 | 0.978 | 1.000 | 0.8% (2 runs) |  |

### 5.5 plain vs b116447: regressions (instructions > +2% or cycles > +3%, >= 20 Mcyc)

| program | Mcyc | cyc ratio | ins ratio | cycle spread of readings | note |
|---|---|---|---|---|---|
| corpus2/heldout/prototype_objects | 1568 | 1.053 | 1.003 | 89.6% (20 runs) | instructions equal: layout / IPC |
| corpus2/heldout/roguelike_turns | 851 | 1.052 | 1.018 | 3.7% (20 runs) | instructions move the same way |
| lib/gmatch_words | 185 | 1.044 | 1.000 | 12.1% (20 runs) | instructions equal: layout / IPC |
| programs/gc_churn | 2263 | 1.037 | 1.000 | 26.4% (20 runs) | instructions equal: layout / IPC |

### 5.5 PGO vs b116447: regressions (instructions > +2% or cycles > +3%, >= 20 Mcyc)

| program | Mcyc | cyc ratio | ins ratio | cycle spread of readings | note |
|---|---|---|---|---|---|
| corpus/parser_combinator | 1927 | 1.064 | 1.001 | 39.5% (20 runs) | instructions equal: layout / IPC |
| corpus2/heldout/roguelike_turns | 804 | 1.040 | 1.018 | 3.6% (20 runs) | instructions move the same way |
| corpus2/heldout/prototype_objects | 1492 | 1.034 | 1.005 | 44.6% (20 runs) | instructions equal: layout / IPC |
| corpus/penlight_mix | 2164 | 1.024 | 1.027 | 1.7% (5 runs) |  |

### 5.4 plain vs b116447: regressions (instructions > +2% or cycles > +3%, >= 20 Mcyc)

| program | Mcyc | cyc ratio | ins ratio | cycle spread of readings | note |
|---|---|---|---|---|---|
| corpus2/heldout/prototype_objects | 1614 | 1.097 | 0.997 | 100.0% (20 runs) | instructions equal: layout / IPC |
| lib/str_method_len | 56 | 1.097 | 1.000 | 27.1% (20 runs) | instructions equal: layout / IPC |
| lib/len_op | 63 | 1.080 | 1.000 | 68.4% (20 runs) | instructions equal: layout / IPC |
| lib/it_pairs_hash | 121 | 1.072 | 1.000 | 7.9% (20 runs) | instructions equal: layout / IPC |
| corpus2/dev/wordfreq_wrap | 685 | 1.046 | 1.000 | 32.6% (20 runs) | instructions equal: layout / IPC |
| jit-trace/rec_loop | 83 | 1.043 | 1.009 | 62.1% (20 runs) | instructions equal: layout / IPC |
| corpus2/heldout/real_dis_mips | 2278 | 1.039 | 1.022 | 3.3% (20 runs) | instructions move the same way |
| corpus2/dev/diff_levenshtein | 450 | 1.036 | 1.000 | 4.1% (20 runs) | instructions equal: layout / IPC |
| lib/m_sqrt | 287 | 1.035 | 1.000 | 0.1% (2 runs) | instructions equal: layout / IPC |
| lib/str_eq_dyn | 140 | 1.033 | 1.000 | 23.9% (20 runs) | instructions equal: layout / IPC |
| awfy/queens | 83 | 1.030 | 1.006 | 13.4% (20 runs) | instructions equal: layout / IPC |

### 5.4 PGO vs b116447: regressions (instructions > +2% or cycles > +3%, >= 20 Mcyc)

| program | Mcyc | cyc ratio | ins ratio | cycle spread of readings | note |
|---|---|---|---|---|---|
| programs/coroutines | 1427 | 1.065 | 1.000 | 7.2% (20 runs) | instructions equal: layout / IPC |
| lib/load_small | 169 | 1.057 | 1.017 | 5.5% (20 runs) | instructions move the same way |
| programs/metamethods | 761 | 1.054 | 1.005 | 0.3% (2 runs) | instructions equal: layout / IPC |
| jit-trace/rec_loop | 82 | 1.043 | 1.008 | 22.8% (20 runs) | instructions equal: layout / IPC |
| corpus2/heldout/prototype_objects | 1521 | 1.032 | 0.994 | 74.2% (20 runs) | instructions equal: layout / IPC |
| corpus2/heldout/real_dis_mips | 1931 | 1.018 | 1.021 | 5.6% (20 runs) |  |

### 5.5 plain: 15 biggest gaps to LuaJIT (ours/LJ cycles)

| program | Mcyc | cyc/LJ | ins/LJ |
|---|---|---|---|
| awfy/permute | 36 | 2.780 | 3.589 |
| apps/raytrace | 108 | 2.668 | 2.071 |
| awfy/cd | 255 | 2.365 | 2.440 |
| lib/fmt_s | 7 | 2.334 | 2.763 |
| lib/co_yield | 237 | 2.326 | 1.882 |
| corpus/lua_lexer | 982 | 2.192 | 2.067 |
| corpus2/dev/text_adventure | 2576 | 2.171 | 2.170 |
| programs/coroutines | 1786 | 2.049 | 1.789 |
| lib/byte_multi | 41 | 2.037 | 3.571 |
| awfy/towers | 47 | 1.986 | 1.704 |
| corpus/classic_vector | 465 | 1.930 | 1.786 |
| awfy/json | 140 | 1.911 | 2.127 |
| corpus2/heldout/stream_closures | 787 | 1.893 | 2.106 |
| lib/find_plain | 128 | 1.891 | 2.353 |
| lib/getmt | 16 | 1.834 | 2.426 |

### 5.5 plain: 15 biggest wins over LuaJIT (>= 20 Mcyc)

| program | Mcyc | cyc/LJ | ins/LJ |
|---|---|---|---|
| lib/sel_loop | 209 | 0.040 | 0.067 |
| lib/error_tbl | 262 | 0.086 | 0.079 |
| jit-trace/int_rec2 | 677 | 0.135 | 1.551 |
| lib/pcall_err | 119 | 0.156 | 0.136 |
| programs/pcall_error | 1479 | 0.177 | 0.184 |
| lib/tfront_1k | 79 | 0.194 | 0.076 |
| lib/it_next | 45 | 0.247 | 0.703 |
| lib/tonumber_flt | 195 | 0.251 | 0.395 |
| corpus/sha2_base64 | 385 | 0.273 | 0.310 |
| lib/match | 61 | 0.314 | 0.316 |
| jit-trace/int_sum | 889 | 0.354 | 1.272 |
| apps/markdown | 118 | 0.358 | 0.267 |
| lib/find_pat | 262 | 0.372 | 0.451 |
| lib/gsub_str | 165 | 0.414 | 0.499 |
| programs/loops_arith | 477 | 0.430 | 1.020 |

### 5.5 PGO: 15 biggest gaps to LuaJIT (ours/LJ cycles)

| program | Mcyc | cyc/LJ | ins/LJ |
|---|---|---|---|
| awfy/permute | 36 | 2.745 | 3.576 |
| apps/raytrace | 101 | 2.492 | 1.954 |
| awfy/cd | 241 | 2.238 | 2.296 |
| lib/fmt_s | 7 | 2.174 | 2.661 |
| lib/byte_multi | 41 | 2.019 | 3.561 |
| corpus/lua_lexer | 877 | 1.956 | 1.935 |
| awfy/towers | 45 | 1.935 | 1.674 |
| lib/co_yield | 185 | 1.814 | 1.634 |
| corpus2/dev/text_adventure | 2149 | 1.811 | 1.923 |
| lib/getmt | 16 | 1.800 | 2.414 |
| corpus/classic_vector | 432 | 1.794 | 1.694 |
| jit-trace/type_entry | 1507 | 1.776 | 2.396 |
| corpus2/heldout/stream_closures | 724 | 1.740 | 1.948 |
| real/lua_interp | 102 | 1.721 | 2.187 |
| lib/find_plain | 116 | 1.716 | 2.123 |

### 5.5 PGO: 15 biggest wins over LuaJIT (>= 20 Mcyc)

| program | Mcyc | cyc/LJ | ins/LJ |
|---|---|---|---|
| lib/sel_loop | 208 | 0.040 | 0.067 |
| lib/error_tbl | 215 | 0.071 | 0.062 |
| lib/pcall_err | 98 | 0.128 | 0.104 |
| jit-trace/int_rec2 | 677 | 0.135 | 1.551 |
| programs/pcall_error | 1229 | 0.147 | 0.151 |
| lib/tfront_1k | 76 | 0.187 | 0.069 |
| lib/tonumber_flt | 169 | 0.218 | 0.350 |
| lib/it_next | 44 | 0.245 | 0.702 |
| corpus/sha2_base64 | 368 | 0.261 | 0.295 |
| lib/match | 51 | 0.266 | 0.282 |
| apps/markdown | 103 | 0.313 | 0.243 |
| lib/gsub_str | 126 | 0.317 | 0.394 |
| lib/find_pat | 230 | 0.326 | 0.413 |
| jit-trace/int_sum | 888 | 0.353 | 1.272 |
| lib/tpack | 118 | 0.384 | 0.395 |

## Focus programs (cycles in M; ratios of cycles)

| program | plain Mcyc | PGO Mcyc | LJ Mcyc | PUC Mcyc | plain/LJ | PGO/LJ | plain/PUC | PGO/PUC | plain vs b cyc | ins | PGO vs b cyc | ins | plain vs i (b116447) cyc | PGO vs i cyc | PGO/LJ 5a064d7 |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| programs/nbody | 341.7 | 340.6 | 345.6 | 4246.2 | 0.989 | 0.986 | 0.080 | 0.080 | 0.996 | 1.000 | 0.986 | 1.000 | 1.001 | 1.000 | 0.999 |
| programs/fib | 631.3 | 627.8 | 469.7 | 4139.5 | 1.344 | 1.337 | 0.153 | 0.152 | 1.000 | 1.000 | 0.995 | 1.000 | 0.507 | 0.504 | 1.343 |
| programs/binary_trees | 1264.7 | 1062.2 | 2007.4 | 4389.1 | 0.630 | 0.529 | 0.288 | 0.242 | 1.040 | 1.000 | 1.031 | 1.000 | 0.824 | 0.815 | 0.513 |
| programs/queens | 1848.3 | 1844.4 | 1907.1 | 4368.7 | 0.969 | 0.967 | 0.423 | 0.422 | 0.993 | 1.000 | 0.999 | 1.000 | 1.002 | 0.998 | 0.968 |
| apps/raytrace | 108.3 | 101.1 | 40.6 | 262.7 | 2.668 | 2.492 | 0.412 | 0.385 | 0.970 | 0.956 | 0.970 | 0.966 | 0.972 | 0.968 | 2.569 |
| awfy/cd | 254.6 | 240.9 | 107.6 | 437.5 | 2.365 | 2.238 | 0.582 | 0.551 | 0.996 | 0.986 | 1.008 | 0.994 | 0.997 | 1.003 | 2.221 |
| awfy/list | 94.0 | 92.8 | 88.9 | 418.8 | 1.058 | 1.045 | 0.225 | 0.222 | 1.003 | 0.999 | 1.011 | 1.000 | 0.993 | 0.990 | 1.034 |
| awfy/richards | 61.5 | 59.2 | 34.7 | 197.7 | 1.773 | 1.707 | 0.311 | 0.300 | 1.033 | 0.995 | 1.035 | 0.993 | 1.026 | 1.019 | 1.650 |
| awfy/deltablue | 106.7 | 99.0 | 103.5 | 157.9 | 1.032 | 0.957 | 0.676 | 0.627 | 0.998 | 0.982 | 0.993 | 0.989 | 0.994 | 0.986 | 0.963 |
| awfy/havlak | 152.4 | 131.3 | 115.2 | 217.5 | 1.322 | 1.139 | 0.700 | 0.603 | 1.034 | 0.993 | 1.010 | 0.993 | 1.013 | 0.991 | 1.128 |
| awfy/permute | 36.5 | 36.0 | 13.1 | 246.3 | 2.780 | 2.745 | 0.148 | 0.146 | 1.007 | 0.999 | 1.007 | 0.999 | 0.831 | 0.829 | 2.724 |
| awfy/towers | 46.6 | 45.4 | 23.5 | 215.1 | 1.986 | 1.935 | 0.217 | 0.211 | 0.997 | 0.990 | 0.998 | 0.994 | 0.953 | 0.954 | 1.939 |
| corpus/lua_lexer | 982.4 | 876.9 | 448.2 | 3281.3 | 2.192 | 1.956 | 0.299 | 0.267 | 0.910 | 0.894 | 0.915 | 0.892 | 0.910 | 0.917 | 2.138 |
| corpus/classic_vector | 464.6 | 431.7 | 240.7 | 2945.5 | 1.930 | 1.794 | 0.158 | 0.147 | 0.986 | 0.993 | 0.998 | 0.995 | 0.971 | 1.001 | 1.797 |
| corpus2/heldout/dijkstra_routes | 1175.4 | 1144.6 | 1265.1 | 3142.9 | 0.929 | 0.905 | 0.374 | 0.364 | 1.002 | 0.984 | 1.005 | 0.984 | 0.996 | 0.995 | 0.900 |
| corpus2/dev/oop_mixins | 1122.4 | 1073.7 | 865.1 | 2464.3 | 1.298 | 1.241 | 0.455 | 0.436 | 0.805 | 0.710 | 0.791 | 0.707 | 0.774 | 0.791 | 1.570 |
| jit-trace/rec_loop | 82.7 | 81.5 | 141.9 | 1440.3 | 0.583 | 0.575 | 0.057 | 0.057 | 0.897 | 0.856 | 0.893 | 0.855 | 1.014 | 1.011 | 0.643 |
| corpus2/heldout/roguelike_turns | 851.1 | 804.5 | 623.3 | 3311.7 | 1.366 | 1.291 | 0.257 | 0.243 | 1.057 | 1.017 | 1.048 | 1.018 | 1.052 | 1.040 | 1.231 |
| corpus2/heldout/vending_fsm | 978.0 | 895.4 | 824.2 | 3290.3 | 1.187 | 1.086 | 0.297 | 0.272 | 0.960 | 0.946 | 0.950 | 0.945 | 0.957 | 0.958 | 1.144 |
| corpus2/heldout/real_dis_mips | 2199.2 | 1865.6 | 1786.6 | 3808.7 | 1.231 | 1.044 | 0.577 | 0.490 | 1.004 | 1.001 | 0.984 | 0.999 | 0.986 | 0.949 | 1.061 |
| awfy/queens | 95.7 | 93.6 | 55.0 | 401.8 | 1.740 | 1.700 | 0.238 | 0.233 | 0.992 | 0.998 | 0.998 | 0.999 | 0.994 | 0.998 | 1.704 |

## Per-program table (5.5 and 5.4; Mcyc, ratios vs 5a064d7 and LJ)

| program | n runs (plain) | plain | PGO | p54 | g54 | LJ | PUC55 | PUC54 | plain/LJ | PGO/LJ | plain vs b cyc | ins | PGO vs b cyc | ins | p54 vs b cyc | ins | g54 vs b cyc | ins | out/exp (all 4) |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| apps/events | 20 | 162.8 | 144.2 | 174.5 | 148.5 | 95.4 | 293.7 | 289.5 | 1.707 | 1.512 | 0.994 | 0.999 | 1.030 | 0.999 | 0.988 | 0.999 | 0.967 | 0.999 | ok |
| apps/expr_lang | 5 | 133.8 | 122.3 | 149.9 | 136.3 | 96.7 | 241.7 | 253.2 | 1.384 | 1.265 | 0.986 | 0.996 | 0.979 | 0.997 | 0.940 | 0.976 | 0.933 | 0.978 | ok |
| apps/lru_cache | 20 | 142.6 | 130.8 | 179.6 | 169.1 | 106.4 | 219.8 | 253.9 | 1.340 | 1.229 | 0.997 | 0.988 | 0.992 | 0.991 | 1.003 | 0.990 | 0.998 | 0.992 | ok |
| apps/markdown | 2 | 117.9 | 103.2 | 122.1 | 106.5 | 329.7 | 412.6 | 390.8 | 0.358 | 0.313 | 1.011 | 0.998 | 1.015 | 0.999 | 1.011 | 0.999 | 0.995 | 0.999 | ok |
| apps/raytrace | 20 | 108.3 | 101.1 | 112.7 | 106.0 | 40.6 | 262.7 | 281.7 | 2.668 | 2.492 | 0.970 | 0.956 | 0.970 | 0.966 | 0.958 | 0.960 | 0.970 | 0.969 | ok |
| awfy/bounce | 5 | 61.2 | 59.5 | 67.5 | 65.3 | 47.7 | 511.2 | 537.1 | 1.283 | 1.248 | 1.016 | 1.000 | 1.010 | 1.000 | 1.018 | 1.000 | 1.007 | 1.000 | ok |
| awfy/cd | 20 | 254.6 | 240.9 | 283.5 | 265.4 | 107.6 | 437.5 | 464.4 | 2.365 | 2.238 | 0.996 | 0.986 | 1.008 | 0.994 | 1.000 | 0.988 | 1.012 | 0.996 | ok |
| awfy/deltablue | 20 | 106.7 | 99.0 | 116.8 | 109.0 | 103.5 | 157.9 | 165.2 | 1.032 | 0.957 | 0.998 | 0.982 | 0.993 | 0.989 | 0.992 | 0.983 | 0.992 | 0.984 | ok |
| awfy/havlak | 20 | 152.4 | 131.3 | 175.8 | 155.6 | 115.2 | 217.5 | 225.5 | 1.322 | 1.139 | 1.034 | 0.993 | 1.010 | 0.993 | 1.016 | 0.995 | 1.006 | 0.995 | ok |
| awfy/json | 5 | 140.3 | 117.7 | 150.9 | 131.2 | 73.4 | 262.9 | 271.8 | 1.911 | 1.604 | 1.003 | 0.986 | 0.974 | 0.987 | 0.992 | 0.994 | 1.003 | 0.995 | ok |
| awfy/list | 17 | 94.0 | 92.8 | 96.3 | 94.5 | 88.9 | 418.8 | 477.9 | 1.058 | 1.045 | 1.003 | 0.999 | 1.011 | 1.000 | 0.995 | 1.000 | 0.994 | 1.001 | ok |
| awfy/mandelbrot | 2 | 273.2 | 272.7 | 273.3 | 272.5 | 273.1 | 1529.3 | 1379.3 | 1.000 | 0.998 | 1.002 | 1.000 | 1.001 | 1.000 | 1.002 | 1.000 | 1.000 | 1.000 | ok |
| awfy/nbody | 5 | 92.2 | 91.5 | 95.1 | 94.4 | 94.6 | 1504.3 | 1603.8 | 0.975 | 0.967 | 0.994 | 0.999 | 0.999 | 1.000 | 0.995 | 0.999 | 0.998 | 1.000 | ok |
| awfy/permute | 17 | 36.5 | 36.0 | 37.8 | 37.4 | 13.1 | 246.3 | 257.2 | 2.780 | 2.745 | 1.007 | 0.999 | 1.007 | 0.999 | 1.019 | 0.999 | 1.007 | 0.999 | ok |
| awfy/queens | 20 | 95.7 | 93.6 | 83.4 | 80.6 | 55.0 | 401.8 | 403.4 | 1.740 | 1.700 | 0.992 | 0.998 | 0.998 | 0.999 | 1.014 | 1.006 | 1.012 | 1.005 | ok |
| awfy/richards | 20 | 61.5 | 59.2 | 61.5 | 59.2 | 34.7 | 197.7 | 199.3 | 1.773 | 1.707 | 1.033 | 0.995 | 1.035 | 0.993 | 1.030 | 0.996 | 1.029 | 0.993 | ok |
| awfy/sieve | 2 | 124.4 | 123.3 | 127.8 | 127.8 | 156.4 | 1032.3 | 1085.2 | 0.796 | 0.788 | 1.013 | 1.000 | 1.006 | 1.000 | 1.006 | 1.000 | 0.993 | 1.000 | ok |
| awfy/storage | 5 | 100.6 | 77.8 | 146.1 | 113.6 | 154.0 | 255.9 | 262.0 | 0.653 | 0.505 | 0.993 | 1.000 | 0.995 | 1.000 | 1.000 | 1.000 | 1.005 | 1.000 | ok |
| awfy/towers | 20 | 46.6 | 45.4 | 47.3 | 45.7 | 23.5 | 215.1 | 213.9 | 1.986 | 1.935 | 0.997 | 0.990 | 0.998 | 0.994 | 0.987 | 0.991 | 0.988 | 0.994 | ok |
| corpus/astar_jumper | 20 | 1675.8 | 1593.5 | 1874.3 | 1803.2 | 997.4 | 3280.6 | 3358.7 | 1.680 | 1.598 | 1.013 | 0.994 | 1.006 | 0.992 | 1.015 | 0.990 | 1.020 | 0.990 | ok |
| corpus/class_middleclass | 5 | 482.0 | 475.8 | 508.7 | 500.8 | 319.5 | 3046.3 | 3098.9 | 1.508 | 1.489 | 1.005 | 0.999 | 1.017 | 0.999 | 1.002 | 0.999 | 1.013 | 0.999 | ok |
| corpus/classic_vector | 20 | 464.6 | 431.7 | 630.4 | 559.1 | 240.7 | 2945.5 | 2905.4 | 1.930 | 1.794 | 0.986 | 0.993 | 0.998 | 0.995 | 0.970 | 0.995 | 0.994 | 0.997 | ok |
| corpus/containers_lru_heap | 5 | 1298.6 | 1202.7 | 1660.5 | 1558.0 | 1417.3 | 2675.7 | 3035.3 | 0.916 | 0.849 | 1.014 | 0.993 | 1.002 | 0.992 | 0.998 | 0.997 | 1.002 | 0.996 | ok |
| corpus/coro_scheduler | 2 | 1519.7 | 1309.7 | 1618.9 | 1405.1 | 941.9 | 2772.8 | 2851.7 | 1.613 | 1.391 | 1.012 | 1.000 | 1.003 | 1.002 | 1.002 | 1.000 | 1.002 | 1.000 | ok |
| corpus/csv_ini_yaml | 17 | 1545.2 | 1363.8 | 1859.3 | 1599.0 | 1771.4 | 2948.4 | 3020.4 | 0.872 | 0.770 | 0.990 | 1.000 | 1.015 | 1.000 | 0.991 | 1.000 | 1.011 | 1.000 | ok |
| corpus/inspect_lua | 5 | 1566.3 | 1418.8 | 1680.2 | 1525.8 | 1056.4 | 2901.3 | 2968.0 | 1.483 | 1.343 | 1.002 | 1.000 | 1.021 | 1.004 | 1.003 | 1.000 | 1.014 | 0.995 | ok |
| corpus/json_rxi | 20 | 1778.8 | 1506.9 | 1882.1 | 1572.8 | 1222.9 | 3647.9 | 3646.8 | 1.455 | 1.232 | 0.980 | 1.000 | 0.994 | 1.000 | 0.987 | 1.000 | 0.994 | 1.000 | ok |
| corpus/lua_lexer | 20 | 982.4 | 876.9 | 1113.7 | 983.8 | 448.2 | 3281.3 | 3358.7 | 2.192 | 1.956 | 0.910 | 0.894 | 0.915 | 0.892 | 1.003 | 1.000 | 0.991 | 1.001 | ok |
| corpus/markdown_conv | 20 | 1414.0 | 1254.0 | 1406.5 | 1273.3 | 3029.8 | 3746.9 | 3671.3 | 0.467 | 0.414 | 1.021 | 1.000 | 1.008 | 1.001 | 1.001 | 1.000 | 1.010 | 1.000 | ok |
| corpus/msgpack_pure | 20 | 2015.4 | 1752.3 | 2212.9 | 1914.7 | 4473.7 | 3436.5 | 3601.3 | 0.451 | 0.392 | 1.001 | 1.000 | 1.005 | 1.005 | 1.018 | 1.000 | 1.018 | 0.996 | ok |
| corpus/mustache_lustache | 20 | 1812.9 | 1428.1 | 1908.8 | 1552.5 | 1537.1 | 3296.2 | 3446.8 | 1.179 | 0.929 | 1.012 | 1.000 | 0.993 | 1.000 | 0.980 | 1.000 | 0.988 | 1.000 | ok |
| corpus/parser_combinator | 20 | 2313.9 | 1926.5 | 2517.6 | 2039.3 | 1563.1 | 3390.6 | 3391.8 | 1.480 | 1.232 | 1.015 | 1.000 | 1.032 | 1.000 | 1.000 | 1.000 | 0.977 | 1.000 | ok |
| corpus/penlight_mix | 5 | 2311.0 | 2163.7 | 2408.4 | 2258.2 | 2678.7 | 3992.1 | 3970.3 | 0.863 | 0.808 | 1.008 | 1.000 | 1.017 | 1.000 | 1.014 | 1.000 | 1.009 | 1.000 | ok |
| corpus/serpent_ser | 20 | 2017.3 | 1758.2 | 2069.8 | 1802.4 | 2426.7 | 3260.6 | 3254.8 | 0.831 | 0.725 | 1.010 | 0.999 | 1.017 | 0.998 | 0.991 | 0.998 | 1.003 | 0.997 | ok |
| corpus/sha2_base64 | 2 | 384.6 | 368.4 | 386.1 | 369.6 | 1411.3 | 3328.6 | 3384.1 | 0.273 | 0.261 | 0.993 | 1.000 | 1.004 | 1.000 | 0.998 | 0.999 | 0.998 | 0.999 | ok |
| corpus/template_etlua | 20 | 1574.3 | 1249.2 | 1551.8 | 1273.2 | 1826.0 | 2680.1 | 2700.4 | 0.862 | 0.684 | 0.999 | 1.000 | 1.011 | 1.000 | 0.989 | 1.000 | 0.998 | 1.000 | ok |
| corpus/testrunner_luaunit | 2 | 1485.9 | 1225.9 | 1648.9 | 1358.8 | 1390.7 | 2601.9 | 2628.8 | 1.068 | 0.882 | 1.008 | 1.002 | 1.003 | 1.003 | 1.019 | 1.000 | 1.018 | 1.002 | ok |
| corpus2/dev/bignum_arith | 20 | 786.3 | 726.8 | 867.3 | 805.1 | 660.2 | 2352.8 | 2412.4 | 1.191 | 1.101 | 1.001 | 1.000 | 1.002 | 1.000 | 1.004 | 1.000 | 1.001 | 0.999 | ok |
| corpus2/dev/coro_generators | 20 | 1256.3 | 973.0 | 1293.5 | 989.7 | 713.4 | 1807.9 | 1836.1 | 1.761 | 1.364 | 1.003 | 0.997 | 1.011 | 1.000 | 0.996 | 0.999 | 0.997 | 0.999 | ok |
| corpus2/dev/csv_report | 20 | 1159.9 | 1036.7 | 1188.4 | 1088.0 | 1262.6 | 2165.3 | 2381.4 | 0.919 | 0.821 | 1.025 | 1.000 | 1.024 | 0.999 | 1.025 | 1.000 | 1.036 | 0.999 | ok |
| corpus2/dev/diff_levenshtein | 20 | 428.6 | 410.7 | 449.8 | 409.6 | 329.3 | 1776.3 | 1850.8 | 1.301 | 1.247 | 1.004 | 1.000 | 1.018 | 1.000 | 1.036 | 1.000 | 1.022 | 0.999 | ok |
| corpus2/dev/ecs_world | 2 | 948.2 | 848.5 | 997.2 | 859.7 | 916.1 | 2115.3 | 2160.1 | 1.035 | 0.926 | 1.009 | 1.000 | 1.013 | 1.000 | 1.003 | 1.000 | 1.001 | 1.000 | ok |
| corpus2/dev/event_promise | 20 | 384.4 | 367.3 | 466.4 | 443.6 | 467.9 | 1906.6 | 1939.2 | 0.821 | 0.785 | 1.033 | 0.999 | 1.043 | 0.999 | 1.029 | 0.999 | 1.027 | 0.999 | ok |
| corpus2/dev/expr_compiler | 20 | 1307.3 | 1128.8 | 1361.9 | 1179.8 | 1080.2 | 2081.3 | 2231.0 | 1.210 | 1.045 | 0.991 | 0.993 | 1.003 | 0.993 | 1.008 | 0.985 | 1.010 | 0.985 | ok |
| corpus2/dev/iter_pipeline | 17 | 178.8 | 169.1 | 188.0 | 179.8 | 132.2 | 2865.2 | 2874.5 | 1.353 | 1.280 | 1.030 | 0.998 | 1.023 | 0.999 | 1.034 | 0.998 | 1.028 | 0.999 | ok |
| corpus2/dev/lua_parser | 17 | 1298.5 | 1116.3 | 1557.4 | 1335.3 | 790.7 | 2461.5 | 2499.0 | 1.642 | 1.412 | 1.031 | 0.998 | 1.001 | 0.998 | 1.016 | 0.998 | 1.018 | 0.998 | ok |
| corpus2/dev/memo_cache | 20 | 1645.7 | 1515.1 | 1679.3 | 1540.5 | 1674.7 | 2669.5 | 2763.8 | 0.983 | 0.905 | 1.024 | 0.996 | 1.021 | 0.997 | 1.029 | 1.000 | 1.019 | 1.002 | ok |
| corpus2/dev/oop_mixins | 20 | 1122.4 | 1073.7 | 1135.1 | 1071.0 | 865.1 | 2464.3 | 2520.8 | 1.298 | 1.241 | 0.805 | 0.710 | 0.791 | 0.707 | 0.793 | 0.722 | 0.810 | 0.722 | ok |
| corpus2/dev/particles_physics | 20 | 405.8 | 378.7 | 512.9 | 479.0 | 379.7 | 1719.5 | 1811.4 | 1.069 | 0.997 | 1.011 | 0.994 | 1.016 | 0.997 | 1.014 | 0.993 | 1.010 | 0.996 | ok |
| corpus2/dev/real_dis_arm | 20 | 2494.4 | 2207.3 | 2486.7 | 2197.6 | 1947.2 | 4487.3 | 4484.7 | 1.281 | 1.134 | 1.028 | 1.005 | 1.015 | 1.005 | 1.021 | 1.009 | 1.017 | 1.009 | ok |
| corpus2/dev/real_dis_x86 | 2 | 1542.0 | 1366.4 | 1616.5 | 1452.9 | 1591.6 | 2708.6 | 2688.6 | 0.969 | 0.859 | 1.012 | 0.999 | 1.018 | 0.999 | 1.014 | 1.000 | 1.019 | 0.999 | ok |
| corpus2/dev/regex_nfa | 20 | 643.0 | 546.0 | 1345.3 | 1166.6 | 554.5 | 2510.1 | 2439.3 | 1.160 | 0.985 | 0.878 | 0.956 | 0.969 | 0.958 | 0.963 | 1.000 | 0.965 | 1.000 | ok |
| corpus2/dev/template_gsub | 5 | 1693.5 | 1428.4 | 1773.9 | 1505.2 | 2051.0 | 2892.7 | 2885.0 | 0.826 | 0.696 | 0.992 | 1.000 | 1.016 | 1.000 | 1.010 | 1.000 | 1.013 | 1.000 | ok |
| corpus2/dev/text_adventure | 5 | 2576.4 | 2148.7 | 2715.3 | 2210.7 | 1186.6 | 3375.1 | 3513.0 | 2.171 | 1.811 | 1.008 | 1.000 | 1.003 | 1.000 | 1.016 | 1.000 | 0.998 | 1.000 | ok |
| corpus2/dev/varargs_util | 2 | 1526.3 | 1239.2 | 1577.8 | 1268.0 | 1413.9 | 2284.8 | 2301.9 | 1.080 | 0.876 | 1.017 | 1.000 | 0.999 | 1.000 | 1.028 | 1.000 | 1.005 | 1.000 | ok |
| corpus2/dev/wordfreq_wrap | 20 | 611.8 | 506.4 | 685.3 | 562.8 | 658.1 | 1219.2 | 1225.4 | 0.930 | 0.770 | 1.019 | 1.000 | 1.032 | 1.001 | 1.064 | 1.000 | 1.032 | 0.999 | ok |
| corpus2/heldout/bigint_rsa | 17 | 1115.8 | 941.0 | 1399.7 | 1199.0 | 1074.5 | 3033.1 | 3149.9 | 1.038 | 0.876 | 0.993 | 1.000 | 1.000 | 1.000 | 1.023 | 1.000 | 1.006 | 1.000 | ok |
| corpus2/heldout/dijkstra_routes | 20 | 1175.4 | 1144.6 | 1467.6 | 1433.2 | 1265.1 | 3142.9 | 3226.1 | 0.929 | 0.905 | 1.002 | 0.984 | 1.005 | 0.984 | 1.012 | 1.001 | 1.023 | 1.001 | ok |
| corpus2/heldout/forth_vm | 5 | 806.5 | 805.2 | 1011.7 | 1127.5 | 696.1 | 2965.2 | 3269.3 | 1.159 | 1.157 | 1.006 | 0.999 | 1.007 | 0.999 | 0.963 | 0.963 | 0.967 | 0.962 | ok |
| corpus2/heldout/log_analyzer | 20 | 2152.1 | 1788.3 | 2166.4 | 1876.0 | 1652.2 | 3115.0 | 3178.7 | 1.303 | 1.082 | 1.024 | 1.000 | 1.023 | 0.999 | 1.004 | 1.000 | 1.003 | 0.997 | ok |
| corpus2/heldout/markov_text | 2 | 1147.1 | 1120.1 | 1217.0 | 1167.4 | 982.0 | 2862.9 | 2851.2 | 1.168 | 1.141 | 1.016 | 1.000 | 1.020 | 1.000 | 1.023 | 1.000 | 1.020 | 1.000 | ok |
| corpus2/heldout/prototype_objects | 20 | 1567.9 | 1491.6 | 1613.7 | 1520.7 | 1280.6 | 2532.8 | 2721.4 | 1.224 | 1.165 | 1.133 | 1.004 | 1.152 | 1.006 | 1.175 | 0.997 | 1.153 | 0.994 | ok |
| corpus2/heldout/real_dis_mips | 20 | 2199.2 | 1865.6 | 2278.3 | 1930.9 | 1786.6 | 3808.7 | 3869.0 | 1.231 | 1.044 | 1.004 | 1.001 | 0.984 | 0.999 | 1.045 | 1.022 | 1.033 | 1.021 | ok |
| corpus2/heldout/real_dis_ppc | 2 | 2112.8 | 1827.0 | 2230.5 | 1925.6 | 1747.6 | 3961.9 | 3962.8 | 1.209 | 1.045 | 1.014 | 1.000 | 0.983 | 0.999 | 1.006 | 0.997 | 0.995 | 0.997 | ok |
| corpus2/heldout/roguelike_turns | 20 | 851.1 | 804.5 | 872.3 | 830.2 | 623.3 | 3311.7 | 3473.6 | 1.366 | 1.291 | 1.057 | 1.017 | 1.048 | 1.018 | 1.024 | 1.009 | 1.004 | 1.009 | ok |
| corpus2/heldout/scheme_interp | 2 | 1442.1 | 1238.4 | 1531.8 | 1302.5 | 1045.1 | 3052.1 | 3021.0 | 1.380 | 1.185 | 0.999 | 0.999 | 1.024 | 1.000 | 1.013 | 1.000 | 0.998 | 1.000 | ok |
| corpus2/heldout/stream_closures | 5 | 787.4 | 723.9 | 838.6 | 768.4 | 416.0 | 3032.0 | 3163.1 | 1.893 | 1.740 | 1.002 | 0.982 | 1.003 | 0.983 | 0.987 | 0.993 | 0.985 | 0.998 | ok |
| corpus2/heldout/sudoku_solver | 2 | 1317.4 | 1184.2 | 1657.9 | 1505.0 | 1387.6 | 3013.0 | 3102.7 | 0.949 | 0.853 | 1.010 | 0.999 | 1.009 | 1.000 | 1.021 | 0.999 | 1.020 | 0.999 | ok |
| corpus2/heldout/vending_fsm | 20 | 978.0 | 895.4 | 1019.2 | 948.3 | 824.2 | 3290.3 | 3377.2 | 1.187 | 1.086 | 0.960 | 0.946 | 0.950 | 0.945 | 0.932 | 0.887 | 0.938 | 0.881 | ok |
| jit-trace/alloc_ctor | 2 | 188.6 | 157.9 | 260.3 | 205.8 | 276.5 | 1652.1 | 2197.8 | 0.682 | 0.571 | 1.009 | 1.000 | 1.002 | 1.000 | 1.010 | 1.000 | 1.021 | 1.000 | ok |
| jit-trace/array_rec | 2 | 1798.7 | 1800.3 | 1824.9 | 1821.2 | 1755.9 | 9671.5 | 10022.8 | 1.024 | 1.025 | 1.002 | 1.000 | 1.002 | 1.000 | 1.002 | 1.000 | 1.001 | 1.000 | ok |
| jit-trace/dot | 20 | 1220.2 | 1220.3 | 1223.5 | 1222.2 | 1166.0 | 11124.9 | 11807.8 | 1.047 | 1.047 | 1.002 | 1.000 | 1.000 | 1.000 | 1.001 | 1.000 | 0.999 | 1.000 | ok |
| jit-trace/field_global | 20 | 538.9 | 534.8 | 524.5 | 533.8 | 491.7 | 17948.4 | 17879.3 | 1.096 | 1.088 | 0.990 | 1.000 | 0.999 | 1.000 | 0.972 | 1.000 | 1.005 | 1.000 | ok |
| jit-trace/field_method | 2 | 568.5 | 565.9 | 578.1 | 564.8 | 423.1 | 20185.1 | 20271.4 | 1.344 | 1.338 | 1.002 | 1.000 | 1.006 | 1.000 | 1.013 | 1.000 | 0.999 | 1.000 | ok |
| jit-trace/field_rw | 2 | 949.0 | 948.2 | 1033.6 | 1032.3 | 929.3 | 19574.6 | 19882.6 | 1.021 | 1.020 | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | 1.001 | ok |
| jit-trace/float_loop | 2 | 2780.1 | 2777.6 | 2800.3 | 2799.3 | 2575.9 | 11381.5 | 13567.0 | 1.079 | 1.078 | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | 1.002 | 1.000 | ok |
| jit-trace/float_poly | 2 | 1784.4 | 1783.1 | 1817.8 | 1819.6 | 1768.0 | 7918.6 | 8940.5 | 1.009 | 1.009 | 1.001 | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | 1.001 | 1.000 | ok |
| jit-trace/inline_2 | 2 | 327.1 | 326.3 | 327.3 | 325.9 | 262.5 | 11057.3 | 10600.9 | 1.246 | 1.243 | 1.002 | 1.000 | 0.998 | 1.000 | 1.004 | 1.000 | 1.000 | 1.000 | ok |
| jit-trace/inline_leaf | 2 | 458.4 | 455.1 | 455.0 | 456.6 | 630.2 | 11902.9 | 12019.3 | 0.727 | 0.722 | 1.011 | 1.000 | 1.004 | 1.000 | 1.002 | 1.000 | 1.005 | 1.000 | ok |
| jit-trace/int_rec2 | 20 | 676.9 | 676.6 | 682.1 | 683.1 | 5016.4 | 12493.6 | 13006.3 | 0.135 | 0.135 | 0.999 | 1.000 | 1.001 | 1.000 | 0.996 | 1.000 | 0.998 | 1.000 | ok |
| jit-trace/int_sum | 2 | 888.9 | 888.0 | 909.3 | 908.6 | 2512.8 | 9858.5 | 12300.9 | 0.354 | 0.353 | 1.001 | 1.000 | 1.000 | 1.000 | 0.998 | 1.000 | 1.000 | 1.000 | ok |
| jit-trace/licm_expr | 2 | 466.2 | 466.1 | 466.4 | 466.0 | 442.6 | 12323.9 | 12827.3 | 1.053 | 1.053 | 1.000 | 1.000 | 0.999 | 1.000 | 1.001 | 1.000 | 0.998 | 1.000 | ok |
| jit-trace/multret | 20 | 150.2 | 149.7 | 150.4 | 149.6 | 156.7 | 5879.7 | 5782.8 | 0.958 | 0.955 | 0.996 | 0.999 | 1.000 | 0.999 | 1.002 | 0.999 | 0.999 | 0.999 | ok |
| jit-trace/oop_inherit | 2 | 57.2 | 57.0 | 48.9 | 48.4 | 112.8 | 2153.1 | 2201.0 | 0.507 | 0.505 | 0.995 | 1.000 | 0.998 | 1.000 | 1.000 | 1.000 | 0.997 | 1.000 | ok |
| jit-trace/oop_override | 2 | 107.4 | 106.6 | 110.2 | 109.4 | 136.7 | 645.6 | 666.3 | 0.786 | 0.780 | 1.003 | 1.000 | 0.999 | 1.000 | 0.994 | 1.000 | 0.992 | 1.000 | ok |
| jit-trace/rec_loop | 20 | 82.7 | 81.5 | 82.9 | 81.9 | 141.9 | 1440.3 | 1449.9 | 0.583 | 0.575 | 0.897 | 0.856 | 0.893 | 0.855 | 0.934 | 0.857 | 0.931 | 0.857 | ok |
| jit-trace/short_inner | 5 | 1102.0 | 1102.2 | 995.6 | 992.6 | 878.8 | 10393.4 | 11908.6 | 1.254 | 1.254 | 1.001 | 1.000 | 1.001 | 1.000 | 1.002 | 0.999 | 1.000 | 1.000 | ok |
| jit-trace/sink_tuple | 2 | 13.3 | 12.9 | 13.3 | 13.0 | 20.0 | 2743.7 | 3358.1 | 0.665 | 0.647 | 1.002 | 0.999 | 1.008 | 1.000 | 1.018 | 0.999 | 1.011 | 1.000 | ok |
| jit-trace/sink_vec | 5 | 15.3 | 15.1 | 15.3 | 15.0 | 20.0 | 1489.0 | 1832.3 | 0.768 | 0.756 | 0.997 | 1.000 | 1.012 | 1.000 | 1.007 | 1.000 | 1.011 | 1.000 | ok |
| jit-trace/str_concat | 20 | 525.8 | 507.4 | 592.3 | 556.7 | 501.6 | 1628.5 | 1630.2 | 1.048 | 1.012 | 0.990 | 1.000 | 0.996 | 1.000 | 0.997 | 1.000 | 1.003 | 1.000 | ok |
| jit-trace/tail_loop | 2 | 249.8 | 247.3 | 251.1 | 248.9 | 223.3 | 6724.0 | 6643.8 | 1.118 | 1.108 | 1.004 | 0.999 | 0.995 | 0.998 | 1.002 | 0.999 | 0.988 | 0.999 | ok |
| jit-trace/tfor_loops | 20 | 226.1 | 226.8 | 233.2 | 226.8 | 362.3 | 3623.0 | 3352.6 | 0.624 | 0.626 | 1.040 | 1.000 | 1.029 | 1.000 | 1.020 | 1.000 | 1.013 | 1.000 | ok |
| jit-trace/type_entry | 20 | 1511.8 | 1506.9 | 1577.9 | 1557.0 | 848.7 | 11725.8 | 12234.3 | 1.781 | 1.776 | 0.998 | 1.000 | 0.996 | 1.000 | 0.992 | 1.000 | 0.987 | 1.000 | ok |
| jit-trace/type_entry_stable | 20 | 1472.9 | 1468.5 | 1520.6 | 1510.6 | 856.9 | 11690.0 | 12248.4 | 1.719 | 1.714 | 1.016 | 1.000 | 1.013 | 1.000 | 1.008 | 1.000 | 0.992 | 1.000 | ok |
| jit-trace/type_late | 5 | 1520.9 | 1522.7 | 1531.2 | 1534.6 | 1461.5 | 10901.2 | 11418.6 | 1.041 | 1.042 | 1.000 | 1.000 | 1.000 | 1.000 | 1.001 | 1.000 | 1.003 | 1.000 | ok |
| jit-trace/type_late_stable | 20 | 1522.8 | 1520.6 | 1532.3 | 1530.8 | 1461.0 | 10905.6 | 11410.8 | 1.042 | 1.041 | 1.001 | 1.000 | 1.001 | 1.000 | 1.000 | 1.000 | 1.001 | 1.000 | ok |
| jit-trace/upval_acc | 20 | 1381.4 | 1380.9 | 1386.1 | 1384.3 | 1356.5 | 9970.0 | 10500.8 | 1.018 | 1.018 | 1.000 | 1.000 | 0.999 | 1.000 | 1.001 | 1.000 | 0.999 | 1.000 | ok |
| jit-trace/upval_inv | 2 | 2134.8 | 2133.6 | 2138.5 | 2140.2 | 1985.3 | 13886.5 | 14901.3 | 1.075 | 1.075 | 1.001 | 1.000 | 1.000 | 1.000 | 0.998 | 1.000 | 1.000 | 1.000 | ok |
| lib/assert_ok | 20 | 19.0 | 18.8 | 20.4 | 20.2 | 23.7 | 295.9 | 329.8 | 0.804 | 0.793 | 1.006 | 1.000 | 1.006 | 1.000 | 1.008 | 1.000 | 0.989 | 1.000 | ok |
| lib/byte | 2 | 50.3 | 50.0 | 51.9 | 50.4 | 33.0 | 776.0 | 845.1 | 1.524 | 1.515 | 1.000 | 1.000 | 1.003 | 1.000 | 1.020 | 1.000 | 1.001 | 1.000 | ok |
| lib/byte_multi | 5 | 40.9 | 40.6 | 41.7 | 40.9 | 20.1 | 336.9 | 366.2 | 2.037 | 2.019 | 0.998 | 1.000 | 0.983 | 1.000 | 1.017 | 1.000 | 1.004 | 1.000 | ok |
| lib/char | 20 | 152.7 | 140.0 | 155.2 | 144.0 | 104.8 | 383.6 | 387.1 | 1.457 | 1.336 | 1.011 | 1.000 | 1.012 | 1.000 | 0.999 | 1.000 | 1.003 | 1.000 | ok |
| lib/co_wrap | 20 | 245.3 | 201.0 | 237.3 | 196.9 | 228.7 | 381.5 | 338.9 | 1.073 | 0.879 | 1.017 | 0.995 | 1.010 | 0.996 | 0.987 | 1.000 | 1.008 | 0.999 | ok |
| lib/co_yield | 20 | 237.2 | 185.0 | 242.8 | 182.0 | 102.0 | 483.9 | 475.0 | 2.326 | 1.814 | 0.994 | 1.000 | 1.026 | 1.005 | 1.005 | 1.000 | 1.003 | 1.000 | ok |
| lib/concat_op | 2 | 139.8 | 130.1 | 143.3 | 134.7 | 212.9 | 419.4 | 445.3 | 0.656 | 0.611 | 0.997 | 1.000 | 1.006 | 1.000 | 1.007 | 1.000 | 1.010 | 1.000 | ok |
| lib/concat_op_str | 17 | 140.2 | 124.7 | 242.7 | 230.2 | 276.8 | 436.1 | 336.3 | 0.506 | 0.450 | 1.041 | 1.000 | 1.002 | 1.000 | 0.995 | 1.000 | 0.999 | 1.000 | ok |
| lib/error_tbl | 20 | 261.6 | 215.3 | 262.4 | 216.3 | 3042.9 | 619.9 | 625.1 | 0.086 | 0.071 | 0.983 | 1.000 | 0.977 | 1.001 | 0.992 | 1.000 | 0.999 | 0.997 | ok |
| lib/find_cap | 5 | 277.3 | 238.6 | 276.4 | 227.7 | 183.4 | 365.9 | 363.4 | 1.512 | 1.301 | 1.007 | 1.000 | 1.022 | 1.000 | 0.998 | 1.000 | 1.002 | 1.000 | ok |
| lib/find_pat | 20 | 262.1 | 229.9 | 263.8 | 235.7 | 705.2 | 1094.0 | 1092.2 | 0.372 | 0.326 | 1.019 | 1.000 | 1.006 | 1.000 | 0.975 | 1.000 | 0.999 | 1.000 | ok |
| lib/find_plain | 5 | 127.8 | 115.9 | 127.3 | 114.9 | 67.6 | 342.8 | 363.4 | 1.891 | 1.716 | 1.001 | 1.000 | 0.994 | 0.989 | 1.008 | 1.000 | 1.003 | 1.000 | ok |
| lib/fmt_d | 20 | 93.7 | 89.2 | 99.9 | 95.5 | 126.3 | 278.1 | 235.1 | 0.742 | 0.706 | 1.007 | 1.000 | 0.993 | 1.006 | 1.011 | 1.000 | 1.004 | 1.000 | ok |
| lib/fmt_f | 20 | 62.4 | 57.4 | 65.4 | 59.3 | 86.8 | 215.0 | 196.2 | 0.719 | 0.662 | 0.995 | 1.000 | 0.999 | 1.002 | 1.003 | 1.000 | 0.995 | 1.001 | ok |
| lib/fmt_g | 5 | 73.6 | 62.2 | 76.0 | 63.8 | 67.3 | 204.6 | 179.3 | 1.093 | 0.924 | 1.006 | 1.000 | 0.987 | 1.001 | 1.009 | 1.000 | 0.996 | 1.000 | ok |
| lib/fmt_mixed | 20 | 59.4 | 49.6 | 66.2 | 56.2 | 106.2 | 254.5 | 238.3 | 0.559 | 0.467 | 0.999 | 1.000 | 0.987 | 1.001 | 0.985 | 1.000 | 0.995 | 1.001 | ok |
| lib/fmt_q | 2 | 147.1 | 123.0 | 146.7 | 121.5 | 103.3 | 238.6 | 226.1 | 1.425 | 1.191 | 1.026 | 1.000 | 1.015 | 1.000 | 1.001 | 1.000 | 0.999 | 1.000 | ok |
| lib/fmt_s | 5 | 7.4 | 6.9 | 7.5 | 6.9 | 3.2 | 217.6 | 222.2 | 2.334 | 2.174 | 1.010 | 0.999 | 1.002 | 1.000 | 1.006 | 0.999 | 0.998 | 0.999 | ok |
| lib/fmt_x_pad | 2 | 56.3 | 48.8 | 59.7 | 51.6 | 54.0 | 177.1 | 177.5 | 1.042 | 0.903 | 0.999 | 1.000 | 1.020 | 1.004 | 1.013 | 1.000 | 1.014 | 0.999 | ok |
| lib/getmt | 17 | 16.2 | 15.9 | 20.5 | 19.6 | 8.8 | 255.7 | 264.8 | 1.834 | 1.800 | 1.008 | 1.000 | 1.017 | 1.001 | 1.032 | 1.000 | 1.008 | 0.999 | ok |
| lib/gmatch_words | 20 | 185.2 | 135.8 | 180.9 | 137.3 | 173.3 | 315.9 | 319.8 | 1.069 | 0.784 | 1.053 | 1.000 | 1.000 | 1.000 | 1.025 | 1.000 | 1.002 | 1.000 | ok |
| lib/gsub_fn | 2 | 177.7 | 129.1 | 179.0 | 131.2 | 200.2 | 323.4 | 312.5 | 0.887 | 0.645 | 1.017 | 1.000 | 1.004 | 1.005 | 1.000 | 1.000 | 0.995 | 1.000 | ok |
| lib/gsub_str | 2 | 165.0 | 126.3 | 165.2 | 127.4 | 398.5 | 491.6 | 515.6 | 0.414 | 0.317 | 0.999 | 1.000 | 0.999 | 0.998 | 0.992 | 1.000 | 1.010 | 1.000 | ok |
| lib/gsub_tbl | 2 | 164.2 | 106.7 | 154.1 | 110.9 | 187.9 | 321.9 | 321.7 | 0.874 | 0.568 | 1.006 | 1.000 | 0.986 | 1.000 | 1.013 | 1.000 | 1.015 | 1.000 | ok |
| lib/it_ipairs | 20 | 236.6 | 234.2 | 280.0 | 263.0 | 251.6 | 5628.0 | 4827.6 | 0.941 | 0.931 | 0.994 | 1.000 | 1.002 | 1.000 | 1.004 | 1.000 | 1.012 | 1.000 | ok |
| lib/it_next | 20 | 44.6 | 44.2 | 44.4 | 44.3 | 180.5 | 371.1 | 401.4 | 0.247 | 0.245 | 1.007 | 1.000 | 1.004 | 1.000 | 1.008 | 1.000 | 1.015 | 1.000 | ok |
| lib/it_next_empty | 20 | 52.1 | 51.4 | 51.9 | 51.5 | 72.5 | 393.0 | 455.4 | 0.719 | 0.709 | 1.007 | 1.000 | 1.003 | 1.000 | 1.001 | 1.000 | 1.000 | 1.000 | ok |
| lib/it_pairs | 20 | 86.9 | 86.9 | 96.1 | 90.4 | 127.9 | 687.0 | 646.3 | 0.679 | 0.679 | 0.994 | 1.000 | 0.995 | 1.000 | 1.035 | 1.000 | 1.010 | 1.000 | ok |
| lib/it_pairs_hash | 20 | 105.3 | 107.4 | 120.8 | 107.1 | 118.1 | 989.3 | 957.1 | 0.891 | 0.909 | 1.018 | 1.000 | 1.049 | 1.000 | 1.071 | 1.000 | 0.997 | 1.000 | ok |
| lib/len_op | 20 | 48.1 | 47.7 | 62.5 | 57.2 | 100.7 | 1384.7 | 1419.8 | 0.477 | 0.474 | 1.004 | 1.000 | 1.007 | 1.000 | 1.090 | 1.000 | 1.002 | 1.000 | ok |
| lib/len_tbl | 20 | 138.4 | 137.7 | 157.9 | 156.9 | 159.7 | 372.6 | 337.9 | 0.866 | 0.862 | 1.009 | 1.000 | 1.018 | 1.000 | 1.003 | 1.000 | 0.999 | 1.000 | ok |
| lib/load_small | 20 | 226.2 | 147.2 | 222.3 | 168.9 | 215.6 | 378.8 | 345.2 | 1.049 | 0.683 | 0.946 | 1.000 | 0.967 | 1.000 | 0.994 | 1.000 | 1.066 | 1.000 | ok |
| lib/lower | 2 | 88.8 | 80.8 | 91.3 | 83.3 | 110.1 | 289.0 | 294.8 | 0.806 | 0.734 | 1.006 | 1.000 | 1.000 | 1.000 | 1.007 | 1.000 | 1.012 | 1.000 | ok |
| lib/m_abs | 20 | 246.3 | 240.9 | 241.9 | 244.8 | 390.3 | 6061.9 | 6506.8 | 0.631 | 0.617 | 0.994 | 1.000 | 0.985 | 1.000 | 0.986 | 1.000 | 1.006 | 1.000 | ok |
| lib/m_floor | 2 | 226.2 | 226.4 | 227.0 | 226.7 | 206.5 | 3681.1 | 4104.1 | 1.095 | 1.096 | 1.001 | 1.000 | 1.003 | 1.000 | 1.001 | 1.000 | 1.002 | 1.000 | ok |
| lib/m_fmod | 2 | 219.3 | 220.0 | 221.3 | 226.4 | 235.1 | 329.2 | 361.2 | 0.933 | 0.936 | 1.002 | 1.000 | 1.010 | 1.000 | 1.000 | 1.000 | 1.003 | 1.000 | ok |
| lib/m_max | 2 | 256.1 | 256.5 | 256.8 | 256.7 | 401.3 | 8004.8 | 10137.5 | 0.638 | 0.639 | 1.001 | 1.000 | 1.004 | 1.000 | 1.000 | 1.000 | 1.002 | 1.000 | ok |
| lib/m_min | 20 | 226.1 | 228.1 | 221.3 | 221.2 | 387.7 | 6530.7 | 8066.5 | 0.583 | 0.588 | 1.032 | 1.000 | 1.012 | 1.000 | 0.997 | 1.000 | 1.009 | 1.000 | ok |
| lib/m_pow_op | 2 | 280.4 | 280.1 | 280.6 | 280.1 | 275.2 | 1923.5 | 1956.0 | 1.019 | 1.018 | 1.000 | 1.000 | 1.001 | 1.000 | 1.000 | 1.000 | 1.001 | 1.000 | ok |
| lib/m_random | 5 | 79.0 | 78.5 | 78.9 | 78.4 | 73.2 | 345.7 | 375.3 | 1.079 | 1.072 | 0.999 | 1.000 | 1.002 | 1.000 | 1.003 | 1.000 | 1.001 | 1.000 | ok |
| lib/m_random_f | 20 | 53.0 | 52.6 | 52.8 | 52.7 | 89.9 | 272.7 | 312.0 | 0.590 | 0.585 | 1.005 | 1.000 | 1.008 | 1.000 | 1.001 | 1.000 | 1.003 | 1.000 | ok |
| lib/m_sin | 2 | 272.4 | 261.2 | 272.6 | 261.1 | 210.5 | 563.9 | 589.3 | 1.294 | 1.241 | 1.003 | 1.000 | 1.004 | 1.000 | 1.004 | 1.000 | 1.003 | 1.000 | ok |
| lib/m_sqrt | 2 | 276.2 | 272.9 | 286.7 | 282.1 | 270.0 | 2022.6 | 2132.3 | 1.023 | 1.011 | 0.994 | 1.000 | 0.989 | 1.000 | 0.991 | 1.000 | 0.987 | 1.000 | ok |
| lib/m_tointeger | 20 | 223.1 | 222.8 | 236.9 | 236.4 | - | 3718.4 | 3972.3 | - | - | 1.001 | 1.000 | 1.000 | 1.000 | 1.061 | 1.000 | 1.001 | 1.000 | ok |
| lib/m_type | 2 | 32.1 | 31.5 | 32.0 | 31.9 | - | 419.3 | 429.1 | - | - | 1.005 | 1.000 | 0.997 | 1.000 | 0.998 | 1.000 | 1.007 | 1.000 | ok |
| lib/m_ult | 20 | 273.1 | 280.1 | 276.2 | 284.6 | - | 415.2 | 435.8 | - | - | 0.996 | 1.000 | 0.994 | 1.000 | 1.001 | 1.000 | 1.001 | 1.000 | ok |
| lib/match | 20 | 60.7 | 51.4 | 61.1 | 51.8 | 193.6 | 319.9 | 303.8 | 0.314 | 0.266 | 0.990 | 1.000 | 1.011 | 1.000 | 1.006 | 1.000 | 1.004 | 1.000 | ok |
| lib/os_clock | 20 | 116.1 | 121.3 | 118.7 | 120.1 | 100.7 | 119.3 | 118.5 | 1.153 | 1.204 | 1.011 | 1.000 | 1.002 | 1.000 | 1.001 | 1.000 | 0.992 | 1.000 | ok |
| lib/pcall_err | 20 | 119.0 | 97.8 | 128.2 | 108.4 | 764.5 | 308.4 | 309.0 | 0.156 | 0.128 | 0.979 | 1.000 | 0.981 | 1.001 | 0.973 | 1.000 | 0.982 | 0.999 | ok |
| lib/pcall_ok | 2 | 30.1 | 29.4 | 30.9 | 29.6 | 41.0 | 1524.0 | 1518.3 | 0.736 | 0.718 | 0.999 | 1.000 | 0.975 | 1.000 | 1.021 | 1.000 | 0.991 | 1.000 | ok |
| lib/rawget_set | 2 | 19.0 | 18.8 | 19.0 | 18.7 | 14.2 | 381.1 | 417.2 | 1.332 | 1.320 | 0.997 | 1.000 | 0.993 | 1.000 | 1.010 | 1.000 | 1.005 | 1.000 | ok |
| lib/rep | 2 | 109.5 | 98.9 | 118.8 | 110.0 | 89.6 | 347.4 | 333.3 | 1.223 | 1.103 | 1.016 | 1.000 | 0.990 | 1.000 | 0.980 | 1.000 | 0.997 | 1.000 | ok |
| lib/rep_sep | 20 | 89.6 | 81.2 | 117.4 | 109.1 | 103.8 | 329.6 | 311.2 | 0.864 | 0.783 | 1.015 | 1.000 | 1.004 | 1.000 | 1.003 | 1.000 | 1.010 | 1.000 | ok |
| lib/reverse | 2 | 99.6 | 94.1 | 104.8 | 99.1 | 115.3 | 309.8 | 314.7 | 0.864 | 0.816 | 1.000 | 1.000 | 1.008 | 1.000 | 1.014 | 1.000 | 1.012 | 1.000 | ok |
| lib/sel_count | 20 | 215.9 | 215.6 | 223.2 | 219.1 | 388.2 | 15509.5 | 14438.4 | 0.556 | 0.555 | 0.997 | 1.000 | 0.998 | 1.000 | 1.029 | 1.000 | 1.001 | 1.000 | ok |
| lib/sel_loop | 20 | 208.8 | 208.2 | 211.6 | 211.3 | 5263.6 | 53689.2 | 54662.9 | 0.040 | 0.040 | 0.991 | 1.000 | 0.994 | 1.000 | 0.991 | 1.000 | 0.998 | 1.000 | ok |
| lib/sel_n | 20 | 323.6 | 337.5 | 325.4 | 324.1 | 675.4 | 25345.4 | 23674.6 | 0.479 | 0.500 | 0.963 | 1.000 | 1.047 | 1.000 | 0.994 | 1.000 | 1.003 | 1.000 | ok |
| lib/setmt | 5 | 185.9 | 159.7 | 228.7 | 194.3 | 331.9 | 743.5 | 835.9 | 0.560 | 0.481 | 1.011 | 1.000 | 1.003 | 1.000 | 1.003 | 1.000 | 1.015 | 1.000 | ok |
| lib/str_eq | 20 | 159.9 | 159.6 | 160.1 | 160.0 | 182.5 | 1479.5 | 1529.4 | 0.876 | 0.875 | 1.000 | 1.000 | 0.998 | 1.000 | 0.998 | 1.000 | 1.001 | 1.000 | ok |
| lib/str_eq_dyn | 20 | 134.0 | 134.7 | 139.7 | 134.6 | 150.4 | 1277.3 | 1348.8 | 0.891 | 0.896 | 0.992 | 1.000 | 1.014 | 1.000 | 1.045 | 1.000 | 1.013 | 1.000 | ok |
| lib/str_eq_long | 20 | 35.6 | 35.5 | 35.6 | 35.0 | 21.5 | 215.5 | 222.1 | 1.656 | 1.653 | 0.997 | 1.000 | 1.012 | 1.000 | 0.997 | 1.000 | 0.987 | 1.000 | ok |
| lib/str_idx_concat_key | 20 | 204.8 | 190.4 | 205.5 | 192.5 | 166.6 | 464.0 | 461.2 | 1.229 | 1.143 | 0.998 | 1.000 | 0.999 | 1.000 | 0.998 | 1.000 | 0.999 | 1.000 | ok |
| lib/str_lt | 17 | 93.7 | 90.8 | 89.8 | 87.5 | 66.3 | 227.1 | 228.3 | 1.412 | 1.368 | 1.003 | 1.000 | 0.997 | 1.000 | 1.009 | 1.000 | 1.012 | 1.000 | ok |
| lib/str_lt_short | 2 | 113.3 | 105.7 | 105.1 | 98.0 | 62.5 | 249.8 | 255.3 | 1.812 | 1.690 | 0.997 | 1.000 | 0.994 | 1.000 | 1.004 | 1.000 | 0.998 | 1.000 | ok |
| lib/str_method_len | 20 | 45.6 | 45.0 | 56.4 | 50.2 | 55.3 | 1386.6 | 1429.7 | 0.825 | 0.815 | 1.002 | 1.000 | 0.994 | 1.000 | 1.108 | 1.000 | 0.982 | 1.000 | ok |
| lib/string_key_const | 2 | 176.7 | 176.5 | 201.8 | 200.5 | 176.9 | 1151.6 | 1165.1 | 0.999 | 0.998 | 0.999 | 1.000 | 1.002 | 1.000 | 1.009 | 1.000 | 1.004 | 1.000 | ok |
| lib/string_key_dyn | 2 | 228.1 | 227.5 | 229.8 | 226.7 | 268.0 | 1082.3 | 1101.0 | 0.851 | 0.849 | 0.995 | 1.000 | 0.995 | 1.000 | 1.007 | 1.000 | 0.993 | 1.000 | ok |
| lib/sub | 2 | 130.9 | 121.9 | 130.3 | 120.7 | 80.5 | 469.5 | 482.7 | 1.626 | 1.514 | 0.995 | 1.000 | 0.955 | 1.000 | 0.994 | 1.000 | 0.970 | 1.000 | ok |
| lib/sub_1char | 20 | 118.3 | 109.2 | 120.6 | 108.2 | 98.2 | 459.2 | 478.8 | 1.205 | 1.112 | 0.985 | 1.000 | 0.998 | 1.000 | 0.992 | 1.000 | 0.997 | 1.000 | ok |
| lib/tconcat | 20 | 145.0 | 99.7 | 170.0 | 120.0 | 133.4 | 346.9 | 322.3 | 1.087 | 0.748 | 1.004 | 1.001 | 0.999 | 1.001 | 1.005 | 1.001 | 0.996 | 1.001 | ok |
| lib/tconcat_num | 2 | 178.8 | 117.2 | 180.7 | 121.4 | 246.0 | 908.0 | 844.8 | 0.727 | 0.477 | 1.000 | 1.000 | 0.981 | 1.000 | 0.975 | 1.000 | 0.993 | 1.000 | ok |
| lib/tfront_1k | 5 | 78.9 | 76.0 | 81.5 | 77.1 | 406.3 | 4375.7 | 4241.0 | 0.194 | 0.187 | 0.997 | 1.000 | 0.998 | 1.001 | 0.996 | 1.000 | 1.013 | 1.000 | ok |
| lib/tinsert_append | 20 | 158.1 | 134.3 | 194.1 | 181.5 | 141.7 | 539.7 | 574.7 | 1.115 | 0.948 | 0.992 | 1.000 | 1.010 | 1.001 | 1.011 | 1.000 | 1.003 | 1.000 | ok |
| lib/tinsert_front | 20 | 227.5 | 179.1 | 262.8 | 209.2 | 272.9 | 455.4 | 493.5 | 0.834 | 0.656 | 1.016 | 1.000 | 1.004 | 1.001 | 1.001 | 1.000 | 1.000 | 1.000 | ok |
| lib/tinsert_idx | 20 | 174.3 | 151.1 | 204.3 | 191.3 | 154.6 | 328.8 | 383.3 | 1.128 | 0.977 | 0.993 | 0.999 | 0.993 | 0.999 | 0.995 | 1.000 | 1.008 | 1.000 | ok |
| lib/tmove | 5 | 103.3 | 102.8 | 124.6 | 124.0 | 105.8 | 700.5 | 683.1 | 0.976 | 0.972 | 1.003 | 1.000 | 1.001 | 1.000 | 1.000 | 1.000 | 0.992 | 1.000 | ok |
| lib/tmove_1k | 2 | 95.8 | 70.4 | 107.7 | 72.0 | 111.6 | 476.4 | 447.3 | 0.859 | 0.631 | 0.950 | 1.000 | 1.029 | 1.000 | 0.972 | 1.000 | 0.964 | 1.000 | ok |
| lib/tonumber_bad | 20 | 148.7 | 111.3 | 163.5 | 141.7 | 168.0 | 408.1 | 431.1 | 0.885 | 0.663 | 0.954 | 1.000 | 0.921 | 1.000 | 0.981 | 1.000 | 0.987 | 1.000 | ok |
| lib/tonumber_base | 5 | 265.5 | 259.4 | 275.4 | 260.7 | 229.2 | 432.1 | 451.7 | 1.158 | 1.132 | 1.001 | 1.000 | 0.998 | 1.000 | 1.014 | 1.000 | 0.988 | 1.000 | ok |
| lib/tonumber_flt | 20 | 194.6 | 169.0 | 195.7 | 167.2 | 776.7 | 577.4 | 596.1 | 0.251 | 0.218 | 1.001 | 1.000 | 0.985 | 1.000 | 0.993 | 1.000 | 0.998 | 1.000 | ok |
| lib/tonumber_int | 5 | 153.0 | 140.4 | 162.9 | 142.4 | 96.2 | 288.1 | 305.0 | 1.591 | 1.460 | 0.987 | 1.000 | 0.997 | 1.000 | 1.016 | 1.000 | 1.001 | 1.000 | ok |
| lib/tonumber_num | 5 | 211.3 | 214.7 | 214.4 | 211.3 | 383.1 | 6476.7 | 7506.3 | 0.552 | 0.561 | 0.983 | 1.000 | 1.015 | 1.000 | 0.985 | 1.000 | 0.997 | 1.000 | ok |
| lib/tostring_flt | 20 | 84.2 | 66.0 | 83.7 | 61.2 | 82.5 | 299.3 | 214.9 | 1.020 | 0.800 | 1.017 | 1.000 | 1.002 | 1.000 | 1.003 | 1.000 | 1.000 | 1.000 | ok |
| lib/tostring_int | 20 | 121.9 | 108.5 | 135.3 | 122.4 | 279.8 | 543.3 | 450.7 | 0.436 | 0.388 | 0.999 | 1.000 | 0.995 | 1.000 | 1.004 | 1.000 | 1.000 | 1.000 | ok |
| lib/tostring_tbl | 2 | 108.8 | 106.5 | 111.6 | 109.1 | 93.3 | 313.8 | 280.9 | 1.165 | 1.141 | 1.007 | 1.000 | 0.984 | 1.000 | 1.000 | 1.000 | 1.014 | 1.000 | ok |
| lib/tpack | 5 | 146.0 | 117.5 | 211.6 | 165.3 | 306.1 | 499.0 | 485.7 | 0.477 | 0.384 | 0.981 | 1.000 | 0.999 | 1.000 | 1.003 | 1.000 | 0.986 | 1.000 | ok |
| lib/tremove_front | 5 | 148.5 | 133.1 | 185.6 | 168.3 | 191.0 | 592.3 | 593.7 | 0.777 | 0.697 | 1.007 | 1.000 | 1.004 | 1.000 | 0.993 | 1.000 | 0.982 | 1.000 | ok |
| lib/tremove_pop | 20 | 240.6 | 172.4 | 223.4 | 206.4 | 136.7 | 483.9 | 552.9 | 1.760 | 1.261 | 1.001 | 1.000 | 0.998 | 1.000 | 0.996 | 1.000 | 0.984 | 1.000 | ok |
| lib/tsort_cmp | 20 | 264.9 | 256.2 | 281.0 | 260.1 | 285.5 | 339.9 | 336.1 | 0.928 | 0.897 | 0.999 | 1.000 | 0.995 | 1.002 | 1.025 | 1.000 | 0.998 | 1.000 | ok |
| lib/tsort_int | 2 | 275.0 | 217.9 | 296.9 | 222.2 | 294.0 | 382.4 | 406.0 | 0.935 | 0.741 | 1.025 | 0.999 | 1.014 | 0.999 | 1.014 | 0.999 | 0.992 | 0.998 | ok |
| lib/tsort_str | 2 | 258.9 | 150.6 | 295.7 | 159.8 | 244.8 | 364.6 | 401.0 | 1.058 | 0.615 | 1.010 | 1.001 | 0.989 | 1.001 | 1.010 | 1.000 | 0.998 | 1.000 | ok |
| lib/tunpack | 5 | 124.9 | 103.9 | 127.3 | 113.5 | 154.8 | 487.1 | 448.5 | 0.807 | 0.671 | 1.005 | 1.000 | 1.004 | 1.000 | 1.003 | 1.000 | 1.011 | 1.000 | ok |
| lib/type_call | 2 | 87.9 | 86.7 | 95.5 | 94.1 | 85.6 | 494.3 | 468.3 | 1.027 | 1.013 | 1.012 | 0.999 | 1.000 | 1.000 | 1.018 | 0.999 | 1.006 | 1.000 | ok |
| lib/upper | 5 | 95.2 | 89.0 | 100.1 | 94.4 | 120.7 | 307.4 | 312.6 | 0.789 | 0.738 | 0.993 | 1.000 | 0.981 | 1.000 | 1.002 | 1.000 | 0.996 | 1.000 | ok |
| lib/utf8_len | 20 | 270.8 | 275.8 | 277.7 | 279.5 | - | 442.2 | 456.7 | - | - | 0.996 | 1.000 | 1.004 | 1.000 | 1.013 | 1.000 | 0.999 | 1.000 | ok |
| programs/binary_trees | 20 | 1264.7 | 1062.2 | 1459.5 | 1198.8 | 2007.4 | 4389.1 | 4447.3 | 0.630 | 0.529 | 1.040 | 1.000 | 1.031 | 1.000 | 1.019 | 1.000 | 1.030 | 1.000 | ok |
| programs/closures | 20 | 454.5 | 423.4 | 519.8 | 474.7 | 949.8 | 5063.2 | 4986.1 | 0.479 | 0.446 | 0.986 | 1.000 | 1.003 | 1.000 | 1.015 | 1.000 | 0.990 | 1.000 | ok |
| programs/coroutines | 20 | 1786.4 | 1340.4 | 1846.8 | 1427.5 | 871.7 | 3733.4 | 3656.9 | 2.049 | 1.538 | 0.991 | 1.000 | 1.032 | 1.003 | 0.995 | 1.000 | 1.010 | 1.000 | ok |
| programs/fannkuch | 2 | 1051.5 | 1049.4 | 1075.3 | 1073.9 | 1075.7 | 4075.1 | 4114.8 | 0.978 | 0.976 | 1.003 | 0.999 | 1.009 | 1.000 | 1.009 | 1.000 | 1.002 | 1.000 | ok |
| programs/fasta | 20 | 1280.9 | 1159.9 | 1387.5 | 1240.3 | 1495.7 | 3776.8 | 3842.3 | 0.856 | 0.775 | 1.007 | 1.000 | 1.005 | 1.000 | 1.026 | 1.000 | 1.008 | 1.000 | ok |
| programs/fib | 20 | 631.3 | 627.8 | 627.5 | 626.2 | 469.7 | 4139.5 | 4150.5 | 1.344 | 1.337 | 1.000 | 1.000 | 0.995 | 1.000 | 0.996 | 1.000 | 0.996 | 1.000 | ok |
| programs/gc_churn | 20 | 2263.1 | 1854.1 | 3391.7 | 2884.7 | 3608.7 | 4605.5 | 5988.9 | 0.627 | 0.514 | 1.060 | 1.000 | 1.049 | 1.000 | 1.057 | 1.000 | 1.045 | 1.000 | ok |
| programs/heapsort | 20 | 1069.3 | 1074.7 | 2261.0 | 2264.0 | 1286.6 | 4063.2 | 3957.0 | 0.831 | 0.835 | 1.021 | 0.953 | 1.025 | 0.953 | 1.022 | 0.995 | 1.026 | 0.996 | ok |
| programs/json | 20 | 1817.8 | 1389.4 | 1965.3 | 1560.1 | 1470.3 | 3127.6 | 3229.9 | 1.236 | 0.945 | 1.001 | 1.000 | 0.968 | 1.000 | 1.001 | 1.000 | 1.021 | 1.000 | ok |
| programs/knucleotide | 2 | 1033.9 | 1067.8 | 1054.4 | 1058.8 | 691.4 | 4061.5 | 4161.0 | 1.495 | 1.544 | 0.994 | 1.000 | 1.002 | 1.000 | 0.997 | 1.000 | 1.001 | 1.000 | ok |
| programs/load_big | 2 | 744.4 | 550.9 | 638.9 | 441.4 | 491.1 | 1787.3 | 620.0 | 1.516 | 1.122 | 0.987 | 1.000 | 1.014 | 1.000 | 0.965 | 1.000 | 1.006 | 1.000 | CHECK |
| programs/loops_arith | 2 | 476.5 | 476.4 | 480.2 | 479.6 | 1108.2 | 4344.5 | 4909.6 | 0.430 | 0.430 | 0.999 | 1.000 | 0.997 | 1.000 | 1.000 | 1.000 | 0.999 | 1.000 | CHECK |
| programs/mandelbrot | 2 | 532.7 | 532.3 | 533.2 | 532.8 | 521.6 | 4727.1 | 5530.2 | 1.021 | 1.021 | 1.000 | 1.000 | 1.000 | 1.000 | 0.999 | 1.000 | 0.999 | 1.000 | ok |
| programs/matmul | 5 | 436.6 | 420.0 | 434.6 | 441.1 | 272.0 | 3171.0 | 3693.4 | 1.605 | 1.544 | 0.991 | 1.000 | 0.965 | 1.000 | 0.977 | 1.000 | 0.994 | 1.000 | ok |
| programs/metamethods | 2 | 660.3 | 616.1 | 790.8 | 761.3 | 540.1 | 4614.3 | 4669.9 | 1.222 | 1.141 | 1.002 | 1.000 | 1.002 | 1.000 | 0.998 | 1.000 | 1.018 | 1.000 | ok |
| programs/nbody | 17 | 341.7 | 340.6 | 351.5 | 350.6 | 345.6 | 4246.2 | 4718.3 | 0.989 | 0.986 | 0.996 | 1.000 | 0.986 | 1.000 | 0.988 | 1.000 | 1.000 | 1.000 | ok |
| programs/oop_methods | 2 | 142.9 | 142.7 | 142.9 | 142.3 | 304.8 | 4567.3 | 4685.2 | 0.469 | 0.468 | 1.003 | 1.000 | 1.003 | 1.000 | 1.010 | 1.000 | 1.009 | 1.000 | ok |
| programs/pcall_error | 5 | 1479.4 | 1229.1 | 1473.4 | 1227.2 | 8346.6 | 4028.8 | 4363.2 | 0.177 | 0.147 | 0.993 | 1.000 | 0.990 | 1.001 | 0.991 | 1.000 | 0.995 | 0.999 | ok |
| programs/queens | 17 | 1848.3 | 1844.4 | 1928.0 | 1927.0 | 1907.1 | 4368.7 | 4120.8 | 0.969 | 0.967 | 0.993 | 1.000 | 0.999 | 1.000 | 1.002 | 1.000 | 1.002 | 1.000 | ok |
| programs/sieve | 20 | 579.4 | 577.4 | 613.9 | 679.9 | 761.7 | 2750.2 | 5029.1 | 0.761 | 0.758 | 1.000 | 1.000 | 0.990 | 1.000 | 1.002 | 1.000 | 1.001 | 1.000 | ok |
| programs/spectral_norm | 5 | 182.0 | 179.9 | 187.2 | 184.6 | 171.1 | 4573.7 | 4337.9 | 1.064 | 1.052 | 1.008 | 1.000 | 0.995 | 1.000 | 1.010 | 1.000 | 1.000 | 1.000 | ok |
| programs/string_ops | 20 | 1118.2 | 1041.7 | 1069.5 | 974.1 | 2238.4 | 2828.0 | 2807.1 | 0.500 | 0.465 | 1.020 | 1.000 | 1.018 | 1.002 | 1.013 | 1.000 | 1.004 | 1.000 | ok |
| programs/table_array | 2 | 362.1 | 300.5 | 408.2 | 310.8 | 600.0 | 3400.3 | 3492.4 | 0.604 | 0.501 | 1.010 | 1.000 | 1.000 | 1.001 | 1.016 | 1.000 | 0.992 | 1.000 | ok |
| programs/table_hash | 20 | 494.6 | 479.3 | 490.6 | 490.7 | 394.4 | 1501.6 | 1831.0 | 1.254 | 1.215 | 1.015 | 1.000 | 1.006 | 1.000 | 1.004 | 1.000 | 1.015 | 1.000 | ok |
| programs/varargs | 2 | 1245.2 | 1100.5 | 1225.7 | 1100.9 | 2194.8 | 5366.5 | 5084.7 | 0.567 | 0.501 | 1.012 | 1.000 | 0.998 | 1.000 | 1.021 | 1.000 | 1.003 | 1.000 | ok |
| real/game_loop | 2 | 34.4 | 33.3 | 39.4 | 37.6 | 21.7 | 207.6 | 221.0 | 1.582 | 1.534 | 0.995 | 0.996 | 0.993 | 0.997 | 1.007 | 0.997 | 0.991 | 0.998 | ok |
| real/json_roundtrip | 20 | 547.0 | 451.3 | 581.5 | 472.4 | 469.1 | 796.8 | 802.8 | 1.166 | 0.962 | 1.001 | 1.000 | 0.988 | 1.000 | 1.001 | 1.000 | 1.003 | 1.000 | ok |
| real/lua_interp | 5 | 105.0 | 102.0 | 104.8 | 101.1 | 59.3 | 223.6 | 225.0 | 1.770 | 1.721 | 0.995 | 0.998 | 1.007 | 0.999 | 1.001 | 0.998 | 0.998 | 0.998 | ok |
| real/template_engine | 20 | 717.2 | 595.7 | 738.9 | 620.8 | 955.0 | 1225.1 | 1249.2 | 0.751 | 0.624 | 1.004 | 1.001 | 1.010 | 1.001 | 0.978 | 1.000 | 0.991 | 1.001 | ok |
| real/wordfreq | 2 | 112.7 | 96.4 | 113.8 | 97.5 | 116.4 | 212.8 | 210.2 | 0.968 | 0.829 | 0.995 | 1.000 | 1.002 | 1.000 | 0.997 | 1.000 | 1.007 | 0.999 | ok |

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
