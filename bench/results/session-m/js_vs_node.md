# JS ports: ours vs node vs LuaJIT (subject 8cd1f06, 5.5 dev release build, JIT on)

node v26.13.1, min of 5 runs, taskset CPU 4, perf stat cycles:u. All 33 JS ports print exactly what the Lua programs print (run.sh checks before timing, 0 mismatches in both runs).

## Default workloads (run.sh)

Empty node script: 48 Mcyc, 21 ms (inside every node number). Ratios of cycles unless marked wall; above 1 = ours slower.

| program | ours Mcyc | node Mcyc | LuaJIT Mcyc | ours/node cyc | ours/node-ex cyc | ours/node wall | ours/LuaJIT cyc | ours ms | node ms | LuaJIT ms |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| classic_vector | 476 | 181 | 244 | 2.63 | 3.58 | 2.08 | 1.95 | 104 | 50 | 57 |
| binary_trees | 1164 | 730 | 2178 | 1.59 | 1.71 | 1.40 | 0.53 | 255 | 182 | 451 |
| queens | 1849 | 1181 | 1897 | 1.57 | 1.63 | 1.52 | 0.97 | 384 | 252 | 390 |
| heapsort | 1050 | 747 | n/a | 1.41 | 1.50 | 1.33 | n/a | 221 | 167 | n/a |
| fannkuch | 1039 | 761 | 1062 | 1.37 | 1.46 | 1.31 | 0.98 | 218 | 166 | 222 |
| dijkstra_routes | 1169 | 857 | 1256 | 1.36 | 1.44 | 1.31 | 0.93 | 244 | 187 | 260 |
| cd | 252 | 186 | 107 | 1.35 | 1.82 | 1.14 | 2.35 | 60 | 52 | 32 |
| nbody | 342 | 278 | 341 | 1.23 | 1.49 | 1.09 | 1.00 | 77 | 71 | 76 |
| list | 94 | 88 | 89 | 1.07 | 2.35 | 0.90 | 1.06 | 25 | 28 | 25 |
| matmul | 420 | 397 | 274 | 1.06 | 1.20 | 0.97 | 1.53 | 94 | 97 | 65 |
| storage | 103 | 98 | 153 | 1.05 | 2.04 | 0.85 | 0.67 | 28 | 32 | 39 |
| fasta | 1277 | 1312 | n/a | 0.97 | 1.01 | 0.95 | n/a | 271 | 285 | n/a |
| mandelbrot | 534 | 569 | 522 | 0.94 | 1.02 | 0.90 | 1.02 | 115 | 127 | 111 |
| deltablue | 105 | 112 | 108 | 0.94 | 1.64 | 0.76 | 0.97 | 28 | 37 | 29 |
| fib | 640 | 702 | 475 | 0.91 | 0.98 | 0.88 | 1.35 | 138 | 156 | 104 |
| lua_lexer | 822 | 947 | 452 | 0.87 | 0.91 | 0.83 | 1.82 | 174 | 211 | 102 |
| table_array | 359 | 445 | n/a | 0.81 | 0.90 | 0.76 | n/a | 81 | 107 | n/a |
| json | 138 | 186 | 74 | 0.75 | 1.00 | 0.70 | 1.86 | 36 | 51 | 22 |
| events | 160 | 215 | 97 | 0.74 | 0.95 | 0.69 | 1.65 | 40 | 57 | 28 |
| lru_cache | 148 | 206 | 117 | 0.72 | 0.94 | 0.69 | 1.27 | 38 | 56 | 33 |
| raytrace | 95 | 142 | 40 | 0.67 | 1.01 | 0.65 | 2.36 | 26 | 40 | 14 |
| closures | 462 | 705 | n/a | 0.66 | 0.70 | 0.65 | n/a | 102 | 156 | n/a |
| expr_lang | 124 | 200 | 97 | 0.62 | 0.81 | 0.63 | 1.27 | 34 | 54 | 29 |
| towers | 46 | 80 | 23 | 0.58 | 1.43 | 0.58 | 1.99 | 16 | 27 | 10 |
| richards | 59 | 104 | 35 | 0.56 | 1.04 | 0.55 | 1.70 | 18 | 33 | 12 |
| bounce | 60 | 107 | 48 | 0.56 | 1.01 | 0.54 | 1.26 | 18 | 33 | 15 |
| havlak | 148 | 271 | 117 | 0.55 | 0.66 | 0.53 | 1.26 | 38 | 72 | 33 |
| spectral_norm | 182 | 378 | 171 | 0.48 | 0.55 | 0.49 | 1.06 | 44 | 90 | 41 |
| permute | 36 | 86 | 18 | 0.41 | 0.93 | 0.45 | 1.96 | 13 | 28 | 12 |
| table_hash | 500 | 1280 | 425 | 0.39 | 0.41 | 0.41 | 1.18 | 112 | 276 | 95 |
| knucleotide | 1034 | 3153 | 682 | 0.33 | 0.33 | 0.33 | 1.52 | 221 | 675 | 151 |
| sieve | 563 | 2079 | 1011 | 0.27 | 0.28 | 0.30 | 0.56 | 141 | 466 | 223 |
| pcall_error | 1471 | 5788 | n/a | 0.25 | 0.26 | 0.26 | n/a | 304 | 1178 | n/a |

Geomean ours/node: cycles 0.79, wall 0.75 (33 programs); ours/LuaJIT cycles 1.27 (28 programs).

## 20x workloads (run.sh -s 20; the 14 short programs)

Empty node script: 51 Mcyc, 20 ms (inside every node number). Ratios of cycles unless marked wall; above 1 = ours slower.

| program | ours Mcyc | node Mcyc | LuaJIT Mcyc | ours/node cyc | ours/node-ex cyc | ours/node wall | ours/LuaJIT cyc | ours ms | node ms | LuaJIT ms |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| classic_vector | 8816 | 969 | 4634 | 9.10 | 9.60 | 8.20 | 1.90 | 1772 | 216 | 942 |
| cd | 2549 | 532 | 1436 | 4.79 | 5.30 | 4.15 | 1.78 | 521 | 126 | 306 |
| storage | 2001 | 429 | 3052 | 4.67 | 5.30 | 3.72 | 0.66 | 412 | 111 | 620 |
| deltablue | 1182 | 306 | 561 | 3.87 | 4.64 | 3.18 | 2.11 | 246 | 77 | 121 |
| raytrace | 1057 | 295 | 671 | 3.58 | 4.32 | 2.90 | 1.58 | 220 | 76 | 142 |
| list | 1604 | 495 | 1729 | 3.24 | 3.61 | 2.90 | 0.93 | 329 | 113 | 352 |
| havlak | 2646 | 881 | 1702 | 3.00 | 3.19 | 2.63 | 1.55 | 537 | 204 | 350 |
| richards | 740 | 248 | 568 | 2.98 | 3.74 | 2.47 | 1.30 | 156 | 63 | 121 |
| bounce | 1004 | 416 | 905 | 2.41 | 2.75 | 2.14 | 1.11 | 209 | 98 | 189 |
| towers | 596 | 265 | 359 | 2.25 | 2.78 | 1.90 | 1.66 | 127 | 67 | 80 |
| permute | 635 | 311 | 243 | 2.04 | 2.44 | 1.77 | 2.61 | 136 | 77 | 58 |
| json | 1969 | 1109 | 1201 | 1.78 | 1.86 | 1.67 | 1.64 | 401 | 241 | 250 |
| events | 2945 | 1718 | 2754 | 1.71 | 1.77 | 1.65 | 1.07 | 599 | 364 | 560 |
| dijkstra_routes | 5237 | 3184 | 5513 | 1.64 | 1.67 | 1.61 | 0.95 | 1056 | 655 | 1111 |
| expr_lang | 1833 | 1286 | 1667 | 1.42 | 1.48 | 1.37 | 1.10 | 377 | 276 | 343 |
| lru_cache | 2077 | 1861 | 2816 | 1.12 | 1.15 | 1.07 | 0.74 | 427 | 398 | 570 |
| lua_lexer | 15611 | 14963 | 8837 | 1.04 | 1.05 | 1.03 | 1.77 | 3122 | 3045 | 1822 |

Geomean ours/node: cycles 2.54, wall 2.26 (17 programs); ours/LuaJIT cycles 1.35 (17 programs).
