# P-W profile (string creation / interning), traces on, RUSTYNAIL_SEED=0, perf cycles:u, one CPU, profiling build

| program | top symbols (self cycles) |
|---|---|
| str_concat | jit_str_probe 29.1%, jit_tr_alloc 27.6%, tostring_int 10.5%, free_obj 6.1%, pool alloc_block 2.9%, free_block 2.3% |
| knucleotide | jit_str_probe 16.9%, jit_tr_alloc 14.1%, libc memcpy ~5% |
| json | str_find_aux 4.8%, jit_call 4.5%, jit_tr_alloc 4.1%, new_str 4.0%, jit_str_probe 3.5% |
| string_ops | new_str 18.3%, free_obj 5.3%, resize_strtab 3.3% (pattern matching dominates the rest) |
| gc_churn | GC mark/sweep ~35%, new_str 3.0% (strings are not the bottleneck) |

Inside `jit_str_probe` (str_concat): the `lua_hash` loop is 31% of the function (about 9% of the program); the remaining 69% is the
bucket-head load and chain walk (cache misses on cold bucket heads / dead-but-unswept nodes) and the table-full check.
str_concat: 1.53 G cycles / 4.06 G instructions for 2M iterations (about 770 cycles per iteration, two concats and a tostring each).
