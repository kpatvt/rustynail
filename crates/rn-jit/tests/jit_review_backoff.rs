//! Review fix: the loop entry back-off weighs a refused loop tick as one entry opportunity (it used to weigh
//! `TICK_PERIOD`, so the first block lasted a single tick and the back-off never took hold).
//!
//! Own binary: `on` mode with explicit thresholds (process-wide).
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
            entry_calls: 2,
            loop_edges: 20,
        };
        assert!(rn_jit::register_with_trace_pinned_off(cfg));
    });
}

fn run(src: &str) -> Stats {
    init();
    let mut st: LuaState = rn_std::new_state_with_libs().expect("state");
    let base = st.top();
    assert_eq!(st.load_buffer(src.as_bytes(), b"=t", None), Status::Ok);
    let func = st.top() - 1;
    assert_eq!(st.pcall(func, LUA_MULTRET, 0), Status::Ok);
    st.set_top(base);
    rn_jit::stats(&mut st)
}

/// A function whose loop runs 3 iterations and whose compiled stay ends a few bytecodes later (a to-be-closed
/// variable: the interpreter finishes the return). Every loop tick would enter and leave at once.
#[test]
fn short_loop_stays_back_off_quickly() {
    // the metamethod frame ends in a TAILCALL that the interpreter finishes (`CIST_MMRET`): every compiled
    // stay is the loop (3 iterations) plus a handful of bytecodes
    let s = run("local function id(x) return x end
         local o = setmetatable({}, {__add = function(a, b)
           local s = 0
           for j = 1, 3 do s = s + j end
           return id(s)
         end})
         local n = 0
         for i = 1, 100000 do n = n + (o + i) end
         assert(n == 600000)");
    // (measured: 3408 enters / 1936 OSR with the old tick weight, 2048 / 576 now; the rest are call entries)
    assert!(s.osr_enters < 1000, "loop entries do not back off: {s:?}");
    assert!(s.enters < 2600, "no back-off: {s:?}");
}
