# Post-M full benchmark, plain build: per-program tables (default mode, cycles:u, min of 11; 4-way parallel run)

## per-set geomeans against the cde96490 binary (bplain), default

| set (n) | plain/bplain cycles min | median | instructions | worst program (min) | best program (min) |
|---|---|---|---|---|---|
| awfy (14) | 0.990 | 0.989 | 0.994 | sieve 1.011 | deltablue 0.957 |
| apps (5) | 1.004 | 1.005 | 0.993 | markdown 1.060 | expr_lang 0.972 |
| programs (25) | 1.000 | 0.996 | 0.997 | table_hash 1.135 | string_ops 0.931 |
| jit-trace (29) | 0.998 | 0.999 | 0.999 | type_entry_stable 1.007 | sink_vec 0.975 |
| lib (95) | 0.989 | 0.989 | 0.997 | find_plain 1.051 | reverse 0.837 |
| real (5) | 0.988 | 0.989 | 0.979 | lua_interp 1.017 | wordfreq 0.964 |
| **all (173)** | **0.993** | 0.992 | 0.997 | | |

## warm0

| set (n) | plain/bplain cycles min | median | instructions | worst program (min) | best program (min) |
|---|---|---|---|---|---|
| awfy (14) | 0.989 | 0.990 | 0.994 | storage 1.004 | deltablue 0.962 |
| apps (5) | 0.996 | 0.993 | 0.993 | markdown 1.059 | raytrace 0.962 |
| programs (25) | 0.997 | 1.001 | 0.998 | table_hash 1.048 | string_ops 0.942 |
| jit-trace (29) | 0.999 | 1.000 | 0.999 | field_global 1.012 | rec_loop 0.984 |
| lib (95) | 0.990 | 0.990 | 0.997 | find_plain 1.045 | fmt_mixed 0.849 |
| real (5) | 0.983 | 0.987 | 0.979 | lua_interp 1.021 | wordfreq 0.952 |
| **all (173)** | **0.992** | 0.993 | 0.997 | | |

## per-set geomeans against PUC / LuaJIT (default)

5 noisiest programs overall (max of median/min of ours or LuaJIT): awfy/permute 4.14, apps/events 1.65, programs/table_hash 1.43, apps/lru_cache 1.29, awfy/cd 1.17

| set (n) | ours/PUC min | ours/PUC median | ours/LJ min (n with LJ) | ours/LJ median | ours/LJ median without the 5 noisiest overall | dropped here |
|---|---|---|---|---|---|---|
| awfy (14) | 0.277 | 0.277 | 1.308 (14) | 1.158 | 1.305 | 2 |
| apps (5) | 0.589 | 0.592 | 1.557 (5) | 1.341 | 1.418 | 2 |
| programs (25) | 0.237 | 0.238 | 1.000 (25) | 1.010 | 0.981 | 1 |
| jit-trace (29) | 0.081 | 0.080 | 1.021 (29) | 1.015 | 1.015 | 0 |
| lib (95) | 0.287 | 0.286 | 1.194 (91) | 1.193 | 1.193 | 0 |
| real (5) | 0.504 | 0.506 | 1.380 (5) | 1.356 | 1.356 | 0 |
| **all** (173) | 0.233 | 0.233 | 1.155 (169) | 1.138 | 1.142 | |

## every program against the cde96490 binary (bplain)

| program | bplain Mcyc | plain Mcyc | cycles min | cycles median | instructions |
|---|---|---|---|---|---|
| awfy/bounce | 63.3 | 63.2 | 0.998 | 0.996 | 0.998 |
| awfy/cd | 241.1 | 235.6 | 0.977 | 0.980 | 0.986 |
| awfy/deltablue | 104.5 | 100.0 | 0.957 | 0.963 | 0.978 |
| awfy/havlak | 158.4 | 156.5 | 0.988 | 0.981 | 0.983 |
| awfy/json | 160.7 | 158.2 | 0.984 | 0.979 | 0.992 |
| awfy/list | 139.2 | 139.6 | 1.002 | 1.001 | 0.999 |
| awfy/mandelbrot | 276.5 | 276.8 | 1.001 | 0.999 | 1.000 |
| awfy/nbody | 125.3 | 124.1 | 0.990 | 0.996 | 0.999 |
| awfy/permute | 49.1 | 49.0 | 0.999 | 1.001 | 0.999 |
| awfy/queens | 103.9 | 103.1 | 0.993 | 0.989 | 0.996 |
| awfy/richards | 55.0 | 54.1 | 0.985 | 0.983 | 0.991 |
| awfy/sieve | 131.7 | 133.1 | 1.011 | 0.998 | 1.000 |
| awfy/storage | 122.6 | 122.6 | 1.000 | 0.994 | 1.000 |
| awfy/towers | 43.6 | 42.7 | 0.980 | 0.983 | 0.994 |
| **awfy geomean (14)** | | | **0.990** | **0.989** | **0.994** |
| apps/events | 170.6 | 173.6 | 1.018 | 1.017 | 1.000 |
| apps/expr_lang | 154.2 | 150.0 | 0.972 | 0.981 | 0.996 |
| apps/lru_cache | 190.5 | 191.0 | 1.003 | 1.009 | 0.998 |
| apps/markdown | 156.4 | 165.8 | 1.060 | 1.054 | 0.999 |
| apps/raytrace | 152.9 | 148.7 | 0.972 | 0.964 | 0.973 |
| **apps geomean (5)** | | | **1.004** | **1.005** | **0.993** |
| programs/binary_trees | 1989.8 | 1965.8 | 0.988 | 0.995 | 0.999 |
| programs/closures | 504.2 | 493.4 | 0.979 | 0.986 | 1.000 |
| programs/coroutines | 1944.9 | 1893.1 | 0.973 | 0.981 | 1.000 |
| programs/fannkuch | 1159.5 | 1159.0 | 1.000 | 0.999 | 1.000 |
| programs/fasta | 1592.6 | 1589.6 | 0.998 | 0.995 | 0.998 |
| programs/fib | 1422.7 | 1426.5 | 1.003 | 1.001 | 1.000 |
| programs/gc_churn | 2230.1 | 2215.7 | 0.994 | 0.996 | 1.000 |
| programs/heapsort | 1314.9 | 1302.0 | 0.990 | 1.008 | 1.000 |
| programs/json | 2074.5 | 2009.4 | 0.969 | 0.974 | 0.962 |
| programs/knucleotide | 1424.9 | 1422.2 | 0.998 | 0.996 | 1.000 |
| programs/load_big | 779.9 | 781.1 | 1.002 | 0.998 | 1.000 |
| programs/loops_arith | 543.7 | 539.8 | 0.993 | 1.000 | 1.000 |
| programs/mandelbrot | 556.8 | 556.7 | 1.000 | 1.000 | 1.000 |
| programs/matmul | 443.1 | 439.9 | 0.993 | 1.000 | 1.000 |
| programs/metamethods | 738.1 | 740.9 | 1.004 | 0.997 | 1.000 |
| programs/nbody | 525.9 | 529.2 | 1.006 | 1.002 | 1.000 |
| programs/oop_methods | 153.8 | 153.9 | 1.000 | 1.000 | 1.000 |
| programs/pcall_error | 2045.6 | 2177.4 | 1.064 | 1.061 | 0.992 |
| programs/queens | 1906.6 | 1903.5 | 0.998 | 0.998 | 1.000 |
| programs/sieve | 605.6 | 603.3 | 0.996 | 1.001 | 1.000 |
| programs/spectral_norm | 229.4 | 229.3 | 0.999 | 1.000 | 1.000 |
| programs/string_ops | 1579.4 | 1469.9 | 0.931 | 1.021 | 0.986 |
| programs/table_array | 364.3 | 360.0 | 0.988 | 0.990 | 1.000 |
| programs/table_hash | 621.2 | 705.0 | 1.135 | 0.919 | 1.000 |
| programs/varargs | 1305.9 | 1313.1 | 1.006 | 0.999 | 1.000 |
| **programs geomean (25)** | | | **1.000** | **0.996** | **0.997** |
| jit-trace/alloc_ctor | 249.3 | 245.6 | 0.985 | 0.985 | 1.000 |
| jit-trace/array_rec | 1809.1 | 1808.9 | 1.000 | 1.000 | 1.000 |
| jit-trace/dot | 1236.1 | 1236.0 | 1.000 | 1.000 | 1.000 |
| jit-trace/field_global | 534.9 | 535.3 | 1.001 | 0.999 | 1.000 |
| jit-trace/field_method | 752.4 | 750.9 | 0.998 | 0.999 | 1.000 |
| jit-trace/field_rw | 1072.2 | 1073.2 | 1.001 | 1.001 | 1.000 |
| jit-trace/float_loop | 2794.2 | 2789.6 | 0.998 | 0.999 | 1.000 |
| jit-trace/float_poly | 1833.9 | 1833.6 | 1.000 | 1.000 | 1.000 |
| jit-trace/inline_2 | 372.0 | 371.4 | 0.998 | 0.999 | 1.000 |
| jit-trace/inline_leaf | 515.0 | 513.2 | 0.997 | 0.997 | 1.000 |
| jit-trace/int_rec2 | 976.8 | 976.5 | 1.000 | 0.999 | 1.000 |
| jit-trace/int_sum | 990.6 | 994.5 | 1.004 | 1.003 | 1.000 |
| jit-trace/licm_expr | 471.8 | 474.2 | 1.005 | 1.003 | 1.000 |
| jit-trace/multret | 154.3 | 153.2 | 0.993 | 0.994 | 0.997 |
| jit-trace/oop_inherit | 57.2 | 57.3 | 1.001 | 1.008 | 1.000 |
| jit-trace/oop_override | 124.7 | 124.5 | 0.998 | 0.999 | 1.000 |
| jit-trace/rec_loop | 143.2 | 141.9 | 0.991 | 0.991 | 0.991 |
| jit-trace/short_inner | 1802.7 | 1802.8 | 1.000 | 1.000 | 1.000 |
| jit-trace/sink_tuple | 12.8 | 12.8 | 0.996 | 0.993 | 0.999 |
| jit-trace/sink_vec | 12.0 | 11.7 | 0.975 | 0.996 | 1.000 |
| jit-trace/str_concat | 612.0 | 612.7 | 1.001 | 1.001 | 1.000 |
| jit-trace/tail_loop | 555.9 | 556.2 | 1.001 | 0.998 | 0.998 |
| jit-trace/tfor_loops | 533.7 | 535.4 | 1.003 | 1.003 | 1.000 |
| jit-trace/type_entry | 1829.8 | 1826.1 | 0.998 | 1.001 | 1.000 |
| jit-trace/type_entry_stable | 1766.5 | 1779.1 | 1.007 | 1.002 | 1.000 |
| jit-trace/type_late | 1541.7 | 1541.4 | 1.000 | 1.001 | 1.000 |
| jit-trace/type_late_stable | 1543.3 | 1543.1 | 1.000 | 1.000 | 1.000 |
| jit-trace/upval_acc | 1476.4 | 1476.7 | 1.000 | 1.000 | 1.000 |
| jit-trace/upval_inv | 2137.2 | 2134.9 | 0.999 | 1.000 | 1.000 |
| **jit-trace geomean (29)** | | | **0.998** | **0.999** | **0.999** |
| lib/assert_ok | 22.4 | 22.3 | 0.993 | 0.998 | 1.000 |
| lib/byte | 70.1 | 70.3 | 1.003 | 0.996 | 1.000 |
| lib/byte_multi | 47.2 | 47.0 | 0.996 | 0.992 | 1.000 |
| lib/char | 256.2 | 256.8 | 1.003 | 1.008 | 1.000 |
| lib/co_wrap | 244.5 | 242.3 | 0.991 | 0.993 | 1.001 |
| lib/co_yield | 255.6 | 249.8 | 0.977 | 0.972 | 1.000 |
| lib/concat_op | 145.7 | 145.3 | 0.997 | 0.997 | 1.000 |
| lib/concat_op_str | 139.3 | 142.2 | 1.021 | 1.001 | 1.000 |
| lib/error_tbl | 273.1 | 276.8 | 1.014 | 1.007 | 1.000 |
| lib/find_cap | 256.1 | 254.8 | 0.995 | 1.001 | 1.000 |
| lib/find_pat | 267.8 | 260.4 | 0.972 | 1.008 | 1.000 |
| lib/find_plain | 253.6 | 266.5 | 1.051 | 1.057 | 1.000 |
| lib/fmt_d | 123.8 | 124.0 | 1.002 | 1.005 | 1.000 |
| lib/fmt_f | 114.4 | 101.7 | 0.889 | 0.894 | 1.000 |
| lib/fmt_g | 132.4 | 118.2 | 0.893 | 0.899 | 1.000 |
| lib/fmt_mixed | 138.9 | 119.4 | 0.860 | 0.855 | 1.000 |
| lib/fmt_q | 183.5 | 185.2 | 1.009 | 1.008 | 1.000 |
| lib/fmt_s | 7.9 | 7.9 | 1.001 | 0.996 | 0.999 |
| lib/fmt_x_pad | 184.0 | 171.6 | 0.932 | 0.936 | 1.000 |
| lib/getmt | 20.3 | 20.3 | 1.001 | 0.997 | 1.000 |
| lib/gmatch_words | 274.5 | 254.6 | 0.928 | 0.924 | 0.992 |
| lib/gsub_fn | 215.8 | 214.2 | 0.993 | 0.989 | 1.000 |
| lib/gsub_str | 202.6 | 205.5 | 1.014 | 1.015 | 1.000 |
| lib/gsub_tbl | 218.8 | 219.2 | 1.002 | 1.002 | 1.000 |
| lib/it_ipairs | 232.4 | 232.4 | 1.000 | 1.001 | 1.000 |
| lib/it_next | 264.7 | 265.6 | 1.003 | 1.004 | 1.000 |
| lib/it_next_empty | 262.7 | 263.7 | 1.004 | 1.010 | 1.000 |
| lib/it_pairs | 270.3 | 273.3 | 1.011 | 1.002 | 1.000 |
| lib/it_pairs_hash | 254.4 | 254.4 | 1.000 | 1.002 | 1.000 |
| lib/len_op | 71.5 | 71.0 | 0.994 | 1.001 | 1.000 |
| lib/len_tbl | 293.4 | 291.9 | 0.995 | 0.995 | 1.000 |
| lib/load_small | 226.9 | 221.1 | 0.975 | 0.973 | 1.000 |
| lib/lower | 242.6 | 224.8 | 0.926 | 0.940 | 0.925 |
| lib/m_abs | 268.1 | 268.0 | 0.999 | 1.000 | 1.000 |
| lib/m_floor | 272.6 | 272.8 | 1.001 | 1.002 | 1.000 |
| lib/m_fmod | 217.6 | 219.7 | 1.009 | 1.010 | 1.000 |
| lib/m_max | 266.3 | 266.6 | 1.001 | 1.002 | 1.000 |
| lib/m_min | 254.7 | 254.7 | 1.000 | 1.000 | 1.000 |
| lib/m_pow_op | 275.2 | 275.1 | 1.000 | 1.000 | 1.000 |
| lib/m_random | 234.0 | 235.1 | 1.005 | 1.003 | 1.000 |
| lib/m_random_f | 261.2 | 264.7 | 1.014 | 1.013 | 1.000 |
| lib/m_sin | 271.1 | 271.0 | 1.000 | 1.000 | 1.000 |
| lib/m_sqrt | 275.4 | 273.3 | 0.992 | 0.998 | 1.000 |
| lib/m_tointeger | 272.7 | 272.7 | 1.000 | 0.999 | 1.000 |
| lib/m_type | 211.2 | 211.0 | 0.999 | 0.999 | 1.000 |
| lib/m_ult | 279.5 | 277.9 | 0.994 | 0.994 | 1.000 |
| lib/match | 265.9 | 241.4 | 0.908 | 0.905 | 0.990 |
| lib/os_clock | 118.4 | 119.7 | 1.011 | 1.017 | 1.000 |
| lib/pcall_err | 215.0 | 214.2 | 0.996 | 1.001 | 1.005 |
| lib/pcall_ok | 32.9 | 32.5 | 0.988 | 0.999 | 1.000 |
| lib/rawget_set | 21.8 | 22.0 | 1.008 | 0.995 | 1.000 |
| lib/rep | 241.2 | 240.1 | 0.995 | 0.993 | 1.000 |
| lib/rep_sep | 233.3 | 231.6 | 0.993 | 0.992 | 1.000 |
| lib/reverse | 303.3 | 254.0 | 0.837 | 0.818 | 0.925 |
| lib/sel_count | 272.9 | 272.5 | 0.999 | 1.000 | 1.000 |
| lib/sel_loop | 272.8 | 270.4 | 0.991 | 0.996 | 1.000 |
| lib/sel_n | 401.1 | 402.1 | 1.002 | 0.963 | 1.000 |
| lib/setmt | 201.2 | 198.8 | 0.988 | 0.987 | 1.000 |
| lib/str_eq | 185.8 | 185.9 | 1.001 | 0.999 | 1.000 |
| lib/str_eq_dyn | 213.6 | 213.6 | 1.000 | 1.000 | 1.000 |
| lib/str_eq_long | 43.9 | 43.9 | 0.999 | 0.998 | 1.000 |
| lib/str_idx_concat_key | 275.9 | 275.1 | 0.997 | 0.997 | 1.000 |
| lib/str_lt | 115.1 | 115.9 | 1.007 | 1.011 | 1.000 |
| lib/str_lt_short | 115.2 | 115.3 | 1.001 | 1.004 | 1.000 |
| lib/str_method_len | 59.1 | 59.0 | 0.998 | 1.009 | 0.999 |
| lib/string_key_const | 221.6 | 227.6 | 1.027 | 1.000 | 1.000 |
| lib/string_key_dyn | 240.3 | 242.2 | 1.008 | 1.000 | 1.000 |
| lib/sub | 261.9 | 262.8 | 1.004 | 1.004 | 1.000 |
| lib/sub_1char | 194.6 | 194.4 | 0.999 | 0.996 | 1.000 |
| lib/tconcat | 140.6 | 141.2 | 1.005 | 0.996 | 1.000 |
| lib/tconcat_num | 174.2 | 174.8 | 1.004 | 1.012 | 1.000 |
| lib/tfront_1k | 80.2 | 80.7 | 1.006 | 1.000 | 1.000 |
| lib/tinsert_append | 243.7 | 241.9 | 0.993 | 0.995 | 1.000 |
| lib/tinsert_front | 246.9 | 242.8 | 0.983 | 0.983 | 1.000 |
| lib/tinsert_idx | 269.1 | 269.7 | 1.002 | 0.997 | 1.000 |
| lib/tmove | 277.5 | 277.6 | 1.000 | 1.002 | 1.000 |
| lib/tmove_1k | 95.5 | 94.3 | 0.988 | 0.989 | 1.000 |
| lib/tonumber_bad | 254.8 | 262.9 | 1.032 | 1.029 | 1.000 |
| lib/tonumber_base | 267.6 | 266.0 | 0.994 | 0.993 | 1.000 |
| lib/tonumber_flt | 272.8 | 271.6 | 0.996 | 1.000 | 1.000 |
| lib/tonumber_int | 269.3 | 265.8 | 0.987 | 0.987 | 1.000 |
| lib/tonumber_num | 230.2 | 226.3 | 0.983 | 0.957 | 1.000 |
| lib/tostring_flt | 129.4 | 129.5 | 1.001 | 0.988 | 1.000 |
| lib/tostring_int | 140.5 | 140.3 | 0.999 | 0.996 | 1.000 |
| lib/tostring_tbl | 258.7 | 263.4 | 1.018 | 1.018 | 1.000 |
| lib/tpack | 170.1 | 166.8 | 0.981 | 1.014 | 1.000 |
| lib/tremove_front | 265.8 | 263.2 | 0.990 | 1.000 | 1.000 |
| lib/tremove_pop | 263.4 | 265.9 | 1.009 | 1.014 | 1.000 |
| lib/tsort_cmp | 266.9 | 265.9 | 0.997 | 0.997 | 1.000 |
| lib/tsort_int | 264.5 | 259.5 | 0.981 | 0.981 | 1.000 |
| lib/tsort_str | 255.7 | 250.9 | 0.981 | 0.983 | 0.999 |
| lib/tunpack | 263.5 | 265.7 | 1.008 | 1.016 | 1.000 |
| lib/type_call | 189.7 | 190.7 | 1.005 | 1.003 | 1.000 |
| lib/upper | 271.1 | 238.3 | 0.879 | 0.881 | 0.925 |
| lib/utf8_len | 270.1 | 267.8 | 0.991 | 0.991 | 1.000 |
| **lib geomean (95)** | | | **0.989** | **0.989** | **0.997** |
| real/game_loop | 36.3 | 35.6 | 0.981 | 0.977 | 0.992 |
| real/json_roundtrip | 607.0 | 588.5 | 0.970 | 0.968 | 0.937 |
| real/lua_interp | 115.6 | 117.6 | 1.017 | 1.027 | 0.998 |
| real/template_engine | 831.0 | 836.6 | 1.007 | 1.007 | 0.996 |
| real/wordfreq | 163.4 | 157.6 | 0.964 | 0.967 | 0.971 |
| **real geomean (5)** | | | **0.988** | **0.989** | **0.979** |

## top ours/LJ ratios

| rank | program | ours/LJ min | ours/LJ median |
|---|---|---|---|
| 1 | lib/find_plain | 3.96 | 3.99 |
| 2 | apps/raytrace | 3.66 | 3.63 |
| 3 | lib/it_next_empty | 3.64 | 3.67 |
| 4 | lib/sub | 3.27 | 3.26 |
| 5 | lib/m_random | 3.21 | 3.20 |
| 6 | lib/fmt_x_pad | 3.17 | 3.17 |
| 7 | programs/fib | 3.10 | 3.37 |
| 8 | lib/m_random_f | 2.94 | 2.95 |
| 9 | lib/tostring_tbl | 2.83 | 2.81 |
| 10 | lib/tonumber_int | 2.72 | 2.72 |
| 11 | lib/rep | 2.65 | 2.64 |
| 12 | lib/tmove | 2.62 | 2.69 |
| 13 | lib/fmt_s | 2.55 | 2.45 |
| 14 | jit-trace/tail_loop | 2.49 | 2.50 |
| 15 | lib/char | 2.45 | 2.37 |
| 16 | lib/co_yield | 2.44 | 2.45 |
| 17 | lib/byte_multi | 2.36 | 2.35 |
| 18 | lib/getmt | 2.29 | 2.26 |
| 19 | lib/reverse | 2.24 | 2.22 |
| 20 | lib/rep_sep | 2.23 | 2.24 |
| 21 | lib/type_call | 2.23 | 2.26 |
| 22 | awfy/cd | 2.19 | 1.90 |
| 23 | programs/coroutines | 2.16 | 2.20 |
| 24 | awfy/json | 2.16 | 2.13 |
| 25 | lib/it_pairs_hash | 2.15 | 2.20 |
| 26 | lib/it_pairs | 2.15 | 2.17 |
| 27 | lib/byte | 2.13 | 2.12 |
| 28 | jit-trace/type_entry | 2.11 | 1.88 |
| 29 | programs/knucleotide | 2.08 | 2.07 |
| 30 | jit-trace/short_inner | 2.05 | 2.05 |

programs with min ratio > 2: 34 of 169; with median ratio > 2: 30

## regressions over 3% (cycles min or instructions) against bplain, default
```
apps/markdown: cycles min 1.060 median 1.054 instructions 0.999 (156.4 -> 165.8 Mcyc)
programs/pcall_error: cycles min 1.064 median 1.061 instructions 0.992 (2045.6 -> 2177.4 Mcyc)
programs/table_hash: cycles min 1.135 median 0.919 instructions 1.000 (621.2 -> 705.0 Mcyc)
lib/find_plain: cycles min 1.051 median 1.057 instructions 1.000 (253.6 -> 266.5 Mcyc)
lib/tonumber_bad: cycles min 1.032 median 1.029 instructions 1.000 (254.8 -> 262.9 Mcyc)
(173 programs compared)
```
## regressions over 3% against bplain, warm0
```
apps/markdown: cycles min 1.059 median 1.054 instructions 0.999 (156.8 -> 166.1 Mcyc)
programs/table_hash: cycles min 1.048 median 1.010 instructions 1.000 (872.2 -> 913.8 Mcyc)
lib/find_plain: cycles min 1.045 median 1.043 instructions 1.000 (252.9 -> 264.3 Mcyc)
lib/tconcat: cycles min 1.032 median 1.017 instructions 1.003 (139.5 -> 143.9 Mcyc)
(173 programs compared)
```
## movers

top 10 improvements (plain / bplain):

| program | Mcyc base -> this | cycles min | cycles median | instructions |
|---|---|---|---|---|
| lib/reverse | 303.3 -> 254.0 | 0.837 | 0.818 | 0.925 |
| lib/fmt_mixed | 138.9 -> 119.4 | 0.860 | 0.855 | 1.000 |
| lib/upper | 271.1 -> 238.3 | 0.879 | 0.881 | 0.925 |
| lib/fmt_f | 114.4 -> 101.7 | 0.889 | 0.894 | 1.000 |
| lib/fmt_g | 132.4 -> 118.2 | 0.893 | 0.899 | 1.000 |
| lib/match | 265.9 -> 241.4 | 0.908 | 0.905 | 0.990 |
| lib/lower | 242.6 -> 224.8 | 0.926 | 0.940 | 0.925 |
| lib/gmatch_words | 274.5 -> 254.6 | 0.928 | 0.924 | 0.992 |
| programs/string_ops | 1579.4 -> 1469.9 | 0.931 | 1.021 | 0.986 |
| lib/fmt_x_pad | 184.0 -> 171.6 | 0.932 | 0.936 | 1.000 |

top 10 regressions (plain / bplain):

| program | Mcyc base -> this | cycles min | cycles median | instructions |
|---|---|---|---|---|
| programs/table_hash | 621.2 -> 705.0 | 1.135 | 0.919 | 1.000 |
| programs/pcall_error | 2045.6 -> 2177.4 | 1.064 | 1.061 | 0.992 |
| apps/markdown | 156.4 -> 165.8 | 1.060 | 1.054 | 0.999 |
| lib/find_plain | 253.6 -> 266.5 | 1.051 | 1.057 | 1.000 |
| lib/tonumber_bad | 254.8 -> 262.9 | 1.032 | 1.029 | 1.000 |
| lib/string_key_const | 221.6 -> 227.6 | 1.027 | 1.000 | 1.000 |
| lib/concat_op_str | 139.3 -> 142.2 | 1.021 | 1.001 | 1.000 |
| lib/tostring_tbl | 258.7 -> 263.4 | 1.018 | 1.018 | 1.000 |
| apps/events | 170.6 -> 173.6 | 1.018 | 1.017 | 1.000 |
| real/lua_interp | 115.6 -> 117.6 | 1.017 | 1.027 | 0.998 |

