//! `lua.h`, part 1: state management, basic stack manipulation, access functions, comparison and
//! arithmetic, and the push functions.
use crate::glue::*;
use crate::types::*;
use rn_core::alloc::Allocator;
use rn_core::arith;
use rn_core::meta::Tm;
use rn_core::number::{self, Number, F2I};
use rn_core::state::State;
use rn_core::status::Status;
use rn_core::value::{Gc, LuaType, Value};
use std::ffi::{c_char, c_int, c_uint, c_void};

// ---------------------------------------------------------------------------------------------
// state management
// ---------------------------------------------------------------------------------------------

/// Shared, replaceable allocator description (`lua_setallocf`).
pub struct AllocCtl {
    pub f: lua_Alloc,
    pub ud: *mut c_void,
}

struct CAlloc {
    ctl: *mut AllocCtl,
}

impl Allocator for CAlloc {
    unsafe fn realloc(&mut self, ptr: *mut u8, old: usize, new: usize) -> *mut u8 {
        // SAFETY: `ctl` is a leaked box freed by `lua_close` after the heap is gone.
        let ctl = unsafe { &*self.ctl };
        match ctl.f {
            // SAFETY: calling the user's allocator with the `lua_Alloc` contract.
            Some(f) => unsafe { f(ctl.ud, ptr.cast(), old, new).cast() },
            None => std::ptr::null_mut(),
        }
    }
}

/// `lua_newstate(f, ud)` (5.4: no seed argument; the seed is `luai_makeseed`'s).
#[cfg(rustynail_lua54)]
#[no_mangle]
pub unsafe extern "C-unwind" fn lua_newstate(f: lua_Alloc, ud: *mut c_void) -> *mut lua_State {
    // SAFETY: same contract.
    unsafe { newstate(f, ud, rn_core::auxlib::make_seed() as c_uint) }
}

#[cfg(not(rustynail_lua54))]
#[no_mangle]
pub unsafe extern "C-unwind" fn lua_newstate(
    f: lua_Alloc,
    ud: *mut c_void,
    seed: c_uint,
) -> *mut lua_State {
    // SAFETY: same contract.
    unsafe { newstate(f, ud, seed) }
}

unsafe fn newstate(f: lua_Alloc, ud: *mut c_void, seed: c_uint) -> *mut lua_State {
    ensure_init();
    if f.is_none() {
        return std::ptr::null_mut();
    }
    let ctl = Box::into_raw(Box::new(AllocCtl { f, ud }));
    let ls = match State::new(Box::new(CAlloc { ctl }), seed) {
        Ok(ls) => ls,
        Err(_) => {
            // SAFETY: freshly leaked box.
            drop(unsafe { Box::from_raw(ctl) });
            return std::ptr::null_mut();
        }
    };
    let mut ls = Box::new(ls);
    ls.set_loader(rn_std::loader);
    let l = lptr(&ls);
    let owner = Box::into_raw(ls);
    // SAFETY: `owner` is a fresh box; the state outlives this call.
    unsafe {
        let g = (*owner).g_mut();
        g.capi.owner = owner;
        g.capi.allocf = ctl as *const ();
    }
    l
}

lua_api!(lua_close(st) {
    let owner = st.g().capi.owner;
    let ctl = st.g().capi.allocf as *mut AllocCtl;
    if owner.is_null() {
        return;
    }
    // lua_close always works on the main thread; dropping the owner closes the state
    // SAFETY: `owner` came from `Box::into_raw` in `lua_newstate`; freed exactly once.
    drop(Box::from_raw(owner));
    drop(Box::from_raw(ctl));
});

lua_api!(lua_newthread(st) -> *mut lua_State {
    let r = st.new_thread();
    let th = ck(&mut st, r);
    th.as_ptr().cast()
});

lua_api!(lua_closethread(st, from: *mut lua_State) -> c_int {
    let f = if from.is_null() { None } else { Some(state(from)) };
    let s = st.close_thread(f.as_ref());
    s as c_int
});

lua_api!(lua_atpanic(st, panicf: lua_CFunction) -> lua_CFunction {
    let old = st.g().capi.panic;
    st.g_mut().capi.panic = match panicf { Some(f) => f as *const (), None => std::ptr::null() };
    if old.is_null() { None } else { Some(std::mem::transmute::<*const (), unsafe extern "C-unwind" fn(*mut lua_State) -> c_int>(old)) }
});

lua_api!(lua_version(_st) -> lua_Number { LUA_VERSION_NUM });

/// 5.4 `lua_resetthread` (deprecated): `lua_closethread(L, NULL)`.
#[cfg(rustynail_lua54)]
#[no_mangle]
pub unsafe extern "C-unwind" fn lua_resetthread(l: *mut lua_State) -> c_int {
    // SAFETY: same contract.
    unsafe { lua_closethread(l, std::ptr::null_mut()) }
}

/// 5.4 `lua_setcstacklimit`: obsolete, returns `LUAI_MAXCCALLS`.
#[cfg(rustynail_lua54)]
#[no_mangle]
pub unsafe extern "C-unwind" fn lua_setcstacklimit(_l: *mut lua_State, _limit: c_uint) -> c_int {
    rn_core::state::LUAI_MAXCCALLS as c_int
}

// ---------------------------------------------------------------------------------------------
// basic stack manipulation
// ---------------------------------------------------------------------------------------------

lua_api!(lua_absindex(st, idx: c_int) -> c_int {
    if idx > 0 || idx <= LUA_REGISTRYINDEX {
        idx
    } else {
        (st.top() - st.ci().r().func) as c_int + idx
    }
});

lua_api!(lua_gettop(st) -> c_int { (st.top() - (st.ci().r().func + 1)) as c_int });

lua_api!(lua_settop(st, idx: c_int) {
    let func = st.ci().r().func;
    let top = st.top();
    let newtop = if idx >= 0 {
        let nt = func + 1 + idx as usize;
        let mut t = top;
        while t < nt {
            st.set(t, Value::NIL);
            t += 1;
        }
        nt
    } else {
        top - ((-(idx + 1)) as usize)
    };
    if newtop < top && st.tbc_count() > 0 {
        // close to-be-closed variables of the removed slots
        let r = st.close(newtop, rn_core::func::CLOSEKTOP, false);
        let nt = ck(&mut st, r);
        st.set_top(nt);
    } else {
        st.set_top(newtop);
    }
});

lua_api!(lua_pushvalue(st, idx: c_int) {
    let v = val(&st, idx);
    st.push(v);
});

lua_api!(lua_rotate(st, idx: c_int, n: c_int) {
    let t = st.top() - 1; // end of stack segment being rotated
    let p = stack_slot(&st, idx); // start of segment
    let m = if n >= 0 { t.0 - n as usize } else { p.0 + (-n) as usize - 1 }; // end of prefix
    fn reverse(st: &mut State, mut from: usize, mut to: usize) {
        while from < to {
            let a = st.get(rn_core::state::StkId(from));
            let b = st.get(rn_core::state::StkId(to));
            st.set(rn_core::state::StkId(from), b);
            st.set(rn_core::state::StkId(to), a);
            from += 1;
            to -= 1;
        }
    }
    reverse(&mut st, p.0, m);
    reverse(&mut st, m + 1, t.0);
    reverse(&mut st, p.0, t.0);
});

lua_api!(lua_copy(st, fromidx: c_int, toidx: c_int) {
    let v = val(&st, fromidx);
    set_idx(&mut st, toidx, v);
});

lua_api!(lua_checkstack(st, n: c_int) -> c_int { st.check_stack_api(n.max(0) as usize) as c_int });

#[no_mangle]
pub unsafe extern "C-unwind" fn lua_xmove(from: *mut lua_State, to: *mut lua_State, n: c_int) {
    // SAFETY: both are valid threads of the same state.
    unsafe {
        let mut f = state(from);
        let mut t = state(to);
        f.xmove(&mut t, n as usize);
    }
}

// ---------------------------------------------------------------------------------------------
// access functions (stack -> C)
// ---------------------------------------------------------------------------------------------

fn tonumber_val(v: &Value) -> Option<f64> {
    arith::to_float(v)
}

lua_api!(lua_isnumber(st, idx: c_int) -> c_int { tonumber_val(&val(&st, idx)).is_some() as c_int });

lua_api!(lua_isstring(st, idx: c_int) -> c_int {
    let v = val(&st, idx);
    (v.is_string() || v.is_number()) as c_int
});

lua_api!(lua_iscfunction(st, idx: c_int) -> c_int {
    let v = val(&st, idx);
    (v.is_light_cfunction() || v.is_cclosure()) as c_int
});

lua_api!(lua_isinteger(st, idx: c_int) -> c_int { val(&st, idx).is_int() as c_int });

lua_api!(lua_isuserdata(st, idx: c_int) -> c_int {
    let v = val(&st, idx);
    (v.is_userdata() || v.is_light_userdata()) as c_int
});

lua_api!(lua_type(st, idx: c_int) -> c_int {
    match index2value(&st, idx) {
        Some(v) if !v.is_empty() || v.is_nil() => v.type_tag() as c_int,
        _ => LUA_TNONE,
    }
});

const TYPENAMES: [&[u8]; 10] = [
    b"no value\0",
    b"nil\0",
    b"boolean\0",
    b"userdata\0",
    b"number\0",
    b"string\0",
    b"table\0",
    b"function\0",
    b"userdata\0",
    b"thread\0",
];

lua_api!(lua_typename(_st, t: c_int) -> *const c_char {
    TYPENAMES[(t + 1).clamp(0, 9) as usize].as_ptr().cast()
});

lua_api!(lua_tonumberx(st, idx: c_int, isnum: *mut c_int) -> lua_Number {
    let r = tonumber_val(&val(&st, idx));
    if !isnum.is_null() {
        *isnum = r.is_some() as c_int;
    }
    r.unwrap_or(0.0)
});

lua_api!(lua_tointegerx(st, idx: c_int, isnum: *mut c_int) -> lua_Integer {
    let r = arith::to_integer(&val(&st, idx), F2I::Eq);
    if !isnum.is_null() {
        *isnum = r.is_some() as c_int;
    }
    r.unwrap_or(0)
});

lua_api!(lua_toboolean(st, idx: c_int) -> c_int { val(&st, idx).is_truthy() as c_int });

lua_api!(lua_tolstring(st, idx: c_int, len: *mut usize) -> *const c_char {
    let mut v = val(&st, idx);
    if !v.is_string() {
        if !v.is_number() {
            if !len.is_null() {
                *len = 0;
            }
            return std::ptr::null();
        }
        // convert the number in place
        let n = v.as_number().expect("number");
        let b = number::tostring(n);
        let r = st.new_str_value(b.as_bytes());
        v = ck(&mut st, r);
        set_idx(&mut st, idx, v);
        st.check_gc();
    }
    let s = v.as_string().expect("string");
    if !len.is_null() {
        *len = s.as_bytes().len();
    }
    s.as_bytes().as_ptr().cast()
});

lua_api!(lua_rawlen(st, idx: c_int) -> lua_Unsigned {
    let v = val(&st, idx);
    if let Some(s) = v.as_string() {
        s.as_bytes().len() as u64
    } else if let Some(u) = v.as_userdata() {
        u.len() as u64
    } else if v.is_table() {
        st.raw_len(&v)
    } else {
        0
    }
});

lua_api!(lua_tocfunction(st, idx: c_int) -> lua_CFunction {
    let v = val(&st, idx);
    let p = match v.as_light_cfn() {
        Some(p) => p,
        None => v.as_cclosure().map_or(std::ptr::null(), |c| c.cfn()),
    };
    if p.is_null() {
        None
    } else {
        Some(std::mem::transmute::<*const (), unsafe extern "C-unwind" fn(*mut lua_State) -> c_int>(p))
    }
});

lua_api!(lua_touserdata(st, idx: c_int) -> *mut c_void {
    let v = val(&st, idx);
    if let Some(u) = v.as_userdata() {
        u.data_ptr().cast()
    } else if let Some(p) = v.as_light_userdata() {
        p
    } else {
        std::ptr::null_mut()
    }
});

lua_api!(lua_tothread(st, idx: c_int) -> *mut lua_State {
    val(&st, idx).as_thread().map_or(std::ptr::null_mut(), |t| t.as_ptr().cast())
});

lua_api!(lua_topointer(st, idx: c_int) -> *const c_void {
    let v = val(&st, idx);
    if let Some(u) = v.as_userdata() {
        return u.data_ptr().cast();
    }
    st.value_pointer(&v) as *const c_void
});

// ---------------------------------------------------------------------------------------------
// comparison and arithmetic
// ---------------------------------------------------------------------------------------------

pub const LUA_OPADD: c_int = 0;
pub const LUA_OPUNM: c_int = 12;
pub const LUA_OPBNOT: c_int = 13;

/// `luaO_rawarith`: arithmetic on numbers only (numeric strings are *not* coerced here, the string
/// metamethods do that), `None` if the operands do not qualify.
fn raw_arith(op: c_int, a: &Value, b: &Value) -> Option<Result<Value, ()>> {
    use number::{fidiv, fmod_lua, idiv, imod, pow_lua, shiftl, shiftr};
    match op {
        7..=11 | 13 => {
            // bitwise: integers (or floats with exact integer value)
            let x = arith::to_integer_ns(a, F2I::Eq)?;
            let y = arith::to_integer_ns(b, F2I::Eq)?;
            Some(Ok(Value::int(match op {
                7 => x & y,
                8 => x | y,
                9 => x ^ y,
                10 => shiftl(x, y),
                11 => shiftr(x, y),
                _ => !x,
            })))
        }
        4 | 5 => {
            // pow, div: always floats
            let x = a.as_number().map(num_f)?;
            let y = b.as_number().map(num_f)?;
            Some(Ok(Value::float(if op == 4 {
                pow_lua(x, y)
            } else {
                x / y
            })))
        }
        0..=3 | 6 | 12 => {
            if let (Some(x), Some(y)) = (a.as_int(), b.as_int()) {
                // integer arithmetic
                let r = match op {
                    0 => x.wrapping_add(y),
                    1 => x.wrapping_sub(y),
                    2 => x.wrapping_mul(y),
                    3 => match imod(x, y) {
                        Some(r) => r,
                        None => return Some(Err(())),
                    },
                    6 => match idiv(x, y) {
                        Some(r) => r,
                        None => return Some(Err(())),
                    },
                    _ => x.wrapping_neg(),
                };
                return Some(Ok(Value::int(r)));
            }
            let x = a.as_number().map(num_f)?;
            let y = b.as_number().map(num_f)?;
            Some(Ok(Value::float(match op {
                0 => x + y,
                1 => x - y,
                2 => x * y,
                3 => fmod_lua(x, y),
                6 => fidiv(x, y),
                _ => -x,
            })))
        }
        _ => None,
    }
}

fn num_f(n: Number) -> f64 {
    match n {
        Number::Int(i) => i as f64,
        Number::Float(f) => f,
    }
}

/// `lua_arith` event for an operation code.
fn arith_event(op: c_int) -> Tm {
    const EVENTS: [Tm; 14] = [
        Tm::Add,
        Tm::Sub,
        Tm::Mul,
        Tm::Mod,
        Tm::Pow,
        Tm::Div,
        Tm::IDiv,
        Tm::BAnd,
        Tm::BOr,
        Tm::BXor,
        Tm::Shl,
        Tm::Shr,
        Tm::Unm,
        Tm::BNot,
    ];
    EVENTS[op as usize]
}

lua_api!(lua_arith(st, op: c_int) {
    if op != LUA_OPUNM && op != LUA_OPBNOT {
        // binary operation: both operands are on the stack
    } else {
        // unary: the operand is duplicated as the second operand
        let v = st.get(st.top() - 1);
        st.push(v);
    }
    let top = st.top();
    let (a, b) = (st.get(top - 2), st.get(top - 1));
    let res = match raw_arith(op, &a, &b) {
        Some(Ok(v)) => v,
        Some(Err(())) => {
            let msg: &[u8] = if op == 3 { b"attempt to perform 'n%0'" } else { b"attempt to divide by zero" };
            let e = st.run_error(msg);
            raise(&mut st, e)
        }
        None => {
            let r = st.try_bin_tm(a.into(), b.into(), top - 2, arith_event(op));
            ck(&mut st, r);
            st.set_top(top - 1);
            return;
        }
    };
    st.set(top - 2, res);
    st.set_top(top - 1);
});

lua_api!(lua_rawequal(st, idx1: c_int, idx2: c_int) -> c_int {
    match (index2value(&st, idx1), index2value(&st, idx2)) {
        (Some(a), Some(b)) => a.raw_equal(&b) as c_int,
        _ => 0,
    }
});

lua_api!(lua_compare(st, idx1: c_int, idx2: c_int, op: c_int) -> c_int {
    match (index2value(&st, idx1), index2value(&st, idx2)) {
        (Some(a), Some(b)) => {
            let r = match op {
                0 => st.equals(a, b),
                1 => st.compare_lt_le(a, b, false),
                2 => st.compare_lt_le(a, b, true),
                _ => Ok(false),
            };
            ck(&mut st, r) as c_int
        }
        _ => 0,
    }
});

// ---------------------------------------------------------------------------------------------
// push functions (C -> stack)
// ---------------------------------------------------------------------------------------------

lua_api!(lua_pushnil(st) { st.push(Value::NIL) });
lua_api!(lua_pushnumber(st, n: lua_Number) { st.push(Value::float(n)) });
lua_api!(lua_pushinteger(st, n: lua_Integer) { st.push(Value::int(n)) });
lua_api!(lua_pushboolean(st, b: c_int) { st.push(Value::boolean(b != 0)) });
lua_api!(lua_pushlightuserdata(st, p: *mut c_void) { st.push(Value::light_userdata(p)) });

lua_api!(lua_pushlstring(st, s: *const c_char, len: usize) -> *const c_char {
    let bytes: &[u8] = if len == 0 { &[] } else { std::slice::from_raw_parts(s.cast(), len) };
    let r = st.new_str_value(bytes);
    let v = ck(&mut st, r);
    st.push(v);
    let p = v.as_string().expect("string").as_bytes().as_ptr().cast();
    st.check_gc();
    p
});

lua_api!(lua_pushstring(st, s: *const c_char) -> *const c_char {
    if s.is_null() {
        st.push(Value::NIL);
        return std::ptr::null();
    }
    let bytes = cstr(s);
    let r = st.new_str_value(bytes);
    let v = ck(&mut st, r);
    st.push(v);
    let p = v.as_string().expect("string").as_bytes().as_ptr().cast();
    st.check_gc();
    p
});

lua_api!(#[cfg(not(rustynail_lua54))] lua_pushexternalstring(
    st, s: *const c_char, len: usize, falloc: lua_Alloc, ud: *mut c_void
) -> *const c_char {
    // the free callback has the `lua_Alloc` signature with `osize = len + 1`, `nsize = 0`
    let ef: Option<rn_core::string::ExtFree> =
        falloc.map(|f| std::mem::transmute::<_, rn_core::string::ExtFree>(f));
    let r = st.heap().new_external_str(s.cast(), len, ef, ud);
    let sref = ck(&mut st, r);
    let v = Value::string(sref);
    st.push(v);
    st.check_gc();
    s
});

lua_api!(lua_pushcclosure(st, f: lua_CFunction, n: c_int) {
    ensure_init();
    let f = f.expect("lua_pushcclosure: null function") as *const ();
    if n == 0 {
        st.push(Value::light_cfunction(f));
        return;
    }
    let r = st.try_alloc(|h| h.new_cclosure(c_closure_fn(), n as usize));
    let cl = ck(&mut st, r);
    cl.set_cfn(f);
    let top = st.top();
    for i in 0..n as usize {
        let v = st.get(top - n as usize + i);
        cl.set_upvalue(st.heap(), i, v);
    }
    st.set_top(top - n as usize);
    st.push(Value::cclosure(cl));
    st.check_gc();
});

lua_api!(lua_pushthread(st) -> c_int {
    let v = Value::thread(st.as_gc());
    st.push(v);
    st.is_main_thread() as c_int
});

lua_api!(#[cfg(not(rustynail_lua54))] lua_numbertocstring(st, idx: c_int, buff: *mut c_char) -> c_uint {
    let v = val(&st, idx);
    match v.as_number() {
        Some(n) => {
            let b = number::tostring(n);
            let bytes = b.as_bytes();
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), buff.cast::<u8>(), bytes.len());
            *buff.add(bytes.len()) = 0;
            (bytes.len() + 1) as c_uint
        }
        None => 0,
    }
});

lua_api!(lua_stringtonumber(st, s: *const c_char) -> usize {
    let bytes = cstr(s);
    match number::str2num(bytes) {
        Some(n) => {
            st.push(Value::number(n));
            bytes.len() + 1
        }
        None => 0,
    }
});

lua_api!(lua_getallocf(st, ud: *mut *mut c_void) -> lua_Alloc {
    let ctl = st.g().capi.allocf as *const AllocCtl;
    if ctl.is_null() {
        // a state not created through this API (e.g. by the Rust CLI): the C allocator
        if !ud.is_null() {
            *ud = std::ptr::null_mut();
        }
        return Some(crate::lauxlib::luaL_alloc);
    }
    if !ud.is_null() {
        *ud = (*ctl).ud;
    }
    (*ctl).f
});

lua_api!(lua_setallocf(st, f: lua_Alloc, ud: *mut c_void) {
    let ctl = st.g().capi.allocf as *mut AllocCtl;
    if !ctl.is_null() {
        (*ctl).f = f;
        (*ctl).ud = ud;
    }
});

#[allow(dead_code)]
fn _unused(_: LuaType, _: Gc<rn_core::state::Thread>, _: Status) {}
