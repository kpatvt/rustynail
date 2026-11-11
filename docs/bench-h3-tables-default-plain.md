### default, plain
5 noisiest programs overall (max of median/min of ours or LuaJIT): awfy/permute 19.78, lib/fmt_s 1.18, apps/lru_cache 1.13, apps/events 1.12, lib/char 1.09

| set (n) | ours/PUC min | ours/PUC median | ours/LJ min (n with LJ) | ours/LJ median | ours/LJ median without the 5 noisiest overall | dropped here |
|---|---|---|---|---|---|---|
| awfy (14) | 0.286 | 0.286 | 1.509 (14) | 1.212 | 1.398 | 1 |
| apps (5) | 0.586 | 0.590 | 1.420 (5) | 1.348 | 1.410 | 2 |
| programs (25) | 0.239 | 0.238 | 0.999 (25) | 0.995 | 0.995 | 0 |
| jit-trace (29) | 0.086 | 0.085 | 1.076 (29) | 1.073 | 1.073 | 0 |
| lib (95) | 0.290 | 0.289 | 1.203 (91) | 1.199 | 1.182 | 2 |
| real (5) | 0.523 | 0.522 | 1.409 (5) | 1.388 | 1.388 | 0 |
| **all** (173) | 0.238 | 0.237 | 1.181 (169) | 1.154 | 1.156 | |

### warm0, plain
5 noisiest programs overall (max of median/min of ours or LuaJIT): awfy/permute 19.78, lib/fmt_s 1.18, apps/lru_cache 1.13, programs/fib 1.13, apps/events 1.12

| set (n) | ours/PUC min | ours/PUC median | ours/LJ min (n with LJ) | ours/LJ median | ours/LJ median without the 5 noisiest overall | dropped here |
|---|---|---|---|---|---|---|
| awfy (14) | 0.289 | 0.290 | 1.523 (14) | 1.230 | 1.419 | 1 |
| apps (5) | 0.590 | 0.591 | 1.430 (5) | 1.351 | 1.422 | 2 |
| programs (25) | 0.241 | 0.241 | 1.007 (25) | 1.006 | 0.956 | 1 |
| jit-trace (29) | 0.086 | 0.085 | 1.077 (29) | 1.074 | 1.074 | 0 |
| lib (95) | 0.289 | 0.289 | 1.202 (91) | 1.201 | 1.192 | 1 |
| real (5) | 0.518 | 0.519 | 1.397 (5) | 1.381 | 1.381 | 0 |
| **all** (173) | 0.239 | 0.238 | 1.183 (169) | 1.159 | 1.158 | |

### ahead of LuaJIT, plain
| set | faster than LuaJIT by min | by median | programs with LuaJIT |
|---|---|---|---|
| awfy | 2 | 3 | 14 |
| apps | 1 | 1 | 5 |
| programs | 11 | 11 | 25 |
| jit-trace | 9 | 9 | 29 |
| lib | 30 | 30 | 91 |
| real | 1 | 1 | 5 |
| **all** | **54** | **55** | 169 |

### top ours/LJ, plain
| rank | program | ours/LJ min | ours/LJ median |
|---|---|---|---|
| 1 | jit-trace/rec_loop | 4.23 | 3.95 |
| 2 | lib/find_plain | 3.77 | 3.76 |
| 3 | apps/raytrace | 3.75 | 3.70 |
| 4 | awfy/permute | 3.72 | 0.19 |
| 5 | lib/it_next_empty | 3.63 | 3.62 |
| 6 | lib/sub | 3.26 | 3.25 |
| 7 | lib/m_random | 3.21 | 3.20 |
| 8 | programs/fib | 3.10 | 2.99 |
| 9 | lib/fmt_x_pad | 3.07 | 3.08 |
| 10 | lib/m_random_f | 2.91 | 2.91 |
| 11 | lib/tostring_tbl | 2.87 | 2.81 |
| 12 | lib/tonumber_int | 2.72 | 2.73 |
| 13 | jit-trace/tail_loop | 2.70 | 2.71 |
| 14 | lib/rep | 2.68 | 2.65 |
| 15 | lib/tmove | 2.63 | 2.66 |
| 16 | lib/fmt_s | 2.56 | 2.23 |
| 17 | lib/reverse | 2.54 | 2.58 |
| 18 | lib/co_yield | 2.54 | 2.54 |
| 19 | lib/char | 2.41 | 2.32 |
| 20 | lib/byte_multi | 2.36 | 2.35 |
| 21 | lib/rep_sep | 2.27 | 2.28 |
| 22 | lib/getmt | 2.27 | 2.18 |
| 23 | lib/upper | 2.26 | 2.26 |
| 24 | awfy/cd | 2.24 | 2.22 |
| 25 | lib/lower | 2.24 | 2.20 |

programs with min ratio > 2: 35 of 169; with median ratio > 2: 33

### rows awfy plain
| program | ours Mcyc | PUC Mcyc | LJ Mcyc | ours/PUC | ours/LJ min | ours/LJ median |
|---|---|---|---|---|---|---|
| bounce | 68.3 | 519.9 | 47.9 | 0.131 | 1.43 | 1.43 |
| cd | 241.9 | 436.7 | 108.1 | 0.554 | 2.24 | 2.22 |
| deltablue | 104.6 | 158.2 | 102.8 | 0.661 | 1.02 | 1.01 |
| havlak | 156.4 | 218.2 | 116.0 | 0.717 | 1.35 | 1.35 |
| json | 160.6 | 265.0 | 73.6 | 0.606 | 2.18 | 2.17 |
| list | 138.8 | 422.9 | 88.7 | 0.328 | 1.57 | 1.56 |
| mandelbrot | 277.2 | 1326.8 | 272.6 | 0.209 | 1.02 | 1.02 |
| nbody | 131.0 | 1494.0 | 95.0 | 0.088 | 1.38 | 1.40 |
| permute | 52.4 | 252.5 | 14.1 | 0.208 | 3.72 | 0.19 |
| queens | 107.7 | 395.1 | 55.1 | 0.273 | 1.96 | 1.94 |
| richards | 55.5 | 199.6 | 35.0 | 0.278 | 1.59 | 1.58 |
| sieve | 133.6 | 1024.9 | 156.3 | 0.130 | 0.85 | 0.86 |
| storage | 122.0 | 254.1 | 154.3 | 0.480 | 0.79 | 0.79 |
| towers | 45.2 | 225.9 | 23.4 | 0.200 | 1.93 | 1.80 |

### rows apps plain
| program | ours Mcyc | PUC Mcyc | LJ Mcyc | ours/PUC | ours/LJ min | ours/LJ median |
|---|---|---|---|---|---|---|
| events | 173.1 | 291.7 | 142.2 | 0.593 | 1.22 | 1.09 |
| expr_lang | 153.4 | 242.7 | 97.6 | 0.632 | 1.57 | 1.52 |
| lru_cache | 188.4 | 219.8 | 115.8 | 0.857 | 1.63 | 1.45 |
| markdown | 157.7 | 419.2 | 318.7 | 0.376 | 0.49 | 0.50 |
| raytrace | 152.4 | 265.8 | 40.6 | 0.574 | 3.75 | 3.70 |

### rows real plain
| program | ours Mcyc | PUC Mcyc | LJ Mcyc | ours/PUC | ours/LJ min | ours/LJ median |
|---|---|---|---|---|---|---|
| game_loop | 38.7 | 211.2 | 22.3 | 0.183 | 1.73 | 1.68 |
| json_roundtrip | 611.5 | 790.0 | 465.6 | 0.774 | 1.31 | 1.31 |
| lua_interp | 117.2 | 223.1 | 58.0 | 0.526 | 2.02 | 1.96 |
| template_engine | 839.4 | 1215.7 | 956.4 | 0.690 | 0.88 | 0.87 |
| wordfreq | 161.6 | 212.8 | 117.4 | 0.759 | 1.38 | 1.37 |

### rows programs plain
| program | ours Mcyc | PUC Mcyc | LJ Mcyc | ours/PUC | ours/LJ min | ours/LJ median |
|---|---|---|---|---|---|---|
| binary_trees | 1999.0 | 4385.1 | 2070.9 | 0.456 | 0.97 | 0.95 |
| closures | 505.7 | 4864.3 | 953.6 | 0.104 | 0.53 | 0.53 |
| coroutines | 1923.3 | 3729.2 | 868.4 | 0.516 | 2.21 | 2.22 |
| fannkuch | 1157.9 | 4011.7 | 1076.7 | 0.289 | 1.08 | 1.06 |
| fasta | 1578.2 | 3657.3 | 1465.7 | 0.432 | 1.08 | 1.08 |
| fib | 1425.6 | 4078.2 | 460.1 | 0.350 | 3.10 | 2.99 |
| gc_churn | 2245.3 | 4449.2 | 3454.6 | 0.505 | 0.65 | 0.65 |
| heapsort | 1082.7 | 4010.4 | 1229.7 | 0.270 | 0.88 | 0.88 |
| json | 2029.8 | 3088.1 | 1431.7 | 0.657 | 1.42 | 1.39 |
| knucleotide | 1425.1 | 4110.0 | 683.7 | 0.347 | 2.08 | 2.05 |
| load_big | 759.4 | 1733.0 | 488.7 | 0.438 | 1.55 | 1.55 |
| loops_arith | 540.3 | 4341.7 | 1108.4 | 0.124 | 0.49 | 0.49 |
| mandelbrot | 556.9 | 4663.3 | 521.8 | 0.119 | 1.07 | 1.07 |
| matmul | 438.7 | 3335.3 | 269.8 | 0.132 | 1.63 | 1.64 |
| metamethods | 800.6 | 4585.7 | 534.8 | 0.175 | 1.50 | 1.49 |
| nbody | 554.2 | 4270.5 | 344.4 | 0.130 | 1.61 | 1.63 |
| oop_methods | 158.0 | 4682.0 | 305.0 | 0.034 | 0.52 | 0.52 |
| pcall_error | 2084.1 | 4004.3 | 8397.5 | 0.520 | 0.25 | 0.25 |
| queens | 1914.5 | 4398.6 | 1892.5 | 0.435 | 1.01 | 1.01 |
| sieve | 607.8 | 3090.9 | 994.3 | 0.197 | 0.61 | 0.61 |
| spectral_norm | 229.8 | 4375.6 | 171.0 | 0.053 | 1.34 | 1.35 |
| string_ops | 1500.3 | 2680.2 | 2080.1 | 0.560 | 0.72 | 0.71 |
| table_array | 362.1 | 3410.4 | 597.0 | 0.106 | 0.61 | 0.61 |
| table_hash | 629.2 | 1682.8 | 403.1 | 0.374 | 1.56 | 1.58 |
| varargs | 1332.7 | 5320.5 | 2178.5 | 0.250 | 0.61 | 0.60 |

