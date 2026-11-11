//! # rustynail: Lua 5.5 (and 5.4) in Rust, safe embedding API
//!
//! This crate embeds the rustynail Lua interpreter in a Rust program. The API is a thin, safe
//! wrapper with the semantics of the Lua C API (`lua.h`/`lauxlib.h`): values live on a stack,
//! operations address them by index, and Rust functions called from Lua read their arguments from
//! and push their results to that stack. If you know the C API you know this one; the
//! differences are listed on [`Stack`].
//!
//! ```
//! use rustynail::Lua;
//!
//! # fn main() -> rustynail::Result<()> {
//! let mut lua = Lua::new()?;
//!
//! // Rust -> Lua: a global function
//! lua.push_function(|s| {
//!     let a = s.to_integer(1).unwrap_or(0);
//!     let b = s.to_integer(2).unwrap_or(0);
//!     s.push_integer(a + b)?;
//!     Ok(1) // number of results
//! })?;
//! lua.set_global("add")?;
//!
//! // run Lua code; its results stay on the stack
//! let n = lua.do_string("return add(40, 2), 'done'")?;
//! assert_eq!(n, 2);
//! assert_eq!(lua.to_integer(-2), Some(42));
//! assert_eq!(lua.to_bytes(-1).as_deref(), Some(&b"done"[..]));
//! lua.pop(2)?;
//!
//! // Lua -> Rust: call a Lua function
//! lua.do_string("function greet(who) return 'hello ' .. who end")?;
//! lua.get_global("greet")?;
//! lua.push_str("world")?;
//! lua.call(1, 1)?;
//! assert_eq!(lua.to_string_lossy(-1).unwrap(), "hello world");
//! # Ok(()) }
//! ```
//!
//! ## Overview
//!
//! - [`Lua`] owns a state ([`Lua::new`], [`Lua::with_libs`], [`Lua::builder`]); it dereferences to
//!   [`dyn Stack`](Stack), which has all operations: pushing and reading values, tables and
//!   fields, calls ([`call`](trait.Stack.html#method.call), [`pcall`](trait.Stack.html#method.pcall)), loading code
//!   ([`load`](trait.Stack.html#method.load), [`do_string`](trait.Stack.html#method.do_string), [`do_file`](trait.Stack.html#method.do_file)),
//!   Rust functions ([`push_function`](trait.Stack.html#method.push_function),
//!   [`push_closure`](trait.Stack.html#method.push_closure)), userdata ([`push_userdata`](trait.Stack.html#method.push_userdata)),
//!   registry references, coroutines and the garbage collector.
//! - A Rust function receives `&mut dyn Stack` for the calling thread and returns
//!   `Result<usize>` (the number of results); `Err` raises a Lua error. See
//!   [`push_function`](trait.Stack.html#method.push_function).
//! - Every fallible operation returns [`Result`] with a typed [`Error`] ([`ErrorKind`] plus the
//!   message). Lua errors never unwind through Rust and nothing aborts.
//! - Resource limits and sandboxing: [`Lua::set_memory_limit`], [`Lua::set_instruction_limit`],
//!   [`Builder::sandbox`], [`Builder::text_only`], [`StdLib`] selection.
//!
//! ## Safety model
//!
//! The crate contains no `unsafe` code (`#![forbid(unsafe_code)]`); the interpreter's own unsafe
//! code is confined to `rn-core` (see `docs/UNSAFE.md` in the repository). The public API cannot
//! cause undefined behaviour from safe code, whatever the indices, values or scripts:
//!
//! - **No dangling handles.** Lua values are never handed out as Rust objects; they stay on the
//!   Lua stack (rooted for the collector) or in the registry ([`RegistryKey`]). A `dyn Stack` is
//!   only ever borrowed (`&mut dyn Stack`), and as an unsized type it cannot be moved, swapped or
//!   stored, so it cannot outlive the state or the callback it was given to.
//! - **Checked indices and stack space.** Invalid indices are reported as "no value" or as
//!   [`ErrorKind::Api`] errors; pushes grow the stack (C leaves both to the caller).
//! - **Rust values owned by Lua** (closures, userdata) live outside Lua memory in a per-state
//!   table and are found by id with a checked downcast, so a userdata of the wrong type, a forged
//!   id or a value used after its finalizer ran is an error, not memory corruption. They are
//!   dropped by the collector (`__gc`) or when the state closes.
//! - **Panics** in Rust functions are caught and raised as Lua errors.
//! - Binary chunks are trusted input, as in PUC Lua (no bytecode verifier): states that run
//!   untrusted code must use [`Builder::text_only`] (included in [`Builder::sandbox`]). The `debug`
//!   library can break Lua-level invariants (not memory safety of this API); do not open it for
//!   untrusted code.
//!
//! ## Lua versions
//!
//! The crate builds for Lua 5.5 by default and for Lua 5.4 with `RUSTFLAGS="--cfg
//! rustynail_lua54"` (`cargo xtask ... --lua-version 5.4`). The API is the same except for the
//! collector parameters: `Stack::gc_param` (5.5) and `Stack::gc_incremental` /
//! `Stack::gc_generational` (5.4). [`VERSION`] and [`VERSION_NUM`] report the version.
//!
//! ## Not covered
//!
//! Debug interface (`lua_getinfo`, hooks other than the instruction limit), `lua_dump`,
//! custom allocators, warnings to a Rust callback, to-be-closed slots and arithmetic through the
//! API (`lua_arith`; use Lua code). C modules (`require` of a `.so`) need the C API crate
//! (`rn-capi`) linked and initialized; the CLI does that.
#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod callback;
mod error;
mod lua;
mod shared;
mod stack;

pub use error::{Error, ErrorKind, Result};
pub use lua::{Builder, JitMode, JitStats, Lua, StdLib};
pub use stack::{Stack, MULTRET};

/// Version string of the Lua language implemented (`"Lua 5.5"` or `"Lua 5.4"`).
pub const VERSION: &str = if cfg!(rustynail_lua54) {
    "Lua 5.4"
} else {
    "Lua 5.5"
};

/// `LUA_VERSION_NUM`: 505 or 504.
pub const VERSION_NUM: u32 = if cfg!(rustynail_lua54) { 504 } else { 505 };

/// Type of a Lua value (`lua_type`).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Type {
    /// `nil`.
    Nil,
    /// `true`/`false`.
    Boolean,
    /// A bare pointer ([`Stack::push_light_userdata`](trait.Stack.html#method.push_light_userdata)).
    LightUserdata,
    /// Integer or float.
    Number,
    /// Byte string.
    String,
    /// Table.
    Table,
    /// Lua or Rust function.
    Function,
    /// Full userdata.
    Userdata,
    /// Coroutine.
    Thread,
}

impl Type {
    /// The name `type()` returns (`lua_typename`).
    pub fn name(self) -> &'static str {
        match self {
            Type::Nil => "nil",
            Type::Boolean => "boolean",
            Type::LightUserdata | Type::Userdata => "userdata",
            Type::Number => "number",
            Type::String => "string",
            Type::Table => "table",
            Type::Function => "function",
            Type::Thread => "thread",
        }
    }
}

/// Which chunk formats [`Stack::load`](trait.Stack.html#method.load) accepts (the `mode` argument of `load`).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub enum LoadMode {
    /// Source text only (`"t"`).
    Text,
    /// Precompiled binary chunks only (`"b"`). Binary chunks are trusted input.
    Binary,
    /// Either (`"bt"`, the default).
    #[default]
    TextOrBinary,
}

impl LoadMode {
    pub(crate) fn as_bytes(self) -> &'static [u8] {
        match self {
            LoadMode::Text => b"t",
            LoadMode::Binary => b"b",
            LoadMode::TextOrBinary => b"bt",
        }
    }
}

/// Collector mode.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum GcMode {
    /// Incremental mark-and-sweep (the library default).
    Incremental,
    /// Generational (the standalone interpreter's default).
    Generational,
}

/// Lua 5.5 collector parameters for [`Stack::gc_param`](trait.Stack.html#method.gc_param) (`LUA_GCP*`).
#[cfg(not(rustynail_lua54))]
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum GcParam {
    /// `LUA_GCPMINORMUL`: minor collection frequency (percent).
    MinorMul = 0,
    /// `LUA_GCPMAJORMINOR`: shift from major back to minor collections (percent).
    MajorMinor = 1,
    /// `LUA_GCPMINORMAJOR`: shift from minor to major collections (percent).
    MinorMajor = 2,
    /// `LUA_GCPPAUSE`: pause between cycles (percent).
    Pause = 3,
    /// `LUA_GCPSTEPMUL`: step multiplier (percent).
    StepMul = 4,
    /// `LUA_GCPSTEPSIZE`: step size (bytes).
    StepSize = 5,
}

/// A value stored in the registry by [`Stack::reference`](trait.Stack.html#method.reference) (`luaL_ref`). Not `Clone`: release it
/// with [`Stack::unreference`](trait.Stack.html#method.unreference) exactly once. A key only has meaning in the state that made it.
#[derive(Debug, PartialEq, Eq, Hash)]
pub struct RegistryKey(pub(crate) i32);

/// Outcome of a successful [`Stack::resume`](trait.Stack.html#method.resume): the number of values pushed.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Resume {
    /// The coroutine yielded these values; it can be resumed again.
    Yield(usize),
    /// The coroutine's body returned these values; it is dead now.
    Return(usize),
}

/// Status of a coroutine, as `coroutine.status` reports it.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum ThreadStatus {
    /// Not started yet or yielded: can be resumed.
    Suspended,
    /// The thread asking (the coroutine is the one running).
    Running,
    /// Active but not running: it resumed another coroutine.
    Normal,
    /// Finished or failed.
    Dead,
}

// ---- low-level entry points used by the crate's tests and tools (not part of the API) ---------

/// The `rn-core` [`Loader`](rn_core::state::Loader) backed by `rn_compiler::load`: `mode` is
/// the `load` mode string (`"b"`, `"t"`, `"bt"`); the error is the reference-identical message.
#[doc(hidden)]
pub fn compiler_loader(
    heap: &mut rn_core::heap::Heap,
    chunk: &[u8],
    chunk_name: &[u8],
    mode: &[u8],
) -> std::result::Result<rn_core::protodata::ProtoData<rn_core::string::StrRef>, Vec<u8>> {
    let mode = rn_compiler::LoadMode {
        text: mode.contains(&b't'),
        binary: mode.contains(&b'b'),
    };
    rn_compiler::load(heap, chunk, chunk_name, mode).map_err(|e| e.message)
}

/// A raw `rn-core` state on the system allocator with the compiler installed as chunk loader
/// (internal tests and `rnvmrun`; the embedding API is [`Lua`]).
#[doc(hidden)]
pub fn new_state(seed: u32) -> rn_core::status::LuaResult<rn_core::state::LuaState> {
    let mut st = rn_core::state::State::with_seed(seed)?;
    st.set_loader(compiler_loader);
    Ok(st)
}
