//! J1c: calls and returns between compiled frames, compared against the same snippets with the JIT off.
//!
//! The whole binary runs in `always` mode (every function is compiled on its first call, every loop on its
//! first back-edge tick); the jit-off twin is a state whose mode is overridden with `set_mode`.
#![cfg(feature = "jit")]

mod progs;

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

/// Runs `src` (which returns one string or number) in a fresh state with the given mode.
fn run_with(src: &str, mode: Mode) -> (String, Stats) {
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
    let out = if status == Status::Ok {
        match (v.as_string(), v.as_number()) {
            (Some(s), _) => String::from_utf8_lossy(s.as_bytes()).into_owned(),
            (None, Some(n)) => {
                String::from_utf8_lossy(rn_core::number::tostring(n).as_bytes()).into_owned()
            }
            _ => panic!("snippet must return a string or a number, got {v:?}"),
        }
    } else {
        format!(
            "ERROR {status:?}: {}",
            v.as_string().map_or_else(
                || format!("{v:?}"),
                |s| String::from_utf8_lossy(s.as_bytes()).into_owned()
            )
        )
    };
    st.set_top(base);
    (out, rn_jit::stats(&mut st))
}

fn same(src: &str) -> Stats {
    let (off, s_off) = run_with(src, Mode::Off);
    assert_eq!(
        s_off.enters, 0,
        "jit-off state must not enter compiled code"
    );
    let (on, s_on) = run_with(src, Mode::Always);
    assert_eq!(off, on, "jit-on result differs from jit-off for:\n{src}");
    s_on
}

#[test]
fn corpus_matches_the_interpreter() {
    for (name, src) in progs::PROGS {
        let s = same(src);
        assert!(s.compiled >= 1, "{name}: nothing compiled: {s:?}");
    }
}

#[test]
fn compiled_shapes_match_the_interpreter() {
    for (name, src) in progs::COMPILED_SHAPES {
        let s = same(src);
        assert!(s.enters >= 1, "{name}: no compiled code was entered: {s:?}");
    }
}

/// `coroutine.close` of the running thread from its own `__close` handler resets `thread.ci` behind the
/// handler's (compiled) frame: the exit must continue in the frame compiled code left, not in `thread.ci`.
#[test]
fn closing_a_coroutine_from_its_own_close_handler() {
    same(
        "local function func2close(f) return setmetatable({}, {__close = f}) end
         local co
         co = coroutine.create(function()
           local x <close> = func2close(function() coroutine.close(co) end)
           coroutine.yield(20)
         end)
         local st, msg = coroutine.resume(co)
         local st2, msg2 = coroutine.close(co)
         return tostring(st) .. tostring(msg) .. tostring(st2) .. tostring(msg2)",
    );
}

#[test]
fn deep_recursion_in_compiled_frames() {
    for (name, src) in progs::DEEP {
        let s = same(src);
        assert!(s.enters >= 1, "{name}: {s:?}");
    }
}

/// Calls between compiled frames stay in compiled code: a million-ish calls cost a handful of exits.
#[test]
fn flat_calls_do_not_leave_compiled_code() {
    let s = same(
        "local function id(x) return x end
         local function g(f, x) local y = f(x) return y end
         local function k(g, f, x) local y = g(f, x) return y end
         local function loop(n, k, g, id)
           local s = 0
           for i = 1, n do s = k(g, id, i) end
           return s
         end
         return loop(5000, k, g, id)",
    );
    let exits = s.exit_continue + s.exit_frame_changed + s.exit_returned;
    assert!(exits <= 12, "too many exits for flat calls: {s:?}");
    assert!(s.enters <= 12, "{s:?}");
}

/// A compiled callee returning into an interpreter-run caller re-enters the caller's compiled code.
#[test]
fn return_re_enters_the_compiled_caller() {
    let s = same(
        "local function id(x) return x end
         local t = 0
         for i = 1, 100 do t = t + id(i) end
         return t",
    );
    assert!(s.compiled >= 2, "{s:?}");
}

#[test]
fn count_budget_exits_inside_calls_resume_correctly() {
    init();
    let src = "local function id(x) return x end
               local function g(f, x) local y = f(x) return y end
               local function loop(n, g, id) local s = 0 for i = 1, n do s = g(id, i) end return s end
               return loop(300, g, id)";
    let (off, _) = run_with(src, Mode::Off);
    for budget in [1, 2, 3, 5, 7, 11, 50] {
        let mut st: LuaState = rn_std::new_state_with_libs().expect("state");
        rn_jit::set_mode(&mut st, Mode::Always);
        rn_jit::set_count_budget(&mut st, budget);
        assert_eq!(st.load_buffer(src.as_bytes(), b"=s", None), Status::Ok);
        let func = st.top() - 1;
        assert_eq!(st.pcall(func, LUA_MULTRET, 0), Status::Ok);
        let v = st.get(func);
        let got = rn_core::number::tostring(v.as_number().expect("number"));
        assert_eq!(
            String::from_utf8_lossy(got.as_bytes()),
            off,
            "budget {budget}"
        );
    }
}

#[test]
fn memory_limit_error_in_a_native_called_from_compiled_code() {
    // a native that fails (string.rep too large) inside compiled frames: the status propagates
    same(
        "local function f() return string.rep('x', 1 << 62) end
         local function g() local v = f() return v end
         local ok, e = pcall(g)
         return tostring(ok) .. ':' .. tostring(e)",
    );
}

#[test]
fn resume_points_are_valid_instruction_starts() {
    use rn_core::instruction::Instruction;
    use rn_jit::lower::{calls::valid_reentry, lower, LowerCfg};
    init();
    // compile many shapes through the real compiler and check every recorded resume pc
    let mut st: LuaState = rn_std::new_state_with_libs().expect("state");
    let chunks = [
        "local a, b = 1, 2 local t = {} t.x = a + b if a < b then t[1] = a end for i = 1, 3 do t[i] = i end return t",
        "local function f(...) local x = ... return x + 1, f end local s = 0 for i, v in ipairs({1,2}) do s = s + f(v) end return s",
        "local t = setmetatable({}, {__index = function(t, k) return k end}) local s = 0 while s < 10 do s = s + t[1] end return s",
        "local x = 1 repeat local y = x x = x + 1 until y > 5 return x .. 'a'",
    ];
    for src in chunks {
        assert_eq!(st.load_buffer(src.as_bytes(), b"=c", None), Status::Ok);
        let cl = st.get(st.top() - 1).as_lclosure().expect("closure");
        let mut protos = vec![cl.proto()];
        let mut i = 0;
        while i < protos.len() {
            let p = protos[i];
            protos.extend(p.protos().iter().copied());
            let cfg = LowerCfg {
                code_addr: p.code_ptr() as usize,
                interrupt_addr: 0,
                consts: p.k(),
                t15_warmup: None,
                calls: Some(rn_jit::lower::calls::CallEnv {
                    stack_off: 0,
                    proto_code: 0,
                    proto_k: 0,
                    call_helper: 0,
                    ccall_helper: 0,
                    armm: false,
                    tail_helper: 0,
                    tail_fast: false,
                    tail_mut: 0,
                    idx_name: 0,
                    idx_hash: 0,
                    idx_levels: 0,
                    str_mt: 0,
                    inl: 0,
                    inl_mut: 0,
                    t1q: 0,
                    hints: std::ptr::null(),
                    tset: 0,
                }),
            };
            if let Some(low) = lower(p.code(), cfg) {
                for (pc, _) in &low.resume {
                    let pc = *pc as usize;
                    let after_call = pc > 0
                        && Instruction(p.code()[pc - 1]).opcode()
                            == Some(rn_core::ver::active::OpCode::Call);
                    if after_call {
                        assert!(valid_reentry(p.code(), pc), "pc {pc} of {src}");
                    }
                }
            }
            i += 1;
        }
    }
}
