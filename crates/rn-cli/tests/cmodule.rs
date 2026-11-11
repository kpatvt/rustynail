//! A prebuilt C module (`tests/c/samplemod.c`, linked against nothing) is loaded with `require`:
//! the interpreter must export the C API for it. The output must equal the reference `lua`'s, also
//! under the collector stress modes.
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

fn build_module(dir: &Path) {
    let out = dir.join("samplemod.so");
    let status = Command::new("cc")
        .args(["-shared", "-fPIC", "-O1", "-w"])
        .arg(format!(
            "-I{}",
            root()
                .join(if cfg!(rustynail_lua54) {
                    "vendor/lua-5.4/src"
                } else {
                    "vendor/lua-5.5/src"
                })
                .display()
        ))
        .arg("-o")
        .arg(&out)
        .arg(root().join("tests/c/samplemod.c"))
        .status()
        .expect("cc not available");
    assert!(status.success());
}

fn run(exe: &Path, dir: &Path, env: &[(&str, &str)]) -> (Option<i32>, String) {
    let mut cmd = Command::new("timeout");
    cmd.arg("120")
        .arg(exe)
        .arg(root().join("tests/c/samplemod.lua"))
        .env("LUA_CPATH", format!("{}/?.so", dir.display()))
        .stdin(Stdio::null());
    for (k, v) in env {
        cmd.env(k, v);
    }
    let o = cmd.output().unwrap();
    let mut s = String::from_utf8_lossy(&o.stdout).into_owned();
    s.push_str(&String::from_utf8_lossy(&o.stderr));
    (o.status.code(), s)
}

#[test]
fn c_module_loads_and_matches_the_reference() {
    let dir = std::env::temp_dir().join(format!("rn_cmod_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    build_module(&dir);
    let ours = Path::new(env!("CARGO_BIN_EXE_rnlua"));
    let (code, out) = run(ours, &dir, &[]);
    assert_eq!(code, Some(0), "{out}");
    let banner = if cfg!(rustynail_lua54) {
        "504.0\tLua 5.4"
    } else {
        "505.0\tLua 5.5"
    };
    assert!(out.contains(banner), "{out}");
    if let Some(reference) = Some(root().join(if cfg!(rustynail_lua54) {
        "ref/54/build-seed0/lua"
    } else {
        "ref/55/build-seed0/lua"
    }))
    .filter(|p| p.exists())
    {
        let (rc, rout) = run(&reference, &dir, &[]);
        assert_eq!(rc, Some(0), "{rout}");
        assert_eq!(out, rout, "output differs from the reference lua");
    }
    // the same under collector stress (full collections, then one step/young collection per
    // safepoint with heap verification), in both collector modes
    for (stress, verify, mode) in [("1", "0", "gen"), ("2", "1", "gen"), ("2", "1", "inc")] {
        let (c, o) = run(
            ours,
            &dir,
            &[
                ("RUSTYNAIL_GC_STRESS", stress),
                ("RUSTYNAIL_GC_VERIFY", verify),
                ("RUSTYNAIL_GC_MODE", mode),
            ],
        );
        assert_eq!(c, Some(0), "stress {stress}/{mode}: {o}");
        assert_eq!(o, out, "stress {stress}/{mode}: output changed");
    }
    let _ = std::fs::remove_dir_all(&dir);
}
