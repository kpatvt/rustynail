//! `RUSTYNAIL_JIT=on` behaviour with explicit thresholds (own process: thresholds are process-wide), the
//! `RUSTYNAIL_JIT_THRESHOLD` parser, and the silent fallback when executable memory is refused.
#![cfg(feature = "jit")]

use rn_core::state::{LuaState, LUA_MULTRET};
use rn_core::status::Status;
use rn_jit::{Config, Mode, Stats};
use std::sync::Once;

fn init() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let cfg = Config {
            mode: Mode::On,
            entry_calls: 3,
            loop_edges: 50,
        };
        assert!(rn_jit::register_with_trace_pinned_off(cfg));
    });
}

fn run(src: &str) -> (String, Stats) {
    init();
    let mut st: LuaState = rn_std::new_state_with_libs().expect("state");
    let base = st.top();
    assert_eq!(st.load_buffer(src.as_bytes(), b"=t", None), Status::Ok);
    let func = st.top() - 1;
    assert_eq!(st.pcall(func, LUA_MULTRET, 0), Status::Ok);
    let v = st.get(func);
    let out = match (v.as_string(), v.as_number()) {
        (Some(s), _) => String::from_utf8_lossy(s.as_bytes()).into_owned(),
        (None, Some(n)) => {
            String::from_utf8_lossy(rn_core::number::tostring(n).as_bytes()).into_owned()
        }
        _ => panic!("bad result {v:?}"),
    };
    st.set_top(base);
    (out, rn_jit::stats(&mut st))
}

#[test]
fn function_compiles_after_the_entry_threshold() {
    let f = "local function f() local s = 0 for i = 1, 3 do s = s + i end return s end";
    let (r, s) = run(&format!("{f} local a = f() a = f() return a"));
    assert_eq!((r.as_str(), s.compiled), ("6", 0), "{s:?}");
    let (r, s) = run(&format!("{f} local a for i = 1, 6 do a = f() end return a"));
    assert_eq!(r, "6");
    assert_eq!(
        s.compiled, 1,
        "f only (the main chunk is called once): {s:?}"
    );
    // heuristic: a loop-free, mostly templated function with a function entry is compiled (calls and returns
    // stay in compiled code); one that starts with an exit stub is not worth it
    let (r, s) =
        run("local function g() return 1 end local a for i = 1, 6 do a = g() end return a");
    assert_eq!(r, "1");
    assert_eq!(s.compiled, 1, "{s:?}");
    let (r, s) =
        run("local function g(...) return ... end local a for i = 1, 6 do a = g(1) end return a");
    assert_eq!(r, "1");
    assert!(s.enters <= 1, "{s:?}");
}

/// Review fix: a hot loop (OSR) followed by a native tail call that installs a hook, in a frame entered from C
/// (`pcall`): the return must go back to the C caller, not resume the wrong frame.
#[test]
fn native_tail_call_installing_a_hook_returns_to_c_in_on_mode() {
    let (r, s) = run(
        "local function arm() local s = 0 for i = 1, 1e5 do s = s + i end
           return debug.sethook(function() end, '', 1000) end
         local ok = pcall(arm) debug.sethook() return tostring(ok)",
    );
    assert_eq!(r, "true");
    assert!(s.osr_enters >= 1, "{s:?}");
}

#[test]
fn loop_compiles_after_the_back_edge_threshold() {
    let (r, s) = run("local a = 0 for i = 1, 10 do a = i end return a");
    assert_eq!((r.as_str(), s.compiled), ("10", 0), "{s:?}");
    let (r, s) = run("local a = 0 for i = 1, 5000 do a = i end return a");
    assert_eq!(r, "5000");
    assert_eq!(s.compiled, 1, "{s:?}");
    assert!(s.osr_enters >= 1, "{s:?}");
}

#[test]
fn threshold_parser_and_modes() {
    // (the environment is process-global: only the pure pieces are exercised here)
    let c = Config::for_mode(Mode::Always);
    assert_eq!((c.entry_calls, c.loop_edges), (1, 1));
    let c = Config::for_mode(Mode::On);
    assert_eq!((c.entry_calls, c.loop_edges), (200, 1000));
    assert_eq!(Config::parse(Some("off"), None).mode, Mode::Off);
    assert_eq!(Config::parse(Some("always"), None).mode, Mode::Always);
    assert_eq!(Config::parse(Some("verify"), None).mode, Mode::Verify);
    assert_eq!(Config::parse(None, None).mode, Mode::On);
    let c = Config::parse(Some("on"), Some("7,30"));
    assert_eq!((c.entry_calls, c.loop_edges), (7, 30));
    let c = Config::parse(Some("on"), Some("9"));
    assert_eq!((c.entry_calls, c.loop_edges), (9, 1000));
    let c = Config::parse(Some("on"), Some("x,y"));
    assert_eq!((c.entry_calls, c.loop_edges), (200, 1000));
}

/// Child process: refuses to gain executable memory (`PR_SET_MDWE`), then runs Lua with the JIT on.
#[test]
fn mdwe_child() {
    if std::env::var_os("JIT_MDWE_CHILD").is_none() {
        return;
    }
    // PR_SET_MDWE = 65, PR_MDWE_REFUSE_EXEC_GAIN = 1
    // SAFETY: plain prctl call without pointers.
    let rc = unsafe { libc::prctl(65, 1, 0, 0, 0) };
    if rc != 0 {
        eprintln!("PR_SET_MDWE unsupported here; fallback not exercised");
        return;
    }
    let cfg = Config::for_mode(Mode::Always);
    assert!(rn_jit::register_with_trace_pinned_off(cfg));
    let mut st: LuaState = rn_std::new_state_with_libs().expect("state");
    let base = st.top();
    let src =
        "local s = 0 for i = 1, 1000 do s = s + i end local function f() return s end return f()";
    assert_eq!(st.load_buffer(src.as_bytes(), b"=t", None), Status::Ok);
    let func = st.top() - 1;
    assert_eq!(st.pcall(func, LUA_MULTRET, 0), Status::Ok);
    assert_eq!(st.get(func).as_int(), Some(500_500));
    st.set_top(base);
    let s = rn_jit::stats(&mut st);
    assert_eq!((s.compiled, s.enters), (0, 0), "no code may run: {s:?}");
}

#[test]
fn silent_fallback_when_executable_memory_is_refused() {
    let exe = std::env::current_exe().unwrap();
    let out = std::process::Command::new(exe)
        .args(["--exact", "mdwe_child", "--nocapture", "--test-threads=1"])
        .env("JIT_MDWE_CHILD", "1")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "child failed: {}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}
