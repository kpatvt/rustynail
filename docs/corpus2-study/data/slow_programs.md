| program | split | ours/LJ (Z5) | Mcyc ours | trace% | tier-1% | interp% | helpers% | other% | main signatures (abort counts / exits) |
|---|---|---|---|---|---|---|---|---|---|
| class_middleclass | corpus | 8.17 | 2599 | 1 | 0 | 77 | 0 | 22 | maxside_blacklisted_exits:6592, recursion/depth:4 |
| classic_vector | corpus | 6.94 | 1744 | 2 | 0 | 27 | 0 | 70 |  |
| lua_lexer | corpus | 3.13 | 1407 | 16 | 29 | 0 | 7 | 48 | maxside_blacklisted_exits:2292879, leave_frame:12, callee_loop:2, c_iterator:2 |
| astar_jumper | corpus | 2.02 | 2018 | 18 | 27 | 21 | 4 | 27 | t1_exit_mm_compare:478151, maxside_blacklisted_exits:400962, table_shape:12, recursion/depth:9, leave_frame:5, vararg:2 |
| json_rxi | corpus | 1.81 | 2231 | 6 | 27 | 1 | 12 | 54 | maxside_blacklisted_exits:279665, leave_frame:19, callee_loop:12, lib_call_not_recorded:4, table_shape:4, leave_loop:2 |
| inspect_lua | corpus | 1.78 | 1878 | 2 | 26 | 9 | 9 | 52 | t1_exit_mm_compare:104727, maxside_blacklisted_exits:42310, table_shape:26, leave_frame:17, leave_loop:7, multret:6 |
| coro_scheduler | corpus | 1.76 | 1614 | 0 | 29 | 27 | 7 | 35 | callee_loop:13, tailcall:8, lib_call_not_recorded:6, c_iterator:4, closure:2, leave_loop:2 |
| parser_combinator | corpus | 1.58 | 2461 | 0 | 43 | 1 | 9 | 46 | maxside_blacklisted_exits:17629, leave_frame:38, callee_loop:21, recursion/depth:10, lib_call_not_recorded:4, inner_loop:3 |
| iter_pipeline | dev | 20.17 | 2752 | 1 | 14 | 67 | 4 | 15 | leave_frame:8, tfor_lua_iter:6, callee_loop:2, closure:2 |
| text_adventure | dev | 2.41 | 2891 | 2 | 32 | 1 | 9 | 55 | maxside_blacklisted_exits:121433, leave_frame:12, lib_call_not_recorded:9, leave_loop:3, c_iterator:2 |
| oop_mixins | dev | 2.28 | 2083 | 1 | 10 | 56 | 3 | 29 | lib_call_not_recorded:40, vararg:4 |
| regex_nfa | dev | 2.17 | 1195 | 20 | 25 | 2 | 8 | 44 | maxside_blacklisted_exits:1067310, leave_loop:24, table_shape:22, leave_frame:18, callee_loop:2 |
| diff_levenshtein | dev | 2.17 | 715 | 24 | 3 | 37 | 1 | 33 | maxside_blacklisted_exits:133434, leave_frame:3, callee_loop:3, leave_loop:3 |
| coro_generators | dev | 1.86 | 1339 | 11 | 3 | 42 | 1 | 41 | maxside_blacklisted_exits:282965, recursion/depth:11, c_iterator:8, lib_call_not_recorded:8, closure:3, leave_frame:2 |
| lua_parser | dev | 1.80 | 1451 | 1 | 33 | 3 | 14 | 47 | maxside_blacklisted_exits:75106, leave_frame:29, lib_call_not_recorded:8, tailcall:6, closure:6, recursion/depth:5 |
| real_dis_arm | dev | 1.65 | 3304 | 3 | 20 | 1 | 8 | 68 | maxside_blacklisted_exits:483160, table_shape:8, lib_call_not_recorded:2, c_iterator:2, leave_frame:2, callee_loop:2 |
| bignum_arith | dev | 1.63 | 1050 | 19 | 41 | 6 | 5 | 26 | metamethod_record:62, callee_loop:5, leave_loop:4, leave_frame:3, tailcall:2, inner_loop:2 |
| stream_closures | heldout | 4.60 | 1894 | 0 | 54 | 2 | 16 | 26 | tfor_lua_iter:8, leave_frame:6, table_shape:6, callee_loop:5, lib_call_not_recorded:3, leave_loop:2 |
| scheme_interp | heldout | 2.47 | 2554 | 17 | 28 | 3 | 14 | 37 | maxside_blacklisted_exits:2691895, t1_exit_mm_compare:148597, callee_loop:20, leave_frame:15, leave_loop:8, tailcall:4 |
| markov_text | heldout | 1.80 | 1702 | 37 | 7 | 0 | 3 | 52 | maxside_blacklisted_exits:1631, multret:10, table_shape:6, leave_loop:1 |
| forth_vm | heldout | 1.72 | 1197 | 71 | 1 | 0 | 0 | 28 | maxside_blacklisted_exits:95879, leave_frame:2 |
| real_dis_ppc | heldout | 1.66 | 2987 | 1 | 18 | 2 | 10 | 69 | callee_loop:8, table_shape:6, lib_call_not_recorded:4, c_iterator:2, leave_frame:2 |
| real_dis_mips | heldout | 1.59 | 2834 | 0 | 21 | 1 | 7 | 71 | callee_loop:17, table_shape:9, lib_call_not_recorded:4, c_iterator:2, leave_frame:2 |
