//! Release engineering (BUNDLE W): `--version` and `--license` of `rnlua` and `rnluac` carry the author and licence
//! information (LICENSE, LICENSE-LUA, NOTICE are embedded in the binaries).
use std::process::{Command, Stdio};

fn run(exe: &str, args: &[&str]) -> (bool, String) {
    let o = Command::new(exe)
        .args(args)
        .env_remove("LUA_INIT")
        .stdin(Stdio::null())
        .output()
        .unwrap();
    (
        o.status.success(),
        String::from_utf8_lossy(&o.stdout).into_owned(),
    )
}

const RNLUA: &str = env!("CARGO_BIN_EXE_rnlua");
const RNLUAC: &str = env!("CARGO_BIN_EXE_rnluac");

#[test]
fn rnlua_version_has_reference_line_then_author_line() {
    for flag in ["-v", "--version"] {
        let (ok, out) = run(RNLUA, &[flag]);
        assert!(ok);
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines.len(), 2, "{out}");
        // the first line is the reference's `lua -v` line
        assert!(
            lines[0].starts_with("Lua 5.") && lines[0].contains("Copyright (C) 1994-"),
            "{out}"
        );
        assert!(lines[0].contains("Lua.org, PUC-Rio"), "{out}");
        assert!(lines[1].starts_with("rnlua "), "{out}");
        assert!(
            lines[1].contains("Copyright (c) 2026 Kiran Pashikanti <kpatvt@gmail.com>"),
            "{out}"
        );
        assert!(lines[1].contains("JIT: "), "{out}");
    }
}

#[test]
fn jit_status_in_version_follows_the_build_and_the_environment() {
    let out = Command::new(RNLUA)
        .arg("--version")
        .env("RUSTYNAIL_JIT", "off")
        .stdin(Stdio::null())
        .output()
        .unwrap();
    let out = String::from_utf8_lossy(&out.stdout).into_owned();
    let expect = if cfg!(feature = "jit") {
        "JIT: off"
    } else {
        "JIT: not built"
    };
    assert!(out.contains(expect), "{out}");
    #[cfg(feature = "jit")]
    {
        let o = Command::new(RNLUA)
            .arg("--version")
            .env_remove("RUSTYNAIL_JIT")
            .stdin(Stdio::null())
            .output()
            .unwrap();
        assert!(String::from_utf8_lossy(&o.stdout).contains("JIT: on"));
    }
}

#[test]
fn rnluac_version() {
    for flag in ["-v", "--version"] {
        let (ok, out) = run(RNLUAC, &[flag]);
        assert!(ok);
        assert!(out.starts_with("Lua 5."), "{out}");
        assert!(out.contains("rnluac "), "{out}");
        assert!(out.contains("Kiran Pashikanti"), "{out}");
    }
}

fn check_license(exe: &str) {
    let (ok, out) = run(exe, &["--license"]);
    assert!(ok);
    assert!(out.contains("Kiran Pashikanti"), "author missing");
    assert!(out.contains("Copyright (c) 2026 Kiran Pashikanti <kpatvt@gmail.com>"));
    assert!(out.contains("Lua.org, PUC-Rio"), "Lua attribution missing");
    assert!(out.contains("MIT License"));
    assert!(out
        .contains("Permission is hereby granted, free of charge, to any person obtaining a copy"));
    assert!(out.contains("THE SOFTWARE IS PROVIDED \"AS IS\""));
    assert!(out.contains("libc "), "third-party notices missing");
    // the embedded texts are the files of the repository
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    for f in ["LICENSE", "LICENSE-LUA", "NOTICE"] {
        let text = std::fs::read_to_string(root.join(f)).unwrap();
        assert!(out.contains(&text), "{f} is not embedded verbatim");
    }
}

#[test]
fn rnlua_license() {
    check_license(RNLUA);
}

#[test]
fn rnluac_license() {
    check_license(RNLUAC);
}

#[test]
fn reference_options_are_unchanged() {
    // `--licence`/`--versions` are still errors, like any unknown `--` option in lua.c
    let o = Command::new(RNLUA)
        .arg("--versions")
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(!o.status.success());
    assert!(String::from_utf8_lossy(&o.stderr).contains("unrecognized option '--versions'"));
}
