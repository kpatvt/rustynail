//! `rnvmrun [--stress] file.lua`: run a script with the real interpreter and the minimal native test
//! library (`tests/support`). Mirrors the reference runner's observable behaviour (stdout, the
//! error message on stderr, exit status) for differential testing.
// The 5.4 executor is not ported yet (Phase 6b): this tool only exists for 5.5.
#[cfg(not(rustynail_lua54))]
#[path = "../../tests/support/mod.rs"]
mod support;

#[cfg(rustynail_lua54)]
fn main() {
    eprintln!("rnvmrun: the Lua 5.4 interpreter is not implemented yet");
    std::process::exit(2);
}

#[cfg(not(rustynail_lua54))]
struct Flush;
#[cfg(not(rustynail_lua54))]
impl Drop for Flush {
    fn drop(&mut self) {
        support::flush();
    }
}

#[cfg(not(rustynail_lua54))]
fn main() {
    let mut stress = std::env::var_os("RUSTYNAIL_GC_STRESS").is_some_and(|v| v == "1");
    let mut file = None;
    for a in std::env::args().skip(1) {
        if a == "--stress" {
            stress = true;
        } else {
            file = Some(a);
        }
    }
    let Some(file) = file else {
        eprintln!("usage: rnvmrun [--stress] file.lua");
        std::process::exit(2);
    };
    let src = match std::fs::read(&file) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("cannot open {file}: {e}");
            std::process::exit(1);
        }
    };
    let src: &[u8] = if src.first() == Some(&b'#') {
        // skip a shebang line, keep the newline so line numbers stay right
        let nl = src.iter().position(|&c| c == b'\n').unwrap_or(src.len());
        &src[nl..]
    } else {
        &src
    };
    support::stream_to_stdout();
    let guard = Flush;
    let r = support::run_chunk(src, format!("@{file}").as_bytes(), stress);
    drop(guard);
    if let Err(m) = r {
        use std::io::Write;
        let mut e = std::io::stderr();
        let _ = e.write_all(&m);
        let _ = e.write_all(b"\n");
        std::process::exit(1);
    }
}
