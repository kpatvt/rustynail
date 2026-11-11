//! Global state, threads and the per-thread stack (port of `lstate.c` + the stack half of `ldo.c`).
//!
//! # Model
//! * [`GlobalState`] (`global_State`) is boxed (stable address) and **owns the [`Heap`] by value**, the
//!   registry, the per-type metatables, the pre-interned names and the host-installed slots
//!   (executor, loader, panic, warn function, var-info hook).
//! * [`Thread`] (`lua_State`) is a GC object allocated with [`LUA_EXTRASPACE`] bytes *before* it, so
//!   `lua_getextraspace` works. Its stack is one contiguous `Value` buffer. Positions in the stack are
//!   **indices** ([`StkId`]), never pointers, so a reallocation cannot leave a stale position behind;
//!   the only pointers into the stack are the `v` fields of open upvalues, which are re-derived on
//!   every reallocation (`correctstack`). Raw pointers for the interpreter's hot loop come from
//!   [`State::slot_ptr`]; a debug-only generation counter ([`State::stack_gen`]) lets the interpreter
//!   assert it never uses a pointer across a reallocation.
//! * [`State`] is a cheap *handle* (`NonNull<Thread>`) to a thread: `&mut State` is what native
//!   functions receive. No long-lived `&mut Thread` is ever created (every accessor is a short
//!   re-borrow), so the collector may traverse any thread through raw pointers while a `State` method
//!   is running without aliasing violations.
//! * Errors are `Result<_, Status>`. The Lua error *object* of a thrown error is left on top of the
//!   stack (`State::error_value`), as `luaG_errormsg` leaves it before `luaD_throw`; `ErrMem` carries
//!   no object (`luaD_seterrorobj` substitutes the preallocated message).
//!
//! `unsafe` here: raw access to the thread, its stack buffer and `CallInfo` nodes, upvalue pointer
//! fix-up. **Invariants:** (ST1) a `State`/`Gc<Thread>` points to a live thread of a live
//! `GlobalState`; (ST2) `stack.cap() == stack_size + EXTRA_STACK` and every slot of the buffer is an
//! initialised `Value` once `stack_init` succeeded; `top <= stack_size + EXTRA_STACK`; (ST3) every
//! open upvalue `v` points into the stack buffer and is re-derived on reallocation; (ST4) `ci` is
//! `base_ci` or a node of the thread's doubly linked `CallInfo` list, nodes after `ci` are free
//! (CI1: a [`Ci`] is valid until `shrink_ci`/thread free, which only free nodes after the current
//! `ci`); (ST5) `twups == self` iff the thread is not in the global `twups` list.
use crate::alloc::{Allocator, Mem, RawBuf};
use crate::func::{TbcList, UpVal};
use crate::gc::{GcHeader, GCSTP_CLOSING, GCSTP_GC};
use crate::heap::Heap;
use crate::host::CompilerHost;
use crate::protodata::ProtoData;
use crate::status::{LuaResult, Status};
use crate::string::StrRef;
use crate::table::{TableError, TableRef};
use crate::value::*;
use std::ptr::NonNull;

// ---------------------------------------------------------------------------------------------
// constants (lua.h / lstate.h / ldo.c / llimits.h)
// ---------------------------------------------------------------------------------------------

/// `EXTRA_STACK`.
pub const EXTRA_STACK: usize = 5;
/// `LUA_MINSTACK`.
pub const LUA_MINSTACK: usize = 20;
/// `BASIC_STACK_SIZE`.
pub const BASIC_STACK_SIZE: usize = 2 * LUA_MINSTACK;
/// `LUAI_MAXSTACK` (ldo.c; not public in 5.5).
pub const LUAI_MAXSTACK: usize = 1_000_000;
/// `STACKERRSPACE`.
pub const STACKERRSPACE: usize = 200;
/// `ERRORSTACKSIZE`.
pub const ERRORSTACKSIZE: usize = LUAI_MAXSTACK + STACKERRSPACE;
/// `LUAI_MAXCCALLS` (llimits.h).
pub const LUAI_MAXCCALLS: u32 = 200;
/// `LUA_MULTRET`.
pub const LUA_MULTRET: i32 = -1;
/// `MAXRESULTS`.
pub const MAXRESULTS: i32 = 250;
/// `LUA_EXTRASPACE`.
pub const LUA_EXTRASPACE: usize = std::mem::size_of::<*mut ()>();
/// Bytes reserved in front of every thread (keeps the thread 16-byte aligned).
const THREAD_PREFIX: usize = 16;
/// Message for memory errors (`MEMERRMSG`).
pub const MEMERRMSG: &[u8] = b"not enough memory";

/// Registry slots (`LUA_RIDX_*`), per version.
#[cfg(not(rustynail_lua54))]
pub const LUA_RIDX_GLOBALS: i64 = 2;
#[cfg(not(rustynail_lua54))]
pub const LUA_RIDX_MAINTHREAD: i64 = 3;
#[cfg(rustynail_lua54)]
pub const LUA_RIDX_MAINTHREAD: i64 = 1;
#[cfg(rustynail_lua54)]
pub const LUA_RIDX_GLOBALS: i64 = 2;
/// `LUA_RIDX_LAST` (the highest of the two).
pub const LUA_RIDX_LAST: i64 = if LUA_RIDX_GLOBALS > LUA_RIDX_MAINTHREAD {
    LUA_RIDX_GLOBALS
} else {
    LUA_RIDX_MAINTHREAD
};

/// Hook masks / events (`lua.h`).
pub const LUA_MASKCALL: u8 = 1;
pub const LUA_MASKRET: u8 = 2;
pub const LUA_MASKLINE: u8 = 4;
pub const LUA_MASKCOUNT: u8 = 8;
pub const LUA_HOOKCALL: i32 = 0;
pub const LUA_HOOKRET: i32 = 1;
pub const LUA_HOOKLINE: i32 = 2;
pub const LUA_HOOKCOUNT: i32 = 3;
pub const LUA_HOOKTAILCALL: i32 = 4;

// `CIST_*` (lstate.h)
pub const CIST_NRESULTS: u32 = 0xff;
pub const CIST_CCMT: u32 = 8;
pub const MAX_CCMT: u32 = 0xf << CIST_CCMT;
pub const CIST_RECST: u32 = 12;
pub const CIST_C: u32 = 1 << (CIST_RECST + 3);
pub const CIST_FRESH: u32 = CIST_C << 1;
pub const CIST_CLSRET: u32 = CIST_FRESH << 1;
pub const CIST_TBC: u32 = CIST_CLSRET << 1;
pub const CIST_OAH: u32 = CIST_TBC << 1;
pub const CIST_HOOKED: u32 = CIST_OAH << 1;
pub const CIST_YPCALL: u32 = CIST_HOOKED << 1;
pub const CIST_TAIL: u32 = CIST_YPCALL << 1;
pub const CIST_HOOKYIELD: u32 = CIST_TAIL << 1;
pub const CIST_FIN: u32 = CIST_HOOKYIELD << 1;
/// (Not in the reference.) A Lua metamethod frame pushed by the interpreter itself instead of a
/// nested `ccall` (`vm_unsafe::mm_frame`): when it returns, the interpreter finishes the
/// caller's instruction with the metamethod's result (`vm_unsafe::mm_return`).
pub const CIST_MMRET: u32 = CIST_FIN << 1;
/// (JIT, placement (d).) The frame's function has compiled code (`Proto.jit != null`).
#[cfg(feature = "jit_check_d")]
pub const CIST_JIT: u32 = 1 << 27;
/// `CIST_LEQ` (5.4, `LUA_COMPAT_LT_LE`): using `__lt` for `__le`.
#[cfg(rustynail_lua54)]
pub const CIST_LEQ: u32 = CIST_MMRET << 1;

// ---------------------------------------------------------------------------------------------
// function-pointer types (the native-function contract)
// ---------------------------------------------------------------------------------------------

/// A native (Rust) function. Arguments are the stack slice `[func + 1, top)` ([`State::args`]);
/// results are pushed on top and their number is returned (the `lua_CFunction` convention).
pub type NativeFn = fn(&mut State) -> LuaResult<usize>;
/// Continuation of a native function after a yield (`lua_KFunction`): `(state, status, ctx)`.
pub type KFn = fn(&mut State, Status, isize) -> LuaResult<usize>;
/// The interpreter entry (`luaV_execute`), installed by task B4. Runs the Lua function whose frame is
/// `ci` until the frame marked `CIST_FRESH` returns.
pub type ExecuteFn = fn(&mut State, Ci) -> LuaResult<()>;
/// Debug hook (the debug task fills in the `lua_Debug` plumbing; the hook sees `State::ci()`).
/// Set (asynchronously, e.g. by a SIGINT handler) to make the running Lua code raise
/// `interrupted!` at the next call, return or jump: the equivalent of the reference `lua.c`
/// installing `lstop` as a hook from its signal handler, without touching interpreter state from
/// signal context (only an atomic store).
pub static INTERRUPT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// P-L: the ONE word a running J5 trace polls once per loop iteration (it replaces the two checks of `ci.trap` and
/// [`INTERRUPT`]). Nonzero means "an asynchronous event may be pending: leave the trace and let the interpreter look":
/// [`request_interrupt`] sets it after [`INTERRUPT`], and [`State::set_hook`] with a nonzero mask sets it after the
/// `trap` stores of the frames. The trace's poll exit clears it **before** control reaches the interpreter, which then
/// examines `INTERRUPT` and the hook mask (a flag is cleared before the state it announces is examined, set after the
/// state is changed: no event is lost; a stale flag costs one spurious exit). Process-wide like [`INTERRUPT`].
pub static TRACE_POLL: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);

/// Request an interrupt (async-signal-safe).
pub fn request_interrupt() {
    INTERRUPT.store(true, std::sync::atomic::Ordering::Relaxed);
    TRACE_POLL.store(1, std::sync::atomic::Ordering::Release);
}

/// A debug hook; an `Err` (error or yield) propagates through the interpreter like in `luaD_hook`.
pub type HookFn = fn(&mut State, event: i32, line: i32) -> LuaResult<()>;
/// Warning function: `(message piece, to be continued)` (`lua_WarnFunction`).
pub type WarnFn = Box<dyn FnMut(&[u8], bool)>;
/// Installed by the facade so `rn-core` does not depend on `rn-compiler`:
/// `(heap, chunk, chunk name, mode string) -> proto | error message`.
pub type Loader = fn(&mut Heap, &[u8], &[u8], &[u8]) -> Result<ProtoData<StrRef>, Vec<u8>>;
/// Variable-info hook (`varinfo` in ldebug.c): suffix like `" (local 'x')"` for an operand location.
pub type VarInfoFn = fn(&State, crate::error::OpLoc) -> Option<Vec<u8>>;

// ---------------------------------------------------------------------------------------------
// StkId
// ---------------------------------------------------------------------------------------------

/// Index of a slot in a thread's stack (an *offset*, stable across reallocation).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
pub struct StkId(pub usize);

impl StkId {
    #[inline]
    pub const fn idx(self) -> usize {
        self.0
    }
}
impl std::ops::Add<usize> for StkId {
    type Output = StkId;
    #[inline]
    fn add(self, n: usize) -> StkId {
        StkId(self.0 + n)
    }
}
impl std::ops::Sub<usize> for StkId {
    type Output = StkId;
    #[inline]
    fn sub(self, n: usize) -> StkId {
        StkId(self.0 - n)
    }
}
impl std::ops::Sub<StkId> for StkId {
    type Output = usize;
    #[inline]
    fn sub(self, o: StkId) -> usize {
        self.0 - o.0
    }
}
impl std::ops::AddAssign<usize> for StkId {
    #[inline]
    fn add_assign(&mut self, n: usize) {
        self.0 += n;
    }
}
impl std::ops::SubAssign<usize> for StkId {
    #[inline]
    fn sub_assign(&mut self, n: usize) {
        self.0 -= n;
    }
}

// ---------------------------------------------------------------------------------------------
// CallInfo
// ---------------------------------------------------------------------------------------------

/// Information about an active call (`CallInfo`). The reference's unions `u.l`/`u.c` are flattened.
#[repr(C)]
pub struct CallInfo {
    /// Function slot of this call.
    pub func: StkId,
    /// Top for this function.
    pub top: StkId,
    pub previous: *mut CallInfo,
    pub next: *mut CallInfo,
    /// Lua functions: next instruction (`savedpc`).
    pub savedpc: *const u32,
    /// Lua functions: function is tracing lines/counts (`trap`).
    pub trap: i32,
    /// Lua functions: number of extra (vararg) arguments.
    pub nextraargs: i32,
    /// Lua functions: the constants of the running closure (set when the interpreter enters the
    /// frame; keeps the pointer out of the interpreter's register set).
    pub kcache: *const Value,
    /// Native functions: continuation after a yield.
    pub k: Option<KFn>,
    /// Native functions: `old_errfunc` (stack index, 0 = none) during a yieldable `pcall`.
    pub old_errfunc: usize,
    /// Native functions: continuation context.
    pub ctx: isize,
    /// C API: the `lua_KFunction` pointer when `k` is the C continuation trampoline.
    pub ck: *const (),
    /// `u2`: `funcidx` / `nyield` / `nres`.
    pub u2: i32,
    /// `CIST_*` flags; bits 0-7 hold the expected number of results + 1.
    pub callstatus: u32,
    /// JIT counter variant C: loop back-edge budget of this activation.
    #[cfg(feature = "jit_counter_c")]
    pub hot: i32,
}

impl CallInfo {
    fn blank() -> CallInfo {
        CallInfo {
            func: StkId(0),
            top: StkId(0),
            previous: std::ptr::null_mut(),
            next: std::ptr::null_mut(),
            savedpc: std::ptr::null(),
            trap: 0,
            nextraargs: 0,
            kcache: std::ptr::null(),
            k: None,
            old_errfunc: 0,
            ctx: 0,
            ck: std::ptr::null(),
            u2: 0,
            callstatus: 0,
            #[cfg(feature = "jit_counter_c")]
            hot: 0,
        }
    }
    /// `isLua`.
    #[inline]
    pub fn is_lua(&self) -> bool {
        self.callstatus & CIST_C == 0
    }
    /// `isLuacode` (not running a hook).
    #[inline]
    pub fn is_lua_code(&self) -> bool {
        self.callstatus & (CIST_C | CIST_HOOKED) == 0
    }
    /// `get_nresults`.
    #[inline]
    pub fn nresults(&self) -> i32 {
        (self.callstatus & CIST_NRESULTS) as i32 - 1
    }
    /// `getcistrecst`.
    #[inline]
    pub fn recst(&self) -> u8 {
        ((self.callstatus >> CIST_RECST) & 7) as u8
    }
    /// `setcistrecst`.
    #[inline]
    pub fn set_recst(&mut self, st: u8) {
        debug_assert!(st < 8);
        self.callstatus = (self.callstatus & !(7 << CIST_RECST)) | ((st as u32) << CIST_RECST);
    }
}

/// Handle to a [`CallInfo`] node (`Copy`). See invariant CI1 in the module docs. `r()`/`w()` give
/// short-lived references; do not keep them across calls that may run Lua code.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Ci(NonNull<CallInfo>);

impl Ci {
    /// # Safety
    /// `p` must be non-null and point to a live `CallInfo` node of some thread.
    #[inline]
    pub unsafe fn from_raw(p: *mut CallInfo) -> Ci {
        // SAFETY: caller contract.
        Ci(unsafe { NonNull::new_unchecked(p) })
    }
    #[inline]
    pub fn as_ptr(self) -> *mut CallInfo {
        self.0.as_ptr()
    }
    /// Shared view of the node.
    #[inline]
    pub fn r<'a>(self) -> &'a CallInfo {
        // SAFETY: CI1 — the node is live while it is `ci` or before it in the list.
        unsafe { &*self.0.as_ptr() }
    }
    /// Exclusive view of the node (single-threaded interpreter; keep the borrow to one expression).
    #[inline]
    #[allow(clippy::mut_from_ref)]
    pub fn w<'a>(self) -> &'a mut CallInfo {
        // SAFETY: CI1; the node is only mutated by the owning thread's code, one expression at a time.
        unsafe { &mut *self.0.as_ptr() }
    }
    /// Previous (caller) frame, if any.
    #[inline]
    pub fn previous(self) -> Option<Ci> {
        let p = self.r().previous;
        if p.is_null() {
            None
        } else {
            // SAFETY: list invariant ST4.
            Some(unsafe { Ci::from_raw(p) })
        }
    }
    /// Next (callee) frame node, if allocated.
    #[inline]
    pub fn next(self) -> Option<Ci> {
        let p = self.r().next;
        if p.is_null() {
            None
        } else {
            // SAFETY: list invariant ST4.
            Some(unsafe { Ci::from_raw(p) })
        }
    }
}

// ---------------------------------------------------------------------------------------------
// GlobalState
// ---------------------------------------------------------------------------------------------

/// `global_State`.
pub struct GlobalState {
    /// Allocator, string table, object lists, collector state.
    pub heap: Heap,
    /// `l_registry` (a table value).
    pub registry: Value,
    /// Metatables of the basic types (`g->mt`), indexed by basic type number.
    pub mt: [Option<TableRef>; LUA_NUMTYPES as usize],
    /// Preallocated message for memory errors.
    pub memerrmsg: StrRef,
    /// Interned, fixed `"__name"`.
    pub name_key: StrRef,
    /// The interned, fixed string `"__tostring"` (`luaL_tolstring` looks it up on every conversion).
    pub tostring_key: StrRef,
    pub mainthread: *mut Thread,
    /// `lua_atpanic` function.
    pub panic: Option<NativeFn>,
    /// Warning function slot.
    pub warnf: Option<WarnFn>,
    /// The interpreter (task B4 installs the real one).
    pub executor: ExecuteFn,
    /// Chunk loader installed by the facade crate.
    pub loader: Option<Loader>,
    /// `varinfo` hook; the default returns nothing.
    pub varinfo: VarInfoFn,
    /// `completestate(g)`.
    pub complete: bool,
    /// B3-2: reusable output buffer of `string.format` / `table.concat` / `string.gsub` (taken while in use, so a
    /// nested call through a metamethod or callback simply finds it empty).
    pub scratch: Vec<u8>,
    /// Slots used by the C API crate (plain data, opaque to the core).
    pub capi: CapiGlobal,
    /// The registered JIT compiler's per-state data (`jit_hooks`).
    #[cfg(feature = "jit")]
    pub jit: crate::jit_hooks::JitSlot,
}

/// Opaque per-state data owned by the C API layer (`rn-capi`): C callback pointers and the
/// owner of the state. All null when the state was not created through the C API.
pub struct CapiGlobal {
    /// `lua_atpanic` function (`lua_CFunction`).
    pub panic: *const (),
    /// `lua_setwarnf`: function and user data.
    pub warnf: *const (),
    pub warnud: *mut std::ffi::c_void,
    /// `lua_newstate` allocator function and user data (`lua_getallocf`).
    pub allocf: *const (),
    pub allocud: *mut std::ffi::c_void,
    /// The boxed [`LuaState`] that owns this global state (`lua_close` drops it).
    pub owner: *mut LuaState,
}

impl CapiGlobal {
    const fn new() -> CapiGlobal {
        CapiGlobal {
            panic: std::ptr::null(),
            warnf: std::ptr::null(),
            warnud: std::ptr::null_mut(),
            allocf: std::ptr::null(),
            allocud: std::ptr::null_mut(),
            owner: std::ptr::null_mut(),
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Thread
// ---------------------------------------------------------------------------------------------

/// `lua_State`. `#[repr(C)]`, GC header first. [`LUA_EXTRASPACE`] bytes precede the object.
#[repr(C)]
pub struct Thread {
    pub(crate) hdr: GcHeader,
    pub(crate) allowhook: bool,
    pub(crate) status: Status,
    /// First free slot.
    pub(crate) top: StkId,
    pub(crate) g: *mut GlobalState,
    pub(crate) ci: *mut CallInfo,
    /// Stack buffer; capacity is `stack_size + EXTRA_STACK` once initialised.
    pub(crate) stack: RawBuf<Value>,
    /// `stacksize(L)` (== index of `stack_last`).
    pub(crate) stack_size: usize,
    pub(crate) openupval: *mut UpVal,
    pub(crate) tbc: TbcList,
    pub(crate) gclist: *mut GcHeader,
    pub(crate) twups: *mut Thread,
    pub(crate) base_ci: CallInfo,
    pub(crate) hook: Option<HookFn>,
    /// C API: the `lua_Hook` pointer when `hook` is the C hook trampoline.
    pub(crate) chook: *const (),
    /// Current message handler (stack index, 0 = none).
    pub(crate) errfunc: usize,
    /// Low 16 bits: nested native calls; high 16 bits: non-yieldable calls.
    pub(crate) ncalls: u32,
    pub(crate) oldpc: i32,
    pub(crate) nci: i32,
    pub(crate) basehookcount: i32,
    pub(crate) hookcount: i32,
    pub(crate) hookmask: u8,
    pub(crate) ftransfer: i32,
    pub(crate) ntransfer: i32,
    /// Bumped on every stack reallocation (checked by the interpreter in debug builds).
    pub(crate) stack_gen: u32,
    /// Nesting depth of `raw_run_protected` on this thread (the `errorJmp` chain length).
    pub(crate) prot_depth: u32,
    /// A throw to the base level (`luaD_throwbaselevel`) is in flight: only the outermost
    /// protected call of the thread may catch it.
    pub(crate) throw_base: bool,
    /// JIT counter variant B: global back-edge/frame-start sampling tick (counts down).
    #[cfg(feature = "jit_counter_b")]
    pub(crate) jit_tick: i32,
    /// R1 (`rearm`): the countdown the thread had when a hand-off to the interpreter armed the tick (0 = not rearmed).
    #[cfg(feature = "jit_counter_b")]
    pub(crate) jit_tick_saved: i32,
    /// R1 (`rearm`): back-edges the rearmed tick may still fire at (the span of the arming).
    #[cfg(feature = "jit_counter_b")]
    pub(crate) jit_rearm_left: u32,
    /// R1: the key (code address) of the prototype the arming was made for.
    #[cfg(feature = "jit_counter_b")]
    pub(crate) jit_rearm_key: usize,
}

impl Thread {
    /// Total size of the allocation holding a thread (prefix + object).
    pub(crate) const ALLOC_SIZE: usize = THREAD_PREFIX + std::mem::size_of::<Thread>();
}

/// `getCcalls`.
#[inline]
pub const fn get_ccalls(n: u32) -> u32 {
    n & 0xffff
}
/// `nyci`.
pub const NYCI: u32 = 0x10000 | 1;

impl Heap {
    /// Allocate and link a thread (`preinit_thread`): no stack yet, `g` set by the caller.
    pub(crate) fn new_thread_obj(&mut self) -> LuaResult<*mut Thread> {
        let raw = self.mem.alloc(Thread::ALLOC_SIZE)?;
        // SAFETY: fresh block of ALLOC_SIZE bytes (16-aligned); the thread lives at offset 16 so it
        // stays 16-aligned; the prefix is zeroed; the thread is fully initialised before linking.
        unsafe {
            std::ptr::write_bytes(raw.as_ptr(), 0, THREAD_PREFIX);
            let th = raw.as_ptr().add(THREAD_PREFIX).cast::<Thread>();
            th.write(Thread {
                hdr: GcHeader::new(self.allgc, LUA_VTHREAD, self.current_white),
                allowhook: true,
                status: Status::Ok,
                top: StkId(0),
                g: std::ptr::null_mut(),
                ci: std::ptr::null_mut(),
                stack: RawBuf::new(),
                stack_size: 0,
                openupval: std::ptr::null_mut(),
                tbc: TbcList::new(),
                gclist: std::ptr::null_mut(),
                twups: th,
                base_ci: CallInfo::blank(),
                hook: None,
                chook: std::ptr::null(),
                errfunc: 0,
                ncalls: 0,
                oldpc: 0,
                nci: 0,
                basehookcount: 0,
                hookcount: 0,
                hookmask: 0,
                ftransfer: 0,
                ntransfer: 0,
                stack_gen: 0,
                prot_depth: 0,
                throw_base: false,
                #[cfg(feature = "jit_counter_b")]
                jit_tick: crate::jit_hooks::tick_period(),
                #[cfg(feature = "jit_counter_b")]
                jit_tick_saved: 0,
                #[cfg(feature = "jit_counter_b")]
                jit_rearm_left: 0,
                #[cfg(feature = "jit_counter_b")]
                jit_rearm_key: 0,
            });
            self.allgc = th.cast();
            Ok(th)
        }
    }
}

/// `luaE_threadsize`.
pub(crate) unsafe fn thread_size(th: *const Thread) -> usize {
    // SAFETY: caller passes a live thread.
    unsafe {
        let mut sz = Thread::ALLOC_SIZE + (*th).nci as usize * std::mem::size_of::<CallInfo>();
        if (*th).stack.cap() != 0 {
            sz += (*th).stack.cap() * std::mem::size_of::<Value>();
        }
        sz
    }
}

/// `resetCI`.
///
/// # Safety
/// `th` must be a live thread whose stack is initialised.
unsafe fn reset_ci(th: *mut Thread) {
    // SAFETY: ST1/ST2.
    unsafe {
        let ci = &raw mut (*th).base_ci;
        (*th).ci = ci;
        (*ci).func = StkId(0);
        (*th).stack.as_ptr().write(Value::NIL);
        (*ci).top = StkId(1 + LUA_MINSTACK);
        (*ci).k = None;
        (*ci).callstatus = CIST_C;
        (*th).status = Status::Ok;
        (*th).errfunc = 0;
    }
}

/// `stack_init`.
///
/// # Safety
/// `th` must be a live thread without a stack.
unsafe fn stack_init(th: *mut Thread, mem: &mut Mem) -> LuaResult<()> {
    // SAFETY: ST1; the buffer is fully initialised before use.
    unsafe {
        (*th)
            .stack
            .try_resize(mem, BASIC_STACK_SIZE + EXTRA_STACK)?;
        let p = (*th).stack.as_ptr();
        for i in 0..BASIC_STACK_SIZE + EXTRA_STACK {
            p.add(i).write(Value::NIL);
        }
        (*th).stack_size = BASIC_STACK_SIZE;
        reset_ci(th);
        (*th).top = StkId(1);
        Ok(())
    }
}

/// `luaE_extendCI`: add a node after `ci` (the current one).
///
/// # Safety
/// `th` must be a live thread with an initialised `ci`.
pub(crate) unsafe fn extend_ci(th: *mut Thread, mem: &mut Mem) -> LuaResult<*mut CallInfo> {
    // SAFETY: ST4.
    unsafe {
        let raw = mem.alloc(std::mem::size_of::<CallInfo>())?;
        let ci = raw.as_ptr().cast::<CallInfo>();
        ci.write(CallInfo::blank());
        let cur = (*th).ci;
        (*ci).next = (*cur).next;
        (*ci).previous = cur;
        (*cur).next = ci;
        if !(*ci).next.is_null() {
            (*(*ci).next).previous = ci;
        }
        (*th).nci += 1;
        Ok(ci)
    }
}

/// `freeCI`: free all nodes after the current `ci`.
///
/// # Safety
/// `th` must be a live thread.
unsafe fn free_ci(th: *mut Thread, mem: &mut Mem) {
    // SAFETY: ST4 — nodes after `ci` are unused and were allocated with size_of::<CallInfo>().
    unsafe {
        let mut ci = (*th).ci;
        let mut next = (*ci).next;
        (*ci).next = std::ptr::null_mut();
        while !next.is_null() {
            ci = next;
            next = (*ci).next;
            mem.free(ci.cast(), std::mem::size_of::<CallInfo>());
            (*th).nci -= 1;
        }
    }
}

/// `luaE_shrinkCI`: free half of the unused nodes.
///
/// # Safety
/// `th` must be a live thread.
pub(crate) unsafe fn shrink_ci(th: *mut Thread, mem: &mut Mem) {
    // SAFETY: ST4.
    unsafe {
        let mut ci = (*(*th).ci).next;
        if ci.is_null() {
            return;
        }
        loop {
            let next = (*ci).next;
            if next.is_null() {
                break;
            }
            let next2 = (*next).next;
            (*ci).next = next2;
            (*th).nci -= 1;
            mem.free(next.cast(), std::mem::size_of::<CallInfo>());
            if next2.is_null() {
                break;
            }
            (*next2).previous = ci;
            ci = next2;
        }
    }
}

/// `stackinuse`.
unsafe fn stack_in_use(th: *mut Thread) -> usize {
    // SAFETY: ST1/ST4.
    unsafe {
        let mut lim = (*th).top;
        let mut ci = (*th).ci;
        while !ci.is_null() {
            if lim < (*ci).top {
                lim = (*ci).top;
            }
            ci = (*ci).previous;
        }
        debug_assert!(lim.0 <= (*th).stack_size + EXTRA_STACK);
        (lim.0 + 1).max(LUA_MINSTACK)
    }
}

/// `luaD_reallocstack`: resize the usable stack to `newsize` slots (`EXTRA_STACK` more are kept).
/// Open upvalues are re-pointed; on allocation failure nothing changes.
///
/// # Safety
/// `th` must be a live thread with an initialised stack; `newsize <= LUAI_MAXSTACK` or
/// `ERRORSTACKSIZE`, and `newsize > top`.
pub(crate) unsafe fn realloc_stack(
    th: *mut Thread,
    mem: &mut Mem,
    newsize: usize,
) -> LuaResult<()> {
    // SAFETY: ST1-ST4.
    unsafe {
        debug_assert!(newsize <= LUAI_MAXSTACK || newsize == ERRORSTACKSIZE);
        let oldsize = (*th).stack_size;
        let oldbase = (*th).stack.as_ptr();
        // relstack: stash upvalue slots as offsets (no provenance) while the block may move
        let mut up = (*th).openupval;
        while !up.is_null() {
            let off = (*up).v.offset_from(oldbase) as usize;
            (*up).v = std::ptr::without_provenance_mut(off);
            up = (*up).u.open.next;
        }
        let r = (*th).stack.try_resize(mem, newsize + EXTRA_STACK);
        // correctstack (also on failure: the buffer is unchanged then)
        let newbase = (*th).stack.as_ptr();
        let mut up = (*th).openupval;
        while !up.is_null() {
            let off = (*up).v.addr();
            (*up).v = newbase.add(off);
            up = (*up).u.open.next;
        }
        let mut ci = (*th).ci;
        while !ci.is_null() {
            if (*ci).callstatus & CIST_C == 0 {
                (*ci).trap = 1;
            }
            ci = (*ci).previous;
        }
        r?;
        (*th).stack_size = newsize;
        (*th).stack_gen = (*th).stack_gen.wrapping_add(1);
        for i in oldsize + EXTRA_STACK..newsize + EXTRA_STACK {
            newbase.add(i).write(Value::NIL);
        }
        Ok(())
    }
}

/// JIT stress (`RUSTYNAIL_JIT_STRESS=stackmove`): move the stack block to a fresh allocation of the same
/// size (the old block is freed, so stale base pointers read freed memory). Open upvalues are re-pointed;
/// unlike [`realloc_stack`] this does not set `trap` on the frames, so compiled code (which ignores it)
/// is the only thing that has to cope. Returns false (nothing changed) if the allocation failed.
///
/// # Safety
/// `th` must be a live thread with an initialised stack.
#[cfg(feature = "jit")]
pub(crate) unsafe fn move_stack(th: *mut Thread, mem: &mut Mem) -> bool {
    // SAFETY: ST1-ST4 as in `realloc_stack`; the new block has the same capacity as the old one.
    unsafe {
        let cap = (*th).stack.cap();
        let mut fresh: RawBuf<Value> = RawBuf::new();
        if fresh.try_resize(mem, cap).is_err() {
            return false;
        }
        let oldbase = (*th).stack.as_ptr();
        let newbase = fresh.as_ptr();
        std::ptr::copy_nonoverlapping(oldbase, newbase, cap);
        let mut up = (*th).openupval;
        while !up.is_null() {
            let off = (*up).v.offset_from(oldbase) as usize;
            (*up).v = newbase.add(off);
            up = (*up).u.open.next;
        }
        // poison the old block before releasing it (a stale pointer reads garbage, not the old values)
        std::ptr::write_bytes(
            oldbase.cast::<u8>(),
            0xA5,
            cap * std::mem::size_of::<Value>(),
        );
        let mut old = std::mem::replace(&mut (*th).stack, fresh);
        old.free(mem);
        (*th).stack_gen = (*th).stack_gen.wrapping_add(1);
        true
    }
}

/// `luaD_shrinkstack` (never fails).
///
/// # Safety
/// `th` must be a live thread with an initialised stack.
pub(crate) unsafe fn shrink_stack(th: *mut Thread, mem: &mut Mem) {
    // SAFETY: ST1-ST4.
    unsafe {
        let inuse = stack_in_use(th);
        let max = if inuse > LUAI_MAXSTACK / 3 {
            LUAI_MAXSTACK
        } else {
            inuse * 3
        };
        if inuse <= LUAI_MAXSTACK && (*th).stack_size > max {
            let nsize = if inuse > LUAI_MAXSTACK / 2 {
                LUAI_MAXSTACK
            } else {
                inuse * 2
            };
            let _ = realloc_stack(th, mem, nsize);
        }
        shrink_ci(th, mem);
    }
}

/// `luaF_closeupval` core usable from the collector (no `State`).
///
/// # Safety
/// `th` must be a live thread; `level` an index into its stack.
pub(crate) unsafe fn close_upvals_raw(heap: &mut Heap, th: *mut Thread, level: usize) {
    // SAFETY: ST3 — open upvalues point into the stack buffer.
    unsafe {
        let base = (*th).stack.as_ptr();
        loop {
            let uv = (*th).openupval;
            if uv.is_null() || ((*uv).v.offset_from(base) as usize) < level {
                break;
            }
            let slot: *mut Value = &raw mut (*uv).u.value;
            // luaF_unlinkupval
            let next = (*uv).u.open.next;
            let prev = (*uv).u.open.previous;
            *prev = next;
            if !next.is_null() {
                (*next).u.open.previous = prev;
            }
            slot.write(*(*uv).v);
            (*uv).v = slot;
            if !crate::gc::iswhite(uv.cast()) {
                crate::gc::nw2black(uv.cast());
                heap.barrier(uv.cast(), &*slot);
            }
        }
    }
}

/// `luaE_freethread`.
///
/// # Safety
/// `th` must be an unlinked thread of `heap` (dead).
pub(crate) unsafe fn free_thread(heap: &mut Heap, th: *mut Thread) {
    // SAFETY: ST1; the block was allocated by `new_thread_obj`.
    unsafe {
        if (*th).stack.cap() != 0 {
            close_upvals_raw(heap, th, 0);
            debug_assert!((*th).openupval.is_null());
            (*th).ci = &raw mut (*th).base_ci;
            free_ci(th, &mut heap.mem);
            debug_assert_eq!((*th).nci, 0);
            (*th).stack.free(&mut heap.mem);
        }
        (*th).tbc.free(&mut heap.mem);
        heap.mem
            .free(th.cast::<u8>().sub(THREAD_PREFIX), Thread::ALLOC_SIZE);
    }
}

// ---------------------------------------------------------------------------------------------
// State handle
// ---------------------------------------------------------------------------------------------

/// Handle to a thread (`lua_State*`). `&mut State` is the first argument of native functions.
/// Not `Send`/`Sync`.
pub struct State {
    th: NonNull<Thread>,
}

/// Owner of a whole Lua state (main thread + global state). Dropping it closes the state
/// (`lua_close`: pending `__close` variables, then every `__gc` finalizer, then frees everything).
pub struct LuaState {
    main: State,
    g: *mut GlobalState,
}

impl std::ops::Deref for LuaState {
    type Target = State;
    fn deref(&self) -> &State {
        &self.main
    }
}
impl std::ops::DerefMut for LuaState {
    fn deref_mut(&mut self) -> &mut State {
        &mut self.main
    }
}

impl Drop for LuaState {
    fn drop(&mut self) {
        self.main.close_state();
        // SAFETY: `g` came from `Box::into_raw` in `State::new` and is freed exactly once, here.
        drop(unsafe { Box::from_raw(self.g) });
    }
}

impl State {
    /// `lua_newstate`: create a state on a custom allocator with the given hash seed.
    #[allow(clippy::new_ret_no_self)]
    pub fn new(alloc: Box<dyn Allocator>, seed: u32) -> LuaResult<LuaState> {
        let mut heap = Heap::with_allocator(alloc, seed)?;
        heap.gc.stp = GCSTP_GC; // no GC while building the state
        let memerrmsg = heap.new_str(MEMERRMSG)?;
        heap.fix_any(memerrmsg.as_ptr().cast());
        let name_key = heap.new_str(b"__name")?;
        heap.fix_any(name_key.as_ptr().cast());
        let tostring_key = heap.new_str(b"__tostring")?;
        heap.fix_any(tostring_key.as_ptr().cast());
        let g = Box::new(GlobalState {
            heap,
            registry: Value::NIL,
            mt: [None; LUA_NUMTYPES as usize],
            memerrmsg,
            name_key,
            tostring_key,
            mainthread: std::ptr::null_mut(),
            panic: None,
            warnf: None,
            executor: crate::vm::execute,
            loader: None,
            varinfo: crate::error::default_varinfo,
            complete: false,
            scratch: Vec::new(),
            capi: CapiGlobal::new(),
            #[cfg(feature = "jit")]
            jit: crate::jit_hooks::JitSlot::new(),
        });
        let gp = Box::into_raw(g);
        // From here on `Box::from_raw(gp)` is the cleanup path (the heap frees partial objects).
        // SAFETY: `gp` is a fresh, uniquely owned box.
        match unsafe { Self::build(gp) } {
            Ok(main) => Ok(LuaState { main, g: gp }),
            Err(e) => {
                // SAFETY: `gp` is still uniquely owned.
                drop(unsafe { Box::from_raw(gp) });
                Err(e)
            }
        }
    }

    /// New state on the default allocator: the small-object [`PoolAllocator`], or plain `malloc`
    /// when `RUSTYNAIL_SYSTEM_MALLOC` is set (sanitizer runs, where pooled blocks would hide
    /// use-after-free).
    ///
    /// [`PoolAllocator`]: crate::alloc::PoolAllocator
    pub fn with_seed(seed: u32) -> LuaResult<LuaState> {
        if cfg!(miri) || std::env::var_os("RUSTYNAIL_SYSTEM_MALLOC").is_some() {
            Self::new(Box::new(crate::alloc::SystemAllocator), seed)
        } else {
            Self::new(Box::new(crate::alloc::PoolAllocator::new()), seed)
        }
    }

    /// `f_luaopen`.
    ///
    /// # Safety
    /// `gp` must be a live, uniquely owned `GlobalState`.
    unsafe fn build(gp: *mut GlobalState) -> LuaResult<State> {
        // SAFETY: caller contract; short-lived exclusive borrows only.
        unsafe {
            let th = (*gp).heap.new_thread_obj()?;
            (*th).g = gp;
            (*gp).mainthread = th;
            (*th).ncalls += 0x10000; // main thread is always non yieldable
            stack_init(th, &mut (*gp).heap.mem)?;
            let mut st = State {
                th: NonNull::new_unchecked(th),
            };
            // registry
            let heap = &mut (*gp).heap;
            let reg = heap.new_table()?;
            (*gp).registry = Value::table(reg);
            let te = |_e: TableError| Status::ErrMem;
            reg.resize(heap, LUA_RIDX_LAST as u32, 0).map_err(te)?;
            #[cfg(not(rustynail_lua54))]
            reg.set_int(heap, 1, &Value::FALSE).map_err(te)?;
            reg.set_int(heap, LUA_RIDX_MAINTHREAD, &Value::thread(Gc::from_raw(th)))
                .map_err(te)?;
            let globals = heap.new_table()?;
            reg.set_int(heap, LUA_RIDX_GLOBALS, &Value::table(globals))
                .map_err(te)?;
            (*gp).complete = true;
            (*gp).heap.gc.stp = 0;
            st.g_mut().heap.gc.threshold = 0;
            Ok(st)
        }
    }

    /// Handle for an existing thread.
    ///
    /// # Safety
    /// At most one `&mut State` for the same thread may be in use at a time (logical contract; the
    /// handle itself aliases nothing).
    pub unsafe fn from_gc(t: Gc<Thread>) -> State {
        State {
            // SAFETY: Gc handles are non-null.
            th: unsafe { NonNull::new_unchecked(t.as_ptr()) },
        }
    }

    /// `Gc` handle of this thread (for storing it in values).
    pub fn as_gc(&self) -> Gc<Thread> {
        // SAFETY: ST1.
        unsafe { Gc::from_raw(self.th.as_ptr()) }
    }

    // ---- raw access ---------------------------------------------------------------------------

    #[inline(always)]
    pub(crate) fn t(&self) -> &Thread {
        // SAFETY: ST1; a short re-borrow tied to `&self`.
        unsafe { &*self.th.as_ptr() }
    }
    #[inline(always)]
    pub(crate) fn tm(&mut self) -> &mut Thread {
        // SAFETY: ST1; a short re-borrow tied to `&mut self`, never kept across collector entry.
        unsafe { &mut *self.th.as_ptr() }
    }
    /// A handle to the thread at `th` (JIT helpers rebuild the handle from the pinned thread register).
    ///
    /// # Safety
    /// `th` must point to a live thread whose `State` handle is not being moved or dropped.
    #[cfg(feature = "jit")]
    pub(crate) unsafe fn from_th_ptr(th: *mut Thread) -> State {
        // SAFETY: caller contract (non-null live thread).
        let th = unsafe { NonNull::new_unchecked(th) };
        State { th }
    }
    #[inline(always)]
    pub(crate) fn th_ptr(&self) -> *mut Thread {
        self.th.as_ptr()
    }
    #[inline(always)]
    pub fn g(&self) -> &GlobalState {
        // SAFETY: ST1 — `g` outlives all threads.
        unsafe { &*self.t().g }
    }
    #[inline(always)]
    pub fn g_mut(&mut self) -> &mut GlobalState {
        // SAFETY: as above; short-lived exclusive borrow.
        unsafe { &mut *self.t().g }
    }
    /// The heap (allocator, strings, tables).
    #[inline(always)]
    pub fn heap(&mut self) -> &mut Heap {
        &mut self.g_mut().heap
    }

    // ---- stack primitives ---------------------------------------------------------------------

    /// `L->top`.
    #[inline(always)]
    pub fn top(&self) -> StkId {
        self.t().top
    }
    /// Set `top` (no checks beyond debug asserts).
    #[inline(always)]
    pub fn set_top(&mut self, t: StkId) {
        debug_assert!(t.0 <= self.t().stack_size + EXTRA_STACK);
        self.tm().top = t;
    }
    /// `stacksize`: usable slots (the index of `stack_last`).
    #[inline(always)]
    pub fn stack_size(&self) -> usize {
        self.t().stack_size
    }
    /// Value in a slot.
    #[inline(always)]
    pub fn get(&self, i: StkId) -> Value {
        debug_assert!(i.0 < self.t().stack_size + EXTRA_STACK);
        // SAFETY: ST2 — in-bounds slot of the initialised buffer.
        unsafe { Value::load(self.t().stack.as_ptr().add(i.0)) }
    }
    /// Store into a slot (the caller handles barriers when the slot belongs to a non-stack object).
    #[inline(always)]
    pub fn set(&mut self, i: StkId, v: Value) {
        debug_assert!(i.0 < self.t().stack_size + EXTRA_STACK);
        // SAFETY: ST2.
        unsafe { Value::store(self.t().stack.as_ptr().add(i.0), v) }
    }
    /// Raw pointer to a slot, for the interpreter's hot loop. Invalidated by anything that can
    /// reallocate the stack; compare [`State::stack_gen`] in debug builds.
    #[inline(always)]
    pub fn slot_ptr(&self, i: StkId) -> *mut Value {
        // SAFETY: in-bounds offset (or one-past) of the stack block.
        unsafe { self.t().stack.as_ptr().add(i.0) }
    }
    /// Debug-only generation counter of the stack block (always 0 in release builds).
    #[inline(always)]
    pub fn stack_gen(&self) -> u32 {
        if cfg!(debug_assertions) {
            self.t().stack_gen
        } else {
            0
        }
    }
    /// Push without checking (relies on `EXTRA_STACK`/a previous `check_stack`).
    #[inline(always)]
    pub fn push(&mut self, v: Value) {
        let t = self.top();
        self.set(t, v);
        self.set_top(t + 1);
    }
    /// Push after making sure there is room.
    pub fn push_checked(&mut self, v: Value) -> LuaResult<()> {
        self.check_stack(1)?;
        self.push(v);
        Ok(())
    }
    /// Pop and return the top value.
    #[inline(always)]
    pub fn pop(&mut self) -> Value {
        let t = self.top() - 1;
        self.set_top(t);
        self.get(t)
    }
    /// The slice `[from, to)` of the stack. Valid until the stack is next modified.
    pub fn slice(&self, from: StkId, to: StkId) -> &[Value] {
        debug_assert!(from <= to && to.0 <= self.t().stack_size + EXTRA_STACK);
        // SAFETY: ST2 — initialised, in-bounds; tied to `&self`.
        unsafe { std::slice::from_raw_parts(self.t().stack.as_ptr().add(from.0), to.0 - from.0) }
    }
    /// Arguments of the running native function: `[func + 1, top)`.
    pub fn args(&self) -> &[Value] {
        let ci = self.ci();
        self.slice(ci.r().func + 1, self.top())
    }
    /// Number of arguments of the running native function.
    pub fn nargs(&self) -> usize {
        self.top() - (self.ci().r().func + 1)
    }
    /// The `i`-th (0-based) argument, `nil` if absent.
    #[inline]
    pub fn arg(&self, i: usize) -> Value {
        let slot = self.ci().r().func + 1 + i;
        if slot < self.top() {
            // field-wise load: the caller just wrote the argument as tag byte + payload, and a
            // whole-`Value` copy would defeat store-to-load forwarding (see `Value::load`)
            self.get(slot)
        } else {
            Value::NIL
        }
    }

    // ---- CallInfo -----------------------------------------------------------------------------

    /// `L->ci`.
    #[inline(always)]
    pub fn ci(&self) -> Ci {
        // SAFETY: ST4 — `ci` is never null once the stack is initialised.
        unsafe { Ci::from_raw(self.t().ci) }
    }
    /// Set the current frame (ldo bookkeeping; the interpreter uses it for calls/returns).
    #[inline(always)]
    pub fn set_ci(&mut self, ci: Ci) {
        self.tm().ci = ci.as_ptr();
    }
    /// The base (host) frame.
    pub fn base_ci(&self) -> Ci {
        // SAFETY: base_ci is inline in the thread.
        unsafe { Ci::from_raw(&raw mut (*self.th.as_ptr()).base_ci) }
    }
    /// `next_ci` / `luaE_extendCI(L, 1)`.
    pub fn next_ci(&mut self) -> LuaResult<Ci> {
        let next = self.ci().r().next;
        if !next.is_null() {
            // SAFETY: list invariant.
            return Ok(unsafe { Ci::from_raw(next) });
        }
        let th = self.th_ptr();
        // SAFETY: ST1/ST4; `mem` and the thread are disjoint objects.
        let p = unsafe { extend_ci(th, &mut (*(*th).g).heap.mem) }?;
        // SAFETY: fresh node.
        Ok(unsafe { Ci::from_raw(p) })
    }
    /// Number of `CallInfo` nodes allocated beyond the base frame (`L->nci`).
    pub fn nci(&self) -> i32 {
        self.t().nci
    }

    // ---- thread bookkeeping accessors --------------------------------------------------------

    /// `L->status`.
    pub fn status(&self) -> Status {
        self.t().status
    }
    /// `lua_isyieldable`.
    pub fn yieldable(&self) -> bool {
        self.t().ncalls & 0xffff_0000 == 0
    }
    /// `getCcalls`.
    pub fn ccalls(&self) -> u32 {
        get_ccalls(self.t().ncalls)
    }
    /// Raw `nCcalls` word.
    pub fn ncalls_raw(&self) -> u32 {
        self.t().ncalls
    }
    pub fn set_ncalls_raw(&mut self, n: u32) {
        self.tm().ncalls = n;
    }
    /// `incnny`.
    pub fn inc_nny(&mut self) {
        self.tm().ncalls += 0x10000;
    }
    /// `decnny`.
    pub fn dec_nny(&mut self) {
        self.tm().ncalls -= 0x10000;
    }
    pub fn allow_hook(&self) -> bool {
        self.t().allowhook
    }
    pub fn set_allow_hook(&mut self, b: bool) {
        self.tm().allowhook = b;
    }
    /// Current message handler (stack index, 0 = none).
    pub fn errfunc(&self) -> usize {
        self.t().errfunc
    }
    pub fn set_errfunc(&mut self, e: usize) {
        self.tm().errfunc = e;
    }
    pub fn hook_mask(&self) -> u8 {
        self.t().hookmask
    }
    /// JIT counter variant B: decrement the sampling tick; true when it reached zero.
    #[cfg(all(feature = "jit_counter_b", not(feature = "jit_counter_g")))]
    #[inline(always)]
    pub(crate) fn jit_tick_dec(&mut self) -> bool {
        let t = self.tm();
        t.jit_tick -= 1;
        t.jit_tick == 0
    }
    /// N3b: the next sampling tick fires after `n` more back-edges (an exit that left compiled code for the interpreter wants the
    /// loop back in compiled code at its next back-edge, not after a whole period).
    #[cfg(all(feature = "jit_counter_b", not(feature = "jit_counter_g")))]
    pub(crate) fn jit_tick_arm(&mut self, n: i32) {
        let t = self.tm();
        t.jit_tick = n.max(1);
        // a normal (formation) tick supersedes a rearmed one
        t.jit_tick_saved = 0;
        t.jit_rearm_left = 0;
    }
    /// R1 (`rearm`): arms a tick at the next back-edge that is NOT a formation tick: the countdown is saved and restored by
    /// [`State::jit_tick_take_rearm`]. No-op when a tick is already due at the next back-edge, when a rearm is pending (the span is
    /// renewed) or under a hook. `span` = back-edges the arming may last (see `jit_hooks::rearm_tick`).
    #[cfg(all(feature = "jit_counter_b", not(feature = "jit_counter_g")))]
    pub(crate) fn jit_tick_rearm(&mut self, span: u32, key: usize) {
        if self.hook_mask() != 0 {
            return;
        }
        let t = self.tm();
        if t.jit_tick_saved == 0 {
            if t.jit_tick <= 1 {
                return;
            }
            t.jit_tick_saved = t.jit_tick;
        }
        t.jit_tick = 1;
        t.jit_rearm_left = span.max(1);
        t.jit_rearm_key = key;
    }
    /// R1: the prototype key of the pending (or last) arming.
    #[cfg(feature = "jit_counter_b")]
    pub(crate) fn jit_rearm_key(&self) -> usize {
        self.t().jit_rearm_key
    }
    /// R1: the countdown of the thread's sampling tick.
    #[cfg(feature = "jit_counter_b")]
    pub(crate) fn jit_tick_left(&self) -> i32 {
        self.t().jit_tick
    }
    /// R1: the tick that just fired was a rearmed one: continues the formation countdown and returns the span left (0 = a normal tick: this edge is also the one the formation countdown ends at).
    #[cfg(all(feature = "jit_counter_b", not(feature = "jit_counter_g")))]
    pub(crate) fn jit_tick_take_rearm(&mut self) -> u32 {
        let t = self.tm();
        if t.jit_tick_saved == 0 {
            return 0;
        }
        // the back-edge that fired the rearmed tick counts for the formation countdown like any other: the countdown continues from the saved value
        // minus one, and when that reaches zero this edge is also the formation tick (a normal one: the caller resets the period)
        let saved = std::mem::take(&mut t.jit_tick_saved);
        let left = std::mem::take(&mut t.jit_rearm_left);
        if saved <= 1 {
            return 0;
        }
        t.jit_tick = saved - 1;
        left.max(1)
    }
    #[cfg(feature = "jit_counter_g")]
    pub(crate) fn jit_tick_rearm(&mut self, _span: u32, _key: usize) {}
    #[cfg(feature = "jit_counter_g")]
    pub(crate) fn jit_tick_take_rearm(&mut self) -> u32 {
        0
    }
    #[cfg(feature = "jit_counter_g")]
    pub(crate) fn jit_tick_arm(&mut self, n: i32) {
        crate::jit_hooks::TICK.store(n.max(1), std::sync::atomic::Ordering::Relaxed);
    }
    #[cfg(all(feature = "jit_counter_b", not(feature = "jit_counter_g")))]
    pub(crate) fn jit_tick_reset(&mut self) {
        self.tm().jit_tick = crate::jit_hooks::tick_period();
    }
    /// Variant G: the tick is a process-global (relaxed, racy by design: it only paces sampling).
    #[cfg(feature = "jit_counter_g")]
    #[inline(always)]
    pub(crate) fn jit_tick_dec(&mut self) -> bool {
        use std::sync::atomic::Ordering::Relaxed;
        let v = crate::jit_hooks::TICK.load(Relaxed).wrapping_sub(1);
        crate::jit_hooks::TICK.store(v, Relaxed);
        v <= 0
    }
    #[cfg(feature = "jit_counter_g")]
    pub(crate) fn jit_tick_reset(&mut self) {
        crate::jit_hooks::TICK.store(
            crate::jit_hooks::tick_period(),
            std::sync::atomic::Ordering::Relaxed,
        );
    }
    /// Install (or clear) the debug hook (`lua_sethook`).
    /// `lua_sethook` for a C hook: `chook` is the `lua_Hook` pointer, `f` the trampoline that
    /// calls it.
    pub fn set_c_hook(&mut self, f: Option<HookFn>, chook: *const (), mask: u8, count: i32) {
        self.set_hook(f, mask, count);
        self.tm().chook = if f.is_some() && mask != 0 {
            chook
        } else {
            std::ptr::null()
        };
    }
    /// The C hook pointer installed by [`State::set_c_hook`] (null if none).
    pub fn c_hook(&self) -> *const () {
        self.t().chook
    }
    pub fn set_hook(&mut self, f: Option<HookFn>, mask: u8, count: i32) {
        let (f, mask) = if f.is_none() || mask == 0 {
            (None, 0)
        } else {
            (f, mask)
        };
        let t = self.tm();
        t.chook = std::ptr::null(); // a C hook sets it again after this call
        t.hook = f;
        t.basehookcount = count;
        t.hookcount = count;
        t.hookmask = mask;
        if mask != 0 {
            // to trace inside the interpreter loop (`settraps`)
            let mut ci = self.ci().as_ptr();
            while !ci.is_null() {
                // SAFETY: the CI list is valid (ST4); only Lua frames carry a meaningful `trap`.
                unsafe {
                    if (*ci).callstatus & CIST_C == 0 {
                        (*ci).trap = 1;
                    }
                    ci = (*ci).previous;
                }
            }
            // P-L: after the frames' `trap` stores (see `TRACE_POLL`)
            TRACE_POLL.store(1, std::sync::atomic::Ordering::Release);
        }
    }
    /// `lua_getextraspace`: the `LUA_EXTRASPACE` bytes in front of the thread.
    pub fn extra_space(&mut self) -> *mut u8 {
        // SAFETY: the prefix belongs to the thread allocation.
        unsafe { self.th.as_ptr().cast::<u8>().sub(LUA_EXTRASPACE) }
    }
    /// The main thread of this state.
    pub fn main_thread(&self) -> Gc<Thread> {
        // SAFETY: set in `build`.
        unsafe { Gc::from_raw(self.g().mainthread) }
    }
    pub fn is_main_thread(&self) -> bool {
        self.th.as_ptr() == self.g().mainthread
    }

    // ---- stack growth (ldo.c) -----------------------------------------------------------------

    /// `luaD_reallocstack` with `raiseerror`: `Ok(true)` on success, `Ok(false)` (or `Err(ErrMem)`
    /// when `raise`) if the allocation failed.
    pub fn realloc_stack(&mut self, newsize: usize, raise: bool) -> LuaResult<bool> {
        let th = self.th_ptr();
        // SAFETY: ST1; `mem` and thread are disjoint.
        let r = unsafe { realloc_stack(th, &mut (*(*th).g).heap.mem, newsize) };
        match r {
            Ok(()) => Ok(true),
            Err(e) if raise => Err(e),
            Err(_) => Ok(false),
        }
    }

    /// `luaD_growstack`: ensure at least `n` free slots beyond `top`.
    pub fn grow_stack(&mut self, n: usize, raise: bool) -> LuaResult<bool> {
        let size = self.stack_size();
        if size > LUAI_MAXSTACK {
            // already using the error space: handling a stack error
            debug_assert_eq!(size, ERRORSTACKSIZE);
            if raise {
                return Err(self.err_err());
            }
            return Ok(false);
        } else if n < LUAI_MAXSTACK {
            let mut newsize = size + (size >> 1);
            let needed = self.top().0 + n;
            if newsize > LUAI_MAXSTACK {
                newsize = LUAI_MAXSTACK;
            }
            if newsize < needed {
                newsize = needed;
            }
            if newsize <= LUAI_MAXSTACK {
                return self.realloc_stack(newsize, raise);
            }
        }
        // stack overflow: add extra size to be able to handle the error message
        self.realloc_stack(ERRORSTACKSIZE, raise)?;
        if raise {
            return Err(self.run_error(b"stack overflow"));
        }
        Ok(false)
    }

    /// `luaD_checkstack`: ensure `n` free slots (grows the stack if needed).
    #[inline(always)]
    pub fn check_stack(&mut self, n: usize) -> LuaResult<()> {
        // signed, like the pointer difference in C: `top` may be past `stack_last` while a stack
        // overflow error is being handled
        if (self.t().stack_size as isize) - (self.top().0 as isize) <= n as isize {
            self.grow_stack(n, true)?;
        }
        Ok(())
    }

    /// `luaD_shrinkstack`.
    pub fn shrink_stack(&mut self) {
        let th = self.th_ptr();
        // SAFETY: ST1.
        unsafe { shrink_stack(th, &mut (*(*th).g).heap.mem) }
    }

    /// C36 (JIT): makes sure at least `n` `CallInfo` nodes follow the current frame (a trace entry needs them, `TR6b`), extending the chain like
    /// `luaE_extendCI` does for a call. Returns how many nodes it added, `None` if an allocation failed.
    pub fn ensure_spare_ci(&mut self, n: usize) -> Option<usize> {
        let th = self.th_ptr();
        let mut have = 0;
        let mut c = self.ci().next();
        while let Some(x) = c {
            have += 1;
            if have >= n {
                return Some(0);
            }
            c = x.next();
        }
        let mut added = 0;
        while have + added < n {
            // SAFETY: ST1/ST4 (as in `check_min_stack`).
            if unsafe { extend_ci(th, &mut (*(*th).g).heap.mem) }.is_err() {
                return None;
            }
            added += 1;
        }
        Some(added)
    }

    /// `luaD_checkminstack`: enough resources to run a simple function (a finalizer)?
    pub fn check_min_stack(&mut self) -> bool {
        if self.ccalls() >= LUAI_MAXCCALLS - 2 {
            return false;
        }
        let th = self.th_ptr();
        // two free CallInfos after the current one
        if self.ci().next().is_none() {
            // SAFETY: ST1/ST4.
            if unsafe { extend_ci(th, &mut (*(*th).g).heap.mem) }.is_err() {
                return false;
            }
        }
        if self.ci().next().and_then(Ci::next).is_none() {
            // SAFETY: ST1/ST4.
            if unsafe { extend_ci(th, &mut (*(*th).g).heap.mem) }.is_err() {
                return false;
            }
        }
        if self.stack_size() - self.top().0 >= BASIC_STACK_SIZE {
            true
        } else {
            self.grow_stack(BASIC_STACK_SIZE, false).unwrap_or(false)
        }
    }

    // ---- object creation with emergency-GC retry ----------------------------------------------

    /// Run an allocation; on `ErrMem` perform an emergency collection and retry once (the reference
    /// does this inside `luaM_malloc_`).
    #[inline(always)]
    pub fn try_alloc<R>(&mut self, mut f: impl FnMut(&mut Heap) -> LuaResult<R>) -> LuaResult<R> {
        match f(self.heap()) {
            Err(Status::ErrMem) if self.can_emergency_gc() => {
                self.collect(true);
                f(self.heap())
            }
            // rebuilt field by field: handing `r` back unchanged makes the compiler copy the whole
            // result with a vector move right after it was written in two halves (a
            // store-forwarding stall)
            Ok(v) => Ok(v),
            Err(e) => Err(e),
        }
    }

    /// `luaS_newlstr` for the running state.
    pub fn new_str(&mut self, bytes: &[u8]) -> LuaResult<StrRef> {
        self.try_alloc(|h| h.new_str(bytes))
    }
    /// [`State::new_str`] for bytes written by byte stores right before (see [`crate::heap::Heap::new_str_fresh`]).
    pub fn new_str_fresh(&mut self, bytes: &[u8]) -> LuaResult<StrRef> {
        self.try_alloc(|h| h.new_str_fresh(bytes))
    }
    /// A string value.
    pub fn new_str_value(&mut self, bytes: &[u8]) -> LuaResult<Value> {
        Ok(Value::string(self.new_str(bytes)?))
    }
    /// `luaH_new`.
    pub fn new_table(&mut self) -> LuaResult<TableRef> {
        self.try_alloc(|h| h.new_table())
    }

    /// Raw table store with the reference's error texts and the write barrier (`luaH_set` as used
    /// by the VM and the API).
    pub fn raw_set(&mut self, t: TableRef, key: &Value, val: &Value) -> LuaResult<()> {
        match t.set(self.heap(), key, val) {
            Ok(()) => {
                self.heap().barrier_back(t.as_ptr().cast(), val);
                Ok(())
            }
            Err(e) => Err(self.table_error(e)),
        }
    }
    /// `luaH_setint` + barrier.
    pub fn raw_set_int(&mut self, t: TableRef, key: i64, val: &Value) -> LuaResult<()> {
        match t.set_int(self.heap(), key, val) {
            Ok(()) => {
                self.heap().barrier_back(t.as_ptr().cast(), val);
                Ok(())
            }
            Err(e) => Err(self.table_error(e)),
        }
    }
    /// Convert a [`TableError`] into a thrown Lua error.
    #[cold]
    pub fn table_error(&mut self, e: TableError) -> Status {
        match e {
            TableError::Mem => Status::ErrMem,
            TableError::NilIndex => self.run_error(b"table index is nil"),
            TableError::NanIndex => self.run_error(b"table index is NaN"),
            TableError::Overflow => self.run_error(b"table overflow"),
            TableError::InvalidNextKey => self.run_error(b"invalid key to 'next'"),
        }
    }

    // ---- registry / globals / metatables ------------------------------------------------------

    /// The registry table.
    pub fn registry(&self) -> TableRef {
        self.g().registry.as_table().expect("registry is a table")
    }
    /// The globals table (`registry[LUA_RIDX_GLOBALS]`).
    pub fn globals(&self) -> TableRef {
        self.registry()
            .get_int(LUA_RIDX_GLOBALS)
            .as_table()
            .expect("globals is a table")
    }
    /// Metatable of a basic type.
    pub fn type_metatable(&self, ty: LuaType) -> Option<TableRef> {
        self.g().mt[ty as usize]
    }
    /// Set the metatable of a basic type (`lua_setmetatable` on a non-table/userdata value).
    pub fn set_type_metatable(&mut self, ty: LuaType, mt: Option<TableRef>) {
        self.g_mut().mt[ty as usize] = mt;
        // (N3: a trace also relies on a number type having no metatable)
        #[cfg(feature = "jit")]
        crate::jit_hooks::type_mt_changed(self);
    }

    /// `lua_setmetatable` semantics for tables and full userdata (barrier + `__gc` registration);
    /// other values set the per-type metatable.
    pub fn set_metatable(&mut self, obj: &Value, mt: Option<TableRef>) {
        if let Some(t) = obj.as_table() {
            t.set_metatable(mt);
            if let Some(m) = mt {
                self.heap().barrier(t.as_ptr().cast(), &Value::table(m));
            }
            // SAFETY: `t` is live and its metatable is `mt`.
            unsafe { self.heap().check_finalizer(t.as_ptr().cast(), mt) };
        } else if let Some(u) = obj.as_userdata() {
            u.set_metatable(mt);
            if let Some(m) = mt {
                self.heap().barrier(u.as_ptr().cast(), &Value::table(m));
            }
            // SAFETY: `u` is live and its metatable is `mt`.
            unsafe { self.heap().check_finalizer(u.as_ptr().cast(), mt) };
        } else {
            self.g_mut().mt[obj.type_tag() as usize] = mt;
            #[cfg(feature = "jit")]
            crate::jit_hooks::type_mt_changed(self);
        }
    }

    // ---- warnings / panic / hooks slots -------------------------------------------------------

    /// Install the interpreter.
    pub fn set_executor(&mut self, f: ExecuteFn) {
        self.g_mut().executor = f;
    }
    /// Install the chunk loader (facade crate).
    pub fn set_loader(&mut self, f: Loader) {
        self.g_mut().loader = Some(f);
    }
    /// Install the `varinfo` hook (debug task).
    pub fn set_varinfo(&mut self, f: VarInfoFn) {
        self.g_mut().varinfo = f;
    }
    /// `lua_atpanic`: returns the previous function.
    pub fn set_panic(&mut self, f: Option<NativeFn>) -> Option<NativeFn> {
        std::mem::replace(&mut self.g_mut().panic, f)
    }
    /// `lua_setwarnf`.
    pub fn set_warn_function(&mut self, f: Option<WarnFn>) {
        self.g_mut().warnf = f;
    }
    /// `luaE_warning`.
    pub fn warning(&mut self, msg: &[u8], tocont: bool) {
        if let Some(w) = self.g_mut().warnf.as_mut() {
            w(msg, tocont);
        }
    }
    /// `luaE_warnerror`: warning `"error in <where> (<msg>)"` from the error object on top of the stack.
    pub fn warn_error(&mut self, whr: &[u8]) {
        let e = self.get(self.top() - 1);
        let msg: Vec<u8> = match e.as_string() {
            Some(s) => s.as_bytes().to_vec(),
            None => b"error object is not a string".to_vec(),
        };
        self.warning(b"error in ", true);
        self.warning(whr, true);
        self.warning(b" (", true);
        self.warning(&msg, true);
        self.warning(b")", false);
    }

    // ---- threads ------------------------------------------------------------------------------

    /// `lua_newthread`: create a coroutine thread, anchored on this thread's stack (pushed).
    /// Hooks are inherited and the extra space is copied from the main thread.
    pub fn new_thread(&mut self) -> LuaResult<Gc<Thread>> {
        self.check_gc();
        self.check_stack(1)?;
        let (hookmask, basehookcount, hook, chook, gp) = {
            let t = self.t();
            (t.hookmask, t.basehookcount, t.hook, t.chook, t.g)
        };
        let th = self.try_alloc(|h| h.new_thread_obj())?;
        // SAFETY: fresh thread; anchored right below; `g` outlives it.
        unsafe {
            (*th).g = gp;
            (*th).hookmask = hookmask;
            (*th).basehookcount = basehookcount;
            (*th).hook = hook;
            (*th).chook = chook;
            (*th).hookcount = basehookcount;
            let g = Gc::from_raw(th);
            self.push(Value::thread(g));
            // luai_userstatethread / extra space
            let main = (*gp).mainthread;
            std::ptr::copy_nonoverlapping(
                main.cast::<u8>().sub(LUA_EXTRASPACE),
                th.cast::<u8>().sub(LUA_EXTRASPACE),
                LUA_EXTRASPACE,
            );
            if let Err(e) = stack_init(th, &mut (*gp).heap.mem) {
                // thread stays anchored/garbage; undo the anchor push
                self.pop();
                return Err(e);
            }
            Ok(g)
        }
    }

    /// `luaE_resetthread`: reset a thread to its initial state, closing pending variables.
    pub fn reset_thread(&mut self, status: Status) -> Status {
        // SAFETY: ST1.
        unsafe { reset_ci(self.th_ptr()) };
        let status = if status == Status::Yield {
            Status::Ok
        } else {
            status
        };
        let status = self.close_protected(StkId(1), status);
        if status != Status::Ok {
            self.set_error_obj(status, StkId(1));
        } else {
            self.set_top(StkId(1));
        }
        let cit = self.ci().r().top.0;
        let _ = self.realloc_stack(cit, false);
        status
    }

    // ---- closing ------------------------------------------------------------------------------

    /// `close_state` for a fully built state: close pending variables, run every finalizer.
    /// Memory is released when the owning [`LuaState`] is dropped.
    pub fn close_state(&mut self) {
        if !self.g().complete || self.t().stack.cap() == 0 {
            return;
        }
        // lua_close always works on the main thread
        let main = self.g().mainthread;
        // SAFETY: the main thread outlives the state.
        let mut m = State {
            // SAFETY: non-null, see above.
            th: unsafe { NonNull::new_unchecked(main) },
        };
        // SAFETY: ST1.
        unsafe { reset_ci(main) };
        // J5: a recording armed by this state ends with `Abort::Close`
        #[cfg(feature = "jit_trace")]
        crate::jit_hooks::rec_close(&mut m);
        m.close_protected(StkId(1), Status::Ok);
        m.set_top(StkId(1));
        // luaC_freeallobjects: no extra finalizers after here; run every pending finalizer
        m.heap().gc_begin_close();
        m.call_all_pending_finalizers();
        debug_assert!(m.g().heap.gc.stp & GCSTP_CLOSING != 0);
    }

    // ---- loading ------------------------------------------------------------------------------

    /// `lua_load`: compile/undump `chunk` with the installed [`Loader`] and push the resulting
    /// closure (first upvalue = globals) on success, or the error message string on failure.
    ///
    /// No GC safepoint exists between the loader creating strings and the proto becoming reachable
    /// (the loader and `Proto::from_data` never call `check_gc`), so compile-time `StrRef`s need no
    /// anchoring; a debug assertion verifies that no collection ran during the load.
    pub fn load_chunk(&mut self, chunk: &[u8], chunk_name: &[u8], mode: Option<&[u8]>) -> Status {
        let before = self.g().heap.mem.total_bytes();
        let mut status = self.load_chunk_once(chunk, chunk_name, mode);
        if status != Status::Ok && self.is_memory_failure(status) {
            // like `luaM_malloc_`: an emergency collection and one more try
            if self.can_emergency_gc() {
                let t = self.top();
                self.set_top(t - 1); // drop the error message
                self.collect(true);
                status = self.load_chunk_once(chunk, chunk_name, mode);
            }
            if status != Status::Ok && self.is_memory_failure(status) {
                status = Status::ErrMem; // the parser reports memory errors as `LUA_ERRMEM`
            }
        }
        if status == Status::Ok {
            // The reference's parser has a safepoint (`luaC_checkGC`) per function it closes, so the
            // collector keeps pace with a big load; ours allocates in one go: take the steps that
            // the allocated bytes would have triggered (bounded).
            let alloc = self.g().heap.mem.total_bytes().saturating_sub(before);
            let steps = self.g().heap.steps_for_alloc(alloc).clamp(1, 256);
            for n in 0..steps {
                if n > 0 {
                    self.g_mut().heap.force_due();
                }
                self.check_gc();
            }
        }
        status
    }

    /// Did a failed load fail for lack of memory (status, or the memory-error message)?
    fn is_memory_failure(&self, status: Status) -> bool {
        status == Status::ErrMem
            || (status == Status::ErrSyntax
                && self
                    .get(self.top() - 1)
                    .as_string()
                    .is_some_and(|s| s.as_bytes() == MEMERRMSG))
    }

    fn load_chunk_once(&mut self, chunk: &[u8], chunk_name: &[u8], mode: Option<&[u8]>) -> Status {
        let loader = match self.g().loader {
            Some(l) => l,
            None => {
                return self.push_load_error(Status::ErrSyntax, b"no chunk loader installed");
            }
        };
        self.inc_nny(); // cannot yield during parsing
        let old_top = self.top();
        let ef = self.errfunc();
        let cycles = self.g().heap.gc_cycles();
        let status = self.pcall_with(
            |st| {
                let mode = mode.unwrap_or(b"bt");
                let r = loader(&mut st.g_mut().heap, chunk, chunk_name, mode);
                let pd = match r {
                    Ok(pd) => pd,
                    Err(msg) => {
                        let s = st.new_str_value(&msg)?;
                        st.push(s);
                        return Err(Status::ErrSyntax);
                    }
                };
                // No emergency-GC retries here: the compile-time `StrRef`s and the not yet
                // anchored proto must not be exposed to a collection (see the doc comment).
                st.check_stack(1)?;
                let p = crate::func::Proto::from_data(st.heap(), &pd)?;
                let cl = st.heap().new_lclosure(pd.upvalues.len())?;
                // SAFETY: fresh closure, proto live, upvalue counts match.
                unsafe { cl.set_proto(p) };
                st.push(Value::lclosure(cl)); // anchor
                st.heap().init_upvals(cl)?;
                Ok(())
            },
            old_top,
            ef,
        );
        debug_assert_eq!(
            cycles,
            self.g().heap.gc_cycles(),
            "a collection ran while loading a chunk"
        );
        self.dec_nny();
        if status == Status::Ok {
            // lua_load: set the first upvalue to the globals table
            let cl = self
                .get(self.top() - 1)
                .as_lclosure()
                .expect("load pushes a closure");
            if cl.nupvalues() >= 1 {
                let gt = Value::table(self.globals());
                let uv = cl.upval(0);
                uv.set(gt);
                self.heap().barrier(uv.as_ptr().cast(), &gt);
            }
        }
        status
    }

    fn push_load_error(&mut self, status: Status, msg: &[u8]) -> Status {
        match self.new_str_value(msg) {
            Ok(s) => {
                let _ = self.push_checked(s);
                status
            }
            Err(e) => e,
        }
    }
}

impl CompilerHost for State {
    type Str = StrRef;
    fn intern(&mut self, bytes: &[u8]) -> LuaResult<StrRef> {
        self.heap().new_str(bytes)
    }
    fn bytes(&self, s: StrRef) -> &[u8] {
        s.get().as_bytes()
    }
    fn str_id(s: StrRef) -> usize {
        s.as_ptr().addr()
    }
}
