//! Spike: Lua-style non-local exit through C frames via C-unwind + Rust panic.
use std::os::raw::{c_char, c_int, c_void};
use std::panic::{catch_unwind, resume_unwind, AssertUnwindSafe};
use std::ffi::CStr;

struct LuaUnwind; // private marker payload

pub type CFunc = unsafe extern "C-unwind" fn(*mut c_void) -> c_int;

static mut LAST_STR: [u8; 256] = [0; 256];

/// like lua_error: never returns, unwinds through C frames
#[no_mangle]
pub extern "C-unwind" fn spike_error(_l: *mut c_void) -> ! {
    resume_unwind(Box::new(LuaUnwind)) // skips panic hook
}

/// like lua_pcall around a C function: 0 = ok, 1 = Lua error caught, 2 = foreign panic
#[no_mangle]
pub extern "C-unwind" fn spike_pcall(l: *mut c_void, f: CFunc) -> c_int {
    match catch_unwind(AssertUnwindSafe(|| unsafe { f(l) })) {
        Ok(_) => 0,
        Err(p) if p.is::<LuaUnwind>() => 1,
        Err(_) => 2,
    }
}

/// target of the vararg C shim: receives the formatted string
#[no_mangle]
pub extern "C-unwind" fn spike_push_str(_l: *mut c_void, s: *const c_char) -> *const c_char {
    let st = unsafe { CStr::from_ptr(s) }.to_bytes();
    let n = st.len().min(255);
    unsafe {
        let buf = &mut *std::ptr::addr_of_mut!(LAST_STR);
        buf[..n].copy_from_slice(&st[..n]);
        buf[n] = 0;
        buf.as_ptr() as *const c_char
    }
}

/// Rust-side "luaL_error" core: formats nothing, just raises (called from the C vararg shim)
#[no_mangle]
pub extern "C-unwind" fn spike_raise_with_msg(l: *mut c_void, s: *const c_char) -> ! {
    spike_push_str(l, s);
    spike_error(l)
}
