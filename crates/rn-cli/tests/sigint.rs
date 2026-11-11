//! Ctrl-C handling of the stand-alone interpreter (`lua.c`'s `laction`/`lstop`).
use std::io::Read;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

fn run_and_interrupt(script: &str) -> (Option<i32>, String) {
    // tests run on parallel threads of one process: give every call its own directory
    static SEQ: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let seq = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("rn_sigint_{}_{seq}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("s.lua");
    std::fs::write(&path, script).unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_rnlua"))
        .arg(&path)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    // wait until the script announced it is running, then interrupt it
    let mut out = child.stdout.take().unwrap();
    let mut buf = [0u8; 16];
    let n = out.read(&mut buf).unwrap();
    assert!(n > 0, "script did not start");
    // SAFETY: plain `kill` of our own child.
    unsafe { libc::kill(child.id() as libc::pid_t, libc::SIGINT) };
    let start = Instant::now();
    let status = loop {
        if let Some(s) = child.try_wait().unwrap() {
            break s;
        }
        if start.elapsed() > Duration::from_secs(20) {
            child.kill().unwrap();
            panic!("interpreter did not stop after SIGINT");
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    let mut err = String::new();
    child
        .stderr
        .take()
        .unwrap()
        .read_to_string(&mut err)
        .unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    (status.code(), err)
}

#[test]
fn sigint_interrupts_a_busy_loop() {
    let (code, err) = run_and_interrupt(
        "io.write('x'); io.stdout:flush(); local i = 0 while true do i = i + 1 end",
    );
    assert_eq!(code, Some(1), "{err}");
    assert!(err.contains("interrupted!"), "{err}");
    assert!(err.contains("stack traceback:"), "{err}");
}

#[test]
fn sigint_error_can_be_caught_by_pcall() {
    let (code, err) = run_and_interrupt(
        "local ok, e = pcall(function() io.write('x'); io.stdout:flush(); while true do end end) \
         io.stderr:write('caught: ', tostring(ok), ' ', tostring(e), '\\n')",
    );
    assert_eq!(code, Some(0), "{err}");
    assert!(err.contains("caught: false interrupted!"), "{err}");
}
