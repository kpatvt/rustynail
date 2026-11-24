//! J5: the trace compiler (docs/JIT_TRACE.md). Feature `trace` of `rn-jit`.
//!
//! P0 contents: the frozen shared types. [`ir`] (the SSA IR), the [`ExitArea`] layout (5.6), the [`RecVTable`]
//! interface of the recorder hook (4.2; the rn-core side is P1's), [`TraceOpts`] (the option set of
//! `RUSTYNAIL_JIT_TRACE_OPT`, 9.10) and the mutant name registry ([`mutants`]). Everything else (recorder, passes,
//! register allocation, assembly, runtime, the pipeline driver) is added by later tasks; the driver ("Finish" of 4.2)
//! will live in this file.

pub mod asm;
pub mod cache;
pub mod diag;
pub mod exit;
pub mod fold;
pub mod interp;
pub mod ir;
pub mod mutants;
pub mod opt_loop;
pub mod opt_mem;
pub mod phase;
pub mod presize;
pub mod print;
pub mod ra;
pub mod record;
pub mod shape;
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
        FPool {
            n: 0,
            bits: [0; FPOOL_N],
        }
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
    /// S4: nonzero once the zero layout ids filled by the exit handler reached the budget (`TrLimits::shapedemote`): the cold path of an `HSHAPE` then re-checks the
    /// keys of the covered `HREFK`s of a table whose id is 0 instead of leaving through the handler.
    pub shape_nofill: u64,
    /// S4: zero ids filled by the exit handler so far (statistics of the budget).
    pub shape_fills: u64,
    /// E5 (`midfold`, docs/JIT_TRACE.md section 35.4): the receiver (table address) and the closure (address) of the region whose method check
    /// passed last: every frame of a method region shares the receiver of its root (pushes pass their own register 0) and runs the checked closure,
    /// so a root return into a caller with the same receiver and closure needs no lookup. Written by every passing check; meaningful only inside a
    /// region whose start (a checked outer entry) wrote them.
    pub mf_recv: u64,
    pub mf_cl: u64,
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
    /// `shape_nofill: u64` (S4).
    pub const SHAPE_NOFILL: usize = offset_of!(ExitArea, shape_nofill);
    /// `mf_recv: u64`, `mf_cl: u64` (E5).
    pub const MF_RECV: usize = offset_of!(ExitArea, mf_recv);
    pub const MF_CL: usize = offset_of!(ExitArea, mf_cl);
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
            shape_nofill: 0,
            shape_fills: 0,
            mf_recv: 0,
            mf_cl: 0,
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
    /// N1b: `TrLimits::nidx` (set from the limits when a recording starts): the negated index of an induction-indexed array load is a second induction variable.
    pub nidx: u32,
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
            nidx: 0,
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
    /// G1: the guard diet (`TrLimits::guarddiet`).
    guarddiet: bool,
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
    ci_spare_fn: u64,
    ci_spare_root_fn: u64,
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
        AbortReason::AtouchFlip => "AtouchFlip",
        AbortReason::SideShort => "SideShort",
        AbortReason::AllocStorm => "AllocStorm",
        AbortReason::Unroll => "Unroll",
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
            AbortReason::LeaveLoop | AbortReason::LeaveFrame | AbortReason::SideShort
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

/// T7 (`t7` bit 3): is the prototype a static leaf, a function whose code holds no call instruction (`CALL`, `TAILCALL`, `TFORCALL`)? It cannot
/// recurse (a metamethod it triggers is another frame's business), so its short recordings are leaf samples of a function that is not worth probing.
fn proto_is_leaf(p: &rn_core::func::Proto) -> bool {
    use rn_core::instruction::Instruction;
    use rn_core::ver::active::OpCode;
    !p.code().iter().any(|&w| {
        matches!(
            Instruction(w).opcode(),
            Some(OpCode::Call | OpCode::Tailcall | OpCode::Tforcall)
        )
    })
}

/// BUNDLE-C: does the closure reach itself through one of its upvalues (`local function f ... f() ... end`)? Such a function recurses: `fnpromo` leaves it to the
/// recursion machinery (a function root of `tail` in awfy/list costs 25% instructions).
/// K2: is the prototype of the frame `ci` already running in one of the `depth` nearest caller frames? (Direct recursion that does not go through an upvalue:
/// `self:permute(n - 1)`. C frames between the Lua frames are skipped, they count as frames.) A policy hint only.
fn recursion_on_stack(st: &State, ci: rn_core::state::Ci, depth: u32) -> bool {
    let Some(cl) = st.ci_lclosure(ci) else {
        return false;
    };
    let me = &*cl.proto() as *const rn_core::func::Proto;
    let mut cur = ci.previous();
    for _ in 0..depth {
        let Some(c) = cur else {
            return false;
        };
        if let Some(other) = st.ci_lclosure(c) {
            if std::ptr::eq(&*other.proto(), me) || mutants::active("fnrec_proto_ignored") {
                return true;
            }
        }
        cur = c.previous();
    }
    false
}

/// E5 (`midfold`, docs/JIT_TRACE.md section 35.4): the method key of a recursion through `self:m(..)`: a `SELF A 0 K` of the prototype (K a short
/// string constant) whose lookup on the running frame's register 0 gives the running closure. Only for a prototype whose register 0 no instruction
/// writes and no closure it makes captures (the receiver of every frame is then the one its call passed). `(constant index, key address,
/// address of the `__index` name)`.
fn midfold_key(
    st: &State,
    ci: Ci,
    cl: rn_core::value::Gc<rn_core::func::LClosure>,
) -> Option<(u32, u64, u64)> {
    use rn_core::instruction::Instruction;
    use rn_core::ver::active::{OpCode, OP_PROPS};
    let p = cl.proto();
    let code = p.code();
    let writes0 = code.iter().any(|&w| {
        let i = Instruction(w);
        i.opcode()
            .is_some_and(|op| OP_PROPS[op as usize].a && i.a() == 0)
    });
    let captured0 = p
        .protos()
        .iter()
        .any(|c| c.upvalue_descs().iter().any(|u| u.instack && u.idx == 0));
    if (writes0 || captured0) && !mutants::active("midfold_reg0_any") {
        return None;
    }
    let func = ci.r().func;
    let fv = st.get(func);
    let recv = st.get(rn_core::state::StkId(func.0 + 1));
    let idx = st.g().heap.tm_name(rn_core::meta::Tm::Index);
    for &w in code {
        let i = Instruction(w);
        if i.opcode() != Some(OpCode::SelfOp) || i.b() != 0 || (cfg!(rustynail_lua54) && i.k() == 0)
        {
            continue;
        }
        let c = i.c();
        let Some(kv) = p.k().get(c as usize) else {
            continue;
        };
        if record::ty_of_value(kv) != Some(ir::Ty::ShrStr) {
            continue;
        }
        let Some(key) = kv.as_strref() else {
            continue;
        };
        if record::method_lookup_is(st, recv, key, &fv) {
            return Some((c, key.as_ptr() as u64, idx.as_ptr() as u64));
        }
    }
    None
}

/// E5: the family rule of a method family (section 35.4): every hash store goes through an `HREFK` of a constant key that is neither the method
/// key nor `__index`, through an insertion (`NEWREF`) of such a key, or through an `HREFV` of a key that is not a string; no `setmetatable`. Array stores and integer-key sets cannot write a short-string key.
fn midfold_stores_ok(ir: &ir::TraceIr, key: u64, idx: u64) -> bool {
    let kptr = |r: ir::Ref| -> Option<u64> {
        if r != ir::NOREF && r < ir::REF_BIAS {
            if let ir::KVal::Ptr(p, _) = ir.kval(r) {
                return Some(p);
            }
        }
        None
    };
    ir.ins.iter().all(|i| match i.op {
        ir::TOp::Hstore => {
            ir::TraceIr::is_ins(i.a) && {
                let h = ir.at(i.a);
                // (through an insertion: its key is checked at the `NEWREF`; a dynamic key that is not a string cannot be either name)
                h.op == ir::TOp::Newref
                    || (h.op == ir::TOp::Hrefk && kptr(h.b).is_some_and(|p| p != key && p != idx))
                    || (h.op == ir::TOp::Hrefv
                        && !matches!(ir.ty_of(h.b), ir::Ty::ShrStr | ir::Ty::LngStr))
            }
        }
        ir::TOp::Newref => kptr(i.b).is_some_and(|p| p != key && p != idx),
        ir::TOp::Lib => i.aux & 0xFF != ir::lib::SETMT,
        _ => true,
    })
}

fn closure_self_ref(cl: rn_core::value::Gc<rn_core::func::LClosure>) -> bool {
    let me = &*cl.proto() as *const rn_core::func::Proto;
    (0..cl.nupvalues()).any(|i| {
        cl.upval(i)
            .get()
            .as_lclosure()
            .is_some_and(|c| std::ptr::eq(&*c.proto(), me))
    })
}

/// The recorder entry the cache calls at a hot head (`cache::StartFn`): arms the recorder for the frame `ci` at `pc`.
/// False: nothing armed (a transient refusal for the cache).
pub fn driver_start(st: &mut State, ci: Ci, id: TrId, pc: u32) -> bool {
    let Some(cl) = st.ci_lclosure(ci) else {
        diag::set_why("NoLuaFrame");
        return false;
    };
    let proto = cl.proto();
    let root = &*proto as *const rn_core::func::Proto as usize;
    let cfg = crate::rt::trace_config(st);
    let resolve: record::ResolveFn = Rc::new(|st, p| {
        cache::proto_ref(p).or_else(|| {
            crate::rt::ensure_compiled(st, p)
                .then(|| cache::proto_ref(p))
                .flatten()
        })
    });
    let pr = cache::proto_ref(&proto);
    // BUNDLE-K L1: RETF (and the frame links that go with it) only in traces of a prototype that is not lazy-aware: the pop assumes every frame below the running one is complete
    // (E4: an undecided candidate with loops records like a lazy-aware prototype, it may become one when this recording is compiled)
    let lazy_root =
        pr.is_some_and(|pr| cache::proto_lazy(st, pr) || cache::proto_lazy_cand(st, pr));
    // BUNDLE-C (`fnpromo`): this function-entry head is a generic function root (not for a function that reaches itself through an upvalue)
    let promoted = pr.is_some_and(|pr| cache::fn_promoted(st, pr)) && !closure_self_ref(cl);
    // K6: remember that the prototype recurses through its own upvalue (the FnShort budget of its entry head)
    if let Some(pr) = pr {
        if cfg.limits.fnself != 0 {
            if closure_self_ref(cl) {
                if !mutants::active("fnself_mark_skip") {
                    cache::mark_selfref(st, pr);
                }
            } else if cfg.limits.fnrec != 0
                && !mutants::active("fnrec_mark_skip")
                && recursion_on_stack(st, ci, cfg.limits.fnrec)
            {
                // K2: method recursion (the callee is found through a table, not an upvalue): the same hint
                cache::mark_selfref(st, pr);
            }
        }
    }
    // E2 (`idfold`): the per-prototype decision at its first recording (a closure that reaches itself through an upvalue)
    if let Some(pr) = pr {
        if cfg.limits.idfold != 0 {
            let me = cl.as_ptr();
            let self_uv = (0..cl.nupvalues())
                .find(|&i| {
                    cl.upval(i)
                        .get()
                        .as_lclosure()
                        .is_some_and(|c| c.as_ptr() == me)
                })
                .and_then(|i| u8::try_from(i).ok());
            let has_setupval = proto.code().iter().any(|&w| {
                rn_core::instruction::Instruction(w).opcode()
                    == Some(rn_core::ver::active::OpCode::Setupval)
            });
            cache::idfold_decide(st, pr, self_uv, has_setupval);
        }
    }
    let (idf_fold, idf_ident) = pr.map_or((None, None), |pr| cache::idfold_cfg(st, pr));
    // E5 (`midfold`): the per-prototype decision at its first recording (a method recursion on its own register 0)
    if let Some(pr) = pr {
        if cfg.limits.midfold != 0 && cache::midfold_pending(st, pr) {
            let key = midfold_key(st, ci, cl);
            cache::midfold_decide(st, pr, key);
        }
    }
    let (mf_fold, mf_ident) = pr.map_or((None, None), |pr| cache::midfold_cfg(st, pr));
    // R2: a continuation head of `recany` records only after a native self-call of the prototype was recorded (otherwise every
    // function with calls would get continuation traces whose return guard fails when no recursion is involved)
    let cok = cfg.limits.contok >= 1 && record::cont_ok(root);
    if cfg.limits.recany != 0
        && pr.is_some_and(|pr| {
            cache::head_is_cont(st, pr, pc) && !cache::proto_recursive(st, pr) && !cok
        })
    {
        diag::set_why("ContNotRecursive");
        return false;
    }
    let deep =
        cfg.limits.deep != 0 && cache::rec_deep(st, id) && !mutants::active("deep_limits_ignored");
    let nest: Option<record::HeadQuery> = if cfg.limits.nest >= 1 {
        pr.map(|pr| {
            Rc::new(move |st: &mut State, pc| cache::head_traced(st, pr, pc)) as record::HeadQuery
        })
    } else {
        None
    };
    let nest_frame: Option<record::FrameHeadQuery> =
        if cfg.limits.nest >= 2 || cfg.limits.sidenest != 0 || cfg.limits.innerlink & 1 != 0 {
            Some(Rc::new(|st: &mut State, p: &rn_core::func::Proto, pc| {
                cache::proto_ref(p).and_then(|pr| cache::head_traced(st, pr, pc))
            }))
        } else {
            None
        };
    let rc = RecCfg {
        hrefv: hrefv_enabled(),
        shape: cfg.limits.shapeguard != 0,
        fold: cfg.opts.fold,
        cse: cfg.opts.cse,
        verify: cfg.opts.verify,
        lockstep: lockstep_on(&cfg),
        restarts: cfg.limits.restarts,
        deps: Some(record::cache_hooks(id, resolve)),
        nest,
        nest_frame,
        nest_frame_roots: cfg.limits.nest >= 2,
        nest_frame_sides: cfg.limits.sidenest != 0,
        inner_link: cfg.limits.innerlink & 1 != 0,
        inner_deep: cfg.limits.innerlink & 4 != 0,
        objkey: cfg.limits.t5,
        // T9: the `RETF` caller rule (`Rec::pop_into_ok`) decides where a link may be made (see `driver_start_side`)
        retf_nolazy: if cfg.limits.lazyci != 0 && cfg.limits.lazyloop != 0 {
            cfg.limits.lazyci | 4
        } else {
            cfg.limits.lazyci
        },
        stitch_min: cfg.limits.stitchmin,
        unroll: if deep {
            cfg.limits.unroll.max(cfg.limits.deep)
        } else {
            cfg.limits.unroll
        },
        max_depth: if deep {
            cfg.limits.maxdepth.max(cfg.limits.deep)
        } else {
            cfg.limits.maxdepth
        },
        max_ins: if deep { cfg.limits.deepins as usize } else { 0 },
        loop_unroll: cfg.limits.loopunroll,
        ncallee: pr.is_some_and(|pr| cache::head_ncallee(st, pr, pc)),
        uvsep: true,
        cloreuse: cfg.limits.cloreuse != 0,
        fn_root: pr.is_some_and(|pr| cache::head_is_fn(st, pr, pc)),
        rec_native: cfg.limits.recnat != 0,
        rec_conts: cfg.limits.recnat != 0,
        rec_any: cfg.limits.recany != 0,
        cnum: true,
        nil_store: cfg.limits.nilstore != 0,
        hstore_mt: cfg.limits.hstoreguard != 0,
        slot_live: cfg.limits.slotlive != 0,
        rec_seen: pr.is_some_and(|pr| cache::proto_recursive(st, pr))
            || (cfg.limits.contok >= 2 && cok),
        rec_only: cfg.limits.recnat != 0
            && cfg.limits.fnroot == 0
            && !promoted
            && (pc == 0 || cfg.limits.recany != 0),
        rec_cont: pr.is_some_and(|pr| cache::head_is_cont(st, pr, pc)),
        // E1: the unroll factor of a function-entry recording (1 everywhere else, and for prototypes the policy demoted)
        rec_unroll: pr.map_or(1, |pr| cache::unroll_k(st, pr, pc)),
        // E2: folding and the region invariant of the family (lazy-aware prototypes only: `idfold_decide`)
        idfold: idf_fold,
        idfold_ident: idf_ident.is_some(),
        // E5: folding and the method invariant of the family (`midfold_decide`)
        midfold: mf_fold,
        midfold_ident: mf_ident.is_some(),
        fn_min: if pr.is_some_and(|pr| cache::fn_cent(st, pr))
            && !mutants::active("centry_fnmin_kept")
        {
            0
        } else if promoted
            && cfg.limits.t7 & 3 != 0
            && !mutants::active("t7_fnmin_kept")
            && (cfg.limits.t7 & 8 == 0 || mutants::active("t7_leaf_any") || proto_is_leaf(&proto))
        {
            // T7: a leaf head that the policy promoted to a generic function root (after its probe recordings were all too short) records its short bodies
            [0, 0, 4, 8][(cfg.limits.t7 & 3) as usize].min(cfg.limits.fnmin)
        } else {
            cfg.limits.fnmin
        },
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
        retf: if lazy_root { 0 } else { cfg.limits.retf },
        retfmax: cfg.limits.retfmax,
        retf_sel: cfg.limits.retfsel != 0,
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
        presize: cfg.limits.presize != 0,
        pcall: true,
        landcache: true,
        storm_step: storm_step(st, cfg.limits.allocguard),
        snapprune: cfg.limits.snapprune != 0,
        snapprune_locals: cfg.limits.snapprune >= 2,
        ..RecCfg::default()
    };
    match record::arm(st, ci, pc, rc) {
        Some(token) => {
            ACTIVE.with(|a| {
                *a.borrow_mut() = Some(Active {
                    token,
                    id,
                    root,
                    opts: {
                        let mut o = cfg.opts.clone();
                        o.hdrlib = cfg.limits.hdrlib
                            | if cfg.limits.tnextinl & 1 != 0 {
                                opt_mem::HDR_TNEXT
                            } else {
                                0
                            };
                        o.sinkmt = cfg.limits.sinkmt;
                        o.sinkside = cfg.limits.sinkside;
                        o.uvsep = 1;
                        o.aidx = cfg.limits.aidx;
                        o.nidx = cfg.limits.nidx;
                        o.bhoist = if pr.is_some_and(|pr| cache::head_nohoist(st, pr, pc)) {
                            0
                        } else {
                            cfg.limits.bhoist
                        };
                        o
                    },
                    hot_exit: cfg.hot_exit,
                    verbose: cfg.verbose,
                    dump: cfg.dump.is_some(),
                    side: false,
                    prelude: false,
                    max_extent: 0,
                    max_ci: 0,
                    side_rooms: false,
                    guarddiet: cfg.limits.guarddiet != 0,
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
    // (see `driver_start`)
    let cfg = crate::rt::trace_config(st);
    let lazy_root = cache::proto_lazy(st, pr);
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
        diag::set_why("DeadLabel");
        return None;
    }
    let rem = snap::land_info(&inner_low, start.pc).1;
    let resolve: record::ResolveFn = Rc::new(|st, p| {
        cache::proto_ref(p).or_else(|| {
            crate::rt::ensure_compiled(st, p)
                .then(|| cache::proto_ref(p))
                .flatten()
        })
    });
    let hs: record::HeadQuery = Rc::new(move |st, pc| cache::head_traced(st, pr, pc));
    let fn_side = cache::head_is_fn(st, pr, 0);
    let (idf_fold, idf_ident) = cache::idfold_cfg(st, pr);
    let (mf_fold, mf_ident) = cache::midfold_cfg(st, pr);
    let deep =
        cfg.limits.deep != 0 && cache::rec_deep(st, id) && !mutants::active("deep_limits_ignored");
    let rc = RecCfg {
        hrefv: hrefv_enabled(),
        shape: cfg.limits.shapeguard != 0,
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
            frame_min: if start.fretry { 0 } else { cfg.limits.framemin },
            carry: start.carry.clone(),
            carry_link: start.carry_link,
        }),
        loop_unroll: cfg.limits.loopunroll,
        // C38 slice 1: a side recording that reaches a callee loop head with a root trace ends with the frame-linking exit
        nest_frame: if cfg.limits.sidenest != 0 || (cfg.limits.retf & 1 != 0 && !lazy_root) {
            Some(Rc::new(|st: &mut State, p: &rn_core::func::Proto, pc| {
                cache::proto_ref(p).and_then(|pr| cache::head_traced(st, pr, pc))
            }) as record::FrameHeadQuery)
        } else {
            None
        },
        // N3a: after a `RETF` the root frame is the caller: a loop head of ITS prototype
        // (E4: bit 2 = `lazyloop`, prototypes with loops may be lazy-aware too)
        retf_nolazy: if cfg.limits.lazyci != 0 && cfg.limits.lazyloop != 0 {
            cfg.limits.lazyci | 4
        } else {
            cfg.limits.lazyci
        },
        retf_head: if cfg.limits.retf & 1 != 0 && !lazy_root && !start.no_retf {
            Some(Rc::new(|st: &mut State, p: &rn_core::func::Proto, pc| {
                cache::proto_ref(p).and_then(|pr| cache::head_traced(st, pr, pc))
            }) as record::FrameHeadQuery)
        } else {
            None
        },
        nest_frame_sides: cfg.limits.sidenest != 0,
        inner_ret: start.inner_ret,
        inner_link_sides: cfg.limits.innerlink & 8 != 0,
        inner_deep: cfg.limits.innerlink & 4 != 0,
        objkey: cfg.limits.t5,
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
        retf: if lazy_root || start.no_retf {
            0
        } else {
            cfg.limits.retf
        },
        retfmax: cfg.limits.retfmax,
        retf_sel: cfg.limits.retfsel != 0,
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
        presize: cfg.limits.presize != 0,
        pcall: true,
        landcache: true,
        fn_root: fn_side,
        rec_native: cfg.limits.recnat != 0 && fn_side,
        rec_conts: cfg.limits.recnat != 0 && fn_side,
        rec_any: cfg.limits.recany != 0,
        cnum: true,
        nil_store: cfg.limits.nilstore != 0,
        hstore_mt: cfg.limits.hstoreguard != 0,
        slot_live: cfg.limits.slotlive != 0,
        rec_seen: cache::proto_recursive(st, pr),
        idfold: idf_fold,
        idfold_ident: idf_ident.is_some(),
        midfold: mf_fold,
        midfold_ident: mf_ident.is_some(),
        unroll: if deep {
            cfg.limits.unroll.max(cfg.limits.deep)
        } else {
            cfg.limits.unroll + start.ubump
        },
        // (BUNDLE-N: a side recording honours `maxdepth` too; it read 0 = the compiled-in 4 unless a function side trace)
        max_depth: if deep {
            cfg.limits.maxdepth.max(cfg.limits.deep)
        } else {
            cfg.limits.maxdepth
        },
        max_ins: if deep { cfg.limits.deepins as usize } else { 0 },
        storm_step: storm_step(st, cfg.limits.allocguard),
        cloreuse: cfg.limits.cloreuse != 0,
        snapprune: cfg.limits.snapprune != 0,
        snapprune_locals: cfg.limits.snapprune >= 2,
        ..RecCfg::default()
    };
    let token = record::arm(st, ci, start.pc, rc)?;
    ACTIVE.with(|a| {
        *a.borrow_mut() = Some(Active {
            token,
            id,
            root,
            opts: {
                let mut o = cfg.opts.clone();
                o.hdrlib = cfg.limits.hdrlib
                    | if cfg.limits.tnextinl & 1 != 0 {
                        opt_mem::HDR_TNEXT
                    } else {
                        0
                    };
                o.sinkmt = cfg.limits.sinkmt;
                o.sinkside = cfg.limits.sinkside;
                o.uvsep = 1;
                o.aidx = cfg.limits.aidx;
                o.nidx = cfg.limits.nidx;
                o.bhoist = cfg.limits.bhoist;
                o
            },
            hot_exit: cfg.hot_exit,
            verbose: cfg.verbose,
            dump: cfg.dump.is_some(),
            side: true,
            prelude: start.prelude,
            max_extent: start.max_extent,
            max_ci: start.max_ci,
            side_rooms: cfg.limits.siderooms != 0,
            guarddiet: cfg.limits.guarddiet != 0,
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
            // section 38 (`deepprobe`): a deep site picks the iteration its root is recorded on (the longest of its probes)
            if !act.side
                && cache::deep_probe_drop(
                    st,
                    act.id,
                    u32::try_from(rec.path.len()).unwrap_or(u32::MAX),
                )
            {
                return;
            }
            // (what the error handling below reads of the recording, which the pipeline consumes)
            let has_segs = !rec.ir.segs.is_empty();
            let unrolled = rec.ir.rec_call.is_some_and(|rc| rc.ninl != 0);
            if let Err(e) = pipeline(st, &act, *rec) {
                if act.verbose >= 2 {
                    eprintln!("[trace] --- pipeline: {}", e.detail);
                }
                match e.kind {
                    Fail::Abort(why) if act.side && has_segs => {
                        cache::abort_retf(st, act.id, abort_name(why));
                    }
                    // E1: an unrolled trace that the pipeline refuses is an unroll failure (the policy counts it; `k = 1` may compile)
                    Fail::Abort(_) if unrolled => {
                        cache::abort_recording(st, act.id, "Unroll", false);
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
                TOp::Aload | TOp::Hload => {
                    ins.flags & F_GUARD != 0
                        && (ins.flags & F_OBS == 0 || any_use)
                        && aux_bits(ins.aux) & 1 != 0
                        && aux_bits(ins.aux) & 2 == 0
                }
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
    // POLL and BUDGET first. A side trace entered with carried values starts with its carried loads (unchecked, position-less: they have no code); they
    // belong to the prefix too, or the checks would be moved in front of the `BUDGET` charge, and a failing check there would leave with `refund = n`
    // for a charge that was never made (the budget word grows by `n`: 5.4 `r1_live_flip [on2 budget=200]` was 168 instructions short)
    let p = ir
        .ins
        .iter()
        .take_while(|i| {
            matches!(i.op, TOp::Poll | TOp::Budget)
                || (i.op == TOp::Sload
                    && aux_bits(i.aux) & ir::SLOAD_CARRIED != 0
                    && !mutants::active("entry_checks_before_budget"))
        })
        .count();
    // (N2b: only the loads before the first `RETF` are entry values: later ones read the caller's frame)
    let first_retf = if ir.segs.is_empty() || mutants::active("retf_entry_checks_past") {
        ir.ins.len()
    } else {
        ir.ins
            .iter()
            .position(|i| i.op == TOp::Retf)
            .unwrap_or(ir.ins.len())
    };
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
            // (a side trace entered with carried values: the stack holds nothing for them, so the entry snapshot names them like snapshot 0 does, or
            // a failing check would resume the interpreter with stale slots; the recorder's `carry_entry_snap_bare` is the same rule. The mutant
            // `entry_checks_before_budget` restores both halves of the old behaviour: the bare snapshot alone is not observable, see docs/JIT_TRACE.md section 23)
            let ents: Vec<ir::SnapEntry> =
                if ir.carry_in.is_empty() || mutants::active("entry_checks_before_budget") {
                    Vec::new()
                } else {
                    ir.snap_entries(0).to_vec()
                };
            let e = ir.snapshot(s, &ents);
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
    // (T2: a mask, not `moved.contains` per instruction: that was quadratic in long recordings)
    let mut is_moved = vec![false; ir.ins.len()];
    for &i in &moved {
        is_moved[i] = true;
    }
    order.extend((p..ir.ins.len()).filter(|&i| !is_moved[i]));
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
    protos: crate::fx::HashMap<usize, (ProtoInfoR, Vec<Option<u64>>)>,
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

/// R1 (`regcarry`): makes every snapshot a root's guard-free second entry can leave through *dense* in the slots of the leading `SLOAD` run. A link that
/// enters the root behind the run (`HeadData::e3`) hands the run's values over in registers and does not write the stack, so the stack slots of those
/// values may be stale from there on: each snapshot used by a guard behind the run gets a `SNAP_CARRY` entry (slot -> the unmodified `SLOAD`) for every
/// carried slot it does not name itself, and the exit restores it (tag and payload) like any other entry. Carried slots are the run's `Int` / `Num` / table
/// slots that no `SSTORE` of the trace writes (an `SSTORE`d slot is omitted from snapshots because the stack holds its value: that would not be the
/// `SLOAD`'s). Returns the carried slots (also in `ir.carry`); a trace with a snapshot shared by a guard of the run and a later one is left alone.
fn carry_dense(ir: &mut ir::TraceIr, min: usize, live: bool) -> Vec<u16> {
    use ir::{aux_snap, SnapEntry, TOp, Ty, F_GUARD, REF_BIAS, SNAP_CARRY};
    ir.carry.clear();
    if ir.loop_at.is_none() {
        return Vec::new();
    }
    let Some((end, req)) = exit::lead_run(ir) else {
        return Vec::new();
    };
    let stored: Vec<u16> = ir
        .ins
        .iter()
        .filter(|i| i.op == TOp::Sstore)
        .map(|i| i.aux as u16)
        .collect();
    let mut slots: Vec<(u16, ir::Ref)> = Vec::new();
    for (k, &(slot, ty)) in req.iter().enumerate() {
        if matches!(ty, Ty::Int | Ty::Num | Ty::Tab) && !stored.contains(&slot) {
            slots.push((slot, REF_BIAS + (2 + k) as u16));
        }
    }
    // a dense entry keeps its `SLOAD`'s value live up to the last guard: carry only the values the trace keeps live anyway (used inside the loop body, which
    // holds them across the back-edge, or at or behind its last guard); the others would cost register pressure (spills) in every iteration
    if live {
        let last_guard = ir
            .ins
            .iter()
            .rposition(|i| i.flags & F_GUARD != 0)
            .unwrap_or(0);
        let loop_idx = ir.loop_at.map(|r| usize::from(r - REF_BIAS));
        let fused = vec![false; ir.ins.len()];
        let mut last_use: crate::fx::HashMap<ir::Ref, usize> = Default::default();
        for i in 0..ir.ins.len() {
            for r in ra::operand_refs(ir, &fused, i) {
                if ir::TraceIr::is_ins(r) {
                    last_use.insert(r, i);
                }
            }
        }
        slots.retain(|&(_, r)| {
            last_use
                .get(&r)
                .is_some_and(|&u| u >= last_guard || loop_idx.is_some_and(|l| u > l))
        });
    }
    // a root with few carried values gains too little from a carrying link to pay for the `stale` test on each of its exits
    if slots.len() < min {
        return Vec::new();
    }
    let n = ir.snaps.len();
    // which snapshots are used by guards of the run (index < end) and which behind it
    let (mut before, mut after) = (vec![false; n], vec![false; n]);
    for (k, i) in ir.ins.iter().enumerate() {
        if i.flags & F_GUARD != 0 {
            let s = aux_snap(i.aux) as usize;
            if s < n {
                if k < end {
                    before[s] = true;
                } else {
                    after[s] = true;
                }
            }
        }
    }
    // a snapshot shared by a guard of the run and a later one (the entry snapshot: callee identity guards placed behind the run keep it) is copied for the
    // later guards
    for s in 0..n {
        if before[s] && after[s] {
            let (snap, entries) = (ir.snaps[s], ir.snap_entries(s as u32).to_vec());
            let ns = ir.snapshot(snap, &entries);
            for (k, i) in ir.ins.iter_mut().enumerate() {
                if k >= end && i.flags & F_GUARD != 0 && aux_snap(i.aux) as usize == s {
                    i.aux = ir::guard_aux(ns, ir::aux_bits(i.aux));
                }
            }
            before.push(false);
            after.push(true);
            after[s] = false;
        }
    }
    for s in 0..ir.snaps.len() {
        if !after[s] {
            continue;
        }
        let old: Vec<SnapEntry> = ir.snap_entries(s as u32).to_vec();
        let mut new: Vec<SnapEntry> = slots
            .iter()
            .filter(|&&(slot, _)| !old.iter().any(|e| e.slot == slot))
            .map(|&(slot, r)| SnapEntry {
                slot,
                r,
                flags: SNAP_CARRY,
            })
            .collect();
        if new.is_empty() {
            continue;
        }
        new.extend(old);
        ir.snaps[s].map = (ir.snapmap.len() as u32, new.len() as u16);
        ir.snapmap.extend(new);
    }
    ir.carry = slots.iter().map(|&(s, _)| s).collect();
    ir.carry.clone()
}

/// R1 (half A): a side trace recorded with a carried load for every slot of the parent's exit map keeps only the loads its instructions use. The others
/// (slots it never reads: unchanged, or overwritten before any read) are dropped from the snapshots and the writeback, and the carry entry stores them to the
/// stack from the parent's registers (`TraceIr::carry_store`): the trace is then as sparse as one entered through the plain stub, and the stack is complete
/// behind the carry entry for them. A slot that no snapshot or writeback entry names (a dead temporary pruned by F4) needs no store at all.
fn carry_demote(rec: &mut Recorded, link_snap: Option<u32>) {
    use ir::{aux_bits, TOp, TraceIr, SLOAD_CARRIED};
    let n = rec.ir.ins.len();
    let fused = vec![false; n];
    let mut used = vec![false; n];
    for i in 0..n {
        for r in ra::operand_refs(&rec.ir, &fused, i) {
            if TraceIr::is_ins(r) {
                used[usize::from(r - ir::REF_BIAS)] = true;
            }
        }
    }
    let mut dropped: Vec<(ir::Ref, u16)> = Vec::new();
    // (section 50: one pass over the snapshot entries and the writeback marks the refs named in a slot other than their own; the per-load scan of every
    // snapshot was quadratic in long side traces, which carry links made common)
    let mut copied_in = vec![false; n];
    {
        let mut mark = |e: &ir::SnapEntry| {
            if TraceIr::is_ins(e.r) {
                let k = usize::from(e.r - ir::REF_BIAS);
                if k < n && rec.ir.ins[k].a != e.slot {
                    copied_in[k] = true;
                }
            }
        };
        for s in 0..rec.ir.snaps.len() {
            for e in rec.ir.snap_entries(s as u32) {
                mark(e);
            }
        }
        for e in &rec.writeback {
            mark(e);
        }
    }
    for k in 0..n {
        let i = rec.ir.ins[k];
        let r = ir::REF_BIAS + k as u16;
        // (a copy of the slot's value in another slot (`MOVE`) names the same ref: the load stays)
        let copied = copied_in[k];
        // (a dead numeric temporary at the link's pc stays carried: it is left out of the writeback like F4 leaves it out of a snapshot, so it costs nothing on
        // the hot path, while storing it at the entry would)
        let dead_at_link = link_snap.is_some_and(|ls| {
            rec.ir.prune.as_ref().is_some_and(|pi| {
                let e = ir::SnapEntry {
                    slot: i.a,
                    r,
                    flags: 0,
                };
                pi.drops(&rec.ir.snaps[ls as usize], &e, i.t)
            })
        });
        if i.op == TOp::Sload
            && aux_bits(i.aux) & SLOAD_CARRIED != 0
            && !used[k]
            && !dead_at_link
            && (!copied || mutants::active("carry_demote_copied"))
        {
            dropped.push((r, i.a));
        }
    }
    if dropped.is_empty() {
        return;
    }
    let mut named_in = vec![false; n];
    for e in rec.ir.snapmap.iter().chain(rec.writeback.iter()) {
        if TraceIr::is_ins(e.r) && usize::from(e.r - ir::REF_BIAS) < n {
            named_in[usize::from(e.r - ir::REF_BIAS)] = true;
        }
    }
    for &(r, slot) in &dropped {
        let named = named_in[usize::from(r - ir::REF_BIAS)];
        let Some(p) = rec.ir.carry_in.iter().position(|c| c.0 == slot) else {
            continue;
        };
        let (_, src, tagok) = rec.ir.carry_in.remove(p);
        if named {
            let ty = rec.ir.ins[usize::from(r - ir::REF_BIAS)].t;
            rec.ir.carry_store.push((slot, ty, src, tagok));
        }
    }
    let mut gone_m = vec![false; n];
    for d in &dropped {
        gone_m[usize::from(d.0 - ir::REF_BIAS)] = true;
    }
    let gone = |r: &ir::Ref| {
        TraceIr::is_ins(*r)
            && usize::from(*r - ir::REF_BIAS) < n
            && gone_m[usize::from(*r - ir::REF_BIAS)]
    };
    for s in 0..rec.ir.snaps.len() {
        if !rec.ir.snap_entries(s as u32).iter().any(|e| gone(&e.r)) {
            continue;
        }
        let old = rec.ir.snap_entries(s as u32).to_vec();
        let new: Vec<ir::SnapEntry> = old.into_iter().filter(|e| !gone(&e.r)).collect();
        rec.ir.snaps[s].map = (rec.ir.snapmap.len() as u32, new.len() as u16);
        rec.ir.snapmap.extend(new);
    }
    rec.writeback.retain(|e| !gone(&e.r));
}

/// R1: is `regcarry` on for this state?
fn carry_limit(st: &mut State) -> bool {
    // SAFETY: the per-state data is live (see `rt::data_ptr`); read-only.
    matches!(
        unsafe { (*crate::rt::data_ptr(st)).tr.cfg.limits.regcarry },
        1 | 2
    )
}

fn pipeline(st: &mut State, act: &Active, rec: Recorded) -> Result<(), PipeErr> {
    let t0 = phase::now();
    // Section 49: the compile-speed levers of this state
    // SAFETY: the per-state data is live (see `rt::data_ptr`); read-only.
    crate::cspeed::set(unsafe { (*crate::rt::data_ptr(st)).tr.cfg.limits.cspeed });
    let r = pipeline_inner(st, act, rec);
    phase::add(phase::PIPELINE, t0);
    r
}

/// The passes of [`pipeline_inner`] before register allocation (sinking, the root passes or the side-trace subset, forwarding, the carry
/// rules, presizing), on `rec` in place.
fn pipeline_front(
    st: &mut State,
    act: &Active,
    rec: &mut Recorded,
    link_root: bool,
    stitched: bool,
) -> Result<(), PipeErr> {
    let opts = &act.opts;
    let side = rec.side;
    let rec = &mut *rec;
    // F2a: a side trace is a straight rolled segment: no entry-check hoisting, no peeling (it has no loop), no type
    // snapshot splitting (its type-guard exits keep the prototype's rule, see `cache::exit_hot`)
    if side.is_some() || stitched {
        // BUNDLE-5 (`sinkside`): allocation sinking is the one pass of the list that also runs on side and stitched traces
        let t0 = phase::now();
        let r = sink::run(rec, opts).map_err(|e| abort_err("pass", e));
        phase::add(phase::PASS_SINK, t0);
        r?;
    }
    if side.is_none() && !stitched {
        for (name, pass) in PASSES {
            let t0 = phase::now();
            let r = pass(rec, opts).map_err(|e| abort_err("pass", e));
            phase::add(
                if *name == "sink" {
                    phase::PASS_SINK
                } else {
                    phase::PASS_OTHER
                },
                t0,
            );
            r?;
        }
    } else if link_root || stitched {
        entry_checks(rec, opts).map_err(|e| abort_err("pass", e))?;
        split_type_snaps(rec, opts).map_err(|e| abort_err("pass", e))?;
    } else if side.is_some_and(|s| s.start_rem == 0 && !s.frameful) {
        // a side trace that starts at a pre-count (head) label gets its live-in type checks hoisted to the start like a
        // rolled root (the pass builds the entry snapshot at that label); one that starts mid-block keeps the checks
        // at their first use
        entry_checks(rec, opts).map_err(|e| abort_err("pass", e))?;
    }
    if side.is_some() || stitched {
        // H1: forwarding / load CSE on the rolled segment of a side or stitched trace
        opt_loop::run_fwd(rec, opts).map_err(|e| abort_err("pass", e))?;
    }
    // R1: dense snapshots for a root that a carrying link can enter
    let carry_on = side.is_none()
        && !stitched
        && !act.prelude
        // SAFETY: the per-state data is live (see `rt::data_ptr`); read-only.
        && matches!(unsafe { (*crate::rt::data_ptr(st)).tr.cfg.limits.regcarry }, 1 | 3);
    if carry_on {
        // SAFETY: the per-state data is live (see `rt::data_ptr`); read-only.
        let lim = unsafe { &(*crate::rt::data_ptr(st)).tr.cfg.limits };
        carry_dense(&mut rec.ir, lim.carrymin as usize, lim.carrylive != 0);
    }
    if side.is_some() && !rec.ir.carry_in.is_empty() {
        carry_demote(
            rec,
            side.filter(|si| !si.no_link && !si.frame_link)
                .map(|si| si.link_snap),
        );
    }
    if let Some(si) = side.filter(|_| !rec.ir.carry_in.is_empty() || !rec.ir.carry_store.is_empty())
    {
        // the writeback stores the slots the parent's exit left stale too (carried loads stand for them); the dead numeric temporaries among them are left
        // out like F4 leaves them out of a snapshot at the link's pc
        if !si.no_link && !si.frame_link {
            if let Some(pi) = rec.ir.prune.clone() {
                let ls = rec.ir.snaps[si.link_snap as usize];
                let ir = &rec.ir;
                let carried = |r: ir::Ref| {
                    ir::TraceIr::is_ins(r) && {
                        let i = ir.at(r);
                        i.op == ir::TOp::Sload && ir::aux_bits(i.aux) & ir::SLOAD_CARRIED != 0
                    }
                };
                let keep: Vec<bool> = rec
                    .writeback
                    .iter()
                    .map(|e| {
                        !(carried(e.r)
                            && (pi.drops(&ls, e, ir.ty_of(e.r))
                                || mutants::active("carry_wb_prune_live")))
                    })
                    .collect();
                let mut k = keep.into_iter();
                rec.writeback.retain(|_| k.next().unwrap_or(true));
            }
        }
    }
    // P1: presized insertion groups (the notes are cleared either way: a NEWREF keeps its plain helper form)
    let groups = presize::run(&mut rec.ir);
    presize::GROUPS.fetch_add(groups as u64, std::sync::atomic::Ordering::Relaxed);
    Ok(())
}

/// Section 49 (`ccheck`): compiles compared with the old paths, and those that differed.
pub static CCHECK: [std::sync::atomic::AtomicU64; 2] = [
    std::sync::atomic::AtomicU64::new(0),
    std::sync::atomic::AtomicU64::new(0),
];

/// The allocation as text for the `ccheck` comparison (without the lazily built register masks).
fn ccheck_alloc_text(a: &ra::Allocation) -> String {
    let mut c = a.clone();
    c.used = Default::default();
    format!("{c:?}")
}

fn pipeline_inner(st: &mut State, act: &Active, rec: Recorded) -> Result<(), PipeErr> {
    let opts = &act.opts;
    // Section 49 (`cspeed` lever `REC`): the pipeline owns the recording (it copied it before; nothing reads it after a compile)
    let mut rec = if crate::cspeed::on(crate::cspeed::REC) {
        rec
    } else {
        #[allow(clippy::redundant_clone)]
        rec.clone()
    };
    let side = rec.side;
    // F2b: a root that ends with a link (a nest's outer loop): head bookkeeping and entry checks of a root, the end of a
    // side trace
    let link_root = side.is_some() && !act.side;
    // (a stitched *side* trace ends like a side trace: with an exit and no link, P-E2)
    let stitched = rec.stitched && side.is_none();
    // Section 49 (`ccheck`): the passes are compared too: the same front on a copy of the recording with the old paths
    // SAFETY: the per-state data is live (see `rt::data_ptr`); read-only.
    let ccheck_on = unsafe { (*crate::rt::data_ptr(st)).tr.cfg.limits.ccheck } != 0;
    let orig = ccheck_on.then(|| rec.clone());
    pipeline_front(st, act, &mut rec, link_root, stitched)?;
    if let Some(mut o) = orig {
        let slow = crate::cspeed::with(0, || pipeline_front(st, act, &mut o, link_root, stitched));
        if slow.is_err()
            || print::dump(&o.ir) != print::dump(&rec.ir)
            || format!("{:?}", o.writeback) != format!("{:?}", rec.writeback)
        {
            CCHECK[0].fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            CCHECK[1].fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            return Err(abort_err("ccheck", "cspeed differs: passes"));
        }
    }
    let ir = &rec.ir;
    if cache::verbose_aborts() {
        eprintln!(
            "[trace] install: extent {} depth {} (parent {} {})",
            ir.extent, ir.depth, act.max_extent, act.max_ci
        );
    }
    let mut side_room = false;
    if let Some(si) = side.filter(|s| s.frameful) {
        // S1: a side trace entered with virtual frames has no entry checks: it may not need more stack or spare `CallInfo`
        // nodes than the parent's checks guarantee. R2 (`siderooms`): one that does checks the room itself at its start.
        let _ = si;
        if (ir.extent > act.max_extent || ir.depth > act.max_ci)
            && !crate::tr::mutants::active("fside_room_ignored")
        {
            if !act.side_rooms {
                return Err(abort_err(
                    "side-room",
                    format!(
                        "extent {} > {} or depth {} > {}",
                        ir.extent, act.max_extent, ir.depth, act.max_ci
                    ),
                ));
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
        // SAFETY: as above.
        hkaddr: unsafe { (*crate::rt::data_ptr(st)).tr.cfg.limits.hkaddr } != 0,
        // SAFETY: as above.
        ixfuse: unsafe { (*crate::rt::data_ptr(st)).tr.cfg.limits.ixfuse } != 0,
        carry_at: if ir.carry.is_empty() {
            0
        } else {
            exit::lead_run(ir).map_or(0, |r| r.0)
        },
        ..RaOpts::from_trace(opts)
    };
    // R1: a side trace that may link to the third entry of the root of its head wants its writeback values in the registers the root holds them in
    let end_hints: Vec<ra::Loc> = match side {
        Some(si)
            if matches!(
                // SAFETY: the per-state data is live (see `rt::data_ptr`); read-only.
                unsafe { (*crate::rt::data_ptr(st)).tr.cfg.limits.regcarry },
                1 | 3
            ) && !si.no_link
                && !si.frame_link
                && !si.frameful
                && si.link_proto.0 == act.root
                && ir.loop_at.is_none() =>
        {
            // SAFETY: the root prototype is live (running frame).
            let rootp = unsafe { &*(act.root as *const rn_core::func::Proto) };
            let sig = cache::proto_ref(rootp)
                .and_then(|pr| {
                    cache::with_tr(st, pr, |tp| {
                        tp.head_index(si.link_pc)
                            .map(|i| tp.head(i).e3_sig_inst.get())
                    })
                    .flatten()
                })
                .unwrap_or(0);
            match exit::carry_info(sig) {
                Some(info) if sig != 0 => rec
                    .writeback
                    .iter()
                    .map(|e| {
                        info.dsts
                            .iter()
                            .find(|d| d.slot == e.slot)
                            .map_or(ra::Loc::None, |d| d.reg)
                    })
                    .collect(),
                _ => Vec::new(),
            }
        }
        _ => Vec::new(),
    };
    let t_ra = phase::now();
    let mut al = ra::allocate_hinted(ir, &end, &end_hints, &ropts).map_err(|e| {
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
        let without = RaOpts {
            xregs: ropts.xregs & !2,
            ..ropts.clone()
        };
        if let Ok(a2) = ra::allocate_hinted(ir, &end, &end_hints, &without) {
            if a2.carried_spills(ir) <= al.carried_spills(ir) {
                al = a2;
            }
        }
    }
    phase::add(phase::RA, t_ra);
    if act.verbose >= 2 && (!ir.carry_in.is_empty() || !ir.carry_store.is_empty()) {
        eprintln!(
            "[trace] carry-in: parent locs {:?}; stored at entry {:?}; allocation {:?}; writeback {:?}",
            ir.carry_in, ir.carry_store, al.carry, rec.writeback
        );
    }
    // tier-1 label addresses of the root and every inlined prototype (TR4: the code version installed now)
    let mut res = Res {
        protos: crate::fx::HashMap::default(),
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
    let exit_tags =
        (act.guarddiet && side.is_some()).then(|| side.map_or([0xFD; 256], |si| si.entry_tags));
    let (mut snaps, mut descs) =
        exit::build_exits_ex(ir, &al, &res, exit_tags).map_err(|e| abort_err("exits", e))?;
    phase::add(phase::EXITS, t_ex);
    // Section 49 (`ccheck`): the allocation and the exits of the old paths, compared now (the snapshots are completed below)
    // SAFETY: the per-state data is live (see `rt::data_ptr`); read-only.
    let ccheck = unsafe { (*crate::rt::data_ptr(st)).tr.cfg.limits.ccheck } != 0
        && !opts.emurun
        && !opts.emuverify;
    if ccheck {
        let slow = crate::cspeed::with(0, || -> Result<(), String> {
            let mut a2 = ra::allocate_hinted(ir, &end, &end_hints, &ropts)
                .map_err(|e| format!("slow allocation failed: {e:?}"))?;
            if ropts.xregs & 2 != 0 && a2.uses_gpr(10) && !mutants::active("xregs_rec_r14") {
                let without = RaOpts {
                    xregs: ropts.xregs & !2,
                    ..ropts.clone()
                };
                if let Ok(a3) = ra::allocate_hinted(ir, &end, &end_hints, &without) {
                    if a3.carried_spills(ir) <= a2.carried_spills(ir) {
                        a2 = a3;
                    }
                }
            }
            if ccheck_alloc_text(&a2) != ccheck_alloc_text(&al) {
                return Err("allocation".into());
            }
            let (s2, d2) = exit::build_exits_ex(ir, &a2, &res, exit_tags)?;
            if format!("{s2:?}{d2:?}") != format!("{snaps:?}{descs:?}") {
                return Err("exits".into());
            }
            Ok(())
        });
        CCHECK[0].fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        if let Err(what) = slow {
            CCHECK[1].fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            return Err(abort_err("ccheck", format!("cspeed differs: {what}")));
        }
    }
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
    // E4 (`lazyloop`, docs/JIT_TRACE.md section 35.1): the lazy decision of a candidate prototype with loops, at the compile of its first root
    // trace (before anything below reads `proto_lazy`): lazy-aware when that trace is its function-entry trace ending in a native self-call
    if !act.side {
        let on = cache::lazy_loop_rule(pc, ir.rec_call.is_some(), !ir.segs.is_empty());
        cache::lazy_loop_decide(st, rpr, pc, on);
        // E5: a first root trace without a folded method lookup turns folding off
        cache::midfold_first(st, rpr, ir.midfold);
    }
    // TR8: every exit continues through its cell, which starts at the tier-1 landing address. A prelude (F2a) is
    // reached from the head's dispatch cell: when its entry checks fail it continues in the root trace's entry, which
    // is what tier 1 would have reached through the dispatch cell without it. That holds for the snapshots that are
    // the state at the head (empty map, the head's pc): the entry checks and the budget exits.
    let mut init: Vec<usize> = snaps.iter().map(|s| s.land_addr as usize).collect();
    // S1: the cell of an exit with virtual-able frames is 0 until a side trace is attached (the stub / handler then
    // restores the frames); a trace that ends with a native frame push or a function-entry root keeps its landings
    let frame_cells_on = (!ir.fn_root && ir.rec_call.is_none() && ir.rec_ret.is_none())
        || !crate::tr::mutants::active("fcfn_cells_off");
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
    // R1: the carry cells of the exits (zero until a side trace compiled for register-carried entry is attached), behind the exit cells
    let carry_exits_on = carry_limit(st)
        && vbreg == Some(ra::BUDGET_GPR)
        && ir.rec_call.is_none()
        && ir.rec_ret.is_none()
        && ir.segs.is_empty()
        && !cache::proto_lazy(st, rpr);
    if carry_exits_on {
        let n = init.len();
        init.extend(std::iter::repeat_n(0, n));
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
    let mut carry_env = (0u64, 0u32);
    let side_env = match side {
        None => None,
        // P-E2: a side trace that ends with its always-taken exit before the root `RETURN` has no link target
        Some(si) if si.no_link => Some(asm::SideEnv {
            link_cell: 0,
            link_snap: si.link_snap,
            link2_cell: 0,
            link2_sig: 0,
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
            // A2: the guard-free entry of the root the head has now (same prototype, a plain frameless link: no prelude, frames or segments)
            let (link2_cell, link2_sig) = if !prelude
                && si.link_proto.0 == root
                && !si.frame_link
                && !si.frameful
                && act.side
                && rec.ir.segs.is_empty()
            {
                cache::with_tr(st, tpr, |tp| {
                    tp.head_index(si.link_pc).map(|i| {
                        let h = tp.head(i);
                        (
                            &h.e2 as *const std::cell::Cell<usize> as u64,
                            h.e2_sig_inst.get(),
                        )
                    })
                })
                .flatten()
                .filter(|&(_, sig)| sig != 0)
                .unwrap_or((0, 0))
            } else {
                (0, 0)
            };
            // R1: the root's third entry, offered to a link that may carry (`regcarry`)
            // SAFETY: the per-state data is live (see `rt::data_ptr`); read-only.
            if link2_cell != 0
                && matches!(
                    unsafe { (*crate::rt::data_ptr(st)).tr.cfg.limits.regcarry },
                    1 | 3
                )
            {
                carry_env = cache::with_tr(st, tpr, |tp| {
                    tp.head_index(si.link_pc).map(|i| {
                        let h = tp.head(i);
                        if h.e3_code.get() != 0 && h.e3_sig_inst.get() != 0 {
                            (h as *const HeadData as u64, h.e3_sig_inst.get())
                        } else {
                            (0, 0)
                        }
                    })
                })
                .flatten()
                .unwrap_or((0, 0));
            }
            Some(asm::SideEnv {
                link_cell,
                link_snap: si.link_snap,
                link2_cell,
                link2_sig,
            })
        }
    };
    // P-M3: the continuation heads of the root prototype (the targets of a native return), and the function-entry head's
    // dispatch cell (the target of the native self-call of a continuation trace)
    let lazy = cache::proto_lazy(st, rpr);
    // E2 (`idfold`, docs/JIT_TRACE.md section 34.3): the family rule. While the family is under the region invariant (folding on, or a folded
    // trace installed), no trace of it may store an upvalue (`(closure, index)` pairs may alias the self upvalue, P3d): the recording is
    // refused and folding goes off for good (the folded families are flushed at their next hot exit)
    let idf_env = if lazy {
        cache::idfold_cfg(st, rpr).1
    } else {
        None
    };
    if idf_env.is_some()
        && ir.ins.iter().any(|i| i.op == ir::TOp::Ustore)
        && !crate::tr::mutants::active("idfold_ustore_family")
    {
        cache::idfold_ban(st, rpr);
        return Err(abort_err(
            "idfold_ustore",
            "an upvalue store in a family under the region invariant",
        ));
    }
    // E5 (`midfold`, docs/JIT_TRACE.md section 35.4): a folded trace needs the region's checked entries, which only a lazy-aware family has (a
    // candidate with loops that was decided eager at this compile: refused, folding off for good); the family rule: while the family is under
    // the method invariant, no trace of it may store with the key or `__index` (or a key not known) into any table, or set a metatable
    if ir.midfold && !lazy {
        cache::midfold_ban(st, rpr);
        return Err(abort_err(
            "midfold_eager",
            "a folded method lookup in a family that is not lazy-aware",
        ));
    }
    let mf_env = if lazy {
        cache::midfold_cfg(st, rpr).1
    } else {
        None
    };
    if mf_env.is_some() && !crate::tr::mutants::active("midfold_store_family") {
        let (k, ix) = cache::midfold_keys(st, rpr);
        if !midfold_stores_ok(&ir, k, ix) {
            cache::midfold_ban(st, rpr);
            return Err(abort_err(
                "midfold_store",
                "a store that may change the method lookup in a family under the method invariant",
            ));
        }
    }
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
        if same {
            cs
        } else {
            None
        }
    } else {
        None
    };
    let mut rec_sites = [(0u64, 0u64, 0u8); cache::MAX_CONT_SITES];
    let mut rec_nsites = 0u8;
    let mut rec_link_cell = 0u64;
    if let Some(rc) = ir.rec_call {
        cache::mark_recursive(st, rpr);
        if rc.ninl != 0 {
            cache::unroll_ok(st, rpr);
        }
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
                    ir.rec_ret.is_none()
                        || w == n_ret
                        || (w < 0 && !lazy)
                        || mutants::active("rec_ret_wanted_unchecked")
                })
                .map(|h| {
                    let a = rn_core::instruction::Instruction(rootp.code()[h.pc as usize - 1]).a()
                        as u8;
                    (
                        code + 4 * u64::from(h.pc),
                        &h.rec as *const std::cell::Cell<usize> as u64,
                        a,
                    )
                })
                .collect();
            let c0 = tp.head_index(0).map_or(0, |i| {
                &tp.head(i).rec as *const std::cell::Cell<usize> as u64
            });
            (sites, c0)
        });
        let (sites, c0) = found.ok_or_else(|| abort_err("no-sites", "native return"))?;
        if cache::verbose_aborts() {
            eprintln!(
                "[trace] native sites for {}: {:x?} (rec_ret {:?}, rec_call {})",
                pc,
                sites,
                ir.rec_ret,
                ir.rec_call.is_some()
            );
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
    // T4 (`t4` bit 0): the guards of a native return at the end of the trace
    // SAFETY: the per-state data is live (see `rt::data_ptr`); read-only.
    let rec_late = unsafe { (*crate::rt::data_ptr(st)).tr.cfg.limits.t4 } & 1 != 0;
    let emurun = opts.emurun;
    let emuverify = opts.emuverify && !opts.emurun;
    // S1 (`lazystub`): deferred exit stubs. Not under the emulator (it runs the program it was given, not code assembled and patched in later).
    // SAFETY: the per-state data is live (see `rt::data_ptr`); read-only.
    let defer_on =
        unsafe { (*crate::rt::data_ptr(st)).tr.cfg.limits.lazystub != 0 } && !emurun && !emuverify;

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
        ci_spare_fn: cache::jit_tr_ci_spare as *const () as usize as u64,
        ci_spare_root_fn: if unsafe { (*crate::rt::data_ptr(st)).tr.cfg.limits.cispare } & 2 != 0 {
            cache::jit_tr_ci_spare_root as *const () as usize as u64
        } else {
            0
        },
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
        entry_tags: side.map_or([0xFD; 256], |si| si.entry_tags),
        guarddiet: act.guarddiet,
        room_have: (
            act.max_extent.max(rec.ir.extent),
            act.max_ci.max(rec.ir.depth),
        ),
        vcell: st.heap().virtual_cell() as u64,
        gc_cells: st.heap().jit_gc_cells(),
        num_names: [
            st.heap().num_type_name(false).as_ptr() as u64,
            st.heap().num_type_name(true).as_ptr() as u64,
        ],
        rec_sites,
        rec_nsites,
        rec_link_cell,
        lazy,
        seg_oracle: opts.verify && !ir.segs.is_empty(),
        link_mat: unsafe { (*crate::rt::data_ptr(st)).tr.cfg.limits.retf } & 2 != 0,
        prelude: act.prelude,
        lazy_leave_fn: exit::jit_tr_lazy_leave as *const () as usize as u64,
        // SAFETY: the per-state data is live (see `rt::data_ptr`); read-only.
        lazy_poison: unsafe { (*crate::rt::data_ptr(st)).tr.cfg.limits.lazypoison } != 0,
        // SAFETY: as above.
        dlink_snaps: unsafe { (*crate::rt::data_ptr(st)).tr.cfg.limits.dlink } & 1 != 0
            && !emurun
            && !emuverify,
        // SAFETY: as above.
        rec_vb_keep: unsafe { (*crate::rt::data_ptr(st)).tr.cfg.limits.dlink } & 2 != 0,
        // SAFETY: as above.
        slink_snaps: unsafe { (*crate::rt::data_ptr(st)).tr.cfg.limits.slink } & 3 != 0
            && !emurun
            && !emuverify,
        // SAFETY: as above.
        rec_late,
        lazy_cs,
        lazy_nparams: u16::from(rootp.numparams),
        idfold_uv: idf_env,
        midfold_kidx: mf_env,
        midfold_fn: cache::jit_tr_midfold_check as *const () as usize as u64,
        root_maxstack: u16::from(rootp.maxstacksize),
        // S1: a frameful side trace has no entry checks. CP2: neither has a frameless one whose needs the parent's checks cover (it is entered
        // by an exit of the parent, in the very frame the parent's checks promised room for); a prelude is entered through the head's cell
        // from tier 1 and keeps them
        no_room_checks: side.is_some_and(|s| {
            s.frameful
                || (!act.prelude
                    && ((ir.extent <= act.max_extent && ir.depth <= act.max_ci)
                        || crate::tr::mutants::active("side_nroom_skip")))
        }),
        side_room,
        frame_cells: frame_cells_on,
        // SAFETY: the per-state data is live (see `rt::data_ptr`); read-only.
        chain: unsafe { (*crate::rt::data_ptr(st)).tr.cfg.limits.chainre } != 0,
        root_code: rootp.code_ptr() as usize as u64,
        frame_link_snap: side
            .filter(|s| s.frame_link)
            .map_or(u32::MAX, |s| s.link_snap),
        setmt_inline: true,
        // SAFETY: as below.
        tnext_inline: unsafe { (*crate::rt::data_ptr(st)).tr.cfg.limits.tnextinl } & 2 != 0,
        // SAFETY: as below.
        rand_inline: unsafe { (*crate::rt::data_ptr(st)).tr.cfg.limits.randinl } != 0,
        next_hint_cell: std::ptr::addr_of_mut!(st.heap().next_hint) as u64,
        b2_arr: true,
        b2_tag: true,
        b2_hk: true,
        // SAFETY: as above.
        shape_min: unsafe { (*crate::rt::data_ptr(st)).tr.cfg.limits.shapemin },
        nopay: true,
        divmagic: true,
        hk_fuse: true,
        // SAFETY: as above.
        cloop: unsafe { (*crate::rt::data_ptr(st)).tr.cfg.limits.cloop } != 0,
        // SAFETY: as above.
        lea_add: unsafe { (*crate::rt::data_ptr(st)).tr.cfg.limits.leaadd } != 0,
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
        // SAFETY: as above.
        f2_byte_ext: unsafe { (*crate::rt::data_ptr(st)).tr.cfg.limits.bytext != 0 },
        ccalls_inline: unsafe { (*crate::rt::data_ptr(st)).tr.cfg.limits.ccalls != 0 },
        // SAFETY: as above.
        f2ifl: unsafe { (*crate::rt::data_ptr(st)).tr.cfg.limits.f2ifl != 0 },
        // SAFETY: as above.
        fibdiet: unsafe { (*crate::rt::data_ptr(st)).tr.cfg.limits.fibdiet },
        lean_sub: true,
        libcall: unsafe { (*crate::rt::data_ptr(st)).tr.cfg.limits.libcall },
        sink_stub: unsafe { (*crate::rt::data_ptr(st)).tr.cfg.limits.sinkstub != 0 },
        mat_fn: exit::jit_tr_mat as *const () as usize as u64,
        carry_head: carry_env.0,
        carry_sig: carry_env.1,
        stale_flag: if head != 0 && !ir.carry.is_empty() {
            head + cache::head_off::STALE as u64
        } else {
            0
        },
        carry_exits: carry_exits_on,
        // R1 (half A): the carry entry skips the room checks of the plain entry: the trace may not need more than the parent's checks guarantee
        // (section 50: a frameful side trace has no entry checks on either path; its R2 room checks, when it needs them, are in the body behind both
        // entries and save the two registers they use)
        carry_entry: (rec.ir.extent <= act.max_extent && rec.ir.depth <= act.max_ci)
            || side.is_some_and(|s| s.frameful),
        defer: defer_on,
    };
    let t_asm = phase::now();
    // E6 (`dlink`, docs/JIT_TRACE.md section 36.3): a side trace attached to an exit that can take a direct link gets a link entry: the parent's exit map
    // stores (the slot part of the parent's inline stub) and the exit's refund
    let carried = !ir.carry_in.is_empty() || !ir.carry_store.is_empty();
    let dlink_in = if act.side && !act.prelude && side.is_some() && !emurun && !emuverify {
        if carried {
            // Section 50 (`slink`): the carry link entry stores what the side trace does not take in registers (every entry of the exit map it does not
            // carry, and the carried ones whose tag the stack lacks: the stub's carry path stores those, tag and payload) and joins the carry entry
            cache::slink_source(st, act.id).map(|es| {
                let carried_slot = |slot: u16| -> Option<bool> {
                    ir.carry_in
                        .iter()
                        .find(|c| c.0 == slot)
                        .map(|c| c.2)
                        .or_else(|| ir.carry_store.iter().find(|c| c.0 == slot).map(|c| c.3))
                };
                let stores: Vec<exit::SlotRestore> = es
                    .slots
                    .iter()
                    .filter(|e| match carried_slot(e.slot) {
                        // (MUTANT `slink_store_skip`: the entries the side trace does not carry are not stored)
                        None => !mutants::active("slink_store_skip"),
                        // (MUTANT `slink_tag_skip`: the tags the stack lacks are not stored)
                        Some(stubbed) => stubbed && !mutants::active("slink_tag_skip"),
                    })
                    .copied()
                    .collect();
                let full =
                    exit::inline_stub_ops_slots(&es, &stores, ra::Loc::G(ra::BUDGET_GPR), None);
                let tail = exit::inline_stub_ops_slots(&es, &[], ra::Loc::G(ra::BUDGET_GPR), None);
                asm::DlinkIn {
                    ops: full[..full.len() - tail.len()].to_vec(),
                    refund: es.refund,
                    carry: true,
                    // (a side trace the stub's R1 carry path enters is not counted by the stub either: its counter is parked)
                    count: if ir.carry_link {
                        cache::slink_count(st, act.id)
                    } else {
                        None
                    },
                }
            })
        } else {
            let count = cache::slink_count(st, act.id);
            cache::dlink_source(st, act.id).map(|es| {
                let full =
                    exit::inline_stub_ops_slots(&es, &es.slots, ra::Loc::G(ra::BUDGET_GPR), None);
                let tail = exit::inline_stub_ops_slots(&es, &[], ra::Loc::G(ra::BUDGET_GPR), None);
                asm::DlinkIn {
                    ops: full[..full.len() - tail.len()].to_vec(),
                    refund: es.refund,
                    carry: false,
                    count,
                }
            })
        }
    } else {
        None
    };
    let mut a = asm::assemble_dl(ir, &al, &snaps, &rec.writeback, &env, dlink_in.as_ref())
        .map_err(|e| {
            if act.verbose >= 2 {
                eprintln!("[trace] FAILIR\n{}\n{}", print::dump(ir), ra::dump(ir, &al));
            }
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
    let dl_ok = a.dl_ok.clone();
    let emu_prog = (emurun || emuverify).then(|| a.prog.clone());
    let mut entry = a.entry;
    if emurun {
        a.prog = Program::default();
        entry = emurun_stub(&mut a.prog, &env, act.id.slot);
    } else if emuverify {
        entry = emuverify_stub(&mut a.prog, a.entry, act.id.slot);
    }
    // A2: the second entry exists only for the real program (the emulator stubs enter through `entry`)
    let e2_label = if emurun || emuverify { None } else { a.entry2 };
    let t_x86 = phase::now();
    // S1: the references to the deferred stubs' labels are wanted (the handler patches them to the stub)
    let mut want: Vec<bool> = Vec::new();
    for &(_, l) in &a.defer_g {
        if want.len() <= l as usize {
            want.resize(l as usize + 1, false);
        }
        want[l as usize] = true;
    }
    let em = x86::emit_ex(&a.prog, &want).map_err(|e| abort_err("x86", format!("x86: {e:?}")))?;
    phase::add(phase::X86, t_x86);
    if ccheck && !emurun && !emuverify {
        // Section 49 (`ccheck`): the assembler and the encoder of the old paths on the same (checked) allocation and exits
        let same = crate::cspeed::with(0, || {
            asm::assemble_dl(ir, &al, &snaps, &rec.writeback, &env, dlink_in.as_ref())
                .ok()
                .and_then(|a2| {
                    let mut want2: Vec<bool> = Vec::new();
                    for &(_, l) in &a2.defer_g {
                        if want2.len() <= l as usize {
                            want2.resize(l as usize + 1, false);
                        }
                        want2[l as usize] = true;
                    }
                    x86::emit_ex(&a2.prog, &want2).ok()
                })
                .is_some_and(|e2| {
                    e2.code == em.code && e2.label_off == em.label_off && e2.refs == em.refs
                })
        });
        if !same {
            CCHECK[1].fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            return Err(abort_err("ccheck", "cspeed differs: machine code"));
        }
    }
    let entry_off =
        em.label_off[entry as usize].ok_or_else(|| abort_err("x86", "entry unbound"))?;
    // S1: the offsets of the patchable `nop`s of the deferred snapshots and of the shared exit routine
    let mut doffs: Vec<(u32, usize)> = Vec::with_capacity(a.defer_g.len());
    for &(k, l) in &a.defer_g {
        let off =
            em.label_off[l as usize].ok_or_else(|| abort_err("x86", "deferred stub unbound"))?;
        doffs.push((k, off));
    }
    // (only with deferred stubs: under the emulator `a.prog` was replaced and the label means nothing in `em`)
    let shared_off = if doffs.is_empty() {
        None
    } else {
        a.shared_label
            .and_then(|l| em.label_off.get(l as usize).copied().flatten())
    };
    // S1: the branches to each deferred stub's `nop` (a `jmp` or `jcc rel32`, checked by their opcode bytes; anything else keeps going through the `nop`)
    let mut dsites: Vec<Vec<(usize, usize)>> = Vec::new();
    if !doffs.is_empty() {
        dsites = vec![Vec::new(); snaps.len()];
        let mut snap_of: Vec<u32> = vec![u32::MAX; want.len()];
        for &(k, l) in &a.defer_g {
            snap_of[l as usize] = k;
        }
        for &(l, pos, end) in &em.refs {
            let k = snap_of.get(l as usize).copied().unwrap_or(u32::MAX);
            let branch = (pos >= 1 && em.code[pos - 1] == 0xE9)
                || (pos >= 2 && em.code[pos - 2] == 0x0F && em.code[pos - 1] & 0xF0 == 0x80);
            if k != u32::MAX && branch {
                dsites[k as usize].push((pos, end));
            }
        }
    }
    let defer_ctx = a.defer.take();
    let e2_off = e2_label.and_then(|l| em.label_off[l as usize]).unwrap_or(0);
    let e2n_off = if e2_off != 0 {
        a.entry2n
            .and_then(|l| em.label_off[l as usize])
            .unwrap_or(0)
    } else {
        0
    };
    let e2_sig = if e2_off != 0 {
        exit::e2_intern_room(&a.e2_req, ir.extent, ir.depth)
    } else {
        0
    };
    // R1: the third entry (only for the real program, like the second)
    let (e3_off, e3_sig) = match a.entry3 {
        Some(l) if e2_off != 0 && !emurun && !emuverify && !a.carry.is_empty() => {
            match (em.label_off[l as usize], vbreg) {
                (Some(off), Some(r)) if off != 0 => (
                    off,
                    exit::carry_intern(exit::CarryInfo {
                        dsts: a.carry.clone(),
                        budget: a.carry_budget,
                        budget_reg: r,
                        room: (ir.extent, ir.depth),
                        e2sig: e2_sig,
                    }),
                ),
                _ => (0, 0),
            }
        }
        _ => (0, 0),
    };
    if act.verbose >= 2 && (e3_off != 0 || carry_env.1 != 0) {
        eprintln!(
            "[trace] carry: root e3 {e3_off:#x} sig {e3_sig} dsts {:?}; link to sig {} (head {:#x})",
            a.carry, carry_env.1, carry_env.0
        );
    }
    // Q3: the shape of the compile (IR + register allocation text, free of trace numbers and addresses of the
    // trace's own counters) identifies a re-compile that would repeat a flushed generation
    // (P-G: hashed from the structures; the text itself is built only for `RUSTYNAIL_JIT_DUMP`)
    let t_h = phase::now();
    let hash = cache::shape_hash_ir(ir, &al);
    phase::add(phase::HASH, t_h);
    let t_inst = phase::now();
    let text = act
        .dump
        .then(|| format!("{}\n{}", print::dump(ir), ra::dump(ir, &al)));
    let newid = cache::install(
        st,
        act.id,
        cache::TraceInstall {
            code: em.code,
            entry_off,
            inner_off: a.inner.and_then(|l| em.label_off[l as usize]).unwrap_or(0),
            e2_off,
            e2n_off,
            e2_sig,
            e3_off,
            e3_sig,
            ce_off: if emurun || emuverify {
                0
            } else {
                a.entry_c
                    .and_then(|l| em.label_off[l as usize])
                    .unwrap_or(0)
            },
            rec_ret: ir.rec_ret.is_some(),
            ret_late_snap: (ir.rec_ret.is_some() && rec_late)
                .then(|| ir.ins.last().map(|i| ir::aux_snap(i.aux)))
                .flatten(),
            dl_off: if emurun || emuverify {
                0
            } else {
                a.entry_dl
                    .and_then(|l| em.label_off[l as usize])
                    .unwrap_or(0)
            },
            carry_link: ir.carry_link,
            text,
            hash,
            weight: ir.ins.len() as u32,
            unrolled: ir.rec_call.is_some_and(|rc| rc.ninl != 0),
            idfold: ir.idfold,
            midfold: ir.midfold,
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
    cache::note_inner(st, rec.inner_link && !act.side, rec.inner_ret);
    // T9 (`innerlink`): the head this root's frame-linking exit enters (the return half reads it)
    if let Some(si) = side.filter(|s| s.frame_link && !act.side) {
        // SAFETY: an inlined callee of the trace, registered as a dependency (TR32) and live.
        let p = unsafe { &*(si.link_proto.0 as *const rn_core::func::Proto) };
        if let Some(lpr) = cache::proto_ref(p) {
            cache::mark_linked_in(st, lpr, si.link_pc);
        }
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
    // S1: the deferred stubs (where their `nop`s are)
    let defer = match (defer_ctx, shared_off, cache::trace_block(st, newid)) {
        (Some(ctx), Some(so), Some((base, _))) if !doffs.is_empty() => {
            let n = snaps.len();
            let mut patch_at = vec![0usize; n];
            for &(k, off) in &doffs {
                patch_at[k as usize] = base + off;
            }
            for v in dsites.iter_mut() {
                for (pos, end) in v.iter_mut() {
                    *pos += base;
                    *end += base;
                }
            }

            Some(Box::new(exit::DeferInfo {
                ctx,
                patch_at,
                sites: dsites,
                shared_exit: base + so,
                built: (0..n).map(|_| std::cell::Cell::new(0)).collect(),
                failed: (0..n).map(|_| std::cell::Cell::new(false)).collect(),
                shared: Default::default(),
                dl_ok: {
                    let mut v = vec![false; n];
                    for &k in &dl_ok {
                        if let Some(x) = v.get_mut(k as usize) {
                            *x = true;
                        }
                    }
                    v
                },
            }))
        }
        _ => None,
    };
    if !doffs.is_empty() && defer.is_none() {
        // (never: a deferred stub always jumps to the embedded shared exit routine, and the trace was installed just now)
        cache::request_flush(st, newid, cache::FlushWhy::Off);
        return Err(PipeErr {
            kind: Fail::Done,
            detail: "lazystub: no shared exit routine".into(),
        });
    }
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
        extent: if side_room && !crate::tr::mutants::active("side_room_meta_full") {
            act.max_extent
        } else {
            ir.extent
        },
        depth: if side_room && !crate::tr::mutants::active("side_room_meta_full") {
            act.max_ci
        } else {
            ir.depth
        },
        frame_cells: frame_cells_on,
        lazy,
        seg_oracle: if opts.verify && !ir.segs.is_empty() {
            al.nspill + asm::ORACLE_SLOT_K + 1
        } else {
            0
        },
        defer,
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
            ci_spare_fn: env.ci_spare_fn,
            ci_spare_root_fn: env.ci_spare_root_fn,
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
    // E1: the code-growth rule of an unrolled family (after everything above used the new trace: it may flush the family)
    if act.side {
        cache::unroll_code_rule(st, newid);
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
        } else if f == self.t.entry_fail_fn
            || f == self.t.ci_spare_fn
            || f == self.t.ci_spare_root_fn
        {
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
    // S4: a native shape-guard exit is not replayed: its handler has just filled the table's layout id, so the emulation (which reads the live table) would
    // pass the guard that the native run missed and leave somewhere else
    if real_area.exitno & exit::EXIT_SHAPE != 0 {
        return;
    }
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
    // S4: the emulation reads the live tables, which the native run has already changed: an inline `NEWREF` cleared a layout id (the emulated shape guard
    // misses where the native one passed), and a native shape-miss exit filled a zero id before this probe (the emulated guard passes where the native one
    // missed). Such a pair of runs is not comparable (the exits agree up to the shape guard, which tests the table, not the trace's state).
    if emu_area.trace_id == real_area.trace_id
        && emu_area.exitno != real_area.exitno
        && (emu_area.exitno | real_area.exitno) & exit::EXIT_SHAPE != 0
    {
        return;
    }
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
        assert_eq!(exit_area::SHAPE_NOFILL, exit_area::LAZY_ROOT + 8);
        // (E5: the verified method pair behind `shape_fills`)
        assert_eq!(exit_area::MF_RECV, exit_area::LAZY_ROOT + 24);
        assert_eq!(exit_area::MF_CL, exit_area::LAZY_ROOT + 32);
        assert_eq!(exit_area::SIZE, exit_area::LAZY_ROOT + 40);
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
        | u8::from(mutants::active("tailfast_dyn_count_short")) << 4
        | u8::from(mutants::active("tailfast_dyn_top_high")) << 5
}

/// BUNDLE-B item 2: the active `tset` mutants as the bit set of `templates_table::NS_*`.
pub(crate) fn nilset_mutants() -> u8 {
    use crate::templates_table::{
        NS_ARR_MT_SKIP, NS_BARRIER_SKIP, NS_FLAGS_SKIP, NS_GET_NIL_HIT, NS_GET_TAG_SKIP,
        NS_INT_KEY_SKIP, NS_MT_SKIP, NS_STR_TAG_SKIP,
    };
    u8::from(mutants::active("nilset_flags_skip")) * NS_FLAGS_SKIP
        | u8::from(mutants::active("nilset_mt_skip")) * NS_MT_SKIP
        | u8::from(mutants::active("nilset_barrier_skip")) * NS_BARRIER_SKIP
        | u8::from(mutants::active("nilset_arr_mt_skip")) * NS_ARR_MT_SKIP
        | u8::from(mutants::active("tset_str_tag_skip")) * NS_STR_TAG_SKIP
        | u8::from(mutants::active("tset_int_key_unchecked")) * NS_INT_KEY_SKIP
        | u8::from(mutants::active("tget_nil_hit")) * NS_GET_NIL_HIT
        | u8::from(mutants::active("tget_tag_skip")) * NS_GET_TAG_SKIP
}

/// H2b: the active mutants of the inline insertion of a new key (tier-1 `SETFIELD`) as the bit set of `templates_table::NK_*`.
pub(crate) fn newkey_mutants() -> u8 {
    use crate::templates_table::{
        NK_BLACK_SKIP, NK_DUMMY_SKIP, NK_FLAGS_SKIP, NK_LAYOUT_SKIP, NK_MAINPOS, NK_MT_SKIP,
        NK_NIL_VALUE, NK_NONEMPTY,
    };
    u8::from(mutants::active("newkey_black_skip")) * NK_BLACK_SKIP
        | u8::from(mutants::active("newkey_nil_value")) * NK_NIL_VALUE
        | u8::from(mutants::active("newkey_nonempty_skip")) * NK_NONEMPTY
        | u8::from(mutants::active("newkey_mt_skip")) * NK_MT_SKIP
        | u8::from(mutants::active("newkey_flags_skip")) * NK_FLAGS_SKIP
        | u8::from(mutants::active("newkey_layout_skip")) * NK_LAYOUT_SKIP
        | u8::from(mutants::active("newkey_mainpos_off")) * NK_MAINPOS
        | u8::from(mutants::active("newkey_dummy_skip")) * NK_DUMMY_SKIP
}

/// Section 44.1 (T6): the active mutants of the fused constructor of tier 1 as the bit set of `templates_ctor::CT_*`.
pub(crate) fn ctor_mutants() -> u8 {
    use crate::templates_ctor::{
        CT_ANY_REG, CT_COLLIDE, CT_FLAGS_SKIP, CT_LAYOUT_SKIP, CT_MAINPOS, CT_NIL_VALUE,
    };
    u8::from(mutants::active("ctor_nil_value")) * CT_NIL_VALUE
        | u8::from(mutants::active("ctor_flags_skip")) * CT_FLAGS_SKIP
        | u8::from(mutants::active("ctor_layout_skip")) * CT_LAYOUT_SKIP
        | u8::from(mutants::active("ctor_mainpos_off")) * CT_MAINPOS
        | u8::from(mutants::active("ctor_collide")) * CT_COLLIDE
        | u8::from(mutants::active("ctor_any_reg")) * CT_ANY_REG
}

/// Section 44.1 (T6): the active mutants `(t6_mm_big_skip, t6_mm_conv_skip)` of the mixed integer / float `math.min` / `math.max` of tier 1.
pub(crate) fn t6_minmax_mut() -> (bool, bool) {
    (
        mutants::active("t6_mm_big_skip"),
        mutants::active("t6_mm_conv_skip"),
    )
}

/// BUNDLE-K item B: the active mutants `(strself_tag_skip, strself_mt_skip, strself_nil_hit)` of the inline string `SELF` of tier 1.
pub(crate) fn strself_mut() -> (bool, bool, bool) {
    (
        mutants::active("strself_tag_skip"),
        mutants::active("strself_mt_skip"),
        mutants::active("strself_nil_hit"),
    )
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
    if mutants::active("t1i_setmt_gc_skip") {
        m |= t::M_SETMT_GC;
    }
    if mutants::active("t1i_setmt_black_skip") {
        m |= t::M_SETMT_BLACK;
    }
    if mutants::active("t1i_setmt_old_skip") {
        m |= t::M_SETMT_OLD;
    }
    if mutants::active("t1i_setmt_arg_skip") {
        m |= t::M_SETMT_ARG;
    }
    if mutants::active("t1i_tail_count_extra") {
        m |= t::M_TAIL_COUNT;
    }
    if mutants::active("t1i_tail_mm_skip") {
        m |= t::M_TAIL_MM;
    }
    m
}

/// T3: the active mutants `(t3idx_nil_hit, t3idx_use_mt)' of the inline `__index` level of the tier-1 table templates.
pub(crate) fn t3idx_mut() -> (bool, bool) {
    (
        mutants::active("t3idx_nil_hit"),
        mutants::active("t3idx_use_mt"),
    )
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
    u8::from(mutants::active("armm_count_skip"))
        | u8::from(mutants::active("armm_land_skip")) << 1
        | u8::from(mutants::active("armm_pay_skip")) << 2
        | u8::from(mutants::active("armm_never")) << 3
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
    (
        mutants::active("c2cmp_swap"),
        mutants::active("c2cmp_exit_skip"),
    )
}
