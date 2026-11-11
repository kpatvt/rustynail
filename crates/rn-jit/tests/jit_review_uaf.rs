//! Review fix (rt::enter): the prototype whose compiled stay is running can be collected while the stay is
//! still running (it returns flat into a compiled caller, which calls the collector). `enter` must not touch
//! the descriptor or the prototype after the call. Debug builds poison collected descriptors (they are
//! quarantined and `JitProto::mon` asserts), so a stale use fails here without ASan.
//!
//! Own binary: `on` mode with explicit thresholds and the verbose profiler (both process-wide).
#![cfg(feature = "jit")]

use rn_core::state::{LuaState, LUA_MULTRET};
use rn_core::status::Status;
use rn_jit::{Config, Mode};
use std::sync::Once;

fn init() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        // SAFETY: set before any other thread of this test binary reads the environment (every test calls
        // `init` first, `Once` blocks the others until it is done).
        unsafe { std::env::set_var("RUSTYNAIL_JIT_VERBOSE", "1") };
        let cfg = Config {
            mode: Mode::On,
            entry_calls: 2,
            loop_edges: 20,
        };
        assert!(rn_jit::register_with_trace_pinned_off(cfg));
    });
}

fn run(src: &str) -> rn_jit::Stats {
    init();
    let mut st: LuaState = rn_std::new_state_with_libs().expect("state");
    let base = st.top();
    assert_eq!(st.load_buffer(src.as_bytes(), b"=t", None), Status::Ok);
    let func = st.top() - 1;
    assert_eq!(st.pcall(func, LUA_MULTRET, 0), Status::Ok);
    st.set_top(base);
    rn_jit::stats(&mut st)
}

/// `P` (a fresh `load`ed chunk with a hot loop) is entered from the interpreter by OSR, returns flat into the
/// compiled caller `C`, which collects it while the stay that entered `P` is still running.
#[test]
fn proto_collected_while_its_own_stay_is_running() {
    let s = run("local function C(n)
           local t = 0
           for i = 1, n do
             local f = load('local s = 0 for i = 1, 300 do s = s + i end return s')
             t = t + f()
             f = nil
             collectgarbage()
           end
           return t
         end
         for k = 1, 4 do assert(C(60) == 60 * 45150) end");
    assert!(s.osr_enters >= 10, "{s:?}");
    assert!(s.freed >= 10, "{s:?}");
}

/// The same with a deeper nest of collections (`pcall` frames in between).
#[test]
fn proto_collected_through_pcall_and_sort_callbacks() {
    let s = run("local function C(n)
           local t = 0
           for i = 1, n do
             local f = load('local s = 0 for i = 1, 300 do s = s + i end return s')
             local ok, v = pcall(f)
             t = t + v
             f = nil
             table.sort({3, 2, 1}, function(a, b) collectgarbage() return a < b end)
           end
           return t
         end
         for k = 1, 4 do assert(C(40) == 40 * 45150) end");
    assert!(s.freed >= 10, "{s:?}");
}
