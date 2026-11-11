//! [`Lua`] (a state and its owner) and [`Builder`].
use crate::callback;
use crate::error::{Error, ErrorKind, Result};
use crate::shared;
use crate::stack::{Handle, Stack};
use crate::GcMode;
use rn_core::heap::Heap;
use rn_core::protodata::ProtoData;
use rn_core::state::{LuaState, NativeFn, State, LUA_MASKCOUNT};
use rn_core::string::StrRef;
use rn_core::value::Value;
use std::ops::{BitOr, BitOrAssign};

/// Execution mode of the optional x86-64 JIT compiler (cargo feature `jit`; see `docs/JIT.md`).
///
/// The default is [`JitMode::Off`], also when the feature is compiled in and also for
/// [`Builder::sandbox`] states. Without the feature, or on a host that forbids executable memory,
/// every mode behaves like `Off` (results are identical in all modes; only speed differs).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum JitMode {
    /// Interpreter only (default).
    #[default]
    Off,
    /// Compile hot functions and loops (about 200 calls / 1000 back-edges).
    On,
    /// Compile every function on its first call (testing; slower to start).
    Always,
}

/// Counters of the JIT compiler for one state (see [`Lua::jit_stats`]).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct JitStats {
    /// Prototypes compiled to machine code.
    pub compiled: u64,
    /// Entries into compiled code.
    pub enters: u64,
    /// Bytes of machine code generated.
    pub code_bytes: u64,
    /// Compilations refused for a transient reason (memory limit or code cap).
    pub refused: u64,
}

/// A set of standard libraries (`luaL_openselectedlibs` flags).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct StdLib(u16);

impl StdLib {
    /// No library at all.
    pub const NONE: StdLib = StdLib(0);
    /// The basic library (`_G`: `print`, `pairs`, `pcall`, `load`, ...).
    pub const BASE: StdLib = StdLib(1);
    /// `package` and `require` (searches the file system; can load C modules only when the C API
    /// crate is linked and initialized).
    pub const PACKAGE: StdLib = StdLib(1 << 1);
    /// `coroutine`.
    pub const COROUTINE: StdLib = StdLib(1 << 2);
    /// `table`.
    pub const TABLE: StdLib = StdLib(1 << 3);
    /// `io` (files, processes).
    pub const IO: StdLib = StdLib(1 << 4);
    /// `os` (time, environment, `os.execute`, `os.remove`, `os.exit`, ...).
    pub const OS: StdLib = StdLib(1 << 5);
    /// `string`.
    pub const STRING: StdLib = StdLib(1 << 6);
    /// `math`.
    pub const MATH: StdLib = StdLib(1 << 7);
    /// `utf8`.
    pub const UTF8: StdLib = StdLib(1 << 8);
    /// `debug` (can break the invariants other code relies on; never give it to untrusted code).
    pub const DEBUG: StdLib = StdLib(1 << 9);
    /// LuaJIT-compatible data extensions: `table.new`, `table.clear` and the `string.buffer` module (reachable through
    /// `require` when `package` is open). Safe for untrusted code: no system access, memory charged to the state's
    /// limit. Not part of [`StdLib::ALL`] (which is `luaL_openlibs`); see docs/EXTENSIONS.md.
    pub const EXT: StdLib = StdLib(1 << 10);
    /// The `jit` control table (`jit.status()`, `jit.on()`, `jit.off()`, `jit.flush()`, ...). It only switches this
    /// state's compiler, but untrusted code has no business doing that: not in [`StdLib::SAFE`] nor [`StdLib::ALL`].
    pub const JIT: StdLib = StdLib(1 << 11);
    /// Every library (`luaL_openlibs`).
    pub const ALL: StdLib = StdLib((1 << 10) - 1);
    /// The libraries without access to the system or to internals: base, coroutine, table,
    /// string, math, utf8 (no `io`, `os`, `package`, `debug`).
    pub const SAFE: StdLib = StdLib(
        Self::BASE.0
            | Self::COROUTINE.0
            | Self::TABLE.0
            | Self::STRING.0
            | Self::MATH.0
            | Self::UTF8.0,
    );

    /// Does this set contain every library of `other`?
    pub const fn contains(self, other: StdLib) -> bool {
        self.0 & other.0 == other.0
    }

    /// The union of two sets.
    pub const fn union(self, other: StdLib) -> StdLib {
        StdLib(self.0 | other.0)
    }

    /// This set without the libraries of `other`.
    pub const fn difference(self, other: StdLib) -> StdLib {
        StdLib(self.0 & !other.0)
    }
}

impl BitOr for StdLib {
    type Output = StdLib;
    fn bitor(self, rhs: StdLib) -> StdLib {
        self.union(rhs)
    }
}

impl BitOrAssign for StdLib {
    fn bitor_assign(&mut self, rhs: StdLib) {
        *self = self.union(rhs);
    }
}

/// The libraries in `luaL_openlibs` order.
const LIBS: [(StdLib, &[u8], NativeFn); 10] = {
    use rn_std::*;
    [
        (StdLib::BASE, b"_G", lib_base::open),
        (StdLib::PACKAGE, b"package", lib_package::open),
        (StdLib::COROUTINE, b"coroutine", lib_coroutine::open),
        (StdLib::TABLE, b"table", lib_table::open),
        (StdLib::IO, b"io", lib_io::open),
        (StdLib::OS, b"os", lib_os::open),
        (StdLib::STRING, b"string", lib_string::open),
        (StdLib::MATH, b"math", lib_math::open),
        (StdLib::UTF8, b"utf8", lib_utf8::open),
        (StdLib::DEBUG, b"debug", lib_debug::open),
    ]
};

/// The chunk loader of sandboxed states: binary chunks are never accepted (`mode` loses `b`),
/// whatever the host or the script asks for.
fn text_only_loader(
    heap: &mut Heap,
    chunk: &[u8],
    chunk_name: &[u8],
    mode: &[u8],
) -> std::result::Result<ProtoData<StrRef>, Vec<u8>> {
    let m: Vec<u8> = mode.iter().copied().filter(|&c| c != b'b').collect();
    rn_std::loader(heap, chunk, chunk_name, &m)
}

/// Configuration of a new [`Lua`] state. Obtained from [`Lua::builder`].
///
/// ```
/// use rustynail::{Lua, StdLib};
/// # fn main() -> rustynail::Result<()> {
/// let mut lua = Lua::builder()
///     .libs(StdLib::BASE | StdLib::STRING)
///     .memory_limit(16 << 20)
///     .build()?;
/// lua.do_string("assert(string.rep('a', 3) == 'aaa')")?;
/// # Ok(()) }
/// ```
#[derive(Clone, Debug)]
pub struct Builder {
    libs: StdLib,
    memory_limit: Option<usize>,
    instruction_limit: Option<u32>,
    text_only: bool,
    sandbox: bool,
    seed: Option<u32>,
    gc_mode: Option<GcMode>,
    warnings: bool,
    jit: JitMode,
    jit_trace: Option<bool>,
}

impl Default for Builder {
    fn default() -> Self {
        Builder {
            libs: StdLib::ALL,
            memory_limit: None,
            instruction_limit: None,
            text_only: false,
            sandbox: false,
            seed: None,
            gc_mode: None,
            warnings: false,
            jit: JitMode::Off,
            jit_trace: None,
        }
    }
}

impl Builder {
    /// Libraries to open (default: [`StdLib::ALL`]).
    pub fn libs(mut self, libs: StdLib) -> Self {
        self.libs = libs;
        self
    }

    /// Fail allocations that would raise the state's memory use above `bytes` (see
    /// [`Lua::set_memory_limit`]).
    pub fn memory_limit(mut self, bytes: usize) -> Self {
        self.memory_limit = Some(bytes);
        self
    }

    /// Instruction budget (see [`Lua::set_instruction_limit`]).
    pub fn instruction_limit(mut self, instructions: u32) -> Self {
        self.instruction_limit = Some(instructions);
        self
    }

    /// Never accept precompiled (binary) chunks, neither from the host nor from scripts
    /// (`load`). Binary chunks are trusted input (there is no bytecode verifier), so a state
    /// running untrusted code must have this on.
    pub fn text_only(mut self, on: bool) -> Self {
        self.text_only = on;
        self
    }

    /// Sandbox preset for untrusted code (PLAN.md §6.1): [`StdLib::SAFE`] only, text-only loading,
    /// and no `dofile`/`loadfile` (file access) in the basic library. Combine with
    /// [`memory_limit`](trait.Stack.html#method.memory_limit) and [`instruction_limit`](trait.Stack.html#method.instruction_limit).
    /// `print` stays (it writes to the process's standard output); remove it with
    /// `lua.push_nil()?; lua.set_global("print")?` if that is not wanted.
    pub fn sandbox(mut self) -> Self {
        self.libs = StdLib::SAFE;
        self.text_only = true;
        self.sandbox = true;
        self
    }

    /// Seed of the string hash (default: a random seed per state, like `luai_makeseed`).
    pub fn seed(mut self, seed: u32) -> Self {
        self.seed = Some(seed);
        self
    }

    /// Initial collector mode (default: incremental, the library default; the standalone
    /// interpreter switches to generational).
    pub fn gc_mode(mut self, mode: GcMode) -> Self {
        self.gc_mode = Some(mode);
        self
    }

    /// Print Lua warnings (`warn(...)`) to standard error (default: off, as in `luaL_newstate`;
    /// scripts can still switch them on with `warn("@on")`).
    pub fn warnings(mut self, on: bool) -> Self {
        self.warnings = on;
        self
    }

    /// JIT mode of the new state (default [`JitMode::Off`]; the [`sandbox`](Builder::sandbox) preset does
    /// not change it, so a sandboxed state runs the JIT only if this is called explicitly). See
    /// [`Lua::set_jit_mode`].
    pub fn jit(mut self, mode: JitMode) -> Self {
        self.jit = mode;
        self
    }

    /// Enables or disables the J5 trace compiler for the new state (default: unchanged, i.e. on whenever the
    /// `jit` cargo feature is built and the JIT mode is not [`JitMode::Off`], unless the process sets
    /// `RUSTYNAIL_JIT_TRACE=off`). `false` gives the J4 behaviour exactly. Has no effect without the `jit`
    /// feature. See [`Lua::set_jit_trace`] and `docs/JIT.md`.
    pub fn jit_trace(mut self, on: bool) -> Self {
        self.jit_trace = Some(on);
        self
    }

    /// Create the state.
    pub fn build(self) -> Result<Lua> {
        #[cfg(feature = "jit")]
        register_jit();
        let seed = self.seed.unwrap_or_else(rn_core::auxlib::make_seed);
        let mut ls = State::with_seed(seed)
            .map_err(|_| Error::new(ErrorKind::Memory, "not enough memory"))?;
        if self.text_only {
            ls.set_loader(text_only_loader);
        } else {
            ls.set_loader(rn_std::loader);
        }
        ls.install_warn_function(false);
        let reg = ls.registry();
        let slot = ls
            .ref_(reg, Value::FALSE)
            .map_err(|_| Error::new(ErrorKind::Memory, "not enough memory"))?;
        shared::register(&ls, i64::from(slot));
        let handle = Handle {
            st: State::from_thread(ls.as_gc()),
        };
        let key = shared::state_key(&ls);
        let mut lua = Lua {
            handle,
            owner: Some(ls),
            key,
        };
        lua.open_libs(self.libs)?;
        if self.sandbox {
            lua.push_nil()?;
            lua.set_global("dofile")?;
            lua.push_nil()?;
            lua.set_global("loadfile")?;
        }
        if self.warnings {
            lua.do_string("warn('@on')")?;
            lua.set_top(0)?;
        }
        if let Some(m) = self.gc_mode {
            lua.set_gc_mode(m);
        }
        if let Some(n) = self.memory_limit {
            lua.set_memory_limit(Some(n));
        }
        if let Some(n) = self.instruction_limit {
            lua.set_instruction_limit(Some(n));
        }
        lua.set_jit_mode(self.jit);
        if let Some(on) = self.jit_trace {
            lua.set_jit_trace(on);
        }
        Ok(lua)
    }
}

/// Installs the compiler for the process (once). The state's own mode decides whether it runs: this
/// registers with the `On` configuration unless `RUSTYNAIL_JIT` is set (`off` then installs nothing, so
/// no state can enable it; `RUSTYNAIL_JIT_THRESHOLD`, `_PERFMAP`, `_VERBOSE` apply as in `rnlua`).
#[cfg(feature = "jit")]
fn register_jit() {
    if std::env::var_os("RUSTYNAIL_JIT").is_some() {
        rn_jit::register();
    } else {
        rn_jit::register_with(rn_jit::Config::for_mode(rn_jit::Mode::On));
    }
}

/// A Lua state: owns the main thread and everything reachable from it, and dereferences to the
/// main thread's [`Stack`] for all operations.
///
/// Dropping it closes the state (`lua_close`): pending `__close` variables run, then every `__gc`
/// finalizer, then all memory is released, including the Rust values owned by Lua.
///
/// A `Lua` is neither `Send` nor `Sync`: a state and all its callbacks belong to the thread that
/// created it.
///
/// # Rust stack usage
///
/// Lua calling Rust calling Lua (and metamethods) nests native frames; the depth is limited to
/// 200 such levels ("C stack overflow" error), as in the reference. Each level uses some
/// kilobytes of the *Rust* stack, so code that recurses deeply through Rust functions should run
/// on a thread with a large stack (8 MB or more, see [`std::thread::Builder::stack_size`]).
pub struct Lua {
    handle: Handle,
    owner: Option<LuaState>,
    key: usize,
}

impl Lua {
    /// A state with all standard libraries open (`luaL_newstate` + `luaL_openlibs`).
    pub fn new() -> Result<Lua> {
        Builder::default().build()
    }

    /// A state with only the given libraries open.
    pub fn with_libs(libs: StdLib) -> Result<Lua> {
        Builder::default().libs(libs).build()
    }

    /// A [`Builder`] for a custom configuration (limits, sandbox, libraries, ...).
    pub fn builder() -> Builder {
        Builder::default()
    }

    /// Open standard libraries into this state (each sets its global, like `luaL_requiref` with
    /// `glb = true`; also registered in `package.loaded` when `package` is open).
    pub fn open_libs(&mut self, libs: StdLib) -> Result<()> {
        for (flag, name, f) in LIBS {
            if libs.contains(flag) {
                let st = self.st_mut();
                let old = st.top();
                let mut ok = false;
                let status = st.pcall_with(
                    |st| {
                        st.check_stack(3)?;
                        st.requiref(name, f, true)?;
                        ok = true;
                        Ok(())
                    },
                    old,
                    0,
                );
                if status != rn_core::status::Status::Ok || !ok {
                    return Err(crate::stack::catch(st, status, false));
                }
                st.set_top(old);
            }
        }
        for (flag, open) in [
            (StdLib::EXT, rn_std::lib_ext::open_ext as fn(&mut State) -> rn_core::status::LuaResult<()>),
            (StdLib::JIT, rn_std::lib_ext::open_jit),
        ] {
            if libs.contains(flag) {
                let st = self.st_mut();
                let old = st.top();
                let status = st.pcall_with(
                    |st| {
                        st.check_stack(6)?;
                        open(st)
                    },
                    old,
                    0,
                );
                if status != rn_core::status::Status::Ok {
                    return Err(crate::stack::catch(st, status, false));
                }
                st.set_top(old);
            }
        }
        Ok(())
    }

    fn st_mut(&mut self) -> &mut State {
        &mut self.handle.st
    }

    /// Limit the total memory of the state to `bytes` (`None`: no limit). Allocations that would
    /// exceed it fail with a memory error ([`ErrorKind::Memory`]; after an emergency collection,
    /// as for a real out-of-memory condition). The limit counts everything the state allocated,
    /// including what is already in use: see [`Stack::gc_count`](trait.Stack.html#method.gc_count).
    pub fn set_memory_limit(&mut self, bytes: Option<usize>) {
        self.st_mut()
            .heap()
            .mem_mut()
            .set_limit(bytes.unwrap_or(usize::MAX));
    }

    /// Bytes currently allocated by the state.
    pub fn memory_used(&self) -> usize {
        self.gc_count()
    }

    /// Give Lua code a budget of `instructions` VM instructions (`None`: unlimited), checked by a
    /// count hook (`lua_sethook` with `LUA_MASKCOUNT`). When it runs out, an error of kind
    /// [`ErrorKind::InstructionLimit`] is raised that scripts cannot catch with `pcall`: it unwinds
    /// to the outermost protected call made by the host. The budget then restarts (the hook
    /// re-arms with the same count); call this method again to reset it explicitly.
    ///
    /// The count is per thread: coroutines created after this call inherit the hook with a
    /// budget of their own; the budget of the main thread keeps running while it drives them.
    /// An error that exhausts a coroutine's budget kills that coroutine. It replaces any hook set
    /// with `debug.sethook` (and `debug.sethook` can remove it: do not open `debug` for untrusted
    /// code). The budget is capped at `i32::MAX`.
    pub fn set_instruction_limit(&mut self, instructions: Option<u32>) {
        let st = self.st_mut();
        match instructions {
            Some(n) if n > 0 => {
                let n = n.min(i32::MAX as u32);
                st.set_hook(Some(callback::limit_hook), LUA_MASKCOUNT, n as i32);
                shared::with(st, |sh| {
                    sh.limit = n;
                    sh.limit_hit = false;
                });
            }
            _ => {
                st.set_hook(None, 0, 0);
                shared::with(st, |sh| {
                    sh.limit = 0;
                    sh.limit_hit = false;
                });
            }
        }
    }

    /// Selects the JIT mode of this state (default [`JitMode::Off`]). Returns whether compiled code can
    /// run for this state afterwards: false for `Off`, without the `jit` cargo feature, when the
    /// process disabled the compiler (`RUSTYNAIL_JIT=off`), or when the host forbids executable memory
    /// (the state then silently stays in the interpreter).
    ///
    /// Results are identical in every mode. Functions already compiled stay compiled but are not entered
    /// after switching to `Off`. `Always` lowers the process-wide compile thresholds (they apply to
    /// prototypes created afterwards, in all states). While a count hook is active (so with
    /// [`set_instruction_limit`](Lua::set_instruction_limit), or `debug.sethook`) compiled code is not
    /// entered at all: the interpreter runs everything and the instruction limit counts exactly as
    /// without the JIT. The JIT's code pages and its Rust-heap structures count against
    /// [`set_memory_limit`](Lua::set_memory_limit); when the limit refuses a compilation the function just
    /// stays interpreted. The JIT is not enabled by the [`Builder::sandbox`] preset (see `docs/JIT.md`).
    pub fn set_jit_mode(&mut self, mode: JitMode) -> bool {
        #[cfg(feature = "jit")]
        {
            let m = match mode {
                JitMode::Off => rn_jit::Mode::Off,
                JitMode::On => rn_jit::Mode::On,
                JitMode::Always => rn_jit::Mode::Always,
            };
            let st = self.st_mut();
            rn_jit::set_mode(st, m);
            mode != JitMode::Off && rn_jit::available()
        }
        #[cfg(not(feature = "jit"))]
        {
            let _ = mode;
            false
        }
    }

    /// Enables (`true`) or disables (`false`) the J5 trace compiler for this state. With the `jit` cargo feature
    /// traces are on by default (`RUSTYNAIL_JIT_TRACE`, default `on`); `false` restores the J4 behaviour. Returns
    /// whether traces can run for this state afterwards: false when disabled, without the `jit` feature, when the
    /// compiler is not installed, or when the process set `RUSTYNAIL_JIT_TRACE=off` (the trace driver is then not
    /// installed). Traces only run while the JIT mode is not [`JitMode::Off`]. Results are identical either way.
    pub fn set_jit_trace(&mut self, on: bool) -> bool {
        #[cfg(feature = "jit")]
        {
            rn_jit::set_trace_enabled(self.st_mut(), on)
        }
        #[cfg(not(feature = "jit"))]
        {
            let _ = on;
            false
        }
    }

    /// Counters of the JIT for this state; `None` without the `jit` feature or when the compiler is not
    /// installed in the process.
    pub fn jit_stats(&mut self) -> Option<JitStats> {
        #[cfg(feature = "jit")]
        {
            if !rn_jit::available() {
                return None;
            }
            let s = rn_jit::stats(self.st_mut());
            Some(JitStats {
                compiled: s.compiled,
                enters: s.enters,
                code_bytes: s.code_bytes,
                refused: s.refused,
            })
        }
        #[cfg(not(feature = "jit"))]
        {
            None
        }
    }

    /// The current instruction budget per [`set_instruction_limit`](trait.Stack.html#method.set_instruction_limit)
    /// (`None`: unlimited).
    pub fn instruction_limit(&self) -> Option<u32> {
        shared::with(&self.handle.st, |sh| sh.limit).filter(|&n| n > 0)
    }
}

impl std::ops::Deref for Lua {
    type Target = dyn Stack;
    fn deref(&self) -> &(dyn Stack + 'static) {
        &self.handle
    }
}

impl std::ops::DerefMut for Lua {
    fn deref_mut(&mut self) -> &mut (dyn Stack + 'static) {
        &mut self.handle
    }
}

impl Drop for Lua {
    fn drop(&mut self) {
        // close the state first: finalizers still find the per-state data
        drop(self.owner.take());
        shared::unregister(self.key);
    }
}

impl std::fmt::Debug for Lua {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Lua")
            .field("version", &crate::VERSION)
            .field("memory_used", &self.memory_used())
            .finish()
    }
}
