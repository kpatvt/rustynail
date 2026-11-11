//! C-stack (`nCcalls`) accounting of compiled code: the inline `next`/`ipairs` step of `TFORCALL` and the
//! helpers that make nested calls must raise "C stack overflow" at the same depth as the interpreter.
//! Includes the JIT fuzzer repro `tests/jit_fuzz_repros/tfor_cstack_limit.lua`.
#![cfg(feature = "jit")]

use rn_core::state::{LuaState, LUA_MULTRET};
use rn_core::status::Status;
use rn_jit::{Config, Mode};
use std::sync::Once;

fn init() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        assert!(rn_jit::register_with_trace_pinned_off(Config::for_mode(Mode::Always)));
    });
}

fn run_with(src: &str, mode: Mode) -> String {
    init();
    let mut st: LuaState = rn_std::new_state_with_libs().expect("state");
    rn_jit::set_mode(&mut st, mode);
    let base = st.top();
    assert_eq!(st.load_buffer(src.as_bytes(), b"=c", None), Status::Ok);
    let func = st.top() - 1;
    let status = st.pcall(func, LUA_MULTRET, 0);
    let v = st.get(func);
    let s = v.as_string().map_or_else(
        || format!("{v:?}"),
        |s| String::from_utf8_lossy(s.as_bytes()).into_owned(),
    );
    st.set_top(base);
    format!("{status:?}:{s}")
}

fn same(src: &str) -> String {
    let off = run_with(src, Mode::Off);
    let on = run_with(src, Mode::Always);
    assert_eq!(off, on, "jit differs from the interpreter for:\n{src}");
    off
}

fn body(iter: &str, extra: &str) -> String {
    format!(
        "local depth, n = 0, 0
         local t1 = {{1, 2, 3}}
         local function f1(p)
           depth = depth + 1
           pcall(f1)
           {extra}
           for k, e in {iter} do n = n + 1 end
         end
         f1(1)
         return depth .. ':' .. n"
    )
}

#[test]
fn repro_file_matches_expected() {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/jit_fuzz_repros/");
    let src = std::fs::read_to_string(format!("{dir}tfor_cstack_limit.lua")).unwrap();
    // the repro prints; run it as a chunk returning the same pair. (Run from a test harness `pcall` the
    // limit is one level earlier than under rnlua, so the exact `expected` pair is checked by
    // `tests/jit_fuzz_repros` runs; here compiled and interpreted must agree and the limit must be hit.)
    let src = src.replace("print(depth, n)", "return depth .. '\\t' .. n");
    let out = same(&src);
    assert!(out.starts_with("Ok:19"), "{out}");
}

#[test]
fn pairs_ipairs_next_near_the_c_limit() {
    for it in [
        "pairs(t1)",
        "ipairs(t1)",
        "next, t1",
        "ipairs({})",
        "pairs({})",
    ] {
        same(&body(it, ""));
    }
}

#[test]
fn close_handlers_and_metamethods_near_the_c_limit() {
    same(&body(
        "ipairs(t1)",
        "local x <close> = setmetatable({}, {__close = function() n = n + 100 end})",
    ));
    same(
        "local depth = 0
         local mt = {}
         mt.__index = function(t, k) depth = depth + 1 local ok = pcall(function() return setmetatable({}, mt).x end) return depth end
         return tostring(setmetatable({}, mt).x)",
    );
    // native tail callees (pcall in tail position) nest C calls too
    same(
        "local depth = 0
         local function f() depth = depth + 1 return pcall(f) end
         f() return tostring(depth)",
    );
}
