| component | cycle-weighted share, corpus18 + dev (37 programs) |
|---|---|
| interpreter (run_mode/execute) | 11.8% |
| metamethod/index paths (index_chain, mm_*, finish_get) | 2.0% |
| string creation (new_str, concat) | 4.6% |
| table growth (newkey, resize, mainposition, rehash) | 4.8% |
| table set slow path (finish_set, jit_set) | 2.9% |
| tier-1/trace call helpers (jit_call, ccall, pre_call, pre_tail_call) | 5.3% |
| pattern matching (MatchState, gmatch, find, gsub) | 12.2% |
| string.format | 1.2% |
| allocation (jit_tr_alloc, alloc) | 4.7% |
| GC (propagate, sweep, mark) | 0.5% |
| coroutines (resume/yield) | 0.7% |
| trace code | 2.6% |
| tier-1 code | 0.0% |

| program | interpreter | metamethod/index paths | string creation | table growth | table set slow path | tier-1/trace call helpers | pattern matching | string.format | allocation | GC | coroutines | trace code | tier-1 code |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| corpus__astar_jumper | 16 | 4 | 0 | 4 | 2 | 1 | 0 | 0 | 6 | 0 | 0 | 0 | 0 |
| corpus__class_middleclass | 66 | 18 | 1 | 0 | 0 | 0 | 0 | 0 | 2 | 0 | 0 | 0 | 0 |
| corpus__classic_vector | 22 | 5 | 0 | 16 | 7 | 2 | 0 | 0 | 17 | 0 | 0 | 0 | 0 |
| corpus__containers_lru_heap | 16 | 1 | 2 | 5 | 5 | 2 | 0 | 0 | 0 | 0 | 0 | 1 | 0 |
| corpus__coro_scheduler | 24 | 1 | 1 | 3 | 3 | 6 | 0 | 0 | 4 | 0 | 11 | 0 | 0 |
| corpus__csv_ini_yaml | 0 | 0 | 8 | 7 | 4 | 7 | 35 | 0 | 4 | 1 | 0 | 2 | 0 |
| corpus__inspect_lua | 9 | 0 | 3 | 3 | 1 | 7 | 17 | 0 | 1 | 0 | 0 | 5 | 0 |
| corpus__json_rxi | 1 | 0 | 8 | 4 | 2 | 7 | 5 | 1 | 6 | 0 | 0 | 3 | 0 |
| corpus__lua_lexer | 0 | 0 | 4 | 2 | 0 | 5 | 7 | 1 | 12 | 0 | 0 | 3 | 0 |
| corpus__markdown_conv | 3 | 0 | 4 | 2 | 1 | 6 | 56 | 0 | 2 | 0 | 0 | 4 | 0 |
| corpus__msgpack_pure | 15 | 2 | 3 | 5 | 2 | 6 | 0 | 0 | 9 | 0 | 0 | 2 | 0 |
| corpus__mustache_lustache | 0 | 0 | 4 | 9 | 6 | 7 | 14 | 0 | 5 | 0 | 0 | 3 | 0 |
| corpus__parser_combinator | 0 | 0 | 2 | 10 | 7 | 3 | 14 | 0 | 4 | 0 | 0 | 3 | 0 |
| corpus__penlight_mix | 4 | 1 | 5 | 4 | 3 | 3 | 53 | 1 | 2 | 0 | 0 | 3 | 0 |
| corpus__serpent_ser | 6 | 1 | 10 | 2 | 1 | 6 | 5 | 6 | 4 | 0 | 0 | 6 | 0 |
| corpus__sha2_base64 | 6 | 0 | 9 | 1 | 0 | 7 | 3 | 0 | 0 | 0 | 0 | 3 | 0 |
| corpus__template_etlua | 14 | 4 | 7 | 1 | 0 | 4 | 21 | 6 | 1 | 0 | 0 | 6 | 0 |
| corpus__testrunner_luaunit | 9 | 0 | 4 | 7 | 5 | 7 | 11 | 1 | 5 | 0 | 0 | 2 | 0 |
| dev__bignum_arith | 5 | 1 | 0 | 11 | 9 | 1 | 0 | 0 | 3 | 0 | 0 | 1 | 0 |
| dev__coro_generators | 36 | 1 | 0 | 0 | 0 | 3 | 2 | 0 | 3 | 0 | 21 | 1 | 0 |
| dev__csv_report | 0 | 0 | 10 | 6 | 3 | 4 | 12 | 2 | 9 | 6 | 0 | 6 | 0 |
| dev__diff_levenshtein | 30 | 5 | 0 | 13 | 2 | 0 | 0 | 0 | 5 | 0 | 0 | 0 | 0 |
| dev__ecs_world | 2 | 0 | 0 | 5 | 2 | 4 | 0 | 0 | 3 | 1 | 0 | 1 | 0 |
| dev__event_promise | 5 | 0 | 0 | 3 | 2 | 2 | 0 | 0 | 9 | 6 | 0 | 2 | 0 |
| dev__expr_compiler | 0 | 0 | 1 | 6 | 5 | 5 | 10 | 0 | 10 | 0 | 0 | 2 | 0 |
| dev__iter_pipeline | 66 | 0 | 0 | 0 | 0 | 16 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| dev__lua_parser | 2 | 1 | 4 | 4 | 3 | 7 | 14 | 1 | 4 | 0 | 0 | 2 | 0 |
| dev__memo_cache | 2 | 0 | 6 | 12 | 5 | 4 | 0 | 0 | 4 | 4 | 0 | 1 | 0 |
| dev__oop_mixins | 43 | 18 | 1 | 2 | 1 | 2 | 0 | 0 | 1 | 1 | 0 | 2 | 0 |
| dev__particles_physics | 0 | 0 | 0 | 9 | 2 | 1 | 0 | 0 | 16 | 1 | 0 | 1 | 0 |
| dev__real_dis_arm | 0 | 0 | 12 | 2 | 2 | 6 | 13 | 6 | 6 | 0 | 0 | 7 | 0 |
| dev__real_dis_x86 | 1 | 0 | 13 | 0 | 0 | 9 | 20 | 7 | 4 | 0 | 0 | 4 | 0 |
| dev__regex_nfa | 0 | 0 | 0 | 14 | 15 | 1 | 0 | 0 | 11 | 0 | 0 | 1 | 0 |
| dev__template_gsub | 1 | 0 | 8 | 2 | 1 | 9 | 34 | 2 | 4 | 0 | 0 | 5 | 0 |
| dev__text_adventure | 0 | 0 | 9 | 4 | 4 | 8 | 12 | 3 | 3 | 0 | 0 | 3 | 0 |
| dev__varargs_util | 2 | 0 | 3 | 6 | 2 | 11 | 3 | 1 | 7 | 0 | 0 | 1 | 0 |
| dev__wordfreq_wrap | 0 | 0 | 7 | 5 | 2 | 8 | 22 | 0 | 5 | 1 | 0 | 7 | 0 |
| micro__alloc_short | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 3 | 0 |
| micro__alloc_short_abl | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 2 | 0 |
| micro__callee_loop | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 41 | 0 | 0 | 0 | 0 |
| micro__callee_loop_inl | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 56 | 0 | 0 | 0 | 0 |
| micro__citer | 0 | 0 | 5 | 0 | 0 | 8 | 31 | 0 | 0 | 0 | 0 | 4 | 0 |
| micro__closure_loop | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 30 | 0 | 0 | 2 | 0 |
| micro__closure_loop_abl | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 3 | 0 |
| micro__coro_body | 81 | 0 | 0 | 0 | 0 | 4 | 0 | 0 | 0 | 0 | 8 | 0 | 0 |
| micro__coro_body_abl | 5 | 0 | 0 | 0 | 0 | 14 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| micro__poly_site | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| micro__poly_site_abl | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| micro__strfind | 0 | 0 | 18 | 0 | 0 | 7 | 33 | 6 | 0 | 0 | 0 | 7 | 0 |
| micro__tail_vararg | 0 | 0 | 0 | 0 | 0 | 21 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| micro__tail_vararg_abl | 1 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 4 | 0 |
