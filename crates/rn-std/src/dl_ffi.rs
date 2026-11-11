//! `dlopen` wrappers for `package.loadlib` and the C searchers (`lsys_load`/`lsys_sym` of
//! `loadlib.c`). Handles are opaque pointers owned by the registry table `_CLIBS`, which closes
//! them in its `__gc`.
//!
//! Soundness contract (as for `io_ffi::CFile`): a [`DlHandle`] is created only by [`open`], kept
//! alive in `_CLIBS`, and closed exactly once, by that table's `__gc`; `lib_package` never
//! fabricates handles.
#![allow(unsafe_code)]

use std::ffi::{c_char, c_void, CStr, CString};

/// An open shared library (`dlopen` handle).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DlHandle(pub *mut c_void);

fn last_error() -> Vec<u8> {
    // SAFETY: `dlerror` returns null or a NUL-terminated message valid until the next dl* call.
    let p = unsafe { libc::dlerror() };
    if p.is_null() {
        b"unknown dynamic loading error".to_vec()
    } else {
        // SAFETY: non-null and NUL-terminated (see above).
        unsafe { CStr::from_ptr(p as *const c_char) }
            .to_bytes()
            .to_vec()
    }
}

/// `lsys_load`: open a shared library (`global`: its symbols become visible to later loads).
pub fn open(path: &[u8], global: bool) -> Result<DlHandle, Vec<u8>> {
    let Ok(c) = CString::new(path) else {
        return Err(b"path contains an embedded zero".to_vec());
    };
    let flags = libc::RTLD_NOW
        | if global {
            libc::RTLD_GLOBAL
        } else {
            libc::RTLD_LOCAL
        };
    // SAFETY: `c` is a valid C string.
    let lib = unsafe { libc::dlopen(c.as_ptr(), flags) };
    if lib.is_null() {
        Err(last_error())
    } else {
        Ok(DlHandle(lib))
    }
}

/// `lsys_sym`: look a function up in an open library.
pub fn sym(lib: DlHandle, name: &[u8]) -> Result<*const (), Vec<u8>> {
    let Ok(c) = CString::new(name) else {
        return Err(b"symbol name contains an embedded zero".to_vec());
    };
    // SAFETY: `lib` is live (module contract); `c` is a valid C string.
    let f = unsafe { libc::dlsym(lib.0, c.as_ptr()) };
    if f.is_null() {
        Err(last_error())
    } else {
        Ok(f as *const ())
    }
}

/// `lsys_unloadlib`: the handle must not be used afterwards.
pub fn close(lib: DlHandle) {
    // SAFETY: `lib` is live and closed once (module contract).
    unsafe { libc::dlclose(lib.0) };
}
