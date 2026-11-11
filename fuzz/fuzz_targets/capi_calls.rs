//! Random sequences of C API calls (the newest `unsafe` surface). The byte program is interpreted
//! inside a C function called with `lua_pcall`, so Lua errors take the normal path; the program
//! only issues calls that satisfy the API contract (valid indices, right types), exactly like a
//! correct C client. Any crash/UB (ASan) or panic is a bug in `rn-capi`/`rn-core`.
#![no_main]
use libfuzzer_sys::fuzz_target;
use rn_capi::lapi::*;
use rn_capi::lapi_call::*;
use rn_capi::lapi_debug::*;
use rn_capi::lapi_get::*;
use rn_capi::lauxlib::*;
use rn_capi::lauxlib_buf::*;
use rn_capi::lualib::*;
use rn_capi::*;
#[cfg(not(rustynail_lua54))]
use std::ffi::c_char;
use std::ffi::{c_int, c_void};

const SNIPPETS: &[&str] = &[
    "return ...",
    "local a, b = ... return (a or 1) + (b or 2)",
    "local t = ... return type(t), #tostring(t)",
    "error('boom')",
    "return coroutine.isyieldable()",
    "local x = {} for i = 1, 50 do x[i] = i end return #x",
    "return select('#', ...)",
];

struct Prog<'a> {
    d: &'a [u8],
    pos: usize,
}

impl Prog<'_> {
    fn next(&mut self) -> Option<u8> {
        let b = *self.d.get(self.pos)?;
        self.pos += 1;
        Some(b)
    }
}

unsafe fn top(l: *mut lua_State) -> c_int {
    unsafe { lua_gettop(l) }
}

/// A valid acceptable index into the current stack, if any.
unsafe fn pick(l: *mut lua_State, b: u8) -> Option<c_int> {
    unsafe {
        let n = top(l);
        if n == 0 {
            return None;
        }
        let i = 1 + (b as c_int % n);
        Some(if b & 0x80 != 0 { i - n - 1 } else { i })
    }
}

unsafe fn pick_type(l: *mut lua_State, b: u8, ty: c_int) -> Option<c_int> {
    unsafe {
        let n = top(l);
        for k in 0..n {
            let i = 1 + ((b as c_int + k) % n);
            if lua_type(l, i) == ty {
                return Some(i);
            }
        }
        None
    }
}

unsafe extern "C-unwind" fn dummy_cf(l: *mut lua_State) -> c_int {
    unsafe { lua_gettop(l) }
}

unsafe extern "C-unwind" fn runner(l: *mut lua_State) -> c_int {
    unsafe {
        let data = lua_touserdata(l, 1).cast::<(*const u8, usize)>();
        let (ptr, len) = *data;
        let mut p = Prog { d: std::slice::from_raw_parts(ptr, len), pos: 0 };
        lua_settop(l, 0);
        let mut steps = 0;
        while let Some(op) = p.next() {
            steps += 1;
            if steps > 400 {
                break;
            }
            if top(l) > 150 {
                lua_settop(l, 20);
            }
            lua_checkstack(l, 20);
            let a = p.next().unwrap_or(0);
            let b = p.next().unwrap_or(0);
            match op % 48 {
                0 => lua_pushnil(l),
                1 => lua_pushboolean(l, (a & 1) as c_int),
                2 => lua_pushinteger(l, i64::from_le_bytes([a, b, a, b, a, 0, 0, b])),
                3 => lua_pushnumber(l, f64::from_bits(u64::from_le_bytes([a, b, 0, 0, 0, 0, b, a]))),
                4 => {
                    let n = (a % 24) as usize;
                    let s: Vec<u8> = (0..n).map(|i| b.wrapping_add(i as u8)).collect();
                    lua_pushlstring(l, s.as_ptr().cast(), s.len());
                }
                5 => lua_createtable(l, (a % 8) as c_int, (b % 8) as c_int),
                6 => {
                    lua_newuserdatauv(l, (a % 64) as usize, (b % 3) as c_int);
                }
                7 => lua_pushlightuserdata(l, (a as usize + 1) as *mut c_void),
                8 => lua_pushcclosure(l, Some(dummy_cf), 0),
                9 => {
                    // closure with upvalues taken from the stack
                    let n = (a % 3) as c_int;
                    if top(l) >= n {
                        lua_pushcclosure(l, Some(dummy_cf), n);
                    }
                }
                10 => {
                    lua_pushthread(l);
                }
                11 => {
                    lua_newthread(l);
                }
                12 => {
                    if let Some(i) = pick(l, a) {
                        lua_pushvalue(l, i);
                    }
                }
                13 => lua_settop(l, ((a % 12) as c_int).min(top(l) + 3)),
                14 => {
                    if top(l) > 0 {
                        lua_settop(l, -1 - (a as c_int % top(l)));
                    }
                }
                15 => {
                    if let Some(i) = pick(l, a) {
                        let i = lua_absindex(l, i);
                        let n = top(l) - i + 1;
                        lua_rotate(l, i, (b as c_int % (n + 1)) * if b & 1 == 0 { 1 } else { -1 } % (n + 1));
                    }
                }
                16 => {
                    if let (Some(i), Some(j)) = (pick(l, a), pick(l, b)) {
                        lua_copy(l, i, j);
                    }
                }
                17 => {
                    // table writes
                    if let Some(t) = pick_type(l, a, LUA_TTABLE) {
                        let t = lua_absindex(l, t);
                        match b % 6 {
                            0 => { lua_pushinteger(l, b as i64); lua_setfield(l, t, c"fld".as_ptr()); }
                            1 => { lua_pushinteger(l, a as i64); lua_pushinteger(l, b as i64); lua_settable(l, t); }
                            2 => { lua_pushinteger(l, a as i64); lua_seti(l, t, (b % 40) as i64); }
                            3 => { lua_pushinteger(l, 1); lua_pushinteger(l, a as i64); lua_rawset(l, t); }
                            4 => { lua_pushinteger(l, b as i64); lua_rawseti(l, t, (a % 40) as i64); }
                            _ => { lua_pushnil(l); lua_rawsetp(l, t, (a as usize + 1) as *const c_void); }
                        }
                    }
                }
                18 => {
                    if let Some(t) = pick_type(l, a, LUA_TTABLE) {
                        let t = lua_absindex(l, t);
                        match b % 6 {
                            0 => { lua_getfield(l, t, c"fld".as_ptr()); }
                            1 => { lua_pushinteger(l, a as i64); lua_gettable(l, t); }
                            2 => { lua_geti(l, t, (b % 40) as i64); }
                            3 => { lua_pushinteger(l, 1); lua_rawget(l, t); }
                            4 => { lua_rawgeti(l, t, (a % 40) as i64); }
                            _ => { lua_rawgetp(l, t, (a as usize + 1) as *const c_void); }
                        }
                    }
                }
                19 => {
                    // metatable set (table or nil on top)
                    if let Some(t) = pick_type(l, a, LUA_TTABLE).or_else(|| pick_type(l, a, LUA_TUSERDATA)) {
                        let t = lua_absindex(l, t);
                        if b & 1 == 0 { lua_pushnil(l) } else { lua_createtable(l, 0, 1) }
                        lua_setmetatable(l, t);
                    }
                }
                20 => {
                    if let Some(i) = pick(l, a) {
                        if lua_getmetatable(l, i) == 0 { lua_pushnil(l) }
                    }
                }
                21 => {
                    // iterate a table with next
                    if let Some(t) = pick_type(l, a, LUA_TTABLE) {
                        let t = lua_absindex(l, t);
                        lua_pushnil(l);
                        let mut guard = 0;
                        while lua_next(l, t) != 0 && guard < 50 {
                            lua_settop(l, -2);
                            guard += 1;
                        }
                        if guard >= 50 { lua_settop(l, t + 0 + (top(l) - t).min(1)); }
                    }
                }
                22 => {
                    if let Some(i) = pick(l, a) {
                        lua_rawlen(l, i);
                        lua_len(l, i);
                    }
                }
                23 => {
                    let n = (a % 4) as c_int;
                    if n <= top(l) { lua_concat(l, n) }
                }
                24 => {
                    if top(l) >= 2 { lua_arith(l, (a % 14) as c_int) }
                }
                25 => {
                    if let (Some(i), Some(j)) = (pick(l, a), pick(l, b)) {
                        lua_compare(l, i, j, (a % 3) as c_int);
                        lua_rawequal(l, i, j);
                    }
                }
                26 => {
                    if let Some(i) = pick(l, a) {
                        lua_tolstring(l, i, std::ptr::null_mut());
                        let mut isnum = 0;
                        lua_tonumberx(l, i, &mut isnum);
                        lua_tointegerx(l, i, &mut isnum);
                        lua_topointer(l, i);
                        lua_type(l, i);
                        lua_typename(l, lua_type(l, i));
                    }
                }
                27 => {
                    if let Some(i) = pick(l, a) {
                        luaL_tolstring(l, i, std::ptr::null_mut());
                    }
                }
                28 => {
                    // load and call a snippet with arguments from the stack
                    let s = SNIPPETS[a as usize % SNIPPETS.len()];
                    if luaL_loadbufferx(l, s.as_ptr().cast(), s.len(), c"=snip".as_ptr(), std::ptr::null()) == 0 {
                        let nargs = (b % 3) as c_int;
                        if top(l) > nargs {
                            lua_rotate(l, top(l) - nargs, 1); // function below its arguments
                            if a & 1 == 0 {
                                lua_callk(l, nargs, (b % 3) as c_int, 0, None);
                            } else {
                                lua_pcallk(l, nargs, LUA_MULTRET, 0, 0, None);
                            }
                        }
                    }
                }
                29 => {
                    // buffer burst
                    let mut buf = std::mem::MaybeUninit::<luaL_Buffer>::uninit();
                    let bp = buf.as_mut_ptr();
                    luaL_buffinit(l, bp);
                    for k in 0..(a % 40) {
                        luaL_addlstring(bp, c"0123456789".as_ptr(), (k % 10) as usize);
                        if k % 7 == 0 {
                            lua_pushinteger(l, k as i64);
                            luaL_addvalue(bp);
                        }
                    }
                    if b & 1 == 0 {
                        let p = luaL_prepbuffsize(bp, 3000);
                        std::ptr::write_bytes(p, b'x', 3000);
                        (*bp).n += 3000;
                    }
                    luaL_pushresult(bp);
                }
                30 => {
                    // registry references
                    if top(l) > 0 {
                        let r = luaL_ref(l, LUA_REGISTRYINDEX);
                        lua_rawgeti(l, LUA_REGISTRYINDEX, r as i64);
                        luaL_unref(l, LUA_REGISTRYINDEX, r);
                    }
                }
                31 => {
                    let w = (a % 7) as c_int;
                    let what = [LUA_GCCOLLECT, LUA_GCSTEP, LUA_GCGEN, LUA_GCINC, LUA_GCCOUNT, LUA_GCISRUNNING, LUA_GCCOLLECT][w as usize];
                    #[cfg(not(rustynail_lua54))]
                    if what == LUA_GCSTEP { lua_gc(l, what, (b as usize) * 8) } else { lua_gc(l, what) };
                    // 5.4: `step(Kbytes)`, `gen(minormul, majormul)`, `inc(pause, stepmul, stepsize)`
                    #[cfg(rustynail_lua54)]
                    match what {
                        LUA_GCSTEP => lua_gc(l, what, (b % 64) as c_int),
                        LUA_GCGEN => lua_gc(l, what, 0 as c_int, 0 as c_int),
                        LUA_GCINC => lua_gc(l, what, 0 as c_int, 0 as c_int, 0 as c_int),
                        _ => lua_gc(l, what),
                    };
                }
                32 => {
                    // user values
                    if let Some(u) = pick_type(l, a, LUA_TUSERDATA) {
                        let u = lua_absindex(l, u);
                        lua_pushinteger(l, b as i64);
                        lua_setiuservalue(l, u, (a % 4) as c_int);
                        lua_getiuservalue(l, u, (b % 4) as c_int);
                    }
                }
                33 => {
                    // debug interface on the running frame
                    let mut ar: lua_Debug = std::mem::zeroed();
                    if lua_getstack(l, (a % 3) as c_int, &mut ar) != 0 {
                        lua_getinfo(l, c"nSltu".as_ptr(), &mut ar);
                        let nm = lua_getlocal(l, &ar, (b % 4) as c_int + 1);
                        if !nm.is_null() { lua_settop(l, -2); }
                    }
                }
                34 => {
                    if let Some(f) = pick_type(l, a, LUA_TFUNCTION) {
                        let f = lua_absindex(l, f);
                        let n = lua_getupvalue(l, f, (b % 3) as c_int + 1);
                        if !n.is_null() { lua_settop(l, -2); }
                        lua_upvalueid(l, f, (b % 3) as c_int + 1);
                    }
                }
                35 => {
                    if let Some(i) = pick(l, a) {
                        luaL_getmetafield(l, i, c"__index".as_ptr());
                        luaL_callmeta(l, i, c"__tostring".as_ptr());
                    }
                }
                36 => { luaL_where(l, (a % 3) as c_int); }
                37 => luaL_traceback(l, l, c"fuzz".as_ptr(), (a % 3) as c_int),
                38 => { lua_pushfstring(l, c"%s|%d|%I|%f|%c|%%|%U".as_ptr(), c"str".as_ptr(), a as c_int, b as i64, a as f64 / 3.0, b as c_int, 0x20ACu64); }
                39 => {
                    // error with a value from the stack (caught by the outer pcall)
                    if a % 5 == 0 { lua_error(l); }
                }
                40 => { luaL_newmetatable(l, if a & 1 == 0 { c"fuzz.T1".as_ptr() } else { c"fuzz.T2".as_ptr() }); }
                41 => {
                    if let Some(i) = pick(l, a) {
                        luaL_testudata(l, i, c"fuzz.T1".as_ptr());
                    }
                }
                42 => {
                    lua_stringtonumber(l, if a & 1 == 0 { c"0x10".as_ptr() } else { c"1e5".as_ptr() });
                }
                43 => {
                    if let Some(i) = pick(l, a) {
                        #[cfg(not(rustynail_lua54))]
                        {
                            let mut buf = [0 as c_char; 64];
                            lua_numbertocstring(l, i, buf.as_mut_ptr());
                        }
                        // (5.4 has no `lua_numbertocstring`: convert a copy)
                        #[cfg(rustynail_lua54)]
                        if lua_type(l, i) == LUA_TNUMBER && top(l) < 60 {
                            lua_pushvalue(l, i);
                            lua_tolstring(l, -1, std::ptr::null_mut());
                            lua_settop(l, -2);
                        }
                    }
                }
                44 => {
                    // resume a fresh coroutine running a snippet
                    let co = lua_newthread(l);
                    let s = "local a = coroutine.yield(1) return a";
                    luaL_loadbufferx(co, s.as_ptr().cast(), s.len(), c"=co".as_ptr(), std::ptr::null());
                    let mut nres = 0;
                    lua_resume(co, l, 0, &mut nres);
                    lua_pushinteger(co, 5);
                    lua_resume(co, l, 1, &mut nres);
                    lua_closethread(co, l);
                }
                45 => {
                    if top(l) >= 1 && top(l) < 60 {
                        let r = lua_checkstack(l, 100);
                        lua_pushboolean(l, r);
                    }
                }
                46 => { luaL_requiref(l, c"fuzzmod".as_ptr(), Some(dummy_cf), 0); }
                _ => {
                    luaL_openselectedlibs(l, 0x2 | 0x4, 0);
                }
            }
        }
        0
    }
}

fuzz_target!(|data: &[u8]| {
    rn_capi::ensure_init();
    unsafe {
        let l = luaL_newstate();
        if l.is_null() {
            return;
        }
        luaL_openselectedlibs(l, !0, 0);
        if data.first().is_some_and(|b| b & 1 != 0) {
            #[cfg(not(rustynail_lua54))]
            lua_gc(l, LUA_GCGEN);
            #[cfg(rustynail_lua54)]
            lua_gc(l, LUA_GCGEN, 0 as c_int, 0 as c_int);
        }
        let prog: (*const u8, usize) = (data.as_ptr().add(1.min(data.len())), data.len().saturating_sub(1));
        lua_pushcclosure(l, Some(runner), 0);
        lua_pushlightuserdata(l, &prog as *const _ as *mut c_void);
        lua_pcallk(l, 1, 0, 0, 0, None);
        lua_close(l);
    }
});
