# Benchmark session 2026-11-10 (main 4ce16b7a)

Data: `tables.md`, `per_program_pass{A,B}.csv`, `pass{A,B}*.json`, `rerun_serial_cpu4_losers_15runs.json`, `knobs_tonumber_num_fib.txt`.
Method: min of 5, equal-length binary dirs, identical script paths, CPUs 3-7 (pass A, all columns) and CPUs 4/6 (pass B, fast cores vs LuaJIT); siblings 11-15 idle (one 2 s sample showed cpu14 at 18%). PGO rebuilt with scripts/pgo-build.sh. All 223 outputs match .expected. Losers re-measured at 15 runs on CPU 4.

## Ours / LuaJIT (pass B, fast cores, ALL = 219 with LuaJIT variant)
| set | n | plain new | PGO new | plain 53133e8d | PGO 53133e8d |
|---|---|---|---|---|---|
| ALL | 219 | 1.150 | 1.046 | 1.181 | 1.078 |
| corpus (old) | 18 | 1.313 | 1.154 | 1.325 | 1.168 |
| corpus2 dev | 19 | 1.284 | 1.118 | 1.339 | 1.178 |
| corpus2 heldout | 13 | 1.303 | 1.182 | 1.351 | 1.237 |
| awfy | 14 | 1.468 | 1.377 | 1.502 | 1.421 |
| programs | 25 | 0.955 | 0.870 | 0.982 | 0.901 |
| jit-trace | 29 | 0.982 | 0.968 | 1.027 | 1.015 |
| lib | 91 | 1.125 | 1.010 | 1.145 | 1.027 |
| apps | 5 | 1.372 | 1.224 | 1.523 | 1.377 |
| real | 5 | 1.373 | 1.215 | 1.352 | 1.218 |
| real_dis (4) | 4 | 1.308 | 1.122 | 1.313 | 1.147 |

Older subjects (ALL, plain/PGO): 8965e73e 1.195/1.091, c276c376 1.229/1.119, cb1af288 1.250/1.135. Vs PUC: 0.264 plain, 0.241 PGO. Interpreter-only build: ours plain / non-jit = 0.349.

## New vs 53133e8d (pass A, geomean cycles / instructions)
| set | plain | PGO |
|---|---|---|
| ALL (223) | 0.975 / 0.957 | 0.972 / 0.958 |
| corpus | 0.991 / 0.985 | 0.986 / 0.990 |
| corpus2 dev | 0.962 / 0.952 | 0.945 / 0.953 |
| corpus2 heldout | 0.961 / 0.953 | 0.958 / 0.957 |
| programs | 0.978 / 0.964 | 0.975 / 0.963 |
| lib | 0.984 / 0.968 | 0.984 / 0.969 |
| awfy | 0.980 / 0.960 | 0.970 / 0.959 |
| apps | 0.904 / 0.934 | 0.900 / 0.941 |
| jit-trace | 0.956 / 0.900 | 0.954 / 0.899 |
| real | 1.010 / 0.989 | 0.998 / 0.989 |
| real_dis (4) | 0.999 / 0.991 | 0.979 / 0.991 |

Vs older (ALL plain cyc/instr): 8965e73e 0.963/0.947, c276c376 0.937/0.923, cb1af288 0.922/0.907. Noise (dup column) 1.001.

## Winners (plain cyc/instr vs 53133e8d)
iter_pipeline 0.552/0.570 (uvsep); apps/expr_lang 0.615/0.753 (C39 fix); jit-trace/type_entry_stable 0.821; awfy/storage 0.825; lib/tinsert_idx 0.852; forth_vm 0.858/0.805; jit-trace/inline_2 0.864/0.679.

## Regressions (12 flagged >3% cycles, re-run at 15 runs; none >3% in instructions)
- **lib/tonumber_num +13% cyc (instr 0.861) and programs/fib +9% cyc (instr 0.966)**: real. Knob test on the plain binary: `budsub=0,pollr15=0,fpool=0` restores both (tonumber_num 231M vs 255M default, 53133e8d 226M; fib 1.429G vs 1.547G, 53133e8d 1.445G); `pollr15=0` alone recovers most of fib. uvsep/fpool/chainre/ncallee alone do nothing. So the P2 loop-top diet removes instructions but costs cycles on these tight loops (hypothesis: dependency/RMW on the budget word; disassembly not inspected). Follow-up item.
- Small, plain only: co_wrap +5.4%, pcall_err +5.3% (instr 0.970, PGO 0.976: layout/path noise as before), real/lua_interp +5.1%, template_engine +4.0%, concat_op_str +3.8%, pcall_error +3.4%, lower +3.1%; lib/rep_sep PGO-only +4.5% (equal instr).
- real: plain 1.010 cycles despite 0.989 instructions.

## Gaps vs LuaJIT (PGO)
classic_vector 4.59, awfy/permute 3.88, lib/find_plain 3.56, lib/it_next_empty 3.53, programs/fib 3.32, class_middleclass 3.22, lib/sub 3.11, apps/raytrace 3.03, lua_lexer 2.64, lib/tonumber_int 2.64. Largest absolute: text_adventure 1.11 Gcyc, fib 1.09, oop_mixins 1.04, jit-trace/short_inner 0.90, classic_vector 0.88.
