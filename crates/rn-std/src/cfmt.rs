//! Thin safe wrapper over libc `snprintf` for the numeric/string conversions of `string.format`.
//!
//! The reference formats every number with the C library, so using the very same function is the
//! only way to be bit-exact (`%a`, `%g`, `%e`, rounding of `%.99f`, ...). Soundness of the safe API:
//! every wrapper first validates that `form` is *exactly one* conversion specification of the kind
//! matching the argument type (see [`valid_form`]), so a variadic argument can never be misread and
//! `%n`/`*` can never occur.
#![allow(unsafe_code)]

use std::ffi::c_char;

/// Check that `form` is `%` flags width [`.` precision] `lenmod` conv with `conv` in `convs`.
pub fn valid_form(form: &[u8], convs: &[u8], lenmod: &[u8]) -> bool {
    let mut i = 0;
    if form.first() != Some(&b'%') {
        return false;
    }
    i += 1;
    while i < form.len() && b"-+#0 ".contains(&form[i]) {
        i += 1;
    }
    let mut digits = 0;
    while i < form.len() && form[i].is_ascii_digit() {
        i += 1;
        digits += 1;
    }
    if digits > 3 {
        return false;
    }
    if i < form.len() && form[i] == b'.' {
        i += 1;
        digits = 0;
        while i < form.len() && form[i].is_ascii_digit() {
            i += 1;
            digits += 1;
        }
        if digits > 3 {
            return false;
        }
    }
    if !form[i..].starts_with(lenmod) {
        return false;
    }
    i += lenmod.len();
    i + 1 == form.len() && convs.contains(&form[i])
}

/// Longest conversion specification `string.format` builds (`MAX_FORMAT` plus the `ll` modifier).
const MAX_FORM: usize = 40;

/// `form` as a NUL-terminated C string on the stack (`None` if it is too long).
fn cform(form: &[u8]) -> Option<[u8; MAX_FORM]> {
    if form.len() >= MAX_FORM {
        return None;
    }
    let mut f = [0u8; MAX_FORM];
    f[..form.len()].copy_from_slice(form);
    Some(f)
}

/// Run `f(buf, cap)` (an `snprintf` call) writing straight into the spare capacity of `out`
/// (growing it and running again if needed); appends the produced bytes.
fn run_into(out: &mut Vec<u8>, f: impl Fn(*mut c_char, usize) -> i32) {
    out.reserve(64);
    let len = out.len();
    let cap = out.capacity() - len;
    // SAFETY: `len <= capacity`: the pointer is inside (or one past) the allocation.
    let n = f(unsafe { out.as_mut_ptr().add(len) } as *mut c_char, cap);
    if n < 0 {
        return;
    }
    let n = n as usize;
    if n >= cap {
        out.reserve(n + 1);
        let cap = out.capacity() - len;
        // SAFETY: as above.
        if f(unsafe { out.as_mut_ptr().add(len) } as *mut c_char, cap) < 0 {
            return;
        }
    }
    // SAFETY: `snprintf` initialised `n` bytes (plus a NUL) at `len..`, within the capacity.
    unsafe { out.set_len(len + n) };
}

/// `snprintf(form, (long long)v)` appended to `out`; `form` must use the `ll` length modifier with
/// one of `diouxX`. `false` if `form` is not such a specification.
pub fn fmt_i64(out: &mut Vec<u8>, form: &[u8], v: i64) -> bool {
    if !valid_form(form, b"diouxX", b"ll") {
        return false;
    }
    let Some(f) = cform(form) else { return false };
    // SAFETY: `f` is a NUL-terminated, validated single `%..lld`-style conversion that consumes
    // exactly one `long long` argument, which is what is passed; `buf`/`cap` describe live memory.
    run_into(out, |buf, cap| unsafe {
        libc::snprintf(buf, cap, f.as_ptr() as *const c_char, v as libc::c_longlong)
    });
    true
}

/// `snprintf(form, (int)v)` for `%c`, appended to `out`.
pub fn fmt_char(out: &mut Vec<u8>, form: &[u8], v: i32) -> bool {
    if !valid_form(form, b"c", b"") {
        return false;
    }
    let Some(f) = cform(form) else { return false };
    // SAFETY: validated single `%c` conversion consuming one `int`; buffer is live.
    run_into(out, |buf, cap| unsafe {
        libc::snprintf(buf, cap, f.as_ptr() as *const c_char, v as libc::c_int)
    });
    true
}

/// `snprintf(form, (double)v)` appended to `out`; `form` is a single `aAeEfFgG` conversion without
/// length modifier.
pub fn fmt_f64(out: &mut Vec<u8>, form: &[u8], v: f64) -> bool {
    if !valid_form(form, b"aAeEfFgG", b"") {
        return false;
    }
    let Some(f) = cform(form) else { return false };
    // SAFETY: validated single floating conversion consuming one `double`; buffer is live.
    run_into(out, |buf, cap| unsafe {
        libc::snprintf(buf, cap, f.as_ptr() as *const c_char, v as libc::c_double)
    });
    true
}

/// `snprintf(form, s)` for `%s` (flags/width/precision allowed), appended to `out`; `s` must not
/// contain NUL.
pub fn fmt_str(out: &mut Vec<u8>, form: &[u8], s: &[u8]) -> bool {
    if !valid_form(form, b"s", b"") || s.contains(&0) {
        return false;
    }
    let Some(f) = cform(form) else { return false };
    let mut cs = s.to_vec();
    cs.push(0);
    // SAFETY: validated single `%s` conversion consuming one `const char *`; `cs` is NUL-terminated
    // and outlives the call; buffer is live.
    run_into(out, |buf, cap| unsafe {
        libc::snprintf(
            buf,
            cap,
            f.as_ptr() as *const c_char,
            cs.as_ptr() as *const c_char,
        )
    });
    true
}

/// `snprintf(form, (void *)p)` for `%p`, appended to `out`; the pointer is only printed, never
/// dereferenced.
pub fn fmt_ptr(out: &mut Vec<u8>, form: &[u8], p: usize) -> bool {
    if !valid_form(form, b"p", b"") {
        return false;
    }
    let Some(f) = cform(form) else { return false };
    // SAFETY: validated single `%p` conversion consuming one `void *`; the value is only formatted.
    run_into(out, |buf, cap| unsafe {
        libc::snprintf(
            buf,
            cap,
            f.as_ptr() as *const c_char,
            p as *const libc::c_void,
        )
    });
    true
}
