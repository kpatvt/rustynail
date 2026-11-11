//! J5 P5b: the trace assembler (docs/JIT_TRACE.md 5.3, 7.5-7.8; decisions in docs/JIT_TRACE_REF.md "P5b decisions").
//!
//! `TraceIr` + P5a's [`Allocation`] + P4's resolved exits ([`ExitSnap`]) in, a micro-op [`Program`] out. **Safe code
//! only** (micro-ops are data; the x86 backend and the emulator run them). The walk is forward over the IR: for each
//! instruction the assembler emits what `Allocation::ins[i]` says (`pre` reloads, the operation reading `ops` and
//! writing `dst`, `stores`, `post` reloads) and nothing else decides a location.
//!
//! # Layout of the program
//!
//! ```text
//! entry:   TR6a stack room, TR6b CallInfo spares        (failure: [entry_fail bookkeeping], pcl[H])
//!          entry-ok bookkeeping (3.3 free peel)
//!   rolled (7.8, loop_at = None)       peeled (loop_at = Some)
//! TOP:     IR 0..n  (POLL, BUDGET n,       IR 0..LOOP  (pre-roll)
//!          the iteration, guards in        entry moves; LOOP:
//!          place)                          IR LOOP+1..n (PHIs, body)
//!          writeback stores                back-edge moves
//!          Jmp TOP                         Jmp LOOP
//! stubs:   one exit stub per (snapshot, entry-check) pair, after the loop
//! shared:  the shared exit routine (`exit::append_shared_exit`), only if some stub is generic
//! ```
//!
//! # Conventions (all decisions are listed in "P5b decisions" of docs/JIT_TRACE_REF.md)
//!
//! * **The budget lives in memory** (`[r15]`, `Loc::None` for `ExitMeta::vb`): `BUDGET n` is `Ld V7; Br Lt V7, n -> exit;
//!   Sub V7, n; St`. RA has no register reserved for it.
//! * **`V7` is the scratch register.** Everything that needs a second scratch saves a register in the *extension
//!   slots* of the exit area (`spill[nspill + k]`, above everything the allocator uses; at most [`EXT_SLOTS`]) and
//!   restores it before any branch out.
//! * **Guards are evaluated before their destination is written** (P5a convention): a result goes through `V7` when
//!   the micro-op would write the destination early (`F2IExact`).
//! * **Exits.** A frameless, usable snapshot gets an *inline stub* (`exit::inline_stub_ops` split around a counter and
//!   hot-exit compare, 7.6): the counter `ctr[snap]` is incremented unless it already is `hot_exit - 1`, in which case
//!   the exit diverts to the *generic stub* of the same snapshot (so the handler counts it, reports the hot exit and
//!   restores from the saved registers). Snapshots with inlined frames use the generic stub. The generic stub stores
//!   `trace_id`, `exitno` and jumps to the shared routine, which this program carries itself unless
//!   [`AsmEnv::shared_exit`] gives an address (a per-arena copy).
//! * **Entry checks and the free peel** (3.3): a checked `SLOAD` or an `EQREF` whose snapshot has an empty map is an
//!   *entry check*. Its exit stub first books an entry failure (`entry_fail += pending_before; pending = 1;
//!   entry_ok -= 1`, `jit_tr_entry_fail` at the threshold) and then proceeds as any exit. The entry sequence books
//!   `pending_before` into an extension slot, clears `pending` and counts `entry_ok` *before* the checks (so the
//!   checks may sit anywhere, in either layout); TR6a/6b failures book without the undo.
//! * **Nothing is ever moved.** IR order is the order of code. In particular no guard is hoisted into the entry
//!   segment: the "hoisting licence" of the P3b review (entry guards never move in the rolled layout; in the peeled
//!   layout only the pre-roll copy may sit in the entry segment, on snapshot 0, never on `entry_snap`) is satisfied
//!   trivially, and `tests/tr_asm.rs` pins it.

use super::cache::head_off;
use super::exit::{self, tag_of, ExitSnap, SharedExitEnv, ValSrc};
use super::interp::{
    kbits, CALLN_CFMOD, CALLN_COS, CALLN_EXP, CALLN_FLOOR, CALLN_FMOD, CALLN_LOG, CALLN_POW,
    CALLN_SIN,
};
use super::ir::*;

/// N5: clears the short / long variant bit of a string tag byte (`ctb(LUA_VSHRSTR)` = 0x44, `ctb(LUA_VLNGSTR)` = 0x54).
const ANYSTR_MASK: i32 = 0xEF;
use super::mutants;
use super::ra::{Allocation, CallSite, Loc, Move, SCRATCH_SPILL};
use super::{exit_area, NSPILL};
use crate::ir::{Base, Cc, FOp, Mi, Op, Program, Src, L, M, V, W, X};
use rn_core::gc::{BLACKBIT, WHITEBITS};
use rn_core::jit_hooks::thread as thr;
use rn_core::jit_layout::{callinfo as ci, lclosure, node, table, upval};
use rn_core::number::{fmod_lua, pow_lua};
use rn_core::value::*;

/// Extension slots reserved in the exit area above the allocator's slots: `0` = the booked `pending` of the entry
/// sequence, `1` = the `+exitstorm` counter, `2..` = save slots of temporaries (`save_slot`).
pub const EXT_SLOTS: u16 = 9;

/// The extension slot of the "first iteration" flag of a rolled trace with entry-check candidates (see [`EntryKind`]).
const FIRST_SLOT_K: u16 = 8;
/// N2c: the extension slot of the segment oracle (the entry `CallInfo`).
pub const ORACLE_SLOT_K: u16 = 9;

/// The spill slot that counts loop iterations when [`AsmEnv::count_iterations`] is on (one below the cycle scratch).
pub const ITER_SLOT: u16 = SCRATCH_SPILL - 1;

/// The pseudo slot of the running closure (`record::FUNC_SLOT`).
const FUNC_SLOT: u16 = u16::MAX;

/// What the assembler needs from the runtime (all addresses are absolute; the program is otherwise position
/// independent).
#[derive(Clone, Copy, Debug)]
pub struct AsmEnv {
    /// `TrId::slot` of the trace (`ExitArea.trace_id` of generic stubs).
    pub trace_id: u32,
    /// Exit counters (`ExitMeta::counters`): `ir.snaps.len()` `u32` words, or `0` for no counting (and no hot diverts).
    pub counters: u64,
    /// `exit_hot`: the inline stub diverts to the generic stub when its counter would reach this value (`>= 1`).
    pub hot_exit: u32,
    /// Address of a shared exit routine (a per-arena copy), or `0`: the program embeds its own.
    pub shared_exit: u64,
    /// `jit_tr_exit` (for the embedded shared routine).
    pub exit_handler: u64,
    /// Offset of the stack data pointer inside the `Thread` (`ReloadFrame` of the shared routine).
    pub stack_off: i32,
    /// `pcl[H]`: where entry failures continue (tier 1, pre-count label of the head).
    pub pcl_h: u64,
    /// Address of the head's `HeadData` (entry counters, 3.3), or `0`: no entry bookkeeping.
    pub head: u64,
    /// The head's pc (argument of `jit_tr_entry_fail`).
    pub head_pc: u32,
    /// `cache::jit_tr_entry_fail`.
    pub entry_fail_fn: u64,
    /// The pure-helper bracket target (`jit_tr_calln` or a test double).
    pub calln: u64,
    /// The write-barrier helper (`jit_tr_barrier` or a test double).
    pub barrier: u64,
    /// F3: the class-A helper (`jit_tr_alloc` or a test double): `TNEW`, `NEWREF`, `TLEN`.
    pub alloc: u64,
    /// Address of `INTERRUPT` (`jit_hooks::interrupt_flag_addr`).
    pub interrupt: u64,
    /// `+exitstorm=N`: every guard fails every N-th execution (the trace-wide counter lives in an extension slot).
    pub exitstorm: Option<u32>,
    /// Every exit uses the generic stub (tests: exercises the shared routine and the Rust restore for all exits).
    pub generic_only: bool,
    /// Counts loop iterations of every trace in `ExitArea.spill[ITER_SLOT]` (tests: proves that the trace ran).
    pub count_iterations: bool,
    /// PIa: keep the budget `vb` in this general register (`ra::BUDGET_GPR`, which the allocation must have reserved
    /// with `RaOpts::reserve_gpr`) instead of in the budget word. The entry loads the word into it, `BUDGET` is
    /// `Br Lt r, n; Sub r, n`, every exit stores it back (`ExitMeta::vb = Loc::G(r)`, see [`AsmEnv::vb`]). `None`:
    /// the memory form (`ExitMeta::vb = Loc::None`).
    pub budget_reg: Option<u8>,
    /// F2a (TR8): base address of the exit cells (`ExitMeta::cells`, one word per snapshot). Nonzero: the inline stub of
    /// every frameless snapshot continues through its cell (`Mov V7, &cell[k]; Ld V7, [V7]; JmpR V7`) instead of an
    /// embedded tier-1 address; `0`: the embedded address (tests, prototype shape).
    pub exit_cells: u64,
    /// F2a: assemble a **side trace** (7.8): entered with the state of a tier-1 label restored, no head bookkeeping, no
    /// loop; ends with a link to another trace head through its dispatch cell.
    pub side: Option<SideEnv>,
    /// S0 (allocation sinking): address of the virtual-byte counter (`Heap::virtual_cell`); `TVIRT` adds its bytes to it
    /// (P-L: the only cell; the collector-visible total is the real bytes plus this counter). `0`: none (tests without
    /// sinking).
    pub vcell: u64,
    /// P-M3: the continuation heads a native `RETURN1` continues at: (address of the instruction after the `CALL` = the
    /// caller's `savedpc`, address of the head's native-entry cell, register `A` of that `CALL`), `rec_nsites` valid entries.
    pub rec_sites: [(u64, u64, u8); super::cache::MAX_CONT_SITES],
    /// P-M3: number of valid entries of `rec_sites`.
    pub rec_nsites: u8,
    /// P-M3: nonzero: a native self-call continues through this dispatch cell (the function-entry head's) instead of
    /// the loop top of the trace itself (a continuation trace).
    pub rec_link_cell: u64,
    /// P-M3: `maxstacksize` of the root prototype (the inner entry of a native-recursion trace skips the room checks the
    /// running frame already satisfies).
    pub root_maxstack: u16,
    /// S1: a side trace entered with virtual frames: no TR6a/b entry checks (the parent's guarantee covers it, the recorder
    /// refused any trace that needs more) and no entry-failure landing.
    pub no_room_checks: bool,
    /// R2: a frameful side trace that needs more stack / spare `CallInfo` nodes than its parent guarantees checks them itself at
    /// its start (failure: an ordinary exit through the start snapshot of its first budget charge) instead of the `side-room` abort.
    pub side_room: bool,
    /// S1: the exits whose snapshot has virtual-able frames ([`ExitSnap::frame_side_ok`]) have an exit cell that starts at 0
    /// and get an inline stub that continues through the cell once a side trace is attached (0: the generic twin, which
    /// restores the frames). Set by the pipeline for the traces whose cells it initialised that way.
    pub frame_cells: bool,
    /// P3 (`chainre`): the exits whose snapshot is used by a callee identity guard (`EQREF`) divert to the handler every [`exit::CHAIN_EXITS`]-th exit while a side trace is
    /// attached (the chain re-order check, `cache::chain_check`).
    pub chain: bool,
    /// H2: the root prototype's `code` pointer (`ExitMeta::root_code`): nonzero lets the inline stub of a frameful exit rebuild
    /// the frames itself (`exit::frame_mat_ops`) while its cell is 0 instead of calling the handler. `0`: always the handler.
    pub root_code: u64,
    /// S1: the snapshot of a linking exit (never has a frame cell), `u32::MAX` for none.
    pub frame_link_snap: u32,
    /// A1: `setmetatable(t, mt)` (`LIB` kind `SETMT`) stores the metatable inline when nothing needs the helper (see `Asm::setmt_fast`).
    pub setmt_inline: bool,
    /// B2: scaled array addressing (`b2arr`).
    pub b2_arr: bool,
    /// B2: skip the node tag store when the tag is known (`b2tag`).
    pub b2_tag: bool,
    /// B2: `HREFK` with displacements and a `lea` (`b2hk`).
    pub b2_hk: bool,
    /// B4-1: stores of nil / false / true write the tag byte only (`nopay`).
    pub nopay: bool,
    /// H3-F (`hk64`): `HREFK` compares the key address with one 64-bit compare (`cmp [m], r`) instead of two 32-bit halves.
    pub hk_key64: bool,
    /// B4-3: an `HREFK` followed by the `HLOAD` of its node checks the key tag and the value tag with one 16-bit compare (`hkfuse`).
    pub hk_fuse: bool,
    /// B4-4: `//` and `%` by a constant use multiplication (`divmagic`).
    pub divmagic: bool,
    /// F2: `string.byte(s, i)` with `1 <= i <= #s` is inline loads (the helper call is the slow path) (`f2byte`).
    pub f2_byte: bool,
    /// BUNDLE-D (`ccalls`): the C-stack check of `LIB CCALLS` is inline (the helper is the slow path).
    pub ccalls_inline: bool,
    /// BUNDLE-P P2 (`f2ifl`): `F2I_EXACT` of an `FFLOOR` result is `cvttsd2si; cmp r, 1; jo` ([`Op::F2IFloor`]).
    pub f2ifl: bool,
    /// BUNDLE-F (`fibdiet`): the pieces of the slimmer native-recursion protocol (bit mask).
    pub fibdiet: u32,
    /// BUNDLE-K L1 (`lazyci`): the root prototype is lazy-aware: natively pushed frames are lazy (`ExitArea::lazy_root`), a native return of a
    /// lazy frame has no `CIST_*` guards, and every path out of the trace completes the chain first (docs/JIT_TRACE_REF.md "BUNDLE-K").
    pub lazy: bool,
    /// N2c (`+verify`, the segment oracle): the trace stores its entry `CallInfo` to an extension slot at the entry, and its exits after a `RETF`
    /// go through the Rust handler, which checks that `r13` is that `CallInfo` popped `k` times (`ExitMeta::seg_oracle`).
    pub seg_oracle: bool,
    /// N3b (`TrLimits::retf` bit 1): the frame-linking exit of a side trace rebuilds its frames in an inline stub and jumps through the head's cell.
    pub link_mat: bool,
    /// BUNDLE-K L1: the side trace is a prelude (entered from the head's dispatch cell with a complete frame, like a root trace).
    pub prelude: bool,
    /// BUNDLE-K L1: `exit::jit_tr_lazy_leave` (or a test double).
    pub lazy_leave_fn: u64,
    /// BUNDLE-K L1: `callstatus` (`C` operand) shared by every `CALL` of the root prototype: a lazy frame is then completed by an in-line loop; `None`: by the helper.
    pub lazy_cs: Option<u32>,
    /// BUNDLE-K L1: `numparams` of the root prototype (the end of the arguments of a frame just pushed natively).
    pub lazy_nparams: u16,
    /// HDRSUB: `LIB sub` calls the lean entry [`jit_tr_lib_sub`] (`libsub`).
    pub lean_sub: bool,
    /// BUNDLE-H (`libcall`): bit 1 = lean direct entries of the `LIB` helpers, bit 2 = inline `type` / string compares / `random` (`TrLimits::libcall`).
    pub libcall: u32,
    /// J1-2 (`sinkstub`): frameless exits that materialise sunk tables are restored by their inline stub (slot stores, then a `CallH` of
    /// `mat_fn`, then the jump through the exit cell) instead of the generic stub and the exit handler.
    pub sink_stub: bool,
    /// J1-2: `exit::jit_tr_mat` (or a test double).
    pub mat_fn: u64,
    /// P2-3a (`budsub`): with `budget_reg`, `BUDGET` is `sub r, n; jl stub` and the cold stub refunds `n` (`add r, n`) before it reaches the exit
    /// (2 instructions in the loop instead of `cmp; jl; sub`).
    pub budget_sub: bool,
    /// P2-3b (`pollr15`): displacement of the poll word from `r15` (the poll word is within +-2 GiB of the per-state data): the poll is
    /// `cmp byte [r15 + disp], 0; jne` (no address in a register). `None`: the address is built in `V7`.
    pub poll_off: Option<i32>,
    /// P2-3c (`fpool`): address of the per-state float-constant pool ([`super::FPool`], inside the area behind `r15`), or `0`: constants
    /// are built in registers (`movabs; movq`).
    pub fpool: u64,
    /// P2-3c: displacement from `r15` of `FPool::bits[0]`.
    pub fpool_off: i32,
}

/// The link at the end of a side trace (7.8, TR22).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SideEnv {
    /// Address of `cell[H]` of the head the trace ends at (the trace entry of its root trace while it exists, tier 1's
    /// counting stub or `pcl[H]` otherwise: whatever tier 1's own back-edge would reach).
    pub link_cell: u64,
    /// Snapshot of the poll before the link (the empty map at `H`, uncharged).
    pub link_snap: u32,
}

impl AsmEnv {
    /// Where the budget is at an exit: the `ExitMeta::vb` of a trace assembled with this environment.
    pub fn vb(&self) -> Loc {
        match self.budget_reg {
            Some(r) => Loc::G(r),
            None => VB,
        }
    }
}

impl Default for AsmEnv {
    fn default() -> AsmEnv {
        AsmEnv {
            trace_id: 0,
            counters: 0,
            hot_exit: 16,
            shared_exit: 0,
            exit_handler: 0,
            stack_off: 0,
            pcl_h: 0,
            head: 0,
            head_pc: 0,
            entry_fail_fn: 0,
            calln: 0,
            barrier: 0,
            alloc: 0,
            interrupt: 0,
            exitstorm: None,
            generic_only: false,
            count_iterations: false,
            budget_reg: None,
            exit_cells: 0,
            side: None,
            vcell: 0,
            rec_sites: [(0, 0, 0); super::cache::MAX_CONT_SITES],
            rec_nsites: 0,
            rec_link_cell: 0,
            root_maxstack: 0,
            no_room_checks: false,
            side_room: false,
            frame_cells: false,
            chain: false,
            root_code: 0,
            frame_link_snap: u32::MAX,
            setmt_inline: false,
            b2_arr: false,
            b2_tag: false,
            b2_hk: false,
            nopay: false,
            divmagic: false,
            hk_fuse: false,
            hk_key64: false,
            f2_byte: false,
            ccalls_inline: false,
            f2ifl: false,
            fibdiet: 0,
            lazy: false,
            seg_oracle: false,
            link_mat: false,
            prelude: false,
            lazy_leave_fn: 0,
            lazy_cs: None,
            lazy_nparams: 0,
            lean_sub: false,
            libcall: 0,
            sink_stub: false,
            mat_fn: 0,
            budget_sub: false,
            poll_off: None,
            fpool: 0,
            fpool_off: 0,
        }
    }
}

/// Why assembly failed (nothing is installed; the recording aborts with `Compile(reason)`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AsmError {
    /// The IR uses something this assembler does not lower (message).
    Unsupported(String),
    /// The allocation or the exit table does not match the IR (message).
    Inconsistent(String),
    /// Too many spill slots for the extension slots.
    Spill,
}

fn unsup<T>(s: impl Into<String>) -> Result<T, AsmError> {
    Err(AsmError::Unsupported(s.into()))
}

/// `(tag byte offset, payload offset)` of element `u` (a constant index, 0-based) from the array base: `Unsupported`
/// when an offset does not fit the 32-bit displacement (`u >= 2^28 - 1`) instead of overflowing.
fn const_index_offsets(u: u64) -> Result<(i32, i32), AsmError> {
    let off = |u: u64| -> Option<(i32, i32)> {
        let u = i64::try_from(u).ok()?;
        let t = i32::try_from(i64::from(table::ARRAY_TAG_BASE as i32) + u).ok()?;
        let p = i32::try_from(-8i64.checked_mul(u.checked_add(1)?)?).ok()?;
        Some((t, p))
    };
    off(u).map_or_else(
        || unsup("constant array index too large for a displacement"),
        Ok,
    )
}

fn incons<T>(s: impl Into<String>) -> Result<T, AsmError> {
    Err(AsmError::Inconsistent(s.into()))
}

/// The assembled trace.
#[derive(Debug)]
pub struct Asm {
    /// The micro-ops.
    pub prog: Program,
    /// The trace entry.
    pub entry: L,
    /// Where the loop (rolled: `TOP`, peeled: `LOOP`) is.
    pub loop_label: L,
    /// Number of exit stubs emitted (inline + generic).
    pub stubs: usize,
    /// J1-1: number of `HREFK`+`HLOAD` pairs whose tag checks were fused (`hkfuse`).
    pub hk_fused: u32,
    /// J1-1: how many of them have two exits (the type guard has its own snapshot).
    pub hk_split: u32,
    /// P3: snapshots whose exit counter counts twice per exit (the `mat_stub` of a frameful exit with a frame cell: see there).
    pub dbl: Vec<u32>,
    /// P-M3: the inner entry of a native-recursion root (room checks only; the entry bookkeeping and the budget load are
    /// skipped: a native return / self-call has stored the budget word and keeps the budget register), `None` if the trace
    /// has none.
    pub inner: Option<L>,
}

/// J1-1: the cold disambiguator of a fused `HREFK`+`HLOAD` pair with two exits.
#[derive(Clone, Copy)]
struct HkCold {
    label: L,
    /// the node register and the displacement of the key tag byte
    m: M,
    hrefk_exit: L,
    hload_exit: L,
}

/// Where the budget is at an exit in the memory form: the budget word (`ExitMeta::vb`); see [`AsmEnv::vb`].
pub const VB: Loc = Loc::None;

// ---- helper entry points ------------------------------------------------------------------------------------------

/// The pure helpers of `Calln` (`aux`: 0 `pow_lua`, 1 `fmod_lua`, 2 `floor`) on bit patterns. The result is returned
/// **bit for bit** (NaN sign and payload included): the VM's `pow_lua` / `fmod_lua` pass them through and `fmt_g`
/// prints the sign (`nan` / `-nan`), so canonicalising would change the output.
pub fn calln_core(which: u32, x: u64, y: u64) -> u64 {
    let (x, y) = (f64::from_bits(x), f64::from_bits(y));
    match which {
        CALLN_POW => pow_lua(x, y),
        CALLN_FMOD => fmod_lua(x, y),
        CALLN_FLOOR => x.floor(),
        CALLN_CFMOD => x % y,
        CALLN_SIN => {
            if mutants::active("f1_sin_cos_swap") {
                x.cos()
            } else {
                x.sin()
            }
        }
        CALLN_COS => x.cos(),
        CALLN_EXP => x.exp(),
        CALLN_LOG => x.ln(),
        _ => f64::NAN,
    }
    .to_bits()
}

/// `jit_tr_calln(thread, ci, which, 0)`: runs a pure helper on `ExitArea.args[0..2]` (bit patterns) and leaves the
/// result in `args[0]`. Needs no allocation and cannot run Lua code (TR1, class N).
///
/// # Safety
/// Called only from trace code with the running thread; the exit area follows the budget word of its state.
pub unsafe extern "C" fn jit_tr_calln(
    th: *mut rn_core::state::Thread,
    _ci: *mut rn_core::state::CallInfo,
    which: u32,
    _b: u32,
) -> u32 {
    // SAFETY: `th` is the live thread pinned in `r12`.
    let mut st = unsafe { rn_core::jit_hooks::state_from_thread(th.cast()) };
    let budget = crate::rt::cnt_ptr(&mut st);
    // SAFETY: the area follows the budget word inside the per-state data (P4b), with provenance over it.
    let area = unsafe {
        &mut *budget
            .cast::<u8>()
            .add(exit_area::FROM_R15 as usize)
            .cast::<super::ExitArea>()
    };
    area.args[0] = calln_core(which, area.args[0], area.args[1]);
    0
}

/// Calls of the barrier helper by kind (Y5 coverage counters: `[0]` the back barrier of a table store, `[1]` the forward barrier of an upvalue
/// store (`b` = 1): the inline test of the trace calls the helper only when the container is black and the value white, so a nonzero count proves
/// that a real run reached the slow path). Process-wide; printed by the `[trace]` summary.
pub static BARRIER_CALLS: [std::sync::atomic::AtomicU64; 2] = [
    std::sync::atomic::AtomicU64::new(0),
    std::sync::atomic::AtomicU64::new(0),
];

/// `jit_tr_barrier(thread, ci, tag, 0)`: `barrier_back(table = args[0], value = (tag, args[1]))` (the inline test of
/// the trace has already established "table black and value white", the call is the slow path of 7.5). Class S, no
/// allocation, no Lua code.
///
/// # Safety
/// As [`jit_tr_calln`]; `args[0]` is a live table of this state and `(tag, args[1])` a live collectable value.
pub unsafe extern "C" fn jit_tr_barrier(
    th: *mut rn_core::state::Thread,
    _ci: *mut rn_core::state::CallInfo,
    tag: u32,
    b: u32,
) -> u32 {
    // SAFETY: `th` is the live thread pinned in `r12`.
    let mut st = unsafe { rn_core::jit_hooks::state_from_thread(th.cast()) };
    let budget = crate::rt::cnt_ptr(&mut st);
    // SAFETY: as in `jit_tr_calln`.
    let area = unsafe {
        &*budget
            .cast::<u8>()
            .add(exit_area::FROM_R15 as usize)
            .cast::<super::ExitArea>()
    };
    let (t, payload) = (area.args[0], area.args[1]);
    BARRIER_CALLS[(b & 1) as usize].fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    if b & 1 != 0 {
        // BUNDLE-V V2: the barrier of an upvalue (`args[0]` = the `UpVal`): `luaC_barrier(uv, v)`, the forward barrier of `OP_SETUPVAL`
        // SAFETY: as below.
        let v: rn_core::value::Value = unsafe {
            std::mem::transmute::<[u64; 2], rn_core::value::Value>([u64::from(tag & 0xff), payload])
        };
        st.heap().barrier(t as *mut rn_core::gc::GcHeader, &v);
        return 0;
    }
    // SAFETY: `Value` is `repr(C)` `{ tag word, payload }` (jit_layout::value); the pair is a value that was stored to
    // the table's array part by the trace (a collectable value of a live object).
    let v: rn_core::value::Value = unsafe {
        std::mem::transmute::<[u64; 2], rn_core::value::Value>([u64::from(tag & 0xff), payload])
    };
    st.heap().barrier_back(t as *mut rn_core::gc::GcHeader, &v);
    0
}

/// `jit_tr_alloc` operations (the low byte of the first immediate; the rest of it and the second immediate are
/// per-operation arguments).
pub const ALLOC_TNEW: u32 = 0;
/// `NEWREF`: `args[0]` = table, `args[1]` = address of the interned short-string key.
pub const ALLOC_NEWREF: u32 = 1;
/// `TLEN`: `args[0]` = table (second immediate 0) or string (1).
pub const ALLOC_TLEN: u32 = 2;
/// `TSET`: `args[0]` = table, `args[1]` = integer key, `args[2]` = payload of the value whose tag is `a >> 8`.
pub const ALLOC_TSET: u32 = 3;
/// `TNEXT`: `args[0]` = table, `args[1]` = payload of the control value whose tag is `a >> 8`; the result is the address of
/// the pseudo node `args[1..4]` (node layout: value payload, value tag byte, key tag byte, key payload) or 0.
pub const ALLOC_TNEXT: u32 = 4;
/// `SNEW`: `args[0]` = the payload of the number to convert; `a >> 8` = 0 for an integer, 1 for a float.
pub const ALLOC_SNEW: u32 = 5;
/// `CONCAT`: `args[0..n]` = the string operands (addresses); `a >> 8` bits 0..3 = operand `k` is a long string, bit 3 = three
/// operands, bit 4 = the result must be a long string.
pub const ALLOC_CONCAT: u32 = 6;
/// `STRTO`: `args[0]` = the string (address); `a >> 8` bit 0 = long string, bit 1 = round up (ceil) instead of down; the result
/// is the pseudo node `args[1..4]` with the integer in its value fields, or 0. BUNDLE-A (`libnum`): `a >> 8` bit 2 = tonumber mode: the
/// string is converted like `tonumber(s)` (`number::str2num`: integer or float, any numeral syntax), the value field holds the number with
/// its tag byte (integer or float); no numeral gives 0. Nothing is allocated and no table is touched.
pub const ALLOC_STRTO: u32 = 7;
/// P-B `LIB`: `a >> 8` low byte = the [`lib`] kind, then flag bits (bit 8: operand 0 is a long string, bit 9: the result is expected to be
/// a long string, bit 10: the number operand is a float); `args[0..3]` = the operands (see [`lib_core`]).
pub const ALLOC_LIB: u32 = 8;
/// B3-4: `CONCAT` with number operands (the `bits` of [`ALLOC_CONCAT`] plus 5..8 = operand `k` is an integer, 8..11 = a float;
/// `args[k]` is then the number's payload, not an address): the helper formats the numbers itself.
pub const ALLOC_CONCATN: u32 = 9;

/// Argument `k` of a class-A helper. The generated code stores the arguments with separate 8-byte stores: a compiler-merged 16-byte load of
/// two neighbours (`[args[0], args[1], ..]` copied as a unit) cannot be forwarded from them and stalls for the length of a store-forwarding
/// failure on every call (F1), so each word is read on its own.
#[inline(always)]
fn arg(area: &super::ExitArea, k: usize) -> u64 {
    // SAFETY: a plain read of an initialised word of the exit area.
    unsafe { std::ptr::read_volatile(&area.args[k]) }
}

/// `ALLOC_TNEXT` of [`jit_tr_alloc`] on its own (F1): the traversal step is the hottest class-A helper, so it has its own lean frame
/// (the big `match` of `jit_tr_alloc` makes a large frame and saves every callee-saved register on each call).
///
/// # Safety
/// As [`jit_tr_alloc`] with `ALLOC_TNEXT`; `x` = the control tag.
#[inline(never)]
unsafe fn tnext_entry(th: *mut rn_core::state::Thread, x: u32) {
    // SAFETY: `th` is the live thread pinned in `r12`.
    let mut st = unsafe { rn_core::jit_hooks::state_from_thread(th.cast()) };
    let budget = crate::rt::cnt_ptr(&mut st);
    // SAFETY: as in `jit_tr_calln`.
    let area = unsafe {
        &mut *budget
            .cast::<u8>()
            .add(exit_area::FROM_R15 as usize)
            .cast::<super::ExitArea>()
    };
    // SAFETY: a live table; `Value` is `repr(C)` `{ tag word, payload }`, the pair is a value the trace holds (a collectable
    // payload is a live object, possibly a dead key of the table).
    let (t, ctl) = unsafe {
        (
            rn_core::table::TableRef::from_raw(arg(area, 0) as *mut rn_core::table::Table),
            std::mem::transmute::<[u64; 2], rn_core::value::Value>([
                u64::from(x as u8),
                arg(area, 1),
            ]),
        )
    };
    let mut hint = st.heap().next_hint;
    let r = t.next_hinted(&ctl, &mut hint);
    st.heap().next_hint = hint;
    area.args[0] = match r {
        Ok(r) => {
            let (k, v) = r.unwrap_or((rn_core::value::Value::NIL, rn_core::value::Value::NIL));
            // SAFETY: `Value` is a plain 16-byte `repr(C)` pair of words.
            let (kw, vw) = unsafe {
                (
                    std::mem::transmute::<rn_core::value::Value, [u64; 2]>(k),
                    std::mem::transmute::<rn_core::value::Value, [u64; 2]>(v),
                )
            };
            area.args[1] = vw[1];
            area.args[2] = (vw[0] & 0xFF) | (kw[0] & 0xFF) << 8;
            area.args[3] = kw[1];
            std::ptr::addr_of!(area.args[1]) as u64
        }
        Err(_) => 0,
    };
}

/// `jit_tr_alloc` for `ALLOC_TNEXT` only (F1: the traversal step is the hottest class-A helper, and the general function's frame costs
/// more than the step itself); the generated code calls this entry for `TNEXT` when the class-A helper is the real one.
///
/// # Safety
/// As [`jit_tr_alloc`] with `a = ALLOC_TNEXT | tag << 8`.
pub unsafe extern "C" fn jit_tr_tnext(
    th: *mut rn_core::state::Thread,
    _ci: *mut rn_core::state::CallInfo,
    a: u32,
    _y: u32,
) -> u32 {
    // SAFETY: the contract above.
    unsafe { tnext_entry(th, a >> 8) };
    0
}

/// Lean entries of `jit_tr_alloc` for the table-insertion helpers (P-TINS6, same idea as [`jit_tr_tnext`]): the general function's
/// frame (six saved registers, 0x88 bytes of stack, a jump table) cost more than a `TLEN`/`TSET` step itself. Generated code calls
/// them when the class-A helper is the real one; the work is the matching arm of `jit_tr_alloc`, unchanged.
///
/// # Safety
/// As [`jit_tr_alloc`] with `a = ALLOC_TLEN` (`y == 0`: `args[0]` a live table).
pub unsafe extern "C" fn jit_tr_tlen(
    th: *mut rn_core::state::Thread,
    _ci: *mut rn_core::state::CallInfo,
    _a: u32,
    _y: u32,
) -> u32 {
    // SAFETY: `th` is the live thread pinned in `r12`.
    let mut st = unsafe { rn_core::jit_hooks::state_from_thread(th.cast()) };
    let budget = crate::rt::cnt_ptr(&mut st);
    // SAFETY: as in `jit_tr_calln`.
    let area = unsafe {
        &mut *budget
            .cast::<u8>()
            .add(exit_area::FROM_R15 as usize)
            .cast::<super::ExitArea>()
    };
    // SAFETY: the contract above.
    let t =
        unsafe { rn_core::table::TableRef::from_raw(area.args[0] as *mut rn_core::table::Table) };
    area.args[0] = t.len(st.heap());
    0
}

/// HDRSUB: lean entry of `jit_tr_alloc` for `LIB sub` (`a = ALLOC_LIB | (SUB | flags << 8) << 8`, `args` = string, i, j): the arm
/// of [`lib_core`] without the dispatch frame. No collector step, no table is touched.
///
/// # Safety
/// As [`jit_tr_alloc`] with `ALLOC_LIB`, kind `SUB`: `args[0]` is a live string of the kind flag bit 0 says.
pub unsafe extern "C" fn jit_tr_lib_sub(
    th: *mut rn_core::state::Thread,
    _ci: *mut rn_core::state::CallInfo,
    a: u32,
    _y: u32,
) -> u32 {
    // SAFETY: `th` is the live thread pinned in `r12`.
    let mut st = unsafe { rn_core::jit_hooks::state_from_thread(th.cast()) };
    let budget = crate::rt::cnt_ptr(&mut st);
    // SAFETY: as in `jit_tr_calln`.
    let area = unsafe {
        &mut *budget
            .cast::<u8>()
            .add(exit_area::FROM_R15 as usize)
            .cast::<super::ExitArea>()
    };
    let fl = (a >> 8) >> 8;
    let tag = if fl & 1 != 0 {
        rn_core::value::ctb(rn_core::value::LUA_VLNGSTR)
    } else {
        rn_core::value::ctb(rn_core::value::LUA_VSHRSTR)
    };
    // SAFETY: the contract above; `Value` is `repr(C)` `{ tag word, payload }`.
    let v: rn_core::value::Value = unsafe {
        std::mem::transmute::<[u64; 2], rn_core::value::Value>([u64::from(tag), area.args[0]])
    };
    // MUTANT `libsub_lean_args_swapped`: the bounds are passed the wrong way round
    let args = if mutants::active("libsub_lean_args_swapped") {
        [area.args[0], area.args[2], area.args[1]]
    } else {
        [area.args[0], area.args[1], area.args[2]]
    };
    area.args[0] = match v.as_strref() {
        Some(s) => sub_piece(&mut st, s.get().as_bytes(), fl, args),
        None => 0,
    };
    0
}

/// # Safety
/// As [`jit_tr_alloc`] with `a = ALLOC_TSET | tag << 8`.
pub unsafe extern "C" fn jit_tr_tset(
    th: *mut rn_core::state::Thread,
    _ci: *mut rn_core::state::CallInfo,
    a: u32,
    _y: u32,
) -> u32 {
    // SAFETY: `th` is the live thread pinned in `r12`.
    let mut st = unsafe { rn_core::jit_hooks::state_from_thread(th.cast()) };
    let budget = crate::rt::cnt_ptr(&mut st);
    // SAFETY: as in `jit_tr_calln`.
    let area = unsafe {
        &mut *budget
            .cast::<u8>()
            .add(exit_area::FROM_R15 as usize)
            .cast::<super::ExitArea>()
    };
    // SAFETY: as the `ALLOC_TSET` arm of `jit_tr_alloc`.
    let (t, v) = unsafe {
        (
            rn_core::table::TableRef::from_raw(area.args[0] as *mut rn_core::table::Table),
            std::mem::transmute::<[u64; 2], rn_core::value::Value>([
                u64::from((a >> 8) as u8),
                area.args[2],
            ]),
        )
    };
    area.args[0] = u64::from(t.set_int(st.heap(), area.args[1] as i64, &v).is_ok());
    0
}

/// # Safety
/// As [`jit_tr_alloc`] with `a = ALLOC_NEWREF`.
pub unsafe extern "C" fn jit_tr_newref(
    th: *mut rn_core::state::Thread,
    _ci: *mut rn_core::state::CallInfo,
    _a: u32,
    _y: u32,
) -> u32 {
    // SAFETY: `th` is the live thread pinned in `r12`.
    let mut st = unsafe { rn_core::jit_hooks::state_from_thread(th.cast()) };
    let budget = crate::rt::cnt_ptr(&mut st);
    // SAFETY: as in `jit_tr_calln`.
    let area = unsafe {
        &mut *budget
            .cast::<u8>()
            .add(exit_area::FROM_R15 as usize)
            .cast::<super::ExitArea>()
    };
    // SAFETY: the contract above: live table, live interned short string.
    area.args[0] = unsafe { newref_core(&mut st, arg(area, 0), arg(area, 1)) };
    0
}

/// BUNDLE-A (`libnum`): the conversion of `STRTO` in tonumber mode: what `tonumber(s)` of the string value `v` returns as `(payload, tag byte)`
/// (an integer or a float numeral), `None` for a non-numeral or another type. Nothing is allocated.
pub fn strto_tonumber(v: &rn_core::value::Value) -> Option<(u64, u8)> {
    match rn_core::number::str2num(v.as_strref()?.get().as_bytes())? {
        rn_core::number::Number::Int(i) => Some((i as u64, rn_core::value::LUA_VNUMINT)),
        rn_core::number::Number::Float(f) => Some((f.to_bits(), rn_core::value::LUA_VNUMFLT)),
    }
}

/// `jit_tr_alloc(thread, ci, which | x << 8, y)`: the class-A helpers of TR1 (docs/JIT_TRACE.md 6.9). They allocate
/// **without** a GC step and decide first whether the interpreter's `check_gc` right after the same allocation would
/// step (or the allocation could fail): then they do **nothing** and report it (`args[0] = 0`), the trace exits before
/// the instruction and tier 1 allocates, steps or raises exactly like the interpreter. No Lua code runs, nothing can
/// move the stack.
///
/// * `ALLOC_TNEW` (`x` = the `NEWTABLE` operand `B`, `y` = `C`): `args[0]` = the new table, or 0.
/// * `ALLOC_NEWREF`: inserts the short-string key `args[1]` (absent from the table) with the placeholder `true` through
///   the table's own `set` (rehash, barrier on the key, metamethod-cache invalidation) and returns the address of the
///   node in `args[0]`; 0 (nothing done) if the key is present (also with a nil value) or the insertion failed (memory
///   limit: the table is left intact, as `luaH_resize` guarantees).
/// * `ALLOC_TLEN`: `args[0]` = the border of the table (or the length of the string), `luaH_getn` exactly.
///
/// # Safety
/// As [`jit_tr_calln`]; `args[0]` is a live table (`ALLOC_NEWREF`, `ALLOC_TLEN` with `y == 0`) or string (`y == 1`) of
/// this state, `args[1]` a live interned short string.
pub unsafe extern "C" fn jit_tr_alloc(
    th: *mut rn_core::state::Thread,
    _ci: *mut rn_core::state::CallInfo,
    a: u32,
    y: u32,
) -> u32 {
    // SAFETY: `th` is the live thread pinned in `r12`.
    let mut st = unsafe { rn_core::jit_hooks::state_from_thread(th.cast()) };
    let budget = crate::rt::cnt_ptr(&mut st);
    // SAFETY: as in `jit_tr_calln`.
    let area = unsafe {
        &mut *budget
            .cast::<u8>()
            .add(exit_area::FROM_R15 as usize)
            .cast::<super::ExitArea>()
    };
    let x = a >> 8;
    area.args[0] = match a & 0xFF {
        ALLOC_TNEW => tnew_core(&mut st, x, y),
        ALLOC_NEWREF => {
            // SAFETY: the contract above: live table, live interned short string.
            unsafe { newref_core(&mut st, arg(area, 0), arg(area, 1)) }
        }
        ALLOC_TSET => {
            // SAFETY: the contract above: a live table; `Value` is `repr(C)` `{ tag word, payload }` and the pair is a
            // value the trace holds (a collectable payload is a live object).
            let (t, v) = unsafe {
                (
                    rn_core::table::TableRef::from_raw(area.args[0] as *mut rn_core::table::Table),
                    std::mem::transmute::<[u64; 2], rn_core::value::Value>([
                        u64::from(x as u8),
                        area.args[2],
                    ]),
                )
            };
            u64::from(t.set_int(st.heap(), area.args[1] as i64, &v).is_ok())
        }
        ALLOC_TNEXT => {
            // (generated code calls `jit_tr_tnext` directly; this is the route of test doubles that only know `alloc`)
            // SAFETY: the contract above.
            unsafe { tnext_entry(th, x) };
            arg(area, 0)
        }
        ALLOC_STRTO => {
            let tag = if x & 1 != 0 {
                rn_core::value::ctb(rn_core::value::LUA_VLNGSTR)
            } else {
                rn_core::value::ctb(rn_core::value::LUA_VSHRSTR)
            };
            // SAFETY: the contract above: a live string of the kind `x` says; `Value` is `repr(C)` `{ tag word, payload }`.
            let v: rn_core::value::Value = unsafe {
                std::mem::transmute::<[u64; 2], rn_core::value::Value>([
                    u64::from(tag),
                    area.args[0],
                ])
            };
            let mode = if x & 2 != 0 {
                rn_core::number::F2I::Ceil
            } else {
                rn_core::number::F2I::Floor
            };
            if x & 4 != 0 && !mutants::active("tonum_mode_ignored") {
                // BUNDLE-A: tonumber mode (`base_tonumber` of a string argument)
                match strto_tonumber(&v) {
                    Some((bits, tg)) => {
                        area.args[1] = bits;
                        area.args[2] = u64::from(tg);
                        area.args[3] = 0;
                        std::ptr::addr_of!(area.args[1]) as u64
                    }
                    None => 0,
                }
            } else {
                match rn_core::arith::to_integer(&v, mode) {
                    Some(i) => {
                        area.args[1] = i as u64;
                        area.args[2] = u64::from(rn_core::value::LUA_VNUMINT);
                        area.args[3] = 0;
                        std::ptr::addr_of!(area.args[1]) as u64
                    }
                    None => 0,
                }
            }
        }
        ALLOC_LIB => {
            // SAFETY: the contract above: the operands are live objects of the types the kind says.
            unsafe { lib_core(&mut st, x, [arg(area, 0), arg(area, 1), arg(area, 2)]) }
        }
        ALLOC_SNEW => {
            let n = if x & 1 != 0 {
                rn_core::number::Number::Float(f64::from_bits(area.args[0]))
            } else {
                rn_core::number::Number::Int(area.args[0] as i64)
            };
            snew_core(&mut st, n)
        }
        ALLOC_CONCATN => {
            // SAFETY: the contract above: string operands are live strings of the kinds `x` says, the others numbers.
            unsafe { concat_core_num(&mut st, x, [arg(area, 0), arg(area, 1), arg(area, 2)]) }
        }
        ALLOC_CONCAT => {
            // SAFETY: the contract above: the operands are live strings of the kinds `x` says.
            unsafe { concat_core(&mut st, x, [arg(area, 0), arg(area, 1), arg(area, 2)]) }
        }
        ALLOC_TLEN => {
            if y == 0 {
                // SAFETY: the contract above.
                let t = unsafe {
                    rn_core::table::TableRef::from_raw(area.args[0] as *mut rn_core::table::Table)
                };
                t.len(st.heap())
            } else {
                // SAFETY: the contract above: a live string object; `Value` is `repr(C)` `{ tag word, payload }`.
                let v: rn_core::value::Value = unsafe {
                    std::mem::transmute::<[u64; 2], rn_core::value::Value>([
                        u64::from(if y == 1 {
                            rn_core::value::ctb(rn_core::value::LUA_VSHRSTR)
                        } else {
                            rn_core::value::ctb(rn_core::value::LUA_VLNGSTR)
                        }),
                        area.args[0],
                    ])
                };
                v.as_strref().map_or(0, |s| s.get().len() as u64)
            }
        }
        _ => 0,
    };
    if area.args[0] == 0
        && matches!(
            a & 0xFF,
            ALLOC_TNEW | ALLOC_SNEW | ALLOC_CONCAT | ALLOC_CONCATN | ALLOC_LIB
        )
    {
        // N3b: the trace is about to leave at this allocation (a collector step is due, or the call needs the interpreter)
        rn_core::jit_hooks::note_alloc_refusal();
    }
    0
}

/// `setmetatable(t, mt)` of the `SETMT` library kind: 1 when done, 0 (nothing changed) when the interpreter must run the call.
///
/// # Safety
/// `t` is a live table, `mt` a live table or 0.
unsafe fn setmt_core(st: &mut rn_core::state::State, t: u64, mt: u64) -> u64 {
    use rn_core::table::{Table, TableRef};
    use rn_core::value::Value;
    // SAFETY: the contract above.
    let t = unsafe { TableRef::from_raw(t as *mut Table) };
    // SAFETY: the contract above.
    let mt = (mt != 0).then(|| unsafe { TableRef::from_raw(mt as *mut Table) });
    // `base_setmetatable`: a protected old metatable is an error (the interpreter raises it)
    if let Some(old) = t.metatable() {
        if setmt_protected(st.heap(), old) && !mutants::active("setmt_protect_skip") {
            return 0;
        }
    }
    // `luaC_checkfinalizer` would move the table to `finobj`: the interpreter does that
    if mt.is_some_and(|m| st.heap().fast_tm(Some(m), rn_core::meta::Tm::Gc).is_some())
        && !mutants::active("setmt_gc_skip")
    {
        return 0;
    }
    // (the barrier, the store and `check_finalizer`, which has nothing to do for a metatable without `__gc`)
    if mutants::active("setmt_barrier_skip") {
        t.set_metatable(mt);
    } else {
        st.set_metatable(&Value::table(t), mt);
    }
    1
}

/// `NEWTABLE B C` without the GC step: 0 when a step would be due afterwards or the allocation could fail.
pub fn tnew_core(st: &mut rn_core::state::State, b: u32, c: u32) -> u64 {
    let hsize = if b > 0 { 1u32 << (b - 1) } else { 0 };
    // MUTANT `tnew_bytes_small`: the node vector is left out of the bound
    let counted_hsize = if mutants::active("tnew_bytes_small") {
        0
    } else {
        hsize
    };
    let Some(bytes) = rn_core::table::new_table_bytes(c, counted_hsize) else {
        return 0;
    };
    let heap = st.heap();
    // MUTANT `tnew_no_gc_check`: allocate even when the collector would step right after the allocation
    if !mutants::active("tnew_no_gc_check") && !heap.jit_alloc_ok(bytes) {
        return 0;
    }
    let Ok(t) = heap.new_table() else {
        return 0;
    };
    if (b != 0 || c != 0) && t.resize(heap, c, hsize).is_err() {
        return 0; // the empty table stays behind as garbage
    }
    t.as_ptr() as u64
}

/// `SNEW` without the GC step: the string `luaO_tostring` makes of `n`, or 0 when a step would be due after the allocation
/// (or it could fail, or it would grow the string table).
pub fn snew_core(st: &mut rn_core::state::State, n: rn_core::number::Number) -> u64 {
    rn_core::number::with_tostring(n, |b| {
        short_core::<true>(
            st,
            b,
            mutants::active("snew_bytes_small"),
            mutants::active("snew_no_gc_check"),
        )
    })
}

/// P-B: the interned short string `s` (at most 40 bytes) with ONE hash and one chain walk (the probe of
/// [`rn_core::heap::Heap::jit_str_probe`] and its commit), or 0 when a step would be due after the allocation, it could fail or the
/// string table would grow. `small` / `nogc` are the names of the two mutants of the caller (`*_bytes_small`, `*_no_gc_check`). `FRESH`: the bytes were written by
/// byte stores just before (a formatted number, an assembled concatenation): the hash is read byte by byte (BUNDLE-P P3, [`rn_core::string::fast_hash_fresh`]).
fn short_core<const FRESH: bool>(
    st: &mut rn_core::state::State,
    s: &[u8],
    small: bool,
    nogc: bool,
) -> u64 {
    let heap = st.heap();
    let (h, found, bytes) = if FRESH {
        heap.jit_str_probe_fresh(s)
    } else {
        heap.jit_str_probe(s)
    };
    let Some(bytes) = bytes else {
        return 0;
    };
    let bytes = if small { 0 } else { bytes };
    if !nogc && !heap.jit_alloc_ok(bytes) {
        return 0;
    }
    // SAFETY: nothing allocated, collected or changed the string table since the probe, which said no growth is needed.
    unsafe { heap.jit_str_commit(s, h, found) }.map_or(0, |s| s.as_ptr() as u64)
}

/// `CONCAT` of two or three non-empty strings without the GC step (see [`jit_tr_alloc`], `ALLOC_CONCAT`): the string
/// `luaV_concat` makes (interned when it has at most 40 bytes), or 0 when nothing may be done: an operand is empty (the
/// interpreter returns another operand: its general path), the result has the other kind than the trace expects, a step
/// would be due after the allocation, it could fail, or creating it would grow the string table.
///
/// # Safety
/// The operands `ops[..n]` are live strings (addresses of `LuaString`s of the kinds `bits` says).
pub unsafe fn concat_core(st: &mut rn_core::state::State, bits: u32, ops: [u64; 3]) -> u64 {
    use rn_core::value::{ctb, Value, LUA_VLNGSTR, LUA_VSHRSTR};
    let n = if bits & 8 != 0 { 3 } else { 2 };
    let mut parts: [Option<rn_core::string::StrRef>; 3] = [None; 3];
    let mut tl = 0usize;
    for k in 0..n {
        let tag = if bits >> k & 1 != 0 {
            ctb(LUA_VLNGSTR)
        } else {
            ctb(LUA_VSHRSTR)
        };
        // SAFETY: caller contract; `Value` is `repr(C)` `{ tag word, payload }`.
        let v: Value = unsafe { std::mem::transmute::<[u64; 2], Value>([u64::from(tag), ops[k]]) };
        let Some(s) = v.as_strref() else {
            return 0;
        };
        let l = s.get().len();
        if l == 0 || l > (1usize << 40) {
            return 0;
        }
        tl += l;
        parts[k] = Some(s);
    }
    // MUTANT `concat_kind_skip`: the result kind is not compared with the recorded one
    if (tl > rn_core::string::MAX_SHORT_LEN) != (bits & 16 != 0)
        && !mutants::active("concat_kind_skip")
    {
        return 0;
    }
    let bytes_of = |k: usize| parts[k].expect("operand").get().as_bytes();
    if tl <= rn_core::string::MAX_SHORT_LEN {
        let mut short = [0u8; rn_core::string::MAX_SHORT_LEN];
        let mut off = 0;
        for k in 0..n {
            let b = bytes_of(k);
            short[off..off + b.len()].copy_from_slice(b);
            off += b.len();
        }
        return short_core::<true>(
            st,
            &short[..tl],
            mutants::active("concat_bytes_small"),
            mutants::active("concat_no_gc_check"),
        );
    }
    let heap = st.heap();
    let Some(bytes) = heap.jit_str_bytes_long(tl) else {
        return 0;
    };
    // MUTANT `concat_bytes_small`: the object size is left out of the bound
    let bytes = if mutants::active("concat_bytes_small") {
        0
    } else {
        bytes
    };
    // MUTANT `concat_no_gc_check`: allocate even when the collector would step right after the allocation
    if !mutants::active("concat_no_gc_check") && !heap.jit_alloc_ok(bytes) {
        return 0;
    }
    // SAFETY: the closure initialises all `tl` bytes (the operands' bytes in order); nothing allocates inside.
    let r = unsafe {
        heap.new_long_str_with(tl, |dst| {
            let mut off = 0;
            for k in 0..n {
                let b = bytes_of(k);
                std::ptr::copy_nonoverlapping(b.as_ptr(), dst.add(off), b.len());
                off += b.len();
            }
        })
    };
    r.map_or(0, |s| s.as_ptr() as u64)
}

/// B3-4: [`concat_core`] with number operands (the text of each is made here: one string-table probe for the whole result).
///
/// # Safety
/// As [`concat_core`]; operand `k` is an integer / float payload where bit `5 + k` / `8 + k` is set.
#[inline(never)]
unsafe fn concat_core_num(st: &mut rn_core::state::State, bits: u32, ops: [u64; 3]) -> u64 {
    use rn_core::value::{ctb, Value, LUA_VLNGSTR, LUA_VSHRSTR};
    let n = if bits & 8 != 0 { 3 } else { 2 };
    // B3-4: bits 5..7 = operand k is an integer, bits 8..10 = a float (its bits): the text is made here, no `SNEW` before
    let mut texts = [[0u8; 32]; 3];
    let mut parts: [(*const u8, usize); 3] = [(std::ptr::null(), 0); 3];
    let mut tl = 0usize;
    for k in 0..n {
        let num = if bits >> (5 + k) & 1 != 0 {
            Some(rn_core::number::Number::Int(ops[k] as i64))
        } else if bits >> (8 + k) & 1 != 0 {
            Some(rn_core::number::Number::Float(f64::from_bits(ops[k])))
        } else {
            None
        };
        let (p, l) = if let Some(num) = num {
            let t = &mut texts[k];
            let l = rn_core::number::with_tostring(num, |b| {
                t[..b.len()].copy_from_slice(b);
                b.len()
            });
            (t.as_ptr(), l)
        } else {
            let tag = if bits >> k & 1 != 0 {
                ctb(LUA_VLNGSTR)
            } else {
                ctb(LUA_VSHRSTR)
            };
            // SAFETY: caller contract; `Value` is `repr(C)` `{ tag word, payload }`.
            let v: Value =
                unsafe { std::mem::transmute::<[u64; 2], Value>([u64::from(tag), ops[k]]) };
            let Some(s) = v.as_strref() else {
                return 0;
            };
            let b = s.get().as_bytes();
            (b.as_ptr(), b.len())
        };
        if l == 0 || l > (1usize << 40) {
            return 0;
        }
        tl += l;
        parts[k] = (p, l);
    }
    // MUTANT `concat_kind_skip`: the result kind is not compared with the recorded one
    if (tl > rn_core::string::MAX_SHORT_LEN) != (bits & 16 != 0)
        && !mutants::active("concat_kind_skip")
    {
        return 0;
    }
    // SAFETY (all `bytes_of` uses): each part is `l` live bytes: a live string of the caller's contract, or `texts`.
    let bytes_of = |k: usize| unsafe { std::slice::from_raw_parts(parts[k].0, parts[k].1) };
    if tl <= rn_core::string::MAX_SHORT_LEN {
        let mut short = [0u8; rn_core::string::MAX_SHORT_LEN];
        let mut off = 0;
        for k in 0..n {
            let b = bytes_of(k);
            short[off..off + b.len()].copy_from_slice(b);
            off += b.len();
        }
        return short_core::<true>(
            st,
            &short[..tl],
            mutants::active("concat_bytes_small"),
            mutants::active("concat_no_gc_check"),
        );
    }
    let heap = st.heap();
    let Some(bytes) = heap.jit_str_bytes_long(tl) else {
        return 0;
    };
    // MUTANT `concat_bytes_small`: the object size is left out of the bound
    let bytes = if mutants::active("concat_bytes_small") {
        0
    } else {
        bytes
    };
    // MUTANT `concat_no_gc_check`: allocate even when the collector would step right after the allocation
    if !mutants::active("concat_no_gc_check") && !heap.jit_alloc_ok(bytes) {
        return 0;
    }
    // SAFETY: the closure initialises all `tl` bytes (the operands' bytes in order); nothing allocates inside.
    let r = unsafe {
        heap.new_long_str_with(tl, |dst| {
            let mut off = 0;
            for k in 0..n {
                let b = bytes_of(k);
                std::ptr::copy_nonoverlapping(b.as_ptr(), dst.add(off), b.len());
                off += b.len();
            }
        })
    };
    r.map_or(0, |s| s.as_ptr() as u64)
}

/// The string made of the bytes `s` (any length) without the GC step: a short one is interned (one probe, see [`short_core`]), a long
/// one is a new object; 0 when a step would be due, the allocation could fail or the string table would grow.
fn bytes_core(st: &mut rn_core::state::State, s: &[u8]) -> u64 {
    if s.len() <= rn_core::string::MAX_SHORT_LEN {
        return short_core::<false>(
            st,
            s,
            mutants::active("lib_bytes_small"),
            mutants::active("lib_no_gc_check"),
        );
    }
    let heap = st.heap();
    let Some(bytes) = heap.jit_str_bytes_long(s.len()) else {
        return 0;
    };
    let bytes = if mutants::active("lib_bytes_small") {
        0
    } else {
        bytes
    };
    if !mutants::active("lib_no_gc_check") && !heap.jit_alloc_ok(bytes) {
        return 0;
    }
    // SAFETY: the closure initialises all `s.len()` bytes; nothing allocates inside.
    unsafe {
        heap.new_long_str_with(s.len(), |dst| {
            std::ptr::copy_nonoverlapping(s.as_ptr(), dst, s.len())
        })
    }
    .map_or(0, |r| r.as_ptr() as u64)
}

/// N3b: whether the metatable `old` has a non-nil `__metatable` field (`get_metafield` of `base_setmetatable`), without creating the
/// name: a name that is not interned cannot be a key of any table.
pub(crate) fn setmt_protected(heap: &rn_core::heap::Heap, old: rn_core::table::TableRef) -> bool {
    let (_, found, _) = heap.jit_str_probe(b"__metatable");
    if found.is_null() {
        return false;
    }
    // SAFETY: `found` is a live interned short string (a probe result with no allocation since); `Value` is `repr(C)` `{ tag word, payload }`.
    let v: rn_core::value::Value = unsafe {
        std::mem::transmute::<[u64; 2], rn_core::value::Value>([
            u64::from(rn_core::value::ctb(rn_core::value::LUA_VSHRSTR)),
            found as u64,
        ])
    };
    match v.as_strref() {
        Some(k) => !old.get_short_str(k).is_nil(),
        None => false,
    }
}

/// P-O: C-stack levels kept free below `LUAI_MAXCCALLS` by the `CCALLS` helper (the real `pcall` nests `ccall`s; an exit lets it raise).
const CCALLS_MARGIN: u64 = 8;

/// `string.sub` of the bytes `b` for `args` = (string, i, j) (flag bits as in [`lib_core`]): the substring, or 0 (see [`bytes_core`] and
/// the kind check of flag bit 9). Shared by [`lib_core`] and the lean entry [`jit_tr_lib_sub`].
#[inline(always)]
fn sub_piece(st: &mut rn_core::state::State, b: &[u8], fl: u32, args: [u64; 3]) -> u64 {
    let l = b.len();
    let posrelat = |pos: i64| -> usize {
        if pos > 0 {
            pos as usize
        } else if pos == 0 || pos < -(l as i64) {
            1
        } else {
            l.wrapping_add(pos as usize).wrapping_add(1)
        }
    };
    let endpos = |pos: i64| -> usize {
        if pos > l as i64 {
            l
        } else if pos >= 0 {
            pos as usize
        } else if pos < -(l as i64) {
            0
        } else {
            l.wrapping_add(pos as usize).wrapping_add(1)
        }
    };
    let start = if mutants::active("sub_start_off") {
        posrelat(args[1] as i64) + 1
    } else {
        posrelat(args[1] as i64)
    };
    let end = endpos(args[2] as i64);
    let piece: &[u8] = if start <= end {
        &b[start - 1..end]
    } else {
        b""
    };
    if (piece.len() > rn_core::string::MAX_SHORT_LEN) != (fl & 2 != 0)
        && !mutants::active("lib_kind_skip")
    {
        return 0;
    }
    bytes_core(st, piece)
}

thread_local! {
    /// BUNDLE-C (`tfmt`): the reusable output buffer of the `FMT` helper.
    static FMT_BUF: std::cell::Cell<Vec<u8>> = const { std::cell::Cell::new(Vec::new()) };
    /// BUNDLE-H (`libcall`): the reusable result buffer of the `LOWER` / `UPPER` / `REVERSE` / `REP` helpers.
    static STR_BUF: std::cell::Cell<Vec<u8>> = const { std::cell::Cell::new(Vec::new()) };
    /// BUNDLE-C (`tfmt`): the trailing arguments the `FMTX` op left for the next extended `FMT` op, and their number.
    static FMT_STASH: std::cell::Cell<([rn_core::value::Value; 3], usize)> = const { std::cell::Cell::new(([rn_core::value::Value::NIL; 3], 0)) };
}

/// BUNDLE-C (`tfmt`): the values of the up to three operands of an `FMT` / `FMTX` op from the type fields of its flags (2 bits each from bit 4:
/// 0 integer, 1 short string, 2 long string, 3 none), and their number; `max` > 0 limits it (the extended form reads two). `None` for a
/// string operand that is not one.
fn fmt_values(
    fl: u32,
    args: [u64; 3],
    strv: &dyn Fn(u64, bool) -> Option<rn_core::string::StrRef>,
    max: usize,
) -> Option<([rn_core::value::Value; 3], usize)> {
    use rn_core::value::Value;
    let mut vals = [Value::NIL; 3];
    let mut n = 0;
    for j in 0..3 {
        if max != 0 && j == max {
            break;
        }
        vals[j] = match fl >> (4 + 2 * j) & 3 {
            0 => Value::int(args[j] as i64),
            c @ (1 | 2) => Value::string(strv(args[j], c == 2)?),
            _ => break,
        };
        n += 1;
    }
    Some((vals, n))
}

/// BUNDLE-H: the string of `kind` (`CHAR`, `CHAR2`, `CHAR3`) from the codes in `args`, or 0 above 255. Shared by [`lib_core`] and the lean entry [`jit_tr_lib_char`].
#[inline(always)]
fn char_core(st: &mut rn_core::state::State, kind: u32, args: [u64; 3]) -> u64 {
    let n = match kind {
        lib::CHAR => 1,
        lib::CHAR2 => 2,
        _ => 3,
    };
    let mut buf = [0u8; 3];
    for j in 0..n {
        // MUTANT `charn_range_skip`: a code above 255 is truncated
        if args[j] > 255 && !(kind != lib::CHAR && mutants::active("charn_range_skip")) {
            return 0;
        }
        buf[j] = args[j] as u8;
    }
    // MUTANT `charn_order`: the bytes come out reversed
    if kind != lib::CHAR && mutants::active("charn_order") {
        buf[..n].reverse();
    }
    bytes_core(st, &buf[..n])
}

/// `math.random(low, up)` (`RANDOM`: `args` = closure, low, up; the drawn integer plus one, 0 = nothing done) or `math.random()` (`RANDF`: the top 53
/// bits of the draw plus one). Shared by [`lib_core`] and the lean entry [`jit_tr_lib_random`].
///
/// # Safety
/// `args[0]` is a live C closure.
#[inline(always)]
unsafe fn random_core(st: &mut rn_core::state::State, kind: u32, args: [u64; 3]) -> u64 {
    use rn_core::value::{ctb, Value, LUA_VCCL};
    // SAFETY: caller contract; `Value` is `repr(C)` `{ tag word, payload }`.
    let v: Value =
        unsafe { std::mem::transmute::<[u64; 2], Value>([u64::from(ctb(LUA_VCCL)), args[0]]) };
    if kind == lib::RANDF {
        return match rn_std::lib_math::jit_random_float(st, &v) {
            // MUTANT `randf_plus_zero`: the result convention's `+ 1` is missing
            Some(m) => {
                if mutants::active("randf_plus_zero") {
                    m
                } else {
                    m + 1
                }
            }
            None => 0,
        };
    }
    let (low, up) = (args[1] as i64, args[2] as i64);
    // MUTANT `random_range_off`: the upper bound is one less
    let up = if mutants::active("random_range_off") {
        up.wrapping_sub(1)
    } else {
        up
    };
    match rn_std::lib_math::jit_random_range(st, &v, low, up) {
        Some(x) => (x as u64).wrapping_add(1),
        None => 0,
    }
}

/// BUNDLE-A: `FIND` / `FINDS` (the plain search, no allocation, no table touched). Shared by [`lib_core`] and the lean entry [`jit_tr_lib_find`].
#[inline(always)]
fn find_core(kind: u32, fl: u32, args: [u64; 3]) -> u64 {
    use rn_core::value::{ctb, Value, LUA_VLNGSTR, LUA_VSHRSTR};
    let strv = |p: u64, long: bool| -> Option<rn_core::string::StrRef> {
        let tag = if long {
            ctb(LUA_VLNGSTR)
        } else {
            ctb(LUA_VSHRSTR)
        };
        // SAFETY: caller contract of `lib_core`; `Value` is `repr(C)` `{ tag word, payload }`.
        let v: Value = unsafe { std::mem::transmute::<[u64; 2], Value>([u64::from(tag), p]) };
        v.as_strref()
    };
    let (Some(x), Some(y)) = (strv(args[0], fl & 1 != 0), strv(args[1], fl & 8 != 0)) else {
        return 0;
    };
    let (x, y) = (x.get().as_bytes(), y.get().as_bytes());
    // MUTANT `find_specials_skip`: a non-plain call is searched plainly even when the pattern has specials now
    if kind == lib::FINDS
        && !mutants::active("find_specials_skip")
        && !rn_std::pattern::nospecials(y)
    {
        return 0;
    }
    // MUTANT `find_init_neg_skip`: a negative start counts from 1
    let init = if mutants::active("find_init_neg_skip") && (args[2] as i64) < 0 {
        1
    } else {
        args[2] as i64
    };
    match rn_std::pattern::find_plain_pos(x, y, init) {
        Some(pos) => pos as u64 + 2,
        None => 1,
    }
}

/// `a == b`, `a < b`, `a <= b` of two strings (`STREQ` / `STRLT` / `STRLE`; flag bits 0 / 3 = `a` / `b` long): 2 when true, 1 when false, 0 for a bad
/// operand. Shared by [`lib_core`] and the lean entry [`jit_tr_lib_cmp`].
///
/// # Safety
/// The operands are live strings of the kinds the flags say.
#[inline(always)]
unsafe fn cmp_core_raw(kind: u32, fl: u32, args: [u64; 3]) -> u64 {
    use rn_core::value::{ctb, Value, LUA_VLNGSTR, LUA_VSHRSTR};
    let strv = |p: u64, long: bool| -> Option<rn_core::string::StrRef> {
        let tag = if long {
            ctb(LUA_VLNGSTR)
        } else {
            ctb(LUA_VSHRSTR)
        };
        // SAFETY: caller contract; `Value` is `repr(C)` `{ tag word, payload }`.
        let v: Value = unsafe { std::mem::transmute::<[u64; 2], Value>([u64::from(tag), p]) };
        v.as_strref()
    };
    let (Some(x), Some(y)) = (strv(args[0], fl & 1 != 0), strv(args[1], fl & 8 != 0)) else {
        return 0;
    };
    let (x, y) = (x.get().as_bytes(), y.get().as_bytes());
    let r = match kind {
        lib::STREQ => x == y,
        lib::STRLT => x < y,
        _ => x <= y,
    };
    // MUTANT `strcmp_lt_as_le`: `<` answers like `<=`
    let r = if kind == lib::STRLT && mutants::active("strcmp_lt_as_le") {
        x <= y
    } else {
        r
    };
    1 + u64::from(r)
}

#[inline(always)]
fn cmp_core(kind: u32, fl: u32, args: [u64; 3]) -> u64 {
    // SAFETY: the contract of [`lib_core`].
    unsafe { cmp_core_raw(kind, fl, args) }
}

/// BUNDLE-H: `LOWER` / `UPPER` / `REVERSE` / `REP` / `REPS` (see the kinds in `ir::lib`). Shared by [`lib_core`] and the lean entry [`jit_tr_lib_strfn`].
#[inline(always)]
fn strfn_core(st: &mut rn_core::state::State, kind: u32, fl: u32, args: [u64; 3]) -> u64 {
    use rn_core::value::{ctb, Value, LUA_VLNGSTR, LUA_VSHRSTR};
    let strv = |p: u64, long: bool| -> Option<rn_core::string::StrRef> {
        let tag = if long {
            ctb(LUA_VLNGSTR)
        } else {
            ctb(LUA_VSHRSTR)
        };
        // SAFETY: caller contract of `lib_core`; `Value` is `repr(C)` `{ tag word, payload }`.
        let v: Value = unsafe { std::mem::transmute::<[u64; 2], Value>([u64::from(tag), p]) };
        v.as_strref()
    };
    let Some(s) = strv(args[0], fl & 1 != 0) else {
        return 0;
    };
    let b = s.get().as_bytes();
    let mut buf = STR_BUF.with(std::cell::Cell::take);
    buf.clear();
    if kind == lib::REP || kind == lib::REPS {
        let sep: &[u8] = if kind == lib::REPS {
            match strv(args[2], fl & 16 != 0) {
                Some(p) => p.get().as_bytes(),
                None => return 0,
            }
        } else {
            b""
        };
        let n = args[1] as i64;
        let per = b.len() + sep.len();
        let total = if n <= 0 || per == 0 {
            0
        } else {
            match per.checked_mul(n as usize) {
                Some(t) if t - sep.len() <= lib::REP_MAX || mutants::active("rep_cap_skip") => {
                    t - sep.len()
                }
                _ => {
                    STR_BUF.with(|c| c.set(buf));
                    return 0;
                }
            }
        };
        if (total > rn_core::string::MAX_SHORT_LEN) != (fl & 2 != 0)
            && !mutants::active("lib_kind_skip")
        {
            STR_BUF.with(|c| c.set(buf));
            return 0;
        }
        // BUNDLE-R: one copy of `s` + `sep`, then the buffer doubles itself (`extend_from_within`) up to `n * per`, and the last separator
        // is cut off. MUTANT `rep_sep_trailing`: the separator also follows the last copy
        if total != 0 {
            let want = total + sep.len();
            buf.reserve(want);
            if per == 1 {
                buf.resize(want, if b.is_empty() { sep[0] } else { b[0] });
            } else {
                buf.extend_from_slice(b);
                buf.extend_from_slice(sep);
                while buf.len() < want {
                    let k = buf.len().min(want - buf.len());
                    buf.extend_from_within(..k);
                }
            }
            if !mutants::active("rep_sep_trailing") {
                buf.truncate(total);
            }
        }
    } else {
        match kind {
            // MUTANT `lower_is_upper`: `lower` upper-cases
            lib::LOWER if !mutants::active("lower_is_upper") => {
                buf.extend(b.iter().map(u8::to_ascii_lowercase))
            }
            lib::LOWER | lib::UPPER => buf.extend(b.iter().map(u8::to_ascii_uppercase)),
            _ => buf.extend(b.iter().rev()),
        }
    }
    let r = bytes_core(st, &buf);
    STR_BUF.with(|c| c.set(buf));
    r
}

/// BUNDLE-H (`libcall` bit 1): the common prologue of the lean entries of the `LIB` helpers (as [`jit_tr_lib_sub`]).
macro_rules! lean_lib {
    ($(#[$m:meta])* $name:ident, |$st:ident, $kind:ident, $fl:ident, $args:ident| $body:expr) => {
        $(#[$m])*
        ///
        /// # Safety
        /// As [`jit_tr_alloc`] with `ALLOC_LIB` of a kind the entry serves.
        pub unsafe extern "C" fn $name(
            th: *mut rn_core::state::Thread,
            _ci: *mut rn_core::state::CallInfo,
            a: u32,
            _y: u32,
        ) -> u32 {
            // SAFETY: `th` is the live thread pinned in `r12`.
            let mut $st = unsafe { rn_core::jit_hooks::state_from_thread(th.cast()) };
            let budget = crate::rt::cnt_ptr(&mut $st);
            // SAFETY: as in `jit_tr_calln`.
            let area = unsafe { &mut *budget.cast::<u8>().add(exit_area::FROM_R15 as usize).cast::<super::ExitArea>() };
            let ($kind, $fl) = ((a >> 8) & 0xFF, a >> 16);
            let $args = [arg(area, 0), arg(area, 1), arg(area, 2)];
            let _ = ($kind, $fl);
            area.args[0] = $body;
            0
        }
    };
}

lean_lib!(
    /// BUNDLE-H: lean entry of `LIB char` / `char2` / `char3`.
    jit_tr_lib_char, |st, kind, _fl, args| char_core(&mut st, kind, if mutants::active("libk_lean_char_args") { [args[0], args[0], args[1]] } else { args })
);
lean_lib!(
    /// BUNDLE-H: lean entry of `LIB random` / `randf`.
    jit_tr_lib_random, |st, kind, _fl, args| unsafe { random_core(&mut st, kind, args) }
);
lean_lib!(
    /// BUNDLE-H: lean entry of `LIB streq` / `strlt` / `strle`.
    jit_tr_lib_cmp, |st, kind, fl, args| { let _ = &st; cmp_core(kind, fl, if mutants::active("libk_lean_cmp_swapped") { [args[1], args[0], args[2]] } else { args }) }
);
lean_lib!(
    /// BUNDLE-H: lean entry of `LIB lower` / `upper` / `reverse` / `rep` / `reps`.
    jit_tr_lib_strfn, |st, kind, fl, args| strfn_core(&mut st, kind, fl, args)
);
lean_lib!(
    /// BUNDLE-R (`libcall` bit 9): lean entry of `LIB find` / `finds`.
    jit_tr_lib_find, |st, kind, fl, args| { let _ = &st; find_core(kind, fl, if mutants::active("libk_lean_find_swapped") { [args[1], args[0], args[2]] } else { args }) }
);

/// BUNDLE-H (`libcall` bit 1): the lean entry that serves `LIB` kind `kind`, if there is one.
fn lean_lib_entry(kind: u32, libcall: u32) -> Option<u64> {
    Some(match kind {
        lib::CHAR | lib::CHAR2 | lib::CHAR3 => jit_tr_lib_char as *const () as usize as u64,
        lib::RANDOM | lib::RANDF => jit_tr_lib_random as *const () as usize as u64,
        lib::STREQ | lib::STRLT | lib::STRLE => jit_tr_lib_cmp as *const () as usize as u64,
        lib::LOWER | lib::UPPER | lib::REVERSE | lib::REP | lib::REPS => {
            jit_tr_lib_strfn as *const () as usize as u64
        }
        lib::FIND | lib::FINDS if libcall & 512 != 0 => {
            jit_tr_lib_find as *const () as usize as u64
        }
        _ => return None,
    })
}

/// The library-call helpers of P-B (`TOp::Lib`; `x` = kind | flags << 8, see [`ALLOC_LIB`]): they do exactly what the C function would do
/// for the arguments the recorder has seen, without the GC step, and return 0 ("did nothing": the trace exits and the interpreter runs the
/// call) for everything else.
///
/// * `SUB`: `args` = (string, i, j) -> the substring (`str_sub`); 0 if its kind is not the recorded one (flag bit 9).
/// * `BYTE`: `args` = (string, i) -> the byte plus one when `i` names exactly one byte, else 0.
/// * `CHAR`: `args[0]` = the code -> the one-byte string, 0 if it is above 255.
/// * `STRMETH`: `args[0]` = interned name -> the light C function stored under it in the `__index` table of the string metatable; 0 otherwise.
/// * `TYPE`: `args` = (C closure, type index) -> that upvalue string of the closure if it is `type`'s; 0 otherwise.
/// * `TYPENAME`: `args[0]` = index into [`lib::TYPENAMES`] -> the interned name.
/// * `TOSTR`: `args[0]` = the number bits (flag bit 10: float) -> its string; 0 if numbers have a metatable.
///
/// # Safety
/// The operands are live objects of the types the recorded kind and flags say (strings of the kind in flag bit 8, a live C closure).
pub unsafe fn lib_core(st: &mut rn_core::state::State, x: u32, args: [u64; 3]) -> u64 {
    use rn_core::value::{ctb, Value, LUA_VCCL, LUA_VLNGSTR, LUA_VSHRSTR};
    let (kind, fl) = (x & 0xFF, x >> 8);
    let strv = |p: u64, long: bool| -> Option<rn_core::string::StrRef> {
        let tag = if long {
            ctb(LUA_VLNGSTR)
        } else {
            ctb(LUA_VSHRSTR)
        };
        // SAFETY: caller contract; `Value` is `repr(C)` `{ tag word, payload }`.
        let v: Value = unsafe { std::mem::transmute::<[u64; 2], Value>([u64::from(tag), p]) };
        v.as_strref()
    };
    match kind {
        lib::SUB | lib::BYTE => {
            let Some(s) = strv(args[0], fl & 1 != 0) else {
                return 0;
            };
            let b = s.get().as_bytes();
            let l = b.len();
            let posrelat = |pos: i64| -> usize {
                if pos > 0 {
                    pos as usize
                } else if pos == 0 || pos < -(l as i64) {
                    1
                } else {
                    l.wrapping_add(pos as usize).wrapping_add(1)
                }
            };
            let endpos = |pos: i64| -> usize {
                if pos > l as i64 {
                    l
                } else if pos >= 0 {
                    pos as usize
                } else if pos < -(l as i64) {
                    0
                } else {
                    l.wrapping_add(pos as usize).wrapping_add(1)
                }
            };
            if kind == lib::BYTE {
                let i = args[1] as i64;
                let (posi, pose) = (posrelat(i), endpos(i));
                if posi != pose || posi < 1 || posi > l {
                    return 0;
                }
                let byte = u64::from(b[posi - 1]);
                return if mutants::active("byte_plus1_skip") {
                    byte
                } else {
                    byte + 1
                };
            }
            sub_piece(st, b, fl, args)
        }
        lib::STREQ | lib::STRLT | lib::STRLE => cmp_core(kind, fl, args),
        lib::FIND | lib::FINDS => find_core(kind, fl, args),
        lib::FMTX => {
            // BUNDLE-C: stash the trailing arguments of the extended `FMT` form (see `lib::FMTX`)
            let Some((vals, n)) = fmt_values(fl, args, &strv, 0) else {
                return 0;
            };
            FMT_STASH.with(|c| c.set((vals, n)));
            1
        }
        k if lib::is_fmt(k) => {
            // BUNDLE-C (`tfmt`): the reference format engine on the argument values (see `rn_std::lib_string::jit_format`)
            let Some(fmt) = rn_std::lib_string::jit_fmt_get(((k - lib::FMT_BASE) & 63) as usize)
            else {
                return 0;
            };
            let ext = lib::is_fmt_ext(k);
            // the direct arguments: three (normal form) or two (extended form: the third operand is the stash op's result)
            let Some((d, dn)) = fmt_values(fl, args, &strv, if ext { 2 } else { 3 }) else {
                return 0;
            };
            let mut vals = [Value::NIL; 5];
            vals[..dn].copy_from_slice(&d[..dn]);
            let mut n = dn;
            if ext {
                // MUTANT `tfmt_stash_stale`: the stash is not cleared by the `FMT` that used it
                let (sv, sn) = if mutants::active("tfmt_stash_stale") {
                    FMT_STASH.with(|c| c.get())
                } else {
                    FMT_STASH.with(|c| c.replace(([Value::NIL; 3], 0)))
                };
                if sn == 0 || dn != 2 {
                    return 0;
                }
                vals[2..2 + sn].copy_from_slice(&sv[..sn]);
                n += sn;
            }
            // MUTANT `tfmt_args_swap`: the first two arguments swap
            if n >= 2 && mutants::active("tfmt_args_swap") {
                vals.swap(0, 1);
            }
            let mut out = FMT_BUF.with(std::cell::Cell::take);
            out.clear();
            let ok = rn_std::lib_string::jit_format(st, fmt, &vals[..n], &mut out);
            // MUTANT `tfmt_kind_skip`: the result kind is not compared with the recorded one
            let r = if ok
                && ((out.len() > rn_core::string::MAX_SHORT_LEN) == (fl & 2 != 0)
                    || mutants::active("tfmt_kind_skip"))
            {
                bytes_core(st, &out)
            } else {
                0
            };
            FMT_BUF.with(|c| c.set(out));
            r
        }
        lib::GMATCH => {
            // SAFETY: caller contract: a live C closure (the trace's type guard on the operand); `Value` is `repr(C)` `{ tag word, payload }`.
            let v: Value = unsafe {
                std::mem::transmute::<[u64; 2], Value>([u64::from(ctb(LUA_VCCL)), args[0]])
            };
            let want_long = fl & 2 != 0;
            let mut res = 0u64;
            let mut with = |b: &[u8]| -> bool {
                if (b.len() > rn_core::string::MAX_SHORT_LEN) != want_long
                    && !mutants::active("gm_kind_skip")
                {
                    return false;
                }
                res = bytes_core(st, b);
                res != 0
            };
            // MUTANT `gm_no_commit`: the position upvalues stay (the loop never advances)
            let commit = !mutants::active("gm_no_commit");
            match rn_std::lib_string::jit_gmatch_step(&v, commit, &mut with) {
                rn_std::lib_string::GmStep::Yield(_) => res,
                // the iteration is over (state advanced like the C function does): 1, which the guard behind the helper tells from a string
                rn_std::lib_string::GmStep::End => 1,
                _ => 0,
            }
        }
        lib::CCALLS => {
            let used =
                u64::from(rn_core::state::get_ccalls(st.ncalls_raw())) + args[0] + CCALLS_MARGIN;
            u64::from(
                used < u64::from(rn_core::state::LUAI_MAXCCALLS)
                    || mutants::active("ccalls_guard_off"),
            )
        }
        lib::SETMT => {
            // SAFETY: caller contract: `args[0]` a live table (the trace's type guard), `args[1]` a live table or 0.
            unsafe { setmt_core(st, args[0], args[1]) }
        }
        lib::IPAIRS_END => {
            // SAFETY: caller contract: a live table (the trace's type guard on the operand).
            let t = unsafe {
                rn_core::table::TableRef::from_raw(args[0] as *mut rn_core::table::Table)
            };
            u64::from(t.get_int(args[1] as i64).is_nil() || mutants::active("gfor_end_always"))
        }
        lib::CHAR | lib::CHAR2 | lib::CHAR3 => char_core(st, kind, args),
        lib::RANDF | lib::RANDOM => {
            // SAFETY: caller contract: a live C closure (its tag is checked by the trace's type guard).
            unsafe { random_core(st, kind, args) }
        }
        lib::LOWER | lib::UPPER | lib::REVERSE | lib::REP | lib::REPS => {
            strfn_core(st, kind, fl, args)
        }
        lib::STRMETH => {
            let Some(key) = strv(args[0], false) else {
                return 0;
            };
            let Some(mt) = st.type_metatable(rn_core::value::LuaType::String) else {
                return 0;
            };
            let name = st.g().heap.tm_name(rn_core::meta::Tm::Index);
            let Some(ix) = mt.short_str_node(name) else {
                return 0;
            };
            let Some(idxt) = mt.node_value(ix).as_table() else {
                return 0;
            };
            let Some(kx) = idxt.short_str_node(key) else {
                return 0;
            };
            let v = idxt.node_value(kx);
            if v.tag() != rn_core::value::LUA_VLCF {
                return 0;
            }
            v.as_native().map_or(0, |f| f as usize as u64)
        }
        lib::TYPE => {
            // SAFETY: caller contract: a live C closure (its tag is checked by the trace's type guard).
            let v: Value = unsafe {
                std::mem::transmute::<[u64; 2], Value>([u64::from(ctb(LUA_VCCL)), args[0]])
            };
            let Some(c) = v.as_cclosure() else {
                return 0;
            };
            if c.f as usize != rn_core::jit_hooks::type_fn_addr()
                && !mutants::active("type_closure_unchecked")
            {
                return 0;
            }
            let k = args[1] as usize;
            if k >= c.nupvalues() {
                return 0;
            }
            c.upvalue(k).as_strref().map_or(0, |s| s.as_ptr() as u64)
        }
        lib::TYPENAME => match lib::TYPENAMES.get(args[0] as usize) {
            Some(n) => short_core::<false>(
                st,
                n.as_bytes(),
                mutants::active("lib_bytes_small"),
                mutants::active("lib_no_gc_check"),
            ),
            None => 0,
        },
        lib::TOSTR => {
            if st.type_metatable(rn_core::value::LuaType::Number).is_some()
                && !mutants::active("tostr_meta_skip")
            {
                return 0;
            }
            let n = if fl & 4 != 0 {
                rn_core::number::Number::Float(f64::from_bits(args[0]))
            } else {
                rn_core::number::Number::Int(args[0] as i64)
            };
            rn_core::number::with_tostring(n, |b| {
                short_core::<true>(
                    st,
                    b,
                    mutants::active("lib_bytes_small"),
                    mutants::active("lib_no_gc_check"),
                )
            })
        }
        _ => 0,
    }
}

/// The `NEWREF` insertion (see [`jit_tr_alloc`]).
///
/// # Safety
/// `t` is a live table, `key` a live interned short string.
pub unsafe fn newref_core(st: &mut rn_core::state::State, t: u64, key: u64) -> u64 {
    // SAFETY: caller contract; `Value` is `repr(C)` `{ tag word, payload }`.
    let (t, key) = unsafe {
        let kv: rn_core::value::Value = std::mem::transmute::<[u64; 2], rn_core::value::Value>([
            u64::from(rn_core::value::ctb(rn_core::value::LUA_VSHRSTR)),
            key,
        ]);
        (
            rn_core::table::TableRef::from_raw(t as *mut rn_core::table::Table),
            kv.as_strref().expect("short string key"),
        )
    };
    // MUTANT `newref_present_ok`: a key that is already there is "inserted" again (a store of the placeholder)
    let heap = st.heap();
    t.newref_shortstr(heap, key, mutants::active("newref_present_ok"))
        .map_or(0, |ix| t.node_addr(ix) as u64)
}

/// Deprecated since PIa: `ra::allocate` accepts the recorder's canonical form (`b = NOREF` on `K*`), so this is a
/// no-op kept for callers of the P5b pipeline. Returns 0.
#[deprecated(note = "ra::allocate accepts KPTR with b = NOREF since PIa")]
pub fn normalise_for_ra(_ir: &mut TraceIr) -> usize {
    0
}

// ---- the context ------------------------------------------------------------------------------------------------

fn area_m(off: usize) -> M {
    M {
        base: Base::Cnt,
        off: exit_area::FROM_R15 + off as i32,
    }
}

fn spill_m(s: u16) -> M {
    area_m(exit_area::SPILL + 8 * usize::from(s))
}

fn fm(slot: u16, off: i32) -> M {
    if slot == FUNC_SLOT {
        // the function slot below the root base: payload of the closure at `rbx - 16 + 8`
        return M {
            base: Base::Frame,
            off: off - 16,
        };
    }
    M {
        base: Base::Frame,
        off: i32::from(slot) * 16 + off,
    }
}

/// H2: identity of a frame chain (`FrameRestore` fields that the rebuild code embeds).
type MatKey = (u16, i16, u32, usize, usize, u16, bool);

fn rm(r: V, off: i32) -> M {
    M {
        base: Base::Reg(r),
        off,
    }
}

fn fits(v: i64) -> Option<i32> {
    i32::try_from(v).ok()
}

fn neg_cc(cc: Cc) -> Cc {
    match cc {
        Cc::Eq => Cc::Ne,
        Cc::Ne => Cc::Eq,
        Cc::Lt => Cc::Ge,
        Cc::Le => Cc::Gt,
        Cc::Gt => Cc::Le,
        Cc::Ge => Cc::Lt,
        Cc::Ult => Cc::Uge,
        Cc::Uge => Cc::Ult,
    }
}

fn collectable(t: Ty) -> bool {
    matches!(
        t,
        Ty::ShrStr | Ty::LngStr | Ty::Tab | Ty::Lcf | Ty::Ccl | Ty::Udata | Ty::Thread
    )
}

struct Stub {
    snap: u32,
    entry: bool,
    /// P-L: the stub of a poll exit (always generic, tagged `exit::EXIT_POLL`).
    poll: bool,
    label: L,
}

/// Is a guard an entry check (3.3)? Exact, by position: a checked `SLOAD` / `EQREF` with an empty map and no frames
/// that is emitted in the entry segment (peeled layout: before `LOOP`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum EntryKind {
    No,
    Yes,
    /// Rolled layout: the code of iteration 1 is the code of every iteration, so the first-iteration flag (extension
    /// slot [`FIRST_SLOT_K`]: set by the entry sequence, cleared at the back-edge) decides at the exit.
    FirstIter,
}

/// A dispatcher stub of a [`EntryKind::FirstIter`] guard: `if first { entry stub } else { plain stub }`.
struct Disp {
    snap: u32,
    label: L,
    entry: L,
    plain: L,
}

struct Cx<'a> {
    ir: &'a TraceIr,
    al: &'a Allocation,
    snaps: &'a [ExitSnap],
    env: &'a AsmEnv,
    p: Program,
    stubs: Vec<Stub>,
    disps: Vec<Disp>,
    /// The first-iteration flag is maintained (rolled layout, head data, some `FirstIter` guard).
    first_flag: bool,
    ext: u16,
    /// Temp registers saved in extension slots right now (nesting is not needed: one at a time per instruction).
    saved: Vec<(Loc, u16)>,
    nstubs: usize,
    /// `exit::pre_roll_snaps(ir)`.
    pre_snaps: Vec<bool>,
    /// P3: snapshots used by a callee identity guard (`exit::guard_kinds` bit 1), empty without `AsmEnv::chain`.
    chain_snaps: Vec<bool>,
    /// H2: `exit::site_classes(ir)`.
    sites: Vec<super::cache::ExitSite>,
    /// H2: the frame chains whose rebuild code is shared (`mat_stub`), with their labels, and the chains themselves.
    mat_shared: Vec<(Vec<MatKey>, L)>,
    mat_frames: Vec<Vec<exit::FrameRestore>>,
    /// N2a: the root code of each `mat_frames` chain (the segment's; `env.root_code` for segment 0).
    mat_codes: Vec<u64>,
    /// B2 (`b2tag`): node addresses (refs of `HREFK` / `HREFV`) whose value tag byte is known at this point of the straight-line
    /// code (an earlier checked `HLOAD` of that type or an `HSTORE` of it).
    ktag: Vec<(Ref, u8)>,
    /// B4-3 (`hkfuse`): the `HLOAD` (instruction index) whose value-tag check the preceding `HREFK` already did (one 16-bit compare of
    /// the value tag byte and the key tag byte).
    hk_fused: Option<usize>,
    /// J1-1: the fused pairs whose two guards leave through different snapshots: a failed 16-bit compare jumps to a cold
    /// disambiguator (`cmp byte [node + KEY_TT], tag; jne hrefk_exit; jmp hload_exit`), so each guard keeps its own exit (and class).
    hk_cold: Vec<HkCold>,
    /// P2-3a: the refund stubs of the `sub r, n; jl` budgets: `(label, exit, n)`: `add r, n; jmp exit`.
    bud_cold: Vec<(L, L, i32)>,
    /// J1-1: number of `HREFK`+`HLOAD` pairs fused (`Asm::hk_fused`).
    hk_fused_n: u32,
    hk_split_n: u32,
    dbl: Vec<u32>,
    /// BUNDLE-K L1: labels of the shared leave blocks (`[0]`: plain, `[1]`: the innermost frame was just pushed natively), see [`Cx::lazy_leave_block`].
    lz_leave: [Option<L>; 2],
}

impl Cx<'_> {
    /// Places the loop head at a deterministic position inside a 64-byte line: `HEAD_OFFSET` bytes after a line start.
    fn head_pad(&mut self) {
        self.push(Op::Align(LINE));
        self.push(Op::Pad(HEAD_OFFSET));
    }

    fn push(&mut self, op: Op) {
        self.p.push(op);
    }

    fn label(&mut self) -> L {
        self.p.label()
    }

    fn bind(&mut self, l: L) {
        self.push(Op::Bind(l));
    }

    fn ext_slot(&self, k: u16) -> u16 {
        self.ext + k
    }

    fn ld(&mut self, dst: V, w: W, m: M) {
        self.push(Op::Ld { dst, w, m });
    }

    fn st(&mut self, w: W, m: M, src: Src) {
        self.push(Op::St { w, m, src });
    }

    fn br(&mut self, cc: Cc, a: V, b: Src, to: L) {
        self.push(Op::Br { cc, a, b, to });
    }

    // ---- constants and locations ----------------------------------------------------------------------------------

    fn kbits_of(&self, r: Ref) -> u64 {
        kbits(self.ir.kval(r))
    }

    /// The general register of an operand (a constant is materialised into `scratch`).
    fn reg(&mut self, loc: Loc, scratch: V) -> V {
        match loc {
            Loc::G(n) => n,
            Loc::Const(r) => {
                let b = self.kbits_of(r);
                self.push(Op::Mov(scratch, b));
                scratch
            }
            l => panic!("operand location {l:?} is not a general register"),
        }
    }

    /// An integer operand as a `Src` (a constant that does not fit 32 bits is materialised into `scratch`).
    fn src(&mut self, loc: Loc, scratch: V) -> Src {
        match loc {
            Loc::G(n) => Src::V(n),
            Loc::Const(r) => {
                let b = self.kbits_of(r) as i64;
                match fits(b) {
                    Some(i) => Src::Imm(i),
                    None => {
                        self.push(Op::Mov(scratch, b as u64));
                        Src::V(scratch)
                    }
                }
            }
            l => panic!("operand location {l:?} is not an integer operand"),
        }
    }

    fn xreg(loc: Loc) -> X {
        match loc {
            Loc::X(n) => n,
            l => panic!("operand location {l:?} is not a double register"),
        }
    }

    /// P2-3c: the pool cell of the double constant `b` (`[r15 + disp]`), or `None` (pool off or full).
    fn fpool_m(&mut self, b: u64) -> Option<M> {
        if self.env.fpool == 0 {
            return None;
        }
        // SAFETY: `env.fpool` is the address of the live per-state pool (`AsmEnv::fpool`); one assembly runs at a time.
        let pool = unsafe { &mut *(self.env.fpool as *mut super::FPool) };
        let k = pool.intern(if mutants::active("fpool_bits_off") {
            b ^ 1
        } else {
            b
        })?;
        Some(M {
            base: Base::Cnt,
            off: self.env.fpool_off + 8 * k as i32,
        })
    }

    /// Loads the double constant `b` into `X` register `dst`: one pool load, else `movabs; movq` through `V7`.
    fn ldf_const(&mut self, dst: X, b: u64) {
        match self.fpool_m(b) {
            Some(m) => self.push(Op::LdF { dst, m }),
            None => {
                self.push(Op::Mov(7, b));
                self.push(Op::GtoF(dst, 7));
            }
        }
    }

    /// Double operand into register `dst` (constants through `V7`).
    fn load_x(&mut self, dst: X, loc: Loc) {
        match loc {
            Loc::X(n) => {
                if n != dst {
                    self.push(Op::MovF(dst, n));
                }
            }
            Loc::Const(r) => {
                let b = self.kbits_of(r);
                self.ldf_const(dst, b);
            }
            l => panic!("operand location {l:?} is not a double operand"),
        }
    }

    // ---- saved temporaries ------------------------------------------------------------------------------------------

    fn save_slot(&self, k: usize) -> u16 {
        self.ext_slot(2 + k as u16)
    }

    /// Saves `loc` (a register) into the next free temp slot.
    fn save(&mut self, loc: Loc) {
        let s = self.save_slot(self.saved.len());
        match loc {
            Loc::G(n) => self.st(W::B8, spill_m(s), Src::V(n)),
            Loc::X(n) => self.push(Op::StF {
                m: spill_m(s),
                src: n,
            }),
            _ => unreachable!(),
        }
        self.saved.push((loc, s));
    }

    /// Restores every saved temporary (registers are reloaded; the list is kept for a second path).
    fn restore_all(&mut self) {
        let v = self.saved.clone();
        for (loc, s) in v {
            match loc {
                Loc::G(n) => self.ld(n, W::B8, spill_m(s)),
                Loc::X(n) => self.push(Op::LdF {
                    dst: n,
                    m: spill_m(s),
                }),
                _ => unreachable!(),
            }
        }
    }

    // ---- exits ------------------------------------------------------------------------------------------------------

    /// Is guard `i` an entry check (3.3)? See [`EntryKind`].
    fn entry_kind(&self, i: usize) -> EntryKind {
        let ins = &self.ir.ins[i];
        if ins.flags & F_GUARD == 0 {
            return EntryKind::No; // (F3: a non-guard's `aux` is not a snapshot index: `TNEW` keeps its array size there)
        }
        let s = &self.ir.snaps[aux_snap(ins.aux) as usize];
        let kind = match ins.op {
            TOp::Sload => aux_bits(ins.aux) & 1 != 0,
            TOp::Eqref => true,
            _ => false,
        };
        // (N2b: a snapshot after a `RETF` is not the state at the head, whatever its map)
        if !(kind && s.seg == 0 && s.map.1 == 0 && s.nframes == 0 && s.frame == 0 && s.top == 0)
            || self.env.head == 0
        {
            return EntryKind::No;
        }
        match self.ir.loop_at {
            None => EntryKind::FirstIter,
            Some(l) if i < (l - REF_BIAS) as usize => EntryKind::Yes,
            Some(_) => EntryKind::No,
        }
    }

    /// The label of the exit stub of guard `i`, with the `+exitstorm` check emitted at the current position.
    fn guard_label(&mut self, i: usize) -> Result<L, AsmError> {
        let snap = aux_snap(self.ir.ins[i].aux);
        let l = match self.entry_kind(i) {
            EntryKind::No if self.ir.ins[i].op == TOp::Poll => self.poll_stub_label(snap)?,
            EntryKind::No => self.stub_label(snap, false)?,
            EntryKind::Yes => self.stub_label(snap, true)?,
            EntryKind::FirstIter => self.disp_label(snap)?,
        };
        // F8: the guard right behind an allocation (`NE new, 0`) fails only when the allocation failed, i.e. when no object
        // exists; a spurious exit there would resume tier 1 *before* the instruction after the object was already
        // allocated (a leaked table that the interpreter never allocates: the byte count of a stopped collector drifts)
        let alloc_guard = {
            let g = &self.ir.ins[i];
            g.op == TOp::Ne
                && TraceIr::is_ins(g.a)
                && (matches!(self.ir.at(g.a).op, TOp::Tnew | TOp::Snew | TOp::Concat)
                    // Y2: `TSET` (`table.insert` / `t[#t + 1] = v` as `TLEN` + `TSET`) likewise: the helper returns 0 only when it stored nothing
                    // (memory limit); a spurious exit behind a successful store resumes at the `CALL` and stores the element a second time
                    // (found by `tr_oracle::oracle_exits`: `bench/programs/table_array` with `+exitstorm=7 +randra=5`, `#u` 377 instead of 375)
                    || self.ir.at(g.a).op == TOp::Tset
                    // BUNDLE-GM: the `gmatch` helper has advanced the iterator's position when its guards run: a spurious exit would
                    // make the interpreter's call skip a match (mutant `gm_storm_guard`: the storm fails them too)
                    || (self.ir.at(g.a).op == TOp::Lib && self.ir.at(g.a).aux & 0xFF == lib::GMATCH && !mutants::active("gm_storm_guard"))
                    // C43: `math.random` likewise: the guard behind the `RANDOM` helper runs after the generator state advanced, its exit
                    // re-executes the `CALL` and draws again (mutant `rand_storm_guard`; test-only machinery: a real failure of this guard
                    // means the helper changed nothing)
                    || (self.ir.at(g.a).op == TOp::Lib && matches!(self.ir.at(g.a).aux & 0xFF, lib::RANDOM | lib::RANDF) && !mutants::active("rand_storm_guard")))
        };
        if let (Some(n), false) = (self.env.exitstorm, alloc_guard) {
            // counter in an extension slot: `if --ctr <= 0 { ctr = n; exit }`; V7 only
            let m = spill_m(self.ext_slot(1));
            self.ld(7, W::B8, m);
            self.push(Op::Sub(7, Src::Imm(1)));
            self.st(W::B8, m, Src::V(7));
            let ok = self.label();
            self.br(Cc::Gt, 7, Src::Imm(0), ok);
            self.push(Op::Mov(7, u64::from(n)));
            self.st(W::B8, m, Src::V(7));
            self.push(Op::Jmp(l));
            self.bind(ok);
        }
        Ok(l)
    }

    fn disp_label(&mut self, snap: u32) -> Result<L, AsmError> {
        if let Some(d) = self.disps.iter().find(|d| d.snap == snap) {
            return Ok(d.label);
        }
        let entry = self.stub_label(snap, true)?;
        let plain = self.stub_label(snap, false)?;
        let label = self.p.label();
        self.disps.push(Disp {
            snap,
            label,
            entry,
            plain,
        });
        Ok(label)
    }

    fn stub_label(&mut self, snap: u32, entry: bool) -> Result<L, AsmError> {
        if (snap as usize) >= self.snaps.len() {
            return incons(format!("snapshot {snap} has no exit"));
        }
        if let Some(s) = self
            .stubs
            .iter()
            .find(|s| s.snap == snap && s.entry == entry && !s.poll)
        {
            return Ok(s.label);
        }
        let label = self.p.label();
        self.stubs.push(Stub {
            snap,
            entry,
            poll: false,
            label,
        });
        Ok(label)
    }

    /// P-L: the label of the poll-exit stub of `snap`: a generic stub whose exit number carries `exit::EXIT_POLL`. The
    /// handler restores the snapshot, does not count the exit, clears the poll word and hands control to the
    /// interpreter at the snapshot's pc (the snapshot is a head: it lands at a pre-count label).
    /// The poll itself (P-L): `Mov V7, &TRACE_POLL; BrM Ne B1 [V7] 0 -> x` (x: the poll-exit stub).
    fn poll_ops(&mut self, x: L) {
        if let Some(off) = self.env.poll_off {
            // P2-3b: the poll word is addressed from `r15`
            let off = if mutants::active("poll_r15_disp_off") {
                off + 1
            } else {
                off
            };
            self.push(Op::BrM {
                cc: Cc::Ne,
                w: W::B1,
                m: M {
                    base: Base::Cnt,
                    off,
                },
                imm: 0,
                to: x,
            });
            return;
        }
        self.push(Op::Mov(7, self.env.interrupt));
        self.push(Op::BrM {
            cc: Cc::Ne,
            w: W::B1,
            m: rm(7, 0),
            imm: 0,
            to: x,
        });
    }

    fn poll_stub_label(&mut self, snap: u32) -> Result<L, AsmError> {
        if (snap as usize) >= self.snaps.len() {
            return incons(format!("snapshot {snap} has no exit"));
        }
        if self.ir.snaps[snap as usize].land != Land::Pre {
            // (never the case for a real trace: the snapshot of a poll is a loop head. The mutant `land_pre_mid_swap` makes
            // it so; the ordinary stub then lands where the snapshot says, as before P-L, and the mutant stays visible)
            return self.stub_label(snap, false);
        }
        if let Some(s) = self.stubs.iter().find(|s| s.snap == snap && s.poll) {
            return Ok(s.label);
        }
        let label = self.p.label();
        self.stubs.push(Stub {
            snap,
            entry: false,
            poll: true,
            label,
        });
        Ok(label)
    }

    // ---- value stores (writeback, array, upvalue) ---------------------------------------------------------------------

    /// Stores the payload of value `r` located at `loc` to `m` (base register must not be `V7` when the value needs a
    /// 64-bit constant: two 32-bit halves are stored then).
    fn store_payload(&mut self, m: M, loc: Loc) {
        match loc {
            Loc::G(n) => self.st(W::B8, m, Src::V(n)),
            Loc::X(n) => self.push(Op::StF { m, src: n }),
            Loc::Const(c) => {
                let b = self.kbits_of(c);
                self.store_const(m, b);
            }
            Loc::Spill(s) => {
                self.ld(7, W::B8, spill_m(s));
                self.st(W::B8, m, Src::V(7));
            }
            Loc::None => {
                // nil / false / true: payload 0 (a value with no location, see `exit::build_exits`)
                self.st(W::B8, m, Src::Imm(0));
            }
        }
    }

    fn store_const(&mut self, m: M, b: u64) {
        match fits(b as i64) {
            Some(i) => self.st(W::B8, m, Src::Imm(i)),
            None => {
                self.st(W::B4, m, Src::Imm(b as u32 as i32));
                let m2 = M {
                    base: m.base,
                    off: m.off + 4,
                };
                self.st(W::B4, m2, Src::Imm((b >> 32) as u32 as i32));
            }
        }
    }

    /// Writes slot `slot` of the frame: tag word and payload (the writeback, same shape as an exit).
    fn store_slot(&mut self, slot: u16, r: Ref, loc: Loc) -> Result<(), AsmError> {
        let t = self.ir.ty_of(r);
        let tag = match (TraceIr::is_const(r), self.ir.kval_opt(r)) {
            (true, Some(KVal::Ptr(_, PtrKind::LClosure))) => ctb(LUA_VLCL),
            (true, Some(KVal::Ptr(_, PtrKind::Builtin))) => LUA_VLCF,
            _ => match tag_of(t) {
                Some(g) => g,
                None => return unsup(format!("writeback of a value of type {t:?}")),
            },
        };
        self.st(W::B8, fm(slot, 0), Src::Imm(i32::from(tag)));
        match loc {
            Loc::Const(c) => {
                let b = self.kbits_of(c);
                self.store_const(fm(slot, 8), b);
            }
            Loc::None => self.st(W::B8, fm(slot, 8), Src::Imm(0)),
            l => self.store_payload(fm(slot, 8), l),
        }
        Ok(())
    }

    // ---- moves -------------------------------------------------------------------------------------------------------

    fn emit_move(&mut self, m: Move) {
        match (m.dst, m.src) {
            (Loc::G(d), Loc::G(s)) => self.push(Op::MovR(d, s)),
            (Loc::G(d), Loc::Spill(s)) => self.ld(d, W::B8, spill_m(s)),
            (Loc::G(d), Loc::Const(c)) => {
                let b = self.kbits_of(c);
                self.push(Op::Mov(d, b));
            }
            (Loc::X(d), Loc::X(s)) => self.push(Op::MovF(d, s)),
            // cycle breaking through V7 (ra::sequentialise)
            (Loc::G(d), Loc::X(s)) => self.push(Op::FtoG(d, s)),
            (Loc::X(d), Loc::G(s)) => self.push(Op::GtoF(d, s)),
            (Loc::X(d), Loc::Spill(s)) => self.push(Op::LdF {
                dst: d,
                m: spill_m(s),
            }),
            (Loc::X(d), Loc::Const(c)) => {
                let b = self.kbits_of(c);
                self.ldf_const(d, b);
            }
            (Loc::Spill(d), Loc::G(s)) => self.st(W::B8, spill_m(d), Src::V(s)),
            (Loc::Spill(d), Loc::X(s)) => self.push(Op::StF {
                m: spill_m(d),
                src: s,
            }),
            (Loc::Spill(d), Loc::Spill(s)) => {
                self.ld(7, W::B8, spill_m(s));
                self.st(W::B8, spill_m(d), Src::V(7));
            }
            (Loc::Spill(d), Loc::Const(c)) => {
                let b = self.kbits_of(c);
                self.push(Op::Mov(7, b));
                self.st(W::B8, spill_m(d), Src::V(7));
            }
            (d, s) => panic!("illegal move {d:?} <- {s:?}"),
        }
    }
}

/// `TraceIr::kval` that does not panic.
trait KvalOpt {
    fn kval_opt(&self, r: Ref) -> Option<KVal>;
}

impl KvalOpt for TraceIr {
    fn kval_opt(&self, r: Ref) -> Option<KVal> {
        (TraceIr::is_const(r) && usize::from(r) <= self.k.len()).then(|| self.k[usize::from(r) - 1])
    }
}

// ---- the assembler ------------------------------------------------------------------------------------------------

/// Code placement (Q2; docs/JIT_TRACE_REF.md "Q2 decisions"): the same trace ran 1.8 G or 2.55 G cycles depending only
/// on where it landed in the code arena. Measured on Zen 5: a loop head ON a 64-byte line start is +19% (geomean over
/// the 16 micro programs), any other 16-byte multiple is the fast case, so plain 32/64-byte alignment of the head is the
/// wrong fix. Trace blocks are placed [`LINE`]-aligned (`cache::install`) and the head is put [`HEAD_OFFSET`] bytes into
/// a line, which makes the position of every trace loop relative to the lines independent of where the block lands.
pub const LINE: u8 = 64;
/// Offset of the loop head inside a [`LINE`] (see there).
///
/// P-L: 16 -> 24. The single poll (3 instructions, 19 bytes instead of 30) moved the loop body 11 bytes towards the head
/// and `oop_methods` fell from 225 to 293 Mcyc (+30%, Zen 5; the same body, only its offset in the line changed). A sweep
/// of the head offset over 0..56 (`docs/bench-pl-headoffset.txt`) shows fast and slow zones that differ by up to 1.7x on
/// the micro loops (`int_sum` 985 vs 1650, `inline_leaf` 542 vs 867); 24 is in the fast zone of all six programs measured,
/// 16 only in five. Not every program is helped: `oop_override` (+13%, 52 K -> 831 K branch mispredictions in the same
/// instructions) is slow at every head offset with the short poll and fast with 4 or more bytes after the poll; those
/// 4 bytes (a nop or a 32-bit displacement) cost `inline_2` / `inline_leaf` 5-10%, so the layout stays as it is.
///
/// P2 fix: 24 -> 48. With the loop-top diet (`budsub` + `pollr15`: 10-12 instructions, 56 bytes for `lib/tonumber_num`) the
/// body is 9 bytes shorter again and 16 / 24 became the slow zone of that loop (259 Mcyc against 194 at 0, 205 at 32, 215 at
/// 48; the old code at the old offset: 230). Slow zones measured with the r15-addressed poll (`pollr15=1`): `field_global` at
/// 0 / 8 (+25 %), `type_entry` at 32 (+20 %), `sink_vec` at 8 / 40 (+24 %); with the `movabs` poll (the process default,
/// `POLLR15_PROCESS` = 0) `inline_leaf` at 32 / 40. 48 has none of them (docs/PERF.md "P2 loop-top placement fix").
pub const HEAD_OFFSET: u8 = 48;

/// Assembles `ir`. `al` is the allocation (`ra::allocate(ir, end_uses)`; `end_uses` = the refs of `writeback` in
/// order for the rolled layout, empty for the peeled one), `snaps` the resolved exits (`exit::build_exits`) and
/// `writeback` the slots the rolled iteration writes back (`Recorded::writeback`; ignored for the peeled layout).
pub fn assemble(
    ir: &TraceIr,
    al: &Allocation,
    snaps: &[ExitSnap],
    writeback: &[SnapEntry],
    env: &AsmEnv,
) -> Result<Asm, AsmError> {
    if al.ins.len() != ir.ins.len() || snaps.len() != ir.snaps.len() {
        return incons("allocation / exits do not match the IR");
    }
    if usize::from(al.nspill) + usize::from(EXT_SLOTS) > usize::from(ITER_SLOT)
        || usize::from(SCRATCH_SPILL) >= NSPILL
    {
        return Err(AsmError::Spill);
    }
    if let Some(r) = env.budget_reg {
        // V0..V4 are scratch of the entry / exit code, V6 of variable shifts, V7 of everything, V8 (`rdx`) of `SDivRem`
        if !exit::vb_valid(Loc::G(r)) || r != super::ra::BUDGET_GPR {
            return incons("the budget register must be ra::BUDGET_GPR");
        }
        if al.uses_gpr(r) {
            return incons("the allocation uses the budget register");
        }
    }
    let rolled = ir.loop_at.is_none();
    if env.side.is_some() && !rolled {
        return incons("a side trace is a rolled iteration");
    }
    if rolled && al.end_locs.len() != writeback.len() {
        return incons("end_locs do not match the writeback list");
    }
    if !rolled && al.backedge_after != ir.ins.len() {
        return unsup("back-edge moves before the last instruction");
    }
    let mut cx = Cx {
        ir,
        al,
        snaps,
        env,
        p: Program::default(),
        stubs: Vec::new(),
        disps: Vec::new(),
        first_flag: false,
        ext: al.nspill,
        saved: Vec::new(),
        nstubs: 0,
        pre_snaps: exit::pre_roll_snaps(ir),
        chain_snaps: if env.chain && env.side.is_some() && !mutants::active("chain_stub_never") {
            exit::guard_kinds(ir).iter().map(|g| g & 1 != 0).collect()
        } else {
            Vec::new()
        },
        sites: exit::site_classes(ir),
        mat_shared: Vec::new(),
        mat_frames: Vec::new(),
        mat_codes: Vec::new(),
        ktag: Vec::new(),
        hk_fused: None,
        hk_cold: Vec::new(),
        bud_cold: Vec::new(),
        hk_fused_n: 0,
        hk_split_n: 0,
        dbl: Vec::new(),
        lz_leave: [None, None],
    };
    cx.first_flag = rolled
        && env.head != 0
        && (0..ir.ins.len()).any(|i| cx.entry_kind(i) == EntryKind::FirstIter);
    let entry = cx.label();
    let loop_l = cx.label();
    let entry_fail = cx.label();
    cx.bind(entry);
    if env.seg_oracle && !ir.segs.is_empty() {
        cx.push(Op::Lea(
            7,
            M {
                base: Base::Ci,
                off: 0,
            },
        ));
        let m = spill_m(cx.ext_slot(ORACLE_SLOT_K));
        cx.st(W::B8, m, Src::V(7));
    }
    cx.entry_checks(entry_fail)?;
    let rec_fail = cx.label();
    let inner = (rolled
        && env.side.is_none()
        && env.head != 0
        && (ir.rec_call.is_some() || ir.rec_ret.is_some()))
    .then(|| cx.label());
    if let Some(inner) = inner {
        let past = cx.label();
        cx.push(Op::Jmp(past));
        cx.bind(inner);
        // P-M4: an inner entry books no entry (no `entry_ok` count, no `pending_before`), so it must not be taken for one by the
        // entry-check stubs: the first-iteration word is stale here (a continuation trace leaves it at 1 when it pushes
        // through the dispatch cell, and the exit area is shared by every trace), and a failing entry check would then undo an
        // `entry_ok` that was never counted (wrap to 2^32-1) and add the stale `pending_before` to `entry_fail`.
        if cx.first_flag && !mutants::active("rec_inner_first_stale") {
            let m = spill_m(cx.ext_slot(FIRST_SLOT_K));
            cx.st(W::B8, m, Src::Imm(0));
        }
        // The frame is a running Lua frame of this prototype entered by a native return or self-call: its own registers are
        // inside the stack (`ci.top <= stack_size`), and the frame it returns into has called before, so its `next` exists.
        // Only the room for the registers of an inlined or pushed callee is not implied.
        let own_only = ir.depth == 0
            && env.head_pc != 0
            && usize::from(ir.extent) <= usize::from(env.root_maxstack)
            && env.root_maxstack != 0;
        if own_only || mutants::active("rec_inner_room_skip") {
            // nothing to check
        } else if ir.depth <= 1 && env.root_maxstack != 0 && env.head_pc != 0 {
            // (a continuation trace: the frame has called before, so its `next` exists; the entry trace's inner entry
            // is reached by a fresh frame whose `next` may be null)
            cx.room_stack_check(rec_fail);
        } else {
            cx.room_checks(rec_fail);
        }
        cx.bind(past);
    }
    if rolled && env.side.is_none() {
        cx.head_pad();
        cx.bind(loop_l);
        cx.count_iteration();
    } else if rolled {
        cx.bind(loop_l); // a side trace has no loop; the label is never referenced
        cx.count_iteration(); // `+verify`: a side trace counts its executions like a loop counts iterations
    }
    for i in 0..ir.ins.len() {
        if ir.rec_call.is_some() && i + 1 == ir.ins.len() {
            // P-M3: the placeholder exit of a native self-call (an always-failing guard) assembles to nothing; the push
            // follows the writeback
            if !(ir.ins[i].op == TOp::Eq && ir.ins[i].flags & F_GUARD != 0) {
                return incons("a native self-call does not end in its placeholder guard");
            }
            continue;
        }
        if ir.rec_ret.is_some() && i + 1 == ir.ins.len() {
            let Some(_) = ir.rec_ret else { unreachable!() };
            if !(ir.ins[i].op == TOp::Eq && ir.ins[i].flags & F_GUARD != 0) {
                return incons("a native return does not end in its placeholder guard");
            }
            continue;
        }
        if env.side_room && ir.ins[i].op == TOp::Budget && !mutants::active("side_room_check_skip")
        {
            // R2: the room checks of a frameful side trace that needs more than its parent guarantees: in front of the first budget
            // charge, behind the loads of the frames' function slots (the start snapshot restores them), failing through that snapshot
            // (V0 and V1 may hold the loaded function slots the snapshot restores: they are saved around the checks)
            let x = cx.guard_label(i)?;
            let (m0, m1) = (spill_m(cx.ext_slot(2)), spill_m(cx.ext_slot(3)));
            cx.st(W::B8, m0, Src::V(0));
            cx.st(W::B8, m1, Src::V(1));
            let (fail, done) = (cx.label(), cx.label());
            cx.room_checks(fail);
            cx.ld(0, W::B8, m0);
            cx.ld(1, W::B8, m1);
            cx.push(Op::Jmp(done));
            cx.bind(fail);
            cx.ld(0, W::B8, m0);
            cx.ld(1, W::B8, m1);
            cx.push(Op::Jmp(x));
            cx.bind(done);
        }
        if let (Some(rr), TOp::Budget) = (ir.rec_ret, ir.ins[i].op) {
            // (the guards depend on the frame only, which nothing in the trace changes: they sit in front of the first charge
            // and fail to the state at the start of the trace, where every register is free)
            cx.rec_ret_guards(i, rr)?;
        }
        cx.pre(i);
        cx.ins(i, loop_l)?;
        cx.post(i);
        cx.track_tags(i);
    }
    if rolled {
        if !mutants::active("rolled_no_writeback") {
            for (k, e) in writeback.iter().enumerate() {
                cx.store_slot(e.slot, e.r, al.end_locs[k])?;
            }
        }
    } else {
        for m in &al.backedge_seq {
            cx.emit_move(*m);
        }
    }
    if ir.rec_ret.is_some() {
        if ir.rec_call.is_some() || !rolled {
            return incons("a native return ends a rolled trace and not a native self-call");
        }
        cx.rec_ret_tail(ir.rec_ret.map_or(1, |r| r.n))?;
    } else if let Some(rc) = ir.rec_call {
        if (env.side.is_some() && env.rec_link_cell == 0) || !rolled {
            return incons("a native self-call ends a rolled trace");
        }
        cx.rec_call_tail(rc, loop_l, rec_fail)?;
    } else if let Some(sd) = env.side {
        // (P-E2: `link_cell == 0`: the trace ended with its always-taken exit, there is nothing to link to)
        if sd.link_cell != 0 {
            cx.emit_link(sd)?;
        }
    } else {
        if cx.first_flag {
            let m = spill_m(cx.ext_slot(FIRST_SLOT_K));
            cx.st(W::B8, m, Src::Imm(0));
        }
        cx.push(Op::Jmp(loop_l));
    }
    cx.stubs_and_tail(
        entry_fail,
        (ir.rec_call.is_some() || ir.rec_ret.is_some()).then_some(rec_fail),
    )?;
    // P2 bundle 1 (`xregs`): `V10` is `r14`, the constants pointer of tier 1. A trace that allocated it leaves through a `JmpR` (every exit stub,
    // link and tail) with `r14` clobbered: reload it from `ci.kcache` (`r13` is the frame the code jumps into at every `JmpR`: the root
    // frame for a frameless exit, the rebuilt one after `SetCi`) in front of each jump. `Ret` needs nothing (the thunk pops the saved `r14`).
    // `V9` is `rbp`: nothing but the thunk's frame-pointer chain reads it, and the thunk restores it on `Ret`.
    if al.uses_gpr(10)
        && (ir.rec_call.is_some() || ir.rec_ret.is_some())
        && !mutants::active("xregs_rec_r14")
    {
        return incons("V10 (r14) allocated in a trace with a native frame push / pop");
    }
    if al.uses_gpr(10) && !mutants::active("xregs_no_reloadk") {
        let old = std::mem::take(&mut cx.p.ops);
        for o in old {
            // (`JmpR V3` of the shared exit routine and `JmpR V6` of the frame-rebuilding stubs follow their own `SetK`)
            if o == Op::JmpR(7) {
                cx.p.ops.push(Op::ReloadK);
            }
            cx.p.ops.push(o);
        }
    }
    let stubs = cx.nstubs;
    let hk_fused = cx.hk_fused_n;
    let hk_split = cx.hk_split_n;
    let dbl = std::mem::take(&mut cx.dbl);
    Ok(Asm {
        prog: cx.p,
        entry,
        loop_label: loop_l,
        stubs,
        hk_fused,
        hk_split,
        dbl,
        inner,
    })
}

impl Cx<'_> {
    /// B2 (`b2tag`): the update of the known node tags after instruction `i` (see [`Cx::ktag`]). Ops that cannot write a hash node's
    /// tag byte or move a table's nodes keep the facts; a store to another address may alias (two tables can be the same at run
    /// time), so it replaces them all; anything else (helpers, allocation, the loop marker, where two paths meet) forgets them.
    fn track_tags(&mut self, i: usize) {
        if !self.env.b2_tag {
            return;
        }
        let ins = self.ir.ins[i];
        match ins.op {
            TOp::Hload => {
                let bits = aux_bits(ins.aux);
                if bits & 1 != 0 && bits & 2 == 0 && ins.flags & F_ANYSTR == 0 && ins.t != Ty::Nil {
                    if let Some(tag) = tag_of(ins.t) {
                        if !self.ktag.iter().any(|&(r, _)| r == ins.a) {
                            self.ktag.push((ins.a, tag));
                        }
                    }
                }
            }
            TOp::Hstore => {
                if !mutants::active("b2tag_alias_keep") {
                    // two `HREFK`s of the same node vector with different constant keys address different nodes; anything else (other
                    // tables may be identical at run time, `HREFV`) may alias
                    let me = ins.a;
                    let mut keep = Vec::new();
                    for &(r, t) in &self.ktag {
                        let (ix, iy) = (self.ir.at(r), self.ir.at(me));
                        if r != me
                            && ix.op == TOp::Hrefk
                            && iy.op == TOp::Hrefk
                            && ix.a == iy.a
                            && self.kbits_of(ix.b) != self.kbits_of(iy.b)
                        {
                            keep.push((r, t));
                        }
                    }
                    self.ktag = keep;
                } else {
                    self.ktag.retain(|&(r, _)| r != ins.a);
                }
                if let Some(tag) = tag_of(self.ir.ty_of(ins.b)) {
                    self.ktag.push((ins.a, tag));
                }
            }
            TOp::Loop => {
                // the loop top is reached from the pre-roll and from the back-edge: a fact holds there only if the body (starting
                // from it) still has it at its end (induction over the iterations)
                let f0 = self.ktag.clone();
                for j in i + 1..self.ir.ins.len() {
                    self.track_tags(j);
                }
                let stable = f0.iter().all(|f| self.ktag.contains(f));
                self.ktag = if stable { f0 } else { Vec::new() };
            }
            TOp::Sload
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
            | TOp::Astore
            | TOp::Uref
            | TOp::Uload
            | TOp::Ustore
            | TOp::Poll
            | TOp::Budget
            | TOp::Phi
            | TOp::Hrefk
            | TOp::Hrefv
            | TOp::Hmiss
            | TOp::Kint
            | TOp::Knum
            | TOp::Kpri
            | TOp::Kptr => {}
            _ => self.ktag.clear(),
        }
    }

    /// The end of a side trace (7.8): the writeback has made the stack hold the whole state at the head, so the poll of
    /// the link (TR22) exits through the empty map at the head; then the budget register goes back to the budget word
    /// (it holds tier 1's state at the head's pre-count label, see the count model of F2a) and the trace continues at
    /// whatever the head's dispatch cell says: the entry of its root trace (which re-checks its types), tier 1's
    /// counting stub, or `pcl[H]`. Nothing here refers to the code of another trace, so a flush of the target needs no
    /// patching (TR8).
    fn emit_link(&mut self, sd: SideEnv) -> Result<(), AsmError> {
        if !mutants::active("side_link_no_poll") {
            let x = self.poll_stub_label(sd.link_snap)?;
            self.poll_ops(x);
        }
        if let Some(r) = self.env.budget_reg {
            if !mutants::active("side_link_no_store") {
                self.st(
                    W::B4,
                    M {
                        base: Base::Cnt,
                        off: 0,
                    },
                    Src::V(r),
                );
            }
        }
        if self.env.lazy {
            // BUNDLE-K L1: the head the link continues at may be tier 1 or a trace that is not lazy-aware
            self.push(Op::Mov(7, sd.link_cell));
            self.ld(6, W::B8, rm(7, 0));
            let l = self.lz_leave_label(false);
            self.push(Op::Jmp(l));
            return Ok(());
        }
        self.push(Op::Mov(7, sd.link_cell));
        self.ld(7, W::B8, rm(7, 0));
        self.push(Op::JmpR(7));
        Ok(())
    }

    fn count_iteration(&mut self) {
        if self.env.count_iterations {
            self.ld(7, W::B8, spill_m(ITER_SLOT));
            self.push(Op::Add(7, Src::Imm(1)));
            self.st(W::B8, spill_m(ITER_SLOT), Src::V(7));
        }
    }

    // ---- entry (7.7) -----------------------------------------------------------------------------------------------

    /// P-M3: the native self-call that ends a function-entry trace, after the writeback (the stack holds the whole state, the
    /// function and its arguments included): the frame of the callee is pushed exactly as tier 1's flat `CALL` pushes it
    /// (`calls.rs`: `top`, `savedpc` of the caller, `CallInfo` fields, `thread.ci`, the pinned registers), then the room
    /// checks (TR6a / TR6b) of the new frame run (a failure continues in tier 1 at `pcl[H]` of the new frame, a legal
    /// fresh frame) and control continues at the loop top. Preconditions of the push (stack room for the callee's frame, a
    /// spare `CallInfo`) are the entry checks of the frame that executes the call; the poll and the budget run at the loop
    /// top of the new frame.
    fn rec_call_tail(&mut self, rc: RecCall, loop_l: L, rec_fail: L) -> Result<(), AsmError> {
        let ciw = |o: usize| M {
            base: Base::Ci,
            off: o as i32,
        };
        let thw = |o: usize| M {
            base: Base::Thread,
            off: o as i32,
        };
        if self.env.lazy {
            return self.rec_call_tail_lazy(rc, loop_l, rec_fail);
        }
        if mutants::active("rec_push_no_ci_next") {
            // (mutant: reuse the running CallInfo instead of its successor)
            self.push(Op::Lea(1, ciw(0)));
        } else {
            self.ld(1, W::B8, ciw(ci::NEXT));
        }
        self.ld(2, W::B8, ciw(ci::FUNC));
        self.push(Op::Add(2, Src::Imm(i32::from(rc.a) + 1))); // callee function index
        self.st(W::B8, rm(1, ci::FUNC as i32), Src::V(2));
        let top_extra = if mutants::active("rec_push_top_off") {
            0
        } else {
            1
        };
        self.push(Op::Lea(3, rm(2, i32::from(rc.maxstack) + top_extra)));
        self.st(W::B8, rm(1, ci::TOP as i32), Src::V(3)); // func + 1 + maxstacksize
        self.push(Op::Lea(3, rm(2, i32::from(rc.nargs) + 1)));
        self.st(W::B8, thw(thr::TOP), Src::V(3)); // end of the arguments
        let cs = if mutants::active("rec_push_callstatus_off") {
            rc.c - 1
        } else {
            rc.c
        };
        self.st(W::B4, rm(1, ci::CALLSTATUS as i32), Src::Imm(cs as i32));
        self.st(W::B4, rm(1, ci::TRAP as i32), Src::Imm(0));
        self.push(Op::Mov(3, rc.code));
        self.st(W::B8, rm(1, ci::SAVEDPC as i32), Src::V(3));
        self.push(Op::Lea(
            3,
            M {
                base: Base::K,
                off: 0,
            },
        ));
        self.st(W::B8, rm(1, ci::KCACHE as i32), Src::V(3));
        let after = if mutants::active("rec_push_savedpc_off") {
            rc.after + 4
        } else {
            rc.after
        };
        self.push(Op::Mov(3, after));
        self.st(W::B8, ciw(ci::SAVEDPC), Src::V(3)); // the caller continues after its CALL
        self.st(W::B8, thw(thr::CI), Src::V(1));
        self.push(Op::Lea(
            4,
            M {
                base: Base::Frame,
                off: (i32::from(rc.a) + 1) * 16,
            },
        ));
        self.push(Op::SetCi(1));
        self.push(Op::SetFrame(4));
        if self.env.rec_link_cell != 0 {
            // a continuation trace: the new frame enters the function-entry head through its dispatch cell (which is the
            // function-entry trace, its counting stub or tier 1: whatever a flat call would reach)
            if let Some(r) = self.env.budget_reg {
                self.st(
                    W::B4,
                    M {
                        base: Base::Cnt,
                        off: 0,
                    },
                    Src::V(r),
                );
            }
            self.push(Op::Mov(7, self.env.rec_link_cell));
            self.ld(7, W::B8, rm(7, 0));
            self.push(Op::JmpR(7));
            return Ok(());
        }
        if self.env.fibdiet & 1 != 0 {
            // BUNDLE-F `fibdiet` bit 0: TR6a/TR6b of the new frame from the register that holds its function index (V2): the
            // reload of the `ci.func` just stored (a store-forwarding chain on the hottest path) is gone. Same value, same limit.
            self.room_stack_check_from(2, rec_fail);
            self.room_ci_check(rec_fail);
        } else {
            self.room_checks(rec_fail);
        }
        if self.first_flag {
            let m = spill_m(self.ext_slot(FIRST_SLOT_K));
            self.st(W::B8, m, Src::Imm(0));
        }
        self.push(Op::Jmp(loop_l));
        Ok(())
    }

    /// BUNDLE-K L1: the lazy push. Only the callee's `func` and the caller's `savedpc` (the continuation, which also tells the native return
    /// where the frame returns to) are stored; `top`, `callstatus`, `trap`, `kcache`, the callee's `savedpc`, `thread.top` and `thread.ci` are
    /// completed by `exit::lazy_leave` on every path that leaves the region. The registers (`r13`, the frame base) switch as in the eager push.
    fn rec_call_tail_lazy(&mut self, rc: RecCall, loop_l: L, rec_fail: L) -> Result<(), AsmError> {
        let ciw = |o: usize| M {
            base: Base::Ci,
            off: o as i32,
        };
        if mutants::active("rec_push_no_ci_next") {
            self.push(Op::Lea(1, ciw(0)));
        } else {
            self.ld(1, W::B8, ciw(ci::NEXT));
        }
        self.ld(2, W::B8, ciw(ci::FUNC));
        self.push(Op::Add(2, Src::Imm(i32::from(rc.a) + 1))); // callee function index
        self.st(W::B8, rm(1, ci::FUNC as i32), Src::V(2));
        let after = if mutants::active("rec_push_savedpc_off") {
            rc.after + 4
        } else {
            rc.after
        };
        self.push(Op::Mov(3, after));
        self.st(W::B8, ciw(ci::SAVEDPC), Src::V(3)); // the caller continues after its CALL
        self.push(Op::Lea(
            4,
            M {
                base: Base::Frame,
                off: (i32::from(rc.a) + 1) * 16,
            },
        ));
        self.push(Op::SetCi(1));
        self.push(Op::SetFrame(4));
        if self.env.rec_link_cell != 0 {
            if let Some(r) = self.env.budget_reg {
                self.st(
                    W::B4,
                    M {
                        base: Base::Cnt,
                        off: 0,
                    },
                    Src::V(r),
                );
            }
            self.lazy_rec_jump(self.env.rec_link_cell, true);
            return Ok(());
        }
        self.room_stack_check_from(2, rec_fail);
        self.room_ci_check(rec_fail);
        if self.first_flag {
            let m = spill_m(self.ext_slot(FIRST_SLOT_K));
            self.st(W::B8, m, Src::Imm(0));
        }
        self.push(Op::Jmp(loop_l));
        Ok(())
    }

    /// BUNDLE-K L1: the jump of a native push / return through the `rec` cell at `rec_cell` (a `HeadData::rec`): the inner entry of a
    /// lazy-aware trace, or 0 while the head is not entered directly: the chain is then completed and the head's plain dispatch cell used.
    fn lazy_rec_jump(&mut self, rec_cell: u64, new_frame: bool) {
        let fb = self.label();
        self.push(Op::Mov(7, rec_cell));
        self.ld(7, W::B8, rm(7, 0));
        if !mutants::active("lazy_rec_cell_nozero") {
            self.br(Cc::Eq, 7, Src::Imm(0), fb);
        }
        self.push(Op::JmpR(7));
        self.bind(fb);
        let plain = rec_cell - head_off::REC as u64 + head_off::CELL as u64;
        self.push(Op::Mov(7, plain));
        self.ld(6, W::B8, rm(7, 0));
        let l = self.lz_leave_label(new_frame);
        self.push(Op::Jmp(l));
    }

    /// P-M3: the guards of the native `RETURN1` (in front of the budget charge `i` of the start of the trace; failures
    /// exit through that instruction's snapshot, the state at the start): the frame was called for exactly one result, and its caller is suspended at a continuation head.
    fn rec_ret_guards(&mut self, i: usize, _rr: RecRet) -> Result<(), AsmError> {
        let x = self.guard_label(i)?;
        let ciw = |o: usize| M {
            base: Base::Ci,
            off: o as i32,
        };
        if self.env.rec_nsites == 0 {
            return incons("a native return without continuation heads");
        }
        // BUNDLE-E: the guards use V1..V3 as scratch. At the start of a root trace every register is free, but a frameful side trace
        // (S1) loads the function slots of its virtual frames *before* its first budget charge, and the start snapshot that the
        // failing guards exit through restores those slots from the registers: clobbered, they write `ci.previous` (a `CallInfo`
        // address) into a function slot. Registers a value of an earlier instruction was computed into are saved around the guards.
        let mut saved: Vec<(V, u16)> = Vec::new();
        for (k, r) in [1u8, 2, 3].into_iter().enumerate() {
            let live = self.al.ins[..i].iter().any(|a| a.dst == Loc::G(r));
            if live && !mutants::active("rec_ret_regs_unsaved") {
                let slot = self.ext_slot(4 + k as u16);
                self.st(W::B8, spill_m(slot), Src::V(r));
                saved.push((r, slot));
            }
        }
        // (without saved registers the guards fail straight to the exit; with them, through a trampoline that restores them)
        let fail = if saved.is_empty() { x } else { self.label() };
        self.ld(1, W::B8, ciw(ci::PREVIOUS));
        if self.env.lazy {
            // BUNDLE-K L1: a frame above the region root was pushed natively by a self-call (never `CIST_FRESH`, its caller a Lua frame): no
            // callstatus tests, and its fields are not complete anyway. The root frame itself (complete, maybe called from C) gets the eager tests.
            let lazy_ok = self.label();
            self.push(Op::Lea(3, ciw(0)));
            if mutants::active("lazy_ret_root_unchecked") {
                // (mutant: the root frame takes the lazy path, without the callstatus tests)
                self.push(Op::Jmp(lazy_ok));
            } else {
                self.push(Op::BrMR {
                    cc: Cc::Ne,
                    m: Self::lroot_m(),
                    src: 3,
                    to: lazy_ok,
                });
            }
            self.ld(3, W::B4, ciw(ci::CALLSTATUS));
            self.push(Op::And(3, Src::Imm(ci::CIST_FRESH as i32)));
            self.br(Cc::Ne, 3, Src::Imm(0), fail);
            self.ld(3, W::B4, rm(1, ci::CALLSTATUS as i32));
            self.push(Op::And(3, Src::Imm(ci::CIST_C as i32)));
            self.br(Cc::Ne, 3, Src::Imm(0), fail);
            self.bind(lazy_ok);
        } else if !mutants::active("rec_ret_cframe_skip") {
            // R2 (a latent P-M3 hole): the returning frame must not be `CIST_FRESH` (called by `ccall`: its return leaves `luaV_execute`)
            // and its caller must be a Lua frame: the `savedpc` of a C frame is not maintained (a reused `CallInfo` node keeps the one of
            // the Lua frame it held before, which can be exactly a continuation site: `pcall(fib, n)` after deep `fib` calls)
            self.ld(3, W::B4, ciw(ci::CALLSTATUS));
            self.push(Op::And(3, Src::Imm(ci::CIST_FRESH as i32)));
            self.br(Cc::Ne, 3, Src::Imm(0), fail);
            self.ld(3, W::B4, rm(1, ci::CALLSTATUS as i32));
            self.push(Op::And(3, Src::Imm(ci::CIST_C as i32)));
            self.br(Cc::Ne, 3, Src::Imm(0), fail);
        }
        self.ld(2, W::B8, rm(1, ci::SAVEDPC as i32));
        if !mutants::active("rec_ret_site_unchecked") {
            let ok = self.label();
            for k in 0..usize::from(self.env.rec_nsites) {
                self.push(Op::Mov(3, self.env.rec_sites[k].0));
                self.br(Cc::Eq, 2, Src::V(3), ok);
            }
            self.push(Op::Jmp(fail));
            self.bind(ok);
        }
        if !saved.is_empty() {
            let done = self.label();
            for &(r, slot) in &saved {
                self.ld(r, W::B8, spill_m(slot));
            }
            self.push(Op::Jmp(done));
            self.bind(fail);
            for &(r, slot) in &saved {
                self.ld(r, W::B8, spill_m(slot));
            }
            self.push(Op::Jmp(x));
            self.bind(done);
        }
        Ok(())
    }

    /// P-M3: the native `RETURN1` after the result was stored into the function slot: pops the frame as tier 1's
    /// `RETURN1` does (`thread.top = func + 1`, `thread.ci = previous`, the pinned registers of the caller) and continues at
    /// the dispatch cell of the continuation head the caller is suspended at.
    fn rec_ret_tail(&mut self, n: u8) -> Result<(), AsmError> {
        let ciw = |o: usize| M {
            base: Base::Ci,
            off: o as i32,
        };
        let thw = |o: usize| M {
            base: Base::Thread,
            off: o as i32,
        };
        let lazy = self.env.lazy;
        self.ld(1, W::B8, ciw(ci::PREVIOUS));
        self.ld(2, W::B8, rm(1, ci::SAVEDPC as i32));
        if !lazy {
            self.ld(3, W::B8, ciw(ci::FUNC));
            let top_n = if mutants::active("rec_ret_top_off") {
                i32::from(n) + 1
            } else {
                i32::from(n)
            };
            self.push(Op::Add(3, Src::Imm(top_n)));
            self.st(W::B8, thw(thr::TOP), Src::V(3));
            self.st(W::B8, thw(thr::CI), Src::V(1));
        } else {
            // the root frame returns into its (complete) caller, which becomes the root: the thread follows it as in the eager protocol
            let lz = self.label();
            self.push(Op::Lea(4, ciw(0)));
            self.push(Op::BrMR {
                cc: Cc::Ne,
                m: Self::lroot_m(),
                src: 4,
                to: lz,
            });
            self.ld(3, W::B8, ciw(ci::FUNC));
            self.push(Op::Add(3, Src::Imm(i32::from(n))));
            self.st(W::B8, thw(thr::TOP), Src::V(3));
            self.st(W::B8, thw(thr::CI), Src::V(1));
            if !mutants::active("lazy_ret_root_stale") {
                self.st(W::B8, Self::lroot_m(), Src::V(1));
            }
            self.bind(lz);
        }
        self.push(Op::SetCi(1));
        if let Some(r) = self.env.budget_reg {
            self.st(
                W::B4,
                M {
                    base: Base::Cnt,
                    off: 0,
                },
                Src::V(r),
            );
        }
        let n = usize::from(self.env.rec_nsites);
        for k in 0..n {
            let (site, cell, a) = self.env.rec_sites[k];
            let next = self.label();
            if k + 1 < n {
                self.push(Op::Mov(3, site));
                self.br(Cc::Ne, 2, Src::V(3), next);
            }
            // the caller's frame base: the returning frame sits `A + 1` slots above it (its function slot is the caller's
            // register `A` of the `CALL` the caller is suspended after)
            let up = if mutants::active("rec_ret_base_off") {
                i32::from(a) + 2
            } else {
                i32::from(a) + 1
            };
            self.push(Op::Lea(
                4,
                M {
                    base: Base::Frame,
                    off: -up * 16,
                },
            ));
            self.push(Op::SetFrame(4));
            if lazy {
                self.lazy_rec_jump(cell, false);
            } else {
                self.push(Op::Mov(7, cell));
                self.ld(7, W::B8, rm(7, 0));
                self.push(Op::JmpR(7));
            }
            self.bind(next);
        }
        Ok(())
    }

    /// N2b: `RETF` (see [`TOp::Retf`]): the guards of the caller's identity (exits through the snapshot of the `RETURN`, with the registers
    /// and frame of the returning frame intact), then the native pop: `thread.ci = r13 = C`, `rbx -= 16 * (A + 1)`, `r14 = C.kcache`.
    /// Scratch: `V1` (`C`), `V2`, `V7`; the allocator excludes `V1` / `V2` from the trace (`ra::op_clobbers`).
    fn retf(&mut self, i: usize) -> Result<(), AsmError> {
        let ins = self.ir.ins[i];
        if self.env.lazy
            || self.ir.loop_at.is_some()
            || self.ir.rec_call.is_some()
            || self.ir.rec_ret.is_some()
        {
            return incons("RETF in a lazy, looping or native-push/pop trace");
        }
        if self.al.uses_gpr(1) || self.al.uses_gpr(2) || self.al.uses_gpr(10) {
            return incons("RETF in a trace that allocated V1 / V2 / V10");
        }
        let x = self.guard_label(i)?;
        let ciw = |o: usize| M {
            base: Base::Ci,
            off: o as i32,
        };
        let thw = |o: usize| M {
            base: Base::Thread,
            off: o as i32,
        };
        let bits = aux_bits(ins.aux);
        self.ld(1, W::B8, ciw(ci::PREVIOUS));
        if !mutants::active("retf_callstatus_unchecked") {
            // the returning frame: called for `nresults + 1` results by a Lua instruction of this very interpreter loop (no `CIST_FRESH`,
            // no run-in-place metamethod, no hook / close state); `CIST_TAIL` and `CIST_JIT` are not compared
            self.ld(2, W::B4, ciw(ci::CALLSTATUS));
            self.push(Op::And(2, Src::Imm(RETF_CS_MASK as i32)));
            self.br(Cc::Ne, 2, Src::Imm((bits & 0xFF) as i32), x);
        }
        if !mutants::active("retf_cframe_unchecked") {
            // (the `savedpc` of a C frame is not maintained: a reused node keeps the one of the Lua frame it held before)
            self.ld(2, W::B4, rm(1, ci::CALLSTATUS as i32));
            self.push(Op::And(2, Src::Imm(ci::CIST_C as i32)));
            self.br(Cc::Ne, 2, Src::Imm(0), x);
        }
        if !mutants::active("retf_site_unchecked") {
            // the caller is suspended after the `CALL` the trace recorded (its proto is the constant's, a trace dependency)
            self.ld(2, W::B8, rm(1, ci::SAVEDPC as i32));
            let site = self.kbits_of(ins.a);
            self.push(Op::Mov(7, site));
            self.br(Cc::Ne, 2, Src::V(7), x);
        }
        if bits & RETF_K != 0 && !mutants::active("retf_k_unchecked") {
            // `luaF_close` must be a no-op: no registered to-be-closed variable, no open upvalue at or above the frame base (as tier 1's RETURN)
            self.ld(2, W::B8, thw(thr::TBC_LEN));
            self.br(Cc::Ne, 2, Src::Imm(0), x);
            let nc = self.label();
            self.ld(2, W::B8, thw(thr::OPENUPVAL));
            self.br(Cc::Eq, 2, Src::Imm(0), nc);
            self.ld(2, W::B8, rm(2, rn_core::jit_layout::upval::V as i32));
            self.push(Op::Lea(
                7,
                M {
                    base: Base::Frame,
                    off: 0,
                },
            ));
            self.br(Cc::Ge, 2, Src::V(7), x);
            self.bind(nc);
        }
        // ---- the pop ----
        if !mutants::active("retf_no_ci_store") {
            self.st(W::B8, thw(thr::CI), Src::V(1)); // (eager protocol: `thread.ci == r13` for helpers and exits)
        }
        if !mutants::active("retf_ci_stays") {
            self.push(Op::SetCi(1));
        }
        let up = if mutants::active("retf_shift_off") {
            i32::from(ins.b) + 1
        } else {
            i32::from(ins.b)
        };
        self.push(Op::Lea(
            2,
            M {
                base: Base::Frame,
                off: -up * 16,
            },
        ));
        self.push(Op::SetFrame(2));
        if !mutants::active("retf_no_reloadk") {
            self.ld(2, W::B8, ciw(ci::KCACHE)); // (r13 = C now)
            self.push(Op::SetK(2));
        }
        Ok(())
    }

    /// TR6a / TR6b: stack room and spare `CallInfo` nodes of the running frame (failures jump to `fail`; `V0`, `V1`).
    fn room_checks(&mut self, fail: L) {
        self.room_stack_check(fail);
        self.room_ci_check(fail);
    }

    /// TR6a only.
    fn room_stack_check(&mut self, fail: L) {
        let ir = self.ir;
        // TR6a: stack room, strict: stack_size - (ci.func + 1 + extent) > 0
        if !mutants::active("entry_no_extent") {
            self.ld(
                0,
                W::B8,
                M {
                    base: Base::Thread,
                    off: thr::STACK_SIZE as i32,
                },
            );
            self.ld(
                1,
                W::B8,
                M {
                    base: Base::Ci,
                    off: ci::FUNC as i32,
                },
            );
            self.push(Op::Add(1, Src::Imm(i32::from(ir.extent) + 1)));
            self.br(Cc::Ge, 1, Src::V(0), fail);
        }
    }

    /// TR6a with the function index of the running frame in register `f` (`f` is `ci.func`; V0, V1 are scratch).
    fn room_stack_check_from(&mut self, f: V, fail: L) {
        let ir = self.ir;
        if !mutants::active("entry_no_extent") {
            let off =
                i32::from(ir.extent) + 1 - 64 * i32::from(mutants::active("fd_room_extent_off"));
            self.ld(
                0,
                W::B8,
                M {
                    base: Base::Thread,
                    off: thr::STACK_SIZE as i32,
                },
            );
            self.push(Op::Lea(1, rm(f, off)));
            self.br(Cc::Ge, 1, Src::V(0), fail);
        }
    }

    /// TR6b only.
    fn room_ci_check(&mut self, fail: L) {
        let ir = self.ir;
        // TR6b: `depth` spare CallInfo nodes behind ci (an intrinsic at depth 4 makes it 5)
        if !mutants::active("entry_no_ci_spare") && ir.depth > 0 {
            let mut base = Base::Ci;
            for _ in 0..ir.depth {
                self.push(Op::Ld {
                    dst: 0,
                    w: W::B8,
                    m: M {
                        base,
                        off: ci::NEXT as i32,
                    },
                });
                self.br(Cc::Eq, 0, Src::Imm(0), fail);
                base = Base::Reg(0);
            }
        }
    }

    fn entry_checks(&mut self, fail: L) -> Result<(), AsmError> {
        if !self.env.no_room_checks {
            self.room_checks(fail);
        }
        // the budget word into its register (after the TR6 checks: their failures leave the word alone)
        if let Some(r) = self.env.budget_reg {
            self.ld(
                r,
                W::B4,
                M {
                    base: Base::Cnt,
                    off: 0,
                },
            );
        }
        // entry-ok bookkeeping (3.3): pending_before -> ext slot 0; pending = 0; entry_ok += 1 (saturating)
        if self.env.head != 0 {
            if self.first_flag {
                let m = spill_m(self.ext_slot(FIRST_SLOT_K));
                self.st(W::B8, m, Src::Imm(1));
            }
            self.push(Op::Mov(0, self.env.head));
            self.ld(1, W::B4, rm(0, head_off::PENDING as i32));
            self.st(W::B8, spill_m(self.ext_slot(0)), Src::V(1));
            self.st(W::B4, rm(0, head_off::PENDING as i32), Src::Imm(0));
            self.ld(1, W::B4, rm(0, head_off::ENTRY_OK as i32));
            // a u32 that wraps would reset the failure ratio: stop counting at ENTRY_OK_CAP (a zero-extended load,
            // so the signed compare sees the true value)
            let skip = self.label();
            self.br(Cc::Ge, 1, Src::Imm(super::cache::ENTRY_OK_CAP as i32), skip);
            self.push(Op::Add(1, Src::Imm(1)));
            self.st(W::B4, rm(0, head_off::ENTRY_OK as i32), Src::V(1));
            self.bind(skip);
        }
        if self.env.lazy
            && (self.env.side.is_none() || self.env.prelude)
            && !mutants::active("lazy_root_not_set")
        {
            // BUNDLE-K L1: this (complete) frame is the root of the region the trace may start
            self.push(Op::Lea(
                1,
                M {
                    base: Base::Ci,
                    off: 0,
                },
            ));
            self.st(W::B8, Self::lroot_m(), Src::V(1));
        }
        Ok(())
    }

    /// The failure bookkeeping of 3.3 (all of `V0..V4`, `V7` are free): `entry_fail += pending_before;
    /// pending = 1; [entry_ok -= 1]`. `undo`: the entry sequence had booked a successful entry (`pending_before` is in
    /// its extension slot). With `threshold` the `jit_tr_entry_fail` check follows (TR6 failures: nothing else runs
    /// afterwards).
    fn entry_failure_ops(&mut self, undo: bool, threshold: bool) {
        if self.env.head == 0 {
            return;
        }
        self.push(Op::Mov(1, self.env.head));
        if undo {
            self.ld(0, W::B8, spill_m(self.ext_slot(0)));
        } else {
            self.ld(0, W::B4, rm(1, head_off::PENDING as i32));
        }
        self.ld(2, W::B4, rm(1, head_off::ENTRY_FAIL as i32));
        self.push(Op::Add(2, Src::V(0)));
        self.st(W::B4, rm(1, head_off::ENTRY_FAIL as i32), Src::V(2));
        self.st(W::B4, rm(1, head_off::PENDING as i32), Src::Imm(1));
        if undo {
            self.ld(3, W::B4, rm(1, head_off::ENTRY_OK as i32));
            self.push(Op::Sub(3, Src::Imm(1)));
            self.st(W::B4, rm(1, head_off::ENTRY_OK as i32), Src::V(3));
        }
        if threshold {
            self.entry_threshold_ops();
        }
    }

    /// `if entry_fail >= 64 && entry_fail * 4 > entry_ok { jit_tr_entry_fail(head_pc) }` (`TrProto::entry_fail_hot`;
    /// `V0..V4` clobbered). The helper queues a flush (the slot of the trace is released at once), so **nothing that
    /// needs the trace's metadata may run after it**: an exit stub calls this last, right before its jump to tier 1.
    fn entry_threshold_ops(&mut self) {
        if self.env.head == 0 {
            return;
        }
        let done = self.label();
        self.push(Op::Mov(1, self.env.head));
        self.ld(2, W::B4, rm(1, head_off::ENTRY_FAIL as i32));
        self.ld(3, W::B4, rm(1, head_off::ENTRY_OK as i32));
        self.br(Cc::Lt, 2, Src::Imm(64), done);
        self.push(Op::MovR(4, 2));
        self.push(Op::ShlI(4, 2));
        self.br(Cc::Uge, 3, Src::V(4), done);
        self.save_vb();
        self.push(Op::CallH {
            f: self.env.entry_fail_fn,
            a: self.env.head_pc,
            b: 0,
        });
        self.reload_vb();
        self.bind(done);
    }

    // ---- reloads / stores ---------------------------------------------------------------------------------------------

    fn reload(&mut self, r: &super::ra::Reload) {
        match r.reg {
            Loc::G(n) => self.ld(n, W::B8, spill_m(r.slot)),
            Loc::X(n) => self.push(Op::LdF {
                dst: n,
                m: spill_m(r.slot),
            }),
            l => panic!("reload into {l:?}"),
        }
    }

    fn pre(&mut self, i: usize) {
        let v = self.al.ins[i].pre.clone();
        for r in &v {
            self.reload(r);
        }
    }

    fn post(&mut self, i: usize) {
        let a = &self.al.ins[i];
        let (dst, stores, post) = (a.dst, a.stores.clone(), a.post.clone());
        for s in stores {
            match dst {
                Loc::G(n) => self.st(W::B8, spill_m(s), Src::V(n)),
                Loc::X(n) => self.push(Op::StF {
                    m: spill_m(s),
                    src: n,
                }),
                l => panic!("store of {l:?}"),
            }
        }
        for r in &post {
            self.reload(r);
        }
    }

    // ---- instructions -------------------------------------------------------------------------------------------------

    fn ops(&self, i: usize) -> [Loc; 3] {
        self.al.ins[i].ops
    }

    fn dst(&self, i: usize) -> Loc {
        self.al.ins[i].dst
    }

    fn ins(&mut self, i: usize, loop_l: L) -> Result<(), AsmError> {
        let ir = self.ir;
        let ins = ir.ins[i];
        let dst = self.dst(i);
        let ops = self.ops(i);
        match ins.op {
            TOp::Kint | TOp::Knum | TOp::Kpri | TOp::Kptr => {
                let b = if ins.op == TOp::Kpri {
                    0
                } else {
                    self.kbits_of(ins.a)
                };
                match dst {
                    Loc::G(d) => self.push(Op::Mov(d, b)),
                    Loc::X(d) => self.ldf_const(d, b),
                    _ => {}
                }
            }
            TOp::Sload => self.sload(i)?,
            TOp::Add | TOp::Sub | TOp::Mul | TOp::Band | TOp::Bor | TOp::Bxor => self.ibin(i)?,
            TOp::Neg | TOp::Bnot => {
                let d = self.g(dst);
                match ops[0] {
                    Loc::Const(_) => {
                        let b = self.kbits_of(match ops[0] {
                            Loc::Const(r) => r,
                            _ => unreachable!(),
                        }) as i64;
                        let v = if ins.op == TOp::Neg {
                            b.wrapping_neg()
                        } else {
                            !b
                        };
                        self.push(Op::Mov(d, v as u64));
                    }
                    Loc::G(a) => {
                        if a != d {
                            self.push(Op::MovR(d, a));
                        }
                        self.push(if ins.op == TOp::Neg {
                            Op::Neg(d)
                        } else {
                            Op::Not(d)
                        });
                    }
                    l => panic!("operand {l:?}"),
                }
            }
            TOp::Shl | TOp::Shr => self.shift(i)?,
            TOp::Idiv | TOp::Mod => self.idivmod(i)?,
            TOp::Fadd | TOp::Fsub | TOp::Fmul | TOp::Fdiv | TOp::Fmin | TOp::Fmax => self.fbin(i),
            TOp::Fneg | TOp::Fabs => {
                let d = Self::xreg(dst);
                self.load_x(d, ops[0]);
                self.push(if ins.op == TOp::Fneg {
                    Op::FNeg(d)
                } else {
                    Op::FAbs(d)
                });
            }
            TOp::Fsqrt => {
                let d = Self::xreg(dst);
                match ops[0] {
                    Loc::X(a) => self.push(Op::FSqrt(d, a)),
                    c => {
                        self.load_x(d, c);
                        self.push(Op::FSqrt(d, d));
                    }
                }
            }
            TOp::Ffloor => {
                let d = Self::xreg(dst);
                match ops[0] {
                    Loc::X(a) => self.push(Op::FFloor(d, a)),
                    c => {
                        self.load_x(d, c);
                        self.push(Op::FFloor(d, d));
                    }
                }
            }
            TOp::ConvNumInt => {
                let d = Self::xreg(dst);
                let a = self.reg(ops[0], 7);
                self.push(Op::I2Fx(d, a));
            }
            TOp::F2iExact => {
                let x = self.guard_label(i)?;
                let src = match ops[0] {
                    Loc::X(n) => n,
                    c => {
                        // X0 is unallocatable in a trace that has F2I_EXACT (`ra::op_clobbers`)
                        self.load_x(0, c);
                        0
                    }
                };
                // through V7: the micro-op writes its destination on the failing path too, and the destination
                // register may hold a value the exit restores
                // P2 (`f2ifl`): the operand is a floor (integral, or NaN / inf / out of range): `cvttsd2si` alone decides
                let fl = self.env.f2ifl
                    && ((TraceIr::is_ins(ins.a) && self.ir.at(ins.a).op == TOp::Ffloor)
                        || mutants::active("f2ifl_all"))
                    && !mutants::active("f2ifl_never");
                if fl {
                    self.push(Op::F2IFloor { dst: 7, src, to: x });
                } else {
                    self.push(Op::F2IExact { dst: 7, src, to: x });
                }
                let d = self.g(dst);
                self.push(Op::MovR(d, 7));
            }
            // A1: the guard behind an inline `NEWREF` (the address of a node, never 0) is not assembled
            TOp::Ne if self.after_inline_newref(ins) => {}
            TOp::Eq | TOp::Ne | TOp::Lt | TOp::Le | TOp::Gt | TOp::Ge | TOp::Ult | TOp::Uge => {
                self.icmp(i)?
            }
            TOp::Fcmp => self.fcmp(i)?,
            TOp::Eqref => self.eqref(i)?,
            TOp::Fload => {
                let d = self.g(dst);
                let (w, off) = match ins.b {
                    b if b == Field::Asize as Ref => (W::B4, table::ASIZE),
                    b if b == Field::Array as Ref => (W::B8, table::ARRAY),
                    b if b == Field::Metatable as Ref => (W::B8, table::METATABLE),
                    b if b == Field::Flags as Ref => (W::B1, table::FLAGS),
                    b if b == Field::Lsizenode as Ref => (W::B1, table::LSIZENODE),
                    b if b == Field::Node as Ref => (W::B8, table::NODE),
                    b => return unsup(format!("table field {b}")),
                };
                let t = self.reg(ops[0], 7);
                self.ld(d, w, rm(t, off as i32));
            }
            TOp::Aref | TOp::Uref => {
                if !self.al.fused[i] {
                    if ins.op == TOp::Uref {
                        // an unfused UREF is the `UpVal` pointer
                        let d = self.g(dst);
                        self.upval_ptr(ops[0], ops[1], d)?;
                    } else {
                        return unsup("an AREF that is not fused into its memory users");
                    }
                }
            }
            TOp::Aload => self.aload(i)?,
            TOp::Astore => self.astore(i)?,
            TOp::Hrefk => self.hrefk(i)?,
            TOp::Hrefv => self.hrefv(i)?,
            TOp::Hmiss => self.hmiss(i)?,
            TOp::Hload => self.hload(i)?,
            TOp::Hstore => self.hstore(i)?,
            TOp::Uload | TOp::Ustore | TOp::Uvrel => self.uop(i)?,
            TOp::Retf => self.retf(i)?,
            TOp::Tbar => self.tbar(i)?,
            TOp::Atouch => self.atouch(i)?,
            TOp::Calln => self.calln(i)?,
            TOp::Tlen if matches!(self.ir.ty_of(ins.a), Ty::ShrStr | Ty::LngStr) => {
                // P-B: the length of a string is a load (`shrlen` byte / `u.len` word), no helper call
                let d = self.g(dst);
                let (w, off) = if self.ir.ty_of(ins.a) == Ty::ShrStr {
                    (
                        if mutants::active("tlen_str_wide") {
                            W::B8
                        } else {
                            W::B1
                        },
                        rn_core::jit_layout::string::SHRLEN,
                    )
                } else {
                    (W::B8, rn_core::jit_layout::string::LNGLEN)
                };
                let t = self.reg(ops[0], 7);
                self.ld(d, w, rm(t, off as i32));
            }
            TOp::Newref if ins.aux & nrplan::INLINE != 0 => self.newref_inline(i)?,
            TOp::Tnew
            | TOp::Newref
            | TOp::Tlen
            | TOp::Tset
            | TOp::Tnext
            | TOp::Snew
            | TOp::Concat
            | TOp::Strto
            | TOp::Lib => self.class_a(i)?,
            TOp::Sstore => {
                let slot = ins.aux as u16;
                self.store_slot(slot, ins.a, ops[0])?;
            }
            TOp::Tvirt => {
                if self.env.vcell == 0 {
                    return unsup("TVIRT without a virtual-byte cell");
                }
                let n = ins.aux as i32;
                self.push(Op::Mov(7, self.env.vcell));
                self.push(Op::AddM {
                    w: W::B8,
                    m: rm(7, 0),
                    imm: n,
                });
            }
            TOp::Loop => {
                for m in &self.al.entry_seq.clone() {
                    self.emit_move(*m);
                }
                self.head_pad();
                self.bind(loop_l);
                self.count_iteration();
            }
            TOp::Phi => {}
            TOp::Poll => {
                if !mutants::active("poll_skip") {
                    let x = self.guard_label(i)?;
                    self.poll_ops(x);
                }
            }
            TOp::Budget => {
                let x = self.guard_label(i)?;
                let n = i32::from(ins.a);
                let cnt = M {
                    base: Base::Cnt,
                    off: 0,
                };
                if let Some(r) = self.env.budget_reg {
                    if self.env.budget_sub
                        && !mutants::active("budget_no_check")
                        && !mutants::active("budget_no_store")
                    {
                        // P2-3a: `sub r, n; jl refund` and the refund stub adds `n` back before the real exit (the exit sees the budget as
                        // the compare-first form left it)
                        let refund = self.label();
                        self.bud_cold.push((refund, x, n));
                        self.push(Op::SubJl {
                            d: r,
                            imm: n,
                            to: refund,
                        });
                    } else {
                        if !mutants::active("budget_no_check") {
                            self.br(Cc::Lt, r, Src::Imm(n), x);
                        }
                        if !mutants::active("budget_no_store") {
                            self.push(Op::Sub(r, Src::Imm(n)));
                        }
                    }
                } else {
                    self.ld(7, W::B4, cnt);
                    if !mutants::active("budget_no_check") {
                        self.br(Cc::Lt, 7, Src::Imm(n), x);
                    }
                    self.push(Op::Sub(7, Src::Imm(n)));
                    if !mutants::active("budget_no_store") {
                        self.st(W::B4, cnt, Src::V(7));
                    }
                }
            }
        }
        Ok(())
    }

    fn g(&self, loc: Loc) -> V {
        match loc {
            Loc::G(n) => n,
            l => panic!("destination {l:?} is not a general register"),
        }
    }

    // ---- SLOAD ----------------------------------------------------------------------------------------------------------

    fn sload(&mut self, i: usize) -> Result<(), AsmError> {
        let ins = self.ir.ins[i];
        let slot = ins.a;
        if aux_bits(ins.aux) & 1 != 0 && !mutants::active("entry_no_typecheck") {
            let x = self.guard_label(i)?;
            let tag = match ins.t {
                Ty::Nil => {
                    // any nil variant (nil, empty, absent key): `tag & 0x0F == 0`
                    self.ld(7, W::B1, fm(slot, 0));
                    self.push(Op::And(7, Src::Imm(0x0F)));
                    self.br(Cc::Ne, 7, Src::Imm(0), x);
                    None
                }
                t => match tag_of(t) {
                    Some(g) => Some(g),
                    None => return unsup(format!("SLOAD of type {t:?}")),
                },
            };
            if let Some(tag) = tag {
                self.push(Op::BrM {
                    cc: Cc::Ne,
                    w: W::B1,
                    m: fm(slot, 0),
                    imm: i32::from(tag as i8),
                    to: x,
                });
            }
        }
        match self.dst(i) {
            Loc::G(d) => {
                if matches!(ins.t, Ty::Nil | Ty::False | Ty::True) {
                    self.push(Op::Mov(d, 0));
                } else {
                    self.ld(d, W::B8, fm(slot, 8));
                }
            }
            Loc::X(d) => self.push(Op::LdF {
                dst: d,
                m: fm(slot, 8),
            }),
            _ => {}
        }
        Ok(())
    }

    // ---- integer arithmetic -----------------------------------------------------------------------------------------------

    fn ibin(&mut self, i: usize) -> Result<(), AsmError> {
        let ins = self.ir.ins[i];
        let ops = self.ops(i);
        let d = self.g(self.dst(i));
        let bloc = ops[1];
        // the work register: `d`, unless `d` is the second operand's register (then `V7`)
        let w = if matches!(bloc, Loc::G(n) if n == d) && ops[0] != bloc {
            7
        } else {
            d
        };
        match ops[0] {
            Loc::G(a) => {
                if a != w {
                    self.push(Op::MovR(w, a));
                }
            }
            Loc::Const(r) => {
                let b = self.kbits_of(r);
                self.push(Op::Mov(w, b));
            }
            l => panic!("operand {l:?}"),
        }
        let s = self.src(bloc, 7);
        if matches!(s, Src::V(7)) && w == 7 {
            return unsup("two scratch operands");
        }
        self.push(match ins.op {
            TOp::Add => Op::Add(w, s),
            TOp::Sub => Op::Sub(w, s),
            TOp::Mul => Op::Mul(w, s),
            TOp::Band => Op::And(w, s),
            TOp::Bor => Op::Or(w, s),
            TOp::Bxor => Op::Xor(w, s),
            _ => unreachable!(),
        });
        if w != d {
            self.push(Op::MovR(d, w));
        }
        Ok(())
    }

    /// `shiftl(a, b)` for `Shl`, `shiftl(a, -b)` for `Shr` (logical).
    fn shift(&mut self, i: usize) -> Result<(), AsmError> {
        let ins = self.ir.ins[i];
        let ops = self.ops(i);
        let d = self.g(self.dst(i));
        let neg = ins.op == TOp::Shr;
        match ops[1] {
            Loc::Const(r) => {
                let y = self.kbits_of(r) as i64;
                let y = if neg { y.wrapping_neg() } else { y };
                self.load_g(d, ops[0]);
                if (1..=63).contains(&y) {
                    self.push(Op::ShlI(d, y as u8));
                } else if (-63..=-1).contains(&y) {
                    self.push(Op::ShrI(d, (-y) as u8));
                } else if y != 0 {
                    self.push(Op::Mov(d, 0));
                }
            }
            Loc::G(b) => {
                // count in V6 (`ra::op_clobbers` keeps V6 out of the allocation of a trace with a variable shift)
                self.push(Op::MovR(6, b));
                if neg {
                    self.push(Op::Neg(6));
                }
                self.load_g(d, ops[0]);
                let (lneg, lzero, lend) = (self.label(), self.label(), self.label());
                self.br(Cc::Lt, 6, Src::Imm(0), lneg);
                self.br(Cc::Ge, 6, Src::Imm(64), lzero);
                self.push(Op::ShlV(d));
                self.push(Op::Jmp(lend));
                self.bind(lneg);
                self.push(Op::Neg(6));
                self.br(Cc::Lt, 6, Src::Imm(0), lzero); // y == i64::MIN
                self.br(Cc::Ge, 6, Src::Imm(64), lzero);
                self.push(Op::ShrV(d));
                self.push(Op::Jmp(lend));
                self.bind(lzero);
                self.push(Op::Mov(d, 0));
                self.bind(lend);
            }
            l => panic!("operand {l:?}"),
        }
        Ok(())
    }

    fn load_g(&mut self, d: V, loc: Loc) {
        match loc {
            Loc::G(a) => {
                if a != d {
                    self.push(Op::MovR(d, a));
                }
            }
            Loc::Const(r) => {
                let b = self.kbits_of(r);
                self.push(Op::Mov(d, b));
            }
            l => panic!("operand {l:?}"),
        }
    }

    /// B4-4: `d = a // c` or `a % c` (floor semantics) for a constant `3 <= c <= 2^62` that is not a power of two, with `(mg, sh) =
    /// magic_u63(c)`. With `s = a >> 63` (all ones for a negative `a`) and `m = a ^ s` (`a` for `a >= 0`, `!a = -a - 1` otherwise, so
    /// `0 <= m < 2^63`): `floor(a / c) = (m / c) ^ s` (the identity `floor(a / c) = !floor(!a / c)` for negative `a`), and `m / c` is
    /// `mulhi(m, mg) >> sh` for every 63-bit `m`. `a % c = a - c * floor(a / c)`. Registers: `V7`, `V8` (`rax`, `rdx`), and `t` = the
    /// destination (saved / restored temporary when the destination is the dividend's register).
    fn div_magic(&mut self, d: V, a: V, c: i64, mg: u64, sh: u8, is_mod: bool) {
        let t = if d != a {
            d
        } else {
            let t = (0..=6u8)
                .find(|&v| v != a && Some(v) != self.env.budget_reg)
                .expect("a free temporary");
            self.saved.clear();
            self.save(Loc::G(t));
            t
        };
        self.push(Op::MovR(t, a));
        self.push(Op::SarI(t, 63)); // t = s
        self.push(Op::MovR(8, a));
        if !mutants::active("divmagic_no_sign") {
            self.push(Op::Xor(8, Src::V(t))); // V8 = m
        }
        let sh = if mutants::active("divmagic_shift_off") {
            sh + 1
        } else {
            sh
        };
        self.push(Op::MulHiU {
            dst: 8,
            a: 8,
            k: mg,
        });
        if sh > 0 {
            self.push(Op::ShrI(8, sh));
        }
        if !is_mod {
            if t == d {
                self.push(Op::Xor(d, Src::V(8)));
            } else {
                self.push(Op::Xor(8, Src::V(t)));
                self.push(Op::MovR(d, 8));
            }
        } else {
            self.push(Op::Xor(8, Src::V(t))); // V8 = floor(a / c)
            match fits(c) {
                Some(i) => self.push(Op::Mul(8, Src::Imm(i))),
                None => {
                    self.push(Op::Mov(7, c as u64));
                    self.push(Op::Mul(8, Src::V(7)));
                }
            }
            if d != a {
                self.push(Op::MovR(d, a));
            }
            self.push(Op::Sub(d, Src::V(8)));
        }
        if t != d {
            self.restore_all();
            self.saved.clear();
        }
    }

    /// `//` and `%` (`number::idiv` / `imod`, floor semantics, zero divisor exits, `-1` special-cased).
    fn idivmod(&mut self, i: usize) -> Result<(), AsmError> {
        let ins = self.ir.ins[i];
        let ops = self.ops(i);
        let is_mod = ins.op == TOp::Mod;
        let d = self.g(self.dst(i));
        let (aloc, bloc) = (ops[0], ops[1]);
        let lend = self.label();
        if ins.flags & F_GUARD != 0 {
            let x = self.guard_label(i)?;
            match bloc {
                Loc::G(b) => self.br(Cc::Eq, b, Src::Imm(0), x),
                Loc::Const(r) => {
                    if self.kbits_of(r) == 0 {
                        return unsup("constant zero divisor with a guard");
                    }
                }
                l => panic!("operand {l:?}"),
            }
        }
        // P-N: a constant power-of-two divisor needs no `idiv` and no sign fix-up (two's complement floor semantics):
        // `a % 2^k = a & (2^k - 1)`, `a // 2^k = a >> k` (arithmetic). `k <= 31` keeps the mask an immediate.
        if let Loc::Const(r) = bloc {
            let c = self.kbits_of(r) as i64;
            if c > 1 && (c as u64).is_power_of_two() && c <= (1 << 31) {
                self.load_g(d, aloc);
                let k = c.trailing_zeros() as u8;
                if is_mod {
                    let mask = if mutants::active("mod_pow2_mask_k") {
                        c
                    } else {
                        c - 1
                    };
                    self.push(Op::And(d, Src::Imm(mask.min(i64::from(i32::MAX)) as i32)));
                } else if mutants::active("idiv_pow2_logical") {
                    self.push(Op::ShrI(d, k));
                } else {
                    self.push(Op::SarI(d, k));
                }
                self.bind(lend);
                return Ok(());
            }
        }
        // B4-4 (`divmagic`): a positive constant divisor that is not a power of two: floor division by multiplication
        if let (true, Loc::Const(r), Loc::G(a)) = (self.env.divmagic, bloc, aloc) {
            let c = self.kbits_of(r) as i64;
            if let Some((mg, sh)) = magic_u63(c) {
                self.div_magic(d, a, c, mg, sh, is_mod);
                self.bind(lend);
                return Ok(());
            }
        }
        // the -1 divisor: q = 0 - a, r = 0 (also avoids the idiv overflow trap)
        let neg1 = |cx: &mut Self| {
            if is_mod {
                cx.push(Op::Mov(7, 0));
            } else {
                match aloc {
                    Loc::G(a) => {
                        cx.push(Op::Mov(7, 0));
                        cx.push(Op::Sub(7, Src::V(a)));
                    }
                    Loc::Const(r) => {
                        let v = (cx.kbits_of(r) as i64).wrapping_neg();
                        cx.push(Op::Mov(7, v as u64));
                    }
                    l => panic!("operand {l:?}"),
                }
            }
            cx.push(Op::MovR(d, 7));
        };
        let lm1 = self.label();
        match bloc {
            Loc::Const(r) => {
                let c = self.kbits_of(r) as i64;
                if c == -1 {
                    neg1(self);
                    self.bind(lend);
                    return Ok(());
                }
            }
            Loc::G(b) => self.br(Cc::Eq, b, Src::Imm(-1), lm1),
            l => panic!("operand {l:?}"),
        }
        // registers: a, b in V0..V6 (materialised into temps when constant), q and r temps
        let mut used: Vec<V> = Vec::new();
        if let Loc::G(a) = aloc {
            used.push(a);
        }
        if let Loc::G(b) = bloc {
            used.push(b);
        }
        let mut pick = |cx: &mut Self, prefer: Option<V>| -> V {
            let cands: Vec<V> = prefer
                .into_iter()
                .chain(0..=6)
                .filter(|v| !used.contains(v) && Some(*v) != cx.env.budget_reg)
                .collect();
            let v = cands[0];
            used.push(v);
            v
        };
        let tq = pick(self, Some(d));
        let tr = pick(self, Some(d));
        let ta = if let Loc::Const(_) = aloc {
            Some(pick(self, None))
        } else {
            None
        };
        let tb = if let Loc::Const(_) = bloc {
            Some(pick(self, None))
        } else {
            None
        };
        let temps: Vec<V> = [Some(tq), Some(tr), ta, tb].into_iter().flatten().collect();
        self.saved.clear();
        for &t in &temps {
            if t != d {
                self.save(Loc::G(t));
            }
        }
        let a = match (aloc, ta) {
            (Loc::G(a), _) => a,
            (Loc::Const(r), Some(t)) => {
                let b = self.kbits_of(r);
                self.push(Op::Mov(t, b));
                t
            }
            _ => unreachable!(),
        };
        let b = match (bloc, tb) {
            (Loc::G(b), _) => b,
            (Loc::Const(r), Some(t)) => {
                let v = self.kbits_of(r);
                self.push(Op::Mov(t, v));
                t
            }
            _ => unreachable!(),
        };
        let lfix = self.label();
        self.push(Op::SDivRem { q: tq, r: tr, a, b });
        self.br(Cc::Eq, tr, Src::Imm(0), lfix);
        self.push(Op::MovR(7, tr));
        self.push(Op::Xor(7, Src::V(b)));
        self.br(Cc::Ge, 7, Src::Imm(0), lfix);
        if is_mod {
            self.push(Op::Add(tr, Src::V(b)));
        } else {
            self.push(Op::Sub(tq, Src::Imm(1)));
        }
        self.bind(lfix);
        let res = if is_mod { tr } else { tq };
        if res != d {
            self.push(Op::MovR(d, res));
        }
        // restore the temporaries (never `d`: it is not in the list)
        self.restore_all();
        self.saved.clear();
        if let Loc::G(_) = bloc {
            self.push(Op::Jmp(lend));
            self.bind(lm1);
            neg1(self);
        }
        self.bind(lend);
        Ok(())
    }

    // ---- floats ---------------------------------------------------------------------------------------------------------

    fn fbin(&mut self, i: usize) {
        let ins = self.ir.ins[i];
        let ops = self.ops(i);
        let d = Self::xreg(self.dst(i));
        let op = match ins.op {
            TOp::Fadd => FOp::Add,
            TOp::Fsub => FOp::Sub,
            TOp::Fmul => FOp::Mul,
            TOp::Fdiv => FOp::Div,
            TOp::Fmin => FOp::Min,
            TOp::Fmax => FOp::Max,
            _ => unreachable!(),
        };
        // P2-3c: `x * 2.0` is `x + x` (bit-exact for every double: both are the correctly rounded `2x`, a NaN operand comes out quieted
        // with its payload and sign, zeros and infinities keep their sign)
        if self.env.fpool != 0 && op == FOp::Mul {
            let two = |c: Loc, cx: &Self| matches!(c, Loc::Const(r) if cx.kbits_of(r) == 2.0f64.to_bits() || (mutants::active("mul2_any_const") && cx.kbits_of(r) == 4.0f64.to_bits()));
            let (x, k) = if two(ops[1], self) {
                (ops[0], true)
            } else if two(ops[0], self) {
                (ops[1], true)
            } else {
                (ops[0], false)
            };
            if k && matches!(x, Loc::X(_)) {
                self.load_x(d, x);
                self.push(Op::FBin {
                    op: FOp::Add,
                    dst: d,
                    src: d,
                });
                return;
            }
        }
        // `d = a op b`; `d` may equal a's register, and equals b's only when both are the same value
        self.load_x(d, ops[0]);
        match ops[1] {
            Loc::X(b) => self.push(Op::FBin { op, dst: d, src: b }),
            Loc::Const(r) if self.env.fpool != 0 && self.fpool_m(self.kbits_of(r)).is_some() => {
                let b = self.kbits_of(r);
                let m = self.fpool_m(b).expect("interned above");
                self.push(Op::FBinM { op, dst: d, m });
            }
            Loc::Const(r) => {
                // the constant is staged in `args[0]` (no register needed)
                let b = self.kbits_of(r);
                self.push(Op::Mov(7, b));
                self.st(W::B8, area_m(exit_area::ARGS), Src::V(7));
                self.push(Op::FBinM {
                    op,
                    dst: d,
                    m: area_m(exit_area::ARGS),
                });
            }
            l => panic!("operand {l:?}"),
        }
    }

    fn icmp(&mut self, i: usize) -> Result<(), AsmError> {
        let ins = self.ir.ins[i];
        let ops = self.ops(i);
        let cc = match ins.op {
            TOp::Eq => Cc::Eq,
            TOp::Ne => Cc::Ne,
            TOp::Lt => Cc::Lt,
            TOp::Le => Cc::Le,
            TOp::Gt => Cc::Gt,
            TOp::Ge => Cc::Ge,
            TOp::Ult => Cc::Ult,
            _ => Cc::Uge,
        };
        let x = self.guard_label(i)?;
        if let (Loc::Const(p), Loc::Const(q)) = (ops[0], ops[1]) {
            // decided at assembly time: a failing guard is an unconditional exit
            let (u, v) = (self.kbits_of(p), self.kbits_of(q));
            let holds = match ins.op {
                TOp::Eq => u == v,
                TOp::Ne => u != v,
                TOp::Lt => (u as i64) < (v as i64),
                TOp::Le => (u as i64) <= (v as i64),
                TOp::Gt => (u as i64) > (v as i64),
                TOp::Ge => (u as i64) >= (v as i64),
                TOp::Ult => u < v,
                _ => u >= v,
            };
            if !holds {
                self.push(Op::Jmp(x));
            }
            return Ok(());
        }
        let a = match ops[0] {
            Loc::G(a) => a,
            Loc::Const(_) => self.reg(ops[0], 7),
            l => panic!("operand {l:?}"),
        };
        let b = match (ops[1], a) {
            (Loc::Const(_), 7) => match self.src(ops[1], 7) {
                Src::Imm(v) => Src::Imm(v),
                Src::V(_) => return unsup("compare of two wide constants"),
            },
            (l, _) => self.src(l, 7),
        };
        // the guard asserts the relation: leave on its negation
        self.br(neg_cc(cc), a, b, x);
        Ok(())
    }

    fn fcmp(&mut self, i: usize) -> Result<(), AsmError> {
        let ins = self.ir.ins[i];
        let ops = self.ops(i);
        let bits = aux_bits(ins.aux);
        let cc = match bits & 15 {
            0 => Cc::Eq,
            1 => Cc::Ne,
            2 => Cc::Lt,
            3 => Cc::Le,
            4 => Cc::Gt,
            5 => Cc::Ge,
            c => return unsup(format!("FCMP condition {c}")),
        };
        let expect = bits >> 4 & 1 != 0;
        let x = self.guard_label(i)?;
        if let (Loc::Const(p), Loc::Const(q)) = (ops[0], ops[1]) {
            let (u, v) = (
                f64::from_bits(self.kbits_of(p)),
                f64::from_bits(self.kbits_of(q)),
            );
            if super::fold::eval_fcmp(bits & 15, u, v) != expect {
                self.push(Op::Jmp(x));
            }
            return Ok(());
        }
        // constant operands go through a temporary X register: one the allocation never uses (no save, no store/load
        // pair on the dependency chain of the compared value: mandelbrot's `zr*zr + zi*zi < 4.0` lost a third of its
        // speed to the save of the live register the old code picked), else a register saved around the compare
        let free_x = (0..16u8).rev().find(|&x| !self.al.uses_xmm(x));
        self.saved.clear();
        let mut regs = [0u8; 2];
        let mut used: Vec<X> = ops[..2]
            .iter()
            .filter_map(|l| if let Loc::X(n) = l { Some(*n) } else { None })
            .collect();
        for k in 0..2 {
            match ops[k] {
                Loc::X(n) => regs[k] = n,
                Loc::Const(r) => {
                    let t = match free_x {
                        Some(t) if !used.contains(&t) => t,
                        _ => {
                            let t = (0..16u8)
                                .find(|v| !used.contains(v))
                                .expect("a free X register");
                            self.save(Loc::X(t));
                            t
                        }
                    };
                    used.push(t);
                    let b = self.kbits_of(r);
                    self.ldf_const(t, b);
                    regs[k] = t;
                }
                l => panic!("operand {l:?}"),
            }
        }
        let have_temp = !self.saved.is_empty();
        if expect {
            // `a cc b` must hold: on false leave. (Never a negated cc, TR11: the mutant does negate it.)
            if mutants::active("fcmp_negated") {
                self.push(Op::FBrX {
                    cc: neg_cc(cc),
                    a: regs[0],
                    b: regs[1],
                    to: x,
                });
                self.restore_all();
            } else {
                let ok = self.label();
                self.push(Op::FBrX {
                    cc,
                    a: regs[0],
                    b: regs[1],
                    to: ok,
                });
                self.restore_all();
                self.push(Op::Jmp(x));
                self.bind(ok);
                self.restore_all();
            }
        } else if have_temp {
            let (fail, done) = (self.label(), self.label());
            self.push(Op::FBrX {
                cc,
                a: regs[0],
                b: regs[1],
                to: fail,
            });
            self.restore_all();
            self.push(Op::Jmp(done));
            self.bind(fail);
            self.restore_all();
            self.push(Op::Jmp(x));
            self.bind(done);
        } else {
            self.push(Op::FBrX {
                cc,
                a: regs[0],
                b: regs[1],
                to: x,
            });
        }
        self.saved.clear();
        Ok(())
    }

    // ---- identity ------------------------------------------------------------------------------------------------------

    fn eqref(&mut self, i: usize) -> Result<(), AsmError> {
        let ins = self.ir.ins[i];
        let ops = self.ops(i);
        let x = self.guard_label(i)?;
        let x_l = x;
        let proto = match self.ir.kval_opt(ins.b) {
            Some(KVal::Ptr(addr, PtrKind::Proto)) => Some(addr),
            _ => None,
        };
        if let (Some(addr), Loc::G(c)) = (proto, ops[0]) {
            if c != 7 && self.env.hk_key64 {
                // J1-1: one 64-bit compare against a register (`movabs v7, proto; cmp [closure + P], v7; jne exit`)
                let k = if mutants::active("eqref_proto64_hi") {
                    addr ^ (1 << 40)
                } else {
                    addr
                };
                self.push(Op::Mov(7, k));
                self.push(Op::BrMR {
                    cc: Cc::Ne,
                    m: rm(c, lclosure::PROTO as i32),
                    src: 7,
                    to: x,
                });
                return Ok(());
            }
            // the callee closure's proto: compare `closure.p` with the constant, 32 bits at a time (no register)
            for (half, off) in [(addr as u32, 0), ((addr >> 32) as u32, 4)] {
                self.push(Op::BrM {
                    cc: Cc::Ne,
                    w: W::B4,
                    m: rm(c, lclosure::PROTO as i32 + off),
                    imm: half as i32,
                    to: x,
                });
            }
            return Ok(());
        }
        if proto.is_some() {
            return unsup("proto identity of a non-register closure");
        }
        // payload identity (builtin address, table, ...)
        let (a, b) = match (ops[0], ops[1]) {
            (Loc::G(a), Loc::G(b)) => (a, Src::V(b)),
            (Loc::G(a), Loc::Const(r)) => {
                let v = self.kbits_of(r);
                self.push(Op::Mov(7, v));
                (a, Src::V(7))
            }
            (Loc::Const(r), Loc::G(b)) => {
                let v = self.kbits_of(r);
                self.push(Op::Mov(7, v));
                (b, Src::V(7))
            }
            (Loc::Const(x), Loc::Const(y)) => {
                // decided at assembly time: a failing guard is an unconditional exit
                if self.kbits_of(x) != self.kbits_of(y) {
                    self.push(Op::Jmp(x_l));
                }
                return Ok(());
            }
            _ => return unsup("EQREF of unsupported operands"),
        };
        self.br(Cc::Ne, a, b, x);
        Ok(())
    }

    // ---- memory ----------------------------------------------------------------------------------------------------------

    /// `V7 = array base - 8 * index` for a register index (payload of element `u` is at `[V7 - 8]`).
    fn payload_base(&mut self, base: V, idx: V) {
        self.push(Op::MovR(7, idx));
        self.push(Op::ShlI(7, 3));
        self.push(Op::Neg(7));
        self.push(Op::Add(7, Src::V(base)));
    }

    /// B2 (`b2arr`): the scaled address of the payload of element `u` of the array at `base`: `[base + 8 * ~u]` (`~u = -u - 1`, and
    /// the payload of element `u` is at `base - 8 * (u + 1)`): two instructions (`mov V7, u; not V7`) instead of four and the access
    /// itself indexed. The index sits in `V7`.
    fn payload_scaled(&mut self, base: V, idx: V) -> Mi {
        self.push(Op::MovR(7, idx));
        if !mutants::active("b2arr_no_not") {
            self.push(Op::Not(7));
        }
        Mi {
            base,
            idx: 7,
            scale: 8,
            off: if mutants::active("b2arr_off8") { 8 } else { 0 },
        }
    }

    /// `st` of a value to an indexed payload address (the `store_value` of the scaled form; `Mi` has no scratch left, so a
    /// 64-bit constant is stored in two halves).
    fn store_value_mi(&mut self, m: Mi, r: Ref, loc: Loc) {
        let t = self.ir.ty_of(r);
        if matches!(t, Ty::Nil | Ty::False | Ty::True) {
            if !self.env.nopay {
                self.push(Op::StI {
                    w: W::B8,
                    m,
                    src: Src::Imm(0),
                });
            }
            return;
        }
        if self.env.nopay && mutants::active("nopay_all_types") {
            return;
        }
        match loc {
            Loc::G(n) => self.push(Op::StI {
                w: W::B8,
                m,
                src: Src::V(n),
            }),
            Loc::X(n) => self.push(Op::StFi { m, src: n }),
            Loc::Const(c) => {
                let b = self.kbits_of(c);
                match fits(b as i64) {
                    Some(i) => self.push(Op::StI {
                        w: W::B8,
                        m,
                        src: Src::Imm(i),
                    }),
                    None => {
                        self.push(Op::StI {
                            w: W::B4,
                            m,
                            src: Src::Imm(b as u32 as i32),
                        });
                        let m2 = Mi {
                            off: m.off + 4,
                            ..m
                        };
                        self.push(Op::StI {
                            w: W::B4,
                            m: m2,
                            src: Src::Imm((b >> 32) as u32 as i32),
                        });
                    }
                }
            }
            l => panic!("operand {l:?}"),
        }
    }

    fn aload(&mut self, i: usize) -> Result<(), AsmError> {
        let ins = self.ir.ins[i];
        let ops = self.ops(i);
        let ar = self.ir.at(ins.a);
        if ar.op != TOp::Aref || !self.al.fused[usize::from(ins.a - REF_BIAS)] {
            return unsup("ALOAD of an unfused address");
        }
        let base = match ops[0] {
            Loc::G(b) => b,
            _ => return unsup("constant array base"),
        };
        let check =
            aux_bits(ins.aux) & 1 != 0 && !(ins.t == Ty::Nil && mutants::active("aload_nil_skip"));
        if check {
            let x = self.guard_label(i)?;
            // M1: a nil element (any nil variant: `tag & 0x0F == 0`, as the nil `HLOAD`)
            let nil = ins.t == Ty::Nil;
            let tag = match tag_of(ins.t) {
                Some(t) => t,
                None if nil => 0,
                None => return unsup(format!("ALOAD of type {:?}", ins.t)),
            };
            let anystr = ins.flags & F_ANYSTR != 0 || nil;
            let (tmask, twant) = if nil {
                (0x0F, 0)
            } else {
                (ANYSTR_MASK, i32::from(tag & ANYSTR_MASK as u8))
            };
            if anystr {
                // N5: any string variant: the tag byte with the variant bit masked
                match ops[1] {
                    Loc::Const(r) => {
                        let (toff, _) = const_index_offsets(self.kbits_of(r))?;
                        self.ld(7, W::B1, rm(base, toff));
                    }
                    Loc::G(u) if self.env.b2_arr => self.push(Op::LdI {
                        dst: 7,
                        w: W::B1,
                        m: Mi {
                            base,
                            idx: u,
                            scale: 1,
                            off: table::ARRAY_TAG_BASE as i32,
                        },
                    }),
                    Loc::G(u) => {
                        self.push(Op::MovR(7, u));
                        self.push(Op::Add(7, Src::V(base)));
                        self.ld(7, W::B1, rm(7, table::ARRAY_TAG_BASE as i32));
                    }
                    l => panic!("operand {l:?}"),
                }
                self.push(Op::And(7, Src::Imm(tmask)));
                self.br(Cc::Ne, 7, Src::Imm(twant), x);
            } else {
                match ops[1] {
                    Loc::Const(r) => {
                        let (toff, _) = const_index_offsets(self.kbits_of(r))?;
                        self.push(Op::BrM {
                            cc: Cc::Ne,
                            w: W::B1,
                            m: rm(base, toff),
                            imm: i32::from(tag as i8),
                            to: x,
                        });
                    }
                    Loc::G(u) => self.push(Op::BrMi {
                        cc: Cc::Ne,
                        w: W::B1,
                        m: Mi {
                            base,
                            idx: u,
                            scale: 1,
                            off: table::ARRAY_TAG_BASE as i32,
                        },
                        imm: i32::from(tag as i8),
                        to: x,
                    }),
                    l => panic!("operand {l:?}"),
                }
            }
        }
        if let (true, Loc::G(u)) = (self.env.b2_arr, ops[1]) {
            // B2: scaled form
            // BUNDLE-L A (`aidx`): the address carries the complemented index as a value: `[base + 8 * n + 8 * d]`, no `mov; not`
            let pre = match (ops[2], ar.third()) {
                (Loc::G(n), Some(_)) => {
                    let d = if mutants::active("aidx_disp_ignored") {
                        0
                    } else {
                        aref_disp(ar.aux)
                    };
                    Some(Mi {
                        base,
                        idx: n,
                        scale: 8,
                        off: 8 * d + if mutants::active("aidx_off8") { 8 } else { 0 },
                    })
                }
                _ => None,
            };
            match self.dst(i) {
                Loc::G(d) => {
                    if matches!(ins.t, Ty::Nil | Ty::False | Ty::True) {
                        self.push(Op::Mov(d, 0));
                    } else {
                        let m = pre.unwrap_or_else(|| self.payload_scaled(base, u));
                        self.push(Op::LdI {
                            dst: d,
                            w: W::B8,
                            m,
                        });
                    }
                }
                Loc::X(d) => {
                    let m = pre.unwrap_or_else(|| self.payload_scaled(base, u));
                    self.push(Op::LdFi { dst: d, m });
                }
                _ => {}
            }
            return Ok(());
        }
        let m = match ops[1] {
            Loc::Const(r) => rm(base, const_index_offsets(self.kbits_of(r))?.1),
            Loc::G(u) => {
                self.payload_base(base, u);
                rm(7, -8)
            }
            l => panic!("operand {l:?}"),
        };
        match self.dst(i) {
            Loc::G(d) => {
                if matches!(ins.t, Ty::Nil | Ty::False | Ty::True) {
                    self.push(Op::Mov(d, 0));
                } else {
                    self.ld(d, W::B8, m);
                }
            }
            Loc::X(d) => self.push(Op::LdF { dst: d, m }),
            _ => {}
        }
        Ok(())
    }

    fn astore(&mut self, i: usize) -> Result<(), AsmError> {
        let ins = self.ir.ins[i];
        let ops = self.ops(i);
        let ar = self.ir.at(ins.a);
        if ar.op != TOp::Aref || !self.al.fused[usize::from(ins.a - REF_BIAS)] {
            return unsup("ASTORE to an unfused address");
        }
        let base = match ops[0] {
            Loc::G(b) => b,
            _ => return unsup("constant array base"),
        };
        let t = self.ir.ty_of(ins.b);
        let tag = match tag_of(t) {
            Some(g) => g,
            None => return unsup(format!("ASTORE of type {t:?}")),
        };
        match ops[1] {
            Loc::Const(r) => {
                let (toff, _) = const_index_offsets(self.kbits_of(r))?;
                self.st(W::B1, rm(base, toff), Src::Imm(i32::from(tag)));
            }
            Loc::G(u) => self.push(Op::StI {
                w: W::B1,
                m: Mi {
                    base,
                    idx: u,
                    scale: 1,
                    off: table::ARRAY_TAG_BASE as i32,
                },
                src: Src::Imm(i32::from(tag)),
            }),
            l => panic!("operand {l:?}"),
        }
        if let (true, Loc::G(u)) = (self.env.b2_arr, ops[1]) {
            let m = self.payload_scaled(base, u);
            self.store_value_mi(m, ins.b, ops[2]);
            return Ok(());
        }
        let m = match ops[1] {
            Loc::Const(r) => rm(base, const_index_offsets(self.kbits_of(r))?.1),
            Loc::G(u) => {
                self.payload_base(base, u);
                rm(7, -8)
            }
            l => panic!("operand {l:?}"),
        };
        self.store_value(m, ins.b, ops[2]);
        Ok(())
    }

    // ---- hash part (F1) -----------------------------------------------------------------------------------------------

    /// B4-3: the exact value tag of the `HLOAD` directly behind the `HREFK` at `i` when its type guard can be folded into the key-tag
    /// compare: the `HLOAD` reads that node's value (not the pseudo key node of `TNEXT`) and checks an exact tag (not nil / any-string).
    /// The second result is true when the two guards leave through different snapshots (J1-1: since `typesides` the type guard has its
    /// own snapshot): the pair is fused only when both snapshots describe the same state at the `HREFK` (same pc / frames / budget
    /// refund / landing / top / map and the same restore locations, nothing between the two instructions moves a value), and a failed
    /// compare then goes through a cold disambiguator that picks the exit of the failing guard.
    fn hk_fuse_tag(&self, i: usize) -> Option<(u8, bool)> {
        if !self.env.hk_fuse || i + 1 >= self.ir.ins.len() {
            return None;
        }
        let (h, l) = (&self.ir.ins[i], &self.ir.ins[i + 1]);
        if l.op != TOp::Hload
            || l.a != REF_BIAS + i as Ref
            || aux_bits(l.aux) != 1
            || l.flags & F_ANYSTR != 0
        {
            return None;
        }
        if matches!(l.t, Ty::Nil) {
            return None;
        }
        let (sh, sl) = (aux_snap(h.aux), aux_snap(l.aux));
        let split = sh != sl;
        if split {
            if mutants::active("hkfuse_split_never")
                || (!mutants::active("hkfuse_state_skip")
                    && !self.hk_same_state(i, sh as usize, sl as usize))
            {
                return None;
            }
        }
        tag_of(l.t).map(|t| (t, split))
    }

    /// J1-1: the exit of the `HLOAD` guard at `i + 1` may be taken at the `HREFK` at `i` (see [`Cx::hk_fuse_tag`]).
    fn hk_same_state(&self, i: usize, sh: usize, sl: usize) -> bool {
        if sh >= self.ir.snaps.len()
            || sl >= self.ir.snaps.len()
            || sh >= self.al.snap_locs.len()
            || sl >= self.al.snap_locs.len()
        {
            return false;
        }
        // the guards must be of the same kind of entry (`stub_label` / `disp_label`), and nothing may run between them
        if self.entry_kind(i) != self.entry_kind(i + 1) {
            return false;
        }
        if !self.al.ins[i].post.is_empty() || !self.al.ins[i + 1].pre.is_empty() {
            return false;
        }
        let (a, b) = (&self.ir.snaps[sh], &self.ir.snaps[sl]);
        if !(a.pc == b.pc
            && a.frame == b.frame
            && a.nframes == b.nframes
            && a.refund == b.refund
            && a.land == b.land
            && a.top == b.top)
        {
            return false;
        }
        let win = |s: &super::ir::Snap| {
            &self.ir.snapmap[s.map.0 as usize..s.map.0 as usize + usize::from(s.map.1)]
        };
        win(a) == win(b) && self.al.snap_locs[sh] == self.al.snap_locs[sl]
    }

    /// `HREFK node, key, idx`: `dst = node + idx * 24` after checking that this node holds the short-string key (tag
    /// and address). Everything goes through `V7`: the destination register may hold a value the exit restores, and
    /// it is written only after the guard has passed.
    fn hrefk(&mut self, i: usize) -> Result<(), AsmError> {
        let ins = self.ir.ins[i];
        let ops = self.ops(i);
        let idx = i64::from(aux_bits(ins.aux));
        let off = match fits(idx * node::SIZE as i64) {
            Some(o) => o,
            None => return unsup("HREFK slot index"),
        };
        let Loc::Const(kr) = ops[1] else {
            return unsup("HREFK with a key that is not a constant");
        };
        let key = self.kbits_of(kr);
        let x = self.guard_label(i)?;
        let n = self.reg(ops[0], 7);
        if self.env.b2_hk {
            // B2: the three checks read the node through displacements; the result is one `lea` (nothing when it is the node
            // itself)
            let at = |o: usize| rm(n, off + o as i32);
            if let Some((tag, split)) = self.hk_fuse_tag(i) {
                // B4-3: key tag (byte +9) and the value tag of the `HLOAD` that follows (byte +8) in one 16-bit compare
                let want = if mutants::active("hkfuse_tag_skip") {
                    0
                } else {
                    u16::from(tag)
                };
                let to = if split {
                    // J1-1: the two guards have their own exits: a failed compare picks the exit of the guard that failed
                    let xl = self.guard_label(i + 1)?;
                    let cold = self.label();
                    self.hk_cold.push(HkCold {
                        label: cold,
                        m: at(node::KEY_TT),
                        hrefk_exit: x,
                        hload_exit: xl,
                    });
                    self.hk_split_n += 1;
                    cold
                } else {
                    x
                };
                self.push(Op::BrM16 {
                    cc: Cc::Ne,
                    m: at(node::VAL_TT),
                    imm: want | (u16::from(ctb(LUA_VSHRSTR)) << 8),
                    to,
                });
                self.hk_fused = Some(i + 1);
                self.hk_fused_n += 1;
            } else {
                self.push(Op::BrM {
                    cc: Cc::Ne,
                    w: W::B1,
                    m: at(node::KEY_TT),
                    imm: i32::from(ctb(LUA_VSHRSTR) as i8),
                    to: x,
                });
            }
            if !mutants::active("hrefk_key_skip") {
                if n != 7 && self.env.hk_key64 {
                    // H3-F: the 64-bit key address in one compare against a register (`movabs v7, key; cmp [node + KEY], v7; jne`:
                    // two micro-ops, the two 32-bit memory-immediate compares were four)
                    let k = if mutants::active("hrefk_key64_hi") {
                        key ^ (1 << 40)
                    } else {
                        key
                    };
                    self.push(Op::Mov(7, k));
                    self.push(Op::BrMR {
                        cc: Cc::Ne,
                        m: at(node::KEY),
                        src: 7,
                        to: x,
                    });
                } else {
                    for (half, o) in [(key as u32, 0), ((key >> 32) as u32, 4)] {
                        self.push(Op::BrM {
                            cc: Cc::Ne,
                            w: W::B4,
                            m: at(node::KEY + o as usize),
                            imm: half as i32,
                            to: x,
                        });
                    }
                }
            }
            let d = self.g(self.dst(i));
            if off != 0 {
                self.push(Op::Lea(
                    d,
                    rm(
                        n,
                        if mutants::active("b2hk_lea_off") {
                            0
                        } else {
                            off
                        },
                    ),
                ));
            } else if d != n {
                self.push(Op::MovR(d, n));
            }
            return Ok(());
        }
        if n != 7 {
            self.push(Op::MovR(7, n));
        }
        if off != 0 {
            self.push(Op::Add(7, Src::Imm(off)));
        }
        self.push(Op::BrM {
            cc: Cc::Ne,
            w: W::B1,
            m: rm(7, node::KEY_TT as i32),
            imm: i32::from(ctb(LUA_VSHRSTR) as i8),
            to: x,
        });
        if !mutants::active("hrefk_key_skip") {
            for (half, o) in [(key as u32, 0), ((key >> 32) as u32, 4)] {
                self.push(Op::BrM {
                    cc: Cc::Ne,
                    w: W::B4,
                    m: rm(7, node::KEY as i32 + o),
                    imm: half as i32,
                    to: x,
                });
            }
        }
        let d = self.g(self.dst(i));
        self.push(Op::MovR(d, 7));
        Ok(())
    }

    /// `HREFV node, key` (N6): `dst` = the node that holds the dynamic short-string key `key` (the address of a live short string)
    /// in the node vector `node`, or an exit when the chain does not hold it. `bits` = the node count's log (the recorder's
    /// `lsizenode` guard precedes this op, so the mask is an immediate). The walk is `luaH_getshortstr`: main position `hash &
    /// mask` (`hash` = the 32-bit header field of the string), then `next` links (signed, in nodes) until the key tag is `SHRSTR`
    /// and the key address equals `key`, or the link is 0 (exit). `V7` is the node cursor; one more scratch register (two when
    /// the key is a constant) is saved in an extension slot and restored on both paths. The destination is written last.
    fn hrefv(&mut self, i: usize) -> Result<(), AsmError> {
        let ins = self.ir.ins[i];
        let ops = self.ops(i);
        let ibits = aux_bits(ins.aux);
        // H1: bit 8 = integer key (`HREFI` form: main position `key % ((size - 1) | 1)`, key tag `LUA_VNUMINT`)
        let int_key = ibits & super::ir::HREF_INT != 0;
        // DYNMASK: `HREF_DYN` = operand `a` is the table; node count and node vector are read when the instruction runs. `HREF_NIL` = no exit: a
        // walk that finds nothing yields the static nil node (loads only)
        let dynm = ibits & super::ir::HREF_DYN != 0;
        let nilm = dynm && ibits & super::ir::HREF_NIL != 0;
        if dynm && int_key {
            return unsup("HREFV dynamic-mask integer key");
        }
        let lsize = if dynm { 0 } else { ibits & 0xFF };
        if lsize > 30 {
            return unsup("HREFV node count");
        }
        let mask = (1i64 << lsize) - 1;
        let Loc::G(n) = ops[0] else {
            return unsup("HREFV of a constant node vector");
        };
        let x = if nilm {
            None
        } else {
            Some(self.guard_label(i)?)
        };
        // BUNDLE-L D1b: `HREF_ABS` = the absence proof of a dynamic key: the walk exits when it FINDS the key and falls through (no result)
        // when the chain ends without it
        let abs = ibits & super::ir::HREF_ABS != 0;
        let d_opt = match self.dst(i) {
            Loc::G(d) => Some(d),
            _ if abs => None,
            l => panic!("destination {l:?} is not a general register"),
        };
        let mut used: Vec<V> = vec![n];
        used.extend(d_opt);
        if let Loc::G(k) = ops[1] {
            used.push(k);
        }
        let mut pick = |cx: &mut Self| -> V {
            let v = (0..=6u8)
                .find(|v| !used.contains(v) && Some(*v) != cx.env.budget_reg)
                .expect("a free scratch register");
            used.push(v);
            v
        };
        let t1 = pick(self);
        let t2 = if let Loc::Const(_) = ops[1] {
            Some(pick(self))
        } else {
            None
        };
        let t3 = if int_key && lsize >= 2 {
            Some(pick(self))
        } else {
            None
        };
        self.saved.clear();
        self.save(Loc::G(t1));
        if let Some(t) = t2 {
            self.save(Loc::G(t));
        }
        if let Some(t) = t3 {
            self.save(Loc::G(t));
        }
        let k = match (ops[1], t2) {
            (Loc::G(k), _) => k,
            (Loc::Const(r), Some(t)) => {
                let b = self.kbits_of(r);
                self.push(Op::Mov(t, b));
                t
            }
            _ => unreachable!(),
        };
        let (lloop, lnext, lmiss, lfound) =
            (self.label(), self.label(), self.label(), self.label());
        if int_key {
            // `hashint`: node `key % ((sizenode - 1) | 1)` (unsigned); 1 node or 2 nodes: index 0
            let c = ((1u64 << lsize) - 1) | 1;
            let c = if mutants::active("hrefi_mod_size") {
                1u64 << lsize
            } else {
                c
            };
            if let Some(td) = t3 {
                self.push(Op::Mov(td, c));
                // (the emulator poisons `rax` / `rdx` after a division: the remainder is `key - quotient * modulus`)
                self.push(Op::UDiv {
                    dst: t1,
                    a: k,
                    b: td,
                });
                self.push(Op::Mul(t1, Src::V(td)));
                self.push(Op::MovR(7, k));
                self.push(Op::Sub(7, Src::V(t1)));
                self.push(Op::Mul(7, Src::Imm(node::SIZE as i32)));
                self.push(Op::Add(7, Src::V(n)));
            } else {
                self.push(Op::MovR(7, n));
            }
        } else if dynm {
            // hash-D: a key whose PUC hash was never computed is not in any table
            // n = the table: V7 = (hash & DYN_MASKS[lsizenode]) * sizeof(Node) + [n + NODE]
            self.ld(t1, W::B1, rm(n, table::LSIZENODE as i32));
            self.push(Op::Mov(7, super::ir::DYN_MASKS.as_ptr() as u64));
            self.push(Op::LdI {
                dst: 7,
                w: W::B4,
                m: crate::ir::Mi {
                    base: 7,
                    idx: t1,
                    scale: 4,
                    off: 0,
                },
            });
            self.ld(
                t1,
                W::B4,
                rm(k, rn_core::jit_layout::string::SHORT_HASH as i32),
            );
            if mutants::active("dynmask_mask_off_by_one") {
                self.push(Op::And(7, Src::Imm(-2))); // MUTANT: bit 0 of the mask dropped
            }
            self.push(Op::And(7, Src::V(t1)));
            self.push(Op::Mul(7, Src::Imm(node::SIZE as i32)));
            self.ld(t1, W::B8, rm(n, table::NODE as i32));
            self.push(Op::Add(7, Src::V(t1)));
        } else {
            self.ld(
                7,
                W::B4,
                rm(k, rn_core::jit_layout::string::SHORT_HASH as i32),
            );
            let m = if mutants::active("hrefv_mask_off_by_one") {
                mask >> 1
            } else {
                mask
            };
            self.push(Op::And(7, Src::Imm(m as i32)));
            self.push(Op::Mul(7, Src::Imm(node::SIZE as i32)));
            self.push(Op::Add(7, Src::V(n)));
        }
        self.bind(lloop);
        self.push(Op::BrM {
            cc: Cc::Ne,
            w: W::B1,
            m: rm(7, node::KEY_TT as i32),
            imm: if int_key {
                i32::from(LUA_VNUMINT as i8)
            } else {
                i32::from(ctb(LUA_VSHRSTR) as i8)
            },
            to: lnext,
        });
        self.ld(t1, W::B8, rm(7, node::KEY as i32));
        self.br(Cc::Eq, t1, Src::V(k), lfound);
        self.bind(lnext);
        self.ld(t1, W::B4, rm(7, node::NEXT as i32));
        self.br(Cc::Eq, t1, Src::Imm(0), lmiss);
        if mutants::active("hrefv_chain_stops_early") {
            // MUTANT: the walk gives up after the main position
            self.push(Op::Jmp(lmiss));
        }
        self.push(Op::Shl(t1, 32));
        self.push(Op::SarI(t1, 32));
        self.push(Op::Mul(t1, Src::Imm(node::SIZE as i32)));
        self.push(Op::Add(7, Src::V(t1)));
        self.push(Op::Jmp(lloop));
        if abs {
            let ldone = self.label();
            self.bind(lfound);
            if !mutants::active("habs_found_ignored") {
                self.restore_all();
                self.push(Op::Jmp(x.expect("an exit label")));
            } else {
                self.restore_all();
                self.push(Op::Jmp(ldone));
            }
            self.bind(lmiss);
            self.restore_all();
            self.bind(ldone);
            self.saved.clear();
            return Ok(());
        }
        let d = d_opt.expect("a destination register");
        if nilm {
            // DYNMASK: a miss yields the static nil node
            let lend = self.label();
            self.bind(lmiss);
            self.restore_all();
            self.push(Op::Mov(d, super::ir::NIL_NODE.as_ptr() as u64));
            self.push(Op::Jmp(lend));
            self.bind(lfound);
            self.push(Op::MovR(d, 7));
            self.restore_all();
            self.bind(lend);
            self.saved.clear();
            return Ok(());
        }
        self.bind(lmiss);
        self.restore_all();
        self.push(Op::Jmp(x.expect("an exit label")));
        self.bind(lfound);
        self.push(Op::MovR(d, 7));
        self.restore_all();
        self.saved.clear();
        Ok(())
    }

    /// `HMISS node, key, idx|delta<<8` (P-A): node `idx` does not hold the key (an exit when its key tag is `SHRSTR` and its
    /// address the constant) and its chain offset equals `delta` (else exit). Addresses the node vector register directly; no
    /// scratch register but `V7`-free, no result.
    fn hmiss(&mut self, i: usize) -> Result<(), AsmError> {
        let ins = self.ir.ins[i];
        let ops = self.ops(i);
        let (idx, delta) = hmiss_unpack(aux_bits(ins.aux));
        let off = match fits(idx as i64 * node::SIZE as i64) {
            Some(o) => o,
            None => return unsup("HMISS slot index"),
        };
        let Loc::G(n) = ops[0] else {
            return unsup("HMISS of a constant node vector");
        };
        let Loc::Const(kr) = ops[1] else {
            return unsup("HMISS with a key that is not a constant");
        };
        let key = self.kbits_of(kr);
        let x = self.guard_label(i)?;
        if !mutants::active("hmiss_next_skip") {
            self.push(Op::BrM {
                cc: Cc::Ne,
                w: W::B4,
                m: rm(n, off + node::NEXT as i32),
                imm: delta,
                to: x,
            });
        }
        let ok = self.label();
        self.push(Op::BrM {
            cc: Cc::Ne,
            w: W::B1,
            m: rm(n, off + node::KEY_TT as i32),
            imm: i32::from(ctb(LUA_VSHRSTR) as i8),
            to: ok,
        });
        if !mutants::active("hmiss_key_skip") && n != 7 && self.env.hk_key64 {
            // J1-1: the 64-bit key address in one compare against a register (`movabs v7, key; cmp [node + KEY], v7; je exit`)
            let k = if mutants::active("hmiss_key64_hi") {
                key ^ (1 << 40)
            } else {
                key
            };
            self.push(Op::Mov(7, k));
            self.push(Op::BrMR {
                cc: Cc::Eq,
                m: rm(n, off + node::KEY as i32),
                src: 7,
                to: x,
            });
        } else if !mutants::active("hmiss_key_skip") {
            self.push(Op::BrM {
                cc: Cc::Ne,
                w: W::B4,
                m: rm(n, off + node::KEY as i32),
                imm: key as u32 as i32,
                to: ok,
            });
            self.push(Op::BrM {
                cc: Cc::Eq,
                w: W::B4,
                m: rm(n, off + node::KEY as i32 + 4),
                imm: (key >> 32) as u32 as i32,
                to: x,
            });
        }
        self.bind(ok);
        Ok(())
    }

    /// `HLOAD hrefk`: type check of the value tag (bit 0 of the op bits), then the payload.
    fn hload(&mut self, i: usize) -> Result<(), AsmError> {
        let ins = self.ir.ins[i];
        let ops = self.ops(i);
        let Loc::G(addr) = ops[0] else {
            return unsup("HLOAD of a constant address");
        };
        // F3b: bit 1 selects the key fields of the node (the pseudo node of `TNEXT`)
        let (tt_off, val_off) = if aux_bits(ins.aux) & 2 != 0 {
            (node::KEY_TT, node::KEY)
        } else {
            (node::VAL_TT, node::VAL)
        };
        let fused = self.hk_fused.take() == Some(i);
        if aux_bits(ins.aux) & 1 != 0 && !fused && !mutants::active("hload_typecheck_skip") {
            let x = self.guard_label(i)?;
            match ins.t {
                Ty::Nil => {
                    // any nil variant (nil, empty): `tag & 0x0F == 0`
                    self.ld(7, W::B1, rm(addr, tt_off as i32));
                    self.push(Op::And(7, Src::Imm(0x0F)));
                    self.br(Cc::Ne, 7, Src::Imm(0), x);
                }
                t if ins.flags & F_ANYSTR != 0 => {
                    // N5: any string variant (the variant bit masked)
                    let Some(tag) = tag_of(t) else {
                        return unsup(format!("HLOAD of type {t:?}"));
                    };
                    self.ld(7, W::B1, rm(addr, tt_off as i32));
                    self.push(Op::And(7, Src::Imm(ANYSTR_MASK)));
                    self.br(Cc::Ne, 7, Src::Imm(i32::from(tag & ANYSTR_MASK as u8)), x);
                }
                t => {
                    let Some(tag) = tag_of(t) else {
                        return unsup(format!("HLOAD of type {t:?}"));
                    };
                    self.push(Op::BrM {
                        cc: Cc::Ne,
                        w: W::B1,
                        m: rm(addr, tt_off as i32),
                        imm: i32::from(tag as i8),
                        to: x,
                    });
                }
            }
        }
        let m = rm(addr, val_off as i32);
        match self.dst(i) {
            Loc::G(d) => {
                if matches!(ins.t, Ty::Nil | Ty::False | Ty::True) {
                    self.push(Op::Mov(d, 0));
                } else {
                    self.ld(d, W::B8, m);
                }
            }
            Loc::X(d) => self.push(Op::LdF { dst: d, m }),
            _ => {}
        }
        Ok(())
    }

    /// `HSTORE hrefk, v`: with the old-value check (guard, bit 0) the store exits when the node's value is nil (that
    /// is an insertion or a `__newindex` for the interpreter); then tag and payload.
    fn hstore(&mut self, i: usize) -> Result<(), AsmError> {
        let ins = self.ir.ins[i];
        let ops = self.ops(i);
        let Loc::G(addr) = ops[0] else {
            return unsup("HSTORE to a constant address");
        };
        if ins.flags & F_GUARD != 0
            && aux_bits(ins.aux) & 1 != 0
            && !mutants::active("hstore_oldcheck_skip")
        {
            let x = self.guard_label(i)?;
            self.ld(7, W::B1, rm(addr, node::VAL_TT as i32));
            self.push(Op::And(7, Src::Imm(0x0F)));
            self.br(Cc::Eq, 7, Src::Imm(0), x);
        }
        let t = self.ir.ty_of(ins.b);
        let Some(tag) = tag_of(t) else {
            return unsup(format!("HSTORE of type {t:?}"));
        };
        let known = self.env.b2_tag && self.ktag.iter().any(|&(r, k)| r == ins.a && k == tag);
        if !known {
            self.st(
                W::B1,
                rm(addr, node::VAL_TT as i32),
                Src::Imm(i32::from(tag)),
            );
        }
        self.store_value(rm(addr, node::VAL as i32), ins.b, ops[1]);
        Ok(())
    }

    /// Payload store of value `r` at `loc` (nil / booleans store 0).
    fn store_value(&mut self, m: M, r: Ref, loc: Loc) {
        let t = self.ir.ty_of(r);
        if matches!(t, Ty::Nil | Ty::False | Ty::True) {
            // B4-1 (`nopay`): the payload of these tags is never read; a mutant stores nothing for every type
            if !self.env.nopay {
                self.st(W::B8, m, Src::Imm(0));
            }
            return;
        }
        if self.env.nopay && mutants::active("nopay_all_types") {
            return;
        }
        match loc {
            Loc::G(n) => self.st(W::B8, m, Src::V(n)),
            Loc::X(n) => self.push(Op::StF { m, src: n }),
            Loc::Const(c) => {
                let b = self.kbits_of(c);
                self.store_const(m, b);
            }
            l => panic!("operand {l:?}"),
        }
    }

    /// `d = UpVal*` of upvalue `idx` of closure `cl` (registers or constants).
    fn upval_ptr(&mut self, cl: Loc, idx: Loc, d: V) -> Result<(), AsmError> {
        let c = match cl {
            Loc::G(c) => c,
            Loc::Const(_) => {
                self.reg(cl, d);
                d
            }
            l => panic!("operand {l:?}"),
        };
        match idx {
            Loc::Const(r) => {
                let j = self.kbits_of(r) as i64;
                self.ld(d, W::B8, rm(c, lclosure::UPVALS as i32 + 8 * j as i32));
            }
            Loc::G(j) => self.push(Op::LdI {
                dst: d,
                w: W::B8,
                m: Mi {
                    base: c,
                    idx: j,
                    scale: 8,
                    off: lclosure::UPVALS as i32,
                },
            }),
            l => panic!("operand {l:?}"),
        }
        Ok(())
    }

    /// `ULOAD`, `USTORE`, `UVREL`: `V7` = the address of the upvalue's value.
    fn uop(&mut self, i: usize) -> Result<(), AsmError> {
        let ins = self.ir.ins[i];
        let ops = self.ops(i);
        let ar = self.ir.at(ins.a);
        let fused = ar.op == TOp::Uref && self.al.fused[usize::from(ins.a - REF_BIAS)];
        // the `+exitstorm` check of the typecheck uses V7: emit it before V7 holds the value address
        let check_l = if ins.op == TOp::Uload && aux_bits(ins.aux) & 1 != 0 {
            Some(self.guard_label(i)?)
        } else {
            None
        };
        let (vals, vslot) = if fused {
            self.upval_ptr(ops[0], ops[1], 7)?;
            (ops[2], 2)
        } else {
            match ops[0] {
                Loc::G(u) => self.push(Op::MovR(7, u)),
                l => panic!("operand {l:?}"),
            }
            (ops[1], 1)
        };
        let _ = vslot;
        self.ld(7, W::B8, rm(7, upval::V as i32));
        match ins.op {
            TOp::Uvrel => {
                let d = self.g(self.dst(i));
                self.push(Op::Lea(
                    d,
                    M {
                        base: Base::Frame,
                        off: 0,
                    },
                ));
                // d = frame; V7 - d
                self.push(Op::Sub(7, Src::V(d)));
                self.push(Op::MovR(d, 7));
            }
            TOp::Uload => {
                if let Some(x) = check_l {
                    let tag = match tag_of(ins.t) {
                        Some(t) => t,
                        None => return unsup(format!("ULOAD of type {:?}", ins.t)),
                    };
                    self.push(Op::BrM {
                        cc: Cc::Ne,
                        w: W::B1,
                        m: rm(7, 0),
                        imm: i32::from(tag as i8),
                        to: x,
                    });
                }
                match self.dst(i) {
                    Loc::G(d) => {
                        if matches!(ins.t, Ty::Nil | Ty::False | Ty::True) {
                            self.push(Op::Mov(d, 0));
                        } else {
                            self.ld(d, W::B8, rm(7, 8));
                        }
                    }
                    Loc::X(d) => self.push(Op::LdF {
                        dst: d,
                        m: rm(7, 8),
                    }),
                    _ => {}
                }
            }
            _ => {
                let t = self.ir.ty_of(ins.b);
                let tag = match tag_of(t) {
                    Some(g) => g,
                    None => return unsup(format!("USTORE of type {t:?}")),
                };
                self.st(W::B8, rm(7, 0), Src::Imm(i32::from(tag)));
                self.store_value(rm(7, 8), ins.b, vals);
            }
        }
        Ok(())
    }

    // ---- helper calls (7.5) ----------------------------------------------------------------------------------------

    fn call_site(&self, i: usize) -> Result<CallSite, AsmError> {
        match self.al.calls.iter().find(|c| c.at == i) {
            Some(c) => Ok(*c),
            None => incons(format!("no call site for instruction {i}")),
        }
    }

    /// Stores the live registers to the exit area.
    fn save_live(&mut self, c: &CallSite) {
        self.save_vb();
        for g in 0..super::ra::NGPR as u8 {
            if c.live.gpr >> g & 1 != 0 && g != 7 {
                self.st(
                    W::B8,
                    area_m(exit_area::GPR + 8 * usize::from(g)),
                    Src::V(g),
                );
            }
        }
        for x in 0..16u8 {
            if c.live.xmm >> x & 1 != 0 {
                self.push(Op::StF {
                    m: area_m(exit_area::XMM + 8 * usize::from(x)),
                    src: x,
                });
            }
        }
    }

    /// The budget register around a `CallH` (which destroys every register).
    fn save_vb(&mut self) {
        if let Some(r) = self.env.budget_reg {
            self.st(
                W::B8,
                area_m(exit_area::GPR + 8 * usize::from(r)),
                Src::V(r),
            );
        }
    }

    fn reload_vb(&mut self) {
        if let Some(r) = self.env.budget_reg {
            self.ld(r, W::B8, area_m(exit_area::GPR + 8 * usize::from(r)));
        }
    }

    fn reload_live(&mut self, c: &CallSite) {
        self.reload_vb();
        for g in 0..super::ra::NGPR as u8 {
            if c.live.gpr >> g & 1 != 0 && g != 7 {
                self.ld(g, W::B8, area_m(exit_area::GPR + 8 * usize::from(g)));
            }
        }
        for x in 0..16u8 {
            if c.live.xmm >> x & 1 != 0 {
                self.push(Op::LdF {
                    dst: x,
                    m: area_m(exit_area::XMM + 8 * usize::from(x)),
                });
            }
        }
    }

    fn calln(&mut self, i: usize) -> Result<(), AsmError> {
        let ins = self.ir.ins[i];
        let ops = self.ops(i);
        let site = self.call_site(i)?;
        let d = Self::xreg(self.dst(i));
        // arguments first (operands that die here are not in `live`), then the live registers
        for (k, a) in [ops[0], ops[1]].into_iter().enumerate() {
            let m = area_m(exit_area::ARGS + 8 * k);
            match a {
                Loc::X(n) => self.push(Op::StF { m, src: n }),
                Loc::Const(r) => {
                    let b = self.kbits_of(r);
                    self.push(Op::Mov(7, b));
                    self.st(W::B8, m, Src::V(7));
                }
                Loc::None => {}
                l => panic!("operand {l:?}"),
            }
        }
        self.save_live(&site);
        self.push(Op::CallH {
            f: self.env.calln,
            a: ins.aux,
            b: 0,
        });
        self.reload_live(&site);
        self.push(Op::LdF {
            dst: d,
            m: area_m(exit_area::ARGS),
        });
        Ok(())
    }

    /// Is `ins` the guard `NE %n, 0` on an inline `NEWREF` (A1)?
    fn after_inline_newref(&self, ins: Ins) -> bool {
        !TraceIr::is_ins(ins.b)
            && self.ir.kval(ins.b) == KVal::Int(0)
            && TraceIr::is_ins(ins.a)
            && self.ir.at(ins.a).op == TOp::Newref
            && self.ir.at(ins.a).aux & nrplan::INLINE != 0
    }

    /// A1: the inline `NEWREF` of a constant short-string key into a table that this trace allocated and whose node vector is in
    /// the state the recorder tracked: what `insertkey` does there, as stores at the node positions of the recorded plan (the key
    /// tag and address, the chain links, the `lastfree` pointer, the table's `flags` after `invalidateTMcache`). The value stays
    /// empty until the `HSTORE` that follows; the table is white (just allocated), so there is no key barrier. `dst` = the new
    /// node's address, written last (it may share a register with the table).
    #[cold]
    #[inline(never)]
    fn newref_inline(&mut self, i: usize) -> Result<(), AsmError> {
        let ins = self.ir.ins[i];
        let ops = self.ops(i);
        let plan = nrplan::decode(ins.aux);
        let Loc::G(t) = ops[0] else {
            return unsup("inline NEWREF of a constant table");
        };
        let Loc::Const(kr) = ops[1] else {
            return unsup("inline NEWREF of a key that is not a constant");
        };
        let key = self.kbits_of(kr);
        let d = self.g(self.dst(i));
        let at = |idx: u8| i32::from(idx) * node::SIZE as i32;
        self.ld(7, W::B8, rm(t, table::NODE as i32));
        let nb = at(plan.node);
        if !mutants::active("nr_key_skip") {
            self.st(
                W::B1,
                rm(7, nb + node::KEY_TT as i32),
                Src::Imm(i32::from(ctb(LUA_VSHRSTR) as i8)),
            );
            self.store_const(rm(7, nb + node::KEY as i32), key);
        }
        if plan.node != plan.main && !mutants::active("nr_next_skip") {
            self.st(
                W::B4,
                rm(7, nb + node::NEXT as i32),
                Src::Imm(plan.node_next),
            );
            self.st(
                W::B4,
                rm(7, at(plan.main) + node::NEXT as i32),
                Src::Imm(plan.main_next),
            );
        }
        if !mutants::active("nr_flags_skip") {
            self.st(
                W::B1,
                rm(t, table::FLAGS as i32),
                Src::Imm(i32::from(plan.flags as i8)),
            );
        }
        if let Some(l) = plan.lastfree {
            self.push(Op::Lea(d, rm(7, at(l))));
            self.st(
                W::B8,
                rm(7, -(rn_core::jit_layout::node::LASTFREE_BYTES as i32)),
                Src::V(d),
            );
        }
        self.push(Op::Lea(d, rm(7, nb)));
        Ok(())
    }

    /// The class-A helpers (TR1, F3): `TNEW`, `NEWREF`, `TLEN` through `jit_tr_alloc`. Operands go to
    /// `ExitArea.args`, the live registers are bracketed like a `CALLN`, the result comes back in `args[0]` (0 = "did
    /// nothing", the guard that follows exits).
    fn class_a(&mut self, i: usize) -> Result<(), AsmError> {
        let ins = self.ir.ins[i];
        let ops = self.ops(i);
        let site = self.call_site(i)?;
        let d = self.g(self.dst(i));
        // 5.4: no inline `#t` (`tlen_fast` replicates the 5.5 length hint and its one-step moves; 5.4 `luaH_getn` moves `alimit` under other rules
        // (`ispow2realasize`, `limitequalsasize`), so the inline answer / store diverged on tables with holes: BUNDLE-VA, `tlen_inline` under
        // `--cfg rustynail_lua54`); the helper call is exact
        // A1: `setmetatable` of a table without a metatable to a metatable without `__gc`: a store (the helper call below is the slow path)
        let fast_done = if ins.op == TOp::Tlen
            && !cfg!(rustynail_lua54)
            && self.ir.ty_of(ins.a) == Ty::Tab
            && self.env.alloc == jit_tr_alloc as *const () as usize as u64
        {
            self.tlen_fast(&ops, d)
        } else if ins.op == TOp::Lib && ins.aux & 0xFF == lib::SETMT && self.env.setmt_inline {
            self.setmt_fast(&ops, d)
        } else if ins.op == TOp::Lib && ins.aux & 0xFF == lib::CCALLS && self.env.ccalls_inline {
            self.ccalls_fast(&ops, d)
        } else if ins.op == TOp::Lib
            && ins.aux & 0xFF == lib::TYPE
            && self.env.libcall & 4 != 0
            && self.env.alloc == jit_tr_alloc as *const () as usize as u64
        {
            self.type_fast(&ops, d)
        } else if ins.op == TOp::Lib && ins.aux & 0xFF == lib::BYTE && self.env.f2_byte {
            self.byte_fast(&ops, d, self.ir.ty_of(ins.a) == Ty::LngStr)
        } else if ins.op == TOp::Lib
            && ins.aux & 0xFF == lib::STREQ
            && self.env.f2_byte
            && self.ir.ty_of(ins.a) == Ty::LngStr
            && self.ir.ty_of(ins.b) == Ty::LngStr
        {
            self.streq_fast(&ops, d)
        } else {
            None
        };
        let (a, y, nops) = match ins.op {
            TOp::Tnew => (ALLOC_TNEW | u32::from(ins.a) << 8, ins.aux, 0),
            TOp::Newref => (ALLOC_NEWREF, 0, 2),
            TOp::Strto => {
                let st = self.ir.ty_of(ins.a);
                if !matches!(st, Ty::ShrStr | Ty::LngStr) {
                    return unsup(format!("STRTO of type {st:?}"));
                }
                (
                    ALLOC_STRTO | u32::from(st == Ty::LngStr) << 8 | ins.aux << 9,
                    0,
                    1,
                )
            }
            TOp::Snew => {
                let nt = self.ir.ty_of(ins.a);
                if !matches!(nt, Ty::Int | Ty::Num) {
                    return unsup(format!("SNEW of type {nt:?}"));
                }
                (ALLOC_SNEW | u32::from(nt == Ty::Num) << 8, 0, 1)
            }
            TOp::Concat => {
                let n = if ins.aux != 0 { 3 } else { 2 };
                let refs = [ins.a, ins.b, ins.aux as Ref];
                let mut bits = 0u32;
                for (k, &r) in refs.iter().enumerate().take(n) {
                    match self.ir.ty_of(r) {
                        Ty::ShrStr => {}
                        Ty::LngStr => bits |= 1 << k,
                        // B3-4 (`cnum`): a number operand (the recorder emits no `SNEW` for it)
                        Ty::Int => bits |= 1 << (5 + k),
                        Ty::Num => bits |= 1 << (8 + k),
                        t => return unsup(format!("CONCAT of an operand of type {t:?}")),
                    }
                }
                if n == 3 {
                    bits |= 8;
                }
                if ins.t == Ty::LngStr {
                    bits |= 16;
                }
                (
                    if bits & 0x7E0 != 0 {
                        ALLOC_CONCATN
                    } else {
                        ALLOC_CONCAT
                    } | bits << 8,
                    0,
                    n,
                )
            }
            TOp::Lib => {
                let kind = ins.aux & 0xFF;
                let mut fl = 0u32;
                if self.ir.ty_of(ins.a) == Ty::LngStr {
                    fl |= 1;
                }
                if ins.t == Ty::LngStr {
                    fl |= 2;
                }
                if self.ir.ty_of(ins.a) == Ty::Num {
                    fl |= 4;
                }
                if matches!(
                    kind,
                    lib::STREQ | lib::STRLT | lib::STRLE | lib::FIND | lib::FINDS
                ) && self.ir.ty_of(ins.b) == Ty::LngStr
                {
                    fl |= 8;
                }
                // BUNDLE-H: the separator of `REPS` is a long string (bit 4)
                if kind == lib::REPS && self.ir.ty_of((ins.aux >> 8) as Ref) == Ty::LngStr {
                    fl |= 16;
                }
                // BUNDLE-C (`tfmt`): the flags are the argument types (2 bits each from bit 4) and the result kind
                if lib::is_fmt(kind) || kind == lib::FMTX {
                    fl = u32::from(ins.t == Ty::LngStr) << 1;
                    for (j, r) in [ins.a, ins.b, (ins.aux >> 8) as Ref]
                        .into_iter()
                        .enumerate()
                    {
                        let c = match self.ir.ty_of(r) {
                            Ty::Int => 0,
                            Ty::ShrStr => 1,
                            Ty::LngStr => 2,
                            _ => 3,
                        };
                        fl |= c << (4 + 2 * j);
                    }
                }
                let n = match kind {
                    k if lib::is_fmt(k) || k == lib::FMTX => 3,
                    lib::SUB | lib::RANDOM | lib::FIND | lib::FINDS | lib::CHAR3 | lib::REPS => 3,
                    lib::BYTE
                    | lib::TYPE
                    | lib::IPAIRS_END
                    | lib::SETMT
                    | lib::STREQ
                    | lib::STRLT
                    | lib::STRLE
                    | lib::CHAR2
                    | lib::REP => 2,
                    lib::CHAR
                    | lib::STRMETH
                    | lib::TYPENAME
                    | lib::TOSTR
                    | lib::CCALLS
                    | lib::GMATCH
                    | lib::RANDF
                    | lib::LOWER
                    | lib::UPPER
                    | lib::REVERSE => 1,
                    k => return unsup(format!("LIB kind {k}")),
                };
                (ALLOC_LIB | (kind | fl << 8) << 8, 0, n)
            }
            TOp::Tnext => {
                let ct = self.ir.ty_of(ins.b);
                let Some(tag) = tag_of(ct) else {
                    return unsup(format!("TNEXT of a control value of type {ct:?}"));
                };
                (ALLOC_TNEXT | u32::from(tag) << 8, 0, 2)
            }
            TOp::Tset => {
                let vt = self.ir.ty_of(ins.aux as Ref);
                let Some(tag) = tag_of(vt) else {
                    return unsup(format!("TSET of type {vt:?}"));
                };
                (ALLOC_TSET | u32::from(tag) << 8, 0, 3)
            }
            _ => {
                let y = match self.ir.ty_of(ins.a) {
                    Ty::Tab => 0,
                    Ty::ShrStr => 1,
                    Ty::LngStr => 2,
                    t => return unsup(format!("TLEN of {t:?}")),
                };
                (ALLOC_TLEN, y, 1)
            }
        };
        for (k, loc) in ops.iter().take(nops).enumerate() {
            let m = area_m(exit_area::ARGS + 8 * k);
            match *loc {
                Loc::G(n) => self.st(W::B8, m, Src::V(n)),
                Loc::X(n) => self.push(Op::StF { m, src: n }),
                Loc::Const(r) => {
                    let b = self.kbits_of(r);
                    self.push(Op::Mov(7, b));
                    self.st(W::B8, m, Src::V(7));
                }
                l => panic!("operand {l:?}"),
            }
        }
        self.save_live(&site);
        // F1: `TNEXT` has its own lean entry (test doubles of `alloc` keep their own)
        let real = self.env.alloc == jit_tr_alloc as *const () as usize as u64;
        let f = if real && ins.op == TOp::Tnext {
            jit_tr_tnext as *const () as usize as u64
        } else if real && ins.op == TOp::Newref {
            jit_tr_newref as *const () as usize as u64
        } else if real && ins.op == TOp::Tset {
            jit_tr_tset as *const () as usize as u64
        } else if real && ins.op == TOp::Tlen && y == 0 {
            jit_tr_tlen as *const () as usize as u64
        } else if real && ins.op == TOp::Lib && ins.aux & 0xFF == lib::SUB && self.env.lean_sub {
            jit_tr_lib_sub as *const () as usize as u64
        } else if let Some(lean) = (real && ins.op == TOp::Lib && self.env.libcall & 2 != 0)
            .then(|| lean_lib_entry(ins.aux & 0xFF, self.env.libcall))
            .flatten()
        {
            lean
        } else {
            self.env.alloc
        };
        self.push(Op::CallH { f, a, b: y });
        self.reload_live(&site);
        self.ld(d, W::B8, area_m(exit_area::ARGS));
        if let Some(l) = fast_done {
            self.push(Op::Bind(l));
        }
        Ok(())
    }

    /// The inline form of `#t` for a table (`TableRef::len`): the length hint `h` (the `u32` at `array + 0`) is the answer when `h >= 1`,
    /// `arr[h-1]` is non-nil and either `h < asize` and `arr[h]` is nil, or `h == asize` and the hash part is the dummy node; an empty
    /// table (`asize == 0` with a dummy hash part) answers 0. The two one-step moves of the hint that the helper makes for a push or a
    /// pop (`t[#t + 1] = v`, `t[#t] = nil`) are replicated, store included: `arr[h]` non-nil with `h + 1 < asize` and `arr[h+1]` nil:
    /// hint and answer `h + 1`; `arr[h-1]` nil with `h >= 2` and `arr[h-2]` non-nil: hint and answer `h - 1`. Every other state (hint 0,
    /// any further search, a populated hash part) takes the helper call, the slow path, so the other hint side effects are the
    /// helper's. Only `V7`, the destination and (spilled when it is not the destination) one more register are written; the
    /// destination is written last.
    #[cold]
    #[inline(never)]
    fn tlen_fast(&mut self, ops: &[Loc], d: V) -> Option<L> {
        let Loc::G(t) = ops[0] else {
            return None;
        };
        let (r, spilled) = if d != t {
            (d, None)
        } else {
            let r = (0..7u8).find(|&r| r != t && r != d)?;
            let m = spill_m(self.save_slot(self.saved.len()));
            self.st(W::B8, m, Src::V(r));
            (r, Some(m))
        };
        let (slow, slow_r, done, got, full, empty) = (
            self.label(),
            self.label(),
            self.label(),
            self.label(),
            self.label(),
            self.label(),
        );
        let (pop, push, zero, pscan, pend, pnil) = (
            self.label(),
            self.label(),
            self.label(),
            self.label(),
            self.label(),
            self.label(),
        );
        self.ld(7, W::B4, rm(t, table::ASIZE as i32));
        self.br(Cc::Eq, 7, Src::Imm(0), empty);
        self.ld(r, W::B8, rm(t, table::ARRAY as i32));
        self.ld(r, W::B4, rm(r, table::LENHINT as i32));
        self.br(Cc::Eq, r, Src::Imm(0), zero);
        self.br(Cc::Gt, r, Src::V(7), slow_r);
        // arr[h-1] nil: the hint is above the border (a pop)
        if !mutants::active("tlen_inline_prev_skip") {
            self.tlen_tag(t, r, -1);
            self.br(Cc::Eq, 7, Src::Imm(0), pop);
        }
        self.ld(7, W::B4, rm(t, table::ASIZE as i32));
        self.br(Cc::Eq, 7, Src::V(r), full);
        // h < asize: arr[h] nil -> h; non-nil (a push) -> `push`
        if !mutants::active("tlen_inline_next_skip") {
            self.tlen_tag(t, r, 0);
            self.br(Cc::Ne, 7, Src::Imm(0), push);
        }
        self.push(Op::Jmp(got));
        // h == asize, or no array part at all: the dummy hash part
        for (l, zero) in [(full, false), (empty, true)] {
            self.push(Op::Bind(l));
            if !mutants::active("tlen_inline_dummy_skip") {
                self.ld(7, W::B1, rm(t, table::FLAGS as i32));
                self.push(Op::And(7, Src::Imm(i32::from(table::BITDUMMY))));
                self.br(Cc::Eq, 7, Src::Imm(0), slow_r);
            }
            if zero {
                self.push(Op::Mov(r, 0));
            } else {
                self.push(Op::Jmp(got));
            }
        }
        self.push(Op::Jmp(got));
        // hint 0 (constructor tables: `asize / 2` is the initial hint, 0 for one slot): the helper starts at limit 1, so slot 0 nil is a
        // binary search (helper); otherwise it takes the upward scan below, which finishes inside the four steps it allows when
        // `asize <= 5`
        self.push(Op::Bind(zero));
        if !mutants::active("tlen_inline_zero_bound_skip") {
            self.br(Cc::Gt, 7, Src::Imm(5), slow_r);
        }
        self.push(Op::Mov(r, 1));
        self.tlen_tag(t, r, -1);
        self.br(Cc::Eq, 7, Src::Imm(0), slow_r);
        self.push(Op::Jmp(pscan));
        // pop (arr[h-1] nil): the helper's first step down: arr[h-2] non-nil -> hint and answer h-1
        self.push(Op::Bind(pop));
        self.br(Cc::Lt, r, Src::Imm(2), slow_r);
        self.tlen_tag(t, r, -2);
        self.br(Cc::Eq, 7, Src::Imm(0), slow_r);
        self.push(Op::Sub(r, Src::Imm(1)));
        self.tlen_set_hint(t, r);
        self.push(Op::Jmp(got));
        // push (h < asize, arr[h] non-nil): the helper's upward scan, one slot per step, at most four steps. Its second step (slot h+1)
        // is the common case (`t[#t + 1] = v` leaves the hint one behind): a nil there, or the array's end, is answered at once; a
        // longer scan is exact when `asize - h <= 4` (it then ends at the array's end, not at the step limit)
        self.push(Op::Bind(push));
        self.push(Op::Add(r, Src::Imm(1)));
        self.ld(7, W::B4, rm(t, table::ASIZE as i32));
        self.br(Cc::Eq, r, Src::V(7), pend);
        self.tlen_tag(t, r, 0);
        self.br(Cc::Eq, 7, Src::Imm(0), pnil);
        if !mutants::active("tlen_inline_zero_bound_skip") {
            self.ld(7, W::B4, rm(t, table::ASIZE as i32));
            self.push(Op::Sub(7, Src::V(r)));
            self.br(Cc::Gt, 7, Src::Imm(3), slow_r);
        }
        self.push(Op::Add(r, Src::Imm(1)));
        // invariant: slots 0..r-1 up to the hint are set and `arr[r-1]` is set, r <= asize
        self.push(Op::Bind(pscan));
        self.ld(7, W::B4, rm(t, table::ASIZE as i32));
        self.br(Cc::Eq, r, Src::V(7), pend);
        self.tlen_tag(t, r, 0);
        self.br(Cc::Eq, 7, Src::Imm(0), pnil);
        self.push(Op::Add(r, Src::Imm(1)));
        self.push(Op::Jmp(pscan));
        self.push(Op::Bind(pnil));
        self.tlen_set_hint(t, r);
        self.push(Op::Jmp(got));
        // the scan reached the array's end (`arr[asize-1]` set): hint = asize, the answer if the hash part is the dummy node
        self.push(Op::Bind(pend));
        self.ld(7, W::B1, rm(t, table::FLAGS as i32));
        self.push(Op::And(7, Src::Imm(i32::from(table::BITDUMMY))));
        self.br(Cc::Eq, 7, Src::Imm(0), slow_r);
        self.tlen_set_hint(t, r);
        self.push(Op::Bind(got));
        if let Some(m) = spilled {
            // the answer is in `r` (a scratch that is not the destination): move it, then give `r` its value back
            self.push(Op::MovR(7, r));
            self.ld(r, W::B8, m);
            self.push(Op::MovR(d, 7));
        }
        self.push(Op::Jmp(done));
        self.push(Op::Bind(slow_r));
        if let Some(m) = spilled {
            self.ld(r, W::B8, m);
        }
        self.push(Op::Bind(slow));
        Some(done)
    }

    /// `V7` = the type nibble (0 = nil) of the array slot `r + k` (0-based) of table `t` (`r` = the hint, so slot `r - 1` is the last one it
    /// covers); the slot must be inside the array.
    fn tlen_tag(&mut self, t: V, r: V, k: i32) {
        self.ld(7, W::B8, rm(t, table::ARRAY as i32));
        self.push(Op::Add(7, Src::V(r)));
        self.ld(7, W::B1, rm(7, table::ARRAY_TAG_BASE as i32 + k));
        self.push(Op::And(7, Src::Imm(0x0F)));
    }

    /// `array[0..4] = r`: the length hint of table `t` (`V7` clobbered).
    fn tlen_set_hint(&mut self, t: V, r: V) {
        if mutants::active("tlen_inline_hint_skip") {
            return;
        }
        self.ld(7, W::B8, rm(t, table::ARRAY as i32));
        self.st(W::B4, rm(7, table::LENHINT as i32), Src::V(r));
    }

    /// A1: the inline form of `setmetatable(t, mt)` (`lib_core`'s `SETMT` does the same in the helper): when the table `t` is white (the
    /// collector's barrier would do nothing), has no metatable (so no `__metatable` field to protect), and `mt` is a table whose cached
    /// "no `__gc`" bit is set (`luaC_checkfinalizer` would return at once) or the constant 0 (no metatable), the pointer is stored
    /// and `d` = 1; every other case falls through to the helper call that follows (label `slow` is bound at the end of this
    /// sequence). Returns the label the helper path must bind after it (`None`: no fast path for these operands). Only `V7` and `d`
    /// are used, `d` is written last.
    #[cold]
    #[inline(never)]
    fn setmt_fast(&mut self, ops: &[Loc], d: V) -> Option<L> {
        let Loc::G(t) = ops[0] else {
            return None;
        };
        let mt_bits = match ops[1] {
            Loc::G(_) => None,
            Loc::Const(r) => Some(self.kbits_of(r)),
            _ => return None,
        };
        let slow = self.label();
        let done = self.label();
        if mt_bits != Some(0) && !mutants::active("smi_gcbit_skip") {
            // `fast_tm(mt, Gc)` is `None` without a lookup only when the "absent" bit is cached
            match ops[1] {
                Loc::G(m) => self.ld(7, W::B1, rm(m, table::FLAGS as i32)),
                _ => {
                    self.push(Op::Mov(7, mt_bits.unwrap_or(0)));
                    self.ld(7, W::B1, rm(7, table::FLAGS as i32));
                }
            }
            self.push(Op::And(7, Src::Imm(1 << rn_core::meta::Tm::Gc as u8)));
            self.br(Cc::Eq, 7, Src::Imm(0), slow);
        }
        if !mutants::active("smi_black_skip") {
            self.ld(7, W::B1, rm(t, table::MARKED as i32));
            self.push(Op::And(7, Src::Imm(i32::from(BLACKBIT))));
            self.br(Cc::Ne, 7, Src::Imm(0), slow);
        }
        if !mutants::active("smi_old_skip") {
            self.ld(7, W::B8, rm(t, table::METATABLE as i32));
            self.br(Cc::Ne, 7, Src::Imm(0), slow);
        }
        match (ops[1], mt_bits) {
            (_, Some(0)) => {}
            (Loc::G(m), _) => self.st(W::B8, rm(t, table::METATABLE as i32), Src::V(m)),
            (_, Some(b)) => {
                self.push(Op::Mov(7, b));
                self.st(W::B8, rm(t, table::METATABLE as i32), Src::V(7));
            }
            _ => return None,
        }
        self.push(Op::Mov(d, 1));
        self.push(Op::Jmp(done));
        self.push(Op::Bind(slow));
        Some(done)
    }

    /// BUNDLE-D (`ccalls`): the inline form of the C-stack check `LIB CCALLS k` (`lib_core` answers `ccalls + k + CCALLS_MARGIN < LUAI_MAXCCALLS`): the thread's
    /// `nCcalls & 0xFFFF` is compared with `LUAI_MAXCCALLS - k - CCALLS_MARGIN`; below it the answer is 1 (a register move), otherwise the helper
    /// call (the slow path) answers, which also makes the guard behind it exit so the interpreter raises "C stack overflow" itself. Writes `V7` and the destination.
    fn ccalls_fast(&mut self, ops: &[Loc], d: V) -> Option<L> {
        let Loc::Const(r) = ops[0] else {
            return None;
        };
        let k = self.kbits_of(r) as i64;
        let bound = i64::from(rn_core::state::LUAI_MAXCCALLS) - k - CCALLS_MARGIN as i64
            + i64::from(mutants::active("ccalls_inline_off_by_one"));
        if !(1..=i64::from(i32::MAX)).contains(&bound) {
            return None;
        }
        let (slow, done) = (self.label(), self.label());
        self.ld(
            7,
            W::B4,
            M {
                base: Base::Thread,
                off: thr::NCALLS as i32,
            },
        );
        self.push(Op::And(7, Src::Imm(0xffff)));
        self.br(Cc::Ge, 7, Src::Imm(bound as i32), slow);
        self.push(Op::Mov(d, 1));
        self.push(Op::Jmp(done));
        self.push(Op::Bind(slow));
        Some(done)
    }

    /// BUNDLE-H (`libcall` bit 2): the inline form of `type(v)` (`LIB TYPE`, `a` = the C closure, `b` = the constant type index `k`): when the closure's function
    /// is `type`'s the answer is the string in upvalue `k` (a load), anything else takes the helper call (the slow path). Writes `V7` and the destination
    /// (the second scratch until the answer; a register that is not the closure and is saved when the destination is the closure).
    fn type_fast(&mut self, ops: &[Loc], d: V) -> Option<L> {
        let (Loc::G(c), Loc::Const(kr)) = (ops[0], ops[1]) else {
            return None;
        };
        let k = self.kbits_of(kr) as i64;
        let tf = rn_core::jit_hooks::type_fn_addr();
        if !(0..9).contains(&k) || tf == 0 {
            return None;
        }
        let (r, spilled) = if d != c {
            (d, None)
        } else {
            let r = (0..7u8).find(|&r| r != c && r != d)?;
            let m = spill_m(self.save_slot(self.saved.len()));
            self.st(W::B8, m, Src::V(r));
            (r, Some(m))
        };
        let (slow, slow_r, done) = (self.label(), self.label(), self.label());
        self.ld(7, W::B8, rm(c, rn_core::jit_layout::cclosure::F as i32));
        self.push(Op::Mov(r, tf as u64));
        if !mutants::active("libk_type_fn_skip") {
            self.br(Cc::Ne, 7, Src::V(r), slow_r);
        }
        if let Some(m) = spilled {
            self.ld(r, W::B8, m);
        }
        // MUTANT `libk_type_index_off`: the neighbouring upvalue (the next type's name)
        let k = if mutants::active("libk_type_index_off") {
            k + 1
        } else {
            k
        };
        self.ld(
            d,
            W::B8,
            rm(
                c,
                (rn_core::jit_layout::cclosure::UPVALS as i64 + 16 * k + 8) as i32,
            ),
        );
        self.push(Op::Jmp(done));
        self.push(Op::Bind(slow_r));
        if let Some(m) = spilled {
            self.ld(r, W::B8, m);
        }
        self.push(Op::Bind(slow));
        Some(done)
    }

    /// F2: the inline form of `string.byte(s, i)` (`lib_core`'s `BYTE` does the same in the helper): for `1 <= i <= #s` (and a long string
    /// whose bytes are inline) the result is the byte plus one, a load; anything else takes the helper call (the slow path), which also
    /// answers 0 for "no result". Only `V7` and the destination are written, the destination last.
    fn byte_fast(&mut self, ops: &[Loc], d: V, long: bool) -> Option<L> {
        let Loc::G(s) = ops[0] else {
            return None;
        };
        let slow = self.label();
        let done = self.label();
        let data = rn_core::jit_layout::string::CONTENTS as i32;
        if long {
            // inline long strings only (`shrlen == -1`); an external buffer takes the helper
            self.ld(7, W::B1, rm(s, rn_core::jit_layout::string::SHRLEN as i32));
            self.br(Cc::Ne, 7, Src::Imm(0xFF), slow);
            self.ld(7, W::B8, rm(s, rn_core::jit_layout::string::LNGLEN as i32));
        } else {
            self.ld(7, W::B1, rm(s, rn_core::jit_layout::string::SHRLEN as i32));
        }
        match ops[1] {
            Loc::G(i) => {
                self.br(Cc::Lt, i, Src::Imm(1), slow);
                if !mutants::active("byte_inline_len_skip") {
                    self.br(Cc::Lt, 7, Src::V(i), slow);
                }
                self.push(Op::MovR(7, i));
                self.push(Op::Add(7, Src::V(s)));
                let off = if mutants::active("byte_inline_off_by_one") {
                    data
                } else {
                    data - 1
                };
                self.ld(d, W::B1, rm(7, off));
            }
            Loc::Const(r) => {
                let k = self.kbits_of(r) as i64;
                if k < 1 || k > i64::from(i32::MAX) / 2 {
                    return None;
                }
                if !mutants::active("byte_inline_len_skip") {
                    self.br(Cc::Lt, 7, Src::Imm(k as i32), slow);
                }
                let off = if mutants::active("byte_inline_off_by_one") {
                    data + k as i32
                } else {
                    data + k as i32 - 1
                };
                self.ld(d, W::B1, rm(s, off));
            }
            _ => return None,
        }
        self.push(Op::Add(d, Src::Imm(1)));
        self.push(Op::Jmp(done));
        self.push(Op::Bind(slow));
        Some(done)
    }

    /// BUNDLE-G1: the inline form of `a == b` of two long strings (`lib_core`'s `STREQ` does the full compare in the helper): the same
    /// object is equal; two inline long strings of different lengths are unequal; two of the same length whose last eight bytes differ
    /// are unequal (a difference is usually at the end of strings that share a prefix); everything else (equal tails, an external
    /// buffer, a short string) takes the helper call, the slow path. Writes `V7` and the destination (a scratch until the answer, so it
    /// must differ from both operand registers), which is 2 (equal) or 1 (unequal) as the helper's answer.
    fn streq_fast(&mut self, ops: &[Loc], d: V) -> Option<L> {
        let (Loc::G(s), Loc::G(t)) = (ops[0], ops[1]) else {
            return None;
        };
        // the second scratch register: the destination, unless it is one of the operands (the allocator reuses the register of a dying
        // operand, the usual case: both strings were loaded just before); then a register that is neither, saved in a save slot
        let (r, spilled) = if d != s && d != t {
            (d, None)
        } else {
            let r = (0..7u8).find(|&r| r != s && r != t && r != d)?;
            let m = spill_m(self.save_slot(self.saved.len()));
            self.st(W::B8, m, Src::V(r));
            (r, Some(m))
        };
        let (slow, slow_r, done, eql, ne) = (
            self.label(),
            self.label(),
            self.label(),
            self.label(),
            self.label(),
        );
        let data = rn_core::jit_layout::string::CONTENTS as i32;
        self.br(Cc::Eq, s, Src::V(t), eql);
        // inline long strings only (`shrlen == -1`)
        self.ld(7, W::B1, rm(s, rn_core::jit_layout::string::SHRLEN as i32));
        self.br(Cc::Ne, 7, Src::Imm(0xFF), slow);
        self.ld(7, W::B1, rm(t, rn_core::jit_layout::string::SHRLEN as i32));
        self.br(Cc::Ne, 7, Src::Imm(0xFF), slow);
        self.ld(r, W::B8, rm(s, rn_core::jit_layout::string::LNGLEN as i32));
        self.ld(7, W::B8, rm(t, rn_core::jit_layout::string::LNGLEN as i32));
        self.br(
            Cc::Ne,
            r,
            Src::V(7),
            if mutants::active("streq_inline_len_diff_eq") {
                eql
            } else {
                ne
            },
        );
        // a long string has more than 40 bytes: the eight-byte read at the end is inside the contents
        self.br(Cc::Lt, r, Src::Imm(8), slow_r);
        self.push(Op::MovR(7, r));
        self.push(Op::Add(7, Src::V(s)));
        self.ld(7, W::B8, rm(7, data - 8));
        self.push(Op::Add(r, Src::V(t)));
        self.ld(r, W::B8, rm(r, data - 8));
        self.br(
            Cc::Ne,
            7,
            Src::V(r),
            if mutants::active("streq_inline_tail_ne_eq") {
                eql
            } else {
                ne
            },
        );
        if mutants::active("streq_inline_tail_eq") {
            self.push(Op::Jmp(eql));
        } else {
            self.push(Op::Jmp(slow_r));
        }
        self.push(Op::Bind(eql));
        if let Some(m) = spilled {
            self.ld(r, W::B8, m);
        }
        self.push(Op::Mov(d, 2));
        self.push(Op::Jmp(done));
        self.push(Op::Bind(ne));
        if let Some(m) = spilled {
            self.ld(r, W::B8, m);
        }
        self.push(Op::Mov(d, 1));
        self.push(Op::Jmp(done));
        // the helper call: the scratch register (when it is not the destination) gets its value back first
        self.push(Op::Bind(slow_r));
        if let Some(m) = spilled {
            self.ld(r, W::B8, m);
        }
        self.push(Op::Bind(slow));
        Some(done)
    }

    fn tbar(&mut self, i: usize) -> Result<(), AsmError> {
        let ins = self.ir.ins[i];
        let ops = self.ops(i);
        let vt = self.ir.ty_of(ins.b);
        if !collectable(vt) {
            return Ok(()); // nothing to protect
        }
        if mutants::active("tbar_skip") || (ins.aux & 1 != 0 && mutants::active("upbar_asm_skip")) {
            return Ok(());
        }
        let site = self.call_site(i)?;
        let done = self.label();
        // table black?
        match ops[0] {
            Loc::G(t) => self.ld(7, W::B1, rm(t, table::MARKED as i32)),
            Loc::Const(r) => {
                let b = self.kbits_of(r);
                self.push(Op::Mov(7, b));
                self.ld(7, W::B1, rm(7, table::MARKED as i32));
            }
            l => panic!("operand {l:?}"),
        }
        self.push(Op::And(7, Src::Imm(i32::from(BLACKBIT))));
        self.br(Cc::Eq, 7, Src::Imm(0), done);
        // value white?
        match ops[1] {
            Loc::G(v) => self.ld(7, W::B1, rm(v, table::MARKED as i32)),
            Loc::Const(r) => {
                let b = self.kbits_of(r);
                self.push(Op::Mov(7, b));
                self.ld(7, W::B1, rm(7, table::MARKED as i32));
            }
            l => panic!("operand {l:?}"),
        }
        self.push(Op::And(7, Src::Imm(i32::from(WHITEBITS))));
        self.br(Cc::Eq, 7, Src::Imm(0), done);
        // slow path: `barrier_back(table, (tag, payload))`, bracketed
        for (k, a) in [ops[0], ops[1]].into_iter().enumerate() {
            let m = area_m(exit_area::ARGS + 8 * k);
            match a {
                Loc::G(n) => self.st(W::B8, m, Src::V(n)),
                Loc::Const(r) => {
                    let b = self.kbits_of(r);
                    self.push(Op::Mov(7, b));
                    self.st(W::B8, m, Src::V(7));
                }
                l => panic!("operand {l:?}"),
            }
        }
        self.save_live(&site);
        let tag = tag_of(vt).expect("a collectable type has a tag");
        self.push(Op::CallH {
            f: self.env.barrier,
            a: u32::from(tag),
            b: u32::from(ins.aux & 1), // (BUNDLE-V: 1 = the barrier of an upvalue)
        });
        self.reload_live(&site);
        self.bind(done);
        Ok(())
    }

    /// `ATOUCH tab u` (5.4, TR30): the `alimit_touch` sequence of `templates_table.rs` on the table `ops[0]` and the
    /// 0-based index `ops[1]` (already known `< asize`): while `flags & BITRAS`, `u >= lim && (u & !(lim - 1)) < lim`
    /// makes `u + 1` the new limit (the `u32` at `array + 0`). Fast path (no `BITRAS`): `V7` only. The slow part
    /// saves the temporaries it needs in extension slots and restores them at its single join point. Mutant
    /// `alimit_touch`: the new limit is not stored.
    #[cfg(rustynail_lua54)]
    fn atouch(&mut self, i: usize) -> Result<(), AsmError> {
        let ops = self.ops(i);
        let done0 = self.label();
        let done = self.label();
        // flags & BITRAS (V7 only)
        let t_reg = match ops[0] {
            Loc::G(t) => {
                self.ld(7, W::B1, rm(t, table::FLAGS as i32));
                Some(t)
            }
            Loc::Const(r) => {
                let b = self.kbits_of(r);
                self.push(Op::Mov(7, b));
                self.ld(7, W::B1, rm(7, table::FLAGS as i32));
                None
            }
            l => panic!("operand {l:?}"),
        };
        self.push(Op::And(7, Src::Imm(i32::from(table::BITRAS))));
        self.br(Cc::Eq, 7, Src::Imm(0), done0);
        // temporaries: `s` (the limit), and `tt` for a constant table
        let mut used: Vec<V> = Vec::new();
        for l in [ops[0], ops[1]] {
            if let Loc::G(n) = l {
                used.push(n);
            }
        }
        let mut pick = |cx: &Self| -> V {
            let v = (0..=6u8)
                .find(|v| !used.contains(v) && Some(*v) != cx.env.budget_reg)
                .expect("a free temporary register");
            used.push(v);
            v
        };
        let s = pick(self);
        let tt = if t_reg.is_none() {
            Some(pick(self))
        } else {
            None
        };
        self.saved.clear();
        self.save(Loc::G(s));
        if let Some(tt) = tt {
            self.save(Loc::G(tt));
        }
        let t = match (t_reg, tt) {
            (Some(t), _) => t,
            (None, Some(tt)) => {
                if let Loc::Const(r) = ops[0] {
                    let b = self.kbits_of(r);
                    self.push(Op::Mov(tt, b));
                }
                tt
            }
            _ => unreachable!(),
        };
        // the index as a `Src` (a constant index fits 32 bits: it is below `asize`)
        let u: Src = match ops[1] {
            Loc::G(n) => Src::V(n),
            Loc::Const(r) => match fits(self.kbits_of(r) as i64) {
                Some(k) if k >= 0 => Src::Imm(k),
                _ => return unsup("ATOUCH of a constant index beyond 32 bits"),
            },
            l => panic!("operand {l:?}"),
        };
        self.ld(s, W::B8, rm(t, table::ARRAY as i32));
        self.ld(s, W::B4, rm(s, table::LENHINT as i32));
        // lim > u: nothing to update
        self.br(Cc::Gt, s, u, done);
        // is (u & !(lim - 1)) < lim ?
        self.push(Op::MovR(7, s));
        self.push(Op::Sub(7, Src::Imm(1)));
        self.push(Op::Not(7));
        self.push(Op::And(7, u));
        self.br(Cc::Ge, 7, Src::V(s), done);
        if !mutants::active("alimit_touch") {
            // new limit u + 1 (V7 = u + 1, stored through the re-read array pointer)
            match u {
                Src::V(n) => {
                    self.push(Op::MovR(7, n));
                    self.push(Op::Add(7, Src::Imm(1)));
                }
                Src::Imm(k) => self.push(Op::Mov(7, k as u64 + 1)),
            }
            self.ld(s, W::B8, rm(t, table::ARRAY as i32));
            self.st(W::B4, rm(s, table::LENHINT as i32), Src::V(7));
        }
        self.bind(done);
        self.restore_all();
        self.saved.clear();
        self.bind(done0);
        Ok(())
    }

    /// `ATOUCH` exists only in the 5.4 build (the 5.5 recorder never emits it).
    #[cfg(not(rustynail_lua54))]
    fn atouch(&mut self, _i: usize) -> Result<(), AsmError> {
        unsup("ATOUCH (Lua 5.4 only)")
    }

    // ---- BUNDLE-K L1 (`lazyci`) ----------------------------------------------------------------------------------------

    /// The region root word (`ExitArea::lazy_root`).
    fn lroot_m() -> M {
        M {
            base: Base::Cnt,
            off: exit_area::FROM_R15 + exit_area::LAZY_ROOT as i32,
        }
    }

    /// Label of the shared leave block: `V6` = the address to continue at, every other register dead. It completes the lazy chain
    /// (`exit::lazy_leave`) and jumps to `V6`; `new_frame`: the innermost frame was pushed natively just now.
    fn lz_leave_label(&mut self, new_frame: bool) -> L {
        let k = usize::from(new_frame);
        if let Some(l) = self.lz_leave[k] {
            return l;
        }
        let l = self.label();
        self.lz_leave[k] = Some(l);
        l
    }

    /// `V6 = target; jmp leave block` (registers are dead).
    fn lazy_leave_then(&mut self, target: u64, new_frame: bool) {
        let l = self.lz_leave_label(new_frame);
        self.push(Op::Mov(6, target));
        self.push(Op::Jmp(l));
    }

    /// The completion sequence of a lazy chain, emitted in line (every register is dead; `V0..V2` are used; `save_v6`: `V6` survives the helper).
    /// `new_frame == false` first tests for the common case without lazy frames (no region, or the running frame is the root).
    fn lazy_leave_seq(&mut self, new_frame: bool, save_v6: bool) {
        let lroot = Self::lroot_m();
        let thw = |o: usize| M {
            base: Base::Thread,
            off: o as i32,
        };
        let call = self.label();
        let done = self.label();
        if !new_frame {
            // no lazy frames: the frame is complete; the thread follows it
            let plain = self.label();
            self.ld(0, W::B8, lroot);
            self.br(Cc::Eq, 0, Src::Imm(0), plain);
            self.push(Op::Lea(
                1,
                M {
                    base: Base::Ci,
                    off: 0,
                },
            ));
            self.br(Cc::Ne, 0, Src::V(1), call);
            self.bind(plain);
            self.push(Op::Lea(
                1,
                M {
                    base: Base::Ci,
                    off: 0,
                },
            ));
            self.st(W::B8, thw(thr::CI), Src::V(1));
            self.st(W::B8, lroot, Src::V(1));
            self.ld(2, W::B8, rm(1, ci::TOP as i32));
            self.st(W::B8, thw(thr::TOP), Src::V(2));
            self.push(Op::Jmp(done));
        }
        self.bind(call);
        if let (Some(cs), true) = (
            self.env.lazy_cs,
            self.env.root_maxstack != 0 && (!new_frame || self.env.root_code != 0),
        ) {
            {
                // the frames above the root are all of the root's prototype: `top`, `kcache`, `trap` and `callstatus` are constants of it (V0 = root, V1 = the frame)
                let (lp, fin) = (self.label(), self.label());
                let top_extra = 1;
                if new_frame {
                    self.ld(0, W::B8, lroot);
                }
                self.ld(3, W::B8, rm(0, ci::KCACHE as i32));
                self.push(Op::Lea(
                    1,
                    M {
                        base: Base::Ci,
                        off: 0,
                    },
                ));
                self.bind(lp);
                self.br(Cc::Eq, 1, Src::V(0), fin);
                self.ld(2, W::B8, rm(1, ci::FUNC as i32));
                self.push(Op::Add(
                    2,
                    Src::Imm(top_extra + i32::from(self.env.root_maxstack)),
                ));
                self.st(W::B8, rm(1, ci::TOP as i32), Src::V(2));
                self.st(W::B8, rm(1, ci::KCACHE as i32), Src::V(3));
                self.st(W::B4, rm(1, ci::CALLSTATUS as i32), Src::Imm(cs as i32));
                self.st(W::B4, rm(1, ci::TRAP as i32), Src::Imm(0));
                self.ld(1, W::B8, rm(1, ci::PREVIOUS as i32));
                self.push(Op::Jmp(lp));
                self.bind(fin);
                self.push(Op::Lea(
                    1,
                    M {
                        base: Base::Ci,
                        off: 0,
                    },
                ));
                self.st(W::B8, thw(thr::CI), Src::V(1));
                self.st(W::B8, lroot, Src::V(1));
                if new_frame {
                    // a frame pushed just now: it starts at the first instruction of its prototype and `thread.top` is the end of its arguments
                    self.push(Op::Mov(3, self.env.root_code));
                    self.st(W::B8, rm(1, ci::SAVEDPC as i32), Src::V(3));
                    self.ld(2, W::B8, rm(1, ci::FUNC as i32));
                    self.push(Op::Add(2, Src::Imm(1 + i32::from(self.env.lazy_nparams))));
                } else {
                    self.ld(2, W::B8, rm(1, ci::TOP as i32));
                }
                self.st(W::B8, thw(thr::TOP), Src::V(2));
                self.push(Op::Jmp(done));
            }
        }
        if save_v6 {
            self.st(W::B8, area_m(exit_area::ARGS), Src::V(6));
        }
        self.push(Op::CallH {
            f: self.env.lazy_leave_fn,
            a: u32::from(new_frame),
            b: 0,
        });
        if save_v6 {
            self.ld(6, W::B8, area_m(exit_area::ARGS));
        }
        self.bind(done);
    }

    /// The bodies of the shared leave blocks requested so far.
    fn emit_lazy_leave_blocks(&mut self) {
        for k in 0..2usize {
            let Some(l) = self.lz_leave[k] else { continue };
            self.bind(l);
            self.lazy_leave_seq(k == 1, true);
            self.push(Op::JmpR(6));
        }
    }

    // ---- stubs (7.6) ---------------------------------------------------------------------------------------------------

    fn stubs_and_tail(&mut self, entry_fail: L, rec_fail: Option<L>) -> Result<(), AsmError> {
        // TR6 failures: entry-failure bookkeeping, then tier 1 at pcl[H]
        self.bind(entry_fail);
        self.entry_failure_ops(false, true);
        self.push(Op::Mov(7, self.env.pcl_h));
        self.push(Op::JmpR(7));
        if let Some(l) = rec_fail {
            // P-M3: the room checks of a natively pushed frame failed: tier 1 continues at the pre-count label of the head
            // in the new frame (a legal fresh frame); the budget register holds tier 1's state at that label
            self.bind(l);
            if let Some(r) = self.env.budget_reg {
                self.st(
                    W::B4,
                    M {
                        base: Base::Cnt,
                        off: 0,
                    },
                    Src::V(r),
                );
            }
            if self.env.lazy && !mutants::active("lazy_recfail_no_leave") {
                // BUNDLE-K L1: the chain is completed first; the frame of a function-entry head was pushed natively just now
                let nf = self.env.head_pc == 0;
                self.lazy_leave_then(self.env.pcl_h, nf);
            } else {
                self.push(Op::Mov(7, self.env.pcl_h));
                self.push(Op::JmpR(7));
            }
        }
        // J1-1: the cold disambiguators of the fused `HREFK`+`HLOAD` pairs (registers are as at the failed compare): a key tag
        // mismatch is the `HREFK` guard, otherwise the value tag was wrong
        for c in std::mem::take(&mut self.hk_cold) {
            self.bind(c.label);
            let (kx, vx) = if mutants::active("hkfuse_cold_swap") {
                (c.hload_exit, c.hrefk_exit)
            } else {
                (c.hrefk_exit, c.hload_exit)
            };
            self.push(Op::BrM {
                cc: Cc::Ne,
                w: W::B1,
                m: c.m,
                imm: i32::from(ctb(LUA_VSHRSTR) as i8),
                to: kx,
            });
            self.push(Op::Jmp(vx));
        }
        // P2-3a: the refund stubs of the budget (registers and flags are dead at the exit; only the budget register changes)
        for (label, exit, n) in std::mem::take(&mut self.bud_cold) {
            self.bind(label);
            let r = self
                .env
                .budget_reg
                .expect("a refund stub needs the budget register");
            let back = if mutants::active("budget_refund_short") {
                n - 1
            } else {
                n
            };
            if !mutants::active("budget_refund_skip") {
                self.push(Op::Add(r, Src::Imm(back)));
            }
            self.push(Op::Jmp(exit));
        }
        let mut shared: Option<L> = None;
        let disps = std::mem::take(&mut self.disps);
        for d in &disps {
            // V7 is dead at every exit
            self.bind(d.label);
            self.ld(7, W::B8, spill_m(self.ext_slot(FIRST_SLOT_K)));
            self.br(Cc::Ne, 7, Src::Imm(0), d.entry);
            self.push(Op::Jmp(d.plain));
        }
        let mut k = 0;
        // stubs are only requested while the instructions are assembled, so the list is final here
        while k < self.stubs.len() {
            let (snap, entry, label, poll) = {
                let s = &self.stubs[k];
                (s.snap, s.entry, s.label, s.poll)
            };
            k += 1;
            self.nstubs += 1;
            let es = self.snaps[snap as usize].clone();
            if !es.usable {
                return incons(format!("snapshot {snap} is used by a guard but unusable"));
            }
            self.bind(label);
            if poll {
                self.generic_stub(snap, false, true, &mut shared);
                continue;
            }
            if entry {
                self.entry_failure_ops(true, false);
            }
            let regs: Vec<u8> = es
                .slots
                .iter()
                .map(|s| s.src)
                .chain(
                    es.sunk
                        .iter()
                        .flat_map(|t| t.fields.iter().map(|f| f.src).chain(t.mt.map(|m| m.1))),
                )
                .filter_map(|src| {
                    if let ValSrc::Reg(r) = src {
                        Some(r)
                    } else {
                        None
                    }
                })
                .collect();
            // (the same scratch register `inline_stub` looks for: one that no slot reads and that is not the budget register)
            let scratch_free = [0u8, 1, 2, 3, 4, 5, 6, 8]
                .iter()
                .any(|r| !regs.contains(r) && Some(*r) != self.env.budget_reg);
            let counting = self.env.counters != 0;
            // P-L: without a free scratch register the exit counter is kept with memory-destination ops (`cmp/add [ctr]`,
            // V7 only); the pre-roll test of Q2 needs a value in a register, so those sites keep the generic stub
            let frame_cell = self.env.frame_cells
                && self.env.exit_cells != 0
                && snap != self.env.frame_link_snap
                && es.frame_side_ok();
            // J1-2: a frameless exit that materialises sunk tables is restored by its inline stub (slot stores, then the light
            // helper `jit_tr_mat` creates the tables); the hot exits and the policy keep going through the handler (the generic twin)
            let lz = self.env.lazy;
            let sunk_stub = !es.sunk.is_empty()
                && !lz
                && self.env.sink_stub
                && self.env.mat_fn != 0
                && !mutants::active("sinkstub_never")
                && exit::sunk_stub_ok(&es)
                && !entry
                && !matches!(
                    self.sites.get(snap as usize),
                    Some(super::cache::ExitSite::Alloc)
                );
            // J1-2: the same for a frameful exit (the callee of an inlined constructor): hook test, the counter of the `fmat` stub, slot
            // stores, budget, the helper, then the frames are rebuilt in place (`mat_shared`) and the stub lands in tier 1
            let site = self.sites.get(snap as usize).copied();
            let sunk_frames = !es.sunk.is_empty()
                && !lz
                && self.env.sink_stub
                && self.env.mat_fn != 0
                && !mutants::active("sinkstub_never")
                && !mutants::active("sinkstub_frames_never")
                && exit::sunk_frame_stub_ok(&es)
                && self.env.root_code != 0
                && self.env.counters != 0
                && !self.env.generic_only
                && !entry
                && match site {
                    Some(super::cache::ExitSite::TypeGuard) => self.env.hot_exit >= 2,
                    Some(super::cache::ExitSite::Other) => {
                        self.env.hot_exit.is_power_of_two() && self.env.hot_exit >= 2
                    }
                    _ => false,
                };
            if sunk_frames {
                let generic_l = self.label();
                self.sunk_frame_stub(&es, snap, generic_l, site.expect("site"), frame_cell)?;
                self.bind(generic_l);
                self.generic_stub(snap, entry, false, &mut shared);
                continue;
            }
            // BUNDLE-K L1: a lazy-aware trace has inline stubs only for frameless exits that continue through their cell (the stub
            // completes the chain before it lands at the tier-1 label); every other exit goes through the handler, which does
            let lz_ok = !lz
                || ((es.frames.is_empty() || frame_cell)
                    && es.sunk.is_empty()
                    && self.env.exit_cells != 0);
            let inline = (es.frames.is_empty() || frame_cell)
                && lz_ok
                && !(self.env.seg_oracle && es.seg_pops > 0)
                && (es.sunk.is_empty() || sunk_stub)
                && es.top.is_none()
                && !self.env.generic_only
                && (scratch_free || !counting || entry || !self.pre_snaps[snap as usize]);
            // H2: while the cell is 0 (no side trace attached) the stub rebuilds the inlined frames itself instead of calling the
            // handler; the handler keeps the exits that do bookkeeping per exit (entry checks, allocation sites, pre-roll sites,
            // hooked threads) and the report at the hot counts
            let site = self.sites.get(snap as usize).copied();
            let mat = inline
                && frame_cell
                && counting
                && self.env.root_code != 0
                && !entry
                && !self.pre_snaps[snap as usize]
                && exit::frame_mat_ok(&es)
                && self.env.counters != 0
                && match site {
                    Some(super::cache::ExitSite::TypeGuard) => self.env.hot_exit >= 2,
                    Some(super::cache::ExitSite::Other) => {
                        self.env.hot_exit.is_power_of_two() && self.env.hot_exit >= 2
                    }
                    _ => false,
                };
            // N3b: the linking exit of a side trace (frames rebuilt, then the head's dispatch cell): an inline stub instead of the handler
            let link_mat = self.env.link_mat
                && snap == self.env.frame_link_snap
                && !lz
                && !entry
                && !self.env.generic_only
                && !(self.env.seg_oracle && es.seg_pops > 0)
                && self.env.root_code != 0
                && self.env.counters != 0
                && !self.pre_snaps[snap as usize]
                && exit::frame_link_mat_ok(&es);
            if link_mat {
                let generic_l = self.label();
                self.link_mat_stub(&es, snap, generic_l)?;
                self.bind(generic_l);
                self.generic_stub(snap, entry, false, &mut shared);
                continue;
            }
            if inline {
                let generic_l = self.label();
                self.inline_stub(
                    &es,
                    snap,
                    generic_l,
                    entry,
                    frame_cell,
                    if mat { site } else { None },
                )?;
                if counting || frame_cell {
                    // the generic twin the hot divert jumps to
                    self.bind(generic_l);
                    self.generic_stub(snap, entry, false, &mut shared);
                }
            } else {
                self.generic_stub(snap, entry, false, &mut shared);
            }
        }
        self.emit_lazy_leave_blocks();
        let chains = std::mem::take(&mut self.mat_frames);
        let labels: Vec<L> = self.mat_shared.iter().map(|(_, l)| *l).collect();
        let codes = std::mem::take(&mut self.mat_codes);
        for ((frames, l), code) in chains.iter().zip(labels).zip(codes) {
            self.bind(l);
            for op in exit::frame_mat_shared_ops(frames, code) {
                self.push(op);
            }
            self.push(Op::ReloadFrame {
                stack_off: self.env.stack_off,
            });
            self.ld(
                4,
                W::B8,
                rm(2, rn_core::jit_layout::callinfo::KCACHE as i32),
            );
            self.push(Op::SetK(4));
            self.push(Op::JmpR(6));
        }
        if let (Some(l), 0) = (shared, self.env.shared_exit) {
            // the embedded copy of the shared routine (every stub ends in a jump: nothing falls into it)
            let at = self.p.ops.len();
            exit::append_shared_exit(
                &mut self.p,
                &SharedExitEnv {
                    handler: self.env.exit_handler,
                    stack_off: self.env.stack_off,
                },
            );
            self.p.ops.insert(at, Op::Bind(l));
        }
        Ok(())
    }

    fn generic_stub(&mut self, snap: u32, entry: bool, poll: bool, shared: &mut Option<L>) {
        let exitno = snap
            | if entry { exit::EXIT_ENTRY } else { 0 }
            | if poll { exit::EXIT_POLL } else { 0 };
        let ops = exit::generic_stub_ops(self.env.trace_id, exitno, self.env.shared_exit);
        self.push(ops[0]);
        self.push(ops[1]);
        if self.env.shared_exit != 0 {
            self.push(ops[2]);
            self.push(ops[3]);
        } else {
            let l = *shared.get_or_insert_with(|| self.p.label());
            self.push(Op::Jmp(l));
        }
    }

    /// H2: the inline stub of a frameful exit with a frame cell: hook test, exit counter (the handler is still called at the hot
    /// counts), slot stores, budget; then a non-zero cell continues in the side trace (frames stay virtual) and a zero cell rebuilds
    /// the inlined frames in place (`exit::frame_mat_ops`) and lands at the tier-1 label. No store happens before the last
    /// divert to the handler, which does the whole restore itself.
    fn mat_stub(
        &mut self,
        es: &ExitSnap,
        snap: u32,
        generic_l: L,
        site: super::cache::ExitSite,
        cell: u64,
    ) -> Result<(), AsmError> {
        use super::cache::ExitSite;
        self.dbl.push(snap);
        let full = exit::inline_stub_ops_via(es, self.env.vb(), Some(cell));
        let mut bare = es.clone();
        bare.slots.clear();
        let tail = exit::inline_stub_ops_via(&bare, self.env.vb(), Some(cell));
        assert!(full.ends_with(&tail), "inline stub layout");
        let head = &full[..full.len() - tail.len()];
        let addr = self.env.counters + 4 * u64::from(snap);
        let hot1 = self.env.hot_exit.clamp(1, i32::MAX as u32) as i32 - 1;
        // (An exit of a frame-cell stub is counted twice: by the inline counter and again by the handler it used to call
        // through the generic twin, so every counter value is even and the handler sees `old + 2`. This stub keeps those values:
        // it adds 2 itself and, when the handler is wanted, 1 (the handler adds the other 1).)
        let common = self.label();
        let unatt = self.label();
        let fix = self.label();
        // a side trace is attached: count and continue in it (no report: the handler had nothing to do for it)
        self.push(Op::Mov(7, cell));
        self.ld(7, W::B8, rm(7, 0));
        self.br(Cc::Eq, 7, Src::Imm(0), unatt);
        self.push(Op::Mov(7, addr));
        if self
            .chain_snaps
            .get(snap as usize)
            .copied()
            .unwrap_or(false)
        {
            // P3: the exit that makes the counter `CHAIN_EXITS` goes through the handler (the counter is even here: the handler adds the second 1 after `fix`
            // added the first). The handler re-bases the counter, so that the test comes up every `CHAIN_EXITS` exits (`cache::chain_check`)
            if !mutants::active("chain_stub_no_test") {
                self.push(Op::BrM {
                    cc: Cc::Eq,
                    w: W::B4,
                    m: rm(7, 0),
                    imm: exit::CHAIN_EXITS as i32 - 2,
                    to: fix,
                });
            }
        }
        self.push(Op::AddM {
            w: W::B4,
            m: rm(7, 0),
            imm: 2,
        });
        self.push(Op::Jmp(common));
        self.bind(unatt);
        // a hooked thread: the rebuilt frames need `trap = 1`
        {
            self.ld(
                7,
                W::B1,
                M {
                    base: Base::Thread,
                    off: rn_core::jit_hooks::thread::HOOKMASK as i32,
                },
            );
            self.br(Cc::Ne, 7, Src::Imm(0), generic_l);
        }
        self.push(Op::Mov(7, addr));
        match site {
            ExitSite::TypeGuard => {
                // the handler reports every exit from the threshold on (the flush rule)
                self.push(Op::BrM {
                    cc: Cc::Ge,
                    w: W::B4,
                    m: rm(7, 0),
                    imm: hot1 - 1,
                    to: fix,
                });
            }
            _ => {
                // the handler reports every `hot_exit`-th exit (the side-trace attempt and its retries)
                if !mutants::active("fmat_mask_skip") {
                    self.ld(7, W::B4, rm(7, 0));
                    // (H3-A: the mask also keeps the `Black` bit, so a blacklisted exit never compares equal)
                    self.push(Op::And(
                        7,
                        Src::Imm(
                            (self.env.hot_exit as i32 - 1) | super::cache::EXIT_BLACK_BIT as i32,
                        ),
                    ));
                    self.br(Cc::Eq, 7, Src::Imm(self.env.hot_exit as i32 - 2), fix);
                    self.push(Op::Mov(7, addr));
                }
            }
        }
        self.push(Op::AddM {
            w: W::B4,
            m: rm(7, 0),
            imm: 2,
        });
        self.bind(common);
        for op in head {
            self.push(*op);
        }
        let (last, rest) = tail.split_last().expect("stub tail");
        for op in rest {
            self.push(*op);
        }
        let mat_l = self.label();
        self.br(Cc::Eq, 7, Src::Imm(0), mat_l);
        self.push(*last);
        self.bind(mat_l);
        if self.env.lazy && !mutants::active("lazy_mat_no_leave") {
            // BUNDLE-K L1: the lazy frames below the root frame of the trace are completed before the inlined frames are rebuilt above it
            self.lazy_leave_seq(false, false);
        }
        // (the chain is rebuilt by a copy shared by the snapshots with the same frames: V5 = the innermost `savedpc`, V6 = the landing)
        self.push(Op::Mov(
            5,
            exit::frame_mat_innermost_pc(es, self.seg_code(es)),
        ));
        self.push(Op::Mov(6, es.land_addr));
        let key: Vec<MatKey> = es
            .frames
            .iter()
            .map(|f| {
                (
                    f.func_slot,
                    f.nresults,
                    f.call_pc,
                    f.code as usize,
                    f.k as usize,
                    f.maxstack,
                    f.tail,
                )
            })
            .collect();
        let shared = self.mat_label(key, es);
        self.push(Op::Jmp(shared));
        // the exit the handler is wanted for: it counts it (the second 1)
        self.bind(fix);
        self.push(Op::Mov(7, addr));
        self.push(Op::AddM {
            w: W::B4,
            m: rm(7, 0),
            imm: 1,
        });
        self.push(Op::Jmp(generic_l));
        Ok(())
    }

    /// N3b: the inline stub of the frame-linking exit of a side trace (the "re-push"): hook test (a hooked thread goes through the handler),
    /// exit counter, slot stores and budget, then the shared frame rebuild (`exit::frame_mat_shared_ops`) with the landing `V6` = the content of the
    /// link target's dispatch cell (read now: it is whatever tier 1 would reach, a root trace entry or the tier-1 label).
    fn link_mat_stub(&mut self, es: &ExitSnap, snap: u32, generic_l: L) -> Result<(), AsmError> {
        let addr = self.env.counters + 4 * u64::from(snap);
        self.ld(
            7,
            W::B1,
            M {
                base: Base::Thread,
                off: rn_core::jit_hooks::thread::HOOKMASK as i32,
            },
        );
        self.br(Cc::Ne, 7, Src::Imm(0), generic_l);
        self.push(Op::Mov(7, addr));
        self.push(Op::AddM {
            w: W::B4,
            m: rm(7, 0),
            imm: 1,
        });
        let full = exit::inline_stub_ops_via(es, self.env.vb(), None);
        // (the last two ops are `Mov V7, land; JmpR V7`)
        for op in &full[..full.len() - 2] {
            self.push(*op);
        }
        self.push(Op::Mov(
            5,
            exit::frame_link_innermost_pc(es, self.seg_code(es)),
        ));
        let (target, through) = exit::frame_link_target(es);
        self.push(Op::Mov(7, target));
        if through {
            self.ld(6, W::B8, rm(7, 0));
        } else {
            self.push(Op::MovR(6, 7));
        }
        let key: Vec<MatKey> = es
            .frames
            .iter()
            .map(|f| {
                (
                    f.func_slot,
                    f.nresults,
                    f.call_pc,
                    f.code as usize,
                    f.k as usize,
                    f.maxstack,
                    f.tail,
                )
            })
            .collect();
        let shared = self.mat_label(key, es);
        self.push(Op::Jmp(shared));
        Ok(())
    }

    /// J1-2: the inline stub of a frameful exit that materialises sunk tables (see `stubs_and_tail`). The hot divert and the counter follow
    /// the `fmat` stub of the same site class (`mat_stub`) but count once per exit (no second count): a TypeGuard exit goes through the
    /// handler from the threshold on (the flush rule), any other exit every `hot_exit`-th time (the side-trace attempt and its retries,
    /// never when blacklisted). SINK-F: with a frame cell (`frame_cell`) a side trace may be attached; then the stub counts, stores the slots,
    /// materialises and jumps through the cell (read after the helper: a flush by its policy leaves 0 or the tier-1 landing, and the frames are rebuilt).
    fn sunk_frame_stub(
        &mut self,
        es: &ExitSnap,
        snap: u32,
        generic_l: L,
        site: super::cache::ExitSite,
        frame_cell: bool,
    ) -> Result<(), AsmError> {
        use super::cache::ExitSite;
        let addr = self.env.counters + 4 * u64::from(snap);
        let hot1 = self.env.hot_exit.clamp(1, i32::MAX as u32) as i32 - 1;
        // SINK-F: a snapshot with a frame cell may have a side trace attached (cell != 0): count (and keep the pre-roll test of the ineffective
        // rule) and go on to the slot stores and the helper; the jump through the cell after it keeps the frames virtual
        let cell = (self.env.exit_cells != 0 && frame_cell)
            .then(|| self.env.exit_cells + 8 * u64::from(snap));
        let common = self.label();
        if let Some(c) = cell {
            let unatt = self.label();
            self.push(Op::Mov(7, c));
            self.ld(7, W::B8, rm(7, 0));
            self.br(Cc::Eq, 7, Src::Imm(0), unatt);
            self.push(Op::Mov(7, addr));
            if self.pre_snaps[snap as usize] {
                self.ld(7, W::B4, rm(7, 0));
                self.push(Op::And(7, Src::Imm(exit::INEFF_EXITS as i32 - 1)));
                self.br(Cc::Eq, 7, Src::Imm(exit::INEFF_EXITS as i32 - 1), generic_l);
                self.push(Op::Mov(7, addr));
            }
            self.push(Op::AddM {
                w: W::B4,
                m: rm(7, 0),
                imm: 1,
            });
            if mutants::active("sunkframe_cell_nomat") {
                // (MUTANT: the side trace is entered without the slot stores and the tables)
                self.push(Op::Mov(7, c));
                self.ld(7, W::B8, rm(7, 0));
                self.push(Op::JmpR(7));
            }
            self.push(Op::Jmp(common));
            self.bind(unatt);
        }
        // a hooked thread: the rebuilt frames need `trap = 1`, and a hook may look at the stack: the handler does it all
        self.ld(
            7,
            W::B1,
            M {
                base: Base::Thread,
                off: rn_core::jit_hooks::thread::HOOKMASK as i32,
            },
        );
        self.br(Cc::Ne, 7, Src::Imm(0), generic_l);
        self.push(Op::Mov(7, addr));
        match site {
            ExitSite::TypeGuard => {
                self.push(Op::BrM {
                    cc: Cc::Ge,
                    w: W::B4,
                    m: rm(7, 0),
                    imm: hot1,
                    to: generic_l,
                });
            }
            _ => {
                self.ld(7, W::B4, rm(7, 0));
                self.push(Op::And(
                    7,
                    Src::Imm((self.env.hot_exit as i32 - 1) | super::cache::EXIT_BLACK_BIT as i32),
                ));
                self.br(Cc::Eq, 7, Src::Imm(hot1), generic_l);
                self.push(Op::Mov(7, addr));
            }
        }
        if self.pre_snaps[snap as usize] {
            // Q2: every INEFF_EXITS-th exit of a pre-roll site goes through the handler (the ineffective-trace rule, `cache::pre_exit_check`;
            // the handler counts that exit, so divert before the store)
            self.push(Op::Mov(7, addr));
            self.ld(7, W::B4, rm(7, 0));
            self.push(Op::And(7, Src::Imm(exit::INEFF_EXITS as i32 - 1)));
            self.br(Cc::Eq, 7, Src::Imm(exit::INEFF_EXITS as i32 - 1), generic_l);
            self.push(Op::Mov(7, addr));
        }
        self.push(Op::AddM {
            w: W::B4,
            m: rm(7, 0),
            imm: 1,
        });
        self.bind(common);
        // the slot stores and the budget of a frameless exit (the helper needs the frame slots in place, the frames come after)
        let mut flat = es.clone();
        flat.frames.clear();
        let full = exit::inline_stub_ops_via(&flat, self.env.vb(), None);
        let mut bare = flat.clone();
        bare.slots.clear();
        let tail = exit::inline_stub_ops_via(&bare, self.env.vb(), None);
        assert!(full.ends_with(&tail), "inline stub layout");
        // (the last two ops of the tail are `Mov V7, land; JmpR V7`)
        for op in &full[..full.len() - 2] {
            self.push(*op);
        }
        for op in exit::sunk_save_ops(es) {
            self.push(op);
        }
        let err = self.label();
        self.push(Op::CallH {
            f: self.env.mat_fn,
            a: snap,
            b: self.env.trace_id,
        });
        if !mutants::active("sinkstub_err_ignored") {
            self.push(Op::Br {
                cc: Cc::Ne,
                a: 0,
                b: Src::Imm(0),
                to: err,
            });
        }
        // the chain is rebuilt by a copy shared by the snapshots with the same frames (V5 = the innermost `savedpc`, V6 = the landing)
        self.push(Op::Mov(
            5,
            exit::frame_mat_innermost_pc(es, self.seg_code(es)),
        ));
        self.push(Op::Mov(6, es.land_addr));
        if let Some(c) = cell {
            // the tables exist: a side trace continues with the frames virtual. The cell is read AFTER the helper, whose policy may have flushed the
            // family: the cell is then 0 or the tier-1 landing again (as the handler tests it), and the frames are rebuilt below
            let rebuild = self.label();
            self.push(Op::Mov(7, c));
            self.ld(7, W::B8, rm(7, 0));
            self.br(Cc::Eq, 7, Src::Imm(0), rebuild);
            self.br(Cc::Eq, 7, Src::V(6), rebuild);
            self.push(Op::JmpR(7));
            self.bind(rebuild);
        }
        let key: Vec<MatKey> = es
            .frames
            .iter()
            .map(|f| {
                (
                    f.func_slot,
                    f.nresults,
                    f.call_pc,
                    f.code as usize,
                    f.k as usize,
                    f.maxstack,
                    f.tail,
                )
            })
            .collect();
        let shared = self.mat_label(key, es);
        self.push(Op::Jmp(shared));
        // a host allocation failed: the helper rebuilt the frames (`restore_chain`) and recorded the memory error
        self.bind(err);
        self.push(Op::Ret(rn_core::jit_hooks::JitExit::Error as u32));
        Ok(())
    }

    /// N2a: the code pointer of the root frame of `es`'s segment (`env.root_code` for segment 0).
    fn seg_code(&self, es: &ExitSnap) -> u64 {
        if es.root_code.is_null() {
            self.env.root_code
        } else {
            es.root_code as usize as u64
        }
    }

    /// The label of the shared frame-rebuild copy for `key` (the frames) with the segment's root code of `es`.
    fn mat_label(&mut self, key: Vec<MatKey>, es: &ExitSnap) -> L {
        let code = self.seg_code(es);
        if let Some(i) = (0..self.mat_shared.len())
            .find(|&i| self.mat_shared[i].0 == key && self.mat_codes[i] == code)
        {
            return self.mat_shared[i].1;
        }
        let l = self.label();
        self.mat_shared.push((key, l));
        self.mat_frames.push(es.frames.clone());
        self.mat_codes.push(code);
        l
    }

    /// The inline stub of `snap` with the exit counter and the hot divert to `generic_l` (7.6).
    fn inline_stub(
        &mut self,
        es: &ExitSnap,
        snap: u32,
        generic_l: L,
        entry_check: bool,
        frame_cell: bool,
        mat: Option<super::cache::ExitSite>,
    ) -> Result<(), AsmError> {
        let cell = (self.env.exit_cells != 0).then(|| self.env.exit_cells + 8 * u64::from(snap));
        if let (Some(site), Some(c)) = (mat, cell) {
            return self.mat_stub(es, snap, generic_l, site, c);
        }
        let full = exit::inline_stub_ops_via(es, self.env.vb(), cell);
        let mut bare = es.clone();
        bare.slots.clear();
        let tail = exit::inline_stub_ops_via(&bare, self.env.vb(), cell);
        assert!(full.ends_with(&tail), "inline stub layout");
        let head = &full[..full.len() - tail.len()];
        if self.env.counters != 0 {
            // a scratch register that no slot of this snapshot reads
            let used: Vec<u8> = es
                .slots
                .iter()
                .map(|s| s.src)
                .chain(
                    es.sunk
                        .iter()
                        .flat_map(|t| t.fields.iter().map(|f| f.src).chain(t.mt.map(|m| m.1))),
                )
                .filter_map(|src| {
                    if let ValSrc::Reg(r) = src {
                        Some(r)
                    } else {
                        None
                    }
                })
                .collect();
            let addr = self.env.counters + 4 * u64::from(snap);
            let sr = [0u8, 1, 2, 3, 4, 5, 6, 8]
                .into_iter()
                .find(|r| !used.contains(r) && Some(*r) != self.env.budget_reg);
            let hot1 = self.env.hot_exit.clamp(1, i32::MAX as u32) as i32 - 1;
            // P-L: the counter lives in memory (`cmp [ctr], hot-1; je divert; add [ctr], 1`: three ops, V7 only) except at
            // a pre-roll site (Q2), whose `And` test needs the value in a register
            let pre_site = !entry_check && self.pre_snaps[snap as usize];
            // P3: a frameless callee-guard exit (no frame cell: counts once per exit) diverts to the handler when the counter reaches `CHAIN_EXITS` (the handler re-bases it)
            let chain_site = !pre_site
                && !entry_check
                && !frame_cell
                && es.frames.is_empty()
                && es.sunk.is_empty()
                && self
                    .chain_snaps
                    .get(snap as usize)
                    .copied()
                    .unwrap_or(false);
            if let (true, Some(sr)) = (pre_site, sr) {
                self.push(Op::Mov(7, addr));
                self.ld(sr, W::B4, rm(7, 0));
                // `Eq`: the divert fires exactly once (the handler's count reaches `hot_exit`); afterwards the inline path
                // counts on (a `Ge` here would send every later exit through the handler)
                self.br(Cc::Eq, sr, Src::Imm(hot1), generic_l);
                if !entry_check && self.pre_snaps[snap as usize] {
                    // Q2: every INEFF_EXITS-th exit of a pre-roll site goes through the handler (the ineffective-trace
                    // rule, `cache::pre_exit_check`); the handler counts that exit, so divert before the store
                    self.push(Op::And(sr, Src::Imm(exit::INEFF_EXITS as i32 - 1)));
                    self.br(
                        Cc::Eq,
                        sr,
                        Src::Imm(exit::INEFF_EXITS as i32 - 1),
                        generic_l,
                    );
                    self.ld(sr, W::B4, rm(7, 0));
                }
                self.push(Op::Add(sr, Src::Imm(1)));
                self.st(W::B4, rm(7, 0), Src::V(sr));
            } else {
                if pre_site {
                    return incons("no scratch register for the exit counter of a pre-roll site");
                }
                self.push(Op::Mov(7, addr));
                if !mutants::active("exit_mem_counter_no_divert") {
                    self.push(Op::BrM {
                        cc: Cc::Eq,
                        w: W::B4,
                        m: rm(7, 0),
                        imm: hot1,
                        to: generic_l,
                    });
                }
                if chain_site && !mutants::active("chain_stub_no_test") {
                    self.push(Op::BrM {
                        cc: Cc::Eq,
                        w: W::B4,
                        m: rm(7, 0),
                        imm: exit::CHAIN_EXITS as i32 - 1,
                        to: generic_l,
                    });
                }
                if !mutants::active("exit_mem_counter_no_count") {
                    self.push(Op::AddM {
                        w: W::B4,
                        m: rm(7, 0),
                        imm: 1,
                    });
                }
            }
        }
        if let (true, Some(c)) = (frame_cell, cell) {
            // S1: an exit with virtual-able frames continues through its cell only once a side trace is attached (the cell
            // starts at 0): until then the generic twin restores the frames. The test comes before any store: the twin
            // does the whole restore (and the budget) itself.
            if !mutants::active("frame_cell_no_test") {
                self.push(Op::Mov(7, c));
                self.ld(7, W::B8, rm(7, 0));
                self.br(Cc::Eq, 7, Src::Imm(0), generic_l);
            }
        }
        for op in head {
            self.push(*op);
        }
        if entry_check {
            // an entry check has an empty map: every register is free here
            self.entry_threshold_ops();
        }
        if !es.sunk.is_empty() {
            // J1-2: the budget is stored and the registers the sunk tables read are in the exit area before the helper call (it
            // clobbers every register); the jump through the cell comes after it (a policy flush in the helper may have changed the cell)
            let jump = if cell.is_some() { 3 } else { 2 };
            let (pre, fin) = tail.split_at(tail.len() - jump);
            for op in pre {
                self.push(*op);
            }
            for op in exit::sunk_save_ops(es) {
                self.push(op);
            }
            let err = self.label();
            self.push(Op::CallH {
                f: self.env.mat_fn,
                a: snap,
                b: self.env.trace_id,
            });
            if !mutants::active("sinkstub_err_ignored") {
                self.push(Op::Br {
                    cc: Cc::Ne,
                    a: 0,
                    b: Src::Imm(0),
                    to: err,
                });
            }
            for op in fin {
                self.push(*op);
            }
            // a host allocation failed: the helper left the frame at the exit's pc and recorded the memory error
            self.bind(err);
            self.push(Op::Ret(rn_core::jit_hooks::JitExit::Error as u32));
            return Ok(());
        }
        if self.env.lazy && cell.is_some() && !mutants::active("lazy_stub_no_leave") {
            // BUNDLE-K L1: a side trace attached to the exit continues in the region (lazy frames stay lazy: it is lazy-aware); the
            // tier-1 landing is entered after the chain is completed. V6 is dead after the slot and budget stores.
            let (last, rest) = tail.split_last().expect("stub tail");
            for op in rest {
                self.push(*op);
            }
            let leave = self.lz_leave_label(false);
            self.push(Op::Mov(6, es.land_addr));
            self.br(Cc::Eq, 7, Src::V(6), leave);
            self.push(*last);
            return Ok(());
        }
        for op in &tail {
            self.push(*op);
        }
        Ok(())
    }
}

// ---- driver support (test-only install path; PI replaces it with a per-proto pc map in `TrProto`) ---------------------

/// Support for tests and for the pipeline driver until `cache` keeps a per-prototype pc -> address map.
#[doc(hidden)]
pub mod support {
    use crate::lower::{lower_ex, LowerCfg};
    use crate::tr::cache::{emit_dispatch, Dispatch, DispatchCfg, TrMode};
    use crate::{rt, x86};
    use rn_core::func::Proto;
    use rn_core::jit_hooks;
    use rn_core::state::State;

    /// The absolute tier-1 address of the label of every pc (`None` for a dead pc) of the compiled prototype whose
    /// `Proto` is at `proto_key` (a `ProtoKey`). The code is re-derived exactly as `rt::compile_inner` produced it (the
    /// same lowering over the same trace table) and **compared byte for byte with the placed block**; `Err` names the
    /// difference. Needs `trace` mode on in the state that compiled the prototype.
    ///
    /// # Safety
    /// `proto_key` must be the address of a live `Proto` of `st`'s heap that has been compiled by `st`'s compiler.
    pub unsafe fn tier1_pc_addrs(
        st: &mut State,
        proto_key: usize,
    ) -> Result<Vec<Option<u64>>, String> {
        // SAFETY: contract.
        let p: &Proto = unsafe { &*(proto_key as *const Proto) };
        let jp = p.jit.get() as usize;
        if jp <= rt::NOJIT {
            return Err("prototype is not compiled".into());
        }
        // SAFETY: a compiled prototype's `jit` is its live descriptor (contract).
        let jpr = unsafe { &*(jp as *const rt::JitProto) };
        let base = jpr.hdr.base_m1 + 1;
        let dp = rt::data_ptr(st);
        // SAFETY: the per-state data is live; no other reference is held.
        let d = unsafe { &mut *dp };
        let env = rt::call_env(st, d);
        let cfg = LowerCfg {
            code_addr: p.code_ptr() as usize,
            interrupt_addr: jit_hooks::interrupt_flag_addr(),
            calls: Some(env),
            consts: p.k(),
            t15_warmup: None,
        };
        // SAFETY: as above.
        let d = unsafe { &*dp };
        let mut dis = (d.tr.cfg.mode != TrMode::Off).then(|| {
            Dispatch::new(
                DispatchCfg {
                    hot: d.tr.cfg.hot_loop,
                    always: d.tr.cfg.mode == TrMode::Always,
                    tracemove: d.tr.cfg.opts.tracemove,
                    seed: (p.linedefined as u32) ^ ((p.code().len() as u32) << 16),
                    line: p.linedefined,
                    fn_root: (d.tr.cfg.limits.fnroot != 0 || d.tr.cfg.limits.recnat != 0)
                        && !p.is_vararg(),
                    recnat: d.tr.cfg.limits.recnat != 0,
                    lazy: d.tr.cfg.limits.lazyci != 0,
                    fn_loops: d.tr.cfg.limits.recany != 0,
                    rec_any: d.tr.cfg.limits.recany != 0,
                    c_entered: d.tr.centry.contains(&(p.code_ptr() as usize)),
                },
                jpr.tr.as_deref(),
            )
        });
        let low = match dis.as_mut() {
            Some(dis) => {
                let mut hook = |cx: &mut crate::lower::Cx<'_>,
                                env: &crate::lower::RegionEnv<'_>| {
                    emit_dispatch(cx, env, dis)
                };
                lower_ex(p.code(), cfg, Some(&mut hook), false)
            }
            None => lower_ex(p.code(), cfg, None, false),
        }
        .ok_or("the prototype does not lower")?;
        let em = x86::emit(&low.prog).map_err(|e| format!("emit: {e:?}"))?;
        if em.code.len() > jpr.block.size() {
            return Err("the re-derived tier-1 code is larger than the placed block".into());
        }
        // SAFETY: the block is the committed, readable code of the live descriptor and `em.code` fits in it.
        let actual = unsafe { std::slice::from_raw_parts(base as *const u8, em.code.len()) };
        if actual != em.code.as_slice() {
            return Err("the re-derived tier-1 code differs from the placed block".into());
        }
        Ok(low
            .land_labels
            .iter()
            .map(|l| {
                l.and_then(|l| em.label_off.get(l as usize).copied().flatten())
                    .map(|off| (base + off) as u64)
            })
            .collect())
    }
}

/// B4-4: the multiplier and shift of the magic-number division of a 63-bit non-negative dividend by `c` (`3 <= c <= 2^62`, not a power
/// of two): `m / c == ((m as u128 * mg as u128) >> 64) as u64 >> sh` for every `0 <= m < 2^63` (Granlund-Montgomery with N = 63:
/// `l = ceil(log2 c)`, `mg = ceil(2^(63 + l) / c)`, `sh = l - 1`; `mg < 2^64` is checked). `None` for every other divisor.
pub fn magic_u63(c: i64) -> Option<(u64, u8)> {
    if c < 3 || c > (1i64 << 62) || (c as u64).is_power_of_two() {
        return None;
    }
    let l = 64 - (c as u64).leading_zeros(); // c is not a power of two: ceil(log2 c) = bit length
    let mg = ((1u128 << (63 + l)) + c as u128 - 1) / c as u128;
    if mg > u128::from(u64::MAX) {
        return None;
    }
    Some((mg as u64, (l - 1) as u8))
}

#[cfg(test)]
mod b4_tests {
    use super::magic_u63;

    /// The arithmetic of `Cx::div_magic` on plain integers.
    fn model(a: i64, c: i64) -> (i64, i64) {
        let (mg, sh) = magic_u63(c).expect("magic");
        let s = a >> 63;
        let m = (a ^ s) as u64;
        let q = ((((m as u128) * (mg as u128)) >> 64) as u64 >> sh) as i64 ^ s;
        (q, a.wrapping_sub(q.wrapping_mul(c)))
    }

    fn floor_ref(a: i64, c: i64) -> (i64, i64) {
        let q = a.wrapping_div(c);
        let r = a.wrapping_rem(c);
        if r != 0 && (r ^ c) < 0 {
            (q - 1, r + c)
        } else {
            (q, r)
        }
    }

    #[test]
    fn magic_matches_floor_division() {
        let mut x = 0x9E37_79B9_7F4A_7C15u64;
        let mut next = || {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            x
        };
        let mut cs: Vec<i64> = (3..400).collect();
        for k in 2..63 {
            for d in [-2i64, -1, 1, 2, 3] {
                cs.push((1i64 << k) + d);
            }
        }
        cs.extend([
            i64::MAX,
            1 << 62,
            (1 << 62) - 1,
            1_000_003,
            255,
            1000,
            86400,
            0x7FFF_FFFF,
            0x8000_0001,
            0xFFFF_FFFF,
            0x1_0000_0001,
        ]);
        for _ in 0..300 {
            let sh = 1 + next() % 62;
            cs.push((next() >> sh) as i64);
        }
        let mut n_ok = 0;
        for &c in &cs {
            if magic_u63(c).is_none() {
                continue;
            }
            n_ok += 1;
            let mut ns: Vec<i64> = vec![
                0,
                1,
                -1,
                2,
                -2,
                c - 1,
                c,
                c + 1,
                -c + 1,
                -c,
                -c - 1,
                i64::MIN,
                i64::MIN + 1,
                i64::MAX,
                i64::MAX - 1,
                1 << 62,
                -(1 << 62),
            ];
            for k in 1..4 {
                for j in -2..=2i64 {
                    ns.push(c.wrapping_mul(k).wrapping_add(j));
                    ns.push(c.wrapping_mul(-k).wrapping_add(j));
                }
            }
            // multiples of c near the ends of the range
            let top = i64::MAX / c * c;
            for j in -2..=2i64 {
                ns.push(top.wrapping_add(j));
                ns.push((-top).wrapping_add(j));
            }
            for _ in 0..400 {
                let v = next();
                let (s1, s2) = (next() % 64, next() % 64);
                ns.push((v >> s1) as i64);
                ns.push(-((v >> s2) as i64));
            }
            for &a in &ns {
                assert_eq!(model(a, c), floor_ref(a, c), "a = {a}, c = {c}");
            }
        }
        assert!(n_ok > 600);
    }

    #[test]
    fn magic_rejects_the_rest() {
        for c in [i64::MIN, -7, -1, 0, 1, 2, 4, 8, (1 << 62) + 1, i64::MAX] {
            let want = c >= 3 && c <= 1 << 62 && !(c as u64).is_power_of_two();
            assert_eq!(magic_u63(c).is_some(), want, "c = {c}");
        }
    }
}
