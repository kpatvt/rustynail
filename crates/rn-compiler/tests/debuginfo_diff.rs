//! Differential test of `rn_core::debuginfo` (port of the pure parts of `ldebug.c`) without a VM:
//! each snippet is run under the reference interpreter to record the exact runtime message
//! (`varinfo` suffix) or `debug.getinfo(1, "n")` result (`funcnamefromcode`); the same snippet is
//! compiled with our front end and the faulting/calling instruction is located by opcode name.
mod common;
use common::*;
use rn_compiler::testhost::TestHost;
use rn_compiler::{load, LoadMode};
use rn_core::debuginfo::*;
use rn_core::host::CompilerHost;
use rn_core::instruction::Instruction;
use rn_core::protodata::ProtoData;
use rn_core::ver::active::OP_NAMES;
use std::process::Command;

/// What to compare.
#[derive(Clone, Debug)]
enum Kind {
    /// Runtime error message with a `varinfo` suffix; `ops` are candidate faulting opcodes (last
    /// occurrence in the proto at `path` wins); `operand` selects the register/upvalue to describe.
    Var(&'static [&'static str], Operand),
    /// `debug.getinfo(1,"n")` of a callee invoked by one of the candidate opcodes.
    Fn(&'static [&'static str]),
    /// 5.5 `global 'x' already defined`.
    #[cfg_attr(rustynail_lua54, allow(dead_code))]
    ErrNNil,
}

#[derive(Clone, Copy, Debug)]
enum Operand {
    /// Register in field A / B.
    RegA,
    RegB,
    /// Register A + 1 (second operand of a two-operand CONCAT).
    RegA1,
    /// Register A + 2.
    RegA2,
    /// Upvalue index in field B / A.
    UpB,
    UpA,
    /// `luaG_callerror` (funcnamefromcode, then varinfo of register A).
    Call,
}

#[derive(Clone, Debug)]
struct Case {
    src: String,
    path: Vec<usize>,
    kind: Kind,
}

fn var(src: &str, path: &[usize], ops: &'static [&'static str], operand: Operand) -> Case {
    Case {
        src: src.into(),
        path: path.to_vec(),
        kind: Kind::Var(ops, operand),
    }
}
fn func(src: &str, path: &[usize], ops: &'static [&'static str]) -> Case {
    Case {
        src: src.into(),
        path: path.to_vec(),
        kind: Kind::Fn(ops),
    }
}

const MMBINS: &[&str] = &["MMBIN", "MMBINI", "MMBINK"];

fn cases() -> Vec<Case> {
    let mut v = Vec::new();

    // ---- systematic: nil-valued sources x uses -------------------------------------------------
    // (prelude, expression, wrapped-in-function?)
    let sources: &[(&str, &str, bool)] = &[
        ("local x = nil; ", "x", false),
        ("", "gx", false),
        ("local t = {}; ", "t.f", false),
        ("local t = {a = {}}; ", "t.a.f", false),
        ("local u; ", "u", true),
        ("local t = {}; ", "t[1]", false),
        ("local t = {}; local k = 'a'; ", "t[k]", false),
        ("local t = {}; ", "t['a b']", false),
        ("local function f() end; ", "f()", false),
        ("local u = {}; ", "u.f", true),
        ("local x <const> = nil; ", "x", false),
        ("local a, b = 1; ", "b", false),
        (
            "local function f(...) local a = ...; return a end; ",
            "f()",
            false,
        ),
    ];
    // (use template with `E`, candidate ops, operand)
    let uses: &[(&str, &'static [&'static str], Operand)] = &[
        ("return E.k", &["GETFIELD"], Operand::RegB),
        ("return E[1]", &["GETI"], Operand::RegB),
        ("local kk = 'a'; return E[kk]", &["GETTABLE"], Operand::RegB),
        ("E.k = 1", &["SETFIELD"], Operand::RegA),
        ("E[1] = 1", &["SETI"], Operand::RegA),
        ("local kk = 'a'; E[kk] = 1", &["SETTABLE"], Operand::RegA),
        ("E:m()", &["SELF"], Operand::RegB),
        ("E()", &["CALL"], Operand::Call),
        ("return E + 1", MMBINS, Operand::RegA),
        ("return E + 1.5", MMBINS, Operand::RegA),
        ("return 1 + E", MMBINS, Operand::RegA),
        ("local y = 1; return y - E", MMBINS, Operand::RegB),
        ("local y = 1; return E * y", MMBINS, Operand::RegA),
        ("return -E", &["UNM"], Operand::RegB),
        ("return #E", &["LEN"], Operand::RegB),
        ("return ~E", &["BNOT"], Operand::RegB),
        ("return E .. 'x'", &["CONCAT"], Operand::RegA),
        ("return 'x' .. E", &["CONCAT"], Operand::RegA1),
        ("return E | 1", MMBINS, Operand::RegA),
        ("return E // 2", MMBINS, Operand::RegA),
    ];
    for (pre, e, wrap) in sources {
        for (use_, ops, operand) in uses {
            let body = use_.replace('E', e);
            // (the replace above also touches other capital E's: none of the templates have any)
            let (src, path): (String, Vec<usize>) = if *wrap {
                (
                    format!("{pre}local function w() {body} end return w()"),
                    vec![0],
                )
            } else {
                (format!("{pre}{body}"), vec![])
            };
            let c = Case {
                src,
                path,
                kind: Kind::Var(ops, *operand),
            };
            // for `u` upvalue sources with direct index/call use, the instruction is GETTABUP/SETTABUP
            if *wrap && *e == "u" {
                match *use_ {
                    "return E.k" => v.push(Case {
                        kind: Kind::Var(&["GETTABUP"], Operand::UpB),
                        ..c
                    }),
                    "E.k = 1" => v.push(Case {
                        kind: Kind::Var(&["SETTABUP"], Operand::UpA),
                        ..c
                    }),
                    _ => v.push(c),
                }
            } else {
                v.push(c);
            }
        }
    }

    // ---- handcrafted varinfo cases ------------------------------------------------------------
    let m = |s: &str, p: &[usize], o: &'static [&'static str], a: Operand| var(s, p, o, a);
    v.push(m(
        "local _ENV = nil; x = 1",
        &[],
        &["SETFIELD"],
        Operand::RegA,
    ));
    v.push(m(
        "local _ENV = nil; return x",
        &[],
        &["GETFIELD"],
        Operand::RegB,
    ));
    v.push(m(
        "local _ENV = nil; local function f() return x end return f()",
        &[0],
        &["GETTABUP"],
        Operand::UpB,
    ));
    v.push(m(
        "local _ENV = nil; local function f() x = 1 end return f()",
        &[0],
        &["SETTABUP"],
        Operand::UpA,
    ));
    v.push(m("local a = 1.5; return a | 1", &[], MMBINS, Operand::RegA));
    v.push(m(
        "local a, b = 1.5, 2; return b & a",
        &[],
        MMBINS,
        Operand::RegB,
    ));
    v.push(m(
        "local a, b = 1.5, 2; return a ~ b",
        &[],
        MMBINS,
        Operand::RegA,
    ));
    v.push(m(
        "local a = 2^63; return a << 1",
        &[],
        MMBINS,
        Operand::RegA,
    ));
    v.push(m("local a = 1.5; return ~a", &[], &["BNOT"], Operand::RegB));
    v.push(m("local a = {}; return a + 1", &[], MMBINS, Operand::RegA));
    v.push(m(
        "local a = {}; local b = 2; return b + a",
        &[],
        MMBINS,
        Operand::RegB,
    ));
    v.push(m(
        "local a, b = {}, {}; return a + b",
        &[],
        MMBINS,
        Operand::RegA,
    ));
    v.push(m(
        "local a = print; return a .. 'x'",
        &[],
        &["CONCAT"],
        Operand::RegA,
    ));
    v.push(m(
        "local a, b, c = 'x', nil, 'z'; return a .. b .. c",
        &[],
        &["CONCAT"],
        Operand::RegA1,
    ));
    v.push(m(
        "local a, b, c = nil, 'y', 'z'; return a .. b .. c",
        &[],
        &["CONCAT"],
        Operand::RegA,
    ));
    v.push(m(
        "local a, b, c = 'x', 'y', nil; return a .. b .. c",
        &[],
        &["CONCAT"],
        Operand::RegA2,
    ));
    v.push(m(
        "local t = {a = {}} return t.a.b.c",
        &[],
        &["GETFIELD"],
        Operand::RegB,
    ));
    v.push(m(
        "local t = {a = {}} t.a.b.c = 1",
        &[],
        &["SETFIELD"],
        Operand::RegA,
    ));
    v.push(m(
        "return ('x')()",
        &[],
        &["CALL", "TAILCALL"],
        Operand::Call,
    ));
    v.push(m(
        "local n = 5; return n.x",
        &[],
        &["GETFIELD"],
        Operand::RegB,
    ));
    v.push(m(
        "local n = true; n.x = 1",
        &[],
        &["SETFIELD"],
        Operand::RegA,
    ));
    v.push(m("local n = 5; n()", &[], &["CALL"], Operand::Call));
    v.push(m("local n = true; n:m()", &[], &["SELF"], Operand::RegB));
    v.push(m("local n = {}; return n + 1", &[], MMBINS, Operand::RegA));
    v.push(m(
        "local n = 'a'; return #n .. n.x.y",
        &[],
        &["GETFIELD"],
        Operand::RegB,
    ));
    v.push(m("local n = 'a'; return -{}", &[], &["UNM"], Operand::RegB));
    v.push(m("return #5", &[], &["LEN"], Operand::RegB));
    v.push(m("local s = 'abc'; s()", &[], &["CALL"], Operand::Call));
    v.push(m(
        "local t = {} t:nomethod()",
        &[],
        &["CALL"],
        Operand::Call,
    ));
    v.push(m(
        "local t = {x = {}} t.x.y()",
        &[],
        &["CALL"],
        Operand::Call,
    ));
    v.push(m(
        "local t = {x = {y = {}}} t.x.y.z()",
        &[],
        &["CALL"],
        Operand::Call,
    ));
    v.push(m("gt = {} gt.method()", &[], &["CALL"], Operand::Call));
    v.push(m("gt = {} gt:method()", &[], &["CALL"], Operand::Call));
    v.push(m("string.nofunc()", &[], &["CALL"], Operand::Call));
    v.push(m("math.huge()", &[], &["CALL"], Operand::Call));
    v.push(m(
        "local up; local function g() up() end g()",
        &[0],
        &["CALL"],
        Operand::Call,
    ));
    v.push(m(
        "local up = {}; local function g() up.f() end g()",
        &[0],
        &["CALL"],
        Operand::Call,
    ));
    v.push(m(
        "local up = {}; local function g() up:f() end g()",
        &[0],
        &["CALL"],
        Operand::Call,
    ));
    v.push(m("for x in nil do end", &[], &["TFORCALL"], Operand::Call));
    v.push(m("for x in 5 do end", &[], &["TFORCALL"], Operand::Call));
    v.push(m(
        "local t = {} for x in t do end",
        &[],
        &["TFORCALL"],
        Operand::Call,
    ));
    // metamethod values that are not callable
    v.push(m(
        "local t = setmetatable({}, {__add = 5}); return t + 1",
        &[],
        MMBINS,
        Operand::Call,
    ));
    v.push(m(
        "local t = setmetatable({}, {__add = 5}); local u = {}; return t + u",
        &[],
        MMBINS,
        Operand::Call,
    ));
    v.push(m(
        "local t = setmetatable({}, {__sub = 5}); return 1 - t",
        &[],
        MMBINS,
        Operand::Call,
    ));
    v.push(m(
        "local t = setmetatable({}, {__shl = 5}); return t << 1",
        &[],
        MMBINS,
        Operand::Call,
    ));
    v.push(m(
        "local t = setmetatable({}, {__concat = 5}); return t .. 'x'",
        &[],
        &["CONCAT"],
        Operand::Call,
    ));
    v.push(m(
        "local t = setmetatable({}, {__len = 5}); return #t",
        &[],
        &["LEN"],
        Operand::Call,
    ));
    v.push(m(
        "local t = setmetatable({}, {__unm = 5}); return -t",
        &[],
        &["UNM"],
        Operand::Call,
    ));
    v.push(m(
        "local t = setmetatable({}, {__bnot = 5}); return ~t",
        &[],
        &["BNOT"],
        Operand::Call,
    ));
    v.push(m("local t = setmetatable({}, {__eq = 5}); local u = setmetatable({}, getmetatable(t)); return t == u", &[], &["EQ"], Operand::Call));
    v.push(m(
        "local t = setmetatable({}, {__lt = 5}); return t < t",
        &[],
        &["LT"],
        Operand::Call,
    ));
    v.push(m(
        "local t = setmetatable({}, {__le = 5}); return t <= t",
        &[],
        &["LE"],
        Operand::Call,
    ));
    v.push(m(
        "local t = setmetatable({}, {__lt = 5}); return t < 1",
        &[],
        &["LTI"],
        Operand::Call,
    ));
    v.push(m(
        "local t = setmetatable({}, {__lt = 5}); return t > 1",
        &[],
        &["GTI"],
        Operand::Call,
    ));
    v.push(m(
        "local t = setmetatable({}, {__le = 5}); return t <= 1",
        &[],
        &["LEI"],
        Operand::Call,
    ));
    v.push(m(
        "local t = setmetatable({}, {__le = 5}); return t >= 1",
        &[],
        &["GEI"],
        Operand::Call,
    ));
    v.push(m(
        "local t = setmetatable({}, {__index = function() return nil end}); return t.x.y",
        &[],
        &["GETFIELD"],
        Operand::RegB,
    ));
    v.push(m(
        "local t = setmetatable({}, {__call = 5}); t()",
        &[],
        &["CALL"],
        Operand::Call,
    ));
    #[cfg(not(rustynail_lua54))]
    {
        v.push(Case {
            src: "x = 1; global x = 2".into(),
            path: vec![],
            kind: Kind::ErrNNil,
        });
        v.push(Case {
            src: "global print; global print = 2".into(),
            path: vec![],
            kind: Kind::ErrNNil,
        });
        v.push(Case {
            src: "local function f(...t) return t.a.b end return f()".into(),
            path: vec![0],
            kind: Kind::Var(&["GETFIELD"], Operand::RegB),
        });
    }

    // ---- funcnamefromcode (getinfo 'n') ---------------------------------------------------------
    let cb = "local rawset, getinfo, G, retv = rawset, debug.getinfo, _G, function() end; local function cb() rawset(G, '__r', getinfo(1, 'n')) return retv end ";
    let f = |s: &str, p: &[usize], o: &'static [&'static str]| func(&format!("{cb}{s}"), p, o);
    v.push(f("cb()", &[], &["CALL"]));
    v.push(f("gcb = cb; gcb()", &[], &["CALL"]));
    v.push(f("local t = {cb = cb}; t.cb()", &[], &["CALL"]));
    v.push(f("local t = {cb = cb}; t:cb()", &[], &["CALL"]));
    v.push(f("local t = {cb = cb}; t['cb']()", &[], &["CALL"]));
    v.push(f("local t = {cb}; t[1]()", &[], &["CALL"]));
    v.push(f(
        "local t = {cb = cb}; local k = 'cb'; t[k]()",
        &[],
        &["CALL"],
    ));
    v.push(f("local t = {{cb = cb}}; t[1].cb()", &[], &["CALL"]));
    v.push(f("local function g() cb() end g()", &[2], &["CALL"]));
    v.push(f(
        "local up = {cb = cb}; local function g() up.cb() end g()",
        &[2],
        &["CALL"],
    ));
    v.push(f(
        "local up = {cb = cb}; local function g() up:cb() end g()",
        &[2],
        &["CALL"],
    ));
    v.push(f(
        "gcb = cb; local function g() gcb() end g()",
        &[2],
        &["CALL"],
    ));
    v.push(f("local _ENV = {gcb = cb}; gcb()", &[], &["CALL"]));
    v.push(f("local e = {gcb = cb}; e.gcb()", &[], &["CALL"]));
    v.push(f(
        "local function cbn() rawset(G, '__r', getinfo(1, 'n')) end for x in cbn do end",
        &[],
        &["TFORCALL"],
    ));
    v.push(f(
        "local function cbn() rawset(G, '__r', getinfo(1, 'n')) end for x in cbn, nil, nil do end",
        &[],
        &["TFORCALL"],
    ));
    v.push(f("local a = cb; a()", &[], &["CALL"]));
    v.push(f(
        "(function() rawset(_G, '__r', debug.getinfo(1, 'n')) end)()",
        &[],
        &["CALL"],
    ));
    v.push(f(
        "local t = {f = function() rawset(_G, '__r', debug.getinfo(1, 'n')) end}; t.f()",
        &[],
        &["CALL"],
    ));
    v.push(f(
        "local s = setmetatable({}, {__index = cb}); local _ = s.x",
        &[],
        &["GETFIELD"],
    ));
    v.push(f(
        "local s = setmetatable({}, {__index = cb}); local _ = s[1]",
        &[],
        &["GETI"],
    ));
    v.push(f(
        "local s = setmetatable({}, {__index = cb}); local k = 'a'; local _ = s[k]",
        &[],
        &["GETTABLE"],
    ));
    v.push(f(
        "local s = setmetatable({}, {__index = cb}); s:foo()",
        &[],
        &["SELF"],
    ));
    v.push(f(
        "local s = setmetatable({}, {__newindex = cb}); s.x = 1",
        &[],
        &["SETFIELD"],
    ));
    v.push(f(
        "local s = setmetatable({}, {__newindex = cb}); s[1] = 1",
        &[],
        &["SETI"],
    ));
    v.push(f(
        "local s = setmetatable({}, {__newindex = cb}); local k = 'a'; s[k] = 1",
        &[],
        &["SETTABLE"],
    ));
    v.push(f(
        "local _ENV = setmetatable({}, {__index = cb}); return gv",
        &[],
        &["GETFIELD"],
    ));
    v.push(f(
        "setmetatable(_G, {__index = cb}); local x = undefinedvar_zz",
        &[],
        &["GETTABUP"],
    ));
    v.push(f(
        "setmetatable(_G, {__newindex = cb}); undefinedvar_zz = 1",
        &[],
        &["SETTABUP"],
    ));
    let mm = |name: &str, expr: &str, ops: &'static [&'static str]| {
        f(&format!("local t = setmetatable({{}}, {{__{name} = cb}}); local u = setmetatable({{}}, getmetatable(t)); local _ = {expr}"), &[], ops)
    };
    for (name, expr, ops) in [
        ("add", "t + 1", MMBINS),
        ("add", "t + u", MMBINS),
        ("add", "1 + t", MMBINS),
        ("sub", "t - 1.5", MMBINS),
        ("mul", "t * u", MMBINS),
        ("mod", "t % 2", MMBINS),
        ("pow", "t ^ 2", MMBINS),
        ("div", "t / 2", MMBINS),
        ("idiv", "t // 2", MMBINS),
        ("band", "t & 1", MMBINS),
        ("bor", "t | u", MMBINS),
        ("bxor", "t ~ 1", MMBINS),
        ("shl", "t << 1", MMBINS),
        ("shr", "t >> 1", MMBINS),
        ("shr", "1 >> t", MMBINS),
        ("unm", "-t", &["UNM"][..]),
        ("bnot", "~t", &["BNOT"][..]),
        ("len", "#t", &["LEN"][..]),
        ("concat", "t .. 'x'", &["CONCAT"][..]),
        ("concat", "'x' .. t", &["CONCAT"][..]),
        ("concat", "t .. 'x' .. 'y'", &["CONCAT"][..]),
        ("eq", "t == u", &["EQ"][..]),
        ("lt", "t < u", &["LT"][..]),
        ("le", "t <= u", &["LE"][..]),
        ("lt", "t < 1", &["LTI"][..]),
        ("lt", "t > 1", &["GTI"][..]),
        ("le", "t <= 1", &["LEI"][..]),
        ("le", "t >= 1", &["GEI"][..]),
        ("lt", "1 < t", &["GTI"][..]),
    ] {
        v.push(mm(name, expr, ops));
    }
    v.push(f(
        "do local x <close> = setmetatable({}, {__close = cb}) end",
        &[],
        &["CLOSE"],
    ));
    v.push(f(
        "local function g() local x <close> = setmetatable({}, {__close = cb}) end g()",
        &[2],
        &["RETURN"],
    ));
    v.push(f(
        "local function g() local x <close> = setmetatable({}, {__close = cb}); return 1 end g()",
        &[2],
        &["RETURN"],
    ));
    v.push(f("local t = setmetatable({}, {__call = function(self) rawset(_G, '__r', debug.getinfo(1, 'n')) end}); t()", &[], &["CALL"]));
    v
}

fn host_bytes(h: &TestHost, s: u32) -> Vec<u8> {
    h.bytes(s).to_vec()
}

fn find_instr(p: &ProtoData<u32>, ops: &[&str]) -> Option<usize> {
    let mut found = None;
    for (pc, &w) in p.code.iter().enumerate() {
        if let Some(op) = Instruction(w).opcode() {
            if ops.contains(&OP_NAMES[op as usize]) {
                found = Some(pc);
            }
        }
    }
    found
}

const KINDS: &[&str] = &[
    "local",
    "upvalue",
    "global",
    "field",
    "method",
    "constant",
    "metamethod",
    "for iterator",
];

/// First ` (kind 'name')` in `msg` (as `formatvarinfo` writes it) or empty.
fn extract_suffix(msg: &[u8]) -> Vec<u8> {
    let mut i = 0;
    while i + 2 < msg.len() {
        if &msg[i..i + 2] == b" (" {
            for k in KINDS {
                let kb = k.as_bytes();
                if msg[i + 2..].starts_with(kb) && msg[i + 2 + kb.len()..].starts_with(b" '") {
                    if let Some(end) = msg[i + 2 + kb.len() + 2..]
                        .windows(2)
                        .position(|w| w == b"')")
                    {
                        return msg[i..i + 2 + kb.len() + 2 + end + 2].to_vec();
                    }
                }
            }
        }
        i += 1;
    }
    Vec::new()
}

const RUNNER: &str = r##"
local function unhex(s) return (s:gsub("%x%x", function(h) return string.char(tonumber(h, 16)) end)) end
local function hex(s) return (s:gsub(".", function(c) return string.format("%02x", c:byte()) end)) end
for line in io.lines(arg[1]) do
  local mode, src = line:match("^(%a+)\t(%x*)$")
  src = unhex(src)
  local f, cerr = load(src, "=s")
  local out
  if not f then out = "COMPILE " .. cerr
  else
    rawset(_G, '__r', nil)
    local ok, msg = pcall(f)
    setmetatable(_G, nil)
    if mode == "fn" then
      if not ok then out = "ERR " .. tostring(msg)
      elseif __r == nil then out = "NOCB"
      else out = tostring(__r.name) .. "|" .. tostring(__r.namewhat) end
    else
      if ok then out = "NOERR" else out = (tostring(msg):gsub("^s:%d+: ", "")) end
    end
  end
  print(hex(out))
end
"##;

#[test]
#[cfg_attr(
    miri,
    ignore = "needs the reference luac/lua processes or the corpus files"
)]
fn debuginfo_matches_reference() {
    let Some(lua) = lua() else { return };
    let cases = cases();
    let mut data = String::new();
    for c in &cases {
        let mode = if matches!(c.kind, Kind::Fn(_)) {
            "fn"
        } else {
            "var"
        };
        data.push_str(&format!(
            "{mode}\t{}\n",
            c.src
                .bytes()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        ));
    }
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("dbg-{VER}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("run.lua"), RUNNER).unwrap();
    std::fs::write(dir.join("data.txt"), data).unwrap();
    let out = Command::new(lua)
        .arg(dir.join("run.lua"))
        .arg(dir.join("data.txt"))
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let _ = std::fs::remove_dir_all(&dir);
    let refs: Vec<Vec<u8>> = String::from_utf8(out.stdout)
        .unwrap()
        .lines()
        .map(|l| {
            (0..l.len() / 2)
                .map(|i| u8::from_str_radix(&l[2 * i..2 * i + 2], 16).unwrap())
                .collect()
        })
        .collect();
    assert_eq!(refs.len(), cases.len());

    let mut bad = 0;
    for (n, (c, r)) in cases.iter().zip(&refs).enumerate() {
        let mut host = TestHost::default();
        let mode = LoadMode {
            text: true,
            binary: false,
        };
        let proto = load(&mut host, c.src.as_bytes(), b"=s", mode).expect("compile");
        let mut p = &proto;
        for &i in &c.path {
            p = &p.protos[i];
        }
        let view = ProtoDataView::new(p, &host);
        let report = |what: &str, ours: &[u8]| {
            eprintln!(
                "MISMATCH case {n}: {}\n  ref : {}\n  ours: {} ({what})",
                c.src,
                String::from_utf8_lossy(r),
                String::from_utf8_lossy(ours)
            );
        };
        match &c.kind {
            Kind::Var(ops, operand) => {
                let Some(pc) = find_instr(p, ops) else {
                    eprintln!("case {n}: opcode {ops:?} not found in {}", c.src);
                    bad += 1;
                    continue;
                };
                let i = Instruction(p.code[pc]);
                let ours = match operand {
                    Operand::RegA => varinfo_register(&view, pc, i.a() as usize),
                    Operand::RegB => varinfo_register(&view, pc, i.b() as usize),
                    Operand::RegA1 => varinfo_register(&view, pc, i.a() as usize + 1),
                    Operand::RegA2 => varinfo_register(&view, pc, i.a() as usize + 2),
                    Operand::UpB => varinfo_upvalue(&view, i.b() as usize),
                    Operand::UpA => varinfo_upvalue(&view, i.a() as usize),
                    Operand::Call => {
                        let fallback = varinfo_register(&view, pc, i.a() as usize);
                        callerror_extra(&view, pc, fallback)
                    }
                };
                let expect = extract_suffix(r);
                if ours != expect {
                    bad += 1;
                    report("varinfo", &ours);
                }
            }
            Kind::Fn(ops) => {
                let Some(pc) = find_instr(p, ops) else {
                    eprintln!("case {n}: opcode {ops:?} not found in {}", c.src);
                    bad += 1;
                    continue;
                };
                let ours = match funcnamefromcode(&view, pc) {
                    Some(o) => format!("{}|{}", String::from_utf8_lossy(o.name), o.kind),
                    None => "nil|".to_string(),
                };
                if ours.as_bytes() != r.as_slice() {
                    bad += 1;
                    report("funcname", ours.as_bytes());
                }
            }
            Kind::ErrNNil => {
                let Some(pc) = find_instr(p, &["ERRNNIL"]) else {
                    eprintln!("case {n}: ERRNNIL not found in {}", c.src);
                    bad += 1;
                    continue;
                };
                let k = Instruction(p.code[pc]).bx() as usize;
                let ours = errnnil_message(&view, k);
                if ours != *r {
                    bad += 1;
                    report("errnnil", &ours);
                }
            }
        }
    }
    let _ = host_bytes;
    eprintln!(
        "debuginfo: {} cases compared, {} mismatches",
        cases.len(),
        bad
    );
    assert_eq!(bad, 0);
}

#[test]
#[cfg_attr(
    miri,
    ignore = "needs the reference luac/lua processes or the corpus files"
)]
fn line_info_and_chunkid() {
    // get_func_line agrees with the listing's own line walk on every proto of a real chunk
    let src = std::fs::read(root().join("tests/lua/basic.lua")).unwrap();
    let mut host = TestHost::default();
    let mode = LoadMode {
        text: true,
        binary: false,
    };
    let proto = load(&mut host, &src, b"@basic.lua", mode).unwrap();
    fn walk(p: &ProtoData<u32>, host: &TestHost, count: &mut usize) {
        let view = ProtoDataView::new(p, host);
        for pc in 0..p.code.len() {
            assert_eq!(
                get_func_line(&view, pc),
                rn_compiler::listing::get_func_line(p, pc)
            );
            *count += 1;
        }
        for c in &p.protos {
            walk(c, host, count);
        }
    }
    let mut count = 0;
    walk(&proto, &host, &mut count);
    assert!(count > 10);
    assert_eq!(chunk_id(b"@basic.lua"), b"basic.lua".to_vec());
    assert_eq!(chunk_id(b"=name"), b"name".to_vec());
    assert_eq!(
        chunk_id(b"print(1)\nx"),
        b"[string \"print(1)...\"]".to_vec()
    );
}
