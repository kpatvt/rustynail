# JS ports: ours vs node vs LuaJIT (subject 5a064d7, 5.5 dev release build, JIT on)

node v26.13.1, min of 5 runs, taskset CPU 4, perf stat cycles:u. All 33 JS ports print exactly what the Lua programs print (run.sh checks before timing, 0 mismatches in both runs).

## Default workloads (run.sh)

Empty node script: 49 Mcyc, 28 ms (inside every node number). Ratios of cycles unless marked wall; above 1 = ours slower.

| program | ours Mcyc | node Mcyc | LuaJIT Mcyc | ours/node cyc | ours/node-ex cyc | ours/node wall | ours/LuaJIT cyc | ours ms | node ms | LuaJIT ms |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| classic_vector | 474 | 180 | 244 | 2.64 | 3.63 | 2.08 | 1.95 | 104 | 50 | 57 |
| queens | 1859 | 1182 | 1901 | 1.57 | 1.64 | 1.52 | 0.98 | 380 | 249 | 394 |
| binary_trees | 1194 | 790 | 2147 | 1.51 | 1.61 | 1.36 | 0.56 | 252 | 186 | 443 |
| heapsort | 1072 | 758 | n/a | 1.41 | 1.51 | 1.34 | n/a | 226 | 168 | n/a |
| fannkuch | 1052 | 767 | 1076 | 1.37 | 1.47 | 1.32 | 0.98 | 219 | 166 | 222 |
| dijkstra_routes | 1189 | 896 | 1280 | 1.33 | 1.40 | 1.27 | 0.93 | 248 | 196 | 267 |
| cd | 264 | 199 | 110 | 1.32 | 1.76 | 1.13 | 2.40 | 63 | 55 | 31 |
| nbody | 348 | 274 | 342 | 1.27 | 1.55 | 1.13 | 1.02 | 79 | 70 | 77 |
| matmul | 440 | 380 | 274 | 1.16 | 1.33 | 1.05 | 1.60 | 98 | 94 | 66 |
| list | 94 | 89 | 89 | 1.06 | 2.37 | 0.88 | 1.06 | 26 | 29 | 26 |
| storage | 101 | 96 | 155 | 1.05 | 2.15 | 0.88 | 0.66 | 29 | 33 | 40 |
| lua_lexer | 1107 | 1136 | 461 | 0.97 | 1.02 | 0.93 | 2.40 | 231 | 249 | 103 |
| mandelbrot | 533 | 566 | 522 | 0.94 | 1.03 | 0.89 | 1.02 | 114 | 128 | 112 |
| fasta | 1285 | 1404 | n/a | 0.92 | 0.95 | 0.89 | n/a | 271 | 304 | n/a |
| fib | 636 | 707 | 465 | 0.90 | 0.97 | 0.87 | 1.37 | 136 | 155 | 103 |
| table_array | 365 | 433 | n/a | 0.84 | 0.95 | 0.77 | n/a | 84 | 109 | n/a |
| raytrace | 107 | 138 | 41 | 0.78 | 1.21 | 0.68 | 2.61 | 29 | 42 | 15 |
| deltablue | 109 | 143 | 109 | 0.76 | 1.16 | 0.68 | 1.00 | 29 | 43 | 30 |
| events | 165 | 223 | 146 | 0.74 | 0.95 | 0.70 | 1.13 | 42 | 59 | 37 |
| json | 139 | 190 | 76 | 0.73 | 0.99 | 0.71 | 1.84 | 37 | 52 | 22 |
| lru_cache | 151 | 227 | 117 | 0.67 | 0.85 | 0.66 | 1.29 | 40 | 60 | 33 |
| closures | 470 | 719 | n/a | 0.65 | 0.70 | 0.64 | n/a | 102 | 160 | n/a |
| expr_lang | 137 | 213 | 98 | 0.64 | 0.84 | 0.63 | 1.40 | 36 | 58 | 28 |
| havlak | 163 | 275 | 119 | 0.59 | 0.72 | 0.58 | 1.37 | 44 | 77 | 35 |
| richards | 60 | 110 | 35 | 0.54 | 0.98 | 0.55 | 1.71 | 19 | 34 | 13 |
| bounce | 61 | 115 | 48 | 0.53 | 0.93 | 0.52 | 1.26 | 18 | 34 | 15 |
| towers | 47 | 90 | 23 | 0.53 | 1.16 | 0.55 | 2.03 | 16 | 29 | 11 |
| spectral_norm | 182 | 381 | 171 | 0.48 | 0.55 | 0.49 | 1.06 | 44 | 90 | 41 |
| permute | 36 | 87 | 11 | 0.42 | 0.96 | 0.44 | 3.21 | 13 | 29 | 8 |
| table_hash | 548 | 1387 | 430 | 0.39 | 0.41 | 0.41 | 1.27 | 122 | 299 | 98 |
| knucleotide | 1051 | 3686 | 692 | 0.29 | 0.29 | 0.29 | 1.52 | 223 | 782 | 151 |
| sieve | 582 | 2324 | 1017 | 0.25 | 0.26 | 0.28 | 0.57 | 146 | 523 | 227 |
| pcall_error | 1422 | 7864 | n/a | 0.18 | 0.18 | 0.18 | n/a | 298 | 1615 | n/a |

Geomean ours/node: cycles 0.77, wall 0.73 (33 programs); ours/LuaJIT cycles 1.31 (28 programs).

## 20x workloads (run.sh -s 20; the 14 short programs)

Empty node script: 50 Mcyc, 21 ms (inside every node number). Ratios of cycles unless marked wall; above 1 = ours slower.

| program | ours Mcyc | node Mcyc | LuaJIT Mcyc | ours/node cyc | ours/node-ex cyc | ours/node wall | ours/LuaJIT cyc | ours ms | node ms | LuaJIT ms |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| classic_vector | 8851 | 1028 | 4656 | 8.61 | 9.05 | 7.89 | 1.90 | 1777 | 225 | 942 |
| cd | 2911 | 559 | 1437 | 5.21 | 5.72 | 4.52 | 2.03 | 606 | 134 | 305 |
| storage | 2017 | 434 | 3067 | 4.65 | 5.26 | 3.55 | 0.66 | 422 | 119 | 624 |
| raytrace | 1175 | 310 | 675 | 3.78 | 4.52 | 3.12 | 1.74 | 244 | 78 | 143 |
| deltablue | 1221 | 342 | 572 | 3.57 | 4.19 | 2.98 | 2.13 | 254 | 85 | 124 |
| list | 1582 | 500 | 1740 | 3.16 | 3.52 | 2.82 | 0.91 | 325 | 115 | 356 |
| richards | 806 | 263 | 567 | 3.06 | 3.78 | 2.48 | 1.42 | 171 | 69 | 123 |
| havlak | 2757 | 940 | 1769 | 2.93 | 3.10 | 2.53 | 1.56 | 578 | 228 | 382 |
| bounce | 1011 | 422 | 913 | 2.40 | 2.72 | 2.12 | 1.11 | 211 | 99 | 191 |
| towers | 590 | 270 | 357 | 2.19 | 2.69 | 1.84 | 1.65 | 126 | 68 | 80 |
| permute | 632 | 317 | 191 | 1.99 | 2.37 | 1.73 | 3.31 | 136 | 79 | 47 |
| json | 2014 | 1170 | 1235 | 1.72 | 1.80 | 1.62 | 1.63 | 420 | 259 | 260 |
| events | 3012 | 1766 | 2735 | 1.71 | 1.76 | 1.62 | 1.10 | 620 | 382 | 555 |
| dijkstra_routes | 5372 | 3283 | 5450 | 1.64 | 1.66 | 1.60 | 0.99 | 1080 | 674 | 1098 |
| expr_lang | 2072 | 1343 | 1706 | 1.54 | 1.60 | 1.48 | 1.21 | 425 | 288 | 350 |
| lua_lexer | 21692 | 17779 | 8995 | 1.22 | 1.22 | 1.20 | 2.41 | 4328 | 3607 | 1852 |
| lru_cache | 2132 | 1961 | 2836 | 1.09 | 1.12 | 1.04 | 0.75 | 435 | 418 | 575 |

Geomean ours/node: cycles 2.55, wall 2.27 (17 programs); ours/LuaJIT cycles 1.43 (17 programs).
