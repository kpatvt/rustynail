//! J3c hardening: debug interface, stack reallocation / shrink, C-stack limits and coroutine stress with
//! compiled frames live. Every program (`tests/j3c/*.lua`, prefixed by `pre.lua`) runs in a fresh state with the
//! JIT off and in `always` mode (also with small count budgets that force count exits at block heads); the
//! outputs must be identical. Run the binary also with `RUSTYNAIL_GC_STRESS=1|2`, `RUSTYNAIL_JIT_STRESS=stackmove`
//! and (5.4) under `RUSTFLAGS='--cfg rustynail_lua54'`.
#![cfg(feature = "jit")]

use rn_core::state::{LuaState, LUA_MULTRET};
use rn_core::status::Status;
use rn_jit::{Config, Mode, Stats};
use std::sync::Once;

const PRE: &str = include_str!("j3c/pre.lua");

fn stackmove() -> bool {
    std::env::var("RUSTYNAIL_JIT_STRESS").is_ok_and(|v| v == "stackmove")
}

fn init() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        assert!(rn_jit::register_with_trace_pinned_off(Config::for_mode(
            Mode::Always
        )));
        if stackmove() {
            rn_core::jit_hooks::set_stress_stackmove(true);
        }
    });
}

/// Replaces `0x<hex>` addresses.
fn norm(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = String::new();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'0' && i + 2 < b.len() && b[i + 1] == b'x' && b[i + 2].is_ascii_hexdigit() {
            let mut j = i + 2;
            while j < b.len() && b[j].is_ascii_hexdigit() {
                j += 1;
            }
            out.push_str("0xADDR");
            i = j;
        } else {
            out.push(b[i] as char);
            i += 1;
        }
    }
    out
}

fn full(body: &str) -> String {
    // deep recursion is quadratic under stackmove (every helper moves the whole stack)
    // (a full collection at every safepoint is quadratic in the stack depth as well)
    let gc1 = std::env::var("RUSTYNAIL_GC_STRESS").is_ok_and(|v| v == "1");
    let (depth, ovf) = if stackmove() || gc1 {
        (400, false)
    } else {
        (6000, true)
    };
    let pre = PRE
        .replace("DEPTH_VALUE", &depth.to_string())
        .replace("OVF_VALUE", &ovf.to_string())
        .replace("SMALL_VALUE", &(stackmove() || gc1).to_string());
    format!("{pre}{body}")
}

fn run_with(body: &str, mode: Mode, budget: Option<i32>) -> (String, Stats) {
    init();
    let src = full(body);
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
        match v.as_string() {
            Some(s) => String::from_utf8_lossy(s.as_bytes()).into_owned(),
            None => panic!("snippet must return a string, got {v:?}"),
        }
    } else {
        let msg = v.as_string().map_or_else(
            || format!("{v:?}"),
            |s| String::from_utf8_lossy(s.as_bytes()).into_owned(),
        );
        format!("ERROR {status:?}: {msg}")
    };
    st.set_top(base);
    (norm(&out), rn_jit::stats(&mut st))
}

/// Same output with the JIT off and on, also with count budgets; returns the stats of the plain jit-on run.
fn same(name: &str, body: &str) -> Stats {
    let (off, s_off) = run_with(body, Mode::Off, None);
    assert_eq!(s_off.enters, 0);
    if std::env::var("J3C_DUMP").is_ok_and(|v| name.contains(&v)) {
        eprintln!("=== {name}\n{off}");
    }
    assert!(
        !off.starts_with("ERROR"),
        "{name}: reference run failed:\n{off}"
    );
    let moves = rn_core::jit_hooks::stress_moves();
    let (on, s_on) = run_with(body, Mode::Always, None);
    assert_eq!(off, on, "{name}: jit-on differs from jit-off");
    if stackmove() {
        // (other tests run in parallel: the counter only grows)
        assert!(
            rn_core::jit_hooks::stress_moves() > moves,
            "{name}: stackmove mode did not move the stack"
        );
    }
    for b in [3, 17, 200] {
        let (on, _) = run_with(body, Mode::Always, Some(b));
        assert_eq!(off, on, "{name}: budget {b}: jit-on differs from jit-off");
    }
    assert!(
        s_on.enters > 0 || name.contains("noenter"),
        "{name}: {s_on:?}"
    );
    s_on
}

macro_rules! prog_test {
    ($name:ident, $file:literal) => {
        #[test]
        fn $name() {
            same(stringify!($name), include_str!(concat!("j3c/", $file)));
        }
    };
}

prog_test!(debug_getinfo_on_compiled_frames, "dbg_getinfo.lua");
prog_test!(debug_getlocal_setlocal_on_compiled_frames, "dbg_locals.lua");
prog_test!(
    debug_getlocal_temporaries_on_compiled_frames,
    "dbg_temps.lua"
);
prog_test!(debug_traceback_through_compiled_frames, "dbg_traceback.lua");
prog_test!(debug_hooks_with_compiled_callers, "dbg_hooks.lua");
prog_test!(
    debug_upvalues_of_running_compiled_functions,
    "dbg_upvalues.lua"
);
prog_test!(
    stack_growth_and_shrink_with_live_compiled_frames,
    "stack_move.lua"
);
prog_test!(
    collector_modes_barriers_and_finalizers_with_compiled_loops,
    "gc_modes.lua"
);
prog_test!(c_stack_limits_are_identical, "cstack.lua");
prog_test!(hundreds_of_coroutines_run_compiled_code, "coro_stress.lua");
prog_test!(suspended_coroutines_keep_their_code_alive, "coro_gc.lua");
