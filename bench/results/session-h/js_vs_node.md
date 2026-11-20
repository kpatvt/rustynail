# JS ports: ours vs node vs LuaJIT (subject 8c39465, 5.5 dev release build, JIT on)

node v26.13.1, min of 5 runs, taskset CPU 4, perf stat cycles:u. All 30 JS ports print exactly what the Lua programs print (run.sh checks before timing, 0 mismatches in both runs).

## Default workloads (run.sh)

Empty node script: 47 Mcyc, 27 ms (inside every node number). Ratios of cycles unless marked wall; above 1 = ours slower.

| program | ours Mcyc | node Mcyc | LuaJIT Mcyc | ours/node cyc | ours/node-ex cyc | ours/node wall | ours/LuaJIT cyc | ours ms | node ms | LuaJIT ms |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| binary_trees | 1471 | 780 | 2124 | 1.88 | 2.00 | 1.67 | 0.69 | 309 | 185 | 440 |
| fib | 1230 | 702 | 475 | 1.75 | 1.88 | 1.68 | 2.59 | 257 | 153 | 106 |
| queens | 1831 | 1179 | 1896 | 1.55 | 1.62 | 1.51 | 0.97 | 375 | 248 | 387 |
| heapsort | 1055 | 738 | n/a | 1.43 | 1.53 | 1.35 | n/a | 221 | 164 | n/a |
| fannkuch | 1069 | 760 | 1077 | 1.41 | 1.50 | 1.34 | 0.99 | 222 | 166 | 221 |
| list | 123 | 91 | 89 | 1.35 | 2.75 | 1.04 | 1.39 | 31 | 30 | 25 |
| cd | 257 | 192 | 107 | 1.33 | 1.76 | 1.11 | 2.39 | 60 | 54 | 32 |
| nbody | 351 | 276 | 340 | 1.27 | 1.53 | 1.13 | 1.03 | 78 | 69 | 76 |
| matmul | 435 | 387 | 274 | 1.12 | 1.28 | 1.02 | 1.59 | 95 | 93 | 63 |
| storage | 102 | 98 | 153 | 1.04 | 1.99 | 0.83 | 0.66 | 28 | 33 | 38 |
| mandelbrot | 532 | 560 | 521 | 0.95 | 1.04 | 0.91 | 1.02 | 115 | 126 | 112 |
| fasta | 1275 | 1387 | n/a | 0.92 | 0.95 | 0.89 | n/a | 269 | 302 | n/a |
| raytrace | 125 | 142 | 41 | 0.88 | 1.31 | 0.77 | 3.07 | 32 | 42 | 14 |
| deltablue | 108 | 137 | 107 | 0.78 | 1.19 | 0.69 | 1.01 | 29 | 42 | 29 |
| table_array | 357 | 470 | n/a | 0.76 | 0.84 | 0.70 | n/a | 80 | 115 | n/a |
| events | 164 | 221 | 100 | 0.74 | 0.94 | 0.68 | 1.64 | 41 | 60 | 28 |
| json | 141 | 196 | 75 | 0.72 | 0.94 | 0.67 | 1.88 | 36 | 54 | 22 |
| lru_cache | 147 | 216 | 115 | 0.68 | 0.87 | 0.66 | 1.28 | 38 | 58 | 33 |
| closures | 479 | 727 | n/a | 0.66 | 0.70 | 0.65 | n/a | 104 | 160 | n/a |
| expr_lang | 136 | 210 | 97 | 0.65 | 0.83 | 0.63 | 1.39 | 36 | 57 | 28 |
| towers | 50 | 90 | 23 | 0.56 | 1.16 | 0.54 | 2.18 | 16 | 30 | 10 |
| havlak | 155 | 284 | 116 | 0.55 | 0.65 | 0.54 | 1.33 | 41 | 76 | 33 |
| bounce | 60 | 111 | 47 | 0.54 | 0.93 | 0.52 | 1.26 | 18 | 34 | 15 |
| permute | 43 | 81 | 14 | 0.53 | 1.26 | 0.53 | 2.98 | 14 | 27 | 12 |
| richards | 60 | 117 | 35 | 0.51 | 0.86 | 0.52 | 1.72 | 18 | 36 | 13 |
| spectral_norm | 180 | 375 | 171 | 0.48 | 0.55 | 0.47 | 1.05 | 42 | 89 | 42 |
| table_hash | 530 | 1403 | 431 | 0.38 | 0.39 | 0.39 | 1.23 | 118 | 301 | 97 |
| knucleotide | 1039 | 3677 | 683 | 0.28 | 0.29 | 0.29 | 1.52 | 222 | 781 | 150 |
| sieve | 573 | 2314 | 1017 | 0.25 | 0.25 | 0.27 | 0.56 | 143 | 520 | 225 |
| pcall_error | 1462 | 7748 | n/a | 0.19 | 0.19 | 0.19 | n/a | 303 | 1567 | n/a |

Geomean ours/node: cycles 0.75, wall 0.71 (30 programs); ours/LuaJIT cycles 1.36 (25 programs).

## 20x workloads (run.sh -s 20; the 14 short programs)

Empty node script: 48 Mcyc, 27 ms (inside every node number). Ratios of cycles unless marked wall; above 1 = ours slower.

| program | ours Mcyc | node Mcyc | LuaJIT Mcyc | ours/node cyc | ours/node-ex cyc | ours/node wall | ours/LuaJIT cyc | ours ms | node ms | LuaJIT ms |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| cd | 2929 | 545 | 1542 | 5.37 | 5.88 | 4.66 | 1.90 | 594 | 127 | 320 |
| list | 2325 | 496 | 1732 | 4.68 | 5.18 | 4.15 | 1.34 | 475 | 114 | 354 |
| storage | 2016 | 431 | 3054 | 4.68 | 5.26 | 3.63 | 0.66 | 413 | 114 | 616 |
| raytrace | 1440 | 309 | 673 | 4.66 | 5.51 | 3.82 | 2.14 | 297 | 78 | 145 |
| deltablue | 1211 | 324 | 572 | 3.73 | 4.38 | 3.06 | 2.12 | 252 | 82 | 124 |
| richards | 797 | 262 | 562 | 3.04 | 3.72 | 2.46 | 1.42 | 167 | 68 | 120 |
| havlak | 2751 | 916 | 1744 | 3.00 | 3.17 | 2.63 | 1.58 | 562 | 213 | 359 |
| permute | 774 | 308 | 237 | 2.51 | 2.97 | 2.19 | 3.26 | 164 | 75 | 57 |
| towers | 644 | 267 | 365 | 2.41 | 2.93 | 2.01 | 1.77 | 137 | 68 | 80 |
| bounce | 1006 | 421 | 904 | 2.39 | 2.69 | 2.14 | 1.11 | 210 | 98 | 188 |
| expr_lang | 2486 | 1302 | 1700 | 1.91 | 1.98 | 1.81 | 1.46 | 507 | 280 | 350 |
| json | 2038 | 1138 | 1214 | 1.79 | 1.87 | 1.68 | 1.68 | 416 | 247 | 252 |
| events | 3038 | 1767 | 3090 | 1.72 | 1.77 | 1.65 | 0.98 | 616 | 373 | 627 |
| lru_cache | 2079 | 1935 | 2781 | 1.07 | 1.10 | 1.03 | 0.75 | 424 | 413 | 563 |

Geomean ours/node: cycles 2.79, wall 2.44 (14 programs); ours/LuaJIT cycles 1.46 (14 programs).
