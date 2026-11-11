//! P-L: the one poll word of a running trace (`rn_core::state::TRACE_POLL`, docs/JIT_TRACE_REF.md "P-L decisions").
//!
//! The trace polls ONE byte per loop iteration. It is raised by `request_interrupt` (the SIGINT path) and by `set_hook`
//! with a nonzero mask, and cleared by the poll exit of the trace, which leaves for the interpreter at the loop head
//! with `ci.trap = 1`. The word is process-global, so these tests run one at a time in a binary of their own.
//! Mutants `poll_skip`, `poll_exit_no_trap`, `poll_exit_flag_kept`.
#![cfg(feature = "trace")]

use rn_core::state::{LuaState, State, LUA_MULTRET, TRACE_POLL};
use rn_core::status::Status;
use rn_jit::rt::set_trace;
use rn_jit::t15::runtime::{T15Config, T15Mode};
use rn_jit::tr::cache::{TrConfig, TrMode};
use rn_jit::tr::exit::set_probe;
use rn_jit::tr::cache::TrId;
use rn_jit::{Config, Mode};
use std::sync::atomic::{AtomicI32, AtomicU32, Ordering};
use std::sync::Mutex;

static SERIAL: Mutex<()> = Mutex::new(());

fn init() {
    use std::sync::Once;
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        assert!(
            rn_jit::register_with_trace_pinned_off(Config::for_mode(Mode::Always)),
            "the compiler must be installed by this test binary"
        );
        rn_jit::tr::install_driver();
        #[cfg(feature = "tr_mutants")]
        rn_jit::tr::mutants::apply_vm_mutants();
    });
}

fn state() -> LuaState {
    init();
    let mut st: LuaState = rn_std::new_state_with_libs().expect("state");
    rn_jit::set_mode(&mut st, Mode::Always);
    rn_jit::set_t15(&mut st, T15Config::for_mode(T15Mode::Off));
    let c = TrConfig::for_mode(TrMode::Always);
    set_trace(&mut st, c);
    rn_jit::tr::attach_driver(&mut st);
    st
}

fn exec(st: &mut LuaState, src: &str) -> (Status, String) {
    let base = st.top();
    assert_eq!(st.load_buffer(src.as_bytes(), b"=snippet", None), Status::Ok, "load");
    let func = st.top() - 1;
    let status = st.pcall(func, LUA_MULTRET, 0);
    let v = st.get(func);
    let out = match (v.as_string(), v.as_number()) {
        (Some(s), _) => String::from_utf8_lossy(s.as_bytes()).into_owned(),
        (None, Some(n)) => String::from_utf8_lossy(rn_core::number::tostring(n).as_bytes()).into_owned(),
        _ => "nil".to_string(),
    };
    st.set_top(base);
    (status, out)
}

static PROBED: AtomicU32 = AtomicU32::new(0);
static PROBED_TRAP: AtomicI32 = AtomicI32::new(-1);

/// Called inside the handler of a poll exit, after `ci.trap` was set and before the poll word is cleared.
fn probe(st: &mut State, _id: TrId) {
    PROBED.fetch_add(1, Ordering::SeqCst);
    PROBED_TRAP.store(st.ci().r().trap, Ordering::SeqCst);
}

/// A raised poll word makes the trace leave at its next loop head: the exit hands over to the interpreter with `trap =
/// 1`, clears the word, and the loop's result is the same.
#[test]
fn raised_poll_word_is_consumed_by_one_poll_exit() {
    let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let mut st = state();
    let src = "local function f(n) local s = 0 for i = 1, n do s = s + i % 7 end return s end return f(200000)";
    let (ok, plain) = exec(&mut st, src);
    assert_eq!(ok, Status::Ok);
    TRACE_POLL.store(1, Ordering::SeqCst);
    PROBED.store(0, Ordering::SeqCst);
    PROBED_TRAP.store(-1, Ordering::SeqCst);
    set_probe(Some(probe));
    // the second run enters the trace of the first (or records a new one): the word is raised before the loop starts
    let (ok2, again) = exec(&mut st, src);
    set_probe(None);
    assert_eq!(ok2, Status::Ok);
    assert_eq!(again, plain, "the result is unchanged");
    assert!(PROBED.load(Ordering::SeqCst) >= 1, "no poll exit happened");
    assert_eq!(PROBED_TRAP.load(Ordering::SeqCst), 1, "the poll exit must leave `ci.trap` set for the interpreter");
    assert_eq!(TRACE_POLL.load(Ordering::SeqCst), 0, "the poll exit clears the word");
}

/// An interrupt that arrives while a trace runs (another OS thread) is noticed within one iteration: the `pcall`
/// returns the "interrupted!" error. (The process-wide flags are cleared first: no other test of this binary raises them.)
#[test]
fn interrupt_reaches_a_running_trace() {
    let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    rn_core::state::INTERRUPT.store(false, Ordering::SeqCst);
    TRACE_POLL.store(0, Ordering::SeqCst);
    let mut st = state();
    let t = std::thread::spawn(|| {
        std::thread::sleep(std::time::Duration::from_millis(400));
        rn_core::state::request_interrupt();
    });
    let (status, msg) = exec(&mut st, "local function f() local x = 0 while true do x = x + 1 end end return f()");
    t.join().unwrap();
    assert_ne!(status, Status::Ok);
    let _ = msg;
    assert!(
        !rn_core::state::INTERRUPT.load(Ordering::SeqCst),
        "the interrupt was consumed by the interpreter"
    );
    TRACE_POLL.store(0, Ordering::SeqCst);
}

/// Installing a hook raises the word (so a trace that is running notices it), requesting an interrupt raises it too.
#[test]
fn hooks_and_interrupts_raise_the_word() {
    let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let mut st = state();
    TRACE_POLL.store(0, Ordering::SeqCst);
    st.set_hook(None, 0, 0);
    assert_eq!(TRACE_POLL.load(Ordering::SeqCst), 0, "removing a hook raises nothing");
    fn h(_: &mut State, _: i32, _: i32) -> rn_core::status::LuaResult<()> {
        Ok(())
    }
    st.set_hook(Some(h), rn_core::state::LUA_MASKCOUNT, 1000);
    assert_eq!(TRACE_POLL.load(Ordering::SeqCst), 1);
    st.set_hook(None, 0, 0);
    TRACE_POLL.store(0, Ordering::SeqCst);
    rn_core::state::INTERRUPT.store(false, Ordering::SeqCst);
    rn_core::state::request_interrupt();
    assert_eq!(TRACE_POLL.load(Ordering::SeqCst), 1);
    rn_core::state::INTERRUPT.store(false, Ordering::SeqCst);
    TRACE_POLL.store(0, Ordering::SeqCst);
}
