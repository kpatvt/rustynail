# Coverage-blocker study (Opus, read-only, base e981bfbb, 2026-11-04)
[M] measured, [X] experiment (env-gated RNX_* knobs in a scratch copy), [E] estimate. All runs RUSTYNAIL_SEED=0, CPUs 6/7/9.

## Hygiene
Without RUSTYNAIL_SEED the string-hash seed changes table layout, so even with warm=0 the trace set varies (richards 124-185 Mcyc unseeded, identical with SEED=0). Every A/B needs SEED=0 AND `warm=0`.

## Findings
1. FnShort = "no self-call found", not short functions: with process defaults fnroot=0, recnat=1, recany=1 every function-entry head is only a native-recursion probe; aborts FnShort if root frame returns/tailcalls before a self-call, Black after 4 (record.rs fn_end ~4555, cache.rs on_abort ~1209, MAX_FN_SHORT=4). Blacklisting costs no coverage; fn_short_budget_ignored ~0 because those heads can never succeed. Generic short roots (fnroot=1, fnmin=4): richards 0.92, deltablue instr -10% cycles flat, short-script regressions (P-E2): low value. Richards coverage needs the scheduler-loop root to survive (P2).
2. TailCall: inlined-frame TAILCALL is already a frame replacement. Remaining richards aborts are SIDE traces at depth 0 (WorkerTask:run `return self.scheduler:queue(packet)`): the down-frame problem via TAILCALL; stitch-only variant gave 0 in M1. ack-style `return ack(m-1, ack(m,n-1))`: native self-call needs wanted>=0, MULTRET inner call is inlined and aborts Recursion; rewriting gives 0.61 (no suite program has it).
3. StartRefused: 100% from R2's gate in mod.rs driver_start (~416): continuation head of a proto not marked recursive (proto_recursive only set when a native self-call trace was INSTALLED, mod.rs ~1188). Refused starts cost <0.3% of cycles, but hide a blocker: recursion inside a loop (queens placeQueen) never gets a native self-call so continuation heads stay refused. Experiments: gate removed (RNX_CONT_ANY) queens 237->156 Mcyc but instr +16% on deltablue/havlak; RNX_RECMARK (mark recursive on any seen self-call) queens instr 0.65, neutral elsewhere BUT SEGV in trace code from rt::enter in default warm mode (rec_seen also enables native returns, end_rec_ret ~4931, which assume a native frame; suspected latent issue: flag never cleared when the native self-call trace is flushed). Fix: separate cont_ok from rec_seen + audit native-return guard.
4. expr_lang ROOT CAUSE: interpreter `pre_tail_call` (rn-core call.rs ~398) has NO jit_call_hook, so a function reached only by TAILCALL is never counted/compiled (evalBin, max/min/abs; richards Scheduler:release/holdCurrent). Tier-1 tail call to an uncompiled callee exits to the interpreter. RNX_TAILHOOK: expr_lang 0.66 cycles (interp 48% -> 1.4%), richards 0.89 (J4 0.82), suite geomean 0.984; cd +4%, list +3.5% regress. expr_lang traces still slower than J4 (189 vs 158 Mcyc, 1.3M exits, 14% compile): traces that lose to tier 1 should be detected and dropped. lru_cache confirmed: IntMiss->TableHash->IntMiss, Metamethod abort at __call pc 61, Nyi(Setupval). raytrace confirmed: for-loop in trace() installed twice, flushed Rerec (int/float snapshots), Identical -> Black; render x-loop aborts InnerLoop at FORPREP of inlined trace().
5. events: 332k cheap exits from Bus:emit short loops (n=1-4); LuaJIT doesn't trace emit either (18% trace), we're 1.25x: runtime vs runtime. Candidates: VARARG in vararg root frame, link-at-heads, min-trip rule; gain <5%.
6. Time gate (`warm`, DEFAULT_WARM=1.2M TSC ticks): bounce default 458/350/274/86/... Mcyc (3-5x slower in 4 of 9 runs: Rerec -> Identical -> Black); fib, fannkuch bimodal. warm=0/default: awfy 1.040 (queens 1.55: with warm=0 queens records into 1% trace/83% tier1), apps 1.003, programs 1.000. Recommendation: replace TSC gate with deterministic count gate; do not flip default to warm=0 until cont_ok fix.

## Coverage table (cycle-weighted trace share, programs LuaJIT can run)
| suite | ours default | ours warm=0 | LuaJIT |
| awfy (14) | 35.3% | 31.8% | 60.2% |
| apps (5) | 2.0% | 2.0% | 15.0% |
| programs (15) | 40.7% | 40.4% | 41.0% |
Total cycles awfy 2490 vs LuaJIT 1368 Mcyc; apps 1086 vs 765. Per program (trace/tier1/interp/runtime %, ours default; LJ trace %): towers 11/58/2/23 (LJ 67), richards 1/51/22/14 (77), raytrace 0/52/0/39 (22), queens 43/19/1/11 (82), cd 9/38/2/35 (29), json 14/17/5/56 (14), expr_lang 1/2/48/41 (36), lru_cache 0/30/11/56 (25), events 9/42/2/43 (18), list 35/54/0/9 (94), permute 68/24/0/5 (62). programs: pcall_error, table_array, string_ops, gc_churn, coroutines, load_big, markdown = runtime-bound (0-3% trace both). rec_loop 0% trace default (57% with warm=0).
Takeaway: programs set is at LuaJIT's trace share; awfy/apps gap is trace share. Many "gap" rows (string_ops, gc_churn, markdown, json, load_big, table_hash, pcall_error) are runtime-bound on BOTH sides.

## Additional fixes (not in the earlier queue)
1. Tail-call hotness: jit_call_hook in interpreter pre_tail_call (+ check tier-1 TAILCALL counts): expr_lang 0.66, richards 0.89, suite 0.984; S-M 5-8h.
2. cont_ok (continuation heads for loop recursion) separate from rec_seen + fix the SEGV/audit native returns: queens 0.72 in warm=0; M-L 10-16h, high risk until crash understood.
3. Deterministic count-based start gate instead of TSC: removes bounce/fib/fannkuch lottery; prerequisite for benchmark gates; S 3-5h.
4. Native self-call with MULTRET (ack): 0.61; none in suite; M, low priority.
5. VARARG in vararg root frame: events <5%; low priority.
6. fnroot=1,fnmin=4: richards 0.92, low priority.
7. Stats hygiene: report probe FnShort/StartRefused separately (deltablue "recorded 497" is ~460 probes); S 1-2h.
Down-frame builder scope addition: root-frame TAILCALL (frame replacement + down-frame return with pc guard). P2 note: bounce slow mode Rerec -> Identical -> Black; drop traces that lose to tier 1.
