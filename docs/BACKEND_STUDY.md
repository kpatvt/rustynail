# Backend instruction study: ours vs LuaJIT on five hot loops (Opus, read-only, base 18ea219b, 2026-11-04)
[M] measured, [X] experiment (throwaway knobs in a scratch copy), [E] estimate. Zen 5, IPC ~6 for ours: issue/port-bound, so memory-operand compare+branch pairs, loads and stores cost cycles; reg-reg moves do not (VEX 3-operand peephole removing 14 of 165 nbody instrs: 0.0% cycles [X]).

## Verdict
Most extra instructions are cheap. Classes that cost real cycles: (1) guard compare+branch pairs that read memory (3-compare HREFK, per-iteration POLL load), (2) stores that do no work (payload stores of false/true/nil; rolled-loop writeback), (3) idiv by a constant. Backend items land ~-4% geomean (range -3..-6) over awfy+apps+programs; ours/LuaJIT on awfy (~2.3) and apps (~1.9) barely move: coverage/connectivity is the main gap (DIFF_STUDY), not the backend.

## Loop facts (instr/iter ours vs LJ)
len_op rolled 93 vs 11; byte rolled 90 vs 17; field_method peeled 46-48 vs 16 (idiv group = 52% of cycles [X]); nbody inner j loop peeled 165 vs ~108 (LJ unrolls inner loop into outer); queens r-loop 21 vs 7 (queens time dominated by exits).
LuaJIT: no interrupt/hook poll in any loop body (verified from dumps), loop end = add; cmp; jle; ABC hoisted; HREFK = mov r,imm64; cmp r,[node.key]; jnz. Type checks same count as ours; 16-byte layout costs 0 extra loads/stores in peeled loops. Rolled loops reload everything each iteration (SLOAD re-checks 9-15%, tag stores 12-13%, `s:len` method chain re-resolved: 26 instrs).

## Ranked backend changes
1. Skip the payload store for nil/false/true in ASTORE (HSTORE optional): programs/sieve 0.495, awfy sieve 0.847 [X]; ~-2.8% programs geomean, -1% awfy [E]. Risk low-medium (audit readers of payload for these tags: tier-1 EQ templates, table copy/next/move, C API rawequal). S, 3-5 h.
2. Fold POLL into BUDGET's cold path (budget-exhausted stub checks TRACE_POLL, refills r11, resumes loop): awfy sieve -17.8%, storage -3.2%, field_method -2.5%, matmul -3%, awfy -2.3%, programs -0.6% [X]. A cheaper 2-instruction poll encoding does NOT help (+6.3% sieve): cost is the load among stores. Risk medium (C1 count identity, hook delivery). M, 10-16 h.
3. Fuse HREFK with the following HLOAD: one `cmp dword [n+off+8]` of val_tt|key_tt|pad (explicit zero pad in Node) + 64-bit key compare, no lea. Ceiling nbody -15.5%, bounce -15%, metamethods -9%, list -5%, queens -6..-7% [X hk1]; sound version nbody -8..-12%, awfy -2..-3% [E]. M, 10-16 h.
4. Integer `%` and `//` by non-power-of-2 constant via multiply-high: field_method -35..-40% [E]; suite ~-0.3%. S-M, 6-10 h. Needs random differential test vs rn-core::number.
5. Prune dead slots from loop-head snapshots and rolled writebacks (liveness/active locals): removes 9 of 11 PHIs in nbody, 4-5 of 7-9 writeback slots in byte/len_op. field_method -5..-10%, rolled loops ~-10% [E]. Risk medium (debug.getlocal, hooks). M-L.
6. Rolled-loop only: skip SLOAD re-checks after the first iteration, hoist writeback tag stores: -22..-26% len_op/byte [X]; superseded by header-safe peeling (E7: 0.29/0.57), build only for loops that must stay rolled.
7. 5.5 for-loop IV merge, ABC hoisting vs invariant limit, `~u`/`u-1` as derived IVs: 1-3% each on tight array loops [E].
8. Low value: VEX 3-operand FP (mandelbrot -2.5%, matmul -1.1%), budget-free loops when no count hook (naive removal made field_method 2.7x and queens 1.38x slower because rt::Mon judges stays by budget consumed), lsizenode as one compare, TLEN CSE: <1% each.

## Fixed costs
Peeled loops pay ~nothing for the 16-byte layout. Real fixed costs: 5.5 1-based array with separate tag array (+2-4 ALU per variable-index access, partly fixable by 7) and integer semantics (idiv, fixable by 4).

## Caveats
One machine; nbody comparison per IR op not like-for-like; hk1/noty/nosl/nowbtag are unsound ceilings; suite screen noisy (time-gated trace set flips permute, richards, spectral_norm, table_hash, programs/sieve by 0.5-1.9x run to run unless warm=0).
