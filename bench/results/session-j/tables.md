# Tables, session-j (subject b116447)

Ratios below 1: first column faster/fewer. Ours = min over all readings (2 quick, 5 full, 15-run serial reruns). Cached columns as stored.


# Aggregates, readings A: the fast pass only (min of 2 or 5 runs per program)

## Headline geomeans (cycles, ours/ref)

| build | /LuaJIT cyc (n) | /LuaJIT ins | /PUC same version cyc (223) | /PUC ins |
|---|---|---|---|---|
| 5.5 plain | 0.960 (219) | 1.350 | 0.219 | 0.221 |
| 5.5 PGO | 0.874 (219) | 1.245 | 0.200 | 0.204 |
| 5.4 plain | 1.019 (219) | 1.442 | 0.227 | 0.235 |
| 5.4 PGO | 0.934 (219) | 1.331 | 0.209 | 0.217 |

- 5.5 plain / LuaJIT -joff (219): 0.325
- 5.5 PGO / LuaJIT -joff (219): 0.296
- cached session-i 22670a1 (same programs): 5.5 plain/LJ 0.950, PGO/LJ 0.869, 5.4 plain/LJ 1.010, 5.4 PGO/LJ 0.925
- cached 8c39465 (same programs): 5.5 plain/LJ 0.955, PGO/LJ 0.871, 5.4 plain/LJ 1.020, 5.4 PGO/LJ 0.929
- cached 1a5aad0 (same programs): 5.5 plain/LJ 0.964, PGO/LJ 0.879, 5.4 plain/LJ 1.020, 5.4 PGO/LJ 0.934
- cached 108b935 (same programs): plain/LJ 0.969, PGO/LJ 0.883
- our 5.4 build / our 5.5 build cycles: plain 1.061, PGO 1.067; PGO/plain: 5.5 0.912, 5.4 0.917

## Subject vs session-i 22670a1 (set B merged), session-i set A, session-h 8c39465, 1a5aad0 and 108b935 (geomean, 223 programs)

| build | vs | cycles | instructions | cyc >3% better / within / >3% worse | ins >3% fewer / more |
|---|---|---|---|---|---|
| 5.5 plain | 22670a1 | 1.010 | 0.993 | 6 / 187 / 30 | 11 / 0 |
| 5.5 plain | 22670a1 setA | 1.006 | 0.993 | 9 / 192 / 22 | 11 / 0 |
| 5.5 plain | 8c39465 | 1.006 | 0.986 | 14 / 185 / 24 | 21 / 1 |
| 5.5 plain | 1a5aad0 | 0.997 | 0.969 | 29 / 158 / 36 | 41 / 1 |
| 5.5 plain | 108b935 | 0.991 | 0.949 | 45 / 144 / 34 | 94 / 0 |
| 5.5 PGO | 22670a1 | 1.006 | 0.994 | 7 / 191 / 25 | 8 / 1 |
| 5.5 PGO | 22670a1 setA | 1.001 | 0.994 | 10 / 193 / 20 | 8 / 0 |
| 5.5 PGO | 8c39465 | 1.004 | 0.987 | 10 / 184 / 29 | 18 / 1 |
| 5.5 PGO | 1a5aad0 | 0.995 | 0.968 | 23 / 176 / 24 | 45 / 0 |
| 5.5 PGO | 108b935 | 0.991 | 0.947 | 45 / 147 / 31 | 97 / 1 |
| 5.4 plain | 22670a1 | 1.009 | 0.997 | 5 / 188 / 30 | 8 / 0 |
| 5.4 plain | 22670a1 setA | 1.003 | 0.997 | 10 / 193 / 20 | 8 / 0 |
| 5.4 plain | 8c39465 | 1.000 | 0.988 | 14 / 176 / 33 | 15 / 3 |
| 5.4 plain | 1a5aad0 | 0.999 | 0.975 | 28 / 150 / 45 | 34 / 3 |
| 5.4 PGO | 22670a1 | 1.009 | 0.998 | 3 / 202 / 18 | 5 / 0 |
| 5.4 PGO | 22670a1 setA | 1.005 | 0.998 | 6 / 203 / 14 | 5 / 0 |
| 5.4 PGO | 8c39465 | 1.005 | 0.989 | 11 / 186 / 26 | 14 / 3 |
| 5.4 PGO | 1a5aad0 | 1.000 | 0.974 | 23 / 174 / 26 | 36 / 3 |

### Per set, geomean subject / 22670a1 and subject / 1a5aad0 (cycles / instructions)

| set | n | plain vs i cyc | ins | PGO vs i cyc | ins | 5.4 plain vs i cyc | ins | 5.4 PGO vs i cyc | ins | plain vs 1a5 cyc | ins | PGO vs 1a5 cyc | ins | 5.4 plain vs 1a5 cyc | ins | 5.4 PGO vs 1a5 cyc | ins |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| apps | 5 | 1.000 | 0.975 | 0.996 | 0.980 | 0.998 | 0.976 | 0.991 | 0.980 | 0.872 | 0.839 | 0.905 | 0.839 | 0.930 | 0.862 | 0.961 | 0.864 |
| awfy | 14 | 1.011 | 0.988 | 1.014 | 0.990 | 1.011 | 0.989 | 1.014 | 0.991 | 0.973 | 0.920 | 0.977 | 0.922 | 0.964 | 0.920 | 0.970 | 0.920 |
| corpus | 18 | 0.977 | 0.968 | 0.978 | 0.976 | 0.995 | 0.986 | 0.995 | 0.989 | 0.963 | 0.944 | 0.965 | 0.945 | 0.992 | 0.970 | 0.983 | 0.970 |
| corpus2/dev | 17 | 1.041 | 0.994 | 1.036 | 0.995 | 1.033 | 0.998 | 1.031 | 0.999 | 1.023 | 0.960 | 1.023 | 0.956 | 1.020 | 0.971 | 1.019 | 0.967 |
| corpus2/heldout | 11 | 1.030 | 0.955 | 1.010 | 0.956 | 1.027 | 0.992 | 1.016 | 0.992 | 0.985 | 0.865 | 0.962 | 0.865 | 0.960 | 0.878 | 0.958 | 0.876 |
| jit-trace | 29 | 1.002 | 1.000 | 1.003 | 0.999 | 1.003 | 1.000 | 1.002 | 1.000 | 1.003 | 0.991 | 1.007 | 0.992 | 1.002 | 0.993 | 1.003 | 0.993 |
| lib | 95 | 1.007 | 1.000 | 1.002 | 1.000 | 1.005 | 1.000 | 1.003 | 1.000 | 1.005 | 0.995 | 0.999 | 0.993 | 1.008 | 0.997 | 1.003 | 0.995 |
| programs | 25 | 1.027 | 1.000 | 1.015 | 1.000 | 1.016 | 1.000 | 1.033 | 1.000 | 1.017 | 0.981 | 1.018 | 0.981 | 1.013 | 0.984 | 1.034 | 0.983 |
| real | 5 | 1.027 | 0.996 | 1.020 | 0.997 | 1.013 | 0.997 | 1.007 | 0.998 | 0.965 | 0.932 | 0.970 | 0.936 | 0.958 | 0.938 | 0.952 | 0.942 |
| real_dis | 4 | 1.003 | 0.984 | 1.008 | 0.982 | 1.013 | 1.000 | 1.005 | 1.000 | 0.987 | 0.968 | 1.002 | 0.957 | 1.014 | 1.000 | 0.996 | 0.994 |
| ALL | 223 | 1.010 | 0.993 | 1.006 | 0.994 | 1.009 | 0.997 | 1.009 | 0.998 | 0.997 | 0.969 | 0.995 | 0.968 | 0.999 | 0.975 | 1.000 | 0.974 |

# Aggregates, readings B: with the 15-run serial reruns of 83 programs merged (min over all readings)

## Headline geomeans (cycles, ours/ref)

| build | /LuaJIT cyc (n) | /LuaJIT ins | /PUC same version cyc (223) | /PUC ins |
|---|---|---|---|---|
| 5.5 plain | 0.952 (219) | 1.350 | 0.217 | 0.221 |
| 5.5 PGO | 0.868 (219) | 1.245 | 0.199 | 0.204 |
| 5.4 plain | 1.011 (219) | 1.442 | 0.226 | 0.235 |
| 5.4 PGO | 0.926 (219) | 1.331 | 0.207 | 0.217 |

- 5.5 plain / LuaJIT -joff (219): 0.322
- 5.5 PGO / LuaJIT -joff (219): 0.293
- cached session-i 22670a1 (same programs): 5.5 plain/LJ 0.950, PGO/LJ 0.869, 5.4 plain/LJ 1.010, 5.4 PGO/LJ 0.925
- cached 8c39465 (same programs): 5.5 plain/LJ 0.955, PGO/LJ 0.871, 5.4 plain/LJ 1.020, 5.4 PGO/LJ 0.929
- cached 1a5aad0 (same programs): 5.5 plain/LJ 0.964, PGO/LJ 0.879, 5.4 plain/LJ 1.020, 5.4 PGO/LJ 0.934
- cached 108b935 (same programs): plain/LJ 0.969, PGO/LJ 0.883
- our 5.4 build / our 5.5 build cycles: plain 1.061, PGO 1.066; PGO/plain: 5.5 0.913, 5.4 0.917

## Subject vs session-i 22670a1 (set B merged), session-i set A, session-h 8c39465, 1a5aad0 and 108b935 (geomean, 223 programs)

| build | vs | cycles | instructions | cyc >3% better / within / >3% worse | ins >3% fewer / more |
|---|---|---|---|---|---|
| 5.5 plain | 22670a1 | 1.002 | 0.993 | 7 / 200 / 16 | 11 / 0 |
| 5.5 plain | 22670a1 setA | 0.997 | 0.993 | 12 / 199 / 12 | 11 / 0 |
| 5.5 plain | 8c39465 | 0.997 | 0.986 | 17 / 192 / 14 | 21 / 1 |
| 5.5 plain | 1a5aad0 | 0.988 | 0.969 | 38 / 160 / 25 | 41 / 1 |
| 5.5 plain | 108b935 | 0.982 | 0.948 | 50 / 147 / 26 | 94 / 0 |
| 5.5 PGO | 22670a1 | 0.999 | 0.994 | 7 / 205 / 11 | 8 / 0 |
| 5.5 PGO | 22670a1 setA | 0.994 | 0.993 | 13 / 202 / 8 | 9 / 0 |
| 5.5 PGO | 8c39465 | 0.996 | 0.987 | 12 / 196 / 15 | 18 / 1 |
| 5.5 PGO | 1a5aad0 | 0.988 | 0.968 | 31 / 176 / 16 | 45 / 0 |
| 5.5 PGO | 108b935 | 0.984 | 0.947 | 53 / 146 / 24 | 97 / 1 |
| 5.4 plain | 22670a1 | 1.001 | 0.997 | 8 / 207 / 8 | 8 / 0 |
| 5.4 plain | 22670a1 setA | 0.995 | 0.997 | 13 / 205 / 5 | 8 / 0 |
| 5.4 plain | 8c39465 | 0.992 | 0.988 | 18 / 189 / 16 | 15 / 3 |
| 5.4 plain | 1a5aad0 | 0.992 | 0.975 | 31 / 166 / 26 | 34 / 3 |
| 5.4 PGO | 22670a1 | 1.001 | 0.998 | 5 / 212 / 6 | 5 / 0 |
| 5.4 PGO | 22670a1 setA | 0.996 | 0.998 | 6 / 213 / 4 | 5 / 0 |
| 5.4 PGO | 8c39465 | 0.996 | 0.989 | 13 / 197 / 13 | 14 / 3 |
| 5.4 PGO | 1a5aad0 | 0.991 | 0.974 | 27 / 179 / 17 | 36 / 3 |

### Per set, geomean subject / 22670a1 and subject / 1a5aad0 (cycles / instructions)

| set | n | plain vs i cyc | ins | PGO vs i cyc | ins | 5.4 plain vs i cyc | ins | 5.4 PGO vs i cyc | ins | plain vs 1a5 cyc | ins | PGO vs 1a5 cyc | ins | 5.4 plain vs 1a5 cyc | ins | 5.4 PGO vs 1a5 cyc | ins |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| apps | 5 | 1.000 | 0.975 | 0.995 | 0.980 | 0.996 | 0.976 | 0.988 | 0.980 | 0.872 | 0.839 | 0.904 | 0.839 | 0.928 | 0.862 | 0.959 | 0.863 |
| awfy | 14 | 0.999 | 0.988 | 1.001 | 0.990 | 1.000 | 0.989 | 0.997 | 0.991 | 0.961 | 0.920 | 0.965 | 0.922 | 0.952 | 0.920 | 0.954 | 0.920 |
| corpus | 18 | 0.973 | 0.968 | 0.973 | 0.973 | 0.992 | 0.986 | 0.991 | 0.989 | 0.959 | 0.944 | 0.960 | 0.943 | 0.989 | 0.970 | 0.979 | 0.970 |
| corpus2/dev | 17 | 1.028 | 0.994 | 1.019 | 0.995 | 1.014 | 0.998 | 1.015 | 0.999 | 1.011 | 0.960 | 1.006 | 0.956 | 1.001 | 0.971 | 1.004 | 0.967 |
| corpus2/heldout | 11 | 0.991 | 0.955 | 0.994 | 0.956 | 1.013 | 0.992 | 1.006 | 0.992 | 0.947 | 0.865 | 0.947 | 0.865 | 0.947 | 0.878 | 0.948 | 0.876 |
| jit-trace | 29 | 1.001 | 1.000 | 1.002 | 0.999 | 1.002 | 1.000 | 1.001 | 1.000 | 1.002 | 0.991 | 1.005 | 0.992 | 1.001 | 0.993 | 1.002 | 0.993 |
| lib | 95 | 1.002 | 1.000 | 0.997 | 1.000 | 0.999 | 1.000 | 0.999 | 1.000 | 1.000 | 0.995 | 0.994 | 0.993 | 1.002 | 0.997 | 0.999 | 0.995 |
| programs | 25 | 1.011 | 1.000 | 1.006 | 1.000 | 1.004 | 1.000 | 1.005 | 1.000 | 1.002 | 0.981 | 1.008 | 0.981 | 1.001 | 0.984 | 1.006 | 0.983 |
| real | 5 | 1.016 | 0.996 | 1.005 | 0.997 | 1.004 | 0.997 | 0.997 | 0.998 | 0.954 | 0.932 | 0.956 | 0.936 | 0.949 | 0.938 | 0.942 | 0.942 |
| real_dis | 4 | 0.996 | 0.984 | 1.000 | 0.982 | 1.006 | 1.000 | 0.999 | 1.000 | 0.981 | 0.968 | 0.993 | 0.957 | 1.006 | 1.000 | 0.990 | 0.994 |
| ALL | 223 | 1.002 | 0.993 | 0.999 | 0.994 | 1.001 | 0.997 | 1.001 | 0.998 | 0.988 | 0.969 | 0.988 | 0.968 | 0.992 | 0.975 | 0.991 | 0.974 |

### 5.5 plain vs 22670a1: 20 biggest improvements by cycles (>= 20 Mcyc)

| program | Mcyc | cyc ratio | ins ratio | cycle spread of readings | note |
|---|---|---|---|---|---|
| corpus/classic_vector | 480 | 0.746 | 0.734 | 10.5% (20 runs) | instructions move the same way |
| corpus2/heldout/dijkstra_routes | 1192 | 0.823 | 0.641 | 4.6% (20 runs) | instructions move the same way |
| corpus/lua_lexer | 1088 | 0.879 | 0.861 | 15.8% (20 runs) | instructions move the same way |
| corpus/sha2_base64 | 386 | 0.892 | 0.936 | 4.6% (20 runs) | instructions move the same way |
| lib/tconcat | 143 | 0.942 | 0.998 | 11.2% (20 runs) | instructions equal: layout / IPC |
| lib/find_pat | 263 | 0.970 | 1.000 | 5.4% (20 runs) | instructions equal: layout / IPC |
| apps/expr_lang | 136 | 0.970 | 0.966 | 2.8% (20 runs) | instructions move the same way |
| apps/raytrace | 112 | 0.972 | 0.953 | 5.7% (20 runs) |  |
| corpus2/heldout/roguelike_turns | 813 | 0.976 | 0.989 | 16.4% (20 runs) |  |
| lib/m_type | 32 | 0.977 | 1.000 | 3.1% (20 runs) |  |
| corpus2/dev/text_adventure | 2535 | 0.977 | 1.000 | 5.7% (20 runs) |  |
| corpus2/heldout/scheme_interp | 1414 | 0.978 | 0.988 | 11.4% (20 runs) |  |
| corpus2/heldout/bigint_rsa | 1105 | 0.979 | 1.000 | 3.9% (5 runs) |  |
| awfy/towers | 49 | 0.979 | 0.965 | 15.9% (20 runs) |  |
| lib/tinsert_idx | 175 | 0.979 | 1.000 | 75.5% (20 runs) |  |
| lib/setmt | 184 | 0.979 | 1.000 | 12.8% (20 runs) |  |
| lib/gmatch_words | 175 | 0.980 | 1.000 | 14.8% (20 runs) |  |
| lib/match | 62 | 0.980 | 1.000 | 8.7% (20 runs) |  |
| corpus2/heldout/stream_closures | 798 | 0.981 | 0.980 | 2.4% (5 runs) |  |
| lib/gsub_fn | 174 | 0.982 | 1.000 | 8.5% (20 runs) |  |

### 5.5 PGO vs 22670a1: 20 biggest improvements by cycles (>= 20 Mcyc)

| program | Mcyc | cyc ratio | ins ratio | cycle spread of readings | note |
|---|---|---|---|---|---|
| corpus/classic_vector | 443 | 0.773 | 0.810 | 8.2% (20 runs) | instructions move the same way |
| corpus2/heldout/dijkstra_routes | 1168 | 0.825 | 0.639 | 4.2% (20 runs) | instructions move the same way |
| corpus/lua_lexer | 966 | 0.881 | 0.849 | 14.0% (20 runs) | instructions move the same way |
| corpus/sha2_base64 | 367 | 0.896 | 0.930 | 3.9% (20 runs) | instructions move the same way |
| lib/find_pat | 227 | 0.959 | 1.000 | 4.9% (20 runs) | instructions equal: layout / IPC |
| lib/len_tbl | 137 | 0.966 | 1.000 | 1.6% (2 runs) | instructions equal: layout / IPC |
| lib/tonumber_bad | 116 | 0.969 | 0.996 | 7.3% (17 runs) | instructions equal: layout / IPC |
| corpus2/dev/wordfreq_wrap | 501 | 0.970 | 0.945 | 16.5% (20 runs) |  |
| lib/tsort_str | 151 | 0.972 | 0.993 | 2.4% (5 runs) |  |
| corpus2/heldout/markov_text | 1105 | 0.972 | 0.993 | 18.7% (20 runs) |  |
| lib/tsort_cmp | 254 | 0.973 | 0.993 | 3.0% (20 runs) |  |
| lib/m_tointeger | 223 | 0.975 | 1.000 | 7.0% (20 runs) |  |
| programs/fib | 1260 | 0.976 | 1.000 | 8.7% (20 runs) |  |
| apps/expr_lang | 125 | 0.977 | 0.976 | 13.2% (20 runs) |  |
| apps/raytrace | 105 | 0.977 | 0.959 | 4.4% (20 runs) |  |
| lib/it_next | 44 | 0.978 | 1.000 | 31.8% (20 runs) |  |
| real/template_engine | 581 | 0.978 | 1.000 | 7.6% (20 runs) |  |
| corpus2/heldout/roguelike_turns | 774 | 0.978 | 0.992 | 7.3% (20 runs) |  |
| lib/tonumber_flt | 169 | 0.979 | 0.996 | 0.3% (2 runs) |  |
| lib/setmt | 159 | 0.979 | 0.998 | 4.7% (20 runs) |  |

### 5.4 plain vs 22670a1: 20 biggest improvements by cycles (>= 20 Mcyc)

| program | Mcyc | cyc ratio | ins ratio | cycle spread of readings | note |
|---|---|---|---|---|---|
| corpus/classic_vector | 648 | 0.803 | 0.782 | 7.6% (20 runs) | instructions move the same way |
| lib/m_min | 234 | 0.940 | 1.000 | 23.5% (20 runs) | instructions equal: layout / IPC |
| corpus2/dev/diff_levenshtein | 439 | 0.947 | 1.000 | 5.7% (20 runs) | instructions equal: layout / IPC |
| corpus2/heldout/dijkstra_routes | 1462 | 0.958 | 0.935 | 4.0% (20 runs) | instructions move the same way |
| awfy/towers | 49 | 0.963 | 0.966 | 47.0% (20 runs) | instructions move the same way |
| apps/raytrace | 116 | 0.965 | 0.949 | 6.9% (20 runs) | instructions move the same way |
| lib/tsort_cmp | 270 | 0.966 | 1.002 | 4.0% (20 runs) | instructions equal: layout / IPC |
| lib/load_small | 220 | 0.967 | 1.000 | 1.2% (5 runs) | instructions equal: layout / IPC |
| programs/binary_trees | 1752 | 0.973 | 1.000 | 9.3% (20 runs) |  |
| corpus2/dev/text_adventure | 2689 | 0.977 | 1.000 | 8.6% (20 runs) |  |
| lib/str_eq_dyn | 134 | 0.978 | 1.000 | 20.5% (20 runs) |  |
| lib/setmt | 224 | 0.979 | 1.000 | 5.3% (20 runs) |  |
| lib/gmatch_words | 176 | 0.980 | 1.000 | 5.4% (20 runs) |  |
| lib/m_type | 32 | 0.981 | 0.999 | 3.9% (20 runs) |  |
| lib/tpack | 210 | 0.982 | 1.007 | 8.2% (20 runs) |  |
| programs/closures | 505 | 0.982 | 1.000 | 23.2% (20 runs) |  |
| lib/str_lt | 90 | 0.982 | 1.000 | 1.1% (5 runs) |  |
| lib/tinsert_front | 259 | 0.983 | 0.992 | 5.6% (20 runs) |  |
| lib/rep | 120 | 0.983 | 1.000 | 1.2% (2 runs) |  |
| lib/str_lt_short | 105 | 0.984 | 1.000 | 2.1% (20 runs) |  |

### 5.4 PGO vs 22670a1: 20 biggest improvements by cycles (>= 20 Mcyc)

| program | Mcyc | cyc ratio | ins ratio | cycle spread of readings | note |
|---|---|---|---|---|---|
| corpus/classic_vector | 556 | 0.826 | 0.828 | 12.4% (20 runs) | instructions move the same way |
| corpus2/dev/diff_levenshtein | 409 | 0.945 | 1.000 | 6.5% (20 runs) | instructions equal: layout / IPC |
| lib/pcall_err | 107 | 0.955 | 1.000 | 38.0% (20 runs) | instructions equal: layout / IPC |
| corpus2/heldout/dijkstra_routes | 1423 | 0.957 | 0.934 | 8.9% (20 runs) | instructions move the same way |
| apps/raytrace | 108 | 0.968 | 0.956 | 5.1% (20 runs) | instructions move the same way |
| awfy/towers | 47 | 0.970 | 0.974 | 28.9% (20 runs) |  |
| lib/load_small | 163 | 0.971 | 1.000 | 3.8% (5 runs) |  |
| corpus/csv_ini_yaml | 1594 | 0.973 | 1.000 | 4.3% (20 runs) |  |
| apps/events | 150 | 0.978 | 1.000 | 13.3% (20 runs) |  |
| corpus2/heldout/stream_closures | 792 | 0.978 | 0.987 | 1.2% (5 runs) |  |
| lib/str_eq_long | 35 | 0.979 | 1.000 | 12.4% (20 runs) |  |
| lib/match | 52 | 0.980 | 1.000 | 20.9% (20 runs) |  |
| corpus2/heldout/log_analyzer | 1859 | 0.981 | 1.000 | 3.3% (17 runs) |  |
| lib/tinsert_front | 207 | 0.982 | 0.985 | 7.9% (20 runs) |  |
| lib/m_type | 32 | 0.982 | 1.000 | 10.2% (20 runs) |  |
| lib/m_random | 79 | 0.983 | 1.000 | 2.0% (5 runs) |  |
| corpus/inspect_lua | 1512 | 0.984 | 1.000 | 0.7% (5 runs) |  |
| corpus/sha2_base64 | 368 | 0.984 | 1.000 | 5.0% (20 runs) |  |
| lib/setmt | 189 | 0.985 | 1.000 | 4.7% (20 runs) |  |
| lib/rep | 110 | 0.985 | 1.000 | 0.1% (2 runs) |  |

### 5.5 plain vs 22670a1: regressions (instructions > +2% or cycles > +3%, >= 20 Mcyc)

| program | Mcyc | cyc ratio | ins ratio | cycle spread of readings | note |
|---|---|---|---|---|---|
| corpus2/heldout/prototype_objects | 1694 | 1.164 | 0.996 | 59.0% (20 runs) | instructions equal: layout / IPC |
| corpus2/dev/csv_report | 1289 | 1.121 | 0.994 | 32.5% (20 runs) | instructions equal: layout / IPC |
| corpus2/dev/regex_nfa | 709 | 1.091 | 0.999 | 5.1% (20 runs) | instructions equal: layout / IPC |
| real/lua_interp | 111 | 1.080 | 0.996 | 2.7% (17 runs) | instructions equal: layout / IPC |
| lib/co_yield | 243 | 1.077 | 1.000 | 7.6% (20 runs) | instructions equal: layout / IPC |
| lib/tonumber_bad | 154 | 1.066 | 1.000 | 9.6% (17 runs) | instructions equal: layout / IPC |
| programs/coroutines | 1813 | 1.056 | 1.000 | 7.7% (17 runs) | instructions equal: layout / IPC |
| corpus2/dev/oop_mixins | 1504 | 1.055 | 0.996 | 12.8% (20 runs) | instructions equal: layout / IPC |
| apps/events | 170 | 1.051 | 0.999 | 39.2% (20 runs) | instructions equal: layout / IPC |
| lib/tunpack | 124 | 1.050 | 1.000 | 6.6% (20 runs) | instructions equal: layout / IPC |
| corpus2/dev/iter_pipeline | 180 | 1.041 | 0.994 | 7.5% (20 runs) | instructions equal: layout / IPC |
| programs/table_hash | 544 | 1.037 | 1.001 | 72.6% (20 runs) | instructions equal: layout / IPC |
| lib/tmove_1k | 99 | 1.034 | 1.000 | 12.3% (20 runs) | instructions equal: layout / IPC |
| corpus2/dev/diff_levenshtein | 443 | 1.033 | 1.000 | 6.8% (20 runs) | instructions equal: layout / IPC |
| corpus/csv_ini_yaml | 1631 | 1.033 | 0.994 | 18.0% (20 runs) | instructions equal: layout / IPC |

### 5.5 PGO vs 22670a1: regressions (instructions > +2% or cycles > +3%, >= 20 Mcyc)

| program | Mcyc | cyc ratio | ins ratio | cycle spread of readings | note |
|---|---|---|---|---|---|
| corpus2/heldout/prototype_objects | 1607 | 1.145 | 0.997 | 45.5% (20 runs) | instructions equal: layout / IPC |
| corpus2/dev/csv_report | 1143 | 1.115 | 0.998 | 29.5% (20 runs) | instructions equal: layout / IPC |
| real/lua_interp | 107 | 1.068 | 0.998 | 1.9% (17 runs) | instructions equal: layout / IPC |
| corpus2/dev/iter_pipeline | 175 | 1.063 | 0.995 | 7.2% (20 runs) | instructions equal: layout / IPC |
| lib/concat_op_str | 124 | 1.054 | 1.017 | 10.4% (20 runs) | instructions move the same way |
| programs/table_hash | 539 | 1.053 | 1.000 | 63.4% (20 runs) | instructions equal: layout / IPC |
| corpus2/dev/regex_nfa | 593 | 1.049 | 0.999 | 3.8% (20 runs) | instructions equal: layout / IPC |
| corpus2/dev/oop_mixins | 1407 | 1.042 | 0.998 | 13.8% (20 runs) | instructions equal: layout / IPC |
| corpus2/dev/memo_cache | 1557 | 1.036 | 0.998 | 10.2% (20 runs) | instructions equal: layout / IPC |
| corpus2/heldout/log_analyzer | 1825 | 1.035 | 0.998 | 7.2% (17 runs) | instructions equal: layout / IPC |
| lib/tpack | 117 | 1.033 | 1.023 | 12.1% (20 runs) | instructions move the same way |

### 5.4 plain vs 22670a1: regressions (instructions > +2% or cycles > +3%, >= 20 Mcyc)

| program | Mcyc | cyc ratio | ins ratio | cycle spread of readings | note |
|---|---|---|---|---|---|
| corpus2/heldout/prototype_objects | 1693 | 1.168 | 0.995 | 54.6% (20 runs) | instructions equal: layout / IPC |
| corpus2/dev/csv_report | 1307 | 1.101 | 0.999 | 36.0% (20 runs) | instructions equal: layout / IPC |
| programs/table_hash | 562 | 1.070 | 1.002 | 62.4% (20 runs) | instructions equal: layout / IPC |
| lib/m_tointeger | 237 | 1.058 | 1.000 | 1.6% (20 runs) | instructions equal: layout / IPC |
| programs/load_big | 673 | 1.056 | 1.000 | 10.3% (20 runs) | instructions equal: layout / IPC |
| lib/find_pat | 287 | 1.056 | 1.000 | 11.0% (20 runs) | instructions equal: layout / IPC |
| corpus2/dev/iter_pipeline | 191 | 1.038 | 0.996 | 8.4% (20 runs) | instructions equal: layout / IPC |
| corpus2/dev/lua_parser | 1589 | 1.038 | 0.994 | 18.7% (20 runs) | instructions equal: layout / IPC |

### 5.4 PGO vs 22670a1: regressions (instructions > +2% or cycles > +3%, >= 20 Mcyc)

| program | Mcyc | cyc ratio | ins ratio | cycle spread of readings | note |
|---|---|---|---|---|---|
| corpus2/heldout/prototype_objects | 1652 | 1.192 | 0.997 | 53.6% (20 runs) | instructions equal: layout / IPC |
| corpus2/dev/csv_report | 1165 | 1.085 | 0.999 | 36.0% (20 runs) | instructions equal: layout / IPC |
| programs/table_hash | 543 | 1.053 | 1.001 | 67.5% (20 runs) | instructions equal: layout / IPC |
| corpus2/dev/memo_cache | 1598 | 1.051 | 0.998 | 8.5% (20 runs) | instructions equal: layout / IPC |
| corpus2/dev/iter_pipeline | 181 | 1.035 | 0.997 | 12.3% (20 runs) | instructions equal: layout / IPC |
| corpus2/dev/oop_mixins | 1384 | 1.033 | 1.000 | 9.6% (20 runs) | instructions equal: layout / IPC |

### 5.5 plain vs 8c39465: 20 biggest improvements by cycles (>= 20 Mcyc)

| program | Mcyc | cyc ratio | ins ratio | cycle spread of readings | note |
|---|---|---|---|---|---|
| lib/tremove_front | 147 | 0.583 | 0.547 | 3.5% (20 runs) | instructions move the same way |
| corpus/classic_vector | 480 | 0.763 | 0.733 | 10.5% (20 runs) | instructions move the same way |
| awfy/list | 97 | 0.779 | 0.636 | 4.7% (5 runs) | instructions move the same way |
| apps/raytrace | 112 | 0.803 | 0.798 | 5.7% (20 runs) | instructions move the same way |
| corpus2/heldout/stream_closures | 798 | 0.854 | 0.832 | 2.4% (5 runs) | instructions move the same way |
| corpus/lua_lexer | 1088 | 0.861 | 0.861 | 15.8% (20 runs) | instructions move the same way |
| corpus/sha2_base64 | 386 | 0.907 | 0.936 | 4.6% (20 runs) | instructions move the same way |
| programs/load_big | 750 | 0.927 | 1.000 | 3.9% (20 runs) | instructions equal: layout / IPC |
| corpus2/heldout/dijkstra_routes | 1192 | 0.937 | 0.828 | 4.6% (20 runs) | instructions move the same way |
| corpus/parser_combinator | 2264 | 0.939 | 0.983 | 11.5% (20 runs) | instructions move the same way |
| corpus/class_middleclass | 476 | 0.958 | 0.979 | 14.1% (20 runs) | instructions move the same way |
| awfy/towers | 49 | 0.958 | 0.956 | 15.9% (20 runs) | instructions move the same way |
| lib/concat_op_str | 135 | 0.964 | 1.000 | 18.5% (20 runs) | instructions equal: layout / IPC |
| lib/tsort_int | 267 | 0.964 | 0.999 | 4.5% (5 runs) | instructions equal: layout / IPC |
| lib/it_pairs_hash | 103 | 0.966 | 1.000 | 1.0% (2 runs) | instructions equal: layout / IPC |
| corpus2/heldout/scheme_interp | 1414 | 0.966 | 0.891 | 11.4% (20 runs) | instructions move the same way |
| lib/tconcat | 143 | 0.966 | 0.998 | 11.2% (20 runs) | instructions equal: layout / IPC |
| corpus2/dev/particles_physics | 410 | 0.971 | 0.947 | 1.6% (5 runs) |  |
| apps/expr_lang | 136 | 0.975 | 0.960 | 2.8% (20 runs) |  |
| lib/load_small | 235 | 0.976 | 1.000 | 2.7% (5 runs) |  |

### 5.5 PGO vs 8c39465: 20 biggest improvements by cycles (>= 20 Mcyc)

| program | Mcyc | cyc ratio | ins ratio | cycle spread of readings | note |
|---|---|---|---|---|---|
| lib/tremove_front | 133 | 0.598 | 0.567 | 9.2% (20 runs) | instructions move the same way |
| awfy/list | 96 | 0.779 | 0.633 | 0.8% (5 runs) | instructions move the same way |
| corpus/classic_vector | 443 | 0.784 | 0.808 | 8.2% (20 runs) | instructions move the same way |
| apps/raytrace | 105 | 0.806 | 0.799 | 4.4% (20 runs) | instructions move the same way |
| corpus2/heldout/stream_closures | 740 | 0.863 | 0.836 | 0.7% (5 runs) | instructions move the same way |
| corpus/lua_lexer | 966 | 0.863 | 0.849 | 14.0% (20 runs) | instructions move the same way |
| corpus/sha2_base64 | 367 | 0.906 | 0.931 | 3.9% (20 runs) | instructions move the same way |
| lib/load_small | 156 | 0.916 | 0.996 | 3.1% (5 runs) | instructions equal: layout / IPC |
| corpus2/heldout/dijkstra_routes | 1168 | 0.941 | 0.828 | 4.2% (20 runs) | instructions move the same way |
| awfy/towers | 47 | 0.961 | 0.965 | 14.0% (20 runs) | instructions move the same way |
| lib/it_pairs_hash | 104 | 0.965 | 1.000 | 1.6% (2 runs) | instructions equal: layout / IPC |
| lib/find_pat | 227 | 0.968 | 1.000 | 4.9% (20 runs) | instructions equal: layout / IPC |
| corpus2/heldout/scheme_interp | 1208 | 0.971 | 0.896 | 14.5% (20 runs) |  |
| corpus2/dev/particles_physics | 383 | 0.973 | 0.946 | 2.0% (5 runs) |  |
| lib/error_tbl | 212 | 0.973 | 1.000 | 6.0% (20 runs) |  |
| lib/it_next | 44 | 0.974 | 1.000 | 31.8% (20 runs) |  |
| corpus/class_middleclass | 475 | 0.976 | 0.980 | 6.2% (20 runs) |  |
| apps/expr_lang | 125 | 0.977 | 0.970 | 13.2% (20 runs) |  |
| corpus2/dev/wordfreq_wrap | 501 | 0.977 | 0.945 | 16.5% (20 runs) |  |
| corpus/parser_combinator | 1863 | 0.978 | 0.982 | 6.5% (20 runs) |  |

### 5.4 plain vs 8c39465: 20 biggest improvements by cycles (>= 20 Mcyc)

| program | Mcyc | cyc ratio | ins ratio | cycle spread of readings | note |
|---|---|---|---|---|---|
| lib/tremove_front | 186 | 0.652 | 0.629 | 4.2% (20 runs) | instructions move the same way |
| apps/expr_lang | 159 | 0.666 | 0.774 | 27.5% (20 runs) | instructions move the same way |
| apps/raytrace | 116 | 0.730 | 0.656 | 6.9% (20 runs) | instructions move the same way |
| awfy/list | 100 | 0.764 | 0.610 | 0.4% (5 runs) | instructions move the same way |
| lib/gsub_fn | 178 | 0.786 | 1.000 | 5.6% (20 runs) | instructions equal: layout / IPC |
| corpus/classic_vector | 648 | 0.794 | 0.774 | 7.6% (20 runs) | instructions move the same way |
| corpus2/heldout/stream_closures | 868 | 0.841 | 0.784 | 2.5% (5 runs) | instructions move the same way |
| corpus2/dev/diff_levenshtein | 439 | 0.850 | 1.000 | 5.7% (20 runs) | instructions equal: layout / IPC |
| lib/co_yield | 223 | 0.919 | 1.000 | 15.8% (20 runs) | instructions equal: layout / IPC |
| lib/gmatch_words | 176 | 0.938 | 1.000 | 5.4% (20 runs) | instructions equal: layout / IPC |
| awfy/towers | 49 | 0.941 | 0.957 | 47.0% (20 runs) | instructions move the same way |
| corpus2/heldout/dijkstra_routes | 1462 | 0.941 | 0.893 | 4.0% (20 runs) | instructions move the same way |
| corpus2/dev/real_dis_arm | 2457 | 0.952 | 0.951 | 4.8% (20 runs) | instructions move the same way |
| real/template_engine | 749 | 0.953 | 1.000 | 4.9% (20 runs) | instructions equal: layout / IPC |
| corpus/class_middleclass | 506 | 0.963 | 0.980 | 17.3% (20 runs) | instructions move the same way |
| programs/coroutines | 1782 | 0.967 | 1.000 | 9.4% (17 runs) | instructions equal: layout / IPC |
| corpus2/heldout/scheme_interp | 1562 | 0.969 | 1.067 | 9.3% (20 runs) |  |
| lib/pcall_err | 130 | 0.970 | 1.000 | 41.8% (20 runs) | instructions equal: layout / IPC |
| corpus2/heldout/real_dis_ppc | 2221 | 0.971 | 0.999 | 4.1% (20 runs) |  |
| awfy/permute | 44 | 0.971 | 0.975 | 5.7% (20 runs) |  |

### 5.4 PGO vs 8c39465: 20 biggest improvements by cycles (>= 20 Mcyc)

| program | Mcyc | cyc ratio | ins ratio | cycle spread of readings | note |
|---|---|---|---|---|---|
| apps/expr_lang | 145 | 0.679 | 0.789 | 5.5% (20 runs) | instructions move the same way |
| lib/tremove_front | 167 | 0.726 | 0.674 | 27.6% (20 runs) | instructions move the same way |
| apps/raytrace | 108 | 0.734 | 0.652 | 5.1% (20 runs) | instructions move the same way |
| awfy/list | 97 | 0.754 | 0.606 | 1.4% (5 runs) | instructions move the same way |
| corpus/classic_vector | 556 | 0.830 | 0.817 | 12.4% (20 runs) | instructions move the same way |
| corpus2/heldout/stream_closures | 792 | 0.841 | 0.782 | 1.2% (5 runs) | instructions move the same way |
| corpus2/heldout/dijkstra_routes | 1423 | 0.944 | 0.893 | 8.9% (20 runs) | instructions move the same way |
| awfy/towers | 47 | 0.948 | 0.965 | 28.9% (20 runs) | instructions move the same way |
| corpus/csv_ini_yaml | 1594 | 0.950 | 1.000 | 4.3% (20 runs) | instructions equal: layout / IPC |
| corpus2/heldout/scheme_interp | 1288 | 0.951 | 1.063 | 13.6% (20 runs) |  |
| lib/pcall_err | 107 | 0.958 | 1.000 | 38.0% (20 runs) | instructions equal: layout / IPC |
| corpus2/dev/real_dis_arm | 2144 | 0.958 | 0.956 | 4.8% (20 runs) | instructions move the same way |
| awfy/permute | 43 | 0.963 | 0.975 | 11.3% (20 runs) | instructions move the same way |
| lib/load_small | 163 | 0.978 | 1.001 | 3.8% (5 runs) |  |
| lib/tinsert_front | 207 | 0.979 | 0.984 | 7.9% (20 runs) |  |
| programs/fannkuch | 1071 | 0.979 | 0.973 | 0.0% (2 runs) |  |
| lib/str_eq_dyn | 134 | 0.980 | 1.000 | 5.6% (20 runs) |  |
| lib/fmt_mixed | 55 | 0.980 | 0.999 | 9.1% (20 runs) |  |
| lib/tpack | 169 | 0.981 | 0.992 | 4.7% (20 runs) |  |
| lib/it_next | 44 | 0.981 | 1.000 | 10.4% (20 runs) |  |

### 5.5 plain vs 8c39465: regressions (instructions > +2% or cycles > +3%, >= 20 Mcyc)

| program | Mcyc | cyc ratio | ins ratio | cycle spread of readings | note |
|---|---|---|---|---|---|
| corpus2/dev/csv_report | 1289 | 1.128 | 0.993 | 32.5% (20 runs) | instructions equal: layout / IPC |
| corpus2/heldout/prototype_objects | 1694 | 1.121 | 0.911 | 59.0% (20 runs) |  |
| programs/table_hash | 544 | 1.081 | 1.001 | 72.6% (20 runs) | instructions equal: layout / IPC |
| real/lua_interp | 111 | 1.081 | 0.996 | 2.7% (17 runs) | instructions equal: layout / IPC |
| apps/events | 170 | 1.072 | 0.995 | 39.2% (20 runs) | instructions equal: layout / IPC |
| lib/tunpack | 124 | 1.057 | 1.000 | 6.6% (20 runs) | instructions equal: layout / IPC |
| corpus2/heldout/log_analyzer | 2211 | 1.047 | 0.998 | 7.5% (17 runs) | instructions equal: layout / IPC |
| corpus/csv_ini_yaml | 1631 | 1.046 | 0.995 | 18.0% (20 runs) | instructions equal: layout / IPC |
| real/template_engine | 749 | 1.043 | 1.000 | 3.1% (20 runs) | instructions equal: layout / IPC |
| corpus2/dev/iter_pipeline | 180 | 1.039 | 0.994 | 7.5% (20 runs) | instructions equal: layout / IPC |
| corpus2/dev/event_promise | 389 | 1.037 | 1.000 | 21.8% (20 runs) | instructions equal: layout / IPC |
| corpus/astar_jumper | 1667 | 1.033 | 1.070 | 13.5% (20 runs) | instructions move the same way |
| lib/m_min | 231 | 1.032 | 1.000 | 50.3% (20 runs) | instructions equal: layout / IPC |
| corpus/coro_scheduler | 1553 | 1.031 | 1.001 | 2.0% (2 runs) | instructions equal: layout / IPC |

### 5.5 PGO vs 8c39465: regressions (instructions > +2% or cycles > +3%, >= 20 Mcyc)

| program | Mcyc | cyc ratio | ins ratio | cycle spread of readings | note |
|---|---|---|---|---|---|
| corpus2/heldout/prototype_objects | 1607 | 1.120 | 0.911 | 45.5% (20 runs) |  |
| corpus2/dev/csv_report | 1143 | 1.100 | 0.997 | 29.5% (20 runs) | instructions equal: layout / IPC |
| programs/table_hash | 539 | 1.096 | 1.000 | 63.4% (20 runs) | instructions equal: layout / IPC |
| corpus2/dev/regex_nfa | 593 | 1.075 | 1.017 | 3.8% (20 runs) | instructions move the same way |
| real/lua_interp | 107 | 1.075 | 0.998 | 1.9% (17 runs) | instructions equal: layout / IPC |
| corpus/astar_jumper | 1607 | 1.056 | 1.075 | 29.4% (20 runs) | instructions move the same way |
| corpus2/dev/iter_pipeline | 175 | 1.056 | 0.995 | 7.2% (20 runs) | instructions equal: layout / IPC |
| corpus2/heldout/log_analyzer | 1825 | 1.038 | 0.998 | 7.2% (17 runs) | instructions equal: layout / IPC |
| apps/markdown | 105 | 1.038 | 0.997 | 0.7% (2 runs) | instructions equal: layout / IPC |
| lib/tonumber_bad | 116 | 1.036 | 0.993 | 7.3% (17 runs) | instructions equal: layout / IPC |
| lib/tpack | 117 | 1.035 | 1.023 | 12.1% (20 runs) | instructions move the same way |
| lib/concat_op_str | 124 | 1.033 | 1.017 | 10.4% (20 runs) | instructions move the same way |
| corpus2/dev/ecs_world | 863 | 1.033 | 0.995 | 3.7% (5 runs) | instructions equal: layout / IPC |
| jit-trace/inline_2 | 334 | 1.033 | 1.000 | 0.5% (2 runs) | instructions equal: layout / IPC |
| lib/fmt_x_pad | 50 | 1.031 | 1.003 | 2.5% (5 runs) | instructions equal: layout / IPC |

### 5.4 plain vs 8c39465: regressions (instructions > +2% or cycles > +3%, >= 20 Mcyc)

| program | Mcyc | cyc ratio | ins ratio | cycle spread of readings | note |
|---|---|---|---|---|---|
| corpus/astar_jumper | 1856 | 1.128 | 1.147 | 20.0% (20 runs) | instructions move the same way |
| corpus2/dev/regex_nfa | 1353 | 1.126 | 1.130 | 9.0% (20 runs) | instructions move the same way |
| corpus2/heldout/prototype_objects | 1693 | 1.111 | 0.843 | 54.6% (20 runs) |  |
| programs/table_hash | 562 | 1.092 | 1.002 | 62.4% (20 runs) | instructions equal: layout / IPC |
| corpus2/dev/csv_report | 1307 | 1.089 | 1.003 | 36.0% (20 runs) | instructions equal: layout / IPC |
| programs/load_big | 673 | 1.083 | 1.000 | 10.3% (20 runs) | instructions equal: layout / IPC |
| lib/m_min | 234 | 1.049 | 1.000 | 23.5% (20 runs) | instructions equal: layout / IPC |
| lib/tmove_1k | 108 | 1.043 | 1.002 | 21.9% (20 runs) | instructions equal: layout / IPC |
| real/lua_interp | 112 | 1.043 | 0.999 | 4.2% (17 runs) | instructions equal: layout / IPC |
| corpus/template_etlua | 1624 | 1.041 | 1.000 | 0.6% (2 runs) | instructions equal: layout / IPC |
| corpus2/dev/iter_pipeline | 191 | 1.041 | 0.995 | 8.4% (20 runs) | instructions equal: layout / IPC |
| corpus2/dev/ecs_world | 1027 | 1.038 | 0.999 | 3.6% (5 runs) | instructions equal: layout / IPC |
| programs/sieve | 622 | 1.037 | 1.000 | 1.4% (5 runs) | instructions equal: layout / IPC |
| corpus/parser_combinator | 2504 | 1.031 | 1.000 | 8.9% (20 runs) | instructions equal: layout / IPC |
| corpus/mustache_lustache | 1977 | 1.031 | 1.001 | 2.1% (5 runs) | instructions equal: layout / IPC |
| corpus2/dev/expr_compiler | 1389 | 1.030 | 1.006 | 3.7% (20 runs) | instructions equal: layout / IPC |
| corpus2/heldout/scheme_interp | 1562 | 0.969 | 1.067 | 9.3% (20 runs) |  |

### 5.4 PGO vs 8c39465: regressions (instructions > +2% or cycles > +3%, >= 20 Mcyc)

| program | Mcyc | cyc ratio | ins ratio | cycle spread of readings | note |
|---|---|---|---|---|---|
| corpus2/heldout/prototype_objects | 1652 | 1.129 | 0.857 | 53.6% (20 runs) |  |
| corpus/astar_jumper | 1784 | 1.121 | 1.155 | 8.8% (20 runs) | instructions move the same way |
| corpus2/dev/regex_nfa | 1157 | 1.115 | 1.127 | 3.7% (20 runs) | instructions move the same way |
| corpus2/dev/csv_report | 1165 | 1.089 | 1.002 | 36.0% (20 runs) | instructions equal: layout / IPC |
| programs/table_hash | 543 | 1.075 | 1.001 | 67.5% (20 runs) | instructions equal: layout / IPC |
| corpus2/dev/expr_compiler | 1197 | 1.039 | 1.008 | 4.1% (20 runs) | instructions equal: layout / IPC |
| corpus/mustache_lustache | 1590 | 1.037 | 1.000 | 2.8% (5 runs) | instructions equal: layout / IPC |
| corpus2/dev/wordfreq_wrap | 564 | 1.037 | 1.001 | 31.2% (20 runs) | instructions equal: layout / IPC |
| corpus/parser_combinator | 2049 | 1.036 | 1.000 | 3.2% (20 runs) | instructions equal: layout / IPC |
| lib/fmt_x_pad | 52 | 1.036 | 1.001 | 1.5% (5 runs) | instructions equal: layout / IPC |
| corpus2/dev/event_promise | 450 | 1.034 | 1.000 | 20.2% (20 runs) | instructions equal: layout / IPC |
| corpus/template_etlua | 1316 | 1.033 | 1.001 | 1.9% (2 runs) | instructions equal: layout / IPC |
| real/lua_interp | 107 | 1.030 | 0.999 | 5.4% (17 runs) | instructions equal: layout / IPC |
| corpus2/heldout/scheme_interp | 1288 | 0.951 | 1.063 | 13.6% (20 runs) |  |

### 5.5 plain vs 1a5aad0: 20 biggest improvements by cycles (>= 20 Mcyc)

| program | Mcyc | cyc ratio | ins ratio | cycle spread of readings | note |
|---|---|---|---|---|---|
| lib/tremove_front | 147 | 0.573 | 0.554 | 3.5% (20 runs) | instructions move the same way |
| apps/expr_lang | 136 | 0.666 | 0.736 | 2.8% (20 runs) | instructions move the same way |
| corpus/classic_vector | 480 | 0.679 | 0.639 | 10.5% (20 runs) | instructions move the same way |
| corpus2/heldout/roguelike_turns | 813 | 0.682 | 0.466 | 16.4% (20 runs) | instructions move the same way |
| awfy/list | 97 | 0.728 | 0.618 | 4.7% (5 runs) | instructions move the same way |
| apps/raytrace | 112 | 0.787 | 0.733 | 5.7% (20 runs) | instructions move the same way |
| programs/binary_trees | 1549 | 0.804 | 0.745 | 16.9% (20 runs) | instructions move the same way |
| real/game_loop | 35 | 0.806 | 0.811 | 4.9% (20 runs) | instructions move the same way |
| awfy/cd | 263 | 0.826 | 0.791 | 2.4% (5 runs) | instructions move the same way |
| corpus2/heldout/stream_closures | 798 | 0.833 | 0.802 | 2.4% (5 runs) | instructions move the same way |
| corpus/lua_lexer | 1088 | 0.874 | 0.852 | 15.8% (20 runs) | instructions move the same way |
| lib/fmt_q | 146 | 0.896 | 1.000 | 13.3% (20 runs) | instructions equal: layout / IPC |
| lib/setmt | 184 | 0.899 | 0.956 | 12.8% (20 runs) | instructions move the same way |
| corpus2/dev/oop_mixins | 1504 | 0.904 | 0.813 | 12.8% (20 runs) | instructions move the same way |
| corpus2/heldout/scheme_interp | 1414 | 0.908 | 0.846 | 11.4% (20 runs) | instructions move the same way |
| corpus2/dev/particles_physics | 410 | 0.908 | 0.852 | 1.6% (5 runs) | instructions move the same way |
| corpus/sha2_base64 | 386 | 0.914 | 0.936 | 4.6% (20 runs) | instructions move the same way |
| jit-trace/rec_loop | 81 | 0.915 | 0.873 | 1.7% (5 runs) | instructions move the same way |
| jit-trace/alloc_ctor | 188 | 0.922 | 0.931 | 8.9% (20 runs) | instructions move the same way |
| corpus/class_middleclass | 476 | 0.924 | 0.960 | 14.1% (20 runs) | instructions move the same way |

### 5.5 PGO vs 1a5aad0: 20 biggest improvements by cycles (>= 20 Mcyc)

| program | Mcyc | cyc ratio | ins ratio | cycle spread of readings | note |
|---|---|---|---|---|---|
| lib/tremove_front | 133 | 0.613 | 0.571 | 9.2% (20 runs) | instructions move the same way |
| corpus2/heldout/roguelike_turns | 774 | 0.667 | 0.459 | 7.3% (20 runs) | instructions move the same way |
| apps/expr_lang | 125 | 0.691 | 0.754 | 13.2% (20 runs) | instructions move the same way |
| corpus/classic_vector | 443 | 0.710 | 0.704 | 8.2% (20 runs) | instructions move the same way |
| awfy/list | 96 | 0.739 | 0.622 | 0.8% (5 runs) | instructions move the same way |
| real/game_loop | 33 | 0.802 | 0.811 | 6.1% (20 runs) | instructions move the same way |
| programs/binary_trees | 1318 | 0.832 | 0.762 | 17.5% (20 runs) | instructions move the same way |
| awfy/cd | 244 | 0.838 | 0.798 | 2.8% (5 runs) | instructions move the same way |
| corpus/lua_lexer | 966 | 0.851 | 0.829 | 14.0% (20 runs) | instructions move the same way |
| corpus2/heldout/stream_closures | 740 | 0.856 | 0.802 | 0.7% (5 runs) | instructions move the same way |
| corpus2/dev/oop_mixins | 1407 | 0.901 | 0.815 | 13.8% (20 runs) | instructions move the same way |
| apps/raytrace | 105 | 0.907 | 0.755 | 4.4% (20 runs) | instructions move the same way |
| corpus/sha2_base64 | 367 | 0.910 | 0.927 | 3.9% (20 runs) | instructions move the same way |
| jit-trace/rec_loop | 81 | 0.913 | 0.874 | 1.4% (5 runs) | instructions move the same way |
| corpus2/dev/particles_physics | 383 | 0.916 | 0.843 | 2.0% (5 runs) | instructions move the same way |
| awfy/deltablue | 102 | 0.928 | 0.889 | 2.8% (5 runs) | instructions move the same way |
| apps/lru_cache | 135 | 0.935 | 0.754 | 9.8% (20 runs) | instructions move the same way |
| lib/fmt_g | 62 | 0.941 | 1.000 | 7.7% (20 runs) | instructions equal: layout / IPC |
| corpus/class_middleclass | 475 | 0.941 | 0.961 | 6.2% (20 runs) | instructions move the same way |
| corpus/astar_jumper | 1607 | 0.942 | 0.862 | 29.4% (20 runs) | instructions move the same way |

### 5.4 plain vs 1a5aad0: 20 biggest improvements by cycles (>= 20 Mcyc)

| program | Mcyc | cyc ratio | ins ratio | cycle spread of readings | note |
|---|---|---|---|---|---|
| lib/tremove_front | 186 | 0.660 | 0.634 | 4.2% (20 runs) | instructions move the same way |
| corpus2/heldout/roguelike_turns | 859 | 0.663 | 0.486 | 4.9% (20 runs) | instructions move the same way |
| awfy/list | 100 | 0.713 | 0.582 | 0.4% (5 runs) | instructions move the same way |
| corpus/classic_vector | 648 | 0.731 | 0.696 | 7.6% (20 runs) | instructions move the same way |
| apps/raytrace | 116 | 0.785 | 0.701 | 6.9% (20 runs) | instructions move the same way |
| programs/binary_trees | 1752 | 0.796 | 0.779 | 9.3% (20 runs) | instructions move the same way |
| real/game_loop | 39 | 0.812 | 0.829 | 6.6% (20 runs) | instructions move the same way |
| awfy/cd | 291 | 0.819 | 0.792 | 1.1% (5 runs) | instructions move the same way |
| corpus2/heldout/stream_closures | 868 | 0.837 | 0.762 | 2.5% (5 runs) | instructions move the same way |
| corpus2/dev/oop_mixins | 1450 | 0.865 | 0.826 | 28.0% (20 runs) | instructions move the same way |
| lib/sel_loop | 214 | 0.900 | 0.998 | 8.9% (20 runs) | instructions equal: layout / IPC |
| jit-trace/rec_loop | 79 | 0.901 | 0.874 | 2.2% (5 runs) | instructions move the same way |
| lib/tunpack | 126 | 0.903 | 0.994 | 80.4% (20 runs) | instructions equal: layout / IPC |
| lib/setmt | 224 | 0.904 | 0.966 | 5.3% (20 runs) | instructions move the same way |
| apps/events | 179 | 0.906 | 0.886 | 2.9% (20 runs) | instructions move the same way |
| corpus2/dev/ecs_world | 1027 | 0.929 | 0.864 | 3.6% (5 runs) | instructions move the same way |
| corpus/class_middleclass | 506 | 0.931 | 0.964 | 17.3% (20 runs) | instructions move the same way |
| real/lua_interp | 112 | 0.934 | 0.878 | 4.2% (17 runs) | instructions move the same way |
| jit-trace/alloc_ctor | 259 | 0.934 | 0.988 | 7.4% (20 runs) | instructions move the same way |
| apps/lru_cache | 183 | 0.936 | 0.808 | 2.6% (20 runs) | instructions move the same way |

### 5.4 PGO vs 1a5aad0: 20 biggest improvements by cycles (>= 20 Mcyc)

| program | Mcyc | cyc ratio | ins ratio | cycle spread of readings | note |
|---|---|---|---|---|---|
| corpus2/heldout/roguelike_turns | 828 | 0.661 | 0.478 | 3.0% (20 runs) | instructions move the same way |
| awfy/list | 97 | 0.720 | 0.587 | 1.4% (5 runs) | instructions move the same way |
| corpus/classic_vector | 556 | 0.731 | 0.722 | 12.4% (20 runs) | instructions move the same way |
| lib/tremove_front | 167 | 0.734 | 0.681 | 27.6% (20 runs) | instructions move the same way |
| real/game_loop | 38 | 0.812 | 0.830 | 4.2% (20 runs) | instructions move the same way |
| programs/binary_trees | 1484 | 0.817 | 0.776 | 10.3% (20 runs) | instructions move the same way |
| awfy/cd | 268 | 0.825 | 0.797 | 3.9% (5 runs) | instructions move the same way |
| corpus2/heldout/stream_closures | 792 | 0.831 | 0.757 | 1.2% (5 runs) | instructions move the same way |
| lib/load_small | 163 | 0.854 | 0.996 | 3.8% (5 runs) | instructions equal: layout / IPC |
| corpus2/dev/oop_mixins | 1384 | 0.875 | 0.821 | 9.6% (20 runs) | instructions move the same way |
| apps/raytrace | 108 | 0.893 | 0.720 | 5.1% (20 runs) | instructions move the same way |
| jit-trace/rec_loop | 78 | 0.902 | 0.875 | 2.3% (5 runs) | instructions move the same way |
| lib/sel_loop | 218 | 0.908 | 0.998 | 8.6% (20 runs) | instructions equal: layout / IPC |
| corpus2/heldout/scheme_interp | 1288 | 0.915 | 1.016 | 13.6% (20 runs) |  |
| corpus2/dev/ecs_world | 877 | 0.925 | 0.853 | 1.7% (5 runs) | instructions move the same way |
| apps/lru_cache | 173 | 0.930 | 0.782 | 4.9% (20 runs) | instructions move the same way |
| apps/events | 150 | 0.930 | 0.894 | 13.3% (20 runs) | instructions move the same way |
| corpus/class_middleclass | 498 | 0.937 | 0.965 | 7.4% (20 runs) | instructions move the same way |
| real/lua_interp | 107 | 0.949 | 0.904 | 5.4% (17 runs) | instructions move the same way |
| awfy/towers | 47 | 0.950 | 0.963 | 28.9% (20 runs) | instructions move the same way |

### 5.5 plain vs 1a5aad0: regressions (instructions > +2% or cycles > +3%, >= 20 Mcyc)

| program | Mcyc | cyc ratio | ins ratio | cycle spread of readings | note |
|---|---|---|---|---|---|
| programs/table_hash | 544 | 1.151 | 1.008 | 72.6% (20 runs) | instructions equal: layout / IPC |
| corpus2/dev/csv_report | 1289 | 1.151 | 0.994 | 32.5% (20 runs) | instructions equal: layout / IPC |
| lib/tonumber_int | 155 | 1.142 | 1.013 | 0.2% (5 runs) | instructions move the same way |
| corpus2/heldout/prototype_objects | 1694 | 1.124 | 0.830 | 59.0% (20 runs) |  |
| corpus2/dev/regex_nfa | 709 | 1.084 | 1.006 | 5.1% (20 runs) | instructions equal: layout / IPC |
| programs/sieve | 590 | 1.068 | 1.000 | 4.6% (5 runs) | instructions equal: layout / IPC |
| lib/tinsert_idx | 175 | 1.066 | 1.034 | 75.5% (20 runs) | instructions move the same way |
| programs/string_ops | 1177 | 1.065 | 1.008 | 28.0% (20 runs) | instructions equal: layout / IPC |
| corpus2/dev/event_promise | 389 | 1.064 | 0.986 | 21.8% (20 runs) |  |
| corpus2/dev/iter_pipeline | 180 | 1.062 | 0.981 | 7.5% (20 runs) |  |
| lib/tostring_int | 122 | 1.055 | 1.029 | 0.5% (5 runs) | instructions move the same way |
| corpus2/dev/diff_levenshtein | 443 | 1.043 | 1.014 | 6.8% (20 runs) | instructions move the same way |
| real/template_engine | 749 | 1.042 | 0.993 | 3.1% (20 runs) | instructions equal: layout / IPC |
| lib/os_clock | 119 | 1.039 | 1.000 | 0.8% (2 runs) | instructions equal: layout / IPC |
| corpus2/dev/bignum_arith | 805 | 1.038 | 1.011 | 0.5% (2 runs) | instructions move the same way |
| lib/tmove_1k | 99 | 1.038 | 1.005 | 12.3% (20 runs) | instructions equal: layout / IPC |
| corpus/penlight_mix | 2362 | 1.034 | 0.995 | 0.1% (2 runs) | instructions equal: layout / IPC |
| corpus2/heldout/real_dis_mips | 2227 | 1.034 | 1.020 | 2.0% (5 runs) | instructions move the same way |
| corpus2/heldout/sudoku_solver | 1336 | 1.033 | 1.003 | 1.5% (2 runs) | instructions equal: layout / IPC |
| lib/pcall_err | 121 | 1.030 | 0.998 | 7.5% (20 runs) | instructions equal: layout / IPC |
| jit-trace/inline_2 | 334 | 1.030 | 1.000 | 0.6% (2 runs) | instructions equal: layout / IPC |
| lib/tinsert_front | 225 | 1.017 | 1.027 | 9.9% (20 runs) |  |
| lib/concat_op | 140 | 0.995 | 1.021 | 42.5% (20 runs) |  |
| lib/fmt_d | 93 | 0.995 | 1.023 | 15.9% (17 runs) |  |

### 5.5 PGO vs 1a5aad0: regressions (instructions > +2% or cycles > +3%, >= 20 Mcyc)

| program | Mcyc | cyc ratio | ins ratio | cycle spread of readings | note |
|---|---|---|---|---|---|
| programs/table_hash | 539 | 1.207 | 1.003 | 63.4% (20 runs) | instructions equal: layout / IPC |
| corpus2/dev/event_promise | 367 | 1.135 | 0.986 | 25.9% (20 runs) |  |
| corpus2/dev/csv_report | 1143 | 1.128 | 0.983 | 29.5% (20 runs) |  |
| corpus2/heldout/prototype_objects | 1607 | 1.113 | 0.832 | 45.5% (20 runs) |  |
| corpus2/dev/iter_pipeline | 175 | 1.074 | 0.984 | 7.2% (20 runs) |  |
| corpus2/dev/regex_nfa | 593 | 1.068 | 1.014 | 3.8% (20 runs) | instructions move the same way |
| programs/sieve | 589 | 1.059 | 1.000 | 1.2% (5 runs) | instructions equal: layout / IPC |
| corpus2/heldout/real_dis_mips | 1912 | 1.046 | 1.006 | 2.7% (5 runs) | instructions equal: layout / IPC |
| programs/string_ops | 1090 | 1.036 | 0.997 | 24.4% (20 runs) | instructions equal: layout / IPC |
| corpus2/dev/bignum_arith | 743 | 1.034 | 1.011 | 0.6% (2 runs) | instructions move the same way |
| apps/markdown | 105 | 1.034 | 0.969 | 0.7% (2 runs) |  |
| jit-trace/inline_2 | 334 | 1.032 | 1.000 | 0.5% (2 runs) | instructions equal: layout / IPC |
| awfy/storage | 78 | 1.031 | 1.009 | 8.7% (20 runs) | instructions equal: layout / IPC |
| lib/tinsert_idx | 152 | 1.027 | 1.028 | 27.5% (20 runs) |  |

### 5.4 plain vs 1a5aad0: regressions (instructions > +2% or cycles > +3%, >= 20 Mcyc)

| program | Mcyc | cyc ratio | ins ratio | cycle spread of readings | note |
|---|---|---|---|---|---|
| lib/tonumber_bad | 167 | 1.218 | 1.010 | 5.5% (17 runs) | instructions move the same way |
| corpus2/dev/csv_report | 1307 | 1.118 | 1.010 | 36.0% (20 runs) | instructions move the same way |
| corpus2/dev/regex_nfa | 1353 | 1.117 | 1.131 | 9.0% (20 runs) | instructions move the same way |
| lib/tonumber_int | 163 | 1.115 | 1.012 | 0.2% (5 runs) | instructions move the same way |
| programs/table_hash | 562 | 1.087 | 1.008 | 62.4% (20 runs) | instructions equal: layout / IPC |
| programs/load_big | 673 | 1.084 | 0.995 | 10.3% (20 runs) | instructions equal: layout / IPC |
| lib/tostring_int | 137 | 1.062 | 1.026 | 1.5% (5 runs) | instructions move the same way |
| lib/m_min | 234 | 1.057 | 1.000 | 23.5% (20 runs) | instructions equal: layout / IPC |
| corpus2/heldout/prototype_objects | 1693 | 1.052 | 0.723 | 54.6% (20 runs) |  |
| lib/tmove_1k | 108 | 1.051 | 1.003 | 21.9% (20 runs) | instructions equal: layout / IPC |
| lib/tconcat | 175 | 1.048 | 1.003 | 5.5% (20 runs) | instructions equal: layout / IPC |
| corpus2/heldout/sudoku_solver | 1679 | 1.042 | 1.010 | 0.2% (2 runs) | instructions equal: layout / IPC |
| corpus2/dev/wordfreq_wrap | 675 | 1.041 | 1.004 | 22.3% (20 runs) | instructions equal: layout / IPC |
| corpus/template_etlua | 1624 | 1.040 | 0.998 | 0.6% (2 runs) | instructions equal: layout / IPC |
| corpus/penlight_mix | 2458 | 1.039 | 0.997 | 1.7% (2 runs) | instructions equal: layout / IPC |
| apps/markdown | 123 | 1.033 | 1.007 | 1.1% (2 runs) | instructions equal: layout / IPC |
| lib/it_ipairs | 280 | 1.033 | 0.997 | 12.2% (20 runs) | instructions equal: layout / IPC |
| programs/sieve | 622 | 1.033 | 1.000 | 1.4% (5 runs) | instructions equal: layout / IPC |
| lib/m_sqrt | 281 | 1.032 | 1.000 | 2.4% (5 runs) | instructions equal: layout / IPC |
| real/json_roundtrip | 590 | 1.032 | 1.002 | 5.5% (20 runs) | instructions equal: layout / IPC |
| lib/tonumber_flt | 201 | 1.032 | 1.005 | 0.1% (2 runs) | instructions equal: layout / IPC |
| corpus2/heldout/bigint_rsa | 1400 | 1.031 | 1.027 | 4.0% (5 runs) | instructions move the same way |
| corpus2/heldout/real_dis_mips | 2227 | 1.031 | 1.026 | 0.8% (5 runs) | instructions move the same way |
| corpus2/dev/event_promise | 466 | 1.031 | 0.989 | 19.6% (20 runs) |  |
| programs/heapsort | 2274 | 1.030 | 1.001 | 11.7% (20 runs) | instructions equal: layout / IPC |
| lib/tinsert_idx | 204 | 1.030 | 1.043 | 34.2% (20 runs) | instructions move the same way |
| lib/tinsert_append | 193 | 1.012 | 1.042 | 5.1% (20 runs) |  |
| awfy/storage | 146 | 1.008 | 1.025 | 5.8% (20 runs) |  |
| lib/fmt_d | 100 | 0.991 | 1.021 | 18.2% (17 runs) |  |

### 5.4 PGO vs 1a5aad0: regressions (instructions > +2% or cycles > +3%, >= 20 Mcyc)

| program | Mcyc | cyc ratio | ins ratio | cycle spread of readings | note |
|---|---|---|---|---|---|
| corpus2/dev/regex_nfa | 1157 | 1.119 | 1.123 | 3.7% (20 runs) | instructions move the same way |
| corpus2/heldout/prototype_objects | 1652 | 1.102 | 0.737 | 53.6% (20 runs) |  |
| lib/tonumber_num | 217 | 1.101 | 0.999 | 2.7% (5 runs) | instructions equal: layout / IPC |
| corpus2/dev/csv_report | 1165 | 1.091 | 1.005 | 36.0% (20 runs) | instructions equal: layout / IPC |
| programs/string_ops | 1042 | 1.066 | 1.000 | 26.3% (20 runs) | instructions equal: layout / IPC |
| programs/table_hash | 543 | 1.062 | 1.003 | 67.5% (20 runs) | instructions equal: layout / IPC |
| corpus2/dev/event_promise | 450 | 1.057 | 0.991 | 20.2% (20 runs) | instructions equal: layout / IPC |
| corpus2/dev/wordfreq_wrap | 564 | 1.055 | 1.002 | 31.2% (20 runs) | instructions equal: layout / IPC |
| lib/tinsert_idx | 193 | 1.041 | 1.037 | 26.0% (20 runs) | instructions move the same way |
| corpus2/heldout/bigint_rsa | 1201 | 1.041 | 1.017 | 1.1% (5 runs) | instructions move the same way |
| apps/expr_lang | 145 | 1.039 | 0.953 | 5.5% (20 runs) |  |
| programs/json | 1574 | 1.035 | 1.002 | 1.4% (2 runs) | instructions equal: layout / IPC |
| lib/tinsert_append | 183 | 1.034 | 1.035 | 16.2% (20 runs) | instructions move the same way |
| lib/tsort_cmp | 261 | 1.033 | 1.004 | 5.4% (20 runs) | instructions equal: layout / IPC |
| lib/sel_n | 330 | 1.031 | 0.999 | 9.9% (20 runs) | instructions equal: layout / IPC |
| corpus2/dev/iter_pipeline | 181 | 1.031 | 0.989 | 12.3% (20 runs) |  |
| programs/matmul | 441 | 1.031 | 1.000 | 2.6% (17 runs) | instructions equal: layout / IPC |

### 5.5 plain: 15 biggest gaps to LuaJIT (ours/LJ cycles)

| program | Mcyc | cyc/LJ | ins/LJ |
|---|---|---|---|
| awfy/permute | 44 | 3.350 | 4.328 |
| apps/raytrace | 112 | 2.757 | 2.166 |
| programs/fib | 1259 | 2.680 | 3.123 |
| awfy/cd | 263 | 2.445 | 2.463 |
| corpus/lua_lexer | 1088 | 2.427 | 2.313 |
| lib/co_yield | 243 | 2.381 | 1.882 |
| lib/fmt_s | 7 | 2.336 | 2.764 |
| corpus2/dev/text_adventure | 2535 | 2.137 | 2.170 |
| awfy/towers | 49 | 2.082 | 1.759 |
| programs/coroutines | 1813 | 2.079 | 1.789 |
| lib/byte_multi | 41 | 2.043 | 3.571 |
| corpus/classic_vector | 480 | 1.994 | 1.797 |
| awfy/json | 145 | 1.977 | 2.156 |
| corpus2/heldout/stream_closures | 798 | 1.918 | 2.145 |
| lib/find_plain | 128 | 1.895 | 2.353 |

### 5.5 plain: 15 biggest wins over LuaJIT (>= 20 Mcyc)

| program | Mcyc | cyc/LJ | ins/LJ |
|---|---|---|---|
| lib/sel_loop | 213 | 0.041 | 0.067 |
| lib/error_tbl | 266 | 0.087 | 0.079 |
| jit-trace/int_rec2 | 697 | 0.139 | 1.551 |
| lib/pcall_err | 121 | 0.158 | 0.136 |
| programs/pcall_error | 1494 | 0.179 | 0.184 |
| lib/tfront_1k | 79 | 0.194 | 0.076 |
| lib/it_next | 45 | 0.247 | 0.703 |
| lib/tonumber_flt | 194 | 0.249 | 0.395 |
| corpus/sha2_base64 | 386 | 0.274 | 0.310 |
| lib/match | 62 | 0.318 | 0.316 |
| jit-trace/int_sum | 891 | 0.354 | 1.272 |
| apps/markdown | 118 | 0.358 | 0.267 |
| lib/find_pat | 263 | 0.373 | 0.451 |
| lib/gsub_str | 166 | 0.416 | 0.499 |
| programs/loops_arith | 483 | 0.436 | 1.020 |

### 5.5 PGO: 15 biggest gaps to LuaJIT (ours/LJ cycles)

| program | Mcyc | cyc/LJ | ins/LJ |
|---|---|---|---|
| awfy/permute | 43 | 3.311 | 4.316 |
| programs/fib | 1260 | 2.683 | 3.123 |
| apps/raytrace | 105 | 2.587 | 2.027 |
| awfy/cd | 244 | 2.265 | 2.305 |
| lib/fmt_s | 7 | 2.170 | 2.664 |
| corpus/lua_lexer | 966 | 2.155 | 2.164 |
| lib/byte_multi | 41 | 2.036 | 3.562 |
| awfy/towers | 47 | 2.011 | 1.727 |
| corpus/classic_vector | 443 | 1.841 | 1.702 |
| real/lua_interp | 107 | 1.806 | 2.181 |
| jit-trace/type_entry | 1531 | 1.804 | 2.396 |
| lib/getmt | 16 | 1.788 | 2.415 |
| corpus2/dev/text_adventure | 2115 | 1.782 | 1.919 |
| corpus2/heldout/stream_closures | 740 | 1.779 | 1.983 |
| lib/co_yield | 180 | 1.766 | 1.626 |

### 5.5 PGO: 15 biggest wins over LuaJIT (>= 20 Mcyc)

| program | Mcyc | cyc/LJ | ins/LJ |
|---|---|---|---|
| lib/sel_loop | 211 | 0.040 | 0.067 |
| lib/error_tbl | 212 | 0.070 | 0.062 |
| lib/pcall_err | 99 | 0.129 | 0.103 |
| jit-trace/int_rec2 | 697 | 0.139 | 1.551 |
| programs/pcall_error | 1235 | 0.148 | 0.151 |
| lib/tfront_1k | 76 | 0.187 | 0.069 |
| lib/tonumber_flt | 169 | 0.218 | 0.350 |
| lib/it_next | 44 | 0.243 | 0.702 |
| corpus/sha2_base64 | 367 | 0.260 | 0.295 |
| lib/match | 51 | 0.264 | 0.279 |
| lib/gsub_str | 127 | 0.318 | 0.394 |
| apps/markdown | 105 | 0.318 | 0.239 |
| lib/find_pat | 227 | 0.322 | 0.408 |
| jit-trace/int_sum | 893 | 0.355 | 1.272 |
| lib/tpack | 117 | 0.383 | 0.395 |

## Focus programs (cycles in M; ratios of cycles)

| program | plain Mcyc | PGO Mcyc | LJ Mcyc | PUC Mcyc | plain/LJ | PGO/LJ | plain/PUC | PGO/PUC | plain vs i cyc | ins | PGO vs i cyc | ins | plain vs 1a5 cyc | PGO vs 1a5 cyc | PGO/LJ session-i |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| programs/nbody | 347.8 | 344.3 | 345.6 | 4246.2 | 1.006 | 0.996 | 0.082 | 0.081 | 1.008 | 0.999 | 1.001 | 1.000 | 1.009 | 0.998 | 0.995 |
| programs/fib | 1258.6 | 1260.0 | 469.7 | 4139.5 | 2.680 | 2.683 | 0.304 | 0.304 | 1.001 | 1.000 | 0.976 | 1.000 | 1.003 | 1.011 | 2.748 |
| programs/binary_trees | 1549.4 | 1318.0 | 2007.4 | 4389.1 | 0.772 | 0.657 | 0.353 | 0.300 | 1.004 | 1.000 | 0.993 | 0.998 | 0.804 | 0.832 | 0.661 |
| programs/queens | 1850.8 | 1853.4 | 1907.1 | 4368.7 | 0.970 | 0.972 | 0.424 | 0.424 | 1.007 | 1.000 | 1.007 | 1.000 | 0.998 | 1.003 | 0.965 |
| apps/raytrace | 111.8 | 105.0 | 40.6 | 262.7 | 2.757 | 2.587 | 0.426 | 0.399 | 0.972 | 0.953 | 0.977 | 0.959 | 0.787 | 0.907 | 2.649 |
| awfy/cd | 263.2 | 243.8 | 107.6 | 437.5 | 2.445 | 2.265 | 0.602 | 0.557 | 1.002 | 0.957 | 0.998 | 0.964 | 0.826 | 0.838 | 2.268 |
| awfy/list | 96.9 | 96.0 | 88.9 | 418.8 | 1.090 | 1.081 | 0.231 | 0.229 | 1.003 | 0.995 | 1.017 | 0.997 | 0.728 | 0.739 | 1.063 |
| awfy/richards | 59.7 | 57.6 | 34.7 | 197.7 | 1.721 | 1.658 | 0.302 | 0.291 | 1.008 | 0.981 | 1.011 | 0.986 | 0.981 | 0.980 | 1.640 |
| awfy/deltablue | 109.6 | 101.6 | 103.5 | 157.9 | 1.060 | 0.982 | 0.694 | 0.643 | 0.989 | 0.963 | 0.985 | 0.971 | 0.935 | 0.928 | 0.997 |
| awfy/havlak | 151.0 | 135.3 | 115.2 | 217.5 | 1.311 | 1.174 | 0.694 | 0.622 | 0.988 | 0.997 | 0.997 | 0.997 | 0.979 | 1.000 | 1.177 |
| awfy/permute | 43.9 | 43.4 | 13.1 | 246.3 | 3.350 | 3.311 | 0.178 | 0.176 | 1.004 | 0.999 | 1.006 | 0.999 | 1.011 | 1.015 | 3.289 |
| programs/fannkuch | 1064.1 | 1056.5 | 1075.7 | 4075.1 | 0.989 | 0.982 | 0.261 | 0.259 | 1.004 | 1.000 | 1.008 | 1.000 | 1.010 | 0.994 | 0.974 |
| awfy/towers | 48.8 | 47.2 | 23.5 | 215.1 | 2.082 | 2.011 | 0.227 | 0.219 | 0.979 | 0.965 | 0.989 | 0.973 | 0.982 | 0.979 | 2.033 |

## Per-program table (5.5 and 5.4; Mcyc, ratios vs session-i and LJ)

| program | n runs (plain) | plain | PGO | p54 | g54 | LJ | PUC55 | PUC54 | plain/LJ | PGO/LJ | plain vs i cyc | ins | PGO vs i cyc | ins | p54 vs i cyc | ins | g54 vs i cyc | ins | out/exp (all 4) |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| apps/events | 20 | 170.3 | 141.0 | 178.7 | 150.2 | 95.4 | 293.7 | 289.5 | 1.785 | 1.478 | 1.051 | 0.999 | 1.006 | 0.999 | 1.007 | 1.000 | 0.978 | 1.000 | ok |
| apps/expr_lang | 20 | 135.8 | 124.9 | 158.8 | 145.1 | 96.7 | 241.7 | 253.2 | 1.405 | 1.292 | 0.970 | 0.966 | 0.977 | 0.976 | 0.989 | 0.969 | 0.992 | 0.975 | ok |
| apps/lru_cache | 20 | 146.7 | 135.2 | 183.1 | 173.0 | 106.4 | 219.8 | 253.9 | 1.379 | 1.271 | 1.008 | 0.958 | 0.992 | 0.969 | 0.991 | 0.963 | 0.988 | 0.968 | ok |
| apps/markdown | 2 | 118.1 | 105.0 | 123.3 | 106.3 | 329.7 | 412.6 | 390.8 | 0.358 | 0.318 | 1.003 | 0.997 | 1.022 | 0.997 | 1.030 | 1.000 | 1.017 | 1.000 | ok |
| apps/raytrace | 20 | 111.8 | 105.0 | 116.3 | 108.4 | 40.6 | 262.7 | 281.7 | 2.757 | 2.587 | 0.972 | 0.953 | 0.977 | 0.959 | 0.965 | 0.949 | 0.968 | 0.956 | ok |
| awfy/bounce | 2 | 61.5 | 59.7 | 67.5 | 65.3 | 47.7 | 511.2 | 537.1 | 1.291 | 1.253 | 0.993 | 0.995 | 0.990 | 0.996 | 0.990 | 0.996 | 0.991 | 0.997 | ok |
| awfy/cd | 5 | 263.2 | 243.8 | 291.2 | 267.8 | 107.6 | 437.5 | 464.4 | 2.445 | 2.265 | 1.002 | 0.957 | 0.998 | 0.964 | 1.002 | 0.959 | 0.992 | 0.966 | ok |
| awfy/deltablue | 5 | 109.6 | 101.6 | 120.7 | 112.0 | 103.5 | 157.9 | 165.2 | 1.060 | 0.982 | 0.989 | 0.963 | 0.985 | 0.971 | 0.996 | 0.968 | 0.985 | 0.975 | ok |
| awfy/havlak | 20 | 151.0 | 135.3 | 176.3 | 157.6 | 115.2 | 217.5 | 225.5 | 1.311 | 1.174 | 0.988 | 0.997 | 0.997 | 0.997 | 1.004 | 0.997 | 0.995 | 0.998 | ok |
| awfy/json | 5 | 145.1 | 123.0 | 157.9 | 134.5 | 73.4 | 262.9 | 271.8 | 1.977 | 1.675 | 1.002 | 0.981 | 1.003 | 0.984 | 1.018 | 0.983 | 1.009 | 0.986 | ok |
| awfy/list | 5 | 96.9 | 96.0 | 99.5 | 97.2 | 88.9 | 418.8 | 477.9 | 1.090 | 1.081 | 1.003 | 0.995 | 1.017 | 0.997 | 1.013 | 0.995 | 1.007 | 0.996 | ok |
| awfy/mandelbrot | 2 | 274.2 | 274.0 | 274.3 | 274.6 | 273.1 | 1529.3 | 1379.3 | 1.004 | 1.003 | 1.002 | 1.000 | 1.003 | 1.000 | 1.004 | 1.000 | 1.007 | 1.000 | ok |
| awfy/nbody | 5 | 93.5 | 93.7 | 97.2 | 96.2 | 94.6 | 1504.3 | 1603.8 | 0.989 | 0.990 | 1.007 | 0.998 | 1.019 | 0.999 | 0.998 | 0.998 | 1.006 | 0.999 | ok |
| awfy/permute | 20 | 43.9 | 43.4 | 43.7 | 43.1 | 13.1 | 246.3 | 257.2 | 3.350 | 3.311 | 1.004 | 0.999 | 1.006 | 0.999 | 0.996 | 0.998 | 0.991 | 0.999 | ok |
| awfy/queens | 5 | 97.7 | 94.5 | 82.3 | 80.7 | 55.0 | 401.8 | 403.4 | 1.775 | 1.718 | 1.007 | 0.999 | 1.004 | 0.999 | 1.009 | 0.999 | 1.016 | 0.999 | ok |
| awfy/richards | 20 | 59.7 | 57.6 | 59.8 | 57.3 | 34.7 | 197.7 | 199.3 | 1.721 | 1.658 | 1.008 | 0.981 | 1.011 | 0.986 | 1.005 | 0.982 | 0.995 | 0.987 | ok |
| awfy/sieve | 17 | 123.9 | 123.6 | 127.5 | 126.6 | 156.4 | 1032.3 | 1085.2 | 0.792 | 0.790 | 0.999 | 1.000 | 0.992 | 1.000 | 0.996 | 1.000 | 0.999 | 1.000 | ok |
| awfy/storage | 20 | 101.4 | 77.9 | 145.8 | 114.0 | 154.0 | 255.9 | 262.0 | 0.658 | 0.506 | 1.005 | 1.000 | 0.996 | 0.999 | 1.000 | 1.002 | 1.000 | 1.002 | ok |
| awfy/towers | 20 | 48.8 | 47.2 | 48.9 | 47.2 | 23.5 | 215.1 | 213.9 | 2.082 | 2.011 | 0.979 | 0.965 | 0.989 | 0.973 | 0.963 | 0.966 | 0.970 | 0.974 | ok |
| corpus/astar_jumper | 20 | 1666.7 | 1607.1 | 1856.1 | 1784.5 | 997.4 | 3280.6 | 3358.7 | 1.671 | 1.611 | 0.992 | 0.997 | 1.001 | 0.998 | 0.989 | 0.998 | 0.995 | 0.998 | ok |
| corpus/class_middleclass | 20 | 475.6 | 474.9 | 505.9 | 498.0 | 319.5 | 3046.3 | 3098.9 | 1.489 | 1.486 | 0.991 | 0.995 | 1.007 | 0.996 | 0.998 | 0.996 | 0.999 | 0.997 | ok |
| corpus/classic_vector | 20 | 479.9 | 443.2 | 647.8 | 556.1 | 240.7 | 2945.5 | 2905.4 | 1.994 | 1.841 | 0.746 | 0.734 | 0.773 | 0.810 | 0.803 | 0.782 | 0.826 | 0.828 | ok |
| corpus/containers_lru_heap | 5 | 1304.0 | 1214.3 | 1643.6 | 1578.3 | 1417.3 | 2675.7 | 3035.3 | 0.920 | 0.857 | 0.996 | 0.998 | 0.988 | 0.998 | 0.987 | 0.999 | 1.018 | 0.999 | ok |
| corpus/coro_scheduler | 2 | 1552.5 | 1324.2 | 1635.8 | 1430.4 | 941.9 | 2772.8 | 2851.7 | 1.648 | 1.406 | 1.020 | 1.001 | 1.004 | 1.000 | 1.001 | 1.000 | 1.007 | 1.000 | ok |
| corpus/csv_ini_yaml | 20 | 1630.7 | 1370.4 | 1902.8 | 1594.0 | 1771.4 | 2948.4 | 3020.4 | 0.921 | 0.774 | 1.033 | 0.994 | 1.000 | 0.998 | 1.021 | 1.001 | 0.973 | 1.000 | ok |
| corpus/inspect_lua | 5 | 1569.6 | 1399.7 | 1687.8 | 1512.4 | 1056.4 | 2901.3 | 2968.0 | 1.486 | 1.325 | 0.988 | 0.999 | 0.995 | 0.998 | 0.986 | 1.000 | 0.984 | 1.000 | ok |
| corpus/json_rxi | 5 | 1828.5 | 1519.0 | 1899.9 | 1598.8 | 1222.9 | 3647.9 | 3646.8 | 1.495 | 1.242 | 0.990 | 0.996 | 0.997 | 0.998 | 0.991 | 1.000 | 0.996 | 1.000 | ok |
| corpus/lua_lexer | 20 | 1088.0 | 965.8 | 1126.7 | 994.5 | 448.2 | 3281.3 | 3358.7 | 2.427 | 2.155 | 0.879 | 0.861 | 0.881 | 0.849 | 1.008 | 0.998 | 1.013 | 0.999 | ok |
| corpus/markdown_conv | 2 | 1434.0 | 1263.0 | 1433.4 | 1278.1 | 3029.8 | 3746.9 | 3671.3 | 0.473 | 0.417 | 1.004 | 0.999 | 0.994 | 1.000 | 1.002 | 1.000 | 0.995 | 1.000 | ok |
| corpus/msgpack_pure | 20 | 2047.4 | 1757.3 | 2211.9 | 1918.5 | 4473.7 | 3436.5 | 3601.3 | 0.458 | 0.393 | 1.015 | 0.984 | 0.987 | 0.983 | 1.017 | 1.000 | 1.016 | 0.999 | ok |
| corpus/mustache_lustache | 5 | 1830.2 | 1471.8 | 1976.7 | 1589.7 | 1537.1 | 3296.2 | 3446.8 | 1.191 | 0.958 | 1.008 | 0.992 | 0.991 | 0.996 | 1.011 | 1.001 | 1.018 | 1.001 | ok |
| corpus/parser_combinator | 20 | 2264.1 | 1862.5 | 2504.1 | 2048.8 | 1563.1 | 3390.6 | 3391.8 | 1.448 | 1.192 | 0.987 | 1.000 | 1.003 | 1.002 | 1.005 | 1.000 | 0.989 | 0.999 | ok |
| corpus/penlight_mix | 2 | 2361.6 | 2171.9 | 2457.7 | 2288.3 | 2678.7 | 3992.1 | 3970.3 | 0.882 | 0.811 | 1.027 | 1.000 | 1.006 | 1.001 | 1.025 | 1.000 | 1.004 | 1.000 | ok |
| corpus/serpent_ser | 5 | 2065.1 | 1790.0 | 2130.9 | 1863.0 | 2426.7 | 3260.6 | 3254.8 | 0.851 | 0.738 | 1.001 | 1.000 | 1.016 | 1.000 | 1.009 | 1.001 | 1.021 | 1.003 | ok |
| corpus/sha2_base64 | 20 | 386.1 | 367.4 | 386.6 | 367.9 | 1411.3 | 3328.6 | 3384.1 | 0.274 | 0.260 | 0.892 | 0.936 | 0.896 | 0.930 | 0.990 | 0.999 | 0.984 | 1.000 | ok |
| corpus/template_etlua | 2 | 1588.5 | 1273.8 | 1623.7 | 1315.6 | 1826.0 | 2680.1 | 2700.4 | 0.870 | 0.698 | 0.984 | 0.998 | 1.002 | 0.999 | 1.019 | 1.000 | 1.022 | 1.001 | ok |
| corpus/testrunner_luaunit | 5 | 1503.9 | 1253.3 | 1659.8 | 1368.0 | 1390.7 | 2601.9 | 2628.8 | 1.081 | 0.901 | 1.015 | 0.993 | 1.014 | 0.996 | 1.017 | 1.000 | 0.999 | 1.000 | ok |
| corpus2/dev/bignum_arith | 2 | 804.8 | 742.7 | 883.7 | 815.6 | 660.2 | 2352.8 | 2412.4 | 1.219 | 1.125 | 1.011 | 0.999 | 1.015 | 0.999 | 1.015 | 1.001 | 1.003 | 1.001 | ok |
| corpus2/dev/coro_generators | 20 | 1263.2 | 959.8 | 1276.0 | 981.4 | 713.4 | 1807.9 | 1836.1 | 1.771 | 1.345 | 1.004 | 0.999 | 0.989 | 0.998 | 0.993 | 0.999 | 0.995 | 1.000 | ok |
| corpus2/dev/csv_report | 20 | 1289.1 | 1143.3 | 1306.9 | 1164.8 | 1262.6 | 2165.3 | 2381.4 | 1.021 | 0.905 | 1.121 | 0.994 | 1.115 | 0.998 | 1.101 | 0.999 | 1.085 | 0.999 | ok |
| corpus2/dev/diff_levenshtein | 20 | 443.2 | 401.5 | 438.6 | 409.3 | 329.3 | 1776.3 | 1850.8 | 1.346 | 1.219 | 1.033 | 1.000 | 0.986 | 0.999 | 0.947 | 1.000 | 0.945 | 1.000 | ok |
| corpus2/dev/ecs_world | 5 | 963.6 | 862.8 | 1027.3 | 877.5 | 916.1 | 2115.3 | 2160.1 | 1.052 | 0.942 | 1.023 | 0.999 | 1.013 | 0.996 | 1.021 | 1.000 | 1.012 | 1.000 | ok |
| corpus2/dev/event_promise | 20 | 388.9 | 366.8 | 466.1 | 450.2 | 467.9 | 1906.6 | 1939.2 | 0.831 | 0.784 | 1.018 | 1.000 | 1.004 | 1.000 | 1.017 | 1.000 | 1.025 | 1.000 | ok |
| corpus2/dev/expr_compiler | 20 | 1317.9 | 1131.4 | 1389.2 | 1196.7 | 1080.2 | 2081.3 | 2231.0 | 1.220 | 1.047 | 1.013 | 0.999 | 1.014 | 0.999 | 1.025 | 0.999 | 1.020 | 0.999 | ok |
| corpus2/dev/iter_pipeline | 20 | 180.5 | 174.8 | 190.7 | 180.5 | 132.2 | 2865.2 | 2874.5 | 1.365 | 1.323 | 1.041 | 0.994 | 1.063 | 0.995 | 1.038 | 0.996 | 1.035 | 0.997 | ok |
| corpus2/dev/lua_parser | 20 | 1292.1 | 1134.1 | 1588.7 | 1353.7 | 790.7 | 2461.5 | 2499.0 | 1.634 | 1.434 | 1.014 | 0.994 | 1.027 | 0.995 | 1.038 | 0.994 | 1.020 | 0.995 | ok |
| corpus2/dev/memo_cache | 20 | 1704.7 | 1556.6 | 1695.4 | 1597.8 | 1674.7 | 2669.5 | 2763.8 | 1.018 | 0.929 | 1.025 | 0.993 | 1.036 | 0.998 | 1.011 | 0.999 | 1.051 | 0.998 | ok |
| corpus2/dev/oop_mixins | 20 | 1504.2 | 1406.8 | 1450.4 | 1384.1 | 865.1 | 2464.3 | 2520.8 | 1.739 | 1.626 | 1.055 | 0.996 | 1.042 | 0.998 | 1.029 | 1.000 | 1.033 | 1.000 | ok |
| corpus2/dev/particles_physics | 5 | 409.9 | 382.8 | 519.3 | 484.4 | 379.7 | 1719.5 | 1811.4 | 1.079 | 1.008 | 1.023 | 0.988 | 1.020 | 0.988 | 1.007 | 0.985 | 1.013 | 0.986 | ok |
| corpus2/dev/real_dis_arm | 20 | 2468.8 | 2196.9 | 2456.7 | 2143.6 | 1947.2 | 4487.3 | 4484.7 | 1.268 | 1.128 | 1.002 | 0.982 | 1.015 | 0.980 | 1.016 | 1.000 | 1.002 | 1.000 | ok |
| corpus2/dev/real_dis_x86 | 20 | 1550.6 | 1368.5 | 1619.8 | 1425.0 | 1591.6 | 2708.6 | 2688.6 | 0.974 | 0.860 | 0.999 | 0.985 | 1.000 | 0.983 | 1.021 | 1.000 | 1.008 | 1.000 | ok |
| corpus2/dev/regex_nfa | 20 | 708.8 | 592.9 | 1352.7 | 1156.8 | 554.5 | 2510.1 | 2439.3 | 1.278 | 1.069 | 1.091 | 0.999 | 1.049 | 0.999 | 1.010 | 1.000 | 1.003 | 1.000 | ok |
| corpus2/dev/template_gsub | 20 | 1719.2 | 1409.2 | 1777.3 | 1480.6 | 2051.0 | 2892.7 | 2885.0 | 0.838 | 0.687 | 1.018 | 1.000 | 1.003 | 1.000 | 1.010 | 1.000 | 0.993 | 1.000 | ok |
| corpus2/dev/text_adventure | 20 | 2535.1 | 2114.9 | 2689.0 | 2217.9 | 1186.6 | 3375.1 | 3513.0 | 2.137 | 1.782 | 0.977 | 1.000 | 0.985 | 1.001 | 0.977 | 1.000 | 1.016 | 1.000 | ok |
| corpus2/dev/varargs_util | 2 | 1551.5 | 1240.5 | 1539.6 | 1258.6 | 1413.9 | 2284.8 | 2301.9 | 1.097 | 0.877 | 1.026 | 1.000 | 1.000 | 1.000 | 0.989 | 0.999 | 0.998 | 1.000 | ok |
| corpus2/dev/wordfreq_wrap | 20 | 615.8 | 500.9 | 674.9 | 564.1 | 658.1 | 1219.2 | 1225.4 | 0.936 | 0.761 | 0.995 | 0.954 | 0.970 | 0.945 | 1.015 | 1.000 | 1.021 | 1.001 | ok |
| corpus2/heldout/bigint_rsa | 5 | 1105.4 | 944.9 | 1399.7 | 1201.2 | 1074.5 | 3033.1 | 3149.9 | 1.029 | 0.879 | 0.979 | 1.000 | 1.010 | 1.000 | 1.020 | 1.001 | 1.011 | 1.002 | ok |
| corpus2/heldout/dijkstra_routes | 20 | 1192.0 | 1167.6 | 1462.0 | 1423.1 | 1265.1 | 3142.9 | 3226.1 | 0.942 | 0.923 | 0.823 | 0.641 | 0.825 | 0.639 | 0.958 | 0.935 | 0.957 | 0.934 | ok |
| corpus2/heldout/forth_vm | 2 | 805.0 | 802.6 | 1051.2 | 1156.5 | 696.1 | 2965.2 | 3269.3 | 1.156 | 1.153 | 0.993 | 0.998 | 0.995 | 0.999 | 0.993 | 0.999 | 1.000 | 0.999 | ok |
| corpus2/heldout/log_analyzer | 17 | 2210.8 | 1824.8 | 2176.9 | 1859.2 | 1652.2 | 3115.0 | 3178.7 | 1.338 | 1.104 | 1.027 | 0.998 | 1.035 | 0.998 | 1.013 | 1.001 | 0.981 | 1.000 | ok |
| corpus2/heldout/markov_text | 20 | 1123.2 | 1105.2 | 1190.3 | 1158.8 | 982.0 | 2862.9 | 2851.2 | 1.144 | 1.126 | 0.984 | 0.994 | 0.972 | 0.993 | 0.986 | 0.999 | 0.999 | 0.999 | ok |
| corpus2/heldout/prototype_objects | 20 | 1694.0 | 1606.9 | 1692.7 | 1652.2 | 1280.6 | 2532.8 | 2721.4 | 1.323 | 1.255 | 1.164 | 0.996 | 1.145 | 0.997 | 1.168 | 0.995 | 1.192 | 0.997 | ok |
| corpus2/heldout/real_dis_mips | 5 | 2227.0 | 1912.0 | 2227.1 | 1895.8 | 1786.6 | 3808.7 | 3869.0 | 1.246 | 1.070 | 0.997 | 0.990 | 1.003 | 0.988 | 1.002 | 0.999 | 0.997 | 0.999 | ok |
| corpus2/heldout/real_dis_ppc | 20 | 2122.8 | 1817.4 | 2221.5 | 1901.0 | 1747.6 | 3961.9 | 3962.8 | 1.215 | 1.040 | 0.988 | 0.979 | 0.983 | 0.976 | 0.986 | 0.999 | 0.989 | 1.000 | ok |
| corpus2/heldout/roguelike_turns | 20 | 813.1 | 773.5 | 859.5 | 828.2 | 623.3 | 3311.7 | 3473.6 | 1.305 | 1.241 | 0.976 | 0.989 | 0.978 | 0.992 | 0.985 | 0.997 | 0.988 | 0.998 | ok |
| corpus2/heldout/scheme_interp | 20 | 1414.1 | 1208.3 | 1561.9 | 1288.5 | 1045.1 | 3052.1 | 3021.0 | 1.353 | 1.156 | 0.978 | 0.988 | 0.990 | 0.995 | 1.008 | 1.000 | 0.985 | 1.000 | ok |
| corpus2/heldout/stream_closures | 5 | 797.8 | 740.0 | 867.8 | 792.4 | 416.0 | 3032.0 | 3163.1 | 1.918 | 1.779 | 0.981 | 0.980 | 0.993 | 0.982 | 0.996 | 0.985 | 0.978 | 0.987 | ok |
| corpus2/heldout/sudoku_solver | 2 | 1336.4 | 1193.6 | 1679.3 | 1506.0 | 1387.6 | 3013.0 | 3102.7 | 0.963 | 0.860 | 1.021 | 1.000 | 1.015 | 1.000 | 1.023 | 1.001 | 0.997 | 1.001 | ok |
| corpus2/heldout/vending_fsm | 2 | 1022.1 | 941.9 | 1105.0 | 1021.2 | 824.2 | 3290.3 | 3377.2 | 1.240 | 1.143 | 1.000 | 0.996 | 1.006 | 0.997 | 1.001 | 0.998 | 0.991 | 0.999 | ok |
| jit-trace/alloc_ctor | 20 | 188.1 | 158.5 | 259.0 | 201.8 | 276.5 | 1652.1 | 2197.8 | 0.680 | 0.573 | 0.989 | 0.999 | 0.987 | 0.996 | 1.006 | 1.000 | 0.992 | 0.999 | ok |
| jit-trace/array_rec | 2 | 1796.6 | 1796.0 | 1822.0 | 1821.5 | 1755.9 | 9671.5 | 10022.8 | 1.023 | 1.023 | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | ok |
| jit-trace/dot | 2 | 1218.8 | 1219.2 | 1224.7 | 1227.7 | 1166.0 | 11124.9 | 11807.8 | 1.045 | 1.046 | 0.994 | 1.000 | 0.995 | 1.000 | 0.995 | 1.000 | 0.995 | 1.000 | ok |
| jit-trace/field_global | 2 | 546.4 | 539.7 | 547.9 | 539.9 | 491.7 | 17948.4 | 17879.3 | 1.111 | 1.098 | 1.001 | 1.000 | 0.998 | 1.000 | 1.003 | 1.000 | 0.996 | 1.000 | ok |
| jit-trace/field_method | 2 | 569.5 | 565.2 | 571.9 | 569.2 | 423.1 | 20185.1 | 20271.4 | 1.346 | 1.336 | 0.997 | 1.000 | 1.000 | 0.999 | 1.001 | 1.000 | 1.003 | 1.000 | ok |
| jit-trace/field_rw | 2 | 950.1 | 949.1 | 1038.7 | 1035.4 | 929.3 | 19574.6 | 19882.6 | 1.022 | 1.021 | 1.000 | 1.000 | 1.000 | 1.000 | 1.002 | 1.000 | 1.001 | 0.999 | ok |
| jit-trace/float_loop | 2 | 2781.4 | 2778.1 | 2798.3 | 2793.6 | 2575.9 | 11381.5 | 13567.0 | 1.080 | 1.079 | 1.000 | 1.000 | 1.000 | 1.000 | 1.002 | 1.000 | 0.999 | 1.000 | ok |
| jit-trace/float_poly | 2 | 1785.8 | 1784.3 | 1820.0 | 1819.9 | 1768.0 | 7918.6 | 8940.5 | 1.010 | 1.009 | 0.999 | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | 0.999 | 1.000 | ok |
| jit-trace/inline_2 | 2 | 333.6 | 333.8 | 333.1 | 332.6 | 262.5 | 11057.3 | 10600.9 | 1.271 | 1.272 | 1.002 | 1.000 | 1.004 | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | ok |
| jit-trace/inline_leaf | 2 | 461.4 | 456.6 | 460.5 | 457.5 | 630.2 | 11902.9 | 12019.3 | 0.732 | 0.725 | 1.009 | 1.000 | 1.005 | 1.000 | 1.012 | 1.000 | 1.001 | 1.000 | ok |
| jit-trace/int_rec2 | 2 | 696.9 | 696.9 | 702.6 | 706.1 | 5016.4 | 12493.6 | 13006.3 | 0.139 | 0.139 | 1.019 | 1.000 | 1.024 | 1.000 | 1.028 | 1.000 | 1.022 | 1.000 | ok |
| jit-trace/int_sum | 2 | 890.7 | 893.2 | 916.4 | 914.8 | 2512.8 | 9858.5 | 12300.9 | 0.354 | 0.355 | 1.002 | 1.000 | 1.005 | 1.000 | 1.006 | 1.000 | 1.004 | 1.000 | ok |
| jit-trace/licm_expr | 2 | 471.1 | 470.5 | 468.8 | 469.3 | 442.6 | 12323.9 | 12827.3 | 1.064 | 1.063 | 1.010 | 1.000 | 1.007 | 1.000 | 1.005 | 1.000 | 1.007 | 1.000 | ok |
| jit-trace/multret | 2 | 151.1 | 151.1 | 151.2 | 150.6 | 156.7 | 5879.7 | 5782.8 | 0.964 | 0.964 | 1.002 | 1.000 | 1.007 | 1.000 | 1.004 | 1.000 | 1.003 | 1.000 | ok |
| jit-trace/oop_inherit | 2 | 57.9 | 57.7 | 49.3 | 48.6 | 112.8 | 2153.1 | 2201.0 | 0.513 | 0.511 | 1.003 | 0.999 | 1.007 | 0.999 | 1.005 | 0.999 | 0.998 | 0.999 | ok |
| jit-trace/oop_override | 20 | 106.7 | 106.7 | 110.7 | 110.2 | 136.7 | 645.6 | 666.3 | 0.781 | 0.781 | 0.999 | 0.999 | 1.001 | 1.000 | 1.001 | 1.000 | 1.002 | 1.000 | ok |
| jit-trace/rec_loop | 5 | 81.5 | 80.6 | 79.3 | 78.3 | 141.9 | 1440.3 | 1449.9 | 0.574 | 0.568 | 0.994 | 0.995 | 0.994 | 0.996 | 0.986 | 0.995 | 0.988 | 0.996 | ok |
| jit-trace/short_inner | 2 | 1105.2 | 1105.9 | 999.5 | 997.8 | 878.8 | 10393.4 | 11908.6 | 1.258 | 1.258 | 1.004 | 1.000 | 1.004 | 1.000 | 0.999 | 1.000 | 0.999 | 1.000 | ok |
| jit-trace/sink_tuple | 2 | 13.5 | 13.1 | 13.4 | 13.1 | 20.0 | 2743.7 | 3358.1 | 0.676 | 0.656 | 1.017 | 0.999 | 1.009 | 0.999 | 1.010 | 0.999 | 1.010 | 0.999 | ok |
| jit-trace/sink_vec | 5 | 15.4 | 15.2 | 15.4 | 15.0 | 20.0 | 1489.0 | 1832.3 | 0.770 | 0.762 | 1.004 | 1.000 | 0.995 | 1.000 | 1.000 | 1.000 | 0.995 | 1.000 | ok |
| jit-trace/str_concat | 2 | 531.6 | 517.3 | 606.4 | 564.1 | 501.6 | 1628.5 | 1630.2 | 1.060 | 1.031 | 0.988 | 1.000 | 1.010 | 0.996 | 1.015 | 1.000 | 1.017 | 1.000 | ok |
| jit-trace/tail_loop | 2 | 250.9 | 251.2 | 253.4 | 251.9 | 223.3 | 6724.0 | 6643.8 | 1.123 | 1.125 | 1.011 | 1.000 | 1.010 | 1.000 | 1.002 | 1.000 | 1.002 | 1.000 | ok |
| jit-trace/tfor_loops | 20 | 219.1 | 218.7 | 228.0 | 226.0 | 362.3 | 3623.0 | 3352.6 | 0.605 | 0.604 | 0.995 | 1.000 | 0.998 | 1.000 | 1.002 | 1.000 | 1.008 | 1.000 | ok |
| jit-trace/type_entry | 2 | 1537.7 | 1530.6 | 1588.6 | 1572.5 | 848.7 | 11725.8 | 12234.3 | 1.812 | 1.804 | 1.017 | 1.000 | 1.010 | 1.000 | 1.007 | 1.000 | 0.992 | 1.000 | ok |
| jit-trace/type_entry_stable | 5 | 1452.3 | 1451.9 | 1517.0 | 1514.4 | 856.9 | 11690.0 | 12248.4 | 1.695 | 1.694 | 0.993 | 1.000 | 0.991 | 1.000 | 0.991 | 1.000 | 0.988 | 1.000 | ok |
| jit-trace/type_late | 2 | 1521.7 | 1521.4 | 1529.8 | 1532.7 | 1461.5 | 10901.2 | 11418.6 | 1.041 | 1.041 | 0.996 | 1.000 | 0.995 | 1.000 | 0.993 | 1.000 | 0.995 | 1.000 | ok |
| jit-trace/type_late_stable | 2 | 1522.4 | 1521.7 | 1534.9 | 1539.9 | 1461.0 | 10905.6 | 11410.8 | 1.042 | 1.042 | 0.995 | 1.000 | 0.994 | 1.000 | 1.002 | 1.000 | 1.005 | 1.000 | ok |
| jit-trace/upval_acc | 2 | 1383.7 | 1396.2 | 1401.3 | 1405.8 | 1356.5 | 9970.0 | 10500.8 | 1.020 | 1.029 | 0.994 | 1.000 | 1.002 | 1.000 | 1.002 | 1.000 | 1.005 | 1.000 | ok |
| jit-trace/upval_inv | 5 | 2135.9 | 2135.0 | 2142.6 | 2142.9 | 1985.3 | 13886.5 | 14901.3 | 1.076 | 1.075 | 1.000 | 1.000 | 0.996 | 1.000 | 0.995 | 1.000 | 1.001 | 1.000 | ok |
| lib/assert_ok | 20 | 19.0 | 18.6 | 20.2 | 19.8 | 23.7 | 295.9 | 329.8 | 0.800 | 0.785 | 0.985 | 1.000 | 0.996 | 1.000 | 0.990 | 1.000 | 0.981 | 1.000 | ok |
| lib/byte | 5 | 50.6 | 50.2 | 51.2 | 50.8 | 33.0 | 776.0 | 845.1 | 1.531 | 1.521 | 1.006 | 1.000 | 1.003 | 1.000 | 1.000 | 1.000 | 0.993 | 1.000 | ok |
| lib/byte_multi | 5 | 41.0 | 40.9 | 41.5 | 40.8 | 20.1 | 336.9 | 366.2 | 2.043 | 2.036 | 0.990 | 1.000 | 0.998 | 1.000 | 0.996 | 1.000 | 0.995 | 1.000 | ok |
| lib/char | 20 | 153.2 | 138.7 | 155.7 | 143.9 | 104.8 | 383.6 | 387.1 | 1.462 | 1.324 | 0.994 | 1.000 | 0.994 | 1.000 | 0.996 | 1.000 | 0.991 | 1.000 | ok |
| lib/co_wrap | 20 | 246.2 | 200.1 | 236.9 | 199.7 | 228.7 | 381.5 | 338.9 | 1.076 | 0.875 | 0.988 | 0.999 | 0.986 | 0.998 | 0.987 | 1.000 | 0.986 | 1.000 | ok |
| lib/co_yield | 20 | 242.8 | 180.1 | 223.1 | 182.3 | 102.0 | 483.9 | 475.0 | 2.381 | 1.766 | 1.077 | 1.000 | 1.004 | 0.998 | 0.991 | 1.000 | 1.013 | 1.000 | ok |
| lib/concat_op | 20 | 140.3 | 129.0 | 144.3 | 135.3 | 212.9 | 419.4 | 445.3 | 0.659 | 0.606 | 1.007 | 1.000 | 1.005 | 0.997 | 1.010 | 1.000 | 1.009 | 1.000 | ok |
| lib/concat_op_str | 20 | 135.3 | 124.3 | 239.0 | 229.5 | 276.8 | 436.1 | 336.3 | 0.489 | 0.449 | 1.003 | 1.000 | 1.054 | 1.017 | 1.000 | 1.000 | 1.012 | 1.000 | ok |
| lib/error_tbl | 20 | 266.0 | 211.7 | 261.3 | 215.2 | 3042.9 | 619.9 | 625.1 | 0.087 | 0.070 | 1.017 | 1.000 | 1.027 | 0.999 | 1.001 | 1.000 | 0.988 | 1.000 | ok |
| lib/find_cap | 2 | 276.3 | 231.7 | 277.8 | 231.6 | 183.4 | 365.9 | 363.4 | 1.506 | 1.263 | 1.013 | 1.000 | 0.981 | 1.000 | 1.004 | 1.000 | 1.024 | 1.000 | ok |
| lib/find_pat | 20 | 263.4 | 227.0 | 287.2 | 237.2 | 705.2 | 1094.0 | 1092.2 | 0.373 | 0.322 | 0.970 | 1.000 | 0.959 | 1.000 | 1.056 | 1.000 | 1.013 | 1.000 | ok |
| lib/find_plain | 20 | 128.0 | 115.9 | 126.5 | 115.0 | 67.6 | 342.8 | 363.4 | 1.895 | 1.716 | 0.994 | 1.000 | 0.993 | 1.000 | 0.989 | 1.000 | 0.990 | 1.000 | ok |
| lib/fmt_d | 17 | 93.1 | 89.0 | 100.1 | 95.3 | 126.3 | 278.1 | 235.1 | 0.737 | 0.705 | 1.002 | 1.000 | 1.008 | 1.004 | 0.999 | 1.000 | 1.006 | 1.000 | ok |
| lib/fmt_f | 20 | 63.0 | 57.5 | 64.9 | 59.2 | 86.8 | 215.0 | 196.2 | 0.726 | 0.663 | 1.011 | 1.000 | 1.001 | 0.999 | 0.984 | 1.000 | 1.004 | 1.000 | ok |
| lib/fmt_g | 20 | 73.0 | 62.3 | 75.4 | 64.1 | 67.3 | 204.6 | 179.3 | 1.084 | 0.926 | 1.005 | 0.999 | 1.002 | 0.999 | 1.000 | 0.999 | 1.005 | 1.000 | ok |
| lib/fmt_mixed | 20 | 59.6 | 49.1 | 66.8 | 55.4 | 106.2 | 254.5 | 238.3 | 0.562 | 0.462 | 1.015 | 1.000 | 1.001 | 1.000 | 0.999 | 1.000 | 0.997 | 1.000 | ok |
| lib/fmt_q | 20 | 146.3 | 120.1 | 145.0 | 119.5 | 103.3 | 238.6 | 226.1 | 1.417 | 1.164 | 1.004 | 1.000 | 0.992 | 1.000 | 1.006 | 1.000 | 1.003 | 1.000 | ok |
| lib/fmt_s | 20 | 7.4 | 6.9 | 7.4 | 6.9 | 3.2 | 217.6 | 222.2 | 2.336 | 2.170 | 1.032 | 0.998 | 0.984 | 0.999 | 0.979 | 0.998 | 0.981 | 0.999 | ok |
| lib/fmt_x_pad | 5 | 57.8 | 49.6 | 60.3 | 51.9 | 54.0 | 177.1 | 177.5 | 1.070 | 0.918 | 1.016 | 1.000 | 1.022 | 1.003 | 1.012 | 1.000 | 1.010 | 1.000 | ok |
| lib/getmt | 5 | 16.3 | 15.8 | 20.0 | 19.6 | 8.8 | 255.7 | 264.8 | 1.848 | 1.788 | 1.013 | 0.999 | 0.999 | 0.999 | 1.006 | 0.999 | 0.989 | 0.999 | ok |
| lib/gmatch_words | 20 | 174.6 | 136.2 | 175.9 | 136.5 | 173.3 | 315.9 | 319.8 | 1.008 | 0.786 | 0.980 | 1.000 | 0.984 | 1.000 | 0.980 | 1.000 | 0.988 | 1.000 | ok |
| lib/gsub_fn | 20 | 174.4 | 129.0 | 178.2 | 131.0 | 200.2 | 323.4 | 312.5 | 0.871 | 0.644 | 0.982 | 1.000 | 0.997 | 0.999 | 1.001 | 1.000 | 1.003 | 1.000 | ok |
| lib/gsub_str | 20 | 165.9 | 126.9 | 165.6 | 126.5 | 398.5 | 491.6 | 515.6 | 0.416 | 0.318 | 1.012 | 1.000 | 0.991 | 1.000 | 0.992 | 1.000 | 0.997 | 1.000 | ok |
| lib/gsub_tbl | 5 | 166.2 | 106.7 | 155.8 | 111.8 | 187.9 | 321.9 | 321.7 | 0.885 | 0.568 | 1.007 | 1.000 | 1.004 | 1.000 | 1.021 | 1.000 | 1.014 | 1.000 | ok |
| lib/it_ipairs | 20 | 238.8 | 233.9 | 279.7 | 256.8 | 251.6 | 5628.0 | 4827.6 | 0.949 | 0.930 | 1.013 | 1.000 | 1.007 | 1.001 | 1.008 | 1.000 | 0.993 | 1.000 | ok |
| lib/it_next | 20 | 44.6 | 43.8 | 44.6 | 43.9 | 180.5 | 371.1 | 401.4 | 0.247 | 0.243 | 0.995 | 1.000 | 0.978 | 1.000 | 0.995 | 1.000 | 0.989 | 1.000 | ok |
| lib/it_next_empty | 20 | 51.9 | 51.6 | 51.7 | 51.6 | 72.5 | 393.0 | 455.4 | 0.715 | 0.712 | 1.005 | 1.000 | 1.003 | 1.000 | 0.998 | 1.000 | 1.003 | 1.000 | ok |
| lib/it_pairs | 20 | 87.8 | 88.5 | 95.3 | 91.2 | 127.9 | 687.0 | 646.3 | 0.686 | 0.692 | 0.998 | 1.000 | 1.019 | 1.000 | 1.020 | 1.000 | 0.990 | 1.000 | ok |
| lib/it_pairs_hash | 2 | 103.0 | 104.4 | 115.0 | 110.2 | 118.1 | 989.3 | 957.1 | 0.872 | 0.884 | 0.995 | 1.000 | 1.001 | 1.000 | 1.028 | 1.000 | 1.006 | 1.000 | ok |
| lib/len_op | 20 | 48.1 | 47.5 | 57.7 | 57.6 | 100.7 | 1384.7 | 1419.8 | 0.478 | 0.472 | 1.010 | 1.000 | 1.000 | 1.000 | 1.002 | 1.000 | 1.005 | 1.000 | ok |
| lib/len_tbl | 2 | 138.4 | 137.0 | 157.7 | 157.5 | 159.7 | 372.6 | 337.9 | 0.867 | 0.858 | 0.985 | 1.000 | 0.966 | 1.000 | 0.990 | 1.000 | 0.994 | 1.000 | ok |
| lib/load_small | 5 | 234.9 | 156.3 | 219.8 | 162.7 | 215.6 | 378.8 | 345.2 | 1.089 | 0.725 | 1.029 | 1.000 | 1.003 | 0.999 | 0.967 | 1.000 | 0.971 | 1.000 | ok |
| lib/lower | 5 | 89.0 | 80.3 | 89.9 | 83.6 | 110.1 | 289.0 | 294.8 | 0.809 | 0.729 | 0.991 | 1.000 | 0.990 | 1.000 | 0.986 | 1.000 | 1.008 | 1.000 | ok |
| lib/m_abs | 5 | 246.5 | 245.5 | 245.4 | 246.8 | 390.3 | 6061.9 | 6506.8 | 0.632 | 0.629 | 1.021 | 1.000 | 1.021 | 1.000 | 1.016 | 1.000 | 1.013 | 1.000 | ok |
| lib/m_floor | 2 | 226.9 | 226.4 | 227.1 | 226.9 | 206.5 | 3681.1 | 4104.1 | 1.099 | 1.096 | 1.000 | 1.000 | 0.999 | 1.000 | 0.997 | 1.000 | 0.998 | 1.000 | ok |
| lib/m_fmod | 20 | 218.1 | 218.5 | 222.4 | 227.2 | 235.1 | 329.2 | 361.2 | 0.928 | 0.930 | 0.989 | 1.000 | 0.983 | 1.000 | 0.994 | 1.000 | 0.997 | 1.000 | ok |
| lib/m_max | 20 | 255.9 | 255.3 | 256.5 | 256.2 | 401.3 | 8004.8 | 10137.5 | 0.638 | 0.636 | 0.992 | 1.000 | 0.991 | 1.000 | 0.988 | 1.000 | 0.987 | 1.000 | ok |
| lib/m_min | 20 | 230.8 | 222.7 | 234.1 | 227.6 | 387.7 | 6530.7 | 8066.5 | 0.595 | 0.574 | 1.018 | 1.000 | 0.987 | 1.000 | 0.940 | 1.000 | 1.026 | 1.000 | ok |
| lib/m_pow_op | 2 | 280.6 | 280.4 | 281.5 | 280.1 | 275.2 | 1923.5 | 1956.0 | 1.020 | 1.019 | 1.001 | 1.000 | 1.001 | 1.000 | 1.004 | 1.000 | 1.000 | 1.000 | ok |
| lib/m_random | 5 | 79.1 | 78.7 | 79.0 | 78.6 | 73.2 | 345.7 | 375.3 | 1.080 | 1.075 | 0.998 | 1.000 | 1.000 | 1.000 | 0.995 | 1.000 | 0.983 | 1.000 | ok |
| lib/m_random_f | 2 | 53.7 | 53.4 | 53.9 | 53.8 | 89.9 | 272.7 | 312.0 | 0.597 | 0.594 | 1.001 | 1.000 | 1.004 | 1.000 | 1.009 | 1.000 | 1.015 | 1.000 | ok |
| lib/m_sin | 5 | 272.3 | 261.1 | 272.8 | 261.2 | 210.5 | 563.9 | 589.3 | 1.293 | 1.240 | 0.996 | 1.000 | 0.996 | 1.000 | 0.996 | 1.000 | 0.998 | 1.000 | ok |
| lib/m_sqrt | 5 | 279.0 | 279.1 | 281.5 | 278.7 | 270.0 | 2022.6 | 2132.3 | 1.034 | 1.034 | 1.011 | 1.000 | 1.015 | 1.000 | 1.000 | 1.000 | 1.002 | 1.000 | ok |
| lib/m_tointeger | 20 | 223.6 | 222.6 | 236.5 | 236.5 | - | 3718.4 | 3972.3 | - | - | 0.983 | 1.000 | 0.975 | 1.000 | 1.058 | 1.000 | 1.001 | 1.000 | ok |
| lib/m_type | 20 | 31.9 | 31.5 | 31.9 | 31.6 | - | 419.3 | 429.1 | - | - | 0.977 | 1.000 | 0.981 | 1.000 | 0.981 | 0.999 | 0.982 | 1.000 | ok |
| lib/m_ult | 20 | 274.8 | 282.1 | 274.4 | 285.3 | - | 415.2 | 435.8 | - | - | 1.010 | 1.000 | 1.006 | 1.000 | 0.995 | 1.000 | 1.015 | 1.000 | ok |
| lib/match | 20 | 61.7 | 51.0 | 61.4 | 51.6 | 193.6 | 319.9 | 303.8 | 0.318 | 0.264 | 0.980 | 1.000 | 0.983 | 1.000 | 0.995 | 1.000 | 0.980 | 1.000 | ok |
| lib/os_clock | 2 | 118.6 | 121.6 | 120.3 | 121.6 | 100.7 | 119.3 | 118.5 | 1.178 | 1.208 | 1.022 | 1.000 | 1.008 | 1.000 | 1.011 | 1.000 | 1.002 | 1.000 | ok |
| lib/pcall_err | 20 | 120.6 | 98.6 | 129.8 | 107.4 | 764.5 | 308.4 | 309.0 | 0.158 | 0.129 | 0.993 | 1.000 | 1.010 | 0.999 | 1.010 | 1.000 | 0.955 | 1.000 | ok |
| lib/pcall_ok | 2 | 30.2 | 30.0 | 30.4 | 30.1 | 41.0 | 1524.0 | 1518.3 | 0.738 | 0.731 | 0.996 | 1.000 | 0.996 | 1.000 | 0.996 | 1.000 | 0.998 | 1.000 | ok |
| lib/rawget_set | 2 | 19.2 | 18.7 | 19.2 | 19.0 | 14.2 | 381.1 | 417.2 | 1.351 | 1.313 | 1.012 | 0.999 | 0.993 | 0.999 | 0.990 | 0.998 | 0.998 | 0.999 | ok |
| lib/rep | 2 | 107.7 | 100.6 | 120.1 | 110.3 | 89.6 | 347.4 | 333.3 | 1.203 | 1.123 | 0.993 | 1.000 | 1.000 | 1.000 | 0.983 | 1.000 | 0.985 | 1.000 | ok |
| lib/rep_sep | 2 | 89.6 | 81.7 | 117.9 | 109.1 | 103.8 | 329.6 | 311.2 | 0.864 | 0.788 | 1.003 | 1.000 | 1.005 | 1.000 | 0.994 | 1.000 | 0.998 | 1.000 | ok |
| lib/reverse | 2 | 98.8 | 93.3 | 106.7 | 99.5 | 115.3 | 309.8 | 314.7 | 0.857 | 0.809 | 0.985 | 1.000 | 0.993 | 1.000 | 1.026 | 1.000 | 0.997 | 1.000 | ok |
| lib/sel_count | 17 | 217.2 | 216.9 | 216.3 | 217.0 | 388.2 | 15509.5 | 14438.4 | 0.559 | 0.559 | 0.997 | 1.000 | 1.003 | 1.000 | 1.000 | 1.000 | 1.005 | 1.000 | ok |
| lib/sel_loop | 20 | 213.2 | 211.5 | 214.5 | 217.8 | 5263.6 | 53689.2 | 54662.9 | 0.041 | 0.040 | 0.998 | 1.000 | 0.988 | 1.000 | 0.985 | 1.000 | 0.998 | 1.000 | ok |
| lib/sel_n | 20 | 320.5 | 324.8 | 327.6 | 330.4 | 675.4 | 25345.4 | 23674.6 | 0.474 | 0.481 | 0.999 | 1.000 | 1.011 | 1.000 | 1.022 | 1.000 | 1.024 | 1.000 | ok |
| lib/setmt | 20 | 184.3 | 159.4 | 224.1 | 189.4 | 331.9 | 743.5 | 835.9 | 0.555 | 0.480 | 0.979 | 1.000 | 0.979 | 0.998 | 0.979 | 1.000 | 0.985 | 1.000 | ok |
| lib/str_eq | 20 | 159.7 | 159.2 | 160.0 | 159.5 | 182.5 | 1479.5 | 1529.4 | 0.875 | 0.872 | 0.993 | 1.000 | 0.989 | 1.000 | 0.992 | 1.000 | 0.992 | 1.000 | ok |
| lib/str_eq_dyn | 20 | 134.7 | 134.3 | 134.1 | 133.6 | 150.4 | 1277.3 | 1348.8 | 0.896 | 0.893 | 1.000 | 1.000 | 0.990 | 1.000 | 0.978 | 1.000 | 0.987 | 1.000 | ok |
| lib/str_eq_long | 20 | 35.4 | 35.2 | 35.5 | 35.0 | 21.5 | 215.5 | 222.1 | 1.646 | 1.640 | 0.990 | 1.000 | 1.007 | 1.000 | 1.012 | 1.000 | 0.979 | 1.000 | ok |
| lib/str_idx_concat_key | 5 | 206.2 | 191.1 | 206.5 | 192.9 | 166.6 | 464.0 | 461.2 | 1.238 | 1.147 | 0.997 | 1.000 | 1.000 | 0.996 | 0.991 | 1.000 | 0.994 | 1.000 | ok |
| lib/str_lt | 5 | 94.9 | 91.5 | 89.7 | 87.4 | 66.3 | 227.1 | 228.3 | 1.430 | 1.379 | 1.000 | 1.000 | 0.993 | 1.000 | 0.982 | 1.000 | 0.998 | 1.000 | ok |
| lib/str_lt_short | 20 | 113.8 | 105.4 | 104.8 | 98.2 | 62.5 | 249.8 | 255.3 | 1.819 | 1.686 | 0.996 | 1.000 | 0.986 | 1.000 | 0.984 | 1.000 | 0.990 | 1.000 | ok |
| lib/str_method_len | 20 | 45.4 | 45.2 | 51.0 | 50.8 | 55.3 | 1386.6 | 1429.7 | 0.821 | 0.818 | 1.002 | 1.000 | 1.012 | 1.000 | 1.006 | 1.000 | 1.014 | 1.000 | ok |
| lib/string_key_const | 2 | 176.9 | 176.5 | 201.0 | 200.6 | 176.9 | 1151.6 | 1165.1 | 1.000 | 0.998 | 0.993 | 1.000 | 0.993 | 1.000 | 0.994 | 1.000 | 0.997 | 1.000 | ok |
| lib/string_key_dyn | 5 | 228.2 | 227.7 | 227.4 | 227.0 | 268.0 | 1082.3 | 1101.0 | 0.851 | 0.850 | 0.994 | 1.000 | 0.993 | 1.000 | 0.995 | 1.000 | 0.995 | 1.000 | ok |
| lib/sub | 2 | 131.0 | 122.1 | 130.6 | 121.1 | 80.5 | 469.5 | 482.7 | 1.627 | 1.517 | 1.000 | 1.000 | 1.000 | 1.000 | 1.004 | 1.000 | 1.003 | 1.000 | ok |
| lib/sub_1char | 20 | 118.4 | 109.0 | 120.8 | 108.0 | 98.2 | 459.2 | 478.8 | 1.206 | 1.110 | 0.990 | 1.000 | 0.991 | 1.000 | 0.990 | 1.000 | 0.991 | 1.000 | ok |
| lib/tconcat | 20 | 142.7 | 99.8 | 175.2 | 121.0 | 133.4 | 346.9 | 322.3 | 1.070 | 0.748 | 0.942 | 0.998 | 0.985 | 0.998 | 1.005 | 1.001 | 0.999 | 1.001 | ok |
| lib/tconcat_num | 20 | 173.8 | 116.9 | 181.4 | 120.4 | 246.0 | 908.0 | 844.8 | 0.706 | 0.475 | 1.007 | 1.000 | 0.997 | 1.000 | 0.994 | 1.000 | 1.004 | 1.000 | ok |
| lib/tfront_1k | 20 | 78.6 | 75.9 | 79.9 | 76.0 | 406.3 | 4375.7 | 4241.0 | 0.194 | 0.187 | 1.005 | 1.000 | 1.003 | 0.999 | 0.999 | 1.002 | 1.004 | 1.002 | ok |
| lib/tinsert_append | 20 | 157.6 | 132.7 | 192.7 | 182.9 | 141.7 | 539.7 | 574.7 | 1.112 | 0.936 | 0.989 | 1.000 | 0.992 | 0.999 | 0.998 | 1.011 | 1.006 | 1.012 | ok |
| lib/tinsert_front | 20 | 224.9 | 179.8 | 259.4 | 207.3 | 272.9 | 455.4 | 493.5 | 0.824 | 0.659 | 1.001 | 1.000 | 1.010 | 0.999 | 0.983 | 0.992 | 0.982 | 0.985 | ok |
| lib/tinsert_idx | 20 | 175.4 | 151.7 | 204.3 | 193.1 | 154.6 | 328.8 | 383.3 | 1.135 | 0.981 | 0.979 | 1.000 | 0.993 | 1.000 | 1.005 | 1.011 | 1.009 | 1.012 | ok |
| lib/tmove | 5 | 103.0 | 102.7 | 124.5 | 124.4 | 105.8 | 700.5 | 683.1 | 0.974 | 0.971 | 0.992 | 1.000 | 0.993 | 1.000 | 0.994 | 1.000 | 0.995 | 1.000 | ok |
| lib/tmove_1k | 20 | 98.8 | 68.3 | 108.4 | 71.2 | 111.6 | 476.4 | 447.3 | 0.885 | 0.612 | 1.034 | 1.000 | 0.994 | 0.996 | 1.012 | 1.002 | 1.005 | 1.000 | ok |
| lib/tonumber_bad | 17 | 153.6 | 115.7 | 167.2 | 143.8 | 168.0 | 408.1 | 431.1 | 0.914 | 0.689 | 1.066 | 1.000 | 0.969 | 0.996 | 1.012 | 1.000 | 1.003 | 1.000 | ok |
| lib/tonumber_base | 2 | 268.0 | 260.3 | 268.7 | 261.4 | 229.2 | 432.1 | 451.7 | 1.169 | 1.136 | 1.005 | 1.000 | 0.993 | 1.000 | 0.992 | 1.000 | 0.995 | 1.000 | ok |
| lib/tonumber_flt | 2 | 193.7 | 169.4 | 200.7 | 166.8 | 776.7 | 577.4 | 596.1 | 0.249 | 0.218 | 0.987 | 1.000 | 0.979 | 0.996 | 0.984 | 1.000 | 1.001 | 1.000 | ok |
| lib/tonumber_int | 5 | 155.1 | 140.8 | 162.5 | 142.5 | 96.2 | 288.1 | 305.0 | 1.613 | 1.465 | 1.017 | 1.000 | 1.012 | 1.005 | 1.017 | 1.000 | 1.005 | 1.000 | ok |
| lib/tonumber_num | 5 | 216.3 | 215.5 | 214.5 | 216.9 | 383.1 | 6476.7 | 7506.3 | 0.565 | 0.563 | 0.999 | 1.000 | 1.005 | 1.000 | 1.000 | 1.000 | 1.015 | 1.000 | ok |
| lib/tostring_flt | 2 | 84.8 | 66.4 | 85.9 | 61.8 | 82.5 | 299.3 | 214.9 | 1.028 | 0.805 | 1.004 | 1.000 | 1.010 | 1.003 | 1.025 | 0.999 | 1.005 | 1.000 | ok |
| lib/tostring_int | 5 | 122.2 | 109.0 | 137.3 | 123.6 | 279.8 | 543.3 | 450.7 | 0.437 | 0.389 | 1.010 | 1.000 | 1.007 | 1.004 | 1.005 | 1.000 | 1.013 | 1.000 | ok |
| lib/tostring_tbl | 2 | 109.6 | 108.2 | 111.0 | 106.9 | 93.3 | 313.8 | 280.9 | 1.174 | 1.159 | 1.019 | 1.000 | 1.021 | 1.000 | 1.012 | 1.000 | 1.008 | 1.000 | ok |
| lib/tpack | 20 | 149.3 | 117.2 | 210.2 | 169.1 | 306.1 | 499.0 | 485.7 | 0.488 | 0.383 | 1.018 | 1.012 | 1.033 | 1.023 | 0.982 | 1.007 | 1.025 | 1.003 | ok |
| lib/tremove_front | 20 | 146.8 | 132.6 | 185.8 | 166.9 | 191.0 | 592.3 | 593.7 | 0.769 | 0.694 | 0.987 | 1.000 | 1.005 | 0.998 | 1.028 | 0.999 | 1.004 | 0.999 | ok |
| lib/tremove_pop | 5 | 242.6 | 172.8 | 221.5 | 208.6 | 136.7 | 483.9 | 552.9 | 1.774 | 1.264 | 1.006 | 1.000 | 1.008 | 1.000 | 1.005 | 1.003 | 1.013 | 1.007 | ok |
| lib/tsort_cmp | 20 | 267.8 | 254.3 | 270.2 | 261.3 | 285.5 | 339.9 | 336.1 | 0.938 | 0.891 | 1.012 | 1.000 | 0.973 | 0.993 | 0.966 | 1.002 | 1.007 | 0.999 | ok |
| lib/tsort_int | 5 | 266.9 | 218.5 | 294.7 | 223.5 | 294.0 | 382.4 | 406.0 | 0.908 | 0.743 | 0.992 | 0.999 | 0.980 | 0.987 | 1.015 | 1.005 | 0.994 | 0.998 | ok |
| lib/tsort_str | 5 | 258.3 | 150.9 | 291.5 | 159.4 | 244.8 | 364.6 | 401.0 | 1.055 | 0.616 | 1.001 | 1.001 | 0.972 | 0.993 | 0.999 | 1.005 | 0.996 | 1.002 | ok |
| lib/tunpack | 20 | 124.3 | 104.0 | 125.6 | 111.3 | 154.8 | 487.1 | 448.5 | 0.803 | 0.672 | 1.050 | 1.000 | 0.996 | 0.998 | 0.988 | 1.000 | 0.996 | 0.999 | ok |
| lib/type_call | 2 | 87.2 | 86.8 | 94.0 | 93.7 | 85.6 | 494.3 | 468.3 | 1.018 | 1.014 | 0.992 | 1.000 | 0.992 | 1.000 | 0.992 | 1.000 | 0.995 | 1.000 | ok |
| lib/upper | 2 | 95.6 | 88.5 | 99.9 | 95.1 | 120.7 | 307.4 | 312.6 | 0.792 | 0.733 | 1.005 | 1.000 | 0.997 | 1.000 | 0.988 | 1.000 | 0.993 | 1.000 | ok |
| lib/utf8_len | 20 | 272.6 | 275.0 | 274.5 | 279.7 | - | 442.2 | 456.7 | - | - | 1.007 | 1.000 | 1.001 | 1.000 | 1.004 | 1.000 | 1.002 | 1.000 | ok |
| programs/binary_trees | 20 | 1549.4 | 1318.0 | 1752.0 | 1484.0 | 2007.4 | 4389.1 | 4447.3 | 0.772 | 0.657 | 1.004 | 1.000 | 0.993 | 0.998 | 0.973 | 1.000 | 0.991 | 1.000 | ok |
| programs/closures | 20 | 460.7 | 424.9 | 504.6 | 477.9 | 949.8 | 5063.2 | 4986.1 | 0.485 | 0.447 | 1.003 | 1.000 | 1.010 | 1.000 | 0.982 | 1.000 | 1.003 | 1.000 | ok |
| programs/coroutines | 17 | 1812.6 | 1334.3 | 1781.7 | 1421.0 | 871.7 | 3733.4 | 3656.9 | 2.079 | 1.531 | 1.056 | 1.000 | 1.010 | 0.999 | 0.990 | 1.000 | 1.018 | 1.000 | ok |
| programs/fannkuch | 2 | 1064.1 | 1056.5 | 1076.7 | 1070.8 | 1075.7 | 4075.1 | 4114.8 | 0.989 | 0.982 | 1.004 | 1.000 | 1.008 | 1.000 | 0.989 | 1.000 | 0.992 | 1.000 | ok |
| programs/fasta | 2 | 1308.5 | 1166.6 | 1394.3 | 1267.6 | 1495.7 | 3776.8 | 3842.3 | 0.875 | 0.780 | 1.013 | 1.000 | 0.988 | 1.000 | 0.999 | 1.000 | 0.997 | 1.000 | ok |
| programs/fib | 20 | 1258.6 | 1260.0 | 1255.5 | 1250.4 | 469.7 | 4139.5 | 4150.5 | 2.680 | 2.683 | 1.001 | 1.000 | 0.976 | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | ok |
| programs/gc_churn | 20 | 2201.5 | 1864.6 | 3315.9 | 2846.2 | 3608.7 | 4605.5 | 5988.9 | 0.610 | 0.517 | 1.020 | 1.000 | 1.010 | 1.003 | 1.000 | 0.996 | 1.014 | 0.994 | ok |
| programs/heapsort | 20 | 1079.3 | 1074.5 | 2273.6 | 2258.0 | 1286.6 | 4063.2 | 3957.0 | 0.839 | 0.835 | 1.022 | 1.000 | 1.014 | 1.000 | 1.005 | 0.999 | 1.001 | 0.999 | ok |
| programs/json | 2 | 1837.7 | 1419.4 | 1956.5 | 1573.7 | 1470.3 | 3127.6 | 3229.9 | 1.250 | 0.965 | 1.015 | 0.997 | 1.011 | 0.998 | 1.020 | 1.001 | 1.021 | 1.001 | ok |
| programs/knucleotide | 5 | 1051.7 | 1073.4 | 1057.0 | 1062.4 | 691.4 | 4061.5 | 4161.0 | 1.521 | 1.552 | 1.000 | 1.000 | 1.001 | 1.000 | 0.987 | 1.000 | 0.989 | 1.000 | ok |
| programs/load_big | 20 | 750.4 | 556.1 | 673.3 | 457.8 | 491.1 | 1787.3 | 620.0 | 1.528 | 1.132 | 1.020 | 1.000 | 1.025 | 1.000 | 1.056 | 1.000 | 1.014 | 1.000 | CHECK |
| programs/loops_arith | 2 | 482.8 | 481.5 | 485.6 | 484.6 | 1108.2 | 4344.5 | 4909.6 | 0.436 | 0.435 | 1.007 | 1.000 | 1.004 | 1.000 | 1.002 | 1.000 | 1.002 | 1.000 | CHECK |
| programs/mandelbrot | 2 | 536.0 | 535.2 | 535.9 | 537.6 | 521.6 | 4727.1 | 5530.2 | 1.028 | 1.026 | 1.004 | 1.000 | 1.003 | 1.000 | 1.001 | 1.000 | 1.006 | 1.000 | ok |
| programs/matmul | 17 | 424.9 | 430.1 | 440.1 | 441.0 | 272.0 | 3171.0 | 3693.4 | 1.562 | 1.581 | 1.009 | 1.000 | 1.001 | 1.000 | 1.019 | 1.000 | 1.018 | 1.000 | ok |
| programs/metamethods | 2 | 674.7 | 620.2 | 801.0 | 751.3 | 540.1 | 4614.3 | 4669.9 | 1.249 | 1.148 | 1.017 | 1.000 | 0.986 | 0.999 | 1.003 | 1.000 | 1.007 | 1.000 | ok |
| programs/nbody | 2 | 347.8 | 344.3 | 354.4 | 356.5 | 345.6 | 4246.2 | 4718.3 | 1.006 | 0.996 | 1.008 | 0.999 | 1.001 | 1.000 | 0.999 | 0.999 | 1.006 | 1.000 | ok |
| programs/oop_methods | 2 | 144.8 | 142.6 | 144.1 | 143.6 | 304.8 | 4567.3 | 4685.2 | 0.475 | 0.468 | 1.014 | 0.999 | 1.009 | 0.999 | 1.001 | 0.999 | 1.006 | 0.999 | ok |
| programs/pcall_error | 20 | 1494.1 | 1235.5 | 1472.1 | 1238.7 | 8346.6 | 4028.8 | 4363.2 | 0.179 | 0.148 | 0.983 | 1.000 | 1.018 | 0.999 | 0.995 | 1.000 | 0.995 | 1.000 | ok |
| programs/queens | 20 | 1850.8 | 1853.4 | 1919.1 | 1920.9 | 1907.1 | 4368.7 | 4120.8 | 0.970 | 0.972 | 1.007 | 1.000 | 1.007 | 1.000 | 0.989 | 1.000 | 0.991 | 1.000 | ok |
| programs/sieve | 5 | 589.8 | 589.1 | 621.9 | 680.1 | 761.7 | 2750.2 | 5029.1 | 0.774 | 0.773 | 1.023 | 1.000 | 1.023 | 1.000 | 1.021 | 1.000 | 1.013 | 1.000 | ok |
| programs/spectral_norm | 20 | 182.9 | 182.4 | 187.1 | 184.8 | 171.1 | 4573.7 | 4337.9 | 1.069 | 1.066 | 1.004 | 1.000 | 0.996 | 1.000 | 0.993 | 1.000 | 0.992 | 1.000 | ok |
| programs/string_ops | 20 | 1176.6 | 1090.5 | 1112.1 | 1042.2 | 2238.4 | 2828.0 | 2807.1 | 0.526 | 0.487 | 0.998 | 1.000 | 0.998 | 1.000 | 0.990 | 1.000 | 1.010 | 1.000 | ok |
| programs/table_array | 5 | 359.6 | 302.1 | 404.0 | 315.0 | 600.0 | 3400.3 | 3492.4 | 0.599 | 0.503 | 1.006 | 1.000 | 0.991 | 0.987 | 1.008 | 1.004 | 1.003 | 1.000 | ok |
| programs/table_hash | 20 | 543.6 | 539.2 | 562.4 | 542.6 | 394.4 | 1501.6 | 1831.0 | 1.378 | 1.367 | 1.037 | 1.001 | 1.053 | 1.000 | 1.070 | 1.002 | 1.053 | 1.001 | ok |
| programs/varargs | 20 | 1236.2 | 1101.8 | 1206.8 | 1098.3 | 2194.8 | 5366.5 | 5084.7 | 0.563 | 0.502 | 1.005 | 1.006 | 1.006 | 1.010 | 1.002 | 1.005 | 0.995 | 1.002 | ok |
| real/game_loop | 20 | 34.8 | 33.3 | 39.4 | 37.9 | 21.7 | 207.6 | 221.0 | 1.601 | 1.536 | 0.991 | 0.989 | 0.994 | 0.992 | 0.995 | 0.990 | 0.994 | 0.993 | ok |
| real/json_roundtrip | 20 | 551.0 | 454.3 | 589.6 | 475.0 | 469.1 | 796.8 | 802.8 | 1.175 | 0.968 | 0.988 | 0.997 | 0.995 | 0.998 | 1.014 | 1.000 | 0.997 | 1.000 | ok |
| real/lua_interp | 17 | 111.2 | 107.1 | 111.5 | 106.8 | 59.3 | 223.6 | 225.0 | 1.876 | 1.806 | 1.080 | 0.996 | 1.068 | 0.998 | 1.010 | 0.997 | 1.010 | 0.998 | ok |
| real/template_engine | 20 | 748.6 | 580.8 | 749.2 | 615.5 | 955.0 | 1225.1 | 1249.2 | 0.784 | 0.608 | 1.030 | 1.000 | 0.978 | 1.000 | 1.008 | 1.000 | 0.997 | 1.000 | ok |
| real/wordfreq | 20 | 112.8 | 96.6 | 114.2 | 97.8 | 116.4 | 212.8 | 210.2 | 0.969 | 0.830 | 0.994 | 1.000 | 0.992 | 1.000 | 0.993 | 1.000 | 0.987 | 1.000 | ok |

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
