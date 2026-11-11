| program | plain/cb1 A (3-7) | B (4,6) | serial x9 | instr ratio | pgo/cb1pgo A | B | serial | instr | dup serial | plain Mcyc | verdict |
|---|---|---|---|---|---|---|---|---|---|---|---|
| awfy/cd | 1.087 | 1.096 | 1.087 | 1.068 | 1.044 | 1.046 | 1.059 | 1.034 | 0.994 | 266 | real instr change |
| awfy/havlak | 1.083 | 0.982 | 1.006 | 0.991 | 0.962 | 0.991 | 0.992 | 0.991 | 1.023 | 156 | noise (gone on serial re-run) |
| corpus/classic_vector | 0.936 | 0.945 | 0.942 | 0.903 | 0.991 | 1.002 | 0.999 | 0.984 | 1.000 | 1581 | win |
| corpus/containers_lru_heap | 0.920 | 0.919 | 0.921 | 0.906 | 0.934 | 0.923 | 0.926 | 0.904 | 1.009 | 1662 | win |
| corpus/json_rxi | 0.940 | 0.935 | 0.928 | 0.945 | 0.905 | 0.922 | 0.912 | 0.938 | 0.997 | 2059 | win |
| corpus/template_etlua | 0.949 | 0.958 | 0.946 | 1.000 | 1.005 | 1.008 | 1.001 | 1.000 | 0.993 | 2398 | win (instr equal: layout/timing) |
| corpus2/dev/csv_report | 0.950 | 0.977 | 1.004 | 1.000 | 1.033 | 1.033 | 0.990 | 1.000 | 1.007 | 1415 | noise (gone on serial re-run) |
| corpus2/dev/event_promise | 0.961 | 1.003 | 0.997 | 1.000 | 1.054 | 0.999 | 0.996 | 1.000 | 0.996 | 397 | noise (gone on serial re-run) |
| corpus2/dev/iter_pipeline | 0.114 | 0.115 | 0.114 | 0.107 | 0.118 | 0.119 | 0.118 | 0.116 | 0.997 | 320 | win |
| corpus2/dev/particles_physics | 0.942 | 0.920 | 0.906 | 0.905 | 0.875 | 0.898 | 0.899 | 0.896 | 0.996 | 464 | win |
| corpus2/heldout/markov_text | 0.783 | 0.792 | 0.781 | 0.693 | 0.763 | 0.777 | 0.766 | 0.675 | 1.002 | 1354 | win |
| corpus2/heldout/prototype_objects | 0.966 | 1.151 | 1.060 | 1.000 | 0.842 | 0.923 | 0.993 | 1.000 | 1.006 | 1856 | layout/timing noise (instr equal) |
| corpus2/heldout/scheme_interp | 0.775 | 0.772 | 0.785 | 0.833 | 0.755 | 0.732 | 0.738 | 0.816 | 1.004 | 2037 | win |
| corpus2/heldout/stream_closures | 0.497 | 0.502 | 0.498 | 0.422 | 0.498 | 0.501 | 0.497 | 0.424 | 0.998 | 940 | win |
| lib/char | 1.001 | 0.994 | 0.999 | 1.000 | 1.034 | 1.053 | 1.036 | 1.000 | 0.998 | 266 | noise (gone on serial re-run) |
| lib/find_pat | 1.021 | 1.015 | 1.015 | 1.000 | 0.941 | 0.954 | 0.955 | 1.000 | 0.987 | 270 | noise (gone on serial re-run) |
| lib/fmt_q | 0.915 | 0.907 | 0.909 | 1.000 | 0.967 | 0.968 | 0.970 | 1.000 | 0.997 | 184 | win (instr equal: layout/timing) |
| lib/match | 1.112 | 1.087 | 1.095 | 1.000 | 1.010 | 0.999 | 0.984 | 1.000 | 1.001 | 261 | layout/timing noise (instr equal) |
| lib/tmove_1k | 0.931 | 0.972 | 0.974 | 1.000 | 0.994 | 0.996 | 0.997 | 1.000 | 0.996 | 95 | noise (gone on serial re-run) |
| lib/tunpack | 1.054 | 1.041 | 1.054 | 1.000 | 0.988 | 0.987 | 0.985 | 1.000 | 1.000 | 272 | layout/timing noise (instr equal) |
| programs/heapsort | 1.090 | 1.020 | 1.011 | 1.001 | 1.062 | 1.026 | 1.006 | 1.001 | 1.020 | 1120 | noise (gone on serial re-run) |
| programs/string_ops | 1.074 | 0.991 | 0.991 | 1.000 | 0.987 | 0.991 | 0.987 | 1.000 | 1.019 | 1643 | noise (gone on serial re-run) |
| programs/table_hash | 0.777 | 0.753 | 1.030 | 1.000 | 0.946 | 1.006 | 1.021 | 1.000 | 1.043 | 837 | noise (gone on serial re-run) |
| real/lua_interp | 1.060 | 1.072 | 1.060 | 0.980 | 1.072 | 1.057 | 1.072 | 0.979 | 0.998 | 127 | layout/timing noise (instr equal) |
