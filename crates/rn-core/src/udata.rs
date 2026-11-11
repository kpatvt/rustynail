//! Full userdata (`Udata`, `lobject.h`): header, optional user values, then the raw payload.
//!
//! Layout (both Lua versions): `[GcHeader][nuvalue: u16][len][metatable][gclist][uv[nuvalue]]
//! [payload...]`. The payload offset is a multiple of 16, so the payload pointer is
//! `max_align_t`-aligned (the block itself is 16-aligned). With `nuvalue == 0` the user-value array
//! is empty and the payload starts right at `UDATA_HEADER` (the reference's `Udata0` shape).
//!
//! **Invariants:** (U1) the block holds `UDATA_HEADER + 16 * nuvalue + len` bytes; the `nuvalue`
//! user values are initialised (nil at creation); (U2) `metatable` is null or a live table.
use crate::gc::GcHeader;
use crate::heap::Heap;
use crate::state::State;
use crate::status::{LuaResult, Status};
use crate::table::TableRef;
use crate::value::*;

/// The userdata object. `#[repr(C)]`, header first.
#[repr(C)]
pub struct Udata {
    pub(crate) hdr: GcHeader,
    pub(crate) nuvalue: u16,
    pub(crate) len: usize,
    pub(crate) metatable: *mut crate::table::Table,
    pub(crate) gclist: *mut GcHeader,
    uv: [Value; 0],
}

/// Offset of the user-value array / payload when there are no user values.
pub const UDATA_HEADER: usize = std::mem::size_of::<Udata>();
const _: () = assert!(UDATA_HEADER.is_multiple_of(16));

/// `udatamemoffset`.
pub(crate) fn mem_offset(nuv: usize) -> usize {
    UDATA_HEADER + nuv * std::mem::size_of::<Value>()
}
/// `sizeudata`.
pub(crate) fn size_udata(nuv: usize, len: usize) -> Option<usize> {
    mem_offset(nuv).checked_add(len)
}

impl Heap {
    /// `luaS_newudata`: `len` bytes of zero-initialised payload and `nuvalue` nil user values.
    pub fn new_udata(&mut self, len: usize, nuvalue: usize) -> LuaResult<Gc<Udata>> {
        if nuvalue > u16::MAX as usize {
            return Err(Status::ErrMem);
        }
        let size = size_udata(nuvalue, len).ok_or(Status::ErrMem)?;
        let o = self.alloc_obj(size, LUA_VUSERDATA)?;
        let u = o.cast::<Udata>();
        // SAFETY: fresh block of `size` bytes (U1).
        unsafe {
            (&raw mut (*u).nuvalue).write(nuvalue as u16);
            (&raw mut (*u).len).write(len);
            (&raw mut (*u).metatable).write(std::ptr::null_mut());
            (&raw mut (*u).gclist).write(std::ptr::null_mut());
            let uvs = (&raw mut (*u).uv).cast::<Value>();
            for i in 0..nuvalue {
                uvs.add(i).write(Value::NIL);
            }
            std::ptr::write_bytes(o.cast::<u8>().add(mem_offset(nuvalue)), 0, len);
            Ok(Gc::from_raw(u))
        }
    }
}

impl Udata {
    /// Payload size in bytes.
    pub fn len(&self) -> usize {
        self.len
    }
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
    pub fn nuvalue(&self) -> usize {
        self.nuvalue as usize
    }
    pub fn metatable(&self) -> Option<TableRef> {
        if self.metatable.is_null() {
            None
        } else {
            // SAFETY: U2.
            Some(unsafe { TableRef::from_raw(self.metatable) })
        }
    }
}

impl Gc<Udata> {
    /// Pointer to the payload (`getudatamem`), 16-byte aligned, stable for the object's lifetime.
    pub fn data_ptr(self) -> *mut u8 {
        // SAFETY: U1 — the payload lies inside the block.
        unsafe {
            self.as_ptr()
                .cast::<u8>()
                .add(mem_offset(self.nuvalue as usize))
        }
    }
    /// Payload as a byte slice.
    ///
    /// # Safety
    /// Native code must not hold the slice across anything that mutates the payload.
    pub unsafe fn data(&self) -> &[u8] {
        // SAFETY: caller contract + U1.
        unsafe { std::slice::from_raw_parts(self.data_ptr(), self.len) }
    }
    /// Payload as a mutable byte slice.
    ///
    /// # Safety
    /// Exclusive access to the payload for the slice's lifetime.
    #[allow(clippy::mut_from_ref)]
    pub unsafe fn data_mut(&self) -> &mut [u8] {
        // SAFETY: caller contract + U1.
        unsafe { std::slice::from_raw_parts_mut(self.data_ptr(), self.len) }
    }
    /// The `n`-th (0-based) user value.
    pub fn user_value(self, n: usize) -> Option<Value> {
        if n < self.nuvalue as usize {
            // SAFETY: in range (U1).
            Some(unsafe { *(&raw mut (*self.as_ptr()).uv).cast::<Value>().add(n) })
        } else {
            None
        }
    }
    /// Set the `n`-th user value; returns false if out of range. Issues the barrier.
    pub fn set_user_value(self, heap: &mut Heap, n: usize, v: Value) -> bool {
        if n < self.nuvalue as usize {
            // SAFETY: in range (U1).
            unsafe {
                (&raw mut (*self.as_ptr()).uv)
                    .cast::<Value>()
                    .add(n)
                    .write(v)
            };
            heap.barrier_back(self.as_ptr().cast(), &v);
            true
        } else {
            false
        }
    }
    /// Set the raw metatable pointer (use [`State::set_metatable`] for the full semantics).
    pub fn set_metatable(self, mt: Option<TableRef>) {
        // SAFETY: handle invariant.
        unsafe { (*self.as_ptr()).metatable = mt.map_or(std::ptr::null_mut(), TableRef::as_ptr) }
    }
}

impl State {
    /// `lua_newuserdatauv`.
    pub fn new_udata(&mut self, len: usize, nuvalue: usize) -> LuaResult<Gc<Udata>> {
        self.try_alloc(|h| h.new_udata(len, nuvalue))
    }
}
