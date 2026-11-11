//! J2a stress: the `stackmove` mode (every helper that can reallocate the stack moves it on return) over the
//! J2a corpus: a stale frame base after NEWTABLE/SETLIST/CONCAT/CLOSURE/table-insertion helpers reads garbage.
#![cfg(feature = "jit")]

mod alloc_progs;

use rn_core::state::{LuaState, LUA_MULTRET};
use rn_core::status::Status;
use rn_jit::{Config, Mode};
use std::sync::Once;

fn init() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        assert!(rn_jit::register_with_trace_pinned_off(Config::for_mode(Mode::Always)));
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
fn alloc_corpus_survives_stack_moves_at_every_helper() {
    init();
    let before = rn_core::jit_hooks::stress_moves();
    for (name, src) in alloc_progs::PROGS {
        let off = run(src, Mode::Off);
        let on = run(src, Mode::Always);
        assert_eq!(off, on, "{name}");
    }
    assert!(
        rn_core::jit_hooks::stress_moves() > before + 100,
        "the stress mode did not move the stack enough"
    );
}
