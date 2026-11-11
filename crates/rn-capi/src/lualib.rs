//! `lualib.h` / `linit.c`: the `luaopen_*` functions and `luaL_openselectedlibs`. The libraries
//! themselves are the Rust implementations of `rn-std`.
use crate::glue::*;
use crate::lapi::*;
use crate::lapi_get::*;
use crate::lauxlib::*;
use crate::types::*;
use std::ffi::{c_char, c_int};

macro_rules! luaopen {
    ($name:ident, $open:path) => {
        lua_api!($name(st) -> c_int {
            let r = $open(&mut st);
            ck(&mut st, r) as c_int
        });
    };
}

luaopen!(luaopen_base, rn_std::lib_base::open);
luaopen!(luaopen_package, rn_std::lib_package::open);
luaopen!(luaopen_coroutine, rn_std::lib_coroutine::open);
luaopen!(luaopen_debug, rn_std::lib_debug::open);
luaopen!(luaopen_io, rn_std::lib_io::open);
luaopen!(luaopen_math, rn_std::lib_math::open);
luaopen!(luaopen_os, rn_std::lib_os::open);
luaopen!(luaopen_string, rn_std::lib_string::open);
luaopen!(luaopen_table, rn_std::lib_table::open);
luaopen!(luaopen_utf8, rn_std::lib_utf8::open);

type Open = unsafe extern "C-unwind" fn(*mut lua_State) -> c_int;

/// The standard libraries in the order of the `LUA_<name>K` flags.
const STDLIBS: [(&std::ffi::CStr, Open); 10] = [
    (c"_G", luaopen_base),
    (c"package", luaopen_package),
    (c"coroutine", luaopen_coroutine),
    (c"debug", luaopen_debug),
    (c"io", luaopen_io),
    (c"math", luaopen_math),
    (c"os", luaopen_os),
    (c"string", luaopen_string),
    (c"table", luaopen_table),
    (c"utf8", luaopen_utf8),
];

#[no_mangle]
pub unsafe extern "C-unwind" fn luaL_openselectedlibs(
    l: *mut lua_State,
    load: c_int,
    preload: c_int,
) {
    // SAFETY: valid state.
    unsafe {
        luaL_getsubtable(l, LUA_REGISTRYINDEX, c"_PRELOAD".as_ptr());
        let mut mask: c_int = 1;
        for (name, f) in STDLIBS {
            if load & mask != 0 {
                luaL_requiref(l, name.as_ptr() as *const c_char, Some(f), 1);
                lua_settop(l, -2); // remove the result
            } else if preload & mask != 0 {
                lua_pushcclosure(l, Some(f), 0);
                lua_setfield(l, -2, name.as_ptr());
            }
            mask <<= 1;
        }
        lua_settop(l, -2); // remove the PRELOAD table
    }
}

/// `luaL_openlibs` (a macro in the 5.5 header; exported for source compatibility).
#[cfg(not(rustynail_lua54))]
#[no_mangle]
pub unsafe extern "C-unwind" fn luaL_openlibs(l: *mut lua_State) {
    // SAFETY: valid state.
    unsafe { luaL_openselectedlibs(l, !0, 0) }
}

/// 5.4 `luaL_openlibs` (`linit.c`): `luaL_requiref` every library, in `loadedlibs` order.
#[cfg(rustynail_lua54)]
#[no_mangle]
pub unsafe extern "C-unwind" fn luaL_openlibs(l: *mut lua_State) {
    const ORDER: [&std::ffi::CStr; 10] = [
        c"_G",
        c"package",
        c"coroutine",
        c"table",
        c"io",
        c"os",
        c"string",
        c"math",
        c"utf8",
        c"debug",
    ];
    // SAFETY: valid state.
    unsafe {
        for name in ORDER {
            let f = STDLIBS.iter().find(|(n, _)| *n == name).map(|(_, f)| *f);
            luaL_requiref(l, name.as_ptr(), f, 1);
            lua_settop(l, -2); // remove lib
        }
    }
}
