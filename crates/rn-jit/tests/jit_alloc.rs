//! J2a end to end: the helper-based templates (NEWTABLE, SETLIST, LEN, CONCAT, CLOSURE, POW and the table slow
//! paths) inside real Lua programs, compared against the jit-off twin (the whole binary runs in `always` mode).
//! GC torture: run this binary with `RUSTYNAIL_GC_STRESS=1` and `=2`; `RUSTYNAIL_JIT_STRESS=stackmove` is
//! `jit_alloc_stress.rs`.
#![cfg(feature = "jit")]

use rn_core::state::{LuaState, LUA_MULTRET};
use rn_core::status::Status;
use rn_jit::{Config, Mode, Stats};
use std::sync::Once;

mod alloc_progs;

fn init() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        assert!(
            rn_jit::register_with_trace_pinned_off(Config::for_mode(Mode::Always)),
            "the compiler must be installed by this test binary"
        );
    });
}

fn run_with(src: &str, mode: Mode, budget: Option<i32>) -> (String, Stats) {
    init();
    let mut st: LuaState = rn_std::new_state_with_libs().expect("state");
    rn_jit::set_mode(&mut st, mode);
    if let Some(b) = budget {
        rn_jit::set_count_budget(&mut st, b);
    }
    let base = st.top();
    assert_eq!(
        st.load_buffer(src.as_bytes(), b"=snippet", None),
        Status::Ok,
        "load"
    );
    let func = st.top() - 1;
    let status = st.pcall(func, LUA_MULTRET, 0);
    let v = st.get(func);
    let out = if status == Status::Ok {
        match (v.as_string(), v.as_number()) {
            (Some(s), _) => String::from_utf8_lossy(s.as_bytes()).into_owned(),
            (None, Some(n)) => {
                String::from_utf8_lossy(rn_core::number::tostring(n).as_bytes()).into_owned()
            }
            _ => panic!("snippet must return a string or a number, got {v:?}"),
        }
    } else {
        format!(
            "ERROR {status:?}: {}",
            v.as_string().map_or_else(
                || format!("{v:?}"),
                |s| String::from_utf8_lossy(s.as_bytes()).into_owned()
            )
        )
    };
    st.set_top(base);
    (out, rn_jit::stats(&mut st))
}

fn same(src: &str) -> Stats {
    let (off, s_off) = run_with(src, Mode::Off, None);
    assert_eq!(s_off.enters, 0);
    assert!(!off.starts_with("ERROR"), "program failed: {off}");
    let (on, s_on) = run_with(src, Mode::Always, None);
    assert_eq!(off, on, "jit-on result differs from jit-off for:\n{src}");
    for b in [3, 17, 200] {
        let (on, _) = run_with(src, Mode::Always, Some(b));
        assert_eq!(off, on, "budget {b}: jit-on result differs for:\n{src}");
    }
    s_on
}

#[test]
fn corpus_matches_the_interpreter() {
    for (name, src) in alloc_progs::PROGS {
        eprintln!("== {name}");
        let s = same(src);
        assert!(s.compiled >= 1 && s.enters >= 1, "{name}: {s:?}");
    }
}

/// The point of J2a: loops that allocate, append, concatenate, build closures and miss in tables stay in
/// compiled code (few exits for thousands of iterations).
#[test]
fn allocating_loops_do_not_exit() {
    let src = r#"
        local acc = 0
        local keep
        local base = 3
        for i = 1, 2000 do
          local t = {}
          t[#t + 1] = i
          t[#t + 1] = i * 2
          t.name = "n"
          t.other = i
          local u = {i, i + 1, x = i}
          local s = "k" .. i
          local c = function() return base + 1 end
          acc = acc + #t + #u + c() + math.floor(i ^ 2) // 7 + (t.missing == nil and 1 or 0)
          keep = {t, u, c}
        end
        return acc + #keep
    "#;
    let s = same(src);
    assert!(s.enters >= 1);
    assert!(
        s.exit_continue < 40,
        "allocating loop still exits per iteration: {s:?}"
    );
    eprintln!("{s:?}");
}
