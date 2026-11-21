# JS ports: ours vs node vs LuaJIT (subject 22670a1, 5.5 dev release build, JIT on)

node v26.13.1, min of 5 runs, taskset CPU 4, perf stat cycles:u. All 30 JS ports print exactly what the Lua programs print (run.sh checks before timing, 0 mismatches in both runs).

## Default workloads (run.sh)

Empty node script: 48 Mcyc, 28 ms (inside every node number). Ratios of cycles unless marked wall; above 1 = ours slower.

| program | ours Mcyc | node Mcyc | LuaJIT Mcyc | ours/node cyc | ours/node-ex cyc | ours/node wall | ours/LuaJIT cyc | ours ms | node ms | LuaJIT ms |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| binary_trees | 1483 | 785 | 2178 | 1.89 | 2.01 | 1.64 | 0.68 | 311 | 190 | 452 |
| fib | 1240 | 707 | 472 | 1.75 | 1.88 | 1.66 | 2.62 | 259 | 156 | 105 |
| queens | 1842 | 1183 | 1908 | 1.56 | 1.62 | 1.51 | 0.97 | 377 | 250 | 388 |
| cd | 263 | 186 | 109 | 1.41 | 1.90 | 1.17 | 2.41 | 63 | 54 | 34 |
| heapsort | 1057 | 752 | n/a | 1.41 | 1.50 | 1.34 | n/a | 222 | 165 | n/a |
| fannkuch | 1073 | 769 | 1084 | 1.39 | 1.49 | 1.33 | 0.99 | 222 | 167 | 225 |
| nbody | 351 | 282 | 343 | 1.24 | 1.50 | 1.11 | 1.02 | 79 | 71 | 77 |
| matmul | 437 | 377 | 277 | 1.16 | 1.33 | 1.06 | 1.58 | 97 | 91 | 65 |
| list | 93 | 90 | 89 | 1.03 | 2.19 | 0.84 | 1.05 | 26 | 32 | 25 |
| storage | 102 | 101 | 154 | 1.01 | 1.92 | 0.78 | 0.66 | 28 | 36 | 40 |
| mandelbrot | 533 | 566 | 522 | 0.94 | 1.03 | 0.89 | 1.02 | 113 | 127 | 112 |
| fasta | 1286 | 1391 | n/a | 0.92 | 0.96 | 0.89 | n/a | 277 | 310 | n/a |
| deltablue | 110 | 140 | 109 | 0.79 | 1.19 | 0.70 | 1.01 | 30 | 43 | 30 |
| json | 141 | 189 | 76 | 0.75 | 1.00 | 0.70 | 1.86 | 38 | 54 | 22 |
| expr_lang | 141 | 191 | 100 | 0.74 | 0.99 | 0.68 | 1.41 | 38 | 56 | 30 |
| raytrace | 109 | 149 | 41 | 0.73 | 1.08 | 0.67 | 2.67 | 30 | 45 | 14 |
| events | 162 | 221 | 143 | 0.73 | 0.93 | 0.66 | 1.13 | 41 | 62 | 38 |
| closures | 483 | 714 | n/a | 0.68 | 0.73 | 0.65 | n/a | 106 | 163 | n/a |
| lru_cache | 149 | 220 | 117 | 0.68 | 0.86 | 0.64 | 1.27 | 39 | 61 | 32 |
| table_array | 361 | 552 | n/a | 0.65 | 0.72 | 0.62 | n/a | 83 | 134 | n/a |
| havlak | 159 | 253 | 120 | 0.63 | 0.77 | 0.60 | 1.33 | 44 | 73 | 36 |
| richards | 59 | 104 | 35 | 0.57 | 1.06 | 0.56 | 1.71 | 19 | 34 | 13 |
| towers | 49 | 87 | 24 | 0.56 | 1.24 | 0.55 | 2.04 | 16 | 30 | 11 |
| permute | 43 | 78 | 14 | 0.55 | 1.40 | 0.51 | 3.05 | 15 | 29 | 10 |
| bounce | 60 | 114 | 48 | 0.53 | 0.91 | 0.53 | 1.26 | 19 | 36 | 16 |
| spectral_norm | 179 | 364 | 171 | 0.49 | 0.57 | 0.49 | 1.05 | 44 | 89 | 42 |
| table_hash | 534 | 1407 | 432 | 0.38 | 0.39 | 0.40 | 1.24 | 124 | 313 | 100 |
| knucleotide | 1053 | 3632 | 688 | 0.29 | 0.29 | 0.29 | 1.53 | 225 | 772 | 151 |
| sieve | 584 | 2309 | 1022 | 0.25 | 0.26 | 0.28 | 0.57 | 144 | 517 | 226 |
| pcall_error | 1424 | 7918 | n/a | 0.18 | 0.18 | 0.18 | n/a | 300 | 1630 | n/a |

Geomean ours/node: cycles 0.75, wall 0.71 (30 programs); ours/LuaJIT cycles 1.32 (25 programs).

## 20x workloads (run.sh -s 20; the 14 short programs)

Empty node script: 53 Mcyc, 21 ms (inside every node number). Ratios of cycles unless marked wall; above 1 = ours slower.

| program | ours Mcyc | node Mcyc | LuaJIT Mcyc | ours/node cyc | ours/node-ex cyc | ours/node wall | ours/LuaJIT cyc | ours ms | node ms | LuaJIT ms |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| cd | 2943 | 550 | 1464 | 5.35 | 5.93 | 4.68 | 2.01 | 597 | 128 | 304 |
| storage | 2003 | 448 | 3045 | 4.48 | 5.08 | 3.54 | 0.66 | 411 | 116 | 616 |
| deltablue | 1244 | 321 | 574 | 3.88 | 4.65 | 3.15 | 2.17 | 258 | 82 | 125 |
| raytrace | 1193 | 311 | 675 | 3.83 | 4.62 | 3.11 | 1.77 | 247 | 79 | 143 |
| list | 1580 | 499 | 1750 | 3.16 | 3.54 | 2.83 | 0.90 | 324 | 114 | 356 |
| richards | 794 | 262 | 568 | 3.03 | 3.80 | 2.52 | 1.40 | 166 | 66 | 120 |
| havlak | 2807 | 948 | 1777 | 2.96 | 3.14 | 2.59 | 1.58 | 568 | 219 | 365 |
| permute | 777 | 322 | 193 | 2.41 | 2.89 | 2.08 | 4.03 | 164 | 78 | 46 |
| bounce | 1013 | 421 | 914 | 2.41 | 2.76 | 2.13 | 1.11 | 211 | 99 | 191 |
| towers | 602 | 272 | 361 | 2.21 | 2.75 | 1.85 | 1.67 | 128 | 69 | 80 |
| json | 2030 | 1167 | 1245 | 1.74 | 1.82 | 1.63 | 1.63 | 413 | 254 | 258 |
| events | 3006 | 1773 | 2791 | 1.70 | 1.75 | 1.62 | 1.08 | 610 | 377 | 566 |
| expr_lang | 2145 | 1329 | 1697 | 1.61 | 1.68 | 1.53 | 1.26 | 439 | 286 | 349 |
| lru_cache | 2049 | 1948 | 2818 | 1.05 | 1.08 | 1.02 | 0.73 | 426 | 416 | 570 |

Geomean ours/node: cycles 2.60, wall 2.28 (14 programs); ours/LuaJIT cycles 1.41 (14 programs).
