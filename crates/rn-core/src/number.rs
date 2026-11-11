//! Number parsing/formatting and arithmetic helpers (port of the number parts of
//! `lobject.c` / `lvm.c` / `llimits.h`). Entirely safe code.
//!
//! * [`str2num`]  = `luaO_str2num` (`l_str2int` + `l_str2d` + `lua_strx2number`).
//! * [`tostring_float`] / [`tostring_int`] = `luaO_tostringbuff` (5.5: `%.15g` then `%.17g`; 5.4: `%.14g`).
//! * [`idiv`], [`imod`], [`shiftl`], [`fmod_lua`], [`pow_lua`], [`float_to_int`], and the exact
//!   int/float comparisons `lt_int_float` & friends = `luaV_*` equivalents.

/// A Lua number: 64-bit integer or double.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Number {
    Int(i64),
    Float(f64),
}

// ---------------------------------------------------------------------------------------------
// ctype helpers (lctype.c: only the "C locale" classes Lua uses)
// ---------------------------------------------------------------------------------------------

/// `lisspace`: `' ' \t \n \v \f \r`.
#[inline]
pub fn is_space(c: u8) -> bool {
    matches!(c, b' ' | b'\t' | b'\n' | 0x0B | 0x0C | b'\r')
}

/// Value of a hexadecimal digit (`luaO_hexavalue`); `None` if `c` is not one.
#[inline]
pub fn hex_value(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    }
}

// ---------------------------------------------------------------------------------------------
// str2num
// ---------------------------------------------------------------------------------------------

/// `luaO_str2num`: convert a numeral string (decimal/hex int or float, surrounding whitespace allowed,
/// exactly as `tonumber`/the lexer accept) to a number. `None` if not a valid numeral.
///
/// Like the C function (which works on C strings) a string with an embedded NUL is rejected.
pub fn str2num(s: &[u8]) -> Option<Number> {
    if s.contains(&0) {
        return None;
    }
    if let Some(i) = str2int(s) {
        return Some(Number::Int(i));
    }
    str2float(s).map(Number::Float)
}

fn skip_space(s: &[u8], mut i: usize) -> usize {
    while i < s.len() && is_space(s[i]) {
        i += 1;
    }
    i
}

/// Consumes an optional sign; returns (new index, is_negative).
fn take_sign(s: &[u8], i: usize) -> (usize, bool) {
    match s.get(i) {
        Some(b'-') => (i + 1, true),
        Some(b'+') => (i + 1, false),
        _ => (i, false),
    }
}

#[inline]
fn is_hex_prefix(s: &[u8], i: usize) -> bool {
    s.get(i) == Some(&b'0') && matches!(s.get(i + 1), Some(b'x' | b'X'))
}

/// `l_str2int`.
fn str2int(s: &[u8]) -> Option<i64> {
    const MAXBY10: u64 = (i64::MAX / 10) as u64;
    const MAXLASTD: u64 = (i64::MAX % 10) as u64;
    let mut i = skip_space(s, 0);
    let (j, neg) = take_sign(s, i);
    i = j;
    let mut a: u64 = 0;
    let mut empty = true;
    if is_hex_prefix(s, i) {
        i += 2;
        while let Some(d) = s.get(i).and_then(|&c| hex_value(c)) {
            a = a.wrapping_mul(16).wrapping_add(d as u64);
            empty = false;
            i += 1;
        }
    } else {
        while let Some(&c) = s.get(i) {
            if !c.is_ascii_digit() {
                break;
            }
            let d = (c - b'0') as u64;
            if a >= MAXBY10 && (a > MAXBY10 || d > MAXLASTD + neg as u64) {
                return None; // overflow: do not accept as integer
            }
            a = a * 10 + d;
            empty = false;
            i += 1;
        }
    }
    i = skip_space(s, i);
    if empty || i != s.len() {
        return None;
    }
    Some(if neg { 0u64.wrapping_sub(a) } else { a } as i64)
}

/// `l_str2d`: float numeral (decimal via correctly-rounded parse, hex via [`hex_float`]).
fn str2float(s: &[u8]) -> Option<f64> {
    // 'n'/'N' anywhere => reject 'inf' and 'nan' (same as the reference; also rejects junk).
    if s.iter().any(|&c| c == b'n' || c == b'N') {
        return None;
    }
    let start = skip_space(s, 0);
    let mut end = s.len();
    while end > start && is_space(s[end - 1]) {
        end -= 1;
    }
    let body = &s[start..end];
    if body.is_empty() {
        return None;
    }
    let (i, neg) = take_sign(body, 0);
    if is_hex_prefix(body, i) {
        let v = hex_float(&body[i + 2..])?;
        return Some(if neg { -v } else { v });
    }
    // strtod decimal grammar: digits [. digits] | . digits, then optional [eE][+-]digits+
    let mut k = i;
    let mut nd = 0;
    while body.get(k).is_some_and(u8::is_ascii_digit) {
        k += 1;
        nd += 1;
    }
    if body.get(k) == Some(&b'.') {
        k += 1;
        while body.get(k).is_some_and(u8::is_ascii_digit) {
            k += 1;
            nd += 1;
        }
    }
    if nd == 0 {
        return None;
    }
    if matches!(body.get(k), Some(b'e' | b'E')) {
        k += 1;
        if matches!(body.get(k), Some(b'+' | b'-')) {
            k += 1;
        }
        let ds = k;
        while body.get(k).is_some_and(u8::is_ascii_digit) {
            k += 1;
        }
        if k == ds {
            return None;
        }
    }
    if k != body.len() {
        return None;
    }
    // Validated ASCII: Rust's parser accepts this grammar and rounds correctly (like glibc strtod).
    std::str::from_utf8(body).ok()?.parse::<f64>().ok()
}

/// Exact `ldexp` for an already-rounded mantissa `q <= 2^53` (so every step is exact or overflows).
fn ldexp_exact(q: u64, mut k: i64) -> f64 {
    let mut x = q as f64;
    let big = f64::from_bits(((1023 + 1000) as u64) << 52); // 2^1000
    let small = f64::from_bits(((1023 - 1000) as u64) << 52); // 2^-1000
    while k > 1000 {
        x *= big;
        k -= 1000;
        if x.is_infinite() {
            return x;
        }
    }
    while k < -1000 {
        x *= small;
        k += 1000;
        if x == 0.0 {
            return x;
        }
    }
    x * f64::from_bits(((1023 + k) as u64) << 52)
}

/// Hex float after the `0x` prefix: `hexdigits [. hexdigits] [pP [+-] digits]`; correctly rounded
/// (nearest-even) like C99 `strtod`, which is what the reference uses.
fn hex_float(s: &[u8]) -> Option<f64> {
    let mut mant: u64 = 0;
    let mut sticky = false;
    let mut e: i64 = 0;
    let mut hasdot = false;
    let mut anydigit = false;
    let mut i = 0;
    while i < s.len() {
        let c = s[i];
        if c == b'.' {
            if hasdot {
                return None; // second dot: trailing junk
            }
            hasdot = true;
        } else if let Some(d) = hex_value(c) {
            anydigit = true;
            if mant < (1u64 << 60) {
                mant = mant * 16 + d as u64;
                if hasdot {
                    e -= 4;
                }
            } else {
                sticky |= d != 0;
                if !hasdot {
                    e += 4;
                }
            }
        } else {
            break;
        }
        i += 1;
    }
    if !anydigit {
        return None;
    }
    if matches!(s.get(i), Some(b'p' | b'P')) {
        i += 1;
        let (j, neg) = take_sign(s, i);
        i = j;
        let ds = i;
        let mut ex: i64 = 0;
        while let Some(&c) = s.get(i) {
            if !c.is_ascii_digit() {
                break;
            }
            ex = (ex * 10 + (c - b'0') as i64).min(1 << 24);
            i += 1;
        }
        if i == ds {
            return None;
        }
        e += if neg { -ex } else { ex };
    }
    if i != s.len() {
        return None;
    }
    if mant == 0 {
        return Some(0.0);
    }
    // value = (mant + sticky*epsilon) * 2^e
    let bl = 64 - mant.leading_zeros() as i64;
    let ex = e + bl - 1; // exponent of the leading bit
    if ex > 1023 {
        return Some(f64::INFINITY);
    }
    let p = if ex >= -1022 { 53 } else { 53 - (-1022 - ex) };
    let drop = (bl - p).max(0);
    let q: u64 = if drop == 0 {
        mant
    } else if drop >= 128 {
        0
    } else {
        let m = mant as u128;
        let mut qq = (m >> drop) as u64;
        let rem = m & ((1u128 << drop) - 1);
        let half = 1u128 << (drop - 1);
        if rem > half || (rem == half && (sticky || qq & 1 == 1)) {
            qq += 1;
        }
        qq
    };
    Some(ldexp_exact(q, e + drop))
}

// ---------------------------------------------------------------------------------------------
// Formatting
// ---------------------------------------------------------------------------------------------

/// Fixed-capacity buffer for number-to-string results (`LUA_N2SBUFFSZ`-like; no heap allocation).
#[derive(Clone, Copy)]
pub struct NumBuf {
    buf: [u8; 64],
    len: usize,
}

impl NumBuf {
    fn new() -> Self {
        NumBuf {
            buf: [0; 64],
            len: 0,
        }
    }
    fn push(&mut self, c: u8) {
        self.buf[self.len] = c;
        self.len += 1;
    }
    fn push_all(&mut self, s: &[u8]) {
        for &c in s {
            self.push(c);
        }
    }
    pub fn as_bytes(&self) -> &[u8] {
        &self.buf[..self.len]
    }
}

impl std::fmt::Debug for NumBuf {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", String::from_utf8_lossy(self.as_bytes()))
    }
}

impl std::fmt::Write for NumBuf {
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        if self.len + s.len() > self.buf.len() {
            return Err(std::fmt::Error);
        }
        self.push_all(s.as_bytes());
        Ok(())
    }
}

/// Equivalent of C `printf("%.<prec>g", x)` in the "C" locale (no flags). `prec` is clamped to 1..=30
/// (the reference only uses 14, 15 and 17 here). B3-3: native, see [`crate::nfmt`].
pub fn fmt_g(x: f64, prec: usize) -> NumBuf {
    let (buf, len, _) = crate::nfmt::fmt_g_buf(x, prec);
    NumBuf { buf, len }
}

/// `tostringbuffFloat`: Lua's float-to-string conversion.
pub fn tostring_float(x: f64) -> NumBuf {
    #[cfg(not(rustynail_lua54))]
    let mut b = {
        // `%.15g`, kept when it reads back as `x`, else `%.17g`. For a normal double, "reads back" is "the shortest
        // round-trip digits number at most 15" (nfmt module doc), so no `strtod` is needed.
        if x.is_finite() && (x == 0.0 || x.abs() >= f64::MIN_POSITIVE) {
            let (buf, len, short) = crate::nfmt::fmt_g_buf(x, 15);
            if short {
                NumBuf { buf, len }
            } else {
                fmt_g(x, 17)
            }
        } else {
            let b = fmt_g(x, 15);
            let back = str2float(b.as_bytes());
            // `check != n` (NaN never equals itself, but both formats give "nan" anyway)
            if back == Some(x) || !x.is_finite() {
                b
            } else {
                fmt_g(x, 17)
            }
        }
    };
    #[cfg(rustynail_lua54)]
    let mut b = fmt_g(x, 14);
    // looks like an integer? add ".0"
    if b.as_bytes()
        .iter()
        .all(|&c| c == b'-' || c.is_ascii_digit())
    {
        b.push_all(b".0");
    }
    b
}

/// The decimal text of an integer (`lua_integer2str`, `%lld`), kept in the buffer of the caller's frame: the digits are written
/// straight to their final place (no scratch buffer and copy, no 72-byte struct returned by value: the copy's wide loads over
/// narrow stores were a store-forwarding stall of about 80 cycles per conversion).
pub struct IntBuf {
    buf: [u8; 20],
    start: u8,
}

impl IntBuf {
    /// `LUA_INTEGER_FMT` ("%lld") without the `core::fmt` machinery: two digits per division, the digit count known first.
    #[inline(always)]
    pub fn new(i: i64) -> IntBuf {
        const DIGITS: &[u8; 200] = b"0001020304050607080910111213141516171819\
            2021222324252627282930313233343536373839\
            4041424344454647484950515253545556575859\
            6061626364656667686970717273747576777879\
            8081828384858687888990919293949596979899";
        // 20 bytes hold the 19 digits of `i64::MAX` plus a sign, and the 19 digits + sign of `i64::MIN` (`-9223372036854775808` is
        // 20 characters).
        let mut r = IntBuf {
            buf: [0u8; 20],
            start: 20,
        };
        let mut u = i.unsigned_abs();
        let mut pos = 20usize;
        while u >= 100 {
            let d = (u % 100) as usize * 2;
            u /= 100;
            pos -= 2;
            r.buf[pos] = DIGITS[d];
            r.buf[pos + 1] = DIGITS[d + 1];
        }
        if u >= 10 {
            let d = u as usize * 2;
            pos -= 2;
            r.buf[pos] = DIGITS[d];
            r.buf[pos + 1] = DIGITS[d + 1];
        } else {
            pos -= 1;
            r.buf[pos] = b'0' + u as u8;
        }
        if i < 0 {
            pos -= 1;
            r.buf[pos] = b'-';
        }
        r.start = pos as u8;
        r
    }

    #[inline(always)]
    pub fn as_bytes(&self) -> &[u8] {
        &self.buf[self.start as usize..]
    }
}

/// `lua_integer2str` (`%lld`).
pub fn tostring_int(i: i64) -> NumBuf {
    let ib = IntBuf::new(i);
    let mut b = NumBuf::new();
    b.push_all(ib.as_bytes());
    b
}

/// `luaO_tostringbuff` for a caller that only needs the bytes for a moment: integers are formatted in place ([`IntBuf`]).
#[inline(always)]
pub fn with_tostring<R>(n: Number, f: impl FnOnce(&[u8]) -> R) -> R {
    match n {
        Number::Int(i) => f(IntBuf::new(i).as_bytes()),
        Number::Float(x) => f(tostring_float(x).as_bytes()),
    }
}

/// `luaO_tostringbuff`.
pub fn tostring(n: Number) -> NumBuf {
    match n {
        Number::Int(i) => tostring_int(i),
        Number::Float(f) => tostring_float(f),
    }
}

// ---------------------------------------------------------------------------------------------
// Arithmetic (lvm.c / llimits.h)
// ---------------------------------------------------------------------------------------------

/// `luaV_idiv`: floor division. `None` means division by zero (caller raises the error).
#[inline]
pub fn idiv(m: i64, n: i64) -> Option<i64> {
    if n == 0 {
        None
    } else if n == -1 {
        Some(0i64.wrapping_sub(m))
    } else {
        let mut q = m / n;
        if (m ^ n) < 0 && m % n != 0 {
            q -= 1;
        }
        Some(q)
    }
}

/// `luaV_mod`: floor modulus. `None` means modulo by zero.
#[inline]
pub fn imod(m: i64, n: i64) -> Option<i64> {
    if n == 0 {
        None
    } else if n == -1 {
        Some(0)
    } else {
        let mut r = m % n;
        if r != 0 && (r ^ n) < 0 {
            r += n;
        }
        Some(r)
    }
}

/// `luaV_shiftl`: logical shift left by `y` (right if negative); |y| >= 64 gives 0.
#[inline]
pub fn shiftl(x: i64, y: i64) -> i64 {
    if y < 0 {
        if y <= -64 {
            0
        } else {
            ((x as u64) >> (-y) as u32) as i64
        }
    } else if y >= 64 {
        0
    } else {
        ((x as u64) << y as u32) as i64
    }
}

/// `luaV_shiftr` (`x >> y` == `shiftl(x, -y)`; careful with `y == i64::MIN`).
#[inline]
pub fn shiftr(x: i64, y: i64) -> i64 {
    shiftl(x, y.wrapping_neg())
}

/// `luai_nummod`: float modulus with the sign of the divisor.
#[inline]
pub fn fmod_lua(a: f64, b: f64) -> f64 {
    let mut m = a % b; // C fmod
    if if m > 0.0 { b < 0.0 } else { m < 0.0 && b > 0.0 } {
        m += b;
    }
    m
}

/// `luai_numpow`.
#[inline]
pub fn pow_lua(a: f64, b: f64) -> f64 {
    if b == 2.0 {
        a * a
    } else {
        a.powf(b)
    }
}

/// `luai_numidiv`: float floor division.
#[inline]
pub fn fidiv(a: f64, b: f64) -> f64 {
    (a / b).floor()
}

/// Rounding mode for float -> integer conversion (`F2Imod`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum F2I {
    /// accepts only integral values
    Eq,
    Floor,
    Ceil,
}

/// `lua_numbertointeger`: `f` must already be integral-valued or the range check is on `f` itself.
#[inline]
pub fn float_in_int_range(f: f64) -> Option<i64> {
    // -2^63 <= f < 2^63 (NaN fails both)
    if (-9223372036854775808.0..9223372036854775808.0).contains(&f) {
        Some(f as i64)
    } else {
        None
    }
}

/// `luaV_flttointeger`.
#[inline]
pub fn float_to_int(n: f64, mode: F2I) -> Option<i64> {
    let mut f = n.floor();
    if n != f {
        match mode {
            F2I::Eq => return None,
            F2I::Ceil => f += 1.0,
            F2I::Floor => {}
        }
    }
    float_in_int_range(f)
}

/// `l_intfitsf`: integer is exactly representable as a double.
#[inline]
fn int_fits_float(i: i64) -> bool {
    const MAXINTFITSF: u64 = 1 << 53;
    MAXINTFITSF.wrapping_add(i as u64) <= 2 * MAXINTFITSF
}

/// `LTintfloat`: `i < f`, exact.
pub fn lt_int_float(i: i64, f: f64) -> bool {
    if int_fits_float(i) {
        (i as f64) < f
    } else if let Some(fi) = float_to_int(f, F2I::Ceil) {
        i < fi
    } else {
        f > 0.0
    }
}

/// `LEintfloat`: `i <= f`, exact.
pub fn le_int_float(i: i64, f: f64) -> bool {
    if int_fits_float(i) {
        (i as f64) <= f
    } else if let Some(fi) = float_to_int(f, F2I::Floor) {
        i <= fi
    } else {
        f > 0.0
    }
}

/// `LTfloatint`: `f < i`, exact.
pub fn lt_float_int(f: f64, i: i64) -> bool {
    if int_fits_float(i) {
        f < (i as f64)
    } else if let Some(fi) = float_to_int(f, F2I::Floor) {
        fi < i
    } else {
        f < 0.0
    }
}

/// `LEfloatint`: `f <= i`, exact.
pub fn le_float_int(f: f64, i: i64) -> bool {
    if int_fits_float(i) {
        f <= (i as f64)
    } else if let Some(fi) = float_to_int(f, F2I::Ceil) {
        fi <= i
    } else {
        f < 0.0
    }
}

/// Number equality across variants (`luaV_equalobj` number cases): an int equals a float iff the
/// float has an exact integer value equal to the int.
pub fn eq_int_float(i: i64, f: f64) -> bool {
    float_to_int(f, F2I::Eq) == Some(i)
}

/// `LTnum`.
pub fn lt_num(a: Number, b: Number) -> bool {
    match (a, b) {
        (Number::Int(x), Number::Int(y)) => x < y,
        (Number::Int(x), Number::Float(y)) => lt_int_float(x, y),
        (Number::Float(x), Number::Float(y)) => x < y,
        (Number::Float(x), Number::Int(y)) => lt_float_int(x, y),
    }
}

/// `LEnum`.
pub fn le_num(a: Number, b: Number) -> bool {
    match (a, b) {
        (Number::Int(x), Number::Int(y)) => x <= y,
        (Number::Int(x), Number::Float(y)) => le_int_float(x, y),
        (Number::Float(x), Number::Float(y)) => x <= y,
        (Number::Float(x), Number::Int(y)) => le_float_int(x, y),
    }
}

/// Raw equality of two numbers (`luaV_equalobj`, number cases).
pub fn eq_num(a: Number, b: Number) -> bool {
    match (a, b) {
        (Number::Int(x), Number::Int(y)) => x == y,
        (Number::Float(x), Number::Float(y)) => x == y,
        (Number::Int(i), Number::Float(f)) | (Number::Float(f), Number::Int(i)) => {
            eq_int_float(i, f)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// F1: `IntBuf` / `tostring_int` / `with_tostring` give `%lld` for every shape of integer (digit counts 1..=19, both signs, the
    /// limits, powers of ten and their neighbours).
    #[test]
    fn int_to_string_all_shapes() {
        let mut vals = vec![
            0i64,
            1,
            -1,
            9,
            10,
            -9,
            -10,
            99,
            100,
            101,
            -99,
            -100,
            -101,
            i64::MAX,
            i64::MIN,
            i64::MIN + 1,
            i64::MAX - 1,
        ];
        let mut p = 1i64;
        for _ in 0..19 {
            for d in [-1i64, 0, 1] {
                vals.push(p.wrapping_add(d));
                vals.push(p.wrapping_add(d).wrapping_neg());
            }
            p = p.wrapping_mul(10);
        }
        let mut x = 0x9E37_79B9_7F4A_7C15u64;
        for _ in 0..20000 {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            vals.push(x as i64);
            vals.push((x >> (x % 64)) as i64);
        }
        for v in vals {
            let want = format!("{v}");
            assert_eq!(IntBuf::new(v).as_bytes(), want.as_bytes(), "IntBuf {v}");
            assert_eq!(
                tostring_int(v).as_bytes(),
                want.as_bytes(),
                "tostring_int {v}"
            );
            assert_eq!(
                tostring(Number::Int(v)).as_bytes(),
                want.as_bytes(),
                "tostring {v}"
            );
            with_tostring(Number::Int(v), |b| {
                assert_eq!(b, want.as_bytes(), "with_tostring {v}")
            });
        }
        with_tostring(Number::Float(1.0), |b| assert_eq!(b, b"1.0"));
    }

    fn n(s: &str) -> Option<Number> {
        str2num(s.as_bytes())
    }

    #[test]
    fn ints() {
        assert_eq!(n("0"), Some(Number::Int(0)));
        assert_eq!(n("  -12  "), Some(Number::Int(-12)));
        assert_eq!(n("+7"), Some(Number::Int(7)));
        assert_eq!(n("9223372036854775807"), Some(Number::Int(i64::MAX)));
        assert_eq!(n("-9223372036854775808"), Some(Number::Int(i64::MIN)));
        assert_eq!(
            n("9223372036854775808"),
            Some(Number::Float(9223372036854775808.0))
        );
        assert_eq!(n("0xffffffffffffffff"), Some(Number::Int(-1)));
        assert_eq!(n("0x10000000000000000"), Some(Number::Int(0)));
        assert_eq!(n("-0x1"), Some(Number::Int(-1)));
        assert_eq!(n("0x"), None);
        assert_eq!(n(""), None);
        assert_eq!(n("  "), None);
        assert_eq!(n("1 2"), None);
        assert_eq!(n("1\0"), None);
    }

    #[test]
    fn floats() {
        assert_eq!(n("1.5"), Some(Number::Float(1.5)));
        assert_eq!(n(".5"), Some(Number::Float(0.5)));
        assert_eq!(n("5."), Some(Number::Float(5.0)));
        assert_eq!(n("1e2"), Some(Number::Float(100.0)));
        assert_eq!(n("1e"), None);
        assert_eq!(n("."), None);
        assert_eq!(n("inf"), None);
        assert_eq!(n("nan"), None);
        assert_eq!(n("1e999"), Some(Number::Float(f64::INFINITY)));
        assert_eq!(n("0x.8"), Some(Number::Float(0.5)));
        assert_eq!(n("0x1p4"), Some(Number::Float(16.0)));
        assert_eq!(n("0xA.8p0"), Some(Number::Float(10.5)));
        assert_eq!(n("0x1p-1074"), Some(Number::Float(f64::from_bits(1))));
        assert_eq!(n("0x1p-1075"), Some(Number::Float(0.0)));
        assert_eq!(n("0x1.00000000000008p0"), Some(Number::Float(1.0)));
        assert_eq!(
            n("0x1.00000000000018p0"),
            Some(Number::Float(1.0 + 2.0 * f64::EPSILON))
        );
        assert_eq!(n("0x1p1024"), Some(Number::Float(f64::INFINITY)));
        assert_eq!(n("0x1p"), None);
        assert_eq!(n("0x1.2.3"), None);
        assert_eq!(n("1e+"), None);
    }

    #[test]
    fn formatting() {
        let f = |x: f64| String::from_utf8(tostring_float(x).as_bytes().to_vec()).unwrap();
        assert_eq!(f(1.0), "1.0");
        assert_eq!(f(-0.0), "-0.0");
        assert_eq!(f(1e100), "1e+100");
        assert_eq!(f(f64::INFINITY), "inf");
        assert_eq!(f(f64::NEG_INFINITY), "-inf");
        assert_eq!(f(0.1), "0.1");
        assert_eq!(f(1e15), "1e+15");
        #[cfg(not(rustynail_lua54))]
        assert_eq!(f(123456789012345.0), "123456789012345.0");
        #[cfg(rustynail_lua54)]
        assert_eq!(f(123456789012345.0), "1.2345678901234e+14");
        #[cfg(not(rustynail_lua54))]
        assert_eq!(f(1.0 / 3.0), "0.33333333333333331");
        #[cfg(rustynail_lua54)]
        assert_eq!(f(1.0 / 3.0), "0.33333333333333");
        assert_eq!(
            String::from_utf8_lossy(fmt_g(100000.0, 6).as_bytes()),
            "100000"
        );
        assert_eq!(
            String::from_utf8_lossy(fmt_g(1000000.0, 6).as_bytes()),
            "1e+06"
        );
        assert_eq!(
            String::from_utf8_lossy(fmt_g(0.0001, 6).as_bytes()),
            "0.0001"
        );
        assert_eq!(
            String::from_utf8_lossy(fmt_g(0.00001, 6).as_bytes()),
            "1e-05"
        );
    }

    #[test]
    fn int_ops_model() {
        let vals = [
            i64::MIN,
            i64::MIN + 1,
            -1000,
            -7,
            -3,
            -2,
            -1,
            0,
            1,
            2,
            3,
            7,
            1000,
            i64::MAX - 1,
            i64::MAX,
        ];
        for &m in &vals {
            for &d in &vals {
                if d == 0 {
                    assert_eq!(idiv(m, d), None);
                    assert_eq!(imod(m, d), None);
                    continue;
                }
                let (mm, dd) = (m as i128, d as i128);
                let mut fq = mm / dd;
                if mm % dd != 0 && ((mm < 0) != (dd < 0)) {
                    fq -= 1;
                }
                let fr = mm - fq * dd;
                assert_eq!(idiv(m, d), Some(fq as i64), "{m} // {d}");
                assert_eq!(imod(m, d), Some(fr as i64), "{m} % {d}");
            }
        }
    }

    #[test]
    fn int_ops_random_vs_i128() {
        let mut x: u64 = 0x2545F4914F6CDD1D;
        let mut next = || {
            x ^= x >> 12;
            x ^= x << 25;
            x ^= x >> 27;
            x.wrapping_mul(0x2545F4914F6CDD1D)
        };
        let iters = if cfg!(miri) { 200 } else { 200_000 };
        for _ in 0..iters {
            let m = (next() as i64) >> (next() % 64);
            let d = (next() as i64) >> (next() % 64);
            if d == 0 {
                continue;
            }
            let (mm, dd) = (m as i128, d as i128);
            let mut fq = mm / dd;
            if mm % dd != 0 && ((mm < 0) != (dd < 0)) {
                fq -= 1;
            }
            // LUA_MININTEGER // -1 wraps
            assert_eq!(idiv(m, d), Some(fq as i64), "{m} // {d}");
            assert_eq!(imod(m, d), Some((mm - fq * dd) as i64), "{m} % {d}");
            let y = (next() as i64) >> (next() % 64);
            let want = if y.unsigned_abs() >= 64 {
                0
            } else if y >= 0 {
                ((m as u64) << y) as i64
            } else {
                ((m as u64) >> (-y)) as i64
            };
            assert_eq!(shiftl(m, y), want);
        }
    }

    #[test]
    fn shifts() {
        assert_eq!(shiftl(1, 63), i64::MIN);
        assert_eq!(shiftl(1, 64), 0);
        assert_eq!(shiftl(-1, -1), i64::MAX);
        assert_eq!(shiftl(-1, -64), 0);
        assert_eq!(shiftl(5, i64::MIN), 0);
        assert_eq!(shiftr(8, 3), 1);
        assert_eq!(shiftr(8, i64::MIN), 0);
        assert_eq!(shiftr(8, -3), 64);
    }

    #[test]
    fn compare_exact() {
        let big = 9007199254740993i64; // 2^53 + 1
        assert!(!eq_int_float(big, 9007199254740992.0));
        assert!(lt_float_int(9007199254740992.0, big));
        assert!(lt_int_float(i64::MAX, 9223372036854775808.0));
        assert!(!le_float_int(9223372036854775808.0, i64::MAX));
        assert!(le_int_float(i64::MIN, -9223372036854775808.0));
        assert!(!lt_int_float(1, f64::NAN));
        assert!(!le_float_int(f64::NAN, 1));
        assert_eq!(
            float_to_int(-9223372036854775808.0, F2I::Eq),
            Some(i64::MIN)
        );
        assert_eq!(float_to_int(9223372036854775808.0, F2I::Eq), None);
        assert_eq!(float_to_int(2.5, F2I::Ceil), Some(3));
        assert_eq!(float_to_int(-2.5, F2I::Floor), Some(-3));
    }

    #[test]
    fn float_mod() {
        assert_eq!(fmod_lua(5.5, 2.0), 1.5);
        assert_eq!(fmod_lua(-5.5, 2.0), 0.5);
        assert_eq!(fmod_lua(5.5, -2.0), -0.5);
        assert!(fmod_lua(1.0, 0.0).is_nan());
        assert_eq!(fmod_lua(1.0, f64::INFINITY), 1.0);
        assert_eq!(fmod_lua(-1.0, f64::INFINITY), f64::INFINITY);
    }
}
