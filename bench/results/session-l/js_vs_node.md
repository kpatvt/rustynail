# JS ports: ours vs node vs LuaJIT (subject 7413bff, 5.5 dev release build, JIT on)

node v26.13.1, min of 5 runs, taskset CPU 4, perf stat cycles:u. All 33 JS ports print exactly what the Lua programs print (run.sh checks before timing, 0 mismatches in both runs).

## Default workloads (run.sh)

Empty node script: 49 Mcyc, 26 ms (inside every node number). Ratios of cycles unless marked wall; above 1 = ours slower.

| program | ours Mcyc | node Mcyc | LuaJIT Mcyc | ours/node cyc | ours/node-ex cyc | ours/node wall | ours/LuaJIT cyc | ours ms | node ms | LuaJIT ms |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| classic_vector | 469 | 178 | 245 | 2.63 | 3.65 | 2.14 | 1.92 | 102 | 48 | 56 |
| binary_trees | 1166 | 719 | 2155 | 1.62 | 1.74 | 1.47 | 0.54 | 246 | 167 | 443 |
| queens | 1850 | 1174 | 1899 | 1.58 | 1.64 | 1.54 | 0.97 | 376 | 244 | 386 |
| cd | 254 | 176 | 108 | 1.44 | 2.01 | 1.22 | 2.36 | 60 | 49 | 31 |
| heapsort | 1057 | 754 | n/a | 1.40 | 1.50 | 1.34 | n/a | 221 | 164 | n/a |
| fannkuch | 1050 | 762 | 1075 | 1.38 | 1.47 | 1.34 | 0.98 | 219 | 163 | 222 |
| dijkstra_routes | 1159 | 848 | 1256 | 1.37 | 1.45 | 1.31 | 0.92 | 242 | 184 | 261 |
| nbody | 351 | 281 | 344 | 1.25 | 1.51 | 1.08 | 1.02 | 77 | 72 | 76 |
| matmul | 430 | 389 | 269 | 1.11 | 1.27 | 1.05 | 1.59 | 96 | 91 | 63 |
| lua_lexer | 1040 | 955 | 455 | 1.09 | 1.15 | 1.04 | 2.29 | 217 | 209 | 102 |
| list | 94 | 90 | 89 | 1.05 | 2.34 | 0.92 | 1.05 | 25 | 28 | 24 |
| storage | 101 | 100 | 153 | 1.01 | 2.00 | 0.82 | 0.66 | 27 | 33 | 39 |
| fasta | 1282 | 1316 | n/a | 0.97 | 1.01 | 0.95 | n/a | 270 | 284 | n/a |
| mandelbrot | 533 | 559 | 522 | 0.95 | 1.05 | 0.93 | 1.02 | 114 | 122 | 111 |
| fib | 630 | 703 | 469 | 0.90 | 0.96 | 0.90 | 1.34 | 136 | 151 | 102 |
| table_array | 360 | 416 | n/a | 0.87 | 0.98 | 0.81 | n/a | 80 | 99 | n/a |
| deltablue | 105 | 132 | 104 | 0.79 | 1.27 | 0.75 | 1.01 | 28 | 38 | 29 |
| expr_lang | 133 | 172 | 97 | 0.77 | 1.09 | 0.73 | 1.37 | 34 | 47 | 29 |
| json | 137 | 181 | 75 | 0.76 | 1.04 | 0.73 | 1.82 | 35 | 48 | 22 |
| events | 163 | 220 | 139 | 0.74 | 0.95 | 0.70 | 1.17 | 40 | 57 | 36 |
| raytrace | 101 | 144 | 40 | 0.70 | 1.07 | 0.68 | 2.49 | 27 | 40 | 14 |
| lru_cache | 147 | 214 | 101 | 0.69 | 0.89 | 0.69 | 1.45 | 38 | 55 | 28 |
| closures | 457 | 713 | n/a | 0.64 | 0.69 | 0.63 | n/a | 100 | 159 | n/a |
| towers | 46 | 83 | 23 | 0.55 | 1.37 | 0.56 | 1.97 | 15 | 27 | 10 |
| richards | 60 | 109 | 35 | 0.55 | 1.01 | 0.57 | 1.75 | 19 | 33 | 13 |
| havlak | 155 | 284 | 117 | 0.54 | 0.66 | 0.55 | 1.33 | 40 | 74 | 32 |
| bounce | 60 | 120 | 47 | 0.50 | 0.85 | 0.52 | 1.26 | 18 | 35 | 15 |
| spectral_norm | 180 | 377 | 171 | 0.48 | 0.55 | 0.50 | 1.05 | 44 | 87 | 40 |
| permute | 36 | 79 | 66 | 0.45 | 1.19 | 0.50 | 0.54 | 13 | 26 | 21 |
| table_hash | 506 | 1259 | 425 | 0.40 | 0.42 | 0.41 | 1.19 | 112 | 270 | 95 |
| knucleotide | 1036 | 3199 | 683 | 0.32 | 0.33 | 0.32 | 1.52 | 221 | 681 | 150 |
| sieve | 587 | 2105 | 1010 | 0.28 | 0.29 | 0.31 | 0.58 | 145 | 471 | 222 |
| pcall_error | 1431 | 5765 | n/a | 0.25 | 0.25 | 0.25 | n/a | 296 | 1169 | n/a |

Geomean ours/node: cycles 0.79, wall 0.77 (33 programs); ours/LuaJIT cycles 1.22 (28 programs).

## 20x workloads (run.sh -s 20; the 14 short programs)

Empty node script: 53 Mcyc, 20 ms (inside every node number). Ratios of cycles unless marked wall; above 1 = ours slower.

| program | ours Mcyc | node Mcyc | LuaJIT Mcyc | ours/node cyc | ours/node-ex cyc | ours/node wall | ours/LuaJIT cyc | ours ms | node ms | LuaJIT ms |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| classic_vector | 8612 | 965 | 4622 | 8.92 | 9.44 | 8.19 | 1.86 | 1728 | 211 | 935 |
| cd | 2871 | 533 | 1429 | 5.38 | 5.98 | 4.74 | 2.01 | 583 | 123 | 296 |
| storage | 1965 | 425 | 3023 | 4.63 | 5.29 | 3.75 | 0.65 | 403 | 107 | 612 |
| raytrace | 1160 | 296 | 672 | 3.91 | 4.77 | 3.27 | 1.73 | 240 | 73 | 142 |
| deltablue | 1187 | 311 | 584 | 3.81 | 4.60 | 3.24 | 2.03 | 246 | 76 | 125 |
| list | 1563 | 494 | 1728 | 3.16 | 3.55 | 2.88 | 0.90 | 320 | 111 | 350 |
| richards | 739 | 249 | 564 | 2.97 | 3.77 | 2.48 | 1.31 | 155 | 62 | 120 |
| havlak | 2504 | 851 | 1671 | 2.94 | 3.14 | 2.59 | 1.50 | 507 | 196 | 343 |
| bounce | 1002 | 416 | 906 | 2.41 | 2.76 | 2.19 | 1.11 | 208 | 95 | 187 |
| towers | 593 | 263 | 369 | 2.26 | 2.83 | 1.98 | 1.61 | 127 | 64 | 82 |
| permute | 632 | 311 | 171 | 2.03 | 2.45 | 1.81 | 3.69 | 134 | 74 | 41 |
| json | 1965 | 1097 | 1210 | 1.79 | 1.88 | 1.69 | 1.62 | 400 | 237 | 250 |
| events | 3019 | 1717 | 3029 | 1.76 | 1.81 | 1.69 | 1.00 | 611 | 361 | 613 |
| dijkstra_routes | 5034 | 3223 | 5302 | 1.56 | 1.59 | 1.53 | 0.95 | 1015 | 662 | 1068 |
| expr_lang | 1975 | 1285 | 1668 | 1.54 | 1.60 | 1.49 | 1.18 | 406 | 273 | 342 |
| lua_lexer | 19891 | 14920 | 9105 | 1.33 | 1.34 | 1.31 | 2.18 | 3964 | 3030 | 1888 |
| lru_cache | 1981 | 1825 | 2812 | 1.09 | 1.12 | 1.04 | 0.70 | 404 | 388 | 569 |

Geomean ours/node: cycles 2.60, wall 2.35 (17 programs); ours/LuaJIT cycles 1.39 (17 programs).
