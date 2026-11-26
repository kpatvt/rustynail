# Benchmark session-o (subject b30f1bc = session-n subject + rbp fixes + library handles M1/R1/B1/B2 ON; reroot, budhoist, pollk OFF)

Method as session-n (fast pass, 223 programs, 127 flagged programs rerun at 9 runs, CPU 4, FORCE_STALE=1: LuaJIT column stale). Data: fast_all.json, rerun_15runs_cpu4.json, per_program.csv, tables.md.
Heavy gate before the run: oracle KNOWN-only, jit-diff PASS, mutants 873/875 caught (survivors rec_inner_first_stale, tmove_interleaved_guards, both unexamined).

| build | vs session-n cyc / ins | ours/LuaJIT cyc (session-n) | programs slower than LuaJIT |
|---|---|---|---|
| 5.5 plain | 0.991 / 0.997 | 0.900 (0.908) | 110 of 219 (113) |
| 5.5 PGO | 0.998 / 0.998 | 0.828 (0.830) | 93 (94) |
| 5.4 plain | 0.997 / 0.997 | 0.955 (0.958) | 123 |
| 5.4 PGO | 0.999 / 0.998 | 0.881 (0.882) | 109 |

Best movers vs session-n (5.5 plain cycles): oop_inherit 0.518, tremove_pop 0.554, dot 0.748, byte_multi 0.869, it_next_empty 0.888, it_next 0.900, metamethods 0.929, events 0.933.
Worst: tonumber_int 1.172, rep_sep 1.149, lru_cache 1.076, prototype_objects 1.070, diff_levenshtein 1.056, log_analyzer 1.047 (not investigated: layout noise or real).

Follow-up (2026-11-26): `lib/rep_sep` (+15.7% instructions, all four builds) was caused by B2 `opencall`; `opencall=0` restores session-n's 432 M instructions. B2 gained only 3.6% on byte_multi, so `OPENCALL_PROCESS` is now 0 (docs/JIT_TRACE.md section 59 note); the other five worst movers have flat instructions (layout noise). Numbers above are for the build with opencall ON.
