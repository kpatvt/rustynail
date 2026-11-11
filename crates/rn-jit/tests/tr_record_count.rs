//! J5-P3c (docs/JIT_TRACE.md TR5, 9.5): the recorder's count model checked against the REAL tier 1.
//!
//! `tr_record.rs::count_model_is_exact` recomputes `refund` / `land` from `Lowered`, the same algebra the recorder
//! uses, so it only checks `Lowered`'s documented contract. Here the compiled code itself runs under a small count
//! budget (`rn_jit::set_count_budget`, the machinery of `tr_identity_pre.rs`) and the observable facts of one stay are
//! compared with what the matching recorded snapshot predicts:
//!
//! * the number of instructions the stay executed (`Stats::insns`, a stay that is the only one of its state),
//! * the pc the interpreter continues at (the first fetch after the stay, `rn_core::oracle`),
//! * the budget word the stay leaves (`B - insns`): the trace exit of snapshot `s` stores `vb + refund`, tier 1's
//!   label of `pc` holds `B - e - land(pc)`; both are equated with the real `left`.
//!
//! The stay ends either by a failed block count (the exit lands on a block HEAD) or by a guard of a template that
//! exits at itself in the middle of a block (a MID label) or at a block head. Programs: call-free loops and a loop
//! with an inlined callee. Needs the feature `t15_oracle` (`--features trace,t15_oracle`).
#![cfg(all(feature = "trace", feature = "t15_oracle"))]

use rn_core::jit_hooks;
use rn_core::state::{LuaState, State, LUA_MULTRET};
use rn_core::status::{LuaResult, Status};
use rn_jit::lower::{LabelKind, Lowered};
use rn_jit::t15::runtime::{T15Config, T15Mode};
use rn_jit::tr::ir::Land;
use rn_jit::tr::record::{self, RecCfg, RecOutcome, Recorded};
use rn_jit::{Config, Mode};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::atomic::Ordering;
use std::sync::{Mutex, Once};

/// The oracle hook is process-global and `rt::ENTERS` counts the stays of every state: one test at a time.
static SERIAL: Mutex<()> = Mutex::new(());

/// What the recording run knows about the program: the block structure of every prototype the loop reaches.
#[derive(Clone)]
struct Known {
    /// `Lowered` per prototype address (the root and the Lua closures among its upvalues).
    lows: HashMap<usize, Rc<Lowered>>,
    /// The bytecode of the same prototypes.
    code: HashMap<usize, Vec<u32>>,
    root_len: usize,
}

thread_local! {
    /// `(pc, stays entered so far, code length of the prototype)` of every interpreter fetch of this thread.
    static LOG: RefCell<Vec<(usize, u64, usize)>> = const { RefCell::new(Vec::new()) };
    static ACTIVE: Cell<bool> = const { Cell::new(false) };
    static KNOWN: RefCell<Option<Known>> = const { RefCell::new(None) };
}

fn hook(
    _frame: usize,
    p: *const rn_core::func::Proto,
    pc: usize,
    _base: *mut rn_core::value::Value,
    _ms: usize,
) {
    if ACTIVE.get() {
        let e = rn_jit::rt::ENTERS.load(Ordering::Relaxed);
        // SAFETY: the interpreter passes the live prototype of the running frame.
        let len = unsafe { (*p).code().len() };
        LOG.with(|l| l.borrow_mut().push((pc, e, len)));
    }
}

fn init() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        // tier 1 enters a loop after two back-edges (the "low" configuration of tr_identity_pre.rs)
        assert!(rn_jit::register_with_trace_pinned_off(Config {
            mode: Mode::On,
            entry_calls: 2,
            loop_edges: 2,
        }));
        assert!(record::install());
        rn_core::oracle::set_hook(Some(hook));
    });
}

fn pc_index(st: &mut State, ci: rn_core::state::Ci, pc: *const u32) -> usize {
    let cl = st.ci_lclosure(ci).expect("Lua frame");
    (pc as usize - cl.proto().code_ptr() as usize) / 4
}

/// `arm()` while recording: record the loop that follows (in a state with the JIT off).
fn n_arm(st: &mut State) -> LuaResult<usize> {
    let caller = st.ci().previous().expect("caller");
    let cl = st.ci_lclosure(caller).unwrap();
    let p = cl.proto();
    let low = Rc::new(record::lower_for_landing(&p).expect("lowers"));
    let pc = pc_index(st, caller, caller.r().savedpc);
    let head = record::first_head_after(&low, pc as u32).expect("a loop follows arm()");
    let mut lows = HashMap::from([(&*p as *const _ as usize, low.clone())]);
    let mut code = HashMap::from([(&*p as *const _ as usize, p.code().to_vec())]);
    for u in 0..cl.nupvalues() {
        if let Some(c) = cl.upval(u).get().as_lclosure() {
            let q = c.proto();
            if let Some(l) = record::lower_for_landing(&q) {
                lows.insert(&*q as *const _ as usize, Rc::new(l));
                code.insert(&*q as *const _ as usize, q.code().to_vec());
            }
        }
    }
    KNOWN.with(|k| {
        *k.borrow_mut() = Some(Known {
            lows,
            code,
            root_len: p.code().len(),
        })
    });
    let cfg = RecCfg {
        verify: true,
        wait_head: true,
        landing: Some(low),
        ..RecCfg::default()
    };
    record::arm(st, caller, head, cfg).expect("recorder armed");
    Ok(0)
}

/// `arm()` in the compiled run: nothing (the bytecode, hence every pc, is the same as in the recording).
fn n_noop(_st: &mut State) -> LuaResult<usize> {
    Ok(0)
}

fn run(st: &mut LuaState, src: &str) -> (Status, String) {
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
    (status, s)
}

fn new_state(mode: Mode, arm: fn(&mut State) -> LuaResult<usize>) -> LuaState {
    init();
    let mut st = rn_std::new_state_with_libs().expect("state");
    rn_jit::set_mode(&mut st, mode);
    rn_jit::set_t15(&mut st, T15Config::for_mode(T15Mode::Off)); // tier 1 only
    let g = st.globals();
    st.set_funcs(g, &[("arm", Some(arm))]).unwrap();
    st
}

/// Records the loop that follows `arm()` of `src`.
fn record_src(src: &str) -> (Recorded, Known) {
    record::discard();
    jit_hooks::rec_set_ctx(jit_hooks::RecCtx::default());
    let mut st = new_state(Mode::Off, n_arm);
    let (status, r) = run(&mut st, src);
    assert_eq!(status, Status::Ok, "{r}");
    drop(st);
    let known = KNOWN.with(|k| k.borrow().clone().expect("arm() ran"));
    match record::take_outcome().expect("outcome").1 {
        RecOutcome::Closed(c) => (*c, known),
        RecOutcome::Aborted(a) => panic!("recording aborted: {a:?}\n{src}"),
    }
}

/// What one run of the compiled code showed.
#[derive(Debug)]
struct Obs {
    /// Stays entered.
    enters: u64,
    /// Instructions the stays executed (all of them).
    insns: u64,
    /// The first interpreter fetch after the first stay: `(pc, code length of its prototype)`.
    landing: Option<(usize, usize)>,
}

/// Runs `src` with tier 1 under the count budget `budget`.
fn observe(src: &str, budget: i32) -> Obs {
    let mut st = new_state(Mode::On, n_noop);
    rn_jit::set_count_budget(&mut st, budget);
    LOG.with(|l| l.borrow_mut().clear());
    ACTIVE.set(true);
    let (status, result) = run(&mut st, src);
    ACTIVE.set(false);
    assert_eq!(status, Status::Ok, "{result}");
    let stats = rn_jit::stats(&mut st);
    drop(st);
    let log = LOG.with(|l| std::mem::take(&mut *l.borrow_mut()));
    let first = log.windows(2).position(|w| w[1].1 != w[0].1);
    Obs {
        enters: stats.enters,
        insns: stats.insns,
        landing: first.map(|i| (log[i + 1].0, log[i + 1].2)),
    }
}

/// How the stay ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Exit {
    /// A failed block count (the exit lands on a block head).
    Budget,
    /// A template guard at the instruction landed on (`Head` / `Mid` label).
    Guard(LabelKind),
}

/// The step of the recorded path where the executed-instruction count `c` (within one iteration) is reached.
fn step_at(r: &Recorded, c: u32) -> Option<usize> {
    let mut sum = 0;
    for (k, cost) in r.cost.iter().enumerate() {
        if sum == c {
            return Some(k);
        }
        sum += cost;
    }
    None
}

/// Is the instruction at `step` one tier 1 counts by itself (`CALL`, `RETURN*`: kind `Guarded`)?
fn guarded(known: &Known, r: &Recorded, step: usize) -> bool {
    use rn_core::ver::active::OpCode as O;
    let w = known.code[&r.path_proto[step].0][r.path[step] as usize];
    matches!(
        rn_core::instruction::Instruction(w).opcode(),
        Some(O::Call | O::Return | O::Return0 | O::Return1 | O::Tailcall)
    )
}

/// What tier 1 does with budget `b` on the recorded path, from the block structure only: the instructions executed
/// and the landing step when a block count fails at a head.
fn predict_budget_exit(r: &Recorded, known: &Known, b: i64) -> (i64, usize) {
    let mut left = b;
    let mut e = 0i64;
    for _ in 0..10_000 {
        for step in 0..r.path.len() {
            let low = &known.lows[&r.path_proto[step].0];
            let pc = r.path[step];
            // a block head pays the count of its block; a `CALL` / `RETURN*` (kind `Guarded`) pays its own 1
            let cnt = if low.label_kind(pc) == LabelKind::Head && low.cnt_at(pc) > 0 {
                i64::from(low.cnt_at(pc))
            } else if guarded(known, r, step) {
                1
            } else {
                0
            };
            if cnt > 0 {
                if left < cnt {
                    return (e, step);
                }
                left -= cnt;
            }
            e += i64::from(r.cost[step]);
        }
    }
    panic!("the budget never runs out");
}

/// Compares one stay with the recorded snapshot of the instruction it ended at.
/// Returns whether a snapshot of the landing instruction existed (and was compared).
fn check_stay(r: &Recorded, known: &Known, budget: i64, obs: &Obs, exit: Exit, what: &str) -> bool {
    let n = i64::from(r.n);
    let e = obs.insns as i64;
    let (j, within) = (e / n, (e % n) as u32);
    let step = step_at(r, within)
        .unwrap_or_else(|| panic!("{what}: {e} executed instructions end inside a step"));
    // 1. the landing pc (and whether it is the root frame or the inlined callee) is the recorded path's
    let (pc, len) = obs.landing.expect("a stay ended");
    assert_eq!(
        pc as u32, r.path[step],
        "{what}: landing pc after {e} instructions"
    );
    assert_eq!(
        len == known.root_len,
        r.path_depth[step] == 0,
        "{what}: landing frame"
    );
    // 2. the label kind of the landing pc matches the way the stay ended
    let low = &known.lows[&r.path_proto[step].0];
    let kind = low.label_kind(pc as u32);
    match exit {
        Exit::Budget => assert!(
            kind == LabelKind::Head || (guarded(known, r, step) && low.rem(pc as u32) == 0),
            "{what}: a count failure lands on a head or on a CALL / RETURN"
        ),
        Exit::Guard(k) => assert_eq!(kind, k, "{what}: guard exit label kind"),
    }
    // 3. the snapshot of that instruction: S0 for a count failure at the loop head, else the instruction's own
    let s = if exit == Exit::Budget && step == 0 {
        Some(0)
    } else {
        (1..r.ir.snaps.len())
            .find(|&s| r.snap_step[s] as usize == step && Some(s as u32) != r.entry_snap)
    };
    let Some(s) = s else {
        // an instruction without a guard of its own (the CALL's identity guard uses the entry snapshot, TR6d): only
        // the executed count, the landing pc and the label kind were compared
        assert!(
            exit == Exit::Budget,
            "{what}: no snapshot at step {step} (pc {pc})"
        );
        return false;
    };
    let sn = r.ir.snaps[s];
    assert_eq!(sn.pc, r.path[step], "{what}");
    assert_eq!(
        sn.land,
        if kind == LabelKind::Head {
            Land::Pre
        } else {
            Land::Mid
        },
        "{what}: S{s} land"
    );
    // 4. the budget: the trace exit stores `vb + refund` where `vb` is the budget after the charge of `j + 1`
    // iterations (S0 is taken before the charge of the `j`-th); tier 1's label of `pc` holds `B - e - land(pc)`,
    // and the real stay left `B - e` (`Stats::insns` = `budget - left`).
    let charged = j + i64::from(s != 0);
    let stored = budget - charged * n + i64::from(sn.refund);
    let land = if kind == LabelKind::Head {
        0
    } else {
        i64::from(low.rem(pc as u32))
    };
    assert_eq!(
        stored + land,
        budget - e,
        "{what}: snapshot S{s} (pc {pc}, step {step}, refund {}) stores {stored}, the real stay left {} with land {land}",
        sn.refund,
        budget - e
    );
    true
}

fn lit(k: usize) -> String {
    (1..=k).map(|i| i.to_string()).collect::<Vec<_>>().join(",")
}

const K: usize = 24;

/// A program of this file (`@T@` = table constructor `{1..K}`, `@M@` = loop count, `@W@` = the extra operand `w`).
struct Prog {
    name: &'static str,
    text: &'static str,
}

impl Prog {
    fn src(&self, m: usize, w: &str) -> String {
        self.text
            .replace("@T@", &format!("{{{}}}", lit(K)))
            .replace("@M@", &m.to_string())
            .replace("@W@", w)
    }
}

fn progs() -> Vec<Prog> {
    vec![
        // one block per iteration: GETUPVAL GETTABLE ADD FORLOOP; the ADD is the first reader of `w` (its typecheck
        // belongs to the ADD) and in the middle of the block
        Prog {
            name: "one-block",
            text: "local t = @T@ local function f(n, w) local s = 0 arm() for i = 1, n do local a = t[i] s = s + w end return s end return f(@M@, @W@)",
        },
        // the ADD is the first instruction of its block (a head)
        Prog {
            name: "head-guard",
            text: "local function f(n, w) local s = 0 arm() for i = 1, n do s = s + w end return s end return f(@M@, @W@)",
        },
        // two blocks per iteration (`or`)
        Prog {
            name: "two-blocks",
            text: "local t = @T@ local function f(n, w) local s = 0 arm() for i = 1, n do s = s + (t[i] or 0) + w end return s end return f(@M@, @W@)",
        },
        // an inlined callee: its blocks and the return land in another prototype
        Prog {
            name: "inlined",
            text: "local t = @T@ local function g(i) return t[i] end local function f(n, w) local s = 0 arm() for i = 1, n do s = s + g(i) + w end return s end return f(@M@, @W@)",
        },
    ]
}

/// Is the landing of `obs` a pc of the recorded loop (not the code after it)?
fn in_loop(r: &Recorded, known: &Known, obs: &Obs) -> bool {
    obs.landing.is_some_and(|(pc, len)| {
        (0..r.path.len())
            .any(|k| r.path[k] as usize == pc && (len == known.root_len) == (r.path_depth[k] == 0))
    })
}

/// The first loop count for which the compiled code runs exactly one stay under `budget` and the stay ended inside the
/// loop (the interpreter finishes it: one more stay would need two more back-edges). `w` is the extra operand.
fn single_stay(p: &Prog, r: &Recorded, budget: i32, known: &Known, w: &str) -> Option<Obs> {
    (3..K)
        .map(|m| observe(&p.src(m, w), budget))
        .find(|o| o.enters == 1 && in_loop(r, known, o))
}

/// Tier 1 under small count budgets: every stay that ends by a failed block count (a HEAD landing) executed exactly
/// the instructions the block structure predicts, landed on the path's pc, and left the budget the matching snapshot
/// stores (call-free and with an inlined callee).
#[test]
fn tier1_count_failures_match_the_snapshots() {
    let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let mut checked = 0;
    let mut snaps = 0;
    for p in progs() {
        let (r, known) = record_src(&p.src(K, "7"));
        let n = r.n as i32;
        let mut interior = 0;
        let mut here = 0;
        for budget in 1..=3 * n + 2 {
            let Some(obs) = single_stay(&p, &r, budget, &known, "7") else {
                continue;
            };
            let what = format!("{} B={budget}", p.name);
            let (pe, pstep) = predict_budget_exit(&r, &known, i64::from(budget));
            if std::env::var("TR_DUMP").is_ok() {
                eprintln!(
                    "{what}: path={:?} depth={:?} cost={:?} real insns={} landing={:?} predicted e={pe} step={pstep}",
                    r.path, r.path_depth, r.cost, obs.insns, obs.landing
                );
                for k in 0..r.path.len() {
                    let low = &known.lows[&r.path_proto[k].0];
                    eprintln!(
                        "    step {k} pc {} {:?} cnt_at={} rem={}",
                        r.path[k],
                        low.label_kind(r.path[k]),
                        low.cnt_at(r.path[k]),
                        low.rem(r.path[k])
                    );
                }
            }
            assert_eq!(
                obs.insns as i64, pe,
                "{what}: instructions executed before the count failed"
            );
            assert_eq!(
                obs.landing.map(|l| l.0 as u32),
                Some(r.path[pstep]),
                "{what}: landing pc"
            );
            snaps += usize::from(check_stay(
                &r,
                &known,
                i64::from(budget),
                &obs,
                Exit::Budget,
                &what,
            ));
            interior += usize::from(pstep != 0);
            here += 1;
        }
        eprintln!(
            "{}: n={} stays checked {here}, interior head landings {interior}",
            p.name, r.n
        );
        if matches!(p.name, "two-blocks" | "inlined") {
            assert!(
                interior > 0,
                "{}: no count failure landed on an interior block head",
                p.name
            );
        }
        checked += here;
    }
    assert!(
        checked >= 12 && snaps >= 12,
        "{checked} stays, {snaps} with a snapshot"
    );
}

/// Tier 1 stays that end at a guard of a template: the `ADD` finds a string operand where the trace expects an
/// integer (the typecheck of the `SLOAD` of `w` belongs to the `ADD`, whose snapshot lands on the same pc as tier 1's
/// exit at the instruction): MID label (ADD after two instructions of its block) and HEAD label (ADD first in its block).
#[test]
fn tier1_guard_exits_match_the_snapshots() {
    let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let cases = [
        ("one-block", LabelKind::Mid),
        ("head-guard", LabelKind::Head),
    ];
    for (name, kind) in cases {
        let p = progs().into_iter().find(|p| p.name == name).unwrap();
        let (r, known) = record_src(&p.src(K, "7"));
        for budget in [1 << 20, 1 << 28] {
            // `w` is a string: the stay's first iteration exits at the ADD, the interpreter finishes the loop
            let obs = (3..K)
                .map(|m| observe(&p.src(m, "'7'"), budget))
                .find(|o| o.enters == 1 && in_loop(&r, &known, o))
                .unwrap_or_else(|| panic!("{name}: no single stay"));
            let what = format!("{name} (guard exit, B={budget})");
            // the stay ran the steps before the ADD of its first iteration and nothing else
            let (pc, _) = obs.landing.unwrap();
            let step = r
                .path
                .iter()
                .position(|&x| x as usize == pc)
                .unwrap_or_else(|| panic!("{what}: pc {pc} not on the path"));
            let before: i64 = r.cost[..step].iter().map(|&c| i64::from(c)).sum();
            assert_eq!(obs.insns as i64, before, "{what}: instructions executed");
            assert!(check_stay(
                &r,
                &known,
                i64::from(budget),
                &obs,
                Exit::Guard(kind),
                &what
            ));
        }
    }
}

/// The count-model mutants are caught by the real-tier-1 comparison (child processes).
#[cfg(feature = "tr_mutants")]
#[test]
fn mutants_are_caught() {
    if std::env::var("TR_COUNT_CHILD").is_ok() {
        return;
    }
    let exe = std::env::current_exe().unwrap();
    let child = |mutant: &str, test: &str| {
        let out = std::process::Command::new(&exe)
            .args(["--exact", test, "--test-threads=1"])
            .env("TR_COUNT_CHILD", "1")
            .env("RUSTYNAIL_JIT_MUT", mutant)
            .stdin(std::process::Stdio::null())
            .output()
            .unwrap();
        (
            out.status.success(),
            format!(
                "{}{}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            ),
        )
    };
    for mutant in ["refund_off_by_one", "land_pre_mid_swap"] {
        for test in [
            "tier1_count_failures_match_the_snapshots",
            "tier1_guard_exits_match_the_snapshots",
        ] {
            let (ok, _) = child("", test);
            assert!(ok, "baseline child of {test} failed");
            let (ok, text) = child(mutant, test);
            assert!(!ok, "mutant {mutant} was NOT caught by {test}\n{text}");
        }
    }
}
