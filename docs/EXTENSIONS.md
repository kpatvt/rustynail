# LuaJIT-compatible extensions

`rnlua` offers the small, data-oriented part of LuaJIT's library surface that Lua code commonly depends on:

| module | what | LuaJIT equivalent |
|---|---|---|
| `table.new(narray, nhash)`, `table.clear(t)` | pre-sized tables, in-place emptying | `require("table.new")`, `require("table.clear")` |
| `string.buffer` | a string buffer with a binary serialization format | `require("string.buffer")` |
| `jit` | JIT control table: `version`, `os`, `arch`, `status()`, `on()`, `off()`, `flush()` | the global `jit` |

Not provided: the FFI and everything that needs C data (`ffi`, cdata buffer methods `reserve`/`commit`/`putcdata`/`ref`),
`jit.util`, `jit.opt`, `jit.dump`/`jit.v` (profiling and bytecode/IR listings).

The behaviour was derived from, and is checked against, LuaJIT 2.1 (`/usr/bin/luajit`, `jit.version` `LuaJIT 2.1.1793383622`)
with `scripts/ext-diff.sh`: every program in `tests/ext/` (except `rn_*`, which test rustynail-only behaviour) runs under both
engines and the outputs must be identical. `crates/rn-cli/tests/ext.rs` pins `rnlua` to the checked-in `.expected` files
(produced by LuaJIT) without needing LuaJIT installed.

## Availability

* **`rnlua`** opens all of it: `table.new` / `table.clear` are fields of `table` (LuaJIT only adds them after `require`;
  having them always present is a deliberate convenience), the three modules are registered in `package.preload`
  (`require("table.new")`, `require("table.clear")`, `require("string.buffer")`), and `jit` is a global (also in
  `package.loaded`). `string.buffer` is **not** a field of `string` (neither is it in LuaJIT).
* **Embedding (`rustynail` crate):** `StdLib::EXT` (table.new/clear and the `string.buffer` preload) and `StdLib::JIT` (the
  `jit` table) are opt-in flags, in neither `StdLib::ALL` (= `luaL_openlibs`) nor `StdLib::SAFE`. `Builder::sandbox()` does not open
  them; `lua.open_libs(StdLib::EXT)` adds the data extensions to a sandbox. They are safe for untrusted code (no system access;
  buffer memory is charged to the state's memory limit), but `string.buffer` is only reachable through `require`, so a state
  without the `package` library cannot load it. `jit` is deliberately kept away from untrusted code (it can switch this state's
  compiler off).
* **C API builds** (`luaL_openlibs` from `lua.c` or from C modules) are unchanged: no extension is added there.

## `table.new(narray, nhash)` / `table.clear(t)`

`table.new` returns an empty table with room for `narray` array and `nhash` hash entries. Both arguments are required numbers
(strings convertible to numbers are accepted; fractions are truncated). A negative, NaN or too large argument (arrays above
2^28, hash parts above 2^26 entries) raises `table overflow` without allocating; a legitimate size beyond the state's memory
limit raises `not enough memory`.

`table.clear(t)` removes every entry in place and returns nothing. It is raw (no metamethods), keeps the allocated array and
hash capacity, the node layout and the metatable, and is exactly `t[k] = nil` for every present key (so no GC barrier is needed
and compiled code or traces that guard on array size / node positions stay valid). A `pairs` loop that clears the table
ends at once, as in LuaJIT. Errors: `bad argument #1 to 'clear' (table expected, got X)`.

Differences: `table.new(-1, 0)` succeeds in LuaJIT (its sizes wrap); here every negative size is `table overflow`. The error
messages say `table.new` / `table.clear` instead of `?` when the function was found through `require`.

## `string.buffer`

```lua
local buffer = require("string.buffer")
local b = buffer.new([size | options] [, options])   -- options: { dict = {...strings}, metatable = {...tables} }
b:put(...)        -- strings, numbers, objects with __tostring; returns b
b:putf(fmt, ...)  -- string.format into the buffer; returns b
b:set(str)        -- replace the content (copies; LuaJIT shares the string)
b:get([n, ...])   -- consume and return n bytes (several arguments: several results); no argument: everything
b:skip(n)         -- discard n bytes
b:tostring()      -- the unread content (also tostring(b)); #b = unread length; b .. x concatenates
b:reset()         -- empty it, keep the memory;   b:free() -- release the memory
b:encode(v)       -- append the serialization of v; returns b
b:decode()        -- consume and return one serialized value
buffer.encode(v)  -- the serialization as a string
buffer.decode(s)  -- the value; the whole string must be consumed
```

`type(b)` is `"userdata"`, as in LuaJIT. The bytes live in Rust-allocated memory that is charged to the state's memory limit
(`not enough memory` on exhaustion; a buffer is limited to 2^31-1 bytes, `buffer overflow`); `__gc` releases it. Sizes that
cannot be satisfied (`buffer.new(1 << 62)`) are rejected by argument checks without allocating.

Differences from LuaJIT, all deliberate:

* `reserve`, `commit`, `putcdata` and `ref` need cdata pointers: they raise `string.buffer: '<name>' is not supported (no FFI/cdata)`.
* Numbers in `put`/`putf` use Lua 5.5 formatting (`1.0` prints as `1.0`; LuaJIT prints `1`), and `putf("%d", 2.5)` is an error
  (Lua 5.5 `string.format`), where LuaJIT truncates. The error message position of `putf` arguments is the real position.
* `buffer.decode(5)` / `b:set(5)`: numbers are not converted to strings (as in LuaJIT).
* `b:set(str)` copies the string.
* Metamethods of `__tostring` objects passed to `put` may touch the buffer (the buffer is consistent; text added meanwhile is kept).

### Serialization format

Byte-identical to LuaJIT 2.1 (checked on `tests/ext/buf_encode.lua`, 150 random values, and 40,000 mutated or truncated inputs for
`decode`). Every item starts with a tag written as an unsigned variable-length integer ("wu124"): 1 byte for values below
0xe0; `0xe0 + (v >> 8)`, `v & 0xff` for values below 0x1fe0; otherwise `0xff` followed by `v` as 4 bytes little endian.

| tag | item | payload |
|---|---|---|
| 0 | nil | |
| 1 / 2 | false / true | |
| 3 | light userdata NULL | |
| 4 / 5 | light userdata (32 / 64 bit address) | 4 / 8 bytes LE (LuaJIT: cdata pointer; here a light userdata) |
| 6 | int32 | 4 bytes LE (decode only; LuaJIT emits it only in dual-number builds) |
| 7 | number | IEEE double, 8 bytes LE |
| 8..13 | table | see below |
| 0x0e | metatable of the following table | `wu124` index into the `metatable` option (0-based) |
| 0x0f | string from the dictionary (table keys only) | `wu124` index into the `dict` option (0-based) |
| 0x10 / 0x11 | int64 / uint64 | 8 bytes LE (LuaJIT: cdata, needs FFI to decode; here integers) |
| 0x20 + n | string of n bytes | the bytes (the tag is `wu124(0x20 + n)`) |

Tables: tag `8 + hash?(1) + array-form(2 or 4)`. Array form 2 = the array part starts at index 0 (`t[0]` is non-nil), 4 = it starts at
index 1; the next item is the array count `narray` (index 0 included in the count: `{1, 2}` has `narray` 3 in form 4, 2 in form 2),
then the hash count (if the hash bit is set), then the array values (index 0 or 1 up to `narray - 1`, nils allowed as tag 0), then
the key/value pairs. Nesting is limited to 100 tables (`too deep to serialize`; this is also the only protection against circular tables).

Which keys go into the array part: LuaJIT looks at its internal array part, whose size depends on how the table was built.
rustynail writes the integer keys `1..n` (up to the last non-nil index of its array part, extended over consecutive keys) as the
array part and everything else as pairs. Consequences: arrays built by constructors, `table.insert` or `t[i] = v` loops encode identically to
LuaJIT; a table `{x = 1}` with `t[1]` added later is written with a one-element array part here but with key 1 in the hash part by LuaJIT; sparse
arrays (`t[1..4]` plus `t[10]`) have `t[10]` as a pair here and as a hole-filled array part in LuaJIT. The order of pairs
is the table's traversal order in both, so tables with several hash keys differ in byte order but decode to equal tables.
The decoder accepts both layouts of every table. `decode(encode(x))` is equal to `x` in every case.

Numbers and the integer subtype: LuaJIT has only doubles, so integers are written as doubles (tag 7) when they fit (|i| <= 2^53,
which makes the bytes identical to LuaJIT's) and with tag 0x10 (int64, exact) otherwise. Decoding turns a double that is integral,
within +-2^53 and not `-0.0` into an integer and every other double into a float, so `{1, 2}` round-trips as integers and `1.5`,
`-0.0`, `inf`, `nan` and `3.0 * 2^60` stay floats (a float with an integral value, `3.0`, comes back as the integer `3`).
Tags 0x10/0x11 decode to integers (0x11 above `math.maxinteger` to a float).

Dictionaries (`dict` option, an array of strings) replace *table keys* that appear in it by `0x0f idx`; values are never replaced.
The first of duplicate entries wins. `metatable` (an array of tables) marks tables whose metatable is listed with `0x0e idx`
(tables with other metatables are written without one); decoding needs a buffer created with the same options
(`cannot deserialize dictionary index N` otherwise; N is `idx + 1`). The option tables are used as given at encode time and read
again at decode time.

### Errors (texts match LuaJIT)

`cannot serialize 'function'` (also `'thread'`, `'userdata'`; a buffer itself is a userdata), `too deep to serialize`,
`unexpected end of buffer`, `left-over data in buffer` (`buffer.decode` with trailing bytes), `table index is nil`, `table index is NaN`,
`table overflow` (count fields above 2^28 array / 2^26 hash entries), `cannot deserialize tag 0x13`, `cannot deserialize dictionary
index N`, `bad options table`, `bad argument #n to 'm' (number out of range)`, `buffer overflow`, `not enough memory`.
Malformed input never crashes or reads out of bounds: table counts only presize up to what the remaining input can hold, so a
forged count costs no memory; the error is raised by the element that is missing. A failed `b:encode(v)` leaves the buffer as it
was. A failed `b:decode()` consumes nothing. `b:put(a, bad, c)` keeps what was appended before the bad argument (as in LuaJIT).

## `jit`

| field | |
|---|---|
| `jit.version` | `"rustynail <version> JIT"` |
| `jit.version_num` | `major*10000 + minor*100 + patch` of rustynail (LuaJIT: 20199) |
| `jit.os`, `jit.arch` | `"Linux"`, `"x64"` (also `OSX`, `Windows`, `BSD`, `Other`; `arm64`, `x86`, ...) |
| `jit.status()` | `true, "tier1", "trace", "x64"` when this state's compiler is on (`trace` only with the trace compiler on); `false` when it is off or there is none (`RUSTYNAIL_JIT=off`, a build without the `jit` feature, a host that forbids executable memory, a non-x86-64 host) |
| `jit.off()` / `jit.on()` | switch this state's compiler off / back to the mode the host chose |
| `jit.flush()` | drop every compiled trace of this state |

Semantics: the switches are per state and safe at any point. `jit.off()` flushes the traces and stops entering compiled code; frames
that are already running in compiled code finish there (the existing `set_mode` machinery). `jit.on()` restores the mode the host selected
(`RUSTYNAIL_JIT`, `Lua::set_jit_mode`), so a script cannot enable a compiler that the environment or the embedder disabled
(a state with the compiler off keeps `jit.status() == false` after `jit.on()`). `jit.flush()` aborts a recording in progress,
queues the flush while an exit handler runs, and retires trace code lazily (a trace that is executing when it is flushed stays valid
until it exits); tier-1 machine code of prototypes is valid independently of traces and is kept. It is safe inside hot loops, coroutines and metamethods
(`tests/ext/rn_jit_ctl.lua`). The per-function forms `jit.on(f)`, `jit.off(f [, recursive])` and `jit.on(true)` are accepted and
ignored; only the global switch is implemented. `jit.opt` and `jit.util` do not exist (the trace policy is configured with
`RUSTYNAIL_JIT_TRACE_LIMITS`, see `docs/JIT_TRACE_REF.md`).

Without the JIT (a `--no-default-features` build, or a host without executable memory) the `jit` table still exists, so scripts that probe
`jit` or `jit.status()` work: `status()` returns `false` and `on`/`off`/`flush` do nothing.

## Platform notes

Everything except the compiler hooks of `jit` is portable Rust (little-endian wire format regardless of the host). The JIT parts exist on x86-64 Linux.

## Tests

`scripts/ext-diff.sh [--update] [test.lua...]` runs `tests/ext/*.lua` under LuaJIT and `rnlua` and checks both against the `.expected` files;
`scripts/ext-matrix.sh` repeats that under `RUSTYNAIL_JIT=off|always`, `RUSTYNAIL_JIT_TRACE=always`, `RUSTYNAIL_GC_MODE=inc` and
`RUSTYNAIL_GC_STRESS=1|2`. `cargo test -p rn-cli --test ext` is the LuaJIT-free version. Fuzzing: `fuzz/` targets `buffer_ops` (new), `string_lib`
(calls `table.new/clear` and `buffer.encode/decode/new` with arbitrary arguments), `table_ops` (model-checks `clear`) and `run_source` (the sandbox opens the
extensions; the buffer module is the global `buffer` there).
