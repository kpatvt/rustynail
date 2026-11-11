//! Review finding 4 (T2b): `debug.upvaluejoin` can make a closure's upvalue alias a slot of a running frame
//! (the open upvalue of another closure created in that frame). A `SETUPVAL` then writes a register of its
//! own frame, behind the back of tier 1's per-block register type tracking (`Cx.ty`). Every program runs in
//! a jit-off twin state and in `always` mode and must produce the identical string.
#![cfg(feature = "jit")]

use rn_core::state::{LuaState, LUA_MULTRET};
use rn_core::status::Status;
use rn_jit::{Config, Mode};
use std::sync::Once;

fn init() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        assert!(
            rn_jit::register_with_trace_pinned_off(Config::for_mode(Mode::Always)),
            "the compiler must be installed by this test binary"
        );
    });
}

fn run_with(src: &str, mode: Mode) -> String {
    init();
    let mut st: LuaState = rn_std::new_state_with_libs().expect("state");
    rn_jit::set_mode(&mut st, mode);
    let base = st.top();
    assert_eq!(
        st.load_buffer(src.as_bytes(), b"=snippet", None),
        Status::Ok,
        "load"
    );
    let func = st.top() - 1;
    let status = st.pcall(func, LUA_MULTRET, 0);
    let v = st.get(func);
    let out = match v.as_string() {
        Some(s) => String::from_utf8_lossy(s.as_bytes()).into_owned(),
        None => format!("{v:?}"),
    };
    st.set_top(base);
    format!("{status:?}:{out}")
}

fn same(src: &str) -> String {
    let off = run_with(src, Mode::Off);
    let on = run_with(src, Mode::Always);
    assert_eq!(off, on, "jit-on result differs from jit-off for:\n{src}");
    off
}

#[test]
fn setupval_through_a_joined_upvalue_changes_a_register_of_the_running_frame() {
    // straight-line version (one basic block): `y = i; z = 2.5; s = s + y` where `z` is joined to `y`'s upvalue
    let r = same(
        r#"
        local z = 0
        local f
        local function joiner(g)
          for k = 1, 10 do
            local nm = debug.getupvalue(f, k)
            if nm == "z" then debug.upvaluejoin(f, k, g, 1) return end
          end
        end
        f = function(n, doit)
          local y = 1
          local g = function() return y end
          local s = 0
          if doit then joiner(g) end
          for i = 1, n do
            y = 7
            z = 2.5
            s = s + y + i
          end
          return s
        end
        return tostring(f(10, false)) .. " " .. tostring(f(10, true)) .. " " .. tostring(f(10, false))
        "#,
    );
    assert!(
        r.starts_with("Ok:") && r.contains("Ok:125 ") || r.contains("125"),
        "{r}"
    );
}

#[test]
fn setupval_joined_in_the_middle_of_a_block() {
    let r = same(
        r#"
        local z = 0
        local f
        local function joiner(g)
          for k = 1, 10 do
            local nm = debug.getupvalue(f, k)
            if nm == "z" then debug.upvaluejoin(f, k, g, 1) return end
          end
        end
        f = function(n)
          local y = 1
          local g = function() return y end
          local acc = 0
          for i = 1, n do
            if i == 3 then joiner(g) end
            y = 7
            local before = y + 1
            z = 0.5
            acc = acc + before + y
          end
          return acc
        end
        return tostring(f(8)) .. " " .. tostring(f(2))
        "#,
    );
    assert!(!r.contains("ERROR"), "{r}");
}
