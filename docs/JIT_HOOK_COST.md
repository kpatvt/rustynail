# JIT hook cost (J0 item 3): what the interpreter pays for the JIT's hooks

Status: measurement prototype, 2026-10-29. Branch work of the `<worktree>` worktree. No JIT code exists; the
hooks only count and test. Implements the question of `docs/JIT_PLAN.md` section 4 J0 item 3 / section 9 (budget: at most
0.5% instructions on call/return heavy programs, 0 with the feature off).

## 1. Result in one paragraph

* **Feature OFF: zero cost.** `objcopy --only-section=.text` of the OFF `rnlua` is byte-identical to the one built
  from main `eca9a7b` (default and `fast` build); instruction counts are identical (ratios 1.000, differences are
  hash-seed noise in the programs that hash strings).
* **Feature ON: the 0.5% budget is NOT met by any deterministic counter we tried.** The floor is the call-time counter
  itself (one memory `sub` + one branch per Lua call): fib +2.5%, call-heavy mean (fib, closures, oop_methods,
  varargs) +1.4%. The cheapest complete configuration (**placement (e) + counter B**, section 3) costs
  **+1.16% instructions geomean over the 25 programs, +1.56% on the call-heavy four, worst case sieve +3.7%**
  (`fast` build: +1.19% / +1.48% / +4.2%). The loop back-edge counter alone costs +0.75% geomean (sieve +3.7%, fannkuch
  +1.8%, loops_arith +1.5%); the entry counter alone +0.46% geomean.
* **Recommendation for J1:** placement (e) with counter B for entry+loops (section 6), and treat the remaining ~1.2% as the
  price of deterministic hotness; or drop the hot-path counters altogether and use a timer-driven sampler (zero
  hot-path instructions, section 6) if the orchestrator wants the budget met.

## 2. What was implemented (all behind cargo features, OFF by default)

`rn-core` features (pass-through `jit`, `jit_counter_b`, `jit_counter_c`, `jit_counter_g`, `jit_check_d`, `jit_check_e`,
`jit_no_loop`, `jit_no_entry` in `rn-std`, `rnlua`, `rustynail`; `cargo build --release -p rn-cli --features jit`):

| feature | meaning |
|---|---|
| `jit` | `Proto` gets `jit: *mut u8` (null) and `hot: Cell<i32>` (appended; `new_proto` is the only constructor, so compiler output, `undump` and tests all go through it); `crates/rn-core/src/jit_hooks.rs` (stubs and statistics); default variant = counter A + placement (c) |
| counter A (default) | `Proto.hot` counts down; entry weight 5, back-edge weight 1, threshold 1000; at the back-edge the proto is reached through the running closure (`base[-1]` -> closure -> proto): 3 dependent loads |
| `jit_counter_b` | **B**: one sampling tick per thread (`Thread.jit_tick`), decremented at frame starts and back-edges; every 1000 events `jit_hot(st, None)` fires (the real implementation samples `st.ci`'s proto then). No per-frame init, no proto access at the back-edge |
| `jit_counter_g` | **G** (variant of B): the tick is a process-global relaxed atomic (no pointer chase). Measured: no better than B |
| `jit_counter_c` | **C**: loop budget in `CallInfo.hot`, initialised at frame creation (1 extra store per call), per activation |
| placement (c) (default) | counter and the E1 check `if !proto.jit.is_null() && jit_try_enter(st, ci)` at the top of `'frame: loop` (after the trap handling and the closure tag check), fast copy only |
| `jit_check_d` | (d): in `pre_call_lua_fast` / `pre_call` (callee side: count, test `p.jit`, set `CIST_JIT` in `callstatus`) and in `do_ret!` (test `prev.callstatus & CIST_JIT`) |
| `jit_check_e` | (e): counter and check **merged**: `p.hot -= 5; if p.hot <= 0 { jit_hot_call(..) }` in `pre_call_lua_fast`/`pre_call`; a compiled proto keeps `hot` parked at <= 0 so every call lands in the cold function, which does the lookup/enter; nothing at the `'frame` head or in `do_ret!` |
| `jit_no_loop`, `jit_no_entry` | ablations: no back-edge counters / no entry counting and no entry check |

Counting sites: `dojump!` when the offset is negative (JMP back, repeat-until via `donextjump!`), FORLOOP (5.4 and 5.5
variants), TFORLOOP (`do_tforloop!`), all in the fast copy only (`!HOOK`). Cold stubs (`#[cold] #[inline(never)]`):
`jit_hot(st, Option<&Proto>)` (resets the counter, bumps `hot_fired()`), `jit_hot_call`, `jit_hot_loop` (C),
`jit_try_enter(st, ci) -> bool` (bumps `enter_calls()`, returns false). After firing, counters are **parked** at a huge
value (`PARK_HIGH`): the prototype never compiles, and re-firing every 200 calls would only measure the stub (the first
measurement run had this artifact: fib +1.2% from `jit_hot` alone, and a bogus +7% for B/G with (e)). Real code compiles
or marks the proto "do not compile" and stops counting; the steady state is what is measured here.

Tests (all pass with the features on): `tests_state::jit_hot_counters_fire_at_back_edges_and_frame_starts` (a
hand-assembled 5000-iteration FORLOOP fires the stub), `tests_state::jit_entry_check_hit_case_declines` (a proto with
`jit != null` reaches `jit_try_enter` in every placement and still returns the right result).

## 3. Method

* Baseline: main `eca9a7b` via `git archive` into a scratch dir with its own `CARGO_TARGET_DIR` (plain release build and the
  `scripts/build-fast.sh` build), binaries kept. Variants: `scripts/jit-hook-cost.sh build|text|bench` (one `rnlua`
  per feature set, built niced on cpus 0-2).
* `perf stat -e instructions:u,cycles:u` pinned with `taskset -c 4` (the `fast` run on cpu 5, never cpu 3), **min of 7
  runs** (5 for `fast`), binaries interleaved per run, 25 programs of `bench/programs`. Instruction counts are the
  signal (the hash-seed noise of string-heavy programs is about +-1%); cycles are shown only as a secondary signal and
  are noisy (+-5% per cell on this machine, see PERF.md; do not read single cells).
* Names: `X_p` = counter X in {A, B, C, G} with placement p in {c, d, e}. `noloop_*` = entry hooks only,
  `noentry_*` = back-edge counters only.

## 4. Instructions (ratio vs baseline, 1.0000 = identical; `off` = feature OFF)

Full configurations:

| benchmark | off | A_c | A_d | B_c | B_d | A_e | B_e | C_e | G_e |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| binary_trees | 1.0000 | 1.0432 | 1.0336 | 1.0350 | 1.0280 | 1.0244 | 1.0138 | 1.0173 | 1.0156 |
| closures | 1.0000 | 1.0888 | 1.0559 | 1.0668 | 1.0426 | 1.0430 | 1.0170 | 1.0249 | 1.0224 |
| coroutines | 1.0000 | 1.0312 | 1.0223 | 1.0185 | 1.0129 | 1.0211 | 1.0085 | 1.0129 | 1.0129 |
| fannkuch | 1.0000 | 1.0430 | 1.0525 | 1.0292 | 1.0386 | 1.0525 | 1.0176 | 1.0245 | 1.0245 |
| fasta | 1.0009 | 1.0389 | 1.0388 | 1.0320 | 1.0332 | 1.0347 | 1.0120 | 1.0151 | 1.0143 |
| fib | 1.0000 | 1.1046 | 1.0732 | 1.0838 | 1.0607 | 1.0523 | 1.0251 | 1.0335 | 1.0293 |
| gc_churn | 1.0000 | 1.0009 | 1.0030 | 1.0002 | 1.0021 | 1.0027 | 1.0015 | 1.0018 | 1.0020 |
| heapsort | 1.0000 | 1.0410 | 1.0450 | 1.0350 | 1.0392 | 1.0445 | 1.0109 | 1.0138 | 1.0138 |
| json | 0.9976 | 1.0125 | 1.0082 | 1.0072 | 1.0068 | 1.0058 | 1.0028 | 1.0045 | 1.0042 |
| knucleotide | 1.0035 | 1.0226 | 1.0186 | 1.0175 | 1.0153 | 1.0208 | 1.0069 | 1.0076 | 1.0084 |
| load_big | 1.0001 | 0.9998 | 1.0002 | 1.0003 | 1.0002 | 0.9997 | 0.9996 | 1.0000 | 1.0001 |
| loops_arith | 1.0000 | 1.0525 | 1.0590 | 1.0358 | 1.0423 | 1.0590 | 1.0151 | 1.0234 | 1.0235 |
| mandelbrot | 1.0000 | 1.0506 | 1.0574 | 1.0421 | 1.0489 | 1.0574 | 1.0108 | 1.0151 | 1.0151 |
| matmul | 1.0000 | 1.0547 | 1.0635 | 1.0366 | 1.0454 | 1.0634 | 1.0137 | 1.0228 | 1.0228 |
| metamethods | 1.0000 | 1.0358 | 1.0346 | 1.0269 | 1.0282 | 1.0311 | 1.0251 | 1.0251 | 1.0238 |
| nbody | 1.0062 | 1.0315 | 1.0316 | 1.0346 | 1.0347 | 1.0371 | 1.0183 | 1.0199 | 1.0198 |
| oop_methods | 1.0157 | 1.0485 | 1.0379 | 1.0389 | 1.0352 | 1.0334 | 1.0177 | 1.0206 | 1.0189 |
| pcall_error | 1.0000 | 1.0110 | 1.0125 | 1.0077 | 1.0091 | 1.0101 | 1.0044 | 1.0061 | 1.0054 |
| queens | 1.0000 | 1.0505 | 1.0616 | 1.0346 | 1.0463 | 1.0602 | 1.0162 | 1.0240 | 1.0237 |
| sieve | 1.0000 | 1.0721 | 1.1002 | 1.0262 | 1.0542 | 1.1002 | 1.0372 | 1.0601 | 1.0602 |
| spectral_norm | 1.0000 | 1.0683 | 1.0599 | 1.0533 | 1.0483 | 1.0516 | 1.0133 | 1.0200 | 1.0183 |
| string_ops | 0.9942 | 0.9950 | 0.9957 | 0.9943 | 0.9954 | 0.9950 | 0.9934 | 1.0011 | 0.9937 |
| table_array | 1.0000 | 1.0023 | 1.0024 | 1.0014 | 1.0016 | 1.0021 | 1.0010 | 1.0015 | 1.0013 |
| table_hash | 1.0000 | 1.0146 | 1.0188 | 1.0104 | 1.0127 | 1.0186 | 1.0065 | 1.0093 | 1.0093 |
| varargs | 1.0014 | 1.0202 | 1.0133 | 1.0133 | 1.0088 | 1.0096 | 1.0028 | 1.0053 | 1.0049 |
| **geomean** | **1.0008** | **1.0370** | **1.0357** | **1.0271** | **1.0275** | **1.0329** | **1.0116** | **1.0163** | **1.0155** |
| **worst** | 1.0157 | 1.1046 | 1.1002 | 1.0838 | 1.0607 | 1.1002 | 1.0372 | 1.0601 | 1.0602 |
| call-heavy mean (fib, closures, oop_methods, varargs) | 1.0042 | 1.0650 | 1.0448 | 1.0504 | 1.0367 | 1.0344 | 1.0156 | 1.0210 | 1.0188 |

Breakdown (hooks of one kind only):

| benchmark | noloop_c | noloop_e | noentry_A | noentry_B | noentry_C |
|---|---:|---:|---:|---:|---:|
| binary_trees | 1.0332 | 1.0152 | 1.0144 | 1.0056 | 1.0062 |
| closures | 1.0635 | 1.0155 | 1.0275 | 1.0042 | 1.0095 |
| coroutines | 1.0119 | 1.0001 | 1.0209 | 1.0083 | 1.0226 |
| fannkuch | 1.0014 | 1.0000 | 1.0525 | 1.0176 | 1.0245 |
| fasta | 1.0112 | 1.0052 | 1.0295 | 1.0076 | 1.0090 |
| fib | 1.0858 | 1.0251 | 1.0272 | 1.0042 | 1.0084 |
| gc_churn | 0.9985 | 1.0012 | 1.0014 | 1.0002 | 1.0006 |
| heapsort | 1.0117 | 1.0006 | 1.0439 | 1.0104 | 1.0133 |
| json | 1.0075 | 1.0020 | 1.0028 | 0.9988 | 1.0003 |
| knucleotide | 1.0053 | 1.0036 | 1.0172 | 1.0035 | 1.0050 |
| load_big | 1.0000 | 0.9999 | 0.9999 | 1.0003 | 1.0003 |
| loops_arith | 0.9895 | 1.0000 | 1.0590 | 1.0151 | 1.0234 |
| mandelbrot | 0.9995 | 1.0000 | 1.0574 | 1.0108 | 1.0151 |
| matmul | 1.0229 | 1.0001 | 1.0633 | 1.0136 | 1.0226 |
| metamethods | 1.0302 | 1.0195 | 1.0116 | 1.0073 | 1.0073 |
| nbody | 1.0141 | 1.0003 | 1.0368 | 1.0119 | 1.0134 |
| oop_methods | 1.0522 | 1.0121 | 1.0213 | 1.0108 | 1.0156 |
| pcall_error | 1.0038 | 1.0030 | 1.0071 | 1.0014 | 1.0031 |
| queens | 1.0027 | 1.0017 | 1.0585 | 1.0148 | 1.0223 |
| sieve | 0.9773 | 1.0000 | 1.1002 | 1.0372 | 1.0601 |
| spectral_norm | 1.0349 | 1.0100 | 1.0416 | 1.0050 | 1.0100 |
| string_ops | 0.9929 | 0.9932 | 0.9953 | 0.9931 | 0.9945 |
| table_array | 1.0007 | 1.0004 | 1.0017 | 1.0006 | 1.0010 |
| table_hash | 0.9983 | 1.0015 | 1.0172 | 1.0049 | 1.0077 |
| varargs | 1.0111 | 1.0042 | 1.0081 | 1.0005 | 1.0039 |
| **geomean** | **1.0141** | **1.0046** | **1.0284** | **1.0075** | **1.0119** |
| **worst** | 1.0858 | 1.0251 | 1.1002 | 1.0372 | 1.0601 |
| call-heavy mean (fib, closures, oop_methods, varargs) | 1.0528 | 1.0142 | 1.0210 | 1.0049 | 1.0093 |

`fast` build (static relocation model, `scripts/build-fast.sh`; baseline = main's fast build; 5 runs, cpu 5):

| benchmark | off | A_c | A_e | B_e | C_e | noloop_e | noentry_B |
|---|---:|---:|---:|---:|---:|---:|---:|
| binary_trees | 1.0044 | 1.0373 | 1.0177 | 1.0176 | 1.0183 | 1.0158 | 1.0063 |
| closures | 1.0000 | 1.0747 | 1.0265 | 1.0207 | 1.0262 | 1.0163 | 1.0044 |
| coroutines | 1.0000 | 1.0288 | 1.0184 | 1.0189 | 1.0133 | 1.0001 | 1.0086 |
| fannkuch | 1.0000 | 1.0234 | 1.0335 | 1.0187 | 1.0261 | 1.0000 | 1.0187 |
| fasta | 0.9992 | 1.0216 | 1.0164 | 1.0131 | 1.0156 | 1.0055 | 1.0077 |
| fib | 1.0000 | 1.0855 | 1.0307 | 1.0307 | 1.0351 | 1.0263 | 1.0044 |
| gc_churn | 0.9999 | 1.0008 | 1.0028 | 1.0020 | 1.0023 | 1.0013 | 1.0006 |
| heapsort | 1.0000 | 1.0140 | 1.0177 | 1.0117 | 1.0148 | 1.0006 | 1.0111 |
| json | 1.0011 | 1.0123 | 1.0067 | 1.0040 | 1.0073 | 1.0023 | 1.0026 |
| knucleotide | 1.0034 | 1.0209 | 1.0111 | 1.0064 | 1.0122 | 1.0037 | 1.0080 |
| load_big | 1.0000 | 0.9997 | 0.9997 | 0.9997 | 0.9995 | 0.9995 | 0.9999 |
| loops_arith | 1.0000 | 1.0268 | 1.0337 | 1.0160 | 1.0248 | 1.0000 | 1.0160 |
| mandelbrot | 1.0000 | 1.0133 | 1.0205 | 1.0115 | 1.0160 | 1.0000 | 1.0115 |
| matmul | 1.0000 | 1.0242 | 1.0334 | 1.0145 | 1.0240 | 1.0001 | 1.0143 |
| metamethods | 0.9964 | 1.0293 | 1.0209 | 1.0219 | 1.0219 | 1.0165 | 1.0054 |
| nbody | 1.0000 | 1.0063 | 0.9992 | 0.9961 | 1.0042 | 0.9938 | 1.0023 |
| oop_methods | 0.9900 | 1.0311 | 1.0055 | 1.0026 | 1.0089 | 1.0076 | 1.0014 |
| pcall_error | 1.0000 | 1.0086 | 1.0085 | 1.0058 | 1.0075 | 1.0031 | 1.0028 |
| queens | 1.0000 | 1.0264 | 1.0368 | 1.0213 | 1.0293 | 1.0018 | 1.0195 |
| sieve | 1.0000 | 1.0615 | 1.0914 | 1.0424 | 1.0668 | 1.0000 | 1.0423 |
| spectral_norm | 1.0000 | 1.0406 | 1.0229 | 1.0159 | 1.0211 | 1.0106 | 1.0053 |
| string_ops | 0.9995 | 0.9955 | 0.9957 | 0.9945 | 0.9948 | 0.9937 | 0.9941 |
| table_array | 1.0000 | 1.0019 | 1.0017 | 1.0010 | 1.0014 | 1.0004 | 1.0006 |
| table_hash | 1.0001 | 1.0089 | 1.0131 | 1.0069 | 1.0099 | 1.0015 | 1.0055 |
| varargs | 1.0000 | 1.0182 | 1.0086 | 1.0054 | 1.0075 | 1.0029 | 1.0024 |
| **geomean** | **0.9998** | **1.0242** | **1.0187** | **1.0119** | **1.0163** | **1.0041** | **1.0078** |
| **worst** | 1.0044 | 1.0855 | 1.0914 | 1.0424 | 1.0668 | 1.0263 | 1.0423 |
| call-heavy mean (fib, closures, oop_methods, varargs) | 0.9975 | 1.0520 | 1.0178 | 1.0148 | 1.0194 | 1.0132 | 1.0031 |

## 5. Cycles (secondary, noisy)

Default build, same runs:

| benchmark | off | A_c | A_d | B_c | B_d | A_e | B_e | C_e | G_e |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| binary_trees | 0.9936 | 1.0759 | 1.0050 | 1.0807 | 1.0102 | 1.0144 | 1.0086 | 1.0232 | 1.0148 |
| closures | 0.9929 | 1.0456 | 1.0171 | 1.0247 | 1.0119 | 1.0183 | 1.0013 | 0.9971 | 1.0042 |
| coroutines | 0.9753 | 0.9838 | 0.9661 | 0.9656 | 0.9632 | 0.9629 | 0.9689 | 0.9515 | 0.9630 |
| fannkuch | 0.9961 | 1.0075 | 1.0097 | 1.0060 | 1.0137 | 1.0498 | 1.0380 | 1.0440 | 1.0814 |
| fasta | 1.0070 | 1.0276 | 1.0261 | 1.0048 | 1.0119 | 1.0090 | 0.9932 | 0.9991 | 1.0120 |
| fib | 0.9983 | 1.1205 | 1.0336 | 1.0363 | 1.0270 | 1.0206 | 1.0082 | 0.9970 | 1.0039 |
| gc_churn | 0.9792 | 0.9812 | 0.9869 | 0.9819 | 0.9754 | 0.9919 | 0.9776 | 0.9846 | 0.9720 |
| heapsort | 1.0034 | 0.9867 | 0.9994 | 0.9959 | 1.0225 | 1.0217 | 1.0012 | 1.0303 | 1.0291 |
| json | 0.9778 | 0.9767 | 0.9558 | 0.9474 | 0.9535 | 0.9665 | 0.9686 | 1.0574 | 0.9848 |
| knucleotide | 1.0725 | 1.0630 | 1.0585 | 1.0283 | 1.0363 | 1.0259 | 1.0299 | 0.9389 | 1.0232 |
| load_big | 0.9912 | 0.9886 | 0.9956 | 1.0071 | 1.0094 | 0.9958 | 1.0021 | 0.9932 | 1.0183 |
| loops_arith | 0.9990 | 1.0357 | 1.0376 | 1.0229 | 1.0357 | 1.0723 | 1.0758 | 1.0536 | 1.0904 |
| mandelbrot | 1.0425 | 1.0700 | 1.0745 | 1.0506 | 1.0688 | 1.1508 | 1.1129 | 1.0904 | 1.0508 |
| matmul | 1.0431 | 1.1177 | 1.1137 | 1.0947 | 1.1007 | 1.1355 | 1.0545 | 1.0425 | 1.0444 |
| metamethods | 0.9952 | 1.0188 | 1.0098 | 1.0334 | 1.0077 | 1.0159 | 1.0375 | 1.0013 | 1.0102 |
| nbody | 0.9924 | 1.0063 | 1.0057 | 1.0081 | 1.0064 | 1.0080 | 1.0145 | 1.0029 | 1.0047 |
| oop_methods | 0.9293 | 0.9956 | 0.9465 | 1.0009 | 0.9103 | 0.9103 | 0.9067 | 0.8948 | 0.8956 |
| pcall_error | 0.9956 | 1.0407 | 1.0284 | 1.0068 | 1.0257 | 1.0288 | 1.0451 | 1.0209 | 1.0371 |
| queens | 0.9856 | 1.0250 | 1.0078 | 1.0137 | 1.0066 | 1.0346 | 1.0269 | 1.0273 | 1.0628 |
| sieve | 0.9959 | 1.0088 | 1.0335 | 1.0052 | 1.0414 | 1.0562 | 1.0581 | 1.0290 | 1.0389 |
| spectral_norm | 0.9977 | 1.1226 | 1.0722 | 1.2124 | 1.1881 | 1.1586 | 1.0681 | 1.0720 | 1.1209 |
| string_ops | 0.9996 | 1.0095 | 1.0078 | 1.0119 | 1.0083 | 0.9996 | 1.0054 | 1.4866 | 1.0279 |
| table_array | 1.0011 | 1.0054 | 1.0033 | 1.0297 | 1.0043 | 1.0053 | 1.0080 | 1.0209 | 0.9990 |
| table_hash | 0.9737 | 0.9889 | 0.9875 | 0.9802 | 0.9872 | 0.9821 | 0.9815 | 1.0013 | 0.9904 |
| varargs | 1.0088 | 1.0142 | 0.9951 | 1.0027 | 0.9965 | 0.9959 | 0.9929 | 0.9974 | 0.9909 |
| **geomean** | **0.9975** | **1.0278** | **1.0144** | **1.0210** | **1.0157** | **1.0238** | **1.0146** | **1.0261** | **1.0179** |
| **worst** | 1.0725 | 1.1226 | 1.1137 | 1.2124 | 1.1881 | 1.1586 | 1.1129 | 1.4866 | 1.1209 |
| call-heavy mean (fib, closures, oop_methods, varargs) | 0.9818 | 1.0429 | 0.9975 | 1.0160 | 0.9853 | 0.9852 | 0.9764 | 0.9705 | 0.9725 |

`fast` build:

| benchmark | off | A_c | A_e | B_e | C_e | noloop_e | noentry_B |
|---|---:|---:|---:|---:|---:|---:|---:|
| binary_trees | 0.9778 | 0.9951 | 0.9601 | 0.9853 | 0.9429 | 0.9853 | 0.9307 |
| closures | 1.0632 | 1.1271 | 1.0759 | 1.0721 | 1.0303 | 1.0618 | 1.0548 |
| coroutines | 1.0148 | 1.0401 | 1.0364 | 1.0545 | 1.0156 | 0.9764 | 1.0478 |
| fannkuch | 0.9997 | 1.0304 | 1.0918 | 1.0793 | 1.0134 | 1.0732 | 1.0007 |
| fasta | 1.0052 | 1.0309 | 1.0360 | 1.0147 | 1.0188 | 0.9980 | 1.0012 |
| fib | 0.9931 | 1.0122 | 1.0009 | 1.0049 | 0.9933 | 0.9979 | 1.0127 |
| gc_churn | 0.9974 | 0.9944 | 0.9977 | 0.9955 | 1.0162 | 0.9913 | 1.0011 |
| heapsort | 0.9896 | 0.9662 | 1.0336 | 0.9913 | 0.9551 | 0.9851 | 0.9648 |
| json | 0.9943 | 0.9831 | 0.9753 | 0.9811 | 0.9719 | 0.9721 | 0.9722 |
| knucleotide | 0.9268 | 0.9683 | 0.9622 | 0.9264 | 0.9589 | 0.8988 | 0.9518 |
| load_big | 1.0061 | 1.0069 | 0.9822 | 1.0073 | 0.9939 | 1.0104 | 0.9975 |
| loops_arith | 0.9995 | 1.0008 | 1.1214 | 1.1252 | 1.0046 | 1.1614 | 0.9926 |
| mandelbrot | 1.0027 | 0.9900 | 1.1418 | 1.1761 | 1.0228 | 1.1626 | 1.0137 |
| matmul | 1.0014 | 1.0154 | 1.0171 | 1.0050 | 1.0022 | 0.9947 | 1.0064 |
| metamethods | 0.9917 | 1.0143 | 1.0084 | 1.0096 | 1.0003 | 1.0022 | 1.0089 |
| nbody | 0.9793 | 0.9814 | 0.9799 | 0.9982 | 0.9729 | 0.9852 | 0.9922 |
| oop_methods | 0.9033 | 0.9997 | 0.9745 | 0.8856 | 0.9849 | 0.9536 | 0.9037 |
| pcall_error | 0.9461 | 0.9516 | 1.0095 | 0.9496 | 0.9557 | 0.9749 | 0.9431 |
| queens | 0.9950 | 0.9886 | 1.0683 | 1.0116 | 0.9783 | 1.0411 | 0.9841 |
| sieve | 0.9908 | 0.9994 | 1.0886 | 1.0499 | 1.0249 | 1.0080 | 1.0270 |
| spectral_norm | 0.9820 | 0.9855 | 0.8696 | 0.9910 | 0.8692 | 1.0302 | 0.9724 |
| string_ops | 1.0076 | 1.0439 | 1.0268 | 1.0365 | 1.0282 | 1.0453 | 0.9999 |
| table_array | 0.9983 | 1.0121 | 1.0163 | 1.0039 | 1.0013 | 1.0115 | 1.0789 |
| table_hash | 0.9852 | 1.0046 | 0.9938 | 0.9833 | 1.0157 | 0.9988 | 0.9790 |
| varargs | 1.0032 | 1.0073 | 1.0032 | 1.0094 | 0.9912 | 1.0119 | 1.0048 |
| **geomean** | **0.9897** | **1.0055** | **1.0173** | **1.0123** | **0.9899** | **1.0118** | **0.9930** |
| **worst** | 1.0632 | 1.1271 | 1.1418 | 1.1761 | 1.0303 | 1.1626 | 1.0789 |
| call-heavy mean (fib, closures, oop_methods, varargs) | 0.9890 | 1.0353 | 1.0129 | 0.9906 | 0.9997 | 1.0056 | 0.9924 |

Cycles agree with the instruction picture where the signal is larger than the noise (fib, spectral_norm, mandelbrot,
matmul, loops_arith, sieve grow; the `oop_methods` 0.90-0.93 cells (they include the OFF binary) and `string_ops` C_e 1.49 are
layout/noise outliers, not effects of the hooks).

## 6. Analysis and recommendation

* **Budget.** The per-call hot-path cost cannot reach 0.5% of fib with any counter that lives in memory: the cheapest
  possible hook is one `sub $5, off(%proto)` + `jle` (placement (e), `noloop_e`): fib +2.5%, closures +1.6%, oop_methods
  +1.2%, varargs +0.4%. fib executes few instructions per call, so two extra instructions are visible. Placement (c)
  (the plan's `'frame` head) is the worst: it runs on every frame restart (calls and returns) and reloads the proto
  (`noloop_c`: fib +8.6%, closures +6.4%, oop_methods +5.2%); (d) is in between (`B_d` call-heavy +3.7%); (e) wins because
  the proto is already in a register in `pre_call_lua_fast`, the counter and the compiled-check are one instruction, and
  returns cost nothing.
* **Back-edges.** B (tick in the thread) costs 4 instructions per back-edge (`mov (%r11),%rax; decl 0x11c(%rax);
  mov %r12,0x10(%rsp) [pc spill store, hot path]; jne`): geomean +0.75%, sieve +3.7%. A needs the closure->proto chain
  (+2.8% geomean, sieve +10%); C needs the per-frame init and a `ci` reload (+1.2%, sieve +6%); G (static atomic) is
  identical to C in practice, no gain over B. No register-allocation cliff was observed in the final builds (the one
  anomaly, B/G with (e) at +7.5% on fib, was the unparked-counter artifact, section 2). `pc` is already
  kept in a stack slot by the baseline (PERF.md); the store shown above is the visible per-edge spill cost, no further
  spills were found.
* **Recommended J1 configuration (smallest cost, deterministic):** `jit_check_e` + `jit_counter_b`: entry counting and the
  compiled-ness test merged in `pre_call_lua_fast` (compiled protos parked at `hot <= 0`), back-edges tick the
  per-thread sampling counter (`Thread.jit_tick`, period 1000; the cold stub samples the running proto and bumps its
  own counters). Cost: instructions geomean +1.16%, call-heavy +1.56%, worst sieve +3.7% (default build);
  +1.19% / +1.48% / +4.2% (`fast`). Not within the 0.5% budget; the orchestrator must either accept it or choose one of
  the options below.
* **What (e) leaves to the J1 protocol.** (1) *Returns into a compiled caller*: the interpreter does nothing on return;
  compiled code that calls an interpreted callee (exit `FrameChanged`) must mark the caller frame so that the interpreter
  re-enters it on return, e.g. by setting `ci.trap` to a JIT-pending value (the `'frame` head already tests `trap != 0`
  for free and `fast_trap` is the cold dispatch point); if the mark is lost the frame simply continues in the interpreter
  (safe). (2) *Tail calls* (`pre_tail_call`) and metamethod frames pushed by `mm_frame` (they use
  `pre_call_lua_fast`, so they are counted) - tail calls are not counted in (d)/(e); recursion by tail call needs a
  hook in `pre_tail_call` if it matters. (3) *Entry from OSR*: loop-counter expiry (cold) calls the compile/enter path
  at `ci.savedpc`. (4) The hooked copy (`HOOK = true`) has no counters; it hands back to the fast copy at the next
  frame transition.
* **Options to get below 0.5% (not implemented, zero hot-path instructions by construction):**
  (a) *timer sampler*: a thread (or `setitimer`) sets a flag word that the existing `ci.trap != 0 || INTERRUPT` poll
  already tests at every back-edge and frame start; the cold `fast_trap` attributes the sample to the running proto.
  Needs `INTERRUPT` to become a small flag word (touches `rnlua`'s signal handler) and gives non-deterministic
  hotness, so tests use `RUSTYNAIL_JIT=always`. (b) *quickening* (J1b): counting opcode variants installed at load and
  patched to the plain opcode when the proto turns hot, so only warm-up pays; invasive (second opcode set, `luac -l`,
  `string.dump`, debuginfo symbolic execution). (c) fold the counter into `ci.trap` (a down-counter whose
  non-positive values mean "trap or expired"); needs every `trap = 1` site (vm.rs, state.rs, vm_unsafe.rs) changed.

## 7. Gates (feature ON)

* `cargo test --workspace --features rn-core/jit`, `... rn-core/jit_counter_b,rn-core/jit_check_e` and
  `... rn-core/jit_counter_c,rn-core/jit_check_d`: all test binaries `ok` (51 result lines per configuration, 0 failed).
* `cargo xtask official` with the features switched on through a scratch `default = [...]` in `crates/rn-cli/Cargo.toml`
  (restored afterwards; `tests/official-status.toml` restored with `git checkout`): **31/31** for `jit` (A + c) and for
  `jit` + `jit_counter_b` + `jit_check_e`.
* Feature OFF `.text` identity (default and `fast`): identical (`scripts/jit-hook-cost.sh text`).
* `cargo clippy -p rn-core --all-targets` (5.5) and `cargo clippy -p rn-core` with `--cfg rustynail_lua54` (5.4) for six
  feature combinations including OFF: no warnings; `cargo fmt --all --check` clean.
