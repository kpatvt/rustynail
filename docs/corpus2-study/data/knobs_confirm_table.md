| knob | set | n | geomean all | affected (instr. changed) | geomean affected | best | worst | output problems |
|---|---|---|---|---|---|---|---|---|
| dup | heldout | 13 | 0.995 | 0 | nan | - | - | - |
| dup | corpus | 18 | 1.002 | 0 | nan | - | - | - |
| dup | dev | 4 | 1.001 | 0 | nan | - | - | - |
| maxside=24 | heldout | 13 | 1.001 | 6 | 0.999 | vending_fsm 0.969 | scheme_interp 1.055 | - |
| maxside=24 | corpus | 18 | 0.948 | 4 | 0.783 | class_middleclass 0.398 | lua_lexer 0.990 | - |
| maxside=24 | dev | 4 | 0.923 | 2 | 0.838 | diff_levenshtein 0.682 | expr_compiler 1.029 | - |
| unroll=3 | heldout | 13 | 0.984 | 2 | 0.902 | scheme_interp 0.862 | forth_vm 0.945 | - |
| unroll=3 | corpus | 18 | 1.009 | 2 | 1.069 | astar_jumper 1.067 | class_middleclass 1.070 | - |
| unroll=3 | dev | 4 | 0.915 | 3 | 0.888 | regex_nfa 0.711 | expr_compiler 1.005 | - |
| unroll=8,maxdepth=8 | heldout | 13 | 0.982 | 2 | 0.892 | scheme_interp 0.861 | forth_vm 0.923 | - |
| unroll=8,maxdepth=8 | corpus | 18 | 1.021 | 2 | 1.173 | msgpack_pure 1.002 | astar_jumper 1.374 | - |
| unroll=8,maxdepth=8 | dev | 4 | 0.919 | 3 | 0.894 | regex_nfa 0.715 | lua_parser 1.028 | - |
| maxside=24,unroll=3 | heldout | 13 | 0.988 | 6 | 0.970 | scheme_interp 0.910 | dijkstra_routes 1.021 | - |
| maxside=24,unroll=3 | corpus | 18 | 0.960 | 4 | 0.824 | class_middleclass 0.465 | astar_jumper 1.032 | - |
| maxside=24,unroll=3 | dev | 4 | 0.843 | 4 | 0.843 | diff_levenshtein 0.684 | expr_compiler 1.026 | - |
| fnroot=1 | heldout | 13 | 0.946 | 7 | 0.901 | stream_closures 0.501 | dijkstra_routes 1.053 | - |
| fnroot=1 | corpus | 18 | 0.929 | 4 | 0.712 | class_middleclass 0.415 | msgpack_pure 0.976 | - |
| fnroot=1 | dev | 4 | 1.004 | 1 | 0.993 | expr_compiler 0.993 | expr_compiler 0.993 | - |
| absmiss=1 | heldout | 13 | 0.974 | 3 | 0.891 | scheme_interp 0.807 | dijkstra_routes 1.031 | - |
| absmiss=1 | corpus | 18 | 0.995 | 3 | 0.969 | json_rxi 0.922 | lua_lexer 0.994 | - |
| absmiss=1 | dev | 4 | 1.020 | 1 | 1.098 | regex_nfa 1.098 | regex_nfa 1.098 | - |
| sinkpol=1 | heldout | 13 | 0.946 | 3 | 0.792 | stream_closures 0.517 | real_dis_mips 1.043 | - |
| sinkpol=1 | corpus | 18 | 1.027 | 3 | 1.159 | parser_combinator 1.013 | classic_vector 1.495 | - |
| sinkpol=1 | dev | 4 | 1.004 | 2 | 1.007 | expr_compiler 0.994 | lua_parser 1.019 | - |
| polyside=48,polywait=2,polyframes=2 | heldout | 13 | 0.999 | 2 | 0.994 | forth_vm 0.993 | vending_fsm 0.994 | - |
| polyside=48,polywait=2,polyframes=2 | corpus | 18 | 0.950 | 3 | 0.728 | class_middleclass 0.397 | astar_jumper 0.996 | - |
| polyside=48,polywait=2,polyframes=2 | dev | 4 | 1.010 | 0 | nan | - | - | - |
