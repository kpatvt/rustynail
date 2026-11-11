//! J3b: the JIT's Rust-heap structures (descriptor, resume table, monitor, compile buffers) are charged to
//! the state's memory limit next to the code pages, refunded when the prototype is freed, and a limit that is
//! too tight makes the function stay in the interpreter (no crash, no abort, same results).
#![cfg(feature = "jit")]

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
    });
}

fn run_in(st: &mut LuaState, src: &str) -> Status {
    let base = st.top();
    assert_eq!(st.load_buffer(src.as_bytes(), b"=ml", None), Status::Ok);
    let func = st.top() - 1;
    let s = st.pcall(func, LUA_MULTRET, 0);
    st.set_top(base);
    s
}

const MAKE: &str = "local a = ... return function(x) return x + a end";

#[test]
fn descriptor_memory_is_charged_and_refunded() {
    init();
    let mut st: LuaState = rn_std::new_state_with_libs().expect("state");
    // warm-up: the thunk page and the first chunk are charged once and stay
    let warm = format!("local f = load('{MAKE}')(1) f(1) collectgarbage() collectgarbage()");
    assert_eq!(run_in(&mut st, &warm), Status::Ok);
    let base = st.heap().mem_mut().ext_bytes();
    let keep = format!("keep = {{}} for i = 1, 300 do keep[i] = load('{MAKE}')(i) keep[i](1) end");
    assert_eq!(run_in(&mut st, &keep), Status::Ok);
    let s = rn_jit::stats(&mut st);
    assert!(s.live >= 300, "{s:?}");
    let held = st.heap().mem_mut().ext_bytes();
    // every live descriptor costs at least its box and resume table
    assert!(
        held - base >= 300 * (std::mem::size_of::<usize>() * 8 + 16),
        "descriptors are not charged: base {base} held {held} {s:?}"
    );
    assert_eq!(
        run_in(&mut st, "keep = nil collectgarbage() collectgarbage()"),
        Status::Ok
    );
    let end = st.heap().mem_mut().ext_bytes();
    let s = rn_jit::stats(&mut st);
    assert!(s.live < 30, "{s:?}");
    // everything but a few still-live prototypes and the pooled chunk is refunded
    assert!(
        end < held && end.saturating_sub(base) < 64 * 1024,
        "refund missing: base {base} held {held} end {end} {s:?}"
    );
}

#[test]
fn compile_buffers_are_charged_only_while_compiling() {
    init();
    let mut st: LuaState = rn_std::new_state_with_libs().expect("state");
    let src = "local function f(n) local s = 0 for i = 1, n do s = s + i % 7 end return s end
               for i = 1, 5 do f(100) end";
    assert_eq!(run_in(&mut st, src), Status::Ok);
    let s = rn_jit::stats(&mut st);
    assert!(s.compiled >= 1, "{s:?}");
    assert_eq!(
        run_in(&mut st, "collectgarbage() collectgarbage()"),
        Status::Ok
    );
    // a leaked transient charge (256 B per instruction) would show as growth with every compile
    let before = st.heap().mem_mut().ext_bytes();
    for _ in 0..5 {
        assert_eq!(run_in(&mut st, src), Status::Ok);
        assert_eq!(
            run_in(&mut st, "collectgarbage() collectgarbage()"),
            Status::Ok
        );
    }
    let after = st.heap().mem_mut().ext_bytes();
    assert!(after < before + 16 * 1024, "{before} -> {after}");
}

#[test]
fn tight_limits_fall_back_to_the_interpreter_consistently() {
    init();
    let prog = "local t = {}
        for i = 1, 200 do t[i] = load('local a = ... return function(x) return x * a + 1 end')(i) end
        local s = 0
        for r = 1, 5 do for i = 1, #t do s = s + t[i](r) end end
        local function fib(n) if n < 2 then return n end return fib(n-1) + fib(n-2) end
        return s + fib(18)";
    let mut expect = None;
    let mut completed = 0;
    // a sweep of headroom values over the state's base use: from nothing to plenty
    for head in [
        0usize, 500, 2_000, 8_000, 20_000, 60_000, 200_000, 4_000_000,
    ] {
        let mut st: LuaState = rn_std::new_state_with_libs().expect("state");
        assert_eq!(run_in(&mut st, "collectgarbage()"), Status::Ok);
        let base = st.heap().mem_mut().total_bytes();
        st.heap().mem_mut().set_limit(base + head);
        let b = st.top();
        let loaded = st.load_buffer(prog.as_bytes(), b"=ml", None);
        if loaded != Status::Ok {
            assert_eq!(loaded, Status::ErrMem, "head {head}");
            st.set_top(b);
            continue;
        }
        let f = st.top() - 1;
        let status = st.pcall(f, 1, 0);
        if status == Status::Ok {
            let top = st.top() - 1;
            let v = rn_core::state::State::to_integer(&st.get(top)).expect("integer");
            match expect {
                None => expect = Some(v),
                Some(e) => assert_eq!(v, e, "head {head}: different result"),
            }
            completed += 1;
        } else {
            // memory errors (or the script calling the nil that a refused `load` returned) are the only failures
            assert!(
                matches!(status, Status::ErrMem | Status::ErrRun),
                "head {head}: {status:?}"
            );
        }
        st.set_top(b);
        st.heap().mem_mut().set_limit(usize::MAX);
    }
    assert!(completed >= 2, "too few runs completed: {completed}");
}
