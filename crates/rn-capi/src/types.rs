//! C types of `lua.h` / `lauxlib.h` (5.5, or 5.4 under `--cfg rustynail_lua54`) (layouts checked against the C headers by `tests/layout.c`).
use std::ffi::{c_char, c_int, c_void};

/// Opaque `lua_State`: a pointer to the thread object.
#[repr(C)]
pub struct lua_State {
    _private: [u8; 0],
}

pub type lua_Number = f64;
pub type lua_Integer = i64;
pub type lua_Unsigned = u64;
pub type lua_KContext = isize;

pub type lua_CFunction = Option<unsafe extern "C-unwind" fn(*mut lua_State) -> c_int>;
pub type lua_KFunction =
    Option<unsafe extern "C-unwind" fn(*mut lua_State, c_int, lua_KContext) -> c_int>;
pub type lua_Reader =
    Option<unsafe extern "C-unwind" fn(*mut lua_State, *mut c_void, *mut usize) -> *const c_char>;
pub type lua_Writer =
    Option<unsafe extern "C-unwind" fn(*mut lua_State, *const c_void, usize, *mut c_void) -> c_int>;
pub type lua_Alloc =
    Option<unsafe extern "C-unwind" fn(*mut c_void, *mut c_void, usize, usize) -> *mut c_void>;
pub type lua_WarnFunction = Option<unsafe extern "C-unwind" fn(*mut c_void, *const c_char, c_int)>;
pub type lua_Hook = Option<unsafe extern "C-unwind" fn(*mut lua_State, *mut lua_Debug)>;

pub const LUA_IDSIZE: usize = 60;
/// `LUA_VERSION_NUM` as a `lua_Number` (`lua_version`).
pub const LUA_VERSION_NUM: lua_Number = if cfg!(rustynail_lua54) { 504.0 } else { 505.0 };
pub const LUA_MULTRET: c_int = -1;
#[cfg(not(rustynail_lua54))]
pub const LUA_REGISTRYINDEX: c_int = -(c_int::MAX / 2 + 1000);
/// 5.4: `-LUAI_MAXSTACK - 1000`.
#[cfg(rustynail_lua54)]
pub const LUA_REGISTRYINDEX: c_int = -1_000_000 - 1000;
pub const LUA_OK: c_int = 0;
pub const LUA_YIELD: c_int = 1;
pub const LUA_ERRRUN: c_int = 2;
pub const LUA_ERRSYNTAX: c_int = 3;
pub const LUA_ERRMEM: c_int = 4;
pub const LUA_ERRERR: c_int = 5;
pub const LUA_ERRFILE: c_int = 6;
pub const LUA_TNONE: c_int = -1;
pub const LUA_TNIL: c_int = 0;
pub const LUA_TBOOLEAN: c_int = 1;
pub const LUA_TLIGHTUSERDATA: c_int = 2;
pub const LUA_TNUMBER: c_int = 3;
pub const LUA_TSTRING: c_int = 4;
pub const LUA_TTABLE: c_int = 5;
pub const LUA_TFUNCTION: c_int = 6;
pub const LUA_TUSERDATA: c_int = 7;
pub const LUA_TTHREAD: c_int = 8;
pub const LUA_NUMTYPES: c_int = 9;
pub const LUA_MINSTACK: c_int = 20;
pub const LUA_RIDX_GLOBALS: lua_Integer = rn_core::state::LUA_RIDX_GLOBALS;
pub const LUA_RIDX_MAINTHREAD: lua_Integer = rn_core::state::LUA_RIDX_MAINTHREAD;

pub const LUA_GCSTOP: c_int = 0;
pub const LUA_GCRESTART: c_int = 1;
pub const LUA_GCCOLLECT: c_int = 2;
pub const LUA_GCCOUNT: c_int = 3;
pub const LUA_GCCOUNTB: c_int = 4;
pub const LUA_GCSTEP: c_int = 5;
#[cfg(not(rustynail_lua54))]
pub const LUA_GCISRUNNING: c_int = 6;
#[cfg(not(rustynail_lua54))]
pub const LUA_GCGEN: c_int = 7;
#[cfg(not(rustynail_lua54))]
pub const LUA_GCINC: c_int = 8;
#[cfg(not(rustynail_lua54))]
pub const LUA_GCPARAM: c_int = 9;
#[cfg(rustynail_lua54)]
pub const LUA_GCSETPAUSE: c_int = 6;
#[cfg(rustynail_lua54)]
pub const LUA_GCSETSTEPMUL: c_int = 7;
#[cfg(rustynail_lua54)]
pub const LUA_GCISRUNNING: c_int = 9;
#[cfg(rustynail_lua54)]
pub const LUA_GCGEN: c_int = 10;
#[cfg(rustynail_lua54)]
pub const LUA_GCINC: c_int = 11;

pub const LUA_HOOKCALL: c_int = 0;
pub const LUA_HOOKRET: c_int = 1;
pub const LUA_HOOKLINE: c_int = 2;
pub const LUA_HOOKCOUNT: c_int = 3;
pub const LUA_HOOKTAILCALL: c_int = 4;

/// `lua_Debug` (`lua.h`).
#[repr(C)]
pub struct lua_Debug {
    pub event: c_int,
    pub name: *const c_char,
    pub namewhat: *const c_char,
    pub what: *const c_char,
    pub source: *const c_char,
    pub srclen: usize,
    pub currentline: c_int,
    pub linedefined: c_int,
    pub lastlinedefined: c_int,
    pub nups: u8,
    pub nparams: u8,
    pub isvararg: c_char,
    #[cfg(not(rustynail_lua54))]
    pub extraargs: u8,
    pub istailcall: c_char,
    #[cfg(not(rustynail_lua54))]
    pub ftransfer: c_int,
    #[cfg(not(rustynail_lua54))]
    pub ntransfer: c_int,
    /// 5.4: `unsigned short`.
    #[cfg(rustynail_lua54)]
    pub ftransfer: u16,
    #[cfg(rustynail_lua54)]
    pub ntransfer: u16,
    pub short_src: [c_char; LUA_IDSIZE],
    /// Private: the active `CallInfo`.
    pub i_ci: *mut c_void,
}

/// `luaL_Reg`.
#[repr(C)]
pub struct luaL_Reg {
    pub name: *const c_char,
    pub func: lua_CFunction,
}

/// `LUAL_BUFFERSIZE` = `16 * sizeof(void*) * sizeof(lua_Number)`.
pub const LUAL_BUFFERSIZE: usize = 16 * 8 * 8;

/// The inline initial buffer of `luaL_Buffer` (`LUAI_MAXALIGN` union: `long double`-aligned in
/// 5.5, 8-byte aligned in 5.4).
#[cfg_attr(not(rustynail_lua54), repr(C, align(16)))]
#[cfg_attr(rustynail_lua54, repr(C, align(8)))]
pub struct BufInit {
    pub b: [c_char; LUAL_BUFFERSIZE],
}

/// `luaL_Buffer`.
#[repr(C)]
pub struct luaL_Buffer {
    pub b: *mut c_char,
    pub size: usize,
    pub n: usize,
    pub L: *mut lua_State,
    pub init: BufInit,
}

/// `luaL_Stream` (file handles of the io library).
#[repr(C)]
pub struct luaL_Stream {
    pub f: *mut libc::FILE,
    pub closef: lua_CFunction,
}
