//! Backend of `lua_pushvfstring`/`lua_pushfstring`/`luaL_error` (the C shim extracts the
//! arguments with `va_arg` and calls these functions one conversion at a time).
use crate::glue::*;
use crate::types::*;
use rn_core::number;
use rn_core::value::Value;
use std::ffi::{c_char, c_int, c_void};

/// A heap buffer being filled; handed to C as an opaque pointer.
type FmtBuf = Vec<u8>;

#[no_mangle]
pub extern "C" fn rn_fmt_new() -> *mut c_void {
    Box::into_raw(Box::new(FmtBuf::new())).cast()
}

unsafe fn buf<'a>(b: *mut c_void) -> &'a mut FmtBuf {
    // SAFETY: created by `rn_fmt_new`, used by one shim invocation.
    unsafe { &mut *b.cast::<FmtBuf>() }
}

#[no_mangle]
pub unsafe extern "C" fn rn_fmt_bytes(b: *mut c_void, s: *const c_char, n: usize) {
    if n > 0 {
        // SAFETY: `s` points to `n` readable bytes (pieces of the format string).
        unsafe { buf(b).extend_from_slice(std::slice::from_raw_parts(s.cast(), n)) }
    }
}

#[no_mangle]
pub unsafe extern "C" fn rn_fmt_cstr(b: *mut c_void, s: *const c_char) {
    // SAFETY: `%s` argument: a C string or NULL.
    unsafe {
        if s.is_null() {
            buf(b).extend_from_slice(b"(null)");
        } else {
            buf(b).extend_from_slice(cstr(s));
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn rn_fmt_char(b: *mut c_void, c: c_int) {
    // SAFETY: valid buffer.
    unsafe { buf(b).push(c as u8) }
}

#[no_mangle]
pub unsafe extern "C" fn rn_fmt_int(b: *mut c_void, i: lua_Integer) {
    // SAFETY: valid buffer.
    unsafe { buf(b).extend_from_slice(number::tostring_int(i).as_bytes()) }
}

#[no_mangle]
pub unsafe extern "C" fn rn_fmt_num(b: *mut c_void, n: lua_Number) {
    // SAFETY: valid buffer.
    unsafe { buf(b).extend_from_slice(number::tostring_float(n).as_bytes()) }
}

#[no_mangle]
pub unsafe extern "C" fn rn_fmt_ptr(b: *mut c_void, p: *mut c_void) {
    let mut tmp = [0u8; 64];
    // SAFETY: `snprintf` into a local buffer with its size.
    let n = unsafe { libc::snprintf(tmp.as_mut_ptr().cast(), tmp.len(), c"%p".as_ptr(), p) };
    let n = (n.max(0) as usize).min(tmp.len() - 1);
    // SAFETY: valid buffer.
    unsafe { buf(b).extend_from_slice(&tmp[..n]) }
}

/// `luaO_utf8esc`: UTF-8 encoding of a code point up to `0x7FFFFFFF`.
fn utf8esc(mut x: u32, out: &mut Vec<u8>) {
    if x < 0x80 {
        out.push(x as u8);
        return;
    }
    let mut tmp = [0u8; 8];
    let mut n = 1usize;
    let mut mfb: u32 = 0x3f; // maximum that fits in the first byte
    loop {
        tmp[8 - n] = 0x80 | (x & 0x3f) as u8;
        n += 1;
        x >>= 6;
        mfb >>= 1;
        if x <= mfb {
            break;
        }
    }
    tmp[8 - n] = ((!mfb << 1) | x) as u8;
    out.extend_from_slice(&tmp[8 - n..]);
}

#[no_mangle]
pub unsafe extern "C" fn rn_fmt_utf8(b: *mut c_void, x: std::ffi::c_ulong) {
    // SAFETY: valid buffer.
    unsafe { utf8esc(x as u32, buf(b)) }
}

/// 5.4 `luaO_pushvfstring`: an unknown conversion is an error
/// ("invalid option '%c' to 'lua_pushfstring'").
#[cfg(rustynail_lua54)]
#[no_mangle]
pub unsafe extern "C-unwind" fn rn_fmt_invalid(l: *mut lua_State, b: *mut c_void, c: c_int) {
    // SAFETY: the buffer is released here (the error does not return); `l` is a valid state.
    unsafe {
        drop(Box::from_raw(b.cast::<FmtBuf>()));
        let mut st = state(l);
        let mut m = b"invalid option '%".to_vec();
        m.push(c as u8);
        m.extend_from_slice(b"' to 'lua_pushfstring'");
        let s = st.run_error(&m);
        raise(&mut st, s)
    }
}

/// Push the finished string and return a pointer to its bytes.
#[no_mangle]
pub unsafe extern "C-unwind" fn rn_fmt_push(l: *mut lua_State, b: *mut c_void) -> *const c_char {
    // SAFETY: the buffer is released exactly once; `l` is a valid state.
    unsafe {
        let bytes = Box::from_raw(b.cast::<FmtBuf>());
        let mut st = state(l);
        let r = st.new_str_value(&bytes);
        drop(bytes);
        let v: Value = ck(&mut st, r);
        st.push(v);
        let p = v.as_string().expect("string").as_bytes().as_ptr().cast();
        st.check_gc();
        p
    }
}
