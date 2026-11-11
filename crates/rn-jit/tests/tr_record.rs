//! J5-P3a (docs/JIT_TRACE.md 4, 9.1 item 5, 10.1): the recorder core on call-free programs.
//!
//! * golden IR dumps (`tests/tr_record_golden.txt`, regenerate with `TR_BLESS=1`) of the call-free programs;
//! * the abort matrix: every 4.6 reason reachable without calls;
//! * lockstep (TR17): the recorded path equals the sequence of instructions the VM executes, and a tampered slot makes
//!   the recorder diverge;
//! * the count model (TR5): `refund`, `land` and `counts` recomputed from `Lowered` independently of the recorder;
//! * IR replay: the recorded IR run with `tr::interp` against the real interpreter on the same frame for many random
//!   initial states (every exit and every complete iteration compared with the VM's state at that point, the snapshot
//!   restore model), plus every guard forced to fail;
//! * mutants (feature `tr_mutants`): each is selected in a child process, which must fail the matching test.
#![cfg(feature = "trace")]

use rn_core::jit_hooks::{self, Abort, RecCtx, RecStep, RecVTable};
use rn_core::state::{Ci, LuaState, State, StkId, LUA_MULTRET};
use rn_core::status::{LuaResult, Status};
use rn_core::table::TableRef;
use rn_core::value::Value;
use rn_jit::lower::{LabelKind, Lowered};
use rn_jit::tr::interp::gen::Rng;
use rn_jit::tr::interp::{self, MTable, Model, Outcome, Val};
use rn_jit::tr::ir::*;
use rn_jit::tr::print::dump;
use rn_jit::tr::record::{self, AbortReason, RecCfg, RecOutcome, Recorded, FUNC_SLOT};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Once;

// ---- harness: the dispatching recorder, natives, logging recorder -----------------------------------------------

/// What the dispatcher does with a recorder callback of this thread.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Record,
    Log,
}

/// One logged state of the real interpreter, before an instruction of the root frame.
#[derive(Clone, Debug, PartialEq)]
struct Step {
    pc: usize,
    slots: Vec<Val>,
    tabs: Vec<Vec<Val>>,
    /// F1: per table, its nodes with a short-string key (node index -> key address and value).
    nodes: Vec<std::collections::BTreeMap<u32, interp::MNode>>,
    /// F1: per table, `(metatable handle or 0, flags byte, lsizenode)`.
    meta: Vec<(u64, u8, u8)>,
    ups: Vec<Val>,
}

#[derive(Default)]
struct LogState {
    token: u64,
    head: usize,
    started: bool,
    limit: usize,
    steps: Vec<Step>,
    map: HashMap<usize, u64>,
    tabs: Vec<TableRef>,
}

#[derive(Default)]
struct Last {
    low: Option<Rc<Lowered>>,
    head: u32,
    nslots: usize,
    token: u64,
}

thread_local! {
    static MODE: Cell<Mode> = const { Cell::new(Mode::Record) };
    static CFG: RefCell<RecCfg> = RefCell::new(RecCfg::default());
    static LAST: RefCell<Last> = RefCell::new(Last::default());
    static LOG: RefCell<LogState> = RefCell::new(LogState::default());
    /// `(step number, slot, value)`: overwrite a slot right before the recorder's n-th step (behind its back).
    static TAMPER: Cell<Option<(usize, usize, Value)>> = const { Cell::new(None) };
    /// Install a hook right before the recorder's n-th step.
    static HOOK_AT: Cell<Option<usize>> = const { Cell::new(None) };
    /// Forget the VM's recording context right after the recorder's n-th step (a lost abort).
    static FORGET_AT: Cell<Option<usize>> = const { Cell::new(None) };
    static STEPNO: Cell<usize> = const { Cell::new(0) };
    static ABORTS: RefCell<Vec<Abort>> = const { RefCell::new(Vec::new()) };
    static PROBES: RefCell<Vec<Vec<i32>>> = const { RefCell::new(Vec::new()) };
    static STASH: Cell<RecCtx> = const { Cell::new(RecCtx { th: 0, root: 0, root_func: 0, root_cl: 0, cur: 0, enter: false, g: 0, token: 0 }) };
}

fn d_step(st: &mut State, ci: Ci, pc: *const u32, token: u64) -> RecStep {
    if MODE.get() == Mode::Log {
        return log_step(st, ci, pc, token);
    }
    // step numbers count from the loop head (the first step the recorder records)
    let idx = pc_index(st, ci, pc);
    if STEPNO.get() == usize::MAX {
        if idx != LAST.with(|l| l.borrow().head) as usize {
            return record::step(st, ci, pc, token);
        }
        STEPNO.set(0);
    }
    let n = STEPNO.get();
    STEPNO.set(n + 1);
    if let Some((at, slot, v)) = TAMPER.get() {
        if at == n {
            st.set(StkId(ci.r().func.0 + 1 + slot), v);
        }
    }
    if HOOK_AT.get() == Some(n) {
        st.set_hook(Some(|_, _, _| Ok(())), 1 << 2, 0);
    }
    let out = record::step(st, ci, pc, token);
    if FORGET_AT.get() == Some(n) {
        STASH.set(jit_hooks::rec_ctx());
        jit_hooks::rec_set_ctx(RecCtx::default());
    }
    out
}

fn d_abort(st: &mut State, why: Abort, token: u64) {
    ABORTS.with(|a| a.borrow_mut().push(why));
    if MODE.get() == Mode::Record {
        record::abort(st, why, token);
    }
}

fn pc_index(st: &mut State, ci: Ci, pc: *const u32) -> usize {
    let cl = st.ci_lclosure(ci).expect("Lua frame");
    (pc as usize - cl.proto().code_ptr() as usize) / 4
}

thread_local! {
    /// Short-string constants of the chunk of the last recording: content -> address in the recording's state. The
    /// recorded IR holds these addresses (HREFK keys, string constants), but the logged run is another `State`, whose
    /// interned strings live at other addresses (they only coincided when the allocator happened to hand out the same
    /// blocks twice, which fails under parallel test threads: Q4). The log translates every short-string address it
    /// reads into the recording's address of the same content.
    static CANON: RefCell<HashMap<Vec<u8>, u64>> = RefCell::new(HashMap::new());
}

/// Registers the short-string constants of `p` and its nested prototypes in [`CANON`].
fn collect_consts(p: &rn_core::func::Proto) {
    for k in p.k() {
        if let Some(s) = k.as_string() {
            if k.tag() == rn_core::value::ctb(rn_core::value::LUA_VSHRSTR) {
                let addr = k.as_gc().map_or(0, |g| g as usize as u64);
                CANON.with(|c| c.borrow_mut().insert(s.as_bytes().to_vec(), addr));
            }
        }
    }
    for q in p.protos() {
        collect_consts(q);
    }
}

/// The address a short string of the logged state has in the recorded state (itself when it is no constant).
fn canon_str(bytes: &[u8], addr: u64) -> u64 {
    CANON.with(|c| c.borrow().get(bytes).copied().unwrap_or(addr))
}

fn vval(reg: &mut LogState, v: &Value) -> Val {
    if v.tag() == rn_core::value::ctb(rn_core::value::LUA_VSHRSTR) {
        if let (Some(s), Some(g)) = (v.as_string(), v.as_gc()) {
            return Val {
                t: Ty::ShrStr,
                bits: canon_str(s.as_bytes(), g as usize as u64),
            };
        }
    }
    match record::ty_of_value(v) {
        None => Val {
            t: Ty::Udata,
            bits: 0xDEAD,
        },
        Some(Ty::Tab) => {
            let t = v.as_table().unwrap();
            let key = t.as_ptr() as usize;
            let h = *reg.map.entry(key).or_insert_with(|| {
                reg.tabs.push(t);
                reg.tabs.len() as u64
            });
            Val {
                t: Ty::Tab,
                bits: h,
            }
        }
        Some(t) => Val {
            t,
            bits: match t {
                Ty::Int => v.as_int().unwrap() as u64,
                Ty::Num => v.as_float().unwrap().to_bits(),
                Ty::Nil | Ty::False | Ty::True => 0,
                Ty::LUdata => v.as_light_userdata().map_or(0, |p| p as usize as u64),
                Ty::Lfn => v.as_light_cfn().map_or(0, |p| p as usize as u64),
                _ => v.as_gc().map_or(0, |p| p as usize as u64),
            },
        },
    }
}

fn log_step(st: &mut State, ci: Ci, pc: *const u32, token: u64) -> RecStep {
    let idx = pc_index(st, ci, pc);
    let cl = st.ci_lclosure(ci).unwrap();
    let p = cl.proto();
    let raw = rn_core::instruction::Instruction(p.code()[idx]).opcode_raw();
    use rn_core::ver::active::OpCode as O;
    let is_ret = [O::Return as u8, O::Return0 as u8, O::Return1 as u8].contains(&raw);
    let nslots = usize::from(p.maxstacksize);
    let base = ci.r().func.0 + 1;
    LOG.with(|l| {
        let mut l = l.borrow_mut();
        if l.token != token {
            return RecStep::Leave;
        }
        if !l.started {
            if idx != l.head {
                return RecStep::Keep;
            }
            l.started = true;
            // register everything reachable from the frame and the upvalues (breadth first)
            let slots: Vec<Value> = (0..nslots).map(|n| st.get(StkId(base + n))).collect();
            for v in &slots {
                vval(&mut l, v);
            }
            for u in 0..cl.nupvalues() {
                vval(&mut l, &cl.upval(u).get());
            }
            let mut i = 0;
            while i < l.tabs.len() {
                let t = l.tabs[i];
                for k in 1..=t.array_size() as i64 {
                    if let Some(e) = t.get_int_arr(k) {
                        vval(&mut l, &e);
                    }
                }
                if t.hash_size() > 0 {
                    for j in 0..t.hash_size() {
                        let (ktt, _) = t.node_key(j);
                        let v = t.node_value(j);
                        if ktt == rn_core::value::ctb(rn_core::value::LUA_VSHRSTR)
                            && v.tag() & 0x0F != 0
                        {
                            vval(&mut l, &v);
                        }
                    }
                }
                if let Some(mt) = t.metatable() {
                    vval(&mut l, &Value::table(mt));
                }
                i += 1;
            }
        }
        if l.steps.len() >= l.limit || is_ret {
            return RecStep::Leave;
        }
        let slots: Vec<Val> = (0..nslots)
            .map(|n| vval(&mut l, &st.get(StkId(base + n))))
            .collect();
        let ups: Vec<Val> = (0..cl.nupvalues())
            .map(|u| vval(&mut l, &cl.upval(u).get()))
            .collect();
        let tabs: Vec<Vec<Val>> = l
            .tabs
            .clone()
            .iter()
            .map(|t| {
                (1..=t.array_size() as i64)
                    .map(|k| match t.get_int_arr(k) {
                        Some(e) => vval(&mut l, &e),
                        None => Val::NIL,
                    })
                    .collect()
            })
            .collect();
        let mut nodes = Vec::new();
        let mut meta = Vec::new();
        for t in l.tabs.clone() {
            let mut m = std::collections::BTreeMap::new();
            if t.hash_size() > 0 {
                for j in 0..t.hash_size() {
                    let (ktt, mut kbits) = t.node_key(j);
                    let kptr = t.node_key_gc(j); // (a pointer with provenance: Miri rejects `kbits as *mut _`)
                    if ktt != rn_core::value::ctb(rn_core::value::LUA_VSHRSTR) {
                        continue;
                    }
                    // SAFETY: a live short-string key of a live table, rebuilt as a value to read its bytes
                    let kv = unsafe {
                        Value::from_gc(
                            rn_core::value::LUA_VSHRSTR,
                            kptr,
                        )
                    };
                    if let Some(ks) = kv.as_string() {
                        kbits = canon_str(ks.as_bytes(), kbits);
                    }
                    let nv = t.node_value(j);
                    let val = if nv.tag() & 0x0F == 0 {
                        Val::NIL
                    } else {
                        vval(&mut l, &nv)
                    };
                    m.insert(j as u32, interp::MNode { key: kbits, val, next: t.node_next(j), int: false });
                }
            }
            nodes.push(m);
            let mh = match t.metatable() {
                Some(mt) => vval(&mut l, &Value::table(mt)).bits,
                None => 0,
            };
            meta.push((mh, t.flags(), t.lsizenode()));
        }
        l.steps.push(Step {
            pc: idx,
            slots,
            tabs,
            nodes,
            meta,
            ups,
        });
        RecStep::Keep
    })
}

/// `arm()`: record the loop that follows. `armlog()`: log the real interpreter over that loop.
fn arm_common(st: &mut State, log: bool) -> LuaResult<usize> {
    let caller = st.ci().previous().expect("caller");
    let cl = st.ci_lclosure(caller).unwrap();
    let p = cl.proto();
    let low = Rc::new(record::lower_for_landing(&p).expect("lowers"));
    let pc = pc_index(st, caller, caller.r().savedpc);
    let head = record::first_head_after(&low, pc as u32).expect("a loop follows arm()");
    LAST.with(|l| {
        let mut l = l.borrow_mut();
        l.low = Some(low.clone());
        l.head = head;
        l.nslots = usize::from(p.maxstacksize);
    });
    STEPNO.set(usize::MAX);
    if log {
        let tok = jit_hooks::rec_arm(st, caller).expect("armed");
        LOG.with(|l| {
            let mut l = l.borrow_mut();
            let limit = l.limit;
            *l = LogState {
                token: tok,
                head: head as usize,
                limit,
                ..LogState::default()
            };
        });
    } else {
        let mut cfg = CFG.with(|c| c.borrow().clone());
        cfg.landing = Some(low);
        cfg.wait_head = true;
        let tok = record::arm(st, caller, head, cfg).expect("recorder armed");
        LAST.with(|l| l.borrow_mut().token = tok);
    }
    Ok(0)
}

fn n_arm(st: &mut State) -> LuaResult<usize> {
    arm_common(st, false)
}

fn n_armlog(st: &mut State) -> LuaResult<usize> {
    arm_common(st, true)
}

/// `probe()`: log the `trap` of every Lua frame of the chain.
fn n_probe(st: &mut State) -> LuaResult<usize> {
    let mut v = vec![];
    let mut c = st.ci().previous();
    while let Some(ci) = c {
        v.push(ci.r().trap);
        c = ci.previous();
    }
    PROBES.with(|t| t.borrow_mut().push(v));
    Ok(0)
}

/// `stale()`: restore the stashed (dead) recording context and flag the calling frame, as a stale context would.
fn n_stale(st: &mut State) -> LuaResult<usize> {
    let caller = st.ci().previous().expect("caller");
    jit_hooks::rec_set_ctx(STASH.get());
    caller.w().trap = 1;
    Ok(0)
}

fn init() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        assert!(jit_hooks::install_rec(RecVTable {
            step: d_step,
            abort: d_abort
        }));
    });
    #[cfg(feature = "tr_mutants")]
    rn_jit::tr::mutants::apply_vm_mutants();
}

fn reset(cfg: RecCfg) {
    init();
    record::discard();
    jit_hooks::rec_set_ctx(RecCtx::default());
    MODE.set(Mode::Record);
    CFG.with(|c| *c.borrow_mut() = cfg);
    TAMPER.set(None);
    HOOK_AT.set(None);
    FORGET_AT.set(None);
    STEPNO.set(usize::MAX);
    ABORTS.with(|a| a.borrow_mut().clear());
    PROBES.with(|a| a.borrow_mut().clear());
}

fn new_state() -> LuaState {
    init();
    // the libraries without io / os / package (they need libc, which Miri lacks), a fixed hash seed
    let mut st = rn_std::new_state(0x5eed).expect("state");
    for (name, open) in rn_std::STD_LIBS {
        if matches!(*name, b"package" | b"io" | b"os") {
            continue;
        }
        st.requiref(name, *open, true).expect("open library");
        st.pop();
    }
    let g = st.globals();
    st.set_funcs(
        g,
        &[
            ("arm", Some(n_arm)),
            ("armlog", Some(n_armlog)),
            ("probe", Some(n_probe)),
            ("stale", Some(n_stale)),
        ],
    )
    .unwrap();
    st
}

/// Runs `src` as a chunk under `pcall`; returns `"<status>:<result as string>"`.
fn run(st: &mut LuaState, src: &str) -> String {
    let base = st.top();
    assert_eq!(
        st.load_buffer(src.as_bytes(), b"=snippet", None),
        Status::Ok
    );
    let func = st.top() - 1;
    if MODE.get() == Mode::Record {
        if let Some(cl) = st.get(func).as_lclosure() {
            collect_consts(&cl.proto());
        }
    }
    let status = st.pcall(func, LUA_MULTRET, 0);
    let v = st.get(func);
    let s = v.as_string().map_or_else(
        || format!("{v:?}"),
        |s| String::from_utf8_lossy(s.as_bytes()).into_owned(),
    );
    st.set_top(base);
    format!("{status:?}:{s}")
}

/// A finished recording with what the test needs to check it.
struct Rec {
    out: RecOutcome,
    low: Rc<Lowered>,
    nslots: usize,
    result: String,
}

/// Records the loop that follows `arm()` in `src` (the state is dropped before returning, so a recording that is still
/// armed gets `Abort::Close`).
fn record_src(src: &str, cfg: RecCfg) -> Rec {
    reset(cfg);
    CANON.with(|c| c.borrow_mut().clear());
    let mut st = new_state();
    let result = run(&mut st, src);
    drop(st);
    let (low, nslots) = LAST.with(|l| {
        let l = l.borrow();
        (l.low.clone().expect("arm() ran"), l.nslots)
    });
    let (tok, out) = record::take_outcome().expect("the recording ended with an outcome");
    assert_eq!(tok, LAST.with(|l| l.borrow().token));
    assert!(!record::is_recording());
    Rec {
        out,
        low,
        nslots,
        result,
    }
}

fn closed(r: &Rec) -> &Recorded {
    match &r.out {
        RecOutcome::Closed(c) => c,
        RecOutcome::Aborted(a) => {
            panic!("expected a closed recording, aborted: {a:?} ({})", r.result)
        }
    }
}

/// The real interpreter's states over the loop that follows `armlog()` in `src` (at most `limit` steps).
fn log_src(src: &str, limit: usize) -> Vec<Step> {
    reset(RecCfg::default());
    MODE.set(Mode::Log);
    LOG.with(|l| l.borrow_mut().limit = limit);
    let mut st = new_state();
    let _ = run(&mut st, src);
    drop(st);
    MODE.set(Mode::Record);
    LOG.with(|l| std::mem::take(&mut l.borrow_mut().steps))
}

// ---- the programs ---------------------------------------------------------------------------------------------

fn lit_i(v: i64) -> String {
    if v == i64::MIN {
        "(-9223372036854775807-1)".to_string()
    } else if v < 0 {
        format!("({v})")
    } else {
        v.to_string()
    }
}

fn lit_f(f: f64) -> String {
    if f.is_nan() {
        "(0/0)".to_string()
    } else if f.is_infinite() {
        if f > 0.0 { "(1/0)" } else { "(-1/0)" }.to_string()
    } else if f == 0.0 && f.is_sign_negative() {
        "(-0.0)".to_string()
    } else if f < 0.0 {
        format!("({f:?})")
    } else {
        format!("{f:?}")
    }
}

const INTS: &[i64] = &[
    0,
    1,
    -1,
    2,
    3,
    5,
    7,
    -7,
    100,
    255,
    0xFFFF,
    i64::MAX,
    i64::MIN,
    i64::MIN + 1,
    1 << 53,
    (1 << 53) + 1,
];
const NUMS: &[f64] = &[
    0.0,
    -0.0,
    0.5,
    -0.5,
    1.0,
    -1.0,
    2.5,
    3.0,
    100.0,
    1e300,
    -1e300,
    f64::INFINITY,
    f64::NEG_INFINITY,
    f64::NAN,
    9007199254740992.0,
    0.1,
];

fn ri(r: &mut Rng) -> String {
    lit_i(r.pick(INTS))
}
fn rf(r: &mut Rng) -> String {
    lit_f(r.pick(NUMS))
}
/// A "mild" float: finite, small.
fn rm(r: &mut Rng) -> String {
    lit_f(r.below(2000) as f64 / 8.0 - 100.0)
}
fn rn(r: &mut Rng, lo: u64, hi: u64) -> String {
    (lo + r.below(hi - lo + 1)).to_string()
}
/// A table constructor; with `v == 1` one element has the other kind.
fn rtab(r: &mut Rng, n: usize, v: u8, int: bool) -> String {
    let at = r.below(n as u64) as usize;
    let e: Vec<String> = (0..n)
        .map(|i| {
            let int = int ^ (v == 1 && i == at);
            if int {
                (r.below(40) as i64 - 20).to_string()
            } else {
                rm(r)
            }
        })
        .collect();
    format!("{{{}}}", e.join(", "))
}
fn rtab_i(r: &mut Rng, n: usize, v: u8) -> String {
    rtab(r, n, v, true)
}
fn rtab_f(r: &mut Rng, n: usize, v: u8) -> String {
    rtab(r, n, v, false)
}
/// The first scalar argument: an int (or a float), of the other kind with `v == 1`.
fn first(r: &mut Rng, v: u8, int: bool) -> String {
    if int ^ (v == 1) {
        ri(r)
    } else {
        rf(r)
    }
}

/// Argument generator: `variant` 1 swaps the kind (int <-> float) of one value, so replays leave the recorded types.
type ArgGen = fn(&mut Rng, u8) -> String;

struct Prog {
    name: &'static str,
    /// Source with `ARM()` before the loop and `@` standing for the argument list.
    src: &'static str,
    args: ArgGen,
}

const PROGS: &[Prog] = &[
    Prog {
        name: "int_sum",
        src: "local function f(s, n) ARM() for i = 1, n do s = s + i * 3 - (i >> 1) end return s end return f(@)",
        args: |r, v| format!("{}, {}", first(r, v, true), rn(r, 3, 14)),
    },
    Prog {
        name: "float_poly",
        src: "local function f(x, acc, n) ARM() for i = 1, n do acc = acc * 0.999 + x * 1.5 - 0.25 x = x * 0.75 + 0.1 end return acc + x end return f(@)",
        args: |r, v| format!("{}, {}, {}", first(r, v, false), rf(r), rn(r, 3, 12)),
    },
    Prog {
        name: "array_rec",
        src: "local function f(t, n, c, x) ARM() for i = 2, n do t[i] = t[i - 1] * c + x end return t[n] end return f(@)",
        args: |r, v| format!("{}, 8, {}, {}", rtab_f(r, 8, v), rm(r), rm(r)),
    },
    Prog {
        name: "upval_acc",
        src: "local acc = 5 local function f(t, n) ARM() for i = 1, n do acc = acc + t[i] end end f(@) return acc",
        args: |r, v| format!("{}, 8", rtab_i(r, 8, v)),
    },
    Prog {
        name: "branches",
        src: "local function f(s, n) ARM() for i = 1, n do if i % 3 == 0 then s = s + i elseif i % 2 == 0 then s = s - 1 else s = s * 2 end end return s end return f(@)",
        args: |r, v| format!("{}, {}", first(r, v, true), rn(r, 3, 12)),
    },
    Prog {
        name: "while_loop",
        src: "local function f(s, n) local i = 0 ARM() while i < n do i = i + 1 s = s + i end return s end return f(@)",
        args: |r, v| format!("{}, {}", first(r, v, true), rn(r, 3, 12)),
    },
    Prog {
        name: "repeat_loop",
        src: "local function f(s, n) local i = 0 ARM() repeat i = i + 1 s = s ~ i until i >= n return s end return f(@)",
        args: |r, v| format!("{}, {}", first(r, v, true), rn(r, 3, 12)),
    },
    Prog {
        name: "int_rec2",
        src: "local function f(t, n) ARM() for i = 3, n do t[i] = (t[i - 1] + t[i - 2]) & 0xFFFFFF end return t[n] end return f(@)",
        args: |r, v| format!("{}, 8", rtab_i(r, 8, v)),
    },
    Prog {
        name: "mixed_arith",
        src: "local function f(s, n, d) ARM() for i = 1, n do s = s + i * 0.5 d = d + (i // 3) % 5 + i / 4 + 2 ^ i end return s + d end return f(@)",
        args: |r, v| format!("{}, {}, {}", first(r, v, false), rn(r, 3, 12), rf(r)),
    },
    Prog {
        name: "bitops",
        src: "local function f(s, n) ARM() for i = 1, n do s = ((s << 1) ~ (i >> 1)) & 0xFFFF | (~i & 0xF) end return s end return f(@)",
        args: |r, v| format!("{}, {}", first(r, v, true), rn(r, 3, 12)),
    },
    Prog {
        name: "float_for",
        src: "local function f(s, lim) ARM() for x = 0.0, lim, 0.5 do s = s + x end return s end return f(@)",
        args: |r, v| format!("{}, {}", first(r, v, false), lit_f(1.5 + r.below(12) as f64 / 2.0)),
    },
    Prog {
        name: "bools_testset",
        src: "local function f(s, n) ARM() for i = 1, n do local big = s > 100 local v = s or 0 if big then s = v - 100 else s = v + i end end return s end return f(@)",
        args: |r, v| format!("{}, {}", if v == 1 { lit_f(r.below(150) as f64) } else { rn(r, 0, 150) }, rn(r, 3, 12)),
    },
    Prog {
        name: "compare_mix",
        src: "local function f(s, n, lim) ARM() for i = 1, n do if i < lim then s = s + 1 end if s >= 2 then s = s - 1 elseif s == 0 then s = 5 end end return s end return f(@)",
        args: |r, v| format!("{}, {}, {}", if v == 1 { lit_f(r.below(4) as f64) } else { rn(r, 0, 4) }, rn(r, 3, 12), if r.chance(1, 2) { rn(r, 0, 8) } else { lit_f(r.below(16) as f64 / 2.0) }),
    },
    Prog {
        name: "down_step",
        src: "local function f(s, n) ARM() for i = n, 1, -2 do s = s + i end return s end return f(@)",
        args: |r, v| format!("{}, {}", first(r, v, true), rn(r, 4, 14)),
    },
    Prog {
        name: "seti_const",
        src: "local function f(t, n) ARM() for i = 1, n do t[1] = i t[3] = t[2] + i end return t[3] end return f(@)",
        args: |r, v| format!("{}, {}", rtab_i(r, 4, v), rn(r, 3, 10)),
    },
    Prog {
        name: "upval_rw",
        src: "local u = 3 local function f(n) ARM() for i = 1, n do u = u + i end end f(@) return u",
        args: |r, _| rn(r, 3, 12),
    },
    // The recorded iteration sees an Int upvalue, the next one a Num: the ULOAD typecheck must exit (review of P3a).
    Prog {
        name: "upval_typeshift",
        src: "local u = 1 local function f(n) ARM() for i = 1, n do local x = u u = x / 2 end end f(@) return u",
        args: |r, _| rn(r, 3, 12),
    },
    Prog {
        name: "str_const",
        src: "local function f(s, n) ARM() for i = 1, n do local z = 'tag' s = s + i end return s end return f(@)",
        args: |r, v| format!("{}, {}", first(r, v, true), rn(r, 3, 12)),
    },
    Prog {
        name: "float_key",
        src: "local function f(t, n, s) ARM() for i = 1, n do local k = i + 0.0 s = s + t[k] end return s end return f(@)",
        args: |r, v| format!("{}, 8, {}", rtab_i(r, 8, v), first(r, v, true)),
    },
    Prog {
        name: "unary_ops",
        src: "local function f(s, n) ARM() for i = 1, n do local x, y local a = -s local b = ~i x = i y = x + 1 s = (a + b + y) % 1000 - 3 end return s end return f(@)",
        args: |r, v| format!("{}, {}", first(r, v, true), rn(r, 3, 12)),
    },
    // ---- F1: the hash part ----
    Prog {
        name: "fields_rw",
        src: "local function f(o, n) ARM() for i = 1, n do o.x = o.x + i o.y = o.y * 0.5 + o.x o.n = o.n + 1 end return o.y end return f(@)",
        args: |r, v| format!("{{x = {}, y = {}, n = 0}}, {}", first(r, v, true), rm(r), rn(r, 3, 12)),
    },
    Prog {
        name: "fields_globals",
        src: "gA = 1 gB = 2.5 local function f(n) ARM() for i = 1, n do gA = gA + i gB = gB * 0.5 + gA end end f(@) return gA, gB",
        args: |r, _| rn(r, 3, 12),
    },
    Prog {
        name: "fields_nil",
        src: "local t = {x = 1, y = 2} t.y = nil local function f(n, s) ARM() for i = 1, n do s = s + (t.y or i) end return s end return f(@)",
        args: |r, v| format!("{}, {}", rn(r, 3, 12), first(r, v, true)),
    },
    Prog {
        name: "fields_strkey",
        src: "local o = {a = 1, b = 'x'} local k = 'a' local function f(n) ARM() for i = 1, n do o[k] = o[k] + i o.b = 'y' end end f(@) return o.a",
        args: |r, _| rn(r, 3, 12),
    },
    Prog {
        name: "fields_meta_flag",
        src: "local t = setmetatable({x = 1, y = 2}, {__tostring = tostring}) t.y = nil local _ = t.y local function f(n, s) ARM() for i = 1, n do s = s + (t.y or i) + t.x end return s end return f(@)",
        args: |r, v| format!("{}, {}", rn(r, 3, 12), first(r, v, true)),
    },
    Prog {
        name: "fields_nested",
        src: "local d = {a = {b = {c = 1}}} local function f(n) ARM() for i = 1, n do d.a.b.c = d.a.b.c + i end return d.a.b.c end return f(@)",
        args: |r, _| rn(r, 3, 12),
    },
    // ---- P-A: `__index` chains ----
    Prog {
        name: "index_chain",
        src: "local A = {k = 3} A.__index = A local B = setmetatable({}, {__index = A}) B.__index = B local o = setmetatable({x = 1}, B) local function f(n, s) ARM() for i = 1, n do s = s + o.k + o.x + (o.zz or i) end return s end return f(@)",
        args: |r, v| format!("{}, {}", rn(r, 3, 12), first(r, v, true)),
    },
    Prog {
        name: "index_nilnode",
        src: "local A = {k = 3} A.__index = A local o = setmetatable({k = 5, x = 2}, A) o.k = nil local function f(n, s) ARM() for i = 1, n do s = s + o.k + o.x end return s end return f(@)",
        args: |r, v| format!("{}, {}", rn(r, 3, 12), first(r, v, true)),
    },
    Prog {
        name: "nested_tabs",
        src: "local function f(m, n) ARM() for i = 1, n do local row = m[1] row[i] = row[i] + m[2][i] end return m[1][n] end return f(@)",
        args: |r, v| format!("{{{}, {}}}, 6", rtab_i(r, 6, v), rtab_i(r, 6, 0)),
    },
];

fn prog(name: &str) -> &'static Prog {
    PROGS.iter().find(|p| p.name == name).expect("program")
}

fn source(p: &Prog, args: &str, arm: &str) -> String {
    p.src.replace("ARM()", arm).replace('@', args)
}

fn rec_args(p: &Prog, seed: u64) -> String {
    (p.args)(&mut Rng::new(seed), 0)
}

fn record_prog(p: &Prog, seed: u64, cfg: RecCfg) -> Rec {
    record_src(&source(p, &rec_args(p, seed), "arm()"), cfg)
}

fn strict() -> RecCfg {
    RecCfg {
        verify: true,
        ..RecCfg::default()
    }
}

// ---- golden dumps ---------------------------------------------------------------------------------------------

fn normalise(s: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(i) = rest.find("0x") {
        out.push_str(&rest[..i]);
        let tail = &rest[i + 2..];
        let n = tail
            .find(|c: char| !c.is_ascii_hexdigit())
            .unwrap_or(tail.len());
        out.push_str("0xADDR");
        rest = &tail[n..];
    }
    out.push_str(rest);
    out
}

/// The golden text of a closed recording: the IR dump, the snapshot landing kinds, and the loop data.
fn golden_text(rc: &Rec) -> String {
    let r = closed(rc);
    let mut s = dump(&r.ir);
    for (i, sn) in r.ir.snaps.iter().enumerate() {
        s.push_str(&format!(
            "S{i} land={:?} step={}\n",
            sn.land, r.snap_step[i]
        ));
    }
    s.push_str(&format!(
        "counts={:?} extent={} depth={} head={} loop_at={:?} n={}\n",
        r.ir.counts, r.ir.extent, r.ir.depth, r.ir.head.1, r.ir.loop_at, r.n
    ));
    s.push_str(&format!("path={:?}\n", r.path));
    let wb: Vec<String> = r
        .writeback
        .iter()
        .map(|e| format!("{}=%{}", e.slot, e.r.wrapping_sub(REF_BIAS)))
        .collect();
    s.push_str(&format!("writeback=[{}]\n", wb.join(" ")));
    normalise(&s)
}

/// P-A: the guards of an `__index` chain lookup. Instance, subclass and class are tables: each level without the key proves
/// its absence (`FLOAD lsizenode; EQ`, `HMISS` steps ending in a link of 0), then `mt != 0`, `HREFK` of `__index` on the
/// metatable with a typechecked `HLOAD` of a table, and the next level reads the table that load returned; the end of the
/// chain (no metatable) takes `metatable == 0`; a key present with a nil value is a typechecked nil `HLOAD`.
#[test]
fn index_chain_records_its_guards() {
    let ir = closed(&record_prog(prog("index_chain"), 1, strict())).ir.clone();
    let hmiss: Vec<usize> = (0..ir.ins.len()).filter(|&x| ir.ins[x].op == TOp::Hmiss).collect();
    assert!(hmiss.len() >= 3, "too few HMISS steps\n{}", dump(&ir));
    for &x in &hmiss {
        let i = ir.ins[x];
        assert!(i.flags & F_GUARD != 0, "unguarded HMISS");
        let node = ir.at(i.a);
        assert!(node.op == TOp::Fload && node.b == Field::Node as Ref, "HMISS of a non-node");
        assert!(
            ir.ins[..x].iter().any(|g| g.op == TOp::Eq
                && TraceIr::is_ins(g.a)
                && ir.at(g.a).op == TOp::Fload
                && ir.at(g.a).b == Field::Lsizenode as Ref
                && ir.at(g.a).a == node.a
                && TraceIr::is_const(g.b)),
            "HMISS without the lsizenode guard\n{}",
            dump(&ir)
        );
    }
    // the last step of a walk has no link (`delta == 0`)
    assert!(hmiss.iter().any(|&x| hmiss_unpack(aux_bits(ir.ins[x].aux)).1 == 0), "no chain end\n{}", dump(&ir));
    // `__index` of a metatable: non-null guard, then a typechecked HLOAD of a table through an HREFK
    let tab_loads: Vec<usize> = (0..ir.ins.len())
        .filter(|&x| ir.ins[x].op == TOp::Hload && ir.ins[x].t == Ty::Tab)
        .collect();
    assert!(tab_loads.len() >= 2, "no `__index` loads\n{}", dump(&ir));
    for &x in &tab_loads {
        let i = ir.ins[x];
        assert!(i.flags & F_GUARD != 0 && aux_bits(i.aux) & 1 != 0, "unchecked `__index` load\n{}", dump(&ir));
        let hr = ir.at(i.a);
        assert_eq!(hr.op, TOp::Hrefk);
        let node = ir.at(hr.a);
        let mt = ir.at(node.a);
        assert!(mt.op == TOp::Fload && mt.b == Field::Metatable as Ref, "`__index` HREFK not on a metatable\n{}", dump(&ir));
        assert!(
            ir.ins[..x].iter().any(|g| g.op == TOp::Ne && g.a == node.a && ir.as_int(g.b) == Some(0)),
            "`__index` load without `mt != 0`\n{}",
            dump(&ir)
        );
    }
    // the end of the chain: the last table has no metatable
    let last_hmiss = *hmiss.last().unwrap();
    assert!(
        ir.ins[last_hmiss..].iter().any(|g| g.op == TOp::Eq
            && TraceIr::is_ins(g.a)
            && ir.at(g.a).op == TOp::Fload
            && ir.at(g.a).b == Field::Metatable as Ref
            && ir.as_int(g.b) == Some(0)),
        "chain end without `metatable == 0`\n{}",
        dump(&ir)
    );
    // a key that is present with a nil value: a typechecked nil load, then the chain
    let ir = closed(&record_prog(prog("index_nilnode"), 1, strict())).ir.clone();
    let nil = ir.ins.iter().find(|i| i.op == TOp::Hload && i.t == Ty::Nil).expect("a nil HLOAD");
    assert!(nil.flags & F_GUARD != 0 && aux_bits(nil.aux) & 1 != 0, "unchecked nil load\n{}", dump(&ir));
}

/// The golden dumps of the 5.5 build; the 5.4 build has its own (`_54`): the `FORLOOP` layout and the `ATOUCH` after
/// every array bounds guard (TR30) change the IR.
#[cfg(not(rustynail_lua54))]
const GOLDEN_FILE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/tr_record_golden.txt");
#[cfg(rustynail_lua54)]
const GOLDEN_FILE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/tr_record_golden_54.txt");

fn parse_golden(text: &str) -> HashMap<String, String> {
    let mut m = HashMap::new();
    let mut cur: Option<(String, String)> = None;
    for line in text.lines() {
        if let Some(n) = line.strip_prefix("=== ") {
            if let Some((k, v)) = cur.take() {
                m.insert(k, v);
            }
            cur = Some((n.to_string(), String::new()));
        } else if let Some((_, v)) = cur.as_mut() {
            v.push_str(line);
            v.push('\n');
        }
    }
    if let Some((k, v)) = cur {
        m.insert(k, v);
    }
    m
}

#[test]
fn golden_ir_dumps() {
    let mut actual: Vec<(String, String)> = Vec::new();
    for p in PROGS {
        let r = record_prog(p, 1, strict());
        actual.push((p.name.to_string(), golden_text(&r)));
    }
    if std::env::var("TR_BLESS").is_ok() {
        let mut out = String::new();
        for (n, t) in &actual {
            out.push_str(&format!("=== {n}\n{t}"));
        }
        std::fs::write(GOLDEN_FILE, out).unwrap();
        return;
    }
    let want = parse_golden(&std::fs::read_to_string(GOLDEN_FILE).expect("golden file"));
    for (n, t) in &actual {
        assert_eq!(
            want.get(n).map(String::as_str),
            Some(t.as_str()),
            "golden dump of {n} differs (TR_BLESS=1 regenerates)\n{t}"
        );
    }
    assert_eq!(want.len(), actual.len(), "stale golden entries");
}

/// BUNDLE-M C1: the lockstep (`RecCfg::lockstep`) only cross-checks: a recording without it is the same IR, snapshots, path and
/// outcome. The corpus of the golden dumps plus allocation programs (`TNEW`, `NEWREF` with the inline-insertion plan that reads the
/// tracked hash image, `TSET` growth). Mutants `nr_img_record_skip`, `light_tnew_unregistered`.
#[test]
fn m_lockstep_off_records_the_same_ir() {
    // (the inline `NEWREF` plan, which reads the tracked hash image, is a production setting: `nrinline`)
    let on = |c: RecCfg| RecCfg { nrinline: true, ..c };
    let off = |c: RecCfg| RecCfg { lockstep: false, nrinline: true, ..c };
    let text = |r: &Rec| match &r.out {
        RecOutcome::Closed(_) => golden_text(r),
        RecOutcome::Aborted(a) => format!("aborted {a:?}"),
    };
    for p in PROGS {
        let a = record_prog(p, 1, on(strict()));
        let b = record_prog(p, 1, off(strict()));
        assert_eq!(text(&a), text(&b), "{}: lockstep off records differently", p.name);
    }
    let alloc: &[&str] = &[
        "local function f(n) local s = 0 arm() for i = 1, n do local t = {} t.x = i t.y = i + 1 s = s + t.x + t.y end return s end return f(5)",
        "local function f(n) local s = 0 arm() for i = 1, n do local t = {x = 1} t.y = i t.z = i s = s + t.x + t.y + t.z end return s end return f(5)",
        "local function f(n) local s = 0 arm() for i = 1, n do local t = {} t[1] = i t[2] = i t[3] = i s = s + #t end return s end return f(5)",
        "local t = {} local function f(n) arm() for i = 1, n do t[i] = i end return #t end return f(20)",
        "local function f(n) local s = 0 arm() for i = 1, n do local t = table.pack(i, i) s = s + t.n end return s end return f(5)",
        "local function f(n) local s = 0 local u = {} arm() for i = 1, n do local t = {a = 1} t.b = 2 u.k = t s = s + t.a + t.b end return s end return f(5)",
        // (a hash part with free nodes: every insertion after the first is an inline `NEWREF` whose plan needs the tracked hash image)
        "local function f(n) local s = 0 local u = {} arm() for i = 1, n do local t = {a = 1, b = 2, c = 3} t.d = i u.k = t s = s + t.a + t.b + t.c end return s end return f(5)",
    ];
    let mut closed_n = 0;
    for (k, src) in alloc.iter().enumerate() {
        let a = record_src(src, on(strict()));
        let b = record_src(src, off(strict()));
        assert_eq!(text(&a), text(&b), "allocation program {k}: lockstep off records differently");
        closed_n += usize::from(matches!(a.out, RecOutcome::Closed(_)));
    }
    assert!(closed_n >= 4, "the allocation programs must record ({closed_n} did)");
}

/// BUNDLE-M C1: `Charge` (the element budget of the model) is bookkeeping the lockstep used to drive for tables made or resized inside the
/// trace; with the lockstep off the recorder still charges them. Mutant `light_charge_skip`.
#[test]
fn m_charge_with_lockstep_off() {
    // (an append into a full array part of 32: the touch charges 32, the rehash the 64 of the new array part: only the resync after the
    // `TSET` sees it, which the lockstep used to run)
    let grow = "local t = {} for i = 1, 32 do t[i] = i end local function f(n) arm() for i = 1, n do t[#t + 1] = i end return #t end return f(4)";
    let fresh = "local function f(n) local s = 0 arm() for i = 1, n do local t = {1, 2, 3, 4, 5, 6, 7, 8} s = s + #t end return s end return f(5)";
    for src in [grow, fresh] {
        for cap in [3usize, 6, 12, 40, 50, 70, 120] {
            let mk = |ls: bool| RecCfg { lockstep: ls, max_model_elems: cap, ..RecCfg::default() };
            let a = record_src(src, mk(true));
            let b = record_src(src, mk(false));
            let kind = |r: &Rec| match &r.out {
                RecOutcome::Closed(_) => "closed".to_string(),
                RecOutcome::Aborted(x) => format!("{x:?}"),
            };
            assert_eq!(kind(&a), kind(&b), "cap {cap}: {src}");
        }
    }
    // (the caps above straddle the sizes: the growth must charge in the resync, i.e. abort at 50 but not at 120)
    let at = |cap: usize| RecCfg { lockstep: false, max_model_elems: cap, ..RecCfg::default() };
    assert_eq!(aborted_cfg(grow, at(50)), AbortReason::Charge, "the resync after the growing store does not charge");
    assert!(matches!(record_src(grow, at(120)).out, RecOutcome::Closed(_)));
    assert_eq!(aborted_cfg(fresh, at(6)), AbortReason::Charge);
}

// ---- IR replay against the real interpreter -----------------------------------------------------------------------

thread_local! {
    /// The IR under replay contains a `CALLN` (its NaNs are canonicalised by `interp`, the VM's are libm's): the only
    /// values whose NaN payload is not compared (the lockstep check of the recorder tracks it per value).
    static NAN_LENIENT: Cell<bool> = const { Cell::new(false) };
}

fn val_eq(a: Val, b: Val) -> bool {
    // collectables other than tables are compared by type only: the recording and the logged run are different
    // states, so their string addresses differ (the lockstep check compares addresses within one state)
    let by_type = !matches!(
        a.t,
        Ty::Int | Ty::Num | Ty::Nil | Ty::False | Ty::True | Ty::Tab
    );
    a.t == b.t
        && (by_type
            || a.bits == b.bits
            || (NAN_LENIENT.get()
                && a.t == Ty::Num
                && f64::from_bits(a.bits).is_nan()
                && f64::from_bits(b.bits).is_nan()))
}

fn vec_eq(a: &[Val], b: &[Val]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(x, y)| val_eq(*x, *y))
}

/// The model the IR runs on, from the real interpreter's state at the loop head.
fn model_from(s: &Step) -> Model {
    Model {
        slots: s.slots.clone(),
        tables: s
            .tabs
            .iter()
            .enumerate()
            .map(|(h, a)| MTable {
                array: a.clone(),
                metatable: s.meta[h].0,
                flags: s.meta[h].1,
                lsizenode: s.meta[h].2,
                lenhint: 0,
                nodes: s.nodes[h].clone(),
            })
            .collect(),
        closures: vec![s.ups.clone()],
        ..Model::new()
    }
}

/// `SLOAD FUNC_SLOT` becomes the model closure (handle 1).
fn rewrite_func(ir: &TraceIr) -> TraceIr {
    let mut ir = ir.clone();
    let kcl = ir.konst(KVal::Ptr(1, PtrKind::LClosure));
    for i in &mut ir.ins {
        if i.op == TOp::Sload && i.a == FUNC_SLOT {
            i.op = TOp::Kptr;
            i.a = kcl;
            i.flags = 0;
            i.aux = 0;
        }
    }
    ir
}

/// An always-failing guard.
fn failing(aux: u32) -> Ins {
    Ins {
        op: TOp::Eq,
        t: Ty::I64,
        flags: F_GUARD,
        a: 0,
        b: 0,
        prev: NOREF,
        aux,
    }
}

/// `ir` followed by a probe: a snapshot of `entries` and an always-failing guard (the complete iteration's writeback
/// values come out as the exit's resolved slots). Returns the IR and the probe snapshot's index.
fn with_probe(ir: &TraceIr, entries: &[SnapEntry]) -> (TraceIr, u32) {
    let mut ir = ir.clone();
    let s = ir.snapshot(Snap::default(), entries);
    let (k0, k1) = (ir.kint(0), ir.kint(1));
    ir.ins.push(Ins {
        a: k0,
        b: k1,
        ..failing(guard_aux(s, 0))
    });
    (ir, s)
}

fn force_fail(ir: &TraceIr, g: usize) -> TraceIr {
    let mut ir = ir.clone();
    let (k0, k1) = (ir.kint(0), ir.kint(1));
    ir.ins[g] = Ins {
        a: k0,
        b: k1,
        ..failing(ir.ins[g].aux)
    };
    ir
}

#[derive(Default, Debug)]
struct Stats {
    /// Complete iterations replayed.
    done: usize,
    /// Natural exits of the IR (the real run left the recorded path or the types).
    exits: usize,
    /// Forced exits compared.
    forced: usize,
    /// Natural exits per snapshot index.
    exit_snaps: HashMap<u32, usize>,
}

/// The state after an exit: the stack at the start of the iteration overlaid with the snapshot's slots, plus the
/// heap the model holds.
fn check_state(
    what: &str,
    base: &[Val],
    exit: &[(u16, u8, Val)],
    m: &Model,
    real: &Step,
) -> Result<(), String> {
    let mut slots = base.to_vec();
    for &(s, _, v) in exit {
        slots[usize::from(s)] = v;
    }
    for (i, (a, b)) in slots.iter().zip(&real.slots).enumerate() {
        if !val_eq(*a, *b) {
            return Err(format!("{what}: slot {i}: restored {a:?}, VM {b:?}"));
        }
    }
    if m.tables.len() != real.tabs.len()
        || m.tables
            .iter()
            .zip(&real.tabs)
            .any(|(a, b)| !vec_eq(&a.array, b))
    {
        return Err(format!("{what}: tables differ"));
    }
    if !vec_eq(&m.closures[0], &real.ups) {
        return Err(format!("{what}: upvalues differ"));
    }
    // F1: every hash node (key and value)
    for (h, (t, rn)) in m.tables.iter().zip(&real.nodes).enumerate() {
        if t.nodes.len() != rn.len()
            || t.nodes.iter().zip(rn).any(|((i, a), (j, b))| {
                i != j || a.key != b.key || !val_eq(a.val, b.val)
            })
        {
            return Err(format!(
                "{what}: hash nodes of table {} differ: model {:?}, VM {:?}",
                h + 1,
                t.nodes,
                rn
            ));
        }
    }
    Ok(())
}

/// Replays the recorded IR of `rc` on the real interpreter's log `log` (states from the loop head on). Natural exits
/// and complete iterations, then every guard forced to fail in the first two iterations. Err = a mismatch.
fn replay(rc: &Rec, log: &[Step], st: &mut Stats) -> Result<(), String> {
    let r = closed(rc);
    let low = &rc.low;
    let len = r.path.len();
    if log.is_empty() || log[0].pc != r.ir.head.1 as usize {
        return Ok(());
    }
    // the instruction counts the recorder claims, recomputed from the tier-1 block structure
    let fetched = |step: usize| {
        r.path[..step]
            .iter()
            .filter(|&&p| low.label_kind(p) != LabelKind::Dead)
            .count()
    };
    if r.n as usize != fetched(len) || r.ir.counts != vec![r.n] {
        return Err("counts differ from the fetches of the path".into());
    }
    let ir = rewrite_func(&r.ir);
    NAN_LENIENT.set(r.ir.ins.iter().any(|i| i.op == TOp::Calln));
    let (ir_wb, probe) = with_probe(&ir, &r.writeback);
    let mut m = model_from(&log[0]);
    let b0 = m.budget;
    let step_of_snap = |s: u32| r.snap_step[s as usize] as usize;
    // budget a snapshot exit must store: tier 1's budget at the landing label (TR5.4)
    let budget_check =
        |what: &str, mk: &Model, s: u32, e: usize, charged: bool| -> Result<(), String> {
            let sn = r.ir.snaps[s as usize];
            let want_land = match low.label_kind(sn.pc) {
                LabelKind::Head => Land::Pre,
                _ => Land::Mid,
            };
            if sn.land != want_land {
                return Err(format!(
                    "{what}: snapshot {s} land {:?}, tier 1 label {want_land:?}",
                    sn.land
                ));
            }
            let stored = mk.budget + i64::from(sn.refund);
            let want = b0 - e as i64 - i64::from(low.land(sn.pc));
            let _ = charged;
            if stored != want {
                return Err(format!(
                    "{what}: snapshot {s} (pc {}) stores budget {stored}, tier 1 holds {want}",
                    sn.pc
                ));
            }
            Ok(())
        };
    for k in 0..3usize {
        let start = k * len;
        if start >= log.len() {
            break;
        }
        // the state the iteration starts from is the VM's
        check_state("iteration start", &m.slots.clone(), &[], &m, &log[start])?;
        // first real index (>= start) whose pc leaves the recorded path
        let follow = (start..log.len())
            .find(|&j| log[j].pc != r.path[j % len] as usize)
            .unwrap_or(log.len());
        // 1. every guard forced to fail (first two iterations): the exit state is the VM's state before its instruction
        if k < 2 {
            for (g, ins) in ir_wb.ins.iter().enumerate() {
                if ins.flags & F_GUARD == 0 || aux_snap(ins.aux) as usize >= r.ir.snaps.len() {
                    continue; // not a guard of the trace (the probe's guard is the last instruction)
                }
                let sg = aux_snap(ins.aux);
                let mut mf = m.clone();
                let out =
                    interp::run(&force_fail(&ir_wb, g), &mut mf).map_err(|e| format!("{e:?}"))?;
                let Outcome::Exit { snap, slots } = out else {
                    continue;
                };
                if snap != sg {
                    continue; // an earlier guard fails naturally on this state
                }
                let e = start + step_of_snap(sg);
                if e >= log.len() || follow <= e {
                    continue;
                }
                st.forced += 1;
                let what = format!("forced exit of guard {g} (snapshot {sg}) in iteration {k}");
                check_state(&what, &m.slots, &slots, &mf, &log[e])?;
                if log[e].pc != r.ir.snaps[sg as usize].pc as usize {
                    return Err(format!(
                        "{what}: snapshot pc {} but the VM is at pc {}",
                        r.ir.snaps[sg as usize].pc, log[e].pc
                    ));
                }
                budget_check(
                    &what,
                    &mf,
                    sg,
                    e,
                    ins.op != TOp::Poll && ins.op != TOp::Budget,
                )?;
            }
        }
        // 2. the natural run
        let mut mk = m.clone();
        let out = interp::run(&ir_wb, &mut mk).map_err(|e| format!("{e:?}"))?;
        match out {
            Outcome::Exit { snap, slots } if snap != probe => {
                let e = start + step_of_snap(snap);
                let what = format!("natural exit at snapshot {snap} in iteration {k}");
                if e >= log.len() {
                    return Ok(()); // the log ends before the exit
                }
                if follow <= e {
                    return Err(format!("{what}: the IR exited at VM step {e}, after the VM left the path at step {follow}"));
                }
                check_state(&what, &m.slots, &slots, &mk, &log[e])?;
                if log[e].pc != r.ir.snaps[snap as usize].pc as usize {
                    return Err(format!(
                        "{what}: snapshot pc {} but the VM is at pc {}",
                        r.ir.snaps[snap as usize].pc, log[e].pc
                    ));
                }
                budget_check(&what, &mk, snap, e, true)?;
                st.exits += 1;
                *st.exit_snaps.entry(snap).or_default() += 1;
                return Ok(());
            }
            Outcome::Exit { slots, .. } => {
                // a complete iteration: the VM must have followed the path all along
                let end = start + len;
                if follow < end.min(log.len()) {
                    return Err(format!("iteration {k} completed in the IR, but the VM left the recorded path at step {follow}"));
                }
                for (s, _, v) in slots {
                    mk.slots[usize::from(s)] = v;
                }
                if end < log.len() {
                    check_state(
                        &format!("end of iteration {k}"),
                        &mk.slots,
                        &[],
                        &mk,
                        &log[end],
                    )?;
                }
                m = mk;
                st.done += 1;
            }
            Outcome::Done { .. } => return Err("no probe exit".into()),
        }
    }
    Ok(())
}

/// Records `p` with `rec_seed` and replays it against `nlogs` real runs.
fn replay_prog(p: &Prog, rec_seed: u64, nlogs: u64, variants: &[u8], st: &mut Stats) {
    let rc = record_prog(p, rec_seed, strict());
    let RecOutcome::Closed(r) = &rc.out else {
        panic!(
            "{}: recording aborted: {:?} ({})",
            p.name, rc.out, rc.result
        );
    };
    for variant in variants {
        for seed in 0..nlogs {
            let args = (p.args)(
                &mut Rng::new(1000 + seed * 7 + u64::from(*variant)),
                *variant,
            );
            let log = log_src(&source(p, &args, "armlog()"), 4 * r.path.len() + 8);
            if let Err(e) = replay(&rc, &log, st) {
                panic!(
                    "{} (recorded with seed {rec_seed}, replay args `{args}`, variant {variant}): {e}\n{}",
                    p.name,
                    golden_text(&rc)
                );
            }
        }
    }
}

#[test]
fn ir_replay_matches_the_interpreter() {
    let nlogs = if cfg!(miri) { 2 } else { 14 };
    for p in PROGS {
        let mut st = Stats::default();
        for rec_seed in 1..=3 {
            // exits are counted per snapshot of ONE recording (snapshot numbers are not comparable across them)
            let mut one = Stats::default();
            replay_prog(p, rec_seed, nlogs, &[0, 1], &mut one);
            assert_eq!(
                one.exit_snaps.values().sum::<usize>(),
                one.exits,
                "{}: every natural exit is counted for its snapshot",
                p.name
            );
            st.done += one.done;
            st.exits += one.exits;
            st.forced += one.forced;
            *st.exit_snaps.entry(rec_seed as u32).or_default() += one.exit_snaps.len();
        }
        eprintln!("{}: {st:?}", p.name);
        assert!(
            st.done > 0 && st.forced > 0,
            "{}: nothing replayed: {st:?}",
            p.name
        );
        // the variant-1 runs leave the recorded types: the typecheck guards must have fired somewhere
        assert!(
            st.exits > 0 && st.exit_snaps.values().all(|&n| n > 0),
            "{}: no natural exit was replayed in some recording: {st:?}",
            p.name
        );
    }
}

/// The replay of the *recorded* iteration: the VM runs the recording's own arguments, so every guard of the first
/// iteration holds and the IR must complete it. A guard that fires here is over-strict (a wrong polarity, a stale
/// type, a bound that is too tight); `replay` accepts such an exit as a legal (if useless) early exit, so this is
/// where it is noticed. Self-test: with any single guard forced to fail the check must notice.
fn completes_recorded_iteration(r: &Recorded, log: &[Step]) -> Result<(), String> {
    let ir = rewrite_func(&r.ir);
    NAN_LENIENT.set(r.ir.ins.iter().any(|i| i.op == TOp::Calln));
    let (ir_wb, probe) = with_probe(&ir, &r.writeback);
    let mut m = model_from(&log[0]);
    match interp::run(&ir_wb, &mut m).map_err(|e| format!("{e:?}"))? {
        Outcome::Exit { snap, .. } if snap == probe => Ok(()),
        other => Err(format!("a guard fired on the recorded inputs: {other:?}")),
    }
}

#[test]
fn over_strict_guards_are_noticed() {
    for p in PROGS {
        let rc = record_prog(p, 1, strict());
        let r = closed(&rc);
        // the VM runs the recording's own arguments (the loop state at H is the recorded one)
        let log = log_src(
            &source(p, &rec_args(p, 1), "armlog()"),
            2 * r.path.len() + 4,
        );
        assert_eq!(
            completes_recorded_iteration(r, &log),
            Ok(()),
            "{}: the recorder's guards must hold on the iteration it recorded",
            p.name
        );
        // self-test: every guard in turn made over-strict (always failing) is noticed
        let ir = rewrite_func(&r.ir);
        let (ir_wb, probe) = with_probe(&ir, &r.writeback);
        let mut noticed = 0;
        for (g, ins) in ir_wb.ins.iter().enumerate() {
            if ins.flags & F_GUARD == 0 || aux_snap(ins.aux) as usize >= r.ir.snaps.len() {
                continue;
            }
            let mut m = model_from(&log[0]);
            let out = interp::run(&force_fail(&ir_wb, g), &mut m).unwrap();
            assert!(
                !matches!(out, Outcome::Exit { snap, .. } if snap == probe),
                "{}: an over-strict guard %{g} went unnoticed",
                p.name
            );
            noticed += 1;
        }
        assert!(noticed >= 2, "{}", p.name);
    }
}

// ---- structure of the recorded IR ---------------------------------------------------------------------------------

/// Every program: the invariants of 5.1 / 5.4 / 7.8 that do not need a VM run.
#[test]
fn recorded_ir_is_well_formed() {
    for p in PROGS {
        let rc = record_prog(p, 1, strict());
        let r = closed(&rc);
        let ir = &r.ir;
        // rolled layout (7.8): no LOOP, no PHI; POLL and BUDGET first, both on snapshot 0 (uncharged)
        assert_eq!(ir.loop_at, None, "{}", p.name);
        assert!(ir.ins.iter().all(|i| i.op != TOp::Loop && i.op != TOp::Phi));
        assert_eq!(
            (ir.ins[0].op, ir.ins[1].op),
            (TOp::Poll, TOp::Budget),
            "{}",
            p.name
        );
        assert_eq!((aux_snap(ir.ins[0].aux), aux_snap(ir.ins[1].aux)), (0, 0));
        assert_eq!(u32::from(ir.ins[1].a), r.n);
        assert_eq!(ir.snaps[0].pc, ir.head.1);
        assert!(ir.snaps[0].map.1 == 0 && ir.snaps[0].refund == -(rc.low.land(ir.head.1) as i32));
        // operands are defined before use (a guard folded away leaves no hole); guards carry a valid snapshot
        for (n, i) in ir.ins.iter().enumerate() {
            let me = REF_BIAS + n as u16;
            let (ra, rb) = i.op.ref_operands();
            for (is_ref, o) in [(ra, i.a), (rb, i.b)] {
                if is_ref && o != NOREF {
                    assert!(o < me, "{}: %{n} uses a later ref", p.name);
                }
            }
            if i.flags & F_GUARD != 0 {
                assert!((aux_snap(i.aux) as usize) < ir.snaps.len());
            }
        }
        // TR3a: a snapshot's pc is the instruction it belongs to, in path order; snapshots are numbered in path order
        assert_eq!(r.snap_step.len(), ir.snaps.len());
        for (s, st) in r.snap_step.iter().enumerate() {
            assert_eq!(ir.snaps[s].pc, r.path[*st as usize], "{}: S{s}", p.name);
        }
        assert!(r.snap_step.windows(2).all(|w| w[0] <= w[1]));
        // the extent covers the frame; the dependency is the root proto
        assert_eq!(usize::from(ir.extent), rc.nslots);
        assert_eq!(ir.deps, vec![ir.head.0]);
        // TR22b: nothing in the call-free scope reads `top`
        assert!(ir.frames.is_empty() && ir.snaps.iter().all(|s| s.nframes == 0 && s.frame == 0));
        // the writeback list is sparse, sorted, and a subset of the slots some snapshot or the path wrote
        assert!(r.writeback.windows(2).all(|w| w[0].slot < w[1].slot));
        // no dead-slot pruning (TR3): every written slot is restored by the last snapshot's successor state
        let last = ir.snaps.last().unwrap();
        for e in ir.snap_entries((ir.snaps.len() - 1) as u32) {
            assert!(
                r.writeback.iter().any(|w| w.slot == e.slot),
                "{}: slot {} of {last:?} not written back",
                p.name,
                e.slot
            );
        }
    }
}

/// TR5 recomputed with the public count-model API, without the recorder's code: `n`, `refund`, `land` of every snapshot.
#[test]
fn count_model_is_exact() {
    for p in PROGS {
        let rc = record_prog(p, 1, strict());
        let r = closed(&rc);
        let low = &rc.low;
        let n = low.fetches(&r.path);
        assert_eq!(r.n, n, "{}", p.name);
        assert_eq!(
            r.path.len() as u32,
            n,
            "{}: every recorded step is a fetch",
            p.name
        );
        assert!(
            r.cost.iter().all(|&c| c == 1),
            "{}: no unfetched chain in the call-free scope",
            p.name
        );
        assert_eq!(r.cost.iter().sum::<u32>(), n);
        for (s, sn) in r.ir.snaps.iter().enumerate() {
            let step = r.snap_step[s] as usize;
            let kind = low.label_kind(sn.pc);
            assert_ne!(kind, LabelKind::Dead);
            let (land, amt) = match kind {
                LabelKind::Head => (Land::Pre, 0),
                _ => (Land::Mid, low.rem(sn.pc)),
            };
            assert_eq!(sn.land, land, "{}: S{s}", p.name);
            // snapshot 0 is taken before the charge, every other one after it
            let charged = if s == 0 { 0 } else { n };
            let fetched = low.fetches(&r.path[..if s == 0 { 0 } else { step }]);
            assert_eq!(
                sn.refund,
                charged as i32 - fetched as i32 - amt as i32,
                "{}: S{s} pc {}",
                p.name,
                sn.pc
            );
            // tier 1 would hold at least 0 after the exit: never refund more than was charged
            assert!(sn.refund <= n as i32);
        }
    }
}

// ---- lockstep (TR17) --------------------------------------------------------------------------------------------

/// The recorded path equals the sequence of instructions the VM executes (the logging recorder sees the same
/// `rec_step` callbacks the oracle counts, `tr_hook.rs`), and the VM is at the head again after one iteration.
#[test]
fn recorded_path_equals_the_vm_sequence() {
    for p in PROGS {
        for seed in 1..=3 {
            let rc = record_prog(p, seed, strict());
            let r = closed(&rc);
            let args = rec_args(p, seed);
            let log = log_src(&source(p, &args, "armlog()"), r.path.len() + 2);
            assert!(log.len() > r.path.len(), "{}: log too short", p.name);
            let pcs: Vec<u32> = log.iter().map(|s| s.pc as u32).collect();
            assert_eq!(&pcs[..r.path.len()], &r.path[..], "{} seed {seed}", p.name);
            assert_eq!(
                pcs[r.path.len()],
                r.ir.head.1,
                "{}: the loop closes at its head",
                p.name
            );
            // recording changed nothing observable
            let plain = {
                reset(RecCfg::default());
                let mut st = new_state();
                run(&mut st, &source(p, &args, ""))
            };
            assert_eq!(rc.result, plain, "{}", p.name);
        }
    }
}

/// A slot modified behind the recorder's back (what a finalizer's `debug.setlocal` would do in the full build) makes
/// the lockstep check abort the recording with `Diverge`; without the check a trace would be recorded for stale values.
#[test]
fn tampered_slot_diverges() {
    let p = prog("int_sum");
    for at in 2..=5usize {
        reset(RecCfg::default());
        // slot 4 is the loop variable `i`: tracked from the first recorded step on
        TAMPER.set(Some((at, 4, Value::int(12345))));
        let mut st = new_state();
        let _ = run(&mut st, &source(p, &rec_args(p, 1), "arm()"));
        drop(st);
        let (_, out) = record::take_outcome().unwrap();
        assert!(
            matches!(out, RecOutcome::Aborted(AbortReason::Diverge)),
            "tamper before step {at}: {out:?}"
        );
    }
}

// ---- the abort matrix (4.6) ---------------------------------------------------------------------------------------

fn aborted(src: &str) -> AbortReason {
    aborted_cfg(src, RecCfg::default())
}

fn aborted_cfg(src: &str, cfg: RecCfg) -> AbortReason {
    match record_src(src, cfg).out {
        RecOutcome::Aborted(a) => a,
        RecOutcome::Closed(_) => panic!("expected an abort: {src}"),
    }
}

use rn_core::ver::active::OpCode;

#[test]
fn abort_matrix() {
    let nyi = |op: OpCode| AbortReason::Nyi(op as u8);
    let cases: Vec<(&str, String, AbortReason)> = vec![
        ("Metamethod (a function-valued __index; an absent key is recorded since P-A)", "local o = setmetatable({}, {__index = function(t, k) return 1 end}) local function f(n) local s = 0 ARM() for i = 1, n do s = s + o.zz end return s end return f(5)".into(), AbortReason::Metamethod),
        ("Nyi (CLOSURE; a CALL of a local is recorded since P3b, see tr_record_calls.rs)", "local function f(n) local s = 0 ARM() for i = 1, n do local h = function() return i end s = s + 1 end return s end return f(5)".into(), nyi(OpCode::Closure)),
        ("Nyi (generic for: its chained TFORCALL / TFORLOOP are never recorded)", "local nx, t = next, {1, 2, 3} local function f(n) local s = 0 ARM() for i = 1, n do for k, v in nx, t do s = s + v end end return s end return f(3)".into(), nyi(OpCode::Tforprep)),
        ("Nyi (CONCAT of four operands)", "local function f(n) local s = 0 ARM() for i = 1, n do local z = i .. 'x' .. i .. 'y' s = s + i end return s end return f(5)".into(), nyi(OpCode::Concat)),
        ("Nyi (string compare)", "local function f(n) local s = 0 local a, b = 'x', 'y' ARM() for i = 1, n do if a < b then s = s + i end end return s end return f(5)".into(), nyi(OpCode::Lt)),
        ("Nyi (collectable SETUPVAL)", "local u local function f(n) ARM() for i = 1, n do u = 'x' end end f(5) return 1".into(), nyi(OpCode::Setupval)),
        ("Metamethod (__add)", "local mt = {__add = function(a, b) return 1 end} local function f(n) local s = setmetatable({}, mt) ARM() for i = 1, n do s = s + i end return 1 end return f(5)".into(), AbortReason::Metamethod),
        ("Metamethod (error predicted: n // 0)", "local function f(n, z) local s = 0 ARM() for i = 1, n do s = s + i // z end return s end return pcall(f, 5, 0)".into(), AbortReason::Metamethod),
        ("Metamethod (error predicted: nil arithmetic)", "local function f(n, z) local s = 0 ARM() for i = 1, n do s = s + z end return s end return pcall(f, 5, nil)".into(), AbortReason::Metamethod),
        ("Metamethod (bitwise of a non-integral float)", "local function f(n, z) local s = 0 ARM() for i = 1, n do s = s | z end return s end return pcall(f, 5, 1.5)".into(), AbortReason::Metamethod),
        ("Coerce (string + number)", "local function f(n) local s = 0 ARM() for i = 1, n do s = s + '1' end return s end return f(5)".into(), AbortReason::Coerce),
        ("MetaTable", "local t = setmetatable({1, 2, 3, 4, 5}, {}) local function f(n) local s = 0 ARM() for i = 1, n do s = s + t[i] end return s end return f(5)".into(), AbortReason::MetaTable),
        ("Metamethod (an `__index` chain of 6 tables: deeper than the recorder follows; an absent field is recorded since P-A)", "local t = {} for i = 1, 5 do t = setmetatable({}, {__index = t}) end local function f(n) local s = 0 ARM() for i = 1, n do s = s + (t.y or 1) end return s end return f(5)".into(), AbortReason::Metamethod),
        ("Insert (store to a field whose value is nil)", "local t = {x = 1} t.x = nil local function f(n) ARM() for i = 1, n do t.x = i end end f(5) return 1".into(), AbortReason::Insert),
        ("Metamethod (nil field of a table whose metatable has __index)", "local t = setmetatable({x = 1}, {__index = function() return 1 end}) t.x = nil local function f(n) local s = 0 ARM() for i = 1, n do s = s + (t.x or 1) end return s end return f(5)".into(), AbortReason::Metamethod),
        ("Metamethod (a field of a number)", "local function f(n) local s, z = 0, 3 ARM() for i = 1, n do s = s + z.x end return s end return pcall(f, 5)".into(), AbortReason::Metamethod),
        ("TableHash (index beyond the array part)", "local t = {1, 2, 3} local function f(n) local s = 0 ARM() for i = 1, n do s = s + (t[i + 10] or 0) end return s end return f(5)".into(), AbortReason::TableHash),
        ("TableHash (non-integral float key)", "local t = {1, 2, 3} local function f(n) local s = 0 ARM() for i = 1, n do s = s + (t[1.5] or 0) end return s end return f(5)".into(), AbortReason::TableHash),
        ("Metamethod (indexing a non-table)", "local function f(n) local s, z = 0, 3 ARM() for i = 1, n do s = s + z[i] end return s end return pcall(f, 5)".into(), AbortReason::Metamethod),
        ("TableNil", "local t = {1, nil, 3} local function f(n) local s = 0 ARM() for i = 1, n do s = s + (t[2] or 0) end return s end return f(5)".into(), AbortReason::TableNil),
        ("Insert", "local t = {nil, nil, nil} local function f(n) ARM() for i = 1, n do t[2] = nil end end f(5) return 1".into(), AbortReason::Insert),
        ("Vararg", "local function f(...) local s = 0 ARM() for i = 1, 5 do s = s + (...) end return s end return f(1)".into(), AbortReason::Vararg),
        ("TailCall", "local function g(x) return x end local function f(n) ARM() for i = 1, n do return g(i) end end return f(5)".into(), AbortReason::TailCall),
        ("LeaveFrame", "local function f(n) ARM() for i = 1, n do if i == 1 then return i end end end return f(5)".into(), AbortReason::LeaveFrame),
        ("LeaveLoop (the last iteration)", "local function f(n) local s = 0 ARM() for i = 1, n do s = s + i end return s end return f(1)".into(), AbortReason::LeaveLoop),
        ("LeaveLoop (break into the enclosing loop)", "local function f(n) local s = 0 for k = 1, 2 do if k == 1 then ARM() end for i = 1, n do if i == 1 then break end s = s + i end end return s end return f(5)".into(), AbortReason::LeaveLoop),
        ("InnerLoop", "local function f(n) local s = 0 ARM() for i = 1, n do local j = 0 while j < 3 do j = j + 1 s = s + j end end return s end return f(5)".into(), AbortReason::InnerLoop),
        ("TooLong", format!("local function f(n, s) ARM() for i = 1, n do {} end return s end return f(5, 0)", "s = s + i ".repeat(4100)), AbortReason::TooLong),
        ("Unstable (int accumulator becomes a float)", "local function f(n) local a = 0 ARM() for i = 1, n do a = a + 0.5 end return a end return f(5)".into(), AbortReason::Unstable),
        ("Unstable (a flag flips)", "local function f(n) local a = true ARM() for i = 1, n do a = not a end return a end return f(5)".into(), AbortReason::Unstable),
    ];
    // the case lists use one `nyi` for the ones the opcode names
    for (name, src, want) in cases {
        let got = aborted(&src.replace("ARM()", "arm()"));
        // the LeaveLoop-break case reaches the RETURN after the loop first: LeaveFrame by construction
        assert_eq!(got, want, "{name}: {src}");
    }
}

/// F1: the guards the recorder puts around the hash ops (TR13.4, TR14): the node-vector size before every `HREFK`; the
/// metatable guard before a `nil` read (`metatable == 0`, or non-null with the flag bit set); the `TBAR` after an
/// `HSTORE` of a collectable value; an `EQREF` for a string key that does not come from a constant.
#[test]
fn hash_ops_record_their_guards() {
    let ops = |name: &str| -> TraceIr {
        let rc = record_prog(prog(name), 1, strict());
        closed(&rc).ir.clone()
    };
    for name in ["fields_rw", "fields_globals", "fields_nil", "fields_meta_flag", "fields_strkey", "fields_nested"] {
        let ir = ops(name);
        let n = |op: TOp| ir.ins.iter().filter(|i| i.op == op).count();
        let dynamic = name == "fields_strkey"; // N6: a key that is not a proto constant is an `HREFV`
        if dynamic {
            assert!(n(TOp::Hrefv) > 0 && n(TOp::Eqref) == 0, "{name}: no HREFV\n{}", dump(&ir));
        } else {
            assert!(n(TOp::Hrefk) > 0, "{name}: no HREFK\n{}", dump(&ir));
        }
        // every HREFK names a node vector (`FLOAD node`) of a table whose `FLOAD lsizenode` is compared with a constant
        // by a guard before it
        for (x, i) in ir.ins.iter().enumerate().filter(|(_, i)| matches!(i.op, TOp::Hrefk | TOp::Hrefv)) {
            assert!(i.flags & F_GUARD != 0, "{name}: unguarded HREFK");
            if i.op == TOp::Hrefv {
                assert!(!TraceIr::is_const(i.b), "{name}: HREFV of a constant key");
            }
            let node = ir.at(i.a);
            assert!(
                node.op == TOp::Fload && node.b == Field::Node as Ref,
                "{name}: HREFK of a non-node"
            );
            let has_size_guard = ir.ins[..x].iter().any(|g| {
                g.op == TOp::Eq
                    && TraceIr::is_ins(g.a)
                    && ir.at(g.a).op == TOp::Fload
                    && ir.at(g.a).b == Field::Lsizenode as Ref
                    && ir.at(g.a).a == node.a
                    && TraceIr::is_const(g.b)
            });
            assert!(has_size_guard, "{name}: HREFK without the lsizenode guard\n{}", dump(&ir));
        }
        // every HLOAD / HSTORE is on an HREFK; loads are typechecked
        for i in ir.ins.iter().filter(|i| matches!(i.op, TOp::Hload | TOp::Hstore)) {
            assert!(matches!(ir.at(i.a).op, TOp::Hrefk | TOp::Hrefv), "{name}");
            assert!(i.flags & F_GUARD != 0 && aux_bits(i.aux) & 1 != 0, "{name}: unchecked {:?}", i.op);
        }
    }
    // a nil read of a table without a metatable: `metatable == 0`
    let ir = ops("fields_nil");
    let nil_load = ir.ins.iter().position(|i| i.op == TOp::Hload && i.t == Ty::Nil).expect("a nil HLOAD");
    assert!(
        ir.ins[..nil_load].iter().any(|g| g.op == TOp::Eq
            && TraceIr::is_ins(g.a)
            && ir.at(g.a).op == TOp::Fload
            && ir.at(g.a).b == Field::Metatable as Ref
            && ir.as_int(g.b) == Some(0)),
        "nil read without the metatable guard\n{}",
        dump(&ir)
    );
    // ... with a metatable: non-null and the `__index` bit of its flags
    let ir = ops("fields_meta_flag");
    let nil_load = ir.ins.iter().position(|i| i.op == TOp::Hload && i.t == Ty::Nil).expect("a nil HLOAD");
    let flag_guard = ir.ins[..nil_load].iter().any(|g| {
        g.op == TOp::Ne
            && TraceIr::is_ins(g.a)
            && ir.at(g.a).op == TOp::Band
            && ir.as_int(ir.at(g.a).b) == Some(1)
            && TraceIr::is_ins(ir.at(g.a).a)
            && ir.at(ir.at(g.a).a).op == TOp::Fload
            && ir.at(ir.at(g.a).a).b == Field::Flags as Ref
    });
    assert!(flag_guard, "nil read with a metatable but without the flag guard\n{}", dump(&ir));
    // a collectable value stored into a field: HSTORE then TBAR of the same table and value
    let ir = ops("fields_strkey");
    let st = ir.ins.iter().enumerate().filter(|(_, i)| i.op == TOp::Hstore && collectable_ty(ir.ty_of(i.b))).count();
    assert!(st > 0, "no HSTORE of a string\n{}", dump(&ir));
    for (x, i) in ir.ins.iter().enumerate().filter(|(_, i)| i.op == TOp::Hstore && collectable_ty(ir.ty_of(i.b))) {
        let next = &ir.ins[x + 1];
        assert!(next.op == TOp::Tbar && next.b == i.b, "HSTORE of a collectable value without TBAR\n{}", dump(&ir));
    }
    // a string key that is not a constant of the proto (`local k = 'a'` outside the loop): EQREF on the key (the F1 path, N6 off)
    let ir = closed(&record_prog(prog("fields_strkey"), 1, RecCfg { hrefv: false, ..strict() })).ir.clone();
    assert!(
        ir.ins.iter().any(|i| i.op == TOp::Eqref && TraceIr::is_const(i.b)
            && matches!(ir.kval(i.b), KVal::Ptr(_, PtrKind::Raw))),
        "no EQREF on a string key\n{}",
        dump(&ir)
    );
}

fn collectable_ty(t: Ty) -> bool {
    matches!(
        t,
        Ty::ShrStr | Ty::LngStr | Ty::Tab | Ty::Lcf | Ty::Ccl | Ty::Udata | Ty::Thread
    )
}

/// The lockstep model is sparse (P3c): a loop over a table with more than 2^20 array elements records (the model
/// array is a zero-initialised allocation of the table's size, only the elements the IR reads or stores are copied),
/// including a store into it. Sizes stay under ~100 MB: the VM's own table is the only real allocation.
#[test]
fn large_array_loops_record() {
    for (name, src) in [
        ("read, 1e6 elements", "local t = {} for i = 1, 1000000 do t[i] = i end local function f(n) local s = 0 ARM() for i = 1, n do s = s + t[i] end return s end return f(3)"),
        ("read, 1.5e6 elements (array part 2^21)", "local t = {} for i = 1, 1500000 do t[i] = i end local function f(n) local s = 0 ARM() for i = 1, n do s = s + t[i] end return s end return f(3)"),
        ("store, 1.5e6 elements", "local t = {} for i = 1, 1500000 do t[i] = i end local function f(n) ARM() for i = 1, n do t[i] = t[i] + 1 end return t[1] end return f(3)"),
    ] {
        let t0 = std::time::Instant::now();
        let rc = record_src(&src.replace("ARM()", "arm()"), strict());
        let r = closed(&rc);
        assert!(r.ir.ins.iter().any(|i| i.op == TOp::Aload), "{name}");
        // the cost of a step no longer depends on the table: the whole recording is a few milliseconds of recording
        // work; the bound only guards against a return of the per-step full-model clone (seconds)
        assert!(t0.elapsed().as_secs() < 30, "{name}: {:?}", t0.elapsed());
    }
}

/// `Charge` measures the model arrays the recording has to address (`RecCfg::max_model_elems`): a table larger than
/// the cap aborts, one that fits records. (The production cap is 2^26 elements, a 1 GiB table: tested with a small
/// cap instead of allocating it.)
#[test]
fn charge_at_the_model_limit() {
    let src = "local t = {} for i = 1, 2000 do t[i] = i end local function f(n) local s = 0 arm() for i = 1, n do s = s + t[i] end return s end return f(3)";
    let at = |cap| RecCfg {
        max_model_elems: cap,
        ..RecCfg::default()
    };
    // the array part of 2000 elements has 2048 slots
    assert_eq!(aborted_cfg(src, at(2047)), AbortReason::Charge);
    assert!(matches!(
        record_src(src, at(2048)).out,
        RecOutcome::Closed(_)
    ));
    // two tables: the total counts
    let two = "local a, b = {} , {} for i = 1, 1000 do a[i] = i b[i] = i end local function f(n) local s = 0 arm() for i = 1, n do s = s + a[i] + b[i] end return s end return f(3)";
    assert_eq!(aborted_cfg(two, at(1500)), AbortReason::Charge);
    assert!(matches!(
        record_src(two, at(2048)).out,
        RecOutcome::Closed(_)
    ));
}

#[test]
fn error_close_and_hook_aborts() {
    // an instruction raises while the recorder waits for the head: the VM's choke point aborts it (Abort::Error)
    let r =
        aborted("local function f() arm() error('boom') for i = 1, 3 do end end return pcall(f)");
    assert_eq!(r, AbortReason::Error);
    // the state closes while the recording is armed (the root chunk ends before the loop is reached)
    let r = aborted("local n = 0 arm() if n > 5 then for i = 1, 3 do end end return 1");
    assert_eq!(r, AbortReason::Close);
    // a hook installed during the recording
    reset(RecCfg::default());
    HOOK_AT.set(Some(4));
    let mut st = new_state();
    let _ = run(
        &mut st,
        &source(prog("int_sum"), &rec_args(prog("int_sum"), 1), "arm()"),
    );
    drop(st);
    let (_, out) = record::take_outcome().unwrap();
    assert!(
        matches!(out, RecOutcome::Aborted(AbortReason::Hook)),
        "{out:?}"
    );
}

/// A stale recording context (the abort was lost: the VM forgot the recording, the recorder still holds its state)
/// restored for a new activation of the same function at the same stack position: the VM's validation passes (same
/// root `CallInfo`, function slot and closure), the recorder's own check of the expected successor pc rejects it
/// (TR23.5). Mutant `rec_no_validate` disables that check.
#[test]
fn stale_context_is_rejected_by_the_recorder_validation() {
    reset(RecCfg::default());
    FORGET_AT.set(Some(3));
    let mut st = new_state();
    let src = "local function f(k)
                 local s = 0
                 if k == 2 then stale() end
                 if k == 1 then arm() end
                 for i = 1, 6 do s = s + i * 2 - 1 + i * 3 - 2 end
                 return s
               end
               f(1)
               local r = f(2)
               return r";
    let _ = run(&mut st, src);
    drop(st);
    let (_, out) = record::take_outcome().expect("an outcome");
    assert!(
        matches!(out, RecOutcome::Aborted(AbortReason::Frame)),
        "the stale callback must be rejected as a Frame abort: {out:?}"
    );
    assert_eq!(jit_hooks::rec_ctx(), RecCtx::default());
    // rejected by the recorder's own validation, not by the VM's (which passes: same node, slot and closure)
    assert_eq!(
        ABORTS.with(|a| a.borrow().clone()),
        vec![],
        "the VM aborted the recording itself"
    );
}

// ---- Unstable re-record (TR24c) -------------------------------------------------------------------------------------

/// `Unstable` without in-place restarts aborts (the driver re-arms); with one restart the recorder re-records from the
/// types the finished iteration produced and closes a trace that replays correctly.
#[test]
fn unstable_rerecords_from_the_new_types() {
    let src = "local function f(n) local a = 0 ARM() for i = 1, n do a = a + 0.5 end return a end return f(@)";
    let args = "7";
    let rc = record_src(
        &src.replace("ARM()", "arm()").replace('@', args),
        RecCfg {
            restarts: 1,
            verify: true,
            ..RecCfg::default()
        },
    );
    let r = closed(&rc);
    assert_eq!(r.rerec, 1);
    // the accumulator is a float in the second recording
    let a_slot =
        r.ir.ins
            .iter()
            .find(|i| i.op == TOp::Sload && i.t == Ty::Num)
            .expect("a float SLOAD");
    assert_eq!(a_slot.t, Ty::Num);
    // and the trace is correct: replay against the VM
    let mut st = Stats::default();
    let log = log_src(
        &src.replace("ARM()", "armlog()").replace('@', args),
        3 * r.path.len() + 8,
    );
    // the VM's first logged state is the first iteration (a is still the integer 0): a SLOAD typecheck exits at once
    replay(&rc, &log, &mut st).unwrap();
    assert_eq!(st.exits, 1, "{st:?}");
    // a trace recorded one iteration later closes on the first try (what the driver gets after re-arming)
    let again = record_src(
        &src.replace("ARM()", "arm()").replace('@', "7"),
        RecCfg {
            restarts: 0,
            ..RecCfg::default()
        },
    );
    assert!(matches!(
        again.out,
        RecOutcome::Aborted(AbortReason::Unstable)
    ));
    // too many type changes: the restart budget is spent
    let flip = "local function f(n) local a = true ARM() for i = 1, n do a = not a end return a end return f(9)";
    let r = aborted_cfg(
        &flip.replace("ARM()", "arm()"),
        RecCfg {
            restarts: 2,
            ..RecCfg::default()
        },
    );
    assert_eq!(r, AbortReason::Unstable);
}

// ---- trap clearing at Leave (TR23.4) ------------------------------------------------------------------------------

/// After a recording ends (closed or aborted) no Lua frame keeps `trap = 1`. Mutant `leave_trap_kept` (rn-core
/// `rec_clear_traps`, cfg `jit_trace_mut`) is caught by the callee cases.
#[test]
fn traps_are_clear_after_the_recording() {
    for (name, src) in [
        ("closed", "local function f(n) local s = 0 arm() for i = 1, n do s = s + i end probe() return s end return f(5)"),
        ("aborted", "local function f(n) local s = 0 arm() for i = 1, n do local z = i .. 'x' .. i .. 'y' end probe() return s end return f(5)"),
        // the recording ends in a CALLEE frame (the CONCAT aborts in g) and g probes right away, before control
        // returns to f (the interpreter would otherwise heal a stale `trap` of f on the way back): Leave runs with
        // ci = g, and the caller frames up to the root must be cleared by `rec_clear_traps`, not only `ci`.
        // Mutant `leave_trap_kept` (rn-core) returns early there and leaves f's trap set.
        ("callee_abort", "local function g(i) local z = i .. 'x' .. i .. 'y' probe() return z end local function f(n) arm() for i = 1, n do local z = g(i) end return 0 end return f(1)"),
        ("callee_depth2", "local function h(i) local z = i .. 'x' .. i .. 'y' probe() return z end local function g(i) return (h(i)) end local function f(n) arm() for i = 1, n do local z = g(i) end return 0 end return f(1)"),
    ] {
        reset(RecCfg::default());
        let mut st = new_state();
        let _ = run(&mut st, src);
        drop(st);
        let traps = PROBES.with(|t| t.borrow().clone());
        assert_eq!(traps.len(), 1, "{name}");
        assert!(traps[0].iter().all(|&t| t == 0), "{name}: stale trap {traps:?}");
        assert_eq!(jit_hooks::rec_ctx(), RecCtx::default());
    }
}

// ---- upvalues -------------------------------------------------------------------------------------------------------

/// TR15: every `GETUPVAL` / `SETUPVAL` carries the range guard (`UVREL uref` then `UGE uvrel, 16 * nslots`) in front of
/// its access. Structure only; the behaviour is `upvalue_range_guard_exits`.
#[test]
fn upvalue_accesses_have_the_range_guard() {
    for name in ["upval_acc", "upval_rw"] {
        let rc = record_prog(prog(name), 1, strict());
        let r = closed(&rc);
        let ir = &r.ir;
        let mut urefs = 0;
        for (n, i) in ir.ins.iter().enumerate() {
            if i.op == TOp::Uload || i.op == TOp::Ustore {
                urefs += 1;
                let u = i.a;
                // a guard on a UVREL of this UREF precedes the access
                let guarded = ir.ins[..n].iter().enumerate().any(|(k, g)| {
                    g.op == TOp::Uge
                        && g.flags & F_GUARD != 0
                        && g.a >= REF_BIAS
                        && ir.ins[usize::from(g.a - REF_BIAS)].op == TOp::Uvrel
                        && ir.ins[usize::from(g.a - REF_BIAS)].a == u
                        && usize::from(g.a - REF_BIAS) < k
                        && ir.as_int(g.b) == Some(16 * rc.nslots as i64)
                });
                assert!(
                    guarded,
                    "{name}: %{n} accesses an upvalue without the TR15 range guard\n{}",
                    dump(ir)
                );
            }
        }
        assert!(urefs > 0, "{name}");
    }
}

/// TR15, behaviour: an upvalue whose value lives inside the traced frame (`uv.v - base` in `[0, 16 * nslots)`) must
/// make the IR exit before the access; one outside (closed, or below the frame) must not. The model's `uv_rel` is
/// what `UVREL` reads. Mutant `upval_range_skip` drops the guard: no exit is taken.
#[test]
fn upvalue_range_guard_exits() {
    for name in ["upval_acc", "upval_rw"] {
        let p = prog(name);
        let rc = record_prog(p, 1, strict());
        let r = closed(&rc);
        let log = log_src(
            &source(p, &rec_args(p, 1), "armlog()"),
            2 * r.path.len() + 4,
        );
        let lim = 16 * rc.nslots as u64;
        let ir = rewrite_func(&r.ir);
        let (ir_wb, probe) = with_probe(&ir, &r.writeback);
        let first_access = ir_wb
            .ins
            .iter()
            .position(|i| i.op == TOp::Uload || i.op == TOp::Ustore)
            .expect("an upvalue access");
        let with_rel = |rel: u64| {
            let mut m = model_from(&log[0]);
            m.uv_rel = vec![vec![rel; m.closures[0].len()]];
            m
        };
        for rel in [0, 16, lim - 1, lim - 16] {
            let mut m = with_rel(rel);
            match interp::run(&ir_wb, &mut m).unwrap() {
                Outcome::Exit { snap, .. } if snap != probe => {
                    // the exit's guard precedes the first access, and it is the range guard
                    let g = ir_wb
                        .ins
                        .iter()
                        .position(|i| i.flags & F_GUARD != 0 && aux_snap(i.aux) == snap)
                        .unwrap();
                    assert!(g < first_access, "{name}: rel {rel}: exit at %{g}");
                }
                other => panic!(
                    "{name}: an upvalue inside the frame (rel {rel}) did not exit: {other:?}"
                ),
            }
        }
        for rel in [lim, lim + 16, 1 << 40, u64::MAX - 7, u64::MAX - lim] {
            let mut m = with_rel(rel);
            match interp::run(&ir_wb, &mut m).unwrap() {
                Outcome::Exit { snap, .. } if snap == probe => {}
                other => {
                    panic!("{name}: an upvalue outside the frame (rel {rel}) exited: {other:?}")
                }
            }
        }
    }
}

/// An upvalue that points into the traced frame itself records as a register access (N1; it aborted `UpvalAlias` before, TR15):
/// no `ULOAD`, an identity guard `EQ UVREL, 16 * slot`, and the strict lockstep (`verify`) agrees with the VM.
#[test]
fn upvalue_into_the_frame_records_as_a_slot() {
    // f's second upvalue is joined to the upvalue of a closure created by f itself: it points at a local of f
    let src = "local u = 0
               local dbg = debug
               local arm = arm
               local function f(n)
                 local x = 7
                 local function g() return x end
                 local s = u
                 dbg.upvaluejoin(dbg.getinfo(1, 'f').func, 1, g, 1)
                 arm()
                 for i = 1, n do s = s + u end
                 return s
               end
               return f(5)";
    let rc = record_src(src, strict());
    let r = closed(&rc);
    assert!(r.ir.ins.iter().all(|i| i.op != TOp::Uload && i.op != TOp::Ustore), "the access is the register's");
    assert!(
        r.ir.ins.iter().any(|i| i.op == TOp::Eq && i.flags & F_GUARD != 0 && r.ir.ins.iter().any(|u| u.op == TOp::Uvrel)),
        "the identity guard EQ UVREL, 16 * slot"
    );
}

/// N3: a call of a Lua metamethod from the loop (`o1 + o2`, `#o1`, `-o1`) records the provider's guards -- a non-null metatable, an `HREFK`
/// of the event name in it, a typechecked `HLOAD` of a Lua closure, the `EQREF` of the closure's prototype -- and the C-stack guard
/// (`Lib` `CCALLS`) that keeps the in-place frames away from `LUAI_MAXCCALLS`. Detector of the mutant `mm_ccalls_skip` (its effect is a
/// safety margin that no output shows).
#[test]
fn mm_calls_record_their_guards() {
    let src = "local V = {}
               V.__add = function(a, b) return a.x + b.x end
               V.__len = function(a) return a.x end
               V.__unm = function(a) return -a.x end
               local o1 = setmetatable({x = 1}, V)
               local o2 = setmetatable({x = 2}, V)
               local arm = arm
               local function f(n)
                 local s = 0
                 arm()
                 for i = 1, n do s = s + (o1 + o2) + #o1 + (-o2) end
                 return s
               end
               return f(5)";
    let rc = record_src(src, RecCfg { mm: true, ..strict() });
    let ir = closed(&rc).ir.clone();
    let ccalls = ir.ins.iter().filter(|i| i.op == TOp::Lib && i.aux & 0xff == lib::CCALLS).count();
    // (N3b: one check per depth: the three calls of the loop body share the first)
    assert_eq!(ccalls, 1, "one C-stack guard per depth\n{}", dump(&ir));
    let loads: Vec<usize> = (0..ir.ins.len()).filter(|&x| ir.ins[x].op == TOp::Hload && ir.ins[x].t == Ty::Lcf).collect();
    assert_eq!(loads.len(), 3, "one closure load per metamethod call\n{}", dump(&ir));
    for &x in &loads {
        let i = ir.ins[x];
        assert!(i.flags & F_GUARD != 0 && aux_bits(i.aux) & 1 != 0, "unchecked metamethod load\n{}", dump(&ir));
        assert_eq!(ir.at(i.a).op, TOp::Hrefk);
        let node = ir.at(ir.at(i.a).a);
        let mt = ir.at(node.a);
        assert!(mt.op == TOp::Fload && mt.b == Field::Metatable as Ref, "the event's HREFK is not on a metatable\n{}", dump(&ir));
        assert!(
            ir.ins[..x].iter().any(|g| g.op == TOp::Ne && g.a == node.a && ir.as_int(g.b) == Some(0)),
            "metamethod load without `mt != 0`\n{}",
            dump(&ir)
        );
        assert!(
            ir.ins[x..].iter().any(|g| g.op == TOp::Eqref && g.a == x as Ref + REF_BIAS && g.flags & F_GUARD != 0),
            "metamethod closure without the prototype guard\n{}",
            dump(&ir)
        );
    }
}

/// N3b: `return setmetatable({..}, V)` in a metamethod (a `TAILCALL` of a C function in an inlined frame) records as the `SETMT` `Lib`
/// kind (the model learns the metatable store: the later `FLOAD metatable` guards of the new table hold in the lockstep), the guard
/// `NE %tnew, 0` of the metamethod's table carries a snapshot of the caller's frame (nothing was changed in the body before it: a due
/// collector step restarts the `+`), and a body that stored something before the allocation keeps the snapshot of its own frame.
/// Detector of the mutants `setmt_model_skip` and `alloc_shallow_impure`.
#[test]
fn setmt_in_a_metamethod_records_with_a_shallow_allocation_exit() {
    let pure = "local V = {}
               V.__index = V
               V.__add = function(a, b) return setmetatable({x = a.x + b.x}, V) end
               local one = setmetatable({x = 1}, V)
               local arm = arm
               local function f(n)
                 local acc = setmetatable({x = 0}, V)
                 arm()
                 for i = 1, n do acc = acc + one end
                 return acc.x
               end
               return f(5)";
    let cfg = || RecCfg { mm: true, setmt: true, tailnat: true, ..strict() };
    let rc = record_src(pure, cfg());
    let ir = closed(&rc).ir.clone();
    let setmts: Vec<usize> = (0..ir.ins.len()).filter(|&x| ir.ins[x].op == TOp::Lib && ir.ins[x].aux & 0xff == lib::SETMT).collect();
    assert_eq!(setmts.len(), 1, "one SETMT\n{}", dump(&ir));
    let tnew = (0..ir.ins.len()).find(|&x| ir.ins[x].op == TOp::Tnew).expect("TNEW");
    let g = ir.ins[tnew + 1];
    assert!(g.op == TOp::Ne && g.flags & F_GUARD != 0 && g.a == tnew as Ref + REF_BIAS, "the allocation guard follows\n{}", dump(&ir));
    assert_eq!(ir.snaps[aux_snap(g.aux) as usize].nframes, 0, "a pure body's allocation exit restarts the caller's instruction\n{}", dump(&ir));
    // a store before the allocation: the exit keeps the metamethod's frame
    let impure = "local V = {}
               V.__index = V
               V.__add = function(a, b) a.n = a.n + 1 return setmetatable({x = a.x + b.x}, V) end
               local one = setmetatable({x = 1, n = 0}, V)
               local arm = arm
               local function f(n)
                 local acc = setmetatable({x = 0, n = 0}, V)
                 arm()
                 for i = 1, n do acc = acc + one end
                 return acc.x
               end
               return f(5)";
    let rc = record_src(impure, cfg());
    let ir = closed(&rc).ir.clone();
    let tnew = (0..ir.ins.len()).find(|&x| ir.ins[x].op == TOp::Tnew).expect("TNEW");
    let g = ir.ins[tnew + 1];
    assert_eq!(ir.snaps[aux_snap(g.aux) as usize].nframes, 1, "an impure body keeps its frame in the exit\n{}", dump(&ir));
}

/// A1: which `NEWREF`s of a constructor are lowered to inline stores: the keys of a small constructor (any number of them, with the
/// collisions of their hashes), not those of a table with 16 nodes, not the keys inserted after an integer-key `TSET` (the node of that key
/// depends on its value), and none with `nrinline` off.
#[test]
fn constructor_keys_are_inserted_inline() {
    let inline = |src: &str, nrinline: bool| -> (usize, usize) {
        let rc = record_src(src, RecCfg { nrinline, ..strict() });
        let ir = closed(&rc).ir.clone();
        let all: Vec<_> = ir.ins.iter().filter(|i| i.op == TOp::Newref).collect();
        let n = all.iter().filter(|i| i.aux & rn_jit::tr::ir::nrplan::INLINE != 0).count();
        (n, all.len())
    };
    let small = "local arm = arm local function f(n) local s = 0 arm() for i = 1, n do local t = {a = i, b = 2, c = 3, d = 4, e = 5} s = s + t.a + t.e end return s end return f(5)";
    assert_eq!(inline(small, true), (5, 5), "five inline insertions");
    assert_eq!(inline(small, false), (0, 5), "none with the knob off");
    let big = "local arm = arm local function f(n) local s = 0 arm() for i = 1, n do local t = {a = i, b = 2, c = 3, d = 4, e = 5, f = 6, g = 7, h = 8, i = 9} s = s + t.a + t.i end return s end return f(5)";
    assert_eq!(inline(big, true).0, 0, "16 nodes: the helper");
    let tset = "local arm = arm local function f(n) local s = 0 arm() for i = 1, n do local t = {a = 1, b = 2, c = 3, d = 4, e = 5} t[i * 3 + 1] = i t.f = 6 s = s + t.a + t.f end return s end return f(5)";
    assert_eq!(inline(tset, true), (5, 6), "the insertion after the integer key is the helper's");
}

// ---- 5.4 ----------------------------------------------------------------------------------------------------------------

// ---- mutants ------------------------------------------------------------------------------------------------------------

/// Every mutant of this task is selected in a child process (`RUSTYNAIL_JIT_MUT`), which must fail the test that guards
/// it; the baseline child must pass. `leave_trap_kept` lives in `rn-core::jit_hooks::rec_clear_traps` (cfg
/// `jit_trace_mut`, set by `apply_vm_mutants`).
#[cfg(feature = "tr_mutants")]
#[test]
fn mutants_are_caught() {
    if std::env::var("TR_RECORD_CHILD").is_ok() {
        return;
    }
    let exe = std::env::current_exe().unwrap();
    let table: &[(&str, &[&str])] = &[
        ("snap_drop_last", &["ir_replay_matches_the_interpreter"]),
        ("snap_prune_dead", &["ir_replay_matches_the_interpreter"]),
        (
            "refund_off_by_one",
            &["count_model_is_exact", "ir_replay_matches_the_interpreter"],
        ),
        (
            "land_pre_mid_swap",
            &["count_model_is_exact", "ir_replay_matches_the_interpreter"],
        ),
        (
            "rec_no_validate",
            &["stale_context_is_rejected_by_the_recorder_validation"],
        ),
        ("leave_trap_kept", &["traps_are_clear_after_the_recording"]),
        ("hrefk_lsize_skip", &["hash_ops_record_their_guards"]),
        ("meta_flag_skip", &["hash_ops_record_their_guards"]),
        ("hash_nil_meta_skip", &["hash_ops_record_their_guards"]),
        ("hstore_tbar_skip", &["hash_ops_record_their_guards"]),
        ("hmiss_skip", &["index_chain_records_its_guards"]),
        ("hmiss_lsize_skip", &["index_chain_records_its_guards"]),
        ("index_mt_null_skip", &["index_chain_records_its_guards"]),
        ("index_tab_typecheck_skip", &["index_chain_records_its_guards"]),
        ("index_nilnode_skip", &["index_chain_records_its_guards"]),
        ("chain_end_skip", &["index_chain_records_its_guards"]),
        (
            "upval_range_skip",
            &[
                "upvalue_accesses_have_the_range_guard",
                "upvalue_range_guard_exits",
            ],
        ),
    ];
    let child = |mutant: &str, test: &str| {
        let out = std::process::Command::new(&exe)
            .args(["--exact", test, "--test-threads=1"])
            .env("TR_RECORD_CHILD", "1")
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
    for (mutant, tests) in table {
        for test in *tests {
            let (ok, _) = child("", test);
            assert!(ok, "baseline child of {test} failed");
        }
        for test in *tests {
            let (ok, text) = child(mutant, test);
            assert!(!ok, "mutant {mutant} was NOT caught by {test}\n{text}");
        }
    }
}

/// TR17 compares floats by exact bits: a NaN payload changed behind the recorder's back diverges (only a value that
/// derives from a `CALLN` helper is compared leniently, because `interp` canonicalises the helpers' NaNs).
#[test]
fn nan_payloads_are_compared_exactly() {
    let src = "local function f(n, z) local s = z arm() for i = 1, n do s = s + 0.5 end return s end return f(5, 0/0)";
    // slot 2 is `s`; a NaN with another payload replaces it before the recorder's step 2
    for (payload, diverges) in [
        (0xFFF8_0000_0000_0000u64, false),
        (0x7FF8_0000_0000_0001, true),
    ] {
        reset(RecCfg::default());
        TAMPER.set(Some((2, 2, Value::float(f64::from_bits(payload)))));
        let mut st = new_state();
        let _ = run(&mut st, src);
        drop(st);
        let (_, out) = record::take_outcome().unwrap();
        assert_eq!(
            matches!(out, RecOutcome::Aborted(AbortReason::Diverge)),
            diverges,
            "payload {payload:#x}: {out:?}"
        );
    }
}

/// A NaN produced by a `CALLN` helper (`pow`) may differ in payload between libm and `interp`: the recording closes.
#[test]
fn calln_nan_is_not_a_divergence() {
    let src = "local function f(n, z) local s = 0.0 arm() for i = 1, n do s = s + z ^ 0.5 end return s end return f(5, -1.0)";
    let rc = record_src(src, strict());
    let r = closed(&rc);
    assert!(r.ir.ins.iter().any(|i| i.op == TOp::Calln));
}

/// F6 (found by the `jit_diff` fuzzer with `+verify`): a table store of a new key right behind an inlined call. The interpreter
/// keeps the table at `top` (above the frame) while it grows it, which overwrote the dead register of the popped callee that the
/// IR model still tracked; the lockstep reported a false divergence (an abort in production, a panic with `verify`).
#[test]
fn newkey_store_after_inlined_call_is_not_a_divergence() {
    for body in ["local x = f(0) t[i] = x", "t[i] = (f(0))", "f(0) t.k = i"] {
        let src = format!(
            "local function f(a, b) return a, b end local function g() local t = {{}} arm() for i = 1, 19 do {body} end return #t end return g()"
        );
        let rc = record_src(&src, strict());
        let _ = closed(&rc);
    }
}

/// The Miri-sized variant of the replay (`scripts/miri.sh jit-trace` selects `miri_*`): three programs, one recording
/// each, two real runs per variant. Same code path as the full test.
#[test]
fn miri_replay_small() {
    for name in ["int_sum", "array_rec", "upval_acc"] {
        let mut st = Stats::default();
        replay_prog(prog(name), 1, 2, &[0, 1], &mut st);
        assert!(st.done + st.exits > 0 && st.forced > 0, "{name}: {st:?}");
    }
}

/// Q4 regression: the recording and the logged run are different `State`s, so the short-string addresses in the
/// recorded IR (HREFK keys) and in the model built from the log must be matched by content, not by allocator luck.
/// With many threads allocating at once the addresses differ; before the fix `fields_meta_flag` failed here every time
/// ("a guard fired on the recorded inputs") and in about 1 of 6 full runs of this binary.
#[test]
fn string_addresses_are_matched_by_content_under_threads() {
    let hs: Vec<_> = (0..12)
        .map(|_| {
            std::thread::spawn(|| {
                for _ in 0..40 {
                    for name in ["fields_meta_flag", "fields_rw", "fields_strkey", "fields_nested"] {
                        let p = prog(name);
                        let rc = record_prog(p, 1, strict());
                        let r = closed(&rc);
                        let log = log_src(
                            &source(p, &rec_args(p, 1), "armlog()"),
                            2 * r.path.len() + 4,
                        );
                        assert_eq!(completes_recorded_iteration(r, &log), Ok(()), "{name}");
                    }
                }
            })
        })
        .collect();
    for h in hs {
        h.join().unwrap();
    }
}

/// BUNDLE-V V2 (`upbar`): `SETUPVAL` of a collectable value into an upvalue cell records `USTORE` + `TBAR uref, v` with `aux` 1 (the upvalue barrier); without the knob
/// the recording aborts `Nyi(Setupval)`; a non-collectable value needs no barrier. Mutant `upbar_skip` (the `TBAR` is not recorded).
#[test]
fn bv_upvalue_store_records_the_barrier() {
    let src = "local u local function f(n) ARM() for i = 1, n do u = {} end end f(5) return 1";
    let on = || RecCfg { verify: true, upbar: 1, ..RecCfg::default() };
    let rc = record_src(&src.replace("ARM()", "arm()"), on());
    let ir = closed(&rc).ir.clone();
    let at = ir.ins.iter().position(|i| i.op == TOp::Ustore).unwrap_or_else(|| panic!("no USTORE\n{}", dump(&ir)));
    let bar = ir.ins[at + 1];
    assert!(bar.op == TOp::Tbar && bar.aux & 1 == 1 && bar.a == ir.ins[at].a && bar.b == ir.ins[at].b, "no upvalue barrier after the USTORE\n{}", dump(&ir));
    // a number needs no barrier
    let rc = record_src("local u local function f(n) arm() for i = 1, n do u = i * 2 end end f(5) return 1", on());
    let ir = closed(&rc).ir.clone();
    assert!(ir.ins.iter().any(|i| i.op == TOp::Ustore) && !ir.ins.iter().any(|i| i.op == TOp::Tbar), "barrier for a number\n{}", dump(&ir));
}

/// F3b: the generic-`for` calls and the string helpers record their guards: `NE` after every class-A helper (the "nothing
/// done: exit" check on the instruction's snapshot), the typechecked loads of the pseudo node after `TNEXT`, an `EQREF` of the
/// iterator against the builtin, the `NE` after `SNEW` / `CONCAT`.
#[test]
fn f3b_ops_record_their_guards() {
    let (next, ipairs_aux) = rn_std::lib_base::jit_builtins();
    rn_core::jit_hooks::set_builtins(next, ipairs_aux);
    let ir_of = |src: &str| -> TraceIr {
        let rc = record_src(&src.replace("ARM()", "arm()"), strict());
        closed(&rc).ir.clone()
    };
    let followed_by_ne = |ir: &TraceIr, op: TOp| {
        let at = ir.ins.iter().position(|i| i.op == op).unwrap_or_else(|| panic!("no {op:?}\n{}", dump(ir)));
        let mut j = at + 1;
        // the helper's own guard is the next guard: `NE %helper, 0`
        while j < ir.ins.len() && ir.ins[j].flags & F_GUARD == 0 {
            j += 1;
        }
        let g = &ir.ins[j];
        assert!(
            g.op == TOp::Ne && g.a == REF_BIAS + at as Ref && ir.as_int(g.b) == Some(0),
            "{op:?} not followed by NE %h, 0\n{}",
            dump(ir)
        );
    };
    // pairs: the iterator is guarded against the builtin, TNEXT is guarded, key and value are typechecked pseudo-node loads
    let ir = ir_of("local t = {} for i = 1, 40 do t['k' .. i] = i end local function f() local s = 0 ARM() for k, v in pairs(t) do s = s + v end return s end return f()");
    followed_by_ne(&ir, TOp::Tnext);
    let loads: Vec<_> = ir
        .ins
        .iter()
        .filter(|i| i.op == TOp::Hload && TraceIr::is_ins(i.a) && ir.at(i.a).op == TOp::Tnext)
        .collect();
    assert_eq!(loads.len(), 2, "key and value\n{}", dump(&ir));
    assert!(loads.iter().all(|l| l.flags & F_GUARD != 0 && aux_bits(l.aux) & 1 != 0), "unchecked pseudo-node load\n{}", dump(&ir));
    assert!(loads.iter().any(|l| aux_bits(l.aux) & 2 != 0), "no key load\n{}", dump(&ir));
    assert!(ir.ins.iter().any(|i| i.op == TOp::Eqref), "no iterator identity guard\n{}", dump(&ir));
    // ipairs: no helper, the iterator guard and the array access
    let ir = ir_of("local t = {} for i = 1, 40 do t[i] = i end local function f() local s = 0 ARM() for i, v in ipairs(t) do s = s + v end return s end return f()");
    assert!(ir.ins.iter().any(|i| i.op == TOp::Eqref), "no iterator identity guard\n{}", dump(&ir));
    assert!(ir.ins.iter().any(|i| i.op == TOp::Aload), "no array load\n{}", dump(&ir));
    // a table key built by CONCAT is a new string almost every iteration: the EQREF guard would fail every time, the loop is not traced
    assert_eq!(
        aborted("local t = {} local function f(n) ARM() for i = 1, n do t['k' .. i] = i end end f(5) return 1".replace("ARM()", "arm()").as_str()),
        AbortReason::TableHash
    );
    // SNEW / CONCAT
    let ir = ir_of("local function f(n) local s = 0 ARM() for i = 1, n do local z = 'k' .. i .. 'x' s = s + #z end return s end return f(30)");
    followed_by_ne(&ir, TOp::Snew);
    followed_by_ne(&ir, TOp::Concat);
    // the converted operand replaces the number in its register (the interpreter converts in place): the iteration's
    // writeback holds the SNEW result
    let rc = record_src(
        &"local function f(n) local s = 0 ARM() for i = 1, n do local z = 'k' .. 'x' .. i s = s + #z end return s end return f(30)".replace("ARM()", "arm()"),
        strict(),
    );
    let c = closed(&rc);
    let sn = c.ir.ins.iter().position(|i| i.op == TOp::Snew).expect("SNEW");
    assert!(
        c.writeback.iter().any(|e| e.r == REF_BIAS + sn as Ref),
        "the converted operand is not written back\n{}",
        dump(&c.ir)
    );
}

/// F2: the library-call recorders emit their guards: string comparisons (`LIB` kind followed by `NE %h, 0` and by `EQ %h, outcome + 1`),
/// `getmetatable` (a metatable-word guard and the `HMISS` proof that `__metatable` is absent), `rawget` / `rawset` (no metatable guard, the
/// write barrier of a collectable value), `assert` (nothing but the argument's own type guard).
#[test]
fn f2_ops_record_their_guards() {
    rn_core::jit_hooks::set_libfns(
        {
            let [sub, byte, chr] = rn_std::lib_string::jit_libfns();
            let [ins, rem] = rn_std::lib_table::jit_libfns();
            let [mty, fmod, toint] = rn_std::lib_math::jit_libfns();
            let ([tostr, rawlen, rawequal, rawget, rawset, tonum], _) = rn_std::lib_base::jit_libfns();
            [sub, byte, chr, mty, tostr, rawlen, rawequal, rawget, rawset, ins, rem, fmod, toint, tonum]
        },
        rn_std::lib_base::jit_libfns().1,
    );
    let cfg = || RecCfg { f2cmp: true, f2mt: true, f2raw: true, f2asrt: true, mm: true, ..strict() };
    let ir_of = |src: &str| -> TraceIr {
        let rc = record_src(&src.replace("ARM()", "arm()"), cfg());
        closed(&rc).ir.clone()
    };
    // string comparisons
    let ir = ir_of("local A, B = {}, {} for i = 1, 8 do A[i] = string.rep('a', 50) .. i; B[i] = string.rep('a', 50) .. (i + 1) end local arm = arm local function f(n) local c = 0 ARM() for i = 1, n do local k = i % 8 + 1 if A[k] < B[k] then c = c + 1 end if A[k] == B[k] then c = c + 1 end end return c end return f(30)");
    for k in [lib::STRLT, lib::STREQ] {
        let at = ir.ins.iter().position(|i| i.op == TOp::Lib && i.aux & 0xFF == k).unwrap_or_else(|| panic!("no LIB {k}\n{}", dump(&ir)));
        let mut j = at + 1;
        while ir.ins[j].flags & F_GUARD == 0 {
            j += 1;
        }
        assert!(ir.ins[j].op == TOp::Ne && ir.ins[j].a == REF_BIAS + at as Ref, "LIB {k} not followed by NE %h, 0\n{}", dump(&ir));
        j += 1;
        while ir.ins[j].flags & F_GUARD == 0 {
            j += 1;
        }
        let g = ir.ins[j];
        assert!(g.op == TOp::Eq && g.a == REF_BIAS + at as Ref && ir.as_int(g.b).is_some_and(|v| v == 1 || v == 2), "LIB {k} outcome not guarded\n{}", dump(&ir));
    }
    // getmetatable
    let ir = ir_of("local mt = {} local objs = {} for i = 1, 8 do objs[i] = setmetatable({}, mt) end local getmetatable = getmetatable local arm = arm local function f(n) local c = 0 ARM() for i = 1, n do if getmetatable(objs[i % 8 + 1]) == mt then c = c + 1 end end return c end return f(30)");
    assert!(ir.ins.iter().any(|i| i.op == TOp::Hmiss && i.flags & F_GUARD != 0), "no HMISS proof of the absent `__metatable`\n{}", dump(&ir));
    assert!(ir.ins.iter().any(|i| i.op == TOp::Ne && i.flags & F_GUARD != 0 && TraceIr::is_ins(i.a) && ir.at(i.a).op == TOp::Fload && ir.at(i.a).b == Field::Metatable as Ref), "no metatable-word guard\n{}", dump(&ir));
    // rawget / rawset: no metatable guard (the table has one), the barrier of a collectable value
    #[cfg(not(rustynail_lua54))]
    {
        let ir = ir_of("local t = setmetatable({}, {__index = function() return 0 end, __newindex = function() end}) for i = 1, 8 do rawset(t, i, i) end local rawset, rawget = rawset, rawget local arm = arm local function f(n) local c = 0 ARM() for i = 1, n do rawset(t, i % 8 + 1, {}) if rawget(t, i % 8 + 1) then c = c + 1 end end return c end return f(30)");
        assert!(ir.ins.iter().any(|i| i.op == TOp::Astore), "no array store\n{}", dump(&ir));
        assert!(ir.ins.iter().any(|i| i.op == TOp::Tbar), "no write barrier after the store of a table\n{}", dump(&ir));
        assert!(!ir.ins.iter().any(|i| i.op == TOp::Eq && i.flags & F_GUARD != 0 && TraceIr::is_ins(i.a) && ir.at(i.a).op == TOp::Fload && ir.at(i.a).b == Field::Metatable as Ref), "metatable guard in a raw access\n{}", dump(&ir));
    }
    // assert of a truthy value records no helper and no extra guard
    let ir = ir_of("local assert = assert local arm = arm local function f(n) local c = 0 ARM() for i = 1, n do c = c + assert(i, 'msg') end return c end return f(30)");
    assert!(!ir.ins.iter().any(|i| i.op == TOp::Lib), "assert recorded a helper\n{}", dump(&ir));
}

/// P-B: the library-call intrinsics record their guards: the `NE %lib, 0` behind every `LIB`, the `__len` / metatable guard of
/// `table.insert` / `table.remove`, the pointer compare of short-string equality; and `AllocStorm` aborts an allocating loop when the
/// collector steps about as often as the loop allocates.
#[test]
fn pb_ops_record_their_guards() {
    rn_core::jit_hooks::set_libfns(
        {
            let [sub, byte, chr] = rn_std::lib_string::jit_libfns();
            let [ins, rem] = rn_std::lib_table::jit_libfns();
            let [mty, fmod, toint] = rn_std::lib_math::jit_libfns();
            let ([tostr, rawlen, rawequal, rawget, rawset, tonum], _) = rn_std::lib_base::jit_libfns();
            [sub, byte, chr, mty, tostr, rawlen, rawequal, rawget, rawset, ins, rem, fmod, toint, tonum]
        },
        rn_std::lib_base::jit_libfns().1,
    );
    let ir_of = |src: &str| -> TraceIr {
        let rc = record_src(&src.replace("ARM()", "arm()"), strict());
        closed(&rc).ir.clone()
    };
    let lib_followed_by_ne = |ir: &TraceIr, kind: u32| {
        let at = ir
            .ins
            .iter()
            .position(|i| i.op == TOp::Lib && i.aux & 0xFF == kind)
            .unwrap_or_else(|| panic!("no LIB {kind}\n{}", dump(ir)));
        let mut j = at + 1;
        while j < ir.ins.len() && ir.ins[j].flags & F_GUARD == 0 {
            j += 1;
        }
        let g = &ir.ins[j];
        assert!(
            g.op == TOp::Ne && g.a == REF_BIAS + at as Ref && ir.as_int(g.b) == Some(0),
            "LIB {kind} not followed by NE %h, 0\n{}",
            dump(ir)
        );
    };
    let ir = ir_of("local s = 'hello world' local function f(n) local a = 0 ARM() for i = 1, n do local x = s:sub(2, 4) a = a + #x + s:byte(3) + #string.char(65 + i % 26) + #tostring(i) + #type(a) + #math.type(a) end return a end return f(30)");
    for k in [lib::SUB, lib::BYTE, lib::CHAR, lib::TOSTR, lib::TYPE, lib::TYPENAME] {
        lib_followed_by_ne(&ir, k);
    }
    // P-V: `s:method` is no helper call any more: the string metatable is a constant (`KPTR` of type table) behind guarded `HREFK` loads
    assert!(!ir.ins.iter().any(|i| i.op == TOp::Lib && i.aux & 0xFF == lib::STRMETH), "STRMETH still recorded\n{}", dump(&ir));
    assert!(ir.ins.iter().any(|i| i.op == TOp::Kptr && i.t == Ty::Tab), "no constant string metatable\n{}", dump(&ir));
    assert!(ir.ins.iter().filter(|i| i.op == TOp::Hrefk).count() >= 2, "no HREFK chain for the string method\n{}", dump(&ir));
    // table.insert / table.remove: the metatable guard before the length, then the stores (5.4: a store into an empty slot inside the array
    // part is an `Insert` abort, F3, so the append of a table with spare capacity is not recorded there)
    #[cfg(not(rustynail_lua54))]
    {
    let ir = ir_of("local t = {} for i = 1, 50 do t[i] = i end local function f(n) local a = 0 ARM() for i = 1, n do table.insert(t, i) a = a + table.remove(t) end return a end return f(30)");
    let tl = ir.ins.iter().position(|i| i.op == TOp::Tlen).unwrap_or_else(|| panic!("no TLEN\n{}", dump(&ir)));
    assert!(
        ir.ins[..tl].iter().any(|i| i.op == TOp::Eq && i.flags & F_GUARD != 0 && TraceIr::is_ins(i.a) && ir.at(i.a).op == TOp::Fload && ir.at(i.a).b == Field::Metatable as Ref && ir.at(i.a).a == ir.ins[tl].a),
        "no metatable guard before the length\n{}",
        dump(&ir)
    );
    }
    // table.remove alone (all versions): the same guard before the length
    let ir = ir_of("local t = {} for i = 1, 500 do t[i] = i end local function f(n) local a = 0 ARM() for i = 1, n do a = a + table.remove(t) end return a end return f(30)");
    let tl = ir.ins.iter().position(|i| i.op == TOp::Tlen).unwrap_or_else(|| panic!("no TLEN\n{}", dump(&ir)));
    assert!(
        ir.ins[..tl].iter().any(|i| i.op == TOp::Eq && i.flags & F_GUARD != 0 && TraceIr::is_ins(i.a) && ir.at(i.a).op == TOp::Fload && ir.at(i.a).b == Field::Metatable as Ref && ir.at(i.a).a == ir.ins[tl].a),
        "no metatable guard before the length (remove)\n{}",
        dump(&ir)
    );
    // short-string equality is a pointer compare guard
    let ir = ir_of("local function f(n) local a = 0 ARM() for i = 1, n do if type(i) == 'number' then a = a + 1 end end return a end return f(30)");
    assert!(
        ir.ins.iter().any(|i| i.op == TOp::Eq && i.flags & F_GUARD != 0 && TraceIr::is_ins(i.a) && ir.at(i.a).op == TOp::Lib),
        "no string equality guard\n{}",
        dump(&ir)
    );
}

/// P-B: the allocation storm guard aborts the recording of a loop that allocates strings when the collector steps every few iterations.
#[test]
fn pb_storm_guard_aborts_allocating_loops() {
    let src = "local function f(n) local s = 0 arm() for i = 1, n do local z = 'k' .. i .. 'x' s = s + #z end return s end return f(30)";
    let cfg = RecCfg { storm_step: 64, ..strict() };
    assert_eq!(aborted_cfg(src, cfg), AbortReason::AllocStorm);
    // a collector that steps every megabyte: recorded
    let rc = record_src(src, RecCfg { storm_step: 1 << 20, ..strict() });
    let _ = closed(&rc);
}

/// BUNDLE-A: the recorded `tonumber(string)` / `next(t [, k])` / plain `string.find` record their guards: `STRTO` in tonumber mode followed by
/// `NE %p, 0` and a typed `HLOAD`; `next` has the callee-identity `EQREF` against the builtin, `TNEXT`, `NE` and the key / value `HLOAD`s; `FIND` /
/// `FINDS` is followed by `NE %lib, 0` and the outcome guard. Mutants `next_callee_guard_skip`, `tonum_type_guard_skip`, `find_outcome_guard_skip`.
#[test]
fn ba_ops_record_their_guards() {
    let (next, ipairs_aux) = rn_std::lib_base::jit_builtins();
    jit_hooks::set_builtins(next, ipairs_aux);
    {
        let [sub, byte, chr] = rn_std::lib_string::jit_libfns();
        let [ins, rem] = rn_std::lib_table::jit_libfns();
        let [mty, fmod, toint] = rn_std::lib_math::jit_libfns();
        let ([tostr, rawlen, rawequal, rawget, rawset, tonum], typefn) = rn_std::lib_base::jit_libfns();
        jit_hooks::set_libfns([sub, byte, chr, mty, tostr, rawlen, rawequal, rawget, rawset, ins, rem, fmod, toint, tonum], typefn);
    }
    let cfg = || RecCfg { libnum: true, libnext: true, libfind: true, ..strict() };
    let ir_of = |src: &str| -> TraceIr {
        eprintln!("recording: {src}");
        let rc = record_src(&src.replace("ARM()", "arm()"), cfg());
        closed(&rc).ir.clone()
    };
    // next: identity guard, TNEXT, NE, typed HLOADs (5.5 only: the 5.4 recorder does not record `next` as a library call, the `CFunc` abort is by design)
    if !cfg!(rustynail_lua54) {
        let ir = ir_of("local t = { 5 } local function f(n) local a = 0 ARM() for i = 1, n do local k, v = next(t) a = a + k + v end return a end return f(30)");
        let builtin_eq = ir.ins.iter().filter(|i| i.op == TOp::Eqref && matches!(ir.k[usize::from(i.b) - 1], KVal::Ptr(_, PtrKind::Builtin))).count();

        assert!(builtin_eq >= 1, "no callee identity guard for next\n{}", dump(&ir));
        assert!(ir.ins.iter().any(|i| i.op == TOp::Tnext), "no TNEXT\n{}", dump(&ir));
        assert!(ir.ins.iter().filter(|i| i.op == TOp::Hload && i.flags & F_GUARD != 0).count() >= 2, "key / value HLOAD guards missing\n{}", dump(&ir));
    }
    // tonumber(string): STRTO in tonumber mode, NE, typed HLOAD
    let ir = ir_of("local s = '2.5' local function f(n) local a = 0.0 ARM() for i = 1, n do a = a + tonumber(s) end return a end return f(30)");
    let at = ir.ins.iter().position(|i| i.op == TOp::Strto && i.aux & 2 != 0).unwrap_or_else(|| panic!("no STRTO in tonumber mode\n{}", dump(&ir)));
    assert!(ir.ins[at..].iter().any(|i| i.op == TOp::Ne && i.a == REF_BIAS + at as Ref && i.flags & F_GUARD != 0), "no NE guard behind STRTO\n{}", dump(&ir));
    assert!(ir.ins[at..].iter().any(|i| i.op == TOp::Hload && i.t == Ty::Num && i.flags & F_GUARD != 0 && i.aux & 1 != 0), "no typed HLOAD of the float result\n{}", dump(&ir));
    // plain find: LIB FIND (FINDS without the plain argument), NE %lib, 0, then the outcome guard against 1
    for (src, kind) in [
        ("local s, p = 'hello world', 'wor' local function f(n) local a = 0 ARM() for i = 1, n do local x, y = s:find(p, 1, true) a = a + x + y end return a end return f(30)", lib::FIND),
        ("local s, p = 'hello world', 'wor' local function f(n) local a = 0 ARM() for i = 1, n do local x, y = s:find(p, 1) a = a + x + y end return a end return f(30)", lib::FINDS),
    ] {
        let ir = ir_of(src);
        let at = ir.ins.iter().position(|i| i.op == TOp::Lib && i.aux & 0xFF == kind).unwrap_or_else(|| panic!("no LIB {kind}\n{}", dump(&ir)));
        let guards: Vec<_> = ir.ins[at + 1..].iter().filter(|i| i.flags & F_GUARD != 0 && i.a == REF_BIAS + at as Ref).collect();
        assert!(guards.len() >= 2, "LIB {kind}: expected the NE 0 guard and the outcome guard\n{}", dump(&ir));
    }
}
