//! J5 P5a: register allocation of a trace (docs/JIT_TRACE.md section 7, decisions in docs/JIT_TRACE_REF.md "P5a
//! decisions"). **Pure**: `TraceIr` in, an [`Allocation`] out, no micro-ops. The assembler (P5b) walks the IR forwards
//! and, for each instruction, emits what [`InsAlloc`] says; the exit handler (P4) restores snapshots from
//! [`Allocation::snap_locs`].
//!
//! # Scheme (LuaJIT's reverse linear scan, 7.2)
//!
//! The IR is walked from the last instruction to the first. A value gets a register at its last use (the first time it
//! is seen) and gives it back at its definition. When no register is free the value with the lowest defining ref
//! (longest remaining range; PHIs and weak values adjusted) is *evicted*: it gets a spill slot (written right after
//! its definition, [`InsAlloc::stores`]) and a reload into the same register right after the current instruction
//! ([`InsAlloc::post`]), so that the register holds the evicted value again for its later uses. A value may therefore
//! live in several registers over its life (one *segment* per eviction); the location of an operand is recorded per
//! use ([`InsAlloc::ops`]). Constants are never allocated (they are [`Loc::Const`], rematerialised by the assembler
//! through `V7`); `Aref` / `Uref` whose users are all memory ops are *fused* into the access (their operands become
//! the operands of the user, 5.3).
//!
//! * **Snapshots (TR27).** The restore location of every snapshot ref ([`Allocation::snap_locs`], one canonical map
//!   per snapshot) is fixed when the *last* guard of the snapshot is allocated. Refs held in a register are pinned
//!   (not evictable) until the *first* guard of the snapshot has been allocated, so all guards of a snapshot see the
//!   same locations. When pressure leaves only pinned victims the ref is forced to a fresh spill slot instead (a
//!   TR27 rename: [`Allocation::forced_spills`]). Snapshot-only refs are weak: a register only if one is free, else a
//!   spill slot (never a reload).
//! * **Loops (7.3, 7.8, TR27b).** Loop-invariant pre-roll values used by the body (*homes*) keep one location for the
//!   whole body (register, or a permanent spill slot read by a reload before each use), so that nothing moves at the
//!   back-edge for them. A PHI is an ordinary value that is defined at the top of the body in [`InsAlloc::dst`] of its
//!   `Phi` instruction; two parallel moves feed it: [`Allocation::entry_moves`] (pre-roll value -> PHI location at the
//!   `LOOP` marker) and [`Allocation::backedge_moves`] (body value -> PHI location at the end of the body, after the
//!   last instruction: no guard follows, [`Allocation::backedge_after`]). Both are sequentialised ([`sequentialise`]).
//! * **Register constraints.** To keep the allocator free of per-instruction fix-ups, a register that some op of the
//!   trace clobbers is simply not allocatable in that trace: `V8` when the trace has `Idiv` / `Mod`, `V6` when it has
//!   a variable shift, `X0 / X1` when it has `Fneg / Fabs / F2iExact` ([`op_clobbers`]). `V7` is never allocated.
//!   Two-address contract for the assembler: the destination register of an op with two register operands may equal
//!   the first operand's register, never the second's (unless both operands are the same value).
//! * **Helper calls (7.5).** `Calln`, `Tbar` and the F3 class-A ops (`Tnew`, `Newref`, `Tlen`) are call sites: [`Allocation::calls`] lists the registers that hold
//!   values live across the call (the set to store before and reload after `CallH`).
//! * **Spill slots (7.4).** Slots are shared by values whose lifetimes do not overlap; at most
//!   [`SCRATCH_SPILL`] (255) slots, the last one ([`SCRATCH_SPILL`]) is reserved for breaking move cycles.
//!   More than that is [`RaError::Spill`] (`Compile(spill)`); more than 64 PHIs is [`RaError::Phi`].
//! * **`+randra=S`.** [`RaOpts::randra`] makes every free choice random (register, hint use, refusal of a free
//!   register, victim, weak ref in register or slot, forced renames, home choice, slot reuse). Any outcome is a valid
//!   allocation and must pass the same tests.

use super::interp::gen::Rng;
use super::ir::*;
use super::mutants;
use super::{TraceOpts, NSPILL};

/// Number of virtual general registers `V0..V10` (`V9` = `rbp`, `V10` = `r14`: callee-saved, allocatable only with `RaOpts::xregs`).
pub const NGPR: usize = 11;
/// The general registers `CallH` destroys: `V0..V8`; `V9` / `V10` (`rbp`, `r14`) are callee-saved and survive a helper call.
pub const NGPR_CLOBBERED: usize = 9;
/// Number of virtual double registers `X0..X15`.
pub const NXMM: usize = 16;
/// The trace scratch register `V7`: never allocated; breaks general-register move cycles and rematerialises
/// constants.
pub const SCRATCH_GPR: u8 = 7;
/// The spill slot reserved for breaking move cycles that involve an XMM register or memory (never allocated).
pub const SCRATCH_SPILL: u16 = (NSPILL - 1) as u16;
/// Most PHIs of a trace (`Compile(phi)` beyond).
pub const MAX_PHI: usize = 64;

/// The register `asm` keeps the budget `vb` in when `RaOpts::reserve_gpr == Some(BUDGET_GPR)`: `V5`. Not `V8`
/// (`rdx`: `SDivRem` and `ReloadFrame` destroy it), not `V6` (variable shifts), not `V0..V4` (scratch of the entry
/// checks, the entry-failure bookkeeping and the exit stubs), and never a temporary of an expansion
/// (`SDivRem` skips it). `asm::assemble` accepts exactly this register.
pub const BUDGET_GPR: u8 = 5;

/// Section 51 (`rapol` bit 0): the eviction victim is the value whose previous use (going up from the current instruction) is furthest away,
/// Belady's choice for a backward scan, instead of the value with the lowest defining ref.
pub const RAPOL_VICTIM: u32 = 1;
/// Section 51 (`rapol` bit 1): a value live across a helper call is spilled (stored once at its definition, reloaded after each call) instead of
/// being saved to the exit area before and reloaded after every call it crosses. Loop homes and values pinned by an open snapshot keep the save.
pub const RAPOL_CALLSPILL: u32 = 2;
/// Every lever of `rapol`.
pub const RAPOL_ALL: u32 = RAPOL_VICTIM | RAPOL_CALLSPILL;

const GPR_POOL: [u8; 10] = [0, 1, 2, 3, 4, 5, 6, 8, 9, 10];

const NOREG: u8 = 255;
const NONE: u32 = u32::MAX;

/// Where a value is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum Loc {
    /// No location (the instruction defines no value / the operand is absent).
    #[default]
    None,
    /// The constant with this ref (`k[r - 1]`): rematerialised by the user.
    Const(Ref),
    /// General register `V<n>` (`n != 7`).
    G(u8),
    /// Double register `X<n>`.
    X(u8),
    /// Spill slot `n` (`ExitArea.spill[n]`).
    Spill(u16),
}

impl Loc {
    /// Is this a register?
    pub fn is_reg(self) -> bool {
        matches!(self, Loc::G(_) | Loc::X(_))
    }
}

/// One simple move (`dst <- src`). Legal pairs: register <- register / slot / constant, slot <- register / slot /
/// constant (memory to memory and constant to anything but a general register go through `V7`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Move {
    /// Destination (register or slot).
    pub dst: Loc,
    /// Source (register, slot or constant).
    pub src: Loc,
}

/// A reload `reg <- spill[slot]` of value `r`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Reload {
    /// The register (`Loc::G` / `Loc::X`).
    pub reg: Loc,
    /// The spill slot.
    pub slot: u16,
    /// The value being reloaded.
    pub r: Ref,
}

/// What the assembler does around one IR instruction, in this order: `pre` reloads, the operation reading its
/// operands from `ops` and writing `dst`, `stores` (`spill[s] <- dst`), `post` reloads.
#[derive(Clone, Debug, Default)]
pub struct InsAlloc {
    /// Register the result is computed into. `Loc::None`: the instruction defines no value (guards without result,
    /// stores, fused `Aref` / `Uref`, `Loop`). For a `Phi` this is the PHI's location at the top of the body: a
    /// register, or `Spill(s)` when the PHI lives in memory (`stores` is then empty).
    pub dst: Loc,
    /// Locations the operands are read from, as listed by [`operand_refs`] (register, [`Loc::Const`] or
    /// [`Loc::None`] for an absent operand).
    pub ops: [Loc; 3],
    /// Spill slots to write from `dst` right after the instruction (the value is reloaded or restored from there).
    pub stores: Vec<u16>,
    /// Reloads before the instruction (operands of loop-invariant values that live in memory).
    pub pre: Vec<Reload>,
    /// Reloads after the instruction (values evicted across it; the register may be the one `dst` just used).
    pub post: Vec<Reload>,
}

/// A set of registers (bit `n` = `V<n>` / `X<n>`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CallClobber {
    /// General registers.
    pub gpr: u16,
    /// Double registers.
    pub xmm: u16,
}

/// What `CallH` destroys: every `V` and every `X` (7.5). `V7` is included (never holds a value).
pub const CALL_CLOBBER: CallClobber = CallClobber {
    gpr: 0x1FF,
    xmm: 0xFFFF,
};

/// A helper call (`Calln`, `Tbar`): the assembler stores the registers of `live` to the exit area before the call
/// and reloads them after it (7.5). `live` holds the values that are live after the instruction (loop-invariant
/// homes included), not the operands that die at it and not the result.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CallSite {
    /// The instruction index.
    pub at: usize,
    /// Registers to save and restore around the call.
    pub live: CallClobber,
}

/// Why allocation failed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RaError {
    /// More than [`MAX_PHI`] PHIs (`Compile(phi)`).
    Phi,
    /// More spill slots than [`SCRATCH_SPILL`] (`Compile(spill)`).
    Spill,
    /// The trace is not valid input (message).
    BadIr(String),
    /// Internal: no register could be freed (should not happen with the home caps).
    Pressure,
}

/// Allocation options.
#[derive(Clone, Copy, Debug)]
pub struct RaOpts {
    /// `+randra=S`: seeded random choices.
    pub randra: Option<u64>,
    /// Most general registers held for loop-invariant values for a whole body.
    pub max_gpr_homes: usize,
    /// Most double registers held for loop-invariant values for a whole body.
    pub max_xmm_homes: usize,
    /// A general register the allocator never assigns in this trace (PIa: the budget register `vb`, see
    /// [`BUDGET_GPR`]). Must be one of the allocatable registers `V0..V6`, `V8`.
    pub reserve_gpr: Option<u8>,
    /// P2 bundle 1 (`xregs`): bit 0 makes `V9` (`rbp`) allocatable, bit 1 `V10` (`r14`; never in a trace with a native frame push / pop,
    /// whose assembler reads `r14`). 0: neither.
    pub xregs: u8,
    /// H1 (`hkaddr`): an `HREFK` whose users are all `HLOAD` / `HSTORE` addresses is *fused* into them like an `AREF`: no register and no `lea`, the
    /// users address `[node + idx * 24 + field]` and the node operand stays live up to them.
    pub hkaddr: bool,
    /// N1a (`ixfuse`): an integer `ADD x, #c` whose users are all the index operand of fused `AREF`s is *fused* into them (no register, no instruction): the
    /// accesses address `[base + x + c + ...]`, `x` stays live up to them.
    pub ixfuse: bool,
    /// R1 (`regcarry`): the index of the first instruction behind the leading `SLOAD` run of a root (`exit::lead_run`): the allocator records where the
    /// run's values are before that instruction ([`Allocation::carry`]). 0: nothing is recorded.
    pub carry_at: usize,
    /// Section 51 (`rapol`): the policy levers (`RAPOL_*` bits).
    pub pol: u32,
}

impl Default for RaOpts {
    fn default() -> RaOpts {
        RaOpts {
            randra: None,
            max_gpr_homes: 3,
            max_xmm_homes: 10,
            reserve_gpr: None,
            xregs: 0,
            hkaddr: false,
            ixfuse: false,
            carry_at: 0,
            pol: super::cache::RAPOL_PROCESS,
        }
    }
}

impl RaOpts {
    /// The options implied by `RUSTYNAIL_JIT_TRACE_OPT`.
    pub fn from_trace(o: &TraceOpts) -> RaOpts {
        RaOpts {
            randra: o.randra,
            ..RaOpts::default()
        }
    }
}

/// The result of allocation.
#[derive(Clone, Debug, Default)]
pub struct Allocation {
    /// Per IR instruction.
    pub ins: Vec<InsAlloc>,
    /// Per IR instruction: `Aref` / `Uref` folded into their memory users (no register, no code).
    pub fused: Vec<bool>,
    /// Per IR instruction: the value is referenced only by snapshots (weak).
    pub weak: Vec<bool>,
    /// Per snapshot, aligned with `TraceIr::snap_entries`: where the exit finds each slot's value. All guards of the
    /// snapshot use this map (TR27). `Loc::None` entries for `NOREF` refs; all `None` for a snapshot no guard uses.
    pub snap_locs: Vec<Vec<Loc>>,
    /// Refs that had to be forced to a spill slot to keep the exit maps of a snapshot identical (snapshot, ref).
    pub forced_spills: Vec<(u32, Ref)>,
    /// Loop entry: PHI locations <- pre-roll values, executed at the `LOOP` marker (simultaneous).
    pub entry_moves: Vec<Move>,
    /// [`Allocation::entry_moves`] as a legal sequence of single moves.
    pub entry_seq: Vec<Move>,
    /// Back-edge: PHI locations <- body values (simultaneous).
    pub backedge_moves: Vec<Move>,
    /// [`Allocation::backedge_moves`] as a legal sequence of single moves.
    pub backedge_seq: Vec<Move>,
    /// The back-edge moves execute just before instruction `backedge_after` (`ins.len()`: after the last instruction,
    /// TR27b: no guard follows them).
    pub backedge_after: usize,
    /// Location, at the end of the body (before the back-edge moves), of each ref of `end_uses` given to
    /// [`allocate`] (same order).
    pub end_locs: Vec<Loc>,
    /// R1: with [`RaOpts::carry_at`] set: per live leading `SLOAD` (ref, register holding it before instruction `carry_at`, spill slots that hold it).
    pub carry: Vec<(Ref, Loc, Vec<u16>)>,
    /// Helper-call sites in instruction order.
    pub calls: Vec<CallSite>,
    /// Spill slots used (`0..nspill`), not counting [`SCRATCH_SPILL`].
    pub nspill: u16,
    /// Section 49 (C1): the registers [`Allocation::uses_gpr`] / [`Allocation::uses_xmm`] answer for, computed by one scan at the first query
    /// (bit `r` of `[0]`: `V<r>`, of `[1]`: `X<r>`). The assembler asks for single registers many times per trace (a constant float compare
    /// scanned the whole allocation up to 16 times); the allocation is not changed once it is built (a test that builds one by hand leaves this
    /// empty and gets the same answers).
    pub used: std::cell::OnceCell<[u32; 2]>,
}

impl Allocation {
    /// Loop-carried values that go through memory on every iteration: PHIs that live in a spill slot and back-edge moves that read or write one (P2 bundle 1:
    /// a store followed by a reload on the loop-carried chain is a dependent memory round trip, the one cost a spare register removes for real; it decides
    /// whether `V10` pays for its `ReloadK` at the exits).
    pub fn carried_spills(&self, ir: &TraceIr) -> usize {
        let Some(l) = ir.loop_at else { return 0 };
        let l = usize::from(l - REF_BIAS);
        let phis = (l + 1..self.ins.len())
            .filter(|&i| ir.ins[i].op == TOp::Phi && matches!(self.ins[i].dst, Loc::Spill(_)))
            .count();
        phis + self
            .backedge_seq
            .iter()
            .filter(|m| matches!(m.src, Loc::Spill(_)) || matches!(m.dst, Loc::Spill(_)))
            .count()
    }

    /// The register masks of [`Allocation::used`] (one scan of everything the old per-register scans looked at).
    fn used_masks(&self) -> [u32; 2] {
        *self.used.get_or_init(|| {
            let mut m = [0u32; 2];
            let mut add = |l: &Loc| match *l {
                Loc::G(r) => m[0] |= 1u32 << (r & 31),
                Loc::X(r) => m[1] |= 1u32 << (r & 31),
                _ => {}
            };
            for a in &self.ins {
                add(&a.dst);
                a.ops.iter().for_each(&mut add);
                a.pre.iter().chain(&a.post).for_each(|x| add(&x.reg));
            }
            self.snap_locs.iter().flatten().for_each(&mut add);
            self.end_locs.iter().for_each(&mut add);
            for mv in self.entry_moves.iter().chain(&self.backedge_moves) {
                add(&mv.src);
                add(&mv.dst);
            }
            m
        })
    }

    /// Does any instruction destination, operand, reload or snapshot location use general register `V<r>`?
    pub fn uses_gpr(&self, r: u8) -> bool {
        if r < 32 && crate::cspeed::on(crate::cspeed::MASKS) {
            return self.used_masks()[0] & (1u32 << r) != 0;
        }
        let is = |l: &Loc| *l == Loc::G(r);
        self.ins.iter().any(|a| {
            is(&a.dst) || a.ops.iter().any(is) || a.pre.iter().chain(&a.post).any(|x| is(&x.reg))
        }) || self.snap_locs.iter().flatten().any(is)
            || self.end_locs.iter().any(is)
            || self
                .entry_moves
                .iter()
                .chain(&self.backedge_moves)
                .any(|m| is(&m.src) || is(&m.dst))
    }

    /// Does any instruction destination, operand, reload or snapshot location use double register `X<r>`?
    pub fn uses_xmm(&self, r: u8) -> bool {
        if r < 32 && crate::cspeed::on(crate::cspeed::MASKS) {
            return self.used_masks()[1] & (1u32 << r) != 0;
        }
        let is = |l: &Loc| *l == Loc::X(r);
        self.ins.iter().any(|a| {
            is(&a.dst) || a.ops.iter().any(is) || a.pre.iter().chain(&a.post).any(|x| is(&x.reg))
        }) || self.snap_locs.iter().flatten().any(is)
            || self.end_locs.iter().any(is)
            || self
                .entry_moves
                .iter()
                .chain(&self.backedge_moves)
                .any(|m| is(&m.src) || is(&m.dst))
    }

    /// The exit map of the guard at instruction `i`.
    pub fn exit_map(&self, ir: &TraceIr, i: usize) -> &[Loc] {
        &self.snap_locs[aux_snap(ir.ins[i].aux) as usize]
    }
}

/// Registers an op's expansion clobbers beyond its operands and result (the reason a register is excluded from the
/// whole trace, see the module documentation).
pub fn op_clobbers(i: &Ins, ir: &TraceIr) -> CallClobber {
    match i.op {
        TOp::Idiv | TOp::Mod => CallClobber {
            gpr: 1 << 8,
            xmm: 0,
        },
        // H1: the integer form of `HREFV` divides by the node-count modulus (`div` destroys `rdx` = V8) unless the modulus is 1
        TOp::Hrefv
            if aux_bits(i.aux) & (HREF_INT | HREF_OBJ) != 0 && (aux_bits(i.aux) & 0xFF) >= 2 =>
        {
            CallClobber {
                gpr: 1 << 8,
                xmm: 0,
            }
        }
        TOp::Shl | TOp::Shr if !TraceIr::is_const(i.b) || ir.as_int(i.b).is_none() => CallClobber {
            gpr: 1 << 6,
            xmm: 0,
        },
        TOp::Fneg | TOp::Fabs | TOp::F2iExact => CallClobber { gpr: 0, xmm: 3 },
        // N2b: V10 (r14) is reloaded from the caller's kcache by the pop and the trace would then hold a stale value. The guards and the pop use V1 and V2 as scratch (and V7): a value in them would be lost at the
        // exits the guards take (the exit stubs restore from the registers)
        TOp::Retf => CallClobber {
            gpr: 0b110 | 1 << 10,
            xmm: 0,
        },
        _ => CallClobber::default(),
    }
}

fn idx(r: Ref) -> usize {
    usize::from(r - REF_BIAS)
}

fn rf(i: usize) -> Ref {
    REF_BIAS + i as Ref
}

/// The pre-roll value a PHI starts from. P7 only produces pre-roll refs and constants; the generator of the property
/// tests may also name an earlier PHI of the same loop, whose first-iteration value is its own entry value.
pub fn entry_ref(ir: &TraceIr, mut p: usize) -> Ref {
    let l = ir.loop_at.map(|r| usize::from(r - REF_BIAS)).unwrap_or(0);
    loop {
        let a = ir.ins[p].a;
        if TraceIr::is_ins(a)
            && usize::from(a - REF_BIAS) > l
            && ir.ins[usize::from(a - REF_BIAS)].op == TOp::Phi
        {
            p = usize::from(a - REF_BIAS);
        } else {
            return a;
        }
    }
}

/// N1b: how many general registers the allocator can hand out in this trace: the pool `V0..V6, V8`, without the budget register (unless `membudget`) and
/// without what an instruction of the trace clobbers (`Idiv` takes `V8`, a variable shift `V6`).
pub fn free_gprs(ir: &TraceIr, membudget: bool) -> u32 {
    let mut m: u16 = 0x17F;
    if !membudget {
        m &= !(1u16 << BUDGET_GPR);
    }
    for i in &ir.ins {
        m &= !op_clobbers(i, ir).gpr;
    }
    m.count_ones()
}

/// Does the trace call a helper (an instruction that clobbers the caller-saved registers, see `is_call`)? A loop with such a call keeps every value it
/// holds in a callee-saved register or a spill slot across it: one more carried value is a store and a load per iteration (N1b `nidx` stays out).
pub fn has_call(ir: &TraceIr) -> bool {
    ir.ins.iter().any(|i| is_call(ir, i))
}

fn is_call(ir: &TraceIr, ins: &Ins) -> bool {
    // P-B: the length of a string is two loads in the assembler, not a helper call
    if ins.op == TOp::Tlen && matches!(ir.ty_of(ins.a), Ty::ShrStr | Ty::LngStr) {
        return false;
    }
    // A1: an inline `NEWREF` is a handful of stores
    if ins.op == TOp::Newref && ins.aux & super::ir::nrplan::INLINE != 0 {
        return false;
    }
    is_call_op(ins.op)
}

fn is_call_op(op: TOp) -> bool {
    matches!(
        op,
        TOp::Calln
            | TOp::Tbar
            | TOp::Tnew
            | TOp::Newref
            | TOp::Tlen
            | TOp::Tset
            | TOp::Tnext
            | TOp::Snew
            | TOp::Concat
            | TOp::Strto
            | TOp::Lib
    )
}

/// Section 51 (`RAPOL_CALLSPILL`): a call site whose helper call is on the main path of the assembled code, whatever the assembler's options:
/// the instructions with an inline fast path (`TLEN` / `TNEXT`, the `LIB` kinds `BYTE`, `STREQ`, `TYPE`, `TYPENAME`, `SETMT`, `CCALLS`,
/// `RANDOM`, `RANDF`; `TBAR`'s test of the colours) keep the save / reload of the call site, which their fast path does not execute.
fn always_calls(ins: &Ins) -> bool {
    match ins.op {
        TOp::Calln | TOp::Tnew | TOp::Newref | TOp::Tset | TOp::Snew | TOp::Concat | TOp::Strto => {
            true
        }
        TOp::Lib => !matches!(
            ins.aux & 0xFF,
            lib::BYTE
                | lib::STREQ
                | lib::TYPE
                | lib::TYPENAME
                | lib::SETMT
                | lib::CCALLS
                | lib::RANDOM
                | lib::RANDF
        ),
        _ => false,
    }
}

/// Constant-defining instructions: operand `a` is the constant, `b` is unused (`NOREF` from the recorder).
fn is_kconst(op: TOp) -> bool {
    matches!(op, TOp::Kint | TOp::Knum | TOp::Kpri | TOp::Kptr)
}

fn is_mem(op: TOp) -> bool {
    matches!(
        op,
        TOp::Aload | TOp::Astore | TOp::Uload | TOp::Ustore | TOp::Uvrel | TOp::Hload | TOp::Hstore
    )
}

/// The value operands of instruction `i`, after fusion: a fused `Aref` / `Uref` operand `a` of a memory op is
/// replaced by its own two operands. Layout: normal ops `[a, b, -]`; `Aload` / `Uload` with a fused address
/// `[base, index, -]`, `Astore` / `Ustore` `[base, index, value]`; non-fused `[address, -, -]` / `[address, value,
/// -]`. Raw operands (`Sload.a`, `Budget.a`, `Fload.b`) and `NOREF` are `NOREF`. `Phi` and `Loop` have none.
pub fn operand_refs(ir: &TraceIr, fused: &[bool], i: usize) -> [Ref; 3] {
    let ins = &ir.ins[i];
    let (ra, rb) = ins.op.ref_operands();
    let a = if ra { ins.a } else { NOREF };
    // Constant instructions (`KINT`..`KPTR`) read `a` only; the recorder emits `b = NOREF` (PIa).
    let b = if rb && !is_kconst(ins.op) {
        ins.b
    } else {
        NOREF
    };
    match ins.op {
        TOp::Phi | TOp::Loop => [NOREF; 3],
        // N1a: a fused index `ADD` is no instruction: its operand is read by the accesses that use it
        TOp::Add if fused[i] => [NOREF; 3],
        TOp::Tset => [a, b, ins.aux as Ref],
        TOp::Concat => [a, b, if ins.aux != 0 { ins.aux as Ref } else { NOREF }],
        TOp::Lib => [
            a,
            b,
            if ins.aux >> 8 != 0 {
                (ins.aux >> 8) as Ref
            } else {
                NOREF
            },
        ],
        op if is_mem(op) && TraceIr::is_ins(a) && fused[idx(a)] => {
            let ar = ir.at(a);
            // N1a: the index of a fused `ADD x, #c` is `x` (the accesses add `c` to their displacement)
            let ab = if ar.op == TOp::Aref && TraceIr::is_ins(ar.b) && fused[idx(ar.b)] {
                ir.at(ar.b).a
            } else {
                ar.b
            };
            if op == TOp::Aload {
                // BUNDLE-L A: the complemented index of the address, if it has one
                [ar.a, ab, ar.third().unwrap_or(NOREF)]
            } else if matches!(op, TOp::Uload | TOp::Uvrel | TOp::Hload) {
                [ar.a, ab, NOREF]
            } else {
                [ar.a, ab, b]
            }
        }
        _ => [a, b, NOREF],
    }
}

/// Does the instruction define a value (a register result)?
fn defines(op: TOp, fused: bool) -> bool {
    match op {
        TOp::Eq
        | TOp::Ne
        | TOp::Lt
        | TOp::Le
        | TOp::Gt
        | TOp::Ge
        | TOp::Ult
        | TOp::Uge
        | TOp::Fcmp
        | TOp::Eqref
        | TOp::Hmiss
        | TOp::Hshape
        | TOp::Poll
        | TOp::Budget
        | TOp::Astore
        | TOp::Ustore
        | TOp::Hstore
        | TOp::Tbar
        | TOp::Atouch
        | TOp::Tvirt
        | TOp::Sstore
        | TOp::Loop => false,
        TOp::Aref | TOp::Uref | TOp::Hrefk => !fused,
        // N1a: a fused index sum
        TOp::Add => !fused,
        _ => true,
    }
}

/// Largest `|c|` of a fused index `ADD x, #c` (the displacements `4 + c` and `8 * (c + 1)` fit 32 bits with room to spare).
pub const IXFUSE_MAX: i64 = 1 << 20;

/// N1a (`ixfuse`): marks in `fused` the integer `ADD x, #c` (`x` an instruction, `c` a small constant) whose users are all the index operand of fused
/// `AREF`s: no snapshot, `PHI`, guard, `end_uses` entry or other operand names it. The accesses of such an `AREF` address `x` and add `c` to the
/// displacement (`operand_refs` hands them `x`), so `x` is live up to them and the sum is neither computed nor given a register. Wrapping is the
/// same on both sides: `x + c` (mod 2^64) in the `ADD`, base + x + (disp) (mod 2^64) in the address.
fn fuse_index_adds(ir: &TraceIr, fused: &mut [bool], end_uses: &[Ref]) {
    let n = ir.ins.len();
    let mut cand: Vec<bool> = ir
        .ins
        .iter()
        .map(|i| {
            i.op == TOp::Add
                && i.t == Ty::Int
                && TraceIr::is_ins(i.a)
                && ir
                    .as_int(i.b)
                    .is_some_and(|c| c.unsigned_abs() <= IXFUSE_MAX as u64)
        })
        .collect();
    let mut users = vec![0u32; n];
    let kill = |cand: &mut Vec<bool>, r: Ref| {
        if TraceIr::is_ins(r) && idx(r) < n {
            cand[idx(r)] = false;
        }
    };
    for (k, ins) in ir.ins.iter().enumerate() {
        if ins.op == TOp::Phi {
            kill(&mut cand, ins.a);
            kill(&mut cand, ins.b);
            continue;
        }
        let (ra, rb) = ins.op.ref_operands();
        if ra {
            kill(&mut cand, ins.a);
        }
        if rb {
            if ins.op == TOp::Aref && fused[k] && TraceIr::is_ins(ins.b) && idx(ins.b) < n {
                if cand[idx(ins.b)] && ins.a != ins.b {
                    users[idx(ins.b)] += 1;
                }
            } else {
                kill(&mut cand, ins.b);
            }
        }
        if let Some(t) = ins.third() {
            kill(&mut cand, t);
        }
    }
    for e in &ir.snapmap {
        kill(&mut cand, e.r);
    }
    for &r in end_uses {
        kill(&mut cand, r);
    }
    for k in 0..n {
        if cand[k] && users[k] > 0 {
            fused[k] = true;
        }
    }
}

// ---- parallel moves ---------------------------------------------------------------------------------------------

/// Turns simultaneous moves (every source is read before any destination is written) into a sequence of single
/// moves. Moves with `dst == src` and moves into [`Loc::None`] are dropped; destinations must be distinct. Sources
/// may be duplicated. Cycles are broken by saving one destination in `V7` (general registers, double registers
/// via `FtoG` / `GtoF`, slots; Q1: no store-forwarding round trip through memory) or, only when the list has a
/// memory-to-memory step (which needs `V7` itself), in `Spill(scratch_slot)`.
/// Constant sources are loaded last. `V7` and the scratch slot are free at entry and at exit.
pub fn sequentialise(moves: &[Move], scratch_slot: u16) -> Vec<Move> {
    sequentialise_x(moves, scratch_slot, None)
}

/// [`sequentialise`] with a free double register `xscratch` (not read or written by any move, holding no live value):
/// a cycle whose saved destination is a double register is broken with `movaps` through it (no `movq` round trip on
/// the carried chain: Q1), and it replaces the scratch slot when `V7` is busy with a memory-to-memory step.
pub fn sequentialise_x(moves: &[Move], scratch_slot: u16, xscratch: Option<u8>) -> Vec<Move> {
    let mut pend: Vec<Move> = Vec::new();
    let mut consts: Vec<Move> = Vec::new();
    for m in moves {
        if m.dst == m.src || m.dst == Loc::None || m.src == Loc::None {
            continue;
        }
        debug_assert!(m.dst.is_reg() || matches!(m.dst, Loc::Spill(_)));
        debug_assert!(
            !moves.iter().any(|o| !std::ptr::eq(o, m) && o.dst == m.dst),
            "duplicate destination {:?}",
            m.dst
        );
        if matches!(m.src, Loc::Const(_)) {
            consts.push(*m);
        } else {
            pend.push(*m);
        }
    }
    let mut out = Vec::with_capacity(pend.len() + consts.len() + 1);
    let mut breaks = 0usize;
    loop {
        let mut progressed = true;
        while progressed {
            progressed = false;
            let mut k = 0;
            while k < pend.len() {
                let d = pend[k].dst;
                if pend.iter().enumerate().any(|(j, m)| j != k && m.src == d) {
                    k += 1;
                } else {
                    out.push(pend.remove(k));
                    progressed = true;
                }
            }
        }
        if pend.is_empty() {
            break;
        }
        // every remaining destination is also a source: cycles. Save the destination of the first move.
        if mutants::active("phi_swap") {
            // mutant: the cycle is "broken" by overwriting the first destination without saving it
            out.push(pend.remove(0));
            continue;
        }
        let m = pend[0];
        // Q4: a cycle needs at most one break per move. The mutant `cycle_stale_xmm` never resolves the pending
        // moves, so the loop would append saves until memory ran out (3 GB per detector child): stop it, the
        // unresolved moves make the sequence wrong, which is what the detectors check.
        breaks += 1;
        if mutants::active("cycle_stale_xmm") && breaks > moves.len() {
            break;
        }
        let mem_mem = pend
            .iter()
            .any(|q| matches!(q.dst, Loc::Spill(_)) && matches!(q.src, Loc::Spill(_)));
        // `V7` holds the saved value (raw 64 bits: general register, double register or slot alike) unless a
        // memory-to-memory step needs it as its own scratch; only then does the save go through memory.
        let tmp = match (m.dst, xscratch) {
            (Loc::X(_), Some(xs)) => Loc::X(xs),
            (_, Some(xs)) if mem_mem => Loc::X(xs),
            _ if mem_mem => Loc::Spill(scratch_slot),
            _ => Loc::G(SCRATCH_GPR),
        };
        out.push(Move {
            dst: tmp,
            src: m.dst,
        });
        let stale = mutants::active("cycle_stale_xmm") && matches!(m.dst, Loc::X(_));
        for q in pend.iter_mut() {
            if q.src == m.dst && !stale {
                q.src = tmp;
            }
        }
    }
    out.extend(consts);
    out
}

// ---- the allocator ------------------------------------------------------------------------------------------------

#[derive(Clone, Copy, Default)]
struct Mask {
    g: u16,
    x: u16,
}

impl Mask {
    fn has(&self, x: bool, r: u8) -> bool {
        (if x { self.x } else { self.g }) >> r & 1 != 0
    }
    fn set(&mut self, x: bool, r: u8) {
        if x {
            self.x |= 1 << r;
        } else {
            self.g |= 1 << r;
        }
    }
}

#[derive(Clone, Default)]
struct Val {
    reg: u8,
    slots: Vec<u16>,
    perm: Vec<u16>,
    pins: u32,
    fixed: bool,
    slot_home: bool,
    weak_only: bool,
    hint: u8,
}

struct Ra<'a> {
    ir: &'a TraceIr,
    opts: RaOpts,
    rng: Option<Rng>,
    n: usize,
    l: Option<usize>,
    fused: Vec<bool>,
    defines: Vec<bool>,
    is_x: Vec<bool>,
    vals: Vec<Val>,
    gpr: [u32; NGPR],
    xmm: [u32; NXMM],
    /// Double registers that held a value at some point of the trace (the others are free at the loop moves).
    used_x: u16,
    avail: Mask,
    free_slots: Vec<u16>,
    next_slot: usize,
    reuse_slots: bool,
    out: Vec<InsAlloc>,
    snap_locs: Vec<Vec<Loc>>,
    snap_first: Vec<usize>,
    snap_last: Vec<usize>,
    pinned_by: Vec<Vec<(usize, usize)>>,
    open: Vec<u32>,
    forced: Vec<(u32, Ref)>,
    cur: Mask,
    cur_i: usize,
    evicted: Mask,
    phis: Vec<usize>,
    carry: Vec<(Ref, Loc, Vec<u16>)>,
    top: Vec<Vec<Loc>>,
    end_loc: Vec<Loc>,
    invariants: Vec<usize>,
    entry_moves: Vec<Move>,
    calls: Vec<CallSite>,
    deferred: Vec<u16>,
    end_hints: Vec<Loc>,
    carried: Vec<bool>,
    /// Section 51 (`RAPOL_VICTIM`): per value, the instructions that read it (built at the first eviction: most traces never evict).
    uses: Option<Uses>,
    /// The `end_uses` of the allocation (read at the end).
    end_refs: Vec<Ref>,
}

/// Section 51 (`RAPOL_VICTIM`): per value, the instructions that read it, ascending (one flat array: `at[off[v]..off[v + 1]]`). The entry values of
/// the PHIs are read at the `LOOP` marker, the back-edge values of the PHIs and `end_uses` at the end (`n`).
#[derive(Default)]
struct Uses {
    off: Vec<u32>,
    at: Vec<u32>,
}

impl Uses {
    fn build(
        ir: &TraceIr,
        fused: &[bool],
        l: Option<usize>,
        phis: &[usize],
        end_uses: &[Ref],
    ) -> Uses {
        let n = ir.ins.len();
        // (value, instruction) in instruction order, then a counting sort by value (stable: each value's reads stay ascending)
        let mut pairs: Vec<(u32, u32)> = Vec::with_capacity(2 * n);
        let mut add = |r: Ref, at: usize| {
            if TraceIr::is_ins(r) && idx(r) < n {
                pairs.push((idx(r) as u32, at as u32));
            }
        };
        for i in 0..n {
            if l == Some(i) {
                for &p in phis {
                    add(entry_ref(ir, p), i);
                }
                continue;
            }
            if ir.ins[i].op == TOp::Phi {
                continue;
            }
            let refs = operand_refs(ir, fused, i);
            for (k, &r) in refs.iter().enumerate() {
                if !refs[..k].contains(&r) {
                    add(r, i);
                }
            }
        }
        for &p in phis {
            add(ir.ins[p].b, n);
        }
        for &r in end_uses {
            add(r, n);
        }
        let mut off = vec![0u32; n + 1];
        for &(v, _) in &pairs {
            off[v as usize + 1] += 1;
        }
        for v in 0..n {
            off[v + 1] += off[v];
        }
        let mut fill: Vec<u32> = off[..n].to_vec();
        let mut at = vec![0u32; pairs.len()];
        for &(v, pos) in &pairs {
            at[fill[v as usize] as usize] = pos;
            fill[v as usize] += 1;
        }
        Uses { off, at }
    }

    /// The last read of `v` before instruction `i`, if any.
    fn before(&self, v: usize, i: usize) -> Option<u32> {
        let u = &self.at[self.off[v] as usize..self.off[v + 1] as usize];
        let k = u.partition_point(|&p| (p as usize) < i);
        (k > 0).then(|| u[k - 1])
    }
}

type R<T> = Result<T, RaError>;

fn bad<T>(s: impl Into<String>) -> R<T> {
    Err(RaError::BadIr(s.into()))
}

/// Allocates registers and spill slots for `ir`. `end_uses` are extra refs that must be available at the end of the
/// trace (before the back-edge moves), e.g. the TR3b writeback values; their locations come back in
/// [`Allocation::end_locs`].
pub fn allocate(ir: &TraceIr, end_uses: &[Ref], opts: &RaOpts) -> Result<Allocation, RaError> {
    allocate_hinted(ir, end_uses, &[], opts)
}

/// [`allocate`] with a preferred register for each ref of `end_uses` (`Loc::G` / `Loc::X`; anything else, or a missing entry, is no hint): R1, a carrying
/// link wants the values where the target root holds them, so that its moves vanish.
pub fn allocate_hinted(
    ir: &TraceIr,
    end_uses: &[Ref],
    end_hints: &[Loc],
    opts: &RaOpts,
) -> Result<Allocation, RaError> {
    let n = ir.ins.len();
    let l = ir.loop_at.map(idx);
    if let Some(l) = l {
        if l >= n || ir.ins[l].op != TOp::Loop {
            return bad("loop_at is not a LOOP");
        }
    }
    // ---- validation and fusion ----
    let mut phis = Vec::new();
    for (i, ins) in ir.ins.iter().enumerate() {
        match ins.op {
            TOp::Loop if l != Some(i) => return bad("stray LOOP"),
            TOp::Phi => {
                let Some(lp) = l else {
                    return bad("PHI without LOOP");
                };
                if i != lp + 1 + phis.len() {
                    return bad("PHIs must directly follow LOOP");
                }
                phis.push(i);
            }
            _ => {}
        }
        let (ra, rb) = ins.op.ref_operands();
        let chk = |r: Ref, fwd_ok: bool| -> R<()> {
            if r == NOREF {
                return bad(format!("NOREF operand at {i}"));
            }
            if r < REF_BIAS {
                if usize::from(r) > ir.k.len() {
                    return bad(format!("constant {r} out of range at {i}"));
                }
            } else if idx(r) >= n || (idx(r) >= i && !fwd_ok) {
                return bad(format!("operand {r} not defined before {i}"));
            }
            Ok(())
        };
        match ins.op {
            TOp::Phi => {
                chk(ins.a, false)?;
                chk(ins.b, true)?;
            }
            TOp::Loop | TOp::Poll => {}
            TOp::Tset => {
                chk(ins.a, false)?;
                chk(ins.b, false)?;
                chk(ins.aux as Ref, false)?;
            }
            TOp::Concat => {
                chk(ins.a, false)?;
                chk(ins.b, false)?;
                if ins.aux != 0 {
                    chk(ins.aux as Ref, false)?;
                }
            }
            TOp::Lib => {
                chk(ins.a, false)?;
                chk(ins.b, false)?;
                if ins.aux >> 8 != 0 {
                    chk((ins.aux >> 8) as Ref, false)?;
                }
            }
            TOp::Calln => {
                chk(ins.a, false)?;
                if ins.b != NOREF {
                    chk(ins.b, false)?;
                }
            }
            TOp::Neg
            | TOp::Bnot
            | TOp::Fneg
            | TOp::Fabs
            | TOp::Fsqrt
            | TOp::Ffloor
            | TOp::ConvNumInt
            | TOp::F2iExact
            | TOp::Aload
            | TOp::Uload
            | TOp::Hload
            | TOp::Uvrel
            | TOp::Fload => chk(ins.a, false)?,
            _ => {
                if ra {
                    chk(ins.a, false)?;
                }
                if rb && !is_kconst(ins.op) {
                    chk(ins.b, false)?;
                }
            }
        }
    }
    if phis.len() > MAX_PHI {
        return Err(RaError::Phi);
    }
    let mut fused: Vec<bool> = ir
        .ins
        .iter()
        .map(|i| {
            matches!(i.op, TOp::Aref | TOp::Uref)
                // H1: a `HREFK` of a node in a register (a constant node would need a scratch register at each user)
                || (opts.hkaddr && i.op == TOp::Hrefk && TraceIr::is_ins(i.a))
        })
        .collect();
    let unfuse = |fused: &mut Vec<bool>, r: Ref| {
        if TraceIr::is_ins(r) && idx(r) < fused.len() {
            fused[idx(r)] = false;
        }
    };
    for ins in &ir.ins {
        let (ra, rb) = ins.op.ref_operands();
        let a_ok = is_mem(ins.op) && ins.a != ins.b;
        if ra && !a_ok {
            unfuse(&mut fused, ins.a);
        }
        if rb {
            unfuse(&mut fused, ins.b);
        }
    }
    let mut guard_snaps: Vec<(usize, u32)> = Vec::new();
    // Section 49 (`cspeed` lever `RA`): the per-snapshot work once per snapshot, not once per guard (the guards of one snapshot share its
    // map, TR27): `snap_once[k]` = this is the first guard of its snapshot
    let once = crate::cspeed::on(crate::cspeed::RA);
    let mut seen = vec![false; if once { ir.snaps.len() } else { 0 }];
    let mut snap_once: Vec<bool> = Vec::new();
    for (i, ins) in ir.ins.iter().enumerate() {
        if ins.flags & F_GUARD != 0 {
            let s = aux_snap(ins.aux);
            if s as usize >= ir.snaps.len() {
                return bad(format!("snapshot {s} out of range at {i}"));
            }
            guard_snaps.push((i, s));
            let first = (!once || !std::mem::replace(&mut seen[s as usize], true))
                && !(once && mutants::active("cspeed_ra_unfuse_skip"));
            snap_once.push(first);
            if first {
                for e in ir.snap_entries(s) {
                    unfuse(&mut fused, e.r);
                }
            }
        }
    }
    for &r in end_uses {
        unfuse(&mut fused, r);
    }
    if opts.ixfuse {
        fuse_index_adds(ir, &mut fused, end_uses);
    }
    for e in &ir.snapmap {
        if e.r != NOREF && e.r < REF_BIAS && usize::from(e.r) > ir.k.len() {
            return bad("snapshot constant out of range");
        }
        if TraceIr::is_ins(e.r) && idx(e.r) >= n {
            return bad("snapshot ref out of range");
        }
    }
    let defs: Vec<bool> = ir
        .ins
        .iter()
        .zip(&fused)
        .map(|(i, &f)| defines(i.op, f))
        .collect();
    // snapshot refs must be values (the first guard of a snapshot is the strictest test: `idx >= i` only grows with `i`)
    for (&(i, s), _) in guard_snaps
        .iter()
        .zip(&snap_once)
        .filter(|&(_, &first)| first)
    {
        for e in ir.snap_entries(s) {
            if TraceIr::is_ins(e.r) && (idx(e.r) >= i || !defs[idx(e.r)]) {
                return bad(format!("snapshot {s} ref {} unusable at {i}", e.r));
            }
        }
    }
    let mut avail = Mask {
        g: 0x17F
            | (u16::from(opts.xregs & 1) << 9)
            | if (ir.rec_call.is_some() || ir.rec_ret.is_some())
                && !mutants::active("xregs_rec_r14")
            {
                0
            } else {
                u16::from(opts.xregs >> 1 & 1) << 10
            },
        x: 0xFFFF,
    };
    if let Some(r) = opts.reserve_gpr {
        if !GPR_POOL.contains(&r) || r >= NGPR_CLOBBERED as u8 {
            return bad(format!("reserve_gpr {r} is not allocatable"));
        }
        avail.g &= !(1u16 << r);
    }
    for ins in &ir.ins {
        let c = op_clobbers(ins, ir);
        avail.g &= !c.gpr;
        avail.x &= !c.xmm;
    }
    let mut snap_first = vec![usize::MAX; ir.snaps.len()];
    let mut snap_last = vec![usize::MAX; ir.snaps.len()];
    for &(i, s) in &guard_snaps {
        let s = s as usize;
        if snap_first[s] == usize::MAX {
            snap_first[s] = i;
        }
        snap_last[s] = i;
    }
    let mut weak = vec![false; n];
    {
        let mut real = vec![false; n];
        let mut snapped = vec![false; n];
        for i in 0..n {
            for r in operand_refs(ir, &fused, i) {
                if TraceIr::is_ins(r) {
                    real[idx(r)] = true;
                }
            }
            if ir.ins[i].op == TOp::Phi {
                for r in [ir.ins[i].a, ir.ins[i].b] {
                    if TraceIr::is_ins(r) {
                        real[idx(r)] = true;
                    }
                }
            }
        }
        for &r in end_uses {
            if TraceIr::is_ins(r) {
                real[idx(r)] = true;
            }
        }
        for (&(_, s), _) in guard_snaps.iter().zip(&snap_once).filter(|&(_, &f)| f) {
            for e in ir.snap_entries(s) {
                if TraceIr::is_ins(e.r) {
                    snapped[idx(e.r)] = true;
                }
            }
        }
        for i in 0..n {
            weak[i] = snapped[i] && !real[i];
        }
    }
    let is_x: Vec<bool> = ir.ins.iter().map(|i| i.t == Ty::Num).collect();
    let mut rng = opts.randra.map(Rng::new);
    let reuse_slots = match rng.as_mut() {
        Some(r) => !r.chance(1, 4),
        None => true,
    };
    let ra = Ra {
        ir,
        opts: *opts,
        rng,
        n,
        l,
        fused,
        defines: defs,
        is_x,
        vals: vec![
            Val {
                reg: NOREG,
                hint: NOREG,
                ..Val::default()
            };
            n
        ],
        gpr: [NONE; NGPR],
        xmm: [NONE; NXMM],
        used_x: 0,
        avail,
        free_slots: Vec::new(),
        next_slot: 0,
        reuse_slots,
        out: vec![InsAlloc::default(); n],
        snap_locs: ir
            .snaps
            .iter()
            .enumerate()
            .map(|(s, _)| vec![Loc::None; ir.snap_entries(s as u32).len()])
            .collect(),
        snap_first,
        snap_last,
        pinned_by: vec![Vec::new(); ir.snaps.len()],
        open: Vec::new(),
        forced: Vec::new(),
        cur: Mask::default(),
        cur_i: 0,
        evicted: Mask::default(),
        phis,
        carry: Vec::new(),
        top: vec![Vec::new(); n],
        end_loc: vec![Loc::None; n],
        invariants: Vec::new(),
        entry_moves: Vec::new(),
        calls: Vec::new(),
        deferred: Vec::new(),
        end_hints: end_hints.to_vec(),
        carried: ir
            .ins
            .iter()
            .map(|i| i.op == TOp::Sload && aux_bits(i.aux) & SLOAD_CARRIED != 0)
            .collect(),
        uses: None,
        end_refs: end_uses.to_vec(),
    };
    ra.run(end_uses, weak)
}

impl Ra<'_> {
    fn rnd(&mut self, num: u64, den: u64) -> bool {
        match self.rng.as_mut() {
            Some(r) => r.chance(num, den),
            None => false,
        }
    }

    fn rbelow(&mut self, n: usize) -> usize {
        match self.rng.as_mut() {
            Some(r) => r.below(n as u64) as usize,
            None => 0,
        }
    }

    fn pool(x: bool) -> Vec<u8> {
        if x {
            (0..NXMM as u8).collect()
        } else {
            GPR_POOL.to_vec()
        }
    }

    /// [`Ra::pool`] as an array on the stack and its length.
    fn pool_arr(x: bool) -> ([u8; 16], usize) {
        let mut a = [0u8; 16];
        if x {
            for (k, r) in a.iter_mut().enumerate().take(NXMM) {
                *r = k as u8;
            }
            (a, NXMM)
        } else {
            a[..GPR_POOL.len()].copy_from_slice(&GPR_POOL);
            (a, GPR_POOL.len())
        }
    }

    fn holder(&self, x: bool, r: u8) -> u32 {
        if x {
            self.xmm[usize::from(r)]
        } else {
            self.gpr[usize::from(r)]
        }
    }

    fn set_holder(&mut self, x: bool, r: u8, v: u32) {
        if x {
            if v != NONE {
                self.used_x |= 1 << r;
            }
            self.xmm[usize::from(r)] = v;
        } else {
            self.gpr[usize::from(r)] = v;
        }
    }

    fn rloc(x: bool, r: u8) -> Loc {
        if x {
            Loc::X(r)
        } else {
            Loc::G(r)
        }
    }

    fn take_reg(&mut self, v: usize, r: u8) {
        let x = self.is_x[v];
        debug_assert_eq!(self.holder(x, r), NONE);
        self.set_holder(x, r, v as u32);
        self.vals[v].reg = r;
    }

    fn free_reg(&mut self, v: usize) {
        let r = self.vals[v].reg;
        if r != NOREG {
            let x = self.is_x[v];
            self.set_holder(x, r, NONE);
            self.vals[v].reg = NOREG;
        }
    }

    fn new_slot(&mut self, perm: bool) -> R<u16> {
        if !perm && self.reuse_slots {
            if let Some(s) = self.free_slots.pop() {
                return Ok(s);
            }
        }
        if self.next_slot >= usize::from(SCRATCH_SPILL) {
            return Err(RaError::Spill);
        }
        self.next_slot += 1;
        Ok((self.next_slot - 1) as u16)
    }

    fn any_slot(&self, v: usize) -> Option<u16> {
        self.vals[v]
            .perm
            .first()
            .or_else(|| self.vals[v].slots.first())
            .copied()
    }

    fn ensure_slot(&mut self, v: usize) -> R<u16> {
        if let Some(s) = self.any_slot(v) {
            return Ok(s);
        }
        let s = self.new_slot(false)?;
        if self.reuse_slots {
            self.vals[v].slots.push(s);
        } else {
            self.vals[v].perm.push(s);
        }
        Ok(s)
    }

    /// A free register of the class (not excluded), preferring `hint`.
    fn pick_free(&mut self, x: bool, excl: Mask, hint: u8) -> Option<u8> {
        if crate::cspeed::on(crate::cspeed::RA) {
            // Section 49 (`cspeed` lever `RA`): the same choice from a list on the stack (the pool is at most 16 registers)
            let (pool, np) = Self::pool_arr(x);
            let mut free = [0u8; 16];
            let mut nf = 0usize;
            for &r in &pool[..np] {
                if self.avail.has(x, r)
                    && self.holder(x, r) == NONE
                    && !excl.has(x, r)
                    && !self.cur.has(x, r)
                {
                    free[nf] = r;
                    nf += 1;
                }
            }
            if nf == 0 {
                return None;
            }
            let free = &free[..nf];
            let use_hint = !(self.rnd(1, 3));
            if hint != NOREG && use_hint && free.contains(&hint) {
                return Some(hint);
            }
            if self.rng.is_some() {
                let k = self.rbelow(free.len());
                return Some(free[k]);
            }
            if mutants::active("cspeed_ra_last_free") {
                // MUTANT: the last free register instead of the first
                return Some(free[free.len() - 1]);
            }
            return Some(free[0]);
        }
        let free: Vec<u8> = Self::pool(x)
            .into_iter()
            .filter(|&r| {
                self.avail.has(x, r)
                    && self.holder(x, r) == NONE
                    && !excl.has(x, r)
                    && !self.cur.has(x, r)
            })
            .collect();
        if free.is_empty() {
            return None;
        }
        let use_hint = !(self.rnd(1, 3));
        if hint != NOREG && use_hint && free.contains(&hint) {
            return Some(hint);
        }
        if self.rng.is_some() {
            let k = self.rbelow(free.len());
            return Some(free[k]);
        }
        Some(free[0])
    }

    fn score(&self, v: usize) -> i64 {
        if self.opts.pol & RAPOL_VICTIM != 0 && self.l.is_some_and(|l| self.cur_i > l) {
            // the previous use before the current instruction (its own operands are not candidates), else the definition: the
            // register is free over the longest stretch above the current instruction
            let mut s = self
                .uses
                .as_ref()
                .and_then(|u| u.before(v, self.cur_i))
                .map_or(v as i64, i64::from);
            if self.vals[v].weak_only {
                s -= 1_000_000;
            }
            return s;
        }
        let mut s = v as i64;
        if self.ir.ins[v].op == TOp::Phi {
            s += 100_000;
        }
        if self.vals[v].weak_only {
            s -= 1_000_000;
        }
        s
    }

    /// Evicts the best victim of the class not in `excl`; returns its register.
    fn try_evict(&mut self, x: bool, excl: Mask) -> R<Option<u8>> {
        if self.opts.pol & RAPOL_VICTIM != 0
            && self.uses.is_none()
            && self.l.is_some_and(|l| self.cur_i > l)
        {
            let ends: Vec<Ref> = std::mem::take(&mut self.end_refs);
            self.uses = Some(Uses::build(self.ir, &self.fused, self.l, &self.phis, &ends));
            self.end_refs = ends;
        }
        let ignore_pins = mutants::active("exit_rename_ignored");
        let mut best: Option<(i64, u8)> = None;
        let mut best_pinned: Option<(i64, u8)> = None;
        let (pool, np) = if crate::cspeed::on(crate::cspeed::RA) {
            Self::pool_arr(x)
        } else {
            let v = Self::pool(x);
            let mut a = [0u8; 16];
            a[..v.len()].copy_from_slice(&v);
            (a, v.len())
        };
        for &r in &pool[..np] {
            if !self.avail.has(x, r) || excl.has(x, r) || self.cur.has(x, r) {
                continue;
            }
            let h = self.holder(x, r);
            if h == NONE {
                continue;
            }
            let v = h as usize;
            if self.vals[v].fixed {
                continue;
            }
            let sc = self.score(v);
            let slot = if self.vals[v].pins > 0 && !ignore_pins {
                &mut best_pinned
            } else {
                &mut best
            };
            if slot.is_none_or(|(b, _)| sc < b) {
                *slot = Some((sc, r));
            }
        }
        let force = best_pinned.is_some() && self.rnd(1, 10);
        let r = match (best, best_pinned) {
            (_, Some((_, r))) if force => {
                self.patch_pins(self.holder(x, r) as usize)?;
                r
            }
            (Some((_, r)), _) => r,
            (None, Some((_, r))) => {
                self.patch_pins(self.holder(x, r) as usize)?;
                r
            }
            (None, None) => return Ok(None),
        };
        let v = self.holder(x, r) as usize;
        self.evict(v, x, r)?;
        Ok(Some(r))
    }

    /// Forces every pinned snapshot location of `v` to a fresh spill slot (TR27: a rename between guards of one
    /// snapshot becomes a spill).
    fn patch_pins(&mut self, v: usize) -> R<()> {
        let s = self.new_slot(true)?;
        self.vals[v].perm.push(s);
        let reg = Self::rloc(self.is_x[v], self.vals[v].reg);
        for si in self.open.clone() {
            let si = si as usize;
            let mut keep = Vec::new();
            for (k, pv) in std::mem::take(&mut self.pinned_by[si]) {
                if pv == v {
                    debug_assert_eq!(self.snap_locs[si][k], reg);
                    self.snap_locs[si][k] = Loc::Spill(s);
                    self.vals[v].pins -= 1;
                    self.forced.push((si as u32, rf(v)));
                } else {
                    keep.push((k, pv));
                }
            }
            self.pinned_by[si] = keep;
        }
        self.open
            .retain(|&si| !self.pinned_by[si as usize].is_empty());
        debug_assert_eq!(self.vals[v].pins, 0);
        Ok(())
    }

    fn evict(&mut self, v: usize, x: bool, r: u8) -> R<()> {
        let slot = self.ensure_slot(v)?;
        if !mutants::active("spill_no_reload") {
            self.out[self.cur_i].post.push(Reload {
                reg: Self::rloc(x, r),
                slot,
                r: rf(v),
            });
        }
        self.free_reg(v);
        self.evicted.set(x, r);
        Ok(())
    }

    /// A register for a value or a temporary. `must`: evict if none is free (and fail with `Pressure` if even that
    /// is impossible); otherwise `None` when none is free.
    fn alloc_temp(&mut self, x: bool, excl: Mask, hint: u8, must: bool) -> R<Option<u8>> {
        if !must && self.rnd(1, 6) {
            return Ok(None);
        }
        if must && self.rnd(1, 8) {
            if let Some(r) = self.try_evict(x, excl)? {
                return Ok(Some(r));
            }
        }
        if let Some(r) = self.pick_free(x, excl, hint) {
            return Ok(Some(r));
        }
        if must {
            return match self.try_evict(x, excl)? {
                Some(r) => Ok(Some(r)),
                None => Err(RaError::Pressure),
            };
        }
        Ok(None)
    }

    // ---- driver ----

    fn run(mut self, end_uses: &[Ref], weak: Vec<bool>) -> R<Allocation> {
        let ir = self.ir;
        if let Some(l) = self.l {
            self.make_homes(l, end_uses)?;
        }
        // locations of the values live at the end of the body
        let mut ends: Vec<Ref> = self.phis.iter().map(|&p| ir.ins[p].b).collect();
        let nphi = ends.len();
        ends.extend_from_slice(end_uses);
        // R1: a carried load prefers the register the parent's exit map holds the value in (the carry entry's move then vanishes)
        for k in 0..self.n {
            if ir.ins[k].op == TOp::Sload && aux_bits(ir.ins[k].aux) & SLOAD_CARRIED != 0 {
                if let Some(&(_, src, _)) = ir.carry_in.iter().find(|(s, _, _)| *s == ir.ins[k].a) {
                    match src {
                        CarrySrc::G(n) if !self.is_x[k] => self.vals[k].hint = n,
                        CarrySrc::X(n) if self.is_x[k] => self.vals[k].hint = n,
                        _ => {}
                    }
                }
            }
        }
        for (k, &r) in ends.iter().enumerate() {
            let hint = k
                .checked_sub(nphi)
                .and_then(|j| self.end_hints.get(j).copied());
            self.end_loc_of(r, hint.unwrap_or(Loc::None))?;
        }
        for &p in &self.phis.clone() {
            let b = ir.ins[p].b;
            if TraceIr::is_ins(b) {
                if let Loc::G(r) | Loc::X(r) = self.end_loc[idx(b)] {
                    if self.is_x[idx(b)] == self.is_x[p] {
                        self.vals[p].hint = r;
                    }
                }
            }
        }
        let mut i = self.n;
        while i > 0 {
            i -= 1;
            self.cur_i = i;
            self.cur = Mask::default();
            self.evicted = Mask::default();
            match ir.ins[i].op {
                TOp::Loop => self.cross_loop(i)?,
                TOp::Phi => self.def_phi(i)?,
                // R1: a carried load defines nothing where it stands: its value lives from the start of the trace
                TOp::Sload if aux_bits(ir.ins[i].aux) & SLOAD_CARRIED != 0 => {}
                _ => self.process(i)?,
            }
            if self.opts.carry_at != 0 && i == self.opts.carry_at {
                self.capture_carry();
            }
        }
        self.capture_carried_loads();
        if !self.open.is_empty() {
            return bad("snapshot left open");
        }
        // back-edge moves
        let mut bm: Vec<Move> = Vec::new();
        for &p in &self.phis {
            let b = ir.ins[p].b;
            let src = if TraceIr::is_ins(b) {
                self.end_loc[idx(b)]
            } else {
                Loc::Const(b)
            };
            for &d in &self.top[p] {
                bm.push(Move { dst: d, src });
            }
        }
        let mut em = std::mem::take(&mut self.entry_moves);
        let mut after = self.n;
        if mutants::active("phi_skip_move") {
            if let Some(k) = bm.iter().position(|m| m.dst != m.src) {
                bm.remove(k);
            }
            if let Some(k) = em.iter().position(|m| m.dst != m.src) {
                em.remove(k);
            }
        }
        if mutants::active("phi_swap") {
            let sw = |v: &mut Vec<Move>| {
                let nz: Vec<usize> = (0..v.len()).filter(|&k| v[k].dst != v[k].src).collect();
                if nz.len() >= 2 {
                    let (a, b) = (v[nz[0]].src, v[nz[1]].src);
                    v[nz[0]].src = b;
                    v[nz[1]].src = a;
                }
            };
            sw(&mut bm);
        }
        if mutants::active("phi_before_guard") {
            // the moves run before the last guard of the body instead of after it
            if let Some(l) = self.l {
                if let Some(g) = (l + 1..self.n)
                    .rev()
                    .find(|&k| ir.ins[k].flags & F_GUARD != 0)
                {
                    after = g;
                }
            }
        }
        // a double register that never held a value: free scratch for XMM cycles. Registers named by a move are
        // excluded as well (they held a value, so the mask already covers them).
        let mut busy = self.used_x;
        for m in bm.iter().chain(&em) {
            for l in [m.dst, m.src] {
                if let Loc::X(r) = l {
                    busy |= 1 << r;
                }
            }
        }
        let xscratch = (0..NXMM as u8).find(|&r| busy >> r & 1 == 0);
        let backedge_seq = sequentialise_x(&bm, SCRATCH_SPILL, xscratch);
        let entry_seq = sequentialise_x(&em, SCRATCH_SPILL, xscratch);
        let end_locs = end_uses
            .iter()
            .map(|&r| {
                if TraceIr::is_ins(r) {
                    self.end_loc[idx(r)]
                } else {
                    Loc::Const(r)
                }
            })
            .collect();
        self.calls.reverse();
        Ok(Allocation {
            ins: self.out,
            fused: self.fused,
            weak,
            snap_locs: self.snap_locs,
            forced_spills: self.forced,
            entry_moves: em,
            entry_seq,
            backedge_moves: bm,
            backedge_seq,
            backedge_after: after,
            end_locs,
            carry: std::mem::take(&mut self.carry),
            calls: self.calls,
            nspill: self.next_slot as u16,
            used: std::cell::OnceCell::new(),
        })
    }

    /// R1 (half A): the locations of the carried loads at the start of the trace.
    fn capture_carried_loads(&mut self) {
        let ir = self.ir;
        let mut out = Vec::new();
        for k in 0..self.n {
            if ir.ins[k].op != TOp::Sload || aux_bits(ir.ins[k].aux) & SLOAD_CARRIED == 0 {
                continue;
            }
            let x = self.is_x[k];
            let reg = if self.vals[k].reg != NOREG {
                Self::rloc(x, self.vals[k].reg)
            } else {
                Loc::None
            };
            let mut slots = self.vals[k].slots.clone();
            slots.extend(self.vals[k].perm.iter().copied());
            if reg == Loc::None && slots.is_empty() {
                continue;
            }
            out.push((rf(k), reg, slots));
        }
        if !out.is_empty() {
            self.carry = out;
        }
    }

    /// R1: the state of the leading `SLOAD` run's values at the point before instruction `carry_at` (called after that instruction is processed).
    fn capture_carry(&mut self) {
        let ir = self.ir;
        let mut out = Vec::new();
        for k in 2..self.opts.carry_at.min(self.n) {
            if ir.ins[k].op != TOp::Sload || !self.defines[k] {
                continue;
            }
            let x = self.is_x[k];
            let reg = if self.vals[k].reg != NOREG {
                Self::rloc(x, self.vals[k].reg)
            } else {
                Loc::None
            };
            let mut slots = self.vals[k].slots.clone();
            slots.extend(self.vals[k].perm.iter().copied());
            if reg == Loc::None && slots.is_empty() {
                continue; // dead behind the run
            }
            out.push((rf(k), reg, slots));
        }
        self.carry = out;
    }

    /// Chooses the whole-body homes of the loop-invariant values (pre-roll values the body uses).
    fn make_homes(&mut self, l: usize, end_uses: &[Ref]) -> R<()> {
        let ir = self.ir;
        let mut rc = vec![0u32; l];
        let mut sc = vec![0u32; l];
        let bump = |v: &mut Vec<u32>, r: Ref| {
            if TraceIr::is_ins(r) && idx(r) < l {
                v[idx(r)] += 1;
            }
        };
        for j in l + 1..self.n {
            let ins = &ir.ins[j];
            if ins.op == TOp::Phi {
                bump(&mut rc, ins.b);
                continue;
            }
            for r in operand_refs(ir, &self.fused, j) {
                bump(&mut rc, r);
            }
            if ins.flags & F_GUARD != 0 {
                for e in ir.snap_entries(aux_snap(ins.aux)) {
                    bump(&mut sc, e.r);
                }
            }
        }
        for &r in end_uses {
            bump(&mut rc, r);
        }
        let mut cands: Vec<usize> = (0..l)
            .filter(|&v| self.defines[v] && rc[v] + sc[v] > 0)
            .collect();
        cands.sort_by_key(|&v| (std::cmp::Reverse(rc[v]), v));
        let (mut cap_g, mut cap_x) = (self.opts.max_gpr_homes, self.opts.max_xmm_homes);
        if self.opts.reserve_gpr.is_some() {
            // a reserved register must not starve the instructions (up to three registers at a time, 7.3)
            cap_g = cap_g.min((self.avail.g.count_ones() as usize).saturating_sub(3));
        }
        if self.rng.is_some() {
            cap_g = self.rbelow(cap_g + 1);
            cap_x = self.rbelow(cap_x + 1);
        }
        for v in cands {
            let x = self.is_x[v];
            let cap = if x { &mut cap_x } else { &mut cap_g };
            let mut reg = None;
            if rc[v] > 0 && *cap > 0 && !(self.rng.is_some() && self.rnd(1, 6)) {
                let mut pool = Self::pool(x);
                pool.reverse();
                pool.retain(|&r| self.avail.has(x, r) && self.holder(x, r) == NONE);
                if !pool.is_empty() {
                    let k = if self.rng.is_some() {
                        self.rbelow(pool.len())
                    } else {
                        0
                    };
                    reg = Some(pool[k]);
                }
            }
            match reg {
                Some(r) => {
                    if x {
                        cap_x -= 1;
                    } else {
                        cap_g -= 1;
                    }
                    self.take_reg(v, r);
                    self.vals[v].fixed = true;
                }
                None => {
                    let s = self.new_slot(true)?;
                    self.vals[v].perm.push(s);
                    self.vals[v].slot_home = true;
                }
            }
            self.invariants.push(v);
        }
        Ok(())
    }

    /// Location at the end of the body of `r`, allocating it if needed.
    fn end_loc_of(&mut self, r: Ref, hint: Loc) -> R<Loc> {
        if r == NOREF {
            return bad("NOREF at the end");
        }
        if r < REF_BIAS {
            return Ok(Loc::Const(r));
        }
        let v = idx(r);
        if !self.defines[v] {
            return bad("end use of a non-value");
        }
        if self.end_loc[v] != Loc::None {
            return Ok(self.end_loc[v]);
        }
        let x = self.is_x[v];
        let loc = if self.vals[v].slot_home {
            Loc::Spill(self.any_slot(v).expect("home slot"))
        } else if self.vals[v].reg != NOREG {
            Self::rloc(x, self.vals[v].reg)
        } else {
            let h = match hint {
                Loc::G(n) if !x => n,
                Loc::X(n) if x => n,
                _ => self.vals[v].hint,
            };
            match self.alloc_temp(x, Mask::default(), h, false)? {
                Some(reg) => {
                    self.take_reg(v, reg);
                    Self::rloc(x, reg)
                }
                None => Loc::Spill(self.ensure_slot(v)?),
            }
        };
        self.end_loc[v] = loc;
        Ok(loc)
    }

    /// The definition of a PHI at the top of the body.
    fn def_phi(&mut self, i: usize) -> R<()> {
        let x = self.is_x[i];
        let v = i;
        let mut tops = Vec::new();
        if self.vals[v].reg != NOREG {
            let loc = Self::rloc(x, self.vals[v].reg);
            self.out[i].dst = loc;
            tops.push(loc);
            self.out[i].stores = self.vals[v]
                .slots
                .iter()
                .chain(&self.vals[v].perm)
                .copied()
                .collect();
        } else if let Some(s) = self.any_slot(v) {
            self.out[i].dst = Loc::Spill(s);
            for &s in self.vals[v].slots.iter().chain(&self.vals[v].perm) {
                tops.push(Loc::Spill(s));
            }
        }
        self.top[i] = tops;
        self.free_reg(v);
        self.release_slots(v);
        Ok(())
    }

    fn release_slots(&mut self, v: usize) {
        let slots = std::mem::take(&mut self.vals[v].slots);
        if self.reuse_slots {
            self.free_slots.extend(slots);
        }
    }

    /// Crossing the `LOOP` marker (going up): the PHIs are defined, the invariants lose their special status and the
    /// entry moves read the pre-roll values.
    fn cross_loop(&mut self, l: usize) -> R<()> {
        if !self.open.is_empty() {
            return bad("a snapshot with register refs spans LOOP");
        }
        for v in std::mem::take(&mut self.invariants) {
            self.vals[v].fixed = false;
            if self.vals[v].slot_home {
                self.vals[v].slot_home = false;
                let perm = std::mem::take(&mut self.vals[v].perm);
                self.vals[v].slots.extend(perm);
            }
        }
        let ir = self.ir;
        for k in 0..self.phis.len() {
            let p = self.phis[k];
            let a = entry_ref(ir, p);
            let x = self.is_x[p];
            let src = if a < REF_BIAS {
                Loc::Const(a)
            } else {
                let v = idx(a);
                if !self.defines[v] {
                    return bad("PHI of a non-value");
                }
                if self.vals[v].reg != NOREG {
                    Self::rloc(self.is_x[v], self.vals[v].reg)
                } else {
                    let hint = match self.top[p].first() {
                        Some(Loc::G(r)) | Some(Loc::X(r)) => *r,
                        _ => NOREG,
                    };
                    match self.alloc_temp(x, Mask::default(), hint, false)? {
                        Some(r) => {
                            self.take_reg(v, r);
                            Self::rloc(x, r)
                        }
                        None => Loc::Spill(self.ensure_slot(v)?),
                    }
                }
            };
            for d in self.top[p].clone() {
                self.entry_moves.push(Move { dst: d, src });
            }
        }
        self.out[l].dst = Loc::None;
        Ok(())
    }

    /// One ordinary instruction.
    fn process(&mut self, i: usize) -> R<()> {
        let ir = self.ir;
        let ins = ir.ins[i];
        let mut dst_mask = Mask::default();
        let mut dst_loc = Loc::None;
        // (a) the result
        if self.defines[i] {
            let x = self.is_x[i];
            let r = if self.vals[i].reg != NOREG {
                let r = self.vals[i].reg;
                self.free_reg(i);
                r
            } else {
                match self.alloc_temp(x, Mask::default(), NOREG, true)? {
                    Some(r) => r,
                    None => return Err(RaError::Pressure),
                }
            };
            dst_loc = Self::rloc(x, r);
            dst_mask.set(x, r);
            self.out[i].dst = dst_loc;
            let mut st: Vec<u16> = self.vals[i].slots.clone();
            st.extend(self.vals[i].perm.iter().copied());
            self.out[i].stores = st;
            // the slots are free for earlier instructions only: a victim of this instruction is reloaded after
            // the store, so they must not be handed out before the instruction is done
            let slots = std::mem::take(&mut self.vals[i].slots);
            self.deferred.extend(slots);
        }
        let live_after = if is_call(ir, &ins) {
            if self.opts.pol & RAPOL_CALLSPILL != 0 && always_calls(&ins) {
                self.spill_across_call()?;
            }
            let mut m = Mask::default();
            // `V9` / `V10` (`rbp`, `r14`) are callee-saved: `CallH` keeps them
            for r in 0..NGPR_CLOBBERED as u8 {
                if self.gpr[usize::from(r)] != NONE {
                    m.set(false, r);
                }
            }
            for r in 0..NXMM as u8 {
                if self.xmm[usize::from(r)] != NONE {
                    m.set(true, r);
                }
            }
            Some(m)
        } else {
            None
        };
        // (b) the operands
        let refs = operand_refs(ir, &self.fused, i);
        let mut locs = [Loc::None; 3];
        for k in 0..3 {
            let r = refs[k];
            if r == NOREF {
                continue;
            }
            if r < REF_BIAS {
                locs[k] = Loc::Const(r);
                continue;
            }
            if let Some(j) = (0..k).find(|&j| refs[j] == r) {
                locs[k] = locs[j];
                continue;
            }
            let v = idx(r);
            let x = self.is_x[v];
            let mut excl = Mask::default();
            if k == 1 && dst_loc.is_reg() {
                // two-address: the second operand never shares the destination register
                if let Loc::G(d) | Loc::X(d) = dst_loc {
                    if self.is_x[i] == x {
                        excl.set(x, d);
                    }
                }
            }
            let hint =
                if self.carried.get(v).copied().unwrap_or(false) && self.vals[v].hint != NOREG {
                    // R1: a carried load wants the register its parent's exit map holds the value in (the carry entry then moves nothing)
                    self.vals[v].hint
                } else if k == 0 {
                    match dst_loc {
                        Loc::G(d) if !x => d,
                        Loc::X(d) if x => d,
                        _ => self.vals[v].hint,
                    }
                } else {
                    self.vals[v].hint
                };
            if self.vals[v].reg != NOREG {
                let reg = self.vals[v].reg;
                self.vals[v].weak_only = false;
                self.cur.set(x, reg);
                locs[k] = Self::rloc(x, reg);
            } else if self.vals[v].slot_home {
                let s = self.any_slot(v).expect("home slot");
                let reg = match self.alloc_temp(x, excl, hint, true)? {
                    Some(r) => r,
                    None => return Err(RaError::Pressure),
                };
                self.cur.set(x, reg);
                self.out[i].pre.push(Reload {
                    reg: Self::rloc(x, reg),
                    slot: s,
                    r,
                });
                locs[k] = Self::rloc(x, reg);
            } else {
                let reg = match self.alloc_temp(x, excl, hint, true)? {
                    Some(r) => r,
                    None => return Err(RaError::Pressure),
                };
                self.take_reg(v, reg);
                self.vals[v].weak_only = false;
                self.cur.set(x, reg);
                locs[k] = Self::rloc(x, reg);
            }
        }
        self.out[i].ops = locs;
        // (c) the snapshot of a guard
        let mut snap: Option<u32> = None;
        if ins.flags & F_GUARD != 0 {
            let s = aux_snap(ins.aux);
            snap = Some(s);
            if self.snap_last[s as usize] == i {
                self.open_snap(s, dst_mask)?;
            }
        }
        // (d) helper call
        if let Some(mut m) = live_after {
            m.g &= !self.evicted.g;
            m.x &= !self.evicted.x;
            self.calls.push(CallSite {
                at: i,
                live: CallClobber { gpr: m.g, xmm: m.x },
            });
        }
        if self.reuse_slots {
            let d = std::mem::take(&mut self.deferred);
            self.free_slots.extend(d);
        } else {
            self.deferred.clear();
        }
        // (e) leaving the first guard of a snapshot: unpin
        if let Some(s) = snap {
            let su = s as usize;
            if self.snap_first[su] == i && self.snap_last[su] != i {
                for (_, v) in std::mem::take(&mut self.pinned_by[su]) {
                    self.vals[v].pins -= 1;
                }
                self.open.retain(|&o| o != s);
            }
        }
        Ok(())
    }

    /// Section 51 (`RAPOL_CALLSPILL`): every value that holds a register the helper call destroys and is live after it is evicted at the call
    /// (reloaded right after it from its spill slot, stored once at its definition), except loop homes (`fixed`) and values pinned by an open
    /// snapshot (their register is in an exit map on both sides of the call): those keep the save / reload of the call site.
    fn spill_across_call(&mut self) -> R<()> {
        for x in [false, true] {
            let (pool, np) = Self::pool_arr(x);
            for &r in &pool[..np] {
                if !x && usize::from(r) >= NGPR_CLOBBERED {
                    continue;
                }
                let h = self.holder(x, r);
                if h == NONE {
                    continue;
                }
                let v = h as usize;
                if self.vals[v].fixed || self.vals[v].pins > 0 {
                    continue;
                }
                if mutants::active("callspill_no_reload") {
                    // MUTANT: the value leaves the call's saved set without a reload behind the call
                    self.free_reg(v);
                    self.evicted.set(x, r);
                    continue;
                }
                self.evict(v, x, r)?;
            }
        }
        Ok(())
    }

    /// Fixes the exit map of snapshot `s` at its last guard.
    fn open_snap(&mut self, s: u32, dst_mask: Mask) -> R<()> {
        let ir = self.ir;
        let su = s as usize;
        let multi = self.snap_first[su] != self.snap_last[su];
        for (k, e) in ir.snap_entries(s).iter().enumerate() {
            let loc = if e.r == NOREF {
                Loc::None
            } else if e.r < REF_BIAS {
                Loc::Const(e.r)
            } else {
                let v = idx(e.r);
                let x = self.is_x[v];
                let mut loc = Loc::None;
                if self.vals[v].reg != NOREG {
                    loc = Self::rloc(x, self.vals[v].reg);
                    if multi && !self.vals[v].fixed {
                        self.vals[v].pins += 1;
                        self.pinned_by[su].push((k, v));
                    }
                } else if let Some(sl) = self.any_slot(v) {
                    loc = Loc::Spill(sl);
                } else {
                    let want_reg = !self.rnd(1, 2);
                    let mut excl = dst_mask;
                    excl.g |= self.cur.g;
                    excl.x |= self.cur.x;
                    if want_reg {
                        if let Some(r) = self.alloc_temp(x, excl, self.vals[v].hint, false)? {
                            self.take_reg(v, r);
                            self.vals[v].weak_only = true;
                            loc = Self::rloc(x, r);
                            if multi {
                                self.vals[v].pins += 1;
                                self.pinned_by[su].push((k, v));
                            }
                        }
                    }
                    if loc == Loc::None {
                        loc = Loc::Spill(self.ensure_slot(v)?);
                    }
                }
                loc
            };
            self.snap_locs[su][k] = loc;
        }
        if !self.pinned_by[su].is_empty() {
            self.open.push(s);
        }
        Ok(())
    }
}

/// A printable dump of an allocation next to the IR (for test failures and debugging).
pub fn dump(ir: &TraceIr, al: &Allocation) -> String {
    use std::fmt::Write;
    let mut s = String::new();
    let lname = |l: Loc| match l {
        Loc::None => "-".to_string(),
        Loc::Const(r) => format!("#{r}"),
        Loc::G(n) => format!("V{n}"),
        Loc::X(n) => format!("X{n}"),
        Loc::Spill(n) => format!("[{n}]"),
    };
    for (i, ins) in ir.ins.iter().enumerate() {
        let ia = &al.ins[i];
        let _ = write!(
            s,
            "{i:4} {:7} {:5} dst={} ops=[{} {} {}]",
            ins.op.name(),
            ins.t.name(),
            lname(ia.dst),
            lname(ia.ops[0]),
            lname(ia.ops[1]),
            lname(ia.ops[2]),
        );
        if al.fused[i] {
            let _ = write!(s, " fused");
        }
        for r in &ia.pre {
            let _ = write!(s, " pre:{}<-[{}]", lname(r.reg), r.slot);
        }
        for t in &ia.stores {
            let _ = write!(s, " store:[{t}]");
        }
        for r in &ia.post {
            let _ = write!(s, " post:{}<-[{}]", lname(r.reg), r.slot);
        }
        if i == al.backedge_after {
            let _ = write!(s, " (backedge moves before)");
        }
        s.push('\n');
    }
    for (k, m) in al.snap_locs.iter().enumerate() {
        let _ = writeln!(
            s,
            "snap {k}: {}",
            m.iter().map(|&l| lname(l)).collect::<Vec<_>>().join(" ")
        );
    }
    let mv = |v: &[Move]| {
        v.iter()
            .map(|m| format!("{}<-{}", lname(m.dst), lname(m.src)))
            .collect::<Vec<_>>()
            .join(" ")
    };
    let _ = writeln!(s, "entry: {}", mv(&al.entry_seq));
    let _ = writeln!(s, "backedge: {}", mv(&al.backedge_seq));
    s
}
