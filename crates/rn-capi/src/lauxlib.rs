//! `lauxlib.h`: the auxiliary library. Functions with an exact internal equivalent delegate to
//! `rn-core` (the stdlib uses the same code, so messages are identical); the others are literal
//! ports of `lauxlib.c` over the `lua_*` functions of this crate.
use crate::glue::*;
use crate::lapi::*;
use crate::lapi_call::*;
use crate::lapi_get::*;
use crate::types::*;
use rn_core::status::Status;
use rn_core::value::Value;
use std::ffi::{c_char, c_int, c_void, CStr};

extern "C-unwind" {
    pub fn luaL_error(l: *mut lua_State, fmt: *const c_char, ...) -> c_int;
    pub fn lua_pushfstring(l: *mut lua_State, fmt: *const c_char, ...) -> *const c_char;
    pub fn lua_pushvfstring(
        l: *mut lua_State,
        fmt: *const c_char,
        argp: *mut c_void,
    ) -> *const c_char;
    pub fn lua_gc(l: *mut lua_State, what: c_int, ...) -> c_int;
}

pub const LUA_NOREF: c_int = -2;
pub const LUA_REFNIL: c_int = -1;
pub const LUAL_NUMSIZES: usize = 8 * 16 + 8; // sizeof(lua_Integer)*16 + sizeof(lua_Number)

/// `luaL_getmetatable` (a macro in the header).
pub unsafe fn luaL_getmetatable(l: *mut lua_State, n: *const c_char) -> c_int {
    // SAFETY: valid state.
    unsafe { lua_getfield(l, LUA_REGISTRYINDEX, n) }
}

// ---- error reporting ---------------------------------------------------------------------------

lua_api!(luaL_argerror(st, arg: c_int, extramsg: *const c_char) -> c_int {
    let s = st.arg_error(arg as usize, cstr(extramsg));
    raise(&mut st, s)
});

lua_api!(luaL_typeerror(st, arg: c_int, tname: *const c_char) -> c_int {
    let s = st.type_error_arg(arg as usize, cstr(tname));
    raise(&mut st, s)
});

unsafe fn tag_error(l: *mut lua_State, arg: c_int, tag: c_int) -> ! {
    // SAFETY: valid state; `lua_typename` returns a static string.
    unsafe {
        luaL_typeerror(l, arg, lua_typename(l, tag));
        std::process::abort() // unreachable: `luaL_typeerror` always raises
    }
}

lua_api!(luaL_where(st, lvl: c_int) {
    let w = st.where_(lvl as usize);
    let r = st.push_bytes(&w);
    ck(&mut st, r);
});

lua_api!(luaL_checkversion_(st, ver: lua_Number, sz: usize) {
    if sz != LUAL_NUMSIZES {
        let s = st.l_error(b"core and library have incompatible numeric types");
        raise(&mut st, s)
    } else if ver != LUA_VERSION_NUM {
        let a = rn_core::number::tostring_float(ver);
        let b = rn_core::number::tostring_float(LUA_VERSION_NUM);
        let msg = format!("version mismatch: app. needs {}, Lua core provides {}",
            String::from_utf8_lossy(a.as_bytes()), String::from_utf8_lossy(b.as_bytes()));
        let s = st.l_error(msg.as_bytes());
        raise(&mut st, s)
    }
});

// ---- argument checking -------------------------------------------------------------------------

lua_api!(luaL_getmetafield(st, obj: c_int, e: *const c_char) -> c_int {
    let v = val(&st, obj);
    let r = st.get_metafield(&v, cstr(e));
    if r.is_nil() {
        return LUA_TNIL;
    }
    st.push(r);
    r.type_tag() as c_int
});

lua_api!(luaL_callmeta(st, obj: c_int, e: *const c_char) -> c_int {
    let v = val(&st, obj);
    let r = st.call_meta(&v, cstr(e));
    match ck(&mut st, r) {
        Some(res) => {
            st.push(res);
            1
        }
        None => 0,
    }
});

lua_api!(luaL_tolstring(st, idx: c_int, len: *mut usize) -> *const c_char {
    let v = val(&st, idx);
    let r = st.tolstring(v);
    let s = ck(&mut st, r);
    let b = s.get().as_bytes();
    if !len.is_null() {
        *len = b.len();
    }
    b.as_ptr().cast()
});

#[no_mangle]
pub unsafe extern "C-unwind" fn luaL_checklstring(
    l: *mut lua_State,
    arg: c_int,
    len: *mut usize,
) -> *const c_char {
    // SAFETY: valid state.
    unsafe {
        let s = lua_tolstring(l, arg, len);
        if s.is_null() {
            tag_error(l, arg, LUA_TSTRING);
        }
        s
    }
}

#[no_mangle]
pub unsafe extern "C-unwind" fn luaL_optlstring(
    l: *mut lua_State,
    arg: c_int,
    def: *const c_char,
    len: *mut usize,
) -> *const c_char {
    // SAFETY: valid state.
    unsafe {
        let t = lua_type(l, arg);
        if t == LUA_TNONE || t == LUA_TNIL {
            if !len.is_null() {
                *len = if def.is_null() { 0 } else { libc::strlen(def) };
            }
            def
        } else {
            luaL_checklstring(l, arg, len)
        }
    }
}

#[no_mangle]
pub unsafe extern "C-unwind" fn luaL_checknumber(l: *mut lua_State, arg: c_int) -> lua_Number {
    // SAFETY: valid state.
    unsafe {
        let mut isnum = 0;
        let d = lua_tonumberx(l, arg, &mut isnum);
        if isnum == 0 {
            tag_error(l, arg, LUA_TNUMBER);
        }
        d
    }
}

#[no_mangle]
pub unsafe extern "C-unwind" fn luaL_optnumber(
    l: *mut lua_State,
    arg: c_int,
    def: lua_Number,
) -> lua_Number {
    // SAFETY: valid state.
    unsafe {
        let t = lua_type(l, arg);
        if t == LUA_TNONE || t == LUA_TNIL {
            def
        } else {
            luaL_checknumber(l, arg)
        }
    }
}

unsafe fn interror(l: *mut lua_State, arg: c_int) -> ! {
    // SAFETY: valid state.
    unsafe {
        if lua_isnumber(l, arg) != 0 {
            luaL_argerror(l, arg, c"number has no integer representation".as_ptr());
        } else {
            tag_error(l, arg, LUA_TNUMBER);
        }
        std::process::abort() // unreachable
    }
}

#[no_mangle]
pub unsafe extern "C-unwind" fn luaL_checkinteger(l: *mut lua_State, arg: c_int) -> lua_Integer {
    // SAFETY: valid state.
    unsafe {
        let mut isnum = 0;
        let d = lua_tointegerx(l, arg, &mut isnum);
        if isnum == 0 {
            interror(l, arg);
        }
        d
    }
}

#[no_mangle]
pub unsafe extern "C-unwind" fn luaL_optinteger(
    l: *mut lua_State,
    arg: c_int,
    def: lua_Integer,
) -> lua_Integer {
    // SAFETY: valid state.
    unsafe {
        let t = lua_type(l, arg);
        if t == LUA_TNONE || t == LUA_TNIL {
            def
        } else {
            luaL_checkinteger(l, arg)
        }
    }
}

lua_api!(luaL_checkstack(st, sz: c_int, msg: *const c_char) {
    if !st.check_stack_api(sz.max(0) as usize) {
        let s = if msg.is_null() {
            st.l_error(b"stack overflow")
        } else {
            let mut m = b"stack overflow (".to_vec();
            m.extend_from_slice(cstr(msg));
            m.push(b')');
            st.l_error(&m)
        };
        raise(&mut st, s)
    }
});

#[no_mangle]
pub unsafe extern "C-unwind" fn luaL_checktype(l: *mut lua_State, arg: c_int, t: c_int) {
    // SAFETY: valid state.
    unsafe {
        if lua_type(l, arg) != t {
            tag_error(l, arg, t);
        }
    }
}

#[no_mangle]
pub unsafe extern "C-unwind" fn luaL_checkany(l: *mut lua_State, arg: c_int) {
    // SAFETY: valid state.
    unsafe {
        if lua_type(l, arg) == LUA_TNONE {
            luaL_argerror(l, arg, c"value expected".as_ptr());
        }
    }
}

#[no_mangle]
pub unsafe extern "C-unwind" fn luaL_checkoption(
    l: *mut lua_State,
    arg: c_int,
    def: *const c_char,
    lst: *const *const c_char,
) -> c_int {
    // SAFETY: `lst` is a NULL-terminated array of C strings.
    unsafe {
        let name = if !def.is_null() {
            luaL_optlstring(l, arg, def, std::ptr::null_mut())
        } else {
            luaL_checklstring(l, arg, std::ptr::null_mut())
        };
        let mut i = 0;
        while !(*lst.add(i)).is_null() {
            if libc::strcmp(*lst.add(i), name) == 0 {
                return i as c_int;
            }
            i += 1;
        }
        let msg = lua_pushfstring(l, c"invalid option '%s'".as_ptr(), name);
        luaL_argerror(l, arg, msg)
    }
}

// ---- userdata metatables -----------------------------------------------------------------------

#[no_mangle]
pub unsafe extern "C-unwind" fn luaL_newmetatable(
    l: *mut lua_State,
    tname: *const c_char,
) -> c_int {
    // SAFETY: valid state.
    unsafe {
        if luaL_getmetatable(l, tname) != LUA_TNIL {
            return 0; // name already in use: leave the previous value on top
        }
        lua_settop(l, -2);
        lua_createtable(l, 0, 2);
        lua_pushstring(l, tname);
        lua_setfield(l, -2, c"__name".as_ptr());
        lua_pushvalue(l, -1);
        lua_setfield(l, LUA_REGISTRYINDEX, tname);
        1
    }
}

#[no_mangle]
pub unsafe extern "C-unwind" fn luaL_setmetatable(l: *mut lua_State, tname: *const c_char) {
    // SAFETY: valid state.
    unsafe {
        luaL_getmetatable(l, tname);
        lua_setmetatable(l, -2);
    }
}

#[no_mangle]
pub unsafe extern "C-unwind" fn luaL_testudata(
    l: *mut lua_State,
    ud: c_int,
    tname: *const c_char,
) -> *mut c_void {
    // SAFETY: valid state.
    unsafe {
        let mut p = lua_touserdata(l, ud);
        if !p.is_null() && lua_getmetatable(l, ud) != 0 {
            luaL_getmetatable(l, tname);
            if lua_rawequal(l, -1, -2) == 0 {
                p = std::ptr::null_mut();
            }
            lua_settop(l, -3);
            return p;
        }
        std::ptr::null_mut()
    }
}

#[no_mangle]
pub unsafe extern "C-unwind" fn luaL_checkudata(
    l: *mut lua_State,
    ud: c_int,
    tname: *const c_char,
) -> *mut c_void {
    // SAFETY: valid state.
    unsafe {
        let p = luaL_testudata(l, ud, tname);
        if p.is_null() {
            luaL_typeerror(l, ud, tname);
        }
        p
    }
}

// ---- results -----------------------------------------------------------------------------------

fn errno() -> c_int {
    // SAFETY: reading this thread's errno.
    unsafe { *libc::__errno_location() }
}

#[no_mangle]
pub unsafe extern "C-unwind" fn luaL_fileresult(
    l: *mut lua_State,
    stat: c_int,
    fname: *const c_char,
) -> c_int {
    // SAFETY: valid state.
    unsafe {
        let en = errno(); // calls to the Lua API may change this value
        if stat != 0 {
            lua_pushboolean(l, 1);
            return 1;
        }
        lua_pushnil(l);
        let msg = if en != 0 {
            libc::strerror(en)
        } else {
            c"(no extra info)".as_ptr().cast_mut()
        };
        if !fname.is_null() {
            lua_pushfstring(l, c"%s: %s".as_ptr(), fname, msg);
        } else {
            lua_pushstring(l, msg);
        }
        lua_pushinteger(l, en as lua_Integer);
        3
    }
}

#[no_mangle]
pub unsafe extern "C-unwind" fn luaL_execresult(l: *mut lua_State, stat: c_int) -> c_int {
    // SAFETY: valid state.
    unsafe {
        let mut stat = stat;
        if stat != 0 && errno() != 0 {
            return luaL_fileresult(l, 0, std::ptr::null());
        }
        let mut what = c"exit".as_ptr();
        if libc::WIFEXITED(stat) {
            stat = libc::WEXITSTATUS(stat);
        } else if libc::WIFSIGNALED(stat) {
            stat = libc::WTERMSIG(stat);
            what = c"signal".as_ptr();
        }
        if *what == b'e' as c_char && stat == 0 {
            lua_pushboolean(l, 1);
        } else {
            lua_pushnil(l);
        }
        lua_pushstring(l, what);
        lua_pushinteger(l, stat as lua_Integer);
        3
    }
}

// ---- references --------------------------------------------------------------------------------

lua_api!(luaL_ref(st, t: c_int) -> c_int {
    let v = st.get(st.top() - 1);
    if v.is_nil() {
        let top = st.top();
        st.set_top(top - 1);
        return LUA_REFNIL;
    }
    let tbl = val(&st, t).as_table().expect("table expected (API contract)");
    let r = st.ref_(tbl, v);
    let rf = ck(&mut st, r);
    let top = st.top();
    st.set_top(top - 1);
    rf
});

lua_api!(luaL_unref(st, t: c_int, r: c_int) {
    if r >= 0 {
        let tbl = val(&st, t).as_table().expect("table expected (API contract)");
        let res = st.unref(tbl, r);
        ck(&mut st, res);
    }
});

// ---- load functions ----------------------------------------------------------------------------

lua_api!(luaL_loadfilex(st, filename: *const c_char, mode: *const c_char) -> c_int {
    let status = st.load_filex(cstr_opt(filename), cstr_opt(mode));
    if status == Status::ErrSyntax {
        // file errors are reported as LUA_ERRFILE
        let m = st.get(st.top() - 1);
        if let Some(s) = m.as_string() {
            let b = s.as_bytes();
            if b.starts_with(b"cannot open ") || b.starts_with(b"cannot read ") {
                return LUA_ERRFILE;
            }
        }
    }
    status as c_int
});

lua_api!(luaL_loadbufferx(
    st, buff: *const c_char, sz: usize, name: *const c_char, mode: *const c_char
) -> c_int {
    let bytes: &[u8] = if sz == 0 { &[] } else { std::slice::from_raw_parts(buff.cast(), sz) };
    st.load_chunk(bytes, cstr_opt(name).unwrap_or(b"?"), cstr_opt(mode)) as c_int
});

lua_api!(luaL_loadstring(st, s: *const c_char) -> c_int {
    let b = cstr(s);
    st.load_chunk(b, b, None) as c_int
});

// ---- state -------------------------------------------------------------------------------------

#[no_mangle]
pub unsafe extern "C-unwind" fn luaL_alloc(
    _ud: *mut c_void,
    ptr: *mut c_void,
    _osize: usize,
    nsize: usize,
) -> *mut c_void {
    // SAFETY: standard `lua_Alloc` contract over the C allocator.
    unsafe {
        if nsize == 0 {
            libc::free(ptr);
            std::ptr::null_mut()
        } else {
            libc::realloc(ptr, nsize)
        }
    }
}

/// The standard panic function: print the error message (the abort follows).
unsafe extern "C-unwind" fn panic(l: *mut lua_State) -> c_int {
    // SAFETY: valid state.
    unsafe {
        let msg: *const c_char = if lua_type(l, -1) == LUA_TSTRING {
            lua_tolstring(l, -1, std::ptr::null_mut())
        } else {
            c"error object is not a string".as_ptr()
        };
        let m = CStr::from_ptr(msg).to_string_lossy();
        eprintln!("PANIC: unprotected error in call to Lua API ({m})");
        0
    }
}

#[no_mangle]
pub unsafe extern "C-unwind" fn luaL_makeseed(_l: *mut lua_State) -> u32 {
    rn_core::auxlib::make_seed()
}

#[no_mangle]
pub unsafe extern "C-unwind" fn luaL_newstate() -> *mut lua_State {
    // SAFETY: creating a state with the C allocator.
    unsafe {
        #[cfg(not(rustynail_lua54))]
        let l = lua_newstate(
            Some(luaL_alloc),
            std::ptr::null_mut(),
            luaL_makeseed(std::ptr::null_mut()),
        );
        #[cfg(rustynail_lua54)]
        let l = lua_newstate(Some(luaL_alloc), std::ptr::null_mut());
        if !l.is_null() {
            lua_atpanic(l, Some(panic));
            state(l).install_warn_function(false);
        }
        l
    }
}

lua_api!(luaL_len(st, idx: c_int) -> lua_Integer {
    let v = val(&st, idx);
    let r = st.len_value(v);
    let l = ck(&mut st, r);
    match l.as_int() {
        Some(i) => i,
        None => {
            let s = st.l_error(b"object length is not an integer");
            raise(&mut st, s)
        }
    }
});

// ---- misc --------------------------------------------------------------------------------------

#[no_mangle]
pub unsafe extern "C-unwind" fn luaL_addgsub(
    b: *mut luaL_Buffer,
    s: *const c_char,
    p: *const c_char,
    r: *const c_char,
) {
    // SAFETY: C strings; `b` is a valid buffer.
    unsafe {
        let l = libc::strlen(p);
        let mut s = s;
        loop {
            let wild = libc::strstr(s, p);
            if wild.is_null() {
                break;
            }
            crate::lauxlib_buf::luaL_addlstring(b, s, wild.offset_from(s) as usize);
            crate::lauxlib_buf::luaL_addstring(b, r);
            s = wild.add(l);
        }
        crate::lauxlib_buf::luaL_addstring(b, s);
    }
}

#[no_mangle]
pub unsafe extern "C-unwind" fn luaL_gsub(
    l: *mut lua_State,
    s: *const c_char,
    p: *const c_char,
    r: *const c_char,
) -> *const c_char {
    // SAFETY: valid state; C strings.
    unsafe {
        let mut b = std::mem::MaybeUninit::<luaL_Buffer>::uninit();
        crate::lauxlib_buf::luaL_buffinit(l, b.as_mut_ptr());
        luaL_addgsub(b.as_mut_ptr(), s, p, r);
        crate::lauxlib_buf::luaL_pushresult(b.as_mut_ptr());
        lua_tolstring(l, -1, std::ptr::null_mut())
    }
}

#[no_mangle]
pub unsafe extern "C-unwind" fn luaL_setfuncs(
    l: *mut lua_State,
    regs: *const luaL_Reg,
    nup: c_int,
) {
    // SAFETY: `regs` is terminated by an entry with a NULL name.
    unsafe {
        luaL_checkstack(l, nup, c"too many upvalues".as_ptr());
        let mut r = regs;
        while !(*r).name.is_null() {
            match (*r).func {
                None => lua_pushboolean(l, 0), // placeholder
                Some(_) => {
                    for _ in 0..nup {
                        lua_pushvalue(l, -nup); // copy upvalues to the top
                    }
                    lua_pushcclosure(l, (*r).func, nup);
                }
            }
            lua_setfield(l, -(nup + 2), (*r).name);
            r = r.add(1);
        }
        lua_settop(l, -nup - 1); // remove upvalues
    }
}

#[no_mangle]
pub unsafe extern "C-unwind" fn luaL_getsubtable(
    l: *mut lua_State,
    idx: c_int,
    fname: *const c_char,
) -> c_int {
    // SAFETY: valid state.
    unsafe {
        if lua_getfield(l, idx, fname) == LUA_TTABLE {
            return 1; // table already there
        }
        lua_settop(l, -2); // remove previous result
        let idx = lua_absindex(l, idx);
        lua_createtable(l, 0, 0);
        lua_pushvalue(l, -1); // copy to be left at top
        lua_setfield(l, idx, fname); // assign new table to field
        0
    }
}

#[no_mangle]
pub unsafe extern "C-unwind" fn luaL_traceback(
    l: *mut lua_State,
    l1: *mut lua_State,
    msg: *const c_char,
    level: c_int,
) {
    // SAFETY: valid states.
    unsafe {
        let mut s1 = state(l1);
        let tb = s1.traceback(cstr_opt(msg), level);
        let mut st = state(l);
        let r = st.push_bytes(&tb);
        ck(&mut st, r);
    }
}

#[no_mangle]
pub unsafe extern "C-unwind" fn luaL_requiref(
    l: *mut lua_State,
    modname: *const c_char,
    openf: lua_CFunction,
    glb: c_int,
) {
    // SAFETY: valid state.
    unsafe {
        luaL_getsubtable(l, LUA_REGISTRYINDEX, c"_LOADED".as_ptr());
        lua_getfield(l, -1, modname); // LOADED[modname]
        if lua_toboolean(l, -1) == 0 {
            // package not already loaded
            lua_settop(l, -2); // remove field
            lua_pushcclosure(l, openf, 0);
            lua_pushstring(l, modname); // argument to the open function
            lua_callk(l, 1, 1, 0, None); // call 'openf' to open the module
            lua_pushvalue(l, -1); // make a copy of the module (call result)
            lua_setfield(l, -3, modname); // LOADED[modname] = module
        }
        lua_rotate(l, -2, -1);
        lua_settop(l, -2); // remove the LOADED table
        if glb != 0 {
            lua_pushvalue(l, -1); // copy of the module
            lua_setglobal(l, modname); // _G[modname] = module
        }
    }
}

#[allow(dead_code)]
fn _unused(_: Value) {}
