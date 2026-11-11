//! Debug library (port of `ldblib.c`).
#![forbid(unsafe_code)]

use rn_core::state::{NativeFn, State, LUA_MASKCALL, LUA_MASKCOUNT, LUA_MASKLINE, LUA_MASKRET};
use rn_core::status::LuaResult;
use rn_core::table::TableRef;
use rn_core::value::{LuaType, Value};

const HOOKKEY: &[u8] = b"_HOOKKEY";

/// `getthread`: the thread of an optional first argument (as `(L1, arg)`).
fn get_thread(st: &mut State) -> (State, usize) {
    if let Some(t) = st.arg(0).as_thread() {
        (State::from_thread(t), 1)
    } else {
        (State::from_thread(st.as_gc()), 0)
    }
}

fn set_s(st: &mut State, t: TableRef, k: &str, v: &[u8]) -> LuaResult<()> {
    let s = st.new_str_value(v)?;
    st.push(s);
    let r = st.raw_set_str(t, k.as_bytes(), s);
    st.pop();
    r
}

fn set_i(st: &mut State, t: TableRef, k: &str, v: i64) -> LuaResult<()> {
    st.raw_set_str(t, k.as_bytes(), Value::int(v))
}

fn set_b(st: &mut State, t: TableRef, k: &str, v: bool) -> LuaResult<()> {
    st.raw_set_str(t, k.as_bytes(), Value::boolean(v))
}

fn db_getregistry(st: &mut State) -> LuaResult<usize> {
    st.push(st.g().registry);
    Ok(1)
}

fn db_getmetatable(st: &mut State) -> LuaResult<usize> {
    let v = st.check_any(1)?;
    match st.metatable_of(&v) {
        Some(mt) => st.push(Value::table(mt)),
        None => st.push(Value::NIL),
    }
    Ok(1)
}

fn db_setmetatable(st: &mut State) -> LuaResult<usize> {
    let a2 = st.arg(1);
    if st.nargs() < 2 || !(a2.is_nil() || a2.is_table()) {
        return Err(st.type_error_arg(2, b"nil or table"));
    }
    let o = st.arg(0);
    st.set_metatable(&o, a2.as_table());
    st.push(o);
    Ok(1)
}

fn db_getuservalue(st: &mut State) -> LuaResult<usize> {
    let n = st.opt_integer(2, 1)?;
    let o = st.arg(0);
    match o.as_userdata() {
        None => {
            st.push(Value::NIL);
            Ok(1)
        }
        Some(u) => {
            let v = if n >= 1 {
                u.user_value((n - 1) as usize)
            } else {
                None
            };
            match v {
                Some(v) => {
                    st.push(v);
                    st.push(Value::TRUE);
                    Ok(2)
                }
                None => {
                    st.push(Value::NIL);
                    Ok(1)
                }
            }
        }
    }
}

fn db_setuservalue(st: &mut State) -> LuaResult<usize> {
    let n = st.opt_integer(3, 1)?;
    let o = st.check_type(1, LuaType::Userdata)?;
    let v = st.check_any(2)?;
    let u = o.as_userdata();
    let ok = match u {
        Some(u) if n >= 1 => u.set_user_value(st.heap(), (n - 1) as usize, v),
        _ => false,
    };
    if ok {
        st.push(o);
    } else {
        st.push(Value::NIL);
    }
    Ok(1)
}

fn db_getinfo(st: &mut State) -> LuaResult<usize> {
    let (l1, arg) = get_thread(st);
    let options = match st.opt_str(arg + 2)? {
        Some(s) => s.get().as_bytes().to_vec(),
        None => b"flnSrtu".to_vec(),
    };
    if options.first() == Some(&b'>') {
        return Err(st.arg_error(arg + 2, b"invalid option '>'"));
    }
    let a = st.arg(arg);
    let info = if a.is_function() {
        l1.get_info_func(a, &options)
    } else {
        let level = st.check_integer(arg + 1)?;
        let ci = if level >= 0 {
            l1.get_stack(level as usize)
        } else {
            None
        };
        match ci {
            None => {
                st.push(Value::NIL);
                return Ok(1);
            }
            Some(ci) => l1.get_info_ci(ci, &options),
        }
    };
    let Some(ar) = info else {
        return Err(st.arg_error(arg + 2, b"invalid option"));
    };
    let t = st.new_table_sized(0, 0)?;
    st.push(Value::table(t));
    let has = |c: u8| options.contains(&c);
    if has(b'S') {
        set_s(st, t, "source", &ar.source)?;
        set_s(st, t, "short_src", &ar.short_src)?;
        set_i(st, t, "linedefined", ar.linedefined as i64)?;
        set_i(st, t, "lastlinedefined", ar.lastlinedefined as i64)?;
        set_s(st, t, "what", ar.what.as_bytes())?;
    }
    if has(b'l') {
        set_i(st, t, "currentline", ar.currentline as i64)?;
    }
    if has(b'u') {
        set_i(st, t, "nups", ar.nups as i64)?;
        set_i(st, t, "nparams", ar.nparams as i64)?;
        set_b(st, t, "isvararg", ar.isvararg)?;
    }
    if has(b'n') {
        if let Some(n) = &ar.name {
            set_s(st, t, "name", n)?
        }
        set_s(st, t, "namewhat", ar.namewhat.as_bytes())?;
    }
    if has(b'r') {
        set_i(st, t, "ftransfer", ar.ftransfer as i64)?;
        set_i(st, t, "ntransfer", ar.ntransfer as i64)?;
    }
    if has(b't') {
        set_b(st, t, "istailcall", ar.istailcall)?;
        #[cfg(not(rustynail_lua54))]
        set_i(st, t, "extraargs", ar.extraargs as i64)?;
    }
    if has(b'L') {
        if let Some(lines) = &ar.activelines {
            let lt = st.new_table_sized(0, 0)?;
            st.push(Value::table(lt));
            for &l in lines {
                st.raw_set_int(lt, l as i64, &Value::TRUE)?;
            }
            st.raw_set_str(t, b"activelines", Value::table(lt))?;
            st.pop();
        }
    }
    if has(b'f') {
        st.raw_set_str(t, b"func", ar.func)?;
    }
    Ok(1)
}

fn db_getlocal(st: &mut State) -> LuaResult<usize> {
    let (l1, arg) = get_thread(st);
    let nvar = st.check_integer(arg + 2)?;
    let a = st.arg(arg);
    if a.is_function() {
        // name of a parameter of a non-active function
        let name = a
            .as_lclosure()
            .and_then(|cl| cl.proto().local_name(nvar as i32, 0))
            .map(|s| s.get().as_bytes().to_vec());
        match name {
            Some(n) => {
                st.push_bytes(&n)?;
            }
            None => st.push(Value::NIL),
        }
        return Ok(1);
    }
    let level = st.check_integer(arg + 1)?;
    let ci = if level >= 0 {
        l1.get_stack(level as usize)
    } else {
        None
    };
    let Some(ci) = ci else {
        return Err(st.arg_error(arg + 1, b"level out of range"));
    };
    match l1.find_local(ci, nvar as i32) {
        Some((name, pos)) => {
            let v = l1.get(pos);
            st.push(v);
            st.push_bytes(&name)?;
            let top = st.top();
            let nm = st.get(top - 1);
            st.set(top - 1, v);
            st.set(top - 2, nm);
            Ok(2)
        }
        None => {
            st.push(Value::NIL);
            Ok(1)
        }
    }
}

fn db_setlocal(st: &mut State) -> LuaResult<usize> {
    let (mut l1, arg) = get_thread(st);
    let level = st.check_integer(arg + 1)?;
    let nvar = st.check_integer(arg + 2)?;
    let ci = if level >= 0 {
        l1.get_stack(level as usize)
    } else {
        None
    };
    let Some(ci) = ci else {
        return Err(st.arg_error(arg + 1, b"level out of range"));
    };
    let v = st.check_any(arg + 3)?;
    match l1.find_local(ci, nvar as i32) {
        Some((name, pos)) => {
            l1.set(pos, v);
            st.push_bytes(&name)?;
        }
        None => st.push(Value::NIL),
    }
    Ok(1)
}

fn auxupvalue(st: &mut State, get: bool) -> LuaResult<usize> {
    let n = st.check_integer(2)?;
    let f = st.check_type(1, LuaType::Function)?;
    let n = if n < 1 { 0 } else { n as usize };
    if get {
        match st.get_upvalue(&f, n) {
            None => Ok(0),
            Some((name, v)) => {
                st.push_bytes(&name)?;
                st.push(v);
                Ok(2)
            }
        }
    } else {
        let v = st.arg(2);
        match st.set_upvalue_fn(&f, n, v) {
            None => Ok(0),
            Some(name) => {
                st.push_bytes(&name)?;
                Ok(1)
            }
        }
    }
}

fn db_getupvalue(st: &mut State) -> LuaResult<usize> {
    auxupvalue(st, true)
}

fn db_setupvalue(st: &mut State) -> LuaResult<usize> {
    st.check_any(3)?;
    auxupvalue(st, false)
}

fn check_upval(
    st: &mut State,
    argf: usize,
    argnup: usize,
    need: bool,
) -> LuaResult<(Value, usize, bool)> {
    let nup = st.check_integer(argnup)?;
    let f = st.check_type(argf, LuaType::Function)?;
    let n = if nup < 1 { 0 } else { nup as usize };
    let id = st.upvalue_id(&f, n);
    if need && id.is_none() {
        return Err(st.arg_error(argnup, b"invalid upvalue index"));
    }
    Ok((f, n, id.is_some()))
}

fn db_upvalueid(st: &mut State) -> LuaResult<usize> {
    let (f, n, _) = check_upval(st, 1, 2, false)?;
    match st.upvalue_id(&f, n) {
        Some(p) => st.push(Value::light_userdata(p)),
        None => st.push(Value::NIL),
    }
    Ok(1)
}

fn db_upvaluejoin(st: &mut State) -> LuaResult<usize> {
    let (f1, n1, _) = check_upval(st, 1, 2, true)?;
    let (f2, n2, _) = check_upval(st, 3, 4, true)?;
    st.arg_check(
        !f1.is_light_cfunction() && !f1.is_cclosure(),
        1,
        b"Lua function expected",
    )?;
    st.arg_check(
        !f2.is_light_cfunction() && !f2.is_cclosure(),
        3,
        b"Lua function expected",
    )?;
    st.upvalue_join(&f1, n1, &f2, n2);
    Ok(0)
}

fn hookf(st: &mut State, event: i32, line: i32) -> LuaResult<()> {
    const NAMES: [&[u8]; 5] = [b"call", b"return", b"line", b"count", b"tail call"];
    let hk = st.registry_get(HOOKKEY)?;
    let Some(ht) = hk.as_table() else {
        return Ok(());
    };
    let th = Value::thread(st.as_gc());
    let f = ht.get(&th);
    if !f.is_function() {
        return Ok(());
    }
    let name = st.new_str_value(NAMES.get(event as usize).copied().unwrap_or(b"?"))?;
    st.push(name);
    let l = if line >= 0 {
        Value::int(line as i64)
    } else {
        Value::NIL
    };
    st.call_value(f, &[name, l], 0)?; // errors propagate; yields fail (C-call boundary)
    st.pop();
    Ok(())
}

fn db_sethook(st: &mut State) -> LuaResult<usize> {
    let (mut l1, arg) = get_thread(st);
    let (func, mask, count): (Option<NativeHook>, u8, i32);
    if st.arg_is_none_or_nil(arg + 1) {
        func = None;
        mask = 0;
        count = 0;
    } else {
        let smask = st.check_str(arg + 2)?.get().as_bytes().to_vec();
        st.check_type(arg + 1, LuaType::Function)?;
        count = st.opt_integer(arg + 3, 0)? as i32;
        func = Some(hookf);
        let mut m = 0;
        if smask.contains(&b'c') {
            m |= LUA_MASKCALL;
        }
        if smask.contains(&b'r') {
            m |= LUA_MASKRET;
        }
        if smask.contains(&b'l') {
            m |= LUA_MASKLINE;
        }
        if count > 0 {
            m |= LUA_MASKCOUNT;
        }
        mask = m;
    }
    let reg = st.registry();
    let (ht, existed) = st.get_sub_table(reg, HOOKKEY)?;
    st.push(Value::table(ht));
    if !existed {
        let k = st_str(st, b"k")?;
        st.raw_set_str(ht, b"__mode", k)?;
        st.set_metatable(&Value::table(ht), Some(ht));
    }
    let th = Value::thread(l1.as_gc());
    let hv = st.arg(arg);
    st.raw_set(ht, &th, &hv)?;
    l1.set_hook(func, mask, count);
    Ok(0)
}

type NativeHook = rn_core::state::HookFn;

fn st_str(st: &mut State, b: &[u8]) -> LuaResult<Value> {
    let v = st.new_str_value(b)?;
    st.push(v);
    Ok(v)
}

fn db_gethook(st: &mut State) -> LuaResult<usize> {
    let (l1, _arg) = get_thread(st);
    let mask = l1.hook_mask();
    match l1.hook_fn() {
        None => {
            st.push(Value::NIL);
            Ok(1)
        }
        Some(h) => {
            if h as usize != hookf as *const () as usize {
                st.push_bytes(b"external hook")?;
            } else {
                let hk = st.registry_get(HOOKKEY)?;
                let v = match hk.as_table() {
                    Some(t) => t.get(&Value::thread(l1.as_gc())),
                    None => Value::NIL,
                };
                st.push(v);
            }
            let mut m = Vec::new();
            if mask & LUA_MASKCALL != 0 {
                m.push(b'c');
            }
            if mask & LUA_MASKRET != 0 {
                m.push(b'r');
            }
            if mask & LUA_MASKLINE != 0 {
                m.push(b'l');
            }
            st.push_bytes(&m)?;
            st.push(Value::int(l1.hook_count() as i64));
            Ok(3)
        }
    }
}

fn db_debug(st: &mut State) -> LuaResult<usize> {
    use std::io::BufRead;
    let stdin = std::io::stdin();
    loop {
        crate::io_ffi::stderr_write(b"lua_debug> ");
        let mut line = Vec::new();
        match stdin.lock().read_until(b'\n', &mut line) {
            Ok(0) | Err(_) => return Ok(0),
            Ok(_) => {}
        }
        if line == b"cont\n" {
            return Ok(0);
        }
        let base = st.ci().r().func + 1;
        let status = st.load_buffer(&line, b"=(debug command)", Some(b"t"));
        let failed = if status == rn_core::status::Status::Ok {
            let f = st.top() - 1;
            st.pcall(f, 0, 0) != rn_core::status::Status::Ok
        } else {
            true
        };
        if failed {
            let e = st.get(st.top() - 1);
            let s = st.tolstring(e)?;
            let mut m = s.get().as_bytes().to_vec();
            m.push(b'\n');
            crate::io_ffi::stderr_write(&m);
        }
        st.set_top(base);
    }
}

fn db_traceback(st: &mut State) -> LuaResult<usize> {
    let (mut l1, arg) = get_thread(st);
    let m = st.arg(arg);
    let msg: Option<Vec<u8>> = if let Some(s) = m.as_string() {
        Some(s.as_bytes().to_vec())
    } else {
        m.as_number()
            .map(|n| rn_core::number::tostring(n).as_bytes().to_vec())
    };
    if msg.is_none() && !st.arg_is_none_or_nil(arg + 1) {
        st.push(m);
        return Ok(1);
    }
    let same = l1.as_gc() == st.as_gc();
    let level = st.opt_integer(arg + 2, if same { 1 } else { 0 })?;
    let level = level.clamp(i32::MIN as i64, i32::MAX as i64) as i32;
    let tb = l1.traceback(msg.as_deref(), level);
    st.push_bytes(&tb)?;
    Ok(1)
}

#[cfg(rustynail_lua54)]
fn db_setcstacklimit(st: &mut State) -> LuaResult<usize> {
    st.check_integer(1)?;
    st.push(Value::int(200));
    Ok(1)
}

/// `luaopen_debug`.
pub fn open(st: &mut State) -> LuaResult<usize> {
    let t = st.new_table_sized(0, 17)?;
    st.push(Value::table(t));
    let funcs: &[(&str, Option<NativeFn>)] = &[
        ("debug", Some(db_debug)),
        ("getuservalue", Some(db_getuservalue)),
        ("gethook", Some(db_gethook)),
        ("getinfo", Some(db_getinfo)),
        ("getlocal", Some(db_getlocal)),
        ("getregistry", Some(db_getregistry)),
        ("getmetatable", Some(db_getmetatable)),
        ("getupvalue", Some(db_getupvalue)),
        ("upvaluejoin", Some(db_upvaluejoin)),
        ("upvalueid", Some(db_upvalueid)),
        ("setuservalue", Some(db_setuservalue)),
        ("sethook", Some(db_sethook)),
        ("setlocal", Some(db_setlocal)),
        ("setmetatable", Some(db_setmetatable)),
        ("setupvalue", Some(db_setupvalue)),
        ("traceback", Some(db_traceback)),
        #[cfg(rustynail_lua54)]
        ("setcstacklimit", Some(db_setcstacklimit)),
    ];
    st.set_funcs(t, funcs)?;
    Ok(1)
}
