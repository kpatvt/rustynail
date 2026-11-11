# rustynail JIT: parked and candidate work (index and ledger)

**Index (2026-11-11, after the session-release benchmark).** This file is the ledger of ideas that were rejected, parked, built but left
off, or never built, with the evidence for each. The index below lists what is still open or worth retesting, grouped by area, with
the section of the ledger that holds the detail and the numbers. Shipped defaults are in `crates/rn-jit/src/tr/cache.rs` (`*_PROCESS`
constants) and in the knob table of `docs/JIT.md`; where this ledger's older sections disagree with the code (the K1 to K3 sections
below record which knobs were deleted or hard-wired), the code wins. Release numbers: `bench/results/session-release/report.md`.

| Area | Candidate / state | Ledger | Why it is parked |
|---|---|---|---|
| Second mid-tier JIT (Maglev-style, "J4F") | NO-GO (project decision); no written analysis of it is kept in this repository | -- | the J4 gates (`docs/JIT_BENCH.md`) and the interpreter study (`docs/INTERP_STUDY.md`: even an infinitely fast interpreter gains 11.7%) show the remaining time is in trace quality and library calls, not in a missing tier |
| Nested loops | `nest=2` (linking exits from inlined frames), `nest=0`; data-dependent callee loops under the `loopany` guard (`lua_lexer`); side-recording nest links (`sidenest`, shipped) | A3, A25, C10, C38 | `nest=2` and a lower `stitchmin` did not pass the 3% gate; `lua_lexer` stays 2.6x LuaJIT |
| Lazy `CallInfo` frames | `lazyci` (L1) built, OFF; L2 (callee-identity hoist) and L3 (transition diet) not built | C55 | L1 misses its gate (`fib` 0.92, `binary_trees` +6% cycles); L2/L3 only pay on top of an L1 that is on |
| Array loops | `aidx` and `bhoist` built (`bhoist` on, `aidx` off); single loop variable, dead-slot snapshot pruning, folded memory operands not built | C56 | `matmul` is cache-bound; no free register for `aidx` to pay |
| Coroutines | no trace crosses `coroutine.yield`/`resume`/`wrap` (a recording that reaches them aborts `CFunc`); resume/yield fast path; coroutine bodies are traced between yields | C9 | one micro program at 2x LuaJIT (`lib/co_yield`), estimated -1..2% geomean |
| Library calls on trace, left | `table.concat`, `string.format` with float/`%g`/`%q`/`%5.2f` items, `table.sort`, general `gsub`/`gmatch`/`match` shapes, `tonumber(s, base)`, positional `table.insert`/`table.remove` (with a position argument: stays a plain C call) | C61, C37, C48, C49 | each is a few percent of the cycles of the programs that hit it (C61 census); C37 (a generic recorded C call) stopped at design |
| `lib/m_random` | `math.random(m, n)` inline generator step (238 instructions per call; 6.4x LuaJIT instructions) | C61 (R5) | 3 of 223 programs use it; needs C-closure upvalue loads in the IR |
| `lib/str_lt_short` | inline short-string `<` / `<=` (byte-swap micro-op in IR, encoder and emulator) | C61 (R6), C54 | one micro benchmark |
| PGO slowdown | `lib/char` +48% cycles under PGO (178 to 263 Mcyc at 0.897x instructions, IPC 4.94 to 3.05, cause not diagnosed); `lib/m_type`, `os_clock`, `tonumber_int`, `knucleotide` +3 to +7% | `bench/results/session-release/report.md` | open: needs a perf-counter diagnosis (branch or front-end stalls); a held-out/profile-mix experiment would show whether `lib/char` is trained at all |
| Lua 5.4 gaps (session-release, 5.4 is 16% less effective relative to PUC than 5.5) | `rawget`/`rawset`/`next` not recorded on 5.4 (`lib/rawget_set` 11.5x relatively, `it_next` 2.2x) | `report.md`, `extra.md`; C61 | `CFunc` abort in the 5.4 recorder; fixable, not done |
| | inline `#t` off on 5.4 (`len_tbl` 1.8x, `tinsert_append` 1.5x, `tpack` 1.4x) | same | 5.4's `luaH_getn` moves `alimit` under different rules (VA-8 in `docs/RELEASE.md`) |
| | integer-key hash-part access and `__index`/`__newindex` array access off (alimit bookkeeping): `tfront_1k` 8.6x, `int_rec2` 3.4x, `sieve` 3.0x, `tmove` 2.7x | same | `alimit` side effects of `get_int_arr` (V2c in `docs/RELEASE.md`) |
| | metamethod-heavy programs (`class_middleclass` 1.04x PUC 5.4 vs 0.16x on 5.5, `metamethods` 0.95x vs 0.16x), `programs/load_big` 2.4x | same; C21, C35, C60, C62 | RETF is inert on 5.4 and the metamethod bundles do not seem to take effect; not root-caused; the largest recoverable item |
| Tail-call and recursion gaps to LuaJIT (5.5) | `fib` 3.1x, `raytrace` 3.0x, `classic_vector` 2.5x, `tail_loop` 2.5x, `cd` 2.2x, `towers` 2.0x | C4, C5, C23, C58 | recursion frame redesign judged below the gate; see the report's gap list |
| Interpreter and build-level | interpreter rewrite / threaded dispatch NO-GO; BOLT on top of PGO no gain; `x86-64-v3` no gain | C31, C32 | measured |

**Status classes used below.** BUILT-OFF (code in the tree behind a knob that is off), BUILT-REVERTED, CEILING (not built: upper bound
too low), NOT-MEASURED, SUPERSEDED (parked once, shipped or revisited later: see the history table at the end).

---

# Ledger (draft of 2026-11-07, extended through 2026-11-14)


Covers ideas that were rejected, parked, built but left off by default, not built because the ceiling was too low, or judged no-go,
across J4 (tier-1/tier-1.5) and J5 (trace compiler). Mined read-only from the repo at HEAD `65e69653` plus its working tree.
Nothing was run.

## How to read this

**Citations.**
- `REF:<line>` = `docs/JIT_TRACE_REF.md` at HEAD.
- **`REF0:<line>` = `git show fec75e2b^:docs/JIT_TRACE_REF.md`.** Commit `fec75e2b` (BUNDLE-G1, "docs section") deleted about 2100
  lines from JIT_TRACE_REF.md: the sections P-J, P-N, P-X, P-V, P-W, P-Y, P-M4, P-O, P-PROF, N1..N7, BD, R1, A1, S1, M1, F1(gap review),
  H1, R2, B2, F2, T2, C2, T3, P2, BUNDLE-1..5, BUNDLE-A..F, BUNDLE-P and the head of BUNDLE-G1 (see `git show --stat fec75e2b`: 2126 lines
  changed in that file). The deletion looks accidental. Those decisions now exist only in git history. Restoring them is a cheap fix
  but outside this read-only task.
- `cache.rs:<line>` = `crates/rn-jit/src/tr/cache.rs` in the **working tree**. It has one uncommitted change: `POLYSIDE_PROCESS` is 0
  at HEAD and 48 in the working tree (see A6).

**Status classes.**
- **BUILT-OFF**: the code is in the tree behind a knob that is off by default.
- **BUILT-REVERTED**: built and measured, then removed. Some survive as a patch.
- **CEILING**: not built. Judged on a measured or estimated upper bound.
- **NOT-MEASURED**: argued only, or only partly sized.
- **SUPERSEDED**: parked once, then shipped or revisited later (see the history table at the end).

**The gate.** Nearly every decision uses one rule: at least 3% geomean on the item's target programs, and no program more than 5%
worse. The exception is layout noise, where instruction counts are unchanged and the duplicate column is unchanged.

**Benchmark sets.** As named in the docs:
- `programs` = bench/programs (25)
- `jit-trace` = bench/jit-trace (27-29)
- `lib` (91-95)
- `awfy` (14)
- `apps` (5)
- `real` (5)
- `corpus` = bench/corpus (18 third-party-style programs). It exists only since BUNDLE-N (`29f4f0fe`), so **every decision taken before
  BUNDLE-N was judged without the third-party corpus.**

Column conventions:
- "plain" / "PGO" are the build types.
- `warm=0` means no start gate in the A/B. The default mode uses the process work gate `wgate=30000`.

**Cheap retest.** A knob string for `RUSTYNAIL_JIT_TRACE_LIMITS` on the current binary. Most A/Bs in the docs also added `warm=0` and
`RUSTYNAIL_SEED=0` for determinism.

Two parser gotchas (`cache.rs:808-1001`, as of the ledger's HEAD): process defaults were skipped when the limits string *contained the substring* of the knob name.
- Naming `deepins=` or `deephop=` alone switched `deep` (process 8) back to its struct default 0.
- Naming `monwarm=` counted as naming `warm`, which turned the work gate off (`cache.rs:863-866`).
- **Fixed in K1 (2026-11-12):** `cache::names_limit` matches the key of an item `key=value` exactly (test `tr_policy::k1_names_limit_is_an_exact_key`).

## K1: knobs deleted from the tree (2026-11-12; step 1 of the knob pruning)

Behaviour-preserving: every knob below had a default-OFF process value and was measured not worth shipping (or never did anything). The OFF
state is the only state left; the code of the ON state, its tests and its mutants are gone. Last commit that has each of them: `ca12cb50`
(`git show ca12cb50:crates/rn-jit/src/tr/cache.rs` etc.); the idea is recoverable from there. The ledger entries below keep their text as history.
Parser consequence: naming a deleted key in `RUSTYNAIL_JIT_TRACE_LIMITS` is now an unknown option (a warning at `RUSTYNAIL_JIT_VERBOSE >= 1`, otherwise ignored).

| Knob | What it was | Ledger | Also removed |
|---|---|---|---|
| `vargtr`, `vargt1` | placeholders for VARARG in traces / tier-1 vararg copy: copied into `RecCfg`, never read | (BUNDLE-L D2) | `VARGTR_PROCESS`, `VARGT1_PROCESS` |
| `fnloops` | function-entry heads for functions with loops; `fn_loops = fnloops \|\| recany` and the process sets `recany=1`, so a no-op | M1 36eae15d | (`fn_loops` is now `recany != 0`) |
| `pchk` | probe shadow check (diagnostic) | A20 | `RecCfg::scout_shadow`, the `[probe-check]` stderr line |
| `probe` | probe recordings (`scout_step`, `scout_call`, `AbortReason::ProbeRestart`, `AbortKind::Restart`, `FN_SHORT` / `NO_SCOUT` tables, `TrStats::probes_run` / `probe_restarts`, the `[trace] probe recordings` line) | A19 BUNDLE-M C2 25ec2afe | mutants `probe_never`, `scout_{recursion,depth,native}_ignored` |
| `onestrike` | one strike for unchangeable aborts (`on_abort_strike`, `one_strike_reason`) | A21 BUNDLE-M C4 79e4df92 | mutants `onestrike_never`, `onestrike_level2_as_1` |
| `recgate`, `recdead` | per-prototype recursion gate (`RecGate`, `rec_gate_why`, `rg_check`, `Begin::Gated`, `TrStats::gated_*`, the `[trace] recursion gate` line, the `RecGate` side-exit refusal) | A12, A13 BUNDLE-J2 a99f1d9e | mutants `recgate_{never,head_open,side_open}` |
| `pollbud` | POLL folded into the stay budget (`stay_quantum`, `AsmEnv::poll_fold`, `Cx::poll_from`, `POLL_QUANTUM`) | A23 BUNDLE-4 8d18c4be | mutants `pollbud_{entry_skip,budget_counted}` |
| `sinkdemote`, `sinkrate`, `sinkpol` | count / TSC-rate based sunk-shape demotion and its umbrella (`note_materialised*`, `PENDING`, the exit-handler `take_pending` flush). `sinkroot` keeps `DEMOTED`, `demote_shape`, `sink_demoted` | A10, A11 H3-G / BUNDLE-L D3 d3c8c5d3 | mutants `sink_demote_never`, `sinkrate_{never,inverted,window_skip}`, `d3_default_ignored` |
| `hopcyc` | TSC hop-rate rule (nondeterministic; off since BUNDLE-P). `hoptop` stays | C26 | `hop_rate_due`, `HOP_CYCLES`, `HopSt::{t,e}`, mutant `hop_clock_default` |
| `ubar`, `absmiss`, `ckey` | upvalue store barrier in traces (`TBAR aux=1`, forward barrier in `jit_tr_barrier`), absent-key loads as guarded misses (`int_absent`, the non-`dynmask` `hash_absent_dyn` form), `CONCAT`-built keys via the computed-key path. `dynmask` keeps the `HREF_ABS` walk | A14, A15, A16 BUNDLE-L D1 | mutants `ubar_{skip,back}`, `habs_lsize_skip`, `iabs_{array_guard,meta}_skip`; test `tr_asm::e2e_ubar_in_gc_cycles` |
| `centry` | C-called Lua functions enter compiled code (`fresh` argument of `hot_call_plain`) | C40 BUNDLE-B(2) 17a24ba5 | (no test, no mutant) |
| `polygk`, `polyrate` | guard-kind and exits-per-entry filters of `polyside` (`family_exit_rate`, `GK_DISPATCH`) | A6 BUNDLE-O | mutant `polyside_any_exit` |
| `framesides` | side traces rooted at the innermost restored frame of fn-root families (`fn_family`) | A2 P-E2 249e7cca | (the `fcfn` conditions lose their `framesides == 0` term) |
| `uvroll` (bit 1 of `uvsep`) | rolled traces past header-mutating ops keep upvalue facts (`Em::{uvroll,dirty}`, `Builder::cse_pure_only`, `OptStats::uvroll_cut`); `uvsep` is now 0/1 | C43 P2 bundle 2 4ce16b7a | mutants `uvroll_{never,fload_cse,all_loads,state_libs}`; tests `tr_loop::uvroll_*` |

Kept although the inventory listed them as candidates: nothing in the K1 list was used by a shipped default path or by anything ON. Notes:
`habs_found_ignored` stays (the `HREF_ABS` walk is `dynmask`'s; detector `tr_core::dm_programs`; `l_d1_programs` now runs with `dynmask=1`), and
`sink_demote_ignored` stays (the demoted-shape set serves `sinkroot`; its detector moved from `j1_sinkdemote_policy` to `tr_core::sinkroot_programs`).
Not touched: `hdrlib` 3/4 (an ON knob's sub-values), `xregs`, `pollr15`, `sinkside`, `sidenest`, `fnroot`, `warm`, the cargo features.

## K2: knobs hard-wired ON (2026-11-12; step 2 of the knob pruning)

Behaviour-preserving: each knob below was ON in the `TrLimits::default()` struct (so ON in every test and every process), never turned off by a shipped
configuration or a process-default constant, and was a switch, not a tuning number. The ON state is now unconditional; the field, the parser entry
and the tests that only exercised the OFF state are gone. Last commit that has each of them: `b3a75d85` (`git show b3a75d85:crates/rn-jit/src/tr/cache.rs`).
Parser consequence as in K1: naming a removed key in `RUSTYNAIL_JIT_TRACE_LIMITS` is an unknown option (a warning at `RUSTYNAIL_JIT_VERBOSE >= 1`, otherwise ignored).
Where the knob was copied into a recorder / backend configuration struct (`RecCfg`, `AsmEnv`), that field stays: `RecCfg::default()` is all-off for the
recorder unit tests (`tr_record*`, golden dumps), so those flags are recorder test configuration; the pipeline (`tr::mod`) sets them to `true`.

| Knob | What it was | Origin | Now |
|---|---|---|---|
| `reccont` | native returns / continuation heads together with `recnat` | P-M3 | `rec_conts = recnat != 0` (test row `recnat=1,reccont=0` dropped) |
| `pack` | record `table.pack(fixed n)` | P-E2 bd42e72d | `RecCfg::pack = true`; the `fnroot=0,pack=0` test limits lost `pack=0` |
| `gfor` | record `ipairs` / `pairs` generic for | N7 426568a2 | `RecCfg::gfor = true` |
| `mm` | Lua metamethods as inlined frames | N3 31b7f67e | `RecCfg::mm = true` |
| `setmt` | `setmetatable` as `SETMT` intrinsic | N3b 4c79b06d | `RecCfg::setmt = true` |
| `tailnat` | `return native(...)` in an inlined frame | N3b | `RecCfg::tailnat = true` |
| `closeret` | close-flag `RETURN` in an inlined frame | M1 36eae15d | `RecCfg::closeret = true` |
| `tailx` | `TAILCALL` of `__call` tables / vararg callees (mask 3) | BUNDLE-N aea3ee71 | `RecCfg::tailx = 3` (script `bench_n_abl.py`, which only varied it, deleted) |
| `idxdeep` | deep `__index` closures, `rawget`, mm frame repeat (mask 7) | BUNDLE-N | `RecCfg::idxdeep = 7` |
| `gforinl` | unroll generic for of inlined callees | M1 8e9315ea | `RecCfg::gfor_inl = !lua54` |
| `gforlua` | generic for over Lua closure iterators | BUNDLE-Q 1c6198be | `RecCfg::gfor_lua = true` |
| `depthstitch` | the inlining depth limit stitches | M1 8e9315ea | `RecCfg::depth_stitch = true` |
| `nilload` | record nil array elements | M1 | `RecCfg::nilload = true` |
| `sidestitch` | side recordings stitch at the recursion limit | M1 | `RecCfg::side_stitch = true`; the side-recording `unroll` is always `unroll + ubump` (`sidestitchmin` stays a tuning knob) |
| `nrinline` | inline `NEWREF` of constant keys into own tables | A1 358960d5 | `RecCfg::nrinline = true` |
| `setmtinl` | inline `setmetatable` store | A1 84ba7e54 | `AsmEnv::setmt_inline = true` |
| `f1pow`, `f1math`, `f1strlen`, `f1floor` | `x^2`, sin/cos/exp/log + n-ary min/max, inline `string.len`, inline `roundsd` | F1 3468f332 | `RecCfg` flags `true` (`f1floor` still needs SSE4.1 at run time) |
| `monwarm` | first sampling tick enters tier 1 at call loops (value 2 = every such loop) | R1 0c2f0394 | unconditional in `rt::loop_enter_hook` (the trace-off mode check stays) |
| `tailfast` | tier-1 inline `TAILCALL` | R1 | unconditional with traces on (`tail_fast`) |
| `pcall` | `pcall` / `xpcall` as trace construct | P-O d566fa49 | `RecCfg::pcall = true` |
| `sideframes` | S1 frame-cell side traces | S1 686b2b81 | the term is gone from `exit_hot_policy` |
| `fcfn` | frame cells for fn-root / recursion traces | H1 5d5280ab | `Active::fcfn` removed; mutants `fcfn_cells_off`, `fcfn_family_excluded` keep injecting |
| `fmat` | inline frame-materialising exit stub | H2 b21c26d5 | `AsmEnv::root_code` is always the root's code pointer (`ExitMeta`); a test row `fmat=0` (handler) dropped |
| `sidefwd` | forwarding / CSE on side and stitched traces | H1 | `Active::sidefwd` removed; mutant `sidefwd_off` keeps injecting |
| `landcache` | cache the tier-1 block structure per prototype | P-G 6c2e3ecd | always cached in the pipeline (`RecCfg::landcache = true`, the cache fill in `rt::compile` unconditional); test `pg_landcache_is_transparent` removed (the debug-build assertion in `compile` still compares the cached structure with a fresh lowering) |

Kept although ON in the struct:
- Debug / oracle: `lockstep` (the recorder cross-check; ON in the struct, process 0, forced by `+verify`); `nest` (0 and 2 are the measured alternatives, A3).
- Tuning numbers: `maxside`, `maxdepth`, `unroll`, `stitchmin`, `loopunroll`, `fnmin`, `loopanymax`, `sidestitchmin`, `fck`, `framemin`, `deepins`, `deephop`, `maxtrace`, `maxrecord`, `maxsnap`.
- ON in the struct, different process default (left for K3a, done there): `lockstep` (1 / 0, kept: an assert-only oracle), `maxside` (64 / 16, now 16 everywhere). None of the 28 hard-wired knobs had a process constant.
- `RecCfg` / `AsmEnv` flags: kept as recorder / assembler test configuration (see above), including `AsmEnv::root_code == 0` (the unit tests of `tr_asm` build the assembler without a root).
- Mutants: none removed (every mutant of these features injects into code that stays live: `RecCfg` paths, `fcfn_*`, `sidefwd_off`, `fmat_*`, `tailfast_*`); the registry count and order are unchanged.

## K3a: tests run the shipped configuration (2026-11-12; step 3a of the knob pruning)

`TrLimits::default()` (the struct default every test starts from) now equals the shipped configuration: every knob has the value of its `*_PROCESS` constant (`tr/cache.rs`). Before K3a
about 56 knobs were OFF in the struct and ON in a process, so the default `cargo test` exercised the OFF paths and the shipped paths ran only under `TR_CORE_PROCESS=1` (with 13 known
failures). Production behaviour is unchanged (gate in docs/PERF.md "K3a"): a process starts from the same values as before, `apply_process_limits` only has to override what differs.

* **Changed struct defaults (65 knobs, old value 0 unless noted)**, each now its `*_PROCESS` value: `maxside` (64 to 16) `polyside` (48) `polywait` (2) `polyframes` (2) `recnat` `recany` `siderooms` `nilstore` `loopany` `laprune` `cispare` `g1fmt` `t3idx` (2) `strself`
  `t1inline` (1023) `dynmask` `ineffcover` `typesides` `hoptop` (98) `second` (3) `sbump` `ccalls` `stitch0` `chaindepth` (4) `fnpromo` (6) `fnpromow` (3) `innerretry` (6) `domtop` (82) `domn` (65536) `ncallee` `chainre` (95) `chainn` (65536) `tset` (15)
  `allocguard` (was set to 1 by `TrConfig::from_env`) `tailhook` `hdrlib` (2) `cnum` `sinkmt` `sinkroot` (2) `sinkstub` `b2arr` `b2mt` `b2tag` `b2hk` `nopay` `divmagic` `hkfuse` `hk64` `deep` (8) `f2rand` `gmatch` `f2byte` `f2asrt` `f2raw` `f2mt` `f2cmp` `libsub`
  `libnum` `libnext` `libfind` `uvsep` `budsub` `pollr15` (2) `fpool` `rearm` (4). Unchanged because their shipped value is 0: `armm sbump2 sidenest fibdiet sinkside xregs t1ccall tfmt rearmside`.
* **Not equal on purpose** (`apply_process_limits` sets them in a process unless the limits string names them): `sidemin` (start gate: 0 in tests, 4 in a process; the twin programs are tiny), `wgate` / `warm` (the work-budget start gate:
  0 in tests, `WGATE_PROCESS` in a process) and `lockstep` (the recorder cross-check: an assert-only oracle that does not change what is recorded; 1 in tests, 0 in a process, forced by `+verify`).
* **`allocguard`** moved from `TrConfig::from_env` into the struct default (1).
* **Mutants.** The `*_default_ignored` mutants now inject into `TrLimits::default()` (the field gets its old struct value) instead of `apply_process_limits`; their detectors in `tr_policy` assert the struct default and the parsed value
  and still catch them. Registry unchanged at 506.
* **`TR_CORE_PROCESS=1`** is kept as an alias; it now differs from the default in `lockstep` only.
* **Where a test wants the OFF path** it names the knob in its limits string (`fn_root_short_recordings_blacklist_the_head`: `fnpromo=0` plus a shipped row; `dm_growth_exits_are_bounded`: `typesides=0`; `p3_chain_programs`: `chaindepth=0` plus a
  shipped-config control row; `po_poly_programs`: `ncallee=0,polyframes=0`; `sinkroot_programs`: `second=0`; `side_traces_install_and_run` (`side_untraced_head`): `recnat=0,recany=0`; `mixed_string_loops_are_not_flushed_as_ineffective`:
  `hdrlib=0,typesides=0` rows for the strict exit bound; `m1_interp_shapes`: a `maxside=64` row for the old 65-side-trace threshold; the head-counting suites `tr_runtime`, `tr_exit`, `tr_policy` (nest tests): `recnat=0,recany=0` in their config helper;
  `twins_options`: the `+tracemove` job of three recursion-to-the-limit programs). Details in docs/JIT_TRACE_REF.md "K3a".
* **Test-support bugs found** (not production): `tr::asm::support::tier1_pc_addrs` re-derived the tier-1 dispatch without `recnat` / `recany` (`DispatchCfg`), so every `tr_asm` e2e test failed under the shipped config; the `+emuverify` probe copied only the budget word
  and exit area of `BudgetHdr`, not the `fpool` constants (wrong float results in the emulation) and could not follow the far `[r15 + disp]` poll word of `pollr15` (segfault): the copy is the whole `BudgetHdr` and `+emuverify` traces poll through the absolute address.

## K3b: mature process-ON knobs hard-wired (2026-11-09; step 3b of the knob pruning)

Behaviour-preserving (gate: docs/PERF.md "K3b"). After K3a the struct default equals the process default, so a knob that is ON there is ON in every test and every process. Hard-wired here: knobs that are ON in the shipped
configuration, were never turned off by a shipped configuration, are switches (not numbers, start gates or alternative modes) and whose OFF state no test needs as a differential oracle. The ON state is unconditional; the field, the parser entry, the `*_PROCESS`
constant, the `*_default_ignored` mutants that injected into the removed struct default and the tests / rows that only exercised OFF are gone. Last commit that has each of them: `ec084529` (`git show ec084529:crates/rn-jit/src/tr/cache.rs`).
Naming a removed key in `RUSTYNAIL_JIT_TRACE_LIMITS` is an unknown option (a warning at `RUSTYNAIL_JIT_VERBOSE >= 1`, otherwise ignored). As in K2 the `RecCfg` / `AsmEnv` / `TraceOpts` flags stay (`RecCfg::default()` is all-off for the recorder unit tests,
`tr_asm` builds the assembler by hand); the pipeline sets them to `true`.

| Knob | What it was | Origin | Now |
|---|---|---|---|
| `cnum` | `CONCAT` takes number operands itself (no `SNEW` per number) | B3-4 | `RecCfg::cnum = true`; test `b3_cnum_programs` runs the shipped row only |
| `g1fmt` | `string.format("%s", s)` folded to `s` | BUNDLE-G1 | `RecCfg::g1fmt = true` |
| `f2rand`, `f2byte`, `f2asrt`, `f2raw`, `f2mt`, `f2cmp` | integer `math.random`, inline `string.byte`, `assert`, `rawget` / `rawset`, `getmetatable`, string comparisons recorded | F2 | `RecCfg` flags `true` (`f2_programs` loses its all-off control row; its `CFunc` / `Nyi` abort count is now an absolute bound, 15) |
| `gmatch` | the `gmatch` iterator step recorded | BUNDLE-GM | `RecCfg::gmatch = true` (absolute bound on `CFunc` aborts: 38) |
| `libsub` | `LIB sub` calls the lean entry `jit_tr_lib_sub` | HDRSUB | `AsmEnv::lean_sub = true` (rows `hdrlib=2,libsub=0` dropped) |
| `libnum`, `libnext`, `libfind` | `tonumber(string)`, `next(t, k)`, plain `string.find` recorded | BUNDLE-A | `RecCfg` flags `true` (`ba_knobs_change_the_traces` now asserts that traces install and are entered) |
| `b2arr`, `b2mt`, `b2tag`, `b2hk` | scaled array addressing, metatable-free non-nil array read, known-tag store skip, `HREFK` scaled key | B2 | `AsmEnv::{b2_arr,b2_tag,b2_hk}` and `RecCfg::b2mt` `true` |
| `nopay` | tag-only stores of nil / false / true | B4-1 | `AsmEnv::nopay = true` (mutant `nopay_all_types` stays) |
| `divmagic` | magic-number `//` and `%` by constants | B4-4 | `AsmEnv::divmagic = true` |
| `hkfuse`, `hk64` | `HREFK` + `HLOAD` one 16-bit tag compare, 64-bit key compare | B4-3 | `AsmEnv::{hk_fuse,hk_key64} = true`; `j1_hkfuse_fires` loses its `hkfuse=0` row |
| `uvsep` | disjoint upvalue cells for forwarding under run-time `NE` guards | P2 bundle 2 | `RecCfg::uvsep = true`, `TraceOpts::uvsep = 1`; the harness variable `TR_CORE_UVSEP` is gone |
| `budsub`, `fpool` | `sub r, n; jl` budget with refunding stub, double constants from the per-state pool | P2 bundle 3 | `AsmEnv::budget_sub = vbreg.is_some()`, `AsmEnv::fpool` always the pool address (`LOOPDIET_PROCESS` gone, `pollr15` stays a knob) |
| `strself` | tier-1 `SELF` on a string receiver | BUNDLE-K | `str_mt_addr` no longer reads the knob (`t3idx != 0` still gates it) |

Removed mutants (registry 506 to 499): `f2_default_ignored`, `gm_default_ignored`, `libbundle_default_ignored` (its `t1ccall` / `tfmt` lines were no-ops: both default to 0), `b2_default_ignored`, `b4_default_ignored`, `uvsep_default_ignored`, `k_default_ignored`.
Every other mutant of these features injects into code that stays live (recorder / assembler paths) and was re-run: library family 36 caught, backend family 13 caught, `strself_*` 3 caught.

Kept although ON in the shipped configuration (reason):
* OFF is an A/B control or differential oracle that a detector needs: `ccalls` (`bd_ccalls_programs` compares entries and exits of the inline check with the helper: it detects `ccalls_inline_off_by_one`), `dynmask` (the `dynmask=0` form of `HREFV` is where `hrefv_mask_off_by_one` and
  `hrefv_meta_skip` live: K3b part 1 added `dynmask=0` rows), `sinkstub` (`sinkstub=0` is the handler path of `sink_mem_error_dropped`), `typesides` (`typesides=0` is the row that detects `tnext_key_site_type`), `sinkmt` (`sinkmt=0` makes the `SETMT` helper live),
  `laprune`, `cispare`, `stitch0` (their detectors assert the stat deltas OFF vs ON: `p_laprune_programs`, `c36_cispare_programs`, `bf_stitch0_programs`), `recnat`, `recany`, `siderooms`, `nilstore`, `loopany`, `ncallee`, `ineffcover` (the head-counting suites and
  the shape tests name the OFF value in about 20-50 limit strings each: the head counts that native recursion / unrolling would change), `hdrlib` 0 (rows added for `strmt_flush_skip`), `tailhook` (0 / 1 / 2 are modes), `allocguard` (a documented user-facing switch for a pacing heuristic).
* Value 0 in the shipped configuration (documented fallbacks, not ON): `hdrlib` 3 / 4, `xregs`, `armm`, `sbump2`, `sidenest`, `fibdiet`, `sinkside`, `t1ccall`, `tfmt`, `rearmside`. (BUNDLE N-B: `sidenest` and `tfmt` are ON now, `retf` shipped as 2; since the RETF flip of 2026-11-10 `retf` ships as 3, bit 2 OFF.)
* Numeric tuning knobs and start gates (K4 or never): `maxside`, `polyside`, `polywait`, `polyframes`, `t3idx`, `t1inline`, `hoptop`, `second`, `sbump`, `chaindepth`, `fnpromo`, `fnpromow`, `innerretry`, `domtop`, `domn`, `chainre`, `chainn`, `tset`, `sinkroot`, `deep`, `pollr15`, `rearm`, `lockstep`, `sidemin`, `wgate`, `warm`.

## Summary table

| # | Entry | Knob / string that turns the idea on | Process default | Status | Origin (bundle / commit) | Cheap retest |
|---|---|---|---|---|---|---|
| A1 | Generic function-entry roots | `fnroot=1` (`fnmin`) | 0 | BUILT-OFF | P-E2 249e7cca, 13269f79 | YES |
| A2 | Side traces rooted in inlined frames of fn roots | `framesides=1` | 0 | BUILT-OFF | P-E2 249e7cca | YES (disables `fcfn`) |
| A3 | Linking exits from inlined frames / no nests | `nest=2` / `nest=0` | 1 | BUILT-OFF | F2b | YES |
| A4 | Lower stitching threshold | `stitchmin=4` | 32 | BUILT-OFF | F2b, F4 | YES |
| A5 | Raise / lift the side-trace family cap | `maxside=24` | 16 | BUILT-OFF (value) | BUNDLE-B 63841cab, BUNDLE-O | YES |
| A6 | Polymorphic side traces past the cap | `polyside=48,polywait=2,polyframes=2` | 0 at HEAD, **48 in working tree** | BUILT-OFF (committed) | BUNDLE-O 65e69653 | YES |
| A7 | Smaller side-trace minimum (O1) | `sidemin=0` / `sidemin=2` | 4 | CEILING (knob exists) | P-F, BUNDLE-O O1 | YES |
| A8 | Sinking of metatabled tables (`sinkmt`: SHIPPED, SINK-F) and in side traces (`sinkside`: still off) | `sinkmt=0` to compare; `sinkside=1` | `sinkmt` 1, `sinkside` 0 | SUPERSEDED (`sinkmt` shipped), `sinkside` BUILT-OFF | BUNDLE-2 179f8731, BUNDLE-5 b22ef012, E, G1, H3-G, SINK-F | YES |
| A9 | Inline materialising exit stub | `sinkstub=0` to compare | 1 | SUPERSEDED (shipped, SINK-F) | BUNDLE-J1 92f527aa (E item 1 sized), SINK-F | YES (`sinkstub=0`) |
| A10 | Count-based sunk-shape demotion | `sinkdemote=1024` | 0 | BUILT-OFF | H3-G patch, J1 | YES |
| A11 | Rate-based per-site sinking policy | `sinkpol=1` (`sinkrate=2500`) | 0 | BUILT-OFF | BUNDLE-L D3 d3c8c5d3 | YES |
| A12 | Recursion gate: dead rule | `recdead=1` | 0 | BUILT-OFF | BUNDLE-J2 a99f1d9e | YES |
| A13 | Recursion gate: budget rule | `recgate=24` | 0 | BUILT-OFF | BUNDLE-J2 a99f1d9e | YES |
| A14 | Upvalue store barrier in traces | `ubar=1` | 0 | BUILT-OFF | BUNDLE-L D1a 7aab6964 (C2, BUNDLE-2 `upbar` before) | YES |
| A15 | Absent-key loads as guarded misses | `absmiss=1` | 0 | BUILT-OFF | BUNDLE-L D1b | YES |
| A16 | CONCAT-built / dynamic keys | `ckey=1` | 0 | BUILT-OFF | BUNDLE-L D1c | YES |
| A17 | VARARG in traces (D2a) | `vargtr=1` | 0 | placeholder, **no effect** | BUNDLE-L | NO |
| A18 | Tier-1 inline vararg copy (D2b) | `vargt1=1` | 0 | placeholder, **no effect** | BUNDLE-L | NO |
| A19 | Probe recordings for fn-entry heads | `probe=3` / `probe=1` | 0 | BUILT-OFF | BUNDLE-M C2 25ec2afe | YES |
| A20 | Probe shadow check (diagnostic) | `probe=1,pchk=1` | 0 | BUILT-OFF (diagnostic) | BUNDLE-M C2 | YES |
| A21 | One strike for unchangeable aborts | `onestrike=2` / `onestrike=1` | 0 | BUILT-OFF | BUNDLE-M C4 79e4df92 | YES |
| A22 | Recorder lockstep off in production | (now shipped: `LOCKSTEP_PROCESS 0`) | 0 | SUPERSEDED (shipped) | BUNDLE-C refusal, M C1 36d3ec64 then e6103dc3 | YES (`lockstep=1`, `lockstep=8`) |
| A23 | POLL folded into BUDGET | `pollbud=1` | 0 | BUILT-OFF | BUNDLE-4 8d18c4be, b76fbd6d | YES |
| A24 | HREFK+HLOAD fusion early attempts | `hkfuse=0` to compare | 1 | SUPERSEDED (shipped) | BUNDLE-4 cc9e4d16 / 9b6b585c, J1 | YES (`hkfuse=0`) |
| A25 | Unroll while/repeat loops of callees | `loopany=0` to compare | 1 | SUPERSEDED (shipped, guarded, BUNDLE-S) | M1 63103302, BUNDLE-S | YES (`loopany=0`) |
| A26 | Function-entry heads for functions with loops | `fnroot=1,fnloops=1` | 0 | BUILT-OFF | M1 | YES |
| A28 | Sink-aware ineffective-root rule | `sinkroot=0` to compare | 2 | SUPERSEDED (shipped, SINK-R) | SINK-R | YES (`sinkroot=0`) |
| A27 | Continuation heads for loop recursion | `contok=1` / `contok=2` | 0 (test knob) | BUILT-OFF | BUNDLE-1 item 3, BUNDLE-E cb3288c0 | YES |
| A28 | Deeper static recursion limits | `unroll=3` / `unroll=8,maxdepth=8` | 2 / 4 | BUILT-OFF (values) | P-E, P-E2, M1, J2 | YES |
| A29 | Side-trace thresholds of frame families | `fck=1024` / `framemin=16` | 64 / 8 | BUILT-OFF (values) | H1, H2 item 2 | YES |
| A30 | Dynamic string keys: run-time hash mask, nil-miss loads, `NEWREF` for absent stores (`dynmask`) | `dynmask=0` to compare | 1 | SUPERSEDED (shipped; supersedes the string-key part of A15 / A16) | DYNMASK (this change) | YES (`dynmask=0`) |
| B1 | Tier-1 `setmetatable` fast path | - | - | BUILT-REVERTED (patch) | H3 D 010f6720 | NO |
| B2 | Late side traces | (`sidelate`, patch only) | - | BUILT-REVERTED (patch) | H3 C | NO |
| B3 | Short-string compare without memcmp | - | - | BUILT-REVERTED (patch) | K item C d4cb1832 | NO |
| B4 | Pattern captures not zeroed | - | - | BUILT-REVERTED | K item A, revert baf748c3 | NO |
| B5 | Down-frame side traces / return into caller | (`retstitch`, removed) | - | BUILT-REVERTED (stitch); real version NOT-MEASURED | BUNDLE-2, M1, R2, C2, B5 | NO |
| B6 | Link-at-heads for side recordings | (`sidelink`, removed) | - | BUILT-REVERTED | BUNDLE-2 item 2 | NO |
| B7 | Number widening for mixed int/float | (`mixminmax`, removed) | - | BUILT-REVERTED / CEILING | BUNDLE-2 item 4, BUNDLE-5 item 3 | NO |
| B8 | Peeling loops that contain allocating ops | `hdrlib=2` (SUB), `3` (+CHAR), `4` (+TOSTR) | - | **RESOLVED (HDRSUB 2026-11-10)**: miscompile was the `cleanup` third-operand bug; `LIB sub` peeling shipped (`hdrlib` 2), CHAR / TOSTR built, off | BUNDLE-1 item 4, HDRSUB | NO |
| B9 | `leaveshort` LeaveLoop budget | - | - | BUILT-REVERTED | R1 | NO |
| B10 | Entry back-off reset | - | - | BUILT-REVERTED | R1 | NO |
| B11 | Compile-pipeline micro-optimisations | - | - | BUILT-REVERTED | BUNDLE-C 602b7649 | NO |
| B12 | LuaJIT-style semantic relaxations | (`RUSTYNAIL_PH`, branch only) | - | BUILT-REVERTED (never merged) | P-H | NO |
| B13 | Tier-1 return address in CallInfo | - | - | BUILT-REVERTED (patch, unsound prototype) | G2 item 1 963228dd | NO |
| C1 | Quickened / inline-cached interpreter | - | - | CEILING | P-K d6ae02c0 | NO |
| C2 | Number narrowing | - | - | CEILING (part later built) | P-C 9ecd030d | NO |
| C3 | Array bounds-check elimination | - | - | CEILING | P-D 3ba94b94 | NO |
| C4 | Recursion frame redesign | (`recnat` now shipped) | 1 | CEILING -> prototype -> SUPERSEDED | P-M ca4d8c2c, P-M2 73c78adb, P-M3, P-M4 a4fe8f40 | partial (`recnat=0`) |
| C5 | Thin tier-1 call/return | - | - | CEILING (unsound prototype) | BUNDLE-D 8c5f4603 | NO |
| C6 | Root-frame varargs (D2) | - | - | NOT-MEASURED (not built) | BUNDLE-L | NO |
| C7 | VARARG / select / native self-call with MULTRET | - | - | CEILING | G2 item 3 | NO |
| C8 | `{...}` / `SETLIST B=0` | - | - | CEILING | P-E, P-E2, P-O part A | NO |
| C9 | Coroutines in traces / trace-level strings | - | - | CEILING (resume fast path built: BUNDLE-T, gate missed) | G2 item 4, P-PROF, BUNDLE-T | NO |
| C10 | Linked entry / transition cost | - | - | CEILING | P-N A 86606dca, H3 B, P-L | NO |
| C11 | Merge POLL+BUDGET / drop the budget compare | - | - | CEILING | BD 57dd7da0, B2 (f), H3 E | NO |
| C12 | Register file, PHIs, loop rotation, lea folding | - | - | CEILING | B2 6a218c08, H3 F, BD (b) | NO |
| C13 | Dead-slot snapshot purge | - | - | CEILING / rejected on semantics | BUNDLE-4 item 5 | NO |
| C14 | C3 lazy / compact exit stubs, smaller side traces | - | - | NOT-MEASURED | H1 D, H3 C, BUNDLE-M | NO |
| C15 | Inline allocation (`TNEW`) fast path | - | - | CEILING | P-J, H2 item 3 | NO |
| C16 | Tier-1 table constructors (inline NEWREF) | - | - | CEILING | BUNDLE-B item 1 | NO |
| C17 | lru_cache coverage / dynamic integer-key HREF | - | - | CEILING (partly SUPERSEDED) | BUNDLE-5 item 2, BUNDLE-D item 2 | NO |
| C18 | Background compile thread | - | - | CEILING | H2 item 4 | NO |
| C19 | Arena allocator for the compiler | - | - | CEILING | G1 item 3, BUNDLE-F | NO |
| C20 | Tier-1 GETI/SETI hash-part probe (key 0 case DONE in BUNDLE-A) | - | - | CEILING / key 0 done | H1 open 5, H2 item 6 | NO |
| C21 | N1(c) tier-1 re-entry after metamethod return / entering small functions from the interpreter | `ientry=<mode>` (scratch patch only) | - | REJECTED (measured 2026-11-08: no selective rule; iter_pipeline 8.7x). Re-tested after C39 (`innerretry`) the same day: iter_pipeline is cured, but containers_lru_heap +9 %, regex_nfa +6 %, inspect_lua +6 % instructions remain, so still off | BUNDLE-N, C21 study, C39 | NO |
| C22 | N2 remainder (fresh-table SETFIELD, constructor sinking) | `sinkmt=1` part only | 0 | PARTIAL (2026-11-09: exact growth fast paths built, classic_vector 0.965 instr; SETFIELD helper / sinking still open) | BUNDLE-N | partial |
| C23 | Native mutual recursion | - | - | NOT-MEASURED | R1, M1, R2 | NO |
| C24 | Q2 flush ratio / frequency-based root path | - | - | BUILT-REVERTED (experiment) | P-F | NO |
| C25 | Start-gate alternatives | `warm=1200000` / `wgate=N` | wgate 30000 | REJECTED (measured) | P-G, BUNDLE-1 | YES (partly; see entry) |
| C26 | Count-based hop-rate rule | `hoptop=0` / `hopcyc=64` to observe | `hoptop=98`, `hopcyc=0` | SHIPPED (BUNDLE-P: counting rule, clock rule opt-in) | BUNDLE-5 item 5, BUNDLE-1, BUNDLE-P | YES |
| C27 | P2 leftover: divert for persistent type change | - | - | CEILING | BUNDLE-2 item 6 | NO |
| C28 | Upvalue folding / CLOSURE in traces / closure escape | - | - | CEILING / NOT-MEASURED | C2, P-PROF P-S | NO |
| C29 | Fused one-allocation concat | - | - | rejected on semantics | P-B | NO |
| C30 | Inline CCALLS compare in asm | - | - | NOT-MEASURED | G2 item 2 | NO |
| C31 | Build-level variants (target-cpu, BOLT, static, huge pages) | - | - | REJECTED (measured) | P-I 8a386e98, BUNDLE-P 578dde97 | NO (rebuild) |
| C32 | Interpreter rewrite / threaded dispatch / call convention | - | - | NO-GO (ceiling) | INTERP_STUDY, T2 | NO |
| C33 | String hash word-at-a-time, trace string helpers | - | - | NOT-MEASURED (skipped) | STRING_STUDY, BUNDLE-3 | NO |
| C34 | Vectorisation | - | - | NOT-MEASURED | P-PROF P-T | NO |
| C35 | Compare metamethods (`__lt`/`__le`/`__eq`) from compiled code | `mm=0` to compare | 1 | SUPERSEDED (already shipped, N3) | BUNDLE-N N3, FEATURE-5 check | YES (`mm=0`) |
| C36 | Recording-moment fragility of loop roots (awfy bounce / list, bigint_rsa floor) | `cispare=0`, `second=0`, `domtop=0` (the study found three distinct causes, one knob each) | `cispare=1`, `second=3`, `domtop=82`, `domn=65536` | PARTLY FIXED (cispare: bigint_rsa and the floor bit; second / domtop: list and bounce at some gates; not at `wgate=60000`) | C36 study | YES |
| C37 | Generic C calls inside traces (`CALLC`, or stitch landing / continuation heads after unknown natives) | - | - | DESIGN ONLY; landing-trace slice measured as a predicted loss, not built (see "Landing-trace slice attempt") | feature-6 study, 2026-11-08 | NO |
| C38 | `lua_lexer` shape: data-dependent callee loops under the `loopany` guard (nested link of side recordings, root-frame-leaving side traces) | `loopanymax=0` (unguarded, storms `heapsort`); slice 1 only: `sidenest=1` | - (`sidenest=0`) | SLICE 1 BUILT (design item 1, side recordings link at callee loop heads; sound, tested, knob OFF: it does not fire on `lua_lexer`, whose callee loop heads have no root; items 2-3 not built; slice 2 attempt 2026-11-08: STOPPED at design, docs only, see C38 "Slice 2 attempt") | C38, C38 slice 1 | partial (`sidenest=0`) |
| C39 | `InnerLoop` abort / blacklist policy: bounded retry rounds for a head blacklisted by two `InnerLoop` aborts | `innerretry=0` | `innerretry=6` | SHIPPED (2026-11-08; iter_pipeline fixpoint under `ientry=1` 2748 -> 314 Mcyc; no regression > 0.5 % instructions over 223 programs, sudoku_solver 0.912, lua_lexer 0.972; 2026-11-09: expr_lang +30 % instructions from the 2nd round fixed by x4 round waits and no re-record of an ineffective retried trace, docs/PERF.md "Regression hunt") | C21 study, C39 task | YES (`innerretry=0`) |
| C40 | BUNDLE-B (2): Lua functions called from C (`CIST_FRESH`, `table.sort` comparators) enter compiled code although small and loop-free | `centry=0` (1 = bypass `interp_entry`, 2 = also the entry back-off monitor) | `centry=0` | BUILT, OFF (2026-11-08): no measurable gain on oop_mixins even with `centry=2` (enters 3038 -> 3166, instructions identical, cycles +2..+10 %) | BUNDLE-B | YES (`centry=1,2`) |
| C41 | Megamorphic call sites in traces (chain of side traces ordered by recording time; callee guard in the entry segment) | `chainre=0` (no chain re-order), `ncallee=0` | `chainre=95`, `chainn=65536`, `ncallee=1` | SHIPPED (2026-11-09; forth_vm 0.813 instructions, fdisp random K=4 2.7x; 223 programs: no program above 1.0002 in instructions; fdisp K=16 +17 % cycles, mdisp +1..5 % cycles are the known losers) | P3 study + build | YES (`chainre=0`, `ncallee=0`) |
| C43 | Upvalue / closure specialisation in traces (P2 bundle 2): guarded disjointness of upvalue cells (`uvsep` bit 0), rolled traces past header-mutating ops (`uvsep` bit 1 = `uvroll`) | `uvsep=0` | `uvsep=1` (shipped), `uvsep=3` (`uvroll` on) | `uvsep=1` SHIPPED (2026-11-09; iter_pipeline 0.554 cycles / 0.594 instructions, all 223 programs 0.997 / 0.998); `uvroll` OFF: 0.995 / 0.997 overall with four programs at 0.93-0.96 but `corpus/lua_lexer` +2.8..3.7 % cycles (over the 3 % gate) | P2 study + build | YES (`uvsep=0`) |

---

## A. Built behind a knob, default off (or a non-default knob value)

### A1. `fnroot`: generic function-entry roots
- **Knob.** `fnroot`. Process default 0: the struct default, never set by `apply_process_limits`.
  - On: `fnroot=1`. The companion `fnmin` is the shortest recording length (default 16).
  - Origin: P-E2 (WIP `249e7cca`). Made opt-in at the merge `13269f79`.
- **Tried.** pc 0 of a loop-free, non-vararg, calling prototype becomes a trace head. The recording ends at the root `RETURN` with a
  stitching-style exit. The ineffective-trace rule flushes at 20% early exits.
- **Evidence.**
  - `programs`, T13 method (RUNS 5, machine load 10-37):
    - fib tr/J4 1.007.
    - fib(35) single runs: J4 867 Mcyc. `unroll=4,maxdepth=6` without the rule 5225; with `maxside=0` 1695; `unroll=8,maxdepth=8,maxside=0` 1346; with the final rule 864.
    - `rec_loop` 1.019 (`fnroot=0,pack=0` 1.001).
  - Short scripts: geomean tr/t1 1.0007 vs 0.9995 for tr0. closures_06 1.137, co_09 1.110.
  - P-M: `rec_loop` with `fnroot=1` 440 M vs 409 M (+4.6%).
  - Coverage study: `fnroot=1,fnmin=4` richards 0.92.
  - J2: the global `fnroot=1,unroll=8,maxdepth=8` gave towers +29%, deltablue +10%.
- **Why rejected.** No gain on fib, because one recorded argument shape fails for the others. Short-script regressions. An open TBC
  oracle finding: protos with a TBC never get a head.
- **Evidence quality.**
  - Judged on fib / rec_loop plus the short suite, under heavy machine load.
  - The richards number comes from a scratch study (RNX knobs), not an A/B of the shipped tree.
  - Predates awfy, apps and corpus as gate sets. `recnat=1` (P-M4) now gives function-entry heads to recursive protos anyway, so what is
    left is "generic short roots".
- **Retest trigger.** A set where hot non-recursive short functions are called from code that tier 1 runs (richards-like schedulers).
  Or after a cheaper function-root exit or trace trees exist.
- **Full-set A/B (FEATURE-5, 2026-11-07, base ba96a5e2, plain build, 223 programs: old corpus 18, awfy 14, dev 19, heldout 13, programs 25, jit-trace 29, lib 95, apps 5, real 5;
  CPUs 3-7, min of 5 rotated runs, duplicate column, outliers re-run serially).** Geomean `fnroot=1` / base: old corpus **0.949**, awfy **1.058**, dev 1.002, heldout 0.994,
  programs 1.002, jit-trace 0.998, lib 1.002, apps 0.997, real 0.997, all 1.000. Winners: classic_vector **0.62**, class_middleclass **0.70**, astar_jumper 0.86,
  stream_closures 0.94, forth_vm 0.94, prototype_objects 0.94, lru_cache 0.95. Losers: awfy/bounce 1.45, awfy/list 1.26, awfy/deltablue 1.08 (`fnmin=4`: also richards 1.05, towers 1.06).
  **Causes (minimal repros, same binary):** bounce and list are not fnroot's fault. The base column is on a lucky recording moment there: `wgate=45000` WITHOUT `fnroot` gives bounce 1.44, list 1.20
  (the loop root is recorded on a rare path of `Ball:bounce`; 81 % of entries leave through one site, under `hoptop=98`). deltablue is real: 49 -> 99 side recordings and 9 -> 22 installed traces in a 25 ms
  program (compile cost), instructions +3 %, gate-independent; `fnmin`, `fnloops=0`, `recnat=0` do not remove it.
  **Decision: stays off** (fails the 5 % rule on deltablue, and on bounce / list at the default gate). The old rejection reasons (fib, short scripts) no longer matter: the winners are class-library / constructor code.
  Retest trigger now: a hotter entry threshold for function-entry heads than for loop heads (cuts the deltablue compile cost), or a fix of the recording-moment hazard of bounce / list (then re-run awfy).
- **Cheap retest.** YES: `fnroot=1` and `fnroot=1,fnmin=4`.
- **Update (BUNDLE-C).** The per-site version shipped as `fnpromo` (process 6, `fnpromow` 3): a function-entry probe head blacklisted by `FnShort` aborts is promoted once to a generic function root, at most 6 per state, never for a closure that reaches itself through an upvalue. astar_jumper 0.881 instructions, json 0.972, stream_closures 0.929, no loser above 2.3 %; does not reach the `class_middleclass` / `classic_vector` / `forth_vm` wins of the global knob (they need an earlier promotion, which loses `apps/events`, `deltablue`, `parser_combinator`). Global `fnroot=1` stays off. See JIT_TRACE_REF.md "BUNDLE-C".
- **Sources.** REF:2894-2951 (P-E2), REF:3027-3042 (addendum: fnroot default 0), REF:3448-3450 (P-M), REF:4023 (J2 global),
  docs/COVERAGE_STUDY.md Findings 1, item 6.

### A2. `framesides`: side traces from exits inside inlined frames of function-entry roots
- **Knob.** `framesides`. Process 0.
  - On: `framesides=1`.
  - Origin: P-E2 `249e7cca` ("framesides experiment (default off)").
- **Tried.** A side trace rooted at the innermost *restored* frame of a function-root exit, with known types dropped.
- **Evidence.**
  - P-E2: "change nothing for `fib`", because the ineffective rule ends the family.
  - P-M: fib(27) `framesides=1` 327 Mcyc vs 311 with the same limits.
  - No full-set numbers.
- **Why rejected.** No gain on fib. It was later found incompatible with the H1 frame cells (`fcfn`): "segfault in `fnroot_fib`".
  `fcfn` is therefore forced off whenever `framesides=1` (`tr/mod.rs:568`).
- **Evidence quality.** Judged on a single program (fib), single runs on a loaded machine. Superseded in practice by S1/H1 frame-cell
  side traces.
- **Retest trigger.** Only if function roots (A1) come back and H1's `fcfn` does not cover their exits.
- **Cheap retest.** YES: `framesides=1`. **Caveat:** it also disables the shipped `fcfn`, so it measures two changes at once.
- **Sources.** REF:2919, REF:3449, REF:3607 (H1 item B).

### A3. `nest=2` (linking exits from inlined frames) and `nest=0` (no nests)
- **Knob.** `nest` (0/1/2). Process and struct default 1.
  - On: `nest=2` (or `nest=0` for the opposite experiment).
  - Origin: F2b.
- **Tried.**
  - `nest=2`: a root recording that reaches a traced inner loop head *inside an inlined frame* ends with a frame-materialising link exit.
  - `nest=0`: no nest linking at all.
- **Evidence.**
  - F2b, T13 method. Two figures for `nest=2` appear in the same section, and the source disagrees with itself:
    - REF:1952: heapsort +6.5%, short_inner +62%, neutral elsewhere.
    - REF:2019: short_inner +31%, heapsort +14.7%.
  - P-F (fannkuch tr/J4, RUNS 3-7): `nest=2` 1.060 (no effect). `nest=0` 1.016-1.018, but mandelbrot 0.97 -> 1.19 and nbody 0.63 -> 0.77.
- **Why rejected.** The link exit goes through the generic handler, about 100 cycles per link, against a tier-1 call of a few tens. The
  callee's loop-exit side traces cannot continue past the callee's `RETURN` (`LeaveFrame`).
- **Evidence quality.** Judged on programs / jit-trace only (heapsort, short_inner, fannkuch). Measured before inline frame
  materialisation (H2 `fmat`) and frame-cell side traces (S1/H1) existed, both of which change the exit cost the decision rests on.
- **Retest trigger.** H2 `fmat` restores frameful exits in the stub. Retest `nest=2` now on short_inner / heapsort / awfy, especially
  when callee loops dominate (short_inner is 1.26x J4).
- **Cheap retest.** YES: `nest=2`. Control: `nest=0`.
- **Sources.** REF:1862, REF:1950-1957, REF:2019-2022, REF:3071, REF:3077.

### A4. Lower stitching threshold (`stitchmin`)
- **Knob.** `stitchmin`. Default 32, both struct and process.
  - On: `stitchmin=4` (0 = off).
  - Origin: F2b, F4.
- **Tried.** Stitch at an unrecordable C call, or beyond the recursion limit, after fewer recorded bytecodes.
- **Evidence.**
  - F2b: a body of ~50 bytecodes before `tostring` ran 0.16 s in tier 1 and 0.17-0.21 s stitched (`stitchmin` 4..24).
  - F4: `rec_loop` 1.168x slower with `stitchmin=4` (`bench-f4-recstitch.json`).
  - P-E: `d3` (`unroll=8,maxdepth=8,stitchmin=4`) rec_loop 1.009.
  - P-M: `unroll=8,stitchmin=4` gave rec_loop 418 M vs J4 420 M and 409 M default, with 100% of entries exiting.
- **Why rejected.** A stitching trace pays an entry and an always-taken exit per iteration, while tier 1 still runs the call. "None of
  the 25 programs stitches" at 32, so the benefit is unproven.
- **Evidence quality.** Micro and scratch programs plus rec_loop. Single runs in the hand experiments.
- **Retest trigger.** If exits become cheap enough (inline frame rebuild exists since H2 `fmat`), or for programs with long prefixes
  before unrecordable library calls (json/markdown-style string code).
- **Cheap retest.** YES: `stitchmin=4` (also `stitchmin=16`).
- **Sources.** REF:1959-1962, REF:2022 item (5), REF:2475, REF:2836, REF:3450.

### A5. Raising or lifting the side-trace family cap (`maxside`)
- **Knob.** `maxside`. Struct default 64, process value 16 (`MAXSIDE_PROCESS`, `cache.rs:143-146`).
  - On: `maxside=24` / `maxside=64`.
  - Origin: BUNDLE-B item 3 `63841cab`.
- **Tried.** A cap of 16 was shipped. The ideas parked are "keep 64" and "lift the cap" (BUNDLE-O).
- **Evidence.**
  - BUNDLE-B sweep (awfy, RUNS 5, `warm=0`, against 64), caps 8 / 16 / 32:
    - json 0.883 / 0.895 / 0.936.
    - queens 0.881 / 0.869 / 0.904.
    - richards 1.159 (8) / 0.961 (16); bounce 1.100 (8) / 0.998 (16).
    - Caps 20 and 24: json 0.913 / 0.924. cd gets worse with higher caps (1.02-1.05).
  - BUNDLE-O: `class_middleclass` 2604 -> 1032 Mcyc with `maxside=24`, 1025 with `maxside=64`. The entity loop with 4 receiver classes:
    1563 Mcyc traced vs 1835 untraced.
- **Why rejected (raising).** json and queens gain from the cap. A family past 16 side traces costs about 150k cycles per compile plus
  L1i pressure.
- **Evidence quality.**
  - The cap of 16 was chosen on awfy alone, before the corpus existed.
  - The class_middleclass numbers are single runs on one corpus program.
  - A full-set A/B of `maxside=24` alone was never run: BUNDLE-O went to `polyside` instead.
- **Retest trigger.** Polymorphic OO code (the corpus) at a meaningful share of cycles. Or a code-size-aware side-trace cost model.
- **Cheap retest.** YES: `maxside=24`, `maxside=64`.
- **Sources.** REF0:5410-5417 (BUNDLE-B §3), REF:4333, `cache.rs:143-146`.

### A6. `polyside`: polymorphic sites may exceed the family cap (BUNDLE-O O2)
- **Knob.** `polyside`, `polywait`, `polyframes`, `polygk`, `polyrate`. Struct defaults 0.
  - **Process defaults:** HEAD `POLYSIDE_PROCESS = 0`. The **working tree (uncommitted) sets 48** "on, user decision", with
    `POLYWAIT_PROCESS 2` and `POLYFRAMES_PROCESS 2`.
  - Measured configuration: `polyside=48,polywait=2,polyframes=2`.
  - Origin: `65e69653`.
- **Tried.** A full family defers an exit instead of blacklisting it. After `polywait` reports, the exit may get a side trace up to
  `maxside + polyside`, gated by guard class, rate or inlined-frame depth.
- **Evidence.** Plain build, 5 workers, RUNS 9, default mode, ratio new/base:

  | set | cycles min |
  |---|---|
  | class_middleclass | **0.396** |
  | target set (4) | 0.794 |
  | corpus (18) | 0.955 |
  | 173 existing | 1.0024 |
  | all 191 | 0.998 |

  - Worst: `awfy/queens` **1.090** (instructions 0.988), `awfy/cd` 1.042.
  - Variants (class_middleclass Mcyc, base 2600):

    | variant | class_middleclass | other effects |
    |---|---|---|
    | `polywait` 1 / 2 / 4 / 8 / 32 / 128 | 1012 / 1043 / 1052 / 1083 / 1276 / 2000 | json +10%, queens +9%, richards +13% |
    | `polygk=11` | 2600 | |
    | `polyrate` 4 / 8 / 16 | 2600 | |
    | `polyframes=2` | 1076 | |
    | `polyframes=3` | 2576 | |

  - No PGO A/B was run.
- **Why rejected (at HEAD).** queens +9% fails "no program > 5% worse", because instructions are not unchanged. No per-exit signal
  separates class_middleclass from queens.
- **Evidence quality.** Plain build only. One winning program. The queens loss is attributed to layout / L1i without proof.
- **Retest trigger.** Already flipped in the working tree. Retest when a side-trace cost model (code bytes vs exits saved) exists, or
  if queens-like small-family programs matter more than OO corpora.
- **Cheap retest.** YES.
  - On a HEAD-built binary: `polyside=48,polywait=2,polyframes=2`.
  - On a working-tree build: `polyside=0` reverts.
- **Sources.** REF:4325-4401, `cache.rs:147-159` (working tree), commit message `65e69653`.

### A7. Smaller side-trace minimum (`sidemin`), and O1 (not built)
- **Knob.** `sidemin`. Struct 0, process 4 (`SIDEMIN_PROCESS`).
  - On: `sidemin=0` or `sidemin=2`.
  - Origin: P-F (shipped 4). BUNDLE-O O1 not built.
- **Tried.** P-F swept the minimum. O1 asked whether relaxing it helps polymorphic loops.
- **Evidence.**
  - P-F (fannkuch tr/J4): 4 gave 1.039-1.048, 6 gave 1.048, 8 gave 1.053, 12 gave 1.056, 0 gave 1.058-1.071. A branch-site-only
    variant of 4 gave 1.051.
  - Cost of 4: queens 0.841 -> 0.866.
  - O1 (class_middleclass Mcyc): with the cap lifted (`polywait=1`), `sidemin=0 / 2 / 4` gave 1011 / 1025 / 1022. Without the lift,
    1290-1730 (4 is best).
- **Why rejected.** Once the cap is out of the way there is nothing to gain. astar_jumper, lua_lexer and queens were unchanged by
  `sidemin`.
- **Evidence quality.** The P-F sweep was on fannkuch (one program, loaded machine). O1 was single runs on corpus programs.
- **Retest trigger.** If programs show many `SideShort` blocks in the O0 table (astar_jumper 1.37 M, json_rxi 1.07 M exits blocked).
- **Cheap retest.** YES: `sidemin=0`, `sidemin=2`.
- **Sources.** REF:3071-3073, REF:3110, REF:4334.

### A8. `sinkmt` and `sinkside`: allocation sinking of metatabled tables and in side or stitched traces
- **SINK-F update (2026-11-07): `sinkmt` SHIPS (process 1, with `sinkstub=1` and the sink-aware ineffective-root rule `sinkroot=2`, A28); `sinkside` stays 0.**
  - **Cause of the old losses (confirmed, minimal repro `repro_sinkmt_slowdown.lua`, N=30000: 0.89 s default, 1.58 s with `sinkmt=1,sinkside=1,sinkstub=1`, trace entered 0 times).**
    An exit inside an inlined frame whose snapshot named a sunk table could not get a side trace (`ExitSnap::frame_side_ok` required `sunk.is_empty()`).
    The hot exit logged `sides false ... frame_sides false`, type-guard exits then re-recorded, the head was blacklisted and the loop stayed in the
    interpreter. Same mechanism on `corpus2/heldout/stream_closures` with the default sinking (one sunk record, pre-roll exit inside the producer/predicate
    frames, no side trace, ineffective-root rule flushes and blacklists: 2x slower than with sinking off).
  - **Now:** frame side traces may hang off snapshots that name sunk tables (the frame stub calls `jit_tr_mat`, then jumps through the exit cell with the
    frames virtual; the handler path already materialised before it unwound), and a root the ineffective rule would flush while its leaving pre-roll
    exits name sunk tables is re-recorded once without sinking those shapes (`sinkroot`, A28).
  - **Numbers** (same binary family, user+sys, CPUs 3-7, one program per core, min of 5, duplicate column; `docs/JIT_TRACE_REF.md` SINK-F/SINK-R):
    `corpus/classic_vector` 0.89 x, `dev/particles_physics` 0.90 x, `heldout/stream_closures` 0.52 x; geomeans corpus 1.00 (0.995 against the duplicate),
    dev 1.00, heldout 0.955, awfy 0.994 (duplicate 0.990).
  - **`sinkside=1` was measured on top and is NOT shipped:** `classic_vector` 1.05 x (0.89 x without `sinkside`), `awfy/cd` 1.04-1.05, corpus geomean 1.010-1.015
    (with `sinkmt=1,sinkstub=1,sinkroot=2` alone: 0.997). On `classic_vector` sinking in side traces sinks 28 tables in 11 traces (9 in 2 without) and materialises 208455 times
    (264179 without) but runs 1.05 x against 0.89 x: the larger side traces and their frame stubs do not pay (cause not dug further). Retest trigger for `sinkside`: a per-site cost
    model (materialisations per avoided allocation) or cheaper materialisation (A9).
- **Knob.** `sinkmt`, `sinkside`. Struct 0; process `sinkmt` 1 (SINK-F), `sinkside` 0.
  - On: `sinkmt=1,sinkside=1`.
  - Origin: BUNDLE-2 (patch `docs/bench-b2-sinkmt.patch`), BUNDLE-5 `b22ef012`.
  - Re-judged in BUNDLE-E item 2, G1 item 2, H3-G and J1.
- **Tried.** Sink `setmetatable({...}, V)` temporaries: a `__gc`-absence guard, and the exit restore sets the metatable. Run the sink
  pass for side and stitched traces.
- **Evidence (all `warm=0` plain unless noted).**

  | bundle | raytrace | cd | json | deltablue | other |
  |---|---|---|---|---|---|
  | BUNDLE-2 §7 (`sinkmt`) | 0.983 | 1.118 | | | micro `vec` 564 -> 13 Mcyc (43x) |
  | BUNDLE-5 (both, RUNS 5) | 0.792 | 1.111-1.118 | 1.038 | 0.933 | |
  | BUNDLE-E (RUNS 9) | 0.897 | **1.072** | 1.041 | 0.979 | expr_lang 1.021; 19-program net 1.000 |
  | G1 | -9.4% | +7.5% | +4.2% | -2.6% | awfy+apps 1.000 |
  | H3-G (vs A+F) | 0.871 | 1.145 | 1.035 | | |
  | J1 (handler path) | 0.876 | 1.206 | 1.052 | | |

  - BUNDLE-5 single knobs: `sinkmt` alone raytrace 0.983, cd 1.104. `sinkside` alone cd 1.000, json 1.048.
  - BUNDLE-N (c): `sinkmt=1` "2.2x slower on the constructor variants", not investigated.
- **Why rejected.** cd's exits are 98% frameful, shape-polymorphic side-trace exits. Each one materialises through the handler at about
  340-390 cycles, against an inline stub of about 15 cycles. That breaks the 5% rule.
  - Rejected sub-ideas:
    - Flush-and-re-record fallback (`sinkguard`): cd 1.23-1.33, json 1.06-1.14.
    - Frame sides for sunk snapshots: cd 1.144.
    - Static filters: they do not separate programs.
    - Cheaper `psetshortstr` materialise: cd 1.073 -> 1.058 (patch `docs/bench-be-materialise-fast.patch`).
- **Evidence quality.** awfy / apps / real only. **Never measured on the corpus**, even though corpus `classic_vector` (6.3x LuaJIT
  after N, where LuaJIT sinks the vectors) is exactly the target shape. The G1 numbers are scratch data, not kept.
- **Retest trigger.** "Re-open only if a program set with more raytrace-like (sink-heavy, few-exit) programs appears" (G1). The corpus
  is that set. Also retest after a cost-aware demotion policy.
- **Cheap retest.** YES: `sinkmt=1,sinkside=1`. With the stub and demotion: `sinkmt=1,sinkside=1,sinkstub=1,sinkdemote=1024`.
- **Sources.** REF0:5221-5223, REF0:5298-5306, REF0:5549-5566, REF0:5611-5614, REF:3796, REF:3918-3930, REF:4311.

### A9. `sinkstub`: inline materialising exit stub (J1 item 2; BUNDLE-E item 1 lean exit)
- **SINK-F update: shipped (process 1).** The frameful stub (`sunk_frame_stub`) now also serves exits whose snapshot has a frame cell: a non-zero cell (a side trace is
  attached) counts, stores the slots, materialises through the helper and jumps through the cell; the cell is read after the helper, whose policy may have flushed the
  family (the cell is then 0 or the tier-1 landing and the stub rebuilds the frames itself). Metamethod-frame snapshots keep the generic handler path.
- **Knob.** `sinkstub`. Process 1 (SINK-F; 0 before).
  - On: `sinkstub=1` (with the sinking knobs).
  - Origin: J1 `92f527aa` / `db97c6a5`. Sized first as the "lean exit handler" in BUNDLE-E item 1, not built there.
- **Tried.** A sunk-table exit restores in its stub and calls the light helper `exit::jit_tr_mat`, instead of the full handler.
- **Evidence.**
  - BUNDLE-E ceiling: the lean handler saves cd 3-4%, json 1-1.5%, raytrace ~1%; geomean ~1.5% over 4 programs.
  - J1 (plain, `warm=0`, RUNS 11), against `sinkmt=0,sinkside=0`:

    | limits | raytrace | cd | json | 3-target | all 24 |
    |---|---|---|---|---|---|
    | handler | 0.876 | 1.206 | 1.052 | 1.036 | 1.003 |
    | `+sinkstub=1` | 0.869 | 1.104 | 1.022 | **0.993** | 0.997 |
    | `+sinkdemote=1024` | 0.886 | 1.041 | 1.028 | **0.982** | 0.997 |
    | `+sinkdemote=64` | 1.037 | 1.237 | 1.008 | 1.089 | 1.013 |

- **Why rejected.** 1.8% at best (gate 3%). cd +10% with the stub alone. What is left is the materialisation itself: about 145 cycles
  per table, 23 Mcyc of cd's 235.
- **Evidence quality.** Plain only. Three target programs. Judged only as part of turning sinking on.
- **Retest trigger.** A cheaper materialisation (a node-vector template per shape) or a ratio-based demotion. Retest together with A8
  on the corpus.
- **Cheap retest.** YES: `sinkmt=1,sinkside=1,sinkstub=1`.
- **Sources.** REF0:5535-5547, REF:3862, REF:3906-3941.

### A10. `sinkdemote`: count-based demotion of sunk shapes (H3-G, J1)
- **Knob.** `sinkdemote=N`. Process 0 = never.
  - On: `sinkdemote=1024` (best measured).
  - Origin: H3-G (patch `docs/bench-h3-G-C-sinkdemote-sidelate.patch`), merged into the tree in J1.
- **Tried.** A shape (NEWTABLE sizes plus first key) materialised N times is demoted: the family is flushed and re-recorded without
  sinking it.
- **Evidence.**
  - H3 (vs A+F, `warm=0` plain), with `sinkmt=1,sinkside=1`:
    - No demotion: raytrace 0.871, cd 1.145, json 1.035.
    - `sinkdemote=1024`: cd 0.994, json 1.024.
    - `sinkdemote=256`: cd 1.014, json 1.077 (instructions +14.7%).
    - 3-target 0.982; awfy+apps 0.974 vs 0.979.
  - J1 numbers: see A9.
- **Why rejected.** 1.8% at best. The count cannot tell cd's losing shapes from raytrace's winning ones, and a low threshold churns
  re-records.
- **Evidence quality.** Three target programs, plain build.
- **Retest trigger.** A denominator for the count (materialisations per avoided allocation), or demotion without a family flush.
- **Cheap retest.** YES: `sinkmt=1,sinkside=1,sinkstub=1,sinkdemote=1024`.
- **Sources.** REF:3796, REF:3840, REF:3914, REF:3931-3933.

### A11. `sinkpol` and `sinkrate`: TSC-rate sinking policy (BUNDLE-L D3)
- **Knob.** `sinkpol`. Process 0 (`SINKPOL_PROCESS`).
  - With 1, `sinkmt`, `sinkside` and `sinkstub` default to 1 and `sinkrate` to 2500 unless named.
  - On: `sinkpol=1`, or `sinkpol=1,sinkrate=4000`.
  - Origin: `d3c8c5d3` (doc calls it "throwaway").
- **Tried.** Demote a shape when, in a window of 256 or more materialisations, they are fewer than `sinkrate` TSC cycles apart.
- **Evidence.**
  - PGO (RUNS 11, `warm=0`), `sinkrate=4000`:
    - Targets 0.983 min / 0.993 median.
    - raytrace 0.855, cd **1.047**, json 1.016, expr_lang 0.983.
    - awfy 1.003, apps 0.967, all 24 0.995.
  - `sinkrate=2500`: cd **1.132** (instructions 1.067), richards 0.910 (base bimodal).
  - Scan over `sinkrate`: cd 1.13 / 1.02 / 1.04 / 1.06 / 1.07 at 1500 / 2500 / 4000 / 8000 / 30000. Run to run cd ranges 1.02-1.13 at
    the same setting.
- **Why rejected.** Gate missed (targets 1.7%, cd +4.7..+13%). The policy is clock-driven, so the trace set varies from run to run.
- **Evidence quality.** Nondeterministic by design. raytrace and cd also depend on the script path string.
- **Retest trigger.** Do not retest as is. The doc says a policy needs the per-site cost (frames restored per materialisation) or a
  deterministic count combined with a ratio.
- **Cheap retest.** YES: `sinkpol=1`, `sinkpol=1,sinkrate=4000`.
- **Sources.** REF:4118, REF:4165-4179, `cache.rs:72-75`, `cache.rs:918-936`.

### A28. `sinkroot`: sink-aware ineffective-root rule (SINK-R)
- **Knob.** `sinkroot=0|1|2`. Struct 0, process 2 (`SINKROOT_PROCESS`). 1 = the shape is demoted globally (same key as `sinkdemote`), 2 = for the root head only.
- **What.** `cache::pre_exit_check`, just before it would flush a root as ineffective: if the pre-roll exits that took the exits name sunk tables, the shapes are
  demoted (`sink::demote_shape`, key scoped to the root head's prototype and pc for `sinkroot=2`) and the family is re-recorded without sinking them
  (`FlushWhy::Unsink`: no penalty). Counter-based (the exit counters of the trace), no clock, fires once per (shape, key): a shape already demoted falls through to
  the ineffective flush.
- **Choice 1 vs 2.** Equal on the full set (`ms_r1` / `ms_r2` columns: corpus 0.995 / 0.995, dev 0.991 / 0.990, heldout 0.957 / 0.956, awfy 0.999 / 0.995, no program
  more than 1.05 x slower in either); 2 shipped for its smaller blast radius. NOT done (the study's variants cost `particles_physics` +12-16% and undid the
  `classic_vector` gain): demoting at the hot exit itself with the global shape key.
- **Evidence.** `heldout/stream_closures` 0.52 x (default sinking alone: 1.00 x; with `sinkroot=2` and no `sinkmt`: 0.53 x), the rest of the sets unchanged.
  Do NOT revive `sinkpol` / `sinkrate` / count-only `sinkdemote` for this.
- **Retest trigger.** A program whose ineffective root still blacklists with sunk exits after the rule fired once (the second flush is the old one).
- **Cheap retest.** YES: `sinkroot=0`.

### A12. `recdead`: per-prototype recursion gate, dead rule (J2 item 1)
- **Knob.** `recdead=N`. Process 0 (`RECDEAD_PROCESS`).
  - On: `recdead=1`.
  - Origin: J2 `a99f1d9e`.
- **Tried.** If a prototype's continuation heads are refused N times and no native self-call is ever recorded, drop its heads.
- **Evidence.**
  - Targets cd, deltablue, json, raytrace, lua_interp, short_inner:
    - `warm=0`: 0.975 / 0.974 plain, 0.972 / 0.973 PGO.
    - Default mode: 0.977 / 0.975 plain, 0.972 / 0.971 PGO.
    - RUNS 25: 0.977 / 0.975.
  - Per program (`warm=0`): deltablue 0.93, raytrace 0.956, json 0.975, cd 0.99, lua_interp 1.00, short_inner 1.00.
  - **In default mode it destroys the recursion winners:** towers 2.2x, permute 2.5x, fib 1.5x, binary_trees 1.13x, storage 1.15x
    slower.
  - Tried and removed:
    - `recflush=1..3`: no gain.
    - `recheat`: havlak +6.4%.
- **Why rejected.** 2.3-2.8% of the 3% gate, and unsafe in default mode. It needs a run-time proof of non-recursion.
- **Evidence quality.** Thorough (plain, PGO, `warm=0` and default, RUNS 25). The default-mode misfire is explained by start-gate
  ordering, not traced.
- **Retest trigger.** After a per-prototype "may recurse" mark from the interpreter or tier-1 call path exists. That would also unblock
  A19.
- **Cheap retest.** YES: `recdead=1`. Default mode shows the misfire; `warm=0` shows the gain.
- **Sources.** REF:3976-4011, REF:4037.

### A13. `recgate`: per-prototype recursion gate, budget rule (J2 item 1)
- **Knob.** `recgate=N`. Process 0.
  - On: `recgate=24` (24..40 measured).
  - Origin: J2.
- **Tried.** A recursive prototype with more than N recordings + side recordings + probes/4 gets no more recursion recordings.
- **Evidence.**
  - `recdead=1,recgate=24..40`: cd 0.98, deltablue 0.93, json 0.98, raytrace 0.93. The budget rule adds 0-1% over `recdead`.
  - `recgate=1`: cd 0.94, deltablue 1.00, json 0.99, but towers 1.95x and permute 1.8x slower.
- **Why rejected.** cd's `put` (68-88 units) cannot be separated from towers' `moveDisks` (29 units) by a count. The rule fires too late.
- **Evidence quality.** Judged on awfy/apps targets. Per-prototype accounts were measured.
- **Retest trigger.** Same as A12, or a compile-cost-weighted budget.
- **Cheap retest.** YES: `recgate=24` (combined: `recdead=1,recgate=24`).
- **Sources.** REF:3989-4007, REF:4038.

### A14. `ubar`: upvalue (forward) barrier for collectable SETUPVAL in traces
- **Knob.** `ubar`. Process 0 (`UBAR_PROCESS`).
  - On: `ubar=1`.
  - Origin: BUNDLE-L D1a `7aab6964`. Earlier versions: C2 §3 experiment (reverted), BUNDLE-2 `upbar` (reverted).
- **Tried.** Record `SETUPVAL` of collectables into cell upvalues with `TBAR aux=1` and the forward `Heap::barrier`, instead of
  aborting `Nyi(Setupval)`.
- **Evidence.**
  - C2: lru_cache 0.998, events 0.991, cd 1.012 (instructions -2.1%).
  - BUNDLE-2: cd +2.1% (293.4 vs 287.4).
  - BUNDLE-L, D1 as a unit:
    - Targets 0.977 plain / 0.987 PGO (gate 0.970).
    - cd 1.029 plain / **1.037 PGO**. "cd is the regression and it comes from `ubar` alone": 145 vs 125 compilations, +3.8 Mtsc compile.
    - lru_cache 0.913 plain / 0.939 PGO.
- **Why rejected.** Removing the abort lets cd trace loops that lose to tier 1.
- **Evidence quality.** Three separate attempts agree. awfy, apps and real; never the corpus.
- **Retest trigger.** If a "traces that lose to tier 1" detector exists, or compile cost drops (cd's loss is compile and exits).
- **Cheap retest.** YES: `ubar=1`.
- **Sources.** REF0:4968-4969, REF0:5214-5216, REF:4116, REF:4122-4124, REF:4137-4156.

### A15. `absmiss`: absent-key loads as guarded misses (D1b)
- **Knob.** `absmiss`. Process 0.
  - On: `absmiss=1`.
  - Origin: BUNDLE-L D1b. It follows the N6 "absent dynamic keys" note.
- **Tried.** `HREFV` with `HREF_ABS`: a chain walk that proves absence, plus an integer-key absent form, instead of `IntMiss` /
  `TableHash` aborts.
- **Evidence.** Judged only as part of D1. "`absmiss` and `ckey` change nothing on cd". lru_cache gain is 0.939 PGO for the whole of D1.
- **Why rejected.** D1 missed the gate as a unit, and the cd loss was ubar's.
- **Evidence quality.** **Never measured without `ubar`.** The parked item's own effect is unknown apart from "nothing on cd".
- **Retest trigger.** Immediately, as the pair without `ubar`. Also when lru_cache-like map code matters.
- **DYNMASK update.** `dynmask=1` (A30, shipped) covers absent and present dynamic STRING keys of loads and absent dynamic stores without `absmiss` / `ckey`
  and without any node-count guard; `absmiss` still adds the integer-key absence proof and `ckey` still gates CONCAT-built keys.
- **Cheap retest.** YES: `absmiss=1`, and `absmiss=1,ckey=1` (no `ubar`).
- **Sources.** REF:4125-4128, REF:4154-4156, REF0:4268-4276 (N6 follow-ups).

### A16. `ckey`: CONCAT-built and dynamic string keys (D1c)
- **Knob.** `ckey`. Process 0.
  - On: `ckey=1`. With `absmiss=1`, stores of absent dynamic keys use the `NEWREF` helper.
  - Origin: BUNDLE-L D1c.
- **Tried.** A key built by `CONCAT` is a computed key, with no `EQREF` against the address seen.
- **Evidence.** As A15. N6 earlier: "enabling them in `reg_key_any` changed nothing in `table_hash`".
- **Why rejected.** As A15.
- **Evidence quality.** As A15: never isolated.
- **Retest trigger.** Programs with `"key"..i` lookups in hot traced loops.
- **Cheap retest.** YES: `ckey=1`, `absmiss=1,ckey=1`.
- **Sources.** REF:4129-4131, REF0:4268-4272.

### A30. `dynmask`: dynamic short-string keys with a run-time hash mask (SHIPPED, supersedes the string-key part of A15 / A16)
- **Knob.** `dynmask` (`DYNMASK_PROCESS` 1; struct default 0). Compare with `RUSTYNAIL_JIT_TRACE_LIMITS=dynmask=0`.
- **Cause (reproduced).** A dynamic key `HREFV` embedded the node count as a guard (`FLOAD lsizenode; EQ`) and an absent key got an identity guard
  (`EQREF`). `markov_text`: a chain of side traces on one `GETTABLE` (one per rehash), 72 M exits for a 1.74 Gcyc run; `scheme_interp` / `json_rxi`:
  one side trace per missing key until the 16-side cap blacklisted the exit. Micro (`poly2`, a miss through `__index` with 16 keys): 59.6 cycles per lookup
  (all side recordings `SideShort`, the loop ends up in the interpreter) against 17 with `dynmask=1`.
- **Result.** See `docs/JIT_TRACE_REF.md`, section DYNMASK. Old corpus 0.987-0.994, heldout 0.956-0.967, dev 1.001-1.008, awfy 1.002-1.004, programs 0.995-1.001,
  jit-trace 0.998-0.999, lib 0.998-0.999, apps 0.984-0.991 (two sweeps); `scheme_interp` 0.78, `markov_text` 0.79-0.81, `json_rxi` 0.91-0.94.
- **Not done.** Integer keys (`HREF_INT` keeps the `lsizenode` guard and `absmiss`), inline `NEWREF` (A1) into a fresh table with a dynamic key (the plan fixes the node
  of the recorded key), sinking of a table that is looked up by a dynamic key.

### A17. `vargtr`: VARARG / `CALL ...` in traces (D2a)
- **Knob.** `vargtr`. Process 0. Origin: BUNDLE-L.
- **Tried.** Nothing. "`vargtr`, `vargt1` exist as placeholders for D2 and do nothing" (REF:4131). The code agrees: `RecCfg::vargtr` is
  set in `tr/mod.rs:535,660` and never read in `tr/record.rs`.
- **Evidence.** No numbers recorded.
- **Why parked.** Not built. See C6.
- **Evidence quality.** Never measured.
- **Retest trigger.** See C6.
- **Cheap retest.** NO. Flipping the knob changes nothing. It needs the D2 implementation.
- **Sources.** REF:4117, REF:4131, REF:4158-4163.

### A18. `vargt1`: tier-1 inline VARARGPREP / VARARG copy (D2b)
- **Knob.** `vargt1`. Process 0. Origin: BUNDLE-L.
- **Tried.** Nothing (placeholder). `vargt1` is not consumed anywhere outside `cache.rs`.
- **Evidence.** "The tier-1 part (10.6% of `events` in the study)" is a study estimate, not a measurement of this knob.
- **Why parked.** Not built (session budget).
- **Evidence quality.** Ceiling estimate from a study.
- **Retest trigger.** Vararg-heavy event or dispatch code (`apps/events`-like) at a significant share.
- **Cheap retest.** NO. It needs a variable-count copy loop in the template assembler.
- **Sources.** REF:4162-4163.

### A19. `probe`: probe recordings for function-entry heads of native recursion (M C2)
- **Knob.** `probe=N`. Process 0.
  - On: `probe=3` (safe) or `probe=1`.
  - Origin: `25ec2afe`.
- **Tried.** Replace a would-be-`FnShort` recording with an IR-less probe. It restarts as a full recording if recursion shows up.
- **Evidence (PGO, `warm=0`, RUNS 11).**
  - `probe=1`: awfy 1.062 (**permute 2.486**, no trace left), real 1.019 (lua_interp 1.105). C2 targets 0.993 / 0.988. Compile -5.2%.
  - `probe=3`: targets 0.996 / 0.996, deltablue 0.984, worst 1.029. Compile -2.1%.
- **Why rejected.** The trace set changes when recursion appears behind leaf calls, and the safe level saves too little.
- **Evidence quality.** Thorough: shadow-checked (A20) and trace-set diffed.
- **Retest trigger.** After a per-prototype "may recurse" mark exists, so the first probe of a head can be cheap. The doc estimates
  -10% compile, deltablue -8%.
- **Cheap retest.** YES: `probe=3`, `probe=1`.
- **Sources.** REF:4203, REF:4223-4230, REF:4256.

### A20. `pchk`: probe shadow check (diagnostic)
- **Knob.** `pchk=1`, with `probe` on. Process 0.
- **Tried.** The full recorder runs and prints on stderr what a probe would have said.
- **Evidence.** 24 programs with `probe=1,pchk=1`, 195 recordings:
  - 181 agree exactly.
  - 8 found recursion.
  - 6 differ: a deltablue loop at depth 3, `Nyi(10)` and `IntMiss` in lru_cache.
- **Why parked.** It is a diagnostic, not a feature.
- **Evidence quality.** Exact counts.
- **Retest trigger.** Use it whenever the probe rules change.
- **Cheap retest.** YES: `probe=1,pchk=1`.
- **Sources.** REF:4225-4227.

### A21. `onestrike`: blacklist at the first unchangeable abort (M C4)
- **Knob.** `onestrike`. Process 0.
  - On: `onestrike=2` (all `unchangeable` reasons) or `onestrike=1` (code-property reasons).
  - Origin: `79e4df92`.
- **Tried.** One strike instead of TR21's two in a row.
- **Evidence.**
  - `onestrike=2`: targets 0.995 / 0.994 (raytrace 0.974, json 0.967, **expr_lang 1.040**, havlak 1.020).
    - Default mode RUNS 11: 1.012 (expr_lang 1.064, events 1.091).
  - `onestrike=1` (RUNS 9): 1.021 (expr_lang 1.053, lua_interp 1.101).
- **Why rejected.** Gate missed. A second attempt finds traces in expr_lang and havlak.
- **Evidence quality.** PGO, `warm=0` and default, awfy / apps / real.
- **Retest trigger.** If compile cost of failed recordings becomes dominant on a new set, or per-reason tuning (only `Nyi(Call)`).
- **Cheap retest.** YES: `onestrike=2`, `onestrike=1`.
- **Sources.** REF:4204, REF:4232-4234.

### A22. `lockstep`: recorder lockstep off in production (history; now SHIPPED off)
- **Knob.** `lockstep` (0 = off, 1 = every recording, K = every K-th).
  - **Process default today 0** (`LOCKSTEP_PROCESS`, `cache.rs:86-87`). Struct default 1. `+verify` and debug builds always run it.
- **History.**
  - BUNDLE-C (`602b7649`) **refused** gating it out: "not mechanically separable, the evaluation feeds `vals`" and "the abort is what
    protects against a miscompile".
  - BUNDLE-M C1 (`36d3ec64`) built it behind the knob with process default 1, "not shipped, gate missed".
  - `68fc29eb` documents it as clearly positive but short of the gate.
  - **`e6103dc3` shipped `LOCKSTEP_PROCESS = 0`.**
  - The BUNDLE-M section still says "1 here" (REF:4217): **the doc is stale.**
- **Evidence.**
  - C1 PGO `warm=0`: 11-program targets 0.975 / 0.974; RUNS 25 0.975 / 0.982.
  - The 7 named programs: 0.968 / 0.969.
  - awfy 0.986, apps 0.984, real 0.993. Compile -15.0% (93.3 -> 79.3 Mtsc).
  - Sampled `lockstep=8`: compile -10.8%.
  - IR and machine code identical on 24/24 bench and 260/260 corpus programs.
- **Why it was parked.** 2.5% against a 3% gate, plus the safety argument. Shipped later anyway.
- **Evidence quality.** Strong (identity checks, RUNS 25).
- **Retest trigger.** If a miscompile slips past tests, run production with sampling.
- **Cheap retest.** YES: `lockstep=1` (old behaviour), `lockstep=8` (sampled).
- **Sources.** REF0:5450, REF0:5469, REF:4194-4221, commits `36d3ec64`, `68fc29eb`, `e6103dc3`.

### A23. `pollbud`: POLL folded into the BUDGET cold path (BUNDLE-4 item 2)
- **Knob.** `pollbud`. Process 0 (`POLLBUD_PROCESS`).
  - On: `pollbud=1` (stay quantum 2^24 bytecodes).
  - Origin: `8d18c4be`; off in `b76fbd6d`.
- **Tried.** Call-free traces poll once per entry. Stays are bounded so the budget check doubles as the poll.
- **Evidence.**
  - Alone: sieve 1.02 (instructions 0.874), matmul 0.979, spectral_norm 0.960.
  - awfy 0.983, jit-trace 0.976 (int_sum 0.92, inline_leaf 0.90, type_entry 0.91, int_rec2 0.89).
  - **Losers:** `lib/tonumber_num` +17% (instructions 0.80), `lib/sel_n` +14% (instructions 0.82), `oop_methods` +12.6%.
- **Why rejected.** The 5% rule fails. The `tonumber_num` cause is not understood.
- **Evidence quality.** Plain, RUNS 9, quiet machine. The cause of the losers is unknown.
- **Retest trigger.** When the `tonumber_num` / `sel_n` anomaly is explained, or the entry-monitor stay judgement changes. Note that
  P-L's one-word poll already took part of this gain.
- **Cheap retest.** YES: `pollbud=1`.
- **Sources.** REF0:5267 (item 2, BUNDLE-4 "Items" list), `cache.rs:127-130`.

### A24. `hkfuse`: early attempts (history; now shipped)
- **Knob.** `hkfuse`. Process 1 today (`HKFUSE_PROCESS`).
- **History.**
  - BUNDLE-4: `cc9e4d16` "hkfuse process default off (nbody only, below the gate)", then `9b6b585c` "process default on (gate met on
    its targets)".
  - BUNDLE-4 doc numbers: nbody 0.971, richards 0.945, oop_override 0.927; target geomean 0.966.
  - J1 found that "**the fusion had silently stopped firing**": `typesides` (P2) gives the HLOAD guard its own snapshot index, and
    `hkfuse=0` changed nothing in the study (awfy 1.006). J1 restored it, with a cold disambiguator and observability counters.
  - J1 numbers: targets 0.971 / 0.967 plain, 0.972 / 0.970 PGO.
- **Inconsistency.** P2 predates BUNDLE-4, so BUNDLE-4's 0.966 cannot have come from a fusion that was always dead. Either the split
  did not hit every shape then, or BUNDLE-4's gain was noise or layout. Unresolved in the docs.
- **Evidence quality.** The J1 numbers are robust (RUNS 25). The BUNDLE-4 numbers are suspect.
- **Retest trigger.** Already guarded by the `[trace] fused HREFK+HLOAD` counter and `tr_core::j1_hkfuse_fires`.
- **Cheap retest.** YES: `hkfuse=0` (A/B the shipped feature).
- **Sources.** REF0:5267 (item 3), REF:3861-3884.

### A25. `loopany`: unroll `while` / `repeat` loops of inlined callees (M1)
- **Knob.** `loopany`. Process 0.
  - On: `loopany=1`.
  - Origin: M1 `63103302` "loopany opt-in".
- **Tried.** Unroll every backward edge of an inlined callee.
- **Evidence.**
  - `m1_rdparse` 204 -> 121 Mcyc (loaded run).
  - `nest_frame_pixel [on2]` exit storm: entries 4649 -> 1209, exits 4703 -> 6770.
  - BUNDLE-2: `loopany=1` richards 1.009 (RUNS 5).
- **Why rejected.** Data-dependent trip counts cause exit storms. It is caught by the `frame_links_and_stitches` test.
- **Evidence quality.** One cliff micro plus a test program. Never run on awfy, apps or corpus as a set.
- **Retest trigger.** If a trip-count guard (unroll only below N iterations) is added, or recursive-descent parsers matter.
  **New (BUNDLE-Q, `gforlua`):** with traces now recording `TFORCALL` of Lua closure iterators, `loopany=1` becomes the lever for lazy-iterator
  pipelines (`while true` inside a `filter` closure is an `InnerLoop` abort of the inlined iterator without it). Retest `loopany=1` (and
  `loopany=1,unroll=4`) whenever the default flips are considered again.
- **Cheap retest.** YES: `loopany=1`.
- **BUNDLE-Q numbers** (wall ms, min of 3, one fast core, plain release build, `rnlua --features jit`, outputs equal to `.expected` in every run;
  columns `gforlua=0` / `gforlua=1` / `gforlua=1,loopany=1`):
  `corpus2/dev` `iter_pipeline` 580 / 600 / **83**; the other 18 dev programs within +-4% across the three columns (noise level;
  `csv_report` 298 / 313 / 289, `oop_mixins` 421 / 441 / 433, `memo_cache` 371 / 371 / 381, `real_dis_arm` 671 / 670 / 677).
  `corpus2/heldout` (13 programs, `stream_closures` is the closure-iterator one): `stream_closures` 395 / 391 / 385, `log_analyzer` 565 / 508 / 480,
  `prototype_objects` 348 / 402 / 337 (single-column dip, noise), all others within +-3%. Not run: `corpus`, `awfy`, `programs`, `jit-trace`
  (so the A25 "never run as a set" caveat still stands for those sets), PGO builds.
  **`loopany=1` stays off**: this A/B shows no regression on corpus2 dev/heldout and a 7x win on `iter_pipeline`, but the earlier exit-storm
  evidence (`nest_frame_pixel`, `frame_links_and_stitches`) was not re-run; flipping the default is a policy decision for the owner.
- **Full-set A/B (BUNDLE-R, `loopany=0` vs `loopany=1`, same binary at 1c6198be, wall ms min of 3, 5 workers on CPUs 3-7, each program on one core for both columns, outputs equal to `.expected` / to each other in every run; regressions re-run serially with 9 runs, user+sys CPU time).**
  Geomean la1/la0: `corpus` (18) 0.996, `awfy` (14) 1.039 (driven by `cd`), `apps` (5) 1.004, `programs` (25) 1.000, `jit-trace` (29) 0.981, `lib` (95) 0.996, `real` (5) 1.000
  (those six = the 173 existing programs), `corpus2/dev` (19) 0.899, `corpus2/heldout` (13) 1.007.
  Wins: `dev/iter_pipeline` 557 -> 70 (0.126), `jit-trace/short_inner` 0.60, `nest_frame_pixel` (12000 rows scaled) 27 -> 16.
  **Real regressions (reproduced on re-run): `awfy/cd` 1.50 (50 -> 76 ms CPU), `programs/heapsort` 1.48 (221 -> 331 ms CPU).** All other >5% single-run outliers
  (`csv_report`, `prototype_objects`, `dijkstra_routes`, `json_rxi`, `awfy/json`, ...) were noise: 1.00 +-0.015 on re-run.
  Cause (`RUSTYNAIL_JIT_VERBOSE=1`): `heapsort`'s `sift` is a callee `while true` with a data-dependent trip count (~log n, random exit); unrolling it into the
  caller's `for` trace gives 395161 exits against 1986 and 54 compile pipelines (64 side recordings) against 3, i.e. the A25 exit storm. In `cd` the
  unrolled callee loops give 48 `TooLong` side aborts, 319 side recordings (236 before) and fewer trace entries.
  `loopany=1` forced in `tr_core` (struct default flipped): everything that finished passed except `frame_links_and_stitches`, whose `nest_frame_pixel [on2]` assertion
  (`entered` with `nest=2` >= 1.5x `nest=1`: 1209 vs 1245) is an expectation about the link, not a wrong result (both modes also unroll). jit-diff default mix seeds 1..60
  (`off tr tron trdef`, `loopany=1,lockstep=1` forced; `trdef` runs the shipped defaults): 0 diffs.
  **Default stays 0.** A flip needs a trip-count guard (unroll only loops whose recorded trip count is small and stable) or a per-site back-off after an exit storm.
- **BUNDLE-S: shipped with a guard (process default `loopany=1`, `loopanymax=4`).** The A25 retest trigger fired (a per-site back-off after an exit storm); see
  docs/JIT_TRACE_REF.md "BUNDLE-S" for the mechanism and the numbers. Verified cause of the `heapsort` regression with a minimal check (`RUSTYNAIL_JIT_VERBOSE=1`, 200000
  elements): with the callee `sift` (`while true`, data-dependent exit after ~log n iterations) unrolled into the caller's `for` trace the exits go 1986 -> 213000..395161 and the
  compile pipelines 3 -> 54, every distinct trip count is another exit of the unrolled trace and another side trace (64 recorded). A per-recording budget of unrolls per site
  does NOT help (tried first, `heapsort` stayed at 1.31-1.35 x: the installed root keeps exiting); the trace has to be flushed. Residual cost accepted: `awfy/cd` 1.035-1.05 x
  (about 13 more compile pipelines than `loopany=0`, no gain to offset them: 33 against 27 Mtsc of pipeline time). `loopanymax=2` is too tight (`corpus/lua_lexer` 1.085, `dijkstra_routes`
  1.07, `expr_lang` 1.07 reproducible; it also wins `forth_vm` 0.83), 8 is neutral but `cd` 1.06; 4 is the shipped value. `nest_frame_pixel` (unguarded 27 -> 16 ms) is now the
  guard's price: the guard flushes the family, 20 -> 20 ms (1.02 x), the 0.59 x win of the unguarded `loopany=1` is given up.
- **BUNDLE-S defect and fix (BUNDLE-P `laprune`, process default 1).** The residual `awfy/cd` cost of BUNDLE-S (+6.8% instructions / +8.7% cycles against `cb1af288`; ~70% BUNDLE-S, ~30% SINK-F/R `sinkmt`, which is the accepted A8 cost) was not "13 more compile pipelines" (160 vs 162): it was a defect of the guard. In `cd` the core root trace (the `for i=1,8` loop in `recurse`) never unrolls a callee loop itself; only its *side traces* unroll the data-dependent `while x ~= nil` of `RBT:get`. Their exits inside callee frames push the family counter past `loopanymax`; the guard blocked the sites (right) but also flushed the whole family `Ineffective`, the good root included. The root is re-recorded at another moment (30 KB instead of 89 KB), its callee-frame exits can no longer get side traces (recordings abort `InnerLoop` at the blocked site) and exits go to the interpreter 29-57K times each. Minimal repro (`~/.cache/study-cd/prot/repro.lua`, BST of 4096 keys, every 8th key calls `get()`): cb1af288 72.0 Mcyc / 307.5 M instr, main 112-115 Mcyc / 514-515 M, `loopany=0` 70-73 / 300-307 M. Fix: when the guard fires and the root's own `Trace::la` is empty, block the sites as before but flush only the family's side traces whose own `la` is non-empty (with subtrees, `FlushWhy::Dep`, the parent exit goes back to `Free`), reset `la_sides`, keep the root; a root that unrolled (the `heapsort` shape) still flushes the family. Knob `laprune` (struct 0, `LAPRUNE_PROCESS` 1). Repro: 69.8 Mcyc / 297 M instr (verbose: guard fired 2, family flushes 1, pruned sides 5, `ineffective` 1 against 2). `awfy/cd` (CPU 4, min of 9, same script path, cycles:u / instructions:u): cb1af288 241.9 Mcyc / 800.2 M, main f218347e 262.1 / 855.7, same binary `laprune=0` 263.1 / 855.6, `laprune=1` 252.2 / 820.1 (0.962 x cycles, 0.958 x instr of main; 1.043 x / 1.025 x of cb1af288, the rest being SINK-F/R `sinkmt` and noise). See docs/JIT_TRACE_REF.md "BUNDLE-P".
- **Sources.** REF0:4596, REF0:5204.

### A26. `fnloops`: function-entry heads for functions with loops (M1)
- **Knob.** `fnloops`. Process 0.
  - On: `fnroot=1,fnloops=1`.
  - Origin: M1.
- **Tried.** Generic function roots also for protos that contain loops.
- **Evidence.** lua_interp: "33 recordings / 5 installed but 935k exits and 212 Mcyc: off".
- **Why rejected.** Exit-heavy on its one target.
- **Evidence quality.**
  - One program, measured in M1, before R2.
  - Since R2, `fn_loops = fnloops || recany` (`rt.rs:1156`) and `recany` is 1 in a process. So `fnloops=1` alone is probably a no-op
    today; what was measured corresponds roughly to `fnroot=1`. This is inferred from the code.
- **Retest trigger.** Together with A1.
- **Cheap retest.** YES: `fnroot=1,fnloops=1`.
- **Sources.** REF0:4596.

### A27. `contok`: continuation heads for loop recursion (BUNDLE-1 item 3, BUNDLE-E)
- **Knob.** `contok`. Struct 0. "Test knob, never set by a process default".
  - On: `contok=1` (heads only) or `contok=2` (also native returns).
- **Tried.** Continuation heads of `recany` also record for a prototype in which any recording saw a self-call. Target: queens
  `placeQueen`.
- **Evidence.**
  - BUNDLE-1 (`warm=0`): queens 162.8 -> 172.0 (`contok=1`, +5.7%), 167.4 (`contok=2`, +2.9%). Plus a **SEGV** (4/6 runs `contok=2`).
  - Coverage-study scratch:
    - `RNX_RECMARK`: queens instructions 0.65.
    - `RNX_CONT_ANY`: queens 237 -> 156 Mcyc, but deltablue/havlak instructions +16%.
  - BUNDLE-E root-caused and **fixed** the SEGV (`rec_ret_guards` clobbered registers of frameful side traces). It added only a
    correctness test.
- **Why rejected.** No queens gain plus the crash. The crash is fixed; **no performance retest was done after the fix.**
- **Evidence quality.** The perf numbers predate the fix and the work gate. Single program.
- **Retest trigger.** Now (the fix is in): queens and recursion-in-loop programs.
- **Cheap retest.** YES: `contok=1`, `contok=2`.
- **Sources.** REF0:5082-5088, REF0:5206-5207, REF0:5526-5533, docs/COVERAGE_STUDY.md finding 3.

### A28. Deeper static recursion / inlining limits (`unroll`, `maxdepth`)
- **Knob.** `unroll` (2), `maxdepth` (4).
  - On: `unroll=3`, or `unroll=8,maxdepth=8`.
  - Origin: P-E, P-E2, M1, J2. The per-site variant shipped as `deep=8` in J2.
- **Tried.** Global deeper limits.
- **Evidence.**
  - P-E rec_loop: `unroll=4,maxdepth=6` 1.015; `unroll=8,maxdepth=8` 1.009.
  - P-E hand runs: `fib(5+i%3)` 683 Mcyc deep vs 886 default vs J4 1170. `fib(7)` constant 14.3 Mcyc (constant-folding artefact).
  - M1: `unroll=3` lua_interp 0.747 -> 0.711 (RUNS 5).
  - J2 global `fnroot=1,unroll=8,maxdepth=8`: towers +29%, deltablue +10%.
  - Tried and dropped: converting stitched loop recordings into deep re-records, richards 2.1x.
- **Why rejected.** A static default cannot be good for both stable and varying recursion shapes. Code growth.
- **Evidence quality.** Mostly scratch or single-program runs. J2 is a proper A/B.
- **Retest trigger.** lua_interp-like interpreters (`unroll=3`). Otherwise `deep=8` per site covers it.
- **Full-set A/B (FEATURE-5, 2026-11-07, same method and 223 programs as A1).** Geomean `unroll=3` / `unroll=4` over all: 1.003 / 1.004 (old corpus 1.013 / 1.027, awfy 1.022 / 1.034, dev 0.980 / 0.976,
  heldout 0.993, apps 1.069 / 1.065). Wins: dev/regex_nfa 0.69, heldout/scheme_interp 0.94. Losses (serial re-run): apps/expr_lang **1.38** (ins 1.14), class_middleclass 1.15, awfy/queens 1.17,
  awfy/towers 1.12; `unroll=4`: astar_jumper 1.32 (ins 1.50), towers 1.28. `loopunroll=32`: nothing moves beyond noise (all 223; `short_inner` flat for `loopunroll` 4..64).
  **Cause of the loss (repro on expr_lang):** varying recursion shapes of a recursive-descent parser; side recordings 67 -> 215, installed 38 -> 90, failed 29 -> 142: compile and recording
  cost of a 150 ms program. A synthetic fixed-vs-varying recursion micro does not reproduce it (native self-call), so the cause is the side-trace growth, not the unrolled code. **Decision: stays 2 / 16**
  (fails the 5 % rule on 4-5 named programs). Retest trigger: a per-site unroll (like `deep`) that backs off once side recordings of the family exceed a budget.
- **Cheap retest.** YES: `unroll=3`, `unroll=8,maxdepth=8`.
- **Update (BUNDLE-C).** The per-site version shipped as `sbump` (process 1): the retry of a side attempt at a stitch exit that ended `SideShort` / `Recursion` inside frames gets one more unroll. regex_nfa 0.707 instructions (global `unroll=3`: 0.69), expr_lang 0.786 (global: 1.38), no loser above 1 % in instructions; price: awfy/towers 1.11 cycles at 0.957 instructions. Global `unroll=3` stays off. See JIT_TRACE_REF.md "BUNDLE-C".
- **Sources.** REF:2836-2843, REF:2909-2919, REF0:4601, REF:4022-4023.

### A29. `fck` / `framemin` policy values (H1 / H2 item 2)
- **Knob.** `fck` (64), `framemin` (8).
  - On: `fck=1024`, `framemin=16`, or the other direction `fck=1,framemin=0`.
- **Tried.** Recover H1's expr_lang loss with a threshold.
- **Evidence.**
  - H2 (`warm=0`, plain, RUNS 9), expr_lang: `fck=256` 0.985, `fck=1024` 0.972, `framemin=16` 0.994, `framemin=32` 0.998.
  - Same knobs on towers: 1.046, **1.200**, 1.261, 1.623.
  - H1: `fck=1,framemin=0` towers 55.4 vs 58.6, expr_lang 167.0 vs 159.0.
- **Why rejected.** The best setting recovers 2.8% of expr_lang at +20% on towers. The loss is compile cost, not a threshold.
- **Evidence quality.** Two programs.
- **Retest trigger.** After compile cost drops.
- **Cheap retest.** YES: `fck=1024`.
- **Sources.** REF:3608, REF:3681, REF:3696.

---

## B. Built and measured, then reverted, patch-parked or never merged

### B1. Tier-1 `setmetatable` fast path (H3 D)
- **Origin.** H3 `010f6720`. Patch `docs/bench-h3-D-setmetatable.patch`.
- **Tried.** A `CALL` fast path in `jit_call` (and in `jit_tailcall`).
- **Evidence.**
  - `CALL` path: raytrace 1.007 (no gain).
  - With `TAILCALL`: raytrace **+14.5% cycles, +27% instructions**. The trace set changed (`vec` entered 438 vs 63 times).
- **Why rejected.** No gain, and the TAILCALL variant perturbs policy. Cause unresolved (spare `CallInfo` nodes or `cyc/stay` timing).
- **Evidence quality.** Single program.
- **Retest trigger.** Constructor-heavy OO corpus programs (classic_vector), after understanding the trace-set flip.
- **Cheap retest.** NO. Apply the patch.
- **Sources.** REF:3792, REF:3839, docs/BENCH_REPORT_2026-11-h3.md §6.

### B2. Late side traces, `sidelate` (H3 C)
- **Origin.** H3. Patch `docs/bench-h3-G-C-sinkdemote-sidelate.patch`. No knob in the current parser.
- **Tried.** After 24 side recordings a side-trace attempt waits for 4x `hot_exit`.
- **Evidence.** cd 0.958 vs 0.960 without, raytrace 0.992 vs 0.984, deltablue 0.951 vs 0.986, awfy geomean 0.965 vs 0.971-0.976.
- **Why rejected.** The S3 `hot_exit 64` gain did not come from late attempts.
- **Evidence quality.** awfy plus raytrace. richards bimodal.
- **Retest trigger.** Compile-cost-bound programs with large families.
- **Cheap retest.** NO (patch).
- **Sources.** REF:3790.

### B3. Short-string compare without memcmp, `shorteq` (K item C)
- **Origin.** BUNDLE-K. Patch `docs/bench-k-itemC-shorteq.patch`.
- **Tried.** A byte-or loop for lengths up to 16 in `intern_short`.
- **Evidence.** Plain, real set: total 1.007 (wordfreq 1.022, template_engine 1.017, json_roundtrip 1.008). The memcmp samples moved to
  `set_upvalue` (4.0% -> 7.7%).
- **Why rejected.** The store-forwarding hypothesis was not supported. The samples are skid from a stall elsewhere.
- **Evidence quality.** Throwaway, plain only, real set.
- **Retest trigger.** If the wordfreq stall is identified (gmatch upvalue store / barrier path).
- **Cheap retest.** NO (patch).
- **Sources.** REF:4087-4089, REF:4098.

### B4. Pattern captures left uninitialised (K item A)
- **Origin.** Built in `d4cb1832`'s bundle. **Reverted by `baf748c3`.** REF:4059-4071 still reads as if it were in the tree.
- **Tried.** Use `MaybeUninit` captures to skip zeroing 32 captures per match.
- **Evidence.** 12 targets: PGO `warm=0` 0.983, PGO default 1.000, plain 0.986 (instructions 0.989).
- **Why rejected.** "0% cycles in PGO default, below the 3% gate, adds unsafe to a forbid(unsafe) module" (commit message).
- **Evidence quality.** Solid A/B.
- **Retest trigger.** A pattern-heavy set where memset shows up in profiles (json_roundtrip had about 15 Mcyc).
- **Cheap retest.** NO. Re-apply the reverted commit.
- **Sources.** REF:4059-4071, commit `baf748c3`.

### B5. Down-frame side traces / return into the caller (`retstitch` and related)
- **Origin.** BUNDLE-2 item 1 (`retstitch`), M1 §3, R2 step 1, C2 §2, BUNDLE-5 item 4.
- **Tried.** Only the *stitch* variant was built: a side recording that reaches the root frame's `RETURN` / `TAILCALL` ends with an
  always-taken exit.
- **Evidence.**
  - M1: 179 vs 185 Mcyc, no gain.
  - R2: instruction counts unchanged on queens, cd, havlak, list, json.
  - BUNDLE-2: richards +3.5% with `sidestitchmin=1`; awfy 0.998 at 4.
- **Why rejected.** No gain. The real version (recording into the caller frame) needs a recorder model with slots below the root
  (`SlotMap` is root-relative `u16`). Never attempted ("multi-day, unmeasured ceiling").
- **Evidence quality.**
  - Stitch variant: measured.
  - Real down-frame: **ceiling never measured**.
  - LUAJIT_GAP_REVIEW: "M1's 'no gain' was a stitch to the interpreter, not recording into the caller". It estimates short_inner ~1.0x
    and type_entry ~1.1x LuaJIT with true down-frame sides.
- **Retest trigger.** Programs dominated by `LeaveFrame` (short_inner 1.26x J4, tail_loop, type_entry).
- **Cheap retest.** NO. A large build.
- **Sources.** REF0:4598-4599, REF0:4764-4766, REF0:4965, REF0:5197-5200, REF0:5313-5314, docs/LUAJIT_GAP_REVIEW.md (plan 2).

### B6. Link-at-heads for side recordings, `sidelink` (BUNDLE-2 item 2)
- **Origin.** BUNDLE-2. Reverted.
- **Tried.** Side recordings frame-link at traced loop heads inside inlined frames, like `nest>=2`.
- **Evidence.** richards / cd: `side aborted InnerLoop` 17 / 10 before and after, `frame links 0`.
- **Why rejected.** The aborting inlined loops have no root trace to link to.
- **Evidence quality.** Abort counts on two programs.
- **Retest trigger.** If callee loops get their own roots (with A25-style trip-count guards).
- **Cheap retest.** NO.
- **Sources.** REF0:5202-5204.

### B7. Number widening for mixed int / float sites (`mixminmax`; BUNDLE-5 item 3)
- **Origin.** BUNDLE-2 item 4 (built, reverted). BUNDLE-5 item 3 (not built).
- **Tried.** Record `math.max(0, float)` with exact mixed compares.
- **Evidence.** raytrace **+7.9% cycles (197.9 -> 213.5), +6.7% instructions**. The `render` trace hits `too many snapshots` (MAX_SNAPS 500).
- **Why rejected.** Worse. Real widening needs a new snapshot-entry kind that converts back.
- **Evidence quality.** One program.
- **Retest trigger.** If raytrace-like numeric code with int/float mixing gains a share, or after raising MAX_SNAPS. Re-size it on top
  of sinking first.
- **Cheap retest.** NO.
- **Sources.** REF0:5210-5212, REF0:5310-5311.

### B8. Header-safe peeling of allocating ops (`hdrlib=2`) -- RESOLVED by HDRSUB (2026-11-10)
- **Origin.** BUNDLE-1 item 4. Level 2 was removed from the knob after the miscompile.
- **Tried (BUNDLE-1).** Make `TNEW` / `SNEW` / `CONCAT` / `LIB sub` header-safe so loops with them peel.
- **Evidence (BUNDLE-1).** `bench/lib/sub` **produced wrong output** and ran 2.4x slower. Cause not found then (suspected PHIs of fresh strings).
- **Cause (found by the P2 bundle 2 work, confirmed by HDRSUB).** The loop optimiser's `cleanup` dropped / did not renumber the third operand
  (`Ins::third`: `LIB sub`'s `j`, `TSET` value, three-way `CONCAT`); fixed in 4ce16b7a. With the fix, `LIB sub` header-safe at `hdrlib=1` gave correct
  output on every program; the PHI of a fresh string is an ordinary PHI (mutant `hdrlib_sub_phi_stale` makes it take the pre-loop value and is caught).
- **Now.** `hdrlib` levels: 2 = also `LIB sub` (shipped, `HDRLIB_PROCESS` = 2), 3 = also `LIB char`, 4 = also `LIB tostring` (built, off: no gate-level gain,
  docs/PERF.md "HDRSUB"). Regression test `tr_core::hs_sub_ring_matrix` (old B8 program shape `b8_sub_ring`, every trace mode, +verify, exitstorm, GC stress 1/2 in
  both collector modes, 5.4 build). Still barriers: `TNEW`, `SNEW`, `CONCAT`, `STRTO` (not tried).
- **Cheap retest.** n/a.
- **Sources.** REF0:5091, REF0:5188; docs/PERF.md "HDRSUB".

### B9. `leaveshort` (R1)
- **Tried.** A loop-root recording ending at its own function's `RETURN` counts as `LeaveLoop`, the larger budget.
- **Evidence.** heapsort 1.34x slower.
- **Why rejected.** It burns the head in the heapify phase.
- **Evidence quality.** Single program.
- **Retest trigger.** None stated.
- **Cheap retest.** NO.
- **Sources.** REF0:4402.

### B10. Entry back-off reset when more protos compiled (R1)
- **Tried.** Reset the entry back-off.
- **Evidence.** multret 488 -> 447 only.
- **Why rejected.** Stays are short because of the C call. Reverted.
- **Evidence quality.** Single program.
- **Retest trigger.** None.
- **Cheap retest.** NO.
- **Sources.** REF0:4403.

### B11. Compile-pipeline micro-optimisations (BUNDLE-C §2-3)
- **Tried and reverted:**
  - `rm_op` stack buffer and presized x86 buffers: x86 phase raytrace 0.86 -> 0.93, cd 2.57 -> 2.63.
  - RA `pool()` without Vecs: 0.93 -> 0.92.
  - Lockstep `stored` memo: 3.87 -> 3.88.
  - Slot memo before the stale lookup: no change.
- **Not built:**
  - `st_sm.clone_from` per step (~1.5% of compile; contract change).
  - Another 2x on the pipeline (no hot spot).
  - Tier-1 compile.
- **Why rejected.** No gain.
- **Evidence quality.** Mtsc phase accounting.
- **Retest trigger.** Compile-bound sets (game_loop, many_loops).
- **Cheap retest.** NO.
- **Sources.** REF0:5463-5473.

### B12. LuaJIT-style semantic relaxations (P-H; branch never merged)
- **Origin.** P-H, branch `<worktree>`, env `RUSTYNAIL_PH`. The relaxations:
  - `h`: hooks.
  - `i`: interrupts.
  - `b`: budget check.
  - `c`: dead snapshot slots.
  - `v`: virtual GC charge.
- **Evidence.**
  - programs (25) geomean: h 1.002, i 0.996, b 1.004, c 0.997 (varargs 0.735), v 1.000, all 0.964.
  - jit-trace geomean: h 0.953, i 0.935, b 0.976, c 0.999, v 0.960 (sink_tuple 0.50), all 0.893.
  - "Close about 8% of the gap to LuaJIT."
- **Why rejected.** Report only. It changes observable semantics. The exactness-preserving wins (one-word poll, cheaper exits) were
  shipped by P-L instead.
- **Evidence quality.** Shared machine (load 6-30). Layout noise of ±30% on single programs.
- **Retest trigger.** Only if an opt-in fast mode is wanted.
- **Cheap retest.** NO (branch).
- **Sources.** REF:3115-3199.

### B13. Tier-1 return address in the CallInfo (G2 item 1)
- **Origin.** Patch `docs/bench-g2-item1-retaddr.patch` (unsound on purpose, reverted).
- **Tried.** Store the continuation address in `ci.ctx`, so `ret_tail` jumps directly.
- **Evidence (`warm=0`, RUNS 5).**
  - awfy 0.991 (cd 0.921, towers 1.010), apps 0.982 (raytrace 0.938).
  - `micro/call_lua` 0.874.
  - Named targets 0.998. All 19: 0.989.
- **Why rejected.** 1.1% geomean. Soundness work is needed (T15 code retirement).
- **Evidence quality.** An unsound prototype, so an upper bound.
- **Retest trigger.** Call-heavy programs whose returns mispredict (cd, raytrace) at a larger share.
- **Cheap retest.** NO.
- **Sources.** REF:3534-3536.

---

## C. Not built: ceiling below the gate, estimated, or never measured

### C1. P-K: quickened / inline-cached interpreter opcodes
- **Origin.** `d6ae02c0`.
- **Evidence.** Profile share only. Ceiling if half the lookup cost goes: geomean 0.987 (1.3%). If all of it: 0.975. oop_methods would
  reach 0.84.
- **Why rejected.** Below 3%. Dispatch and the library, not lookups, dominate.
- **Evidence quality.** Ceiling estimate, not built. `programs` only. Load ~20.
- **Retest trigger.** Interpreter-bound OO code. Partly addressed later by T3 tier-1 `__index` levels and K item B.
- **Cheap retest.** NO.
- **Sources.** REF:3393-3406.

### C2. P-C: number narrowing
- **Origin.** `9ecd030d`.
- **Evidence.**
  - Guard and conversion ceiling 0.
  - Constant `%` / `//` bound: loops_arith 0.958, oop_methods 0.697. All 25: ~1.8% bound, ~1.2% realistic.
- **Why rejected.** Below 3%.
- **History.** The "one real candidate" was built later: P-N power-of-two divisors and BUNDLE-4 `divmagic` (loops_arith 0.792,
  oop_methods 0.711).
- **Evidence quality.** Ceiling. Part superseded.
- **Retest trigger.** None left beyond what shipped.
- **Cheap retest.** NO.
- **Sources.** REF:3408-3425.

### C3. P-D: array bounds-check elimination
- **Origin.** `3ba94b94`.
- **Evidence.** Unsound build with all checks deleted:
  - fannkuch 0.990, heapsort 0.942, matmul 0.986, queens 0.982, spectral_norm 0.937.
  - Geomean 0.976 (2.4% upper bound). The `dot` micro 0.881.
- **Why rejected.** Below 3%. Sound ABC would remove about half.
- **Evidence quality.** Upper bound from an unsound build. Load 17-30.
- **Retest trigger.** Workloads like `dot` (invariant arrays indexed by the loop counter).
- **Cheap retest.** NO.
- **Sources.** REF:3427-3437.

### C4. Recursion frame redesign, judged on fib (P-M, P-M2, P-M3; later P-M4)
- **Origin.** `ca4d8c2c` (P-M), `73c78adb` (P-M2), P-M3 branch, P-M4 `a4fe8f40`.
- **Tried.**
  - P-M: model of down / up / native-return traces.
  - P-M2: slimming the call / return protocol.
  - P-M3: native self-recursion prototype.
- **Evidence.**
  - P-M: fib(32) 44.6 cycles and 151 instructions per call. Model ceiling 0.5-0.6x J4. Every limits combination measured was
    0.98-1.34x J4 on fib.
  - P-M2 prototype: instructions -4.0%, cycles unchanged (291.1/282.3/297.1 vs 290.6/282.5/284.8 M).
  - P-M3: fib rc/j4 0.737 (final), target 0.70 missed.
  - P-M4 then **shipped `recnat=1`**: fib 0.626, rec_loop 0.807, programs geomean 0.973.
- **Why parked.** P-M: three-part redesign. P-M2: cycles are bound by store-to-load chains. P-M3: missed 0.70 and verification was
  incomplete. Superseded by P-M4.
- **Evidence quality.** **Judged on a single program (fib)**, loaded machine (load 20-39), single runs.
- **Retest trigger.** The open parts are the up-trace / native return for mutual recursion (C23) and ack-style MULTRET (C7).
- **Cheap retest.** Partial: `recnat=0` measures what P-M4 bought.
- **Sources.** REF:3439-3504, REF0:3679-3700, REF0:3755-3810.

### C5. Thin tier-1 call/return (BUNDLE-D item 1)
- **Origin.** `8c5f4603`. Throwaway unsound patch.
- **Evidence.** `call_lua` 18.3 -> 17.2 cycles per call (-6%). Targets towers 0.976, cd 0.990, lru_cache 0.979: geomean **0.998**.
- **Why rejected.** The cost is a dependency chain, not instruction count.
  - INTERP_STUDY rates a call-convention redesign at -2..-4% for 20-40 days.
  - P-M2 and G2 item 1 (B13) concur.
- **Evidence quality.** Upper bound from an unsound build.
- **Retest trigger.** A latency model showing the dependency chain can be shortened.
- **Cheap retest.** NO.
- **Sources.** REF0:5490-5508, docs/bench-bd-ceiling.md, docs/INTERP_STUDY.md.

### C6. D2: root-frame varargs (BUNDLE-L)
- **Origin.** Not built ("session budget").
- **Tried.** Code reading only. The `Bus:emit(name, ...)` loops are rooted in a vararg function. It would need:
  - an entry guard on `nextraargs`;
  - negative-offset slot reads;
  - restore of `nextra`.
- **Evidence.**
  - G2: the `events` proto is 1.1 of 160 Mcyc in compiled code, so even removing it entirely is under 1% of events.
  - Study: tier-1 part 10.6% of events.
- **Why parked.** Ceiling under the gate (G2) and budget (L).
- **Evidence quality.** Ceiling by abort statistics.
- **Retest trigger.** Event or dispatch libraries in the corpus that loop inside vararg functions.
- **Cheap retest.** NO. `vargtr` / `vargt1` are placeholders (A17 / A18).
- **Sources.** REF:4158-4163, REF:3547, docs/COVERAGE_STUDY.md finding 5.

### C7. VARARG recording / `select` / native self-call with MULTRET (G2 item 3)
- **Evidence.** Across all 173 programs:
  - `Vararg` aborts in 4 programs (events, varargs, sel_loop).
  - `MultRet` in json and raytrace (2 recordings each).
  - `lib/sel_loop` is already 0.05x LuaJIT.
  - ack-style `return ack(m-1, ack(m,n-1))`: a rewrite gives 0.61, but "no suite program has it".
- **Why rejected.** Not coverage-bound.
- **Evidence quality.** Abort statistics. The ack number is a coverage-study scratch figure.
- **Retest trigger.** A corpus program with ack-shaped recursion or hot vararg forwarding.
- **Cheap retest.** NO.
- **Sources.** REF:3546-3547, docs/COVERAGE_STUDY.md finding 2.

### C8. `{...}` / `SETLIST B=0` in traces (P-E, P-E2, P-O part A)
- **Evidence.**
  - P-O: `va_brace` 2384 Mcyc vs 1384 with a fixed constructor (ceiling 0.58).
  - No program in programs, jit-trace or micro contains `{...}`, so the targeted geomean is 1.000 "by construction".
- **Why rejected.** Gate unreachable on the suites. Needs a class-A resize-array IR op.
- **Evidence quality.** Judged on our own sets only (before the corpus).
- **Retest trigger.** The corpus or real code with `{...}` in hot loops.
- **Cheap retest.** NO.
- **Sources.** REF:2857, REF:2946, REF0:3827-3829.

### C9. Coroutines in traces / trace-level strings (G2 item 4; P-PROF)
- **Evidence.**
  - No awfy or apps program uses coroutines. `lib/co_yield` 2.0x LuaJIT.
  - P-PROF: "coroutines in traces: no (<= 20% of one program: -0.9% geomean)". A resume/yield fast path is -1.0..-2.3% (estimate).
  - String aborts (`Nyi(Concat)`, `Nyi(Loadk)`) are isolated.
- **Why rejected.** A project of its own for one program at 2x.
- **Feature-2 investigation (coroutine bodies in compiled code), cause measured, nothing built beyond one glue fix.** The corpus2 study's
  "0.6x" is the ratio to LuaJIT (`coro_generators` puc 1823 / lj 720 / ours 1343 Mcyc), not to the interpreter: on this build
  `coro_generators` is JIT=0 0.435 s, trace off 0.295 s, trace on 0.279 s; `coro_scheduler` 0.403 / 0.368 / 0.339; `bench/programs/coroutines`
  0.408 / 0.400 / 0.394; `lib/co_yield`, `lib/co_wrap` flat (all user+sys, one run each, unpinned). The hypothesis "coroutine resume/yield
  across the JIT boundary falls back" is only half true: a coroutine body that runs a compute loop between yields is already traced
  (`x=x+i%7` loop in a `coroutine.wrap` body: 0.031 s interpreter-trace-off vs 0.019 s trace on). What aborts is a recording whose path
  reaches `coroutine.yield` / the wrap closure / `coroutine.resume` (all `CFunc`): in the minimal repros (`for v in wrap(gen)`, explicit
  `resume` loop, bodies `for i=1,n do yield(i) end`) 4 recordings abort with `CFunc` (path shorter than `stitchmin=32`) and no trace is ever
  installed. With `stitchmin=1` the producer/consumer loops do get traces that end in a stitching exit before the `CALL` (res.lua: 2.0 M
  entries) and the time moves 0.112 -> 0.094 s (res.lua) and not at all (wrap.lua, 0.084 -> 0.091): the loops are 4-6 instructions per
  switch, so there is nothing for a trace to win. Where the time goes (perf, `coro_generators`, trace on): `run_mode` 35% self (mostly entry
  latency: the load chain `ci -> thread -> stack -> function slot` of the coroutine stack, 13% of `run_mode` samples on one `cmp` after the
  frame switch, plus `GETTABUP _ENV "coroutine"` / `GETFIELD "yield"` hash lookups), `auxresume` 7%, `auxwrap` 6%, `State::resume` 4%,
  `unroll` 4%, `post_call` 3%, `finish_op` 2%, `co_yield` 1%: the resume/yield glue is about 25-30%. LuaJIT does **not** trace across a
  resume either (its coroutine fast functions are assembler and traces abort at them); its advantage is a cheap switch, not trace
  coverage. One real, local cost found and fixed: `co_resume` built a `Vec<Value>` per resume to put `true` below the results (malloc +
  free = about 8% of an explicit resume loop); it now shifts the values in place (res.lua 0.1215 -> 0.1103 s min of 5 pinned, wrap.lua,
  coro_generators, coro_scheduler -1..-3%, noise level).
  **Design if the switch is ever to be made cheap** (not built; estimate 1-2 bundles + a new mutant/test family): (1) a `resume` fast path
  that does not unwind through Rust: a flat resume entry that, for a coroutine suspended in `yield` (a `CFunc` with no continuation), moves the
  arguments, completes the `yield` call frame (`post_call`) and re-enters the executor directly, skipping `raw_run_protected` /
  `precover` / `unroll` generality; (2) a `yield` that returns to the resumer without `Err(Status::Yield)` travelling through every Rust
  frame when the yielding frame is the coroutine's bottom Lua frame called straight from the executor; (3) trace-level stitching at
  `yield`/`resume` only after (1)-(2), since an exit before the `CALL` already composes with them. Risk: errors in the body, `close`, tbc
  variables, yield across `pcall`, hooks, `isyieldable`, `nCcalls`/C-stack accounting, GC of suspended threads. Expected gain from (1)+(2):
  halving of the 25-30% glue, i.e. 10-15% on `coro_generators` and `coro_scheduler`, 0 elsewhere; it does not close the 1.86x to LuaJIT.
- **BUNDLE-T (2026-11-10): the resume side of the design above was built as a fused `auxresume` (`State::resume_fused`, docs/JIT_TRACE_REF.md "BUNDLE-T", docs/PERF.md "BUNDLE-T"); the gate (10 % on `lib/co_yield` and `programs/coroutines`) is missed.** Cycles / instructions (5 runs): `co_yield` 0.968 / 0.913, `programs/coroutines` 0.958 / 0.915, `coro_scheduler` 0.976 / 0.970, `coro_generators` 0.908 / 0.951, `co_wrap` 1.004 / 0.994; a micro loop with `yield` in an upvalue 0.912 (`y1`) and 0.857 (`y3`, no argument). The profile of the base binary (per resume + yield pair 826 instructions and 171 cycles at IPC 4.8; PUC 890 / 332; LuaJIT 401 / 69): `run_mode` (interpreting the coroutine body) 53 % of the cycles and 40 % of the instructions, the call glue (`auxwrap`, `auxresume`, `resume`, `post_call`, `unroll`, `finish_op`, `execute`) about 35 % of the instructions, of which the fused path removed 70 instructions per pair; `jit_call` 18 % of the instructions. No branch mispredictions, no cache misses: the loop is bound by dependent loads (59 store-to-load forwards per pair). **Why the gate is out of reach with this mechanism:** the two programs look `coroutine.yield` up in `_ENV` at every switch (`GETTABUP` + `GETFIELD`, then the `CALL` waits for the second result), a dependent-load chain of about 85 instructions and 31 cycles that the shorter switch path no longer hides (`co_yield` 8.7 % fewer instructions but 3.2 % fewer cycles; the same loop with `yield` cached in an upvalue 9.7 % / 8.8 %). **Next steps, none cheap:** (1) a cache for `GETTABUP` / `GETFIELD` with constant string keys in the interpreter (helps every program; touches the dispatch loop, so the layout lottery of the whole suite), (2) a flat resume entry that skips `run` / `run_mode`'s frame setup (about 100 instructions of entry and exit per switch; the same loop), (3) stackless switching inside the interpreter loop (the only way to LuaJIT's 400 instructions; JIT code pins the thread in `r12`, and the frame protocol, hooks and the GC all assume one Rust frame per resumed coroutine), (4) `CallInfo` nodes and the stack allocated with the thread (`co_wrap`, the creation loop of `programs/coroutines`: `new_thread` 10 %, `free_obj` 6 %). The recorder still aborts with `CFunc` at `yield` / `resume` / the wrap closure; a loop whose body yields exits its trace before the `CALL` (nothing to gain: 4 to 6 bytecodes between switches). **Tried and dropped:** a `jit_call` result-move specialisation for native callees (library programs +1.6..3.1 % instructions, no coroutine gain). Retest trigger unchanged.
- **Evidence quality.** Statistics and estimates; the Feature-2 numbers are single-machine, interleaved min-of-5 on one pinned core (micro) and single runs (programs).
- **Retest trigger.** Coroutine-heavy real code (schedulers, generators) in a set.
- **Cheap retest.** NO.
- **Sources.** REF:3549-3550, REF0:3983 (P-PROF rank 7).

### C10. Linked entry / transition cost of short loops (P-N A, H3 B, P-L, P-F)
- **Evidence.**
  - P-N: mutant `entry_no_typecheck` gives fannkuch 0.928 / 0.920, sieve no change, short_inner -1.4%. Upper bound ~1.5-2% geomean.
  - H3 B: queens prologue 2.6%, about 0.4% on 7 targets.
  - P-L: `entry_ok` in memory tried and reverted.
  - P-PROF P-Z: -0.3%.
- **Why rejected.** Below the gate. Soundness cost (per-head linked-entry cell plus a type-signature compare).
- **Evidence quality.** P-N ran on a saturated machine (load 18-22). Ceiling via an unsound mutant.
- **Retest trigger.** Programs dominated by root-to-side hops of 1-4 iteration loops (fannkuch-like).
- **Cheap retest.** NO.
- **Sources.** REF0:3584-3596, REF:3789, REF:3286, REF0:3986.

### C11. Merge POLL+BUDGET / drop the trace BUDGET compare (BD (a), B2 (f), H3 E)
- **Evidence.**
  - BD unsound removal: matmul 0.951, sieve 0.947, nbody 1.001. A merge recovers ~2% on matmul/sieve. Target geomean ~-1.2%.
  - H3 E (S2 `b`): geomean 0.994, sieve 0.93, about 1.4% on its targets.
- **Why rejected.** Below the gate. `pollbud` needs the compare. The interrupt flag is process-wide.
- **Evidence quality.** Unsound upper bounds.
- **Retest trigger.** Tight numeric loops at a large share. See A23 `pollbud` for the knob-level variant.
- **Cheap retest.** NO.
- **Sources.** REF0:4305-4314, REF0:4838, REF:3793.

### C12. Register file, PHIs in registers, loop rotation, lea folding (B2 (a)/(b), H3 F rest, BD (b))
- **Evidence.**
  - B2 step 0: an 8th register (`+membudget`) gives geomean 1.010 cycles (worse). Losing registers costs 7-9% each on matmul and
    spectral_norm.
  - BD (b): HREFK tag removal nbody 0.932 (unsound).
  - H3 F lea folding: not sized.
- **Why rejected.** The allocator is at the knee. PHIs already sit in registers.
- **Evidence quality.** Throwaway builds, quiet machine.
- **Retest trigger.** A new backend or ISA. Lower-IPC cores (the gap review notes Zen 5 hides instruction excess).
- **Cheap retest.** NO. `RUSTYNAIL_JIT_TRACE_OPT=+membudget` exists as a test option only.
- **Sources.** REF0:4803-4840, REF0:4884-4888, REF:3795.

### C13. Dead-slot snapshot purge (BUNDLE-4 item 5)
- **Evidence.**
  - The study removed 14 of 165 instructions at 0.0% cycles. Possible remainder: field_method -5..-10%.
  - P-H `c`: programs 0.997 apart from varargs (the varargs cost was later cut by P-L).
- **Why rejected.** It contradicts TR3 (exact state at exits, `debug.getlocal`, finalizer timing). Payoff nil.
- **Evidence quality.** Semantic objection plus the study's measurement.
- **Retest trigger.** None unless TR3 is relaxed.
- **Cheap retest.** NO.
- **Sources.** REF0:5267 (item 5), REF:3162.

### C14. C3 lazy / compact exit stubs and smaller side traces (H1 D, H3 C, BUNDLE-M)
- **Evidence.**
  - H3: exit stubs are 72-75% of trace code bytes. "No experiment found a size-for-speed trade".
  - H1 D: queens side traces are 1-4 KB each; raising `maxside` worsens L1i.
  - BUNDLE-M: the remaining compile cost is in recorder steps (C5) and exit stubs (C3).
- **Why parked.** No ceiling measured.
- **Evidence quality.** **Never measured.**
- **Retest trigger.** If A5 / A6 (larger families) are wanted. queens' 1.09x under polyside is attributed to code size.
- **Cheap retest.** NO.
- **Sources.** REF:3594, REF:3791, REF:4255.

### C15. Inline allocation fast path in generated code (P-J; H2 item 3)
- **Evidence.**
  - P-J: not built after the `data_ptr` fix took 11-28% of instructions off the targets.
  - H2: raytrace table creation ~20%, `TNEW` helper ~7.7%. Ceiling 3-4% of raytrace, realistic 2%.
- **Why rejected.** One program of 19.
- **Evidence quality.** Profile ceiling.
- **Retest trigger.** Allocation-bound corpus programs (classic_vector, metamethods: alloc+GC ~53%).
- **Cheap retest.** NO.
- **Sources.** REF0:3534, REF:3697, REF:3739.

### C16. Tier-1 table constructors: inline NEWREF of constant keys (BUNDLE-B item 1)
- **Evidence.** Counters times 40 cycles: raytrace 7%, lru_cache 4%, cd 3%. Geomean **2.7% ideal** (1.5-2% realistic).
- **Why rejected.** Below 3%. Needs inline `mainposition` with PUC `lastfree` placement.
- **Evidence quality.** Ceiling estimate.
- **Retest trigger.** Constructor-heavy OO code.
- **Cheap retest.** NO.
- **Sources.** REF0:5398-5401.

### C17. lru_cache coverage / dynamic integer-key HREF (BUNDLE-5 item 2, BUNDLE-D item 2)
- **Evidence.** BD-2: 5-9% of lru_cache, 1.0-1.9% of the 5-program geomean. B5-2: ~1.7% of apps.
- **Why rejected.** Below the gate.
- **History.** Partly superseded: H1 item A shipped integer `HREFV` for present keys. L `absmiss` (A15) covers absent keys.
- **Evidence quality.** Ceiling.
- **Retest trigger.** With A15 / A16.
- **Cheap retest.** Partly, via A15.
- **Sources.** REF0:5307-5308, REF0:5500-5502.

### C18. Background compile thread (H2 item 4)
- **Evidence.** Pipeline 6-10% of a cd run. "Measured ceiling: 0% on cycles:u, at most 6-10% wall on cd".
- **Why rejected.** `cycles:u` counts all threads. Determinism under `RUSTYNAIL_SEED=0` removes the gain.
- **Evidence quality.** Argued from phase accounting.
- **Retest trigger.** If wall-time latency becomes the metric.
- **Cheap retest.** NO.
- **Sources.** REF:3714-3728.

### C19. Arena / bump allocator for the trace compiler (G1 item 3)
- **Evidence.** jemalloc / tcmalloc LD_PRELOAD on awfy+apps: 1.001 / 1.005 (cd -2.0..-2.4%). ~-0.4% geomean.
- **Why rejected.** Below the gate.
- **Evidence quality.** Upper bound via allocator swap.
- **Retest trigger.** Compile-heavy sets.
- **Cheap retest.** NO.
- **Sources.** REF0:5616-5619, REF0:5588.

### C20. Tier-1 GETI / SETI with a hash-part key (H1 open 5, H2 item 6)
- **Evidence.** No `Geti` / `Seti` exit line in any awfy+apps program. Ceiling 0% on those sets. **Later finding (BUNDLE-A, 2026-11-08):** corpus2 dev/diff_levenshtein had 48067 `Some(Seti)` exits for `cur[0] = i`: the constant key 0 was not compiled at all (`kind` required `B >= 1` for SETI, `C >= 1` for GETI), so every execution left tier 1. The "retest trigger" below was met.
- **Resolution.** Done (docs/PERF.md BUNDLE-A): GETI/SETI with key 0 are now compiled as an unconditional jump to the out-of-line helper call (`jit_get` / `jit_set`, which already handle `Geti` / `Seti` incl. `__index` / `__newindex`, insertion and the barrier). The other miss paths (nonzero constants outside the array part, GETFIELD / SETFIELD / GETTABLE / SETTABLE) already went to the helper, not to an exit.
- **Why rejected.** (original hash-part-key idea) Nothing to gain on the sets.
- **Evidence quality.** Exit statistics.
- **Retest trigger.** (met, see above.)
- **Cheap retest.** NO.
- **Sources.** REF:3685, REF:3700.

### C21. N1(c): tier-1 / interpreter re-entry after a metamethod return; entering small loop-free functions from the interpreter (BUNDLE-N; re-measured 2026-11-08 on C38 main)
- **Question.** `interp_entry=false` functions (no loop) are not entered from the interpreter (`hot_call_plain` -> `NoJit`, parked for good),
  so a frame of a compiled prototype that the interpreter runs (after a trace exit) is never put back into compiled code by a call. Is there a
  deterministic, counters-only rule that keeps the wins of "enter always" and avoids its losses?
- **Method.** Knob `ientry=<mode>` in `RUSTYNAIL_JIT_TRACE_LIMITS` (scratch build, not merged; patch kept in `~/.cache/c21/keep/c21-experiment.patch`):
  0 = heuristic (today), 1 = always enter, 2.. = rules on `CallInfo` flags, the calling frame and the head states of the caller's `TrProto`.
  `hot_call_plain` must return `Retry` (not `NoJit`) when a rule refuses, otherwise the verdict is parked for good. Same binary, equal-length
  LIMITS strings, CPUs 3-7, one program per core, min of 5 (full set) / 3 (subset), cycles:u + instructions:u.
- **Global hack on current main (64 programs: corpus 18, dev 19, heldout 13, awfy 14; `ientry=1` / `ientry=0`, cycles ; instructions).**

  | program | cycles | instructions | mechanism |
  |---|---|---|---|
  | dev/iter_pipeline | **8.66x** | 9.25x | trace formation lost (below); runs at the JIT-off speed (JIT off 3153 Mcyc, base 311, hack 2744) |
  | dev/regex_nfa | **1.21** | 1.28 | one entered leaf (`rnd`, 108 calls from one site) removes the trace set: trace entries 2.64M -> 22k, side traces installed 24 -> 9 |
  | corpus/containers_lru_heap | 1.054 | 1.090 | same class, smaller (entries from a main-chunk loop) |
  | corpus/inspect_lua | 1.019 | 1.063 | same class |
  | corpus/class_middleclass | **0.637** | 0.687 | win |
  | corpus/classic_vector | **0.840** | 0.843 | win (`Vec:new`, 38k entries from the loop-free `Object.__call`) |
  | dev/expr_compiler | 0.910 | 0.915 | win (closure-tree nodes) |
  | heldout/forth_vm 0.951, corpus/lua_lexer 0.959, dev/event_promise 0.937 (noisy: dup column 0.961) | | | small wins |
  | scheme_interp, oop_mixins, ecs_world, bounce, queens, particles_physics | 1.00 (+-1 %) | | the picture of the study has gone: no win, no loss since C36/T1I/P-TINS/BUNDLE-LIB |

  Per-set geomeans of the hack: corpus 0.969, dev 1.123 (iter_pipeline alone; 0.99 without it), heldout 0.997, awfy 1.005. The duplicate column is 1.000 +-0.005.
- **Mechanisms found.**
  1. All of the effect (wins and losses, same numbers) comes from calls whose calling frame is a **Lua frame of a compiled prototype run by the
     interpreter** and that are not C-entered (rule 3 below reproduces `ientry=1` exactly: middleclass 0.643, iter_pipeline 8.74, regex_nfa 1.209).
     Fresh/MMRET frames (rule 2), calls from interpreted-only callers (4) and from C frames (5) have no effect on any program. So the win is not
     about metamethod returns or C-entered comparators in the current tree; it is "compiled callee returns into a compiled caller frame": the
     entry puts the **caller's frame** back into compiled code mid-loop.
  2. The win and the loss are the same event. middleclass: any ONE of three different callees (`Bird:speak` 3 insns, lines 181, 166) entered
     alone gives 0.64-0.66, so the effect is the caller's loop leaving the interpreter, not the callee's work. The base spends 407 Mcyc in the
     interpreter there after trace exits.
  3. The losers are **trace-formation perturbations, not entry cost**: stays are long (iter_pipeline: `Iter:count`'s callback, 708 enters,
     1482 insns/enter; regex_nfa `rnd`: 922 insns/enter), Mon never blocks. With the caller frame resumed in compiled code the
     interpreter no longer drives the loop head's recording: iter_pipeline's `Iter:reduce` loop (proto line 170 pc 4) aborts `InnerLoop (Wait(70))`
     on the 3rd recording, is blacklisted, and the loop stays in tier 1 (TFORCALL -> `ccall` -> nested interpreter for every iterator step: 51 % in
     `run_mode`, `jit_tforcall`, `pre_tail_call`); the base installs the 3rd trace. Bisect: `ientry=1182` (that one callback) alone gives 2756 Mcyc; entering any
     other function leaves it at 305-311. regex_nfa: `ientry=1006` alone (`rnd`) gives 1383 vs 1149.
- **Rules tried (subset of 12: 5 winners, 4 losers, 3 neutral; cycles ratio vs `ientry=0`; 3 runs).**

  | rule | middleclass | classic_vector | expr_compiler | iter_pipeline | regex_nfa | containers_lru | inspect_lua |
  |---|---|---|---|---|---|---|---|
  | 1 always | 0.637 | 0.842 | 0.921 | 8.92 | 1.195 | 1.054 | 1.020 |
  | 2 C-entered (FRESH / MMRET) only | 0.997 | 0.996 | 1.014 | 1.00 | 0.994 | 1.004 | 0.999 |
  | 3 caller = Lua frame of a compiled proto | 0.643 | 0.841 | 1.010 | 8.74 | 1.209 | 1.058 | 1.007 |
  | 4 caller = Lua frame of an uncompiled proto | 0.999 | 1.001 | 1.009 | 1.00 | 1.000 | 1.003 | 1.008 |
  | 5 caller = C frame | 1.002 | 1.001 | 1.017 | 1.01 | 0.995 | 1.001 | 1.004 |
  | 7 rule 3 + no caller head Warm/Recording | 0.998 | 0.841 | 0.991 | 0.98 | 1.003 | 0.996 | 0.988 |
  | 8 rule 3 + no caller head Recording | 0.623 | 0.843 | 0.996 | 8.82 | 1.006 | 1.052 | 1.011 |
  | 9 rule 3 + innermost loop around the call is none/Black/Traced | 0.640 | 0.849 | 0.999 | 8.83 | 0.999 | 0.988 | 1.141 |
  | 10 rule 3 + none/Black | 1.004 | 0.845 | 1.002 | 0.98 | 1.001 | 1.007 | 1.089 |
  | 11 rule 3 + not inside a loop | 1.004 | 0.844 | 0.994 | 1.01 | 0.993 | 1.001 | 1.119 |

  Rule 9 separates every program except iter_pipeline (its caller loop is `Traced` like middleclass's, both are exits at a megamorphic call site)
  and creates a new loss on inspect_lua (+14 %, instructions +18 %; rules 10/11 too: +9..12 %, while rule 3 is neutral there: another formation flip).
  Rule 7 keeps the safe set (iter_pipeline 0.98, regex_nfa, lru, inspect) but loses middleclass and expr_compiler (the winners enter from callers with
  Warm heads). event_promise reads +6..+10 % in every rule with a check (rules 2-5, 7-11) but 0.94..1.02 under 0/1: noise or the `Retry` ticks (small program, 335 Mcyc).
- **Why parked (STOP).** No rule on call origin, caller state or caller head states keeps middleclass/classic_vector/expr_compiler and avoids
  iter_pipeline: the harm there is a recorder fixpoint (`InnerLoop` abort twice -> permanent blacklist of a loop that is 10x faster as a trace
  than in tier 1/interpreter) that ANY change of event order can flip, and the base is exposed to the same flip by other perturbations. The hard gate (iter_pipeline
  0.114x of the session start kept) rules out every rule above. Shipping the knob default-off would add a `Retry` per call for refused entries
  for no default gain.
- **Where a fix would have to go (not built).**
  (a) The recorder: `InnerLoop` on an inlined callee loop must not blacklist the OUTER loop on the second occurrence when the inner loop has a trace
  (or when the abort came from a nested activation); a blacklisted loop whose body calls a Lua iterator via TFORCALL costs 9x (the tier-1 TFORCALL ->
  `ccall` -> nested `execute` path is slower than the interpreter's own loop). Then re-run `ientry=1` / rule 9: middleclass 0.64, classic_vector 0.84,
  expr_compiler 0.91-0.92, lua_lexer 0.96, forth_vm 0.95 are available (geomean corpus 0.969).
  (b) Or a re-entry of the compiled caller only at loop heads that are `Traced` and not blacklisted-by-InnerLoop, which needs the recorder's abort reason per head.
- **Update (C39, same day): the recorder fix (a) was built and `ientry=1` re-measured; the rule stays off.** The fixpoint is the `loopany` guard blocking the
  callee `while` site process-wide, after which every recording of the reduce loop through a `filter` chain aborts `InnerLoop` twice and blacklists the loop
  (details: docs/JIT_TRACE_REF.md "C39"). `innerretry` (bounded retry rounds with growing waits for heads blacklisted by `InnerLoop`, default 6) restores the trace
  (iter_pipeline 2748 -> 314 Mcyc under `ientry=1`). The "inner loop gets its own root first" idea cannot work there (the inner roots are blacklisted `InnerLoop` themselves).
  Re-measure of `ientry=1` with the fix, over `innerretry=6` (cycles / instructions): wins class_middleclass 0.628 / 0.687, classic_vector 0.785 / 0.784, expr_compiler 0.920 / 0.923, forth_vm 0.959 / 0.930, lua_lexer 0.944 / 0.959,
  stream_closures 0.943 / 0.979; losses containers_lru_heap 1.063 / 1.090, regex_nfa 1.073 / 1.058, inspect_lua 1.034 / 1.063, real_dis_x86 1.033 / 1.025, instructions +3.5 % on awfy/cd and awfy/richards.
  The losses are not `InnerLoop` cases (regex_nfa: another trace set, 10 instead of 8 heads blacklisted for `LeaveLoop` / `IntMiss`): the ask "keep only without regressions" is not met, so no knob was merged. Next step if wanted: rule 3
  (caller = Lua frame of a compiled proto) plus a check on the losers' formation (regex_nfa `rnd`, lru_heap, inspect_lua), or entry only for callees whose caller frame sits at a `Traced` loop head.
- **Not run.** The sets programs / jit-trace / lib / apps / real (no rule reached the build stage); tests and mutants (no code merged); the C-API,
  hooks/yield/GC-stress checks. The `table.sort` + `__lt` comparator shape: rule 2 (C-entered) has no effect on the 64 programs, a micro was not built.
- **Evidence quality.** Solid for the table (5 runs, duplicate column, same binary); subset rule table is 3 runs. Mechanism 3 is from the verbose-2 trace logs
  (`~/.cache/c21/ip{0,1}.v2`) and `perf record` flat profiles.
- **Retest trigger.** A change of the `InnerLoop` / blacklist policy in the recorder; or a new loser/winner pair of the shape "callee entry converts a caller frame".
- **Cheap retest.** NO (needs the scratch patch; `git apply ~/.cache/c21/keep/c21-experiment.patch` on C38 main, then `RUSTYNAIL_JIT_TRACE_LIMITS=ientry=<mode>`).
- **Sources.** study-gap h3 (`~/.cache/study-gap/h3.json`, patch `h3-interp-entry.patch`), REF:4309, this session's `~/.cache/c21/data/a1..a4.json`.

- **Update (BUNDLE-B, 2026-11-08): the tier-1 comparison-metamethod half of N1(c) is built and shipped.** `LT` / `LE` / `EQ` of two tables / userdata with a Lua `__lt` /
  `__le` / `__eq` no longer leave tier 1: `jit_cmp` pushes the in-place metamethod frame exactly as the interpreter does (`mm_bin` / `mm_eq`, `CIST_MMRET`, `nCcalls + 1`,
  `savedpc` = the `JMP` after the comparison), the shared `enter` stub runs a compiled callee, and the callee's `RETURN0` / `RETURN1` (`Cold::MmRet`) calls `jit_mm_ret`, which pops
  the frame like `mm_return` and stores the comparison's boolean at the caller's `top`; the shared `ret_mm` stub then jumps through a **resume entry at the `JMP`** (a new kind of
  entry, `Lowered.mm_resume`, emitted per comparison by `Cold::Cmp`) whose stub reads the boolean and branches to the comparison's two targets (`lt` / `lf`, `k` and the
  backward-jump poll included). A caller without that entry (interpreter frame of an uncompiled prototype) gets `jit_mm_fin` (the `mm_return` tail: skip the `JMP`) and continues in
  the interpreter (`FrameChanged`). Everything else (no metamethod, a C metamethod, hooks, the C-stack limit, errors, `__le` without `__le` in 5.4 mode, `LTI` / `LEI` /
  `GTI` / `GEI`, raw-equal or `__eq`-less objects) still leaves at the instruction. Yields inside the metamethod work because the frames are the interpreter's own.
  Not done: `RETURN` (vararg / `k`) of the metamethod and `TAILCALL` in it still exit; the interpreter's own `mm_return` does not re-enter compiled code.
  Answering raw-equal / `__eq`-less table `==` in the helper (`H_TRUE` / `H_FALSE`) was built and **rejected**: it changes which loops stay in tier 1 and so the trace set
  (awfy/deltablue 48 instead of 35 recordings, +6.6 % instructions).

### C22. N2 remainder: new-key SETFIELD into fresh metatabled tables, constructor sinking (BUNDLE-N)
- **Evidence.** classic_vector 1.7 Gcyc vs LuaJIT 0.24. `finish_set` was 15% in the study. `sinkmt=1` is 2.2x slower on constructor
  variants (not investigated).
- **Why parked.** Not touched.
- **Evidence quality.** Never measured.
- **Retest trigger.** The corpus vector classes.
- **Cheap retest.** Partial: `sinkmt=1,sinkside=1` (A8). The SETFIELD part needs a build.
- **Update 2026-11-09 (growth fast paths, docs/PERF.md "C22-C / C22-E").** The growth steps of fresh tables, not the SETFIELD
  helper itself, were the big part of the cost: `resize_hash` (string-key growth without the general `resize`), spare capacity for
  1- and 2-slot array parts and the dense-append `rehash_append`, all exact (layout, `#t`, byte accounting identical, tested
  against the generic path and the reference). classic_vector 0.965 instructions, storage 0.826, tinsert_* 0.88-0.89, bigint_rsa
  0.889, regex_nfa 0.906, sudoku 0.934. What remains of the field-store cost is the A1/NEWREF helper path, `newcheckedkey` of
  the new key and chain collisions (exact work); presizing from site feedback stays rejected (observable). Still parked: the
  SETFIELD helper inline fast path and constructor sinking.
- **Sources.** REF:4311.

### C23. Native mutual recursion (R1, M1, R2)
- **Evidence.**
  - lua_interp: 3.0x LuaJIT's 59 Mcyc after M1. 170k stitching exits at ~245 cycles = 7%.
  - expr_lang: mutual recursion, not attempted (R2).
- **Why parked.** Larger than any box.
- **Evidence quality.** Never measured.
- **Retest trigger.** Interpreters and tree walkers in the corpus.
- **Cheap retest.** NO.
- **Sources.** REF0:4404, REF0:4602, REF0:4630, REF0:4769.

### C24. Q2 flush ratio / frequency-based root-path choice (P-F)
- **Evidence.**
  - fannkuch: Q2 ratio 50% / 35% gives 1.47. `maxside=0` gives 1.233.
  - The flips root was recorded on the 20% minority path.
- **Why rejected.** Far worse. A frequency-aware re-record needs a design.
- **Evidence quality.** One program.
- **Retest trigger.** Loops with a dominant branch recorded on the wrong side.
- **Cheap retest.** NO (the Q2 ratio is a constant).
- **Sources.** REF:3071, REF:3111.

### C25. Start-gate alternatives (P-G, BUNDLE-1)
- **Evidence.**
  - P-G `TRACE_HOT=1000/2000`: heapsort 1.341, fannkuch 1.270, sieve 1.229.
  - Firing gate: multret 13.0x.
  - Iteration gate: sink_tuple +23%, queens +16%.
  - BUNDLE-1 replaced the TSC `warm` gate with `wgate=30000`. The TSC gate made bounce 3-5x slower in 4 of 9 runs.
  - `wgate` 15000 / 60000 / 100000 each had a loser above 5%.
- **Why rejected.** Measured worse or nondeterministic.
- **Evidence quality.** Solid. `WGATE_PROCESS` is called "a lottery ticket per program".
- **Retest trigger.** Re-scan `wgate` "when the trace formation changes".
- **Cheap retest.** Partly YES:
  - `warm=1200000` (old TSC gate).
  - `wgate=60000`.
  - Hotness via the env `RUSTYNAIL_JIT_TRACE_HOT=1000` (not the LIMITS string).
- **Sources.** REF:3228-3231, REF0:5068-5073, REF0:5185-5188.

### C26. Count-based hop-rate rule (BUNDLE-5 item 5)
- **Evidence.**
  - `hopcyc=0`: richards 0.499, but oop_methods 4.4x slower (684 vs 155 Mcyc).
  - Stay-budget readings overlap (richards 5, 0, 21, 9, 49 vs oop_methods 46).
- **Why rejected.** The count version cannot separate the programs. 64 ticks was shipped.
- **Evidence quality.** Three programs.
- **Retest trigger.** A CPU where richards' loop runs under 64 ticks per entry, or new hop-heavy programs.
- **Cheap retest.** Partial: `hopcyc=0` / `hopcyc=300` observe the rule. The count version needs a build.
- **Sources.** REF0:5316-5319.

### C27. P2 leftover: divert for persistent type changes in rolled loops (BUNDLE-2 item 6)
- **Evidence.** `late_rolled` 1.19x (scratch), still 1.4x faster than tier 1.
- **Why rejected.** No suite program has the shape.
- **Evidence quality.** Scratch program.
- **Retest trigger.** A suite program showing it.
- **Cheap retest.** NO.
- **Sources.** REF0:5218-5219.

### C28. Immutable-upvalue folding, `CLOSURE` in traces, closure escape analysis (C2 §3, P-PROF P-S)
- **Evidence.** P-S: closures -6% on the suite, ~0 on `bench/real`. "Low: benchmark-shaped, big effort".
- **Why rejected.** No real program allocates a non-escaping closure hot.
- **Evidence quality.** Estimate.
- **Retest trigger.** Callback-heavy corpus code (tail_loop's 12-deep upvalue chase, per the gap review).
- **Cheap retest.** NO.
- **Sources.** REF0:4970, REF0:3985 (P-PROF rank 9).

### C29. Fused one-allocation concat (P-B)
- **Evidence.** str_concat 1.07x J4 in incremental mode.
- **Why rejected.** Semantics: interned temporaries, GC accounting, `debug.getlocal`.
- **Evidence quality.** Argued.
- **Retest trigger.** None without a semantics change.
- **Cheap retest.** NO.
- **Sources.** REF:3021.

### C30. Inline `CCALLS` comparison in the assembler (G2 item 2, not done)
- **Evidence.** None. "No benchmark program needs it" (function roots and side traces execute the guard once per entry).
- **Why parked.** No gain on any set.
- **Evidence quality.** Not measured.
- **Retest trigger.** Hot non-loop traces with `pcall`.
- **Cheap retest.** NO.
- **Sources.** REF:3544.

### C31. Build-level variants (P-I, BUNDLE-P)
- **Evidence.**
  - target-cpu x86-64-v3: 1.000 / 0.994 / 0.999. `native`: 1.005 (slower).
  - BOLT on PGO: 0.900 vs 0.901, 0.951 vs 0.953.
  - Static relocation: +5% interpreter instructions, but oop_override +13-14% in trace mode. Opt-in `PGO_STATIC=1`.
  - Huge pages: ceiling < 0.5%.
  - `panic=abort`: unsafe, not built.
- **Why rejected.** No gain, or breaks the 5% rule.
- **Evidence quality.** Solid.
- **Retest trigger.** New toolchain or CPU.
- **Cheap retest.** NO (rebuild).
- **Sources.** REF:3336-3391, docs/BENCH_REPORT_2026-11.md table (lines ~165-181).

### C32. Interpreter rewrite, threaded dispatch, call-convention redesign (INTERP_STUDY; T2)
- **Evidence.**
  - An infinitely fast interpreter gives -11.7% overall. A realistic one -3..-5% at 60-100+ days: **NO-GO**.
  - Threaded dispatch: -0.5%.
  - Call convention: -2..-4%.
  - T2: STLI 1.9 per C call, equal to PUC.
- **Why rejected.** Ceiling and effort.
- **Evidence quality.** Study estimates plus measurements.
- **Retest trigger.** If real programs remain interpreter-capped (the gap review: "decide on the interpreter").
- **Cheap retest.** NO.
- **Sources.** docs/INTERP_STUDY.md, REF0:4935.

### C33. Word-at-a-time string hash; trace string helpers T4/T5 (STRING_STUDY; BUNDLE-3)
- **Evidence.**
  - H: lower / upper 0.57-0.62, sub 0.82 / 0.73 (scratch).
  - T4: find_plain 260 -> 120-150 (estimate). T5: 2-5%.
  - BUNDLE-3: "skipped as instructed: pairs-order harness decision still open".
- **Why parked.** H changes `pairs` order (PUC-order harness). T4/T5 not prioritised.
- **Evidence quality.** Scratch experiments.
- **Retest trigger.** If the harness stops depending on PUC `pairs` order.
- **Cheap retest.** NO.
- **Sources.** docs/STRING_STUDY.md, REF0:5241.

### C34. Vectorisation (P-PROF P-T)
- **Evidence.** "Not measured", "<= -1% (estimate, reasoning not measurement)".
- **Why rejected.** Float reductions cannot be reassociated.
- **Evidence quality.** Never measured.
- **Retest trigger.** Integer array kernels.
- **Cheap retest.** NO.
- **Sources.** REF0:3987.

---

### C35. Compare metamethods (`__lt`, `__le`, `__eq`) called from compiled code (FEATURE-5 check, 2026-11-07)
- **Prior claim.** None in the ledger (C21 is only about tier-1 re-entry); the task text assumed it might be missing.
- **Minimal repro (same binary):** `__lt`/`__le`/`__eq` on tables in a hot loop, `__lt` with mixed table / number operands, `__lt` inside a called function. Default installs a trace and runs
  3.15x / 1.96x / 17x faster than `mm=0` (1642 -> 521 Mcyc; 1599 -> 817; 535 -> 31). The recorder already handles them (`Rec::mm_order`, `mm_eq`, immediates `LTI/LEI/GTI/GEI`, since N3, `cfg.mm`
  process default 1).
- **Full set.** `mm=0` over 223 programs: geomean 1.030; class_middleclass 3.39, programs/metamethods 6.0, lib/getmt 13.3, classic_vector 1.56, astar_jumper 1.31. A scan of all 223 for remaining
  `aborted Metamethod`: 3 programs (lru_cache 1, template_gsub 11, prototype_objects 28 + 30 side), none defines a compare metamethod.
- **Decision.** Nothing to build. Not compiled: C-function metamethods, and compares inside `table.sort` (C function calling `lua_lessthan`). Retest trigger: a program whose hot loop aborts `Metamethod`
  at a compare op (check `RUSTYNAIL_JIT_VERBOSE=2` abort lines).

### C36. Recording-moment fragility of loop roots (found in FEATURE-5; studied and partly fixed in the C36 task, 2026-11-08)
- **Evidence (FEATURE-5).** `awfy/bounce`: 63 Mcyc at the default `wgate=30000`, 91 at 45000, 86 at 60000, no other change (instructions 333 M -> 489 M). `awfy/list`: 140 Mcyc default, 167-176 at every other
  `wgate` tried (10000..60000). A/B verdicts on awfy bounce / list under any knob that changes the number of recordings before the root is recorded are unreliable; use `wgate=20000/45000/60000` and `fnroot=1` columns as a control.
- **The study found three different causes, not one.** There is no single counter rule that covers them; each got a small deterministic mechanism behind its own knob (details, numbers and tests: docs/JIT_TRACE_REF.md "C36").
  1. **`heldout/bigint_rsa` with the `math.floor` bit of `t1inline` (T1I): not a recording-moment effect at all.** The trace of `mul_small`'s loops needs one spare `CallInfo` node behind its frame (TR6b: the recorded `math.floor` is an intrinsic call, `ExitMeta::depth` 1). Tier 1 with the inline floor never makes the real call that used to allocate that node (the interpreter's call of the C function did), so a leaf at the deepest call depth reached so far fails TR6b at every entry, and the GC (`shrink_ci`, frees half the unused nodes) takes the nodes away again. The persistent-entry-failure rule (`fail >= 64 && fail * 4 > ok`, 19663 of 78649 entries) then flushed the root, the re-record compiled to identical code, was refused `Identical` and the head blacklisted; the extra root recordings at proto:71 pc 30/32 of the T1I note were consequences. Fix: `cispare` (struct 0, `CISPARE_PROCESS` 1): when the entry-failure threshold fires for a trace with `depth > 0` whose frame lacks the nodes, the handler (`jit_tr_entry_fail`) allocates them (`State::ensure_spare_ci`), restarts the head's failure account and keeps the trace. `bigint_rsa` with the floor bit: 8.96 G -> 7.09 G instructions (floor off: 7.28 G).
  2. **`awfy/bounce` at `wgate=45000` / `fnroot=1`: the loop root is recorded on a rare path** (top pre-roll site 85 % of 1.65 M entries, below `hoptop=98`; at `wgate=60000` the best site holds only 73 %). Fix: `domtop` / `domn` (`DOMTOP_PROCESS` 82, `DOMN_PROCESS` 65536): a root with at least `domn` entries of which `domtop` percent leave through one pre-roll site into a side trace, 4 checks in a row, is re-recorded once per head (`FlushWhy::Dom`: no `rerec`, no `Identical` blacklisting). bounce 90 -> 67 Mcyc at 45000 and `fnroot=1`. **Limits:** the threshold is a measured separation, not a law: `class_middleclass` has a 78 % root whose re-record is 10 % worse (4.54 G -> 5.00 G instructions at `domtop=75`), `real_dis_arm` 84 % (neutral), `expr_compiler` 75 %; 82 clears all of them and fires on `real_dis_arm` only at the default gate. Not fixed at `wgate=60000` (73 %).
  3. **`awfy/list` at `wgate=45000`: a head lost to a transient.** The root at proto:21 pc 2 is flushed `Rerec` by a hot exit after 124 entries, the re-record then aborts `LeaveFrame` five times (the counter trigger always lands where the loop leaves its function), the head is blacklisted by `MAX_ABORTS`, and a later recording is `Identical` to the flushed trace. Fix: `second` (struct 0, `SECOND_PROCESS` 3): bit 0 gives a head blacklisted by a changeable `LeaveFrame` abort one second chance (wait `64 << MAX_ABORTS` back-edges, two more attempts; retrying every changeable cause is wrong: `IntMiss` retries recorded traces 28 % worse than tier 1 in `regex_nfa`), bit 1 installs the first `Identical` re-record of a head instead of blacklisting it. list 168 -> 133 Mcyc (default gate 140). Not fixed at `wgate=60000` / `fnroot=1` (another mechanism: not diagnosed).
  4. **`corpus/lua_lexer` (+2% since `laprune`)** not touched (diagnosed in C38): `InnerLoop` aborts of an ineffective re-record, 0.5 % in cycles on this machine, within what the three rules above do not address.
- **Retest trigger.** A program whose root is recorded on a rare path with a top share below 82 % (the `wgate=60000` bounce): needs a guided root recording (record the iteration that follows the dominant side's path) rather than a re-record lottery; `list` at `wgate=60000`: study its aborts first.

## History: ideas parked once and shipped or revisited later

| Idea | Parked where (why) | Shipped / revisited where |
|---|---|---|
| Recorder lockstep off | BUNDLE-C refused; M C1 2.5% < gate | `e6103dc3` `LOCKSTEP_PROCESS=0` (A22) |
| HREFK+HLOAD fusion | BUNDLE-4 off (`cc9e4d16`) | on `9b6b585c`; found dead, restored J1 (A24) |
| Native self-recursion | P-M / P-M3 (target missed) | P-M4 `recnat=1` (C4) |
| `pcall` in traces | P-E "not a trace construct" | P-O built (`pcall=1`); G2 hoisted CCALLS |
| Constant-divisor `%` / `//` | P-C ceiling < 3% | P-N pow2; BUNDLE-4 `divmagic` |
| 64-bit HREFK key compare | B2 (d) "not built in the time box"; BD unsound | H3-F `hk64` |
| Integer keys in the hash part | R2 step 5 built and dropped (no gain) | H1 item A (`HREFV` int, towers 0.566 with B+C) |
| `closeret` | R1 reverted (no gain) | M1 shipped (default 1) |
| Tier-1 nil-slot stores (`nilset`) | BUNDLE-A item 2 (0.986, patch) | BUNDLE-B `tset` bit 4 |
| Drop traces that lose to tier 1 | BUNDLE-1 item 5 (gate met without it) | BUNDLE-B `maxside=16` |
| SETUPVAL barrier in traces | C2 / BUNDLE-2 reverted | BUNDLE-L `ubar` built, still off (A14) |
| Dynamic-key lookups without a node-count guard (LuaJIT's `HREF`) | N6 / BUNDLE-L D1b (`absmiss`, per-key side traces, 16-side cap, off) | DYNMASK `dynmask=1` (A30): scheme_interp 0.78, markov_text 0.79-0.81, json_rxi 0.91-0.94 |
| Inline long-string `==` | STRING_STUDY T3 | BUNDLE-G1 item 5 (`f2byte`) |
| `polyside` | BUNDLE-O committed off | working tree `POLYSIDE_PROCESS=48`, uncommitted (A6) |
| Sinking of metatabled tables (`sinkmt`) + inline stub (`sinkstub`) | BUNDLE-2 / E / G1 / H3-G / J1 / L (cd +10%, classic_vector 1.5x slower, never measured on the corpus) | SINK-F: frame side traces for sunk snapshots + `sinkroot` (A8, A9, A28): `sinkmt`, `sinkstub` default 1; `sinkside` still parked |
| Shipped below the gate (for completeness) | K item B `strself` 2.5% | left on, `STRSELF_PROCESS=1` |

---

## Knobs that are default-off but were never framed as rejected ideas

Source: `TrLimits` doc comments (`cache.rs:165-451`), process defaults (`cache.rs:56-161`, `808-1030`).

| Knob / value | Process value | One-line description (from the code) | Note |
|---|---|---|---|
| `tailhook=2` | 1 | "2 = compile only, never enter" (BUNDLE-1) | richards 0.726, expr_lang only 0.824 (REF0:5080); a knob value with no default use |
| `t3idx=3` / `4` | 2 | tier-1 templates probe this many `__index` tables inline | "deeper than 2 inline levels by default" skipped, never measured (REF0:5014). Cheap: `t3idx=4` |
| `lockstep=K` (K >= 2) | 0 | "every K-th recording" | sampled mode, compile -10.8% at 8 (A22) |
| `reccont=0` | 1 (struct, not process-applied) | "native returns and continuation heads (experiment knob)" | control knob of P-M3 |
| `contok` | 0 | test knob (A27) | |
| `polygk`, `polyrate` | 0 (= no gate) | guard-class / exits-per-entry gates for `polyside` | measured as polyside variants (A6) |
| `warm=N` | unset (`wgate` 30000 used) | legacy TSC start gate | naming `warm` disables `wgate` (C25) |
| `deephop=0` | 1 | deep roots are exempt from the hop-rate rule; 0 = judged normally | control |
| `allocguard=0` | 1 | allocation-storm guard off | control |
| `pack=0`, `gfor=0`, `mm=0`, `setmt=0`, `pcall=0`, `fmat=0`, `sidefwd=0`, `fcfn=0`, `landcache=0` | 1 | reverts of shipped features | A/B switches, not ideas |
| env `RUSTYNAIL_JIT_HASHINT=0`, `RUSTYNAIL_JIT_HREFV=0`, `RUSTYNAIL_JIT_TRACE_OPT=-relaxstr` | on | integer `HREFV`, dynamic-key `HREFV`, relaxed string guards | env, not LIMITS |
| env `RUSTYNAIL_JIT_TRACE_PREF=region` | `trace` | J4 regions own a nest first (TR19b, X5) | not reviewed for this ledger |

---

## Retest on corpus2 (2026-11-07, binary = HEAD 65e69653 plain release `--features jit,trace`, `POLYSIDE_PROCESS = 0`)

Method:
- `tools/bench2.py` of the corpus2 study: 5 workers on CPUs 3-7, RUNS 7, interleaved columns, `RUSTYNAIL_SEED=0`, default mode
  (no `warm=0`), equal-length environments (`RUSTYNAIL_PAD`), a duplicate baseline column.
- Ratio = knob / base of min cycles:u. "Affected" = instructions:u changed by more than 0.3%.
- Each knob string was applied alone on top of the process defaults.
- The 19 **dev** programs of `bench/corpus2/dev` (MANIFEST) selected the knobs; heldout and corpus were measured only for the
  knobs that looked positive on dev (plus `fnroot`, `absmiss`, `sinkpol`, `polyside`, for information).
- Duplicate baseline: dev 1.002, heldout 0.995, corpus 1.002.
- No knob changed any output (all runs compared with the PUC `.expected`).
- Raw data: `data/knobs_dev.json`, `data/knobs_confirm.json`; tables `data/knobs_dev_table.md`, `data/knobs_confirm_table.md`.

### Dev (19 programs): geomean over affected programs, >= 3% = "positive on dev"

| knob | affected | geomean affected | geomean all 19 | best / worst |
|---|---|---|---|---|
| `maxside=24` (A5) | 4 | **0.923** | 0.987 | diff_levenshtein 0.685 / expr_compiler 1.032 |
| `unroll=3` (A28) | 4 | **0.909** | 0.984 | regex_nfa 0.710 / expr_compiler 0.999 |
| `unroll=8,maxdepth=8` (A28) | 4 | **0.915** | 0.984 | regex_nfa 0.708 / lua_parser 1.030 |
| `absmiss=1` (A15) | 4 | 0.975 | 0.996 | ecs_world 0.887 / regex_nfa 1.086 |
| `sinkpol=1` (A11) | 5 | 0.982 | 0.999 | particles_physics 0.885 / lua_parser 1.021 |
| `fnroot=1` (A1), `fnroot=1,fnloops=1` (A26) | 8 | 0.992 | 1.001 | particles_physics 0.922 / real_dis_x86 1.042 (the study's ablation variant `iter_pipeline__whileloop`: 0.742) |
| `maxside=64` (A5) | 8 | 0.990 | 0.997 | diff_levenshtein 0.664 / expr_compiler 1.248 |
| `sinkmt=1,sinkside=1,sinkstub=1` (A9) | 3 | 0.988 | 1.000 | particles_physics 0.951 / lua_parser 1.017 |
| `loopany=1` (A25, unguarded), `framesides=1` (A2), `nest=2` (A3), `t3idx=4` | 1-2 | 0.98-1.00 | 1.00 | - |
| `polyside=48,polywait=2,polyframes=2` (A6) | 1 | 1.022 | 1.007 | coro_generators 1.022 (no dev program has middleclass's shape) |
| `sidemin=0` / `sidemin=2` (A7) | 5 | 1.005 / 1.018 | 1.01 | ecs_world 0.912 / diff_levenshtein 1.164 |
| `recdead=1` (A12) | 7 | 1.018 | 1.009 | expr_compiler 0.922 / regex_nfa 1.220 |
| `nest=0` (A3) | 9 | 1.040 | 1.019 | particles_physics 0.919 / regex_nfa 1.208 |
| `onestrike=2` (A21) | 4 | 1.053 | 1.014 | regex_nfa 1.212 |
| `stitchmin=4` (A4) | 4 | 1.012 | 1.006 | real_dis_x86 1.075 |
| `ckey=1`, `absmiss=1,ckey=1`, `contok=1`, `probe=3`, `pollbud=1`, `framemin=16`, `lockstep=1` | 0-5 | 0.99-1.03 | 1.00-1.01 | - |
| `recgate=24`, `ubar=1`, `fck=1024`, `vargtr=1` | **0** | - | 1.000-1.005 | no instruction change anywhere (`vargtr` is a placeholder: `RecCfg::vargtr` is never read, verified) |

**Positive on dev:** `maxside=24`, `unroll=3`, `unroll=8,maxdepth=8`. Each rests on ONE program: diff_levenshtein for maxside,
regex_nfa for unroll. That is the overfitting risk in its plainest form.

### Confirmation on heldout (13) and on the old corpus (18)

| knob | heldout: geomean all / affected (n) | heldout best / worst | corpus: geomean all / affected (n) | corpus best / worst | verdict |
|---|---|---|---|---|---|
| `maxside=24` | 1.001 / 0.999 (6) | vending_fsm 0.969 / scheme_interp 1.055 | 0.948 / 0.783 (4) | class_middleclass 0.398 / lua_lexer 0.990 | **NOT confirmed on heldout**. Large win only where a family really needs more than 16 sides (middleclass, diff_levenshtein). The "MaxSide-blacklisted exits" signature does not predict a gain: lua_lexer (2.3 M), regex_nfa (1.07 M), astar, real_dis_arm do not gain |
| `unroll=3` | 0.984 / 0.902 (2) | scheme_interp 0.862 / forth_vm 0.945 | 1.009 / 1.069 (2) | astar_jumper 1.067, class_middleclass 1.070 | **confirmed on heldout** (2 programs). Regresses 2 corpus programs by 7%: fails the no-program-worse-than-5% gate |
| `unroll=8,maxdepth=8` | 0.982 / 0.892 (2) | scheme_interp 0.861 / forth_vm 0.923 | 1.021 / 1.173 (2) | astar_jumper **1.374** | heldout positive, corpus regression 37%: no |
| `maxside=24,unroll=3` | 0.988 / 0.970 (6) | scheme_interp 0.910 / dijkstra_routes 1.021 | 0.960 / 0.824 (4) | class_middleclass 0.465 / astar_jumper 1.032 | mixed |
| `fnroot=1` (not dev-positive) | **0.946** / 0.901 (7) | stream_closures **0.501** / dijkstra_routes 1.053 | **0.929** / 0.712 (4) | class_middleclass **0.415** / msgpack_pure 0.976 | Neutral on dev, large on heldout and corpus. A dev-only selection would have missed it. Its original rejection was on own sets (A1): needs a full-set A/B before anything |
| `absmiss=1` | 0.974 / 0.891 (3) | scheme_interp 0.807 / dijkstra_routes 1.031 | 0.995 / 0.969 (3) | json_rxi 0.922 | dev worst regex_nfa +8.6/9.8%: no |
| `sinkpol=1` | 0.946 / 0.792 (3) | stream_closures 0.517 / real_dis_mips 1.043 | 1.027 / 1.159 (3) | classic_vector **1.495** | no (classic_vector +50%) |
| `polyside=48,polywait=2,polyframes=2` | 0.999 / 0.994 (2) | - | 0.950 / 0.728 (3) | class_middleclass 0.397 | Only middleclass. The uncommitted `POLYSIDE_PROCESS = 48` in the main working tree buys nothing on dev or heldout |

**Honest summary.**
- No parked knob is a clean win across dev, heldout and corpus.
- The dev-positive knobs are single-program effects. One of them (`unroll=3`) also helps 2 heldout programs and hurts 2 corpus
  programs. The other (`maxside=24`) does not move heldout.
- The biggest effects on heldout and corpus (`fnroot=1`: stream_closures 0.50, middleclass 0.42; `sinkpol=1`: stream_closures 0.52)
  come from knobs that were neutral on dev. With 19 dev programs, "positive on dev" is a weak filter in both directions.
- A retest decision needs the full 173 + 50 program set, which was not run here.

### New entry: TSC-based hop-rate rule makes trace selection non-deterministic (`hopcyc`, BUNDLE-5 / P2)

- The hop rule measures time-stamp-counter ticks between two checks of a trace (`cache.rs` ~4370, `tsc()`). The trace set therefore
  depends on wall-clock timing, even with `RUSTYNAIL_SEED=0` and ASLR off (`setarch -R`).
- Repro: the study's `var/diff_levenshtein__flatloop.lua`.
  - Default mode is bimodal: 1502-1568 Mcyc or 495-560 Mcyc, about 3x. 8 of 9 runs were slow in one session, 1 of 5 in another.
  - The VERBOSE=1 summaries of the two modes differ: recorded 14 vs 16, entered 0.73 M vs 4.8 M.
  - `hopcyc=0`: 8/8 runs fast (571-611). `warm=0`: 8/8 at 494-498.
- `hopcyc=0` on all dev and corpus programs: geomean 1.003 / 1.000. No program improves except the repro (0.381);
  csv_report 1.061, with a base spread of 1.16.
- Consequences:
  - Any A/B in which a program sits near the rule's threshold can flip between modes, run to run and also by core type:
    TSC ticks per core cycle differ between this machine's 5.1 GHz Zen5 cores (CPUs 4, 6) and its 3.5 GHz Zen5c cores
    (CPUs 3, 5, 7).
  - Some past "no gain / regression" verdicts on small sets may rest on this.
- Retest trigger: before trusting any single-program verdict that moved by more than 2x, or any bimodal sample set.
- Cheap retest: YES (`hopcyc=0`). A deterministic replacement (counted work instead of TSC ticks) would remove the effect.

**Resolution (BUNDLE-P).** The clock rule is gone from the default policy; the entry above is closed.
- Reproduced before the change (plain build, `RUSTYNAIL_SEED=0`, `setarch -R`, `taskset`, 12 runs per core): CPU 4 gave 1515-1521 Mcyc in 4 of 12 runs and 517-563 in the rest, with
  four different instruction counts (8156 M, 3089 M, 3040 M, 2760 M); CPU 5 gave 529-555 in 12 of 12 with 2745 / 2876 / 3040 M. VERBOSE=1 shows three different trace sets.
  With `hopcyc=0`: 573-584 Mcyc and 3122 M instructions in 24 of 24 runs (CPUs 4, 5).
- Why the rule existed: root traces recorded on a rare path (`oop_methods`: root recorded on the `i % 100 == 0` branch) hop through one side trace at every entry;
  the loop then runs in the side trace without the root's loop optimisations (3.8x slower, 587 vs 153 Mcyc) and flushing the root fixes it. The clock rule was
  the guard against flushing roots whose hops are cheap relative to their work (deltablue 1.43x instructions, richards).
- Over the 223 programs (173 existing, 18 corpus, 32 corpus2) `hopcyc=0` changes the instruction count of exactly one program: `oop_methods` 3.82x. An instruction-count
  clock (budget word) was tried as a replacement and does not separate the cases (flat loop 18-24 bytecodes per entry, `oop_methods` 46, deltablue 20-280).
- Replacement: `hoptop=98` (counters only): a root of which at least 98 % of the entries leave through ONE pre-roll site into a side trace, 4 checks in a row, is a trampoline and
  is flushed. Top-site shares at the checks: `oop_methods` 99 %, `ecs_world` 99 % (fires: instructions 0.995, neutral), `scheme_interp` 96 % and `deltablue` 95 % (must NOT fire: flushing
  them gives 0.88x / 1.43x instructions), every other program below 90 %. The margin to the programs that must stay is 2 points; it is an empirical cut, not a derived one.
  `hopcyc=N` is still accepted as an explicit opt-in. Knob and mutants: `hoptop`, `hop_top_ignored`, `hop_clock_default`; test `tr_determinism`.
- A/B (PGO builds, 223 programs, 9 interleaved runs, CPUs 3-7, same program on one core for all columns, duplicate baseline D; N = new, A = main):
  default mode geomean N/A cycles min 1.001 / median 1.001, instructions 1.000 / 1.000 (D/A 0.999 / 0.999); `warm=0` mode 1.001 / 1.001, instructions 1.000 / 1.000 (D/A 1.000).
  No program is more than 5% worse at equal instructions. Only instruction changes: `pcall_error` 0.983 (the old rule fired there in some runs), `programs` set 0.999.
  All 50 corpus / corpus2 programs match the PUC output in every column. The 14 study variants (12 runs): the flat-loop variant of `diff_levenshtein` ranged 528-1470 Mcyc
  (instruction spread 2.72x) on main and is stable on the new build; `warm=0` showed the same effect on `richards` (instruction spread 1.25x) on one baseline column.
- Side effects found: `pcall_err` and `real_dis_arm` also flipped under the old rule (the old rule fired on them in some runs).

### C22 follow-up: constructor idiom `Class(...)` -> `__call` -> `new` -> `init` (feature 3 investigation, 2026-11-07)
- Real cause (not the recorder): the chain records and runs, `__call` is free (direct `setmetatable({}, C); o.x=..; o.y=..` loop 1.70 s vs 1.77 s via `__call`, 20M iterations).
  About 50 % of the time is the new-key path of a fresh table: `{}` has hash size 0, key 1 rehashes to size 1, key 2 to size 2, and NEWREF takes the out-of-line helper
  (`jit_tr_alloc` NEWREF branch, `mainposition`, `newkey`, `insertkey`, `resize`, `finish_set`) because no free node exists (inline NEWREF A1 needs one). Alloc/free another 8 %.
  The metatable only matters because it blocks sinking. A plain `{}` loop is 0.014 s (sunk).
- `classic_vector`: 89 % of cycles outside JIT code (`finish_set`, `newkey`, `run_mode`); traces buy only 1.5x (0.756 s off, 0.502 s on).
  `sinkmt=1,sinkside=1` still makes it slower (0.494 -> 0.738 s; loop traces shrink 105 KB -> 22 KB, side traces 3 -> 5); on the micro repro `sinkmt` wins 85x. Cause not dug.
- Upper bound (hack, reverted): presize the hash part of `NEWTABLE b=0,c=0` to 2 nodes in interpreter AND `tnew_core`: classic_vector 0.49 -> 0.29-0.31 s (-40 %), outputs equal;
  class_middleclass, oop_mixins, prototype_objects, awfy deltablue/richards/json/cd/havlak/list/bounce flat. Trace-only presize is slower (0.77 s): recorded `lsizenode` guards fail
  when trace and interpreter tables differ in node size, so both must agree.
- Not sound as-is: presizing is not layout-exact vs reference Lua (a presized table receiving integer keys keeps them in the hash: `pairs` order / `#` borders can differ; 3+ fields
  reinsert in a different order). Exact version = trace-only presize where the trace statically stores >= 2 constant string keys into the fresh TNEW, plus recorder/lockstep
  (`record.rs`, `interp.rs` lsizenode, `asm.rs` tnew_core, `opt_mem.rs`) knowing the size and a rule for exits in the window. Estimate: one large bundle, payoff ~40 % on classic_vector, nothing measured elsewhere.
- Cheaper untested alternative: remove redundant lookups in `newref_core` (`short_str_node`, `set_str`, `short_str_node`); would remove only part of the 50 %.
- Retest trigger: if an exact-layout trace presize is built, or a program set shows several constructor-heavy programs.
- **SINK-F update (cause of the `sinkmt=1,sinkside=1` slowdown, dug):** the loop's exit inside an inlined constructor frame names a sunk table, had no side trace, type-guard exits re-recorded and the head was
  blacklisted (trace entered 256 times against 391960 on the 32-line repro). With frame side traces for sunk snapshots and `sinkmt=1,sinkstub=1,sinkroot=2` (the new defaults) classic_vector is 0.89 x of the old default
  (0.388 -> 0.348 s), `sinkside=1` on top gives 1.05 x (A8).

- **P-TINS update (2026-11-08, docs/PERF.md "P-TINS")**: the "cheaper untested alternative" was built and more: `newref_core` now uses `TableRef::newref_shortstr` (two lookups instead of four, no `set`/`pset` round trip), the hash-only rehash skips `Counters`, `insertkey`/`mainposition` have inline fast cases, and `TableRef::finish_set` no longer copies its key with a 16-byte load (a store-to-load forwarding failure: the VM stores operands as two 8-byte halves). Constructor micro (fresh `{}` + 4 fields, escaping) 523 -> 374 cycles (LuaJIT 309), `classic_vector` 0.940 of the base (instructions 0.924), text_adventure `stli_other` -31 %; whole-set geomeans 0.97-1.00. Layout-exact presizing is still NOT done: the upper bound of the presize hack (-40 % on classic_vector, measured before these commits) was not re-measured and is certainly smaller now; retest trigger unchanged.

### BUNDLE-T1I: inline built-in calls in tier 1 (`t1inline`): BUILT, DEFAULT 1021 (2026-11-07, base 0a30941e); 1023 = everything since C36 (2026-11-08)
- Built: `type`, `math.floor/abs/max/min/sqrt`, `string.byte/len`, `select('#', ...)`, `rawlen` inline in the tier-1 `CALL` template, guarded on the callee identity, generic call on any failed guard; call-site name hints; 10 mutants; jit-diff template `t1i`. Full description, numbers and tests: `docs/JIT_TRACE_REF.md` "BUNDLE-T1I".
- Process default `T1INLINE_PROCESS = 1021` (re-measured on all 223 programs, `t1inline=1021` vs `0`: 0.997 cycles / 0.998 instructions over 219 programs, no program slower beyond noise; wins scheme_interp 0.871, inspect_lua 0.945, serpent 0.976 / 0.961 instr, msgpack 0.984 serial / 0.982 instr, roguelike 0.976, heldout geomean 0.980). The `math.floor` bit (2) stays OFF: `heldout/bigint_rsa` is 1.31 cycles / 1.24 instructions with it. Cause (verbose recorder logs, floor on vs off, same trace sequence until event 45): after the `SideShort` abort of a side at exit 5 of slot 8 the floor-on run starts an extra *root* recording at `proto:71` pc 30 (the `while carry > 0` loop of `mul_small`; the baseline starts nothing there), which aborts as `StartRefused (Wait(75))` and is followed by a recording at pc 32 that closes a loop; from then on the trace numbering, the parent slots and the `Rerec` of that root differ and two more heads end blacklisted (the multiplication loops stay in tier 1). The root start is driven by the loop-head hotness counters; 3 bytes of difference in the trace code move the iteration at which tier 1 versus trace code runs, so the counter fires at another moment. It is C36 (recording-moment fragility) and the start gate has no counters-only rule that would be safe for the other programs, so no fix was built. Floor alone matters (instructions of `t1inline=2` vs off) in: sha2_base64 0.949, containers_lru_heap 0.981, msgpack_pure 0.975, real_dis_arm 0.982 / mips 0.980 / ppc 0.986, text_adventure 0.983, particles_physics 0.983, raytrace 0.987, lib/pcall_err 0.970; geomean 1.000 over 219 only because of bigint_rsa 1.239.
- Retest trigger for adding the floor bit (`T1INLINE_PROCESS = 1023`): a fix of the recorder's start gate fragility (C36), or a gate that accepts a policy-trajectory regression on one program of 219 (the 1023 column is 0.997 / 0.998 over 219 programs too, but bigint_rsa 1.315 / 1.239; sha2_base64 0.932 and containers_lru_heap 0.964 need the floor bit). **Resolved by C36 (2026-11-08):** the cause was not the start gate but the missing spare `CallInfo` nodes (see C36); with `cispare` the floor bit is on (`T1INLINE_PROCESS = 1023`): bigint_rsa 0.944-0.953 cycles / 0.972 instructions of the 1021 default, sha2_base64 0.926 / 0.949, real_dis 0.982 / 0.985, all 219 other programs 1.000 / 0.999.
- Not built, with the reasons: (1) `TAILCALL` of the same built-ins (`return type(x)`; needs the one-result `RETURN1` sequence inline after the native result, ~40 lines); (2) `string.sub`, `tostring`, `string.char`, `string.format`, `tostring`: they allocate, an inline path would call a helper, which is what the study's H1 / H3 (leaner `jit_call`: -20 instructions, 0 cycles) measured as worthless; (3) `math.ceil` (needs an `FCeil` micro-op, `roundsd` mode 10); (4) aliases without a recognisable name (`local f = math.floor`) get no inline code (a runtime callee profile would fix it; names covered every program of the sets); (5) a tier-1-only (`RUSTYNAIL_JIT_TRACE=off`) variant: the knob lives in the trace limits like `tset` / `strself`.
- Study hacks H1 (lean native `jit_call`, 0 gain), H3 (inline native frame push + shim, -20 instructions, 0 cycles) and H4's per-program payoff: only the hot built-ins are worth inlining; the identity-test chain costs 3 instructions per row on every call of any other native unless the site is filtered (hints).


### C37: generic C calls in traces (feature 6 build attempt, 2026-11-08): STOPPED AT DESIGN, nothing built
- Task: a recorded `CALLC` IR op so loops with unknown natives (`string.format/find/gmatch/lower`, `table.concat`, ...) stay compiled instead of `CFunc`-aborting / stitching.
- Why the in-trace `CALLC` is not one bundle. The trace compiler's soundness rests on invariants a generic native breaks (REF "no GC step / no stack move / no allocation inside a trace",
  "helper calls never write savedpc/top", frames of inlined calls are virtual, sunk allocations and virtual GC debt (TVIRT), loop-invariant / forwarded table loads, `HMISS` and node-pointer
  caches, the lockstep recorder that predicts every VM slot after each instruction, `hooks are asynchronous-only`). Existing `TOp::Lib` helpers (`sub`, `byte`, `char`, `tostring(number)`, `random`, ...) are
  safe only because each does exactly what the C function does for the recorded argument kinds, never steps the collector and answers 0 ("did nothing") so the trace exits BEFORE any side effect and tier 1
  re-runs the call. A generic native cannot use that protocol (an error or a GC step happens after its effects). Needed, none of it existing:
  1. a full-barrier call snapshot: materialise every virtual frame / sunk table / live register-only value into the real stack and `CallInfo` chain (the exit machinery `mat_fn` does this, but only on the way out)
     and re-derive base / top / ci afterwards, then re-enter compiled code: i.e. a mid-trace exit-and-reentry with fresh type guards on every live slot (a second entry point per call);
  2. an error protocol from native to trace code (trace code is entered from Rust with no unwinding through it): native `Err` -> materialise the call snapshot -> return the pending error to the dispatcher,
     with `error`-position/traceback identical (frames must be real for `luaL_where`), plus a hook / yield / `__gc`-finalizer story (finalizers run arbitrary Lua during the native's GC step);
  3. `opt_mem` / `opt_loop` / forwarding / HMISS / array-pointer caches invalidated across the op (every table, every upvalue, the stack), GC timing equal to the interpreter's (TR29.6) given sinking and virtual debt;
  4. recorder lockstep for results of unknown type (type-guarded result loads), `stale` slices for the native's `check_gc`, hook/count budget identity, 5.4 arm (traces are off there but the cfg must compile);
  5. about 10 mutants, tr_core / jit-diff templates for errors, GC stress 1/2 in both modes, stack growth inside a native, hooks, natives replaced mid-loop.
  Estimate: 2-3 bundles for the in-trace op alone, and the soundness risk is high (it touches every optimiser pass).
- Cheaper alternative (recommended if feature 6 is pursued): LuaJIT-style **stitching with a landing trace**, built from machinery that exists. Today a stitched trace ends in an always-taken exit before the `CALL`;
  tier 1 runs the call and the rest of the iteration until the loop head re-enters the root. Instead: give the pc after a stitched `CALL` a dispatch cell and a root-kind recording ("continuation head"),
  exactly the way P-M3 continuation heads sit at the pc after a recursive call (`head_is_cont`, `RecCfg::rec_cont`, `D_H` cell in tier 1, entry guards read from the VM slots at start) and let that recording
  run to the loop head and link to the root trace (as a side trace links). The native call itself stays in tier 1 (measured: from trace code it costs the same, ~26-36 cycles trivial, find 121, format %d 128),
  so there is no error / GC / stack-move / barrier problem at all: every invariant above holds because no trace code runs across the native. Missing pieces: (a) tier 1 emits the post-call dispatch cell
  at call sites (heads are chosen at tier-1 compile time by a bytecode pattern, so a site needs a cheap way to be flagged, e.g. a cell at every `CALL` followed by a `RETURN`-free use of the results, or a
  recompile / patch when the first stitch happens); (b) a recording start state "native call just returned, results in slots, types guarded"; (c) the end of such a recording at the loop back-edge as a link to
  the root head (side-trace style) and its blacklist / penalty policy (C36-style fragility applies: the recording moment is counter-driven); (d) mutants and tests like `rec_cont`. Estimate: 1-2 bundles.
  Gain bound: only the non-call remainder of the loop moves into optimised code; the call stays ~100-130 cycles. A rough micro (tier-1/stitched path, wall incl. startup, 3 M iterations of an 8-table/arith-op body
  + one call): body alone 144 ms (LuaJIT 38, PUC 260); +find(plain) 175 (LuaJIT 19: it folds the call), +format("%d") 273 (190), +concat 230 (77), +tostring(i) 252 (192), +lower 249 (25), PUC 249-440. The
  call dominates every row (+30..130 ms of the total), so the remainder-in-trace win is bounded by roughly the 144 ms body column vs its compiled form, only for loops whose non-call work is large.
- Retest trigger: a program set where >= 5 programs spend >30 % of time in tier 1 + glue because of `CFunc` blacklists AND the landing-trace design is built; or a decision to accept the full-barrier `CALLC` project.
- **Landing-trace slice attempt (2026-11-08, builder run on main d5b17875): STOPPED BEFORE BUILDING, docs only, no knob, no mutant, registry count stays 468.**
  - **What is already stitched (read `Rec` CFunc handling, REF "Stitching").** A ROOT recording that reaches a non-intrinsic C `CALL` after >= `stitchmin` (32) bytecodes ends with an always-taken exit before the call; tier 1 runs the call and the rest of the iteration, the next head dispatch re-enters the root. Side recordings never stitch (`CFunc` abort). Gap (a): `for w in s:gmatch(..)`: the head is the `TFORCALL` itself (text_adventure `Game:parse`, proto:79 pc 8; the log shows `abort proto:79 pc 8 CFunc` then `head pc 8 ... blacklisted`): the prefix is 0 bytecodes (< `stitchmin`), so nothing is stitched and the head is blacklisted; the body after the call is ~10 bytecodes (two compares, `words[#words+1] = w`). Gap (b): C calls mid-body in loops whose prefix is < 32 bytecodes, and every call in a SIDE trace.
  - **Why the landing trace has no gain bound worth building (measured, same binary, `--features jit` release, `RUSTYNAIL_SEED=0`, CPU 4, `perf stat` cycles:u / instructions:u; scratch micros, not committed).** A landing trace (root at the pc after the C call, ends with an always-taken exit before the next call, tier 1 does the call, the continuation cell re-enters) has exactly the per-iteration cost structure of an existing stitched root with the roles swapped: one trace entry (guards), one always-taken exit (`exit_entry` + `restore`), the call in tier 1. `stitchmin=4` forces that structure on a loop whose body is ALU / table work plus one `string.find(s, "wor", 1, true)` at the end:
    | body before the call | `stitchmin=0` (tier 1) cycles / instr | `stitchmin=4` (stitched) cycles / instr | ratio cycles / instr |
    |---|---|---|---|
    | ~30 bytecodes (3 M iter) | 563.6 M / 2.511 G | 580.5 M / 2.740 G | 1.030 / 1.091 |
    | ~120 bytecodes (1 M iter) | 397.3 M / 1.474 G | 381.9 M / 1.703 G | 0.961 / 1.155 |
    Instructions get WORSE by 9-16 % at every size (the exit materialises and restores, tier 1 re-dispatches), cycles only win (4 %) once ~120 bytecodes ride in the trace. text_adventure's gmatch body is ~10 bytecodes and `real_dis_*` / `json_rxi` loops are the same shape (short remainder after the call): a landing trace there would be a clear instruction regression (entry guards + exit per iteration vs ~10 tier-1 bytecodes). The measured premise of the study ("the call stays ~100-130 cycles, only the non-call remainder gains") holds, but the remainder gain at tier-1-vs-trace is smaller than the entry+exit pair for any body under ~100 bytecodes, i.e. below where the stitching default of 32 was already found to lose (F2b: 0.16 s tier 1 vs 0.17-0.21 s stitched at ~50 bytecodes).
  - **What would change the verdict (next-attempt design).** The pair entry+exit per call is the cost, so the landing trace only pays when the call does NOT leave the trace: that is the in-trace `CALLC` (design above, 2-3 bundles), or a cheaper "exit-less stitch" in which the trace *calls* the native through a helper with a full materialising barrier and continues (the same invariants as `CALLC`, so no cheaper). A cheaper intermediate that does not need either: make the always-taken exit and the re-entry cheap (a "stitch exit" that skips `restore` of slots the continuation guards re-read, entry guards only for slots the remainder reads); that attacks the 9-16 % instruction overhead directly and is worth a measurement before any landing-trace work. A `for ... in gmatch` special case (the `TFORCALL` of `gmatch_aux`) as a recorded intrinsic (`Lib` helper that does the match step for a plain-capture `%a+` style pattern, answering 0 when it cannot) would remove the call from the stitch problem for the single most common C iterator without the barrier; it is the better-targeted slice for text_adventure / real_dis_*.
  - **Update (BUNDLE-GM, 2026-11-08): the gmatch slice was built** (docs/JIT_TRACE_REF.md "BUNDLE-GM", docs/PERF.md "BUNDLE-GM"): `gmatch=1` default, one-string results (whole match / one capture), end of iteration exits. Wins: real_dis_{mips,ppc,arm} 0.955 / 0.969 / 0.975 instructions, wordfreq_wrap 0.954, lib/gmatch_words 0.917; text_adventure itself is +0.6 % (2-3 words per loop: entry + end exit cost as much as the saved `jit_tforcall` work). Open: a side recording behind the loop end, tier-1 fast path for the iterator, several-capture results.
  - **State.** No code change; jit-diff / perf gates not run (nothing to gate).

### C38: `corpus/lua_lexer`, data-dependent callee loops under the `loopany` guard (feature 6 study, 2026-11-08): STOPPED AT DIAGNOSIS, nothing built
- **Task.** Keep the data-dependent inner loops of `Lexer:next` compiled (nested / linked loop traces) without the `heapsort` exit storm, the `awfy/cd` cost or losing `iter_pipeline`.
- **Confirmed sequence** (plain release `--features jit`, `RUSTYNAIL_JIT_VERBOSE=2`; proto:80 = `Lexer:next`, proto:0 = the main chunk, loop head pc 127 = the token loop):
  1. Roots at proto:80 pc 1 (whitespace loop of `next`, trace 1) and pc 0 (fn entry, trace 8) install first, then the **main token loop** root (trace 14, proto:0 pc 127, 36 KB; `next` inlined; it unrolls no callee loop itself: `Trace::la` empty).
  2. Its side traces taken in the callee frames (exits inside `next`, `is_alpha` ...) unroll `next`'s `while` loops (`loopany`); five callee-frame side attempts exceed `loopanymax = 4`: `loopany guard: ... root unrolled nothing: pruned side traces [12, 15, 16], root kept` (`laprune`, plan `Sides`), the callee loop sites (`next`: pc 1 and pc 99 are the sites that matter) are blocked **process-wide**.
  3. The root's hot exits at the callee loop tests (the pre-roll marked exits 9 and 11, count 2^30+) now find no side trace: every recording through the blocked site aborts `InnerLoop`. Trace 14 is flushed `Ineffective` after 880 entries / 1394 exits, the re-record (48) aborts `InnerLoop`, the next (51) too and the head pc 127 is blacklisted. Other heads: a loop root at proto:80 pc 99 (the identifier loop, trace 42) installs and is entered from tier 1 only.
  4. End state: the token loop and the callee code run in tier 1 (with `laprune=0` the family flush and re-record happen at an earlier moment and the result is 6 % better, 1.31 G against 1.39 G cycles).
- **Cycle buckets** (perf record cycles:u, 200000 period, perf map symbols; CPU 4; total 1389.8 Mcyc default, 1198 with `loopanymax=0`): default trace code 348 M, tier-1 code 262 M, exit handlers (`exit_entry` + `restore`) 122 M, JIT helpers 295 M, stdlib (pattern / find / sub / concat) 188 M, heap/gc/table 78 M, other 96 M. `loopanymax=0`: trace 444 M, tier 1 122 M, exit handlers 7 M, helpers 314 M, stdlib 145 M, heap 72 M. cycles (min of 3, CPU 4): default 1389 M (5.80 G instr), `laprune=0` 1321, `loopany=0` 1299-1333, `loopanymax=0` 1170 (4.84 G), `nest=2` 1566 (6.69 G), `nest=2,loopany=0` 1543, `nest=0` 1384. LuaJIT 2.1: 454 M cycles / 1.91 G instr (3.06 x of ours).
- **What LuaJIT does** (`-jv`): the token loop is never a root. Its first recording aborts "inner loop in root trace" (and "leaving loop in root trace"), the inner loops compile first (trace 2 = the whitespace loop in `next`, trace 6, 14 = the other loops), and everything else is a *side trace of an inner-loop trace that leaves the loop, returns from `next` below the trace's base frame, runs the rest of the token loop body and links back to the head of the inner loop trace* (`(3/5) ... -> 2`, `-> 6`, `-> 14`; about 45 traces, all but a few are such side traces). The root-frame return and re-entry is the part that ours lacks.
- **Why the machinery that exists does not do it.** (a) `nest=1` links only a root recording that reaches an inner loop head *of its own frame*; (b) `nest=2` (`nest_frame`, `frame_head` in `Rec::step`) links a **root** recording that reaches a callee-frame loop head with a root trace by a linking exit that materialises the frames; `side.is_none()` is required, a *side* recording at a callee loop head gets no link (it unrolls under `loopany` or aborts `InnerLoop`); in the lexer the exits at the callee loop tests are exactly side attempts. And the loop trace it links to leaves `next` by an exit to tier 1: its side traces cannot cross the return of the root frame (`LeaveFrame` aborts: 16-17 side aborts in the lexer logs), so every token pays materialise + exit + tier-1 for the rest of `next`, the return and the loop body (`nest=2` measured 1.12 x slower than the default, 1.17 x with `loopany=0`).
- **Why no counters-only guard tweak separates the shapes** (measured; knob `loopanymax` only, cycles min of 3): `lua_lexer` at `loopanymax` 4 / 6 / 8 / 16 / 24 / 32 / 48 / 64 = 1377 / 1308 / 1346 / 1338 / 1388 / 1316 / 1306 / 1162 M (the guard fires up to 48 and not at 64; unguarded 1170); `heapsort` at 4 / 8 / 16 / 24 / 32 / 48 / 64 = 1086 / 1087 / 1081 / 1083 / 1085 / 1634 / 1627 M (storm from 48). There is no window: the lexer needs more than 48 callee-frame side attempts in a family, `heapsort` storms at 48. Both counts are paths of a state machine (lexer: one per token kind / loop test) against trip counts of one loop (heapsort), and the counter (`Trace::la_sides`) counts callee-frame hot exits without telling which loop test or which trip count they belong to. A per-pc histogram of the counted exits (instrumented, not kept) does not separate either: `heapsort` 3 pcs with 22 / 12 / 8 exits, the lexer 15 pcs mostly with 1-7 each, `cd` 12 pcs with up to 67; repeats across flushed generations blur the picture.
- **Design of the real fix (more than one bundle; not built).**
  1. **Side-recording links at callee loop heads.** In `Rec::step`, a side recording (`cfg.side.is_some()`) with `depth > 0` that reaches a callee loop head with a root trace and a blocked (or not unrolled) site ends with the existing frame-link close (`close_frame_link`) instead of `InnerLoop`. Needs: the `frame_side_ok` / virtual frames of the parent snapshot to be consistent with the link-cell protocol (`ExitSnap::link_cell`), mutants `side_frame_link_never`, tr_core program with a root loop, a data-dependent callee loop and a call from a side trace.
  2. **Side traces that leave the root frame.** The callee loop trace (root frame `next`) must have side traces that continue through `Return` into the caller frame (a frame below the trace base, outer frame real in tier 1's `CallInfo` chain), run the caller loop body and link back to the loop head (LuaJIT's `(3/5) -> 2`). That is a change to the trace frame model (a negative-depth recording: the root frame is not the lowest frame; snapshots restore frames above *and* below; `base` moves on return; `LeaveFrame` today), the stack-check / `top` bookkeeping (`ExitMeta::depth`, TR6b), the entry (`jit_tr_entry`) frame assumptions, the lockstep recorder's slot predictions and the sink / escape passes. Estimate 2-3 bundles, soundness risk comparable to C37.
  3. **Policy.** With 1 and 2 the guard's blocked sites no longer abort the token loop, so `loopanymax` can stay; a *re-record of an ineffective root whose recordings abort `InnerLoop` at blocked sites* should keep the old root rather than blacklist the head (alone: the root is entered 880 times and the ineffective rule fires on the pre-roll exits, so keeping it is not a win without 1 and 2).
- **Smaller options considered and not built.** (i) Per-family (not process-wide) site blocking: the sites are blocked for the families that stormed only; the lexer family is the only user of those loops, so it does not change this program. (ii) Raising `loopanymax` to 64: fixes the lexer (-16 %), `heapsort` storms from 48 (1.5 x). (iii) Dropping the guard for roots that unrolled nothing (`laprune` plan `Sides`: do not block): is `loopanymax=0` for such families; unmeasured outside the lexer, likely a repeat of the `cd` cost (the `laprune` defect description).
- **Retest trigger.** Feature 6 or any task that touches the root-frame model; programs of the lexer shape (state-machine scanner with several data-dependent loops in a called function): `corpus/lua_lexer` (3.1 x), `dev/lua_parser`, `heldout/real_dis_*`, `json_rxi`, `text_adventure` (not measured in this study beyond `lua_lexer`).

- **Slice 1 built (C38 slice 1, 2026-11-08): `sidenest=1`, design item 1; knob OFF (`SIDENEST_PROCESS = 0`).** A SIDE recording (`depth > 0`) that reaches a loop head of an inlined callee frame with a root trace now ends with the existing frame-linking exit (`close_frame_link`) instead of unrolling the loop under `loopany` or aborting `InnerLoop` (`RecCfg::nest_frame_sides`; in `Rec::step` the side arm `reached_head == None && frame_head`). Tests: `tr_core::c38_side_programs` (programs `c38_side`, `c38_side2`, both trace modes, +verify / exit storm / budget 17 / GC stress 2 / emulator), `tr_policy::c38_policy`, mutant `side_frame_link_never` (registry 456, caught). jit-diff seeds 1..40 (`off tr trdef`, `lockstep=1,sidenest=1`) and 41..160 (`+nest=2`): 0 diffs; tr_core 133, tr_policy 38, vm_corpus 9 pass. Stat `sidelinks` in the `[trace] links:` line.
- **Result: does not fire where it was wanted, and is not clean.** In `lua_lexer` the callee loop heads of `next` have NO root trace at the moment the side attempts hit them (`abort proto:80 pc 99 LeaveFrame`: a loop root inside `next` cannot form because its trace leaves the frame by `RETURN`: design item 2), so `head_traced` is false and the side recordings still abort `InnerLoop`: 0 frame links in the verbose log; instructions 0.992 (cycles 0.987), within what a different recording order gives. It fires on programs that have a rooted callee loop plus a rarely taken call (`c38_side`). Same-binary pinned A/B (`sidenest=1` over `sidenest=0`, CPUs 3-7, one lane per core, min of 5, `RUSTYNAIL_SEED=0`, equal-length knob strings, dup column = `sidenest=0` again; cycles / instructions geomean): awfy (14) 1.014 / 1.026 (queens 1.164 / 1.358, havlak 1.018 / 1.034, cd 1.025 / 1.019), corpus2 heldout excluding real_dis (11) 1.008 / 1.008 (roguelike_turns 1.140 / 1.132, sudoku_solver 1.024 / 0.967), lib (95) 1.001 / 1.000 (pcall_err 0.987 / 1.026), jit-trace (29) 1.000 / 1.000, programs (25) 0.997 / 1.000 (heapsort 1.011 / 1.000), corpus (18) 1.000 / 1.001 (msgpack_pure 1.022 / 1.022, lua_lexer 0.987 / 0.992), corpus2 dev (17) 0.998 / 1.000 (lua_parser 0.955 / 1.018), real (5) 1.000 / 1.000, real_dis (2 dev, 2 heldout) 0.998 / 1.000 and 0.997 / 1.000; iter_pipeline, stream_closures, classic_vector within 0.1 %. Dup columns 0.996-1.001 / 1.000. The gate (no set or program > 3 % instructions) FAILS on awfy queens (+36 %), roguelike_turns (+13 %), havlak, pcall_err: linking a side trace into a callee loop trace replaces an unrolled path with materialise + exit + tier-1 landing + entry (the same cost as `nest=2`), a loss when the callee loop trip count is short. Hence off, no enabling rule found. **Update N-B: ON now** (the inline re-push stub removes the cost; no short-trip guard needed, see the N-B paragraph below).
- **What the next slice needs** (unchanged from design items 2-3): loop roots in callee frames that survive a `RETURN` (side traces of the loop trace that leave the root frame and link back, LuaJIT style) so that the heads exist and the link lands in compiled code instead of tier 1; only then can `sidenest` be turned on, together with a short-trip-count guard (queens / roguelike_turns lose where the loop runs a few iterations per call).
- **Slice 2 attempt (2026-11-08, builder run on main d5b17875): STOPPED AT DESIGN, docs only, nothing built.** Task: side traces of a callee-frame loop trace that return below the root frame, plus the cheaper alternative (un-blocking `loopany`-blocked sites). Read: `Rec::ret` (`d == 0` -> `LeaveFrame`, or `fn_end` / `end_rec_ret` for function and recursion-continuation roots), `snap.rs` (`SlotMap` is indexed by **root-relative non-negative slots**; callee frames are just slots above the caller's), `Rec::target`, `cache::side_attempt`.
  - **Cheaper alternative measured and rejected (no code).** Same binary (main d5b17875 `--features jit` release), `RUSTYNAIL_SEED=0`, `perf stat` cycles:u / instructions:u, CPU 4, one run each (instruction counts are deterministic), equal-length knob strings. The guard-free variant for roots that unrolled nothing (the `Sides` plan; `loopanymax=0` is the closest existing knob and also unguards the `Family` plan) against default `loopanymax=4`: `corpus/lua_lexer` 4.83 G / 5.60 G instr (0.86x) [cycles 1.163 G vs 1.339 G], `awfy/cd` 1.122 G vs 0.807 G (**1.39x**, cd is a `Sides`-plan family), `programs/heapsort` 5.60 G vs 3.31 G (1.69x, `Family` plan), `corpus2/dev/lua_parser` 5.688 G vs 5.688 G (equal; cycles 0.98), `corpus2/dev/text_adventure` identical instructions (8.906 G): **`text_adventure` is not touched by `loopany` at all** (so neither C38 slice can move it). `laprune=0`: lexer 5.50 G (0.98x), cd 0.837 G (1.04x), others equal. cd and the lexer are both `Sides`-plan families that unroll callee loops only in side traces, so the plan cannot separate them; an "unblock on a phase change" rule needs a counter that distinguishes the lexer (many distinct loop-test exits, each hit a few times, the root hot) from cd (the same counts, interpreter exits 29-57 K per exit): the earlier per-pc histogram study found none. Not built, the loss on cd (+39 %) is the price of unguarding.
  - **Why frame-leaving side traces are not one bundle (the precise obstacle).** A loop root recorded in a callee frame `F` (e.g. proto:80 pc 99) has `F` as frame 0: all slot indices, `SLOAD`/`SSTORE` offsets, snapshots (`SnapFrame` = frames *above* frame 0), `ExitMeta::depth`, the stack-extent check and the `jit_tr_entry` contract (`base` = frame 0's base, the `CallInfo` of frame 0 is the top one) assume frame 0 is the lowest frame the trace knows. A side recording that executes `RETURN` out of `F` needs (1) a **negative-slot extension** of `SlotMap` / the IR slot space: the caller's registers live at `base - (A+1) ... `, with the fixed offset of the callee function slot `A` taken from the `CALL` at the caller's `savedpc`; (2) **runtime guards for the caller identity** (the `CallInfo::previous` closure proto, its `savedpc` == the recorded pc after the call, `func` offset == `A`, and no hook / vararg adjustment / C boundary in between), emitted once per recording because the root loop trace never needed them; (3) snapshots after the return that restore the **callee frame's destruction** (the `CallInfo` chain pops: the exit state must be "caller frame is top, `L->ci` moved") together with the slots of both frames, i.e. a snapshot kind whose frame list *shrinks* below the entry state, which `restore` / `exit_entry` / `ExitMeta::depth` / TR6b `top` bookkeeping cannot express (frames are only ever *added* above the entry frame); (4) the lockstep recorder's slot predictions and the sink / escape passes (`sink.rs`) keyed on root-relative non-negative slots; (5) the **link back** to the inner loop head: the link target's entry state has frame `F` as the top one with `base` = the old callee base, so the continuation in the caller must *re-push* `F`'s `CallInfo` (a call-in-trace) when it jumps to the head, the part LuaJIT gets from its frame-stack-in-trace model (`-> 2` links with the base re-established by the `base` slot of the snapshot); that re-push is the `ci` creation of the `nest=2` frame-link exit, which today leaves through tier 1. Estimate unchanged (2-3 bundles, soundness comparable to C37).
  - **Smallest sound step if the next attempt starts (design for next time).** (a) Add the *caller-identity guard* and a `RETURN`-out-of-root continuation that ends immediately with a stitching exit whose snapshot has frame list `[]` and `pc` after the caller's `CALL` (no recording of the caller body): this removes the `LeaveFrame` abort of a callee-frame loop root and the 16-17 `LeaveFrame` side aborts, so the callee loop heads of the lexer get roots *and* their exit through the return is a cheap native stitch instead of materialise + tier 1. That alone is checkable by the lockstep recorder and needs only the shrinking-frame snapshot (3); (b) then record the caller body behind it (negative slots, 1, 4); (c) then the re-push link (5); (d) then enable `sidenest` with a trip-count guard. Each step needs its own mutants and `tr_core` programs (frame-below-root program: loop in a callee called from a loop; callee returning inside a `pcall`, through a coroutine boundary, under hooks, vararg callee, recursion).
  - **State.** No knob, no mutant, no code change; registry count stays 468 (verified in tests/mutants.rs only by reading; nothing was changed). jit-diff / perf gates not run (no code to gate).

- **Slice 2 re-assessment (BUNDLE N, 2026-11-09, builder run on main ece9f891): STOPPED AT DESIGN again, docs only, no code, no knob, no mutant (registry unchanged).** Re-read `Rec::ret` (`d == 0`: `end_rec_ret` / `fn_end` / `LeaveFrame`), `fn_end`, `Rec::step` side arm and the verbose log of `corpus/lua_lexer` on this main (release `--features jit`, `RUSTYNAIL_JIT_VERBOSE=2`): 162 abort lines, of which 56 side aborts (30 `LeaveFrame`, 8 `TailCall`, 7 `Nyi(Call)`, 6 `SideShort`, 4 `InnerLoop`, 1 `CFunc`) and 30 root aborts at proto:80 (`next`) that are all `StartRefused (Wait(..))` back-offs at pc 69 / 76; the `LeaveFrame` aborts are side recordings of callee-frame loop roots reaching the `RETURN` of their root frame, as C38 says.
  - **Cheap variant considered and rejected: end the recording with an always-taken exit before the root frame's `RETURN` (as `fn_end` does for function roots; tier 1 executes the `RETURN`).** It is sound (an exit before an instruction needs no frame shrinking: the snapshot is the state in the root frame), and it would remove the `LeaveFrame` abort. But it only turns "abort" into "install a trace that ends in an always-taken exit": the caller's loop body after the `RETURN` (the token loop, which is where the lexer's time goes) stays in tier 1, so every token still pays trace exit + tier-1 `RETURN` + tier-1 token loop + re-entry; the same cost structure as `nest=2` (measured 1.12x slower than default). For a loop root it would also need a non-looping root kind (the loop optimiser, PHIs and the `Trace::la` / link-cell machinery assume a back-edge), i.e. a third root kind next to loop and function roots. Not a win without the continuation into the caller, so not built.
  - **The real work is unchanged and is the negative-slot model** (items (1)-(5) above: caller-identity guards, shrinking-frame snapshots, slots below the root base in `SlotMap` / sink / lockstep, re-push link). It cannot be built behind a default-OFF knob in a way that is testable in small increments: step (a) of the earlier plan is the only independently checkable one and by itself has no performance value (see above). Given the stated rule (soundness first; stop with docs if infeasible within the bundle), nothing was shipped. N2 (`sidenest` + short-trip guard) and N3 (`tfmt` retest) depend on N1 and were not started; `SIDENEST_PROCESS` stays 0 and `TFMT_PROCESS` stays 0.
  - **Suggested restart point.** A dedicated bundle for the frame model alone, with no performance goal: negative-slot `SlotMap`, caller-identity guard IR op, shrinking-frame `ExitSnap` and the `restore` / `exit_entry` / TR6b changes, validated by a recorder that ends immediately after the `RETURN` with a stitch exit (step (a)) under lockstep/emulator/exit storm; only then record the caller body and link back. `fside_room_ignored` stays the single documented mutant survivor until a frameful side trace exists.

- **Slice 2 BUILT, partly shipped (BUNDLE N-B, 2026-11-10; docs/PERF.md "BUNDLE-N-B", docs/JIT_TRACE_REF.md "BUNDLE-N-B").** N3a: side recordings go on in the caller after the pop (knob `retf` bit 0, `retfmax`), N3b: the frame-linking exit is an inline stub (bit 1), N3c-lite: function-entry families (bit 2), N4: `sidenest` + the stub ON, N5: `tfmt` ON. **Shipped defaults: `sidenest = 1`, `retf = 2`, `tfmt = 1`** (223-program geomean 0.9999 cycles / 1.0002 instructions for sidenest + stub, 0.9987 / 0.9986 with `tfmt`; queens 0.884, roguelike_turns 1.009: the stub cured the earlier +36 % / +13 % without a trip-count guard, the cost was the Rust handler re-push, not the short trips). **FLIPPED afterwards (2026-11-10, user waived the lua_lexer gate): `RETF_PROCESS = 3` (`retf=3,retfmax=1`: bit 0 ON, bit 2 stays OFF), numbers in docs/PERF.md "RETF flip": all-223 0.9975 cycles / 0.9980 instructions against `retf=2`, short_inner 0.637, field_rw 0.923, list 0.937, lua_lexer 0.997, no program above +1 % instructions, `programs/matmul` 1.03 at the limit (identical trace statistics, not diagnosed). The rest of this paragraph is the state when N-B shipped. NOT shipped then: RETF recording (`retf` bits 0 and 2).** The gate "lua_lexer <= 0.92 cycles" fails with every setting (`retf=3,retfmax=1`: 1.020; with bit 2: 0.977 cycles / 0.919 instructions); bit 0 alone gives `short_inner` 0.637 (with `sidenest`), `awfy/list` 0.951, `field_rw` 0.922, `dijkstra_routes` 0.964 and no regression above noise (`retf=3,retfmax=1` geomean 0.9970 cycles / 0.9983 instructions, passes every gate but the lua_lexer one, **flip candidate if the lexer gate is waived**); bit 2 regresses native-recursion families (`lua_interp` 1.40, `regex_nfa` 1.25, `roguelike_turns` 1.45, `dijkstra_routes` 1.26, `stream_closures` 1.05; `Compile(asm-inconsistent)` aborts in `lua_interp`) and needs its own policy (keep function-entry RETF out of recursive prototypes, look at the asm inconsistency), `retfmax = 2` costs `awfy/list` +24 %. Why lua_lexer stays at ~1.0 with bit 0: its `Lexer:next` is a function-entry family, so only bit 2 reaches it, and the callee scanning loops are short and dominated by library calls (`jit_tr_alloc`, `string.sub`, `tconcat`: 40 % of the cycles); the hand-inlined ceiling (0.78) removes the call and the token tuple, which RETF does not. **Cheap retests:** `retf=3,retfmax=1` (columns with `scripts/bench_session.py`), `retf=7,retfmax=1` for the function-entry bit; retest triggers: a recursion-aware policy for bit 2, a `sink` pass for RETF traces (the returned token tuple is a candidate), a `LeaveFrame` re-arm policy.
- **Slice 2 infrastructure BUILT (BUNDLE N-A, 2026-11-10; knob `retf` default 0 = OFF, byte-identical IR and machine code with it off): the RETF model of the N0 study, steps N2a-N2c; no performance goal and no default flip.** The replacement for the negative-slot model is a REAL native pop plus a re-root (LuaJIT's `RETF`): `TOp::Retf` guards the caller's identity, pops the root frame (`thread.ci = r13 = C`, `rbx -= 16 * (A + 1)`, `r14 = C.kcache`) and the trace goes on with `C` as its root frame; slots stay `u16`, numbered from the base of the root of their SEGMENT (`Snap::seg`, `TraceIr::segs`); exit stubs and `restore` do not change their model (they build frames above `r13`, which after the pop IS the new root; only the root code pointer is per snapshot now: `ExitSnap::root_code`). Details: docs/JIT_TRACE_REF.md "BUNDLE-N: RETF".
  - **Built:** N2a segment plumbing (`Snap::seg`, `TraceIr::segs`, `ExitSnap::{root_code, seg_shift, seg_pops}`, `build_exits` landing key per segment, shared frame-rebuild copies keyed by root code, `rn-core jit_hooks::rec_reroot`, knobs `retf` / `retfmax`); N2b the op end to end (asm guards: `callstatus & RETF_CS_MASK == nresults + 1`, caller no C frame, `savedpc == site`, `k` form: no tbc / no open upvalue at or above the frame base; emulator `SetFrame`, register allocator excludes V1 / V2 / V10 from the trace, model `retf_below` / `shift`, `Uvrel` CSE chain cut in `Builder::emit_raw`, passes that assume one root frame skip traces with segments, `entry_checks` hoists only before the first RETF, `entry_kind` requires `seg == 0`); N2c `Rec::retf` in SIDE recordings (not function-entry families, not with `lazyci`, not 5.4): the recording rebases (slot map, known types, stale set, frame stack, root fields, extents, deps, lockstep model in IR order) and ENDS with the stitching exit at the caller's first instruction after the `CALL` (segment 1 snapshot). `+verify` segment oracle (`AsmEnv::seg_oracle`, `ExitMeta::seg_oracle`): the entry `CallInfo` is stored at the entry and every segment exit goes through the handler, which checks `r13 == entry popped k times` and the function slot. `+emurun` works (the emulator now hands its pinned ci / frame / k to helpers); `+emuverify` skips traces with a RETF (the private copy of the frame cannot hold the caller's region).
  - **What it is worth alone:** nothing (it saves the tier-1 `RETURN` only). N3a (continue recording the caller body), N3b (inline re-push stub through the head cell) and the policy that re-arms root recordings (`LeaveFrame`) are NOT built; the default stays 0 until the N3 gate (lua_lexer <= 0.92 cycles, no program > 3 % worse, 5.4 build, ASan, full mutant run).
  - **Hazards the stop-after-the-pop cut avoids, to be faced by N3a:** (R4) an upvalue alias into the caller region written before the RETF while the model / `SlotMap` holds the old value (`RetfAlias` abort at record time, not built: nothing is recorded after the pop yet); `known` / `func_ref` / entry-guard hoisting after the pop (the rebase code is there and the `entry_checks` rule is built, but no instruction after the pop is recorded, so mutants for them would be equivalent and were NOT registered); room accounting of frameful children of a segment-k exit (`side_attempt` translation, `retf_extent_unshifted`, `retf_maxci_unshifted`: the children of the only post-pop snapshot are frameless, extent / depth are entry-relative maxima computed at close).
  - **Deviations from the N0 spec:** the `callstatus` guard compares `NRESULTS | C | FRESH | CLSRET | TBC | OAH | HOOKED | YPCALL | FIN | MMRET` against `nresults + 1` (the spec said "all bits but TAIL": `CIST_JIT`, the C-call counter and the recover status vary between record and run without mattering); `RETF` with `k` is supported already (the tier-1 `RETURN` tests are reused), not only refused; `V1`, `V2` and `V10` are excluded from register allocation in a trace with a RETF (`ra::op_clobbers`) instead of saving live registers around the guards; the model's slots are rebased when the lockstep evaluates the `RETF` (not at record time: the loads before it are evaluated one step later).

### C41: megamorphic dispatch in traces (P3, 2026-11-09): BUILT, SHIPPED
- **What was built.** `ncallee` (a root whose callee guard sits in the entry segment and was flushed for entry failures is recorded again with the guard in the body, so a callee mismatch is a side-trace exit instead of an entry failure) and `chainre` (a pass-through side trace of a call chain, whose chain would visit >= 1 link more per entry than hotness order, is flushed with its subtree once per exit, bounded 3 per exit / 12 per family, so that the exit re-records the callee that is hot now). Mechanism, soundness (TR8 / TR18 / TR32), mutants and tests: docs/JIT_TRACE_REF.md "P3"; numbers: docs/PERF.md "P3".
- **What the lottery looks like.** The re-record samples the callee seen at the exit when its counter reaches `hot_exit`, i.e. in proportion to the current shares; hot callees win with their share, cold ones are appended later. A chain of more than ~8 links (K >= 16 uniform) is slower than tier 1 and neither rule helps: the old behaviour gave the loop up (fdisp K=16 1126 Mcyc), the new one keeps a 15-link chain (1314 Mcyc). A `loopany` side-attempt counter that is not refunded on a flush starves the re-recorded chain (forth_vm: 3 links, half of the calls in tier 1): the refund is part of the rule.
- **Not done / open.** (1) A give-up rule for chains deeper than the break-even (K=16 fdisp, mdisp K=16 lose to tier 1 already in the base). (2) The periodic check costs one `cmp/je` on every attached callee-guard exit of a side trace (mdisp +1..5 % cycles; frame-cell exits that are not `mat_stub`s, and sunk-frame stubs, have no check at all). (3) Roots' own callee-guard exits are never re-ordered. (4) `ncallee` is set by any entry-failure flush of a trace with a callee guard in its entry segment, also when an `SLOAD` check failed.


### C42: PHI coalescing and snapshot-only PHI temporaries (P2 bundle 1, second half, 2026-11-09): NOT BUILT
- **What was asked.** After freeing registers: compute the body value of a PHI straight into the PHI register (no back-edge shuffle) and keep snapshot-only PHI temporaries (the dead Lua temporaries TR3 forbids pruning from snapshots) in a slot written once per iteration.
- **Why not.** The register file half (`xregs`, docs/JIT_TRACE_REF.md "P2 bundle 1", docs/PERF.md) already took field_method from 6 PHIs in spill slots to registers (instructions 0.69), so the case the study measured at 33 % of a loop is gone. What remains is the 3-move rotation of dead temporaries at the back-edge (`programs/spectral_norm` `Atv`: 6 moves in a 37-instruction loop; its dead temporaries are the previous `j` and the previous `A(i, j)`): a slot per temporary turns the moves into stores (same instruction count, no scratch-register cycle), and coalescing cannot help because the old value is still live in the snapshot when the new one is computed. Both are `ra.rs` PHI-handling changes with the TR3 / TR27b snapshot contracts in play and were not attempted in the same change as the register file; gain not measured.
- **Related, open.** (1) `programs/spectral_norm` +7.8 % cycles with -2.5 % instructions at `HEAD_OFFSET` 24 under `xregs` (layout lottery of Q2; a sweep of the offset shows 5 of 7 offsets fast), `jit-trace/rec_loop` +4..6 % cycles with `rbp` alone (unexplained): these two keep `XREGS_PROCESS` at 0. (2) `r12` / `r13` as allocatable registers (needs memory copies of the thread and `CallInfo` for every `CallH` and exit): not tried.


### C43: upvalue / closure specialisation, the parts not built (P2 bundle 2, 2026-11-09)
Built: `uvsep` (guarded disjointness of upvalue cells recorded on different `UpVal`s) and `uvroll` (docs/JIT_TRACE_REF.md "P2 bundle 2"); `uvroll` is off (`corpus/lua_lexer` +2.8-3.7 % cycles).
- **Closure identity instead of proto identity.** Guarding the closure object itself (`EQREF` against the closure address) would make `UREF` a constant and `UVREL` / `ULOAD` addressing static, but the constant is an address in the code and a collected closure's address is reused by a new one: it needs a GC dependency on the trace (flush when the closure dies) or a per-trace closure handle, both heavier than the `NE` guard of `uvsep`. Not built.
- **`uvroll` for the peeled layout.** A loop with a constructor in its body (`for ... do t = {..} ... uv = ... end`) still gives up at the first header-mutating op and is not peeled (no hoisting of its upvalue loads); only rolled traces (function roots, side traces) profit. Doing it for the peel needs per-class facts instead of the all-or-nothing `header_safe` rule: upvalue facts (and `UREF` / `UVREL` / `UGE` hoisting) stay valid across the op, table facts end at it, and the PHI / pre-roll bookkeeping must know which of the two a hoisted instruction depends on. Medium effort, soundness risk in the invariant analysis.
- **Why `uvroll` is slower on `lua_lexer`.** Instructions -0.5 %, branches and branch misses equal, cycles +2.8..3.7 % (15 runs). Not diagnosed beyond that (layout lottery like Q2, or a forwarded value kept live across an allocation helper call that spills it); a diagnosis would start with the trace dumps of `lua_lexer` under `uvsep=2` vs `0` (traces that were unoptimised and now are). The whole gain of `uvroll` (forth_vm 0.926, astar_jumper 0.950, varargs 0.947, stream_closures 0.959) waits on this.
- **RESOLVED (2026-11-09, task E): the `f2_lib` sum was not a `uvroll` miscompile; it was the exit storm.** Reproduced with the `uvroll_state_libs` mutant (which removes the cut at `RANDOM` / `SETMT` / `GMATCH`) and `tr_core::adhoc_file`; a delta-debugged standalone program and, more tellingly, a plain three-draw loop (`rand_storm`, `tr_core/progs.rs`) fail in the DEFAULT configuration (`uvsep=0` or 1, no mutant) under `+exitstorm=17` (also 19, 20, 22, 23, 25, 27, 28, 30, 31, 33; the periods 2..16 happen not to hit). Cause: `asm::guard_label` makes every guard fail every N-th execution (`+exitstorm`), exempting only the guards behind `Tnew` / `Snew` / `Concat` and the `GMATCH` helper (state advanced before the guard). The guard `NE raw, 0` behind the `LIB RANDOM` helper is the same shape (BUNDLE-GM noted "RANDOM has the same shape and the same latent issue; left alone"): its snapshot is the `CALL`, the generator has already advanced, and the interpreter re-executes the `CALL` and draws again, so every later draw (and sum) shifts. Which guard the N-th execution hits depends on how many guards one iteration holds: forwarding the upvalue loads removes guards (the `ULOAD` type guard and the `FLOAD` / `HREFK` / `HLOAD` chain after each draw are re-emitted without `uvroll`, folded with it), which moved the hit onto the `RANDOM` guard; with the loads left alone it moved elsewhere (hence "disappears with forwarding off", "needs the storm", "needs `randra`" = another trace numbering / allocation changing the guard order). The IR, the snapshots and the register allocation were right all along. Fix: `guard_label` also exempts the guards behind `LIB RANDOM` (mutant `rand_storm_guard`, detector `tr_core::random_storm_programs`, registry 529). Test-only machinery: in production a failing guard there means the helper changed nothing (`jit_random_range` returns `None` untouched), so default-config PROGRAMS were never affected; default-config storm TESTS were (the pass of `f2_programs` / `gm_programs`-style families depended on the storm period). The cut at `RANDOM` in `uvroll` stays as a conservative rule (the same run passes without it with the fix: `f2_programs` under `TR_CORE_UVSEP=3` and `uvroll_state_libs`), so `uvroll` can be enabled without a soundness caveat from this item; the `lua_lexer` cycles remain the only blocker. Old notes follow.
- **(historic) `uvroll` past `LIB random` looked unsound.** `tr_core` with `TR_CORE_UVSEP=3` failed `f2_programs` (`f2_lib`, `on2 +exitstorm=7 +verify +randra=5`, also under `+emurun`, so not a CPU effect): one `math.random(-3, 3 + i)` sum was off by 10. The IR of the trace (4 draws, the `math` table forwarded from the first `ULOAD tab` over the three later ones) and its snapshots look right, `+verify` replays it correctly, with the upvalue load forwarding switched off (only that) the output is right, and it needs the exit storm and the random register allocation together (every other option combination is right). The `LIB random` recording has the "same exit-storm shape as GMATCH" (a state-changing helper followed by an exit-capable guard) noted since BUNDLE-GM; `uvroll` now gives up at `SETMT` / `RANDOM` / `GMATCH` kinds, and the whole `tr_core` passes with both bits. Whether the miscompile is in the register allocator (a table value kept live across four `CallH` sites with `post:` reloads) or in the exit storm's treatment of the guard is not known; a peeled loop in a process (`hdrlib=1`, RANDOM header-safe) would hoist the same kind of upvalue load over the same helper, so it is worth a standalone reproduction before `uvroll` is turned on.
- **`UVREL` + `ULOAD` reload `uv->v`.** Each distinct `(closure, index)` access is `ld uv; ld v; lea frame; sub; cmp; jae` then `ld v; cmp tag; jne; ld val`: the second `ld v` could reuse the register of the first. Instruction share in `iter_pipeline` before this bundle: 27 % upvalue traffic of which this is a small part; not measured separately. The range guard itself is hoisted by the loop optimiser as soon as the cell is.
- **Side traces and `uvsep`.** Guards are emitted only where the loop optimiser's `find_sep` runs (peeled, rolled-with-forwarding, side traces via `run_fwd`); the recorder never forwards. A side trace entered from a parent does not see the parent's upvalue facts (no cross-trace forwarding).
- **`queens` / the 4 % estimate.** `awfy/queens` has no `ULOAD` / `USTORE` in its traces; the Opus study's "upvalue" share for it was the closure `GETUPVAL`s of the interpreter. Nothing to do in the trace compiler.

### C44: r15-addressed poll (`pollr15`) costs `programs/fib` 7-9 % cycles (P2 loop-top placement fix, 2026-11-10): OPEN, knob default off

- **What is known.** `cmp byte [r15+disp32],0` is slower than `movabs rax,&poll; cmp byte [rax],0` in the recursion loop of `fib` at every head offset (1.49-1.61 G vs 1.44-1.46 G cycles); `movzx` + `cmp`, `lea` + indirect `cmp` and a shifted `r15` block are as slow, so it is the r15-derived address, not the encoding, the instruction count, 4K aliasing or the poll word. Backend stalls rise (3.69 G vs 2.55 G dispatch slots); samples sit on the loads that wait for call-frame stores. Not diagnosed further (no counter for store-forwarding / memory-dependence prediction was found on Zen 5 here).
- **Ideas not tried.** A poll word reachable `rip`-relative (code arena mapped near the static, or a mirrored poll byte in the arena written by `request_interrupt` / `set_hook`); a per-state poll byte at a small displacement from `r15` set by those writers (they are process-wide today); `budget` check ordered before the poll.
- **Cost of leaving it off.** all 223 programs 0.989 / 0.983 (cycles / instructions) instead of 0.985 / 0.971 against the all-off code, mostly in `jit-trace` (0.973 / 0.930 vs 0.962 / 0.901); `RUSTYNAIL_JIT_TRACE_LIMITS=pollr15=1` turns it on.
- **Layout lottery, again.** The head offset is global; per-program slow zones differ (`type_entry` at 32, `sink_vec` at 8 / 40, `field_global` at 0 / 8, `inline_leaf` with the movabs poll at 32 / 40). 48 avoids the known ones; a per-trace rule needs a model of the zone (none found: the body of `tonumber_num` is 56 bytes and is fast at 0, 32, 48 and slow at 16, 24).


### C45: R1 `rearm` (rearmed sampling tick after a hand-off to the interpreter, 2026-11-10): BUILT, SHIPPED (`rearm=4`); parts not built
- **Update 2026-11-10 (C45 study + implementation).** The formation-cliff risk of the 1024 gate is answered by a policy, not by a derived constant: young heads are entered only through the clean-family rule (`rearmside=2`, `rearmclean=1`; docs/JIT_TRACE_REF.md "BUNDLE-C45", docs/PERF.md "BUNDLE-C45"). The gate stays 1024 for every family that is not clean; the underlying cause (permanent decisions made on small, exit-clocked samples) is still not fixed.
- **What was built.** docs/JIT_TRACE_REF.md "R1", numbers docs/PERF.md "R1 rearm": class_middleclass 0.75 cycles / 0.79 instructions, classic_vector 0.89 / 0.94, all 223 programs 0.996 / 0.999.
- **Hand-off arming only in `rt::enter`, not in `tr/exit.rs`.** The result handling of `enter` sees every way out of compiled code, so no hook in the frameful exit restore was needed; the cost is that the stay's entry frame (not the frame the exit restored) decides whether to arm. A stay that was entered at a callee and ended by returning into the interpreter's caller frame arms nothing (its caller's loops wait for the normal tick), which is also what keeps `iter_pipeline` / `heapsort` stable (see below); it leaves some of the class_middleclass gap: the study's THRESHOLD=,30 proxy reached 0.645 instructions, this is 0.786.
- **Young traces are never entered by a rearmed tick (`REARM_MIN_ENTRIES` = 1024).** Entering a trace through a rearmed tick makes its exits hot earlier than the base would; `iter_pipeline` (15x) and `heapsort` (1.76x) have a formation cliff that this triggered (docs/PERF.md). The base is stable under tick-period changes; the cliff with the gate is chaotic (min 128 fails iter_pipeline, 512 / 1024 / 4096 pass), so 1024 is not a derived safety margin. A proper fix would be to find out why those two programs have a cliff at all (the `InnerLoop` abort of a recording at a `reduce` loop whose iterator chain contains a loop, followed by the `TooLong` blacklisting of the function-entry head; heapsort's `sift` trace never forming when its caller loop is entered by a trace early): **open, base-visible risk** for any other change that moves recording times.
- **Not built.** (1) R2 (inline `LIB CCALLS` helper entry in asm, 5-7 % of class_middleclass: `jit_tr_alloc` 8.7 % of instructions per trace entry). (2) R3 `sbump` for the `Recursion` / `Depth` side aborts of `isSubclassOf` (<= 5 %). (3) R4 per-class loop copies for polymorphic receivers (15-25 %, big). (4) Re-entry of **tier-1-only** loops (no `Traced` head in the prototype): a rearmed tick would need the stay monitor's blessing (`mon.blocked`) and a bounce guard on tier-1 stays; the tier-1 loop re-enters only at the normal tick. (5) Entering the caller's loop after a stay that popped its entry frame (see above): would need the maturity test of the caller's trace instead of the entry-frame rule. (6) Arming across a C boundary (`pcall`, a sort comparator, a coroutine resumed by compiled code): the arming stops at the first non-Lua frame. (7) The span is counted in back-edges of the armed prototype's loops with callee loops at 1/32; a loop-free interpreted stretch (long straight-line callee) keeps the arming alive until the next loop edge of any kind.
- **Known cost.** Every interpreter back-edge during an armed span is a cold call into `jit_hot_loop` (`rearm_loop`: a proto lookup and a head lookup); a program with many exits and long interpreted callee loops pays up to 512 such calls (`rearm=4`: 128) per arming (event_promise: 512 misses for one arming, below noise).

### C46: inline short-string compare in `Heap::jit_str_probe` (HDRSUB, 2026-11-10): BUILT, MEASURED, REVERTED
- **Tried.** `bcmp` of the chain-walk hit test replaced by overlapping unaligned 8 / 4-byte loads for length <= 40 (`#[inline(always)]` helper).
- **Evidence.** 223 programs, 5 runs, equal-length binary dirs: instructions 0.9985 overall (0.959 `lib/sub`), cycles 1.0013 (jit_str_bytes only) / 1.0037 (also jit_str_probe) against a noise column 0.998. Targets at 15 runs: `sub_1char` 0.948, `knucleotide` 0.950, `lib/sub` 0.985. Losers at 15 runs with equal instruction counts: `lib/find_plain` 1.047, `jit-trace/alloc_ctor` 1.057 / 1.035, `lib/pcall_err` 1.075 (one variant), `havlak` 1.029.
- **Why rejected.** Breaks the no-program-over-3 % rule; the losers do not use the probe (code-layout shift of the binary, the same family as the old LTO inlining effect on `sub_1char` / `knucleotide`).
- **Retest trigger.** A layout-stable build (PGO / pinned function alignment) or a knob-able call site (a `TrLimits` flag read by `short_core`).
- **Cheap retest.** YES (about 30 lines: an `eq_short(a, b)` comparing 8 bytes at a time with an overlapping tail, used in the chain walk).
- **Sources.** docs/PERF.md "HDRSUB", docs/bench-hdrsub.txt.

### C47: BUNDLE-A leftovers (2026-11-10)
- **Built.** `libnum` (`tonumber(string)` via `STRTO` tonumber mode), `libnext` (direct `next(t [, k])` through `TNEXT`), `libfind` (plain `string.find`, `LIB FIND` / `FINDS`), all on by default (docs/JIT_TRACE_REF.md "BUNDLE-A", docs/PERF.md "BUNDLE-A").
- **Not built.** (a) `tonumber(s)` with a non-numeral (result nil) is `Nyi` at recording time: a loop that alternates numerals and non-numerals runs the numeral side on traces and leaves through `NE 0` on the other (the side recording of that path aborts `Nyi(Call)`). (b) `string.find` with a real pattern (`find_cap`, `find_pat`): needs the matcher on trace = the string-runtime bundle (S1-S3). (c) `find` / `next` / `tonumber` as `return f(...)` tail calls: the tail path is the old one for `find` (it is not a `libfn` slot). (d) Lean entries for the `STRTO` / `FIND` helpers (not measured). (e) `next(t, k)` on 5.4 (`rustynail_lua54`): disabled, the interpreter's `alimit` touches are not modelled.
- **Mutant-runner notes.** `tonum_mode_ignored` is only visible through trace statistics (the interpreter re-runs the call): detected by `tr_core::ba_num_does_not_exit_on_numerals`. `next_ctl_nil` makes the traversal loop forever: its detector (`ba_next_matrix`) takes the full child timeout.
- **Sources.** docs/PERF.md "BUNDLE-A", docs/bench-bundle-a.txt.

### C48: BUNDLE-B leftovers (string runtime, 2026-11-10)
- **Built.** `fmtc` (S2, process default ON) and `pat1` (S1, knob, process default OFF), `MaybeUninit` captures (always on). See docs/PERF.md "BUNDLE-B".
- **S1 off by default (until S3).** wordfreq 0.914 (< 10 % rule), json_roundtrip 1.050, find_cap 1.042: the per-call analysis (`MatchState::analyze`, ~35 instr per `find` / `gmatch` step; `gmatch_aux` rebuilds the `MatchState` every step because the state lives in 4 upvalues) cost more than the scan saved for non-one-item patterns. **Resolved by S3 below: `pat1` takes its plan from the cached program and ships ON together with `patc` (process default 7).**
- **S3 `patc` BUILT (2026-11-11), process default ON** (`crates/rn-std/src/pattern/patc.rs`, docs/PERF.md "BUNDLE-S3", docs/bench-bundle-s3.txt). ALL 223 programs 0.979 cycles / 0.974 instructions vs all-off (0.997 / 0.994 for the previous default), real_dis 0.920 (previous default 0.950), wordfreq 0.878, template_engine 0.858, markdown_conv 0.889, `lib/match` 0.204. Design as in the notes (address hint, bytes verified, per-pattern bitmaps, literal runs, `.*` run = rest of subject, same recursion structure at quantifier boundaries, lazy malformed-pattern error items, no per-call classification); deviations: the slots ARE the programs (inline fixed capacity: 32 items / 8 sets / 48 literal bytes / 48-byte key; larger patterns and patterns with more items stay on the general matcher), the matcher takes its depth as an argument, a start filter that looks through up to 8 leading `(`, straight-line programs without recursion, pinning (`PinGuard`) for states that outlive foreign code (gsub callbacks, `GMatch`).
  - **Open.** (a) The probe (~35 instructions per call) is a loss for one-shot `find` calls with a trivial match (`lib/find_cap` 1.029, `gsub_str` 1.017, `find_pat` 1.005 at 15 runs; all under the 3 % rule but they swing between +1 % and +10 % across builds of the same source: C46 layout class). Ideas not built: a per-call-site hint (the recorder knows the pattern constant: record `FIND` / `MATCH` with the program), a one-entry front cache keyed by the string object. (b) A subject-length threshold and a "first attempt on the general matcher" variant were measured and rejected (they lose the real_dis gain). (c) S5 (run scans on the bitmap with wide loads) is not built (no SIMD by decision; a scalar 8-byte-at-a-time table walk was not tried). (d) Programs with more than 32 items still use the general matcher; raising the capacity costs 650 B per slot (166 KB of thread-local storage at 256 slots).
- **S4 `copy` NOT BUILT** (inline small copies / compares in `intern_short`, `create_inline`, `extend_from_slice`): C46 layout losers; the SIMD study measured it neutral to worse. `tconcat` raw-read inlining and stopping the `gmatch_aux` `start_filter` recompute not done either.
- **Sources.** docs/PERF.md "BUNDLE-B", docs/bench-bundle-b.txt.

### C49: BUNDLE-C: `t1ccall` and `tfmt` built, both OFF (2026-11-11)

*(Update 2026-11-10: `tfmt` ships ON since N-B; the RETF bit of C38 ships ON since the RETF flip, `RETF_PROCESS = 3`, see C38 slice 2 and docs/PERF.md "RETF flip". The `tfmt` / mips retest note below stays valid: with RETF on, real_dis is within noise of `retf=2`.)*
- **Built.** `t1ccall` (lean tier-1 call of light C functions, `jit_ccall`) and `tfmt` (`string.format` of constant `%d %x %X %s %c` formats with widths, one to five arguments, as `LIB FMT` / `FMTX`), each with its own `TrLimits` knob, process default 0 (docs/JIT_TRACE_REF.md "BUNDLE-C", docs/PERF.md "BUNDLE-C", docs/bench-bundle-c.txt).
- **`t1ccall` result.** No instruction gain (the helper is about 60 instructions against 96; total instructions +0.2 %), cycles flat on every target, layout losers at 15 runs with equal instructions (`lib/sel_loop` 1.144, `lib/m_abs` 1.130, `lib/m_sqrt` 1.093, `corpus/parser_combinator` 1.092, `corpus2/dev/regex_nfa` 1.084). Stop rule: skipped. Retest trigger: a layout-stable build, or a template change that removes work (recording the call instead). Cheap retest: YES (flip `T1CCALL_PROCESS`).
- **`tfmt` result.** `lib/fmt_d` 0.895 cycles, `lib/fmt_x_pad` 0.913, `real_dis_x86` 0.963; but `real_dis_arm` 1.008, `ppc` 1.009, `mips` **1.042 (instructions 1.022, 15 runs)**: the recorded `format` removes the stitch that ended the main loop's exit-6 side trace, the recording now runs to the end of the loop body and aborts `LeaveLoop` (C38), so that path stays in tier 1. real_dis geomean 1.005 cycles / 0.992 instructions. The rule "no program > 3 % worse" is not met. Retest trigger: C38 (loop-leaving side traces). Cheap retest: YES (`TFMT_PROCESS = 1`, rerun `scripts/bench_session.py` with the columns `tfmt=0` / `tfmt=1`). **RETESTED and SHIPPED in BUNDLE N-B (N5, 2026-11-10): `TFMT_PROCESS = 1`.** With `sidenest = 1` / `retf = 2` on: real_dis_mips 0.999 cycles / 0.993 instructions (15 runs), real_dis_arm 0.981, x86 0.938, ppc 0.962, lib/fmt_d 0.888, fmt_x_pad 0.888, the unsupported formats 0.988-1.001; suite geomean 0.9987 / 0.9986; jit-diff `cc` 1..80 0 diffs; the mechanism behind the old mips loss was not re-diagnosed.
- **Not built.** `%f` / `%g` / `%q` / precision items (the engine is unchanged, only the shape check and the `Value` kinds would grow; `fmt_f` / `fmt_g` are 0.99-1.02 stitched), 6+ arguments, lean entries for the `jit_tr_alloc` frame of `STRTO` / `FIND` / `FMT` (C47 (d); not measured), `%d` of floats with an integral value.
- **Sources.** docs/PERF.md "BUNDLE-C", docs/bench-bundle-c.txt.


### C50: BUNDLE-D: `armm` and `sbump2` built, both OFF (2026-11-11)

`armm` (tier-1 continuation after an arithmetic metamethod) works and is count-exact, but the tier-1 stay it creates keeps hot loops out of traces (`programs/metamethods` 2.24x) and the usual `return Vec(...)` metamethod is a `TAILCALL` that still leaves tier 1 inside a `CIST_MMRET` frame. Next attempt: (a) only take the arm when the enclosing loop has no trace / is not about to get one (a counter-driven choice, deterministic), (b) tail calls (Lua closure and C function targets) inside metamethod frames in tier 1, with the callee's `RETURN*` finishing the caller's instruction (the callee frame keeps `CIST_MMRET`; `Cold::MmRet` already handles `RETURN0/1`). Not done: `__len`, `__concat`, `__pow` (own templates), tier-1 `__index`/`__newindex` functions.
`sbump2` does not touch the `class_middleclass` gap (the `isSubclassOf` stays are interpreter-side exits at its `SELF`, ~36 % of the program); an extra unroll per site is not what is missing. See docs/PERF.md "BUNDLE-D".

### C51: R4 polymorphic loop-invariant receivers (per-class loop copy / cheaper hop, 2026-11-11): NOT BUILT (stop rule, ceiling measured)

**Idea.** `class_middleclass` pair loop (`for j ... a:distance2(world[j]); a < world[j]`): the receiver `a` is invariant over the inner loop, the root trace is specialised on one class (the SELF metatable guard is hoisted into the pre-roll), so for the other classes every iteration runs the entry segment and pre-roll, fails the SELF guard (pc 193), takes the exit stub and a non-looping side trace, and links back to the root head. Designs: (a) a side trace from a pre-roll guard exit records the rotated iteration (head = its own start pc, reuse of the root `close` / peel with `known` types from the exit snapshot) and loops on itself; (b) patched direct jump exit -> side trace with inherited registers; (c) loop-peel specialisation by receiver class.

**Ceiling experiment (decides the stop rule; plain release `--features jit` build of 8d6a28f4, one binary, `RUSTYNAIL_SEED=0`, CPU 3, 3 runs each, instructions equal across runs).** The upper bound of (a) is the program rewritten by hand so that each class has its own copy of the pair loop (`local c = a.class; if c == Dog then <loop> elseif c == Bird then <loop> elseif c == Parrot then <loop> else <loop> end`, four textual copies = four root traces, each monomorphic and loop-invariant, no hop at all, no compile-time or code-size limit):

| program | cycles | instructions | exits (EXITDUMP total) |
|---|---|---|---|
| `corpus/class_middleclass` | 710-713 M | 3.383 G | 12.56 M (SELF pc 193 exits: 2.55 M + 1.57 M + 1.04 M; pc 167: 0.74 M) |
| hand-specialised pair loop | 714-715 M | 3.090 G (0.913) | 5.43 M (all SELF hops gone) |

The hops are removed (-57 % exits, -8.7 % instructions) but **cycles do not move (1.003)**: the hop is a well-predicted, high-IPC sequence (base IPC 4.76, specialised 4.33); the cycles sit in the interpreter stays after exits from inlined method frames (`pc 1 Getfield`, `Lei` in the `isSubclassOf`/`__lt` chains, see C50) that both versions keep. Design (a) cannot beat this ceiling, (b) only removes part of the same instructions. Stop rule of the task (< 2 % cycles on `class_middleclass` and < 1 % overall): nothing built, no knob, no code change.

**`classic_vector` is not loop-invariant.** The spring loop (`for _, s in ipairs(springs) do s:apply() end`) takes its Particle / HeavyParticle receivers from the spring table: the class changes per iteration (by position), not per loop. Per-class loop copies do not apply there (the entity loop of `class_middleclass`, `world[i]:update`, is the same), only a cheaper per-iteration hop (b) would, and the ceiling above (hops 57 % fewer, cycles 1.003) says that hop is not where the cycles are.

**Retest trigger.** The `Getfield` / `Lei` exits at the first guard of inlined method frames (frame-leaving side traces, C38 slice 2) are the remaining cost; revisit per-class loops only after those are traced (then the hop instructions may matter). Cheap retest: YES, the hand-specialised program is generated by replacing the pair-loop body in `bench/corpus/class_middleclass.lua` with the four-way `if` above.


### C52: BUNDLE-F leftovers (2026-11-12)

- **`lands` (F3) NOT BUILT.** The unchecked tier-1 landing needs both addresses in the exit metadata and a proof, per exit, that the stub writes every slot that tier 1 types at
  that pc with exactly that type. `lower.rs` knows the typed slots only for mid-block pcs (`land_checks`, `ty_at` is recorded only for `record_ty && hook`), the snapshot elides
  slots it does not change (the trace may know their type from an `SLOAD` check, the snapshot does not say so), and the landing address is baked into more places than the
  exit stubs: the exit cells (`c != meta.snaps[..].land_addr`), `flush_keeps_cells`, `verify_landings`, the per-version rewrite of `pcs` (TR19b(c)) which would have to patch two
  addresses per pc across retired code versions. Lean re-entry (skipping `entry_ok`/`pending` bookkeeping and the `SLOAD` tag checks of a stitched root) would change the
  entry-failure statistics that `ncallee` / C36 read. ~15 + ~25 instructions per stitch (of ~377) are the bound; with F2's lesson (a few instructions on a store-forwarding
  limited path do not show in cycles, C44) the expected gain is below the 1 % noise of the suite. Retest trigger: a program whose profile is dominated by stitch exits (the
  `real_dis_*` stitch exits are ~2 % of cycles).
- **F2 pieces not built** (see docs/PERF.md BUNDLE-F): fused CIST test (saves <= 1 instruction), `n < 2` re-check (cross-trace fact), upvalue chain hoist (per frame, no loop to hoist from).
  The only sound piece built (`fibdiet` bit 0) is -0.3 % cycles on `fib`: OFF. The remaining fib gap to LuaJIT (270 vs 64 instructions per node+leaf) is the `CallInfo` protocol
  itself (17 stores/loads of a push, 13 of a return): closing it needs a different frame representation for traces (LuaJIT's up-recursion trace has no `CallInfo`), not a diet.
- **Process default sanity:** `stitch0` is ON only through `STITCH0_PROCESS`; K3a: the struct default is `STITCH0_PROCESS` as well.

### C53: BUNDLE-G leftovers (2026-11-12)

- **Hot callees beyond the chain cap** (`chaindepth`): a skewed dispatch with more than 4 links in front of its hot callees whose first 4 links serve under 30 % is cut before it records the hot ones, and `chain_check` cannot
  re-order a chain that does not hold them. Adding the calls that leave the last link as a share of `chain_due` thrashes on uniform dispatch (the share is a mixture, sorting it first fakes a gain). A proper fix needs the callee identity of
  the leaving calls (a small per-chain histogram in the terminal exit stub) to decide whether one callee dominates; not built, no program of the 223 has the shape (`forth_vm` has 8 links).
- **Real give-up** (hash / jump-table dispatch for K >= 16): not built. Tier 1 alone is better than the capped chain for `fdisp rand` K >= 16 (1016 vs 1252) and much worse for `mdisp` (2535 vs 1422), so a loop-level give-up would be wrong
  for method dispatch; an indirect-jump dispatch on the callee's prototype identity would beat both (LuaJIT's polymorphic call sites stay at ~1.3 Gcyc for K=16 too).
- **`load` chunkname with an embedded NUL** keeps the whole name, the reference cuts at the NUL (`[string "x"]` vs ours `[string "x`): found by `err_16_where_prefix.lua`, left alone (pre-existing, nothing depends on it).
- **`lib/find_pat` +5 % cycles** with the `pcallerr` binary at equal instructions (pattern-matcher inlining / alignment); one more entry of the layout lottery (P-L, C46).

## C50: BUNDLE-H (2026-11-13)
- **Shipped.** `libcall=7` (docs/PERF.md "BUNDLE-H", docs/JIT_TRACE_REF.md "BUNDLE-H"): all 223 programs 0.9836 cycles / 0.9839 instructions, none above 1.001 in instructions, gate passed.
- **`libcall` bit 3 (`string.rep`) built, OFF.** `lib/rep` +18 % instructions (the benchmark's result lengths straddle the 40-byte short-string limit, so the result-class guard presumably exits; not verified with exit counts). Retest trigger: a class-polymorphic string result in the IR (or per-site result-class widening: record the long class when both were seen). Cheap retest: YES (`libcall=15`, or `LIBCALL_PROCESS = 15`).
- **Not built.** Inline short-string `<` / `<=` (needs a byte-swap micro-op in `ir::Op`, the x86-64 encoder and the emulator; the lean entry gives -30 of the -95 instr/iter target on `str_lt_short`), inline `math.random(m, n)` generator step (lean entry gives -58 of -150..-220; the xoshiro step is 12 instructions but the `project` loop and the closure-upvalue load/store need registers the class-A frame does not have), lean entries for `FIND` / `GMATCH` / `FMT` / `STRTO` / `TOSTR` / `TYPENAME`, `string.char` with 4+ arguments, `random(0)`, `tostring`/`lower` of numbers.
- **`xregs=3` (H3) re-measured, still OFF.** 223 programs 0.9820 / 0.9847, but `heapsort` +5.7 %, `rec_loop` +6.0 % (15 runs, fewer instructions: micro-architectural), `cd` +1.2 % instructions; `xregs=1`: `rec_loop` +5.9 % only; `xregs=2`: `richards` +3.5 % / +1.5 % instructions. Details in docs/PERF.md "BUNDLE-H". Retest trigger: diagnose `heapsort` / `rec_loop` (perf annotate). Cheap retest: YES (`XREGS_PROCESS = 3`).
- **H2 inline `TNEXT` step NOT built (C50).** `tnextinl` bit 0 (header-safe `TNEXT`) shipped (it_pairs 156 -> 134.5 instr/element, it_next_empty 177 -> 139; targets ~40 / ~45 not met). The inline scan (array and hash part from `next_hint`, empty-table fast path, helper as the mismatch fallback) is not
  built: the `TNEXT` result is a pseudo node in the exit area that the following `HLOAD`s read (`class_a` + `hload` with the "pseudo key node" variant, asm.rs `TNEXT` / bit 1 of `HLOAD`), so an inline form needs a new result convention (key tag / key payload / value in registers) through `ra.rs`, the lockstep (`pending_next`) and the sink/escape passes.
  Retest trigger: this design. Cheap retest: NO (1-2 bundles).


## C54: BUNDLE-J leftovers (2026-11-09)
- **C40 `centry` is SHIPPED (process default 2)** with the weight rule `centrymin=60`; the row in the table above is the 2026-11-08 attempt (it never fired: `CIST_FRESH` is not yet set when the entry counter fires). See docs/PERF.md "BUNDLE-J". Cheap retest of the old behaviour: `centry=0`.
- **Lean entry from C not built.** About 280 instructions per call (`hot_call` through the vtable, `rt::enter` with the monitor / stats bookkeeping, thunk, tier-1 prologue) separate a traced comparator from the interpreter, so roots below 60 IR (`a > b`, 6 IR; `pcall`'d functions, gsub callbacks) stay in the interpreter. A direct
  trace-entry path for a `Traced` function head (skip the monitor, the stay accounting and the thunk's register loads) and a native `RETURN` of the fresh frame in the trace would lower the threshold; the sort_* / tsort_* programs would gain. Retest trigger: that path; `centrymin=0` shows the cost today (tsort_cmp +68 % instructions).
- **oop_mixins stops at 0.74-0.75 cycles** (target 0.55-0.7): exits inside inlined `__lt` frames cannot get side traces on function roots (783 K handler calls), so the receiver classes that differ from the recorded one pay a handler call each. Retest trigger: side traces for exits inside inlined frames of function roots (`framesides`, C38 slice 2).
- **Registration only before the first compile.** A function first hot through a Lua caller (flat call, `NoJit` parked) and called from C later is never a C-entered root. Rare for comparators; a recompile on first C call would fix it.
- **msgpack_pure +4.1 % instructions** with `centry` (formation shift, see PERF.md); `centry=1` is neutral on it and loses the oop_mixins win.
- **Hash-D (J2) leftovers.** (a) memo_cache +8..13 % cycles (lookup-miss-then-insert with promoted keys; `rehash` growth / `remove_short` recompute `fast_hash` of promoted strings; JIT `HREFV` lookups of an unpromoted key still walk a chain): ideas: an `extra == 0` shortcut in the `HREFV` / tier-1
  `SHORT_HASH` lookups (+2 instructions per dynamic-key lookup), keeping 8 bits of the strtab hash in a header spare bit field for small string tables. Cheap retest of laziness: `cargo build --features rn-core/eager_hash`. (b) The 5.4 build (`--cfg rustynail_lua54`) was not run. (c) Fuzz targets not run (not required); `string_lib` and `table_ops` would exercise it.
  (d) `lib/char` +1.9..2.2 % cycles, `programs/string_ops` +3.4 % at 5 runs (+0.1 % at 15), `str_idx_concat_key` +1.8..3.4 %: small cycle losers of the study, instructions fewer.

- **C54 follow-up.** (a) memo_cache stays +6 % cycles (15 runs): the dual hash recomputes `fast_hash` from the bytes of promoted strings in `rehash` growth and `remove_short`; idea: stash the strtab bucket bits in spare header bits (8 bits of `extra` / `shrlen`, or the 16 free high bits of `hnext`).
  The `HREFV` miss check on `extra == 0` is built and was removed again (+3.1 % instructions on `lib/string_key_dyn`, no gain on memo_cache); the tier-1 templates keep theirs. (b) `centrywait` (default 8) delays the first entry of a C-entered function: oop_mixins 0.74 -> 0.82 cycles for msgpack_pure neutral.
  Rejected: delaying the registration of small functions (pcall_error +26 %), a head recording wait (no effect). (c) Layout-lottery programs of the final tree: `tremove_front`, `tonumber_flt`, `load_small`, `lib/match` (equal instructions, `dup` 1.000).

## C55: BUNDLE-K (2026-11-09): L1 `lazyci` built, OFF; L2 and L3 not built
- **L1 lazy CallInfo is complete, tested and OFF** (`RUSTYNAIL_JIT_TRACE_LIMITS=lazyci=1`). Design: docs/JIT_TRACE_REF.md "BUNDLE-K L1"; numbers: docs/PERF.md "BUNDLE-K". It does not pass its gate: fib 0.92 cycles in the 223-program run (0.93-0.95 pinned A/B, instructions 0.89; gate <= 0.88), and
  `binary_trees` is +6 % cycles / +3 % instructions in A/B (+12 % in the suite before the in-line new-frame completion). The cost it moved: the instructions saved at the push (about 14) are partly paid back by the root-frame return test (guard and tail, 4 instructions per return) and by the leave at every exit
  with lazy frames (binary_trees: `rec_fail` on a missing spare `CallInfo` after every collector cycle that shrank the list, 2.8 M leaves). Cheap retest: change the process default `LAZYCI_PROCESS` in `tr/cache.rs`; everything is covered by `lazyci=1` in the tests.
- **What would close the gap** (each measured or counted in the dump, none built): (a) the callee-identity guard of the entry trace (`SLOAD slot65535; UREF; UVREL; UGE; ULOAD; EQ`, 16 instructions of 49 on the down half, per call) is redundant after a native self-push (the callee *is* the running closure, nothing in the region stores an upvalue):
  a second loop label past it (the L2 plan), needs the guard sequence at the front of the iteration in the IR (it is interleaved with the `ADD` of the argument today) and `V0 = V2 = callee` re-established by the push; saves about 13 % of the instructions of fib; (b) the root-frame return test: 4 instructions per
  return; an exit-and-side-trace formulation (root returns exit, a root-return side trace with the eager guards takes them) needs a side trace that starts at a continuation head, which `no_start_side` forbids for side traces; (c) the double upvalue load (`UREF` and `ULOAD` both walk `closure -> upvals -> uv.v`) and the 3-op `UVREL`
  (`lea; sub; mov`; needs a `V` that names the frame register); (d) the exit counters of an exit that already has a side trace (4 instructions on the base-case exit of every leaf); (e) `rec_fail` after the collector shrinks the `CallInfo` list (`luaE_shrinkCI`): a spare-node reservation in the trace entry instead of a fall to tier 1.
- **L2 (callee-identity hoist) and L3 (transition diet) were not built**: L2 depends on the layout change in (a) and is only worth it on top of an L1 that is ON; the L3 candidates are (b), (d) and the first-iteration flag store on every push. Retest trigger: L1 on by default.
- **Study numbers that held**: the unsound ceiling patch (fields not stored, no return guards) gave 0.855 here (0.87 in the study); the sound version reaches 0.93 because the ceiling skipped the root-return and leave bookkeeping.
- Not run: the 5.4 build (`--cfg rustynail_lua54`), the ASan scripts, the official-suite GC stress runs with `lazyci=1`, fuzz targets with the knob on.

## C56: BUNDLE-L (2026-11-09): array loops. A `aidx` and B `bhoist` built, OFF; C, D, E+H not built

* **A `aidx` (OFF)**: complete and tested, no gain while the inner loops have no free register (matmul instructions 1.000, cycles 1.053; spectral_norm +2.2 % instructions; docs/PERF.md BUNDLE-L). Cheap retest: `RUSTYNAIL_JIT_TRACE_LIMITS=aidx=1` after anything that frees registers in array loops (D below, `xregs=3` with a cheaper `r14` reload, the register file bundle).
  The expected value is the study's: -5..6 instructions per matmul iteration when `NEG k` and `-j` fit in registers.
* **B `bhoist` (OFF)**: all-223 cycles 0.988, instructions 0.9835 (matmul instructions 0.859, cycles 0.987; `lib/*` string-array loops 0.7-0.9); the stated gate (matmul <= 0.62 instructions / <= 0.93 cycles after A + C + D) cannot be met without them, matmul being cache-bound (3 lines per row). To switch it on: `BHOIST_PROCESS = 1` in `tr/cache.rs`
  (plus `tr_policy` process-default test rows if the owner wants them). Known costs: lib/type_call +3.6 % instructions (short loops pay the pre-roll guards per entry), lib/tsort_int +3.5 % (an extra trace), expr_compiler +1.3 %. Ideas not done: hoist the guards only for loops whose trip count the entry profile shows to be large; hoisting for `while i <= n` loops (compare against `n` instead of the
  count); hoisting the 2-D row bound (`b[k][j]`: needs the row sizes of all rows, not possible without a loop-invariant row).
* **C snapshot dead-slot pruning (not built)**: `snap::prune_dead` is the registered *bug* `snap_prune_dead` ("`debug.getlocal` reads temporaries", TR3). `tests/bl/bl_obs1.lua` shows the observer (an `xpcall` handler printing `debug.getlocal(3, i)` in a faulting loop sees the temporary slots of the loop's frame, bytecode registers above the active locals).
  A sound version must either prune only slots that the re-executed instruction overwrites before anything can run (destination registers of the exit pc that the instruction does not read, with errors counted as running code), or switch the pruning off when `debug.getlocal` / `lua_getlocal` has ever been called (a process flag in `jit_hooks`, set by `db_getlocal`, with `flush_all`) and accept the first observation after
  a pruned exit (not exact). The ranking numbers (study): spectral_norm -7 of 45 instructions per iteration, nbody -3..5, dot -2.
* **D single loop variable (not built)**: `count` of a `for` loop is `end - k` at every exit; the body needs one register less (this is what would let A pay). Needs a derived snapshot entry kind (`count = K - k`, `K = k0 + count0` computed in the pre-roll) in `exit.rs` stubs and the writeback, a `NE k, K` loop guard in place of `NE count, 0`
  (equivalent in wrapping arithmetic), and no PHI for `count`. `bhoist` already has `count0` and `k0` in the pre-roll.
* **E + H (not started)**: fold single-use loads into memory operands (`mulsd xmm, [mem]`), drop the tag store of an `ASTORE` whose element tag is known from an earlier checked `ALOAD`.

## C57: BUNDLE-M (2026-11-09): hygiene
- **C54 follow-up (a) is FIXED**: the strtab tag (docs/PERF.md "BUNDLE-M" M3) removes the `fast_hash` recompute from `rehash` growth and `remove_short`: memo_cache 0.975 cycles against the tree before the change (the `eager_hash` build is 0.991), table_hash 0.974; lib instructions +0.05 %, `lib/sub` +0.7 % instructions (+1.7 % cycles, deterministic, the chain-walk mask). Not done: tags wider than 19 bits (tables above 2^19 buckets recompute, `hashd_string_table_beyond_the_tag`); other targets than x86-64 (plain pointer, recompute).
- **C56 B `bhoist` is ON** (`BHOIST_PROCESS = 1`): see docs/PERF.md BUNDLE-M M2. The "known costs" of C56 (tsort_int, type_call, expr_compiler instructions) stay. `tr_core::type_stability` pins `bhoist=0` (pre-hoist dynamics), `type_stability_hoisted` restates the `type_late` expectation. Opt-out `bhoist=0`.
- **K4 (numeric tuning knobs to consts) was NOT done.** All of `maxside polyside polywait polyframes t3idx t1inline hoptop second sbump chaindepth fnpromo fnpromow innerretry domtop domn chainre chainn tset sinkroot deep pollr15 rearm` appear in the limit strings of the tests (maxside 88 lines, second 64, deep 124, rearm 36, polyside 21, chainre 19, fnpromo 19, polywait 20, polyframes 16, chaindepth 16, sbump 16, domtop 14, ... t3idx 10, hoptop 6 at the least), used as controls (head-count suites, the K3b rows, mutant detectors). Converting them to consts means rewriting those tests or deleting the controls; neither is behaviour-identical test hygiene, so the item was skipped as the task allows. Retest trigger: a bundle that rewrites the policy suites anyway.
- **Mutants**: `abc_with_store` removed (there is no ABC pass: `TraceOpts::abc` is parsed but unused, bounds-check de-duplication is the generic CSE guarded by `header_safe`, whose mutants `newref_header_safe`, `hdrlib_*` already cover a header-changing op between two checks); `vbytes_no_credit` and `vbytes_no_limit_guard` injected in `rt::recompile` (`t15_runtime::{a_failed_recompile_refunds_its_version_bytes, memory_limit_refuses_a_promotion_and_the_nest_retries_later}`); `rec_push_top_off` detector `tr_core::bm_rec_top` (harness native `__citop`). 5 more registry rows (`V54_ONLY`) have their injection site only under `--cfg rustynail_lua54` and are skipped on the 5.5 build; 4 (`V55_ONLY`) the other way round.


### C58: BUNDLE-P: `classic_vector` diagnosis; `rearmside` built, OFF (SHIPPED as `rearmside=2` + `rearmclean=1` by C45-impl, see below); `armm` and tail calls in metamethod frames NOT worth building (2026-11-14)

- **Diagnosis (current build, `RUSTYNAIL_JIT_VERBOSE=2`, per-10-step timing, an interpreter frame-start counter, `perf record`).** The two inner loops of the step loop are recorded early (traces at step 25 / 50: 3 traces, 14 side traces installed (17 recorded), 25 short function-head aborts `FnShort` / `StartRefused` that are expected for 3-bytecode metamethod bodies). Everything the task named is already on the traces: the arithmetic metamethods, `Object:__call(...)` (vararg, a `TAILCALL` of a table with `__call`: `tailx`), `setmetatable({}, self)` and the `return Vec(...)` tail calls are inlined frames. `armm=1` changes nothing (instructions 5152.30 M vs 5152.33 M; the tier-1 stays it enables are not where the time is) and `centry=1` neither (5152.42 M). The time is in the first 255 steps (0.38 ms per step against 0.10 ms afterwards, 18 % of the cycles of a 1000-step run are warm-up above steady state): the loops are not entered at all (`entered 0`) until the head's `entry_ok` reaches `REARM_MIN_ENTRIES` (1024) at step 255, because only the 1000-edge formation tick enters a young head, and the side-trace cascade (every polymorphic receiver class is one exit; depth 5) advances one level per tick. Hot-exit threshold 2 instead of 16 finishes the cascade at step 80 (0.69x cycles) but costs (global `hot_exit`, 223 programs, geomean cycles 1.021 at 4, 1.008 at 8; classic_vector 0.82 / 0.87) `heapsort` 1.57x, `regex_nfa` 1.44x, `awfy/queens` 1.33x at 8, and at 4 also `iter_pipeline` 16x, `expr_lang` 1.49x, `roguelike_turns` 1.44x; lowering `REARM_MIN_ENTRIES` alone does the same (min 256: `iter_pipeline` 10.6x; 512: nothing gained, classic_vector 0.99).
- **Built: `rearmside=k`** (docs/JIT_TRACE_REF.md "BUNDLE-P"): the rearmed tick also enters a young head whose family has `k` installed side traces. `k=3`: classic_vector 0.904-0.918 cycles / 0.915 instructions, all 223 programs 0.9984 cycles / 0.9994 instructions against the same binary without it. Gate misses, so the process default is **0 (OFF)**: `corpus/lua_lexer` +4.8 % instructions (+3-4 % cycles), `awfy/json` +1.3 % instructions / +3.4 % cycles, `awfy/queens` -3.8 % instructions / +3.2 % cycles. `k` = 1, 2 are better for classic_vector (0.87 / 0.88) and worse elsewhere (`heapsort` 1.53 and `iter_pipeline` 15 at 1; `awfy/queens` 1.37 at 1 and 2); 4: 0.946 (lexer +2.2 %), 6: 0.966. The three losers are formation shifts (a rearmed entry makes the exits of a young trace hot earlier, so side traces are recorded with less history; lua_lexer installs 51 side traces instead of 61): the C45 risk ("1024 is a margin, not derived") in its other direction. Not a soundness problem: entering a `Traced` head is what a formation tick does.
- **SHIPPED later (2026-11-10, BUNDLE-C45 in docs/PERF.md):** `rearmside=2` with the clean-family test `rearmclean=1` (a young head is entered only if its family has had no failed side recording and no member unrolled a callee loop): classic_vector 0.881 cycles / 0.885 instructions, all 223 0.9994 cycles / 0.9989 instructions, no program above +0.83 % instructions, heapsort / iter_pipeline / awfy/queens / awfy/json unchanged. The text below is the state before.
- **What would have made it ship.** (1) The C45 prerequisite: find out why `iter_pipeline` / `heapsort` / `awfy/queens` / `lua_lexer` change their trace sets with the time of the first entries (the `InnerLoop` abort of a function-head recording followed by `innerretry` waits of 16 k for `proto:95`, `iter_pipeline`; the `SideShort` / `LeaveFrame` aborts at `awfy/queens`), so that the entry gate can be lowered by a derived rule. (2) A cascade-aware alternative that does not change entry timing: record the side traces of a young family from the exits of the first entries (a hot count of 4 for exits of side traces that are themselves younger than 16 entries), measured with the same 223-program gate. Cheap retest: change `REARMSIDE_PROCESS` (one constant), the tests pass with the knob explicit.
- **`armm` / tail calls in metamethod frames (C50 follow-up (a), (b)): not built.** Both would change tier-1 behaviour only; classic_vector's metamethod frames run in traces, `programs/metamethods` is the program that loses (2.24x) when tier 1 keeps the loop. Next trigger: a program whose traces cannot form around arithmetic metamethods (none in the 223). **SUPERSEDED (2026-11-10, BUNDLE-V, C62): both built and shipped (`t1q` bits 4 / 5 / 6); `classic_vector` 0.67 (the warm-up and the interpreter stays behind the arithmetic handlers, not the traces, were its cost: `armm` alone did not move it, with tail calls and vararg returns in metamethod frames it did), `programs/metamethods` unchanged: the 2.24x was the ordinary resume entries, not the continuation.**
- **Allocation cost is the floor.** In the 0.10 ms steady state `newkey` / `resize_hash` / `newcheckedkey` / `newref_shortstr` / `alloc_block` are 40-45 % of the instructions (two-field objects `self.x = x; self.y = y` on fresh tables); site presizing is excluded (P1 study: pairs order and `#t` change), so the remaining classic_vector gap to LuaJIT (allocation sinking of the `Vec` temporaries) needs sink support for escaping-through-`__call` objects, not policy.

### C59: BUNDLE-Q (2026-11-10): `t1q` bit 0 (tier-1 `==` of objects) SHIPPED, bit 1 (float `%`) built, OFF

- **Shipped: bit 0** (docs/JIT_TRACE_REF.md "BUNDLE-Q"): gate set cycles 0.9864 / instructions 0.9898, all-223 0.9971 / 0.9984; astar 0.959 (15 runs 0.955), lru_cache 0.925, scheme_interp 0.946, containers_lru_heap 0.928, testrunner 0.951, class_middleclass 0.970, inspect 0.977. The formation lottery named in the old `jit_cmp` comment is real but small here: `awfy/deltablue` +1.5 % cycles (15 runs) / +2.2 % instructions, `awfy/json` +1.1 % cycles (15 runs) at equal instructions; richards and every other awfy program unchanged. Outside those, no program moves by more than its noise except the layout cases listed in docs/PERF.md (they reproduce with `t1q=0` in the same binary).
- **BUNDLE-V diagnosis (2026-11-10, docs/PERF.md "BUNDLE-V" V3): the +280 M instructions are the missing rearm entries, not the helper.** With `t1q=127` the 209 k `MODK` exits (2599 cycles per stay, 543 Mcyc) disappear and with them the rearm ticks they armed (`armed 124686 entered 105` -> `armed 131 entered 29`); trace exits 9.58 M -> 14.82 M, entries 3.49 M -> 3.81 M, instructions 5027 -> 5338 M, cycles 1657 -> 1814 M. Retest idea: tick the R1 rearm counter from the tier-1 helper path (or the inline integral-float `%` fast path with an exit for the rest) and re-measure `t1q=127`; bit stays OFF.
- **Built, OFF: bit 1 (`jit_fmod`).** Correct (the `bq_mod` matrix equals the reference in every mode; six mutants), but astar gets **+6 % cycles and instructions** with it (`t1q=2` alone 1.062 / 1.062; with bit 0 as well 1.013 / 1.020 against 0.953 / 0.971 with bit 0 alone), and no other program gains (all-223 `t1q=2` / `t1q=0` = 1.0016 / 1.0002). Only 168k `MODK` exits happen in astar, so the +280 M instructions are not the helper. In the verbose summary the function that holds the `MODK` (line 22) is no longer entered from the interpreter (131k entries to 169): its call now stays inside tier 1, main's compiled share rises, but the interpreter's remaining part of the loop (after the `Eq` exit at main's pc 29, 3000+ cycles per stay) is the same, and the total goes up; the cause is a different execution pattern around the frame that the exit used to hand to the interpreter, not found. The census's "MODK exits" (astar `index % 2`, sha2 `l % 256`) were exits worth ~170k, not a cost centre; sha2's `l % 256` is not on a float path in this build (instructions identical). To revisit: find why a tier-1-resident callee makes the caller's remaining interpreter stay more expensive (an `interp_entry` re-entry cost?), and re-measure with bit 0 on. An inline integer-valued fast path for `%` (no helper call) is the other open option; not built.
- **Not done:** `EQ` of full userdata with a C `__eq` (the C function leaves to the interpreter, as before; the helper path is exercised by io files without `__eq` only, no C test module for the new bit), the 5.4 build (`--cfg rustynail_lua54`), ASan, fuzz targets, PGO benchmarks, the full 563-mutant run.


### C60: BUNDLE-S (2026-11-10): Lua `__index` / `__newindex` frames in tier 1 and array accesses of metatabled tables SHIPPED; what is not done

- **Shipped** (docs/JIT_TRACE_REF.md "BUNDLE-S", docs/PERF.md "BUNDLE-S"): `t1q` bits 2 / 3 (process default now 13) and `amt` (process default 1). Gate set cycles 0.9742 / instructions 0.9864, all-223 0.9934 / 0.9973; msgpack_pure 0.846, template_etlua 0.884, class_middleclass **0.739** (instructions 0.793), bignum_arith **0.838** (0.755), real_dis_ppc 0.969, lru_cache 0.980. The census estimate for the dominant piece was 0.97 to 0.99 (haircut 2 to 5x); the measured gain is far above it because the stay behind every exit was long (the interpreter ran the handler *and* the rest of the caller until the next entry) and class_middleclass's `isSubclassOf` walks an `__index` function chain on every call.
- **The trap found on the way** (do not repeat): putting the continuation after every table access into the resume table (what `Cold::ArithMm` / `Cold::Cmp` do for their rare sites) turned every `GET*` / `SET*` pc into an interpreter re-entry point, which moves the end of every stay and so the trace formation of *all* programs (`apps/expr_lang` +9.5 % cycles with no `__index` function in it; `mustache_lustache` 0.875 and `class_middleclass` 0.805 in the first pass were the same effect partly). The entries are private to `ret_mm` now (`RESUME_MM_ONLY`). **Idea not tried:** the same private entries for `armm` (arithmetic metamethod frames, OFF since Bundle D because `programs/metamethods` kept its loop in tier 1: 2.24x) and `Cold::Cmp`; the formation effect of `armm` may have been this one. A one-line change (`Cold::ArithMm` pushes into `s1_resume`) and a re-measure of `programs/metamethods` would show.
- **Interpreter re-entry after an exit is worth something by itself.** The first pass (public resume entries) gave `mustache_lustache` 0.875 / 0.838 instructions with `t1q=9`; with private entries it is 1.013. A generic "re-enter tier 1 right after the instruction that exited" (resume entries at the pc after the most frequent exit sites, chosen by the exit counters) is a candidate for a separate bundle; it is also the mechanism that changed `expr_lang` by +9.5 %, so it needs the C45 formation understanding first (docs/JIT_PARKED.md C58).
- **BUNDLE-V (2026-11-10) did (3) in part and the `armm` idea of the "trap" bullet above: arithmetic / bitwise / unary handler frames (`t1q` bit 4, private entries), `TAILCALL` in metamethod frames (bit 5), general `RETURN` incl. vararg frames (bit 6), see C62.**
- **Not built.** (1) `__call` through tier 1 (a table with a Lua `__call` called from tier 1 goes through `jit_call`'s general path; the interpreter and the recorder run it in place), `__len` / `__concat` / `__unm` / `__close` handlers. (2) `__index` / `__newindex` functions in the tier-1.5 region fallbacks (`capture_rejoin`: the cold stubs rejoin through captured labels; the mm tail would need the same capture for `land_of`). (3) A handler `RETURN` with more than one result or `B == 0` (vararg tail), vararg frames and frames with a registered to-be-closed variable still leave tier 1 at the `RETURN` (the interpreter's `mm_return` finishes them). (4) `__index` as a C function, `__newindex` as a table chain (`H_EXIT` as before). (5) A trace-side change: the recorder already inlines `__index` / `__newindex` functions (`mm_index_fn`, `mm_index_int`, `mm_newindex_fn`); no new recorder piece was needed for S1 (the census's hot exits were all in tier 1). The recorder gaps that remain: `lib_tinsert` / `lib_tremove` on tables with a metatable (`MetaTable`), a read of a nil element of a table whose metatable's absence of `__index` was never looked up (`Metamethod`).
- **Layout lottery (equal instructions, `off` column = the same binary with both knobs off reproduces them):** `corpus2/heldout/prototype_objects` +6 % cycles at 1.002 instructions (the base-again column says +1.8 %, and the `off` column +6.3 %: the binary, not the mechanism), `lib/sel_loop` +10 % in the `new` and `s1only` columns but +1.4 % `off` (and `lib/sel_count` 0.85, `lib/sel_n` 0.91, `programs/heapsort` 0.87 the other way: code moved by the new cold stubs; instructions identical), `jit-trace/alloc_ctor`, `lib/m_type`, `lib/gsub_fn` +2..3 % in every column. Reported, not chased.
- **Gate misses to be honest about:** see docs/PERF.md "BUNDLE-S", "Gate misses".


### C61: BUNDLE-R (2026-11-10): more library calls on trace; what is not built and why

- **Shipped** (docs/JIT_TRACE_REF.md "BUNDLE-R", docs/PERF.md "BUNDLE-R"): `libcall` bits 3..9 (process default 7 -> 1023): `string.rep` with the result class as a trace guard, `table.unpack`, `table.move`, `tonumber` of a non-numeral, concatenated string keys through `HREFV`, lean `find`.
- **Census of what still aborts or stitches** (default config before the bundle, one run of all 223 programs with a throw-away build that logged the callee of every recorder `CFunc` / `Nyi(Call)` / stitch; callee resolved with `nm`; "cycle share" = share of the total base cycles of the programs that hit the callee at least once, so it is an upper bound on the gain): `str_match` 19 programs / 22 %, `str_format` (formats `tfmt` does not take: floats, `%5.2f`, `%g`, `%q`, ...) 22 / 19 %, `str_gsub` 16 / 15 %, `str_find` with patterns 12 / 10 %, `table.concat` 11 / 8 %, `table.sort` 8 / 7 %, `str_rep` 9 / 7 % (now recorded), `gmatch` 7 / 6.5 % (the recorded `GMATCH` takes one-string captures only), `table.unpack` 7 / 6.5 % (now recorded for `unpack(t)`), `table.remove` positional form 9 / 6 %, `tonumber` with a base or a non-numeral 9 / 6 % (non-numeral now recorded), `tostring` of non-numbers 6 / 6 %, `string.char` of non-integers 5 / 6.6 %, `coroutine.yield` / `resume` / `wrap` 5 / 3.5 %, `error` 4 / 2.4 %, `table.insert` positional 7 / 3 %, `table.move` 5 / 1.7 % (now recorded). The big ones are heavy library functions (pattern matcher, formatter, `sort`, `concat`): recording them would remove only the stitch exit / re-entry, not the work, so they were not built in this bundle. Cheapest next candidates by (cycle share x chance that the call protocol matters): `table.concat(t [, sep])` as an allocating `LIB` (the helper is `tconcat` on a table without metatable, same result class problem as `rep`: use the length-guard trick of bit 4 with a `TLEN`-based bound), `string.format` with a float argument (needs a third type code in the `fmt_values` flag layout: 2 bits per argument are used up, 9 bits for three arguments still fit the 16 flag bits), positional `table.insert` / `table.remove` of small tables (shifting loop unrolled like `table.move`).
- **`string.rep` class**: see TRACE_REF; the `Alloc`-site classification of the `NE %lib, 0` guard is the general reason why a class-polymorphic `LIB` result never got side traces. Any future alloc `LIB` whose result class varies (`tconcat`, `sub` with a varying length, `gsub`) should guard the length class with an ordinary guard before the helper, as `libcall` bit 4 does. `lib/sub` (class guard inside the helper) is the same shape and is probably paying this price today (not measured).
- **Not built: R5 `math.random(m, n)` inline generator step** (`lib/m_random` 238 instructions per call, `LIB RANDOM` lean entry only). The state lives in four integer upvalues of the `math.random` C closure; an inline step needs `UREF`-like loads of C-closure upvalues (the IR has them for Lua closures only), the xoshiro256** rotate / shift / xor sequence as IR ops (no rotate op: two shifts and an `or`), four stores, and the `project` loop with its data-dependent retry (a loop exit in the middle of the trace). Estimated 1.5 bundles for 238 -> about 80 instructions per call on one micro program (`lib/m_random`) and `m_random_f`; `math.random` appears in the source of 3 of the 223 programs only (`lib/m_random`, `lib/m_random_f`, and `programs/fasta`, which defines its own LCG named `random`): no real program has the library call in a hot loop. Cheaper partial step if wanted: avoid `set_upvalue`'s barrier path in `store_rand` (the four stores of `Value::int` into a C closure need no barrier).
- **Not built: R6 inline short-string `<` / `<=`** (`lib/str_lt_short`, needs a byte-swap micro-op in the portable IR, in `x86.rs` (`BSWAP`), `emu.rs` and the lockstep): left because R1-R5 used the budget and the program is one micro benchmark; the lean entry from BUNDLE-H stays. Design: both strings short (guard classes), compare the first `min(len)` bytes with 8-byte big-endian loads (`bswap` + unsigned compare) when both lengths are >= 8, else loop through the helper; most real keys are shorter than 8 bytes, so a two-word compare with the length tie-break covers short strings entirely.
- **Not built: `tonumber(s, base)`** (stays `Nyi`; 3 programs), **`unpack(t, i, j)`** (stays a plain C call), `select('#')` / `select(n, ...)` were already recorded (BUNDLE-P-E), `string.byte` multi-result already recorded for `byte(s, i, j)` with constant-shaped ranges up to 8 (`lib/byte_multi` is at 110 instructions per iteration; the gap to LuaJIT is the loop and the argument loads, not the call), `fmt_s` is recorded since BUNDLE-C (the program is tiny: 28 Minstr).
- **Cheap retests**: every bit is one `libcall=` value (`RUSTYNAIL_JIT_TRACE_LIMITS=libcall=N`; 1023 = all, 7 = BUNDLE-H state); `LIBCALL_PROCESS` is the only constant. The jit-diff set `br` and `tr_core::br_lib_matrix` cover bits 5..8; `bh_lib_matrix` bits 3..4; `ba_find_matrix` bit 9.

### C62: BUNDLE-V (2026-11-10): tier-1 arithmetic metamethod frames, tail calls and general returns in metamethod frames, recorded upvalue stores SHIPPED; what is not done

- **Shipped** (docs/JIT_TRACE_REF.md "BUNDLE-V", docs/PERF.md "BUNDLE-V"): `t1q` bits 4 / 5 / 6 (process default 61 -> 125; `t1q` named values clamp to 127) and `upbar` (process 1). All 223 programs 0.9955 cycles / 0.9963 instructions, gate set 0.9840 / 0.9723; `classic_vector` **0.67**, `lru_cache` 0.95 / 0.846 and `containers_lru_heap` 0.89 / 0.831 (both V2), `bignum_arith` 0.94. No gain: `oop_mixins`, `cd` (their metamethod frames are inside traces), `roguelike_turns` (the `Nyi(Setupval)` aborts are gone, the loops still end at `IntMiss`).
- **Gate misses:** `apps/expr_lang` +3.6 % cycles at 15 runs (1.001 instructions; `off` column +2.0 %, `off0` +1.2 %: layout of the new cold stubs plus about 1.6 %); `awfy/cd` +2.0 % instructions / +1.7 % cycles (V2: `upbar=0` gives 1.001; the trace set of the upvalue-store loops changes). See docs/PERF.md.
- **Built, kept narrow on purpose:** `jit_mm_tail_ok`: native (non-Lua-closure) tail callees in `__index` / `__newindex` frames stay with the interpreter (`class_middleclass` +7 % cycles, trace formation); they are in tier 1 for arithmetic / comparison callers (needed by `programs/metamethods`: `return setmetatable(...)`). Cheap retest: change the predicate; `bv_mm_programs` covers both shapes.
- **Not built.** (1) `__len` / `__concat` handler frames (`LEN` / `CONCAT` have their own helpers; no program of the 223 has them on the exit list). (2) The landing-label cost after partial-leaf arithmetic (`IDIV`, `MOD`, bitwise results are typed `Int` at the next pc; a metamethod result fails the tag check and exits): fix = drop the destination's type at the continuation pc when the instruction has an `armm` stub, i.e. a second code path or `Unk` typing of those results (costs the numeric fast path); a per-site hint (`t1i_hints`) would do. (3) A vararg-frame mutant (`bv_vararg_delta_skip` dropped: a wrong `top` after the pop is not observable by output). (4) Upvalue-barrier coverage in real runs: no stress mode produced a black upvalue with a white value at a recorded store, the hand-built `tr_asm` test is the only proof that the inline test and the helper call work together. (5) `upbar` for upvalue stores of collectable values into *open upvalues that alias a slot of an inlined (non-root) frame* still aborts `UpvalAlias`, as before. (6) PGO / ASan / fuzz.
- **Record:** the BUNDLE-D 2.24x of `programs/metamethods` with `armm` was the ordinary resume entries (formation), as BUNDLE-S suspected; `armm=1` with private entries is neutral there.
