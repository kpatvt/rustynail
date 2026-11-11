### default, pgo
5 noisiest programs overall (max of median/min of ours or LuaJIT): awfy/permute 19.78, lib/fmt_s 1.18, apps/lru_cache 1.13, apps/events 1.12, programs/fib 1.11

| set (n) | ours/PUC min | ours/PUC median | ours/LJ min (n with LJ) | ours/LJ median | ours/LJ median without the 5 noisiest overall | dropped here |
|---|---|---|---|---|---|---|
| awfy (14) | 0.273 | 0.272 | 1.441 (14) | 1.156 | 1.329 | 1 |
| apps (5) | 0.524 | 0.527 | 1.268 (5) | 1.203 | 1.236 | 2 |
| programs (25) | 0.222 | 0.222 | 0.930 (25) | 0.929 | 0.881 | 1 |
| jit-trace (29) | 0.085 | 0.084 | 1.066 (29) | 1.062 | 1.062 | 0 |
| lib (95) | 0.265 | 0.264 | 1.093 (91) | 1.090 | 1.082 | 1 |
| real (5) | 0.466 | 0.465 | 1.255 (5) | 1.238 | 1.238 | 0 |
| **all** (173) | 0.222 | 0.221 | 1.097 (169) | 1.072 | 1.071 | |

### warm0, pgo
5 noisiest programs overall (max of median/min of ours or LuaJIT): awfy/permute 19.78, lib/fmt_s 1.18, programs/fib 1.13, apps/lru_cache 1.13, apps/events 1.12

| set (n) | ours/PUC min | ours/PUC median | ours/LJ min (n with LJ) | ours/LJ median | ours/LJ median without the 5 noisiest overall | dropped here |
|---|---|---|---|---|---|---|
| awfy (14) | 0.275 | 0.276 | 1.452 (14) | 1.171 | 1.348 | 1 |
| apps (5) | 0.525 | 0.527 | 1.271 (5) | 1.205 | 1.246 | 2 |
| programs (25) | 0.223 | 0.224 | 0.934 (25) | 0.935 | 0.886 | 1 |
| jit-trace (29) | 0.085 | 0.084 | 1.068 (29) | 1.063 | 1.063 | 0 |
| lib (95) | 0.265 | 0.264 | 1.094 (91) | 1.090 | 1.083 | 1 |
| real (5) | 0.463 | 0.463 | 1.248 (5) | 1.230 | 1.230 | 0 |
| **all** (173) | 0.222 | 0.221 | 1.098 (169) | 1.074 | 1.073 | |

### ahead of LuaJIT, pgo
| set | faster than LuaJIT by min | by median | programs with LuaJIT |
|---|---|---|---|
| awfy | 3 | 4 | 14 |
| apps | 1 | 1 | 5 |
| programs | 11 | 11 | 25 |
| jit-trace | 9 | 9 | 29 |
| lib | 37 | 36 | 91 |
| real | 1 | 1 | 5 |
| **all** | **62** | **62** | 169 |

### top ours/LJ, pgo
| rank | program | ours/LJ min | ours/LJ median |
|---|---|---|---|
| 1 | jit-trace/rec_loop | 4.05 | 3.80 |
| 2 | awfy/permute | 3.70 | 0.19 |
| 3 | lib/it_next_empty | 3.58 | 3.57 |
| 4 | lib/find_plain | 3.51 | 3.59 |
| 5 | apps/raytrace | 3.17 | 3.14 |
| 6 | programs/fib | 3.10 | 3.32 |
| 7 | lib/sub | 3.08 | 3.08 |
| 8 | jit-trace/tail_loop | 2.70 | 2.71 |
| 9 | lib/tonumber_int | 2.65 | 2.65 |
| 10 | lib/m_random_f | 2.51 | 2.51 |
| 11 | lib/reverse | 2.47 | 2.50 |
| 12 | lib/fmt_s | 2.46 | 2.12 |
| 13 | lib/byte_multi | 2.35 | 2.33 |
| 14 | lib/type_call | 2.31 | 2.34 |
| 15 | lib/tostring_tbl | 2.30 | 2.26 |
| 16 | lib/char | 2.28 | 2.22 |
| 17 | lib/getmt | 2.22 | 2.15 |
| 18 | lib/rep_sep | 2.19 | 2.19 |
| 19 | lib/lower | 2.18 | 2.14 |
| 20 | lib/upper | 2.17 | 2.13 |
| 21 | lib/rep | 2.17 | 2.18 |
| 22 | lib/it_pairs_hash | 2.15 | 2.19 |
| 23 | lib/tmove | 2.13 | 2.20 |
| 24 | lib/byte | 2.12 | 2.12 |
| 25 | awfy/cd | 2.06 | 2.05 |

programs with min ratio > 2: 29 of 169; with median ratio > 2: 28

### rows awfy pgo
| program | ours Mcyc | PUC Mcyc | LJ Mcyc | ours/PUC | ours/LJ min | ours/LJ median |
|---|---|---|---|---|---|---|
| bounce | 67.1 | 519.9 | 47.9 | 0.129 | 1.40 | 1.40 |
| cd | 223.1 | 436.7 | 108.1 | 0.511 | 2.06 | 2.05 |
| deltablue | 98.4 | 158.2 | 102.8 | 0.622 | 0.96 | 0.95 |
| havlak | 137.7 | 218.2 | 116.0 | 0.631 | 1.19 | 1.18 |
| json | 139.4 | 265.0 | 73.6 | 0.526 | 1.89 | 1.87 |
| list | 135.8 | 422.9 | 88.7 | 0.321 | 1.53 | 1.52 |
| mandelbrot | 277.0 | 1326.8 | 272.6 | 0.209 | 1.02 | 1.02 |
| nbody | 131.1 | 1494.0 | 95.0 | 0.088 | 1.38 | 1.38 |
| permute | 52.1 | 252.5 | 14.1 | 0.206 | 3.70 | 0.19 |
| queens | 106.1 | 395.1 | 55.1 | 0.268 | 1.93 | 1.92 |
| richards | 54.3 | 199.6 | 35.0 | 0.272 | 1.55 | 1.54 |
| sieve | 133.7 | 1024.9 | 156.3 | 0.130 | 0.86 | 0.86 |
| storage | 107.3 | 254.1 | 154.3 | 0.422 | 0.70 | 0.69 |
| towers | 44.4 | 225.9 | 23.4 | 0.196 | 1.90 | 1.77 |

### rows apps pgo
| program | ours Mcyc | PUC Mcyc | LJ Mcyc | ours/PUC | ours/LJ min | ours/LJ median |
|---|---|---|---|---|---|---|
| events | 158.6 | 291.7 | 142.2 | 0.544 | 1.12 | 1.01 |
| expr_lang | 135.9 | 242.7 | 97.6 | 0.560 | 1.39 | 1.35 |
| lru_cache | 171.4 | 219.8 | 115.8 | 0.780 | 1.48 | 1.33 |
| markdown | 143.6 | 419.2 | 318.7 | 0.343 | 0.45 | 0.45 |
| raytrace | 128.7 | 265.8 | 40.6 | 0.484 | 3.17 | 3.14 |

### rows real pgo
| program | ours Mcyc | PUC Mcyc | LJ Mcyc | ours/PUC | ours/LJ min | ours/LJ median |
|---|---|---|---|---|---|---|
| game_loop | 37.8 | 211.2 | 22.3 | 0.179 | 1.69 | 1.64 |
| json_roundtrip | 505.4 | 790.0 | 465.6 | 0.640 | 1.09 | 1.09 |
| lua_interp | 110.1 | 223.1 | 58.0 | 0.494 | 1.90 | 1.85 |
| template_engine | 713.4 | 1215.7 | 956.4 | 0.587 | 0.75 | 0.74 |
| wordfreq | 140.3 | 212.8 | 117.4 | 0.659 | 1.19 | 1.19 |

### rows programs pgo
| program | ours Mcyc | PUC Mcyc | LJ Mcyc | ours/PUC | ours/LJ min | ours/LJ median |
|---|---|---|---|---|---|---|
| binary_trees | 1748.0 | 4385.1 | 2070.9 | 0.399 | 0.84 | 0.82 |
| closures | 467.9 | 4864.3 | 953.6 | 0.096 | 0.49 | 0.49 |
| coroutines | 1585.3 | 3729.2 | 868.4 | 0.425 | 1.83 | 1.82 |
| fannkuch | 1159.4 | 4011.7 | 1076.7 | 0.289 | 1.08 | 1.06 |
| fasta | 1501.6 | 3657.3 | 1465.7 | 0.411 | 1.02 | 1.03 |
| fib | 1426.3 | 4078.2 | 460.1 | 0.350 | 3.10 | 3.32 |
| gc_churn | 1774.3 | 4449.2 | 3454.6 | 0.399 | 0.51 | 0.51 |
| heapsort | 1085.2 | 4010.4 | 1229.7 | 0.271 | 0.88 | 0.88 |
| json | 1638.7 | 3088.1 | 1431.7 | 0.531 | 1.14 | 1.12 |
| knucleotide | 1351.0 | 4110.0 | 683.7 | 0.329 | 1.98 | 1.95 |
| load_big | 619.8 | 1733.0 | 488.7 | 0.358 | 1.27 | 1.26 |
| loops_arith | 540.6 | 4341.7 | 1108.4 | 0.125 | 0.49 | 0.49 |
| mandelbrot | 556.7 | 4663.3 | 521.8 | 0.119 | 1.07 | 1.07 |
| matmul | 442.0 | 3335.3 | 269.8 | 0.133 | 1.64 | 1.63 |
| metamethods | 756.5 | 4585.7 | 534.8 | 0.165 | 1.41 | 1.42 |
| nbody | 556.6 | 4270.5 | 344.4 | 0.130 | 1.62 | 1.62 |
| oop_methods | 156.7 | 4682.0 | 305.0 | 0.033 | 0.51 | 0.52 |
| pcall_error | 1878.0 | 4004.3 | 8397.5 | 0.469 | 0.22 | 0.22 |
| queens | 1911.0 | 4398.6 | 1892.5 | 0.434 | 1.01 | 1.01 |
| sieve | 638.9 | 3090.9 | 994.3 | 0.207 | 0.64 | 0.64 |
| spectral_norm | 228.9 | 4375.6 | 171.0 | 0.052 | 1.34 | 1.34 |
| string_ops | 1300.6 | 2680.2 | 2080.1 | 0.485 | 0.63 | 0.62 |
| table_array | 311.9 | 3410.4 | 597.0 | 0.091 | 0.52 | 0.52 |
| table_hash | 608.5 | 1682.8 | 403.1 | 0.362 | 1.51 | 1.51 |
| varargs | 1107.4 | 5320.5 | 2178.5 | 0.208 | 0.51 | 0.50 |

