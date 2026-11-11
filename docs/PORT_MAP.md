# Port map: upstream C file -> Rust module -> owner -> status
Status: todo / in-progress / ported / verified.

| Upstream | Rust | Owner | Status |
|---|---|---|---|
| lopcodes.h/.c | rn-core/src/ver/*/opcodes_gen.rs (generated), rn-core/src/instruction.rs | orch/A | tables generated; instruction.rs verified (5.5; 5.4 order cfg'd) |
| llex.c (+ luaO_chunkid) | rn-compiler/src/lexer.rs | A | verified (5.5 and 5.4; reserved set and `luaK_semerror` line cfg'd); parser differential (`rn-std/tests/parser_diff.rs`, ~50M cases): `lexerror` prints no "near" for token 0 (a NUL byte) - fixed |
| lparser.c, lcode.c | rn-compiler/src/v55/{mod,parser,codegen}.rs, rn-compiler/src/v54/{mod,parser,codegen}.rs (forked per version) | A | verified: 5.5 422/422, 5.4 500/500 corpus files identical listing + dump; parser differential vs the reference front end (accept/reject, messages, dumps, listings; ~25M cases per version, 0 mismatches) |
| luac.c (print.c part) | rn-compiler/src/listing.rs (+ examples/list.rs) | A | verified (5.5 and 5.4 cfg branches) |
| ldebug.c luaG_getfuncline | rn-core/src/debuginfo.rs `get_func_line` (listing.rs keeps its own copy over `ProtoData`) | A | verified (agrees with listing walk on a real chunk; 5.5 and 5.4) |
| ldump.c, lundump.c | rn-compiler/src/{dump,undump}.rs | A | verified for 5.5 and 5.4 (cfg branches; 984 5.4 chunks round-trip byte-identical); parser differential: binary chunks (whole, every-prefix truncations, header mutations) identical errors/re-dumps |
| lobject.c (str2num, tostring, arith helpers) | rn-core/src/number.rs, value.rs | B | verified (5.5 and 5.4 differential vs ref binaries; 220k tostring, 60k str2num) |
| lstring.c | rn-core/src/string.rs, heap.rs (string table, interning + dead-string resurrection, external strings; luaS_new strcache not ported) | B | verified (unit + miri + GC stress) |
| lmem.c | rn-core/src/alloc.rs | B | ported (Allocator/Mem/RawBuf; unit-tested + miri) |
| lgc.c (G1: stop-the-world mark & sweep, weak/ephemeron, finobj/tobefnz) | rn-core/src/gc/mod.rs | B | ported for tables+strings (unit/stress/miri); incremental+generational = later; closures/upvalues/protos/userdata/threads = B3 |
| ltable.c, ltable.h (5.5; 5.4 deltas cfg'd) | rn-core/src/table.rs | B | verified (400-script differential: `#t` + exact `pairs` order vs refrun seed 0, 5.5 and 5.4; model tests; miri) |
| ltm.h/ltm.c (TM enum, names, fast-flag cache) | rn-core/src/meta.rs, `Heap::fast_tm` in gc/mod.rs | B | ported (fast events + the `luaT_*` call paths in meta.rs, B3) |
| lstate.c, ldo.c (stack, calls, protected calls) | rn-core/src/{state,call}.rs | B | ported (calls, precall/poscall/pretailcall, hooks plumbing, stack grow/shrink, CI list, pcall/closeprotected, call_k/pcall_k, load via Loader); `resume`/`yield`/`finishCcall`/`unroll` = coroutine task |
| lfunc.c, lobject.h objects (Proto, LClosure, CClosure, UpVal, Udata) | rn-core/src/{func,udata}.rs | B | ported (tbc list is a fallible index stack instead of stack-slot deltas) |
| ltm.c (lookup, calls, varargs) | rn-core/src/meta.rs | B | ported (`luaT_*`; error texts via error.rs, `VarInfoFn` hook for variable names) |
| ldebug.c (errors subset: runerror, errormsg, type/op/order/concat errors) | rn-core/src/error.rs | B | ported; variable names / getinfo / traceback = A's debug task (`debuginfo.rs`) |
| lgc.c (threads, closures, protos, upvalues, userdata; GCTM, closing) | rn-core/src/gc/obj.rs, collect.rs, state.rs (`close_state`) | B | ported for the stop-the-world collector |
| lvm.c | rn-core/src/vm.rs (B4); `stubvm.rs` = small test-only executor | B | todo (B4) |
| ldebug.c (pure parts: `getbaseline/luaG_getfuncline`, `findsetreg`, `basicgetobjname/getobjname/isEnv/rname/kname/rkname`, `funcnamefromcode`, `formatvarinfo/varinfo`, `funcinfo`, `collectvalidlines`, `changedline`, `luaG_addinfo`, `luaG_errnnil` text), lfunc.c `luaF_getlocalname`, lobject.c `luaO_chunkid` | rn-core/src/debuginfo.rs (over trait `ProtoView`; `ProtoDataView` adapter for compiler output) | A | verified: 383 snippets vs reference messages / `debug.getinfo(1,'n')` (5.5 and 5.4); State-dependent parts (`lua_getinfo`, `getlocal`, hooks, traceback, `luaG_callerror` frame logic) = later task |
| lapi.c, lauxlib.c | rn-capi, rn-core auxlib | A/B (later) | todo |
| lstrlib.c pattern matching (`match`, `str_find_aux`, `gmatch`, `gsub`) | rn-std/src/pattern.rs | A | verified: 120k randomized cases vs reference (5.5 and 5.4; identical code in both) |
| lstrlib.c `str_format` + `quotefloat/addquoted/checkformat/getformat` | rn-std/src/format.rs (+ `cfmt.rs`: libc `snprintf` wrapper) | A | verified: 200k randomized cases (all conversions, flags, widths, errors) vs reference, 5.5 and 5.4 |
| lstrlib.c `str_pack/unpack/packsize` | rn-std/src/pack.rs | A | verified: 60k cases vs reference, 5.5 and 5.4 (maxalign 16 vs 8, int vs size_t limits cfg'd) |
| lutf8lib.c | rn-std/src/utf8.rs (+ `luaO_utf8esc`) | A | verified: 60k valid/invalid sequences, 5.5 and 5.4 (`offset` returns 2 values in 5.5) |
| lmathlib.c random (xoshiro256**) | rn-std/src/random.rs | A | verified: 120 seeds x 3300 outputs bit-exact (5.5 and 5.4); default seed is non-deterministic by design |
| loslib.c date/time (`os_date`, `checkoption`, `getfield` range check, `os_time` core, `difftime`) | rn-std/src/strtime.rs (libc FFI) | A | verified: ~290k `os.date` results (all specifiers, 1206 timestamps) + 12k `os.time` normalisations over UTC and 3 DST zones (New_York, London, Lord_Howe) vs reference |
| lauxlib.c (`luaL_Buffer`, arg checking), other l*lib.c (base, table, string simple funcs, math, io, os, coroutine, debug) | rn-std | A (later) | todo (needs VM) |
| lua.c, luac.c | rnlua | A (later) | todo |
| lua.h/lauxlib.h semantics for Rust hosts (Phase 8) | rustynail/src/{lua,stack,callback,shared,error}.rs | orch | verified: 24 integration tests + 6 examples + doctests, 5.5 and 5.4 |
