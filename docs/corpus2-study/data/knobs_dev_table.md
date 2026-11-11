| knob | programs affected (instr. changed) | geomean all dev | geomean affected (min) | geomean affected (median) | best | worst | output/rc problems |
|---|---|---|---|---|---|---|---|
| dup | 0 | 1.002 | nan | nan | - | - | - |
| fnroot=1 | 9 | 0.986 | 0.961 **POSITIVE** | 0.960 | iter_pipeline__whileloop 0.742 | real_dis_x86 1.042 | - |
| framesides=1 | 1 | 1.001 | 0.990 | 0.991 | expr_compiler 0.990 | expr_compiler 0.990 | - |
| nest=2 | 2 | 1.002 | 0.998 | 1.001 | particles_physics 0.984 | real_dis_arm 1.012 | - |
| nest=0 | 10 | 1.019 | 1.036 | 1.039 | particles_physics 0.919 | regex_nfa 1.208 | - |
| stitchmin=4 | 4 | 1.006 | 1.012 | 1.011 | lua_parser 0.983 | real_dis_x86 1.075 | - |
| maxside=24 | 4 | 0.987 | 0.923 **POSITIVE** | 0.922 | diff_levenshtein 0.685 | expr_compiler 1.032 | - |
| maxside=64 | 8 | 0.997 | 0.990 | 0.990 | diff_levenshtein 0.664 | expr_compiler 1.248 | - |
| polyside=48,polywait=2,polyframes=2 | 1 | 1.006 | 1.022 | 1.012 | coro_generators 1.022 | coro_generators 1.022 | - |
| sidemin=0 | 5 | 1.008 | 1.005 | 1.004 | ecs_world 0.912 | diff_levenshtein 1.164 | - |
| sidemin=2 | 5 | 1.010 | 1.018 | 1.015 | ecs_world 0.963 | diff_levenshtein 1.171 | - |
| sinkmt=1,sinkside=1 | 4 | 1.004 | 1.005 | 1.007 | expr_compiler 0.993 | text_adventure 1.020 | - |
| sinkmt=1,sinkside=1,sinkstub=1 | 3 | 1.000 | 0.988 | 0.991 | particles_physics 0.951 | lua_parser 1.017 | - |
| sinkmt=1,sinkside=1,sinkstub=1,sinkdemote=1024 | 4 | 1.005 | 1.006 | 1.001 | particles_physics 0.985 | lua_parser 1.029 | - |
| sinkpol=1 | 5 | 0.999 | 0.982 | 0.978 | particles_physics 0.885 | lua_parser 1.021 | - |
| recdead=1 | 7 | 1.009 | 1.018 | 1.019 | expr_compiler 0.922 | regex_nfa 1.220 | - |
| recgate=24 | 0 | 1.000 | nan | nan | - | - | - |
| ubar=1 | 0 | 1.002 | nan | nan | - | - | - |
| absmiss=1 | 4 | 0.996 | 0.975 | 0.978 | ecs_world 0.887 | regex_nfa 1.086 | - |
| absmiss=1,ckey=1 | 5 | 0.998 | 0.990 | 0.979 | ecs_world 0.889 | regex_nfa 1.088 | - |
| ckey=1 | 1 | 1.003 | 1.031 | 0.996 | csv_report 1.031 | csv_report 1.031 | - |
| probe=3 | 1 | 1.001 | 1.000 | 1.005 | expr_compiler 1.000 | expr_compiler 1.000 | - |
| onestrike=2 | 4 | 1.014 | 1.053 | 1.056 | lua_parser 0.989 | regex_nfa 1.212 | - |
| pollbud=1 | 1 | 1.004 | 1.005 | 1.006 | bignum_arith 1.005 | bignum_arith 1.005 | - |
| loopany=1 | 2 | 1.002 | 0.988 | 0.990 | iter_pipeline__whileloop 0.981 | real_dis_arm 0.995 | - |
| fnroot=1,fnloops=1 | 9 | 0.987 | 0.962 **POSITIVE** | 0.960 | iter_pipeline__whileloop 0.740 | real_dis_x86 1.040 | - |
| contok=1 | 3 | 1.008 | 1.017 | 1.023 | expr_compiler 1.007 | regex_nfa 1.025 | - |
| unroll=3 | 4 | 0.985 | 0.909 **POSITIVE** | 0.917 | regex_nfa 0.710 | expr_compiler 0.999 | - |
| unroll=8,maxdepth=8 | 4 | 0.985 | 0.915 **POSITIVE** | 0.917 | regex_nfa 0.708 | lua_parser 1.030 | - |
| fck=1024 | 0 | 1.000 | nan | nan | - | - | - |
| framemin=16 | 1 | 1.002 | 1.002 | 1.005 | lua_parser 1.002 | lua_parser 1.002 | - |
| vargtr=1 | 0 | 1.005 | nan | nan | - | - | - |
| t3idx=4 | 1 | 1.003 | 0.984 | 0.988 | oop_mixins 0.984 | oop_mixins 0.984 | - |
| lockstep=1 | 1 | 1.008 | 1.024 | 1.023 | particles_physics 1.024 | particles_physics 1.024 | - |
