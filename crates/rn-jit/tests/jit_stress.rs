//! J1c stress: the `RUSTYNAIL_JIT_STRESS=stackmove` mode. Every helper that can reallocate the stack moves
//! it to a fresh block on return (the old block is poisoned and freed), so a stale frame base in compiled
//! code reads garbage or faults. The shared corpus must still match the interpreter.
#![cfg(feature = "jit")]

mod progs;

use rn_core::state::{LuaState, LUA_MULTRET};
use rn_core::status::Status;
use rn_jit::{Config, Mode};
use std::sync::Once;

fn init() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        assert!(rn_jit::register_with_trace_pinned_off(Config::for_mode(
            Mode::Always
        )));
        rn_core::jit_hooks::set_stress_stackmove(true);
    });
}

fn run(src: &str, mode: Mode) -> String {
    init();
    let mut st: LuaState = rn_std::new_state_with_libs().expect("state");
    rn_jit::set_mode(&mut st, mode);
    assert_eq!(
        st.load_buffer(src.as_bytes(), b"=snippet", None),
        Status::Ok
    );
    let func = st.top() - 1;
    let status = st.pcall(func, LUA_MULTRET, 0);
    let v = st.get(func);
    if status != Status::Ok {
        return format!(
            "ERROR {status:?}: {}",
            v.as_string().map_or_else(
                || format!("{v:?}"),
                |s| String::from_utf8_lossy(s.as_bytes()).into_owned()
            )
        );
    }
    match (v.as_string(), v.as_number()) {
        (Some(s), _) => String::from_utf8_lossy(s.as_bytes()).into_owned(),
        (None, Some(n)) => {
            String::from_utf8_lossy(rn_core::number::tostring(n).as_bytes()).into_owned()
        }
        _ => panic!("snippet must return a string or a number"),
    }
}

#[test]
fn corpus_survives_stack_moves_at_every_helper() {
    init();
    let before = rn_core::jit_hooks::stress_moves();
    for (name, src) in progs::PROGS.iter().chain(progs::COMPILED_SHAPES) {
        // deep recursion: every new CallInfo takes the helper path and moves a huge stack (quadratic)
        if matches!(*name, "recursion_depth_and_stack_growth" | "stack_overflow") {
            continue;
        }
        // the jit-off twin does not call helpers, so it never moves the stack
        let off = run(src, Mode::Off);
        let on = run(src, Mode::Always);
        assert_eq!(off, on, "{name}");
    }
    assert!(
        rn_core::jit_hooks::stress_moves() > before + 100,
        "the stress mode did not move the stack enough: {} -> {}",
        before,
        rn_core::jit_hooks::stress_moves()
    );
}

#[test]
fn natives_returning_into_compiled_frames_reload_the_base() {
    // values written before and read after a native call that moved the stack
    let src = "local function f(a, b, c)
                 local x, y = a * 2, b * 3
                 local s = string.rep('z', 3)
                 local t = tostring(c)
                 return x + y + #s + #t
               end
               local r = 0
               for i = 1, 200 do r = r + f(i, i + 1, i + 2) end
               return r";
    assert_eq!(run(src, Mode::Off), run(src, Mode::Always));
}
