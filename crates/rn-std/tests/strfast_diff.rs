//! BUNDLE-B: the pattern fast paths against the general matcher (same results, same errors), in process.
use rn_std::pattern::*;
use rn_std::strfast;
use std::sync::Mutex;

static LOCK: Mutex<()> = Mutex::new(());

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
    fn pick<'a>(&mut self, v: &[&'a str]) -> &'a str {
        v[self.below(v.len())]
    }
}

const ITEMS: &[&str] = &[
    "a",
    "b",
    ".",
    "%a",
    "%A",
    "%d",
    "%D",
    "%s",
    "%S",
    "%w",
    "%W",
    "%x",
    "%l",
    "%u",
    "%p",
    "%c",
    "%g",
    "%z",
    "%$",
    "%.",
    "%%",
    "%(",
    "%-",
    "%Z",
    "%q",
    "[abc]",
    "[^abc]",
    "[a-c]",
    "[^a-c]",
    "[%a_]",
    "[^%s]",
    "[%d%.]",
    "[^\n]",
    "[]]",
    "[^]]",
    "[a-]",
    "[%]",
    "[a",
    "[",
    "%",
    "%b()",
    "%f[%w]",
    "%1",
    "$",
    "^",
    "*",
    "+",
    "-",
    "?",
    "(",
    ")",
    "()",
    "[a-%a]",
    "[%a-z]",
    "[z-a]",
    "[\u{ff}-\u{fe}]",
    "\u{ff}",
    "\0",
    "[%z]",
    "[^%z]",
];
const QUANT: &[&str] = &["", "", "+", "*", "-", "?"];
const SUBJ: &[u8] = b"abcabc  12 3x()(a(b)c))_-.\n\t\0AZ\xff[]^$%";

fn gen_pattern(r: &mut Rng) -> Vec<u8> {
    let mut p = String::new();
    if r.below(4) == 0 {
        p.push('^');
    }
    let cap = r.below(3) == 0;
    if cap {
        p.push('(');
    }
    let n = if r.below(3) == 0 { 1 + r.below(3) } else { 1 };
    for _ in 0..n {
        p.push_str(r.pick(ITEMS));
        p.push_str(r.pick(QUANT));
    }
    if cap {
        p.push(')');
    }
    if r.below(8) == 0 {
        p.push('$');
    }
    if r.below(10) == 0 {
        p.push_str(r.pick(ITEMS));
    }
    // chars >= 0x80 written as \u{ff} are two bytes in UTF-8; map them back to single bytes
    let mut out = Vec::new();
    for ch in p.chars() {
        if (ch as u32) < 256 {
            out.push(ch as u8);
        } else {
            unreachable!()
        }
    }
    out
}

fn gen_subject(r: &mut Rng, long: bool) -> Vec<u8> {
    let n = if long { 40 + r.below(80) } else { r.below(14) };
    let mut v: Vec<u8> = (0..n).map(|_| SUBJ[r.below(SUBJ.len())]).collect();
    if long && r.below(2) == 0 {
        // long runs of one class (exercise the set bitmap)
        let c = SUBJ[r.below(SUBJ.len())];
        let at = r.below(v.len());
        for x in v.iter_mut().skip(at).take(50) {
            *x = c;
        }
    }
    v
}

fn caps_repr(c: &[Cap<'_>]) -> String {
    format!("{:?}", c)
}

/// Everything the library derives from (subject, pattern, init): find, match, gmatch, gsub.
fn observe(s: &[u8], p: &[u8], init: i64) -> Vec<String> {
    let mut out = Vec::new();
    for find in [true, false] {
        match find_aux(s, p, init, find, false) {
            Ok(FindResult::NotFound) => out.push("nf".into()),
            Ok(FindResult::Found { start, end, caps }) => {
                out.push(format!("{start} {end} {}", caps_repr(&caps)))
            }
            Err(e) => out.push(format!("err {}", String::from_utf8_lossy(&e))),
        }
    }
    let mut g = GMatch::new(s, p, init);
    for _ in 0..200 {
        match g.next_match() {
            Ok(Some(c)) => out.push(caps_repr(&c)),
            Ok(None) => {
                out.push("end".into());
                break;
            }
            Err(e) => {
                out.push(format!("gerr {}", String::from_utf8_lossy(&e)));
                break;
            }
        }
    }
    for (repl, max) in [(&b"<%0>"[..], 1000i64), (&b"%1"[..], 3), (&b"x"[..], 1000)] {
        match gsub(s, p, Replacement::Str(repl), max) {
            Ok((o, n)) => out.push(format!("{:?} {n}", o)),
            Err(e) => out.push(format!("serr {}", String::from_utf8_lossy(&e))),
        }
    }
    let mut cb = |m: &Matched<'_, '_>| -> PResult<ReplValue> {
        let all = m.all()?;
        Ok(ReplValue::Bytes(
            format!("[{}:{}]", all.len(), caps_repr(&all)).into_bytes(),
        ))
    };
    match gsub(s, p, Replacement::Callback(&mut cb), 1000) {
        Ok((o, n)) => out.push(format!("{:?} {n}", o)),
        Err(e) => out.push(format!("cerr {}", String::from_utf8_lossy(&e))),
    }
    out
}

#[test]
fn fast_paths_match_the_general_matcher() {
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let saved = strfast::mask();
    let mut r = Rng(0x9E37_79B9_7F4A_7C15);
    let mut checked = 0;
    let iters = if cfg!(miri) { 400 } else { 60_000 };
    for i in 0..iters {
        let p = gen_pattern(&mut r);
        let s = gen_subject(&mut r, i % 3 == 0);
        let init = match r.below(6) {
            0 => 0,
            1 => -(r.below(s.len() + 3) as i64),
            2 => s.len() as i64 + r.below(3) as i64,
            _ => 1,
        };
        strfast::set_mask(0);
        let slow = observe(&s, &p, init);
        strfast::set_mask(strfast::PAT1);
        let fast = observe(&s, &p, init);
        assert_eq!(
            slow,
            fast,
            "pattern {:?} subject {:?} init {init}",
            String::from_utf8_lossy(&p),
            String::from_utf8_lossy(&s)
        );
        checked += 1;
    }
    strfast::set_mask(saved);
    assert!(checked > 0);
}

#[test]
fn one_item_examples() {
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let saved = strfast::mask();
    strfast::set_mask(strfast::PAT1);
    let s = b"hello, world 42\nsecond line $x";
    let cases: &[&[u8]] = &[
        b"%a+",
        b"[^\n]+",
        b"%S+",
        b".",
        b"%$",
        b"[WDQMXYFG]",
        b"(%a+)",
        b"(%d*)",
        b"x?",
        b"%s-",
        b"^%a+",
    ];
    for p in cases {
        strfast::set_mask(0);
        let slow = observe(s, p, 1);
        strfast::set_mask(strfast::PAT1);
        assert_eq!(slow, observe(s, p, 1), "{}", String::from_utf8_lossy(p));
    }
    strfast::set_mask(saved);
}

/// Longer patterns: several items, nested captures, `%b`, `%f`, back references, anchors, malformed tails.
fn gen_pattern_long(r: &mut Rng) -> Vec<u8> {
    let mut p = String::new();
    if r.below(5) == 0 {
        p.push('^');
    }
    let n = 2 + r.below(4);
    let mut open = 0;
    for _ in 0..n {
        if r.below(5) == 0 && open < 3 {
            p.push('(');
            open += 1;
        }
        p.push_str(r.pick(ITEMS));
        p.push_str(r.pick(QUANT));
        if open > 0 && r.below(3) == 0 {
            p.push(')');
            open -= 1;
        }
    }
    for _ in 0..open {
        if r.below(10) != 0 {
            p.push(')');
        }
    }
    if r.below(6) == 0 {
        p.push('$');
    }
    p.chars().map(|c| c as u32 as u8).collect()
}

/// find / match / the first `gmatch` step / `gsub` with one replacement (no full traversals: a failing deep pattern at
/// every later start position would backtrack exponentially, in the reference too).
fn observe_light(s: &[u8], p: &[u8]) -> Vec<String> {
    let mut out = Vec::new();
    for find in [true, false] {
        match find_aux(s, p, 1, find, false) {
            Ok(FindResult::NotFound) => out.push("nf".into()),
            Ok(FindResult::Found { start, end, caps }) => {
                out.push(format!("{start} {end} {}", caps_repr(&caps)))
            }
            Err(e) => out.push(format!("err {}", String::from_utf8_lossy(&e))),
        }
    }
    let mut g = GMatch::new(s, p, 1);
    match g.next_match() {
        Ok(c) => out.push(format!("{:?}", c.map(|c| caps_repr(&c)))),
        Err(e) => out.push(format!("gerr {}", String::from_utf8_lossy(&e))),
    }
    match gsub(s, p, Replacement::Str(b"<%0>"), 1) {
        Ok((o, n)) => out.push(format!("{:?} {n}", o)),
        Err(e) => out.push(format!("serr {}", String::from_utf8_lossy(&e))),
    }
    out
}

const PATC_MASKS: [u32; 3] = [strfast::PATC, strfast::PATC | strfast::PAT1, strfast::PAT1];

/// The compiled-pattern cache against the general matcher: first sight, compile and cached rounds, every mask.
#[test]
fn patc_matches_the_general_matcher() {
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let saved = strfast::mask();
    let mut r = Rng(0xC0FF_EE00_1234_5678);
    let iters = if cfg!(miri) { 200 } else { 40_000 };
    for i in 0..iters {
        let p = if i % 2 == 0 {
            gen_pattern(&mut r)
        } else {
            gen_pattern_long(&mut r)
        };
        let mut s = gen_subject(&mut r, i % 3 == 0);
        s.truncate(48);
        let init = match r.below(6) {
            0 => 0,
            1 => -(r.below(s.len() + 3) as i64),
            2 => s.len() as i64 + r.below(3) as i64,
            _ => 1,
        };
        strfast::set_mask(0);
        let slow = observe(&s, &p, init);
        for m in PATC_MASKS {
            strfast::set_mask(m);
            for round in 0..3 {
                // a fresh copy per round: another address (first sight) or the same one after a recycle
                let pc = p.clone();
                let fast = observe(&s, &pc, init);
                assert_eq!(
                    slow,
                    fast,
                    "mask {m} round {round} pattern {:?} subject {:?} init {init}",
                    String::from_utf8_lossy(&p),
                    String::from_utf8_lossy(&s)
                );
            }
        }
    }
    strfast::set_mask(saved);
}

/// Malformed patterns report their error lazily: only when matching reaches the bad item (so a subject that fails
/// earlier gives "no match", exactly as the general matcher does).
#[test]
fn patc_malformed_errors_are_lazy_and_positional() {
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let saved = strfast::mask();
    let bads: &[&str] = &[
        "%", "[a", "[", "[^", "%b", "%bx", "%f", "%fa", "%f[a", "%2", "%0", "(", "((a)", "a)",
        "(a))",
    ];
    let subjects: &[&[u8]] = &[b"", b"a", b"xyz", b"abab", b"aaa)", b"((a)"];
    for bad in bads {
        for pre in ["", "a", "a*", "x?", "(a)", "%a+", "[ab]*", "a-", "^"] {
            for post in ["", "b", "a*"] {
                let p = format!("{pre}{bad}{post}").into_bytes();
                for s in subjects {
                    strfast::set_mask(0);
                    let slow = observe(s, &p, 1);
                    for m in PATC_MASKS {
                        strfast::set_mask(m);
                        for round in 0..3 {
                            assert_eq!(
                                slow,
                                observe(s, &p, 1),
                                "mask {m} round {round} {:?} on {:?}",
                                String::from_utf8_lossy(&p),
                                String::from_utf8_lossy(s)
                            );
                        }
                    }
                }
            }
        }
    }
    strfast::set_mask(saved);
}

/// `pattern too complex` (matchdepth 200) and the 32 capture limit at and around their boundaries.
#[test]
fn patc_depth_and_capture_limits() {
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let saved = strfast::mask();
    let subject = vec![b'a'; 300];
    // (Miri: the boundary values only)
    let ns: Vec<usize> = if cfg!(miri) {
        vec![199, 200, 201, 250]
    } else {
        (185..215).chain([250, 400]).collect()
    };
    let units: &[&str] = if cfg!(miri) {
        &["a?", "(a)"]
    } else {
        &[
            "a?", "a-", "(a)", "(a*)", ".-b?", "a*b?", "%a?", "[ab]?", "()a",
        ]
    };
    for n in ns {
        for &unit in units {
            // below the limit a failing tail would backtrack exponentially (in the reference too)
            for tail in if n >= 205 {
                vec!["", "$", "c"]
            } else {
                vec![""]
            } {
                let p = format!("{}{}", unit.repeat(n), tail).into_bytes();
                strfast::set_mask(0);
                let slow = observe_light(&subject, &p);
                for m in PATC_MASKS {
                    strfast::set_mask(m);
                    for round in 0..3 {
                        assert_eq!(
                            slow,
                            observe_light(&subject, &p),
                            "mask {m} round {round} {unit} x {n} {tail}"
                        );
                    }
                }
            }
        }
    }
    for n in 28..36 {
        let p = "(a)".repeat(n).into_bytes();
        let q = "()".repeat(n).into_bytes();
        for pat in [p, q] {
            strfast::set_mask(0);
            let slow = observe_light(&subject, &pat);
            for m in PATC_MASKS {
                strfast::set_mask(m);
                for _ in 0..3 {
                    assert_eq!(slow, observe_light(&subject, &pat), "mask {m} captures {n}");
                }
            }
        }
    }
    // the backtracking loop of `*` / `+` / `-` reaching the depth limit: the call of the rest of the pattern must still
    // report "pattern too complex" although the next item cannot match at any position (k = 198 would backtrack
    // exponentially, in the reference too; from 199 on the first descent already fails)
    for k in [199usize, 200, 201, 230] {
        for item in ["a*b", "a+b", "a-b", "a*bc", "[ab]*c", "%a+x", ".-x", "a*$"] {
            let p = format!("{}{}", "a?".repeat(k), item).into_bytes();
            strfast::set_mask(0);
            let slow = observe_light(&subject, &p);
            for m in PATC_MASKS {
                strfast::set_mask(m);
                for _ in 0..3 {
                    assert_eq!(
                        slow,
                        observe_light(&subject, &p),
                        "mask {m} a? x {k} then {item}"
                    );
                }
            }
        }
    }
    // leading capture openers in front of the first consuming item: a position the start filter skips must not hide the
    // "too many captures" error the general matcher raises there (more than 32 openers), nor change anything below that
    for n in [1usize, 2, 8, 9, 31, 32, 33, 34, 40] {
        for subj in [&b"baaa"[..], b"aaab", b"", b"b"] {
            for tail in ["a", "a+", "%a", "[a]*", "b-", ""] {
                let p = format!("{}{}{}", "(".repeat(n), tail, ")".repeat(n)).into_bytes();
                let q = format!("{}{}", "()".repeat(n), tail).into_bytes();
                for pat in [p, q] {
                    strfast::set_mask(0);
                    let slow = observe_light(subj, &pat);
                    for m in PATC_MASKS {
                        strfast::set_mask(m);
                        for _ in 0..3 {
                            assert_eq!(
                                slow,
                                observe_light(subj, &pat),
                                "mask {m} openers {n} tail {tail:?}"
                            );
                        }
                    }
                }
            }
        }
    }
    strfast::set_mask(saved);
}

/// A different pattern at the address of an earlier cached one (what GC address reuse does) must not hit.
#[test]
fn patc_is_keyed_by_bytes() {
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let saved = strfast::mask();
    strfast::set_mask(strfast::PATC);
    let s = b"hello world 123 foo_bar";
    let mut buf = *b"(%a+) (%a+)";
    for _ in 0..4 {
        let _ = observe(s, &buf, 1);
    }
    for next in [*b"(%d+) (%w+)", *b"[%a]+  [%d]", *b"o w(o)r(l)d"] {
        buf.copy_from_slice(&next);
        let got = observe(s, &buf, 1);
        strfast::set_mask(0);
        let want = observe(s, &buf, 1);
        assert_eq!(want, got, "{}", String::from_utf8_lossy(&buf));
        strfast::set_mask(strfast::PATC);
        for _ in 0..3 {
            assert_eq!(want, observe(s, &buf, 1));
        }
    }
    strfast::set_mask(saved);
}

// ---------------------------------------------------------------------------------------------
// string.format: the spec cache against the uncached engine
// ---------------------------------------------------------------------------------------------
use rn_std::format::{format_into, FormatError, FormatSource, Literal};

#[derive(Clone, Debug)]
enum A {
    I(i64),
    F(f64),
    S(Vec<u8>),
    Nil,
}

struct Src<'a> {
    args: &'a [A],
    log: Vec<String>,
}

impl FormatSource for Src<'_> {
    type Error = String;
    fn top(&self) -> usize {
        self.args.len() + 1
    }
    fn check_integer(&mut self, arg: usize) -> Result<i64, String> {
        self.log.push(format!("ci{arg}"));
        match &self.args[arg - 2] {
            A::I(i) => Ok(*i),
            A::F(f) if f.fract() == 0.0 && f.abs() < 1e18 => Ok(*f as i64),
            A::F(_) => Err(format!(
                "bad argument #{arg} (number has no integer representation)"
            )),
            A::S(s) => match std::str::from_utf8(s)
                .ok()
                .and_then(|x| x.trim().parse::<i64>().ok())
            {
                Some(i) => Ok(i),
                None => Err(format!("bad argument #{arg} (number expected, got string)")),
            },
            A::Nil => Err(format!("bad argument #{arg} (number expected, got nil)")),
        }
    }
    fn check_number(&mut self, arg: usize) -> Result<f64, String> {
        self.log.push(format!("cn{arg}"));
        match &self.args[arg - 2] {
            A::I(i) => Ok(*i as f64),
            A::F(f) => Ok(*f),
            A::S(s) => match std::str::from_utf8(s)
                .ok()
                .and_then(|x| x.trim().parse::<f64>().ok())
            {
                Some(i) => Ok(i),
                None => Err(format!("bad argument #{arg} (number expected, got string)")),
            },
            A::Nil => Err(format!("bad argument #{arg} (number expected, got nil)")),
        }
    }
    fn tolstring(&mut self, arg: usize) -> Result<Vec<u8>, String> {
        self.log.push(format!("ts{arg}"));
        Ok(match &self.args[arg - 2] {
            A::I(i) => i.to_string().into_bytes(),
            A::F(f) => format!("{f:?}").into_bytes(),
            A::S(s) => s.clone(),
            A::Nil => b"nil".to_vec(),
        })
    }
    fn literal(&mut self, arg: usize) -> Literal {
        match &self.args[arg - 2] {
            A::I(i) => Literal::Int(*i),
            A::F(f) => Literal::Float(*f),
            A::S(s) => Literal::Str(s.clone()),
            A::Nil => Literal::Plain(b"nil".to_vec()),
        }
    }
    fn topointer(&mut self, arg: usize) -> Option<usize> {
        match &self.args[arg - 2] {
            A::S(_) => Some(0x1000 + arg),
            _ => None,
        }
    }
}

const FSPECS: &[&str] = &[
    "%d",
    "%i",
    "%5d",
    "%-5d",
    "%05d",
    "%+d",
    "% d",
    "%.3d",
    "%5.3d",
    "%x",
    "%X",
    "%08x",
    "%5x",
    "%-8X",
    "%#x",
    "%#o",
    "%o",
    "%u",
    "%5u",
    "%c",
    "%5c",
    "%-5c",
    "%s",
    "%10s",
    "%-10s",
    "%.2s",
    "%10.2s",
    "%.s",
    "%f",
    "%.2f",
    "%10.3f",
    "%-10.1f",
    "%+.0f",
    "%e",
    "%E",
    "%g",
    "%G",
    "%.14g",
    "%a",
    "%A",
    "%q",
    "%p",
    "%%",
    "%5%",
    "%y",
    "%",
    "%5",
    "%ld",
    "%.123d",
    "%123d",
    "%0-5d",
    "%+5s",
    "%#5s",
    "%-+d",
    "%.30f",
    "%--5d",
    "%1234567890123456789012345678901234567890d",
    "%.f",
    "%i0",
    "%5.f",
    "%5.1s",
    "%#.3x",
    "%-#8x",
    "%099d",
    "%.99f",
];
const LITS: &[&str] = &[
    "",
    "x",
    " = ",
    "\n",
    "a%%b",
    "\t",
    "(",
    ")",
    "long literal run here: ",
];

fn gen_args(r: &mut Rng, n: usize) -> Vec<A> {
    (0..n)
        .map(|_| match r.below(9) {
            0 => A::I(r.next() as i64),
            1 => A::I(r.below(300) as i64 - 100),
            2 => A::F(r.below(2000) as f64 / 7.0 - 100.0),
            3 => A::F(r.below(50) as f64),
            4 => A::S(b"hello".to_vec()),
            5 => A::S(b"a\0b".to_vec()),
            6 => A::S(b"12".to_vec()),
            7 => A::Nil,
            _ => A::F(if r.below(2) == 0 {
                f64::INFINITY
            } else {
                f64::NAN
            }),
        })
        .collect()
}

fn run_fmt(fmt: &[u8], args: &[A]) -> (Result<Vec<u8>, FormatError<String>>, Vec<String>) {
    let mut src = Src { args, log: vec![] };
    let mut out = b"prefix:".to_vec();
    let r = format_into(fmt, &mut src, &mut out).map(|_| out);
    (r, src.log)
}

// snprintf (libc) is not runnable under Miri
#[test]
#[cfg_attr(miri, ignore)]
fn format_cache_matches_uncached() {
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let saved = strfast::mask();
    let mut r = Rng(0x1234_5678_9ABC_DEF1);
    for _ in 0..30_000 {
        let nitems = r.below(6);
        let mut fmt = String::new();
        for _ in 0..nitems {
            fmt.push_str(r.pick(LITS));
            fmt.push_str(r.pick(FSPECS));
        }
        fmt.push_str(r.pick(LITS));
        let fmt = fmt.into_bytes();
        let nargs = nitems.saturating_sub(r.below(2)) + r.below(2);
        let args = gen_args(&mut r, nargs);
        strfast::set_mask(0);
        let want = run_fmt(&fmt, &args);
        strfast::set_mask(strfast::FMTC);
        // first call: seen only; second: compiled; third: hit
        for round in 0..3 {
            let got = run_fmt(&fmt, &args);
            assert_eq!(
                want.0,
                got.0,
                "fmt {:?} args {:?} round {round}",
                String::from_utf8_lossy(&fmt),
                args
            );
            assert_eq!(
                want.1,
                got.1,
                "arg access order: fmt {:?} args {:?}",
                String::from_utf8_lossy(&fmt),
                args
            );
        }
    }
    strfast::set_mask(saved);
}

/// A different format string at the address of an earlier cached one (what GC address reuse does) must not hit.
#[test]
#[cfg_attr(miri, ignore)]
fn format_cache_is_keyed_by_bytes() {
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let saved = strfast::mask();
    strfast::set_mask(strfast::FMTC);
    let args = [A::I(255), A::S(b"zz".to_vec()), A::I(255)];
    let mut buf = *b"%d|%s|%5d|";
    let a = run_fmt(&buf, &args);
    let b = run_fmt(&buf, &args);
    let c = run_fmt(&buf, &args);
    assert_eq!(a.0.clone().unwrap(), b"prefix:255|zz|  255|".to_vec());
    assert_eq!(b.0, c.0);
    // same address, same length, different bytes
    buf.copy_from_slice(b"%x|%s|%s|x");
    let d = run_fmt(&buf, &args);
    strfast::set_mask(0);
    let want = run_fmt(&buf, &args);
    assert_eq!(d.0, want.0);
    assert_eq!(d.1, want.1);
    strfast::set_mask(saved);
}
