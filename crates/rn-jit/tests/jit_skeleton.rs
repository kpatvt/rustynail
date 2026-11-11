//! J1a: interpreter <-> compiled code round trips, compared against the same snippets with the JIT off.
//!
//! The whole binary runs in `always` mode (every function is compiled on its first call, every loop on its
//! first back-edge tick); the jit-off twin is a state whose mode is overridden with `set_mode`.
#![cfg(feature = "jit")]

use rn_core::state::{LuaState, LUA_MULTRET};
use rn_core::status::Status;
use rn_jit::{Config, Mode, Stats};
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

/// Runs `src` (which returns one string) in a fresh state with the given mode.
fn run_with(src: &str, mode: Mode, budget: Option<i32>) -> (String, Stats) {
    init();
    let mut st: LuaState = rn_std::new_state_with_libs().expect("state");
    rn_jit::set_mode(&mut st, mode);
    if let Some(b) = budget {
        rn_jit::set_count_budget(&mut st, b);
    }
    let base = st.top();
    assert_eq!(
        st.load_buffer(src.as_bytes(), b"=snippet", None),
        Status::Ok,
        "load"
    );
    let func = st.top() - 1;
    let status = st.pcall(func, LUA_MULTRET, 0);
    let v = st.get(func);
    let out = if status == Status::Ok {
        match (v.as_string(), v.as_number()) {
            (Some(s), _) => String::from_utf8_lossy(s.as_bytes()).into_owned(),
            (None, Some(n)) => {
                String::from_utf8_lossy(rn_core::number::tostring(n).as_bytes()).into_owned()
            }
            _ => panic!("snippet must return a string or a number, got {v:?}"),
        }
    } else {
        format!("ERROR {status:?}: {v:?}")
    };
    st.set_top(base);
    (out, rn_jit::stats(&mut st))
}

/// Same result with the JIT off and on; returns the stats of the jit-on run.
fn same(src: &str) -> Stats {
    let (off, s_off) = run_with(src, Mode::Off, None);
    assert_eq!(
        s_off.enters, 0,
        "jit-off state must not enter compiled code"
    );
    let (on, s_on) = run_with(src, Mode::Always, None);
    assert_eq!(off, on, "jit-on result differs from jit-off for:\n{src}");
    s_on
}

#[test]
fn empty_function_round_trip() {
    let s = same("local function f() end f() f() return 'ok'");
    assert!(s.compiled >= 2, "{s:?}"); // main chunk and f
                                       // (the main chunk starts with VARARGPREP, an exit stub: it has no function entry; `f` is entered)
    assert!(s.enters >= 1, "{s:?}");
    assert!(s.exit_returned >= 1, "{s:?}"); // (J2b: the vararg main chunk is entered at pc 0 and returns itself)
}

#[test]
fn return_values_and_nresults() {
    same(
        "local function z() end local function o() return 1 end local function t() return 1, 2 end
         local a, b, c = z() local d, e, f = o() local g, h, i = t()
         return tostring(a)..tostring(b)..tostring(c)..tostring(d)..tostring(e)..tostring(f)..
                tostring(g)..tostring(h)..tostring(i)..select('#', z())..select('#', o())..select('#', t())",
    );
}

#[test]
fn return_into_caller_and_loop() {
    let s = same(
        "local function f() return 42 end local s = 0 for i = 1, 100 do s = s + f() end return s",
    );
    assert!(s.compiled >= 2, "{s:?}");
}

#[test]
fn osr_into_a_hot_loop() {
    let s = same("local s = 0 for i = 1, 20000 do s = s + i end return s");
    // (J2b: the vararg main chunk has a function entry, so `always` enters it at pc 0; OSR is covered by thresholds.rs)
    assert!(s.osr_enters >= 1 || s.enters >= 1, "{s:?}");
}

#[test]
fn compiled_loop_runs_natively() {
    // MOVE/LOADI/FORLOOP only: after OSR the whole loop runs in machine code
    let src = "local a, b = 0, 0 for i = 1, 200000 do a = 7 b = a end return a .. ',' .. b";
    let s = same(src);
    assert!(s.osr_enters >= 1 || s.enters >= 1, "{s:?}");
    assert!(
        s.exit_continue < 50,
        "the loop must not bounce to the interpreter every iteration: {s:?}"
    );
    // a count budget forces C1 exits at block heads; the program still gives the same answer
    let (off, _) = run_with(src, Mode::Off, None);
    let (on, s2) = run_with(src, Mode::Always, Some(5000));
    assert_eq!(off, on);
    assert!(s2.exit_continue > 40, "budget exits expected: {s2:?}");
}

#[test]
fn loops_with_steps_and_bounds() {
    same(
        "local t = {} local a = 0
         for i = 1, 10, 3 do a = i t[#t+1] = i end
         for i = 10, 1, -3 do a = i t[#t+1] = i end
         for i = 5, 1 do t[#t+1] = 'never' end
         for i = 1, 0.5 do t[#t+1] = 'never' end
         for i = 1.0, 3 do t[#t+1] = i end
         for i = math.maxinteger - 1, math.maxinteger do t[#t+1] = i end
         for i = math.mininteger, math.mininteger + 2, 2 do t[#t+1] = i end
         for i = 3, 1, -1 do a = i end
         return table.concat(t, ',') .. '|' .. a",
    );
}

#[test]
fn for_loop_errors_are_the_interpreters() {
    same("local ok, e = pcall(function() for i = 1, 10, 0 do end end) return e");
    same("local ok, e = pcall(function() for i = 1, 'x' do end end) return e");
}

#[test]
fn error_inside_loop_reports_the_same_line() {
    same(
        "local function f()
           local s = 0
           for i = 1, 10 do s = s + i end
           local t = nil
           return s + t
         end
         local ok, msg = pcall(f)
         local ok2, tb = xpcall(f, debug.traceback)
         return msg .. '\\n' .. tb",
    );
}

#[test]
fn coroutine_yield_across_compiled_function() {
    let s = same(
        "local co = coroutine.wrap(function()
           local s = 0
           for i = 1, 5 do coroutine.yield(i) s = s + i end
           return s
         end)
         local r = {}
         for i = 1, 6 do r[#r + 1] = tostring(co()) end
         return table.concat(r, ',')",
    );
    assert!(s.enters >= 1, "{s:?}");
}

#[test]
fn count_hook_counts_match() {
    same(
        "local n = 0
         debug.sethook(function() n = n + 1 end, '', 100)
         local a = 0
         for i = 1, 10000 do a = i end
         debug.sethook()
         return n",
    );
    same(
        "local function f() local a = 0 for i = 1, 3000 do a = i end return a end
         f()
         local n = 0
         debug.sethook(function() n = n + 1 end, '', 7)
         f() f()
         debug.sethook()
         return n",
    );
}

#[test]
fn line_hook_on_a_compiled_function() {
    same(
        "local function f(n)
           local a = 0
           for i = 1, n do
             a = i
           end
           return a
         end
         f(3)
         local lines = {}
         debug.sethook(function(ev, line) lines[#lines + 1] = line end, 'l')
         f(3)
         debug.sethook()
         return table.concat(lines, ',')",
    );
}

#[test]
fn call_hook_and_return_hook() {
    same(
        "local function f() return 1 end
         f()
         local ev = {}
         debug.sethook(function(e) ev[#ev + 1] = e end, 'cr')
         f()
         debug.sethook()
         return table.concat(ev, ',')",
    );
}

#[test]
fn deep_recursion_overflows_cleanly() {
    same(
        "local function f(n) return 1 + f(n + 1) end
         local ok, e = pcall(f, 1)
         return tostring(ok) .. ':' .. tostring(e):sub(1, 40)",
    );
}

#[test]
fn varargs_and_goto_and_while() {
    same(
        "local function v(...) local a, b = ... return a, b end
         local x, y = v(1, 2, 3)
         local i = 0
         while i < 1000 do i = i + 1 end
         repeat i = i - 1 until i < 500
         do local k = 0 ::top:: k = k + 1 if k < 300 then goto top end i = i + k end
         return x .. y .. i",
    );
}

#[test]
fn gc_and_closures_with_compiled_loops() {
    same(
        "local fs = {}
         for i = 1, 100 do fs[i] = function() return i end end
         collectgarbage() collectgarbage('step', 1)
         local s = 0
         for i = 1, 100 do s = s + fs[i]() end
         return s",
    );
}
