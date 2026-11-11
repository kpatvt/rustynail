//! `string.format` engine (port of the format section of `lstrlib.c`; identical in 5.4.9 and 5.5.1).
//!
//! Numbers are formatted by libc `snprintf` through [`crate::cfmt`], exactly like the reference
//! (`lua_number2strx` is `snprintf` on C99 builds), so results are bit-exact by construction.
//! Argument access goes through [`FormatSource`] so the VM keeps its own conversion semantics
//! (`luaL_checkinteger`, `luaL_tolstring` with `__tostring`/`__name`, ...).
#![forbid(unsafe_code)]

use crate::cfmt;
use rn_core::nfmt;

const MAX_FORMAT: usize = 32;
const L_FMTFLAGSF: &[u8] = b"-+#0 ";
const L_FMTFLAGSX: &[u8] = b"-#0";
const L_FMTFLAGSI: &[u8] = b"-+0 ";
const L_FMTFLAGSU: &[u8] = b"-0";
const L_FMTFLAGSC: &[u8] = b"-";

/// Errors of `string.format`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FormatError<E> {
    /// `luaL_error` message (no position).
    Msg(Vec<u8>),
    /// `luaL_argerror`: argument index and message (`bad argument #n to 'format' (msg)`).
    Arg(usize, &'static str),
    /// Error raised by the argument source (type errors, `__tostring` failures, ...).
    Src(E),
}

/// How `%q` sees an argument (`lua_type` / `lua_isinteger` dispatch of `addliteral`).
#[derive(Clone, Debug, PartialEq)]
pub enum Literal {
    Str(Vec<u8>),
    Int(i64),
    Float(f64),
    /// `nil` / boolean: the `luaL_tolstring` text (`nil`, `true`, `false`).
    Plain(Vec<u8>),
    /// Any other type: `value has no literal form`.
    NoLiteral,
}

/// Argument access for `format`. Argument numbers follow Lua (the format string is argument 1).
pub trait FormatSource {
    type Error;
    /// `lua_gettop`.
    fn top(&self) -> usize;
    /// `luaL_checkinteger`.
    fn check_integer(&mut self, arg: usize) -> Result<i64, Self::Error>;
    /// `luaL_checknumber`.
    fn check_number(&mut self, arg: usize) -> Result<f64, Self::Error>;
    /// `luaL_tolstring` (may call `__tostring`).
    fn tolstring(&mut self, arg: usize) -> Result<Vec<u8>, Self::Error>;
    /// `luaL_tolstring` appended to `out` (sources can avoid the intermediate `Vec`).
    fn tolstring_into(&mut self, arg: usize, out: &mut Vec<u8>) -> Result<(), Self::Error> {
        out.extend_from_slice(&self.tolstring(arg)?);
        Ok(())
    }
    /// Classification for `%q`.
    fn literal(&mut self, arg: usize) -> Literal;
    /// `lua_topointer` (`None` = NULL).
    fn topointer(&mut self, arg: usize) -> Option<usize>;
}

type R<T, E> = Result<T, FormatError<E>>;

fn msg<T, E>(m: Vec<u8>) -> R<T, E> {
    Err(FormatError::Msg(m))
}

fn at(s: &[u8], i: usize) -> u8 {
    s.get(i).copied().unwrap_or(0)
}

fn get2digits(spec: &[u8], mut i: usize) -> usize {
    if at(spec, i).is_ascii_digit() {
        i += 1;
        if at(spec, i).is_ascii_digit() {
            i += 1;
        }
    }
    i
}

/// `checkformat`: `form` is the C string of the whole specification (starts with `%`).
fn checkformat<E>(form: &[u8], flags: &[u8], precision: bool) -> R<(), E> {
    let mut i = 1;
    while i < form.len() && flags.contains(&form[i]) {
        i += 1;
    }
    if at(form, i) != b'0' {
        i = get2digits(form, i);
        if at(form, i) == b'.' && precision {
            i += 1;
            i = get2digits(form, i);
        }
    }
    if !at(form, i).is_ascii_alphabetic() {
        let mut m = b"invalid conversion specification: '".to_vec();
        m.extend_from_slice(form);
        m.push(b'\'');
        return msg(m);
    }
    Ok(())
}

/// `(left-justified, width)` of a specification already accepted by `checkformat` with the flags `-` only
/// (`%` `-`* width of at most two digits [`.` precision] conversion).
fn parse_str_form(f: &[u8]) -> (bool, usize) {
    let mut i = 1;
    let mut left = false;
    while at(f, i) == b'-' {
        left = true;
        i += 1;
    }
    let mut w = 0usize;
    while at(f, i).is_ascii_digit() {
        w = w * 10 + (f[i] - b'0') as usize;
        i += 1;
    }
    (left, w)
}

/// The precision of a `%s` specification that contains a `.` (`%.s` means 0).
fn parse_str_prec(f: &[u8]) -> usize {
    let mut i = f.iter().position(|&c| c == b'.').map_or(f.len(), |d| d + 1);
    let mut p = 0usize;
    while at(f, i).is_ascii_digit() {
        p = p * 10 + (f[i] - b'0') as usize;
        i += 1;
    }
    p
}

/// `addquoted`.
fn addquoted(out: &mut Vec<u8>, s: &[u8]) {
    out.push(b'"');
    for (i, &c) in s.iter().enumerate() {
        if c == b'"' || c == b'\\' || c == b'\n' {
            out.push(b'\\');
            out.push(c);
        } else if c.is_ascii_control() {
            let next_digit = s.get(i + 1).is_some_and(|d| d.is_ascii_digit());
            if next_digit {
                out.extend_from_slice(format!("\\{:03}", c).as_bytes());
            } else {
                out.extend_from_slice(format!("\\{}", c).as_bytes());
            }
        } else {
            out.push(c);
        }
    }
    out.push(b'"');
}

/// `quotefloat`.
pub fn quotefloat(n: f64) -> Vec<u8> {
    if n == f64::INFINITY {
        b"1e9999".to_vec()
    } else if n == f64::NEG_INFINITY {
        b"-1e9999".to_vec()
    } else if n.is_nan() {
        b"(0/0)".to_vec()
    } else {
        let mut out = Vec::new();
        cfmt::fmt_f64(&mut out, b"%a", n);
        out
    }
}

fn internal<T, E>() -> R<T, E> {
    msg(b"invalid conversion".to_vec())
}

/// A conversion specification being built (`getformat`): at most `MAX_FORMAT` bytes, on the stack.
struct Form {
    buf: [u8; MAX_FORMAT],
    len: usize,
}

impl Form {
    fn bytes(&self) -> &[u8] {
        &self.buf[..self.len]
    }
    /// The same specification with the `ll` length modifier before the conversion letter.
    fn with_ll(&self) -> Form {
        let mut f = Form {
            buf: [0; MAX_FORMAT],
            len: self.len + 2,
        };
        let n = self.len - 1;
        f.buf[..n].copy_from_slice(&self.buf[..n]);
        f.buf[n] = b'l';
        f.buf[n + 1] = b'l';
        f.buf[n + 2] = self.buf[n];
        f
    }
    /// The same specification with the conversion letter replaced by `c`.
    fn with_conv(&self, c: u8) -> Form {
        let mut f = Form {
            buf: self.buf,
            len: self.len,
        };
        f.buf[self.len - 1] = c;
        f
    }
}

/// `string.format(fmt, ...)`.
pub fn format<S: FormatSource>(fmt: &[u8], src: &mut S) -> Result<Vec<u8>, FormatError<S::Error>> {
    let mut out = Vec::with_capacity(fmt.len() + 16);
    format_into(fmt, src, &mut out)?;
    Ok(out)
}

/// One format item whose specification starts at `i` (just after the `%`); `arg` is its argument number (already
/// checked against `top`). Returns the index after the item. This is the reference code path (the spec cache runs it
/// for the specifications it has no op for).
fn item_slow<S: FormatSource>(
    fmt: &[u8],
    i: usize,
    arg: usize,
    src: &mut S,
    out: &mut Vec<u8>,
) -> Result<usize, FormatError<S::Error>> {
    // getformat
    let mut len = 0;
    while b"-+#0 123456789.".contains(&at(fmt, i + len)) {
        len += 1;
    }
    len += 1;
    if len >= MAX_FORMAT - 10 {
        return msg(b"invalid format (too long)".to_vec());
    }
    let mut form = Form {
        buf: [0; MAX_FORMAT],
        len: 1 + len,
    };
    form.buf[0] = b'%';
    for k in 0..len {
        form.buf[1 + k] = at(fmt, i + k);
    }
    let conv = at(fmt, i + len - 1);
    let i = i + len;
    // `form` is a C string: it ends at the first NUL
    if let Some(z) = form.bytes().iter().position(|&c| c == 0) {
        form.len = z;
    }
    let f = form.bytes();
    match conv {
        b'c' => {
            checkformat(f, L_FMTFLAGSC, false)?;
            let n = src.check_integer(arg).map_err(FormatError::Src)?;
            // after `checkformat`: `%` `-`* width(<= 2 digits) `c`
            let (left, width) = parse_str_form(f);
            let pad = width.saturating_sub(1);
            if left {
                out.push(n as u8);
                out.resize(out.len() + pad, b' ');
            } else {
                out.resize(out.len() + pad, b' ');
                out.push(n as u8);
            }
        }
        b'd' | b'i' if f.len() == 2 => {
            // plain `%d`: the same digits as `%lld`, without `snprintf`
            let n = src.check_integer(arg).map_err(FormatError::Src)?;
            out.extend_from_slice(rn_core::number::IntBuf::new(n).as_bytes());
        }
        b'd' | b'i' | b'u' | b'o' | b'x' | b'X' => {
            let flags = match conv {
                b'd' | b'i' => L_FMTFLAGSI,
                b'u' => L_FMTFLAGSU,
                _ => L_FMTFLAGSX,
            };
            let n = src.check_integer(arg).map_err(FormatError::Src)?;
            checkformat(f, flags, true)?;
            if let Some(sp) = nfmt::parse_spec(f) {
                if nfmt::format_int(out, &sp, n) {
                    return Ok(i);
                }
            }
            if !cfmt::fmt_i64(out, form.with_ll().bytes(), n) {
                return internal();
            }
        }
        b'a' | b'A' => {
            checkformat(f, L_FMTFLAGSF, true)?;
            let n = src.check_number(arg).map_err(FormatError::Src)?;
            if !cfmt::fmt_f64(out, f, n) {
                return internal();
            }
        }
        b'f' | b'e' | b'E' | b'g' | b'G' => {
            let n = src.check_number(arg).map_err(FormatError::Src)?;
            checkformat(f, L_FMTFLAGSF, true)?;
            if let Some(sp) = nfmt::parse_spec(f) {
                if nfmt::format_float(out, &sp, n) {
                    return Ok(i);
                }
            }
            if !cfmt::fmt_f64(out, f, n) {
                return internal();
            }
        }
        b'p' => {
            let p = src.topointer(arg);
            checkformat(f, L_FMTFLAGSC, false)?;
            let ok = match p {
                None => cfmt::fmt_str(out, form.with_conv(b's').bytes(), b"(null)"),
                Some(p) => cfmt::fmt_ptr(out, f, p),
            };
            if !ok {
                return internal();
            }
        }
        b'q' => {
            if f.len() > 2 {
                return msg(b"specifier '%q' cannot have modifiers".to_vec());
            }
            match src.literal(arg) {
                Literal::Str(s) => addquoted(out, &s),
                Literal::Int(n) => {
                    if n == i64::MIN {
                        out.extend_from_slice(b"0x8000000000000000");
                    } else {
                        out.extend_from_slice(n.to_string().as_bytes());
                    }
                }
                Literal::Float(f) => out.extend_from_slice(&quotefloat(f)),
                Literal::Plain(p) => out.extend_from_slice(&p),
                Literal::NoLiteral => {
                    return Err(FormatError::Arg(arg, "value has no literal form"))
                }
            }
        }
        b's' => {
            if f.len() <= 2 {
                src.tolstring_into(arg, out).map_err(FormatError::Src)?;
            } else {
                // the text goes straight to its place in `out`; padding / truncation fix it up afterwards
                let start = out.len();
                src.tolstring_into(arg, out).map_err(FormatError::Src)?;
                if out[start..].contains(&0) {
                    out.truncate(start);
                    return Err(FormatError::Arg(arg, "string contains zeros"));
                }
                checkformat(f, L_FMTFLAGSC, true)?;
                let has_prec = f.contains(&b'.');
                if has_prec || out.len() - start < 100 {
                    // `snprintf("%-W.Ps")`: truncate to the precision, pad with spaces to the width
                    let (left, width) = parse_str_form(f);
                    let mut slen = out.len() - start;
                    if has_prec {
                        let prec = parse_str_prec(f);
                        if prec < slen {
                            out.truncate(start + prec);
                            slen = prec;
                        }
                    }
                    if width > slen {
                        let pad = width - slen;
                        if left {
                            out.resize(out.len() + pad, b' ');
                        } else {
                            out.resize(out.len() + pad, b' ');
                            out.copy_within(start..start + slen, start + pad);
                            out[start..start + pad].fill(b' ');
                        }
                    }
                }
            }
        }
        _ => {
            let mut m = b"invalid conversion '".to_vec();
            m.extend_from_slice(f);
            m.extend_from_slice(b"' to 'format'");
            return msg(m);
        }
    }
    Ok(i)
}

/// [`format()`] appending to a caller-owned buffer (B3-2: the library reuses one).
pub fn format_into<S: FormatSource>(
    fmt: &[u8],
    src: &mut S,
    out: &mut Vec<u8>,
) -> Result<(), FormatError<S::Error>> {
    if crate::strfast::on(crate::strfast::FMTC) {
        if let Some(c) = fmtc::lookup(fmt) {
            return fmtc::run(&c, fmt, src, out);
        }
    }
    format_slow(fmt, src, out)
}

/// The uncached engine.
fn format_slow<S: FormatSource>(
    fmt: &[u8],
    src: &mut S,
    out: &mut Vec<u8>,
) -> Result<(), FormatError<S::Error>> {
    let top = src.top();
    let mut arg = 1usize;
    let mut i = 0usize;
    while i < fmt.len() {
        if fmt[i] != b'%' {
            // copy the run up to the next '%'
            let n = fmt[i..]
                .iter()
                .position(|&c| c == b'%')
                .unwrap_or(fmt.len() - i);
            out.extend_from_slice(&fmt[i..i + n]);
            i += n;
            continue;
        }
        i += 1;
        if at(fmt, i) == b'%' {
            out.push(b'%');
            i += 1;
            continue;
        }
        // format item
        arg += 1;
        if arg > top {
            return Err(FormatError::Arg(arg, "no value"));
        }
        i = item_slow(fmt, i, arg, src, out)?;
    }
    Ok(())
}

/// BUNDLE-B S2 (`fmtc`): parsed format strings. A format string seen twice at the same address and length is compiled to
/// a list of ops (literal runs and the common conversions with their width/flags already parsed); the compiled form
/// is verified against the string's BYTES on every hit, so address reuse after a collection can never produce a wrong
/// hit. Specifications without an op (and every erroneous one) run the reference per-item code, so argument checks,
/// error order and messages are unchanged. The cache lives outside the Lua heap: it is not charged to GC debt.
mod fmtc {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    pub(super) enum Op {
        Lit {
            off: u32,
            len: u32,
        },
        /// plain `%d` / `%i`
        D,
        Int {
            sp: nfmt::Spec,
            ll: Form,
        },
        Float {
            sp: nfmt::Spec,
            f: Form,
        },
        Char {
            left: bool,
            width: usize,
        },
        /// plain `%s`
        S,
        SSpec {
            left: bool,
            width: usize,
            has_prec: bool,
            prec: usize,
        },
        /// the reference code for the item whose specification starts at this format index
        Slow {
            at: usize,
        },
    }

    pub(super) struct Compiled {
        key: Box<[u8]>,
        lits: Vec<u8>,
        ops: Vec<Op>,
    }

    fn push_lit(ops: &mut Vec<Op>, lits: &mut Vec<u8>, b: &[u8]) {
        let off = lits.len();
        lits.extend_from_slice(b);
        if let Some(Op::Lit { off: o, len }) = ops.last_mut() {
            if (*o + *len) as usize == off {
                *len += b.len() as u32;
                return;
            }
        }
        ops.push(Op::Lit {
            off: off as u32,
            len: b.len() as u32,
        });
    }

    fn compile(fmt: &[u8]) -> Compiled {
        let mut ops = Vec::new();
        let mut lits: Vec<u8> = Vec::new();
        let mut i = 0usize;
        while i < fmt.len() {
            if fmt[i] != b'%' {
                let n = fmt[i..]
                    .iter()
                    .position(|&c| c == b'%')
                    .unwrap_or(fmt.len() - i);
                push_lit(&mut ops, &mut lits, &fmt[i..i + n]);
                i += n;
                continue;
            }
            i += 1;
            if at(fmt, i) == b'%' {
                push_lit(&mut ops, &mut lits, b"%");
                i += 1;
                continue;
            }
            // getformat
            let mut len = 0;
            while b"-+#0 123456789.".contains(&at(fmt, i + len)) {
                len += 1;
            }
            len += 1;
            if len >= MAX_FORMAT - 10 {
                ops.push(Op::Slow { at: i });
                break; // always an error
            }
            let conv = at(fmt, i + len - 1);
            let mut form = Form {
                buf: [0; MAX_FORMAT],
                len: 1 + len,
            };
            form.buf[0] = b'%';
            for k in 0..len {
                form.buf[1 + k] = at(fmt, i + k);
            }
            if conv == 0 {
                ops.push(Op::Slow { at: i });
                break; // `invalid conversion` (the NUL truncates the specification)
            }
            let f = form.bytes();
            let slow = Op::Slow { at: i };
            let op = match conv {
                b'c' => {
                    if checkformat::<()>(f, L_FMTFLAGSC, false).is_ok() {
                        let (left, width) = parse_str_form(f);
                        Op::Char { left, width }
                    } else {
                        slow
                    }
                }
                b'd' | b'i' if f.len() == 2 => Op::D,
                b'd' | b'i' | b'u' | b'o' | b'x' | b'X' => {
                    let flags = match conv {
                        b'd' | b'i' => L_FMTFLAGSI,
                        b'u' => L_FMTFLAGSU,
                        _ => L_FMTFLAGSX,
                    };
                    match nfmt::parse_spec(f) {
                        Some(sp) if checkformat::<()>(f, flags, true).is_ok() => Op::Int {
                            sp,
                            ll: form.with_ll(),
                        },
                        _ => slow,
                    }
                }
                b'f' | b'e' | b'E' | b'g' | b'G' => match nfmt::parse_spec(f) {
                    Some(sp) if checkformat::<()>(f, L_FMTFLAGSF, true).is_ok() => {
                        Op::Float { sp, f: form }
                    }
                    _ => slow,
                },
                b's' => {
                    if f.len() <= 2 {
                        Op::S
                    } else if checkformat::<()>(f, L_FMTFLAGSC, true).is_ok() {
                        let (left, width) = parse_str_form(f);
                        let has_prec = f.contains(&b'.');
                        let prec = if has_prec { parse_str_prec(f) } else { 0 };
                        Op::SSpec {
                            left,
                            width,
                            has_prec,
                            prec,
                        }
                    } else {
                        slow
                    }
                }
                b'a' | b'A' | b'p' | b'q' => slow,
                _ => {
                    ops.push(slow);
                    break; // invalid conversion: always an error
                }
            };
            ops.push(op);
            i += len;
        }
        Compiled {
            key: fmt.into(),
            lits,
            ops,
        }
    }

    #[derive(Default)]
    enum Slot {
        #[default]
        Empty,
        Seen {
            addr: usize,
            len: usize,
        },
        Full(Rc<Compiled>),
    }

    const WAYS: usize = 32;
    const MAX_KEY: usize = 512;

    struct Cache {
        slots: [Slot; WAYS],
        rr: usize,
    }

    thread_local! {
        static CACHE: RefCell<Cache> = RefCell::new(Cache { slots: Default::default(), rr: 0 });
    }

    /// The compiled form of `fmt` once it was seen before (at the same address and length), else `None`
    /// (the caller runs the uncached engine). Two slots per set; hits compare the bytes.
    pub(super) fn lookup(fmt: &[u8]) -> Option<Rc<Compiled>> {
        if fmt.len() > MAX_KEY {
            return None;
        }
        let addr = fmt.as_ptr() as usize;
        let len = fmt.len();
        let set = ((addr >> 3) ^ (addr >> 9) ^ len.wrapping_mul(7)) & (WAYS - 1) & !1;
        CACHE
            .try_with(|c| {
                let mut c = c.borrow_mut();
                for w in 0..2 {
                    if let Slot::Full(rc) = &c.slots[set + w] {
                        if *rc.key == *fmt {
                            return Some(rc.clone());
                        }
                    }
                }
                for w in 0..2 {
                    if let Slot::Seen { addr: a, len: l } = c.slots[set + w] {
                        if a == addr && l == len {
                            let rc = Rc::new(compile(fmt));
                            c.slots[set + w] = Slot::Full(rc.clone());
                            return Some(rc);
                        }
                    }
                }
                // first sight: remember it (prefer an empty or a `Seen` slot; never evict both compiled forms of a set)
                let w = (0..2)
                    .find(|&w| matches!(c.slots[set + w], Slot::Empty | Slot::Seen { .. }))
                    .unwrap_or_else(|| {
                        c.rr = c.rr.wrapping_add(1);
                        c.rr & 1
                    });
                c.slots[set + w] = Slot::Seen { addr, len };
                None
            })
            .ok()
            .flatten()
    }

    pub(super) fn run<S: FormatSource>(
        c: &Compiled,
        fmt: &[u8],
        src: &mut S,
        out: &mut Vec<u8>,
    ) -> Result<(), FormatError<S::Error>> {
        let top = src.top();
        let mut arg = 1usize;
        for op in &c.ops {
            if let Op::Lit { off, len } = op {
                out.extend_from_slice(&c.lits[*off as usize..(*off + *len) as usize]);
                continue;
            }
            arg += 1;
            if arg > top {
                return Err(FormatError::Arg(arg, "no value"));
            }
            match op {
                Op::Lit { .. } => {}
                Op::D => {
                    let n = src.check_integer(arg).map_err(FormatError::Src)?;
                    out.extend_from_slice(rn_core::number::IntBuf::new(n).as_bytes());
                }
                Op::Int { sp, ll } => {
                    let n = src.check_integer(arg).map_err(FormatError::Src)?;
                    if !nfmt::format_int(out, sp, n) && !cfmt::fmt_i64(out, ll.bytes(), n) {
                        return internal();
                    }
                }
                Op::Float { sp, f } => {
                    let n = src.check_number(arg).map_err(FormatError::Src)?;
                    if !nfmt::format_float(out, sp, n) && !cfmt::fmt_f64(out, f.bytes(), n) {
                        return internal();
                    }
                }
                Op::Char { left, width } => {
                    let n = src.check_integer(arg).map_err(FormatError::Src)?;
                    let pad = width.saturating_sub(1);
                    if *left {
                        out.push(n as u8);
                        out.resize(out.len() + pad, b' ');
                    } else {
                        out.resize(out.len() + pad, b' ');
                        out.push(n as u8);
                    }
                }
                Op::S => src.tolstring_into(arg, out).map_err(FormatError::Src)?,
                Op::SSpec {
                    left,
                    width,
                    has_prec,
                    prec,
                } => {
                    let start = out.len();
                    src.tolstring_into(arg, out).map_err(FormatError::Src)?;
                    if out[start..].contains(&0) {
                        out.truncate(start);
                        return Err(FormatError::Arg(arg, "string contains zeros"));
                    }
                    if *has_prec || out.len() - start < 100 {
                        let mut slen = out.len() - start;
                        if *has_prec && *prec < slen {
                            out.truncate(start + *prec);
                            slen = *prec;
                        }
                        if *width > slen {
                            let pad = *width - slen;
                            if *left {
                                out.resize(out.len() + pad, b' ');
                            } else {
                                out.resize(out.len() + pad, b' ');
                                out.copy_within(start..start + slen, start + pad);
                                out[start..start + pad].fill(b' ');
                            }
                        }
                    }
                }
                Op::Slow { at } => {
                    item_slow(fmt, *at, arg, src, out)?;
                }
            }
        }
        Ok(())
    }
}
