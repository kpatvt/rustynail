//! `utf8` library cores (port of `lutf8lib.c`, plus `luaO_utf8esc`). Pure functions over `&[u8]`.
//!
//! `Utf8Error::Arg` corresponds to `luaL_argcheck` failures (the VM formats
//! `bad argument #n to 'fname' (msg)`); `Utf8Error::Msg` to `luaL_error` messages.
//! Version differences: 5.5 `utf8.offset` returns start *and* end positions (see [`offset`]).
#![forbid(unsafe_code)]

pub const MAXUNICODE: u32 = 0x10FFFF;
pub const MAXUTF: u32 = 0x7FFF_FFFF;
pub const MSG_INVALID: &str = "invalid UTF-8 code";
/// `UTF8PATT`: pattern matching a single UTF-8 character (`utf8.charpattern`).
pub const CHARPATTERN: &[u8] = b"[\0-\x7F\xC2-\xFD][\x80-\xBF]*";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Utf8Error {
    /// Argument error: argument index (1-based) and message.
    Arg(u8, &'static str),
    /// Plain `luaL_error` message (without position).
    Msg(&'static str),
}

fn at(s: &[u8], i: usize) -> u8 {
    s.get(i).copied().unwrap_or(0)
}

fn iscont(c: u8) -> bool {
    c & 0xC0 == 0x80
}

/// `u_posrelat`.
pub fn u_posrelat(pos: i64, len: usize) -> i64 {
    if pos >= 0 {
        pos
    } else if (pos as i128).unsigned_abs() > len as u128 {
        0
    } else {
        len as i64 + pos + 1
    }
}

/// `utf8_decode`: decode one sequence at `pos`; returns `(code point, next position)` or `None`
/// if invalid. Reading past the end sees a NUL terminator, like the C string.
pub fn decode(s: &[u8], pos: usize, strict: bool) -> Option<(u32, usize)> {
    const LIMITS: [u32; 6] = [u32::MAX, 0x80, 0x800, 0x10000, 0x200000, 0x4000000];
    let mut c = at(s, pos) as u32;
    let mut res: u32 = 0;
    let mut next = pos;
    if c < 0x80 {
        res = c;
    } else if c >= 0xfe {
        return None;
    } else {
        let mut count = 0usize;
        while c & 0x40 != 0 {
            count += 1;
            let cc = at(s, pos + count);
            if !iscont(cc) {
                return None;
            }
            res = (res << 6) | (cc as u32 & 0x3F);
            c <<= 1;
        }
        res |= (c & 0x7F) << (count * 5);
        if res > MAXUTF || res < LIMITS[count] {
            return None;
        }
        next += count;
    }
    if strict && (res > MAXUNICODE || (0xD800..=0xDFFF).contains(&res)) {
        return None;
    }
    Some((res, next + 1))
}

/// `luaO_utf8esc`: encode `x` (< 2^31) as (extended) UTF-8.
pub fn encode(x: u32) -> Vec<u8> {
    debug_assert!(x <= MAXUTF);
    if x < 0x80 {
        return vec![x as u8];
    }
    let mut buf = [0u8; 8];
    let mut n = 1usize;
    let mut x = x;
    let mut mfb: u32 = 0x3f; // maximum that fits in first byte
    loop {
        buf[8 - n] = (0x80 | (x & 0x3f)) as u8;
        n += 1;
        x >>= 6;
        mfb >>= 1;
        if x <= mfb {
            break;
        }
    }
    buf[8 - n] = (((!mfb) << 1) | x) as u8;
    buf[8 - n..].to_vec()
}

/// Result of [`len`].
#[derive(Debug, PartialEq, Eq)]
pub enum LenResult {
    Count(i64),
    /// `nil, position` (invalid byte sequence).
    Fail(i64),
}

/// `utf8.len(s [, i [, j [, lax]]])`; `i`/`j` are the raw arguments (defaults 1 and -1).
pub fn len(s: &[u8], i: i64, j: i64, lax: bool) -> Result<LenResult, Utf8Error> {
    let l = s.len();
    let mut posi = u_posrelat(i, l);
    let mut posj = u_posrelat(j, l);
    if !(1 <= posi && {
        posi -= 1;
        posi <= l as i64
    }) {
        return Err(Utf8Error::Arg(2, "initial position out of bounds"));
    }
    posj -= 1;
    if posj >= l as i64 {
        return Err(Utf8Error::Arg(3, "final position out of bounds"));
    }
    let mut n = 0;
    while posi <= posj {
        match decode(s, posi as usize, !lax) {
            None => return Ok(LenResult::Fail(posi + 1)),
            Some((_, next)) => posi = next as i64,
        }
        n += 1;
    }
    Ok(LenResult::Count(n))
}

/// `utf8.codepoint(s [, i [, j [, lax]]])`; `i` raw argument (default 1), `j` raw or `None` (= `i` after translation).
pub fn codepoint(s: &[u8], i: i64, j: Option<i64>, lax: bool) -> Result<Vec<u32>, Utf8Error> {
    let l = s.len();
    let posi = u_posrelat(i, l);
    let pose = u_posrelat(j.unwrap_or(posi), l);
    if posi < 1 {
        return Err(Utf8Error::Arg(2, "out of bounds"));
    }
    if pose > l as i64 {
        return Err(Utf8Error::Arg(3, "out of bounds"));
    }
    if posi > pose {
        return Ok(Vec::new());
    }
    if pose - posi >= i32::MAX as i64 {
        return Err(Utf8Error::Msg("string slice too long"));
    }
    if pose - posi + 1 >= 1_000_000 {
        // luaL_checkstack: LUAI_MAXSTACK is 1,000,000
        return Err(Utf8Error::Msg("stack overflow (string slice too long)"));
    }
    let mut out = Vec::new();
    let se = pose as usize;
    let mut p = (posi - 1) as usize;
    while p < se {
        match decode(s, p, !lax) {
            None => return Err(Utf8Error::Msg(MSG_INVALID)),
            Some((code, next)) => {
                out.push(code);
                p = next;
            }
        }
    }
    Ok(out)
}

/// `utf8.char`: `codes` are the raw integer arguments.
pub fn char_from(codes: &[i64]) -> Result<Vec<u8>, Utf8Error> {
    let mut out = Vec::new();
    for (i, &c) in codes.iter().enumerate() {
        if (c as u64) > MAXUTF as u64 {
            return Err(Utf8Error::Arg((i + 1).min(255) as u8, "value out of range"));
        }
        out.extend_from_slice(&encode(c as u32));
    }
    Ok(out)
}

/// Result of [`offset`].
#[derive(Debug, PartialEq, Eq)]
pub enum OffsetResult {
    /// `nil` (fail).
    Fail,
    /// Start position and (5.5 only; 5.4 ignores it) end position of the character.
    Found(i64, i64),
}

/// `utf8.offset(s, n [, i])`; `i` is the raw argument or `None` for the default.
pub fn offset(s: &[u8], n: i64, i: Option<i64>) -> Result<OffsetResult, Utf8Error> {
    let l = s.len() as i64;
    let mut n = n;
    let def = if n >= 0 { 1 } else { l + 1 };
    let mut posi = u_posrelat(i.unwrap_or(def), s.len());
    if !(1 <= posi && {
        posi -= 1;
        posi <= l
    }) {
        return Err(Utf8Error::Arg(3, "position out of bounds"));
    }
    let cont = |p: i64| iscont(at(s, p as usize));
    if n == 0 {
        while posi > 0 && cont(posi) {
            posi -= 1;
        }
    } else {
        if cont(posi) {
            return Err(Utf8Error::Msg("initial position is a continuation byte"));
        }
        if n < 0 {
            while n < 0 && posi > 0 {
                loop {
                    posi -= 1;
                    if !(posi > 0 && cont(posi)) {
                        break;
                    }
                }
                n += 1;
            }
        } else {
            n -= 1;
            while n > 0 && posi < l {
                loop {
                    posi += 1;
                    if !cont(posi) {
                        break;
                    }
                }
                n -= 1;
            }
        }
    }
    if n != 0 {
        return Ok(OffsetResult::Fail);
    }
    let start = posi + 1;
    if cfg!(rustynail_lua54) {
        return Ok(OffsetResult::Found(start, start));
    }
    if at(s, posi as usize) & 0x80 != 0 {
        if iscont(at(s, posi as usize)) {
            return Err(Utf8Error::Msg("initial position is a continuation byte"));
        }
        while cont(posi + 1) {
            posi += 1;
        }
    }
    Ok(OffsetResult::Found(start, posi + 1))
}

/// Check done by `utf8.codes` before iterating: `luaL_argcheck(!iscontp(s), 1, MSGInvalid)`.
pub fn codes_check(s: &[u8]) -> Result<(), Utf8Error> {
    if iscont(at(s, 0)) {
        Err(Utf8Error::Arg(1, MSG_INVALID))
    } else {
        Ok(())
    }
}

/// `iter_aux`: one step of the `utf8.codes` iterator. `n` is `lua_tointeger(arg 2)` (the previous
/// position). Returns `(position, code point)` or `None` when finished.
pub fn codes_next(s: &[u8], n: i64, strict: bool) -> Result<Option<(i64, u32)>, Utf8Error> {
    let len = s.len();
    let mut n = n as u64; // lua_Unsigned cast: negative becomes huge
    if n < len as u64 {
        while iscont(at(s, n as usize)) {
            n += 1;
        }
    }
    if n >= len as u64 {
        return Ok(None);
    }
    let n = n as usize;
    match decode(s, n, strict) {
        Some((code, next)) if !iscont(at(s, next)) => Ok(Some((n as i64 + 1, code))),
        _ => Err(Utf8Error::Msg(MSG_INVALID)),
    }
}
