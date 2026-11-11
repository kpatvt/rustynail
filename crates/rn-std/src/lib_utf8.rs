//! Library `utf8` (port of `lutf8lib.c`) over the pure engine in [`crate::utf8`].
#![forbid(unsafe_code)]

use rn_core::state::{NativeFn, State};
use rn_core::status::{LuaResult, Status};
use rn_core::value::Value;

use crate::lib_string::l_checkstack;
use crate::utf8::{self as core, LenResult, OffsetResult, Utf8Error};

fn raise(st: &mut State, e: Utf8Error) -> Status {
    match e {
        Utf8Error::Arg(n, m) => st.arg_error(n as usize, m.as_bytes()),
        Utf8Error::Msg(m) => st.l_error(m.as_bytes()),
    }
}

fn utflen(st: &mut State) -> LuaResult<usize> {
    let s = st.check_str(1)?.get().as_bytes();
    let i = st.opt_integer(2, 1)?;
    let j = st.opt_integer(3, -1)?;
    let lax = st.arg(3).is_truthy();
    match core::len(s, i, j, lax) {
        Ok(LenResult::Count(n)) => {
            st.push(Value::int(n));
            Ok(1)
        }
        Ok(LenResult::Fail(p)) => {
            st.push(Value::NIL);
            st.push(Value::int(p));
            Ok(2)
        }
        Err(e) => Err(raise(st, e)),
    }
}

fn codepoint(st: &mut State) -> LuaResult<usize> {
    let s = st.check_str(1)?.get().as_bytes();
    let i = st.opt_integer(2, 1)?;
    // default of `j` is the translated `i`; the core takes the raw optional argument
    let j = if st.arg_is_none_or_nil(3) {
        None
    } else {
        Some(st.check_integer(3)?)
    };
    let lax = st.arg(3).is_truthy();
    // the reference checks the slice size before decoding
    let l = s.len();
    let posi = core::u_posrelat(i, l);
    let pose = match j {
        Some(j) => core::u_posrelat(j, l),
        None => posi,
    };
    if posi >= 1 && pose <= l as i64 && posi <= pose && pose - posi >= i32::MAX as i64 {
        return Err(st.l_error(b"string slice too long"));
    }
    match core::codepoint(s, i, j, lax) {
        Ok(v) => {
            l_checkstack(st, v.len(), "string slice too long")?;
            for c in &v {
                st.push(Value::int(*c as i64));
            }
            Ok(v.len())
        }
        Err(e) => Err(raise(st, e)),
    }
}

fn utfchar(st: &mut State) -> LuaResult<usize> {
    let n = st.nargs();
    let mut codes: Vec<i64> = Vec::with_capacity(n);
    for i in 1..=n {
        let c = st.check_integer(i)?;
        if (c as u64) > core::MAXUTF as u64 {
            return Err(st.arg_error(i, b"value out of range"));
        }
        codes.push(c);
    }
    match core::char_from(&codes) {
        Ok(b) => {
            st.push_bytes(&b)?;
            Ok(1)
        }
        Err(e) => Err(raise(st, e)),
    }
}

fn byteoffset(st: &mut State) -> LuaResult<usize> {
    let s = st.check_str(1)?.get().as_bytes();
    let n = st.check_integer(2)?;
    let i = if st.arg_is_none_or_nil(3) {
        None
    } else {
        Some(st.check_integer(3)?)
    };
    match core::offset(s, n, i) {
        Ok(OffsetResult::Fail) => {
            st.push(Value::NIL);
            Ok(1)
        }
        Ok(OffsetResult::Found(a, b)) => {
            st.push(Value::int(a));
            if cfg!(rustynail_lua54) {
                Ok(1)
            } else {
                st.push(Value::int(b));
                Ok(2)
            }
        }
        Err(e) => Err(raise(st, e)),
    }
}

fn iter_aux(st: &mut State, strict: bool) -> LuaResult<usize> {
    let s = st.check_str(1)?.get().as_bytes();
    let n = State::to_integer(&st.arg(1)).unwrap_or(0);
    match core::codes_next(s, n, strict) {
        Ok(Some((pos, code))) => {
            st.push(Value::int(pos));
            st.push(Value::int(code as i64));
            Ok(2)
        }
        Ok(None) => Ok(0),
        Err(e) => Err(raise(st, e)),
    }
}

fn iter_auxstrict(st: &mut State) -> LuaResult<usize> {
    iter_aux(st, true)
}

fn iter_auxlax(st: &mut State) -> LuaResult<usize> {
    iter_aux(st, false)
}

fn iter_codes(st: &mut State) -> LuaResult<usize> {
    let lax = st.arg(1).is_truthy();
    let s = st.check_str(1)?;
    if let Err(e) = core::codes_check(s.get().as_bytes()) {
        return Err(raise(st, e));
    }
    let f: NativeFn = if lax { iter_auxlax } else { iter_auxstrict };
    st.push(Value::native(f));
    st.push(Value::string(s));
    st.push(Value::int(0));
    Ok(3)
}

/// `luaopen_utf8`.
pub fn open(st: &mut State) -> LuaResult<usize> {
    let lib = st.new_table_sized(0, 6)?;
    st.push(Value::table(lib));
    st.set_funcs(
        lib,
        &[
            ("offset", Some(byteoffset)),
            ("codepoint", Some(codepoint)),
            ("char", Some(utfchar)),
            ("len", Some(utflen)),
            ("codes", Some(iter_codes)),
        ],
    )?;
    let pat = st.new_str_value(core::CHARPATTERN)?;
    st.push(pat);
    st.raw_set_str(lib, b"charpattern", pat)?;
    st.pop();
    Ok(1)
}
