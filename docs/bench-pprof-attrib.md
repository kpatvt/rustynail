### fasta
| category | trace on | J4 only | JIT off |
|---|---|---|---|
| tier-1 (J4) code |  50.8% |  51.3% |   0.0% |
| interpreter dispatch |   0.6% |   0.6% |  72.5% |
| JIT helpers |  12.6% |   8.8% |   0.0% |
| allocator |   1.9% |   1.4% |   0.9% |
| GC |   3.2% |   4.4% |   2.6% |
| table internals |  17.0% |  20.9% |  14.5% |
| stdlib |   6.7% |   4.8% |   5.2% |
| libc (memcpy etc.) |   5.3% |   5.8% |   2.9% |
| total Mcyc (perf, sampled) | 2214 | 2165 | 3196 |

top helpers (on): jit_len 9.8%, jit_set 2.2%, jit_call 0.4%, jit_get 0.1%, jit_newtable 0.1%
top symbols (on): lua_proto@0x7f7fe21f6bf0 30.4%, lua_proto@0x7f7fe21f6770.v1 20.5%, jit_helpers::jit_len 9.8%, <table::TableRef>::len 6.9%, lib_table::tconcat::{closure#0} 5.2%, <table::TableRef>::finish_set 4.7%, <heap::Heap>::propagatemark 2.7%, jit_helpers::jit_set 2.2%
bytecodes (hook count, JIT off): 284.5 M; interpreter self cycles per bytecode (JIT off) 8.15; ESTIMATED share of bytecodes still interpreted with traces on: 1% (raw 1%)
stats: recorded 6 installed 0 entered 0 flushes 0 rerecord 0 blacklisted 1 refused 0 deferred 0 ineffective 0 / aborted UpvalAlias: 6 / exits 0 (live traces 0)

### load_big
| category | trace on | J4 only | JIT off |
|---|---|---|---|
| Lua compiler |  69.7% |  70.3% |  72.5% |
| allocator |   7.5% |   5.8% |   5.8% |
| string interning |   2.9% |   1.9% |   2.2% |
| GC |   2.5% |   3.2% |   2.4% |
| libc (memcpy etc.) |  15.7% |  17.4% |  15.1% |
| kernel/unknown |   0.6% |   0.9% |   1.1% |
| total Mcyc (perf, sampled) | 821 | 821 | 816 |

top helpers (on): 
top symbols (on): <lua_compiler::lexer::LexState<heap::Heap>>::lle 12.9%, <lua_compiler::v55::Parser<heap::Heap>>::subexpr 9.2%, 0x00000000001ae49e 4.3%, <std::hash::random::DefaultHasher as core::hash: 3.6%, cfree 3.4%, <lua_compiler::v55::Parser<heap::Heap>>::single_ 3.1%, <heap::Heap>::new_str 2.9%, <lua_compiler::lexer::Token<string::StrRef> as c 2.8%
bytecodes (hook count, JIT off): 0.0 M; interpreter self cycles per bytecode (JIT off) 0.00; ESTIMATED share of bytecodes still interpreted with traces on: nan% (raw nan%)
stats: recorded 1 installed 0 entered 0 flushes 0 rerecord 0 blacklisted 0 refused 0 deferred 0 ineffective 0 / aborted Nyi(Loadk): 1 / exits 0 (live traces 0)

### coroutines
| category | trace on | J4 only | JIT off |
|---|---|---|---|
| tier-1 (J4) code |   3.1% |   5.4% |   0.0% |
| interpreter dispatch |  52.6% |  54.0% |  66.8% |
| call machinery |  34.4% |  32.7% |  31.1% |
| JIT helpers |   7.3% |   5.0% |   0.0% |
| allocator |   0.9% |   1.3% |   0.6% |
| GC |   1.3% |   1.3% |   1.4% |
| total Mcyc (perf, sampled) | 1990 | 1920 | 1995 |

top helpers (on): jit_call 6.5%, jit_tforcall 0.8%
top symbols (on): vm_unsafe::run_mode::<false> 39.9%, vm_unsafe::finish_op 11.5%, lib_coroutine::auxwrap 9.9%, lib_coroutine::auxresume 7.3%, jit_helpers::jit_call 6.5%, <state::State>::ccall 4.8%, <state::State>::unroll 3.8%, lib_coroutine::co_yield 3.3%
bytecodes (hook count, JIT off): 37.8 M; interpreter self cycles per bytecode (JIT off) 35.31; ESTIMATED share of bytecodes still interpreted with traces on: 79% (raw 79%)
stats: recorded 10 installed 0 entered 0 flushes 0 rerecord 0 blacklisted 5 refused 0 deferred 0 ineffective 0 / aborted CFunc: 8 / aborted Nyi(Closure): 2 / exits 0 (live traces 0)

### pcall_error
| category | trace on | J4 only | JIT off |
|---|---|---|---|
| tier-1 (J4) code |   3.3% |   3.4% |   0.0% |
| interpreter dispatch |  19.1% |  16.5% |  30.0% |
| call machinery |  30.6% |  32.1% |  31.5% |
| JIT helpers |  10.9% |  10.4% |   0.0% |
| allocator |   6.9% |   7.4% |   7.6% |
| string interning |   3.5% |   4.6% |   3.8% |
| GC |   3.8% |   3.8% |   2.8% |
| table internals |   2.7% |   1.9% |   3.3% |
| number<->string |   5.8% |   7.3% |   5.3% |
| stdlib |   2.4% |   1.1% |   0.7% |
| libc (memcpy etc.) |   9.4% |  10.0% |  13.5% |
| other |   0.9% |   1.1% |   1.1% |
| total Mcyc (perf, sampled) | 2776 | 2868 | 2938 |

top helpers (on): jit_call 10.7%, jit_closure 0.2%
top symbols (on): vm_unsafe::run_mode::<false> 16.7%, <state::State>::get_info_aux 11.8%, jit_helpers::jit_call 10.7%, <state::State>::where_ 4.2%, <state::State>::pcall_k 3.9%, <state::State>::ccall 3.7%, <i32 as alloc::string::SpecToString>::spec_to_st 3.7%, <heap::Heap>::new_str 3.0%
bytecodes (hook count, JIT off): 73.3 M; interpreter self cycles per bytecode (JIT off) 12.00; ESTIMATED share of bytecodes still interpreted with traces on: 60% (raw 60%)
stats: recorded 13 installed 1 entered 0 flushes 1 rerecord 0 blacklisted 4 refused 0 deferred 0 ineffective 0 / aborted CFunc: 2 / aborted FnShort: 8 / aborted Nyi(Closure): 2 / exits 1331551 (live traces 0)

### metamethods
| category | trace on | J4 only | JIT off |
|---|---|---|---|
| interpreter dispatch |  67.5% |  66.6% |  69.2% |
| metamethod paths |  15.3% |  15.3% |  13.7% |
| call machinery |   5.0% |   5.4% |   4.9% |
| allocator |   0.4% |   0.4% |   0.7% |
| GC |   1.9% |   2.4% |   1.7% |
| table internals |   4.1% |   3.7% |   4.0% |
| stdlib |   5.4% |   6.0% |   5.5% |
| total Mcyc (perf, sampled) | 4379 | 4362 | 4398 |

top helpers (on): 
top symbols (on): vm_unsafe::run_mode::<false> 62.1%, vm_unsafe::mm_return 7.4%, lib_base::base_setmetatable 5.0%, vm_unsafe::finish_op 3.8%, vm_unsafe::mm_eq 3.6%, vm_unsafe::mm_bin 3.2%, <value::Value>::raw_equal 2.0%, vm_unsafe::index_chain_short_str 1.7%
bytecodes (hook count, JIT off): 228.0 M; interpreter self cycles per bytecode (JIT off) 13.36; ESTIMATED share of bytecodes still interpreted with traces on: 97% (raw 97%)
stats: recorded 2 installed 0 entered 0 flushes 0 rerecord 0 blacklisted 0 refused 0 deferred 0 ineffective 0 / aborted Metamethod: 2 / exits 0 (live traces 0)

### table_hash
| category | trace on | J4 only | JIT off |
|---|---|---|---|
| trace code |   2.3% |   0.0% |   0.0% |
| tier-1 (J4) code |  15.3% |  16.3% |   0.0% |
| interpreter dispatch |   0.0% |   0.6% |  12.2% |
| call machinery |   0.0% |   0.0% |   2.0% |
| JIT helpers |   1.5% |   2.9% |   0.0% |
| allocator |   1.3% |   0.3% |   0.4% |
| string interning |  10.7% |   9.2% |   9.8% |
| GC |   4.7% |   4.5% |   4.0% |
| table internals |  16.9% |  15.6% |  27.2% |
| number<->string |   4.4% |   5.5% |   5.2% |
| stdlib |  24.1% |  27.1% |  24.7% |
| libc (memcpy etc.) |  18.0% |  17.4% |  14.1% |
| total Mcyc (perf, sampled) | 1375 | 1490 | 1473 |

top helpers (on): jit_concat 1.0%, jit_set 0.5%
top symbols (on): <state::State>::concat_result::{closure#0} 22.3%, lua_proto@0x7fbff75fb640.v2 15.0%, 0x00000000001b7af9 13.4%, <heap::Heap>::new_str 9.1%, <table::TableRef>::next 7.5%, number::tostring_int 4.4%, <table::TableRef>::psetshortstr 2.9%, table::insertkey 2.6%
bytecodes (hook count, JIT off): 16.1 M; interpreter self cycles per bytecode (JIT off) 11.14; ESTIMATED share of bytecodes still interpreted with traces on: 0% (raw 0%)
stats: recorded 10 installed 4 entered 4 flushes 4 rerecord 0 blacklisted 3 refused 0 deferred 0 ineffective 0 / aborted TableHash: 6 / exits 4 (live traces 0)

### table_array
| category | trace on | J4 only | JIT off |
|---|---|---|---|
| tier-1 (J4) code |   0.0% |   0.9% |   0.0% |
| interpreter dispatch |   0.7% |   1.0% |   2.3% |
| metamethod paths |   0.7% |   1.3% |   1.1% |
| call machinery |   1.0% |   0.7% |   3.2% |
| table internals |  12.5% |   3.6% |   6.0% |
| stdlib |  84.0% |  92.0% |  87.3% |
| total Mcyc (perf, sampled) | 2495 | 2214 | 2302 |

top helpers (on): 
top symbols (on): lib_table::tremove 39.0%, lib_table::tinsert 36.1%, <state::State>::raw_set_int 12.1%, lib_table::auxsort 6.6%, lib_table::sort_comp 2.2%, <state::State>::ccall 1.0%, <state::State>::compare_lt_le 0.7%, vm::execute 0.4%
bytecodes (hook count, JIT off): 9.8 M; interpreter self cycles per bytecode (JIT off) 5.49; ESTIMATED share of bytecodes still interpreted with traces on: 33% (raw 33%)
stats: recorded 8 installed 6 entered 29 flushes 6 rerecord 0 blacklisted 1 refused 0 deferred 0 ineffective 0 / aborted Nyi(Call): 2 / exits 29 (live traces 0)

### oop_methods
| category | trace on | J4 only | JIT off |
|---|---|---|---|
| trace code |  93.7% |   0.0% |   0.0% |
| tier-1 (J4) code |   0.0% |  63.3% |   0.0% |
| JIT compile |   0.6% |   0.0% |   0.0% |
| interpreter dispatch |   4.7% |   0.2% |  99.9% |
| JIT helpers |   0.0% |  13.1% |   0.0% |
| table internals |   0.0% |  23.3% |   0.0% |
| total Mcyc (perf, sampled) | 227 | 4047 | 5008 |

top helpers (on): 
top symbols (on): 0x00007fd7f8308669 10.9%, 0x00007fd7f83085a1 8.8%, 0x00007fd7f83084d8 7.0%, 0x00007fd7f8308500 6.6%, 0x00007fd7f8308681 5.2%, 0x00007fd7f83085e2 4.4%, 0x00007fd7f830863c 3.9%, vm_unsafe::run_mode::<false> 3.2%
bytecodes (hook count, JIT off): 415.4 M; interpreter self cycles per bytecode (JIT off) 12.04; ESTIMATED share of bytecodes still interpreted with traces on: 0% (raw 0%)
stats: recorded 1 installed 1 entered 0 flushes 1 rerecord 0 blacklisted 0 refused 0 deferred 0 ineffective 0 / exits 1 (live traces 0)

### string_ops
| category | trace on | J4 only | JIT off |
|---|---|---|---|
| tier-1 (J4) code |   4.2% |   4.0% |   0.0% |
| interpreter dispatch |   2.3% |   2.2% |  13.9% |
| call machinery |   0.9% |   0.9% |   0.6% |
| JIT helpers |   4.8% |   4.4% |   0.0% |
| allocator |   4.8% |   4.0% |   3.7% |
| string interning |  17.8% |  17.6% |  18.5% |
| GC |  11.8% |  11.4% |  10.9% |
| table internals |   0.2% |   0.2% |   1.8% |
| number<->string |   1.3% |   1.9% |   1.3% |
| stdlib |  29.2% |  29.1% |  25.5% |
| libc (memcpy etc.) |  20.8% |  22.6% |  22.0% |
| kernel/unknown |   0.8% |   0.7% |   0.5% |
| total Mcyc (perf, sampled) | 1810 | 1803 | 1836 |

top helpers (on): jit_call 2.1%, jit_get 1.3%, jit_len 1.0%, jit_tforcall 0.4%
top symbols (on): <heap::Heap>::new_str 14.2%, lib_string::gmatch_aux 8.1%, <pattern::MatchState>::m 8.0%, <heap::Heap>::free_obj 5.3%, <heap::Heap>::resize_strtab 3.3%, lua_proto@0x7f0ccaaf6780.v1 3.2%, lib_string::str_format 2.9%, <pattern::MatchState>::max_expand 2.4%
bytecodes (hook count, JIT off): 11.3 M; interpreter self cycles per bytecode (JIT off) 22.58; ESTIMATED share of bytecodes still interpreted with traces on: 16% (raw 16%)
stats: recorded 5 installed 1 entered 43 flushes 1 rerecord 0 blacklisted 2 refused 0 deferred 0 ineffective 0 / aborted CFunc: 4 / exits 43 (live traces 0)

### json
| category | trace on | J4 only | JIT off |
|---|---|---|---|
| trace code |   1.0% |   0.0% |   0.0% |
| tier-1 (J4) code |  10.2% |  13.3% |   0.0% |
| interpreter dispatch |   4.5% |   5.1% |  23.2% |
| metamethod paths |   0.8% |   0.9% |   0.8% |
| call machinery |   3.0% |   3.0% |   3.0% |
| JIT helpers |   9.7% |  12.6% |   0.0% |
| allocator |   5.8% |   3.1% |   2.6% |
| string interning |   3.7% |   5.2% |   6.1% |
| GC |   3.2% |   3.0% |   3.2% |
| table internals |  14.2% |  17.8% |  19.8% |
| number<->string |   2.9% |   2.1% |   2.8% |
| stdlib |  33.0% |  27.7% |  33.4% |
| libc (memcpy etc.) |   6.8% |   4.9% |   4.3% |
| total Mcyc (perf, sampled) | 2269 | 2517 | 2627 |

top helpers (on): jit_call 5.8%, jit_get 1.5%, jit_set 0.8%, jit_newtable 0.5%, jit_len 0.3%
top symbols (on): jit_helpers::jit_call 5.8%, <pattern::MatchState>::skip 5.1%, <pattern::MatchState>::m 4.5%, lib_string::str_find_aux 4.4%, vm_unsafe::index_string_short_str 4.4%, <state::State>::push_vec 4.1%, table::mainposition 3.5%, lua_proto@0x55cb3f10a500.v2 3.4%
bytecodes (hook count, JIT off): 94.1 M; interpreter self cycles per bytecode (JIT off) 6.48; ESTIMATED share of bytecodes still interpreted with traces on: 17% (raw 17%)
stats: recorded 20 installed 5 entered 539967 flushes 5 rerecord 4 blacklisted 6 refused 0 deferred 4 ineffective 0 / aborted CFunc: 7 / aborted FnShort: 4 / aborted LeaveLoop: 2 / aborted Nyi(Closure): 2 / exits 540571 (live traces 0)

### binary_trees
| category | trace on | J4 only | JIT off |
|---|---|---|---|
| trace code |  24.0% |   0.0% |   0.0% |
| tier-1 (J4) code |  14.0% |  38.9% |   0.0% |
| interpreter dispatch |   0.0% |   0.0% |  67.3% |
| JIT helpers |  13.3% |  23.4% |   0.0% |
| allocator |  15.9% |  14.6% |  12.4% |
| GC |  19.5% |  16.2% |  12.5% |
| table internals |  10.1% |   6.6% |   7.5% |
| total Mcyc (perf, sampled) | 2117 | 2584 | 3284 |

top helpers (on): jit_get 7.9%, jit_newtable 5.3%, jit_setlist 0.1%
top symbols (on): jit_helpers::jit_get 7.9%, lua_proto@0x7f8c80751c50 7.5%, <heap::Heap>::free_obj 7.3%, lua_proto@0x7f8c8074f3f0 6.5%, <table::TableRef>::get 5.3%, jit_helpers::jit_newtable 5.3%, <heap::Heap>::propagatemark 4.8%, tr::asm::jit_tr_alloc 4.8%
bytecodes (hook count, JIT off): 232.2 M; interpreter self cycles per bytecode (JIT off) 9.52; ESTIMATED share of bytecodes still interpreted with traces on: 0% (raw 0%)
stats: recorded 13 installed 6 entered 15120186 flushes 6 rerecord 0 blacklisted 1 refused 0 deferred 0 ineffective 0 / aborted FnShort: 1 / aborted UpvalAlias: 6 / exits 363854 (live traces 0)

### gc_churn
| category | trace on | J4 only | JIT off |
|---|---|---|---|
| tier-1 (J4) code |   4.3% |   5.6% |   0.0% |
| interpreter dispatch |   1.2% |   1.3% |  14.9% |
| metamethod paths |   0.9% |   1.2% |   1.0% |
| call machinery |   1.0% |   0.7% |   0.5% |
| JIT helpers |  10.6% |  10.9% |   0.0% |
| allocator |   8.2% |   9.4% |   8.3% |
| string interning |   2.6% |   3.1% |   3.1% |
| GC |  45.0% |  44.5% |  46.7% |
| table internals |  13.1% |  12.5% |  11.9% |
| number<->string |   2.6% |   2.9% |   3.4% |
| stdlib |   1.6% |   2.2% |   2.7% |
| libc (memcpy etc.) |   8.2% |   5.2% |   7.1% |
| total Mcyc (perf, sampled) | 3501 | 3485 | 3395 |

top helpers (on): jit_setlist 6.2%, jit_set 1.7%, jit_newtable 1.4%, jit_call 0.6%, jit_closure 0.5%
top symbols (on): <heap::Heap>::propagatemark 14.0%, <heap::Heap>::sweepstep 9.8%, jit_helpers::jit_setlist 6.2%, <heap::Heap>::atomic2gen 5.5%, <heap::Heap>::reallymark 5.4%, lua_proto@0x7f25f63c25e0.v1 4.3%, table::newkey 4.3%, <state::State>::gc_step 4.2%
bytecodes (hook count, JIT off): 45.3 M; interpreter self cycles per bytecode (JIT off) 11.13; ESTIMATED share of bytecodes still interpreted with traces on: 9% (raw 9%)
stats: recorded 4 installed 0 entered 0 flushes 0 rerecord 0 blacklisted 2 refused 0 deferred 0 ineffective 0 / aborted Nyi(Closure): 2 / aborted TableHash: 2 / exits 0 (live traces 0)

### closures
| category | trace on | J4 only | JIT off |
|---|---|---|---|
| trace code |   3.9% |   0.0% |   0.0% |
| tier-1 (J4) code |   9.0% |  62.3% |   0.0% |
| interpreter dispatch |   0.2% |   0.0% |  82.2% |
| call machinery |   3.9% |  18.4% |   6.2% |
| JIT helpers |  17.9% |  10.1% |   0.0% |
| allocator |  49.9% |   6.5% |   9.6% |
| GC |  12.7% |   2.4% |   1.5% |
| libc (memcpy etc.) |   1.7% |   0.1% |   0.2% |
| total Mcyc (perf, sampled) | 525 | 3509 | 5878 |

top helpers (on): jit_closure 14.3%, jit_return_close 3.4%, jit_tailcall 0.2%
top symbols (on): vm_unsafe::push_closure 25.0%, <alloc::PoolAllocator as alloc::Allocator>::allo 21.3%, jit_helpers::jit_closure 14.3%, lua_proto@0x7fca2f08d9a0 7.2%, <heap::Heap>::free_obj 7.1%, <state::State>::gc_step 4.9%, <state::State>::close 3.9%, <alloc::PoolAllocator as alloc::Allocator>::free 3.6%
bytecodes (hook count, JIT off): 550.5 M; interpreter self cycles per bytecode (JIT off) 8.77; ESTIMATED share of bytecodes still interpreted with traces on: 0% (raw 0%)
stats: recorded 11 installed 2 entered 0 flushes 2 rerecord 0 blacklisted 3 refused 0 deferred 0 ineffective 0 / aborted FnShort: 4 / aborted LeaveLoop: 1 / aborted Nyi(Closure): 2 / aborted TailCall: 2 / exits 4 (live traces 0)

### fannkuch
| category | trace on | J4 only | JIT off |
|---|---|---|---|
| trace code |  96.6% |   0.0% |   0.0% |
| tier-1 (J4) code |   2.9% |  99.8% |   0.0% |
| interpreter dispatch |   0.1% |   0.0% |  94.8% |
| other |   0.0% |   0.1% |   4.9% |
| total Mcyc (perf, sampled) | 1622 | 1704 | 4154 |

top helpers (on): 
top symbols (on): 0x00007f8c0da9ef6b 3.3%, 0x00007f8c0daa0900 2.9%, lua_proto@0x7f8c0db9d940 2.9%, 0x00007f8c0daa010c 2.6%, 0x00007f8c0daa00a7 1.8%, 0x00007f8c0daa0093 1.8%, 0x00007f8c0daa0911 1.8%, 0x00007f8c0da9ef9b 1.8%
bytecodes (hook count, JIT off): 565.0 M; interpreter self cycles per bytecode (JIT off) 6.97; ESTIMATED share of bytecodes still interpreted with traces on: 0% (raw 0%)
stats: recorded 8 installed 6 entered 57436707 flushes 6 rerecord 0 blacklisted 0 refused 0 deferred 0 ineffective 0 / aborted LeaveLoop: 2 / exits 41195202 (live traces 0)

### varargs
| category | trace on | J4 only | JIT off |
|---|---|---|---|
| trace code |  19.7% |   0.0% |   0.0% |
| tier-1 (J4) code |  10.5% |  43.1% |   0.0% |
| trace exit (Rust) |   1.4% |   0.0% |   0.0% |
| interpreter dispatch |   0.0% |   0.0% |  51.1% |
| call machinery |   3.1% |   0.2% |   3.0% |
| JIT helpers |  17.9% |  19.2% |   0.0% |
| allocator |  17.8% |   0.6% |   1.7% |
| string interning |   0.2% |   6.2% |   5.6% |
| GC |   7.7% |   3.1% |   1.9% |
| table internals |  11.2% |   3.4% |   2.7% |
| stdlib |  10.1% |   7.3% |  12.2% |
| other |   0.1% |  16.2% |  21.3% |
| total Mcyc (perf, sampled) | 1455 | 3968 | 4708 |

top helpers (on): jit_call 17.9%
top symbols (on): jit_helpers::jit_call 17.9%, tr::asm::jit_tr_alloc 12.7%, lua_proto@0x7f76b03fe5f0 9.7%, 0x00007f76b02fef2a 9.1%, lib_table::tunpack 6.9%, <state::State>::gc_step 4.5%, 0x00007f76b02feefe 3.8%, table::resize 3.5%
bytecodes (hook count, JIT off): 292.0 M; interpreter self cycles per bytecode (JIT off) 8.23; ESTIMATED share of bytecodes still interpreted with traces on: 0% (raw 0%)
stats: recorded 3 installed 1 entered 0 flushes 1 rerecord 0 blacklisted 1 refused 0 deferred 0 ineffective 0 / aborted Vararg: 2 / exits 3998497 (live traces 0)

### knucleotide
| category | trace on | J4 only | JIT off |
|---|---|---|---|
| trace code |  43.0% |   0.0% |   0.0% |
| tier-1 (J4) code |   8.8% |  36.3% |   0.0% |
| interpreter dispatch |   0.0% |   4.8% |  46.0% |
| JIT helpers |   0.0% |  28.5% |   0.0% |
| allocator |  26.3% |   0.0% |   0.0% |
| string interning |  13.9% |   1.8% |  30.6% |
| GC |   0.4% |   0.5% |   0.3% |
| table internals |   0.3% |   5.6% |  11.4% |
| stdlib |   2.0% |  19.6% |  10.0% |
| libc (memcpy etc.) |   2.0% |   2.3% |   0.9% |
| total Mcyc (perf, sampled) | 2633 | 3497 | 4174 |

top helpers (on): 
top symbols (on): tr::asm::jit_tr_alloc 26.3%, <heap::Heap>::jit_str_probe 13.9%, 0x00007f78312d4864 10.7%, 0x00007f78312d4859 4.6%, lua_proto@0x7f78313ce700.v1 4.4%, lua_proto@0x7f78313cec60 4.3%, 0x00007f78312d48a8 3.5%, 0x00007f78312d5901 2.5%
bytecodes (hook count, JIT off): 270.0 M; interpreter self cycles per bytecode (JIT off) 7.11; ESTIMATED share of bytecodes still interpreted with traces on: 0% (raw 0%)
stats: recorded 7 installed 1 entered 18749053 flushes 1 rerecord 0 blacklisted 1 refused 0 deferred 0 ineffective 0 / aborted UpvalAlias: 6 / exits 149997893 (live traces 0)

### fib
| category | trace on | J4 only | JIT off |
|---|---|---|---|
| trace code |  99.8% |   0.0% |   0.0% |
| tier-1 (J4) code |   0.1% |  99.9% |   0.0% |
| interpreter dispatch |   0.0% |   0.0% |  99.9% |
| total Mcyc (perf, sampled) | 1458 | 2205 | 4332 |

top helpers (on): 
top symbols (on): 0x00007f8f746edf49 9.2%, 0x00007f8f746eda7a 7.7%, 0x00007f8f746eda5c 4.8%, 0x00007f8f746edf61 3.0%, 0x00007f8f746edb12 2.7%, 0x00007f8f746ed046 2.4%, 0x00007f8f746ecff0 2.3%, 0x00007f8f746edaff 2.0%
bytecodes (hook count, JIT off): 430.0 M; interpreter self cycles per bytecode (JIT off) 10.07; ESTIMATED share of bytecodes still interpreted with traces on: 0% (raw 0%)
stats: recorded 4 installed 3 entered 99 flushes 3 rerecord 0 blacklisted 0 refused 0 deferred 0 ineffective 0 / aborted FnShort: 1 / exits 39063600 (live traces 0)

### queens
| category | trace on | J4 only | JIT off |
|---|---|---|---|
| trace code |  46.2% |   0.0% |   0.0% |
| tier-1 (J4) code |  51.9% |  99.9% |   0.0% |
| interpreter dispatch |   0.0% |   0.0% |  98.2% |
| other |   0.0% |   0.0% |   1.8% |
| total Mcyc (perf, sampled) | 2129 | 2469 | 4554 |

top helpers (on): 
top symbols (on): lua_proto@0x7f4bc3b36750 51.9%, 0x00007f4bc3702c22 2.7%, 0x00007f4bc3702c1e 2.7%, 0x00007f4bc3702c58 2.7%, 0x00007f4bc3703493 2.6%, 0x00007f4bc37034c2 1.9%, 0x00007f4bc3702c43 1.6%, 0x00007f4bc3702c7f 1.5%
bytecodes (hook count, JIT off): 519.7 M; interpreter self cycles per bytecode (JIT off) 8.60; ESTIMATED share of bytecodes still interpreted with traces on: 0% (raw 0%)
stats: recorded 2 installed 2 entered 1545 flushes 2 rerecord 2 blacklisted 1 refused 0 deferred 4 ineffective 2 / exits 18819 (live traces 0)

### sieve
| category | trace on | J4 only | JIT off |
|---|---|---|---|
| trace code |  90.4% |   0.0% |   0.0% |
| tier-1 (J4) code |   0.1% |  93.9% |   0.0% |
| interpreter dispatch |   0.1% |   0.0% |  95.9% |
| table internals |   3.6% |   2.3% |   1.6% |
| libc (memcpy etc.) |   5.1% |   3.2% |   2.0% |
| total Mcyc (perf, sampled) | 1513 | 2400 | 3553 |

top helpers (on): 
top symbols (on): 0x00007fe1fd1044a3 23.9%, 0x00007fe1fd104465 14.6%, 0x00007fe1fd10446f 10.7%, 0x00007fe1fd10449b 10.0%, 0x00007fe1fd10447c 5.8%, 0x00007fe1fd103de4 3.1%, 0x00007fe1fd10448e 2.8%, 0x00007fe1fd1069e9 2.8%
bytecodes (hook count, JIT off): 347.8 M; interpreter self cycles per bytecode (JIT off) 9.79; ESTIMATED share of bytecodes still interpreted with traces on: 0% (raw 0%)
stats: recorded 6 installed 6 entered 2352660 flushes 6 rerecord 2 blacklisted 0 refused 0 deferred 2 ineffective 1 / exits 2491349 (live traces 0)

### heapsort
| category | trace on | J4 only | JIT off |
|---|---|---|---|
| trace code |  89.1% |   0.0% |   0.0% |
| tier-1 (J4) code |   9.7% |  99.4% |   0.0% |
| interpreter dispatch |   0.1% |   0.0% |  99.8% |
| total Mcyc (perf, sampled) | 1290 | 2152 | 4164 |

top helpers (on): 
top symbols (on): 0x00007f30b30c366a 8.9%, lua_proto@0x7f30b31c2a50 5.8%, 0x00007f30b30c3805 5.7%, 0x00007f30b30c3655 4.2%, lua_proto@0x7f30b31c2860.v2 3.6%, 0x00007f30b30c3691 3.5%, 0x00007f30b30c6ba2 3.2%, 0x00007f30b30c4901 2.9%
bytecodes (hook count, JIT off): 578.0 M; interpreter self cycles per bytecode (JIT off) 7.19; ESTIMATED share of bytecodes still interpreted with traces on: 0% (raw 0%)
stats: recorded 17 installed 3 entered 13000297 flushes 3 rerecord 0 blacklisted 2 refused 0 deferred 0 ineffective 0 / aborted LeaveFrame: 2 / aborted UpvalAlias: 12 / exits 13209487 (live traces 0)

### matmul
| category | trace on | J4 only | JIT off |
|---|---|---|---|
| trace code |  94.1% |   0.0% |   0.0% |
| tier-1 (J4) code |   2.3% |  99.5% |   0.0% |
| interpreter dispatch |   0.4% |   0.1% |  99.7% |
| table internals |   1.2% |   0.0% |   0.1% |
| total Mcyc (perf, sampled) | 505 | 1248 | 3482 |

top helpers (on): jit_set 0.2%
top symbols (on): 0x00007f3f3f90855c 21.6%, 0x00007f3f3f908558 7.8%, 0x00007f3f3f90850d 6.8%, 0x00007f3f3f9084e9 6.1%, 0x00007f3f3f908549 5.9%, 0x00007f3f3f90853a 5.7%, 0x00007f3f3f9084df 5.7%, 0x00007f3f3f9084af 5.5%
bytecodes (hook count, JIT off): 388.5 M; interpreter self cycles per bytecode (JIT off) 8.93; ESTIMATED share of bytecodes still interpreted with traces on: 0% (raw 0%)
stats: recorded 7 installed 7 entered 320372 flushes 7 rerecord 1 blacklisted 0 refused 0 deferred 2 ineffective 0 / exits 167434 (live traces 0)

### nbody
| category | trace on | J4 only | JIT off |
|---|---|---|---|
| trace code |  92.1% |   0.0% |   0.0% |
| tier-1 (J4) code |   6.9% |  69.4% |   0.0% |
| interpreter dispatch |   0.2% |   0.1% |  86.2% |
| JIT helpers |   0.0% |   5.2% |   0.0% |
| stdlib |   0.0% |  25.2% |   8.9% |
| other |   0.1% |   0.0% |   4.9% |
| total Mcyc (perf, sampled) | 720 | 2549 | 4453 |

top helpers (on): 
top symbols (on): 0x00007f7ce1509e59 18.1%, 0x00007f7ce15060bf 10.4%, 0x00007f7ce150a815 6.9%, lua_proto@0x7f7ce18ceba0 4.7%, 0x00007f7ce1509f2e 3.8%, 0x00007f7ce1508ef5 3.8%, 0x00007f7ce1508f14 3.3%, 0x00007f7ce1508f1e 3.0%
bytecodes (hook count, JIT off): 526.0 M; interpreter self cycles per bytecode (JIT off) 7.30; ESTIMATED share of bytecodes still interpreted with traces on: 0% (raw 0%)
stats: recorded 4 installed 2 entered 8997374 flushes 2 rerecord 0 blacklisted 1 refused 0 deferred 0 ineffective 0 / aborted InnerLoop: 2 / exits 4998469 (live traces 0)

### mandelbrot
| category | trace on | J4 only | JIT off |
|---|---|---|---|
| trace code |  95.7% |   0.0% |   0.0% |
| tier-1 (J4) code |   3.7% | 100.0% |   0.0% |
| interpreter dispatch |   0.2% |   0.0% | 100.0% |
| total Mcyc (perf, sampled) | 559 | 2770 | 4778 |

top helpers (on): 
top symbols (on): 0x00007fbf202fd3d8 32.7%, 0x00007fbf202fd3af 19.6%, 0x00007fbf202fd3f7 12.6%, 0x00007fbf202fd426 8.4%, lua_proto@0x7fbf203ff5e0 3.7%, 0x00007fbf202fd3ca 2.5%, 0x00007fbf202fd3ec 2.3%, 0x00007fbf202fdaba 1.6%
bytecodes (hook count, JIT off): 850.3 M; interpreter self cycles per bytecode (JIT off) 5.62; ESTIMATED share of bytecodes still interpreted with traces on: 0% (raw 0%)
stats: recorded 4 installed 3 entered 4483022 flushes 3 rerecord 0 blacklisted 0 refused 0 deferred 0 ineffective 0 / aborted InnerLoop: 1 / exits 2240846 (live traces 0)

### spectral_norm
| category | trace on | J4 only | JIT off |
|---|---|---|---|
| trace code |  92.6% |   0.0% |   0.0% |
| tier-1 (J4) code |   0.7% |  99.4% |   0.0% |
| JIT compile |   1.0% |   0.0% |   0.0% |
| interpreter dispatch |   5.0% |   0.4% |  99.9% |
| total Mcyc (perf, sampled) | 252 | 2769 | 5033 |

top helpers (on): 
top symbols (on): 0x00007f08d709e146 21.9%, 0x00007f08d709ff3f 20.6%, 0x00007f08d709e133 15.9%, 0x00007f08d709ff2c 11.5%, vm_unsafe::run_mode::<false> 5.0%, 0x00007f08d709ff3b 2.0%, 0x00007f08d709e142 2.0%, 0x00007f08d709e166 1.6%
bytecodes (hook count, JIT off): 680.3 M; interpreter self cycles per bytecode (JIT off) 7.39; ESTIMATED share of bytecodes still interpreted with traces on: 0% (raw 0%)
stats: recorded 5 installed 5 entered 78357 flushes 5 rerecord 1 blacklisted 0 refused 0 deferred 3 ineffective 1 / exits 41513 (live traces 0)

### loops_arith
| category | trace on | J4 only | JIT off |
|---|---|---|---|
| trace code |  99.1% |   0.0% |   0.0% |
| tier-1 (J4) code |   0.5% |  99.9% |   0.0% |
| interpreter dispatch |   0.0% |   0.0% | 100.0% |
| total Mcyc (perf, sampled) | 690 | 2087 | 4825 |

top helpers (on): 
top symbols (on): 0x00007f422ed02bc2 16.7%, 0x00007f422ed02b95 16.1%, 0x00007f422ed03738 15.6%, 0x00007f422ed02bcd 8.9%, 0x00007f422ed02b98 8.9%, 0x00007f422ed03730 4.3%, 0x00007f422ed02bd3 3.8%, 0x00007f422ed031e5 2.0%
bytecodes (hook count, JIT off): 740.0 M; interpreter self cycles per bytecode (JIT off) 6.52; ESTIMATED share of bytecodes still interpreted with traces on: 0% (raw 0%)
stats: recorded 3 installed 3 entered 6665918 flushes 3 rerecord 0 blacklisted 0 refused 0 deferred 0 ineffective 0 / exits 6665902 (live traces 0)

### json_roundtrip
| category | trace on | J4 only | JIT off |
|---|---|---|---|
| tier-1 (J4) code |  16.8% |  12.9% |   0.0% |
| JIT compile |   0.5% |   0.2% |   0.0% |
| interpreter dispatch |   3.6% |   5.4% |  29.0% |
| metamethod paths |   0.8% |   0.5% |   0.6% |
| call machinery |   1.6% |   1.5% |   1.5% |
| JIT helpers |  12.3% |  16.8% |   0.0% |
| allocator |   3.1% |   1.4% |   2.5% |
| string interning |   3.9% |   3.4% |   6.5% |
| GC |   3.1% |   3.5% |   2.5% |
| table internals |   6.3% |   8.2% |   8.1% |
| number<->string |   1.8% |   1.6% |   1.1% |
| stdlib |  34.8% |  34.3% |  42.8% |
| libc (memcpy etc.) |  10.8% |  10.1% |   4.9% |
| total Mcyc (perf, sampled) | 665 | 663 | 714 |

top helpers (on): jit_get 5.5%, jit_call 5.1%, jit_len 0.8%, jit_set 0.6%, jit_return_close 0.1%
top symbols (on): lua_proto@0x560dc18d4380.v3 8.9%, <pattern::MatchState>::skip 5.8%, jit_helpers::jit_get 5.5%, lib_string::str_find_aux 5.2%, jit_helpers::jit_call 5.1%, <heap::Heap>::new_str 3.9%, <state::State>::push_vec 3.5%, lua_proto@0x560dc18d3f00.v3 3.3%
stats: recorded 22 installed 1 entered 0 flushes 1 rerecord 1 blacklisted 8 refused 0 deferred 1 ineffective 0 / aborted CFunc: 10 / aborted FnShort: 7 / aborted Identical: 1 / aborted LeaveLoop: 1 / aborted Nyi(Call): 2

### template_engine
| category | trace on | J4 only | JIT off |
|---|---|---|---|
| tier-1 (J4) code |   0.8% |   1.1% |   0.0% |
| interpreter dispatch |  11.5% |  12.4% |  13.1% |
| call machinery |   7.2% |   7.1% |   6.6% |
| JIT helpers |   1.8% |   1.4% |   0.0% |
| allocator |  10.1% |  10.7% |   8.7% |
| string interning |   3.7% |   3.8% |   4.1% |
| GC |   8.7% |   7.7% |   9.0% |
| table internals |   6.6% |   6.8% |   6.4% |
| stdlib |  41.6% |  40.8% |  44.6% |
| libc (memcpy etc.) |   6.2% |   7.8% |   6.5% |
| other |   1.1% |   0.3% |   0.6% |
| total Mcyc (perf, sampled) | 988 | 987 | 955 |

top helpers (on): jit_newtable 0.6%, jit_closure 0.5%, jit_call 0.4%, jit_set 0.3%
top symbols (on): vm_unsafe::run_mode::<false> 8.4%, <pattern::MatchState>::classend 7.7%, <pattern::MatchState>::skip 6.3%, <value::Gc<func::CClosure>>::set_upvalue 5.7%, <pattern::MatchState>::m 5.3%, <state::State>::post_call 4.3%, pattern::gsub 4.0%, lib_string::gmatch_aux 3.6%
stats: recorded 14 installed 0 entered 0 flushes 0 rerecord 0 blacklisted 6 refused 0 deferred 0 ineffective 0 / aborted CFunc: 4 / aborted FnShort: 4 / aborted Nyi(Loadk): 4 / aborted TailCall: 2 / exits 0 (live traces 0)

### lua_interp
| category | trace on | J4 only | JIT off |
|---|---|---|---|
| tier-1 (J4) code |  11.0% |   6.6% |   0.0% |
| JIT compile |   1.3% |   1.5% |   0.0% |
| interpreter dispatch |  59.3% |  64.2% |  68.5% |
| call machinery |  15.2% |  11.2% |  18.4% |
| JIT helpers |   1.7% |   0.9% |   0.0% |
| GC |   0.5% |   1.0% |   0.0% |
| table internals |   9.4% |  12.6% |  11.9% |
| stdlib |   0.9% |   0.3% |   0.3% |
| libc (memcpy etc.) |   0.1% |   0.5% |   0.1% |
| total Mcyc (perf, sampled) | 229 | 226 | 249 |

top helpers (on): jit_tailcall 0.4%, jit_return_close 0.4%, jit_len 0.4%, jit_tforcall 0.4%
top symbols (on): vm_unsafe::run_mode::<false> 58.8%, lua_proto@0x7fecd65c5070 10.7%, <state::State>::post_call 10.0%, <value::Value>::raw_equal 4.3%, <state::State>::close 3.5%, <table::TableRef>::get 2.6%, <state::State>::pre_tail_call 1.7%, <table::TableRef>::len 0.9%
stats: recorded 8 installed 1 entered 101190 flushes 1 rerecord 0 blacklisted 2 refused 0 deferred 0 ineffective 0 / aborted Nyi(Return): 5 / aborted Recursion: 2 / exits 101158 (live traces 0)

### wordfreq
| category | trace on | J4 only | JIT off |
|---|---|---|---|
| tier-1 (J4) code |   2.3% |   5.7% |   0.0% |
| JIT compile |   1.7% |   0.0% |   0.0% |
| Lua compiler |   0.2% |   0.7% |   0.1% |
| interpreter dispatch |   5.0% |   4.0% |  15.3% |
| call machinery |   1.8% |   1.8% |   1.2% |
| JIT helpers |   5.2% |   9.2% |   0.0% |
| allocator |   0.9% |   0.6% |   0.6% |
| string interning |   5.1% |   9.9% |   8.8% |
| GC |   8.8% |   5.8% |  15.9% |
| table internals |   1.0% |   2.0% |   3.2% |
| stdlib |  49.3% |  44.0% |  40.2% |
| libc (memcpy etc.) |  17.9% |  15.6% |  14.5% |
| other |   0.3% |   0.6% |   0.0% |
| total Mcyc (perf, sampled) | 172 | 170 | 172 |

top helpers (on): jit_call 3.5%, jit_get 1.1%, jit_set 0.6%
top symbols (on): <pattern::MatchState>::max_expand 13.4%, <pattern::MatchState>::m 9.3%, 0x00000000001ae49e 9.3%, <value::Gc<func::CClosure>>::set_upvalue 7.5%, 0x00000000001ae4ab 6.9%, <pattern::MatchState>::skip 5.8%, <heap::Heap>::new_str 5.1%, lib_string::gmatch_aux 4.1%
stats: recorded 11 installed 0 entered 0 flushes 0 rerecord 0 blacklisted 2 refused 0 deferred 0 ineffective 0 / aborted CFunc: 2 / aborted UpvalAlias: 9 / exits 0 (live traces 0)

### game_loop
| category | trace on | J4 only | JIT off |
|---|---|---|---|
| trace code |  31.3% |   0.0% |   0.0% |
| tier-1 (J4) code |  52.0% |  84.6% |   0.0% |
| JIT compile |   6.2% |   0.9% |   0.0% |
| Lua compiler |   3.0% |   0.0% |   0.1% |
| interpreter dispatch |   1.9% |   1.1% |  98.6% |
| metamethod paths |   0.0% |   0.8% |   0.0% |
| JIT helpers |   0.0% |   5.8% |   0.0% |
| table internals |   2.1% |   4.3% |   0.5% |
| stdlib |   0.0% |   1.0% |   0.0% |
| libc (memcpy etc.) |   1.0% |   0.1% |   0.0% |
| kernel/unknown |   0.7% |   0.1% |   0.0% |
| other |   1.8% |   0.7% |   0.6% |
| total Mcyc (perf, sampled) | 62 | 117 | 225 |

top helpers (on): 
top symbols (on): lua_proto@0x7f4b007f0dc0 27.2%, lua_proto@0x7f4b007f1230 17.4%, lua_proto@0x558ac2191030 7.4%, 0x00007f4b006fa7b9 3.2%, 0x00007f4b006fd257 3.1%, vm_unsafe::run_mode::<false> 1.9%, 0x00007f4b006fd1e7 1.6%, 0x00007f4b006fa89d 1.6%
stats: recorded 11 installed 5 entered 146214 flushes 5 rerecord 0 blacklisted 1 refused 0 deferred 0 ineffective 0 / aborted FnShort: 4 / aborted UpvalAlias: 2 / exits 251542 (live traces 0)

### alloc_ctor
| category | trace on | J4 only | JIT off |
|---|---|---|---|
| trace code |   5.5% |   0.0% |   0.0% |
| tier-1 (J4) code |   0.3% |   9.7% |   0.0% |
| interpreter dispatch |   0.3% |   0.9% |  26.1% |
| call machinery |   0.6% |   1.1% |   1.2% |
| JIT helpers |   3.1% |  21.9% |   0.0% |
| allocator |  45.4% |  12.7% |   6.0% |
| GC |  18.0% |   8.4% |  10.7% |
| table internals |  26.2% |  45.2% |  55.2% |
| total Mcyc (perf, sampled) | 337 | 1134 | 1146 |

top helpers (on): jit_set 1.3%, jit_newtable 1.1%, jit_setlist 0.6%
top symbols (on): tr::asm::jit_tr_alloc 24.7%, <alloc::PoolAllocator as alloc::Allocator>::allo 14.8%, <table::TableRef>::len 11.2%, table::resize 8.7%, <heap::Heap>::free_obj 8.6%, <alloc::PoolAllocator as alloc::Allocator>::free 5.0%, <state::State>::gc_step 4.4%, <heap::Heap>::propagatemark 2.7%
stats: recorded 1 installed 1 entered 130091 flushes 1 rerecord 0 blacklisted 0 refused 0 deferred 0 ineffective 0 / exits 130091 (live traces 0)

