#![allow(dead_code)]
use std::path::{Path, PathBuf};
use std::process::Command;

/// Version directory name of the build under test (`ref/<VER>/build`).
pub const VER: &str = if cfg!(rustynail_lua54) { "54" } else { "55" };

pub fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Path of the reference `luac` if built; prints a notice and returns `None` otherwise.
pub fn luac() -> Option<PathBuf> {
    let p = root().join(format!("ref/{VER}/build/luac"));
    if p.exists() {
        Some(p)
    } else {
        eprintln!(
            "SKIP: {} missing (run `cargo xtask ref-build`)",
            p.display()
        );
        None
    }
}

pub fn lua() -> Option<PathBuf> {
    let p = root().join(format!("ref/{VER}/build/lua"));
    if p.exists() {
        Some(p)
    } else {
        eprintln!("SKIP: {} missing", p.display());
        None
    }
}

/// All `.lua` files in the vendored test-suite for the active version plus `tests/lua` (recursively;
/// `tests/lua/gen` and `tests/lua/only55` are 5.5-only, `tests/lua/gen54` is 5.4-only).
pub fn corpus() -> Vec<PathBuf> {
    fn walk(dir: &Path, v: &mut Vec<PathBuf>) {
        if let Ok(rd) = std::fs::read_dir(dir) {
            for e in rd.flatten() {
                let p = e.path();
                let name = e.file_name();
                let skip: &[&str] = if cfg!(rustynail_lua54) {
                    &["gen", "only55"]
                } else {
                    &["gen54"]
                };
                if p.is_dir() {
                    if skip.iter().any(|s| name == *s) && dir.ends_with("tests/lua") {
                        continue;
                    }
                    walk(&p, v);
                } else if p.extension().is_some_and(|x| x == "lua") {
                    v.push(p);
                }
            }
        }
    }
    let mut v = Vec::new();
    for dir in [
        format!(
            "vendor/lua-tests-{}",
            if cfg!(rustynail_lua54) { "5.4" } else { "5.5" }
        ),
        "tests/lua".to_string(),
    ] {
        walk(&root().join(dir), &mut v);
    }
    v.sort();
    v
}

/// Compile `file` with the reference `luac`, optionally stripped. `None` if luac rejects it.
pub fn luac_compile(luac: &Path, file: &Path, strip: bool) -> Option<Vec<u8>> {
    static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let tag = format!(
        "{}-{}-{}-{}",
        std::process::id(),
        N.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
        file.file_name()?.to_string_lossy(),
        strip
    );
    let out = std::env::temp_dir().join(format!("rn-a1-{tag}.luac"));
    let mut cmd = Command::new(luac);
    if strip {
        cmd.arg("-s");
    }
    let st = cmd.arg("-o").arg(&out).arg(file).output().ok()?;
    if !st.status.success() {
        return None;
    }
    let b = std::fs::read(&out).ok();
    let _ = std::fs::remove_file(&out);
    b
}
