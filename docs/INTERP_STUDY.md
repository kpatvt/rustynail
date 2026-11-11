# Interpreter/runtime cost study (Opus, read-only, main 6a218c08, 2026-11-03)

Question: how much of the remaining gap to LuaJIT is interpreter cost, and is a rewrite worth it?
Labels: [M] measured, [E] estimated. Zen 5, 5.5 only, perf cycles:u/instructions:u, min of 3.

## Verdict
- Interpreter proper ~6% of the log gap to LuaJIT; interpreter + all runtime (tables, strings, GC, library, parser) ~22%. The other ~78% is JIT coverage/code quality (awfy: 93%).
- A LuaJIT-class interpreter rewrite is NO-GO: even an infinitely fast interpreter gives -11.7% overall geomean (programs -12.6, awfy -3.9, apps -22.8, real -16.5); a realistic one -3..-5%, at 60-100+ engineer-days and permanent dual maintenance (Rust interpreter must stay).

## Measured
- JIT-off ours/LuaJIT -joff: programs 1.08, awfy 1.26, apps 1.13, real 1.24, all 49 = 1.15. Ours matches PUC on dispatch-heavy code, 0.5-0.75x PUC on library-heavy code.
- Dispatch is not the problem (branch misses ~0.15%; empty for-loop at parity). Costs are calls/returns (Lua call 35 cyc/204 instr vs LuaJIT 11/69), the C-call boundary (37 vs 10), table field access (18.5 vs 7.3), coroutine switch (185 vs 73), table `==` (37 cyc, 2x PUC: mm_eq + equal_obj twice). Store-to-load-forwarding failures in CallInfo/thread fields: 9.7 per C call vs 2.
- Per program JIT-off/LuaJIT-joff: tfor_loops 2.72, table_hash 2.43, coroutines 1.94, lua_interp 1.86, load_big 1.67 (parser), expr_lang 1.48, richards 1.42.
- JIT-on time split (mean of 49): traces 38%, tier-1 11%, interpreter 8.7%, compile 2.6%, helpers/exits 5.5%, table 7.3%, strings 3.3%, GC+alloc 10%, library+libc 10.4%.
- JIT barely helps/hurts on OOP code (JIT-on/JIT-off): deltablue 1.20 (slower), expr_lang 0.98, raytrace 1.06, lru_cache 1.04, richards 0.99. Cause: tier-1 sends every instance-miss/__index lookup to generic `jit_get`; share of jit_get/TableRef::get/jit_set chain: towers 35.6%, deltablue 24.9, richards 24.8, list 23.3, raytrace 19.0.

## Ranked plan
1. PGO release build by default: ~-5% (P-I), no code. 
2. Tier-1 `__index`/`jit_get` short-string path (inline one level of __index in J4 templates): awfy -5..-8%, apps -3% [E]; 2-4 days.
3. Interpreter quick wins [M in scratch copy]: frameless next/ipairs in TFORCALL via existing `tfor_fast` (tfor_loops JIT-off -47%); EQ/EQK and no-metatable table fast paths (expr_lang -6% JIT-on); 1-2 days.
4. Investigate STLI in `ccall`/`pre_call_native_fast`: 1-2 days.
5. JIT coverage for apps that stay 23-56% in interpreter (Recursion/mutual-recursion aborts, functions not reaching tier 1): larger upside than any interpreter change.
6. Runtime: string creation/interning (real -5.9%), GC/alloc (programs -6%), patterns/format; 7. coroutine switch fast path.
- Not worth it: threaded dispatch (ceiling -0.5%), call-convention redesign (-2..-4%, 20-40 days, high risk), NaN-boxing/removing CallInfo (incompatible with C API/5.5 integers).

## Uncertainties
Category shares are perf self-cycles (runtime called from helpers counted as runtime); per-handler attribution is a layout heuristic; LuaJIT timings bimodal on some programs; lua_interp JIT-on bimodal (157 vs 237 Mcyc); engineer-day figures are estimates. Stray `perf-*.map` files remain in /tmp.
