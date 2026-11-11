//! `cargo xtask <cmd>` — build/test/verification orchestration.
mod official;
use std::path::{Path, PathBuf};
use std::process::{exit, Command};

pub(crate) fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask has a parent")
        .to_path_buf()
}

fn run(cmd: &mut Command) {
    eprintln!("+ {cmd:?}");
    let st = cmd.status().unwrap_or_else(|e| {
        eprintln!("failed to spawn: {e}");
        exit(1)
    });
    if !st.success() {
        exit(st.code().unwrap_or(1));
    }
}

/// Build the reference C Lua for `ver` ("5.4" | "5.5") into ref/<54|55>/build.
fn ref_build(ver: &str) {
    let short = ver.replace('.', "");
    let src = root().join(format!("vendor/lua-{ver}/src"));
    let dst = root().join(format!("ref/{short}/build"));
    assert!(src.is_dir(), "missing {src:?}");
    let _ = std::fs::remove_dir_all(&dst);
    std::fs::create_dir_all(&dst).expect("mkdir");
    run(Command::new("cp")
        .arg("-r")
        .arg(format!("{}/.", src.display()))
        .arg(&dst));
    run(Command::new("make")
        .args(["-j", "16", "linux"])
        .current_dir(&dst));
    // Deterministic-seed variant (string hash seed fixed to 0) + `refrun` driver, for iteration-order differentials.
    let dst0 = root().join(format!("ref/{short}/build-seed0"));
    let _ = std::fs::remove_dir_all(&dst0);
    std::fs::create_dir_all(&dst0).expect("mkdir");
    run(Command::new("cp")
        .arg("-r")
        .arg(format!("{}/.", src.display()))
        .arg(&dst0));
    run(Command::new("make")
        .args(["-j", "16", "linux", "MYCFLAGS=-Dluai_makeseed\\(L\\)=0"])
        .current_dir(&dst0));
    run(Command::new("gcc")
        .args(["-O2", "-o"])
        .arg(dst0.join("refrun"))
        .arg(root().join("tools/refrun.c"))
        .arg(format!("-I{}", src.display()))
        .arg(dst0.join("liblua.a"))
        .args(["-lm", "-ldl"]));
}

/// Pass `--cfg rustynail_luaXY` to rustc for the requested version.
fn cargo_with_version(ver: &str, args: &[String]) {
    let cfg = if ver == "5.4" {
        "rustynail_lua54"
    } else {
        "rustynail_lua55"
    };
    run(Command::new("cargo")
        .args(args)
        .env("RUSTFLAGS", format!("--cfg {cfg}")));
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let ver = args
        .iter()
        .position(|a| a == "--lua-version")
        .and_then(|i| args.get(i + 1))
        .map_or("5.5", String::as_str)
        .to_string();
    match args.first().map(String::as_str) {
        Some("ref-build") => {
            if args.iter().any(|a| a == "--all") {
                ref_build("5.4");
                ref_build("5.5");
            } else {
                ref_build(&ver);
            }
        }
        Some("check") => cargo_with_version(&ver, &["check".into(), "--workspace".into()]),
        Some("test") => cargo_with_version(
            &ver,
            &["nextest".into(), "run".into(), "--workspace".into()],
        ),
        Some("ci") => {
            run(Command::new("cargo").args(["fmt", "--all", "--check"]));
            for v in ["5.5", "5.4"] {
                let cfg = if v == "5.4" {
                    "rustynail_lua54"
                } else {
                    "rustynail_lua55"
                };
                run(Command::new("cargo")
                    .args([
                        "clippy",
                        "--workspace",
                        "--all-targets",
                        "--",
                        "-D",
                        "warnings",
                    ])
                    .env("RUSTFLAGS", format!("--cfg {cfg}")));
                cargo_with_version(v, &["nextest".into(), "run".into(), "--workspace".into()]);
            }
        }
        Some("official") => official::run(&ver, &args),
        Some("diff-run") => official::diff_run(&ver, &args),
        _ => {
            eprintln!(
                "usage: cargo xtask <ref-build [--all]|check|test|ci|official|diff-run> [--lua-version 5.4|5.5]\n\
                 \x20 official [--release] [--timeout SECS] [--only a.lua,b.lua]\n\
                 \x20 diff-run [--release] [--timeout SECS]"
            );
            exit(2);
        }
    }
}
