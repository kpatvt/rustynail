//! J5: the trace compiler (docs/JIT_TRACE.md). Feature `trace` of `rn-jit`.
//!
//! P0 contents: the frozen shared types. [`ir`] (the SSA IR), the [`ExitArea`] layout (5.6), the [`RecVTable`]
//! interface of the recorder hook (4.2; the rn-core side is P1's), [`TraceOpts`] (the option set of
//! `RUSTYNAIL_JIT_TRACE_OPT`, 9.10) and the mutant name registry ([`mutants`]). Everything else (recorder, passes,
//! register allocation, assembly, runtime, the pipeline driver) is added by later tasks; the driver ("Finish" of 4.2)
//! will live in this file.

pub mod asm;
pub mod cache;
pub mod exit;
pub mod fold;
pub mod interp;
pub mod ir;
pub mod mutants;
pub mod opt_loop;
pub mod opt_mem;
pub mod phase;
pub mod print;
pub mod ra;
pub mod record;
pub mod sink;
pub mod snap;

/// Allocatable spill slots in the exit area (5.6).
pub const NSPILL: usize = 256;

/// P2-3c: capacity of the per-state double-constant pool.
pub const FPOOL_N: usize = 256;

/// P2-3c: the per-state pool of double constants that traces load with `[r15 + disp]` instead of building them in a register (`movabs; movq`
/// per use). Append-only (a constant is immutable, so a trace that was flushed leaves its entries valid for the next); full: the caller falls
/// back to the register form. It lives behind the exit area in `rt::BudgetHdr`.
#[repr(C)]
#[derive(Clone, Debug)]
pub struct FPool {
    /// Entries in use.
    pub n: u64,
    /// The bit patterns.
    pub bits: [u64; FPOOL_N],
}

impl FPool {
    /// The empty pool.
    pub const fn new() -> FPool {
        FPool { n: 0, bits: [0; FPOOL_N] }
    }

    /// Index of `b` (appended when new), `None` when full.
    pub fn intern(&mut self, b: u64) -> Option<usize> {
        let n = self.n as usize;
        if let Some(k) = self.bits[..n].iter().position(|&x| x == b) {
            return Some(k);
        }
        if n == FPOOL_N {
            return None;
        }
        self.bits[n] = b;
        self.n += 1;
        Some(n)
    }
}

impl Default for FPool {
    fn default() -> FPool {
        FPool::new()
    }
}

/// Per-state exit area (5.6): where generic exit stubs and the shared exit routine leave their state for
/// `jit_tr_exit`, and the spill slots and helper argument words of a running trace (TR2: one activation per state, so
/// one area suffices). It lives in the boxed per-state `JitData`, [`exit_area::FROM_R15`] bytes after the budget word
/// that `r15` points at. Offsets are frozen ([`exit_area`]).
#[repr(C)]
#[derive(Clone, Debug)]
pub struct ExitArea {
    /// Id of the trace that exits (stored by a generic stub, `St B4` immediate).
    pub trace_id: u32,
    /// Number of the exit (snapshot / stub) within the trace.
    pub exitno: u32,
    /// The allocatable general registers `V0..V10` at the exit (`V9` = `rbp`, `V10` = `r14`).
    pub gpr: [u64; 11],
    /// The double registers `X0..X15` (bit patterns).
    pub xmm: [u64; 16],
    /// Spill slots of the register allocator.
    pub spill: [u64; NSPILL],
    /// Argument words of bracketed helper calls (7.5; `CallH` passes only two `u32` immediates).
    pub args: [u64; 4],
    /// BUNDLE-K L1 (`lazyci`): the region root. 0 = no lazy region is active; else the `CallInfo` of the frame at which the
    /// region of natively pushed (lazy) frames starts: every frame at or below it is complete, the frames above it up to the
    /// running one (`r13`) have only `func` and the caller's `savedpc` stored (docs/JIT_TRACE_REF.md "BUNDLE-K").
    pub lazy_root: u64,
}

/// Byte offsets inside [`ExitArea`] (frozen; asm, exit handler and tests use these, never `offset_of` directly).
pub mod exit_area {
    use super::ExitArea;
    use std::mem::offset_of;

    /// Distance from the budget word (`r15`) to the start of the area. The budget word is an `i32`; the area is
    /// 8-aligned right behind it.
    pub const FROM_R15: i32 = 8;
    /// `trace_id: u32`.
    pub const TRACE_ID: usize = offset_of!(ExitArea, trace_id);
    /// `exitno: u32`.
    pub const EXITNO: usize = offset_of!(ExitArea, exitno);
    /// `gpr: [u64; 11]`.
    pub const GPR: usize = offset_of!(ExitArea, gpr);
    /// `xmm: [u64; 16]`.
    pub const XMM: usize = offset_of!(ExitArea, xmm);
    /// `spill: [u64; NSPILL]`.
    pub const SPILL: usize = offset_of!(ExitArea, spill);
    /// `args: [u64; 4]`.
    pub const ARGS: usize = offset_of!(ExitArea, args);
    /// `lazy_root: u64` (BUNDLE-K L1).
    pub const LAZY_ROOT: usize = offset_of!(ExitArea, lazy_root);
    /// Size of the area.
    pub const SIZE: usize = std::mem::size_of::<ExitArea>();
}

impl Default for ExitArea {
    fn default() -> ExitArea {
        ExitArea {
            trace_id: 0,
            exitno: 0,
            gpr: [0; 11],
            xmm: [0; 16],
            spill: [0; NSPILL],
            args: [0; 4],
            lazy_root: 0,
        }
    }
}

/// The recorder hook interface (4.2) and its helper types now live on the rn-core side (P1, feature `jit_trace`,
/// implied by this crate's `trace`): [`RecStep`], [`Abort`] and [`RecVTable`] with the signatures P0 declared.
pub use rn_core::jit_hooks::{Abort, RecStep, RecVTable};

/// The option set of `RUSTYNAIL_JIT_TRACE_OPT` (9.10): pass switches (all on by default) and stress modes (off by
/// default). Frozen here; the effect of each option is implemented by the task that owns the code (10.1) and
/// [`TraceOpts::parse`] is a plain parser that P6 hardens.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TraceOpts {
    /// `-fold`: constant folding (6.1).
    pub fold: bool,
    /// `-cse`: common subexpression elimination (6.2).
    pub cse: bool,
    /// `-loop`: peeling / LICM; off selects the rolled layout of 7.8.
    pub loop_opt: bool,
    /// `-fwd`: store-to-load forwarding (6.6).
    pub fwd: bool,
    /// `-abc`: bounds-check hoisting (6.5).
    pub abc: bool,
    /// `-sink`: allocation sinking, the GC-observable kill switch (6.8, TR26e).
    pub sink: bool,
    /// `-relaxstr` (N5): checked string loads whose value no consumer needs the short / long variant of guard "some string".
    pub relaxstr: bool,
    /// `+randra=SEED`: random register eviction and refusal.
    pub randra: Option<u64>,
    /// `+exitstorm=N`: every guard fails every N-th execution.
    pub exitstorm: Option<u32>,
    /// `+verify`: runtime assertions (TR2, stack pointer, exit consistency).
    pub verify: bool,
    /// `+emurun`: run every installed trace in the emulator as well.
    pub emurun: bool,
    /// `+emuverify`: compare emulator and native state at each entry.
    pub emuverify: bool,
    /// `+tracemove`: move the stack right before every trace entry.
    pub tracemove: bool,
    /// `+membudget` (PI): keep the budget in the budget word (the memory form of 7.7) instead of in
    /// [`ra::BUDGET_GPR`].
    pub membudget: bool,
    /// `+wb` (S0, TR3b): the slots of the iteration that the next one does not read are stored at the end of the pre-roll and of
    /// the body instead of being carried by PHIs. Off by default (experiment X11).
    pub wb: bool,
    /// `-nestrule`: no nest rule (TR19b(a)): a nest whose outer head is blacklisted keeps its inner traces.
    pub nestrule: bool,
    /// `+allblack` (F5, TR19b measurement): every head starts `Black`; the dispatch stays, nothing is ever recorded.
    pub allblack: bool,
    /// BUNDLE-1 (4): `TrLimits::hdrlib` (set from the limits when a recording starts).
    pub hdrlib: u32,
    /// BUNDLE-2: `TrLimits::sinkmt` (set from the limits when a recording starts): sink tables that get a metatable.
    pub sinkmt: u32,
    /// BUNDLE-5: `TrLimits::sinkside` (set from the limits when a recording starts): allocation sinking in side and stitched traces.
    pub sinkside: u32,
    /// P2 bundle 2: `TrLimits::uvsep` (set from the limits when a recording starts): guarded disjointness of upvalue cells in the loop optimiser.
    pub uvsep: u32,
    /// BUNDLE-L A: `TrLimits::aidx` (set from the limits when a recording starts): the complemented array index is an IR value.
    pub aidx: u32,
    /// BUNDLE-L B: `TrLimits::bhoist` (set from the limits when a recording starts): array bounds checks of a counted loop hoisted into its pre-roll.
    pub bhoist: u32,
}

/// H1: `RUSTYNAIL_JIT_HASHINT=0` turns the integer-key hash access (`HREFV` integer form) off; read once.
pub(crate) fn hashint_enabled() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("RUSTYNAIL_JIT_HASHINT").is_none_or(|v| v != "0"))
}

/// N6: `RUSTYNAIL_JIT_HREFV=0` records every computed key as a constant (the F1 path); read once.
fn hrefv_enabled() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("RUSTYNAIL_JIT_HREFV").is_none_or(|v| v != "0"))
}

impl Default for TraceOpts {
    fn default() -> TraceOpts {
        TraceOpts {
            fold: true,
            cse: true,
            loop_opt: true,
            fwd: true,
            abc: true,
            sink: true,
            relaxstr: true,
            randra: None,
            exitstorm: None,
            verify: false,
            emurun: false,
            emuverify: false,
            tracemove: false,
            membudget: false,
            wb: false,
            nestrule: true,
            allblack: false,
            hdrlib: 0,
            sinkmt: 0,
            sinkside: 0,
            uvsep: 0,
            aidx: 0,
            bhoist: 0,
        }
    }
}

impl TraceOpts {
    /// Lenient parse for the environment (P6): every valid item is applied, every bad one (unknown option, bad
    /// number, a pass switch given twice with opposite signs is not an error) is returned as a message instead of
    /// discarding the whole string.
    pub fn parse_lenient(s: &str) -> (TraceOpts, Vec<String>) {
        let mut o = TraceOpts::default();
        let mut bad = Vec::new();
        for w in s.split([' ', ',', '\t', '\n']).filter(|w| !w.is_empty()) {
            if let Err(e) = TraceOpts::parse_into(&mut o, w) {
                bad.push(e);
            }
        }
        (o, bad)
    }

    /// Parses a `RUSTYNAIL_JIT_TRACE_OPT` value: options separated by spaces or commas. Unknown options and bad
    /// numbers are errors (the message names the offender).
    pub fn parse(s: &str) -> Result<TraceOpts, String> {
        let mut o = TraceOpts::default();
        for w in s.split([' ', ',', '\t', '\n']).filter(|w| !w.is_empty()) {
            TraceOpts::parse_into(&mut o, w)?;
        }
        Ok(o)
    }

    fn parse_into(o: &mut TraceOpts, w: &str) -> Result<(), String> {
        {
            let num = |v: &str| v.parse::<u64>().map_err(|_| format!("bad number in {w:?}"));
            match w {
                "-fold" => o.fold = false,
                "-cse" => o.cse = false,
                "-loop" => o.loop_opt = false,
                "-fwd" => o.fwd = false,
                "-abc" => o.abc = false,
                "-sink" => o.sink = false,
                "-relaxstr" => o.relaxstr = false,
                "-nestrule" => o.nestrule = false,
                "+wb" => o.wb = true,
                "+allblack" => o.allblack = true,
                "+verify" => o.verify = true,
                "+emurun" => o.emurun = true,
                "+emuverify" => o.emuverify = true,
                "+tracemove" => o.tracemove = true,
                "+membudget" => o.membudget = true,
                _ => {
                    if let Some(v) = w.strip_prefix("+randra=") {
                        o.randra = Some(num(v)?);
                    } else if w == "+randra" || w == "+exitstorm" {
                        return Err(format!("missing value in {w:?}"));
                    } else if let Some(v) = w.strip_prefix("+exitstorm=") {
                        let n = num(v)?;
                        if n == 0 || n > u64::from(u32::MAX) {
                            return Err(format!("bad number in {w:?}"));
                        }
                        o.exitstorm = Some(n as u32);
                    } else {
                        return Err(format!("unknown trace option {w:?}"));
                    }
                }
            }
        }
        Ok(())
    }
}

// ---- the pipeline driver (PI, "Finish" of 4.2) ------------------------------------------------------------------

use self::cache::{HeadData, TrId};
use self::exit::{ExitMeta, ProtoInfoR, Resolver};
use self::ra::RaOpts;
use self::record::{AbortReason, RecCfg, RecOutcome, Recorded};
use crate::emu::{Emu, Env, HelperOut, Helpers, Stop, Until};
use crate::ir::{Base, Op, Program, L, M, W};
use crate::x86;
use rn_core::jit_hooks::{self, thread as thr};
use rn_core::jit_layout::callinfo as ci_layout;
use rn_core::state::{CallInfo, Ci, State, Thread};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

/// The recording the driver started on this OS thread (TR16: at most one): what the pipeline needs when the recorder
/// reports back.
struct Active {
    token: u64,
    id: TrId,
    /// Address of the root `Proto`.
    root: usize,
    opts: TraceOpts,
    hot_exit: u32,
    verbose: u8,
    dump: bool,
    /// F2a: the recording is a side trace's (its aborts are judged by [`side_unchangeable`]).
    side: bool,
    /// F2a: the side trace is the head's prelude (`cache::HeadData::pre`).
    prelude: bool,
    /// S1: what the parent's entry checks guarantee (see `cache::SideStart`); only used for a side trace with frames.
    max_extent: u16,
    max_ci: u8,
    /// R2: frameful side traces check their own room (`TrLimits::siderooms`).
    side_rooms: bool,
}

/// A trace installed with `+emurun` / `+emuverify`: the micro-op program the emulator runs.
struct EmuTrace {
    prog: Program,
    entry: L,
    labels: Arc<Vec<usize>>,
    /// Frame slots of the trace (`ir.extent`).
    extent: usize,
    /// `+emuverify` applies only to traces without stores (the heap is then unchanged by the native run).
    verifiable: bool,
    root: usize,
    head_pc: u32,
    /// Address of the exit handler (the emulation intercepts it).
    exit_fn: u64,
    calln_fn: u64,
    entry_fail_fn: u64,
}

/// The entry state of the trace that runs natively under `+emuverify`.
struct VerifyPre {
    slot: u32,
    th: *mut Thread,
    ci: *mut CallInfo,
    /// The frame from slot `-1` (the closure) up to `extent`.
    frame: Vec<u64>,
    /// The budget word and the exit area as they were at entry.
    hdr: Vec<u64>,
    /// `ci.trap` at entry: the exit handler may have armed a side recording (which sets it) before the probe runs, and
    /// the emulated `POLL` reads the live `CallInfo`.
    trap: i32,
}

thread_local! {
    static ACTIVE: RefCell<Option<Active>> = const { RefCell::new(None) };
    /// Emulated traces keyed by (per-state data address, trace slot); replaced when the slot is reused.
    static EMU: RefCell<HashMap<(usize, u32), Rc<EmuTrace>>> = RefCell::new(HashMap::new());
    static VERIFY: RefCell<Option<VerifyPre>> = const { RefCell::new(None) };
}

/// Interns an abort reason (the cache counts reasons by `&'static str`; the set of distinct reasons is small).
fn intern(s: String) -> &'static str {
    use std::sync::Mutex;
    static T: Mutex<Option<HashMap<String, &'static str>>> = Mutex::new(None);
    let mut g = T.lock().unwrap_or_else(|e| e.into_inner());
    let m = g.get_or_insert_with(HashMap::new);
    if let Some(r) = m.get(&s) {
        return r;
    }
    let r: &'static str = Box::leak(s.clone().into_boxed_str());
    m.insert(s, r);
    r
}

/// The name a recorder abort is counted under (4.6; `Nyi` and `Compile` carry their detail).
pub fn abort_name(r: AbortReason) -> &'static str {
    match r {
        AbortReason::Nyi(op) => intern(format!(
            "Nyi({})",
            rn_core::instruction::opcode_from_u8(op)
                .map_or_else(|| format!("op{op}"), |o| format!("{o:?}"))
        )),
        AbortReason::Compile(s) => intern(format!("Compile({s})")),
        AbortReason::CFunc => "CFunc",
        AbortReason::Metamethod => "Metamethod",
        AbortReason::Coerce => "Coerce",
        AbortReason::MetaTable => "MetaTable",
        AbortReason::TableHash => "TableHash",
        AbortReason::TableNil => "TableNil",
        AbortReason::IntMiss => "IntMiss",
        AbortReason::Insert => "Insert",
        AbortReason::Vararg => "Vararg",
        AbortReason::MultRet => "MultRet",
        AbortReason::TailCall => "TailCall",
        AbortReason::Recursion => "Recursion",
        AbortReason::Depth => "Depth",
        AbortReason::LeaveFrame => "LeaveFrame",
        AbortReason::LeaveLoop => "LeaveLoop",
        AbortReason::FnShort => "FnShort",
        AbortReason::InnerLoop => "InnerLoop",
        AbortReason::TooLong => "TooLong",
        AbortReason::Unstable => "Unstable",
        AbortReason::DepFreed => "DepFreed",
        AbortReason::UpvalAlias => "UpvalAlias",
        AbortReason::Hook => "Hook",
        AbortReason::Error => "Error",
        AbortReason::Frame => "Frame",
        AbortReason::Diverge => "Diverge",
        AbortReason::Close => "Close",
        AbortReason::Charge => "Charge",
        AbortReason::Cap => "Cap",
        AbortReason::SideUntraced => "SideUntraced",
        AbortReason::RetfAlias => "RetfAlias",
        AbortReason::SideShort => "SideShort",
        AbortReason::AllocStorm => "AllocStorm",
    }
}

/// Is the cause of this abort a property of the code (so a second occurrence blacklists, TR21) rather than of the
/// data observed by this recording?
fn unchangeable(r: AbortReason) -> bool {
    matches!(
        r,
        AbortReason::Nyi(_)
            | AbortReason::CFunc
            | AbortReason::Vararg
            | AbortReason::MultRet
            | AbortReason::TailCall
            | AbortReason::Recursion
            | AbortReason::Depth
            | AbortReason::TableHash
            | AbortReason::InnerLoop
            | AbortReason::TooLong
            | AbortReason::Compile(_)
    )
}

/// F2a: is the cause of this abort of a *side* recording a property of the code at the exit (so a second occurrence
/// blacklists the exit)? A side trace that reaches the end of its loop, the end of its function or a head without a trace
/// will do so again: the path is the same.
fn side_unchangeable(r: AbortReason) -> bool {
    unchangeable(r)
        || matches!(
            r,
            AbortReason::LeaveLoop
                | AbortReason::LeaveFrame
                | AbortReason::SideShort
        )
}

/// P-B: `RecCfg::storm_step` of a recording started now.
fn storm_step(st: &mut State, guard: u32) -> usize {
    let heap = &st.g().heap;
    if guard == 0 || heap.jit_gc_stress() {
        0
    } else {
        heap.jit_gc_step_hint().max(1)
    }
}

/// BUNDLE-M C1: does the recording starting now run the recorder's lockstep (TR17)? `+verify` and debug builds always do; otherwise
/// `limits.lockstep` = 0 never, 1 always, K >= 2 every K-th recording of the thread.
fn lockstep_on(cfg: &cache::TrConfig) -> bool {
    thread_local!(static N: std::cell::Cell<u32> = const { std::cell::Cell::new(0) });
    let k = cfg.limits.lockstep;
    if cfg.opts.verify || cfg!(debug_assertions) || k == 1 {
        return true;
    }
    if k == 0 {
        return false;
    }
    N.with(|n| {
        let i = n.get();
        n.set(i.wrapping_add(1));
        i % k == 0
    })
}

/// BUNDLE-C: does the closure reach itself through one of its upvalues (`local function f ... f() ... end`)? Such a function recurses: `fnpromo` leaves it to the
/// recursion machinery (a function root of `tail` in awfy/list costs 25% instructions).
fn closure_self_ref(cl: rn_core::value::Gc<rn_core::func::LClosure>) -> bool {
    let me = &*cl.proto() as *const rn_core::func::Proto;
    (0..cl.nupvalues()).any(|i| cl.upval(i).get().as_lclosure().is_some_and(|c| std::ptr::eq(&*c.proto(), me)))
}

/// The recorder entry the cache calls at a hot head (`cache::StartFn`): arms the recorder for the frame `ci` at `pc`.
/// False: nothing armed (a transient refusal for the cache).
pub fn driver_start(st: &mut State, ci: Ci, id: TrId, pc: u32) -> bool {
    let Some(cl) = st.ci_lclosure(ci) else {
        return false;
    };
    let proto = cl.proto();
    let root = &*proto as *const rn_core::func::Proto as usize;
    let cfg = crate::rt::trace_config(st);
    let resolve: record::ResolveFn = Rc::new(|st, p| cache::proto_ref(p).or_else(|| crate::rt::ensure_compiled(st, p).then(|| cache::proto_ref(p)).flatten()));
    let pr = cache::proto_ref(&proto);
    // BUNDLE-C (`fnpromo`): this function-entry head is a generic function root (not for a function that reaches itself through an upvalue)
    let promoted = pr.is_some_and(|pr| cache::fn_promoted(st, pr)) && !closure_self_ref(cl);
    // R2: a continuation head of `recany` records only after a native self-call of the prototype was recorded (otherwise every
    // function with calls would get continuation traces whose return guard fails when no recursion is involved)
    let cok = cfg.limits.contok >= 1 && record::cont_ok(root);
    if cfg.limits.recany != 0 && pr.is_some_and(|pr| cache::head_is_cont(st, pr, pc) && !cache::proto_recursive(st, pr) && !cok) {
        return false;
    }
    let deep = cfg.limits.deep != 0 && cache::rec_deep(st, id) && !mutants::active("deep_limits_ignored");
    let nest: Option<record::HeadQuery> = if cfg.limits.nest >= 1 {
        pr.map(|pr| Rc::new(move |st: &mut State, pc| cache::head_traced(st, pr, pc)) as record::HeadQuery)
    } else {
        None
    };
    let nest_frame: Option<record::FrameHeadQuery> = if cfg.limits.nest >= 2 || cfg.limits.sidenest != 0 {
        Some(Rc::new(|st: &mut State, p: &rn_core::func::Proto, pc| {
            cache::proto_ref(p).and_then(|pr| cache::head_traced(st, pr, pc))
        }))
    } else {
        None
    };
    let rc = RecCfg {
        hrefv: hrefv_enabled(),
        fold: cfg.opts.fold,
        cse: cfg.opts.cse,
        verify: cfg.opts.verify,
        lockstep: lockstep_on(&cfg),
        deps: Some(record::cache_hooks(id, resolve)),
        nest,
        nest_frame,
        nest_frame_roots: cfg.limits.nest >= 2,
        nest_frame_sides: cfg.limits.sidenest != 0,
        stitch_min: cfg.limits.stitchmin,
        unroll: if deep { cfg.limits.unroll.max(cfg.limits.deep) } else { cfg.limits.unroll },
        max_depth: if deep { cfg.limits.maxdepth.max(cfg.limits.deep) } else { cfg.limits.maxdepth },
        max_ins: if deep { cfg.limits.deepins as usize } else { 0 },
        loop_unroll: cfg.limits.loopunroll,
        ncallee: pr.is_some_and(|pr| cache::head_ncallee(st, pr, pc)),
        uvsep: true,
        fn_root: pr.is_some_and(|pr| cache::head_is_fn(st, pr, pc)),
        rec_native: cfg.limits.recnat != 0,
        rec_conts: cfg.limits.recnat != 0,
        rec_any: cfg.limits.recany != 0,
        cnum: true,
        nil_store: cfg.limits.nilstore != 0,
        rec_seen: pr.is_some_and(|pr| cache::proto_recursive(st, pr)) || (cfg.limits.contok >= 2 && cok),
        rec_only: cfg.limits.recnat != 0 && cfg.limits.fnroot == 0 && !promoted && (pc == 0 || cfg.limits.recany != 0),
        rec_cont: pr.is_some_and(|pr| cache::head_is_cont(st, pr, pc)),
        fn_min: if pr.is_some_and(|pr| cache::fn_cent(st, pr)) && !mutants::active("centry_fnmin_kept") { 0 } else { cfg.limits.fnmin },
        pack: true,
        gfor: true,
        mm: true,
        setmt: true,
        f1pow: true,
        f1math: true,
        f1strlen: true,
        g1fmt: true,
        f2rand: true,
        gmatch: true,
        f2byte: true,
        f2asrt: true,
        f2raw: true,
        libnum: true,
        libnext: true,
        libfind: true,
        tfmt: cfg.limits.tfmt != 0,
        libcall: cfg.limits.libcall & 1 != 0,
        libcall_rep: cfg.limits.libcall & 8 != 0,
        libcall_repcls: cfg.limits.libcall & 16 != 0,
        libcall_mask: cfg.limits.libcall,
        // BUNDLE-N: RETF (not with lazy frames: the pop assumes every frame below the running one is complete)
        retf: if cfg.limits.lazyci != 0 { 0 } else { cfg.limits.retf },
        retfmax: cfg.limits.retfmax,
        upbar: cfg.limits.upbar,
        f2mt: true,
        f2cmp: true,
        f1floor: std::is_x86_feature_detected!("sse4.1"),
        tailnat: true,
        closeret: true,
        tailx: 3,
        idxdeep: 7,
        loop_any: cfg.limits.loopany != 0,
        dynmask: cfg.limits.dynmask != 0,
        depth_stitch: true,
        gfor_inl: !cfg!(rustynail_lua54), // (5.4: the end of a traversal is not modelled, see `Rec::tforcall`)
        gfor_lua: true,
        nilload: true,
        b2mt: true,
        amt: cfg.limits.amt != 0,
        side_stitch: true,
        side_stitch_min: cfg.limits.sidestitchmin,
        stitch0: cfg.limits.stitch0 != 0,
        nrinline: true,
        pcall: true,
        landcache: true,
        storm_step: storm_step(st, cfg.limits.allocguard),
        ..RecCfg::default()
    };
    match record::arm(st, ci, pc, rc) {
        Some(token) => {
            ACTIVE.with(|a| {
                *a.borrow_mut() = Some(Active {
                    token,
                    id,
                    root,
                    opts: { let mut o = cfg.opts.clone(); o.hdrlib = cfg.limits.hdrlib | if cfg.limits.tnextinl & 1 != 0 { opt_mem::HDR_TNEXT } else { 0 }; o.sinkmt = cfg.limits.sinkmt; o.sinkside = cfg.limits.sinkside; o.uvsep = 1; o.aidx = cfg.limits.aidx; o.bhoist = if pr.is_some_and(|pr| cache::head_nohoist(st, pr, pc)) { 0 } else { cfg.limits.bhoist }; o },
                    hot_exit: cfg.hot_exit,
                    verbose: cfg.verbose,
                    dump: cfg.dump.is_some(),
                    side: false,
                    prelude: false,
                    max_extent: 0,
                    max_ci: 0,
                    side_rooms: false,
                });
            });
            true
        }
        None => false,
    }
}

/// F2a: the side recorder entry the cache calls at a hot exit (`cache::StartSideFn`): arms the recorder for the frame
/// `ci` at the exit's pc with the exit snapshot's types as known types. `Some(rem)`: armed, `rem` = the instructions the
/// tier-1 label of that pc had prepaid (the handler adds them to the budget word before it leaves for the interpreter).
pub fn driver_start_side(
    st: &mut State,
    ci: Ci,
    id: TrId,
    start: &cache::SideStart,
) -> Option<u32> {
    let cl = st.ci_lclosure(ci)?;
    let proto = cl.proto();
    let root = &*proto as *const rn_core::func::Proto as usize;
    let pr = cache::proto_ref(&proto)?;
    let cfg = crate::rt::trace_config(st);
    let low = cache::landing_cached(&proto)?;
    // S1: a start inside inlined frames lands at a label of the innermost frame's prototype
    let inner_low = if start.frames.is_empty() {
        low.clone()
    } else {
        let icl = st.ci_lclosure(st.ci())?;
        let ip = icl.proto();
        cache::landing_cached(&ip)?
    };
    if inner_low.label_kind(start.pc) == crate::lower::LabelKind::Dead {
        return None;
    }
    let rem = snap::land_info(&inner_low, start.pc).1;
    let resolve: record::ResolveFn = Rc::new(|st, p| cache::proto_ref(p).or_else(|| crate::rt::ensure_compiled(st, p).then(|| cache::proto_ref(p)).flatten()));
    let hs: record::HeadQuery = Rc::new(move |st, pc| cache::head_traced(st, pr, pc));
    let fn_side = cache::head_is_fn(st, pr, 0);
    let deep = cfg.limits.deep != 0 && cache::rec_deep(st, id) && !mutants::active("deep_limits_ignored");
    let rc = RecCfg {
        hrefv: hrefv_enabled(),
        fold: cfg.opts.fold,
        cse: cfg.opts.cse,
        verify: cfg.opts.verify,
        lockstep: lockstep_on(&cfg),
        deps: Some(record::cache_hooks(id, resolve)),
        landing: Some(low),
        side: Some(record::SideCfg {
            known: start.known.clone(),
            head_state: hs,
            // a prelude replaces a failing entry (type_entry): it pays at any length
            min_len: if start.prelude { 0 } else { cfg.limits.sidemin },
            frames: start.frames.clone(),
            max_extent: start.max_extent,
            max_ci: start.max_ci,
            frame_min: cfg.limits.framemin,
        }),
        loop_unroll: cfg.limits.loopunroll,
        // C38 slice 1: a side recording that reaches a callee loop head with a root trace ends with the frame-linking exit
        nest_frame: if cfg.limits.sidenest != 0 || (cfg.limits.retf & 1 != 0 && cfg.limits.lazyci == 0) {
            Some(Rc::new(|st: &mut State, p: &rn_core::func::Proto, pc| cache::proto_ref(p).and_then(|pr| cache::head_traced(st, pr, pc))) as record::FrameHeadQuery)
        } else {
            None
        },
        // N3a: after a `RETF` the root frame is the caller: a loop head of ITS prototype
        retf_head: if cfg.limits.retf & 1 != 0 && cfg.limits.lazyci == 0 && !start.no_retf {
            Some(Rc::new(|st: &mut State, p: &rn_core::func::Proto, pc| cache::proto_ref(p).and_then(|pr| cache::head_traced(st, pr, pc))) as record::FrameHeadQuery)
        } else {
            None
        },
        nest_frame_sides: cfg.limits.sidenest != 0,
        // P-E2: a side trace of a function-entry root ends before the root frame's `RETURN` (exit, no link)
        pack: true,
        gfor: true,
        mm: true,
        setmt: true,
        f1pow: true,
        f1math: true,
        f1strlen: true,
        g1fmt: true,
        f2rand: true,
        gmatch: true,
        f2byte: true,
        f2asrt: true,
        f2raw: true,
        libnum: true,
        libnext: true,
        libfind: true,
        tfmt: cfg.limits.tfmt != 0,
        libcall: cfg.limits.libcall & 1 != 0,
        libcall_rep: cfg.limits.libcall & 8 != 0,
        libcall_repcls: cfg.limits.libcall & 16 != 0,
        libcall_mask: cfg.limits.libcall,
        // BUNDLE-N: RETF (not with lazy frames: the pop assumes every frame below the running one is complete)
        retf: if cfg.limits.lazyci != 0 || start.no_retf { 0 } else { cfg.limits.retf },
        retfmax: cfg.limits.retfmax,
        upbar: cfg.limits.upbar,
        f2mt: true,
        f2cmp: true,
        f1floor: std::is_x86_feature_detected!("sse4.1"),
        tailnat: true,
        closeret: true,
        tailx: 3,
        idxdeep: 7,
        loop_any: cfg.limits.loopany != 0,
        dynmask: cfg.limits.dynmask != 0,
        depth_stitch: true,
        gfor_inl: !cfg!(rustynail_lua54), // (5.4: the end of a traversal is not modelled, see `Rec::tforcall`)
        gfor_lua: true,
        nilload: true,
        b2mt: true,
        amt: cfg.limits.amt != 0,
        side_stitch: true,
        side_stitch_min: cfg.limits.sidestitchmin,
        stitch0: cfg.limits.stitch0 != 0,
        nrinline: true,
        pcall: true,
        landcache: true,
        fn_root: fn_side,
        rec_native: cfg.limits.recnat != 0 && fn_side,
        rec_conts: cfg.limits.recnat != 0 && fn_side,
        rec_any: cfg.limits.recany != 0,
        cnum: true,
        nil_store: cfg.limits.nilstore != 0,
        rec_seen: cache::proto_recursive(st, pr),
        unroll: if deep { cfg.limits.unroll.max(cfg.limits.deep) } else { cfg.limits.unroll + start.ubump },
        // (BUNDLE-N: a side recording honours `maxdepth` too; it read 0 = the compiled-in 4 unless a function side trace)
        max_depth: if deep { cfg.limits.maxdepth.max(cfg.limits.deep) } else { cfg.limits.maxdepth },
        max_ins: if deep { cfg.limits.deepins as usize } else { 0 },
        storm_step: storm_step(st, cfg.limits.allocguard),
        ..RecCfg::default()
    };
    let token = record::arm(st, ci, start.pc, rc)?;
    ACTIVE.with(|a| {
        *a.borrow_mut() = Some(Active {
            token,
            id,
            root,
            opts: { let mut o = cfg.opts.clone(); o.hdrlib = cfg.limits.hdrlib | if cfg.limits.tnextinl & 1 != 0 { opt_mem::HDR_TNEXT } else { 0 }; o.sinkmt = cfg.limits.sinkmt; o.sinkside = cfg.limits.sinkside; o.uvsep = 1; o.aidx = cfg.limits.aidx; o.bhoist = cfg.limits.bhoist; o },
            hot_exit: cfg.hot_exit,
            verbose: cfg.verbose,
            dump: cfg.dump.is_some(),
            side: true,
            prelude: start.prelude,
            max_extent: start.max_extent,
            max_ci: start.max_ci,
            side_rooms: cfg.limits.siderooms != 0,
        });
    });
    Some(rem)
}

/// `RecVTable::step` of the driver: the recorder's step; when it ends the recording the pipeline runs (the
/// `Flow::Closed` arm of 4.2) before `Leave` returns to the VM, which then requests the entry into the new trace.
pub fn driver_step(st: &mut State, ci: Ci, pc: *const u32, token: u64) -> RecStep {
    let t0 = phase::now();
    let r = record::step(st, ci, pc, token);
    phase::add(phase::REC_STEP, t0);
    if r == RecStep::Leave {
        finish_outcome(st);
    }
    r
}

/// `RecVTable::abort` of the driver: the VM ended the recording (error, hook, frame switch, close).
pub fn driver_abort(st: &mut State, why: Abort, token: u64) {
    record::abort(st, why, token);
    finish_outcome(st);
}

/// The recorder VTABLE with the pipeline attached (what [`install_driver`] registers).
pub const DRIVER_VTABLE: RecVTable = RecVTable {
    step: driver_step,
    abort: driver_abort,
};

/// Registers the driver for the process: the VM's recorder VTABLE and the cache's default recorder entry (first
/// registration wins; a test that installed its own recorder first is not disturbed).
pub fn install_driver() -> bool {
    jit_hooks::set_type_mt_hook(cache::type_mt_changed);
    let a = jit_hooks::install_rec(DRIVER_VTABLE);
    let b = cache::install_recorder(driver_start);
    cache::install_side_recorder(driver_start_side);
    a && b
}

/// Makes `st` use the driver's recorder entry (tests: the process-wide default is installed only when the environment
/// switches traces on).
pub fn attach_driver(st: &mut State) {
    jit_hooks::set_type_mt_hook(cache::type_mt_changed);
    cache::set_starter(st, Some(driver_start));
    cache::set_side_starter(st, Some(driver_start_side));
}

fn finish_outcome(st: &mut State) {
    let Some((tok, out)) = record::take_outcome() else {
        return;
    };
    let Some(act) = ACTIVE.with(|a| {
        let mut a = a.borrow_mut();
        match &*a {
            Some(x) if x.token == tok => a.take(),
            _ => None,
        }
    }) else {
        return; // not the driver's recording (a test recorder), or a stale outcome
    };
    match out {
        RecOutcome::Aborted(why) if act.side && record::take_retf_used() => {
            cache::abort_retf(st, act.id, abort_name(why));
        }
        RecOutcome::Aborted(why) => match why {
            AbortReason::Unstable => cache::abort_unstable(st, act.id),
            AbortReason::FnShort if !act.side => {
                cache::abort_recording(st, act.id, "FnShort", unchangeable(why));
            }
            w => cache::abort_recording(
                st,
                act.id,
                abort_name(w),
                if act.side {
                    side_unchangeable(w)
                } else {
                    unchangeable(w)
                },
            ),
        },
        RecOutcome::Closed(rec) => {
            if let Err(e) = pipeline(st, &act, &rec) {
                if act.verbose >= 2 {
                    eprintln!("[trace] --- pipeline: {}", e.detail);
                }
                match e.kind {
                    Fail::Abort(why) if act.side && !rec.ir.segs.is_empty() => {
                        cache::abort_retf(st, act.id, abort_name(why));
                    }
                    Fail::Abort(why) => {
                        let u = if act.side {
                            side_unchangeable(why)
                        } else {
                            unchangeable(why)
                        };
                        cache::abort_recording(st, act.id, abort_name(why), u);
                    }
                    // `install` already finished the recording (aborted it with its own reason)
                    Fail::Done => {}
                }
            }
        }
    }
}

enum Fail {
    /// Abort the recording (counted, penalty per TR21).
    Abort(AbortReason),
    /// The recording is already finished (`install` aborted it, or the trace was flushed).
    Done,
}

struct PipeErr {
    kind: Fail,
    detail: String,
}

fn abort_err(why: &'static str, detail: impl Into<String>) -> PipeErr {
    PipeErr {
        kind: Fail::Abort(AbortReason::Compile(why)),
        detail: detail.into(),
    }
}

/// The optimisation passes after recording (6): `fold` and `cse` run inside the recorder's builder
/// ([`RecCfg::fold`] / [`RecCfg::cse`]); P7's loop / forwarding / bounds-check passes are one line each in this list.
type Pass = fn(&mut Recorded, &TraceOpts) -> Result<(), String>;
/// The post-recording passes, in order. `entry_checks` and `split_type_snaps` are the PI passes that give the rolled
/// layout the entry / type-guard structure 3.3 relies on (see their documentation); P7 adds its passes here.
const PASSES: &[(&str, Pass)] = &[
    ("sink", sink::run),
    ("entry_checks", entry_checks),
    ("loop", opt_loop::run),
    ("split_type_snaps", split_type_snaps),
    ("relaxstr", relaxstr),
];

/// N5: string loads that do not specialise on the short / long variant. `ALOAD` / `HLOAD` type checks of a string (`ShrStr` or
/// `LngStr` by what the recording saw) compare the full tag, so a table whose strings straddle `LUAI_MAXSHORTLEN` made the
/// guard fail on every other load (a 30x cliff, bench/lj/cliff_mixed_strings.lua). A loaded string whose **every use** is a
/// plain snapshot slot (the interpreter state at an exit), or a `PHI` that is itself such a value, never has its variant
/// observed by the trace: the guard then checks "some string" (`F_ANYSTR`, tag with the variant bit masked) and the restore
/// reads the tag from the object header. Any other use (`Tlen`: `shrlen` / `u.len`, string equality: interned or not,
/// table keys, stores, concatenation, helper calls, the writeback, sunk fields) keeps the exact guard. Greatest fixed point: a
/// load stays relaxed only while all its `PHI` users do.
fn relaxstr(rec: &mut Recorded, opts: &TraceOpts) -> Result<(), String> {
    use ir::{aux_bits, TOp, Ty, F_ANYSTR, F_GUARD, F_OBS, REF_BIAS, SNAP_PSEUDO, SNAP_SUNK_ANY};
    if !opts.relaxstr {
        return Ok(());
    }
    let any_use = mutants::active("relaxstr_any_use");
    let ir = &mut rec.ir;
    let n = ir.ins.len();
    let is_str = |t: Ty| matches!(t, Ty::ShrStr | Ty::LngStr);
    let idx = |r: ir::Ref| usize::from(r - REF_BIAS);
    // candidates: checked string loads and the string PHIs
    let mut cand = vec![false; n];
    for (i, ins) in ir.ins.iter().enumerate() {
        cand[i] = is_str(ins.t)
            && match ins.op {
                TOp::Aload | TOp::Hload => ins.flags & F_GUARD != 0 && (ins.flags & F_OBS == 0 || any_use) && aux_bits(ins.aux) & 1 != 0 && aux_bits(ins.aux) & 2 == 0,
                TOp::Phi => true,
                _ => false,
            };
    }
    if !cand.iter().any(|&c| c) {
        return Ok(());
    }
    // uses that keep the exact variant (and everything a PHI user drags along)
    let mut changed = true;
    while changed {
        changed = false;
        let mut need = vec![false; n];
        for (j, ins) in ir.ins.iter().enumerate() {
            let (ra, rb) = ins.op.ref_operands();
            let ops = [
                if ra { ins.a } else { ir::NOREF },
                if rb { ins.b } else { ir::NOREF },
                ins.third().unwrap_or(ir::NOREF),
            ];
            for r in ops {
                if !ir::TraceIr::is_ins(r) {
                    continue;
                }
                let x = idx(r);
                // a PHI of a candidate PHI-or-load is the only consumer that keeps a value relaxed
                if !(ins.op == TOp::Phi && cand[j]) {
                    need[x] = true;
                }
            }
        }
        for e in &ir.snapmap {
            if (e.flags & SNAP_SUNK_ANY != 0 || e.slot >= SNAP_PSEUDO) && ir::TraceIr::is_ins(e.r) {
                need[idx(e.r)] = true;
            }
        }
        for e in &rec.writeback {
            if ir::TraceIr::is_ins(e.r) {
                need[idx(e.r)] = true;
            }
        }
        for x in 0..n {
            if cand[x] && need[x] && !any_use {
                cand[x] = false;
                changed = true;
            }
        }
    }
    // a PHI is relaxed only with both inputs candidates or non-candidates that are strings (a non-load input keeps its static type,
    // the restore reads the tag of the object anyway)
    for x in 0..n {
        if cand[x] {
            ir.ins[x].flags |= F_ANYSTR;
        }
    }
    Ok(())
}

/// Moves every checked `SLOAD` (the trace's live-in type checks) to the start of the iteration, right behind `POLL` and
/// `BUDGET`, and gives them all the *entry snapshot* (empty map at `H`, `Land::Pre`, `refund = n`: the state before the
/// iteration, after the charge). Sound in the rolled layout because a slot is read from the stack only until it is
/// first written, so its `SLOAD` observes the iteration's entry value wherever it sits, and a failing check at the
/// start leaves through the same state as the iteration's own first instruction would.
///
/// Why: the recorder puts each `SLOAD` at its first use, with a snapshot that holds every slot written before it (the
/// temporaries of earlier instructions). Such an exit is not an *entry check* for the assembler (empty map, 3.3), so
/// the free-peel bookkeeping (`pending` / `entry_fail`) never applied to it, and a live-in whose type differs at
/// every entry (`type_entry`: `local a = 0` that turns float) was counted as a hot type-guard exit and re-recorded
/// forever. With the check at the entry, persistent failures are handled by the entry-failure rule of 3.3 instead.
fn entry_checks(rec: &mut Recorded, _opts: &TraceOpts) -> Result<(), String> {
    use ir::{aux_bits, guard_aux, TOp, F_GUARD, REF_BIAS};
    let ir = &mut rec.ir;
    if ir.loop_at.is_some() {
        return Ok(()); // peeled layouts (P7) have their own entry segment
    }
    let checked =
        |i: &ir::Ins| i.op == TOp::Sload && i.flags & F_GUARD != 0 && aux_bits(i.aux) & 1 != 0;
    // POLL and BUDGET first
    let p = ir
        .ins
        .iter()
        .take_while(|i| matches!(i.op, TOp::Poll | TOp::Budget))
        .count();
    // (N2b: only the loads before the first `RETF` are entry values: later ones read the caller's frame)
    let first_retf = if ir.segs.is_empty() || mutants::active("retf_entry_checks_past") { ir.ins.len() } else { ir.ins.iter().position(|i| i.op == TOp::Retf).unwrap_or(ir.ins.len()) };
    let moved: Vec<usize> = (p..first_retf).filter(|&i| checked(&ir.ins[i])).collect();
    if moved.is_empty() {
        return Ok(());
    }
    let es = match rec.entry_snap {
        Some(e) => e,
        None => {
            let s = ir::Snap {
                pc: ir.head.1,
                land: ir::Land::Pre,
                refund: rec.n as i32,
                ..ir::Snap::default()
            };
            let e = ir.snapshot(s, &[]);
            rec.snap_step.push(0);
            rec.entry_snap = Some(e);
            e
        }
    };
    for &i in &moved {
        let bits = aux_bits(ir.ins[i].aux);
        ir.ins[i].aux = guard_aux(es, bits);
    }
    // new order: the prefix, the checks, everything else
    let mut order: Vec<usize> = (0..p).collect();
    order.extend(moved.iter().copied());
    order.extend((p..ir.ins.len()).filter(|i| !moved.contains(i)));
    if order.iter().copied().eq(0..ir.ins.len()) {
        return Ok(());
    }
    let mut newidx = vec![0usize; order.len()];
    for (n, &o) in order.iter().enumerate() {
        newidx[o] = n;
    }
    let map = |r: ir::Ref| -> ir::Ref {
        if r >= REF_BIAS {
            REF_BIAS + newidx[usize::from(r - REF_BIAS)] as ir::Ref
        } else {
            r
        }
    };
    let old = std::mem::take(&mut ir.ins);
    for &o in &order {
        let mut i = old[o];
        let (ra, rb) = i.op.ref_operands();
        if ra {
            i.a = map(i.a);
        }
        if rb {
            i.b = map(i.b);
        }
        if let Some(v) = i.third() {
            i.aux = i.aux_with_third(map(v));
        }
        i.prev = map(i.prev);
        ir.ins.push(i);
    }
    for e in &mut ir.snapmap {
        e.r = map(e.r);
    }
    for e in &mut rec.writeback {
        e.r = map(e.r);
    }
    if let Some(l) = ir.loop_at.as_mut() {
        *l = map(*l);
    }
    Ok(())
}

/// Gives every `ALOAD` / `ULOAD` type check its own snapshot (a copy of its instruction's). The recorder shares one
/// snapshot between all guards of an instruction (TR3a), and exits are counted and classified per snapshot, so the
/// bounds and metatable guards of `t[i]` made its element type check look like any other exit: the hot type-guard
/// rule of 3.3 (flush and re-record from the observed types) never fired for `type_late`. A guard that is the only
/// user of its snapshot keeps it.
fn split_type_snaps(rec: &mut Recorded, _opts: &TraceOpts) -> Result<(), String> {
    use ir::{aux_bits, aux_snap, guard_aux, TOp, F_GUARD};
    let ir = &mut rec.ir;
    let nsnaps = ir.snaps.len();
    let mut users = vec![0u32; nsnaps];
    for i in &ir.ins {
        if i.flags & F_GUARD != 0 {
            users[aux_snap(i.aux) as usize] += 1;
        }
    }
    for n in 0..ir.ins.len() {
        let i = ir.ins[n];
        let tc = matches!(i.op, TOp::Aload | TOp::Uload | TOp::Hload)
            && i.flags & F_GUARD != 0
            && aux_bits(i.aux) & 1 != 0;
        if !tc {
            continue;
        }
        let s = aux_snap(i.aux);
        if users[s as usize] <= 1 {
            continue;
        }
        if ir.snaps.len() >= ir::MAX_SNAPS - 1 {
            return Err("too many snapshots".into());
        }
        let src = ir.snaps[s as usize];
        let entries: Vec<ir::SnapEntry> = ir.snap_entries(s).to_vec();
        let links: Vec<ir::FrameLink> = snap::frame_links(ir, s).to_vec();
        // the new snapshot is the last one, so its frame links go last too (links are in snapshot order)
        let ns = ir.snapshot(src, &entries);
        ir.frames.extend(links);
        rec.snap_step.push(rec.snap_step[s as usize]);
        users[s as usize] -= 1;
        ir.ins[n].aux = guard_aux(ns, aux_bits(i.aux));
    }
    Ok(())
}

struct Res {
    protos: HashMap<usize, (ProtoInfoR, Vec<Option<u64>>)>,
}

impl Resolver for Res {
    fn proto(&self, key: ir::ProtoKey) -> Option<ProtoInfoR> {
        self.protos.get(&key.0).map(|p| p.0)
    }
    fn land_addr(&self, key: ir::ProtoKey, pc: u32, _land: ir::Land) -> Option<u64> {
        self.protos
            .get(&key.0)?
            .1
            .get(pc as usize)
            .copied()
            .flatten()
    }
}

fn pipeline(st: &mut State, act: &Active, rec: &Recorded) -> Result<(), PipeErr> {
    let t0 = phase::now();
    let r = pipeline_inner(st, act, rec);
    phase::add(phase::PIPELINE, t0);
    r
}

fn pipeline_inner(st: &mut State, act: &Active, rec: &Recorded) -> Result<(), PipeErr> {
    let opts = &act.opts;
    let mut rec = rec.clone();
    let side = rec.side;
    // F2b: a root that ends with a link (a nest's outer loop): head bookkeeping and entry checks of a root, the end of a
    // side trace
    let link_root = side.is_some() && !act.side;
    // (a stitched *side* trace ends like a side trace: with an exit and no link, P-E2)
    let stitched = rec.stitched && side.is_none();
    // F2a: a side trace is a straight rolled segment: no entry-check hoisting, no peeling (it has no loop), no type
    // snapshot splitting (its type-guard exits keep the prototype's rule, see `cache::exit_hot`)
    if side.is_some() || stitched {
        // BUNDLE-5 (`sinkside`): allocation sinking is the one pass of the list that also runs on side and stitched traces
        let t0 = phase::now();
        let r = sink::run(&mut rec, opts).map_err(|e| abort_err("pass", e));
        phase::add(phase::PASS_SINK, t0);
        r?;
    }
    if side.is_none() && !stitched {
        for (name, pass) in PASSES {
            let t0 = phase::now();
            let r = pass(&mut rec, opts).map_err(|e| abort_err("pass", e));
            phase::add(if *name == "sink" { phase::PASS_SINK } else { phase::PASS_OTHER }, t0);
            r?;
        }
    } else if link_root || stitched {
        entry_checks(&mut rec, opts).map_err(|e| abort_err("pass", e))?;
        split_type_snaps(&mut rec, opts).map_err(|e| abort_err("pass", e))?;
    } else if side.is_some_and(|s| s.start_rem == 0 && !s.frameful) {
        // a side trace that starts at a pre-count (head) label gets its live-in type checks hoisted to the start like a
        // rolled root (the pass builds the entry snapshot at that label); one that starts mid-block keeps the checks
        // at their first use
        entry_checks(&mut rec, opts).map_err(|e| abort_err("pass", e))?;
    }
    if side.is_some() || stitched {
        // H1: forwarding / load CSE on the rolled segment of a side or stitched trace
        opt_loop::run_fwd(&mut rec, opts).map_err(|e| abort_err("pass", e))?;
    }
    let ir = &rec.ir;
    if cache::verbose_aborts() {
        eprintln!("[trace] install: extent {} depth {} (parent {} {})", ir.extent, ir.depth, act.max_extent, act.max_ci);
    }
    let mut side_room = false;
    if let Some(si) = side.filter(|s| s.frameful) {
        // S1: a side trace entered with virtual frames has no entry checks: it may not need more stack or spare `CallInfo`
        // nodes than the parent's checks guarantee. R2 (`siderooms`): one that does checks the room itself at its start.
        let _ = si;
        if (ir.extent > act.max_extent || ir.depth > act.max_ci) && !crate::tr::mutants::active("fside_room_ignored") {
            if !act.side_rooms {
                return Err(abort_err("side-room", format!("extent {} > {} or depth {} > {}", ir.extent, act.max_extent, ir.depth, act.max_ci)));
            }
            side_room = true;
        }
    }
    let end: Vec<ir::Ref> = if ir.loop_at.is_none() {
        rec.writeback.iter().map(|e| e.r).collect()
    } else {
        Vec::new()
    };
    let vbreg = (!opts.membudget).then_some(ra::BUDGET_GPR);
    let ropts = RaOpts {
        reserve_gpr: vbreg,
        // SAFETY: the per-state data is live (see `rt::data_ptr`); read-only.
        xregs: unsafe { (*crate::rt::data_ptr(st)).tr.cfg.limits.xregs } as u8,
        ..RaOpts::from_trace(opts)
    };
    let t_ra = phase::now();
    let mut al = ra::allocate(ir, &end, &ropts).map_err(|e| {
        abort_err(
            match e {
                ra::RaError::Phi => "phi",
                ra::RaError::Spill => "spill",
                ra::RaError::BadIr(_) => "ra-badir",
                ra::RaError::Pressure => "ra-pressure",
            },
            format!("ra: {e:?}"),
        )
    })?;
    // P2 bundle 1: `r14` (`V10`) costs a reload from `ci.kcache` in front of every jump out of the trace (a dependent load right before tier 1 reads its
    // constants; `rbp` (`V9`) costs nothing). A trace keeps the allocation with `V10` only if it removes a loop-carried memory round trip (a PHI in a spill
    // slot, a back-edge move through one) that the allocation without it has: spills off the loop-carried chain are cheap (store forwarding) and
    // never paid for the reload on the short, exit-heavy traces of the awfy / corpus programs (+2-6 % cycles measured).
    if ropts.xregs & 2 != 0 && al.uses_gpr(10) && !mutants::active("xregs_rec_r14") {
        let without = RaOpts { xregs: ropts.xregs & !2, ..ropts.clone() };
        if let Ok(a2) = ra::allocate(ir, &end, &without) {
            if a2.carried_spills(ir) <= al.carried_spills(ir) {
                al = a2;
            }
        }
    }
    phase::add(phase::RA, t_ra);
    // tier-1 label addresses of the root and every inlined prototype (TR4: the code version installed now)
    let mut res = Res {
        protos: HashMap::new(),
    };
    for k in ir.deps.iter().map(|k| k.0).chain([act.root]) {
        if res.protos.contains_key(&k) {
            continue;
        }
        // SAFETY: the keys are live prototypes: the root is the running closure's, the others are registered
        // dependencies (TR32), flushed before they are freed.
        let p = unsafe { &*(k as *const rn_core::func::Proto) };
        let pr =
            cache::proto_ref(p).ok_or_else(|| abort_err("no-descriptor", format!("{k:#x}")))?;
        let addrs: Vec<Option<u64>> = cache::with_tr(st, pr, |tp| {
            (0..p.code().len() as u32)
                .map(|pc| tp.pc_addr(pc))
                .collect()
        })
        .ok_or_else(|| abort_err("no-descriptor", format!("{k:#x}")))?;
        res.protos.insert(
            k,
            (
                ProtoInfoR {
                    code: p.code_ptr(),
                    k: p.k_ptr().cast(),
                    maxstack: u16::from(p.maxstacksize),
                },
                addrs,
            ),
        );
    }
    let t_ex = phase::now();
    let (mut snaps, mut descs) =
        exit::build_exits(ir, &al, &res).map_err(|e| abort_err("exits", e))?;
    phase::add(phase::EXITS, t_ex);
    // The entry snapshot's exits are entry-check failures: the entry-failure rule of 3.3 (`entry_fail` / free peel)
    // decides about them, not the hot type-guard rule, so they never flush.
    if let Some(e) = rec.entry_snap {
        for d in descs.iter_mut().filter(|d| d.snap == e) {
            d.site = cache::ExitSite::Other;
        }
    }
    let counters = cache::alloc_exit_counters(st, act.id, snaps.len())
        .ok_or_else(|| abort_err("counters", "alloc_exit_counters"))?;
    let pc = ir.head.1;
    let root = act.root;
    // SAFETY: the root prototype is live (running frame).
    let rootp = unsafe { &*(root as *const rn_core::func::Proto) };
    let rpr = cache::proto_ref(rootp).ok_or_else(|| abort_err("no-descriptor", "root"))?;
    // TR8: every exit continues through its cell, which starts at the tier-1 landing address. A prelude (F2a) is
    // reached from the head's dispatch cell: when its entry checks fail it continues in the root trace's entry, which
    // is what tier 1 would have reached through the dispatch cell without it. That holds for the snapshots that are
    // the state at the head (empty map, the head's pc): the entry checks and the budget exits.
    let mut init: Vec<usize> = snaps.iter().map(|s| s.land_addr as usize).collect();
    // S1: the cell of an exit with virtual-able frames is 0 until a side trace is attached (the stub / handler then
    // restores the frames); a trace that ends with a native frame push or a function-entry root keeps its landings
    let frame_cells_on = (!ir.fn_root && ir.rec_call.is_none() && ir.rec_ret.is_none()) || !crate::tr::mutants::active("fcfn_cells_off");
    if frame_cells_on {
        for (k, (c, s)) in init.iter_mut().zip(&snaps).enumerate() {
            let linking = side.is_some_and(|si| si.frame_link && si.link_snap as usize == k);
            if s.frame_side_ok() && !linking {
                *c = 0;
            }
        }
    }
    if act.prelude && !crate::tr::mutants::active("prelude_no_chain") {
        let (root_entry, _) = cache::head_entry(st, rpr, pc)
            .filter(|e| e.0 != 0)
            .ok_or_else(|| abort_err("no-root-entry", format!("pc {pc}")))?;
        for (c, s) in init.iter_mut().zip(&snaps) {
            if s.slots.is_empty() && s.frames.is_empty() && s.pc == pc {
                *c = root_entry;
            }
        }
    }
    if act.verbose >= 2 && act.side {
        eprintln!(
            "[trace] side cells: prelude {} pc {pc} init {:x?} snaps {:?}",
            act.prelude,
            init,
            snaps
                .iter()
                .map(|s| (s.pc, s.slots.len(), s.frames.len()))
                .collect::<Vec<_>>()
        );
    }
    let cells = cache::alloc_exit_cells(st, act.id, &init)
        .ok_or_else(|| abort_err("cells", "alloc_exit_cells"))?;
    let (head, pcl) = if act.side {
        // a side trace books no entry counters; a failing TR6 check lands at the tier-1 label it was entered at
        let si = side.expect("side recording without SideInfo");
        let pcl = res
            .land_addr(si.start_proto, si.start_pc, ir::Land::Pre)
            .ok_or_else(|| abort_err("no-label", format!("pc {}", si.start_pc)))?;
        (0, pcl)
    } else {
        let (head, pcl) = cache::with_tr(st, rpr, |tp| {
            tp.head_index(pc).map(|i| {
                (
                    tp.head(i) as *const HeadData as u64,
                    tp.head(i).pcl_addr.get() as u64,
                )
            })
        })
        .flatten()
        .ok_or_else(|| abort_err("no-head", format!("pc {pc}")))?;
        (head, pcl)
    };
    let side_env = match side {
        None => None,
        // P-E2: a side trace that ends with its always-taken exit before the root `RETURN` has no link target
        Some(si) if si.no_link => Some(asm::SideEnv {
            link_cell: 0,
            link_snap: si.link_snap,
        }),
        Some(si) => {
            // the link goes through the dispatch cell of the head the trace ends at (a prelude ends with a jump through
            // the `entry` cell of the head: the root trace's entry, skipping the dispatch; any other trace goes through
            // the dispatch cell, which is whatever tier 1 would reach)
            let prelude = act.prelude && si.link_pc == pc && si.link_proto.0 == root;
            let tpr = if si.link_proto.0 == root {
                rpr
            } else {
                // SAFETY: an inlined callee of the trace, registered as a dependency (TR32) and live.
                let p = unsafe { &*(si.link_proto.0 as *const rn_core::func::Proto) };
                cache::proto_ref(p).ok_or_else(|| abort_err("no-descriptor", "link proto"))?
            };
            let link_cell = cache::with_tr(st, tpr, |tp| {
                tp.head_index(si.link_pc).map(|i| {
                    if prelude {
                        &tp.head(i).entry as *const std::cell::Cell<usize> as u64
                    } else {
                        &tp.head(i).cell as *const std::cell::Cell<usize> as u64
                    }
                })
            })
            .flatten()
            .ok_or_else(|| abort_err("no-head", format!("link pc {}", si.link_pc)))?;
            if si.frame_link && !crate::tr::mutants::active("frame_link_cell_ignored") {
                // the exit continues at the dispatch cell of the callee's head
                snaps[si.link_snap as usize].link_cell = link_cell;
            }
            Some(asm::SideEnv {
                link_cell,
                link_snap: si.link_snap,
            })
        }
    };
    // P-M3: the continuation heads of the root prototype (the targets of a native return), and the function-entry head's
    // dispatch cell (the target of the native self-call of a continuation trace)
    let lazy = cache::proto_lazy(st, rpr);
    // BUNDLE-K L1: the `callstatus` every lazy frame of this prototype gets when all of its `CALL`s want the same number of results
    let lazy_cs = if lazy {
        let mut cs: Option<u32> = None;
        let mut same = true;
        for &w in rootp.code() {
            let i = rn_core::instruction::Instruction(w);
            if i.opcode() == Some(rn_core::ver::active::OpCode::Call) {
                match cs {
                    None => cs = Some(i.c()),
                    Some(c) if c != i.c() => same = false,
                    _ => {}
                }
            }
        }
        if same { cs } else { None }
    } else {
        None
    };
    let mut rec_sites = [(0u64, 0u64, 0u8); cache::MAX_CONT_SITES];
    let mut rec_nsites = 0u8;
    let mut rec_link_cell = 0u64;
    if ir.rec_call.is_some() {
        cache::mark_recursive(st, rpr);
    }
    if ir.rec_ret.is_some() || ir.rec_call.is_some() {
        let code = rootp.code_ptr() as u64;
        let found = cache::with_tr(st, rpr, |tp| {
            // R2: a native return of `n` results continues only at sites whose `CALL` wants exactly `n` results or all of
            // them (`C == 0`): the interpreter's `RETURN1` / `RETURN0` would nil-fill a larger wanted count, the native one does not
            let n_ret = ir.rec_ret.map_or(0, |r| i32::from(r.n));
            let sites: Vec<(u64, u64, u8)> = tp
                .heads()
                .iter()
                .filter(|h| tp.is_cont_head(h.pc))
                .filter(|h| {
                    let call = rn_core::instruction::Instruction(rootp.code()[h.pc as usize - 1]);
                    let w = call.c() as i32 - 1;
                    // BUNDLE-K L1: a lazy native return leaves `thread.top` alone: a site that wants all results (`C == 0`) reads it
                    ir.rec_ret.is_none() || w == n_ret || (w < 0 && !lazy) || mutants::active("rec_ret_wanted_unchecked")
                })
                .map(|h| {
                    let a = rn_core::instruction::Instruction(rootp.code()[h.pc as usize - 1]).a() as u8;
                    (code + 4 * u64::from(h.pc), &h.rec as *const std::cell::Cell<usize> as u64, a)
                })
                .collect();
            let c0 = tp
                .head_index(0)
                .map_or(0, |i| &tp.head(i).rec as *const std::cell::Cell<usize> as u64);
            (sites, c0)
        });
        let (sites, c0) = found.ok_or_else(|| abort_err("no-sites", "native return"))?;
        if cache::verbose_aborts() {
            eprintln!("[trace] native sites for {}: {:x?} (rec_ret {:?}, rec_call {})", pc, sites, ir.rec_ret, ir.rec_call.is_some());
        }
        if ir.rec_ret.is_some() && (sites.is_empty() || sites.len() > cache::MAX_CONT_SITES) {
            return Err(abort_err("no-sites", "native return: continuation heads"));
        }
        for (k, s) in sites.iter().take(cache::MAX_CONT_SITES).enumerate() {
            rec_sites[k] = *s;
            rec_nsites = k as u8 + 1;
        }
        if ir.rec_call.is_some() && (pc != 0 || side.is_some()) {
            rec_link_cell = c0;
        }
    }
    let emurun = opts.emurun;
    let emuverify = opts.emuverify && !opts.emurun;
    if (ir.rec_ret.is_some() || ir.rec_call.is_some()) && (emurun || emuverify) {
        // (the emulator cannot run the pinned-register switch of a native frame push)
        return Err(abort_err("emu-unsupported", "native self-call"));
    }
    let env = asm::AsmEnv {
        trace_id: act.id.slot,
        counters: counters as u64,
        hot_exit: act.hot_exit,
        shared_exit: 0,
        exit_handler: exit::jit_tr_exit as *const () as usize as u64,
        stack_off: thr::stack_ptr_offset(st) as i32,
        pcl_h: pcl,
        head,
        head_pc: pc,
        entry_fail_fn: cache::jit_tr_entry_fail as *const () as usize as u64,
        calln: asm::jit_tr_calln as *const () as usize as u64,
        barrier: asm::jit_tr_barrier as *const () as usize as u64,
        alloc: asm::jit_tr_alloc as *const () as usize as u64,
        interrupt: jit_hooks::trace_poll_flag_addr() as u64,
        exitstorm: opts.exitstorm,
        // `+emuverify` compares at the handler, so every exit goes through it
        generic_only: emuverify,
        count_iterations: opts.verify,
        budget_reg: vbreg,
        exit_cells: cells as u64,
        side: side_env,
        vcell: st.heap().virtual_cell() as u64,
        rec_sites,
        rec_nsites,
        rec_link_cell,
        lazy,
        seg_oracle: opts.verify && !ir.segs.is_empty(),
        link_mat: unsafe { (*crate::rt::data_ptr(st)).tr.cfg.limits.retf } & 2 != 0,
        prelude: act.prelude,
        lazy_leave_fn: exit::jit_tr_lazy_leave as *const () as usize as u64,
        lazy_cs,
        lazy_nparams: u16::from(rootp.numparams),
        root_maxstack: u16::from(rootp.maxstacksize),
        no_room_checks: side.is_some_and(|s| s.frameful),
        side_room,
        frame_cells: frame_cells_on,
        // SAFETY: the per-state data is live (see `rt::data_ptr`); read-only.
        chain: unsafe { (*crate::rt::data_ptr(st)).tr.cfg.limits.chainre } != 0,
        root_code: rootp.code_ptr() as usize as u64,
        frame_link_snap: side.filter(|s| s.frame_link).map_or(u32::MAX, |s| s.link_snap),
        setmt_inline: true,
        b2_arr: true,
        b2_tag: true,
        b2_hk: true,
        nopay: true,
        divmagic: true,
        hk_fuse: true,
        hk_key64: true,
        budget_sub: vbreg.is_some(),
        // `+emuverify` runs the emulation on a private copy of the header, which the far `[r15 + disp]` poll word is not part of: the trace polls
        // by the absolute address there (the only code difference of that mode; the exit area and the `fpool` pool are copied)
        poll_off: if match unsafe { (*crate::rt::data_ptr(st)).tr.cfg.limits.pollr15 } {
            _ if emuverify => false,
            0 => false,
            // 2: not in traces of native self-recursion (`rec_call` / `rec_ret`: call-frame stores in a store-forwarding chain, `programs/fib` +8 %)
            2 => ir.rec_call.is_none() && ir.rec_ret.is_none(),
            _ => true,
        } {
            let d = jit_hooks::trace_poll_flag_addr() as i64 - crate::rt::cnt_ptr(st) as i64;
            i32::try_from(d).ok()
        } else {
            None
        },
        fpool: crate::rt::fpool_ptr(st) as u64,
        fpool_off: crate::rt::fpool_off(),
        f2_byte: true,
        ccalls_inline: unsafe { (*crate::rt::data_ptr(st)).tr.cfg.limits.ccalls != 0 },
        // SAFETY: as above.
        f2ifl: unsafe { (*crate::rt::data_ptr(st)).tr.cfg.limits.f2ifl != 0 },
        // SAFETY: as above.
        fibdiet: unsafe { (*crate::rt::data_ptr(st)).tr.cfg.limits.fibdiet },
        lean_sub: true,
        libcall: unsafe { (*crate::rt::data_ptr(st)).tr.cfg.limits.libcall },
        sink_stub: unsafe { (*crate::rt::data_ptr(st)).tr.cfg.limits.sinkstub != 0 },
        mat_fn: exit::jit_tr_mat as *const () as usize as u64,
    };
    let t_asm = phase::now();
    let mut a = asm::assemble(ir, &al, &snaps, &rec.writeback, &env).map_err(|e| {
        abort_err(
            match e {
                asm::AsmError::Unsupported(_) => "asm-unsupported",
                asm::AsmError::Inconsistent(_) => "asm-inconsistent",
                asm::AsmError::Spill => "asm-spill",
            },
            format!("asm: {e:?}"),
        )
    })?;
    phase::add(phase::ASM, t_asm);
    let hk_fused = a.hk_fused;
    let hk_split = a.hk_split;
    let dbl = a.dbl.clone();
    let emu_prog = (emurun || emuverify).then(|| a.prog.clone());
    let mut entry = a.entry;
    if emurun {
        a.prog = Program::default();
        entry = emurun_stub(&mut a.prog, &env, act.id.slot);
    } else if emuverify {
        entry = emuverify_stub(&mut a.prog, a.entry, act.id.slot);
    }
    let t_x86 = phase::now();
    let em = x86::emit(&a.prog).map_err(|e| abort_err("x86", format!("x86: {e:?}")))?;
    phase::add(phase::X86, t_x86);
    let entry_off =
        em.label_off[entry as usize].ok_or_else(|| abort_err("x86", "entry unbound"))?;
    // Q3: the shape of the compile (IR + register allocation text, free of trace numbers and addresses of the
    // trace's own counters) identifies a re-compile that would repeat a flushed generation
    // (P-G: hashed from the structures; the text itself is built only for `RUSTYNAIL_JIT_DUMP`)
    let t_h = phase::now();
    let hash = cache::shape_hash_ir(ir, &al);
    phase::add(phase::HASH, t_h);
    let t_inst = phase::now();
    let text = act.dump.then(|| format!("{}\n{}", print::dump(ir), ra::dump(ir, &al)));
    let newid = cache::install(
        st,
        act.id,
        cache::TraceInstall {
            code: em.code,
            entry_off,
            inner_off: a.inner.and_then(|l| em.label_off[l as usize]).unwrap_or(0),
            rec_ret: ir.rec_ret.is_some(),
            text,
            hash,
            weight: ir.ins.len() as u32,
        },
    )
    .map_err(|e| PipeErr {
        kind: Fail::Done,
        detail: format!("install: {e:?}"),
    })?;
    phase::add(phase::INSTALL, t_inst);
    if link_root {
        cache::note_nest(st, side.is_some_and(|s| s.frame_link));
    }
    if act.side && side.is_some_and(|s| s.frame_link) {
        cache::note_side_frame_link(st);
    }
    if stitched {
        cache::note_stitch(st);
    }
    cache::note_hk_fused(st, hk_fused, hk_split);
    let gk = exit::guard_kinds(ir);
    let la = record::last_la_sites();
    let meta = ExitMeta {
        root_code: rootp.code_ptr(),
        snaps,
        exits: descs,
        vb: env.vb(),
        counters,
        cells,
        peeled: ir.loop_at.is_some(),
        // R2: a side trace that checks its own room fails through its start snapshot BEFORE the check: the room an exit of it
        // can promise to a side trace attached to that exit is the parent's (the later exits could promise more, which is not worth a
        // second number: a child that needs more checks its own room)
        extent: if side_room && !crate::tr::mutants::active("side_room_meta_full") { act.max_extent } else { ir.extent },
        depth: if side_room && !crate::tr::mutants::active("side_room_meta_full") { act.max_ci } else { ir.depth },
        frame_cells: frame_cells_on,
        lazy,
        seg_oracle: if opts.verify && !ir.segs.is_empty() { al.nspill + asm::ORACLE_SLOT_K + 1 } else { 0 },
    };
    let attached = cache::attach_exits(st, newid, meta);
    if attached.is_some() && !gk.is_empty() {
        cache::set_gkinds(st, newid, gk, &dbl);
    }
    if attached.is_some() && !la.is_empty() {
        cache::set_la_sites(st, newid, la);
    }
    if attached.is_some() && !act.side && ir.hoisted {
        cache::set_hoisted(st, newid, true);
    }
    if attached.is_some() && !act.side {
        // P3 (`ncallee`): does the entry segment of this root hold a callee identity guard (the guards `Asm::entry_kind` treats as entry checks)?
        let lp = ir.loop_at.map(|l| (l - ir::REF_BIAS) as usize);
        let ec = ir.ins.iter().enumerate().any(|(k, i)| {
            i.flags & ir::F_GUARD != 0 && i.op == ir::TOp::Eqref && lp.is_none_or(|l| k < l) && {
                let s = &ir.snaps[ir::aux_snap(i.aux) as usize];
                s.map.1 == 0 && s.nframes == 0 && s.frame == 0 && s.top == 0
            }
        });
        if ec {
            cache::set_entry_callee(st, newid, true);
        }
    }
    if attached.is_none() {
        // never leave a trace without exit metadata
        cache::request_flush(st, newid, cache::FlushWhy::Off);
        return Err(PipeErr {
            kind: Fail::Done,
            detail: "attach_exits: stale".into(),
        });
    }
    if let Some(prog) = emu_prog {
        let labels = Arc::new(crate::emu::label_table_of(&prog));
        let verifiable = !ir.ins.iter().any(|i| {
            matches!(
                i.op,
                ir::TOp::Astore
                    | ir::TOp::Ustore
                    | ir::TOp::Hstore
                    | ir::TOp::Tbar
                    | ir::TOp::Atouch
                    // F3: class-A helpers allocate / insert / move the length hint on the live heap
                    | ir::TOp::Tnew
                    | ir::TOp::Newref
                    | ir::TOp::Tlen
                    | ir::TOp::Tset
                    | ir::TOp::Tvirt
                    | ir::TOp::Tnext
                    | ir::TOp::Snew
                    | ir::TOp::Strto
                    | ir::TOp::Concat
                    | ir::TOp::Lib
                    // N2b: the native run moves the frame below the private copy the re-run uses (`+emurun` runs on the live frame)
                    | ir::TOp::Retf
            ) || is_uv_identity_guard(&ir, i)
        });
        let t = EmuTrace {
            labels,
            entry: a.entry,
            prog,
            extent: usize::from(ir.extent),
            verifiable,
            root,
            head_pc: pc,
            exit_fn: env.exit_handler,
            calln_fn: env.calln,
            entry_fail_fn: env.entry_fail_fn,
        };
        let key = (crate::rt::data_ptr(st) as usize, newid.slot);
        EMU.with(|e| e.borrow_mut().insert(key, Rc::new(t)));
        if emuverify {
            exit::set_probe(Some(verify_probe));
        }
    }
    if !ir.segs.is_empty() {
        sink::RETF_TRACES.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }
    jit_hooks::rec_request_enter();
    Ok(())
}

fn area_arg0() -> M {
    M {
        base: Base::Cnt,
        off: (exit_area::FROM_R15 as usize + exit_area::ARGS) as i32,
    }
}

/// N1: `EQ UVREL(uref), 16 * slot`, the identity guard of an upvalue that is a slot of the root frame. `UVREL` is the address of
/// the upvalue's value relative to the frame base, which the `+emuverify` re-run (a private copy of the frame at another address)
/// cannot reproduce: such a trace is not verifiable (`+emurun` runs on the live frame and does check it).
fn is_uv_identity_guard(ir: &ir::TraceIr, i: &ir::Ins) -> bool {
    i.op == ir::TOp::Eq
        && [i.a, i.b].iter().any(|&o| {
            ir::TraceIr::is_ins(o) && ir.ins[usize::from(o - ir::REF_BIAS)].op == ir::TOp::Uvrel
        })
}

/// `+emurun`: the installed code is this stub; it runs the trace in the emulator ([`jit_tr_emurun`]) and then continues
/// at the landing address the emulation (the exit handler or an inline stub) produced, re-establishing the pinned
/// registers for `thread.ci` as the shared exit routine does.
fn emurun_stub(p: &mut Program, env: &asm::AsmEnv, slot: u32) -> L {
    let l = p.label();
    p.push(Op::Bind(l));
    p.push(Op::CallH {
        f: jit_tr_emurun as *const () as usize as u64,
        a: slot,
        b: 0,
    });
    // F6: non-zero = the emulated exit handler recorded a memory error (TR29.3); like the shared exit routine, leave
    // compiled code with `JitExit::Error` (the frame was restored by the handler)
    let err = p.label();
    p.push(Op::Br {
        cc: crate::ir::Cc::Ne,
        a: 0,
        b: crate::ir::Src::Imm(0),
        to: err,
    });
    p.push(Op::Ld {
        dst: 1,
        w: W::B8,
        m: M {
            base: Base::Thread,
            off: thr::CI as i32,
        },
    });
    p.push(Op::SetCi(1));
    p.push(Op::ReloadFrame {
        stack_off: env.stack_off,
    });
    p.push(Op::Ld {
        dst: 2,
        w: W::B8,
        m: M {
            base: Base::Reg(1),
            off: ci_layout::KCACHE as i32,
        },
    });
    p.push(Op::SetK(2));
    p.push(Op::Ld {
        dst: 3,
        w: W::B8,
        m: area_arg0(),
    });
    // landing address 0: the handler started a side recording, leave for the interpreter (as the shared routine does)
    let go = p.label();
    p.push(Op::Br {
        cc: crate::ir::Cc::Ne,
        a: 3,
        b: crate::ir::Src::Imm(0),
        to: go,
    });
    p.push(Op::Ret(jit_hooks::JitExit::Continue as u32));
    p.push(Op::Bind(go));
    p.push(Op::JmpR(3));
    p.push(Op::Bind(err));
    p.push(Op::Ret(jit_hooks::JitExit::Error as u32));
    l
}

/// `+emuverify`: a new entry in front of the native trace snapshots the entry state ([`jit_tr_emuverify_pre`]).
fn emuverify_stub(p: &mut Program, native_entry: L, slot: u32) -> L {
    let l = p.label();
    p.push(Op::Bind(l));
    p.push(Op::CallH {
        f: jit_tr_emuverify_pre as *const () as usize as u64,
        a: slot,
        b: 0,
    });
    p.push(Op::Jmp(native_entry));
    l
}

/// The frame base (`rbx`): register 0 of the frame of `ci`.
///
/// # Safety
/// `th` and `ci` are the live thread and one of its frames.
unsafe fn frame_of(th: *mut Thread, ci: *mut CallInfo, stack_off: usize) -> *mut u8 {
    // SAFETY: contract; the offsets are the layout of `Thread` / `CallInfo` (jit_layout).
    unsafe {
        let stack = *th.cast::<u8>().add(stack_off).cast::<usize>();
        let func = *ci.cast::<u8>().add(ci_layout::FUNC).cast::<usize>();
        (stack + (func + 1) * 16) as *mut u8
    }
}

/// Helpers of an emulation that runs on the live state: every `CallH` calls the real `extern "C"` helper.
struct LiveHelpers;

impl Helpers for LiveHelpers {
    unsafe fn call(&mut self, f: u64, a: u32, b: u32, env: &Env, _frame: *mut u8) -> HelperOut {
        // SAFETY: `f` is the address of an `extern "C" fn(thread, ci, u32, u32) -> u32` the assembler embedded
        // (exit handler, helpers); the arguments are the live thread and root frame.
        let r = unsafe {
            let func: unsafe extern "C" fn(*mut u8, *mut u8, u32, u32) -> u32 =
                std::mem::transmute(f as usize);
            func(env.thread, env.ci, a, b)
        };
        HelperOut {
            result: r,
            new_frame: None,
        }
    }
}

fn emu_lookup(st: &mut State, slot: u32) -> Rc<EmuTrace> {
    let key = (crate::rt::data_ptr(st) as usize, slot);
    EMU.with(|e| e.borrow().get(&key).cloned())
        .expect("`+emurun` trace without an emulation entry")
}

/// `jit_tr_emurun(thread, ci, slot, 0)`: runs trace `slot` in the emulator on the live state (`+emurun`, 9.6). The
/// emulation ends at a `JmpR` out of the program (an exit); its target is left in `ExitArea.args[0]`.
///
/// # Safety
/// Called only from the `+emurun` stub with the running thread and its current Lua frame.
pub unsafe extern "C" fn jit_tr_emurun(
    th: *mut Thread,
    ci: *mut CallInfo,
    slot: u32,
    _b: u32,
) -> u32 {
    // SAFETY: `th` is the live thread pinned in `r12`.
    let mut st = unsafe { jit_hooks::state_from_thread(th.cast()) };
    let t = emu_lookup(&mut st, slot);
    let budget = crate::rt::cnt_ptr(&mut st);
    let stack_off = thr::stack_ptr_offset(&mut st);
    // SAFETY: the live thread and frame (contract).
    let (frame, k) = unsafe {
        (
            frame_of(th, ci, stack_off),
            *ci.cast::<u8>().add(ci_layout::KCACHE).cast::<*const u8>(),
        )
    };
    let env = Env {
        frame,
        ci: ci.cast(),
        thread: th.cast(),
        k,
        cnt: budget,
    };
    let mut emu = Emu::with_labels(Arc::clone(&t.labels), t.entry);
    // SAFETY: `env` describes the live frame; the helpers are the real ones (as the native code would call).
    let u = unsafe { emu.run(&t.prog, &[], &env, u64::MAX, &mut LiveHelpers) };
    let target = match u {
        // the shared exit routine returned to the interpreter (F2a: a side recording started at the exit)
        Until::Stop(Stop::Ret(c)) if c == jit_hooks::JitExit::Continue as u32 => 0,
        // F6: the emulated shared routine returned `JitExit::Error` (memory error while materialising a sunk table):
        // tell the stub to leave compiled code with the same status
        Until::Stop(Stop::Ret(c)) if c == jit_hooks::JitExit::Error as u32 => return 1,
        Until::Stop(Stop::Unsupported) => emu.jmp_target().expect("emurun: a JmpR target"),
        u => panic!("emurun: {u:?}"),
    };
    // SAFETY: the area follows the budget word.
    unsafe {
        (*budget
            .cast::<u8>()
            .add(exit_area::FROM_R15 as usize)
            .cast::<ExitArea>())
        .args[0] = target;
    }
    0
}

/// `jit_tr_emuverify_pre(thread, ci, slot, 0)`: records the entry state of a trace without stores (the frame, the
/// budget word and the exit area).
///
/// # Safety
/// Called only from the `+emuverify` stub with the running thread and its current Lua frame.
pub unsafe extern "C" fn jit_tr_emuverify_pre(
    th: *mut Thread,
    ci: *mut CallInfo,
    slot: u32,
    _b: u32,
) -> u32 {
    // SAFETY: `th` is the live thread.
    let mut st = unsafe { jit_hooks::state_from_thread(th.cast()) };
    let t = emu_lookup(&mut st, slot);
    VERIFY.with(|v| *v.borrow_mut() = None);
    if !t.verifiable {
        return 0;
    }
    let budget = crate::rt::cnt_ptr(&mut st);
    let stack_off = thr::stack_ptr_offset(&mut st);
    let n = (t.extent + 1) * 2;
    // SAFETY: the frame has at least `extent` slots (TR6a ran before the stub? no: the stub runs before TR6, so
    // clamp to the stack): the closure slot sits right below register 0 and `extent + 1` slots are in the stack
    // whenever the trace is going to run; when TR6 would fail the copy is merely larger than needed but inside the
    // stack limit check below.
    let frame = unsafe { frame_of(th, ci, stack_off) };
    let stack_size = st.stack_size();
    // SAFETY: as above.
    let func = unsafe { *ci.cast::<u8>().add(ci_layout::FUNC).cast::<usize>() };
    if func + 1 + t.extent + 1 >= stack_size {
        return 0; // TR6 fails: no trace run to verify
    }
    let mut fr = vec![0u64; n];
    // SAFETY: `[frame - 16, frame + extent * 16)` is inside the stack (checked above).
    unsafe { std::ptr::copy_nonoverlapping(frame.sub(16), fr.as_mut_ptr().cast::<u8>(), n * 8) };
    // the whole `BudgetHdr`: budget word, exit area and (P2-3c `fpool`) the double-constant pool the trace addresses as `[r15 + disp]`
    let hbytes = std::mem::size_of::<crate::rt::BudgetHdr>();
    let hsz = hbytes.div_ceil(8);
    let mut hdr = vec![0u64; hsz];
    // SAFETY: the header and area are one allocation behind the budget word.
    unsafe {
        std::ptr::copy_nonoverlapping(budget.cast::<u8>(), hdr.as_mut_ptr().cast::<u8>(), hbytes);
    }
    // SAFETY: `ci` is the live running frame (contract).
    let trap = unsafe { *ci.cast::<u8>().add(ci_layout::TRAP).cast::<i32>() };
    VERIFY.with(|v| {
        *v.borrow_mut() = Some(VerifyPre {
            slot,
            th,
            ci,
            frame: fr,
            hdr,
            trap,
        });
    });
    0
}

/// Helpers of the verification re-run: the exit handler is intercepted (the native run already did the exit), pure
/// helpers run on the copy.
struct VerifyHelpers<'a> {
    t: &'a EmuTrace,
    exit_reached: bool,
}

impl Helpers for VerifyHelpers<'_> {
    unsafe fn call(&mut self, f: u64, a: u32, _b: u32, env: &Env, _frame: *mut u8) -> HelperOut {
        // SAFETY: `env.cnt` is the private copy of the budget word and area.
        let area = unsafe {
            &mut *env
                .cnt
                .cast::<u8>()
                .add(exit_area::FROM_R15 as usize)
                .cast::<ExitArea>()
        };
        if f == self.t.exit_fn {
            self.exit_reached = true;
            area.args[0] = VERIFY_SENTINEL;
        } else if f == self.t.calln_fn {
            area.args[0] = asm::calln_core(a, area.args[0], area.args[1]);
        } else if f == self.t.entry_fail_fn {
            // the head counters are restored by the caller
        } else {
            panic!("emuverify: unexpected helper {f:#x}");
        }
        HelperOut {
            result: 0,
            new_frame: None,
        }
    }
}

const VERIFY_SENTINEL: u64 = 0xE0E0_0000_0000_0001;

static VERIFIED: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Number of native exits `+emuverify` has re-run in the emulator and compared so far (tests).
pub fn emuverify_count() -> u64 {
    VERIFIED.load(std::sync::atomic::Ordering::Relaxed)
}

/// The `+emuverify` probe (called inside the exit handler after the native exit was restored): re-runs the trace in the
/// emulator from the entry copy and compares the exit and the values of every slot the snapshot restores.
fn verify_probe(st: &mut State, id: TrId) {
    let Some(pre) = VERIFY.with(|v| v.borrow_mut().take()) else {
        return;
    };
    if pre.slot != id.slot {
        return;
    }
    let t = emu_lookup(st, id.slot);
    let Some((_, meta)) = cache::exit_meta(st, id.slot) else {
        return;
    };
    let real = crate::rt::cnt_ptr(st);
    // SAFETY: the area follows the budget word.
    let real_area = unsafe {
        &*real
            .cast::<u8>()
            .add(exit_area::FROM_R15 as usize)
            .cast::<ExitArea>()
    };
    let mut fr = pre.frame;
    let mut hdr = pre.hdr;
    let stack_off = thr::stack_ptr_offset(st);
    // SAFETY: the thread and frame of the activation that is exiting (still live inside its handler).
    let k = unsafe {
        *pre.ci
            .cast::<u8>()
            .add(ci_layout::KCACHE)
            .cast::<*const u8>()
    };
    let _ = stack_off;
    let env = Env {
        // SAFETY: slot 0 of the copy is slot 1 of `fr` (two words per slot).
        frame: unsafe { fr.as_mut_ptr().cast::<u8>().add(16) },
        ci: pre.ci.cast(),
        thread: pre.th.cast(),
        k,
        cnt: hdr.as_mut_ptr().cast::<i32>(),
    };
    // the emulation books entries in the head's counters again: save and restore them
    let head_state = head_counters(st, t.root, t.head_pc);
    let mut hl = VerifyHelpers {
        t: &t,
        exit_reached: false,
    };
    let mut emu = Emu::with_labels(Arc::clone(&t.labels), t.entry);
    // the live `ci.trap` as it was at entry for the duration of the emulation (see `VerifyPre::trap`)
    // SAFETY: `pre.ci` is the live frame of the activation that is exiting; the field is restored right after the run.
    let live_trap = unsafe {
        let p = pre.ci.cast::<u8>().add(ci_layout::TRAP).cast::<i32>();
        let old = *p;
        *p = pre.trap;
        old
    };
    // SAFETY: `env` describes private copies plus the live thread/ci, which the trace only reads (no stores).
    let u = unsafe { emu.run(&t.prog, &[], &env, u64::MAX, &mut hl) };
    // SAFETY: as above.
    unsafe { *pre.ci.cast::<u8>().add(ci_layout::TRAP).cast::<i32>() = live_trap };
    set_head_counters(st, t.root, t.head_pc, head_state);
    assert!(
        matches!(u, Until::Stop(Stop::Unsupported)),
        "emuverify: emulator stopped with {u:?}"
    );
    assert!(
        hl.exit_reached && emu.jmp_target() == Some(VERIFY_SENTINEL),
        "emuverify: the emulation did not exit through the handler (target {:?})",
        emu.jmp_target()
    );
    // SAFETY: as above.
    let emu_area = unsafe {
        &*hdr
            .as_ptr()
            .cast::<u8>()
            .add(exit_area::FROM_R15 as usize)
            .cast::<ExitArea>()
    };
    assert_eq!(
        (emu_area.trace_id, emu_area.exitno),
        (real_area.trace_id, real_area.exitno),
        "emuverify: native and emulated runs left through different exits"
    );
    // the generic twin of an entry stub tags the number with `EXIT_ENTRY` (compared raw above, so the flag agrees)
    let exitno = real_area.exitno & !exit::EXIT_FLAGS;
    let ex = meta.exits[exitno as usize];
    let snap = &meta.snaps[ex.snap as usize];
    let val = |a: &ExitArea, s: exit::ValSrc| -> Option<u64> {
        match s {
            exit::ValSrc::Reg(r) => Some(a.gpr[usize::from(r)]),
            exit::ValSrc::Xmm(x) => Some(a.xmm[usize::from(x)]),
            exit::ValSrc::Spill(k) => Some(a.spill[usize::from(k)]),
            exit::ValSrc::Imm(_) => None,
        }
    };
    for s in &snap.slots {
        let (n, e) = (val(real_area, s.src), val(emu_area, s.src));
        assert_eq!(
            n, e,
            "emuverify: slot {} of exit {} differs: native {:?} emulated {:?}",
            s.slot, exitno, n, e
        );
    }
    if let ra::Loc::G(r) = meta.vb {
        assert_eq!(
            real_area.gpr[usize::from(r)],
            emu_area.gpr[usize::from(r)],
            "emuverify: the budget differs at exit {}",
            exitno
        );
    }
    VERIFIED.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
}

thread_local! {}

fn head_counters(st: &mut State, root: usize, pc: u32) -> (u32, u32, u32) {
    // SAFETY: the root prototype is live (the trace is running in it).
    let p = unsafe { &*(root as *const rn_core::func::Proto) };
    cache::proto_ref(p)
        .and_then(|pr| {
            cache::with_tr(st, pr, |tp| {
                let h = tp.head(tp.head_index(pc)?);
                Some((h.entry_fail.get(), h.entry_ok.get(), h.pending.get()))
            })
        })
        .flatten()
        .unwrap_or((0, 0, 0))
}

fn set_head_counters(st: &mut State, root: usize, pc: u32, c: (u32, u32, u32)) {
    // SAFETY: as in `head_counters`.
    let p = unsafe { &*(root as *const rn_core::func::Proto) };
    if let Some(pr) = cache::proto_ref(p) {
        cache::with_tr(st, pr, |tp| {
            if let Some(i) = tp.head_index(pc) {
                let h = tp.head(i);
                h.entry_fail.set(c.0);
                h.entry_ok.set(c.1);
                h.pending.set(c.2);
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exit_area_layout_is_frozen() {
        assert_eq!(exit_area::TRACE_ID, 0);
        assert_eq!(exit_area::EXITNO, 4);
        assert_eq!(exit_area::GPR, 8);
        // (P2 bundle 1: 11 general registers, `V9` = `rbp` and `V10` = `r14` added)
        assert_eq!(exit_area::XMM, 8 + 88);
        assert_eq!(exit_area::SPILL, 8 + 88 + 128);
        assert_eq!(exit_area::ARGS, 8 + 88 + 128 + 8 * NSPILL);
        // (BUNDLE-K L1: `lazy_root` right behind `args`; asm reads it only through `exit_area::LAZY_ROOT`)
        assert_eq!(exit_area::LAZY_ROOT, exit_area::ARGS + 32);
        assert_eq!(exit_area::SIZE, exit_area::LAZY_ROOT + 8);
    }

    #[test]
    fn options_parse() {
        assert_eq!(TraceOpts::parse("").unwrap(), TraceOpts::default());
        let o = TraceOpts::parse(
            "-loop -sink,+randra=7 +exitstorm=61 +verify +emurun +emuverify +tracemove",
        )
        .unwrap();
        assert!(!o.loop_opt && !o.sink && o.fold && o.cse && o.fwd && o.abc);
        assert_eq!((o.randra, o.exitstorm), (Some(7), Some(61)));
        assert!(o.verify && o.emurun && o.emuverify && o.tracemove);
        assert!(TraceOpts::parse("-nosuch").is_err());
        assert!(TraceOpts::parse("+exitstorm=0").is_err());
        assert!(TraceOpts::parse("+randra=x").is_err());
    }
}

/// R1: the active mutants of the inline `TAILCALL` of tier 1 as the bits `calls::TM_*` (the injection sites are in `calls.rs`; the
/// names are looked up here so that the mutant table finds them under `src/tr`).
pub(crate) fn tail_mutants() -> u8 {
    u8::from(mutants::active("tailfast_args_skip"))
        | u8::from(mutants::active("tailfast_copy_short")) << 1
        | u8::from(mutants::active("tailfast_status_skip")) << 2
        | u8::from(mutants::active("tailfast_close_skip")) << 3
}

/// BUNDLE-B item 2: the active `tset` mutants as the bit set of `templates_table::NS_*`.
pub(crate) fn nilset_mutants() -> u8 {
    use crate::templates_table::{NS_ARR_MT_SKIP, NS_BARRIER_SKIP, NS_FLAGS_SKIP, NS_GET_NIL_HIT, NS_GET_TAG_SKIP, NS_INT_KEY_SKIP, NS_MT_SKIP, NS_STR_TAG_SKIP};
    u8::from(mutants::active("nilset_flags_skip")) * NS_FLAGS_SKIP
        | u8::from(mutants::active("nilset_mt_skip")) * NS_MT_SKIP
        | u8::from(mutants::active("nilset_barrier_skip")) * NS_BARRIER_SKIP
        | u8::from(mutants::active("nilset_arr_mt_skip")) * NS_ARR_MT_SKIP
        | u8::from(mutants::active("tset_str_tag_skip")) * NS_STR_TAG_SKIP
        | u8::from(mutants::active("tset_int_key_unchecked")) * NS_INT_KEY_SKIP
        | u8::from(mutants::active("tget_nil_hit")) * NS_GET_NIL_HIT
        | u8::from(mutants::active("tget_tag_skip")) * NS_GET_TAG_SKIP
}

/// BUNDLE-K item B: the active mutants `(strself_tag_skip, strself_mt_skip, strself_nil_hit)` of the inline string `SELF` of tier 1.
pub(crate) fn strself_mut() -> (bool, bool, bool) {
    (mutants::active("strself_tag_skip"), mutants::active("strself_mt_skip"), mutants::active("strself_nil_hit"))
}

/// BUNDLE-T1I: the active mutants of the inline built-ins of tier 1 (bits of `lower::t1inline::M_*`).
pub(crate) fn t1inline_mut() -> u16 {
    use crate::lower::t1inline as t;
    let mut m = 0;
    if mutants::active("t1i_type_ident_skip") {
        m |= t::M_TYPE_IDENT;
    }
    if mutants::active("t1i_lcf_ident_skip") {
        m |= t::M_LCF_IDENT;
    }
    if mutants::active("t1i_floor_inexact") {
        m |= t::M_FLOOR_EXACT;
    }
    if mutants::active("t1i_minmax_le") {
        m |= t::M_MM_LE;
    }
    if mutants::active("t1i_byte_lo_skip") {
        m |= t::M_BYTE_LO;
    }
    if mutants::active("t1i_abs_float_sign") {
        m |= t::M_ABS_FLT;
    }
    if mutants::active("t1i_select_count") {
        m |= t::M_SELECT_N;
    }
    if mutants::active("t1i_rawlen_next_skip") {
        m |= t::M_RAWLEN_NEXT;
    }
    if mutants::active("t1i_sqrt_int_bits") {
        m |= t::M_SQRT_INT;
    }
    if mutants::active("t1i_type_index") {
        m |= t::M_TYPE_IDX;
    }
    m
}

/// T3: the active mutants `(t3idx_nil_hit, t3idx_use_mt)' of the inline `__index` level of the tier-1 table templates.
pub(crate) fn t3idx_mut() -> (bool, bool) {
    (mutants::active("t3idx_nil_hit"), mutants::active("t3idx_use_mt"))
}

/// The active inline-`#t` mutants `(prev_skip, next_skip, dummy_skip, metatable_skip, hint_skip, zero_bound_skip)` (queried by `templates_alloc`).
pub(crate) fn tlen_mut() -> (bool, bool, bool, bool, bool, bool) {
    (
        mutants::active("tlen_inline_prev_skip"),
        mutants::active("tlen_inline_next_skip"),
        mutants::active("tlen_inline_dummy_skip"),
        mutants::active("tlen_inline_mt_skip"),
        mutants::active("tlen_inline_hint_skip"),
        mutants::active("tlen_inline_zero_bound_skip"),
    )
}

/// BUNDLE-D: the active `armm` code mutants as bits (queried by `templates_alloc`): 1 `armm_count_skip`, 2 `armm_land_skip`, 4 `armm_pay_skip`.
pub(crate) fn armm_mut() -> u8 {
    u8::from(mutants::active("armm_count_skip")) | u8::from(mutants::active("armm_land_skip")) << 1 | u8::from(mutants::active("armm_pay_skip")) << 2 | u8::from(mutants::active("armm_never")) << 3
}

/// BUNDLE-B: mutant `mmcmp_savedpc_skip` (queried by `templates_alloc`): the comparison does not save `savedpc` before `jit_cmp`.
pub(crate) fn mmcmp_savedpc_skip() -> bool {
    mutants::active("mmcmp_savedpc_skip")
}

/// BUNDLE-Q: the active `t1q` mutants as bits of the `CallEnv::t1q` word: 0x100 `t1q_eq_same_false`, 0x200 `t1q_eq_nomm_true`, 0x400 `t1q_eq_second_mm_skip`,
/// 0x800 `t1q_mod_sign_skip`, 0x1000 `t1q_mod_negzero`, 0x2000 `t1q_mod_inf_fix`, BUNDLE-S: bits 14..20 `s1_self_skip`, `s1_get_dst_off`, `s1_savedpc_skip`, `s1_count_skip`, `s1_pay_skip`, `s1_land_skip`, `s1_obj_orig`.
pub(crate) fn t1q_mut() -> u32 {
    u32::from(mutants::active("t1q_eq_same_false")) << 8
        | u32::from(mutants::active("t1q_eq_nomm_true")) << 9
        | u32::from(mutants::active("t1q_eq_second_mm_skip")) << 10
        | u32::from(mutants::active("t1q_mod_sign_skip")) << 11
        | u32::from(mutants::active("t1q_mod_negzero")) << 12
        | u32::from(mutants::active("t1q_mod_inf_fix")) << 13
        | u32::from(mutants::active("s1_self_skip")) << 14
        | u32::from(mutants::active("s1_get_dst_off")) << 15
        | u32::from(mutants::active("s1_savedpc_skip")) << 16
        | u32::from(mutants::active("s1_count_skip")) << 17
        | u32::from(mutants::active("s1_pay_skip")) << 18
        | u32::from(mutants::active("s1_land_skip")) << 19
        | u32::from(mutants::active("s1_obj_orig")) << 20
        | u32::from(mutants::active("s1_close_skip")) << 21
        | u32::from(mutants::active("bv_tail_nil")) << 22
        | u32::from(mutants::active("bv_ret_first_nil")) << 23
}

/// C2: the active tier-1 comparison mutants `(c2cmp_swap, c2cmp_exit_skip)` (queried by `templates_alloc`).
pub(crate) fn c2cmp_mut() -> (bool, bool) {
    (mutants::active("c2cmp_swap"), mutants::active("c2cmp_exit_skip"))
}
