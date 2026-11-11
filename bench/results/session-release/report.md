# Release benchmark (session-release): HEAD 8f422153, Lua 5.5 and 5.4, plain and PGO

Data: `tables.md` (all tables), `extra.md`, `per_program_pass{A,B,54}.csv`, raw JSONs, logs, `build_logs/`, `startup_size.json`, `release_script_check.txt`.
Method as in bm2/bm3: min of 5 interleaved runs, `perf stat cycles:u,instructions:u`, `RUSTYNAIL_SEED=0`, SPIN_MS=300, equal-length binary paths.
Pass A: 223 programs, CPUs 3-7. Pass B (5.5 vs LuaJIT, 219 programs) and pass C (5.4, 223 programs, 9 columns): CPUs 4 and 6.
All our outputs (5.5 and 5.4, plain, PGO, JIT off, nojit build) equal PUC and `.expected` on all 223 programs.

## Headline: geomean cycles, below 1 means ours is faster

| build | ours/LuaJIT | ours/PUC (same version) |
|---|---|---|
| 5.5 plain | 0.995 | 0.230 |
| 5.5 PGO | 0.905 | 0.210 |
| 5.4 plain | 1.184 | 0.268 |
| 5.4 PGO | 1.081 | 0.245 |

- 5.5 without real and real_dis (210 programs): 0.988 plain, 0.900 PGO.
- Against bm3 main (1e6ec4ca, rebuilt same session), pass A: plain 0.989 cycles / 0.994 instructions, PGO 0.994 / 0.994. Nothing regressed.
- The JIT makes 5.5 programs about 3.2x faster than the same build with JIT off (plain/JIT-off 0.314), 5.4 about 2.8x (0.354).

## PGO A/B (5.5, pass A, 223 programs): ship PGO as the release default

PGO/plain: 0.913 cycles, 0.919 instructions. 135 programs >3% faster, 81 within 3%, 7 >3% slower. Median 0.941, p10 0.794, p90 1.001.

| set | n | PGO/plain cyc | PGO/plain ins | trained |
|---|---|---|---|---|
| apps | 5 | 0.879 | 0.911 | yes |
| awfy | 14 | 0.941 | 0.947 | yes |
| corpus | 18 | 0.876 | 0.919 | held out |
| corpus2 dev | 19 | 0.877 | 0.902 | held out |
| corpus2 heldout | 13 | 0.910 | 0.923 | held out |
| jit-trace | 29 | 0.991 | 0.989 | yes |
| lib | 95 | 0.902 | 0.899 | yes |
| programs | 25 | 0.924 | 0.916 | yes |
| real | 5 | 0.876 | 0.905 | yes |
| real_dis | 4 | 0.865 | 0.890 | no |
| ALL | 223 | 0.913 | 0.919 | - |

- Held-out sets (corpus, corpus2) gain as much as trained sets, and instructions fall too, so it is not only layout.
- Seven programs are >3% worse. lib/char is systematic (+48%, 178 -> 263 Mcyc at 0.897x instructions, IPC 4.94 -> 3.05, cause not diagnosed). lib/m_type, os_clock, tonumber_int, knucleotide reproduce at +3 to +7%. heapsort and sel_count were layout lottery.
- 5.4 PGO training: `bench/train/gcheavy.lua` calls `collectgarbage("param")` (5.5 only) and fails twice on 5.4; harmless, worth a guard in `scripts/train.sh`.
- corpus and corpus2 are not in the training set.

## 5.5 vs LuaJIT and PUC (pass B)

| set | n | plain/LJ | PGO/LJ | plain/PUC | PGO/PUC |
|---|---|---|---|---|---|
| apps | 5 | 1.275 | 1.119 | 0.515 | 0.452 |
| awfy | 14 | 1.292 | 1.211 | 0.275 | 0.258 |
| corpus | 18 | 1.100 | 0.963 | 0.444 | 0.389 |
| corpus2 dev | 19 | 1.204 | 1.053 | 0.427 | 0.374 |
| corpus2 heldout | 13 | 1.240 | 1.123 | 0.414 | 0.375 |
| jit-trace | 29 | 0.941 | 0.932 | 0.074 | 0.073 |
| lib | 91 | 0.892 | 0.799 | 0.215 | 0.194 |
| programs | 25 | 0.898 | 0.828 | 0.217 | 0.200 |
| real | 5 | 1.220 | 1.068 | 0.452 | 0.395 |
| real_dis | 4 | 1.146 | 0.992 | 0.559 | 0.484 |
| ALL | 219 | 0.995 | 0.905 | 0.230 | 0.210 |

Top gaps to LuaJIT (PGO): fib 3.14, raytrace 3.02, lua_lexer 2.61, lib/char 2.52 (the PGO IPC drop), classic_vector 2.49, tail_loop 2.48, cd 2.18, fmt_s 2.16, byte_multi 2.10, towers 1.99, stream_closures 1.98, lua_interp 1.87, it_pairs_hash 1.83, queens 1.81, type_entry 1.78.
Plain worst: raytrace 3.60, lib/m_random 3.19 (6.4x LuaJIT instructions), fib 3.14, lua_lexer 2.90, classic_vector 2.81.
Biggest wins (PGO): sel_loop 0.045, error_tbl 0.070, pcall_err 0.128, int_rec2 0.135, pcall_error 0.146, tfront_1k 0.189.
Every program is faster than PUC 5.5; closest are lib/os_clock 0.949 (a syscall), tsort_cmp 0.787, text_adventure 0.779.

## Lua 5.4 (pass C)

5.4 is 16% less effective relative to its reference than 5.5 (geomean over 223: (p54/PUC54)/(p55/PUC55) = 1.157). By set: programs 23%, corpus and lib 18.5%, corpus2 12-13%, jit-trace and apps 9%, awfy 8%, real 6%, real_dis 5%. PUC 5.4 is only 1.021x slower than PUC 5.5, so the reference does not explain it.

| set | n | p54/PUC54 | g54/PUC54 | p54/LJ | g54/LJ |
|---|---|---|---|---|---|
| apps | 5 | 0.571 | 0.503 | 1.384 | 1.221 |
| awfy | 14 | 0.295 | 0.279 | 1.497 | 1.413 |
| corpus | 18 | 0.527 | 0.462 | 1.346 | 1.181 |
| corpus2 dev | 19 | 0.479 | 0.422 | 1.375 | 1.211 |
| corpus2 heldout | 13 | 0.472 | 0.431 | 1.443 | 1.317 |
| jit-trace | 29 | 0.081 | 0.080 | 1.095 | 1.083 |
| lib | 95 | 0.257 | 0.233 | 1.072 | 0.966 |
| programs | 25 | 0.267 | 0.247 | 1.137 | 1.050 |
| real | 5 | 0.478 | 0.417 | 1.297 | 1.130 |
| real_dis | 4 | 0.588 | 0.516 | 1.191 | 1.045 |
| ALL | 223 | 0.268 | 0.245 | 1.184 | 1.081 |

5.4-specific costs (relative = (p54/PUC54)/(p55/PUC55); mechanisms checked in source, see `extra.md`):
- `rawget`/`rawset`/`next` not recorded on 5.4: lib/rawget_set 11.5x (0.642 vs 0.057 of PUC), it_next 2.2x.
- Inline `#t` off on 5.4: len_tbl 1.8x, tinsert_append 1.5x, tpack 1.4x.
- Integer-key hash-part access and `__index`/`__newindex` array access off (alimit bookkeeping): tfront_1k 8.6x, int_rec2 3.4x, sieve 3.0x, tmove 2.7x, array_rec 2.5x, tunpack 2.3x, table_array 2.1x.
- Metamethod-heavy programs (class_middleclass 1.04x PUC 5.4 vs 0.16x on 5.5, metamethods 0.95x vs 0.16x): RETF is inert on 5.4 and the metamethod bundles do not seem to take effect. Not root-caused; the largest recoverable item. programs/load_big 2.4x also not root-caused.
- Faster relatively on 5.4: tonumber_num 0.748, m_max 0.781, queens 0.817, int_sum 0.817, mandelbrot 0.847 (pure arithmetic).

## Startup, size, memory

| artifact | unstripped | stripped copy |
|---|---|---|
| 5.5 plain rnlua | 18.9 MB | 2.94 MB |
| 5.5 PGO rnlua | 21.0 MB | 3.31 MB |
| 5.4 plain / PGO | 17.6 / 19.2 MB | 2.73 / 2.99 MB |
| nojit rnlua (5.5 / 5.4) | 7.0 / 7.0 MB | |
| rnluac | 3.5 MB | |
| PUC 5.5 / 5.4 `lua` | 341 / 328 KB | |

The shipped `rnlua` carries debuginfo ("optimized + debuginfo" profile); `strip` would save about 16 MB.

| build | empty script cycles (k) | wall min (ms) | VmHWM (empty state) |
|---|---|---|---|
| PUC 5.5 | 705 | 0.321 | 2.3 MB |
| LuaJIT | 795 | 0.378 | 3.0 MB |
| 5.5 plain | 961 | 0.491 | 4.2 MB |
| 5.5 PGO | 927 | 0.494 | 4.8 MB |
| 5.5 JIT off | 921 | 0.489 | 4.3 MB |
| 5.5 nojit build | 948 | 0.469 | 3.7 MB |
| 5.4 plain | 906 | 0.485 | 4.2 MB |

- We start about 150 us slower than PUC and 110 us slower than LuaJIT. The JIT itself costs about 20 us (0.491 vs 0.489 ms with JIT off); the rest is std library and state setup and the larger binary.
- Tier-1 compile of a 1000-iteration loop costs about 0.1 ms (+358 k cycles); it pays off for loops longer than a few thousand iterations.
- `collectgarbage("count")` of an empty state: 29.2 KB (PUC 20.2 KB, LuaJIT 48.2 KB).

## Release script check

`scripts/release-build.sh` ran for plain 5.5, plain 5.4, `--pgo` and `--pgo --lua54`. `sha256sum -c SHA256SUMS` is OK in all four `dist/` dirs, `--version` and `--license` are correct, smoke tests pass. PGO build takes about 80 s plus training (181 runs).

## Caveats
- The same-session bm3 column is the honest trend; cross-session numbers are indicative.
- The 5.4 pass has no duplicate column; 15-run reruns on CPU 4 show p54 repeatability of 0.993-1.010.
- The 5.4 vs 5.5 JIT-gain comparison mixes pass C and pass A.
- Not diagnosed: lib/char under PGO, the 5.4 metamethod and load_big gaps.
- The 5.4 rerun was started once while the 5.5 rerun was still on CPU 4; it was killed at once and discarded.
- Layout-lottery cases vs bm3 (instructions equal): inline_leaf, awfy/sieve, rec_loop, fmt_q (plain), load_small (PGO), sel_count (PGO). Slightly real: apps/expr_lang +2.0% instructions, awfy/cd +3.8%.
- Machine: AMD Ryzen AI 7 350, governor powersave, boost on, idle-checked before and after every pass (see `machine_state.txt`).
