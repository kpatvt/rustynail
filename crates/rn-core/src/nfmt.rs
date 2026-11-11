//! B3-3: native number formatting (no libc `snprintf` on the hot path). Entirely safe code.
//!
//! Bit-exact with glibc `printf` in the "C" locale for the conversions handled here (`d i u x X o` with the `ll`
//! length, `f F e E g G`; flags `- + space 0`, width, precision); anything else (`#`, `a`, ...) is left to the caller's
//! `snprintf` fallback ([`parse_spec`] returns `None`). The differential fuzz is `crates/rn-std/tests/nfmt_fuzz.rs`.
//!
//! Digits come from Rust's `core::fmt`, which is exact (Grisu with a Dragon fallback, round-half-even on the exact
//! binary value, like glibc). The one shortcut: for at most 15 significant digits and a *normal* double, the
//! shortest round-trip digits `D` (when they number at most `p <= 15`) are the exact `p`-digit rounding padded with zeros,
//! because `|x - D| < ulp/2 <= 2^-53 |x|` is smaller than half the spacing (`>= 5e-16 |x|`) of `p`-digit decimals; this
//! fails for subnormals (their relative ulp is huge), which take the exact path.

use std::fmt::Write as _;

/// A fixed-capacity byte buffer implementing `fmt::Write` (error on overflow).
struct Arr<const N: usize> {
    b: [u8; N],
    n: usize,
}

impl<const N: usize> Arr<N> {
    #[inline]
    fn new() -> Self {
        Arr { b: [0; N], n: 0 }
    }
    #[inline]
    fn bytes(&self) -> &[u8] {
        &self.b[..self.n]
    }
    #[inline]
    fn push(&mut self, c: u8) -> bool {
        if self.n < N {
            self.b[self.n] = c;
            self.n += 1;
            true
        } else {
            false
        }
    }
    #[inline]
    fn push_all(&mut self, s: &[u8]) -> bool {
        if self.n + s.len() > N {
            return false;
        }
        self.b[self.n..self.n + s.len()].copy_from_slice(s);
        self.n += s.len();
        true
    }
}

impl<const N: usize> std::fmt::Write for Arr<N> {
    #[inline]
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        if self.push_all(s.as_bytes()) {
            Ok(())
        } else {
            Err(std::fmt::Error)
        }
    }
}

/// Largest precision handled natively for `e` / `g` (significant digits); `f` allows [`MAX_PREC`] decimals.
pub const MAX_PREC: usize = 99;

const POW10: [u64; 20] = [
    1,
    10,
    100,
    1_000,
    10_000,
    100_000,
    1_000_000,
    10_000_000,
    100_000_000,
    1_000_000_000,
    10_000_000_000,
    100_000_000_000,
    1_000_000_000_000,
    10_000_000_000_000,
    100_000_000_000_000,
    1_000_000_000_000_000,
    10_000_000_000_000_000,
    100_000_000_000_000_000,
    1_000_000_000_000_000_000,
    10_000_000_000_000_000_000,
];

/// `round_half_even(x * 10^k)` computed exactly in 128-bit integers, for the finite `x >= 0` and `k` in `0..=19`.
/// `x = m * 2^e` with a 53-bit `m`, so `m * 10^k < 2^117`; a negative `e` is a right shift with exact remainder
/// (the rounding is that of glibc `printf`: exact binary value, ties to even). `None` when `x` is too large
/// (`e > 10`) for the product to stay in 128 bits.
#[inline]
fn scaled(x: f64, k: u32) -> Option<u128> {
    let bits = x.to_bits();
    let be = ((bits >> 52) & 0x7ff) as i32;
    let frac = bits & ((1u64 << 52) - 1);
    let (m, e) = if be == 0 {
        (frac, -1074)
    } else {
        (frac | (1u64 << 52), be - 1075)
    };
    let prod = (m as u128) * (POW10[k as usize] as u128);
    if e >= 0 {
        return if e > 10 { None } else { Some(prod << e) };
    }
    let s = (-e) as u32;
    if s >= 128 {
        // `prod < 2^117 <= 2^(s-1)`: below one half
        return Some(0);
    }
    let q = prod >> s;
    let rem = prod & ((1u128 << s) - 1);
    let half = 1u128 << (s - 1);
    Some(if rem > half || (rem == half && q & 1 == 1) {
        q + 1
    } else {
        q
    })
}

/// Write the decimal digits of `v` into `d[..p]` (zero-padded on the left; `v < 10^p`).
#[inline]
fn put_digits(mut v: u64, d: &mut [u8], p: usize) {
    const T: &[u8; 200] = b"0001020304050607080910111213141516171819\
        2021222324252627282930313233343536373839\
        4041424344454647484950515253545556575859\
        6061626364656667686970717273747576777879\
        8081828384858687888990919293949596979899";
    let mut i = p;
    while i >= 2 {
        let r = (v % 100) as usize * 2;
        v /= 100;
        i -= 2;
        d[i] = T[r];
        d[i + 1] = T[r + 1];
    }
    if i == 1 {
        d[0] = b'0' + v as u8;
    }
}

/// [`digits2`] for a normal `x > 0` and `1 <= p <= 19` without `core::fmt`: the digits are `scaled(x, p - 1 - x10)`
/// for the decimal exponent `x10` (estimated from the binary exponent, corrected by the digit count of the result).
/// `None` when the scale `10^(p-1-x10)` is outside `1..=10^19` (then the caller uses the general path).
/// The flag (`p <= 15`: the shortest round-trip text has at most `p` digits) is the Clinger fast-path read-back test:
/// `n < 2^53` and `10^k` are exact doubles, so `n / 10^k` is the correctly rounded value of the decimal.
#[inline]
fn digits_fast(x: f64, p: usize, d: &mut [u8; 128]) -> Option<(i32, bool)> {
    if !(1..=19).contains(&p) || x < f64::MIN_POSITIVE {
        return None;
    }
    let ebin = ((x.to_bits() >> 52) & 0x7ff) as i32 - 1023;
    let mut x10 = (ebin * 78913) >> 18; // floor(ebin * log10(2)), possibly one too small
    let hi = POW10.get(p).map_or(u128::MAX, |&v| v as u128);
    let lo = POW10[p - 1] as u128;
    for _ in 0..3 {
        let k = p as i32 - 1 - x10;
        if !(0..=19).contains(&k) {
            return None;
        }
        let n = scaled(x, k as u32)?;
        if n >= hi {
            x10 += 1;
            continue;
        }
        if n < lo {
            x10 -= 1;
            continue;
        }
        put_digits(n as u64, d, p);
        let short = p <= 15 && (n as f64) / (POW10[k as usize] as f64) == x;
        return Some((x10, short));
    }
    None
}

/// `p` significant decimal digits of the finite, non-negative `x`, correctly rounded: the digits go to `d[..p]`, the
/// decimal exponent of the first digit (after rounding, as in `%e`) is returned. `p` is in `1..=MAX_PREC + 1`.
pub fn digits(x: f64, p: usize, d: &mut [u8; 128]) -> Option<i32> {
    digits2(x, p, d).map(|r| r.0)
}

/// [`digits`] plus whether the digits came from the shortest-digits shortcut (or `x` is zero), i.e. the shortest
/// round-trip text of `x` has at most `p` digits.
pub fn digits2(x: f64, p: usize, d: &mut [u8; 128]) -> Option<(i32, bool)> {
    debug_assert!(x >= 0.0 && x.is_finite() && (1..=MAX_PREC + 1).contains(&p));
    if x == 0.0 {
        d[..p].fill(b'0');
        return Some((0, true));
    }
    if let Some(r) = digits_fast(x, p, d) {
        return Some(r);
    }
    // scientific text "d.ddde-x" (shortest, or exact with p digits)
    let mut t = Arr::<160>::new();
    let shortcut = p <= 15 && x >= f64::MIN_POSITIVE;
    if shortcut {
        write!(t, "{:e}", x).ok()?;
        let s = t.bytes();
        let epos = s.iter().position(|&c| c == b'e')?;
        let nd = if s[..epos].contains(&b'.') {
            epos - 1
        } else {
            epos
        };
        if nd <= p {
            let mut k = 0;
            for &c in &s[..epos] {
                if c != b'.' {
                    d[k] = c;
                    k += 1;
                }
            }
            d[k..p].fill(b'0');
            return parse_exp(&s[epos + 1..]).map(|e| (e, true));
        }
        t.n = 0;
    }
    write!(t, "{:.*e}", p - 1, x).ok()?;
    let s = t.bytes();
    let epos = s.iter().position(|&c| c == b'e')?;
    let mut k = 0;
    for &c in &s[..epos] {
        if c != b'.' {
            if k >= p {
                return None;
            }
            d[k] = c;
            k += 1;
        }
    }
    if k != p {
        return None;
    }
    parse_exp(&s[epos + 1..]).map(|e| (e, false))
}

#[inline]
fn parse_exp(e: &[u8]) -> Option<i32> {
    let (neg, ds) = match e.first()? {
        b'-' => (true, &e[1..]),
        _ => (false, e),
    };
    let mut v: i32 = 0;
    for &c in ds {
        v = v * 10 + (c - b'0') as i32;
    }
    Some(if neg { -v } else { v })
}

/// The conversion specification of one `string.format` item (no length modifier).
#[derive(Clone, Copy, Debug)]
pub struct Spec {
    pub minus: bool,
    pub plus: bool,
    pub space: bool,
    pub zero: bool,
    pub width: usize,
    pub prec: Option<usize>,
    pub conv: u8,
}

/// Parse `%[-+ 0]*[width][.prec]conv` for the conversions of this module; `None` for anything else (`#`, lengths over
/// 3 digits, other conversions).
pub fn parse_spec(form: &[u8]) -> Option<Spec> {
    if form.first() != Some(&b'%') {
        return None;
    }
    let mut sp = Spec {
        minus: false,
        plus: false,
        space: false,
        zero: false,
        width: 0,
        prec: None,
        conv: 0,
    };
    let mut i = 1;
    while i < form.len() {
        match form[i] {
            b'-' => sp.minus = true,
            b'+' => sp.plus = true,
            b' ' => sp.space = true,
            b'0' => sp.zero = true,
            b'#' => return None,
            _ => break,
        }
        i += 1;
    }
    let mut nd = 0;
    while i < form.len() && form[i].is_ascii_digit() {
        sp.width = sp.width * 10 + (form[i] - b'0') as usize;
        i += 1;
        nd += 1;
    }
    if nd > 3 {
        return None;
    }
    if i < form.len() && form[i] == b'.' {
        i += 1;
        let mut p = 0usize;
        nd = 0;
        while i < form.len() && form[i].is_ascii_digit() {
            p = p * 10 + (form[i] - b'0') as usize;
            i += 1;
            nd += 1;
        }
        if nd > 3 {
            return None;
        }
        sp.prec = Some(p);
    }
    if i + 1 != form.len() {
        return None;
    }
    sp.conv = form[i];
    if !matches!(
        sp.conv,
        b'd' | b'i' | b'u' | b'x' | b'X' | b'o' | b'f' | b'F' | b'e' | b'E' | b'g' | b'G'
    ) {
        return None;
    }
    Some(sp)
}

/// Append `sign body` padded to the width of `sp`.
#[inline]
fn pad(out: &mut Vec<u8>, sp: &Spec, sign: &[u8], body: &[u8], zero_ok: bool) {
    let len = sign.len() + body.len();
    if sp.width <= len {
        out.extend_from_slice(sign);
        out.extend_from_slice(body);
    } else if sp.minus {
        out.extend_from_slice(sign);
        out.extend_from_slice(body);
        out.resize(out.len() + sp.width - len, b' ');
    } else if sp.zero && zero_ok {
        out.extend_from_slice(sign);
        out.resize(out.len() + sp.width - len, b'0');
        out.extend_from_slice(body);
    } else {
        out.resize(out.len() + sp.width - len, b' ');
        out.extend_from_slice(sign);
        out.extend_from_slice(body);
    }
}

/// `d i u x X o` of `v` (as `long long`; unsigned conversions reinterpret it). `false`: not handled.
pub fn format_int(out: &mut Vec<u8>, sp: &Spec, v: i64) -> bool {
    let mut digs = [0u8; 64];
    let mut pos = 64;
    let (neg, mut u) = match sp.conv {
        b'd' | b'i' => (v < 0, v.unsigned_abs()),
        b'u' | b'x' | b'X' | b'o' => (false, v as u64),
        _ => return false,
    };
    let prec0 = sp.prec == Some(0) && u == 0;
    if !prec0 {
        match sp.conv {
            b'd' | b'i' | b'u' => loop {
                pos -= 1;
                digs[pos] = b'0' + (u % 10) as u8;
                u /= 10;
                if u == 0 {
                    break;
                }
            },
            b'o' => loop {
                pos -= 1;
                digs[pos] = b'0' + (u & 7) as u8;
                u >>= 3;
                if u == 0 {
                    break;
                }
            },
            c => {
                let tab: &[u8; 16] = if c == b'x' {
                    b"0123456789abcdef"
                } else {
                    b"0123456789ABCDEF"
                };
                loop {
                    pos -= 1;
                    digs[pos] = tab[(u & 15) as usize];
                    u >>= 4;
                    if u == 0 {
                        break;
                    }
                }
            }
        }
    }
    let nd = 64 - pos;
    let sign: &[u8] = if matches!(sp.conv, b'd' | b'i') {
        if neg {
            b"-"
        } else if sp.plus {
            b"+"
        } else if sp.space {
            b" "
        } else {
            b""
        }
    } else {
        b""
    };
    match sp.prec {
        Some(p) if p > nd => {
            if p > MAX_PREC {
                return false;
            }
            // `p - nd` leading zeros: build in a small buffer
            let mut body = [b'0'; 128];
            body[p - nd..p].copy_from_slice(&digs[pos..]);
            pad(out, sp, sign, &body[..p], false);
        }
        Some(_) => pad(out, sp, sign, &digs[pos..], false),
        None => pad(out, sp, sign, &digs[pos..], true),
    }
    true
}

/// Append the `%g`-style text (no sign) of the digits `d[..p]` with decimal exponent `x10`; `alt` is `#` (unused: the
/// caller never passes it). Trailing zeros of the fraction are removed.
fn g_body(b: &mut Arr<512>, d: &[u8], p: usize, x10: i32, upper: bool) -> bool {
    if x10 < -4 || x10 >= p as i32 {
        let mut last = p;
        while last > 1 && d[last - 1] == b'0' {
            last -= 1;
        }
        let mut ok = b.push(d[0]);
        if last > 1 {
            ok &= b.push(b'.') && b.push_all(&d[1..last]);
        }
        ok &= b.push(if upper { b'E' } else { b'e' });
        ok &= b.push(if x10 < 0 { b'-' } else { b'+' });
        let ax = x10.unsigned_abs();
        if ax < 10 {
            ok &= b.push(b'0');
        }
        ok & write!(b, "{ax}").is_ok()
    } else if x10 >= 0 {
        let ip = x10 as usize + 1;
        let mut last = p;
        while last > ip && d[last - 1] == b'0' {
            last -= 1;
        }
        let mut ok = b.push_all(&d[..ip]);
        if last > ip {
            ok &= b.push(b'.') && b.push_all(&d[ip..last]);
        }
        ok
    } else {
        let mut last = p;
        while last > 1 && d[last - 1] == b'0' {
            last -= 1;
        }
        let mut ok = b.push_all(b"0.");
        for _ in 0..(-x10 - 1) {
            ok &= b.push(b'0');
        }
        ok & b.push_all(&d[..last])
    }
}

/// `f F e E g G` of `x`. `false`: not handled (the caller falls back to `snprintf`).
pub fn format_float(out: &mut Vec<u8>, sp: &Spec, x: f64) -> bool {
    let neg = x.is_sign_negative();
    let sign: &[u8] = if neg {
        b"-"
    } else if sp.plus {
        b"+"
    } else if sp.space {
        b" "
    } else {
        b""
    };
    let upper = matches!(sp.conv, b'F' | b'E' | b'G');
    if !x.is_finite() {
        let body: &[u8] = match (x.is_nan(), upper) {
            (true, false) => b"nan",
            (true, true) => b"NAN",
            (false, false) => b"inf",
            (false, true) => b"INF",
        };
        pad(out, sp, sign, body, false);
        return true;
    }
    let ax = x.abs();
    let mut b = Arr::<512>::new();
    match sp.conv {
        b'f' | b'F' => {
            let prec = sp.prec.unwrap_or(6);
            if prec > MAX_PREC {
                return false;
            }
            let fast = if prec <= 19 {
                scaled(ax, prec as u32).filter(|&n| n <= u64::MAX as u128)
            } else {
                None
            };
            if let Some(n) = fast {
                // `n = round(ax * 10^prec)`: integer digits, the point, `prec` fraction digits
                let mut buf = [b'0'; 40];
                let nu = n as u64;
                let t = (64 - nu.leading_zeros() as usize) * 1233 >> 12;
                let nd = if nu == 0 {
                    1
                } else {
                    t + (nu >= POW10[t]) as usize
                };
                let total = nd.max(prec + 1);
                put_digits(nu, &mut buf[..total], total);
                let ip = total - prec;
                let mut ok = b.push_all(&buf[..ip]);
                if prec > 0 {
                    ok &= b.push(b'.') && b.push_all(&buf[ip..total]);
                }
                if !ok {
                    return false;
                }
            } else if write!(b, "{:.*}", prec, ax).is_err() {
                return false;
            }
        }
        b'e' | b'E' => {
            let prec = sp.prec.unwrap_or(6);
            if prec > MAX_PREC {
                return false;
            }
            let mut d = [0u8; 128];
            let Some(x10) = digits(ax, prec + 1, &mut d) else {
                return false;
            };
            let mut ok = b.push(d[0]);
            if prec > 0 {
                ok &= b.push(b'.') && b.push_all(&d[1..=prec]);
            }
            ok &= b.push(if upper { b'E' } else { b'e' });
            ok &= b.push(if x10 < 0 { b'-' } else { b'+' });
            let a = x10.unsigned_abs();
            if a < 10 {
                ok &= b.push(b'0');
            }
            if !ok || write!(b, "{a}").is_err() {
                return false;
            }
        }
        _ => {
            let p = match sp.prec {
                None => 6,
                Some(0) => 1,
                Some(p) => p,
            };
            if p > MAX_PREC {
                return false;
            }
            let mut d = [0u8; 128];
            let Some(x10) = digits(ax, p, &mut d) else {
                return false;
            };
            if !g_body(&mut b, &d, p, x10, upper) {
                return false;
            }
        }
    }
    pad(out, sp, sign, b.bytes(), true);
    true
}

/// Equivalent of C `printf("%.<prec>g", x)` (1 <= prec <= 30) into a small buffer; the flag is the one of [`digits2`].
pub fn fmt_g_buf(x: f64, prec: usize) -> ([u8; 64], usize, bool) {
    let mut out = [0u8; 64];
    let mut n = 0;
    let mut put = |s: &[u8]| {
        out[n..n + s.len()].copy_from_slice(s);
        n += s.len();
    };
    if x.is_sign_negative() {
        put(b"-");
    }
    if x.is_nan() {
        put(b"nan");
        return (out, n, true);
    }
    if x.is_infinite() {
        put(b"inf");
        return (out, n, true);
    }
    let p = prec.clamp(1, 30);
    let mut d = [0u8; 128];
    let (x10, short) = digits2(x.abs(), p, &mut d).expect("digits");
    let mut b = Arr::<512>::new();
    assert!(g_body(&mut b, &d, p, x10, false));
    put(b.bytes());
    (out, n, short)
}
