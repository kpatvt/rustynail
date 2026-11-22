# Tables, session-k (subject 5a064d7)

Ratios below 1: first column faster/fewer. Ours = min over all readings (2 quick, 5 full, 15-run serial reruns). Cached columns as stored.


# Aggregates, readings A: the fast pass only (min of 2 or 5 runs per program)

## Headline geomeans (cycles, ours/ref)

| build | /LuaJIT cyc (n) | /LuaJIT ins | /PUC same version cyc (223) | /PUC ins |
|---|---|---|---|---|
| 5.5 plain | 0.948 (219) | 1.344 | 0.216 | 0.220 |
| 5.5 PGO | 0.866 (219) | 1.240 | 0.198 | 0.203 |
| 5.4 plain | 1.010 (219) | 1.436 | 0.225 | 0.234 |
| 5.4 PGO | 0.924 (219) | 1.325 | 0.207 | 0.217 |

- 5.5 plain / LuaJIT -joff (219): 0.320
- 5.5 PGO / LuaJIT -joff (219): 0.293
- b116447 same-session (set B) (same programs): 5.5 plain/LJ 0.948, PGO/LJ 0.866, 5.4 plain/LJ 1.007, 5.4 PGO/LJ 0.923
- session-j published b116447 (same programs): 5.5 plain/LJ 0.952, PGO/LJ 0.868, 5.4 plain/LJ 1.011, 5.4 PGO/LJ 0.926
- 22670a1 session-i (same programs): 5.5 plain/LJ 0.950, PGO/LJ 0.869, 5.4 plain/LJ 1.010, 5.4 PGO/LJ 0.925
- cached 8c39465 (same programs): 5.5 plain/LJ 0.955, PGO/LJ 0.871, 5.4 plain/LJ 1.020, 5.4 PGO/LJ 0.929
- cached 1a5aad0 (same programs): 5.5 plain/LJ 0.964, PGO/LJ 0.879, 5.4 plain/LJ 1.020, 5.4 PGO/LJ 0.934
- cached 108b935 (same programs): plain/LJ 0.969, PGO/LJ 0.883
- our 5.4 build / our 5.5 build cycles: plain 1.064, PGO 1.066; PGO/plain: 5.5 0.915, 5.4 0.917

## Subject vs b116447 (same session, set B), b116447 set A, session-j published, 22670a1, 8c39465, 1a5aad0 and 108b935 (geomean, 223 programs)

| build | vs | cycles | instructions | cyc >3% better / within / >3% worse | ins >3% fewer / more |
|---|---|---|---|---|---|
| 5.5 plain | b116447 | 1.000 | 0.996 | 5 / 207 / 11 | 3 / 1 |
| 5.5 plain | b116447 setA | 0.997 | 0.996 | 7 / 208 / 8 | 3 / 1 |
| 5.5 plain | session-j published | 0.996 | 0.996 | 13 / 203 / 7 | 3 / 1 |
| 5.5 plain | 22670a1 | 0.998 | 0.989 | 9 / 197 / 17 | 14 / 1 |
| 5.5 plain | 8c39465 | 0.993 | 0.982 | 16 / 191 / 16 | 24 / 2 |
| 5.5 plain | 1a5aad0 | 0.984 | 0.965 | 40 / 157 / 26 | 41 / 1 |
| 5.5 plain | 108b935 | 0.978 | 0.944 | 47 / 150 / 26 | 93 / 0 |
| 5.5 PGO | b116447 | 0.999 | 0.996 | 9 / 206 / 8 | 3 / 1 |
| 5.5 PGO | b116447 setA | 0.997 | 0.996 | 12 / 205 / 6 | 3 / 1 |
| 5.5 PGO | session-j published | 0.998 | 0.996 | 8 / 210 / 5 | 3 / 1 |
| 5.5 PGO | 22670a1 | 0.996 | 0.990 | 11 / 202 / 10 | 13 / 1 |
| 5.5 PGO | 8c39465 | 0.994 | 0.983 | 15 / 193 / 15 | 23 / 2 |
| 5.5 PGO | 1a5aad0 | 0.985 | 0.964 | 30 / 178 / 15 | 42 / 1 |
| 5.5 PGO | 108b935 | 0.981 | 0.943 | 48 / 145 / 30 | 95 / 1 |
| 5.4 plain | b116447 | 1.002 | 0.996 | 6 / 206 / 11 | 3 / 1 |
| 5.4 plain | b116447 setA | 1.000 | 0.996 | 7 / 208 / 8 | 3 / 1 |
| 5.4 plain | session-j published | 0.998 | 0.996 | 9 / 204 / 10 | 3 / 1 |
| 5.4 plain | 22670a1 | 0.999 | 0.993 | 12 / 191 / 20 | 10 / 1 |
| 5.4 plain | 8c39465 | 0.990 | 0.984 | 20 / 185 / 18 | 18 / 4 |
| 5.4 plain | 1a5aad0 | 0.990 | 0.971 | 32 / 163 / 28 | 34 / 3 |
| 5.4 PGO | b116447 | 1.000 | 0.996 | 6 / 205 / 12 | 3 / 1 |
| 5.4 PGO | b116447 setA | 0.999 | 0.996 | 8 / 205 / 10 | 3 / 1 |
| 5.4 PGO | session-j published | 0.998 | 0.996 | 9 / 203 / 11 | 3 / 1 |
| 5.4 PGO | 22670a1 | 0.999 | 0.993 | 10 / 205 / 8 | 9 / 1 |
| 5.4 PGO | 8c39465 | 0.994 | 0.985 | 16 / 194 / 13 | 17 / 4 |
| 5.4 PGO | 1a5aad0 | 0.989 | 0.970 | 26 / 176 / 21 | 35 / 3 |

### Per set, geomean subject / b116447 and subject / 22670a1 (cycles / instructions)

| set | n | plain vs b cyc | ins | PGO vs b cyc | ins | 5.4 plain vs b cyc | ins | 5.4 PGO vs b cyc | ins | plain vs i cyc | ins | PGO vs i cyc | ins | 5.4 plain vs i cyc | ins | 5.4 PGO vs i cyc | ins |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| apps | 5 | 1.009 | 1.001 | 1.012 | 1.003 | 1.009 | 1.001 | 1.009 | 1.000 | 1.003 | 0.975 | 1.001 | 0.982 | 1.000 | 0.977 | 0.997 | 0.980 |
| awfy | 14 | 0.997 | 0.984 | 0.992 | 0.983 | 1.004 | 0.986 | 0.998 | 0.985 | 0.987 | 0.972 | 0.986 | 0.973 | 0.994 | 0.974 | 0.989 | 0.976 |
| corpus | 18 | 1.006 | 1.001 | 1.019 | 1.003 | 1.015 | 1.000 | 1.015 | 1.000 | 0.968 | 0.969 | 0.976 | 0.977 | 0.992 | 0.986 | 0.994 | 0.989 |
| corpus2/dev | 17 | 1.008 | 0.999 | 1.014 | 1.000 | 1.011 | 0.999 | 1.005 | 0.999 | 1.013 | 0.994 | 1.009 | 0.995 | 1.005 | 0.998 | 1.002 | 0.998 |
| corpus2/heldout | 11 | 1.017 | 1.000 | 1.001 | 1.000 | 1.006 | 1.000 | 1.006 | 1.000 | 0.990 | 0.955 | 0.981 | 0.956 | 1.001 | 0.992 | 0.999 | 0.992 |
| jit-trace | 29 | 1.003 | 1.006 | 1.004 | 1.006 | 1.004 | 1.006 | 1.005 | 1.006 | 1.005 | 1.005 | 1.005 | 1.005 | 1.006 | 1.005 | 1.006 | 1.006 |
| lib | 95 | 1.002 | 1.000 | 1.000 | 1.001 | 1.003 | 1.000 | 1.001 | 1.000 | 1.006 | 1.000 | 1.005 | 1.001 | 1.005 | 1.000 | 1.005 | 1.000 |
| programs | 25 | 0.972 | 0.963 | 0.975 | 0.962 | 0.977 | 0.964 | 0.976 | 0.963 | 0.976 | 0.963 | 0.970 | 0.961 | 0.974 | 0.964 | 0.969 | 0.963 |
| real | 5 | 0.994 | 1.001 | 0.984 | 1.002 | 1.000 | 1.002 | 0.997 | 1.002 | 1.004 | 0.997 | 1.005 | 0.999 | 1.000 | 1.000 | 0.998 | 1.000 |
| real_dis | 4 | 1.005 | 1.001 | 0.979 | 1.003 | 1.012 | 1.000 | 1.008 | 1.000 | 0.993 | 0.985 | 0.997 | 0.985 | 1.008 | 1.000 | 1.014 | 1.000 |
| ALL | 223 | 1.000 | 0.996 | 0.999 | 0.996 | 1.002 | 0.996 | 1.000 | 0.996 | 0.998 | 0.989 | 0.996 | 0.990 | 0.999 | 0.993 | 0.999 | 0.993 |

# Aggregates, readings B: with the 15-run serial reruns of 83 programs merged (min over all readings)

## Headline geomeans (cycles, ours/ref)

| build | /LuaJIT cyc (n) | /LuaJIT ins | /PUC same version cyc (223) | /PUC ins |
|---|---|---|---|---|
| 5.5 plain | 0.945 (219) | 1.344 | 0.216 | 0.220 |
| 5.5 PGO | 0.864 (219) | 1.240 | 0.198 | 0.203 |
| 5.4 plain | 1.007 (219) | 1.436 | 0.225 | 0.234 |
| 5.4 PGO | 0.921 (219) | 1.325 | 0.206 | 0.217 |

- 5.5 plain / LuaJIT -joff (219): 0.320
- 5.5 PGO / LuaJIT -joff (219): 0.292
- b116447 same-session (set B) (same programs): 5.5 plain/LJ 0.948, PGO/LJ 0.866, 5.4 plain/LJ 1.007, 5.4 PGO/LJ 0.923
- session-j published b116447 (same programs): 5.5 plain/LJ 0.952, PGO/LJ 0.868, 5.4 plain/LJ 1.011, 5.4 PGO/LJ 0.926
- 22670a1 session-i (same programs): 5.5 plain/LJ 0.950, PGO/LJ 0.869, 5.4 plain/LJ 1.010, 5.4 PGO/LJ 0.925
- cached 8c39465 (same programs): 5.5 plain/LJ 0.955, PGO/LJ 0.871, 5.4 plain/LJ 1.020, 5.4 PGO/LJ 0.929
- cached 1a5aad0 (same programs): 5.5 plain/LJ 0.964, PGO/LJ 0.879, 5.4 plain/LJ 1.020, 5.4 PGO/LJ 0.934
- cached 108b935 (same programs): plain/LJ 0.969, PGO/LJ 0.883
- our 5.4 build / our 5.5 build cycles: plain 1.064, PGO 1.066; PGO/plain: 5.5 0.915, 5.4 0.917

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

### 5.5 plain: 15 biggest gaps to LuaJIT (ours/LJ cycles)

| program | Mcyc | cyc/LJ | ins/LJ |
|---|---|---|---|
| awfy/permute | 37 | 2.797 | 3.592 |
| apps/raytrace | 113 | 2.785 | 2.167 |
| awfy/cd | 261 | 2.427 | 2.475 |
| corpus/lua_lexer | 1085 | 2.421 | 2.313 |
| lib/fmt_s | 7 | 2.312 | 2.765 |
| lib/co_yield | 221 | 2.168 | 1.882 |
| corpus2/dev/text_adventure | 2564 | 2.161 | 2.170 |
| lib/byte_multi | 41 | 2.048 | 3.571 |
| awfy/towers | 48 | 2.030 | 1.720 |
| programs/coroutines | 1763 | 2.022 | 1.789 |
| corpus/classic_vector | 477 | 1.981 | 1.799 |
| corpus2/heldout/stream_closures | 806 | 1.936 | 2.144 |
| awfy/json | 142 | 1.936 | 2.158 |
| lib/find_plain | 129 | 1.903 | 2.353 |
| lib/getmt | 16 | 1.850 | 2.426 |

### 5.5 plain: 15 biggest wins over LuaJIT (>= 20 Mcyc)

| program | Mcyc | cyc/LJ | ins/LJ |
|---|---|---|---|
| lib/sel_loop | 213 | 0.041 | 0.067 |
| lib/error_tbl | 265 | 0.087 | 0.079 |
| jit-trace/int_rec2 | 684 | 0.136 | 1.551 |
| lib/pcall_err | 121 | 0.158 | 0.136 |
| programs/pcall_error | 1483 | 0.178 | 0.184 |
| lib/tfront_1k | 79 | 0.194 | 0.076 |
| lib/it_next | 45 | 0.247 | 0.703 |
| lib/tonumber_flt | 193 | 0.249 | 0.395 |
| corpus/sha2_base64 | 386 | 0.274 | 0.310 |
| lib/match | 61 | 0.316 | 0.316 |
| jit-trace/int_sum | 890 | 0.354 | 1.272 |
| apps/markdown | 118 | 0.359 | 0.268 |
| lib/find_pat | 271 | 0.385 | 0.451 |
| lib/gsub_str | 168 | 0.423 | 0.499 |
| programs/loops_arith | 479 | 0.433 | 1.020 |

### 5.5 PGO: 15 biggest gaps to LuaJIT (ours/LJ cycles)

| program | Mcyc | cyc/LJ | ins/LJ |
|---|---|---|---|
| awfy/permute | 36 | 2.766 | 3.578 |
| apps/raytrace | 105 | 2.584 | 2.024 |
| awfy/cd | 243 | 2.255 | 2.310 |
| lib/fmt_s | 7 | 2.216 | 2.662 |
| corpus/lua_lexer | 949 | 2.118 | 2.169 |
| lib/byte_multi | 40 | 2.014 | 3.562 |
| awfy/towers | 46 | 1.965 | 1.686 |
| corpus/classic_vector | 443 | 1.839 | 1.703 |
| corpus2/dev/text_adventure | 2129 | 1.794 | 1.922 |
| lib/getmt | 16 | 1.788 | 2.413 |
| jit-trace/type_entry | 1510 | 1.779 | 2.396 |
| lib/co_yield | 181 | 1.774 | 1.626 |
| corpus2/heldout/stream_closures | 737 | 1.772 | 1.982 |
| lib/find_plain | 119 | 1.755 | 2.147 |
| real/lua_interp | 103 | 1.730 | 2.190 |

### 5.5 PGO: 15 biggest wins over LuaJIT (>= 20 Mcyc)

| program | Mcyc | cyc/LJ | ins/LJ |
|---|---|---|---|
| lib/sel_loop | 211 | 0.040 | 0.067 |
| lib/error_tbl | 215 | 0.071 | 0.062 |
| lib/pcall_err | 99 | 0.130 | 0.104 |
| jit-trace/int_rec2 | 683 | 0.136 | 1.551 |
| programs/pcall_error | 1230 | 0.147 | 0.151 |
| lib/tfront_1k | 77 | 0.190 | 0.069 |
| lib/tonumber_flt | 168 | 0.217 | 0.350 |
| lib/it_next | 44 | 0.245 | 0.702 |
| lib/match | 52 | 0.267 | 0.282 |
| corpus/sha2_base64 | 381 | 0.270 | 0.296 |
| apps/markdown | 104 | 0.316 | 0.243 |
| lib/gsub_str | 128 | 0.321 | 0.394 |
| lib/find_pat | 237 | 0.336 | 0.413 |
| jit-trace/int_sum | 891 | 0.354 | 1.272 |
| lib/tpack | 118 | 0.384 | 0.395 |

## Focus programs (cycles in M; ratios of cycles)

| program | plain Mcyc | PGO Mcyc | LJ Mcyc | PUC Mcyc | plain/LJ | PGO/LJ | plain/PUC | PGO/PUC | plain vs b cyc | ins | PGO vs b cyc | ins | plain vs i cyc | PGO vs i cyc | PGO/LJ b116447 |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| programs/nbody | 342.6 | 343.4 | 345.6 | 4246.2 | 0.991 | 0.994 | 0.081 | 0.081 | 1.004 | 1.000 | 1.009 | 1.000 | 0.994 | 0.996 | 0.985 |
| programs/fib | 634.4 | 633.4 | 469.7 | 4139.5 | 1.351 | 1.349 | 0.153 | 0.153 | 0.510 | 0.505 | 0.509 | 0.505 | 0.505 | 0.508 | 2.650 |
| programs/binary_trees | 1244.4 | 1059.3 | 2007.4 | 4389.1 | 0.620 | 0.528 | 0.284 | 0.241 | 0.811 | 0.762 | 0.813 | 0.738 | 0.646 | 0.669 | 0.649 |
| programs/queens | 1850.6 | 1852.0 | 1907.1 | 4368.7 | 0.970 | 0.971 | 0.424 | 0.424 | 1.003 | 1.000 | 1.002 | 1.000 | 0.997 | 1.002 | 0.969 |
| apps/raytrace | 113.0 | 104.8 | 40.6 | 262.7 | 2.785 | 2.584 | 0.430 | 0.399 | 1.015 | 1.001 | 1.004 | 0.999 | 0.795 | 0.906 | 2.574 |
| awfy/cd | 261.3 | 242.8 | 107.6 | 437.5 | 2.427 | 2.255 | 0.597 | 0.555 | 1.023 | 1.005 | 1.011 | 1.002 | 0.820 | 0.835 | 2.231 |
| awfy/list | 94.8 | 93.5 | 88.9 | 418.8 | 1.067 | 1.052 | 0.226 | 0.223 | 1.001 | 0.976 | 0.996 | 0.975 | 0.713 | 0.719 | 1.056 |
| awfy/richards | 60.6 | 58.5 | 34.7 | 197.7 | 1.747 | 1.685 | 0.307 | 0.296 | 1.010 | 1.002 | 1.006 | 0.999 | 0.996 | 0.996 | 1.675 |
| awfy/deltablue | 108.9 | 100.7 | 103.5 | 157.9 | 1.053 | 0.973 | 0.690 | 0.637 | 1.015 | 1.002 | 1.003 | 0.996 | 0.929 | 0.920 | 0.970 |
| awfy/havlak | 152.7 | 134.9 | 115.2 | 217.5 | 1.325 | 1.171 | 0.702 | 0.620 | 1.015 | 1.001 | 1.018 | 1.000 | 0.990 | 0.997 | 1.149 |
| awfy/permute | 36.7 | 36.3 | 13.1 | 246.3 | 2.797 | 2.766 | 0.149 | 0.147 | 0.836 | 0.830 | 0.836 | 0.829 | 0.844 | 0.848 | 3.309 |
| awfy/towers | 47.6 | 46.1 | 23.5 | 215.1 | 2.030 | 1.965 | 0.221 | 0.214 | 0.975 | 0.978 | 0.968 | 0.976 | 0.958 | 0.957 | 2.029 |
| corpus/lua_lexer | 1085.0 | 949.3 | 448.2 | 3281.3 | 2.421 | 2.118 | 0.331 | 0.289 | 1.005 | 1.000 | 0.992 | 1.002 | 0.872 | 0.836 | 2.134 |
| corpus/classic_vector | 476.7 | 442.6 | 240.7 | 2945.5 | 1.981 | 1.839 | 0.162 | 0.150 | 0.997 | 1.001 | 1.026 | 1.000 | 0.675 | 0.709 | 1.793 |
| corpus2/heldout/dijkstra_routes | 1185.9 | 1149.9 | 1265.1 | 3142.9 | 0.937 | 0.909 | 0.377 | 0.366 | 1.005 | 1.000 | 0.999 | 1.000 | 0.955 | 0.950 | 0.910 |
| programs/fannkuch | 1037.4 | 1053.5 | 1075.7 | 4075.1 | 0.964 | 0.979 | 0.255 | 0.259 | 0.973 | 1.000 | 0.994 | 1.000 | 0.984 | 0.991 | 0.986 |

## Per-program table (5.5 and 5.4; Mcyc, ratios vs b116447 and LJ)

| program | n runs (plain) | plain | PGO | p54 | g54 | LJ | PUC55 | PUC54 | plain/LJ | PGO/LJ | plain vs b cyc | ins | PGO vs b cyc | ins | p54 vs b cyc | ins | g54 vs b cyc | ins | out/exp (all 4) |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| apps/events | 5 | 164.9 | 141.8 | 177.6 | 151.6 | 95.4 | 293.7 | 289.5 | 1.728 | 1.487 | 0.973 | 1.000 | 0.991 | 1.000 | 1.002 | 1.000 | 0.986 | 1.001 | ok |
| apps/expr_lang | 2 | 137.2 | 126.5 | 160.4 | 145.6 | 96.7 | 241.7 | 253.2 | 1.419 | 1.308 | 1.021 | 1.000 | 1.026 | 0.999 | 1.016 | 1.000 | 1.014 | 1.000 | ok |
| apps/lru_cache | 2 | 148.9 | 138.3 | 183.6 | 174.1 | 106.4 | 219.8 | 253.9 | 1.399 | 1.299 | 1.017 | 1.001 | 1.021 | 0.999 | 1.014 | 1.001 | 1.009 | 0.999 | ok |
| apps/markdown | 5 | 118.4 | 104.1 | 123.0 | 107.3 | 329.7 | 412.6 | 390.8 | 0.359 | 0.316 | 1.012 | 1.001 | 1.015 | 1.017 | 1.000 | 1.000 | 1.016 | 1.000 | ok |
| apps/raytrace | 20 | 113.0 | 104.8 | 118.0 | 110.0 | 40.6 | 262.7 | 281.7 | 2.785 | 2.584 | 1.015 | 1.001 | 1.004 | 0.999 | 1.014 | 1.001 | 1.018 | 1.001 | ok |
| awfy/bounce | 2 | 61.5 | 59.7 | 67.8 | 65.7 | 47.7 | 511.2 | 537.1 | 1.290 | 1.252 | 1.008 | 1.000 | 1.004 | 1.000 | 1.008 | 1.000 | 1.006 | 1.000 | ok |
| awfy/cd | 17 | 261.3 | 242.8 | 288.9 | 266.9 | 107.6 | 437.5 | 464.4 | 2.427 | 2.255 | 1.023 | 1.005 | 1.011 | 1.002 | 1.016 | 1.004 | 1.015 | 1.003 | ok |
| awfy/deltablue | 17 | 108.9 | 100.7 | 120.4 | 111.1 | 103.5 | 157.9 | 165.2 | 1.053 | 0.973 | 1.015 | 1.002 | 1.003 | 0.996 | 1.021 | 1.002 | 1.013 | 0.998 | ok |
| awfy/havlak | 20 | 152.7 | 134.9 | 177.2 | 156.3 | 115.2 | 217.5 | 225.5 | 1.325 | 1.171 | 1.015 | 1.001 | 1.018 | 1.000 | 1.018 | 1.000 | 1.002 | 1.000 | ok |
| awfy/json | 17 | 142.1 | 120.6 | 154.4 | 132.4 | 73.4 | 262.9 | 271.8 | 1.936 | 1.642 | 1.003 | 1.001 | 1.000 | 0.999 | 1.017 | 1.001 | 1.010 | 1.000 | ok |
| awfy/list | 20 | 94.8 | 93.5 | 98.3 | 96.1 | 88.9 | 418.8 | 477.9 | 1.067 | 1.052 | 1.001 | 0.976 | 0.996 | 0.975 | 1.012 | 0.990 | 1.010 | 0.989 | ok |
| awfy/mandelbrot | 2 | 273.6 | 273.2 | 273.9 | 273.2 | 273.1 | 1529.3 | 1379.3 | 1.002 | 1.000 | 1.001 | 1.000 | 1.003 | 1.000 | 1.003 | 1.000 | 1.002 | 1.000 | ok |
| awfy/nbody | 2 | 93.6 | 92.6 | 96.9 | 96.1 | 94.6 | 1504.3 | 1603.8 | 0.989 | 0.979 | 1.010 | 1.000 | 1.008 | 1.000 | 1.020 | 1.000 | 1.013 | 1.000 | ok |
| awfy/permute | 20 | 36.7 | 36.3 | 37.9 | 37.6 | 13.1 | 246.3 | 257.2 | 2.797 | 2.766 | 0.836 | 0.830 | 0.836 | 0.829 | 0.867 | 0.835 | 0.866 | 0.834 | ok |
| awfy/queens | 17 | 96.9 | 94.6 | 83.0 | 80.5 | 55.0 | 401.8 | 403.4 | 1.762 | 1.719 | 1.007 | 1.000 | 1.009 | 1.000 | 1.026 | 1.000 | 1.019 | 1.000 | ok |
| awfy/richards | 20 | 60.6 | 58.5 | 60.7 | 58.4 | 34.7 | 197.7 | 199.3 | 1.747 | 1.685 | 1.010 | 1.002 | 1.006 | 0.999 | 1.006 | 1.001 | 1.012 | 1.000 | ok |
| awfy/sieve | 5 | 124.4 | 124.0 | 128.9 | 128.6 | 156.4 | 1032.3 | 1085.2 | 0.796 | 0.793 | 1.008 | 1.000 | 1.007 | 1.000 | 1.004 | 1.000 | 1.002 | 1.000 | ok |
| awfy/storage | 5 | 102.7 | 79.5 | 146.9 | 114.3 | 154.0 | 255.9 | 262.0 | 0.667 | 0.516 | 1.006 | 1.000 | 1.001 | 1.000 | 0.999 | 1.000 | 0.984 | 1.000 | ok |
| awfy/towers | 20 | 47.6 | 46.1 | 48.8 | 46.8 | 23.5 | 215.1 | 213.9 | 2.030 | 1.965 | 0.975 | 0.978 | 0.968 | 0.976 | 0.988 | 0.978 | 0.987 | 0.977 | ok |
| corpus/astar_jumper | 20 | 1655.0 | 1601.4 | 1868.5 | 1793.0 | 997.4 | 3280.6 | 3358.7 | 1.659 | 1.606 | 1.000 | 1.003 | 0.997 | 1.000 | 1.016 | 1.000 | 1.008 | 1.000 | ok |
| corpus/class_middleclass | 5 | 486.6 | 476.6 | 516.9 | 501.4 | 319.5 | 3046.3 | 3098.9 | 1.523 | 1.492 | 1.020 | 1.000 | 1.017 | 1.000 | 1.013 | 1.000 | 1.011 | 1.000 | ok |
| corpus/classic_vector | 20 | 476.7 | 442.6 | 651.5 | 571.8 | 240.7 | 2945.5 | 2905.4 | 1.981 | 1.839 | 0.997 | 1.001 | 1.026 | 1.000 | 1.011 | 1.001 | 1.013 | 1.003 | ok |
| corpus/containers_lru_heap | 17 | 1285.3 | 1201.2 | 1671.9 | 1540.6 | 1417.3 | 2675.7 | 3035.3 | 0.907 | 0.848 | 0.989 | 1.001 | 0.990 | 1.001 | 1.033 | 1.001 | 1.005 | 1.001 | ok |
| corpus/coro_scheduler | 2 | 1518.4 | 1329.9 | 1636.6 | 1416.1 | 941.9 | 2772.8 | 2851.7 | 1.612 | 1.412 | 1.002 | 1.000 | 1.009 | 1.000 | 1.021 | 1.000 | 1.008 | 1.001 | ok |
| corpus/csv_ini_yaml | 20 | 1558.6 | 1364.1 | 1900.8 | 1590.9 | 1771.4 | 2948.4 | 3020.4 | 0.880 | 0.770 | 0.967 | 1.000 | 1.013 | 1.012 | 1.022 | 1.000 | 0.998 | 1.000 | ok |
| corpus/inspect_lua | 5 | 1573.9 | 1392.3 | 1689.8 | 1535.0 | 1056.4 | 2901.3 | 2968.0 | 1.490 | 1.318 | 1.012 | 1.000 | 1.002 | 0.997 | 1.012 | 1.000 | 1.020 | 0.996 | ok |
| corpus/json_rxi | 20 | 1813.8 | 1503.6 | 1909.7 | 1590.2 | 1222.9 | 3647.9 | 3646.8 | 1.483 | 1.229 | 0.995 | 1.000 | 0.994 | 1.003 | 1.017 | 1.000 | 1.013 | 1.000 | ok |
| corpus/lua_lexer | 17 | 1085.0 | 949.3 | 1122.9 | 991.5 | 448.2 | 3281.3 | 3358.7 | 2.421 | 2.118 | 1.005 | 1.000 | 0.992 | 1.002 | 1.010 | 1.000 | 0.989 | 1.000 | ok |
| corpus/markdown_conv | 20 | 1404.8 | 1275.3 | 1432.7 | 1287.6 | 3029.8 | 3746.9 | 3671.3 | 0.464 | 0.421 | 0.996 | 1.000 | 1.025 | 1.013 | 1.010 | 1.000 | 1.022 | 1.000 | ok |
| corpus/msgpack_pure | 2 | 2072.6 | 1795.6 | 2201.6 | 1916.9 | 4473.7 | 3436.5 | 3601.3 | 0.463 | 0.401 | 1.019 | 1.000 | 1.027 | 0.996 | 1.000 | 1.000 | 1.011 | 0.997 | ok |
| corpus/mustache_lustache | 17 | 1810.7 | 1448.0 | 1962.6 | 1552.1 | 1537.1 | 3296.2 | 3446.8 | 1.178 | 0.942 | 1.023 | 1.000 | 1.031 | 1.001 | 1.023 | 1.000 | 1.010 | 1.000 | ok |
| corpus/parser_combinator | 17 | 2301.9 | 1878.5 | 2534.2 | 2096.6 | 1563.1 | 3390.6 | 3391.8 | 1.473 | 1.202 | 1.016 | 1.000 | 1.038 | 1.001 | 1.012 | 1.000 | 1.029 | 1.001 | ok |
| corpus/penlight_mix | 20 | 2273.7 | 2153.0 | 2384.5 | 2301.1 | 2678.7 | 3992.1 | 3970.3 | 0.849 | 0.804 | 0.986 | 1.000 | 1.019 | 1.027 | 1.003 | 1.000 | 1.030 | 1.000 | ok |
| corpus/serpent_ser | 2 | 2042.6 | 1767.8 | 2102.7 | 1864.0 | 2426.7 | 3260.6 | 3254.8 | 0.842 | 0.728 | 1.016 | 1.001 | 1.015 | 1.003 | 1.009 | 1.000 | 1.026 | 0.999 | ok |
| corpus/sha2_base64 | 17 | 386.3 | 380.7 | 389.8 | 371.9 | 1411.3 | 3328.6 | 3384.1 | 0.274 | 0.270 | 1.002 | 1.000 | 1.038 | 1.003 | 1.013 | 1.000 | 1.013 | 1.000 | ok |
| corpus/template_etlua | 2 | 1567.3 | 1283.4 | 1564.4 | 1264.6 | 1826.0 | 2680.1 | 2700.4 | 0.858 | 0.703 | 0.998 | 1.000 | 1.025 | 1.000 | 0.999 | 1.000 | 0.992 | 0.999 | ok |
| corpus/testrunner_luaunit | 5 | 1489.5 | 1220.6 | 1636.6 | 1368.0 | 1390.7 | 2601.9 | 2628.8 | 1.071 | 0.878 | 1.012 | 1.000 | 1.003 | 1.006 | 1.015 | 1.000 | 1.018 | 1.001 | ok |
| corpus2/dev/bignum_arith | 2 | 793.8 | 731.9 | 872.7 | 810.2 | 660.2 | 2352.8 | 2412.4 | 1.202 | 1.109 | 1.006 | 1.000 | 1.012 | 1.000 | 1.006 | 1.000 | 1.002 | 1.000 | ok |
| corpus2/dev/coro_generators | 5 | 1276.2 | 985.7 | 1305.9 | 1004.0 | 713.4 | 1807.9 | 1836.1 | 1.789 | 1.382 | 1.021 | 0.985 | 1.028 | 0.983 | 1.026 | 0.988 | 1.005 | 0.986 | ok |
| corpus2/dev/csv_report | 20 | 1146.0 | 1045.2 | 1177.3 | 1069.8 | 1262.6 | 2165.3 | 2381.4 | 0.908 | 0.828 | 0.998 | 1.000 | 1.018 | 1.006 | 0.997 | 1.000 | 0.998 | 0.999 | ok |
| corpus2/dev/diff_levenshtein | 5 | 431.2 | 400.9 | 440.9 | 409.1 | 329.3 | 1776.3 | 1850.8 | 1.309 | 1.217 | 0.987 | 1.000 | 1.004 | 1.000 | 1.016 | 1.000 | 1.025 | 1.000 | ok |
| corpus2/dev/ecs_world | 2 | 959.5 | 860.9 | 1012.2 | 873.3 | 916.1 | 2115.3 | 2160.1 | 1.047 | 0.940 | 1.019 | 1.000 | 1.019 | 1.000 | 1.010 | 1.000 | 1.015 | 1.000 | ok |
| corpus2/dev/event_promise | 2 | 391.8 | 374.0 | 475.0 | 443.3 | 467.9 | 1906.6 | 1939.2 | 0.837 | 0.799 | 1.030 | 1.000 | 1.021 | 1.001 | 1.026 | 1.000 | 0.991 | 1.000 | ok |
| corpus2/dev/expr_compiler | 5 | 1316.5 | 1127.0 | 1374.8 | 1184.3 | 1080.2 | 2081.3 | 2231.0 | 1.219 | 1.043 | 1.002 | 1.000 | 1.011 | 0.999 | 1.010 | 1.000 | 0.998 | 1.001 | ok |
| corpus2/dev/iter_pipeline | 2 | 178.2 | 171.4 | 185.5 | 178.7 | 132.2 | 2865.2 | 2874.5 | 1.348 | 1.297 | 1.006 | 1.000 | 1.029 | 1.000 | 1.004 | 1.000 | 1.012 | 1.000 | ok |
| corpus2/dev/lua_parser | 2 | 1283.1 | 1129.6 | 1572.1 | 1327.2 | 790.7 | 2461.5 | 2499.0 | 1.623 | 1.429 | 1.015 | 1.000 | 1.015 | 1.001 | 1.028 | 1.000 | 1.006 | 1.000 | ok |
| corpus2/dev/memo_cache | 5 | 1634.0 | 1522.7 | 1681.0 | 1537.0 | 1674.7 | 2669.5 | 2763.8 | 0.976 | 0.909 | 0.995 | 1.000 | 1.012 | 0.998 | 1.010 | 0.997 | 1.001 | 0.997 | ok |
| corpus2/dev/oop_mixins | 17 | 1425.7 | 1364.6 | 1442.7 | 1357.9 | 865.1 | 2464.3 | 2520.8 | 1.648 | 1.577 | 0.984 | 1.000 | 1.005 | 0.997 | 1.004 | 0.999 | 1.015 | 0.999 | ok |
| corpus2/dev/particles_physics | 2 | 404.9 | 378.5 | 517.2 | 477.6 | 379.7 | 1719.5 | 1811.4 | 1.066 | 0.997 | 1.013 | 1.000 | 1.006 | 1.000 | 1.023 | 1.000 | 1.014 | 1.002 | ok |
| corpus2/dev/real_dis_arm | 2 | 2460.8 | 2210.0 | 2459.4 | 2172.0 | 1947.2 | 4487.3 | 4484.7 | 1.264 | 1.135 | 1.004 | 1.003 | 0.997 | 1.003 | 1.009 | 1.000 | 0.989 | 1.000 | ok |
| corpus2/dev/real_dis_x86 | 5 | 1525.7 | 1347.6 | 1608.1 | 1432.3 | 1591.6 | 2708.6 | 2688.6 | 0.959 | 0.847 | 1.005 | 1.000 | 0.985 | 1.004 | 1.008 | 1.000 | 1.005 | 1.000 | ok |
| corpus2/dev/regex_nfa | 20 | 720.2 | 559.8 | 1394.4 | 1162.9 | 554.5 | 2510.1 | 2439.3 | 1.299 | 1.010 | 1.020 | 1.000 | 0.977 | 1.001 | 1.037 | 1.000 | 1.006 | 1.001 | ok |
| corpus2/dev/template_gsub | 5 | 1700.2 | 1404.5 | 1761.1 | 1492.3 | 2051.0 | 2892.7 | 2885.0 | 0.829 | 0.685 | 1.002 | 1.000 | 1.012 | 1.010 | 0.991 | 1.000 | 0.986 | 1.000 | ok |
| corpus2/dev/text_adventure | 5 | 2564.4 | 2128.7 | 2668.1 | 2203.2 | 1186.6 | 3375.1 | 3513.0 | 2.161 | 1.794 | 1.008 | 1.000 | 1.008 | 1.002 | 0.989 | 1.000 | 1.007 | 1.001 | ok |
| corpus2/dev/varargs_util | 5 | 1514.7 | 1240.2 | 1519.4 | 1246.4 | 1413.9 | 2284.8 | 2301.9 | 1.071 | 0.877 | 0.992 | 1.000 | 1.013 | 1.001 | 0.981 | 1.000 | 0.997 | 1.002 | ok |
| corpus2/dev/wordfreq_wrap | 5 | 611.5 | 498.5 | 657.8 | 553.6 | 658.1 | 1219.2 | 1225.4 | 0.929 | 0.757 | 1.004 | 1.000 | 1.013 | 1.007 | 1.004 | 1.000 | 1.005 | 1.000 | ok |
| corpus2/heldout/bigint_rsa | 2 | 1130.4 | 938.6 | 1377.0 | 1183.9 | 1074.5 | 3033.1 | 3149.9 | 1.052 | 0.874 | 1.024 | 1.000 | 0.993 | 1.000 | 0.989 | 1.000 | 0.993 | 1.001 | ok |
| corpus2/heldout/dijkstra_routes | 17 | 1185.9 | 1149.9 | 1459.5 | 1415.6 | 1265.1 | 3142.9 | 3226.1 | 0.937 | 0.909 | 1.005 | 1.000 | 0.999 | 1.000 | 1.000 | 1.000 | 0.997 | 1.000 | ok |
| corpus2/heldout/forth_vm | 20 | 804.5 | 801.2 | 1060.1 | 1166.1 | 696.1 | 2965.2 | 3269.3 | 1.156 | 1.151 | 1.003 | 1.000 | 1.002 | 1.000 | 1.008 | 1.000 | 1.016 | 1.000 | ok |
| corpus2/heldout/log_analyzer | 5 | 2146.1 | 1760.8 | 2152.1 | 1899.6 | 1652.2 | 3115.0 | 3178.7 | 1.299 | 1.066 | 0.994 | 1.000 | 0.972 | 1.003 | 0.992 | 1.000 | 1.008 | 1.000 | ok |
| corpus2/heldout/markov_text | 2 | 1140.8 | 1110.5 | 1203.1 | 1153.1 | 982.0 | 2862.9 | 2851.2 | 1.162 | 1.131 | 1.013 | 1.000 | 1.007 | 1.002 | 1.012 | 1.000 | 1.008 | 1.000 | ok |
| corpus2/heldout/prototype_objects | 20 | 1497.2 | 1432.2 | 1517.0 | 1457.4 | 1280.6 | 2532.8 | 2721.4 | 1.169 | 1.118 | 1.006 | 0.998 | 0.993 | 0.999 | 1.031 | 1.000 | 0.989 | 0.997 | ok |
| corpus2/heldout/real_dis_mips | 2 | 2222.7 | 1893.9 | 2247.3 | 1892.7 | 1786.6 | 3808.7 | 3869.0 | 1.244 | 1.060 | 0.997 | 1.000 | 0.963 | 1.003 | 1.024 | 1.000 | 0.998 | 1.000 | ok |
| corpus2/heldout/real_dis_ppc | 17 | 2090.2 | 1818.7 | 2230.4 | 1928.4 | 1747.6 | 3961.9 | 3962.8 | 1.196 | 1.041 | 0.991 | 1.000 | 0.965 | 1.004 | 1.007 | 1.001 | 1.008 | 1.001 | ok |
| corpus2/heldout/roguelike_turns | 2 | 825.2 | 781.8 | 869.9 | 848.2 | 623.3 | 3311.7 | 3473.6 | 1.324 | 1.254 | 1.020 | 1.000 | 1.011 | 1.000 | 1.013 | 1.000 | 1.023 | 1.000 | ok |
| corpus2/heldout/scheme_interp | 17 | 1464.4 | 1225.4 | 1513.7 | 1288.1 | 1045.1 | 3052.1 | 3021.0 | 1.401 | 1.173 | 1.029 | 1.000 | 1.006 | 1.000 | 0.975 | 1.000 | 0.991 | 1.001 | ok |
| corpus2/heldout/stream_closures | 2 | 805.5 | 737.0 | 860.3 | 803.7 | 416.0 | 3032.0 | 3163.1 | 1.936 | 1.772 | 1.025 | 0.999 | 1.014 | 0.999 | 1.008 | 1.000 | 1.016 | 1.002 | ok |
| corpus2/heldout/sudoku_solver | 20 | 1319.0 | 1184.6 | 1645.9 | 1496.5 | 1387.6 | 3013.0 | 3102.7 | 0.951 | 0.854 | 0.999 | 1.000 | 1.001 | 1.000 | 0.999 | 1.000 | 0.996 | 1.000 | ok |
| corpus2/heldout/vending_fsm | 5 | 1027.4 | 941.8 | 1100.2 | 1019.3 | 824.2 | 3290.3 | 3377.2 | 1.246 | 1.143 | 1.005 | 1.000 | 1.008 | 1.000 | 1.002 | 1.000 | 1.002 | 1.001 | ok |
| jit-trace/alloc_ctor | 20 | 187.4 | 159.7 | 257.1 | 207.8 | 276.5 | 1652.1 | 2197.8 | 0.678 | 0.578 | 0.992 | 1.000 | 1.011 | 1.000 | 0.999 | 1.000 | 1.035 | 1.009 | ok |
| jit-trace/array_rec | 17 | 1796.3 | 1796.0 | 1821.5 | 1821.0 | 1755.9 | 9671.5 | 10022.8 | 1.023 | 1.023 | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | ok |
| jit-trace/dot | 2 | 1217.2 | 1216.9 | 1228.6 | 1227.0 | 1166.0 | 11124.9 | 11807.8 | 1.044 | 1.044 | 0.996 | 1.000 | 0.996 | 1.000 | 0.998 | 1.000 | 0.998 | 1.000 | ok |
| jit-trace/field_global | 2 | 545.3 | 539.3 | 543.8 | 540.7 | 491.7 | 17948.4 | 17879.3 | 1.109 | 1.097 | 0.997 | 1.000 | 0.993 | 1.000 | 0.994 | 1.000 | 0.999 | 1.000 | ok |
| jit-trace/field_method | 2 | 571.1 | 564.8 | 573.4 | 567.6 | 423.1 | 20185.1 | 20271.4 | 1.350 | 1.335 | 1.000 | 1.000 | 1.001 | 1.000 | 1.003 | 1.000 | 1.000 | 1.000 | ok |
| jit-trace/field_rw | 2 | 950.0 | 948.8 | 1036.4 | 1034.4 | 929.3 | 19574.6 | 19882.6 | 1.022 | 1.021 | 1.000 | 1.000 | 1.000 | 1.000 | 1.001 | 1.000 | 1.000 | 1.001 | ok |
| jit-trace/float_loop | 2 | 2777.3 | 2779.7 | 2795.2 | 2797.5 | 2575.9 | 11381.5 | 13567.0 | 1.078 | 1.079 | 1.001 | 1.000 | 1.000 | 1.000 | 0.999 | 1.000 | 0.999 | 1.000 | ok |
| jit-trace/float_poly | 2 | 1783.8 | 1783.2 | 1819.7 | 1820.3 | 1768.0 | 7918.6 | 8940.5 | 1.009 | 1.009 | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | ok |
| jit-trace/inline_2 | 2 | 333.2 | 332.4 | 333.2 | 332.8 | 262.5 | 11057.3 | 10600.9 | 1.270 | 1.266 | 1.000 | 1.000 | 1.002 | 1.000 | 1.002 | 1.000 | 1.001 | 1.000 | ok |
| jit-trace/inline_leaf | 2 | 459.0 | 458.0 | 456.7 | 458.1 | 630.2 | 11902.9 | 12019.3 | 0.728 | 0.727 | 0.991 | 1.000 | 1.003 | 1.000 | 0.998 | 1.000 | 0.999 | 1.000 | ok |
| jit-trace/int_rec2 | 20 | 684.1 | 682.8 | 694.1 | 691.2 | 5016.4 | 12493.6 | 13006.3 | 0.136 | 0.136 | 1.005 | 1.000 | 1.004 | 1.000 | 1.005 | 1.000 | 0.997 | 1.000 | ok |
| jit-trace/int_sum | 2 | 890.4 | 890.7 | 915.7 | 913.1 | 2512.8 | 9858.5 | 12300.9 | 0.354 | 0.354 | 1.001 | 1.000 | 1.000 | 1.000 | 1.004 | 1.000 | 1.000 | 1.000 | ok |
| jit-trace/licm_expr | 2 | 470.6 | 468.6 | 469.6 | 467.7 | 442.6 | 12323.9 | 12827.3 | 1.063 | 1.059 | 1.004 | 1.000 | 1.001 | 1.000 | 1.005 | 1.000 | 1.000 | 1.000 | ok |
| jit-trace/multret | 2 | 151.0 | 150.4 | 151.3 | 150.7 | 156.7 | 5879.7 | 5782.8 | 0.963 | 0.960 | 0.997 | 1.000 | 1.000 | 1.000 | 1.001 | 1.000 | 1.000 | 1.000 | ok |
| jit-trace/oop_inherit | 2 | 57.9 | 57.6 | 49.6 | 49.1 | 112.8 | 2153.1 | 2201.0 | 0.513 | 0.511 | 1.000 | 1.000 | 1.001 | 1.000 | 1.004 | 1.000 | 1.004 | 1.000 | ok |
| jit-trace/oop_override | 2 | 108.5 | 108.0 | 112.0 | 111.4 | 136.7 | 645.6 | 666.3 | 0.794 | 0.790 | 1.002 | 1.000 | 1.002 | 1.000 | 1.002 | 1.000 | 1.001 | 1.000 | ok |
| jit-trace/rec_loop | 20 | 93.5 | 91.9 | 89.9 | 89.0 | 141.9 | 1440.3 | 1449.9 | 0.659 | 0.648 | 1.146 | 1.179 | 1.139 | 1.179 | 1.131 | 1.177 | 1.133 | 1.176 | ok |
| jit-trace/short_inner | 2 | 1106.8 | 1109.0 | 1003.4 | 999.5 | 878.8 | 10393.4 | 11908.6 | 1.259 | 1.262 | 0.998 | 1.000 | 1.002 | 1.000 | 1.002 | 1.000 | 1.001 | 1.000 | ok |
| jit-trace/sink_tuple | 5 | 13.4 | 13.0 | 13.3 | 13.0 | 20.0 | 2743.7 | 3358.1 | 0.670 | 0.651 | 1.008 | 1.000 | 1.005 | 1.000 | 1.002 | 1.000 | 1.004 | 1.000 | ok |
| jit-trace/sink_vec | 2 | 15.3 | 15.3 | 15.5 | 15.2 | 20.0 | 1489.0 | 1832.3 | 0.769 | 0.765 | 1.000 | 1.000 | 1.017 | 1.000 | 1.013 | 1.000 | 1.004 | 1.000 | ok |
| jit-trace/str_concat | 2 | 535.2 | 511.8 | 602.2 | 558.1 | 501.6 | 1628.5 | 1630.2 | 1.067 | 1.020 | 0.995 | 1.000 | 1.001 | 1.000 | 0.995 | 1.000 | 1.002 | 0.997 | ok |
| jit-trace/tail_loop | 2 | 247.9 | 248.8 | 251.6 | 250.5 | 223.3 | 6724.0 | 6643.8 | 1.110 | 1.114 | 0.995 | 1.000 | 0.996 | 1.000 | 0.995 | 1.000 | 0.994 | 1.000 | ok |
| jit-trace/tfor_loops | 5 | 220.5 | 218.8 | 226.7 | 224.2 | 362.3 | 3623.0 | 3352.6 | 0.609 | 0.604 | 0.991 | 1.000 | 0.982 | 1.000 | 0.969 | 1.000 | 0.981 | 1.000 | ok |
| jit-trace/type_entry | 2 | 1521.3 | 1510.0 | 1569.0 | 1575.0 | 848.7 | 11725.8 | 12234.3 | 1.793 | 1.779 | 0.999 | 1.000 | 0.994 | 1.000 | 0.999 | 1.000 | 1.001 | 1.000 | ok |
| jit-trace/type_entry_stable | 2 | 1450.7 | 1451.2 | 1534.7 | 1519.8 | 856.9 | 11690.0 | 12248.4 | 1.693 | 1.694 | 0.991 | 1.000 | 0.990 | 1.000 | 1.008 | 1.000 | 1.001 | 1.000 | ok |
| jit-trace/type_late | 2 | 1521.8 | 1521.0 | 1533.2 | 1531.6 | 1461.5 | 10901.2 | 11418.6 | 1.041 | 1.041 | 0.996 | 1.000 | 0.996 | 1.000 | 1.000 | 1.000 | 0.995 | 1.000 | ok |
| jit-trace/type_late_stable | 2 | 1521.1 | 1521.1 | 1531.5 | 1532.3 | 1461.0 | 10905.6 | 11410.8 | 1.041 | 1.041 | 0.996 | 1.000 | 0.996 | 1.000 | 0.997 | 1.000 | 0.995 | 1.000 | ok |
| jit-trace/upval_acc | 2 | 1383.0 | 1392.9 | 1386.8 | 1385.5 | 1356.5 | 9970.0 | 10500.8 | 1.020 | 1.027 | 0.994 | 1.000 | 0.999 | 1.000 | 0.993 | 1.000 | 0.994 | 1.000 | ok |
| jit-trace/upval_inv | 2 | 2135.3 | 2133.8 | 2141.7 | 2140.1 | 1985.3 | 13886.5 | 14901.3 | 1.076 | 1.075 | 1.000 | 1.000 | 0.996 | 1.000 | 0.996 | 1.000 | 0.998 | 1.000 | ok |
| lib/assert_ok | 20 | 19.1 | 18.8 | 20.5 | 20.1 | 23.7 | 295.9 | 329.8 | 0.807 | 0.794 | 1.001 | 1.000 | 0.997 | 1.000 | 1.003 | 1.000 | 0.991 | 1.000 | ok |
| lib/byte | 5 | 50.4 | 50.0 | 50.9 | 50.5 | 33.0 | 776.0 | 845.1 | 1.525 | 1.514 | 0.993 | 1.000 | 0.995 | 1.000 | 0.992 | 1.000 | 0.991 | 1.000 | ok |
| lib/byte_multi | 5 | 41.2 | 40.5 | 41.1 | 40.9 | 20.1 | 336.9 | 366.2 | 2.048 | 2.014 | 0.992 | 1.000 | 0.980 | 1.000 | 0.987 | 1.000 | 0.983 | 1.000 | ok |
| lib/char | 20 | 151.5 | 139.6 | 156.1 | 143.7 | 104.8 | 383.6 | 387.1 | 1.446 | 1.332 | 0.986 | 1.000 | 1.002 | 1.000 | 0.999 | 1.000 | 0.993 | 1.000 | ok |
| lib/co_wrap | 20 | 248.9 | 202.4 | 241.1 | 195.8 | 228.7 | 381.5 | 338.9 | 1.089 | 0.885 | 1.016 | 1.002 | 1.006 | 1.003 | 0.993 | 1.000 | 0.857 | 0.999 | ok |
| lib/co_yield | 20 | 221.1 | 180.9 | 244.2 | 187.8 | 102.0 | 483.9 | 475.0 | 2.168 | 1.774 | 0.909 | 1.000 | 0.985 | 1.000 | 0.992 | 1.000 | 1.033 | 1.000 | ok |
| lib/concat_op | 20 | 141.0 | 129.4 | 143.8 | 134.8 | 212.9 | 419.4 | 445.3 | 0.662 | 0.608 | 1.002 | 1.000 | 1.000 | 1.000 | 0.992 | 1.000 | 0.999 | 0.995 | ok |
| lib/concat_op_str | 2 | 139.0 | 127.2 | 246.6 | 232.7 | 276.8 | 436.1 | 336.3 | 0.502 | 0.460 | 0.998 | 1.000 | 0.995 | 1.000 | 1.025 | 1.000 | 1.005 | 0.999 | ok |
| lib/error_tbl | 5 | 265.2 | 215.0 | 269.8 | 221.3 | 3042.9 | 619.9 | 625.1 | 0.087 | 0.071 | 0.991 | 1.000 | 1.013 | 1.008 | 1.026 | 1.000 | 1.001 | 1.000 | ok |
| lib/find_cap | 5 | 274.6 | 239.2 | 276.9 | 226.7 | 183.4 | 365.9 | 363.4 | 1.497 | 1.304 | 0.996 | 1.000 | 1.002 | 1.014 | 0.994 | 1.000 | 0.993 | 1.000 | ok |
| lib/find_pat | 20 | 271.4 | 236.6 | 262.8 | 237.5 | 705.2 | 1094.0 | 1092.2 | 0.385 | 0.336 | 1.043 | 1.000 | 1.000 | 1.012 | 0.986 | 1.000 | 1.001 | 1.000 | ok |
| lib/find_plain | 20 | 128.6 | 118.5 | 127.5 | 116.0 | 67.6 | 342.8 | 363.4 | 1.903 | 1.755 | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | 0.996 | 0.989 | ok |
| lib/fmt_d | 20 | 94.2 | 90.3 | 99.9 | 95.7 | 126.3 | 278.1 | 235.1 | 0.746 | 0.715 | 1.003 | 1.000 | 1.004 | 1.006 | 0.986 | 1.000 | 1.004 | 0.995 | ok |
| lib/fmt_f | 20 | 62.8 | 57.6 | 65.0 | 59.5 | 86.8 | 215.0 | 196.2 | 0.724 | 0.663 | 1.000 | 1.000 | 0.997 | 1.000 | 0.993 | 1.000 | 1.002 | 0.997 | ok |
| lib/fmt_g | 20 | 72.9 | 62.6 | 75.2 | 63.7 | 67.3 | 204.6 | 179.3 | 1.082 | 0.930 | 1.001 | 1.000 | 1.004 | 1.000 | 0.991 | 1.000 | 0.996 | 0.997 | ok |
| lib/fmt_mixed | 20 | 59.0 | 50.1 | 67.3 | 56.4 | 106.2 | 254.5 | 238.3 | 0.556 | 0.472 | 0.988 | 1.000 | 1.003 | 1.000 | 1.000 | 1.000 | 0.997 | 0.999 | ok |
| lib/fmt_q | 2 | 145.6 | 122.3 | 148.3 | 120.3 | 103.3 | 238.6 | 226.1 | 1.410 | 1.184 | 0.988 | 1.000 | 1.000 | 1.000 | 1.006 | 1.000 | 0.999 | 1.000 | ok |
| lib/fmt_s | 5 | 7.3 | 7.0 | 7.6 | 7.1 | 3.2 | 217.6 | 222.2 | 2.312 | 2.216 | 0.976 | 1.000 | 1.002 | 0.999 | 0.998 | 1.000 | 0.991 | 1.000 | ok |
| lib/fmt_x_pad | 5 | 57.0 | 48.9 | 59.5 | 51.2 | 54.0 | 177.1 | 177.5 | 1.056 | 0.905 | 1.002 | 1.000 | 0.988 | 1.005 | 0.993 | 1.000 | 0.999 | 0.997 | ok |
| lib/getmt | 2 | 16.3 | 15.8 | 20.0 | 19.6 | 8.8 | 255.7 | 264.8 | 1.850 | 1.788 | 1.017 | 1.000 | 0.999 | 0.999 | 0.997 | 1.000 | 1.002 | 0.999 | ok |
| lib/gmatch_words | 20 | 176.9 | 136.1 | 178.2 | 138.0 | 173.3 | 315.9 | 319.8 | 1.021 | 0.785 | 0.997 | 1.000 | 0.992 | 1.003 | 1.003 | 1.000 | 1.000 | 1.001 | ok |
| lib/gsub_fn | 2 | 176.2 | 129.3 | 180.8 | 132.7 | 200.2 | 323.4 | 312.5 | 0.880 | 0.646 | 0.999 | 1.000 | 1.000 | 1.003 | 1.011 | 1.000 | 1.004 | 1.000 | ok |
| lib/gsub_str | 2 | 168.5 | 127.9 | 168.6 | 128.7 | 398.5 | 491.6 | 515.6 | 0.423 | 0.321 | 1.005 | 1.000 | 0.998 | 0.998 | 0.973 | 1.000 | 1.012 | 1.000 | ok |
| lib/gsub_tbl | 2 | 165.7 | 108.8 | 154.8 | 109.8 | 187.9 | 321.9 | 321.7 | 0.882 | 0.579 | 1.003 | 1.000 | 0.998 | 1.000 | 1.008 | 1.000 | 0.995 | 1.000 | ok |
| lib/it_ipairs | 20 | 239.4 | 235.8 | 282.6 | 262.2 | 251.6 | 5628.0 | 4827.6 | 0.952 | 0.937 | 0.999 | 1.000 | 1.000 | 1.000 | 1.014 | 1.000 | 0.989 | 1.000 | ok |
| lib/it_next | 20 | 44.6 | 44.2 | 44.7 | 44.3 | 180.5 | 371.1 | 401.4 | 0.247 | 0.245 | 0.985 | 1.000 | 0.998 | 1.000 | 0.991 | 1.000 | 1.002 | 1.000 | ok |
| lib/it_next_empty | 20 | 52.2 | 52.0 | 52.4 | 51.9 | 72.5 | 393.0 | 455.4 | 0.720 | 0.717 | 1.006 | 1.000 | 1.007 | 1.000 | 1.007 | 1.000 | 1.008 | 1.000 | ok |
| lib/it_pairs | 20 | 88.7 | 87.6 | 95.5 | 94.0 | 127.9 | 687.0 | 646.3 | 0.693 | 0.685 | 1.005 | 1.000 | 0.991 | 1.000 | 0.989 | 1.000 | 1.017 | 1.000 | ok |
| lib/it_pairs_hash | 20 | 104.3 | 104.7 | 113.9 | 110.4 | 118.1 | 989.3 | 957.1 | 0.883 | 0.886 | 1.004 | 1.000 | 1.003 | 1.000 | 1.010 | 1.000 | 1.000 | 1.000 | ok |
| lib/len_op | 20 | 48.6 | 47.8 | 58.1 | 57.4 | 100.7 | 1384.7 | 1419.8 | 0.482 | 0.475 | 1.004 | 1.000 | 1.003 | 1.000 | 1.004 | 1.000 | 0.996 | 1.000 | ok |
| lib/len_tbl | 2 | 140.5 | 140.3 | 158.6 | 158.2 | 159.7 | 372.6 | 337.9 | 0.880 | 0.878 | 1.007 | 1.000 | 1.019 | 1.000 | 1.007 | 1.000 | 1.005 | 1.000 | ok |
| lib/load_small | 20 | 241.3 | 154.1 | 228.7 | 159.5 | 215.6 | 378.8 | 345.2 | 1.119 | 0.715 | 1.044 | 1.000 | 0.958 | 1.000 | 1.028 | 1.015 | 0.998 | 1.017 | ok |
| lib/lower | 2 | 88.8 | 80.9 | 90.2 | 83.3 | 110.1 | 289.0 | 294.8 | 0.807 | 0.735 | 0.999 | 1.000 | 1.000 | 1.000 | 0.997 | 1.000 | 1.001 | 1.000 | ok |
| lib/m_abs | 2 | 248.0 | 247.2 | 247.0 | 247.1 | 390.3 | 6061.9 | 6506.8 | 0.636 | 0.633 | 1.012 | 1.000 | 1.011 | 1.000 | 1.004 | 1.000 | 1.006 | 1.000 | ok |
| lib/m_floor | 2 | 226.2 | 226.1 | 226.8 | 226.5 | 206.5 | 3681.1 | 4104.1 | 1.095 | 1.095 | 1.000 | 1.000 | 1.001 | 1.000 | 0.999 | 1.000 | 1.000 | 1.000 | ok |
| lib/m_fmod | 20 | 219.4 | 219.6 | 221.7 | 226.2 | 235.1 | 329.2 | 361.2 | 0.933 | 0.934 | 1.005 | 1.000 | 1.003 | 1.000 | 0.996 | 1.000 | 1.001 | 1.000 | ok |
| lib/m_max | 2 | 257.2 | 257.7 | 256.9 | 257.1 | 401.3 | 8004.8 | 10137.5 | 0.641 | 0.642 | 1.004 | 1.000 | 1.007 | 1.000 | 0.998 | 1.000 | 1.003 | 1.000 | ok |
| lib/m_min | 20 | 227.2 | 230.8 | 228.2 | 231.8 | 387.7 | 6530.7 | 8066.5 | 0.586 | 0.595 | 1.002 | 1.000 | 0.994 | 1.000 | 1.012 | 1.000 | 1.020 | 1.000 | ok |
| lib/m_pow_op | 2 | 280.5 | 280.1 | 280.5 | 280.2 | 275.2 | 1923.5 | 1956.0 | 1.020 | 1.018 | 1.001 | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | ok |
| lib/m_random | 2 | 79.1 | 78.7 | 79.0 | 78.4 | 73.2 | 345.7 | 375.3 | 1.080 | 1.074 | 0.998 | 1.000 | 0.998 | 1.000 | 0.992 | 1.000 | 0.997 | 1.000 | ok |
| lib/m_random_f | 2 | 53.0 | 52.7 | 53.0 | 52.7 | 89.9 | 272.7 | 312.0 | 0.589 | 0.586 | 0.995 | 1.000 | 0.993 | 1.000 | 0.991 | 1.000 | 0.994 | 1.000 | ok |
| lib/m_sin | 2 | 272.9 | 261.8 | 273.1 | 261.6 | 210.5 | 563.9 | 589.3 | 1.296 | 1.243 | 0.999 | 1.000 | 1.000 | 1.000 | 1.001 | 1.000 | 0.999 | 1.000 | ok |
| lib/m_sqrt | 2 | 278.0 | 274.6 | 281.0 | 283.2 | 270.0 | 2022.6 | 2132.3 | 1.030 | 1.017 | 1.001 | 1.000 | 0.990 | 1.000 | 1.014 | 1.000 | 1.012 | 1.000 | ok |
| lib/m_tointeger | 20 | 224.5 | 224.4 | 224.8 | 237.6 | - | 3718.4 | 3972.3 | - | - | 1.000 | 1.000 | 1.001 | 1.000 | 0.944 | 1.000 | 0.999 | 1.000 | ok |
| lib/m_type | 2 | 32.4 | 32.2 | 32.5 | 32.1 | - | 419.3 | 429.1 | - | - | 1.001 | 1.000 | 1.004 | 1.000 | 1.003 | 1.000 | 1.002 | 1.000 | ok |
| lib/m_ult | 20 | 276.3 | 281.1 | 277.8 | 282.6 | - | 415.2 | 435.8 | - | - | 1.000 | 1.000 | 0.984 | 1.000 | 1.006 | 1.000 | 0.997 | 1.000 | ok |
| lib/match | 20 | 61.1 | 51.6 | 61.5 | 52.4 | 193.6 | 319.9 | 303.8 | 0.316 | 0.267 | 0.997 | 1.000 | 1.002 | 1.009 | 1.004 | 1.000 | 1.002 | 1.000 | ok |
| lib/os_clock | 2 | 116.0 | 121.8 | 120.8 | 122.3 | 100.7 | 119.3 | 118.5 | 1.152 | 1.209 | 1.014 | 1.000 | 1.001 | 1.000 | 1.000 | 1.000 | 1.006 | 1.000 | ok |
| lib/pcall_err | 20 | 120.8 | 99.1 | 135.5 | 112.9 | 764.5 | 308.4 | 309.0 | 0.158 | 0.130 | 0.989 | 1.000 | 1.002 | 1.004 | 1.024 | 1.000 | 1.004 | 1.000 | ok |
| lib/pcall_ok | 2 | 30.4 | 29.9 | 30.6 | 30.1 | 41.0 | 1524.0 | 1518.3 | 0.741 | 0.731 | 0.996 | 1.000 | 0.999 | 1.000 | 1.012 | 1.000 | 0.998 | 1.000 | ok |
| lib/rawget_set | 5 | 19.1 | 18.7 | 19.1 | 18.9 | 14.2 | 381.1 | 417.2 | 1.340 | 1.313 | 0.994 | 1.000 | 1.000 | 1.000 | 1.001 | 1.000 | 0.989 | 1.000 | ok |
| lib/rep | 5 | 109.0 | 101.0 | 120.5 | 111.2 | 89.6 | 347.4 | 333.3 | 1.217 | 1.128 | 1.008 | 1.000 | 1.010 | 1.000 | 1.006 | 1.000 | 1.005 | 0.998 | ok |
| lib/rep_sep | 2 | 89.6 | 82.0 | 118.7 | 109.7 | 103.8 | 329.6 | 311.2 | 0.864 | 0.790 | 1.001 | 1.000 | 1.005 | 1.000 | 1.014 | 1.000 | 1.002 | 1.000 | ok |
| lib/reverse | 2 | 100.1 | 93.8 | 104.0 | 99.9 | 115.3 | 309.8 | 314.7 | 0.868 | 0.814 | 1.008 | 1.000 | 0.996 | 1.000 | 0.995 | 1.000 | 1.000 | 1.000 | ok |
| lib/sel_count | 20 | 215.2 | 215.9 | 216.5 | 215.8 | 388.2 | 15509.5 | 14438.4 | 0.554 | 0.556 | 0.990 | 1.000 | 0.989 | 1.000 | 0.995 | 1.000 | 0.989 | 1.000 | ok |
| lib/sel_loop | 17 | 213.3 | 211.5 | 217.8 | 214.7 | 5263.6 | 53689.2 | 54662.9 | 0.041 | 0.040 | 1.018 | 1.000 | 1.004 | 1.000 | 1.020 | 1.000 | 1.005 | 1.000 | ok |
| lib/sel_n | 20 | 331.9 | 330.6 | 331.8 | 329.7 | 675.4 | 25345.4 | 23674.6 | 0.491 | 0.489 | 1.016 | 1.000 | 1.006 | 1.000 | 1.002 | 1.000 | 0.999 | 1.000 | ok |
| lib/setmt | 20 | 184.5 | 161.8 | 227.9 | 195.2 | 331.9 | 743.5 | 835.9 | 0.556 | 0.487 | 0.993 | 1.000 | 1.005 | 1.000 | 1.009 | 1.000 | 1.027 | 1.007 | ok |
| lib/str_eq | 2 | 161.4 | 161.0 | 161.9 | 161.3 | 182.5 | 1479.5 | 1529.4 | 0.884 | 0.882 | 1.002 | 1.000 | 0.999 | 1.000 | 1.004 | 1.000 | 1.001 | 1.000 | ok |
| lib/str_eq_dyn | 5 | 135.7 | 134.9 | 134.9 | 135.3 | 150.4 | 1277.3 | 1348.8 | 0.903 | 0.897 | 1.002 | 1.000 | 0.987 | 1.000 | 0.997 | 1.000 | 0.997 | 1.000 | ok |
| lib/str_eq_long | 20 | 35.6 | 35.3 | 35.5 | 35.7 | 21.5 | 215.5 | 222.1 | 1.656 | 1.643 | 0.991 | 1.000 | 0.988 | 1.000 | 0.995 | 1.000 | 1.012 | 1.000 | ok |
| lib/str_idx_concat_key | 20 | 205.6 | 190.4 | 206.3 | 192.7 | 166.6 | 464.0 | 461.2 | 1.234 | 1.143 | 0.998 | 1.000 | 0.996 | 1.000 | 0.995 | 1.000 | 0.995 | 1.000 | ok |
| lib/str_lt | 2 | 94.5 | 92.0 | 90.4 | 87.9 | 66.3 | 227.1 | 228.3 | 1.425 | 1.386 | 0.998 | 1.000 | 0.997 | 1.000 | 1.002 | 1.000 | 1.001 | 1.000 | ok |
| lib/str_lt_short | 2 | 115.4 | 107.3 | 105.7 | 98.8 | 62.5 | 249.8 | 255.3 | 1.846 | 1.716 | 1.013 | 1.000 | 1.010 | 1.000 | 1.001 | 1.000 | 0.998 | 1.000 | ok |
| lib/str_method_len | 20 | 46.0 | 45.3 | 51.4 | 50.8 | 55.3 | 1386.6 | 1429.7 | 0.832 | 0.819 | 1.001 | 1.000 | 0.998 | 1.000 | 0.998 | 1.000 | 0.994 | 1.000 | ok |
| lib/string_key_const | 5 | 177.7 | 177.3 | 201.5 | 201.4 | 176.9 | 1151.6 | 1165.1 | 1.004 | 1.002 | 1.001 | 1.000 | 0.999 | 1.000 | 0.994 | 1.000 | 1.000 | 1.000 | ok |
| lib/string_key_dyn | 2 | 229.3 | 228.9 | 228.4 | 228.1 | 268.0 | 1082.3 | 1101.0 | 0.856 | 0.854 | 1.001 | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | 0.999 | 1.000 | ok |
| lib/sub | 20 | 131.9 | 122.3 | 130.6 | 121.0 | 80.5 | 469.5 | 482.7 | 1.638 | 1.519 | 1.001 | 1.000 | 0.996 | 1.000 | 0.995 | 1.000 | 0.987 | 1.000 | ok |
| lib/sub_1char | 2 | 121.5 | 110.0 | 122.0 | 108.8 | 98.2 | 459.2 | 478.8 | 1.238 | 1.120 | 1.014 | 1.000 | 1.001 | 1.000 | 1.001 | 1.000 | 0.996 | 1.000 | ok |
| lib/tconcat | 20 | 143.9 | 100.5 | 166.6 | 120.7 | 133.4 | 346.9 | 322.3 | 1.079 | 0.753 | 0.989 | 1.000 | 0.997 | 1.000 | 0.983 | 0.999 | 0.990 | 0.999 | ok |
| lib/tconcat_num | 5 | 175.1 | 118.3 | 183.9 | 121.0 | 246.0 | 908.0 | 844.8 | 0.712 | 0.481 | 0.996 | 1.000 | 1.003 | 1.000 | 1.000 | 1.000 | 1.005 | 0.999 | ok |
| lib/tfront_1k | 2 | 78.7 | 77.0 | 82.1 | 77.1 | 406.3 | 4375.7 | 4241.0 | 0.194 | 0.190 | 0.992 | 1.000 | 1.011 | 1.000 | 1.011 | 1.000 | 1.003 | 1.000 | ok |
| lib/tinsert_append | 20 | 158.2 | 134.6 | 193.3 | 183.5 | 141.7 | 539.7 | 574.7 | 1.116 | 0.950 | 0.993 | 1.000 | 1.001 | 1.000 | 0.988 | 1.000 | 1.004 | 1.000 | ok |
| lib/tinsert_front | 20 | 225.8 | 179.6 | 261.7 | 208.4 | 272.9 | 455.4 | 493.5 | 0.828 | 0.658 | 1.000 | 1.000 | 0.994 | 1.001 | 1.004 | 1.000 | 1.006 | 1.002 | ok |
| lib/tinsert_idx | 20 | 175.3 | 152.7 | 204.1 | 190.4 | 154.6 | 328.8 | 383.3 | 1.134 | 0.988 | 0.999 | 1.000 | 1.000 | 1.001 | 0.992 | 1.000 | 1.000 | 1.000 | ok |
| lib/tmove | 2 | 103.7 | 103.5 | 125.2 | 125.1 | 105.8 | 700.5 | 683.1 | 0.980 | 0.979 | 0.997 | 1.000 | 1.003 | 1.000 | 1.000 | 1.000 | 1.003 | 1.000 | ok |
| lib/tmove_1k | 20 | 97.0 | 68.3 | 109.5 | 71.0 | 111.6 | 476.4 | 447.3 | 0.869 | 0.612 | 0.990 | 1.000 | 0.998 | 1.000 | 1.009 | 1.000 | 1.003 | 1.000 | ok |
| lib/tonumber_bad | 17 | 156.0 | 113.8 | 166.7 | 142.5 | 168.0 | 408.1 | 431.1 | 0.929 | 0.678 | 1.019 | 1.000 | 0.895 | 1.004 | 0.987 | 1.000 | 0.994 | 0.996 | ok |
| lib/tonumber_base | 2 | 265.3 | 259.4 | 271.5 | 261.6 | 229.2 | 432.1 | 451.7 | 1.158 | 1.132 | 0.986 | 1.000 | 0.992 | 1.000 | 1.006 | 1.000 | 0.998 | 1.000 | ok |
| lib/tonumber_flt | 5 | 193.1 | 168.4 | 196.1 | 165.6 | 776.7 | 577.4 | 596.1 | 0.249 | 0.217 | 0.986 | 1.000 | 0.972 | 1.002 | 0.993 | 1.000 | 0.985 | 0.998 | ok |
| lib/tonumber_int | 20 | 154.8 | 140.5 | 160.1 | 142.3 | 96.2 | 288.1 | 305.0 | 1.610 | 1.461 | 0.999 | 1.000 | 0.999 | 1.000 | 0.988 | 1.000 | 1.000 | 1.000 | ok |
| lib/tonumber_num | 20 | 213.1 | 213.4 | 210.9 | 204.2 | 383.1 | 6476.7 | 7506.3 | 0.556 | 0.557 | 0.991 | 1.000 | 0.997 | 1.000 | 0.992 | 1.000 | 0.959 | 1.000 | ok |
| lib/tostring_flt | 20 | 83.1 | 66.0 | 85.0 | 61.6 | 82.5 | 299.3 | 214.9 | 1.008 | 0.800 | 1.000 | 1.000 | 1.006 | 1.000 | 1.009 | 1.000 | 0.999 | 0.996 | ok |
| lib/tostring_int | 20 | 121.5 | 108.9 | 134.2 | 123.2 | 279.8 | 543.3 | 450.7 | 0.434 | 0.389 | 0.996 | 1.000 | 1.004 | 1.002 | 0.980 | 1.000 | 1.007 | 0.993 | ok |
| lib/tostring_tbl | 2 | 108.5 | 105.4 | 111.7 | 108.6 | 93.3 | 313.8 | 280.9 | 1.162 | 1.130 | 1.000 | 1.000 | 0.985 | 0.999 | 0.988 | 1.000 | 0.978 | 1.000 | ok |
| lib/tpack | 5 | 150.3 | 117.5 | 212.8 | 169.9 | 306.1 | 499.0 | 485.7 | 0.491 | 0.384 | 0.989 | 1.000 | 0.998 | 1.000 | 1.015 | 1.000 | 0.985 | 1.001 | ok |
| lib/tremove_front | 5 | 149.0 | 133.7 | 186.9 | 170.8 | 191.0 | 592.3 | 593.7 | 0.780 | 0.700 | 0.998 | 1.000 | 1.002 | 1.000 | 1.000 | 1.000 | 1.019 | 1.005 | ok |
| lib/tremove_pop | 20 | 239.4 | 172.1 | 224.9 | 206.3 | 136.7 | 483.9 | 552.9 | 1.751 | 1.259 | 0.991 | 1.000 | 0.998 | 1.000 | 1.009 | 1.000 | 0.995 | 1.000 | ok |
| lib/tsort_cmp | 2 | 275.8 | 255.7 | 278.0 | 259.2 | 285.5 | 339.9 | 336.1 | 0.966 | 0.896 | 1.011 | 1.000 | 0.982 | 1.000 | 1.009 | 1.000 | 1.010 | 1.000 | ok |
| lib/tsort_int | 2 | 269.2 | 218.3 | 293.0 | 224.2 | 294.0 | 382.4 | 406.0 | 0.915 | 0.743 | 0.998 | 1.000 | 1.005 | 1.000 | 0.991 | 1.001 | 1.012 | 1.002 | ok |
| lib/tsort_str | 5 | 255.9 | 150.5 | 293.0 | 158.6 | 244.8 | 364.6 | 401.0 | 1.046 | 0.615 | 0.982 | 0.999 | 0.989 | 0.998 | 1.001 | 1.000 | 0.994 | 1.000 | ok |
| lib/tunpack | 5 | 124.1 | 103.6 | 126.2 | 112.2 | 154.8 | 487.1 | 448.5 | 0.802 | 0.670 | 0.990 | 1.000 | 0.995 | 1.000 | 1.006 | 1.000 | 1.002 | 1.008 | ok |
| lib/type_call | 2 | 87.9 | 87.6 | 94.6 | 94.4 | 85.6 | 494.3 | 468.3 | 1.027 | 1.023 | 1.002 | 1.000 | 1.002 | 1.000 | 1.001 | 1.000 | 0.995 | 1.000 | ok |
| lib/upper | 5 | 96.3 | 89.8 | 100.6 | 95.5 | 120.7 | 307.4 | 312.6 | 0.798 | 0.744 | 1.012 | 1.000 | 1.009 | 1.000 | 0.995 | 1.000 | 1.000 | 1.000 | ok |
| lib/utf8_len | 2 | 273.6 | 276.9 | 276.6 | 281.2 | - | 442.2 | 456.7 | - | - | 0.999 | 1.000 | 1.002 | 1.000 | 1.007 | 1.000 | 1.000 | 1.000 | ok |
| programs/binary_trees | 20 | 1244.4 | 1059.3 | 1454.5 | 1210.3 | 2007.4 | 4389.1 | 4447.3 | 0.620 | 0.528 | 0.811 | 0.762 | 0.813 | 0.738 | 0.835 | 0.789 | 0.816 | 0.769 | ok |
| programs/closures | 20 | 461.6 | 423.4 | 519.5 | 475.0 | 949.8 | 5063.2 | 4986.1 | 0.486 | 0.446 | 0.998 | 1.000 | 1.003 | 1.001 | 1.016 | 1.000 | 1.003 | 1.002 | ok |
| programs/coroutines | 20 | 1762.9 | 1319.5 | 1878.0 | 1418.6 | 871.7 | 3733.4 | 3656.9 | 2.022 | 1.514 | 0.975 | 1.000 | 1.003 | 0.999 | 1.026 | 1.000 | 1.058 | 1.000 | ok |
| programs/fannkuch | 2 | 1037.4 | 1053.5 | 1069.9 | 1075.3 | 1075.7 | 4075.1 | 4114.8 | 0.964 | 0.979 | 0.973 | 1.000 | 0.994 | 1.000 | 0.995 | 1.000 | 1.011 | 1.000 | ok |
| programs/fasta | 5 | 1288.2 | 1166.8 | 1373.0 | 1271.2 | 1495.7 | 3776.8 | 3842.3 | 0.861 | 0.780 | 0.992 | 1.000 | 1.001 | 1.000 | 0.993 | 1.000 | 1.023 | 1.000 | ok |
| programs/fib | 20 | 634.4 | 633.4 | 631.8 | 630.5 | 469.7 | 4139.5 | 4150.5 | 1.351 | 1.349 | 0.510 | 0.505 | 0.509 | 0.505 | 0.509 | 0.505 | 0.508 | 0.505 | ok |
| programs/gc_churn | 5 | 2223.4 | 1862.5 | 3377.9 | 2878.9 | 3608.7 | 4605.5 | 5988.9 | 0.616 | 0.516 | 1.018 | 1.000 | 1.011 | 1.002 | 1.023 | 1.000 | 1.016 | 1.001 | ok |
| programs/heapsort | 5 | 1070.4 | 1059.7 | 2246.7 | 2243.7 | 1286.6 | 4063.2 | 3957.0 | 0.832 | 0.824 | 1.000 | 1.000 | 0.987 | 1.000 | 1.006 | 1.000 | 1.001 | 1.000 | ok |
| programs/json | 2 | 1821.1 | 1433.9 | 1985.4 | 1536.7 | 1470.3 | 3127.6 | 3229.9 | 1.239 | 0.975 | 0.995 | 1.000 | 1.026 | 1.001 | 1.023 | 1.000 | 0.988 | 1.000 | ok |
| programs/knucleotide | 2 | 1046.8 | 1069.0 | 1066.1 | 1067.7 | 691.4 | 4061.5 | 4161.0 | 1.514 | 1.546 | 0.993 | 1.000 | 1.001 | 1.000 | 1.005 | 1.000 | 0.999 | 1.000 | ok |
| programs/load_big | 17 | 765.5 | 554.9 | 654.2 | 450.4 | 491.1 | 1787.3 | 620.0 | 1.559 | 1.130 | 1.031 | 1.000 | 1.021 | 1.000 | 0.996 | 1.000 | 0.998 | 1.000 | CHECK |
| programs/loops_arith | 2 | 479.4 | 478.8 | 483.0 | 482.6 | 1108.2 | 4344.5 | 4909.6 | 0.433 | 0.432 | 1.001 | 1.000 | 1.005 | 1.000 | 1.003 | 1.000 | 1.005 | 1.000 | CHECK |
| programs/mandelbrot | 2 | 533.9 | 533.1 | 534.9 | 534.4 | 521.6 | 4727.1 | 5530.2 | 1.024 | 1.022 | 1.000 | 1.000 | 0.999 | 1.000 | 1.000 | 1.000 | 0.999 | 1.000 | ok |
| programs/matmul | 2 | 442.4 | 442.6 | 449.3 | 443.9 | 272.0 | 3171.0 | 3693.4 | 1.627 | 1.627 | 1.017 | 1.000 | 1.021 | 1.000 | 1.015 | 1.000 | 0.999 | 1.000 | ok |
| programs/metamethods | 20 | 658.9 | 620.9 | 790.8 | 748.3 | 540.1 | 4614.3 | 4669.9 | 1.220 | 1.150 | 0.999 | 1.000 | 1.011 | 1.000 | 1.001 | 1.000 | 1.036 | 1.004 | ok |
| programs/nbody | 17 | 342.6 | 343.4 | 354.2 | 351.3 | 345.6 | 4246.2 | 4718.3 | 0.991 | 0.994 | 1.004 | 1.000 | 1.009 | 1.000 | 1.011 | 1.000 | 1.002 | 1.000 | ok |
| programs/oop_methods | 2 | 144.3 | 141.6 | 144.1 | 142.2 | 304.8 | 4567.3 | 4685.2 | 0.473 | 0.465 | 1.010 | 1.000 | 0.993 | 1.000 | 1.013 | 1.000 | 0.992 | 1.000 | ok |
| programs/pcall_error | 5 | 1483.2 | 1229.8 | 1485.5 | 1237.3 | 8346.6 | 4028.8 | 4363.2 | 0.178 | 0.147 | 1.001 | 1.000 | 1.006 | 1.002 | 1.007 | 1.000 | 1.003 | 1.000 | ok |
| programs/queens | 20 | 1850.6 | 1852.0 | 1927.8 | 1921.1 | 1907.1 | 4368.7 | 4120.8 | 0.970 | 0.971 | 1.003 | 1.000 | 1.002 | 1.000 | 1.005 | 1.000 | 1.000 | 1.000 | ok |
| programs/sieve | 5 | 587.9 | 589.7 | 618.3 | 681.7 | 761.7 | 2750.2 | 5029.1 | 0.772 | 0.774 | 1.013 | 1.000 | 1.025 | 1.000 | 1.021 | 1.000 | 1.009 | 1.000 | ok |
| programs/spectral_norm | 2 | 183.4 | 181.7 | 187.8 | 187.3 | 171.1 | 4573.7 | 4337.9 | 1.072 | 1.062 | 1.013 | 1.000 | 1.009 | 1.000 | 1.008 | 1.000 | 1.014 | 1.000 | ok |
| programs/string_ops | 20 | 1153.4 | 1039.0 | 1090.8 | 996.3 | 2238.4 | 2828.0 | 2807.1 | 0.515 | 0.464 | 1.020 | 1.000 | 0.982 | 1.005 | 1.009 | 1.000 | 1.009 | 1.000 | ok |
| programs/table_array | 2 | 373.7 | 301.2 | 402.1 | 311.9 | 600.0 | 3400.3 | 3492.4 | 0.623 | 0.502 | 1.023 | 1.000 | 0.994 | 1.000 | 0.983 | 1.000 | 1.002 | 1.000 | ok |
| programs/table_hash | 20 | 538.2 | 505.2 | 524.0 | 523.7 | 394.4 | 1501.6 | 1831.0 | 1.365 | 1.281 | 1.008 | 1.000 | 0.998 | 1.000 | 0.977 | 1.000 | 0.986 | 0.999 | ok |
| programs/varargs | 20 | 1236.8 | 1107.4 | 1206.2 | 1099.3 | 2194.8 | 5366.5 | 5084.7 | 0.564 | 0.505 | 1.005 | 1.000 | 1.014 | 1.000 | 0.999 | 1.000 | 1.006 | 1.002 | ok |
| real/game_loop | 2 | 35.2 | 33.7 | 39.9 | 38.4 | 21.7 | 207.6 | 221.0 | 1.623 | 1.551 | 1.021 | 1.001 | 1.017 | 0.999 | 1.015 | 1.000 | 1.013 | 0.999 | ok |
| real/json_roundtrip | 2 | 551.1 | 453.2 | 586.5 | 481.8 | 469.1 | 796.8 | 802.8 | 1.175 | 0.966 | 1.000 | 1.000 | 0.977 | 1.001 | 1.009 | 1.000 | 1.013 | 1.000 | ok |
| real/lua_interp | 5 | 106.6 | 102.6 | 106.4 | 102.3 | 59.3 | 223.6 | 225.0 | 1.798 | 1.730 | 0.958 | 1.004 | 0.945 | 1.004 | 0.951 | 1.010 | 0.954 | 1.011 | ok |
| real/template_engine | 20 | 716.2 | 587.7 | 751.5 | 624.0 | 955.0 | 1225.1 | 1249.2 | 0.750 | 0.615 | 0.986 | 1.000 | 0.958 | 1.005 | 1.001 | 1.000 | 0.998 | 1.001 | ok |
| real/wordfreq | 20 | 113.5 | 97.4 | 115.0 | 98.1 | 116.4 | 212.8 | 210.2 | 0.975 | 0.836 | 0.998 | 1.000 | 1.012 | 1.001 | 1.014 | 1.000 | 1.004 | 1.000 | ok |

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
