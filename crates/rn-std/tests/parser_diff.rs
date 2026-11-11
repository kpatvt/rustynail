//! Parser/compiler differential against the reference front end (`tools/parsediff.c`, which loads
//! every case exactly like `luac -p` and prints the `luac -l -l` listing).
//!
//! For each case (corpus files, truncations, token mutations, generated programs, literal/limit edge
//! cases) it compares
//! - accept/reject and the full error message (chunk id, line, `near` token),
//! - for accepted chunks: `string.dump` bytes (stripped and not) and, for a share of the cases, the
//!   full listing (`luac -l -l`),
//!
//! along two of our paths: the compiler crate with a `TestHost` (+ our listing tool) and the runtime
//! (`State::load_chunk` on the real heap, then `string.dump` of the loaded closure).
//!
//! Default budget is small (a few seconds). Large runs:
//! `RUSTYNAIL_PARSE_DIFF_CASES=500000 [RUSTYNAIL_PARSE_DIFF_SEED=n] [RUSTYNAIL_PARSE_DIFF_OUT=dir]
//!  [RUSTYNAIL_PARSE_DIFF_MIN=1] cargo test -p rn-std --release --test parser_diff -- --nocapture`
//! (see `scripts/parser-diff.sh`). `_OUT` saves every mismatching case; `_MIN` minimizes them.
mod common;
use common::*;
use rn_compiler::listing::{list, ListOpts};
use rn_compiler::testhost::TestHost;
use rn_compiler::{dump, load_with_mode_str};
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

const V55: bool = !cfg!(rustynail_lua54);

// ---------------------------------------------------------------------------------------------
// cases and outcomes
// ---------------------------------------------------------------------------------------------

#[derive(Clone)]
struct Case {
    kind: &'static str,
    name: Vec<u8>,
    src: Vec<u8>,
    listing: bool,
    /// load mode "bt" (binary chunks allowed) instead of "t"
    binary: bool,
}

#[derive(Clone, PartialEq, Eq, Debug)]
enum Outcome {
    Err(Vec<u8>),
    Ok {
        full: Vec<u8>,
        strip: Vec<u8>,
        listing: Vec<u8>,
    },
    Panic(String),
    RefCrash(String),
}

// ---------------------------------------------------------------------------------------------
// reference side
// ---------------------------------------------------------------------------------------------

fn driver() -> Option<&'static Path> {
    static D: OnceLock<Option<PathBuf>> = OnceLock::new();
    D.get_or_init(|| {
        let build = root().join(format!("ref/{VER}/build"));
        let lib = build.join("liblua.a");
        if !lib.exists() {
            eprintln!(
                "SKIP: {} missing (run `cargo xtask ref-build`)",
                lib.display()
            );
            return None;
        }
        let out = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
            .join(format!("parsediff-{VER}-{}", std::process::id()));
        let st = Command::new("cc")
            .args(["-O1", "-w"])
            .arg(format!("-I{}", build.display()))
            .arg(root().join("tools/parsediff.c"))
            .arg(&lib)
            .args(["-lm", "-o"])
            .arg(&out)
            .status();
        match st {
            Ok(s) if s.success() => Some(out),
            _ => {
                eprintln!("SKIP: cannot build tools/parsediff.c with cc");
                None
            }
        }
    })
    .as_deref()
}

fn tmpfile(tag: &str) -> PathBuf {
    static N: AtomicUsize = AtomicUsize::new(0);
    PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!(
        "pd-{}-{}-{tag}",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ))
}

fn put32(v: &mut Vec<u8>, n: usize) {
    v.extend_from_slice(&(n as u32).to_le_bytes());
}

fn get_chunk(b: &[u8], pos: &mut usize) -> Option<Vec<u8>> {
    let n = u32::from_le_bytes(b.get(*pos..*pos + 4)?.try_into().ok()?) as usize;
    *pos += 4;
    let v = b.get(*pos..*pos + n)?.to_vec();
    *pos += n;
    Some(v)
}

/// Parse complete records of the driver output.
fn parse_out(b: &[u8], max: usize) -> Vec<Outcome> {
    let mut v = Vec::new();
    let mut pos = 0;
    while v.len() < max && pos < b.len() {
        let save = pos;
        let st = b[pos];
        pos += 1;
        let r = if st == 1 {
            get_chunk(b, &mut pos).map(Outcome::Err)
        } else {
            (|| {
                let full = get_chunk(b, &mut pos)?;
                let strip = get_chunk(b, &mut pos)?;
                let listing = get_chunk(b, &mut pos)?;
                Some(Outcome::Ok {
                    full,
                    strip,
                    listing,
                })
            })()
        };
        match r {
            Some(o) => v.push(o),
            None => {
                let _ = save;
                break;
            }
        }
    }
    v
}

fn run_ref(cases: &[Case]) -> Vec<Outcome> {
    let drv = driver().expect("driver");
    let mut res: Vec<Outcome> = Vec::with_capacity(cases.len());
    while res.len() < cases.len() {
        let rest = &cases[res.len()..];
        let mut input = Vec::new();
        for c in rest {
            put32(&mut input, c.name.len());
            input.extend_from_slice(&c.name);
            put32(&mut input, c.src.len());
            input.extend_from_slice(&c.src);
            input.push(u8::from(c.listing) | (u8::from(c.binary) << 1));
        }
        let inp = tmpfile("in");
        let outp = tmpfile("out");
        std::fs::write(&inp, &input).unwrap();
        let _ = std::fs::remove_file(&outp);
        let mut child = Command::new(drv)
            .arg(&inp)
            .arg(&outp)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let start = Instant::now();
        let mut timed_out = false;
        let status = loop {
            if let Some(s) = child.try_wait().unwrap() {
                break s;
            }
            if start.elapsed() > Duration::from_secs(300) {
                let _ = child.kill();
                timed_out = true;
                break child.wait().unwrap();
            }
            std::thread::sleep(Duration::from_millis(2));
        };
        let mut err = String::new();
        if let Some(mut e) = child.stderr.take() {
            let _ = e.read_to_string(&mut err);
        }
        let out = std::fs::read(&outp).unwrap_or_default();
        let _ = std::fs::remove_file(&inp);
        let _ = std::fs::remove_file(&outp);
        let got = parse_out(&out, rest.len());
        let n = got.len();
        res.extend(got);
        if n < rest.len() {
            assert!(
                !status.success() || timed_out,
                "driver stopped early without error: {err}"
            );
            // the reference crashed (or hung) on this case: record it and go on with the next
            res.push(Outcome::RefCrash(format!(
                "{status:?}{} {}",
                if timed_out { " (timeout)" } else { "" },
                err.trim()
            )));
        }
    }
    res
}

// ---------------------------------------------------------------------------------------------
// our side
// ---------------------------------------------------------------------------------------------

/// Normalize the addresses printed by `%p` in listings (same as `rn-compiler/tests/bytecode_diff.rs`).
fn normalize(s: &str) -> String {
    let mut out = String::new();
    for line in s.lines() {
        let mut l = line.to_string();
        let hex_tail = |l: &str, marker: &str, end: &str| -> Option<String> {
            let i = l.rfind(marker)?;
            let rest = &l[i + marker.len()..];
            let h = rest.strip_suffix(end)?;
            if !h.is_empty() && h.bytes().all(|c| c.is_ascii_hexdigit()) {
                Some(format!(
                    "{}{}0xX{}",
                    &l[..i],
                    marker.trim_end_matches("0x"),
                    end
                ))
            } else {
                None
            }
        };
        if l.starts_with("constants (") || l.starts_with("locals (") || l.starts_with("upvalues (")
        {
            if let Some(x) = hex_tail(&l, " for 0x", ":") {
                l = x;
            }
        } else if let Some(x) = hex_tail(&l, " at 0x", ")") {
            l = x;
        } else if l.contains("\tCLOSURE") {
            if let Some(x) = hex_tail(&l, "\t; 0x", "") {
                l = x;
            }
        }
        out.push_str(&l);
        out.push('\n');
    }
    out
}

fn panic_text(e: Box<dyn std::any::Any + Send>) -> String {
    e.downcast_ref::<String>()
        .cloned()
        .or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string()))
        .unwrap_or_else(|| "panic".into())
}

fn mode(c: &Case) -> &'static [u8] {
    if c.binary {
        b"bt"
    } else {
        b"t"
    }
}

/// The compiler crate on a `TestHost` (+ our `luac -l -l`).
fn ours_compiler(c: &Case) -> Outcome {
    let r = std::panic::catch_unwind(|| {
        let mut host = TestHost::default();
        match load_with_mode_str(&mut host, &c.src, &c.name, Some(mode(c))) {
            Err(e) => Outcome::Err(e.message),
            Ok(p) => Outcome::Ok {
                full: dump(&p, &host, false),
                strip: dump(&p, &host, true),
                listing: if c.listing {
                    list(
                        &p,
                        &host,
                        ListOpts {
                            full: true,
                            normalize_addresses: true,
                        },
                    )
                    .into_bytes()
                } else {
                    Vec::new()
                },
            },
        }
    });
    r.unwrap_or_else(|e| Outcome::Panic(panic_text(e)))
}

/// The runtime: `load` on the real heap, then `string.dump` of the closure (no listing).
fn ours_runtime(st: &mut Option<rn_core::state::LuaState>, c: &Case) -> Outcome {
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let s = st.get_or_insert_with(|| rn_std::new_state(0).unwrap());
        let base = s.top();
        let status = s.load_chunk(&c.src, &c.name, Some(mode(c)));
        let v = s.get(s.top() - 1);
        let o = if status == rn_core::status::Status::Ok {
            let cl = v.as_lclosure().expect("closure");
            Outcome::Ok {
                full: rn_std::lib_string::dump_closure(s, cl, false),
                strip: rn_std::lib_string::dump_closure(s, cl, true),
                listing: Vec::new(),
            }
        } else {
            Outcome::Err(
                v.as_string()
                    .map(|m| m.as_bytes().to_vec())
                    .unwrap_or_else(|| format!("status {status:?} without a message").into()),
            )
        };
        s.set_top(base);
        o
    }));
    r.unwrap_or_else(|e| {
        *st = None; // state of unknown consistency after a panic
        Outcome::Panic(panic_text(e))
    })
}

// ---------------------------------------------------------------------------------------------
// comparison
// ---------------------------------------------------------------------------------------------

fn lossy(b: &[u8]) -> String {
    String::from_utf8_lossy(b).into_owned()
}

/// `None` = identical; else (category, detail).
fn compare(c: &Case, r: &Outcome, oc: &Outcome, or: &Outcome) -> Option<(String, String)> {
    let cat = |k: &str| k.to_string();
    match (r, oc) {
        (Outcome::RefCrash(e), _) => return Some((cat("ref-crash"), e.clone())),
        (_, Outcome::Panic(p)) => return Some((cat("panic(compiler)"), p.clone())),
        (Outcome::Err(a), Outcome::Ok { .. }) => {
            return Some((cat("accepts-invalid"), format!("ref: {}", lossy(a))))
        }
        (Outcome::Ok { .. }, Outcome::Err(b)) => {
            return Some((cat("rejects-valid"), format!("ours: {}", lossy(b))))
        }
        (Outcome::Err(a), Outcome::Err(b)) => {
            if a != b {
                return Some((
                    cat("message"),
                    format!("ref : {}\nours: {}", lossy(a), lossy(b)),
                ));
            }
        }
        (
            Outcome::Ok {
                full: f1,
                strip: s1,
                listing: l1,
            },
            Outcome::Ok {
                full: f2,
                strip: s2,
                listing: l2,
            },
        ) => {
            if c.listing {
                let a = normalize(&lossy(l1));
                let b = normalize(&lossy(l2));
                if a != b {
                    let at = a
                        .lines()
                        .zip(b.lines())
                        .position(|(x, y)| x != y)
                        .unwrap_or(a.lines().count().min(b.lines().count()));
                    return Some((
                        cat("listing"),
                        format!(
                            "line {}\nref : {}\nours: {}",
                            at + 1,
                            a.lines().nth(at).unwrap_or("<end>"),
                            b.lines().nth(at).unwrap_or("<end>")
                        ),
                    ));
                }
            }
            for (k, x, y) in [("dump", f1, f2), ("dump-strip", s1, s2)] {
                if x != y {
                    let at = x.iter().zip(y.iter()).position(|(a, b)| a != b);
                    return Some((
                        cat(k),
                        format!("lens {}/{} first diff at {at:?}", x.len(), y.len()),
                    ));
                }
            }
        }
        _ => unreachable!(),
    }
    // runtime path vs compiler path
    match (oc, or) {
        (_, Outcome::Panic(p)) => Some((cat("panic(runtime)"), p.clone())),
        (Outcome::Err(a), Outcome::Err(b)) if a != b => Some((
            cat("runtime-message"),
            format!("compiler: {}\nruntime : {}", lossy(a), lossy(b)),
        )),
        (Outcome::Err(_), Outcome::Err(_)) => None,
        (
            Outcome::Ok {
                full: f1,
                strip: s1,
                ..
            },
            Outcome::Ok {
                full: f2,
                strip: s2,
                ..
            },
        ) => {
            if f1 != f2 || s1 != s2 {
                let at = f1.iter().zip(f2.iter()).position(|(a, b)| a != b);
                Some((
                    cat("runtime-dump"),
                    format!("lens {}/{} first diff at {at:?}", f1.len(), f2.len()),
                ))
            } else {
                None
            }
        }
        _ => Some((
            cat("runtime-accept"),
            format!("compiler {oc:?}\nruntime {or:?}"),
        )),
    }
}

struct Stats {
    cases: usize,
    accepted: usize,
    listings: usize,
    listing_bytes: usize,
    dump_bytes: usize,
    by_kind: BTreeMap<&'static str, (usize, usize)>,
    mism: BTreeMap<String, Vec<(Case, String)>>,
}

/// Run `cases` on `workers` threads (each: one reference driver process + our two paths).
/// Returns up to `keep_dumps` reference dumps of accepted text chunks (seeds for binary cases).
fn run_cases(
    cases: Vec<Case>,
    workers: usize,
    stats: &mut Stats,
    keep_dumps: usize,
) -> Vec<Vec<u8>> {
    let mut dumps = Vec::new();
    let chunk = cases.len().div_ceil(workers.max(1)).max(1);
    let results: Vec<Vec<(Case, Outcome, Outcome, Outcome)>> = std::thread::scope(|s| {
        let hs: Vec<_> = cases
            .chunks(chunk)
            .map(|part| {
                std::thread::Builder::new()
                    .stack_size(16 << 20)
                    .spawn_scoped(s, move || {
                        let r = run_ref(part);
                        let mut st = None;
                        part.iter()
                            .zip(r)
                            .map(|(c, r)| {
                                let oc = ours_compiler(c);
                                let or = ours_runtime(&mut st, c);
                                (c.clone(), r, oc, or)
                            })
                            .collect::<Vec<_>>()
                    })
                    .unwrap()
            })
            .collect();
        hs.into_iter().map(|h| h.join().unwrap()).collect()
    });
    for (c, r, oc, or) in results.into_iter().flatten() {
        stats.cases += 1;
        let e = stats.by_kind.entry(c.kind).or_default();
        e.0 += 1;
        if let Outcome::Ok {
            full,
            strip,
            listing,
        } = &r
        {
            stats.accepted += 1;
            e.1 += 1;
            if !c.binary && dumps.len() < keep_dumps && full.len() < 64 * 1024 {
                dumps.push(if dumps.len() % 2 == 0 {
                    full.clone()
                } else {
                    strip.clone()
                });
            }
            stats.dump_bytes += full.len() + strip.len();
            if c.listing {
                stats.listings += 1;
                stats.listing_bytes += listing.len();
            }
        }
        if let Some((cat, detail)) = compare(&c, &r, &oc, &or) {
            stats.mism.entry(cat).or_default().push((c, detail));
        }
    }
    dumps
}

/// Binary chunks: each reference dump whole, truncated at `per` random points, and with `per`
/// random header bytes replaced (the reference trusts the body, so only the header is mutated).
fn binary_cases(rng: &mut Rng, dumps: &[Vec<u8>], per: usize) -> Vec<Case> {
    let header = if V55 { 40 } else { 31 };
    let mut v = Vec::new();
    for d in dumps {
        let mk = |kind: &'static str, src: Vec<u8>, name: Vec<u8>| Case {
            kind,
            name,
            src,
            listing: true,
            binary: true,
        };
        v.push(mk("binary", d.clone(), b"=stdin".to_vec()));
        for _ in 0..per {
            let name = if rng.chance(1, 4) {
                random_name(rng)
            } else {
                b"=stdin".to_vec()
            };
            let cut = rng.below(d.len() as u64) as usize;
            v.push(mk("binary-truncate", d[..cut].to_vec(), name.clone()));
            let mut m = d.clone();
            let i = rng.below(header.min(m.len()) as u64) as usize;
            m[i] = if rng.chance(1, 2) {
                rng.next() as u8
            } else {
                m[i] ^ (1 << rng.below(8))
            };
            v.push(mk("binary-header", m, name));
        }
    }
    v
}

// ---------------------------------------------------------------------------------------------
// minimization of a mismatching case (opt-in)
// ---------------------------------------------------------------------------------------------

fn category_of(c: &Case) -> Option<String> {
    let r = run_ref(std::slice::from_ref(c)).pop().unwrap();
    let oc = ours_compiler(c);
    let mut st = None;
    let or = ours_runtime(&mut st, c);
    compare(c, &r, &oc, &or).map(|x| x.0)
}

fn minimize(c: &Case, cat: &str) -> Case {
    let mut best = c.clone();
    let mut n = best.src.len() / 2;
    while n >= 1 {
        let mut progress = false;
        let mut i = 0;
        while i + n <= best.src.len() {
            // a batch of candidates: remove [j, j+n) for the next 64 positions
            let cands: Vec<Case> = (0..64)
                .map(|k| i + k * n)
                .filter(|&j| j + n <= best.src.len())
                .map(|j| {
                    let mut s = best.src.clone();
                    s.drain(j..j + n);
                    Case {
                        src: s,
                        ..best.clone()
                    }
                })
                .collect();
            if cands.is_empty() {
                break;
            }
            let refs = run_ref(&cands);
            let mut found = None;
            for (k, (cand, r)) in cands.iter().zip(refs).enumerate() {
                let oc = ours_compiler(cand);
                let mut st = None;
                let or = ours_runtime(&mut st, cand);
                if compare(cand, &r, &oc, &or).is_some_and(|x| x.0 == cat) {
                    found = Some(k);
                    break;
                }
            }
            match found {
                Some(k) => {
                    best = cands[k].clone();
                    i += k * n;
                    progress = true;
                }
                None => i += 64 * n,
            }
        }
        if !progress {
            n /= 2;
        }
    }
    let _ = category_of;
    best
}

// ---------------------------------------------------------------------------------------------
// generators
// ---------------------------------------------------------------------------------------------

fn stdin_case(kind: &'static str, src: impl Into<Vec<u8>>) -> Case {
    Case {
        kind,
        name: b"=stdin".to_vec(),
        src: src.into(),
        listing: true,
        binary: false,
    }
}

/// `.lua` files of the corpus for the active version.
fn corpus_files() -> Vec<PathBuf> {
    fn walk(dir: &Path, v: &mut Vec<PathBuf>) {
        let Ok(rd) = std::fs::read_dir(dir) else {
            return;
        };
        let mut es: Vec<_> = rd.flatten().map(|e| e.path()).collect();
        es.sort();
        for p in es {
            let name = p.file_name().unwrap().to_string_lossy().into_owned();
            let skip: &[&str] = if V55 { &["gen54"] } else { &["gen", "only55"] };
            if p.is_dir() {
                if skip.contains(&name.as_str()) && dir.ends_with("tests/lua") {
                    continue;
                }
                walk(&p, v);
            } else if p.extension().is_some_and(|x| x == "lua") {
                v.push(p);
            }
        }
    }
    let mut v = Vec::new();
    for d in [
        if V55 {
            "vendor/lua-tests-5.5"
        } else {
            "vendor/lua-tests-5.4"
        },
        "tests/vm",
        "tests/lua",
        "bench",
    ] {
        walk(&root().join(d), &mut v);
    }
    v
}

fn file_cases() -> Vec<Case> {
    corpus_files()
        .into_iter()
        .filter_map(|p| {
            let b = std::fs::read(&p).ok()?;
            let rel = p.strip_prefix(root()).unwrap_or(&p).display().to_string();
            Some(Case {
                kind: "file",
                name: format!("@{rel}").into_bytes(),
                src: rn_compiler::skip_file_prefix(&b).to_vec(),
                listing: true,
                binary: false,
            })
        })
        .collect()
}

/// Rough Lua tokenizer for the mutators: token spans (comments and blanks are separators).
fn tokens(s: &[u8]) -> Vec<(usize, usize)> {
    let mut v = Vec::new();
    let mut i = 0;
    let long_open = |i: usize| -> Option<(usize, usize)> {
        // `[` `=`* `[` -> (level, length of the opener)
        if s.get(i) != Some(&b'[') {
            return None;
        }
        let mut j = i + 1;
        while s.get(j) == Some(&b'=') {
            j += 1;
        }
        (s.get(j) == Some(&b'[')).then(|| (j - i - 1, j + 1 - i))
    };
    let long_close = |mut j: usize, level: usize| -> usize {
        let close: Vec<u8> = std::iter::once(b']')
            .chain(std::iter::repeat_n(b'=', level))
            .chain(std::iter::once(b']'))
            .collect();
        while j < s.len() {
            if s[j..].starts_with(&close) {
                return j + close.len();
            }
            j += 1;
        }
        s.len()
    };
    const OPS: [&[u8]; 14] = [
        b"...", b"..", b"==", b"~=", b"<=", b">=", b"<<", b">>", b"//", b"::", b"->", b"=>", b"&&",
        b"||",
    ];
    while i < s.len() {
        let c = s[i];
        if c.is_ascii_whitespace() {
            i += 1;
        } else if s[i..].starts_with(b"--") {
            if let Some((lv, n)) = long_open(i + 2) {
                i = long_close(i + 2 + n, lv);
            } else {
                while i < s.len() && s[i] != b'\n' {
                    i += 1;
                }
            }
        } else if c.is_ascii_alphabetic() || c == b'_' {
            let st = i;
            while i < s.len() && (s[i].is_ascii_alphanumeric() || s[i] == b'_') {
                i += 1;
            }
            v.push((st, i));
        } else if c.is_ascii_digit() || (c == b'.' && s.get(i + 1).is_some_and(u8::is_ascii_digit))
        {
            let st = i;
            while i < s.len() {
                let d = s[i];
                let sign =
                    (d == b'+' || d == b'-') && matches!(s[i - 1], b'e' | b'E' | b'p' | b'P');
                if sign || d.is_ascii_alphanumeric() || d == b'.' || d == b'_' {
                    i += 1;
                } else {
                    break;
                }
            }
            v.push((st, i));
        } else if c == b'"' || c == b'\'' {
            let st = i;
            i += 1;
            while i < s.len() && s[i] != c && s[i] != b'\n' {
                if s[i] == b'\\' {
                    i += 1;
                }
                i += 1;
            }
            i = (i + 1).min(s.len());
            v.push((st, i));
        } else if let Some((lv, n)) = long_open(i) {
            let st = i;
            i = long_close(i + n, lv);
            v.push((st, i));
        } else {
            let st = i;
            let n = OPS
                .iter()
                .find(|o| s[i..].starts_with(o))
                .map_or(1, |o| o.len());
            i += n;
            v.push((st, i));
        }
    }
    v
}

const TOKS: &[&str] = &[
    "and",
    "break",
    "do",
    "else",
    "elseif",
    "end",
    "false",
    "for",
    "function",
    "goto",
    "if",
    "in",
    "local",
    "nil",
    "not",
    "or",
    "repeat",
    "return",
    "then",
    "true",
    "until",
    "while",
    "global",
    "+",
    "-",
    "*",
    "/",
    "%",
    "^",
    "#",
    "&",
    "~",
    "|",
    "<<",
    ">>",
    "//",
    "==",
    "~=",
    "<=",
    ">=",
    "<",
    ">",
    "=",
    "(",
    ")",
    "{",
    "}",
    "[",
    "]",
    "::",
    ";",
    ":",
    ",",
    ".",
    "..",
    "...",
    "x",
    "y",
    "t",
    "f",
    "_ENV",
    "self",
    "<const>",
    "<close>",
    "*",
    "1",
    "0",
    "2.5",
    "0x10",
    "1e10",
    "'s'",
    "\"s\"",
    "[[l]]",
    "-1",
    "...t",
    "lbl",
    "::lbl::",
    "goto lbl",
    "9223372036854775807",
    "9223372036854775808",
    "0x7fffffffffffffff",
    "0x8000000000000000",
    "1e309",
    "-0.0",
    "3//0",
    "0/0",
    "1 << 63",
    "'\\0'",
    "'\\u{10FFFF}'",
    "\n",
    "\r\n",
    "--c\n",
    "--[[c]]",
    "@",
    "$",
    "?",
    "!",
    "`",
    "\\",
    "\0",
];

fn mutate(rng: &mut Rng, src: &[u8]) -> Vec<u8> {
    let mut s = src.to_vec();
    for _ in 0..1 + rng.below(3) {
        let toks = tokens(&s);
        if toks.is_empty() {
            s.extend_from_slice(rng.pick(TOKS).as_bytes());
            continue;
        }
        let k = rng.below(toks.len() as u64) as usize;
        let (a, b) = toks[k];
        let pick_tok = |rng: &mut Rng, s: &[u8]| -> Vec<u8> {
            if rng.chance(1, 2) {
                rng.pick(TOKS).as_bytes().to_vec()
            } else {
                let (x, y) = toks[rng.below(toks.len() as u64) as usize];
                s[x..y].to_vec()
            }
        };
        match rng.below(10) {
            0 | 1 => {
                s.drain(a..b);
            }
            2 => {
                let t = pick_tok(rng, &s);
                let mut ins = b" ".to_vec();
                ins.extend(t);
                ins.push(b' ');
                s.splice(a..a, ins);
            }
            3 => {
                let t = pick_tok(rng, &s);
                s.splice(a..b, t);
            }
            4 if k + 1 < toks.len() => {
                let (c, d) = toks[k + 1];
                let mut rep = s[c..d].to_vec();
                rep.extend_from_slice(&s[b..c]);
                rep.extend_from_slice(&s[a..b]);
                s.splice(a..d, rep);
            }
            5 => {
                let t = s[a..b].to_vec();
                let mut ins = vec![b' '];
                ins.extend(t);
                s.splice(b..b, ins);
            }
            6 => {
                // delete a run of tokens
                let e = (k + 1 + rng.below(6) as usize).min(toks.len());
                s.drain(a..toks[e - 1].1);
            }
            7 => {
                let i = rng.below(s.len() as u64 + 1) as usize;
                s.insert(i, rng.next() as u8);
            }
            8 if !s.is_empty() => {
                let i = rng.below(s.len() as u64) as usize;
                if rng.chance(1, 2) {
                    s.remove(i);
                } else {
                    s[i] = rng.next() as u8;
                }
            }
            _ => {
                // copy a token run elsewhere
                let e = (k + 1 + rng.below(8) as usize).min(toks.len());
                let piece = s[a..toks[e - 1].1].to_vec();
                let (x, _) = toks[rng.below(toks.len() as u64) as usize];
                s.splice(x..x, piece);
            }
        }
    }
    s
}

/// Grammar-ish random program generator.
struct Gen<'a> {
    rng: &'a mut Rng,
    out: String,
    depth: usize,
    max_depth: usize,
}

const NAMES: &[&str] = &[
    "a", "b", "c", "x", "y", "t", "f", "i", "k", "v", "_ENV", "self", "print", "g",
];
const NUMS: &[&str] = &[
    "0",
    "1",
    "2",
    "3",
    "-1",
    "255",
    "256",
    "65535",
    "65536",
    "1.5",
    "0.0",
    "-0.0",
    "1e308",
    "1e309",
    "2^53",
    "2^63",
    "9223372036854775807",
    "9223372036854775808",
    "0x7fffffffffffffff",
    "0xffffffffffffffff",
    "(-9223372036854775807-1)",
    "0x1p-1074",
    ".5",
    "3.",
    "0x.8",
    "0xA.8P1",
    "100000000000000",
    "1/0",
    "-1/0",
    "0/0",
    "131071",
    "131072",
    "-131072",
    "-131071",
    "16777215",
    "-16777216",
    "4294967296",
];
const STRS: &[&str] = &[
    "''",
    "'a'",
    "\"b\"",
    "'\\n'",
    "[[long]]",
    "[==[x]==]",
    "'\\x41\\65\\u{7FF}'",
    "'\\z\n   z'",
    "'10'",
    "'0x10'",
    "'1e1'",
    "' 5 '",
    "'abcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyz'",
];
const BINOPS: &[&str] = &[
    "+", "-", "*", "/", "//", "%", "^", "..", "==", "~=", "<", "<=", ">", ">=", "and", "or", "&",
    "|", "~", "<<", ">>",
];
const UNOPS: &[&str] = &["-", "not ", "#", "~"];

impl Gen<'_> {
    fn w(&mut self, s: &str) {
        // separators sometimes become line breaks (instruction line info, `fixline`)
        let s = match s {
            " " if self.rng.chance(1, 8) => "\n",
            ", " if self.rng.chance(1, 8) => ",\n\n",
            s => s,
        };
        self.out.push_str(s);
    }
    fn name(&mut self) {
        let n = *self.rng.pick(NAMES);
        self.w(n);
    }
    fn sp(&mut self) {
        let c = match self.rng.below(12) {
            0 => "\n",
            1 => "  ",
            2 => "\n\n",
            3 => "\t",
            4 if self.rng.chance(1, 4) => "\r\n",
            5 if self.rng.chance(1, 4) => " --c\n",
            _ => " ",
        };
        self.w(c);
    }
    fn exp(&mut self) {
        self.depth += 1;
        let deep = self.depth >= self.max_depth;
        match if deep {
            self.rng.below(4)
        } else {
            self.rng.below(16)
        } {
            0 => {
                let n = *self.rng.pick(NUMS);
                self.w(n);
            }
            1 => {
                let s = *self.rng.pick(STRS);
                self.w(s);
            }
            2 => {
                let s = *self.rng.pick(&["nil", "true", "false", "..."]);
                self.w(s);
            }
            3 => self.name(),
            4..=6 => {
                self.exp();
                self.w(" ");
                let o = *self.rng.pick(BINOPS);
                self.w(o);
                self.w(" ");
                self.exp();
            }
            7 => {
                let o = *self.rng.pick(UNOPS);
                self.w(o);
                self.exp();
            }
            8 => {
                self.w("(");
                self.exp();
                self.w(")");
            }
            9 => self.table(),
            10 => {
                self.w("function");
                self.body();
            }
            11 | 12 => self.suffixed(),
            _ => {
                self.suffixed();
                self.args();
            }
        }
        self.depth -= 1;
    }
    fn explist(&mut self) {
        let n = 1 + self.rng.below(3);
        for i in 0..n {
            if i > 0 {
                self.w(", ");
            }
            self.exp();
        }
    }
    fn table(&mut self) {
        self.w("{");
        let n = self.rng.below(5);
        for i in 0..n {
            if i > 0 {
                let sep = *self.rng.pick(&[", ", "; ", ","]);
                self.w(sep);
            }
            match self.rng.below(3) {
                0 => {
                    self.name();
                    self.w(" = ");
                    self.exp();
                }
                1 => {
                    self.w("[");
                    self.exp();
                    self.w("] = ");
                    self.exp();
                }
                _ => self.exp(),
            }
        }
        if self.rng.chance(1, 6) {
            self.w(",");
        }
        self.w("}");
    }
    fn args(&mut self) {
        match self.rng.below(5) {
            0 => {
                let s = *self.rng.pick(STRS);
                self.w(s);
            }
            1 => self.table(),
            2 => self.w("()"),
            _ => {
                self.w("(");
                self.explist();
                self.w(")");
            }
        }
    }
    fn suffixed(&mut self) {
        if self.rng.chance(1, 5) && self.depth < self.max_depth {
            self.w("(");
            self.exp();
            self.w(")");
        } else {
            self.name();
        }
        for _ in 0..self.rng.below(3) {
            match self.rng.below(4) {
                0 => {
                    self.w(".");
                    self.name();
                }
                1 if self.depth < self.max_depth => {
                    self.w("[");
                    self.exp();
                    self.w("]");
                }
                2 => {
                    self.w(":");
                    self.name();
                    self.args();
                }
                _ => self.args(),
            }
        }
    }
    fn params(&mut self) {
        self.w("(");
        let n = self.rng.below(4);
        for i in 0..n {
            if i > 0 {
                self.w(", ");
            }
            self.name();
        }
        if self.rng.chance(1, 3) {
            if n > 0 {
                self.w(", ");
            }
            self.w("...");
            if self.rng.chance(1, 3) {
                self.name();
            }
        }
        self.w(")");
    }
    fn body(&mut self) {
        self.params();
        self.block();
        self.w(" end");
    }
    fn attrib(&mut self) {
        if self.rng.chance(1, 3) {
            let a = *self
                .rng
                .pick(&["<const>", "<close>", " <const>", "<const >"]);
            self.w(a);
        }
    }
    fn stmt(&mut self) {
        self.depth += 1;
        let deep = self.depth >= self.max_depth;
        let k = if deep {
            self.rng.below(5)
        } else {
            self.rng.below(22)
        };
        match k {
            0 => {
                self.suffixed();
                self.w(" = ");
                self.explist();
            }
            1 => {
                self.w("local ");
                let n = 1 + self.rng.below(3);
                for i in 0..n {
                    if i > 0 {
                        self.w(", ");
                    }
                    self.name();
                    self.attrib();
                }
                if self.rng.chance(3, 4) {
                    self.w(" = ");
                    self.explist();
                }
            }
            2 => {
                self.suffixed();
                self.args();
            }
            3 => self.w(";"),
            4 => {
                let l = *self.rng.pick(&["L1", "L2", "continue"]);
                if self.rng.chance(1, 2) {
                    self.w("goto ");
                    self.w(l);
                } else {
                    self.w("::");
                    self.w(l);
                    self.w("::");
                }
            }
            5 => {
                self.w("do");
                self.block();
                self.w(" end");
            }
            6 => {
                self.w("while ");
                self.exp();
                self.w(" do");
                self.block();
                self.w(" end");
            }
            7 => {
                self.w("repeat");
                self.block();
                self.w(" until ");
                self.exp();
            }
            8 => {
                self.w("if ");
                self.exp();
                self.w(" then");
                self.block();
                for _ in 0..self.rng.below(3) {
                    self.w(" elseif ");
                    self.exp();
                    self.w(" then");
                    self.block();
                }
                if self.rng.chance(1, 2) {
                    self.w(" else");
                    self.block();
                }
                self.w(" end");
            }
            9 => {
                self.w("for ");
                self.name();
                self.w(" = ");
                self.exp();
                self.w(", ");
                self.exp();
                if self.rng.chance(1, 2) {
                    self.w(", ");
                    self.exp();
                }
                self.w(" do");
                self.block();
                self.w(" end");
            }
            10 => {
                self.w("for ");
                self.name();
                for _ in 0..self.rng.below(3) {
                    self.w(", ");
                    self.name();
                }
                self.w(" in ");
                self.explist();
                self.w(" do");
                self.block();
                self.w(" end");
            }
            11 => {
                self.w("function ");
                self.name();
                for _ in 0..self.rng.below(3) {
                    self.w(".");
                    self.name();
                }
                if self.rng.chance(1, 3) {
                    self.w(":");
                    self.name();
                }
                self.body();
            }
            12 => {
                self.w("local function ");
                self.name();
                self.body();
            }
            13 => self.w("break"),
            14 | 15 => {
                // 5.5 global declarations (an ordinary name in 5.4)
                self.w("global");
                match self.rng.below(4) {
                    0 => {
                        self.attrib();
                        self.w(" *");
                    }
                    1 => {
                        self.w(" function ");
                        self.name();
                        self.body();
                    }
                    _ => {
                        self.attrib();
                        self.w(" ");
                        let n = 1 + self.rng.below(3);
                        for i in 0..n {
                            if i > 0 {
                                self.w(", ");
                            }
                            self.name();
                            self.attrib();
                        }
                        if self.rng.chance(1, 2) {
                            self.w(" = ");
                            self.explist();
                        }
                    }
                }
            }
            16 => {
                self.name();
                for _ in 0..self.rng.below(3) {
                    self.w(", ");
                    self.suffixed();
                }
                self.w(" = ");
                self.explist();
            }
            _ => {
                self.w("local ");
                self.name();
                self.w(" = ");
                self.exp();
            }
        }
        self.depth -= 1;
    }
    fn block(&mut self) {
        let n = self.rng.below(5);
        for _ in 0..n {
            self.sp();
            self.stmt();
        }
        if self.rng.chance(1, 4) {
            self.sp();
            self.w("return");
            if self.rng.chance(2, 3) {
                self.w(" ");
                self.explist();
            }
            if self.rng.chance(1, 4) {
                self.w(";");
            }
        }
        self.sp();
    }
}

fn gen_program(rng: &mut Rng) -> String {
    let max_depth = 2 + rng.below(6) as usize;
    let mut pre = String::new();
    let mut post = String::new();
    match rng.below(12) {
        // constant pool beyond MAXINDEXRK / the 8-bit K operands
        0 => {
            let n = 250 + rng.below(60) as usize;
            pre = format!(
                "local _k = {{{}}}\n",
                (0..n)
                    .map(|i| format!("'k{i}', {i}.25"))
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
        // register pressure close to MAXREGS
        1 => {
            let n = 150 + rng.below(50) as usize;
            pre = format!("local {}\n", names_list("r", n, ", "));
        }
        // the names become upvalues of a nested function
        2 => {
            pre = format!(
                "local {} = ...\nreturn function(...)\n",
                NAMES[..11].join(", ")
            );
            post = "\nend".into();
        }
        3 => pre = "local _ENV = {}\n".into(),
        4 => pre = "local _ENV <const> = _ENV\n".into(),
        5 if V55 => pre = "global <const> *\n".into(),
        _ => {}
    }
    let mut g = Gen {
        rng,
        out: pre,
        depth: 0,
        max_depth,
    };
    g.block();
    g.out.push_str(&post);
    g.out
}

fn gen_soup(rng: &mut Rng) -> Vec<u8> {
    let n = 1 + rng.below(30);
    let mut s = Vec::new();
    for _ in 0..n {
        s.extend_from_slice(rng.pick(TOKS).as_bytes());
        s.push(*rng.pick(b"  \n"));
    }
    s
}

fn names_list(prefix: &str, n: usize, sep: &str) -> String {
    (0..n)
        .map(|i| format!("{prefix}{i}"))
        .collect::<Vec<_>>()
        .join(sep)
}

/// Literal, lexer, operator, limit and declaration edge cases (deterministic).
fn edge_cases() -> Vec<Case> {
    let mut v: Vec<String> = Vec::new();
    let numerals = [
        "0",
        "1",
        "00",
        "007",
        "08",
        "0x",
        "0X",
        "0x0",
        "0xffffffffffffffff",
        "0x7fffffffffffffff",
        "0x8000000000000000",
        "0x10000000000000000",
        "0x1p4",
        "0x1P-4",
        "0x.1",
        "0x1.",
        "0x.",
        "0x1.8p+1",
        "0x1e",
        "0x1e+1",
        "0xg",
        "0x1p",
        "0x1p+",
        "0xp1",
        "0x.p1",
        "1e",
        "1e+",
        "1e-",
        "1e10",
        "1E+308",
        "1e309",
        "1e-400",
        "2e-324",
        ".5",
        ".",
        "5.",
        "5..",
        "5...",
        "5.5.5",
        "3.e2",
        "3e2.5",
        "9223372036854775807",
        "9223372036854775808",
        "18446744073709551615",
        "18446744073709551616",
        "123456789012345678901234567890",
        "0.1e",
        "1x",
        "1_000",
        "0b101",
        "1ee2",
        "1e2e3",
        "3..2",
        "1..2",
        "0x123456789abcdef0123",
        "1.7976931348623157e308",
        "1.7976931348623159e308",
        "4.9e-324",
        "0e0",
        "0x0p0",
        "0xA",
        "0Xa",
        "0xa.",
        "1e+5x",
        "1.5e",
        "0x1.ffffffffffffffp1023",
        "0x1p1024",
        "0x1p-1075",
        "0x.0000000000001p-1022",
        "0e",
        "1.e",
        "1.e1",
        "1.0e-0",
        "3",
        "3 3",
        "1.",
        "0x1P+",
        "00000000000000000000001",
        "0.000000000000000000000000000000000000001",
        "1e00000000000000000000000001",
        "0x00000000000000000000000000000001",
        "0x1000000000000000000000000000000",
        "1",
        "2.2250738585072014e-308",
        "1e-310",
        "0x.Fp0",
        "0xF.Fp-0",
    ];
    for n in numerals {
        v.push(format!("return {n}"));
        v.push(format!("return -{n}"));
        v.push(format!(
            "local x = {n}; return x, -x, x + 1, x * 2.5, x // 1, x % 7"
        ));
        v.push(format!("x = {n}"));
        v.push(n.to_string());
        v.push(format!("return {n}..''"));
        v.push(format!("return {n}x"));
        v.push(format!("return {n}.."));
        v.push(format!("return {{{n}, {n}}}"));
    }
    let esc = [
        "\\a\\b\\f\\n\\r\\t\\v\\\\\\\"\\'",
        "\\z   \n  x",
        "\\z",
        "\\x41",
        "\\x4",
        "\\xg",
        "\\x",
        "\\65",
        "\\256",
        "\\255",
        "\\0",
        "\\000",
        "\\0001",
        "\\u{41}",
        "\\u{7FFFFFFF}",
        "\\u{80000000}",
        "\\u{}",
        "\\u{110000}",
        "\\u41",
        "\\u{41",
        "\\u{d800}",
        "\\u{0}",
        "\\u{000000000000041}",
        "\\u{7FF}\\u{FFFF}\\u{10FFFF}",
        "\\q",
        "\\",
        "abc\ndef",
        "abc\\\ndef",
        "abc\\\r\ndef",
        "abc\\\n\rdef",
        "abc\\\r\rdef",
        "\\9",
        "\\99",
        "\\999",
        "\\1234",
        "a\\0b",
        "\\xff\\xFF\\x00",
        "\\u{G}",
        "\\u",
        "\\u{",
        "\\z\\z",
        "x\\z\r\n\r\n y",
        "\\\"",
        "\t\x01\x7f\u{80}",
        "\r",
        "\\\r",
        "\\\n",
    ];
    for e in esc {
        for q in ["'", "\""] {
            v.push(format!("return {q}{e}{q}"));
            v.push(format!("x = {q}{e}{q} 1"));
            v.push(format!("return {q}{e}"));
        }
    }
    let longs = [
        "[[x]]",
        "[==[x]==]",
        "[==[x]=]",
        "[=[x",
        "[=",
        "[==",
        "[[\nx]]",
        "[[\r\nx]]",
        "[[\n\rx]]",
        "[[\r\rx]]",
        "[[]]]",
        "[=[]]]=]",
        "[[ [[ ]]",
        "[[\\n]]",
        "[=[a]=]=]",
        "[ [x]]",
        "[=x",
        "[[a\nb\r\nc\n\rd]]",
        "[====[]====]",
        "[[\n]]",
        "[[\n\n]]",
    ];
    for l in longs {
        v.push(format!("return {l}"));
        v.push(format!("x = {l} 1"));
        v.push(format!("--{l} return 1"));
        v.push(format!("--{l}\nreturn 1"));
    }
    for c in [
        "--[[ unfinished",
        "--[=[ unfinished",
        "--[ [",
        "-- x\nreturn",
        "--[==",
        "#!shebang",
        "\u{feff}return 1",
        "-- \r return 1",
        "--\r\nx=",
        "--[[\n]]x=",
        "--[[]]--[[]]return 1",
        "return --[[ inline ]] 1",
        "return 1--[[x",
        "x = 1 --[==[ a ]] b ]=] c ]==] y = 2",
    ] {
        v.push(c.to_string());
    }
    // line ending variants with an error on a later line
    for nl in [
        "\n", "\r", "\r\n", "\n\r", "\n\n", "\r\r", "\n\r\n", "\r\n\r",
    ] {
        v.push(format!("x = 1{nl}y ={nl}{nl}@"));
        v.push(format!("{nl}{nl}{nl}local a = {nl}'unfinished"));
        v.push(format!("local t = {{{nl}1,{nl}2{nl}3}}"));
        v.push(format!("f({nl}){nl}({nl}g){nl}()"));
        v.push(format!("return [[{nl}a{nl}]]{nl}..{nl}"));
        v.push(format!("x = 'a\\{nl}b'{nl}y = = 1"));
        v.push(format!(
            "local function f(){nl}return{nl}end{nl}return f{nl}+"
        ));
    }
    // every single byte, as an expression and as a statement
    for b in 0u8..=255 {
        v.push(String::from_utf8_lossy(&[b'x', b' ', b'=', b' ', b]).into_owned());
    }
    // operators and precedence
    for s in [
        "return 2^3^2",
        "return -2^2",
        "return not 1 == 2",
        "return 1 .. 2 .. 3",
        "return 1 + 2 * 3 - 4 / 5 // 6 % 7",
        "return 1 << 2 + 3",
        "return 1 & 2 | 3 ~ 4",
        "return 1 < 2 == true",
        "return a and b or c and d",
        "return ~~1",
        "return - -1",
        "return #'abc' + 1",
        "return 'a'..1 .. 2",
        "return 1 ..2",
        "return not not nil",
        "return - - - 1",
        "return 2^-2",
        "return -x^-y",
        "return (1)(2)",
        "return a.b.c:d(e)[f]",
        "return 1 // 0",
        "return 1 % 0",
        "return 1.0 // 0",
        "return 1.0 % 0",
        "return 0/0 ~= 0/0",
        "return 1 < 2 < 3",
        "return 'a' < 'b'",
        "return 1 == 1.0",
        "return 3 & 1.0",
        "return 3 & 1.5",
        "return '3' & 1",
        "return 1 << 64",
        "return 1 << -1",
        "return 1 >> 63",
        "return ~0",
        "return ~1.0",
        "return ~1.5",
        "return -(-9223372036854775807-1)",
        "return math.mininteger // -1",
        "return (-9223372036854775807-1) // -1",
        "return (-9223372036854775807-1) % -1",
        "return 9223372036854775807 + 1",
        "return 2^1024",
        "return -0.0",
        "return 0.0 * -1",
        "return 1e308 * 10",
        "return a + b * c ^ d .. e < f and g or h",
        "return #t[1]",
        "return -t.x",
        "return a..b..c",
        "return f() + g()",
        "return x == nil",
        "return nil == x",
        "return 1 ~= x",
        "return x and nil",
        "return nil and x",
        "return false or x",
        "return x or true",
        "return (x)",
        "return (f())",
        "return (...)",
        "return ...",
        "local a = ... return a",
        "return x >= 1",
        "return 1 >= x",
        "return x > 255",
        "return x < 256",
        "return x == 255",
        "return x == 256",
        "return x == 'k'",
        "return x == 1.5",
        "return x == -1",
        "return x == -129",
        "return x < -128",
        "return x + 127",
        "return x - 128",
        "return x + 1.0",
        "return x * -1",
        "return x .. 1",
        "return x // 2",
        "return x % 2",
        "return x >> 1",
        "return x << 1",
        "return 1 >> x",
        "return 1 << x",
        "return x & 0xff",
        "return x | 1",
        "return x ~ 1",
        "return 1 - x",
        "return 2 * x",
        "return 2 ^ x",
        "return x ^ 2",
        "return x / 2",
        "return 2 / x",
        "return 2 .. x",
    ] {
        v.push(s.to_string());
    }
    // constant folding: every binary operator on a set of constants
    let ks = [
        "0",
        "1",
        "-1",
        "2",
        "3",
        "0.0",
        "-0.0",
        "1.5",
        "1e308",
        "9223372036854775807",
        "(-9223372036854775807-1)",
        "1/0",
        "0/0",
        "2^63",
        "'1'",
        "nil",
        "true",
        "x",
        "63",
        "64",
        "-64",
        "0x7fffffff",
    ];
    for op in BINOPS {
        for a in ks {
            for b in ks {
                v.push(format!("return {a} {op} {b}"));
            }
        }
    }
    for op in UNOPS {
        for a in ks {
            v.push(format!("return {op}{a}"));
            v.push(format!("return {op}({op}{a})"));
            v.push(format!("local k <const> = {a}; return {op}k"));
        }
    }
    for a in ks {
        for b in ["1", "2.0", "x", "'s'"] {
            v.push(format!("local k <const> = {a}; local j <const> = {b}; return k + j, k .. j, k == j, k < j, -k, {{k, j}}, k and j"));
            v.push(format!(
                "local k <const> = {a}; local function f() return k, k + 1 end return f"
            ));
            v.push(format!(
                "local k <const> = {a}; x = k; t[k] = k; t.k = k; return t[k]"
            ));
            v.push(format!(
                "if {a} then x = 1 elseif {b} then x = 2 else x = 3 end"
            ));
            v.push(format!("while {a} do x = 1 end repeat x = 2 until {b}"));
            v.push(format!("for i = {a}, {b} do end"));
            v.push(format!("for i = {a}, {b}, {a} do end"));
            v.push(format!("return {a} == {b} and {a} or {b}"));
            v.push(format!("local x <const> = {a}; if x then return 1 end"));
            v.push(format!("return t[{a}], t.x, t[{b}]"));
        }
    }
    // goto / labels / break
    for s in [
        "goto l; ::l::",
        "::l:: goto l",
        "do goto l end ::l::",
        "do ::l:: end goto l",
        "goto l; local x; ::l::",
        "goto l; local x; ::l:: return",
        "goto l; local x; ::l:: ;;",
        "do goto l; local x; ::l:: end",
        "do goto l; local x; ::l:: x = 1 end",
        "while x do goto continue; local a; ::continue:: end",
        "repeat goto continue; local a; ::continue:: until a",
        "repeat local a; goto continue; ::continue:: until a",
        "for i = 1, 2 do if i then goto next end ::next:: end",
        "::a:: ::b:: ::a::",
        "::a:: do ::a:: end",
        "do ::a:: end ::a::",
        "goto a; do ::a:: end",
        "do goto a end",
        "local function f() goto a end ::a::",
        "::a:: local function f() goto a end",
        "break",
        "do break end",
        "while 1 do break break end",
        "repeat break until 1",
        "while 1 do local function f() break end end",
        "if x then break end",
        "for i = 1, 2 do do break end end",
        "goto continue",
        "::continue::",
        "goto goto",
        "goto 1",
        "goto ;",
        ":: a ::",
        "::a",
        "::a: :",
        "::a:: ::b:: goto a goto b",
        "while true do ::top:: goto top end",
        "do local x ::l1:: local y goto l1 end",
        "goto l1 do ::l1:: end ::l1::",
        "local x <close> = nil goto l ::l::",
        "do local x <close> = nil goto l end ::l::",
        "goto l local x <const> = 1 ::l::",
        "goto l local x <const> = 1 ::l:: return x",
        "do goto l end local x = 1 ::l:: print(x)",
        "do goto l end local x = 1 ::l::",
        "do goto l end local x = 1 ::l:: ;",
        "if a then goto l1 elseif b then goto l2 else goto l3 end ::l1:: ::l2:: ::l3::",
    ] {
        v.push(s.to_string());
    }
    // attributes, globals, varargs
    for s in [
        "local x <const> = 1",
        "local x <close> = nil",
        "local x <const>",
        "local x <close>",
        "local x <foo> = 1",
        "local x <const> = 1; x = 2",
        "local x <const>, y <const> = 1, 2",
        "local x <close>, y <close> = nil, nil",
        "local x <const>, y <close> = 1, nil",
        "local x, y <close>, z = 1, 2, 3",
        "local x <const> = f()",
        "local x <const> = {}",
        "local x <const> = 'str'; return x .. x",
        "local x <const> = 1.5; return x * 2",
        "local x <const> = nil; return x",
        "local x <const> = true; return not x",
        "local x <const> = ...; return x",
        "local x < const > = 1",
        "local x <const = 1",
        "local x <> = 1",
        "local x <const><close> = 1",
        "local function f <const> () end",
        "local x <close> = 1; local function f() return x end",
        "local x <const> = 1; local function f() x = 2 end",
        "local x <const> = 1; local function f() return function() x = 2 end end",
        "global x",
        "global x, y",
        "global x <const>",
        "global <const> x",
        "global <const> *",
        "global *",
        "global * ; x = 1",
        "global x; y = 1",
        "global x = 1",
        "global x, y = 1, 2",
        "global x <close>",
        "global <close> x",
        "global function f() end",
        "global function f.g() end",
        "global function f:g() end",
        "global x; local x; x = 1",
        "global <const> x; x = 1",
        "global x <const> = 1; x = 2",
        "global x; do global *; y = 1 end",
        "local _ENV = {}; global x; x = 1",
        "global _ENV; _ENV = 1",
        "global x, *",
        "global *, x",
        "global (x)",
        "global 1",
        "global x =",
        "global x = 1, 2, 3",
        "global <const> x, y = 1; return x + y",
        "global x <const>, y; y = 2; x = 1",
        "function f(...) return ... end",
        "function f(...t) return t end",
        "function f(...t) return ... end",
        "function f(...t) t = 1 return t end",
        "function f(...t) local function g() return t end end",
        "function f(...t) return select('#', ...) end",
        "function f(a, ...t) return a, t[1] end",
        "function f(...t) t[1] = 2 return t end",
        "function f(...t) return #t end",
        "function f() return ...t end",
        "return ...t",
        "local function f(..., t) end",
        "function f(...t, a) end",
        "function f(... t) end",
        "function f(...t) local t end",
        "local function f(...) local a <const> = ... return a end",
    ] {
        v.push(s.to_string());
    }
    // nesting limits (boundary of LUAI_MAXCCALLS) -- validates the driver's C depth == luac's
    for n in [195usize, 197, 198, 199, 200, 201] {
        v.push("do ".repeat(n) + &"end ".repeat(n));
        v.push(format!("x = {}1{}", "(".repeat(n), ")".repeat(n)));
        v.push(format!("x = {}1{}", "{".repeat(n), "}".repeat(n)));
        v.push(format!("x = {}1", "- ".repeat(n)));
        v.push(format!("x = 1{}", " .. 1".repeat(n)));
        v.push(format!("x = 1{}", " ^ 1".repeat(n)));
        v.push("function f() ".repeat(n) + &"end ".repeat(n));
        v.push(format!("{}x = 1", "a, ".repeat(n)));
        v.push(format!("x = {}{}", "f(".repeat(n), ")".repeat(n)));
        v.push(format!("x = {}", "a.b[".repeat(n / 2)) + &"1]".repeat(n / 2));
        if V55 {
            v.push(format!("global x = {}1{}", "{".repeat(n), "}".repeat(n)));
        }
    }
    // registers, locals, upvalues, constants, table constructors
    for n in [198usize, 199, 200, 201] {
        v.push(format!("local {}", names_list("a", n, ", ")));
        v.push(names_list("local a", n, "\n"));
        v.push(format!(
            "local function f()\n{}\nend",
            names_list("local a", n, "\n")
        ));
        v.push(format!("function f({}) end", names_list("p", n, ", ")));
        v.push(format!("for {} in x do end", names_list("v", n, ", ")));
    }
    for n in [250usize, 252, 253, 254, 255, 256, 260] {
        v.push(format!(
            "return f({})",
            (0..n).map(|i| i.to_string()).collect::<Vec<_>>().join(", ")
        ));
        v.push(format!(
            "return {{{}}}",
            (0..n)
                .map(|_| "x".to_string())
                .collect::<Vec<_>>()
                .join(", ")
        ));
        v.push(format!(
            "local t = {{{}}}",
            (0..n)
                .map(|i| format!("f({i})"))
                .collect::<Vec<_>>()
                .join(", ")
        ));
        v.push(format!("x = {}1", "a + (".repeat(n)) + &")".repeat(n));
        v.push(format!("return {}", names_list("x", n, ", ")));
        v.push(format!("{} = 1", names_list("x", n.min(180), ", ")));
        // upvalues: split across two enclosing functions
        let (a, b) = (150, n - 150);
        v.push(format!(
            "local {}\nlocal function g()\nlocal {}\nreturn function() return {} + {} end end",
            names_list("a", a, ", "),
            names_list("b", b, ", "),
            names_list("a", a, " + "),
            names_list("b", b, " + ")
        ));
    }
    for n in [
        255usize, 256, 257, 1000, 65535, 65536, 65537, 131071, 131072, 131073,
    ] {
        if n > 20000 && !big_budget() {
            continue;
        }
        // n distinct constants
        v.push(format!(
            "local t\n{}",
            (0..n).map(|i| format!("t = {i}.5\n")).collect::<String>()
        ));
        v.push(format!(
            "local t\n{}",
            (0..n).map(|i| format!("t = 's{i}'\n")).collect::<String>()
        ));
        v.push(format!(
            "local t\n{}",
            (0..n.min(70000))
                .map(|i| format!("t = t.f{i}\n"))
                .collect::<String>()
        ));
    }
    for n in [
        0usize, 1, 2, 15, 16, 17, 49, 50, 51, 100, 127, 128, 255, 256, 257, 511, 512, 1000, 12750,
        12751, 25600, 70000,
    ] {
        v.push(format!("return {{{}}}", "1,".repeat(n)));
        v.push(format!(
            "return {{{}}}",
            (0..n).map(|i| format!("k{i}=1,")).collect::<String>()
        ));
        v.push(format!("return {{{}f()}}", "1,".repeat(n)));
        v.push(format!("return {{{}...}}", "1,".repeat(n)));
        v.push(format!("return {{{}[1]=2, 3}}", "x,".repeat(n)));
    }
    // line numbers: big gaps and long functions (abslineinfo every 128 instructions / deltas > 127)
    for gap in [1usize, 126, 127, 128, 129, 255, 256, 1000, 70000] {
        v.push(format!(
            "x = 1{}y = 2{}z =",
            "\n".repeat(gap),
            "\n".repeat(gap)
        ));
        v.push(format!(
            "local a\n{}a = 1\n{}return a",
            "\n".repeat(gap),
            "x = a\n".repeat(130)
        ));
        v.push(format!(
            "local function f()\n{}end\n{}",
            "a = 1 b = 2\n".repeat(gap.min(300)),
            "\n".repeat(gap)
        ));
        v.push(format!("local t = {{\n{}}}", "1,\n".repeat(gap.min(3000))));
    }
    // long lines, names, chunk ids
    let longname = "n".repeat(1000);
    v.push(format!("local {longname} = 1 return {longname}"));
    v.push(format!("{longname} {longname}"));
    v.push(format!("x = {}", "1 + ".repeat(20000) + "1"));
    v.push(format!("x = '{}' + = 1", "s".repeat(5000)));
    v.push(format!("x = [[{}", "s".repeat(5000)));
    v.push(format!("x = {} =", "9".repeat(400)));
    v.push(format!("x = 1 {}", "y".repeat(400)));
    v.push(format!("x = '{}\\q'", "y".repeat(400)));
    let mut cases: Vec<Case> = v.into_iter().map(|s| stdin_case("edge", s)).collect();
    // chunk names: luaO_chunkid formats
    let bodies = [
        "x = = 1",
        "x = 1\ny =",
        "return 1",
        "local x <const> = 1; x = 2",
    ];
    let names: Vec<String> = vec![
        "=stdin".into(),
        "@x.lua".into(),
        format!("@{}", "d/".repeat(40) + "file.lua"),
        format!("={}", "n".repeat(100)),
        "=".into(),
        "@".into(),
        "".into(),
        "x = 1".into(),
        "x = 1\ny = 2".into(),
        "\nleading newline".into(),
        "s".repeat(44),
        "s".repeat(45),
        "s".repeat(46),
        "s".repeat(100),
        "%s %d %%".into(),
        format!("@{}", "f".repeat(58)),
        format!("@{}", "f".repeat(59)),
        format!("@{}", "f".repeat(60)),
        format!("={}", "f".repeat(59)),
        format!("={}", "f".repeat(60)),
        "\x1bLua".into(),
    ];
    for n in &names {
        for b in bodies {
            cases.push(Case {
                kind: "chunkname",
                name: n.clone().into_bytes(),
                src: b.as_bytes().to_vec(),
                listing: true,
                binary: false,
            });
        }
    }
    cases
}

fn random_name(rng: &mut Rng) -> Vec<u8> {
    match rng.below(10) {
        0 => b"@t.lua".to_vec(),
        1 => format!("@{}", "p/".repeat(rng.below(40) as usize)).into_bytes(),
        2 => b"x = 1\nreturn".to_vec(),
        3 => "s".repeat(rng.below(80) as usize).into_bytes(),
        _ => b"=stdin".to_vec(),
    }
}

fn big_budget() -> bool {
    std::env::var("RUSTYNAIL_PARSE_DIFF_CASES").is_ok()
}

/// Generated cases: truncations, token mutations, grammar programs and token soup.
fn generated(rng: &mut Rng, n: usize, seeds: &[Vec<u8>], big: &[Vec<u8>]) -> Vec<Case> {
    let mut v = Vec::with_capacity(n);
    while v.len() < n {
        let listing = rng.chance(1, 4);
        let (kind, src): (&'static str, Vec<u8>) = match rng.below(20) {
            0..=3 => {
                let s = rng.pick(seeds);
                (
                    "truncate",
                    s[..rng.below(s.len() as u64 + 1) as usize].to_vec(),
                )
            }
            4..=9 => {
                let s = rng.pick(seeds).clone();
                ("mutate", mutate(rng, &s))
            }
            10 => {
                // any corpus file, including the big official test files
                let s = rng.pick(big).clone();
                ("mutate-big", mutate(rng, &s))
            }
            11..=14 => ("grammar", gen_program(rng).into_bytes()),
            15..=16 => {
                let p = gen_program(rng).into_bytes();
                ("grammar-mutate", mutate(rng, &p))
            }
            17 => {
                // crossover: a token run of one file spliced into another
                let a = rng.pick(seeds).clone();
                let b = rng.pick(seeds);
                let (ta, tb) = (tokens(&a), tokens(b));
                let mut s = a.clone();
                if !ta.is_empty() && !tb.is_empty() {
                    let k = rng.below(tb.len() as u64) as usize;
                    let e = (k + 1 + rng.below(20) as usize).min(tb.len());
                    let piece = b[tb[k].0..tb[e - 1].1].to_vec();
                    let at = ta[rng.below(ta.len() as u64) as usize].0;
                    s.splice(at..at, piece);
                }
                ("crossover", s)
            }
            _ => ("soup", gen_soup(rng)),
        };
        let name = if rng.chance(1, 8) {
            random_name(rng)
        } else {
            b"=stdin".to_vec()
        };
        v.push(Case {
            kind,
            name,
            src,
            listing,
            binary: false,
        });
    }
    v
}

/// Every prefix of each small seed file (exhaustive truncation).
fn all_truncations(seeds: &[Vec<u8>], max_len: usize) -> Vec<Case> {
    let mut v = Vec::new();
    for s in seeds.iter().filter(|s| s.len() <= max_len) {
        for i in 0..=s.len() {
            v.push(Case {
                kind: "truncate-all",
                name: b"=stdin".to_vec(),
                src: s[..i].to_vec(),
                listing: false,
                binary: false,
            });
        }
    }
    v
}

fn dump_mismatches(stats: &Stats, minimize_them: bool) {
    let out = std::env::var_os("RUSTYNAIL_PARSE_DIFF_OUT").map(PathBuf::from);
    if let Some(d) = &out {
        let _ = std::fs::create_dir_all(d);
    }
    for (cat, list) in &stats.mism {
        eprintln!("== {cat}: {} case(s)", list.len());
        for (i, (c, detail)) in list.iter().enumerate() {
            if let Some(d) = &out {
                let base = d.join(format!("{}-{i}", cat.replace(['(', ')'], "_")));
                let _ = std::fs::write(base.with_extension("lua"), &c.src);
                let _ = std::fs::write(
                    base.with_extension("txt"),
                    format!("kind {}\nname {:?}\n{detail}\n", c.kind, lossy(&c.name)),
                );
                if minimize_them && i < 20 {
                    let m = minimize(c, cat);
                    let _ = std::fs::write(base.with_extension("min.lua"), &m.src);
                }
            }
            if i < 5 {
                let src: String = lossy(&c.src).chars().take(300).collect();
                eprintln!(
                    "-- [{}] name {:?} src {src:?}\n{detail}",
                    c.kind,
                    lossy(&c.name)
                );
            }
        }
    }
}

#[test]
fn parser_matches_reference() {
    if driver().is_none() {
        return;
    }
    let t0 = Instant::now();
    let budget: usize = std::env::var("RUSTYNAIL_PARSE_DIFF_CASES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(3000);
    let seed: u64 = std::env::var("RUSTYNAIL_PARSE_DIFF_SEED")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1);
    let workers = std::env::var("RUSTYNAIL_PARSE_DIFF_JOBS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(|| {
            std::thread::available_parallelism()
                .map(|n| n.get().min(8))
                .unwrap_or(4)
        });
    let mut stats = Stats {
        cases: 0,
        accepted: 0,
        listings: 0,
        listing_bytes: 0,
        dump_bytes: 0,
        by_kind: BTreeMap::new(),
        mism: BTreeMap::new(),
    };
    let files = file_cases();
    let seeds: Vec<Vec<u8>> = files
        .iter()
        .filter(|c| c.src.len() < 20_000)
        .map(|c| c.src.clone())
        .collect();
    let big: Vec<Vec<u8>> = files.iter().map(|c| c.src.clone()).collect();
    let mut dumps = run_cases(files, workers, &mut stats, 400);
    dumps.extend(run_cases(edge_cases(), workers, &mut stats, 400));
    let mut rng = Rng::new(seed);
    if big_budget() {
        let small: Vec<Vec<u8>> = seeds.iter().filter(|s| s.len() <= 3000).cloned().collect();
        for part in all_truncations(&small, 3000).chunks(20_000) {
            run_cases(part.to_vec(), workers, &mut stats, 0);
        }
        // every prefix of the smaller reference dumps
        let small: Vec<Vec<u8>> = dumps.iter().filter(|d| d.len() <= 1500).cloned().collect();
        for part in all_truncations(&small, 1500).chunks(20_000) {
            let part = part
                .iter()
                .map(|c| Case {
                    kind: "binary-truncate-all",
                    binary: true,
                    ..c.clone()
                })
                .collect();
            run_cases(part, workers, &mut stats, 0);
        }
    }
    run_cases(
        binary_cases(&mut rng, &dumps, if big_budget() { 50 } else { 3 }),
        workers,
        &mut stats,
        0,
    );
    let mut done = 0;
    while done < budget {
        let n = (budget - done).min(20_000);
        let more = run_cases(
            generated(&mut rng, n, &seeds, &big),
            workers,
            &mut stats,
            200,
        );
        if !more.is_empty() {
            run_cases(binary_cases(&mut rng, &more, 2), workers, &mut stats, 0);
        }
        done += n;
        if big_budget() {
            eprintln!(
                "  .. {} cases, {} mismatches, {:.0}s",
                stats.cases,
                stats.mism.values().map(Vec::len).sum::<usize>(),
                t0.elapsed().as_secs_f64()
            );
        }
    }
    let total_mism: usize = stats.mism.values().map(Vec::len).sum();
    eprintln!(
        "parser diff ({VER}): {} cases ({} accepted by the reference), {} mismatches, {:.1}s",
        stats.cases,
        stats.accepted,
        total_mism,
        t0.elapsed().as_secs_f64()
    );
    eprintln!(
        "  compared: {} listings ({} bytes), {} dump bytes (stripped + full)",
        stats.listings, stats.listing_bytes, stats.dump_bytes
    );
    for (k, (n, a)) in &stats.by_kind {
        eprintln!("  {k:<15} {n:>8} cases {a:>8} accepted");
    }
    dump_mismatches(
        &stats,
        std::env::var_os("RUSTYNAIL_PARSE_DIFF_MIN").is_some(),
    );
    assert_eq!(total_mism, 0, "parser differential mismatches");
}

#[test]
fn tokenizer_and_mutator_sanity() {
    let t = tokens(b"local x = [==[a]==] .. 'b\\'c' --[[c]] 0x1p4 ... ::l::");
    assert_eq!(t.len(), 11);
    let mut rng = Rng::new(7);
    for _ in 0..200 {
        let _ = mutate(&mut rng, b"local a = {1, 2} return a[1] + f(a)");
        let _ = gen_program(&mut rng);
    }
    let _ = Write::flush(&mut std::io::stderr());
}
