//! `os` library (port of `loslib.c`; identical in 5.4 and 5.5 apart from integer types).
#![forbid(unsafe_code)]
#![allow(clippy::needless_late_init, clippy::explicit_counter_loop)]

use crate::io_ffi;
use crate::lib_io::{exec_result, file_result};
use crate::strtime::{self, DateError, DateOut, Tm};
use rn_core::state::{NativeFn, State};
use rn_core::status::LuaResult;
use rn_core::value::{LuaType, Value};

fn os_execute(st: &mut State) -> LuaResult<usize> {
    let cmd = st.opt_str(1)?.map(|s| s.get().as_bytes());
    io_ffi::set_errno(0);
    let stat = io_ffi::system(cmd);
    match cmd {
        Some(_) => match stat {
            Some(s) => exec_result(st, s),
            None => file_result(st, false, None),
        },
        None => {
            st.push(Value::boolean(stat.unwrap_or(0) != 0));
            Ok(1)
        }
    }
}

fn os_remove(st: &mut State) -> LuaResult<usize> {
    let f = st.check_str(1)?.get().as_bytes();
    io_ffi::set_errno(0);
    let ok = io_ffi::remove(f);
    file_result(st, ok, Some(f))
}

fn os_rename(st: &mut State) -> LuaResult<usize> {
    let a = st.check_str(1)?.get().as_bytes();
    let b = st.check_str(2)?.get().as_bytes();
    io_ffi::set_errno(0);
    let ok = io_ffi::rename(a, b);
    file_result(st, ok, None)
}

fn os_tmpname(st: &mut State) -> LuaResult<usize> {
    match io_ffi::tmpnam() {
        Some(n) => {
            st.push_bytes(&n)?;
            Ok(1)
        }
        None => Err(st.l_error(b"unable to generate a unique filename")),
    }
}

fn os_getenv(st: &mut State) -> LuaResult<usize> {
    use std::os::unix::ffi::{OsStrExt, OsStringExt};
    let name = st.check_str(1)?.get().as_bytes();
    let name = match name.iter().position(|&c| c == 0) {
        Some(i) => &name[..i],
        None => name,
    };
    if name.is_empty() || name.contains(&b'=') {
        st.push(Value::NIL);
        return Ok(1);
    }
    match std::env::var_os(std::ffi::OsStr::from_bytes(name)) {
        Some(v) => {
            st.push_bytes(&v.into_vec())?;
        }
        None => st.push(Value::NIL),
    }
    Ok(1)
}

fn os_clock(st: &mut State) -> LuaResult<usize> {
    st.push(Value::float(io_ffi::clock_secs()));
    Ok(1)
}

fn set_field(st: &mut State, t: Value, key: &str, v: i64) -> LuaResult<()> {
    st.set_index_str(t, key.as_bytes(), Value::int(v))
}

fn set_all_fields(st: &mut State, t: Value, tm: &Tm) -> LuaResult<()> {
    let f = tm.fields();
    set_field(st, t, "year", f.year)?;
    set_field(st, t, "month", f.month)?;
    set_field(st, t, "day", f.day)?;
    set_field(st, t, "hour", f.hour)?;
    set_field(st, t, "min", f.min)?;
    set_field(st, t, "sec", f.sec)?;
    set_field(st, t, "yday", f.yday)?;
    set_field(st, t, "wday", f.wday)?;
    if let Some(b) = f.isdst {
        st.set_index_str(t, b"isdst", Value::boolean(b))?;
    }
    Ok(())
}

fn get_bool_field(st: &mut State, t: Value, key: &str) -> LuaResult<i32> {
    let v = st.index_str(t, key.as_bytes())?;
    Ok(if v.is_nil() { -1 } else { v.is_truthy() as i32 })
}

fn get_field(st: &mut State, t: Value, key: &str, d: i64, delta: i64) -> LuaResult<i32> {
    let v = st.index_str(t, key.as_bytes())?;
    match State::to_integer(&v) {
        None => {
            if !v.is_nil() {
                return Err(st.l_error(format!("field '{key}' is not an integer").as_bytes()));
            } else if d < 0 {
                return Err(st.l_error(format!("field '{key}' missing in date table").as_bytes()));
            }
            Ok(d as i32)
        }
        Some(res) => match strtime::field_to_c(res, delta) {
            Some(c) => Ok(c),
            None => Err(st.l_error(format!("field '{key}' is out-of-bound").as_bytes())),
        },
    }
}

fn os_date(st: &mut State) -> LuaResult<usize> {
    let fmt: &[u8] = match st.opt_str(1)? {
        Some(s) => s.get().as_bytes(),
        None => b"%c",
    };
    let t = if st.arg_is_none_or_nil(2) {
        strtime::now()
    } else {
        st.check_integer(2)?
    };
    match strtime::date(fmt, t) {
        Ok(DateOut::Table(tm)) => {
            let tbl = st.new_table_sized(0, 9)?;
            let tv = Value::table(tbl);
            st.push(tv);
            set_all_fields(st, tv, &tm)?;
            Ok(1)
        }
        Ok(DateOut::Text(b)) => {
            st.push_bytes(&b)?;
            Ok(1)
        }
        Err(DateError::Arg(n, m)) => Err(st.arg_error(n, &m)),
        Err(DateError::Msg(m)) => Err(st.l_error(&m)),
    }
}

fn os_time(st: &mut State) -> LuaResult<usize> {
    let t;
    if st.arg_is_none_or_nil(1) {
        t = strtime::now();
    } else {
        let tv = st.check_type(1, LuaType::Table)?;
        st.set_top(st.ci().r().func + 2);
        let year = get_field(st, tv, "year", -1, 1900)?;
        let mon = get_field(st, tv, "month", -1, 1)?;
        let mday = get_field(st, tv, "day", -1, 0)?;
        let hour = get_field(st, tv, "hour", 12, 0)?;
        let min = get_field(st, tv, "min", 0, 0)?;
        let sec = get_field(st, tv, "sec", 0, 0)?;
        let isdst = get_bool_field(st, tv, "isdst")?;
        let tm = Tm::from_c_fields(year, mon, mday, hour, min, sec, isdst);
        let (tt, tm) = strtime::time_from_tm(tm);
        set_all_fields(st, tv, &tm)?;
        t = tt;
    }
    if !strtime::time_ok(t) {
        return Err(st.l_error(b"time result cannot be represented in this installation"));
    }
    st.push(Value::int(t));
    Ok(1)
}

fn os_difftime(st: &mut State) -> LuaResult<usize> {
    let t1 = st.check_integer(1)?;
    let t2 = st.check_integer(2)?;
    st.push(Value::float(strtime::difftime(t1, t2)));
    Ok(1)
}

fn os_setlocale(st: &mut State) -> LuaResult<usize> {
    let l = st.opt_str(1)?.map(|s| s.get().as_bytes());
    let op = st.check_option(
        2,
        Some("all"),
        &["all", "collate", "ctype", "monetary", "numeric", "time"],
    )?;
    let cat = [
        libc::LC_ALL,
        libc::LC_COLLATE,
        libc::LC_CTYPE,
        libc::LC_MONETARY,
        libc::LC_NUMERIC,
        libc::LC_TIME,
    ][op];
    match io_ffi::setlocale(cat, l) {
        Some(r) => {
            st.push_bytes(&r)?;
        }
        None => st.push(Value::NIL),
    }
    Ok(1)
}

fn os_exit(st: &mut State) -> LuaResult<usize> {
    let a = st.arg(0);
    let status = if let Some(b) = a.as_bool() {
        if b {
            0
        } else {
            1
        }
    } else {
        st.opt_integer(1, 0)? as i32
    };
    if st.arg(1).is_truthy() {
        st.close_state();
    }
    io_ffi::exit(status)
}

const SYSLIB: &[(&str, Option<NativeFn>)] = &[
    ("clock", Some(os_clock)),
    ("date", Some(os_date)),
    ("difftime", Some(os_difftime)),
    ("execute", Some(os_execute)),
    ("exit", Some(os_exit)),
    ("getenv", Some(os_getenv)),
    ("remove", Some(os_remove)),
    ("rename", Some(os_rename)),
    ("setlocale", Some(os_setlocale)),
    ("time", Some(os_time)),
    ("tmpname", Some(os_tmpname)),
];

/// `luaopen_os`.
pub fn open(st: &mut State) -> LuaResult<usize> {
    let t = st.new_table_sized(0, SYSLIB.len())?;
    st.push(Value::table(t));
    st.set_funcs(t, SYSLIB)?;
    Ok(1)
}
