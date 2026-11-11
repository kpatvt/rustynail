//! The trace recorder (docs/JIT_TRACE.md section 4): the protocol and state machine, the prototype opcodes of 4.3, the
//! lockstep check (TR17), the count model (TR5.1) and the snapshots (TR3, TR3a, TR22b, see [`super::snap`]). P3a: the
//! root frame; P3b: `CALL`/`RETURN*` of Lua closures (inlined, 4.4), the `math` intrinsics (TR12), dependency edges at
//! inline time (TR32) and the entry-guard marks (TR6d); decisions in docs/JIT_TRACE_REF.md "P3b decisions".
//!
//! # Protocol
//!
//! The recorder is a [`RecVTable`] consumer ([`VTABLE`]): `rn-core`'s hooked interpreter copy calls [`step`] before
//! every instruction of the recorded frame and [`abort`] when the VM ends the recording (error, hook, frame switch,
//! close). The recording is armed with [`arm`] (what `jit_tr_hot` does at a hot loop head, P4/P6; tests do it from a
//! native). Its state lives in a per-OS-thread cell (one recording per thread, TR16); PI moves it behind `JitSlot.data`
//! without changing this interface. The result is left in a second cell and fetched with [`take_outcome`].
//!
//! ```text
//!   arm  -->  WaitHead (only with RecCfg::wait_head: tests) --pc == H--> Rec --pc == H again--> Closed
//!                                                            |
//!                                                            +--> Aborted(reason)
//! ```
//!
//! A step (1) validates the frame (TR23.5: token, root `CallInfo`, function slot, closure, expected successor pc),
//! (2) checks the previous instruction in lockstep (TR17: the IR interpreter's value of every tracked slot and of every
//! stored-to table / upvalue equals the VM's), (3) closes the loop when `pc == H` again, or (4) records the instruction.
//! Closing produces the *rolled* iteration of 7.8 (`loop_at = None`): `POLL`, `BUDGET n`, the recorded iteration with
//! guards at their instructions, and the writeback list (the slots the iteration wrote and their final refs, which is
//! also the `S_H` map of the peeled layout). P7 turns it into the peeled layout.
//!
//! # Conventions the IR leaves open (decisions, docs/JIT_TRACE_REF.md "P3a decisions")
//!
//! * The running closure is `SLOAD` of the pseudo slot [`FUNC_SLOT`] (type `Lcf`, unchecked): a trace may be entered by
//!   any closure of the proto and TR7 forbids embedding closure addresses.
//! * The TR15 range guard is `UGE uvrel, 16 * nslots` where `uvrel = UVREL uref` is the address of the upvalue's value
//!   relative to the root base (`TOp::Uvrel`, P3c; `interp` reads `Model::uv_rel`).
//! * String constants are `KPTR` instructions of type `ShrStr`.

use super::interp::{self, Model, Val};
use super::ir::*;
use super::mutants;
use super::snap::{self, SlotMap, SnapMeta};
use crate::fx::{HashMap, HashSet};
use crate::lower::calls::CallEnv;
use crate::lower::{lower, LabelKind, LowerCfg, Lowered};
use rn_core::func::{LClosure, Proto};
use rn_core::instruction::Instruction;
use rn_core::jit_hooks::{self, Abort, RecStep, RecVTable};
use rn_core::meta::Tm;
use rn_core::number::{eq_num, float_to_int, le_num, lt_num, F2I};
use rn_core::state::{Ci, State, StkId, CIST_C, CIST_MMRET, LUA_MINSTACK};
use rn_core::string::StrRef;
use rn_core::table::TableRef;
use rn_core::value::*;
use rn_core::ver::active::OpCode;
use std::cell::RefCell;
use std::rc::Rc;

/// A1: the hash part of a table as the recorder tracks it: per node `(key tag, key payload, chain link, value is empty)` and the
/// `lastfree` index (`Table::hash_image`).
type NrImage = rn_core::table::HashImage;

/// The pseudo slot that `SLOAD` reads the running closure from (the function slot below the root base).
pub const FUNC_SLOT: u16 = u16::MAX;

/// Most recorded steps of one iteration before `TooLong`.
pub const MAX_PATH: usize = 8000;
/// Most array elements the lockstep model may *address* (`Charge`, [`RecCfg::max_model_elems`]): the model array of a
/// table is a zero-initialised allocation of its size (untouched pages cost nothing), so this bounds virtual memory;
/// the elements really copied are the ones the IR reads or stores (at most one per recorded instruction).
pub const MAX_MODEL_ELEMS: usize = 1 << 26;
/// Most extent (slots) of a trace (`TooLong`, 4.6).
pub const MAX_EXTENT: usize = 200;
/// Largest integer that converts to a double exactly.
const TWO53: i64 = 1 << 53;
/// Most IR instructions of a side trace (7.10: side-trace length 1000 IR).
pub const MAX_SIDE_INS: usize = 1000;
/// Deepest inline depth (4.4; the root is depth 0).
pub const MAX_DEPTH: usize = 4;
/// P-E2: most arguments of an inlined `table.pack`.
pub const MAX_PACK: usize = 64;
/// F3: largest preallocated array part (`NEWTABLE C`) a trace allocates (the lockstep model holds a vector of that size).
pub const MAX_NEW_ASIZE: usize = 1 << 12;
/// F3: largest `NEWTABLE B` (hash part of `2^(B-1)` nodes) a trace allocates.
pub const MAX_NEW_HBITS: u32 = 12;
/// P-A: the longest `__index` chain (tables only) a trace follows, and the longest node chain an absent-key proof walks.
const MAX_INDEX_CHAIN: usize = 4;
const MAX_MISS_CHAIN: usize = 8;

/// Why a recording stopped without a trace (4.6). Reasons that need calls (`CFunc`, `MultRet`, `TailCall` at depth,
/// `Recursion`, `Depth`) and the pipeline (`DepFreed`, `Charge`, `Cap`, `Compile` beyond `const_guard`) belong to later
/// tasks and are listed for completeness.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AbortReason {
    /// An opcode outside the recorded set (the raw opcode).
    Nyi(u8),
    /// A call of a C function.
    CFunc,
    /// An operand type or table state that would call a metamethod function, or an error the recorder predicts.
    Metamethod,
    /// String -> number coercion.
    Coerce,
    /// A table with a metatable on a path that needs its absence.
    MetaTable,
    /// Hash-part access (or a non-integer key).
    TableHash,
    /// A nil array element read.
    TableNil,
    /// R2: an integer key outside the array part (data: a table that has not grown yet, not a property of the code: no blacklisting).
    IntMiss,
    /// A store to an empty array slot (a possible insertion).
    Insert,
    /// `VARARG` or a vararg callee.
    Vararg,
    /// A multi-value call or return.
    MultRet,
    /// `TAILCALL`.
    TailCall,
    /// A recursive call.
    Recursion,
    /// Inline depth exceeded.
    Depth,
    /// The root frame returns.
    LeaveFrame,
    /// P-E2: a function-entry recording ended (at the root `RETURN`) before `fnmin` bytecodes: data (a leaf call), not a
    /// fault of the head.
    FnShort,
    /// The loop exited, or a different loop head was reached before `H`.
    LeaveLoop,
    /// A loop head inside the recorded path.
    InnerLoop,
    /// More than the instruction, snapshot, path or extent limits.
    TooLong,
    /// A slot's type at the second visit of `H` differs from its type at the start (TR24c).
    Unstable,
    /// A proto with an edge to the recording was freed.
    DepFreed,
    /// An open upvalue points into the trace's frames.
    UpvalAlias,
    /// A hook was installed.
    Hook,
    /// N3a (RETF): an upvalue store into the region below the root frame was recorded before a `RETF` (the caller's slots would be stale).
    RetfAlias,
    /// An instruction raised.
    Error,
    /// A callback from an unexpected frame or thread (validation, TR23.5).
    Frame,
    /// Lockstep mismatch (TR17).
    Diverge,
    /// The state is closing.
    Close,
    /// A memory or code charge was refused.
    Charge,
    /// `maxtrace` reached.
    Cap,
    /// A later pipeline stage failed (`Compile(reason)`).
    Compile(&'static str),
    /// F2a: a side trace shorter than `TrLimits::sidemin` bytecodes (an opt-in knob, default 0) is not installed.
    SideShort,
    /// F2a: a side recording reached a loop head that has no installed trace (7.8: reaching an untraced head aborts).
    SideUntraced,
    /// P-B: the loop allocates strings in every iteration and the collector steps about as often (a trace would exit at each
    /// allocation: see `RecCfg::storm_step`).
    AllocStorm,
}

impl From<IrAbort> for AbortReason {
    fn from(e: IrAbort) -> AbortReason {
        match e {
            IrAbort::TooLong => AbortReason::TooLong,
            IrAbort::ConstGuard => AbortReason::Compile("const_guard"),
        }
    }
}

/// Configuration of one recording.
#[derive(Clone)]
pub struct RecCfg {
    /// N6: dynamic short-string keys record as `HREFV` (`RUSTYNAIL_JIT_HREFV=0` turns it off: every computed key is specialised on
    /// its address by an `EQREF`, as before).
    pub hrefv: bool,
    /// Constant folding while recording (`-fold`).
    pub fold: bool,
    /// CSE while recording (`-cse`).
    pub cse: bool,
    /// Ignore steps until the root frame reaches the loop head (tests arm earlier than `H`); production arms at `H`
    /// and a first step elsewhere aborts.
    pub wait_head: bool,
    /// Number of in-place re-records after `Unstable` (TR24c): the recorder restarts at `H` with the types the
    /// finished iteration produced. `0`: the recording aborts with `Unstable` and the driver re-arms (3.3, `rerec`).
    pub restarts: u32,
    /// `+verify`: a lockstep mismatch is a panic instead of `Abort::Diverge`.
    pub verify: bool,
    /// BUNDLE-M C1: run the lockstep cross-check (TR17). Off: the step only does the bookkeeping the recorder itself depends on
    /// ([`Rec::light_new`]); the IR is the same.
    pub lockstep: bool,
    /// The tier-1 block structure of the root proto (any code version has the same, `BlockSig`); computed by
    /// [`lower_for_landing`] when `None`.
    pub landing: Option<Rc<Lowered>>,
    /// P-G: take the block structures of the root and of inlined callees from the prototypes' caches
    /// (`cache::landing_cached`; always on in the pipeline) instead of lowering them again.
    pub landcache: bool,
    /// The driver's side of TR32 / TR4b (see [`DepHooks`]); `None`: inlining records no edges (tests, standalone use).
    pub deps: Option<DepHooks>,
    /// Most array elements the lockstep model may address in total (`Charge`); [`MAX_MODEL_ELEMS`] in production.
    pub max_model_elems: usize,
    /// F2a: record a side trace from the pc `arm` is given (a hot exit of a parent trace) instead of a loop from its
    /// head; `None`: a root loop (every other field of this configuration applies to both).
    pub side: Option<SideCfg>,
    /// F2b: nest linking for a **root** recording: reaching a loop head of the root frame (other than `H`) whose root
    /// trace is installed ends the recording with a link into it (`Recorded.side`, 7.8 "Nests"); a head without a
    /// trace keeps the prototype's behaviour (`InnerLoop`). `None`: no linking (prototype shape, tests).
    pub nest: Option<HeadQuery>,
    /// F2b: linking exits: a root recording that, inside an inlined frame, reaches a loop head of that frame's prototype
    /// whose root trace is installed ends with an exit that materialises the frames and continues at that head's
    /// dispatch cell (heapsort's `sift`). The question is asked for (prototype, pc).
    pub nest_frame: Option<FrameHeadQuery>,
    /// C38 slice 1 (`sidenest`): `nest_frame` also applies to a **side** recording: reaching, inside an inlined frame of a side trace, a loop head of that
    /// frame's prototype whose root trace is installed ends the side trace with the same linking exit (instead of unrolling the loop under `loopany`
    /// or aborting `InnerLoop`). `nest_frame_roots`: the query applies to root recordings (`nest >= 2`).
    pub nest_frame_sides: bool,
    /// See `nest_frame_sides`.
    pub nest_frame_roots: bool,
    /// F2b: stitching at a C function call: a root recording that has executed at least this many bytecodes when it
    /// reaches a `CALL` of a C function it cannot record ends with an always-taken exit at the `CALL` (tier 1 performs
    /// the call and runs on to the next dispatch of the head). 0: off (the call aborts the recording, `CFunc`).
    pub stitch_min: u32,
    /// F4: recursion unroll: a call of a prototype that is already on the frame stack `unroll` times (the root counts)
    /// is not inlined: the recording ends there with a stitching exit (or aborts `Recursion` when it cannot stitch).
    /// 0: any recursion aborts `Recursion` (the prototype's rule).
    pub unroll: u32,
    /// P-E: backward edges of inlined callees' loops that are unrolled per recording (0 = none: `InnerLoop`).
    pub loop_unroll: u32,
    /// P3 (`ncallee`): callee identity guards (`EQREF`) never use the entry snapshot (see [`TraceIr`] TR6d): a callee mismatch is an ordinary body exit.
    pub ncallee: bool,
    /// P2 bundle 2 (`uvsep`): a `UREF` carries the identity of the real `UpVal` it was recorded on (`aux`), for the loop optimiser's guarded disjointness.
    pub uvsep: bool,
    /// P-E: inlining depth limit (`maxdepth`, at most [`exit::MAX_FRAMES`]); 0 = the default [`MAX_DEPTH`].
    pub max_depth: u32,
    /// P-E2: a function-entry root: the head is pc 0 of the root frame's prototype and the recording ends, with the
    /// stitching-style exit, before the `RETURN` / `TAILCALL` of the root frame (tier 1 performs the return).
    pub fn_root: bool,
    /// P-M3: native self-recursion: the first self-call of the root frame of a function-entry recording ends the
    /// recording with a native frame push (`TraceIr::rec_call`); with `rec_only` nothing else of a function root is
    /// recorded (a recording that reaches the root's `RETURN` / `TAILCALL` or a stitching point aborts).
    pub rec_native: bool,
    /// P-M3: function-entry heads exist for native recursion only (`fnroot = 0`).
    pub rec_only: bool,
    /// P-M3: the root is a continuation head (the pc after a self-`CALL`).
    pub rec_cont: bool,
    /// P-M3: continuation heads and native returns exist (`reccont`).
    pub rec_conts: bool,
    /// R2: native `RETURN0` (`TrLimits::recany`).
    pub rec_any: bool,
    /// B3-4: `CONCAT` takes number operands itself (no `SNEW` per number; `TrLimits::cnum`).
    pub cnum: bool,
    /// R2: stores into existing hash nodes that hold nil are recorded (`TrLimits::nilstore`).
    pub nil_store: bool,
    /// R2: a native self-call of the root prototype was recorded before (`recany` gates native returns on it).
    pub rec_seen: bool,
    /// J2-2: most IR instructions of this recording (0 = the general limit): a deep recording's bound.
    pub max_ins: usize,
    /// P-E2: record `table.pack(...)` of a fixed argument count (see `Rec::pack_call`).
    pub pack: bool,
    /// N7: record `ipairs(t)` / `pairs(t)`, `TFORPREP` and the end of a traversal (see `Rec::gfor_call`).
    pub gfor: bool,
    /// P-O: record `pcall` / `xpcall` of a Lua closure (see `Rec::pcall_call`).
    pub pcall: bool,
    /// N3: record metamethods that are Lua closures (`__add` ... `__concat`, `__len`, ...) as inlined frames (see `Rec::mm_push`).
    pub mm: bool,
    /// N3b: record `setmetatable(t, mt)` (see `Rec::setmt_call`).
    pub setmt: bool,
    /// F1: `x ^ 2` is `x * x`.
    pub f1pow: bool,
    /// F1: `math.sin` / `cos` / `exp` / `log` and `math.min` / `math.max` of more than two arguments.
    pub f1math: bool,
    /// F1: `string.len(s)` / `s:len()`.
    pub f1strlen: bool,
    /// BUNDLE-G1: `string.format("%s", s)`.
    pub g1fmt: bool,
    /// F2: record integer `math.random(m [, n])` as a helper call (0 = the stitching C call)
    pub f2rand: bool,
    /// BUNDLE-GM: record the `gmatch` iterator step of a generic `for` as a helper call.
    pub gmatch: bool,
    /// F2: inline `string.byte(s, i)` (a bounds check and a load, the helper is the slow path)
    pub f2byte: bool,
    /// F2: record `assert(v, ...)` of a truthy first argument as a type guard (0 = the stitching C call)
    pub f2asrt: bool,
    /// F2: record `rawget` / `rawset` / `rawequal` as inline loads and stores (0 = the stitching C call)
    pub f2raw: bool,
    /// BUNDLE-A: `tonumber(string)` / `next(t [, k])` / plain `string.find` recorded (`TrLimits::libnum` / `libnext` / `libfind`).
    pub libnum: bool,
    pub libnext: bool,
    pub libfind: bool,
    /// BUNDLE-C: `string.format` of a constant format of `%d %x %X %s %c` items recorded (`TrLimits::tfmt`).
    pub tfmt: bool,
    /// BUNDLE-H (`TrLimits::libcall` bit 0): n-ary `string.char`, `math.random()`, `string.lower` / `upper` / `reverse` / `rep` recorded.
    pub libcall: bool,
    /// BUNDLE-H (`TrLimits::libcall` bit 3): `string.rep` recorded too (its result length class varies in typical loops: the kind guard then exits).
    pub libcall_rep: bool,
    /// BUNDLE-R (`TrLimits::libcall` bit 4): the result length class of a recorded `string.rep` is a guard of the trace on the computed length (an ordinary exit that gets side traces), not only the helper's refusal.
    pub libcall_repcls: bool,
    /// BUNDLE-R: the whole `libcall` mask (bit 5 `table.unpack`, bit 6 `table.move`, bit 7 failing `tonumber`, ...).
    pub libcall_mask: u32,
    /// BUNDLE-N (`TrLimits::retf`): bit 0 = a side recording that reaches the `RETURN` of its root frame executes `RETF` (the pop is native and the
    /// recording goes on in the caller).
    pub retf: u32,
    /// BUNDLE-N: root `RETURN`s a recording may turn into `RETF`.
    pub retfmax: u32,
    /// BUNDLE-V V2 (`TrLimits::upbar`): record `SETUPVAL` of a collectable value into a real upvalue cell (`USTORE` + the upvalue write barrier).
    pub upbar: u32,
    /// N3a: (prototype, pc) head query for the root frame of a side recording AFTER a `RETF` (its root is the caller `C`, not the entry
    /// prototype the side configuration's `head_state` is bound to).
    pub retf_head: Option<FrameHeadQuery>,
    /// F2: record `getmetatable(t)` of a table as inline loads (0 = the stitching C call)
    pub f2mt: bool,
    /// F2: record string comparisons (long-string `==`, `<`, `<=`) with a helper (0 = the Nyi abort)
    pub f2cmp: bool,
    /// F1: `floor` / `ceil` / float `//` as the inline `FFLOOR` (the CPU has SSE4.1).
    pub f1floor: bool,
    /// N3b: record `return f(...)` of a recorded native in an inlined frame (see `Rec::tailcall`).
    pub tailnat: bool,
    /// M1: `RETURN` with the close flag in an inlined frame (see `Rec::ret`).
    pub closeret: bool,
    /// BUNDLE-N (K2: the pipeline always sets 3): bit 0 = a `TAILCALL` of a table with a Lua `__call` is recorded; bit 1 = a `TAILCALL` into a vararg Lua function is recorded.
    pub tailx: u32,
    /// BUNDLE-N: an `__index` Lua closure is also inlined at chain depth > 1 (K2: the pipeline always sets 7; bit 1 = `rawget(t, shortstring)` is recorded; bit 2 = a metamethod frame may repeat a prototype on the frame stack up to `unroll`).
    pub idxdeep: u32,
    /// M1: generic `for` loops (`ipairs` / `pairs`) of inlined callees are unrolled.
    pub gfor_inl: bool,
    /// BUNDLE-Q: `TFORCALL` of a Lua closure is recorded (always on in the pipeline).
    pub gfor_lua: bool,
    /// M1: the inlining depth limit ends the recording with a stitching exit instead of aborting `Depth`.
    pub depth_stitch: bool,
    /// M1: every backward edge of an inlined callee (`while`, `repeat`) is unrolled, up to `loop_unroll` per recording.
    pub loop_any: bool,
    /// DYNMASK: dynamic short-string keys use the run-time mask `HREFV` (`HREF_DYN`), loads of absent keys read the static nil node (`HREF_NIL`), absent dynamic stores are `NEWREF`.
    pub dynmask: bool,
    /// M1: an array element that is nil is recorded as an `ALOAD` of type nil.
    pub nilload: bool,
    /// B2: an array read of a non-nil element has no `metatable == 0` guard.
    pub b2mt: bool,
    /// BUNDLE-S S2 (`amt`): array reads and stores of a table with a metatable are recorded (non-nil element: raw; nil element: the TR14 flag guard).
    pub amt: bool,
    /// M1: a side recording of a loop root ends with a stitching exit at a recursive call beyond the unroll limit.
    pub side_stitch: bool,
    /// M1: the least number of recorded steps for it.
    pub side_stitch_min: u32,
    /// BUNDLE-F (`stitch0`): see [`TrLimits::stitch0`].
    pub stitch0: bool,
    /// A1: lower the `NEWREF` of a short-string key into a table allocated by the same trace to stores at record-time-known nodes
    /// (see `Rec::nr_plan`).
    pub nrinline: bool,
    /// P-E2: fewest bytecodes a function-entry recording must execute (shorter ones abort `LeaveFrame`).
    pub fn_min: u32,
    /// P-B: bytes the collector lets the program allocate between two steps (`Heap::jit_gc_step_hint`) when the allocation storm guard is on
    /// (`TrLimits::allocguard`, not in a GC stress mode), else 0. A recording whose string allocations per iteration (estimated
    /// at [`STORM_BYTES`] each) times [`STORM_FACTOR`] exceed it aborts `AllocStorm`.
    pub storm_step: usize,
}

/// P-B: estimated bytes of one string a class-A helper allocates (header + a typical short body).
pub const STORM_BYTES: usize = 48;
/// P-B: a trace is not worth recording when the collector steps more often than every `STORM_FACTOR` iterations.
pub const STORM_FACTOR: usize = 4;

/// [`HeadQuery`] for any prototype: is `pc` of this prototype a loop head with an installed root trace?
pub type FrameHeadQuery = Rc<dyn Fn(&mut State, &Proto, u32) -> Option<bool>>;

/// The question a side recording asks the trace cache when it reaches a pc of the root frame: `None` = not a loop head,
/// `Some(true)` = a head whose root trace is installed (the recording ends with a link into it), `Some(false)` = a head
/// without a trace (the recording aborts, 7.8).
pub type HeadQuery = Rc<dyn Fn(&mut State, u32) -> Option<bool>>;

/// Configuration of a side recording (F2a, 7.8).
#[derive(Clone)]
pub struct SideCfg {
    /// Root-relative slots whose type the parent's exit snapshot fixes (the exit stub stores them with these tags): the
    /// side trace's `SLOAD`s of them need no typecheck. Slots of the stack the snapshot does not restore keep their
    /// checked `SLOAD`.
    pub known: Vec<(u16, Ty)>,
    /// See [`HeadQuery`].
    pub head_state: HeadQuery,
    /// Fewest bytecodes the finished side trace must execute (`TrLimits::sidemin`; 0 for a prelude).
    pub min_len: u32,
    /// S1: the inlined frames the exit left virtual (outermost first); the recording starts inside the innermost one. Empty:
    /// the exit is in the root frame.
    pub frames: Vec<super::cache::SideFrame>,
    /// S1: what the parent's entry checks guarantee (stack extent, spare `CallInfo` nodes); a side trace with frames runs
    /// without entry checks, so a recording that needs more aborts `TooLong` / `Depth`.
    pub max_extent: u16,
    /// See `max_extent`: spare `CallInfo` nodes.
    pub max_ci: u8,
    /// H1: fewest bytecodes of a side trace with frames that ends at a stitching exit (`TrLimits::framemin`).
    pub frame_min: u32,
}

/// What a closed side recording adds to [`Recorded`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SideInfo {
    /// The pc the side trace starts at (the parent's exit pc, in the root frame).
    pub start_pc: u32,
    /// `rem(start_pc)` of the tier-1 label it is entered at (0 for a pre-count head label): the instructions tier 1 had
    /// prepaid for the rest of that block. The segment's `BUDGET` charges `n - start_rem` (3.7, F2a decisions).
    pub start_rem: u32,
    /// The loop head the trace ends at (its `cell[H]` is the link target) and the snapshot of the link's `POLL`.
    pub link_pc: u32,
    /// Snapshot of the poll before the link: the empty map at `link_pc`, uncharged.
    pub link_snap: u32,
    /// The prototype `link_pc` belongs to (the root's for a link in the root frame).
    pub link_proto: ProtoKey,
    /// F2b: a linking exit: `link_snap` materialises inlined frames (it is the trace's last, always-taken exit) and the
    /// target is a head of an inlined callee's prototype.
    pub frame_link: bool,
    /// P-E2: the side trace ends with an always-taken exit before the root frame's `RETURN` / `TAILCALL` (function-entry
    /// root family) instead of a link: `link_snap` is that exit, there is no target cell.
    pub no_link: bool,
    /// S1: the prototype `start_pc` belongs to (the innermost virtual frame's; the root's when `frameful` is false).
    pub start_proto: ProtoKey,
    /// S1: the trace starts inside inlined frames that are virtual at its entry (no entry checks).
    pub frameful: bool,
}

/// The edge-registering hook of [`DepHooks`].
pub type RegisterFn = Rc<dyn Fn(&mut State, &Proto) -> bool>;

/// What the recorder needs from the trace cache (P6) to keep TR32: the driver builds it from `cache::register_dep` and
/// `cache::rec_aborted` ([`cache_hooks`]).
#[derive(Clone)]
pub struct DepHooks {
    /// Called **when the recorder inlines** a call to `callee` (not at install): make sure the prototype has a
    /// descriptor (TR4b: compile it to tier 1) and register the edge against the active recording. `false`: refused
    /// (`register_dep` returned false, the compile was refused): the recording aborts with [`AbortReason::DepFreed`].
    pub register: RegisterFn,
    /// Polled before every step: `false` once the recording was flagged because a dependency was freed (TR32).
    pub alive: Rc<dyn Fn(&mut State) -> bool>,
}

/// Resolves an inlined callee to its descriptor handle (`None`: it has none, the compile was refused).
pub type ResolveFn = Rc<dyn Fn(&mut State, &Proto) -> Option<super::cache::ProtoRef>>;

/// [`DepHooks`] over the trace cache for recording `id`: `resolve` maps an inlined callee to its descriptor handle. The
/// hooks only call `cache::register_dep` and `cache::rec_aborted`.
pub fn cache_hooks(id: super::cache::TrId, resolve: ResolveFn) -> DepHooks {
    DepHooks {
        register: Rc::new(move |st, p| match resolve(st, p) {
            Some(pr) => super::cache::register_dep(st, id, pr),
            None => false,
        }),
        alive: Rc::new(move |st| !super::cache::rec_aborted(st, id)),
    }
}

impl Default for RecCfg {
    fn default() -> RecCfg {
        RecCfg {
            hrefv: true,
            fold: true,
            cse: true,
            wait_head: false,
            restarts: 0,
            verify: false,
            lockstep: true,
            landing: None,
            landcache: false,
            deps: None,
            max_model_elems: MAX_MODEL_ELEMS,
            side: None,
            nest: None,
            nest_frame: None,
            nest_frame_sides: false,
            nest_frame_roots: false,
            retf_head: None,
            stitch_min: 0,
            unroll: 0,
            max_depth: 0,
            loop_unroll: 0,
            ncallee: false,
            uvsep: false,
            fn_root: false,
            rec_native: false,
            rec_only: false,
            rec_cont: false,
            rec_conts: false,
            rec_any: false,
            cnum: false,
            nil_store: false,
            rec_seen: false,
            max_ins: 0,
            pack: false,
            gfor: false,
            mm: false,
            setmt: false,
            f1pow: false,
            f1math: false,
            f1strlen: false,
            g1fmt: false,
            f2rand: false,
            gmatch: false,
            f2byte: false,
            f2asrt: false,
            f2raw: false,
            libnum: false,
            libnext: false,
            libfind: false,
            tfmt: false,
            libcall: false,
            libcall_rep: false,
            libcall_repcls: false,
            libcall_mask: 0,
            retf: 0,
            retfmax: 0,
            upbar: 0,
            f2mt: false,
            f2cmp: false,
            f1floor: false,
            tailnat: false,
            closeret: false,
            tailx: 0,
            idxdeep: 0,
            gfor_inl: false,
            gfor_lua: false,
            depth_stitch: false,
            loop_any: false,
            dynmask: false,
            nilload: false,
            b2mt: false,
            amt: false,
            side_stitch: false,
            side_stitch_min: 0,
            stitch0: false,
            nrinline: false,
            pcall: false,
            fn_min: 0,
            storm_step: 0,
        }
    }
}

/// A closed recording: the rolled iteration of 7.8.
#[derive(Clone, Debug)]
pub struct Recorded {
    /// The IR (`loop_at = None`, snapshots with `refund` / `land` patched, `counts = [n]`).
    pub ir: TraceIr,
    /// The writeback list: the slots the iteration wrote and their final refs (the `S_H` map).
    pub writeback: Vec<SnapEntry>,
    /// The recorded path: the pc of every fetched instruction of the iteration (TR5.1).
    pub path: Vec<u32>,
    /// Per step of `path`, the instructions tier 1 counts for it ([`snap::step_cost`]: 1, plus the unfetched
    /// `TFORCALL` / `TFORLOOP` chains, which the prototype never records).
    pub cost: Vec<u32>,
    /// Per snapshot, the index into `path` of the instruction it belongs to (snapshot 0: 0).
    pub snap_step: Vec<u32>,
    /// The prototype of every step of `path` (a pc alone is ambiguous across inlined frames).
    pub path_proto: Vec<ProtoKey>,
    /// The inline depth of every step of `path` (0 = the root frame).
    pub path_depth: Vec<u8>,
    /// TR6c: the stack room the entry check needs, root-relative: the maximum over every call site of `required_top +
    /// required_room` (Lua call: `top + maxstacksize(callee)`; intrinsic: `top + LUA_MINSTACK`), checked strictly as
    /// `stack_size - (ci.func + 1 + room) > 0`; `0` when the path has no call. `ir.extent` is at least this.
    pub room: u32,
    /// TR6d: the snapshot that entry-valid guards (callee identity) use instead of their instruction's own: the empty
    /// map at `H`, charged (`refund = n`, it is the slice's `S0R`); `None` when no guard used it. A guard with this
    /// snapshot may be placed in the entry segment (and re-checked by links); every other guard stays in place.
    pub entry_snap: Option<u32>,
    /// Instructions one iteration counts (`counts[0]`, the sum of `cost`).
    pub n: u32,
    /// In-place re-records (`Unstable`) before this recording succeeded.
    pub rerec: u32,
    /// F2a: `Some` for a side trace (rolled iteration without a back-edge: it ends with a link to a loop head).
    pub side: Option<SideInfo>,
    /// F2b: the recording ends with an always-taken exit at a `CALL` of a C function (stitching): no loop, no back-edge.
    pub stitched: bool,
}

/// The result of a recording.
#[derive(Clone, Debug)]
pub enum RecOutcome {
    /// The loop closed.
    Closed(Box<Recorded>),
    /// The recording stopped.
    Aborted(AbortReason),
}

thread_local! {
    static REC: RefCell<Option<Box<Rec>>> = const { RefCell::new(None) };
    static OUT: RefCell<Option<(u64, RecOutcome)>> = const { RefCell::new(None) };
}

/// The recorder's entry points for `jit_hooks::install_rec`.
pub const VTABLE: RecVTable = RecVTable { step, abort };

/// Registers [`VTABLE`] with the VM (first registration wins; returns whether this call installed it).
pub fn install() -> bool {
    jit_hooks::install_rec(VTABLE)
}

/// The outcome of the last finished recording of this OS thread, with the token it belonged to.
pub fn take_outcome() -> Option<(u64, RecOutcome)> {
    OUT.with(|o| o.borrow_mut().take())
}

/// Is a recording held by this OS thread?
pub fn is_recording() -> bool {
    REC.with(|r| r.borrow().is_some())
}

/// Drops the recorder state and the pending outcome of this OS thread (tests).
pub fn discard() {
    REC.with(|r| *r.borrow_mut() = None);
    OUT.with(|o| *o.borrow_mut() = None);
}

/// The tier-1 block structure of `p`, lowered with the production configuration (calls on), for the count model.
pub fn lower_for_landing(p: &Proto) -> Option<Lowered> {
    lower(
        p.code(),
        LowerCfg {
            code_addr: p.code_ptr() as usize,
            interrupt_addr: jit_hooks::interrupt_flag_addr(),
            calls: Some(CallEnv {
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
            consts: p.k(),
            t15_warmup: None,
        },
    )
}

/// The head of the first loop that starts at or after `pc`: the target of the first back-edge of `low` whose head is
/// `>= pc` (a test helper; the production arm site knows its head).
pub fn first_head_after(low: &Lowered, pc: u32) -> Option<u32> {
    low.backedges
        .iter()
        .filter(|&&(_, h)| h >= pc)
        .map(|&(_, h)| h)
        .min()
}

thread_local! {
    /// N3a: the recording armed last executed a `RETF` (read by the driver when the recording fails: the exit is retried without `RETF`).
    static RETF_USED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// N3a: did the last (side) recording execute a `RETF`? (and clears the flag)
pub fn take_retf_used() -> bool {
    RETF_USED.with(|c| c.replace(false))
}

/// Arms a recording rooted at the Lua frame `ci` for the loop whose head is `head` (a pc of the frame's proto). Returns
/// the token (also given to the callbacks), or `None` when nothing was armed (not a Lua frame, hook installed,
/// recording already armed, the proto does not lower).
pub fn arm(st: &mut State, ci: Ci, head: u32, cfg: RecCfg) -> Option<u64> {
    RETF_USED.with(|c| c.set(false));
    let cl = st.ci_lclosure(ci)?;
    let p = cl.proto();
    let low = match &cfg.landing {
        Some(l) => l.clone(),
        None if cfg.landcache => super::cache::landing_cached(&p)?,
        None => Rc::new(lower_for_landing(&p)?),
    };
    let frameful = cfg.side.as_ref().is_some_and(|sd| !sd.frames.is_empty());
    let token = if frameful {
        jit_hooks::rec_arm_chain(st, ci, st.ci())?
    } else {
        jit_hooks::rec_arm(st, ci)?
    };
    st.heap().unwatch_all();
    let mut rec = Rec::new(token, st, ci, &p, cl.as_ptr() as usize, head, low, cfg);
    if frameful && rec.seed_frames(st).is_none() {
        jit_hooks::rec_disarm(st);
        return None;
    }
    REC.with(|r| *r.borrow_mut() = Some(Box::new(rec)));
    OUT.with(|o| *o.borrow_mut() = None);
    Some(token)
}

fn finish_with(token: u64, o: RecOutcome) {
    OUT.with(|c| *c.borrow_mut() = Some((token, o)));
}

/// `RecVTable::step`: before each instruction of the recorded frame.
pub fn step(st: &mut State, ci: Ci, pc: *const u32, token: u64) -> RecStep {
    let Some(mut rec) = REC.with(|r| r.borrow_mut().take()) else {
        return RecStep::Leave; // nothing armed here: a foreign or stale callback is a no-op
    };
    if rec.token != token {
        REC.with(|r| *r.borrow_mut() = Some(rec)); // not our recording: leave it alone
        return RecStep::Leave;
    }
    let r = rec.on_step(st, ci, pc);
    match r {
        Ok(Flow::Keep) => {
            rec.watch_new(st);
            REC.with(|r| *r.borrow_mut() = Some(rec));
            RecStep::Keep
        }
        Ok(Flow::Closed(done)) => {
            st.heap().unwatch_all();
            finish_with(token, RecOutcome::Closed(done));
            RecStep::Leave
        }
        Err(why) => {
            st.heap().unwatch_all();
            if super::cache::verbose_aborts() {
                eprintln!(
                    "[trace] abort {:?} at pc {} (depth {}, op {})",
                    why, rec.cur_pc, rec.cur_depth, rec.cur_op
                );
            }
            finish_with(token, RecOutcome::Aborted(why));
            RecStep::Leave
        }
    }
}

/// `RecVTable::abort`: the VM ended the recording.
pub fn abort(st: &mut State, why: Abort, token: u64) {
    st.heap().unwatch_all();
    let Some(rec) = REC.with(|r| r.borrow_mut().take()) else {
        return;
    };
    if rec.token != token {
        REC.with(|r| *r.borrow_mut() = Some(rec));
        return;
    }
    let why = match why {
        Abort::Error => AbortReason::Error,
        Abort::Frame => AbortReason::Frame,
        Abort::Hook => AbortReason::Hook,
        Abort::Close => AbortReason::Close,
    };
    finish_with(token, RecOutcome::Aborted(why));
}

// ---- value mapping --------------------------------------------------------------------------------------------

/// The IR type of a VM value (`None`: a tag the prototype recorder does not model).
pub fn ty_of_value(v: &Value) -> Option<Ty> {
    let t = v.tag();
    Some(match t {
        LUA_VNIL => Ty::Nil,
        LUA_VFALSE => Ty::False,
        LUA_VTRUE => Ty::True,
        LUA_VNUMINT => Ty::Int,
        LUA_VNUMFLT => Ty::Num,
        LUA_VLIGHTUSERDATA => Ty::LUdata,
        LUA_VLCF => Ty::Lfn,
        _ if t == ctb(LUA_VSHRSTR) => Ty::ShrStr,
        _ if t == ctb(LUA_VLNGSTR) => Ty::LngStr,
        _ if t == ctb(LUA_VTABLE) => Ty::Tab,
        _ if t == ctb(LUA_VLCL) => Ty::Lcf,
        _ if t == ctb(LUA_VCCL) => Ty::Ccl,
        _ if t == ctb(LUA_VUSERDATA) => Ty::Udata,
        _ if t == ctb(LUA_VTHREAD) => Ty::Thread,
        _ => return None,
    })
}

/// May a value of this type be collectable (needs `TBAR` when stored into a table)?
fn collectable(t: Ty) -> bool {
    matches!(
        t,
        Ty::ShrStr | Ty::LngStr | Ty::Tab | Ty::Lcf | Ty::Ccl | Ty::Udata | Ty::Thread
    )
}

fn falsy(t: Ty) -> bool {
    matches!(t, Ty::Nil | Ty::False)
}

/// Payload bits of a non-table value as the IR interpreter models them.
fn payload_bits(v: &Value, t: Ty) -> u64 {
    match t {
        Ty::Int => v.as_int().unwrap_or(0) as u64,
        Ty::Num => v.as_float().unwrap_or(0.0).to_bits(),
        Ty::Nil | Ty::False | Ty::True => 0,
        Ty::LUdata => v.as_light_userdata().map_or(0, |p| p as usize as u64),
        Ty::Lfn => v.as_native().map_or(0, |f| f as usize as u64),
        _ => v.as_gc().map_or(0, |p| p as usize as u64),
    }
}

// ---- the recorder ---------------------------------------------------------------------------------------------

enum Flow {
    Keep,
    Closed(Box<Recorded>),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Phase {
    WaitHead,
    Rec,
}

/// What the previous instruction left to verify.
#[derive(Clone, Copy)]
struct Pend {
    /// The expected pc index of the next step.
    next: usize,
    /// The inline depth of the frame the next step belongs to (a `CALL` expects the callee, a `RETURN` the caller).
    depth: usize,
}

/// Read-only view of the frame being recorded (the innermost one).
struct Vm<'a> {
    st: &'a mut State,
    /// Absolute stack index of register 0 of this frame.
    base: usize,
    /// Absolute stack index of register 0 of the root frame (root-relative slots are offsets from it).
    root_base: usize,
    k: &'a [Value],
    code: &'a [u32],
    cl: rn_core::value::Gc<LClosure>,
}

impl Vm<'_> {
    /// Register `n` of this frame.
    fn slot(&self, n: usize) -> Value {
        self.st.get(StkId(self.base + n))
    }

    /// Root-relative slot `n` (any frame).
    fn root_slot(&self, n: usize) -> Value {
        self.st.get(StkId(self.root_base + n))
    }
}

/// S1: can a side recording start inside `frames` (the frames the exit restored, outermost first; the running frame is the
/// innermost)? Every frame must be a non-vararg Lua frame whose `CIST_MMRET` mark agrees with the snapshot, and a metamethod
/// frame's caller must be an instruction the recorder inlines a metamethod call for (the policy asks before it arms).
pub fn side_frames_supported(st: &mut State, frames: &[super::cache::SideFrame]) -> bool {
    let mut c = st.ci();
    for f in frames.iter().rev() {
        let Some(cl) = st.ci_lclosure(c) else {
            return false;
        };
        if cl.proto().is_vararg() || (c.r().callstatus & CIST_MMRET != 0) != f.mm {
            return false;
        }
        let Some(caller) = c.previous() else {
            return false;
        };
        if f.mm {
            let Some(ccl) = st.ci_lclosure(caller) else {
                return false;
            };
            if Rec::mm_fin_at(ccl.proto().code(), f.call_pc).is_none() {
                return false;
            }
        }
        c = caller;
    }
    true
}

/// One frame of the recorder's frame stack (4.2): index 0 is the root.
#[derive(Clone)]
struct RecFrame {
    /// The frame's `CallInfo` (address); 0 until its first step (a callee is bound at its first step).
    ci: usize,
    /// Absolute stack index of the function slot.
    func_abs: usize,
    /// The prototype (address).
    proto: usize,
    /// The closure object (address).
    closure: usize,
    /// Root-relative slot of register 0.
    base_off: usize,
    /// `maxstacksize`.
    nslots: usize,
    /// The caller's `CALL` pc (root: 0).
    call_pc: u32,
    /// Results the call wants (root: 0).
    nresults: i16,
    /// Index into `Rec::lows` of this proto's `Lowered`.
    low: u32,
    /// The ref of the closure in the function slot (callee frames; the root's is the `FUNC_SLOT` load).
    cref: Ref,
    /// F4: the frame was entered by a `TAILCALL` (`CIST_TAIL`).
    tail: bool,
    /// P-E: a vararg callee whose `VARARGPREP` has not been recorded yet: the total argument count the frame will have
    /// (`max(nargs, numparams)`, what `luaT_adjustvarargs` reads as `totalargs`).
    vpending: Option<usize>,
    /// P-E: a vararg frame after its `VARARGPREP`: `totalargs + 1`, how far `func`, the parameters and the registers moved
    /// up (`base_off`/`func_abs` are the shifted ones); 0 for every other frame.
    vdelta: usize,
    /// P-E: a vararg frame after its `VARARGPREP`: the number of extra arguments (below the shifted function slot).
    nextra: usize,
    /// P-O: a protected call (`pcall` / `xpcall` of a Lua closure) is inlined here: how far above the native's function slot the
    /// callee's function slot lies (2 for `pcall`, 4 for `xpcall`); 0 for every other frame. The native's `CallInfo` sits
    /// between this frame's `CallInfo` and its caller's, and the frame's results go through `finishpcall` (`true` first).
    pc_shift: usize,
    /// P-O: the results the inlined `pcall` call site wants (`C - 1`).
    pc_wanted: i32,
    /// N3: the frame is a metamethod run in place (`CIST_MMRET`): how its result reaches the caller's instruction.
    mm: Option<MmFin>,
    /// N3b: the state before the instruction that called this metamethod (see [`EntryCtx`]).
    ectx: Option<std::rc::Rc<EntryCtx>>,
    /// Generic `for` over a Lua iterator: the frame was called by a `TFORCALL`; its return also runs the `TFORLOOP` behind it.
    tfor: Option<TforRet>,
}

/// The `TFORLOOP` the interpreter runs unfetched after the iterator returned (see [`RecFrame::tfor`]).
#[derive(Clone, Copy, Debug)]
struct TforRet {
    /// Register `A` of the loop.
    a: usize,
    /// `Bx` of the `TFORLOOP` (the jump back).
    bx: u32,
}

/// N3: how the result of a metamethod frame reaches the caller (what `mm_return` does for the instruction that called it).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MmFin {
    /// The first result (nil when there is none) is stored in register `r` of the caller (arithmetic, unary, `#`, `..`).
    Reg(u16),
    /// A comparison (`__eq`, `__lt`, `__le`): the result's truth value is the instruction's condition, `k` the instruction's flag (when the
    /// condition differs from it the caller skips the `JMP` behind the comparison, else the `JMP` is dispatched).
    Cond(bool),
    /// The results are discarded (`__newindex`).
    Drop,
}

/// N3: a metamethod call prepared by the instruction that triggers it (guards emitted), pushed by [`Rec::mm_push`].
#[derive(Clone)]
struct MmCall {
    cl: rn_core::value::Gc<LClosure>,
    cref: Ref,
    args: [Ref; 3],
    nargs: usize,
    fin: MmFin,
}

/// N3: an operand of a metamethod-triggering instruction.
#[derive(Clone, Copy)]
struct MmOpd {
    r: Ref,
    t: Ty,
    v: Value,
}

/// P-O: the state before an inlined protected call (taken at its `CALL`): every guard of the call's body uses a snapshot built from
/// it, so an exit re-executes the real `pcall` from the start (the body has no side effect before any such guard, which the
/// recorder checks: [`Rec::guard_with`]).
#[derive(Clone)]
struct PcallCtx {
    sm: SlotMap,
    frames: Vec<snap::FrameRec>,
    top: u16,
    pc: u32,
    step: u32,
    low: u32,
    /// The snapshot built from this context (on the first guard of the body).
    snap: Option<u32>,
    /// First IR instruction of the body not yet checked for side effects.
    scan: usize,
    /// A side-effecting instruction was emitted: no guard may follow before the body returns.
    impure: bool,
}

/// N3b: the state before the instruction that called a metamethod (taken at the triggering step): an allocation guard in the metamethod's
/// body whose body has changed nothing yet exits through a snapshot built from it, so the interpreter re-executes the triggering
/// instruction in the caller's frame instead of finishing the callee (a frame-restoring exit leaves compiled code for the interpreter
/// until the next loop tick; a frame-less one continues in tier 1).
struct EntryCtx {
    sm: SlotMap,
    frames: Vec<snap::FrameRec>,
    top: u16,
    pc: u32,
    step: u32,
    low: u32,
    /// First IR instruction of the callee's body.
    ins_at: usize,
}

/// BUNDLE-C: reusable buffers of the lockstep evaluation of one IR instruction.
#[derive(Default)]
struct Scratch {
    mini: TraceIr,
    extra: Vec<Val>,
    vbuf: Vec<u64>,
}

struct Rec {
    token: u64,
    cfg: RecCfg,
    thread: usize,
    root_ci: usize,
    root_func: usize,
    proto: usize,
    head: u32,
    nslots: usize,
    /// The `Lowered` of every proto of the frame stack seen so far (index 0: the root).
    lows: Vec<Rc<Lowered>>,
    low_of: HashMap<usize, u32>,
    phase: Phase,
    // the frame stack
    frames: Vec<RecFrame>,
    cur_depth: usize,
    next_depth: usize,
    cur_len: usize,
    /// Slots of extent so far (largest `base_off + maxstacksize` or written slot + 1).
    ext: usize,
    /// TR6c room (see [`Recorded::room`]).
    room: u32,
    /// TR6b: most `CallInfo` nodes needed after the root's.
    max_ci: u8,
    /// Inlined protos (not the root), in inline order.
    deps: Vec<ProtoKey>,
    /// Protos inlined by the current instruction whose edge is not registered yet.
    want_dep: Vec<rn_core::value::Gc<Proto>>,
    /// Mutant `dep_edge_at_install`: every inlined proto, registered at close instead.
    late_dep: Vec<rn_core::value::Gc<Proto>>,
    /// P-E: backward edges of inlined callees' loops taken so far (see [`RecCfg::loop_unroll`]).
    inl_back: std::cell::Cell<u32>,
    /// `loop_any` sites (callee prototype, loop head pc) already charged to the budget by this recording.
    la_seen: std::cell::RefCell<Vec<(usize, usize)>>,
    /// Index of the first `ASTORE` / `USTORE` (TR6d: no entry guard after it).
    first_store: Option<usize>,
    entry_snap: Option<u32>,
    // the trace under construction
    b: Builder,
    sm: SlotMap,
    path: Vec<u32>,
    path_proto: Vec<ProtoKey>,
    path_depth: Vec<u8>,
    cost: Vec<u32>,
    meta: Vec<SnapMeta>,
    pend: Option<Pend>,
    budget_at: Ref,
    func_ref: Ref,
    kstr: HashMap<(usize, usize), Ref>,
    /// Real `UpVal` address -> model cell + 1 (`Model::uv_cell`).
    uv_cells: HashMap<usize, u32>,
    /// P2 bundle 2 (`uvsep`): identity (1, 2, ...) of each real `UpVal` address a `UREF` of this recording names.
    uv_ids: HashMap<usize, u32>,
    cur_pc: u32,
    cur_op: u8,
    cur_snap: Option<u32>,
    /// P-O: the inlined protected call whose body is being recorded (see [`PcallCtx`]).
    pcall: Option<PcallCtx>,
    // the lockstep model
    model: Model,
    reg: HashMap<usize, u64>,
    tabs: Vec<TableRef>,
    /// F8: how many of `tabs` are watched by the collector (`Heap::watch_object`).
    pinned: usize,
    /// Per table handle: the model array has its true size (allocated by `touch`).
    filled: Vec<bool>,
    /// Array elements (`h << 32 | index`) already copied from the VM (first read or store by the IR).
    efilled: HashSet<u64>,
    /// Array elements stored to by the IR (`(h, index)`), compared with the VM at every step.
    stored: Vec<(u64, usize)>,
    /// F1: hash nodes (`h << 32 | index`) already copied from the VM into the model (at record time: see
    /// [`Rec::fill_node`]), and the ones the IR stored to (`(h, index)`), compared with the VM at every step.
    nfilled: HashSet<u64>,
    stored_nodes: Vec<(u64, u32)>,
    /// The IR interpreter's value of every IR instruction evaluated so far (`vals[ref - REF_BIAS]`) and whether it
    /// derives from a `CALLN` (its NaN payload is not comparable, see [`Rec::same`]); `evald` instructions are done.
    vals: Vec<u64>,
    taint: Vec<bool>,
    evald: usize,
    /// Model cells (`kind` 0 array / 1 upvalue, handle, index) holding a value derived from a `CALLN`.
    cell_taint: HashSet<(u8, u64, usize)>,
    /// Address of the root base slot (the origin of `UVREL`).
    root_ptr: usize,
    cl_reg: HashMap<usize, u64>,
    cls: Vec<rn_core::value::Gc<LClosure>>,
    cl_filled: Vec<bool>,
    elems: usize,
    ups_dirty: bool,
    rerec: u32,
    restarts_left: u32,
    /// F2a side recording: types the parent's snapshot fixes, per root-relative slot (`None`: check at the first read).
    known: Vec<Option<Ty>>,
    /// F2a: `rem(head)` of the label the side trace is entered at (see [`SideInfo::start_rem`]).
    start_rem: u32,
    /// S1: the inline depth the recording starts at and the prototype of that frame.
    start_depth: usize,
    start_proto: ProtoKey,
    /// F2a: the loop head the side recording ended at.
    link: Option<u32>,
    /// F2b: the linking exit (snapshot and prototype of the callee's head) of a root that ends inside an inlined frame.
    frame_link: Option<(u32, ProtoKey)>,
    /// P-E2: the snapshot of the exit that ended the recording (`end_exit`).
    end_snap: Option<u32>,
    /// F2b: the instruction being recorded ended the recording with a stitching exit.
    stitched: bool,
    /// P-M3: the native self-call that ended the recording (`end_rec_call`).
    rec_call: Option<RecCall>,
    /// P-M3: the native `RETURN1` that ended the recording (`end_rec_ret`) and the returned value.
    rec_ret: Option<(RecRet, Ref)>,
    /// F3: absolute stack index of the register the last recorded class-A instruction (`NEWTABLE`, `LEN`) writes; the
    /// lockstep reads its value from the VM (`post_val`) before it evaluates the instruction's IR, because the result
    /// of an allocation (the table's address) and of a border (a hidden-state function) exists only in the VM.
    post_slot: Option<usize>,
    /// F3: root-relative slots whose value the lockstep does not compare until the recorded code defines them again.
    /// The interpreter's GC step inside an allocating instruction (`NEWTABLE` sets `top = ra + 1` first) clears the dead
    /// stack slice above `top`; the IR keeps the stale values (a trace runs no GC step: when one would be due it exits
    /// before the instruction, and an exit writes the slots exactly as the interpreter without a step would have them).
    stale: HashSet<usize>,
    /// BUNDLE-C: per root-relative slot, the `(ref, tag word, payload bits)` the lockstep last found equal to the model: the same ref (its
    /// model value never changes) against the same raw VM value compares equal again, so the slot is skipped.
    ls_memo: Vec<(Ref, u64, u64)>,
    /// F3: the value `post_slot` holds after the VM ran the instruction.
    post_val: Option<Value>,
    /// BUNDLE-R: the lengths the lockstep hands to the `TLEN` ops of strings a recorded library call emitted itself (in emission order; they are not the result of the VM instruction).
    pend_lens: std::collections::VecDeque<i64>,
    /// F3b: the `(key, value)` of the `TNEXT` just recorded as the model will see them (the lockstep hands them to
    /// `Model::next_result` when it evaluates the instruction).
    pending_next: Option<(Val, Val)>,
    /// F3b: the stack slots (absolute) whose VM values the model's `SNEW` / `CONCAT` results are (read after the
    /// instruction ran, in emission order; `usize::MAX`: unknown, the result is not used by anything), and the values the
    /// lockstep read from them.
    pend_str: Vec<usize>,
    /// BUNDLE-C: scratch of `eval_one` (the one-instruction IR, its operand values, the interpreter's value buffer).
    scratch: Scratch,
    /// N3b: the native being recorded is the target of a `TAILCALL` (its results land in the returning frame's function slot).
    tail_lib: bool,
    /// P-B: string-allocating helpers recorded so far (the allocation storm guard).
    str_allocs: usize,
    /// F3b: the integer the model's `STRTO` yields (converted by the recorder from the VM's string).
    pending_strto: Option<Val>,
    post_strs: std::collections::VecDeque<u64>,
    /// F3: the string keys of the `NEWREF`s emitted and not yet evaluated, in order (the model learns the node index the
    /// VM chose from the table after the instruction ran).
    newref_keys: std::collections::VecDeque<StrRef>,
    /// A1: for each `NEWREF` in `newref_keys` order: the trace table (`TNEW` ref) and the hash image the inline insertion is
    /// expected to leave (checked against the VM's table when the model syncs), or `None` for a helper `NEWREF`.
    newref_exp: std::collections::VecDeque<Option<(Ref, NrImage)>>,
    /// A1: per `TNEW` ref, the hash image of its table after the last inline `NEWREF` (`None`: poisoned, a helper `NEWREF` or
    /// anything else changed it).
    nr_img: HashMap<Ref, Option<NrImage>>,
    /// BUNDLE-M C1 (lockstep off): for each `NEWREF` / `TSET` in IR order, the table the recorder knew at record time (`None`: a table
    /// of this trace made by a `TNEW` of the same step, see [`Rec::light_new`]).
    ls_tabs: std::collections::VecDeque<Option<TableRef>>,
    /// BUNDLE-M C1 (lockstep off): model handle of every `TNEW` ref adopted so far.
    ls_tnew: HashMap<Ref, u64>,
    /// A1: IR position of the latest `TSET` (its position in the hash part is data dependent): tables allocated before it are
    /// never given inline `NEWREF`s.
    last_tset: usize,
    /// F4 (TR22b relaxed): the root-relative slot the interpreter's `thread.top` holds after the instruction just recorded
    /// (a producer of a multiple-result sequence: `CALL C=0`, a `RETURN` into one, an intrinsic with `C=0`); the lockstep
    /// compares it with the VM and the next step moves it to `top_in`.
    top_rel: Option<usize>,
    /// F4: the `top` the instruction being recorded reads (`CALL B=0`, `RETURN B=0`, `TAILCALL B=0`, `SETLIST B=0`); every
    /// snapshot of that instruction carries it.
    top_in: Option<usize>,
    /// N3: an arithmetic instruction whose `MMBIN*` follower will call a metamethod: the call (guards already emitted at the
    /// arithmetic instruction) the `MMBIN*` step pushes. The follower has no tier-1 label and is counted as one fetched instruction.
    mm_bridge: Option<MmCall>,
    /// N3b: the state at the start of the last step that is not an `MMBIN*` follower (see [`EntryCtx`]).
    st_sm: SlotMap,
    /// N3b: the deepest `CCALLS` check already on the trace (see `Rec::mm_ccalls`).
    ccalls_done: u32,
    st_pc: u32,
    st_step: u32,
    st_top: u16,
    st_low: u32,
    /// N3: the next step is the `JMP` behind a comparison whose metamethod returned (the interpreter dispatches it; in the plain path
    /// it never does, tier 1 has no label there).
    mm_jmp: bool,
    /// N3: an `__index` / `__newindex` function call the lookup / store helpers prepared (their result is `NOREF` / `()`): the
    /// instruction pushes it.
    mm_pending: Option<MmCall>,
    /// N2c (RETF): the segments of the recording (`segs[0]` = the entry root, pushed at the first `RETF`); empty when no `RETF` was recorded.
    segs: Vec<Seg>,
    /// N2c: slots between the current root frame and the entry root (the sum of the `A + 1` of the `RETF`s).
    shift: usize,
    /// N2c: the path index of the first step of each segment after a `RETF` (snapshots of those steps belong to segment `k`).
    seg_steps: Vec<u32>,
    /// N2c: the values of the caller's registers `0..A+1` for each `RETF` not yet evaluated by the lockstep (the model is rebased when the
    /// lockstep evaluates the instruction, see `eval_one`).
    retf_below: std::collections::VecDeque<Vec<Val>>,
    /// N3a: the model handles of the closures of the segments after each `RETF` not yet evaluated, and the handle `Sload FUNC_SLOT` evaluates to now.
    retf_funcs: std::collections::VecDeque<u64>,
    model_func: u64,
    /// N3a (R4): an upvalue store below the root frame was recorded since the last `RETF`.
    uv_below_stores: Vec<usize>,
    /// N2c: the `CallInfo` of the caller the VM is about to return into (the step re-roots the VM-side recording after the instruction).
    reroot: Option<usize>,
    /// N2c: the prototype of the entry root (the current root's, `proto`, changes at a `RETF`).
    entry_proto: usize,
    /// N2c: entry-relative extent, `room` and spare-`CallInfo` count of the segments before the current one.
    ext_entry: usize,
    room_entry: u32,
    ci_entry: u8,
    /// BUNDLE-N: the argument refs of a `TAILCALL` of a table (`__call`: the object first), consumed by the tail path of [`Rec::call_lua`] (the shifted slots may lie beyond the frame).
    tail_args: Option<Vec<Ref>>,
}

type R<T> = Result<T, AbortReason>;

/// `n` model values, all `nil`, from a zero-initialised allocation (untouched pages stay unmapped, so a huge table
/// costs address space, not memory).
fn zeroed_vals(n: usize) -> Vec<Val> {
    const _: () = assert!(Ty::Nil as u8 == 0);
    if n == 0 {
        return Vec::new();
    }
    let layout = std::alloc::Layout::array::<Val>(n).expect("model array layout");
    // SAFETY: `Val` is `{Ty, u64}` and the all-zero pattern is valid: `Ty::Nil` is discriminant 0 (asserted above)
    // and `Val::NIL` has `bits == 0`. The allocation comes from the global allocator with `Val`'s array layout and
    // is handed over with `len == capacity == n`, as `Vec` requires.
    unsafe {
        let p = std::alloc::alloc_zeroed(layout).cast::<Val>();
        if p.is_null() {
            std::alloc::handle_alloc_error(layout);
        }
        Vec::from_raw_parts(p, n, n)
    }
}

impl Rec {
    #[allow(clippy::too_many_arguments)]
    fn new(
        token: u64,
        st: &mut State,
        ci: Ci,
        p: &Proto,
        clp: usize,
        head: u32,
        low: Rc<Lowered>,
        cfg: RecCfg,
    ) -> Rec {
        let nslots = usize::from(p.maxstacksize);
        let restarts_left = cfg.restarts;
        let proto = p as *const Proto as usize;
        let root_func = ci.r().func.0;
        let known = cfg.side.as_ref().map_or_else(Vec::new, |sd| {
            let n = sd
                .known
                .iter()
                .map(|k| usize::from(k.0) + 1)
                .max()
                .unwrap_or(0);
            let mut v = vec![None; n];
            for &(slot, t) in &sd.known {
                v[usize::from(slot)] = Some(t);
            }
            v
        });
        Rec {
            token,
            thread: jit_hooks::thread_ptr(st) as usize,
            root_ci: ci.as_ptr() as usize,
            root_func,
            proto,
            head,
            nslots,
            lows: vec![low],
            low_of: [(proto, 0)].into_iter().collect(),
            phase: if cfg.wait_head {
                Phase::WaitHead
            } else {
                Phase::Rec
            },
            frames: vec![RecFrame {
                ci: ci.as_ptr() as usize,
                func_abs: root_func,
                proto,
                closure: clp,
                base_off: 0,
                nslots,
                call_pc: 0,
                nresults: 0,
                low: 0,
                cref: NOREF,
                tail: false,
                vpending: None,
                vdelta: 0,
                nextra: 0,
                pc_shift: 0,
                pc_wanted: 0,
                mm: None,
                ectx: None,
                tfor: None,
            }],
            cur_depth: 0,
            next_depth: 0,
            cur_len: 0,
            ext: nslots,
            room: 0,
            max_ci: 0,
            deps: Vec::new(),
            want_dep: Vec::new(),
            late_dep: Vec::new(),
            inl_back: std::cell::Cell::new(0),
            la_seen: Default::default(),
            first_store: None,
            entry_snap: None,
            b: Builder::new(cfg.fold, cfg.cse),
            sm: SlotMap::new(nslots),
            path: Vec::new(),
            path_proto: Vec::new(),
            path_depth: Vec::new(),
            cost: Vec::new(),
            meta: Vec::new(),
            pend: None,
            budget_at: NOREF,
            func_ref: NOREF,
            kstr: HashMap::default(),
            uv_cells: HashMap::default(),
            uv_ids: HashMap::default(),
            cur_pc: 0,
            cur_op: 0,
            cur_snap: None,
            pcall: None,
            model: Model::new(),
            reg: HashMap::default(),
            tabs: Vec::new(),
            pinned: 0,
            filled: Vec::new(),
            efilled: HashSet::default(),
            stored: Vec::new(),
            nfilled: HashSet::default(),
            stored_nodes: Vec::new(),
            vals: Vec::new(),
            taint: Vec::new(),
            evald: 0,
            cell_taint: HashSet::default(),
            root_ptr: 0,
            cl_reg: HashMap::default(),
            cls: Vec::new(),
            cl_filled: Vec::new(),
            elems: 0,
            ups_dirty: false,
            rerec: 0,
            restarts_left,
            known,
            start_rem: 0,
            start_depth: 0,
            start_proto: ProtoKey(proto),
            link: None,
            frame_link: None,
            end_snap: None,
            stitched: false,
            rec_call: None,
            rec_ret: None,
            post_slot: None,
            stale: HashSet::default(),
            ls_memo: Vec::new(),
            post_val: None,
            pend_lens: std::collections::VecDeque::new(),
            pending_next: None,
            pend_str: Vec::new(),
            scratch: Scratch::default(),
            tail_lib: false,
            str_allocs: 0,
            pending_strto: None,
            post_strs: std::collections::VecDeque::new(),
            newref_keys: Default::default(),
            newref_exp: Default::default(),
            nr_img: HashMap::default(),
            ls_tabs: Default::default(),
            ls_tnew: HashMap::default(),
            last_tset: 0,
            top_rel: None,
            top_in: None,
            mm_bridge: None,
            st_sm: SlotMap::default(),
            ccalls_done: 0,
            st_pc: 0,
            st_step: 0,
            st_top: 0,
            st_low: 0,
            mm_jmp: false,
            mm_pending: None,
            tail_args: None,
            segs: Vec::new(),
            retf_below: Default::default(),
            retf_funcs: Default::default(),
            model_func: 1,
            uv_below_stores: Vec::new(),
            shift: 0,
            seg_steps: Vec::new(),
            reroot: None,
            entry_proto: proto,
            ext_entry: 0,
            room_entry: 0,
            ci_entry: 0,
            cfg,
        }
    }

    /// S1: the completion (`MmFin`) of the metamethod call that the instruction at `call_pc` of `code` made: what `ret_from`
    /// does when the metamethod frame returns (`None`: an instruction the recorder never inlines a metamethod call for).
    fn mm_fin_at(code: &[u32], call_pc: u32) -> Option<MmFin> {
        let i = Instruction(*code.get(call_pc as usize)?);
        Some(match i.opcode()? {
            OpCode::Mmbin | OpCode::Mmbini | OpCode::Mmbink => {
                let prev = Instruction(*code.get((call_pc as usize).checked_sub(1)?)?);
                MmFin::Reg(prev.a() as u16)
            }
            OpCode::Unm | OpCode::Bnot | OpCode::Len | OpCode::Concat => MmFin::Reg(i.a() as u16),
            OpCode::Eq
            | OpCode::Lt
            | OpCode::Le
            | OpCode::Lti
            | OpCode::Lei
            | OpCode::Gti
            | OpCode::Gei => MmFin::Cond(i.k() != 0),
            OpCode::Geti
            | OpCode::Gettable
            | OpCode::Getfield
            | OpCode::Gettabup
            | OpCode::SelfOp => MmFin::Reg(i.a() as u16),
            OpCode::Seti | OpCode::Settable | OpCode::Setfield | OpCode::Settabup => MmFin::Drop,
            _ => return None,
        })
    }

    /// S1: a side recording that starts inside inlined frames: the frame stack is rebuilt from the exit snapshot and the live
    /// `CallInfo` chain (the exit's restore built it; the recorded frames stay virtual in the trace). Only plain Lua frames
    /// (the policy checked `ExitSnap::frame_side_ok`); every frame is verified against the live chain.
    fn seed_frames(&mut self, st: &mut State) -> Option<()> {
        let fr = self.cfg.side.as_ref()?.frames.clone();
        let mut cis = Vec::with_capacity(fr.len());
        let mut c = st.ci();
        for _ in 0..fr.len() {
            cis.push(c);
            c = c.previous()?;
        }
        if c.as_ptr() as usize != self.root_ci {
            return None;
        }
        cis.reverse();
        for (f, ci) in fr.iter().zip(cis) {
            let cl = st.ci_lclosure(ci)?;
            let p = cl.proto();
            let key = &*p as *const Proto as usize;
            let func_abs = self.root_func + 1 + usize::from(f.func_slot);
            if ci.r().func.0 != func_abs || p.is_vararg() {
                return None;
            }
            // a metamethod frame: how its result reaches the caller's instruction (from the caller's code at `call_pc`)
            let mm = if f.mm {
                if ci.r().callstatus & CIST_MMRET == 0 {
                    return None;
                }
                let caller = ci.previous()?;
                let ccl = st.ci_lclosure(caller)?;
                let fin = Self::mm_fin_at(ccl.proto().code(), f.call_pc)?;
                (!mutants::active("fside_mm_plain")).then_some(fin)
            } else {
                if ci.r().callstatus & CIST_MMRET != 0 {
                    return None;
                }
                None
            };
            let nslots = usize::from(p.maxstacksize);
            let base_off = usize::from(f.func_slot) + 1;
            self.need_ext(base_off + nslots).ok()?;
            let low = self.low_for(&p).ok()?;
            if !self.deps.iter().any(|k| k.0 == key) {
                self.deps.push(ProtoKey(key));
                self.want_dep.push(p);
            }
            self.frames.push(RecFrame {
                ci: ci.as_ptr() as usize,
                func_abs,
                proto: key,
                closure: cl.as_ptr() as usize,
                base_off,
                nslots,
                call_pc: f.call_pc + u32::from(mutants::active("fside_call_pc_off")),
                nresults: f.nresults + i16::from(mutants::active("fside_nresults_off")),
                low,
                cref: NOREF,
                tail: f.tail && !mutants::active("fside_tail_dropped"),
                vpending: None,
                vdelta: 0,
                nextra: 0,
                pc_shift: 0,
                pc_wanted: 0,
                mm,
                ectx: None,
                tfor: None,
            });
        }
        let d = fr.len();
        self.cur_depth = d;
        self.next_depth = d;
        // the frame at depth `k` needed `k` spare `CallInfo` nodes behind the root's
        self.max_ci = d as u8;
        self.start_depth = d;
        self.start_proto = ProtoKey(self.frames[d].proto);
        self.pend = Some(Pend {
            next: self.head as usize,
            depth: d,
        });
        Some(())
    }

    /// Discards the recorded iteration (an in-place re-record, TR24c).
    fn reset(&mut self) {
        self.b = Builder::new(self.cfg.fold, self.cfg.cse);
        self.sm = SlotMap::new(self.nslots);
        self.frames.truncate(1);
        self.cur_depth = 0;
        self.next_depth = 0;
        self.ext = self.nslots;
        self.room = 0;
        self.max_ci = 0;
        self.deps.clear();
        self.want_dep.clear();
        self.late_dep.clear();
        self.inl_back.set(0);
        self.la_seen.borrow_mut().clear();
        self.first_store = None;
        self.ccalls_done = 0;
        self.entry_snap = None;
        self.path.clear();
        self.path_proto.clear();
        self.path_depth.clear();
        self.cost.clear();
        self.meta.clear();
        self.pend = None;
        self.budget_at = NOREF;
        self.func_ref = NOREF;
        self.kstr.clear();
        self.uv_cells.clear();
        self.uv_ids.clear();
        self.model = Model::new();
        self.reg.clear();
        self.tabs.clear();
        self.pinned = 0;
        self.filled.clear();
        self.efilled.clear();
        self.stored.clear();
        self.nfilled.clear();
        self.stored_nodes.clear();
        self.vals.clear();
        self.taint.clear();
        self.evald = 0;
        self.cell_taint.clear();
        self.cl_reg.clear();
        self.cls.clear();
        self.cl_filled.clear();
        self.elems = 0;
        self.ups_dirty = false;
        self.post_slot = None;
        self.stale.clear();
        self.ls_memo.clear();
        self.post_val = None;
        self.pend_lens.clear();
        self.newref_keys.clear();
        self.newref_exp.clear();
        self.nr_img.clear();
        self.ls_tabs.clear();
        self.ls_tnew.clear();
        self.last_tset = 0;
        self.top_rel = None;
        self.top_in = None;
        self.pcall = None;
        self.mm_bridge = None;
        self.mm_jmp = false;
        self.mm_pending = None;
        self.tail_args = None;
    }

    fn diverge<T>(&self, msg: impl FnOnce() -> String) -> R<T> {
        if self.cfg.verify {
            panic!(
                "recorder lockstep (TR17) after pc {} (op {}): {}\n{}",
                self.cur_pc,
                self.cur_op,
                msg(),
                super::print::dump(&self.b.ir)
            );
        }
        Err(AbortReason::Diverge)
    }

    /// P-E: the instruction being recorded belongs to an inlined callee and loops of callees are unrolled.
    fn unrolling(&self) -> bool {
        self.cur_depth > 0 && self.cfg.loop_unroll > 0
    }

    /// The `Lowered` of the frame being recorded.
    fn cur_low(&self) -> &Rc<Lowered> {
        &self.lows[self.frames[self.cur_depth].low as usize]
    }

    /// One callback (4.2 "Step").
    fn on_step(&mut self, st: &mut State, ci: Ci, pc: *const u32) -> R<Flow> {
        if st.hook_mask() != 0 {
            return Err(AbortReason::Hook);
        }
        let Some(cl) = st.ci_lclosure(ci) else {
            return Err(AbortReason::Frame);
        };
        let p = cl.proto();
        let code = p.code();
        let idx = (pc as usize).wrapping_sub(code.as_ptr() as usize) / 4;
        if self.phase == Phase::WaitHead && ci.as_ptr() as usize != self.root_ci {
            return Ok(Flow::Keep); // a call made before the head (tests arm early): not recorded
        }
        let depth = self.pend.map_or(0, |e| e.depth);
        // lazy validation (TR23.5): the same thread, the expected frame (by identity, function slot and closure; a
        // callee is bound at its first step: its caller is the expected caller `CallInfo`), the expected successor
        // pc and frame depth
        if !mutants::active("rec_no_validate") {
            let ok = self.frames.get(depth).is_some_and(|f| {
                jit_hooks::thread_ptr(st) as usize == self.thread
                    && &*p as *const Proto as usize == f.proto
                    && cl.as_ptr() as usize == f.closure
                    && ci.r().func.0 == f.func_abs
                    && idx < code.len()
                    && self.pend.is_none_or(|e| e.next == idx)
                    && if f.ci != 0 {
                        ci.as_ptr() as usize == f.ci
                    } else if f.pc_shift != 0 {
                        // P-O: the native's `CallInfo` (a C frame) lies between the callee's and the caller's
                        depth > 0
                            && ci
                                .previous()
                                .is_some_and(|m| m.r().callstatus & CIST_C != 0)
                            && ci
                                .previous()
                                .and_then(|m| m.previous())
                                .map(|c| c.as_ptr() as usize)
                                == Some(self.frames[depth - 1].ci)
                    } else {
                        depth > 0
                            && ci.previous().map(|c| c.as_ptr() as usize)
                                == Some(self.frames[depth - 1].ci)
                    }
            });
            if !ok {
                return Err(AbortReason::Frame);
            }
        }
        if idx >= code.len() || depth >= self.frames.len() {
            return Err(AbortReason::Frame);
        }
        if self.frames[depth].ci == 0 {
            // N3: a metamethod frame is the interpreter's own `mm_frame` (marked `CIST_MMRET`) and nothing else is
            // (a metamethod it calls through `ccall` would be a nested activation the trace cannot model)
            if self.frames[depth].mm.is_some() != (ci.r().callstatus & CIST_MMRET != 0) {
                return Err(AbortReason::Frame);
            }
            self.frames[depth].ci = ci.as_ptr() as usize;
        }
        // F2a: a side recording ends (links) when it reaches a loop head of the root frame after its first step
        let reached_head = match &self.cfg.side {
            Some(sd) if depth == 0 && !self.path.is_empty() && self.segs.is_empty() => {
                (sd.head_state.clone())(st, idx as u32)
            }
            // N3a: after a `RETF` the root is the caller: its loop heads are asked with its own prototype
            Some(_) if depth == 0 && !self.path.is_empty() => self
                .cfg
                .retf_head
                .clone()
                .and_then(|q| q(st, &p, idx as u32)),
            _ => None,
        };
        // F2b: a nest root ends (links) when it reaches a loop head of the root frame, other than its own, that has a
        // root trace. A backward arrival at any other pc is the inner loop of the prototype: the same abort as before.
        let cont_unroll = self.cont_unroll() && depth == 0;
        let nest_head = match &self.cfg.nest {
            Some(q)
                if self.cfg.side.is_none()
                    && depth == 0
                    && !self.path.is_empty()
                    && idx as u32 != self.head
                    && !cont_unroll =>
            {
                let h = (q.clone())(st, idx as u32);
                let back = self.path_depth.last() == Some(&0)
                    && self.path.last().is_some_and(|&p| idx as u32 <= p);
                if back && h != Some(true) {
                    return Err(if idx as u32 > self.head {
                        AbortReason::InnerLoop
                    } else {
                        AbortReason::LeaveLoop
                    });
                }
                h
            }
            _ => None,
        };
        // F2b: a linking exit (see `RecCfg::nest_frame`)
        let frame_head = match &self.cfg.nest_frame {
            Some(q)
                if (if self.cfg.side.is_none() {
                    self.cfg.nest_frame_roots
                } else {
                    (self.cfg.nest_frame_sides || !self.segs.is_empty())
                        && !mutants::active("side_frame_link_never")
                }) && depth > 0
                    && !self.path.is_empty() =>
            {
                (q.clone())(st, &p, idx as u32) == Some(true)
            }
            _ => false,
        };
        // F8: a table the recording holds a handle to was freed by a collector step since the last step
        if st.heap().watch_hit() {
            return Err(AbortReason::DepFreed);
        }
        // TR32: a dependency freed since the last step flags the recording
        if let Some(h) = self.cfg.deps.clone() {
            if !(h.alive)(st) {
                return Err(AbortReason::DepFreed);
            }
        }
        let vm = Vm {
            base: ci.r().func.0 + 1,
            root_base: self.root_func + 1,
            k: p.k(),
            code,
            cl,
            st,
        };
        // (the stack may have been reallocated since the last step: open upvalues of closures met from now on are located relative to the
        // current address of the root base)
        self.root_ptr = vm.st.slot_ptr(StkId(vm.root_base)) as usize;
        self.cur_depth = depth;
        self.next_depth = depth;
        self.cur_len = code.len();
        let t_ls = super::phase::now();
        let r_ls = self.lockstep(&vm);
        super::phase::add(super::phase::LOCKSTEP, t_ls);
        r_ls?;
        self.top_in = self.top_rel.take();
        let at = idx as u32;
        if self.phase == Phase::WaitHead {
            if at != self.head {
                return Ok(Flow::Keep);
            }
            self.begin(&vm)?;
        } else if self.path.is_empty() && at != self.head {
            return Err(AbortReason::Frame); // production arm sites arm at H (or at the exit pc of a side trace)
        } else if self.path.is_empty() {
            self.begin(&vm)?;
        }
        let mut retf_stop = false;
        if self.cfg.side.is_some() {
            match reached_head {
                None if frame_head => {
                    // C38 slice 1 (`sidenest`): a callee loop head with a root trace ends the side trace with the linking exit
                    if self.pcall.is_some() {
                        return Err(AbortReason::CFunc);
                    }
                    let n: u32 = self.cost.iter().sum();
                    let min = self.cfg.side.as_ref().map_or(0, |s| s.min_len);
                    if n < min && !mutants::active("side_short_ignored") {
                        return Err(AbortReason::SideShort);
                    }
                    return self.close_frame_link(at, &p);
                }
                None => {}
                Some(false) if self.side_unroll() => {} // R2: a loop of a recursive function is unrolled (see `target`)
                // N3a: the caller's loop head has no root trace (yet): the recording ends with the stitching exit before it (tier 1 dispatches the head)
                Some(false)
                    if !self.segs.is_empty() && !mutants::active("retf_untraced_aborts") =>
                {
                    retf_stop = true
                }
                Some(false) => return Err(AbortReason::SideUntraced),
                Some(true) => {
                    let n: u32 = self.cost.iter().sum();
                    let min = self.cfg.side.as_ref().map_or(0, |s| s.min_len);
                    if n < min && !mutants::active("side_short_ignored") {
                        return Err(AbortReason::SideShort);
                    }
                    self.link = Some(at);
                    return Ok(Flow::Closed(Box::new(self.close())));
                }
            }
        } else if frame_head && !mutants::active("frame_link_never") {
            if self.pcall.is_some() {
                return Err(AbortReason::CFunc);
            }
            return self.close_frame_link(at, &p);
        } else if nest_head == Some(true) && !mutants::active("nest_link_never") {
            self.link = Some(at);
            return Ok(Flow::Closed(Box::new(self.close())));
        } else if depth == 0 && at == self.head && !self.path.is_empty() {
            if let Some((slot, from, to)) = self.sm.unstable(&self.b.ir) {
                let _ = (slot, from, to);
                if self.restarts_left == 0 {
                    return Err(AbortReason::Unstable);
                }
                // TR24c: re-record in place from the types the finished iteration produced
                self.restarts_left -= 1;
                self.rerec += 1;
                self.reset();
                self.cur_len = code.len();
                self.begin(&vm)?;
            } else {
                if mutants::active("dep_edge_at_install") {
                    // mutant: the edges are registered at install (here) instead of when the call is inlined
                    for p in std::mem::take(&mut self.late_dep) {
                        if let Some(h) = self.cfg.deps.clone() {
                            if !(h.register)(vm.st, &p) {
                                return Err(AbortReason::DepFreed);
                            }
                        }
                    }
                }
                return Ok(Flow::Closed(Box::new(self.close())));
            }
        }
        if depth == 0 && at < self.head && self.cfg.side.is_none() && !cont_unroll {
            return Err(AbortReason::LeaveLoop);
        }
        if self.path.len() >= MAX_PATH
            || self.b.ir.ins.len() > MAX_INS - 16
            || (self.cfg.side.is_some() && self.b.ir.ins.len() > MAX_SIDE_INS)
            || (self.cfg.max_ins != 0
                && self.b.ir.ins.len() > self.cfg.max_ins
                && !mutants::active("deep_ins_unbounded"))
        {
            return Err(AbortReason::TooLong);
        }
        let mm_follower = self.mm_bridge.is_some() || std::mem::take(&mut self.mm_jmp);
        if self.cur_low().label_kind(at) == LabelKind::Dead && !mm_follower {
            return self.diverge(|| format!("stepped on pc {at} that has no tier-1 label"));
        }
        let mut cost = snap::step_cost(self.cur_low(), vm.code, at);
        if mm_follower && !mutants::active("mm_follower_free") {
            // N3: the interpreter fetches the `MMBIN*` behind a failed arithmetic instruction; tier 1 never does (no label)
            cost = cost.max(1);
        }
        self.path.push(at);
        self.path_proto.push(ProtoKey(self.frames[depth].proto));
        self.path_depth.push(depth as u8);
        self.cost.push(cost);
        self.cur_pc = at;
        if !mm_follower && self.cfg.mm {
            // N3b: the state a metamethod called by this step would restart from
            self.st_sm.clone_from(&self.sm);
            self.st_pc = at;
            self.st_step = self.path.len() as u32 - 1;
            self.st_top = self.top_code();
            self.st_low = self.frames[depth].low;
        }
        // P-O: inside an inlined protected call every guard uses the snapshot of the call
        self.cur_snap = self.pcall.as_ref().and_then(|c| c.snap);
        if !self.segs.is_empty() && (self.cfg.retf & 8 != 0 || retf_stop) {
            // N2c (test mode `retf` bit 3): the first instruction of the caller after the pop. The recording ends here with the stitching exit (the caller's
            // body is not recorded yet): the snapshot is the state in the caller's frame, segment 1
            self.end_exit()?;
            return Ok(Flow::Closed(Box::new(self.close())));
        }
        let next = self.record_one(&vm, idx)?;
        if self.stitched {
            return Ok(Flow::Closed(Box::new(self.close())));
        }
        // TR32: the edge of a proto inlined by this instruction is registered now, before the callee runs a step
        if !self.want_dep.is_empty() {
            let protos = std::mem::take(&mut self.want_dep);
            if mutants::active("dep_edge_at_install") {
                self.late_dep.extend(protos);
            } else if let Some(h) = self.cfg.deps.clone() {
                for p in protos {
                    if !(h.register)(vm.st, &p) {
                        return Err(AbortReason::DepFreed);
                    }
                }
            }
        }
        if let Some(c) = self.reroot.take() {
            // N2c: the instruction just recorded was the root `RETURN` that `Rec::retf` turned into a `RETF`: the interpreter performs it
            // next; its caller becomes the root of the VM-side recording
            // SAFETY: `c` is the `previous` of the live root `CallInfo` (checked by `Rec::retf` this very step).
            let caller = unsafe { Ci::from_raw(c as *mut _) };
            if !jit_hooks::rec_reroot(vm.st, caller) {
                return Err(AbortReason::Frame);
            }
        }
        self.pend = Some(Pend {
            next,
            depth: self.next_depth,
        });
        Ok(Flow::Keep)
    }

    /// Start of the recorded iteration at `H`: entry model, `POLL`, `BUDGET` (7.8: `TOP`).
    fn begin(&mut self, vm: &Vm<'_>) -> R<()> {
        if self.nslots > MAX_EXTENT {
            return Err(AbortReason::TooLong);
        }
        let side = self.cfg.side.is_some();
        // S1: the frame the recording starts in (the root unless the exit was inside inlined frames)
        let d0 = self.cur_depth;
        let low0 = self.frames[d0].low as usize;
        if side {
            // the exit pc of a parent trace: any tier-1 label; `rem` is what the label has prepaid (3.7)
            if self.lows[low0].label_kind(self.head) == LabelKind::Dead {
                return Err(AbortReason::Compile("side start has no tier-1 label"));
            }
            let lo = if mutants::active("fside_rem_root") {
                0
            } else {
                low0
            };
            self.start_rem = snap::land_info(&self.lows[lo], self.head).1;
        } else if self.lows[0].label_kind(self.head) != LabelKind::Head {
            return Err(AbortReason::Compile("loop head is not a tier-1 block head"));
        }
        // the model frame (lazily filled tables) and the closures' upvalues; the root closure is handle 1
        self.root_ptr = vm.st.slot_ptr(StkId(vm.root_base)) as usize;
        let root_cl = if d0 == 0 {
            vm.cl
        } else {
            // SAFETY: `root_ci` is the live root frame of the armed recording (validated by the VM at every step).
            vm.st
                .ci_lclosure(unsafe { Ci::from_raw(self.root_ci as *mut _) })
                .ok_or(AbortReason::Frame)?
        };
        let h = self.closure_handle(root_cl);
        debug_assert_eq!(h, 1);
        self.fill_closure(h);
        let nroot = if d0 == 0 { self.nslots } else { self.ext };
        let mut slots = Vec::with_capacity(nroot);
        for n in 0..nroot {
            slots.push(self.val_of(&vm.root_slot(n)));
        }
        self.model.slots = slots;
        // S1: the closures of the virtual frames (the callee refs that upvalue accesses and the snapshots' function slots
        // go through): the exit stub stored the function slots with their types, so the loads are unchecked; they come
        // before `BUDGET` because snapshot 0 already carries the frames
        for k in 1..=d0 {
            let slot = self.frames[k].base_off - 1;
            if self.known.get(slot).copied().flatten() != Some(Ty::Lcf) {
                return Err(AbortReason::Frame);
            }
            let (r, _) = self.rd_slot(vm, slot)?;
            self.frames[k].cref = r;
        }
        // snapshot 0: the state at H before anything is charged (POLL and BUDGET exits land on pcl[H])
        let sm0 = SlotMap::new(if d0 == 0 { self.nslots } else { self.sm.len() });
        let frames0 = if mutants::active("fside_frames_dropped") {
            Vec::new()
        } else {
            self.frame_recs()
        };
        let s0 = snap::take(&mut self.b.ir, &sm0, self.head, Land::Pre, &frames0, 0);
        self.meta.push(SnapMeta {
            step: 0,
            pc: self.head,
            uncharged: true,
            low: low0 as u32,
        });
        if !side {
            self.b
                .emit_flags(TOp::Poll, Ty::I64, 0, NOREF, NOREF, guard_aux(s0, 0))?;
        }
        self.budget_at = self
            .b
            .emit_flags(TOp::Budget, Ty::I64, 0, 0, NOREF, guard_aux(s0, 0))?;
        self.phase = Phase::Rec;
        Ok(())
    }

    /// F2b: ends the recording with the linking exit at pc `at` of the inlined frame `depth > 0`, whose prototype `p`
    /// has a root trace at that head: the snapshot is the state before `at` (frames materialised, pre-count label), the
    /// exit is an always-taken guard (`EQ 0, 1`), and the pipeline makes the handler continue at the head's dispatch
    /// cell instead of the tier-1 label.
    fn close_frame_link(&mut self, at: u32, p: &Proto) -> R<Flow> {
        if self.b.ir.snaps.len() >= MAX_SNAPS - 2 {
            return Err(AbortReason::TooLong);
        }
        let low = self.frames[self.cur_depth].low;
        let frames = self.frame_recs();
        self.cur_pc = at;
        let s = snap::take(&mut self.b.ir, &self.sm, at, Land::Pre, &frames, 0);
        self.meta.push(SnapMeta {
            step: self.path.len() as u32,
            pc: at,
            uncharged: false,
            low,
        });
        let (k0, k1) = (self.kint(0), self.kint(1));
        self.b.emit_raw(Ins {
            op: TOp::Eq,
            t: Ty::I64,
            flags: F_GUARD,
            a: k0,
            b: k1,
            prev: NOREF,
            aux: guard_aux(s, 0),
        })?;
        self.link = Some(at);
        self.frame_link = Some((s, ProtoKey(p as *const Proto as usize)));
        Ok(Flow::Closed(Box::new(self.close())))
    }

    /// Closes the iteration (rolled layout, 7.8).
    fn close(&mut self) -> Recorded {
        let n: u32 = self.cost.iter().sum();
        let bi = (self.budget_at - REF_BIAS) as usize;
        let mut side = None;
        let end_side = self.stitched && self.cfg.side.is_some() && self.link.is_none();
        if let Some(link_pc) = self.link.or(end_side.then_some(self.cur_pc)) {
            // F2a: the poll before the link exits through the empty map at the head (uncharged: the budget register
            // already holds tier 1's state at the head, see `fixup` below)
            let link_snap = match self.frame_link {
                Some((s, _)) => s,
                None if end_side => self.end_snap.expect("end_exit sets end_snap"),
                None => {
                    let s =
                        snap::take(&mut self.b.ir, &SlotMap::new(0), link_pc, Land::Pre, &[], 0);
                    // (N3a: after a `RETF` the link is in the last segment, whose root is the caller)
                    self.meta.push(SnapMeta {
                        step: 0,
                        pc: link_pc,
                        uncharged: true,
                        low: if self.segs.is_empty() {
                            0
                        } else {
                            self.frames[0].low as u32
                        },
                    });
                    s
                }
            };
            // the label the trace is entered at had prepaid `start_rem` instructions: charge only the rest. The
            // refunds below keep using `n` (the exit state is a function of the virtual budget `b + start_rem - n`).
            let start_rem = if mutants::active("side_budget_no_rem") {
                0
            } else {
                self.start_rem
            };
            self.b.ir.ins[bi].a = (n - start_rem.min(n)) as Ref;
            side = Some(SideInfo {
                start_pc: self.head,
                start_rem: self.start_rem,
                link_pc,
                link_snap,
                link_proto: self.frame_link.map_or(
                    ProtoKey(if self.segs.is_empty() {
                        self.entry_proto
                    } else {
                        self.proto
                    }),
                    |f| f.1,
                ),
                frame_link: self.frame_link.is_some(),
                no_link: end_side,
                start_proto: self.start_proto,
                frameful: self.start_depth > 0,
            });
        } else {
            self.b.ir.ins[bi].a = n as Ref;
        }
        snap::fixup(&mut self.b.ir, &self.meta, &self.lows, &self.cost, n);
        if side.is_some() && !mutants::active("side_start_refund") {
            // the uncharged snapshots (the start state, the link poll) are the tier-1 state of their label as it is:
            // nothing is refunded, not even the prepaid rest of the block `fixup` subtracts for a mid-block label
            for (s, m) in self.b.ir.snaps.iter_mut().zip(&self.meta) {
                if m.uncharged {
                    s.refund = 0;
                }
            }
        }
        if mutants::active("snap_prune_dead") {
            snap::prune_dead(&mut self.b.ir, &self.meta, &self.sm.last_read);
        }
        let mut ir = std::mem::take(&mut self.b.ir);
        ir.counts = vec![n];
        // (N2c: entry-relative extent / spare-CallInfo count over all segments: segment k's coordinates are `shift` slots higher, and the
        // CallInfo node of every popped frame exists already)
        let (ext_k, room_k) = (
            self.ext.saturating_sub(self.shift),
            (self.room as usize).saturating_sub(self.shift),
        );
        ir.extent = self
            .ext_entry
            .max(ext_k)
            .max(self.room_entry as usize)
            .max(room_k) as u16;
        if self.segs.is_empty() {
            ir.extent = self.ext.max(self.room as usize) as u16;
        }
        ir.depth = self.ci_entry.max(
            self.max_ci
                .saturating_sub(self.segs.len().saturating_sub(1) as u8),
        );
        ir.deps = std::iter::once(ProtoKey(self.entry_proto))
            .chain(self.deps.iter().copied())
            .collect();
        ir.head = (ProtoKey(self.entry_proto), self.head);
        if !self.segs.is_empty() {
            ir.segs = self.segs.clone();
            // snapshots of the steps behind a `RETF` belong to its segment
            for (s, m) in ir.snaps.iter_mut().zip(&self.meta) {
                s.seg = self.seg_steps.iter().filter(|&&st| m.step >= st).count() as u8;
            }
            // (the plain link snapshot of the caller's loop head belongs to the last segment)
            if let Some(si) = side.as_ref().filter(|si| !si.frame_link && !end_side) {
                ir.snaps[si.link_snap as usize].seg = (self.segs.len() - 1) as u8;
            }
        }
        ir.loop_at = None;
        ir.rec_call = self.rec_call.take();
        ir.rec_ret = self.rec_ret.map(|x| x.0);
        LA_LAST.with(|l| *l.borrow_mut() = self.la_seen.borrow().clone());
        ir.fn_root = self.cfg.fn_root && self.cfg.side.is_none() && ir.rec_call.is_none();
        Recorded {
            writeback: match self.rec_ret {
                Some((rr, r)) if rr.n == 1 => vec![SnapEntry {
                    slot: FUNC_SLOT,
                    r,
                    flags: 0,
                }],
                Some(_) => Vec::new(),
                None => self.sm.entries(),
            },
            snap_step: self.meta.iter().map(|m| m.step).collect(),
            path: std::mem::take(&mut self.path),
            path_proto: std::mem::take(&mut self.path_proto),
            path_depth: std::mem::take(&mut self.path_depth),
            cost: std::mem::take(&mut self.cost),
            room: self.room,
            entry_snap: self.entry_snap,
            ir,
            n,
            rerec: self.rerec,
            side,
            stitched: self.stitched,
        }
    }

    /// Makes room for slots below `n` (root-relative): the map, the model frame and the extent (`TooLong` above 200).
    fn need_ext(&mut self, n: usize) -> R<()> {
        if n > MAX_EXTENT {
            return Err(AbortReason::TooLong);
        }
        self.ext = self.ext.max(n);
        self.sm.grow(n);
        while self.model.slots.len() < n {
            self.model.slots.push(Val::NIL); // never loaded: a callee writes its registers before reading them
        }
        Ok(())
    }

    /// The `Lowered` of a callee (TR5.5), cached per proto; `Compile` if it does not lower.
    fn low_for(&mut self, p: &Proto) -> R<u32> {
        let key = p as *const Proto as usize;
        if let Some(&i) = self.low_of.get(&key) {
            return Ok(i);
        }
        let l = if self.cfg.landcache {
            super::cache::landing_cached(p)
        } else {
            lower_for_landing(p).map(Rc::new)
        }
        .ok_or(AbortReason::Compile("callee does not lower"))?;
        self.lows.push(l);
        let i = (self.lows.len() - 1) as u32;
        self.low_of.insert(key, i);
        Ok(i)
    }

    /// The inlined frames `1..=cur_depth` as snapshots carry them.
    fn frame_recs(&self) -> Vec<snap::FrameRec> {
        self.frames[1..=self.cur_depth]
            .iter()
            .map(|f| snap::FrameRec {
                proto: ProtoKey(f.proto),
                func_slot: (f.base_off - 1) as u16,
                call_pc: f.call_pc,
                nresults: f.nresults,
                closure: f.cref,
                tail: f.tail,
                nextra: if f.vdelta != 0 { f.nextra as i32 } else { -1 },
                mm: f.mm.is_some(),
            })
            .collect()
    }

    // ---- the lockstep model (TR17) ----

    fn handle(&mut self, t: TableRef) -> u64 {
        let key = t.as_ptr() as usize;
        if let Some(&h) = self.reg.get(&key) {
            return h;
        }
        self.model.tables.push(interp::MTable {
            array: Vec::new(),
            metatable: 0,
            // the flag byte as it is now: 5.4 (TR30) `BITRAS` for `ATOUCH`, F1 the metamethod-absence cache bits that
            // the metatable flag guards read (nothing recorded before this encounter can have changed them: a bit is
            // only ever set by the interpreter's own lookups and cleared by insertions, which are not recorded)
            flags: t.flags(),
            lsizenode: t.lsizenode(),
            // 5.4 (TR30): the model starts with the table's array limit (nothing recorded before this
            // encounter can have touched it: a touch is an access, which is an encounter)
            #[cfg(rustynail_lua54)]
            lenhint: t.alimit_hint(),
            #[cfg(not(rustynail_lua54))]
            lenhint: 0,
            nodes: Default::default(),
        });
        self.tabs.push(t);
        self.filled.push(false);
        let h = self.model.tables.len() as u64;
        self.reg.insert(key, h);
        // F1: the metatable word is the metatable's handle (a table may be its own metatable: the table is registered
        // first, so the recursion ends there)
        if let Some(mt) = t.metatable() {
            let mh = self.handle(mt);
            self.model.tables[h as usize - 1].metatable = mh;
        }
        h
    }

    /// The model handle of a closure (handle 1 is the root closure); its upvalues are copied by [`Rec::fill_closure`].
    fn closure_handle(&mut self, c: rn_core::value::Gc<LClosure>) -> u64 {
        let key = c.as_ptr() as usize;
        if let Some(&h) = self.cl_reg.get(&key) {
            return h;
        }
        self.model.closures.push(Vec::new());
        self.model.uv_rel.push(Vec::new());
        self.model.uv_cell.push(Vec::new());
        self.cls.push(c);
        self.cl_filled.push(false);
        let h = self.cls.len() as u64;
        self.cl_reg.insert(key, h);
        h
    }

    /// Copies the upvalues of closure `h` into the model (first encounter; nothing recorded before can have changed
    /// them: a store needs a `UREF` of this closure, which is an encounter).
    fn fill_closure(&mut self, h: u64) {
        let i = h as usize - 1;
        if self.cl_filled[i] {
            return;
        }
        self.cl_filled[i] = true;
        let c = self.cls[i];
        let mut ups = Vec::with_capacity(c.nupvalues());
        let mut rel = Vec::with_capacity(c.nupvalues());
        let mut cells = Vec::with_capacity(c.nupvalues());
        for u in 0..c.nupvalues() {
            let uv = c.upval(u);
            let v = self.val_of(&uv.get());
            // one model cell per REAL `UpVal` (P3d): closures that share an upvalue share its cell. The cell is
            // created at the first encounter of the `UpVal` (its value is the VM's then: only recorded stores can
            // have changed it, and each of them goes through an encountered closure, hence through its cell).
            let key = uv.as_ptr() as usize;
            let cell = match self.uv_cells.get(&key) {
                Some(&c) => c,
                None => {
                    self.model.cells.push(v);
                    let c = self.model.cells.len() as u32;
                    self.uv_cells.insert(key, c);
                    c
                }
            };
            cells.push(cell);
            ups.push(v);
            // (relative to the ENTRY base: `Model::shift` moves it to the current root after a `RETF`)
            rel.push(
                (uv.value_ptr() as usize).wrapping_sub(self.root_ptr.wrapping_add(16 * self.shift))
                    as u64,
            );
        }
        self.model.closures[i] = ups;
        self.model.uv_rel[i] = rel;
        self.model.uv_cell[i] = cells;
    }

    /// The model value of a VM value (a table or a Lua closure gets a handle; contents are copied by [`Rec::touch`]).
    fn val_of(&mut self, v: &Value) -> Val {
        match ty_of_value(v) {
            None => Val {
                t: Ty::Udata,
                bits: 0xDEAD,
            },
            Some(Ty::Lcf) => {
                let c = v.as_lclosure().expect("closure tag");
                Val {
                    t: Ty::Lcf,
                    bits: self.closure_handle(c),
                }
            }
            Some(Ty::Tab) => {
                let t = v.as_table().expect("table tag");
                Val {
                    t: Ty::Tab,
                    bits: self.handle(t),
                }
            }
            Some(t) => Val {
                t,
                bits: payload_bits(v, t),
            },
        }
    }

    /// A table or closure value enters the IR (an `SLOAD`, an element or upvalue load): its model array gets the true
    /// size (a zero-initialised allocation, nothing is copied), a closure's upvalues are copied.
    ///
    /// Why elements can be copied lazily, at the IR's first read or store of each one ([`Rec::fill_elem`], done when
    /// the lockstep evaluates that instruction, so the VM has already executed it): between the entry (`H`) and that
    /// instruction the VM can only have changed an element through an instruction the recorder recorded, as an
    /// `ASTORE` of that very element, and that store is the element's first access (it fills the element before it
    /// stores). Every source of a table reference is an encounter that allocates the array first: `SLOAD` (`rd`), an
    /// `ALOAD` / `ULOAD` of a table-valued cell (`array_get`, `Getupval`), the `FUNC_SLOT` closure (`begin`) and the
    /// callee closures (their `SLOAD` / `ULOAD` / `ALOAD`); call results, intrinsics and the registers of callees only
    /// carry refs and numbers that already exist, and nothing the recorder emits resizes an array (`Insert` aborts).
    /// `array_guards` asserts the encounter happened.
    fn touch(&mut self, v: &Value) -> R<()> {
        if let Some(c) = v.as_lclosure() {
            let h = self.closure_handle(c);
            self.fill_closure(h);
            return Ok(());
        }
        let Some(t) = v.as_table() else {
            return Ok(());
        };
        let h = self.handle(t);
        if self.filled[h as usize - 1] {
            return Ok(());
        }
        let asize = t.array_size() as usize;
        if self.elems.saturating_add(asize) > self.cfg.max_model_elems {
            return Err(AbortReason::Charge);
        }
        self.filled[h as usize - 1] = true;
        self.elems += asize;
        self.model.tables[h as usize - 1].array = zeroed_vals(asize);
        Ok(())
    }

    /// Copies array element `ix` (0-based) of table `h` into the model (the IR's first access to it).
    fn fill_elem(&mut self, h: u64, ix: usize) {
        let key = h << 32 | ix as u64;
        if !self.efilled.insert(key) {
            return;
        }
        let Some(&t) = self.tabs.get((h as usize).wrapping_sub(1)) else {
            return;
        };
        debug_assert!(
            self.filled[h as usize - 1],
            "array of table {h} not allocated"
        );
        if ix >= self.model.tables[h as usize - 1].array.len() {
            return; // the IR interpreter reports it
        }
        let v = match t.peek_int_arr(ix as i64 + 1) {
            Some(e) => self.val_of(&e),
            None => Val::NIL,
        };
        self.model.tables[h as usize - 1].array[ix] = v;
    }

    /// Copies hash node `ix` of table `h` into the model (F1). Unlike array elements this is done **at record time**,
    /// before the VM executes the instruction: an `HSTORE` guards the old value of its node (the node must not hold
    /// nil), and the lazy copy of [`Rec::fill_elem`] would see the value the instruction has already stored. At record
    /// time the model is in step with the VM (the lockstep of the previous instruction has run), so the copy is the
    /// node's current value, and a node copied earlier has been kept current by the IR's own stores.
    fn fill_node(&mut self, h: u64, ix: usize) {
        if !self.nfilled.insert(h << 32 | ix as u64) {
            return;
        }
        let t = self.tabs[h as usize - 1];
        let (ktt, kbits) = t.node_key(ix);
        let int = ktt == LUA_VNUMINT;
        let key = if ktt == ctb(LUA_VSHRSTR) || int {
            kbits
        } else {
            0
        };
        let nv = t.node_value(ix);
        let val = if nv.tag() & 0x0F == LUA_TNIL {
            Val::NIL
        } else {
            self.val_of(&nv)
        };
        self.model.tables[h as usize - 1].nodes.insert(
            ix as u32,
            interp::MNode {
                key,
                val,
                next: t.node_next(ix),
                int,
            },
        );
    }

    /// F3: the model's [`TOp::Tnew`] made table `h`; it is the table the VM left in the instruction's register.
    fn adopt_new_table(&mut self, h: u64, ins: &Ins) -> R<()> {
        let real = self.post_val.take().and_then(|v| v.as_table());
        let Some(real) = real.filter(|_| h as usize == self.tabs.len() + 1) else {
            return self
                .diverge(|| format!("TNEW: no table in the VM's register (model handle {h})"));
        };
        let mlen = self.model.tables[h as usize - 1].array.len();
        if mlen != real.array_size() as usize {
            return self
                .diverge(|| format!("TNEW: model array {mlen} VM array {}", real.array_size()));
        }
        let m = &mut self.model.tables[h as usize - 1];
        m.flags = real.flags();
        m.lsizenode = real.lsizenode();
        #[cfg(rustynail_lua54)]
        {
            m.lenhint = real.alimit_hint();
        }
        self.elems += m.array.len();
        self.tabs.push(real);
        self.filled.push(true);
        self.reg.insert(real.as_ptr() as usize, h);
        let _ = ins;
        Ok(())
    }

    /// BUNDLE-M C1: the lockstep is off for this recording. What the step still does is what the *recorder* depends on and the lockstep only
    /// happened to drive: the VM-observed values (`post_val`) a `TNEW` adopts, the registration of the tables the trace creates (model
    /// handle, `tabs` for the collector watch, `reg`), and the element budget of the tables a `NEWREF` / `TSET` rebuilds
    /// ([`Rec::resync_table`]: `Charge`, the caches of lazily copied parts). Nothing is compared.
    fn light_step(&mut self, vm: &Vm<'_>) -> R<()> {
        if let Some(abs) = self.post_slot.take() {
            self.post_val = Some(vm.st.get(StkId(abs)));
        }
        self.pend_str.clear();
        self.light_new()
    }

    /// [`Rec::light_step`]: the IR instructions emitted since the last step that change recorder state.
    fn light_new(&mut self) -> R<()> {
        while self.evald < self.b.ir.ins.len() {
            let i = self.evald;
            let ins = self.b.ir.ins[i];
            self.evald += 1;
            match ins.op {
                TOp::Tnew => {
                    // (what `eval_mini` + `adopt_new_table` do: the same handle numbering, an empty model array: nothing reads it)
                    let h = self.model.tables.len() as u64 + 1;
                    let real = self.post_val.take().and_then(|v| v.as_table());
                    let Some(real) = real.filter(|_| h as usize == self.tabs.len() + 1) else {
                        return self.diverge(|| {
                            format!("TNEW: no table in the VM's register (model handle {h})")
                        });
                    };
                    self.model.tables.push(interp::MTable {
                        array: Vec::new(),
                        metatable: 0,
                        flags: real.flags(),
                        lsizenode: real.lsizenode(),
                        lenhint: 0,
                        nodes: Default::default(),
                    });
                    #[cfg(rustynail_lua54)]
                    {
                        self.model.tables[h as usize - 1].lenhint = real.alimit_hint();
                    }
                    self.elems += ins.aux as usize;
                    if !mutants::active("light_tnew_unregistered") {
                        self.tabs.push(real);
                        self.filled.push(true);
                        self.reg.insert(real.as_ptr() as usize, h);
                    }
                    self.ls_tnew.insert(REF_BIAS + i as Ref, h);
                }
                TOp::Newref | TOp::Tset => {
                    if ins.op == TOp::Newref {
                        self.newref_keys.pop_front();
                        self.newref_exp.pop_front();
                    }
                    let known = self.ls_tabs.pop_front().flatten();
                    let h = match known {
                        Some(t) => self.reg.get(&(t.as_ptr() as usize)).copied(),
                        None => self.ls_tnew.get(&ins.a).copied(),
                    };
                    if let Some(h) = h.filter(|_| !mutants::active("light_charge_skip")) {
                        if let Some(&t) = self.tabs.get(h as usize - 1) {
                            self.resync_table(h, t)?;
                        }
                    }
                }
                TOp::Tlen => {
                    self.post_val.take();
                }
                _ => {}
            }
        }
        Ok(())
    }

    /// F3: before the model evaluates a [`TOp::Newref`] (the VM has inserted the key): the table's hash and array parts may
    /// have been rebuilt by the rehash (every node index and the array size can differ), so the model of the table is
    /// rebuilt from the VM (the facts about it are killed, TR13.6) and told the node the key went to.
    fn sync_newref(&mut self, ins: &Ins) -> R<()> {
        let h = self.val_bits(ins.a);
        let Some(key) = self.newref_keys.pop_front() else {
            return self.diverge(|| "NEWREF without a recorded key".to_string());
        };
        let Some(&t) = self.tabs.get((h as usize).wrapping_sub(1)) else {
            return self.diverge(|| format!("NEWREF on unknown table handle {h}"));
        };
        let Some(ix) = t.short_str_node(key) else {
            return self.diverge(|| "NEWREF: the VM did not insert the key".to_string());
        };
        self.resync_table(h, t)?;
        self.model.newref_ix = Some(ix as u32);
        // A1: an inline insertion must have left exactly the hash part the plan said (a divergence aborts the recording)
        if let Some(exp) = self.newref_exp.pop_front() {
            match exp {
                Some((tr, img)) => {
                    if t.hash_image() != img {
                        return self.diverge(|| {
                            "NEWREF: the VM's insertion differs from the inline plan".to_string()
                        });
                    }
                    self.nr_img.insert(tr, Some(img));
                }
                None => {
                    if TraceIr::is_ins(ins.a) {
                        self.nr_img.insert(ins.a, None);
                    }
                }
            }
        }
        Ok(())
    }

    /// F3: rebuilds the model of table `h` from the VM after an instruction that may have rehashed it (TR13.6: every
    /// fact about its parts is gone; array elements are copied again lazily, hash nodes at record time).
    fn resync_table(&mut self, h: u64, t: TableRef) -> R<()> {
        let asize = t.array_size() as usize;
        if self.elems.saturating_add(asize) > self.cfg.max_model_elems {
            return Err(AbortReason::Charge);
        }
        self.elems += asize;
        if mutants::active("newref_model_stale") {
            // MUTANT: the model of the table keeps its pre-insertion nodes and array (TR13.6 kill forgotten)
            return Ok(());
        }
        let m = &mut self.model.tables[h as usize - 1];
        m.nodes.clear();
        m.flags = t.flags();
        m.lsizenode = t.lsizenode();
        m.array = zeroed_vals(asize);
        #[cfg(rustynail_lua54)]
        {
            m.lenhint = t.alimit_hint();
        }
        self.filled[h as usize - 1] = true;
        self.efilled.retain(|k| k >> 32 != h);
        self.nfilled.retain(|k| k >> 32 != h);
        self.stored.retain(|&(hh, _)| hh != h);
        self.stored_nodes.retain(|&(hh, _)| hh != h);

        self.cell_taint.retain(|c| c.0 == 1 || c.1 != h);
        Ok(())
    }

    /// Evaluates the IR instructions emitted since the last step on the persistent model (TR17): each one runs alone
    /// through `tr::interp::run` on a one-instruction IR whose operands are the values computed before (so the cost of
    /// a step is independent of the trace length and of the table sizes). A guard that fails here is a divergence:
    /// the VM took the recorded path.
    fn eval_new(&mut self) -> R<()> {
        while self.evald < self.b.ir.ins.len() {
            let (v, t) = self.eval_one(self.evald)?;
            self.vals.push(v);
            self.taint.push(t);
            self.evald += 1;
        }
        Ok(())
    }

    fn val_bits(&self, r: Ref) -> u64 {
        if r == NOREF {
            0
        } else if r < REF_BIAS {
            interp::kbits(self.b.ir.kval(r))
        } else {
            self.vals[usize::from(r - REF_BIAS)]
        }
    }

    fn is_tainted(&self, r: Ref) -> bool {
        r != NOREF && r >= REF_BIAS && self.taint[usize::from(r - REF_BIAS)]
    }

    /// An operand of the one-instruction IR: a constant, or a pseudo `SLOAD` of a scratch slot holding the value.
    fn mini_operand(&self, mini: &mut TraceIr, extra: &mut Vec<Val>, base: usize, r: Ref) -> Ref {
        if r == NOREF {
            return NOREF;
        }
        if r < REF_BIAS {
            return mini.konst(self.b.ir.kval(r));
        }
        let t = self.b.ir.ty_of(r);
        extra.push(Val {
            t,
            bits: self.val_bits(r),
        });
        mini.emit(Ins {
            op: TOp::Sload,
            t,
            flags: 0,
            a: (base + extra.len() - 1) as Ref,
            b: 0,
            prev: NOREF,
            aux: 0,
        })
    }

    fn eval_one(&mut self, i: usize) -> R<(u64, bool)> {
        let ins = self.b.ir.ins[i];
        match ins.op {
            TOp::Sload if ins.a == FUNC_SLOT => {
                return Ok((
                    if mutants::active("retf_model_func_stale") {
                        1
                    } else {
                        self.model_func
                    },
                    false,
                ))
            } // the root closure is handle 1 (N3a: the current segment's after a `RETF`)
            TOp::Poll | TOp::Budget => return Ok((0, false)),
            // N2c: the guards of a `RETF` hold by construction (the recorder checked the caller's identity); the model was rebased by `Rec::retf`
            TOp::Retf => {
                // (the model is rebased here, in IR order: the loads emitted before the `RETF` are evaluated against the callee's frame, which
                // happens one step later than their emission)
                let below = self.retf_below.pop_front().ok_or(AbortReason::Diverge)?;
                let mut s = below;
                s.append(&mut self.model.slots);
                self.model.slots = s;
                self.model.shift += ins.b;
                self.model_func = self.retf_funcs.pop_front().ok_or(AbortReason::Diverge)?;
                return Ok((0, false));
            }
            // P-V: the constant string metatable: its model handle (registered by `string_method`)
            TOp::Kptr if ins.t == Ty::Tab => {
                let p = self.val_bits(ins.a);
                let h = self
                    .reg
                    .get(&(p as usize))
                    .copied()
                    .ok_or(AbortReason::Diverge)?;
                return Ok((h, false));
            }
            _ => {}
        }
        let (ra, rb) = ins.op.ref_operands();
        // the callee-identity guard compares the closure's proto with a proto address; the model's closures are
        // handles, and the recorder built the guard from the very closure the VM runs: it holds
        let mut bref = ins.b;
        if ins.op == TOp::Eqref
            && bref != NOREF
            && bref < REF_BIAS
            && matches!(self.b.ir.kval(bref), KVal::Ptr(_, PtrKind::Proto))
        {
            bref = ins.a;
        }
        let mut taint =
            ins.op == TOp::Calln || (ra && self.is_tainted(ins.a)) || (rb && self.is_tainted(bref));
        // memory: the first access to an array element copies it; cells carry the NaN taint
        let cell = match ins.op {
            TOp::Aload | TOp::Astore | TOp::Uload | TOp::Ustore | TOp::Hload | TOp::Hstore => {
                let addr = self.val_bits(ins.a);
                let (h, ix) = (addr >> 32, (addr & 0xFFFF_FFFF) as usize);
                let kind = match ins.op {
                    TOp::Uload | TOp::Ustore => 1,
                    TOp::Hload | TOp::Hstore => 2,
                    _ => 0,
                };
                if kind == 2 {
                    Some((2, h, ix))
                } else if kind == 0 {
                    self.fill_elem(h, ix);
                    Some((kind, h, ix))
                } else {
                    Some(self.ukey(h, ix))
                }
            }
            _ => None,
        };
        if let Some(c) = cell {
            match ins.op {
                TOp::Aload | TOp::Uload | TOp::Hload => taint |= self.cell_taint.contains(&c),
                _ => {
                    if self.is_tainted(ins.b) {
                        self.cell_taint.insert(c);
                    } else {
                        self.cell_taint.remove(&c);
                    }
                    if ins.op == TOp::Astore && !self.stored.contains(&(c.1, c.2)) {
                        self.stored.push((c.1, c.2));
                    }
                    if ins.op == TOp::Hstore && !self.stored_nodes.contains(&(c.1, c.2 as u32)) {
                        self.stored_nodes.push((c.1, c.2 as u32));
                    }
                }
            }
        }
        // F3: the model of a class-A helper learns the VM's choice (its placement, its border) first
        match ins.op {
            TOp::Newref => self.sync_newref(&ins)?,
            TOp::Tset => {
                let h = self.val_bits(ins.a);
                let Some(&t) = self.tabs.get((h as usize).wrapping_sub(1)) else {
                    return self.diverge(|| format!("TSET on unknown table handle {h}"));
                };
                self.resync_table(h, t)?;
            }
            TOp::Strto => {
                self.model.strto_result = self.pending_strto.take();
            }
            TOp::Snew | TOp::Concat | TOp::Lib => {
                self.model.str_result = self.post_strs.pop_front();
                if self.model.str_result.is_none() {
                    return self
                        .diverge(|| format!("{:?} without a recorded result at %{i}", ins.op));
                }
            }
            TOp::Tnext => {
                self.model.next_result = self.pending_next.take();
                if self.model.next_result.is_none() {
                    return self.diverge(|| format!("TNEXT without a recorded result at %{i}"));
                }
            }
            TOp::Tlen => {
                let n = match self.pend_lens.pop_front() {
                    Some(l) => Some(l),
                    None => self.post_val.take().and_then(|v| v.as_int()),
                };
                self.model.len_result = n.map(|n| n as u64);
                if n.is_none() {
                    return self.diverge(|| format!("LEN result is not an integer at %{i}"));
                }
            }
            _ => {}
        }
        // BUNDLE-C: the common pure / load / store ops are evaluated directly (the same semantics as `interp::run` on the
        // one-instruction IR); a guard that would fail, and every op outside the list, takes the generic path below. Under
        // `+verify` (and in debug builds) the generic path runs as well and must agree.
        if let Some(bits) = self.fast_eval(&ins) {
            if self.cfg.verify || cfg!(debug_assertions) {
                let slow = self.eval_mini(i, ins, ra, rb, bref, taint)?;
                assert_eq!(
                    slow.0, bits,
                    "fast_eval disagrees with interp::run at %{i} {ins:?}"
                );
            }
            return Ok((bits, taint));
        }
        self.eval_mini(i, ins, ra, rb, bref, taint)
    }

    /// BUNDLE-C: direct evaluation of the ops that dominate the lockstep (see the call in `eval_one0`); `None` = not handled here
    /// (or the instruction's own guard would fail: the generic path reports it).
    fn fast_eval(&mut self, ins: &Ins) -> Option<u64> {
        let bits = aux_bits(ins.aux);
        let two = |s: &Self| -> Option<(u64, u64)> {
            (ins.a != NOREF && ins.b != NOREF).then(|| (s.val_bits(ins.a), s.val_bits(ins.b)))
        };
        match ins.op {
            TOp::Sload => {
                let s = self.model.slots.get(usize::from(ins.a))?;
                if bits & 1 != 0 && s.t != ins.t {
                    return None;
                }
                Some(s.bits)
            }
            TOp::Add => two(self).map(|(a, b)| (a as i64).wrapping_add(b as i64) as u64),
            TOp::Sub => two(self).map(|(a, b)| (a as i64).wrapping_sub(b as i64) as u64),
            TOp::Mul => two(self).map(|(a, b)| (a as i64).wrapping_mul(b as i64) as u64),
            TOp::Band => two(self).map(|(a, b)| a & b),
            TOp::Bor => two(self).map(|(a, b)| a | b),
            TOp::Bxor => two(self).map(|(a, b)| a ^ b),
            TOp::Fadd | TOp::Fsub | TOp::Fmul | TOp::Fdiv => {
                two(self).map(|(a, b)| interp::fbin(ins.op, a, b))
            }
            TOp::Eq | TOp::Ne | TOp::Lt | TOp::Le | TOp::Gt | TOp::Ge | TOp::Ult | TOp::Uge => {
                let (a, b) = two(self)?;
                let ok = match ins.op {
                    TOp::Eq => a == b,
                    TOp::Ne => a != b,
                    TOp::Lt => (a as i64) < (b as i64),
                    TOp::Le => (a as i64) <= (b as i64),
                    TOp::Gt => (a as i64) > (b as i64),
                    TOp::Ge => (a as i64) >= (b as i64),
                    TOp::Ult => a < b,
                    _ => a >= b,
                };
                ok.then_some(0)
            }
            TOp::Aref | TOp::Uref => two(self).map(|(a, b)| a << 32 | (b & 0xFFFF_FFFF)),
            TOp::Aload => {
                if ins.a == NOREF {
                    return None;
                }
                let a = self.val_bits(ins.a);
                let (h, ix) = (a >> 32, (a & 0xFFFF_FFFF) as usize);
                let e = *h
                    .checked_sub(1)
                    .and_then(|x| self.model.tables.get(x as usize))?
                    .array
                    .get(ix)?;
                if bits & 1 != 0
                    && e.t != ins.t
                    && !(ins.flags & F_ANYSTR != 0 && matches!(e.t, Ty::ShrStr | Ty::LngStr))
                {
                    return None;
                }
                Some(e.bits)
            }
            TOp::Astore => {
                if ins.a == NOREF || ins.b == NOREF {
                    return None;
                }
                let a = self.val_bits(ins.a);
                let (h, ix) = (a >> 32, (a & 0xFFFF_FFFF) as usize);
                let nv = Val {
                    t: self.b.ir.ty_of(ins.b),
                    bits: self.val_bits(ins.b),
                };
                let e = h
                    .checked_sub(1)
                    .and_then(|x| self.model.tables.get_mut(x as usize))?
                    .array
                    .get_mut(ix)?;
                *e = nv;
                Some(0)
            }
            TOp::Tbar => Some(0),
            TOp::Fload => {
                if ins.a == NOREF {
                    return None;
                }
                let h = self.val_bits(ins.a);
                let t = h
                    .checked_sub(1)
                    .and_then(|x| self.model.tables.get(x as usize))?;
                match ins.b {
                    0 => Some(t.array.len() as u64),
                    1 | 5 => Some(h),
                    2 => Some(t.metatable),
                    3 => Some(u64::from(t.flags)),
                    4 => Some(u64::from(t.lsizenode)),
                    _ => None,
                }
            }
            TOp::Hrefk => {
                let (h, key) = two(self)?;
                let t = h
                    .checked_sub(1)
                    .and_then(|x| self.model.tables.get(x as usize))?;
                match t.nodes.get(&bits) {
                    Some(n) if n.key == key && !n.int => Some(h << 32 | u64::from(bits)),
                    _ => None,
                }
            }
            TOp::Hmiss => {
                let (h, key) = two(self)?;
                let (ix, delta) = hmiss_unpack(bits);
                let t = h
                    .checked_sub(1)
                    .and_then(|x| self.model.tables.get(x as usize))?;
                match t.nodes.get(&(ix as u32)) {
                    Some(n) if (n.key != key || n.int) && n.next == delta => Some(0),
                    _ => None,
                }
            }
            TOp::Hload => {
                if ins.a == NOREF {
                    return None;
                }
                let a = self.val_bits(ins.a);
                if a == interp::NEXT_CELL {
                    return None;
                }
                let (h, ix) = (a >> 32, (a & 0xFFFF_FFFF) as u32);
                let e = h
                    .checked_sub(1)
                    .and_then(|x| self.model.tables.get(x as usize))?
                    .nodes
                    .get(&ix)?;
                if bits & 1 != 0
                    && e.val.t != ins.t
                    && !(ins.flags & F_ANYSTR != 0 && matches!(e.val.t, Ty::ShrStr | Ty::LngStr))
                {
                    return None;
                }
                Some(e.val.bits)
            }
            _ => None,
        }
    }

    /// The generic evaluation of one instruction: `interp::run` on a one-instruction IR whose operands are the values computed before.
    fn eval_mini(
        &mut self,
        i: usize,
        ins: Ins,
        ra: bool,
        rb: bool,
        bref: Ref,
        taint: bool,
    ) -> R<(u64, bool)> {
        let mut mini = std::mem::take(&mut self.scratch.mini);
        mini.reset();
        mini.snapshot(Default::default(), &[]); // snapshot 0: the instruction's own guard failed
        let base = self.model.slots.len();
        let mut extra = std::mem::take(&mut self.scratch.extra);
        extra.clear();
        let a = if ra {
            self.mini_operand(&mut mini, &mut extra, base, ins.a)
        } else {
            ins.a
        };
        let b = if rb {
            self.mini_operand(&mut mini, &mut extra, base, bref)
        } else {
            ins.b
        };
        let aux = if ins.op.always_guard() || ins.flags & F_GUARD != 0 {
            guard_aux(0, aux_bits(ins.aux))
        } else if let Some(v) = ins.third() {
            ins.aux_with_third(self.mini_operand(&mut mini, &mut extra, base, v))
        } else {
            ins.aux
        };
        let rr = mini.emit(Ins {
            op: ins.op,
            t: ins.t,
            flags: ins.flags,
            a,
            b,
            prev: NOREF,
            aux,
        });
        // the probe: report the instruction's value through an always-failing guard
        let s1 = mini.snapshot(
            Default::default(),
            &[SnapEntry {
                slot: 0,
                r: rr,
                flags: 0,
            }],
        );
        let (k0, k1) = (mini.kint(0), mini.kint(1));
        mini.emit(Ins {
            op: TOp::Eq,
            t: Ty::I64,
            flags: F_GUARD,
            a: k0,
            b: k1,
            prev: NOREF,
            aux: guard_aux(s1, 0),
        });
        self.model.slots.extend_from_slice(&extra);
        let mut vbuf = std::mem::take(&mut self.scratch.vbuf);
        let res = interp::run_buf(&mini, &mut self.model, &mut vbuf);
        self.model.slots.truncate(base);
        self.scratch = Scratch { mini, extra, vbuf };
        match res {
            Ok(interp::Outcome::Exit { snap, slots }) if snap == s1 => {
                let bits = slots[0].2.bits;
                if ins.op == TOp::Tnew {
                    self.adopt_new_table(bits, &ins)?;
                }
                #[cfg(rustynail_lua54)]
                if ins.op == TOp::Tlen && ins.aux == 0 {
                    // `luaH_getn` moved the array limit: the model follows the VM
                    let h = self.val_bits(ins.a);
                    let hint = self.tabs[h as usize - 1].alimit_hint();
                    self.model.tables[h as usize - 1].lenhint = hint;
                }
                Ok((bits, taint))
            }
            other => {
                self.diverge(|| format!("the IR interpreter ended with {other:?} at %{i} {ins:?}"))
            }
        }
    }

    /// Does the VM value `real` equal the model value `m`? Exact bits (floats too); `lenient`: a NaN equals any NaN
    /// (the value derives from a `CALLN` helper: the interpreter canonicalises its NaNs, the VM's are libm's).
    fn same(&self, real: &Value, m: Val, lenient: bool) -> bool {
        let Some(t) = ty_of_value(real) else {
            return true; // not modelled
        };
        if t != m.t {
            return false;
        }
        match t {
            Ty::Num => {
                let (r, b) = (real.as_float().unwrap_or(0.0), f64::from_bits(m.bits));
                r.to_bits() == m.bits || (lenient && r.is_nan() && b.is_nan())
            }
            Ty::Tab => real
                .as_table()
                .is_some_and(|t| self.reg.get(&(t.as_ptr() as usize)) == Some(&m.bits)),
            Ty::Lcf => real
                .as_lclosure()
                .is_some_and(|c| self.cl_reg.get(&(c.as_ptr() as usize)) == Some(&m.bits)),
            _ => payload_bits(real, t) == m.bits,
        }
    }

    /// F8: watches the tables registered since the last call. The recorder keeps raw handles to every table it has
    /// encountered (`tabs`, `reg` keys by address); a collector step inside the recording (every allocating instruction is a
    /// safepoint) may free one that the program has dropped (a table replaced by a tail call, a dead `local`), and the
    /// lockstep would then read freed memory, and a new table could reuse the address. The collector sets a flag when it
    /// frees a watched object ([`rn_core::gc::Heap::watch_object`]) and the next step aborts the recording (`DepFreed`, no
    /// penalty); nothing is kept alive, so the run's GC behaviour stays the interpreter's.
    fn watch_new(&mut self, st: &mut State) {
        if mutants::active("rec_no_watch") {
            return;
        }
        for t in &self.tabs[self.pinned..] {
            st.heap().watch_object(t.as_ptr().cast());
        }
        self.pinned = self.tabs.len();
    }

    /// TR17: the instruction recorded last, checked against the VM's frame (the successor pc is checked by the validation of
    /// the step): the new IR instructions are evaluated, then every tracked slot, every array element the IR stored to and
    /// (after a `SETUPVAL`) every upvalue of an encountered closure is compared with the VM.
    fn lockstep(&mut self, vm: &Vm<'_>) -> R<()> {
        if self.pend.take().is_none() {
            return Ok(());
        }
        if !self.cfg.lockstep {
            return self.light_step(vm);
        }
        if let Some(t) = self.top_rel {
            // F4: the `thread.top` the producer of a multiple-result sequence left (TR22b relaxed)
            if vm.st.top().0 != vm.root_base + t {
                let real = vm.st.top().0;
                return self.diverge(|| {
                    format!(
                        "top: VM {}, model {}",
                        real as isize - vm.root_base as isize,
                        t
                    )
                });
            }
        }
        if !self.pend_str.is_empty() {
            let slots = std::mem::take(&mut self.pend_str);
            self.post_strs = slots
                .into_iter()
                .map(|abs| {
                    if abs == usize::MAX {
                        1
                    } else if abs == usize::MAX - 1 {
                        2 // F2: a string comparison that came out true
                    } else {
                        let v = vm.st.get(StkId(abs));
                        // P-B: a `Lib` result is a string, a light C function (its address) or `byte + 1`
                        if v.tag() == LUA_VLCF {
                            v.as_native().map_or(1, |f| f as usize as u64)
                        } else if let Some(i) = v.as_int() {
                            (i as u64).wrapping_add(1)
                        } else if let Some(f) = v.as_float() {
                            // BUNDLE-H: `math.random()`: the helper's result is the 53-bit integer of the float plus one
                            ((f * 9007199254740992.0) as u64).wrapping_add(1)
                        } else {
                            // (a collector step right behind the instruction cleared the dead slice above the result: any address)
                            v.as_strref().map_or(1, |s| s.as_ptr() as u64)
                        }
                    }
                })
                .collect();
        }
        if let Some(abs) = self.post_slot.take() {
            self.post_val = Some(vm.st.get(StkId(abs)));
        }
        self.eval_new()?;
        // F6: slots at or above the current frame's top are dead (a popped inlined callee's registers stay in the IR model): the
        // interpreter may use that slice as scratch (`luaH_newkey` / rehash keep the table at `top` while they allocate), so the VM
        // value there means nothing. The results of a multiple-result sequence (`top_rel`) are live beyond the frame.
        let limit = {
            let f = &self.frames[self.cur_depth];
            let l = f.base_off + f.nslots;
            self.top_rel.map_or(l, |t| l.max(t))
        };
        for slot in 0..self.sm.len() {
            let r = self.sm.cur[slot];
            if r == NOREF || self.stale.contains(&slot) || slot >= limit {
                continue;
            }
            let real = vm.root_slot(slot);
            let (tw, pw) = real.raw_words();
            if self.ls_memo.get(slot) == Some(&(r, tw, pw)) {
                continue;
            }
            let v = Val {
                t: self.b.ir.ty_of(r),
                bits: self.val_bits(r),
            };
            if !self.same(&real, v, self.is_tainted(r)) {
                return self.diverge(|| format!("slot {slot}: VM {real:?}, IR {v:?}"));
            }
            if self.ls_memo.len() <= slot {
                self.ls_memo.resize(slot + 1, (NOREF, 0, 0));
            }
            self.ls_memo[slot] = (r, tw, pw);
        }
        for &(h, ix) in &self.stored {
            let t = self.tabs[h as usize - 1];
            let real = t.peek_int_arr(ix as i64 + 1).unwrap_or(Value::NIL);
            let e = self.model.tables[h as usize - 1].array[ix];
            if !self.same(&real, e, self.cell_taint.contains(&(0, h, ix))) {
                return self.diverge(|| format!("table {h}[{}]: VM {real:?}, IR {e:?}", ix + 1));
            }
        }
        for &(h, ix) in &self.stored_nodes {
            let t = self.tabs[h as usize - 1];
            let real = t.node_value(ix as usize);
            let Some(e) = self.model.tables[h as usize - 1].nodes.get(&ix).copied() else {
                return self.diverge(|| format!("table {h} node {ix}: not in the model"));
            };
            if !self.same(&real, e.val, self.cell_taint.contains(&(2, h, ix as usize))) {
                return self
                    .diverge(|| format!("table {h} node {ix}: VM {real:?}, IR {:?}", e.val));
            }
        }
        if cfg!(rustynail_lua54) {
            // TR30: the model's array limits are the VM's (`ATOUCH` mirrors `alimit_touch` exactly)
            #[cfg(rustynail_lua54)]
            for (h, t) in self.tabs.iter().enumerate() {
                if self.filled[h] && t.alimit_hint() != self.model.tables[h].lenhint {
                    return self.diverge(|| {
                        format!(
                            "table {}: VM alimit {}, IR {}",
                            h + 1,
                            t.alimit_hint(),
                            self.model.tables[h].lenhint
                        )
                    });
                }
            }
        }
        if self.ups_dirty {
            for (h, c) in self.cls.iter().enumerate() {
                if !self.cl_filled[h] {
                    continue;
                }
                for u in 0..c.nupvalues() {
                    if self.in_root_frame(c.upval(u).value_ptr() as usize) {
                        continue; // N1: a root-frame slot, compared with the slot map above
                    }
                    let e = &self.model.upval(h as u64 + 1, u).expect("filled closure");
                    let real = c.upval(u).get();
                    let lenient = self.cell_taint.contains(&self.ukey(h as u64 + 1, u));
                    if !self.same(&real, *e, lenient) {
                        return self.diverge(|| {
                            format!("closure {} upvalue {u}: VM {real:?}, IR {e:?}", h + 1)
                        });
                    }
                }
            }
        }
        Ok(())
    }

    /// The taint key of upvalue `ix` of closure `h`: `(1, cell, 0)` (the shared cell of its real `UpVal`).
    fn ukey(&self, h: u64, ix: usize) -> (u8, u64, usize) {
        let c = self.model.uv_cell[h as usize - 1]
            .get(ix)
            .copied()
            .unwrap_or(0);
        (1, u64::from(c), if c == 0 { ix } else { 0 })
    }

    // ---- emission helpers ----

    fn ins(&mut self, op: TOp, t: Ty, a: Ref, b: Ref) -> R<Ref> {
        Ok(self.b.emit(op, t, a, b, 0)?)
    }

    /// Emits a guard of the current instruction. The snapshot (state before the instruction, TR3a) is created when
    /// the guard is really emitted: a guard that folds away or hits the CSE chain makes none.
    /// N3b: the guard `NE %alloc, 0` behind the class-A allocation at IR position `at`. Inside a metamethod frame whose body has changed
    /// nothing so far (only loads, arithmetic and guards since the call) it carries a snapshot of the state before the instruction that
    /// called the metamethod: an exit (a collector step is due) lets the interpreter run that instruction, instead of rebuilding the
    /// metamethod's frame and finishing it there (mutant `alloc_shallow_impure`: no purity check).
    fn alloc_guard(&mut self, a: Ref, b: Ref, at: usize) -> R<()> {
        let d = self.cur_depth;
        if d > 0 && self.pcall.is_none() {
            if let Some(ctx) = self.frames[d].ectx.clone() {
                let pure = self.b.ir.ins[ctx.ins_at..at]
                    .iter()
                    .all(|i| pcall_pure(i.op))
                    || mutants::active("alloc_shallow_impure");
                if pure && self.b.ir.snaps.len() < MAX_SNAPS - 2 {
                    let (land, _) = snap::land_info(&self.lows[ctx.low as usize], ctx.pc);
                    let s = snap::take(&mut self.b.ir, &ctx.sm, ctx.pc, land, &ctx.frames, ctx.top);
                    self.meta.push(SnapMeta {
                        step: ctx.step,
                        pc: ctx.pc,
                        uncharged: false,
                        low: ctx.low,
                    });
                    self.b
                        .emit_flags(TOp::Ne, Ty::I64, F_GUARD, a, b, guard_aux(s, 0))?;
                    return Ok(());
                }
            }
        }
        self.guard(TOp::Ne, Ty::I64, a, b, 0)?;
        Ok(())
    }

    fn guard(&mut self, op: TOp, t: Ty, a: Ref, b: Ref, bits: u32) -> R<Ref> {
        self.guard_with(op, t, a, b, bits, false)
    }

    /// A guard that may be an *entry guard* (TR6d: callee identity): when every operand is an entry value and no
    /// store precedes it on the path, it carries the entry snapshot (empty map at `H`, charged), which re-executes
    /// the whole iteration; otherwise it is an ordinary guard with its instruction's own snapshot. The mutant
    /// `entry_guard_past_store` ignores the stores.
    fn entry_guard(&mut self, op: TOp, t: Ty, a: Ref, b: Ref, bits: u32) -> R<Ref> {
        self.guard_with(op, t, a, b, bits, !(op == TOp::Eqref && self.cfg.ncallee))
    }

    fn guard_with(&mut self, op: TOp, t: Ty, a: Ref, b: Ref, bits: u32, entry: bool) -> R<Ref> {
        let at_entry = entry && self.entry_ok(a, b);
        let tentative = if at_entry {
            self.entry_snap
        } else {
            self.cur_snap
        }
        .unwrap_or(self.b.ir.snaps.len() as u32);
        let before = self.b.ir.ins.len();
        let r = self
            .b
            .emit_flags(op, t, F_GUARD, a, b, guard_aux(tentative, bits))?;
        let emitted = self.b.ir.ins.len() > before && self.b.ir.ins[before].flags & F_GUARD != 0;
        if !emitted {
            return Ok(r);
        }
        if !at_entry {
            self.pcall_guard_check(before)?;
        }
        if at_entry && self.entry_snap.is_none() {
            if self.b.ir.snaps.len() >= MAX_SNAPS - 2 {
                return Err(AbortReason::TooLong);
            }
            // the slice's `S0R`: the state at H (empty map), after the budget charge (`refund = n`, patched at close)
            let s = snap::take(
                &mut self.b.ir,
                &SlotMap::new(0),
                self.head,
                Land::Pre,
                &[],
                0,
            );
            debug_assert_eq!(s, tentative);
            self.meta.push(SnapMeta {
                step: 0,
                pc: self.head,
                uncharged: false,
                low: 0,
            });
            self.entry_snap = Some(s);
        } else if !at_entry && self.cur_snap.is_none() && self.pcall.is_some() {
            // P-O: the snapshot of the inlined call's `CALL` (state before it), shared by every guard of the body
            let c = self.pcall.as_ref().unwrap();
            if self.b.ir.snaps.len() >= MAX_SNAPS - 2 {
                return Err(AbortReason::TooLong);
            }
            let (land, _) = snap::land_info(&self.lows[c.low as usize], c.pc);
            let s = snap::take(&mut self.b.ir, &c.sm, c.pc, land, &c.frames, c.top);
            debug_assert_eq!(s, tentative);
            self.meta.push(SnapMeta {
                step: c.step,
                pc: c.pc,
                uncharged: false,
                low: c.low,
            });
            self.cur_snap = Some(s);
            self.pcall.as_mut().unwrap().snap = Some(s);
        } else if !at_entry && self.cur_snap.is_none() {
            if self.b.ir.snaps.len() >= MAX_SNAPS - 2 {
                return Err(AbortReason::TooLong);
            }
            let low = self.frames[self.cur_depth].low;
            let (land, _) = snap::land_info(&self.lows[low as usize], self.cur_pc);
            let frames = self.frame_recs();
            let top = self.top_code();
            let s = snap::take(&mut self.b.ir, &self.sm, self.cur_pc, land, &frames, top);
            debug_assert_eq!(s, tentative);
            self.meta.push(SnapMeta {
                step: self.path.len() as u32 - 1,
                pc: self.cur_pc,
                uncharged: false,
                low,
            });
            self.cur_snap = Some(s);
        }
        Ok(r)
    }

    /// TR6d: may a guard on `a`, `b` use the entry snapshot? Every operand must be an entry value and no `ASTORE` /
    /// `USTORE` (a possible alias of the operands' loads) may precede the guard on the recorded path.
    fn entry_ok(&self, a: Ref, b: Ref) -> bool {
        // (S1: the entry snapshot is the empty map at the head in the root frame; a side recording that starts inside inlined frames has none)
        self.start_depth == 0
            // (N3a: a value read after a `RETF` is in the caller's frame, not an entry value of the trace)
            && (self.segs.is_empty() || mutants::active("retf_entry_hoist"))
            && (self.first_store.is_none() || mutants::active("entry_guard_past_store"))
            && self.entry_value(a, 0)
            && self.entry_value(b, 0)
    }

    /// Is `r` an entry value (TR6d): a constant, an `SLOAD`, or a load of entry values (the no-store condition is the
    /// caller's)? Index arithmetic of an address counts as part of the load.
    fn entry_value(&self, r: Ref, depth: u32) -> bool {
        if r == NOREF || r < REF_BIAS {
            return true;
        }
        if depth > 8 {
            return false;
        }
        let i = self.b.ir.at(r);
        match i.op {
            TOp::Sload | TOp::Kptr => true,
            TOp::Fload | TOp::Uload | TOp::Aload => self.entry_value(i.a, depth + 1),
            TOp::Uref | TOp::Aref | TOp::Add | TOp::Sub => {
                self.entry_value(i.a, depth + 1) && self.entry_value(i.b, depth + 1)
            }
            _ => false,
        }
    }

    /// P-O: a guard is about to keep its snapshot. Inside an inlined protected call the snapshot is the call's, so the
    /// instructions emitted since the `CALL` must not have changed anything an exit could observe: a side-effecting IR
    /// instruction (a store, an allocation, a call, a library helper) before the guard aborts the recording (`CFunc`: the
    /// call stays a C call). The mutant `pcall_impure_ignored` skips the check.
    fn pcall_guard_check(&mut self, upto: usize) -> R<()> {
        let Some(c) = self.pcall.as_mut() else {
            return Ok(());
        };
        for i in c.scan..upto {
            if !pcall_pure(self.b.ir.ins[i].op) {
                c.impure = true;
            }
        }
        c.scan = c.scan.max(upto);
        if c.impure && !mutants::active("pcall_impure_ignored") {
            return Err(AbortReason::CFunc);
        }
        Ok(())
    }

    /// P-O: the `CallInfo` index (root frame 0) of the frame at depth `d`: each inlined protected call adds the native's frame.
    fn ci_depth(&self, d: usize) -> usize {
        d + self.frames[1..=d]
            .iter()
            .filter(|f| f.pc_shift != 0)
            .count()
    }

    fn kint(&mut self, i: i64) -> Ref {
        self.b.ir.kint(i)
    }

    fn nyi(&self) -> AbortReason {
        AbortReason::Nyi(self.cur_op)
    }

    /// Reads register `slot`: its current ref, or an `SLOAD` with typecheck at the first read.
    fn rd(&mut self, vm: &Vm<'_>, local: usize) -> R<(Ref, Ty)> {
        let f = &self.frames[self.cur_depth];
        if local >= f.nslots || f.base_off + local >= self.sm.len() {
            return Err(AbortReason::Diverge);
        }
        let slot = f.base_off + local;
        self.rd_slot(vm, slot)
    }

    /// Reads root-relative slot `slot` (any frame; P-E: the hidden arguments of a vararg frame lie below its base).
    fn rd_slot(&mut self, vm: &Vm<'_>, slot: usize) -> R<(Ref, Ty)> {
        if slot >= self.sm.len() {
            return Err(AbortReason::Diverge);
        }
        self.sm.last_read[slot] = self.path.len() as i32 - 1;
        let c = self.sm.cur[slot];
        if c != NOREF {
            let t = self.b.ir.ty_of(c);
            if matches!(t, Ty::ShrStr | Ty::LngStr) && TraceIr::is_ins(c) {
                // N5: the variant of this string may steer a recording decision
                self.b.ir.ins[usize::from(c - REF_BIAS)].flags |= F_OBS;
            }
            return Ok((c, t));
        }
        let v = vm.root_slot(slot);
        let ty = ty_of_value(&v).ok_or_else(|| self.nyi())?;
        self.touch(&v)?;
        // F2a: the parent's exit stub stored this slot with exactly this type: no check (an unchecked `SLOAD`, like the
        // closure slot's)
        let known = self.known.get(slot).copied().flatten() == Some(ty)
            || (mutants::active("side_known_all") && self.cfg.side.is_some());
        let r = if known {
            self.b.emit_raw(Ins {
                op: TOp::Sload,
                t: ty,
                flags: 0,
                a: slot as Ref,
                b: 0,
                prev: NOREF,
                aux: 0,
            })?
        } else {
            self.guard(TOp::Sload, ty, slot as Ref, NOREF, 1)?
        };
        self.sm.loaded(slot, r, ty);
        Ok((r, ty))
    }

    /// Defines register `local` of the current frame as `r`.
    fn def(&mut self, local: usize, r: Ref) {
        let slot = self.frames[self.cur_depth].base_off + local;
        self.def_abs(slot, r);
    }

    /// Defines the root-relative slot `slot` as `r`.
    fn def_abs(&mut self, slot: usize, r: Ref) {
        self.sm.grow(slot + 1);
        self.sm.def(slot, r);
        self.stale.remove(&slot);
    }

    /// A constant operand `K[idx]`.
    fn kconst(&mut self, vm: &Vm<'_>, idx: usize) -> R<(Ref, Ty, Value)> {
        let v = *vm.k.get(idx).ok_or(AbortReason::Diverge)?;
        let ty = ty_of_value(&v).ok_or_else(|| self.nyi())?;
        let r = match ty {
            Ty::Int => self.kint(v.as_int().unwrap_or(0)),
            Ty::Num => self.b.ir.knum(v.as_float().unwrap_or(0.0)),
            Ty::Nil | Ty::False | Ty::True => self.b.ir.konst(KVal::Pri(ty)),
            Ty::ShrStr => {
                let key = (self.frames[self.cur_depth].proto, idx);
                if let Some(&r) = self.kstr.get(&key) {
                    r
                } else {
                    let a = self
                        .b
                        .ir
                        .konst(KVal::Ptr(payload_bits(&v, ty), PtrKind::Raw));
                    let r = self.b.emit_raw(Ins {
                        op: TOp::Kptr,
                        t: Ty::ShrStr,
                        flags: 0,
                        a,
                        b: NOREF,
                        prev: NOREF,
                        aux: 0,
                    })?;
                    self.kstr.insert(key, r);
                    r
                }
            }
            _ => return Err(self.nyi()),
        };
        Ok((r, ty, v))
    }

    fn imm(&mut self, v: i64) -> (Ref, Ty, Value) {
        (self.kint(v), Ty::Int, Value::int(v))
    }

    fn num_of(&mut self, r: Ref, t: Ty) -> R<Ref> {
        if t == Ty::Num {
            Ok(r)
        } else {
            self.ins(TOp::ConvNumInt, Ty::Num, r, NOREF)
        }
    }

    /// An integer operand of a bitwise operation: integers pass, floats must hold an exact integer (`F2I_EXACT`).
    fn int_of(&mut self, r: Ref, t: Ty, v: &Value) -> R<Ref> {
        if t == Ty::Int {
            return Ok(r);
        }
        let f = v.as_float().ok_or(AbortReason::Metamethod)?;
        if float_to_int(f, F2I::Eq).is_none() {
            return Err(AbortReason::Metamethod); // "number has no integer representation": an error
        }
        self.guard(TOp::F2iExact, Ty::Int, r, NOREF, 0)
    }

    fn non_number(&self, tx: Ty, ty: Ty) -> AbortReason {
        if matches!(tx, Ty::ShrStr | Ty::LngStr) || matches!(ty, Ty::ShrStr | Ty::LngStr) {
            AbortReason::Coerce
        } else {
            AbortReason::Metamethod
        }
    }

    fn arith(&mut self, w: Ar, x: (Ref, Ty), y: (Ref, Ty), vx: &Value, vy: &Value) -> R<(Ref, Ty)> {
        let (a, ta) = x;
        let (b, tb) = y;
        let num = |t: Ty| matches!(t, Ty::Int | Ty::Num);
        if !num(ta) || !num(tb) {
            return Err(self.non_number(ta, tb));
        }
        use Ar::*;
        match w {
            Band | Bor | Bxor | Shl | Shr => {
                let ai = self.int_of(a, ta, vx)?;
                let bi = self.int_of(b, tb, vy)?;
                let op = match w {
                    Band => TOp::Band,
                    Bor => TOp::Bor,
                    Bxor => TOp::Bxor,
                    Shl => TOp::Shl,
                    _ => TOp::Shr,
                };
                Ok((self.ins(op, Ty::Int, ai, bi)?, Ty::Int))
            }
            Add | Sub | Mul if ta == Ty::Int && tb == Ty::Int => {
                let op = match w {
                    Add => TOp::Add,
                    Sub => TOp::Sub,
                    _ => TOp::Mul,
                };
                Ok((self.ins(op, Ty::Int, a, b)?, Ty::Int))
            }
            Idiv | Mod if ta == Ty::Int && tb == Ty::Int => {
                if vy.as_int() == Some(0) {
                    return Err(AbortReason::Metamethod); // "attempt to perform 'n//0'": an error
                }
                let op = if w == Idiv { TOp::Idiv } else { TOp::Mod };
                Ok((self.guard(op, Ty::Int, a, b, 0)?, Ty::Int))
            }
            _ => {
                let af = self.num_of(a, ta)?;
                let bf = self.num_of(b, tb)?;
                // F1: `luai_numpow` is `b == 2 ? a * a : pow(a, b)`: a constant exponent 2 gives the product
                let sq = self.cfg.f1pow
                    && TraceIr::is_const(b)
                    && match self.b.ir.kval(b) {
                        KVal::Int(k) => k == 2 || mutants::active("f1_pow_any"),
                        KVal::Num(k) => k == 2.0f64.to_bits() || mutants::active("f1_pow_any"),
                        _ => false,
                    };
                let r = match w {
                    Add => self.ins(TOp::Fadd, Ty::Num, af, bf)?,
                    Sub => self.ins(TOp::Fsub, Ty::Num, af, bf)?,
                    Mul => self.ins(TOp::Fmul, Ty::Num, af, bf)?,
                    Div => self.ins(TOp::Fdiv, Ty::Num, af, bf)?,
                    // F1: `luai_numpow` is `b == 2 ? a * a : pow(a, b)`: a constant exponent 2 is the product
                    Pow if sq => self.ins(TOp::Fmul, Ty::Num, af, af)?,
                    Pow => self
                        .b
                        .emit(TOp::Calln, Ty::Num, af, bf, interp::CALLN_POW)?,
                    Idiv => {
                        let q = self.ins(TOp::Fdiv, Ty::Num, af, bf)?;
                        if self.cfg.f1floor {
                            self.ins(TOp::Ffloor, Ty::Num, q, NOREF)?
                        } else {
                            self.b
                                .emit(TOp::Calln, Ty::Num, q, NOREF, interp::CALLN_FLOOR)?
                        }
                    }
                    Mod => self
                        .b
                        .emit(TOp::Calln, Ty::Num, af, bf, interp::CALLN_FMOD)?,
                    _ => unreachable!(),
                };
                Ok((r, Ty::Num))
            }
        }
    }

    /// A comparison of two values; emits the guard that asserts the observed outcome and returns it.
    fn compare(&mut self, c: Cmp, x: (Ref, Ty), y: (Ref, Ty), vx: &Value, vy: &Value) -> R<bool> {
        let (a, ta) = x;
        let (b, tb) = y;
        let num = |t: Ty| matches!(t, Ty::Int | Ty::Num);
        if !num(ta) || !num(tb) {
            if c != Cmp::Eq {
                if self.cfg.f2cmp
                    && matches!(ta, Ty::ShrStr | Ty::LngStr)
                    && matches!(tb, Ty::ShrStr | Ty::LngStr)
                {
                    return self.str_order(c, x, y, vx, vy);
                }
                return Err(
                    if matches!(ta, Ty::ShrStr | Ty::LngStr)
                        && matches!(tb, Ty::ShrStr | Ty::LngStr)
                    {
                        self.nyi()
                    } else {
                        AbortReason::Metamethod
                    },
                );
            }
            // equality of non-numbers: decided by the types (their checks are guards already)
            if ta != tb {
                return Ok(false);
            }
            return match ta {
                Ty::Nil | Ty::True | Ty::False => Ok(true),
                // P-B: short strings are interned: equal iff the same object (the pointers are the refs' values)
                Ty::ShrStr => {
                    let eq =
                        vx.as_strref().map(|s| s.as_ptr()) == vy.as_strref().map(|s| s.as_ptr());
                    if !mutants::active("streq_guard_skip") {
                        self.guard(if eq { TOp::Eq } else { TOp::Ne }, Ty::I64, a, b, 0)?;
                    }
                    Ok(eq)
                }
                // F2: long strings compare by content, in a helper
                Ty::LngStr if self.cfg.f2cmp => {
                    let (sx, sy) = (
                        vx.as_strref().ok_or(AbortReason::Diverge)?,
                        vy.as_strref().ok_or(AbortReason::Diverge)?,
                    );
                    let eq =
                        sx.as_ptr() == sy.as_ptr() || sx.get().as_bytes() == sy.get().as_bytes();
                    self.str_cmp_op(lib::STREQ, a, b, eq)?;
                    Ok(eq)
                }
                _ => Err(self.nyi()),
            };
        }
        let (nx, ny) = (
            vx.as_number().ok_or(AbortReason::Diverge)?,
            vy.as_number().ok_or(AbortReason::Diverge)?,
        );
        let observed = match c {
            Cmp::Eq => eq_num(nx, ny),
            Cmp::Lt => lt_num(nx, ny),
            Cmp::Le => le_num(nx, ny),
            Cmp::Gt => lt_num(ny, nx),
            Cmp::Ge => le_num(ny, nx),
        };
        if ta == Ty::Int && tb == Ty::Int {
            let op = match (c, observed) {
                (Cmp::Eq, true) => TOp::Eq,
                (Cmp::Eq, false) => TOp::Ne,
                (Cmp::Lt, true) | (Cmp::Ge, false) => TOp::Lt,
                (Cmp::Le, true) | (Cmp::Gt, false) => TOp::Le,
                (Cmp::Gt, true) | (Cmp::Le, false) => TOp::Gt,
                (Cmp::Ge, true) | (Cmp::Lt, false) => TOp::Ge,
            };
            self.guard(op, Ty::I64, a, b, 0)?;
            return Ok(observed);
        }
        // floats, or int against float: an int converts exactly only within +-2^53 (guarded, 8)
        let conv = |s: &mut Rec, r: Ref, t: Ty, v: &Value| -> R<Ref> {
            if t == Ty::Num {
                return Ok(r);
            }
            let i = v.as_int().ok_or(AbortReason::Diverge)?;
            if !(-TWO53..=TWO53).contains(&i) {
                return Err(s.nyi()); // a lossy conversion: no guard can express the exact compare
            }
            let hi = s.kint(TWO53);
            let lo = s.kint(-TWO53);
            s.guard(TOp::Le, Ty::I64, r, hi, 0)?;
            s.guard(TOp::Ge, Ty::I64, r, lo, 0)?;
            s.ins(TOp::ConvNumInt, Ty::Num, r, NOREF)
        };
        let af = conv(self, a, ta, vx)?;
        let bf = conv(self, b, tb, vy)?;
        let cc = match c {
            Cmp::Eq => 0,
            Cmp::Lt => 2,
            Cmp::Le => 3,
            Cmp::Gt => 4,
            Cmp::Ge => 5,
        };
        self.guard(TOp::Fcmp, Ty::I64, af, bf, cc | u32::from(observed) << 4)?;
        Ok(observed)
    }

    /// F2: the helper `kind` (`STREQ` / `STRLT` / `STRLE`) of two strings and the guard that the outcome is the recorded one `observed`.
    fn str_cmp_op(&mut self, kind: u32, a: Ref, b: Ref, observed: bool) -> R<()> {
        // the lockstep gives the model the outcome as the helper encodes it (2 = true, 1 = false)
        let hint = if observed { usize::MAX - 1 } else { usize::MAX };
        let r = self.lib_op(kind, Ty::I64, a, b, None, hint)?;
        if !mutants::active("strcmp_guard_skip") {
            let k = self.kint(1 + i64::from(observed));
            self.guard(TOp::Eq, Ty::I64, r, k, 0)?;
        }
        Ok(())
    }

    /// F2: `<`, `<=`, `>`, `>=` of two strings (bytewise, as `less_than`).
    fn str_order(&mut self, c: Cmp, x: (Ref, Ty), y: (Ref, Ty), vx: &Value, vy: &Value) -> R<bool> {
        let (sx, sy) = (
            vx.as_strref().ok_or(AbortReason::Diverge)?,
            vy.as_strref().ok_or(AbortReason::Diverge)?,
        );
        let (bx, by) = (sx.get().as_bytes(), sy.get().as_bytes());
        let (kind, p, q, observed) = match c {
            Cmp::Lt => (lib::STRLT, x.0, y.0, bx < by),
            Cmp::Le => (lib::STRLE, x.0, y.0, bx <= by),
            Cmp::Gt => (lib::STRLT, y.0, x.0, by < bx),
            Cmp::Ge => (lib::STRLE, y.0, x.0, by <= bx),
            Cmp::Eq => return Err(AbortReason::Diverge),
        };
        self.str_cmp_op(kind, p, q, observed)?;
        Ok(observed)
    }

    /// `FLOAD t field`.
    fn fload(&mut self, t: Ref, f: Field, ty: Ty) -> R<Ref> {
        self.ins(TOp::Fload, ty, t, f as Ref)
    }

    /// The guards of an array access on a table without metatable (4.3): `metatable == 0`, `0 <= key - 1 < asize`;
    /// returns the `AREF`.
    fn array_guards(&mut self, t: Ref, key: Ref, tbl: TableRef, nonnil_read: bool) -> R<Ref> {
        self.array_guards_raw(t, key, tbl, nonnil_read, false)
    }

    /// [`Rec::array_guards`]; `raw` (F2: `rawget` / `rawset`) never looks at the metatable.
    fn array_guards_raw(
        &mut self,
        t: Ref,
        key: Ref,
        tbl: TableRef,
        nonnil_read: bool,
        raw: bool,
    ) -> R<Ref> {
        debug_assert!(
            self.reg
                .get(&(tbl.as_ptr() as usize))
                .is_some_and(|&h| self.filled[h as usize - 1]),
            "array access on a table whose model array was never allocated (touch)"
        );
        // B2: the read of an element recorded non-nil is a raw hit whatever the metatable (`__index` only answers a nil); the `ALOAD`
        // tag guard exits on a nil. (MUTANT `b2mt_store`: the same for the stores, where `__newindex` applies to a nil slot.)
        let skip_mt = raw || (self.cfg.b2mt && (nonnil_read || mutants::active("b2mt_store")));
        if !skip_mt {
            let mt = self.fload(t, Field::Metatable, Ty::P64)?;
            let z = self.kint(0);
            self.guard(TOp::Eq, Ty::I64, mt, z, 0)?;
        }
        let asize = self.fload(t, Field::Asize, Ty::U32)?;
        let one = self.kint(1);
        let idx0 = self.ins(TOp::Sub, Ty::Int, key, one)?;
        self.guard(TOp::Ult, Ty::I64, idx0, asize, 0)?;
        if cfg!(rustynail_lua54) && !mutants::active("atouch_skip") {
            // TR30: every integer-key array access performs `alimit_touch` right after its bounds guard (like the
            // interpreter's `luaH_getint` and `templates_table.rs`). It is an effect of the instruction: no entry
            // guard may move past it (`first_store`), and the optimiser never merges or hoists it.
            self.first_store.get_or_insert(self.b.ir.ins.len());
            self.ins(TOp::Atouch, Ty::Nil, t, idx0)?;
        }
        let arr = self.fload(t, Field::Array, Ty::P64)?;
        self.ins(TOp::Aref, Ty::P64, arr, idx0)
    }

    /// The integer key of an array access: an integer, or a float with an exact integer value (`F2I_EXACT`).
    fn int_key(&mut self, vm: &Vm<'_>, slot: usize) -> R<(Ref, i64)> {
        let (r, t) = self.rd(vm, slot)?;
        let v = vm.slot(slot);
        match t {
            Ty::Int => Ok((r, v.as_int().unwrap_or(0))),
            Ty::Num => {
                let f = v.as_float().unwrap_or(f64::NAN);
                match float_to_int(f, F2I::Eq) {
                    Some(i) => Ok((self.guard(TOp::F2iExact, Ty::Int, r, NOREF, 0)?, i)),
                    None => Err(AbortReason::TableHash),
                }
            }
            _ => Err(AbortReason::TableHash),
        }
    }

    /// The table of an array access and its real element: aborts for a non-table, a metatable, a key outside the
    /// array part.
    fn table_for(&self, tv: &Value, tt: Ty, key: i64) -> R<(TableRef, Option<Value>)> {
        if tt != Ty::Tab {
            return Err(AbortReason::Metamethod);
        }
        let t = tv.as_table().ok_or(AbortReason::Diverge)?;
        // BUNDLE-S S2 (`amt`): a table with a metatable is an array access like any other (the callers guard what the metatable can change)
        if t.metatable().is_some() && !self.cfg.amt {
            return Err(AbortReason::MetaTable);
        }
        if !(1..=i64::from(t.array_size())).contains(&key) {
            if super::cache::verbose_aborts() {
                eprintln!(
                    "[trace] int key {} outside the array part (size {})",
                    key,
                    t.array_size()
                );
            }
            return Err(if self.cfg.rec_any {
                AbortReason::IntMiss
            } else {
                AbortReason::TableHash
            });
        }
        Ok((t, t.get_int_arr(key)))
    }

    /// H1: the table of an integer-key access and the node index of the key when the key lives in the hash part (outside the array
    /// part, present with any value, nil included). Off for 5.4 (`alimit` bookkeeping of the array accesses) and `RUSTYNAIL_JIT_HASHINT=0`.
    fn int_hash_node(&self, tv: &Value, tt: Ty, k: i64) -> Option<(TableRef, usize)> {
        if cfg!(rustynail_lua54) || tt != Ty::Tab || !super::hashint_enabled() {
            return None;
        }
        let tbl = tv.as_table()?;
        if (1..=i64::from(tbl.array_size())).contains(&k) {
            return None;
        }
        let idx = tbl.int_node(k)?;
        (idx < 1 << 15).then_some((tbl, idx))
    }

    /// H1: the node of the integer key `key` (value `k` at record time) at node index `idx` of `tbl` (reference `t`): the node-vector
    /// size guard, the node vector and the integer `HREFV` (a `hashint` chain walk at run time; exits when the key is not in the chain).
    fn hash_ref_int(&mut self, t: Ref, tbl: TableRef, key: Ref, idx: usize) -> R<Ref> {
        let h = self.handle(tbl);
        self.fill_node(h, idx);
        if !mutants::active("hrefi_lsize_skip") {
            let ls = self.fload(t, Field::Lsizenode, Ty::U32)?;
            let kl = self.kint(i64::from(tbl.lsizenode()));
            self.guard(TOp::Eq, Ty::I64, ls, kl, 0)?;
        }
        let node = self.fload(t, Field::Node, Ty::P64)?;
        self.guard(
            TOp::Hrefv,
            Ty::P64,
            node,
            key,
            u32::from(tbl.lsizenode()) | HREF_INT,
        )
    }

    /// H1: `t[k]` for an integer key held by the hash part of `tbl`. A present non-nil value is a raw hit whatever the metatable (`HREFV` +
    /// typed `HLOAD`); a nil value needs the TR14 guard of `__index` (`None` when a metatable may have one: the caller's other paths apply).
    fn hash_int_get(
        &mut self,
        t: (Ref, Ty),
        tbl: TableRef,
        key: Ref,
        idx: usize,
    ) -> R<Option<Ref>> {
        let nv = tbl.node_value(idx);
        let nil_valued = nv.tag() & 0x0F == LUA_TNIL;
        if nil_valued && !TableRef::no_tm(tbl.metatable(), Tm::Index) {
            return Ok(None);
        }
        let ety = if nil_valued {
            Ty::Nil
        } else {
            ty_of_value(&nv).ok_or_else(|| self.nyi())?
        };
        if nil_valued {
            if !mutants::active("hashint_nil_meta_skip") {
                self.meta_guard(t.0, tbl, Tm::Index)?;
            }
        } else {
            self.touch(&nv)?;
        }
        let hr = self.hash_ref_int(t.0, tbl, key, idx)?;
        Ok(Some(self.guard(TOp::Hload, ety, hr, NOREF, 1)?))
    }

    /// H1: `t[k] = val` for an integer key held by the hash part of `tbl`: `HSTORE` into the node (guarded against a nil old value, which
    /// would be an insertion or a `__newindex`), or -- for a node that holds nil -- the TR14 `__newindex` guard and an unconditional store;
    /// the write barrier for a collectable value.
    fn hash_int_set(
        &mut self,
        t: (Ref, Ty),
        tbl: TableRef,
        key: Ref,
        idx: usize,
        val: (Ref, Ty),
    ) -> R<()> {
        let nil_old = tbl.node_value(idx).tag() & 0x0F == LUA_TNIL;
        if nil_old && !self.cfg.nil_store {
            return Err(AbortReason::Insert);
        }
        let hr = self.hash_ref_int(t.0, tbl, key, idx)?;
        if nil_old {
            if !mutants::active("hashint_newindex_skip") {
                self.meta_guard(t.0, tbl, Tm::NewIndex)?;
            }
            self.first_store.get_or_insert(self.b.ir.ins.len());
            // H1: the interpreter's store into a nil-valued node goes through `luaV_finishset`, which keeps the table at `top` (scratch above the
            // frame, not modelled)
            self.stale_above_frame();
            self.ins(TOp::Hstore, Ty::I64, hr, val.0)?;
        } else {
            self.first_store.get_or_insert(self.b.ir.ins.len());
            self.guard(TOp::Hstore, Ty::I64, hr, val.0, 1)?;
        }
        if collectable(val.1) && !mutants::active("hstore_tbar_skip") {
            self.ins(TOp::Tbar, Ty::I64, t.0, val.0)?;
        }
        Ok(())
    }

    fn array_get(&mut self, vm: &Vm<'_>, t: (Ref, Ty), tslot: usize, key: Ref, k: i64) -> R<Ref> {
        self.array_get_k(vm, t, tslot, key, k, key)
    }

    /// [`Rec::array_get`]; `orig`: the key as the program holds it (a float with an integer value is converted for the access, but `__index` is
    /// called with the float, as `luaV_finishget` does).
    fn array_get_k(
        &mut self,
        vm: &Vm<'_>,
        t: (Ref, Ty),
        tslot: usize,
        key: Ref,
        k: i64,
        orig: Ref,
    ) -> R<Ref> {
        let tv = vm.slot(tslot);
        if let Some((tbl, idx)) = self.int_hash_node(&tv, t.1, k) {
            if let Some(r) = self.hash_int_get(t, tbl, key, idx)? {
                return Ok(r);
            }
        }
        if self.cfg.mm && t.1 == Ty::Tab && !cfg!(rustynail_lua54) {
            if let Some(tbl) = tv.as_table() {
                if tbl.metatable().is_some() && !(1..=i64::from(tbl.array_size())).contains(&k) {
                    self.mm_index_int(vm, t.0, tbl, key, orig)?;
                    return Ok(NOREF);
                }
            }
        }
        let (tbl, ev) = self.table_for(&tv, t.1, k)?;
        // M1: a nil element of the array part (`{ "if", c, b, e }` with `e == nil`) is an `ALOAD` of type nil (any nil variant)
        let ev = match ev {
            Some(v) => v,
            None if self.cfg.nilload => Value::NIL,
            None => return Err(AbortReason::TableNil),
        };
        let ety = ty_of_value(&ev).ok_or_else(|| self.nyi())?;
        self.touch(&ev)?;
        if tbl.metatable().is_some() {
            // BUNDLE-S S2: the read of an element recorded non-nil is a raw hit whatever the metatable (`__index` only answers a nil; the `ALOAD`
            // type guard exits on a nil); a nil element needs the TR14 guard of `__index` (the metatable exists and its flag cache says "absent")
            if ety == Ty::Nil && !mutants::active("amt_get_nil_skip") {
                self.meta_guard(t.0, tbl, Tm::Index)?;
            }
            let aref = self.array_guards_raw(t.0, key, tbl, false, true)?;
            return self.guard(TOp::Aload, ety, aref, NOREF, 1);
        }
        let aref = self.array_guards(t.0, key, tbl, ety != Ty::Nil)?;
        self.guard(TOp::Aload, ety, aref, NOREF, 1)
    }

    fn array_set(
        &mut self,
        vm: &Vm<'_>,
        t: (Ref, Ty),
        tslot: usize,
        key: Ref,
        k: i64,
        val: (Ref, Ty),
    ) -> R<()> {
        let tv = vm.slot(tslot);
        // F3: a key outside the array part is a raw set by the table itself (class A: growth, rehash). An empty slot
        // inside the array part is stored inline: `table_for` has refused a metatable (no `__newindex` can apply), and
        // the interpreter's set of an array slot is the plain store.
        if t.1 == Ty::Tab {
            if let Some(tbl) = tv.as_table() {
                if !(1..=i64::from(tbl.array_size())).contains(&k) {
                    if let Some((tbl, idx)) = self.int_hash_node(&tv, t.1, k) {
                        if tbl.node_value(idx).tag() & 0x0F != LUA_TNIL || self.cfg.nil_store {
                            return self.hash_int_set(t, tbl, key, idx, val);
                        }
                    }
                    return self.int_insert(tbl, t, key, val);
                }
            }
        }
        let (tbl, ev) = self.table_for(&tv, t.1, k)?;
        // (5.4: the interpreter's store into an empty slot goes through `finish_set`, whose `alimit` touches are not
        // those of the inline path -- the lockstep sees alimit 4 against 3 -- so it stays an `Insert` abort there)
        if ev.is_none() && (val.1 == Ty::Nil || cfg!(rustynail_lua54)) {
            return Err(AbortReason::Insert);
        }
        let has_mt = tbl.metatable().is_some();
        let aref = self.array_guards_raw(t.0, key, tbl, false, has_mt)?;
        if has_mt {
            // BUNDLE-S S2: a store over an element that is not nil is raw whatever the metatable (`__newindex` only applies to an absent key: an
            // `ALOAD` type guard of the old value); a store into a nil slot needs the TR14 guard of `__newindex`
            match ev {
                Some(old) => {
                    let ety = ty_of_value(&old).ok_or_else(|| self.nyi())?;
                    // (the lockstep copies an element lazily, after the VM has run the instruction: the old value is copied now, as `lib_tremove` does)
                    let h = self.handle(tbl);
                    self.fill_elem(h, (k - 1) as usize);
                    if !mutants::active("amt_set_old_skip") {
                        self.guard(TOp::Aload, ety, aref, NOREF, 1)?;
                    }
                }
                None => {
                    if !mutants::active("amt_set_nil_skip") {
                        self.meta_guard(t.0, tbl, Tm::NewIndex)?;
                    }
                }
            }
        }
        self.first_store.get_or_insert(self.b.ir.ins.len());
        self.ins(TOp::Astore, Ty::I64, aref, val.0)?;
        if collectable(val.1) {
            self.ins(TOp::Tbar, Ty::I64, t.0, val.0)?;
        }
        Ok(())
    }

    // ---- hash part (F1) ---------------------------------------------------------------------------------------------

    /// The key of a field access: constant `K[idx]` (a short string; a long one is `Nyi`). Returns the constant ref of
    /// the key's address (`KVal::Ptr`, interned: one ref per string) and the string.
    fn field_key(&mut self, vm: &Vm<'_>, idx: usize) -> R<(Ref, StrRef)> {
        let v = *vm.k.get(idx).ok_or(AbortReason::Diverge)?;
        if ty_of_value(&v) != Some(Ty::ShrStr) {
            return Err(self.nyi());
        }
        let key = v.as_strref().ok_or(AbortReason::Diverge)?;
        let k = self
            .b
            .ir
            .konst(KVal::Ptr(payload_bits(&v, Ty::ShrStr), PtrKind::Raw));
        Ok((k, key))
    }

    /// The key of `t[R[slot]]` when the register holds a short string. A string that comes from a proto constant
    /// (`LOADK` in the recorded path) is its `KPTR` already; any other string value (a loaded register, an upvalue, an
    /// element) is compared with the address seen at record time by an `EQREF` guard. The address is never
    /// dereferenced, and it cannot be taken for another string: a passing `HREFK` means that the node holds a **live**
    /// string at that address (the node references it), and a passing `EQREF` means that the register holds a live
    /// string at that address, so both are the same object even if the string recorded was freed and its address
    /// reused (the ABA of TR7 needs a compare against a *dead* object's identity that nothing else pins).
    fn reg_key(&mut self, vm: &Vm<'_>, slot: usize, r: Ref) -> R<(Ref, StrRef)> {
        let v = vm.slot(slot);
        let key = v.as_strref().ok_or(AbortReason::Diverge)?;
        if TraceIr::is_ins(r) && self.b.ir.at(r).op == TOp::Kptr {
            let k = self.b.ir.at(r).a;
            return Ok((k, key));
        }
        // F3b: a key built by this iteration's `CONCAT` is a different string almost every time: the `EQREF` guard below
        // would fail on nearly every iteration (an exit storm slower than tier 1), so such a loop is not traced
        if TraceIr::is_ins(r)
            && matches!(self.b.ir.at(r).op, TOp::Concat)
            && !mutants::active("concat_key_traced")
        {
            return Err(AbortReason::TableHash);
        }
        let k = self
            .b
            .ir
            .konst(KVal::Ptr(payload_bits(&v, Ty::ShrStr), PtrKind::Raw));
        self.guard(TOp::Eqref, Ty::I64, r, k, 0)?;
        Ok((k, key))
    }

    /// N6: the key of `t[R[slot]]` for a short-string register. When the key is a computed value (not a proto constant, not a
    /// `CONCAT` result) and **present** in the hash part of the table at record time, the key is the SSA value itself: no
    /// `EQREF` against the address seen, no constant node index; the access becomes `HREFV` (a chain walk at run time), so one trace
    /// serves every key of the loop (`t[k] = (t[k] or 0) + 1`). The marker for such a key is that its ref is not a constant. An absent
    /// key, a constant key and everything else keep the specialised path ([`Rec::reg_key`]).
    fn reg_key_any(
        &mut self,
        vm: &Vm<'_>,
        slot: usize,
        r: Ref,
        tv: &Value,
        tty: Ty,
    ) -> R<(Ref, StrRef)> {
        if tty == Ty::Tab && TraceIr::is_ins(r) && self.cfg.hrefv && self.cfg.dynmask {
            // DYNMASK: a computed key is the SSA value whether it is present or absent at record time (loads: a nil-miss `HREFV`; stores: the
            // `HREFV` of a present key, the `NEWREF` helper for an absent one)
            let op = self.b.ir.at(r).op;
            if op != TOp::Kptr
                && (op != TOp::Concat || self.cfg.libcall_mask & 256 != 0)
                && tv.as_table().is_some()
            {
                let key = vm.slot(slot).as_strref().ok_or(AbortReason::Diverge)?;
                return Ok((r, key));
            }
        }
        if tty == Ty::Tab && TraceIr::is_ins(r) && self.cfg.hrefv {
            let op = self.b.ir.at(r).op;
            if op != TOp::Kptr && (op != TOp::Concat || self.cfg.libcall_mask & 256 != 0) {
                let key = vm.slot(slot).as_strref().ok_or(AbortReason::Diverge)?;
                if let Some(tbl) = tv.as_table() {
                    if tbl.short_str_node(key).is_some() {
                        return Ok((r, key));
                    }
                }
            }
        }
        self.reg_key(vm, slot, r)
    }

    /// TR14 for a read that may find `nil`: the table has no metatable (`FLOAD metatable == 0`), or its metatable lacks
    /// `__index` according to the flag cache (`metatable != 0` and `flags & (1 << TM_INDEX) != 0`); either way no
    /// metamethod can run. The cache bits are set by the interpreter's own lookups and cleared by `invalidate_tm_cache`
    /// on every insertion, which happens only outside traces (no `NEWREF` in F1), so they are stable on a trace.
    fn meta_guard(&mut self, t: Ref, tbl: TableRef, ev: rn_core::meta::Tm) -> R<()> {
        let mt = self.fload(t, Field::Metatable, Ty::P64)?;
        let z = self.kint(0);
        match tbl.metatable() {
            None => {
                self.guard(TOp::Eq, Ty::I64, mt, z, 0)?;
            }
            Some(m) => {
                if !TableRef::no_tm(Some(m), ev) {
                    return Err(AbortReason::Metamethod);
                }
                self.guard(TOp::Ne, Ty::I64, mt, z, 0)?;
                if !mutants::active("meta_flag_skip") {
                    let fl = self.fload(mt, Field::Flags, Ty::U32)?;
                    let bit = self.kint(1i64 << ev as u8);
                    let masked = self.ins(TOp::Band, Ty::I64, fl, bit)?;
                    self.guard(TOp::Ne, Ty::I64, masked, z, 0)?;
                }
            }
        }
        Ok(())
    }

    /// The guards that name the node of an existing short-string key: the node-vector size (so the slot index is in
    /// range), the node vector, and `HREFK` (the node at the recorded index holds this key). Returns the `HREFK`.
    /// The table is `tbl`, whose model entry gets the node ([`Rec::fill_node`]).
    fn hash_ref(&mut self, t: Ref, tbl: TableRef, kref: Ref, idx: usize) -> R<Ref> {
        // (the mutant makes every non-load lookup a nil-miss one: a store through it would write the static nil node)
        self.hash_ref_k(t, tbl, kref, idx, mutants::active("dynmask_nil_store"))
    }

    /// DYNMASK: is `kref` a dynamic key whose lookup takes the run-time-mask `HREFV` (`HREF_DYN`)?
    fn dyn_key(&self, kref: Ref) -> bool {
        self.cfg.dynmask && TraceIr::is_ins(kref)
    }

    /// [`Rec::hash_ref`]; `nilmiss`: (a load of a dynamic key under `dynmask`) a key that is not found resolves to the static nil node instead of
    /// exiting, so the typed `HLOAD` behind it decides (the type check fails on a present value of another type, passes for nil).
    fn hash_ref_k(
        &mut self,
        t: Ref,
        tbl: TableRef,
        kref: Ref,
        idx: usize,
        nilmiss: bool,
    ) -> R<Ref> {
        let h = self.handle(tbl);
        if idx != usize::MAX {
            self.fill_node(h, idx);
        }
        if self.dyn_key(kref) {
            // DYNMASK: no node-count guard, no node-vector load: the instruction reads both from the table when it runs
            if nilmiss {
                let aux = guard_aux(0, HREF_DYN | HREF_NIL);
                return Ok(self.b.emit(TOp::Hrefv, Ty::P64, t, kref, aux)?);
            }
            return self.guard(TOp::Hrefv, Ty::P64, t, kref, HREF_DYN);
        }
        if !mutants::active("hrefk_lsize_skip") {
            let ls = self.fload(t, Field::Lsizenode, Ty::U32)?;
            let kl = self.kint(i64::from(tbl.lsizenode()));
            self.guard(TOp::Eq, Ty::I64, ls, kl, 0)?;
        }
        let node = self.fload(t, Field::Node, Ty::P64)?;
        if TraceIr::is_ins(kref) {
            // N6: a dynamic key (an instruction's value, never an interned constant): the walk finds the node at run time
            return self.guard(TOp::Hrefv, Ty::P64, node, kref, u32::from(tbl.lsizenode()));
        }
        self.guard(TOp::Hrefk, Ty::P64, node, kref, idx as u32)
    }

    /// The node of `key` in the table `tv` (a table value; `Metamethod` for anything else: an index operation on a
    /// non-table calls a metamethod), `TableHash` when the key is not in the hash part (absent: only a miss could
    /// prove it, or the slot index does not fit the guard bits).
    fn hash_target(&self, tv: &Value, tt: Ty, key: StrRef, dynk: bool) -> R<(TableRef, usize)> {
        if tt != Ty::Tab {
            return Err(AbortReason::Metamethod);
        }
        let tbl = tv.as_table().ok_or(AbortReason::Diverge)?;
        let idx = tbl.short_str_node(key).ok_or(AbortReason::TableHash)?;
        // (a dynamic key under `dynmask` names no node index: only the model's `fill_node` sees it)
        if idx >= 1 << 15 && !dynk {
            return Err(AbortReason::TableHash);
        }
        Ok((tbl, idx))
    }

    /// `t[key]` for a short-string key (a field, a global, a method): the raw value of the table when the key is present
    /// with a non-nil value (`HREFK` + typed `HLOAD`), else -- P-A -- what the interpreter's `luaV_finishget` finds: `nil`
    /// when no `__index` can run, or the same lookup in the table-valued `__index` of the metatable, up to
    /// [`MAX_INDEX_CHAIN`] levels (a function-valued `__index`, or a non-table indexed value, aborts `Metamethod`).
    ///
    /// The guards of one level that has no value: the key is absent from the node vector (`HMISS` steps along the
    /// recorded chain, after the `lsizenode` guard) or present with a nil value (`HREFK` + `HLOAD` nil typecheck); then the
    /// metatable: `mt != 0` (an instance whose metatable was removed), `HREFK` + `HLOAD` typed `Tab` of its `__index` (an
    /// `__index` replaced by a function, removed or a non-table), and the next level through the loaded table (no table
    /// address is embedded, so nothing can be freed and reused under the trace: every later guard reads the table the
    /// loaded reference names). The end of a chain with no `__index` takes the TR14 flag guard of the last table.
    fn hash_get(&mut self, vm: &Vm<'_>, tv: &Value, t: (Ref, Ty), key: (Ref, StrRef)) -> R<Ref> {
        if t.1 != Ty::Tab {
            return Err(AbortReason::Metamethod);
        }
        let cur = tv.as_table().ok_or(AbortReason::Diverge)?;
        self.hash_get_chain(vm, cur, t.0, key)
    }

    /// P-V: `s:method` on a string receiver: the string metatable is a constant of the trace (`KPTR`; the runtime flushes
    /// every trace when the string metatable is replaced, `jit_hooks::type_mt_changed`), its `__index` field is a guarded
    /// `HREFK` + `HLOAD` (so replacing or rewriting the field exits), and the table it names is walked like any
    /// `__index` chain ([`Rec::hash_get_chain`]): the method is a guarded load, no helper call.
    fn string_method(&mut self, vm: &Vm<'_>, key: (Ref, StrRef)) -> R<Ref> {
        let m = vm
            .st
            .type_metatable(rn_core::value::LuaType::String)
            .ok_or(AbortReason::Metamethod)?;
        let name = vm.st.g().heap.tm_name(rn_core::meta::Tm::Index);
        let ix = m.short_str_node(name).ok_or(AbortReason::Metamethod)?;
        if ix >= 1 << 15 {
            return Err(AbortReason::TableHash);
        }
        let iv = m.node_value(ix);
        let next = iv.as_table().ok_or(AbortReason::Metamethod)?;
        self.touch(&Value::table(m))?;
        self.touch(&iv)?;
        let ka = self.b.ir.konst(KVal::Ptr(m.as_ptr() as u64, PtrKind::Raw));
        let mr = self.b.emit_raw(Ins {
            op: TOp::Kptr,
            t: Ty::Tab,
            flags: 0,
            a: ka,
            b: NOREF,
            prev: NOREF,
            aux: 0,
        })?;
        let kname = self.b.ir.konst(KVal::Ptr(
            payload_bits(&Value::string(name), Ty::ShrStr),
            PtrKind::Raw,
        ));
        let hr = self.hash_ref(mr, m, kname, ix)?;
        let bit = u32::from(!mutants::active("index_tab_typecheck_skip"));
        let cref = self.guard(TOp::Hload, Ty::Tab, hr, NOREF, bit)?;
        let r = self.hash_get_chain(vm, next, cref, key)?;
        if r == NOREF {
            self.mm_pending = None;
            return Err(AbortReason::Metamethod); // (a function-valued `__index` behind the string metatable: not an in-place call)
        }
        Ok(r)
    }

    /// The `__index` chain walk of [`Rec::hash_get`], starting at the table `cur` (reference `cref`).
    fn hash_get_chain(
        &mut self,
        vm: &Vm<'_>,
        mut cur: TableRef,
        mut cref: Ref,
        key: (Ref, StrRef),
    ) -> R<Ref> {
        let mut depth = 0usize;
        loop {
            let found = cur.short_str_node(key.1);
            let dynk = self.dyn_key(key.0);
            if let Some(idx) = found {
                if idx >= 1 << 15 && !dynk {
                    return Err(AbortReason::TableHash);
                }
            }
            if dynk {
                // DYNMASK: the lookup of a dynamic key is the run-time-mask `HREFV` that yields the static nil node on a miss, then the typed
                // `HLOAD`: a value of the recorded type (or nil) passes whatever the table's size, a different one exits
                let ev = found.map(|i| cur.node_value(i));
                let nilv = ev.as_ref().map_or(true, |v| v.tag() & 0x0F == LUA_TNIL);
                let mt = cur.metatable();
                if !nilv || TableRef::no_tm(mt, rn_core::meta::Tm::Index) {
                    let ety = if nilv {
                        Ty::Nil
                    } else {
                        ty_of_value(ev.as_ref().unwrap()).ok_or_else(|| self.nyi())?
                    };
                    if nilv {
                        if !mutants::active("dynmask_nil_meta_skip") {
                            self.meta_guard(cref, cur, rn_core::meta::Tm::Index)?;
                        }
                    } else {
                        self.touch(ev.as_ref().unwrap())?;
                    }
                    let hr =
                        self.hash_ref_k(cref, cur, key.0, found.unwrap_or(usize::MAX), true)?;
                    return self.guard(TOp::Hload, ety, hr, NOREF, 1);
                }
            }
            let nil_valued = found.is_some_and(|i| cur.node_value(i).tag() & 0x0F == LUA_TNIL);
            let mt = cur.metatable();
            if let Some(idx) =
                found.filter(|_| !nil_valued || TableRef::no_tm(mt, rn_core::meta::Tm::Index))
            {
                // a present value, or a nil value that no `__index` can replace (F1)
                let ev = cur.node_value(idx);
                let ety = if nil_valued {
                    Ty::Nil
                } else {
                    ty_of_value(&ev).ok_or_else(|| self.nyi())?
                };
                if ety == Ty::Nil {
                    if !mutants::active("hash_nil_meta_skip")
                        && !(TraceIr::is_ins(key.0) && mutants::active("hrefv_meta_skip"))
                    {
                        self.meta_guard(cref, cur, rn_core::meta::Tm::Index)?;
                    }
                } else {
                    self.touch(&ev)?;
                }
                let hr = self.hash_ref(cref, cur, key.0, idx)?;
                return self.guard(TOp::Hload, ety, hr, NOREF, 1);
            }
            // no raw value at this level
            match found {
                Some(idx) => {
                    // present, nil: the typecheck proves it is still nil on the trace
                    let hr = self.hash_ref_k(cref, cur, key.0, idx, true)?;
                    let bit = u32::from(!mutants::active("index_nilnode_skip"));
                    self.guard(TOp::Hload, Ty::Nil, hr, NOREF, bit)?;
                }
                None if dynk => {
                    // DYNMASK: the key is not in the table (as recorded): the nil-miss lookup + a nil type check (a key that appears makes it exit)
                    let hr = self.hash_ref_k(cref, cur, key.0, usize::MAX, true)?;
                    self.guard(TOp::Hload, Ty::Nil, hr, NOREF, 1)?;
                }
                None => self.hash_absent(cref, cur, key)?,
            }
            if TableRef::no_tm(mt, rn_core::meta::Tm::Index) {
                // the end of the chain: `nil`, if no `__index` can appear (flag cache, or no metatable)
                if !mutants::active("chain_end_skip") {
                    self.meta_guard(cref, cur, rn_core::meta::Tm::Index)?;
                }
                return Ok(self.b.ir.konst(KVal::Pri(Ty::Nil)));
            }
            depth += 1;
            if depth > MAX_INDEX_CHAIN {
                return Err(AbortReason::Metamethod);
            }
            let m = mt.ok_or(AbortReason::Diverge)?;
            let name = vm.st.g().heap.tm_name(rn_core::meta::Tm::Index);
            let ix = m.short_str_node(name).ok_or(AbortReason::Metamethod)?;
            if ix >= 1 << 15 {
                return Err(AbortReason::TableHash);
            }
            let iv = m.node_value(ix);
            let Some(next) = iv.as_table() else {
                // a function (or anything else): the interpreter calls it. N3: a Lua closure of the first table's metatable is run in
                // place (`mm_index`; deeper levels go through `finish_get` and `ccall`)
                if self.cfg.mm && (depth == 1 || self.cfg.idxdeep & 1 != 0) {
                    if let Some(cl) = iv.as_lclosure() {
                        let kr = self.mm_str_key(key)?;
                        self.mm_index_fn(vm, cref, m, ix, cl, kr)?;
                        return Ok(NOREF);
                    }
                }
                return Err(AbortReason::Metamethod);
            };
            let mr = self.fload(cref, Field::Metatable, Ty::P64)?;
            let z = self.kint(0);
            if !mutants::active("index_mt_null_skip") {
                self.guard(TOp::Ne, Ty::I64, mr, z, 0)?;
            }
            self.touch(&Value::table(m))?;
            self.touch(&iv)?;
            let kname = self.b.ir.konst(KVal::Ptr(
                payload_bits(&Value::string(name), Ty::ShrStr),
                PtrKind::Raw,
            ));
            let hr = self.hash_ref(mr, m, kname, ix)?;
            let bit = u32::from(!mutants::active("index_tab_typecheck_skip"));
            cref = self.guard(TOp::Hload, Ty::Tab, hr, NOREF, bit)?;
            cur = next;
        }
    }

    /// The proof that the dynamic key `key` (an SSA short string) is absent from the table `t` (`dynmask`): `HREFV` with `HREF_DYN | HREF_ABS`, the walk
    /// of the table as it is when the instruction runs (exits when the key is found: a key inserted by anything between iterations is found).
    fn hash_absent_dyn(&mut self, t: Ref, key: Ref) -> R<()> {
        self.guard(TOp::Hrefv, Ty::P64, t, key, HREF_DYN | HREF_ABS)?;
        Ok(())
    }

    /// The proof that the short-string key is absent from the node vector of `tbl` (`t` its reference): the node-vector
    /// size, the node vector, and one `HMISS` per node of the chain `luaH_getshortstr` walks from the key's main
    /// position (each: not this key, `next` as recorded; the last: `next == 0`). Any insertion that could have put the key
    /// somewhere reachable changes a key or a link on the walk or the size, and exits.
    fn hash_absent(&mut self, t: Ref, tbl: TableRef, key: (Ref, StrRef)) -> R<()> {
        if TraceIr::is_ins(key.0) {
            // N6: the absent-key proof walks the chain of a constant key; a dynamic key takes the run-time walk (`HREFV`
            // with `HREF_ABS`: exits when it finds the key), which needs `dynmask`
            if !self.cfg.dynmask {
                return Err(AbortReason::TableHash);
            }
            return self.hash_absent_dyn(t, key.0);
        }
        let h = self.handle(tbl);
        let mut ix = tbl.short_str_mainpos(key.1);
        if !mutants::active("hmiss_lsize_skip") {
            let ls = self.fload(t, Field::Lsizenode, Ty::U32)?;
            let kl = self.kint(i64::from(tbl.lsizenode()));
            self.guard(TOp::Eq, Ty::I64, ls, kl, 0)?;
        }
        let node = self.fload(t, Field::Node, Ty::P64)?;
        for _ in 0..=MAX_MISS_CHAIN {
            self.fill_node(h, ix);
            let d = tbl.node_next(ix);
            let bits = hmiss_bits(ix, d).ok_or(AbortReason::TableHash)?;
            if !mutants::active("hmiss_skip") {
                self.guard(TOp::Hmiss, Ty::I64, node, key.0, bits)?;
            }
            if d == 0 {
                return Ok(());
            }
            ix = (ix as i64 + i64::from(d)) as usize;
        }
        Err(AbortReason::TableHash)
    }

    /// `t[key] = val` for an existing short-string key whose value is not nil: `HSTORE` (which exits at run time if the
    /// old value is nil: that would be an insertion or a `__newindex`) and the table write barrier for a collectable
    /// value. An absent or nil-valued key is `Insert`.
    fn hash_set(
        &mut self,
        vm: &Vm<'_>,
        tv: &Value,
        t: (Ref, Ty),
        key: (Ref, StrRef),
        val: (Ref, Ty),
    ) -> R<()> {
        if t.1 == Ty::Tab {
            if let Some(tbl) = tv.as_table() {
                if self.cfg.mm && self.mm_newindex_fn(vm, tbl, t, key, val)? {
                    return Ok(());
                }
                if tbl.short_str_node(key.1).is_none() {
                    return self.hash_insert(tbl, t, key, val);
                }
            }
        }
        let (tbl, idx) = self.hash_target(tv, t.1, key.1, self.dyn_key(key.0))?;
        let nil_old = tbl.node_value(idx).tag() & 0x0F == LUA_TNIL;
        if nil_old && !self.nil_store_ok(tbl, key.1, val.1) {
            return Err(AbortReason::Insert);
        }
        let hr = self.hash_ref(t.0, tbl, key.0, idx)?;
        if nil_old {
            // R2: the node is there (its key is alive: `HREFK` checks the tag and address) and holds nil: the interpreter's `luaV_finishset`
            // finds no `__newindex` (the TR14 guard) and `luaH_finishset` stores into the slot, like the fast path of a non-nil slot.
            // The store is unconditional (whatever the old value is at run time, a node that `HREFK` found takes a plain store as long
            // as there is no `__newindex`, which the guard covers for the nil case)
            if !mutants::active("nilstore_newindex_skip") {
                self.meta_guard(t.0, tbl, rn_core::meta::Tm::NewIndex)?;
            }
            self.first_store.get_or_insert(self.b.ir.ins.len());
            // H1: the interpreter's store into a nil-valued node goes through `luaV_finishset`, which keeps the table at `top` (scratch above the
            // frame, not modelled)
            self.stale_above_frame();
            self.ins(TOp::Hstore, Ty::I64, hr, val.0)?;
        } else {
            self.first_store.get_or_insert(self.b.ir.ins.len());
            self.guard(TOp::Hstore, Ty::I64, hr, val.0, 1)?;
        }
        if collectable(val.1) && !mutants::active("hstore_tbar_skip") {
            self.ins(TOp::Tbar, Ty::I64, t.0, val.0)?;
        }
        Ok(())
    }

    /// R2: may a store into an existing hash node that holds nil be recorded (`nilstore`)? The key must not be a metamethod name
    /// (`luaV_finishset` invalidates the table's metamethod-absence cache, a store behind a trace does not: a key that could be an event
    /// name keeps the abort).
    fn nil_store_ok(&self, tbl: TableRef, key: StrRef, val: Ty) -> bool {
        let _ = tbl;
        if !self.cfg.nil_store {
            return false;
        }
        let _ = val;
        let name = key.get().as_bytes();
        !name.starts_with(b"__") || mutants::active("nilstore_meta_name_ok")
    }

    // ---- F3: class-A helpers (TR1) ------------------------------------------------------------------------------------

    /// `NEWTABLE A B C k` (+ `EXTRAARG`): `TNEW` and the guard `NE %tnew, 0` on the instruction's snapshot (the helper
    /// returns 0 having done nothing when the interpreter's `check_gc` after this allocation would step or the
    /// allocation could fail: the exit re-executes the `NEWTABLE` in tier 1).
    fn newtable(&mut self, vm: &Vm<'_>, idx: usize, i: Instruction) -> R<usize> {
        #[cfg(not(rustynail_lua54))]
        let (b, mut c, cmul) = (i.vb(), i.vc(), rn_core::instruction::MAXARG_VC + 1);
        #[cfg(rustynail_lua54)]
        let (b, mut c, cmul) = (i.b(), i.c(), rn_core::instruction::MAXARG_C + 1);
        if i.k() != 0 {
            let x = Instruction(*vm.code.get(idx + 1).ok_or(AbortReason::Diverge)?).ax();
            c = c.wrapping_add(x.wrapping_mul(cmul));
        }
        // the model array is a vector of the true size: bounded like every other model table
        if c as usize > MAX_NEW_ASIZE || b > MAX_NEW_HBITS {
            return Err(self.nyi());
        }
        if self.elems.saturating_add(c as usize) > self.cfg.max_model_elems {
            return Err(AbortReason::Charge);
        }
        self.first_store.get_or_insert(self.b.ir.ins.len());
        let tpos = self.b.ir.ins.len();
        let t = self
            .b
            .emit_flags(TOp::Tnew, Ty::Tab, 0, b as Ref, NOREF, c)?;
        let z = self.kint(0);
        if !mutants::active("tnew_ne_guard_skip") {
            self.alloc_guard(t, z, tpos)?;
        }
        self.post_slot = Some(vm.base + i.a() as usize);
        let a_root = self.frames[self.cur_depth].base_off + i.a() as usize;
        self.def(i.a() as usize, t);
        for slot in a_root + 1..self.sm.len() {
            self.stale.insert(slot);
        }
        Ok(idx + 2)
    }

    /// `SETLIST A B C k` with a fixed count (`B != 0`) into an array part that already has the room: one bounds guard
    /// (`last <= asize`: the interpreter would grow the array otherwise), then the stores, highest index first like
    /// `jit_setlist`, each with the table barrier for a collectable value (5.4: and the `alimit` touch).
    fn setlist(&mut self, vm: &Vm<'_>, idx: usize, i: Instruction) -> R<usize> {
        #[cfg(not(rustynail_lua54))]
        let (n, mut last, cmul) = (
            i.vb() as usize,
            i.vc() as usize,
            rn_core::instruction::MAXARG_VC as usize + 1,
        );
        #[cfg(rustynail_lua54)]
        let (n, mut last, cmul) = (
            i.b() as usize,
            i.c() as usize,
            rn_core::instruction::MAXARG_C as usize + 1,
        );
        if n == 0 {
            // `{f()}`, `{...}`: up to the `top` a producer left. The constructor's array part is sized for the fixed items
            // only, so the stores would be insertions (`Insert`) whatever the count: not recorded (F4 decision).
            return Err(AbortReason::MultRet);
        }
        let mut next = idx + 1;
        if i.k() != 0 {
            last += Instruction(*vm.code.get(idx + 1).ok_or(AbortReason::Diverge)?).ax() as usize
                * cmul;
            next += 1;
        }
        let a = i.a() as usize;
        let (t, tt) = self.rd(vm, a)?;
        let tbl = vm.slot(a).as_table().ok_or(AbortReason::Diverge)?;
        if tt != Ty::Tab {
            return Err(AbortReason::Diverge);
        }
        last += n;
        if last > tbl.array_size() as usize || last > MAX_NEW_ASIZE * 4 {
            return Err(AbortReason::Insert); // the interpreter resizes the array
        }
        let mut vals = Vec::with_capacity(n);
        for j in 1..=n {
            vals.push(self.rd(vm, a + j)?);
        }
        // one guard for all the stores: nothing is stored unless every index is in the array part
        let asize = self.fload(t, Field::Asize, Ty::U32)?;
        let lim = self.kint(last as i64 - 1);
        self.guard(TOp::Ult, Ty::I64, lim, asize, 0)?;
        let arr = self.fload(t, Field::Array, Ty::P64)?;
        self.first_store.get_or_insert(self.b.ir.ins.len());
        for j in (0..n).rev() {
            let u = self.kint((last - n + j) as i64); // 0-based index of `t[last - n + j + 1]`
            #[cfg(rustynail_lua54)]
            if !mutants::active("atouch_skip") {
                self.ins(TOp::Atouch, Ty::Nil, t, u)?;
            }
            let aref = self.ins(TOp::Aref, Ty::P64, arr, u)?;
            self.ins(TOp::Astore, Ty::I64, aref, vals[j].0)?;
            if collectable(vals[j].1) {
                self.ins(TOp::Tbar, Ty::I64, t, vals[j].0)?;
            }
        }
        Ok(next)
    }

    /// A key insertion (`luaH_newkey` / rehash) keeps the table at `top` of the running frame while it allocates: the
    /// slots above the frame are scratch in the VM and not modelled (the IR may hold the dead registers of an earlier
    /// callee there, which matters to the lockstep once a caller with a larger frame is current again: N1 made such
    /// recordings possible).
    fn stale_above_frame(&mut self) {
        let f = &self.frames[self.cur_depth];
        let l = f.base_off + f.nslots;
        for slot in l..self.sm.len() {
            self.stale.insert(slot);
        }
    }

    /// `t[key] = val` for an integer key that is not in the array part (or whose slot there is empty), F3: no `__newindex`
    /// (flag guard), then `TSET` = the table's own `set_int` (array growth or hash insertion with the interpreter's
    /// rehash and accounting), the guard `NE %tset, 0` on the instruction's snapshot (0: the helper did nothing, the
    /// memory limit), and the barrier for a collectable value. A nil value is `Insert`.
    fn int_insert(&mut self, tbl: TableRef, t: (Ref, Ty), key: Ref, val: (Ref, Ty)) -> R<()> {
        if val.1 == Ty::Nil {
            return Err(AbortReason::Insert);
        }
        if !mutants::active("newref_no_newindex_guard") {
            self.meta_guard(t.0, tbl, rn_core::meta::Tm::NewIndex)?;
        }
        self.first_store.get_or_insert(self.b.ir.ins.len());
        self.stale_above_frame();
        let r = self
            .b
            .emit_flags(TOp::Tset, Ty::I64, 0, t.0, key, u32::from(val.0))?;
        self.last_tset = self.b.ir.ins.len();
        if !self.cfg.lockstep {
            self.ls_tabs.push_back(Some(tbl));
        }
        let z = self.kint(0);
        if !mutants::active("newref_ne_guard_skip") {
            self.guard(TOp::Ne, Ty::I64, r, z, 0)?;
        }
        if collectable(val.1) && !mutants::active("hstore_tbar_skip") {
            self.ins(TOp::Tbar, Ty::I64, t.0, val.0)?;
        }
        Ok(())
    }

    /// A1: the inline form of the `NEWREF` of `key` into `tbl`, the table `t` (an IR ref): `Some((aux, image))` when `t` is a `TNEW` of this
    /// trace whose hash part is in the state the recorder tracked since the allocation (nothing but inline insertions touched it, no
    /// `TSET` since, so every run of the trace reaches the same node vector), `aux` = the [`nrplan`] of the insertion and `image` the
    /// hash image the VM's insertion must leave (checked in `sync_newref`).
    #[cold]
    #[inline(never)]
    fn nr_plan(&mut self, t: Ref, tbl: TableRef, key: StrRef) -> Option<(u32, NrImage)> {
        if !self.cfg.nrinline || !TraceIr::is_ins(t) || self.b.ir.at(t).op != TOp::Tnew {
            return None;
        }
        let pos = (t - REF_BIAS) as usize;
        if pos < self.last_tset || tbl.lsizenode() > 3 {
            return None; // (more than 8 nodes: the image of a big table is not worth building at every insertion)
        }
        let cur = tbl.hash_image();
        let tracked = match self.nr_img.get(&t) {
            Some(Some(img)) => img == &cur,
            Some(None) => false,
            None => {
                let n = cur.0.len();
                n > 0
                    && cur
                        .0
                        .iter()
                        .all(|&(kt, kp, nx, ve)| kt == LUA_TNIL && kp == 0 && nx == 0 && ve)
                    && cur.1
                        == if cfg!(rustynail_lua54) || n >= 8 {
                            n
                        } else {
                            0
                        }
            }
        };
        if !tracked {
            return None;
        }
        let plan = tbl.plan_shortstr_insert(key)?;
        let aux = nrplan::encode(&plan)?;
        let mut img = cur;
        plan.apply(&mut img, key.as_ptr() as usize as u64);
        Some((aux, img))
    }

    /// `t[key] = val` for a short-string key that is **absent** from the hash part of `tbl` (F3): the metatable has no
    /// `__newindex` (TR14 flag guard), `NEWREF` inserts the key through the table's own `set` (class A: 0 and no effect
    /// when the key is present after all or the insertion cannot be done without an error), the guard `NE %newref, 0`
    /// takes the instruction's snapshot, and the value is stored with an unguarded `HSTORE` into the new node
    /// (+ `TBAR`). A nil value is `Insert` (nothing would be inserted).
    fn hash_insert(
        &mut self,
        tbl: TableRef,
        t: (Ref, Ty),
        key: (Ref, StrRef),
        val: (Ref, Ty),
    ) -> R<()> {
        if val.1 == Ty::Nil {
            if !self.cfg.nil_store {
                return Err(AbortReason::Insert);
            }
            // R2: `t.k = nil` for an absent key inserts nothing (`luaH_newkey` ignores a nil value): with no `__newindex` (TR14 guard) and
            // the proof that the key is absent, the instruction has no effect the trace has to reproduce
            self.meta_guard(t.0, tbl, rn_core::meta::Tm::NewIndex)?;
            return self.hash_absent(t.0, tbl, key);
        }
        // `__newindex` absent: no metatable, or a cached "absent" bit (the key is absent, so the table has no raw value)
        if !mutants::active("newref_no_newindex_guard") {
            self.meta_guard(t.0, tbl, rn_core::meta::Tm::NewIndex)?;
        }
        self.first_store.get_or_insert(self.b.ir.ins.len());
        self.stale_above_frame();
        // (an inline insertion plan fixes the node the recorded key lands on: not for a dynamic key)
        let plan = if TraceIr::is_ins(key.0) {
            None
        } else {
            self.nr_plan(t.0, tbl, key.1)
        };
        let n = match plan {
            Some((aux, _)) => self.b.emit(TOp::Newref, Ty::P64, t.0, key.0, aux)?,
            None => self.ins(TOp::Newref, Ty::P64, t.0, key.0)?,
        };
        let z = self.kint(0);
        if !mutants::active("newref_ne_guard_skip") {
            self.guard(TOp::Ne, Ty::I64, n, z, 0)?;
        }
        self.newref_keys.push_back(key.1);
        if !self.cfg.lockstep {
            self.ls_tabs.push_back(Some(tbl));
        }
        // BUNDLE-M C1: the hash image the inline insertion leaves is known from the plan (the lockstep's `sync_newref` confirms it against
        // the VM's table when it runs); the next `nr_plan` of this table compares it with the table itself either way
        match &plan {
            _ if mutants::active("nr_img_record_skip") => {}
            Some((_, img)) => {
                self.nr_img.insert(t.0, Some(img.clone()));
            }
            None => {
                if TraceIr::is_ins(t.0) {
                    self.nr_img.insert(t.0, None);
                }
            }
        }
        self.newref_exp.push_back(plan.map(|(_, img)| (t.0, img)));
        self.ins(TOp::Hstore, Ty::I64, n, val.0)?;
        if collectable(val.1) && !mutants::active("hstore_tbar_skip") {
            self.ins(TOp::Tbar, Ty::I64, t.0, val.0)?;
        }
        Ok(())
    }

    /// The table of `GETTABUP` / `SETTABUP`: upvalue `n` (its `ULOAD`, typed by the value) and the value itself.
    fn upval_table(&mut self, vm: &Vm<'_>, n: usize) -> R<((Ref, Ty), Value)> {
        self.uload(vm, n)
    }

    /// The value operand `RK(C)` of a store.
    fn rk_value(&mut self, vm: &Vm<'_>, i: Instruction) -> R<(Ref, Ty)> {
        if i.k() != 0 {
            let (r, ty, _) = self.kconst(vm, i.c() as usize)?;
            Ok((r, ty))
        } else {
            self.rd(vm, i.c() as usize)
        }
    }

    /// The access path of upvalue `n` of the current frame's closure (`UREF` with the TR15 range guard, or N1: the
    /// root-frame slot the open upvalue points at).
    ///
    /// The range guard covers the **root frame** (`16 * nslots` bytes): an open upvalue points into an *active* frame
    /// of the thread, the frames above the root are created by the trace itself (no `CLOSURE` is recorded, so none can
    /// be the target of an upvalue), and anything below the root base is outside by the unsigned compare.
    ///
    /// N1: an open upvalue that points at slot `s` of the **root frame** (a closure called from the loop captures a
    /// local of the loop's own frame) is the same storage as register `s`, which the trace keeps as an SSA value
    /// (`SlotMap`) and writes back only in snapshots. `ULOAD` / `USTORE` of it would read / write the stack memory behind
    /// the trace's back, so the access is recorded **as the register access** (`rd_slot` / `def_abs`) and the identity
    /// of the pair is guarded: `EQ UVREL(uref), 16 * s` (the closure's upvalue `n` is the one at slot `s`). Any other
    /// closure of the proto (an upvalue elsewhere, or at another slot) fails that guard, or the `UGE` range guard of the
    /// ordinary accesses, and leaves the trace through the snapshot of the instruction. An address inside the frames
    /// above the root's `nslots` (a callee's registers) still aborts `UpvalAlias`.
    ///
    /// Aliasing (P3d): two `(closure, index)` pairs may name one real `UpVal` (closures share upvalues). The IR
    /// addresses upvalues only as `(closure, index)` pairs and `ULOAD` / `USTORE` are never forwarded, CSE'd or
    /// reordered by the recorder, so the trace is memory-correct as emitted; the lockstep / `interp` model keeps one
    /// cell per REAL `UpVal` (`Model::uv_cell`), so a store through one pair is seen through the other. Optimisers
    /// must not treat distinct `(cref, idx)` pairs as disjoint (docs/JIT_TRACE_REF.md, P3d decisions).
    fn uref(&mut self, vm: &Vm<'_>, n: usize) -> R<UvAcc> {
        if n >= vm.cl.nupvalues() {
            return Err(AbortReason::Diverge);
        }
        let at = vm.cl.upval(n).value_ptr() as usize;
        let lo = vm.st.slot_ptr(StkId(vm.root_base)) as usize;
        let sz = std::mem::size_of::<Value>();
        let mut slot = None;
        if at >= lo && at < lo + self.ext.max(self.nslots) * sz {
            // the real upvalue points into the frames of the trace (TR15)
            let off = at - lo;
            if off >= self.nslots * sz || off % sz != 0 {
                return Err(AbortReason::UpvalAlias);
            }
            slot = Some(off / sz);
        }
        let cref = if self.cur_depth > 0 {
            self.frames[self.cur_depth].cref
        } else {
            if self.func_ref == NOREF {
                self.func_ref = self.b.emit_raw(Ins {
                    op: TOp::Sload,
                    t: Ty::Lcf,
                    flags: 0,
                    a: FUNC_SLOT,
                    b: 0,
                    prev: NOREF,
                    aux: 0,
                })?;
            }
            self.func_ref
        };
        let kn = self.kint(n as i64);
        // P2 bundle 2: the identity of the real `UpVal` (0 = unknown): two `UREF`s of different identities were different upvalues when recorded
        let uvid = if self.cfg.uvsep {
            let key = vm.cl.upval(n).as_ptr() as usize;
            let next = self.uv_ids.len() as u32 + 1;
            *self.uv_ids.entry(key).or_insert(next)
        } else {
            0
        };
        let u = self.b.emit(TOp::Uref, Ty::P64, cref, kn, uvid)?;
        if let Some(s) = slot {
            let rel = self.ins(TOp::Uvrel, Ty::I64, u, NOREF)?;
            let want = s + usize::from(mutants::active("uvslot_off_by_one"));
            let k = self.kint((want * sz) as i64);
            if !mutants::active("uvslot_guard_skip") {
                self.guard(TOp::Eq, Ty::I64, rel, k, 0)?;
            }
            if mutants::active("uvslot_as_cell") {
                // mutant: the access stays a memory access behind the SSA slot values
                return Ok(UvAcc::Cell(u));
            }
            return Ok(UvAcc::Slot(s));
        }
        if !mutants::active("upval_range_skip") {
            // `UVREL` is the value address relative to the root base; the guard asserts it is outside the frame
            let rel = self.ins(TOp::Uvrel, Ty::I64, u, NOREF)?;
            let lim = self.kint((self.nslots * std::mem::size_of::<Value>()) as i64);
            self.guard(TOp::Uge, Ty::I64, rel, lim, 0)?;
        }
        Ok(UvAcc::Cell(u))
    }

    /// Whether upvalue `n` of the current closure is an open upvalue at a slot of the root frame (N1; no emission).
    fn uv_is_slot(&self, vm: &Vm<'_>, n: usize) -> bool {
        n < vm.cl.nupvalues() && self.in_root_frame(vm.cl.upval(n).value_ptr() as usize)
    }

    /// Whether `at` is the address of a slot of the root frame (`0 .. nslots`).
    fn in_root_frame(&self, at: usize) -> bool {
        at >= self.root_ptr && at < self.root_ptr + self.nslots * std::mem::size_of::<Value>()
    }

    /// `GETUPVAL`-style read of upvalue `n`: the value ref and type (typed by the VM's current value for a cell; a slot
    /// alias reads the register).
    fn uload(&mut self, vm: &Vm<'_>, n: usize) -> R<((Ref, Ty), Value)> {
        let acc = self.uref(vm, n)?;
        let v = vm.cl.upval(n).get();
        match acc {
            UvAcc::Slot(s) => Ok((self.rd_slot(vm, s)?, v)),
            UvAcc::Cell(u) => {
                let ty = ty_of_value(&v).ok_or_else(|| self.nyi())?;
                self.touch(&v)?;
                let r = self.guard(TOp::Uload, ty, u, NOREF, 1)?;
                Ok(((r, ty), v))
            }
        }
    }

    /// R2: a side recording of a function-entry family with native recursion (`recany`) crosses the back edge of a `for` loop of the root
    /// frame to an untraced head (up to `loop_unroll` per recording) instead of aborting `SideUntraced`.
    fn side_unroll(&self) -> bool {
        self.cfg.side.is_some() && self.cfg.rec_any && self.cfg.rec_conts && self.cfg.fn_root
    }

    /// R2: the recording is a continuation root of the root frame (depth 0) that may cross the back edge of a `for` loop of that frame.
    fn cont_unroll(&self) -> bool {
        self.cfg.rec_cont && self.cfg.rec_any && self.cfg.side.is_none()
    }

    /// May the callee loop at head `t` of the current frame be unrolled (false: its site is blocked, see [`la_block`])? Notes the site for the
    /// trace this recording becomes (`last_la_sites`).
    fn loop_any_charge(&self, t: usize) -> bool {
        let site = (self.frames[self.cur_depth].proto as usize, t);
        if self.la_seen.borrow().contains(&site) {
            return true;
        }
        if LOOP_ANY.with(|m| m.borrow().contains(&site)) {
            return false;
        }
        self.la_seen.borrow_mut().push(site);
        true
    }

    /// Classifies a taken branch to `target` from `idx`: backward edges close the loop only at `H` of the root frame;
    /// inside an inlined frame every backward edge is an inner loop.
    fn target(&self, idx: usize, target: isize) -> R<usize> {
        if target < 0 || target as usize >= self.cur_len {
            return Err(AbortReason::Diverge);
        }
        let t = target as usize;
        if t <= idx {
            if self.cur_depth > 0 {
                // P-E: a loop of an inlined callee is unrolled (each iteration recorded) up to `loop_unroll` backward edges
                // per recording; beyond that it is the inner loop the prototype always aborted on
                // (only a counted `for` whose `FORPREP` was recorded with constant bounds: see `forprep`)
                let gfor_only = self.cfg.gfor_inl
                    && (self.cur_op == OpCode::Tforcall as u8
                        || self.cur_op == OpCode::Tforprep as u8);
                // M1: any other backward edge (a `while` / `repeat` of the callee: its exit is a guarded branch), within the per-site budget
                let any_edge = self.cur_op != OpCode::Forloop as u8 && !gfor_only;
                let gfor_back =
                    gfor_only || (self.cfg.loop_any && (!any_edge || self.loop_any_charge(t)));
                if self.cfg.loop_unroll == 0
                    || (self.cur_op != OpCode::Forloop as u8 && !gfor_back)
                    || self.inl_back.get() >= self.cfg.loop_unroll
                {
                    return Err(AbortReason::InnerLoop);
                }
                self.inl_back.set(self.inl_back.get() + 1);
                return Ok(t);
            }
            if self.cont_unroll() {
                // R2: a continuation root crosses the back edge of a loop of the root frame (`for` loops: the next iteration is
                // recorded) up to `loop_unroll` per recording, like the loop of an inlined callee
                if self.cur_op != OpCode::Forloop as u8
                    || self.inl_back.get() >= self.cfg.loop_unroll
                {
                    return Err(AbortReason::InnerLoop);
                }
                self.inl_back.set(self.inl_back.get() + 1);
                return Ok(t);
            }
            if self.side_unroll() && self.cur_op == OpCode::Forloop as u8 {
                if self.inl_back.get() >= self.cfg.loop_unroll {
                    return Err(AbortReason::InnerLoop);
                }
                self.inl_back.set(self.inl_back.get() + 1);
                return Ok(t);
            }
            if self.cfg.side.is_some() || (self.cfg.nest.is_some() && t as u32 != self.head) {
                // F2a: the next step decides (a head with a trace links, anything else aborts)
                return Ok(t);
            }
            if t as u32 != self.head {
                return Err(if t as u32 > self.head {
                    AbortReason::InnerLoop
                } else {
                    AbortReason::LeaveLoop
                });
            }
        }
        Ok(t)
    }

    /// The conditional-jump follower: where control goes after a compare / test with outcome `cond` and flag `k`.
    fn cond_next(&self, vm: &Vm<'_>, idx: usize, i: Instruction, cond: bool) -> R<usize> {
        if (u32::from(cond)) != i.k() {
            return Ok(idx + 2);
        }
        let ni = Instruction(*vm.code.get(idx + 1).ok_or(AbortReason::Diverge)?);
        if ni.opcode_raw() != OpCode::Jmp as u8 {
            return Err(AbortReason::Diverge);
        }
        self.target(idx, idx as isize + 2 + ni.sj() as isize)
    }
}

/// How an upvalue is accessed (see [`Rec::uref`]).
#[derive(Clone, Copy)]
enum UvAcc {
    /// The `UREF` of the `(closure, index)` pair: `ULOAD` / `USTORE`.
    Cell(Ref),
    /// N1: the root-frame slot the open upvalue points at.
    Slot(usize),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Ar {
    Add,
    Sub,
    Mul,
    Div,
    Pow,
    Idiv,
    Mod,
    Band,
    Bor,
    Bxor,
    Shl,
    Shr,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Cmp {
    Eq,
    Lt,
    Le,
    Gt,
    Ge,
}

impl Rec {
    /// Records the instruction at `idx` (the VM has not executed it yet) and returns the index of its successor.
    fn record_one(&mut self, vm: &Vm<'_>, idx: usize) -> R<usize> {
        let i = Instruction(vm.code[idx]);
        self.cur_op = i.opcode_raw();
        let op = i.opcode().ok_or(AbortReason::Nyi(i.opcode_raw()))?;
        let a = i.a() as usize;
        let b = i.b() as usize;
        let next = idx + 1;
        use OpCode as O;
        // N3: an arithmetic instruction with a table operand is followed (when it fails) by the `MMBIN*` that calls the metamethod
        if self.cfg.mm
            && matches!(
                op,
                O::Add
                    | O::Sub
                    | O::Mul
                    | O::Mod
                    | O::Idiv
                    | O::Div
                    | O::Pow
                    | O::Band
                    | O::Bor
                    | O::Bxor
                    | O::Shl
                    | O::Shr
                    | O::Addk
                    | O::Subk
                    | O::Mulk
                    | O::Modk
                    | O::Idivk
                    | O::Divk
                    | O::Powk
                    | O::Bandk
                    | O::Bork
                    | O::Bxork
                    | O::Addi
                    | O::Shli
                    | O::Shri
            )
            && Self::mm_pair_has_table(vm, idx)
        {
            return self.mm_arith(vm, idx);
        }
        match op {
            O::Mmbin | O::Mmbini | O::Mmbink => {
                let c = self.mm_bridge.take().ok_or(AbortReason::Diverge)?;
                let func_local = self.frames[self.cur_depth].nslots;
                self.mm_push(vm, idx, func_local, c, 1)
            }
            O::Move => {
                let (r, _) = self.rd(vm, b)?;
                self.def(a, r);
                Ok(next)
            }
            O::Loadi => {
                let r = self.kint(i64::from(i.sbx()));
                self.def(a, r);
                Ok(next)
            }
            O::Loadf => {
                let r = self.b.ir.knum(f64::from(i.sbx()));
                self.def(a, r);
                Ok(next)
            }
            O::Loadk => {
                let (r, _, _) = self.kconst(vm, i.bx() as usize)?;
                self.def(a, r);
                Ok(next)
            }
            O::Loadfalse | O::Lfalseskip | O::Loadtrue => {
                let t = if op == O::Loadtrue {
                    Ty::True
                } else {
                    Ty::False
                };
                let r = self.b.ir.konst(KVal::Pri(t));
                self.def(a, r);
                Ok(if op == O::Lfalseskip { idx + 2 } else { next })
            }
            O::Loadnil => {
                let r = self.b.ir.konst(KVal::Pri(Ty::Nil));
                for n in 0..=b {
                    if a + n >= self.frames[self.cur_depth].nslots {
                        return Err(AbortReason::Diverge);
                    }
                    self.def(a + n, r);
                }
                Ok(next)
            }
            O::Getupval => {
                let (r, _) = self.uload(vm, b)?;
                self.def(a, r.0);
                Ok(next)
            }
            O::Setupval => {
                let (v, t) = self.rd(vm, a)?;
                // BUNDLE-V V2 (`upbar`): the upvalue barrier of a collectable value stored into a real upvalue cell is `TBAR(uref, v)` with `aux` 1 (below)
                if collectable(t) && !self.uv_is_slot(vm, b) && self.cfg.upbar == 0 {
                    return Err(self.nyi()); // needs the upvalue barrier (TR10)
                }
                let acc = self.uref(vm, b)?;
                match acc {
                    UvAcc::Slot(s) => {
                        // N1: the register itself (no upvalue barrier: a stack slot)
                        if !mutants::active("uvslot_store_skip") {
                            self.def_abs(s, v);
                        }
                    }
                    UvAcc::Cell(u) => {
                        // N3a (R4): an open upvalue below the root frame is a slot of a caller
                        if self.cfg.retf & 1 != 0
                            && vm.cl.upval(b).is_open()
                            && (vm.cl.upval(b).value_ptr() as usize) < self.root_ptr
                        {
                            let at = vm.cl.upval(b).value_ptr() as usize;
                            // (an open upvalue of another coroutine's stack lies elsewhere: its index is out of range of every caller)
                            if let Some(d) = at.checked_sub(vm.st.slot_ptr(StkId(0)) as usize) {
                                let idx = d / std::mem::size_of::<Value>();
                                if !self.uv_below_stores.contains(&idx) {
                                    self.uv_below_stores.push(idx);
                                }
                            }
                        }
                        self.first_store.get_or_insert(self.b.ir.ins.len());
                        self.ins(TOp::Ustore, Ty::I64, u, v)?;
                        if collectable(t) && self.cfg.upbar != 0 && !mutants::active("upbar_skip") {
                            // (`luaC_barrier(uv, v)`: forward barrier on the UpVal object when it is black and the value white; `aux` 1 = upvalue)
                            self.b.emit(TOp::Tbar, Ty::I64, u, v, 1)?;
                        }
                        self.ups_dirty = true;
                    }
                }
                Ok(next)
            }
            O::Geti => {
                let t = self.rd(vm, b)?;
                let key = i64::from(i.c());
                let kr = self.kint(key);
                let r = self.array_get(vm, t, b, kr, key)?;
                if r == NOREF {
                    return self.mm_push_pending(vm, idx, a, true);
                }
                self.def(a, r);
                Ok(next)
            }
            O::Gettable => {
                let t = self.rd(vm, b)?;
                let c = i.c() as usize;
                let (kref, kty) = self.rd(vm, c)?;
                let r = if kty == Ty::ShrStr {
                    let key = self.reg_key_any(vm, c, kref, &vm.slot(b), t.1)?;
                    self.hash_get(vm, &vm.slot(b), t, key)?
                } else {
                    let (kr, key) = self.int_key(vm, c)?;
                    self.array_get_k(vm, t, b, kr, key, kref)?
                };
                if r == NOREF {
                    return self.mm_push_pending(vm, idx, a, true);
                }
                self.def(a, r);
                Ok(next)
            }
            O::Seti | O::Settable => {
                let t = self.rd(vm, a)?;
                if op == O::Settable && self.rd(vm, b)?.1 == Ty::ShrStr {
                    let (kref, _) = self.rd(vm, b)?;
                    let key = self.reg_key_any(vm, b, kref, &vm.slot(a), t.1)?;
                    let val = self.rk_value(vm, i)?;
                    self.hash_set(vm, &vm.slot(a), t, key, val)?;
                    if self.mm_pending.is_some() {
                        return self.mm_push_pending(vm, idx, a, false);
                    }
                    return Ok(next);
                }
                let (kr, key) = if op == O::Seti {
                    (self.kint(i64::from(i.b())), i64::from(i.b()))
                } else {
                    self.int_key(vm, b)?
                };
                let val = self.rk_value(vm, i)?;
                self.array_set(vm, t, a, kr, key, val)?;
                Ok(next)
            }
            O::Getfield => {
                let t = self.rd(vm, b)?;
                let key = self.field_key(vm, i.c() as usize)?;
                let r = self.hash_get(vm, &vm.slot(b), t, key)?;
                if r == NOREF {
                    return self.mm_push_pending(vm, idx, a, true);
                }
                self.def(a, r);
                Ok(next)
            }
            O::Gettabup => {
                let (t, tv) = self.upval_table(vm, b)?;
                let key = self.field_key(vm, i.c() as usize)?;
                let r = self.hash_get(vm, &tv, t, key)?;
                if r == NOREF {
                    return self.mm_push_pending(vm, idx, a, true);
                }
                self.def(a, r);
                Ok(next)
            }
            O::SelfOp => {
                let t = self.rd(vm, b)?;
                let c = i.c() as usize;
                // 5.5: the key is `K[C]`; 5.4: `RK(C)`, a register when `k` is clear
                let key = if !cfg!(rustynail_lua54) || i.k() != 0 {
                    self.field_key(vm, c)?
                } else {
                    let (kref, kty) = self.rd(vm, c)?;
                    if kty != Ty::ShrStr {
                        return Err(AbortReason::Metamethod);
                    }
                    self.reg_key(vm, c, kref)?
                };
                if a + 1 >= self.frames[self.cur_depth].nslots {
                    return Err(AbortReason::Diverge);
                }
                if matches!(t.1, Ty::ShrStr | Ty::LngStr) {
                    // P-V: `s:method`: a guarded constant resolution through the string metatable (no helper call)
                    let r = self.string_method(vm, key)?;
                    self.def(a + 1, t.0);
                    self.def(a, r);
                    return Ok(next);
                }
                let r = self.hash_get(vm, &vm.slot(b), t, key)?;
                self.def(a + 1, t.0);
                if r == NOREF {
                    return self.mm_push_pending(vm, idx, a, true);
                }
                self.def(a, r);
                Ok(next)
            }
            O::Setfield => {
                let t = self.rd(vm, a)?;
                let key = self.field_key(vm, b)?;
                let val = self.rk_value(vm, i)?;
                self.hash_set(vm, &vm.slot(a), t, key, val)?;
                if self.mm_pending.is_some() {
                    return self.mm_push_pending(vm, idx, a, false);
                }
                Ok(next)
            }
            O::Settabup => {
                let (t, tv) = self.upval_table(vm, a)?;
                let key = self.field_key(vm, b)?;
                let val = self.rk_value(vm, i)?;
                self.hash_set(vm, &tv, t, key, val)?;
                if self.mm_pending.is_some() {
                    return self.mm_push_pending(vm, idx, a, false);
                }
                Ok(next)
            }
            // ---- arithmetic ----
            O::Add
            | O::Sub
            | O::Mul
            | O::Mod
            | O::Idiv
            | O::Div
            | O::Pow
            | O::Band
            | O::Bor
            | O::Bxor
            | O::Shl
            | O::Shr => {
                let w = match op {
                    O::Add => Ar::Add,
                    O::Sub => Ar::Sub,
                    O::Mul => Ar::Mul,
                    O::Mod => Ar::Mod,
                    O::Idiv => Ar::Idiv,
                    O::Div => Ar::Div,
                    O::Pow => Ar::Pow,
                    O::Band => Ar::Band,
                    O::Bor => Ar::Bor,
                    O::Bxor => Ar::Bxor,
                    O::Shl => Ar::Shl,
                    _ => Ar::Shr,
                };
                let x = self.rd(vm, b)?;
                let y = self.rd(vm, i.c() as usize)?;
                let (vx, vy) = (vm.slot(b), vm.slot(i.c() as usize));
                let (r, _) = self.arith(w, x, y, &vx, &vy)?;
                self.def(a, r);
                Ok(idx + 2)
            }
            O::Addk
            | O::Subk
            | O::Mulk
            | O::Modk
            | O::Idivk
            | O::Divk
            | O::Powk
            | O::Bandk
            | O::Bork
            | O::Bxork => {
                let w = match op {
                    O::Addk => Ar::Add,
                    O::Subk => Ar::Sub,
                    O::Mulk => Ar::Mul,
                    O::Modk => Ar::Mod,
                    O::Idivk => Ar::Idiv,
                    O::Divk => Ar::Div,
                    O::Powk => Ar::Pow,
                    O::Bandk => Ar::Band,
                    O::Bork => Ar::Bor,
                    _ => Ar::Bxor,
                };
                let x = self.rd(vm, b)?;
                let vx = vm.slot(b);
                let (kr, kt, vy) = self.kconst(vm, i.c() as usize)?;
                let (r, _) = self.arith(w, x, (kr, kt), &vx, &vy)?;
                self.def(a, r);
                Ok(idx + 2)
            }
            O::Addi => {
                let x = self.rd(vm, b)?;
                let vx = vm.slot(b);
                let (kr, kt, vy) = self.imm(i64::from(i.sc()));
                let (r, _) = self.arith(Ar::Add, x, (kr, kt), &vx, &vy)?;
                self.def(a, r);
                Ok(idx + 2)
            }
            O::Shli => {
                // R[A] := sC << R[B]
                let (kr, kt, vx) = self.imm(i64::from(i.sc()));
                let y = self.rd(vm, b)?;
                let vy = vm.slot(b);
                let (r, _) = self.arith(Ar::Shl, (kr, kt), y, &vx, &vy)?;
                self.def(a, r);
                Ok(idx + 2)
            }
            O::Shri => {
                // R[A] := R[B] >> sC
                let x = self.rd(vm, b)?;
                let vx = vm.slot(b);
                let (kr, kt, vy) = self.imm(i64::from(i.sc()));
                let (r, _) = self.arith(Ar::Shr, x, (kr, kt), &vx, &vy)?;
                self.def(a, r);
                Ok(idx + 2)
            }
            O::Unm => {
                let (x, t) = self.rd(vm, b)?;
                if self.cfg.mm && t == Ty::Tab {
                    return self.mm_unary(vm, idx, Tm::Unm, x, a, b);
                }
                let r = match t {
                    Ty::Int => self.ins(TOp::Neg, Ty::Int, x, NOREF)?,
                    Ty::Num => self.ins(TOp::Fneg, Ty::Num, x, NOREF)?,
                    _ => return Err(self.non_number(t, Ty::Int)),
                };
                self.def(a, r);
                Ok(next)
            }
            O::Bnot => {
                let (x, t) = self.rd(vm, b)?;
                if self.cfg.mm && t == Ty::Tab {
                    return self.mm_unary(vm, idx, Tm::BNot, x, a, b);
                }
                if !matches!(t, Ty::Int | Ty::Num) {
                    return Err(self.non_number(t, Ty::Int));
                }
                let xi = self.int_of(x, t, &vm.slot(b))?;
                let r = self.ins(TOp::Bnot, Ty::Int, xi, NOREF)?;
                self.def(a, r);
                Ok(next)
            }
            O::Not => {
                let (_, t) = self.rd(vm, b)?;
                let r = self
                    .b
                    .ir
                    .konst(KVal::Pri(if falsy(t) { Ty::True } else { Ty::False }));
                self.def(a, r);
                Ok(next)
            }
            // ---- control ----
            O::Jmp => self.target(idx, idx as isize + 1 + i.sj() as isize),
            O::Eq | O::Lt | O::Le => {
                let x = self.rd(vm, a)?;
                let y = self.rd(vm, b)?;
                if self.cfg.mm {
                    if op == O::Eq && x.1 == Ty::Tab && y.1 == Ty::Tab {
                        return self.mm_eq(vm, idx, i, x, y);
                    }
                    if op != O::Eq && (x.1 == Ty::Tab || y.1 == Ty::Tab) {
                        let ev = if op == O::Lt { Tm::Lt } else { Tm::Le };
                        let (o1, o2) = (
                            MmOpd {
                                r: x.0,
                                t: x.1,
                                v: vm.slot(a),
                            },
                            MmOpd {
                                r: y.0,
                                t: y.1,
                                v: vm.slot(b),
                            },
                        );
                        return self.mm_order(vm, idx, i, ev, o1, o2);
                    }
                }
                let c = match op {
                    O::Eq => Cmp::Eq,
                    O::Lt => Cmp::Lt,
                    _ => Cmp::Le,
                };
                let cond = self.compare(c, x, y, &vm.slot(a), &vm.slot(b))?;
                self.cond_next(vm, idx, i, cond)
            }
            O::Eqk => {
                let x = self.rd(vm, a)?;
                let (kr, kt, vk) = self.kconst(vm, b)?;
                let cond = self.compare(Cmp::Eq, x, (kr, kt), &vm.slot(a), &vk)?;
                self.cond_next(vm, idx, i, cond)
            }
            O::Eqi | O::Lti | O::Lei | O::Gti | O::Gei => {
                let x = self.rd(vm, a)?;
                let vx = vm.slot(a);
                let im = i64::from(i.sb());
                if self.cfg.mm && op != O::Eqi && x.1 == Ty::Tab {
                    // `order_i!`: the immediate is an integer, a float when `C` is set; `>` / `>=` swap the operands
                    let (ir, it, iv) = if i.c() != 0 {
                        (self.b.ir.knum(im as f64), Ty::Num, Value::float(im as f64))
                    } else {
                        self.imm(im)
                    };
                    let (ev, inv) = match op {
                        O::Lti => (Tm::Lt, false),
                        O::Lei => (Tm::Le, false),
                        O::Gti => (Tm::Lt, true),
                        _ => (Tm::Le, true),
                    };
                    let (o, k) = (
                        MmOpd {
                            r: x.0,
                            t: x.1,
                            v: vx,
                        },
                        MmOpd {
                            r: ir,
                            t: it,
                            v: iv,
                        },
                    );
                    let (o1, o2) = if inv { (k, o) } else { (o, k) };
                    return self.mm_order(vm, idx, i, ev, o1, o2);
                }
                let c = match op {
                    O::Eqi => Cmp::Eq,
                    O::Lti => Cmp::Lt,
                    O::Lei => Cmp::Le,
                    O::Gti => Cmp::Gt,
                    _ => Cmp::Ge,
                };
                // the immediate has the type of the register's number (float compares use `im as f64`)
                let (yr, yt, vy) = match x.1 {
                    Ty::Num => (self.b.ir.knum(im as f64), Ty::Num, Value::float(im as f64)),
                    _ => self.imm(im),
                };
                let cond = self.compare(c, x, (yr, yt), &vx, &vy)?;
                self.cond_next(vm, idx, i, cond)
            }
            O::Test => {
                let (_, t) = self.rd(vm, a)?;
                self.cond_next(vm, idx, i, !falsy(t))
            }
            O::Testset => {
                let (r, t) = self.rd(vm, b)?;
                if falsy(t) == (i.k() != 0) {
                    Ok(idx + 2)
                } else {
                    self.def(a, r);
                    let ni = Instruction(*vm.code.get(idx + 1).ok_or(AbortReason::Diverge)?);
                    if ni.opcode_raw() != O::Jmp as u8 {
                        return Err(AbortReason::Diverge);
                    }
                    self.target(idx, idx as isize + 2 + ni.sj() as isize)
                }
            }
            // ---- F3: class-A helpers (TR1) ----
            O::Newtable => self.newtable(vm, idx, i),
            O::Setlist => self.setlist(vm, idx, i),
            O::Len => {
                let (r, t) = self.rd(vm, b)?;
                let v = vm.slot(b);
                let res = match t {
                    Ty::Tab => {
                        let tbl = v.as_table().ok_or(AbortReason::Diverge)?;
                        if self.cfg.mm
                            && !TableRef::no_tm(tbl.metatable(), Tm::Len)
                            && tbl.metatable().is_some()
                        {
                            return self.mm_unary(vm, idx, Tm::Len, r, a, b);
                        }
                        if !mutants::active("tlen_meta_skip") {
                            self.meta_guard(r, tbl, rn_core::meta::Tm::Len)?;
                        }
                        self.first_store.get_or_insert(self.b.ir.ins.len());
                        self.b.emit_flags(TOp::Tlen, Ty::Int, 0, r, NOREF, 0)?
                    }
                    Ty::ShrStr | Ty::LngStr => self.b.emit_flags(
                        TOp::Tlen,
                        Ty::Int,
                        0,
                        r,
                        NOREF,
                        if t == Ty::ShrStr { 1 } else { 2 },
                    )?,
                    _ => return Err(AbortReason::Metamethod), // `__len` of another type, or an error
                };
                self.post_slot = Some(vm.base + a);
                self.def(a, res);
                Ok(next)
            }
            O::Forloop => self.forloop(vm, idx, i),
            // N7: the `CLOSE` right behind the `TFORLOOP` of a generic `for` (the compiler marks the loop's block as having
            // upvalues because of the closing value): everything at or above its register is the loop state (hidden variables,
            // which no closure can capture) and the loop variables, whose captures the body's own `CLOSE` ended with every
            // iteration; recorded as a no-op when the interpreter has nothing to close either (not guardable: no IR for it)
            O::Close
                if self.cfg.gfor
                    && idx > 0
                    && Instruction(vm.code[idx - 1]).opcode_raw() == OpCode::Tforloop as u8
                    && Instruction(vm.code[idx - 1]).a() == i.a()
                    && vm.st.jit_close_is_noop(StkId(vm.base + i.a() as usize)) =>
            {
                Ok(idx + 1)
            }
            O::Tforcall => self.tforcall(vm, idx, i, false),
            O::Tforloop if self.cfg.gfor_lua => self.tforloop(vm, idx, i),
            O::Tforprep
                if self.cfg.gfor
                    && self.cur_depth == 0
                    && (self.cfg.side.is_some() || self.cfg.nest.is_some()) =>
            {
                self.tforprep(vm, idx, i)
            }
            // M1: the `ipairs` / `pairs` loop of an inlined callee is unrolled like its numeric loops (`loop_unroll` backward edges
            // per recording; the end of the traversal is a guarded fall-through)
            O::Tforprep if self.cfg.gfor && self.cfg.gfor_inl && self.unrolling() => {
                self.tforprep(vm, idx, i)
            }
            O::Concat => self.concat(vm, idx, i),
            O::Forprep
                if (self.cur_depth == 0
                    && (self.cfg.side.is_some() || self.cfg.nest.is_some()))
                    || self.unrolling() =>
            {
                self.forprep(vm, idx, i)
            }
            O::Call => self.call(vm, idx, i),
            O::Return | O::Return0 | O::Return1 => self.ret(vm, i, op),
            O::Tailcall => self.tailcall(vm, idx, i),
            #[cfg(not(rustynail_lua54))]
            O::Getvarg => Err(AbortReason::Vararg),
            O::Varargprep => self.varargprep(vm, idx),
            O::Vararg => self.vararg_op(vm, idx, i),
            _ => Err(AbortReason::Nyi(i.opcode_raw())),
        }
    }

    /// `CALL` (4.3, 4.4): a Lua closure is inlined, one of the `math` intrinsics is recorded as typed IR, everything
    /// else aborts (`CFunc`, `Metamethod` for a non-function, `MultRet` for `B == 0` / `C == 0`).
    fn call(&mut self, vm: &Vm<'_>, idx: usize, i: Instruction) -> R<usize> {
        let (a, b, c) = (i.a() as usize, i.b() as usize, i.c() as usize);
        // F4: `B == 0` reads the `top` an earlier producer left (`top_in`), `C == 0` leaves one (`wanted == -1`)
        let nargs = if b == 0 {
            self.values_to_top(a + 1)?
        } else {
            b - 1
        };
        let wanted = c as i32 - 1;
        let fv = vm.slot(a);
        if let Some(cl) = fv.as_lclosure() {
            return self.call_lua(vm, idx, a, nargs, wanted, cl, false);
        }
        if fv.tag() == LUA_VLCF {
            let addr = fv.as_native().map_or(0, |f| f as usize);
            return match jit_hooks::intrinsic_of(addr) {
                Some(k) if k <= jit_hooks::intr::MAX || self.cfg.f1math => {
                    self.intrinsic(vm, idx, a, nargs, wanted, k, addr)
                }
                None if addr == rn_std::lib_base::jit_select_addr() => {
                    self.select_call(vm, idx, a, nargs, wanted, addr)
                }
                None if self.cfg.pcall && addr == rn_std::lib_base::jit_pcall_addr() => {
                    self.pcall_call(vm, idx, a, nargs, wanted, addr, false)
                }
                None if self.cfg.pcall && addr == rn_std::lib_base::jit_xpcall_addr() => {
                    self.pcall_call(vm, idx, a, nargs, wanted, addr, true)
                }
                None if self.cfg.gfor && addr == rn_std::lib_base::jit_gfor_addrs().0 => {
                    self.gfor_call(vm, idx, a, nargs, wanted, addr, false)
                }
                None if self.cfg.gfor && addr == rn_std::lib_base::jit_gfor_addrs().1 => {
                    self.gfor_call(vm, idx, a, nargs, wanted, addr, true)
                }
                None if addr == rn_std::lib_table::jit_pack_addr() && self.cfg.pack => {
                    self.pack_call(vm, idx, a, nargs, wanted, addr)
                }
                None if self.cfg.setmt && addr == rn_std::lib_base::jit_setmt_addr() => {
                    self.setmt_call(vm, idx, a, nargs, wanted, addr)
                }
                None if self.cfg.libnext
                    && !cfg!(rustynail_lua54)
                    && addr != 0
                    && addr == jit_hooks::builtin_next() =>
                {
                    self.next_call(vm, idx, a, nargs, wanted, addr)
                }
                None if self.cfg.libcall
                    && rn_std::lib_string::jit_str_fns()
                        .iter()
                        .take(if self.cfg.libcall_rep { 4 } else { 3 })
                        .any(|&x| x == addr) =>
                {
                    self.lib_strfn(vm, idx, a, nargs, wanted, addr)
                }
                None if self.cfg.libfind && addr == rn_std::lib_string::jit_find_addr() => {
                    self.lib_find(vm, idx, a, nargs, wanted, addr)
                }
                None if self.cfg.f1strlen && addr == jit_hooks::strlen_addr() => {
                    self.lib_str_len(vm, idx, a, nargs, wanted, addr)
                }
                None if self.cfg.g1fmt
                    && addr == jit_hooks::strformat_addr()
                    && self.fmt_s_shape(vm, a, nargs) =>
                {
                    self.lib_fmt_s(vm, idx, a, nargs, wanted, addr)
                }
                None if self.cfg.tfmt
                    && addr == jit_hooks::strformat_addr()
                    && self.fmt_gen_ok(vm, a, nargs) =>
                {
                    self.lib_fmt(vm, idx, a, nargs, wanted, addr)
                }
                None if self.cfg.f2mt && addr == rn_std::lib_base::jit_getmt_addr() => {
                    self.getmt_call(vm, idx, a, nargs, wanted, addr)
                }
                None if self.cfg.f2asrt && addr == rn_std::lib_base::jit_assert_addr() => {
                    self.assert_call(vm, idx, a, nargs, wanted, addr)
                }
                None => match self.libfn_enabled(addr) {
                    Some(f) => self.lib_call(vm, idx, a, nargs, wanted, f, addr),
                    _ => self.stitch(idx, AbortReason::CFunc),
                },
                _ => self.stitch(idx, AbortReason::CFunc),
            };
        }
        if let Some(c) = fv.as_cclosure() {
            let tf = jit_hooks::type_fn_addr();
            if tf != 0 && c.f as usize == tf {
                return self.lib_type(vm, idx, a, nargs, wanted);
            }
            if self.cfg.f2rand && c.f as usize == rn_std::lib_math::jit_random_addr() {
                return self.lib_random(vm, idx, a, nargs, wanted);
            }
        }
        if fv.tag() == LUA_VLCFC || fv.as_cclosure().is_some() {
            return self.stitch(idx, AbortReason::CFunc);
        }
        if self.cfg.mm && fv.is_table() {
            return self.mm_call(vm, idx, a, nargs, wanted, false);
        }
        Err(AbortReason::Metamethod) // not a function: `__call` or an error
    }

    /// A `CALL` of a C function that the recorder cannot represent: with stitching on (and enough recorded work to pay
    /// for the extra entry and exit) the recording ends here with an always-taken exit whose snapshot is the state
    /// before the `CALL` (tier 1 re-executes it), otherwise `why` (`CFunc`; F4: `Recursion` beyond the unroll limit).
    fn stitch(&mut self, idx: usize, why: AbortReason) -> R<usize> {
        let _ = idx;
        if self.cfg.stitch0
            && self.cfg.side.is_some()
            && self.path.len() <= 1
            && !mutants::active("stitch_side_empty")
        {
            // BUNDLE-F: a side recording that stitches before its first bytecode is an empty trace (an entry, an exit at the same
            // `CALL`, hot again): chains of up to 16 of them. (`fn_end` refuses the same case.)
            return Err(why);
        }
        if self.cfg.rec_only
            && (self.cfg.fn_root || (self.cfg.rec_cont && self.cfg.rec_any))
            && self.cfg.side.is_none()
        {
            // P-M3: a native-recursion head records nothing but the recursion (R2: nor does a continuation head of `recany`)
            return Err(if why == AbortReason::Recursion {
                why
            } else {
                AbortReason::FnShort
            });
        }
        // (a function-entry recording stitches by its own minimum: it has no loop to pay for the extra exit)
        let min = if self.cfg.fn_root {
            self.cfg.fn_min
        } else {
            self.cfg.stitch_min
        };
        if self.cfg.fn_root && self.cfg.side.is_none() && self.path.len() < min as usize {
            return Err(AbortReason::FnShort);
        }
        // M1: a side recording of a loop root stitches at a recursive call (the exit before the `CALL` has no link target,
        // as a function root's side trace; the interpreter / tier 1 runs the call and re-enters the root trace at its head)
        let side_stitch = self.cfg.side_stitch
            && matches!(why, AbortReason::Recursion | AbortReason::Depth)
            && self.path.len() >= self.cfg.side_stitch_min as usize
            && !mutants::active("side_stitch_never");
        if (self.cfg.side.is_some() && !self.cfg.fn_root && !side_stitch)
            || (self.cfg.stitch_min == 0 && !self.cfg.fn_root && !self.cfg.side.is_some())
            || (self.cfg.side.is_none() && self.path.len() < min as usize)
            || mutants::active("stitch_never")
        {
            return Err(why);
        }
        self.end_exit()
    }

    /// P-E2: the root frame of a function-entry recording reaches its `RETURN` / `TAILCALL`: the recording ends with an
    /// always-taken exit before the instruction (tier 1 executes it), provided it recorded enough (`fn_min`).
    fn fn_end(&mut self) -> R<usize> {
        if self.cfg.rec_only && self.cfg.side.is_none() {
            // P-M3: the root frame returned before a self-call (a leaf invocation): no recursion on this path
            return Err(AbortReason::FnShort);
        }
        if self.cfg.side.is_none() && self.path.len() < self.cfg.fn_min as usize {
            return Err(AbortReason::FnShort);
        }
        if self.cfg.side.is_some() && self.path.len() <= 1 && !mutants::active("fn_side_at_return")
        {
            // a side trace that would start at the `RETURN` itself (the final exit of a function root is hot by
            // construction) is empty: its own final exit would be hot again, and so on
            return Err(AbortReason::LeaveFrame);
        }
        self.end_exit()
    }

    /// P-M3: the root frame of a function-entry recording calls its own prototype: the callee-identity guard, the stack and
    /// `CallInfo` conditions of the interpreter's fast path (the entry checks), then the recording ends before the
    /// `CALL` with the placeholder exit, which the assembler turns into the native frame push (`TraceIr::rec_call`).
    #[allow(clippy::too_many_arguments)]
    fn end_rec_call(
        &mut self,
        vm: &Vm<'_>,
        idx: usize,
        a: usize,
        nargs: usize,
        wanted: i32,
        p: &Proto,
        cl: rn_core::value::Gc<LClosure>,
    ) -> R<usize> {
        let _ = cl;
        let key = p as *const Proto as usize;
        let nslots = usize::from(p.maxstacksize);
        let top_rel = self.frames[0].base_off + a + 1 + nargs;
        // callee identity (TR7a): the function slot holds a Lua closure of this very proto
        let (cref, _) = self.rd(vm, a)?;
        // (the callee is the running closure: equality with it implies the proto, and costs one compare)
        let running = self.func_ref;
        if mutants::active("callee_guard_skip") {
            // (mutant: no guard)
        } else if running != NOREF {
            self.entry_guard(TOp::Eq, Ty::I64, cref, running, 0)?;
        } else {
            let kp = self.b.ir.konst(KVal::Ptr(key as u64, PtrKind::Proto));
            self.entry_guard(TOp::Eqref, Ty::I64, cref, kp, 0)?;
        }
        // TR6c / TR6b: what the interpreter's fast path needs for this call (the entry checks of the trace)
        self.room = self.room.max((top_rel + nslots) as u32);
        self.max_ci = self.max_ci.max(1);
        let code = p.code_ptr() as u64;
        self.rec_call = Some(RecCall {
            a: a as u16,
            nargs: nargs as u16,
            c: (wanted + 1) as u32,
            maxstack: p.maxstacksize as u16,
            code,
            after: code + 4 * (idx as u64 + 1),
        });
        self.end_exit()
    }

    /// P-M3: `RETURN1` of the root frame of a continuation or return recording: the value is read (its type guard is an
    /// ordinary entry guard), the recording ends before the instruction with the placeholder exit, which the assembler
    /// guards (callstatus, calling frame) and turns into the native return (`TraceIr::rec_ret`).
    fn end_rec_ret(&mut self, vm: &Vm<'_>, i: Instruction, op: OpCode) -> R<usize> {
        if op == OpCode::Return0 {
            self.rec_ret = Some((RecRet { c: 1, n: 0 }, NOREF));
            return self.end_exit();
        }
        let (r, _) = self.rd(vm, i.a() as usize)?;
        self.rec_ret = Some((RecRet { c: 2, n: 1 }, r));
        self.end_exit()
    }

    /// The always-taken exit that ends a straight-line recording before the instruction being recorded.
    fn end_exit(&mut self) -> R<usize> {
        if self.pcall.is_some() {
            // P-O: a recording cannot end inside an inlined protected call (its frames are not restorable)
            return Err(AbortReason::CFunc);
        }
        if self.cfg.side.as_ref().is_some_and(|s| !s.frames.is_empty()) {
            let n: u32 = self.cost.iter().sum();
            let min = self.cfg.side.as_ref().map_or(0, |s| s.frame_min);
            if n < min && !mutants::active("framemin_ignored") {
                return Err(AbortReason::SideShort);
            }
        }
        let s = self.cur_snapshot()?;
        let (k0, k1) = (self.kint(0), self.kint(1));
        self.b.emit_raw(Ins {
            op: TOp::Eq,
            t: Ty::I64,
            flags: F_GUARD,
            a: k0,
            b: k1,
            prev: NOREF,
            aux: guard_aux(s, 0),
        })?;
        self.end_snap = Some(s);
        self.stitched = true;
        Ok(self.cur_pc as usize)
    }

    /// [`Snap::top`] of a snapshot of the instruction being recorded.
    fn top_code(&self) -> u16 {
        if mutants::active("snap_top_dropped") {
            return 0;
        }
        self.top_in.map_or(0, |t| (t + 1) as u16)
    }

    /// The `top` the instruction being recorded reads, as the number of values from register `from` of the current frame
    /// to it (`CALL B=0`, `RETURN B=0`, `TAILCALL B=0`, `SETLIST B=0`); `MultRet` when no producer left one.
    fn values_to_top(&self, from: usize) -> R<usize> {
        let t = self.top_in.ok_or(AbortReason::MultRet)?;
        let f = self.frames[self.cur_depth].base_off + from;
        let t = if mutants::active("top_count_off") {
            t + 1
        } else {
            t
        };
        t.checked_sub(f).ok_or(AbortReason::Diverge)
    }

    /// The snapshot of the instruction being recorded (state before it, TR3a), created on first use.
    fn cur_snapshot(&mut self) -> R<u32> {
        if let Some(s) = self.cur_snap {
            return Ok(s);
        }
        if self.b.ir.snaps.len() >= MAX_SNAPS - 2 {
            return Err(AbortReason::TooLong);
        }
        let low = self.frames[self.cur_depth].low;
        let (land, _) = snap::land_info(&self.lows[low as usize], self.cur_pc);
        let frames = self.frame_recs();
        let top = self.top_code();
        let s = snap::take(&mut self.b.ir, &self.sm, self.cur_pc, land, &frames, top);
        self.meta.push(SnapMeta {
            step: self.path.len() as u32 - 1,
            pc: self.cur_pc,
            uncharged: false,
            low,
        });
        self.cur_snap = Some(s);
        Ok(s)
    }

    /// Inlines a call of the Lua closure `cl` (4.4): checks (vararg, recursion, depth, extent, tier-1 labels), the
    /// callee-identity guard (`EQREF closure, KPTR proto`, an entry guard when TR6d allows), the callee frame (its
    /// parameters are the caller's argument slots; missing ones are `nil`) and the TR6b/TR6c conditions. The edge to
    /// the callee proto is registered by the step, right after this instruction (TR32).
    ///
    /// F4: `wanted == -1` is a call with `C == 0` (the results end at a `top`, left by the callee's `RETURN`);
    /// recursion is inlined up to `RecCfg::unroll` activations of a prototype (the root counts) and ends the recording
    /// with a stitching exit beyond; `tail` is a `TAILCALL`: the callee replaces the frame of the current function (same
    /// function slot, `CallInfo`, caller `pc` and wanted results; `CIST_TAIL`) instead of pushing one.
    #[allow(clippy::too_many_arguments)]
    fn call_lua(
        &mut self,
        vm: &Vm<'_>,
        idx: usize,
        a: usize,
        nargs: usize,
        wanted: i32,
        cl: rn_core::value::Gc<LClosure>,
        tail: bool,
    ) -> R<usize> {
        let p = cl.proto();
        let vararg = p.is_vararg();
        if vararg {
            // P-E: a vararg callee with the hidden-argument layout is inlined (its `VARARGPREP` is recorded as a step);
            // the 5.5 vararg *table* (`...t`) and a `TAILCALL` into a vararg function stay aborts
            #[cfg(not(rustynail_lua54))]
            if p.flag & rn_core::protodata::PF_VATAB != 0 {
                return Err(AbortReason::Vararg);
            }
            if tail && self.cfg.tailx & 2 == 0 {
                return Err(AbortReason::Vararg);
            }
        }
        let key = &*p as *const Proto as usize;
        let d = self.cur_depth;
        // recursion: the activations of this prototype on the frame stack (a tail call replaces frame `d`)
        let uses = self
            .frames
            .iter()
            .enumerate()
            .filter(|(i, f)| f.proto == key && !(tail && *i == d))
            .count();
        if uses > 0
            && self.cfg.rec_native
            && (self.cfg.fn_root || self.cfg.rec_cont)
            && self.cfg.side.as_ref().map_or(true, |sd| {
                self.cfg.rec_any && self.cfg.fn_root && sd.frames.is_empty()
            })
            && d == 0
            && !tail
            && !vararg
            && wanted >= 0
            && nargs == usize::from(p.numparams)
        {
            return self.end_rec_call(vm, idx, a, nargs, wanted, &p, cl);
        }
        if uses > 0 {
            CONT_OK.with(|c| c.borrow_mut().insert(key));
            if self.cfg.unroll == 0 {
                return Err(AbortReason::Recursion);
            }
            let limit =
                self.cfg.unroll as usize + usize::from(mutants::active("unroll_off_by_one"));
            if uses >= limit {
                return self.stitch(idx, AbortReason::Recursion);
            }
        }
        let max_depth = if self.cfg.max_depth == 0 {
            MAX_DEPTH
        } else {
            (self.cfg.max_depth as usize).min(super::exit::MAX_FRAMES)
        };
        if !tail && d >= max_depth {
            // M1: beyond the inlining depth the recording ends with a stitching exit before the call (as at the recursion unroll
            // limit) when it recorded enough; otherwise `Depth`
            if self.cfg.depth_stitch && !mutants::active("depth_stitch_never") {
                return self.stitch(idx, AbortReason::Depth);
            }
            return Err(AbortReason::Depth);
        }
        let func_rel = self.frames[d].base_off + a;
        let base_rel = func_rel + 1;
        let top_rel = base_rel + nargs;
        let nslots = usize::from(p.maxstacksize);
        // the frame's slots: a tail call reuses the replaced frame's base
        let new_base = if tail {
            self.frames[d].base_off
        } else {
            base_rel
        };
        // P-E: `luaT_adjustvarargs` lays the frame out `totalargs + 1` slots higher
        let totalargs = nargs.max(usize::from(p.numparams));
        let vshift = if vararg { totalargs + 1 } else { 0 };
        self.need_ext(new_base + vshift + nslots + usize::from(vararg))?;
        let low = self.low_for(&p)?;
        if !self.deps.iter().any(|k| k.0 == key) {
            self.deps.push(ProtoKey(key));
            self.want_dep.push(p);
        }
        // callee identity: the function slot holds a Lua closure (typecheck by the read) of this very proto (TR7a)
        let (cref, _) = self.rd(vm, a)?;
        let kp = self.b.ir.konst(KVal::Ptr(key as u64, PtrKind::Proto));
        if !mutants::active("callee_guard_skip") {
            self.entry_guard(TOp::Eqref, Ty::I64, cref, kp, 0)?;
        }
        let nil = self.b.ir.konst(KVal::Pri(Ty::Nil));
        if tail {
            // `pre_tail_call`: the function and its arguments move down over the replaced frame's function slot (all
            // values are read before any slot is written), missing parameters are completed with nil. The stale
            // registers above stay as they are (TR3).
            let mut args = Vec::with_capacity(nargs);
            if let Some(v) = self.tail_args.take() {
                debug_assert_eq!(v.len(), nargs);
                args = v;
            } else {
                for j in 1..=nargs {
                    args.push(self.rd(vm, a + j)?.0);
                }
            }
            let fslot = self.frames[d].base_off - 1;
            if !mutants::active("tail_args_not_moved") {
                self.def_abs(fslot, cref);
                for (j, r) in args.into_iter().enumerate() {
                    self.def_abs(new_base + j, r);
                }
            }
            for j in nargs..usize::from(p.numparams) {
                if !mutants::active("tail_nil_fill_skip") {
                    self.def_abs(new_base + j, nil);
                }
            }
            // TR6c: `pre_tail_call` checks the stack like a call does (`stack_size - top > maxstacksize`)
            self.room = self.room.max((top_rel + nslots) as u32);
            // BUNDLE-N: a vararg callee (the replaced frame never is one: see `Rec::tailcall`): `VARARGPREP` runs next, as after a `CALL`
            if vararg && !mutants::active("vararg_room_skip") {
                self.room = self.room.max((new_base + totalargs + nslots + 1) as u32);
            }
            let f = &mut self.frames[d];
            f.proto = key;
            f.closure = cl.as_ptr() as usize;
            f.nslots = nslots;
            f.low = low;
            f.cref = cref;
            f.tail = true;
            f.vpending = vararg.then_some(totalargs);
            f.vdelta = 0;
            f.nextra = 0;
            self.next_depth = d;
            return Ok(0);
        }
        // `pre_call_lua_fast` completes the missing arguments with nil
        for j in nargs..usize::from(p.numparams) {
            self.def_abs(base_rel + j, nil);
        }
        // TR6c / TR6b: the interpreter's fast path needs `stack_size - top > maxstacksize` and a spare CallInfo; a vararg
        // callee's `VARARGPREP` runs `check_stack(maxstacksize + 1)` at `top = func + 1 + totalargs` (P-E, mutant
        // `vararg_room_skip`)
        self.room = self.room.max((top_rel + nslots) as u32);
        if vararg && !mutants::active("vararg_room_skip") {
            self.room = self.room.max((base_rel + totalargs + nslots + 1) as u32);
        }
        let vpending = vararg.then_some(totalargs);
        self.max_ci = self.max_ci.max((self.ci_depth(d) + 1) as u8);
        self.frames.push(RecFrame {
            ci: 0,
            func_abs: vm.base + a,
            proto: key,
            closure: cl.as_ptr() as usize,
            base_off: base_rel,
            nslots,
            call_pc: idx as u32,
            nresults: wanted as i16,
            low,
            cref,
            tail: false,
            vpending,
            vdelta: 0,
            nextra: 0,
            pc_shift: 0,
            pc_wanted: 0,
            mm: None,
            ectx: None,
            tfor: None,
        });
        self.next_depth = d + 1;
        Ok(0)
    }

    /// `TAILCALL` of a Lua closure in an inlined frame (F4): [`Rec::call_lua`] with `tail`. The root frame, a callee
    /// that is not a Lua closure, upvalues to close (`k`) and vararg frames (`C`) stay aborts.
    fn tailcall(&mut self, vm: &Vm<'_>, idx: usize, i: Instruction) -> R<usize> {
        let (a, b) = (i.a() as usize, i.b() as usize);
        if self.cur_depth == 0 {
            if self.cfg.fn_root {
                return self.fn_end();
            }
            return Err(AbortReason::TailCall);
        }
        if self.frames[self.cur_depth].pc_shift != 0 {
            return Err(AbortReason::TailCall); // P-O: the body of an inlined protected call returns normally
        }
        // P-E: in a vararg frame `C = numparams + 1`; only a native (intrinsic / `select`) may be tail-called from there
        let vtail = i.c() != 0;
        if vtail
            && !(self.frames[self.cur_depth].vdelta != 0
                && i.c() as usize == usize::from(vm.cl.proto().numparams) + 1)
        {
            return Err(AbortReason::Vararg);
        }
        if i.k() != 0 && !self.cfg.closeret {
            return Err(self.nyi()); // closes upvalues: no CLOSURE is recorded (M1: an inlined frame has none, see `Rec::ret`)
        }
        let nargs = if b == 0 {
            self.values_to_top(a + 1)?
        } else {
            b - 1
        };
        let fv = vm.slot(a);
        if vtail && fv.as_lclosure().is_some() {
            return Err(AbortReason::Vararg);
        }
        if let Some(cl) = fv.as_lclosure() {
            return self.call_lua(
                vm,
                idx,
                a,
                nargs,
                self.frames[self.cur_depth].nresults as i32,
                cl,
                true,
            );
        }
        if fv.tag() == LUA_VLCF {
            let addr = fv.as_native().map_or(0, |f| f as usize);
            if let Some(k) = jit_hooks::intrinsic_of(addr)
                .filter(|&k| k <= jit_hooks::intr::MAX || self.cfg.f1math)
            {
                // `return math.max(a, b)`: the interpreter runs the native on top of the frame (its results land at the
                // call register, `top` after them), then `post_call` moves them to the frame's function slot: an
                // intrinsic with `C = 0` followed by a `RETURN` of its one result from register `A`
                self.intrinsic(vm, idx, a, nargs, -1, k, addr)?;
                self.top_rel = None;
                return self.ret_from(vm, a, 1);
            }
            if addr == rn_std::lib_base::jit_select_addr() {
                // `return select(...)`: the native's results (any number) are returned from the call register
                self.select_call(vm, idx, a, nargs, -1, addr)?;
                let top = self.top_rel.take().ok_or(AbortReason::Diverge)?;
                let n = top - (self.frames[self.cur_depth].base_off + a);
                return self.ret_from(vm, a, n);
            }
            // N3b: `return setmetatable(t, mt)` / `return string.sub(...)`: the recorded native with `C = 0`, then a `RETURN` of its results
            if self.cfg.tailnat {
                let setmt = self.cfg.setmt && addr == rn_std::lib_base::jit_setmt_addr();
                let lf = self.libfn_enabled(addr);
                if setmt || lf.is_some() {
                    self.tail_lib = true;
                    let r = if setmt {
                        self.setmt_call(vm, idx, a, nargs, -1, addr)
                    } else {
                        self.lib_call(vm, idx, a, nargs, -1, lf.unwrap_or(u8::MAX), addr)
                    };
                    self.tail_lib = false;
                    r?;
                    let top = self.top_rel.take().ok_or(AbortReason::Diverge)?;
                    let n = top - (self.frames[self.cur_depth].base_off + a);
                    return self.ret_from(vm, a, n);
                }
            }
        }
        if self.cfg.mm && self.cfg.tailx & 1 != 0 && !vtail && fv.is_table() {
            return self.mm_call(
                vm,
                idx,
                a,
                nargs,
                self.frames[self.cur_depth].nresults as i32,
                true,
            );
        }
        Err(AbortReason::TailCall)
    }

    /// `RETURN0` / `RETURN1` / `RETURN` in an inlined frame: the results move to the caller's slots from the function slot
    /// on (nil-filled up to the wanted count; F4: `RETURN B=0` returns up to the `top` a producer left, and a call with
    /// `C=0` takes all of them and leaves `top` after the last), the frame is popped. The callee's registers stay in the
    /// slot map (TR3: the interpreter leaves them in the stack, snapshots of later instructions restore them).
    fn ret(&mut self, vm: &Vm<'_>, i: Instruction, op: OpCode) -> R<usize> {
        let d = self.cur_depth;
        if d == 0 {
            if self.cfg.rec_conts
                && (op == OpCode::Return1 || (op == OpCode::Return0 && self.cfg.rec_any))
                && (!self.cfg.rec_any || self.cfg.rec_seen)
                && (self.cfg.rec_cont || (self.cfg.fn_root && self.cfg.side.is_some()))
            {
                return self.end_rec_ret(vm, i, op);
            }
            // N2c (BUNDLE-N, `retf` bit 0): a side recording that reaches the `RETURN` of its root frame pops it for real and goes on in the
            // caller; anything that makes the pop unsound or unprofitable keeps the old behaviour (`None`)
            if self.cfg.retf & 1 != 0
                && self.cfg.side.is_some()
                && (!self.cfg.fn_root || self.cfg.retf & 4 != 0)
            {
                if let Some(r) = self.retf(vm, i, op)? {
                    return Ok(r);
                }
            }
            if !self.segs.is_empty() && !mutants::active("retf_second_return_aborts") {
                // N3a: a second root `RETURN` the recording cannot pop (`retfmax`, a vararg / C caller, ...): it ends with the stitching exit before it
                return self.end_exit();
            }
            if self.cfg.fn_root {
                return self.fn_end();
            }
            return Err(AbortReason::LeaveFrame);
        }
        let a = i.a() as usize;
        let nret = match op {
            OpCode::Return0 => 0,
            OpCode::Return1 => 1,
            _ => {
                // P-E: a vararg frame returns with `C = numparams + 1` (the delta `OP_RETURN` takes off `func`)
                let f = &self.frames[d];
                let ok_c = if f.vdelta != 0 {
                    i.c() as usize == usize::from(vm.cl.proto().numparams) + 1
                } else {
                    i.c() == 0
                };
                if !ok_c {
                    return Err(AbortReason::Vararg);
                }
                // M1: `k` (upvalues or to-be-closed variables to close): an inlined frame has none, because the recorder
                // entered the frame itself and records no `CLOSURE` / `TBC` / `TFORPREP` in it (they abort `Nyi`), so
                // `OP_RETURN`'s `luaF_close` is a no-op. The compiler sets `k` on every `RETURN` of a function that
                // contains a closure or a generic `for` (its closing value), which is most interpreter-shaped code.
                if i.k() != 0 && !self.cfg.closeret {
                    return Err(self.nyi()); // upvalues to close: no CLOSURE is recorded
                }
                if i.b() == 0 {
                    self.values_to_top(a)?
                } else {
                    i.b() as usize - 1
                }
            }
        };
        self.ret_from(vm, a, nret)
    }

    /// N2c (RETF): the `RETURN` of the root frame in a side recording. Emits `RETF` (the caller-identity guards and the native pop, see
    /// [`TOp::Retf`]) and rebases the recording on the caller `C`: the slot map, the known types, the model, the frame stack and the
    /// root fields all move to `C`'s coordinates (`C`'s base is `A + 1` slots below the returning frame's). `Ok(None)`: not applicable
    /// (the caller is not a plain Lua frame suspended after an `OP_CALL`, the frame is a vararg, closes upvalues, ...): the caller keeps
    /// the old behaviour. The recording ends at the first instruction of `C` after the pop (see `on_step`).
    fn retf(&mut self, vm: &Vm<'_>, i: Instruction, op: OpCode) -> R<Option<usize>> {
        const NO: R<Option<usize>> = Ok(None);
        if cfg!(rustynail_lua54)
            || self.cfg.retfmax == 0
            || self.segs.len() > self.cfg.retfmax as usize
        {
            return NO; // (5.4: `nresults` is a field of its own and the CIST bits differ: not wired)
        }
        if self.pcall.is_some() || self.cur_depth != 0 || vm.cl.proto().is_vararg() {
            return NO;
        }
        // the returning frame F (the root) and its caller C
        let f_ci = vm.st.ci();
        if f_ci.as_ptr() as usize != self.root_ci
            || f_ci.r().callstatus & (CIST_C | CIST_MMRET | rn_core::state::CIST_FRESH) != 0
        {
            return NO;
        }
        let Some(c_ci) = f_ci.previous() else {
            return NO;
        };
        if c_ci.r().callstatus & CIST_C != 0 {
            return NO;
        }
        let Some(ccl) = vm.st.ci_lclosure(c_ci) else {
            return NO;
        };
        let cp = ccl.proto();
        let ccode = cp.code();
        // C is suspended after an `OP_CALL`; F's function slot is its register A
        let sp = c_ci.r().savedpc as usize;
        let cbase = c_ci.r().func.0 + 1;
        let Some(call_pc) = (sp.wrapping_sub(ccode.as_ptr() as usize) / 4).checked_sub(1) else {
            return NO;
        };
        if call_pc >= ccode.len() || ccode.as_ptr().wrapping_add(call_pc + 1) as usize != sp {
            return NO;
        }
        let ci_ins = Instruction(ccode[call_pc]);
        if ci_ins.opcode() != Some(OpCode::Call) {
            return NO;
        }
        let ca = ci_ins.a() as usize;
        if f_ci.r().func.0 != cbase + ca || ca + 1 > 200 {
            return NO;
        }
        let wanted = ci_ins.c() as i32 - 1;
        if (f_ci.r().callstatus & 0xFF) as i32 != wanted + 1 {
            return NO; // (a call through something that changed the wanted count)
        }
        // the instruction after the CALL must have a tier-1 label (the exit lands there)
        let cow = self.low_for(&cp)?;
        if self.lows[cow as usize].label_kind(call_pc as u32 + 1) == LabelKind::Dead {
            return NO;
        }
        // `RETURN` operands as for an inlined frame (the root is never vararg)
        let a = i.a() as usize;
        let nret = match op {
            OpCode::Return0 => 0,
            OpCode::Return1 => 1,
            _ => {
                if i.c() != 0 {
                    return NO;
                }
                if i.b() == 0 {
                    self.values_to_top(a)?
                } else {
                    i.b() as usize - 1
                }
            }
        };
        // `k`: the RETURN closes upvalues / to-be-closed variables. The guards of the `RETF` require that there are none at run time; if there are
        // some now, the trace could never pass them
        let kflag = op == OpCode::Return && i.k() != 0;
        if kflag && self.retf_closes(vm, vm.base) {
            return NO;
        }
        // the dependency: C's code address is a constant of the trace (TR32), and the exit lands in C's tier-1 code
        let ckey = &*cp as *const Proto as usize;
        // N3a (R4): a store through an open upvalue into the region below the root frame (the caller's slots) was recorded; after the pop the
        // caller's slots are the root's and the recorder's picture of them could be stale
        // (stack indices of the slots written: only the caller's registers `A..` below the returning frame matter, the other stores go to memory the
        // recording does not model)
        let c_lo = cbase;
        let f_lo = f_ci.r().func.0;
        if self
            .uv_below_stores
            .iter()
            .any(|&x| x >= c_lo && x < f_lo + 1)
        {
            return Err(AbortReason::RetfAlias);
        }
        self.uv_below_stores.clear();
        // ---- point of no return: from here the recording is a RETF recording ----
        let b = ca + 1;
        let nmove = if wanted < 0 { nret } else { wanted as usize };
        if wanted < 0 {
            self.need_ext(a + nret)?;
        }
        let nil = self.b.ir.konst(KVal::Pri(Ty::Nil));
        let mut vals = Vec::with_capacity(nmove);
        for j in 0..nmove {
            vals.push(if j < nret {
                self.rd_slot(vm, a + j)?.0
            } else {
                nil
            });
        }
        if !self.deps.iter().any(|k| k.0 == ckey) {
            self.deps.push(ProtoKey(ckey));
            self.want_dep.push(cp.clone());
        }
        let snap = self.cur_snapshot()?;
        let site = self.b.ir.konst(KVal::Ptr(sp as u64, PtrKind::Raw));
        let bits = ((wanted + 1) as u32 & 0xFF) | if kflag { RETF_K } else { 0 };
        let retf_ins = self.b.emit_retf(site, b as Ref, guard_aux(snap, bits))?;
        // ---- the rebase onto C ----
        let entry_code = vm.code.as_ptr() as u64;
        if self.segs.is_empty() {
            self.segs.push(Seg {
                proto: ProtoKey(self.entry_proto),
                code: entry_code,
                shift: 0,
                pops: 0,
                call_pc: 0,
                a: 0,
                retf_ins: NOREF,
            });
        }
        let (cum_shift, cum_pops) = (self.shift + b, self.segs.len() as u8);
        RETF_USED.with(|c| c.set(true));
        self.segs.push(Seg {
            proto: ProtoKey(ckey),
            code: ccode.as_ptr() as u64,
            shift: cum_shift as u16,
            pops: cum_pops,
            call_pc: call_pc as u32,
            a: ca as u16,
            retf_ins,
        });
        self.seg_steps.push(self.path.len() as u32);
        // entry-relative maxima of the segment that ends here (its coordinates are `shift` slots / `segs.len() - 1` pops above the entry's)
        let (sh0, pops0) = (self.shift, self.segs.len().saturating_sub(2) as u8);
        self.ext_entry = self.ext_entry.max(self.ext.saturating_sub(sh0));
        self.room_entry = self
            .room_entry
            .max((self.room as usize).saturating_sub(sh0) as u32);
        self.ci_entry = self.ci_entry.max(self.max_ci.saturating_sub(pops0));
        // the model: C's registers 0..=A (the last is F's function slot) go below the old frame
        let mut below = Vec::with_capacity(b);
        for j in 0..b {
            let v = vm.st.get(StkId(cbase + j));
            below.push(self.val_of(&v));
        }
        self.retf_below.push_back(below);
        let ch = self.closure_handle(ccl);
        self.fill_closure(ch);
        self.retf_funcs.push_back(ch);
        self.shift += b;
        // the slot map and the facts about slots move up by `b`
        let old = std::mem::take(&mut self.sm);
        let cn = usize::from(cp.maxstacksize);
        let len = (old.len() + b).max(cn);
        let mut nsm = SlotMap::new(len);
        let sh = if mutants::active("retf_slot_remap_off") {
            b + 1
        } else {
            b
        };
        for s in 0..old.len() {
            if s + sh >= len {
                break;
            }
            nsm.cur[s + sh] = old.cur[s];
            nsm.sload[s + sh] = old.sload[s];
            nsm.entry_ty[s + sh] = old.entry_ty[s];
            nsm.written[s + sh] = old.written[s];
            nsm.last_read[s + sh] = old.last_read[s];
        }
        self.sm = nsm;
        let mut nk = vec![None; self.known.len() + b];
        let kb = if mutants::active("retf_known_unshifted") {
            0
        } else {
            b
        };
        for (s, t) in self.known.iter().enumerate() {
            nk[s + kb] = *t;
        }
        self.known = nk;
        self.stale = std::mem::take(&mut self.stale)
            .into_iter()
            .map(|s| s + b)
            .collect();
        self.ls_memo.clear();
        // the results land in C's registers A.. (over the popped frame's function slot and first registers)
        let dest = if mutants::active("retf_results_dest_off") {
            ca + 1
        } else {
            ca
        };
        for (j, r) in vals.into_iter().enumerate() {
            self.def_abs(dest + j, r);
        }
        if wanted < 0 {
            let t = if mutants::active("retf_multret_top_unmapped") {
                nret
            } else {
                ca + nret
            };
            self.top_rel = Some(t);
        }
        // the frame stack is [C]; the root fields describe C
        let low = self.low_for(&cp)?;
        self.frames.clear();
        self.frames.push(RecFrame {
            ci: c_ci.as_ptr() as usize,
            func_abs: c_ci.r().func.0,
            proto: ckey,
            closure: ccl.as_ptr() as usize,
            base_off: 0,
            nslots: cn,
            call_pc: 0,
            nresults: 0,
            low,
            cref: NOREF,
            tail: false,
            vpending: None,
            vdelta: 0,
            nextra: 0,
            pc_shift: 0,
            pc_wanted: 0,
            mm: None,
            ectx: None,
            tfor: None,
        });
        self.root_ci = c_ci.as_ptr() as usize;
        if !mutants::active("retf_root_func_stale") {
            self.root_func = c_ci.r().func.0;
        }
        self.proto = ckey;
        self.nslots = cn;
        if !mutants::active("retf_func_ref_stale") {
            self.func_ref = NOREF;
        }
        self.ext = (self.ext + b).max(cn);
        self.room += b as u32;
        self.max_ci = 0;
        self.reroot = Some(c_ci.as_ptr() as usize);
        self.first_store = None;
        Ok(Some(call_pc + 1))
    }

    /// N2c: would the `RETURN` of the frame based at stack index `base` close something (a registered to-be-closed variable, an open upvalue at
    /// or above `base`)?
    fn retf_closes(&self, vm: &Vm<'_>, base: usize) -> bool {
        use rn_core::jit_hooks::thread as thr;
        let th = jit_hooks::thread_ptr_ro(vm.st);
        let bp = vm.st.slot_ptr(StkId(base)) as usize;
        // SAFETY: `th` is the live `Thread` of the recording (`Thread` offsets from `jit_hooks::thread`); `openupval` is null or a live `UpVal`.
        unsafe {
            if th.add(thr::TBC_LEN).cast::<usize>().read_unaligned() != 0 {
                return true;
            }
            let uv = th.add(thr::OPENUPVAL).cast::<usize>().read_unaligned();
            uv != 0
                && ((uv as *const u8)
                    .add(rn_core::jit_layout::upval::V)
                    .cast::<usize>()
                    .read_unaligned())
                    >= bp
        }
    }

    /// The results `R[a .. a + nret]` of the current inlined frame move to the caller (see [`Rec::ret`]).
    fn ret_from(&mut self, vm: &Vm<'_>, a: usize, nret: usize) -> R<usize> {
        let d = self.cur_depth;
        if self.frames[d].pc_shift != 0 {
            return self.pcall_ret(vm, a, nret);
        }
        let (wanted, fb, nslots, call_pc, vdelta) = {
            let f = &self.frames[d];
            (f.nresults as i32, f.base_off, f.nslots, f.call_pc, f.vdelta)
        };
        let mm = self.frames[d].mm;
        let tfor = self.frames[d].tfor;
        // P-E: a vararg frame returns its results to the *original* function slot (`ci.func -= delta`)
        let vdelta = if mutants::active("vararg_ret_dest") {
            0
        } else {
            vdelta
        };
        let dest = fb - 1 - vdelta;
        let nmove = if wanted < 0 { nret } else { wanted as usize };
        if wanted < 0 {
            self.need_ext(fb + a + nret)?;
        }
        let nil = self.b.ir.konst(KVal::Pri(Ty::Nil));
        let mut vals = Vec::with_capacity(nmove);
        for j in 0..nmove {
            // (P-E: the values of a `RETURN B=0` after a `VARARG C=0` may lie beyond the frame's `maxstacksize`)
            vals.push(if j < nret {
                self.rd_slot(vm, fb + a + j)?.0
            } else {
                nil
            });
        }
        self.frames.pop();
        self.next_depth = d - 1;
        if mutants::active("popped_frame_drop") {
            // mutant: the popped frame's registers are forgotten (the snapshots after the return omit them)
            for s in fb..fb + nslots {
                self.sm.cur[s] = NOREF;
                self.sm.written[s] = false;
            }
        }
        let first = vals.first().copied();
        for (j, r) in vals.into_iter().enumerate() {
            self.def_abs(dest + j, r);
        }
        if wanted < 0 {
            // `post_call` leaves `top` after the last result: the next instruction reads it
            self.top_rel = Some(dest + nret);
        }
        if let Some(tf) = tfor {
            return self.tfor_after_ret(
                call_pc as usize,
                tf,
                first.ok_or(AbortReason::Diverge)?,
                d - 1,
            );
        }
        match mm {
            Some(MmFin::Reg(r)) => {
                // N3: `mm_return` completes the caller's instruction: the result also lands in its target register
                if !mutants::active("mm_result_skip") {
                    let v = first.ok_or(AbortReason::Diverge)?;
                    let cb = self.frames[d - 1].base_off;
                    self.def_abs(
                        cb + usize::from(r) + usize::from(mutants::active("mm_result_slot_off")),
                        v,
                    );
                }
            }
            Some(MmFin::Cond(k)) => {
                // N3: the comparison's condition is the truth value of the first result (its type is a recorded fact: the value was
                // produced on the recorded path); `mm_return` skips the `JMP` when the condition differs from `k`, else dispatches it
                let v = first.ok_or(AbortReason::Diverge)?;
                let cond = !falsy(self.b.ir.ty_of(v)) != mutants::active("mm_cond_inverted");
                if cond != k {
                    return Ok(call_pc as usize + 2);
                }
                self.mm_jmp = true;
                return Ok(call_pc as usize + 1);
            }
            Some(MmFin::Drop) | None => {}
        }
        Ok(call_pc as usize + 1)
    }

    /// `VARARGPREP` of an inlined vararg callee (P-E), recorded as `luaT_adjustvarargs` performs it (the `CALL` pushed the
    /// frame with the pre-adjust layout; no guard is emitted here, so no snapshot sees that layout): the function and the
    /// fixed parameters are copied above the arguments (`totalargs + 1` slots up), the original parameters are erased,
    /// `R[numparams]` of the new frame is nil, and the frame's function slot, base and `CallInfo::func` move up.
    fn varargprep(&mut self, vm: &Vm<'_>, idx: usize) -> R<usize> {
        let d = self.cur_depth;
        let Some(total) = self.frames[d].vpending else {
            return Err(AbortReason::Vararg); // a vararg root frame: `nextraargs` is a runtime value
        };
        let np = usize::from(vm.cl.proto().numparams);
        let base_rel = self.frames[d].base_off;
        let nslots = self.frames[d].nslots;
        let cref = self.frames[d].cref;
        let delta = total + 1;
        let nb = base_rel + delta;
        self.need_ext(nb + nslots + 1)?;
        let mut params = Vec::with_capacity(np);
        for j in 0..np {
            params.push(self.rd_slot(vm, base_rel + j)?.0);
        }
        let nil = self.b.ir.konst(KVal::Pri(Ty::Nil));
        self.def_abs(nb - 1, cref);
        for (j, r) in params.into_iter().enumerate() {
            self.def_abs(nb + j, r);
            if !mutants::active("vararg_orig_not_nilled") {
                self.def_abs(base_rel + j, nil);
            }
        }
        self.def_abs(nb + np, nil);
        let shift = if mutants::active("vararg_delta_off") {
            delta + 1
        } else {
            delta
        };
        let f = &mut self.frames[d];
        f.vpending = None;
        f.vdelta = delta;
        f.nextra = total - np;
        f.base_off = base_rel + shift;
        f.func_abs += shift;
        Ok(idx + 1)
    }

    /// `VARARG A C` in an inlined vararg frame (P-E): the hidden arguments are slots below the frame's function slot, and
    /// their number is a recorder constant (the inlined `CALL`'s argument count), so this is a plain copy: `C - 1` values
    /// (nil-filled), or (`C == 0`) all of them with an explicit `top` after the last.
    fn vararg_op(&mut self, vm: &Vm<'_>, idx: usize, i: Instruction) -> R<usize> {
        let d = self.cur_depth;
        let (vdelta, nextra, base, nslots) = {
            let f = &self.frames[d];
            (f.vdelta, f.nextra, f.base_off, f.nslots)
        };
        #[cfg(not(rustynail_lua54))]
        if i.k() != 0 {
            return Err(AbortReason::Vararg); // the vararg table
        }
        if vdelta == 0 {
            return Err(AbortReason::Vararg);
        }
        let a = i.a() as usize;
        let n = i.c() as i32 - 1;
        let cnt = if n < 0 { nextra } else { n as usize };
        if n >= 0 && a + cnt > nslots {
            return Err(AbortReason::Diverge);
        }
        let hidden0 = base - 1 - nextra;
        let nil = self.b.ir.konst(KVal::Pri(Ty::Nil));
        let mut vals = Vec::with_capacity(cnt);
        for j in 0..cnt {
            vals.push(if j < nextra {
                self.rd_slot(vm, hidden0 + j)?.0
            } else {
                nil
            });
        }
        if n < 0 {
            self.need_ext(base + a + nextra + 1)?;
            // `get_varargs` runs `check_stack(nextra)` with `top = ci.top` (`protect!`): the slow path grows the stack
            if !mutants::active("vararg_room_skip") {
                self.room = self.room.max((base + nslots + nextra) as u32);
            }
        }
        for (j, r) in vals.into_iter().enumerate() {
            let r = if j == 0 && mutants::active("vararg_first_nil") {
                nil
            } else {
                r
            };
            self.def_abs(base + a + j, r);
        }
        if n < 0 {
            let t = base + a + nextra + usize::from(mutants::active("vararg_top_off"));
            self.top_rel = Some(t);
        }
        Ok(idx + 1)
    }

    /// N7: `ipairs(t)` / `pairs(t)`, the call that creates the state of a generic `for`. The native (`base_ipairs` / `base_pairs`)
    /// pushes `(ipairs_aux, t, 0)` / `(next, t, nil[, nil])` above its arguments (`nil, nil` in 5.5: the closing value) and returns them; the
    /// recorder reproduces the pushes and the moved results under an identity guard on the callee. `ipairs` accepts any value (`luaL_checkany`:
    /// it only needs an argument; the iteration indexes through metamethods, which `TFORCALL` refuses by itself). `pairs` of a table without
    /// metatable has no `__pairs` to honour: the metatable pointer is guarded to be nil (a table with a metatable stays a `MetaTable` abort,
    /// the absence of `__pairs` in a metatable cannot be guarded); another type is not recorded (`__pairs` of its metatable, the error).
    #[allow(clippy::too_many_arguments)]
    fn gfor_call(
        &mut self,
        vm: &Vm<'_>,
        idx: usize,
        a: usize,
        nargs: usize,
        wanted: i32,
        addr: usize,
        pairs: bool,
    ) -> R<usize> {
        if nargs == 0 {
            return Err(self.nyi()); // "bad argument #1 to 'ipairs' (table expected, got no value)"
        }
        let (func_rel, top_rel) = self.lib_head(vm, a, nargs, addr)?;
        let (tr, ty) = self.rd(vm, a + 1)?;
        let nil = self.b.ir.konst(KVal::Pri(Ty::Nil));
        let res: Vec<Ref> = if pairs {
            if ty != Ty::Tab {
                return Err(self.nyi());
            }
            let t = vm.slot(a + 1).as_table().ok_or(AbortReason::Diverge)?;
            if t.metatable().is_some() {
                return Err(AbortReason::MetaTable);
            }
            if !mutants::active("gfor_pairs_meta_skip") {
                let mt = self.fload(tr, Field::Metatable, Ty::P64)?;
                let z = self.kint(0);
                self.guard(TOp::Eq, Ty::I64, mt, z, 0)?;
            }
            let next =
                jit_hooks::builtin_next() + usize::from(mutants::active("gfor_wrong_iterator"));
            let kn = self.b.ir.konst(KVal::Ptr(next as u64, PtrKind::Builtin));
            let mut v = vec![kn, tr, nil];
            if !cfg!(rustynail_lua54) {
                v.push(nil);
            }
            if mutants::active("gfor_result_count") {
                v.pop();
            }
            v
        } else {
            let aux = jit_hooks::builtin_ipairs_aux();
            let kn = self.b.ir.konst(KVal::Ptr(aux as u64, PtrKind::Builtin));
            let z = self.kint(i64::from(mutants::active("gfor_ipairs_start_one")));
            vec![kn, tr, z]
        };
        // the native's pushes land above its arguments (dead slots the interpreter's stack holds)
        self.need_ext(top_rel + res.len() + 1)?;
        for (j, &r) in res.iter().enumerate() {
            self.def_abs(top_rel + j, r);
        }
        self.lib_tail(idx, func_rel, top_rel, &res, false, wanted)
    }

    /// `select(n, ...)` / `select('#', ...)` with the argument count known at record time (P-E; the `...` of an inlined
    /// vararg frame or any fixed argument list): the callee-identity guard on the registered `select`, a guard on the
    /// selector, then pure slot moves. `'#'` pushes the count (the native's push above its arguments, then the result
    /// move); an integer selector `i` returns the arguments from `i` on, which are already on the stack.
    fn select_call(
        &mut self,
        vm: &Vm<'_>,
        idx: usize,
        a: usize,
        nargs: usize,
        wanted: i32,
        addr: usize,
    ) -> R<usize> {
        if nargs == 0 {
            return Err(self.nyi()); // "bad argument #1"
        }
        let d = self.cur_depth;
        let func_rel = self.frames[d].base_off + a;
        let top_rel = func_rel + 1 + nargs;
        self.need_ext(top_rel + 1)?;
        let (cr, _) = self.rd(vm, a)?;
        let kb = self.b.ir.konst(KVal::Ptr(addr as u64, PtrKind::Builtin));
        if !mutants::active("callee_guard_skip") {
            self.entry_guard(TOp::Eqref, Ty::I64, cr, kb, 0)?;
        }
        let (sr, sty) = self.rd(vm, a + 1)?;
        let vsel = vm.slot(a + 1);
        let nil = self.b.ir.konst(KVal::Pri(Ty::Nil));
        let mut res: Vec<Ref> = Vec::new();
        match sty {
            Ty::ShrStr => {
                let s = vsel.as_strref().ok_or(AbortReason::Diverge)?;
                if s.get().as_bytes() != b"#" {
                    return Err(self.nyi());
                }
                if !mutants::active("select_guard_skip") {
                    let kp = self
                        .b
                        .ir
                        .konst(KVal::Ptr(payload_bits(&vsel, sty), PtrKind::Raw));
                    let kr = self.b.emit_raw(Ins {
                        op: TOp::Kptr,
                        t: Ty::ShrStr,
                        flags: 0,
                        a: kp,
                        b: NOREF,
                        prev: NOREF,
                        aux: 0,
                    })?;
                    self.guard(TOp::Eqref, Ty::I64, sr, kr, 0)?;
                }
                let cnt = nargs as i64 - 1 + i64::from(mutants::active("select_count_off"));
                let c = self.kint(cnt);
                // the native's push (`lua_pushinteger`) lands above its arguments
                self.def_abs(top_rel, c);
                res.push(c);
            }
            Ty::Int => {
                let i = vsel.as_int().ok_or(AbortReason::Diverge)?;
                let n = nargs as i64;
                let ii = if i < 0 {
                    i + n
                } else if i > n {
                    n
                } else {
                    i
                };
                if ii < 1 {
                    return Err(self.nyi()); // "index out of range"
                }
                if !mutants::active("select_guard_skip") {
                    let ki = self.kint(i);
                    self.guard(TOp::Eq, Ty::I64, sr, ki, 0)?;
                }
                let ii = ii as usize + usize::from(mutants::active("select_index_off"));
                for j in 0..(nargs.saturating_sub(ii)) {
                    res.push(self.rd_slot(vm, func_rel + 1 + ii + j)?.0);
                }
            }
            _ => return Err(self.nyi()),
        }
        // TR6c / TR6b for the native frame
        if !mutants::active("native_room_skip") {
            self.room = self.room.max((top_rel + LUA_MINSTACK) as u32);
        }
        if !mutants::active("native_ci_skip") {
            self.max_ci = self.max_ci.max((self.ci_depth(d) + 1) as u8);
        }
        let nmove = if wanted < 0 {
            res.len()
        } else {
            wanted as usize
        };
        self.need_ext(func_rel + nmove + 1)?;
        for j in 0..nmove {
            let r = res.get(j).copied().unwrap_or(nil);
            self.def_abs(func_rel + j, r);
        }
        if wanted < 0 {
            self.top_rel = Some(func_rel + res.len());
        }
        Ok(idx + 1)
    }

    /// P-O: `pcall(f, ...)` / `xpcall(f, msgh, ...)` of a non-vararg Lua closure `f`, the call stays inside the trace. The native
    /// (`base_pcall` / `base_xpcall`) rearranges its frame (`[pcall, true, f, args]`, `[xpcall, f, msgh, true, f, args]`) and runs
    /// `f` protected; on a normal return `finishpcall` hands `true` and the results to the caller. The recorder models exactly
    /// that: the callee frame lies `shift` slots above the native's function slot with the native's `CallInfo` in between
    /// (`RecFrame::pc_shift`), and every guard of the body takes the snapshot of the `CALL` itself (`PcallCtx`): an exit, which is
    /// the only way an error of the body can leave the trace, lands on the `CALL`, so the interpreter runs the real `pcall` and
    /// raises (or not) with its own error values, status, handler order, traceback and `__close` calls. That is sound because no
    /// side effect may precede any exit of the body ([`Rec::pcall_guard_check`]). One more guard keeps the C-stack depth of the
    /// real call in reach ([`lib::CCALLS`]).
    #[allow(clippy::too_many_arguments)]
    fn pcall_call(
        &mut self,
        vm: &Vm<'_>,
        idx: usize,
        a: usize,
        nargs: usize,
        wanted: i32,
        addr: usize,
        x: bool,
    ) -> R<usize> {
        let need = if x { 2 } else { 1 };
        if nargs < need {
            return Err(self.nyi()); // "bad argument"
        }
        let Some(cl) = vm.slot(a + 1).as_lclosure() else {
            return self.stitch(idx, AbortReason::CFunc);
        };
        let p = cl.proto();
        let key = &*p as *const Proto as usize;
        let d = self.cur_depth;
        let max_depth = if self.cfg.max_depth == 0 {
            MAX_DEPTH
        } else {
            (self.cfg.max_depth as usize).min(super::exit::MAX_FRAMES)
        };
        if self.pcall.is_some()
            || p.is_vararg()
            || d >= max_depth
            || self.frames.iter().any(|f| f.proto == key)
            || self.frames.len() + 1 >= super::exit::MAX_FRAMES
        {
            return self.stitch(idx, AbortReason::CFunc);
        }
        if x {
            // `luaL_checktype(L, 2, LUA_TFUNCTION)`: the handler's type is guarded, anything else raises
            let t = vm.slot(a + 2).tag();
            let is_fn = vm.slot(a + 2).as_lclosure().is_some()
                || t == LUA_VLCF
                || vm.slot(a + 2).as_cclosure().is_some();
            if !is_fn {
                return Err(self.nyi());
            }
            self.rd(vm, a + 2)?;
        }
        let shift = if x { 4 } else { 2 } + usize::from(mutants::active("pcall_shift_off"));
        let nf = nargs - need;
        let func_rel = self.frames[d].base_off + a;
        let nat_top = func_rel + 1 + nargs;
        let fslot = func_rel + shift;
        let base_rel = fslot + 1;
        let top_rel = base_rel + nf;
        let nslots = usize::from(p.maxstacksize);
        self.need_ext(base_rel + nslots.max(nf) + 1)?;
        let low = self.low_for(&p)?;
        if !self.deps.iter().any(|k| k.0 == key) {
            self.deps.push(ProtoKey(key));
            self.want_dep.push(p.clone());
        }
        // identity of the native and of `f` (entry guards when TR6d allows)
        let (cr, _) = self.rd(vm, a)?;
        let kb = self.b.ir.konst(KVal::Ptr(addr as u64, PtrKind::Builtin));
        if !mutants::active("callee_guard_skip") {
            self.entry_guard(TOp::Eqref, Ty::I64, cr, kb, 0)?;
        }
        let (fr, _) = self.rd(vm, a + 1)?;
        let kp = self.b.ir.konst(KVal::Ptr(key as u64, PtrKind::Proto));
        if !mutants::active("callee_guard_skip") {
            self.entry_guard(TOp::Eqref, Ty::I64, fr, kp, 0)?;
        }
        // the C stack: `pcall_with` -> `ccall` adds one level
        if !mutants::active("pcall_ccalls_skip") {
            let k1 = self.kint(1);
            self.lib_op(lib::CCALLS, Ty::Int, k1, k1, None, usize::MAX)?;
        }
        // TR6c / TR6b: the native's frame (`LUA_MINSTACK` free slots, one `CallInfo`), `base_pcall`'s `check_stack`, then the
        // callee's frame (`stack_size - top > maxstacksize`, one more `CallInfo`)
        if !mutants::active("native_room_skip") {
            self.room = self.room.max((nat_top + LUA_MINSTACK) as u32);
        }
        if !mutants::active("pcall_room_skip") {
            self.room = self.room.max((top_rel + nslots) as u32);
        }
        if !mutants::active("native_ci_skip") {
            self.max_ci = self.max_ci.max((self.ci_depth(d) + 2) as u8);
        }
        // the state a guard of the body restores: before this `CALL`
        let ctx = PcallCtx {
            sm: self.sm.clone(),
            frames: self.frame_recs(),
            top: self.top_code(),
            pc: self.cur_pc,
            step: self.path.len() as u32 - 1,
            low: self.frames[d].low,
            snap: self.cur_snap,
            scan: self.b.ir.ins.len(),
            impure: false,
        };
        // the arguments of `f`, read before the native moves them up
        let mut args = Vec::with_capacity(nf);
        for j in 0..nf {
            args.push(self.rd_slot(vm, func_rel + 1 + need + j)?.0);
        }
        let tr = self.b.ir.konst(KVal::Pri(Ty::True));
        let nil = self.b.ir.konst(KVal::Pri(Ty::Nil));
        self.first_store.get_or_insert(self.b.ir.ins.len());
        self.def_abs(fslot - 1, tr);
        self.def_abs(fslot, fr);
        for (j, r) in args.into_iter().enumerate() {
            self.def_abs(base_rel + j, r);
        }
        for j in nf..usize::from(p.numparams) {
            self.def_abs(base_rel + j, nil);
        }
        self.pcall = Some(ctx);
        self.frames.push(RecFrame {
            ci: 0,
            func_abs: vm.base + a + shift,
            proto: key,
            closure: cl.as_ptr() as usize,
            base_off: base_rel,
            nslots,
            call_pc: idx as u32,
            nresults: -1,
            low,
            cref: fr,
            tail: false,
            vpending: None,
            vdelta: 0,
            nextra: 0,
            pc_shift: shift,
            pc_wanted: wanted,
            mm: None,
            ectx: None,
            tfor: None,
        });
        self.next_depth = d + 1;
        Ok(0)
    }

    /// P-O: the callee of an inlined protected call returns `R[a .. a + nret]`: `RETURN` puts them at its function slot, then
    /// `finishpcall` returns `true` and the results (the top `1 + nret` values of the native's frame, which already lie in
    /// place) and `poscall` moves them to the native's function slot, adjusted to the results the `CALL` wants.
    fn pcall_ret(&mut self, vm: &Vm<'_>, a: usize, nret: usize) -> R<usize> {
        let d = self.cur_depth;
        let (fb, shift, wanted, call_pc) = {
            let f = &self.frames[d];
            (f.base_off, f.pc_shift, f.pc_wanted, f.call_pc)
        };
        let fslot = fb - 1;
        let dest = fslot - shift;
        let mut src = Vec::with_capacity(nret + 1);
        let tr = self.sm.cur[fslot - 1];
        src.push(if tr == NOREF {
            self.b.ir.konst(KVal::Pri(Ty::True))
        } else {
            tr
        });
        for j in 0..nret {
            src.push(self.rd_slot(vm, fb + a + j)?.0);
        }
        self.frames.pop();
        self.next_depth = d - 1;
        self.pcall = None;
        self.need_ext(fslot + nret + 1)?;
        for j in 0..nret {
            self.def_abs(fslot + j, src[1 + j]);
        }
        let total = nret + 1;
        let nmove = if wanted < 0 { total } else { wanted as usize };
        self.need_ext(dest + nmove.max(total) + 1)?;
        let nil = self.b.ir.konst(KVal::Pri(Ty::Nil));
        for k in 0..nmove {
            let r = src.get(k).copied().unwrap_or(nil);
            self.def_abs(dest + k, r);
        }
        if wanted < 0 {
            self.top_rel = Some(dest + total);
        }
        Ok(call_pc as usize + 1)
    }

    /// `table.pack(...)` with the argument count known at record time (P-E2): the callee-identity guard on the registered
    /// `tpack`, then what `tpack` does as IR: `TNEW` with array size `n` and a one-node hash part
    /// (`new_table_sized(n, 1)`), `n` array stores, and the field `n` inserted through `NEWREF` + `HSTORE` into the free
    /// node. The key string must be a constant of the current prototype (it is kept alive by the traces' dependency on
    /// it; the recorder never creates or pins a string), otherwise the call stays a stitching C call. The native pushes
    /// the table above its arguments and the call moves it to the function slot (TR3); the native frame needs
    /// `LUA_MINSTACK` free slots and a spare `CallInfo`. An allocation the interpreter would have to step the collector
    /// for (or that fails) exits before the `CALL` (the guard on `TNEW`), tier 1 then runs the real function.
    fn pack_call(
        &mut self,
        vm: &Vm<'_>,
        idx: usize,
        a: usize,
        nargs: usize,
        wanted: i32,
        addr: usize,
    ) -> R<usize> {
        let _ = idx;
        if nargs > MAX_PACK || wanted > 1 {
            return self.stitch(idx, AbortReason::CFunc);
        }
        // the key "n": a short-string constant of the current prototype
        let Some(nk) = vm.k.iter().find_map(|v| {
            (ty_of_value(v) == Some(Ty::ShrStr))
                .then(|| v.as_strref())
                .flatten()
                .filter(|s| s.get().as_bytes() == b"n")
        }) else {
            return self.stitch(idx, AbortReason::CFunc);
        };
        let nkv =
            vm.k.iter()
                .find(|v| v.as_strref().is_some_and(|s| s.get().as_bytes() == b"n"))
                .copied()
                .ok_or(AbortReason::Diverge)?;
        if self.elems.saturating_add(nargs) > self.cfg.max_model_elems {
            return Err(AbortReason::Charge);
        }
        let d = self.cur_depth;
        let func_rel = self.frames[d].base_off + a;
        let top_rel = func_rel + 1 + nargs;
        self.need_ext(top_rel + 1)?;
        let (cr, _) = self.rd(vm, a)?;
        let kb = self.b.ir.konst(KVal::Ptr(addr as u64, PtrKind::Builtin));
        if !mutants::active("callee_guard_skip") {
            self.entry_guard(TOp::Eqref, Ty::I64, cr, kb, 0)?;
        }
        let mut vals = Vec::with_capacity(nargs);
        for j in 0..nargs {
            vals.push(self.rd_slot(vm, func_rel + 1 + j)?);
        }
        self.first_store.get_or_insert(self.b.ir.ins.len());
        // `new_table_sized(n, 1)`: array part `n`, hash part of one node (NEWTABLE's B = log2(1) + 1)
        let asize = nargs as u32 + u32::from(mutants::active("pack_asize_off"));
        let t = self
            .b
            .emit_flags(TOp::Tnew, Ty::Tab, 0, 1 as Ref, NOREF, asize)?;
        let z = self.kint(0);
        if !mutants::active("tnew_ne_guard_skip") {
            self.guard(TOp::Ne, Ty::I64, t, z, 0)?;
        }
        self.post_slot = Some(vm.base + a);
        if nargs > 0 {
            let arr = self.fload(t, Field::Array, Ty::P64)?;
            for (j, v) in vals.iter().enumerate() {
                let u = self.kint(j as i64);
                #[cfg(rustynail_lua54)]
                if !mutants::active("atouch_skip") {
                    self.ins(TOp::Atouch, Ty::Nil, t, u)?;
                }
                let aref = self.ins(TOp::Aref, Ty::P64, arr, u)?;
                self.ins(TOp::Astore, Ty::I64, aref, v.0)?;
                if collectable(v.1) {
                    self.ins(TOp::Tbar, Ty::I64, t, v.0)?;
                }
            }
        }
        // `raw_set_str(t, "n", nargs)`: the key is absent from the fresh table, whose one node is free
        let key = self
            .b
            .ir
            .konst(KVal::Ptr(payload_bits(&nkv, Ty::ShrStr), PtrKind::Raw));
        let nr = self.ins(TOp::Newref, Ty::P64, t, key)?;
        if !mutants::active("newref_ne_guard_skip") {
            self.guard(TOp::Ne, Ty::I64, nr, z, 0)?;
        }
        self.newref_keys.push_back(nk);
        if !self.cfg.lockstep {
            self.ls_tabs.push_back(None);
        }
        if TraceIr::is_ins(t) && !mutants::active("nr_img_record_skip") {
            self.nr_img.insert(t, None);
        }
        self.newref_exp.push_back(None);
        let cnt = self.kint(nargs as i64 + i64::from(mutants::active("pack_n_off")));
        self.ins(TOp::Hstore, Ty::I64, nr, cnt)?;
        // TR6c / TR6b for the native frame; the native's push (the table) lands above its arguments
        if !mutants::active("native_room_skip") {
            self.room = self.room.max((top_rel + LUA_MINSTACK) as u32);
        }
        if !mutants::active("native_ci_skip") {
            self.max_ci = self.max_ci.max((self.ci_depth(d) + 1) as u8);
        }
        self.def_abs(top_rel, t);
        // what the native leaves above its own push (scratch of `new_table_sized` / `raw_set_*`) is not modelled: stale
        for slot in top_rel + 1..self.sm.len() {
            self.stale.insert(slot);
        }
        let nil = self.b.ir.konst(KVal::Pri(Ty::Nil));
        let nmove = if wanted < 0 { 1 } else { wanted as usize };
        self.need_ext(func_rel + nmove.max(1) + 1)?;
        for j in 0..nmove {
            self.def_abs(func_rel + j, if j == 0 { t } else { nil });
        }
        if wanted < 0 {
            self.top_rel = Some(func_rel + 1);
        }
        Ok(idx + 1)
    }

    /// The `math` intrinsics `sqrt abs floor ceil min max` (TR12), called with the fixed argument count (1; `min` /
    /// `max`: 2): guards on the callee (`EQREF callee, KPTR builtin`, an entry guard when TR6d allows) and the
    /// argument types, then typed IR. The interpreter's native call pushes its result above the arguments and moves it
    /// to the function slot, so besides the results the slot `func + 1 + nargs` holds a copy (TR3); the native frame
    /// needs `LUA_MINSTACK` free slots and a spare `CallInfo` (TR6c / TR6b, mutants `native_room_skip` /
    /// `native_ci_skip`).
    #[allow(clippy::too_many_arguments)]
    fn intrinsic(
        &mut self,
        vm: &Vm<'_>,
        idx: usize,
        a: usize,
        nargs: usize,
        wanted: i32,
        kind: u8,
        addr: usize,
    ) -> R<usize> {
        use jit_hooks::intr as I;
        let two = matches!(kind, I::MIN | I::MAX);
        // F1: `min` / `max` of three or more arguments fold pairwise (`math_min`: `imin` starts from the first argument)
        let nary = two && self.cfg.f1math && nargs >= 2;
        if nargs != if two { 2 } else { 1 } && !nary {
            return Err(self.nyi()); // fewer arguments raise, more are ignored: not recorded
        }
        let d = self.cur_depth;
        let func_rel = self.frames[d].base_off + a;
        let top_rel = func_rel + 1 + nargs;
        self.need_ext(top_rel + 1)?;
        let (cr, _) = self.rd(vm, a)?;
        let kb = self.b.ir.konst(KVal::Ptr(addr as u64, PtrKind::Builtin));
        if !mutants::active("callee_guard_skip") {
            self.entry_guard(TOp::Eqref, Ty::I64, cr, kb, 0)?;
        }
        let x = self.rd(vm, a + 1)?;
        let vx = vm.slot(a + 1);
        let y = if two {
            Some((self.rd(vm, a + 2)?, vm.slot(a + 2)))
        } else {
            None
        };
        let mut more: Vec<((Ref, Ty), Value)> = Vec::new();
        if nary {
            for k in 3..=nargs {
                more.push((self.rd(vm, a + k)?, vm.slot(a + k)));
            }
        }
        let num = |t: Ty| matches!(t, Ty::Int | Ty::Num);
        if !num(x.1) || y.is_some_and(|(y, _)| !num(y.1)) || more.iter().any(|(m, _)| !num(m.1)) {
            return Err(self.non_number(x.1, y.map_or(Ty::Int, |(y, _)| y.1)));
        }
        let (res, rty): (Ref, Ty) = match kind {
            I::SIN | I::COS | I::EXP | I::LOG => {
                // F1: a pure helper call (the same `f64` function as the library's, no call frame)
                let xf = self.num_of(x.0, x.1)?;
                let which = match kind {
                    I::SIN => interp::CALLN_SIN,
                    I::COS => interp::CALLN_COS,
                    I::EXP => interp::CALLN_EXP,
                    _ => interp::CALLN_LOG,
                };
                (self.b.emit(TOp::Calln, Ty::Num, xf, NOREF, which)?, Ty::Num)
            }
            I::SQRT => {
                let xf = self.num_of(x.0, x.1)?;
                (self.ins(TOp::Fsqrt, Ty::Num, xf, NOREF)?, Ty::Num)
            }
            I::ABS => match x.1 {
                Ty::Num => (self.ins(TOp::Fabs, Ty::Num, x.0, NOREF)?, Ty::Num),
                _ => {
                    // `wrapping_abs`: the observed sign is guarded (the IR has no integer abs)
                    let z = self.kint(0);
                    if vx.as_int().unwrap_or(0) >= 0 {
                        self.guard(TOp::Ge, Ty::I64, x.0, z, 0)?;
                        (x.0, Ty::Int)
                    } else {
                        self.guard(TOp::Lt, Ty::I64, x.0, z, 0)?;
                        (self.ins(TOp::Neg, Ty::Int, x.0, NOREF)?, Ty::Int)
                    }
                }
            },
            I::FLOOR | I::CEIL => match x.1 {
                Ty::Int => (x.0, Ty::Int),
                _ => {
                    let f = vx.as_float().unwrap_or(f64::NAN);
                    let obs = if kind == I::FLOOR {
                        f.floor()
                    } else {
                        f.ceil()
                    };
                    if float_to_int(obs, F2I::Eq).is_none() {
                        return Err(self.nyi()); // `pushnumint` would push a float: not recorded
                    }
                    let fl = if self.cfg.f1floor
                        && (kind == I::FLOOR || mutants::active("f1_ceil_as_floor"))
                    {
                        self.ins(TOp::Ffloor, Ty::Num, x.0, NOREF)?
                    } else if self.cfg.f1floor {
                        // ceil(x) = -floor(-x), exactly
                        let n = self.ins(TOp::Fneg, Ty::Num, x.0, NOREF)?;
                        let q = self.ins(TOp::Ffloor, Ty::Num, n, NOREF)?;
                        self.ins(TOp::Fneg, Ty::Num, q, NOREF)?
                    } else if kind == I::FLOOR {
                        self.b
                            .emit(TOp::Calln, Ty::Num, x.0, NOREF, interp::CALLN_FLOOR)?
                    } else {
                        // ceil(x) = -floor(-x), exactly
                        let n = self.ins(TOp::Fneg, Ty::Num, x.0, NOREF)?;
                        let q = self
                            .b
                            .emit(TOp::Calln, Ty::Num, n, NOREF, interp::CALLN_FLOOR)?;
                        self.ins(TOp::Fneg, Ty::Num, q, NOREF)?
                    };
                    (self.guard(TOp::F2iExact, Ty::Int, fl, NOREF, 0)?, Ty::Int)
                }
            },
            _ => {
                let (y, vy) = y.expect("two arguments");
                // (accumulator, observed value) after each argument: `max = (acc < next) ? next : acc` (`min`: `next < acc`)
                let mut acc = (x.0, x.1, vx);
                let mut rest = vec![(y, vy)];
                rest.extend(more.iter().copied());
                for (nx, vnx) in rest {
                    if acc.1 != nx.1 {
                        return Err(self.nyi()); // mixed integer / float: not recorded
                    }
                    if acc.1 == Ty::Num {
                        // Lua: min = (b < a) ? b : a, max = (a < b) ? b : a; `Fmin` / `Fmax` return their *second* operand
                        // unless the first is strictly smaller / larger, so the operands are passed as (b, a) (J4 M4)
                        let op = if kind == I::MIN { TOp::Fmin } else { TOp::Fmax };
                        let r = if mutants::active("f1_minmax_order") {
                            self.ins(op, Ty::Num, acc.0, nx.0)?
                        } else {
                            self.ins(op, Ty::Num, nx.0, acc.0)?
                        };
                        let (fa, fb) = (
                            acc.2.as_float().unwrap_or(f64::NAN),
                            vnx.as_float().unwrap_or(f64::NAN),
                        );
                        let take_b = if kind == I::MIN { fb < fa } else { fa < fb };
                        acc = (r, Ty::Num, if take_b { vnx } else { acc.2 });
                    } else {
                        let (xi, yi) = (acc.2.as_int().unwrap_or(0), vnx.as_int().unwrap_or(0));
                        // min: take b iff b < a; max: take b iff a < b
                        let take_b = if kind == I::MIN { yi < xi } else { xi < yi };
                        let (lo, hi) = if kind == I::MIN {
                            (nx.0, acc.0)
                        } else {
                            (acc.0, nx.0)
                        };
                        if take_b {
                            self.guard(TOp::Lt, Ty::I64, lo, hi, 0)?;
                            acc = (nx.0, Ty::Int, vnx);
                        } else {
                            self.guard(TOp::Ge, Ty::I64, lo, hi, 0)?;
                        }
                    }
                }
                (acc.0, acc.1)
            }
        };
        let _ = rty;
        // TR6c / TR6b for the native frame
        if !mutants::active("native_room_skip") {
            self.room = self.room.max((top_rel + LUA_MINSTACK) as u32);
        }
        if !mutants::active("native_ci_skip") {
            self.max_ci = self.max_ci.max((self.ci_depth(d) + 1) as u8);
        }
        // effects (after every guard): the native's push, then the moved results
        let nil = self.b.ir.konst(KVal::Pri(Ty::Nil));
        self.def_abs(top_rel, res);
        // F1: the copy above the result is dead; a collector step in the native (a string allocated by the next call, `luaD_shrinkstack`'s
        // clearing of the dead slice) may nil it in the VM, so the lockstep leaves it out
        self.stale.insert(top_rel);
        let nmove = if wanted < 0 { 1 } else { wanted as usize };
        for j in 0..nmove {
            self.def_abs(func_rel + j, if j == 0 { res } else { nil });
        }
        if wanted < 0 {
            self.top_rel = Some(func_rel + 1); // F4: `post_call` leaves `top` after the one result
        }
        Ok(idx + 1)
    }

    /// `CONCAT A B` of two or three operands that are non-empty strings or numbers (F3b). The interpreter converts the
    /// numbers in place, from the top down (`luaO_tostring`: an interned short string each), then builds one string from
    /// all operands. The recording does the same with class-A helpers: a `SNEW` per number operand (the converted string
    /// replaces the operand's register), then one `CONCAT` of the strings, each with the "nothing done: exit" guard on the
    /// snapshot of the instruction. The interpreter checks the collector once, behind the whole instruction; each helper
    /// checks that its own allocation fits, and the totals only grow, so the last helper's check is the interpreter's. An
    /// exit after an earlier helper allocated leaves interned strings that the interpreter's re-execution finds again.
    fn concat(&mut self, vm: &Vm<'_>, idx: usize, i: Instruction) -> R<usize> {
        let a = i.a() as usize;
        let n = i.b() as usize;
        if !(2..=3).contains(&n) {
            return Err(self.nyi());
        }
        if self.cfg.mm && n == 2 && (vm.slot(a).is_table() || vm.slot(a + 1).is_table()) {
            return self.mm_concat(vm, idx, a);
        }
        let mut ops = [(NOREF, Ty::Nil); 3];
        let mut tl = 0usize;
        for (j, op) in ops.iter_mut().enumerate().take(n) {
            *op = self.rd(vm, a + j)?;
            let v = vm.slot(a + j);
            let l = match op.1 {
                Ty::ShrStr | Ty::LngStr => v.as_strref().ok_or(AbortReason::Diverge)?.get().len(),
                Ty::Int => {
                    rn_core::number::tostring(rn_core::number::Number::Int(v.as_int().unwrap_or(0)))
                        .as_bytes()
                        .len()
                }
                Ty::Num => rn_core::number::tostring(rn_core::number::Number::Float(
                    v.as_float().unwrap_or(0.0),
                ))
                .as_bytes()
                .len(),
                _ => return Err(AbortReason::Metamethod), // `__concat`, or an error
            };
            if l == 0 || l > (1 << 40) {
                return Err(self.nyi()); // an empty string: the interpreter returns another operand (its general path)
            }
            tl += l;
        }
        // (the model never holds a string's bytes: the lockstep takes the strings' addresses from the VM)
        let nconv = if self.cfg.cnum {
            0
        } else {
            ops[..n]
                .iter()
                .filter(|o| matches!(o.1, Ty::Int | Ty::Num))
                .count()
        };
        for _ in 0..=nconv {
            self.storm_check()?;
        }
        self.first_store.get_or_insert(self.b.ir.ins.len());
        let z = self.kint(0);
        let mut sref = [ops[0].0, ops[1].0, ops[2].0];
        let mut converted = [false; 3];
        let mut hints: Vec<usize> = Vec::new();
        for j in (0..n).rev() {
            // (B3-4 `cnum`: the `CONCAT` helper formats the number operands itself)
            if matches!(ops[j].1, Ty::Int | Ty::Num) && !self.cfg.cnum {
                let p = self
                    .b
                    .emit_flags(TOp::Snew, Ty::ShrStr, 0, ops[j].0, NOREF, 0)?;
                if !mutants::active("snew_ne_guard_skip") {
                    self.guard(TOp::Ne, Ty::I64, p, z, 0)?;
                }
                sref[j] = p;
                converted[j] = true;
                // R[A] is overwritten by the result: its converted string is unobservable
                hints.push(if j == 0 { usize::MAX } else { vm.base + a + j });
            }
        }
        let rty = if tl > rn_core::string::MAX_SHORT_LEN {
            Ty::LngStr
        } else {
            Ty::ShrStr
        };
        let third = if n == 3 { u32::from(sref[2]) } else { 0 };
        let c = self
            .b
            .emit_flags(TOp::Concat, rty, 0, sref[0], sref[1], third)?;
        if !mutants::active("concat_ne_guard_skip") {
            self.guard(TOp::Ne, Ty::I64, c, z, 0)?;
        }
        hints.push(vm.base + a);
        self.pend_str = hints;
        let a_root = self.frames[self.cur_depth].base_off + a;
        for j in 1..n {
            if converted[j] && !mutants::active("concat_conv_slot_skip") {
                self.def(a + j, sref[j]);
            }
        }
        self.def(a, c);
        // the interpreter's `check_gc` right behind runs with `top = A + 1` and clears the dead slice above it when it steps
        // (GC stress): like `NEWTABLE`, the lockstep does not compare those slots
        for slot in a_root + 1..self.sm.len() {
            self.stale.insert(slot);
        }
        Ok(idx + 1)
    }

    /// `TFORCALL` (+ the `TFORLOOP` the interpreter runs unfetched right behind it, F3b): the generic `for` whose iterator
    /// is the `ipairs` iterator or `next` (the builtin identities of `jit_hooks`). The call is recorded the way the
    /// interpreter performs it, *with* its native frame (the copies of iterator / state / control above the loop
    /// registers, the native's pushes above its arguments, the results moved down), so that the lockstep and the exit
    /// snapshots see the interpreter's stack. 5.5 layout: `R[A]` iterator, `R[A+1]` state, `R[A+2]` closing value,
    /// `R[A+3]` control variable = first variable; the call is made at `A+3`. 5.4 (TR30): `R[A+2]` control, `R[A+3]`
    /// closing value, the call at `A+4`, and `TFORLOOP` copies the first variable back to `R[A+2]`.
    fn tforcall(&mut self, vm: &Vm<'_>, idx: usize, i: Instruction, prep: bool) -> R<usize> {
        let a = i.a() as usize;
        let nvars = i.c() as usize;
        let ni = Instruction(*vm.code.get(idx + 1).ok_or(AbortReason::Diverge)?);
        if ni.opcode_raw() != OpCode::Tforloop as u8 || ni.a() as usize != a || nvars == 0 {
            return Err(AbortReason::Diverge);
        }
        // (`prep`, N7: the first call of the loop, chained behind `TFORPREP`, which in 5.5 has not swapped the control and the
        // closing value yet: the control is still in `R[A+2]`)
        let (fslot, cslot) = if cfg!(rustynail_lua54) {
            (a + 4, a + 2)
        } else {
            (a + 3, if prep { a + 2 } else { a + 3 })
        };
        let fv = vm.slot(a);
        let addr = if fv.tag() == LUA_VLCF {
            fv.as_native().map_or(0, |f| f as usize)
        } else {
            0
        };
        let is_ipairs = addr != 0 && addr == jit_hooks::builtin_ipairs_aux();
        let is_next = addr != 0 && addr == jit_hooks::builtin_next();
        if let Some(cl) = fv.as_lclosure() {
            if self.cfg.gfor_lua {
                return self.tforcall_lua(vm, idx, a, fslot, cslot, nvars, ni.bx(), cl);
            }
        }
        if self.cfg.gmatch
            && fv
                .as_cclosure()
                .is_some_and(|c| c.f as usize == rn_std::lib_string::jit_gmatch_addr())
        {
            return self.tforcall_gmatch(vm, idx, i, fslot, cslot, prep);
        }
        if !is_ipairs && !is_next {
            return Err(if fv.as_lclosure().is_some() {
                self.nyi()
            } else {
                AbortReason::CFunc
            });
        }
        // N7: the end of a traversal is recorded by the side recordings of the root frame (the exit of the loop's trace lands on this
        // `TFORCALL`, the recording continues behind the loop); a root recording keeps `LeaveLoop`. (5.5 only: 5.4's `alimit` touches
        // of the interpreter's `get_int` are not modelled by the helper.)
        let can_end = self.cfg.gfor
            && ((self.cfg.side.is_some() && self.cur_depth == 0)
                || (self.cfg.gfor_inl && self.unrolling()))
            && !cfg!(rustynail_lua54);
        let d = self.cur_depth;
        let func_rel = self.frames[d].base_off + fslot;
        let top_rel = func_rel + 3; // the native's frame: function, state, control
        self.need_ext(top_rel + 2 + 1)?;
        let (cr, _) = self.rd(vm, a)?;
        let kb = self.b.ir.konst(KVal::Ptr(addr as u64, PtrKind::Builtin));
        if !mutants::active("tfor_callee_guard_skip") && !(TraceIr::is_const(cr) && cr == kb) {
            self.entry_guard(TOp::Eqref, Ty::I64, cr, kb, 0)?;
        }
        let st = self.rd(vm, a + 1)?;
        let ct = self.rd(vm, cslot)?;
        if st.1 != Ty::Tab {
            return Err(AbortReason::Metamethod); // `ipairs` of another type indexes through metamethods; `next` raises
        }
        let mut ended = false;
        let (key, val): (Ref, Ref) = if is_ipairs {
            if ct.1 != Ty::Int {
                return Err(self.nyi());
            }
            let kv = vm.slot(cslot).as_int().unwrap_or(0).wrapping_add(1);
            let tv = vm.slot(a + 1);
            let tbl = tv.as_table().ok_or(AbortReason::Diverge)?;
            if tbl.metatable().is_some() {
                return Err(AbortReason::MetaTable);
            }
            let inside = (1..=i64::from(tbl.array_size())).contains(&kv);
            if !inside && !tbl.get_int(kv).is_nil() {
                return Err(AbortReason::TableHash);
            }
            if !inside || tbl.peek_int_arr(kv).is_none_or(|e| e.is_nil()) {
                if !can_end {
                    return Err(AbortReason::LeaveLoop); // the iteration ends here
                }
                // N7: `t[i + 1]` is nil: no metatable (`__index` would be consulted) and the raw integer-key lookup misses
                // (a nil array slot or an absent hash entry: a helper, once per traversal)
                if !mutants::active("gfor_end_meta_skip") {
                    let mt = self.fload(st.0, Field::Metatable, Ty::P64)?;
                    let z = self.kint(0);
                    self.guard(TOp::Eq, Ty::I64, mt, z, 0)?;
                }
                let one = self.kint(1);
                let k1 = self.ins(TOp::Add, Ty::Int, ct.0, one)?;
                if !mutants::active("gfor_end_guard_skip") {
                    self.lib_op(lib::IPAIRS_END, Ty::Int, st.0, k1, None, usize::MAX)?;
                }
                ended = true;
                (k1, k1)
            } else {
                let one = self.kint(1);
                let k1 = self.ins(TOp::Add, Ty::Int, ct.0, one)?;
                let v = self.array_get(vm, st, a + 1, k1, kv)?;
                (k1, v)
            }
        } else {
            let cv = vm.slot(cslot);
            let supported = |t: Ty| {
                matches!(
                    t,
                    Ty::Int | Ty::Num | Ty::ShrStr | Ty::LngStr | Ty::Tab | Ty::True | Ty::False
                )
            };
            // (a nil control is the first call, chained behind `TFORPREP`)
            if !supported(ct.1) && !(self.cfg.gfor && ct.1 == Ty::Nil) {
                return Err(self.nyi()); // a type the helper cannot pass
            }
            let tbl = vm.slot(a + 1).as_table().ok_or(AbortReason::Diverge)?;
            let (k, v) = match tbl.next(&cv) {
                Ok(Some(kv)) => kv,
                Ok(None) if can_end => (Value::NIL, Value::NIL), // N7: the traversal ends here
                Ok(None) => return Err(AbortReason::LeaveLoop),
                Err(_) => return Err(AbortReason::Error),
            };
            ended = k.is_nil();
            let (tk, tv) = match (ty_of_value(&k), ty_of_value(&v)) {
                (Some(tk), Some(tv)) if (supported(tk) || ended) && (supported(tv) || ended) => {
                    (tk, tv)
                }
                _ => return Err(self.nyi()),
            };
            self.touch(&k)?;
            self.touch(&v)?;
            let mv = (self.val_of(&k), self.val_of(&v));
            let z = self.kint(0);
            let p = self.ins(TOp::Tnext, Ty::P64, st.0, ct.0)?;
            if !mutants::active("tnext_ne_guard_skip") {
                self.guard(TOp::Ne, Ty::I64, p, z, 0)?;
            }
            self.pending_next = Some(mv);
            let kbits = if mutants::active("tnext_key_type_skip") {
                0
            } else {
                1
            };
            let kr = self.guard(TOp::Hload, tk, p, NOREF, kbits | 2)?;
            if ended {
                (kr, kr)
            } else {
                let vr = self.guard(TOp::Hload, tv, p, NOREF, 1)?;
                (kr, vr)
            }
        };
        // effects (after every guard): the copies, the native's pushes, the moved results
        if !mutants::active("native_room_skip") {
            self.room = self.room.max((top_rel + LUA_MINSTACK) as u32);
        }
        if !mutants::active("native_ci_skip") {
            self.max_ci = self.max_ci.max((self.ci_depth(d) + 1) as u8);
        }
        let nil = self.b.ir.konst(KVal::Pri(Ty::Nil));
        let args = (self.rd(vm, a + 1)?.0, ct.0);
        self.def_abs(func_rel + 2, args.1);
        self.def_abs(func_rel + 1, args.0);
        self.def_abs(func_rel, cr);
        if ended {
            // the iterator pushed one `nil` and returned it: every variable is nil, `TFORLOOP` falls through
            self.def_abs(top_rel, nil);
            for j in 0..nvars {
                self.def_abs(func_rel + j, nil);
            }
            return Ok(idx + 2);
        }
        self.def_abs(top_rel, key);
        self.def_abs(top_rel + 1, val);
        for j in 0..nvars {
            self.def_abs(
                func_rel + j,
                match j {
                    0 => key,
                    1 => val,
                    _ => nil,
                },
            );
        }
        if cfg!(rustynail_lua54) && !mutants::active("tfor54_no_ctl_copy") {
            self.def(a + 2, key); // TFORLOOP: the control variable
        }
        self.target(idx, idx as isize + 2 - ni.bx() as isize)
    }

    /// BUNDLE-GM: `TFORCALL` of a `gmatch` iterator (`gmatch_aux`, a C closure) whose step yields exactly one string (the whole match or one
    /// string capture): the `GMATCH` helper does the step (and advances the closure's position upvalues, so nothing may be forwarded across
    /// it: it is a `Lib` op, never hoisted or merged), the call is recorded with its native frame like the `ipairs` one. The end of the
    /// iteration, several or position captures and pattern errors are not recorded (`LeaveLoop` / `Nyi`: the interpreter's step raises or
    /// finishes the loop; at run time the helper answers 0 for them and the trace exits before the call).
    fn tforcall_gmatch(
        &mut self,
        vm: &Vm<'_>,
        idx: usize,
        i: Instruction,
        fslot: usize,
        cslot: usize,
        prep: bool,
    ) -> R<usize> {
        let _ = prep;
        let a = i.a() as usize;
        let nvars = i.c() as usize;
        let ni = Instruction(*vm.code.get(idx + 1).ok_or(AbortReason::Diverge)?);
        let fv = vm.slot(a);
        let mut len = None;
        match rn_std::lib_string::jit_gmatch_step(&fv, false, &mut |b| {
            len = Some(b.len());
            true
        }) {
            rn_std::lib_string::GmStep::Yield(_) => {}
            rn_std::lib_string::GmStep::End => return Err(AbortReason::LeaveLoop),
            rn_std::lib_string::GmStep::Other => return Err(self.nyi()),
        }
        let rty = if len.unwrap_or(0) > rn_core::string::MAX_SHORT_LEN {
            Ty::LngStr
        } else {
            Ty::ShrStr
        };
        let d = self.cur_depth;
        let func_rel = self.frames[d].base_off + fslot;
        let top_rel = func_rel + 3; // the native's frame: function, state, control
        self.need_ext(top_rel + 2 + 1)?;
        let (cr, cty) = self.rd(vm, a)?;
        if cty != Ty::Ccl {
            return Err(AbortReason::Diverge);
        }
        let st = self.rd(vm, a + 1)?;
        let ct = self.rd(vm, cslot)?;
        let hint = vm.base + fslot;
        let r = self.lib_op(lib::GMATCH, rty, cr, cr, None, hint)?;
        if !mutants::active("gm_end_guard_skip") {
            let one = self.kint(1);
            self.guard(TOp::Ne, Ty::I64, r, one, 0)?;
        }
        // effects (after the guard of the helper): the copies, the native's push, the moved results
        self.def_abs(func_rel + 2, ct.0);
        self.def_abs(func_rel + 1, st.0);
        self.def_abs(func_rel, cr);
        self.lib_tail(idx, func_rel, top_rel, &[r], true, nvars as i32)?;
        if cfg!(rustynail_lua54) && !mutants::active("tfor54_no_ctl_copy") {
            self.def(a + 2, r);
        }
        self.target(idx, idx as isize + 2 - ni.bx() as isize)
    }

    /// The `TFORLOOP` the interpreter runs without a fetch right after a Lua iterator called by `TFORCALL` returned (`do_tforcall!`):
    /// the loop continues when the first variable (`first`, the iterator's first result) is not nil (5.4: the control copy
    /// `R[A+2] := R[A+4]`), else falls through. `depth` is the caller's depth (the frame is already popped). The nil test is a
    /// recorded fact: the type of `first` is known (an iterator's result is a typed load, a constant, or the result of a guarded
    /// operation), so no guard is emitted here.
    fn tfor_after_ret(
        &mut self,
        call_pc: usize,
        tf: TforRet,
        first: Ref,
        depth: usize,
    ) -> R<usize> {
        if !mutants::active("tfor_lua_ret_cost_skip") {
            if let Some(c) = self.cost.last_mut() {
                *c += 1; // the unfetched `TFORLOOP`
            }
        }
        let ended = self.b.ir.ty_of(first) == Ty::Nil;
        let saved = (self.cur_depth, self.cur_op);
        self.cur_depth = depth;
        self.cur_op = OpCode::Tforcall as u8;
        let r = if ended {
            let can_end = self.cfg.gfor
                && ((self.cfg.side.is_some() && depth == 0)
                    || (self.cfg.gfor_inl && self.unrolling()))
                && !cfg!(rustynail_lua54);
            if can_end {
                Ok(call_pc + 2)
            } else {
                Err(AbortReason::LeaveLoop)
            }
        } else {
            if cfg!(rustynail_lua54) && !mutants::active("tfor54_no_ctl_copy") {
                self.def(tf.a + 2, first); // TFORLOOP: the control variable
            }
            // (`cur_len` is the callee's code length; the target is a pc of the caller, which the interpreter validated)
            let len = std::mem::replace(&mut self.cur_len, usize::MAX);
            let r = self.target(call_pc, call_pc as isize + 2 - tf.bx as isize);
            self.cur_len = len;
            r
        };
        self.cur_depth = saved.0;
        self.cur_op = saved.1;
        r
    }

    /// A fetched `TFORLOOP`: the interpreter fetches it when a Lua iterator frame that a trace exit restored (a plain frame, not the
    /// nested `execute` of `do_tforcall!`) returns to its `TFORCALL`, which is how a side recording that starts inside the iterator
    /// reaches it. Same decision as [`Rec::tfor_after_ret`] (without the cost charge: this step is charged as a fetch).
    fn tforloop(&mut self, vm: &Vm<'_>, idx: usize, i: Instruction) -> R<usize> {
        let a = i.a() as usize;
        let slot = if cfg!(rustynail_lua54) { a + 4 } else { a + 3 };
        let (r, ty) = self.rd(vm, slot)?;
        if ty == Ty::Nil {
            let can_end = self.cfg.gfor
                && ((self.cfg.side.is_some() && self.cur_depth == 0)
                    || (self.cfg.gfor_inl && self.unrolling()))
                && !cfg!(rustynail_lua54);
            return if can_end {
                Ok(idx + 1)
            } else {
                Err(AbortReason::LeaveLoop)
            };
        }
        if cfg!(rustynail_lua54) && !mutants::active("tfor54_no_ctl_copy") {
            self.def(a + 2, r);
        }
        self.cur_op = OpCode::Tforcall as u8; // (`target`: a generic `for` back edge)
        let t = self.target(idx, idx as isize + 1 - i.bx() as isize);
        self.cur_op = OpCode::Tforloop as u8;
        t
    }

    /// `TFORCALL` of a Lua closure: the iterator is called like an ordinary call (its frame is inlined, 4.4) with the state and the
    /// control variable as arguments and `nvars` wanted results; the `TFORLOOP` behind it is recorded when the frame returns
    /// ([`Rec::tfor_after_ret`]). The interpreter runs the call in a nested `execute` (`CIST_FRESH`), which a trace never sees;
    /// an exit inside the frame restores an ordinary frame that the outer loop continues.
    #[allow(clippy::too_many_arguments)]
    fn tforcall_lua(
        &mut self,
        vm: &Vm<'_>,
        idx: usize,
        a: usize,
        fslot: usize,
        cslot: usize,
        nvars: usize,
        bx: u32,
        cl: rn_core::value::Gc<LClosure>,
    ) -> R<usize> {
        let key = &*cl.proto() as *const Proto as usize;
        if self.frames.iter().any(|f| f.proto == key) {
            return Err(self.nyi()); // recursion through the iterator: the plain call path's stitching does not apply here
        }
        let d = self.cur_depth;
        let st = self.rd(vm, a + 1)?;
        let ct = self.rd(vm, cslot)?;
        let (cr, _) = self.rd(vm, a)?;
        // the snapshot of this step is the state before the copies below
        self.cur_snapshot()?;
        // the interpreter copies function, state and control above the loop registers, then calls with two arguments
        self.def(fslot, cr);
        self.def(fslot + 1, st.0);
        self.def(fslot + 2, ct.0);
        self.call_lua(vm, idx, fslot, 2, nvars as i32, cl, false)?;
        if self.stitched || self.next_depth != d + 1 {
            return Err(AbortReason::Diverge);
        }
        if let Some(c) = self.cost.last_mut() {
            *c -= 1; // `step_cost` charges the unfetched `TFORLOOP` here; it is charged at the return instead
        }
        if let Some(f) = self.frames.last_mut() {
            f.tfor = Some(TforRet { a, bx });
        }
        Ok(0)
    }

    /// N7: `TFORPREP` (side recordings and nest roots, root frame): the closing value must be nil / false (`new_tbc_upval` does
    /// nothing), 5.5 swaps the control and the closing value, then the interpreter runs the first `TFORCALL` (and the `TFORLOOP` behind
    /// it) without another dispatch: one recorded step. Guards of the call see the state *before* `TFORPREP` (the snapshot of this
    /// step); the swap is an effect after them.
    fn tforprep(&mut self, vm: &Vm<'_>, idx: usize, i: Instruction) -> R<usize> {
        let a = i.a() as usize;
        let call = idx + 1 + i.bx() as usize;
        let ci = Instruction(*vm.code.get(call).ok_or(AbortReason::Diverge)?);
        if ci.opcode_raw() != OpCode::Tforcall as u8 || ci.a() as usize != a {
            return Err(AbortReason::Diverge);
        }
        let (clr, clty) = self.rd(vm, a + 3)?;
        if !matches!(clty, Ty::Nil | Ty::False) {
            return Err(self.nyi()); // a to-be-closed value
        }
        let r = self.tforcall(vm, call, ci, true)?;
        // 5.5: `R[A+2]` now holds the closing value (`R[A+3]` was overwritten by the call). 5.4 swaps nothing. (The mutant leaves the
        // swap out on 5.5 and adds it on 5.4.)
        if cfg!(rustynail_lua54) == mutants::active("tforprep_no_swap") {
            self.def(a + 2, clr);
        }
        Ok(r)
    }

    /// `FORLOOP`. 5.5 layout: `R[A]` count / limit, `R[A+1]` step, `R[A+2]` control variable. 5.4 layout (TR30):
    /// `R[A]` internal index, `R[A+1]` count (integer loop) / limit (float loop), `R[A+2]` step, `R[A+3]` the visible
    /// control variable, a copy of the new index that every continuing `FORLOOP` stores (the body may overwrite it
    /// without disturbing the loop; mutant `forloop54_no_ctl_copy`).
    fn forloop(&mut self, vm: &Vm<'_>, idx: usize, i: Instruction) -> R<usize> {
        let a = i.a() as usize;
        // the (count / limit, step, index) slots of the version's layout
        let (sl_lim, sl_step, sl_ix) = if cfg!(rustynail_lua54) {
            (a + 1, a + 2, a)
        } else {
            (a, a + 1, a + 2)
        };
        let (step, ts) = self.rd(vm, sl_step)?;
        let target = idx as isize + 1 - i.bx() as isize;
        if vm.slot(sl_step).tag() == LUA_VNUMINT {
            if ts != Ty::Int {
                return Err(AbortReason::Diverge);
            }
            let (cnt, tc) = self.rd(vm, sl_lim)?;
            let (ix, ti) = self.rd(vm, sl_ix)?;
            if tc != Ty::Int || ti != Ty::Int {
                return Err(self.nyi());
            }
            if vm.slot(sl_lim).as_int().unwrap_or(0) as u64 == 0 {
                if (self.cfg.side.is_some() || self.cur_depth > 0 || self.cont_unroll())
                    && !mutants::active("side_forloop_end_abort")
                {
                    // F2b: a side trace continues behind a loop that ends (P-E: so does a loop of an inlined callee): guard `count == 0`, nothing is written
                    let z = self.kint(0);
                    if !(self.cont_unroll() && mutants::active("rec_loop_end_guard_skip")) {
                        self.guard(TOp::Eq, Ty::I64, cnt, z, 0)?;
                    }
                    return Ok(idx + 1);
                }
                return Err(AbortReason::LeaveLoop); // the loop ends here
            }
            // P7: the observed step is a constant of the trace (an `EQ` guard on the invariant step slot), so the index
            // arithmetic of consecutive iterations folds (TR24e) and array accesses of iteration `i + 1` can be proven
            // to hit (or miss) the elements of iteration `i`. In the peeled layout the guard is loop invariant.
            //
            // P-F: only when the step is a literal of the prototype (`LOADI` / `LOADK` right before the `FORPREP`, e.g. the
            // default step 1 or `for j = a, b, 2`). A step computed at run time (`for j = i * i, n, i`: the sieve) differs
            // between runs of the same loop, so a constant guard failed at the first iteration of every other run, the trace
            // never completed an iteration (Q2: `Ineffective`, then blacklisted) and its side traces ping-ponged; the
            // index then advances by the invariant `SLOAD`ed step (an integer `FORLOOP` needs no sign: the count is exact).
            let ks = vm.slot(sl_step).as_int().unwrap_or(0);
            let literal = (Self::forloop_literal_step(vm, target as usize, a + 2)
                && !mutants::active("forloop_step_always_var"))
                || mutants::active("forloop_step_always_lit");
            let kstep = if literal || mutants::active("forloop_step_guard_skip") {
                self.kint(ks)
            } else {
                step
            };
            let z = self.kint(0);
            self.guard(TOp::Ne, Ty::I64, cnt, z, 0)?;
            if literal && !mutants::active("forloop_step_guard_skip") {
                self.guard(TOp::Eq, Ty::I64, step, kstep, 0)?;
            }
            let one = self.kint(1);
            let c1 = self.ins(TOp::Sub, Ty::Int, cnt, one)?;
            let kadd = if mutants::active("forloop_step_add_wrong") {
                self.kint(ks.wrapping_add(1)) // MUTANT: the index advances by the wrong constant
            } else {
                kstep
            };
            let i1 = self.ins(TOp::Add, Ty::Int, ix, kadd)?;
            self.def(sl_lim, c1);
            self.def(sl_ix, i1);
            self.forloop_ctl_copy(a, i1);
        } else {
            let (lim, tl) = self.rd(vm, sl_lim)?;
            let (ix, ti) = self.rd(vm, sl_ix)?;
            if ts != Ty::Num || tl != Ty::Num || ti != Ty::Num {
                return Err(self.nyi());
            }
            let f = |n: usize| vm.slot(n).as_float().unwrap_or(f64::NAN);
            let (vs, vl, vi) = (f(sl_step), f(sl_lim), f(sl_ix));
            let up = 0.0 < vs;
            let idx1 = vi + vs;
            let continues = if up { idx1 <= vl } else { vl <= idx1 };
            if !continues
                && !((self.cfg.side.is_some() || self.cur_depth > 0 || self.cont_unroll())
                    && !mutants::active("side_forloop_end_abort"))
            {
                return Err(AbortReason::LeaveLoop);
            }
            let i1 = self.ins(TOp::Fadd, Ty::Num, ix, step)?;
            if !continues {
                // F2b: a side trace continues behind a float loop that ends: the step sign (invariant), then the
                // negated loop condition
                let zero = self.b.ir.knum(0.0);
                self.guard(TOp::Fcmp, Ty::I64, zero, step, 2 | u32::from(up) << 4)?;
                if up {
                    self.guard(TOp::Fcmp, Ty::I64, i1, lim, 3)?;
                } else {
                    self.guard(TOp::Fcmp, Ty::I64, lim, i1, 3)?;
                }
                return Ok(idx + 1);
            }
            // the sign of the step decides the compare (invariant guard), then the loop condition itself
            let zero = self.b.ir.knum(0.0);
            self.guard(TOp::Fcmp, Ty::I64, zero, step, 2 | u32::from(up) << 4)?;
            if up {
                self.guard(TOp::Fcmp, Ty::I64, i1, lim, 3 | 1 << 4)?;
            } else {
                self.guard(TOp::Fcmp, Ty::I64, lim, i1, 3 | 1 << 4)?;
            }
            self.def(sl_ix, i1);
            self.forloop_ctl_copy(a, i1);
        }
        self.target(idx, target)
    }

    /// `FORPREP` of an integer loop with an observed step (F2b; side traces and nest roots, root frame only): the
    /// operands are `SLOAD`ed and typechecked, the step is guarded to its observed value, the skip test is guarded in
    /// the observed direction and the iteration count is computed exactly as `forprep` does (unsigned: the guard
    /// `count >= 0` leaves for the interpreter when the difference does not fit). Float loops and a step of zero or
    /// `mininteger` are `Nyi`.
    fn forprep(&mut self, vm: &Vm<'_>, idx: usize, i: Instruction) -> R<usize> {
        let a = i.a() as usize;
        let (init, ti) = self.rd(vm, a)?;
        let (mut lim, mut tl) = self.rd(vm, a + 1)?;
        let (step, ts) = self.rd(vm, a + 2)?;
        // F3b: an integer loop whose limit is a numeral string: `forlimit` coerces it (`luaV_tointeger`)
        let mut limv = vm.slot(a + 1);
        if ti == Ty::Int && ts == Ty::Int && matches!(tl, Ty::ShrStr | Ty::LngStr) {
            let vs = vm.slot(a + 2).as_int().unwrap_or(0);
            let ceil = vs < 0;
            let mode = if ceil { F2I::Ceil } else { F2I::Floor };
            let Some(n) = rn_core::arith::to_integer(&limv, mode) else {
                return Err(self.nyi()); // not a numeral, or out of the integer range: the interpreter raises / clamps
            };
            let want = if mutants::active("strto_mode_swap") {
                !ceil
            } else {
                ceil
            };
            let p = self
                .b
                .emit_flags(TOp::Strto, Ty::P64, 0, lim, NOREF, u32::from(want))?;
            let z = self.kint(0);
            if !mutants::active("strto_ne_guard_skip") {
                self.guard(TOp::Ne, Ty::I64, p, z, 0)?;
            }
            self.pending_strto = Some(super::interp::Val::int(n));
            lim = self.guard(TOp::Hload, Ty::Int, p, NOREF, 1)?;
            tl = Ty::Int;
            limv = Value::int(n);
        }
        if ti != Ty::Int || tl != Ty::Int || ts != Ty::Int {
            return Err(self.nyi());
        }
        // P-E: a loop of an inlined callee is unrolled only when its trip count is a trace constant (e.g. `select('#', ...)`
        // of an inlined vararg frame); a data-dependent count keeps the prototype's `InnerLoop` abort
        if self.cur_depth > 0 && (init >= REF_BIAS || lim >= REF_BIAS) {
            return Err(AbortReason::InnerLoop);
        }
        let (vi, vl, vs) = (
            vm.slot(a).as_int().unwrap_or(0),
            limv.as_int().unwrap_or(0),
            vm.slot(a + 2).as_int().unwrap_or(0),
        );
        if vs == 0 || vs == i64::MIN {
            return Err(self.nyi());
        }
        // P-F: a step computed at run time (`for j = i * i, n, i`) is not a trace constant: only its sign is guarded and the
        // count divides by the magnitude (a literal step keeps the constant guard and the division by a constant)
        let literal = (idx >= 1
            && (Instruction(vm.code[idx - 1]).opcode_raw() == OpCode::Loadi as u8
                || Instruction(vm.code[idx - 1]).opcode_raw() == OpCode::Loadk as u8)
            && Instruction(vm.code[idx - 1]).a() as usize == a + 2)
            || mutants::active("forprep_step_always_lit");
        let kstep = self.kint(vs);
        let mut mag_ref = None;
        if literal {
            if !mutants::active("forprep_step_guard_skip") {
                self.guard(TOp::Eq, Ty::I64, step, kstep, 0)?;
            }
        } else {
            let z0 = self.kint(0);
            if !mutants::active("forprep_var_sign_skip")
                && !mutants::active("forprep_step_guard_skip")
            {
                self.guard(if vs > 0 { TOp::Gt } else { TOp::Lt }, Ty::I64, step, z0, 0)?;
            }
            mag_ref = Some(if vs > 0 {
                step
            } else {
                // `0 - step`; the magnitude of `mininteger` stays negative: the guard leaves for the interpreter
                let m = self.ins(TOp::Sub, Ty::Int, z0, step)?;
                self.guard(TOp::Gt, Ty::I64, m, z0, 0)?;
                m
            });
        }
        let skip = if vs > 0 { vi > vl } else { vi < vl };
        // 5.4 stores the control variable before it looks at the limit
        if cfg!(rustynail_lua54) {
            self.def(a + 3, init);
        }
        if skip {
            let op = if vs > 0 { TOp::Gt } else { TOp::Lt };
            self.guard(op, Ty::I64, init, lim, 0)?;
            return Ok(idx + i.bx() as usize + 2);
        }
        // P-F: a difference that does not fit a signed integer (`-5 .. maxinteger`) would make the `count >= 0` guard below fail
        // at record time (the interpreter runs the loop with an unsigned count): not recorded
        let diff = if vs > 0 {
            vl.wrapping_sub(vi)
        } else {
            vi.wrapping_sub(vl)
        };
        if diff < 0 && !mutants::active("forprep_huge_diff_record") {
            return Err(self.nyi());
        }
        let op = if vs > 0 { TOp::Le } else { TOp::Ge };
        self.guard(op, Ty::I64, init, lim, 0)?;
        let sub = if vs > 0 {
            self.ins(TOp::Sub, Ty::Int, lim, init)?
        } else {
            self.ins(TOp::Sub, Ty::Int, init, lim)?
        };
        let z = self.kint(0);
        self.guard(TOp::Ge, Ty::I64, sub, z, 0)?;
        let mag = vs.unsigned_abs();
        let count = if let Some(m) = mag_ref {
            self.guard(TOp::Idiv, Ty::Int, sub, m, 0)?
        } else if mag == 1 {
            sub
        } else {
            let km = self.kint(mag as i64);
            self.guard(TOp::Idiv, Ty::Int, sub, km, 0)?
        };
        if cfg!(rustynail_lua54) {
            self.def(a + 1, count);
        } else {
            self.def(a, count);
            self.def(a + 1, step);
            self.def(a + 2, init);
        }
        Ok(idx + 1)
    }

    /// 5.4: the visible control variable `R[A+3]` receives the new index (stored by every `FORLOOP` that continues).
    /// 5.5: the index slot is the control variable itself, nothing to copy.
    fn forloop_literal_step(vm: &Vm<'_>, body: usize, step_reg: usize) -> bool {
        // `FORPREP` sits right before the loop body; the step expression is the last one evaluated, so its code is the
        // instruction before the `FORPREP`
        let Some(fp) = body.checked_sub(1) else {
            return false;
        };
        let Some(prev) = fp.checked_sub(1) else {
            return false;
        };
        let (Some(&w_prep), Some(&w_prev)) = (vm.code.get(fp), vm.code.get(prev)) else {
            return false;
        };
        let (ip, iv) = (Instruction(w_prep), Instruction(w_prev));
        ip.opcode_raw() == OpCode::Forprep as u8
            && (iv.opcode_raw() == OpCode::Loadi as u8 || iv.opcode_raw() == OpCode::Loadk as u8)
            && iv.a() as usize == step_reg
    }

    fn forloop_ctl_copy(&mut self, a: usize, i1: Ref) {
        if cfg!(rustynail_lua54) && !mutants::active("forloop54_no_ctl_copy") {
            self.def(a + 3, i1);
        }
    }
}

// ---- P-B: library-call intrinsics (docs/JIT_TRACE_REF.md, "P-B decisions") ----

/// `posrelatI` of `lstrlib.c`.
/// P-O: IR operations that change nothing an exit could observe (the body of an inlined protected call may contain only these
/// before a guard): arithmetic, comparisons, constants, loads. Everything else (stores, barriers, allocations, calls, helpers,
/// control markers) is a side effect.
fn pcall_pure(op: TOp) -> bool {
    matches!(
        op,
        TOp::Kint
            | TOp::Knum
            | TOp::Kpri
            | TOp::Kptr
            | TOp::Sload
            | TOp::Add
            | TOp::Sub
            | TOp::Mul
            | TOp::Neg
            | TOp::Band
            | TOp::Bor
            | TOp::Bxor
            | TOp::Bnot
            | TOp::Shl
            | TOp::Shr
            | TOp::Idiv
            | TOp::Mod
            | TOp::Fadd
            | TOp::Fsub
            | TOp::Fmul
            | TOp::Fdiv
            | TOp::Fneg
            | TOp::Fabs
            | TOp::Fsqrt
            | TOp::Ffloor
            | TOp::Fmin
            | TOp::Fmax
            | TOp::ConvNumInt
            | TOp::F2iExact
            | TOp::Eq
            | TOp::Ne
            | TOp::Lt
            | TOp::Le
            | TOp::Gt
            | TOp::Ge
            | TOp::Ult
            | TOp::Uge
            | TOp::Fcmp
            | TOp::Eqref
            | TOp::Fload
            | TOp::Aref
            | TOp::Aload
            | TOp::Uref
            | TOp::Uload
            | TOp::Hrefk
            | TOp::Hrefv
            | TOp::Hload
            | TOp::Hmiss
            | TOp::Uvrel
    )
}

fn lib_posrelat_i(pos: i64, len: usize) -> usize {
    if pos > 0 {
        pos as usize
    } else if pos == 0 || pos < -(len as i64) {
        1
    } else {
        len.wrapping_add(pos as usize).wrapping_add(1)
    }
}

/// `getendpos` of `lstrlib.c` (the argument already read).
fn lib_getendpos(pos: i64, len: usize) -> usize {
    if pos > len as i64 {
        len
    } else if pos >= 0 {
        pos as usize
    } else if pos < -(len as i64) {
        0
    } else {
        len.wrapping_add(pos as usize).wrapping_add(1)
    }
}

impl Rec {
    /// The shared head of a recorded library call: the callee-identity guard (an entry guard when TR6d allows) and the room for
    /// the argument pushes. Returns `(func_rel, top_rel)`.
    fn lib_head(&mut self, vm: &Vm<'_>, a: usize, nargs: usize, addr: usize) -> R<(usize, usize)> {
        let d = self.cur_depth;
        let func_rel = self.frames[d].base_off + a;
        let top_rel = func_rel + 1 + nargs;
        self.need_ext(top_rel + 1)?;
        let (cr, _) = self.rd(vm, a)?;
        let kb = self.b.ir.konst(KVal::Ptr(addr as u64, PtrKind::Builtin));
        if !mutants::active("callee_guard_skip")
            && !(mutants::active("next_callee_guard_skip") && addr == jit_hooks::builtin_next())
        {
            self.entry_guard(TOp::Eqref, Ty::I64, cr, kb, 0)?;
        }
        Ok((func_rel, top_rel))
    }

    /// The tail of a recorded library call (the native's effects after every guard): TR6c / TR6b for the native frame, the push
    /// of the first result above the arguments (`push`: the native pushes its result there, as `lua_push*` does), the results moved
    /// to the function slot and nil-filled up to `wanted`; `top` for `wanted < 0`.
    fn lib_tail(
        &mut self,
        idx: usize,
        func_rel: usize,
        top_rel: usize,
        res: &[Ref],
        push: bool,
        wanted: i32,
    ) -> R<usize> {
        let d = self.cur_depth;
        if !mutants::active("native_room_skip") {
            self.room = self.room.max((top_rel + LUA_MINSTACK) as u32);
        }
        if !mutants::active("native_ci_skip") {
            self.max_ci = self.max_ci.max((self.ci_depth(d) + 1) as u8);
        }
        let nil = self.b.ir.konst(KVal::Pri(Ty::Nil));
        if push {
            if let Some(&r) = res.first() {
                self.def_abs(top_rel, r);
            }
        }
        let nmove = if wanted < 0 {
            res.len()
        } else {
            wanted as usize
        };
        self.need_ext(func_rel + nmove + 1)?;
        for j in 0..nmove {
            let r = res.get(j).copied().unwrap_or(nil);
            self.def_abs(func_rel + j, r);
        }
        if wanted < 0 {
            self.top_rel = Some(func_rel + res.len());
        } else {
            // the native (`tostring` & co. end in `check_gc`) runs with `top` right above its results: a collector step clears the
            // dead slice above them, so the lockstep does not compare those slots (like `NEWTABLE`); which call the step lands on
            // depends on the allocation pattern of the whole run
            for slot in func_rel + nmove..self.sm.len() {
                self.stale.insert(slot);
            }
        }
        Ok(idx + 1)
    }

    /// Emits `Lib(kind, a, b, third)` of type `t` followed by the guard `NE %lib, 0` on the snapshot of the call. `hint` is the VM slot the
    /// lockstep reads the helper's result from after the call.
    fn lib_op(
        &mut self,
        kind: u32,
        t: Ty,
        a: Ref,
        b: Ref,
        third: Option<Ref>,
        hint: usize,
    ) -> R<Ref> {
        if matches!(
            kind,
            lib::SUB
                | lib::CHAR
                | lib::TYPENAME
                | lib::TOSTR
                | lib::GMATCH
                | lib::CHAR2
                | lib::CHAR3
                | lib::LOWER
                | lib::UPPER
                | lib::REVERSE
                | lib::REP
                | lib::REPS
        ) || lib::is_fmt(kind)
        {
            self.storm_check()?;
        }
        self.first_store.get_or_insert(self.b.ir.ins.len());
        let aux = kind | u32::from(third.unwrap_or(0)) << 8;
        let r = self.b.emit_flags(TOp::Lib, t, 0, a, b, aux)?;
        let z = self.kint(0);
        if !mutants::active("lib_ne_guard_skip") {
            self.guard(TOp::Ne, Ty::I64, r, z, 0)?;
        }
        self.pend_str.push(hint);
        Ok(r)
    }

    /// P-B: one more string allocation per iteration; aborts `AllocStorm` when the collector steps about as often as the loop allocates
    /// (see [`RecCfg::storm_step`]).
    fn storm_check(&mut self) -> R<()> {
        self.str_allocs += 1;
        let step = self.cfg.storm_step;
        if step != 0
            && step < STORM_FACTOR * STORM_BYTES * self.str_allocs
            && !mutants::active("storm_guard_skip")
        {
            return Err(AbortReason::AllocStorm);
        }
        Ok(())
    }

    /// The VM slot the lockstep reads the result of a native call from: the function slot when a result is kept, else the push slot.
    fn lib_hint(&self, vm: &Vm<'_>, a: usize, nargs: usize, wanted: i32) -> usize {
        if self.tail_lib {
            // N3b: a tail call returned already: the first result is in the function slot the frame was called from
            return self.frames[self.cur_depth].func_abs;
        }
        if wanted == 0 {
            vm.base + a + 1 + nargs
        } else {
            vm.base + a
        }
    }

    /// N3b: `setmetatable(t, mt)` of a table `t` and a table or nil `mt`, as the `SETMT` `Lib` kind. Everything the C function checks and
    /// that can differ between iterations is checked by the helper at run time (a protected old metatable, a `__gc` in the new one: it
    /// does nothing and returns 0, the guard exits and the interpreter runs the call, with its error or its finalizer registration);
    /// the recording refuses what already fails or finalizes now (`MetaTable`) and every other argument type (`Nyi`: the interpreter's
    /// message).
    fn setmt_call(
        &mut self,
        vm: &Vm<'_>,
        idx: usize,
        a: usize,
        nargs: usize,
        wanted: i32,
        addr: usize,
    ) -> R<usize> {
        if nargs != 2 {
            return Err(self.nyi());
        }
        let (func_rel, top_rel) = self.lib_head(vm, a, nargs, addr)?;
        let tv = vm.slot(a + 1);
        let Some(tbl) = tv.as_table() else {
            return Err(self.nyi());
        };
        if let Some(old) = tbl.metatable() {
            if super::asm::setmt_protected(&vm.st.g().heap, old) {
                return Err(AbortReason::MetaTable); // "cannot change a protected metatable"
            }
        }
        let mv = vm.slot(a + 2);
        if let Some(m) = mv.as_table() {
            // (before the model sees the table: the flag byte it copies includes the cache bit this lookup sets, as the VM's will)
            if vm.st.g().heap.fast_tm(Some(m), Tm::Gc).is_some() {
                return Err(AbortReason::MetaTable); // `__gc`: the interpreter registers the finalizer
            }
        } else if !mv.is_nil() {
            return Err(self.nyi());
        }
        let t = self.rd(vm, a + 1)?;
        if t.1 != Ty::Tab {
            return Err(self.nyi());
        }
        let m = self.rd(vm, a + 2)?;
        let mr = match m.1 {
            Ty::Tab => m.0,
            Ty::Nil => self.kint(0),
            _ => return Err(self.nyi()),
        };
        let hint = usize::MAX;
        self.lib_op(lib::SETMT, Ty::Int, t.0, mr, None, hint)?;
        // MUTANT `setmt_result_mt`: the call returns the metatable
        let res = if mutants::active("setmt_result_mt") && m.1 == Ty::Tab {
            m.0
        } else {
            t.0
        };
        self.lib_tail(idx, func_rel, top_rel, &[res], true, wanted)
    }

    /// [`jit_hooks::libfn_of`], without the library functions of `set_libfns_ext` whose `libcall` bit is off (they stay plain C calls: stitching, tail-call aborts as before).
    fn libfn_enabled(&self, addr: usize) -> Option<u8> {
        use jit_hooks::libfn as L;
        jit_hooks::libfn_of(addr).filter(|&f| match f {
            L::TBL_UNPACK => self.cfg.libcall_mask & 32 != 0 && !cfg!(rustynail_lua54),
            L::TBL_MOVE => self.cfg.libcall_mask & 64 != 0 && !cfg!(rustynail_lua54),
            _ => true,
        })
    }

    /// `CALL` of a registered library function ([`jit_hooks::libfn_of`]).
    pub(super) fn lib_call(
        &mut self,
        vm: &Vm<'_>,
        idx: usize,
        a: usize,
        nargs: usize,
        wanted: i32,
        f: u8,
        addr: usize,
    ) -> R<usize> {
        use jit_hooks::libfn as L;
        match f {
            L::STR_SUB => self.lib_str_sub(vm, idx, a, nargs, wanted, addr),
            L::STR_BYTE => self.lib_str_byte(vm, idx, a, nargs, wanted, addr),
            L::STR_CHAR => self.lib_str_char(vm, idx, a, nargs, wanted, addr),
            L::MATH_TYPE => self.lib_math_type(vm, idx, a, nargs, wanted, addr),
            L::TOSTRING => self.lib_tostring(vm, idx, a, nargs, wanted, addr),
            L::RAWLEN => self.lib_rawlen(vm, idx, a, nargs, wanted, addr),
            L::TBL_INSERT => self.lib_tinsert(vm, idx, a, nargs, wanted, addr),
            L::TBL_REMOVE => self.lib_tremove(vm, idx, a, nargs, wanted, addr),
            L::TONUMBER | L::MATH_TOINTEGER => {
                self.lib_num_identity(vm, idx, a, nargs, wanted, addr, f)
            }
            L::MATH_FMOD => self.lib_fmod(vm, idx, a, nargs, wanted, addr),
            L::RAWGET if self.cfg.f2raw && !cfg!(rustynail_lua54) => {
                self.lib_rawget(vm, idx, a, nargs, wanted, addr)
            }
            L::RAWSET if self.cfg.f2raw && !cfg!(rustynail_lua54) => {
                self.lib_rawset(vm, idx, a, nargs, wanted, addr)
            }
            L::TBL_MOVE if self.cfg.libcall_mask & 64 != 0 && !cfg!(rustynail_lua54) => {
                self.lib_tmove(vm, idx, a, nargs, wanted, addr)
            }
            L::TBL_UNPACK if self.cfg.libcall_mask & 32 != 0 && !cfg!(rustynail_lua54) => {
                self.lib_tunpack(vm, idx, a, nargs, wanted, addr)
            }
            _ => Err(AbortReason::CFunc),
        }
    }

    /// BUNDLE-G1: is the call at `a` `string.format("%s", s)` with a short-string `s` (the shape [`Rec::lib_fmt_s`] folds)? Anything
    /// else keeps the stitching C call.
    fn fmt_s_shape(&self, vm: &Vm<'_>, a: usize, nargs: usize) -> bool {
        if nargs != 2 {
            return false;
        }
        let (f, s) = (vm.slot(a + 1), vm.slot(a + 2));
        match (f.as_strref(), s.as_strref()) {
            (Some(f), Some(_))
                if f.get().as_bytes() == b"%s"
                    && ty_of_value(&vm.slot(a + 1)) == Some(Ty::ShrStr) =>
            {
                ty_of_value(&s) == Some(Ty::ShrStr)
            }
            _ => false,
        }
    }

    /// BUNDLE-G1: `string.format("%s", s)` with a short string `s` returns `s` itself (the native's `luaL_tolstring` of a string is the
    /// string, `luaL_addvalue` copies it into the buffer, `luaL_pushresult` interns the bytes again: the same short string). Guards: the
    /// callee is the builtin, the format register is the recorded `"%s"` string (a proto constant needs none), the argument is a short
    /// string (the type guard of its load). Any failure exits and the interpreter runs the real call.
    fn lib_fmt_s(
        &mut self,
        vm: &Vm<'_>,
        idx: usize,
        a: usize,
        nargs: usize,
        wanted: i32,
        addr: usize,
    ) -> R<usize> {
        let (func_rel, top_rel) = self.lib_head(vm, a, nargs, addr)?;
        let (f, ft) = self.rd(vm, a + 1)?;
        let (s, st) = self.rd(vm, a + 2)?;
        if ft != Ty::ShrStr || st != Ty::ShrStr {
            return Err(self.nyi());
        }
        if !mutants::active("g1fmt_fmt_guard_skip") {
            self.reg_key(vm, a + 1, f)?;
        }
        // MUTANT `g1fmt_result_fmt`: the result is the format string
        let r = if mutants::active("g1fmt_result_fmt") {
            f
        } else {
            s
        };
        self.lib_tail(idx, func_rel, top_rel, &[r], true, wanted)
    }

    /// BUNDLE-C (`tfmt`): is the call at `a` `string.format(f, ...)` with a short-string format of `%d %x %X %s %c` items (one to five, see
    /// `rn_std::lib_string::jit_fmt_shape`), exactly as many arguments as items of the types the items take (integers for `d x X c`, strings or
    /// integers for `s`), and does the reference engine succeed on them now? Anything else keeps the stitching C call. No side effects.
    fn fmt_gen_ok(&self, vm: &Vm<'_>, a: usize, nargs: usize) -> bool {
        if !(2..=6).contains(&nargs) {
            return false;
        }
        let f = vm.slot(a + 1);
        let (Some(fs), Some(Ty::ShrStr)) = (f.as_strref(), ty_of_value(&f)) else {
            return false;
        };
        let fb = fs.get().as_bytes();
        let Some(kinds) = rn_std::lib_string::jit_fmt_shape(fb) else {
            return false;
        };
        if kinds.len() != nargs - 1 {
            return false;
        }
        let mut vals = Vec::with_capacity(3);
        for (j, &k) in kinds.iter().enumerate() {
            let v = vm.slot(a + 2 + j);
            match (k, ty_of_value(&v)) {
                (0, Some(Ty::Int)) | (1, Some(Ty::ShrStr | Ty::LngStr | Ty::Int)) => vals.push(v),
                _ => return false,
            }
        }
        let mut out = Vec::new();
        rn_std::lib_string::jit_format(&*vm.st, fb, &vals, &mut out)
    }

    /// BUNDLE-C (`tfmt`): `string.format(f, ...)` of a shape [`Rec::fmt_gen_ok`] accepted: one `LIB` op of kind `FMT_BASE + id` over the
    /// arguments (a constant nil fills the absent operands), guarded against the result kind seen (the helper's own check) and the format
    /// register being the recorded string (a proto constant needs none). The helper answers 0 (the guard exits, the interpreter runs the
    /// real call) for what the reference engine would reject or what depends on a metatable now.
    fn lib_fmt(
        &mut self,
        vm: &Vm<'_>,
        idx: usize,
        a: usize,
        nargs: usize,
        wanted: i32,
        addr: usize,
    ) -> R<usize> {
        let (func_rel, top_rel) = self.lib_head(vm, a, nargs, addr)?;
        let (f, ft) = self.rd(vm, a + 1)?;
        let fv = vm.slot(a + 1);
        let (Some(fs), Ty::ShrStr) = (fv.as_strref(), ft) else {
            return Err(self.nyi());
        };
        let fb = fs.get().as_bytes();
        let Some(id) = rn_std::lib_string::jit_fmt_register(fb) else {
            return Err(AbortReason::CFunc); // the registry is full
        };
        let nil = self.b.ir.konst(KVal::Pri(Ty::Nil));
        let mut ops = [nil; 5];
        let mut tys = [Ty::Nil; 5];
        let mut vals = Vec::with_capacity(5);
        let kinds = rn_std::lib_string::jit_fmt_shape(fb).ok_or(AbortReason::Diverge)?;
        for (j, &k) in kinds.iter().enumerate() {
            let (r, t) = self.rd(vm, a + 2 + j)?;
            let ok = matches!(
                (k, t),
                (0, Ty::Int) | (1, Ty::ShrStr | Ty::LngStr | Ty::Int)
            );
            if !ok {
                return Err(self.nyi());
            }
            ops[j] = r;
            tys[j] = t;
            vals.push(vm.slot(a + 2 + j));
        }
        let _ = tys;
        let mut out = Vec::new();
        if !rn_std::lib_string::jit_format(&*vm.st, fb, &vals, &mut out) {
            return Err(self.nyi());
        }
        if !mutants::active("tfmt_fmt_guard_skip") {
            self.reg_key(vm, a + 1, f)?;
        }
        let rty = if out.len() > rn_core::string::MAX_SHORT_LEN {
            Ty::LngStr
        } else {
            Ty::ShrStr
        };
        let hint = self.lib_hint(vm, a, nargs, wanted);
        let r = if kinds.len() <= 3 {
            self.lib_op(
                lib::FMT_BASE + id as u32,
                rty,
                ops[0],
                ops[1],
                Some(ops[2]),
                hint,
            )?
        } else {
            // four or five arguments: the stash op carries the last ones (consecutive ops: nothing is allocated in between)
            let link = self.lib_op(lib::FMTX, Ty::Int, ops[2], ops[3], Some(ops[4]), hint)?;
            self.lib_op(
                lib::FMT_BASE + 64 + id as u32,
                rty,
                ops[0],
                ops[1],
                Some(link),
                hint,
            )?
        };
        self.lib_tail(idx, func_rel, top_rel, &[r], true, wanted)
    }

    /// F1: `string.len(s)` / `s:len()` of a string: its length, a load of the header (`TLEN` of a string).
    fn lib_str_len(
        &mut self,
        vm: &Vm<'_>,
        idx: usize,
        a: usize,
        nargs: usize,
        wanted: i32,
        addr: usize,
    ) -> R<usize> {
        if nargs != 1 {
            return Err(self.nyi());
        }
        let (func_rel, top_rel) = self.lib_head(vm, a, nargs, addr)?;
        let (s, t, _) = self.lib_str_arg(vm, a + 1)?;
        let r = self.b.emit_flags(
            TOp::Tlen,
            Ty::Int,
            0,
            s,
            NOREF,
            if t == Ty::ShrStr { 1 } else { 2 },
        )?;
        // MUTANT `f1_strlen_off`: the length is one too large
        let r = if mutants::active("f1_strlen_off") {
            let one = self.kint(1);
            self.ins(TOp::Add, Ty::Int, r, one)?
        } else {
            r
        };
        // the lockstep reads the length from the VM (the pushed result)
        self.post_slot = Some(vm.base + a + 1 + nargs);
        self.lib_tail(idx, func_rel, top_rel, &[r], true, wanted)
    }

    /// F2: `math.random(m)` / `math.random(m, n)` of integers: the `RANDOM` helper draws from the generator in the closure's upvalues. The interval
    /// is the run-time one (an empty interval, or a draw of -1, makes the helper answer 0: the interpreter runs the call and raises or draws).
    fn lib_random(
        &mut self,
        vm: &Vm<'_>,
        idx: usize,
        a: usize,
        nargs: usize,
        wanted: i32,
    ) -> R<usize> {
        if !(1..=2).contains(&nargs) && !(nargs == 0 && self.cfg.libcall) {
            return Err(self.nyi()); // the float form, `random(0)`'s raw word, "wrong number of arguments"
        }
        let d = self.cur_depth;
        let func_rel = self.frames[d].base_off + a;
        let top_rel = func_rel + 1 + nargs;
        self.need_ext(top_rel + 1)?;
        let (cr, cty) = self.rd(vm, a)?;
        if cty != Ty::Ccl {
            return Err(AbortReason::Diverge);
        }
        if nargs == 0 {
            // BUNDLE-H (`libcall`): `math.random()`: the helper draws and returns the 53-bit integer plus one; the float is `(raw - 1) * 2^-53`
            let hint = self.lib_hint(vm, a, nargs, wanted);
            let z = self.kint(0);
            let raw = self.lib_op(lib::RANDF, Ty::Int, cr, z, None, hint)?;
            let km1 = self.kint(-1);
            let m = self.ins(TOp::Add, Ty::Int, raw, km1)?;
            let f = self.ins(TOp::ConvNumInt, Ty::Num, m, NOREF)?;
            // MUTANT `randf_scale_off`: the scale is 2^-52
            let scale = self.b.ir.knum(if mutants::active("randf_scale_off") {
                0.5 / (1u64 << 51) as f64
            } else {
                0.5 / (1u64 << 52) as f64
            });
            let r = self.ins(TOp::Fmul, Ty::Num, f, scale)?;
            return self.lib_tail(idx, func_rel, top_rel, &[r], true, wanted);
        }
        let (low, up) = if nargs == 1 {
            let (u, vu) = self.lib_int_arg(vm, a + 1)?;
            if vu < 1 {
                return Err(self.nyi());
            }
            (self.kint(1), u)
        } else {
            let (l, vl) = self.lib_int_arg(vm, a + 1)?;
            let (u, vu) = self.lib_int_arg(vm, a + 2)?;
            if vl > vu {
                return Err(self.nyi());
            }
            (l, u)
        };
        // (the draw the interpreter makes right after this step: a -1 cannot be carried by the helper's result convention)
        let (vl, vu) = if nargs == 1 {
            (1, vm.slot(a + 1).as_int().unwrap_or(0))
        } else {
            (
                vm.slot(a + 1).as_int().unwrap_or(0),
                vm.slot(a + 2).as_int().unwrap_or(0),
            )
        };
        if rn_std::lib_math::jit_random_peek(&vm.slot(a), vl, vu).is_none() {
            return Err(self.nyi());
        }
        let hint = self.lib_hint(vm, a, nargs, wanted);
        let raw = self.lib_op(lib::RANDOM, Ty::Int, cr, low, Some(up), hint)?;
        let km1 = self.kint(-1);
        let r = self.ins(TOp::Add, Ty::Int, raw, km1)?;
        self.lib_tail(idx, func_rel, top_rel, &[r], true, wanted)
    }

    /// F2: `getmetatable(t)` of a table whose metatable (if any) has no `__metatable` field: the metatable word of the table (`FLOAD`), nil
    /// for none. The absence of the field in the metatable is proven like a missing metamethod (`HMISS` walk of the recorded layout).
    fn getmt_call(
        &mut self,
        vm: &Vm<'_>,
        idx: usize,
        a: usize,
        nargs: usize,
        wanted: i32,
        addr: usize,
    ) -> R<usize> {
        if nargs != 1 {
            return Err(self.nyi());
        }
        let (func_rel, top_rel) = self.lib_head(vm, a, nargs, addr)?;
        let (tr, ty) = self.rd(vm, a + 1)?;
        if ty != Ty::Tab {
            return Err(self.nyi());
        }
        let tbl = vm.slot(a + 1).as_table().ok_or(AbortReason::Diverge)?;
        let mp = self.fload(tr, Field::Metatable, Ty::P64)?;
        let z = self.kint(0);
        let res = match tbl.metatable() {
            None => {
                if !mutants::active("getmt_null_skip") {
                    self.guard(TOp::Eq, Ty::I64, mp, z, 0)?;
                }
                self.b.ir.konst(KVal::Pri(Ty::Nil))
            }
            Some(m) => {
                if !mutants::active("getmt_null_skip") {
                    self.guard(TOp::Ne, Ty::I64, mp, z, 0)?;
                }
                let name = vm.st.g().heap.metatable_name();
                if m.short_str_node(name).is_some() {
                    return Err(self.nyi()); // a protected metatable (or a nil-valued field): the value of the field is the result
                }
                self.touch(&Value::table(m))?;
                if !mutants::active("getmt_protected_skip") {
                    let kname = self.b.ir.konst(KVal::Ptr(
                        payload_bits(&Value::string(name), Ty::ShrStr),
                        PtrKind::Raw,
                    ));
                    self.hash_absent(mp, m, (kname, name))?;
                }
                self.fload(tr, Field::Metatable, Ty::Tab)?
            }
        };
        self.lib_tail(idx, func_rel, top_rel, &[res], true, wanted)
    }

    /// F2: `assert(v, ...)` with a truthy `v`: returns its arguments (the type of `v` is a guard of its load; a falsy one raises).
    fn assert_call(
        &mut self,
        vm: &Vm<'_>,
        idx: usize,
        a: usize,
        nargs: usize,
        wanted: i32,
        addr: usize,
    ) -> R<usize> {
        if nargs == 0 {
            return Err(self.nyi());
        }
        let (func_rel, top_rel) = self.lib_head(vm, a, nargs, addr)?;
        let (vr, vt) = self.rd(vm, a + 1)?;
        if matches!(vt, Ty::Nil | Ty::False) {
            return Err(self.nyi()); // the error: the interpreter raises it
        }
        let nres = if wanted < 0 {
            nargs
        } else {
            (wanted as usize).min(nargs)
        };
        let mut res = vec![vr];
        for j in 1..nres {
            res.push(self.rd(vm, a + 1 + j)?.0);
        }
        // MUTANT `assert_extra_dropped`: only the first argument is returned
        if mutants::active("assert_extra_dropped") {
            res.truncate(1);
        }
        self.lib_tail(idx, func_rel, top_rel, &res, false, wanted)
    }

    /// F2: `rawget(t, k)` of an integer key inside the array part: the element, whatever the metatable.
    fn lib_rawget(
        &mut self,
        vm: &Vm<'_>,
        idx: usize,
        a: usize,
        nargs: usize,
        wanted: i32,
        addr: usize,
    ) -> R<usize> {
        if nargs != 2 {
            return Err(self.nyi());
        }
        let (func_rel, top_rel) = self.lib_head(vm, a, nargs, addr)?;
        let (tr, ty) = self.rd(vm, a + 1)?;
        if ty != Ty::Tab {
            return Err(self.nyi());
        }
        if self.cfg.idxdeep & 2 != 0 && self.rd(vm, a + 2)?.1 == Ty::ShrStr {
            return self.lib_rawget_str(vm, idx, a, func_rel, top_rel, wanted);
        }
        let (key, k) = self.lib_int_arg(vm, a + 2)?;
        let tbl = vm.slot(a + 1).as_table().ok_or(AbortReason::Diverge)?;
        if !(1..=i64::from(tbl.array_size())).contains(&k) {
            return Err(self.nyi());
        }
        let ev = match tbl.get_int_arr(k) {
            Some(v) => v,
            None if self.cfg.nilload => Value::NIL,
            None => return Err(AbortReason::TableNil),
        };
        let ety = ty_of_value(&ev).ok_or_else(|| self.nyi())?;
        self.touch(&ev)?;
        let aref = self.array_guards_raw(tr, key, tbl, ety != Ty::Nil, true)?;
        let r = self.guard(TOp::Aload, ety, aref, NOREF, 1)?;
        self.lib_tail(idx, func_rel, top_rel, &[r], true, wanted)
    }

    /// BUNDLE-N: `rawget(t, k)` of a short-string key: the present value (a guarded `HREFK` / `HREFV` + `HLOAD`) or `nil` (the absence proof, or a
    /// present nil node), whatever the metatable: it is never looked at.
    fn lib_rawget_str(
        &mut self,
        vm: &Vm<'_>,
        idx: usize,
        a: usize,
        func_rel: usize,
        top_rel: usize,
        wanted: i32,
    ) -> R<usize> {
        let (tr, _) = self.rd(vm, a + 1)?;
        let tv = vm.slot(a + 1);
        let tbl = tv.as_table().ok_or(AbortReason::Diverge)?;
        let (kr, _) = self.rd(vm, a + 2)?;
        let key = self.reg_key_any(vm, a + 2, kr, &tv, Ty::Tab)?;
        let found = tbl.short_str_node(key.1);
        if let Some(ix) = found {
            if ix >= 1 << 15 {
                return Err(AbortReason::TableHash);
            }
        }
        let r = match found {
            Some(ix) => {
                let ev = tbl.node_value(ix);
                let ety = if ev.tag() & 0x0F == LUA_TNIL {
                    Ty::Nil
                } else {
                    ty_of_value(&ev).ok_or_else(|| self.nyi())?
                };
                self.touch(&ev)?;
                let hr = self.hash_ref(tr, tbl, key.0, ix)?;
                self.guard(TOp::Hload, ety, hr, NOREF, 1)?
            }
            None => {
                if mutants::active("rawget_str_absent_skip") {
                    // (mutant: the key is taken as absent without a proof)
                } else {
                    self.hash_absent(tr, tbl, key)?;
                }
                self.b.ir.konst(KVal::Pri(Ty::Nil))
            }
        };
        self.lib_tail(idx, func_rel, top_rel, &[r], true, wanted)
    }

    /// F2: `rawset(t, k, v)` of an integer key inside the array part: the plain store, whatever the metatable (`__newindex` is not
    /// looked at); returns `t`.
    fn lib_rawset(
        &mut self,
        vm: &Vm<'_>,
        idx: usize,
        a: usize,
        nargs: usize,
        wanted: i32,
        addr: usize,
    ) -> R<usize> {
        if nargs != 3 {
            return Err(self.nyi());
        }
        let (func_rel, top_rel) = self.lib_head(vm, a, nargs, addr)?;
        let (tr, ty) = self.rd(vm, a + 1)?;
        if ty != Ty::Tab {
            return Err(self.nyi());
        }
        let (key, k) = self.lib_int_arg(vm, a + 2)?;
        let val = self.rd(vm, a + 3)?;
        let tbl = vm.slot(a + 1).as_table().ok_or(AbortReason::Diverge)?;
        if !(1..=i64::from(tbl.array_size())).contains(&k) || val.1 == Ty::Nil {
            return Err(self.nyi());
        }
        let ev = tbl.get_int_arr(k).unwrap_or(Value::NIL);
        self.touch(&ev)?;
        let aref = self.array_guards_raw(tr, key, tbl, false, true)?;
        self.first_store.get_or_insert(self.b.ir.ins.len());
        self.ins(TOp::Astore, Ty::I64, aref, val.0)?;
        if collectable(val.1) && !mutants::active("rawset_tbar_skip") {
            self.ins(TOp::Tbar, Ty::I64, tr, val.0)?;
        }
        let nil = self.b.ir.konst(KVal::Pri(Ty::Nil));
        let res = if mutants::active("rawset_result_nil") {
            nil
        } else {
            tr
        };
        self.lib_tail(idx, func_rel, top_rel, &[res], true, wanted)
    }

    /// A string (not a number: `luaL_checklstring` would convert it in place) argument.
    fn lib_str_arg(&mut self, vm: &Vm<'_>, local: usize) -> R<(Ref, Ty, usize)> {
        let (r, t) = self.rd(vm, local)?;
        if !matches!(t, Ty::ShrStr | Ty::LngStr) {
            return Err(self.nyi());
        }
        let l = vm
            .slot(local)
            .as_strref()
            .ok_or(AbortReason::Diverge)?
            .get()
            .len();
        Ok((r, t, l))
    }

    /// An integer argument (a float with an integral value converts: not recorded).
    fn lib_int_arg(&mut self, vm: &Vm<'_>, local: usize) -> R<(Ref, i64)> {
        let (r, t) = self.rd(vm, local)?;
        if t != Ty::Int {
            return Err(self.nyi());
        }
        Ok((r, vm.slot(local).as_int().ok_or(AbortReason::Diverge)?))
    }

    /// `string.sub(s, i [, j])` with integer `i`, `j`.
    fn lib_str_sub(
        &mut self,
        vm: &Vm<'_>,
        idx: usize,
        a: usize,
        nargs: usize,
        wanted: i32,
        addr: usize,
    ) -> R<usize> {
        if !(2..=3).contains(&nargs) {
            return Err(self.nyi());
        }
        let (func_rel, top_rel) = self.lib_head(vm, a, nargs, addr)?;
        let (s, _, l) = self.lib_str_arg(vm, a + 1)?;
        let (i, vi) = self.lib_int_arg(vm, a + 2)?;
        let (j, vj) = if nargs == 3 {
            self.lib_int_arg(vm, a + 3)?
        } else {
            (self.kint(-1), -1)
        };
        let start = lib_posrelat_i(vi, l);
        let end = lib_getendpos(vj, l);
        let rl = if start <= end { end - start + 1 } else { 0 };
        let rty = if rl > rn_core::string::MAX_SHORT_LEN {
            Ty::LngStr
        } else {
            Ty::ShrStr
        };
        let hint = self.lib_hint(vm, a, nargs, wanted);
        let r = self.lib_op(lib::SUB, rty, s, i, Some(j), hint)?;
        self.lib_tail(idx, func_rel, top_rel, &[r], true, wanted)
    }

    /// `string.byte(s [, i])` with one result.
    fn lib_str_byte(
        &mut self,
        vm: &Vm<'_>,
        idx: usize,
        a: usize,
        nargs: usize,
        wanted: i32,
        addr: usize,
    ) -> R<usize> {
        if nargs == 3 && self.cfg.f2byte {
            return self.lib_str_byte_range(vm, idx, a, wanted, addr);
        }
        if !(1..=2).contains(&nargs) {
            return Err(self.nyi());
        }
        let (func_rel, top_rel) = self.lib_head(vm, a, nargs, addr)?;
        let (s, _, l) = self.lib_str_arg(vm, a + 1)?;
        let (i, vi) = if nargs == 2 {
            self.lib_int_arg(vm, a + 2)?
        } else {
            (self.kint(1), 1)
        };
        if lib_posrelat_i(vi, l) != lib_getendpos(vi, l) {
            return Err(self.nyi()); // no result, or a range
        }
        let hint = self.lib_hint(vm, a, nargs, wanted);
        let raw = self.lib_op(lib::BYTE, Ty::Int, s, i, None, hint)?;
        let km1 = self.kint(-1);
        let r = self.ins(TOp::Add, Ty::Int, raw, km1)?;
        self.lib_tail(idx, func_rel, top_rel, &[r], true, wanted)
    }

    /// F2: `string.byte(s, i, j)` with `1 <= i <= j <= #s` and at most `MAX_BYTE_RANGE` results that the call site wants all of (`wanted` is
    /// at least the count): the guards say that `i >= 1` and `j == i + n - 1` (the same count at run time; the last `BYTE` guards `j <= #s`), then one `BYTE` per
    /// result. Other shapes (negative or clamped positions, an open result list) abort `Nyi` as before.
    fn lib_str_byte_range(
        &mut self,
        vm: &Vm<'_>,
        idx: usize,
        a: usize,
        wanted: i32,
        addr: usize,
    ) -> R<usize> {
        const MAX_BYTE_RANGE: i64 = 8;
        let nargs = 3;
        let (s_v, i_v, j_v) = (vm.slot(a + 1), vm.slot(a + 2), vm.slot(a + 3));
        let (Some(sref), Some(vi), Some(vj)) = (s_v.as_strref(), i_v.as_int(), j_v.as_int()) else {
            return Err(self.nyi());
        };
        let l = sref.get().len() as i64;
        let n = vj.wrapping_sub(vi).wrapping_add(1);
        if vi < 1 || vj < vi || vj > l || !(1..=MAX_BYTE_RANGE).contains(&n) || wanted < n as i32 {
            return Err(self.nyi());
        }
        let (func_rel, top_rel) = self.lib_head(vm, a, nargs, addr)?;
        let (s, _, _) = self.lib_str_arg(vm, a + 1)?;
        let (i, _) = self.lib_int_arg(vm, a + 2)?;
        let (j, _) = self.lib_int_arg(vm, a + 3)?;
        let one = self.kint(1);
        if !mutants::active("byte_range_guard_skip") {
            self.guard(TOp::Ge, Ty::I64, i, one, 0)?;
            let last = if n == 1 {
                i
            } else {
                let dn = self.kint(n - 1 - i64::from(mutants::active("byte_range_count_off")));
                self.ins(TOp::Add, Ty::Int, i, dn)?
            };
            self.guard(TOp::Eq, Ty::I64, j, last, 0)?;
            // (`j <= #s` is the guard of the last `BYTE`: its helper answers 0 for a position behind the end)
        }
        let km1 = self.kint(-1);
        let mut res = Vec::new();
        for p in 0..n {
            let ix = if p == 0 {
                i
            } else {
                let kp = self.kint(p);
                self.ins(TOp::Add, Ty::Int, i, kp)?
            };
            let hint = vm.base + a + p as usize;
            let raw = self.lib_op(lib::BYTE, Ty::Int, s, ix, None, hint)?;
            res.push(self.ins(TOp::Add, Ty::Int, raw, km1)?);
        }
        // the native pushes every result above its arguments
        self.need_ext(top_rel + res.len() + 1)?;
        for (p, &r) in res.iter().enumerate().skip(1) {
            self.def_abs(top_rel + p, r);
        }
        self.lib_tail(idx, func_rel, top_rel, &res, true, wanted)
    }

    /// `string.char(n)` with one argument.
    fn lib_str_char(
        &mut self,
        vm: &Vm<'_>,
        idx: usize,
        a: usize,
        nargs: usize,
        wanted: i32,
        addr: usize,
    ) -> R<usize> {
        // BUNDLE-H (`libcall`): two or three arguments are one `LIB` op of kind `CHAR2` / `CHAR3`
        if nargs != 1 && !(self.cfg.libcall && (2..=3).contains(&nargs)) {
            return Err(self.nyi());
        }
        let (func_rel, top_rel) = self.lib_head(vm, a, nargs, addr)?;
        let mut ops = [NOREF; 3];
        for (j, o) in ops.iter_mut().enumerate().take(nargs) {
            let (n, vn) = self.lib_int_arg(vm, a + 1 + j)?;
            if !(0..=255).contains(&vn) {
                return Err(self.nyi()); // "value out of range"
            }
            *o = n;
        }
        let hint = self.lib_hint(vm, a, nargs, wanted);
        let r = match nargs {
            1 => {
                let z = self.kint(0);
                self.lib_op(lib::CHAR, Ty::ShrStr, ops[0], z, None, hint)?
            }
            2 => self.lib_op(lib::CHAR2, Ty::ShrStr, ops[0], ops[1], None, hint)?,
            _ => self.lib_op(lib::CHAR3, Ty::ShrStr, ops[0], ops[1], Some(ops[2]), hint)?,
        };
        self.lib_tail(idx, func_rel, top_rel, &[r], true, wanted)
    }

    /// BUNDLE-H (`libcall`): `string.lower(s)` / `upper(s)` / `reverse(s)` (one string argument, the result has its length) and
    /// `string.rep(s, n [, sep])` (a string, an integer and, for three arguments, a string: the result is built by the helper, which
    /// answers 0 above [`lib::REP_MAX`] bytes so the interpreter allocates it or raises).
    fn lib_strfn(
        &mut self,
        vm: &Vm<'_>,
        idx: usize,
        a: usize,
        nargs: usize,
        wanted: i32,
        addr: usize,
    ) -> R<usize> {
        let which = rn_std::lib_string::jit_str_fns()
            .iter()
            .position(|&x| x == addr)
            .ok_or(AbortReason::Diverge)?;
        if which < 3 {
            if nargs != 1 {
                return Err(self.nyi());
            }
            let (func_rel, top_rel) = self.lib_head(vm, a, nargs, addr)?;
            let (s, t, _) = self.lib_str_arg(vm, a + 1)?;
            let hint = self.lib_hint(vm, a, nargs, wanted);
            let kind = [lib::LOWER, lib::UPPER, lib::REVERSE][which];
            let z = self.kint(0);
            let r = self.lib_op(kind, t, s, z, None, hint)?;
            return self.lib_tail(idx, func_rel, top_rel, &[r], true, wanted);
        }
        if !(2..=3).contains(&nargs) || !self.cfg.libcall_rep {
            return Err(self.nyi());
        }
        let (func_rel, top_rel) = self.lib_head(vm, a, nargs, addr)?;
        let (s, _, l) = self.lib_str_arg(vm, a + 1)?;
        let (n, vn) = self.lib_int_arg(vm, a + 2)?;
        let (sep, lsep) = if nargs == 3 {
            let (r, _, ls) = self.lib_str_arg(vm, a + 3)?;
            (Some(r), ls)
        } else {
            (None, 0)
        };
        // the length of the result as the library computes it
        let total = if vn <= 0 || l + lsep == 0 {
            0
        } else {
            (l + lsep)
                .checked_mul(vn as usize)
                .map(|x| x - lsep)
                .ok_or_else(|| self.nyi())?
        };
        if total > lib::REP_MAX {
            return Err(self.nyi());
        }
        let rty = if total > rn_core::string::MAX_SHORT_LEN {
            Ty::LngStr
        } else {
            Ty::ShrStr
        };
        if self.cfg.libcall_repcls && !mutants::active("repcls_guard_skip") {
            // BUNDLE-R (`libcall` bit 4): the class of the result as an ordinary guard on the computed length, `(len(s) + len(sep)) * n - len(sep)`
            // against the short-string limit (`n` is first guarded into `0..=REP_MAX`, so the product cannot wrap for any string that fits
            // the helper's cap; the helper still refuses a result of the other class). A mixed-class loop then exits at this guard, which is an
            // `Other` exit (a side trace can follow it); before, the class mismatch was the helper's answer 0 behind the allocation guard
            // (`Alloc` site: no side traces, a storm of exits).
            if vn <= 0 {
                // a count of zero or less gives the empty string (a short one) whatever the lengths are
                let z = self.kint(0);
                self.guard(TOp::Le, Ty::I64, n, z, 0)?;
            } else {
                let lim = self.kint(lib::REP_MAX as i64 + 1);
                self.guard(TOp::Ult, Ty::I64, n, lim, 0)?;
                let flag = |t: Ty| if t == Ty::ShrStr { 1 } else { 2 };
                let ts = self.rd(vm, a + 1)?.1;
                let n0 = self.b.ir.ins.len();
                let ls = self
                    .b
                    .emit_flags(TOp::Tlen, Ty::Int, 0, s, NOREF, flag(ts))?;
                if self.b.ir.ins.len() > n0 {
                    self.pend_lens.push_back(l as i64);
                }
                let (per, lsp) = match sep {
                    Some(p) => {
                        let tp = self.rd(vm, a + 3)?.1;
                        let n1 = self.b.ir.ins.len();
                        let lp = self
                            .b
                            .emit_flags(TOp::Tlen, Ty::Int, 0, p, NOREF, flag(tp))?;
                        if self.b.ir.ins.len() > n1 {
                            self.pend_lens.push_back(lsep as i64);
                        }
                        (self.ins(TOp::Add, Ty::Int, ls, lp)?, Some(lp))
                    }
                    None => (ls, None),
                };
                let prod = self.ins(TOp::Mul, Ty::Int, per, n)?;
                let tot = match lsp {
                    Some(lp) => self.ins(TOp::Sub, Ty::Int, prod, lp)?,
                    None => prod,
                };
                let kl = self.kint(rn_core::string::MAX_SHORT_LEN as i64);
                self.guard(
                    if rty == Ty::LngStr { TOp::Gt } else { TOp::Le },
                    Ty::I64,
                    tot,
                    kl,
                    0,
                )?;
            }
        }
        let hint = self.lib_hint(vm, a, nargs, wanted);
        let r = match sep {
            None => self.lib_op(lib::REP, rty, s, n, None, hint)?,
            Some(p) => self.lib_op(lib::REPS, rty, s, n, Some(p), hint)?,
        };
        self.lib_tail(idx, func_rel, top_rel, &[r], true, wanted)
    }

    /// `math.type(x)`: the name of the number kind (the string is interned by the helper), `nil` for anything else.
    fn lib_math_type(
        &mut self,
        vm: &Vm<'_>,
        idx: usize,
        a: usize,
        nargs: usize,
        wanted: i32,
        addr: usize,
    ) -> R<usize> {
        if nargs != 1 {
            return Err(self.nyi());
        }
        let (func_rel, top_rel) = self.lib_head(vm, a, nargs, addr)?;
        let (_, t) = self.rd(vm, a + 1)?;
        let r = match t {
            Ty::Int | Ty::Num => {
                let k = self.kint(if t == Ty::Int { 8 } else { 9 });
                let z = self.kint(0);
                let hint = self.lib_hint(vm, a, nargs, wanted);
                self.lib_op(lib::TYPENAME, Ty::ShrStr, k, z, None, hint)?
            }
            _ => self.b.ir.konst(KVal::Pri(Ty::Nil)),
        };
        self.lib_tail(idx, func_rel, top_rel, &[r], true, wanted)
    }

    /// `type(v)`: the callee is the C closure whose upvalues are the type names; the helper checks the closure's function at run time and
    /// returns the name (an upvalue of the closure: no allocation).
    pub(super) fn lib_type(
        &mut self,
        vm: &Vm<'_>,
        idx: usize,
        a: usize,
        nargs: usize,
        wanted: i32,
    ) -> R<usize> {
        if nargs == 0 {
            return Err(self.nyi());
        }
        let d = self.cur_depth;
        let func_rel = self.frames[d].base_off + a;
        let top_rel = func_rel + 1 + nargs;
        self.need_ext(top_rel + 1)?;
        let (cr, cty) = self.rd(vm, a)?;
        if cty != Ty::Ccl {
            return Err(AbortReason::Diverge);
        }
        let (_, _) = self.rd(vm, a + 1)?; // the argument's type is a guard of its load
        let tidx = vm.slot(a + 1).type_of() as i64;
        let k = self.kint(tidx);
        let hint = self.lib_hint(vm, a, nargs, wanted);
        let r = self.lib_op(lib::TYPE, Ty::ShrStr, cr, k, None, hint)?;
        self.lib_tail(idx, func_rel, top_rel, &[r], true, wanted)
    }

    /// `tostring(n)` of a number.
    fn lib_tostring(
        &mut self,
        vm: &Vm<'_>,
        idx: usize,
        a: usize,
        nargs: usize,
        wanted: i32,
        addr: usize,
    ) -> R<usize> {
        if nargs != 1 {
            return Err(self.nyi());
        }
        let (func_rel, top_rel) = self.lib_head(vm, a, nargs, addr)?;
        let (x, t) = self.rd(vm, a + 1)?;
        if !matches!(t, Ty::Int | Ty::Num) {
            return Err(self.nyi());
        }
        let hint = self.lib_hint(vm, a, nargs, wanted);
        let z = self.kint(0);
        let r = self.lib_op(lib::TOSTR, Ty::ShrStr, x, z, None, hint)?;
        self.lib_tail(idx, func_rel, top_rel, &[r], true, wanted)
    }

    /// `rawlen(t)` / `rawlen(s)`: the length, no `__len`.
    fn lib_rawlen(
        &mut self,
        vm: &Vm<'_>,
        idx: usize,
        a: usize,
        nargs: usize,
        wanted: i32,
        addr: usize,
    ) -> R<usize> {
        if nargs != 1 {
            return Err(self.nyi());
        }
        let (func_rel, top_rel) = self.lib_head(vm, a, nargs, addr)?;
        let (x, t) = self.rd(vm, a + 1)?;
        let r = match t {
            Ty::Tab => {
                self.first_store.get_or_insert(self.b.ir.ins.len());
                self.b.emit_flags(TOp::Tlen, Ty::Int, 0, x, NOREF, 0)?
            }
            Ty::ShrStr | Ty::LngStr => self.b.emit_flags(
                TOp::Tlen,
                Ty::Int,
                0,
                x,
                NOREF,
                if t == Ty::ShrStr { 1 } else { 2 },
            )?,
            _ => return Err(self.nyi()),
        };
        // the lockstep reads the border from the VM (the pushed result)
        self.post_slot = Some(vm.base + a + 1 + nargs);
        self.lib_tail(idx, func_rel, top_rel, &[r], true, wanted)
    }

    /// The border the recorder expects of a table without metatable: the leading non-nil elements of its array part (an estimate; a
    /// wrong one is caught by the lockstep, which compares the VM's elements with the model's).
    fn lib_border(tbl: TableRef) -> i64 {
        let mut n = 0i64;
        while n < i64::from(tbl.array_size()) && tbl.peek_int_arr(n + 1).is_some() {
            n += 1;
        }
        n
    }

    /// The only border of `tbl` when it has exactly one (the array part is a prefix of non-nils followed by nils only, and when the prefix fills
    /// the array part, `t[asize + 1]` is absent); `None` otherwise (`#t` may be any of several borders, or need the hash part).
    fn lib_border_exact(tbl: TableRef) -> Option<i64> {
        let asize = i64::from(tbl.array_size());
        let n = Self::lib_border(tbl);
        for k in n + 1..=asize {
            if tbl.peek_int_arr(k).is_some() {
                return None;
            }
        }
        if n == asize && !tbl.get_int(asize + 1).is_nil() {
            return None;
        }
        Some(n)
    }

    /// BUNDLE-R (`libcall` bit 5): `table.unpack(t)` of a table without a metatable whose `#t` is at most 16: one `TLEN` guarded to the
    /// length seen, then the elements `t[1] .. t[n]` as array loads (no helper). The results are pushed above the arguments as the native does.
    fn lib_tunpack(
        &mut self,
        vm: &Vm<'_>,
        idx: usize,
        a: usize,
        nargs: usize,
        wanted: i32,
        addr: usize,
    ) -> R<usize> {
        const MAX_UNPACK: i64 = 16;
        if nargs != 1 {
            return self.lib_unrecorded(idx);
        }
        let tv = vm.slot(a + 1);
        // (a shape this does not record is a plain C call, as with the bit off: it stitches or aborts `CFunc`)
        let Some(tbl) = tv.as_table().filter(|t| t.metatable().is_none()) else {
            return self.lib_unrecorded(idx);
        };
        let Some(nobs) = Self::lib_border_exact(tbl).filter(|&n| n <= MAX_UNPACK) else {
            return self.lib_unrecorded(idx);
        };
        let (func_rel, top_rel) = self.lib_head(vm, a, nargs, addr)?;
        let t = self.rd(vm, a + 1)?;
        if t.1 != Ty::Tab {
            return Err(self.nyi());
        }
        if !mutants::active("tunpack_meta_skip") {
            self.meta_guard(t.0, tbl, rn_core::meta::Tm::Len)?;
        }
        let n = self.b.emit_flags(TOp::Tlen, Ty::Int, 0, t.0, NOREF, 0)?;
        self.post_val = Some(Value::int(nobs));
        if !mutants::active("tunpack_len_guard_skip") {
            let kn = self.kint(nobs);
            self.guard(TOp::Eq, Ty::I64, n, kn, 0)?;
        }
        let mut res = Vec::new();
        for p in 1..=nobs {
            let key = self.kint(p + i64::from(mutants::active("tunpack_off_by_one")));
            res.push(self.array_get(vm, t, a + 1, key, p)?);
        }
        // the native pushes every result above its arguments
        self.need_ext(top_rel + res.len() + 1)?;
        for (p, &r) in res.iter().enumerate().skip(1) {
            self.def_abs(top_rel + p, r);
        }
        self.lib_tail(idx, func_rel, top_rel, &res, true, wanted)
    }

    /// BUNDLE-R: a call of a library function the recorder knows but not in this shape: what a plain C call does (stitch, or `CFunc`); a tail call keeps
    /// its `TailCall` abort.
    fn lib_unrecorded(&mut self, idx: usize) -> R<usize> {
        if self.tail_lib {
            return Err(AbortReason::TailCall);
        }
        self.stitch(idx, AbortReason::CFunc)
    }

    /// BUNDLE-R (`libcall` bit 6): `table.move(a1, f, e, t [, a2])` of tables without metatables, at most 16 elements (`1 <= f <= e`, `t >= 1`): `f`, `e`, `t` are
    /// guarded to the values seen (they fold when constants), the two tables to be the same / different objects as seen, and the copy is
    /// unrolled in the library's order (`fwd`: first to last, else last to first) as an array load and store per element (the stores keep the
    /// write barrier and the growth path of `table.insert`). The result is the destination table.
    fn lib_tmove(
        &mut self,
        vm: &Vm<'_>,
        idx: usize,
        a: usize,
        nargs: usize,
        wanted: i32,
        addr: usize,
    ) -> R<usize> {
        const MAX_MOVE: i64 = 16;
        if !(4..=5).contains(&nargs) {
            return self.lib_unrecorded(idx);
        }
        // (a shape this does not record is a plain C call, as with the bit off: it stitches or aborts `CFunc`)
        let Some(ints) = (2..=4)
            .map(|j| vm.slot(a + j).as_int())
            .collect::<Option<Vec<_>>>()
        else {
            return self.lib_unrecorded(idx);
        };
        let (f, e, t) = (ints[0], ints[1], ints[2]);
        let n = e.wrapping_sub(f).wrapping_add(1);
        if f < 1 || t < 1 || e < f || n > MAX_MOVE || t > i64::MAX - MAX_MOVE {
            return self.lib_unrecorded(idx);
        }
        let sv = vm.slot(a + 1);
        let Some(stbl) = sv.as_table() else {
            return self.lib_unrecorded(idx);
        };
        let (dslot, dtbl) = if nargs == 5 && !vm.slot(a + 5).is_nil() {
            let Some(dt) = vm.slot(a + 5).as_table() else {
                return self.lib_unrecorded(idx);
            };
            (a + 5, dt)
        } else {
            (a + 1, stbl)
        };
        if stbl.metatable().is_some() || dtbl.metatable().is_some() {
            return self.lib_unrecorded(idx);
        }
        let (func_rel, top_rel) = self.lib_head(vm, a, nargs, addr)?;
        self.need_ext(top_rel + 1)?;
        let (fr, _) = self.lib_int_arg(vm, a + 2)?;
        let (er, _) = self.lib_int_arg(vm, a + 3)?;
        let (tr, _) = self.lib_int_arg(vm, a + 4)?;
        let src = self.rd(vm, a + 1)?;
        let dst = if dslot == a + 1 {
            src
        } else if nargs == 5 {
            // (the fifth argument may be nil: the destination is then the source; a nil is a type guard of the load)
            self.rd(vm, dslot)?
        } else {
            src
        };
        if src.1 != Ty::Tab || dst.1 != Ty::Tab {
            return Err(self.nyi());
        }
        if !mutants::active("tmove_args_guard_skip") {
            for (r, v) in [(fr, f), (er, e), (tr, t)] {
                let k = self.kint(v);
                self.guard(TOp::Eq, Ty::I64, r, k, 0)?;
            }
        }
        let same = stbl.as_ptr() == dtbl.as_ptr();
        if !mutants::active("tmove_same_guard_skip") && src.0 != dst.0 {
            self.guard(
                if same { TOp::Eqref } else { TOp::Ne },
                Ty::I64,
                src.0,
                dst.0,
                0,
            )?;
        }
        // (the library: `t > e || t <= f || (a2 given and different from a1)`)
        let fwd = t > e || t <= f || (dslot != a + 1 && !same);
        // the lockstep copies elements of the model lazily, after the VM has run the call: copy every element the IR reads or overwrites now
        let (hs, hd) = (self.handle(stbl), self.handle(dtbl));
        for i in 0..n {
            if f + i <= i64::from(stbl.array_size()) {
                self.fill_elem(hs, (f + i - 1) as usize);
            }
            if t + i <= i64::from(dtbl.array_size()) {
                self.fill_elem(hd, (t + i - 1) as usize);
            }
        }
        // Same table: the elements are read (type / bounds guards) BEFORE the first store. A guard that fails between two stores exits to
        // the `CALL`, which the interpreter re-executes from the start over the partly moved elements; an overlapping move is not
        // idempotent (`move(t, 2, 6, 3)` run twice over a half-done first run copies the wrong values). The loads of the copy loop
        // below are then ordinary (CSE-able) re-reads of elements that no store before them has touched. A different destination
        // table is idempotent (the sources are not modified), so it needs no pre-read. (BUNDLE-VA: `instruction_count_identity`
        // `br_lib` `+exitstorm=7` found a storm exit between two stores; a natural guard failure has the same effect.)
        if same && !mutants::active("tmove_no_preread") {
            for i in 0..n {
                let ks = self.kint(f + i);
                self.array_get(vm, src, a + 1, ks, f + i)?;
            }
        }
        let order: Vec<i64> = if fwd != mutants::active("tmove_order_flip") {
            (0..n).collect()
        } else {
            (0..n).rev().collect()
        };
        for i in order {
            let ks = self.kint(f + i);
            let v = self.array_get(vm, src, a + 1, ks, f + i)?;
            let ety = ty_of_value(&stbl.get_int(f + i)).ok_or_else(|| self.nyi())?;
            let kd = self.kint(t + i);
            self.array_set(vm, dst, dslot, kd, t + i, (v, ety))?;
        }
        self.lib_tail(idx, func_rel, top_rel, &[dst.0], true, wanted)
    }

    /// `table.insert(t, v)`: `t[#t + 1] = v` for a table without metatable.
    fn lib_tinsert(
        &mut self,
        vm: &Vm<'_>,
        idx: usize,
        a: usize,
        nargs: usize,
        wanted: i32,
        addr: usize,
    ) -> R<usize> {
        if nargs != 2 {
            return Err(self.nyi()); // the positional form shifts elements
        }
        let (func_rel, top_rel) = self.lib_head(vm, a, nargs, addr)?;
        let t = self.rd(vm, a + 1)?;
        if t.1 != Ty::Tab {
            return Err(self.nyi());
        }
        let tv = vm.slot(a + 1);
        let tbl = tv.as_table().ok_or(AbortReason::Diverge)?;
        if tbl.metatable().is_some() {
            return Err(AbortReason::MetaTable);
        }
        let val = self.rd(vm, a + 2)?;
        if val.1 == Ty::Nil {
            return Err(AbortReason::Insert);
        }
        // `aux_getn`: the length of a table without `__len` (the metatable guard of the store covers `__newindex`)
        if !mutants::active("tinsert_meta_skip") {
            self.meta_guard(t.0, tbl, rn_core::meta::Tm::Len)?;
        }
        self.first_store.get_or_insert(self.b.ir.ins.len());
        let n = self.b.emit_flags(TOp::Tlen, Ty::Int, 0, t.0, NOREF, 0)?;
        let nobs = Self::lib_border(tbl);
        self.post_val = Some(Value::int(nobs));
        let one = self.kint(1);
        let key = self.ins(TOp::Add, Ty::Int, n, one)?;
        self.array_set(vm, t, a + 1, key, nobs + 1, val)?;
        self.lib_tail(idx, func_rel, top_rel, &[], false, wanted)
    }

    /// `table.remove(t)`: the last element of a table without metatable, removed.
    fn lib_tremove(
        &mut self,
        vm: &Vm<'_>,
        idx: usize,
        a: usize,
        nargs: usize,
        wanted: i32,
        addr: usize,
    ) -> R<usize> {
        if nargs != 1 {
            return Err(self.nyi());
        }
        let (func_rel, top_rel) = self.lib_head(vm, a, nargs, addr)?;
        let t = self.rd(vm, a + 1)?;
        if t.1 != Ty::Tab {
            return Err(self.nyi());
        }
        let tv = vm.slot(a + 1);
        let tbl = tv.as_table().ok_or(AbortReason::Diverge)?;
        if tbl.metatable().is_some() {
            return Err(AbortReason::MetaTable);
        }
        let nobs = Self::lib_border(tbl);
        if nobs < 1 {
            return Err(self.nyi()); // t[0]
        }
        // the popped element is copied into the model now: the lazy copy at the IR's first read would run after the VM has removed it
        if nobs <= i64::from(tbl.array_size()) {
            let h = self.handle(tbl);
            self.fill_elem(h, (nobs - 1) as usize);
        }
        if !mutants::active("tinsert_meta_skip") {
            self.meta_guard(t.0, tbl, rn_core::meta::Tm::Len)?;
        }
        self.first_store.get_or_insert(self.b.ir.ins.len());
        let n = self.b.emit_flags(TOp::Tlen, Ty::Int, 0, t.0, NOREF, 0)?;
        self.post_val = Some(Value::int(nobs));
        // `res = t[n]`, then `t[n] = nil`
        let res = self.array_get(vm, t, a + 1, n, nobs)?;
        let nil = self.b.ir.konst(KVal::Pri(Ty::Nil));
        self.array_set(vm, t, a + 1, n, nobs, (nil, Ty::Nil))?;
        self.lib_tail(idx, func_rel, top_rel, &[res], true, wanted)
    }

    /// `tonumber(x)` / `math.tointeger(x)` of a number: the number itself (an integral float for `tointeger`).
    fn lib_num_identity(
        &mut self,
        vm: &Vm<'_>,
        idx: usize,
        a: usize,
        nargs: usize,
        wanted: i32,
        addr: usize,
        f: u8,
    ) -> R<usize> {
        if nargs != 1 {
            return Err(self.nyi());
        }
        let (func_rel, top_rel) = self.lib_head(vm, a, nargs, addr)?;
        let (x, t) = self.rd(vm, a + 1)?;
        let r = match (t, f == jit_hooks::libfn::MATH_TOINTEGER) {
            // BUNDLE-A (`libnum`): `tonumber(s)` of a numeral string: `STRTO` in tonumber mode, `NE 0` leaves on a non-numeral, the `HLOAD`
            // pins the observed result type (integer or float)
            (Ty::ShrStr | Ty::LngStr, false) if self.cfg.libnum => {
                let sv = vm.slot(a + 1);
                let Some(n) = sv
                    .as_strref()
                    .and_then(|s| rn_core::number::str2num(s.get().as_bytes()))
                else {
                    if self.cfg.libcall_mask & 128 == 0 {
                        return Err(self.nyi()); // not a numeral: the result is nil, no guard expresses it
                    }
                    // BUNDLE-R (`libcall` bit 7): `tonumber(s)` of a string that is no numeral: `STRTO` in tonumber mode answers 0 (it parses, allocates
                    // nothing), `EQ 0` pins the failure (a numeral exits), the result is `nil`. MUTANT `tonum_fail_guard_skip`: no guard
                    let flags = if mutants::active("tonum_mode_ignored") {
                        0
                    } else {
                        2
                    };
                    let p = self.b.emit_flags(TOp::Strto, Ty::P64, 0, x, NOREF, flags)?;
                    let z = self.kint(0);
                    if !mutants::active("tonum_fail_guard_skip") {
                        self.guard(TOp::Eq, Ty::I64, p, z, 0)?;
                    }
                    self.pending_strto = None;
                    let nil = self.b.ir.konst(KVal::Pri(Ty::Nil));
                    return self.lib_tail(idx, func_rel, top_rel, &[nil], true, wanted);
                };
                let (mv, rt) = match n {
                    rn_core::number::Number::Int(i) => (super::interp::Val::int(i), Ty::Int),
                    rn_core::number::Number::Float(f) => (super::interp::Val::num(f), Ty::Num),
                };
                let flags = if mutants::active("tonum_mode_ignored") {
                    0
                } else {
                    2
                };
                let p = self.b.emit_flags(TOp::Strto, Ty::P64, 0, x, NOREF, flags)?;
                let z = self.kint(0);
                self.guard(TOp::Ne, Ty::I64, p, z, 0)?;
                self.pending_strto = Some(mv);
                let bit = u32::from(!mutants::active("tonum_type_guard_skip"));
                self.guard(TOp::Hload, rt, p, NOREF, bit)?
            }
            (Ty::Int, _) => x,
            (Ty::Num, false) => x,
            (Ty::Num, true) => {
                let fv = vm.slot(a + 1).as_float().unwrap_or(f64::NAN);
                if float_to_int(fv, F2I::Eq).is_none() {
                    return Err(self.nyi()); // fail (nil): no guard expresses "not integral"
                }
                self.guard(TOp::F2iExact, Ty::Int, x, NOREF, 0)?
            }
            _ => return Err(self.nyi()),
        };
        self.lib_tail(idx, func_rel, top_rel, &[r], true, wanted)
    }

    /// BUNDLE-A (`libfind`): `string.find(s, p [, init [, plain]])` of two strings and an integer (or nil / absent) `init`, searched plainly: the
    /// `plain` argument is truthy (its type is guarded), or the pattern had no specials (the helper checks at run time and exits when it has
    /// some, kind `FINDS`). The helper's result is guarded against the outcome seen (found / not found); start is `r - 1`, end `start + #p - 1`.
    /// Number arguments that need a string conversion and real patterns are `Nyi`.
    fn lib_find(
        &mut self,
        vm: &Vm<'_>,
        idx: usize,
        a: usize,
        nargs: usize,
        wanted: i32,
        addr: usize,
    ) -> R<usize> {
        if !(2..=4).contains(&nargs) {
            return Err(self.nyi());
        }
        let (func_rel, top_rel) = self.lib_head(vm, a, nargs, addr)?;
        self.need_ext(top_rel + 3)?;
        let (s, _, _) = self.lib_str_arg(vm, a + 1)?;
        let (p, tp, plen) = self.lib_str_arg(vm, a + 2)?;
        let (init, vinit) = if nargs >= 3 {
            let (r, t) = self.rd(vm, a + 3)?;
            match t {
                Ty::Int => (r, vm.slot(a + 3).as_int().ok_or(AbortReason::Diverge)?),
                Ty::Nil => (self.kint(1), 1),
                _ => return Err(self.nyi()), // a float / string start converts
            }
        } else {
            (self.kint(1), 1)
        };
        let plain = if nargs == 4 {
            let (_, t) = self.rd(vm, a + 4)?;
            !matches!(t, Ty::Nil | Ty::False)
        } else {
            false
        };
        let sv = vm.slot(a + 1);
        let pv = vm.slot(a + 2);
        let (sb, pb) = match (sv.as_strref(), pv.as_strref()) {
            (Some(x), Some(y)) => (x.get().as_bytes(), y.get().as_bytes()),
            _ => return Err(AbortReason::Diverge),
        };
        if !plain && !rn_std::pattern::nospecials(pb) {
            return Err(self.nyi()); // a real pattern
        }
        let pos = rn_std::pattern::find_plain_pos(sb, pb, vinit);
        let hint = self.lib_hint(vm, a, nargs, wanted);
        let kind = if plain { lib::FIND } else { lib::FINDS };
        let r = self.lib_op(kind, Ty::Int, s, p, Some(init), hint)?;
        let one = self.kint(1);
        if !mutants::active("find_outcome_guard_skip") {
            self.guard(
                if pos.is_some() { TOp::Ne } else { TOp::Eq },
                Ty::I64,
                r,
                one,
                0,
            )?;
        }
        if pos.is_none() {
            let nil = self.b.ir.konst(KVal::Pri(Ty::Nil));
            self.def_abs(top_rel, nil);
            return self.lib_tail(idx, func_rel, top_rel, &[nil], false, wanted);
        }
        let start = self.ins(TOp::Sub, Ty::Int, r, one)?;
        // the length of the pattern (the model's value is the recorded one)
        self.post_val = Some(Value::int(plen as i64));
        let len = self.b.emit_flags(
            TOp::Tlen,
            Ty::Int,
            0,
            p,
            NOREF,
            if tp == Ty::ShrStr { 1 } else { 2 },
        )?;
        // MUTANT `find_end_off`: the end is one too large
        let two = self.kint(if mutants::active("find_end_off") {
            1
        } else {
            2
        });
        let t1 = self.ins(TOp::Sub, Ty::Int, r, two)?;
        let end = self.ins(TOp::Add, Ty::Int, t1, len)?;
        self.def_abs(top_rel, start);
        self.def_abs(top_rel + 1, end);
        self.lib_tail(idx, func_rel, top_rel, &[start, end], false, wanted)
    }

    /// BUNDLE-A (`libnext`): the plain call `next(t [, k])` (1 or 2 arguments, a table, a control value the helper can pass or nil): the
    /// `TNEXT` path of the generic `for` (docs/JIT_TRACE_REF.md "BUNDLE-A"). The snapshot is the call's: a failed guard (the key is gone,
    /// another type, the traversal ended / did not end) re-runs the call in the interpreter, which raises its own errors. The helper updates
    /// `heap.next_hint` exactly like `base_next`. Results: key, value (two values), or the single `nil` of the end.
    fn next_call(
        &mut self,
        vm: &Vm<'_>,
        idx: usize,
        a: usize,
        nargs: usize,
        wanted: i32,
        addr: usize,
    ) -> R<usize> {
        if nargs == 0 || nargs > 2 {
            return Err(self.nyi());
        }
        let (func_rel, top_rel) = self.lib_head(vm, a, nargs, addr)?;
        self.need_ext(top_rel + 3)?;
        let st = self.rd(vm, a + 1)?;
        if st.1 != Ty::Tab {
            return Err(self.nyi()); // `next` raises
        }
        let nil_k = self.b.ir.konst(KVal::Pri(Ty::Nil));
        let (cv, ct) = if nargs == 2 {
            (vm.slot(a + 2), self.rd(vm, a + 2)?)
        } else {
            (Value::NIL, (nil_k, Ty::Nil))
        };
        let supported = |t: Ty| {
            matches!(
                t,
                Ty::Int
                    | Ty::Num
                    | Ty::ShrStr
                    | Ty::LngStr
                    | Ty::Tab
                    | Ty::True
                    | Ty::False
                    | Ty::Nil
            )
        };
        if !supported(ct.1) {
            return Err(self.nyi());
        }
        let tbl = vm.slot(a + 1).as_table().ok_or(AbortReason::Diverge)?;
        let (k, v) = match tbl.next(&cv) {
            Ok(Some(kv)) => kv,
            Ok(None) => (Value::NIL, Value::NIL),
            Err(_) => return Err(AbortReason::Error), // "invalid key to 'next'": the interpreter raises
        };
        let ended = k.is_nil();
        let (tk, tv) = match (ty_of_value(&k), ty_of_value(&v)) {
            (Some(tk), Some(tv)) if (supported(tk) || ended) && (supported(tv) || ended) => {
                (tk, tv)
            }
            _ => return Err(self.nyi()),
        };
        self.touch(&k)?;
        self.touch(&v)?;
        let mv = (self.val_of(&k), self.val_of(&v));
        let z = self.kint(0);
        // MUTANT `next_ctl_nil`: the traversal always restarts (the control is passed as nil)
        let ctl = if mutants::active("next_ctl_nil") {
            nil_k
        } else {
            ct.0
        };
        let p = self.ins(TOp::Tnext, Ty::P64, st.0, ctl)?;
        if !mutants::active("tnext_ne_guard_skip") {
            self.guard(TOp::Ne, Ty::I64, p, z, 0)?;
        }
        self.pending_next = Some(mv);
        let kbits = if mutants::active("tnext_key_type_skip") {
            0
        } else {
            1
        };
        let kr = self.guard(TOp::Hload, tk, p, NOREF, kbits | 2)?;
        if ended {
            self.def_abs(top_rel, kr);
            return self.lib_tail(idx, func_rel, top_rel, &[kr], false, wanted);
        }
        let vr = self.guard(TOp::Hload, tv, p, NOREF, 1)?;
        self.def_abs(top_rel, kr);
        self.def_abs(top_rel + 1, vr);
        self.lib_tail(idx, func_rel, top_rel, &[kr, vr], false, wanted)
    }

    /// `math.fmod(x, y)` of two floats (C `fmod`); integers and mixed arguments are not recorded.
    fn lib_fmod(
        &mut self,
        vm: &Vm<'_>,
        idx: usize,
        a: usize,
        nargs: usize,
        wanted: i32,
        addr: usize,
    ) -> R<usize> {
        if nargs != 2 {
            return Err(self.nyi());
        }
        let (func_rel, top_rel) = self.lib_head(vm, a, nargs, addr)?;
        let (x, tx) = self.rd(vm, a + 1)?;
        let (y, ty) = self.rd(vm, a + 2)?;
        if tx != Ty::Num || ty != Ty::Num {
            return Err(self.nyi());
        }
        let r = self
            .b
            .emit(TOp::Calln, Ty::Num, x, y, interp::CALLN_CFMOD)?;
        self.lib_tail(idx, func_rel, top_rel, &[r], true, wanted)
    }
}

// ---- N3: metamethods that are Lua closures, run in place by the interpreter (`mm_frame`), recorded as inlined frames ----------------

impl Rec {
    /// Does the `MMBIN*` behind the arithmetic instruction at `idx` have a table operand? (Then the interpreter, finding no number
    /// arithmetic to do, falls through to it and calls the table's metamethod.)
    fn mm_pair_has_table(vm: &Vm<'_>, idx: usize) -> bool {
        let Some(&w) = vm.code.get(idx + 1) else {
            return false;
        };
        let m = Instruction(w);
        match m.opcode() {
            Some(OpCode::Mmbin) => {
                vm.slot(m.a() as usize).is_table() || vm.slot(m.b() as usize).is_table()
            }
            Some(OpCode::Mmbini | OpCode::Mmbink) => vm.slot(m.a() as usize).is_table(),
            _ => false,
        }
    }

    /// The guards of the metamethod `ev` provided by the table operand `o` (the operand `get_tm_by_obj` finds it in): its
    /// metatable is not nil, the metatable's node for the event name is the recorded one (`HREFK`), holds a Lua closure (`HLOAD`
    /// typecheck) of the recorded prototype (`EQREF`). Returns the closure and its ref. A metamethod that is not a Lua closure is
    /// called by the interpreter through `ccall` (no frame to inline): `Metamethod`.
    fn mm_provider(
        &mut self,
        vm: &Vm<'_>,
        o: MmOpd,
        ev: Tm,
    ) -> R<(rn_core::value::Gc<LClosure>, Ref)> {
        if o.t != Ty::Tab {
            return Err(AbortReason::Metamethod);
        }
        let tbl = o.v.as_table().ok_or(AbortReason::Diverge)?;
        let m = tbl.metatable().ok_or(AbortReason::Metamethod)?;
        let name = vm.st.g().heap.tm_name(ev);
        let ix = m.short_str_node(name).ok_or(AbortReason::Metamethod)?;
        if ix >= 1 << 15 {
            return Err(AbortReason::TableHash);
        }
        let tmv = m.node_value(ix);
        let cl = tmv.as_lclosure().ok_or(AbortReason::Metamethod)?;
        let mr = self.fload(o.r, Field::Metatable, Ty::P64)?;
        let z = self.kint(0);
        if !mutants::active("mm_mt_null_skip") {
            self.guard(TOp::Ne, Ty::I64, mr, z, 0)?;
        }
        self.touch(&Value::table(m))?;
        self.touch(&tmv)?;
        let kname = self.b.ir.konst(KVal::Ptr(
            payload_bits(&Value::string(name), Ty::ShrStr),
            PtrKind::Raw,
        ));
        let hr = self.hash_ref(mr, m, kname, ix)?;
        let bit = u32::from(!mutants::active("mm_tm_typecheck_skip"));
        let cref = self.guard(TOp::Hload, Ty::Lcf, hr, NOREF, bit)?;
        let p = cl.proto();
        let key = &*p as *const Proto as usize;
        let kp = self.b.ir.konst(KVal::Ptr(key as u64, PtrKind::Proto));
        if !mutants::active("mm_callee_guard_skip") {
            self.guard(TOp::Eqref, Ty::I64, cref, kp, 0)?;
        }
        Ok((cl, cref))
    }

    /// The proof that operand `o` provides no metamethod `ev` (`get_tm_by_obj` finds nil, the interpreter goes on to the next
    /// operand): a number whose type has no metatable (assignments to a type metatable flush the traces, `type_mt_changed`), a
    /// table without a metatable or whose metatable has no such key (an `HMISS` walk, or a nil-valued node).
    fn mm_absent(&mut self, vm: &Vm<'_>, o: MmOpd, ev: Tm) -> R<()> {
        match o.t {
            Ty::Int | Ty::Num => {
                if vm
                    .st
                    .type_metatable(rn_core::value::LuaType::Number)
                    .is_some()
                    && !mutants::active("mm_num_mt_skip")
                {
                    return Err(AbortReason::Metamethod);
                }
                Ok(())
            }
            Ty::Tab => {
                let tbl = o.v.as_table().ok_or(AbortReason::Diverge)?;
                let mr = self.fload(o.r, Field::Metatable, Ty::P64)?;
                let z = self.kint(0);
                let Some(m) = tbl.metatable() else {
                    if !mutants::active("mm_absent_skip") {
                        self.guard(TOp::Eq, Ty::I64, mr, z, 0)?;
                    }
                    return Ok(());
                };
                if !mutants::active("mm_absent_skip") {
                    self.guard(TOp::Ne, Ty::I64, mr, z, 0)?;
                }
                let name = vm.st.g().heap.tm_name(ev);
                let kname = self.b.ir.konst(KVal::Ptr(
                    payload_bits(&Value::string(name), Ty::ShrStr),
                    PtrKind::Raw,
                ));
                match m.short_str_node(name) {
                    Some(ix) => {
                        if !m.node_value(ix).is_nil() || ix >= 1 << 15 {
                            return Err(AbortReason::Diverge);
                        }
                        if !mutants::active("mm_absent_skip") {
                            let hr = self.hash_ref(mr, m, kname, ix)?;
                            self.guard(TOp::Hload, Ty::Nil, hr, NOREF, 1)?;
                        }
                    }
                    None => {
                        if !mutants::active("mm_absent_skip") {
                            self.hash_absent(mr, m, (kname, name))?;
                        }
                    }
                }
                Ok(())
            }
            _ => Err(AbortReason::Metamethod),
        }
    }

    /// `mm_bin`: the event `ev` of the first operand, else of the second (`get_tm_by_obj` order). Guards the provider
    /// ([`Rec::mm_provider`]) and, when it is the second operand, the absence in the first ([`Rec::mm_absent`]).
    fn mm_binary(
        &mut self,
        vm: &Vm<'_>,
        ev: Tm,
        o1: MmOpd,
        o2: MmOpd,
    ) -> R<(rn_core::value::Gc<LClosure>, Ref)> {
        if !vm.st.get_tm_by_obj(&o1.v, ev).is_nil() {
            return self.mm_provider(vm, o1, ev);
        }
        if vm.st.get_tm_by_obj(&o2.v, ev).is_nil() {
            return Err(AbortReason::Metamethod); // an error
        }
        self.mm_absent(vm, o1, ev)?;
        self.mm_provider(vm, o2, ev)
    }

    /// The C-stack depth the in-place metamethod frames add to the interpreter's `nCcalls` must stay in reach of the limit (the
    /// interpreter falls back to `ccall`, which raises "C stack overflow", beyond it): the same helper guard as an inlined `pcall`.
    fn mm_ccalls(&mut self) -> R<()> {
        if mutants::active("mm_ccalls_skip") {
            return Ok(());
        }
        let n = 1 + self
            .frames
            .iter()
            .filter(|f| f.mm.is_some() || f.pc_shift != 0)
            .count();
        // N3b: `nCcalls` is the same at every point of the trace that is not inside a deeper in-place frame, and a check for `n`
        // levels implies the checks for fewer: one helper call per depth is enough
        if n as u32 <= self.ccalls_done {
            return Ok(());
        }
        self.ccalls_done = self.ccalls_done.max(n as u32);
        let k = self.kint(n as i64);
        self.lib_op(lib::CCALLS, Ty::Int, k, k, None, usize::MAX)?;
        Ok(())
    }

    /// An arithmetic instruction (at `idx`) with a table operand: the guards of the metamethod call are emitted now (their
    /// snapshots are the state before the arithmetic instruction, which tier 1 can land on), the frame is pushed by the `MMBIN*`
    /// step the interpreter executes next ([`Rec::mm_bridge`]).
    fn mm_arith(&mut self, vm: &Vm<'_>, idx: usize) -> R<usize> {
        let prev = Instruction(vm.code[idx]);
        let m = Instruction(*vm.code.get(idx + 1).ok_or(AbortReason::Diverge)?);
        let ev = Tm::from_u8(m.c() as u8);
        let reg = |s: &mut Rec, n: usize| -> R<MmOpd> {
            let (r, t) = s.rd(vm, n)?;
            Ok(MmOpd {
                r,
                t,
                v: vm.slot(n),
            })
        };
        let (o1, o2) = match m.opcode() {
            Some(OpCode::Mmbin) => (reg(self, m.a() as usize)?, reg(self, m.b() as usize)?),
            Some(OpCode::Mmbini) => {
                let x = reg(self, m.a() as usize)?;
                let (r, t, v) = self.imm(i64::from(m.sb()));
                let k = MmOpd { r, t, v };
                if m.k() != 0 {
                    (k, x)
                } else {
                    (x, k)
                }
            }
            Some(OpCode::Mmbink) => {
                let x = reg(self, m.a() as usize)?;
                let (r, t, v) = self.kconst(vm, m.b() as usize)?;
                let k = MmOpd { r, t, v };
                if m.k() != 0 {
                    (k, x)
                } else {
                    (x, k)
                }
            }
            _ => return Err(self.nyi()),
        };
        self.mm_ccalls()?;
        let (cl, cref) = self.mm_binary(vm, ev, o1, o2)?;
        self.mm_bridge = Some(MmCall {
            cl,
            cref,
            args: [o1.r, o2.r, NOREF],
            nargs: 2,
            fin: MmFin::Reg(prev.a() as u16),
        });
        Ok(idx + 1)
    }

    /// `UNM` / `BNOT` / `LEN` of a table: the metamethod gets the operand twice (`mm_bin(.., rb, rb, ev)`); its first result goes to `R[a]`.
    fn mm_unary(
        &mut self,
        vm: &Vm<'_>,
        idx: usize,
        ev: Tm,
        x: Ref,
        a: usize,
        b: usize,
    ) -> R<usize> {
        let o = MmOpd {
            r: x,
            t: Ty::Tab,
            v: vm.slot(b),
        };
        self.mm_ccalls()?;
        let (cl, cref) = self.mm_binary(vm, ev, o, o)?;
        let func_local = self.frames[self.cur_depth].nslots;
        let c = MmCall {
            cl,
            cref,
            args: [x, x, NOREF],
            nargs: 2,
            fin: MmFin::Reg(a as u16),
        };
        self.mm_push(vm, idx, func_local, c, 1)
    }

    /// `CONCAT` of two operands, one of them a table (`luaV_concat` -> `luaT_tryconcatTM`; the interpreter runs the metamethod in
    /// place at `R[a + 2]` and `finish_op` stores the result in `R[a]`).
    fn mm_concat(&mut self, vm: &Vm<'_>, idx: usize, a: usize) -> R<usize> {
        let (r1, t1) = self.rd(vm, a)?;
        let (r2, t2) = self.rd(vm, a + 1)?;
        let o1 = MmOpd {
            r: r1,
            t: t1,
            v: vm.slot(a),
        };
        let o2 = MmOpd {
            r: r2,
            t: t2,
            v: vm.slot(a + 1),
        };
        self.mm_ccalls()?;
        let (cl, cref) = self.mm_binary(vm, Tm::Concat, o1, o2)?;
        let c = MmCall {
            cl,
            cref,
            args: [r1, r2, NOREF],
            nargs: 2,
            fin: MmFin::Reg(a as u16),
        };
        self.mm_push(vm, idx, a + 2, c, 1)
    }

    /// Pushes the frame of the metamethod `c.cl` as `mm_frame` + `pre_call_lua_fast` build it: the function slot `func_local`
    /// registers above the caller's base holds the closure, the arguments follow, missing parameters are nil. The caller resumes
    /// at `call_pc + 1`; the result goes where `c.fin` says (see [`Rec::ret_from`]).
    fn mm_push(
        &mut self,
        vm: &Vm<'_>,
        call_pc: usize,
        func_local: usize,
        c: MmCall,
        wanted: i32,
    ) -> R<usize> {
        let p = c.cl.proto();
        if p.is_vararg() {
            return Err(AbortReason::Vararg);
        }
        let key = &*p as *const Proto as usize;
        let d = self.cur_depth;
        let uses = self.frames.iter().filter(|f| f.proto == key).count();
        // BUNDLE-N: the same prototype on the frame stack again (middleclass's `__index` function of a subclass calls `super.static[k]`,
        // the parent's closure of the same prototype) is unrolled up to the recursion limit like a `CALL` (the closures may differ: the
        // callee guard compares the prototype, its upvalues are read through the closure ref)
        if uses > 0
            && (self.cfg.idxdeep & 4 == 0
                || self.cfg.unroll == 0
                || uses >= (self.cfg.unroll as usize).max(3))
        {
            return Err(AbortReason::Recursion);
        }
        let max_depth = if self.cfg.max_depth == 0 {
            MAX_DEPTH
        } else {
            (self.cfg.max_depth as usize).min(super::exit::MAX_FRAMES)
        };
        if d >= max_depth {
            return Err(AbortReason::Depth);
        }
        let func_rel = self.frames[d].base_off + func_local;
        let base_rel = func_rel + 1;
        let top_rel = base_rel + c.nargs;
        let nslots = usize::from(p.maxstacksize);
        self.need_ext(base_rel + nslots.max(c.nargs))?;
        let low = self.low_for(&p)?;
        if !self.deps.iter().any(|k| k.0 == key) {
            self.deps.push(ProtoKey(key));
            self.want_dep.push(p.clone());
        }
        let nil = self.b.ir.konst(KVal::Pri(Ty::Nil));
        // N3b: the state before the triggering instruction, for the allocation guards of the body
        let ectx = (self.cfg.mm && self.pcall.is_none()).then(|| {
            std::rc::Rc::new(EntryCtx {
                sm: self.st_sm.clone(),
                frames: self.frame_recs(),
                top: self.st_top,
                pc: self.st_pc,
                step: self.st_step,
                low: self.st_low,
                ins_at: self.b.ir.ins.len(),
            })
        });
        self.def_abs(func_rel, c.cref);
        for (j, &r) in c.args[..c.nargs].iter().enumerate() {
            self.def_abs(base_rel + j, r);
        }
        for j in c.nargs..usize::from(p.numparams) {
            self.def_abs(base_rel + j, nil);
        }
        self.room = self.room.max((top_rel + nslots) as u32);
        self.max_ci = self.max_ci.max((self.ci_depth(d) + 1) as u8);
        self.frames.push(RecFrame {
            ci: 0,
            func_abs: vm.base + func_local,
            proto: key,
            closure: c.cl.as_ptr() as usize,
            base_off: base_rel,
            nslots,
            call_pc: call_pc as u32,
            nresults: wanted as i16,
            low,
            cref: c.cref,
            tail: false,
            vpending: None,
            vdelta: 0,
            nextra: 0,
            pc_shift: 0,
            pc_wanted: 0,
            mm: Some(c.fin),
            ectx,
            tfor: None,
        });
        self.next_depth = d + 1;
        Ok(0)
    }
}

impl Rec {
    /// `EQ` of two tables (N3; the plain path refuses table equality): raw equality is a pointer guard; two different tables call
    /// the `__eq` of the first operand's metatable, else of the second's (`mm_eq`), converted to a condition; without one the answer is
    /// false and both tables are proven to have none.
    fn mm_eq(
        &mut self,
        vm: &Vm<'_>,
        idx: usize,
        i: Instruction,
        x: (Ref, Ty),
        y: (Ref, Ty),
    ) -> R<usize> {
        let (a, b) = (i.a() as usize, i.b() as usize);
        let (vx, vy) = (vm.slot(a), vm.slot(b));
        let (tx, ty) = (
            vx.as_table().ok_or(AbortReason::Diverge)?,
            vy.as_table().ok_or(AbortReason::Diverge)?,
        );
        if tx.as_ptr() == ty.as_ptr() {
            if !mutants::active("mm_eq_raw_skip") {
                self.guard(TOp::Eq, Ty::I64, x.0, y.0, 0)?;
            }
            return self.cond_next(vm, idx, i, true);
        }
        if !mutants::active("mm_eq_raw_skip") {
            self.guard(TOp::Ne, Ty::I64, x.0, y.0, 0)?;
        }
        let (o1, o2) = (
            MmOpd {
                r: x.0,
                t: x.1,
                v: vx,
            },
            MmOpd {
                r: y.0,
                t: y.1,
                v: vy,
            },
        );
        if vm.st.get_tm_by_obj(&vx, Tm::Eq).is_nil() && vm.st.get_tm_by_obj(&vy, Tm::Eq).is_nil() {
            if !mutants::active("mm_eq_absent_skip") {
                self.mm_absent(vm, o1, Tm::Eq)?;
                self.mm_absent(vm, o2, Tm::Eq)?;
            }
            return self.cond_next(vm, idx, i, false);
        }
        self.mm_order(vm, idx, i, Tm::Eq, o1, o2)
    }

    /// A comparison (`EQ`, `LT`, `LE`, `LTI` ...) that calls a metamethod with the operands `o1`, `o2` (in the order `mm_bin` gets
    /// them); its result decides the condition ([`MmFin::Cond`]).
    fn mm_order(
        &mut self,
        vm: &Vm<'_>,
        idx: usize,
        i: Instruction,
        ev: Tm,
        o1: MmOpd,
        o2: MmOpd,
    ) -> R<usize> {
        self.mm_ccalls()?;
        let (cl, cref) = self.mm_binary(vm, ev, o1, o2)?;
        let func_local = self.frames[self.cur_depth].nslots;
        let c = MmCall {
            cl,
            cref,
            args: [o1.r, o2.r, NOREF],
            nargs: 2,
            fin: MmFin::Cond(i.k() != 0),
        };
        self.mm_push(vm, idx, func_local, c, 1)
    }
}

impl Rec {
    /// The guards of the closure `cl` stored under `ev` in the metatable `m` of the table `tr` (node `ix`): `mt != 0`, `HREFK`, the
    /// typechecked `HLOAD` of a Lua closure, the `EQREF` of its prototype. Returns the closure's ref.
    fn mm_field_closure(
        &mut self,
        vm: &Vm<'_>,
        tr: Ref,
        m: TableRef,
        ix: usize,
        ev: Tm,
        cl: rn_core::value::Gc<LClosure>,
    ) -> R<Ref> {
        if ix >= 1 << 15 {
            return Err(AbortReason::TableHash);
        }
        let mr = self.fload(tr, Field::Metatable, Ty::P64)?;
        let z = self.kint(0);
        if !mutants::active("mm_mt_null_skip") {
            self.guard(TOp::Ne, Ty::I64, mr, z, 0)?;
        }
        let tmv = m.node_value(ix);
        self.touch(&Value::table(m))?;
        self.touch(&tmv)?;
        let name = vm.st.g().heap.tm_name(ev);
        let kname = self.b.ir.konst(KVal::Ptr(
            payload_bits(&Value::string(name), Ty::ShrStr),
            PtrKind::Raw,
        ));
        let hr = self.hash_ref(mr, m, kname, ix)?;
        let bit = u32::from(!mutants::active("mm_tm_typecheck_skip"));
        let cref = self.guard(TOp::Hload, Ty::Lcf, hr, NOREF, bit)?;
        let p = cl.proto();
        let kp = self
            .b
            .ir
            .konst(KVal::Ptr(&*p as *const Proto as u64, PtrKind::Proto));
        if !mutants::active("mm_callee_guard_skip") {
            self.guard(TOp::Eqref, Ty::I64, cref, kp, 0)?;
        }
        Ok(cref)
    }

    /// The key of a lookup as a value (a string-typed ref): a dynamic key is its register's ref, a constant one a `KPTR` of the string.
    fn mm_str_key(&mut self, key: (Ref, StrRef)) -> R<Ref> {
        if TraceIr::is_ins(key.0) {
            return Ok(key.0);
        }
        let id = (usize::MAX, key.1.as_ptr() as usize);
        if let Some(&r) = self.kstr.get(&id) {
            return Ok(r);
        }
        let r = self.b.emit_raw(Ins {
            op: TOp::Kptr,
            t: Ty::ShrStr,
            flags: 0,
            a: key.0,
            b: NOREF,
            prev: NOREF,
            aux: 0,
        })?;
        self.kstr.insert(id, r);
        Ok(r)
    }

    /// `__index` function of the metatable `m` (node `ix`) of the table `tr`, found by a lookup of `key` that missed: the call
    /// `__index(t, key)` (`mm_index`) is left in [`Rec::mm_pending`].
    fn mm_index_fn(
        &mut self,
        vm: &Vm<'_>,
        tr: Ref,
        m: TableRef,
        ix: usize,
        cl: rn_core::value::Gc<LClosure>,
        key: Ref,
    ) -> R<()> {
        self.mm_ccalls()?;
        let cref = self.mm_field_closure(vm, tr, m, ix, Tm::Index, cl)?;
        self.mm_pending = Some(MmCall {
            cl,
            cref,
            args: [tr, key, NOREF],
            nargs: 2,
            fin: MmFin::Reg(0),
        });
        Ok(())
    }

    /// `t[k]` for an integer key outside the array part of a table with a metatable whose `__index` is a Lua closure (5.5): the raw
    /// lookup misses when the key is outside the array part (guarded) and the hash part is the shared dummy node (flag bit, guarded).
    fn mm_index_int(&mut self, vm: &Vm<'_>, tr: Ref, tbl: TableRef, key: Ref, orig: Ref) -> R<()> {
        let m = tbl.metatable().ok_or(AbortReason::Diverge)?;
        let name = vm.st.g().heap.tm_name(Tm::Index);
        let ix = m.short_str_node(name).ok_or(AbortReason::MetaTable)?;
        let cl = m
            .node_value(ix)
            .as_lclosure()
            .ok_or(AbortReason::MetaTable)?;
        if tbl.flags() & rn_core::table::BITDUMMY == 0 {
            return Err(AbortReason::TableHash);
        }
        let h = self.handle(tbl);
        let _ = h;
        let asize = self.fload(tr, Field::Asize, Ty::U32)?;
        let one = self.kint(1);
        let idx0 = self.ins(TOp::Sub, Ty::Int, key, one)?;
        if !mutants::active("mm_index_range_skip") {
            self.guard(TOp::Uge, Ty::I64, idx0, asize, 0)?;
        }
        let fl = self.fload(tr, Field::Flags, Ty::U32)?;
        let bit = self.kint(i64::from(rn_core::table::BITDUMMY));
        let masked = self.ins(TOp::Band, Ty::I64, fl, bit)?;
        let z = self.kint(0);
        if !mutants::active("mm_index_dummy_skip") {
            self.guard(TOp::Ne, Ty::I64, masked, z, 0)?;
        }
        self.mm_index_fn(vm, tr, m, ix, cl, orig)
    }

    /// `t[key] = val` for a short-string key that has no non-nil value in the table whose metatable has a Lua closure `__newindex`: the
    /// call `__newindex(t, key, val)` (`mm_newindex`, results discarded) is left in [`Rec::mm_pending`]; the key is proven absent (or
    /// nil-valued) first. `false`: not this case (the plain store paths apply).
    fn mm_newindex_fn(
        &mut self,
        vm: &Vm<'_>,
        tbl: TableRef,
        t: (Ref, Ty),
        key: (Ref, StrRef),
        val: (Ref, Ty),
    ) -> R<bool> {
        let Some(m) = tbl.metatable() else {
            return Ok(false);
        };
        let nv = tbl.short_str_node(key.1);
        if nv.is_some_and(|ix| tbl.node_value(ix).tag() & 0x0F != LUA_TNIL) {
            return Ok(false); // an existing key: a raw store
        }
        let name = vm.st.g().heap.tm_name(Tm::NewIndex);
        let Some(ix) = m.short_str_node(name) else {
            return Ok(false);
        };
        let Some(cl) = m.node_value(ix).as_lclosure() else {
            return Ok(false);
        };
        if val.1 == Ty::Nil && nv.is_none() {
            // (the plain path refuses a nil value too; the metamethod is called with it all the same)
        }
        self.mm_ccalls()?;
        match nv {
            Some(ix) => {
                let hr = self.hash_ref(t.0, tbl, key.0, ix)?;
                self.guard(TOp::Hload, Ty::Nil, hr, NOREF, 1)?;
            }
            None => {
                if !mutants::active("mm_newindex_absent_skip") {
                    self.hash_absent(t.0, tbl, key)?;
                }
            }
        }
        let cref = self.mm_field_closure(vm, t.0, m, ix, Tm::NewIndex, cl)?;
        let kr = self.mm_str_key(key)?;
        self.mm_pending = Some(MmCall {
            cl,
            cref,
            args: [t.0, kr, val.0],
            nargs: 3,
            fin: MmFin::Drop,
        });
        Ok(true)
    }

    /// Pushes the call [`Rec::mm_pending`] left, for the instruction at `idx`; `wanted_result`: the result goes to register `a`.
    fn mm_push_pending(
        &mut self,
        vm: &Vm<'_>,
        idx: usize,
        a: usize,
        wanted_result: bool,
    ) -> R<usize> {
        let mut c = self.mm_pending.take().ok_or(AbortReason::Diverge)?;
        if wanted_result {
            c.fin = MmFin::Reg(a as u16);
        }
        let func_local = self.frames[self.cur_depth].nslots;
        self.mm_push(vm, idx, func_local, c, i32::from(wanted_result))
    }

    /// `CALL` of a table with a Lua closure `__call` (`luaD_tryfuncTM`): the object moves to the first argument, the arguments one slot
    /// up, the closure into the function slot; then an ordinary call of the closure.
    fn mm_call(
        &mut self,
        vm: &Vm<'_>,
        idx: usize,
        a: usize,
        nargs: usize,
        wanted: i32,
        tail: bool,
    ) -> R<usize> {
        let (r0, t0) = self.rd(vm, a)?;
        let o = MmOpd {
            r: r0,
            t: t0,
            v: vm.slot(a),
        };
        let mut refs = Vec::with_capacity(nargs + 1);
        refs.push(r0);
        for j in 1..=nargs {
            refs.push(self.rd(vm, a + j)?.0);
        }
        let (cl, cref) = self.mm_provider(vm, o, Tm::Call)?;
        self.def(a, cref);
        // (`luaD_tryfuncTM` writes the shifted arguments into the frame's registers also for a tail call: they stay behind as stale
        // values above the moved frame)
        for (j, &r) in refs.iter().enumerate() {
            let slot = a + 1 + j + usize::from(mutants::active("mm_call_shift_off"));
            self.def(slot, r);
        }
        if tail {
            // the shifted slots can lie beyond the frame's registers (`rd` refuses them): the tail path takes the refs directly
            self.tail_args = Some(refs);
        }
        self.call_lua(vm, idx, a, nargs + 1, wanted, cl, tail)
    }
}

thread_local! {
    /// `loopany` guard: the sites the last closed recording unrolled (read by the pipeline right after `close`).
    static LA_LAST: std::cell::RefCell<Vec<(usize, usize)>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// `loopany` guard: the sites (callee prototype address, loop head pc) the last closed recording unrolled.
pub fn last_la_sites() -> Vec<(usize, usize)> {
    LA_LAST.with(|l| std::mem::take(&mut *l.borrow_mut()))
}

/// `loopany` guard: the site is never unrolled again (its unrolled traces exit too often).
pub fn la_block(site: (usize, usize)) {
    LOOP_ANY.with(|m| {
        m.borrow_mut().insert(site);
    });
}

thread_local! {
    /// `loopany` guard: the blocked sites (callee prototype address, loop head pc).
    static LOOP_ANY: std::cell::RefCell<std::collections::HashSet<(usize, usize)>> = std::cell::RefCell::new(Default::default());
}

thread_local! {
    /// BUNDLE-E (`contok` test knob): prototypes (addresses) in which a recording observed a self-call.
    static CONT_OK: std::cell::RefCell<std::collections::HashSet<usize>> = std::cell::RefCell::new(Default::default());
}

/// BUNDLE-E (`contok`): did a recording observe a self-call of the prototype at `key`?
pub fn cont_ok(key: usize) -> bool {
    CONT_OK.with(|c| c.borrow().contains(&key))
}
