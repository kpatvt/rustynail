//! F9 (docs/JIT_TRACE.md D3/TR31): the runtime defaults of the stand-alone interpreter. In a build with the `jit`
//! cargo feature `RUSTYNAIL_JIT_TRACE` unset means traces on (and J4 regions as fallback owner); `off` is the
//! documented off switch. Without the feature nothing JIT-related exists and the environment is ignored.
use std::process::{Command, Stdio};

const PROG: &str = "local s = 0 for i = 1, 3000000 do s = s + i % 7 end print(s)\n";

/// Runs the script with a scrubbed JIT environment plus `env`; returns (stdout, stderr).
fn run(env: &[(&str, &str)]) -> (String, String) {
    static SEQ: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let seq = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("rn_jitdef_{}_{seq}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("s.lua");
    std::fs::write(&path, PROG).unwrap();
    let mut c = Command::new(env!("CARGO_BIN_EXE_rnlua"));
    c.arg(&path).stdin(Stdio::null());
    for (k, _) in std::env::vars() {
        if k.starts_with("RUSTYNAIL_JIT") {
            c.env_remove(k);
        }
    }
    c.env("RUSTYNAIL_JIT_VERBOSE", "1");
    for (k, v) in env {
        c.env(k, v);
    }
    let o = c.output().unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    (
        String::from_utf8_lossy(&o.stdout).into_owned(),
        String::from_utf8_lossy(&o.stderr).into_owned(),
    )
}

#[cfg(feature = "jit")]
#[test]
fn traces_are_on_by_default() {
    let (out, err) = run(&[]);
    assert_eq!(out.trim(), "8999997", "{out}");
    assert!(err.contains("[trace] recorded 1 installed 1"), "{err}");
}

#[cfg(feature = "jit")]
#[test]
fn off_switch_restores_j4() {
    let (out, err) = run(&[("RUSTYNAIL_JIT_TRACE", "off")]);
    assert_eq!(out.trim(), "8999997");
    assert!(!err.contains("[trace]"), "{err}");
    assert!(err.contains("[jit]"), "the JIT itself still runs: {err}");
}

#[cfg(feature = "jit")]
#[test]
fn explicit_values_override() {
    for v in ["on", "always"] {
        let (out, err) = run(&[("RUSTYNAIL_JIT_TRACE", v)]);
        assert_eq!(out.trim(), "8999997");
        assert!(err.contains("[trace] recorded"), "{v}: {err}");
    }
    // the whole JIT off wins over the trace default
    let (out, err) = run(&[("RUSTYNAIL_JIT", "off")]);
    assert_eq!(out.trim(), "8999997");
    assert!(!err.contains("[trace]"), "{err}");
}

#[cfg(not(feature = "jit"))]
#[test]
fn build_without_jit_ignores_the_environment() {
    for v in ["", "on", "always", "off"] {
        let (out, err) = if v.is_empty() {
            run(&[])
        } else {
            run(&[("RUSTYNAIL_JIT_TRACE", v)])
        };
        assert_eq!(out.trim(), "8999997");
        assert!(!err.contains("[trace]") && !err.contains("[jit]"), "{err}");
    }
}
