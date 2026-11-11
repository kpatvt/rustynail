//! Review fixes: hooks installed by natives in tail position / returns / close handlers while the frame that
//! the native returns from was entered from C (`CIST_FRESH`: pcall, resume, sort, gsub, TFORCALL's `call`).
//! Compared against the jit-off twin (and the whole binary runs in `always` mode).
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

fn run_with(src: &str, mode: Mode) -> String {
    init();
    let mut st: LuaState = rn_std::new_state_with_libs().expect("state");
    rn_jit::set_mode(&mut st, mode);
    let base = st.top();
    assert_eq!(
        st.load_buffer(src.as_bytes(), b"=snippet", None),
        Status::Ok
    );
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

fn same(src: &str) {
    let off = run_with(src, Mode::Off);
    let on = run_with(src, Mode::Always);
    assert!(off.starts_with("Ok:"), "reference failed: {off}\n{src}");
    assert_eq!(off, on, "jit differs from the interpreter for:\n{src}");
}

const H: &str = "local h = function() end ";

#[test]
fn native_tail_callee_installs_hook_in_pcall_frame() {
    same(&format!(
        "{H} local ok = pcall(function() return debug.sethook(h, 'l') end)
         debug.sethook() return tostring(ok)"
    ));
    same(&format!(
        "{H} local function arm() local s = 0 for i = 1, 1e5 do s = s + i end
            return debug.sethook(h, '', 1000) end
         local ok = pcall(arm) debug.sethook() return tostring(ok)"
    ));
}

#[test]
fn native_tail_callee_installs_hook_in_c_called_frames() {
    // iterator called by TFORCALL (nested call), sort comparator, gsub callback, coroutine body
    same(&format!(
        "{H} local n = 0
         for k in function() n = n + 1 if n < 3 then return debug.sethook(h, 'l') end end do end
         debug.sethook() return tostring(n)"
    ));
    same(&format!(
        "{H} local t = {{3, 1, 2}}
         table.sort(t, function(a, b) debug.sethook(h, 'l') return a < b end)
         debug.sethook() return table.concat(t, ',')"
    ));
    same(&format!(
        "{H} local r = ('abc'):gsub('.', function(c) return debug.sethook(h, 'l') end)
         debug.sethook() return r"
    ));
    same(&format!(
        "{H} local co = coroutine.wrap(function() return debug.sethook(h, 'l') end)
         local r = select('#', co()) debug.sethook() return tostring(r)"
    ));
}

#[test]
fn finalizer_installing_a_hook_during_native_tail_callee() {
    same(&format!(
        "{H} collectgarbage('stop')
         local function f()
           setmetatable({{}}, {{__gc = function() debug.sethook(h, 'l') end}})
           return collectgarbage('collect')
         end
         local ok = pcall(f)
         debug.sethook() collectgarbage('restart') return tostring(ok)"
    ));
}

#[test]
fn hook_installed_around_returns_and_close_handlers() {
    // a __close handler installs a hook while its frame returns to C (pcall) / to Lua
    same(&format!(
        "{H} local ok = pcall(function()
           local x <close> = setmetatable({{}}, {{__close = function() debug.sethook(h, 'l') end}})
           return 1
         end)
         debug.sethook() return tostring(ok)"
    ));
    same(&format!(
        "{H} local function g()
           local x <close> = setmetatable({{}}, {{__close = function() return debug.sethook(h, 'r') end}})
           local y = 1
           return y + 1
         end
         local a = g() local b = pcall(g) debug.sethook() return a .. tostring(b)"
    ));
    // upvalue-closing return (k bit) with a finalizer-installed hook
    same(&format!(
        "{H} local function g() local a = 1 local function c() return a end
            setmetatable({{}}, {{__gc = function() debug.sethook(h, 'l') end}})
            collectgarbage() return c end
         local ok = pcall(g) debug.sethook() return tostring(ok)"
    ));
}
