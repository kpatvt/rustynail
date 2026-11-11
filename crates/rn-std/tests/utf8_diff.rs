mod common;
use common::*;
use rn_std::utf8::*;

const RUNNER: &str = r##"
local function unhex(s) return (s:gsub("%x%x", function(h) return string.char(tonumber(h, 16)) end)) end
local function hex(s) return (s:gsub(".", function(c) return string.format("%02x", c:byte()) end)) end
local function fmt(t)
  local o = {}
  for i = 1, t.n do
    local v = t[i]
    if v == nil then o[#o+1] = "n"
    elseif math.type(v) then o[#o+1] = "i" .. tostring(v)
    else o[#o+1] = "s" .. hex(v) end
  end
  return table.concat(o, " ")
end
local function err(m) return "E" .. hex((m:gsub("to '[^']*'", "to '?'"))) end
local function num(x) if x == "-" then return nil end return tonumber(x) end
for line in io.lines(arg[1]) do
  local f = {}
  for x in (line .. "\t"):gmatch("(.-)\t") do f[#f+1] = x end
  local op = f[1]
  local out
  if op == "len" then
    local r = table.pack(pcall(utf8.len, unhex(f[2]), num(f[3]), num(f[4]), f[5] == "1"))
    if r[1] then out = fmt({n = r.n - 1, table.unpack(r, 2, r.n)}) else out = err(r[2]) end
  elseif op == "codepoint" then
    local r = table.pack(pcall(utf8.codepoint, unhex(f[2]), num(f[3]), num(f[4]), f[5] == "1"))
    if r[1] then out = fmt({n = r.n - 1, table.unpack(r, 2, r.n)}) else out = err(r[2]) end
  elseif op == "char" then
    local args = {}
    for i = 2, #f do args[#args+1] = tonumber(f[i]) end
    local r = table.pack(pcall(utf8.char, table.unpack(args)))
    if r[1] then out = "s" .. hex(r[2]) else out = err(r[2]) end
  elseif op == "offset" then
    local r = table.pack(pcall(utf8.offset, unhex(f[2]), tonumber(f[3]), num(f[4])))
    if r[1] then out = fmt({n = r.n - 1, table.unpack(r, 2, r.n)}) else out = err(r[2]) end
  else -- codes
    local s = unhex(f[2])
    local ok, a, b, c = pcall(utf8.codes, s, f[3] == "1")
    local parts = {}
    if not ok then parts[1] = err(a) else
      local i = 0
      for _ = 1, 100 do
        local r = table.pack(pcall(a, b, i))
        if not r[1] then parts[#parts+1] = err(r[2]); break end
        if r[2] == nil then break end
        parts[#parts+1] = r[2] .. ":" .. r[3]
        i = r[2]
      end
    end
    out = table.concat(parts, "|")
  end
  print(out)
end
"##;

fn ihex(i: i64) -> String {
    format!("i{i}")
}
fn e(x: Utf8Error) -> String {
    let m = match x {
        Utf8Error::Arg(n, m) => format!("bad argument #{n} to '?' ({m})"),
        Utf8Error::Msg(m) => m.to_string(),
    };
    format!("E{}", hex(m.as_bytes()))
}

const PIECES: &[&[u8]] = &[
    b"a",
    b"Z",
    b"0",
    b" ",
    b"\0",
    b"\xC2\xA9",
    b"\xDF\xBF",
    b"\xE2\x82\xAC",
    b"\xEF\xBF\xBF",
    b"\xF0\x9F\x98\x80",
    b"\xF4\x8F\xBF\xBF",
    b"\xF4\x90\x80\x80",
    b"\xF8\x88\x80\x80\x80",
    b"\xFD\xBF\xBF\xBF\xBF\xBF",
    b"\xFC\x84\x80\x80\x80\x80",
    b"\xED\xA0\x80",
    b"\xED\xBF\xBF",
    b"\xC0\x80",
    b"\xE0\x80\x80",
    b"\xC1\xBF",
    b"\x80",
    b"\xBF",
    b"\xFE",
    b"\xFF",
    b"\xC2",
    b"\xE2\x82",
    b"\xF0\x9F\x98",
    b"\xE0\x9F\xBF",
    b"\xF0\x8F\xBF\xBF",
    b"\xFB\xBF\xBF\xBF",
];

fn rand_str(r: &mut Rng) -> Vec<u8> {
    let mut v = Vec::new();
    for _ in 0..r.below(8) {
        v.extend_from_slice(r.pick(PIECES));
    }
    v
}

fn opt_num(r: &mut Rng, len: usize) -> String {
    match r.below(5) {
        0 => "-".into(),
        1 => r.range(-(len as i64) - 3, len as i64 + 3).to_string(),
        2 => r.range(-3, 3).to_string(),
        3 => (len as i64 + 1).to_string(),
        _ => r.range(-(len as i64) - 1, len as i64 + 1).to_string(),
    }
}

fn parse_opt(x: &str) -> Option<i64> {
    if x == "-" {
        None
    } else {
        Some(x.parse().unwrap())
    }
}

fn gen(n: usize) -> Vec<String> {
    let mut r = Rng::new(0xBEEF);
    let mut v = Vec::new();
    for i in 0..n {
        let s = rand_str(&mut r);
        let lax = r.chance(1, 3) as u8;
        v.push(match i % 5 {
            0 => format!(
                "len\t{}\t{}\t{}\t{}",
                hex(&s),
                opt_num(&mut r, s.len()),
                opt_num(&mut r, s.len()),
                lax
            ),
            1 => format!(
                "codepoint\t{}\t{}\t{}\t{}",
                hex(&s),
                opt_num(&mut r, s.len()),
                opt_num(&mut r, s.len()),
                lax
            ),
            2 => {
                let mut l = String::from("char");
                for _ in 0..r.range(0, 4) {
                    let x: i64 = match r.below(8) {
                        0 => -1,
                        1 => 0x7FFF_FFFF + r.below(3) as i64,
                        2 => r.range(0, 0x7F),
                        3 => r.range(0x80, 0x7FF),
                        4 => r.range(0x800, 0xFFFF),
                        5 => r.range(0x10000, 0x10FFFF),
                        6 => r.range(0x110000, 0x7FFF_FFFF),
                        _ => 0xD800 + r.below(0x800) as i64,
                    };
                    l.push_str(&format!("\t{x}"));
                }
                l
            }
            3 => format!(
                "offset\t{}\t{}\t{}",
                hex(&s),
                r.range(-5, 5),
                opt_num(&mut r, s.len())
            ),
            _ => format!("codes\t{}\t{}", hex(&s), lax),
        });
    }
    v
}

fn run(line: &str) -> String {
    let f: Vec<&str> = line.split('\t').collect();
    match f[0] {
        "len" => {
            let s = unhex(f[1]);
            let i = parse_opt(f[2]).unwrap_or(1);
            let j = parse_opt(f[3]).unwrap_or(-1);
            match len(&s, i, j, f[4] == "1") {
                Ok(LenResult::Count(n)) => ihex(n),
                Ok(LenResult::Fail(p)) => format!("n {}", ihex(p)),
                Err(x) => e(x),
            }
        }
        "codepoint" => {
            let s = unhex(f[1]);
            let i = parse_opt(f[2]).unwrap_or(1);
            let j = parse_opt(f[3]);
            match codepoint(&s, i, j, f[4] == "1") {
                Ok(v) => v
                    .iter()
                    .map(|c| ihex(*c as i64))
                    .collect::<Vec<_>>()
                    .join(" "),
                Err(x) => e(x),
            }
        }
        "char" => {
            let args: Vec<i64> = f[1..].iter().map(|x| x.parse().unwrap()).collect();
            match char_from(&args) {
                Ok(b) => format!("s{}", hex(&b)),
                Err(x) => e(x),
            }
        }
        "offset" => {
            let s = unhex(f[1]);
            match offset(&s, f[2].parse().unwrap(), parse_opt(f[3])) {
                Ok(OffsetResult::Fail) => "n".into(),
                Ok(OffsetResult::Found(a, b)) => {
                    if cfg!(rustynail_lua54) {
                        ihex(a)
                    } else {
                        format!("{} {}", ihex(a), ihex(b))
                    }
                }
                Err(x) => e(x),
            }
        }
        _ => {
            let s = unhex(f[1]);
            let strict = f[2] != "1";
            if let Err(x) = codes_check(&s) {
                return e(x);
            }
            let mut parts = Vec::new();
            let mut i = 0i64;
            for _ in 0..100 {
                match codes_next(&s, i, strict) {
                    Err(x) => {
                        parts.push(e(x));
                        break;
                    }
                    Ok(None) => break,
                    Ok(Some((p, c))) => {
                        parts.push(format!("{p}:{c}"));
                        i = p;
                    }
                }
            }
            parts.join("|")
        }
    }
}

#[test]
fn utf8_differential() {
    let lines = gen(60_000);
    let data = lines.join("\n") + "\n";
    let Some(reference) = run_ref("utf8", false, RUNNER, &data, &[]) else {
        return;
    };
    let ours: Vec<String> = lines.iter().map(|l| run(l)).collect();
    compare("utf8", &lines, &reference, &ours);
}
