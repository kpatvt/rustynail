#![allow(dead_code)]
//! Shared helpers for the differential tests against the reference interpreters.
use std::path::{Path, PathBuf};
use std::process::Command;

pub const VER: &str = if cfg!(rustynail_lua54) { "54" } else { "55" };

pub fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Reference `lua` (`build-seed0/refrun` when `seed0`), `None` (with a notice) if not built.
pub fn ref_lua(seed0: bool) -> Option<PathBuf> {
    let p = root().join(format!(
        "ref/{VER}/{}",
        if seed0 {
            "build-seed0/refrun"
        } else {
            "build/lua"
        }
    ));
    if p.exists() {
        Some(p)
    } else {
        eprintln!(
            "SKIP: {} missing (run `cargo xtask ref-build --all`)",
            p.display()
        );
        None
    }
}

/// Deterministic xorshift64* generator.
pub struct Rng(pub u64);
impl Rng {
    pub fn new(seed: u64) -> Self {
        Rng(seed.wrapping_mul(0x9E3779B97F4A7C15) | 1)
    }
    pub fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545F4914F6CDD1D)
    }
    /// Uniform-ish in `0..n`.
    pub fn below(&mut self, n: u64) -> u64 {
        self.next() % n.max(1)
    }
    pub fn range(&mut self, lo: i64, hi: i64) -> i64 {
        lo + self.below((hi - lo + 1) as u64) as i64
    }
    pub fn chance(&mut self, num: u64, den: u64) -> bool {
        self.below(den) < num
    }
    pub fn pick<'a, T>(&mut self, xs: &'a [T]) -> &'a T {
        &xs[self.below(xs.len() as u64) as usize]
    }
}

pub fn hex(b: &[u8]) -> String {
    let mut s = String::with_capacity(b.len() * 2);
    for c in b {
        s.push_str(&format!("{c:02x}"));
    }
    s
}

pub fn unhex(s: &str) -> Vec<u8> {
    (0..s.len() / 2)
        .map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).unwrap())
        .collect()
}

/// Run `lua_src` with the reference interpreter; `data` is written next to it as `data.txt`
/// (the script reads it from `arg[1]`). Returns stdout lines.
pub fn run_ref(
    name: &str,
    seed0: bool,
    lua_src: &str,
    data: &str,
    env: &[(&str, &str)],
) -> Option<Vec<String>> {
    let exe = ref_lua(seed0)?;
    let dir =
        Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("{name}-{VER}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let script = dir.join("run.lua");
    let datap = dir.join("data.txt");
    std::fs::write(&script, lua_src).unwrap();
    std::fs::write(&datap, data).unwrap();
    let mut cmd = Command::new(exe);
    cmd.arg(&script).arg(&datap);
    for (k, v) in env {
        cmd.env(k, v);
    }
    let out = cmd.output().unwrap();
    assert!(
        out.status.success(),
        "reference run failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let _ = std::fs::remove_dir_all(&dir);
    let s = String::from_utf8(out.stdout).expect("utf8 output");
    Some(s.lines().map(|l| l.to_string()).collect())
}

/// Compare two line lists, panicking with up to 10 mismatches (case = input line).
pub fn compare(what: &str, cases: &[String], reference: &[String], ours: &[String]) {
    assert_eq!(reference.len(), ours.len(), "{what}: line count mismatch");
    assert_eq!(reference.len(), cases.len(), "{what}: case count mismatch");
    let mut bad = 0;
    for i in 0..cases.len() {
        if reference[i] != ours[i] {
            bad += 1;
            if bad <= 10 {
                eprintln!(
                    "MISMATCH {what} case {i}: {}\n  ref : {}\n  ours: {}",
                    cases[i], reference[i], ours[i]
                );
            }
        }
    }
    assert_eq!(bad, 0, "{what}: {bad} mismatches out of {}", cases.len());
    eprintln!("{what}: {} cases identical", cases.len());
}
