# Study: the runtime gap on interpreter-shaped programs (2026-11-24, base d7bab19, LuaJIT 2.1.1795119671)

Question: on the programs whose time is not trace code (text_adventure, lua_interp, parser_combinator, json_rxi, inspect_lua, the coroutine
programs, the `lib/` string and table micro programs), which runtime operations are slower than LuaJIT's, by how many cycles per operation, how
often do the programs execute them, and what is each one worth (the measured ceiling)? Then build the fixes in order of measured ceiling.
What was built is docs/JIT_TRACE.md section 54. Tags: **[M]** measured here, **[D]** arithmetic on [M] numbers as stated.

## 0. Answer in one page

1. **text_adventure's gap is not in the library functions.** Doubling each operation inside the program on both engines [M, section 3]:
   `gmatch` loops cost us 642 cycles per command against LuaJIT's 624, `indexOf` (an `ipairs` scan), the `counts` update, a vararg method call and
   a new table with three appends cost the same or less than on LuaJIT. The measurable per-operation gaps are `string.format` (+141 Mcyc,
   5.4 % of the program; against LuaJIT's *interpreter* we are equal), the rest of `parse` (+134), a vararg call with `select('#', ...)` (+115),
   `gmatch` with other inputs (+111), concatenation (+60), the log rotation (+33), `lower` / `upper` (+28): about 620 of the 1420 Mcyc gap.
   The rest is LuaJIT running the whole program as stitched traces (`luajit -jv`: it stitches around `gmatch_aux` / `gmatch` / `table.sort` and
   compiles everything else), while 33 % of our cycles are tier-1 template code and 13 % the call protocol. STUDY_WHY_GAP C5's "+535 Mcyc in
   library helpers" was a bucket share; in context, our helpers are as fast as LuaJIT's interpreter's.
2. **Per operation, our tier 1 is at or below LuaJIT's interpreter for most operations** (`byte`, `sub`, `lower`, `concat`, `format`, `ipairs`
   over long arrays, method calls, string-key updates, table constructors, closures: 0.4-0.95x of `luajit -joff`) [M, section 2]. The exceptions
   are **`pairs` / `next` (2.1-2.9x)**, **coroutine resume + yield (1.31x)**, `gmatch` (1.31-1.36x), `table.concat` (1.31-1.45x), `select('#')`
   in a vararg function (1.58x), `tostring` (1.24x), positional `table.insert` (1.24x). Against LuaJIT's JIT the gaps are larger wherever LuaJIT
   compiles the library call into the trace (`format` 2.1-4.5x, `tostring` 23x, `table.concat` 2-3x, `find` plain 2.3x, `sub` 1.9x).
3. **Across the 40 programs with a cycle gap above 1.4x** (IBS cycle profiles, weighted by each program's cycles [M, section 4]): trace code
   6712 Mcyc, tier-1 code 4615, the interpreter 1349 (almost all coroutine bodies: co_yield, coroutines, coro_scheduler), the coroutine switch
   (`coro_direct`) 590, `jit_call` 636, string creation from traces (`short_core`, `jit_tr_lib_sub`, `new_str`, `push_bytes`) about 1260, table
   insertion (`newkey`, `resize`, `jit_set`, `finish_set`, `setnodevector`, ...) about 1100. Inside tier-1 code (a per-bytecode perf map, scratch
   build) `CALL` / `RETURN*` / `TAILCALL` are the largest templates: parser_combinator 26 %, lua_interp 38 %, json_rxi 20 % of the whole program.
4. **Built** (docs/JIT_TRACE.md 54): R1 SSE2 plain substring search (`find_plain` -7.9 % cycles), R2 tier-1 generic-`for` paths (knob `t1g`),
   R5 word-load comparison in the string-table probes (`lib/sub` -6.8 %, knucleotide -1.7 %), R6 inline native calls with all results wanted and
   light C tail calls (-5..-8 % of such a call). All four against the control, 44 programs: **5.5 geomean -0.97 % cycles, -0.51 % instructions;
   5.4 -1.08 % / -0.56 %**; inspect_lua -3.9 / -5.3 %, parser_combinator -4.8 / -0.6 %, text_adventure -1.2 / -3.5 %, coroutines -3.7 / -7.4 %.
   No program over +1 % instructions; three cycle misses at equal or fewer instructions (5.5 lua_parser +2.2 %, 5.4 events +3.7 %: binary and
   tier-1 code layout, docs/JIT_TRACE.md 54.6). **Measured ceilings realised 0.14-0.5**: the per-operation gaps are spread over call overheads
   that no single helper owns.
5. **Not built, with the measurement that says why**: entering tier 1 at the resume point of a coroutine (the obvious fix for 1900 Mcyc of
   interpreter + switch) made co_yield +16 % and coroutines +20 % slower (the tier-1 entry costs 35 cycles per resume, more than interpreting the
   five bytecodes); the E9 return cells (`t1call=1`) and the lean C call (`t1ccall=1`) move the interpreter-shaped programs by -1..+1 % (formation
   noise), `table.concat` and `tostring` are executed too rarely (text_adventure: 224 k and 36 k calls, ceiling 0.7 %), `table.move` into a fresh
   table is 1.11x LuaJIT's (ceiling 9 Mcyc on tremove_pop, whose real loss is a formation one: the inner loop's trace is flushed by the nest rule
   because the outer loop's `table.move` of 50 elements cannot be recorded or stitched).

## 1. Method

* Binaries: `cargo build --release -p rn-cli` (5.5, plain, not PGO), copied to equal-length directories (`bin/b00`, `bin/r01`, ...) so that
  `argv[0]` has the same length. `RUSTYNAIL_SEED=0`. LuaJIT: `/usr/bin/luajit`. Cycles and instructions: `perf stat -e cycles:u,instructions:u`,
  `taskset -c 4`, a 300 ms spin on CPU 4 before each run, min of 3-7 runs, nothing else on CPU 4 or its SMT sibling 12 during timing.
* **Per-operation microbenchmarks** (section 2): one Lua file per operation, `for i = 1, N do local k = i % 64 + 1; <op> end` with 64 varying
  operands and results kept alive (`ring[k] = r` or summed into `acc`). Cost per operation = (cycles(N = 1.2 M) - cycles(N = 0.2 M)) / 1 M (start-up
  and compilation cancel). Five engine modes: ours with JIT and traces (`rn`), tier 1 only (`RUSTYNAIL_JIT_TRACE=off`, `rnt1`), interpreter
  (`RUSTYNAIL_JIT=off`, `rni`), LuaJIT, `luajit -joff`. The same Lua source on both engines.
* **In-context doubling** (section 3): text_adventure with one operation added once per command (same inputs, or other inputs where LuaJIT's
  trace could CSE a pure operation: LuaJIT folds a repeated `string.format` / `lower` of the same operands, so the first series' `format` and
  `lower` rows under-count LuaJIT; series B uses another format string and `upper`). Delta cycles = cost of 800 k operations in the program's own
  surroundings, on both engines. This is the doubling method of docs/STUDY_CAUSAL_GAP.md (realised ~0.95 of prediction in its backtest).
* **Counts**: a `debug.sethook(.., "c")` counter (C functions by name, Lua functions by line) with JIT off; the per-command structure of
  text_adventure for the Lua-level operations.
* **Profiles**: AMD IBS op sampling in cycle mode (`perf record -e ibs_op/cnt_ctl=0,swfilt=1/u -c 50000`, exact IPs), `RUSTYNAIL_JIT_PERFMAP=2`,
  pinned to a CPU away from the timing CPU. The per-opcode split of tier-1 code (section 4.2) comes from a scratch build that registers one
  perf symbol per bytecode's template range (`t1op:<opcode>`; not committed): cold stubs placed after a template are counted with it, so the
  split is approximate.

## 2. Per-operation costs, ours against LuaJIT [M]

Cycles / instructions per operation. `tier 1 / -joff` compares our tier 1 with LuaJIT's interpreter (what an interpreter-shaped program pays);
`JIT / LuaJIT` compares the full engines on a loop both trace.

| op | ours JIT | ours tier 1 | ours interp | LuaJIT | LuaJIT -joff | tier 1 / -joff | JIT / LuaJIT |
|---|---:|---:|---:|---:|---:|---:|---:|
| byte | 4.3 / 32 | 60.5 / 390 | 78.9 / 513 | 4.0 / 18 | 87.6 / 325 | 0.69 | 1.07 |
| closure | 106.3 / 602 | 103.5 / 594 | 108.5 / 651 | 159.2 / 813 | 158.1 / 813 | 0.65 | 0.67 |
| concat2 | 85.4 / 379 | 101.7 / 533 | 115.0 / 637 | 61.8 / 224 | 112.9 / 445 | 0.90 | 1.38 |
| concat3 | 93.7 / 438 | 109.9 / 606 | 126.2 / 712 | 62.6 / 262 | 140.0 / 531 | 0.79 | 1.50 |
| coyield (resume + yield) | 126.8 / 717 | 116.6 / 717 | 145.6 / 802 | 67.5 / 407 | 89.0 / 377 | 1.31 | 1.88 |
| empty loop | 1.5 / 11 | 7.7 / 57 | 25.0 / 148 | 2.8 / 4 | 18.3 / 87 | 0.42 | 0.54 |
| findplain | 36.8 / 246 | 83.3 / 524 | 101.4 / 678 | 16.3 / 101 | 92.3 / 386 | 0.90 | 2.26 |
| findplain2 (varying needle) | 50.1 / 293 | 94.4 / 588 | - | 28.2 / 145 | 158.4 / 480 | 0.60 | 1.78 |
| findshort | 33.1 / 224 | 77.7 / 511 | - | 16.9 / 101 | 99.9 / 399 | 0.78 | 1.96 |
| fmt0 (no `%`) | 139.0 / 722 | 139.4 / 720 | 169.1 / 824 | 62.3 / 342 | 156.3 / 540 | 0.89 | 2.23 |
| fmt1s | 139.8 / 733 | 148.8 / 857 | 175.8 / 978 | 66.2 / 233 | 156.2 / 799 | 0.95 | 2.11 |
| fmt2s | 148.0 / 804 | 163.7 / 932 | - | 63.8 / 222 | 175.0 / 870 | 0.94 | 2.32 |
| fmt3s | 181.3 / 1097 | 203.1 / 1239 | 239.7 / 1394 | 100.2 / 362 | 307.3 / 1402 | 0.66 | 1.81 |
| fmtd | 134.6 / 577 | 144.3 / 698 | - | 29.8 / 151 | 119.1 / 559 | 1.21 | 4.52 |
| gmatch1 (1 word) | 411.6 / 2154 | 407.1 / 2153 | - | 278.2 / 1544 | 301.1 / 1458 | 1.35 | 1.48 |
| gmatch3 | 703.8 / 3958 | 725.0 / 4005 | 762.8 / 4147 | 533.1 / 3015 | 551.9 / 2793 | 1.31 | 1.32 |
| gmatch6 | 1274.0 / 6780 | 1281.8 / 6779 | - | 908.5 / 5208 | 942.8 / 4787 | 1.36 | 1.40 |
| ipairs12 | 66.6 / 483 | 211.1 / 1519 | - | 54.6 / 252 | 360.7 / 1338 | 0.59 | 1.22 |
| ipairs3 | 53.2 / 342 | 132.6 / 882 | - | 27.1 / 135 | 113.1 / 519 | 1.17 | 1.96 |
| ipairsfind (indexOf of 4) | 34.4 / 203 | 207.5 / 1260 | 359.8 / 1733 | 161.7 / 487 | 224.1 / 657 | 0.93 | 0.21 |
| lower | 84.3 / 422 | 99.7 / 558 | 110.3 / 662 | 61.4 / 219 | 118.4 / 396 | 0.84 | 1.37 |
| mathmax | 2.5 / 18 | 49.7 / 335 | - | 3.2 / 13 | 39.0 / 169 | 1.27 | 0.78 |
| methcall | 2.0 / 15 | 66.0 / 437 | 91.2 / 492 | 3.0 / 4 | 70.6 / 370 | 0.93 | 0.67 |
| newtab3 ({} + 3 appends) | 225.0 / 1410 | 346.2 / 2215 | 366.4 / 2370 | 303.8 / 1544 | 415.8 / 2150 | 0.83 | 0.74 |
| newtabh (3-field record) | 79.3 / 483 | 176.8 / 1186 | 158.6 / 983 | 147.7 / 809 | 196.9 / 982 | 0.90 | 0.54 |
| next3 | 64.0 / 432 | 144.4 / 1008 | - | 45.6 / 178 | 68.9 / 302 | 2.10 | 1.40 |
| pairs0 (empty table) | 102.6 / 732 | 121.3 / 732 | - | 7.0 / 30 | 47.0 / 207 | 2.58 | 14.66 |
| pairs12 | 146.0 / 1073 | 541.4 / 3302 | - | 153.7 / 418 | 184.5 / 783 | 2.93 | 0.95 |
| pairs3 | 65.1 / 434 | 217.3 / 1367 | 303.9 / 1620 | 45.6 / 178 | 78.1 / 345 | 2.78 | 1.43 |
| selectn (`select('#', ...)`, tail call) | 1.5 / 11 | 153.9 / 828 | - | 3.0 / 4 | 97.4 / 390 | 1.58 | 0.50 |
| sort3 | 292.0 / 1584 | 290.2 / 1583 | 300.5 / 1645 | 271.0 / 1353 | 304.5 / 1478 | 0.95 | 1.08 |
| strkeyinc | 10.4 / 70 | 65.1 / 337 | 91.5 / 522 | 4.5 / 29 | 94.4 / 264 | 0.69 | 2.31 |
| sub | 36.6 / 257 | 80.9 / 521 | 102.7 / 659 | 19.7 / 131 | 93.5 / 344 | 0.87 | 1.86 |
| tconcat1 | 123.5 / 693 | 124.5 / 692 | - | 39.1 / 230 | 94.5 / 459 | 1.32 | 3.16 |
| tconcat3 | 163.4 / 964 | 164.4 / 963 | 182.9 / 1083 | 80.3 / 402 | 125.7 / 631 | 1.31 | 2.03 |
| tconcat12 | 399.5 / 2214 | 393.0 / 2213 | - | 241.8 / 1109 | 271.1 / 1338 | 1.45 | 1.65 |
| tinsert | 28.5 / 181 | 79.5 / 468 | - | 24.4 / 115 | 96.3 / 466 | 0.83 | 1.17 |
| tinsertpos | 141.7 / 861 | 141.9 / 861 | - | 60.2 / 412 | 114.7 / 563 | 1.24 | 2.35 |
| tmove50 (into `{}`) | 1478.8 / 9073 | 1464.3 / 9065 | 1486.0 / 9157 | 1317.7 / 6469 | 2044.6 / 9397 | 0.72 | 1.12 |
| tostr (of a string) | 61.0 / 386 | 61.2 / 384 | 74.9 / 495 | 2.6 / 17 | 49.3 / 206 | 1.24 | 23.5 |
| tremove1 (append + remove(t, 1)) | 31.2 / 214 | 91.6 / 560 | 117.7 / 738 | 16.0 / 113 | 169.9 / 704 | 0.54 | 1.95 |
| tremovepop | 23.9 / 159 | 79.7 / 499 | 106.3 / 680 | 14.1 / 85 | 90.8 / 400 | 0.88 | 1.70 |
| upper | 83.8 / 422 | 98.8 / 558 | - | 62.6 / 218 | 116.5 / 395 | 0.85 | 1.34 |
| varcall (vararg Lua call) | 1.4 / 11 | 76.8 / 484 | 123.6 / 722 | 3.2 / 10 | 63.3 / 252 | 1.21 | 0.44 |
| varfwd (`return fmt(x, ...)`) | 259.4 / 1389 | 255.2 / 1386 | 285.4 / 1547 | 1.4 / 10 | 179.0 / 906 | 1.43 | (LuaJIT folds) |

Readings: our tier 1 is a fast interpreter for plain bytecode (empty loop 0.42x, method call 0.93x, closure 0.65x) and for most string functions;
the C-call protocol from tier 1 costs about 10 cycles more than LuaJIT's (`mathmax` 49.7 against 39.0). Where LuaJIT's JIT compiles a library call
into the trace (`format`, `tostring`, `table.concat`, `remove`, `find`, `sub`), a program whose loop we keep in tier 1 pays 2-20x.

**pairs / next in tier 1** (IBS, `pairs12`, 541 cycles for one `pairs` call and 13 iterations): `jit_tforcall` 37.5 %, `next_hinted` 21.6 %, the
loop's template code 27.9 %; for an empty table (`pairs0`, 121 cycles) the `pairs` call itself: `jit_call` 15 %, the general `post_call` 13.6 % (the
5.5 generic `for` asks for four results, which the inline result move did not handle), `base_pairs` 12 %, `get_metafield` 5 %, `jit_close` +
`State::close` 9.5 % (every 5.5 / 5.4 generic `for` ends with a `CLOSE`). These are the pieces R2 changed.

**Coroutine resume + yield** (co_yield, 121 cycles per round trip): the coroutine's body runs in the interpreter (`run_mode` 53 %), the fused
switch `coro_direct` 24 %, `jit_call` 12.7 %. LuaJIT's interpreter does the round trip in 89 cycles.

## 3. text_adventure in context [M]

Doubling (section 1); delta Mcyc for 800 k extra operations (cycles per operation = delta x 1.25). `rnt1` = ours in tier 1 only.

Series A (base binary):

| variant (added once per command) | ours | LuaJIT | LuaJIT -joff | ours - LuaJIT |
|---|---:|---:|---:|---:|
| `"go " .. dir` | +148 | +88 | +121 | +60 |
| `counts[verb]` update | +23 | +42 | -12 | -19 |
| a whole `exec("look")` | +2449 | +1226 | +2016 | +1222 |
| `string.format(fmt, ...)` (same operands: LuaJIT CSEs) | +106 | +16 | +161 | (+89) |
| `for w in line:gmatch("%a+")` | +514 | +499 | +412 | +14 |
| `indexOf(inv, "zzz")` | +105 | +126 | +97 | -21 |
| log rotation (append + `remove(log, 1)`) | +54 | +21 | +156 | +33 |
| `line:lower()` (same operand: LuaJIT CSEs) | +34 | +2 | +343 | (+32) |
| `math.max(1, math.floor(N / 8))` | +71 | +147 | +63 | -76 |
| `{}` | +26 | +64 | +136 | -38 |
| empty vararg method call | +13 | +26 | +73 | -13 |
| a whole `parse(line)` | +1045 | +644 | +788 | +401 |
| a whole `say(...)` | +362 | +205 | +529 | +158 |
| a table store (control) | -4 | -8 | -4 | +4 |
| (base run, Mcyc) | 2652 | 1192 | 2204 | |

Series B (R2 binary; other operands where LuaJIT could CSE):

| variant | ours | ours tier 1 | LuaJIT | LuaJIT -joff | ours - LuaJIT |
|---|---:|---:|---:|---:|---:|
| `{}` + three appends | +401 | +374 | +392 | +467 | +9 |
| `string.format(FMT2[fmt], ...)` (another format string) | +266 | +224 | +125 | +244 | **+141** |
| `for w in line:gmatch("%a+") do end` | +535 | +587 | +424 | +398 | **+111** |
| three `verbs` / `directions` lookups | +48 | +13 | +12 | +23 | +37 |
| `parse` without `lower` / `gmatch` (a method returning two values, a table, lookups) | +385 | +352 | +251 | +313 | **+134** |
| vararg method call doing `select('#', ...)` | +148 | +148 | +33 | +82 | **+115** |
| `line:upper()` | +137 | +132 | +109 | +178 | +28 |
| (base run, Mcyc) | 2564 | 2838 | 1187 | 2197 | |

The `exec("look")` row is a whole command (pairs over the exits, `table.sort`, two `table.concat`, a 3-argument `format`, `say`): 3061 cycles per
look against LuaJIT's 1532; looks are 10 % of the commands, so 122 Mcyc of the real program [D]. The operation rows sum to about 620 Mcyc of
the 1420 Mcyc gap [D]; nothing in them points at a library function that is slow in itself (`format`: ours 333 cycles per call in context, LuaJIT
-joff 305, LuaJIT's JIT 156 because the trace compiles it).

## 4. Where the runtime time is across the gap programs [M]

### 4.1 Aggregate over the 40 programs with plain / LuaJIT cycles > 1.4 (session-m)

IBS cycle shares of each program x its cycles (Mcyc), summed (binary of R2):

| symbol | Mcyc | | symbol | Mcyc |
|---|---:|---|---|---:|
| trace code | 6712 | | `table::resize` | 228 |
| tier-1 code | 4615 | | `MatchState::cm` | 216 |
| interpreter `run_mode` | 1349 | | `str_find_aux` | 209 |
| `jit_call` | 636 | | `jit_set` | 208 |
| `coro_direct` | 590 | | `State::ccall` | 195 |
| `short_core` (trace string creation) | 489 + 100 | | `jit_tforcall` | 172 |
| unknown (libc, vdso) | 475 | | `patc::lookup` | 169 |
| trace stubs | 338 | | `list_remove_after` (GC) | 160 |
| `jit_tr_lib_sub` | 332 | | `tconcat` | 146 + 55 |
| `jit_tr_alloc` | 315 | | `push_bytes` | 136 |
| `Heap::new_str` | 306 | | `finish_set` | 124 |
| `table::newkey` | 286 | | `jit_newtable` | 114 |

The interpreter and `coro_direct` rows are the coroutine programs (co_yield: interpreter 51 %, coroutines 40 %, coro_scheduler 29 %) and
inspect_lua (9 %). Per program (top symbols): survey output in the reproduction section.

### 4.2 Tier-1 code by opcode (scratch per-bytecode perf map, % of the whole program)

| program | CALL | RETURN | RETURN0 | RETURN1 | TAILCALL | GETFIELD | GETTABLE | others |
|---|---:|---:|---:|---:|---:|---:|---:|---|
| text_adventure | 4.9 | 4.8 | 2.7 | 0.5 | 0.4 | 4.3 | 2.3 | LEN 1.5, GETTABUP 1.3, SETTABLE 1.1, SELF 1.1, TFORCALL 1.0 |
| parser_combinator | 5.0 | 8.9 | 8.0 | | | | | (the rest of tier 1 is one unsplit `.v4` version: 12.7) |
| json_rxi | 6.5 | | 6.6 | | 1.7 | | | MOVE 1.5 |
| apps/events | 6.1 | 4.7 | 3.1 | | | 4.6 | 4.3 | GETUPVAL 2.5, FORLOOP 2.4, MODK 1.9 |
| real/lua_interp | 9.1 | 13.4 | 5.0 | 3.9 | 6.4 | 0.9 | | GETI 3.4 |

The call / return templates dominate the tier-1 share. In parser_combinator the hottest return site is the result copy loop of `RETURN` (two
results, the trip count is `min(results, wanted)` and depends on the caller): about 0.6 % of the program stalls at its head [M, per-address IBS].
The E9 return cells (`t1call=1`, which replace the caller walk of the return stub) do not move these programs (text_adventure +0.4 %,
parser_combinator +0.6 %, json_rxi -0.4 %, lua_interp +2.0 %, events -2.6 %, inspect_lua -1.0 % cycles, same binary): the walk is not the cost.

## 5. Ranked table: measured ceilings

Ceiling = (ours - LuaJIT) cycles per operation x the program's count of the operation; the "LuaJIT" column is the engine mode that matches how the
program runs it (in-context doubling for text_adventure; the JIT column of section 2 for a loop both engines trace).

| # | operation | ours cyc/op | LuaJIT cyc/op | count (program) | ceiling Mcyc | % of program | evidence |
|---|---|---:|---:|---|---:|---:|---|
| 1 | coroutine resume + yield round trip | 127 | 67.5 | 1.49 M (co_yield); ~10 M (coroutines) | 89; ~600 [D] | 49 %; ~40 % | micro + IBS; coro_scheduler 39 % interp + switch |
| 2 | plain `string.find` (in a trace) | 36.8 | 16.3 | 2.94 M (find_plain) | 60 | 49 % | micro; memfind 45 % of find_plain [IBS] |
| 3 | `string.format` in tier 1 | 333 | 156 | 0.80 M (text_adventure) | 141 | 5.4 % | doubling B (LuaJIT -joff 305: no gap to its interpreter) |
| 4 | `parse` minus lower / gmatch (call returning 2, table, lookups) | 481 | 314 | 0.80 M | 134 | 5.1 % | doubling B |
| 5 | vararg call + `select('#', ...)` | 185 | 41 | 0.80 M | 115 | 4.4 % | doubling B; micro `selectn` tier 1 154 vs -joff 97 |
| 6 | `gmatch` loop of a command | 669 | 530 | 0.80 M | 111 | 4.3 % | doubling B (series A: +14) |
| 7 | string creation by `string.sub` in a trace | 36.6 | 19.7 | 6.25 M (json_rxi), 2.55 M (lua_lexer) | 106, 43 | 5.9 %, 5.3 % | micro; `short_core` + `jit_tr_lib_sub` 820 Mcyc over the 40 programs |
| 8 | `pairs` call + iterations in tier 1 | 217 (3 keys) | 78 (-joff) | 0.44 M pairs (lua_parser), 0.10 M (text_adventure) | ~60, ~14 [D] | ~5 %, 0.5 % | micro + IBS |
| 9 | concatenation of 2 parts | 185 | 110 | 0.80 M | 60 | 2.3 % | doubling A |
| 10 | log rotation (`t[#t+1]`, `remove(t, 1)`) | 68 | 26 | 0.80 M | 33 | 1.3 % | doubling A |
| 11 | `table.concat` of 3 | 164 | 80 | 0.22 M (text_adventure) | 19 | 0.7 % | micro |
| 12 | `table.move` of 50 into `{}` | 1464 | 1318 | 61 k (tremove_pop) | 9 | 3.7 % | micro |
| 13 | `tostring` of a string | 61 | 2.6 (JIT) / 49 (-joff) | 36 k (text_adventure) | 2 | 0.1 % | micro |
| - | `remove(t)` inside tremove_pop's inner loop | 80 (tier 1) | 14 (trace) | 3.06 M | 200 | 82 % | formation: the nest rule flushes the inner trace (section 0.5) |

## 6. What was built against the ceilings

Details, knobs and tests: docs/JIT_TRACE.md section 54. Cycles min of 5 interleaved runs, CPU 4, base = d7bab19 built in this session.

| fix | ceiling it addresses | achieved (Mcyc) | realised |
|---|---|---|---|
| R1 `memfind` (STR_FAST bit 16) | #2: 60 Mcyc on find_plain | find_plain 122.2 -> 112.6 (-9.6); same binary bit off 129.7 | 0.16 (0.29 against the bit off) |
| R2 generic `for` (`t1g`, four results inline) | #8 and the `for`-loop setups of #6 | same binary `t1g=0 -> 3`: text_adventure -0.65 %, inspect_lua -0.8 %, lua_parser -0.7 %, json_rxi -0.3 % instructions at every hot threshold 248-264; whole branch: inspect_lua 1532 -> 1473, parser_combinator 2435 -> 2318, text_adventure 2589 -> 2559 Mcyc | pairs3 in tier 1 218 -> 193 cycles (0.18 of its 139-cycle gap) |
| R5 word-load string comparison | #7 | lib/sub 124.2 -> 116.2, knucleotide 1017.5 -> 988.4 (A/B builds) | sub: 0.5 of the 17-cycle gap |
| R6 MULTRET / C tail calls inline | #5 (the tail-call half) | tier 1: `return floor(x)` 90 -> 83, `select('#')` tail call 151 -> 143 cycles; sample instructions unchanged except scheme_interp -0.8 % | 0.14 of `selectn`'s gap |

## 7. Caveats

* LuaJIT is stripped: its side is whole-run cycles of the same Lua source (microbenchmarks, doubling), never a function profile.
* Doubling on LuaJIT's JIT changes its trace tree: deltas of +-50 Mcyc on text_adventure are noise (`maxfloor` +147 on LuaJIT, `counts` -19).
  Rows under 60 Mcyc are not used as ceilings.
* The per-opcode split counts cold stubs and dispatch code placed after a template with that template.
* Our binaries that differ in any function are not formation-equal (docs/JIT_TRACE.md 43); same-binary knob columns are used where a knob exists.

## 8. Reproduction (scratch scripts, not committed)

* Microbenchmarks: one file per operation (section 1 shape), `cycles(N=1.2M) - cycles(N=0.2M)`, five modes as named above.
* Doubling: insert one statement per command into a copy of `bench/corpus2/dev/text_adventure.lua` (inside `Game:parse`, `Game:say`, `Game:exec`
  or the main loop) and run it with `perf stat` min of 5 on CPU 4 on both engines.
* Survey: `perf record -e ibs_op/cnt_ctl=0,swfilt=1/u -c 50000` with `RUSTYNAIL_JIT_PERFMAP=2` per program, `perf report --sort sym`, weighted by the
  session-m cycles; the 40 programs are every row of `bench/results/session-m/per_program.csv` with `plain_cyc_min / lj_cyc_min > 1.4`.
