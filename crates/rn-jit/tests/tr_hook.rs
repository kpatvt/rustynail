//! J5-P1: the recorder hook of rn-core (docs/JIT_TRACE.md 4.1, TR23 items 1-5, experiment hooks for X1).
//!
//! A *test recorder* (`step`/`abort`, registered with `jit_hooks::install_rec`) logs `(proto, pc, depth)` for every
//! instruction it is called for. The logs are compared with the VM-truth oracle (feature `t15_oracle`: a callback before
//! every instruction fetch of both interpreter copies), **including the instructions of inlined callees**, with the
//! callees both interpreted and tier-1 compiled (`jit_check_e` places `TRAP_JIT` on them, which the hooked `'frame`
//! head must not honour while recording, TR23.2).
//!
//! Tests without the oracle (aborts on error / hook / frame, ABA, `trap` clearing at `Leave`) need only feature `trace`.
//! The VM mutant `hooked_head_no_trap` (feature `tr_mutants`) must make the oracle test fail: a child process runs the
//! oracle test with `RUSTYNAIL_JIT_MUT=hooked_head_no_trap` and the parent asserts that it fails.
#![cfg(feature = "trace")]

use rn_core::jit_hooks::{self, Abort, RecCtx, RecStep, RecVTable};
use rn_core::state::{Ci, LuaState, State, StkId, LUA_MULTRET};
use rn_core::status::{LuaResult, Status};
use rn_core::ver::active::OpCode;
use rn_jit::{Config, Mode};
use std::cell::{Cell, RefCell};
use std::sync::{Mutex, MutexGuard, Once};

/// One logged instruction: `(proto address, pc index, absolute frame depth)`.
type Ev = (usize, usize, usize);

#[derive(Clone, Copy)]
struct Policy {
    /// Leave after this many logged instructions.
    limit: usize,
    /// Leave at a `CALL` of a non-Lua-closure callee (the real recorder aborts there, except intrinsics).
    leave_on_native: bool,
}

thread_local! {
    static POLICY: Cell<Policy> = const { Cell::new(Policy { limit: 300, leave_on_native: true }) };
    static LOG: RefCell<Vec<Ev>> = const { RefCell::new(Vec::new()) };
    /// pc of the instruction the recorder asked to leave at.
    static LEFT_AT: Cell<Option<Ev>> = const { Cell::new(None) };
    static ABORTS: RefCell<Vec<Abort>> = const { RefCell::new(Vec::new()) };
    static STASH: Cell<RecCtx> = const { Cell::new(RecCtx { th: 0, root: 0, root_func: 0, root_cl: 0, cur: 0, enter: false, g: 0, token: 0 }) };
    /// The tokens the recorder callbacks received (`step`, `abort`).
    static TOKENS: RefCell<Vec<u64>> = const { RefCell::new(Vec::new()) };
    /// Second state of the two-states tests, and the `RecCtx` seen around its run.
    static OTHER: RefCell<Option<LuaState>> = const { RefCell::new(None) };
    static OTHER_RES: RefCell<String> = const { RefCell::new(String::new()) };
    static CTX_AROUND: RefCell<Vec<RecCtx>> = const { RefCell::new(Vec::new()) };
    /// The oracle log of this thread (`None` = not collecting), and its length when `arm` was called.
    static ORACLE: RefCell<Option<Vec<Ev>>> = const { RefCell::new(None) };
    static ARMED_AT: Cell<usize> = const { Cell::new(0) };
    static TRAPS: RefCell<Vec<Vec<i32>>> = const { RefCell::new(Vec::new()) };
    static ROOTS: RefCell<Vec<usize>> = const { RefCell::new(Vec::new()) };
    /// N2a: re-root the recording (`rec_reroot`) at the first `RETURN` of the root frame instead of leaving; counts the re-roots.
    static REROOT: Cell<Option<usize>> = const { Cell::new(None) };
    static ARM_RESULTS: RefCell<Vec<bool>> = const { RefCell::new(Vec::new()) };
}

/// The tests share the process-global `REC_ARMED` count: run them one at a time, and check on exit that nothing is
/// left armed.
static SERIAL: Mutex<()> = Mutex::new(());

struct Serial(#[allow(dead_code)] MutexGuard<'static, ()>);

fn serial() -> Serial {
    Serial(SERIAL.lock().unwrap_or_else(|e| e.into_inner()))
}

impl Drop for Serial {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            assert!(!jit_hooks::rec_armed(), "a recording is still armed");
        }
    }
}

fn abs_depth(ci: Ci) -> usize {
    let mut n = 0;
    let mut c = ci;
    while let Some(p) = c.previous() {
        n += 1;
        c = p;
    }
    n
}

fn proto_of(st: &mut State, ci: Ci) -> usize {
    let cl = st.ci_lclosure(ci).expect("Lua frame");
    let p = cl.proto();
    &*p as *const _ as usize
}

fn step(st: &mut State, ci: Ci, pc: *const u32, token: u64) -> RecStep {
    TOKENS.with(|t| t.borrow_mut().push(token));
    let p = proto_of(st, ci);
    let cl = st.ci_lclosure(ci).unwrap();
    // SAFETY: `pc` is inside the code array (validated by the VM).
    let ix = unsafe { pc.offset_from(cl.proto().code_ptr()) } as usize;
    let ev = (p, ix, abs_depth(ci));
    let pol = POLICY.get();
    // SAFETY: `pc` is the instruction about to run (inside the code array).
    let ins = rn_core::instruction::Instruction(unsafe { *pc });
    let raw = ins.opcode_raw();
    let mut leave = LOG.with(|l| l.borrow().len()) >= pol.limit;
    if raw == OpCode::Call as u8 && pol.leave_on_native {
        let f = st.get(StkId(ci.r().func.0 + 1 + ins.a() as usize));
        leave |= f.as_lclosure().is_none();
    }
    // a tail call replaces the closure of the frame: at the root the real recorder aborts (TailCall)
    if raw == OpCode::Tailcall as u8 {
        let f = st.get(StkId(ci.r().func.0 + 1 + ins.a() as usize));
        leave |= f.as_lclosure().is_none() || ci.as_ptr() as usize == jit_hooks::rec_ctx().root;
    }
    if [
        OpCode::Return as u8,
        OpCode::Return0 as u8,
        OpCode::Return1 as u8,
    ]
    .contains(&raw)
        && ci.as_ptr() as usize == jit_hooks::rec_ctx().root
    {
        match (REROOT.get(), ci.previous()) {
            (Some(n), Some(prev)) if n == 0 && prev.r().is_lua() => {
                assert!(jit_hooks::rec_reroot(st, prev), "rec_reroot refused");
                REROOT.set(Some(n + 1));
                let c = jit_hooks::rec_ctx();
                assert_eq!(c.root, prev.as_ptr() as usize);
                assert_eq!(c.cur, prev.as_ptr() as usize);
            }
            _ => leave = true,
        }
    }
    if LOG.with(|l| l.borrow().is_empty()) {
        STASH.set(jit_hooks::rec_ctx());
    }
    if leave {
        LEFT_AT.set(Some(ev));
        return RecStep::Leave;
    }
    LOG.with(|l| l.borrow_mut().push(ev));
    RecStep::Keep
}

fn abort(_st: &mut State, why: Abort, token: u64) {
    TOKENS.with(|t| t.borrow_mut().push(token));
    ABORTS.with(|a| a.borrow_mut().push(why));
}

#[cfg_attr(not(feature = "t15_oracle"), allow(dead_code))]
fn oracle_hook(
    frame: usize,
    proto: *const rn_core::func::Proto,
    pc: usize,
    _b: *mut rn_core::value::Value,
    _m: usize,
) {
    ORACLE.with(|o| {
        if let Some(v) = o.borrow_mut().as_mut() {
            // SAFETY: `frame` is the live `CallInfo` of the running frame.
            let ci = unsafe { Ci::from_raw(frame as *mut _) };
            v.push((proto as usize, pc, abs_depth(ci)));
        }
    });
}

// ---- natives for the Lua test programs -----------------------------------------------------------------

/// `arm()`: arm a recording rooted at the calling Lua frame.
fn n_arm(st: &mut State) -> LuaResult<usize> {
    let caller = st.ci().previous().expect("caller");
    ORACLE.with(|o| ARMED_AT.set(o.borrow().as_ref().map_or(0, Vec::len)));
    ROOTS.with(|r| r.borrow_mut().push(caller.as_ptr() as usize));
    assert!(jit_hooks::rec_arm(st, caller).is_some(), "rec_arm refused");
    Ok(0)
}

/// `armerr()`: arm a recording rooted at the calling Lua frame, then raise before the frame is handed to the hooked
/// copy (the error unwinds through the fast copy). Records whether the arming was accepted.
fn n_armerr(st: &mut State) -> LuaResult<usize> {
    let caller = st.ci().previous().expect("caller");
    ROOTS.with(|r| r.borrow_mut().push(caller.as_ptr() as usize));
    let armed = jit_hooks::rec_arm(st, caller).is_some();
    ARM_RESULTS.with(|a| a.borrow_mut().push(armed));
    Err(Status::ErrRun)
}

/// `runb(src)`: run `src` in the other state while this one is recording; the context must survive.
fn n_runb(st: &mut State) -> LuaResult<usize> {
    let src = st.get(StkId(st.ci().r().func.0 + 1));
    let src = String::from_utf8(src.as_string().expect("source").as_bytes().to_vec()).unwrap();
    let before = jit_hooks::rec_ctx();
    let res = OTHER.with(|o| run(o.borrow_mut().as_mut().expect("other state"), &src));
    OTHER_RES.with(|r| *r.borrow_mut() = res);
    CTX_AROUND.with(|c| c.borrow_mut().extend([before, jit_hooks::rec_ctx()]));
    Ok(0)
}

/// `closeb()`: close (drop) the other state while this one is recording.
fn n_closeb(_st: &mut State) -> LuaResult<usize> {
    let before = jit_hooks::rec_ctx();
    drop(OTHER.with(|o| o.borrow_mut().take()));
    CTX_AROUND.with(|c| c.borrow_mut().extend([before, jit_hooks::rec_ctx()]));
    Ok(0)
}

/// `probe()`: log the `trap` of every Lua frame of the chain.
fn n_probe(st: &mut State) -> LuaResult<usize> {
    let mut v = vec![];
    let mut c = st.ci().previous();
    while let Some(ci) = c {
        v.push(ci.r().trap);
        c = ci.previous();
    }
    TRAPS.with(|t| t.borrow_mut().push(v));
    Ok(0)
}

/// `stale()`: restore the stashed (dead) recording context and flag the calling frame, as a stale recorder would.
fn n_stale(st: &mut State) -> LuaResult<usize> {
    let caller = st.ci().previous().expect("caller");
    ROOTS.with(|r| r.borrow_mut().push(caller.as_ptr() as usize));
    jit_hooks::rec_set_ctx(STASH.get());
    caller.w().trap = 1;
    Ok(0)
}

/// `poke()`: call `rec_step` with the native's own frame (not a recorded Lua frame).
fn n_poke(st: &mut State) -> LuaResult<usize> {
    let ci = st.ci();
    let r = jit_hooks::rec_step(st, ci, std::ptr::null());
    assert_eq!(r, RecStep::Leave);
    Ok(0)
}

fn init() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        assert!(rn_jit::register_with_trace_pinned_off(Config::for_mode(
            Mode::On
        )));
        assert!(jit_hooks::install_rec(RecVTable { step, abort }));
        #[cfg(feature = "t15_oracle")]
        rn_core::oracle::set_hook(Some(oracle_hook));
    });
    #[cfg(feature = "tr_mutants")]
    rn_jit::tr::mutants::apply_vm_mutants();
}

fn reset(pol: Policy) {
    POLICY.set(pol);
    REROOT.set(None);
    LOG.with(|l| l.borrow_mut().clear());
    LEFT_AT.set(None);
    ABORTS.with(|a| a.borrow_mut().clear());
    TRAPS.with(|a| a.borrow_mut().clear());
    ROOTS.with(|a| a.borrow_mut().clear());
    ARM_RESULTS.with(|a| a.borrow_mut().clear());
    TOKENS.with(|a| a.borrow_mut().clear());
    CTX_AROUND.with(|a| a.borrow_mut().clear());
    OTHER_RES.with(|a| a.borrow_mut().clear());
    ORACLE.with(|o| *o.borrow_mut() = None);
    jit_hooks::rec_set_ctx(RecCtx::default());
}

fn new_state(mode: Mode) -> LuaState {
    init();
    let mut st = rn_std::new_state_with_libs().expect("state");
    rn_jit::set_mode(&mut st, mode);
    let g = st.globals();
    st.set_funcs(
        g,
        &[
            ("arm", Some(n_arm)),
            ("probe", Some(n_probe)),
            ("stale", Some(n_stale)),
            ("poke", Some(n_poke)),
            ("armerr", Some(n_armerr)),
            ("runb", Some(n_runb)),
            ("closeb", Some(n_closeb)),
        ],
    )
    .unwrap();
    st
}

/// Run `src` as a chunk under `pcall`; returns `"<status>:<result as string>"`.
fn run(st: &mut LuaState, src: &str) -> String {
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

// ---- 1. the instruction oracle -----------------------------------------------------------------------------

/// Programs: `(name, source, calls Lua callbacks through a native)`. `arm()` is called at the point of interest; the
/// functions are tiny so the recorded window crosses many call and return boundaries.
const PRELUDE: &str = "
local function leaf(x) return x + 1 end
local function mid(x) local y = leaf(x) return leaf(y) * 2 end
local function top(n) local s = 0 for i = 1, n do s = s + mid(i) end return s end
local function fib(n) if n < 2 then return n end return fib(n - 1) + fib(n - 2) end
local function tail(x) return leaf(x) end
local function va(...) local a, b = ... return (a or 0) + (b or 0) end
local function multi(x) return x, x + 1, x + 2 end
local acc = 0
local obj = {v = 3}
function obj:get(k) return self.v + k end
-- warm-up: make the callees hot (tier-1 compiled in the `On` runs; the main chunk stays interpreted)
for i = 1, 300 do leaf(i) mid(i) tail(i) fib(2) va(i, 1) multi(i) obj:get(i) end
";

#[cfg_attr(not(feature = "t15_oracle"), allow(dead_code))]
const PROGRAMS: &[(&str, &str, bool)] = &[
    ("calls", "arm() local r = top(6) return r", false),
    ("recursion", "arm() local r = fib(7) return r", false),
    ("tailcall", "arm() local r = tail(5) + tail(6) return r", false),
    ("method", "arm() local r = obj:get(1) + obj:get(2) return r", false),
    ("multret", "arm() local a, b, c = multi(1) local d = multi(5) return a + b + c + d", false),
    ("vararg", "arm() local r = va(1, 2) + va(3) return r", false),
    ("loop_cmp", "arm() local s = 0 for i = 1, 20 do if i % 3 == 0 then s = s + leaf(i) else s = s - 1 end end return s", false),
    ("natives", "arm() local r = math.floor(3.5) + select('#', 1, 2) + top(2) return r", false),
    ("pcall", "arm() local ok, r = pcall(leaf, 4) return r", true),
];

/// Run one program under `mode`; compare the recorder log with the oracle window.
#[cfg_attr(not(feature = "t15_oracle"), allow(dead_code))]
fn oracle_case(name: &str, src: &str, keep_natives: bool, mode: Mode, limit: usize) -> String {
    let mut st = new_state(mode);
    reset(Policy {
        limit,
        leave_on_native: !keep_natives,
    });
    ORACLE.with(|o| *o.borrow_mut() = Some(Vec::new()));
    let full = format!("{PRELUDE}\n{src}");
    let before = rn_jit::stats(&mut st);
    let res = run(&mut st, &full);
    let after = rn_jit::stats(&mut st);
    let log = LOG.with(|l| l.borrow().clone());
    let oracle = ORACLE.with(|o| o.borrow().clone()).unwrap();
    let at = ARMED_AT.get();
    assert!(!log.is_empty(), "{name}: nothing recorded ({res})");
    assert!(
        at + log.len() <= oracle.len(),
        "{name}: oracle window too short: armed at {at}, {} recorded, {} oracle events",
        log.len(),
        oracle.len()
    );
    let window = &oracle[at..at + log.len()];
    if window != &log[..] {
        let k = window
            .iter()
            .zip(&log)
            .position(|(a, b)| a != b)
            .unwrap_or(0);
        panic!(
            "{name} ({mode:?}): recorder differs from the oracle at event {k} of {}: recorder {:?} oracle {:?}",
            log.len(),
            log.get(k),
            window.get(k)
        );
    }
    // the instruction the recorder left at is the next oracle event (the fast copy executes it)
    if let (Some(left), Some(next)) = (LEFT_AT.get(), oracle.get(at + log.len())) {
        assert_eq!(
            left, *next,
            "{name}: the Leave pc is not the next oracle event"
        );
    }
    // inlined callees were recorded: the log reaches deeper frames than the root's
    let root_depth = log[0].2;
    let maxd = log.iter().map(|e| e.2).max().unwrap();
    if limit >= 100_000
        && name != "pcall"
        && name != "loop_cmp"
        && !(name == "natives" && !keep_natives)
    {
        assert!(maxd > root_depth, "{name}: no callee frame in the log");
    }
    if mode == Mode::On {
        assert!(
            after.compiled > before.compiled,
            "{name}: no callee was tier-1 compiled"
        );
    }
    assert!(
        ABORTS.with(|a| a.borrow().is_empty()),
        "{name}: unexpected abort"
    );
    assert_eq!(
        jit_hooks::rec_ctx(),
        RecCtx::default(),
        "{name}: recording still armed"
    );
    res
}

#[cfg_attr(not(feature = "t15_oracle"), allow(dead_code))]
fn all_oracle_cases() {
    for &(name, src, callbacks) in PROGRAMS {
        for mode in [Mode::Off, Mode::On] {
            for limit in [12, 60, 100_000] {
                for keep in [false, true] {
                    if keep && callbacks {
                        continue; // callbacks run nested frames the recorder (like the real one) never sees
                    }
                    let r = oracle_case(name, src, keep, mode, limit);
                    assert!(r.starts_with("Ok:"), "{name}: {r}");
                }
            }
        }
    }
}

#[cfg(feature = "t15_oracle")]
#[test]
fn recorder_log_equals_the_instruction_oracle() {
    let _serial = serial();
    all_oracle_cases();
}

/// The recording does not change results: same value as the plain run of the same chunk.
#[cfg(feature = "t15_oracle")]
#[test]
fn recording_does_not_change_results() {
    let _serial = serial();
    for &(name, src, _) in PROGRAMS {
        let mut st = new_state(Mode::Off);
        reset(Policy {
            limit: 100_000,
            leave_on_native: true,
        });
        let with = run(&mut st, &format!("{PRELUDE}\n{src}"));
        reset(Policy {
            limit: 100_000,
            leave_on_native: true,
        });
        let mut st2 = new_state(Mode::Off);
        let without = run(
            &mut st2,
            &format!("{PRELUDE}\n{}", src.replacen("arm()", "", 1)),
        );
        assert_eq!(with, without, "{name}");
    }
}

/// Mutant `hooked_head_no_trap` (rn-core cfg `jit_trace_mut`, forwarded by `tr_mutants`): without the hooked head's
/// `trap = 1` the inlined callee frames run in the fast copy, the recorder never sees them and the log differs from the
/// oracle. A child process selects the mutant; this test passes iff the oracle test FAILS there.
#[cfg(all(feature = "t15_oracle", feature = "tr_mutants"))]
#[test]
fn mutant_hooked_head_no_trap_is_caught() {
    let _serial = serial();
    if std::env::var("TR_HOOK_CHILD").is_ok() {
        return;
    }
    let exe = std::env::current_exe().unwrap();
    for mutant in ["", "hooked_head_no_trap"] {
        let out = std::process::Command::new(&exe)
            .args([
                "--exact",
                "recorder_log_equals_the_instruction_oracle",
                "--test-threads=1",
            ])
            .env("TR_HOOK_CHILD", "1")
            .env("RUSTYNAIL_JIT_MUT", mutant)
            .stdin(std::process::Stdio::null())
            .output()
            .unwrap();
        if mutant.is_empty() {
            assert!(
                out.status.success(),
                "baseline child failed:\n{}",
                String::from_utf8_lossy(&out.stdout)
            );
        } else {
            assert!(!out.status.success(), "mutant {mutant} was NOT caught");
            let text = format!(
                "{}{}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            );
            assert!(
                [
                    "differs from the oracle",
                    "no callee frame",
                    "is not the next oracle event"
                ]
                .iter()
                .any(|m| text.contains(m)),
                "mutant {mutant} failed, but not by the oracle comparison:\n{text}"
            );
        }
    }
}

// ---- 2. aborts ------------------------------------------------------------------------------------------------

#[test]
fn error_in_recorded_frame_aborts_and_clears_traps() {
    let _serial = serial();
    let mut st = new_state(Mode::Off);
    reset(Policy {
        limit: 100_000,
        leave_on_native: false,
    });
    let r = run(
        &mut st,
        "local function bad(x) local y = x + 1 return y + nil end
         local function f() arm() local r = bad(1) return r end
         local ok, e = pcall(f)
         probe()
         return tostring(ok) .. ':' .. #tostring(e)",
    );
    assert!(r.starts_with("Ok:false:"), "{r}");
    assert_eq!(ABORTS.with(|a| a.borrow().clone()), vec![Abort::Error]);
    assert_eq!(jit_hooks::rec_ctx(), RecCtx::default());
    assert!(
        !LOG.with(|l| l.borrow().is_empty()),
        "the recorder saw the frames before the error"
    );
    // after the unwind every Lua frame has trap 0
    let traps = TRAPS.with(|t| t.borrow().clone());
    assert_eq!(traps.len(), 1);
    assert!(traps[0].iter().all(|&t| t == 0), "stale trap: {traps:?}");
}

#[test]
fn hook_installed_during_recording_aborts() {
    let _serial = serial();
    let mut st = new_state(Mode::Off);
    reset(Policy {
        limit: 100_000,
        leave_on_native: false,
    });
    let r = run(
        &mut st,
        "local n = 0
         local function h() n = n + 1 end
         arm()
         local x = 1 + 2
         debug.sethook(h, '', 1000)
         local y = x * 2
         debug.sethook()
         return y + n",
    );
    assert_eq!(r, "Ok:6", "{r}");
    assert_eq!(ABORTS.with(|a| a.borrow().clone()), vec![Abort::Hook]);
}

#[test]
fn unexpected_frames_abort() {
    let _serial = serial();
    // a native frame (not a recorded Lua frame) and a coroutine (other thread)
    let mut st = new_state(Mode::Off);
    reset(Policy {
        limit: 100_000,
        leave_on_native: false,
    });
    let r = run(&mut st, "arm() poke() return 1");
    assert_eq!(r, "Ok:1");
    assert_eq!(ABORTS.with(|a| a.borrow().clone()), vec![Abort::Frame]);
    reset(Policy {
        limit: 100_000,
        leave_on_native: false,
    });
    let r = run(
        &mut st,
        "arm() local co = coroutine.wrap(function() poke() return 5 end) local r = co() return r",
    );
    assert_eq!(r, "Ok:5");
    assert_eq!(ABORTS.with(|a| a.borrow().clone()), vec![Abort::Frame]);
}

/// TR23.4: `Leave` inside an inlined callee clears `trap` of the callee and of all its callers.
#[test]
fn leave_clears_trap_on_all_recorded_frames() {
    let _serial = serial();
    let mut st = new_state(Mode::Off);
    // leave at the first native call, which is `probe` inside the innermost callee
    reset(Policy {
        limit: 100_000,
        leave_on_native: true,
    });
    let r = run(
        &mut st,
        "local function c3() probe() return 1 end
         local function c2() return c3() + 1 end
         local function c1() return c2() + 1 end
         arm()
         local r = c1()
         probe()
         return r",
    );
    assert_eq!(r, "Ok:3");
    let traps = TRAPS.with(|t| t.borrow().clone());
    assert_eq!(traps.len(), 2, "{traps:?}");
    for t in &traps {
        assert!(
            t.iter().all(|&x| x == 0),
            "trap kept after Leave: {traps:?}"
        );
    }
    assert_eq!(traps[0].len(), 5, "pcall + chunk + c1 + c2 + c3: {traps:?}");

    // control (makes the assertion above non-vacuous): when the recorder keeps recording across `probe`, the same
    // four Lua frames (c3, c2, c1, chunk) DO carry trap = 1 at that point
    reset(Policy {
        limit: 100_000,
        leave_on_native: false,
    });
    let r = run(
        &mut st,
        "local function c3() probe() return 1 end
         local function c2() return c3() + 1 end
         local function c1() return c2() + 1 end
         arm()
         local r = c1()
         return r",
    );
    assert_eq!(r, "Ok:3");
    let traps = TRAPS.with(|t| t.borrow().clone());
    assert_eq!(traps.len(), 1, "{traps:?}");
    assert!(
        traps[0][..4].iter().all(|&x| x == 1),
        "the recorded frames must be trapped while recording: {traps:?}"
    );
}

/// ABA: an error unwinds a recording whose abort is lost (stale context restored), and a new activation reuses the same
/// `CallInfo` node; the lazy validation (TR23.5) must reject the stale recording.
#[test]
fn aba_reused_callinfo_does_not_continue_a_stale_recording() {
    let _serial = serial();
    let mut st = new_state(Mode::Off);
    reset(Policy {
        limit: 100_000,
        leave_on_native: false,
    });
    let r = run(
        &mut st,
        "local function f() arm() local x = 1 return x + nil end
         local function g() stale() local y = 2 y = y + 1 return y end
         local ok1 = pcall(f)
         local ok2, v = pcall(g)
         return tostring(ok1) .. tostring(ok2) .. tostring(v)",
    );
    assert_eq!(r, "Ok:falsetrue3", "{r}");
    let roots = ROOTS.with(|r| r.borrow().clone());
    assert_eq!(roots.len(), 2);
    assert_eq!(
        roots[0], roots[1],
        "the CallInfo node was not reused (the test does not exercise ABA)"
    );
    // Error from the choke point, then Frame from the validation of the stale context
    assert_eq!(
        ABORTS.with(|a| a.borrow().clone()),
        vec![Abort::Error, Abort::Frame]
    );
    let log = LOG.with(|l| l.borrow().clone());
    // the stale recording logged nothing for g
    let g_events = log.iter().filter(|e| e.0 != log[0].0).count();
    assert_eq!(
        g_events, 0,
        "the stale recording continued in the new activation"
    );
    assert_eq!(jit_hooks::rec_ctx(), RecCtx::default());
}

/// Without a registered... the guard that the hook is invisible when nothing is armed: a plain run with the vtable
/// installed behaves as without it.
#[test]
fn nothing_armed_is_invisible() {
    let _serial = serial();
    let mut st = new_state(Mode::Off);
    reset(Policy {
        limit: 100_000,
        leave_on_native: true,
    });
    let r = run(&mut st, &format!("{PRELUDE}\nreturn top(5)"));
    assert!(r.starts_with("Ok:"), "{r}");
    assert!(LOG.with(|l| l.borrow().is_empty()));
    assert!(ABORTS.with(|a| a.borrow().is_empty()));
}

/// F2: an error before the first hooked step (raised by the arming native itself, unwinding through the fast copy)
/// must not leave a context with a dead root: the next activation of the same closure from the same caller slot reuses
/// the `CallInfo` node and would pass the lazy validation. The fast branch of `run()` aborts (TR23.5).
#[test]
fn error_before_the_first_hooked_step_in_the_fast_copy() {
    let _serial = serial();
    let mut st = new_state(Mode::Off);
    reset(Policy::default_test());
    let r = run(
        &mut st,
        "local function f() armerr() end
         local n = 0
         for i = 1, 2 do if not pcall(f) then n = n + 1 end end
         return n",
    );
    assert_eq!(r, "Ok:2", "{r}");
    let roots = ROOTS.with(|r| r.borrow().clone());
    assert_eq!(roots.len(), 2);
    assert_eq!(roots[0], roots[1], "the CallInfo node was not reused");
    // the stale recording was rejected by the abort, not by luck: both armings were accepted, both ended with Error
    assert_eq!(ARM_RESULTS.with(|a| a.borrow().clone()), vec![true, true]);
    assert_eq!(
        ABORTS.with(|a| a.borrow().clone()),
        vec![Abort::Error, Abort::Error]
    );
    assert!(
        LOG.with(|l| l.borrow().is_empty()),
        "a stale recording stepped"
    );
    assert_eq!(jit_hooks::rec_ctx(), RecCtx::default());
}

const DEEP: &str =
    "local function r(n) if n == 0 then return 0 end return 1 + r(n - 1) end return r(150)";

/// F1: frames of another state on the same OS thread enter the hooked copy (stack growth traps every frame while a
/// recording is armed anywhere): the recording of this state is not touched and its recorder is not called for them.
#[test]
fn foreign_state_frames_do_not_disturb_a_recording() {
    let _serial = serial();
    let mut st = new_state(Mode::Off);
    reset(Policy::default_test());
    OTHER.with(|o| *o.borrow_mut() = Some(new_state(Mode::Off)));
    let src = format!("arm() runb([[{DEEP}]]) local x = 1 + 2 return x");
    let r = run(&mut st, &src);
    assert_eq!(r, "Ok:3", "{r}");
    assert_eq!(OTHER_RES.with(|r| r.borrow().clone()), "Ok:150");
    let around = CTX_AROUND.with(|c| c.borrow().clone());
    assert_eq!(around.len(), 2);
    assert_ne!(around[0].th, 0);
    assert_eq!(around[0], around[1], "the foreign run changed the context");
    assert!(
        ABORTS.with(|a| a.borrow().is_empty()),
        "foreign frames aborted the recording"
    );
    // every callback received this recording's token
    let toks = TOKENS.with(|t| t.borrow().clone());
    assert!(!toks.is_empty());
    assert!(toks.iter().all(|&t| t == around[0].token), "{toks:?}");
    OTHER.with(|o| *o.borrow_mut() = None);
}

/// F1: closing a foreign state while this one records neither clears the context nor aborts.
#[test]
fn closing_a_foreign_state_keeps_the_recording() {
    let _serial = serial();
    let mut st = new_state(Mode::Off);
    reset(Policy::default_test());
    OTHER.with(|o| *o.borrow_mut() = Some(new_state(Mode::Off)));
    let r = run(&mut st, "arm() closeb() local x = 1 + 2 return x");
    assert_eq!(r, "Ok:3", "{r}");
    let around = CTX_AROUND.with(|c| c.borrow().clone());
    assert_eq!(around.len(), 2);
    assert_ne!(around[0].th, 0);
    assert_eq!(around[0], around[1]);
    assert!(ABORTS.with(|a| a.borrow().is_empty()));
}

fn synthetic_ctx(st: &mut State, token: u64) -> RecCtx {
    RecCtx {
        th: jit_hooks::thread_ptr(st) as usize,
        root: 8,
        root_func: 1,
        root_cl: 8,
        cur: 8,
        enter: false,
        g: jit_hooks::rec_owner_id(st),
        token,
    }
}

/// F1: closing the owning state delivers `Abort::Close` (with the token) and clears the context.
#[test]
fn closing_the_owner_aborts_with_close() {
    let _serial = serial();
    let mut st = new_state(Mode::Off);
    reset(Policy::default_test());
    let ctx = synthetic_ctx(&mut st, 4242);
    jit_hooks::rec_set_ctx(ctx);
    assert!(jit_hooks::rec_armed());
    drop(st);
    assert_eq!(ABORTS.with(|a| a.borrow().clone()), vec![Abort::Close]);
    assert_eq!(TOKENS.with(|t| t.borrow().clone()), vec![4242]);
    assert_eq!(jit_hooks::rec_ctx(), RecCtx::default());
}

impl Policy {
    fn default_test() -> Policy {
        Policy {
            limit: 100_000,
            leave_on_native: false,
        }
    }
}

struct SendState(#[allow(dead_code)] LuaState);
// SAFETY: test only; the state is created on one thread and dropped on another, never used concurrently.
unsafe impl Send for SendState {}

/// F1: a state armed on T1 and closed on T2 leaves T1's context stale (documented, 3.6): it must not reach a later
/// state on T1 (`rec_step` hands foreign frames back without calling the recorder) and it is released with the thread
/// (`REC_ARMED` back to 0).
#[test]
fn state_armed_on_one_thread_closed_on_another() {
    let _serial = serial();
    reset(Policy::default_test());
    let (to_main, from_t1) = std::sync::mpsc::channel::<SendState>();
    let (go_on, wait_t1) = std::sync::mpsc::channel::<()>();
    let (done, finished) = std::sync::mpsc::channel::<(bool, bool, String)>();
    let t1 = std::thread::spawn(move || {
        reset(Policy::default_test());
        let mut a = new_state(Mode::Off);
        let ctx = synthetic_ctx(&mut a, 99);
        jit_hooks::rec_set_ctx(ctx);
        to_main.send(SendState(a)).unwrap();
        wait_t1.recv().unwrap();
        // a state created later on this thread, while `a` is still alive: the stale context must not reach it
        let mut c = new_state(Mode::Off);
        let res = run(&mut c, DEEP);
        let untouched =
            LOG.with(|l| l.borrow().is_empty()) && ABORTS.with(|a| a.borrow().is_empty());
        let stale_kept = jit_hooks::rec_ctx().token == 99;
        done.send((untouched, stale_kept, res)).unwrap();
        wait_t1.recv().unwrap();
        // thread exit with the stale context: the thread-local's Drop gives back the count
    });
    let a = from_t1.recv().unwrap();
    assert!(jit_hooks::rec_armed());
    go_on.send(()).unwrap();
    let (untouched, stale_kept, res) = finished.recv().unwrap();
    drop(a); // closed on another OS thread than the one that armed it
    assert!(
        untouched,
        "the recorder was called for a state that does not own the context"
    );
    assert!(stale_kept);
    assert_eq!(res, "Ok:150");
    assert!(
        jit_hooks::rec_armed(),
        "the stale context of T1 is still counted"
    );
    go_on.send(()).unwrap();
    t1.join().unwrap();
    assert!(
        !jit_hooks::rec_armed(),
        "thread exit must release the count"
    );
}

/// A natural coroutine switch: the recording is armed in the main thread; the coroutine's frames (another `Thread` of
/// the same state) enter the hooked copy through the stack-growth trap and abort the recording with `Frame`. (The real
/// recorder never gets there: it aborts at the native `CALL` of the coroutine function, which is what keeps coroutine
/// switches out of recorded code.)
#[test]
fn coroutine_frames_abort_a_recording() {
    let _serial = serial();
    let mut st = new_state(Mode::Off);
    reset(Policy::default_test());
    let r = run(
        &mut st,
        &format!("arm() local co = coroutine.wrap(function() {DEEP} end) local r = co() return r"),
    );
    assert_eq!(r, "Ok:150", "{r}");
    assert_eq!(ABORTS.with(|a| a.borrow().clone()), vec![Abort::Frame]);
    assert_eq!(jit_hooks::rec_ctx(), RecCtx::default());
}

/// F4: a hook installed inside the recorded frame aborts the recording, and the frame stays in the hooked copy (no
/// bounce through the fast copy's `'frame` head, which would run `trace_call` again and fire a late call hook): the
/// hook events seen are the same with and without the armed recording.
#[test]
fn hook_installed_during_recording_fires_the_same_events() {
    let _serial = serial();
    let prog = |arm: &str| {
        format!(
            "local n = 0
             local function h(e) n = n + 1 end
             local function f() return 1 end
             {arm}
             debug.sethook(h, 'cr')
             f() f()
             debug.sethook()
             return n"
        )
    };
    let mut st = new_state(Mode::Off);
    reset(Policy::default_test());
    let with = run(&mut st, &prog("arm()"));
    // (`Hook` at the next step of the root, or `Frame` when the hook function's own frame is seen first)
    let aborts = ABORTS.with(|a| a.borrow().clone());
    assert!(aborts.len() == 1 && aborts[0] != Abort::Error, "{aborts:?}");
    reset(Policy::default_test());
    let without = run(&mut st, &prog(""));
    assert!(with.starts_with("Ok:"), "{with}");
    assert_eq!(with, without);
}

/// N2a: `rec_reroot` at the `RETURN` of the root frame: the recording continues in the Lua caller at the instruction after
/// its `CALL` (the interpreter performs the return in the same hooked run), the validation of the following steps reaches
/// the new root, and `Leave` clears the traps of the frames that remain. Natively called roots (`pcall`) are refused by the
/// test recorder before (a C frame is no Lua frame).
#[test]
fn n2a_reroot_continues_the_recording_in_the_caller() {
    let _serial = serial();
    let mut st = new_state(Mode::Off);
    reset(Policy {
        limit: 100_000,
        leave_on_native: true,
    });
    REROOT.set(Some(0));
    let r = run(
        &mut st,
        "local function f(x) arm() return x + 1 end
         local function g() local a = f(1) local b = a * 3 return b + 10 end
         local r = g()
         return r",
    );
    assert_eq!(r, "Ok:16");
    assert_eq!(REROOT.get(), Some(1), "the RETURN of the root was met once");
    let log = LOG.with(|l| l.borrow().clone());
    assert!(
        ABORTS.with(|a| a.borrow().is_empty()),
        "no abort: {:?}",
        ABORTS.with(|a| a.borrow().clone())
    );
    // f: [ADDK/ADD..., RETURN1] then g continues after its CALL (depth one lower)
    let d0 = log[0].2;
    let depths: Vec<usize> = log.iter().map(|e| e.2).collect();
    assert!(
        depths.iter().any(|&d| d + 1 == d0),
        "the recording continued in the caller: {log:?}"
    );
    let first_caller = log.iter().position(|e| e.2 + 1 == d0).unwrap();
    assert!(
        log[..first_caller]
            .iter()
            .all(|e| e.2 == d0 && e.0 == log[0].0),
        "root part: {log:?}"
    );
    assert!(
        log[first_caller..]
            .iter()
            .all(|e| e.2 + 1 == d0 && e.0 != log[0].0),
        "caller part: {log:?}"
    );
    // g's pc after the CALL
    assert!(log[first_caller].1 >= 1, "{log:?}");
    let traps = TRAPS.with(|t| t.borrow().clone());
    assert!(
        traps.iter().all(|t| t.iter().all(|&x| x == 0)),
        "trap kept after Leave: {traps:?}"
    );
    // nothing is left armed (checked by `Serial`)
}
