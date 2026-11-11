# Post-M full benchmark, pgo build: per-program tables (default mode, cycles:u, min of 11; 4-way parallel run)

## per-set geomeans against the cde96490 binary (bpgo), default

| set (n) | pgo/bpgo cycles min | median | instructions | worst program (min) | best program (min) |
|---|---|---|---|---|---|
| awfy (14) | 0.989 | 0.987 | 0.993 | sieve 1.008 | deltablue 0.954 |
| apps (5) | 0.991 | 0.990 | 0.992 | markdown 1.042 | raytrace 0.951 |
| programs (25) | 0.996 | 1.007 | 0.997 | closures 1.016 | pcall_error 0.944 |
| jit-trace (29) | 1.000 | 0.999 | 0.999 | field_global 1.019 | rec_loop 0.986 |
| lib (95) | 0.996 | 0.995 | 0.997 | error_tbl 1.092 | pcall_err 0.880 |
| real (5) | 0.985 | 0.985 | 0.975 | template_engine 1.030 | json_roundtrip 0.940 |
| **all (173)** | **0.995** | 0.997 | 0.996 | | |

## warm0

| set (n) | pgo/bpgo cycles min | median | instructions | worst program (min) | best program (min) |
|---|---|---|---|---|---|
| awfy (14) | 0.987 | 0.989 | 0.994 | nbody 1.004 | havlak 0.954 |
| apps (5) | 0.990 | 0.993 | 0.992 | markdown 1.028 | raytrace 0.954 |
| programs (25) | 0.995 | 0.993 | 0.997 | load_big 1.025 | json 0.961 |
| jit-trace (29) | 1.000 | 1.001 | 0.999 | alloc_ctor 1.014 | sink_tuple 0.991 |
| lib (95) | 0.996 | 0.996 | 0.997 | error_tbl 1.085 | pcall_err 0.881 |
| real (5) | 0.987 | 0.984 | 0.976 | template_engine 1.032 | json_roundtrip 0.942 |
| **all (173)** | **0.995** | 0.995 | 0.996 | | |

## per-set geomeans against PUC / LuaJIT (default)

5 noisiest programs overall (max of median/min of ours or LuaJIT): awfy/permute 4.14, apps/events 1.65, programs/table_hash 1.51, apps/lru_cache 1.29, awfy/cd 1.17

| set (n) | ours/PUC min | ours/PUC median | ours/LJ min (n with LJ) | ours/LJ median | ours/LJ median without the 5 noisiest overall | dropped here |
|---|---|---|---|---|---|---|
| awfy (14) | 0.264 | 0.264 | 1.248 (14) | 1.104 | 1.244 | 2 |
| apps (5) | 0.522 | 0.527 | 1.379 (5) | 1.193 | 1.238 | 2 |
| programs (25) | 0.220 | 0.222 | 0.931 (25) | 0.941 | 0.912 | 1 |
| jit-trace (29) | 0.080 | 0.080 | 1.014 (29) | 1.006 | 1.006 | 0 |
| lib (95) | 0.263 | 0.262 | 1.088 (91) | 1.087 | 1.087 | 0 |
| real (5) | 0.451 | 0.451 | 1.233 (5) | 1.210 | 1.210 | 0 |
| **all** (173) | 0.218 | 0.218 | 1.074 (169) | 1.058 | 1.062 | |

## every program against the cde96490 binary (bpgo)

| program | bpgo Mcyc | pgo Mcyc | cycles min | cycles median | instructions |
|---|---|---|---|---|---|
| awfy/bounce | 62.3 | 62.2 | 0.998 | 0.995 | 0.998 |
| awfy/cd | 222.8 | 218.5 | 0.981 | 0.976 | 0.985 |
| awfy/deltablue | 97.5 | 93.1 | 0.954 | 0.956 | 0.976 |
| awfy/havlak | 141.1 | 140.6 | 0.997 | 0.962 | 0.980 |
| awfy/json | 138.9 | 137.5 | 0.990 | 0.994 | 0.991 |
| awfy/list | 134.9 | 134.8 | 1.000 | 0.998 | 0.999 |
| awfy/mandelbrot | 276.4 | 276.5 | 1.000 | 1.000 | 1.000 |
| awfy/nbody | 123.9 | 123.2 | 0.994 | 0.998 | 0.999 |
| awfy/permute | 48.6 | 48.6 | 1.001 | 1.002 | 0.999 |
| awfy/queens | 101.7 | 100.2 | 0.986 | 0.991 | 0.996 |
| awfy/richards | 54.0 | 53.2 | 0.985 | 0.988 | 0.991 |
| awfy/sieve | 131.8 | 132.8 | 1.008 | 0.996 | 1.000 |
| awfy/storage | 108.3 | 107.0 | 0.989 | 0.984 | 1.000 |
| awfy/towers | 43.3 | 42.0 | 0.970 | 0.979 | 0.994 |
| **awfy geomean (14)** | | | **0.989** | **0.987** | **0.993** |
| apps/events | 157.2 | 157.7 | 1.003 | 1.014 | 1.000 |
| apps/expr_lang | 136.9 | 131.5 | 0.960 | 0.959 | 0.995 |
| apps/lru_cache | 173.5 | 173.3 | 0.999 | 0.985 | 0.998 |
| apps/markdown | 142.7 | 148.7 | 1.042 | 1.030 | 0.999 |
| apps/raytrace | 131.4 | 125.0 | 0.951 | 0.963 | 0.970 |
| **apps geomean (5)** | | | **0.991** | **0.990** | **0.992** |
| programs/binary_trees | 1720.3 | 1712.8 | 0.996 | 0.996 | 0.990 |
| programs/closures | 455.3 | 462.7 | 1.016 | 1.016 | 1.000 |
| programs/coroutines | 1613.8 | 1613.3 | 1.000 | 1.006 | 1.000 |
| programs/fannkuch | 1153.7 | 1157.9 | 1.004 | 1.002 | 1.000 |
| programs/fasta | 1510.4 | 1497.3 | 0.991 | 0.996 | 0.998 |
| programs/fib | 1424.6 | 1426.5 | 1.001 | 1.122 | 1.000 |
| programs/gc_churn | 1771.0 | 1769.3 | 0.999 | 0.989 | 1.000 |
| programs/heapsort | 1274.4 | 1289.3 | 1.012 | 1.000 | 1.000 |
| programs/json | 1713.3 | 1620.5 | 0.946 | 0.945 | 0.955 |
| programs/knucleotide | 1348.3 | 1346.6 | 0.999 | 1.002 | 1.000 |
| programs/load_big | 630.6 | 637.7 | 1.011 | 1.013 | 1.000 |
| programs/loops_arith | 542.1 | 547.5 | 1.010 | 1.000 | 1.000 |
| programs/mandelbrot | 556.5 | 556.5 | 1.000 | 1.000 | 1.000 |
| programs/matmul | 441.1 | 436.0 | 0.989 | 0.999 | 1.000 |
| programs/metamethods | 707.6 | 711.8 | 1.006 | 1.001 | 1.000 |
| programs/nbody | 528.4 | 526.1 | 0.996 | 1.001 | 1.000 |
| programs/oop_methods | 153.5 | 152.3 | 0.992 | 0.996 | 1.000 |
| programs/pcall_error | 1913.3 | 1806.8 | 0.944 | 0.955 | 0.991 |
| programs/queens | 1904.5 | 1903.1 | 0.999 | 0.998 | 1.000 |
| programs/sieve | 637.8 | 637.1 | 0.999 | 1.002 | 1.000 |
| programs/spectral_norm | 229.0 | 228.6 | 0.998 | 0.999 | 1.000 |
| programs/string_ops | 1319.0 | 1335.5 | 1.013 | 1.086 | 0.984 |
| programs/table_array | 308.5 | 309.6 | 1.003 | 1.003 | 1.000 |
| programs/table_hash | 685.8 | 671.7 | 0.979 | 1.071 | 1.000 |
| programs/varargs | 1089.0 | 1094.5 | 1.005 | 1.002 | 1.000 |
| **programs geomean (25)** | | | **0.996** | **1.007** | **0.997** |
| jit-trace/alloc_ctor | 220.7 | 222.2 | 1.007 | 1.003 | 1.000 |
| jit-trace/array_rec | 1806.5 | 1806.1 | 1.000 | 1.000 | 1.000 |
| jit-trace/dot | 1235.5 | 1235.4 | 1.000 | 1.000 | 1.000 |
| jit-trace/field_global | 526.5 | 536.7 | 1.019 | 1.000 | 1.000 |
| jit-trace/field_method | 746.8 | 750.6 | 1.005 | 1.000 | 1.000 |
| jit-trace/field_rw | 1069.6 | 1069.7 | 1.000 | 0.999 | 1.000 |
| jit-trace/float_loop | 2793.6 | 2789.5 | 0.999 | 0.999 | 1.000 |
| jit-trace/float_poly | 1834.7 | 1832.3 | 0.999 | 1.000 | 1.000 |
| jit-trace/inline_2 | 371.4 | 371.1 | 0.999 | 0.999 | 1.000 |
| jit-trace/inline_leaf | 519.2 | 517.8 | 0.997 | 0.995 | 1.000 |
| jit-trace/int_rec2 | 976.7 | 977.5 | 1.001 | 1.000 | 1.000 |
| jit-trace/int_sum | 987.6 | 989.0 | 1.001 | 1.000 | 1.000 |
| jit-trace/licm_expr | 472.8 | 469.0 | 0.992 | 1.011 | 1.000 |
| jit-trace/multret | 153.2 | 152.7 | 0.997 | 0.997 | 0.997 |
| jit-trace/oop_inherit | 56.8 | 56.6 | 0.997 | 0.999 | 1.000 |
| jit-trace/oop_override | 124.0 | 124.5 | 1.004 | 1.001 | 1.000 |
| jit-trace/rec_loop | 142.9 | 141.0 | 0.986 | 0.989 | 0.991 |
| jit-trace/short_inner | 1803.6 | 1803.2 | 1.000 | 1.000 | 1.000 |
| jit-trace/sink_tuple | 12.4 | 12.5 | 1.004 | 0.993 | 0.999 |
| jit-trace/sink_vec | 11.9 | 11.7 | 0.987 | 0.998 | 1.000 |
| jit-trace/str_concat | 589.0 | 588.6 | 0.999 | 1.002 | 1.000 |
| jit-trace/tail_loop | 556.5 | 555.8 | 0.999 | 0.999 | 0.998 |
| jit-trace/tfor_loops | 534.1 | 534.3 | 1.000 | 1.002 | 1.000 |
| jit-trace/type_entry | 1824.2 | 1830.2 | 1.003 | 1.002 | 1.000 |
| jit-trace/type_entry_stable | 1770.0 | 1770.3 | 1.000 | 0.998 | 1.000 |
| jit-trace/type_late | 1542.0 | 1540.8 | 0.999 | 1.000 | 1.000 |
| jit-trace/type_late_stable | 1542.2 | 1541.5 | 1.000 | 0.999 | 1.000 |
| jit-trace/upval_acc | 1476.2 | 1474.3 | 0.999 | 1.000 | 1.000 |
| jit-trace/upval_inv | 2135.8 | 2136.4 | 1.000 | 1.000 | 1.000 |
| **jit-trace geomean (29)** | | | **1.000** | **0.999** | **0.999** |
| lib/assert_ok | 22.3 | 22.2 | 0.996 | 0.999 | 1.000 |
| lib/byte | 69.9 | 70.0 | 1.001 | 1.001 | 1.000 |
| lib/byte_multi | 46.7 | 46.7 | 0.999 | 1.002 | 1.000 |
| lib/char | 240.4 | 239.2 | 0.995 | 1.018 | 1.000 |
| lib/co_wrap | 219.2 | 206.5 | 0.942 | 0.947 | 1.002 |
| lib/co_yield | 204.6 | 196.9 | 0.962 | 0.957 | 1.000 |
| lib/concat_op | 136.0 | 136.2 | 1.002 | 1.006 | 1.000 |
| lib/concat_op_str | 127.6 | 125.6 | 0.984 | 0.988 | 1.000 |
| lib/error_tbl | 213.3 | 232.9 | 1.092 | 1.022 | 1.001 |
| lib/find_cap | 212.4 | 213.3 | 1.004 | 1.004 | 1.000 |
| lib/find_pat | 207.9 | 208.2 | 1.002 | 1.030 | 1.000 |
| lib/find_plain | 236.2 | 237.7 | 1.006 | 1.023 | 1.000 |
| lib/fmt_d | 113.9 | 114.5 | 1.005 | 1.002 | 1.000 |
| lib/fmt_f | 92.6 | 92.1 | 0.995 | 1.007 | 1.000 |
| lib/fmt_g | 110.0 | 110.7 | 1.006 | 1.006 | 1.000 |
| lib/fmt_mixed | 105.5 | 105.4 | 0.999 | 1.007 | 1.000 |
| lib/fmt_q | 165.4 | 165.9 | 1.003 | 0.997 | 1.000 |
| lib/fmt_s | 7.7 | 7.5 | 0.982 | 0.992 | 0.999 |
| lib/fmt_x_pad | 105.9 | 107.6 | 1.016 | 1.020 | 1.000 |
| lib/getmt | 20.2 | 20.1 | 0.995 | 0.998 | 1.000 |
| lib/gmatch_words | 188.8 | 184.0 | 0.974 | 0.976 | 0.989 |
| lib/gsub_fn | 180.1 | 179.5 | 0.997 | 1.003 | 1.000 |
| lib/gsub_str | 181.4 | 181.7 | 1.001 | 0.994 | 1.000 |
| lib/gsub_tbl | 163.1 | 162.0 | 0.993 | 0.996 | 1.000 |
| lib/it_ipairs | 233.7 | 234.2 | 1.002 | 0.989 | 1.000 |
| lib/it_next | 260.9 | 260.7 | 0.999 | 0.996 | 1.000 |
| lib/it_next_empty | 259.2 | 258.9 | 0.999 | 1.001 | 1.000 |
| lib/it_pairs | 249.4 | 249.8 | 1.001 | 1.000 | 1.000 |
| lib/it_pairs_hash | 255.3 | 252.6 | 0.990 | 1.002 | 1.000 |
| lib/len_op | 70.6 | 71.3 | 1.010 | 1.003 | 1.000 |
| lib/len_tbl | 283.7 | 283.0 | 0.998 | 0.996 | 1.000 |
| lib/load_small | 183.3 | 194.9 | 1.063 | 1.065 | 1.000 |
| lib/lower | 237.6 | 217.4 | 0.915 | 0.927 | 0.924 |
| lib/m_abs | 267.6 | 267.7 | 1.000 | 1.003 | 1.000 |
| lib/m_floor | 272.5 | 272.4 | 1.000 | 0.998 | 1.000 |
| lib/m_fmod | 219.8 | 218.8 | 0.996 | 0.993 | 1.000 |
| lib/m_max | 266.1 | 266.0 | 0.999 | 1.000 | 1.000 |
| lib/m_min | 254.5 | 254.7 | 1.001 | 1.000 | 1.000 |
| lib/m_pow_op | 275.1 | 274.6 | 0.998 | 1.000 | 1.000 |
| lib/m_random | 146.3 | 149.0 | 1.018 | 1.017 | 1.000 |
| lib/m_random_f | 225.0 | 225.1 | 1.001 | 0.995 | 1.000 |
| lib/m_sin | 259.6 | 259.5 | 0.999 | 0.999 | 1.000 |
| lib/m_sqrt | 273.2 | 272.9 | 0.999 | 0.999 | 1.000 |
| lib/m_tointeger | 272.6 | 272.4 | 0.999 | 1.000 | 1.000 |
| lib/m_type | 235.9 | 235.7 | 0.999 | 1.001 | 1.000 |
| lib/m_ult | 284.9 | 285.6 | 1.002 | 1.003 | 1.000 |
| lib/match | 206.7 | 205.1 | 0.993 | 0.937 | 0.989 |
| lib/os_clock | 120.2 | 120.1 | 0.999 | 0.999 | 1.000 |
| lib/pcall_err | 204.7 | 180.1 | 0.880 | 0.879 | 1.007 |
| lib/pcall_ok | 32.8 | 32.3 | 0.986 | 1.008 | 1.000 |
| lib/rawget_set | 21.7 | 21.6 | 0.995 | 1.001 | 1.000 |
| lib/rep | 195.7 | 194.2 | 0.993 | 1.002 | 1.000 |
| lib/rep_sep | 213.3 | 212.9 | 0.998 | 1.000 | 1.000 |
| lib/reverse | 282.8 | 252.8 | 0.894 | 0.871 | 0.924 |
| lib/sel_count | 272.6 | 272.3 | 0.999 | 1.000 | 1.000 |
| lib/sel_loop | 270.0 | 270.2 | 1.001 | 1.002 | 1.000 |
| lib/sel_n | 400.0 | 400.1 | 1.000 | 1.025 | 1.000 |
| lib/setmt | 176.5 | 176.2 | 0.998 | 0.994 | 1.000 |
| lib/str_eq | 185.7 | 185.7 | 1.000 | 1.000 | 1.000 |
| lib/str_eq_dyn | 213.5 | 213.4 | 1.000 | 1.000 | 1.000 |
| lib/str_eq_long | 43.7 | 43.7 | 1.000 | 1.001 | 1.000 |
| lib/str_idx_concat_key | 257.1 | 256.2 | 0.996 | 1.006 | 1.000 |
| lib/str_lt | 133.8 | 133.9 | 1.001 | 1.003 | 1.000 |
| lib/str_lt_short | 113.6 | 113.4 | 0.998 | 1.002 | 1.000 |
| lib/str_method_len | 58.7 | 58.9 | 1.003 | 1.008 | 0.999 |
| lib/string_key_const | 226.2 | 221.6 | 0.980 | 1.000 | 1.000 |
| lib/string_key_dyn | 242.4 | 242.4 | 1.000 | 1.000 | 1.000 |
| lib/sub | 247.8 | 247.7 | 1.000 | 0.998 | 1.000 |
| lib/sub_1char | 176.2 | 176.2 | 1.000 | 1.007 | 1.000 |
| lib/tconcat | 114.3 | 114.3 | 1.000 | 1.003 | 1.000 |
| lib/tconcat_num | 135.1 | 136.0 | 1.006 | 1.003 | 1.000 |
| lib/tfront_1k | 78.2 | 78.2 | 1.000 | 0.995 | 1.000 |
| lib/tinsert_append | 218.7 | 220.1 | 1.006 | 1.009 | 1.000 |
| lib/tinsert_front | 205.9 | 206.5 | 1.003 | 1.007 | 1.000 |
| lib/tinsert_idx | 277.6 | 275.6 | 0.993 | 0.997 | 1.000 |
| lib/tmove | 223.4 | 223.9 | 1.002 | 0.988 | 1.000 |
| lib/tmove_1k | 70.3 | 70.2 | 0.998 | 0.993 | 1.000 |
| lib/tonumber_bad | 242.9 | 244.8 | 1.008 | 1.011 | 1.000 |
| lib/tonumber_base | 261.5 | 261.5 | 1.000 | 0.997 | 1.000 |
| lib/tonumber_flt | 248.7 | 247.8 | 0.996 | 0.997 | 1.000 |
| lib/tonumber_int | 258.9 | 258.6 | 0.999 | 1.001 | 1.000 |
| lib/tonumber_num | 226.9 | 225.7 | 0.995 | 0.914 | 1.000 |
| lib/tostring_flt | 116.1 | 117.3 | 1.010 | 1.008 | 1.000 |
| lib/tostring_int | 133.1 | 133.8 | 1.005 | 1.003 | 1.000 |
| lib/tostring_tbl | 217.7 | 218.5 | 1.003 | 1.001 | 1.000 |
| lib/tpack | 132.9 | 131.6 | 0.990 | 0.981 | 1.000 |
| lib/tremove_front | 233.3 | 233.0 | 0.998 | 0.998 | 1.000 |
| lib/tremove_pop | 203.1 | 201.1 | 0.990 | 0.997 | 1.000 |
| lib/tsort_cmp | 257.6 | 260.7 | 1.012 | 1.011 | 1.000 |
| lib/tsort_int | 222.3 | 222.9 | 1.003 | 1.003 | 1.000 |
| lib/tsort_str | 154.3 | 153.8 | 0.997 | 0.996 | 0.999 |
| lib/tunpack | 218.3 | 218.9 | 1.003 | 1.002 | 1.000 |
| lib/type_call | 197.1 | 197.5 | 1.002 | 1.010 | 1.000 |
| lib/upper | 258.4 | 241.5 | 0.935 | 0.937 | 0.921 |
| lib/utf8_len | 270.5 | 270.8 | 1.001 | 1.000 | 1.000 |
| **lib geomean (95)** | | | **0.996** | **0.995** | **0.997** |
| real/game_loop | 35.5 | 34.6 | 0.976 | 0.977 | 0.992 |
| real/json_roundtrip | 515.2 | 484.5 | 0.940 | 0.944 | 0.928 |
| real/lua_interp | 110.1 | 110.2 | 1.001 | 1.000 | 0.998 |
| real/template_engine | 705.0 | 725.9 | 1.030 | 1.026 | 0.995 |
| real/wordfreq | 140.5 | 137.7 | 0.980 | 0.980 | 0.965 |
| **real geomean (5)** | | | **0.985** | **0.985** | **0.975** |

## top ours/LJ ratios

| rank | program | ours/LJ min | ours/LJ median |
|---|---|---|---|
| 1 | lib/it_next_empty | 3.58 | 3.59 |
| 2 | lib/find_plain | 3.53 | 3.57 |
| 3 | programs/fib | 3.10 | 3.38 |
| 4 | lib/sub | 3.09 | 3.07 |
| 5 | apps/raytrace | 3.08 | 3.09 |
| 6 | lib/tonumber_int | 2.65 | 2.66 |
| 7 | lib/m_random_f | 2.50 | 2.51 |
| 8 | jit-trace/tail_loop | 2.49 | 2.49 |
| 9 | lib/fmt_s | 2.42 | 2.33 |
| 10 | lib/tostring_tbl | 2.35 | 2.33 |
| 11 | lib/byte_multi | 2.35 | 2.34 |
| 12 | lib/type_call | 2.31 | 2.34 |
| 13 | lib/char | 2.28 | 2.22 |
| 14 | lib/getmt | 2.27 | 2.24 |
| 15 | lib/reverse | 2.23 | 2.21 |
| 16 | lib/rep | 2.14 | 2.15 |
| 17 | lib/it_pairs_hash | 2.14 | 2.18 |
| 18 | lib/byte | 2.12 | 2.12 |
| 19 | jit-trace/type_entry | 2.12 | 1.89 |
| 20 | lib/tmove | 2.12 | 2.13 |
| 21 | lib/rep_sep | 2.05 | 2.06 |
| 22 | jit-trace/short_inner | 2.05 | 2.05 |
| 23 | lib/str_eq_long | 2.04 | 2.02 |
| 24 | awfy/cd | 2.03 | 1.74 |
| 25 | lib/m_random | 2.03 | 2.03 |
| 26 | lib/upper | 2.03 | 2.01 |
| 27 | lib/str_lt | 2.02 | 2.02 |
| 28 | lib/fmt_x_pad | 1.99 | 1.99 |
| 29 | lib/lower | 1.97 | 1.99 |
| 30 | programs/knucleotide | 1.97 | 1.97 |

programs with min ratio > 2: 27 of 169; with median ratio > 2: 26

## regressions over 3% (cycles min or instructions) against bpgo, default
```
apps/markdown: cycles min 1.042 median 1.030 instructions 0.999 (142.7 -> 148.7 Mcyc)
lib/error_tbl: cycles min 1.092 median 1.022 instructions 1.001 (213.3 -> 232.9 Mcyc)
lib/load_small: cycles min 1.063 median 1.065 instructions 1.000 (183.3 -> 194.9 Mcyc)
(173 programs compared)
```
## regressions over 3% against bpgo, warm0
```
lib/error_tbl: cycles min 1.085 median 1.041 instructions 1.001 (214.4 -> 232.7 Mcyc)
lib/load_small: cycles min 1.058 median 1.059 instructions 1.000 (182.7 -> 193.4 Mcyc)
real/template_engine: cycles min 1.032 median 1.031 instructions 0.995 (705.1 -> 727.8 Mcyc)
(173 programs compared)
```
## movers

top 10 improvements (pgo / bpgo):

| program | Mcyc base -> this | cycles min | cycles median | instructions |
|---|---|---|---|---|
| lib/pcall_err | 204.7 -> 180.1 | 0.880 | 0.879 | 1.007 |
| lib/reverse | 282.8 -> 252.8 | 0.894 | 0.871 | 0.924 |
| lib/lower | 237.6 -> 217.4 | 0.915 | 0.927 | 0.924 |
| lib/upper | 258.4 -> 241.5 | 0.935 | 0.937 | 0.921 |
| real/json_roundtrip | 515.2 -> 484.5 | 0.940 | 0.944 | 0.928 |
| lib/co_wrap | 219.2 -> 206.5 | 0.942 | 0.947 | 1.002 |
| programs/pcall_error | 1913.3 -> 1806.8 | 0.944 | 0.955 | 0.991 |
| programs/json | 1713.3 -> 1620.5 | 0.946 | 0.945 | 0.955 |
| apps/raytrace | 131.4 -> 125.0 | 0.951 | 0.963 | 0.970 |
| awfy/deltablue | 97.5 -> 93.1 | 0.954 | 0.956 | 0.976 |

top 10 regressions (pgo / bpgo):

| program | Mcyc base -> this | cycles min | cycles median | instructions |
|---|---|---|---|---|
| lib/error_tbl | 213.3 -> 232.9 | 1.092 | 1.022 | 1.001 |
| lib/load_small | 183.3 -> 194.9 | 1.063 | 1.065 | 1.000 |
| apps/markdown | 142.7 -> 148.7 | 1.042 | 1.030 | 0.999 |
| real/template_engine | 705.0 -> 725.9 | 1.030 | 1.026 | 0.995 |
| jit-trace/field_global | 526.5 -> 536.7 | 1.019 | 1.000 | 1.000 |
| lib/m_random | 146.3 -> 149.0 | 1.018 | 1.017 | 1.000 |
| programs/closures | 455.3 -> 462.7 | 1.016 | 1.016 | 1.000 |
| lib/fmt_x_pad | 105.9 -> 107.6 | 1.016 | 1.020 | 1.000 |
| programs/string_ops | 1319.0 -> 1335.5 | 1.013 | 1.086 | 0.984 |
| lib/tsort_cmp | 257.6 -> 260.7 | 1.012 | 1.011 | 1.000 |

