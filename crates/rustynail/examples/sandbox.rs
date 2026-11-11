//! Running untrusted code: safe libraries only, no binary chunks, a memory cap and an
//! instruction budget that scripts cannot escape with `pcall`.
//!
//! `cargo run -p rustynail --example sandbox`
use rustynail::{ErrorKind, Lua};

/// Run `src` in a fresh sandbox; returns the outcome as text.
fn run_untrusted(src: &str) -> String {
    let mut lua = match Lua::builder()
        .sandbox() // StdLib::SAFE, text-only loading, no dofile/loadfile
        .memory_limit(16 << 20) // 16 MiB for everything the script allocates
        .instruction_limit(5_000_000) // a few milliseconds of Lua code
        .build()
    {
        Ok(l) => l,
        Err(e) => return format!("cannot create state: {e}"),
    };
    // no output from untrusted code
    if lua
        .push_nil()
        .and_then(|_| lua.set_global("print"))
        .is_err()
    {
        return "setup failed".into();
    }
    match lua.do_string(src) {
        Ok(n) => {
            let r = if n > 0 {
                lua.tostring(-1).unwrap_or_default()
            } else {
                Vec::new()
            };
            format!("ok: {}", String::from_utf8_lossy(&r))
        }
        Err(e) => match e.kind() {
            ErrorKind::InstructionLimit => "stopped: instruction limit".into(),
            ErrorKind::Memory => "stopped: memory limit".into(),
            _ => format!("error: {e}"),
        },
    }
}

fn run() -> Vec<String> {
    let scripts = [
        "local s = 0 for i = 1, 100 do s = s + i end return s",
        "while true do end",
        "repeat pcall(function() while true do end end) until false",
        "local t = {} for i = 1, 1e8 do t[i] = ('x'):rep(100) .. i end",
        "return io.open('/etc/passwd')",
        "return load(string.dump(function() end))",
    ];
    let mut out = Vec::new();
    for s in scripts {
        let r = run_untrusted(s);
        println!("{s:65} => {r}");
        out.push(r);
    }
    out
}

fn main() {
    run();
}

#[test]
fn example_runs() {
    let out = run();
    assert_eq!(out[0], "ok: 5050");
    assert_eq!(out[1], "stopped: instruction limit");
    assert_eq!(out[2], "stopped: instruction limit");
    assert_eq!(out[3], "stopped: memory limit");
    assert!(
        out[4].contains("attempt to index a nil value (global 'io')"),
        "{}",
        out[4]
    );
    // load refuses the binary chunk: it returns nil and this message
    assert_eq!(out[5], "ok: attempt to load a binary chunk (mode is 't')");
}
