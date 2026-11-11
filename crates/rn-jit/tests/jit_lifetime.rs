//! J1c code lifetime: compiled code is freed with its prototype, the memory-limit charge is refunded,
//! compiles are packed into shared pages, and exhausting the code cap does not disable the JIT for good.
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
    assert_eq!(st.load_buffer(src.as_bytes(), b"=life", None), Status::Ok);
    let func = st.top() - 1;
    let s = st.pcall(func, LUA_MULTRET, 0);
    st.set_top(base);
    s
}

#[test]
fn code_is_freed_when_the_prototype_is_collected() {
    init();
    let mut st: LuaState = rn_std::new_state_with_libs().expect("state");
    let src = "for i = 1, 300 do
                 local f = load('local a = ... return function(x) return x + a end')(i)
                 f(1)
               end";
    assert_eq!(run_in(&mut st, src), Status::Ok);
    let mid = rn_jit::stats(&mut st);
    assert!(mid.compiled >= 300, "{mid:?}");
    assert_eq!(
        run_in(&mut st, "collectgarbage() collectgarbage()"),
        Status::Ok
    );
    let end = rn_jit::stats(&mut st);
    assert!(end.freed >= 290, "protos were not freed: {end:?}");
    assert!(end.live < 30, "{end:?}");
}

#[test]
fn compiles_share_pages() {
    init();
    let mut st: LuaState = rn_std::new_state_with_libs().expect("state");
    let src = "local keep = {}
               for i = 1, 200 do
                 keep[i] = load('local a = ... return function(x) return x + a end')(i)
                 keep[i](1)
               end";
    assert_eq!(run_in(&mut st, src), Status::Ok);
    let s = rn_jit::stats(&mut st);
    assert!(s.compiled >= 200, "{s:?}");
    // 200 small functions (plus their loaders) must not take a page each
    assert!(
        s.sealed < s.compiled * 4096 / 2, // (vararg loader chunks compile too since J2b)
        "compiles are not packed: {s:?}"
    );
}

#[test]
fn memory_limit_charge_is_refunded_with_the_code() {
    init();
    let mut st: LuaState = rn_std::new_state_with_libs().expect("state");
    assert_eq!(run_in(&mut st, "collectgarbage()"), Status::Ok);
    let base = st.heap().mem_mut().total_bytes();
    // a tight limit: the transient compiled functions only fit if their charge is given back
    st.heap().mem_mut().set_limit(base + 700_000);
    let src = "for i = 1, 3000 do
                 local f = load('local a = ... return function(x) return x + a end')(i)
                 f(1)
                 if i % 64 == 0 then collectgarbage() end
               end";
    assert_eq!(run_in(&mut st, src), Status::Ok);
    let s = rn_jit::stats(&mut st);
    assert!(
        s.compiled >= 3000,
        "compiles were refused under the limit: {s:?}"
    );
    assert_eq!(s.refused, 0, "{s:?}");
    st.heap().mem_mut().set_limit(usize::MAX);
}

#[test]
fn refused_compiles_are_retried_after_memory_is_freed() {
    init();
    let mut st: LuaState = rn_std::new_state_with_libs().expect("state");
    assert_eq!(run_in(&mut st, "collectgarbage()"), Status::Ok);
    let base = st.heap().mem_mut().total_bytes();
    st.heap().mem_mut().set_limit(base + 60_000);
    // keep many compiled functions alive until the limit refuses compiles (they run in the interpreter)
    let hold = "keep = {} for i = 1, 400 do keep[i] = load('local a = ... return function(x) return x + a end')(i) keep[i](1) end";
    let _ = run_in(&mut st, hold);
    let s1 = rn_jit::stats(&mut st);
    // (B3-1: the string table no longer shrinks right after a cycle, so the loader of the next chunk needs a little room)
    st.heap().mem_mut().set_limit(base + 90_000);
    assert_eq!(
        run_in(&mut st, "keep = nil collectgarbage() collectgarbage()"),
        Status::Ok
    );
    st.heap().mem_mut().set_limit(usize::MAX);
    let before = rn_jit::stats(&mut st);
    assert_eq!(
        run_in(
            &mut st,
            "local function f(x) return x end for i = 1, 100 do f(i) end"
        ),
        Status::Ok
    );
    let after = rn_jit::stats(&mut st);
    assert!(
        after.compiled > before.compiled,
        "the JIT stayed disabled: {s1:?} {before:?} {after:?}"
    );
}
