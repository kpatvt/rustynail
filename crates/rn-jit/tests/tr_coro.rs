//! BUNDLE-T (`coro_fused`): the fused coroutine resume (rn-core `State::resume_fused`, used by `coroutine.wrap` / `coroutine.resume`).
//! The coroutine programs `tests/vm/co_11 .. co_15` are run in-process (compiler registered, `print` / `io.write` captured, chunk names as when
//! the script is run by path) and their output is compared with the reference interpreter's (checked in as
//! `tests/data/co_NN_*.expected`, produced by `ref/55/build/lua` run in `tests/vm`).
//!
//! Detectors of the mutants `cofuse_room_skip` (co_11: 3000 arguments / 5000 results), `cofuse_hook_ignored` (co_13: hooks in a coroutine),
//! `cofuse_ncalls_skip` (co_14: the C level of the resumer), `cofuse_finish_op_skip` (co_11: `coroutine.yield` used as a metamethod),
//! `cofuse_wanted_nil_skip` (co_11: `local x = yield()`), `cofuse_results_room_skip` (co_11: 5000 yielded values), `cofuse_prev_c_ignored`
//! (co_15: `pcall(coroutine.yield)`).
#![cfg(feature = "trace")]
#![cfg_attr(rustynail_lua54, allow(dead_code))] // every test is Lua 5.5 only: the helpers are unused on 5.4

use rn_core::state::{LuaState, LUA_MULTRET};
use rn_core::status::Status;

#[cfg(not(rustynail_lua54))]
mod progs {
    pub const CO_11: (&str, &str, &str) = (
        "co_11_switch_shapes.lua",
        include_str!("../../../tests/vm/co_11_switch_shapes.lua"),
        include_str!("data/co_11_switch_shapes.expected"),
    );
    pub const CO_12: (&str, &str, &str) = (
        "co_12_errors_status.lua",
        include_str!("../../../tests/vm/co_12_errors_status.lua"),
        include_str!("data/co_12_errors_status.expected"),
    );
    pub const CO_13: (&str, &str, &str) = (
        "co_13_close_tbc_hooks.lua",
        include_str!("../../../tests/vm/co_13_close_tbc_hooks.lua"),
        include_str!("data/co_13_close_tbc_hooks.expected"),
    );
    pub const CO_14: (&str, &str, &str) = (
        "co_14_cstack_overflow.lua",
        include_str!("../../../tests/vm/co_14_cstack_overflow.lua"),
        include_str!("data/co_14_cstack_overflow.expected"),
    );
    pub const CO_15: (&str, &str, &str) = (
        "co_15_patterns.lua",
        include_str!("../../../tests/vm/co_15_patterns.lua"),
        include_str!("data/co_15_patterns.expected"),
    );
}

/// `print` and `io.write` append to a buffer returned by the chunk (the real ones write to the process's stdout).
const PRE: &str = "local __buf = {}\n\
local function print(...) local n = select('#', ...) local t = {} \
for i = 1, n do t[i] = tostring((select(i, ...))) end __buf[#__buf + 1] = table.concat(t, '\\t') .. '\\n' end\n\
io.write = function(...) local n = select('#', ...) for i = 1, n do __buf[#__buf + 1] = tostring((select(i, ...))) end end\n";
const POST: &str = "\nreturn table.concat(__buf)";

fn run(name: &str, src: &str) -> String {
    let mut st: LuaState = rn_std::new_state_with_libs().expect("state");
    rn_jit::register();
    #[cfg(feature = "tr_mutants")]
    rn_jit::tr::mutants::apply_vm_mutants();
    let base = st.top();
    // (the prologue is on the first line of the chunk so that the line numbers of the script shift by a constant: the messages carry
    // them, so the script follows on the same lines by putting the prologue before it without a newline)
    let prologue = PRE.replace('\n', " ");
    let text = format!("{prologue}{src}{POST}");
    let chunk = format!("@{name}");
    assert_eq!(
        st.load_buffer(text.as_bytes(), chunk.as_bytes(), None),
        Status::Ok,
        "load"
    );
    let func = st.top() - 1;
    let status = st.pcall(func, LUA_MULTRET, 0);
    let v = st.get(func);
    assert_eq!(status, Status::Ok, "script failed: {v:?}");
    let s = String::from_utf8_lossy(v.as_string().expect("string result").as_bytes()).into_owned();
    st.set_top(base);
    s
}

fn check(p: (&str, &str, &str)) {
    let (name, src, want) = (p.0.to_string(), p.1.to_string(), p.2.to_string());
    // (nested resumes of the C-stack tests need more than a test thread's default stack)
    let h = std::thread::Builder::new()
        .stack_size(256 << 20)
        .spawn(move || {
            let got = run(&name, &src);
            let (g, w): (Vec<&str>, Vec<&str>) = (got.lines().collect(), want.lines().collect());
            for (i, (a, b)) in g.iter().zip(&w).enumerate() {
                assert_eq!(a, b, "{name}: output line {}", i + 1);
            }
            assert_eq!(g.len(), w.len(), "{name}: number of output lines");
        })
        .unwrap();
    if let Err(e) = h.join() {
        std::panic::resume_unwind(e);
    }
}

#[cfg(not(rustynail_lua54))]
#[test]
fn co_switch_shapes_match_the_reference() {
    check(progs::CO_11);
}

#[cfg(not(rustynail_lua54))]
#[test]
fn co_errors_and_status_match_the_reference() {
    check(progs::CO_12);
}

#[cfg(not(rustynail_lua54))]
#[test]
fn co_hooks_close_and_tracebacks_match_the_reference() {
    check(progs::CO_13);
}

#[cfg(not(rustynail_lua54))]
#[test]
fn co_cstack_gc_and_overflow_match_the_reference() {
    check(progs::CO_14);
}

#[cfg(not(rustynail_lua54))]
#[test]
fn co_patterns_match_the_reference() {
    check(progs::CO_15);
}
