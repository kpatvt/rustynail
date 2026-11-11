//! Differential tests of `number::{str2num, tostring}` against the reference `lua` binary
//! (`ref/55/build/lua`, or `ref/54/build/lua` under `--cfg rustynail_lua54`). Skipped with a notice
//! when the binary is missing.
use rn_core::number::{str2num, tostring, Number};
use std::io::Write;
use std::path::PathBuf;
use std::process::Command;

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        // xorshift64*
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545F4914F6CDD1D)
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
    fn pick<'a>(&mut self, xs: &[&'a str]) -> &'a str {
        xs[self.below(xs.len() as u64) as usize]
    }
}

fn ref_lua() -> Option<PathBuf> {
    let v = if cfg!(rustynail_lua54) { "54" } else { "55" };
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!("../../ref/{v}/build/lua"));
    if p.exists() {
        Some(p)
    } else {
        eprintln!(
            "NOTICE: reference binary {} missing; skipping differential test",
            p.display()
        );
        None
    }
}

const DRIVER: &str = r#"
local out = {}
for line in io.lines(arg[1]) do
  local kind, hex = line:match("^(%a) (%x*)$")
  local s = hex:gsub("%x%x", function(h) return string.char(tonumber(h, 16)) end)
  if kind == "s" then
    local v = tonumber(s)
    if v == nil then out[#out+1] = "F"
    elseif math.type(v) == "integer" then out[#out+1] = "I " .. string.format("%d", v)
    else out[#out+1] = "D " .. string.format("%016x", string.unpack("<I8", string.pack("<d", v))) end
  elseif kind == "d" then
    out[#out+1] = tostring((string.unpack("<d", s)))
  else
    out[#out+1] = tostring((string.unpack("<i8", s)))
  end
  if #out >= 1000 then io.write(table.concat(out, "\n"), "\n"); out = {} end
end
io.write(table.concat(out, "\n"), "\n")
"#;

fn hex(b: &[u8]) -> String {
    b.iter().map(|c| format!("{c:02x}")).collect()
}

/// Runs the driver over `lines` (already formatted "k hex") and returns one output line per input.
fn run_ref(bin: &PathBuf, lines: &[String], tag: &str) -> Vec<String> {
    let dir = std::env::temp_dir().join(format!("rustynail-numdiff-{}-{tag}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let drv = dir.join("driver.lua");
    let inp = dir.join("input.txt");
    std::fs::write(&drv, DRIVER).unwrap();
    let mut f = std::io::BufWriter::new(std::fs::File::create(&inp).unwrap());
    for l in lines {
        writeln!(f, "{l}").unwrap();
    }
    drop(f);
    let out = Command::new(bin)
        .arg(&drv)
        .arg(&inp)
        .output()
        .expect("run reference lua");
    assert!(
        out.status.success(),
        "ref lua failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let _ = std::fs::remove_dir_all(&dir);
    let text = String::from_utf8(out.stdout).unwrap();
    let mut v: Vec<String> = text.split('\n').map(str::to_string).collect();
    v.pop(); // trailing newline artefact
    v
}

fn gen_numeral(r: &mut Rng) -> Vec<u8> {
    let ws = ["", " ", "  ", "\t", "\n", "\x0b", "\x0c", "\r", " \t "];
    let sg = ["", "", "-", "+"];
    let mut s = String::new();
    s += r.pick(&ws);
    s += r.pick(&sg);
    match r.below(12) {
        0 => s += &(r.next() as i64).to_string(),
        1 => {
            // near +-2^63
            let d = r.below(2000) as i128 - 1000;
            s += &(((1i128 << 63) + d).abs()).to_string();
        }
        2 => s += &format!("0x{:x}", r.next() >> r.below(64)),
        3 => s += &format!("0X{:X}", r.next()),
        4 => {
            // hex digits beyond 64 bit
            s += "0x";
            for _ in 0..(r.below(24) + 1) {
                s.push(char::from_digit(r.below(16) as u32, 16).unwrap());
            }
        }
        5 => {
            // hex float
            s += "0x";
            let n = r.below(20);
            for _ in 0..n {
                s.push(char::from_digit(r.below(16) as u32, 16).unwrap());
            }
            if r.below(3) > 0 {
                s.push('.');
                for _ in 0..r.below(20) {
                    s.push(char::from_digit(r.below(16) as u32, 16).unwrap());
                }
            }
            if r.below(3) > 0 {
                s += r.pick(&["p", "P"]);
                s += r.pick(&["", "+", "-"]);
                s += &(r.below(1200)).to_string();
            }
        }
        6 => {
            // double from random bits, many digits
            let d = f64::from_bits(r.next());
            if d.is_finite() {
                s += &format!("{:.*e}", r.below(25) as usize, d);
            } else {
                s += "1e5";
            }
        }
        7 => {
            let d = f64::from_bits(r.next());
            if d.is_finite() {
                s += &format!("{d}");
            } else {
                s += "0.5";
            }
        }
        8 => {
            // decimal with fraction / exponent
            s += &r.below(1_000_000_000).to_string();
            if r.below(2) == 0 {
                s.push('.');
                s += &r.below(1_000_000_000_000).to_string();
            }
            if r.below(2) == 0 {
                s += r.pick(&["e", "E"]);
                s += r.pick(&["", "+", "-"]);
                s += &r.below(400).to_string();
            }
        }
        9 => {
            // junk
            let junk = [
                "", ".", "e", "0x", "0x.", "1e", "1e+", "inf", "nan", "-inf", "0x1p", "1..2",
                "--1", "1 1", "0b1", "1f", "١", "∞", "1_0", "0x1.2.3", "infinity", "NaN", ".e1",
                "0xp1", "1e1.5", "5.e1", ".5e-1", "0x.1p1",
            ];
            s += r.pick(&junk);
        }
        10 => {
            // long digit strings
            for _ in 0..r.below(60) + 1 {
                s.push(char::from_digit(r.below(10) as u32, 10).unwrap());
            }
            if r.below(2) == 0 {
                s += ".";
                for _ in 0..r.below(40) {
                    s.push(char::from_digit(r.below(10) as u32, 10).unwrap());
                }
            }
        }
        _ => {
            // subnormal / huge boundaries
            s += r.pick(&[
                "4.9e-324",
                "2.4703282292062327e-324",
                "2.4703282292062328e-324",
                "1.7976931348623157e308",
                "1.7976931348623159e308",
                "1e308",
                "1e309",
                "1e-400",
                "0x1p-1074",
                "0x1p-1075",
                "0x1.8p-1075",
                "0x1.fffffffffffffp1023",
                "0x1.fffffffffffff8p1023",
                "0x.0000000000000000001p-1000",
                "9007199254740993",
                "9007199254740993.0",
                "0.1e1",
                "00012",
                "0e0",
                "-0.0",
                "0x0.0",
                "1e0000000000005",
            ]);
        }
    }
    s += r.pick(&ws);
    let mut b = s.into_bytes();
    if r.below(200) == 0 {
        let at = r.below(b.len() as u64 + 1) as usize;
        b.insert(at, 0);
    }
    b
}

fn show(n: Option<Number>) -> String {
    match n {
        None => "F".into(),
        Some(Number::Int(i)) => format!("I {i}"),
        Some(Number::Float(f)) => format!("D {:016x}", f.to_bits()),
    }
}

#[test]
#[cfg_attr(
    miri,
    ignore = "spawns the reference interpreter (number.rs is safe code)"
)]
fn str2num_matches_reference() {
    let Some(bin) = ref_lua() else { return };
    let mut r = Rng(0x1234_5678_9abc_def1);
    let mut corpus: Vec<Vec<u8>> = (0..60_000).map(|_| gen_numeral(&mut r)).collect();
    for i in [0i64, 1, -1, i64::MAX, i64::MIN, 255] {
        corpus.push(i.to_string().into_bytes());
    }
    let lines: Vec<String> = corpus.iter().map(|b| format!("s {}", hex(b))).collect();
    let want = run_ref(&bin, &lines, "s");
    assert_eq!(want.len(), corpus.len());
    let mut bad = 0;
    for (b, w) in corpus.iter().zip(&want) {
        let g = show(str2num(b));
        if &g != w {
            bad += 1;
            if bad < 20 {
                eprintln!(
                    "MISMATCH {:?}: ours {g}, ref {w}",
                    String::from_utf8_lossy(b)
                );
            }
        }
    }
    assert_eq!(bad, 0, "{bad} mismatches out of {}", corpus.len());
    let valid = want.iter().filter(|w| *w != "F").count();
    let floats = want.iter().filter(|w| w.starts_with('D')).count();
    assert!(valid > 20_000 && floats > 10_000, "corpus too degenerate");
    eprintln!(
        "str2num: {} strings compared ({valid} valid, {floats} floats)",
        corpus.len()
    );
}

#[test]
#[cfg_attr(
    miri,
    ignore = "spawns the reference interpreter (number.rs is safe code)"
)]
fn tostring_matches_reference() {
    let Some(bin) = ref_lua() else { return };
    let mut r = Rng(0x9e37_79b9_7f4a_7c15);
    let mut lines = Vec::new();
    let mut vals = Vec::new();
    let specials = [
        0.0f64,
        -0.0,
        1.0,
        -1.0,
        0.1,
        1e15,
        1e16,
        1e100,
        1e-5,
        123456789012345.0,
        2f64.powi(53),
        2f64.powi(63),
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NAN,
        -f64::NAN,
        f64::MIN_POSITIVE,
        5e-324,
        f64::MAX,
        3.2,
        1.0 / 3.0,
        100.0,
        1e14,
        99999999999999.9,
    ];
    for d in specials {
        vals.push(Number::Float(d));
    }
    for i in [0i64, -1, i64::MAX, i64::MIN] {
        vals.push(Number::Int(i));
    }
    for k in 0..220_000 {
        match k % 5 {
            0 => vals.push(Number::Float(f64::from_bits(r.next()))),
            1 => vals.push(Number::Float((r.next() as i64 as f64) / 1000.0)),
            2 => vals.push(Number::Float(
                r.below(1 << 40) as f64 * 10f64.powi(r.below(40) as i32 - 20),
            )),
            3 => vals.push(Number::Float(
                (r.below(2_000_000) as f64 - 1_000_000.0) / 100.0,
            )),
            _ => vals.push(Number::Int(r.next() as i64 >> r.below(64))),
        }
    }
    for v in &vals {
        match v {
            Number::Float(f) => lines.push(format!("d {}", hex(&f.to_le_bytes()))),
            Number::Int(i) => lines.push(format!("i {}", hex(&i.to_le_bytes()))),
        }
    }
    let want = run_ref(&bin, &lines, "t");
    assert_eq!(want.len(), vals.len());
    let mut bad = 0;
    for (v, w) in vals.iter().zip(&want) {
        let g = String::from_utf8_lossy(tostring(*v).as_bytes()).into_owned();
        // the reference prints NaN as "nan" or "-nan" depending on the sign bit, as we do
        if &g != w {
            bad += 1;
            if bad < 20 {
                eprintln!("MISMATCH {v:?}: ours {g}, ref {w}");
            }
        }
    }
    assert_eq!(bad, 0, "{bad} mismatches out of {}", vals.len());
    eprintln!("tostring: {} numbers compared", vals.len());
}
