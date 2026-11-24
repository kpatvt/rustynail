# Study: why the worst LuaJIT gaps do not close (2026-11-24, main b28ad86, LuaJIT 2.1.1795119671)

Question: after dozens of handles (E1-E9, N1, N2, T1-T9, ...) each giving a few percent, why are awfy/permute, apps/raytrace, awfy/cd,
awfy/towers, corpus/classic_vector, corpus/lua_lexer, awfy/richards, programs/fib, lib/byte_multi, lib/fmt_s, text_adventure still 1.3-2.4x
LuaJIT in cycles, and what is structurally missing?

Tags: **[M]** measured here (command below), **[D]** read off disassembly / IR dumps of the same run, **[I]** inferred from [M] numbers by
the arithmetic stated. No [E]-only claims are used for the ranking.

## 0. Answer in one page

1. **No single cause exists.** Every program's cycle gap splits into 3-6 independent mechanisms, each 10-60 % of the gap (section 2).
   A handle that removes part of one mechanism in a subset of programs saves at most a few percent. That is the arithmetic behind
   "a few percent per handle", and it will go on that way as long as handles are diets inside the current design.
2. **The five real causes, ranked by measured cycles over the 10 programs (permute excluded, see 1.2):**
   | # | cause | where it dominates (share of that program's cycle gap) | class |
   |---|---|---|---|
   | C1 | **Trace compiler throughput**: 4-9x LuaJIT's instructions per recorded trace, and low IPC | richards 56 %, raytrace 54 %, fmt_s ~45 %, cd 43 %, towers 36 % | fixable, large rewrite of the back end |
   | C2 | **Cost per fragment boundary**: side-trace entries through stubs and cells with every slot round-tripped through memory, sunk tables materialised at each exit instead of replayed into the side trace, calls/returns through `CallInfo` | classic_vector 33 % (exits) + part of trace code, cd 28 %, lua_lexer 19 %, fib ~90 %, permute | partly fixable (side links, re-sinking); the call/return part needs a frame-model change |
   | C3 | **Trace code quality inside a fragment**: memory guards 2-3x LuaJIT's (16-byte tagged values, shape + key re-checks), spills (8 GPRs), no load forwarding of slot/heap reloads | byte_multi 90 %, classic_vector 60 %, lua_lexer 51 %, towers 40 % | allocator and guard work fixable; key/tag compares are the value layout |
   | C4 | **Coverage**: code LuaJIT traces runs in our tier 1 (function-entry roots refused, no C-call stitching), and our tier 1 costs more per unit than LuaJIT's interpreter | text_adventure 75 %, lua_lexer 14 %, raytrace 18 %, richards 24 % | fixable (formation policy) |
   | C5 | **Library / allocation helpers** slower than LuaJIT's | text_adventure 40 %, lua_lexer 8 % | fixable, per function |
3. **Why the handles stayed small (section 5):** they attacked C2/C3 sub-pieces one at a time (N1: 6 loop instructions, E1/E2: fewer fragments
   for one recursion shape, T4/T9: link variants), while C1, the largest single cycle item in four of the programs, was only trimmed (T2/T2b:
   1-5 %); trace-code instructions are cheap cycles (IPC 3-7), so instruction diets convert at 1/3 to cycles (session-m: -3.5 % instructions,
   -1 % cycles); and two baselines are lotteries (permute, cd).
4. **Plan (section 6):** (1) a trace back end built for compile speed (C1), (2) register-passing side links with sunk-allocation replay (C2,
   exits), (3) a LuaJIT-style backward linear-scan allocator with rematerialisation (C3 spills), (4) function-entry roots and C-call stitching
   for interpreter-shaped programs (C4). Each is worth 10-25 % on its target programs, measured from the shares below; nothing else on the
   list is.

## 1. Method

* Build: `cargo build --release -p rn-cli` at b28ad86 (plain, not PGO). LuaJIT: `/usr/bin/luajit`, same commit as upstream v2.1 HEAD
  (1795119671), stripped.
* Totals [M]: `perf stat -e cycles:u,instructions:u`, `taskset -c 4`, 300 ms spin on CPU 4 before each run, `RUSTYNAIL_SEED=0`, 9 runs,
  median. (A first pass without the spin and with another perf job on CPU 4 inflated our cycles by up to 1.8x at equal instructions: cycles
  here are only from the clean pass.)
* Category split [M]: **AMD IBS op sampling** (`perf record -e ibs_op/cnt_ctl=1,swfilt=1/u -c 200000`, precise IP, no skid) for instruction
  shares and IBS cycle mode (`cnt_ctl=0`) for cycle shares; repeated 1-40 runs per program to get 4 000-36 000 samples under the 100 k/s
  throttle; our JIT code named through `RUSTYNAIL_JIT_PERFMAP=2`. LuaJIT is stripped, so its non-trace time is split with its own profiler
  (`-jp=vi1`, 2-200 runs averaged: Interpreted / JIT Compiler / GC / C) [I]; its exit handling is inside "helpers" (it has no separate
  exit bucket). Valgrind could not be used (the system glibc uses AVX-512 that VEX does not decode).
* Trace-code split [M+D]: IBS op profile and `RUSTYNAIL_JIT_DUMP` (ours) / `-jdump=+a` (LuaJIT) **in the same run**, every sampled IP
  matched to its instruction (100 % matched on LuaJIT), instructions classified by pattern (call protocol, spills, guards, loads, stores,
  ...). The classification is by operand pattern (e.g. `0x..(%r15)` = spill/exit area, `(%r13)` = `CallInfo`, `[rsp+..]` = LuaJIT spill),
  so bucket borders are approximate [I]; totals are exact.
* Warm-up vs steady state [M]: each program at two sizes; steady cost = difference, warm-up = intercept.

## 2. Per-program breakdown

### 2.1 Totals [M] (median of 9; M = millions)

| program | ours Mcyc | LJ Mcyc | cyc ratio | ours Minstr | LJ Minstr | ins ratio |
|---|---:|---:|---:|---:|---:|---:|
| fib | 643 | 481 | 1.34 | 3865 | 2576 | 1.50 |
| permute | 36.4 | 277 (min 39, earlier min 17) | 0.13 (min: 0.93 / 2.1) | 193 | 1171 (min 161, earlier 71) | 0.17 |
| towers | 47.4 | 25.2 | 1.88 | 173 | 107 | 1.62 |
| richards | 60.7 | 35.5 | 1.71 | 173 | 121 | 1.43 |
| raytrace | 98.5 | 41.4 | 2.38 | 302 | 167 | 1.80 |
| cd | 257 | 113 | 2.27 | 723 | 322 | 2.24 |
| classic_vector | 481 | 246 | 1.96 | 2038 | 1143 | 1.78 |
| lua_lexer | 837 | 459 | 1.82 | 3337 | 1912 | 1.74 |
| byte_multi | 40.3 | 20.0 | 2.01 | 294 | 85 | 3.48 |
| fmt_s | 7.5 | 3.2 | 2.32 | 26.4 | 10.0 | 2.64 |
| text_adventure | 2629 | 1217 | 2.16 | 8084 | 3707 | 2.18 |

### 2.2 permute is a LuaJIT lottery, now proven by mechanism [M+D]

12 runs of `luajit -jdump` on permute: 3 runs record the up-recursion trace at `P:permute` (line 14) first and run 224 M instructions;
the rest start at the loop (line 19), abort with "call unroll limit reached" up to 415 times per trace number and run 2.5-26.9 G. The
order is decided by LuaJIT's randomised hot-count penalty (`lj_trace.c` penalty with PRNG bits). We run 193 M every time; against LuaJIT's
median we are 7.7x faster. Against its lucky run (82 instructions per `permute` call vs our 223) the difference is the C2 protocol: our hottest
trace (34 % of samples) is 117 instructions per execution, of which ~64 are entry/slot reload/return dispatch/cells, 27 the `self.swap`
method chain, 24 the swap itself [D]; LuaJIT's lucky trace inlines three recursion levels and CSEs the `__index` chain once per three calls.

### 2.3 Category tables, **cycles** [M] (Mcyc; `exits` for LuaJIT are inside `helpers`; libc on our side is malloc/free/memcpy of the compiler)

| program | side | total | (a) traces | (b) tier 1 + interp | (c) exits | (d) compile | (e) helpers/alloc/GC/lib | libc | other |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|
| fib | ours / LJ / gap | 643 / 481 / +162 | 639 / 480 / **+159** | 0 / 0 / 0 | 1 / - / +1 | 1 / 0 / +1 | 1 / 1 / 0 | 0 | 0 |
| towers | ours / LJ / gap | 47 / 25 / +22 | 29 / 18 / **+11** | 2 / 0 / +2 | 3 / - / +3 | 9 / 3 / **+6** | 1 / 4 / -3 | +2 | 0 |
| richards | ours / LJ / gap | 61 / 35 / +25 | 31 / 30 / +1 | 6 / 0 / **+6** | 4 / - / +4 | 14 / 4 / **+10** | 2 / 1 / +1 | +4 | 0 |
| raytrace | ours / LJ / gap | 99 / 41 / +57 | 25 / 13 / **+12** | 22 / 12 / **+10** | 8 / - / +8 | 27 / 3 / **+24** | 10 / 13 / -3 | +7 | 0 |
| cd | ours / LJ / gap | 257 / 113 / +144 | 53 / 33 / +20 | 30 / 15 / +15 | 41 / - / **+41** | 76 / 37 / **+39** | 32 / 26 / +5 | +23 | +1 |
| classic_vector | ours / LJ / gap | 481 / 246 / +235 | 193 / 48 / **+145** | 6 / 0 / +6 | 77 / - / **+77** | 17 / 2 / +15 | 182 / 194 / -12 | +4 | 0 |
| lua_lexer | ours / LJ / gap | 837 / 459 / +378 | 351 / 151 / **+200** | 58 / 6 / +52 | 70 / - / **+70** | 22 / 0 / +22 | 301 / 277 / +24 | +5 | +5 |
| byte_multi | ours / LJ / gap | 40 / 20 / +20 | 36 / 19 / **+18** | 1 / 0 / +1 | 0 | 1 / 0 / +1 | 1 / 1 / 0 | 0 | 0 |
| fmt_s | ours / LJ / gap | 7.5 / 3.2 / +4.3 | 3 / 2 / +1 | 1 / 0 / +1 | 0 | 2 / 0 / **+2** | 1 / 1 / 0 | +0.5 | +0.5 |
| text_adventure | ours / LJ / gap | 2629 / 1217 / +1412 | 99 / 368 / -269 | 1243 / 176 / **+1066** | 38 / - / +38 | 15 / 0 / +15 | 1178 / 643 / **+535** | +27 | 0 |

Instruction version of the same table (Minstr): fib gap +1288 = traces +1289; towers +66 = traces +44, compile +8 (+5 libc), exits +11,
tier1 +10, helpers -12; richards +52 = compile +24 (+8 libc), tier1 +20, exits +11, traces -12; raytrace +134 = compile +60 (+13 libc), traces +32,
tier1 +29, exits +16, helpers -15; cd +400 = exits +145, compile +69 (+45 libc), tier1 +62, traces +46, helpers +33; classic_vector +895 =
traces +464, exits +324, compile +45, helpers +28, tier1 +24; lua_lexer +1424 = traces +761, helpers +265, exits +184, tier1 +158, compile
+55; byte_multi +210 = traces +199; text_adventure +4376 = tier1 +3347, helpers +1482, traces -578. Each row sums to the measured total gap
within rounding (the split is of the measured totals by sample shares, so it sums by construction; the sampling error is +-1-2 points per
bucket for 4 000 samples).

### 2.4 Warm-up vs steady state [M] (two sizes; Minstr / Mcyc)

| program (sizes) | ours steady cyc / LJ | steady ratio | ours warm-up cyc | LJ warm-up cyc | share of the default-size cycle gap that is warm-up |
|---|---:|---:|---:|---:|---:|
| towers (45 / 90) | 28.6 / 15.9 | 1.80 | 19.0 | 10.0 | 41 % |
| richards (25 / 50) | 36.8 / 28.6 | 1.29 | 23.7 | 6.7 | **68 %** |
| raytrace (2 / 4) | 51.7 / 34.4 | 1.50 | 45.4 | 6.7 | **69 %** |
| cd (1 / 2) | 126.9 / 78.8 | 1.61 | 127.7 | 31.1 | **67 %** |
| classic_vector (1000 / 2000) | 440 / 241 | 1.83 | 40 | 4 | 15 % |
| lua_lexer (50 / 100) | 818 / 441 | 1.85 | 15 | 38 | ~0 % |

Two independent methods agree: the IBS compile bucket (2.3) and the warm-up intercept. For raytrace / cd / richards two thirds of the
cycle gap is paid before steady state, and most of that is the compiler.

## 3. The mechanisms, with evidence

### C1. Trace compiler throughput [M]

Per recording attempt (our root + side recordings from `[trace]` counters; LuaJIT's traces + aborts from `-jv`):

| program | ours recordings | ours compile Minstr (incl. libc) | per recording | LJ attempts | LJ compile Minstr [I: `-jp` share] | per attempt | ratio |
|---|---:|---:|---:|---:|---:|---:|---:|
| raytrace | 70 | 84 | 1.20 M | 82 | 11 | 0.13 M | 8.9x |
| cd | 327 | 220 | 0.67 M | 688 | 106 | 0.15 M | 4.4x |
| richards | 45 | 45 | 1.00 M | 81 | 13 | 0.16 M | 6.2x |
| towers | 41 | 26 | 0.63 M | 70 | 12 | 0.17 M | 3.7x |

We do not compile more traces than LuaJIT (fewer, in fact); each one costs 3.7-8.9x. Where it goes (IBS op, % of all compile instructions, raytrace
/ cd / richards / towers): trace register allocator + assembler (`ra::allocate_hinted`, `Ra::run`, `Assembler::rm_op`) 24 / 29 / 30 / 30;
libc malloc/free/realloc/memcpy 16 / 21 / 18 / 22; recorder + fold (`record::step`, `Rec::close`, `ir::Builder::emit_flags`) 23 / 13 / 13 / 16;
snapshots / sinking / cache (`SlotMap::entries_into`, `sink::rebuild`, `cache::shape_hash_ir`) 16 / 16 / 12 / 13; **tier-1 lowering** of
prototypes (`lower::lower_ex`, `x86::emit_ex`, a cost LuaJIT does not have at all) 10 / 8 / 16 / 7; Rust Vec growth / hashing / sorting /
drop 6 / 9 / 9 / 8. The compiler is uniformly heavy (allocation-heavy data structures, several passes, tier-1 code for every hot prototype),
not one hot spot, which is why T2/T2b (one hot spot each) gave 1-5 %. Compile code also runs at low IPC: its cycle share exceeds its
instruction share in every program (raytrace 33.5 vs 28.0 %, cd 38.7 vs 30.6 %, towers 23.2 vs 14.9 %, fmt_s 27.4 vs 10.8 %) [M].
Code size points the same way: a 4-IR-op permute trace is 2152 bytes / 416 instructions of machine code (prologue, return dispatch, stubs),
LuaJIT's comparable fib side trace is 133 bytes [D].

### C2. Cost per fragment boundary [M+D]

(a) **Call and return (recursion).** fib: 49.4 instructions per call against LuaJIT's 33.0 [M: 3865 M / 78.2 M calls, 2576 M]. The trace-code
split [M+D, fib 37, same run]: call/return protocol (`CallInfo` push/pop/checks, return-site compare chain, cell loads + `jmp *%rax`, base
adjust, room checks) **1483 M vs 421 M**, poll/budget 344 vs 114 (vmstate store), loads + stores 980 vs 512, constants 69 vs 616 (LuaJIT
reloads its constants from `[r14-..]` at every trace entry), spills 104 vs 0, the arithmetic and guards equal (286 / 294, 429 / 431).
**92 % of fib's trace excess is boundary protocol.** One return, side by side [D]:

```
LuaJIT RETF (trace 4)                       ours, leaf return (base-case trace)
cmp  rdi, [rdx-0x8]   ; saved PC = site?    mov  0x10(%r13),%rdi            ; ci->previous
jnz  ->exit                                 lea  0x0(%r13),%r9 ; cmp %r9,0x908(%r15) ; jne  ; lazy-root check
add  rdx, -0x20       ; pop frame           mov  0x20(%rdi),%r8             ; caller savedpc
mov  [r14-0xe20], rdx                       movabs $site1,%r9 ; cmp %r9,%r8 ; je    ; return-site chain (2-3 sites)
                                            movabs $site2,%r9 ; cmp %r9,%r8 ; je
(5 instructions incl. lea of the PC)        movq $3,-0x10(%rbx) ; mov %rsi,-0x8(%rbx) ; result tag + payload
                                            mov  %rdi,%r13 ; add $-0x20,%rbx ; pop ci, base
                                            movabs $cell,%rax ; mov (%rax),%rax ; cmp $0,%rax ; jmp *%rax  ; next fragment
                                            (~19-22 instructions)
```

A push is 4 slot stores (16-byte values: tag + payload) + 7 `CallInfo` instructions against LuaJIT's 3 stores (function, frame link, argument).
Our fib is 6 traces of 2-5 IR ops each and every one ends at a call or return (`EQ #0 #1` + native transfer) [D]; LuaJIT's 14 traces each cover
up to three returns plus the next call. E1/E2/E6 cut the *number* of boundaries; the *price* per boundary is the frame model.

(b) **Side-trace entry and links.** By design (docs/JIT_TRACE.md 7.8, 36.3) an exit with a side trace attached runs its stub: exit-map slot
stores to the Lua stack (tag + payload each), the exit counter, the budget write-back, a cell load and `jmp *`; the side trace reloads the
slots (`SLOAD`) from memory; every link to another trace goes through the target's dispatch cell and re-runs its entry checks. `dlink` (E6)
removes part of this only for lazy-aware recursion families. LuaJIT patches the parent's exit branch to jump into the side trace, which
inherits the parent's registers and spill slots (no stores, no reloads, no re-check). Measured cost: the `c` bucket plus the slot traffic
in traces: classic_vector 301 k exits, 324 M instructions in exit code = **1080 instructions per exit**; cd 1.59 M exits / 145 M; lua_lexer
7.3 M exits / 184 M [M].

(c) **Sunk allocations are materialised at the boundary, not replayed.** classic_vector: `materialise` 7.8 % + `exit_entry` 2.7 % + `restore`
2.6 % of all instructions; `[trace] sink: materialised 555851 ... stub-tables 292096` [M]. LuaJIT's `snap_replay` re-emits a sunk allocation
into the side trace's IR, where it can be sunk again or allocated inline. Turning sinking off measures the trade [M, 3 runs]: classic_vector
+13.5 % instructions / +13 % cycles (sinking pays there), **cd -2.2 % instructions / -1 % cycles (sinking costs cd more than it saves,
because of the materialisation)**.

### C3. Trace code quality inside a fragment [M+D]

Trace-code instructions by bucket, ours vs LuaJIT, same runs (M instructions; towers 450, richards 250, defaults otherwise):

| bucket | towers | richards | lua_lexer | classic_vector | byte_multi |
|---|---:|---:|---:|---:|---:|
| memory guards (type / tag / shape / key) | 277 / 96 | 318 / 140 | 160 / 57 | 147 / 22 | 32 / 4 |
| value compares (register guards) | 99 / 78 | 83 / 71 | 212 / 85 | 75 / 15 | 59 / 6 |
| spill / reload | **147 / 0** | 60 / 1 | **206 / 18** | 86 / 15 | **82 / 0** |
| loads (frame slots + heap) | 199 / 70 | 144 / 76 | 347 / 55 | 126 / 14 | 5 / 6 |
| stores (frame slots + heap) | 132 / 26 | 59 / 7 | 67 / 26 | 32 / 5 | 0 / 0 |
| call protocol + poll/budget | 144 / 16 | 43 / 7 | 61 / 32 | 13 / 5 | 2 / 0 |
| constants | 72 / 63 | 122 / 79 | 59 / 62 | 56 / 16 | 0 / 0 |
| arithmetic / moves / unbox | 83 / 156 | 73 / 144 | 136 / 181 | 48 / 34 | 75 / 41 |
| jumps / calls | 66 / 27 | 86 / 35 | 87 / 42 | 40 / 11 | 18 / 5 |
| **trace total** | **1218 / 533** | **988 / 560** | **1335 / 558** | **622 / 138** | **274 / 61** |

Shares of the trace excess: guards 29 / 44 / 30 / 38 / 38 %, spills 21 / 14 / 24 / 15 / 38 %, loads + stores 34 / 28 / 43 / 29 / 0 %
(towers / richards / lua_lexer / classic_vector / byte_multi). LuaJIT's NaN-box unboxing (`rorx; shr`) is extra arithmetic we do not pay
(the negative arithmetic cells): the value layout costs LuaJIT there and costs us in guards and stores.

* **Guards / value layout.** A hash-key check is one 64-bit compare for LuaJIT (`mov rdi, imm64; cmp rdi, [r15+0x38]; jnz`) and two for us
  (`cmpw $0x4445,0x38(%r8); jne; movabs $key,%rax; cmp %rax,0x40(%r8); jne`) because tag and payload are separate words [D, permute trace 7 /
  LuaJIT trace 2]. Every slot store is two instructions (tag + payload: towers' recursive push writes 11 slots = 22 stores [D]). Per method
  call we re-prove instance-key absence (`HMISS` per key, sometimes two probes), metatable, `__index`, `HSHAPE`, `HREFK`, `EQREF` on the
  proto (towers' hottest trace: 33 of 83 IR ops are method resolution [D]); LuaJIT emits the same chain once per trace and CSEs the
  repeats, but pays a hash loop for absence. Net, memory guards are 2.3-6.7x LuaJIT's.
* **Register allocation.** The trace allocator has 8 general registers (`V0..V8` minus scratch `V7`; `rbp`, `r14` only with `xregs`, off;
  `rbx` base, `r11` budget, `r12` thread, `r13` `CallInfo`, `r15` state pinned: crates/rn-jit/src/tr/ra.rs) and puts loop "homes" in
  permanent spill slots reloaded before each use. LuaJIT allocates ~13. byte_multi's loop is 103 instructions per iteration against
  LuaJIT's 29.6 [M: 294 M and 85 M / 2.86 M iterations]; ~20 of ours are `0x..(%r15)` spill traffic and the loop-carried values are moved
  through memory at the back-edge [D]. **Validation [M]: `xregs=3` (two more registers)**: byte_multi -7.7 % instructions / -1.5 % cycles,
  fmt_s -10 % / -12 %, towers -0.6 % / -0.7 %, but lua_lexer +2.9 % cycles, richards +3.6 %, cd +2.1 %, raytrace +2.2 % (callee-saved
  registers saved at every entry/exit). The register count is not the lever; the allocator's policy (homes in memory, snapshot pinning,
  spill-everything-across-call-sites) is.
* **Dead multi-results and per-result checks.** `string.byte(s, i, i+3)` with two of four results unused: we load and range-check all four
  results separately (each re-reads the string length); LuaJIT emits one range check (`ULE`, `EQ 4`) and two loads [D]. 
* **No load forwarding across fragment boundaries.** lua_lexer: frame-slot loads are 12 % of its trace samples (160 M) against LuaJIT's
  55 M loads of all kinds; slot values written by one fragment are re-read (and type-checked) by the next.

### C4. Coverage [M]

text_adventure: 47 % of our cycles in tier 1 (`lua_proto:*`, `jit_call`, `ccall`, `jit_tforcall`), traces 3.8 %; LuaJIT runs it 57 % compiled
(`-jp`), 14.5 % interpreted. Our `[trace]` counters: `StartRefused` 231, `FnShort` 42, 20 heads blacklisted. LuaJIT starts root traces at hot
function entries and stitches around C functions (`gmatch`, `format`); we refuse non-recursive function-entry roots (T7 exists, off) and end
traces at unsupported C calls. The tier-1 bucket alone is 1066 of the 1412 Mcyc gap (75 %), minus the 269 Mcyc LuaJIT spends in traces for
the same code. raytrace / richards / lua_lexer: tier 1 + interpreter +10 / +6 / +52 Mcyc. LuaJIT's interpreter runs 30-36 % of raytrace for
12 Mcyc; our tier 1 + interpreter take 22 Mcyc for the residue.

### C5. Library and runtime helpers [M]

text_adventure +535 Mcyc (`Heap::new_str`, `gmatch_aux`, `table::newkey`, `format_at`, `tremove`, the pattern matcher, `push_scratch` /
`adjust_varargs`): our library path costs 1.8x LuaJIT's helpers + C code. lua_lexer: helpers +24 Mcyc (+265 M instructions: `short_core`
(string creation from traces) 9.7 %, `tconcat` 5.7 %, `jit_tr_lib_sub` 4.8 %, pattern `MatchState::cm` 4.5 % of all instructions).
Allocation + GC proper is not behind: classic_vector's helper bucket is 182 Mcyc against LuaJIT's 194 (LuaJIT spends 34 % in its GC there).

## 4. Ranked causes with the share of each program's cycle gap they explain

Shares of the measured cycle gap (section 2.3; trace-code cells split by the C3 instruction shares [I]; our libc counted with compile for
towers / richards / raytrace / cd, with helpers for lua_lexer / text_adventure). Rows sum to 100 % +- rounding.

| program (gap Mcyc) | C1 compile | C2 boundaries (exits + call protocol) | C3 code quality (guards, spills, loads/stores) | C4 coverage (tier 1) | C5 helpers | other |
|---|---:|---:|---:|---:|---:|---:|
| fib (+162) | 1 % | **91 %** (trace protocol) | 8 % (spills) | 0 | 0 | 0 |
| towers (+22) | **36 %** | 14 % exits + 10 % protocol | 40 % | 9 % | -14 % | 5 % |
| richards (+25) | **56 %** | 16 % exits + 1 % | 3 % | **24 %** | 4 % | -4 % |
| raytrace (+57) | **54 %** | 14 % | 21 % | 18 % | -5 % | -2 % |
| cd (+144) | **43 %** | **28 %** | 14 % | 10 % | 3 % | 2 % |
| classic_vector (+235) | 6 % | **33 %** + 1 % | **60 %** | 3 % | -5 % | 2 % (libc) |
| lua_lexer (+378) | 6 % | 19 % + 2 % | **51 %** | 14 % | 8 % | 1 % |
| byte_multi (+20) | 5 % | 0 | **90 %** | 5 % | 0 | 0 |
| fmt_s (+4.3) | **47 %** | 0 | 23 % | 23 % | 0 | 7 % |
| text_adventure (+1412) | 1 % | 3 % | -19 % (LJ traces more) | **75 %** | **40 %** | 0 |

Instruction-gap shares follow the same ranking except that C1 is smaller in instructions than in cycles (compile IPC is low) and C3 is larger
in instructions than in cycles (trace IPC is high).

## 5. Why the incremental handles saved only a few percent each

1. **The gap is spread.** The largest single cause of any program is 36-60 % of its gap (except fib and byte_multi). A handle that removes
   a quarter of one cause in the programs it applies to is worth 5-15 % on those programs and ~1 % on the suite. That is what was observed.
2. **The largest cycle item was never attacked as a whole.** C1 is 36-56 % of the cycle gap of raytrace, cd, richards, towers and fmt_s.
   T2 and T2b each shaved one hot spot of a compiler whose cost is spread over a dozen functions and malloc (3.1). No handle changed the
   per-trace cost by more than 5 %.
3. **The handles optimised the number of boundaries, not their price.** E1/E2/E4/E5/E6/T1/T3/T4/T9 changed which fragments exist and how
   they link; each fragment still pays stub stores, cell jumps, slot reloads with type guards and the `CallInfo` protocol (C2). fib is
   still 92 % protocol in its trace excess after E1-E6.
4. **Instructions are not cycles in trace code.** Trace code runs at IPC 3-7, compile and exit code at IPC ~1.5-2.5. The diets (N1,
   guard and slot elisions) removed trace instructions: session-m measured -3.5 % instructions for -1 % cycles over the suite.
5. **Estimates were built on skidded profiles and on per-piece counts.** The N2 estimate counted pieces shared with every native call and
   used skidded `instructions:u` shares (docs/STUDY_LUAJIT_GAP_2.md section 3). This study uses IBS (precise IPs) and same-run dumps; the
   trace-code buckets of section 3 are what the code executes.
6. **Two baselines are noise.** permute's LuaJIT minimum is a lottery (2.2); LuaJIT's cd varies 108-136 Mcyc. The 2.76 for permute was
   never a gap.

## 6. What is fixable, what is architectural, and the plan (largest win first)

Gains below are computed from the measured buckets: removing X % of a bucket saves X % of its cycles; nothing else is assumed.

| # | item | class | what it takes | measured basis | gain (cycles) |
|---|---|---|---|---|---|
| 1 | **Compile-speed back end** | fixable, large | one arena per trace (no per-trace `Vec`/`HashMap`/malloc), a single backward pass that allocates registers while emitting code (LuaJIT `lj_asm` model) instead of `ra` + `asm` passes, snapshots stored compactly, no shape hash per trace, and no tier-1 lowering for prototypes whose hot paths are traced (lower it lazily on the first tier-1 entry) | compile + libc cycles: raytrace 31 of 99, cd 62 of 257, richards 14 of 61, towers 8 of 47, fmt_s 2.5 of 7.5 | at 3x cheaper per trace: raytrace -21 %, richards -15 %, cd -16 %, towers -11 %, fmt_s -22 % |
| 2 | **Register-passing side links + sunk-allocation replay** | fixable, medium-large (allocator coupling: side trace allocated with the parent's exit-map locations as entry hints; LuaJIT's `asm_head_side`) | side trace entered by a patched jump, inherits registers / spill slots, no stub stores or `SLOAD` re-checks; sunk tables of the parent snapshot replayed as virtual allocations in the side trace (`snap_replay`), materialised only on a real exit to the interpreter | exit bucket: classic_vector 77 of 481, cd 41 of 257, lua_lexer 70 of 837, raytrace 8 of 99 Mcyc, plus slot reloads in trace code | removing 80 % of the exit bucket: classic_vector -13 %, cd -13 %, lua_lexer -7 %, raytrace -6 % |
| 3 | **Allocator rewrite** | fixable, medium | backward linear scan with rematerialisation of constants and loads, loop-carried values in registers (no memory "homes"), spill only on the cold side of a helper call, snapshot refs never forced to a slot when a register is free | spill bucket: byte_multi 82 of 294 M instr, towers 147 of 1413, lua_lexer 206 of 3337, classic_vector 86 of 2038 | instructions: byte_multi -25 %, towers -9 %, lua_lexer -5 %, classic_vector -3 %; cycles about 1/2 of that |
| 4 | **Function-entry roots + C-call stitching** for interpreter-shaped code | fixable (formation; T7 was a first probe) | root traces at hot non-recursive function entries when the body is long enough; continue a trace after an unsupported C call (LuaJIT 2.1 stitching) instead of ending it | text_adventure tier 1 1243 Mcyc (LuaJIT: 176 interpreted + 368 traced for the same code) | text_adventure up to -25 % if half the tier-1 residue becomes trace code at LuaJIT's cost [I] |
| 5 | Guard diet: CSE of method resolution across inlined frames of one trace (one absence proof per key per trace), multi-result library calls with dead results dropped | fixable, small-medium | IR-level CSE keyed on (table ref, key) with alias rules for `HSTORE`; recorder change for `string.byte` multi | guard bucket: towers 181 M excess, richards 178, classic_vector 125 | towers / richards -5..-8 % instructions |
| 6 | **Frame model** (frame link in the Lua stack, return = compare of the saved PC, `CallInfo` built lazily only when something outside a trace needs it) | **architectural** (touches tier 1, debug API, errors, coroutines; DESIGN_CALL_PROTOCOL rejected it) | | fib: protocol 1483 vs 421 M instr | fib -25 %, permute / towers -10..-15 % [I from the protocol buckets] |
| 7 | Value layout (NaN-boxing or tag+payload in one word) | **architectural**, the largest | every value access in every layer | key checks 2 -> 1 compare, slot stores 2 -> 1 | bounded by the guard + store buckets: 15-25 % of trace code; not worth it before 1-4 |

Recommended order: **1, then 2, then 3**, then 4 for the interpreter-shaped programs. 1-3 are back-end work with no semantic change (the
soundness oracle, jit-diff and mutant gates apply unchanged). 6 and 7 are the only items that close fib and the recursion programs to
LuaJIT and both are rewrites of the runtime contract; do them only if 1-4 leave the user unsatisfied. Stop building diets of the current
loop-body / boundary code (N1-style): the measured remaining room in those buckets is a few percent per item.

## 7. Caveats

* LuaJIT's non-trace split comes from `-jp` time shares applied to IBS op shares [I]; LuaJIT's exits are inside its helpers bucket.
* Trace-code buckets are classified by operand pattern; borders (e.g. a `0x..(%r15)` exit-area store counted as spill) move a few points.
* Our cycles depend on the run environment: the first pass of this study (no spin, a concurrent perf job on CPU 4) read cd at 460 Mcyc
  with equal instructions; only the clean pass is used.
* fmt_s runs 3-8 ms: its split has +-5 points of noise.
