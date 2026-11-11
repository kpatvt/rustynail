//! `lua.h`, part 4: the debug interface.
use crate::glue::*;
use crate::types::*;
use rn_core::state::{CallInfo, Ci, State};
use rn_core::value::Value;
use std::collections::HashSet;
use std::ffi::{c_char, c_int, c_void};
use std::sync::{Mutex, OnceLock};

/// A NUL-terminated copy of `b` that lives forever (names handed out in `lua_Debug` and by
/// `lua_getlocal`/`lua_getupvalue` must stay valid; distinct names are few).
pub fn intern(b: &[u8]) -> *const c_char {
    static NAMES: OnceLock<Mutex<HashSet<Box<[u8]>>>> = OnceLock::new();
    let set = NAMES.get_or_init(|| Mutex::new(HashSet::new()));
    let mut g = set.lock().unwrap_or_else(|e| e.into_inner());
    let mut v = b.to_vec();
    v.push(0);
    if let Some(e) = g.get(v.as_slice()) {
        return e.as_ptr().cast();
    }
    let boxed: Box<[u8]> = v.into_boxed_slice();
    let p = boxed.as_ptr().cast();
    g.insert(boxed);
    p
}

lua_api!(lua_getstack(st, level: c_int, ar: *mut lua_Debug) -> c_int {
    if level < 0 {
        return 0;
    }
    match st.get_stack(level as usize) {
        Some(ci) => {
            (*ar).i_ci = ci.as_ptr().cast();
            1
        }
        None => 0,
    }
});

fn ci_of(ar: *const lua_Debug) -> Ci {
    // SAFETY: `i_ci` was set by `lua_getstack` / the hook trampoline from a live frame.
    unsafe { Ci::from_raw((*ar).i_ci.cast::<CallInfo>()) }
}

lua_api!(lua_getinfo(st, what: *const c_char, ar: *mut lua_Debug) -> c_int {
    let mut w = cstr(what);
    let (ci, func) = if w.first() == Some(&b'>') {
        // information about a function popped from the stack
        w = &w[1..];
        let f = st.pop();
        (None, f)
    } else {
        let ci = ci_of(ar);
        (Some(ci), st.get(ci.r().func))
    };
    let info = match ci {
        Some(ci) => st.get_info_ci(ci, w),
        None => st.get_info_func(func, w),
    };
    let Some(info) = info else { return 0 };
    let a = &mut *ar;
    for &c in w {
        match c {
            b'S' => {
                // the source of a Lua function is the string stored in its prototype
                a.source = match func.as_lclosure().and_then(|cl| cl.proto().source()) {
                    Some(s) => s.get().as_bytes().as_ptr().cast(),
                    None => intern(&info.source),
                };
                a.srclen = info.source.len();
                a.linedefined = info.linedefined;
                a.lastlinedefined = info.lastlinedefined;
                a.what = intern(info.what.as_bytes());
                let n = info.short_src.len().min(LUA_IDSIZE - 1);
                for (i, b) in info.short_src[..n].iter().enumerate() {
                    a.short_src[i] = *b as c_char;
                }
                a.short_src[n] = 0;
            }
            b'l' => a.currentline = info.currentline,
            b'u' => {
                a.nups = info.nups;
                a.nparams = info.nparams;
                a.isvararg = info.isvararg as c_char;
            }
            b't' => {
                // (5.4 stores the raw `ci->callstatus & CIST_TAIL` bit, 1 << 5)
                a.istailcall = (info.istailcall as c_char) << if cfg!(rustynail_lua54) { 5 } else { 0 };
                #[cfg(not(rustynail_lua54))]
                {
                    a.extraargs = info.extraargs;
                }
            }
            b'n' => {
                a.namewhat = intern(info.namewhat.as_bytes());
                a.name = match &info.name {
                    Some(n) => intern(n),
                    None => std::ptr::null(),
                };
            }
            b'r' => {
                a.ftransfer = info.ftransfer as _;
                a.ntransfer = info.ntransfer as _;
            }
            _ => {}
        }
    }
    if w.contains(&b'f') {
        st.push(func);
    }
    if w.contains(&b'L') {
        match &info.activelines {
            None => st.push(Value::NIL),
            Some(lines) => {
                let r = st.new_table();
                let t = ck(&mut st, r);
                for l in lines {
                    let r = st.raw_set_int(t, *l as i64, &Value::TRUE);
                    ck(&mut st, r);
                }
                st.push(Value::table(t));
            }
        }
    }
    1
});

lua_api!(lua_getlocal(st, ar: *const lua_Debug, n: c_int) -> *const c_char {
    if ar.is_null() {
        // information about a non-active function: only parameter names
        let f = st.get(st.top() - 1);
        return match f.as_lclosure().and_then(|cl| cl.proto().local_name(n, 0)) {
            Some(s) => s.get().as_bytes().as_ptr().cast(),
            None => std::ptr::null(),
        };
    }
    match st.find_local(ci_of(ar), n) {
        Some((name, pos)) => {
            let v = st.get(pos);
            st.push(v);
            intern(&name)
        }
        None => std::ptr::null(),
    }
});

lua_api!(lua_setlocal(st, ar: *const lua_Debug, n: c_int) -> *const c_char {
    match st.find_local(ci_of(ar), n) {
        Some((name, pos)) => {
            let v = st.get(st.top() - 1);
            st.set(pos, v);
            let t = st.top();
            st.set_top(t - 1);
            intern(&name)
        }
        None => std::ptr::null(),
    }
});

lua_api!(lua_getupvalue(st, funcindex: c_int, n: c_int) -> *const c_char {
    let f = val(&st, funcindex);
    if n < 1 {
        return std::ptr::null();
    }
    match st.get_upvalue(&f, n as usize) {
        Some((name, v)) => {
            st.push(v);
            intern(&name)
        }
        None => std::ptr::null(),
    }
});

lua_api!(lua_setupvalue(st, funcindex: c_int, n: c_int) -> *const c_char {
    let f = val(&st, funcindex);
    if n < 1 {
        return std::ptr::null();
    }
    let v = st.get(st.top() - 1);
    match st.set_upvalue_fn(&f, n as usize, v) {
        Some(name) => {
            let t = st.top();
            st.set_top(t - 1);
            intern(&name)
        }
        None => std::ptr::null(),
    }
});

lua_api!(lua_upvalueid(st, fidx: c_int, n: c_int) -> *mut c_void {
    let f = val(&st, fidx);
    if n < 1 {
        return std::ptr::null_mut();
    }
    st.upvalue_id(&f, n as usize).unwrap_or(std::ptr::null_mut())
});

lua_api!(lua_upvaluejoin(st, fidx1: c_int, n1: c_int, fidx2: c_int, n2: c_int) {
    let f1 = val(&st, fidx1);
    let f2 = val(&st, fidx2);
    st.upvalue_join(&f1, n1 as usize, &f2, n2 as usize);
});

lua_api!(lua_sethook(st, func: lua_Hook, mask: c_int, count: c_int) {
    match func {
        Some(f) if mask != 0 => st.set_c_hook(Some(c_hook_fn()), f as *const (), mask as u8, count),
        _ => st.set_c_hook(None, std::ptr::null(), 0, 0),
    }
});

lua_api!(lua_gethook(st) -> lua_Hook {
    let p = st.c_hook();
    if p.is_null() {
        None
    } else {
        Some(std::mem::transmute::<*const (), unsafe extern "C-unwind" fn(*mut lua_State, *mut lua_Debug)>(p))
    }
});

lua_api!(lua_gethookmask(st) -> c_int { st.hook_mask() as c_int });
lua_api!(lua_gethookcount(st) -> c_int { st.hook_count() });

#[allow(dead_code)]
fn _unused(_: &State) {}
