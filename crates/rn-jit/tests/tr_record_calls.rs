//! J5-P3b (docs/JIT_TRACE.md 4.4, 4.6, 5.4, 5.5, TR3, TR5.5, TR6b/c/d, TR12, TR32): the recorder with Lua calls
//! (inlined) and the `math` intrinsics.
//!
//! * golden IR dumps (`tests/tr_record_golden_calls.txt`, regenerate with `TR_BLESS=1`) of eight inlining programs;
//! * the abort matrix of every call-related reason of 4.6;
//! * lockstep (TR17): the recorded path, callee frames included, equals the instruction sequence the VM executes;
//! * structure: frames and `FUNC` entries of snapshots, popped-frame slots, the count model across frames (TR5.5),
//!   room / CallInfo conditions (TR6b/c), entry-guard snapshots (TR6d), dependency edges at inline time (TR32);
//! * IR replay: the recorded IR run with `tr::interp` against the real interpreter, comparing at every exit the
//!   restored stack (the whole extent, popped frames and the natives' result copies included), the tables and
//!   upvalues, **the `CallInfo` chain** (`func`, `top`, `nresults`, the callers' `savedpc`) and the budget; every guard
//!   is also forced to fail;
//! * mutants (feature `tr_mutants`): each is selected in a child process, which must fail the matching test.
#![cfg(feature = "trace")]

use rn_core::func::{LClosure, Proto};
use rn_core::instruction::Instruction;
use rn_core::jit_hooks::{self, Abort, RecCtx, RecStep, RecVTable};
use rn_core::state::{Ci, LuaState, State, StkId, CIST_NRESULTS, CIST_TAIL, LUA_MINSTACK, LUA_MULTRET};
use rn_core::status::{LuaResult, Status};
use rn_core::table::TableRef;
use rn_core::value::{Gc, Value};
use rn_core::ver::active::OpCode;
use rn_jit::lower::{LabelKind, Lowered};
use rn_jit::tr::interp::gen::Rng;
use rn_jit::tr::interp::{self, MTable, Model, Outcome, Val};
use rn_jit::tr::ir::*;
use rn_jit::tr::print::dump;
use rn_jit::tr::record::{
    self, AbortReason, DepHooks, RecCfg, RecOutcome, Recorded, FUNC_SLOT, MAX_DEPTH,
};
use rn_jit::tr::snap;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Once;

/// F6: the tests leak programs / strings on purpose (`&'static`); recording the address in a static keeps the memory *reachable*, so
/// LeakSanitizer (scripts/asan-jit.sh) does not report it.
static KEPT: std::sync::Mutex<Vec<usize>> = std::sync::Mutex::new(Vec::new());
fn keep<T: ?Sized>(r: &'static T) -> &'static T {
    KEPT.lock().unwrap_or_else(|e| e.into_inner()).push((r as *const T).cast::<u8>() as usize);
    r
}

// ---- harness: the dispatching recorder, natives, logging recorder -----------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Record,
    Log,
}

/// One frame of the VM's `CallInfo` chain at a logged step (outermost first; index 0 is the root).
#[derive(Clone, Debug, PartialEq)]
struct FrameObs {
    proto: usize,
    /// `func` relative to the root function slot.
    func_rel: i64,
    /// `top` relative to the root function slot.
    top_rel: i64,
    /// Wanted results (`callstatus - 1`).
    nres: i32,
    /// Index of the instruction the frame resumes at (the innermost frame: the one about to run).
    pc: usize,
    maxstack: usize,
    /// F4: entered by a `TAILCALL` (`CIST_TAIL`).
    tail: bool,
    /// P-E: `CallInfo::nextraargs` of a vararg frame (`-1`: the prototype is not vararg).
    nextra: i32,
}

/// One logged state of the real interpreter, before an instruction of a recorded frame.
#[derive(Clone, Debug, PartialEq)]
struct Step {
    pc: usize,
    proto: usize,
    depth: usize,
    slots: Vec<Val>,
    tabs: Vec<Vec<Val>>,
    cups: Vec<Vec<Val>>,
    /// Shared upvalue cells (P3d): per closure and upvalue the cell (+ 1) of its REAL `UpVal`, and the cells' values.
    ucell: Vec<Vec<u32>>,
    cells: Vec<Val>,
    /// Prototype address of each closure (handle - 1).
    cproto: Vec<u64>,
    frames: Vec<FrameObs>,
    /// F4: `thread.top` relative to the root base.
    top: i64,
}

#[derive(Default)]
struct LogState {
    token: u64,
    head: usize,
    started: bool,
    limit: usize,
    extent: usize,
    root_ci: usize,
    root_func: usize,
    steps: Vec<Step>,
    tmap: HashMap<usize, u64>,
    tabs: Vec<TableRef>,
    cmap: HashMap<usize, u64>,
    cls: Vec<Gc<LClosure>>,
}

#[derive(Default)]
struct Last {
    low: Option<Rc<Lowered>>,
    head: u32,
    nslots: usize,
    token: u64,
    root_func: usize,
}

/// The sink of the dependency hooks: what the recorder asked, and when.
#[derive(Default)]
struct Sink {
    /// `(callee proto address, recorder step number)` per `register` call.
    registered: Vec<(usize, usize)>,
    /// `register` fails from its n-th call on (0-based).
    refuse_from: Option<usize>,
    /// `alive` turns false after this many polls.
    alive_polls: Option<usize>,
    polls: usize,
}

thread_local! {
    static MODE: Cell<Mode> = const { Cell::new(Mode::Record) };
    static CFG: RefCell<RecCfg> = RefCell::new(RecCfg::default());
    static LAST: RefCell<Last> = RefCell::new(Last::default());
    static LOG: RefCell<LogState> = RefCell::new(LogState::default());
    /// Every proto the recorder was stepped in: its tier-1 block structure and its code.
    static LOWS: RefCell<HashMap<usize, Rc<Lowered>>> = RefCell::new(HashMap::new());
    static CODES: RefCell<HashMap<usize, Vec<u32>>> = RefCell::new(HashMap::new());
    static MAXSTACKS: RefCell<HashMap<usize, usize>> = RefCell::new(HashMap::new());
    /// `arm()` calls so far (only the first one arms).
    static ARMS: Cell<usize> = const { Cell::new(0) };
    /// F2a: the types the next `side()` recording is told (the parent's exit snapshot), and whether the heads it reaches
    /// have a trace.
    static SIDE_KNOWN: RefCell<Vec<(u16, Ty)>> = const { RefCell::new(Vec::new()) };
    static SIDE_TRACED: Cell<bool> = const { Cell::new(true) };
    static NEST_ROOT: Cell<bool> = const { Cell::new(false) };
    /// F2a: `side()` arms this many instructions after the one that follows the call (the recorder waits for it).
    static SIDE_SKIP: Cell<u32> = const { Cell::new(0) };
    /// F2a: `SideCfg::min_len` of the next `side()` recording.
    static SIDE_MIN: Cell<u32> = const { Cell::new(0) };
    static SINK: RefCell<Sink> = RefCell::new(Sink::default());
    /// `(step number, root-relative slot, value)`: overwrite a slot right before the recorder's n-th step.
    static TAMPER: Cell<Option<(usize, usize, Value)>> = const { Cell::new(None) };
    static STEPNO: Cell<usize> = const { Cell::new(0) };
    static ABORTS: RefCell<Vec<Abort>> = const { RefCell::new(Vec::new()) };
    /// Forget the VM's recording context right after the recorder's first step in a callee frame (a lost abort).
    static FORGET_IN_CALLEE: Cell<bool> = const { Cell::new(false) };
    static STASH: Cell<RecCtx> = const { Cell::new(RecCtx { th: 0, root: 0, root_func: 0, root_cl: 0, cur: 0, enter: false, g: 0, token: 0 }) };
}

fn d_step(st: &mut State, ci: Ci, pc: *const u32, token: u64) -> RecStep {
    if MODE.get() == Mode::Log {
        return log_step(st, ci, pc, token);
    }
    // step numbers count from the loop head (the first step the recorder records)
    if let Some(cl) = st.ci_lclosure(ci) {
        let p = cl.proto();
        let key = &*p as *const Proto as usize;
        LOWS.with(|l| {
            l.borrow_mut()
                .entry(key)
                .or_insert_with(|| Rc::new(record::lower_for_landing(&p).expect("lowers")));
        });
        CODES.with(|c| {
            c.borrow_mut()
                .entry(key)
                .or_insert_with(|| p.code().to_vec());
        });
        MAXSTACKS.with(|c| {
            c.borrow_mut().insert(key, usize::from(p.maxstacksize));
        });
    }
    let idx = pc_index(st, ci, pc);
    if STEPNO.get() == usize::MAX {
        let is_root = ci.r().func.0 == LAST.with(|l| l.borrow().root_func);
        if !is_root || idx != LAST.with(|l| l.borrow().head) as usize {
            return record::step(st, ci, pc, token);
        }
        STEPNO.set(0);
    }
    let n = STEPNO.get();
    STEPNO.set(n + 1);
    if let Some((at, slot, v)) = TAMPER.get() {
        if at == n {
            let root = LAST.with(|l| l.borrow().root_func);
            st.set(StkId(root + 1 + slot), v);
        }
    }
    let out = record::step(st, ci, pc, token);
    if FORGET_IN_CALLEE.get() && ci.r().func.0 != LAST.with(|l| l.borrow().root_func) {
        FORGET_IN_CALLEE.set(false);
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

fn vval(l: &mut LogState, v: &Value) -> Val {
    match record::ty_of_value(v) {
        None => Val {
            t: Ty::Udata,
            bits: 0xDEAD,
        },
        Some(Ty::Tab) => {
            let t = v.as_table().unwrap();
            let key = t.as_ptr() as usize;
            let h = *l.tmap.entry(key).or_insert_with(|| {
                l.tabs.push(t);
                l.tabs.len() as u64
            });
            Val {
                t: Ty::Tab,
                bits: h,
            }
        }
        Some(Ty::Lcf) => {
            let c = v.as_lclosure().unwrap();
            let key = c.as_ptr() as usize;
            let h = *l.cmap.entry(key).or_insert_with(|| {
                l.cls.push(c);
                l.cls.len() as u64
            });
            Val {
                t: Ty::Lcf,
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
                Ty::Lfn => v.as_native().map_or(0, |f| f as usize as u64),
                _ => v.as_gc().map_or(0, |p| p as usize as u64),
            },
        },
    }
}

/// Registers everything reachable from the registered tables and closures (breadth first).
fn reach(l: &mut LogState) {
    let (mut ti, mut ci) = (0, 0);
    loop {
        let mut progressed = false;
        while ti < l.tabs.len() {
            let t = l.tabs[ti];
            ti += 1;
            progressed = true;
            for k in 1..=t.array_size() as i64 {
                if let Some(e) = t.get_int_arr(k) {
                    vval(l, &e);
                }
            }
        }
        while ci < l.cls.len() {
            let c = l.cls[ci];
            ci += 1;
            progressed = true;
            for u in 0..c.nupvalues() {
                vval(l, &c.upval(u).get());
            }
        }
        if !progressed {
            break;
        }
    }
}

fn log_step(st: &mut State, ci: Ci, pc: *const u32, token: u64) -> RecStep {
    let idx = pc_index(st, ci, pc);
    let cl = st.ci_lclosure(ci).unwrap();
    let p = cl.proto();
    let raw = Instruction(p.code()[idx]).opcode_raw();
    let is_ret = [
        OpCode::Return as u8,
        OpCode::Return0 as u8,
        OpCode::Return1 as u8,
    ]
    .contains(&raw);
    LOG.with(|l| {
        let mut l = l.borrow_mut();
        if l.token != token {
            return RecStep::Leave;
        }
        // the chain of frames, innermost first, up to the root
        let mut chain = vec![ci];
        while chain.last().unwrap().as_ptr() as usize != l.root_ci {
            match chain.last().unwrap().previous() {
                Some(c) => chain.push(c),
                None => return RecStep::Leave,
            }
        }
        let depth = chain.len() - 1;
        let root_base = l.root_func + 1;
        if !l.started {
            if depth != 0 || idx != l.head {
                return RecStep::Keep;
            }
            l.started = true;
            vval(&mut l, &Value::lclosure(cl)); // the root closure is handle 1
            for n in 0..l.extent {
                vval(&mut l, &st.get(StkId(root_base + n)));
            }
            reach(&mut l);
        }
        if l.steps.len() >= l.limit || (is_ret && depth == 0) {
            return RecStep::Leave;
        }
        let slots: Vec<Val> = (0..l.extent)
            .map(|n| {
                if root_base + n < st.stack_size() {
                    vval(&mut l, &st.get(StkId(root_base + n)))
                } else {
                    Val::NIL
                }
            })
            .collect();
        reach(&mut l);
        let cups: Vec<Vec<Val>> = l
            .cls
            .clone()
            .iter()
            .map(|c| {
                (0..c.nupvalues())
                    .map(|u| vval(&mut l, &c.upval(u).get()))
                    .collect()
            })
            .collect();
        let mut ukeys: HashMap<usize, u32> = HashMap::new();
        let mut cells: Vec<Val> = Vec::new();
        let ucell: Vec<Vec<u32>> = l
            .cls
            .clone()
            .iter()
            .map(|c| {
                (0..c.nupvalues())
                    .map(|u| {
                        let uv = c.upval(u);
                        let key = uv.as_ptr() as usize;
                        *ukeys.entry(key).or_insert_with(|| {
                            cells.push(vval(&mut l, &uv.get()));
                            cells.len() as u32
                        })
                    })
                    .collect()
            })
            .collect();
        let cproto: Vec<u64> = l
            .cls
            .iter()
            .map(|c| &*c.proto() as *const Proto as u64)
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
        let frames: Vec<FrameObs> = chain
            .iter()
            .rev()
            .map(|&c| {
                let fcl = st.ci_lclosure(c).unwrap();
                let fp = fcl.proto();
                let here = c.as_ptr() == ci.as_ptr();
                let pcx = if here {
                    idx
                } else {
                    (c.r().savedpc as usize - fp.code_ptr() as usize) / 4
                };
                FrameObs {
                    proto: &*fp as *const Proto as usize,
                    func_rel: c.r().func.0 as i64 - l.root_func as i64,
                    top_rel: c.r().top.0 as i64 - l.root_func as i64,
                    nres: (c.r().callstatus & CIST_NRESULTS) as i32 - 1,
                    pc: pcx,
                    maxstack: usize::from(fp.maxstacksize),
                    tail: c.r().callstatus & CIST_TAIL != 0,
                    nextra: if fp.is_vararg() { c.r().nextraargs } else { -1 },
                }
            })
            .collect();
        l.steps.push(Step {
            pc: idx,
            proto: &*p as *const Proto as usize,
            depth,
            slots,
            tabs,
            cups,
            ucell,
            cells,
            cproto,
            frames,
            top: st.top().0 as i64 - root_base as i64,
        });
        RecStep::Keep
    })
}

/// `arm()`: record (or, in log mode, log) the loop that follows in the calling function.
fn n_arm(st: &mut State) -> LuaResult<usize> {
    if ARMS.get() > 0 && MODE.get() == Mode::Record {
        return Ok(0); // a nested activation reaching `arm()` again: only the first call arms
    }
    ARMS.set(ARMS.get() + 1);
    let log = MODE.get() == Mode::Log;
    let caller = st.ci().previous().expect("caller");
    let cl = st.ci_lclosure(caller).unwrap();
    let p = cl.proto();
    let low = Rc::new(record::lower_for_landing(&p).expect("lowers"));
    let pc = pc_index(st, caller, caller.r().savedpc);
    let head = record::first_head_after(&low, pc as u32).expect("a loop follows arm()");
    let root_func = caller.r().func.0;
    LAST.with(|l| {
        let mut l = l.borrow_mut();
        l.low = Some(low.clone());
        l.head = head;
        l.nslots = usize::from(p.maxstacksize);
        l.root_func = root_func;
    });
    STEPNO.set(usize::MAX);
    if log {
        let tok = jit_hooks::rec_arm(st, caller).expect("armed");
        LOG.with(|l| {
            let mut l = l.borrow_mut();
            let (limit, extent) = (l.limit, l.extent);
            *l = LogState {
                token: tok,
                head: head as usize,
                limit,
                extent,
                root_ci: caller.as_ptr() as usize,
                root_func,
                ..LogState::default()
            };
        });
    } else {
        let mut cfg = CFG.with(|c| c.borrow().clone());
        if NEST_ROOT.get() {
            // F2b: every other loop head of the function has a root trace
            let heads: Vec<u32> = low.backedges.iter().map(|&(_, h)| h).collect();
            cfg.nest = Some(Rc::new(move |_st, pc| heads.contains(&pc).then_some(true)));
        }
        cfg.landing = Some(low);
        cfg.wait_head = true;
        let tok = record::arm(st, caller, head, cfg).expect("recorder armed");
        LAST.with(|l| l.borrow_mut().token = tok);
    }
    Ok(0)
}

/// `side()`: record a SIDE trace (F2a) from the instruction after this call to the next loop head of the calling
/// function (a head that has a trace as far as `SIDE_TRACED` says).
fn n_side(st: &mut State) -> LuaResult<usize> {
    if ARMS.get() > 0 {
        return Ok(0);
    }
    ARMS.set(ARMS.get() + 1);
    let caller = st.ci().previous().expect("caller");
    let cl = st.ci_lclosure(caller).unwrap();
    let p = cl.proto();
    let low = Rc::new(record::lower_for_landing(&p).expect("lowers"));
    let pc = pc_index(st, caller, caller.r().savedpc) as u32 + SIDE_SKIP.get();
    let root_func = caller.r().func.0;
    LAST.with(|l| {
        let mut l = l.borrow_mut();
        l.low = Some(low.clone());
        l.head = pc;
        l.nslots = usize::from(p.maxstacksize);
        l.root_func = root_func;
    });
    STEPNO.set(usize::MAX);
    let mut cfg = CFG.with(|c| c.borrow().clone());
    cfg.landing = Some(low.clone());
    cfg.wait_head = true; // the recorder skips the steps before the start pc
    let heads: Vec<u32> = low.backedges.iter().map(|&(_, h)| h).collect();
    cfg.side = Some(record::SideCfg {
        known: SIDE_KNOWN.with(|k| k.borrow().clone()),
        head_state: Rc::new(move |_st, pc| heads.contains(&pc).then(|| SIDE_TRACED.get())),
        min_len: SIDE_MIN.get(),
        frames: Vec::new(),
        max_extent: 0,
        max_ci: 0,
        frame_min: 0,
    });
    let tok = record::arm(st, caller, pc, cfg).expect("recorder armed");
    LAST.with(|l| l.borrow_mut().token = tok);
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
        jit_hooks::set_intrinsics(rn_std::lib_math::jit_builtins());
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
    FORGET_IN_CALLEE.set(false);
    STEPNO.set(usize::MAX);
    ABORTS.with(|a| a.borrow_mut().clear());
    LOWS.with(|l| l.borrow_mut().clear());
    CODES.with(|l| l.borrow_mut().clear());
    MAXSTACKS.with(|l| l.borrow_mut().clear());
    ARMS.set(0);
}

fn new_state() -> LuaState {
    init();
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
            ("stale", Some(n_stale)),
            ("side", Some(n_side)),
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
        Status::Ok,
        "load: {src}"
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

/// A finished recording with what the tests need to check it.
struct Rec {
    out: RecOutcome,
    result: String,
    lows: HashMap<usize, Rc<Lowered>>,
    codes: HashMap<usize, Vec<u32>>,
    maxstacks: HashMap<usize, usize>,
}

/// Records the loop that follows `arm()` in `call` after running `defs` once. Returns the state when it is still
/// usable (the recording ended before the state closed).
fn record_chunks(defs: &str, call: &str, cfg: RecCfg) -> (Option<LuaState>, Rec) {
    reset(cfg);
    let mut st = new_state();
    let _ = run(&mut st, defs);
    let result = run(&mut st, call);
    let (st, out) = if record::is_recording() {
        drop(st); // armed but never finished: the state closes the recording (Abort::Close)
        (None, record::take_outcome())
    } else {
        (Some(st), record::take_outcome())
    };
    let (tok, out) = out.expect("the recording ended with an outcome");
    assert_eq!(tok, LAST.with(|l| l.borrow().token));
    assert!(!record::is_recording());
    let rec = Rec {
        out,
        result,
        lows: LOWS.with(|l| l.borrow().clone()),
        codes: CODES.with(|l| l.borrow().clone()),
        maxstacks: MAXSTACKS.with(|l| l.borrow().clone()),
    };
    (st, rec)
}

fn record_src(src: &str, cfg: RecCfg) -> Rec {
    record_chunks("", &src.replace("ARM()", "arm()"), cfg).1
}

fn closed(r: &Rec) -> &Recorded {
    match &r.out {
        RecOutcome::Closed(c) => c,
        RecOutcome::Aborted(a) => {
            panic!("expected a closed recording, aborted: {a:?} ({})", r.result)
        }
    }
}

fn aborted(src: &str) -> AbortReason {
    aborted_cfg(src, RecCfg::default())
}

fn aborted_cfg(src: &str, cfg: RecCfg) -> AbortReason {
    match record_src(src, cfg).out {
        RecOutcome::Aborted(a) => a,
        RecOutcome::Closed(_) => panic!("expected an abort: {src}"),
    }
}

/// The real interpreter's states over the loop that follows `arm()` in the session's `f(args)`.
struct LogOut {
    steps: Vec<Step>,
}

fn log_run(st: &mut LuaState, call: &str, limit: usize, extent: usize) -> LogOut {
    record::discard();
    jit_hooks::rec_set_ctx(RecCtx::default());
    MODE.set(Mode::Log);
    ARMS.set(0);
    LOG.with(|l| {
        let mut l = l.borrow_mut();
        l.limit = limit;
        l.extent = extent;
    });
    let _ = run(st, call);
    MODE.set(Mode::Record);
    jit_hooks::rec_set_ctx(RecCtx::default());
    LOG.with(|l| {
        let mut l = std::mem::take(&mut *l.borrow_mut());
        LogOut {
            steps: std::mem::take(&mut l.steps),
        }
    })
}

// ---- the programs ---------------------------------------------------------------------------------------------

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

fn rn(r: &mut Rng, lo: u64, hi: u64) -> String {
    (lo + r.below(hi - lo + 1)).to_string()
}

/// A mild float: finite, small.
fn rm(r: &mut Rng) -> String {
    lit_f(r.below(2000) as f64 / 8.0 - 100.0)
}

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

/// Floats that hit the NaN / signed-zero / infinity corners of `min` and `max` (TR11, J4 M4).
fn special(r: &mut Rng) -> &'static str {
    r.pick(&[
        "(0/0)", "0.0", "(-0.0)", "1.5", "(-2.5)", "(1/0)", "(-1/0)", "2.5",
    ])
}

type ArgGen = fn(&mut Rng, u8) -> String;

struct Prog {
    name: &'static str,
    /// Global definitions run once; `ARM()` stands for the recording point inside `f`.
    defs: &'static str,
    args: ArgGen,
}

const PROGS: &[Prog] = &[
    Prog {
        name: "inline_leaf",
        defs: "local function leaf(a, b) local t = a * b return t + a - b end
               function f(n) local s = 0 ARM() for i = 1, n do s = s + leaf(i, 3) end return s end",
        args: |r, _| rn(r, 3, 12),
    },
    Prog {
        name: "inline_2",
        defs: "local function leaf(a, b) local t = a * b return t + a - b end
               local function mid(x) local y = leaf(x, 0.5) return y * 0.25 + 1.0 end
               function f(n) local s = 0.0 ARM() for i = 1, n do s = s + mid(i + 0.0) end return s end",
        args: |r, _| rn(r, 3, 12),
    },
    Prog {
        name: "spectral_a",
        defs: "local function A(i, j) local ij = i + j - 1; return 1.0 / (ij * (ij - 1) * 0.5 + i) end
               function f(x, N) local a = 0.0 ARM() for j = 1, N do a = a + x[j] * A(2, j) end return a end",
        args: |r, v| format!("{}, 8", rtab(r, 8, v, false)),
    },
    Prog {
        name: "math_intrinsics",
        defs: "local sqrt, abs, floor, ceil, min, max = math.sqrt, math.abs, math.floor, math.ceil, math.min, math.max
               function f(n, a)
                 local s = 0.0
                 ARM()
                 for i = 1, n do
                   s = s + sqrt(a * a + i) + abs(a - i) + floor(a * 0.5) + ceil(a) + min(a, s) + max(i, 3) + abs(i - 5)
                 end
                 return s
               end",
        args: |r, v| format!("{}, {}", rn(r, 3, 12), if v == 1 { rn(r, 1, 6) } else { rm(r) }),
    },
    Prog {
        name: "minmax_special",
        defs: "local min, max = math.min, math.max
               function f(n, a, b)
                 local s, t = 0.0, 0.0
                 ARM()
                 for i = 1, n do local x = min(a, b) local y = max(a, b) s = x t = y end
                 return s + t
               end",
        args: |r, v| {
            let (a, b) = (special(r), special(r));
            format!("{}, {}, {}", rn(r, 3, 6), if v == 1 { "3" } else { a }, b)
        },
    },
    Prog {
        name: "floorceil_special",
        defs: "local floor, ceil, abs, min, max = math.floor, math.ceil, math.abs, math.min, math.max
               function f(n, x, c)
                 local s = 0
                 ARM()
                 for i = 1, n do s = s + floor(x) + ceil(x) + abs(c) + min(c, 3) + max(c, 3) end
                 return s
               end",
        args: |r, v| {
            let x = r.pick(&["0.5", "(-0.5)", "2.5", "(-2.5)", "(-0.0)", "3.0", "1e15", "(-1e15)", "7.25"]);
            let c = r.pick(&["math.mininteger", "(-5)", "0", "7", "3"]);
            format!("{}, {}, {}", rn(r, 3, 6), if v == 1 { "4" } else { x }, if v == 1 { "2.5" } else { c })
        },
    },
    Prog {
        name: "upval_callee",
        defs: "local k1, k2 = 3, 4.5
               local function g(x) return x * k1 + k2 end
               function f(n) local s = 0.0 ARM() for i = 1, n do s = s + g(i) end return s end",
        args: |r, _| rn(r, 3, 12),
    },
    Prog {
        name: "early_return",
        defs: "local function g(x, c) if x > c then return x * 2 end return x + 1 end
               function f(n, c) local s = 0 ARM() for i = 1, n do s = s + g(i, c) end return s end",
        args: |r, _| format!("{}, {}", rn(r, 5, 12), rn(r, 0, 4)),
    },
    Prog {
        name: "popped_temps",
        defs: "local function put(t, i, v) local a = v * 2 local b = a + t[i] t[i] = b end
               function f(t, n) ARM() for i = 1, n do put(t, i, i + 1) end return t[n] end",
        args: |r, v| format!("{}, 6", rtab(r, 6, v, true)),
    },
    Prog {
        name: "nil_fill_results",
        defs: "local function g(x, y) if y then return x end return x, x + 1 end
               function f(n) local s = 0 ARM() for i = 1, n do local a, b, c = g(i) s = s + a + b + (c or 0) end return s end",
        args: |r, _| rn(r, 3, 12),
    },
    Prog {
        name: "depth4",
        defs: "local function d4(x) return x + 1 end
               local function d3(x) return d4(x) * 2 end
               local function d2(x) return d3(x) - 1 end
               local function d1(x) return d2(x) + 3 end
               function f(n) local s = 0 ARM() for i = 1, n do s = s + d1(i) end return s end",
        args: |r, _| rn(r, 3, 12),
    },
    Prog {
        name: "store_then_call",
        defs: "local acc = 0
               local function add1(x) return x + 1 end
               function f(n) ARM() for i = 1, n do acc = acc + 1 acc = acc + add1(i) end return acc end",
        args: |r, _| rn(r, 3, 12),
    },
    Prog {
        name: "sibling_calls",
        defs: "local function sq(x) return x * x end
               local function neg(x) return -x end
               function f(n) local s = 0 ARM() for i = 1, n do local q = sq(i + 1) s = s + sq(i) + neg(q) end return s end",
        args: |r, _| rn(r, 3, 12),
    },
    // P3d: closures of ONE prototype with different upvalue cells, called alternately (item 1 of the P3b review)
    Prog {
        name: "mk_closures_upvals",
        defs: "local function mk(v) return function(x) return x + v end end
               local ca, cb = mk(10), mk(1000)
               function f(n) local s = 0 ARM() for i = 1, n do s = s + ca(i) + cb(i) end return s end",
        args: |r, _| rn(r, 3, 12),
    },
    Prog {
        name: "mk_closures_array",
        defs: "local function mk(v) return function(x) return x * v end end
               local c = { mk(3), mk(5), mk(7) }
               function f(n) local s = 0 ARM() for i = 1, n do s = s + c[1](i) + c[3](i) + c[2](i) end return s end",
        args: |r, _| rn(r, 3, 12),
    },
    // P3d: shared upvalues (UpvalAlias is gone)
    Prog {
        name: "acc_bump",
        defs: "local u = 1
               local function bump(x) u = u + x return u end
               function f(n) ARM() for i = 1, n do u = u + bump(i) end return u end",
        args: |r, _| rn(r, 3, 8),
    },
    Prog {
        name: "shared_pair",
        defs: "local inc, get
               do local c = 0
                  inc = function() c = c + 1 return c end
                  get = function() return c end end
               function f(n) local s = 0 ARM() for i = 1, n do inc() s = s + get() + inc() end return s end",
        args: |r, _| rn(r, 3, 12),
    },
    Prog {
        name: "rng_seed",
        defs: "local seed = 12345
               local function rnd() seed = (seed * 1103515245 + 12345) % 2147483648 return seed end
               local function peek() return seed end
               function f(n) local s = 0 ARM() for i = 1, n do s = s + rnd() % 7 + peek() % 3 end return s end",
        args: |r, _| rn(r, 3, 12),
    },
    Prog {
        name: "shared_read",
        defs: "local u = 5
               local function rd(x) return u + x end
               function f(n) local s = 0 ARM() for i = 1, n do s = s + u + rd(i) end return s end",
        args: |r, _| rn(r, 3, 12),
    },
    // F4: TAILCALL, multiple results with an explicit `top`, recursion within the unroll limit
    Prog {
        name: "tail_chain",
        defs: "local function leaf(x) if x > 5 then return x * 2 end return x + 1 end
               local function mid(x) return leaf(x + 1) end
               function f(n) local s = 0 ARM() for i = 1, n do s = s + mid(i) end return s end",
        args: |r, _| rn(r, 3, 12),
    },
    Prog {
        name: "tail_missing_args",
        defs: "local function t3(a, b, c) if c == nil then return a + b end return a + b + c end
               local function m(a) return t3(a, 1) end
               local function m2(a) return t3(a, 1, 2) end
               function f(n) local s = 0 ARM() for i = 1, n do s = s + m(i) + m2(i) end return s end",
        args: |r, _| rn(r, 3, 12),
    },
    Prog {
        name: "tail_two_deep",
        defs: "local function c(x) if x % 3 == 0 then return x end return x + 100 end
               local function b(x) return c(x * 2) end
               local function a(x) local t = x + 1 return b(t) end
               function f(n) local s = 0 ARM() for i = 1, n do s = s + a(i) + b(i) end return s end",
        args: |r, _| rn(r, 3, 12),
    },
    Prog {
        name: "tail_intrinsic",
        defs: "local max, sqrt = math.max, math.sqrt
               local function pick(a, b) return max(a, b) end
               local function root(a) return sqrt(a) end
               function f(n) local s = 0.0 ARM() for i = 1, n do s = s + pick(i, 5) + pick(2, i) + root(i) end return s end",
        args: |r, _| rn(r, 3, 12),
    },
    Prog {
        name: "multret_chain",
        defs: "local function two(a) return a, a + 1 end
               local function sum(a, b) return a + b end
               local function pass(a) return 0, two(a) end
               function f(n) local s = 0 ARM() for i = 1, n do
                 s = s + sum(two(i)) + sum(pass(i))
                 local p, q, r = pass(i)
                 s = s + r
               end return s end",
        args: |r, _| rn(r, 3, 12),
    },
    Prog {
        name: "multret_intrinsic2",
        defs: "local max, min = math.max, math.min
               local function two(a) return a, 7 end
               function f(n) local s = 0 ARM() for i = 1, n do s = s + max(two(i)) + min(two(i)) end return s end",
        args: |r, _| rn(r, 3, 12),
    },
    Prog {
        name: "rec_within_limit",
        defs: "local function depth(n) if n == 0 then return 0 end return 1 + depth(n - 1) end
               function f(n) local s = 0 ARM() for i = 1, n do s = s + depth(i % 2) end return s end",
        args: |r, _| rn(r, 3, 12),
    },
    Prog {
        name: "rec_two_args",
        defs: "local function ack(m, n)
                 if m == 0 then return n + 1 end
                 if n == 0 then return ack(m - 1, 1) end
                 return ack(m - 1, ack(m, n - 1))
               end
               function f(n) local s = 0 ARM() for i = 1, n do s = s + ack(1, i % 2) end return s end",
        args: |r, _| rn(r, 3, 12),
    },
];

fn prog(name: &str) -> &'static Prog {
    PROGS.iter().find(|p| p.name == name).expect("program")
}

fn rec_args(p: &Prog, seed: u64) -> String {
    (p.args)(&mut Rng::new(seed), 0)
}

fn strict() -> RecCfg {
    RecCfg {
        verify: true,
        unroll: 2,
        loop_unroll: 16,
        ..RecCfg::default()
    }
}

fn record_prog(p: &Prog, seed: u64, cfg: RecCfg) -> (Option<LuaState>, Rec) {
    record_chunks(
        &p.defs.replace("ARM()", "arm()"),
        &format!("return f({})", rec_args(p, seed)),
        cfg,
    )
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

/// The golden text of a closed recording: the IR dump, snapshots with their frames and `FUNC` slots, the loop data.
fn golden_text(rc: &Rec) -> String {
    let r = closed(rc);
    // protos are named by first appearance on the path
    let mut names: Vec<usize> = Vec::new();
    let mut pn = |k: usize| -> String {
        let i = names.iter().position(|&x| x == k).unwrap_or_else(|| {
            names.push(k);
            names.len() - 1
        });
        format!("P{i}")
    };
    let path: Vec<String> = (0..r.path.len())
        .map(|i| format!("{}:{}", pn(r.path_proto[i].0), r.path[i]))
        .collect();
    let mut s = dump(&r.ir);
    for (i, sn) in r.ir.snaps.iter().enumerate() {
        let links: Vec<String> = snap::frame_links(&r.ir, i as u32)
            .iter()
            .map(|l| {
                format!(
                    "{}@f{}:c{}:n{}",
                    pn(l.proto.0),
                    l.func_slot,
                    l.call_pc,
                    l.nresults
                )
            })
            .collect();
        let func: Vec<String> =
            r.ir.snap_entries(i as u32)
                .iter()
                .filter(|e| e.flags & SNAP_FUNC != 0)
                .map(|e| e.slot.to_string())
                .collect();
        s.push_str(&format!(
            "S{i} land={:?} step={} frames=[{}] func=[{}]\n",
            sn.land,
            r.snap_step[i],
            links.join(" "),
            func.join(" ")
        ));
    }
    s.push_str(&format!(
        "counts={:?} extent={} depth={} room={} entry_snap={:?} head={} loop_at={:?} n={} deps={}\n",
        r.ir.counts,
        r.ir.extent,
        r.ir.depth,
        r.room,
        r.entry_snap,
        r.ir.head.1,
        r.ir.loop_at,
        r.n,
        r.ir.deps.len()
    ));
    s.push_str(&format!("path=[{}]\n", path.join(" ")));
    let wb: Vec<String> = r
        .writeback
        .iter()
        .map(|e| format!("{}=%{}", e.slot, e.r.wrapping_sub(REF_BIAS)))
        .collect();
    s.push_str(&format!("writeback=[{}]\n", wb.join(" ")));
    normalise(&s)
}

/// The golden dumps of the 5.5 build; the 5.4 build has its own (`_54`, TR30).
#[cfg(not(rustynail_lua54))]
const GOLDEN_FILE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/tr_record_golden_calls.txt"
);
#[cfg(rustynail_lua54)]
const GOLDEN_FILE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/tr_record_golden_calls_54.txt"
);

/// The programs of the golden file (the others are covered by the replay and structure tests).
const GOLDEN: &[&str] = &[
    "inline_leaf",
    "inline_2",
    "spectral_a",
    "math_intrinsics",
    "upval_callee",
    "early_return",
    "popped_temps",
    "nil_fill_results",
    "mk_closures_upvals",
    "acc_bump",
    "shared_pair",
    "rng_seed",
];

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
    for name in GOLDEN {
        let (_, r) = record_prog(prog(name), 1, strict());
        actual.push((name.to_string(), golden_text(&r)));
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

// ---- IR replay against the real interpreter -----------------------------------------------------------------------

thread_local! {
    /// The IR under replay contains a `CALLN`: its NaNs are canonicalised by `interp`, the VM's are libm's, so NaN
    /// payloads are not compared (the recorder's lockstep tracks this per value; everything else is exact).
    static NAN_LENIENT: Cell<bool> = const { Cell::new(false) };
}

fn val_eq(a: Val, b: Val) -> bool {
    // collectables other than tables are compared by type only (the logged run registers its handles in its own
    // order; the recorder's lockstep compares addresses within one state)
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

thread_local! {
    /// Self-test knobs of the replay: model every `(closure, index)` pair as its own cell (the pre-P3d model).
    static NO_SHARE: Cell<bool> = const { Cell::new(false) };
}

fn model_from(s: &Step) -> Model {
    Model {
        // every real closure keeps its own handle; `EQREF closure, KPTR(proto)` compares the closure's prototype
        cl_proto: s.cproto.clone(),
        uv_cell: if NO_SHARE.get() {
            Vec::new()
        } else {
            s.ucell.clone()
        },
        cells: s.cells.clone(),
        slots: s.slots.clone(),
        tables: s
            .tabs
            .iter()
            .map(|a| MTable {
                array: a.clone(),
                metatable: 0,
                flags: 0,
                lsizenode: 0,
                lenhint: 0,
                nodes: Default::default(),
            })
            .collect(),
        closures: s.cups.clone(),
        ..Model::new()
    }
}

/// `SLOAD FUNC_SLOT` becomes the root closure (handle 1). The callee-identity `EQREF closure, KPTR(proto)` is NOT
/// rewritten (P3d): every real closure has its own handle and the model compares the closure's prototype
/// (`Model::cl_proto`), so closures of one prototype with different upvalue cells stay distinct.
fn rewrite_ir(ir: &TraceIr, _lg: &LogOut) -> TraceIr {
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
    done: usize,
    exits: usize,
    forced: usize,
    /// Exits whose snapshot has at least one inlined frame (natural and forced).
    in_frames: usize,
    exit_snaps: HashMap<u32, usize>,
}

/// The state after an exit: the stack at the start of the iteration overlaid with the snapshot's slots, the heap the
/// model holds, and the `CallInfo` chain the snapshot's frames describe against the VM's.
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
    if m.closures.len() != real.cups.len()
        || real.cups.iter().enumerate().any(|(h, b)| {
            let a: Vec<Val> = (0..b.len())
                .map(|u| m.upval(h as u64 + 1, u).expect("upvalue"))
                .collect();
            !vec_eq(&a, b)
        })
    {
        return Err(format!("{what}: upvalues differ"));
    }
    Ok(())
}

/// The frames a snapshot reconstructs (5.5) against the VM's `CallInfo` chain at the instruction it re-executes.
fn check_frames(what: &str, ir: &TraceIr, s: u32, real: &Step) -> Result<(), String> {
    let sn = ir.snaps[s as usize];
    let links = snap::frame_links(ir, s);
    if usize::from(sn.nframes) != real.depth || sn.frame != sn.nframes || links.len() != real.depth
    {
        return Err(format!(
            "{what}: snapshot has {} frames (frame {}), the VM runs at depth {}",
            sn.nframes, sn.frame, real.depth
        ));
    }
    if real.frames.len() != real.depth + 1 {
        return Err(format!("{what}: bad log"));
    }
    for (k, l) in links.iter().enumerate() {
        let f = &real.frames[k + 1];
        let caller = &real.frames[k];
        let top = l.func_slot as i64 + 2 + f.maxstack as i64; // both relative to the root function slot (slot 0 is +1)
        if f.proto != l.proto.0
            || f.func_rel != i64::from(l.func_slot) + 1
            || f.nres != i32::from(l.nresults)
            || f.top_rel != top
            || f.tail != l.tail
            || f.nextra != l.nextra
        {
            return Err(format!(
                "{what}: frame {} link {l:?} but the VM has {f:?}",
                k + 1
            ));
        }
        if caller.pc != l.call_pc as usize + 1 {
            return Err(format!(
                "{what}: caller of frame {} resumes at {} in the VM, link says call pc {}",
                k + 1,
                caller.pc,
                l.call_pc
            ));
        }
    }
    // F4: a snapshot that carries a `top` restores exactly the `thread.top` the interpreter holds before the instruction
    if sn.top != 0 && real.top != i64::from(sn.top) - 1 {
        return Err(format!("{what}: snapshot top {} but the VM's top is {}", i64::from(sn.top) - 1, real.top));
    }
    if real.frames[real.depth].pc != sn.pc as usize || real.pc != sn.pc as usize {
        return Err(format!(
            "{what}: snapshot pc {} but the VM is at pc {}",
            sn.pc, real.pc
        ));
    }
    Ok(())
}

/// Replays the recorded IR of `rc` on the real interpreter's log. Natural exits and complete iterations, then every
/// guard forced to fail in the first two iterations. Err = a mismatch.
fn replay(rc: &Rec, lg: &LogOut, st: &mut Stats) -> Result<(), String> {
    let r = closed(rc);
    let log = &lg.steps;
    let len = r.path.len();
    if log.is_empty() || log[0].pc != r.ir.head.1 as usize || log[0].depth != 0 {
        return Ok(());
    }
    let low_of = |step: usize| -> &Lowered { &rc.lows[&r.path_proto[step].0] };
    // the instruction counts the recorder claims, recomputed from the tier-1 block structures of every frame's proto
    let fetched = |upto: usize| -> u32 { (0..upto).map(|i| low_of(i).fetches(&[r.path[i]])).sum() };
    if r.n != fetched(len) || r.ir.counts != vec![r.n] {
        return Err("counts differ from the fetches of the path".into());
    }
    let ir = rewrite_ir(&r.ir, lg);
    NAN_LENIENT.set(r.ir.ins.iter().any(|i| i.op == TOp::Calln));
    let (ir_wb, probe) = with_probe(&ir, &r.writeback);
    let mut m = model_from(&log[0]);
    let b0 = m.budget;
    let step_of_snap = |s: u32| r.snap_step[s as usize] as usize;
    // budget a snapshot exit must store: tier 1's budget at the landing label of the snapshot's frame (TR5.4, TR5.5)
    let budget_check = |what: &str, mk: &Model, s: u32, e: usize| -> Result<(), String> {
        let sn = r.ir.snaps[s as usize];
        let low = low_of(step_of_snap(s));
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
        if stored != want {
            return Err(format!(
                "{what}: snapshot {s} (pc {}) stores budget {stored}, tier 1 holds {want}",
                sn.pc
            ));
        }
        Ok(())
    };
    let key_ir = |j: usize| {
        (
            r.path[j % len] as usize,
            r.path_proto[j % len].0,
            r.path_depth[j % len] as usize,
        )
    };
    for k in 0..3usize {
        let start = k * len;
        if start >= log.len() {
            break;
        }
        check_state("iteration start", &m.slots.clone(), &[], &m, &log[start])?;
        let follow = (start..log.len())
            .find(|&j| (log[j].pc, log[j].proto, log[j].depth) != key_ir(j))
            .unwrap_or(log.len());
        // 1. every guard forced to fail (first two iterations): the exit state is the VM's state before its instruction
        if k < 2 {
            for (g, ins) in ir_wb.ins.iter().enumerate() {
                if ins.flags & F_GUARD == 0 || aux_snap(ins.aux) as usize >= r.ir.snaps.len() {
                    continue;
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
                check_frames(&what, &r.ir, sg, &log[e])?;
                budget_check(&what, &mf, sg, e)?;
                if r.ir.snaps[sg as usize].nframes > 0 {
                    st.in_frames += 1;
                }
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
                    return Ok(());
                }
                if follow <= e {
                    return Err(format!("{what}: the IR exited at VM step {e}, after the VM left the path at step {follow}"));
                }
                check_state(&what, &m.slots, &slots, &mk, &log[e])?;
                check_frames(&what, &r.ir, snap, &log[e])?;
                budget_check(&what, &mk, snap, e)?;
                st.exits += 1;
                if r.ir.snaps[snap as usize].nframes > 0 {
                    st.in_frames += 1;
                }
                *st.exit_snaps.entry(snap).or_default() += 1;
                return Ok(());
            }
            Outcome::Exit { slots, .. } => {
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

/// Records `p` with `rec_seed` and replays it against `nlogs` real runs of the same state.
fn replay_prog(p: &Prog, rec_seed: u64, nlogs: u64, variants: &[u8], st: &mut Stats) {
    let (state, rc) = record_prog(p, rec_seed, strict());
    let RecOutcome::Closed(r) = &rc.out else {
        panic!(
            "{}: recording aborted: {:?} ({})",
            p.name, rc.out, rc.result
        );
    };
    let mut state = state.expect("the state outlives a closed recording");
    for variant in variants {
        for seed in 0..nlogs {
            let args = (p.args)(
                &mut Rng::new(1000 + seed * 7 + u64::from(*variant)),
                *variant,
            );
            let lg = log_run(
                &mut state,
                &format!("return f({args})"),
                4 * r.path.len() + 8,
                usize::from(r.ir.extent),
            );
            if let Err(e) = replay(&rc, &lg, st) {
                panic!(
                    "{} (recorded with seed {rec_seed}, replay args `{args}`, variant {variant}): {e}\n{}",
                    p.name,
                    golden_text(&rc)
                );
            }
        }
    }
}

/// P-E: inlined vararg callees (kept apart from [`PROGS`]: they have no golden IR text and `frames_along` models only
/// ordinary frames).
const VPROGS: &[Prog] = &[
    Prog {
        name: "vararg_pass",
        defs: "local function pass(...) return ... end
               function f(n) local s = 0 ARM() for i = 1, n do local a, b, c = pass(i, 3, i + 1) s = s + a + b + c end return s end",
        args: |r, _| rn(r, 3, 12),
    },
    Prog {
        name: "vararg_select",
        defs: "local select = select local function cnt(...) return select('#', ...) end
               local function nth(k, ...) return (select(k, ...)) end
               function f(n) local s = 0 ARM() for i = 1, n do s = s + cnt(i, i) + cnt() + nth(2, i, 4, 5) + nth(-1, 7, i) end return s end",
        args: |r, _| rn(r, 3, 12),
    },
    Prog {
        name: "vararg_selvar",
        defs: "local select = select local function nth(k, ...) return (select(k, ...)) end
               function f(n) local s = 0 ARM() for i = 1, n do s = s + nth(i % 3 + 1, i, 4, 5) end return s end",
        args: |r, _| rn(r, 3, 12),
    },
    Prog {
        name: "vararg_fixed",
        defs: "local function fx(a, b, ...) local c, d = ... return a + (b or 0) + (c or 0), d end
               function f(n) local s = 0 ARM() for i = 1, n do local x, y = fx(i, 2, 3) s = s + x + (y or 1) local z = fx(i) end return s end",
        args: |r, _| rn(r, 3, 12),
    },
    Prog {
        name: "vararg_loop",
        defs: "local select = select local function sum(...) local s = 0 for i = 1, select('#', ...) do s = s + (select(i, ...)) end return s end
               function f(n) local s = 0 ARM() for i = 1, n do s = s + sum(i, 2, 3) + sum(i) end return s end",
        args: |r, _| rn(r, 3, 12),
    },
    Prog {
        name: "vararg_types",
        defs: "local select = select local function g(a, ...) local x = a * 2 local n, v = select('#', ...), ... return x + n + v end
               function f(a, n) local s = 0 ARM() for i = 1, n do s = s + g(a, i, 7) end return s end",
        args: |r, v| format!("{}, {}", if v == 0 { rn(r, 3, 9) } else { rm(r) }, rn(r, 3, 12)),
    },
];

/// The replay of the recorded vararg iterations against the real interpreter: the frame links (shifted function slot,
/// `nextraargs`), the slots, the `top` after `VARARG C=0` and the results of `select`, at every natural and forced exit.
#[test]
fn vararg_replay_matches_the_interpreter() {
    let nlogs = if cfg!(miri) { 2 } else { 14 };
    for p in VPROGS {
        let mut st = Stats::default();
        for rec_seed in 1..=3 {
            replay_prog(p, rec_seed, nlogs, &[0, 1], &mut st);
        }
        eprintln!("{}: {st:?}", p.name);
        // (`vararg_selvar`: the selector changes every iteration, so every replayed iteration leaves the trace)
        assert!((st.done > 0 || p.name == "vararg_selvar") && st.forced > 0, "{}: nothing replayed: {st:?}", p.name);
        if ["vararg_selvar", "vararg_types"].contains(&p.name) {
            assert!(st.in_frames > 0, "{}: no exit inside an inlined vararg frame: {st:?}", p.name);
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
            st.in_frames += one.in_frames;
            *st.exit_snaps.entry(rec_seed as u32).or_default() += one.exit_snaps.len();
        }
        eprintln!("{}: {st:?}", p.name);
        assert!(
            st.done > 0 && st.forced > 0,
            "{}: nothing replayed: {st:?}",
            p.name
        );
        // the variant-1 runs leave the recorded types: the guards must have fired somewhere in every recording
        assert!(
            st.exits > 0 && st.exit_snaps.values().all(|&n| n > 0),
            "{}: no natural exit was replayed in some recording: {st:?}",
            p.name
        );
        // callees with guards of their own: exits (natural and forced) inside inlined frames were compared
        if [
            "early_return",
            "upval_callee",
            "popped_temps",
            "shared_read",
            "depth4",
        ]
        .contains(&p.name)
        {
            assert!(
                st.in_frames > 0,
                "{}: no exit inside an inlined frame: {st:?}",
                p.name
            );
        }
    }
}

/// The replay of the *recorded* iteration: the VM runs the recording's own arguments, so every guard of the first
/// iteration holds and the IR must complete it. A guard that fires here is over-strict (wrong polarity, stale type, a
/// callee identity guard that compares the wrong thing); `replay` accepts such an exit as a legal early exit, so this
/// is where it is noticed. Self-test: with any single guard forced to fail the check must notice.
#[test]
fn over_strict_guards_are_noticed() {
    for p in PROGS {
        let (state, rc) = record_prog(p, 1, strict());
        let r = closed(&rc);
        let mut state = state.expect("the state outlives a closed recording");
        let lg = log_run(
            &mut state,
            &format!("return f({})", rec_args(p, 1)),
            2 * r.path.len() + 4,
            usize::from(r.ir.extent),
        );
        if lg.steps.is_empty() || lg.steps[0].pc != r.ir.head.1 as usize || lg.steps[0].depth != 0 {
            continue;
        }
        let ir = rewrite_ir(&r.ir, &lg);
        NAN_LENIENT.set(r.ir.ins.iter().any(|i| i.op == TOp::Calln));
        let (ir_wb, probe) = with_probe(&ir, &r.writeback);
        let mut m = model_from(&lg.steps[0]);
        match interp::run(&ir_wb, &mut m).unwrap() {
            Outcome::Exit { snap, .. } if snap == probe => {}
            other => panic!(
                "{}: a guard fired on the recorded inputs (over-strict): {other:?}\n{}",
                p.name,
                golden_text(&rc)
            ),
        }
        let mut noticed = 0;
        for (g, ins) in ir_wb.ins.iter().enumerate() {
            if ins.flags & F_GUARD == 0 || aux_snap(ins.aux) as usize >= r.ir.snaps.len() {
                continue;
            }
            let mut m = model_from(&lg.steps[0]);
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

// ---- structure of the recorded IR -----------------------------------------------------------------------------------

/// The frame stack the path implies: per step, `(proto, call_pc, func_slot, nresults)` of frames `1..=depth`.
fn frames_along(rc: &Rec, r: &Recorded) -> Vec<Vec<(usize, u32, u16, i16)>> {
    let mut out = Vec::new();
    let mut stack: Vec<(usize, u32, u16, i16, usize)> = Vec::new(); // + base_off
    let mut base = 0usize;
    for i in 0..r.path.len() {
        let d = r.path_depth[i] as usize;
        if i > 0 {
            let prev = r.path_depth[i - 1] as usize;
            if d == prev + 1 {
                // the previous step was the CALL
                let code = &rc.codes[&r.path_proto[i - 1].0];
                let ins = Instruction(code[r.path[i - 1] as usize]);
                let func = base + ins.a() as usize;
                stack.push((
                    r.path_proto[i].0,
                    r.path[i - 1],
                    func as u16,
                    ins.c() as i16 - 1,
                    func + 1,
                ));
                base = func + 1;
            } else if d + 1 == prev {
                stack.pop();
                base = stack.last().map_or(0, |f| f.4);
            } else {
                assert_eq!(d, prev, "depth changes by at most one per step");
                // F4: a TAILCALL replaces the frame: the same function slot, the callee's prototype
                let code = &rc.codes[&r.path_proto[i - 1].0];
                let ins = Instruction(code[r.path[i - 1] as usize]);
                if ins.opcode() == Some(OpCode::Tailcall) {
                    stack.last_mut().expect("a tail-called frame").0 = r.path_proto[i].0;
                }
            }
        }
        assert_eq!(stack.len(), d);
        out.push(stack.iter().map(|f| (f.0, f.1, f.2, f.3)).collect());
    }
    out
}

#[test]
fn frames_and_snapshots_are_well_formed() {
    for p in PROGS {
        let (_, rc) = record_prog(p, 1, strict());
        let r = closed(&rc);
        let ir = &r.ir;
        let along = frames_along(&rc, r);
        // rolled layout, deps, extent
        assert_eq!(ir.loop_at, None, "{}", p.name);
        assert_eq!(ir.deps[0], ir.head.0);
        let mut seen: Vec<ProtoKey> = r.path_proto.clone();
        seen.dedup();
        for k in &seen {
            assert!(
                ir.deps.contains(k),
                "{}: {k:?} is on the path but not in deps",
                p.name
            );
        }
        assert_eq!(ir.deps.len(), {
            let mut v: Vec<_> = r.path_proto.iter().map(|k| k.0).collect();
            v.sort_unstable();
            v.dedup();
            v.len()
        });
        assert_eq!(
            usize::from(ir.depth),
            r.path_depth
                .iter()
                .map(|&d| usize::from(d))
                .max()
                .unwrap()
                .max(
                    // an intrinsic's native frame is one deeper than its caller
                    (0..r.path.len())
                        .filter(|&i| is_intrinsic_call(&rc, r, i))
                        .map(|i| usize::from(r.path_depth[i]) + 1)
                        .max()
                        .unwrap_or(0)
                ),
            "{}: TR6b CallInfo depth",
            p.name
        );
        // frames of snapshots: depth, links, FUNC entries
        assert_eq!(
            ir.frames.len(),
            ir.snaps
                .iter()
                .map(|s| usize::from(s.nframes))
                .sum::<usize>()
        );
        for (s, sn) in ir.snaps.iter().enumerate() {
            let step = r.snap_step[s] as usize;
            let want = &along[step];
            let links = snap::frame_links(ir, s as u32);
            assert_eq!(links.len(), want.len(), "{}: S{s}", p.name);
            assert_eq!(
                (sn.frame as usize, sn.nframes as usize),
                (want.len(), want.len())
            );
            for (l, w) in links.iter().zip(want) {
                assert_eq!(
                    (l.proto.0, l.call_pc, l.func_slot, l.nresults),
                    *w,
                    "{}: S{s}",
                    p.name
                );
                // the FUNC entry of the function slot is present and holds a closure
                let e = ir
                    .snap_entries(s as u32)
                    .iter()
                    .find(|e| e.slot == l.func_slot && e.flags & SNAP_FUNC != 0)
                    .unwrap_or_else(|| {
                        panic!(
                            "{}: S{s} lacks the FUNC entry of slot {}",
                            p.name, l.func_slot
                        )
                    });
                assert_eq!(ir.ty_of(e.r), Ty::Lcf, "{}: S{s}", p.name);
            }
            assert_eq!(
                ir.snap_entries(s as u32)
                    .iter()
                    .filter(|e| e.flags & SNAP_FUNC != 0)
                    .count(),
                links.len()
            );
            // the snapshot's pc is the pc of its step in the proto of that frame
            assert_eq!(sn.pc, r.path[step], "{}: S{s}", p.name);
        }
        // TR22b: no multi-value instruction on the path
        // (calls and returns are fixed-count, checked by the abort matrix)
    }
}

fn is_intrinsic_call(rc: &Rec, r: &Recorded, i: usize) -> bool {
    let code = &rc.codes[&r.path_proto[i].0];
    let ins = Instruction(code[r.path[i] as usize]);
    // a CALL whose next step stays in the same frame is a native call; F4: so is a TAILCALL whose next step is in the caller
    // (the native returns for the frame)
    (ins.opcode() == Some(OpCode::Call)
        && i + 1 < r.path.len()
        && r.path_depth[i + 1] <= r.path_depth[i]
        && r.path_proto[i + 1] == r.path_proto[i])
        || (ins.opcode() == Some(OpCode::Tailcall) && i + 1 < r.path.len() && r.path_depth[i + 1] < r.path_depth[i])
}

/// TR3: the slots of a popped frame stay in the snapshots (and in the writeback) until the iteration redefines them.
#[test]
fn popped_frame_slots_stay_in_snapshots() {
    let (_, rc) = record_prog(prog("popped_temps"), 1, strict());
    let r = closed(&rc);
    let ir = &r.ir;
    // `put(t, i, v)` has registers a, b above its three parameters: root-relative slots base + 3 and base + 4
    let along = frames_along(&rc, r);
    let call_step = (0..r.path.len())
        .find(|&i| r.path_depth[i] == 1)
        .expect("a callee step")
        - 1;
    let code = &rc.codes[&r.path_proto[call_step].0];
    let func = Instruction(code[r.path[call_step] as usize]).a() as usize;
    let (ta, tb) = (func + 1 + 3, func + 1 + 4);
    let ret = (call_step..r.path.len())
        .find(|&i| r.path_depth[i] == 0 && i > call_step + 1)
        .expect("back in the root");
    assert!(along[ret].is_empty());
    // a snapshot taken after the return (a later instruction of the iteration, or the writeback) restores them
    let after: Vec<usize> = (0..ir.snaps.len())
        .filter(|&s| r.snap_step[s] as usize >= ret)
        .collect();
    let wb: Vec<u16> = r.writeback.iter().map(|e| e.slot).collect();
    assert!(
        wb.contains(&(ta as u16)) && wb.contains(&(tb as u16)),
        "writeback {wb:?} lacks the popped frame's temporaries {ta} {tb}"
    );
    for s in after {
        let slots: Vec<u16> = ir.snap_entries(s as u32).iter().map(|e| e.slot).collect();
        assert!(
            slots.contains(&(ta as u16)) && slots.contains(&(tb as u16)),
            "S{s} (after the return) lacks the popped frame's slots {ta} {tb}: {slots:?}\n{}",
            dump(ir)
        );
    }
}

/// TR5 recomputed with the public count-model API across frames: `n`, `refund`, `land` of every snapshot.
#[test]
fn count_model_is_exact_across_frames() {
    for p in PROGS {
        let (_, rc) = record_prog(p, 1, strict());
        let r = closed(&rc);
        let low = |i: usize| &rc.lows[&r.path_proto[i].0];
        let cost: Vec<u32> = (0..r.path.len())
            .map(|i| low(i).fetches(&[r.path[i]]))
            .collect();
        assert!(
            cost.iter().all(|&c| c == 1),
            "{}: every step is one fetch",
            p.name
        );
        let n: u32 = cost.iter().sum();
        assert_eq!(r.n, n, "{}", p.name);
        assert_eq!(r.cost, cost);
        for (s, sn) in r.ir.snaps.iter().enumerate() {
            let step = r.snap_step[s] as usize;
            let l = low(step);
            let kind = l.label_kind(sn.pc);
            assert_ne!(kind, LabelKind::Dead, "{}: S{s}", p.name);
            let (land, amt) = match kind {
                LabelKind::Head => (Land::Pre, 0),
                _ => (Land::Mid, l.rem(sn.pc)),
            };
            assert_eq!(sn.land, land, "{}: S{s}", p.name);
            let charged = if s == 0 { 0 } else { n };
            let fetched: u32 = if s == 0 { 0 } else { cost[..step].iter().sum() };
            assert_eq!(
                sn.refund,
                charged as i32 - fetched as i32 - amt as i32,
                "{}: S{s} pc {}",
                p.name,
                sn.pc
            );
        }
        // every callee instruction, the CALL and the RETURN are steps of the path (TR5.1)
        let calls = (0..r.path.len()).filter(|&i| r.path_depth[i] > 0).count();
        assert_eq!(
            calls + (0..r.path.len()).filter(|&i| r.path_depth[i] == 0).count(),
            r.path.len()
        );
    }
}

// ---- lockstep (TR17) --------------------------------------------------------------------------------------------

/// The recorded path, callee frames included, equals the sequence of instructions the VM executes, and the recording
/// changes nothing observable.
#[test]
fn recorded_path_equals_the_vm_sequence() {
    for p in PROGS {
        for seed in 1..=3 {
            let (state, rc) = record_prog(p, seed, strict());
            let r = closed(&rc);
            let mut state = state.unwrap();
            let args = rec_args(p, seed);
            let lg = log_run(
                &mut state,
                &format!("return f({args})"),
                r.path.len() + 2,
                usize::from(r.ir.extent),
            );
            assert!(lg.steps.len() > r.path.len(), "{}: log too short", p.name);
            let got: Vec<(u32, usize, usize)> = lg
                .steps
                .iter()
                .map(|s| (s.pc as u32, s.proto, s.depth))
                .collect();
            let want: Vec<(u32, usize, usize)> = (0..r.path.len())
                .map(|i| (r.path[i], r.path_proto[i].0, r.path_depth[i] as usize))
                .collect();
            assert_eq!(&got[..r.path.len()], &want[..], "{} seed {seed}", p.name);
            assert_eq!(
                (got[r.path.len()].0, got[r.path.len()].2),
                (r.ir.head.1, 0),
                "{}: the loop closes at its head",
                p.name
            );
            // recording changed nothing observable
            let plain = {
                reset(RecCfg::default());
                let mut st = new_state();
                let _ = run(&mut st, &p.defs.replace("ARM()", ""));
                run(&mut st, &format!("return f({args})"))
            };
            assert_eq!(rc.result, plain, "{}", p.name);
        }
    }
}

/// A slot of an inlined frame modified behind the recorder's back makes the lockstep check abort with `Diverge`.
#[test]
fn tampered_callee_slot_diverges() {
    let p = prog("inline_leaf");
    // `leaf(a, b)`: parameters at base+1..: find a callee slot from the path of a normal recording
    let (_, rc) = record_prog(p, 1, strict());
    let r = closed(&rc);
    let first_callee = (0..r.path.len()).find(|&i| r.path_depth[i] == 1).unwrap();
    let call_step = first_callee - 1;
    let func =
        Instruction(rc.codes[&r.path_proto[call_step].0][r.path[call_step] as usize]).a() as usize;
    let slot = func + 1 + 2; // `t`, the first local of leaf
    for at in first_callee + 2..first_callee + 4 {
        reset(RecCfg::default());
        TAMPER.set(Some((at, slot, Value::int(12345))));
        let mut st = new_state();
        let _ = run(&mut st, &p.defs.replace("ARM()", "arm()"));
        let _ = run(&mut st, &format!("return f({})", rec_args(p, 1)));
        drop(st);
        let (_, out) = record::take_outcome().unwrap();
        assert!(
            matches!(out, RecOutcome::Aborted(AbortReason::Diverge)),
            "tamper before step {at}: {out:?}"
        );
    }
}

// ---- stack room and CallInfo spares (TR6b, TR6c) -------------------------------------------------------------------

/// `room` and `depth` recomputed from the bytecode of the recorded path: a Lua call needs `top + maxstacksize(callee)`
/// (strictly), an intrinsic `top + LUA_MINSTACK`, and its native frame one `CallInfo` (mutants `native_room_skip`,
/// `native_ci_skip`).
#[test]
fn room_and_ci_conditions() {
    for name in ["math_intrinsics", "inline_2", "depth4", "inline_leaf"] {
        let (_, rc) = record_prog(prog(name), 1, strict());
        let r = closed(&rc);
        let along = frames_along(&rc, r);
        let mut room = 0usize;
        let mut ci = 0usize;
        for i in 0..r.path.len() {
            // base of the frame of step i: the function slot of its innermost link + 1
            let base = along[i].last().map_or(0, |l| l.2 as usize + 1);
            let code = &rc.codes[&r.path_proto[i].0];
            let ins = Instruction(code[r.path[i] as usize]);
            if ins.opcode() != Some(OpCode::Call) {
                continue;
            }
            let top = base + ins.a() as usize + 1 + (ins.b() as usize - 1);
            let d = r.path_depth[i] as usize;
            if i + 1 < r.path.len() && usize::from(r.path_depth[i + 1]) == d + 1 {
                // a Lua call: the callee's maxstacksize
                room = room.max(top + rc.maxstacks[&r.path_proto[i + 1].0]);
            } else {
                room = room.max(top + LUA_MINSTACK);
            }
            ci = ci.max(d + 1);
        }
        assert_eq!(r.room as usize, room, "{name}: TR6c room");
        assert_eq!(usize::from(r.ir.depth), ci, "{name}: TR6b CallInfo spares");
        assert!(usize::from(r.ir.extent) >= room);
    }
}

// ---- P-E: stack room and shape of inlined vararg frames ---------------------------------------------------------------

/// `room` recomputed from the bytecode of a recorded vararg path (callers at depth 0): a vararg callee needs the call's
/// `top + maxstacksize` and `VARARGPREP`'s `check_stack(maxstacksize + 1)` at `func + 1 + totalargs`; a `VARARG` with `C = 0`
/// runs `check_stack(nextra)` above the frame's `top` (mutant `vararg_room_skip`). The frame's shifted base is
/// `func + 1 + totalargs + 1` (mutant `vararg_delta_off`), and its links carry `nextraargs`.
#[test]
fn vararg_room_and_layout() {
    for (name, np) in [("vararg_pass", 0usize), ("vararg_fixed", 2usize)] {
        let p = VPROGS.iter().find(|p| p.name == name).unwrap();
        let (_, rc) = record_prog(p, 1, strict());
        let r = closed(&rc);
        let mut room = 0usize;
        let mut ncalls = 0;
        let mut cur: Option<(usize, usize)> = None; // (shifted base, nextra) of the vararg frame being recorded
        for i in 0..r.path.len() {
            let code = &rc.codes[&r.path_proto[i].0];
            let ins = Instruction(code[r.path[i] as usize]);
            let d = r.path_depth[i] as usize;
            if d == 0 && ins.opcode() == Some(OpCode::Call) && i + 1 < r.path.len() && r.path_depth[i + 1] == 1 {
                let nargs = ins.b() as usize - 1;
                let total = nargs.max(np);
                let func = ins.a() as usize;
                let ns = rc.maxstacks[&r.path_proto[i + 1].0];
                let top = func + 1 + nargs;
                room = room.max(top + ns).max(func + 1 + total + ns + 1);
                cur = Some((func + 1 + total + 1, total - np));
                ncalls += 1;
            }
            if d == 1 && ins.opcode() == Some(OpCode::Vararg) && ins.c() == 0 {
                let (base, nextra) = cur.expect("a vararg frame");
                let ns = rc.maxstacks[&r.path_proto[i].0];
                room = room.max(base + ns + nextra);
            }
        }
        assert!(ncalls > 0, "{name}: no vararg call on the path");
        assert_eq!(r.room as usize, room, "{name}: TR6c room of the vararg frames\n{}", dump(&r.ir));
        // every snapshot inside a vararg frame carries the number of extra arguments and the shifted function slot
        let mut seen = false;
        for s in 0..r.ir.snaps.len() as u32 {
            for l in snap::frame_links(&r.ir, s) {
                seen = true;
                assert!(l.nextra >= 0, "{name}: snapshot {s}: vararg frame without nextraargs");
            }
        }
        let _ = seen;
    }
}

/// The selector of an inlined `select` is guarded (`'#'`: identity of the string; integer: its value), unless it folded to a
/// constant (mutant `select_guard_skip`).
#[test]
fn select_selectors_are_guarded() {
    let p = VPROGS.iter().find(|p| p.name == "vararg_selvar").unwrap();
    let (_, rc) = record_prog(p, 1, strict());
    let r = closed(&rc);
    let int_guards = r.ir.ins.iter().filter(|i| i.op == TOp::Eq && i.flags & F_GUARD != 0).count();
    assert!(int_guards >= 1, "no EQ guard on the integer selector\n{}", dump(&r.ir));
    let p = VPROGS.iter().find(|p| p.name == "vararg_select").unwrap();
    let (_, rc) = record_prog(p, 1, strict());
    let r = closed(&rc);
    let str_guards = r.ir.ins.iter().filter(|i| i.op == TOp::Eqref && i.flags & F_GUARD != 0 && i.b != NOREF && i.b >= REF_BIAS).count();
    assert!(str_guards >= 1, "no EQREF guard on the '#' selector\n{}", dump(&r.ir));
}

// ---- TR6d: entry guards -----------------------------------------------------------------------------------------

fn eqref_proto_guards(ir: &TraceIr) -> Vec<(usize, u32)> {
    ir.ins
        .iter()
        .enumerate()
        .filter(|(_, i)| {
            i.op == TOp::Eqref
                && i.b != NOREF
                && i.b < REF_BIAS
                && matches!(ir.k[usize::from(i.b) - 1], KVal::Ptr(_, PtrKind::Proto))
        })
        .map(|(n, i)| (n, aux_snap(i.aux)))
        .collect()
}

/// Every inlined call carries a callee-identity guard (`EQREF closure, KPTR proto`): mutant `callee_guard_skip`.
#[test]
fn callee_identity_guards_are_recorded() {
    for (name, calls) in [
        ("inline_leaf", 1),
        ("inline_2", 2),
        ("sibling_calls", 3),
        ("depth4", 4),
    ] {
        let (_, rc) = record_prog(prog(name), 1, strict());
        let r = closed(&rc);
        let guards = eqref_proto_guards(&r.ir);
        assert_eq!(guards.len(), calls, "{name}\n{}", dump(&r.ir));
        // each compares against the proto of a callee on the path
        for (n, _) in &guards {
            let KVal::Ptr(a, PtrKind::Proto) = r.ir.k[usize::from(r.ir.ins[*n].b) - 1] else {
                unreachable!()
            };
            assert!(r.path_proto.iter().any(|k| k.0 == a as usize), "{name}");
            assert!(r.ir.deps.iter().any(|k| k.0 == a as usize), "{name}");
        }
    }
    // intrinsics: one EQREF against a builtin address per call
    let (_, rc) = record_prog(prog("math_intrinsics"), 1, strict());
    let r = closed(&rc);
    let builtins =
        r.ir.ins
            .iter()
            .filter(|i| {
                i.op == TOp::Eqref
                    && matches!(r.ir.k[usize::from(i.b) - 1], KVal::Ptr(_, PtrKind::Builtin))
            })
            .count();
    assert_eq!(builtins, 7, "{}", dump(&r.ir)); // sqrt abs floor ceil min max abs
}

/// TR6d: a callee-identity guard uses the entry snapshot only when every operand is an entry value and no store
/// precedes it; after a (possibly aliasing) store it keeps its instruction's own snapshot. Mutant
/// `entry_guard_past_store`.
#[test]
fn entry_guard_snapshot_rules() {
    // no store before the call: the guard is an entry guard (snapshot: empty map at H, charged)
    let (_, rc) = record_prog(prog("inline_leaf"), 1, strict());
    let r = closed(&rc);
    let es = r
        .entry_snap
        .expect("the identity guard uses the entry snapshot");
    let g = eqref_proto_guards(&r.ir);
    assert_eq!(g.len(), 1);
    assert_eq!(g[0].1, es);
    let sn = r.ir.snaps[es as usize];
    assert_eq!(
        (sn.pc, sn.nframes, sn.map.1, sn.land),
        (r.ir.head.1, 0, 0, Land::Pre)
    );
    assert_eq!(sn.refund, r.n as i32, "charged: it is the slice's S0R");
    assert_ne!(es, 0);
    // a store (`acc = acc + 1`, an upvalue that may alias the callee's) precedes the call: the guard stays at its
    // instruction with its own snapshot, which lists the stored value's slot
    let (_, rc) = record_prog(prog("store_then_call"), 1, strict());
    let r = closed(&rc);
    let g = eqref_proto_guards(&r.ir);
    assert_eq!(g.len(), 1, "{}", dump(&r.ir));
    assert_ne!(Some(g[0].1), r.entry_snap, "{}", dump(&r.ir));
    assert!(r.entry_snap.is_none() || r.entry_snap.is_some_and(|e| e != g[0].1));
    let own = r.ir.snaps[g[0].1 as usize];
    assert!(
        own.refund < r.n as i32,
        "the own snapshot is mid-iteration: {own:?}"
    );
    // the guard of an intrinsic follows the same rule
    let (_, rc) = record_prog(prog("math_intrinsics"), 1, strict());
    let r = closed(&rc);
    assert!(r.entry_snap.is_some());
    for (n, s) in r.ir.ins.iter().enumerate().filter_map(|(n, i)| {
        (i.op == TOp::Eqref && i.flags & F_GUARD != 0).then_some((n, aux_snap(i.aux)))
    }) {
        // no store in this loop: all of them are entry guards
        assert_eq!(Some(s), r.entry_snap, "%{n}");
    }
}

// ---- F2a: side recordings (7.8) ------------------------------------------------------------------------------------------

fn side_rec(defs: &str, call: &str, known: &[(u16, Ty)], traced: bool) -> Rec {
    side_rec_skip(defs, call, known, traced, 0)
}

fn side_rec_skip(defs: &str, call: &str, known: &[(u16, Ty)], traced: bool, skip: u32) -> Rec {
    SIDE_KNOWN.with(|k| *k.borrow_mut() = known.to_vec());
    SIDE_TRACED.set(traced);
    SIDE_SKIP.set(skip);
    let cfg = RecCfg {
        verify: true,
        ..RecCfg::default()
    };
    record_chunks(
        &defs.replace("SIDE()", "side()"),
        &call.replace("SIDE()", "side()"),
        cfg,
    )
    .1
}

const SIDE_LOOP: &str = "function f(n) local s = 0 for i = 1, n do s = s + i SIDE() s = s * 2 + i end return s end return f(6)";

/// A side recording starts at the pc after `side()` (in the middle of the loop body's block), runs on to the next loop
/// head and ends with a link into it: the iteration is rolled (no `LOOP`), starts with `BUDGET` (no `POLL`), its
/// first snapshot is the entry state (refund 0 at a mid-block label), the `BUDGET` charge is `n - rem(start)` (the rest
/// of the block was prepaid by tier 1) and the poll of the link has the empty map at the head. Mutants
/// `side_budget_no_rem`, `side_start_refund`.
#[test]
fn side_recording_shape_and_count_model() {
    // (the instruction after a call starts a block: skip two instructions to start in the middle of one)
    let r = side_rec_skip("", SIDE_LOOP, &[], true, 2);
    let rc = closed(&r);
    let info = rc.side.expect("a side trace");
    let low = r.lows.values().next().expect("lowered");
    let (land, rem) = snap::land_info(low, info.start_pc);
    assert_eq!(land, Land::Mid, "the start is mid-block");
    assert!(rem > 0);
    assert_eq!(info.start_rem, rem);
    assert!(low.backedges.iter().any(|&(_, h)| h == info.link_pc));
    assert!(rc.ir.loop_at.is_none());
    assert_eq!(rc.ir.ins[0].op, TOp::Budget, "no POLL: {}", dump(&rc.ir));
    assert!(!rc.ir.ins.iter().any(|i| i.op == TOp::Poll));
    assert_eq!(i64::from(rc.ir.ins[0].a), i64::from(rc.n) - i64::from(rem));
    // snapshot 0: the entry state, nothing refunded; the link's poll: the empty map at the head
    let s0 = rc.ir.snaps[0];
    assert_eq!((s0.pc, s0.refund, s0.map.1), (info.start_pc, 0, 0));
    let sl = rc.ir.snaps[info.link_snap as usize];
    assert_eq!((sl.pc, sl.refund, sl.map.1, sl.land), (info.link_pc, 0, 0, Land::Pre));
    // every other snapshot refunds what the interpreter would not have fetched yet (n - fetches - land)
    for (k, s) in rc.ir.snaps.iter().enumerate() {
        if k == 0 || k as u32 == info.link_snap {
            continue;
        }
        let at = rc.snap_step[k] as usize;
        let fetched: u32 = rc.cost[..at].iter().sum();
        let (_, amt) = snap::land_info(low, s.pc);
        assert_eq!(s.refund, rc.n as i32 - fetched as i32 - amt as i32, "S{k}");
    }
}

/// The types the parent's exit snapshot fixes need no check; the other slots keep theirs. A known type that does not
/// match the value is not trusted. Mutant `side_known_all`.
#[test]
fn side_known_types_skip_typechecks() {
    let checked = |rc: &Recorded| {
        rc.ir
            .ins
            .iter()
            .filter(|i| i.op == TOp::Sload && i.flags & F_GUARD != 0)
            .count()
    };
    let any = |rc: &Recorded| rc.ir.ins.iter().filter(|i| i.op == TOp::Sload).count();
    let none = side_rec("", SIDE_LOOP, &[], true);
    let n0 = checked(closed(&none));
    assert!(n0 >= 2, "s and i are read: {}", dump(&closed(&none).ir));
    let all_int: Vec<(u16, Ty)> = (0..8).map(|s| (s, Ty::Int)).collect();
    let known = side_rec("", SIDE_LOOP, &all_int, true);
    assert_eq!(checked(closed(&known)), 0, "{}", dump(&closed(&known).ir));
    assert_eq!(any(closed(&known)), any(closed(&none)), "the loads stay");
    let wrong: Vec<(u16, Ty)> = (0..8).map(|s| (s, Ty::Num)).collect();
    let bad = side_rec("", SIDE_LOOP, &wrong, true);
    assert_eq!(checked(closed(&bad)), n0, "a known type that is wrong is not trusted");
}

/// A side trace of fewer bytecodes than `min_len` is not installed (`SideShort`); one of exactly that length is.
/// Mutant `side_short_ignored`.
#[test]
fn short_side_traces_are_refused() {
    SIDE_MIN.set(1000);
    let r = side_rec("", SIDE_LOOP, &[], true);
    SIDE_MIN.set(0);
    assert!(
        matches!(r.out, RecOutcome::Aborted(AbortReason::SideShort)),
        "{}",
        r.result
    );
    let ok = side_rec("", SIDE_LOOP, &[], true);
    let n = closed(&ok).n;
    SIDE_MIN.set(n);
    let at = side_rec("", SIDE_LOOP, &[], true);
    SIDE_MIN.set(n + 1);
    let below = side_rec("", SIDE_LOOP, &[], true);
    SIDE_MIN.set(0);
    assert!(closed(&at).side.is_some());
    assert!(matches!(below.out, RecOutcome::Aborted(AbortReason::SideShort)));
}

/// Reaching a loop head without a trace aborts the side recording (7.8); the loop end of the function does too.
#[test]
fn side_recording_aborts_at_untraced_heads_and_frame_ends() {
    let r = side_rec("", SIDE_LOOP, &[], false);
    assert!(
        matches!(r.out, RecOutcome::Aborted(AbortReason::SideUntraced)),
        "{:?}",
        match &r.out {
            RecOutcome::Aborted(a) => format!("{a:?}"),
            RecOutcome::Closed(_) => "closed".into(),
        }
    );
    // a side recording that runs into the end of the frame
    let r = side_rec(
        "",
        "function f(n) local s = 0 SIDE() s = s + n return s end return f(6)",
        &[],
        true,
    );
    assert!(
        matches!(r.out, RecOutcome::Aborted(AbortReason::LeaveFrame)),
        "{}",
        r.result
    );
}

/// TR6d for side traces: a guard may use the entry snapshot (the empty map at the start pc) only when no store precedes
/// it on the side path. Mutant `entry_guard_past_store`.
#[test]
fn side_entry_guard_snapshot_rules() {
    let clean = side_rec(
        "local acc = 0
         local function add1(x) return x + 1 end
         function f(n) for i = 1, n do SIDE() acc = acc + add1(i) end return acc end",
        "return f(6)",
        &[],
        true,
    );
    let r = closed(&clean);
    let g = eqref_proto_guards(&r.ir);
    assert_eq!(g.len(), 1, "{}", dump(&r.ir));
    let es = r.entry_snap.expect("the identity guard is an entry guard");
    assert_eq!(g[0].1, es);
    let sn = r.ir.snaps[es as usize];
    let info = r.side.unwrap();
    assert_eq!((sn.pc, sn.nframes, sn.map.1), (info.start_pc, 0, 0));
    // a store (an upvalue that may alias the callee's) precedes the call: the guard keeps its own snapshot
    let stored = side_rec(
        "local acc = 0
         local function add1(x) return x + 1 end
         function f(n) for i = 1, n do SIDE() acc = acc + 1 acc = acc + add1(i) end return acc end",
        "return f(6)",
        &[],
        true,
    );
    let r = closed(&stored);
    let g = eqref_proto_guards(&r.ir);
    assert_eq!(g.len(), 1, "{}", dump(&r.ir));
    assert_ne!(Some(g[0].1), r.entry_snap, "{}", dump(&r.ir));
    assert!(r.ir.snaps[g[0].1 as usize].refund < r.n as i32);
}

/// F2b: a root recording whose path reaches the head of an inner loop that has a trace ends with a link into it (nest
/// root): rolled, `POLL` + `BUDGET` first, the link poll has the empty map at the inner head, and the TR6d rule for
/// callee-identity guards is the root's (entry snapshot only when no store precedes). Mutants `entry_guard_past_store`,
/// `nest_link_never`.
#[test]
fn nest_root_shape_and_entry_guard_rules() {
    let nest = |body: &str| {
        NEST_ROOT.set(true);
        let r = record_chunks(
            &format!(
                "local acc = 0
                 local function add1(x) return x + 1 end
                 function f(n) arm() for i = 1, n do {body} local k = 0 while k < 3 do k = k + 1 end end return acc end"
            ),
            "return f(6)",
            RecCfg {
                verify: true,
                ..RecCfg::default()
            },
        )
        .1;
        NEST_ROOT.set(false);
        r
    };
    let clean = nest("acc = acc + add1(i)");
    let rc = closed(&clean);
    let info = rc.side.expect("a nest root ends with a link");
    assert!(!info.frame_link);
    assert!(clean
        .lows
        .values()
        .any(|low| low.backedges.iter().any(|&(_, h)| h == info.link_pc)));
    assert_ne!(info.link_pc, rc.ir.head.1, "the link goes to an inner head");
    assert!(rc.ir.loop_at.is_none());
    assert_eq!(rc.ir.ins[0].op, TOp::Poll, "{}", dump(&rc.ir));
    assert_eq!(rc.ir.ins[1].op, TOp::Budget);
    let sl = rc.ir.snaps[info.link_snap as usize];
    assert_eq!((sl.pc, sl.refund, sl.map.1, sl.land), (info.link_pc, 0, 0, Land::Pre));
    let g = eqref_proto_guards(&rc.ir);
    assert_eq!(g.len(), 1, "{}", dump(&rc.ir));
    assert_eq!(Some(g[0].1), rc.entry_snap, "{}", dump(&rc.ir));
    let stored = nest("acc = acc + 1 acc = acc + add1(i)");
    let rc = closed(&stored);
    let g = eqref_proto_guards(&rc.ir);
    assert_eq!(g.len(), 1, "{}", dump(&rc.ir));
    assert_ne!(Some(g[0].1), rc.entry_snap, "{}", dump(&rc.ir));
}

// ---- TR32: dependency edges -------------------------------------------------------------------------------------------

fn sink_hooks() -> DepHooks {
    DepHooks {
        register: Rc::new(|_st, p| {
            SINK.with(|s| {
                let mut s = s.borrow_mut();
                let n = s.registered.len();
                s.registered
                    .push((&*p as *const Proto as usize, STEPNO.get()));
                !s.refuse_from.is_some_and(|k| n >= k)
            })
        }),
        alive: Rc::new(|_st| {
            SINK.with(|s| {
                let mut s = s.borrow_mut();
                s.polls += 1;
                !s.alive_polls.is_some_and(|k| s.polls > k)
            })
        }),
    }
}

fn with_sink(sink: Sink) -> RecCfg {
    SINK.with(|s| *s.borrow_mut() = sink);
    RecCfg {
        verify: true,
        deps: Some(sink_hooks()),
        ..RecCfg::default()
    }
}

/// The edge to an inlined proto is registered when the recorder inlines the call (after the `CALL` step, before the
/// callee's first step), once per proto, never for the root; mutant `dep_edge_at_install` registers at close.
#[test]
fn dep_edges_are_registered_when_inlining() {
    let (_, rc) = record_prog(prog("inline_2"), 1, with_sink(Sink::default()));
    let r = closed(&rc);
    let reg = SINK.with(|s| s.borrow().registered.clone());
    assert_eq!(reg.len(), 2, "mid and leaf, once each: {reg:?}");
    assert!(
        !reg.iter().any(|(p, _)| *p == r.ir.head.0 .0),
        "the root has no edge"
    );
    for (proto, at_step) in &reg {
        // the first path index in this proto = the callee's first step; the CALL was the step before, recorded when
        // the step counter read (call index + 1)
        let first = r
            .path_proto
            .iter()
            .position(|k| k.0 == *proto)
            .expect("on the path");
        assert_eq!(
            *at_step, first,
            "edge of proto {proto:x} registered at step {at_step}, callee starts at step {first}"
        );
    }
    // a callee called twice is registered once
    let (_, rc2) = record_prog(prog("sibling_calls"), 1, with_sink(Sink::default()));
    let _ = closed(&rc2);
    let reg = SINK.with(|s| s.borrow().registered.clone());
    assert_eq!(reg.len(), 2, "sq and neg: {reg:?}");
}

/// A refused edge or a recording flagged by a freed dependency aborts with `DepFreed`.
#[test]
fn dep_hooks_abort_with_dep_freed() {
    let p = prog("inline_2");
    // `register` returns false (compile refused, or `register_dep` failed)
    let (_, rc) = record_prog(
        p,
        1,
        with_sink(Sink {
            refuse_from: Some(1),
            ..Sink::default()
        }),
    );
    assert!(
        matches!(rc.out, RecOutcome::Aborted(AbortReason::DepFreed)),
        "{:?}",
        rc.out
    );
    let (_, rc) = record_prog(
        p,
        1,
        with_sink(Sink {
            refuse_from: Some(0),
            ..Sink::default()
        }),
    );
    assert!(
        matches!(rc.out, RecOutcome::Aborted(AbortReason::DepFreed)),
        "{:?}",
        rc.out
    );
    // the recording is flagged between two steps (a dependency was freed): the next step aborts
    for polls in [3, 9, 15] {
        let (_, rc) = record_prog(
            p,
            1,
            with_sink(Sink {
                alive_polls: Some(polls),
                ..Sink::default()
            }),
        );
        assert!(
            matches!(rc.out, RecOutcome::Aborted(AbortReason::DepFreed)),
            "{polls}: {:?}",
            rc.out
        );
    }
    // nothing flagged, nothing refused: closes
    let (_, rc) = record_prog(p, 1, with_sink(Sink::default()));
    let _ = closed(&rc);
}

// ---- the abort matrix (4.6) ---------------------------------------------------------------------------------------

#[test]
fn abort_matrix_calls() {
    let nyi = |op: OpCode| AbortReason::Nyi(op as u8);
    let cases: Vec<(&str, &str, AbortReason)> = vec![
        ("CFunc (math.sin is not an intrinsic)", "local sin = math.sin local function f(n) local s = 0.0 ARM() for i = 1, n do s = s + sin(i) end return s end return f(5)", AbortReason::CFunc),
        ("CFunc (tostring)", "local ts = tostring local function f(n) local s = 0 ARM() for i = 1, n do local z = ts(i) s = s + i end return s end return f(5)", AbortReason::CFunc),
        ("MultRet (SETLIST up to the top)", "local function two(x) return x, x end local function f(n) local s = 0 ARM() for i = 1, n do local t = {i, two(i)} s = s + t[1] end return s end return f(5)", AbortReason::MultRet),
        ("Vararg (tail call into a vararg callee)", "local function g(...) return 1 end local function h(a) return g(a) end local function f(n) local s = 0 ARM() for i = 1, n do s = s + h(i) end return s end return f(5)", AbortReason::Vararg),
        ("Recursion (callee already on the frame stack)", "local r r = function(x) if x > 0 then return 1 + r(x - 1) end return 0 end local function f(n) local s = 0 ARM() for i = 1, n do s = s + r(2) end return s end return f(5)", AbortReason::Recursion),
        ("Recursion (the root calls itself)", "local f f = function(n, d) local s = 0 ARM() for i = 1, n do if d == 0 then s = s + f(1, 1) end end return s end return f(3, 0)", AbortReason::Recursion),
        ("Depth (a fifth nested call)", "local function e5(x) return x + 1 end local function d4(x) return e5(x) * 2 end local function d3(x) return d4(x) * 2 end local function d2(x) return d3(x) * 2 end local function d1(x) return d2(x) * 2 end local function f(n) local s = 0 ARM() for i = 1, n do s = s + d1(i) end return s end return f(5)", AbortReason::Depth),
        ("TailCall (a C function that is not an intrinsic)", "local sin = math.sin local function g(x) return sin(x) end local function f(n) local s = 0 ARM() for i = 1, n do s = s + g(i) end return s end return f(5)", AbortReason::TailCall),
        ("InnerLoop (a loop in the callee)", "local function g(n) local j = 0 while j < n do j = j + 1 end return j end local function f(n) local s = 0 ARM() for i = 1, n do s = s + g(3) end return s end return f(5)", AbortReason::InnerLoop),
        ("Nyi (a numeric for in the callee)", "local function g(n) local s = 0 for j = 1, n do s = s + j end return s end local function f(n) local s = 0 ARM() for i = 1, n do s = s + g(3) end return s end return f(5)", nyi(OpCode::Forprep)),
        ("Metamethod (a table with __call)", "local t = setmetatable({}, {__call = function(self, x) return x end}) local function f(n) local s = 0 ARM() for i = 1, n do s = s + t(i) end return s end return f(5)", AbortReason::Metamethod),
        ("Metamethod (calling nil: an error is predicted)", "local g local function f(n) local s = 0 ARM() for i = 1, n do s = s + g(i) end return s end return pcall(f, 5)", AbortReason::Metamethod),
        ("Nyi (an intrinsic with a surplus argument)", "local sqrt = math.sqrt local function f(n) local s = 0.0 ARM() for i = 1, n do s = s + sqrt(i, 2) end return s end return f(5)", nyi(OpCode::Call)),
        ("Nyi (max with one argument)", "local max = math.max local function f(n) local s = 0 ARM() for i = 1, n do s = s + max(i) end return s end return f(5)", nyi(OpCode::Call)),
        ("Nyi (floor of a float that does not fit an integer)", "local floor = math.floor local function f(n) local s = 0.0 ARM() for i = 1, n do s = s + floor(1e300 + i) end return s end return f(5)", nyi(OpCode::Call)),
        ("Nyi (min of an integer and a float)", "local min = math.min local function f(n) local s = 0.0 ARM() for i = 1, n do s = s + min(i, 2.5) end return s end return f(5)", nyi(OpCode::Call)),
        ("Coerce (sqrt of a string)", "local sqrt = math.sqrt local function f(n) local s = 0.0 ARM() for i = 1, n do s = s + sqrt('4') end return s end return f(5)", AbortReason::Coerce),
        ("Metamethod (sqrt of nil: an error is predicted)", "local sqrt = math.sqrt local function f(n, z) local s = 0.0 ARM() for i = 1, n do s = s + sqrt(z) end return s end return pcall(f, 5, nil)", AbortReason::Metamethod),
    ];
    for (name, src, want) in cases {
        let got = aborted(src);
        assert_eq!(got, want, "{name}: {src}");
    }
}

/// Four nested calls are the limit (4.4 `maxdepth`): recorded, with the deepest frame at depth 4.
#[test]
fn depth_four_is_inlined() {
    let (_, rc) = record_prog(prog("depth4"), 1, strict());
    let r = closed(&rc);
    assert_eq!(r.path_depth.iter().max(), Some(&(MAX_DEPTH as u8)));
    assert_eq!(usize::from(r.ir.depth), MAX_DEPTH);
}

/// Missing arguments are `nil` in the callee (`pre_call_lua_fast`), extra arguments stay in its registers; both
/// replay against the VM.
#[test]
fn missing_and_extra_arguments() {
    let defs = "local function g(a, b, c) if c == nil and b == nil then return a end return a + b end
                local function h(a) return a * 2 end
                function f(n) local s = 0 ARM() for i = 1, n do s = s + g(i) + h(i, 7, 8) end return s end";
    let p = Prog {
        name: "args",
        defs: keep(Box::leak(defs.to_string().into_boxed_str())),
        args: |r, _| rn(r, 3, 12),
    };
    let mut st = Stats::default();
    replay_prog(&p, 1, 6, &[0], &mut st);
    assert!(st.done > 0 && st.forced > 0, "{st:?}");
}

// ---- mutants ------------------------------------------------------------------------------------------------------------

/// Every mutant of this task is selected in a child process (`RUSTYNAIL_JIT_MUT`), which must fail the test that guards
/// it; the baseline child must pass.
#[cfg(feature = "tr_mutants")]
#[test]
fn mutants_are_caught() {
    if std::env::var("TR_CALLS_CHILD").is_ok() {
        return;
    }
    let exe = std::env::current_exe().unwrap();
    let table: &[(&str, &[&str])] = &[
        (
            "popped_frame_drop",
            &[
                "popped_frame_slots_stay_in_snapshots",
                "ir_replay_matches_the_interpreter",
            ],
        ),
        ("native_room_skip", &["room_and_ci_conditions"]),
        ("vararg_room_skip", &["vararg_room_and_layout"]),
        ("vararg_orig_not_nilled", &["vararg_replay_matches_the_interpreter"]),
        ("vararg_delta_off", &["vararg_replay_matches_the_interpreter"]),
        ("vararg_ret_dest", &["vararg_replay_matches_the_interpreter"]),
        ("vararg_first_nil", &["vararg_replay_matches_the_interpreter"]),
        ("vararg_top_off", &["vararg_replay_matches_the_interpreter"]),
        ("select_guard_skip", &["select_selectors_are_guarded"]),
        ("select_count_off", &["vararg_replay_matches_the_interpreter"]),
        ("select_index_off", &["vararg_replay_matches_the_interpreter"]),
        (
            "native_ci_skip",
            &[
                "room_and_ci_conditions",
                "frames_and_snapshots_are_well_formed",
            ],
        ),
        (
            "callee_guard_skip",
            &["callee_identity_guards_are_recorded"],
        ),
        (
            "entry_guard_past_store",
            &[
                "entry_guard_snapshot_rules",
                "nest_root_shape_and_entry_guard_rules",
            ],
        ),
        (
            "dep_edge_at_install",
            &["dep_edges_are_registered_when_inlining"],
        ),
        (
            "rec_no_validate",
            &["stale_context_in_a_reused_callee_frame_is_rejected"],
        ),
    ];
    let child = |mutant: &str, test: &str| {
        let out = std::process::Command::new(&exe)
            .args(["--exact", test, "--test-threads=1"])
            .env("TR_CALLS_CHILD", "1")
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
            let (ok, text) = child("", test);
            assert!(ok, "baseline child of {test} failed\n{text}");
        }
        for test in *tests {
            let (ok, text) = child(mutant, test);
            assert!(!ok, "mutant {mutant} was NOT caught by {test}\n{text}");
        }
    }
}

/// The Miri-sized variant (`scripts/miri.sh jit-trace` selects `miri_*`): three programs, one recording each, two real
/// runs per variant. The intrinsic programs are left out: Miri does not keep function-pointer addresses comparable,
/// which the callee identity of `math.*` needs (the `jit` suite excludes its `intrinsic_calls` tests for the same
/// reason).
#[test]
fn miri_replay_calls_small() {
    for name in ["upval_callee", "early_return", "popped_temps"] {
        let mut st = Stats::default();
        replay_prog(prog(name), 1, 2, &[0, 1], &mut st);
        assert!(st.done + st.exits > 0 && st.forced > 0, "{name}: {st:?}");
        assert!(st.in_frames > 0, "{name}: {st:?}");
    }
}

// ---- P3d: closure identity, shared upvalues ----------------------------------------------------------------------

/// Replays `nlogs` runs per variant against the recording `rc`; Err = the replay's verdict.
fn replay_verdict(rc: &Rec, p: &Prog, state: &mut LuaState, nlogs: u64) -> Result<Stats, String> {
    let r = closed(rc);
    let mut st = Stats::default();
    for variant in [0u8, 1] {
        for seed in 0..nlogs {
            let args = (p.args)(&mut Rng::new(1000 + seed * 7 + u64::from(variant)), variant);
            let lg = log_run(
                state,
                &format!("return f({args})"),
                4 * r.path.len() + 8,
                usize::from(r.ir.extent),
            );
            replay(rc, &lg, &mut st)?;
        }
    }
    Ok(st)
}

/// Review #2 of P3b: `EQREF closure, KPTR(proto)` pins the proto only, a callee's `UREF` takes ITS closure. Two closures
/// of one prototype with different upvalue cells are called in every iteration; each has its own handle in the replay
/// model (no rewriting of the guard to "the logged closure of that proto"), and a trace whose callee `UREF` reads the
/// other closure's cells (what reading the record-time closure's cells would do) fails the replay.
#[test]
fn closures_of_one_proto_read_their_own_cells() {
    for name in ["mk_closures_upvals", "mk_closures_array"] {
        let p = prog(name);
        let (state, rc) = record_prog(p, 1, strict());
        let r = closed(&rc);
        let mut state = state.expect("state");
        // the recording has several distinct callee closure refs
        let urefs: Vec<usize> =
            r.ir.ins
                .iter()
                .enumerate()
                // a callee's UREF: its closure operand is a loaded closure, not the root's FUNC_SLOT
                .filter(|(_, i)| {
                    i.op == TOp::Uref
                        && i.a >= REF_BIAS
                        && r.ir.ins[usize::from(i.a - REF_BIAS)].op != TOp::Sload
                })
                .map(|(n, _)| n)
                .collect();
        let mut crefs: Vec<Ref> = urefs.iter().map(|&n| r.ir.ins[n].a).collect();
        crefs.sort_unstable();
        crefs.dedup();
        assert!(
            crefs.len() >= 2,
            "{name}: callee closures are not distinct\n{}",
            golden_text(&rc)
        );
        // the honest trace replays
        let ok = replay_verdict(&rc, p, &mut state, 8).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert!(ok.done > 0, "{name}: {ok:?}");
        // wrong cells: every callee UREF reads the first closure's cells
        let mut bad = Box::new(r.clone());
        let first = r.ir.ins[urefs[0]].a;
        for &n in &urefs {
            bad.ir.ins[n].a = first;
        }
        let bad_rc = Rec {
            out: RecOutcome::Closed(bad),
            result: rc.result.clone(),
            lows: rc.lows.clone(),
            codes: rc.codes.clone(),
            maxstacks: rc.maxstacks.clone(),
        };
        let verdict = replay_verdict(&bad_rc, p, &mut state, 8);
        assert!(
            verdict.is_err(),
            "{name}: a UREF through the wrong closure went unnoticed"
        );
    }
}

/// Shared upvalues record (the `UpvalAlias` abort of the pair-per-cell model is gone) and replay: `u = u + bump(i)`
/// (an accumulator written by the callee and by the caller), a closed upvalue shared by two closures, an RNG seed
/// updated by one closure and read by another. With the pair-per-cell model (`NO_SHARE`) the replay must fail:
/// the shared cells are what makes these programs agree with the VM.
#[test]
fn shared_upvalues_record_and_replay() {
    for name in ["acc_bump", "shared_pair", "rng_seed"] {
        let p = prog(name);
        let (state, rc) = record_prog(p, 1, strict());
        let r = closed(&rc);
        assert!(
            r.ir.ins.iter().any(|i| i.op == TOp::Ustore),
            "{name}: no upvalue store"
        );
        let mut state = state.expect("state");
        NO_SHARE.set(false);
        let st = replay_verdict(&rc, p, &mut state, 8)
            .unwrap_or_else(|e| panic!("{name}: {e}\n{}", golden_text(&rc)));
        assert!(st.done > 0 && st.forced > 0, "{name}: {st:?}");
        NO_SHARE.set(true);
        let v = replay_verdict(&rc, p, &mut state, 8);
        NO_SHARE.set(false);
        assert!(
            v.is_err(),
            "{name}: the replay does not depend on the shared cells"
        );
    }
}

// ---- P3d: ABA of a callee frame -----------------------------------------------------------------------------------

/// A stale recording context (the abort was lost after the recorder's first step inside the callee `g1`) restored in a
/// later call that reuses the SAME `CallInfo` node and function slot for a different callee: another prototype, or
/// another closure of the same prototype. The VM's own validation passes (same root node, slot and closure; the callee
/// frame is not checked by it), the recorder's frame check (prototype and closure of the expected callee frame, TR23.5)
/// rejects it. Mutant `rec_no_validate` disables that check: the recorder then goes on with the wrong callee.
#[test]
fn stale_context_in_a_reused_callee_frame_is_rejected() {
    let cases = [
        (
            "other proto",
            "local function g1(x) return x + 1 end
             local function g2(x) stale() return x + 2 end
             local G = g1
             local function f(n)
               local s = 0
               arm()
               for i = 1, n do s = s + G(i) if i == 1 then G = g2 end end
               return s
             end
             return f(4)",
        ),
        (
            "same proto, other closure and cells",
            "local function mk(c) return function(x) if c then stale() end return x + 1 end end
             local g1, g2 = mk(false), mk(true)
             local G = g1
             local function f(n)
               local s = 0
               arm()
               for i = 1, n do s = s + G(i) if i == 1 then G = g2 end end
               return s
             end
             return f(4)",
        ),
    ];
    for (name, src) in cases {
        reset(strict());
        FORGET_IN_CALLEE.set(true);
        let mut st = new_state();
        let _ = run(&mut st, src);
        drop(st);
        let (_, out) = record::take_outcome().expect("an outcome");
        assert!(
            matches!(out, RecOutcome::Aborted(AbortReason::Frame)),
            "{name}: the stale callback must be rejected as a Frame abort: {out:?}"
        );
        assert_eq!(jit_hooks::rec_ctx(), RecCtx::default(), "{name}");
        // rejected by the recorder's own validation, not by the VM's
        assert_eq!(
            ABORTS.with(|a| a.borrow().clone()),
            vec![],
            "{name}: the VM aborted the recording itself"
        );
    }
}

// ---- P3d: primitive constants (P3c obligation towards P4c) ---------------------------------------------------------

/// P4c: `build_exits` refuses `KVal::Pri` of anything but nil / false / true (payload 0 would be a null GC pointer
/// otherwise). The recorder must never emit one: every `Pri` constant of every recording of every program (and of
/// a nil / boolean heavy one) is of those three kinds; collectables only appear through the `KPTR` forms.
#[test]
fn recorder_emits_only_nil_false_true_primitives() {
    let extra = Prog {
        name: "prims",
        defs: "local function g(x, y) if y then return x end return nil end
               function f(n) local s = 0 ARM() for i = 1, n do local z = nil local c = i > 2 if c then z = true end if z == nil then s = s + 1 end local r = g(i, c) if r == nil then s = s + 2 end local d = not c if d == false then s = s + 4 end end return s end",
        args: |r, _| rn(r, 3, 12),
    };
    let mut seen = 0;
    for p in PROGS.iter().chain(std::iter::once(&extra)) {
        let (_, rc) = record_prog(p, 1, strict());
        let r = match &rc.out {
            RecOutcome::Closed(r) => r,
            RecOutcome::Aborted(a) if p.name == "prims" => panic!("prims aborted: {a:?}"),
            RecOutcome::Aborted(_) => continue,
        };
        for k in &r.ir.k {
            if let KVal::Pri(t) = k {
                assert!(
                    matches!(t, Ty::Nil | Ty::False | Ty::True),
                    "{}: primitive constant of type {t:?}",
                    p.name
                );
                seen += 1;
            }
        }
    }
    assert!(seen > 0, "no primitive constant was recorded at all");
}

// ---- F4: recursion unroll, TAILCALL, multiple results with an explicit `top` ---------------------------------------

fn f4_cfg(unroll: u32, stitch_min: u32) -> RecCfg {
    RecCfg {
        verify: true,
        unroll,
        stitch_min,
        ..RecCfg::default()
    }
}

const FIB_LOOP: &str = "local function fib(n) if n < 2 then return n end return fib(n - 1) + fib(n - 2) end
                        local function f(n) local s = 0 ARM() for i = 1, n do s = s + fib(6) end return s end return f(5)";

/// The recursion limit: a prototype is inlined as long as it is on the frame stack fewer than `unroll` times (the root
/// counts); the call beyond ends the recording with the stitching exit (or aborts `Recursion` when stitching is off or
/// the recorded prefix is shorter than `stitch_min`, and in a side recording). Mutant `unroll_off_by_one`.
#[test]
fn recursion_unrolls_to_the_limit_and_exits_beyond() {
    for unroll in 1..=3u32 {
        let rc = record_src(FIB_LOOP, f4_cfg(unroll, 4));
        let r = closed(&rc);
        assert!(r.stitched, "unroll {unroll}: the call beyond the limit ends the recording with a stitching exit");
        assert_eq!(
            r.path_depth.iter().max(),
            Some(&(unroll as u8)),
            "unroll {unroll}: the recursion is inlined exactly `unroll` activations deep"
        );
        assert_eq!(usize::from(r.ir.depth), unroll as usize);
        // the always-taken exit is the last guard; its snapshot rebuilds `unroll` frames and carries no top
        let last = r.ir.ins.iter().rev().find(|i| i.flags & F_GUARD != 0).expect("the exit guard");
        let sn = r.ir.snaps[aux_snap(last.aux) as usize];
        assert_eq!(usize::from(sn.nframes), unroll as usize);
        assert_eq!(sn.top, 0);
    }
    // unroll 0 (the prototype's rule), no stitching, a prefix shorter than `stitch_min`: Recursion
    assert_eq!(aborted_cfg(FIB_LOOP, f4_cfg(0, 4)), AbortReason::Recursion);
    assert_eq!(aborted_cfg(FIB_LOOP, f4_cfg(2, 0)), AbortReason::Recursion);
    assert_eq!(aborted_cfg(FIB_LOOP, f4_cfg(2, 4000)), AbortReason::Recursion);
}

/// `TAILCALL` replaces the frame: one frame deeper only for the original call, `CIST_TAIL` and the callee's prototype in
/// the snapshots inside the replaced frame, the original caller `pc` and wanted results; the snapshot is checked against
/// the VM's `CallInfo` chain by `ir_replay_matches_the_interpreter` (`tail_chain`, `tail_missing_args`, `tail_two_deep`).
/// Mutants `tail_flag_skip` (exit), `tail_args_not_moved`, `tail_nil_fill_skip` (the lockstep).
#[test]
fn tailcall_replaces_the_frame() {
    let (_, rc) = record_prog(prog("tail_chain"), 1, strict());
    let r = closed(&rc);
    assert_eq!(r.path_depth.iter().max(), Some(&1), "mid -> leaf is one frame, not two");
    assert_eq!(usize::from(r.ir.depth), 1);
    let mut tails = 0;
    for s in 0..r.ir.snaps.len() as u32 {
        let links = snap::frame_links(&r.ir, s);
        for l in links {
            if l.tail {
                tails += 1;
                assert_eq!(l.nresults, 1, "the replaced frame keeps the wanted results of the original call");
            }
        }
    }
    assert!(tails > 0, "no snapshot inside the tail-called frame");
    // the frames' prototype is the callee's (`leaf`), not the original callee's (`mid`)
    let protos: std::collections::HashSet<_> = (0..r.ir.snaps.len() as u32)
        .flat_map(|s| snap::frame_links(&r.ir, s).iter().filter(|l| l.tail).map(|l| l.proto.0).collect::<Vec<_>>())
        .collect();
    assert_eq!(protos.len(), 1);
}

/// A consumer of a multiple-result sequence records the producer's `top` in its snapshots (and only there): the lockstep
/// compares the model's `top` with the VM's after each producer, `ir_replay_matches_the_interpreter` compares the snapshots'.
/// Mutants `snap_top_dropped`, `top_count_off`.
#[test]
fn multret_consumers_carry_top() {
    let (_, rc) = record_prog(prog("multret_intrinsic2"), 1, strict());
    let r = closed(&rc);
    let with_top = r.ir.snaps.iter().filter(|s| s.top != 0).count();
    assert!(with_top >= 2, "the min / max guards of the consumers carry the top: {with_top}");
    // snapshot 0 (the loop head) never has one
    assert_eq!(r.ir.snaps[0].top, 0);
    let (_, rc) = record_prog(prog("multret_chain"), 1, strict());
    let r = closed(&rc);
    // the whole chain (CALL C=0, CALL B=0, RETURN B=0 in an inlined frame, nested) records and the frames stay balanced
    assert_eq!(r.path_depth.iter().max(), Some(&2));
}

// ---- P-O: pcall / xpcall of a Lua closure -------------------------------------------------------------------------

fn pcall_cfg() -> RecCfg {
    RecCfg { pcall: true, ..strict() }
}

/// Counts the `Lib` ops of kind `lib::CCALLS` in `ir`.
fn ccalls_guards(ir: &TraceIr) -> usize {
    ir.ins.iter().filter(|i| i.op == TOp::Lib && (i.aux & 0xFF) == lib::CCALLS).count()
}

/// A pure body is inlined: the recording closes, the room covers the native's frame (`top + LUA_MINSTACK`) and the callee's
/// (`top' + maxstacksize`, the callee sitting `shift` slots above the native), two `CallInfo`s are needed, and one `CCALLS`
/// guard keeps the C stack in reach. Mutants `pcall_room_skip`, `pcall_ccalls_skip`, `native_ci_skip`, `pcall_shift_off`.
#[test]
fn pcall_room_ci_and_snapshots() {
    for (src, shift, x) in [
        (
            "local function g(x, y) if x < y then return x + y end return y end
             function f(n) local s = 0 arm() for i = 1, n do local ok, v = pcall(g, i, 100000) s = s + v end return s end
             return f(40)",
            2usize,
            false,
        ),
        (
            "local function g(x, y) if x < y then return x + y end return y end
             function f(n) local s = 0 arm() for i = 1, n do local ok, v = xpcall(g, print, i, 100000) s = s + v end return s end
             return f(40)",
            4usize,
            true,
        ),
        (
            // a body with many registers: the callee's frame decides the room
            "local function g(x) local a1,a2,a3,a4,a5,a6,a7,a8,a9,a10,a11,a12,a13,a14,a15,a16,a17,a18,a19,a20,a21,a22,a23,a24,a25,a26,a27,a28,a29,a30 = x,x,x,x,x,x,x,x,x,x,x,x,x,x,x,x,x,x,x,x,x,x,x,x,x,x,x,x,x,x
              return a1 + a30 end
             function f(n) local s = 0 arm() for i = 1, n do local ok, v = pcall(g, i) s = s + v end return s end
             return f(40)",
            2usize,
            false,
        ),
    ] {
        let rc = record_src(src, pcall_cfg());
        let r = closed(&rc);
        // the CALL of pcall / xpcall is at the root; its register A and the argument count give the expected room
        let mut call = None;
        for (i, &pc) in r.path.iter().enumerate() {
            let ins = Instruction(rc.codes[&r.path_proto[i].0][pc as usize]);
            if r.path_depth[i] == 0 && ins.opcode() == Some(OpCode::Call) && r.path_depth.get(i + 1) == Some(&1) {
                call = Some((ins.a() as usize, ins.b() as usize - 1, rc.maxstacks[&r.path_proto[i + 1].0], i));
            }
        }
        let (a, nargs, ns, step) = call.expect("a CALL entering the protected body");
        let nf = nargs - if x { 2 } else { 1 };
        let expect = (a + 1 + nargs + LUA_MINSTACK).max(a + shift + 1 + nf + ns);
        assert_eq!(r.room as usize, expect, "TR6c room of the protected call\n{}", dump(&r.ir));
        assert_eq!(r.ir.depth, 2, "two CallInfos (the native's and the body's)\n{}", dump(&r.ir));
        assert_eq!(ccalls_guards(&r.ir), 1, "one C-stack guard\n{}", dump(&r.ir));
        // a guard of the body exits to the CALL itself
        let call_pc = r.path[step];
        let at_call = r.ir.snaps.iter().filter(|s| s.pc == call_pc && s.frame == 0).count();
        assert!(at_call >= 1, "no snapshot at the CALL\n{}", dump(&r.ir));
    }
}

/// BUNDLE-G2: the C-stack guard (`LIB CCALLS`) of an inlined `pcall` is loop invariant (nothing on a trace changes `nCcalls`): after
/// the peel only the pre-roll copy is left, one per distinct depth, and the body has none. Mutant `g2_ccalls_no_hoist`.
#[test]
fn g2_ccalls_guard_is_hoisted() {
    use rn_jit::tr::{opt_loop, TraceOpts};
    for src in [
        "local function g(x, y) if x < y then return x + y end return y end
         function f(n) local s = 0 arm() for i = 1, n do local ok, v = pcall(g, i, 100000) s = s + v end return s end
         return f(40)",
    ] {
        let rc = record_src(src, pcall_cfg());
        let r = closed(&rc);
        let before = ccalls_guards(&r.ir);
        assert!(before >= 1, "the recording has a C-stack guard\n{}", dump(&r.ir));
        let mut o = r.clone();
        opt_loop::run(&mut o, &TraceOpts { hdrlib: 1, ..TraceOpts::default() }).unwrap();
        let Some(at) = o.ir.loop_at else {
            panic!("the loop is not peeled\n{}", dump(&o.ir));
        };
        let pre = o.ir.ins[..usize::from(at - REF_BIAS)].iter().filter(|i| i.op == TOp::Lib && (i.aux & 0xFF) == lib::CCALLS).count();
        assert_eq!(ccalls_guards(&o.ir), pre, "no C-stack guard in the loop body\n{}", dump(&o.ir));
        assert!(pre >= 1, "the pre-roll keeps its guards\n{}", dump(&o.ir));
    }
}

/// BUNDLE-H1: integer keys outside the array part that live in the hash part record the integer `HREFV` (a chain walk with the `hashint` main
/// position, an exit when the key is not in the chain) behind the node-count guard, `HLOAD` / `HSTORE` on its node; a key that is absent
/// from the hash part still aborts. Mutants `hrefi_lsize_skip` (the node-count guard), `hashint_nil_meta_skip` (a nil-valued node needs the
/// `__index` guard), `hashint_newindex_skip`.
#[test]
#[cfg(not(rustynail_lua54))]
fn h1_int_hash_keys_record_the_integer_hrefv() {
    let src = "local t = { [0] = 5, [-1] = 6, [1 << 40] = 7, 1 } local m = { [0] = false, [-9] = 1 } m[0] = nil
         function f(n) local s = 0 arm() for i = 1, n do s = s + t[0] + t[-1] + t[1 << 40] t[0] = i if m[0] == nil then s = s + 1 end m[0] = nil end return s end return f(40)";
    let rc = record_src(src, RecCfg { nil_store: true, ..strict() });
    let r = closed(&rc);
    let hv: Vec<(usize, &Ins)> = r.ir.ins.iter().enumerate().filter(|(_, i)| i.op == TOp::Hrefv && aux_bits(i.aux) & HREF_INT != 0).collect();
    assert!(hv.len() >= 5, "integer HREFV for every access\n{}", dump(&r.ir));
    for (n, h) in &hv {
        // the node-count guard: an `EQ` of `FLOAD lsizenode` precedes (otherwise the modulus is wrong for a table with a smaller hash part)
        let node = r.ir.at(h.a);
        assert!(node.op == TOp::Fload && node.b == Field::Node as Ref, "{}", dump(&r.ir));
        let guarded = r.ir.ins[..*n].iter().any(|g| {
            g.op == TOp::Eq && TraceIr::is_ins(g.a) && r.ir.at(g.a).op == TOp::Fload && r.ir.at(g.a).b == Field::Lsizenode as Ref && r.ir.at(g.a).a == node.a
        });
        assert!(guarded, "HREFV {n} without the node-count guard\n{}", dump(&r.ir));
    }
    assert!(r.ir.ins.iter().any(|i| i.op == TOp::Hstore), "{}", dump(&r.ir));
    // the nil-valued node `m[0]` and its store: the metatable guards (`metatable == 0`; with a metatable, the flag-cache guards)
    let mt_guards = r.ir.ins.iter().filter(|g| g.op == TOp::Eq && TraceIr::is_ins(g.a) && r.ir.at(g.a).op == TOp::Fload && r.ir.at(g.a).b == Field::Metatable as Ref).count();
    assert!(mt_guards >= 2, "the metatable guards of the nil-valued reads and stores\n{}", dump(&r.ir));
    // a key that is in neither part is not recorded as a hash access
    let absent = "local t = { [0] = 5, 1 } function f(n) local s = 0 arm() for i = 1, n do s = s + (t[77] or 0) end return s end return f(40)";
    let a = record_src(absent, strict());
    assert!(matches!(a.out, RecOutcome::Aborted(_)), "an absent key does not record");
}

/// A body that changes something before it can leave the trace is not inlined (an exit would repeat the change): the
/// recording aborts `CFunc`. A body whose changes come after its last guard records. Mutant `pcall_impure_ignored`.
#[test]
fn pcall_bodies_with_effects_before_a_guard_abort() {
    let effect_then_guard = [
        "local c = 0 local function g(i) c = c + 1 if i == 99999 then error('x') end return i end
         function f(n) local s = 0 arm() for i = 1, n do local ok, v = pcall(g, i) s = s + v end return s end return f(30)",
        "local t = {} local function g(i) t[1] = i if i == 99999 then error('x') end return i end
         function f(n) local s = 0 arm() for i = 1, n do local ok, v = pcall(g, i) s = s + v end return s end return f(30)",
        "local function g(i) local t = {i} if i == 99999 then error('x') end return #t end
         function f(n) local s = 0 arm() for i = 1, n do local ok, v = pcall(g, i) s = s + v end return s end return f(30)",
    ];
    for src in effect_then_guard {
        assert_eq!(aborted_cfg(src, pcall_cfg()), AbortReason::CFunc, "{src}");
    }
    let late = "local c = 0 local function g(i) if i == 99999 then error('x') end c = c + 1 return i end
         function f(n) local s = 0 arm() for i = 1, n do local ok, v = pcall(g, i) s = s + v end return s end return f(30)";
    closed(&record_src(late, pcall_cfg()));
}
