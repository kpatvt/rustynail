//! J5-F5: policy integration of the trace compiler with J4 regions (docs/JIT_TRACE.md 3.2, TR19/TR19b/TR20).
//!
//! * **Stub composition (TR19b(b)).** `D_H` -> cell -> {`C_H`, trace, `W_H`, `pcl[H]`}: the cell of a head follows its
//!   state and the eligibility of its nest (no head `Recording`/`Traced`, at least one `Black`, J4 still owns the decision).
//! * **Version install (TR19b(c)).** A J4 recompile rewrites every non-trace cell of the prototype to the new version's
//!   labels in the step that installs it; `Traced` cells keep their traces; retired blocks referenced by trace landings
//!   survive (`+verify` assertion `cache::verify_landings`).
//! * **Nest rule (TR19b(a)).** The outer head goes `Black` while an inner head is `Traced`: the nest goes to J4.
//! * **Ownership order.** `TRACE_PREF=region` gives J4 the nest first; programs compute what the interpreter computes.
//!
//! Mutants (cargo feature `tr_mutants`): `cell_stale_version`, `w_cell_skip`, `region_wait_stale`, `close_nest_keep_warm`,
//! `nest_rule_skip`, `retired_freed_early`.
#![cfg(feature = "trace")]

use rn_core::state::{LuaState, State, LUA_MULTRET};
use rn_core::status::{LuaResult, Status};
use rn_core::value::Value;
use rn_jit::rt::set_trace;
use rn_jit::t15::runtime::{test_event, test_info, EventKind, T15Config, T15Mode};
use rn_jit::tr::cache::{self as c, HeadInfo, HeadState, ProtoRef, TrConfig, TrMode, TrPref};
use rn_jit::{Config, Mode};
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Once;

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

thread_local! {
    static GRABBED: RefCell<HashMap<i64, ProtoRef>> = RefCell::new(HashMap::new());
}

/// `grab(k)`: remember the compiled prototype of the calling Lua function under key `k`.
fn n_grab(st: &mut State) -> LuaResult<usize> {
    let k = st.arg(0).as_int().unwrap_or(0);
    if let Some(p) = c::caller_proto(st) {
        GRABBED.with(|g| g.borrow_mut().insert(k, p));
    }
    Ok(0)
}

/// `black(i)`: blacklist head `i` (in pc order) of the calling function.
fn n_black(st: &mut State) -> LuaResult<usize> {
    let i = st.arg(0).as_int().unwrap_or(0) as usize;
    if let Some(p) = c::caller_proto(st) {
        c::blacklist_head(st, p, i, false);
    }
    Ok(0)
}

/// `blackn(i)`: blacklist head `i` the way a failing recording does (nest rule included).
fn n_blackn(st: &mut State) -> LuaResult<usize> {
    let i = st.arg(0).as_int().unwrap_or(0) as usize;
    if let Some(p) = c::caller_proto(st) {
        c::blacklist_head(st, p, i, true);
    }
    Ok(0)
}

/// `respec()`: a respecialisation request from the current version (what a region stub's policy decides).
fn n_respec(st: &mut State) -> LuaResult<usize> {
    let cur = test_info(st).map_or(0, |i| u32::from(i.cur));
    let _ = test_event(st, cur, 0, EventKind::Respecialize, 0);
    Ok(0)
}

fn grabbed(k: i64) -> ProtoRef {
    GRABBED
        .with(|g| g.borrow().get(&k).copied())
        .expect("grab(k) was not reached")
}

fn trcfg(hot: i32) -> TrConfig {
    let mut c = TrConfig::for_mode(TrMode::On);
    c.hot_loop = hot;
    c.opts.verify = true;
    // K3a: these tests drive and count the loop heads themselves; the function-entry heads that native recursion adds (the shipped `recnat` /
    // `recany`) are covered by tr_core and tr_asm, so they are switched off here explicitly
    c.limits.recnat = 0;
    c.limits.recany = 0;
    c
}

fn t15cfg(hot: i32) -> T15Config {
    let mut c = T15Config::for_mode(T15Mode::On);
    c.hot = hot;
    c
}

fn new_state(tr: TrConfig, t15: T15Config) -> LuaState {
    init();
    let mut st: LuaState = rn_std::new_state_with_libs().expect("state");
    rn_jit::set_mode(&mut st, Mode::Always);
    rn_jit::set_t15(&mut st, t15);
    set_trace(&mut st, tr);
    let g = st.globals();
    st.raw_set_str(g, b"grab", Value::native(n_grab)).unwrap();
    st.raw_set_str(g, b"black", Value::native(n_black)).unwrap();
    st.raw_set_str(g, b"blackn", Value::native(n_blackn)).unwrap();
    st.raw_set_str(g, b"respec", Value::native(n_respec)).unwrap();
    st
}

fn exec(st: &mut LuaState, src: &str) -> String {
    let base = st.top();
    assert_eq!(st.load_buffer(src.as_bytes(), b"=snippet", None), Status::Ok, "load");
    let func = st.top() - 1;
    let status = st.pcall(func, LUA_MULTRET, 0);
    let v = st.get(func);
    let out = if status == Status::Ok {
        match (v.as_string(), v.as_number()) {
            (Some(s), _) => String::from_utf8_lossy(s.as_bytes()).into_owned(),
            (None, Some(n)) => String::from_utf8_lossy(rn_core::number::tostring(n).as_bytes()).into_owned(),
            _ => "nil".to_string(),
        }
    } else {
        let msg = v.as_string().map_or_else(String::new, |s| String::from_utf8_lossy(s.as_bytes()).into_owned());
        format!("ERROR {status:?} {msg}")
    };
    st.set_top(base);
    out
}

fn heads(st: &mut LuaState, k: i64) -> Vec<HeadInfo> {
    c::head_infos(st, grabbed(k))
}

/// Three sequential loops: L1 gets a trace, L2 is blacklisted by the test (`fl`: J4 region, and a respecialisation at
/// the end), L3 stays cold.
const THREE: &str = "
function f(n, m, fl)
  local a, b, c = 0, 0, 0
  if fl then black(1) end
  for i = 1, n do a = a + i end
  for i = 1, n do b = b + i * 2 end
  for i = 1, m do c = c + i end
  if fl then respec() end
  grab(1)
  return a + b + c
end";

fn expected(n: i64, m: i64) -> i64 {
    n * (n + 1) / 2 + n * (n + 1) + m * (m + 1) / 2
}

#[test]
fn cells_follow_head_state_and_nest_eligibility() {
    // J4 never promotes here (huge threshold): only the cells are observed
    let mut st = new_state(trcfg(1_000_000), t15cfg(1_000_000));
    assert_eq!(exec(&mut st, &format!("{THREE} return f(2, 1)")), "10");
    let h = heads(&mut st, 1);
    assert_eq!(h.len(), 3, "three loop heads: {h:?}");
    for x in &h {
        assert_eq!(x.state, HeadState::Warm);
        assert_ne!(x.c_addr, 0);
        assert_ne!(x.w_addr, 0, "J4's promote stub exists in version 0: {x:?}");
        assert_eq!(x.cell, x.c_addr, "Warm: the counting stub");
        assert!(!x.region_wait);
    }
    // a Black head of an eligible nest counts for J4 (W_H); the other nests are unaffected
    let p = grabbed(1);
    c::with_tr(&mut st, p, |tp| tp.blacklist(1)).unwrap();
    let h = heads(&mut st, 1);
    assert_eq!(h[1].state, HeadState::Black);
    assert!(h[1].region_wait);
    assert_eq!(h[1].cell, h[1].w_addr, "Black + eligible: W_H");
    assert_eq!(h[0].cell, h[0].c_addr);
    assert_eq!(h[2].cell, h[2].c_addr);
}

#[test]
fn a_black_head_whose_nest_is_traced_falls_into_tier_1() {
    // two heads in one nest: the inner one gets a trace; the outer one is blacklisted afterwards
    let src = "
function g(n, k)
  local s = 0
  for i = 1, n do
    for j = 1, k do s = s + j end
  end
  grab(2)
  return s
end";
    let mut st = new_state(trcfg(4), t15cfg(1_000_000));
    assert_eq!(exec(&mut st, &format!("{src} return g(2, 1)")), "2");
    let p = grabbed(2);
    // outer head blacklisted while the inner one is still cold: the nest is eligible (W_H)
    let before = heads(&mut st, 2);
    assert_eq!(before.len(), 2);
    let outer = before.iter().position(|h| h.pc == before.iter().map(|x| x.pc).min().unwrap()).unwrap();
    let inner = 1 - outer;
    c::with_tr(&mut st, p, |tp| tp.blacklist(outer)).unwrap();
    let h = heads(&mut st, 2);
    assert!(h[outer].region_wait && h[outer].cell == h[outer].w_addr, "{h:?}");
    // the inner loop gets hot and is traced: the nest is not eligible any more, the outer cell must fall back
    assert_eq!(exec(&mut st, "return g(50, 20)"), "10500");
    let h = heads(&mut st, 2);
    assert_eq!(h[inner].state, HeadState::Traced, "{h:?}");
    assert_eq!(h[outer].state, HeadState::Black);
    assert!(!h[outer].region_wait, "a Traced sibling revokes the eligibility: {h:?}");
    assert_eq!(h[outer].cell, h[outer].pcl_addr, "{h:?}");
}

#[test]
fn j4_respecialisation_rewrites_every_non_trace_cell() {
    let mut st = new_state(trcfg(8), t15cfg(16));
    assert_eq!(exec(&mut st, &format!("{THREE} return f(2, 1)")), "10");
    let v0 = heads(&mut st, 1);
    // L2 is the tracer's blacklisted head; L1 gets its trace; L3 stays cold
    assert_eq!(exec(&mut st, "return f(2000, 3, true)"), expected(2000, 3).to_string());
    let after = heads(&mut st, 1);
    assert_eq!(after[0].state, HeadState::Traced, "{after:?}");
    assert_eq!(after[1].state, HeadState::Black, "{after:?}");
    assert_eq!(after[2].state, HeadState::Warm, "a cold head stays Warm: {after:?}");
    // the respecialisation installed a new version of the tier-1 code: every label moved ...
    for (a, b) in v0.iter().zip(&after) {
        assert_ne!(a.pcl_addr, b.pcl_addr, "pcl[H] was not rewritten: {a:?} / {b:?}");
    }
    assert_ne!(v0[2].c_addr, after[2].c_addr, "C_H of the cold head was not rewritten");
    // ... and the cells point at the new ones, except the Traced one which keeps its trace
    assert_eq!(after[0].cell, after[0].entry, "Traced: the trace entry");
    assert_ne!(after[0].entry, 0);
    assert_eq!(after[2].cell, after[2].c_addr, "Warm: the new C_H");
    // L2's nest is a region now: no counting stub, the cell enters the region through pcl[H]
    assert_eq!(after[1].cell, after[1].pcl_addr, "Black promoted: pcl[H] (the region entry)");
    assert!(!after[1].region_wait);
    // the traced loop still runs (its exits land in the retired version's code) and the results stay right
    assert_eq!(exec(&mut st, "return f(500, 5, true)"), expected(500, 5).to_string());
    let s = c::tr_stats(&mut st);
    assert!(s.installed >= 1);
}

#[test]
fn promotion_blacks_the_cold_heads_of_the_nest() {
    // outer head blacklisted, inner head cold (its counter is far from firing): J4 promotes through W_H and the
    // inner head becomes Black with the nest (TR19)
    let src = "
function h(n, fl)
  local s = 0
  if fl then black(0) end
  for i = 1, n do
    for j = 1, 1 do s = s + i end
  end
  grab(3)
  return s
end
";
    let mut st = new_state(trcfg(100_000), t15cfg(8));
    assert_eq!(exec(&mut st, &format!("{src} return h(2)")), "3");
    assert_eq!(exec(&mut st, "return h(500, true)"), "125250");
    let h = heads(&mut st, 3);
    assert_eq!(h.len(), 2);
    assert!(h.iter().all(|x| x.state == HeadState::Black), "the nest is promoted as a whole: {h:?}");
    assert!(h.iter().all(|x| !x.region_wait), "{h:?}");
    assert_eq!(exec(&mut st, "return h(500)"), "125250");
}

#[test]
fn the_nest_rule_hands_the_nest_to_j4() {
    // the inner loop has a trace (50 back-edges per entry), the outer head is still cold when it is blacklisted the way
    // a failing recording does it (`blackn`): no outer trace links the inner one, so the nest goes to J4
    let src = "
function k(n, fl)
  local s = 0
  for i = 1, n do
    for j = 1, 50 do s = s + j end
    if fl and i == 20 then blackn(0) end
    s = s + i
  end
  grab(4)
  return s
end";
    let mut st = new_state(trcfg(60), t15cfg(32));
    assert_eq!(exec(&mut st, &format!("{src} return k(2)")), "2553");
    let want = |n: i64| (1..=n).map(|i| 1275 + i).sum::<i64>().to_string();
    assert_eq!(exec(&mut st, "return k(3000, true)"), want(3000));
    let h = heads(&mut st, 4);
    let s = c::tr_stats(&mut st);
    assert_eq!(s.nest_rule, 1, "the nest rule fired once: {s:?}");
    assert_eq!(s.installed, 1, "only the inner trace was ever installed: {s:?}");
    assert!(h.iter().all(|x| x.state == HeadState::Black), "every head of the nest is Black: {h:?}");
    assert!(h.iter().all(|x| x.trace.is_none() && !x.region_wait), "{h:?}");
    assert_eq!(exec(&mut st, "return k(3000)"), want(3000));
}

#[test]
fn the_nest_rule_keeps_inner_traces_with_inlined_calls() {
    let src = "
function k(n, fl)
  local s = 0
  local function add(a) return a + 1 end
  for i = 1, n do
    for j = 1, 50 do s = add(s) end
    if fl and i == 20 then blackn(0) end
  end
  grab(6)
  return s
end";
    let mut st = new_state(trcfg(60), t15cfg(1_000_000));
    assert_eq!(exec(&mut st, &format!("{src} return k(2)")), "100");
    assert_eq!(exec(&mut st, "return k(3000, true)"), "150000");
    let s = c::tr_stats(&mut st);
    assert_eq!(s.nest_rule, 0, "{s:?}");
    assert_eq!(s.nest_rule_kept, 1, "{s:?}");
    let h = heads(&mut st, 6);
    assert!(h.iter().any(|x| x.state == HeadState::Traced), "the inner trace stays: {h:?}");
}

#[test]
fn trace_pref_region_gives_j4_the_nest_first() {
    let mut cfg = trcfg(8);
    cfg.pref = TrPref::Region;
    let mut st = new_state(cfg, t15cfg(8));
    let src = "
function r(n) local s = 0 for i = 1, n do s = s + i end grab(5) return s end";
    assert_eq!(exec(&mut st, &format!("{src} return r(1000)")), "500500");
    let h = heads(&mut st, 5);
    assert_eq!(h[0].state, HeadState::Black, "{h:?}");
    assert_eq!(c::tr_stats(&mut st).installed, 0, "the tracer never recorded: {h:?}");
    assert_eq!(exec(&mut st, "return r(100000)"), "5000050000");
}

#[test]
fn j4_off_means_no_w_stub_and_the_old_behaviour() {
    let mut st = new_state(trcfg(4), T15Config::for_mode(T15Mode::Off));
    assert_eq!(exec(&mut st, &format!("{THREE} return f(2, 1)")), "10");
    let h = heads(&mut st, 1);
    assert!(h.iter().all(|x| x.w_addr == 0), "{h:?}");
    let p = grabbed(1);
    c::with_tr(&mut st, p, |tp| tp.blacklist(1)).unwrap();
    let h = heads(&mut st, 1);
    assert!(!h[1].region_wait);
    assert_eq!(h[1].cell, h[1].pcl_addr);
}

#[test]
fn programs_compute_the_same_under_every_ownership_order() {
    let prog = format!(
        "{THREE}
         local t = {{}}
         for k = 1, 3 do t[#t+1] = f(300 * k, 40, k == 2) end
         return table.concat(t, ',') .. '|' .. f(1000, 7, true)"
    );
    let want = {
        let mut st = new_state(TrConfig::for_mode(TrMode::Off), T15Config::for_mode(T15Mode::Off));
        exec(&mut st, &prog)
    };
    assert!(!want.starts_with("ERROR"), "{want}");
    for pref in [TrPref::Trace, TrPref::Region] {
        for (hot, t15hot) in [(2, 2), (8, 16), (50, 4)] {
            let mut cfg = trcfg(hot);
            cfg.pref = pref;
            eprintln!("config {pref:?} hot {hot}/{t15hot}");
            let mut st = new_state(cfg, t15cfg(t15hot));
            assert_eq!(exec(&mut st, &prog), want, "{pref:?} hot {hot}/{t15hot}");
        }
    }
}

/// P-M4: the process default turns native self-recursion on; the struct default (tests) has it off and an explicit `recnat=` wins.
/// Mutant `recnat_default_ignored`.
#[test]
fn pm4_process_default_recnat() {
    use rn_jit::tr::cache::{TrConfig, TrMode, RECNAT_PROCESS};
    assert_eq!(RECNAT_PROCESS, 1);
    assert_eq!(TrConfig::for_mode(TrMode::On).limits.recnat, RECNAT_PROCESS, "struct default (tests) = shipped");
    for (env, want) in [(None, 1), (Some(""), 1), (Some("nest=2"), 1), (Some("recnat=0"), 0), (Some("maxside=9,recnat=1"), 1)] {
        let mut c = TrConfig::parse(None, None, None, env, None);
        c.apply_process_limits(env);
        assert_eq!(c.limits.recnat, want, "limits env {env:?}");
    }
}

/// T3: the process default of `t3idx` is two inline `__index` levels; the struct default (tests) is 0 and an explicit value wins.
/// Mutant `t3_default_ignored`.
#[test]
fn t3_process_default() {
    use rn_jit::tr::cache::{TrConfig, TrMode, T3IDX_PROCESS};
    assert_eq!(T3IDX_PROCESS, 2);
    assert_eq!(TrConfig::for_mode(TrMode::On).limits.t3idx, T3IDX_PROCESS, "struct default (tests) = shipped");
    for (env, want) in [(None, 2), (Some(""), 2), (Some("nest=2"), 2), (Some("t3idx=0"), 0), (Some("maxside=9,t3idx=1"), 1), (Some("t3idx=3"), 3)] {
        let mut c = TrConfig::parse(None, None, None, env, None);
        c.apply_process_limits(env);
        assert_eq!(c.limits.t3idx, want, "limits env {env:?}");
    }
}

/// BUNDLE-T1I / C36: the process default of `t1inline` is 1023 (all rows; the floor bit cost `heldout/bigint_rsa` 30% until `cispare` repaired its entry failures),
/// the struct default (tests) is off, and an explicit value wins (clamped to the 10 rows).
#[test]
fn t1i_process_default() {
    use rn_jit::tr::cache::{TrConfig, TrMode, T1INLINE_PROCESS};
    assert_eq!(T1INLINE_PROCESS, 1023);
    assert_eq!(TrConfig::for_mode(TrMode::On).limits.t1inline, T1INLINE_PROCESS, "struct default (tests) = shipped");
    for (env, want) in [(None, T1INLINE_PROCESS), (Some(""), T1INLINE_PROCESS), (Some("nest=2"), T1INLINE_PROCESS), (Some("t1inline=0"), 0), (Some("maxside=9,t1inline=65"), 65), (Some("t1inline=1021"), 1021), (Some("t1inline=4096"), 1023)] {
        let mut c = TrConfig::parse(None, None, None, env, None);
        c.apply_process_limits(env);
        assert_eq!(c.limits.t1inline, want, "limits env {env:?}");
    }
}

/// C36: process defaults and pure rules of `cispare`, `domtop` / `domn` and `second` (struct defaults 0, an explicit value wins), the `dom_due` /
/// `second_retry_due` / `ident_keep_due` decisions, and the once-per-head second chance of `TrProto`. Mutants `dom_never`, `dom_repeats`,
/// `second_retry_never`, `second_retry_repeats`, `second_identical_never`, `second_identical_repeats`.
#[test]
fn c36_policy() {
    use rn_jit::tr::cache::{dom_due, ident_keep_due, second_retry_due, Penalty, TrConfig, TrMode, TrProto, CISPARE_PROCESS, DOMN_PROCESS, DOMTOP_PROCESS, MAX_ABORTS, SECOND_PROCESS};
    let (c0, d0, n0, s0) = (CISPARE_PROCESS, DOMTOP_PROCESS, DOMN_PROCESS, SECOND_PROCESS);
    assert_eq!((c0, d0, n0, s0), (1, 82, 65536, 3), "the shipped process defaults");
    let z = TrConfig::for_mode(TrMode::On).limits;
    assert_eq!((z.cispare, z.domtop, z.domn, z.second), (c0, d0, n0, s0), "struct defaults (tests) = shipped");
    for (env, want) in [
        (None, (c0, d0, n0, s0)),
        (Some(""), (c0, d0, n0, s0)),
        (Some("nest=2"), (c0, d0, n0, s0)),
        (Some("cispare=1"), (1, d0, n0, s0)),
        (Some("cispare=0"), (0, d0, n0, s0)),
        (Some("domtop=75"), (c0, 75, n0, s0)),
        (Some("domtop=75,domn=1000"), (c0, 75, 1000, s0)),
        (Some("domtop=200"), (c0, 100, n0, s0)),
        (Some("second=3"), (c0, d0, n0, 3)),
        (Some("second=9"), (c0, d0, n0, 3)),
        (Some("second=0"), (c0, d0, n0, 0)),
    ] {
        let mut c = TrConfig::parse(None, None, None, env, None);
        c.apply_process_limits(env);
        let l = c.limits;
        assert_eq!((l.cispare, l.domtop, l.domn, l.second), want, "limits env {env:?}");
    }
    // dom_due: off at 0; share and entry thresholds; once per head
    assert!(!dom_due(0, 10, 100, 100, false));
    assert!(dom_due(75, 1000, 800, 1000, false));
    assert!(!dom_due(75, 1000, 749, 1000, false), "share below the percent");
    assert!(!dom_due(75, 1000, 800, 999, false), "too few entries");
    assert!(!dom_due(75, 1000, 800, 1000, true), "once per head");
    // second_retry_due: a changeable LeaveFrame abort that blacklisted, bit 0 only
    assert!(second_retry_due(1, true, false, "LeaveFrame"));
    assert!(second_retry_due(3, true, false, "LeaveFrame"));
    assert!(!second_retry_due(0, true, false, "LeaveFrame"));
    assert!(!second_retry_due(2, true, false, "LeaveFrame"), "bit 1 is the identical-compile chance");
    assert!(!second_retry_due(1, false, false, "LeaveFrame"), "not blacklisted");
    assert!(!second_retry_due(1, true, true, "LeaveFrame"), "an unchangeable cause stays blacklisted");
    for r in ["IntMiss", "LeaveLoop", "StartRefused", "InnerLoop", "SideShort"] {
        assert!(!second_retry_due(3, true, false, r), "{r}");
    }
    // ident_keep_due: bit 1, the first time only
    assert!(ident_keep_due(2, false));
    assert!(ident_keep_due(3, false));
    assert!(!ident_keep_due(1, false));
    assert!(!ident_keep_due(0, false));
    assert!(!ident_keep_due(3, true), "once per head");
    // TrProto: MAX_ABORTS changeable aborts blacklist; the second chance waits, allows two more attempts, and exists once
    let tp = TrProto::new(10, &[3, 7], 256, true, 77, 1);
    for _ in 0..MAX_ABORTS - 1 {
        assert!(matches!(tp.on_abort(0, "LeaveFrame", false), Penalty::Wait(_)));
    }
    assert_eq!(tp.on_abort(0, "LeaveFrame", false), Penalty::Black);
    match tp.second_chance(0) {
        Some(Penalty::Wait(w)) => assert!(w >= 64 << MAX_ABORTS, "a long wait: {w}"),
        o => panic!("first second chance: {o:?}"),
    }
    assert!(matches!(tp.on_abort(0, "LeaveFrame", false), Penalty::Wait(_)), "attempt 1 of 2");
    assert_eq!(tp.on_abort(0, "LeaveFrame", false), Penalty::Black, "attempt 2 of 2 blacklists again");
    assert_eq!(tp.second_chance(0), None, "once per head");
    assert!(matches!(tp.second_chance(1), Some(Penalty::Wait(_))), "the other head has its own chance");
}

/// P3: process defaults of `ncallee` / `chainre` / `chainn` (struct defaults 0, an explicit value wins), the pure `ncallee_due` and `chain_due` rules. `chain_due`: the entry
/// threshold, the pass-through percent of the first link, the gain test (a chain in hotness order, and a uniform one, never fire however long: the thrash bound), the per-exit and
/// per-family bounds. Mutants `ncallee_never`, `ncallee_without_entry_guard`, `chain_never`, `chain_unbounded`, `chain_pass_ignored`, `chain_gain_ignored`.
#[test]
fn p3_policy() {
    use rn_jit::tr::cache::{chain_due, ncallee_due, TrConfig, TrMode, CHAINN_PROCESS, CHAINRE_PROCESS, CHAIN_MAX_EXIT, CHAIN_MAX_FAMILY, NCALLEE_PROCESS};
    assert_eq!((NCALLEE_PROCESS, CHAINRE_PROCESS, CHAINN_PROCESS), (1, 95, 65536), "the shipped process defaults");
    let z = TrConfig::for_mode(TrMode::On).limits;
    assert_eq!((z.ncallee, z.chainre, z.chainn), (NCALLEE_PROCESS, CHAINRE_PROCESS, CHAINN_PROCESS), "struct defaults (tests) = shipped");
    let (n0, c0, m0) = (NCALLEE_PROCESS, CHAINRE_PROCESS, CHAINN_PROCESS);
    for (env, want) in [
        (None, (n0, c0, m0)),
        (Some(""), (n0, c0, m0)),
        (Some("nest=2"), (n0, c0, m0)),
        (Some("ncallee=0"), (0, c0, m0)),
        (Some("chainre=0"), (n0, 0, m0)),
        (Some("chainre=80,chainn=1000"), (n0, 80, 1000)),
        (Some("chainre=500"), (n0, 100, m0)),
        (Some("chainn=7"), (n0, c0, 7)),
        (Some("ncallee=5"), (1, c0, m0)),
    ] {
        let mut c = TrConfig::parse(None, None, None, env, None);
        c.apply_process_limits(env);
        let l = c.limits;
        assert_eq!((l.ncallee, l.chainre, l.chainn), want, "limits env {env:?}");
    }
    // ncallee_due: the knob and a callee guard in the entry segment of the flushed trace
    assert!(ncallee_due(1, true));
    assert!(!ncallee_due(0, true), "knob off");
    assert!(!ncallee_due(1, false), "no callee guard in the entry segment: nothing to move");
    // chain_due. v = [entries of link 0, calls leaving link 0, calls leaving link 1, ...]
    // the forth_vm shape: 10 links, the four hot ones behind links that carry a few calls
    let forth = [8_667_000u64, 8_666_600, 6_560_000, 6_464_000, 6_406_000, 4_305_000, 4_280_000, 2_179_000, 2_159_000, 30_000, 0];
    assert!(chain_due(95, 65536, &forth, 0, 0), "the hot callees sit behind pass-through links");
    assert!(!chain_due(0, 65536, &forth, 0, 0), "knob off");
    assert!(!chain_due(95, 9_000_000, &forth, 0, 0), "too few entries");
    // in hotness order already: the hot link first, then the cold ones (gain 0)
    let sorted = [1_000_000u64, 400_000, 200_000, 100_000, 50_000, 10_000, 0];
    assert!(!chain_due(30, 1000, &sorted, 0, 0), "a chain in hotness order is never re-ordered");
    // uniform: 16 equal shares; the first link passes 94 % through, the order cannot be improved
    let mut uni = vec![1_600_000u64];
    for i in 1..=16u64 {
        uni.push(1_600_000 - i * 100_000);
    }
    assert!(!chain_due(90, 1000, &uni, 0, 0), "a uniform chain (94 % pass-through at the first link) is not re-ordered");
    // the pass-through test: link 0 carries 40 % of its entries
    let mut half = forth;
    half[1] = 5_000_000;
    assert!(!chain_due(95, 65536, &half, 0, 0), "link 0 is not a pass-through");
    // two links only: the gain is too small
    assert!(!chain_due(95, 10, &[1000, 990, 0], 0, 0), "two links: the gain is below one link visit per entry");
    // bounds
    assert!(chain_due(95, 65536, &forth, CHAIN_MAX_EXIT - 1, CHAIN_MAX_FAMILY - 1));
    assert!(!chain_due(95, 65536, &forth, CHAIN_MAX_EXIT, 0), "per-exit bound");
    assert!(!chain_due(95, 65536, &forth, 0, CHAIN_MAX_FAMILY), "per-family bound");
}

/// BUNDLE-G `chaindepth`: process default, parsing and the pure refusal rule (`chain_refuse_rule`: `c[0]` calls reached the chain, `c[i]` left link `i`).
#[test]
fn bg_policy() {
    use rn_jit::tr::cache::{chain_refuse_rule, TrConfig, TrMode, CHAINDEPTH_PROCESS, CHAIN_COVER_PCT};
    assert_eq!(TrConfig::for_mode(TrMode::On).limits.chaindepth, CHAINDEPTH_PROCESS, "struct default (tests) = shipped");
    let p0 = CHAINDEPTH_PROCESS;
    for (env, want) in [(None, p0), (Some(""), p0), (Some("nest=2"), p0), (Some("chaindepth=0"), 0), (Some("chaindepth=4"), 4), (Some("chaindepth=9999"), 256)] {
        let mut c = TrConfig::parse(None, None, None, env, None);
        c.apply_process_limits(env);
        assert_eq!(c.limits.chaindepth, want, "limits env {env:?}");
    }
    let hot = 64u64;
    // 24 uniform callees, four links: 1000000 calls reach the chain, each link passes 23/24 .. 21/24 of its entries on
    let mut wide = vec![1_000_000u64];
    for i in 0..4u64 {
        let rest = wide[i as usize].saturating_sub(hot);
        wide.push(rest * (23 - i) / (24 - i));
    }
    assert!(chain_refuse_rule(4, &wide, hot), "a flat chain of 4 links serves {} % only", 100 - wide[4] * 100 / wide[0]);
    assert!(!chain_refuse_rule(0, &wide, hot), "knob off");
    assert!(!chain_refuse_rule(5, &wide, hot), "the chain is shorter than the cap");
    // 4 uniform callees: the four links serve everything (nothing is left) and 8 callees: half
    let narrow = [1_000_000u64, 750_000, 500_000, 250_000, 60_000];
    assert!(!chain_refuse_rule(4, &narrow, hot), "a narrow dispatch is covered by its links");
    let eight = [1_000_000u64, 875_000, 750_000, 625_000, 500_000];
    assert!(!chain_refuse_rule(4, &eight, hot), "8 callees: 50 % served, the chain grows on");
    // the boundary of the coverage test
    let edge_lo = [1_000_000u64, 900_000, 800_000, 750_000, 701_000];
    let edge_hi = [1_000_000u64, 900_000, 800_000, 750_000, 699_000];
    assert!(chain_refuse_rule(4, &edge_lo, 0), "just under {CHAIN_COVER_PCT} % served");
    assert!(!chain_refuse_rule(4, &edge_hi, 0), "just over");
    // no chain / single counter
    assert!(!chain_refuse_rule(4, &[], hot));
    assert!(!chain_refuse_rule(1, &[5], hot));
}

/// C38 slice 1: process default and parsing of `sidenest` (struct default = `SIDENEST_PROCESS`, an explicit value wins and clamps to 1).
#[test]
fn c38_policy() {
    use rn_jit::tr::cache::{TrConfig, TrMode, SIDENEST_PROCESS};
    let p0 = SIDENEST_PROCESS;
    assert_eq!(p0, 1, "the shipped process default (BUNDLE-N-B: on, with the inline re-push stub `retf` bit 1)");
    assert_eq!(TrConfig::for_mode(TrMode::On).limits.sidenest, p0, "struct default (tests) = shipped");
    for (env, want) in [(None, p0), (Some(""), p0), (Some("nest=2"), p0), (Some("sidenest=0"), 0), (Some("sidenest=1"), 1), (Some("sidenest=9"), 1)] {
        let mut c = TrConfig::parse(None, None, None, env, None);
        c.apply_process_limits(env);
        assert_eq!(c.limits.sidenest, want, "limits env {env:?}");
    }
}

/// C39: process default and pure rules of `innerretry`: struct default 0, an explicit value wins (clamped to 16), `inner_retry_due` is bounded by the knob and by
/// the reason, `TrProto::inner_chance` gives rounds with growing waits and one attempt each (the next `InnerLoop` abort blacklists again), a different head has its own count.
#[test]
fn c39_policy() {
    use rn_jit::tr::cache::{inner_retry_due, Penalty, TrConfig, TrMode, TrProto, INNERRETRY_PROCESS, INNERRETRY_WAIT_SHIFT, MAX_ABORTS};
    let p0 = INNERRETRY_PROCESS;
    assert_eq!(p0, 6, "the shipped process default");
    assert_eq!(TrConfig::for_mode(TrMode::On).limits.innerretry, INNERRETRY_PROCESS, "struct default (tests) = shipped");
    for (env, want) in [(None, p0), (Some(""), p0), (Some("nest=2"), p0), (Some("innerretry=0"), 0), (Some("innerretry=3"), 3), (Some("innerretry=99"), 16)] {
        let mut c = TrConfig::parse(None, None, None, env, None);
        c.apply_process_limits(env);
        assert_eq!(c.limits.innerretry, want, "limits env {env:?}");
    }
    // the pure rule
    assert!(inner_retry_due(6, true, "InnerLoop", 0));
    assert!(inner_retry_due(6, true, "InnerLoop", 5));
    assert!(!inner_retry_due(6, true, "InnerLoop", 6), "bounded by the knob");
    assert!(!inner_retry_due(0, true, "InnerLoop", 0), "off");
    assert!(!inner_retry_due(6, false, "InnerLoop", 0), "only a blacklisting abort");
    for r in ["LeaveFrame", "IntMiss", "LeaveLoop", "StartRefused", "TooLong", "Nyi", "SideShort"] {
        assert!(!inner_retry_due(6, true, r, 0), "{r}");
    }
    // TrProto: two InnerLoop aborts blacklist, then `max` rounds of one attempt with growing waits
    let tp = TrProto::new(10, &[3, 7], 256, true, 77, 1);
    assert!(matches!(tp.on_abort(0, "InnerLoop", true), Penalty::Wait(_)));
    assert_eq!(tp.on_abort(0, "InnerLoop", true), Penalty::Black);
    let mut last = 0;
    for round in 0..3u32 {
        match tp.inner_chance(0, 3) {
            Some(Penalty::Wait(w)) => {
                assert!(i64::from(w) >= 64 << (MAX_ABORTS + INNERRETRY_WAIT_SHIFT + round as u8), "round {round} waits {w}");
                assert!(i64::from(w) > last, "waits grow: {w} after {last}");
                last = i64::from(w);
            }
            o => panic!("round {round}: {o:?}"),
        }
        assert_eq!(tp.on_abort(0, "InnerLoop", true), Penalty::Black, "one attempt per round: the next InnerLoop abort blacklists");
    }
    assert_eq!(tp.inner_chance(0, 3), None, "bounded by the knob");
    assert!(matches!(tp.inner_chance(1, 3), Some(Penalty::Wait(_))), "the other head has its own count");
    assert_eq!(tp.inner_chance(1, 0), None, "knob 0");
    // follow-up: an ineffective flush of a trace installed after retry rounds blacklists at once (no re-record); without rounds the first one re-records
    let tq = TrProto::new(10, &[3, 7], 256, true, 77, 1);
    assert!(!matches!(tq.on_ineffective(0), Penalty::Black), "first ineffective flush without retry rounds: re-record");
    assert!(matches!(tq.on_abort(1, "InnerLoop", true), Penalty::Wait(_)));
    assert_eq!(tq.on_abort(1, "InnerLoop", true), Penalty::Black);
    assert!(matches!(tq.inner_chance(1, 3), Some(Penalty::Wait(_))));
    assert_eq!(tq.on_ineffective(1), Penalty::Black, "ineffective after a retry round: blacklisted for good");
}

/// BUNDLE-O: the process defaults of `polyside` / `polywait` are the shipped values, the struct defaults (tests) are off and an explicit value wins.
/// Mutant `polyside_default_ignored`.
#[test]
fn o_polyside_process_default() {
    use rn_jit::tr::cache::{TrConfig, TrMode, POLYFRAMES_PROCESS, POLYSIDE_PROCESS, POLYWAIT_PROCESS};
    let c0 = TrConfig::for_mode(TrMode::On);
    assert_eq!((c0.limits.polyside, c0.limits.polywait, c0.limits.polyframes), (POLYSIDE_PROCESS, POLYWAIT_PROCESS, POLYFRAMES_PROCESS));
    let mut c = TrConfig::parse(None, None, None, None, None);
    c.apply_process_limits(None);
    assert_eq!(c.limits.polyframes, POLYFRAMES_PROCESS);
    for (env, want) in [
        (None, (POLYSIDE_PROCESS, POLYWAIT_PROCESS)),
        (Some("nest=2"), (POLYSIDE_PROCESS, POLYWAIT_PROCESS)),
        (Some("polyside=0"), (0, POLYWAIT_PROCESS)),
        (Some("polywait=9"), (POLYSIDE_PROCESS, 9)),
        (Some("polyside=5,polywait=7"), (5, 7)),
    ] {
        let mut c = TrConfig::parse(None, None, None, env, None);
        c.apply_process_limits(env);
        assert_eq!((c.limits.polyside, c.limits.polywait), want, "limits env {env:?}");
    }
}

/// BUNDLE-S: `loopany` is on in a process (guarded by `loopanymax`), off as the struct default (tests), and an explicit value wins.
/// Mutant `loopany_default_ignored`.
#[test]
fn s_loopany_process_default() {
    use rn_jit::tr::cache::{TrConfig, TrMode, LOOPANY_PROCESS};
    assert_eq!(LOOPANY_PROCESS, 1);
    let c0 = TrConfig::for_mode(TrMode::On);
    assert_eq!((c0.limits.loopany, c0.limits.loopanymax), (LOOPANY_PROCESS, 4));
    for (env, want) in [(None, (1, 4)), (Some(""), (1, 4)), (Some("nest=2"), (1, 4)), (Some("loopany=0"), (0, 4)), (Some("loopany=1,loopanymax=9"), (1, 9)), (Some("loopanymax=5"), (1, 5))] {
        let mut c = TrConfig::parse(None, None, None, env, None);
        c.apply_process_limits(env);
        assert_eq!((c.limits.loopany, c.limits.loopanymax), want, "limits env {env:?}");
    }
}

/// BUNDLE-P: `laprune` process default (struct default 0, explicit value wins) and the pure policy of the `loopany` guard: a root that unrolled a callee
/// loop (the `heapsort` shape) flushes the whole family, a root that unrolled nothing prunes only the unrolling side traces of its family.
/// Mutant `laprune_flushes_root`.
#[test]
fn p_laprune_policy() {
    use rn_jit::tr::cache::{la_is_victim, la_plan, LaPlan, TrConfig, TrMode, LAPRUNE_PROCESS};
    assert_eq!(LAPRUNE_PROCESS, 1);
    assert_eq!(TrConfig::for_mode(TrMode::On).limits.laprune, LAPRUNE_PROCESS);
    for (env, want) in [(None, 1), (Some(""), 1), (Some("loopany=1"), 1), (Some("laprune=0"), 0), (Some("laprune=1,loopanymax=2"), 1)] {
        let mut c = TrConfig::parse(None, None, None, env, None);
        c.apply_process_limits(env);
        assert_eq!(c.limits.laprune, want, "limits env {env:?}");
    }
    // (root_unrolled: whole family, in every setting of the knob; root unrolled nothing: sides only when the knob is on)
    assert_eq!(la_plan(0, false), LaPlan::Family);
    assert_eq!(la_plan(0, true), LaPlan::Family);
    assert_eq!(la_plan(1, true), LaPlan::Family);
    assert_eq!(la_plan(1, false), LaPlan::Sides);
    // victims: a side trace of the firing family with a non-empty unrolled-site list, never the root, never another family, never a side without sites
    assert!(la_is_victim(true, true, true));
    assert!(!la_is_victim(true, false, true), "the root stays");
    assert!(!la_is_victim(false, true, true), "other families stay");
    assert!(!la_is_victim(true, true, false), "sides that unrolled nothing stay");
}

/// DYNMASK: the process default of `dynmask` is the shipped value, the struct default (tests) is off and an explicit value wins.
#[test]
fn dynmask_process_default() {
    use rn_jit::tr::cache::{TrConfig, TrMode, DYNMASK_PROCESS};
    assert_eq!(TrConfig::for_mode(TrMode::On).limits.dynmask, DYNMASK_PROCESS);
    for (env, want) in [(None, DYNMASK_PROCESS), (Some("nest=2"), DYNMASK_PROCESS), (Some("dynmask=1"), 1), (Some("dynmask=0"), 0), (Some("dynmask=7"), 1)] {
        let mut c = TrConfig::parse(None, None, None, env, None);
        c.apply_process_limits(env);
        assert_eq!(c.limits.dynmask, want, "limits env {env:?}");
    }
}

/// P2: the process defaults of `ineffcover` and `typesides` are on; the struct defaults (tests) are off and an explicit value wins.
/// Mutant `p2_default_ignored`.
#[test]
fn p2_process_default() {
    use rn_jit::tr::cache::{TrConfig, TrMode, INEFFCOVER_PROCESS, TYPESIDES_PROCESS};
    assert_eq!((INEFFCOVER_PROCESS, TYPESIDES_PROCESS), (1, 1));
    let c = TrConfig::for_mode(TrMode::On);
    assert_eq!((c.limits.ineffcover, c.limits.typesides), (INEFFCOVER_PROCESS, TYPESIDES_PROCESS));
    for (env, want) in [
        (None, (1, 1)),
        (Some(""), (1, 1)),
        (Some("nest=2"), (1, 1)),
        (Some("ineffcover=0"), (0, 1)),
        (Some("typesides=0"), (1, 0)),
        (Some("typesides=0,ineffcover=0"), (0, 0)),
        (Some("maxside=9,typesides=1"), (1, 1)),
    ] {
        let mut c = TrConfig::parse(None, None, None, env, None);
        c.apply_process_limits(env);
        assert_eq!((c.limits.ineffcover, c.limits.typesides), want, "limits env {env:?}");
    }
}

/// R2: the process defaults turn `recany`, `siderooms` and `nilstore` on; the struct defaults (tests) have them off and an explicit
/// value wins. Mutant `r2_default_ignored`.
#[test]
fn r2_process_defaults() {
    use rn_jit::tr::cache::{TrConfig, TrMode};
    let c = TrConfig::for_mode(TrMode::On);
    assert_eq!((c.limits.recany, c.limits.siderooms, c.limits.nilstore), (rn_jit::tr::cache::RECANY_PROCESS, rn_jit::tr::cache::SIDEROOMS_PROCESS, rn_jit::tr::cache::NILSTORE_PROCESS));
    for (env, want) in [
        (None, (1, 1, 1)),
        (Some(""), (1, 1, 1)),
        (Some("nest=2"), (1, 1, 1)),
        (Some("recany=0"), (0, 1, 1)),
        (Some("siderooms=0"), (1, 0, 1)),
        (Some("nilstore=0,maxside=9"), (1, 1, 0)),
    ] {
        let mut c = TrConfig::parse(None, None, None, env, None);
        c.apply_process_limits(env);
        assert_eq!((c.limits.recany, c.limits.siderooms, c.limits.nilstore), want, "limits env {env:?}");
    }
}

/// B5: the BUNDLE-5 knobs are off in a process (their gate was not met, see docs/JIT_TRACE_REF.md); an explicit value wins.
/// Mutant `b5_default_ignored` (a process default that overrides the named value).
#[test]
fn b5_process_defaults() {
    use rn_jit::tr::cache::{TrConfig, TrMode};
    use rn_jit::tr::cache::{HOPTOP_PROCESS, SINKMT_PROCESS, SINKSIDE_PROCESS};
    let get = |l: &rn_jit::tr::cache::TrLimits| (l.sinkmt, l.sinkside);
    assert_eq!(get(&TrConfig::for_mode(TrMode::On).limits), (SINKMT_PROCESS, SINKSIDE_PROCESS));
    // BUNDLE-P: no clock in the default policy; the counting rule `hoptop` is the process default
    assert_eq!(HOPTOP_PROCESS, 98);
    assert_eq!(TrConfig::for_mode(TrMode::On).limits.hoptop, HOPTOP_PROCESS);
    for (env, want) in [(None, HOPTOP_PROCESS), (Some("nest=2"), HOPTOP_PROCESS), (Some("hoptop=0"), 0), (Some("hoptop=90,maxside=9"), 90)] {
        let mut c = TrConfig::parse(None, None, None, env, None);
        c.apply_process_limits(env);
        assert_eq!(c.limits.hoptop, want, "limits env {env:?}");
    }
    for (env, want) in [
        (None, (SINKMT_PROCESS, SINKSIDE_PROCESS)),
        (Some("sinkmt=1"), (1, SINKSIDE_PROCESS)),
        (Some("sinkside=1"), (SINKMT_PROCESS, 1)),
        (Some("sinkmt=1,sinkside=1"), (1, 1)),
        (Some("sinkmt=0,sinkside=0"), (0, 0)),
        (Some("sinkmt=0,maxside=9"), (0, SINKSIDE_PROCESS)),
    ] {
        let mut c = TrConfig::parse(None, None, None, env, None);
        c.apply_process_limits(env);
        assert_eq!(get(&c.limits), want, "limits env {env:?}");
    }
}

/// J1-2: `sinkstub` (the inline materialising exit stub) is 0 in a bare config; a process
/// takes `SINKSTUB_PROCESS` (1 since SINK-F) unless the limits env names it.
#[test]
fn j1_process_defaults() {
    use rn_jit::tr::cache::{TrConfig, TrMode, SINKSTUB_PROCESS};
    let get = |l: &rn_jit::tr::cache::TrLimits| l.sinkstub;
    assert_eq!(get(&TrConfig::for_mode(TrMode::On).limits), SINKSTUB_PROCESS);
    for (env, want) in [
        (None, SINKSTUB_PROCESS),
        (Some("warm=0"), SINKSTUB_PROCESS),
        (Some("sinkstub=1"), 1),
        (Some("sinkstub=0"), 0),
    ] {
        let mut c = TrConfig::parse(None, None, None, env, None);
        c.apply_process_limits(env);
        assert_eq!(get(&c.limits), want, "limits env {env:?}");
    }
}

/// SINK-R: `sinkroot` (the sink-aware ineffective-root rule) is 0 in a bare config; a process takes `SINKROOT_PROCESS` unless the limits env names it.
/// Mutant `sinkroot_default_ignored`.
#[test]
fn sinkroot_process_defaults() {
    use rn_jit::tr::cache::{TrConfig, TrMode, SINKROOT_PROCESS};
    assert_eq!(TrConfig::for_mode(TrMode::On).limits.sinkroot, SINKROOT_PROCESS);
    for (env, want) in [(None, SINKROOT_PROCESS), (Some("warm=0"), SINKROOT_PROCESS), (Some("sinkroot=0"), 0), (Some("sinkroot=1"), 1), (Some("sinkroot=2"), 2), (Some("sinkroot=9"), 2)] {
        let mut c = TrConfig::parse(None, None, None, env, None);
        c.apply_process_limits(env);
        assert_eq!(c.limits.sinkroot, want, "limits env {env:?}");
    }
}

/// BUNDLE-B item 3: a process caps the side traces of a family at `MAXSIDE_PROCESS`; the struct default stays 64 and a named value wins.
/// Mutant `bb_default_ignored` (the process default overrides the named value).
#[test]
fn bb_process_defaults() {
    use rn_jit::tr::cache::{TrConfig, TrMode, MAXSIDE_PROCESS, TSET_PROCESS};
    assert_eq!(MAXSIDE_PROCESS, 16);
    assert_eq!(TSET_PROCESS, 15);
    assert_eq!(TrConfig::for_mode(TrMode::On).limits.maxside, MAXSIDE_PROCESS);
    assert_eq!(TrConfig::for_mode(TrMode::On).limits.tset, TSET_PROCESS);
    for (env, want) in [(None, 15), (Some("warm=0"), 15), (Some("tset=0"), 0), (Some("tset=3,maxside=9"), 3), (Some("tset=9"), 9), (Some("tset=99"), 15)] {
        let mut c = TrConfig::parse(None, None, None, env, None);
        c.apply_process_limits(env);
        assert_eq!(c.limits.tset, want, "limits env {env:?}");
    }
    for (env, want) in [
        (None, MAXSIDE_PROCESS),
        (Some(""), MAXSIDE_PROCESS),
        (Some("warm=0"), MAXSIDE_PROCESS),
        (Some("maxside=64"), 64),
        (Some("maxside=0"), 0),
        (Some("warm=0,maxside=9"), 9),
    ] {
        let mut c = TrConfig::parse(None, None, None, env, None);
        c.apply_process_limits(env);
        assert_eq!(c.limits.maxside, want, "limits env {env:?}");
    }
}

/// P2 bundle 3: the process default of `pollr15` is 2 = on except in native self-recursion traces (`POLLR15_PROCESS`; 0 between the P2 fix and the 2026-11-12 regression fix). K3b: `budsub` and `fpool` are hard-wired on.
/// Mutant `loopdiet_default_ignored`.
#[test]
fn p2_loopdiet_process_defaults() {
    use rn_jit::tr::cache::{TrConfig, TrMode};
    let c = TrConfig::for_mode(TrMode::On);
    assert_eq!(c.limits.pollr15, rn_jit::tr::cache::POLLR15_PROCESS);
    for (env, want) in [(None, 2), (Some(""), 2), (Some("nest=2"), 2), (Some("pollr15=1,maxside=9"), 1), (Some("pollr15=0"), 0)] {
        let mut c = TrConfig::parse(None, None, None, env, None);
        c.apply_process_limits(env);
        assert_eq!(c.limits.pollr15, want, "limits env {env:?}");
    }
}

/// P2 bundle 1: the process default of `xregs` (`XREGS_PROCESS`, 0 = off while the A/B gate is open); the struct default (tests) is 0 and an explicit value wins and is clamped.
#[test]
fn p2_xregs_process_default() {
    use rn_jit::tr::cache::{TrConfig, TrMode, XREGS_PROCESS};
    assert_eq!(TrConfig::for_mode(TrMode::On).limits.xregs, 0);
    let d = XREGS_PROCESS;
    for (env, want) in [(None, d), (Some(""), d), (Some("nest=2"), d), (Some("xregs=0"), 0), (Some("maxside=9,xregs=1"), 1), (Some("xregs=2"), 2), (Some("xregs=3"), 3), (Some("xregs=9"), 3)] {
        let mut c = TrConfig::parse(None, None, None, env, None);
        c.apply_process_limits(env);
        assert_eq!(c.limits.xregs, want, "limits env {env:?}");
    }
}

/// BUNDLE-H: the process default of `libcall` (`LIBCALL_PROCESS`); the struct default is the same, an explicit value wins and is clamped to 1023.
#[test]
fn bh_libcall_process_default() {
    use rn_jit::tr::cache::{TrConfig, TrMode, LIBCALL_PROCESS};
    assert_eq!(TrConfig::for_mode(TrMode::On).limits.libcall, LIBCALL_PROCESS);
    let d = LIBCALL_PROCESS;
    for (env, want) in [(None, d), (Some(""), d), (Some("nest=2"), d), (Some("libcall=0"), 0), (Some("maxside=9,libcall=1"), 1), (Some("libcall=7"), 7), (Some("libcall=9999"), 1023)] {
        let mut c = TrConfig::parse(None, None, None, env, None);
        c.apply_process_limits(env);
        assert_eq!(c.limits.libcall, want, "limits env {env:?}");
    }
}

/// BUNDLE-N: `retf` is 3 by default (side RETF recording + inline re-push stub; function-entry families, bit 2, stay off); an explicit value wins and is clamped to 15 (`retfmax` to 8).
#[test]
fn bn_retf_process_default() {
    use rn_jit::tr::cache::{TrConfig, TrMode, RETFMAX_PROCESS, RETF_PROCESS};
    assert_eq!(RETF_PROCESS, 3, "side RETF recording (bit 0) and the inline re-push stub (bit 1) ship on; function-entry families (bit 2) stay off");
    assert_eq!(RETFMAX_PROCESS, 1);
    for (env, want, wmax) in [(None, RETF_PROCESS, RETFMAX_PROCESS), (Some("retf=1"), 1, RETFMAX_PROCESS), (Some("retf=99,retfmax=2"), 15, 2), (Some("retfmax=99"), RETF_PROCESS, 8)] {
        let mut c = TrConfig::parse(None, None, None, env, None);
        c.apply_process_limits(env);
        assert_eq!((c.limits.retf, c.limits.retfmax), (want, wmax), "limits env {env:?}");
        let _ = TrMode::On;
    }
}

/// N3a: the room a side trace attached to a segment-k exit may count on is the parent's entry-relative extent shifted by the segment's slots and its spare `CallInfo`
/// nodes plus the popped frames' (mutants `retf_extent_unshifted`, `retf_maxci_unshifted`).
#[test]
fn n3a_child_room_is_translated_by_the_segment() {
    use rn_jit::tr::cache::child_room;
    assert_eq!(child_room(40, 2, 0, 0), (40, 2), "segment 0 is the entry frame");
    assert_eq!(child_room(40, 2, 7, 1), (47, 3));
    assert_eq!(child_room(40, 2, 19, 2), (59, 4));
    assert_eq!(child_room(u16::MAX - 1, 254, 9, 3), (u16::MAX, 255), "saturates");
}

/// BUNDLE-H: the process default of `tnextinl` (`TNEXTINL_PROCESS`); an explicit value wins and is clamped to 1.
#[test]
fn bh_tnextinl_process_default() {
    use rn_jit::tr::cache::{TrConfig, TrMode, TNEXTINL_PROCESS};
    assert_eq!(TrConfig::for_mode(TrMode::On).limits.tnextinl, TNEXTINL_PROCESS);
    let d = TNEXTINL_PROCESS;
    for (env, want) in [(None, d), (Some(""), d), (Some("tnextinl=0"), 0), (Some("maxside=9,tnextinl=1"), 1), (Some("tnextinl=5"), 1)] {
        let mut c = TrConfig::parse(None, None, None, env, None);
        c.apply_process_limits(env);
        assert_eq!(c.limits.tnextinl, want, "limits env {env:?}");
    }
}

/// BUNDLE-J: the process default of `centry` (`CENTRY_PROCESS`) and of the weight rule `centrymin`; an explicit value wins and `centry` is clamped to 2.
#[test]
fn bj_centry_process_default() {
    use rn_jit::tr::cache::{TrConfig, TrMode, CENTRY_MIN_IR, CENTRY_PROCESS};
    assert_eq!(TrConfig::for_mode(TrMode::On).limits.centry, CENTRY_PROCESS);
    assert_eq!(TrConfig::for_mode(TrMode::On).limits.centrymin, CENTRY_MIN_IR);
    let d = CENTRY_PROCESS;
    for (env, want) in [(None, d), (Some(""), d), (Some("centry=0"), 0), (Some("maxside=9,centry=1"), 1), (Some("centry=2"), 2), (Some("centry=9"), 2)] {
        let mut c = TrConfig::parse(None, None, None, env, None);
        c.apply_process_limits(env);
        assert_eq!(c.limits.centry, want, "limits env {env:?}");
    }
    let c = TrConfig::parse(None, None, None, Some("centrymin=0"), None);
    assert_eq!(c.limits.centrymin, 0);
}

/// R1: the process default of `rearm` (`REARM_PROCESS`); the struct default (tests) is 0, an explicit value wins and is clamped to 64. Mutant `rearm_default_ignored`.
#[test]
fn r1_rearm_process_default() {
    use rn_jit::tr::cache::{TrConfig, TrMode, REARM_PROCESS};
    assert_eq!(TrConfig::for_mode(TrMode::On).limits.rearm, REARM_PROCESS);
    let d = REARM_PROCESS;
    for (env, want) in [(None, d), (Some(""), d), (Some("nest=2"), d), (Some("rearm=0"), 0), (Some("maxside=9,rearm=1"), 1), (Some("rearm=16"), 16), (Some("rearm=999"), 64)] {
        let mut c = TrConfig::parse(None, None, None, env, None);
        c.apply_process_limits(env);
        assert_eq!(c.limits.rearm, want, "limits env {env:?}");
    }
}

/// P-F: the process default refuses side traces of fewer than 4 bytecodes (`fannkuch`: a 3-bytecode path is cheaper in tier 1 than
/// behind a side-trace hop); the struct default (tests) has no minimum and an explicit `sidemin=` wins. Mutant
/// `sidemin_default_ignored`.
#[test]
fn pf_process_default_sidemin() {
    use rn_jit::tr::cache::{TrConfig, TrMode, SIDEMIN_PROCESS};
    assert_eq!(SIDEMIN_PROCESS, 4);
    assert_eq!(TrConfig::for_mode(TrMode::On).limits.sidemin, 0);
    for (env, want) in [(None, 4), (Some(""), 4), (Some("nest=2"), 4), (Some("sidemin=0"), 0), (Some("maxside=9,sidemin=7"), 7)] {
        let mut c = TrConfig::parse(None, None, None, env, None);
        c.apply_process_limits(env);
        assert_eq!(c.limits.sidemin, want, "limits env {env:?}");
    }
}

/// BUNDLE-1: the process defaults of `wgate` (the deterministic start gate), `tailhook` and `hdrlib`; the struct defaults (tests) are 0, an explicit
/// value wins, and naming `warm` selects the old time gate (`warm=0` switches every gate off). Mutant `b1_default_ignored`.
#[test]
fn b1_process_defaults() {
    use rn_jit::tr::cache::{TrConfig, TrMode, HDRLIB_PROCESS, TAILHOOK_PROCESS, WGATE_PROCESS};
    let c = TrConfig::for_mode(TrMode::On);
    assert_eq!((c.limits.wgate, c.limits.tailhook, c.limits.hdrlib), (0, TAILHOOK_PROCESS, HDRLIB_PROCESS), "the work gate is a start gate: off in tests, WGATE_PROCESS in a process");
    for (env, want) in [
        (None, (WGATE_PROCESS, TAILHOOK_PROCESS, HDRLIB_PROCESS)),
        (Some(""), (WGATE_PROCESS, TAILHOOK_PROCESS, HDRLIB_PROCESS)),
        (Some("nest=2"), (WGATE_PROCESS, TAILHOOK_PROCESS, HDRLIB_PROCESS)),
        (Some("warm=0"), (0, TAILHOOK_PROCESS, HDRLIB_PROCESS)),
        (Some("warm=5"), (0, TAILHOOK_PROCESS, HDRLIB_PROCESS)),
        (Some("wgate=7"), (7, TAILHOOK_PROCESS, HDRLIB_PROCESS)),
        (Some("tailhook=0"), (WGATE_PROCESS, 0, HDRLIB_PROCESS)),
        (Some("hdrlib=1,tailhook=2"), (WGATE_PROCESS, 2, 1)),
    ] {
        let mut c = TrConfig::parse(None, None, None, env, None);
        c.apply_process_limits(env);
        assert_eq!((c.limits.wgate, c.limits.tailhook, c.limits.hdrlib), want, "limits env {env:?}");
    }
}

/// BUNDLE-C: `t1ccall` and `tfmt` are the `*_PROCESS` constants in the struct default (tests) and in a process (N5: `tfmt` ships on, `t1ccall` off); a named value wins and the knobs
/// are independent of each other. Mutant `bundlec_default_ignored`.
#[test]
fn bc_process_defaults() {
    use rn_jit::tr::cache::{T1CCALL_PROCESS, TFMT_PROCESS};
    let d = TrConfig::for_mode(rn_jit::tr::cache::TrMode::On);
    assert_eq!((d.limits.t1ccall, d.limits.tfmt), (T1CCALL_PROCESS, TFMT_PROCESS));
    for (env, want) in [
        (None, (T1CCALL_PROCESS, TFMT_PROCESS)),
        (Some("hdrlib=1"), (T1CCALL_PROCESS, TFMT_PROCESS)),
        (Some("t1ccall=1"), (1, TFMT_PROCESS)),
        (Some("tfmt=1"), (T1CCALL_PROCESS, 1)),
        (Some("t1ccall=0,tfmt=0"), (0, 0)),
        (Some("t1ccall=1,tfmt=1"), (1, 1)),
    ] {
        let mut c = TrConfig::parse(None, None, None, env, None);
        c.apply_process_limits(env);
        assert_eq!((c.limits.t1ccall, c.limits.tfmt), want, "limits env {env:?}");
    }
}

/// BUNDLE-D: `ccalls`, `armm` and `sbump2` are 0 in the struct default (tests) and the `*_PROCESS` constants in a process; a named value wins, the knobs are
/// independent of each other, and naming `sbump2` leaves the process default of `sbump` alone (and the reverse).
#[test]
fn bd_process_defaults() {
    use rn_jit::tr::cache::{ARMM_PROCESS, CCALLS_PROCESS, SBUMP2_PROCESS, SBUMP_PROCESS};
    let d = TrConfig::for_mode(rn_jit::tr::cache::TrMode::On);
    assert_eq!((d.limits.ccalls, d.limits.armm, d.limits.sbump2, d.limits.sbump), (CCALLS_PROCESS, ARMM_PROCESS, SBUMP2_PROCESS, SBUMP_PROCESS));
    for (env, want) in [
        (None, (CCALLS_PROCESS, ARMM_PROCESS, SBUMP2_PROCESS, SBUMP_PROCESS)),
        (Some("hdrlib=1"), (CCALLS_PROCESS, ARMM_PROCESS, SBUMP2_PROCESS, SBUMP_PROCESS)),
        (Some("ccalls=1"), (1, ARMM_PROCESS, SBUMP2_PROCESS, SBUMP_PROCESS)),
        (Some("armm=1"), (CCALLS_PROCESS, 1, SBUMP2_PROCESS, SBUMP_PROCESS)),
        (Some("sbump2=2"), (CCALLS_PROCESS, ARMM_PROCESS, 2, SBUMP_PROCESS)),
        (Some("sbump=3"), (CCALLS_PROCESS, ARMM_PROCESS, SBUMP2_PROCESS, 3)),
        (Some("ccalls=0,armm=0,sbump2=0"), (0, 0, 0, SBUMP_PROCESS)),
    ] {
        let mut c = TrConfig::parse(None, None, None, env, None);
        c.apply_process_limits(env);
        assert_eq!((c.limits.ccalls, c.limits.armm, c.limits.sbump2, c.limits.sbump), want, "limits env {env:?}");
    }
}

/// BUNDLE-J2: the process default of the deep re-record (`deep`); the struct default (tests)
/// is 0 (and `deepins` 300), a named value wins; a knob whose name merely starts with `deep` (`deepins`, `deephop`) does not count as naming it (K1).
/// Mutant `deep_default_ignored`.
#[test]
fn j2_process_defaults() {
    use rn_jit::tr::cache::{TrConfig, TrMode, DEEP_PROCESS};
    let l = TrConfig::for_mode(TrMode::On).limits;
    assert_eq!((l.deep, l.deepins, l.deephop), (DEEP_PROCESS, 300, 1));
    let get = |l: &rn_jit::tr::cache::TrLimits| l.deep;
    for (env, want) in [
        (None, DEEP_PROCESS),
        (Some("warm=0"), DEEP_PROCESS),
        (Some("deep=6"), 6),
        (Some("deep=0"), 0),
        (Some("deep=99"), 8),
        (Some("deepins=200"), DEEP_PROCESS),
        (Some("deephop=0,deepins=500"), DEEP_PROCESS),
        (Some("deep=7,deepins=2"), 7),
    ] {
        let mut c = TrConfig::parse(None, None, None, env, None);
        c.apply_process_limits(env);
        assert_eq!(get(&c.limits), want, "limits env {env:?}");
    }
}

/// K1: a process default is skipped only for a knob the limits string names as a key (`key=value`), not for a string that merely contains
/// the name (`deepins=` used to switch `deep` off, `monwarm=` (a K2-removed knob) counted as `warm`, `loopanymax=` as `loopany`, `fnpromow=` as `fnpromo`).
#[test]
fn k1_names_limit_is_an_exact_key() {
    use rn_jit::tr::cache::{names_limit, TrConfig, TrMode, DEEP_PROCESS, FNPROMO_PROCESS, LOOPANY_PROCESS, WGATE_PROCESS};
    assert!(!names_limit(None, "deep"));
    assert!(names_limit(Some("deep=3"), "deep"));
    assert!(names_limit(Some("maxside=9,deep=3"), "deep"));
    assert!(names_limit(Some("maxside=9 deep=3"), "deep"));
    assert!(names_limit(Some("deep"), "deep"), "a bare item is the key too (the parser reports it as bad, the default is still skipped)");
    for s in ["deepins=300", "deephop=0", "xdeep=1", "deepx=1", "maxside=9,deepins=1", "xwarm=1"] {
        assert!(!names_limit(Some(s), "deep"), "{s}");
    }
    assert!(!names_limit(Some("xwarm=1"), "warm"));
    let process = |env: &str| {
        let mut c = TrConfig::parse(None, None, None, Some(env), None);
        c.apply_process_limits(Some(env));
        c.limits
    };
    let _ = TrConfig::for_mode(TrMode::On);
    assert_eq!(process("deepins=200").deep, DEEP_PROCESS);
    assert_eq!(process("deephop=0").deep, DEEP_PROCESS);
    assert_eq!(process("deep=2").deep, 2);
    assert_eq!(process("xwarm=1").wgate, WGATE_PROCESS, "xwarm does not name warm");
    assert_eq!(process("warm=5").wgate, 0, "warm named, wgate not: the work gate is off");
    assert_eq!(process("warm=5,wgate=77").wgate, 77);
    assert_eq!(process("loopanymax=2").loopany, LOOPANY_PROCESS);
    assert_eq!(process("loopany=0,loopanymax=2").loopany, 0);
    assert_eq!(process("fnpromow=1").fnpromo, FNPROMO_PROCESS);
    assert_eq!(process("fnpromo=0,fnpromow=1").fnpromo, 0);
}

/// J2-2: the hop-rate rule of the ineffective-trace check is skipped for a root recorded with the deep limits (only with `ineffcover`); and
/// a head gets its deeper re-record exactly once (`TrProto::on_deep`). Mutants `deep_hop_rule_kept`, `deep_every_time`.
#[test]
fn j2_deep_site_state() {
    use rn_jit::tr::cache::{hop_rule_applies, Penalty, TrProto};
    assert!(hop_rule_applies(true, false));
    assert!(!hop_rule_applies(true, true));
    assert!(!hop_rule_applies(false, false));
    assert!(!hop_rule_applies(false, true));
    let tp = TrProto::new(40, &[5, 17], 56, true, 7, 1);
    for i in 0..2 {
        assert!(!tp.head_deep(i));
    }
    assert!(tp.on_deep(0), "the first limit abort of a site earns the deeper re-record");
    assert!(tp.head_deep(0) && !tp.head_deep(1), "only the site itself");
    assert!(!tp.on_deep(0), "the second one does not: no re-record loop");
    assert!(!tp.on_deep(0));
    // the normal penalty path is untouched for a deep site: a repeated unchangeable abort blacklists
    assert!(matches!(tp.on_abort(0, "Recursion", true), Penalty::Wait(_)));
    assert_eq!(tp.on_abort(0, "Recursion", true), Penalty::Black);
    assert!(tp.on_deep(1) && !tp.on_deep(1));
}

/// BUNDLE-M: the process default of `lockstep` is the shipped value, the struct default (tests) is the
/// pre-M behaviour (lockstep on), and an explicit value wins (no mutant: no shipped default differs from the struct
/// default, so ignoring the process defaults would be an equivalent mutation).
#[test]
fn m_process_defaults() {
    use rn_jit::tr::cache::{TrConfig, TrMode, LOCKSTEP_PROCESS};
    let c0 = TrConfig::for_mode(TrMode::On);
    assert_eq!(c0.limits.lockstep, 1);
    for (env, ls) in [
        (None, LOCKSTEP_PROCESS),
        (Some(""), LOCKSTEP_PROCESS),
        (Some("nest=2"), LOCKSTEP_PROCESS),
        (Some("lockstep=0"), 0),
        (Some("lockstep=1,maxside=3"), 1),
        (Some("lockstep=8"), 8),
    ] {
        let mut c = TrConfig::parse(None, None, None, env, None);
        c.apply_process_limits(env);
        assert_eq!(c.limits.lockstep, ls, "limits env {env:?}");
    }
}

/// BUNDLE-C: process defaults of `sbump`, `fnpromo`, `fnpromow` (struct defaults 0, an explicit value wins) and the once-per-prototype promotion of `TrProto`.
/// Mutants `fnpromo_never` (the live program `tr_core::bc_fnpromo_programs`), `fnpromo_repeats`.
#[test]
fn bc_policy() {
    use rn_jit::tr::cache::{Penalty, TrConfig, TrMode, TrProto, FNPROMOW_PROCESS, FNPROMO_PROCESS, MAX_ABORTS, SBUMP_PROCESS};
    assert_eq!((SBUMP_PROCESS, FNPROMO_PROCESS, FNPROMOW_PROCESS), (1, 6, 3), "the shipped process defaults");
    let z = TrConfig::for_mode(TrMode::On).limits;
    assert_eq!((z.sbump, z.fnpromo, z.fnpromow), (SBUMP_PROCESS, FNPROMO_PROCESS, FNPROMOW_PROCESS), "struct defaults (tests) = shipped");
    for (env, want) in [
        (None, (1, 6, 3)),
        (Some(""), (1, 6, 3)),
        (Some("nest=2"), (1, 6, 3)),
        (Some("sbump=0"), (0, 6, 3)),
        (Some("sbump=2"), (2, 6, 3)),
        (Some("sbump=9"), (4, 6, 3)),
        (Some("fnpromo=0"), (1, 0, 3)),
        (Some("fnpromo=2"), (1, 2, 3)),
        (Some("fnpromo=2,fnpromow=0"), (1, 2, 0)),
        (Some("fnpromow=5"), (1, 6, 5)),
    ] {
        let mut c = TrConfig::parse(None, None, None, env, None);
        c.apply_process_limits(env);
        let l = c.limits;
        assert_eq!((l.sbump, l.fnpromo, l.fnpromow), want, "limits env {env:?}");
    }
    // promotion: once per prototype, a fresh abort budget, the wait is 64 << w
    let tp = TrProto::new(10, &[3, 7], 256, true, 77, 1);
    assert!(!tp.fn_promoted());
    match tp.promote_fn(0, 3) {
        Some(Penalty::Wait(w)) => assert!(w >= 64 << 3 && w < (64 << 3) + 64, "wait {w}"),
        o => panic!("first promotion: {o:?}"),
    }
    assert!(tp.fn_promoted());
    assert_eq!(tp.promote_fn(0, 3), None, "once per prototype");
    assert_eq!(tp.promote_fn(1, 3), None, "the flag is the prototype's");
    for _ in 0..MAX_ABORTS - 1 {
        assert!(matches!(tp.on_abort(0, "LeaveFrame", false), Penalty::Wait(_)), "fresh abort budget after the promotion");
    }
}

/// BUNDLE-F: `stitch0` and `fibdiet` are 0 in the struct default (tests) and the `*_PROCESS` constants in a process; a named value wins and the two are
/// independent of each other.
#[test]
fn bf_process_defaults() {
    use rn_jit::tr::cache::{FIBDIET_PROCESS, STITCH0_PROCESS};
    let d = TrConfig::for_mode(TrMode::On);
    assert_eq!((d.limits.stitch0, d.limits.fibdiet), (STITCH0_PROCESS, FIBDIET_PROCESS));
    for (env, want) in [
        (None, (STITCH0_PROCESS, FIBDIET_PROCESS)),
        (Some("hdrlib=1"), (STITCH0_PROCESS, FIBDIET_PROCESS)),
        (Some("stitch0=0"), (0, FIBDIET_PROCESS)),
        (Some("stitch0=1"), (1, FIBDIET_PROCESS)),
        (Some("fibdiet=1"), (STITCH0_PROCESS, 1)),
        (Some("fibdiet=0,stitch0=1"), (1, 0)),
    ] {
        let mut c = TrConfig::parse(None, None, None, env, None);
        c.apply_process_limits(env);
        assert_eq!((c.limits.stitch0, c.limits.fibdiet), want, "limits env {env:?}");
    }
}

/// BUNDLE-P / C45: the process defaults of `f2ifl` (`F2IFL_PROCESS`), `rearmside` (`REARMSIDE_PROCESS` = 2) and `rearmclean` (`REARMCLEAN_PROCESS` = 1); the struct default equals
/// them (K3a), a named value wins (`rearmside` is clamped to 64, `f2ifl` and `rearmclean` to 1). Mutant `rearmclean_default_ignored`.
#[test]
fn bp_process_defaults() {
    use rn_jit::tr::cache::{TrConfig, TrMode, F2IFL_PROCESS, REARMCLEAN_PROCESS, REARMSIDE_PROCESS};
    assert_eq!((REARMSIDE_PROCESS, REARMCLEAN_PROCESS), (2, 1));
    let c = TrConfig::for_mode(TrMode::On);
    assert_eq!((c.limits.f2ifl, c.limits.rearmside, c.limits.rearmclean), (F2IFL_PROCESS, REARMSIDE_PROCESS, REARMCLEAN_PROCESS));
    for (env, want) in [
        (None, (F2IFL_PROCESS, REARMSIDE_PROCESS, REARMCLEAN_PROCESS)),
        (Some(""), (F2IFL_PROCESS, REARMSIDE_PROCESS, REARMCLEAN_PROCESS)),
        (Some("nest=2"), (F2IFL_PROCESS, REARMSIDE_PROCESS, REARMCLEAN_PROCESS)),
        (Some("f2ifl=0"), (0, REARMSIDE_PROCESS, REARMCLEAN_PROCESS)),
        (Some("rearmside=0"), (F2IFL_PROCESS, 0, REARMCLEAN_PROCESS)),
        (Some("rearmclean=0"), (F2IFL_PROCESS, REARMSIDE_PROCESS, 0)),
        (Some("rearmside=3,rearmclean=0,f2ifl=1"), (1, 3, 0)),
        (Some("rearmside=999,f2ifl=9,rearmclean=9"), (1, 64, 1)),
    ] {
        let mut c = TrConfig::parse(None, None, None, env, None);
        c.apply_process_limits(env);
        assert_eq!((c.limits.f2ifl, c.limits.rearmside, c.limits.rearmclean), want, "limits env {env:?}");
    }
}

/// BUNDLE-Q: the process default of `t1q` (`T1Q_PROCESS`, a bit mask: 1 = `EQ` of objects, 2 = float `MOD`) is the struct default; a named value is clamped to 127 (BUNDLE-V: bits 16 / 32 / 64 = arithmetic metamethod frames / tail calls / general `RETURN` in metamethod frames; BUNDLE-S: bits 4 / 8 = Lua
/// `__index` / `__newindex` frames) and marked explicit (bit 0x80: it then also applies with `RUSTYNAIL_JIT_TRACE=off`).
#[test]
fn bq_process_default() {
    use rn_jit::tr::cache::{TrConfig, TrMode, T1Q_PROCESS};
    assert_eq!(TrConfig::for_mode(TrMode::On).limits.t1q, T1Q_PROCESS);
    for (env, want) in [(None, T1Q_PROCESS), (Some("nest=2"), T1Q_PROCESS), (Some("t1q=0"), 0x80), (Some("t1q=2"), 0x82), (Some("t1q=9"), 0x89), (Some("t1q=99"), 0xE3), (Some("t1q=125"), 0xFD), (Some("t1q=255"), 0xFF)] {
        let mut c = TrConfig::parse(None, None, None, env, None);
        c.apply_process_limits(env);
        assert_eq!(c.limits.t1q, want, "limits env {env:?}");
    }
}

/// BUNDLE-S / BUNDLE-V: the process defaults (`T1Q_PROCESS` = 125: EQ of objects + Lua `__index` / `__newindex` frames in tier 1 + arithmetic metamethod frames + tail calls
/// and general returns in metamethod frames; `AMT_PROCESS` = 1; `UPBAR_PROCESS` = 1) are the struct defaults; `amt` is clamped to 1.
#[test]
fn bs_process_defaults() {
    use rn_jit::tr::cache::{TrConfig, TrMode, AMT_PROCESS, T1Q_PROCESS};
    assert_eq!(T1Q_PROCESS, 125);
    assert_eq!(rn_jit::tr::cache::UPBAR_PROCESS, 1);
    assert_eq!(AMT_PROCESS, 1);
    assert_eq!(TrConfig::for_mode(TrMode::On).limits.amt, AMT_PROCESS);
    for (env, want) in [(None, AMT_PROCESS), (Some("nest=2"), AMT_PROCESS), (Some("amt=0"), 0), (Some("amt=1"), 1), (Some("amt=7"), 1)] {
        let mut c = TrConfig::parse(None, None, None, env, None);
        c.apply_process_limits(env);
        assert_eq!(c.limits.amt, want, "limits env {env:?}");
    }
}

/// BUNDLE-P P1 / C45: the rule of [`rearm_young_ok`]: a head with at least `REARM_MIN_ENTRIES` entries is always entered; a younger one only when `rearmside` > 0, the
/// family has that many installed side traces and (`rearmclean` = 1) the family is clean. Mutants `rearmside_never` (no young entry), `rearmside_any` (any young head once the
/// knob is on; the clean test still applies), `rearmclean_ignores_faults` / `rearmclean_ignores_la` (`TrState::family_clean`: the programs of `tr_core::bp_rearmside_programs`).
#[test]
fn bp_rearmside_rule() {
    use rn_jit::tr::cache::{rearm_young_ok as ok, REARM_MIN_ENTRIES as M};
    for clean in [false, true] {
        for rc in [0, 1] {
            assert!(ok(M, 0, clean, 0, rc), "an old head is entered whatever the knob and the family");
            assert!(ok(M + 5, 0, clean, 3, rc));
            assert!(!ok(M - 1, 9, clean, 0, rc), "knob off: the R1 rule");
            assert!(!ok(0, 2, clean, 3, rc), "two sides of three");
            assert!(!ok(M - 1, 0, clean, 1, rc), "no side trace at all");
        }
    }
    // clean family
    assert!(ok(0, 3, true, 3, 1), "three sides of three");
    assert!(ok(1, 7, true, 3, 1));
    assert!(ok(M - 1, 1, true, 1, 1));
    assert!(ok(0, 2, true, 2, 1), "the shipped default: two sides");
    // unclean family: only the plain rule (rearmclean=0) enters it
    assert!(!ok(0, 3, false, 3, 1), "a family with a failed side recording or an unrolled callee loop is not entered young");
    assert!(!ok(M - 1, 64, false, 2, 1));
    assert!(ok(0, 3, false, 3, 0), "rearmclean=0: the BUNDLE-P rule");
    assert!(ok(M - 1, 1, false, 1, 0));
}
