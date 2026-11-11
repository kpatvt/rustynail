| task / category | f = 2 | f = 4 | delete (inf) | top programs (delete bound, new/old) |
|---|---|---|---|---|
| P-P allocator + GC (alloc+GC) | 0.942 | 0.909 | 0.871 | closures 0.37, gc_churn 0.47, binary_trees 0.65, knucleotide 0.73 |
| P-P allocator only | 0.969 | 0.951 | 0.932 | closures 0.50, knucleotide 0.74, varargs 0.82, binary_trees 0.84 |
| P-P GC only | 0.975 | 0.961 | 0.946 | gc_churn 0.55, binary_trees 0.81, closures 0.87, string_ops 0.88 |
| P-Q table internals + interning | 0.967 | 0.949 | 0.931 | table_hash 0.72, string_ops 0.82, json 0.82, fasta 0.83 |
| P-Q interning only | 0.989 | 0.983 | 0.977 | string_ops 0.82, knucleotide 0.86, table_hash 0.89, json 0.96 |
| P-Q table internals only | 0.978 | 0.967 | 0.955 | fasta 0.83, table_hash 0.83, json 0.86, gc_churn 0.87 |
| JIT helper calls (jit_*: get/set/call/len/closure...) | 0.978 | 0.967 | 0.955 | closures 0.82, varargs 0.82, binary_trees 0.87, fasta 0.87 |
| stdlib + libc (library code) | 0.931 | 0.888 | 0.830 | table_array 0.16, string_ops 0.49, table_hash 0.53, json 0.57 |
| call/error machinery + interpreter + metamethods (code no trace covers) | 0.937 | 0.891 | 0.812 | metamethods 0.12, coroutines 0.13, pcall_error 0.50, json 0.92 |
| Lua compiler (parse/codegen) | 0.983 | 0.971 | 0.953 | load_big 0.30, matmul 1.00, table_hash 1.00, sieve 1.00 |
| JIT compile (record/opt/asm) | 0.999 | 0.999 | 0.999 | spectral_norm 0.99, oop_methods 0.99, closures 1.00, mandelbrot 1.00 |
