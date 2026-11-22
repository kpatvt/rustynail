# JS ports: ours vs node vs LuaJIT (subject b116447, 5.5 dev release build, JIT on)

node v26.13.1, min of 5 runs, taskset CPU 4, perf stat cycles:u. All 30 JS ports print exactly what the Lua programs print (run.sh checks before timing, 0 mismatches in both runs).

## Default workloads (run.sh)

Empty node script: 49 Mcyc, 26 ms (inside every node number). Ratios of cycles unless marked wall; above 1 = ours slower.

| program | ours Mcyc | node Mcyc | LuaJIT Mcyc | ours/node cyc | ours/node-ex cyc | ours/node wall | ours/LuaJIT cyc | ours ms | node ms | LuaJIT ms |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| binary_trees | 1466 | 791 | 2185 | 1.85 | 1.98 | 1.65 | 0.67 | 309 | 187 | 453 |
| fib | 1234 | 701 | 471 | 1.76 | 1.89 | 1.67 | 2.62 | 259 | 155 | 103 |
| queens | 1850 | 1175 | 1897 | 1.57 | 1.64 | 1.52 | 0.98 | 378 | 248 | 387 |
| heapsort | 1075 | 758 | n/a | 1.42 | 1.52 | 1.34 | n/a | 224 | 167 | n/a |
| cd | 254 | 183 | 108 | 1.39 | 1.91 | 1.17 | 2.35 | 60 | 51 | 31 |
| fannkuch | 1049 | 767 | 1068 | 1.37 | 1.46 | 1.32 | 0.98 | 219 | 166 | 221 |
| nbody | 348 | 279 | 343 | 1.24 | 1.51 | 1.12 | 1.01 | 78 | 70 | 76 |
| matmul | 423 | 370 | 272 | 1.14 | 1.32 | 1.03 | 1.56 | 94 | 91 | 62 |
| storage | 101 | 91 | 153 | 1.11 | 2.42 | 0.84 | 0.66 | 28 | 33 | 40 |
| list | 92 | 91 | 89 | 1.01 | 2.23 | 0.85 | 1.04 | 25 | 30 | 24 |
| mandelbrot | 532 | 562 | 522 | 0.95 | 1.04 | 0.90 | 1.02 | 114 | 126 | 111 |
| fasta | 1292 | 1398 | n/a | 0.92 | 0.96 | 0.90 | n/a | 273 | 303 | n/a |
| deltablue | 106 | 136 | 107 | 0.77 | 1.21 | 0.70 | 0.98 | 28 | 40 | 29 |
| json | 139 | 186 | 75 | 0.75 | 1.02 | 0.68 | 1.86 | 35 | 52 | 21 |
| raytrace | 106 | 142 | 41 | 0.74 | 1.14 | 0.69 | 2.60 | 28 | 41 | 14 |
| events | 164 | 223 | 145 | 0.74 | 0.95 | 0.69 | 1.13 | 41 | 59 | 36 |
| lru_cache | 150 | 222 | 119 | 0.68 | 0.87 | 0.66 | 1.26 | 39 | 59 | 33 |
| closures | 470 | 714 | n/a | 0.66 | 0.71 | 0.64 | n/a | 102 | 159 | n/a |
| expr_lang | 135 | 205 | 102 | 0.66 | 0.87 | 0.65 | 1.32 | 37 | 56 | 28 |
| table_array | 361 | 550 | n/a | 0.66 | 0.72 | 0.63 | n/a | 81 | 128 | n/a |
| towers | 48 | 86 | 24 | 0.55 | 1.30 | 0.53 | 1.97 | 16 | 30 | 10 |
| bounce | 60 | 117 | 48 | 0.51 | 0.89 | 0.51 | 1.26 | 18 | 36 | 15 |
| richards | 59 | 115 | 35 | 0.51 | 0.90 | 0.52 | 1.68 | 18 | 36 | 13 |
| havlak | 159 | 315 | 123 | 0.50 | 0.60 | 0.50 | 1.30 | 42 | 83 | 34 |
| permute | 43 | 86 | 15 | 0.50 | 1.18 | 0.50 | 2.84 | 14 | 29 | 8 |
| spectral_norm | 180 | 379 | 171 | 0.47 | 0.55 | 0.48 | 1.05 | 44 | 90 | 42 |
| table_hash | 539 | 1345 | 424 | 0.40 | 0.42 | 0.41 | 1.27 | 120 | 289 | 96 |
| knucleotide | 1038 | 3567 | 685 | 0.29 | 0.30 | 0.29 | 1.52 | 222 | 761 | 151 |
| sieve | 584 | 2304 | 1017 | 0.25 | 0.26 | 0.28 | 0.57 | 145 | 516 | 225 |
| pcall_error | 1441 | 7757 | n/a | 0.19 | 0.19 | 0.19 | n/a | 298 | 1572 | n/a |

Geomean ours/node: cycles 0.74, wall 0.70 (30 programs); ours/LuaJIT cycles 1.30 (25 programs).

## 20x workloads (run.sh -s 20; the 14 short programs)

Empty node script: 48 Mcyc, 19 ms (inside every node number). Ratios of cycles unless marked wall; above 1 = ours slower.

| program | ours Mcyc | node Mcyc | LuaJIT Mcyc | ours/node cyc | ours/node-ex cyc | ours/node wall | ours/LuaJIT cyc | ours ms | node ms | LuaJIT ms |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| cd | 2866 | 537 | 1443 | 5.34 | 5.86 | 4.66 | 1.99 | 582 | 125 | 299 |
| storage | 1998 | 433 | 3051 | 4.61 | 5.19 | 3.66 | 0.65 | 410 | 112 | 617 |
| raytrace | 1185 | 319 | 677 | 3.72 | 4.37 | 3.03 | 1.75 | 246 | 81 | 143 |
| deltablue | 1191 | 327 | 592 | 3.64 | 4.27 | 3.02 | 2.01 | 247 | 82 | 127 |
| havlak | 2987 | 925 | 1984 | 3.23 | 3.41 | 2.84 | 1.51 | 605 | 213 | 407 |
| list | 1559 | 495 | 1728 | 3.15 | 3.49 | 2.78 | 0.90 | 319 | 114 | 352 |
| richards | 802 | 263 | 569 | 3.05 | 3.73 | 2.50 | 1.41 | 168 | 67 | 123 |
| permute | 775 | 307 | 218 | 2.52 | 2.99 | 2.17 | 3.55 | 164 | 75 | 51 |
| bounce | 1005 | 422 | 907 | 2.38 | 2.69 | 2.13 | 1.11 | 208 | 98 | 190 |
| towers | 610 | 271 | 361 | 2.25 | 2.74 | 1.90 | 1.69 | 130 | 68 | 80 |
| events | 3040 | 1775 | 2701 | 1.71 | 1.76 | 1.65 | 1.13 | 617 | 375 | 559 |
| json | 2017 | 1182 | 1213 | 1.71 | 1.78 | 1.61 | 1.66 | 412 | 256 | 250 |
| expr_lang | 2024 | 1343 | 1705 | 1.51 | 1.56 | 1.44 | 1.19 | 418 | 290 | 351 |
| lru_cache | 2180 | 1985 | 2911 | 1.10 | 1.13 | 1.04 | 0.75 | 444 | 425 | 591 |

Geomean ours/node: cycles 2.61, wall 2.29 (14 programs); ours/LuaJIT cycles 1.39 (14 programs).
