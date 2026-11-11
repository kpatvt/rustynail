//! `os.date` / `os.time` / `os.difftime` cores (port of the time section of `loslib.c`).
//!
//! This is the one `rn-std` module that talks to libc (`localtime_r`, `gmtime_r`, `mktime`,
//! `strftime`, `difftime`), exactly like the reference built with `LUA_USE_POSIX`; all `unsafe`
//! is confined to the small wrappers at the top, each with a SAFETY comment (docs/UNSAFE.md).
#![allow(unsafe_code)]

use std::ffi::c_char;

/// C99 `LUA_STRFTIMEOPTIONS` (one-char options, then `||`, two-char options).
const STRFTIME_OPTIONS: &[u8] =
    b"aAbBcCdDeFgGhHIjmMnprRStTuUVwWxXyYzZ%||EcECExEXEyEYOdOeOHOIOmOMOSOuOUOVOwOWOy";

/// Maximum size for an individual `strftime` item (`SIZETIMEFMT`).
const SIZETIMEFMT: usize = 250;

/// A broken-down time (`struct tm`).
#[derive(Clone)]
pub struct Tm(libc::tm);

impl std::fmt::Debug for Tm {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Tm({}-{}-{} {}:{}:{} wday={} yday={} isdst={})",
            self.0.tm_year,
            self.0.tm_mon,
            self.0.tm_mday,
            self.0.tm_hour,
            self.0.tm_min,
            self.0.tm_sec,
            self.0.tm_wday,
            self.0.tm_yday,
            self.0.tm_isdst
        )
    }
}

/// The values `setallfields` stores into a date table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Fields {
    pub year: i64,
    pub month: i64,
    pub day: i64,
    pub hour: i64,
    pub min: i64,
    pub sec: i64,
    pub yday: i64,
    pub wday: i64,
    /// `None` when `tm_isdst < 0` (field not set).
    pub isdst: Option<bool>,
}

impl Tm {
    fn zeroed() -> Tm {
        // SAFETY: `libc::tm` is a plain C struct of integers and (on glibc) one `*const c_char`;
        // the all-zero bit pattern is a valid value for every field (NULL zone pointer).
        Tm(unsafe { std::mem::zeroed() })
    }

    /// Build a `struct tm` from already-adjusted C fields (`tm_year` is years since 1900, `tm_mon` 0-based).
    pub fn from_c_fields(
        year: i32,
        mon: i32,
        mday: i32,
        hour: i32,
        min: i32,
        sec: i32,
        isdst: i32,
    ) -> Tm {
        let mut t = Tm::zeroed();
        t.0.tm_year = year;
        t.0.tm_mon = mon;
        t.0.tm_mday = mday;
        t.0.tm_hour = hour;
        t.0.tm_min = min;
        t.0.tm_sec = sec;
        t.0.tm_isdst = isdst;
        t
    }

    /// `setallfields` values.
    pub fn fields(&self) -> Fields {
        let t = &self.0;
        Fields {
            year: t.tm_year as i64 + 1900,
            month: t.tm_mon as i64 + 1,
            day: t.tm_mday as i64,
            hour: t.tm_hour as i64,
            min: t.tm_min as i64,
            sec: t.tm_sec as i64,
            yday: t.tm_yday as i64 + 1,
            wday: t.tm_wday as i64 + 1,
            isdst: if t.tm_isdst < 0 {
                None
            } else {
                Some(t.tm_isdst != 0)
            },
        }
    }

    /// `mktime`: normalises the fields in place and returns the `time_t` (`-1` on failure).
    pub fn mktime(&mut self) -> i64 {
        // SAFETY: `self.0` is a valid, initialised `struct tm`; mktime only reads/writes its fields
        // (`tm_zone` is zeroed or set by libc itself).
        unsafe { libc::mktime(&mut self.0) as i64 }
    }
}

/// Re-read the `TZ` environment variable (libc `tzset`).
pub fn tzset() {
    extern "C" {
        fn tzset();
    }
    // SAFETY: `tzset` takes no arguments and only updates libc's internal time-zone state.
    unsafe { tzset() }
}

/// Current time (`time(NULL)`).
pub fn now() -> i64 {
    // SAFETY: `time(NULL)` is always valid.
    unsafe { libc::time(std::ptr::null_mut()) as i64 }
}

/// `gmtime_r`; `None` if the time cannot be represented.
pub fn gmtime(t: i64) -> Option<Tm> {
    let mut tm = Tm::zeroed();
    let tt = t as libc::time_t;
    // SAFETY: both pointers refer to live, properly aligned locals.
    let r = unsafe { libc::gmtime_r(&tt, &mut tm.0) };
    if r.is_null() {
        None
    } else {
        Some(tm)
    }
}

/// `localtime_r`; `None` if the time cannot be represented.
pub fn localtime(t: i64) -> Option<Tm> {
    let mut tm = Tm::zeroed();
    let tt = t as libc::time_t;
    // SAFETY: both pointers refer to live, properly aligned locals.
    let r = unsafe { libc::localtime_r(&tt, &mut tm.0) };
    if r.is_null() {
        None
    } else {
        Some(tm)
    }
}

/// `difftime`.
pub fn difftime(t1: i64, t2: i64) -> f64 {
    // SAFETY: pure function on two integers.
    unsafe { libc::difftime(t1 as libc::time_t, t2 as libc::time_t) }
}

/// `strftime(buff, SIZETIMEFMT, cc, stm)` for one validated conversion `cc` (e.g. `b"%Y"`).
pub fn strftime(cc: &[u8], tm: &Tm) -> Vec<u8> {
    debug_assert!(!cc.contains(&0));
    let mut f = cc.to_vec();
    f.push(0);
    let mut buf = vec![0u8; SIZETIMEFMT];
    // SAFETY: `buf` is a live buffer of `SIZETIMEFMT` bytes, `f` is NUL-terminated and `tm.0` is a
    // valid `struct tm` filled by gmtime_r/localtime_r/mktime (or fully zeroed).
    let n = unsafe {
        libc::strftime(
            buf.as_mut_ptr() as *mut c_char,
            SIZETIMEFMT,
            f.as_ptr() as *const c_char,
            &tm.0,
        )
    };
    buf.truncate(n.min(SIZETIMEFMT));
    buf
}

/// Errors of `os.date` (and helpers).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DateError {
    /// `luaL_argerror(L, arg, msg)`.
    Arg(usize, Vec<u8>),
    /// `luaL_error` message.
    Msg(Vec<u8>),
}

fn cstr(s: &[u8]) -> &[u8] {
    match s.iter().position(|&c| c == 0) {
        Some(i) => &s[..i],
        None => s,
    }
}

/// `checkoption`: returns the length of the valid specifier at the start of `conv`
/// (`conv` is everything after the `%`).
pub fn check_option(conv: &[u8]) -> Result<usize, DateError> {
    let mut oplen = 1usize;
    let mut i = 0usize;
    while i < STRFTIME_OPTIONS.len() && oplen <= conv.len() {
        let c = STRFTIME_OPTIONS[i];
        if c == b'|' {
            oplen += 1;
        } else if i + oplen <= STRFTIME_OPTIONS.len()
            && conv[..oplen] == STRFTIME_OPTIONS[i..i + oplen]
        {
            return Ok(oplen);
        }
        i += oplen;
    }
    let mut m = b"invalid conversion specifier '%".to_vec();
    m.extend_from_slice(cstr(conv));
    m.push(b'\'');
    Err(DateError::Arg(1, m))
}

/// Result of `os.date`.
#[derive(Debug)]
pub enum DateOut {
    /// `"*t"` / `"!*t"`: the broken-down time.
    Table(Tm),
    Text(Vec<u8>),
}

/// `os.date(format, t)` with `t` already resolved (`time(NULL)` default) and checked.
pub fn date(format: &[u8], t: i64) -> Result<DateOut, DateError> {
    let (utc, s) = if format.first() == Some(&b'!') {
        (true, &format[1..])
    } else {
        (false, format)
    };
    let stm = if utc { gmtime(t) } else { localtime(t) };
    let Some(stm) = stm else {
        return Err(DateError::Msg(
            b"date result cannot be represented in this installation".to_vec(),
        ));
    };
    if cstr(s) == b"*t" {
        return Ok(DateOut::Table(stm));
    }
    let mut out = Vec::new();
    let mut i = 0;
    while i < s.len() {
        if s[i] != b'%' {
            out.push(s[i]);
            i += 1;
        } else {
            i += 1;
            let oplen = check_option(&s[i..])?;
            let mut cc = vec![b'%'];
            cc.extend_from_slice(&s[i..i + oplen]);
            out.extend_from_slice(&strftime(&cc, &stm));
            i += oplen;
        }
    }
    Ok(DateOut::Text(out))
}

/// `getfield`'s range check: `res` is the Lua integer, `delta` the offset (1900, 1 or 0).
/// Returns the C `int` value (`res - delta`) or `None` if "out-of-bound".
pub fn field_to_c(res: i64, delta: i64) -> Option<i32> {
    let ok = if res >= 0 {
        res.checked_sub(delta).is_some_and(|v| v <= i32::MAX as i64)
    } else {
        (i32::MIN as i64 + delta) <= res
    };
    if !ok {
        return None;
    }
    Some((res - delta) as i32)
}

/// `os.time(table)` core: normalise via `mktime`; returns `(time, normalised tm)`. The caller must
/// raise "time result cannot be represented in this installation" when [`time_ok`] is false (after
/// updating the table, as the reference does).
pub fn time_from_tm(mut tm: Tm) -> (i64, Tm) {
    let t = tm.mktime();
    (t, tm)
}

/// The reference's validity test on the `mktime` result.
pub fn time_ok(t: i64) -> bool {
    t != -1
}
