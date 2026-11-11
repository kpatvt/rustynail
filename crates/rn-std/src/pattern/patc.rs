//! BUNDLE-S3 `patc`: compiled patterns. A pattern string seen twice at the same address and length is compiled once into a
//! flat item list (literal runs, single-character items with their class as a 256-bit bitmap, captures, `%b`, `%f`, back
//! references, the end) and cached thread-locally; every hit verifies the pattern BYTES, so GC address reuse can never
//! produce a wrong hit. The cache lives outside the Lua heap (not charged to GC debt).
//!
//! The compiled matcher has the same recursion structure as `MatchState::match_inner` (a recursive [`MatchState::cm`]
//! call exactly where `m` is called: `?`, the backtracking positions of `*` / `+` / `-`, captures), so `matchdepth` /
//! "pattern too complex" and the 32-capture limit behave as before. Malformed patterns are compiled to an error item at
//! the position where the reference reports them (lazily, when matching reaches the item).
use super::*;
use std::cell::{Cell, UnsafeCell};
use std::marker::PhantomData;

pub(super) const I_END: u8 = 0;
/// a run of literal bytes (`x` = offset in `lits`, `y` = length)
pub(super) const I_RUN: u8 = 1;
/// one single-character item with a quantifier (`q`)
pub(super) const I_SINGLE: u8 = 2;
pub(super) const I_OPEN: u8 = 3;
pub(super) const I_OPENPOS: u8 = 4;
pub(super) const I_CLOSE: u8 = 5;
pub(super) const I_DOLLAR: u8 = 6;
pub(super) const I_BAL: u8 = 7;
pub(super) const I_FRONT: u8 = 8;
pub(super) const I_BACKREF: u8 = 9;
pub(super) const I_ERR: u8 = 10;

pub(super) const CL_LIT: u8 = 0;
pub(super) const CL_ANY: u8 = 1;
pub(super) const CL_SET: u8 = 2;

pub(super) const Q_NONE: u8 = 0;
pub(super) const Q_OPT: u8 = 1;
pub(super) const Q_STAR: u8 = 2;
pub(super) const Q_PLUS: u8 = 3;
pub(super) const Q_MIN: u8 = 4;

const E_PCT: u8 = 0;
const E_BRACKET: u8 = 1;
const E_BAL: u8 = 2;
const E_FRONT: u8 = 3;

#[derive(Clone, Copy)]
pub(super) struct Item {
    pub kind: u8,
    pub q: u8,
    pub cls: u8,
    /// literal byte / `%b` open byte / back-reference digit / error code
    pub b: u8,
    /// set index / literal offset / `%b` close byte
    pub x: u16,
    pub y: u16,
}

impl Item {
    const fn new(kind: u8) -> Item {
        Item {
            kind,
            q: 0,
            cls: 0,
            b: 0,
            x: 0,
            y: 0,
        }
    }
}

/// Capacity of a compiled program: everything lives inline in ONE allocation (no pointer chains on the hot path); a
/// pattern that does not fit, or is longer than `MAX_KEY`, is never compiled (the general matcher runs it).
pub(super) const MAX_ITEMS: usize = 32;
pub(super) const MAX_SETS: usize = 8;
pub(super) const MAX_LITS: usize = 48;
pub(super) const MAX_KEY: usize = 48;

pub(crate) struct Prog {
    /// the cache slot bookkeeping: the address and length of the pattern string when it was compiled / seen
    addr: usize,
    len: usize,
    /// `S_EMPTY`, `S_SEEN` (seen once, nothing compiled), `S_OK`, `S_NOFIT` (does not fit: never used)
    state: u8,
    key: [u8; MAX_KEY],
    pub(super) items: [Item; MAX_ITEMS],
    pub(super) sets: [[u64; 4]; MAX_SETS],
    pub(super) lits: [u8; MAX_LITS],
    pub(super) start: Start,
    /// the one-item analysis of `MatchState::analyze` (0 = none)
    pub(super) plan: u64,
    /// straight-line program: only literal runs, unquantified single-character items, a final `$` and the end (no
    /// choice points, no captures): matched by a plain loop, with no recursion
    pub(super) simple: bool,
}

const S_EMPTY: u8 = 0;
const S_SEEN: u8 = 1;
const S_OK: u8 = 2;
const S_NOFIT: u8 = 3;

impl Prog {
    const EMPTY: Prog = Prog {
        addr: 0,
        len: 0,
        state: S_EMPTY,
        key: [0; MAX_KEY],
        items: [Item::new(I_END); MAX_ITEMS],
        sets: [[0; 4]; MAX_SETS],
        lits: [0; MAX_LITS],
        start: Start::Any,
        plan: 0,
        simple: false,
    };
}

impl Prog {
    #[inline(always)]
    pub(super) fn item(&self, i: usize) -> Item {
        self.items[i & (MAX_ITEMS - 1)]
    }
    #[inline(always)]
    pub(super) fn set(&self, k: usize) -> &[u64; 4] {
        &self.sets[k & (MAX_SETS - 1)]
    }
    #[inline(always)]
    fn lit(&self, i: usize) -> u8 {
        self.lits[i % MAX_LITS]
    }
}

/// Slice equality without a `bcmp` call for the short patterns this cache deals with.
#[inline(always)]
pub(super) fn same_bytes(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let n = a.len();
    if n >= 8 {
        if n > 16 {
            return a == b;
        }
        let w = |x: &[u8], i: usize| u64::from_le_bytes(x[i..i + 8].try_into().unwrap());
        return w(a, 0) == w(b, 0) && w(a, n - 8) == w(b, n - 8);
    }
    if n >= 4 {
        let w = |x: &[u8], i: usize| u32::from_le_bytes(x[i..i + 4].try_into().unwrap());
        return w(a, 0) == w(b, 0) && w(a, n - 4) == w(b, n - 4);
    }
    let mut i = 0;
    while i < n {
        if a[i] != b[i] {
            return false;
        }
        i += 1;
    }
    true
}

#[inline(always)]
pub(super) fn bm_has(bm: &[u64; 4], c: u8) -> bool {
    bm[(c >> 6) as usize] >> (c & 63) & 1 != 0
}

fn err_msg(code: u8) -> &'static [u8] {
    match code {
        E_PCT => b"malformed pattern (ends with '%')",
        E_BRACKET => b"malformed pattern (missing ']')",
        E_BAL => b"malformed pattern (missing arguments to '%b')",
        _ => b"missing '[' after '%f' in pattern",
    }
}

/// `classend` on a plain pattern: the end of the single-character class at `p`, or the error code.
fn classend(pat: &[u8], mut p: usize) -> Result<usize, u8> {
    let at = |i: usize| pat.get(i).copied().unwrap_or(0);
    let c = at(p);
    p += 1;
    match c {
        L_ESC => {
            if p >= pat.len() {
                return Err(E_PCT);
            }
            Ok(p + 1)
        }
        b'[' => {
            if at(p) == b'^' {
                p += 1;
            }
            loop {
                if p >= pat.len() {
                    return Err(E_BRACKET);
                }
                let c = at(p);
                p += 1;
                if c == L_ESC && p < pat.len() {
                    p += 1;
                }
                if at(p) == b']' {
                    break;
                }
            }
            Ok(p + 1)
        }
        _ => Ok(p),
    }
}

/// `CLASS_BM[cl]`: the bytes `c` with `match_class(c, cl)`, as a bitmap (built at compile time).
pub(super) static CLASS_BM: [[u64; 4]; 256] = {
    let mut t = [[0u64; 4]; 256];
    let mut cl = 0;
    while cl < 256 {
        let b = CLBIT[cl];
        let mut c = 0;
        while c < 256 {
            let m = if b == 0 {
                cl == c
            } else {
                (CTYPE[c] & b != 0) == (cl as u8).is_ascii_lowercase()
            };
            if m {
                t[cl][c >> 6] |= 1 << (c & 63);
            }
            c += 1;
        }
        cl += 1;
    }
    t
};

#[inline]
fn class_bitmap(cl: u8) -> [u64; 4] {
    CLASS_BM[cl as usize]
}

fn add_set(sets: &mut Vec<[u64; 4]>, bm: [u64; 4]) -> u16 {
    if let Some(k) = sets.iter().position(|s| *s == bm) {
        return k as u16;
    }
    sets.push(bm);
    (sets.len() - 1) as u16
}

pub(super) fn compile(pat: &[u8]) -> Prog {
    let at = |i: usize| pat.get(i).copied().unwrap_or(0);
    let pend = pat.len();
    let mut items: Vec<Item> = Vec::new();
    let mut sets: Vec<[u64; 4]> = Vec::new();
    let mut lits: Vec<u8> = Vec::new();
    let mut p = 0;
    let mut last_run = false;
    let error = |items: &mut Vec<Item>, code: u8| {
        let mut it = Item::new(I_ERR);
        it.b = code;
        items.push(it);
    };
    while p < pend {
        let mut single = false;
        let mut was_run = false;
        match pat[p] {
            b'(' => {
                if at(p + 1) == b')' {
                    items.push(Item::new(I_OPENPOS));
                    p += 2;
                } else {
                    items.push(Item::new(I_OPEN));
                    p += 1;
                }
            }
            b')' => {
                items.push(Item::new(I_CLOSE));
                p += 1;
            }
            b'$' if p + 1 == pend => {
                items.push(Item::new(I_DOLLAR));
                p += 1;
            }
            L_ESC => match at(p + 1) {
                b'b' => {
                    if p + 3 >= pend {
                        error(&mut items, E_BAL);
                        break;
                    }
                    let mut it = Item::new(I_BAL);
                    it.b = pat[p + 2];
                    it.x = pat[p + 3] as u16;
                    items.push(it);
                    p += 4;
                }
                b'f' => {
                    p += 2;
                    if at(p) != b'[' {
                        error(&mut items, E_FRONT);
                        break;
                    }
                    let ep = match classend(pat, p) {
                        Ok(ep) => ep,
                        Err(code) => {
                            error(&mut items, code);
                            break;
                        }
                    };
                    let mut it = Item::new(I_FRONT);
                    it.x = add_set(&mut sets, set_bitmap(pat, p, ep - 1));
                    items.push(it);
                    p = ep;
                }
                d @ b'0'..=b'9' => {
                    let mut it = Item::new(I_BACKREF);
                    it.b = d;
                    items.push(it);
                    p += 2;
                }
                _ => single = true,
            },
            _ => single = true,
        }
        if single {
            let ep = match classend(pat, p) {
                Ok(ep) => ep,
                Err(code) => {
                    error(&mut items, code);
                    break;
                }
            };
            let q = match at(ep) {
                b'?' => Q_OPT,
                b'*' => Q_STAR,
                b'+' => Q_PLUS,
                b'-' => Q_MIN,
                _ => Q_NONE,
            };
            let mut it = Item::new(I_SINGLE);
            it.q = q;
            match pat[p] {
                b'.' => it.cls = CL_ANY,
                L_ESC => {
                    let d = at(p + 1);
                    if CLBIT[d as usize] == 0 {
                        it.cls = CL_LIT;
                        it.b = d;
                    } else {
                        it.cls = CL_SET;
                        it.x = add_set(&mut sets, class_bitmap(d));
                    }
                }
                b'[' => {
                    it.cls = CL_SET;
                    it.x = add_set(&mut sets, set_bitmap(pat, p, ep - 1));
                }
                c => {
                    it.cls = CL_LIT;
                    it.b = c;
                }
            }
            if q == Q_NONE && it.cls == CL_LIT {
                // literal run (merged with a directly preceding run)
                match items.last_mut() {
                    Some(r) if last_run && r.kind == I_RUN => {
                        lits.push(it.b);
                        r.y += 1;
                    }
                    _ => {
                        let mut r = Item::new(I_RUN);
                        r.x = lits.len() as u16;
                        r.y = 1;
                        lits.push(it.b);
                        items.push(r);
                    }
                }
                was_run = true;
            } else {
                items.push(it);
            }
            p = ep + (q != Q_NONE) as usize;
        }
        last_run = was_run;
    }
    if !matches!(items.last(), Some(i) if i.kind == I_ERR) {
        items.push(Item::new(I_END));
    }
    // The start filter (see `MatchState::start_filter_inner`), looking through up to 8 leading capture openers: a match
    // at `s` needs the first consuming item to match at `s`, and a failing position has no side effects (the depth
    // stays far below the limit and `level` stays far below 32, so no error can be hidden by skipping a position).
    let mut lead = 0;
    while lead < 8
        && matches!(items.get(lead), Some(it) if it.kind == I_OPEN || it.kind == I_OPENPOS)
    {
        lead += 1;
    }
    let start = match items.get(lead) {
        Some(it) if it.kind == I_RUN => Start::Byte(lits[it.x as usize]),
        Some(it) if it.kind == I_SINGLE && (it.q == Q_NONE || it.q == Q_PLUS) => match it.cls {
            CL_LIT => Start::Byte(it.b),
            CL_ANY => Start::Any,
            _ => Start::Bm(it.x as u32),
        },
        _ => Start::Any,
    };
    let plan = MatchState::new(&[], pat).analyze();
    let mut pg = Prog::EMPTY;
    pg.start = start;
    pg.plan = plan;
    let n = pat.len().min(MAX_KEY);
    pg.key[..n].copy_from_slice(&pat[..n]);
    pg.len = pat.len();
    pg.state = S_NOFIT;
    if pat.len() <= MAX_KEY
        && items.len() <= MAX_ITEMS
        && sets.len() <= MAX_SETS
        && lits.len() <= MAX_LITS
    {
        pg.items[..items.len()].copy_from_slice(&items);
        pg.sets[..sets.len()].copy_from_slice(&sets);
        pg.lits[..lits.len()].copy_from_slice(&lits);
        pg.state = S_OK;
        pg.simple = items.iter().all(|it| match it.kind {
            I_END | I_RUN | I_DOLLAR => true,
            I_SINGLE => it.q == Q_NONE,
            _ => false,
        });
    }
    pg
}

// ---------------------------------------------------------------------------------------------
// The cache
// ---------------------------------------------------------------------------------------------
const SLOTS: usize = 256;

/// The slots ARE the programs (`Prog` carries its own address / length / state), so a lookup is a computed address and
/// independent loads: no pointer chain between the slot and the program (the cost of such a chain was measured at ~15
/// cycles per call on short subjects). `lookup` hands out a reference into a slot; a slot is rewritten only by `miss`
/// (first / second sight of a pattern string), which runs inside some other `set_plan`. A `MatchState` that keeps using its
/// program after running foreign code (a `gsub` callback may run any Lua, including pattern calls; `GMatch` lives across
/// calls) holds a [`PinGuard`]: while a pin of this thread is alive `miss` writes nothing (the general matcher runs).
/// Everything else finishes with its program before any foreign code can run (captures are copied out of the
/// `MatchState` without touching the program).
struct Cache {
    slots: [Prog; SLOTS],
    rr: usize,
}

thread_local! {
    // Accessed only by `lookup` / `miss` below; they never re-enter (compiling does not touch the cache).
    static CACHE: UnsafeCell<Cache> = const { UnsafeCell::new(Cache { slots: [Prog::EMPTY; SLOTS], rr: 0 }) };
    static PINS: Cell<u32> = const { Cell::new(0) };
}

/// Marks that a `MatchState` of this thread holds a reference into the cache (see [`Cache`]). Not `Send`.
pub(super) struct PinGuard(bool, PhantomData<*const ()>);

impl PinGuard {
    pub(super) const NONE: PinGuard = PinGuard(false, PhantomData);

    pub(super) fn pin() -> PinGuard {
        PINS.with(|p| p.set(p.get() + 1));
        PinGuard(true, PhantomData)
    }
}

impl Drop for PinGuard {
    #[inline]
    fn drop(&mut self) {
        if self.0 {
            PINS.with(|p| p.set(p.get() - 1));
        }
    }
}

/// The compiled form of `pat` once it was seen before (at the same address and length), else `None` (the caller runs
/// the general matcher). Two slots per set; hits compare the BYTES (a different string at a recycled address never hits).
/// A leaf on the hit path (the miss is a tail call): no frame setup per probe.
#[inline(never)]
#[allow(unsafe_code)]
pub(super) fn lookup(pat: &[u8]) -> Option<&'static Prog> {
    let addr = pat.as_ptr() as usize;
    let len = pat.len();
    if len > MAX_KEY || len == 0 {
        return None;
    }
    let set = ((addr >> 4) ^ len) & (SLOTS - 1) & !1;
    // SAFETY: thread-local; slots are only written by `miss` when no pin is held (see `Cache`).
    let c = CACHE.with(|c| unsafe { &*c.get() });
    for w in 0..2 {
        let sl = &c.slots[set + w];
        if sl.addr == addr && sl.len == len && sl.state >= S_OK && same_bytes(&sl.key[..len], pat) {
            // a program that does not fit is remembered as such (the general matcher runs)
            if sl.state == S_OK {
                // SAFETY: slots are rewritten only by `miss`, never while a pin of this thread is alive, and the
                // callers that outlive foreign code hold a pin (see `Cache`); the reference does not leave the thread.
                return Some(unsafe { &*(sl as *const Prog) });
            }
            return None;
        }
    }
    miss(pat, set)
}

/// Not a hit: first sight (remember the address), second sight (compile), or a recycled address. Writes nothing while
/// a `MatchState` holds a pin; the compiled form is used from the next call. Always `None` (this call runs the general
/// matcher).
#[cold]
#[inline(never)]
#[allow(unsafe_code)]
fn miss(pat: &[u8], set: usize) -> Option<&'static Prog> {
    if PINS.with(|p| p.get()) != 0 {
        return None;
    }
    let addr = pat.as_ptr() as usize;
    let len = pat.len();
    // SAFETY: no pin is held: nothing references the slots.
    let c = CACHE.with(|c| unsafe { &mut *c.get() });
    for w in 0..2 {
        let sl = &mut c.slots[set + w];
        if sl.addr == addr && sl.len == len && sl.state != S_EMPTY {
            // seen before (`S_SEEN`), or a different string was compiled at this address
            let mut pg = compile(pat);
            pg.addr = addr;
            *sl = pg;
            return None;
        }
    }
    // first sight: prefer an empty or `Seen` slot; never evict both compiled forms of a set
    let w = (0..2)
        .find(|&w| c.slots[set + w].state <= S_SEEN)
        .unwrap_or_else(|| {
            c.rr = c.rr.wrapping_add(1);
            c.rr & 1
        });
    let sl = &mut c.slots[set + w];
    sl.addr = addr;
    sl.len = len;
    sl.state = S_SEEN;
    None
}

// ---------------------------------------------------------------------------------------------
// The compiled matcher
// ---------------------------------------------------------------------------------------------
impl<'a> MatchState<'a> {
    /// Does the single-character item `it` match the subject byte at `s`?
    #[inline(always)]
    fn csm(&self, pg: &Prog, it: &Item, s: usize) -> bool {
        let Some(&c) = self.src.get(s) else {
            return false;
        };
        match it.cls {
            CL_ANY => true,
            CL_LIT => it.b == c,
            _ => bm_has(pg.set(it.x as usize), c),
        }
    }

    /// Number of consecutive subject bytes from `s` matching the item.
    #[inline(always)]
    fn crun(&self, pg: &Prog, it: &Item, s: usize) -> usize {
        let rest = &self.src[s.min(self.src.len())..];
        match it.cls {
            CL_ANY => rest.len(),
            CL_LIT => {
                let b = it.b;
                rest.iter().take_while(|&&c| c == b).count()
            }
            _ => {
                let bm = pg.set(it.x as usize);
                rest.iter().take_while(|&&c| bm_has(bm, c)).count()
            }
        }
    }

    /// May the item `nx` match at `pos`? (`true` = unknown: call the matcher.) Never `false` for an item whose
    /// matching at `pos` would do anything but report "no match".
    #[inline(always)]
    fn ccan_start(&self, pg: &Prog, nx: &Item, pos: usize) -> bool {
        match nx.kind {
            I_RUN => self.src.get(pos) == Some(&pg.lit(nx.x as usize)),
            I_SINGLE if nx.q == Q_NONE || nx.q == Q_PLUS => self.csm(pg, nx, pos),
            _ => true,
        }
    }

    /// A straight-line program ([`Prog::simple`]) at `s`: the end position or [`NOM`].
    #[inline(never)]
    pub(super) fn csimple(&self, pg: &Prog, mut s: usize) -> usize {
        let mut i = 0;
        loop {
            let it = pg.item(i);
            match it.kind {
                I_RUN => {
                    let n = it.y as usize;
                    let lit = &pg.lits[it.x as usize..it.x as usize + n];
                    match self.src.get(s..s + n) {
                        Some(w) if same_bytes(w, lit) => s += n,
                        _ => return NOM,
                    }
                }
                I_SINGLE => {
                    if !self.csm(pg, &it, s) {
                        return NOM;
                    }
                    s += 1;
                }
                I_DOLLAR => return if s == self.src.len() { s } else { NOM },
                _ => return s,
            }
            i += 1;
        }
    }

    /// `match` on a compiled pattern from item `i` at subject position `s`.
    ///
    /// `md` is the remaining recursion depth (`matchdepth` of the general matcher, which keeps it in the state and
    /// updates it at every call; here it is an argument, so no store-forwarding chain through memory).
    #[inline(never)]
    pub(super) fn cm(&mut self, pg: &Prog, s: usize, i: usize, md: i32) -> usize {
        if md == 0 {
            return self.fail(b"pattern too complex".to_vec());
        }
        self.cm_inner(pg, s, i, md - 1)
    }

    /// The body of `cm`; `md` is already decremented (the depth the calls made from here are checked against).
    #[inline(always)]
    fn cm_inner(&mut self, pg: &Prog, mut s: usize, mut i: usize, md: i32) -> usize {
        loop {
            let it = pg.item(i);
            match it.kind {
                I_END => return s,
                I_RUN => {
                    let n = it.y as usize;
                    let lit = &pg.lits[it.x as usize..it.x as usize + n];
                    match self.src.get(s..s + n) {
                        Some(w) if same_bytes(w, lit) => {
                            s += n;
                            i += 1;
                        }
                        _ => return NOM,
                    }
                }
                I_SINGLE => match it.q {
                    Q_NONE => {
                        if !self.csm(pg, &it, s) {
                            return NOM;
                        }
                        s += 1;
                        i += 1;
                    }
                    Q_OPT => {
                        if self.csm(pg, &it, s) {
                            let r = self.cm(pg, s + 1, i + 1, md);
                            if r != NOM {
                                return r;
                            }
                        }
                        i += 1;
                    }
                    Q_STAR => {
                        let n = self.crun(pg, &it, s);
                        if n == 0 {
                            i += 1;
                            continue;
                        }
                        return self.cmax(pg, s, n, i + 1, md);
                    }
                    Q_PLUS => {
                        let n = self.crun(pg, &it, s);
                        if n == 0 {
                            return NOM;
                        }
                        return self.cmax(pg, s + 1, n - 1, i + 1, md);
                    }
                    _ => {
                        if !self.csm(pg, &it, s) {
                            i += 1;
                            continue;
                        }
                        return self.cmin(pg, s, i, md);
                    }
                },
                I_OPEN => return self.cstart_capture(pg, s, i + 1, CAP_UNFINISHED, md),
                I_OPENPOS => return self.cstart_capture(pg, s, i + 1, CAP_POSITION, md),
                I_CLOSE => return self.cend_capture(pg, s, i + 1, md),
                I_DOLLAR => return if s == self.src.len() { s } else { NOM },
                I_BAL => match self.cbalance(s, it.b, it.x as u8) {
                    NOM => return NOM,
                    ns => {
                        s = ns;
                        i += 1;
                    }
                },
                I_FRONT => {
                    let bm = pg.set(it.x as usize);
                    let previous = if s == 0 { 0 } else { self.src[s - 1] };
                    if !bm_has(bm, previous) && bm_has(bm, self.s(s)) {
                        i += 1;
                    } else {
                        return NOM;
                    }
                }
                I_BACKREF => match self.match_capture(s, it.b) {
                    NOM => return NOM,
                    ERR => return ERR,
                    ns => {
                        s = ns;
                        i += 1;
                    }
                },
                _ => return self.fail(err_msg(it.b).to_vec()),
            }
        }
    }

    /// `max_expand` for a run of `n` matched items starting at `base`: try the rest of the pattern (item `next`) at
    /// `base + n`, `base + n - 1`, ... `base`.
    fn cmax(&mut self, pg: &Prog, base: usize, n: usize, next: usize, md: i32) -> usize {
        let nx = pg.item(next);
        if nx.kind == I_END {
            // `m(.., End)` would return the position unless the depth limit is reached
            if md == 0 {
                return self.fail(b"pattern too complex".to_vec());
            }
            return base + n;
        }
        let mut k = n;
        loop {
            if md == 0 || self.ccan_start(pg, &nx, base + k) {
                let r = self.cm(pg, base + k, next, md);
                if r != NOM {
                    return r; // a match or an error
                }
            }
            if k == 0 {
                return NOM;
            }
            k -= 1;
        }
    }

    fn cmin(&mut self, pg: &Prog, mut s: usize, i: usize, md: i32) -> usize {
        let it = pg.item(i);
        let nx = pg.item(i + 1);
        loop {
            if md == 0 || self.ccan_start(pg, &nx, s) {
                let r = self.cm(pg, s, i + 1, md);
                if r != NOM {
                    return r;
                }
            }
            if self.csm(pg, &it, s) {
                s += 1;
            } else {
                return NOM;
            }
        }
    }

    fn cstart_capture(&mut self, pg: &Prog, s: usize, next: usize, what: isize, md: i32) -> usize {
        let level = self.level;
        if level >= MAXCAPTURES {
            return self.fail(b"too many captures".to_vec());
        }
        self.capture[level] = MaybeUninit::new(Capture { init: s, len: what });
        self.level = level + 1;
        let res = self.cm(pg, s, next, md);
        if res == NOM {
            self.level -= 1;
        }
        res
    }

    fn cend_capture(&mut self, pg: &Prog, s: usize, next: usize, md: i32) -> usize {
        let l = self.capture_to_close();
        if l == ERR {
            return ERR;
        }
        let c = self.cap_mut(l);
        c.len = (s - c.init) as isize;
        let res = self.cm(pg, s, next, md);
        if res == NOM {
            self.cap_mut(l).len = CAP_UNFINISHED;
        }
        res
    }

    /// `matchbalance` with the two bytes known: the position after the balanced run, or [`NOM`].
    fn cbalance(&self, mut s: usize, b: u8, e: u8) -> usize {
        if s >= self.src.len() || self.src[s] != b {
            return NOM;
        }
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
}
