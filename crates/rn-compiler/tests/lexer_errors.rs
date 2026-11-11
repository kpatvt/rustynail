//! Lexical error messages compared with the reference `luac -p -` (skips if the reference is missing).
mod common;
use common::*;
use rn_compiler::lexer::{chunk_id, LexState, Token};
use rn_compiler::testhost::TestHost;
use std::io::Write;
use std::process::{Command, Stdio};

/// Scan the whole input; `Ok(())` if it lexes cleanly, else the error message.
fn lex_all(src: &[u8], name: &[u8]) -> Result<(), String> {
    let mut host = TestHost::default();
    let mut ls = LexState::new(&mut host, src, name).map_err(|e| format!("{:?}", e.message))?;
    loop {
        ls.next_token()
            .map_err(|e| String::from_utf8_lossy(&e.message).into_owned())?;
        if ls.t == Token::Eos {
            return Ok(());
        }
    }
}

fn reference(luac: &std::path::Path, src: &[u8]) -> String {
    let mut c = Command::new(luac)
        .args(["-p", "-"])
        .stdin(Stdio::piped())
        .stderr(Stdio::piped())
        .stdout(Stdio::null())
        .spawn()
        .unwrap();
    c.stdin.take().unwrap().write_all(src).unwrap();
    let out = c.wait_with_output().unwrap();
    let e = String::from_utf8_lossy(&out.stderr).into_owned();
    e.trim_end()
        .strip_prefix(&format!("{}: ", luac.display()))
        .unwrap_or(e.trim_end())
        .to_string()
}

#[test]
#[cfg_attr(
    miri,
    ignore = "needs the reference luac/lua processes or the corpus files"
)]
fn lexical_errors_match_reference() {
    let Some(luac) = luac() else { return };
    let cases: Vec<&[u8]> = vec![
        b"x = \"abc",
        b"x = 'abc\ndef'",
        b"x = \"abc\\",
        b"x = \"a\\qb\"",
        b"x = \"a\\xZZ\"",
        b"x = \"a\\x1\"",
        b"x = \"a\\x\"",
        b"x = \"\\300\"",
        b"x = \"\\256\"",
        b"x = \"\\u{110000000}\"",
        b"x = \"\\u{7FFFFFFF}\\u{80000000}\"",
        b"x = \"\\u{}\"",
        b"x = \"\\u123\"",
        b"x = \"\\u{12\"",
        b"x = \"\\u{12Z}\"",
        b"x = [==",
        b"x = [=",
        b"x = [==x",
        b"x = [[never closed",
        b"x = [==[ never ]=] closed\n\n",
        b"x = [[a\nb\nc",
        b"--[[ unfinished comment",
        b"--[==[ unfinished\n\n\n comment ]=]",
        b"x = 3x",
        b"x = 0x",
        b"x = 0xg",
        b"x = 1e",
        b"x = 1e+",
        b"x = 1..2",
        b"x = 0x1p",
        b"x = 08a",
        b"x = 3.4.5",
        b"x = .5.5",
        b"x = 0x.p1",
        b"x = 1e5e",
        b"x = 12abc",
        b"x = 1_000",
        b"x = 99999999999999999999999999999999999999999999999999999999999999e999999 3z",
        b"x = 0xep+",
        b"\n\n\nx = \"line4",
        b"x = 'a\rb'",
        b"x = \"\\z  \n\n  \\q\"",
        b"x = \"abc\\\ndef\\\r\nghi\\q\"",
        b"x = \"tab\there\\k\"",
        b"x = \"\0",
        b"x = \"nul\0inside\\q\"",
        b"x = \"\\xA\"",
        b"x = '\\u{FFFFFFFF}'",
        b"x = \"\\9999\"",
        b"x = 5 ~ ~= 3 \\ 4",
        b"x = \"unfinished at eof \\",
        b"x = 1 --[=[ comment\nstill\n",
        b"x = [[\n\n",
        b"local s = '\\\n\\\n\\\n\\p'",
    ];
    assert!(cases.len() >= 40);
    let mut checked = 0;
    for src in cases {
        let want = reference(&luac, src);
        let got = match lex_all(src, b"=stdin") {
            Ok(()) => continue, // lexically valid; the reference error (if any) is a parser error
            Err(m) => m,
        };
        assert_eq!(got, want, "input {:?}", String::from_utf8_lossy(src));
        checked += 1;
    }
    eprintln!("lexer error cases compared: {checked}");
    assert!(checked >= 40);
}

#[test]
#[cfg_attr(
    miri,
    ignore = "needs the reference luac/lua processes or the corpus files"
)]
fn chunk_id_matches_reference() {
    let Some(lua) = lua() else { return };
    let long_at = format!("@{}", "d/".repeat(40));
    let long_eq = format!("={}", "e".repeat(80));
    let long_src = "x".repeat(100);
    let names: Vec<String> = vec![
        "=stdin".into(),
        "@file.lua".into(),
        long_at,
        long_eq,
        "return +".into(),
        "x = = 1\nsecond line".into(),
        long_src,
        format!("@{}", "a".repeat(58)),
        format!("@{}", "a".repeat(59)),
        format!("@{}", "a".repeat(60)),
        format!("={}", "b".repeat(59)),
        format!("={}", "b".repeat(60)),
        "x".repeat(44),
        "x".repeat(45),
        "x".repeat(46),
        "\n".into(),
        "".into(),
    ];
    for name in names {
        let script = "local n = ...; local m = select(2, load('x = = 1', n)); print(m)";
        let out = Command::new(&lua)
            .arg("-e")
            .arg(format!(
                "arg = nil; {}",
                script.replace("...", &format!("{name:?}").replace("\\n", "\\n"))
            ))
            .output()
            .unwrap();
        let line = String::from_utf8_lossy(&out.stdout).to_string();
        let line = line.trim_end_matches('\n');
        let suffix = ":1: unexpected symbol near '='";
        let ref_id = line
            .strip_suffix(suffix)
            .unwrap_or_else(|| panic!("{line}"));
        assert_eq!(
            String::from_utf8_lossy(&chunk_id(name.as_bytes())),
            ref_id,
            "name {name:?}"
        );
    }
}

#[test]
#[cfg_attr(
    miri,
    ignore = "needs the reference luac/lua processes or the corpus files"
)]
fn lexes_whole_corpus() {
    let mut n = 0;
    for f in corpus() {
        let mut src = std::fs::read(&f).unwrap();
        if src.first() == Some(&b'#') {
            let nl = src.iter().position(|&c| c == b'\n').unwrap_or(src.len());
            src.drain(..nl);
        }
        let name = format!("@{}", f.display());
        lex_all(&src, name.as_bytes()).unwrap_or_else(|e| panic!("{e}"));
        n += 1;
    }
    assert!(n > 20);
}
