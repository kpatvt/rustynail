//! Storage behind the `string.buffer` userdata: the one unsafe corner of the buffer library.
//!
//! The userdata payload is a single pointer to a leaked `Box<BufInner>` (so the byte vector can grow with
//! `Vec`'s allocator and is charged to the state's memory limit through `Mem::charge_ext`). The metatable's
//! `__gc` drops the box. Soundness contract: [`BufInner`] is created only by [`install`] and freed only by
//! [`drop_inner`] (from `__gc`, once); a native accesses it through [`inner`] and never keeps the reference
//! across a call that can run Lua code (the library moves the byte vector out with `mem::take` around such
//! calls, see `lib_buffer::with_data`).
#![allow(unsafe_code)]

use rn_core::udata::Udata;
use rn_core::value::Gc;

/// Largest buffer content (the same bound as a Lua string, and LuaJIT's).
pub const MAX_LEN: usize = 0x7fff_ffff;

/// The state of one buffer.
#[derive(Default)]
pub struct BufInner {
    /// The bytes; the unread part is `data[rpos..]`.
    pub data: Vec<u8>,
    /// Read position.
    pub rpos: usize,
    /// Bytes currently charged to the memory limit (`data.capacity()` at the last charge).
    pub charged: usize,
}

/// The payload size of a buffer userdata.
pub const PAYLOAD: usize = std::mem::size_of::<*mut BufInner>();

/// Store a fresh [`BufInner`] in a zero-initialised buffer userdata.
pub fn install(u: Gc<Udata>) {
    debug_assert!(u.len() >= PAYLOAD);
    let p = Box::into_raw(Box::<BufInner>::default());
    // SAFETY: the payload has `PAYLOAD` bytes, 16-aligned, zero-initialised, and holds no live pointer yet.
    unsafe { u.data_ptr().cast::<*mut BufInner>().write(p) }
}

/// The inner state, or `None` once `__gc` ran (or before `install`).
///
/// The reference must not be held across a call that can run Lua code (finalizers, metamethods), and at most
/// one reference to one buffer may be live at a time.
#[allow(clippy::mut_from_ref)]
pub fn inner<'a>(u: &'a Gc<Udata>) -> Option<&'a mut BufInner> {
    // SAFETY: the payload was written by `install` (or is null); the box stays alive until `drop_inner`.
    unsafe {
        let p = u.data_ptr().cast::<*mut BufInner>().read();
        if p.is_null() {
            None
        } else {
            Some(&mut *p)
        }
    }
}

/// Free the inner state (`__gc`); returns the bytes that were still charged. Idempotent.
pub fn drop_inner(u: Gc<Udata>) -> usize {
    // SAFETY: as in `inner`; the pointer is nulled before the box is dropped, so a second call is a no-op.
    unsafe {
        let slot = u.data_ptr().cast::<*mut BufInner>();
        let p = slot.read();
        if p.is_null() {
            return 0;
        }
        slot.write(std::ptr::null_mut());
        let b = Box::from_raw(p);
        b.charged
    }
}
