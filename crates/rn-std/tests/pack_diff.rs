mod common;
use common::*;
use rn_std::pack::*;

const RUNNER: &str = r##"
local function unhex(s) return (s:gsub("%x%x", function(h) return string.char(tonumber(h, 16)) end)) end
local function hex(s) return (s:gsub(".", function(c) return string.format("%02x", c:byte()) end)) end
local function bits(x) return ("%016x"):format((string.unpack("<i8", string.pack("<d", x)))) end
local function err(m) return "E" .. hex((m:gsub("to '[^']*'", "to '?'"))) end
local function fmt1(v)
  if math.type(v) == "integer" then return "i" .. v
  elseif math.type(v) == "float" then return "d" .. bits(v)
  else return "s" .. hex(v) end
end
for line in io.lines(arg[1]) do
  local f = {}
  for x in (line .. "\t"):gmatch("(.-)\t") do f[#f+1] = x end
  local op = f[1]
  local out
  local format = unhex(f[2])
  if op == "pack" then
    local args = {}
    for i = 3, #f do
      local k, v = f[i]:sub(1, 1), f[i]:sub(2)
      if k == "i" then args[#args+1] = math.tointeger(v)
      elseif k == "d" then args[#args+1] = string.unpack("<d", string.pack("<i8", tonumber(v, 16) | 0))
      else args[#args+1] = unhex(v) end
    end
    local r = table.pack(pcall(string.pack, format, table.unpack(args)))
    if r[1] then out = "s" .. hex(r[2]) else out = err(r[2]) end
  elseif op == "packsize" then
    local r = table.pack(pcall(string.packsize, format))
    if r[1] then out = "i" .. r[2] else out = err(r[2]) end
  else
    local r = table.pack(pcall(string.unpack, format, unhex(f[3]), tonumber(f[4])))
    if r[1] then
      local o = {}
      for i = 2, r.n do o[#o+1] = fmt1(r[i]) end
      out = table.concat(o, " ")
    else out = err(r[2]) end
  end
  print(out)
end
"##;

enum A {
    I(i64),
    D(f64),
    S(Vec<u8>),
}
struct Src {
    args: Vec<A>,
}
impl Src {
    fn bad(&self, idx: usize, expected: &str) -> String {
        let got = match self.args.get(idx - 2) {
            Some(A::I(_)) | Some(A::D(_)) => "number",
            Some(A::S(_)) => "string",
            None if idx == self.args.len() + 2 => "nil",
            None => "no value",
        };
        format!("bad argument #{idx} to '?' ({expected} expected, got {got})")
    }
}
impl PackSource for Src {
    type Error = String;
    fn int(&mut self, idx: usize) -> Result<i64, String> {
        match self.args.get(idx - 2) {
            Some(A::I(i)) => Ok(*i),
            Some(A::D(d)) => {
                if *d >= -9223372036854775808.0 && *d < 9223372036854775808.0 && d.floor() == *d {
                    Ok(*d as i64)
                } else {
                    Err(format!(
                        "bad argument #{idx} to '?' (number has no integer representation)"
                    ))
                }
            }
            _ => Err(self.bad(idx, "number")),
        }
    }
    fn num(&mut self, idx: usize) -> Result<f64, String> {
        match self.args.get(idx - 2) {
            Some(A::D(d)) => Ok(*d),
            Some(A::I(i)) => Ok(*i as f64),
            _ => Err(self.bad(idx, "number")),
        }
    }
    fn str(&mut self, idx: usize) -> Result<Vec<u8>, String> {
        match self.args.get(idx - 2) {
            Some(A::S(s)) => Ok(s.clone()),
            Some(A::I(i)) => Ok(i.to_string().into_bytes()),
            Some(A::D(d)) => Ok(rn_core::number::tostring_float(*d).as_bytes().to_vec()),
            None => Err(self.bad(idx, "string")),
        }
    }
}

fn e<X: std::fmt::Display>(x: PackError<X>) -> String {
    let m = match x {
        PackError::Arg(n, m) => format!("bad argument #{n} to '?' ({m})").into_bytes(),
        PackError::Msg(m) => m,
        PackError::Src(m) => m.to_string().into_bytes(),
    };
    format!("E{}", hex(&m))
}

fn fmt_unpacked(v: &Unpacked) -> String {
    match v {
        Unpacked::Int(i) => format!("i{i}"),
        Unpacked::Num(d) => format!("d{:016x}", d.to_bits()),
        Unpacked::Str(s) => format!("s{}", hex(s)),
    }
}

const INTS: &[i64] = &[
    0,
    1,
    -1,
    2,
    127,
    128,
    255,
    256,
    -128,
    -129,
    32767,
    32768,
    65535,
    65536,
    -32768,
    -32769,
    i32::MAX as i64,
    i32::MAX as i64 + 1,
    u32::MAX as i64,
    i64::MAX,
    i64::MIN,
    1 << 40,
    -(1 << 40),
    1000,
    42,
];
const FLOATS: &[f64] = &[
    0.0,
    -0.0,
    1.5,
    -2.25,
    1e10,
    3.4e38,
    1e300,
    f64::INFINITY,
    f64::NEG_INFINITY,
    0.1,
    5e-324,
    16777217.0,
];

fn gen_pack(r: &mut Rng) -> (String, Vec<A>) {
    let mut f = String::new();
    let mut args = Vec::new();
    for _ in 0..r.below(6) {
        match r.below(24) {
            0 => {
                f.push_str(r.pick(&["<", ">", "=", "!", "!1", "!2", "!4", "!8", "!16", "!3", " "]))
            }
            1 => f.push('x'),
            2 => {
                f.push_str(r.pick(&["Xi", "Xb", "Xh", "Xi4", "Xd", "X", "Xc1", "Xz", "XI3", "Xj"]))
            }
            3..=9 => {
                let o = r.pick(&[
                    "b", "B", "h", "H", "l", "L", "j", "J", "T", "i", "I", "i1", "i2", "i3", "i5",
                    "i7", "i8", "i9", "i16", "I1", "I2", "I3", "I4", "I8", "I12", "I16", "i17",
                    "i0", "I0",
                ]);
                f.push_str(o);
                args.push(A::I(*r.pick(INTS)));
            }
            10..=12 => {
                f.push_str(r.pick(&["f", "d", "n"]));
                args.push(A::D(*r.pick(FLOATS)));
            }
            13 | 14 => {
                f.push_str(r.pick(&["c0", "c1", "c2", "c5", "c10", "c", "c20"]));
                let n = r.below(8) as usize;
                args.push(A::S((0..n).map(|_| b'a' + r.below(26) as u8).collect()));
            }
            15..=17 => {
                f.push_str(r.pick(&["s", "s1", "s2", "s4", "s8", "s16", "s0", "s3"]));
                let n = r.below(300) as usize;
                args.push(A::S(vec![b'q'; if r.chance(1, 3) { n } else { n % 5 }]));
            }
            18 | 19 => {
                f.push('z');
                let mut s: Vec<u8> = (0..r.below(6)).map(|_| b'a' + r.below(26) as u8).collect();
                if r.chance(1, 8) {
                    s.push(0);
                }
                args.push(A::S(s));
            }
            20 => {
                // (5.4 would try to pad 2 GB byte by byte: far too slow to be a useful test)
                let huge = if cfg!(rustynail_lua54) {
                    "c99"
                } else {
                    "c99999999999999999999"
                };
                f.push_str(r.pick(&["q", "!x", "i99999999999", huge, "s17"]))
            }
            21 => f.push_str(r.pick(&["Zi", "?", "%"])),
            _ => {
                f.push_str("i4");
                args.push(A::I(*r.pick(INTS)));
            }
        }
    }
    (f, args)
}

fn gen_unpack(r: &mut Rng) -> (String, Vec<u8>, i64) {
    let (f, _) = gen_pack(r);
    let n = r.below(30) as usize;
    let data: Vec<u8> = (0..n)
        .map(|_| match r.below(5) {
            0 => 0,
            1 => 0xff,
            2 => 1 + r.below(3) as u8,
            _ => r.below(256) as u8,
        })
        .collect();
    let init = match r.below(6) {
        0 => r.range(-35, 35),
        1 => n as i64 + 1,
        2 => n as i64 + 2,
        _ => 1,
    };
    (f, data, init)
}

#[test]
fn pack_differential() {
    let mut r = Rng::new(0xABCD);
    let mut lines = Vec::new();
    let mut ours = Vec::new();
    for i in 0..60_000 {
        match i % 3 {
            0 => {
                let (f, args) = gen_pack(&mut r);
                let mut l = format!("pack\t{}", hex(f.as_bytes()));
                for a in &args {
                    match a {
                        A::I(v) => l.push_str(&format!("\ti{v}")),
                        A::D(d) => l.push_str(&format!("\td{:016x}", d.to_bits())),
                        A::S(s) => l.push_str(&format!("\ts{}", hex(s))),
                    }
                }
                lines.push(l);
                let mut src = Src { args };
                ours.push(match pack(f.as_bytes(), &mut src) {
                    Ok(b) => format!("s{}", hex(&b)),
                    Err(x) => e(x),
                });
            }
            1 => {
                let (f, _) = gen_pack(&mut r);
                lines.push(format!("packsize\t{}", hex(f.as_bytes())));
                ours.push(match packsize(f.as_bytes()) {
                    Ok(n) => format!("i{n}"),
                    Err(x) => e(x),
                });
            }
            _ => {
                let (f, data, init) = gen_unpack(&mut r);
                lines.push(format!(
                    "unpack\t{}\t{}\t{}",
                    hex(f.as_bytes()),
                    hex(&data),
                    init
                ));
                ours.push(match unpack(f.as_bytes(), &data, init) {
                    Ok((vals, next)) => {
                        let mut o: Vec<String> = vals.iter().map(fmt_unpacked).collect();
                        o.push(format!("i{next}"));
                        o.join(" ")
                    }
                    Err(x) => e(x),
                });
            }
        }
    }
    let data = lines.join("\n") + "\n";
    let Some(reference) = run_ref("pack", false, RUNNER, &data, &[]) else {
        return;
    };
    compare("pack", &lines, &reference, &ours);
}
