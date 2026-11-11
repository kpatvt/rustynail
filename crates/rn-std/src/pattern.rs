//! Lua pattern matching engine (port of the pattern section of `lstrlib.c`; identical in 5.4 and 5.5).
//!
//! Pure functions over `&[u8]`; errors are the reference messages *without* position prefix
//! (the VM wraps them like `luaL_error`).
#![deny(unsafe_code)]

use std::mem::MaybeUninit;

mod patc;

/// Maximum recursion depth of `match` (`MAXCCALLS`).
pub const MAXCCALLS: i32 = 200;
/// Maximum number of captures (`LUA_MAXCAPTURES`).
pub const MAXCAPTURES: usize = 32;

const CAP_UNFINISHED: isize = -1;
const CAP_POSITION: isize = -2;
const L_ESC: u8 = b'%';

pub type PResult<T> = Result<T, Vec<u8>>;

fn err<T>(msg: &str) -> PResult<T> {
    Err(msg.as_bytes().to_vec())
}

/// A capture value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cap<'a> {
    Str(&'a [u8]),
    /// Position capture (1-based).
    Pos(i64),
}

#[derive(Clone, Copy)]
struct Capture {
    init: usize,
    len: isize,
}

/// `MatchState`.
pub struct MatchState<'a> {
    src: &'a [u8],
    pat: &'a [u8],
    matchdepth: i32,
    level: usize,
    /// Message of the error that made an internal function return [`ERR`].
    error: Option<Vec<u8>>,
    /// Only the first `level` entries are initialised (never read above it): no 512 byte clear per match.
    capture: [MaybeUninit<Capture>; MAXCAPTURES],
    /// The one-item fast path (BUNDLE-B S1), set by [`MatchState::start_filter`] when the pattern qualifies (a packed
    /// [`Plan`], 0 = none: one word, so the analysis result is not forwarded through a partially written stack slot).
    plan: u64,
    /// The compiled pattern (BUNDLE-S3 `patc`), set by [`MatchState::set_plan`] when the pattern is in the cache.
    prog: Option<&'static patc::Prog>,
    /// keeps the cache slot of `prog` from being rewritten while this state lives
    #[allow(dead_code)]
    pin: patc::PinGuard,
}

/// A pattern that is exactly one single-character item with an optional quantifier, optionally wrapped in one
/// whole-match capture: matched by a direct scan instead of `match_inner`. Packed in a `u64`:
/// bits 0-2 item kind (1 literal, 2 `.`, 3 `%` class, 4 set), bit 3 capture, bits 4-6 quantifier (0 none, 1 `?`, 2 `*`,
/// 3 `+`, 4 `-`), bits 8-15 literal/class byte, bits 16-39 set start, bits 40-63 set end (patterns under 16 MB).
#[derive(Clone, Copy)]
struct Plan(u64);

const K_LIT: u64 = 1;
const K_ANY: u64 = 2;
const K_SET: u64 = 4;
/// `%` class selected by a lower-case letter / its complement (upper case): the `CTYPE` mask is in bits 16-26.
const K_CPOS: u64 = 5;
const K_CNEG: u64 = 6;

impl Plan {
    #[inline(always)]
    fn kind(self) -> u64 {
        self.0 & 7
    }
    #[inline(always)]
    fn cap(self) -> bool {
        self.0 & 8 != 0
    }
    #[inline(always)]
    fn quant(self) -> u64 {
        (self.0 >> 4) & 7
    }
    #[inline(always)]
    fn byte(self) -> u8 {
        (self.0 >> 8) as u8
    }
    #[inline(always)]
    fn set_p(self) -> usize {
        ((self.0 >> 16) & 0xff_ffff) as usize
    }
    #[inline(always)]
    fn set_ep(self) -> usize {
        (self.0 >> 40) as usize
    }
}

/// Internal results of the matcher are subject positions; these two values (never valid
/// positions: a subject is shorter than `isize::MAX`) mean "no match" and "error (message in
/// `MatchState::error`)". Returning a plain `usize` keeps results in registers (a
/// `Result<Option<usize>, Vec<u8>>` is returned through memory at every recursion level).
const NOM: usize = usize::MAX;
const ERR: usize = usize::MAX - 1;

// Character classes (`isalpha` & co. in the "C" locale) as bit masks: `CTYPE[c]` holds the classes of
// byte `c`, `CLBIT[cl]` the class selected by the letter `cl` (either case; 0 = not a class letter).
const C_A: u16 = 1;
const C_C: u16 = 2;
const C_D: u16 = 4;
const C_G: u16 = 8;
const C_L: u16 = 16;
const C_P: u16 = 32;
const C_S: u16 = 64;
const C_U: u16 = 128;
const C_W: u16 = 256;
const C_X: u16 = 512;
const C_Z: u16 = 1024;

const CTYPE: [u16; 256] = {
    let mut t = [0u16; 256];
    let mut i = 0;
    while i < 256 {
        let c = i as u8;
        let mut m = 0;
        if c.is_ascii_alphabetic() {
            m |= C_A;
        }
        if c.is_ascii_control() {
            m |= C_C;
        }
        if c.is_ascii_digit() {
            m |= C_D;
        }
        if c.is_ascii_graphic() {
            m |= C_G;
        }
        if c.is_ascii_lowercase() {
            m |= C_L;
        }
        if c.is_ascii_punctuation() {
            m |= C_P;
        }
        if matches!(c, b' ' | b'\t' | b'\n' | 0x0b | 0x0c | b'\r') {
            m |= C_S;
        }
        if c.is_ascii_uppercase() {
            m |= C_U;
        }
        if c.is_ascii_alphanumeric() {
            m |= C_W;
        }
        if c.is_ascii_hexdigit() {
            m |= C_X;
        }
        if c == 0 {
            m |= C_Z;
        }
        t[i] = m;
        i += 1;
    }
    t
};

const CLBIT: [u16; 256] = {
    let mut t = [0u16; 256];
    let l: [(u8, u16); 11] = [
        (b'a', C_A),
        (b'c', C_C),
        (b'd', C_D),
        (b'g', C_G),
        (b'l', C_L),
        (b'p', C_P),
        (b's', C_S),
        (b'u', C_U),
        (b'w', C_W),
        (b'x', C_X),
        (b'z', C_Z),
    ];
    let mut i = 0;
    while i < l.len() {
        t[l[i].0 as usize] = l[i].1;
        t[l[i].0.to_ascii_uppercase() as usize] = l[i].1;
        i += 1;
    }
    t
};

#[inline(always)]
fn match_class(c: u8, cl: u8) -> bool {
    let b = CLBIT[cl as usize];
    if b == 0 {
        return cl == c;
    }
    // lower-case class letter: the class; upper case: its complement
    (CTYPE[c as usize] & b != 0) == cl.is_ascii_lowercase()
}

/// First position of byte `b` in `h` (word-at-a-time scan).
fn find_byte(h: &[u8], b: u8) -> Option<usize> {
    const LO: u64 = 0x0101_0101_0101_0101;
    const HI: u64 = 0x8080_8080_8080_8080;
    let rep = LO.wrapping_mul(b as u64);
    let (chunks, rest) = h.as_chunks::<8>();
    let mut base = 0;
    for c in chunks {
        let w = u64::from_le_bytes(*c) ^ rep;
        // the lowest flagged byte is the first zero byte (false positives only above a real one)
        let t = w.wrapping_sub(LO) & !w & HI;
        if t != 0 {
            return Some(base + (t.trailing_zeros() / 8) as usize);
        }
        base += 8;
    }
    rest.iter().position(|&x| x == b).map(|i| base + i)
}

/// The membership bitmap of the set `p..=ec` of `pat` (same semantics as [`MatchState::matchbracketclass`]).
fn set_bitmap(pat: &[u8], mut p: usize, ec: usize) -> [u64; 4] {
    let at = |i: usize| pat.get(i).copied().unwrap_or(0);
    let mut bm = [0u64; 4];
    let mut sig = true;
    if at(p + 1) == b'^' {
        sig = false;
        p += 1;
    }
    loop {
        p += 1;
        if p >= ec {
            break;
        }
        if at(p) == L_ESC {
            p += 1;
            let m = &patc::CLASS_BM[at(p) as usize];
            for k in 0..4 {
                bm[k] |= m[k];
            }
        } else if at(p + 1) == b'-' && p + 2 < ec {
            p += 2;
            let (lo, hi) = (at(p - 2), at(p));
            let mut c = lo;
            while c <= hi {
                bm[(c >> 6) as usize] |= 1 << (c & 63);
                if c == 255 {
                    break;
                }
                c += 1;
            }
        } else {
            let c = at(p);
            bm[(c >> 6) as usize] |= 1 << (c & 63);
        }
    }
    if !sig {
        for w in &mut bm {
            *w = !*w;
        }
    }
    bm
}

/// What a match must start with (see [`MatchState::start_filter`]).
#[derive(Clone, Copy, Debug)]
pub enum Start {
    /// No constraint: every position must be tried.
    Any,
    /// The first pattern item is this literal byte, not optional.
    Byte(u8),
    /// The first pattern item is the single-character class ending at this pattern index, not optional.
    Class(usize),
    /// (compiled patterns) the first item is a single-character class: index of its bitmap in the program.
    Bm(u32),
}

impl<'a> MatchState<'a> {
    /// `prepstate` + `reprepstate`.
    #[inline(always)]
    pub fn new(src: &'a [u8], pat: &'a [u8]) -> Self {
        MatchState {
            src,
            pat,
            matchdepth: MAXCCALLS,
            level: 0,
            error: None,
            capture: [const { MaybeUninit::uninit() }; MAXCAPTURES],
            plan: 0,
            prog: None,
            pin: patc::PinGuard::NONE,
        }
    }

    /// Capture `i` (`i < level`: initialised by `start_capture`).
    #[inline(always)]
    #[allow(unsafe_code)]
    fn cap(&self, i: usize) -> Capture {
        debug_assert!(i < self.level);
        // SAFETY: slots below `level` were written by `start_capture` (the only place that raises `level`).
        unsafe { self.capture[i].assume_init() }
    }

    #[inline(always)]
    fn cap_mut(&mut self, i: usize) -> &mut Capture {
        debug_assert!(i < self.level);
        #[allow(unsafe_code)]
        // SAFETY: as `cap`.
        unsafe {
            self.capture[i].assume_init_mut()
        }
    }

    /// `reprepstate`: reset per-match fields.
    pub fn reprep(&mut self) {
        self.matchdepth = MAXCCALLS;
        self.level = 0;
    }

    /// Number of captures (finished or not).
    pub fn level(&self) -> usize {
        self.level
    }

    /// A necessary condition on the first subject byte of any match of the whole pattern: its first
    /// item is a single-character class without `*`/`?`/`-` (so a match at `s` needs
    /// `singlematch(s)`; no capture, `%b`, `%f`, back reference or final `$`). Positions failing it
    /// can be skipped without calling `match` (which would return "no match" there without
    /// side effects).
    pub fn start_filter(&mut self) -> Start {
        self.set_plan();
        if let Some(pg) = &self.prog {
            return pg.start;
        }
        self.start_filter_inner()
    }

    /// Keep this state's compiled program valid across foreign code (see `patc::Cache`): call before running anything
    /// that may call back into pattern matching (a `gsub` replacement function) or before keeping the state alive
    /// between match calls.
    pub fn pin_prog(&mut self) {
        if self.prog.is_some() {
            self.pin = patc::PinGuard::pin();
        }
    }

    /// Only the one-item analysis (anchored patterns need no start filter); looks the pattern up in the compiled-pattern
    /// cache (BUNDLE-S3 `patc`).
    #[inline]
    pub fn set_plan(&mut self) {
        let m = crate::strfast::mask();
        if m & crate::strfast::PATC != 0 {
            if let Some(pg) = patc::lookup(self.pat) {
                self.plan = if m & crate::strfast::PAT1 != 0 {
                    pg.plan
                } else {
                    0
                };
                self.prog = Some(pg);
                return;
            }
        }
        self.plan = if m & crate::strfast::PAT1 != 0 {
            self.analyze()
        } else {
            0
        };
    }

    /// The one-item shape of the pattern (see [`Plan`]); `None` for anything else, including every malformed
    /// pattern (the general matcher reports those lazily, exactly as before).
    #[inline(never)]
    fn analyze(&mut self) -> u64 {
        let pat = self.pat;
        if pat.len() >= 1 << 24 {
            return 0;
        }
        let mut p = 0;
        let cap = pat.first() == Some(&b'(');
        if cap {
            if pat.get(1) == Some(&b')') {
                return 0; // position capture
            }
            p = 1;
        }
        let Some(&c) = pat.get(p) else {
            return 0;
        };
        let (kind, byte, setp, setep, ep) = match c {
            b'(' | b')' => return 0,
            b'$' if p + 1 == pat.len() => return 0,
            L_ESC => {
                let Some(&d) = pat.get(p + 1) else {
                    return 0;
                };
                if d == b'b' || d == b'f' || d.is_ascii_digit() {
                    return 0;
                }
                {
                    let m = CLBIT[d as usize];
                    if m == 0 {
                        (K_LIT, d, 0, 0, p + 2)
                    } else {
                        (
                            if d.is_ascii_lowercase() {
                                K_CPOS
                            } else {
                                K_CNEG
                            },
                            d,
                            m as usize,
                            0,
                            p + 2,
                        )
                    }
                }
            }
            b'[' => {
                let ep = self.classend(p);
                if ep == ERR {
                    self.error = None;
                    return 0;
                }
                (K_SET, 0, p, ep, ep)
            }
            b'.' => (K_ANY, 0, 0, 0, p + 1),
            c => (K_LIT, c, 0, 0, p + 1),
        };
        let mut ep = ep;
        let mut quant = 0u64;
        if let Some(&q) = pat.get(ep) {
            quant = match q {
                b'?' => 1,
                b'*' => 2,
                b'+' => 3,
                b'-' => 4,
                _ => 0,
            };
            if quant != 0 {
                ep += 1;
            }
        }
        if cap {
            if pat.get(ep) != Some(&b')') {
                return 0;
            }
            ep += 1;
        }
        if ep != pat.len() {
            return 0;
        }
        kind | (cap as u64) << 3
            | quant << 4
            | (byte as u64) << 8
            | (setp as u64) << 16
            | (setep as u64) << 40
    }

    /// Number of consecutive subject bytes from `s` (at most `max`) that match the item of `pl`.
    fn run_len(&self, pl: Plan, s: usize, max: usize) -> usize {
        let rest = &self.src[s..];
        let rest = &rest[..rest.len().min(max)];
        let b = pl.byte();
        match pl.kind() {
            K_LIT => rest.iter().take_while(|&&c| c == b).count(),
            K_ANY => rest.len(),
            K_CPOS => {
                let m = pl.set_p() as u16;
                rest.iter()
                    .take_while(|&&c| CTYPE[c as usize] & m != 0)
                    .count()
            }
            K_CNEG => {
                let m = pl.set_p() as u16;
                rest.iter()
                    .take_while(|&&c| CTYPE[c as usize] & m == 0)
                    .count()
            }
            _ => {
                if let Some(pg) = &self.prog {
                    // the compiled program already holds the set's bitmap (item 0, or 1 after the capture opener)
                    let it = pg.item(pl.cap() as usize);
                    if it.kind == patc::I_SINGLE && it.cls == patc::CL_SET {
                        let bm = pg.set(it.x as usize);
                        return rest.iter().take_while(|&&c| patc::bm_has(bm, c)).count();
                    }
                }
                let (p, ep) = (pl.set_p(), pl.set_ep());
                let n = rest.len();
                let head = n.min(32);
                let mut i = 0;
                while i < head {
                    if !self.matchbracketclass(rest[i], p, ep - 1) {
                        return i;
                    }
                    i += 1;
                }
                if i == n {
                    return n;
                }
                let bm = self.build_set(p, ep - 1);
                while i < n && bm[(rest[i] >> 6) as usize] >> (rest[i] & 63) & 1 != 0 {
                    i += 1;
                }
                i
            }
        }
    }

    /// The membership bitmap of the set `p..=ec` (same semantics as [`Self::matchbracketclass`]).
    fn build_set(&self, p: usize, ec: usize) -> [u64; 4] {
        set_bitmap(self.pat, p, ec)
    }

    /// Match of a [`Plan`] at `s` (what `match_inner` returns for that pattern: a position or [`NOM`]).
    #[inline(never)]
    fn fast_match(&mut self, pl: Plan, s: usize) -> usize {
        let e = match pl.quant() {
            0 => {
                if self.run_len(pl, s, 1) == 1 {
                    s + 1
                } else {
                    return NOM;
                }
            }
            1 => s + self.run_len(pl, s, 1),
            2 => s + self.run_len(pl, s, usize::MAX),
            3 => {
                let n = self.run_len(pl, s, usize::MAX);
                if n == 0 {
                    return NOM;
                }
                s + n
            }
            _ => s, // `-`: the empty remainder matches at once
        };
        if pl.cap() {
            self.capture[0] = MaybeUninit::new(Capture {
                init: s,
                len: (e - s) as isize,
            });
            self.level = 1;
        }
        e
    }

    fn start_filter_inner(&mut self) -> Start {
        let pat = self.pat;
        let Some(&c) = pat.first() else {
            return Start::Any;
        };
        match c {
            b'(' | b')' => return Start::Any,
            b'$' if pat.len() == 1 => return Start::Any,
            L_ESC => match pat.get(1) {
                None | Some(b'b' | b'f' | b'0'..=b'9') => return Start::Any,
                _ => {}
            },
            _ => {}
        }
        let ep = self.classend(0);
        if ep == ERR {
            self.error = None; // reported by `match` itself
            return Start::Any;
        }
        match self.p(ep) {
            b'*' | b'?' | b'-' => Start::Any,
            _ => match c {
                b'.' | L_ESC | b'[' => Start::Class(ep),
                _ => Start::Byte(c),
            },
        }
    }

    /// First position `>= s` that passes the filter `f` (`src.len()` if none; positions
    /// at or beyond the end are returned unchanged).
    #[inline]
    pub fn skip(&self, f: Start, s: usize) -> usize {
        let len = self.src.len();
        if s >= len {
            return s;
        }
        match f {
            Start::Any => s,
            Start::Byte(b) => find_byte(&self.src[s..], b).map_or(len, |i| s + i),
            Start::Bm(k) => match &self.prog {
                Some(pg) => {
                    let bm = pg.set(k as usize);
                    let src = self.src;
                    let mut i = s;
                    while i < len && !patc::bm_has(bm, src[i]) {
                        i += 1;
                    }
                    i
                }
                None => s,
            },
            Start::Class(_) if matches!(self.plan & 7, K_CPOS | K_CNEG) => {
                // `K_CPOS` / `K_CNEG` (kinds 5 and 6)
                let m = Plan(self.plan).set_p() as u16;
                let rest = &self.src[s..];
                if self.plan & 7 == K_CPOS {
                    rest.iter().position(|&c| CTYPE[c as usize] & m != 0)
                } else {
                    rest.iter().position(|&c| CTYPE[c as usize] & m == 0)
                }
                .map_or(len, |i| s + i)
            }
            Start::Class(ep) => {
                let mut s = s;
                while s < len && !self.singlematch(s, 0, ep) {
                    s += 1;
                }
                s
            }
        }
    }

    /// Number of values `push_captures` produces (`whole`: a whole match is available).
    #[inline]
    pub fn ncaptures(&self, whole: bool) -> usize {
        if self.level == 0 && whole {
            1
        } else {
            self.level
        }
    }

    /// The C string view: reading at/after the end yields NUL.
    #[inline]
    fn p(&self, i: usize) -> u8 {
        self.pat.get(i).copied().unwrap_or(0)
    }
    #[inline]
    fn s(&self, i: usize) -> u8 {
        self.src.get(i).copied().unwrap_or(0)
    }

    /// Record an error message; returns [`ERR`].
    #[cold]
    fn fail(&mut self, msg: Vec<u8>) -> usize {
        self.error = Some(msg);
        ERR
    }

    fn check_capture(&mut self, l: u8) -> usize {
        let l = l as i32 - b'1' as i32;
        if l < 0 || l as usize >= self.level || self.cap(l as usize).len == CAP_UNFINISHED {
            return self.fail(format!("invalid capture index %{}", l + 1).into_bytes());
        }
        l as usize
    }

    fn capture_to_close(&mut self) -> usize {
        let mut level = self.level;
        while level > 0 {
            level -= 1;
            if self.cap(level).len == CAP_UNFINISHED {
                return level;
            }
        }
        self.fail(b"invalid pattern capture".to_vec())
    }

    /// End of the single-character class at `p` (or [`ERR`]).
    #[inline(always)]
    fn classend(&mut self, mut p: usize) -> usize {
        let c = self.p(p);
        p += 1;
        match c {
            L_ESC => {
                if p >= self.pat.len() {
                    return self.fail(b"malformed pattern (ends with '%')".to_vec());
                }
                p + 1
            }
            b'[' => {
                if self.p(p) == b'^' {
                    p += 1;
                }
                loop {
                    // look for a ']'
                    if p >= self.pat.len() {
                        return self.fail(b"malformed pattern (missing ']')".to_vec());
                    }
                    let c = self.p(p);
                    p += 1;
                    if c == L_ESC && p < self.pat.len() {
                        p += 1;
                    }
                    if self.p(p) == b']' {
                        break;
                    }
                }
                p + 1
            }
            _ => p,
        }
    }

    /// `matchbracketclass`: `p` points at '[', `ec` at the closing ']'.
    fn matchbracketclass(&self, c: u8, mut p: usize, ec: usize) -> bool {
        let mut sig = true;
        if self.p(p + 1) == b'^' {
            sig = false;
            p += 1;
        }
        loop {
            p += 1;
            if p >= ec {
                break;
            }
            if self.p(p) == L_ESC {
                p += 1;
                if match_class(c, self.p(p)) {
                    return sig;
                }
            } else if self.p(p + 1) == b'-' && p + 2 < ec {
                p += 2;
                if self.p(p - 2) <= c && c <= self.p(p) {
                    return sig;
                }
            } else if self.p(p) == c {
                return sig;
            }
        }
        !sig
    }

    #[inline(always)]
    fn singlematch(&self, s: usize, p: usize, ep: usize) -> bool {
        let Some(&c) = self.src.get(s) else {
            return false;
        };
        match self.p(p) {
            b'.' => true,
            L_ESC => match_class(c, self.p(p + 1)),
            b'[' => self.matchbracketclass(c, p, ep - 1),
            pc => pc == c,
        }
    }

    fn matchbalance(&mut self, mut s: usize, p: usize) -> usize {
        if p + 1 >= self.pat.len() {
            return self.fail(b"malformed pattern (missing arguments to '%b')".to_vec());
        }
        if s >= self.src.len() || self.src[s] != self.pat[p] {
            return NOM;
        }
        let b = self.pat[p];
        let e = self.pat[p + 1];
        let mut cont = 1;
        loop {
            s += 1;
            if s >= self.src.len() {
                break;
            }
            let c = self.src[s];
            if c == e {
                cont -= 1;
                if cont == 0 {
                    return s + 1;
                }
            } else if c == b {
                cont += 1;
            }
        }
        NOM
    }

    fn max_expand(&mut self, s: usize, p: usize, ep: usize) -> usize {
        let mut i = 0usize;
        while self.singlematch(s + i, p, ep) {
            i += 1;
        }
        loop {
            let r = self.m(s + i, ep + 1);
            if r != NOM {
                return r; // a match or an error
            }
            if i == 0 {
                return NOM;
            }
            i -= 1;
        }
    }

    fn min_expand(&mut self, mut s: usize, p: usize, ep: usize) -> usize {
        loop {
            let r = self.m(s, ep + 1);
            if r != NOM {
                return r;
            } else if self.singlematch(s, p, ep) {
                s += 1;
            } else {
                return NOM;
            }
        }
    }

    fn start_capture(&mut self, s: usize, p: usize, what: isize) -> usize {
        let level = self.level;
        if level >= MAXCAPTURES {
            return self.fail(b"too many captures".to_vec());
        }
        self.capture[level] = MaybeUninit::new(Capture { init: s, len: what });
        self.level = level + 1;
        let res = self.m(s, p);
        if res == NOM {
            self.level -= 1;
        }
        res
    }

    fn end_capture(&mut self, s: usize, p: usize) -> usize {
        let l = self.capture_to_close();
        if l == ERR {
            return ERR;
        }
        let c = self.cap_mut(l);
        c.len = (s - c.init) as isize;
        let res = self.m(s, p);
        if res == NOM {
            self.cap_mut(l).len = CAP_UNFINISHED;
        }
        res
    }

    fn match_capture(&mut self, s: usize, l: u8) -> usize {
        let l = self.check_capture(l);
        if l == ERR {
            return ERR;
        }
        let c = self.cap(l);
        let len = c.len as usize; // CAP_POSITION (-2) wraps to a huge size_t, as in C
        let init = c.init;
        if c.len >= 0
            && self.src.len() - s >= len
            && self.src[init..init + len] == self.src[s..s + len]
        {
            s + len
        } else {
            NOM
        }
    }

    /// `match`: try to match the pattern from `p` at subject position `s`; returns the end position.
    pub fn do_match(&mut self, s: usize, p: usize) -> PResult<Option<usize>> {
        if p == 0 && self.plan != 0 {
            return Ok(match self.fast_match(Plan(self.plan), s) {
                NOM => None,
                e => Some(e),
            });
        }
        if p == 0 {
            if let Some(pg) = self.prog {
                let r = if pg.simple && self.matchdepth != 0 {
                    self.csimple(pg, s)
                } else {
                    self.cm(pg, s, 0, self.matchdepth)
                };
                return match r {
                    NOM => Ok(None),
                    ERR => Err(self.error.take().unwrap_or_default()),
                    e => Ok(Some(e)),
                };
            }
        }
        match self.m(s, p) {
            NOM => Ok(None),
            ERR => Err(self.error.take().unwrap_or_default()),
            e => Ok(Some(e)),
        }
    }

    /// `match` with the internal result encoding (a position, [`NOM`] or [`ERR`]).
    #[inline(never)]
    fn m(&mut self, s: usize, p: usize) -> usize {
        if self.matchdepth == 0 {
            return self.fail(b"pattern too complex".to_vec());
        }
        self.matchdepth -= 1;
        let r = self.match_inner(s, p);
        self.matchdepth += 1;
        r
    }

    fn match_inner(&mut self, mut s: usize, mut p: usize) -> usize {
        let pend = self.pat.len();
        loop {
            if p == pend {
                return s;
            }
            match self.pat[p] {
                b'(' => {
                    return if self.p(p + 1) == b')' {
                        self.start_capture(s, p + 2, CAP_POSITION)
                    } else {
                        self.start_capture(s, p + 1, CAP_UNFINISHED)
                    };
                }
                b')' => return self.end_capture(s, p + 1),
                b'$' if p + 1 == pend => {
                    return if s == self.src.len() { s } else { NOM };
                }
                L_ESC => match self.p(p + 1) {
                    b'b' => match self.matchbalance(s, p + 2) {
                        NOM => return NOM,
                        ERR => return ERR,
                        ns => {
                            s = ns;
                            p += 4;
                            continue;
                        }
                    },
                    b'f' => {
                        p += 2;
                        if self.p(p) != b'[' {
                            return self.fail(b"missing '[' after '%f' in pattern".to_vec());
                        }
                        let ep = self.classend(p);
                        if ep == ERR {
                            return ERR;
                        }
                        let previous = if s == 0 { 0 } else { self.src[s - 1] };
                        if !self.matchbracketclass(previous, p, ep - 1)
                            && self.matchbracketclass(self.s(s), p, ep - 1)
                        {
                            p = ep;
                            continue;
                        }
                        return NOM;
                    }
                    d @ b'0'..=b'9' => match self.match_capture(s, d) {
                        NOM => return NOM,
                        ERR => return ERR,
                        ns => {
                            s = ns;
                            p += 2;
                            continue;
                        }
                    },
                    _ => {}
                },
                _ => {}
            }
            let ep = self.classend(p);
            if ep == ERR {
                return ERR;
            }
            let epc = self.p(ep);
            if !self.singlematch(s, p, ep) {
                if epc == b'*' || epc == b'?' || epc == b'-' {
                    p = ep + 1;
                    continue;
                }
                return NOM;
            }
            match epc {
                b'?' => {
                    let r = self.m(s + 1, ep + 1);
                    if r != NOM {
                        return r;
                    }
                    p = ep + 1;
                }
                b'+' => return self.max_expand(s + 1, p, ep),
                b'*' => return self.max_expand(s, p, ep),
                b'-' => return self.min_expand(s, p, ep),
                _ => {
                    s += 1;
                    p = ep;
                }
            }
        }
    }

    /// `get_onecapture`: capture `i` (0-based) of a match spanning `s..e`.
    pub fn get_onecapture(&self, i: usize, s: usize, e: usize) -> PResult<Cap<'a>> {
        if i >= self.level {
            if i != 0 {
                return Err(format!("invalid capture index %{}", i + 1).into_bytes());
            }
            Ok(Cap::Str(&self.src[s..e]))
        } else {
            let c = self.cap(i);
            if c.len == CAP_UNFINISHED {
                return err("unfinished capture");
            }
            if c.len == CAP_POSITION {
                Ok(Cap::Pos(c.init as i64 + 1))
            } else {
                Ok(Cap::Str(&self.src[c.init..c.init + c.len as usize]))
            }
        }
    }

    /// `push_captures`: all captures (or the whole match `s..e` when there are none).
    /// `whole = None` mirrors passing `s == NULL` (used by `find`).
    pub fn captures(&self, whole: Option<(usize, usize)>) -> PResult<Vec<Cap<'a>>> {
        let nlevels = if self.level == 0 && whole.is_some() {
            1
        } else {
            self.level
        };
        let (s, e) = whole.unwrap_or((0, 0));
        (0..nlevels).map(|i| self.get_onecapture(i, s, e)).collect()
    }
}

/// `posrelatI`: translate a relative initial position (>= 1) for a string of length `len`.
pub fn posrelat_i(pos: i64, len: usize) -> usize {
    if pos > 0 {
        pos as usize
    } else if pos == 0 || pos < -(len as i64) {
        1
    } else {
        (len as i64 + pos + 1) as usize
    }
}

/// The plain `string.find` of `s` for `p` from the 1-based (possibly negative) `init` (`str_find_aux`'s plain branch): the 0-based start of the
/// first occurrence, `None` when there is none or `init` lies beyond `#s + 1`. An empty `p` is found at the start position. Shared by the
/// library and the trace helper (BUNDLE-A `libfind`).
pub fn find_plain_pos(s: &[u8], p: &[u8], init: i64) -> Option<usize> {
    let init = posrelat_i(init, s.len()) - 1;
    if init > s.len() {
        return None;
    }
    memfind(&s[init..], p).map(|pos| init + pos)
}

/// Plain substring search (`lmemfind`).
pub fn memfind(s1: &[u8], s2: &[u8]) -> Option<usize> {
    if s2.is_empty() {
        return Some(0);
    }
    if s2.len() > s1.len() {
        return None;
    }
    let (first, rest) = (s2[0], &s2[1..]);
    let last = s1.len() - s2.len(); // last possible start
    let mut i = 0;
    while i <= last {
        let j = i + find_byte(&s1[i..=last], first)?;
        if &s1[j + 1..j + s2.len()] == rest {
            return Some(j);
        }
        i = j + 1;
    }
    None
}

pub fn nospecials(p: &[u8]) -> bool {
    !p.iter().any(|c| b"^$*+?.([%-".contains(c) && *c != 0)
}

/// Result of `string.find` / `string.match`.
#[derive(Debug, PartialEq)]
pub enum FindResult<'a> {
    NotFound,
    /// `find`: 1-based start, end, and captures; `match`: captures only (start/end still filled).
    Found {
        start: usize,
        end: usize,
        caps: Vec<Cap<'a>>,
    },
}

/// `str_find_aux` core. `init` is the raw Lua argument (default 1). `plain` is
/// `lua_toboolean(arg 4)`. Calls `found(ms, start0, end)` on a match (`ms = None` for a plain
/// search) and returns its result; `Ok(None)` = not found.
pub fn find_with<'a, R>(
    s: &'a [u8],
    p: &'a [u8],
    init: i64,
    find: bool,
    plain: bool,
    found: impl FnOnce(Option<&MatchState<'a>>, usize, usize) -> R,
) -> PResult<Option<R>> {
    let ls = s.len();
    if find && (plain || nospecials(p)) {
        if let Some(s2) = find_plain_pos(s, p, init) {
            return Ok(Some(found(None, s2, s2 + p.len())));
        }
        return Ok(None);
    }
    let init = posrelat_i(init, ls) - 1;
    if init > ls {
        return Ok(None);
    }
    let (anchor, p) = match p.first() {
        Some(b'^') => (true, &p[1..]),
        _ => (false, p),
    };
    let mut ms = MatchState::new(s, p);
    let f = if anchor {
        ms.set_plan();
        Start::Any
    } else {
        ms.start_filter()
    };
    let mut s1 = init;
    loop {
        s1 = ms.skip(f, s1);
        ms.reprep();
        if let Some(res) = ms.do_match(s1, 0)? {
            return Ok(Some(found(Some(&ms), s1, res)));
        }
        let more = s1 < s.len();
        s1 += 1;
        if !(more && !anchor) {
            break;
        }
    }
    Ok(None)
}

/// `str_find_aux`, collecting the captures.
pub fn find_aux<'a>(
    s: &'a [u8],
    p: &'a [u8],
    init: i64,
    find: bool,
    plain: bool,
) -> PResult<FindResult<'a>> {
    let r = find_with(
        s,
        p,
        init,
        find,
        plain,
        |ms, s1, end| -> PResult<FindResult<'a>> {
            let caps = match ms {
                None => Vec::new(),
                Some(ms) if find => ms.captures(None)?,
                Some(ms) => ms.captures(Some((s1, end)))?,
            };
            Ok(FindResult::Found {
                start: s1 + 1,
                end,
                caps,
            })
        },
    )?;
    r.unwrap_or(Ok(FindResult::NotFound))
}

/// State of a `gmatch` iterator.
pub struct GMatch<'a> {
    ms: MatchState<'a>,
    start: Start,
    src: usize,
    lastmatch: Option<usize>,
}

impl<'a> GMatch<'a> {
    /// `gmatch(s, p [, init])`; `init` is the raw Lua argument (default 1).
    pub fn new(s: &'a [u8], p: &'a [u8], init: i64) -> Self {
        let mut init = posrelat_i(init, s.len()) - 1;
        if init > s.len() {
            init = s.len() + 1;
        }
        let mut ms = MatchState::new(s, p);
        let start = ms.start_filter();
        ms.pin_prog();
        GMatch {
            start,
            ms,
            src: init,
            lastmatch: None,
        }
    }

    /// `gmatch_aux`: next match's captures, or `None` when exhausted.
    pub fn next_match(&mut self) -> PResult<Option<Vec<Cap<'a>>>> {
        let mut src = self.src;
        while src <= self.ms.src.len() {
            src = self.ms.skip(self.start, src);
            self.ms.reprep();
            if let Some(e) = self.ms.do_match(src, 0)? {
                if Some(e) != self.lastmatch {
                    self.src = e;
                    self.lastmatch = Some(e);
                    return Ok(Some(self.ms.captures(Some((src, e)))?));
                }
            }
            src += 1;
        }
        Ok(None)
    }
}

/// A successful match handed to a `gsub` replacement callback.
pub struct Matched<'m, 'a> {
    ms: &'m MatchState<'a>,
    s: usize,
    e: usize,
}

impl<'a> Matched<'_, 'a> {
    /// All captures (arguments of a replacement function).
    pub fn all(&self) -> PResult<Vec<Cap<'a>>> {
        self.ms.captures(Some((self.s, self.e)))
    }
    /// Number of values [`Matched::all`] returns.
    pub fn count(&self) -> usize {
        self.ms.ncaptures(true)
    }
    /// Capture `i` (`i < count()`).
    pub fn get(&self, i: usize) -> PResult<Cap<'a>> {
        self.ms.get_onecapture(i, self.s, self.e)
    }
    /// First capture (key for a replacement table).
    pub fn first(&self) -> PResult<Cap<'a>> {
        self.ms.get_onecapture(0, self.s, self.e)
    }
    /// The whole match.
    pub fn whole(&self) -> &'a [u8] {
        &self.ms.src[self.s..self.e]
    }
}

/// Value produced by a function/table replacement.
pub enum ReplValue {
    /// `nil` / `false`: keep the original text.
    Keep,
    /// A string (or number converted to a string).
    Bytes(Vec<u8>),
}

/// Replacement argument of `gsub`.
pub enum Replacement<'r> {
    /// A string/number replacement (already converted to bytes); `%` escapes are processed.
    Str(&'r [u8]),
    /// Function or table: called for each match. It appends the replacement text to the output buffer and returns
    /// `true`, or returns `false` (and appends nothing) for `nil` / `false` (keep the original text).
    Callback(&'r mut dyn FnMut(&Matched<'_, '_>) -> PResult<ReplValue>),
    /// Like `Callback`, without a result allocation per match (B3-2): the function appends the replacement text to
    /// the output buffer and returns `true`, or returns `false` (appending nothing) to keep the original text.
    Append(&'r mut dyn FnMut(&Matched<'_, '_>, &mut Vec<u8>) -> PResult<bool>),
}

fn add_s(ms: &MatchState<'_>, out: &mut Vec<u8>, s: usize, e: usize, news: &[u8]) -> PResult<()> {
    let mut i = 0;
    while i < news.len() {
        // copy the run up to the next '%'
        let Some(k) = find_byte(&news[i..], L_ESC) else {
            out.extend_from_slice(&news[i..]);
            break;
        };
        out.extend_from_slice(&news[i..i + k]);
        i += k + 1;
        let d = news.get(i).copied().unwrap_or(0);
        if d == L_ESC {
            out.push(L_ESC);
        } else if d == b'0' {
            out.extend_from_slice(&ms.src[s..e]);
        } else if d.is_ascii_digit() {
            match ms.get_onecapture((d - b'1') as usize, s, e)? {
                Cap::Str(b) => out.extend_from_slice(b),
                Cap::Pos(n) => out.extend_from_slice(n.to_string().as_bytes()),
            }
        } else {
            return err("invalid use of '%' in replacement string");
        }
        i += 1;
    }
    Ok(())
}

/// `str_gsub` core. `max_s` is the raw (or defaulted `len+1`) max replacements. The new string is appended to
/// `out` (B3-2: a caller-owned buffer); the flag says whether anything was replaced (`false`: `out` is unspecified
/// and the caller returns the original string), and the substitution count follows.
pub fn gsub(
    src: &[u8],
    pat: &[u8],
    repl: Replacement<'_>,
    max_s: i64,
) -> PResult<(Option<Vec<u8>>, i64)> {
    let mut out = Vec::new();
    let (changed, n) = gsub_into(src, pat, repl, max_s, &mut out)?;
    Ok((changed.then_some(out), n))
}

/// [`gsub`] appending to a caller-owned buffer.
pub fn gsub_into(
    src: &[u8],
    pat: &[u8],
    repl: Replacement<'_>,
    max_s: i64,
    out: &mut Vec<u8>,
) -> PResult<(bool, i64)> {
    let (anchor, p) = match pat.first() {
        Some(b'^') => (true, &pat[1..]),
        _ => (false, pat),
    };
    let mut repl = repl;
    let mut ms = MatchState::new(src, p);
    let f = if anchor {
        ms.set_plan();
        Start::Any
    } else {
        ms.start_filter()
    };
    if !matches!(repl, Replacement::Str(_)) {
        ms.pin_prog(); // the replacement function / table may run any Lua code, including pattern calls
    }
    let mut pos = 0usize;
    let mut lastmatch: Option<usize> = None;
    let mut n: i64 = 0;
    let mut changed = false;
    while n < max_s {
        let next = ms.skip(f, pos);
        out.extend_from_slice(&src[pos..next]);
        pos = next;
        ms.reprep();
        let m = ms.do_match(pos, 0)?;
        match m {
            Some(e) if Some(e) != lastmatch => {
                n += 1;
                match &mut repl {
                    Replacement::Str(news) => {
                        add_s(&ms, out, pos, e, news)?;
                        changed = true;
                    }
                    Replacement::Callback(f) => {
                        let r = f(&Matched { ms: &ms, s: pos, e })?;
                        match r {
                            ReplValue::Keep => out.extend_from_slice(&src[pos..e]),
                            ReplValue::Bytes(b) => {
                                out.extend_from_slice(&b);
                                changed = true;
                            }
                        }
                    }
                    Replacement::Append(f) => {
                        if f(&Matched { ms: &ms, s: pos, e }, out)? {
                            changed = true;
                        } else {
                            out.extend_from_slice(&src[pos..e]);
                        }
                    }
                }
                pos = e;
                lastmatch = Some(e);
            }
            _ => {
                if pos < src.len() {
                    out.push(src[pos]);
                    pos += 1;
                } else {
                    break;
                }
            }
        }
        if anchor {
            break;
        }
    }
    if changed {
        out.extend_from_slice(&src[pos..]);
    }
    Ok((changed, n))
}
