//! Basic library (port of `lbaselib.c`).
#![forbid(unsafe_code)]

use rn_core::number::{self, Number};
use rn_core::state::{State, StkId, LUA_MULTRET};
use rn_core::status::{LuaResult, Status};
use rn_core::value::{LuaType, Value};

#[cfg(not(rustynail_lua54))]
const LUA_VERSION: &[u8] = b"Lua 5.5";
#[cfg(rustynail_lua54)]
const LUA_VERSION: &[u8] = b"Lua 5.4";

// ---------------------------------------------------------------------------------------------
// print / warn
// ---------------------------------------------------------------------------------------------

fn base_print(st: &mut State) -> LuaResult<usize> {
    let n = st.nargs();
    let mut out: Vec<u8> = Vec::with_capacity(64);
    for i in 1..=n {
        let a = st.arg(i - 1);
        if i > 1 {
            out.push(b'\t');
        }
        // (`luaL_tolstring` honours a `__tostring` in the string metatable too)
        if let Some(s) = a
            .as_string()
            .filter(|_| st.tostring_metafield(&a).is_nil())
        {
            out.extend_from_slice(s.as_bytes());
        } else if let Some(n) = a.as_number().filter(|_| st.metatable_of(&a).is_none()) {
            rn_core::number::with_tostring(n, |b| out.extend_from_slice(b));
        } else {
            let s = st.tolstring(a)?;
            out.extend_from_slice(s.get().as_bytes());
            st.pop();
        }
    }
    out.push(b'\n');
    crate::io_ffi::stdout_write(&out);
    Ok(0)
}

fn base_warn(st: &mut State) -> LuaResult<usize> {
    let n = st.nargs();
    st.check_str(1)?;
    for i in 2..=n {
        st.check_str(i)?;
    }
    for i in 1..=n {
        let s = st.check_str(i)?;
        st.warning(s.get().as_bytes(), i < n);
    }
    Ok(0)
}

// ---------------------------------------------------------------------------------------------
// tonumber / tostring / type
// ---------------------------------------------------------------------------------------------

const SPACECHARS: &[u8] = b" \x0c\n\r\t\x0b";

fn str2int_base(s: &[u8], base: u32) -> Option<i64> {
    let mut i = 0;
    while i < s.len() && SPACECHARS.contains(&s[i]) {
        i += 1;
    }
    let mut neg = false;
    if i < s.len() && s[i] == b'-' {
        i += 1;
        neg = true;
    } else if i < s.len() && s[i] == b'+' {
        i += 1;
    }
    if i >= s.len() || !s[i].is_ascii_alphanumeric() {
        return None;
    }
    let mut n: u64 = 0;
    while i < s.len() && s[i].is_ascii_alphanumeric() {
        let c = s[i];
        let digit = if c.is_ascii_digit() {
            (c - b'0') as u32
        } else {
            (c.to_ascii_uppercase() - b'A') as u32 + 10
        };
        if digit >= base {
            return None;
        }
        n = n.wrapping_mul(base as u64).wrapping_add(digit as u64);
        i += 1;
    }
    while i < s.len() && SPACECHARS.contains(&s[i]) {
        i += 1;
    }
    if i != s.len() {
        return None;
    }
    Some(if neg {
        0u64.wrapping_sub(n) as i64
    } else {
        n as i64
    })
}

fn base_tonumber(st: &mut State) -> LuaResult<usize> {
    if st.arg_is_none_or_nil(2) {
        let a = st.arg(0);
        if a.is_number() {
            st.push(a);
            return Ok(1);
        }
        if let Some(s) = a.as_string() {
            if let Some(n) = number::str2num(s.as_bytes()) {
                st.push(Value::number(n));
                return Ok(1);
            }
        }
        st.check_any(1)?;
    } else {
        let base = st.check_integer(2)?;
        let a = st.check_type(1, LuaType::String)?;
        st.arg_check((2..=36).contains(&base), 2, b"base out of range")?;
        if let Some(n) = str2int_base(a.as_string().map_or(&[][..], |s| s.as_bytes()), base as u32)
        {
            st.push(Value::int(n));
            return Ok(1);
        }
    }
    st.push(Value::NIL);
    Ok(1)
}

fn base_tostring(st: &mut State) -> LuaResult<usize> {
    let v = st.check_any(1)?;
    st.tolstring(v)?;
    Ok(1)
}

/// `type`: native closure whose 9 upvalues are the type names (indexed by `LuaType`).
fn base_type(st: &mut State) -> LuaResult<usize> {
    if st.nargs() == 0 {
        return Err(st.arg_error(1, b"value expected"));
    }
    let t = st.arg(0).type_of() as usize;
    let f = st.get(st.ci().r().func);
    match f.as_cclosure() {
        Some(c) => st.push(c.upvalue(t)),
        None => unreachable!("type is a closure"),
    }
    Ok(1)
}

// ---------------------------------------------------------------------------------------------
// error / metatables / raw access
// ---------------------------------------------------------------------------------------------

fn do_error(st: &mut State, level: i64, msg: Value) -> Status {
    if msg.is_string() && level > 0 {
        match st.new_where_str(level as usize, msg.as_string().map_or(&[][..], |s| s.as_bytes())) {
            Ok(v) => st.throw_value(v),
            Err(e) => e,
        }
    } else {
        st.throw_value(msg)
    }
}

fn base_error(st: &mut State) -> LuaResult<usize> {
    let level = st.opt_integer(2, 1)?;
    let msg = st.arg(0);
    Err(do_error(st, level, msg))
}

fn base_assert(st: &mut State) -> LuaResult<usize> {
    if st.arg(0).is_truthy() {
        return Ok(st.nargs());
    }
    st.check_any(1)?;
    let msg = if st.nargs() >= 2 {
        st.arg(1)
    } else {
        st.new_str_value(b"assertion failed!")?
    };
    // `luaB_error` with only the message on the stack: level 1
    Err(do_error(st, 1, msg))
}

fn base_getmetatable(st: &mut State) -> LuaResult<usize> {
    let v = st.check_any(1)?;
    match st.metatable_of(&v) {
        None => st.push(Value::NIL),
        Some(mt) => {
            let f = st.get_metafield(&v, b"__metatable");
            if f.is_nil() {
                st.push(Value::table(mt));
            } else {
                st.push(f);
            }
        }
    }
    Ok(1)
}

fn base_setmetatable(st: &mut State) -> LuaResult<usize> {
    let t = st.check_type(1, LuaType::Table)?;
    let a2 = st.arg(1);
    if st.nargs() < 2 || !(a2.is_nil() || a2.is_table()) {
        return Err(st.type_error_arg(2, b"nil or table"));
    }
    if !st.get_metafield(&t, b"__metatable").is_nil() {
        return Err(st.l_error(b"cannot change a protected metatable"));
    }
    st.set_metatable(&t, a2.as_table());
    st.push(t);
    Ok(1)
}

fn base_rawequal(st: &mut State) -> LuaResult<usize> {
    let a = st.check_any(1)?;
    let b = st.check_any(2)?;
    st.push(Value::boolean(a.raw_equal(&b)));
    Ok(1)
}

fn base_rawlen(st: &mut State) -> LuaResult<usize> {
    let v = st.arg(0);
    if st.nargs() < 1 || !(v.is_table() || v.is_string()) {
        return Err(st.type_error_arg(1, b"table or string"));
    }
    let n = st.raw_len(&v);
    st.push(Value::int(n as i64));
    Ok(1)
}

fn base_rawget(st: &mut State) -> LuaResult<usize> {
    let t = st.check_table(1)?;
    let k = st.check_any(2)?;
    st.push(t.get(&k));
    Ok(1)
}

fn base_rawset(st: &mut State) -> LuaResult<usize> {
    let t = st.check_table(1)?;
    let k = st.check_any(2)?;
    let v = st.check_any(3)?;
    st.raw_set(t, &k, &v)?;
    st.push(Value::table(t));
    Ok(1)
}

// ---------------------------------------------------------------------------------------------
// collectgarbage
// ---------------------------------------------------------------------------------------------

/// Parameter names, in `LUA_GCP*` index order.
#[cfg(not(rustynail_lua54))]
const GC_PARAMS: &[&str] = &[
    "minormul",
    "majorminor",
    "minormajor",
    "pause",
    "stepmul",
    "stepsize",
];

/// `pushmode` (5.4 `LUA_GCGEN`/`LUA_GCINC`): switch modes, push the name of the previous one.
#[cfg(rustynail_lua54)]
fn gc_mode_swap(st: &mut State, generational: bool) -> LuaResult<usize> {
    use rn_core::gc::GcKind;
    let new = if generational {
        GcKind::GenMinor
    } else {
        GcKind::Incremental
    };
    let old = st.gc_change_mode(new);
    st.push_bytes(if old.is_generational() {
        b"generational"
    } else {
        b"incremental"
    })?;
    Ok(1)
}

fn base_collectgarbage(st: &mut State) -> LuaResult<usize> {
    #[cfg(not(rustynail_lua54))]
    const OPTS: &[&str] = &[
        "stop",
        "restart",
        "collect",
        "count",
        "step",
        "isrunning",
        "generational",
        "incremental",
        "param",
    ];
    #[cfg(rustynail_lua54)]
    const OPTS: &[&str] = &[
        "stop",
        "restart",
        "collect",
        "count",
        "step",
        "setpause",
        "setstepmul",
        "isrunning",
        "generational",
        "incremental",
    ];
    let o = OPTS[st.check_option(1, Some("collect"), OPTS)?];
    if st.heap().gc_internally_stopped() {
        // `lua_gc` returns -1 inside a finalizer: the call fails (`luaL_pushfail`)
        st.push(Value::NIL);
        return Ok(1);
    }
    match o {
        "stop" => {
            st.heap().gc_stop();
            st.push(Value::int(0));
        }
        "restart" => {
            st.heap().gc_restart();
            st.push(Value::int(0));
        }
        "collect" => {
            st.collect(false);
            st.push(Value::int(0));
        }
        "count" => {
            let b = st.heap().total_bytes();
            st.push(Value::float(b as f64 / 1024.0));
        }
        "step" => {
            let n = st.opt_integer(2, 0)?;
            #[cfg(not(rustynail_lua54))]
            {
                let ended = st.gc_api_step(n.max(0) as usize);
                st.push(Value::boolean(ended));
            }
            // 5.4: the argument is in Kbytes (`data * 1024` added to the debt)
            #[cfg(rustynail_lua54)]
            {
                let ended = st.gc_api_step54(n as i32);
                st.push(Value::boolean(ended));
            }
        }
        "isrunning" => {
            let r = st.heap().gc_is_running();
            st.push(Value::boolean(r));
        }
        #[cfg(not(rustynail_lua54))]
        "generational" | "incremental" => {
            use rn_core::gc::GcKind;
            let new = if o == "generational" {
                GcKind::GenMinor
            } else {
                GcKind::Incremental
            };
            let old = st.gc_change_mode(new);
            st.push_bytes(if old.is_generational() {
                b"generational"
            } else {
                b"incremental"
            })?;
        }
        #[cfg(rustynail_lua54)]
        "generational" => {
            let minormul = st.opt_integer(2, 0)? as i32 as i64;
            let majormul = st.opt_integer(3, 0)? as i32 as i64;
            let p = st.heap().gc_params();
            if minormul != 0 {
                p.set_genminormul54(minormul);
            }
            if majormul != 0 {
                p.set_genmajormul54(majormul);
            }
            return gc_mode_swap(st, true);
        }
        #[cfg(rustynail_lua54)]
        "incremental" => {
            let pause = st.opt_integer(2, 0)? as i32 as i64;
            let stepmul = st.opt_integer(3, 0)? as i32 as i64;
            let stepsize = st.opt_integer(4, 0)? as i32 as i64;
            let p = st.heap().gc_params();
            if pause != 0 {
                p.set_pause54(pause);
            }
            if stepmul != 0 {
                p.set_stepmul54(stepmul);
            }
            if stepsize != 0 {
                p.set_stepsize54(stepsize);
            }
            return gc_mode_swap(st, false);
        }
        #[cfg(not(rustynail_lua54))]
        "param" => {
            let idx = st.check_option(2, None, GC_PARAMS)?;
            let value = st.opt_integer(3, -1)?;
            let old = st.heap().gc_params().get(idx);
            if value >= 0 {
                st.heap()
                    .gc_params()
                    .set(idx, value.min(i32::MAX as i64) as u32);
            }
            st.push(Value::int(old));
        }
        #[cfg(rustynail_lua54)]
        "setpause" | "setstepmul" => {
            let v = st.opt_integer(2, 0)? as i32 as i64;
            let p = st.heap().gc_params();
            let old = if o == "setpause" {
                p.set_pause54(v)
            } else {
                p.set_stepmul54(v)
            };
            st.push(Value::int(old));
        }
        _ => unreachable!(),
    }
    Ok(1)
}

// ---------------------------------------------------------------------------------------------
// next / pairs / ipairs
// ---------------------------------------------------------------------------------------------

fn base_next(st: &mut State) -> LuaResult<usize> {
    let t = st.check_table(1)?;
    let k = st.arg(1);
    let mut hint = st.heap().next_hint;
    let r = t.next_hinted(&k, &mut hint);
    st.heap().next_hint = hint;
    match r {
        Ok(Some((k, v))) => {
            st.push(k);
            st.push(v);
            Ok(2)
        }
        Ok(None) => {
            st.push(Value::NIL);
            Ok(1)
        }
        Err(e) => Err(st.table_error(e)),
    }
}

/// Addresses of `next` and the `ipairs` iterator, which the JIT's generic-`for` helper runs without a call
/// frame (`rn_core::jit_hooks::set_builtins`).
#[cfg(feature = "jit")]
pub fn jit_builtins() -> (usize, usize) {
    (
        base_next as *const () as usize,
        ipairs_aux as *const () as usize,
    )
}

fn pairs_cont(_st: &mut State, _status: Status, _ctx: isize) -> LuaResult<usize> {
    #[cfg(not(rustynail_lua54))]
    return Ok(4);
    #[cfg(rustynail_lua54)]
    return Ok(3);
}

fn base_pairs(st: &mut State) -> LuaResult<usize> {
    let v = st.check_any(1)?;
    let mm = st.get_metafield(&v, b"__pairs");
    #[cfg(not(rustynail_lua54))]
    const NRES: usize = 4;
    #[cfg(rustynail_lua54)]
    const NRES: usize = 3;
    if mm.is_nil() {
        st.push(Value::native(base_next));
        st.push(v);
        for _ in 2..NRES {
            st.push(Value::NIL);
        }
        Ok(NRES)
    } else {
        st.check_stack(2)?;
        let func = st.top();
        st.push(mm);
        st.push(v);
        st.call_k(func, NRES as i32, 0, Some(pairs_cont))?;
        Ok(NRES)
    }
}

fn ipairs_aux(st: &mut State) -> LuaResult<usize> {
    let i = st.check_integer(2)?.wrapping_add(1);
    let t = st.arg(0);
    let v = match t.as_table() {
        Some(tb) => {
            let v = tb.get_int(i);
            if v.is_nil() && tb.metatable().is_some() {
                st.index_int(t, i)?
            } else {
                v
            }
        }
        None => st.index_int(t, i)?,
    };
    if v.is_nil() {
        st.push(Value::NIL);
        Ok(1)
    } else {
        st.push(Value::int(i));
        st.push(v);
        Ok(2)
    }
}

fn base_ipairs(st: &mut State) -> LuaResult<usize> {
    let v = st.check_any(1)?;
    st.push(Value::native(ipairs_aux));
    st.push(v);
    st.push(Value::int(0));
    Ok(3)
}

// ---------------------------------------------------------------------------------------------
// load / loadfile / dofile
// ---------------------------------------------------------------------------------------------

/// `getMode`: optional mode string; 5.5 rejects `B`.
fn get_mode(st: &mut State, idx: usize) -> LuaResult<Option<Vec<u8>>> {
    #[cfg(rustynail_lua54)]
    {
        if idx == 3 {
            return Ok(Some(match st.opt_str(idx)? {
                Some(s) => s.get().as_bytes().to_vec(),
                None => b"bt".to_vec(),
            }));
        }
    }
    match st.opt_str(idx)? {
        None => Ok(None),
        Some(s) => {
            let b = s.get().as_bytes();
            #[cfg(not(rustynail_lua54))]
            if b.contains(&b'B') {
                return Err(st.arg_error(idx, b"invalid mode"));
            }
            Ok(Some(b.to_vec()))
        }
    }
}

/// `load_aux`: the load status is in `status`; the closure / message is on top of the stack.
fn load_aux(st: &mut State, status: Status, env: Option<Value>) -> LuaResult<usize> {
    if status == Status::Ok {
        if let Some(e) = env {
            let f = st.get(st.top() - 1);
            let _ = st.set_upvalue_fn(&f, 1, e);
        }
        Ok(1)
    } else {
        let msg = st.pop();
        st.push(Value::NIL);
        st.push(msg);
        Ok(2)
    }
}

fn base_loadfile(st: &mut State) -> LuaResult<usize> {
    let fname = st.opt_str(1)?.map(|s| s.get().as_bytes().to_vec());
    let mode = get_mode(st, 2)?;
    let env = if st.nargs() >= 3 {
        Some(st.arg(2))
    } else {
        None
    };
    let status = st.load_filex(fname.as_deref(), mode.as_deref());
    load_aux(st, status, env)
}

fn base_load(st: &mut State) -> LuaResult<usize> {
    let a1 = st.arg(0);
    let mode = get_mode(st, 3)?;
    let env = if st.nargs() >= 4 {
        Some(st.arg(3))
    } else {
        None
    };
    let status;
    if let Some(s) = a1.as_string().map(|s| s.as_bytes().to_vec()).or_else(|| {
        a1.as_number()
            .map(|n| number::tostring(n).as_bytes().to_vec())
    }) {
        let name = match st.opt_str(2)? {
            Some(n) => n.get().as_bytes().to_vec(),
            None => s.clone(),
        };
        status = st.load_buffer(&s, &name, mode.as_deref());
    } else {
        let name = match st.opt_str(2)? {
            Some(n) => n.get().as_bytes().to_vec(),
            None => b"=(load)".to_vec(),
        };
        st.check_type(1, LuaType::Function)?;
        let mut buf: Vec<u8> = Vec::new();
        let mut failure: Option<Value> = None;
        loop {
            st.check_stack_msg(2, "too many nested functions")?;
            if let Err((_, e)) = st.pcall_value(a1, &[], 1) {
                failure = Some(e);
                break;
            }
            let r = st.pop();
            if r.is_nil() {
                break;
            }
            if let Some(s) = r.as_string() {
                if s.is_empty() {
                    break;
                }
                buf.extend_from_slice(s.as_bytes());
            } else if let Some(n) = r.as_number() {
                buf.extend_from_slice(number::tostring(n).as_bytes());
            } else {
                let mut m = st.where_(1);
                m.extend_from_slice(b"reader function must return a string");
                failure = Some(st.new_str_value(&m)?);
                break;
            }
        }
        if let Some(e) = failure {
            st.push(e);
            status = Status::ErrRun;
        } else {
            status = st.load_buffer(&buf, &name, mode.as_deref());
        }
    }
    load_aux(st, status, env)
}

fn dofile_cont(st: &mut State, _s: Status, _c: isize) -> LuaResult<usize> {
    Ok(st.nargs() - 1)
}

fn base_dofile(st: &mut State) -> LuaResult<usize> {
    let fname = st.opt_str(1)?.map(|s| s.get().as_bytes().to_vec());
    let base = st.ci().r().func + 1;
    st.set_top(base + 1);
    if st.load_filex(fname.as_deref(), Some(b"bt")) != Status::Ok {
        let e = st.get(st.top() - 1);
        return Err(st.throw_value(e));
    }
    let func = st.top() - 1;
    st.call_k(func, LUA_MULTRET, 0, Some(dofile_cont))?;
    dofile_cont(st, Status::Ok, 0)
}

// ---------------------------------------------------------------------------------------------
// select / pcall / xpcall
// ---------------------------------------------------------------------------------------------

fn base_select(st: &mut State) -> LuaResult<usize> {
    let n = st.nargs() as i64;
    let a = st.arg(0);
    if let Some(s) = a.as_string() {
        if s.as_bytes().first() == Some(&b'#') {
            st.push(Value::int(n - 1));
            return Ok(1);
        }
    }
    let mut i = st.check_integer(1)?;
    if i < 0 {
        i += n;
    } else if i > n {
        i = n;
    }
    st.arg_check(i >= 1, 1, b"index out of range")?;
    Ok((n - i) as usize)
}

/// P-E: the address of `select` (the J5 recorder inlines `select('#', ...)` / `select(n, ...)`).
#[cfg(feature = "jit")]
pub fn jit_select_addr() -> usize {
    base_select as *const () as usize
}

/// N3b: the address of `setmetatable` (the J5 recorder runs it as a `SETMT` intrinsic).
#[cfg(feature = "jit")]
pub fn jit_setmt_addr() -> usize {
    base_setmetatable as *const () as usize
}

/// F2: the address of `getmetatable` (the J5 recorder runs it as inline loads).
#[cfg(feature = "jit")]
pub fn jit_getmt_addr() -> usize {
    base_getmetatable as *const () as usize
}

/// F2: the address of `assert` (the J5 recorder runs a truthy first argument as a type guard).
#[cfg(feature = "jit")]
pub fn jit_assert_addr() -> usize {
    base_assert as *const () as usize
}

/// N7: the addresses of `ipairs` and `pairs` (the J5 recorder records the call that creates the generic-`for` state).
#[cfg(feature = "jit")]
pub fn jit_gfor_addrs() -> (usize, usize) {
    (
        base_ipairs as *const () as usize,
        base_pairs as *const () as usize,
    )
}

/// P-O: the addresses of `pcall` / `xpcall` (the J5 recorder inlines a protected call of a Lua closure whose body is free of
/// side effects before every guard: an exit re-executes the real call).
#[cfg(feature = "jit")]
pub fn jit_pcall_addr() -> usize {
    base_pcall as *const () as usize
}

/// P-O: see [`jit_pcall_addr`].
#[cfg(feature = "jit")]
pub fn jit_xpcall_addr() -> usize {
    base_xpcall as *const () as usize
}

fn finishpcall(st: &mut State, status: Status, extra: isize) -> LuaResult<usize> {
    if status != Status::Ok && status != Status::Yield {
        let e = st.get(st.top() - 1);
        st.push(Value::FALSE);
        st.push(e);
        Ok(2)
    } else {
        Ok((st.nargs() as isize - extra) as usize)
    }
}

fn base_pcall(st: &mut State) -> LuaResult<usize> {
    st.check_any(1)?;
    st.check_stack(1)?;
    let base = st.ci().r().func + 1;
    let n = st.nargs();
    for i in (0..n).rev() {
        let v = st.get(base + i);
        st.set(base + i + 1, v);
    }
    st.set(base, Value::TRUE);
    st.set_top(base + n + 1);
    let status = st.pcall_k(base + 1, LUA_MULTRET, 0, 0, Some(finishpcall))?;
    finishpcall(st, status, 0)
}

fn base_xpcall(st: &mut State) -> LuaResult<usize> {
    let n = st.nargs();
    st.check_type(2, LuaType::Function)?;
    st.check_stack(2)?;
    let base = st.ci().r().func + 1;
    let f = st.get(base);
    for i in (2..n).rev() {
        let v = st.get(base + i);
        st.set(base + i + 2, v);
    }
    st.set(base + 2, Value::TRUE);
    st.set(base + 3, f);
    st.set_top(base + n + 2);
    let ef = (base + 1).0;
    let status = st.pcall_k(base + 3, LUA_MULTRET, ef, 2, Some(finishpcall))?;
    finishpcall(st, status, 2)
}

// ---------------------------------------------------------------------------------------------
// open
// ---------------------------------------------------------------------------------------------

/// `luaopen_base`: registers the functions in the global table and returns it.
pub fn open(st: &mut State) -> LuaResult<usize> {
    let g = st.globals();
    st.push(Value::table(g));
    let funcs: &[(&str, Option<rn_core::state::NativeFn>)] = &[
        ("assert", Some(base_assert)),
        ("collectgarbage", Some(base_collectgarbage)),
        ("dofile", Some(base_dofile)),
        ("error", Some(base_error)),
        ("getmetatable", Some(base_getmetatable)),
        ("ipairs", Some(base_ipairs)),
        ("loadfile", Some(base_loadfile)),
        ("load", Some(base_load)),
        ("next", Some(base_next)),
        ("pairs", Some(base_pairs)),
        ("pcall", Some(base_pcall)),
        ("print", Some(base_print)),
        ("warn", Some(base_warn)),
        ("rawequal", Some(base_rawequal)),
        ("rawlen", Some(base_rawlen)),
        ("rawget", Some(base_rawget)),
        ("rawset", Some(base_rawset)),
        ("select", Some(base_select)),
        ("setmetatable", Some(base_setmetatable)),
        ("tonumber", Some(base_tonumber)),
        ("tostring", Some(base_tostring)),
        ("xpcall", Some(base_xpcall)),
    ];
    st.set_funcs(g, funcs)?;
    // T2: the interpreter's generic `for` runs `next` / the `ipairs` iterator without a frame as well
    // (`vm_unsafe` `OP_TFORCALL`) while no compiler is registered
    rn_core::frameless::set_interp_builtins(
        base_next as *const () as usize,
        ipairs_aux as *const () as usize,
    );
    // `type` as a closure caching the type-name strings
    let names: [&[u8]; 9] = [
        b"nil",
        b"boolean",
        b"userdata",
        b"number",
        b"string",
        b"table",
        b"function",
        b"userdata",
        b"thread",
    ];
    let cl = st.try_alloc(|h| h.new_cclosure(base_type, names.len()))?;
    st.push(Value::cclosure(cl));
    for (i, n) in names.iter().enumerate() {
        let s = st.new_str_value(n)?;
        // set directly: the closure is anchored on the stack
        cl.set_upvalue(st.heap(), i, s);
    }
    st.raw_set_str(g, b"type", Value::cclosure(cl))?;
    st.pop();
    st.raw_set_str(g, b"_G", Value::table(g))?;
    let v = st.new_str_value(LUA_VERSION)?;
    st.push(v);
    st.raw_set_str(g, b"_VERSION", v)?;
    st.pop();
    Ok(1)
}

/// Placeholder for the `coroutine` library until the coroutine task lands (creates an empty table).
pub fn open_coroutine_placeholder(st: &mut State) -> LuaResult<usize> {
    let t = st.new_table_sized(0, 0)?;
    st.push(Value::table(t));
    Ok(1)
}

#[allow(unused)]
fn _unused(_: Number, _: StkId) {}

/// P-B: the addresses of `tostring`, `rawlen`, `rawequal`, `rawget`, `rawset`, `tonumber` and `type` (the last is a C closure: its
/// function pointer; the J5 helper compares it with the running closure's).
#[cfg(feature = "jit")]
pub fn jit_libfns() -> ([usize; 6], usize) {
    (
        [
            base_tostring as *const () as usize,
            base_rawlen as *const () as usize,
            base_rawequal as *const () as usize,
            base_rawget as *const () as usize,
            base_rawset as *const () as usize,
            base_tonumber as *const () as usize,
        ],
        base_type as *const () as usize,
    )
}
