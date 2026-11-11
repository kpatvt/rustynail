//! Property: the engines never panic on arbitrary (Lua-controlled) input.
mod common;
use common::Rng;
use rn_std::{format, pack, pattern, strtime, utf8};

fn rand_bytes(r: &mut Rng, max: u64, alphabet: &[u8]) -> Vec<u8> {
    (0..r.below(max))
        .map(|_| {
            if r.chance(1, 3) {
                r.below(256) as u8
            } else {
                alphabet[r.below(alphabet.len() as u64) as usize]
            }
        })
        .collect()
}

struct Src;
impl format::FormatSource for Src {
    type Error = ();
    fn top(&self) -> usize {
        6
    }
    fn check_integer(&mut self, a: usize) -> Result<i64, ()> {
        Ok(a as i64 * 977 - 3000)
    }
    fn check_number(&mut self, a: usize) -> Result<f64, ()> {
        Ok(a as f64 * 1e300)
    }
    fn tolstring(&mut self, _: usize) -> Result<Vec<u8>, ()> {
        Ok(b"str".to_vec())
    }
    fn literal(&mut self, a: usize) -> format::Literal {
        format::Literal::Int(a as i64)
    }
    fn topointer(&mut self, a: usize) -> Option<usize> {
        if a.is_multiple_of(2) {
            None
        } else {
            Some(a)
        }
    }
}

struct PSrc;
impl pack::PackSource for PSrc {
    type Error = ();
    fn int(&mut self, i: usize) -> Result<i64, ()> {
        Ok(i as i64 - 1000)
    }
    fn num(&mut self, i: usize) -> Result<f64, ()> {
        Ok(i as f64)
    }
    fn str(&mut self, i: usize) -> Result<Vec<u8>, ()> {
        Ok(vec![b'x'; i % 7])
    }
}

#[test]
fn engines_never_panic() {
    let mut r = Rng::new(31337);
    for _ in 0..150_000 {
        let s = rand_bytes(&mut r, 24, b"abc 12(),%.-");
        let p = rand_bytes(&mut r, 14, b"%()[]^$*+-?.abfsd1");
        let init = r.range(-30, 30);
        let _ = pattern::find_aux(&s, &p, init, r.chance(1, 2), r.chance(1, 4));
        let mut g = pattern::GMatch::new(&s, &p, init);
        for _ in 0..30 {
            match g.next_match() {
                Ok(Some(_)) => {}
                _ => break,
            }
        }
        let mut cb = |m: &pattern::Matched| -> pattern::PResult<pattern::ReplValue> {
            let _ = m.all()?;
            let _ = m.first()?;
            Ok(pattern::ReplValue::Keep)
        };
        let _ = pattern::gsub(
            &s,
            &p,
            pattern::Replacement::Callback(&mut cb),
            r.range(-2, 30),
        );
        let _ = pattern::gsub(
            &s,
            &p,
            pattern::Replacement::Str(&rand_bytes(&mut r, 6, b"%0129x")),
            100,
        );

        let f = rand_bytes(&mut r, 14, b"%-+ #0123456789.dsqcxXeEfFgGaApuio");
        let _ = format::format(&f, &mut Src);
        let pf = rand_bytes(&mut r, 12, b"<>=!bBhHlLjJTfdnzxXisc0123456789 ");
        let _ = pack::pack(&pf, &mut PSrc);
        let _ = pack::packsize(&pf);
        let _ = pack::unpack(&pf, &s, r.range(-40, 40));

        let _ = utf8::len(&s, r.range(-30, 30), r.range(-30, 30), r.chance(1, 2));
        let _ = utf8::codepoint(&s, r.range(-30, 30), Some(r.range(-30, 30)), r.chance(1, 2));
        let _ = utf8::offset(&s, r.range(-30, 30), Some(r.range(-30, 30)));
        let _ = utf8::offset(&s, i64::MIN, None);
        let _ = utf8::len(&s, i64::MIN, i64::MAX, false);
        let _ = utf8::codes_next(&s, r.range(-5, 40), r.chance(1, 2));
        let _ = utf8::char_from(&[r.next() as i64, r.range(-3, 0x7fff_ffff)]);

        let fmt = rand_bytes(&mut r, 8, b"%YmdHMSaAbBcCDeFgGhIjnprRtTuUVwWxXyzZE O!*t");
        let _ = strtime::date(&fmt, r.range(-100_000_000_000, 100_000_000_000));
    }
    // extreme arguments
    for t in [i64::MIN, i64::MAX, 0, -1] {
        let _ = strtime::date(b"%c", t);
        let _ = strtime::date(b"*t", t);
    }
    let _ = pack::unpack(b"i16", &[0xff; 16], i64::MIN);
    let _ = pack::packsize(b"c9223372036854775807c9223372036854775807");
    let _ = pattern::find_aux(b"abc", b"b", i64::MAX, true, false);
    let _ = pattern::find_aux(b"abc", b"b", i64::MIN, true, false);
}
