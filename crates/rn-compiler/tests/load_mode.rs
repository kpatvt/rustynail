//! `load` mode-string errors are printed exactly as the caller wrote the mode (`checkmode` in ldo.c).
mod common;
use common::*;
use rn_compiler::testhost::TestHost;
use rn_compiler::{load_with_mode_str, LoadMode};
use std::process::Command;

const MODES: &[&str] = &[
    "b", "t", "bt", "tb", "x", "", "bb", "tt", "T", "btx", "xyz", "t\0b",
];

#[test]
#[cfg_attr(
    miri,
    ignore = "needs the reference luac/lua processes or the corpus files"
)]
fn load_mode_errors_match_reference() {
    let Some(lua) = lua() else { return };
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("mode-{VER}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut script = String::from("local bin = string.dump(load('x = 1'))\nlocal function r(chunk, mode) local f, e = load(chunk, '=s', mode) print(f and 'OK' or e) end\n");
    let mut ours = Vec::new();
    for mode in MODES {
        let lit = mode.replace('\0', "\\0");
        script.push_str(&format!("r('x = 1', \"{lit}\")\nr(bin, \"{lit}\")\n"));
        for (chunk, bin) in [(&b"x = 1"[..], false), (&b"\x1bLua"[..], true)] {
            let mut host = TestHost::default();
            let r = load_with_mode_str(&mut host, chunk, b"=s", Some(mode.as_bytes()));
            ours.push(match r {
                Err(e) if e.message.starts_with(b"attempt to load") => {
                    String::from_utf8_lossy(&e.message).into_owned()
                }
                _ => "OK".into(),
            });
            let _ = bin;
        }
    }
    std::fs::write(dir.join("m.lua"), script).unwrap();
    let out = Command::new(lua).arg(dir.join("m.lua")).output().unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let refs: Vec<String> = String::from_utf8(out.stdout)
        .unwrap()
        .lines()
        .map(|s| s.to_string())
        .collect();
    assert_eq!(refs, ours);
    // the LoadMode-based entry point still works
    let mut host = TestHost::default();
    assert!(rn_compiler::load(&mut host, b"x = 1", b"=s", LoadMode::BT).is_ok());
}
