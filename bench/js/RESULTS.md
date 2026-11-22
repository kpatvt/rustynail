# rnlua vs node vs LuaJIT

Environment: node v26.13.1; rnlua 0.0.2 (Lua 5.5, JIT on); LuaJIT 2.1.1794386141; runs=5 cpu=4. Measured with `bench/js/run.sh` (min of 5 runs, `taskset -c 4`, `RUSTYNAIL_SEED=0`,
`perf stat -e cycles:u,instructions:u`; Mcyc = millions of user cycles; the cycle counts of node include its helper
threads, which are pinned to the same core). rnlua = `target/release/rnlua` (dev release build, JIT on, generational GC),
Lua 5.5 semantics. LuaJIT runs the same `.lua` files; `n/a` = it cannot run the program (5.3+ syntax such as `//`) or
prints something different. Every JS port prints exactly what the Lua program prints (checked by `run.sh` before timing).
Ratios are rnlua/other cycles: above 1 means rnlua is slower. `node-ex` subtracts the cycles of an empty node script.
The "note" column says what the program mostly does (from its structure; nothing here was profiled).

Reproduce: `systemd-run --user --scope -q -p MemoryMax=8G -p MemorySwapMax=0 timeout 3000 bench/js/run.sh -o OUTDIR`
(default workloads) and `... run.sh -s 20 -o OUTDIR20` (20x workload for the short programs).

## 1. Default workloads (same size as the Lua benchmark programs)

Most of the AWFY-style programs run 15-60 ms at the default size, so in this table node's startup and interpreter warm-up
are a large part of its number and the ratio flatters node's *cold* behaviour: read table 2 for them.

Empty node script: 46 Mcycles, 0.027 s (included in every node number below; the `node-ex` ratio subtracts it).
| program | rnlua Mcyc | node Mcyc | LuaJIT Mcyc | rnlua/node | rnlua/node-ex | rnlua/LuaJIT | rnlua s | node s | LuaJIT s | note |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---|
| binary_trees | 1914 | 770 | 2172 | 2.49 | 2.64 | 0.88 | 0.398 | 0.184 | 0.448 |  |
| fib | 1232 | 698 | 470 | 1.76 | 1.89 | 2.62 | 0.257 | 0.153 | 0.103 | calls: pure recursion, integer add/compare	queens	array reads/writes (boolean tables) + recursion |
| queens | 1844 | 1179 | 1896 | 1.56 | 1.63 | 0.97 | 0.378 | 0.248 | 0.384 |  |
| list | 134 | 90 | 88 | 1.49 | 3.03 | 1.52 | 0.034 | 0.030 | 0.024 | calls + allocation: deep recursive Tak-like calls over linked 2-field objects, method call for length	cd	allocation (many tiny Vector2D/3D objects) + method calls + float math + red-black tree |
| cd | 274 | 191 | 108 | 1.44 | 1.89 | 2.54 | 0.064 | 0.054 | 0.032 |  |
| heapsort | 1051 | 740 | n/a | 1.42 | 1.52 | n/a | 0.221 | 0.164 | n/a | numeric array reads/writes, no allocation	fannkuch	small integer array reads/writes (permutation flips) |
| fannkuch | 1062 | 761 | 1061 | 1.40 | 1.49 | 1.00 | 0.220 | 0.165 | 0.222 |  |
| nbody | 351 | 277 | 340 | 1.27 | 1.52 | 1.03 | 0.077 | 0.070 | 0.076 | float field access on 5 objects, sqrt	matmul	nested float array access |
| matmul | 422 | 389 | 274 | 1.09 | 1.23 | 1.54 | 0.094 | 0.093 | 0.064 |  |
| raytrace | 141 | 133 | 40 | 1.06 | 1.63 | 3.50 | 0.036 | 0.040 | 0.014 | allocation (vec objects per op) + method calls + float math	storage	allocation: tree of small arrays, recursion |
| storage | 101 | 99 | 153 | 1.02 | 1.90 | 0.66 | 0.027 | 0.033 | 0.039 |  |
| mandelbrot | 532 | 561 | 522 | 0.95 | 1.03 | 1.02 | 0.112 | 0.127 | 0.111 | float arithmetic loop	fasta	LCG float math + 1-char string push/concat of 60-char lines |
| fasta | 1268 | 1395 | n/a | 0.91 | 0.94 | n/a | 0.268 | 0.303 | n/a |  |
| deltablue | 108 | 137 | 108 | 0.78 | 1.18 | 1.00 | 0.029 | 0.042 | 0.029 | method calls (class hierarchy, dynamic dispatch) + array push/splice, field access	expr_lang	string tokenizing (regex/find), AST allocation, recursive eval, hash lookups of env |
| expr_lang | 150 | 195 | 95 | 0.77 | 1.01 | 1.58 | 0.039 | 0.054 | 0.027 |  |
| json | 143 | 188 | 76 | 0.76 | 1.00 | 1.88 | 0.037 | 0.052 | 0.022 | per-character string ops (sub/charAt), small object allocation, string building	events	closure calls with varargs, string-key map lookups, array splice |
| events | 162 | 225 | 99 | 0.72 | 0.91 | 1.65 | 0.041 | 0.059 | 0.028 |  |
| lru_cache | 155 | 220 | 109 | 0.70 | 0.89 | 1.43 | 0.040 | 0.059 | 0.035 | hash map (string/int keys) + linked-list updates, closures	closures	calls: closure creation and call through upvalues, compose chains |
| closures | 485 | 707 | n/a | 0.69 | 0.73 | n/a | 0.106 | 0.157 | n/a |  |
| table_array | 355 | 538 | n/a | 0.66 | 0.72 | n/a | 0.080 | 0.126 | n/a | array push/pop/shift, ipairs/for-of, sort with comparator	havlak	allocation of small arrays/sets, object-keyed map lookups (Map/Set in JS), union-find calls |
| havlak | 160 | 269 | 120 | 0.59 | 0.71 | 1.33 | 0.042 | 0.073 | 0.033 |  |
| towers | 50 | 88 | 23 | 0.57 | 1.20 | 2.16 | 0.016 | 0.029 | 0.010 | method calls + field updates on linked disks, recursion	bounce	method calls + float/int field updates, small allocation per round |
| bounce | 60 | 111 | 48 | 0.54 | 0.92 | 1.27 | 0.018 | 0.034 | 0.015 |  |
| richards | 59 | 113 | 35 | 0.52 | 0.88 | 1.70 | 0.018 | 0.035 | 0.013 | method calls (polymorphic task run), field access, linked packets	permute	method calls + array swaps, recursion |
| permute | 43 | 86 | 67 | 0.50 | 1.08 | 0.64 | 0.015 | 0.029 | 0.021 |  |
| spectral_norm | 180 | 376 | 171 | 0.48 | 0.54 | 1.05 | 0.043 | 0.089 | 0.041 | float arithmetic with function calls in nested loops	table_hash	hash table of 200k string keys with string concatenation for each key; sparse integer keys |
| table_hash | 521 | 1376 | 437 | 0.38 | 0.39 | 1.19 | 0.116 | 0.296 | 0.098 |  |
| knucleotide | 1026 | 3565 | 685 | 0.29 | 0.29 | 1.50 | 0.220 | 0.762 | 0.152 | 1-8 char substring creation + hash counting of 5M keys (string interning/hash)	sieve	boolean array fill and strided writes |
| sieve | 571 | 2351 | 1011 | 0.24 | 0.25 | 0.57 | 0.141 | 0.528 | 0.224 |  |
| pcall_error | 1442 | 7856 | n/a | 0.18 | 0.18 | n/a | 0.298 | 1.588 | n/a | exception throw/catch (pcall vs try/catch): V8 exceptions are costly, Lua error is a longjmp-like unwind	 |

## 2. 20x workload (programs whose default run is under ~0.1 s; same argument passed to Lua, JS and LuaJIT)

Empty node script: 47 Mcycles, 0.027 s (included in every node number below; the `node-ex` ratio subtracts it).
| program | rnlua Mcyc | node Mcyc | LuaJIT Mcyc | rnlua/node | rnlua/node-ex | rnlua/LuaJIT | rnlua s | node s | LuaJIT s | note |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---|
| raytrace | 2017 | 311 | 672 | 6.49 | 7.64 | 3.00 | 0.412 | 0.080 | 0.143 | allocation (vec objects per op) + method calls + float math	storage	allocation: tree of small arrays, recursion |
| cd | 3221 | 546 | 1435 | 5.90 | 6.45 | 2.24 | 0.652 | 0.127 | 0.299 |  |
| list | 2508 | 497 | 1733 | 5.04 | 5.57 | 1.45 | 0.510 | 0.116 | 0.353 | calls + allocation: deep recursive Tak-like calls over linked 2-field objects, method call for length	cd	allocation (many tiny Vector2D/3D objects) + method calls + float math + red-black tree |
| storage | 2014 | 439 | 3031 | 4.59 | 5.14 | 0.66 | 0.412 | 0.113 | 0.613 |  |
| deltablue | 1227 | 320 | 556 | 3.83 | 4.49 | 2.21 | 0.253 | 0.080 | 0.122 | method calls (class hierarchy, dynamic dispatch) + array push/splice, field access	expr_lang	string tokenizing (regex/find), AST allocation, recursive eval, hash lookups of env |
| havlak | 2934 | 917 | 1764 | 3.20 | 3.37 | 1.66 | 0.594 | 0.214 | 0.362 |  |
| richards | 806 | 258 | 559 | 3.13 | 3.82 | 1.44 | 0.169 | 0.066 | 0.120 | method calls (polymorphic task run), field access, linked packets	permute	method calls + array swaps, recursion |
| permute | 774 | 310 | 172 | 2.50 | 2.94 | 4.50 | 0.162 | 0.077 | 0.044 |  |
| bounce | 1012 | 416 | 902 | 2.43 | 2.74 | 1.12 | 0.210 | 0.098 | 0.188 |  |
| towers | 637 | 268 | 361 | 2.37 | 2.87 | 1.76 | 0.136 | 0.067 | 0.080 | method calls + field updates on linked disks, recursion	bounce	method calls + float/int field updates, small allocation per round |
| expr_lang | 2641 | 1308 | 1697 | 2.02 | 2.09 | 1.56 | 0.539 | 0.281 | 0.348 |  |
| json | 2095 | 1161 | 1217 | 1.80 | 1.88 | 1.72 | 0.425 | 0.252 | 0.253 | per-character string ops (sub/charAt), small object allocation, string building	events	closure calls with varargs, string-key map lookups, array splice |
| events | 3017 | 1759 | 1711 | 1.72 | 1.76 | 1.76 | 0.613 | 0.372 | 0.351 |  |
| lru_cache | 2664 | 1913 | 2827 | 1.39 | 1.43 | 0.94 | 0.540 | 0.410 | 0.579 | hash map (string/int keys) + linked-list updates, closures	closures	calls: closure creation and call through upvalues, compose chains |

## Not ported

- `programs/oop_methods`, `programs/coroutines`: the Lua programs rely on 64-bit integer wrap-around (the savings balance
  grows 2%/100 iterations until it overflows int64; the coroutine checksum exceeds 2^63), so a double-based JS port cannot
  print the same output without BigInt in the hot loop, which would not be a fair comparison.
- `programs/metamethods`, `programs/varargs`: operator metamethods and `select('#', ...)`/`table.pack` have no JS equivalent
  that is not either a Proxy (unfair) or a different program.
- `programs/gc_churn` (weak tables, `collectgarbage`), `programs/string_ops`, `programs/json` (`string.format`/`gsub`/`%q`
  heavy), `programs/load_big` (uses `load`), `programs/loops_arith` (integer-width arithmetic): skipped as Lua specific.
- AWFY `sieve`, `queens`, `mandelbrot`, `nbody`: numeric duplicates of programs already ported from `bench/programs`.
- `apps/markdown`, most of `bench/corpus*` (middleclass, penlight, serpent, luaunit, ...): library code that leans on Lua string
  patterns, metatables and `load`; no faithful JS counterpart.

## Caveats

- `corpus/classic_vector`, `corpus/lua_lexer` and `corpus2/heldout/dijkstra_routes` are ported too (they were listed as not
  portable before): the vector operators are plain methods (no metamethods) and `is()` is `instanceof`; the lexer uses
  `charCodeAt`/`indexOf` and sticky regexes where Lua uses `string.byte`/`find` with patterns, and implements `%q` by hand
  (value returned in a field instead of a second return value); `dijkstra_routes` keeps 1-based node numbers and the
  hand-written heap. LuaJIT runs all three with identical output. `dijkstra_routes` scales as N^2, so `-s 20` means N=400.
- `lru_cache` has no `__index/__newindex/__call` in JS: the three entry points are plain methods (`get`, `put`, `call`); the
  closure/map/linked-list structure is unchanged. `coroutines`-style code is not included (see above).
- `pcall_error` throws strings that start with the same `pcall_error.lua:3: ` prefix Lua's `error` adds (the Lua program
  must be run as `rnlua pcall_error.lua` from `bench/programs`, which `run.sh` does) so that the printed lengths agree.
- Lua tables used as dynamic string-keyed or sparse integer maps are `Map` in JS (`table_hash`, `knucleotide`, `events`,
  `lru_cache`, `havlak`); tables used as arrays are arrays; objects are classes or object literals.
- Lua `%` (floored) is emulated where operands can be negative (`expr_lang`, `raytrace`); integer-vs-float differences do
  not occur in these workloads (all integer values stay below 2^53).
- The heapsort/binary_trees/... ports from before this change were only touched where needed (heapsort printed spaces
  instead of tabs).
- Single machine, single core pinned, no thermal control; JIT warm-up is part of every measurement for every engine.
