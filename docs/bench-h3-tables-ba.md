### default: plain / bplain
| set (n) | plain/bplain cycles min | median | instructions | worst program (min) | best program (min) |
|---|---|---|---|---|---|
| awfy (14) | 0.968 | 0.968 | 0.981 | havlak 1.011 | queens 0.923 |
| apps (5) | 0.986 | 0.989 | 0.997 | lru_cache 1.012 | markdown 0.959 |
| programs (25) | 0.998 | 0.999 | 0.997 | oop_methods 1.028 | metamethods 0.949 |
| jit-trace (29) | 0.999 | 0.998 | 0.999 | sink_vec 1.020 | oop_override 0.950 |
| lib (95) | 1.000 | 1.000 | 1.000 | co_wrap 1.089 | string_key_const 0.913 |
| real (5) | 0.982 | 0.982 | 0.993 | json_roundtrip 1.010 | game_loop 0.957 |
| **all (173)** | **0.996** | 0.996 | 0.997 | | |

### warm0: plain / bplain
| set (n) | plain/bplain cycles min | median | instructions | worst program (min) | best program (min) |
|---|---|---|---|---|---|
| awfy (14) | 0.972 | 0.978 | 0.981 | richards 1.023 | queens 0.918 |
| apps (5) | 0.995 | 0.994 | 0.997 | expr_lang 1.011 | markdown 0.974 |
| programs (25) | 0.998 | 1.001 | 0.997 | binary_trees 1.027 | nbody 0.928 |
| jit-trace (29) | 1.000 | 1.000 | 0.999 | field_global 1.033 | oop_override 0.952 |
| lib (95) | 1.000 | 1.003 | 1.000 | co_wrap 1.081 | string_key_const 0.916 |
| real (5) | 0.986 | 0.988 | 0.995 | json_roundtrip 1.004 | game_loop 0.965 |
| **all (173)** | **0.997** | 0.999 | 0.997 | | |

### worst (>3%) default: plain / bplain
lib/co_wrap: cycles min 1.089 median 1.085 instructions 1.000 (240.3 -> 261.5 Mcyc)
lib/fmt_mixed: cycles min 1.046 median 1.040 instructions 1.000 (123.2 -> 128.9 Mcyc)
lib/m_type: cycles min 1.084 median 1.085 instructions 1.000 (211.5 -> 229.2 Mcyc)
lib/match: cycles min 1.033 median 1.016 instructions 1.000 (246.7 -> 254.8 Mcyc)
(173 programs compared)

### worst (>3%) warm0: plain / bplain
jit-trace/field_global: cycles min 1.033 median 1.001 instructions 1.000 (526.0 -> 543.2 Mcyc)
lib/co_wrap: cycles min 1.081 median 1.088 instructions 1.000 (243.1 -> 262.8 Mcyc)
lib/fmt_mixed: cycles min 1.051 median 1.051 instructions 1.000 (122.7 -> 129.0 Mcyc)
lib/match: cycles min 1.039 median 1.042 instructions 1.000 (246.0 -> 255.5 Mcyc)
(173 programs compared)

### awfy+apps rows default: plain / bplain
| program | bplain Mcyc | plain Mcyc | cycles min | cycles median | instructions |
|---|---|---|---|---|---|
| awfy/bounce | 73.9 | 68.3 | 0.924 | 0.922 | 0.964 |
| awfy/cd | 250.5 | 241.9 | 0.966 | 0.963 | 0.975 |
| awfy/deltablue | 105.7 | 104.6 | 0.990 | 0.993 | 0.991 |
| awfy/havlak | 154.7 | 156.4 | 1.011 | 1.010 | 1.000 |
| awfy/json | 161.0 | 160.6 | 0.997 | 1.002 | 0.995 |
| awfy/list | 143.8 | 138.8 | 0.966 | 0.963 | 0.988 |
| awfy/mandelbrot | 277.3 | 277.2 | 1.000 | 0.999 | 1.000 |
| awfy/nbody | 137.0 | 131.0 | 0.956 | 0.961 | 0.960 |
| awfy/permute | 54.3 | 52.4 | 0.966 | 0.967 | 0.985 |
| awfy/queens | 116.8 | 107.7 | 0.923 | 0.920 | 0.962 |
| awfy/richards | 59.5 | 55.5 | 0.934 | 0.936 | 0.961 |
| awfy/sieve | 133.9 | 133.6 | 0.998 | 1.001 | 1.000 |
| awfy/storage | 123.1 | 122.0 | 0.991 | 0.990 | 0.998 |
| awfy/towers | 48.3 | 45.2 | 0.936 | 0.932 | 0.957 |
| **awfy geomean (14)** | | | **0.968** | **0.968** | **0.981** |
| apps/events | 175.4 | 173.1 | 0.987 | 0.989 | 1.000 |
| apps/expr_lang | 154.8 | 153.4 | 0.990 | 0.994 | 0.996 |
| apps/lru_cache | 186.2 | 188.4 | 1.012 | 1.008 | 1.000 |
| apps/markdown | 164.5 | 157.7 | 0.959 | 0.973 | 1.000 |
| apps/raytrace | 154.9 | 152.4 | 0.984 | 0.985 | 0.991 |
| **apps geomean (5)** | | | **0.986** | **0.989** | **0.997** |

### awfy+apps rows warm0: plain / bplain
| program | bplain Mcyc | plain Mcyc | cycles min | cycles median | instructions |
|---|---|---|---|---|---|
| awfy/bounce | 73.7 | 67.9 | 0.921 | 0.917 | 0.964 |
| awfy/cd | 246.2 | 237.6 | 0.965 | 0.968 | 0.975 |
| awfy/deltablue | 105.7 | 104.4 | 0.988 | 0.987 | 0.991 |
| awfy/havlak | 154.0 | 156.0 | 1.013 | 1.012 | 1.000 |
| awfy/json | 161.7 | 160.4 | 0.992 | 0.997 | 0.995 |
| awfy/list | 144.2 | 138.4 | 0.959 | 0.961 | 0.988 |
| awfy/mandelbrot | 277.4 | 277.5 | 1.001 | 1.000 | 1.000 |
| awfy/nbody | 138.9 | 131.4 | 0.946 | 0.959 | 0.960 |
| awfy/permute | 54.8 | 52.9 | 0.966 | 0.973 | 0.985 |
| awfy/queens | 116.5 | 106.9 | 0.918 | 0.920 | 0.962 |
| awfy/richards | 60.8 | 62.2 | 1.023 | 1.082 | 0.960 |
| awfy/sieve | 134.5 | 134.1 | 0.997 | 0.997 | 1.000 |
| awfy/storage | 122.7 | 122.2 | 0.996 | 0.996 | 0.998 |
| awfy/towers | 50.5 | 47.4 | 0.937 | 0.939 | 0.956 |
| **awfy geomean (14)** | | | **0.972** | **0.978** | **0.981** |
| apps/events | 172.8 | 171.5 | 0.993 | 0.995 | 1.000 |
| apps/expr_lang | 154.5 | 156.1 | 1.011 | 1.015 | 0.996 |
| apps/lru_cache | 187.7 | 188.5 | 1.004 | 1.004 | 1.000 |
| apps/markdown | 164.5 | 160.3 | 0.974 | 0.971 | 1.000 |
| apps/raytrace | 155.0 | 153.7 | 0.992 | 0.987 | 0.992 |
| **apps geomean (5)** | | | **0.995** | **0.994** | **0.997** |

### default: pgo / bpgo
| set (n) | pgo/bpgo cycles min | median | instructions | worst program (min) | best program (min) |
|---|---|---|---|---|---|
| awfy (14) | 0.966 | 0.964 | 0.980 | sieve 1.002 | bounce 0.919 |
| apps (5) | 0.989 | 0.987 | 0.996 | markdown 1.012 | raytrace 0.967 |
| programs (25) | 0.998 | 1.001 | 0.997 | pcall_error 1.054 | metamethods 0.925 |
| jit-trace (29) | 0.997 | 0.997 | 0.999 | sink_tuple 1.012 | oop_override 0.962 |
| lib (95) | 0.998 | 0.999 | 1.000 | load_small 1.071 | string_key_const 0.910 |
| real (5) | 0.987 | 0.988 | 0.993 | template_engine 1.013 | game_loop 0.960 |
| **all (173)** | **0.995** | 0.995 | 0.997 | | |

### warm0: pgo / bpgo
| set (n) | pgo/bpgo cycles min | median | instructions | worst program (min) | best program (min) |
|---|---|---|---|---|---|
| awfy (14) | 0.969 | 0.974 | 0.980 | sieve 1.002 | queens 0.918 |
| apps (5) | 0.989 | 0.990 | 0.996 | expr_lang 1.011 | raytrace 0.963 |
| programs (25) | 0.994 | 0.997 | 0.997 | oop_methods 1.029 | metamethods 0.923 |
| jit-trace (29) | 0.998 | 0.998 | 0.999 | rec_loop 1.009 | oop_override 0.962 |
| lib (95) | 0.999 | 0.999 | 1.000 | load_small 1.068 | string_key_const 0.902 |
| real (5) | 0.992 | 0.990 | 0.995 | template_engine 1.018 | game_loop 0.953 |
| **all (173)** | **0.995** | 0.996 | 0.997 | | |

### worst (>3%) default: pgo / bpgo
programs/coroutines: cycles min 1.030 median 1.015 instructions 1.000 (1538.6 -> 1585.3 Mcyc)
programs/pcall_error: cycles min 1.054 median 1.045 instructions 1.000 (1782.0 -> 1878.0 Mcyc)
lib/error_tbl: cycles min 1.055 median 1.070 instructions 0.999 (215.0 -> 226.8 Mcyc)
lib/load_small: cycles min 1.071 median 1.072 instructions 1.000 (171.2 -> 183.4 Mcyc)
lib/rep_sep: cycles min 1.036 median 1.043 instructions 1.000 (218.4 -> 226.2 Mcyc)
lib/upper: cycles min 1.054 median 1.055 instructions 1.001 (242.7 -> 256.0 Mcyc)
(173 programs compared)

### worst (>3%) warm0: pgo / bpgo
lib/error_tbl: cycles min 1.060 median 1.053 instructions 0.999 (213.7 -> 226.5 Mcyc)
lib/load_small: cycles min 1.068 median 1.065 instructions 1.000 (171.4 -> 183.1 Mcyc)
lib/rep_sep: cycles min 1.049 median 1.045 instructions 1.000 (216.2 -> 226.8 Mcyc)
lib/upper: cycles min 1.063 median 1.069 instructions 1.001 (245.0 -> 260.6 Mcyc)
(173 programs compared)

### awfy+apps rows default: pgo / bpgo
| program | bpgo Mcyc | pgo Mcyc | cycles min | cycles median | instructions |
|---|---|---|---|---|---|
| awfy/bounce | 73.0 | 67.1 | 0.919 | 0.912 | 0.963 |
| awfy/cd | 232.0 | 223.1 | 0.962 | 0.955 | 0.972 |
| awfy/deltablue | 99.1 | 98.4 | 0.993 | 0.988 | 0.990 |
| awfy/havlak | 138.6 | 137.7 | 0.994 | 0.990 | 0.998 |
| awfy/json | 141.1 | 139.4 | 0.988 | 0.984 | 0.994 |
| awfy/list | 140.3 | 135.8 | 0.968 | 0.966 | 0.988 |
| awfy/mandelbrot | 276.9 | 277.0 | 1.000 | 1.000 | 1.000 |
| awfy/nbody | 137.3 | 131.1 | 0.955 | 0.954 | 0.960 |
| awfy/permute | 54.0 | 52.1 | 0.965 | 0.963 | 0.985 |
| awfy/queens | 114.4 | 106.1 | 0.927 | 0.926 | 0.962 |
| awfy/richards | 58.3 | 54.3 | 0.931 | 0.933 | 0.960 |
| awfy/sieve | 133.4 | 133.7 | 1.002 | 1.002 | 1.000 |
| awfy/storage | 107.7 | 107.3 | 0.996 | 0.988 | 0.997 |
| awfy/towers | 47.5 | 44.4 | 0.935 | 0.936 | 0.957 |
| **awfy geomean (14)** | | | **0.966** | **0.964** | **0.980** |
| apps/events | 161.6 | 158.6 | 0.981 | 0.988 | 1.000 |
| apps/expr_lang | 137.3 | 135.9 | 0.990 | 0.992 | 0.995 |
| apps/lru_cache | 172.7 | 171.4 | 0.993 | 0.994 | 0.998 |
| apps/markdown | 141.8 | 143.6 | 1.012 | 1.001 | 0.999 |
| apps/raytrace | 133.1 | 128.7 | 0.967 | 0.960 | 0.988 |
| **apps geomean (5)** | | | **0.989** | **0.987** | **0.996** |

### awfy+apps rows warm0: pgo / bpgo
| program | bpgo Mcyc | pgo Mcyc | cycles min | cycles median | instructions |
|---|---|---|---|---|---|
| awfy/bounce | 72.3 | 66.8 | 0.923 | 0.922 | 0.963 |
| awfy/cd | 228.5 | 218.8 | 0.957 | 0.955 | 0.972 |
| awfy/deltablue | 99.3 | 98.1 | 0.988 | 0.984 | 0.990 |
| awfy/havlak | 138.8 | 137.9 | 0.994 | 0.993 | 0.997 |
| awfy/json | 140.9 | 139.4 | 0.989 | 0.988 | 0.994 |
| awfy/list | 140.4 | 134.8 | 0.960 | 0.960 | 0.988 |
| awfy/mandelbrot | 277.1 | 277.3 | 1.001 | 1.000 | 1.000 |
| awfy/nbody | 138.8 | 131.9 | 0.950 | 0.958 | 0.960 |
| awfy/permute | 54.4 | 52.5 | 0.966 | 0.964 | 0.985 |
| awfy/queens | 113.9 | 104.5 | 0.918 | 0.928 | 0.962 |
| awfy/richards | 59.6 | 58.9 | 0.988 | 1.066 | 0.960 |
| awfy/sieve | 134.0 | 134.2 | 1.002 | 1.001 | 1.000 |
| awfy/storage | 107.9 | 107.3 | 0.994 | 0.997 | 0.997 |
| awfy/towers | 49.6 | 46.6 | 0.938 | 0.936 | 0.956 |
| **awfy geomean (14)** | | | **0.969** | **0.974** | **0.980** |
| apps/events | 161.7 | 157.5 | 0.974 | 0.977 | 0.999 |
| apps/expr_lang | 136.8 | 138.2 | 1.011 | 1.017 | 0.995 |
| apps/lru_cache | 173.2 | 171.8 | 0.992 | 0.990 | 0.998 |
| apps/markdown | 142.5 | 143.5 | 1.008 | 1.002 | 0.999 |
| apps/raytrace | 133.7 | 128.7 | 0.963 | 0.964 | 0.988 |
| **apps geomean (5)** | | | **0.989** | **0.990** | **0.996** |

duplicate column pgo2 against pgo (default): worst deviation cycles min 1.065, median 1.095, instructions 1.000
duplicate column pgo2 against pgo (warm0): worst deviation cycles min 1.075, median 1.084, instructions 1.041
