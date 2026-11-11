//! BUNDLE-P: the default trace policy is deterministic.
//!
//! The hop-rate rule used to compare time-stamp-counter ticks per entry (`hopcyc`), so the trace set of a program near the rule's threshold
//! depended on the speed of the core and the load of the machine (`diff_levenshtein` flat-loop variant: 540 or 1515 Mcyc between runs with the
//! same seed and ASLR off). Since BUNDLE-P the default rule counts (`hoptop`: the share of a root's entries that leave through one pre-roll site);
//! the clock rule was removed in K1 (docs/JIT_PARKED.md).
//!
//! * `default_policy_reads_no_clock_and_repeats`: a trace-heavy program run several times in fresh threads (every thread-local policy table is
//!   fresh) under the process default limits gives identical trace statistics (traces recorded, installed, flushed, entered, exits ...), and the
//!   process-wide counter of policy clock reads (`cache::clock_reads`) does not move; with `warm=1` (the time start gate) it does (the counter is live).
//!   Mutant `hop_top_ignored` (the count rule never fires) makes the trampoline program keep its root (`ineffective` stays 0).
#![cfg(feature = "trace")]

use rn_core::state::{LuaState, LUA_MULTRET};
use rn_core::status::Status;
use rn_jit::rt::set_trace;
use rn_jit::tr::cache::{self as c, TrConfig, TrStats};
use rn_jit::{Config, Mode};
use std::sync::Once;

/// The sampling thresholds are process-wide: `run_rearm` switches them to the production `on` values (a tick per 1000 back-edges; the registration of this
/// binary uses the `always` ones, a tick per back-edge) and every run takes this lock.
static THRESHOLDS: std::sync::Mutex<()> = std::sync::Mutex::new(());

struct Restore(i32, i32);
impl Drop for Restore {
    fn drop(&mut self) {
        rn_core::jit_hooks::set_thresholds(self.0, self.1);
    }
}

fn init() {
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

/// A loop whose branch flips for good after the root was recorded: nearly every entry then leaves through the same pre-roll site into the same
/// side trace (a trampoline root, the case the hop rule exists for: `bench/programs/oop_methods.lua` has it by a root recorded on a rare path).
const PROG: &str = "
local x, y = 0, 0
for i = 1, 400000 do
  if i < 30000 then x = x + i % 7 else y = y + i % 5 end
  x = x + 1
end
return x .. ' ' .. y
";

/// R1: a hot plain loop (traced), then a 200-iteration loop (its head stays warm) whose callee leaves compiled code for an `__index` function every 4th iteration.
const REARM_PROG: &str = "
local calls = 0
local base = setmetatable({}, {__index = function(t, k)
  local n = 0
  for j = 1, 3 do n = n + j; calls = calls + 1 end
  return n + select('#', k, t)
end})
local obj = setmetatable({}, {__index = base})
local plain = {val = 3}
local function get(o, k) return o[k] end
local s = 0
for i = 1, 30000 do
  s = s + get(plain, 'val') + (i & 3)
end
for i = 1, 200 do
  local o = plain
  if i % 4 == 0 then o = obj end
  s = s + get(o, 'val') + (i & 3)
end
return s .. ' ' .. calls
";

/// `REARM_PROG` in a fresh thread under the production `on` thresholds (tick per 1000 back-edges).
fn run_rearm(limits: &'static str) -> (String, TrStats) {
    std::thread::spawn(move || {
        init();
        let _g = THRESHOLDS.lock().unwrap_or_else(|e| e.into_inner());
        let (on, always) = (Config::for_mode(Mode::On), Config::for_mode(Mode::Always));
        rn_core::jit_hooks::set_thresholds(on.entry_calls, on.loop_edges);
        let _restore = Restore(always.entry_calls, always.loop_edges);
        let mut tr = TrConfig::parse(None, None, None, Some(limits), None);
        tr.apply_process_limits(Some(limits));
        let mut st: LuaState = rn_std::new_state_with_libs().expect("state");
        rn_jit::set_mode(&mut st, Mode::On);
        rn_jit::set_t15(&mut st, rn_jit::t15::runtime::T15Config::for_mode(rn_jit::t15::runtime::T15Mode::On));
        set_trace(&mut st, tr);
        let base = st.top();
        assert_eq!(st.load_buffer(REARM_PROG.as_bytes(), b"=prog", None), Status::Ok, "load");
        let func = st.top() - 1;
        assert_eq!(st.pcall(func, LUA_MULTRET, 0), Status::Ok, "run");
        let out = st.get(func).as_string().map(|s| String::from_utf8_lossy(s.as_bytes()).into_owned()).unwrap_or_default();
        st.set_top(base);
        (out, c::tr_stats(&mut st))
    })
    .join()
    .expect("run thread")
}

/// R1 `rearm` (with `rearmside=0`: the C45 young entries are a formation change by design): the rearmed tick does not perturb formation. The program has one traced loop and a warm loop whose exits arm the tick: with `rearm=0` and
/// `rearm=16` the output and every trace statistic except the `rearm_*` counters are identical (recordings, installs, flushes, side traces: the
/// rearmed tick feeds neither the start gate nor the head counters and restores the formation countdown), nothing enters the warm head, and the tick was armed.
/// Mutants `rearm_warm_credit` (warm-up credit by rearmed ticks), `rearm_tick_reset` (the countdown restarts), `rearm_any_head`.
#[test]
fn rearm_leaves_formation_alone() {
    let (o0, s0) = run_rearm("rearm=0,rearmside=0");
    let (o1, s1) = run_rearm("rearm=16,rearmside=0");
    assert_eq!(o0, o1);
    assert!(s0.installed >= 1, "the first loop must be traced: {s0:?}");
    assert_eq!((s0.rearm_arms, s0.rearm_enters, s0.rearm_misses), (0, 0, 0), "{s0:?}");
    eprintln!("rearm=16: armed {} entered {} missed {}", s1.rearm_arms, s1.rearm_enters, s1.rearm_misses);
    assert!(s1.rearm_arms >= 1, "the exits of the second loop arm the tick: {s1:?}");
    assert_eq!(s1.rearm_enters, 0, "a rearmed tick entered a head without a root trace: {s1:?}");
    let mut s1 = s1;
    (s1.rearm_arms, s1.rearm_misses, s1.rearm_restored) = (0, 0, 0);
    assert_eq!(s0, s1, "rearm changed the trace statistics");
}

/// One run in a fresh thread under the process default limits plus `extra` (a limits string): the result of the program and the trace statistics.
fn run(extra: Option<&'static str>) -> (String, TrStats) {
    std::thread::spawn(move || {
        init();
        let _g = THRESHOLDS.lock().unwrap_or_else(|e| e.into_inner());
        let mut tr = TrConfig::parse(None, None, None, extra, None);
        tr.apply_process_limits(extra);
        let mut st: LuaState = rn_std::new_state_with_libs().expect("state");
        rn_jit::set_mode(&mut st, Mode::Always);
        rn_jit::set_t15(&mut st, rn_jit::t15::runtime::T15Config::for_mode(rn_jit::t15::runtime::T15Mode::On));
        set_trace(&mut st, tr);
        let base = st.top();
        assert_eq!(st.load_buffer(PROG.as_bytes(), b"=prog", None), Status::Ok, "load");
        let func = st.top() - 1;
        assert_eq!(st.pcall(func, LUA_MULTRET, 0), Status::Ok, "run");
        let out = st.get(func).as_string().map(|s| String::from_utf8_lossy(s.as_bytes()).into_owned()).unwrap_or_default();
        st.set_top(base);
        (out, c::tr_stats(&mut st))
    })
    .join()
    .expect("run thread")
}

#[test]
fn default_policy_reads_no_clock_and_repeats() {
    let before = c::clock_reads();
    let runs: Vec<(String, TrStats)> = (0..6).map(|_| run(None)).collect();
    assert_eq!(c::clock_reads(), before, "a policy decision read a clock under the default limits");
    let (out0, st0) = &runs[0];
    assert!(out0.split(' ').count() == 2, "result {out0:?}");
    assert!(st0.installed >= 1, "the program must be traced: {st0:?}");
    for (k, (out, st)) in runs.iter().enumerate() {
        assert_eq!(out, out0, "run {k}: result");
        assert_eq!(st, st0, "run {k}: trace statistics differ from run 0");
    }
    // the trampoline root of the program is flushed by the counting rule (and only by it: there is no clock in this configuration)
    assert!(st0.ineffective >= 1, "the hop rule did not fire: {st0:?}");
    // the opt-in time start gate does read it (the counter is live)
    let _ = run(Some("warm=1"));
    assert!(c::clock_reads() > before, "warm=1 must read the time-stamp counter");
}
