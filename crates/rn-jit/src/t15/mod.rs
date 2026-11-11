//! Tier 1.5 (typed loop regions, `docs/JIT_TIER15.md`): types shared by the region analysis.
//!
//! This module holds only *data*: the type lattice ([`TagSet`]), slot sets, the function description the
//! analysis reads ([`FuncInfo`]), the speculation inputs ([`Speculation`]), the tunables ([`Policy`]) and the
//! output ([`RegionPlan`]). The algorithms live in [`analysis`] (CFG, loops, liveness, fixpoint), the per-opcode
//! tables (def/use sets, transfer functions, instruction classes) in [`effects`] and the textual dump in
//! [`print`]. Nothing here touches VM state, so it all runs under Miri.
//!
//! # Decisions where `docs/JIT_TIER15.md` is ambiguous or was amended (read before changing a rule)
//!
//! * **Amendments applied** (orchestrator message after the Opus review of the design): (B5) the guard set of a
//!   stub is the *upward-exposed typed use set* ([`InsnPlan::guard_live_in`], liveness where every region exit
//!   has an empty live-out), not the live-out of the exits; write-back of dirty dead values is a separate set
//!   ([`RegionPlan::writeback_all`], R-EXACT: all slots below `max_stack`). (M1) [`InsnPlan::mem_known`]: slots
//!   whose VM-stack tag byte is known to equal the fact; reset at every barrier op for every slot the next stub
//!   does not re-guard. (M2) class-N fallbacks define `Any`, or the static result type with a *mandatory* guard
//!   ([`InsnPlan::guard_def`]) when [`Policy::guard_n_defs`]. (M3) [`RegionPlan::has_setupval`] and escaped
//!   slots are re-guarded after every barrier ([`EscapedMode::Reguard`], default). (A6) `FORLOOP` reads its
//!   control slot regardless of tag, so that slot is in the use set and hence in the guard set after barriers.
//!   (M9) follower pcs (`JMP` after a compare, `MMBIN*`, `EXTRAARG`) are marked [`Role`]; [`RegionPlan::insn`]
//!   refuses them and they are never resume points.
//! * **5.5 generic-`for` layout follows the VM, not the doc**: after `TFORPREP` the registers are `A` iterator,
//!   `A+1` state, `A+2` closing, `A+3` control/first variable; `TFORCALL A C` reads `A, A+1, A+3`, writes
//!   `A+3 ..= A+2+C` and uses `A+4, A+5` as argument scratch (the doc's "A+4..A+3+C" is the 5.4 layout).
//! * **Followers**: `ADD..` + `MMBIN*`, `LOADKX/NEWTABLE/SETLIST k` + `EXTRAARG`, compare/`TEST` + `JMP` are
//!   subsumed by the leading instruction exactly as in tier 1: the CFG successor of an arithmetic op is `pc+2`
//!   (the metamethod path is a tier-1 matter: region code exits there), a compare's successors are the `JMP`
//!   target and `pc+2`.
//! * **Not-a-number operands**: arithmetic whose operand types make every template exit (strings, tables ...) yields
//!   the empty set (bottom): the region never continues past it; consumers treat an empty operand as
//!   [`Disp::Dead`]. Soundness argument: facts only describe executions that stay in region code; everything else
//!   re-enters through a guarded stub. **Resume points are entry edges (T2b, docs section 14)**: the head, inner
//!   loop heads and the continuation of every barrier op can be entered from outside the region, checking only
//!   [`RegionPlan::stub_guards`]; the facts of their guard set are therefore singleton or `Any`, `mem_known` is
//!   cut to the checked set, and a typed instruction with a bottom operand is `Dead`. Facts of slots outside the
//!   guard set are unreliable (never consumed before a later stub re-guards them). `SETUPVAL` may write any
//!   escaped slot (`debug.upvaluejoin`); a table access that falls to the tier-1 template is
//!   [`Disp::LocalBarrier`], a barrier op. The VM-truth check of all this is `tests/t15_oracle.rs`.
//! * **Tier-1 block heads**: analysis blocks use the leaders listed in `analysis::leaders`. Tier 1 additionally
//!   starts a block after any exit stub (data dependent); pass those through [`FuncInfo::extra_heads`] so the
//!   region blocks are exactly tier 1's (C1). Analysis leaders are a subset of tier 1's heads otherwise.
//! * **Entry facts at the region head**: only singleton or `Any` (non-singleton joins are widened to `Any`), so a
//!   stub guard is one compare. Peel: a slot whose back-edge type is a singleton different from the observed tag
//!   gets the back-edge type and is flagged `peel` (once per slot; a second disagreement widens to `Any`).
//! * **Step speculation (values)** is not computed (needs frame payloads, the inputs are tags); only statically
//!   known steps (`LOADI/LOADK/LOADF` right before `FORPREP`) are reported in [`ForInfo::step_const`].
//! * **Intrinsics (5.2)** and `k(s)` constant facts are not modelled (T6).

pub mod alloc;
pub mod analysis;
pub mod codegen;
pub mod effects;
pub mod intrinsics;
pub mod mutants;
pub mod print;
pub mod runtime;
pub mod stubs;
pub mod verify;

use rn_core::protodata::{Const, ProtoData};
use rn_core::value::{
    ctb, LUA_VABSTKEY, LUA_VCCL, LUA_VEMPTY, LUA_VFALSE, LUA_VLCF, LUA_VLCFC, LUA_VLCL,
    LUA_VLIGHTUSERDATA, LUA_VLNGSTR, LUA_VNIL, LUA_VNOTABLE, LUA_VNUMFLT, LUA_VNUMINT, LUA_VSHRSTR,
    LUA_VTABLE, LUA_VTHREAD, LUA_VTRUE, LUA_VUSERDATA,
};

/// Bitmask over tag classes: bit 0 `NIL` ... bit 13 `THREAD`. `0` is bottom (unreached), join is OR,
/// [`TagSet::ANY`] is top. One bit per exact tag byte class the VM uses (the collectable bit is ignored).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Default, Debug)]
pub struct TagSet(pub u16);

impl TagSet {
    /// Bottom.
    pub const EMPTY: TagSet = TagSet(0);
    /// `nil` (every nil variant).
    pub const NIL: TagSet = TagSet(1 << 0);
    /// `false`.
    pub const FALSE: TagSet = TagSet(1 << 1);
    /// `true`.
    pub const TRUE: TagSet = TagSet(1 << 2);
    /// Integer.
    pub const INT: TagSet = TagSet(1 << 3);
    /// Float.
    pub const FLT: TagSet = TagSet(1 << 4);
    /// Short string.
    pub const SHRSTR: TagSet = TagSet(1 << 5);
    /// Long string.
    pub const LNGSTR: TagSet = TagSet(1 << 6);
    /// Table.
    pub const TABLE: TagSet = TagSet(1 << 7);
    /// Lua closure.
    pub const LCL: TagSet = TagSet(1 << 8);
    /// Light C function (both variants).
    pub const LCF: TagSet = TagSet(1 << 9);
    /// C closure.
    pub const CCL: TagSet = TagSet(1 << 10);
    /// Full userdata.
    pub const UDATA: TagSet = TagSet(1 << 11);
    /// Light userdata.
    pub const LUDATA: TagSet = TagSet(1 << 12);
    /// Thread.
    pub const THREAD: TagSet = TagSet(1 << 13);
    /// `FALSE | TRUE`.
    pub const BOOL: TagSet = TagSet(Self::FALSE.0 | Self::TRUE.0);
    /// `INT | FLT`.
    pub const NUM: TagSet = TagSet(Self::INT.0 | Self::FLT.0);
    /// Top.
    pub const ANY: TagSet = TagSet((1 << 14) - 1);

    /// Names of the 14 classes, by bit.
    pub const NAMES: [&'static str; 14] = [
        "nil", "false", "true", "int", "flt", "shrstr", "lngstr", "table", "lcl", "lcf", "ccl",
        "udata", "ludata", "thread",
    ];

    /// The class of an exact tag byte as stored in a slot (collectable bit ignored). Unknown bytes map to
    /// `ANY` (never a wrong singleton).
    pub const fn of_tag(tag: u8) -> TagSet {
        match tag & !rn_core::value::BIT_ISCOLLECTABLE {
            LUA_VNIL | LUA_VEMPTY | LUA_VABSTKEY | LUA_VNOTABLE => Self::NIL,
            LUA_VFALSE => Self::FALSE,
            LUA_VTRUE => Self::TRUE,
            LUA_VNUMINT => Self::INT,
            LUA_VNUMFLT => Self::FLT,
            LUA_VSHRSTR => Self::SHRSTR,
            LUA_VLNGSTR => Self::LNGSTR,
            LUA_VTABLE => Self::TABLE,
            LUA_VLCL => Self::LCL,
            LUA_VLCF | LUA_VLCFC => Self::LCF,
            LUA_VCCL => Self::CCL,
            LUA_VUSERDATA => Self::UDATA,
            LUA_VLIGHTUSERDATA => Self::LUDATA,
            LUA_VTHREAD => Self::THREAD,
            _ => Self::ANY,
        }
    }
    /// Join (least upper bound).
    pub const fn join(self, o: TagSet) -> TagSet {
        TagSet(self.0 | o.0)
    }
    /// Meet.
    pub const fn meet(self, o: TagSet) -> TagSet {
        TagSet(self.0 & o.0)
    }
    /// `self` without the bits of `o`.
    pub const fn minus(self, o: TagSet) -> TagSet {
        TagSet(self.0 & !o.0)
    }
    /// Lattice order.
    pub const fn subset(self, o: TagSet) -> bool {
        self.0 & !o.0 == 0
    }
    /// Bottom?
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }
    /// Exactly one class?
    pub const fn is_singleton(self) -> bool {
        self.0 != 0 && self.0 & (self.0 - 1) == 0
    }
    /// Top?
    pub const fn is_any(self) -> bool {
        self.0 == Self::ANY.0
    }
    /// The slot tag byte of a singleton set (collectables with the collectable bit), as the VM stores it.
    pub fn tag_byte(self) -> Option<u8> {
        Some(match self {
            Self::NIL => LUA_VNIL,
            Self::FALSE => LUA_VFALSE,
            Self::TRUE => LUA_VTRUE,
            Self::INT => LUA_VNUMINT,
            Self::FLT => LUA_VNUMFLT,
            Self::SHRSTR => ctb(LUA_VSHRSTR),
            Self::LNGSTR => ctb(LUA_VLNGSTR),
            Self::TABLE => ctb(LUA_VTABLE),
            Self::LCL => ctb(LUA_VLCL),
            Self::LCF => LUA_VLCF,
            Self::CCL => ctb(LUA_VCCL),
            Self::UDATA => ctb(LUA_VUSERDATA),
            Self::LUDATA => LUA_VLIGHTUSERDATA,
            Self::THREAD => ctb(LUA_VTHREAD),
            _ => return None,
        })
    }
    /// Is every member a collectable (GC object) class? (Write-through values, R-G1.)
    pub const fn is_collectable(self) -> bool {
        self.0 != 0
            && self.subset(TagSet(
                Self::SHRSTR.0
                    | Self::LNGSTR.0
                    | Self::TABLE.0
                    | Self::LCL.0
                    | Self::CCL.0
                    | Self::UDATA.0
                    | Self::THREAD.0,
            ))
    }
    /// Iterate the single-class sets contained in `self`.
    pub fn bits(self) -> impl Iterator<Item = TagSet> {
        (0..14).filter_map(move |b| (self.0 >> b & 1 != 0).then_some(TagSet(1 << b)))
    }
}

impl std::fmt::Display for TagSet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {
            Self::EMPTY => f.write_str("bot"),
            Self::ANY => f.write_str("any"),
            Self::NUM => f.write_str("num"),
            Self::BOOL => f.write_str("bool"),
            s => {
                let mut first = true;
                for (b, n) in Self::NAMES.iter().enumerate() {
                    if s.0 >> b & 1 != 0 {
                        if !first {
                            f.write_str("|")?;
                        }
                        first = false;
                        f.write_str(n)?;
                    }
                }
                Ok(())
            }
        }
    }
}

/// A set of Lua registers (the frame has at most 256).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Default, Debug)]
pub struct SlotSet(pub [u64; 4]);

impl SlotSet {
    /// The empty set.
    pub const EMPTY: SlotSet = SlotSet([0; 4]);
    /// Slots `0..n` (`n <= 256`).
    pub fn first(n: u32) -> SlotSet {
        let mut s = SlotSet::EMPTY;
        s.insert_range(0, n);
        s
    }
    /// Add one slot.
    pub fn insert(&mut self, s: u32) {
        self.0[(s >> 6) as usize & 3] |= 1 << (s & 63);
    }
    /// Remove one slot.
    pub fn remove(&mut self, s: u32) {
        self.0[(s >> 6) as usize & 3] &= !(1 << (s & 63));
    }
    /// Membership.
    pub fn contains(&self, s: u32) -> bool {
        s < 256 && self.0[(s >> 6) as usize] >> (s & 63) & 1 != 0
    }
    /// Add `lo..hi` (clamped to 256).
    pub fn insert_range(&mut self, lo: u32, hi: u32) {
        for s in lo..hi.min(256) {
            self.insert(s);
        }
    }
    /// Union; true if `self` grew.
    pub fn union_with(&mut self, o: &SlotSet) -> bool {
        let old = *self;
        for i in 0..4 {
            self.0[i] |= o.0[i];
        }
        *self != old
    }
    /// `self` without `o`.
    pub fn minus(&self, o: &SlotSet) -> SlotSet {
        let mut r = *self;
        for i in 0..4 {
            r.0[i] &= !o.0[i];
        }
        r
    }
    /// Intersection.
    pub fn meet(&self, o: &SlotSet) -> SlotSet {
        let mut r = *self;
        for i in 0..4 {
            r.0[i] &= o.0[i];
        }
        r
    }
    /// Empty?
    pub fn is_empty(&self) -> bool {
        self.0 == [0; 4]
    }
    /// Members in increasing order.
    pub fn iter(&self) -> impl Iterator<Item = u32> + '_ {
        (0..256u32).filter(move |&s| self.contains(s))
    }
}

/// How an instruction's pc relates to the instruction before it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Role {
    /// A real instruction.
    Normal,
    /// `MMBIN*` after an arithmetic op: metamethod slow path, executed by the interpreter only.
    Mmbin,
    /// `EXTRAARG` after `LOADKX/NEWTABLE/SETLIST`.
    Extra,
    /// The `JMP` after a compare/`TEST`/`TESTSET`, subsumed by it.
    JmpFollower,
}

impl Role {
    /// Followers are dead continuation targets (M9): nothing may jump or resume there.
    pub fn is_follower(self) -> bool {
        self != Role::Normal
    }
}

/// Static class of an instruction inside a region (doc 3.6).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum InsClass {
    /// Register templates (arithmetic, compare, TEST, MOVE/LOAD, JMP, FORPREP/FORLOOP, GETUPVAL ...).
    Typed,
    /// Table access with a typed form (section 5); its slow form is a fallback.
    Table,
    /// Fallback N: a helper that runs no Lua code and allocates nothing (`jit_get`, `jit_len`, `jit_pow`).
    FallbackN,
    /// Fallback G: the helper may allocate / run Lua code (a barrier op).
    FallbackG,
    /// Call, tail call, return, `TFORCALL` (a barrier op; frame leaving ones have no successor).
    Call,
    /// A `CALL` of a `math` builtin seen at warm-up ([`Policy::intr`]): the inline form is not a barrier (its slow
    /// path is the tier-1 `CALL` template, entered with the guards of `stub(P+1)` behind it); when the operand
    /// facts do not allow it the instruction is a [`Disp::LocalBarrier`] and behaves exactly like [`InsClass::Call`].
    Intr,
    /// Always leaves the region (tier-1 stub: `LOADKX`, `GETVARG`, `ERRNNIL`, `VARARGPREP` ...).
    Exit,
    /// A follower (see [`Role`]); never executed by region code.
    Follower,
}

impl InsClass {
    /// Barrier op: Lua code, GC, `debug.setlocal`, rehash ... may run inside it.
    pub fn is_barrier(self) -> bool {
        matches!(self, InsClass::FallbackG | InsClass::Call)
    }
}

/// How the instruction can be compiled given the facts at it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Disp {
    /// All type-relevant operands are singletons: no guards, single path.
    Specialised,
    /// Operands within `INT|FLT` but not all singletons: dual-path code.
    Dual,
    /// Operand types too weak: local fallback to the tier-1 template (or the class's own fallback).
    Local,
    /// An operand or the result is bottom: region code always exits here.
    Dead,
    /// No typed form exists for this class (G, Call, Exit, N).
    Opaque,
    /// A table access whose operands are not typed enough for the inline form: the tier-1 template runs, and
    /// its cold paths call `jit_get` / `jit_set` (insertion, rehash, GC step, finalizers, `__index` chains).
    /// Unlike [`Disp::Local`] it is **not** call-free: T4 treats the instruction exactly like a class-G
    /// barrier op (flush everything, template, reload through `stub(P+1)`; [`InsnPlan::barrier`] is set and
    /// `P+1` is a resume point). Review finding 5 / B1.
    LocalBarrier,
}

/// Where the closing of an upvalue-captured (escaped) slot's facts goes at a barrier op.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EscapedMode {
    /// Keep the fact, `stub(P+1)` re-guards it (R-REGUARD, the amended design; default).
    Reguard,
    /// The fact becomes `Any` at every barrier (the original doc 3.4 wording).
    ToAny,
}

/// Tunables of the analysis (nothing is hard-coded elsewhere).
#[derive(Clone, Debug)]
pub struct Policy {
    /// Largest region in instructions (`[H, B]` inclusive; doc: 512).
    pub max_region: usize,
    /// Table-access pcs that must use the in-region fallback protocol (class G, barrier) instead of the
    /// typed form (miss counter reached its limit, doc 3.8).
    pub fallback_g: Vec<u32>,
    /// Class-N fallbacks define their static result type behind a mandatory guard (`true`) or `Any` (`false`).
    pub guard_n_defs: bool,
    /// Treatment of escaped slots at barriers.
    pub escaped: EscapedMode,
    /// Refuse regions containing `GETVARG` / `ERRNNIL` (named varargs).
    pub refuse_named_vararg: bool,
    /// Extra sweeps of the dataflow beyond the lattice-height bound (safety margin only).
    pub extra_sweeps: usize,
    /// `CALL` sites that may use the inline form of a `math` builtin ([`intrinsics`]).
    pub intr: Vec<intrinsics::Site>,
}

impl Default for Policy {
    fn default() -> Self {
        Policy {
            max_region: 512,
            fallback_g: Vec::new(),
            guard_n_defs: true,
            escaped: EscapedMode::Reguard,
            refuse_named_vararg: true,
            extra_sweeps: 8,
            intr: Vec::new(),
        }
    }
}

/// What the analysis needs to know about a prototype (no VM state).
#[derive(Clone, Debug)]
pub struct FuncInfo<'a> {
    /// The bytecode.
    pub code: &'a [u32],
    /// Tag class of every constant.
    pub k: Vec<TagSet>,
    /// Value of integer constants (`None` for the others); used for statically known loop steps.
    pub k_int: Vec<Option<i64>>,
    /// `maxstacksize` (slots `>=` this never appear).
    pub max_stack: u32,
    /// Slots captured by some `CLOSURE` of the function (open upvalues point at them).
    pub escaped: SlotSet,
    /// Additional block heads of the tier-1 lowering (`lower()`'s `head[]`); empty = none.
    pub extra_heads: Vec<bool>,
}

/// Longest short string (`rn_core::string::MAX_SHORT_LEN`).
pub const MAX_SHORT_LEN: usize = rn_core::string::MAX_SHORT_LEN;

impl<'a> FuncInfo<'a> {
    /// From a compiled prototype; `str_len` gives the length of a string constant.
    pub fn from_proto<S>(p: &'a ProtoData<S>, str_len: impl Fn(&S) -> usize) -> FuncInfo<'a> {
        let k =
            p.k.iter()
                .map(|c| match c {
                    Const::Nil => TagSet::NIL,
                    Const::False => TagSet::FALSE,
                    Const::True => TagSet::TRUE,
                    Const::Int(_) => TagSet::INT,
                    Const::Float(_) => TagSet::FLT,
                    Const::Str(s) if str_len(s) <= MAX_SHORT_LEN => TagSet::SHRSTR,
                    Const::Str(_) => TagSet::LNGSTR,
                })
                .collect();
        let k_int =
            p.k.iter()
                .map(|c| {
                    if let Const::Int(v) = c {
                        Some(*v)
                    } else {
                        None
                    }
                })
                .collect();
        let mut escaped = SlotSet::EMPTY;
        for c in &p.protos {
            for u in c.upvalues.iter().filter(|u| u.in_stack) {
                escaped.insert(u32::from(u.idx));
            }
        }
        FuncInfo {
            code: &p.code,
            k,
            k_int,
            max_stack: u32::from(p.max_stack_size),
            escaped,
            extra_heads: Vec::new(),
        }
    }
}

/// Speculation inputs (doc 3.3): frame snapshot tags and per-pc profile tags, plus respecialisation widenings.
#[derive(Clone, Copy, Debug, Default)]
pub struct Speculation<'a> {
    /// Tag byte of every slot at promotion; missing or `0xFF` entries are unknown (`Any`).
    pub obs: &'a [u8],
    /// Last observed result tag per pc (`0xFF` = never executed -> `Any`).
    pub prof: &'a [u8],
    /// Entry-fact widenings from exit feedback: `(slot, set)` joined into the entry fact.
    pub entry_widen: &'a [(u8, TagSet)],
    /// Def-site widenings: `(pc, set)` joined into the speculated result of the load at `pc`.
    pub def_widen: &'a [(u32, TagSet)],
}

/// A natural loop inside the region.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RegionLoop {
    /// Head pc.
    pub head: u32,
    /// Back-edge instruction pc (last latch).
    pub end: u32,
    /// Nesting depth relative to the region (the region's own loop is 0).
    pub depth: u32,
    /// Index of the parent in [`RegionPlan::loops`].
    pub parent: Option<usize>,
}

/// A basic block inside the region.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RegionBlock {
    /// First pc.
    pub start: u32,
    /// One past the last pc (followers included).
    pub end: u32,
    /// Successor pcs (block starts) that are inside the region.
    pub succs: Vec<u32>,
    /// Loop depth of the block (0 = outside every loop of the region, i.e. only the region loop itself is 1).
    pub depth: u32,
}

/// A speculated fact at the region head (see [`EntryFact::guarded`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EntryFact {
    /// Slot.
    pub slot: u32,
    /// Singleton set to check.
    pub set: TagSet,
    /// The type differs from the observed one: the first entry from tier 1 fails by design (not counted).
    pub peel: bool,
    /// Checked by `stub(H)` (upward-exposed before the first barrier). Unguarded facts are speculation that the
    /// stub after the first barrier verifies before any use.
    pub guarded: bool,
}

/// A numeric `for` loop inside the region.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ForInfo {
    /// `FORPREP` pc.
    pub pc: u32,
    /// Base register `A`.
    pub a: u32,
    /// Type of the control variable after `FORPREP`: `INT`, `FLT` or `NUM`.
    pub ctl: TagSet,
    /// The step if it is a compile-time integer constant loaded right before the loop.
    pub step_const: Option<i64>,
}

/// Per-instruction result.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InsnPlan {
    /// pc.
    pub pc: u32,
    /// Follower role (M9).
    pub role: Role,
    /// Static class (after [`Policy::fallback_g`]).
    pub class: InsClass,
    /// Compilation mode given the facts.
    pub disp: Disp,
    /// Was the instruction reached by the region fixpoint? (Unreached: facts are bottom.)
    pub reached: bool,
    /// Barrier op (class G or Call).
    pub barrier: bool,
    /// Registers read (types of those that matter are in the facts).
    pub uses: SlotSet,
    /// Registers written for sure.
    pub defs: SlotSet,
    /// Registers possibly written (no kill) or garbage afterwards (calls).
    pub may_defs: SlotSet,
    /// The result is speculated or comes out of a fallback: region code must guard the def against this set
    /// (before committing it, or right after the fallback) and the facts after the pc assume it.
    pub guard_def: Option<TagSet>,
    /// Upward-exposed typed uses from here to the region's exits (B5): what a stub at this pc must guard.
    pub guard_live_in: SlotSet,
    /// Slots whose VM-stack tag byte is known to equal their fact before this instruction (M1).
    pub mem_known: SlotSet,
}

/// The analysis result for one region (types only, no code).
#[derive(Clone, Debug)]
pub struct RegionPlan {
    /// Head pc `H`.
    pub head: u32,
    /// Back-edge instruction pc `B` (`[H, B]` inclusive).
    pub end: u32,
    /// Number of tracked slots (`max_stack`).
    pub nslots: u32,
    /// Loops of the nest, outermost first (index 0 = the region loop).
    pub loops: Vec<RegionLoop>,
    /// Reachable blocks of the region, in pc order.
    pub blocks: Vec<RegionBlock>,
    /// `(source pc, target pc)` edges leaving the region.
    pub exits: Vec<(u32, u32)>,
    /// Entry speculation at `H`: singleton facts of the slots region code reads (only the `guarded` ones are
    /// checked by `stub(H)`).
    pub entry: Vec<EntryFact>,
    /// One entry per pc in `[H, B]`.
    pub insns: Vec<InsnPlan>,
    /// Slots written by some instruction of the region (including clobbers).
    pub written: SlotSet,
    /// Captured by a closure of the function (write-through, always re-guarded after barriers).
    pub escaped: SlotSet,
    /// Not written in the region and not escaped.
    pub invariant: SlotSet,
    /// pcs that get a resume stub: the head, inner loop heads, the pc after every barrier op that falls through.
    pub resume_points: Vec<u32>,
    /// Slots to write back at any exit: all of `0..nslots` (R-EXACT, no dead-slot pruning).
    pub writeback_all: SlotSet,
    /// Does the region contain a `SETUPVAL`? (Upvalue hoisting is allowed only if not, M3.)
    pub has_setupval: bool,
    /// The intrinsic call sites of the region ([`Policy::intr`] restricted to `[H, B]`).
    pub intr: Vec<intrinsics::Site>,
    /// Numeric `for` loops.
    pub fors: Vec<ForInfo>,
    /// Dataflow sweeps of the last solve and number of peel/widen rounds (diagnostics).
    pub stats: PlanStats,
    pub(crate) facts: Vec<TagSet>,
}

/// Diagnostics of a plan computation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PlanStats {
    /// Entry-fixpoint rounds (peel / widen).
    pub rounds: u32,
    /// Largest number of sweeps of any solve.
    pub sweeps: u32,
}

impl RegionPlan {
    /// The instruction plan at `pc`, or `None` if `pc` is outside the region or a follower (M9).
    pub fn insn(&self, pc: u32) -> Option<&InsnPlan> {
        let i = self.insns.get(pc.checked_sub(self.head)? as usize)?;
        (!i.role.is_follower()).then_some(i)
    }
    /// Fact of `slot` before `pc` (bottom outside the region, in unreached code and for followers' own pc).
    pub fn fact(&self, pc: u32, slot: u32) -> TagSet {
        if pc < self.head || pc > self.end || slot >= self.nslots {
            return TagSet::EMPTY;
        }
        self.facts[(pc - self.head) as usize * self.nslots as usize + slot as usize]
    }
    /// The guards a stub at `pc` needs: upward-exposed slots with a singleton fact (B5).
    pub fn stub_guards(&self, pc: u32) -> Vec<(u32, TagSet)> {
        let Some(i) = self.insn(pc) else {
            return Vec::new();
        };
        i.guard_live_in
            .iter()
            .filter_map(|s| {
                let f = self.fact(pc, s);
                f.is_singleton().then_some((s, f))
            })
            .collect()
    }
}
