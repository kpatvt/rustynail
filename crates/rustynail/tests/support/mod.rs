//! Minimal native library for interpreter tests (NOT the standard library; that is `rn-std`).
//!
//! Enough of the base library to run the corpus programs and compare their stdout with the
//! reference interpreter: `print type tostring tonumber pairs next ipairs pcall xpcall error assert
//! select setmetatable getmetatable rawget rawset rawequal rawlen load collectgarbage`, plus tiny
//! `table` (`unpack pack`), `math` (`mininteger maxinteger huge type tointeger floor abs fmod`),
//! `string` (`rep len sub byte char`), `io.write`, `os.clock`, and the string-arithmetic
//! metamethods of `lstrlib.c` (`stringmetamethods`), which in 5.4+ implement string -> number
//! coercion for arithmetic.
#![allow(dead_code)]
use rn_core::debuginfo::chunk_id;
use rn_core::error::Opnd;
use rn_core::meta::{type_name, Tm};
use rn_core::number::{self, Number};
use rn_core::state::{LuaState, NativeFn, State, StkId, LUA_MULTRET};
use rn_core::status::{LuaResult, Status};
use rn_core::table::TableRef;
use rn_core::value::{LuaType, Value};
use std::cell::RefCell;
use std::io::Write;

thread_local! {
    static OUT: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
}

/// Append to the captured standard output (flushed by [`flush`] / [`take_output`]).
pub fn emit(b: &[u8]) {
    OUT.with(|o| {
        let mut o = o.borrow_mut();
        o.extend_from_slice(b);
        if o.len() > (1 << 16) && STREAMING.with(|s| s.get()) {
            let _ = std::io::stdout().write_all(&o);
            o.clear();
        }
    });
}

thread_local! {
    static STREAMING: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Write captured output directly to the process' stdout from now on.
pub fn stream_to_stdout() {
    STREAMING.with(|s| s.set(true));
}

/// Flush captured output to stdout.
pub fn flush() {
    OUT.with(|o| {
        let mut o = o.borrow_mut();
        let mut out = std::io::stdout().lock();
        let _ = out.write_all(&o);
        let _ = out.flush();
        o.clear();
    });
}

/// Take the captured output.
pub fn take_output() -> Vec<u8> {
    OUT.with(|o| std::mem::take(&mut *o.borrow_mut()))
}

/// `luaL_argerror`: "bad argument #n to 'name' (msg)" with the name taken from the calling
/// instruction when possible, positioned at the Lua caller.
fn arg_err<T>(st: &mut State, n: usize, fname: &str, msg: &str) -> LuaResult<T> {
    let mut name = fname.to_string();
    let mut n = n;
    if let Some(prev) = st.ci().previous() {
        if let Some(cl) = st.ci_lclosure(prev) {
            let pc = st.current_pc(prev);
            if pc >= 0 {
                if let Some(o) = rn_core::debuginfo::funcnamefromcode(&*cl.proto(), pc as usize) {
                    if o.kind == "method" {
                        n -= 1; // do not count 'self'
                        if n == 0 {
                            let mut m = where_(st, 1);
                            m.extend_from_slice(
                                format!(
                                    "calling '{}' on bad self ({msg})",
                                    String::from_utf8_lossy(o.name)
                                )
                                .as_bytes(),
                            );
                            let s = st.new_str_value(&m)?;
                            return Err(st.throw_value(s));
                        }
                    }
                    name = String::from_utf8_lossy(o.name).into_owned();
                }
            }
        }
    }
    let mut m = where_(st, 1);
    m.extend_from_slice(format!("bad argument #{n} to '{name}' ({msg})").as_bytes());
    let s = st.new_str_value(&m)?;
    Err(st.throw_value(s))
}

/// `luaL_typeerror`: "<expected> expected, got <actual>" for argument `n` (1-based).
fn type_err<T>(st: &mut State, n: usize, fname: &str, expected: &str) -> LuaResult<T> {
    let got = if n > st.nargs() {
        "no value".to_string()
    } else {
        let v = st.arg(n - 1);
        String::from_utf8_lossy(st.obj_typename(&v)).into_owned()
    };
    arg_err(st, n, fname, &format!("{expected} expected, got {got}"))
}

fn str_of(v: &Value) -> Option<Vec<u8>> {
    if let Some(s) = v.as_string() {
        return Some(s.as_bytes().to_vec());
    }
    v.as_number()
        .map(|n| number::tostring(n).as_bytes().to_vec())
}

/// `luaL_tolstring`.
pub fn tostring_bytes(st: &mut State, v: Value) -> LuaResult<Vec<u8>> {
    if let Some(mt) = st.metatable_of(&v) {
        let key = st.new_str_value(b"__tostring")?;
        let f = mt.get(&key);
        if !f.is_nil() {
            st.check_stack(3)?;
            let func = st.top();
            st.push(f);
            st.push(v);
            st.call(func, 1)?;
            let r = st.pop();
            return match r.as_string() {
                Some(s) => Ok(s.as_bytes().to_vec()),
                None => Err(st.run_error(b"'__tostring' must return a string")),
            };
        }
    }
    Ok(match v.type_tag() {
        0 => b"nil".to_vec(),
        1 => {
            if v.is_truthy() {
                b"true".to_vec()
            } else {
                b"false".to_vec()
            }
        }
        3 | 4 => str_of(&v).unwrap_or_default(),
        _ => {
            let name = st.obj_typename(&v).to_vec();
            let addr = v.as_gc().map_or(0usize, |p| p as usize);
            let mut out = name;
            out.extend_from_slice(format!(": 0x{addr:014x}").as_bytes());
            out
        }
    })
}

fn n_print(st: &mut State) -> LuaResult<usize> {
    let n = st.nargs();
    let mut line: Vec<u8> = Vec::new();
    for i in 0..n {
        if i > 0 {
            line.push(b'\t');
        }
        let v = st.arg(i);
        line.extend_from_slice(&tostring_bytes(st, v)?);
    }
    line.push(b'\n');
    emit(&line);
    Ok(0)
}

fn n_io_write(st: &mut State) -> LuaResult<usize> {
    for i in 0..st.nargs() {
        let v = st.arg(i);
        match str_of(&v) {
            Some(b) => emit(&b),
            None => return arg_err(st, i + 1, "write", "string expected"),
        }
    }
    Ok(0)
}

fn n_type(st: &mut State) -> LuaResult<usize> {
    if st.nargs() == 0 {
        return arg_err(st, 1, "type", "value expected");
    }
    let v = st.arg(0);
    let s = st.new_str_value(type_name(v.type_tag()))?;
    st.push(s);
    Ok(1)
}

fn n_tostring(st: &mut State) -> LuaResult<usize> {
    let v = st.arg(0);
    let b = tostring_bytes(st, v)?;
    let s = st.new_str_value(&b)?;
    st.push(s);
    Ok(1)
}

fn n_tonumber(st: &mut State) -> LuaResult<usize> {
    let v = st.arg(0);
    let base = st.arg(1);
    if base.is_nil() {
        if v.is_number() {
            st.push(v);
            return Ok(1);
        }
        if let Some(s) = v.as_string() {
            if let Some(n) = number::str2num(s.as_bytes()) {
                st.push(Value::number(n));
                return Ok(1);
            }
        }
        st.push(Value::NIL);
        return Ok(1);
    }
    let b = base.as_int().unwrap_or(10);
    let Some(s) = v.as_string() else {
        return arg_err(st, 1, "tonumber", "string expected");
    };
    let bytes = s.as_bytes();
    let t: Vec<u8> = {
        let mut a = 0;
        let mut e = bytes.len();
        while a < e && number::is_space(bytes[a]) {
            a += 1;
        }
        while e > a && number::is_space(bytes[e - 1]) {
            e -= 1;
        }
        bytes[a..e].to_vec()
    };
    let (neg, digits) = match t.first() {
        Some(b'-') => (true, &t[1..]),
        _ => (false, &t[..]),
    };
    let mut acc: i64 = 0;
    let mut ok = !digits.is_empty();
    for &c in digits {
        let d = match c {
            b'0'..=b'9' => (c - b'0') as i64,
            b'a'..=b'z' => (c - b'a') as i64 + 10,
            b'A'..=b'Z' => (c - b'A') as i64 + 10,
            _ => 99,
        };
        if d >= b {
            ok = false;
            break;
        }
        acc = acc.wrapping_mul(b).wrapping_add(d);
    }
    st.push(if ok {
        Value::int(if neg { acc.wrapping_neg() } else { acc })
    } else {
        Value::NIL
    });
    Ok(1)
}

fn n_setmetatable(st: &mut State) -> LuaResult<usize> {
    let t = st.arg(0);
    if !t.is_table() {
        return type_err(st, 1, "setmetatable", "table");
    }
    let a1 = st.arg(1);
    if !(a1.is_nil() || a1.is_table()) {
        return type_err(st, 2, "setmetatable", "nil or table");
    }
    if let Some(old) = st.metatable_of(&t) {
        let k = st.new_str_value(b"__metatable")?;
        if !old.get(&k).is_nil() {
            return Err(st.run_error(b"cannot change a protected metatable"));
        }
    }
    st.set_metatable(&t, a1.as_table());
    st.push(t);
    Ok(1)
}

fn n_getmetatable(st: &mut State) -> LuaResult<usize> {
    let t = st.arg(0);
    match st.metatable_of(&t) {
        None => st.push(Value::NIL),
        Some(mt) => {
            let k = st.new_str_value(b"__metatable")?;
            let p = mt.get(&k);
            st.push(if p.is_nil() { Value::table(mt) } else { p });
        }
    }
    Ok(1)
}

fn n_rawequal(st: &mut State) -> LuaResult<usize> {
    let r = st.arg(0).raw_equal(&st.arg(1));
    st.push(Value::boolean(r));
    Ok(1)
}

fn n_rawget(st: &mut State) -> LuaResult<usize> {
    let Some(t) = st.arg(0).as_table() else {
        return type_err(st, 1, "rawget", "table");
    };
    let v = t.get(&st.arg(1));
    st.push(v);
    Ok(1)
}

fn n_rawset(st: &mut State) -> LuaResult<usize> {
    let Some(t) = st.arg(0).as_table() else {
        return type_err(st, 1, "rawset", "table");
    };
    let (k, v) = (st.arg(1), st.arg(2));
    st.raw_set(t, &k, &v)?;
    st.push(Value::table(t));
    Ok(1)
}

fn n_rawlen(st: &mut State) -> LuaResult<usize> {
    let v = st.arg(0);
    let n = if let Some(t) = v.as_table() {
        t.len(st.heap()) as i64
    } else if let Some(s) = v.as_string() {
        s.len() as i64
    } else {
        return arg_err(st, 1, "rawlen", "table or string expected");
    };
    st.push(Value::int(n));
    Ok(1)
}

/// Where information (`luaL_where`): `chunk:line: ` of the function at `level` (1 = caller).
fn where_(st: &State, level: usize) -> Vec<u8> {
    let mut ci = st.ci();
    for _ in 0..level {
        match ci.previous() {
            Some(p) => ci = p,
            None => return Vec::new(),
        }
    }
    match st.ci_lclosure(ci) {
        Some(cl) => {
            let line = st.current_line(ci);
            if line < 0 {
                return Vec::new();
            }
            let mut out = match cl.proto().source() {
                Some(s) => chunk_id(s.get().as_bytes()),
                None => b"?".to_vec(),
            };
            out.extend_from_slice(format!(":{line}: ").as_bytes());
            out
        }
        None => Vec::new(),
    }
}

fn n_error(st: &mut State) -> LuaResult<usize> {
    let v = st.arg(0);
    let level = st.arg(1).as_int().unwrap_or(1);
    if v.is_string() && level > 0 {
        let mut m = where_(st, level as usize);
        m.extend_from_slice(v.as_string().unwrap().as_bytes());
        let s = st.new_str_value(&m)?;
        return Err(st.throw_value(s));
    }
    Err(st.throw_value(v))
}

fn n_assert(st: &mut State) -> LuaResult<usize> {
    if st.nargs() == 0 {
        return arg_err(st, 1, "assert", "value expected");
    }
    if st.arg(0).is_falsy() {
        if st.nargs() > 1 {
            let m = st.arg(1);
            return Err(st.throw_value(m));
        }
        return Err(st.run_error(b"assertion failed!"));
    }
    Ok(st.nargs())
}

fn n_select(st: &mut State) -> LuaResult<usize> {
    let n = st.nargs();
    let a = st.arg(0);
    if a.as_string().is_some_and(|s| s.as_bytes() == b"#") {
        st.push(Value::int(n as i64 - 1));
        return Ok(1);
    }
    let Some(i) = a.as_int() else {
        return arg_err(st, 1, "select", "number expected");
    };
    let extra = n as i64 - 1;
    let cnt = if i < 0 {
        if -i > extra {
            return arg_err(st, 1, "select", "index out of range");
        }
        -i
    } else if i == 0 {
        return arg_err(st, 1, "select", "index out of range");
    } else if i > extra {
        0
    } else {
        extra - i + 1
    };
    Ok(cnt as usize)
}

fn n_pcall(st: &mut State) -> LuaResult<usize> {
    if st.nargs() == 0 {
        return arg_err(st, 1, "pcall", "value expected");
    }
    let base = st.ci().r().func + 1;
    let top = st.top();
    st.check_stack(1)?;
    // shift arguments up and insert `true` at the bottom
    let mut p = top;
    while p > base {
        let v = st.get(p - 1);
        st.set(p, v);
        p -= 1;
    }
    st.set(base, Value::TRUE);
    st.set_top(top + 1);
    let status = st.pcall(base + 1, LUA_MULTRET, 0);
    if status != Status::Ok {
        st.set(base, Value::FALSE);
    }
    Ok(st.top() - base)
}

fn n_xpcall(st: &mut State) -> LuaResult<usize> {
    if st.nargs() < 2 {
        return arg_err(st, 2, "xpcall", "value expected");
    }
    // stack: f msgh args...   ->   f msgh true f args...
    let base = st.ci().r().func + 1;
    let top = st.top();
    st.check_stack(2)?;
    let f = st.get(base);
    // move args (base+2 .. top) up by two
    let mut p = top;
    while p > base + 2 {
        let v = st.get(p - 1);
        st.set(p + 1, v);
        p -= 1;
    }
    st.set(base + 2, Value::TRUE);
    st.set(base + 3, f);
    st.set_top(top + 2);
    let status = st.pcall(base + 3, LUA_MULTRET, (base + 1).idx());
    if status != Status::Ok {
        let e = st.get(st.top() - 1);
        st.set(base + 2, Value::FALSE);
        st.set(base + 3, e);
        st.set_top(base + 4);
    }
    Ok(st.top() - (base + 2))
}

fn n_next(st: &mut State) -> LuaResult<usize> {
    let Some(t) = st.arg(0).as_table() else {
        return type_err(st, 1, "next", "table");
    };
    match t.next(&st.arg(1)) {
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

fn n_pairs(st: &mut State) -> LuaResult<usize> {
    let t = st.arg(0);
    if let Some(mt) = st.metatable_of(&t) {
        let k = st.new_str_value(b"__pairs")?;
        let f = mt.get(&k);
        if !f.is_nil() {
            st.check_stack(4)?;
            let func = st.top();
            st.push(f);
            st.push(t);
            st.call(func, 3)?;
            return Ok(3);
        }
    }
    if st.nargs() == 0 {
        return type_err(st, 1, "pairs", "table");
    }
    st.push(Value::native(n_next));
    st.push(t);
    st.push(Value::NIL);
    Ok(3)
}

/// `lua_geti` with metamethods.
fn geti(st: &mut State, obj: Value, i: i64) -> LuaResult<Value> {
    if let Some(t) = obj.as_table() {
        let v = t.get_int(i);
        if !v.is_nil() {
            return Ok(v);
        }
    }
    st.check_stack(1)?;
    st.push(Value::NIL);
    let res = st.top() - 1;
    st.finish_get(Opnd::from(obj), Value::int(i), res)?;
    Ok(st.pop())
}

fn n_ipairs_iter(st: &mut State) -> LuaResult<usize> {
    let t = st.arg(0);
    let i = st.arg(1).as_int().unwrap_or(0).wrapping_add(1);
    let v = geti(st, t, i)?;
    if v.is_nil() {
        st.push(Value::NIL);
        Ok(1)
    } else {
        st.push(Value::int(i));
        st.push(v);
        Ok(2)
    }
}

fn n_ipairs(st: &mut State) -> LuaResult<usize> {
    if st.nargs() == 0 {
        return arg_err(st, 1, "ipairs", "table expected, got no value");
    }
    let t = st.arg(0);
    st.push(Value::native(n_ipairs_iter));
    st.push(t);
    st.push(Value::int(0));
    Ok(3)
}

fn n_unpack(st: &mut State) -> LuaResult<usize> {
    let t = st.arg(0);
    let i = st.arg(1).as_int().unwrap_or(1);
    let e = match st.arg(2).as_int() {
        Some(e) => e,
        None => match t.as_table() {
            Some(h) => h.len(st.heap()) as i64,
            None => 0,
        },
    };
    if i > e {
        return Ok(0);
    }
    let n = (e as i128 - i as i128 + 1) as u128;
    if n >= 1_000_000 {
        return Err(st.run_error(b"too many results to unpack"));
    }
    st.check_stack(n as usize)?;
    for k in i..=e {
        let v = geti(st, t, k)?;
        st.push(v);
    }
    Ok(n as usize)
}

fn n_pack(st: &mut State) -> LuaResult<usize> {
    let n = st.nargs();
    let t = st.new_table()?;
    st.push(Value::table(t));
    for i in 0..n {
        let v = st.arg(i);
        st.raw_set_int(t, i as i64 + 1, &v)?;
    }
    let k = st.new_str_value(b"n")?;
    st.raw_set(t, &k, &Value::int(n as i64))?;
    Ok(1)
}

fn n_collectgarbage(st: &mut State) -> LuaResult<usize> {
    let opt = st.arg(0);
    let o = opt
        .as_string()
        .map_or(b"collect".to_vec(), |s| s.as_bytes().to_vec());
    match &o[..] {
        b"collect" => {
            st.collect(false);
            st.push(Value::int(0));
        }
        b"count" => {
            let b = st.heap().total_bytes();
            st.push(Value::float(b as f64 / 1024.0));
        }
        b"stop" => {
            st.heap().gc_stop();
            st.push(Value::int(0));
        }
        b"restart" => {
            st.heap().gc_restart();
            st.push(Value::int(0));
        }
        b"isrunning" => {
            let r = st.heap().gc_stp() == 0;
            st.push(Value::boolean(r));
        }
        b"step" => {
            st.collect(false);
            st.push(Value::TRUE);
        }
        _ => {
            st.push(Value::int(0));
        }
    }
    Ok(1)
}

fn n_load(st: &mut State) -> LuaResult<usize> {
    let chunk = st.arg(0);
    let name = st.arg(1);
    let mode = st.arg(2);
    let Some(s) = chunk.as_string() else {
        // functions as chunk readers are not supported by this minimal library
        return arg_err(st, 1, "load", "string expected");
    };
    let bytes = s.as_bytes().to_vec();
    let cname = name
        .as_string()
        .map_or_else(|| bytes.clone(), |n| n.as_bytes().to_vec());
    let mode = mode.as_string().map(|m| m.as_bytes().to_vec());
    let top = st.top();
    let status = st.load_chunk(&bytes, &cname, mode.as_deref());
    if status != Status::Ok {
        let msg = st.get(st.top() - 1);
        st.set_top(top);
        st.push(Value::NIL);
        st.push(msg);
        return Ok(2);
    }
    if st.nargs() >= 4 {
        let env = st.arg(3);
        let cl = st.get(st.top() - 1).as_lclosure().unwrap();
        if cl.nupvalues() >= 1 {
            let uv = cl.upval(0);
            uv.set(env);
        }
    }
    Ok(1)
}

// ---- tiny math / string ---------------------------------------------------------------------

fn m_type(st: &mut State) -> LuaResult<usize> {
    let v = st.arg(0);
    let r = match v.as_number() {
        Some(Number::Int(_)) => st.new_str_value(b"integer")?,
        Some(Number::Float(_)) => st.new_str_value(b"float")?,
        None => Value::NIL,
    };
    st.push(r);
    Ok(1)
}

fn m_tointeger(st: &mut State) -> LuaResult<usize> {
    let v = st.arg(0);
    let r = match v.as_number() {
        Some(Number::Int(i)) => Value::int(i),
        Some(Number::Float(f)) => {
            number::float_to_int(f, number::F2I::Eq).map_or(Value::NIL, Value::int)
        }
        None => Value::NIL,
    };
    st.push(r);
    Ok(1)
}

fn m_floor(st: &mut State) -> LuaResult<usize> {
    let v = st.arg(0);
    let r = match v.as_number() {
        Some(Number::Int(i)) => Value::int(i),
        Some(Number::Float(f)) => {
            let fl = f.floor();
            number::float_to_int(fl, number::F2I::Eq).map_or(Value::float(fl), Value::int)
        }
        None => return arg_err(st, 1, "floor", "number expected"),
    };
    st.push(r);
    Ok(1)
}

fn m_abs(st: &mut State) -> LuaResult<usize> {
    let r = match st.arg(0).as_number() {
        Some(Number::Int(i)) => Value::int(i.wrapping_abs()),
        Some(Number::Float(f)) => Value::float(f.abs()),
        None => return arg_err(st, 1, "abs", "number expected"),
    };
    st.push(r);
    Ok(1)
}

fn m_fmod(st: &mut State) -> LuaResult<usize> {
    let r = match (st.arg(0).as_number(), st.arg(1).as_number()) {
        (Some(Number::Int(a)), Some(Number::Int(b))) => {
            if b == 0 {
                return arg_err(st, 2, "fmod", "zero");
            }
            Value::int(if b == -1 { 0 } else { a % b })
        }
        (Some(a), Some(b)) => {
            let f = |n: Number| match n {
                Number::Int(i) => i as f64,
                Number::Float(f) => f,
            };
            Value::float(f(a) % f(b))
        }
        _ => return arg_err(st, 1, "fmod", "number expected"),
    };
    st.push(r);
    Ok(1)
}

fn s_len(st: &mut State) -> LuaResult<usize> {
    let n = str_of(&st.arg(0)).map_or(0, |s| s.len());
    st.push(Value::int(n as i64));
    Ok(1)
}

fn s_rep(st: &mut State) -> LuaResult<usize> {
    let s = str_of(&st.arg(0)).unwrap_or_default();
    let n = st.arg(1).as_int().unwrap_or(0);
    let sep = str_of(&st.arg(2)).unwrap_or_default();
    if n <= 0 {
        let e = st.new_str_value(b"")?;
        st.push(e);
        return Ok(1);
    }
    let total = (s.len() as u128 + sep.len() as u128) * n as u128;
    if total > (1 << 31) {
        return Err(st.run_error(b"resulting string too large"));
    }
    let mut out = Vec::with_capacity(total as usize);
    for k in 0..n {
        out.extend_from_slice(&s);
        if k + 1 < n {
            out.extend_from_slice(&sep);
        }
    }
    let r = st.new_str_value(&out)?;
    st.push(r);
    Ok(1)
}

fn s_sub(st: &mut State) -> LuaResult<usize> {
    let s = str_of(&st.arg(0)).unwrap_or_default();
    let l = s.len() as i64;
    let mut i = st.arg(1).as_int().unwrap_or(1);
    let mut j = st.arg(2).as_int().unwrap_or(-1);
    if i < 0 {
        i = (l + i + 1).max(1);
    } else if i == 0 {
        i = 1;
    }
    if j < 0 {
        j += l + 1;
    } else if j > l {
        j = l;
    }
    let r = if i > j {
        st.new_str_value(b"")?
    } else {
        st.new_str_value(&s[(i - 1) as usize..j as usize])?
    };
    st.push(r);
    Ok(1)
}

fn s_byte(st: &mut State) -> LuaResult<usize> {
    let s = str_of(&st.arg(0)).unwrap_or_default();
    let i = st.arg(1).as_int().unwrap_or(1);
    let j = st.arg(2).as_int().unwrap_or(i);
    let l = s.len() as i64;
    let i = if i < 0 { l + i + 1 } else { i }.max(1);
    let j = if j < 0 { l + j + 1 } else { j }.min(l);
    let mut n = 0;
    for k in i..=j {
        st.push(Value::int(s[(k - 1) as usize] as i64));
        n += 1;
    }
    Ok(n)
}

fn s_char(st: &mut State) -> LuaResult<usize> {
    let b: Vec<u8> = (0..st.nargs())
        .map(|i| st.arg(i).as_int().unwrap_or(0) as u8)
        .collect();
    let r = st.new_str_value(&b)?;
    st.push(r);
    Ok(1)
}

// ---- string arithmetic metamethods (lstrlib.c `stringmetamethods`) --------------------------

fn tonum(v: &Value) -> Option<Number> {
    match v.as_number() {
        Some(n) => Some(n),
        None => number::str2num(v.as_string()?.as_bytes()),
    }
}

fn f64_of(n: Number) -> f64 {
    match n {
        Number::Int(i) => i as f64,
        Number::Float(f) => f,
    }
}

fn str_arith(st: &mut State, ev: Tm, mtname: &str) -> LuaResult<usize> {
    let (a, b) = (st.arg(0), st.arg(1));
    if let (Some(x), Some(y)) = (tonum(&a), tonum(&b)) {
        let r = match (x, y) {
            (Number::Int(i), Number::Int(j)) if !matches!(ev, Tm::Div | Tm::Pow) => match ev {
                Tm::Add => Value::int(i.wrapping_add(j)),
                Tm::Sub => Value::int(i.wrapping_sub(j)),
                Tm::Mul => Value::int(i.wrapping_mul(j)),
                Tm::Mod => match number::imod(i, j) {
                    Some(r) => Value::int(r),
                    None => return Err(st.run_error(b"attempt to perform 'n%0'")),
                },
                Tm::IDiv => match number::idiv(i, j) {
                    Some(r) => Value::int(r),
                    None => return Err(st.run_error(b"attempt to divide by zero")),
                },
                Tm::Unm => Value::int(i.wrapping_neg()),
                _ => Value::NIL,
            },
            _ => {
                let (i, j) = (f64_of(x), f64_of(y));
                Value::float(match ev {
                    Tm::Add => i + j,
                    Tm::Sub => i - j,
                    Tm::Mul => i * j,
                    Tm::Div => i / j,
                    Tm::Pow => number::pow_lua(i, j),
                    Tm::Mod => number::fmod_lua(i, j),
                    Tm::IDiv => number::fidiv(i, j),
                    Tm::Unm => -i,
                    _ => 0.0,
                })
            }
        };
        st.push(r);
        return Ok(1);
    }
    // trymt
    let second_is_string = b.is_string();
    let mm = if second_is_string {
        Value::NIL
    } else {
        match st.metatable_of(&b) {
            Some(mt) => {
                let k = st.new_str_value(mtname.as_bytes())?;
                mt.get(&k)
            }
            None => Value::NIL,
        }
    };
    if mm.is_nil() {
        let t1 = String::from_utf8_lossy(type_name(a.type_tag())).into_owned();
        let t2 = String::from_utf8_lossy(type_name(b.type_tag())).into_owned();
        let mut m = where_(st, 1);
        m.extend_from_slice(
            format!("attempt to {} a '{t1}' with a '{t2}'", &mtname[2..]).as_bytes(),
        );
        let s = st.new_str_value(&m)?;
        return Err(st.throw_value(s));
    }
    st.check_stack(3)?;
    let func = st.top();
    st.push(mm);
    st.push(a);
    st.push(b);
    st.call(func, 1)?;
    Ok(1)
}

macro_rules! strmm {
    ($name:ident, $ev:expr, $mt:expr) => {
        fn $name(st: &mut State) -> LuaResult<usize> {
            str_arith(st, $ev, $mt)
        }
    };
}
strmm!(sm_add, Tm::Add, "__add");
strmm!(sm_sub, Tm::Sub, "__sub");
strmm!(sm_mul, Tm::Mul, "__mul");
strmm!(sm_mod, Tm::Mod, "__mod");
strmm!(sm_pow, Tm::Pow, "__pow");
strmm!(sm_div, Tm::Div, "__div");
strmm!(sm_idiv, Tm::IDiv, "__idiv");
strmm!(sm_unm, Tm::Unm, "__unm");

fn set_fn(st: &mut State, t: TableRef, name: &str, f: NativeFn) {
    let k = st.new_str_value(name.as_bytes()).expect("alloc");
    st.raw_set(t, &k, &Value::native(f)).expect("set");
}

fn set_val(st: &mut State, t: TableRef, name: &str, v: Value) {
    let k = st.new_str_value(name.as_bytes()).expect("alloc");
    st.raw_set(t, &k, &v).expect("set");
}

fn new_lib(st: &mut State, name: &str, fns: &[(&str, NativeFn)]) -> TableRef {
    let t = st.new_table().expect("alloc");
    for (n, f) in fns {
        set_fn(st, t, n, *f);
    }
    let g = st.globals();
    set_val(st, g, name, Value::table(t));
    t
}

/// Install the minimal library into the globals of `st`.
pub fn install(st: &mut State) {
    let g = st.globals();
    let base: [(&str, NativeFn); 20] = [
        ("print", n_print),
        ("type", n_type),
        ("tostring", n_tostring),
        ("tonumber", n_tonumber),
        ("setmetatable", n_setmetatable),
        ("getmetatable", n_getmetatable),
        ("rawequal", n_rawequal),
        ("rawget", n_rawget),
        ("rawset", n_rawset),
        ("rawlen", n_rawlen),
        ("error", n_error),
        ("assert", n_assert),
        ("select", n_select),
        ("pcall", n_pcall),
        ("xpcall", n_xpcall),
        ("collectgarbage", n_collectgarbage),
        ("next", n_next),
        ("pairs", n_pairs),
        ("ipairs", n_ipairs),
        ("load", n_load),
    ];
    for (n, f) in base {
        set_fn(st, g, n, f);
    }
    set_val(st, g, "_G", Value::table(g));
    new_lib(st, "table", &[("unpack", n_unpack), ("pack", n_pack)]);
    let m = new_lib(
        st,
        "math",
        &[
            ("type", m_type),
            ("tointeger", m_tointeger),
            ("floor", m_floor),
            ("abs", m_abs),
            ("fmod", m_fmod),
        ],
    );
    set_val(st, m, "mininteger", Value::int(i64::MIN));
    set_val(st, m, "maxinteger", Value::int(i64::MAX));
    set_val(st, m, "huge", Value::float(f64::INFINITY));
    set_val(st, m, "pi", Value::float(std::f64::consts::PI));
    let s = new_lib(
        st,
        "string",
        &[
            ("len", s_len),
            ("rep", s_rep),
            ("sub", s_sub),
            ("byte", s_byte),
            ("char", s_char),
        ],
    );
    new_lib(st, "io", &[("write", n_io_write)]);
    new_lib(st, "os", &[("clock", os_clock)]);
    // string metatable: __index = string, plus the arithmetic metamethods
    let mt = st.new_table().expect("alloc");
    set_val(st, mt, "__index", Value::table(s));
    for (n, f) in [
        ("__add", sm_add as NativeFn),
        ("__sub", sm_sub),
        ("__mul", sm_mul),
        ("__mod", sm_mod),
        ("__pow", sm_pow),
        ("__div", sm_div),
        ("__idiv", sm_idiv),
        ("__unm", sm_unm),
    ] {
        set_fn(st, mt, n, f);
    }
    st.set_type_metatable(LuaType::String, Some(mt));
}

fn os_clock(st: &mut State) -> LuaResult<usize> {
    thread_local! {
        static START: std::time::Instant = std::time::Instant::now();
    }
    let t = START.with(|s| s.elapsed().as_secs_f64());
    st.push(Value::float(t));
    Ok(1)
}

/// A new state with the real compiler/interpreter and the minimal library.
pub fn new_state(stress: bool) -> LuaState {
    let mut st = rustynail::new_state(0).expect("state");
    install(&mut st);
    if stress {
        st.heap().set_gc_stress(true);
    }
    st
}

/// Run a chunk like `lua file.lua` would: returns `Err(message)` when it raises an error.
/// Output is accumulated by [`emit`].
pub fn run_chunk(src: &[u8], chunkname: &[u8], stress: bool) -> Result<(), Vec<u8>> {
    let mut st = new_state(stress);
    let top = st.top();
    let status = st.load_chunk(src, chunkname, None);
    let result = if status != Status::Ok {
        Err(error_text(&mut st, top))
    } else {
        let status = st.pcall(top, 0, 0);
        if status != Status::Ok {
            Err(error_text(&mut st, top))
        } else {
            Ok(())
        }
    };
    st.set_top(top);
    drop(st); // closing the state runs the remaining finalizers (they may print)
    result
}

fn error_text(st: &mut State, at: StkId) -> Vec<u8> {
    let v = st.get(at);
    str_of(&v).unwrap_or_else(|| b"(null)".to_vec())
}
