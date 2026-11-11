//! Plumbing shared by the whole C API: `lua_State*` <-> `State`, error throwing/catching, the
//! trampolines that call C functions/continuations/hooks, and index translation.
use crate::types::*;
use rn_core::state::{State, Thread};
use rn_core::status::{LuaResult, Status};
use rn_core::value::{Gc, Value};
use std::cell::Cell;
use std::ffi::{c_char, c_int, CStr};
use std::panic::{catch_unwind, resume_unwind, AssertUnwindSafe};

/// Defines an exported `lua_*`/`luaL_*` function whose first parameter is the `lua_State*`,
/// bound to a `State` handle named by the caller.
macro_rules! lua_api {
    ($(#[$m:meta])* $name:ident ($st:ident $(, $a:ident : $t:ty)* $(,)?) $(-> $r:ty)? $body:block) => {
        $(#[$m])*
        #[no_mangle]
        pub unsafe extern "C-unwind" fn $name(l_: *mut $crate::lua_State $(, $a : $t)*) $(-> $r)? {
            #[allow(unused_mut, unused_unsafe)]
            unsafe {
                let mut $st = $crate::glue::state(l_);
                $body
            }
        }
    };
}

/// The `State` handle of a `lua_State*`.
///
/// # Safety
/// `l` must be a live thread of this library.
#[inline]
pub unsafe fn state(l: *mut lua_State) -> State {
    // SAFETY: caller contract; `State` is a plain handle (several may exist for one thread).
    unsafe { State::from_gc(Gc::from_raw(l.cast::<Thread>())) }
}

/// The `lua_State*` of a handle.
#[inline]
pub fn lptr(st: &State) -> *mut lua_State {
    st.as_gc().as_ptr().cast()
}

/// Payload of the unwinding used for Lua errors/yields raised from the C API.
pub struct Throw(pub Status);

thread_local! {
    /// Number of active catchers (C-function/continuation/hook trampolines) on this OS thread.
    static DEPTH: Cell<u32> = const { Cell::new(0) };
}

/// Run `f`, converting a [`Throw`] unwinding out of it into `Err(status)`.
pub fn catch<R>(f: impl FnOnce() -> R) -> Result<R, Status> {
    DEPTH.with(|d| d.set(d.get() + 1));
    let r = catch_unwind(AssertUnwindSafe(f));
    DEPTH.with(|d| d.set(d.get() - 1));
    match r {
        Ok(v) => Ok(v),
        Err(p) => match p.downcast::<Throw>() {
            Ok(t) => Err(t.0),
            Err(p) => resume_unwind(p), // a real Rust panic: keep unwinding
        },
    }
}

/// Raise an error (or yield) from the C API: the error object is already on the stack.
pub fn throw(st: &mut State, status: Status) -> ! {
    if DEPTH.with(|d| d.get()) == 0 {
        unprotected(st, status)
    }
    resume_unwind(Box::new(Throw(status)))
}

/// No catcher: call the panic function (`lua_atpanic`) and abort, like `luaD_throw`.
fn unprotected(st: &mut State, _status: Status) -> ! {
    let f = st.g().capi.panic;
    if !f.is_null() {
        // SAFETY: stored by `lua_atpanic` from a `lua_CFunction`.
        let f: unsafe extern "C-unwind" fn(*mut lua_State) -> c_int =
            unsafe { std::mem::transmute(f) };
        // SAFETY: calling the user's panic function with the state.
        unsafe { f(lptr(st)) };
    }
    std::process::abort()
}

/// Unwrap an internal result, raising the error as a C API error.
#[inline]
pub fn ck<T>(st: &mut State, r: LuaResult<T>) -> T {
    match r {
        Ok(v) => v,
        Err(s) => throw(st, s),
    }
}

/// Raise the (non-`Result`) status returned by internal error constructors (`run_error`, ...).
#[inline]
pub fn raise(st: &mut State, status: Status) -> ! {
    throw(st, status)
}

// ---------------------------------------------------------------------------------------------
// trampolines
// ---------------------------------------------------------------------------------------------

type CFn = unsafe extern "C-unwind" fn(*mut lua_State) -> c_int;

/// Runs light C functions and C closures: the C pointer is read from the frame's function slot.
fn c_call_trampoline(st: &mut State) -> LuaResult<usize> {
    let fv = st.get(st.ci().r().func);
    let p = match fv.as_light_cfn() {
        Some(p) => p,
        None => fv.as_cclosure().map_or(std::ptr::null(), |c| c.cfn()),
    };
    debug_assert!(!p.is_null());
    // SAFETY: the pointer was stored from a `lua_CFunction` by `lua_pushcclosure`.
    let f: CFn = unsafe { std::mem::transmute(p) };
    let l = lptr(st);
    // SAFETY: calling a C function with its state; errors come back as `Throw`.
    let n = catch(|| unsafe { f(l) })?;
    Ok(n.max(0) as usize)
}

/// Runs the C continuation (`lua_KFunction`) stored in the frame (`CallInfo::ck`).
fn c_k_trampoline(st: &mut State, status: Status, ctx: isize) -> LuaResult<usize> {
    let p = st.ci().r().ck;
    debug_assert!(!p.is_null());
    // SAFETY: stored by `set_k` from a `lua_KFunction`.
    let k: unsafe extern "C-unwind" fn(*mut lua_State, c_int, lua_KContext) -> c_int =
        unsafe { std::mem::transmute(p) };
    let l = lptr(st);
    // SAFETY: calling the user's continuation.
    let n = catch(|| unsafe { k(l, status as c_int, ctx) })?;
    Ok(n.max(0) as usize)
}

/// Runs the C hook (`lua_Hook`) of the thread.
fn c_hook_trampoline(st: &mut State, event: i32, line: i32) -> LuaResult<()> {
    let p = st.c_hook();
    if p.is_null() {
        return Ok(());
    }
    // SAFETY: stored by `lua_sethook` from a `lua_Hook`.
    let h: unsafe extern "C-unwind" fn(*mut lua_State, *mut lua_Debug) =
        unsafe { std::mem::transmute(p) };
    // SAFETY: an all-zero `lua_Debug` is valid (null pointers, zero numbers).
    let mut ar: lua_Debug = unsafe { std::mem::zeroed() };
    ar.event = event;
    ar.currentline = line;
    ar.i_ci = st.ci().as_ptr().cast();
    let l = lptr(st);
    // SAFETY: calling the user's hook with a valid `lua_Debug`.
    catch(|| unsafe { h(l, &mut ar) })?;
    Ok(())
}

/// Make the core call C functions through `c_call_trampoline` (idempotent).
pub fn ensure_init() {
    static INIT: std::sync::Once = std::sync::Once::new();
    INIT.call_once(|| rn_core::call::set_c_trampoline(c_call_trampoline));
}

/// The internal continuation to register for a C `lua_KFunction` (None if null), storing the C
/// pointer in the frame.
pub fn set_k(st: &mut State, k: lua_KFunction) -> Option<rn_core::state::KFn> {
    match k {
        Some(f) => {
            st.ci().w().ck = f as *const ();
            Some(c_k_trampoline)
        }
        None => None,
    }
}

/// The internal hook function that calls C hooks.
pub fn c_hook_fn() -> rn_core::state::HookFn {
    c_hook_trampoline
}

/// The native function used as the `f` of C closures.
pub fn c_closure_fn() -> rn_core::state::NativeFn {
    c_call_trampoline
}

// ---------------------------------------------------------------------------------------------
// indices
// ---------------------------------------------------------------------------------------------

/// `index2value`: the value at an acceptable index, `None` for a non-valid ("none") index.
pub fn index2value(st: &State, idx: c_int) -> Option<Value> {
    let func = st.ci().r().func;
    if idx > 0 {
        let slot = func + idx as usize;
        if slot >= st.top() {
            None
        } else {
            Some(st.get(slot))
        }
    } else if idx > LUA_REGISTRYINDEX {
        Some(st.get(st.top() - (-idx) as usize)) // negative index
    } else if idx == LUA_REGISTRYINDEX {
        Some(st.g().registry)
    } else {
        // upvalue of the running C closure
        let i = (LUA_REGISTRYINDEX - idx) as usize;
        let fv = st.get(func);
        match fv.as_cclosure() {
            Some(cl) if i <= cl.nupvalues() => Some(cl.upvalue(i - 1)),
            _ => None,
        }
    }
}

/// The value at `idx`, `nil` for a "none" index (callers that must tell them apart use
/// [`index2value`]).
#[inline]
pub fn val(st: &State, idx: c_int) -> Value {
    index2value(st, idx).unwrap_or(Value::NIL)
}

/// Stack slot of a non-pseudo index.
pub fn stack_slot(st: &State, idx: c_int) -> rn_core::state::StkId {
    let func = st.ci().r().func;
    if idx > 0 {
        func + idx as usize
    } else {
        st.top() - (-idx) as usize
    }
}

/// Assign to an acceptable index (stack slot or C-closure upvalue).
pub fn set_idx(st: &mut State, idx: c_int, v: Value) {
    if idx > LUA_REGISTRYINDEX {
        let s = stack_slot(st, idx);
        st.set(s, v);
    } else if idx < LUA_REGISTRYINDEX {
        let i = (LUA_REGISTRYINDEX - idx) as usize;
        let func = st.ci().r().func;
        if let Some(cl) = st.get(func).as_cclosure() {
            if i <= cl.nupvalues() {
                cl.set_upvalue(st.heap(), i - 1, v);
            }
        }
    }
    // the registry itself cannot be replaced
}

/// Bytes of a NUL-terminated C string.
///
/// # Safety
/// `p` must point to a valid NUL-terminated string.
#[inline]
pub unsafe fn cstr<'a>(p: *const c_char) -> &'a [u8] {
    // SAFETY: caller contract.
    unsafe { CStr::from_ptr(p) }.to_bytes()
}

/// Bytes of a C string or `None` for a null pointer.
///
/// # Safety
/// `p` is null or a valid NUL-terminated string.
#[inline]
pub unsafe fn cstr_opt<'a>(p: *const c_char) -> Option<&'a [u8]> {
    if p.is_null() {
        None
    } else {
        // SAFETY: caller contract.
        Some(unsafe { cstr(p) })
    }
}
