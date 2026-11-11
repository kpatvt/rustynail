//! `lua.h`, part 2: get functions (Lua -> stack) and set functions (stack -> Lua).
use crate::glue::*;
use crate::types::*;
use rn_core::value::Value;
use std::ffi::{c_char, c_int, c_void};

#[inline]
fn tt(v: &Value) -> c_int {
    v.type_tag() as c_int
}

lua_api!(lua_getglobal(st, name: *const c_char) -> c_int {
    let g = Value::table(st.globals());
    let r = st.index_str(g, cstr(name));
    let v = ck(&mut st, r);
    st.push(v);
    tt(&v)
});

lua_api!(lua_gettable(st, idx: c_int) -> c_int {
    let t = val(&st, idx);
    let key = st.get(st.top() - 1);
    let r = st.index(t, key);
    let v = ck(&mut st, r);
    let top = st.top();
    st.set(top - 1, v);
    tt(&v)
});

lua_api!(lua_getfield(st, idx: c_int, k: *const c_char) -> c_int {
    let t = val(&st, idx);
    let r = st.index_str(t, cstr(k));
    let v = ck(&mut st, r);
    st.push(v);
    tt(&v)
});

lua_api!(lua_geti(st, idx: c_int, n: lua_Integer) -> c_int {
    let t = val(&st, idx);
    let r = st.index_int(t, n);
    let v = ck(&mut st, r);
    st.push(v);
    tt(&v)
});

fn raw_table(st: &rn_core::state::State, idx: c_int) -> rn_core::table::TableRef {
    val(st, idx)
        .as_table()
        .expect("table expected (API contract)")
}

lua_api!(lua_rawget(st, idx: c_int) -> c_int {
    let t = raw_table(&st, idx);
    let key = st.get(st.top() - 1);
    let v = t.get(&key);
    let top = st.top();
    st.set(top - 1, v);
    tt(&v)
});

lua_api!(lua_rawgeti(st, idx: c_int, n: lua_Integer) -> c_int {
    let t = raw_table(&st, idx);
    let v = t.get_int(n);
    st.push(v);
    tt(&v)
});

lua_api!(lua_rawgetp(st, idx: c_int, p: *const c_void) -> c_int {
    let t = raw_table(&st, idx);
    let v = t.get(&Value::light_userdata(p as *mut c_void));
    st.push(v);
    tt(&v)
});

lua_api!(lua_createtable(st, narr: c_int, nrec: c_int) {
    let r = st.new_table_sized(narr.max(0) as usize, nrec.max(0) as usize);
    let t = ck(&mut st, r);
    st.push(Value::table(t));
    st.check_gc();
});

lua_api!(lua_newuserdatauv(st, size: usize, nuvalue: c_int) -> *mut c_void {
    let r = st.try_alloc(|h| h.new_udata(size, nuvalue.max(0) as usize));
    let u = ck(&mut st, r);
    st.push(Value::userdata(u));
    st.check_gc();
    u.data_ptr().cast()
});

lua_api!(lua_getmetatable(st, idx: c_int) -> c_int {
    let v = val(&st, idx);
    match st.metatable_of(&v) {
        Some(mt) => {
            st.push(Value::table(mt));
            1
        }
        None => 0,
    }
});

lua_api!(lua_getiuservalue(st, idx: c_int, n: c_int) -> c_int {
    let v = val(&st, idx);
    let u = v.as_userdata().expect("userdata expected (API contract)");
    match if n <= 0 { None } else { u.user_value(n as usize - 1) } {
        None => {
            st.push(Value::NIL);
            LUA_TNONE
        }
        Some(uv) => {
            st.push(uv);
            tt(&uv)
        }
    }
});

// ---- set functions ---------------------------------------------------------------------------

lua_api!(lua_setglobal(st, name: *const c_char) {
    let g = Value::table(st.globals());
    let v = st.get(st.top() - 1);
    let r = st.set_index_str(g, cstr(name), v);
    ck(&mut st, r);
    let t = st.top();
    st.set_top(t - 1);
});

lua_api!(lua_settable(st, idx: c_int) {
    let t = val(&st, idx);
    let top = st.top();
    let (key, v) = (st.get(top - 2), st.get(top - 1));
    let r = st.set_index(t, key, v);
    ck(&mut st, r);
    st.set_top(top - 2);
});

lua_api!(lua_setfield(st, idx: c_int, k: *const c_char) {
    let t = val(&st, idx);
    let v = st.get(st.top() - 1);
    let r = st.set_index_str(t, cstr(k), v);
    ck(&mut st, r);
    let top = st.top();
    st.set_top(top - 1);
});

lua_api!(lua_seti(st, idx: c_int, n: lua_Integer) {
    let t = val(&st, idx);
    let v = st.get(st.top() - 1);
    let r = st.set_index_int(t, n, v);
    ck(&mut st, r);
    let top = st.top();
    st.set_top(top - 1);
});

lua_api!(lua_rawset(st, idx: c_int) {
    let t = raw_table(&st, idx);
    let top = st.top();
    let (key, v) = (st.get(top - 2), st.get(top - 1));
    let r = st.raw_set(t, &key, &v);
    ck(&mut st, r);
    st.set_top(top - 2);
});

lua_api!(lua_rawseti(st, idx: c_int, n: lua_Integer) {
    let t = raw_table(&st, idx);
    let v = st.get(st.top() - 1);
    let r = st.raw_set_int(t, n, &v);
    ck(&mut st, r);
    let top = st.top();
    st.set_top(top - 1);
});

lua_api!(lua_rawsetp(st, idx: c_int, p: *const c_void) {
    let t = raw_table(&st, idx);
    let v = st.get(st.top() - 1);
    let r = st.raw_set(t, &Value::light_userdata(p as *mut c_void), &v);
    ck(&mut st, r);
    let top = st.top();
    st.set_top(top - 1);
});

lua_api!(lua_setmetatable(st, idx: c_int) -> c_int {
    let obj = val(&st, idx);
    let mtv = st.get(st.top() - 1);
    let mt = mtv.as_table();
    st.set_metatable(&obj, mt);
    let top = st.top();
    st.set_top(top - 1);
    1
});

lua_api!(lua_setiuservalue(st, idx: c_int, n: c_int) -> c_int {
    let v = val(&st, idx);
    let u = v.as_userdata().expect("userdata expected (API contract)");
    let uv = st.get(st.top() - 1);
    let ok = n > 0 && u.set_user_value(st.heap(), n as usize - 1, uv);
    let top = st.top();
    st.set_top(top - 1);
    ok as c_int
});
