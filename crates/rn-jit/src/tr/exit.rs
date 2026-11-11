//! J5 P4: exits of a trace (docs/JIT_TRACE.md 5.4-5.6, 7.6; decisions in docs/JIT_TRACE_REF.md "P4 decisions").
//!
//! Three parts, from pure to unsafe:
//!
//! 1. **Resolved exit descriptors** ([`ExitSnap`], [`ExitMeta`]) and the pure builder [`build_exits`] that derives them
//!    from a [`TraceIr`] and P5a's [`Allocation`] (`snap_locs`): per snapshot the slots to restore (slot, tag, where the
//!    value is), the inlined frames (0..N), the pc, the refund of the budget (TR5.4), the landing address and the site
//!    class of the exit. The inline stub (frameless snapshots, 7.6), the generic stub and the Rust restore are all
//!    generated from this one descriptor (5.5: no second source of truth).
//! 2. **Machine-code contracts** (micro-ops, safe code): [`append_shared_exit`] (the per-arena shared exit routine,
//!    5.6), [`generic_stub_ops`] / [`GENERIC_STUB_STORES`] (what a generic stub stores before it jumps to the shared
//!    routine) and [`inline_stub_ops`] (the frameless stub).
//! 3. **The Rust exit handler**: [`restore`] (slots, then `CallInfo`s of inlined frames, `thread.ci`, `thread.top`,
//!    budget word: 5.5) over raw pointers, [`exit_entry`] / [`jit_tr_exit`] (the `CallH` target: finds the trace
//!    metadata, restores, counts the exit, reports hot exits through `cache::exit_hot`, writes the landing address).
//!
//! # The shared exit routine (5.6)
//!
//! A generic stub stores `trace_id` and `exitno` into the [`ExitArea`] and jumps to the routine with `Mov V7, addr;
//! JmpR V7`. The routine stores every allocatable register (`V0..V6, V8`, `X0..X15`; `V7` holds no value, 7.1) into the
//! area, calls `jit_tr_exit(thread, ci, 0, 0)` (`r12`, `r13` = the root frame), and then re-establishes the pinned
//! registers for the frame the handler left in `thread.ci` (`SetCi`, `ReloadFrame`, `SetK` from `ci.kcache`) and
//! `JmpR`s to the landing address, which the handler left in `ExitArea.args[0]` (no extra continuation cell: at an exit
//! no bracketed helper call is active and TR2 gives one activation per state). The budget needs no stub code: the
//! handler computes `vb + refund` from the saved register file ([`ExitMeta::vb`]) and stores it into the budget word.
//!
//! # Failure policy (fail-stop)
//!
//! [`jit_tr_exit`] is an `extern "C"` function called from generated code. A panic that reaches its boundary does not
//! unwind into the trace: an unwind out of an `extern "C"` function aborts the process (Rust 1.81+; the workspace
//! profile says `panic = "unwind"`, but generated frames have no unwind info, so unwinding must never enter them). Every `assert!` / `expect` in
//! [`restore`], [`exit_core`] and [`exit_entry`] (a bad slot, a missing spare `CallInfo`, an unusable snapshot, a
//! budget location that is not G/Spill/None, a trace without metadata) therefore means **fail-stop**: the process
//! aborts with the message. That is deliberate (a half-restored frame chain cannot be resumed or reported as a Lua
//! error), and the conditions are unreachable by construction (entry checks TR6b, `build_exits` validation).
//!
//! # Interpreter fields the restore does not set
//!
//! `pre_call_lua_fast` also sets `ci.hot` (feature `jit_counter_c` of rn-core) and `CIST_JIT` / calls
//! `jit_try_enter` (feature `jit_check_d`). The J5 trace compiler is incompatible with both (it is built on counter B
//! and check E, which the `jit` feature selects): `jit_counter_c` changes the layout of `CallInfo` and is rejected at
//! compile time below; `jit_check_d` is rejected by the `compile_error!` on the rn-jit feature of the same name (a
//! build that enables the rn-core feature behind rn-jit's back is not detectable from here: P4c decisions).
//!
//! # Unsafe
//!
//! Confined to [`restore`] (raw stores into the Lua stack and `CallInfo` nodes of the running thread), [`exit_entry`] /
//! [`jit_tr_exit`] (the C entry points) and two `Send`/`Sync` impls for the metadata that holds raw pointers to
//! prototype data. See the contracts on each item.

use super::cache::{self, ExitSite, TrId};
use super::ir::*;
use super::mutants;
use super::ra::{Allocation, Loc, NGPR, NXMM};
use super::{exit_area, ExitArea};
use crate::ir::{Base, Cc, Op, Program, Src, L, M, W};
use rn_core::jit_hooks::{self, thread as thr, JitExit};
use rn_core::jit_layout::callinfo as ci;
use rn_core::state::{CallInfo, Ci, State, StkId, Thread, CIST_MMRET, CIST_TAIL};
use rn_core::value::*;
use std::cell::{Cell, RefCell};

// `CallInfo.hot` (rn-core feature `jit_counter_c`) is the only field after `callstatus`: `restore` does not set it,
// so a build that has it is rejected (the layout is the only thing rn-jit can observe of a rn-core feature).
const _: () = assert!(
    ci::CALLSTATUS + std::mem::size_of::<u32>() == ci::SIZE,
    "CallInfo has a field after callstatus (feature jit_counter_c?): the J5 exit restore does not set it (P4c)"
);
#[cfg(feature = "jit_check_d")]
compile_error!("the J5 trace compiler (`trace`) does not support the interpreter hook `jit_check_d` (CIST_JIT): P4c");

/// Most inlined frames an exit can rebuild (`maxdepth` 4 in the prototype, 8 in the full build).
pub const MAX_FRAMES: usize = 8;

// ---- resolved descriptors ---------------------------------------------------------------------------------------

/// Where the value of a snapshot slot is at the exit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ValSrc {
    /// General register `V<n>` (`ExitArea.gpr[n]`; never 7).
    Reg(u8),
    /// Double register `X<n>` (`ExitArea.xmm[n]`, bit pattern).
    Xmm(u8),
    /// Spill slot (`ExitArea.spill[n]`).
    Spill(u16),
    /// A constant payload (the tag is in [`SlotRestore::tag`]).
    Imm(u64),
}

/// N5: [`SlotRestore::tag`] of a string whose short / long variant is not static (`ir::F_ANYSTR`): the restore reads the tag
/// from the object header (`tt | collectable bit`). Not a tag any value has (the variant 15 of type 15).
pub const TAG_DYN_STR: u8 = 0xFF;

/// One slot to write at an exit: root-relative slot, the tag byte of the value and where the payload is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SlotRestore {
    /// Root-relative slot (`rbx[slot]`).
    pub slot: u16,
    /// Tag byte of the Lua value (collectable bit included).
    pub tag: u8,
    /// Payload location.
    pub src: ValSrc,
    /// The slot is a function slot of an inlined frame (informational: it is restored like any other).
    pub func: bool,
}

/// What an inlined frame needs to be rebuilt as `pre_call_lua_fast` / the tier-1 flat call would have built it (5.5).
#[derive(Clone, Copy, Debug)]
pub struct FrameRestore {
    /// Root-relative slot of the callee's function slot.
    pub func_slot: u16,
    /// Results wanted by the call (`CALL`'s `C - 1`); `callstatus = nresults + 1`.
    pub nresults: i16,
    /// The caller's `CALL` instruction (the caller's `savedpc` becomes `call_pc + 1`).
    pub call_pc: u32,
    /// The callee's `Proto.code`.
    pub code: *const u32,
    /// The callee's constants (`Proto.k`), stored in `ci.kcache`.
    pub k: *const u8,
    /// The callee's `maxstacksize` (`ci.top = func + 1 + maxstack`).
    pub maxstack: u16,
    /// F4: entered by a `TAILCALL`: `callstatus` carries `CIST_TAIL`.
    pub tail: bool,
    /// P-E: a vararg callee: `ci.nextraargs` (`-1`: not a vararg frame, the field is left alone).
    pub nextra: i32,
    /// N3: a metamethod frame run in place: `callstatus` carries `CIST_MMRET`, `ctx` the `nCcalls` before its call, and the
    /// thread's `nCcalls` counts it (as `mm_frame` does).
    pub mm: bool,
}

/// One field of a sunk table: the interned short-string key and the value (tag + payload source).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SunkField {
    /// Address of the key (an interned short string, kept alive by the trace's constants).
    pub key: u64,
    /// Tag byte of the value.
    pub tag: u8,
    /// Payload location of the value.
    pub src: ValSrc,
}

/// A table the trace never allocated (S0, TR26c): created at the exit with the `NEWTABLE`'s sizes and filled in insertion order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SunkTable {
    /// The slot that holds it.
    pub slot: u16,
    /// `NEWTABLE` operand `B` (hash size `1 << (B - 1)`, 0: none).
    pub b: u32,
    /// `NEWTABLE` array size (always 0 in S0).
    pub c: u32,
    /// The fields, in insertion order.
    pub fields: Vec<SunkField>,
    /// Further slots that hold the same table.
    pub aliases: Vec<u16>,
    /// BUNDLE-2 (`sinkmt`): the metatable the table gets after its fields (tag and location of the metatable value).
    pub mt: Option<(u8, ValSrc)>,
}

/// A fully resolved snapshot (one exit).
#[derive(Clone, Debug)]
pub struct ExitSnap {
    /// The instruction of the innermost frame that tier 1 re-executes.
    pub pc: u32,
    /// Slots to write (all frames, root-relative).
    pub slots: Vec<SlotRestore>,
    /// The inlined frames, outermost first; empty for a frameless snapshot.
    pub frames: Vec<FrameRestore>,
    /// TR5.4: the budget word becomes `vb + refund`.
    pub refund: i32,
    /// Landing label kind (informational; the refund already includes `land`).
    pub land: Land,
    /// Absolute address of the tier-1 label of `pc` in the innermost prototype.
    pub land_addr: u64,
    /// `false` for a snapshot no guard uses (the allocator gave its refs no location): it must never be restored.
    pub usable: bool,
    /// S0: the sunk tables to create at this exit (empty for nearly every snapshot; such a snapshot always uses the
    /// generic stub).
    pub sunk: Vec<SunkTable>,
    /// F2b: address of `cell[H]` (a `Cell<usize>` in the callee prototype's `TrProto`, address-stable) of the loop head
    /// this snapshot's exit links to: after the frames are materialised the exit continues at that cell's value (the
    /// head's trace entry, or whatever tier 1 would reach), not at `land_addr`. 0: an ordinary exit.
    pub link_cell: u64,
    /// F4 (TR22b relaxed): `Some(slot)`: `thread.top` becomes `base + slot` (root-relative) instead of the innermost
    /// frame's `ci.top`; the exit is restored by the Rust handler (no inline stub).
    pub top: Option<u16>,
    /// N2a (RETF): `Proto.code` of the root frame of this snapshot's segment; null for segment 0 (then `ExitMeta::root_code`).
    pub root_code: *const u32,
    /// N2a: slots between this snapshot's root frame and the entry root, and frames popped since the entry (0 for segment 0).
    pub seg_shift: u16,
    /// See `seg_shift`.
    pub seg_pops: u8,
}

/// One exit of a trace: the snapshot it restores and the class of the site (for the hot-exit policy, 3.3 / 7.9).
#[derive(Clone, Copy, Debug)]
pub struct ExitDesc {
    /// Index into [`ExitMeta::snaps`].
    pub snap: u32,
    /// `TypeGuard` exits that get hot flush the trace and re-record; `Other` ones are only counted.
    pub site: ExitSite,
    /// Every guard that uses the snapshot lies in the pre-roll (the entry segment, before `LOOP`) of a peeled trace:
    /// taking this exit means the trace left before completing one loop iteration (Q2 ineffective-trace rule,
    /// [`INEFF_EXITS`]). Entry-check exits are never `pre`.
    pub pre: bool,
}

/// Everything the exit handler needs about one installed trace.
pub struct ExitMeta {
    /// `Proto.code` of the root prototype (the root frame's code, for `savedpc`).
    pub root_code: *const u32,
    /// Resolved snapshots.
    pub snaps: Vec<ExitSnap>,
    /// Exit numbers (`ExitArea.exitno`) -> snapshot and site class. [`build_exits`] numbers exits like snapshots.
    pub exits: Vec<ExitDesc>,
    /// Where the budget register `vb` is at an exit (`Loc::None`: the budget word itself holds `vb`).
    pub vb: Loc,
    /// The exit counters of the trace (`cache::alloc_exit_counters`, `exits.len()` words), or null for no counting.
    pub counters: *const Cell<u32>,
    /// F2a (TR8): the exit cells (`cache::alloc_exit_cells`, `snaps.len()` words), or null for none. `cells[k]` is where
    /// an exit through snapshot `k` continues once the state is restored: initially `snaps[k].land_addr` (tier 1), the
    /// entry of a side trace after one is attached. Only frameless snapshots use their cell.
    pub cells: *const Cell<usize>,
    /// F2a: the trace has a pre-roll (a peeled root): a persistently failing type guard is caught by the ineffective-trace
    /// rule (Q2), so a side trace may be recorded for it. Rolled roots and side traces keep the prototype's re-record rule.
    pub peeled: bool,
    /// S1: the trace's stack extent (`TraceIr::extent`) and spare-`CallInfo` count (`TraceIr::depth`): what its entry checks
    /// guarantee. A side trace entered with virtual frames (see [`ExitSnap::frame_side_ok`]) runs without entry checks and
    /// may not need more than its parent's.
    pub extent: u16,
    /// See `extent`: the spare-`CallInfo` count.
    pub depth: u8,
    /// S1: the frame-cell exits of this trace start at 0 (see `AsmEnv::frame_cells`); only then does a cell of a frameful exit
    /// hold the entry of a side trace with virtual frames.
    pub frame_cells: bool,
    /// BUNDLE-K L1: the trace is lazy-aware (`AsmEnv::lazy`): its exits complete a lazy chain first (`lazy_leave`).
    pub lazy: bool,
    /// N2c (`+verify`): extension slot + 1 of the exit area that holds the trace's entry `CallInfo` (0: no segment oracle). The handler checks
    /// that an exit after `k` `RETF`s runs in the entry frame popped `k` times, `seg_shift` slots lower.
    pub seg_oracle: u16,
}

impl ExitSnap {
    /// N2a: the code pointer the root frame of this snapshot's segment runs (`meta_root` for segment 0).
    #[inline]
    pub fn code_or(&self, meta_root: *const u32) -> *const u32 {
        // (one test of `seg_pops`, which the exit handler needs anyway: segment 0 is the hot case)
        if self.seg_pops == 0 || mutants::active("seg_root_code_stale") {
            meta_root
        } else {
            self.root_code
        }
    }

    /// S1: may a side trace hang off an exit through this snapshot although it has inlined frames? The frames stay virtual
    /// in the side trace (its own snapshots carry the whole chain); only non-vararg Lua frames (plain, tail or metamethod frames),
    /// no multiple-result `top` and no linking exit. Sunk tables are allowed (SINK-F): the exit's restore materialises them into
    /// their slots before it continues in the side trace (the handler does, and the sunk frame stub calls `jit_tr_mat` first), so the
    /// side trace sees ordinary tables in its slots; only the allocation-free `fmat` stub (`frame_mat_ok`) stays out.
    pub fn frame_side_ok(&self) -> bool {
        self.frames_plain() && self.link_cell == 0
    }

    /// [`ExitSnap::frame_side_ok`] without the "no link" condition: plain or tail Lua frames, no vararg frame, no `top`.
    pub fn frames_plain(&self) -> bool {
        self.usable
            && !self.frames.is_empty()
            && self.frames.iter().all(|f| f.nextra < 0)
            && self.top.is_none()
    }
}

// SAFETY: the metadata is immutable after construction. The raw pointers refer to prototype data (code, constants) that
// outlives the trace (TR32 dependency edges: the trace is flushed before a prototype is freed) and to the trace's exit
// counters (owned by the trace); they are dereferenced only by the thread that runs the state (the handler runs on the
// trace's activation, TR2). The `Arc` is handed out only to that thread.
unsafe impl Send for ExitMeta {}
// SAFETY: see above; no `&self` method mutates anything but the `Cell` counters, and those are only touched by the
// state's own thread.
unsafe impl Sync for ExitMeta {}

/// The Lua tag byte of a value of type `t`, if `t` is a Lua type (not a raw machine type).
pub fn tag_of(t: Ty) -> Option<u8> {
    Some(match t {
        Ty::Nil => LUA_VNIL,
        Ty::False => LUA_VFALSE,
        Ty::True => LUA_VTRUE,
        Ty::Int => LUA_VNUMINT,
        Ty::Num => LUA_VNUMFLT,
        Ty::ShrStr => ctb(LUA_VSHRSTR),
        Ty::LngStr => ctb(LUA_VLNGSTR),
        Ty::Tab => ctb(LUA_VTABLE),
        Ty::Lfn => LUA_VLCF,
        Ty::Lcf => ctb(LUA_VLCL),
        Ty::Ccl => ctb(LUA_VCCL),
        Ty::Udata => ctb(LUA_VUSERDATA),
        Ty::LUdata => LUA_VLIGHTUSERDATA,
        Ty::Thread => ctb(LUA_VTHREAD),
        Ty::I64 | Ty::P64 | Ty::U32 => return None,
    })
}

/// The IR type of a Lua tag byte as [`tag_of`] produced it (`None`: not a tag of a type the traces model).
pub fn ty_of_tag(tag: u8) -> Option<Ty> {
    Some(match tag {
        LUA_VNIL => Ty::Nil,
        LUA_VFALSE => Ty::False,
        LUA_VTRUE => Ty::True,
        LUA_VNUMINT => Ty::Int,
        LUA_VNUMFLT => Ty::Num,
        LUA_VLCF => Ty::Lfn,
        LUA_VLIGHTUSERDATA => Ty::LUdata,
        t if t == ctb(LUA_VSHRSTR) => Ty::ShrStr,
        t if t == ctb(LUA_VLNGSTR) => Ty::LngStr,
        t if t == ctb(LUA_VTABLE) => Ty::Tab,
        t if t == ctb(LUA_VLCL) => Ty::Lcf,
        t if t == ctb(LUA_VCCL) => Ty::Ccl,
        t if t == ctb(LUA_VUSERDATA) => Ty::Udata,
        t if t == ctb(LUA_VTHREAD) => Ty::Thread,
        _ => return None,
    })
}

/// The window of `ir.frames` that snapshot `snap` owns. **Convention (P4, for P3b):** every snapshot with
/// `nframes > 0` owns its own consecutive run of `FrameLink`s, outermost first, in snapshot order; a snapshot's run
/// starts after the runs of all earlier snapshots. (`Snap` has no start index and is frozen.)
pub fn frame_window(ir: &TraceIr, snap: usize) -> std::ops::Range<usize> {
    let start: usize = ir.snaps[..snap]
        .iter()
        .map(|s| usize::from(s.nframes))
        .sum();
    start..start + usize::from(ir.snaps[snap].nframes)
}

/// The site class of every snapshot: `TypeGuard` when every guard that uses the snapshot is a typecheck load
/// (`Sload` / `Aload` / `Uload` with bit 0 of the op bits), `Other` otherwise (a snapshot shared with a bounds or
/// branch guard is not flushed on its own count). A snapshot no guard uses is `Other`.
pub fn site_classes(ir: &TraceIr) -> Vec<ExitSite> {
    let mut any = vec![false; ir.snaps.len()];
    let mut all_type = vec![true; ir.snaps.len()];
    let mut all_alloc = vec![true; ir.snaps.len()];
    for i in &ir.ins {
        if i.flags & F_GUARD == 0 {
            continue;
        }
        let s = aux_snap(i.aux) as usize;
        if s >= any.len() {
            continue;
        }
        any[s] = true;
        // N7: the key guard of a `TNEXT` (`HLOAD` bit 2) fails at the end of every traversal: not a type instability (a re-record would
        // build the same trace), the exit gets a side trace that continues behind the loop
        let ty_check = matches!(i.op, TOp::Sload | TOp::Aload | TOp::Uload | TOp::Hload)
            && aux_bits(i.aux) & 1 != 0
            && !(i.op == TOp::Hload
                && aux_bits(i.aux) & 2 != 0
                && !mutants::active("tnext_key_site_type"));
        if !ty_check {
            all_type[s] = false;
        }
        // P-B: the guard `NE %alloc, 0` behind a class-A allocation
        let alloc_guard = i.op == TOp::Ne
            && TraceIr::is_ins(i.a)
            && matches!(
                ir.at(i.a).op,
                TOp::Tnew | TOp::Snew | TOp::Concat | TOp::Lib
            );
        if !alloc_guard {
            all_alloc[s] = false;
        }
    }
    (0..any.len())
        .map(|s| {
            if any[s] && all_type[s] {
                ExitSite::TypeGuard
            } else if any[s] && all_alloc[s] && !mutants::active("storm_site_skip") {
                ExitSite::Alloc
            } else {
                ExitSite::Other
            }
        })
        .collect()
}

/// BUNDLE-O O0 (diagnostic): per snapshot, the bit set of guard classes that use it. Bits: 1 callee / identity (`EQREF`), 2 hash node
/// (`HREFK` / `HREFV` / `HMISS`), 4 hash load typecheck, 8 metatable / flags / node-size field compare, 16 slot / array / upvalue
/// typecheck, 32 anything else (branches, bounds, loop exit, budget, poll).
pub fn guard_kinds(ir: &TraceIr) -> Vec<u16> {
    let mut out = vec![0u16; ir.snaps.len()];
    for i in &ir.ins {
        if i.flags & F_GUARD == 0 {
            continue;
        }
        let s = aux_snap(i.aux) as usize;
        if s >= out.len() {
            continue;
        }
        let fld = |r: Ref| TraceIr::is_ins(r) && matches!(ir.at(r).op, TOp::Fload | TOp::Band);
        out[s] |= match i.op {
            TOp::Eqref => 1,
            TOp::Hrefk | TOp::Hrefv | TOp::Hmiss => 2,
            TOp::Hload if aux_bits(i.aux) & 1 != 0 => 4,
            TOp::Eq | TOp::Ne if fld(i.a) => 8,
            TOp::Sload | TOp::Aload | TOp::Uload if aux_bits(i.aux) & 1 != 0 => 16,
            _ => 32,
        };
    }
    out
}

/// Exits of one pre-roll site after which the ineffective-trace rule is evaluated (every multiple of this count, Q2).
pub const INEFF_EXITS: u32 = 1024;

/// P3 (`chainre`): exits of one callee-guard site (with a side trace attached) after which the chain re-order rule is evaluated (every multiple of this count; a power of two).
pub const CHAIN_EXITS: u32 = 4096;

/// Which snapshots are used only by guards of the pre-roll of a peeled trace (index below `LOOP`; all `false` for the
/// rolled layout). A snapshot no guard uses is `false`.
pub fn pre_roll_snaps(ir: &TraceIr) -> Vec<bool> {
    let mut pre = vec![false; ir.snaps.len()];
    if ir.fn_root && ir.loop_at.is_none() && !mutants::active("fn_exits_not_pre") {
        // P-E2: a function-entry root: every guard but the last (the always-taken final exit) is an early exit
        let last = ir.ins.iter().rposition(|i| i.flags & F_GUARD != 0);
        for (k, i) in ir.ins.iter().enumerate() {
            if i.flags & F_GUARD != 0 && Some(k) != last {
                let s = aux_snap(i.aux) as usize;
                if s < pre.len() {
                    pre[s] = true;
                }
            }
        }
        // (the final exit's snapshot is not shared with an earlier guard: it is the snapshot before the `RETURN`)
        if let Some(k) = last {
            let s = aux_snap(ir.ins[k].aux) as usize;
            if s < pre.len() {
                pre[s] = false;
            }
        }
        return pre;
    }
    let Some(l) = ir.loop_at else {
        return pre;
    };
    let lp = (l - REF_BIAS) as usize;
    let mut post = vec![false; ir.snaps.len()];
    for (k, i) in ir.ins.iter().enumerate() {
        if i.flags & F_GUARD == 0 {
            continue;
        }
        let s = aux_snap(i.aux) as usize;
        if s >= pre.len() {
            continue;
        }
        if k < lp {
            pre[s] = true;
        } else {
            post[s] = true;
        }
    }
    for (p, q) in pre.iter_mut().zip(&post) {
        *p = *p && !*q;
    }
    pre
}

/// What the builder needs from the runtime: prototype data and tier-1 landing addresses.
pub trait Resolver {
    /// Code, constants and frame size of the prototype `key` (a member of the trace's `deps` or its root).
    fn proto(&self, key: ProtoKey) -> Option<ProtoInfoR>;
    /// Absolute address of the tier-1 label of `pc` in prototype `key` (`Pre`: the pre-count label of a head,
    /// `Mid`: the mid-block label), in the code version recorded in the trace (TR4).
    fn land_addr(&self, key: ProtoKey, pc: u32, land: Land) -> Option<u64>;
}

/// Prototype data for [`Resolver`].
#[derive(Clone, Copy, Debug)]
pub struct ProtoInfoR {
    /// `Proto.code`.
    pub code: *const u32,
    /// `Proto.k`.
    pub k: *const u8,
    /// `maxstacksize`.
    pub maxstack: u16,
}

fn src_of(loc: Loc) -> Option<ValSrc> {
    Some(match loc {
        Loc::G(n) if n != 7 && usize::from(n) < NGPR => ValSrc::Reg(n),
        Loc::X(n) if usize::from(n) < NXMM => ValSrc::Xmm(n),
        Loc::Spill(s) if usize::from(s) < super::NSPILL => ValSrc::Spill(s),
        _ => return None,
    })
}

/// Tag byte and payload location of the value `r` of a snapshot entry placed at `loc` (shared by stack slots and sunk
/// fields). `usable` becomes false for a value no guard of the snapshot uses (the allocator left it unplaced).
fn slot_value(
    ir: &TraceIr,
    si: usize,
    r: Ref,
    loc: Loc,
    usable: &mut bool,
) -> Result<(u8, ValSrc), String> {
    if TraceIr::is_const(r) {
        return match ir.kval(r) {
            KVal::Int(v) => Ok((LUA_VNUMINT, ValSrc::Imm(v as u64))),
            KVal::Num(b) => Ok((LUA_VNUMFLT, ValSrc::Imm(b))),
            // payload 0 is only a valid value for nil / false / true; a collectable type here would be a
            // null GC pointer (P4c: the recorder must not emit it)
            KVal::Pri(t @ (Ty::Nil | Ty::False | Ty::True)) => {
                Ok((tag_of(t).expect("a Lua type"), ValSrc::Imm(0)))
            }
            KVal::Pri(t) => Err(format!("snapshot {si}: primitive constant of type {t:?}")),
            KVal::Ptr(a, PtrKind::LClosure) => Ok((ctb(LUA_VLCL), ValSrc::Imm(a))),
            KVal::Ptr(a, PtrKind::Builtin) => Ok((LUA_VLCF, ValSrc::Imm(a))),
            KVal::Ptr(..) => Err(format!("snapshot {si}: pointer constant in a slot")),
        };
    }
    let t = ir.ty_of(r);
    let mut tag = tag_of(t).ok_or_else(|| format!("snapshot {si}: raw type {t:?} in a slot"))?;
    if ir.at(r).flags & super::ir::F_ANYSTR != 0 {
        if !matches!(t, Ty::ShrStr | Ty::LngStr)
            || src_of(loc).is_none_or(|s| !matches!(s, ValSrc::Reg(_) | ValSrc::Spill(_)))
        {
            // (an unplaced value is "never used by a guard": the `usable` rule below)
            if loc != Loc::None {
                return Err(format!("snapshot {si}: dynamic string tag at {loc:?}"));
            }
        } else {
            tag = TAG_DYN_STR;
        }
    }
    match (src_of(loc), loc, t) {
        (Some(src), _, _) => Ok((tag, src)),
        (None, Loc::Const(c), _) => match ir.kval(c) {
            KVal::Int(v) => Ok((tag, ValSrc::Imm(v as u64))),
            KVal::Num(b) => Ok((tag, ValSrc::Imm(b))),
            _ => Err(format!("snapshot {si}: bad constant location")),
        },
        (None, Loc::None, Ty::Nil | Ty::False | Ty::True) => Ok((tag, ValSrc::Imm(0))),
        (None, Loc::None, _) => {
            *usable = false; // never used by a guard: the allocator left it unplaced
            Ok((tag, ValSrc::Imm(0)))
        }
        _ => Err(format!("snapshot {si}: bad location {loc:?}")),
    }
}

/// Resolves every snapshot of `ir` with the locations of `al` (P5a's `snap_locs`). `Err` names the first problem
/// (an unresolvable prototype or label, a slot of a raw machine type, an inconsistent frame count).
pub fn build_exits(
    ir: &TraceIr,
    al: &Allocation,
    res: &dyn Resolver,
) -> Result<(Vec<ExitSnap>, Vec<ExitDesc>), String> {
    if al.snap_locs.len() != ir.snaps.len() {
        return Err("allocation does not match the snapshots".into());
    }
    let sites = site_classes(ir);
    let mut out = Vec::with_capacity(ir.snaps.len());
    for (si, s) in ir.snaps.iter().enumerate() {
        let w = frame_window(ir, si);
        if w.end > ir.frames.len() || usize::from(s.nframes) > MAX_FRAMES {
            return Err(format!("snapshot {si}: bad frame window"));
        }
        if usize::from(s.frame) != usize::from(s.nframes) {
            return Err(format!("snapshot {si}: frame depth != nframes"));
        }
        let mut frames = Vec::new();
        let seg = ir.segs.get(usize::from(s.seg)).filter(|_| s.seg != 0);
        if s.seg != 0 && seg.is_none() {
            return Err(format!("snapshot {si}: segment {} does not exist", s.seg));
        }
        let mut key = seg.map_or(ir.head.0, |g| g.proto);
        if mutants::active("seg_land_proto_stale") {
            key = ir.head.0;
        }
        for l in &ir.frames[w] {
            let p = res
                .proto(l.proto)
                .ok_or_else(|| format!("snapshot {si}: unknown callee proto"))?;
            frames.push(FrameRestore {
                func_slot: l.func_slot,
                nresults: l.nresults,
                call_pc: l.call_pc,
                code: p.code,
                k: p.k,
                maxstack: p.maxstack,
                tail: l.tail,
                nextra: l.nextra,
                mm: l.mm,
            });
            key = l.proto;
        }
        let land_addr = res
            .land_addr(key, s.pc, s.land)
            .ok_or_else(|| format!("snapshot {si}: no landing label for pc {}", s.pc))?;
        let mut usable = true;
        let mut slots = Vec::new();
        let mut sunk: Vec<SunkTable> = Vec::new();
        let mut pending_key: Option<u64> = None;
        for (e, &loc) in ir.snap_entries(si as u32).iter().zip(&al.snap_locs[si]) {
            if e.r == NOREF {
                continue;
            }
            if e.flags & SNAP_SUNK_ANY != 0 {
                let int_of = |r: Ref| match TraceIr::is_const(r).then(|| ir.kval(r)) {
                    Some(KVal::Int(v)) => Some(v),
                    _ => None,
                };
                if e.flags & SNAP_SUNK != 0 {
                    let v = int_of(e.r).ok_or_else(|| format!("snapshot {si}: bad sunk header"))?;
                    sunk.push(SunkTable {
                        slot: e.slot,
                        b: (v & 0xFFFF) as u32,
                        c: ((v >> 16) & 0xFFFF) as u32,
                        fields: Vec::new(),
                        aliases: Vec::new(),
                        mt: None,
                    });
                } else if e.flags & SNAP_SMT != 0 {
                    let (tag, src) = slot_value(ir, si, e.r, loc, &mut usable)?;
                    sunk.last_mut()
                        .ok_or_else(|| format!("snapshot {si}: sunk metatable without table"))?
                        .mt = Some((tag, src));
                } else if e.flags & SNAP_SALIAS != 0 {
                    let first =
                        int_of(e.r).ok_or_else(|| format!("snapshot {si}: bad sunk alias"))?;
                    let t = sunk
                        .iter_mut()
                        .find(|t| i64::from(t.slot) == first)
                        .ok_or_else(|| format!("snapshot {si}: alias of an unknown sunk table"))?;
                    t.aliases.push(e.slot);
                } else if e.flags & SNAP_SFKEY != 0 {
                    pending_key = match TraceIr::is_const(e.r).then(|| ir.kval(e.r)) {
                        Some(KVal::Ptr(a, _)) => Some(a),
                        _ => return Err(format!("snapshot {si}: bad sunk key")),
                    };
                } else {
                    let (tag, src) = slot_value(ir, si, e.r, loc, &mut usable)?;
                    let key = pending_key
                        .take()
                        .ok_or_else(|| format!("snapshot {si}: sunk value without key"))?;
                    sunk.last_mut()
                        .ok_or_else(|| format!("snapshot {si}: sunk field without table"))?
                        .fields
                        .push(SunkField { key, tag, src });
                }
                continue;
            }
            let (tag, src) = slot_value(ir, si, e.r, loc, &mut usable)?;
            slots.push(SlotRestore {
                slot: e.slot,
                tag,
                src,
                func: e.flags & SNAP_FUNC != 0,
            });
        }
        out.push(ExitSnap {
            pc: s.pc,
            slots,
            frames,
            refund: s.refund,
            land: s.land,
            land_addr,
            usable,
            sunk,
            link_cell: 0,
            top: (s.top != 0).then(|| s.top - 1),
            root_code: seg.map_or(std::ptr::null(), |g| g.code as usize as *const u32),
            seg_shift: seg.map_or(0, |g| g.shift),
            seg_pops: seg.map_or(0, |g| g.pops),
        });
    }
    let pre = pre_roll_snaps(ir);
    let descs = (0..out.len())
        .map(|i| ExitDesc {
            snap: i as u32,
            site: sites[i],
            pre: pre[i],
        })
        .collect();
    Ok((out, descs))
}

// ---- machine-code contracts -------------------------------------------------------------------------------------

fn area_m(off: usize) -> M {
    M {
        base: Base::Cnt,
        off: exit_area::FROM_R15 + off as i32,
    }
}

/// What the shared exit routine needs to know about the runtime.
#[derive(Clone, Copy, Debug)]
pub struct SharedExitEnv {
    /// Address of [`jit_tr_exit`].
    pub handler: u64,
    /// Offset of the stack data pointer inside the `Thread` (`ReloadFrame`).
    pub stack_off: i32,
}

/// Appends the shared exit routine (5.6) to `p` and returns its label. Entry state: `r12` thread, `r13` root frame,
/// `r15` budget word, `trace_id` / `exitno` stored, every `V` / `X` register as the trace left it (`V7` is dead).
/// Exit: `JmpR` to `ExitArea.args[0]` with `r13`, `rbx`, `r14` set for `thread.ci`.
pub fn append_shared_exit(p: &mut Program, env: &SharedExitEnv) -> L {
    let l = p.label();
    p.push(Op::Bind(l));
    for v in 0..NGPR as u8 {
        if v != 7 {
            p.push(Op::St {
                w: W::B8,
                m: area_m(exit_area::GPR + 8 * usize::from(v)),
                src: Src::V(v),
            });
        }
    }
    for x in 0..NXMM as u8 {
        p.push(Op::StF {
            m: area_m(exit_area::XMM + 8 * usize::from(x)),
            src: x,
        });
    }
    p.push(Op::CallH {
        f: env.handler,
        a: 0,
        b: 0,
    });
    // F8 (TR29.3): a non-zero result (V0) = the handler recorded a memory error (a sunk table could not be allocated
    // while materialising): leave compiled code with `JitExit::Error`; the frame was restored by the handler
    let err = p.label();
    p.push(Op::Br {
        cc: Cc::Ne,
        a: 0,
        b: Src::Imm(0),
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
            off: ci::KCACHE as i32,
        },
    });
    p.push(Op::SetK(2));
    p.push(Op::Ld {
        dst: 3,
        w: W::B8,
        m: area_m(exit_area::ARGS),
    });
    // F2a: landing address 0 = leave compiled code for the interpreter at `savedpc` (the handler started a side
    // recording there, `ci.trap` is set); the restore has put the frame in the state tier 1 leaves it in
    let go = p.label();
    p.push(Op::Br {
        cc: Cc::Ne,
        a: 3,
        b: Src::Imm(0),
        to: go,
    });
    p.push(Op::Ret(JitExit::Continue as u32));
    p.push(Op::Bind(go));
    p.push(Op::JmpR(3));
    p.push(Op::Bind(err));
    p.push(Op::Ret(JitExit::Error as u32));
    l
}

/// Bit 31 of `ExitArea.exitno`: the exit is an *entry check* failure (3.3). The inline twin of an entry stub runs the
/// `jit_tr_entry_fail` threshold test itself; the generic twin sets this bit so that [`exit_entry`] runs it
/// (`cache::entry_fail_hot_id`) after counting the exit, which is what keeps the flush working in `generic_only` mode
/// and after the hot divert.
pub const EXIT_ENTRY: u32 = 1 << 31;

/// Bit 30 of `ExitArea.exitno` (P-L): the exit is a *poll* exit (the trace saw `state::TRACE_POLL` set at its loop head
/// or link). The handler restores the snapshot like any exit but does not count it, clears the poll word and leaves
/// compiled code for the interpreter at the snapshot's pc with `ci.trap = 1`, so that the interpreter looks at
/// `INTERRUPT` and the hooks (landing address 0, as for a side recording).
pub const EXIT_POLL: u32 = 1 << 30;

/// Every flag bit of `ExitArea.exitno`.
pub const EXIT_FLAGS: u32 = EXIT_ENTRY | EXIT_POLL;

/// The two stores of a generic stub, as `(offset in the ExitArea, width)`: `trace_id` then `exitno`, both `B4`.
pub const GENERIC_STUB_STORES: [(usize, W); 2] =
    [(exit_area::TRACE_ID, W::B4), (exit_area::EXITNO, W::B4)];

/// The generic stub of exit `exitno` of trace `trace_id` (`TrId::slot`): `St B4 trace_id; St B4 exitno; Mov V7,
/// shared; JmpR V7` (7.6). Nothing else is needed: the budget and every register are handled by the shared routine
/// and the handler.
pub fn generic_stub_ops(trace_id: u32, exitno: u32, shared: u64) -> [Op; 4] {
    [
        Op::St {
            w: W::B4,
            m: area_m(GENERIC_STUB_STORES[0].0),
            src: Src::Imm(trace_id as i32),
        },
        Op::St {
            w: W::B4,
            m: area_m(GENERIC_STUB_STORES[1].0),
            src: Src::Imm(exitno as i32),
        },
        Op::Mov(7, shared),
        Op::JmpR(7),
    ]
}

/// The inline stub of a frameless snapshot (7.6): for each slot `St B8 tag; St B8 payload`, the budget `vb + refund`
/// (through `V7`), `Mov V7, land; JmpR V7`. `vb` is where the budget register is at the guard (`Loc::None`: the budget
/// word already holds it). The exit counter increment is the assembler's (it needs a register that is dead at this exit).
pub fn inline_stub_ops(s: &ExitSnap, vb: Loc) -> Vec<Op> {
    inline_stub_ops_via(s, vb, None)
}

/// [`inline_stub_ops`] whose final jump goes through the exit cell at the absolute address `cell` (F2a, TR8): `Mov V7,
/// cell; Ld V7, [V7]; JmpR V7`. The cell holds the tier-1 landing address of `s` until a side trace is attached.
pub fn inline_stub_ops_via(s: &ExitSnap, vb: Loc, cell: Option<u64>) -> Vec<Op> {
    assert!(
        (s.frames.is_empty() || s.frames_plain()) && s.usable,
        "inline stubs are frameless (or S1 frame-cell stubs, or N3b frame-link stubs)"
    );
    assert!(vb_valid(vb), "budget register location {vb:?} (P4c)");
    let mut v = Vec::new();
    for e in &s.slots {
        let fm = |off: i32| M {
            base: Base::Frame,
            off: i32::from(e.slot) * 16 + off,
        };
        // the whole 8-byte tag word, as `restore` writes it (the upper bytes are zero: `tag` is below 0x100)
        if e.tag == TAG_DYN_STR {
            // N5: the variant of the string is read from its header: `tag = tt | ctb`
            match e.src {
                ValSrc::Reg(r) => v.push(Op::Ld {
                    dst: 7,
                    w: W::B1,
                    m: M {
                        base: Base::Reg(r),
                        off: rn_core::jit_layout::string::TT as i32,
                    },
                }),
                ValSrc::Spill(k) => {
                    v.push(Op::Ld {
                        dst: 7,
                        w: W::B8,
                        m: area_m(exit_area::SPILL + 8 * usize::from(k)),
                    });
                    v.push(Op::Ld {
                        dst: 7,
                        w: W::B1,
                        m: M {
                            base: Base::Reg(7),
                            off: rn_core::jit_layout::string::TT as i32,
                        },
                    });
                }
                _ => unreachable!("dynamic string tag without a register or spill source"),
            }
            v.push(Op::Or(7, Src::Imm(i32::from(BIT_ISCOLLECTABLE))));
            v.push(Op::St {
                w: W::B8,
                m: fm(0),
                src: Src::V(7),
            });
        } else {
            v.push(Op::St {
                w: W::B8,
                m: fm(0),
                src: Src::Imm(i32::from(e.tag)),
            });
        }
        match e.src {
            ValSrc::Reg(r) => v.push(Op::St {
                w: W::B8,
                m: fm(8),
                src: Src::V(r),
            }),
            ValSrc::Xmm(x) => v.push(Op::StF { m: fm(8), src: x }),
            ValSrc::Spill(k) => {
                v.push(Op::Ld {
                    dst: 7,
                    w: W::B8,
                    m: area_m(exit_area::SPILL + 8 * usize::from(k)),
                });
                v.push(Op::St {
                    w: W::B8,
                    m: fm(8),
                    src: Src::V(7),
                });
            }
            ValSrc::Imm(c) => {
                if let Ok(i) = i32::try_from(c as i64) {
                    v.push(Op::St {
                        w: W::B8,
                        m: fm(8),
                        src: Src::Imm(i),
                    });
                } else {
                    v.push(Op::Mov(7, c));
                    v.push(Op::St {
                        w: W::B8,
                        m: fm(8),
                        src: Src::V(7),
                    });
                }
            }
        }
    }
    match vb {
        Loc::G(r) => v.push(Op::MovR(7, r)),
        Loc::Spill(k) => v.push(Op::Ld {
            dst: 7,
            w: W::B8,
            m: area_m(exit_area::SPILL + 8 * usize::from(k)),
        }),
        Loc::None => v.push(Op::Ld {
            dst: 7,
            w: W::B4,
            m: M {
                base: Base::Cnt,
                off: 0,
            },
        }),
        Loc::X(_) | Loc::Const(_) => unreachable!("checked by vb_valid"),
    }
    if s.refund != 0 {
        v.push(Op::Add(7, Src::Imm(s.refund)));
    }
    v.push(Op::St {
        w: W::B4,
        m: M {
            base: Base::Cnt,
            off: 0,
        },
        src: Src::V(7),
    });
    match cell {
        Some(c) => {
            v.push(Op::Mov(7, c));
            v.push(Op::Ld {
                dst: 7,
                w: W::B8,
                m: M {
                    base: Base::Reg(7),
                    off: 0,
                },
            });
        }
        None => v.push(Op::Mov(7, s.land_addr)),
    }
    v.push(Op::JmpR(7));
    v
}

/// H2: may the inline stub of this snapshot rebuild its frames itself (`frame_mat_ops`)? Plain or tail Lua frames of a
/// frame-side-capable snapshot; metamethod frames (they count `nCcalls`) keep the handler.
pub fn frame_mat_ok(s: &ExitSnap) -> bool {
    s.frame_side_ok() && s.sunk.is_empty() && s.frames.iter().all(|f| !f.mm)
}

/// N3b (`retf` bit 1): may the inline stub of this LINKING snapshot (the frame-linking exit of a side trace: frames rebuilt, then a jump through the
/// head's dispatch cell) rebuild its frames itself? Plain or tail Lua frames (no metamethod frame), nothing sunk.
pub fn frame_link_mat_ok(s: &ExitSnap) -> bool {
    s.link_cell != 0 && s.frames_plain() && s.sunk.is_empty() && s.frames.iter().all(|f| !f.mm)
}

/// N3b: the `savedpc` the innermost frame of a linking snapshot resumes at (input `V5` of the shared frame rebuild).
pub fn frame_link_innermost_pc(s: &ExitSnap, root_code: u64) -> u64 {
    frame_mat_innermost_pc(s, root_code).wrapping_add(if mutants::active("relink_savedpc_off") {
        4
    } else {
        0
    })
}

/// N3b: where the stub of a linking snapshot continues after the rebuild: `(address, through)`; `through`: the address is a dispatch cell holding the target
/// (read at run time: a flushed head's cell holds the tier-1 label again), otherwise the address itself.
pub fn frame_link_target(s: &ExitSnap) -> (u64, bool) {
    if mutants::active("relink_jumps_to_land") {
        (s.land_addr, false)
    } else {
        (s.link_cell, true)
    }
}

/// H2: the shared part of the inline stub of a frameful snapshot (after the slot stores and the budget store, every `V` register
/// is dead): the machine-code twin of [`restore`]'s step (2) and (3) for a hook-free thread, for the chain `frames` below a root frame
/// whose prototype code is `root_code`. Input: `V5` = the innermost frame's `savedpc` (its code address plus `pc`). Uses `V1` = root
/// `func` + 1 (the stack index of slot 0), `V2` = the previous node, `V3` = the next node, `V4` = scratch; ends with `SetCi(V2)`
/// (the innermost node), the caller adds the frame / constants reload and the jump. Snapshots with the same chain share one copy.
pub fn frame_mat_shared_ops(frames: &[FrameRestore], root_code: u64) -> Vec<Op> {
    let ld8 = |dst, base, off: usize| Op::Ld {
        dst,
        w: W::B8,
        m: M {
            base,
            off: off as i32,
        },
    };
    let st = |w, base, off: usize, src| Op::St {
        w,
        m: M {
            base,
            off: off as i32,
        },
        src,
    };
    let mut v = vec![ld8(1, Base::Ci, ci::FUNC), Op::Add(1, Src::Imm(1))];
    let mut prev = Base::Ci;
    let mut prev_code = root_code;
    for f in frames {
        let after = if mutants::active("fmat_savedpc_caller_off") {
            f.call_pc
        } else {
            f.call_pc + 1
        };
        v.push(ld8(3, prev, ci::NEXT));
        v.push(Op::Mov(4, prev_code.wrapping_add(4 * u64::from(after))));
        v.push(st(W::B8, prev, ci::SAVEDPC, Src::V(4)));
        v.push(Op::MovR(4, 1));
        v.push(Op::Add(4, Src::Imm(i32::from(f.func_slot))));
        v.push(st(W::B8, Base::Reg(3), ci::FUNC, Src::V(4)));
        v.push(Op::Add(
            4,
            Src::Imm(1 + i32::from(f.maxstack) + i32::from(mutants::active("fmat_top_off_by_one"))),
        ));
        v.push(st(W::B8, Base::Reg(3), ci::TOP, Src::V(4)));
        let nres = f.nresults as i32
            + 1
            + if mutants::active("fmat_nresults_off") {
                1
            } else {
                0
            };
        let cs = nres as u32
            | if f.tail && !mutants::active("fmat_tail_flag_skip") {
                CIST_TAIL
            } else {
                0
            };
        v.push(st(W::B4, Base::Reg(3), ci::CALLSTATUS, Src::Imm(cs as i32)));
        if !mutants::active("fmat_trap_skip") {
            v.push(st(W::B4, Base::Reg(3), ci::TRAP, Src::Imm(0)));
        }
        v.push(Op::Mov(4, f.k as u64));
        v.push(st(W::B8, Base::Reg(3), ci::KCACHE, Src::V(4)));
        v.push(Op::Mov(4, f.code as u64));
        v.push(st(W::B8, Base::Reg(3), ci::SAVEDPC, Src::V(4)));
        v.push(Op::MovR(2, 3));
        prev = Base::Reg(2);
        prev_code = f.code as u64;
    }
    v.push(st(W::B8, prev, ci::SAVEDPC, Src::V(5)));
    v.push(st(W::B8, Base::Thread, thr::CI, Src::V(2)));
    v.push(ld8(4, prev, ci::TOP));
    if !mutants::active("fmat_th_top_skip") {
        v.push(st(W::B8, Base::Thread, thr::TOP, Src::V(4)));
    }
    v.push(Op::SetCi(2));
    v
}

/// H2: the code address the innermost frame of `s` resumes at (`savedpc` of the restored chain) in the form [`frame_mat_shared_ops`] takes
/// in `V5`.
pub fn frame_mat_innermost_pc(s: &ExitSnap, root_code: u64) -> u64 {
    let code = s.frames.last().map_or(root_code, |f| f.code as u64);
    code.wrapping_add(4 * u64::from(s.pc))
}

/// H2: [`frame_mat_shared_ops`] for one snapshot with its input set up (the form the emulator test runs; `ExitSnap::frames` must satisfy
/// [`frame_mat_ok`]).
pub fn frame_mat_ops(s: &ExitSnap, root_code: u64) -> Vec<Op> {
    debug_assert!(frame_mat_ok(s));
    let mut v = vec![Op::Mov(5, frame_mat_innermost_pc(s, root_code))];
    v.extend(frame_mat_shared_ops(&s.frames, root_code));
    v
}

/// May `vb` be the location of the budget register at an exit? **The one rule of both exit paths** ([`restore`] and
/// [`inline_stub_ops`] assert it): a general register other than `V7` (scratch of the stubs), a spill slot, or
/// `Loc::None` (the budget word itself holds `vb`). RA never places the budget in an `X` register: `Budget` has a raw
/// operand and `vb` is a 64-bit integer; `Loc::X` and `Loc::Const` are rejected rather than read from two sources.
pub fn vb_valid(vb: Loc) -> bool {
    match vb {
        Loc::G(r) => r != 7 && usize::from(r) < NGPR,
        Loc::Spill(k) => usize::from(k) < super::NSPILL,
        Loc::None => true,
        Loc::X(_) | Loc::Const(_) => false,
    }
}

// ---- the restore ------------------------------------------------------------------------------------------------

/// Raw access to the state the restore writes. Built by [`exit_entry`] from the live thread; tests build it over
/// synthetic memory.
#[derive(Clone, Copy, Debug)]
pub struct RestoreCtx {
    /// Address of stack slot 0 (`Value` 16 bytes each); only offsets of valid slots are dereferenced.
    pub stack0: *mut u8,
    /// Usable stack slots.
    pub stack_size: usize,
    /// The root frame (`r13`).
    pub root: *mut CallInfo,
    /// `&thread.ci`.
    pub th_ci: *mut *mut CallInfo,
    /// `&thread.top` (a stack index).
    pub th_top: *mut usize,
    /// The budget word (`r15`).
    pub budget: *mut i32,
    /// `&thread.ncalls` (N3: metamethod frames count as C calls); null for a context without such frames.
    pub ncalls: *mut u32,
    /// `thread.hookmask != 0`: rebuilt frames get `trap = 1`, as the interpreter's start of a function sets it.
    pub hooked: bool,
}

fn read_src(area: &ExitArea, s: ValSrc) -> u64 {
    match s {
        ValSrc::Reg(n) => area.gpr[usize::from(n)],
        ValSrc::Xmm(n) => area.xmm[usize::from(n)],
        ValSrc::Spill(n) => area.spill[usize::from(n)],
        ValSrc::Imm(v) => v,
    }
}

/// J1-2: steps (2) and (3) of [`restore`] without the slot writes and the budget: the inlined frames (as `pre_call_lua_fast` fills them), the
/// innermost frame's `savedpc`, `thread.ci` and `thread.top`. The inline materialising exit stub, which stored the slots and the budget
/// itself, uses it when a host allocation failed (the memory error is raised at the exit's pc).
///
/// # Safety
/// The contract of [`restore`].
pub unsafe fn restore_chain(ctx: &RestoreCtx, s: &ExitSnap, root_code: *const u32) {
    // SAFETY: `root` is the live running frame (contract of `restore`).
    let base = unsafe { (*ctx.root).func.0 } + 1;
    // (2) frames
    let mut prev = ctx.root;
    let mut prev_code = root_code;
    // N3: the C-stack depth the next metamethod frame starts from (`mm_frame` stores it in `ctx` and counts the frame)
    let mut mm_nc = if s.frames.iter().any(|f| f.mm) {
        assert!(
            !ctx.ncalls.is_null(),
            "a metamethod frame without the thread's nCcalls"
        );
        // SAFETY: `ncalls` is `&thread.ncalls` of the live thread (contract).
        unsafe { ctx.ncalls.read() }
    } else {
        0
    };
    for f in &s.frames {
        // SAFETY: `prev` is a live node of the thread's CallInfo list (contract).
        let next = unsafe { (*prev).next };
        assert!(!next.is_null(), "no spare CallInfo (TR6b)");
        let func = base + usize::from(f.func_slot);
        let top = func
            + 1
            + usize::from(f.maxstack)
            + usize::from(mutants::active("exit_top_off_by_one"));
        assert!(top <= ctx.stack_size, "inlined frame beyond the stack");
        let nres = f.nresults as i32
            + if mutants::active("frame_nresults_off") {
                2
            } else {
                1
            };
        let after_call = if mutants::active("frame_savedpc_caller_off") {
            f.call_pc as usize
        } else {
            f.call_pc as usize + 1
        };
        // SAFETY: `prev` and `next` are live nodes; `prev_code` is the caller prototype's code and `call_pc` a
        // valid index of it (contract); the writes are plain field stores into nodes only this thread touches.
        unsafe {
            std::ptr::addr_of_mut!((*prev).savedpc).write(prev_code.add(after_call));
            std::ptr::addr_of_mut!((*next).func).write(StkId(func));
            std::ptr::addr_of_mut!((*next).top).write(StkId(top));
            let tail = if f.tail && !mutants::active("tail_flag_skip") {
                CIST_TAIL
            } else {
                0
            };
            let mmf = if f.mm && !mutants::active("mm_flag_skip") {
                CIST_MMRET
            } else {
                0
            };
            std::ptr::addr_of_mut!((*next).callstatus).write(nres as u32 | tail | mmf);
            if f.mm {
                std::ptr::addr_of_mut!((*next).ctx)
                    .write(mm_nc as isize + isize::from(mutants::active("mm_ctx_off")));
                mm_nc += 1;
                if !mutants::active("mm_ncalls_skip") {
                    ctx.ncalls.write(mm_nc);
                }
            }
            std::ptr::addr_of_mut!((*next).trap).write(i32::from(ctx.hooked));
            let k = if mutants::active("exit_kcache_wrong") {
                f.k.wrapping_add(16)
            } else {
                f.k
            };
            std::ptr::addr_of_mut!((*next).kcache).write(k.cast());
            std::ptr::addr_of_mut!((*next).savedpc).write(f.code);
            if f.nextra >= 0 {
                let n = if mutants::active("vararg_nextra_off") {
                    f.nextra + 1
                } else {
                    f.nextra
                };
                std::ptr::addr_of_mut!((*next).nextraargs).write(n);
            }
        }
        prev = next;
        prev_code = f.code;
    }
    // (3) innermost frame, thread, budget
    // SAFETY: `prev` is the innermost node; `pc` indexes its prototype's code (contract); `th_ci`, `th_top` and
    // `budget` point into the live thread / JitData (contract).
    unsafe {
        std::ptr::addr_of_mut!((*prev).savedpc).write(prev_code.add(s.pc as usize));
        ctx.th_ci.write(prev);
        if !mutants::active("exit_th_top_not_written") {
            // F4: a consumer of a multiple-result sequence re-executes with the interpreter's `top` (TR22b relaxed)
            match s.top {
                Some(t) if !mutants::active("exit_top_ignored") => {
                    let at = base
                        + usize::from(t)
                        + usize::from(mutants::active("exit_top_off_by_one_slot"));
                    assert!(at <= ctx.stack_size, "snapshot top beyond the stack");
                    ctx.th_top.write(at);
                }
                _ => ctx.th_top.write((*prev).top.0),
            }
        }
    }
}

/// The 5.5 restore for snapshot `s` with the register state of `area`: (1) writes every snapshot slot (tag + payload);
/// (2) for each inlined frame `k`, `ci_k = ci_{k-1}.next` is filled as `pre_call_lua_fast` fills it (`func`, `top =
/// func + 1 + maxstack`, `callstatus = nresults + 1`, `kcache`, `trap = hooked`) and the caller's `savedpc` becomes the
/// instruction after its `CALL`; (3) the innermost frame's `savedpc = &code[pc]`, `thread.ci`, `thread.top = ci.top`,
/// and the budget word `vb + refund` (TR5.4). Returns the landing address.
///
/// # Safety
/// `ctx` must describe the live thread the trace ran on: `root` is its running Lua frame whose `next` chain has at
/// least `s.frames.len()` allocated nodes (the entry check TR6b), the stack has at least `stack_size` slots from
/// `stack0`, and nothing else accesses these structures during the call (TR2). The pointers inside `s` (prototype code
/// and constants) must be valid for the instruction/constant indices used (`pc`, `call_pc + 1` are in range of their
/// prototype's code). Slot and `top` indices are checked against `stack_size` (a bad snapshot panics instead of
/// writing out of bounds).
pub unsafe fn restore(
    ctx: &RestoreCtx,
    s: &ExitSnap,
    root_code: *const u32,
    vb: Loc,
    area: &ExitArea,
) -> u64 {
    assert!(s.usable, "restore of a snapshot no guard uses");
    assert!(vb_valid(vb), "budget register location {vb:?} (P4c)");
    // SAFETY: `root` is the live running frame (contract).
    let base = unsafe { (*ctx.root).func.0 } + 1;
    // (1) slots
    for e in &s.slots {
        let idx = base + usize::from(e.slot);
        assert!(idx < ctx.stack_size, "snapshot slot beyond the stack");
        let v = read_src(area, e.src);
        // SAFETY: `idx < stack_size`, so the 16-byte slot is inside the stack (contract); the write is
        // unaligned-safe and covers the whole `Value` (widened tag, then payload).
        unsafe {
            let p = ctx.stack0.wrapping_add(idx * 16);
            let tag = if e.tag == TAG_DYN_STR {
                // N5: the string's variant, from its header (`v` is a live `LuaString`)
                u64::from(
                    *(v as *const u8).add(rn_core::jit_layout::string::TT) | BIT_ISCOLLECTABLE,
                )
            } else {
                u64::from(e.tag)
            };
            p.cast::<u64>().write_unaligned(tag);
            p.add(8).cast::<u64>().write_unaligned(v);
        }
    }
    // (2) frames, (3) innermost frame and thread
    // SAFETY: the contract of this function is the contract of `restore_chain`.
    unsafe { restore_chain(ctx, s, root_code) };
    // (3') the budget word
    // SAFETY: `budget` points into the live JitData (contract).
    unsafe {
        let vbv = match vb {
            Loc::G(r) => area.gpr[usize::from(r)],
            Loc::Spill(k) => area.spill[usize::from(k)],
            Loc::None => ctx.budget.read() as u32 as u64,
            Loc::X(_) | Loc::Const(_) => unreachable!("checked by vb_valid"),
        };
        let refund = if mutants::active("exit_refund_sign_flipped") {
            (s.refund as u32).wrapping_neg()
        } else {
            s.refund as u32
        };
        ctx.budget.write((vbv as u32).wrapping_add(refund) as i32);
    }
    s.land_addr
}

/// J1-2: the machine code that stores the registers the sunk tables of `s` read (field values, metatables) into the exit area, where
/// [`materialise`] finds them (`read_src`). Spill slots are in the area already, constants are in the descriptor.
pub fn sunk_save_ops(s: &ExitSnap) -> Vec<Op> {
    let mut gpr = [false; NGPR];
    let mut xmm = [false; NXMM];
    let mut note = |src: ValSrc| match src {
        ValSrc::Reg(r) => gpr[usize::from(r)] = true,
        ValSrc::Xmm(x) => xmm[usize::from(x)] = true,
        ValSrc::Spill(_) | ValSrc::Imm(_) => {}
    };
    for t in &s.sunk {
        for f in &t.fields {
            note(f.src);
        }
        if let Some((_, src)) = t.mt {
            note(src);
        }
    }
    let mut v = Vec::new();
    for (r, used) in gpr.iter().enumerate() {
        if *used && !mutants::active("sinkstub_save_skip") {
            v.push(Op::St {
                w: W::B8,
                m: area_m(exit_area::GPR + 8 * r),
                src: Src::V(r as u8),
            });
        }
    }
    for (x, used) in xmm.iter().enumerate() {
        if *used && !mutants::active("sinkstub_save_skip") {
            v.push(Op::StF {
                m: area_m(exit_area::XMM + 8 * x),
                src: x as u8,
            });
        }
    }
    v
}

/// J1-2: the frameful twin of [`sunk_stub_ok`]: plain or tail Lua frames (no metamethod or vararg frame, as for [`frame_mat_ok`]), no `top`,
/// no link. The stub rebuilds the frames inline after the helper has created the tables.
pub fn sunk_frame_stub_ok(s: &ExitSnap) -> bool {
    !s.sunk.is_empty()
        && !s.frames.is_empty()
        && s.frames.iter().all(|f| f.nextra < 0 && !f.mm)
        && s.top.is_none()
        && s.usable
        && s.link_cell == 0
}

/// J1-2: may the inline stub of `s` materialise its sunk tables itself (through [`jit_tr_mat`])? Frameless, no `top`, no link.
pub fn sunk_stub_ok(s: &ExitSnap) -> bool {
    !s.sunk.is_empty() && s.frames.is_empty() && s.top.is_none() && s.usable && s.link_cell == 0
}

/// TR26c: creates the sunk tables of `s` after [`restore`] wrote the snapshot slots. Runs with the collector stopped
/// (`GCSTP_GC`): each table is allocated with the `NEWTABLE`'s sizes, stored into its slot at once, then gets its fields
/// in insertion order through the table's own `set_str` (same sizes and order give the interpreter's node layout), and
/// its bytes move from virtual to real accounting.
///
/// Returns `false` when a host allocation failed (TR29.3: the caller raises the Lua memory error at the exit's landing
/// pc): the tables created so far are in their slots, the others are not created (their slots keep the values the
/// restore found), the collector is re-enabled and the virtual bytes of the tables not created stay charged (dead,
/// credited at the next cycle end like any dead sunk table).
///
/// # Safety
/// `ctx` and `area` as for [`restore`]; the key addresses of the fields are live interned short strings and the
/// collectable values are live objects of this state.
pub unsafe fn materialise(st: &mut State, ctx: &RestoreCtx, s: &ExitSnap, area: &ExitArea) -> bool {
    // SAFETY: `root` is the live running frame (contract of `restore`).
    let base = unsafe { (*ctx.root).func.0 } + 1;
    let heap = st.heap();
    let oldstp = heap.gc_stp();
    heap.gc_set_stp(oldstp | rn_core::gc::GCSTP_GC);
    // the failure exit: re-enables the collector (the virtual bytes of the table that was not created were credited already:
    // it never existed, neither virtually)
    macro_rules! mat_failed {
        () => {{
            if !mutants::active("sink_no_gcstp") {
                heap.gc_set_stp(oldstp);
            }
            false
        }};
    }
    let put = |slot: u16, tag: u64, payload: u64| {
        let idx = base + usize::from(slot);
        assert!(idx < ctx.stack_size, "sunk slot beyond the stack");
        // SAFETY: `idx < stack_size` (checked above): the 16-byte slot is inside the stack.
        unsafe {
            let p = ctx.stack0.wrapping_add(idx * 16);
            p.cast::<u64>().write_unaligned(tag);
            p.add(8).cast::<u64>().write_unaligned(payload);
        }
    };
    super::sink::bump(&super::sink::MATERIALISED, s.sunk.len() as u64);
    for t in &s.sunk {
        let hsize = if t.b > 0 { 1u32 << (t.b - 1) } else { 0 };
        let bytes = rn_core::table::new_table_bytes(t.c, hsize).expect("sunk table size");
        if !mutants::active("sink_no_uncharge") {
            heap.uncharge_virtual(bytes);
        }
        let tab = match (mat_fault(), heap.new_table()) {
            (false, Ok(tab)) => tab,
            _ => return mat_failed!(),
        };
        if (t.b != 0 || t.c != 0) && (mat_fault() || tab.resize(heap, t.c, hsize).is_err()) {
            // (the half-built table is garbage; it is not rooted anywhere)
            return mat_failed!();
        }
        let tag = u64::from(ctb(LUA_VTABLE));
        let ptr = tab.as_ptr() as u64;
        put(t.slot, tag, ptr);
        for f in &t.fields {
            // SAFETY: `Value` is `repr(C)` `{ tag word, payload }`; the key is a live interned short string and the
            // value a number, boolean or live object (contract).
            let (kv, v) = unsafe {
                (
                    std::mem::transmute::<[u64; 2], Value>([u64::from(ctb(LUA_VSHRSTR)), f.key]),
                    std::mem::transmute::<[u64; 2], Value>([
                        u64::from(f.tag),
                        read_src(area, f.src),
                    ]),
                )
            };
            let key = kv.as_strref().expect("short string key");
            if mat_fault() || tab.set_str(heap, key, &v).is_err() {
                // (the table is already in its slot: it is rooted, partly filled, and nothing can observe it before the
                // error unwinds the frame)
                return mat_failed!();
            }
        }
        if let Some((mtag, msrc)) = t.mt {
            // BUNDLE-2: the `setmetatable` of the sunk table (the trace guarded that the metatable has no `__gc`: no finalizer to register)
            // SAFETY: `Value` is `repr(C)` `{ tag word, payload }`; the metatable is a live table (a value of the trace).
            let mv = unsafe {
                std::mem::transmute::<[u64; 2], Value>([u64::from(mtag), read_src(area, msrc)])
            };
            if let Some(m) = mv.as_table() {
                if !mutants::active("sinkmt_no_mt_materialise") {
                    tab.set_metatable(Some(m));
                    heap.barrier(tab.as_ptr().cast(), &mv);
                }
            }
        }
        if !mutants::active("sink_slots_after_alloc") {
            for &a in &t.aliases {
                put(a, tag, ptr);
            }
        }
    }
    if !mutants::active("sink_no_gcstp") {
        heap.gc_set_stp(oldstp);
    }
    true
}

thread_local! {
    /// Test hook: the n-th (1-based) host allocation of a materialisation fails.
    static MAT_FAULT: Cell<u32> = const { Cell::new(0) };
}

/// Test hook (`set_materialise_fault`): makes the `n`-th host allocation of the materialisations on this OS thread fail
/// (`0` = off). The counter is consumed by failing, so exactly one allocation fails.
#[doc(hidden)]
pub fn set_materialise_fault(n: u32) {
    MAT_FAULT.with(|c| c.set(n));
}

/// Does the injected fault hit this allocation?
fn mat_fault() -> bool {
    MAT_FAULT.with(|c| match c.get() {
        0 => false,
        1 => {
            c.set(0);
            true
        }
        n => {
            c.set(n - 1);
            false
        }
    })
}

/// A test hook called inside the exit handler.
pub type ProbeFn = fn(&mut State, TrId);

thread_local! {
    /// Test hook: called inside the handler after the exit was counted (and a hot flush requested), while the handler
    /// is still active.
    static PROBE: RefCell<Option<ProbeFn>> = const { RefCell::new(None) };
}

/// Installs (or removes) a probe called inside the exit handler of this OS thread (tests).
#[doc(hidden)]
pub fn set_probe(f: Option<ProbeFn>) {
    PROBE.with(|p| *p.borrow_mut() = f);
}

// ---- the handler ------------------------------------------------------------------------------------------------

/// What [`exit_core`] did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CoreOut {
    /// The exit number ([`EXIT_ENTRY`] stripped).
    pub exitno: u32,
    /// The exit was an entry check failure (`exitno` carried [`EXIT_ENTRY`]).
    pub entry: bool,
    /// The site class of the exit.
    pub site: ExitSite,
    /// The exit is a pre-roll exit ([`ExitDesc::pre`]).
    pub pre: bool,
    /// The exit counter after this exit (0 when the trace has no counters).
    pub count: u32,
    /// The landing address written to `args[0]`.
    pub land: u64,
    /// F2a: the exit continues in a side trace (its cell no longer holds the tier-1 landing address).
    pub diverted: bool,
    /// F2a: the exit continues through its cell (a frameless snapshot of a trace that has cells).
    pub diverted_possible: bool,
}

/// N2c: the segment oracle (`+verify`): `r13` is the entry frame popped `seg_pops` times, its function slot `seg_shift` below the entry's.
/// The entry frame's function slot is read BEFORE `restore` (a restore that rebuilds frames behind the caller reuses the popped frame's
/// `CallInfo` node, which is the entry node itself, and overwrites its `func`): [`seg_oracle_entry_func`] first, [`seg_oracle_check`] after.
///
/// # Safety
/// The entry `CallInfo` stored at the trace's entry is live.
#[cold]
#[inline(never)]
unsafe fn seg_oracle_entry_func(meta: &ExitMeta, area: &ExitArea) -> usize {
    let slot = usize::from(meta.seg_oracle - 1);
    let entry = area.spill[slot] as usize as *mut CallInfo;
    // SAFETY: `entry` was stored by the trace's entry sequence (a live `CallInfo` of the running thread).
    unsafe { (*entry).func.0 }
}

/// # Safety
/// `ctx.root` is the live `CallInfo` the exit runs in; the entry `CallInfo` stored at the trace's entry is live (as a node: only its `previous` chain is read).
#[cold]
#[inline(never)]
unsafe fn seg_oracle_check(
    meta: &ExitMeta,
    snap: &ExitSnap,
    ctx: &RestoreCtx,
    area: &ExitArea,
    entry_func: usize,
) {
    let slot = usize::from(meta.seg_oracle - 1);
    let entry = area.spill[slot] as usize as *mut CallInfo;
    // SAFETY: each `previous` of a live Lua frame is a live node.
    unsafe {
        let mut c = entry;
        for _ in 0..snap.seg_pops {
            c = (*c).previous;
        }
        let want_func = entry_func - usize::from(snap.seg_shift);
        assert_eq!(
            c, ctx.root,
            "segment oracle: r13 is not the entry frame popped {} times",
            snap.seg_pops
        );
        assert_eq!(
            (*ctx.root).func.0,
            want_func,
            "segment oracle: the function slot of the caller frame"
        );
    }
}

/// Is an exit of class `site` that has been taken `n` times reported as hot at threshold `hot` (3.3, 7.9)? Type-guard
/// exits keep reporting from the threshold on (the first report flushes, later ones find a stale id); other exits report
/// exactly once.
pub fn is_hot(site: ExitSite, n: u32, hot: u32) -> bool {
    match site {
        ExitSite::TypeGuard => n >= hot,
        ExitSite::Other | ExitSite::Alloc => n == hot,
    }
}

/// The state-free part of the handler: restores snapshot `area.exitno` of `meta` into `ctx`, counts the exit and
/// leaves the landing address in `area.args[0]`.
///
/// # Safety
/// The contract of [`restore`]; `meta.counters` (when non-null) points at `meta.exits.len()` live cells.
pub unsafe fn exit_core(meta: &ExitMeta, ctx: &RestoreCtx, area: &mut ExitArea) -> CoreOut {
    let entry = area.exitno & EXIT_ENTRY != 0;
    let exitno = area.exitno & !EXIT_FLAGS;
    let ex = meta.exits[exitno as usize];
    let snap = &meta.snaps[ex.snap as usize];
    let oracle = snap.seg_pops != 0 && meta.seg_oracle != 0;
    // SAFETY: contract of this function (the entry node is live until the restore).
    let entry_func = if oracle {
        unsafe { seg_oracle_entry_func(meta, area) }
    } else {
        0
    };
    // SAFETY: contract of this function.
    let land = unsafe { restore(ctx, snap, snap.code_or(meta.root_code), meta.vb, area) };
    if oracle {
        // SAFETY: contract of this function (`ctx.root` is the live frame the exit runs in).
        unsafe { seg_oracle_check(meta, snap, ctx, area, entry_func) };
    }
    // F2a: a frameless exit continues where its cell says (the tier-1 label, or the entry of a side trace)
    let mut diverted = false;
    let via_cell =
        !meta.cells.is_null() && snap.frames.is_empty() && !mutants::active("exit_cell_ignored");
    let linked = snap.link_cell != 0 && !mutants::active("frame_link_cell_ignored");
    let land = if linked {
        // F2b: a linking exit continues at the dispatch cell of the head it links to
        // SAFETY: `link_cell` is the address of a `Cell<usize>` in the `TrProto` of a prototype this trace depends on
        // (TR32: the trace is flushed before the prototype is freed); the cell is address-stable.
        unsafe { &*(snap.link_cell as *const Cell<usize>) }.get() as u64
    } else if via_cell {
        // SAFETY: `cells` points at `snaps.len()` live cells owned by the trace and `ex.snap` indexed `snaps`.
        let c = unsafe { &*meta.cells.add(ex.snap as usize) }.get() as u64;
        diverted = c != land;
        c
    } else {
        land
    };
    area.args[0] = land;
    let mut count = 0;
    if !meta.counters.is_null() {
        // SAFETY: `counters` points at `exits.len()` cells owned by the (live) trace and `exitno` indexed `exits`.
        let c = unsafe { &*meta.counters.add(exitno as usize) };
        let old = c.get();
        count = super::cache::exit_count(old).saturating_add(1);
        c.set(count | (old & super::cache::EXIT_BLACK_BIT));
    }
    CoreOut {
        exitno,
        entry,
        site: ex.site,
        pre: ex.pre,
        count,
        land,
        diverted,
        diverted_possible: via_cell,
    }
}

/// What the handler did (tests).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Handled {
    /// The trace that exited.
    pub id: TrId,
    /// What [`exit_core`] reported.
    pub out: CoreOut,
    /// The exit reached the hot threshold and a flush was requested.
    pub flushed: bool,
    /// F2a: the exit reached the hot threshold and a side recording was started at its pc: the handler left the budget
    /// in the interpreter's state and `args[0]` is 0 (the shared routine returns to the interpreter).
    pub side_started: bool,
    /// F8 (TR29.3): materialising a sunk table failed to allocate; the memory error is recorded (`jit_hooks::set_error`)
    /// and the shared exit routine leaves compiled code with [`JitExit::Error`].
    pub mem_error: bool,
}

/// The exit handler: finds the trace of `area.trace_id`, restores snapshot `area.exitno`, counts the exit and reports a
/// hot one, and leaves the landing address in `area.args[0]`. Flushes requested here are deferred (TR18: the handler
/// was entered from the trace's own block). The area is the one [`exit_area::FROM_R15`] bytes behind the budget word.
///
/// # Safety
/// `th` is the live thread pinned in `r12`, `ci` its running Lua frame (`r13`), `budget` the budget word (`r15`) of a
/// block whose exit area follows it, holding the registers saved by the shared routine; the contract of [`restore`]
/// applies. `budget` must have provenance over the word and the area (the whole `JitData` prefix).
pub unsafe fn exit_entry(th: *mut Thread, ci: *mut CallInfo, budget: *mut i32) -> Handled {
    // SAFETY: `th` is the live running thread (contract).
    let mut st = unsafe { jit_hooks::state_from_thread(th.cast()) };
    // SAFETY: the area follows the budget word inside the same allocation (contract); nothing else accesses it.
    let area = unsafe {
        &mut *budget
            .cast::<u8>()
            .add(exit_area::FROM_R15 as usize)
            .cast::<ExitArea>()
    };
    // The `Arc` keeps the metadata alive while the handler runs, even if the trace is flushed meanwhile.
    // P-L: the metadata is borrowed from the trace body, not reference counted (`cache::exit_meta_ptr`); the handler
    // marks itself active right below, before the pointer is used
    let (id, meta) = cache::exit_meta_ptr(&mut st, area.trace_id).unwrap_or_else(|| {
        panic!(
            "exit of a trace without attached metadata (`cache::attach_exits`): slot {} exit {:#x} is {}",
            area.trace_id,
            area.exitno,
            cache::slot_kind(&mut st, area.trace_id)
        )
    });
    cache::handler_enter(&mut st);
    // SAFETY: `handler_enter` has run: flushes are deferred, so the trace body (which owns the `Arc`) outlives this
    // function (`exit_meta_ptr`); the pointer came from a live `Arc` and the metadata is immutable.
    let meta: &ExitMeta = unsafe { &*meta };
    // BUNDLE-K L1: a lazy region is completed first (every exit path of a lazy-aware trace that reaches the handler)
    if meta.lazy && area.lazy_root != 0 && !mutants::active("lazy_leave_in_handler_skip") {
        // SAFETY: contract of this function (`ci` is the innermost frame `r13`).
        unsafe { lazy_leave(th, ci, area, 0) };
    }
    let th8 = th.cast::<u8>();
    let ctx = RestoreCtx {
        stack0: st.slot_ptr(StkId(0)).cast(),
        stack_size: st.stack_size(),
        hooked: st.hook_mask() != 0,
        root: ci,
        th_ci: th8.wrapping_add(thr::CI).cast(),
        th_top: th8.wrapping_add(thr::TOP).cast(),
        ncalls: th8.wrapping_add(rn_core::jit_hooks::thread::NCALLS).cast(),
        budget,
    };
    if area.exitno & EXIT_POLL != 0 {
        // SAFETY: contract of this function; `handler_enter` has run.
        return unsafe { poll_exit(&mut st, meta, &ctx, area, id, ci) };
    }
    // S1: the thread's `nCcalls` before a restore that counts metamethod frames (a frame-cell exit unwinds it again)
    let ncalls0 = if ctx.ncalls.is_null() {
        0
    } else {
        // SAFETY: `ncalls` is `&thread.ncalls` of the live thread (contract).
        unsafe { ctx.ncalls.read() }
    };
    // SAFETY: contract of this function.
    let out = unsafe { exit_core(meta, &ctx, area) };
    {
        let sn = &meta.snaps[meta.exits[out.exitno as usize].snap as usize];
        if !sn.sunk.is_empty() {
            // SAFETY: contract of this function; the snapshot slots are written, nothing allocated since.
            let ok = unsafe { materialise(&mut st, &ctx, sn, area) };
            if !ok && !mutants::active("sink_mem_error_dropped") {
                // TR29.3: the frame is restored (landing pc P); raise the memory error there through the shared exit
                // routine (it returns `JitExit::Error` when the handler returns non-zero)
                jit_hooks::set_error(&mut st, rn_core::status::Status::ErrMem);
                cache::handler_leave(&mut st);
                area.args[0] = 0;
                return Handled {
                    id,
                    out,
                    flushed: false,
                    side_started: false,
                    mem_error: true,
                };
            }
        }
    }
    if out.site == ExitSite::Other
        && cache::exit_is_black(&mut st, id, out.exitno as usize)
        && !out.entry
        && !out.pre
        && {
            let sn = &meta.snaps[meta.exits[out.exitno as usize].snap as usize];
            !sn.frames.is_empty() && sn.sunk.is_empty()
        }
    {
        // SAFETY: the per-state data is live.
        unsafe { (*crate::rt::data_ptr(&mut st)).tr.stats.black_handled += 1 };
    }
    let mut flushed = false;
    let mut side_started = false;
    let mut interp_rem = 0u32;
    if out.site == ExitSite::Alloc && !out.entry {
        // P-B: the exit-storm rule (allocation exits that come about once per iteration give the loop back to tier 1)
        // SAFETY: `budget` is the live budget word (contract).
        flushed = cache::alloc_storm(&mut st, id, unsafe { budget.read() });
        if !flushed {
            // N3b: the interpreter allocates (and steps the collector) now; the loop's next back-edge should enter compiled code again
            rn_core::jit_hooks::arm_tick_next(&mut st);
        }
    }
    if out.count > 0 && !flushed {
        // SAFETY: the per-state data is live.
        let hot = unsafe { (*crate::rt::data_ptr(&mut st)).tr.cfg.hot_exit };
        // a side trace is tried at exactly `hot` (the inline counter diverts once per arming); the prototype's flush
        // rule of type-guard exits keeps reporting from the threshold on
        if is_hot(out.site, out.count, hot)
            || out.count == hot
            || (out.count > hot && out.count % hot == 0 && out.count / hot <= 1024)
        {
            // SAFETY: `ci` is the running Lua frame of the exiting trace (contract).
            let ci = unsafe { Ci::from_raw(ci) };
            match cache::exit_hot_policy(&mut st, ci, id, &out, meta) {
                cache::HotAction::None => {}
                cache::HotAction::Flushed => flushed = true,
                cache::HotAction::Interp(rem) => {
                    side_started = true;
                    interp_rem = rem;
                }
            }
        }
    }
    if !flushed
        && !side_started
        && out.pre
        && !out.entry
        && out.count > 0
        && out.count % INEFF_EXITS == 0
    {
        // Q2: a pre-roll site took another INEFF_EXITS exits: do most entries of the head leave before one iteration?
        // SAFETY: `ci` is the running Lua frame of the exiting trace (contract).
        flushed = cache::pre_exit_check(&mut st, unsafe { Ci::from_raw(ci) }, id, meta);
    }
    if !flushed
        && !side_started
        && !out.entry
        && out.count > 0
        && out.count % CHAIN_EXITS == 0
        && out.site == ExitSite::Other
    {
        // P3: a callee-guard exit with a side trace attached took another CHAIN_EXITS exits: is its chain in a bad order? (flushes the child subtree of the exit only: the
        // continuation cell is re-read below, like after any policy flush)
        flushed = cache::chain_check(&mut st, id, meta.exits[out.exitno as usize].snap as usize);
    }
    if out.entry && !side_started {
        // an entry check failed through the generic stub: the threshold test of the inline path (3.3). The flush is
        // deferred (`handler_enter`); nothing of the trace's metadata is used afterwards.
        // SAFETY: `ci` is the running Lua frame of the exiting trace (contract).
        cache::entry_fail_hot_id(&mut st, unsafe { Ci::from_raw(ci) }, id);
    }
    if let Some(p) = PROBE.with(|p| *p.borrow()) {
        p(&mut st, id);
    }
    if side_started {
        // the interpreter takes over at `savedpc`: tier 1 would give the prepaid rest of the block back before it
        // leaves (after the probe: observers see the budget word of the tier-1 label, as for every other exit)
        // SAFETY: `budget` is the live budget word (contract).
        unsafe {
            let b = budget.read();
            let rem = if mutants::active("side_interp_no_rem") {
                0
            } else {
                interp_rem
            };
            budget.write(b.wrapping_add(rem as i32));
        }
        area.args[0] = 0;
    }
    if !side_started
        && meta.snaps[meta.exits[out.exitno as usize].snap as usize].link_cell != 0
        && !mutants::active("frame_link_cell_ignored")
    {
        // F2b: the policy above may have flushed the target of the link: continue where its cell says *now*
        // SAFETY: as in `exit_core`.
        area.args[0] = unsafe {
            &*(meta.snaps[meta.exits[out.exitno as usize].snap as usize].link_cell
                as *const Cell<usize>)
        }
        .get() as u64;
    } else if !side_started
        && !meta.cells.is_null()
        && meta.frame_cells
        && meta.snaps[meta.exits[out.exitno as usize].snap as usize].frame_side_ok()
    {
        // S1: a frameful exit with a side trace attached continues in it with the frames virtual again: the restore above
        // rebuilt them (the handler is the slow path; the inline stub never builds them), so unwind to the root frame. A flush
        // of the family by the policy above put the cell back to 0: land at the tier-1 label with the frames as restored.
        let ex = meta.exits[out.exitno as usize];
        // SAFETY: as for the frameless cells below.
        let c = unsafe { &*meta.cells.add(ex.snap as usize) }.get() as u64;
        if c != 0 && c != meta.snaps[ex.snap as usize].land_addr {
            // SAFETY: `ci` is the live root frame; `th_ci` / `th_top` point into the live thread (contract of `restore`).
            unsafe {
                if !mutants::active("frame_cell_unwind_skip") {
                    ctx.th_ci.write(ctx.root);
                    ctx.th_top.write((*ctx.root).top.0);
                }
                if !ctx.ncalls.is_null() {
                    ctx.ncalls.write(ncalls0);
                }
            }
            area.args[0] = c;
        }
    } else if !side_started && out.diverted_possible {
        // F2a (TR18): the policy above may have flushed this trace and its side traces (their exit cells now hold the
        // tier-1 addresses): continue where the cell says *now*
        let ex = meta.exits[out.exitno as usize];
        // SAFETY: `cells` points at live cells of the trace (kept alive by the `Arc`d metadata and the deferred
        // retirement of TR18) and `ex.snap` indexed them.
        area.args[0] = unsafe { &*meta.cells.add(ex.snap as usize) }.get() as u64;
    }
    cache::handler_leave(&mut st);
    Handled {
        id,
        out,
        flushed,
        side_started,
        mem_error: false,
    }
}

/// P-L: the handler of a poll exit ([`EXIT_POLL`]): the trace found `state::TRACE_POLL` set at its loop head (or link).
/// Restores the snapshot (a head: it lands at a pre-count label, so the budget word needs no further adjustment),
/// creates its sunk tables, does not count the exit, sets `ci.trap` and leaves compiled code for the interpreter at the
/// snapshot's pc (landing address 0, like a started side recording). The poll word is cleared last, **before** the
/// interpreter examines `INTERRUPT` and the hooks (and after the `+emuverify` probe, whose emulation reads it).
///
/// # Safety
/// As [`exit_entry`]; `handler_enter` has run and `meta` is the metadata of trace `id`.
unsafe fn poll_exit(
    st: &mut State,
    meta: &ExitMeta,
    ctx: &RestoreCtx,
    area: &mut ExitArea,
    id: TrId,
    ci: *mut CallInfo,
) -> Handled {
    let exitno = area.exitno & !EXIT_FLAGS;
    let ex = meta.exits[exitno as usize];
    let snap = &meta.snaps[ex.snap as usize];
    // SAFETY: contract of this function (the contract of `restore`).
    let _ = unsafe { restore(ctx, snap, snap.code_or(meta.root_code), meta.vb, area) };
    let out = CoreOut {
        exitno,
        entry: false,
        site: ex.site,
        pre: ex.pre,
        count: 0,
        land: 0,
        diverted: false,
        diverted_possible: false,
    };
    if !snap.sunk.is_empty() {
        // SAFETY: as for any exit: the snapshot slots are written, nothing allocated since.
        let ok = unsafe { materialise(st, ctx, snap, area) };
        if !ok {
            jit_hooks::set_error(st, rn_core::status::Status::ErrMem);
            cache::handler_leave(st);
            area.args[0] = 0;
            return Handled {
                id,
                out,
                flushed: false,
                side_started: false,
                mem_error: true,
            };
        }
    }
    // SAFETY: `ci` is the running Lua frame of the exiting trace (contract).
    if !mutants::active("poll_exit_no_trap") {
        unsafe { Ci::from_raw(ci) }.w().trap = 1;
    }
    area.args[0] = 0;
    if let Some(p) = PROBE.with(|p| *p.borrow()) {
        p(st, id);
    }
    if !mutants::active("poll_exit_flag_kept") {
        rn_core::state::TRACE_POLL.store(0, std::sync::atomic::Ordering::SeqCst);
    }
    cache::handler_leave(st);
    Handled {
        id,
        out,
        flushed: false,
        side_started: false,
        mem_error: false,
    }
}

/// J1-2: `jit_tr_mat(thread, ci, snap, trace_slot) -> 0 | 1`: the `CallH` target of the inline materialising exit stub. The stub has stored the
/// snapshot's slots, the budget and the registers the sunk tables read (`sunk_save_ops`); this creates the sunk tables (no counting, no
/// policy: the stub counted the exit and left the hot ones to the handler). `1`: a host allocation failed, the memory error is recorded
/// and the frame is in the state of the exit's pc (the stub returns `JitExit::Error`).
///
/// # Safety
/// Called only from such a stub with the running thread and its root Lua frame; `snap` is a frameless snapshot of the live trace `trace_slot`.
pub unsafe extern "C" fn jit_tr_mat(
    th: *mut Thread,
    ci: *mut CallInfo,
    snap: u32,
    trace_slot: u32,
) -> u32 {
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        // SAFETY: `th` is the live thread pinned in `r12`.
        let mut st = unsafe { jit_hooks::state_from_thread(th.cast()) };
        let budget = crate::rt::cnt_ptr(&mut st);
        // SAFETY: the area follows the budget word in the per-state data; nothing else accesses it.
        let area = unsafe {
            &*budget
                .cast::<u8>()
                .add(exit_area::FROM_R15 as usize)
                .cast::<ExitArea>()
        };
        let (_, meta) = cache::exit_meta_ptr(&mut st, trace_slot)
            .expect("inline sunk stub of a trace without metadata");
        // SAFETY: the trace is running (its stub called us), so it is live; flushes of it are deferred while it runs.
        let meta: &ExitMeta = unsafe { &*meta };
        let s = &meta.snaps[snap as usize];
        let th8 = th.cast::<u8>();
        let ctx = RestoreCtx {
            stack0: st.slot_ptr(StkId(0)).cast(),
            stack_size: st.stack_size(),
            hooked: st.hook_mask() != 0,
            root: ci,
            th_ci: th8.wrapping_add(thr::CI).cast(),
            th_top: th8.wrapping_add(thr::TOP).cast(),
            ncalls: th8.wrapping_add(rn_core::jit_hooks::thread::NCALLS).cast(),
            budget,
        };
        super::sink::bump(&super::sink::MAT_STUB_CALLS, 1);
        super::sink::bump(&super::sink::MAT_STUB_TABLES, s.sunk.len() as u64);
        if !s.frames.is_empty() {
            super::sink::bump(&super::sink::MAT_STUB_FRAMES, 1);
        }
        // SAFETY: the slots were written by the stub, nothing was allocated since (the contract of `materialise`).
        let ok = unsafe { materialise(&mut st, &ctx, s, area) };
        cache::handler_enter(&mut st);
        if !ok {
            // SAFETY: the contract of `restore` (frameless snapshot, live frame).
            unsafe { restore_chain(&ctx, s, s.code_or(meta.root_code)) };
            jit_hooks::set_error(&mut st, rn_core::status::Status::ErrMem);
        }
        cache::handler_leave(&mut st);
        !ok
    }));
    match r {
        Ok(err) => u32::from(err),
        Err(e) => {
            let msg = e
                .downcast_ref::<String>()
                .map(String::as_str)
                .or_else(|| e.downcast_ref::<&str>().copied())
                .unwrap_or("<non-string payload>");
            eprintln!("fatal: panic in jit_tr_mat: {msg}");
            std::process::abort();
        }
    }
}

/// `jit_tr_exit(thread, ci, 0, 0) -> 0`: the `CallH` target of the shared exit routine (5.6). The exit area is
/// [`exit_area::FROM_R15`] bytes behind the budget word.
///
// ---- BUNDLE-K L1 (`lazyci`): leaving a lazy region -------------------------------------------------------------------------

/// `flags` bit of [`lazy_leave`] / [`jit_tr_lazy_leave`]: the innermost frame was just pushed natively by a self-call (its
/// `savedpc` was not stored: it is the first instruction of its prototype and `thread.top` the end of its arguments).
pub const LAZY_NEW_FRAME: u32 = 1;

/// Completes the lazily pushed frames of the chain that ends at `inner` (exclusive of `root`, which and whose callers are complete) and
/// leaves the thread as an eager push/return sequence would have: `thread.ci = inner`, `thread.top`, `area.lazy_root = 0`.
///
/// A lazy frame has only `func` (stored at the push) and the caller's `savedpc` (the continuation, stored at the push). Everything else is a function
/// of those: `top = func + 1 + maxstacksize`, `kcache = proto.k`, `trap = 0`, `callstatus = C operand of the caller's CALL`
/// (`wanted results + 1`, which is what the push stores); the `savedpc` of an inner frame is its caller's continuation.
///
/// # Safety
/// `th` is the live running thread, `inner` its innermost frame of the region (`thread.ci` may be stale), `area` its exit area; a nonzero
/// `area.lazy_root` is a frame of the chain below `inner` (or `inner` itself).
pub unsafe fn lazy_leave(th: *mut Thread, inner: *mut CallInfo, area: &mut ExitArea, flags: u32) {
    let root = area.lazy_root as *mut CallInfo;
    // every frame is complete from here on: the running frame is the new root (a side trace the exit continues in is lazy-aware and runs
    // with a root; a root that is 0 would mean `no region` to the fill)
    area.lazy_root = if mutants::active("lazy_leave_root_zero") {
        0
    } else {
        inner as u64
    };
    // SAFETY: `th` is the live running thread (contract).
    let st = unsafe { jit_hooks::state_from_thread(th.cast()) };
    let th8 = th.cast::<u8>();
    let mut c = inner;
    if !root.is_null() && !mutants::active("lazy_fill_skip") {
        // Every lazy frame is a native self-call of one prototype (the callee identity guard of the push): `maxstacksize` and the constants are
        // those of the innermost frame's closure; only `callstatus` depends on the call site (the caller's CALL).
        // SAFETY: the chain from the innermost frame down to the root consists of live nodes of this thread (contract).
        unsafe {
            let cl = st
                .get((*inner).func)
                .as_lclosure()
                .expect("a lazy frame runs a Lua closure");
            let p = cl.proto();
            let (maxstack, kptr, code) = (usize::from(p.maxstacksize), p.k_ptr(), p.code_ptr());
            let top_extra = if mutants::active("lazy_fill_top_off") {
                0
            } else {
                1
            };
            while !c.is_null() && c != root {
                let prev = (*c).previous;
                debug_assert!(
                    st.get((*c).func)
                        .as_lclosure()
                        .is_some_and(|k| std::ptr::eq(k.proto().code_ptr(), code)),
                    "lazy frames are of one prototype"
                );
                (*c).top = StkId((*c).func.0 + top_extra + maxstack);
                (*c).kcache = kptr;
                if !mutants::active("lazy_fill_trap_skip") {
                    (*c).trap = 0;
                }
                let call = rn_core::instruction::Instruction(*(*prev).savedpc.sub(1));
                let cs = if mutants::active("lazy_fill_cs_off") {
                    call.c() + 1
                } else {
                    call.c()
                };
                (*c).callstatus = cs;
                if c == inner
                    && flags & LAZY_NEW_FRAME != 0
                    && !mutants::active("lazy_fill_newframe_skip")
                {
                    (*c).savedpc = code;
                }
                c = prev;
            }
        }
    }
    // SAFETY: the thread's fields are written at their frozen offsets (the same stores `restore` makes).
    unsafe {
        th8.add(thr::CI).cast::<*mut CallInfo>().write(inner);
        let top = if flags & LAZY_NEW_FRAME != 0 {
            let func = (*inner).func;
            let cl = st.get(func).as_lclosure().expect("a Lua closure");
            func.0 + 1 + usize::from(cl.proto().numparams)
        } else {
            (*inner).top.0
        };
        if !mutants::active("lazy_fill_th_top_skip") {
            th8.add(thr::TOP).cast::<StkId>().write(StkId(top));
        }
    }
}

/// `jit_tr_lazy_leave(thread, ci, flags, 0)`: the out-of-line twin of the leave sequence of generated code (a `CallH` of a stub whose
/// registers are dead); `ci` is the innermost frame (`r13`).
///
/// # Safety
/// As [`lazy_leave`]; called from generated code with the running thread.
pub unsafe extern "C" fn jit_tr_lazy_leave(
    th: *mut Thread,
    ci: *mut CallInfo,
    flags: u32,
    _b: u32,
) -> u32 {
    // SAFETY: `th` is the live thread pinned in `r12`.
    let mut st = unsafe { jit_hooks::state_from_thread(th.cast()) };
    let budget = crate::rt::cnt_ptr(&mut st);
    // SAFETY: the area follows the budget word in the per-state data (contract of `jit_tr_exit`).
    let area = unsafe {
        &mut *budget
            .cast::<u8>()
            .add(exit_area::FROM_R15 as usize)
            .cast::<ExitArea>()
    };
    // SAFETY: contract of this function.
    unsafe { lazy_leave(th, ci, area, flags) };
    0
}

/// # Safety
/// Called only from the shared exit routine with the running thread and its root Lua frame, after the routine saved the
/// registers (`JitData` must place the area behind the budget word: P4 decisions in docs/JIT_TRACE_REF.md).
pub unsafe extern "C" fn jit_tr_exit(th: *mut Thread, ci: *mut CallInfo, _a: u32, _b: u32) -> u32 {
    // A panic cannot unwind through the generated code: fail stop WITH the message (the default abort of an
    // `extern "C"` boundary prints only "panic in a function that cannot unwind").
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        // SAFETY: `th` is the live thread pinned in `r12`.
        let mut st = unsafe { jit_hooks::state_from_thread(th.cast()) };
        let budget = crate::rt::cnt_ptr(&mut st);
        // SAFETY: the contract of the function; the area follows the budget word in the per-state data.
        unsafe { exit_entry(th, ci, budget) }.mem_error
    }));
    match r {
        Ok(mem_error) => u32::from(mem_error),
        Err(e) => {
            let msg = e
                .downcast_ref::<String>()
                .map(String::as_str)
                .or_else(|| e.downcast_ref::<&str>().copied())
                .unwrap_or("<non-string payload>");
            eprintln!("fatal: panic in jit_tr_exit: {msg}");
            std::process::abort();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tags_cover_every_lua_type() {
        for t in [
            Ty::Nil,
            Ty::False,
            Ty::True,
            Ty::Int,
            Ty::Num,
            Ty::ShrStr,
            Ty::LngStr,
            Ty::Tab,
            Ty::Lfn,
            Ty::Lcf,
            Ty::Ccl,
            Ty::Udata,
            Ty::LUdata,
            Ty::Thread,
        ] {
            assert!(tag_of(t).is_some(), "{t:?}");
        }
        assert!(
            tag_of(Ty::I64).is_none() && tag_of(Ty::P64).is_none() && tag_of(Ty::U32).is_none()
        );
        assert_eq!(tag_of(Ty::Tab), Some(ctb(LUA_VTABLE)));
    }

    #[test]
    fn frame_windows_are_consecutive() {
        let mut ir = TraceIr::default();
        for n in [0u8, 2, 0, 1] {
            ir.snapshot(
                Snap {
                    nframes: n,
                    frame: n,
                    ..Snap::default()
                },
                &[],
            );
        }
        assert_eq!(frame_window(&ir, 0), 0..0);
        assert_eq!(frame_window(&ir, 1), 0..2);
        assert_eq!(frame_window(&ir, 2), 2..2);
        assert_eq!(frame_window(&ir, 3), 2..3);
    }
}
