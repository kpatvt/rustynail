# BUNDLE-D ceiling measurements (throwaway patch, reverted)

Patch: tier-1 CALL without hook-mask test, interrupt test and budget CountSub; RETURN without interrupt poll and budget CountSub.
Binaries: b = base 602b7649, d = byte copy of b, n = patched. CPU 4, RUSTYNAIL_SEED=0, warm=0, load < 1.0.

## call_lua.lua (80 M empty calls, tier-1, RUSTYNAIL_JIT_TRACE=off), perf stat, 5 runs (cycles, instructions)
b: 1463999709 9801750743 / 1464339642 / 1464526832 / 1464583487 / 1466039596 (instr 9.8017e9)
n: 1375856542 8601781372 / 1377718925 / 1378196273 / 1378251941 (instr 8.6018e9)
per call+return: 18.3 -> 17.2 cycles, 122.5 -> 107.5 instructions

## awfy (RUNS 7, min Mcyc): b / d / n
cd 278.8 / 278.8 / 275.9 (0.990); deltablue 107.5 / 107.7 / 108.9 (1.013); richards 69.5 / 69.9 / 70.0 (1.007); towers 101.4 / 101.5 / 99.0 (0.976)
geomean n (4): min 0.996, median 0.999, instructions 1.004; dup 1.002 / 1.001

## apps (RUNS 7, min Mcyc)
events 177.5 / 177.4 / 182.5 (1.028); lru_cache 186.8 / 186.5 / 182.9 (0.979)
Six-program geomean n: 0.998
