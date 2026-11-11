//! With the `trace` feature: slice the differential harness (emulator `World`, helpers, `compile`, `diff_case`, the
//! native runner) out of `crates/rn-jit/tests/tr_asm.rs` (everything before its first `#[test]`, minus the inner
//! attributes) into `$OUT_DIR/tr_asm_harness.rs`, so the `trace_ir` fuzz target drives exactly the code the test suite
//! drives and cannot drift from it. See `src/trace_harness_ext.rs`.
use std::{env, fs, path::PathBuf};

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    let src = PathBuf::from("../crates/rn-jit/tests/tr_asm.rs");
    println!("cargo:rerun-if-changed={}", src.display());
    if env::var_os("CARGO_FEATURE_TRACE").is_none() {
        return;
    }
    let text = fs::read_to_string(&src).expect("read tests/tr_asm.rs");
    let mut out = String::new();
    for line in text.lines() {
        if line == "#[test]" {
            break;
        }
        if line.starts_with("//!") || line.starts_with("#![") {
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    let dst = PathBuf::from(env::var("OUT_DIR").unwrap()).join("tr_asm_harness.rs");
    fs::write(dst, out).expect("write harness");
}
