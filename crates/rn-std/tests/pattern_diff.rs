mod common;
use common::*;
use rn_std::pattern::*;

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
local TAB = {a = "X", b = false, c = 12, d = true, e = {}, [1] = "one", [2] = 2.5, [3] = false, f = "%1"}
local function mkfunc(mode)
  return function(...)
    local n = select("#", ...)
    if mode == 0 then
      local o = {}
      for i = 1, n do o[i] = tostring((select(i, ...))) end
      return table.concat(o, "|")
    elseif mode == 1 then return nil
    elseif mode == 2 then return false
    elseif mode == 3 then return 42
    elseif mode == 4 then return {}
    elseif mode == 5 then return true
    else return (...) end
  end
end
for line in io.lines(arg[1]) do
  local f = {}
  for x in (line .. "\t"):gmatch("(.-)\t") do f[#f+1] = x end
  local op = f[1]
  local out
  if op == "find" or op == "match" then
    local s, p = unhex(f[2]), unhex(f[3])
    local r = table.pack(pcall(string[op], s, p, tonumber(f[4]), f[5] == "1"))
    if r[1] then out = fmt({n = r.n - 1, table.unpack(r, 2, r.n)}) else out = "E" .. hex(r[2]) end
  elseif op == "gmatch" then
    local s, p = unhex(f[2]), unhex(f[3])
    local ok, it = pcall(string.gmatch, s, p, tonumber(f[4]))
    local parts = {}
    if not ok then parts[1] = "E" .. hex(it) else
      for i = 1, 40 do
        local r = table.pack(pcall(it))
        if not r[1] then parts[#parts+1] = "E" .. hex(r[2]); break end
        if r.n == 1 then break end
        parts[#parts+1] = fmt({n = r.n - 1, table.unpack(r, 2, r.n)})
      end
    end
    out = table.concat(parts, "|")
  else -- gsub
    local s, p = unhex(f[2]), unhex(f[3])
    local repl
    if f[4] == "str" then repl = unhex(f[5]) elseif f[4] == "func" then repl = mkfunc(tonumber(f[5])) else repl = TAB end
    local r
    if f[6] == "-" then r = table.pack(pcall(string.gsub, s, p, repl))
    else r = table.pack(pcall(string.gsub, s, p, repl, tonumber(f[6]))) end
    if r[1] then out = hex(r[2]) .. " " .. tostring(r[3]) else out = "E" .. hex(r[2]) end
  end
  print(out)
end
"##;

fn ihex(i: i64) -> String {
    format!("i{i}")
}
fn shex(b: &[u8]) -> String {
    format!("s{}", hex(b))
}
fn cap_str(c: &Cap) -> String {
    match c {
        Cap::Str(b) => shex(b),
        Cap::Pos(n) => ihex(*n),
    }
}
fn join(v: Vec<String>) -> String {
    v.join(" ")
}
fn e(m: &[u8]) -> String {
    format!("E{}", hex(m))
}

const TOKS: &[&str] = &[
    "a", "b", "c", "x", " ", "1", "2", ".", "%a", "%d", "%s", "%w", "%p", "%l", "%u", "%x", "%c",
    "%g", "%z", "%A", "%D", "%S", "[abc]", "[^a-c]", "[%a_]", "[a-]", "[]]", "[^]]", "[%]", "[a",
    "*", "+", "-", "?", "*", "+", "?", "(", ")", "()", "(", ")", "^", "$", "%b()", "%bxy", "%b",
    "%f[%w]", "%f[^%w]", "%f", "%f[a-c]", "%1", "%2", "%0", "%9", "%%", "%", "[", "]", "-", "^",
    "$", "(", ")", "%.", "%(", "%-", "\0", "%f[%z]",
];
const SUBJ: &[u8] = b"abcabc  12 3x()(a(b)c))_-.\n\t\0AZ\xff";

fn rand_subject(r: &mut Rng) -> Vec<u8> {
    let n = r.below(22) as usize;
    (0..n)
        .map(|_| SUBJ[r.below(SUBJ.len() as u64) as usize])
        .collect()
}

fn rand_pattern(r: &mut Rng) -> Vec<u8> {
    let mut p = Vec::new();
    if r.chance(1, 4) {
        p.push(b'^');
    }
    let n = r.below(9);
    for _ in 0..n {
        if r.chance(1, 6) {
            // balanced capture group around a simple item
            p.push(b'(');
            p.extend_from_slice(r.pick(TOKS).as_bytes());
            if r.chance(1, 2) {
                p.extend_from_slice(r.pick(&["*", "+", "-", "?", ""]).as_bytes());
            }
            p.push(b')');
        } else {
            p.extend_from_slice(r.pick(TOKS).as_bytes());
        }
    }
    if r.chance(1, 6) {
        p.push(b'$');
    }
    p
}

fn rand_init(r: &mut Rng, len: usize) -> i64 {
    match r.below(8) {
        0 => 0,
        1 => -(r.below(len as u64 + 3) as i64),
        2 => len as i64 + r.range(-1, 3),
        3 => r.range(-30, 30),
        _ => 1,
    }
}

struct Case {
    line: String,
}

fn gen_cases(n: usize) -> Vec<Case> {
    let mut r = Rng::new(0xC0FFEE);
    let mut v = Vec::new();
    for i in 0..n {
        let s = rand_subject(&mut r);
        let mut p = rand_pattern(&mut r);
        if r.chance(1, 5) {
            // derive a pattern that probably matches: take a slice of the subject and escape it
            let a = r.below(s.len() as u64 + 1) as usize;
            let b = a + r.below((s.len() - a) as u64 + 1) as usize;
            p = Vec::new();
            for &c in &s[a..b] {
                if r.chance(1, 4) {
                    p.extend_from_slice(b"(.)");
                } else if c.is_ascii_alphanumeric() {
                    p.push(c);
                } else {
                    p.push(b'%');
                    p.push(c);
                }
            }
        }
        let init = rand_init(&mut r, s.len());
        let line = match i % 5 {
            0 => format!(
                "find\t{}\t{}\t{}\t{}",
                hex(&s),
                hex(&p),
                init,
                r.chance(1, 8) as u8
            ),
            1 => format!("match\t{}\t{}\t{}\t0", hex(&s), hex(&p), init),
            2 => format!("gmatch\t{}\t{}\t{}", hex(&s), hex(&p), init),
            _ => {
                let max = if r.chance(1, 3) {
                    r.range(-1, 6).to_string()
                } else {
                    "-".into()
                };
                let (kind, karg) = match r.below(4) {
                    0 | 1 => {
                        let toks: &[&str] = &[
                            "%1", "%2", "%0", "x", "%%", "%", "%3", "-", "%a", "[%0]", "%9",
                        ];
                        let mut rep = String::new();
                        for _ in 0..r.below(4) {
                            rep.push_str(r.pick(toks));
                        }
                        ("str", hex(rep.as_bytes()))
                    }
                    2 => ("func", r.below(7).to_string()),
                    _ => ("table", "0".to_string()),
                };
                format!(
                    "gsub\t{}\t{}\t{}\t{}\t{}",
                    hex(&s),
                    hex(&p),
                    kind,
                    karg,
                    max
                )
            }
        };
        v.push(Case { line });
    }
    v
}

fn run_case(line: &str) -> String {
    let f: Vec<&str> = line.split('\t').collect();
    let s = unhex(f[1]);
    let p = unhex(f[2]);
    match f[0] {
        "find" | "match" => {
            let init: i64 = f[3].parse().unwrap();
            let find = f[0] == "find";
            match find_aux(&s, &p, init, find, f[4] == "1") {
                Err(m) => e(&m),
                Ok(FindResult::NotFound) => "n".into(),
                Ok(FindResult::Found { start, end, caps }) => {
                    let mut o = Vec::new();
                    if find {
                        o.push(ihex(start as i64));
                        o.push(ihex(end as i64));
                    }
                    o.extend(caps.iter().map(cap_str));
                    join(o)
                }
            }
        }
        "gmatch" => {
            let init: i64 = f[3].parse().unwrap();
            let mut g = GMatch::new(&s, &p, init);
            let mut parts = Vec::new();
            for _ in 0..40 {
                match g.next_match() {
                    Err(m) => {
                        parts.push(e(&m));
                        break;
                    }
                    Ok(None) => break,
                    Ok(Some(c)) => parts.push(join(c.iter().map(cap_str).collect())),
                }
            }
            parts.join("|")
        }
        _ => {
            let max: i64 = if f[5] == "-" {
                s.len() as i64 + 1
            } else {
                f[5].parse().unwrap()
            };
            let karg = f[4];
            let rep_bytes;
            let mut errv: Option<Vec<u8>> = None;
            let mut table_cb;
            let mut func_cb;
            let repl = match f[3] {
                "str" => {
                    rep_bytes = unhex(karg);
                    Replacement::Str(&rep_bytes)
                }
                "func" => {
                    let mode: u32 = karg.parse().unwrap();
                    func_cb = move |m: &Matched| -> PResult<ReplValue> {
                        let caps = m.all()?;
                        Ok(match mode {
                            0 => {
                                let mut o = Vec::new();
                                for (i, c) in caps.iter().enumerate() {
                                    if i > 0 {
                                        o.push(b'|');
                                    }
                                    match c {
                                        Cap::Str(b) => o.extend_from_slice(b),
                                        Cap::Pos(n) => {
                                            o.extend_from_slice(n.to_string().as_bytes())
                                        }
                                    }
                                }
                                ReplValue::Bytes(o)
                            }
                            1 | 2 => ReplValue::Keep,
                            3 => ReplValue::Bytes(b"42".to_vec()),
                            4 => return Err(b"invalid replacement value (a table)".to_vec()),
                            5 => return Err(b"invalid replacement value (a boolean)".to_vec()),
                            _ => match caps[0] {
                                Cap::Str(b) => ReplValue::Bytes(b.to_vec()),
                                Cap::Pos(n) => ReplValue::Bytes(n.to_string().into_bytes()),
                            },
                        })
                    };
                    Replacement::Callback(&mut func_cb)
                }
                _ => {
                    table_cb = |m: &Matched| -> PResult<ReplValue> {
                        let k = m.first()?;
                        Ok(match k {
                            Cap::Str(b"a") => ReplValue::Bytes(b"X".to_vec()),
                            Cap::Str(b"c") => ReplValue::Bytes(b"12".to_vec()),
                            Cap::Str(b"e") => {
                                return Err(b"invalid replacement value (a table)".to_vec())
                            }
                            Cap::Str(b"d") => {
                                return Err(b"invalid replacement value (a boolean)".to_vec())
                            }
                            Cap::Str(b"f") => ReplValue::Bytes(b"%1".to_vec()),
                            Cap::Pos(1) => ReplValue::Bytes(b"one".to_vec()),
                            Cap::Pos(2) => ReplValue::Bytes(b"2.5".to_vec()),
                            _ => ReplValue::Keep,
                        })
                    };
                    Replacement::Callback(&mut table_cb)
                }
            };
            let _ = &mut errv;
            match gsub(&s, &p, repl, max) {
                Err(m) => e(&m),
                Ok((r, n)) => format!("{} {}", hex(r.as_deref().unwrap_or(&s)), n),
            }
        }
    }
}

#[test]
fn pattern_differential() {
    let cases = gen_cases(120_000);
    let lines: Vec<String> = cases.iter().map(|c| c.line.clone()).collect();
    let data = lines.join("\n") + "\n";
    let Some(reference) = run_ref("pattern", false, RUNNER, &data, &[]) else {
        return;
    };
    let ours: Vec<String> = lines.iter().map(|l| run_case(l)).collect();
    compare("pattern", &lines, &reference, &ours);
}

#[test]
fn pattern_limits() {
    // recursion limit and errors
    let s = vec![b'a'; 300];
    let mut p = Vec::new();
    for _ in 0..250 {
        p.extend_from_slice(b"a?");
    }
    assert!(find_aux(&s, &p, 1, false, false).is_err());
    assert_eq!(
        find_aux(b"x", b"%", 1, true, false).unwrap_err(),
        b"malformed pattern (ends with '%')".to_vec()
    );
}
