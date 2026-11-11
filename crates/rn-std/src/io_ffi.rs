//! libc FFI for the `io` and `os` libraries: a thin wrapper over `FILE*` (the reference uses C stdio,
//! so buffering and ordering between `io.write`, `print` and `warn` match), plus the few process and
//! file-system calls of `loslib.c`.
//!
//! Soundness contract: a [`CFile`] is created only by this module (`fopen`, `popen`, `tmpfile`, the
//! three standard streams) and the caller (`lib_io`) never uses it after `close`/`pclose` (the
//! `LStream.closef` protocol marks the stream closed first). Under that protocol every wrapper
//! below passes a live `FILE*` to libc.
#![allow(unsafe_code)]

use std::ffi::{c_char, c_int, CString};

pub use libc::FILE;

extern "C" {
    static stdin: *mut FILE;
    static stdout: *mut FILE;
    static stderr: *mut FILE;
    fn flockfile(f: *mut FILE);
    fn funlockfile(f: *mut FILE);
    fn getc_unlocked(f: *mut FILE) -> c_int;
    fn clock() -> libc::clock_t;
}

/// A C `FILE*`. See the module docs for the validity protocol.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CFile(pub *mut FILE);

/// `luaL_Stream`-compatible layout: `{ FILE *f; lua_CFunction closef; }`.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct LStream {
    pub f: *mut FILE,
    pub closef: Option<rn_core::state::NativeFn>,
}

/// Read the stream record stored in a userdata of at least `size_of::<LStream>()` bytes.
pub fn read_stream(u: rn_core::value::Gc<rn_core::udata::Udata>) -> LStream {
    debug_assert!(u.len() >= std::mem::size_of::<LStream>());
    // SAFETY: file userdata are created with `size_of::<LStream>()` payload bytes (16-aligned,
    // zero-initialised) and every write goes through `write_stream`.
    unsafe { u.data_ptr().cast::<LStream>().read() }
}

/// Store the stream record.
pub fn write_stream(u: rn_core::value::Gc<rn_core::udata::Udata>, s: LStream) {
    debug_assert!(u.len() >= std::mem::size_of::<LStream>());
    // SAFETY: as in `read_stream`; the payload is exclusively ours while a native runs.
    unsafe { u.data_ptr().cast::<LStream>().write(s) }
}

pub fn std_in() -> CFile {
    // SAFETY: reading libc's `stdin` pointer variable.
    CFile(unsafe { stdin })
}
pub fn std_out() -> CFile {
    // SAFETY: reading libc's `stdout` pointer variable.
    CFile(unsafe { stdout })
}
pub fn std_err() -> CFile {
    // SAFETY: reading libc's `stderr` pointer variable.
    CFile(unsafe { stderr })
}

/// Write bytes to C `stdout` (same buffer as `io.write`).
pub fn stdout_write(b: &[u8]) {
    std_out().write(b);
}
/// Write bytes to C `stderr`.
pub fn stderr_write(b: &[u8]) {
    std_err().write(b);
}
/// `fflush(stdout)`.
pub fn stdout_flush() {
    std_out().flush();
}

/// Current `errno`.
pub fn errno() -> i32 {
    std::io::Error::last_os_error().raw_os_error().unwrap_or(0)
}
/// Set `errno`.
pub fn set_errno(v: i32) {
    // SAFETY: `__errno_location` returns the valid thread-local errno slot.
    unsafe { *libc::__errno_location() = v }
}

fn cstring(b: &[u8]) -> Option<CString> {
    CString::new(b).ok()
}

impl CFile {
    pub fn open(name: &[u8], mode: &[u8]) -> Option<CFile> {
        let (n, m) = (cstring(name), cstring(mode));
        let (Some(n), Some(m)) = (n, m) else {
            set_errno(libc::ENOENT);
            return None;
        };
        // SAFETY: both strings are NUL-terminated and live.
        let f = unsafe { libc::fopen(n.as_ptr(), m.as_ptr()) };
        (!f.is_null()).then_some(CFile(f))
    }
    pub fn popen(cmd: &[u8], mode: &[u8]) -> Option<CFile> {
        let (Some(c), Some(m)) = (cstring(cmd), cstring(mode)) else {
            set_errno(libc::EINVAL);
            return None;
        };
        // SAFETY: NUL-terminated live strings; fflush(NULL) as `l_popen`.
        let f = unsafe {
            libc::fflush(std::ptr::null_mut());
            libc::popen(c.as_ptr(), m.as_ptr())
        };
        (!f.is_null()).then_some(CFile(f))
    }
    pub fn tmpfile() -> Option<CFile> {
        // SAFETY: no arguments.
        let f = unsafe { libc::tmpfile() };
        (!f.is_null()).then_some(CFile(f))
    }
    /// `fclose`; true on success.
    pub fn close(self) -> bool {
        // SAFETY: protocol (module docs): live file, never used afterwards.
        unsafe { libc::fclose(self.0) == 0 }
    }
    /// `pclose`: raw wait status (-1 on error).
    pub fn pclose(self) -> i32 {
        // SAFETY: protocol.
        unsafe { libc::pclose(self.0) }
    }
    pub fn write(self, b: &[u8]) -> usize {
        // SAFETY: protocol; `b` is a live slice.
        unsafe { libc::fwrite(b.as_ptr().cast(), 1, b.len(), self.0) }
    }
    /// `fread` into `buf`; returns bytes read.
    pub fn read(self, buf: &mut [u8]) -> usize {
        // SAFETY: protocol; `buf` is writable for its length.
        unsafe { libc::fread(buf.as_mut_ptr().cast(), 1, buf.len(), self.0) }
    }
    pub fn flush(self) -> bool {
        // SAFETY: protocol.
        unsafe { libc::fflush(self.0) == 0 }
    }
    pub fn getc(self) -> c_int {
        // SAFETY: protocol.
        unsafe { libc::fgetc(self.0) }
    }
    pub fn ungetc(self, c: c_int) {
        // SAFETY: protocol.
        unsafe {
            libc::ungetc(c, self.0);
        }
    }
    pub fn lock(self) {
        // SAFETY: protocol.
        unsafe { flockfile(self.0) }
    }
    pub fn unlock(self) {
        // SAFETY: protocol; paired with `lock`.
        unsafe { funlockfile(self.0) }
    }
    /// `getc_unlocked`: only between `lock`/`unlock`.
    pub fn getc_unlocked(self) -> c_int {
        // SAFETY: protocol.
        unsafe { getc_unlocked(self.0) }
    }
    pub fn clearerr(self) {
        // SAFETY: protocol.
        unsafe { libc::clearerr(self.0) }
    }
    pub fn error(self) -> bool {
        // SAFETY: protocol.
        unsafe { libc::ferror(self.0) != 0 }
    }
    pub fn eof(self) -> bool {
        // SAFETY: protocol.
        unsafe { libc::feof(self.0) != 0 }
    }
    /// `fseeko`: true on success.
    pub fn seek(self, off: i64, whence: i32) -> bool {
        // SAFETY: protocol.
        unsafe { libc::fseeko(self.0, off as libc::off_t, whence) == 0 }
    }
    pub fn tell(self) -> i64 {
        // SAFETY: protocol.
        unsafe { libc::ftello(self.0) as i64 }
    }
    /// `setvbuf(f, NULL, mode, size)`; true on success.
    pub fn setvbuf(self, mode: i32, size: usize) -> bool {
        // SAFETY: protocol; a NULL buffer lets libc allocate.
        unsafe { libc::setvbuf(self.0, std::ptr::null_mut(), mode, size) == 0 }
    }
}

/// `fflush(NULL)`.
pub fn flush_all() {
    // SAFETY: NULL flushes all output streams.
    unsafe {
        libc::fflush(std::ptr::null_mut());
    }
}

// ---- os ---------------------------------------------------------------------------------------

/// `remove(path)`; true on success.
pub fn remove(path: &[u8]) -> bool {
    let Some(p) = cstring(path) else {
        set_errno(libc::ENOENT);
        return false;
    };
    // SAFETY: NUL-terminated live string.
    unsafe { libc::remove(p.as_ptr()) == 0 }
}

/// `rename(a, b)`.
pub fn rename(a: &[u8], b: &[u8]) -> bool {
    let (Some(x), Some(y)) = (cstring(a), cstring(b)) else {
        set_errno(libc::ENOENT);
        return false;
    };
    // SAFETY: NUL-terminated live strings.
    unsafe { libc::rename(x.as_ptr(), y.as_ptr()) == 0 }
}

/// `system(cmd)` (`None` asks whether a shell exists).
pub fn system(cmd: Option<&[u8]>) -> Option<i32> {
    match cmd {
        None => {
            // SAFETY: system(NULL) only queries.
            Some(unsafe { libc::system(std::ptr::null()) })
        }
        Some(c) => {
            let c = cstring(c)?;
            flush_all();
            // SAFETY: NUL-terminated live string.
            Some(unsafe { libc::system(c.as_ptr()) })
        }
    }
}

/// `clock()/CLOCKS_PER_SEC`.
pub fn clock_secs() -> f64 {
    // SAFETY: no arguments.
    let c = unsafe { clock() };
    c as f64 / 1_000_000.0
}

/// `lua_tmpnam` (`mkstemp` on `/tmp/lua_XXXXXX`).
pub fn tmpnam() -> Option<Vec<u8>> {
    let mut buf = b"/tmp/lua_XXXXXX\0".to_vec();
    // SAFETY: `buf` is a writable NUL-terminated template.
    let fd = unsafe { libc::mkstemp(buf.as_mut_ptr().cast::<c_char>()) };
    if fd == -1 {
        return None;
    }
    // SAFETY: `fd` was just returned by mkstemp.
    unsafe {
        libc::close(fd);
    }
    buf.pop();
    Some(buf)
}

/// `setlocale(category, locale)`; the resulting locale name.
pub fn setlocale(cat: i32, loc: Option<&[u8]>) -> Option<Vec<u8>> {
    let c = match loc {
        Some(l) => Some(cstring(l)?),
        None => None,
    };
    // SAFETY: NULL or a NUL-terminated live string; the returned pointer is copied immediately.
    unsafe {
        let r = libc::setlocale(cat, c.as_ref().map_or(std::ptr::null(), |c| c.as_ptr()));
        if r.is_null() {
            None
        } else {
            Some(std::ffi::CStr::from_ptr(r).to_bytes().to_vec())
        }
    }
}

/// `exit(status)` after flushing C stdio.
pub fn exit(status: i32) -> ! {
    // SAFETY: `exit` runs atexit handlers and flushes stdio; never returns.
    unsafe { libc::exit(status) }
}
