mod common;
use common::*;
use rn_core::number::tostring_float;
use rn_std::format::*;

const RUNNER: &str = r##"
local function unhex(s) return (s:gsub("%x%x", function(h) return string.char(tonumber(h, 16)) end)) end
local function hex(s) return (s:gsub(".", function(c) return string.format("%02x", c:byte()) end)) end
local function err(m) return "E" .. hex((m:gsub("(bad argument #%d+) to '[^']*'", "%1 to '?'"))) end
local TBL = setmetatable({}, {__tostring = function() return "tbl" end})
for line in io.lines(arg[1]) do
  local f = {}
  for x in (line .. "\t"):gmatch("(.-)\t") do f[#f+1] = x end
  local format = unhex(f[1])
  local args, n = {}, 0
  for i = 2, #f do
    n = n + 1
    local k, v = f[i]:sub(1, 1), f[i]:sub(2)
    if k == "i" then args[n] = math.tointeger(v)
    elseif k == "d" then args[n] = string.unpack("<d", string.pack("<i8", math.tointeger("0x" .. v)))
    elseif k == "s" then args[n] = unhex(v)
    elseif k == "b" then args[n] = (v == "1")
    elseif k == "t" then args[n] = TBL
    else args[n] = nil end
  end
  local r = table.pack(pcall(string.format, format, table.unpack(args, 1, n)))
  if r[1] then print("s" .. hex(r[2])) else print(err(r[2])) end
end
"##;

#[derive(Clone)]
enum A {
    I(i64),
    D(f64),
    S(Vec<u8>),
    B(bool),
    T,
    Nil,
}

struct Src {
    args: Vec<A>,
}

impl Src {
    fn get(&self, arg: usize) -> &A {
        &self.args[arg - 2]
    }
}

fn tname(a: &A) -> &'static str {
    match a {
        A::I(_) | A::D(_) => "number",
        A::S(_) => "string",
        A::B(_) => "boolean",
        A::T => "table",
        A::Nil => "nil",
    }
}

impl FormatSource for Src {
    type Error = String;
    fn top(&self) -> usize {
        self.args.len() + 1
    }
    fn check_integer(&mut self, arg: usize) -> Result<i64, String> {
        match self.get(arg) {
            A::I(i) => Ok(*i),
            A::D(d) => {
                if *d >= -9223372036854775808.0 && *d < 9223372036854775808.0 && d.floor() == *d {
                    Ok(*d as i64)
                } else {
                    Err(format!(
                        "bad argument #{arg} to '?' (number has no integer representation)"
                    ))
                }
            }
            a => Err(format!(
                "bad argument #{arg} to '?' (number expected, got {})",
                tname(a)
            )),
        }
    }
    fn check_number(&mut self, arg: usize) -> Result<f64, String> {
        match self.get(arg) {
            A::I(i) => Ok(*i as f64),
            A::D(d) => Ok(*d),
            a => Err(format!(
                "bad argument #{arg} to '?' (number expected, got {})",
                tname(a)
            )),
        }
    }
    fn tolstring(&mut self, arg: usize) -> Result<Vec<u8>, String> {
        Ok(match self.get(arg) {
            A::I(i) => i.to_string().into_bytes(),
            A::D(d) => tostring_float(*d).as_bytes().to_vec(),
            A::S(s) => s.clone(),
            A::B(b) => b.to_string().into_bytes(),
            A::T => b"tbl".to_vec(),
            A::Nil => b"nil".to_vec(),
        })
    }
    fn literal(&mut self, arg: usize) -> Literal {
        match self.get(arg) {
            A::I(i) => Literal::Int(*i),
            A::D(d) => Literal::Float(*d),
            A::S(s) => Literal::Str(s.clone()),
            A::B(b) => Literal::Plain(b.to_string().into_bytes()),
            A::T => Literal::NoLiteral,
            A::Nil => Literal::Plain(b"nil".to_vec()),
        }
    }
    fn topointer(&mut self, arg: usize) -> Option<usize> {
        match self.get(arg) {
            A::T | A::S(_) => Some(0x1000), // not differential-tested (addresses differ)
            _ => None,
        }
    }
}

const INTS: &[i64] = &[
    0,
    1,
    -1,
    7,
    42,
    -42,
    255,
    256,
    65,
    97,
    127,
    128,
    1000,
    -1000,
    123456789,
    i32::MAX as i64,
    i32::MIN as i64,
    i32::MAX as i64 + 1,
    u32::MAX as i64,
    i64::MAX,
    i64::MIN,
    i64::MIN + 1,
    1 << 40,
];

fn rand_float(r: &mut Rng) -> f64 {
    match r.below(14) {
        0 => 0.0,
        1 => -0.0,
        2 => f64::INFINITY,
        3 => f64::NEG_INFINITY,
        4 => f64::NAN,
        5 => 0.1,
        6 => 1.5,
        7 => -2.5e-5,
        8 => 123_456_789.123_456_79,
        9 => f64::from_bits(r.next()),
        10 => f64::MAX,
        11 => 5e-324,
        12 => (r.range(-100000, 100000) as f64) / 8.0,
        _ => (r.next() as f64) * 10f64.powi(r.range(-30, 30) as i32),
    }
}

fn rand_spec(r: &mut Rng, conv: char) -> String {
    let mut s = String::from("%");
    if r.chance(1, 2) {
        for _ in 0..r.below(4) {
            s.push(*r.pick(&['-', '+', ' ', '#', '0']));
        }
    }
    if r.chance(1, 2) {
        let w = r.below(120);
        s.push_str(&w.to_string());
    }
    if r.chance(1, 2) {
        s.push('.');
        if r.chance(7, 8) {
            s.push_str(&r.below(110).to_string());
        }
    }
    if r.chance(1, 40) {
        s.push_str(r.pick(&["l", "h", "ll", "L", "5$", "*", "$"]));
    }
    s.push(conv);
    s
}

fn gen(r: &mut Rng) -> (String, Vec<A>) {
    let mut f = String::new();
    let mut args = Vec::new();
    for _ in 0..r.below(4) + 1 {
        if r.chance(1, 4) {
            f.push_str(r.pick(&["abc", " ", "%%", "x=", "\u{e9}", "\n"]));
        }
        let conv = *r.pick(&[
            'c', 'd', 'i', 'u', 'o', 'x', 'X', 'a', 'A', 'e', 'E', 'f', 'F', 'g', 'G', 'q', 's',
            'p', 'd', 'd', 's', 'g', 'f', 'x', 'y', 'n', 'l',
        ]);
        f.push_str(&if r.chance(1, 6) {
            format!("%{conv}")
        } else {
            rand_spec(r, conv)
        });
        // argument consistent with the conversion (mostly)
        let a = if r.chance(1, 12) {
            match r.below(5) {
                0 => A::Nil,
                1 => A::B(r.chance(1, 2)),
                2 => A::T,
                3 => A::S(b"xyz".to_vec()),
                _ => A::D(r.range(-5, 5) as f64 + 0.5),
            }
        } else {
            match conv {
                'c' | 'd' | 'i' | 'u' | 'o' | 'x' | 'X' => {
                    if r.chance(1, 10) {
                        A::D(r.range(-100, 100) as f64)
                    } else {
                        A::I(*r.pick(INTS))
                    }
                }
                'a' | 'A' | 'e' | 'E' | 'f' | 'F' | 'g' | 'G' => {
                    if r.chance(1, 8) {
                        A::I(*r.pick(INTS))
                    } else {
                        A::D(rand_float(r))
                    }
                }
                'q' => match r.below(6) {
                    0 => A::I(*r.pick(INTS)),
                    1 => A::D(rand_float(r)),
                    2 => A::Nil,
                    3 => A::B(r.chance(1, 2)),
                    _ => A::S(
                        (0..r.below(12))
                            .map(|_| {
                                *r.pick(&[
                                    b'a', b'1', b'"', b'\\', b'\n', 0, 1, 13, 9, 127, 200, b'9',
                                    0x1f,
                                ])
                            })
                            .collect(),
                    ),
                },
                'p' => match r.below(3) {
                    0 => A::Nil,
                    1 => A::I(5),
                    _ => A::B(true),
                },
                's' => match r.below(7) {
                    0 => A::I(*r.pick(INTS)),
                    1 => A::D(rand_float(r)),
                    2 => A::Nil,
                    3 => A::T,
                    4 => {
                        let n = *r.pick(&[0u64, 3, 10, 99, 100, 150]);
                        A::S((0..n).map(|_| b'a' + r.below(26) as u8).collect())
                    }
                    5 => A::S(vec![b'a', 0, b'b']),
                    _ => A::B(false),
                },
                _ => A::I(1),
            }
        };
        args.push(a);
    }
    if r.chance(1, 20) {
        args.pop();
    }
    (f, args)
}

fn enc(a: &A) -> String {
    match a {
        A::I(i) => format!("i{i}"),
        A::D(d) => format!("d{:016x}", d.to_bits()),
        A::S(s) => format!("s{}", hex(s)),
        A::B(b) => format!("b{}", *b as u8),
        A::T => "t".into(),
        A::Nil => "n".into(),
    }
}

#[test]
fn format_differential() {
    let mut r = Rng::new(0xF00D);
    let mut lines = Vec::new();
    let mut ours = Vec::new();
    for _ in 0..200_000 {
        let (f, args) = gen(&mut r);
        let mut l = hex(f.as_bytes());
        for a in &args {
            l.push('\t');
            l.push_str(&enc(a));
        }
        lines.push(l);
        let mut src = Src { args };
        ours.push(match format(f.as_bytes(), &mut src) {
            Ok(b) => format!("s{}", hex(&b)),
            Err(FormatError::Msg(m)) => format!("E{}", hex(&m)),
            Err(FormatError::Arg(n, m)) => format!(
                "E{}",
                hex(format!("bad argument #{n} to '?' ({m})").as_bytes())
            ),
            Err(FormatError::Src(m)) => format!("E{}", hex(m.as_bytes())),
        });
    }
    let data = lines.join("\n") + "\n";
    let Some(reference) = run_ref("format", false, RUNNER, &data, &[]) else {
        return;
    };
    // %p of strings/tables prints real addresses: those cases are excluded from the comparison
    let mut c2 = Vec::new();
    let mut r2 = Vec::new();
    let mut o2 = Vec::new();
    for i in 0..lines.len() {
        let has_p_ptr = String::from_utf8_lossy(&unhex(lines[i].split('\t').next().unwrap()))
            .contains('p')
            && (lines[i].contains("\tt") || lines[i].contains("\ts"));
        if has_p_ptr && reference[i] != ours[i] {
            continue;
        }
        c2.push(lines[i].clone());
        r2.push(reference[i].clone());
        o2.push(ours[i].clone());
    }
    eprintln!(
        "format: {} of {} cases compared ({} skipped as address-dependent)",
        c2.len(),
        lines.len(),
        lines.len() - c2.len()
    );
    compare("format", &c2, &r2, &o2);
}
