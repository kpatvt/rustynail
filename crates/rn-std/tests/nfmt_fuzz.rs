//! B3-3: differential fuzz of the native number formatting (`rn_core::nfmt`, `number::tostring_float`, `fmt_g`) against
//! libc `snprintf` / `strtod` (the reference's `lua_Number2str` / `tostringbuff`).
//!
//! `NFMT_CASES` (default 400 000) sets the number of cases; the 10M run is
//! `NFMT_CASES=10000000 systemd-run --user --scope -q -p MemoryMax=8G -p MemorySwapMax=0 timeout 900 cargo test -p rn-std --release --test nfmt_fuzz -- --nocapture </dev/null`.
//! `NFMT_THREADS` (default 4), `NFMT_SEED` select the threads and the stream.
#![allow(unsafe_code)]

use rn_core::nfmt;
use rn_core::number;
use rn_std::cfmt;

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        // splitmix64
        self.0 = self.0.wrapping_add(0x9E3779B97F4A7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
        z ^ (z >> 31)
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
    fn chance(&mut self, n: u64) -> bool {
        self.below(n) == 0
    }
}

const SPECIALS: [f64; 24] = [
    0.0,
    -0.0,
    f64::INFINITY,
    f64::NEG_INFINITY,
    f64::NAN,
    -f64::NAN,
    f64::MAX,
    f64::MIN,
    f64::MIN_POSITIVE,
    -f64::MIN_POSITIVE,
    5e-324,
    -5e-324,
    2.225_073_858_507_201e-308, // largest subnormal
    1e15,
    1e16,
    1e17,
    1e21,
    1e22,
    9007199254740992.0,
    9007199254740993.0,
    0.1,
    0.5,
    2.5,
    123456789012345.5,
];

fn gen_f64(r: &mut Rng) -> f64 {
    match r.below(12) {
        0 => f64::from_bits(r.next()),
        1 => {
            // normal, exponent within +-40
            let e = 1023 + r.below(81) as i64 - 40;
            f64::from_bits((r.next() & 0x800F_FFFF_FFFF_FFFF) | ((e as u64) << 52))
        }
        2 => {
            // short decimals
            let k = r.below(9) as i32;
            let v = r.below(2_000_000) as f64 / 10f64.powi(k);
            if r.chance(4) {
                -v
            } else {
                v
            }
        }
        3 => {
            // exact ties in binary: k / 2^m
            let v = r.below(1 << 20) as f64 / (1u64 << r.below(14)) as f64;
            if r.chance(2) {
                -v
            } else {
                v
            }
        }
        4 => {
            // subnormals
            let v = f64::from_bits(r.next() & 0x000F_FFFF_FFFF_FFFF);
            if r.chance(2) {
                -v
            } else {
                v
            }
        }
        5 => SPECIALS[r.below(SPECIALS.len() as u64) as usize],
        6 => {
            // powers of ten and their neighbours
            let k = r.below(641) as i32 - 320;
            let b = 10f64.powi(k.clamp(-307, 308));
            let b = if k < -307 { b * 10f64.powi(k + 307) } else { b };
            let d = r.below(7) as i64 - 3;
            let bits = (b.to_bits() as i64).wrapping_add(d) as u64;
            f64::from_bits(bits)
        }
        7 => {
            // integer-valued
            let v = (r.next() >> r.below(64)) as f64;
            if r.chance(3) {
                -v
            } else {
                v
            }
        }
        8 => {
            // 15..17 digit boundaries: (n + 0.5) / 2^m, exactly representable
            let n = r.below(4_000_000_000_000_000) as f64 + 0.5;
            n / (1u64 << r.below(20)) as f64
        }
        9 => {
            // decimal-looking numbers with 14..17 digits, scaled by a power of ten (not exact: stresses the shortest path)
            let digits = 10u64.pow(13 + r.below(5) as u32);
            let m = r.below(digits * 9) + digits;
            let e = r.below(40) as i32 - 20;
            let v = m as f64 * 10f64.powi(e) / digits as f64;
            if r.chance(4) {
                -v
            } else {
                v
            }
        }
        10 => {
            // sums / products of simple values (the usual 16-17 digit results)
            let a = r.below(1000) as f64 / 10.0;
            let b = r.below(1000) as f64 / 7.0;
            match r.below(3) {
                0 => a + b,
                1 => a * b,
                _ => a / (b + 1.0),
            }
        }
        _ => {
            // 1 ulp neighbours of a short decimal
            let v = r.below(100_000) as f64 / 10f64.powi(r.below(6) as i32);
            let d = r.below(5) as i64 - 2;
            f64::from_bits((v.to_bits() as i64).wrapping_add(d) as u64)
        }
    }
}

fn gen_i64(r: &mut Rng) -> i64 {
    match r.below(6) {
        0 => r.next() as i64,
        1 => (r.next() >> r.below(64)) as i64,
        2 => r.below(2000) as i64 - 1000,
        3 => [
            0,
            1,
            -1,
            i64::MAX,
            i64::MIN,
            i64::MIN + 1,
            255,
            256,
            4095,
            -4096,
        ][r.below(10) as usize],
        4 => 1i64 << r.below(63),
        _ => -((r.next() >> r.below(64)) as i64),
    }
}

fn gen_spec(r: &mut Rng, convs: &[u8], maxprec: u64) -> Vec<u8> {
    let mut f = vec![b'%'];
    for fl in b"-+ 0" {
        if r.chance(4) {
            f.push(*fl);
        }
    }
    if r.chance(2) {
        let w = if r.chance(8) {
            r.below(100)
        } else {
            r.below(20)
        };
        f.extend_from_slice(w.to_string().as_bytes());
    }
    if r.chance(2) {
        let p = if r.chance(10) {
            r.below(maxprec + 1)
        } else {
            r.below(20)
        };
        f.push(b'.');
        if !r.chance(20) {
            f.extend_from_slice(p.to_string().as_bytes());
        }
    }
    f.push(convs[r.below(convs.len() as u64) as usize]);
    f
}

fn c_f64(form: &[u8], x: f64) -> Vec<u8> {
    let mut v = Vec::new();
    assert!(
        cfmt::fmt_f64(&mut v, form, x),
        "snprintf form {:?}",
        String::from_utf8_lossy(form)
    );
    v
}

fn c_i64(form: &[u8], x: i64) -> Vec<u8> {
    let n = form.len() - 1;
    let mut ll = form[..n].to_vec();
    ll.extend_from_slice(b"ll");
    ll.push(form[n]);
    let mut v = Vec::new();
    assert!(cfmt::fmt_i64(&mut v, &ll, x));
    v
}

/// `tostringbuff` as the reference does it, through libc.
fn ref_tostring(x: f64) -> Vec<u8> {
    let mut b = if cfg!(rustynail_lua54) {
        c_f64(b"%.14g", x)
    } else {
        let b = c_f64(b"%.15g", x);
        let mut z = b.clone();
        z.push(0);
        // SAFETY: `z` is NUL-terminated.
        let back = unsafe { libc::strtod(z.as_ptr().cast(), std::ptr::null_mut()) };
        if back == x {
            b
        } else {
            c_f64(b"%.17g", x)
        }
    };
    if b.iter().all(|&c| c == b'-' || c.is_ascii_digit()) {
        b.extend_from_slice(b".0");
    }
    b
}

struct Fails {
    n: usize,
    msgs: Vec<String>,
}

impl Fails {
    fn add(&mut self, m: String) {
        self.n += 1;
        if self.msgs.len() < 20 {
            self.msgs.push(m);
        }
    }
}

fn run(seed: u64, cases: u64) -> Fails {
    let mut r = Rng(seed);
    let mut fails = Fails {
        n: 0,
        msgs: Vec::new(),
    };
    let mut out = Vec::new();
    for _ in 0..cases {
        match r.below(8) {
            0 | 1 => {
                // tostring (float) and fmt_g of the three precisions Lua uses
                let x = gen_f64(&mut r);
                let want = ref_tostring(x);
                let got = number::tostring_float(x);
                if got.as_bytes() != want.as_slice() {
                    fails.add(format!(
                        "tostring {:#018x} {x:e}: got {:?} want {:?}",
                        x.to_bits(),
                        got,
                        String::from_utf8_lossy(&want)
                    ));
                }
                for p in [14usize, 15, 17] {
                    let want = c_f64(format!("%.{p}g").as_bytes(), x);
                    let got = number::fmt_g(x, p);
                    if got.as_bytes() != want.as_slice() {
                        fails.add(format!(
                            "fmt_g {p} {:#018x}: got {:?} want {:?}",
                            x.to_bits(),
                            got,
                            String::from_utf8_lossy(&want)
                        ));
                    }
                }
            }
            2..=5 => {
                let x = gen_f64(&mut r);
                let form = gen_spec(&mut r, b"fFeEgG", 99);
                let sp = nfmt::parse_spec(&form).expect("parse");
                out.clear();
                let want = c_f64(&form, x);
                if !nfmt::format_float(&mut out, &sp, x) {
                    fails.add(format!(
                        "float unhandled {} {:#018x}",
                        String::from_utf8_lossy(&form),
                        x.to_bits()
                    ));
                } else if out != want {
                    fails.add(format!(
                        "float {} {:#018x}: got {:?} want {:?}",
                        String::from_utf8_lossy(&form),
                        x.to_bits(),
                        String::from_utf8_lossy(&out),
                        String::from_utf8_lossy(&want)
                    ));
                }
            }
            _ => {
                let x = gen_i64(&mut r);
                let form = gen_spec(&mut r, b"diuxXo", 60);
                let sp = nfmt::parse_spec(&form).expect("parse");
                out.clear();
                let want = c_i64(&form, x);
                if !nfmt::format_int(&mut out, &sp, x) {
                    fails.add(format!(
                        "int unhandled {} {x}",
                        String::from_utf8_lossy(&form)
                    ));
                } else if out != want {
                    fails.add(format!(
                        "int {} {x}: got {:?} want {:?}",
                        String::from_utf8_lossy(&form),
                        String::from_utf8_lossy(&out),
                        String::from_utf8_lossy(&want)
                    ));
                }
            }
        }
    }
    fails
}

#[test]
fn native_number_formatting_matches_snprintf() {
    let cases: u64 = std::env::var("NFMT_CASES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(400_000);
    let threads: u64 = std::env::var("NFMT_THREADS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(4);
    let seed: u64 = std::env::var("NFMT_SEED")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1);
    let per = cases.div_ceil(threads);
    let results: Vec<Fails> = std::thread::scope(|sc| {
        let hs: Vec<_> = (0..threads)
            .map(|t| sc.spawn(move || run(seed * 1000 + t, per)))
            .collect();
        hs.into_iter().map(|h| h.join().unwrap()).collect()
    });
    let n: usize = results.iter().map(|f| f.n).sum();
    for f in &results {
        for m in &f.msgs {
            eprintln!("MISMATCH {m}");
        }
    }
    eprintln!("nfmt fuzz: {} cases, {} mismatches", per * threads, n);
    assert_eq!(n, 0);
}

#[test]
fn nfmt_edge_cases() {
    // fixed spot checks of round-half-even, zero padding, signs and special values
    let xs = [
        0.5,
        1.5,
        2.5,
        0.125,
        0.375,
        1e300,
        -0.0,
        f64::NAN,
        f64::INFINITY,
        5e-324,
        9.9999995e5,
        99999.95,
        0.00001,
        123456789.0,
    ];
    for form in [
        "%.0f", "%.2f", "%5.1f", "%-8.3f|", "%08.3f", "%+.3e", "% g", "%G", "%.3g", "%10.4g",
        "%.99f", "%.0e", "%e", "%f", "%g",
    ] {
        let form = form.trim_end_matches('|').as_bytes();
        for &x in &xs {
            let sp = nfmt::parse_spec(form).unwrap();
            let mut out = Vec::new();
            assert!(nfmt::format_float(&mut out, &sp, x));
            assert_eq!(
                out,
                c_f64(form, x),
                "{} {x:e}",
                String::from_utf8_lossy(form)
            );
        }
    }
    assert!(nfmt::parse_spec(b"%#x").is_none());
    assert!(nfmt::parse_spec(b"%a").is_none());
}
