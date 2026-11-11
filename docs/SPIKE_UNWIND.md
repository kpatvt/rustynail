# P0.3 spike result: Lua-style errors via `extern "C-unwind"` + Rust panic (2026-10-27)

Code: `spikes/unwind/` (standalone staticlib + `test.c`, `shim.c`). Host: x86-64 Linux, gcc and clang, rustc 1.99.1.

| Check | Result |
|---|---|
| `lua_error` analogue (`resume_unwind`, private payload) unwinds through 3 C frames to a `catch_unwind` (`spike_pcall`) | **PASS** gcc/clang × `-O0`, `-O2`, `-O2 -fomit-frame-pointer` (default flags: `.eh_frame` is emitted on x86-64) |
| C `__attribute__((cleanup))` during unwind | runs only with `-fexceptions` (expected; PUC's longjmp never ran it either, so irrelevant) |
| C-variadic shim (`vsnprintf` → Rust fn; and `luaL_error` analogue that raises from inside C vararg frame) | **PASS** |
| Cost: 200,000 raise+catch through 3 C frames | ~0.2 s total ⇒ **~1 µs per C-originated error** (longjmp ≈ 20 ns). Only C-API errors pay this; ordinary Lua errors use `Result<_, Status>`. |
| `valgrind` | glibc binaries SIGILL (AVX-512 znver4 ld.so); **musl-target binaries work** (verified earlier) |

**Covered since (Phase 5, `tests/c/capi_diff.c` sections `tables` and `coroutines`, byte-identical to the reference `liblua.a`):**
error raised by a metamethod during `lua_gettable` from C; `lua_callk`/`lua_pcallk` across yield+resume with continuation; `lua_yieldk` from a C function; `lua_yield` from a C hook is supported by the hook trampoline.

**Decision: GO** for the panic-based C-API error design, with the rules in PLAN.md §4.5 and the C variadic shim approach (~15 lines of C per variadic function; only `lua_pushfstring`, `luaL_error`, and `lua_gc`).
Reminder: `extern "C"` (not `"C-unwind"`) on any callback type aborts on unwind — keep every callback `extern "C-unwind"`.
