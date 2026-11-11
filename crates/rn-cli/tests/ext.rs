//! The LuaJIT-compatible extensions (docs/EXTENSIONS.md): every `tests/ext/NAME.lua` runs through `rnlua` and its
//! output must equal the checked-in `NAME.expected`. The expectations of the non-`rn_` programs were produced by
//! LuaJIT 2.1 (`scripts/ext-diff.sh` re-checks them against /usr/bin/luajit when it is installed), so this test pins
//! rnlua to LuaJIT's behaviour without needing LuaJIT.
use std::path::PathBuf;
use std::process::{Command, Stdio};

fn dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/ext")
}

/// Replaces `0x<hex>` addresses (6 or more digits) by `0xADDR`.
fn norm(s: &str) -> String {
    let mut out = String::new();
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'0' && i + 1 < b.len() && b[i + 1] == b'x' {
            let mut j = i + 2;
            while j < b.len() && (b[j].is_ascii_digit() || (b'a'..=b'f').contains(&b[j])) {
                j += 1;
            }
            if j - (i + 2) >= 6 {
                out.push_str("0xADDR");
                i = j;
                continue;
            }
        }
        out.push(b[i] as char);
        i += 1;
    }
    out
}

fn run_one(name: &str, env: &[(&str, &str)]) {
    let d = dir();
    let mut c = Command::new(env!("CARGO_BIN_EXE_rnlua"));
    c.arg(format!("{name}.lua"))
        .current_dir(&d)
        .stdin(Stdio::null());
    for (k, v) in env {
        c.env(k, v);
    }
    let o = c.output().expect("run rnlua");
    let got = norm(
        &(String::from_utf8_lossy(&o.stdout).into_owned() + &String::from_utf8_lossy(&o.stderr)),
    );
    let want = std::fs::read_to_string(d.join(format!("{name}.expected"))).expect("expected file");
    // (the script prints through `print`: one trailing newline; the expectation file has one more from the shell `printf`)
    assert_eq!(
        got.trim_end(),
        want.trim_end(),
        "{name} {env:?}: output differs from {name}.expected"
    );
}

fn names() -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(dir())
        .unwrap()
        .filter_map(|e| {
            let n = e.unwrap().file_name().into_string().unwrap();
            let stem = n.strip_suffix(".lua")?.to_string();
            // (`rn_buffer_ext.expected` was produced by the 5.5 build: it prints floats with `lua_Number2str`'s shortest round-trip form
            // (`1.8446744073709552e+19`), the 5.4 `%.14g` gives `1.844674407371e+19`; the number formatting of the two versions is not what the
            // test is about, and the LuaJIT-derived `buf_*` / `tbl_*` programs run under both)
            (!stem.starts_with('_') && !(cfg!(rustynail_lua54) && stem == "rn_buffer_ext"))
                .then_some(stem)
        })
        .collect();
    v.sort();
    v
}

#[test]
fn ext_programs_match_expectations() {
    let n = names();
    assert!(n.len() >= 8, "{n:?}");
    for name in &n {
        run_one(name, &[]);
    }
}

#[test]
fn ext_programs_under_jit_and_gc_modes() {
    for name in names() {
        for env in [
            &[("RUSTYNAIL_JIT", "off")][..],
            &[("RUSTYNAIL_JIT", "always")],
            &[("RUSTYNAIL_JIT_TRACE", "always")],
            &[("RUSTYNAIL_GC_MODE", "inc")],
            &[("RUSTYNAIL_GC_STRESS", "2")],
        ] {
            run_one(&name, env);
        }
    }
}
