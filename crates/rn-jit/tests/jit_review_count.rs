//! Review fix: nested entries into compiled code (a callback run by `pcall`, a sort comparator, an iterator
//! called by `TFORCALL`) must not reset the counter word the suspended outer activation counts with.
//!
//! `Stats::insns` sums the bytecodes consumed by every entry; with a nested entry that reset the shared
//! counter, the outer entry's share was lost (it read `budget - counter` after the nested stay had rewritten
//! the counter).
#![cfg(feature = "jit")]

use rn_core::state::{LuaState, LUA_MULTRET};
use rn_core::status::Status;
use rn_jit::{Config, Mode, Stats};
use std::sync::Once;

fn init() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        assert!(rn_jit::register_with_trace_pinned_off(Config::for_mode(
            Mode::Always
        )));
    });
}

fn run(src: &str, budget: Option<i32>) -> Stats {
    init();
    let mut st: LuaState = rn_std::new_state_with_libs().expect("state");
    if let Some(b) = budget {
        rn_jit::set_count_budget(&mut st, b);
    }
    let base = st.top();
    assert_eq!(st.load_buffer(src.as_bytes(), b"=t", None), Status::Ok);
    let func = st.top() - 1;
    assert_eq!(st.pcall(func, LUA_MULTRET, 0), Status::Ok);
    st.set_top(base);
    rn_jit::stats(&mut st)
}

/// Bytecodes per iteration of the loop `body` repeated `n` times (difference of two runs, so the fixed
/// prologue/epilogue cancels).
fn per_iteration(body: &str, budget: Option<i32>) -> f64 {
    let prog = |n: u32| {
        format!(
            "local function inner(x) return x + 1 end
             local cmp = function(a, b) return a < b end
             local function iter(s, c) if c < 1 then return c + 1 end end
             local t = 0
             for i = 1, {n} do {body} end"
        )
    };
    let a = run(&prog(100), budget).insns;
    let b = run(&prog(300), budget).insns;
    (b - a) as f64 / 200.0
}

#[test]
fn nested_pcall_entries_keep_the_outer_count() {
    let direct = per_iteration("t = inner(t)", None);
    let nested = per_iteration("local ok, v = pcall(inner, t) t = v", None);
    // pcall passes one more argument register (`MOVE`) and takes two results: a few bytecodes more than the
    // direct call, never fewer. A reset counter loses the outer share and leaves only the callee's.
    assert!(
        nested >= direct && nested <= direct + 6.0,
        "outer count lost under nesting: direct {direct}, nested {nested}"
    );
}

#[test]
fn nested_entries_keep_the_outer_count_with_callbacks() {
    let direct = per_iteration("t = inner(t) t = inner(t)", None);
    let sorted = per_iteration("local x = {3, 1, 2} table.sort(x, cmp) t = inner(t)", None);
    let iterated = per_iteration("for k in iter, 0, -1 do t = inner(t) end", None);
    // a callback-heavy iteration costs more than a plain double call
    assert!(sorted > direct, "sort: {sorted} vs {direct}");
    assert!(iterated > 0.0 && iterated < 40.0, "iterator: {iterated}");
}

#[test]
fn small_budget_with_nesting_matches_the_result() {
    // count exits inside nested stays: results must not change with the budget
    let src = "local function inner(x) return x + 1 end
               local t = 0
               for i = 1, 500 do local ok, v = pcall(inner, t) t = v end
               assert(t == 500)";
    let big = run(src, None);
    let small = run(src, Some(37));
    assert!(small.enters >= big.enters, "{small:?} vs {big:?}");
}
