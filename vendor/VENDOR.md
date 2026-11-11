# Vendored upstream sources

Pristine, unmodified. **Spec and test oracle only** — not a build input for the Rust crates
(except `xtask ref-build`, and the C-API tests that compile `lua.c` against our library).
Retrieved 2026-10-27 from https://www.lua.org/ftp/ and https://www.lua.org/tests/ into an empty scratch directory;
SHA256 verified against the values published on those pages.

| Dir | Upstream file | SHA256 |
|---|---|---|
| `lua-5.5/` | `lua-5.5.1.tar.gz` | `1c4b4068d67061f2a2231ad2b5422e77acea1487ea9890f6320af614f4373dce` |
| `lua-tests-5.5/` | `lua-5.5.1-tests.tar.gz` | `da07b543872dc0bb2ff12aabd0c248578d78df3eb6b67efdc537a46d455c7f31` |
| `lua-5.4/` | `lua-5.4.9.tar.gz` | `2335b6c582a52654f94612bf10d2f4672805d05329aa6568b1d8cd9e5c6fb8e6` |
| `lua-tests-5.4/` | `lua-5.4.9-tests.tar.gz` | `7d971845f545ffc09fbb3128a86b2c6524161c70d0fdf0154a16e8c00c343fca` |

Local patches: **none**. License: MIT (reproduced in `LICENSE-LUA`).

## License verification (2026-10-28, Phase 8)
- `lua-5.5/`, `lua-5.4/`: the MIT notice "Copyright (C) 1994-2026 Lua.org, PUC-Rio" is at the end of
  `src/lua.h` (every other source file says "See Copyright Notice in lua.h"); `doc/readme.html`
  ("License" section) states that Lua is distributed under the MIT license and reproduces it. The
  reference manual (`doc/manual.html`) is under the same terms.
- `lua-tests-5.5/`, `lua-tests-5.4/`: every test file except the two helper scripts `bwcoercion.lua` and
  `tracegc.lua` (no header) says "See Copyright Notice in file lua.h"; `ltests/` and `libs/*.c` likewise.
  The test suites are distributed by lua.org under the Lua license.
- Outside `vendor/`, the only third-party-derived files are benchmark programs under `bench/` (LuaJIT `jit/`
  disassemblers, and programs derived from json.lua, classic, middleclass, inspect.lua, lustache and serpent): they
  are listed with their licences in `THIRD_PARTY.md`. `tests/c/*.c` and `tools/refrun.c` were written for
  this project (the C API tests compile the vendored, unmodified `lua.c`). The Rust dependencies are
  audited by `cargo deny` (`deny.toml`; libc, cc, shlex, find-msvc-tools: MIT OR Apache-2.0; the fuzz
  workspace additionally pulls libfuzzer-sys, `(MIT OR Apache-2.0) AND NCSA`, never shipped).
- Shipped binaries contain only rustynail code (MIT), the Rust standard library (MIT OR Apache-2.0),
  `libc` bindings and the `lua-capi` C shim (`csrc/shim.c`, project code): the notices to ship are
  `LICENSE` and `LICENSE-LUA` (`scripts/release.sh` copies both into every `dist/` variant).

## Reference build flags (pinned)
Built by `cargo xtask ref-build --lua-version {5.4|5.5}` = upstream `make linux` (gcc, `-O2`), out of tree in `ref/<54|55>/build`.
- 5.4.9 upstream Makefile adds `-DLUA_COMPAT_5_3` (so e.g. `math.pow` exists). 5.5.1 has no such default.
- Our implementation must match whatever compat set the reference uses for each version. Decision: mirror upstream defaults exactly.
- `lua.c` switches the GC to generational at startup in both versions (5.4 `lua.c:646`, 5.5 `lua.c:754`); library default is incremental.
