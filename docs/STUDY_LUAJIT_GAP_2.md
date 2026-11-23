# Study: why the tail is still behind LuaJIT, and what to build next (2026-11-23, main 7258220, subject 7413bff)

Refresh of docs/STUDY_TIER1_RESIDUE.md and docs/STUDY_LUAJIT_GAP.md with bench/results/session-l. [M] = measured (this study, private release build of
main, CPUs 10-15, memory cap, `perf stat` / `perf record -e instructions:u -c 1000000` (5000000 above 2.5 G instructions) with the perf map and
scripts/tier1-residue.py), [E] = estimate (method stated). Cycle numbers quoted are session-l's CPU-4 minima; this study's own cycle counts (CPUs 10-15)
agree within 3 % (permute 35.7, raytrace 111.9, cd 262.1, co_yield 248.4 Mcyc). Scratch data: scratchpad/prof/*.res, tailcats.json.

## 1. The tail (session-l, 5.5 plain, ours/LuaJIT cycles > 1.5, >= 20 Mcyc)

Geomean 0.939 over 219 programs [M]. 34 programs are above 1.5; their ln-ratio excess sums to 20.58 against a total of -13.88, so the other 185 programs
are at geomean ~0.77 and the tail alone costs 9.9 % of geomean (0.854 without it) [M arithmetic]. Nine programs are above 2x (permute 2.78, raytrace 2.67,
cd 2.37, co_yield 2.33, fmt_s 2.33 (7 Mcyc, below the 20 Mcyc cut), lua_lexer 2.19, text_adventure 2.17, coroutines 2.05, byte_multi 2.04, towers 1.99).

**permute is an artefact of the baseline.** LuaJIT's cycles on awfy/permute across 6 runs here: 292, 134, 537, 14, 264, 629 M (instructions 1253, 557,
2185, 59, 1134, 2647 M) [M]. The bench takes the minimum (the 14 Mcyc lucky run = 54 Minstr). Ours is 36 Mcyc every run: we are 7-8x FASTER than LuaJIT's
typical run and 2.78x slower than its best run. Remove it from the target list and report LuaJIT's median for it (geomean would be ~0.926 with a median
baseline [E: ln(2.78/0.13)/219]). The other focus programs were checked: raytrace 42-44, cd 109-138, towers 24-31, richards 35-39, co_yield 102-104,
lua_lexer 449-472, classic_vector 243-254 Mcyc over 5 runs: stable (cd varies 25 %).

### 1.1 Category shares per tail program [M, instruction shares of our profile]

traces = machine code of traces; cov = tier-1 code + tier-1 helpers + interpreter (the "coverage / residue" category of STUDY_TIER1_RESIDUE 4);
compile = rn_jit/rn_compiler + libc (for `lib/*` programs libc is memchr/memcmp, counted as library); lib = rn_std + strings + table ops + trace helpers.

| program | ours/LJ cyc | ours/LJ ins | Minstr | traces | cov | exits | compile | alloc+GC | lib | other |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| permute (LJ lottery) | 2.78 | 3.59 | 195 | 96.9 | 1.0 | 0 | 2.0 | 0 | 0 | 0 |
| raytrace | 2.67 | 2.07 | 343 | 23.9 | 26.6 | 1.1 | **33.1** | 9.2 | 3.1 | 2.8 |
| cd | 2.37 | 2.44 | 763 | 19.5 | 27.0 | **14.4** | 24.8 | 10.1 | 1.1 | 3.7 |
| lib/co_yield | 2.33 | 1.88 | 1121 | 0 | 72.5 (interp 49) | 0 | 0.1 | 0 | 11.2 | 16.2 (resume_fused etc.) |
| lua_lexer | 2.19 | 2.07 | 3947 | 47.1 | 8.6 | 0.2 | 5.5 | 3.8 | **33.2** | 1.8 |
| text_adventure | 2.17 | 2.17 | 8077 | 3.8 | **45.8** | 0 | 3.9 | 11.5 | 28.1 | 6.9 |
| programs/coroutines | 2.05 | 1.79 | 8568 | 0 | 61.0 (interp 41) | 0 | 1.2 | 0.8 | 12.4 | 24.8 (resume_fused etc.) |
| lib/byte_multi | 2.04 | 3.57 | 303 | 97.3 | 0.6 | 0 | 1.0 | 0 | 0.7 | 0.3 |
| towers | 1.99 | 1.70 | 174 | 76.0 | 6.3 | 0.6 | 15.9 | 0 | 0 | 1.1 |
| classic_vector | 1.93 | 1.79 | 2039 | 33.5 | 1.4 | **17.9** | 3.4 | **26.8** | 16.8 | 0.4 |
| json | 1.91 | 2.13 | 449 | 17.1 | 22.5 | 0.7 | 19.9 | 8.1 | 26.4 | 5.4 |
| stream_closures | 1.89 | 2.11 | 2948 | 43.1 | 14.7 | 4.4 | 9.8 | 16.3 | 9.6 | 2.3 |
| richards | 1.77 | 1.46 | 177 | 57.6 | 11.9 | 1.6 | **24.8** | 1.1 | 0.5 | 2.7 |
| lua_interp | 1.77 | 2.22 | 561 | 36.7 | **56.0** | 0 | 2.8 | 1.6 | 1.7 | 1.3 |
| type_entry / _stable | 1.78 / 1.72 | 2.40 / 2.32 | 10959 / 10588 | 98.8 / 98.5 | 1.2 | 0 | 0 | 0 | 0 | 0 |
| matmul | 1.61 | 1.90 | 2131 | 97.0 | 2.0 | 0 | 0.3 | 0.6 | 0.1 | 0.1 |
| events | 1.71 | 2.04 | 689 | 11.7 | **54.5** | 0 | 2.0 | 15.5 | 10.3 | 5.9 |
| queens | 1.74 | 1.93 | 511 | 63.6 | 24.2 | 0.6 | 6.2 | 1.8 | 1.3 | 2.1 |

(Remaining tail rows are in scratchpad/tailcats.json: lua_parser cov 38 / lib 27, astar_jumper cov 35, coro_scheduler / coro_generators cov 64 / 53 with
resume cost, lib/find_*, sub, tonumber_int, tremove_pop, str_lt_short: library helpers 44-75 %, load_big: parser + alloc.)

### 1.2 Which category explains the tail [E: attribution rule, M: inputs]

Rule: each program's instruction excess over LuaJIT (ours - LJ instructions) is split: compile, cov, exits and coroutine switch charged at 100 % of their
measured instructions (LuaJIT pays little for them), alloc+GC, library and other at 50 % (LuaJIT pays the work too), the rest is "trace code". Capped at the
excess, converted to ln-ratio points in proportion to the cycle ratio.

| category | share of the tail excess | geomean points if the whole tail category were free [E] | programs |
|---|---:|---:|---|
| trace code (protocol, guards, address and loop-control instructions) | 28 % (23 % without permute) | 2.7 % | type_entry x2, matmul, byte / byte_multi / str_eq_long, towers, queens, class_middleclass, lua_lexer |
| coverage / tier-1 residue | 21 % | 2.0 % | text_adventure, lua_parser, astar, events, lua_interp, cd, queens, json |
| library / strings / table ops | 16 % | 1.5 % | lua_lexer, lib/find*, sub, tonumber, tremove, json, text_adventure |
| trace-compile cost at the bench size | 13 % | 1.2 % | raytrace, cd, richards, towers, json, game_loop |
| **coroutine switch (new)** | 11 % | 1.1 % | co_yield, coroutines, coro_scheduler, coro_generators |
| allocation + GC | 5 % | 0.5 % | classic_vector, load_big, stream_closures |
| exits / materialisation | 3 % | 0.3 % | cd, classic_vector |

Compared with STUDY_TIER1_RESIDUE (ten tier-1 programs only): coverage and compile dominated there; over the whole tail the largest bucket is trace
code and the new third is the coroutine switch. The three largest handled-nowhere categories are therefore (1) trace code in the loop bodies, (2) the
coroutine switch, (3) the library / string cost in interpreter-shaped programs (partly T8).

## 2. Measurements for the three categories

### 2.1 Trace code: the loop body of an array loop is twice LuaJIT's [M]

`jit-trace/type_entry_stable` (`for j = 1, m do a = a + t[j] * 0.5 end`, 500 M inner iterations): ours 10588 Minstr = **21.2 instr per iteration**, LuaJIT
4573 = **9.1**. IPC ours 7.2, LuaJIT 3.1, so cycles follow instructions here (1473 vs 856 Mcyc). RUSTYNAIL_JIT_DUMP disassembly of our loop (18 instructions)
against `luajit -jdump=im` (8 instructions):

| ours (per iteration) | n | LuaJIT | n |
|---|---:|---|---:|
| `cmp byte [r15+POLL],0; jne` | 2 | - | 0 |
| `sub r11,4; jl` (BUDGET) | 2 | - | 0 |
| `mov r8,rdi; add r8,-1` (IR `ADD int %i #-1`: 1-based -> 0-based) | 2 | index folded into `[rcx+rbp*8]` | 0 |
| `cmp byte [rdx+r8+4],tag; jne` | 2 | `cmp dword [rcx+rbp*8+4],..; jnb` | 2 |
| `mov rax,r8; not rax; movsd xmm1,[rdx+rax*8]` (values stored below the base) | 3 | `movsd xmm6,[rcx+rbp*8]` | 1 |
| `mulsd; addsd` | 2 | same | 2 |
| `cmp rsi,0; je; add rsi,-1; add rdi,1; jmp` (separate iteration count AND index) | 5 | `add ebp,1; cmp ebp,eax; jle` | 3 |
| total | 18 | | 8 |

Excess 10 = poll 2 + budget 2 + index arithmetic 4 + loop control 2. The poll and budget are the price of exactness (the P-H ablation, JIT_TRACE_REF
3118, priced removing all four LuaJIT relaxations at 3.6 % on the 25 programs and 10.7 % on jit-trace micros; not to be re-opened). The other 6 are plain code
quality: (a) a constant `ADD #-1` in front of every Lua array access is a separate IR op / two machine instructions where the address mode has a
displacement; (b) the negated element address needs `mov+not` per access; (c) FORLOOP keeps the Lua iteration count (`NE %c #0; ADD %c #-1`) next to
the index. Same pattern in matmul (inner body 17 IR ops, ~30 instructions per iteration against LuaJIT's 17.5: `ADD #-1`, two `AREF`s, `FLOAD asize` per row)
and permute (`self.v[i+1]`). byte_multi / str_eq_long (2.0 / 1.7x, 97 % traces) are inline string ops, a separate item.

### 2.2 Coroutine switch [M]

lib/co_yield: 1.488 M resume+yield pairs; ours 1120.6 Minstr / 248.4 Mcyc = **753 instr / 167 cycles per pair**; LuaJIT 595.5 M / 102 Mcyc = 400 / 69.
programs/coroutines: ~10.3 M switches (5 M + 5 M + 0.25 M creates, [E] from the source): ours 8568 M = ~830 / pair, LuaJIT 4789 M = ~465. Both sides
interpret the coroutine body (a trace never spans a yield; our loop heads are blacklisted by `CFunc`, as LuaJIT's are), so the gap is the protocol.
Instruction profile of co_yield per pair: interpreter `run_mode` 345 (46 %; of its cycles 12.7 % + 9.5 % sit on two loads in the generic C-call
sequence of OP_CALL: `ci.next`, stack-room check: pointer-chasing latency), `resume_fused` 86, tier-1 `jit_call` 73 (17 % of the CYCLES: 28 per pair),
`auxwrap` 66, `raw_run_protected` 51, `co_yield` 34 (8 % of cycles), `vm::execute` 25, tier-1 loop 68. Mechanism (rn-core/src/coro.rs): a yield is
`Err(Status::Yield)` travelling up the Rust frames of the interpreter and out of `run_mode`; the resume re-enters through `raw_run_protected` and
`execute`, `finish_op`. LuaJIT does both in assembly fast functions without a frame push for `yield` and without unwinding. BUNDLE-T (`resume_fused`)
already cut the resume half; the yield half and the C-call sequence for `yield` are still the generic path.

### 2.3 Compile cost and the residue at the bench size [M]

raytrace (default size): 343 Minstr; compile + libc 33.1 %, tier 1 + helpers + interpreter 26.6 %, traces 23.9 %. `[trace]` log: 23 root + 44 side
recordings (28 never installed), 12538 recorded bytecode steps cost 15.6 Mtsc = **1245 tsc per recorded bytecode** (34 % of 46 Mtsc total compile time),
pipeline 25.6 Mtsc for 45 pipelines (0.57 Mtsc each), tier-1 lowering 3.5, lazystubs 1.7. cd: compile + libc 24.8 %, rec_step 9.6 Mtsc / 9042 steps
(1063 per step), pipeline 37 Mtsc / 196. richards 24.8 %, towers 15.9 %, json 19.9 %, game_loop 19.4 %. T2 (built) took 1-5 % of instructions; the
recorder step and the back end are untouched. Even with free compilation raytrace would be at ~230 Minstr = 1.4x LuaJIT, cd ~575 = 1.8x [E].
cd's exits: `materialise` 9.7 % + `jit_tr_mat` 2.4 % of all instructions [M] (classic_vector 17.9 %): sunk tables rebuilt at exits.

permute (for completeness [M]): 96.9 % in traces, 225 instr per `permute` call (195 M / 866 k calls; LuaJIT's lucky run 62), method lookups through the
`__index` chain guarded per inlined `swap` (about 40 IR ops with 3 HREFK + HMISS guards). That is protocol cost, not a coverage question; no handle here.

## 3. Handles re-ranked

Gain = instruction gain of the programs named, geomean column = ln-sum over the suite / 219 [E]. "M" cites an existing measurement.

| # | handle | gain and method | geomean [E] | risk | effort | programs |
|---|---|---|---:|---|---|---|
| **N1 (new)** | **Trace loop-body diet** (const-offset fusion into AREF/ALOAD/ASTORE displacement; strength-reduced negated element index; FORLOOP exit by index-vs-limit instead of a second count PHI) | type_entry: 21 -> ~15 instr/iter (-28 % of loop-body instructions [M count, E result]); P-H priced removing 4 of the same 18 as type_* -15..-20 % cycles [M, earlier]; matmul -10 %, permute / queens / towers / sieve-like array loops -5..-8 % [E]; ~30 programs of the suite have such loops | **-1.0 %** | low (pure back end; oracle / jit-diff / mutants) | S-M | type_entry x2, matmul, permute, queens, towers, byte, nbody-like |
| N2 (new) | Coroutine switch: `yield` recognised in the interpreter CALL path (no C frame, no unwinding), resume re-enters the Lua frame directly | co_yield 167 -> ~115 cycles per pair, -30 % [E: removes co_yield + C-call sequence + unwinding ~ 40 % of per-pair instructions, loads latency partly]; 4 tail programs -20..-30 % | -0.6 % | medium (hooks, count budget, `finish_op`, yield inside pcall/metamethods, error paths) | M-L | co_yield 2.33, coroutines 2.05, coro_scheduler, coro_generators |
| TR24c | In-place `Unstable` restarts (`restarts = 1`) | raytrace -3.9 % [M scratch, JIT_TRACE 4810]; roguelike_turns +100 % -> +2.2 % in the T1 knob study [M]; session-l roguelike +5.7 % | -0.15 % | low-medium (formation) | S | raytrace, roguelike, richards? |
| T2b | Compile cost: recorder per-step (1245 tsc), abort-heavy recordings (28 of 67 wasted in raytrace), pipeline | halve rec_step + 25 % of the wasted recordings: raytrace -6 %, cd -4 %, richards -5 % [E from the shares above] | -0.3 % | low | M | raytrace, cd, richards, towers, json, game_loop |
| T6 | Tier-1 per-call cost (+ new_str, string helpers) | tier 1 + helpers is 23-56 % of events / lua_interp / text_adventure / lua_parser / astar: -10 % of that = -3..-5 % each [E] | -0.3 % | low-medium | M | events, lua_interp, text_adventure, lua_parser, astar |
| T8 | Library calls in traces (`string.find` pattern, `tostring`, `table.insert(t,pos,v)`, `string.char` n>3, `table.concat`) | lua_lexer -2..-4 % [E earlier]; lib micros are helper cost, not coverage | -0.2 % | low each | M | lua_lexer, json, lua_parser |
| T4 | Down-recursion links | cd up to -12 % [E: L97 tier-1 share 12.25 %]; naive form +9.8 % [M] | -0.15 % | medium-high | M-L | cd |
| N3 (new) | Exit materialisation cost (cd 12 %, classic_vector 16-18 % of instructions) | halving: cd -6 %, classic_vector -8 % [E] | -0.1 % | medium | M-L | cd, classic_vector |
| T5 | `TableHash` keys | havlak -4..-8 % [E]; havlak is 1.32x, not in the tail | -0.03 % | medium | M | havlak |
| T7 | `FnShort` probes for leaves | deltablue / richards L -2 %, towers def +4.4 % [M] | ~0 | low-medium | S | - |
| T9 | `InnerLoop` | tried: `preplink`, `nest=2` 0; L | ? | high | L | deltablue, raytrace |

Status: T1, T2 (partly), T3 built; the heavy-gate state of those is still open (report.md). TR24c is a recorded lead only.

## 4. Recommendation: build N1, the trace loop-body diet

Why: it is the largest single category of the tail (trace code, 23 % without the permute artefact), it is the only one with a direct instruction-for-
instruction comparison against LuaJIT (18 vs 8), IPC of these loops is 5-7 so instructions are cycles (P-H confirms: 4 of 18 removed = 15-20 %), it is a
back-end only change with no semantic relaxation (soundness oracle unchanged), and it benefits ~30 programs outside the tail too.

Outline (crates/rn-jit/src/tr/):
1. `asm.rs` `aload` / `astore` / `Aref` fusion (around 5100 / 5990 / 6137): when the index operand is an `ADD int x #c` (or `x` plus a constant of
   a `ULT`-checked index) that has no other user, emit `Mi { base, idx: x, scale, off: ARRAY_TAG_BASE + c }` for the tag check and the value address;
   `ra.rs` (`fused` around 435 / 473 / 764) must keep `x` live instead of the `ADD`. `const_index_offsets` already handles the constant-index case.
2. Negated value address: values live below the base (`not` + `[base + idx*8]`): strength-reduce in `opt_loop.rs` (the `Aref if self.body && self.loop_ref`
   hook around 904) by carrying a second induction variable `-idx` for loops whose array index is the loop induction variable; otherwise `mov;neg` once per
   iteration instead of per access.
3. `record.rs` `forloop` (9013 / `forloop_ctl_copy` 9272): for a literal step, emit the exit as `EQ idx last` (last = idx0 + count0 in the pre-roll) and drop the
   count PHI when nothing else reads it (T-PHI count only feeds `NE #0` / `ADD #-1`).
Expected result [E]: 18 -> 13 instructions on type_entry_stable (-28 % loop body, ~ -20 % program), matmul 33 -> ~27; type_entry 1.75 -> ~1.4,
matmul 1.61 -> ~1.4, suite geomean -1.0 %. Gate: gate-light + `tr_oracle` shards + the `tr_core` mutants for AREF / FORLOOP (new mutants: wrong displacement,
off-by-one `last`). Measure with `scripts/bench-fast.sh` on type_entry, matmul, sieve, permute, queens, towers, nbody, spectral_norm, fannkuch.

Order after N1: TR24c (S, quick), then N2 (coroutines) or T2b, whichever the user values (N2 has the larger per-program gain, T2b the wider spread).

Would NOT build: (a) the LuaJIT-style relaxations (hooks / budget / dead slots / GC accounting): P-H priced them at 3.6 % and they change semantics;
(b) T9 `InnerLoop` and the naive T4 sides (measured 0 / +9.8 %); (c) T5 now (havlak is not in the tail; -0.03 % geomean); (d) T7 (towers +4.4 %);
(e) tracing through coroutine yields (LuaJIT cannot either); (f) any work targeted at permute against the min-of-5 LuaJIT baseline: the baseline is a
lottery, change the harness to report the LuaJIT median instead.

## 5. Caveats
* Profiles are single runs at the default size (about 300-8000 samples at 1 M-5 M period): shares have +-1-2 points of noise for the small programs.
* The category attribution in 1.2 is an estimate (rule above); tier-1 + helpers + interpreter is counted as coverage although part of it is call/return
  protocol that LuaJIT also pays in its interpreter for the same code (LuaJIT interprets 32-41 % of raytrace).
* Cycle gains for N1, N2 are extrapolated from instruction counts (N1) and from the instruction + cycle split (N2); neither was built here.
* Session-l ran with a LuaJIT upgrade in the middle (report.md caveats); the LuaJIT cycles re-measured here are within 3-5 % of the cached column.
