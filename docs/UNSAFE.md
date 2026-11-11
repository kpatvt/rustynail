# Unsafe inventory (Builder B; rn-std FFI rows by Builder A)

Every `unsafe` block carries a `// SAFETY:` comment; `unsafe_op_in_unsafe_fn` is denied workspace-wide.

| Module | Why unsafe | Invariants |
|---|---|---|
| `rn-core/src/alloc.rs` | raw `std::alloc` calls behind the `lua_Alloc`-style `Allocator`; `Mem::realloc/free`; `RawBuf` | (A1) `(ptr, old)` always describes a live block previously returned by the same `Mem` with exactly that size (null/0 for none); (A2) all blocks are 16-byte aligned (`BLOCK_ALIGN`); (A3) `Mem::total` equals the sum of live block sizes; (A4) failure leaves the old block untouched. `RawBuf<T: Copy>` owns one block of `cap * size_of::<T>()` bytes; owner must call `free` with the same `Mem`; elements are never dropped. |
| `rn-core/src/value.rs` | reads of the `Payload` union; `Value::as_string`; `from_gc` | (V1) the tag byte always identifies the active union field, and a collectable tag implies the payload is a pointer to a live GC object of the matching variant while the owning `Heap` has not freed it. Constructors that cannot prove this (`from_gc`) are `unsafe`. |
| `rn-core/src/string.rs` | `LuaString::as_bytes`, `StrRef::get` | (S1) `contents` points to `len` readable bytes (NUL-terminated for inline strings, per `lua_pushexternalstring` contract for external); (S2) inline strings keep bytes in the same allocation after the struct; (S3) `StrRef` is created only by the owning `Heap` and is valid until that heap frees/collects the object (a `StrRef` outliving its `Heap` is a contract violation; the `Eq`/`Debug` impls dereference it). |
| `rn-core/src/heap.rs` | object creation/linking, string-table chains, `Drop`, external string callbacks | (H1) every node on `allgc` was allocated by `self.mem` with `string::alloc_size(kind, len)` bytes and is freed only by `free_string`; (H2) `strt.hash` has `strt.size` (power of two) bucket heads, each null or a live short string chained through `hnext`; (H3) each short string is in exactly one bucket and on `allgc`; (H2t) the `hnext` word of a short string holds the chain link in bits 3..47 and, once the string is promoted (`promote_hash`), the low 19 bits of its string-table hash in bits 48..63 (`tag >> 3`) and bits 0..2 (`tag & 7`); every read of the link goes through `LuaString::hnext` (masks the tag), every write of an existing string through `set_hnext` (keeps it); this needs user addresses below 2^47 and 8-aligned strings (x86-64; other targets keep a plain pointer and recompute the index of a promoted string); `strtab_index` uses the tag only while `strt.size <= 2^19`, `Heap::verify` checks it; (H4) `ExtMem` strings run `falloc(ud, ptr, len+1, 0)` exactly once (on free, or immediately if the object allocation fails). |
| `rn-core/src/table.rs` | table layout: inverted array part + hint + tag bytes, node array with `lastfree` before it, shared immutable dummy node, all lookups/inserts/rehash through raw pointers | (T1) `array` is null iff `asize == 0`, else points into a block of `asize*9+4` bytes allocated from the heap `Mem` at `array - asize*8`; (T2) `node` points to `2^lsizenode` initialised nodes (or the dummy node when `flags & BITDUMMY`), with a `lastfree` slot in the 8 bytes before `node` iff `lsizenode >= 3`; (T3) node chains (`next` offsets) stay inside the node array; (T4) collectable keys point to live objects unless tagged `LUA_TDEADKEY` (then only compared by address); the dummy node is never written. `gcaccess` exposes node/array iteration to the collector. Pointers stored as keys/values must carry full provenance (derived from raw payload pointers, never from `&LuaString`). |
| `rn-core/src/gc/mod.rs` | raw traversal of the object graph, sweep/free, finalizer lists | (G1) every pointer reachable from `allgc`/`finobj`/`tobefnz`/`fixedgc`/gray lists/roots is a live object of this heap; (G2) between steps of an incremental cycle the tri-colour invariant holds while marking (`Propagate`/`Atomic`: no black object points to a white one, enforced by `barrier`/`barrier_back`; threads are always revisited in the atomic phase), in the sweep phases dead objects carry the other white and are not referenced by live ones, and in `Pause` every object is current white with no black/gray; in generational minor mode the age invariants replace the colour invariant (young objects are white, `OLD` objects point only to old objects, `OLD1`/`TOUCHED2` objects not to new ones; `Heap::verify_gen`); (G3) objects on `finobj`/`tobefnz` have `FINALIZEDBIT` and are not on `allgc`; sweeping (incremental cursor `sweep_prev`, generational boundary pointers `survival`/`old1`/`reallyold`/`firstold1`/`finobj*`) tracks positions as predecessor *objects* (never pointers into `Heap`) so `free_obj(&mut self)` cannot invalidate them; `check_finalizer` repairs the cursor/boundaries when it unlinks an object. `Marker::mark_object` is `unsafe` (caller passes a live object of the heap). `Heap::verify` checks G1/G2/G3 + string-table consistency in tests (`RUSTYNAIL_GC_VERIFY=1` runs it after every step; `RUSTYNAIL_GC_STRESS=2` steps at every safepoint). |
| `rn-core/src/value.rs` (Payload) | `pub(crate)` payload union accessors used by table nodes | payload (tag, bits) pairs always come from `Value::to_parts`; copying the union preserves pointer provenance. |
| `rn-core/src/meta.rs` | none | - |
| `rn-core/src/state.rs` | `Thread` allocation with a 16-byte prefix (`LUA_EXTRASPACE`), raw access to the thread/stack/`CallInfo` nodes through the `State` handle, stack reallocation with upvalue pointer fix-up, `Ci` references | (ST1) a `State`/`Gc<Thread>` points to a live thread of a live `GlobalState` (the handle holds no reference into the thread: every accessor is a short re-borrow, so the collector may walk a running thread through raw pointers); (ST2) `stack.cap() == stack_size + EXTRA_STACK`, all slots initialised once `stack_init` succeeded, `top <= cap`; (ST3) every open `UpVal.v` points into the stack buffer and is rebuilt on each reallocation (`realloc_stack` stashes offsets without provenance, then re-derives); (ST4) `ci` is `base_ci` or a node of the doubly linked list, nodes after `ci` are free, only `shrink_ci`/`free_thread` free nodes (CI1: a `Ci` handle is valid while it is `ci` or before it); (ST5) `twups == self` iff not in `GcState.twups`. Tests: `tests_state` (stack growth with open upvalues, CI list growth/shrink, threads, memory errors), Miri. |
| `rn-core/src/func.rs` | `Proto` arrays (`RawBuf`s), closure trailing arrays (flexible-array layout reached through the owning `Gc` handle's pointer), open-upvalue intrusive list, raw object creation | (F1) `LClosure.upvals`/`CClosure.upvalue` have `nupvalues` initialised slots (upvalue pointers may be NULL only before the next safepoint); (F2) open `UpVal.v` points into the owner thread's stack, list sorted by decreasing level, linked by `u.open.{next,previous}` (`previous` = address of the pointer that points to this node); (F3) closed `v == &u.value`; (F4) `Proto` buffers: `cap` initialised elements, pointer arrays NULL-filled before children are built. Trailing arrays are accessed only through `Gc::as_ptr()` (full provenance), never through `&LClosure`. |
| `rn-core/src/udata.rs` | userdata layout `[hdr][nuvalue][len][metatable][gclist][uv..][payload]`, payload accessors | (U1) block size `UDATA_HEADER + 16*nuvalue + len`, payload offset multiple of 16 (so `max_align_t`-aligned), user values initialised (nil); (U2) `metatable` null or live table. |
| `rn-core/src/gc/generational.rs` | young collections: age-ordered sweeps of `allgc`/`finobj` between boundary pointers, `sweep2old`, gray-list correction through the objects' `gclist` links | G1 plus the generational boundaries: `allgc` is ordered `[new][survival][old1][reallyold]` with `survival`/`old1`/`reallyold` null or members of the list; ages live in the low 3 bits of `marked`. Positions are predecessor objects (null = list head). |
| `rn-core/src/gc/gen54.rs` (5.4 only) | 5.4 collector control: `whitelist` (rewrites `marked` of every object of a list), `work54` (reads size fields of a gray object for the 5.4 work units); everything else calls the shared `mod.rs`/`generational.rs` primitives | G1; `work54` only reads `nuvalue`/`nupvalues`/buffer capacities/`stack_size` of a live object of a kind with a gray list (tables via `gcaccess::work54`, which reads the hint slot only when `BITRAS` is set, i.e. `asize > 0`). |
| `rn-core/src/gc/obj.rs` | traversal/free of closures, protos, upvalues, userdata, threads (`traversethread`, `remarkupvals`, `luaF_freeproto`) | G1 plus F1-F4/U1/ST1-ST3; the `twups` walk tracks its predecessor as an object pointer (never a pointer into `Heap`) so `markvalue(&mut self)` calls cannot invalidate it. `Heap::verify` (+ `verify_refs`) checks that closure/proto/upvalue/userdata/thread references point to live objects. |
| `rn-core/src/call.rs`, `error.rs`, `collect.rs`, `meta.rs` | no `unsafe` except `savedpc` pointer arithmetic in `hook_call` (pointer stays inside/at the end of the proto's code) and the object-pointer dereferences in `gctm` | `savedpc` always points into the running proto's `code` buffer (or one past the instruction being executed). |
| `rn-core/src/value.rs` (B3 additions) | `Gc<T>` handle (`Deref`), `Value::native`/`as_native` (fn pointer <-> payload), typed constructors/accessors | (V2) every `LUA_VLCF` payload was created by `Value::native` from a `NativeFn` (the only constructor), which makes the `transmute` in `as_native` sound; typed constructors take `Gc<T>` handles, which prove liveness (V1). |
| `rn-core/src/stubvm.rs` | `savedpc` pointer reads/offsets | Test-support interpreter: every jump/skip stays inside the function's code (the compiler guarantees a terminating RETURN); replaced by B4's `execute`. |

| `rn-std/src/cfmt.rs` (Builder A, A4) | FFI: libc `snprintf` for `string.format` numeric/string conversions (bit-exact `%a/%g/%e/%f/%d...`, same as the reference) | (C1) every wrapper first validates `form` as exactly one `%`-spec with only `-+#0 ` flags, <= 3 width/precision digits, the expected length modifier (`ll` for integers) and a conversion letter from the allowed set for the argument type (`valid_form`), so no `*`, `n` or mismatched variadic type can occur; (C2) the format is NUL-terminated, `%s` arguments are NUL-terminated copies without embedded NULs; (C3) the output buffer is live for `cap` bytes and re-run with the exact size if truncated. |
| `rn-std/src/strtime.rs` (Builder A, A4) | FFI: `localtime_r`, `gmtime_r`, `mktime`, `strftime`, `difftime`, `time`, `tzset`, and a zeroed `libc::tm` | (D1) all pointers refer to live locals; `tm` is either fully zeroed or filled by libc; `tm_zone` is only ever set by libc; (D2) `strftime` gets a validated single conversion (`check_option`) and a 250-byte buffer, the returned length is clamped; (D3) `tzset` is declared locally (`libc` crate lacks it). |

`rn-std`'s other modules (`pattern`, `format`, `pack`, `utf8`, `random`) and `rn-core/src/debuginfo.rs` are `#![forbid(unsafe_code)]`.

`number.rs` is entirely safe code (no libc; `%g`, `strtod` and hex-float parsing are implemented in Rust and verified differentially against the reference binaries).

| `rn-std/src/io_ffi.rs` (Builder A, A5 LIBS2) | FFI: libc `FILE*` stdio (`fopen/popen/tmpfile/fclose/pclose/fread/fwrite/fflush/getc/getc_unlocked/ungetc/flockfile/fseeko/ftello/setvbuf/ferror/clearerr`), glibc `stdin/stdout/stderr` statics, `remove/rename/system/clock/mkstemp/setlocale/exit`, `__errno_location`, and raw read/write of the `LStream` record (`luaL_Stream` layout `{FILE *f; closef}`) in a userdata payload | (I1) a `CFile` is created only here (`fopen/popen/tmpfile`, std streams) and `lib_io` never uses it after the stream's `closef` was cleared (`aux_close` clears before calling `fclose`/`pclose`), so every wrapper receives a live `FILE*`; (I2) `lock`/`unlock` are paired in `read_number`/`read_line`, `getc_unlocked` only between them; (I3) all C strings are `CString` copies (NUL-free, else the call fails with an errno); (I4) `read_stream/write_stream` access `size_of::<LStream>()` bytes of a 16-aligned userdata payload created with that size (checked by `len()` in `test_file`); (I5) `exit` runs libc atexit/stdio flush, never returns. `lib_io`, `lib_os`, `lib_package` are `#![forbid(unsafe_code)]`. |
| `rn-capi/src/*.rs` | the exported `lua_*`/`luaL_*` C functions: raw `lua_State*`/pointer arguments, `transmute` of stored C function pointers, raw buffers of `luaL_Buffer`, unwinding (`resume_unwind`) across C frames | (C1) every `lua_State*` is the address of a live `Thread` of this library (the `State` handle built from it is a plain handle: several may exist for one thread); (C2) C function pointers are stored only from `lua_CFunction`/`lua_KFunction`/`lua_Hook` values (`LUA_VLCFC` payload, `CClosure.cfn`, `CallInfo.ck`, `Thread.chook`) and transmuted back to the same types; (C3) a Lua error raised by an API function unwinds as a [`Throw`] panic that only ever crosses C frames plus one API frame and is caught by the trampoline that called the C code (`catch`), which turns it back into `Err(status)`; with no catcher (`DEPTH == 0`) the panic function runs and the process aborts; (C4) strings returned to C point into the string object (inline, NUL-terminated) and stay valid while the value is on the stack/anchored, names handed out in `lua_Debug`/`lua_getlocal` are interned forever; (C5) `luaL_Buffer`'s `b` points to the inline `init.b` or to a box userdata on the stack (reference algorithm). Struct layouts are checked against the C headers (`tests/capi.rs::struct_layouts_match_the_c_headers`). The whole API contract is "the caller obeys the reference API rules" (valid indices, stack space), documented once in the crate docs. |
| `rn-capi/csrc/shim.c` | C code (not Rust): `va_arg` extraction for the variadic `lua_pushfstring`/`lua_pushvfstring`/`luaL_error`/`lua_gc` | Only extracts arguments and calls Rust (`fmt.rs`, `lapi_call.rs::rn_gc`); built by `build.rs` with the `cc` crate; compiled with `-fexceptions` so errors unwind through it. |
| `rn-std/src/dl_ffi.rs` | `dlopen`/`dlsym`/`dlclose` for `package.loadlib` and the C searchers | (D1) a `DlHandle` is created only by `open`, kept in the registry table `_CLIBS`, and closed once by that table's `__gc`; `lib_package` (which forbids `unsafe`) only uses the safe wrappers. |

## Miri (2026-10-28)

`scripts/miri.sh [--lua-version 5.5|5.4] [--borrows sb|tb] [--jobs N] [SUITE...]` (nightly `cargo miri nextest run`:
one process per test, so leaks are checked per test; memory cap + timeout; logs in `target/miri-logs/`).

| Suite | What runs | Flags |
|---|---|---|
| `core-lib` | all `rn-core` unit tests: alloc, value, string, heap/string table, tables (model tests), GC (incremental, generational, stress and random workloads with `Heap::verify`), `State` layer with the stub VM (stack growth with open upvalues, CI list, threads, tbc, userdata, errors) | none (isolation on) |
| `core-it` | `rn-core` integration tests: `table_diff` runs our side of its first 40 scripts under Miri | none |
| `compiler` | `rn-compiler` unit tests + `load_mode_and_stub`, `deep_nesting_is_limited_not_overflowing` | none |
| `interp` | `rn-std/tests/interp_scripts.rs`: the 111 FFI-free `tests/vm` scripts (103 under Miri, see below; 5.4 skips those its compiler rejects) through the real compiler, interpreter (`vm_unsafe.rs`), calls/coroutines/hooks/metamethods/tbc, string/table/math/utf8/debug libraries; 12 shards x {incremental, generational, incremental on `PoolAllocator`} | `-Zmiri-disable-isolation` (scripts are read from disk, `make_seed` reads the clock), `-Zmiri-deterministic-floats` |
| `interp-s1` / `interp-s2` | the inc/gen shards with `RUSTYNAIL_GC_STRESS=1` (full GC at every safepoint) / `=2` (one step or young collection per safepoint) | + `-Zmiri-env-forward=RUSTYNAIL_GC_STRESS` |

Gated or reduced for Miri (never weakening native runs):
- Pre-existing `cfg!(miri)` sizes in `rn-core`: `number.rs` round-trip loop 200 iterations (200k native), `tests_state` 12
  coroutine rounds (200), `tests_table` model test 3 seeds x 150 ops (400 x 600), `heap.rs` string-table test 300 strings
  (5000), `tests_gc` workloads 300-400 ops and the bad-collection loop 3000 (60000).
- `rn-core/tests/table_diff.rs`: under Miri only our side of 40 of the 400 scripts (the reference is a subprocess).
- `rn-core/tests/number_diff.rs` (2 tests) and 14 `rn-compiler` integration tests: `cfg_attr(miri, ignore)`, they spawn
  `luac`/`lua` or read the corpus (Miri cannot spawn processes; `number.rs` and the compiler are safe code, and the compiler
  is exercised by the `interp` suite).
- `interp_scripts`: scripts using `io`/`os`/`package`/`string.format` are excluded in every mode (libc FFI: Miri cannot run
  `snprintf`, `FILE*` statics, `dlopen`); under Miri also `err_15_select_and_misc_native_errors.lua`, whose
  `string.rep('x', 1 << 62)` expects a failing `malloc` ("not enough memory"), which Miri reports as resource exhaustion
  and aborts; and 7 scripts that cost 25M-900M native instructions each (hours under Miri: `err_08`/`err_09` stack
  overflows, `heavy_tail_02_million`, `co_09`, `for_09`, `tail_01`, `strings_04`), listed in `MIRI_SKIP`.
- `-Zmiri-deterministic-floats`: without it Miri adds random error to float intrinsics (`powf`), so `2^31 | 0` failed with
  "number has no integer representation" (Miri semantics, not a bug).
- Not runnable under Miri at all (FFI): `rn-capi` (C callers, `csrc/shim.c`), `rn-std` `io_ffi`/`cfmt`/`strtime`/
  `dl_ffi`, `rnlua`. These are covered by ASan (`scripts/asan.sh`) instead.

Results (2026-10-28, nightly Miri 2026-10-26, `--jobs 4`, two versions in parallel on 16 cores; every run: no UB, no
leaks, no unsupported operation):

| Suite | 5.5 stacked | 5.5 tree | 5.4 stacked | 5.4 tree |
|---|---|---|---|---|
| `core-lib` | 88/88 (213 s) | 88/88 (245 s) | 60/60 (200 s) | 60/60 (252 s) |
| `core-it` | 1/1 (247 s; 5 ignored) | 1/1 (344 s) | 1/1 (272 s) | 1/1 (318 s) |
| `compiler` | 7/7 (33 s; 14 ignored) | 7/7 (311 s) | 7/7 (29 s) | 7/7 (290 s) |
| `interp` | 36/36 (1396 s) | 36/36 (1984 s) | 36/36 (1252 s) | 36/36 (1894 s) |
| `interp-s2` | 24/24 (784 s) | - | 24/24 (928 s) | - |
| `interp-s1` | 24/24 (2014 s) | - | 24/24 (2008 s) | - |

(5.4 `core-lib` has fewer tests: `tests_state` and the stub-VM tests are 5.5-only.) Findings: none in our code. Two Miri
artefacts were handled as described above (float imprecision in `powf`, resource exhaustion on a 4 EiB allocation);
`nextest` drops tab characters from captured output, which made a correct `table.concat(t, "\t")` look wrong in a failure
report (verified with `--nocapture`).

## rn-jit executable memory (`crates/rn-jit/src/mem.rs`)

All `unsafe` of the JIT memory manager lives in this file. Invariants:

- **W^X:** memory is mapped `PROT_READ|PROT_WRITE` and only flipped to `PROT_READ|PROT_EXEC` by `CodeArena::commit`
  (one `mprotect` per chunk). Within a chunk the RX region is `[0, sealed_end)` (page aligned) and the RW region is
  above it; a page is never both. A chunk whose blocks were all freed is pooled *still RX* (the flip back to RW is
  deferred to reuse, `revive`) or unmapped; pooled chunks beyond 1 MiB are `madvise(MADV_DONTNEED)`-trimmed. The
  tests parse `/proc/self/maps` after every operation of a mixed workload (no `rwx`, RX below / RW above `sealed_end`).
- **Partial commit:** `commit` processes chunks one by one; on an `mprotect` error the already committed chunks stay
  committed, the failing and remaining ones stay pending and writable (`entry` => `NotCommitted`), so state is always
  consistent and the call may be retried (tested under `PR_SET_MDWE`, which makes every commit fail with `EACCES`).
- **Writes:** `CodeArena::writer` requires the block to lie above `sealed_end`; the `&mut CodeBlock` gives unique
  access and the shared arena borrow prevents `commit`/`free` while the writer lives. Padding and fresh blocks are
  `int3` filled, as is the tail of the last committed page. `CodeWriter::write_at` panics (with a message) and
  `try_write_at` errors on out-of-range or overflowing offsets.
- **Limits:** the cap bounds *mapped* bytes (every chunk is charged its full length on map, refunded on unmap;
  pooled/quarantined chunks stay charged). `CodeArena::new` defaults to 256 MiB, a block is at most 64 MiB, chunk
  sizes are clamped to `[page, 1 GiB]`, all size arithmetic is checked (`align_up` returns `Option`).
- **Handles:** `CodeBlock` is move-only; `free` validates arena id, chunk generation (re-stamped whenever a chunk is
  released, so a reused slot or re-pooled chunk never validates an old handle) and live offset (double free /
  foreign block => `InvalidBlock`). `entry` checks `size_of::<F>() == size_of::<usize>()` at compile time.
- **Threads (contract, not a type-system guarantee):** the arena is `Send` and `!Sync`, but that only serialises its
  bookkeeping and `mprotect` calls. A function pointer obtained from `entry` is a plain `Copy` value that any thread
  can call; nothing in the types stops a thread from executing a block that another thread frees, or code of an arena
  that is dropped (`Drop` unmaps all chunks *including live code*). `free`/`entry` are `unsafe` for this reason: the
  caller guarantees no thread executes in a freed block or after the arena is dropped, and that `F` matches the code.
- **Quarantine:** `set_quarantine(true)` / `RUSTYNAIL_JIT=verify` makes fully freed chunks `PROT_NONE`, never reused or
  unmapped until drop, so stale calls fault (`SIGSEGV`) deterministically (tested in a child process).
- **Probe:** `probe_exec` maps RW, writes `ret`, flips to RX, calls, unmaps (cached; never RWX), and fails gracefully
  under MDWE/SELinux `execmem`.
- **Perf map:** `/tmp/perf-<pid>.map` is opened `O_NOFOLLOW`, mode 0600, and must be a regular file owned by the
  current euid (else the map is disabled); after `fork` a map from `open_current`/`from_env` reopens for the new pid.
  Symbols are never unregistered.
- Tests (`cargo test -p rn-jit`): `probe_works`, `call_functions`, `padding_and_tail_are_int3`,
  `commit_batches_mprotect`, `rest_of_chunk_reused_after_commit`, `cap_exceeded`, `a1_*` (cap vs mapped),
  `a4_*`, `a5_*`, `a6_*` (quarantine), `a7_*` (perf map), `a8_*`, `alloc_align_32_and_oversized`,
  `free_and_reuse_chunk`, `double_free_and_foreign_block_are_errors`, `stale_handles_are_invalid_after_reuse`,
  `stress_many_blocks_no_leak`, `wx_invariant_in_proc_maps`, `faults_on_write_to_committed_and_call_uncommitted`,
  `commit_failure_under_mdwe_is_clean`, `assembler_to_arena_integration`, `probe_fails_under_mdwe` (child tests
  re-exec the test binary). All are `ignore`d under Miri (raw `mmap`/`mprotect`/machine-code execution are
  unsupported); ASan covers them.

## rn-jit compiler (`crates/rn-jit/src/{emu,ir,lower,templates_arith,rt,x86}.rs`, feature `jit`; J1a/J1b)

`ir`, `lower`, `templates_arith` and `x86` are safe code (IR construction, bytecode lowering, byte emission). `unsafe` lives in
`emu` (raw memory accesses of the emulator), `rt` (calling generated code; the `Proto.jit` descriptor pointer) and,
in rn-core, the hooks under `cfg(feature = "jit")` (`jit_hooks.rs`, the `'frame` head and `hot_edge!` in
`vm_unsafe.rs`, `GlobalState.jit`, `Mem::charge_ext`). Invariants:

- **J-ENTRY (one entry mechanism):** compiled code is entered only by `jit_hooks::enter` from the fast interpreter copy's
  `'frame` head, when `ci.trap == TRAP_JIT` (set by `jit_hot_call` on a fresh frame or by the loop hook before an OSR
  restart), `hook_mask() == 0` and the interrupt flag is clear. The flag is reset to 0 before entering. The hooked copy
  turns a stray `TRAP_JIT` into an ordinary trap. A flag that is never consumed is harmless (`fast_trap` clears it).
- **J-REGS (generated code ABI):** the `jit_enter` thunk (`x86::thunk_code`) is `extern "C" fn(thread, ci, frame_base,
  cnt, target) -> u32`; it pushes `rbp rbx r12 r13 r14 r15`, loads `r12 = thread`, `r13 = ci`, `rbx = frame base`
  (`func + 1`, computed by `rt::enter` after any stack move), `r15 = &cnt` (instruction budget word, a field of the
  boxed per-state `JitData`), `r14 = ci.kcache`, and jumps to `target`; every `Ret(code)` pops the same registers
  (`x86::epilogue`) and returns `code` in `eax` (0 Continue, 1 FrameChanged, 2 Returned; anything else aborts).
  The thunk starts with `push rbp; mov rbp, rsp` (frame-pointer chain). Compiled code calls nothing (no helpers yet), so
  no panic can unwind through it (P1) and the frame base stays valid for the whole run (the stack cannot move).
- **J-MEM (what compiled code may touch):** register slots `[base, base + maxstacksize)` and `base - 16` (the function
  slot, RETURN and GETUPVAL/SETUPVAL, which also read the closure's upvalue array and `UpVal.v`, i.e. the value an upvalue
  points to; SETUPVAL stores only non-collectable values, so no barrier is needed), constants `[kcache, +k.len)`, the `ci` fields `savedpc trap callstatus previous func kcache`, the thread's
  `ci` and `top`, the budget word, and the process-wide `INTERRUPT` flag. Offsets come from `rn_core::jit_layout` and
  `jit_hooks::thread` (`offset_of!`), never from literals.
- **J-EXIT (exit before side effects):** a template stores nothing before its last guard; every exit stores the address
  of the instruction to run in `ci.savedpc` (`code_addr + 4 * pc`, valid while the `Proto` lives, which is as long as
  its compiled code can run) and leaves with `Continue`. Trap/interrupt exits at back-edges also set `ci.trap = 1`.
- **J-DESC (`Proto.jit`):** `Cell<*mut u8>`; null = not compiled, `1` = do not compile, else a `*const JitProto`
  (`Box::into_raw`, see J-OWN below; the per-state `JitData` in `GlobalState.jit` is created lazily and freed through
  `VTable::free_state` when the state closes, which also unmaps the code arena). A proto belongs to one state, so the
  descriptor outlives every use. Since J1c the code is freed with its prototype.
- **J-CHARGE:** generated bytes are charged to the memory limit with `Mem::charge_ext` (limit only, not
  `collectgarbage("count")`); a refused charge is a transient failed compile (retried later, not NOJIT). Refunds: see
  "Charge accounting" under J1c.
- **J-EMU:** `emu::run` is `unsafe` with the same contract as the machine code (all computed addresses valid);
  `tests/emu_diff.rs` runs it under Miri (stacked borrows, permissive provenance for the integer addresses of
  `Lea`/`Mov`) and compares it with the native code on random memory images (600 cases: exit code, frame, thread,
  budget, `savedpc`, `trap`).
- **W^X / MDWE:** the arena is `mem::CodeArena` (see above); `JitData::new` runs `probe_exec` and a refusing host
  (`PR_SET_MDWE`) simply leaves the state in interpreter-only mode (`tests/thresholds.rs`, child process).

### J1c additions (calls, helpers, lifetime)

- **J-HELP (helpers):** `rn_core::jit_helpers::{jit_call, jit_tailcall}` are `unsafe extern "C" fn(thread, ci, instr, _)`; they
  rebuild a `State` handle from the pinned thread pointer (`State::from_th_ptr`; the interpreter's own `&mut State` is not used
  while compiled code runs) and call the same `pre_call*` functions as the interpreter. A panic aborts (P1). They run with
  `ci.savedpc` and `thread.top` set by the template. After any helper the stack may have moved: templates `ReloadFrame`.
  `Op::CallH` clobbers every virtual register; callee-saved `rbx r12-r15` survive.
- **J-FLAT (flat calls):** the inline `CallInfo` setup writes exactly the fields `pre_call_lua_fast` writes (`func top callstatus
  trap savedpc kcache`) into `ci.next` (a valid spare node by the list invariant), after the same stack-room test; `ci.next.previous
  == ci` already holds. Jumps go to `JitProtoHdr.entry0` or a resume address computed from a live descriptor
  (`base_m1 + table[byte offset]`); descriptors and tables are owned by the callee's `Proto` and outlive every frame of it.
- **J-OWN (descriptor ownership):** `Proto.jit` (> 1) is a `Box::into_raw(JitProto)` registered in the owner's `live` set; it is
  freed by `free_code` (called from `free_proto` under `cfg(feature = "jit")`) or, if the state closes first, by `JitData`'s drop
  (the arena's drop unmaps the code). `rt::enter` keeps no `&mut JitData` across the generated-code call (helpers compile and
  the collector frees code meanwhile).
  The descriptor lives exactly as long as its *prototype*, not the state: while a stay is running its frame can return
  flat into a compiled caller that runs the collector. `enter` therefore takes everything it needs after the call before
  it (an `Rc<Mon>`, the proto's code address as a plain key) and never dereferences `p`, the descriptor or the entered `ci`
  afterwards (the post-call frame is `ret.ci`). Debug builds quarantine freed descriptors (`dead` flag, `JitProto::mon()`
  asserts) so a stale use fails deterministically (`tests/jit_review_uaf.rs`). The shared counter word (`r15`) is saved and
  restored around every entry (nested entries from helpers must not reset an outer activation's count).
- **J-PACK:** `CodeArena::reopen_tail` (mem.rs) makes the sealed last page of the current chunk writable again (RX -> RW) and marks
  the chunk pending; `rt::compile` calls it only at the start of a compile and always ends the compile with `commit` (re-seal);
  a failed re-seal aborts. W^X per page holds at every instant that generated code can run, because a compile executes no
  generated code and the owning thread is the only one that can run it. `CodeArena::free` `madvise(MADV_DONTNEED)`s the whole
  pages strictly inside a freed block (never a page shared with a neighbour). Tests: `tests/jit_lifetime.rs`.
- **J-STRESS:** `state::move_stack` (feature `jit`) re-allocates the stack block, fixes open upvalues, poisons and frees the old
  block; only reachable through `jit_hooks::stress_move_stack` when `RUSTYNAIL_JIT_STRESS=stackmove`.
- **Charge accounting (replaces J-CHARGE):** `Mem::charge_ext` up front for whole pages, `Mem::uncharge_ext` for the unused part
  after the commit, on every failure path and when the proto is collected.

### J2b additions (generic for, varargs, closing)

- **J-HELP2:** `jit_tforcall`, `jit_varargprep`, `jit_vararg`, `jit_close`, `jit_tbc`, `jit_return_close` follow J-HELP (safe Rust
  inside; the only unsafe is rebuilding the `State` from the pinned thread/ci pointers). They may run Lua code (`__close`,
  iterator calls), reallocate the stack and change `ci.func`/`ci.top` (VARARGPREP): templates `ReloadFrame` after each and keep no
  stack pointer across them. `savedpc` is the instruction after the opcode, `top` is as the interpreter's `Protect` leaves it (set by
  the helper). A non-`H_DONE` status is an `Error` exit (also for yields).
- **J-BUILTIN:** `jit_hooks::set_builtins(next, ipairs_aux)` stores two code addresses (set once by `rn_jit::register`). The helper
  and the inline `ipairs` step recognise the builtins by address only; an address of 0 disables the fast paths. The inline `ipairs`
  step reads the table through the T1 array layout (`asize`, `array`, tag at `array + 4 + k`, payload at `array - 8 (k + 1)`) after
  the table tag and the integer control are checked and `0 <= c < asize`; it writes only `R[A+3..A+2+C]`.
- **J-VARARG-FRAME:** a vararg frame has `func` moved above its extra arguments; `RETURN` with `C != 0` moves the results to
  `func - (nextraargs + C)` using the same arithmetic as `OP_RETURN`; the destination is below the frame and inside the stack.

### J2a additions (allocation / table helpers)

- **J-HELP2:** `jit_get jit_set jit_newtable jit_setlist jit_len jit_concat jit_closure jit_pow` (`rn_core::jit_helpers`) follow the
  J-HELP contract. Their `unsafe` is the raw operand access of the interpreter (`Value::load(kcache.add(n))` with a compiler-guaranteed
  constant index, `upvals_ptr().add(n)` with `n < nupvalues`, `as_lclosure().unwrap_unchecked()` on the running function slot,
  `strref_unchecked` on a constant that VM3 guarantees is a short string, `psetshortstr*`): the same obligations as the VM1-VM4
  entries, discharged by the same compiler guarantees. A helper returning `H_EXIT` has stored nothing to the Lua heap (it may have set
  `thread.top` and cached metamethod absence flags, both idempotent for the interpreter that redoes the instruction).
- Compiled code never holds a collectable pointer across these helpers (G1): operands are re-read from the frame inside the helper, the
  frame base is reloaded afterwards.

### J2b-5.4 additions (Lua 5.4 build)

- **J-ALIMIT:** in the 5.4 build, inline integer-key array accesses (GETI, GETTABLE, SETI, SETTABLE, the inline `ipairs` step of
  TFORCALL) emit `Cx::alimit_touch` after the `u < asize` bounds check. It reads `Table.flags` (`BITRAS`) and, when set, the `u32`
  length hint at `array + 0` (T1: the hint slot exists when `asize > 0`, and the bounds check has proved `asize > u >= 0`), and
  stores `u + 1` there under exactly the condition of `table.rs::alimit_touch` (`u >= lim && u & !(lim - 1) < lim`). The store is
  4 bytes into the live array block; no other table field is written. Mutation-checked by `table_diff` (tables 15-18) and
  `jit_table::array_limit_hint_sequences`.
- Layout constants added to `jit_layout::table`: `BITRAS` (5.4 only) and `LENHINT` (0). The 5.4 numeric/generic `for` templates and the
  5.4 `jit_tforcall`/`tfor_fast` offsets only address registers inside the frame (`A..A+6`, inside `maxstacksize` by the compiler's
  register allocation, as for every other template).

## J3b additions (hardening)

- **No new `unsafe`.** Per-state mode (`rt::set_mode`, `JitData::heuristic`) and the memory charging use the existing
  `data_ptr` accessor; the facade (`crates/rustynail`, `forbid(unsafe_code)`) only calls the safe functions
  `rn_jit::{set_mode, stats, available}`.
- **Memory accounting.** `rt::compile` charges `descriptor_bytes` (the `JitProto` box, its resume table, `Mon`, the live-set slot;
  refunded with the prototype through `free_code`'s return value) and `transient_bytes` (256 B per bytecode instruction for the
  lowering buffers; refunded at the end of every compile) with `Mem::charge_ext`, next to the code pages. A refused charge is
  `Fail::Retry` (interpreter keeps running). Invariant: after all prototypes are collected `Mem::ext_bytes()` is back to the
  pages of the arena (tests `jit_memlimit.rs`).
- **Sanitizers.** `scripts/asan-jit.sh` runs the `jit` build under ASan (generated code is not instrumented; the arena's
  anonymous mappings are outside ASan's shadow and need no special flags; `allocator_may_return_null=1` is needed for the
  tests that request absurd allocation sizes). `scripts/miri.sh jit` covers the pure-Rust parts (monitor, encoder basics, `emu_diff`; not `table_diff` nor the iced-x86 oracle tests).

## J4 (tier 1.5 regions; `crates/rn-jit/src/t15/**`, `rt.rs`, `emu.rs`, `rn-core/src/jit_hooks.rs`, `rn-std/src/lib_math.rs`)

Consolidated 2026-10-30 (tasks T3, T5, T6, T7, T8; T9/T10 skipped). Everything is behind `cfg(feature = "jit")`, tier 1.5 is off unless
`RUSTYNAIL_JIT_T15=on|always` (`T15Config::parse`; default `off`), and `+shadow` / `+exitstorm=N` / `+verify` / `+randra` exist only via
`RUSTYNAIL_JIT_OPT` (never default). Without the feature the `.text` of the non-JIT build is unchanged (`scripts/jit-text-identity.sh`); with the
feature and T15 off the tier-1 machine code is byte-identical (`scripts/jit-dump-identity.sh`). Design/as-built: `docs/JIT_TIER15.md`;
tool-to-mutant matrix: `docs/JIT_TIER15.md` section 22.4; test modes: `docs/JIT_TESTING.md`. Every `unsafe` block in the code carries a
`SAFETY:` comment; the entries below are the invariants behind them. `mut:` = mutants of `t15/mutants.rs` (feature `t15_mutants`, run by
`scripts/t15-mutants.sh`) that break the invariant on purpose.

### Rust `unsafe` items and shared state

| ID | Where | Invariant | Upheld by | Checked by |
|---|---|---|---|---|
| **J4-DATA** | `t15/runtime.rs` (`T15Data`, `NestState`, `VersionData`, `T15Proto`), embedded as raw addresses in warm-up and region code | Machine code embeds addresses of the promote counter (`NestState::ctr`, `Box<[NestState]>` of fixed length), `prof[pc]` (`Box<[Cell<u8>]>`), `T15Data::cont`, and (T7) `NestState::fail` (`i32` countdown) and `NestState::entry_ok` (`u32`). All live in boxes that are never moved or grown; every field is a `Cell`/`OnceCell`, so no `&mut` to them exists. Machine code does plain 32-bit RMW / 8-bit stores and reads `cont`; Rust uses the same words through `Cell`. One VM thread touches them. They are freed with the descriptor (`free_code`), after every code block (current and retired), so retired stubs only write into live memory. Per-version counters (`VersionData`) are boxed, indexed by version id, never reallocated. | `rt::compile`/`recompile` placement; `free_code` order | `tests/t15_runtime.rs` (nested activations, suspended coroutines, collection during a stay under `verify` quarantine, limits); ASan (`scripts/asan-jit.sh`); mut: `pol_drop_old_block` |
| **J4-RETIRE** | `rt.rs::recompile` / `recompile_with`, `T15Proto::retired` | The new block is placed like a first compile (`place_code`, charge up front, refund the unused part); `JitProto.block`, `resume`, `hdr.{entry0,base_m1,resume}`, `T15Proto.{pcmap,cur,versions}` are swapped in one infallible sequence (no generated code runs during a compile). The old `CodeBlock` goes to `retired` and stays mapped and charged until the prototype is freed: helpers called from old code (pcall, metamethod, iterator, GC) return into it. An activation in an older version that reaches a promoted back-edge calls `promote` with its own version id and is moved to the current code. | `rt::recompile`; `free_code` frees `retired` | `t15_runtime.rs`, `t15_policy.rs`, `t15_stress`; mut: `pol_drop_old_block` (detected by every tool) |
| **J4-CONT** | `jit_t15_promote`, `jit_t15_event` (`runtime.rs`, `extern "C"`) | Same contract as J-HELP: called only from generated code with the live thread pinned in `r12` and its current Lua frame in `r13`; the `State` handle is rebuilt by `jit_hooks::state_from_thread`, panics abort. `CallH` results are 32 bits, so the 64-bit continuation address is written to `T15Data::cont` and the stub does `Mov r,&cont; Ld r,[r]; JmpR r`. The address is `block.addr() + pcmap[pc]` of the *current* code, taken after the recompile and used at once; registers `rbx r13 r14 r15` stay valid because tier 1 keeps the frame exact. Raw derefs in the helpers (`&*jpp`, `&mut *rt::data_ptr(st)`) re-derive the descriptor after every recompile; no `&T15Proto` is held across one. **M9:** `pcmap[pc] == 0` marks dead pcs (operand words, a `JMP` consumed by a compare, `MMBIN*`); a request for such a pc, an out-of-range pc or a nonexistent version returns 0 and changes nothing. | stub emitter (`t15/stubs.rs`); helpers | `t15_runtime.rs`, `t15_policy.rs`, `t15_core`, jit-diff `t15*` modes; mut: `pol_stale_no_rearm`, `pol_countdown_64bit` |
| **J4-VER** | `runtime.rs` (`promote`, `event`) | Every helper takes the version id of the calling code. `promote` of an already `Promoted` nest is idempotent; `event` updates `versions[ver]` only and never counts an older id against a newer version; only the current version may trigger a recompile; a stale id only moves the activation to the current code. | helpers | `t15_policy.rs`; mut: `pol_stale_no_rearm`, `pol_no_fail_count` |
| **J4-SIG** | `rt::recompile` | `Lowered::sig` (`head`, `cnt_at`, `rem`, resume pcs, `entry_ok`, `interp_entry`) of every new version must equal version 0 (`debug_assert!` plus refusal of the new version in release: old code keeps running), so the tier-1 entry/return protocol is the same in all versions. | recompile | `t15_runtime.rs`, debug-assertion builds of the suites |
| **J4-CHARGE** | `rt.rs`, `runtime.rs::data_bytes`, `JitProto::charged` | All tier 1.5 memory is charged to the state's memory/code limits: `data_bytes` at first compile; transient buffers, new pages (unused part refunded) and one `VersionData` per recompile; everything is refunded by `free_code`. A refused charge (`Fail::Retry`) leaves old code running and re-arms the nest counter with back-off (`hot << k`, `k <= 10`, 4 random bits, then `Off`). Compile slots: `MAX_COMPILES = 6`, plus `GIVEUP_RESERVE = 2` for a give-up recompile (at most 8 blocks per prototype). | `rt.rs` | `tests/jit_memlimit.rs`, `t15_runtime.rs` (limits and caps), `t15_policy.rs`; mut: `pol_cap_no_giveup` |
| **J4-FB** | `runtime.rs` (`EventKind::StubFail`: `stub_fail`, `give_up`), failure pads in `stubs.rs` | Runs from a stub with the frame exact (every value is in its VM slot); reads the failing stub's guard tags through the safe `State::get`; may recompile (respecialise or give up). Continuation via `cont` (1 = `JmpR` to the tier-1/stub label of the failing pc in the *current* code, 0 = the caller's own label). | helper | `t15_policy.rs`, `t15_core`; mut: `pol_no_widen`, `pol_no_giveup`, `pol_no_penalty`, `pol_no_demote`, `pol_no_bool_widen`, `gate_upval_single` |
| **J4-MATH** | `t15/codegen/calls.rs::jit_t15_math` (re-exported by `t15/codegen.rs`), the `unsafe extern "C" fn` added for T6 | Contract as J4-CONT (live thread in `r12`, current Lua frame in `r13`). Rebuilds `State`/`Ci` with `state_from_thread`/`Ci::from_raw`, reads slot `func+1+slot+1` and writes slot `func+1+slot` and `thread.top` through the checked `State::get/set/set_top` (no raw slot pointer). Called only after a flush and after the inline guards (registered `math` function in the callee slot, number argument, no hook, `LUA_MINSTACK` free slots); allocates nothing, runs no Lua code, cannot move the stack. | region codegen (`classify_call`) | `t15_core_emu.rs::intrinsic_calls_match_tier1_at_every_pc`, `t15_core.rs` (`in_*`), jit-diff T6 templates; mut: `intr_*` |
| **J4-INTR-REG** | `rn-core/src/jit_hooks.rs` (`BUILTIN_INTR: [AtomicUsize; NUM_INTR]`, `set_intrinsics`, `intrinsic_addr`, `intrinsic_of`), `rn-std/src/lib_math.rs::jit_builtins` | Safe code only (`lib_math` keeps `forbid(unsafe_code)`; it casts function items to `usize`). Registered values are code addresses (never GC objects), set once in `rt.rs` at init and embedded as immediates; region code identifies a `math` call by comparing the light C function payload with them (0 = unknown, never matches). | `rt.rs` init (tests that call `set_intrinsics` themselves) | mut: `intr_no_identity`, `intr_wrong_identity`; `t15_core`, `t15_core_emu` |
| **J4-SHADOW-HELP** | `t15/verify.rs`: `frame_of`, `jit_t15_shadow_pre`, `jit_t15_shadow_post` (`unsafe extern "C"`); `emu.rs::run_until_shared` (`unsafe`, contract of `Emu::run`) | Contract as J4-CONT. The frame base is `State::slot_ptr(func + 1)`; `pre` copies `maxstacksize` slots (16 bytes each, inside the frame) into a thread-local buffer with 4 slots of padding on both sides and reads the budget word through `rt::cnt_ptr`; `post` runs the tier-1 `Program` on the copy via `run_until_shared` with `Env { frame: shadow buffer, ci/thread: live ones, k: constants, cnt: local i32 }`, then compares both frames slot by slot (raw reads inside `maxstacksize`). Only blocks whose acts are all pure templates are registered, so the emulated code only reads `CallInfo`/thread/constants; a helper `CallH`, trap exit or 20000 steps stops the emulation (inconclusive, counted). A mismatch aborts the process. | `codegen/shadow.rs::pure_act` | `t15_stress` `s_shadow*`, jit-diff `t15shadow*`, mutants marked **S** in JIT_TIER15.md 22.4 |
| **J4-SHADOW-REG** | `t15/verify.rs` (`REGISTRY: Mutex<Vec<Arc<ShadowBlock>>>`, thread-locals `PENDING`, `SHADOW`) | Blocks and their `Program` clones live for the life of the process (retired code may still call the helpers; debug aid, memory proportional to verified lowerings). Registry is mutex-guarded; shadow buffers are per thread. | by construction | `t15_stress`, ASan runs with `+shadow` |
| **J4-STORM** | `t15/verify.rs` (`STORM: [AtomicU32; 1 << 15]`, `STORM_NEXT`, `storm_counter`) | Region code embeds addresses of entries of a process-wide `static`, which outlives every code block. Sites are assigned round-robin modulo 32768 (after that sites share words: only the phase of a pseudo-guard changes). Generated code does a plain non-atomic 32-bit load/add/store, nothing else reads the words (benign race across threads). | by construction | `t15_stress` `s_storm*`, `s_budget_storm`, jit-diff `t15storm*`, ASan |
| **J4-EMU** | `emu.rs` (`load`/`store`/`run`, `run_until`, `run_until_shared`, `Emu::with_labels`) | The emulator dereferences raw addresses exactly as the machine code would; callers promise that frame, constants, `Env.cnt` and any addresses are valid for the run. J4 adds the `run_until*` entry points and the shared `Arc<Vec<usize>>` label table, no new kind of unsafety. Float semantics follow x86 SSE explicitly, so results do not depend on host or Miri. | tests / shadow helpers | `emu_ops_diff.rs`, `emu_diff`, Miri (`scripts/miri.sh jit`) |
| **J4-MUTANT-FREE** | `rt.rs::recompile_with` (`t15::mutants::active("pol_drop_old_block")`) | Test-only (feature `t15_mutants`): frees the old block on recompile via `CodeArena::free` in an `unsafe` block. Exists to prove the suites detect a return into freed code; never enabled otherwise. | feature gate | `t15_policy.rs`, `t15_core`, shadow, storm (all detect it) |

### Contracts of generated region code (no Rust `unsafe`; for the review of the memory-safety argument)

| ID | Where | Invariant | Checked by |
|---|---|---|---|
| **J4-TBL-READ** | `t15/codegen/tables.rs` | Inline table reads dereference the table (from its VM stack slot), its `array`/`node` pointers, one element or node, and, for the barrier test of a store of a possibly collectable value, the `marked` byte of the value's object. Each pointer is loaded in the same instruction from a slot (collectables are write-through, so the object is alive), checked first (`asize`, tag byte, key compare) and dropped at the end of the instruction; no helper call, allocation or GC point sits between load and last use and nothing is cached across instructions (R-DERIVED, G1). `array` is read only after `0 <= key-1 < asize`; the node vector is never null (dummy node); a payload is dereferenced only if its run-time tag has the collectable bit. | `t15_core_emu.rs` (synthetic `Table`/`Node` blocks from `Pool`; failures are crashes of the program under test, never UB in the library); mut: `tbl_no_lower_bound`, `tbl_no_upper_bound`, `tbl_no_tag_check`, `tbl_no_nil_check`, `tbl_no_key_tag`, `tbl_no_table_tag`, `tbl_no_coll_test`, `tbl_no_s0_reload`, `tbl_next_zext`, `tbl_const_off`, `tbl_widen_unchecked` |
| **J4-TBL-WRITE** | same | Stores write an existing array slot (`index < asize`) or the value of an existing node; never insert or resize. The tier-1 conditions for `__newindex` and the GC barrier are tested before the store, failing to the tier-1 template. No GC bit is written. | mut: `tbl_no_meta_check`, `tbl_no_nonnil_check`, `tbl_no_barrier` |
| **J4-INTR** | `codegen/calls.rs` (inline `math` forms) | Dereferences only the callee slot (tag, payload compared with the registered address), `thread.hookmask`, `thread.stack_size`, `ci.func`, `ci.next` (compared with 0, not dereferenced) and the interrupt flag; writes the result slot `A` of the running frame, `ci.savedpc` and `thread.top`. No helper call, allocation or GC point inside the inline form (libm functions go through `jit_t15_math`, J4-MATH). | mut: `intr_no_hook_check`, `intr_no_identity`, `intr_wrong_identity`, `intr_no_stack_check`, `intr_no_ci_check`, `intr_no_count`, `intr_abs_no_clear`, `intr_minmax_*`, `intr_floor_*` |
| **J4-UPV** | `codegen/calls.rs` (inline `GETUPVAL`) | Reads the closure from the function slot below the frame (`rbx - 8`, live: the frame holds it), its `upvals[b]` entry and `UpVal.v`, copies one tag/payload pair; the three pointers live in `rax` only until the end of the instruction. Open upvalues of this frame may alias stack slots (`debug.upvaluejoin`), so register-resident escaped slots are written back (`spill`) before the read. | mut: `up_no_spill` |
| **J4-FLUSH** | `t15/codegen*.rs`, `t15/stubs.rs`, `t15/alloc.rs` | Every exit writes back the dirty registers inline and jumps to the tier-1 label of the same pc; calls, GC points, hooks, errors and coroutine switches see an exact VM stack (flush before, resume stub after; JIT_TIER15.md 4.10 is the checklist). The budget (C1) word is stored before exits and reloaded after calls. | mut: `flush_skip_*`, `leave_*`, `exitmap_wrong_reg`, `fall_*`, `drop_guard_*`, `count_after_fall`, `no_budget_reload_after_call`, `stub_no_load_last`, `rr_join_union`, `pad_no_irq_poll`; options `+flushall`, `+verify`, `+randra`, `+exitstorm` |

P-V (string methods): two small additions. `jit_hooks::type_mt_changed` transmutes a `usize` stored by `set_type_mt_hook` back to a `fn(&mut State)` (stored only from that type; same pattern as the other registered addresses). `jit_get` calls `index_string_short_str` with `strref_unchecked` after checking `is_short_string` on the key. A trace holds the string metatable as a raw constant: valid because every assignment of `g.mt[string]` flushes all traces first (`type_mt_changed`), and the metatable stays alive through `g.mt` until then.

A1 (inline `NEWREF` of constant keys into a table the same trace allocated, `asm::newref_inline`, `rn-core::table::{plan_shortstr_insert, hash_image}`): the
generated code writes raw stores into a node vector the trace itself just allocated. Invariants: (1) the table is the result of a `TNEW` of the same trace
(`nr_plan` checks `ir.at(t).op == Tnew`), so it is white (no key barrier: `Heap::new_table` makes white objects, no collector step runs inside a trace), has
`2^lsizenode <= 8` nodes (`aux` encoding), is not the dummy node and no table of the trace can be reached by another thread; (2) the node vector is in the
state the recorder tracked (`Rec::nr_img`: the hash image after the last inline insertion, compared with the VM's table before each plan, and checked against
the VM's table after each insertion in `sync_newref`; a mismatch aborts the recording `Diverge`) and no `TSET` was recorded since the `TNEW`, so every run of
the trace holds the same keys, chain links, `lastfree` position and empty values as the recording did, and the stores (key tag + address, chain links, `lastfree`
pointer at `node - 8`, the header `flags` byte) only touch bytes inside the node block or the table header; (3) no guard separates an inline `NEWREF` from the
`HSTORE` that fills its value, so an exit never sees a key with an empty value. `plan_shortstr_insert` / `hash_image` are read-only and use the file's
existing `unsafe` plumbing (T2/T3). The unit test `tests_table::insert_plan_matches_the_real_insertion` compares plan and real insertion for 400 tables.

S1 (side traces from exits inside inlined frames, frames kept virtual): two new `unsafe` expressions and no new memory access pattern. (1) `tr/exit.rs::exit_entry`
(frame-cell branch): after the Rust restore rebuilt the frames, an exit whose cell holds a side trace's entry writes `thread.ci` (the root frame), `thread.top`
(`root.top`) and, when the restore counted metamethod frames, `nCcalls` (the value read before the restore) through the same `RestoreCtx` pointers `restore` uses; invariant:
the pointers are the live thread's fields and `ctx.root` is the live root `CallInfo` (the contract of `exit_entry`); the rebuilt `CallInfo` nodes stay allocated and unreferenced
(every later restore rewrites all their fields). The inline stub of such an exit (`asm::inline_stub`, generated code) writes only the snapshot slots (`Base::Frame` + slot
offset inside the extent the parent's entry checks covered) and the budget word, exactly like a frameless stub; it never touches a `CallInfo`. (2) `tr/record.rs::begin`:
`Ci::from_raw(root_ci)` to read the root closure of a side recording that starts in an inlined frame; the address is the root `CallInfo` of the armed recording, validated by
the VM at every step (`rec_validate`). A side trace entered with virtual frames has no stack-room / spare-`CallInfo` entry checks: its extent and depth are checked against
the parent's at install (`mod.rs`, `Compile(side-room)`), the parent's checks cover it (mutant `fside_room_ignored`).

H1 (integer keys in the hash part): one new `unsafe` expression, `rn-core::table::TableRef::int_node` (a read-only `getintfromhash` walk behind the `ikeyinarray` test, the same code `get_int` runs; the node found lies in the node vector). Generated code: the integer form of `HREFV` (`tr/asm.rs::hrefv`) reads the node vector at `hashint`'s main position `key % ((sizenode - 1) | 1)` (a `div` by an immediate-built modulus, so `rdx` = V8 is excluded from the allocation of such a trace, `ra::op_clobbers`) and follows `next` links; the recorder's guard `lsizenode == recorded` precedes it, so the modulus matches the node count the walk indexes (mutant `hrefi_lsize_skip`: with a smaller node vector the main position would lie outside it).

M1 (`RETURN` / `TAILCALL` with the close flag, nil array elements, side stitching): no new `unsafe` expression in Rust. Generated code (tier 1, `calls.rs::tpl_return` / `tpl_tailcall`) gained two reads: `thread.openupval` (the head of the open-upvalue list, offset `jit_hooks::thread::OPENUPVAL`) and, when it is not null, `upval.v` of that head (`jit_layout::upval::V`) compared with the frame base. Invariants: the list is sorted by level, highest first (`luaF_findupval`), so the head is the only upvalue that can lie at or above the frame base; the template runs on the thread that owns the frame (no other thread writes the list while it runs); the head and its `v` are live (an open upvalue is unlinked by `close` before its stack slot dies; a stack reallocation rewrites `v` in place before any template resumes). A head at or above the base, and every frame with the `TBC` status, takes the interpreter's helper exactly as before (mutant `tailfast_close_skip`). The recorder's argument that `OP_RETURN`'s `luaF_close` is a no-op in an inlined frame (no `CLOSURE` / `TBC` / `TFORPREP` is recorded in a frame the recorder entered) is not memory-related; it is covered by `tr_core::m1_interp_shapes`. The nil `ALOAD` reads the tag byte of an array slot exactly like the typed `ALOAD` (after the same `ULT` bound guard).

### Tools that check these invariants

`cargo test -p rn-jit --features jit` (also with `RUSTYNAIL_JIT_T15=always` and `--cfg rustynail_lua54`), `--test t15_stress` (opt-in; 23 option strings,
GC stress 1/2, stackmove), jit-diff modes `t15*` (`docs/JIT_TESTING.md`), `fuzz/fuzz_targets/jit_diff.rs`, `scripts/asan-jit.sh` (T15 corpus with `+shadow`,
`+exitstorm=7`), `scripts/miri.sh jit` (`t15::alloc`, analysis, emulator tests, `exit_storm_regions_match_tier1_emulated`; `t15_core_emu` tests that use
integer-to-pointer synthetic tables or need over an hour are filtered out: open item), `scripts/t15-mutants.sh` (matrix in JIT_TIER15.md 22.4).

## J5 (trace compiler; `crates/rn-jit/src/tr/**`, `rt.rs`, `lower.rs`, `rn-core/src/{jit_hooks.rs,vm.rs,vm_unsafe.rs,heap.rs,gc.rs,table.rs}`, feature `trace` / `jit_trace`)

Consolidated in task F6 from the per-task paragraphs that used to live in `docs/JIT_TRACE_REF.md` (F7a, F1, F2a, F2b, F3, F5, F3b, F4, S0, F8; the
P-era facts are taken from the code and the P-task decision lists there). Everything is behind `cfg(feature = "trace")` (rn-core: `jit_trace`) and the run-time
mode `RUSTYNAIL_JIT_TRACE` (default `off` at the time of writing); without the features the `.text` of the default build is unchanged
(`scripts/jit-text-identity.sh`, `scripts/jit-dump-identity.sh`). The `jit_trace` build of `rn-core` has larger `Mem`/`GcState` layouts (virtual bytes, watch
list) than the plain `jit` build, so the *fast-copy* symbol comparison shows moved displacements (same 6275 instructions): known and accepted. Every `unsafe`
block carries a `SAFETY:` comment; the entries below are the invariants behind them. Mutant names (`tr/mutants.rs`, runner `tests/tr_mutants.rs`) break the
named invariant on purpose.

### Rust `unsafe` items and shared state

#### Recorder hook (P1, P3a, F8)

`rn-core::jit_hooks` holds the recorder vtable (`RecVTable { step, abort }`) and `state_from_thread`; `vm.rs`/`vm_unsafe.rs` call it with the live thread, ci and
pc. The recorder (`tr/record.rs`) re-reads interpreter state after every instruction (lockstep) and has one `unsafe` (a stack-slot read of the running frame).
**Handles kept across steps.** The recorder keeps raw `TableRef`s of every table it has seen (`Rec::tabs`). Any instruction of a recording is a collector
safepoint, so a dropped table could be freed under it. Invariant (F8): `Heap::watch_object` stores the table's *address as `usize`* (never dereferenced by the
collector, only compared in `free_obj`); `Rec::watch_new` registers each table right after the step that added it; `free_obj` sets the flag when it frees a
watched address; the next `on_step` checks `watch_hit()` *before* anything reads `tabs` and aborts with `DepFreed` (no penalty). The list is cleared when a
recording starts, ends or aborts. Closures are not watched: a freed callee is a TR32 dependency edge (`cache.rs`, below). Nothing is kept alive for the recorder,
so with `-sink` the collector behaves exactly like the interpreter's. Mutant `rec_no_watch`, test `sink_programs_under_gc_stress` (stress 1 and 2, `GC_VERIFY`).
`tr/mod.rs` dereferences prototype keys (`&*(key as *const Proto)`, pipeline: the recording's own running closure or an inlined callee registered as a
dependency; `DepFreed` aborts the recording otherwise).

#### Dispatch cells, counting stubs and `TrProto` (P6, F5)

Every loop head of a compiled prototype has a `HeadData` (`repr(C)`: `cell`, `entry`, `pre`, counters) in a boxed fixed-length slice owned by the `TrProto` of the
prototype's `JitProto`; tier-1 code embeds the *addresses* of these cells (TR8): the dispatch stub `D_H` does `Ld V7,[cell]; JmpR V7`. The cell
holds, at any instant, a tier-1 label of a live code version, a counting stub, the entry of a live trace of the same family, or (F5) `W_H`, J4's promote-counter
stub of the current tier-1 version (rewritten by `Dispatch::install_version`). Only data cells hold trace addresses; nothing in generated code is patched.
Retired code blocks stay mapped until `free_code` (J4-RETIRE), which is what lets stale cells and landings point into them. Counting stubs call
`jit_tr_hot` / `jit_tr_entry_fail` / `jit_tr_move` (`extern "C"`, same contract as the J-HELP helpers: live thread in `r12`, current frame in `r13`).
`cache.rs` dereferences `JitProto` descriptors by key (`&*(key as *const JitProto)`): every key is a registered, not yet freed descriptor of this state and the
calls run synchronously (also `cache::verify_landings`, `+verify` only, after `recompile_inner` and in `free_code` after the freed prototype left
`JitData::live`; `t15::runtime::tr_close` / `rt::recompile_inner` re-borrow the live descriptor after a recompile as the J4 code does).

#### Trace cache, dependency edges and code lifetime (P6, F2a, F2b, F5)

`TrId` slots with generation counters; a trace is detached from its cells *before* anything is freed (`flush_now` resets all exit cells of the flushed trace to
its tier-1 addresses first; a side trace flushed alone restores its parent's cell in the same step; a family is detached as a whole; repeated requests are
no-ops through the generation counter; `HeadData::pre` / `entry` are cleared in the same step as the root's flush). `free_code` runs inside the collector
without a `State`: a freed dependency/root of the active recording sets a flag on the recording slot (`DepFreed`). Flushes requested inside an exit handler
(`handler_enter/leave`) are queued and run at the next `safe_point` (TR18), so a trace stays allocated while its handler runs (the handler also holds the
`Arc<ExitMeta>`). The dependency of a trace on every inlined callee prototype (TR32) means the trace is flushed before that descriptor is freed; that is what
makes the `*const Proto` reads and the link-cell read of a linking exit (`&TrProto.heads[i].cell` of the callee) reads of live memory. Mutants:
`dep_edge_at_install`, `cell_stale_version`, `entry_guard_past_store`.

#### Exit area, shared exit routine and the Rust restore of inlined frames (P4, F2a, F2b, F4, S0, F8)

`ExitArea` (budget word + saved registers + `args[0..3]`) is part of per-state data that outlives every trace. `exit::exit_entry` / `exit_core`
(`unsafe fn`, called from `jit_tr_exit`) read `ExitMeta::cells[snap]` and `counters` through raw pointers (boxed slices owned by the `Trace`, address-stable;
the handler borrows the `ExitMeta` from the trace body, see the P-L paragraph below), and the link cell of a linking exit (`*(ExitSnap::link_cell as *const Cell<usize>)`). `restore` (`unsafe fn`) writes
the snapshot slots (`stack0 + idx * 16`, `idx < stack_size` asserted; a bad snapshot is a fail-stop), rebuilds the `CallInfo` chain of inlined frames
(`callstatus = nresults + 1`, `| CIST_TAIL` for a replaced tail frame, **P-E: `nextraargs` for a vararg frame, whose `func_slot` is the shifted function slot** (a plain field store into the same spare node as `func`/`top`, no new invariant); saved pcs; spare nodes guaranteed by the TR6b entry check) and `thread.top`
(`base + top_slot` for a snapshot with a `top`, asserted `<= stack_size`; the slot is a value the recorder took from the interpreter's own `thread.top` and
verified by lockstep). `exit::materialise` (S0/F8; runs after `restore`) writes the sunk tables: raw 16-byte slot writes with the same assertion,
`transmute::<[u64; 2], Value>` of keys (live interned strings kept alive by trace constants) and field values (numbers, booleans, objects the trace holds);
`GCSTP_GC` is set for the duration so nothing can collect, and `Heap::new_table`/`set_str` never step the collector. **Failure path (F8):** `materialise`
returns `false` after re-enabling the collector, with the tables already built rooted in their slots; `exit_entry` records `ErrMem` with
`jit_hooks::set_error` and returns `Handled { mem_error: true }`; `jit_tr_exit` returns non-zero and the **shared exit routine tests `V0` after the `CallH`
and returns `JitExit::Error`** (the interpreter then takes the ordinary `take_error` path, "not enough memory", at the landing pc). Nothing else returns
non-zero; `Handled` users (including the emulator stubs `emurun`/`emuverify`) must treat `mem_error` as "no landing address". Machine-code contract of the
shared routine (P4): saves all registers into the area, `CallH jit_tr_exit`, reloads `ci`/`kcache`/frame from the thread and jumps to the continuation in
`ExitArea.args[0]` (a tier-1 label, a side trace entry via its cell, or, for a linking exit, the cell value of the callee head); landing address 0 means "a side
recording was armed" (`Ret(Continue)`). The test-only `+emuverify` (`jit_tr_emuverify_pre`, `verify_probe`) temporarily rewrites `ci.trap` of the live frame
and restores it right after the emulation. `unsafe impl Send/Sync for ExitMeta` is justified by the raw pointers pointing to boxed data owned by the same
`Trace`.

**P-L (exit handler without reference counting; poll exits).** `exit_entry` obtains the metadata with `cache::exit_meta_ptr` (a raw pointer from
`Arc::as_ptr` of the trace body's `xmeta`, no atomic increment) and dereferences it after `cache::handler_enter`. Why it stays valid until
`handler_leave`: the body is owned by its `Slot`; a flush requested while `in_handler > 0` only parks the `Trace` (with its `Arc`) in the graveyard
(`flush_or_defer`), `safe_point`/`retire` do nothing while a handler is active, `attach_exits` replaces the `Arc` only at install (never inside a handler),
and the `Arc` heap block does not move when the slot vector grows. This is the same argument that keeps the code block mapped under the handler (TR18),
so it adds no new obligation; the mutant `flush_in_handler` (retire at once) is the test of it. `exit::poll_exit` (`unsafe fn`, the `EXIT_POLL` branch of
`exit_entry`) performs the same `restore`/`materialise` calls as the ordinary path under the same contract and writes `ci.trap = 1` through
`Ci::from_raw(ci)` (the live root frame, contract of `exit_entry`); it clears `rn_core::state::TRACE_POLL`, a plain atomic.
`AddM { w: B4 }` (exit counters) is an in-place 32-bit add on a `Cell<u32>` of the trace's counter array, which the generated code addresses exactly
as it did with the register form (`Mov V7, &ctr[snap]`).

#### Assembly, placement and installation (P5b, PI, F1, F3, F3b)

Code is placed with the J-arena (W^X batches, J-MEM); trace code is read-only data after `install`. Embedded absolute addresses (TR7): `KPTR` constants (proto
constants kept alive by the dependency on the proto; **only compared, never dereferenced**), exit counters and cells, the `Mem` word (`vcell`). `asm.rs` has
`extern "C"` helpers called from trace code with the contract of `jit_tr_calln`: the thread is the live one pinned in `r12`, the exit area follows the budget
word, arguments are in `args[0..3]` (a live table, live interned short string or string, an integer key, the tag and payload of a value the trace holds).
`jit_tr_alloc` (class A, TR1) implements `TNEW`, `TSET`, `NEWREF` (`newref_core`, `unsafe fn`), `TNEXT`, `SNEW`, `CONCAT` (`concat_core`, `unsafe fn`: copies
operands into a stack buffer or into the new long string through `Heap::new_long_str_with`, whose closure initialises all bytes and does not allocate),
`STRTO`. Values are rebuilt from `[tag, payload]` with `transmute::<[u64; 2], Value>` (`Value` is `repr(C)` `{ tag word, payload }`; all 16 bytes
initialised; the pair belongs to a live object or is a number/boolean). Class A never steps the collector and never raises: it returns 0 first when
`Heap::jit_alloc_ok` / `Mem::would_fit` says the allocation would fail, and the next guard (`NE new, 0`) exits before using the null; the objects it allocates
are white and reachable only from a register until an exit stores the snapshot, and nothing can collect them before then (no GC point on a trace).
`+exitstorm` never fabricates a failure of that particular guard (F8, `Cx::guard_label`): the real failure has no object, and a fabricated one leaks the
allocated object. The returned node address of `NEWREF` is used by the immediately following `HSTORE`. **Pseudo node** (`TNEXT`, `STRTO`): the helper returns
the address of `args[1..4]` of the exit area laid out as a `Node`; typechecked `HLOAD`s directly follow the call (no other helper in between; the optimiser
does not run on a trace with these ops) and the guard `NE %h, 0` precedes them. `Heap::jit_str_bytes` (rn-core, the one read-only `unsafe` block added to
`heap.rs`) walks the interned-string chain: no write, no resurrection. `AddM` (`add qword [m], imm`) writes the virtual-byte words of `Mem` (`total`, `virt`),
which have stable addresses for the state's lifetime and are written only by the thread running the trace. `tier1_pc_addrs` (`unsafe fn`, tests/driver) reads
the tier-1 pc map through `JitProto`.

#### P-B (library-call intrinsics: `tr::asm::{lib_core, bytes_core, short_core}`, `rn-core::heap::{jit_str_probe, jit_str_commit}`, `jit_layout::string`)

`lib_core` (`unsafe fn`, reached only through `jit_tr_alloc` `ALLOC_LIB`) transmutes the operand words of the exit area to `Value`s exactly like `concat_core`:
a string operand is a live string of the kind the instruction's operand type says (the recorded `ShrStr` / `LngStr`, a trace type guard), the `TYPE` operand a live
`CClosure` (tag guarded by the trace; the helper still compares the closure's function pointer with `type`'s and bounds the upvalue index). `STRMETH` dereferences the
interned name, which is a constant of a prototype the trace depends on (TR32: the trace is flushed before it dies). Every other case returns 0 ("did nothing"). Like
the other class-A helpers they allocate without a collector step only when `Heap::jit_alloc_ok` says none is due. `Heap::jit_str_commit` (`unsafe fn`) writes
the string table: its contract is "the probe of the same bytes, nothing allocated or swept since, no table growth needed", which `short_core` satisfies (probe, check,
commit, nothing in between). The inline `TLEN` of a string reads the `shrlen` byte / the `u.len` word at the offsets of `jit_layout::string` (`SHRLEN`, `LNGLEN`) of a
string the trace has typed `ShrStr` / `LngStr`.

#### P-J additions (allocation path; `rn-core/src/{alloc,table}.rs`, `rn-jit/src/rt.rs`)

No new kind of unsafe. `Allocator::alloc_block` / `free_block` (unsafe trait methods, default = `realloc(null, 0, n)` / `realloc(p, n, 0)`; `PoolAllocator`
overrides them with its size-class push/pop, `SystemAllocator` is reached for blocks above `POOL_MAX` exactly as in `realloc`) carry the (A1)-(A4)
contract of `realloc`; `Mem::alloc` / `Mem::free` perform the same limit check / `total` update as `Mem::realloc` for `old == 0` / `new == 0`.
`table::resize_empty` (a table with `asize == 0` and the dummy node, the only state `new_table` makes) writes `node`/`lsizenode`/`BITDUMMY`/`array`/`asize`
/ array tags/hint by the T1/T2 invariants and restores the dummy node when the array allocation fails after the node vector succeeded (the general path's
alloc/free order). `rt::data_ptr` is unchanged in contract; its first-use initialisation moved to the `#[cold]` `init_data`.

#### F1 additions (`tr::asm::{tnext_entry, alloc_general, arg}`, `rn-core::{table::next_hinted, number::IntBuf}`, `rn-std::lib_math`)

No new kind of unsafe. `jit_tr_alloc` is now a thin dispatcher: `ALLOC_TNEXT` goes to `tnext_entry` (`unsafe fn`, `#[inline(never)]`, the old `TNEXT` arm with the
same contract: a live table in `args[0]`, the control tag in `a >> 8`, the control payload in `args[1]`, a collectable payload being a live object or a dead key
of the table), everything else to `alloc_general` (the unchanged body). `arg(area, k)` reads one word of the exit area with `read_volatile` (the generated
code stores the operands as separate 8-byte words; a compiler-merged 16-byte load of two of them is a store-forwarding stall); the area is the one the caller
already holds a `&mut` to, the read is in bounds (`k < 4`). `TableRef::next_hinted` is safe Rust over the existing `unsafe` table internals: the hint index is
checked against `sizenode` before the node is read and the node's key is compared (tag and payload bits) with the control key before it is trusted, so any
value of the hint (`Heap::next_hint`, a `pub u32`) is sound; a mismatch falls back to `findindex`. `number::IntBuf` is safe code (the digits are written
into a local `[u8; 20]`, indices bounded by the digit count). `math.random` keeps its state in the integer upvalues of its C closure (`CClosure::upvalue` /
`set_upvalue`, bounds `assert`ed, integers need no barrier); no unsafe.

#### Emulator and test-only entries (P0, P0b, F2a, F6)

`emu.rs` dereferences raw addresses exactly as the machine code does (J4-EMU contract). `jit_tr_emurun`, `jit_tr_emuverify_pre` and `LiveHelpers`
(`+emurun`, `+emuverify`) run the emulator against the live thread. F6: the `emurun` stub no longer panics on the `JitExit::Error` return; it propagates the
error to the interpreter like the machine-code routine. Test hooks (`exit::set_materialise_fault`) are `thread_local` counters (no unsafe). Mutant machinery
and the `TR_*` environment knobs exist only with `tr_mutants` / test builds.

### Contracts of generated trace code (no Rust `unsafe`; for the review of the memory-safety argument)

#### Entry checks and snapshot-0 guards (TR6, TR6d, F1)

A trace is entered with the state of its head's pre-count tier-1 label (frame in the stack, budget word, `rbx/r12/r13/r14/r15` valid) and re-checks everything it
assumes: stack room (TR6b), types of the live-in slots (`SLOAD`), closure identity, upvalue identity, array/hash shapes by guards. Every following access is
conditioned on a guard that dominates it, and entry guards are emitted before the first store (mutant `entry_guard_past_store`). `lsizenode` is guarded before
`HREFK`.

#### Exits and snapshot restore (TR3, TR5.4, TR27, F2a, F2b, F4, F8)

Inline (frameless) exit stubs write the dirty registers and end with `Mov V7,&cell; Ld V7,[V7]; JmpR V7`; the cell holds the tier-1 label of the snapshot's pc
or the entry of a side trace, which is entered with exactly the tier-1 label state and re-checks its own TR6 room. The link of a side trace stores the budget
register to the budget word and jumps through `cell[H]` (or the head's `entry` cell for a prelude). A nest root (`AsmEnv.head != 0`) and the stitching exit
end with an unconditional exit whose loop/link epilogue is dead code. Every address a stub can jump through is a tier-1 label of a live code version or the
entry of a live trace of the same family. Generic stubs (snapshots with frames, a `top`, or sunk tables) go through the shared exit routine (above).

#### Loop layouts: rolled and peeled (7.8)

The peeled body keeps IR order; hoisted loads are guarded by the loop-invariance facts of `opt_mem` and are exited from before any store;
`opt_loop::bounds_known` removes only a dominated duplicate guard against the same table's `asize` (which cannot change on a trace, TR13.4).

#### Memory access: array, upvalue, header and hash loads and stores (TR13, F1, F7a, S0)

- **Hash (F1).** `HREFK` reads `[node + idx*24 + KEY_TT/KEY]`; `HLOAD/HSTORE` access the value words of that node. (1) The table is a live `Tab`-typed value
  pinned by the entry/`SLOAD`/`ULOAD`/`HLOAD` typecheck; (2) `FLOAD lsizenode == recorded` makes `idx < 2^lsizenode` and `node` the table's node vector (shared
  dummy node of size 1, idx 0, dead key tag, for an empty hash part); (3) the node vector cannot be reallocated on a trace (no rehashing `NEWREF`, no
  inserting helper: TR1; `HSTORE` writes only the value of an existing key); (4) all accesses are in-bounds 1/4/8-byte accesses, the key compare is two
  32-bit loads; (5) `HSTORE` writes tag byte and payload of a node whose old value is non-nil (the old-value guard precedes the stores), then `TBAR`; (6) the
  string address embedded in the code is only compared.
- **Dynamic-key hash lookup (N6, `HREFV`).** No new Rust `unsafe`; the generated chain walk reads `[key + SHORT_HASH]` (4 bytes), then, from the node `(hash &
  (2^lsize - 1))` of the node vector, `[node + KEY_TT]` (1), `[node + KEY]` (8), `[node + NEXT]` (4, signed, in nodes). (1) The key operand is the address of a live
  string object (an SSA value of a `ShrStr`-typed ref: a checked load, a `LIB` result or a slot guard), so the header read is in-bounds; a long string that got
  here by mistake has the same header layout and can only cause a miss, never a hit (a hit needs the node's key tag `SHRSTR` and the equal address, and a node
  holds a short string's address). (2) `FLOAD lsizenode == recorded` precedes it (the mask is an immediate and `mask < node count`; the shared dummy node has
  lsizenode 0, one node, a nil key tag and `next == 0`); every `next` link stays inside the node vector (invariant of the table), the walk ends at `next == 0`
  (acyclic chains, as in `luaH_getshortstr`). (3) The walk saves one scratch register in an extension slot and restores it on the hit and the miss path before
  the exit branch. (4) Loads only; the result feeds `HLOAD` / `HSTORE` exactly as `HREFK`'s.
- **Array on 5.4 (`ATOUCH`, F7a).** Loads `[table+FLAGS]` and, only when `BITRAS` is set, `[table+ARRAY]`, `[array+LENHINT]`, and stores a `u32` to
  `[array+LENHINT]`. Emitted strictly after the `ULT idx0, asize` guard of the same access, in IR order, so `asize > 0` and the hint word exists
  (invariant T1, `LENHINT = 0`); the header cannot be resized on a trace; the 4-byte store never touches a tag or payload; temporaries are saved in the
  exit-area extension slots (`EXT_SLOTS = 9`) and no guard is emitted inside the sequence. `TableRef::alimit_hint` is a safe `pub fn` used by the lockstep only.
- **Allocation sinking (S0, F8).** `TVIRT` writes `total`/`virt` through absolute addresses (above). A sunk table has no memory on the fast path; every
  consumer is a virtual-field operation (`Sstore`), and an exit materialises it from the snapshot's `SUNK` entry group (`sink::slot_is_dead` has the one
  `unsafe` of `sink.rs`: it dereferences the head prototype key, same argument as the pipeline's). A table held by a live local is never sunk. There is no
  in-trace step/limit guard (accepted deviation, TR29.3 first half; documented in `docs/JIT.md`).

#### Helper calls from a trace (TR1, 7.5, F3, F3b)

Class-A helpers (`jit_tr_calln`, `jit_tr_barrier`, `jit_tr_alloc`) run no Lua code, never move the stack and never step the collector or raise; calls that
could are not traced (TR1). Barriers (`TBAR`) go through `jit_tr_barrier` with a rebuilt `Value` and the real `Heap` barrier. `TSET` writes through the table's
own `set_int` (no raw pointer). `rn-core` gained safe `pub fn`s for these (`new_table_bytes`, `Heap::jit_alloc_ok`, `Mem::would_fit`, `TableRef::node_addr`,
`charge_virtual`, `uncharge_virtual`, `virtual_bytes`, `virtual_cell`, `watch_object`/`unwatch_all`/`watch_hit`).

P-O (`pcall` / `xpcall` of a Lua closure inlined in a trace): one new `unsafe` expression, in `rn-core/src/jit_hooks.rs::rec_via_native`: `&*ci.previous`
to read `callstatus` and `previous` of the C frame between a Lua callee and the recording's last frame. Invariant: `previous` of a live `CallInfo` is a live
node of the same list or null (checked before the dereference); only the recording thread's chain is walked, the function reads two fields and keeps no
reference. `rec_validate` gained a safe branch that skips one `CIST_C` frame (through `Ci::previous`). `vm_unsafe::run` sets `trap` of a fresh frame that
`rec_wants` (safe `Ci::w`), and the hooked copy returns to the fast copy when its frame's `trap` was cleared by an abort in a nested activation (no new
memory access). The class-A helper `CCALLS` (`asm::lib_core`) reads `State::ncalls_raw` (safe). A trace never restores the native's `CallInfo`: every guard
of an inlined body exits to the `CALL` itself, so no frame is rebuilt and no `CallInfo` node is written for the C function.

### Tools that check these invariants

`cargo test -p rn-jit --features tr_mutants,t15_oracle` (51 test binaries incl. the `tr_core` twin matrix with GC stress 1/2 + `GC_VERIFY`, `tr_asm` in both
`TR_ASM_VB` modes, `tr_oracle`, `tr_identity*`, `tr_class_a*`, `tr_policy`, `tr_exit`, `tr_record*`); the mutant runner (`tests/tr_mutants.rs`, under
`ulimit -v 16000000`); jit-diff modes `tr tron trstack trstorm tremu trrand trnosink trverify trnoopt` and the `GCOBS` template (`docs/JIT_TESTING.md`);
the official suite via `scripts/jit-official.sh` (traces `always`/`on`, GC stress); `scripts/asan.sh` and `scripts/asan-jit.sh` with traces; Miri on the pure
modules (`scripts/miri.sh`, F6 notes in `docs/JIT_TRACE_REF.md`); fuzz targets `run_source` (trace/sink modes by first byte), `jit_diff`, `trace_ir`.
