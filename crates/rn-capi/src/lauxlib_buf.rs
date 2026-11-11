//! `luaL_Buffer`: a literal port of the buffer system of `lauxlib.c` (5.5), including the userdata
//! "box" that holds a grown buffer on the stack (so C code sees the same stack discipline).
use crate::lapi::*;
use crate::lapi_call::*;
use crate::lapi_get::*;
use crate::lauxlib::*;
use crate::types::*;
use std::ffi::{c_char, c_int, c_void};

/// Maximum size of a string (`MAX_SIZE` in `llimits.h`).
const MAX_SIZE: usize = i64::MAX as usize;

#[repr(C)]
struct UBox {
    r#box: *mut c_void,
    bsize: usize,
}

unsafe fn resizebox(l: *mut lua_State, idx: c_int, newsize: usize) -> *mut c_void {
    // SAFETY: `idx` holds a box (userdata created by `newbox`).
    unsafe {
        let b = lua_touserdata(l, idx).cast::<UBox>();
        if (*b).bsize == newsize {
            return (*b).r#box; // not changing size: keep the buffer
        }
        let mut ud: *mut c_void = std::ptr::null_mut();
        let allocf = lua_getallocf(l, &mut ud).expect("allocator");
        let temp = allocf(ud, (*b).r#box, (*b).bsize, newsize);
        if temp.is_null() && newsize > 0 {
            lua_pushstring(l, c"not enough memory".as_ptr());
            lua_error(l); // raise a memory error
        }
        (*b).r#box = temp;
        (*b).bsize = newsize;
        temp
    }
}

unsafe extern "C-unwind" fn boxgc(l: *mut lua_State) -> c_int {
    // SAFETY: called by Lua with the box as argument 1.
    unsafe { resizebox(l, 1, 0) };
    0
}

const BOXMT: &std::ffi::CStr = c"_UBOX*";

unsafe fn get_box_mt(l: *mut lua_State) {
    // SAFETY: valid state.
    unsafe {
        if luaL_getmetatable(l, BOXMT.as_ptr()) == LUA_TNIL {
            // metatable not created yet
            lua_createtable(l, 0, 2);
            lua_pushcclosure(l, Some(boxgc), 0);
            lua_setfield(l, -2, c"__gc".as_ptr());
            lua_pushcclosure(l, Some(boxgc), 0);
            lua_setfield(l, -2, c"__close".as_ptr());
            lua_copy(l, -1, -2); // change stack from nil,MT to MT,MT
            lua_setfield(l, LUA_REGISTRYINDEX, BOXMT.as_ptr());
        }
    }
}

unsafe fn newbox(l: *mut lua_State) {
    // SAFETY: valid state.
    unsafe {
        let b = lua_newuserdatauv(l, std::mem::size_of::<UBox>(), 0).cast::<UBox>();
        (*b).r#box = std::ptr::null_mut();
        (*b).bsize = 0;
        get_box_mt(l);
        lua_setmetatable(l, -2);
    }
}

#[inline]
unsafe fn buffonstack(b: *const luaL_Buffer) -> bool {
    // SAFETY: valid buffer.
    unsafe { !std::ptr::eq((*b).b, (*b).init.b.as_ptr()) }
}

unsafe fn newbuffsize(b: *mut luaL_Buffer, sz: usize) -> usize {
    // SAFETY: valid buffer.
    unsafe {
        let mut newsize = (*b).size;
        if sz >= MAX_SIZE - (*b).n {
            luaL_error(
                (*b).L,
                if cfg!(rustynail_lua54) {
                    c"buffer too large"
                } else {
                    c"resulting string too large"
                }
                .as_ptr(),
            );
        }
        if newsize <= MAX_SIZE / 3 * 2 {
            newsize += newsize >> 1; // new size *= 1.5
        }
        if newsize < (*b).n + sz + 1 {
            newsize = (*b).n + sz + 1;
        }
        newsize
    }
}

unsafe fn prepbuffsize(b: *mut luaL_Buffer, sz: usize, boxidx: c_int) -> *mut c_char {
    // SAFETY: valid buffer; `boxidx` holds the box or the placeholder.
    unsafe {
        if (*b).size - (*b).n >= sz {
            return (*b).b.add((*b).n);
        }
        let l = (*b).L;
        let newsize = newbuffsize(b, sz);
        let newbuff: *mut c_char;
        if buffonstack(b) {
            newbuff = resizebox(l, boxidx, newsize).cast();
        } else {
            // no box yet: remove placeholder, create the box, move it to its position
            lua_rotate(l, boxidx, -1);
            lua_settop(l, -2); // lua_remove
            newbox(l);
            lua_rotate(l, boxidx, 1); // lua_insert
            lua_toclose(l, boxidx);
            newbuff = resizebox(l, boxidx, newsize).cast();
            std::ptr::copy_nonoverlapping((*b).b, newbuff, (*b).n);
        }
        (*b).b = newbuff;
        (*b).size = newsize;
        newbuff.add((*b).n)
    }
}

#[no_mangle]
pub unsafe extern "C-unwind" fn luaL_prepbuffsize(b: *mut luaL_Buffer, sz: usize) -> *mut c_char {
    // SAFETY: valid buffer.
    unsafe { prepbuffsize(b, sz, -1) }
}

#[no_mangle]
pub unsafe extern "C-unwind" fn luaL_addlstring(b: *mut luaL_Buffer, s: *const c_char, l: usize) {
    // SAFETY: valid buffer; `s` has `l` readable bytes.
    unsafe {
        if l > 0 {
            let dst = prepbuffsize(b, l, -1);
            std::ptr::copy_nonoverlapping(s, dst, l);
            (*b).n += l;
        }
    }
}

#[no_mangle]
pub unsafe extern "C-unwind" fn luaL_addstring(b: *mut luaL_Buffer, s: *const c_char) {
    // SAFETY: `s` is a C string.
    unsafe { luaL_addlstring(b, s, libc::strlen(s)) }
}

#[no_mangle]
pub unsafe extern "C-unwind" fn luaL_pushresult(b: *mut luaL_Buffer) {
    // SAFETY: valid buffer; the top of the stack is its box/placeholder.
    unsafe {
        let l = (*b).L;
        // 5.4: always a copy into a regular string (the box is closed afterwards)
        #[cfg(rustynail_lua54)]
        {
            lua_pushlstring(l, (*b).b, (*b).n);
            if buffonstack(b) {
                lua_closeslot(l, -2); // close the box
            }
        }
        #[cfg(not(rustynail_lua54))]
        if !buffonstack(b) {
            lua_pushlstring(l, (*b).b, (*b).n); // save result as a regular string
        } else {
            // reuse the buffer already allocated
            let bx = lua_touserdata(l, -1).cast::<UBox>();
            let mut ud: *mut c_void = std::ptr::null_mut();
            let allocf = lua_getallocf(l, &mut ud);
            let len = (*b).n;
            resizebox(l, -1, len + 1); // adjust box size to content size
            let s = (*bx).r#box.cast::<c_char>();
            *s.add(len) = 0;
            // clear the box: Lua takes control of the buffer
            (*bx).bsize = 0;
            (*bx).r#box = std::ptr::null_mut();
            lua_pushexternalstring(l, s, len, allocf, ud);
            lua_closeslot(l, -2); // close the box
            rn_gc(l, LUA_GCSTEP, len, 0, 0);
        }
        lua_rotate(l, -2, -1);
        lua_settop(l, -2); // lua_remove(L, -2)
    }
}

#[no_mangle]
pub unsafe extern "C-unwind" fn luaL_pushresultsize(b: *mut luaL_Buffer, sz: usize) {
    // SAFETY: valid buffer.
    unsafe {
        (*b).n += sz;
        luaL_pushresult(b);
    }
}

#[no_mangle]
pub unsafe extern "C-unwind" fn luaL_addvalue(b: *mut luaL_Buffer) {
    // SAFETY: valid buffer; the value to add is on top (the box, if any, right below it).
    unsafe {
        let l = (*b).L;
        let mut len: usize = 0;
        let s = lua_tolstring(l, -1, &mut len);
        let dst = prepbuffsize(b, len, -2);
        std::ptr::copy_nonoverlapping(s, dst, len);
        (*b).n += len;
        lua_settop(l, -2); // pop string
    }
}

#[no_mangle]
pub unsafe extern "C-unwind" fn luaL_buffinit(l: *mut lua_State, b: *mut luaL_Buffer) {
    // SAFETY: `b` points to writable storage for a buffer.
    unsafe {
        (*b).L = l;
        (*b).b = (*b).init.b.as_mut_ptr();
        (*b).n = 0;
        (*b).size = LUAL_BUFFERSIZE;
        lua_pushlightuserdata(l, b.cast()); // push placeholder
    }
}

#[no_mangle]
pub unsafe extern "C-unwind" fn luaL_buffinitsize(
    l: *mut lua_State,
    b: *mut luaL_Buffer,
    sz: usize,
) -> *mut c_char {
    // SAFETY: as `luaL_buffinit`.
    unsafe {
        luaL_buffinit(l, b);
        prepbuffsize(b, sz, -1)
    }
}
