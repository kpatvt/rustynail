//! Syntax/semantic error messages compared with the reference `luac -p -`, plus nesting-limit and
//! never-panic properties.
mod common;
use common::*;
use rn_compiler::testhost::TestHost;
use rn_compiler::{load, LoadMode};
use std::io::Write;
use std::process::{Command, Stdio};

fn ours(src: &[u8]) -> Result<(), String> {
    let src = src.to_vec();
    // small explicit stack: the nesting limit must protect it
    let h = std::thread::Builder::new()
        .stack_size(512 * 1024)
        .spawn(move || {
            let mut host = TestHost::default();
            let mode = LoadMode {
                text: true,
                binary: false,
            };
            load(&mut host, &src, b"=stdin", mode)
                .map(|_| ())
                .map_err(|e| String::from_utf8_lossy(&e.message).into_owned())
        })
        .unwrap();
    h.join().expect("compiler panicked or overflowed its stack")
}

fn reference(luac: &std::path::Path, src: &[u8]) -> Result<(), String> {
    let mut c = Command::new(luac)
        .args(["-p", "-"])
        .stdin(Stdio::piped())
        .stderr(Stdio::piped())
        .stdout(Stdio::null())
        .spawn()
        .unwrap();
    c.stdin.take().unwrap().write_all(src).unwrap();
    let out = c.wait_with_output().unwrap();
    if out.status.success() {
        return Ok(());
    }
    let e = String::from_utf8_lossy(&out.stderr).into_owned();
    let e = e.trim_end();
    Err(e
        .strip_prefix(&format!("{}: ", luac.display()))
        .unwrap_or(e)
        .to_string())
}

fn rep(s: &str, n: usize) -> String {
    s.repeat(n)
}

#[test]
#[cfg_attr(
    miri,
    ignore = "needs the reference luac/lua processes or the corpus files"
)]
fn syntax_errors_match_reference() {
    let Some(luac) = luac() else { return };
    let mut cases: Vec<String> = [
        // generic syntax errors
        "x =",
        "x = = 1",
        "x = 1 +",
        "local",
        "local 1",
        "local x =",
        "local function",
        "local function 1() end",
        "function",
        "function f",
        "function f(",
        "function f(a,",
        "function f(a b) end",
        "function f(1) end",
        "function f() end end",
        "if x then",
        "if x",
        "if x then else",
        "if x then elseif end",
        "if then end",
        "while x",
        "while x do",
        "while do end",
        "repeat",
        "repeat x = 1",
        "repeat until",
        "for",
        "for i",
        "for i =",
        "for i = 1 do end",
        "for i = 1, 2, 3, 4 do end",
        "for i, j = 1, 2 do end",
        "for 1 = 1, 2 do end",
        "for i in do end",
        "for i in x end",
        "do",
        "do end end",
        "end",
        "return return",
        "return 1 2",
        "x = 1 2",
        "x y",
        "1 = x",
        "f() = 1",
        "x, y",
        "x, y +",
        "(x) = 1",
        "x.y.z",
        "x:y",
        "x:y.z()",
        "x:",
        "x.",
        "x[",
        "x[1",
        "x[]",
        "{",
        "x = {",
        "x = {1,",
        "x = {1 2}",
        "x = {[1] 2}",
        "x = {[1] =}",
        "x = {y =}",
        "x = {,}",
        "x = (",
        "x = (1",
        "x = )",
        "f(",
        "f(1",
        "f(1,",
        "f(,)",
        "f(1 2)",
        "f{",
        "f'unterminated",
        "x = 'abc",
        "x = \"abc\ndef\"",
        "x = [[abc",
        "x = 1 .. ",
        "x = ..",
        "x = ...",
        "x = function end",
        "x = function(",
        "x = function() return end end",
        "x = not",
        "x = -",
        "x = #",
        "x = ~",
        "x = a ~",
        "x = a.1",
        "x = a.b.",
        "x = a:b",
        "local x <",
        "local x <const",
        "local x <const>",
        "local x <foo> = 1",
        "local x <close>, y <close> = 1, 2",
        "local x <const> = 1; x = 2",
        "local x <close> = nil; x = 1",
        "local x <const> = 1; x, y = 1, 2",
        "local x <const> = 1; function x() end",
        "local x <const> = {}; x.y = 1; x = 2",
        "local a <const> = 1; local function f() a = 2 end",
        "local a <const> = f(); a = 1",
        "for i = 1, 2 do i = 3 end",
        "for k, v in pairs({}) do k = 1 end",
        "for i = 1, 2 do local function f() i = 1 end end",
        // goto / labels / break
        "goto x",
        "goto",
        "::x::",
        "::x",
        "::x:: ::x::",
        "do ::x:: end ::x:: goto x",
        "break",
        "if x then break end",
        "do goto l1; local a; ::l1:: print(a) end",
        "goto l1; local a; ::l1:: print(a)",
        "repeat goto cont; local x; ::cont:: until x",
        "do goto l end local x ::l:: ; print(x)",
        "::a:: do ::a:: end",
        "function f() goto out end ::out::",
        "while true do local function f() break end end",
        "goto = 1",
        "x = goto",
        // vararg
        "function f() return ... end",
        "x = ...",
        "local function f() return function() return ... end end",
        "function f(a, ...) return function() return ... end end",
        "function f(..., a) end",
        "function f(...,) end",
        "function f(... a) end",
        "function f(...t, a) end",
        // lexer level
        "x = 3x",
        "x = 0x",
        "x = 1e",
        "x = 'a\\qb'",
        "x = '\\300'",
        "x = '\\u{110000000}'",
        "x = [==[",
        "x = [=",
        "--[[ unfinished",
        "x = @",
        "x = $",
        "x = ?",
        "x = `",
        "x = \\",
        "x = 1 ! 2",
        "x = 'a' 'b'",
        "x = '\\x4'",
        // a NUL byte is token 0: `lexerror` prints no "near" part (parser_diff finding)
        "x = \0",
        "\0",
        "local t = {1, \0}",
        "if \0 then end",
        "x = 1 \0",
        // 5.5: global declarations
        "global",
        "global x y",
        "global function",
        "global function 1() end",
        "global <const>",
        "global <const> x; x = 1",
        "global <close> x",
        "global <foo> x",
        "global x <close>",
        "global x = ",
        "global x, y = 1",
        "global *; y = 1; z",
        "global x; y = 1",
        "global x; return y",
        "global <const> *; x = 1",
        "global <const> x = 1; x = 2",
        "global x = 1; global <const> y = 2; y = 3",
        "global function f() end f = 1; global <const> f",
        "global <const> f; function f() end",
        "local _ENV = nil; global x",
        "do local _ENV <const> = 1; x = 1 end",
        "global x <const>; x = 1",
        "global *, y",
        "global * = 1",
        "global x, <const> y",
        "global a <const>, b = 1, 2; a = 3",
        "global a, b <const> = 1, 2; b = 3",
        "global function f() end; local function g() global *; z = 1 end",
        "global x; do global y; z = 1 end",
        // named varargs
        "function f(...t) t = 1 end f(1)",
        "function f(...t) return ... end function g() return ...t end",
        "function f(...t, ...u) end",
        "local function f(...t) local ...u end",
        "function f(...[t]) end",
        "local t <const> = 1; function f(...t) end; t = 2",
        // misc
        "x = 1 y = 2 z =",
        "local t = {} t.x:y = 1",
        "return 1,",
        "return,",
        "return;",
        "return ; x = 1",
        "x = ;",
        ";;; x",
        "a.b:c = 1",
        "local function f() end f() = 1",
        "x = {...}",
        "(f)",
        "(f)()",
        "f()()=",
        "local a, b, = 1",
        "local a,",
        "local a = 1,",
        "for i = 1,2,3,4,5 do end",
        "if x then y() elseif then end",
        "x = function() end()",
        "x = (function() end)(",
        "x = a and",
        "x = a or or b",
        "x = a == == b",
        "x = a < < b",
        "x = a >> >> b",
        "x = a // // b",
        "x = a ~~ b",
        "x = a ~= = b",
        "x = 'a' .. .. 'b'",
        "x = - - -",
        "x = not not",
        "x = #",
        "x = # #",
        "x = -",
        "x = 2 ^",
        "x = 2 ^ ^ 3",
        "return function() end end",
        "return (",
        "return {",
        "return x[",
        "return x:",
        "return x.",
        "local x = function() return",
        "local function f() local x <close> = nil; x = 1 end",
        "local a <close> = 1, 2, 3; local b <close>",
        "local x <const>, y <const> = 1; x = 2",
        "local function f() local function g() end g = 1 end local g <const> = 1; g = 2",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    // Lua 5.4: 'global' is an ordinary name; goto/label/attrib/break semantics of 5.4
    #[cfg(rustynail_lua54)]
    cases.extend(
        [
            "global x",
            "global x = 1",
            "global function f() end",
            "global <const> x = 1",
            "global *",
            "global",
            "global = 1; global.x = 2; return global",
            "local global <const> = 1; global = 2",
            "function f(...t) end",
            "function f(a, ...) return ... end",
            "function f() return ... end",
            "local function f(...) return function() return ... end end",
            "break",
            "if x then break end",
            "do break end",
            "while x do local function f() break end end",
            "repeat break until x break",
            "goto l",
            "goto l; local x; ::l:: print(x)",
            "goto l; local x; ::l::",
            "do goto l; local x; ::l:: end",
            "do goto l; local x; ::l:: x = 1 end",
            "::l:: ::l::",
            "::l:: do ::l:: end",
            "do ::l:: end ::l::",
            "::l:: goto l; ::l::",
            "goto continue; local a; ::continue:: a = 1",
            "repeat goto continue; local a; ::continue:: until a",
            "for i = 1, 3 do goto continue; local a ::continue:: end",
            "for i = 1, 3 do i = 1 end",
            "for i, v in pairs(t) do i = 1 end",
            "for i = 1, 3 do local function f() i = 2 end end",
            "for i = 1 do end",
            "for i = 1, 2, 3, 4 do end",
            "for i in do end",
            "for 1 = 1, 2 do end",
            "for i, in x do end",
            "for = 1, 2 do end",
            "local x <const> = 1; x = 2",
            "local x <close> = nil; x = 2",
            "local x <const>; x = 2",
            "local x <foo> = 1",
            "local x < const > = 1; x = 1",
            "local x <const <close>> = 1",
            "local x <close>, y <close> = 1, 2",
            "local x <const>, y <close> = 1, 2; y = 3",
            "local function f() local a <close> = nil; a = 1 end",
            "local x <const> = 1; local function f() x = 2 end",
            "local x <const> = {}; x.y = 1; x = 2",
            "local a <const>, b = 1, 2; b = 3; a = 4",
            "local a <const> = 1; function a() end",
            "local a <const> = 1; function a.b() end; function a:c() end; function a() end",
            "local t <close> = 1; return t",
            "x = 1 + ",
            "x = {1, 2,, 3}",
            "x = {[1] 2}",
            "x = {1 2}",
            "x = f(",
            "x = f(1,",
            "x = a.1",
            "x = a:b",
            "x = a:1()",
            "x = 'a' 'b'",
            "x = function(a, 1) end",
            "x = function(..., a) end",
            "x = function(a,) end",
            "return return",
            "return 1 2",
            "x = 3 = 4",
            "f() = 1",
            "a, b() = 1, 2",
            "(a) = 1",
            "(a.b) = 1",
            "a.b:c() = 1",
            "local 1",
            "local function 1() end",
            "local function f.g() end",
            "function f:g:h() end",
            "function f.() end",
            "function () end",
            "if x then else elseif y then end",
            "if x end",
            "while x end",
            "repeat until",
            "x = 0x",
            "x = 1e",
            "x = 3..4",
            "x = 0xep",
            "x = '\\u{110000000}'",
            "x = ~",
            "x = a ~ ~ b",
            "x = a // = b",
            "::l",
            ":: l ::",
            "goto",
            "goto 1",
            "goto goto",
        ]
        .map(String::from),
    );
    // nesting limits: exactly at the boundary of LUAI_MAXCCALLS
    for n in [195, 196, 197, 198, 199, 200] {
        cases.push(rep("do ", n) + &rep("end ", n));
        cases.push(format!("x = {}1{}", rep("(", n - 2), rep(")", n - 2)));
        cases.push(format!("x = {}1{}", rep("{", n - 2), rep("}", n - 2)));
        cases.push(format!("x = {}1", rep("- ", n - 2)));
        cases.push(format!("x = {}1", rep("not ", n - 2)));
        cases.push(format!("x = 1{}", rep(" ^ 1", n - 2)));
        cases.push(rep("while x do ", n) + &rep("end ", n));
        cases.push(rep("if x then ", n) + &rep("end ", n));
        cases.push(rep("function f() ", n) + &rep("end ", n));
        cases.push(format!("{}x = 1", rep("a, ", n - 2)));
        cases.push(format!("x = {}", rep("f(", n - 2)) + &rep(")", n - 2));
        cases.push(rep("for i = 1, 2 do ", n) + &rep("end ", n));
    }
    // limits and nesting
    // luaM_growvector limit on the number of functions in one function (MAXARG_Bx = 131071)
    for n in [131070usize, 131071, 131072] {
        cases.push(format!("local x\n{}", rep("x = function() end\n", n)));
    }
    cases.push(rep("(", 300) + "1" + &rep(")", 300));
    cases.push(rep("do ", 250));
    cases.push(format!("x = {}1{}", rep("{", 250), rep("}", 250)));
    cases.push(format!("x = {}1", rep("- ", 250)));
    cases.push(format!("x = 1{}", rep(" .. 1", 300)));
    cases.push(format!("x = {}1{}", rep("f(", 250), rep(")", 250)));
    cases.push(format!("{}x = 1", rep("a, ", 300)));
    cases.push(
        (0..250)
            .map(|i| format!("local a{i}"))
            .collect::<Vec<_>>()
            .join("\n"),
    );
    cases.push(format!(
        "local {}",
        (0..250)
            .map(|i| format!("a{i}"))
            .collect::<Vec<_>>()
            .join(", ")
    ));
    cases.push(format!(
        "local function f()\n{}\nreturn function() return {} end end",
        (0..150)
            .map(|i| format!("local a{i}"))
            .collect::<Vec<_>>()
            .join("\n"),
        (0..150)
            .map(|i| format!("a{i}"))
            .collect::<Vec<_>>()
            .join(" + ")
    ));
    cases.push(format!(
        "{}\nreturn function() {} end",
        (0..180)
            .map(|i| format!("local a{i}"))
            .collect::<Vec<_>>()
            .join("\n"),
        (0..300).map(|_| "return 1\n").collect::<String>()
    ));
    cases.push(format!(
        "local x\n{}",
        (0..300)
            .map(|_| "local function f() return x end\n")
            .collect::<String>()
    ));
    cases.push(format!(
        "local t = ...\nreturn {}",
        (0..300)
            .map(|i| format!("t.k{i}"))
            .collect::<Vec<_>>()
            .join(", ")
    ));
    cases.push(format!(
        "local f = ...\nf({})",
        (0..300)
            .map(|i| i.to_string())
            .collect::<Vec<_>>()
            .join(", ")
    ));
    cases.push(format!(
        "return {}",
        (0..300)
            .map(|i| i.to_string())
            .collect::<Vec<_>>()
            .join(", ")
    ));
    cases.push(format!(
        "local {} = ...",
        (0..201)
            .map(|i| format!("v{i}"))
            .collect::<Vec<_>>()
            .join(", ")
    ));
    cases.push(format!(
        "function f({}) end",
        (0..255)
            .map(|i| format!("p{i}"))
            .collect::<Vec<_>>()
            .join(", ")
    ));
    assert!(cases.len() >= 100, "{}", cases.len());
    let (mut pass, mut rejected_by_ref) = (0, 0);
    let mut fails = Vec::new();
    for c in &cases {
        let want = reference(&luac, c.as_bytes());
        let got = ours(c.as_bytes());
        if want.is_err() {
            rejected_by_ref += 1;
        }
        if want == got {
            pass += 1;
        } else {
            let short: String = c.chars().take(80).collect();
            fails.push(format!(
                "input {short:?}\n  ref : {want:?}\n  ours: {got:?}"
            ));
        }
    }
    eprintln!(
        "syntax errors: {pass}/{} identical ({rejected_by_ref} rejected by the reference)",
        cases.len()
    );
    for f in &fails {
        eprintln!("FAIL {f}");
    }
    assert!(fails.is_empty());
    assert!(rejected_by_ref >= 100);
}

#[test]
fn deep_nesting_is_limited_not_overflowing() {
    // run on a deliberately small stack: the 200-level limit must trigger first
    for src in [
        rep("(", 100_000) + "1" + &rep(")", 100_000),
        rep("do ", 100_000),
        format!("x = {}1{}", rep("{", 100_000), rep("}", 100_000)),
        format!("x = {}1", rep("-", 100_000)),
        format!("x = {}", rep("function() return ", 100_000)),
        rep("if x then ", 100_000),
        format!("x = {}", rep("f(", 100_000)),
        format!("local a = {}", rep("a.b[", 100_000)),
        rep("a, ", 100_000) + "b = 1",
        format!("x = {}", rep("not ", 100_000)) + "1",
        format!("x = 1{}", rep(" ^ 1", 100_000)),
        rep("for i = 1, 2 do ", 100_000),
        format!("global x = {}", rep("{", 100_000)),
        rep("global a, ", 50_000) + "b = 1",
    ] {
        let r = ours(src.as_bytes());
        assert!(r.is_err());
    }
    assert_eq!(
        ours(rep("do ", 100_000).as_bytes()).unwrap_err(),
        "C stack overflow"
    );
}

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
}

#[test]
#[cfg_attr(
    miri,
    ignore = "needs the reference luac/lua processes or the corpus files"
)]
fn fuzz_lite_never_panics() {
    let toks = [
        "local", "function", "end", "if", "then", "else", "elseif", "while", "do", "for", "in",
        "repeat", "until", "return", "break", "goto", "::", "x", "y", "f", "t", "...", "(", ")",
        "{", "}", "[", "]", ",", ";", "=", "==", "+", "-", "*", "/", "//", "..", "<", ">", "<=",
        "~=", "<<", ">>", "&", "|", "~", "#", "not", "and", "or", "nil", "true", "false", "1",
        "2.5", "0x10", "'s'", "[[l]]", ".", ":", "global", "<const>", "<close>", "*", "<", "a",
        "b", "...t", "lbl",
    ];
    let mut rng = Rng(0x1234_5678_9ABC_DEF1);
    let mut cases = 0;
    let mut ok = 0;
    // token soup
    for _ in 0..6000 {
        let n = 1 + rng.next() % 40;
        let mut s = String::new();
        for _ in 0..n {
            s.push_str(toks[(rng.next() as usize) % toks.len()]);
            s.push(' ');
        }
        if ours(s.as_bytes()).is_ok() {
            ok += 1;
        }
        cases += 1;
    }
    // mutated corpus files
    let files: Vec<_> = corpus()
        .into_iter()
        .filter(|f| {
            std::fs::metadata(f)
                .map(|m| m.len() < 6000)
                .unwrap_or(false)
        })
        .collect();
    for f in files.iter().cycle().take(3000) {
        let mut b = std::fs::read(f).unwrap();
        if b.is_empty() {
            continue;
        }
        for _ in 0..1 + rng.next() % 4 {
            let i = (rng.next() as usize) % b.len();
            match rng.next() % 3 {
                0 => b[i] = rng.next() as u8,
                1 => {
                    b.remove(i);
                }
                _ => {
                    let t = toks[(rng.next() as usize) % toks.len()];
                    for (k, c) in t.bytes().enumerate() {
                        b.insert(i + k, c);
                    }
                }
            }
            if b.is_empty() {
                break;
            }
        }
        if b.first() == Some(&0x1b) {
            b[0] = b' ';
        }
        let _ = ours(&b);
        cases += 1;
    }
    eprintln!("fuzz-lite: {cases} cases ({ok} compiled)");
    assert!(cases >= 9000);
}
