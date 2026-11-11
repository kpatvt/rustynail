//! Region code generator, integer-only slice (task T4a; docs/JIT_TIER15.md section 4, as built: section 16).
//!
//! # Shape of the generated code
//!
//! A region is appended to the tier-1 program of the prototype (same `Program`, same labels): tier-1 code for every
//! pc stays as the deopt target, region code is entered only through `stub(P)` (`stubs.rs`). Every Lua register
//! is either **register-resident** (`RR`: its value lives in the machine register [`Assign`] gave it, the VM stack
//! slot is stale) or **slot-resident** (the slot is exact). `RR(pc)` is a pure function of the code
//! (`alloc::solve_rr`), so every edge knows exactly which slots to write back.
//!
//! Per instruction ([`Act`]):
//! * integer templates (`MOVE LOADI LOADK ADD SUB MUL ADDI ADDK SUBK MULK BAND BOR BXOR (+K) SHRI UNM BNOT`,
//!   `EQ LT LE EQI LTI LEI GTI GEI EQK TEST JMP FORLOOP`) work on registers and need no guards: the operand facts are
//!   singletons established by `stub` guards or by an earlier template;
//! * pure memory definitions (`LOADNIL LOADTRUE LOADFALSE LOADF LOADK MOVE` of non-integers) write the slot directly;
//! * **everything else is a fallback** (docs 4.8): flush all, the tier-1 template verbatim (its `pcl` targets
//!   remapped to the stubs of the successors), `stub(next)` (guards, reloads). The region therefore compiles any
//!   nest.
//!
//! Counting (C1): `Sub r11, cnt_at; js cexit` at the region label of every tier-1 block head; `r11` is stored to
//! `[r15]` at every flush and exit and reloaded by every stub.

use super::alloc::{self, Assign, Node};
use super::intrinsics::Intr;
use super::runtime::OptFlags;
use super::{RegionPlan, SlotSet, TagSet};
use crate::ir::{Base, Cc, FOp, Op, Src, L, M, V, W, X};
use crate::lower::{fr, Cx, Kind, RegionEnv, RegionOut};
use crate::t15::stubs::Pad;
use rn_core::instruction::Instruction;
use rn_core::value::{LUA_VFALSE, LUA_VNIL, LUA_VNUMFLT, LUA_VNUMINT, LUA_VTRUE};
use rn_core::ver::active::OpCode;
use std::collections::{HashMap, HashSet};

mod calls;
mod shadow;
mod tables;
pub use calls::jit_t15_math;
use calls::{URes, IK};
use tables::{TKey, TRes, TVal};

const TAG_INT: i32 = LUA_VNUMINT as i32;
/// Cost of a fallback `CALL` in the profitability gate, in units of an ordinary fallback.
const CALL_FALL_WEIGHT: u64 = 4;

thread_local! {
    static REGIONS: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

/// Regions emitted on this thread since the last call (statistics: `Stats::t15_regions`).
pub fn take_regions_emitted() -> u32 {
    REGIONS.with(std::cell::Cell::take)
}
const TAG_FALSE: i32 = LUA_VFALSE as i32;
/// Scratch registers of region code: `rax`, `rdx`, `rcx`.
const S0: V = 7;
const S1: V = 8;
const S2: V = 6;

/// Integer operand: a Lua register or an immediate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Opd {
    R(u32),
    I(i64),
}

/// Operand of a float operation (T4b): the value as a double.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum FOpd {
    /// A slot whose fact is `FLT`.
    Flt(u32),
    /// A slot whose fact is `INT` (converted with `cvtsi2sd`, exactly `(double)i`).
    Int(u32),
    /// A float constant of the constants array (memory operand).
    Kf(u32),
    /// A constant given by its bit pattern (`(double)` of an integer constant or immediate).
    C(u64),
}

/// Integer binary operations with a register destination.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum AOp {
    Add,
    Sub,
    Mul,
    And,
    Or,
    Xor,
}

/// What region code does for one instruction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Act {
    /// Nothing is emitted (unreached by the analysis, a follower, or not reachable in the register dataflow).
    Dead,
    /// The tier-1 template between a flush and the successors' stubs.
    Fall,
    LoadI(u32, i64),
    MoveI(u32, u32),
    Bin {
        a: u32,
        op: AOp,
        x: u32,
        y: Opd,
    },
    Neg(u32, u32),
    /// `a = b % c` (`is_mod`) or `a = b // c` for a constant integer divisor `c` (not 0, not -1): floor semantics.
    DivK {
        a: u32,
        b: u32,
        c: i64,
        is_mod: bool,
    },
    Not(u32, u32),
    /// `a = b << n` for `n >= 0`, `a = b >>> -n` (logical) for `n < 0`; `|n| >= 64` gives 0.
    Shift {
        a: u32,
        b: u32,
        n: i64,
    },
    /// A one-slot constant of the given tag (payload 0): `LOADFALSE LOADTRUE`.
    MemConst(u32, u8),
    LoadNil(u32, u32),
    LoadK(u32, u32),
    MoveM(u32, u32),
    Jmp {
        t: u32,
        poll: bool,
    },
    Cmp {
        cc: Cc,
        x: u32,
        y: Opd,
        k: bool,
        jt: u32,
        skip: u32,
        poll: bool,
    },
    Test {
        a: u32,
        k: bool,
        jt: u32,
        skip: u32,
        poll: bool,
        st: Option<bool>,
    },
    ForLoop {
        a: u32,
        t: u32,
        fall: u32,
    },
    /// `R[a] = <double constant>` (`LOADF`, `LOADK` of a float): bits are kept exactly.
    LoadFc(u32, u64),
    /// `R[a] = R[b]`, both `FLT`.
    MoveF(u32, u32),
    /// `R[a] = x <op> y` in doubles (`ADD SUB MUL DIV` and their `I`/`K` forms with at least one float operand,
    /// or `DIV` of anything numeric).
    FBin {
        a: u32,
        op: FOp,
        x: FOpd,
        y: FOpd,
    },
    /// `R[a] = -R[b]`, `FLT`.
    FNeg(u32, u32),
    /// Float comparison and branch; `Eq`/`Lt`/`Le`/`Gt`/`Ge`, NaN compares false.
    FCmp {
        cc: Cc,
        x: FOpd,
        y: FOpd,
        k: bool,
        jt: u32,
        skip: u32,
        poll: bool,
    },
    /// Float `FORLOOP` (all of `a`, `a+1`, `a+2` are `FLT`: limit, step, index).
    FForLoop {
        a: u32,
        t: u32,
        fall: u32,
    },
    /// Inline array read `R[a] = R[t][key]` (T5, `tables.rs`); misses take the instruction's slow path.
    GetArr {
        a: u32,
        t: u32,
        tp: bool,
        key: TKey,
        res: TRes,
    },
    /// Inline array write `R[t][key] = val`.
    SetArr {
        t: u32,
        tp: bool,
        key: TKey,
        val: TVal,
    },
    /// Inline hash read of the constant short string `K[kx]`.
    GetFld {
        a: u32,
        t: u32,
        tp: bool,
        kx: u32,
        res: TRes,
    },
    /// Inline hash write of an existing field.
    SetFld {
        t: u32,
        tp: bool,
        kx: u32,
        val: TVal,
    },
    /// Inline `math` builtin call (T6, `calls.rs`): result slot `a` (the callee slot), arguments `a+1`, `a+2`.
    Intr {
        kind: Intr,
        a: u32,
        ac: IK,
        rc: IK,
        addr: usize,
    },
    /// `sin cos exp log`: guards, flush, libm helper, continue in the stub of the next instruction.
    IntrH {
        kind: Intr,
        a: u32,
    },
    /// Inline `GETUPVAL` (T6).
    GetUp {
        a: u32,
        b: u32,
        res: URes,
    },
}

/// Register effects of an act before the assignment (see [`Rg::raw_effects`]).
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Raw {
    pub iuse: SlotSet,
    pub idef: SlotSet,
    pub fuse: SlotSet,
    pub fdef: SlotSet,
    pub mdef: SlotSet,
}

/// The region generator for one [`RegionPlan`].
pub(super) struct Rg<'c, 'a> {
    pub cx: &'c mut Cx<'a>,
    pub env: &'c RegionEnv<'c>,
    pub plan: &'c RegionPlan,
    pub opt: &'c OptFlags,
    pub asg: Assign,
    pub acts: Vec<Act>,
    /// `RR` at the start of every pc (index `pc - head`); `None` = not reached.
    pub rr_in: Vec<Option<SlotSet>>,
    pub reg_l: Vec<L>,
    pub stub_l: Vec<L>,
    pub need_stub: HashSet<u32>,
    pub rr: SlotSet,
    pub pads: Vec<Pad>,
    pub pad_map: HashMap<(u32, [u64; 4], bool), L>,
    pub cexit_map: HashMap<u32, (L, u32)>,
    /// Set when the generator met a shape it cannot handle: the whole region is dropped (tier 1 keeps running the
    /// nest) and the reason is reported by `RUSTYNAIL_JIT_VERBOSE`. Never a panic.
    pub refusal: Option<String>,
    /// `+verify`: pc -> label of the type-knowledge check in front of tier 1's code of `pc`.
    pub chk_map: HashMap<u32, L>,
    /// Successors of fallbacks (the pcs entered through a stub after a tier-1 template ran).
    pub fall_succ: HashSet<u32>,
    /// Out-of-line slow paths of inline table accesses: (label, pc, register-resident set at the jump).
    pub slows: Vec<(L, u32, SlotSet)>,
    /// Slow-path labels whose entry counts a demotable failure (callee identity, stack room, spare `CallInfo`); hook,
    /// interrupt and representability misses use plain slow paths (T7 after T6 review).
    pub slow_counted: Vec<L>,
    /// Tier 1.5 policy: failure-pad addresses (`None` with `-respec`: stubs fall straight into tier 1).
    pub fb: Option<super::runtime::FeedbackSpec>,
    /// The stubs emitted with failure accounting.
    pub sites: Vec<super::runtime::StubSite>,
    /// `+exitstorm`: out-of-line exits `(label, register-resident set, tier-1 pc)` taken by pseudo-guards.
    pub storm_pads: Vec<(L, SlotSet, u32)>,
    /// `+exitstorm`: the cache of `storm_pads` by `(set, pc)`.
    pub storm_map: HashMap<([u64; 4], u32), L>,
    /// `+exitstorm`: pc -> region entry label behind the pseudo-guard of the instruction (taken by stubs).
    pub storm_entry: HashMap<u32, L>,
    /// `+shadow`: per pc of the region the id of the verified block it belongs to.
    pub vblock_of: Vec<Option<u32>>,
    /// `+shadow`: the block whose instructions are being emitted.
    pub vcur: Option<u32>,
    /// `+shadow`: out-of-line verification sequences on the exits of verified blocks.
    pub vpads: Vec<VPad>,
    /// `+shadow`: the cache of `vpads`.
    pub vpad_map: HashMap<(u32, [u64; 4], L, u32), L>,
    /// `+shadow`: first pcs of the verified blocks.
    pub vstart: HashSet<u32>,
}

/// A verification sequence on a successor edge of a shadow-verified block (`+shadow`): flush, compare against
/// tier 1 (`jit_t15_shadow_post`), reload, continue on the ordinary edge.
#[derive(Clone, Copy, Debug)]
pub(super) struct VPad {
    pub l: L,
    pub from: SlotSet,
    pub to: u32,
    pub id: u32,
    pub normal: L,
}

fn is_int(plan: &RegionPlan, pc: u32, s: u32) -> bool {
    plan.fact(pc, s) == TagSet::INT
}

fn fits32(x: i64) -> bool {
    i32::try_from(x).is_ok()
}

fn set(v: &[u32]) -> SlotSet {
    let mut s = SlotSet::EMPTY;
    for &x in v {
        s.insert(x);
    }
    s
}

impl<'c, 'a> Rg<'c, 'a> {
    fn op_at(&self, pc: u32) -> Option<OpCode> {
        self.env.ops[pc as usize]
    }
    fn ins(&self, pc: u32) -> Instruction {
        self.env.ins[pc as usize]
    }
    fn in_region(&self, pc: u32) -> bool {
        pc >= self.plan.head && pc <= self.plan.end
    }
    /// The instruction after `pc` that straight-line code continues at (MMBIN / EXTRAARG followers skipped).
    pub(super) fn follow(&self, pc: u32) -> u32 {
        let mut q = pc + 1;
        while (q as usize) < self.env.follower.len() && self.env.follower[q as usize] {
            q += 1;
        }
        q
    }

    /// Successors of the instruction at `pc` (all of them, inside the region or not).
    fn succs(&self, pc: u32) -> Vec<u32> {
        let i = self.ins(pc);
        let p = i64::from(pc);
        use OpCode as O;
        let op = self.op_at(pc).expect("opcode");
        let v: Vec<i64> = match op {
            O::Jmp => vec![p + 1 + i64::from(i.sj())],
            O::Eq
            | O::Lt
            | O::Le
            | O::Eqk
            | O::Eqi
            | O::Lti
            | O::Lei
            | O::Gti
            | O::Gei
            | O::Test
            | O::Testset => {
                let j = self.ins(pc + 1);
                vec![p + 2 + i64::from(j.sj()), p + 2]
            }
            O::Lfalseskip => vec![p + 2],
            O::Forloop | O::Tforloop => vec![p + 1, p + 1 - i64::from(i.bx())],
            O::Forprep => vec![p + 1, p + i64::from(i.bx()) + 2],
            O::Tforprep => vec![p + 1 + i64::from(i.bx())],
            O::Return | O::Return0 | O::Return1 | O::Tailcall => vec![],
            _ => vec![i64::from(self.follow(pc))],
        };
        let n = self.env.ins.len() as i64;
        v.into_iter()
            .filter(|&t| (0..n).contains(&t))
            .map(|t| t as u32)
            .collect()
    }

    /// Every pc the tier-1 template of a fallback at `pc` may continue at: its successors, plus (a superset, for
    /// safety) the next two instructions.
    fn fall_targets(&self, pc: u32) -> Vec<u32> {
        if self.env.kinds[pc as usize] == Kind::Stub {
            return vec![];
        }
        if super::mutants::active("bad_fall_targets") {
            return vec![];
        }
        let mut v = self.succs(pc);
        let n = self.env.ins.len() as u32;
        for q in [pc + 1, pc + 2] {
            if q < n && !v.contains(&q) && !self.env.follower[q as usize] {
                v.push(q);
            }
        }
        v
    }

    fn k_int(&self, kx: u32) -> Option<i64> {
        let v = self.cx.cfg.consts.get(kx as usize)?;
        (v.tag() == LUA_VNUMINT).then(|| v.as_int()).flatten()
    }

    /// The float constant `kx` of the constants array.
    fn k_flt(&self, kx: u32) -> Option<f64> {
        let v = self.cx.cfg.consts.get(kx as usize)?;
        (v.tag() == LUA_VNUMFLT).then(|| v.as_float()).flatten()
    }

    /// A numeric operand constant of `K[kx]` as a double operand (`None`: not a number, or an integer that a
    /// double does not hold exactly is still converted exactly like `(double)i`, which is what Lua does).
    fn kopd(&self, kx: u32) -> Option<(FOpd, bool)> {
        if self.k_flt(kx).is_some() {
            Some((FOpd::Kf(kx), true))
        } else {
            self.k_int(kx)
                .map(|i| (FOpd::C((i as f64).to_bits()), false))
        }
    }

    /// Float templates (T4b). Only chosen when the facts say that every operand is a singleton number and at
    /// least one is a float (or the operation is `/`); the integer/integer cases are `classify`'s.
    fn classify_float(&self, pc: u32, op: OpCode, i: Instruction) -> Option<Act> {
        use OpCode as O;
        let (a, b, c) = (i.a(), i.b(), i.c());
        let f = |s: u32| self.plan.fact(pc, s);
        let flt = |s: u32| f(s) == TagSet::FLT;
        let num = |s: u32| -> Option<(FOpd, bool)> {
            if f(s) == TagSet::FLT {
                Some((FOpd::Flt(s), true))
            } else if f(s) == TagSet::INT {
                Some((FOpd::Int(s), false))
            } else {
                None
            }
        };
        let bin = |fop: FOp, x: (FOpd, bool), y: (FOpd, bool), is_div: bool| {
            (x.1 || y.1 || is_div).then_some(Act::FBin {
                a,
                op: fop,
                x: x.0,
                y: y.0,
            })
        };
        let fo = |op: OpCode| match op {
            O::Add | O::Addk | O::Addi => FOp::Add,
            O::Sub | O::Subk => FOp::Sub,
            O::Mul | O::Mulk => FOp::Mul,
            _ => FOp::Div,
        };
        let cond = |cc: Cc, x: FOpd, y: FOpd| {
            let j = self.ins(pc + 1);
            let sj = j.sj();
            Act::FCmp {
                cc,
                x,
                y,
                k: i.k() != 0,
                jt: (i64::from(pc) + 2 + i64::from(sj)) as u32,
                skip: pc + 2,
                poll: sj < 0,
            }
        };
        let imm = |v: i32| FOpd::C(f64::from(v).to_bits());
        match op {
            O::Move if flt(b) => Some(Act::MoveF(a, b)),
            O::Loadf => Some(Act::LoadFc(a, f64::from(i.sbx()).to_bits())),
            O::Loadk => self.k_flt(i.bx()).map(|x| Act::LoadFc(a, x.to_bits())),
            O::Add | O::Sub | O::Mul | O::Div => bin(fo(op), num(b)?, num(c)?, op == O::Div),
            O::Addi => bin(FOp::Add, num(b)?, (imm(i.sc()), false), false),
            O::Addk | O::Subk | O::Mulk | O::Divk => {
                bin(fo(op), num(b)?, self.kopd(c)?, op == O::Divk)
            }
            O::Unm if flt(b) => Some(Act::FNeg(a, b)),
            O::Eq if flt(a) && flt(b) => Some(cond(Cc::Eq, FOpd::Flt(a), FOpd::Flt(b))),
            O::Lt if flt(a) && flt(b) => Some(cond(Cc::Lt, FOpd::Flt(a), FOpd::Flt(b))),
            O::Le if flt(a) && flt(b) => Some(cond(Cc::Le, FOpd::Flt(a), FOpd::Flt(b))),
            O::Eqi if flt(a) => Some(cond(Cc::Eq, FOpd::Flt(a), imm(i.sb()))),
            O::Lti if flt(a) => Some(cond(Cc::Lt, FOpd::Flt(a), imm(i.sb()))),
            O::Lei if flt(a) => Some(cond(Cc::Le, FOpd::Flt(a), imm(i.sb()))),
            O::Gti if flt(a) => Some(cond(Cc::Gt, FOpd::Flt(a), imm(i.sb()))),
            O::Gei if flt(a) => Some(cond(Cc::Ge, FOpd::Flt(a), imm(i.sb()))),
            O::Eqk if flt(a) => {
                // float == integer constant is exact only where the integer is a double (|k| <= 2^53)
                let y = match self.kopd(b)? {
                    (y @ FOpd::Kf(_), _) => y,
                    (y, _) => {
                        let k = self.k_int(b)?;
                        if k.unsigned_abs() > (1u64 << 53) {
                            return None;
                        }
                        y
                    }
                };
                Some(cond(Cc::Eq, FOpd::Flt(a), y))
            }
            O::Forloop if flt(a) && flt(a + 1) && flt(a + 2) => Some(Act::FForLoop {
                a,
                t: (i64::from(pc) + 1 - i64::from(i.bx())) as u32,
                fall: pc + 1,
            }),
            _ => None,
        }
    }

    /// Chooses what region code does for `pc`.
    fn classify(&self, pc: u32) -> Act {
        use OpCode as O;
        let Some(insn) = self.plan.insn(pc) else {
            return Act::Dead;
        };
        if !insn.reached {
            return Act::Dead;
        }
        let op = self.op_at(pc).expect("opcode");
        if self.env.kinds[pc as usize] == Kind::Stub {
            return Act::Fall;
        }
        let i = self.ins(pc);
        let (a, b, c) = (i.a(), i.b(), i.c());
        let int = |s: u32| is_int(self.plan, pc, s);
        let bin = |op: AOp, y: Opd| Act::Bin { a, op, x: b, y };
        let cond = |cc: Cc, x: u32, y: Opd| {
            let j = self.ins(pc + 1);
            let sj = j.sj();
            Act::Cmp {
                cc,
                x,
                y,
                k: i.k() != 0,
                jt: (i64::from(pc) + 2 + i64::from(sj)) as u32,
                skip: pc + 2,
                poll: sj < 0,
            }
        };
        if let Some(act) = self.classify_float(pc, op, i) {
            return act;
        }
        if let Some(act) = self.classify_table(pc, op, i) {
            return act;
        }
        if let Some(act) = self.classify_call(pc, op, i) {
            return act;
        }
        match op {
            O::Move if int(b) => Act::MoveI(a, b),
            O::Move => Act::MoveM(a, b),
            O::Loadi => Act::LoadI(a, i64::from(i.sbx())),
            O::Loadk => match self.k_int(i.bx()) {
                Some(x) => Act::LoadI(a, x),
                None => Act::LoadK(a, i.bx()),
            },
            O::Loadfalse => Act::MemConst(a, LUA_VFALSE),
            O::Loadtrue => Act::MemConst(a, LUA_VTRUE),
            O::Loadnil => Act::LoadNil(a, b + 1),
            O::Add if int(b) && int(c) => bin(AOp::Add, Opd::R(c)),
            O::Sub if int(b) && int(c) => bin(AOp::Sub, Opd::R(c)),
            O::Mul if int(b) && int(c) => bin(AOp::Mul, Opd::R(c)),
            O::Band if int(b) && int(c) => bin(AOp::And, Opd::R(c)),
            O::Bor if int(b) && int(c) => bin(AOp::Or, Opd::R(c)),
            O::Bxor if int(b) && int(c) => bin(AOp::Xor, Opd::R(c)),
            O::Addi if int(b) => bin(AOp::Add, Opd::I(i64::from(i.sc()))),
            O::Addk if int(b) && self.k_int(c).is_some() => {
                bin(AOp::Add, Opd::I(self.k_int(c).expect("checked")))
            }
            O::Subk if int(b) && self.k_int(c).is_some() => {
                bin(AOp::Sub, Opd::I(self.k_int(c).expect("checked")))
            }
            O::Mulk if int(b) && self.k_int(c).is_some() => {
                bin(AOp::Mul, Opd::I(self.k_int(c).expect("checked")))
            }
            O::Bandk if int(b) && self.k_int(c).is_some() => {
                bin(AOp::And, Opd::I(self.k_int(c).expect("checked")))
            }
            O::Bork if int(b) && self.k_int(c).is_some() => {
                bin(AOp::Or, Opd::I(self.k_int(c).expect("checked")))
            }
            O::Bxork if int(b) && self.k_int(c).is_some() => {
                bin(AOp::Xor, Opd::I(self.k_int(c).expect("checked")))
            }
            // R[A] = R[B] >> sC  (= shiftl(R[B], -sC))
            O::Shri if int(b) => Act::Shift {
                a,
                b,
                n: -i64::from(i.sc()),
            },
            O::Unm if int(b) => Act::Neg(a, b),
            O::Modk | O::Idivk if int(b) => match self.k_int(c) {
                Some(k) if k != 0 && k != -1 => Act::DivK {
                    a,
                    b,
                    c: k,
                    is_mod: op == O::Modk,
                },
                _ => Act::Fall,
            },
            O::Bnot if int(b) => Act::Not(a, b),
            O::Jmp => {
                let sj = i.sj();
                Act::Jmp {
                    t: (i64::from(pc) + 1 + i64::from(sj)) as u32,
                    poll: sj < 0,
                }
            }
            O::Eq if int(a) && int(b) => cond(Cc::Eq, a, Opd::R(b)),
            O::Lt if int(a) && int(b) => cond(Cc::Lt, a, Opd::R(b)),
            O::Le if int(a) && int(b) => cond(Cc::Le, a, Opd::R(b)),
            O::Eqi if int(a) => cond(Cc::Eq, a, Opd::I(i64::from(i.sb()))),
            O::Lti if int(a) => cond(Cc::Lt, a, Opd::I(i64::from(i.sb()))),
            O::Lei if int(a) => cond(Cc::Le, a, Opd::I(i64::from(i.sb()))),
            O::Gti if int(a) => cond(Cc::Gt, a, Opd::I(i64::from(i.sb()))),
            O::Gei if int(a) => cond(Cc::Ge, a, Opd::I(i64::from(i.sb()))),
            O::Eqk if int(a) && self.k_int(b).is_some() => {
                cond(Cc::Eq, a, Opd::I(self.k_int(b).expect("checked")))
            }
            O::Test => {
                let j = self.ins(pc + 1);
                let sj = j.sj();
                let f = self.plan.fact(pc, a);
                let st = if f.is_empty() || !f.is_singleton() {
                    None
                } else if f == TagSet::NIL || f == TagSet::FALSE {
                    Some(false)
                } else {
                    Some(true)
                };
                if f.is_empty() {
                    return Act::Fall;
                }
                Act::Test {
                    a,
                    k: i.k() != 0,
                    jt: (i64::from(pc) + 2 + i64::from(sj)) as u32,
                    skip: pc + 2,
                    poll: sj < 0,
                    st,
                }
            }
            O::Forloop if int(a) && int(a + 1) && int(a + 2) => Act::ForLoop {
                a,
                t: (i64::from(pc) + 1 - i64::from(i.bx())) as u32,
                fall: pc + 1,
            },
            _ => Act::Fall,
        }
    }

    /// Raw effects of an act, before the register assignment is known: integer register uses and defs, double
    /// register uses and defs, memory defs. Which of them really are register operations depends on the class
    /// each slot was assigned (see [`Rg::eff`]).
    fn raw_effects(act: &Act) -> Raw {
        let none = SlotSet::EMPTY;
        let mut r = Raw::default();
        let fo = |r: &mut Raw, o: &FOpd| match *o {
            FOpd::Flt(s) => {
                r.fuse.insert(s);
            }
            FOpd::Int(s) => {
                r.iuse.insert(s);
            }
            FOpd::Kf(_) | FOpd::C(_) => {}
        };
        match *act {
            Act::LoadI(a, _) => r.idef = set(&[a]),
            Act::MoveI(a, b) => {
                r.iuse = set(&[b]);
                r.idef = set(&[a]);
            }
            Act::Bin { a, x, y, .. } => {
                let mut u = set(&[x]);
                if let Opd::R(s) = y {
                    u.insert(s);
                }
                r.iuse = u;
                r.idef = set(&[a]);
            }
            Act::Neg(a, b) | Act::Not(a, b) | Act::Shift { a, b, .. } | Act::DivK { a, b, .. } => {
                r.iuse = set(&[b]);
                r.idef = set(&[a]);
            }
            Act::MemConst(a, _) | Act::LoadK(a, _) | Act::MoveM(a, _) => r.mdef = set(&[a]),
            Act::LoadNil(a, n) => {
                let mut m = none;
                m.insert_range(a, a + n);
                r.mdef = m;
            }
            Act::Cmp { x, y, .. } => {
                let mut u = set(&[x]);
                if let Opd::R(s) = y {
                    u.insert(s);
                }
                r.iuse = u;
            }
            Act::ForLoop { a, .. } => {
                r.iuse = set(&[a, a + 1, a + 2]);
                r.idef = set(&[a, a + 2]);
            }
            Act::LoadFc(a, _) => r.fdef = set(&[a]),
            Act::MoveF(a, b) => {
                r.fuse = set(&[b]);
                r.fdef = set(&[a]);
            }
            Act::FBin { a, x, y, .. } => {
                fo(&mut r, &x);
                fo(&mut r, &y);
                r.fdef = set(&[a]);
            }
            Act::FNeg(a, b) => {
                r.fuse = set(&[b]);
                r.fdef = set(&[a]);
            }
            Act::FCmp { x, y, .. } => {
                fo(&mut r, &x);
                fo(&mut r, &y);
            }
            Act::FForLoop { a, .. } => {
                r.fuse = set(&[a, a + 1, a + 2]);
                r.fdef = set(&[a + 2]);
            }
            Act::GetArr { a, key, res, .. } => {
                if let TKey::Reg(k) = key {
                    r.iuse.insert(k);
                }
                match res {
                    TRes::Int => r.idef.insert(a),
                    TRes::Flt => r.fdef.insert(a),
                    TRes::Mem(_) => r.mdef.insert(a),
                }
            }
            Act::GetFld { a, res, .. } => match res {
                TRes::Int => r.idef.insert(a),
                TRes::Flt => r.fdef.insert(a),
                TRes::Mem(_) => r.mdef.insert(a),
            },
            Act::SetArr { key, val, .. } => {
                if let TKey::Reg(k) = key {
                    r.iuse.insert(k);
                }
                match val {
                    TVal::Int(v) => r.iuse.insert(v),
                    TVal::Flt(v) => r.fuse.insert(v),
                    _ => {}
                }
            }
            Act::SetFld { val, .. } => match val {
                TVal::Int(v) => r.iuse.insert(v),
                TVal::Flt(v) => r.fuse.insert(v),
                _ => {}
            },
            Act::Intr {
                kind, a, ac, rc, ..
            } => {
                let args: &[u32] = if kind.nargs() == 2 {
                    &[a + 1, a + 2]
                } else {
                    &[a + 1]
                };
                for &s in args {
                    match ac {
                        IK::Int => r.iuse.insert(s),
                        IK::Flt => r.fuse.insert(s),
                    }
                }
                match rc {
                    IK::Int => r.idef.insert(a),
                    IK::Flt => r.fdef.insert(a),
                }
            }
            Act::GetUp { a, res, .. } => match res {
                URes::Int => r.idef.insert(a),
                URes::Flt => r.fdef.insert(a),
                URes::Mem(_) => r.mdef.insert(a),
            },
            Act::Dead | Act::Fall | Act::IntrH { .. } | Act::Jmp { .. } | Act::Test { .. } => {}
        }
        r
    }

    /// `(register uses, register defs, memory defs)` of an act under the current assignment: an integer operation
    /// on a slot with an integer register, a double operation on a slot with a double register. A definition of
    /// the other class writes the slot in memory (and so removes it from the register-resident set).
    fn eff(&self, act: &Act) -> (SlotSet, SlotSet, SlotSet) {
        let r = Self::raw_effects(act);
        let (g, x) = (self.asg.gslots(), self.asg.xslots());
        let mut ruse = r.iuse.meet(&g);
        ruse.union_with(&r.fuse.meet(&x));
        let mut rdef = r.idef.meet(&g);
        rdef.union_with(&r.fdef.meet(&x));
        let mut mdef = r.mdef;
        mdef.union_with(&r.idef.minus(&g));
        mdef.union_with(&r.fdef.minus(&x));
        (ruse, rdef, mdef)
    }

    /// Successor edges `(target, poll)` of a non-fallback act.
    fn act_edges(&self, pc: u32, act: &Act) -> Vec<(u32, bool)> {
        match *act {
            Act::Dead | Act::Fall | Act::IntrH { .. } => vec![],
            Act::Jmp { t, poll } => vec![(t, poll)],
            Act::Cmp { jt, skip, poll, .. }
            | Act::Test { jt, skip, poll, .. }
            | Act::FCmp { jt, skip, poll, .. } => vec![(jt, poll), (skip, false)],
            Act::ForLoop { t, fall, .. } | Act::FForLoop { t, fall, .. } => {
                vec![(fall, false), (t, true)]
            }
            _ => vec![(self.follow(pc), false)],
        }
    }

    /// The slots a stub at `pc` loads into registers at most: assigned slots whose fact is the class of their
    /// register (integer / float) and that the stub guards.
    fn load_set(&self, pc: u32) -> SlotSet {
        let mut s = SlotSet::EMPTY;
        if !self.in_region(pc) {
            return s;
        }
        for (slot, t) in self.plan.stub_guards(pc) {
            if (t == TagSet::INT && self.asg.of(slot).is_some())
                || (t == TagSet::FLT && self.asg.xof(slot).is_some())
            {
                s.insert(slot);
            }
        }
        s
    }
}

/// Options of [`emit_region`] derived from the knobs.
pub(super) fn max_regs(opt: &OptFlags) -> usize {
    if opt.no_ra {
        0
    } else {
        alloc::NALLOC
    }
}

/// Emits the region of `plan` into `cx`. Returns the entry stub label and the resume overrides, or `None` when the
/// region would not pay (no typed instruction at all).
pub(crate) fn emit_region(
    cx: &mut Cx<'_>,
    env: &RegionEnv<'_>,
    plan: &RegionPlan,
    opt: &OptFlags,
    fb: Option<super::runtime::FeedbackSpec>,
) -> Option<RegionOut> {
    if rn_core::jit_layout::LUA54 {
        return None; // T-5.5: 5.4 regions need the 5.4 FORLOOP layout and `alimit_touch`
    }
    let (h, e) = (plan.head, plan.end);
    let npc = (e - h + 1) as usize;
    let reg_l: Vec<L> = (0..npc).map(|_| cx.p.label()).collect();
    let stub_l: Vec<L> = (0..npc).map(|_| cx.p.label()).collect();
    let mut g = Rg {
        cx,
        env,
        plan,
        opt,
        asg: Assign::none(plan.nslots as usize),
        acts: vec![Act::Dead; npc],
        rr_in: vec![None; npc],
        reg_l,
        stub_l,
        need_stub: HashSet::new(),
        rr: SlotSet::EMPTY,
        pads: Vec::new(),
        pad_map: HashMap::new(),
        cexit_map: HashMap::new(),
        refusal: None,
        chk_map: HashMap::new(),
        fall_succ: HashSet::new(),
        slows: Vec::new(),
        slow_counted: Vec::new(),
        fb,
        sites: Vec::new(),
        storm_pads: Vec::new(),
        storm_map: HashMap::new(),
        storm_entry: HashMap::new(),
        vblock_of: vec![None; npc],
        vcur: None,
        vpads: Vec::new(),
        vpad_map: HashMap::new(),
        vstart: HashSet::new(),
    };
    for k in 0..npc {
        g.acts[k] = g.classify(h + k as u32);
    }
    if crate::rt::verbose() && std::env::var_os("RUSTYNAIL_JIT_ACTS").is_some() {
        for k in 0..npc {
            let pc = h + k as u32;
            let i = g.ins(pc);
            eprintln!(
                "[t15]   pc {pc} {:?} {:?}  facts a={} b={} c={}",
                g.op_at(pc),
                g.acts[k],
                plan.fact(pc, i.a()),
                plan.fact(pc, i.b()),
                plan.fact(pc, i.c())
            );
        }
    }
    // profitability: every fallback costs a flush and a reload, so loop-depth-weighted typed work must dominate
    let depth_of = |pc: u32| {
        plan.blocks
            .iter()
            .find(|b| b.start <= pc && pc < b.end)
            .map_or(1, |b| b.depth.max(1))
    };
    let (mut typed_w, mut fall_w, mut typed, mut falls) = (0u64, 0u64, 0u32, 0u32);
    for k in 0..npc {
        let w = 10u64.pow(depth_of(h + k as u32).min(6));
        match g.acts[k] {
            Act::Dead => {}
            Act::Fall | Act::IntrH { .. } => {
                // a `CALL` of a Lua function costs a flush, the tier-1 call sequence (the callee runs in tier 1 or in
                // another region), a stub with its guards and a register reload: the typed work around it must pay
                // for that (spectral_norm: typed work around `A(i, j)` does not, 10% slower than tier 1)
                let call_w = if matches!(g.op_at(h + k as u32), Some(OpCode::Call)) {
                    CALL_FALL_WEIGHT
                } else {
                    1
                };
                // (an upvalue load that stays a fallback is a whole flush / template / stub round trip around one
                // load; it counts double, docs 21.4)
                let up = matches!(g.op_at(h + k as u32), Some(OpCode::Getupval))
                    && !super::mutants::active("gate_upval_single");
                fall_w += if up { 2 * w * call_w } else { w * call_w };
                falls += 1;
            }
            // (an inline table access whose table fact is not proven keeps a run-time tag check that mostly
            // fails when the speculation is wrong: it does not count as typed work for the gate)
            Act::GetArr { tp: false, .. }
            | Act::SetArr { tp: false, .. }
            | Act::GetFld { tp: false, .. }
            | Act::SetFld { tp: false, .. } => {
                fall_w += w;
                falls += 1;
            }
            // (a load whose result type is unknown - no profile - leaves an `Any` slot whose consumers are fallbacks
            // (flush, tier-1 template, stub) next to the inline load: it counts as fallback work, not as typed work)
            Act::GetArr {
                res: TRes::Mem(None),
                ..
            }
            | Act::GetFld {
                res: TRes::Mem(None),
                ..
            } => {
                fall_w += w;
                falls += 1;
            }
            _ => {
                typed_w += w;
                typed += 1;
            }
        }
    }
    if typed == 0 || (typed_w < 2 * fall_w && !opt.force) {
        if crate::rt::verbose() {
            eprintln!("[t15] region [{h}, {e}] not generated: {typed} typed, {falls} fallback (weighted {typed_w}/{fall_w})");
        }
        return None;
    }
    // weights
    let mut weight = vec![0u64; plan.nslots as usize];
    let mut fweight = vec![0u64; plan.nslots as usize];
    for k in 0..npc {
        let raw = Rg::raw_effects(&g.acts[k]);
        let (u, d) = (raw.iuse, raw.idef);
        let (fu, fd) = (raw.fuse, raw.fdef);
        let depth = plan
            .blocks
            .iter()
            .find(|b| b.start <= h + k as u32 && h + (k as u32) < b.end)
            .map_or(1, |b| b.depth.max(1));
        let w = 10u64.pow(depth.min(6));
        let mut all = u;
        all.union_with(&d);
        for s in all.iter() {
            weight[s as usize] += w;
        }
        let mut fall = fu;
        fall.union_with(&fd);
        for s in fall.iter() {
            fweight[s as usize] += w;
        }
    }
    // `DivK` uses V4 (divisor) and V6 as scratch, inline table accesses V4 as their fourth scratch register
    let has_div = g
        .acts
        .iter()
        .any(|a| matches!(a, Act::DivK { .. }) || Rg::is_table_act(a));
    g.asg = alloc::choose_all(
        &weight,
        &fweight,
        max_regs(opt).min(if has_div { 4 } else { alloc::NALLOC }),
        if opt.no_ra { 0 } else { alloc::NALLOC_X },
        opt.randra,
    );
    let assigned = g.asg.slots();
    // stub-entered pcs: the head, resume entries inside the region, successors of fallbacks
    let mut stub_entered: Vec<usize> = vec![0];
    g.need_stub.insert(h);
    for pc in h..=e {
        let k = (pc - h) as usize;
        if matches!(g.acts[k], Act::Dead) {
            continue;
        }
        if pc != h && env.resume[pc as usize] {
            g.need_stub.insert(pc);
            stub_entered.push(k);
        }
        if Rg::is_table_act(&g.acts[k]) || Rg::is_call_act(&g.acts[k]) {
            // the slow path of an inline table access continues in the stub of the next instruction
            let s = g.follow(pc);
            if g.in_region(s) {
                g.fall_succ.insert(s);
                g.need_stub.insert(s);
                stub_entered.push((s - h) as usize);
            }
        }
        if matches!(g.acts[k], Act::Fall | Act::IntrH { .. }) {
            for s in g.fall_targets(pc) {
                if g.in_region(s) {
                    g.fall_succ.insert(s);
                    g.need_stub.insert(s);
                    stub_entered.push((s - h) as usize);
                }
            }
        }
    }
    // register dataflow
    let nodes: Vec<Node> = (0..npc)
        .map(|k| {
            let pc = h + k as u32;
            let act = g.acts[k];
            let (ruse, rdef, mdef) = g.eff(&act);
            let live = !matches!(act, Act::Dead);
            let succs: Vec<usize> = if matches!(act, Act::Fall | Act::IntrH { .. }) {
                g.fall_targets(pc)
                    .into_iter()
                    .filter(|&s| g.in_region(s))
                    .map(|s| (s - h) as usize)
                    .collect()
            } else {
                g.act_edges(pc, &act)
                    .into_iter()
                    .filter(|&(s, _)| g.in_region(s))
                    .map(|(s, _)| (s - h) as usize)
                    .collect()
            };
            Node {
                live,
                ruse,
                rdef,
                mdef,
                fall: matches!(act, Act::Fall | Act::IntrH { .. }),
                succs,
            }
        })
        .collect();
    let load = |k: usize| g.load_set(h + k as u32);
    g.rr_in = alloc::solve_rr(&nodes, &assigned, &stub_entered, &load);
    for k in 0..npc {
        if g.rr_in[k].is_none() {
            g.acts[k] = Act::Dead;
        }
    }
    // everything below appends to the tier-1 program: remember how to take it back if the generator refuses
    let snap = (g.cx.p.ops.len(), g.cx.cold.len(), g.cx.exit_at_snapshot());
    g.plan_shadow();
    g.emit_all();
    if g.refusal.is_none() {
        g.finish();
    }
    if let Some(why) = g.refusal.take() {
        g.cx.p.ops.truncate(snap.0);
        g.cx.cold.truncate(snap.1);
        g.cx.exit_restore(snap.2);
        g.cx.capture_rejoin = false;
        if crate::rt::verbose() {
            eprintln!("[t15] region [{h}, {e}] refused by the code generator ({why}); the nest stays in tier 1");
        }
        return None;
    }
    let mut overrides: Vec<(u32, L)> = Vec::new();
    let mut ids: Vec<u32> = g.need_stub.iter().copied().collect();
    ids.sort_unstable();
    for pc in ids {
        if g.reached(pc) && (pc == h || env.resume[pc as usize]) {
            overrides.push((pc, g.stub_l[(pc - h) as usize]));
        }
    }
    if crate::rt::verbose() {
        eprintln!(
            "[t15] region [{h}, {e}]: {typed} typed, {falls} fallback, registers {:?}",
            g.asg.slots().iter().collect::<Vec<_>>()
        );
    }
    REGIONS.with(|c| c.set(c.get() + 1));
    let points: Vec<crate::lower::RegionPoint> = (h..=e)
        .map(|pc| {
            let k = (pc - h) as usize;
            let live = !matches!(g.acts[k], Act::Dead) && g.rr_in[k].is_some();
            crate::lower::RegionPoint {
                pc,
                t1: g.cx.pcl[pc as usize],
                reg: live.then(|| g.reg_l[k]),
                rr: g.rr_in[k]
                    .filter(|_| live)
                    .map(|rr| {
                        rr.iter()
                            .filter_map(|s| g.asg.of(s).map(|v| (s, v)))
                            .collect()
                    })
                    .unwrap_or_default(),
                rx: g.rr_in[k]
                    .filter(|_| live)
                    .map(|rr| {
                        rr.iter()
                            .filter_map(|s| g.asg.xof(s).map(|v| (s, v)))
                            .collect()
                    })
                    .unwrap_or_default(),
            }
        })
        .collect();
    Some(RegionOut {
        points,
        heads: vec![(h, g.stub_l[0])],
        overrides,
        sites: std::mem::take(&mut g.sites),
        resume_overrides: Vec::new(),
    })
}

// ---- emission ----------------------------------------------------------------------------------------------

impl Rg<'_, '_> {
    /// Gives up on the region (first reason wins); see [`Rg::refusal`].
    pub(super) fn refuse(&mut self, why: impl Into<String>) {
        if self.refusal.is_none() {
            self.refusal = Some(why.into());
        }
    }

    /// The label region code jumps to in order to continue in tier 1 at `pc`. With `+verify` it is a check of the
    /// type knowledge (`Cx::ty`) that tier 1 compiled `pc` with (docs/JIT_TIER15.md section 12): a jump into the middle
    /// of a tier-1 block is only sound if the frame has the tags tier 1 assumed there. The checks are emitted by
    /// `finish`; without `+verify` this is exactly `pcl[pc]`.
    pub(super) fn tier1_label(&mut self, pc: u32) -> L {
        let real = self.cx.pcl[pc as usize];
        if !self.opt.verify || self.env.ty_at.get(pc as usize).is_none_or(Vec::is_empty) {
            return real;
        }
        if let Some(&l) = self.chk_map.get(&pc) {
            return l;
        }
        let l = self.cx.p.label();
        self.chk_map.insert(pc, l);
        l
    }

    fn emit_all(&mut self) {
        let (h, e) = (self.plan.head, self.plan.end);
        for pc in h..=e {
            let k = (pc - h) as usize;
            if self.refusal.is_some() {
                return;
            }
            if matches!(self.acts[k], Act::Dead) {
                continue;
            }
            let Some(rr) = self.rr_in[k] else { continue };
            self.rr = rr;
            self.cx.op(Op::Bind(self.reg_l[k]));
            self.vcur = self.vblock_of[k];
            if let Some(n) = self.opt.exitstorm {
                // pseudo-guard at the instruction boundary: leave for tier 1 at `pc` every n-th arrival (stub entries
                // skip it, so that a region visit always executes at least one instruction)
                let fire = self.storm_leave(rr, pc);
                self.storm_test(n, fire);
                let l = self.cx.p.label();
                self.cx.op(Op::Bind(l));
                self.storm_entry.insert(pc, l);
            }
            if let Some(id) = self.vblock_of[k].filter(|_| self.vstart.contains(&pc)) {
                self.emit_shadow_pre(id);
            }
            if self.env.head[pc as usize] && self.env.cnt_at[pc as usize] > 0 {
                // block count (C1) in the budget register, same blocks and counts as tier 1
                let x = self.cexit(pc, self.env.cnt_at[pc as usize]);
                let mut cnt = self.env.cnt_at[pc as usize] as i32;
                if k > 0
                    && matches!(self.acts[k - 1], Act::Fall)
                    && super::mutants::active("count_after_fall")
                {
                    cnt += 1;
                }
                self.cx.op(Op::Sub(5, Src::Imm(cnt)));
                self.cx.br(Cc::Lt, 5, Src::Imm(0), x);
            }
            if self.opt.flushall {
                self.flush_stores(self.rr);
                self.store_budget();
            }
            let act = self.acts[k];
            self.emit_act(pc, act);
            debug_assert!(
                matches!(
                    act,
                    Act::Fall
                        | Act::IntrH { .. }
                        | Act::Jmp { .. }
                        | Act::Cmp { .. }
                        | Act::Test { .. }
                        | Act::ForLoop { .. }
                        | Act::FCmp { .. }
                        | Act::FForLoop { .. }
                ) || {
                    let (ruse, rdef, mdef) = self.eff(&act);
                    let n = Node {
                        ruse,
                        rdef,
                        mdef,
                        ..Node::default()
                    };
                    alloc::transfer(rr, &n, &self.asg.slots()) == self.rr
                },
                "register state after {act:?} differs from the dataflow"
            );
        }
    }

    // -- operand helpers

    /// The register holding integer slot `s`: its own when it has one (loaded first if it is not register-resident
    /// yet), else `scratch` loaded from the slot.
    fn src_reg(&mut self, s: u32, scratch: V) -> V {
        match self.asg.of(s) {
            Some(v) => {
                if !self.rr.contains(s) {
                    self.cx.ld(v, W::B8, fr(s, 8));
                    self.rr.insert(s);
                }
                v
            }
            None => {
                self.cx.ld(scratch, W::B8, fr(s, 8));
                scratch
            }
        }
    }

    fn opd_src(&mut self, o: Opd, scratch: V) -> Src {
        match o {
            Opd::R(s) => Src::V(self.src_reg(s, scratch)),
            Opd::I(k) if fits32(k) => Src::Imm(k as i32),
            Opd::I(k) => {
                self.cx.op(Op::Mov(scratch, k as u64));
                Src::V(scratch)
            }
        }
    }

    /// Register that receives a new integer value of slot `a`.
    fn def_reg(&self, a: u32) -> V {
        self.asg.of(a).unwrap_or(S0)
    }

    /// The integer value of slot `a` is in `v`: keep it in the register (register-resident) or write it to the slot.
    fn commit_int(&mut self, a: u32, v: V) {
        if self.asg.of(a).is_some() {
            debug_assert_eq!(self.asg.of(a), Some(v));
            self.rr.insert(a);
        } else {
            self.cx.st_imm(W::B1, fr(a, 0), TAG_INT);
            self.cx.st(W::B8, fr(a, 8), v);
            // (the slot may have a double register: its value there is dead now)
            self.rr.remove(a);
        }
    }

    /// A slot is overwritten in memory.
    fn mem_def(&mut self, a: u32) {
        self.rr.remove(a);
    }

    /// Makes the VM stack slot of `s` exact (register-resident integers are written, the register stays valid).
    fn spill(&mut self, s: u32) {
        if self.rr.contains(s) {
            if let Some(v) = self.asg.of(s) {
                self.cx.st_imm(W::B1, fr(s, 0), TAG_INT);
                self.cx.st(W::B8, fr(s, 8), v);
            } else {
                let x = self.asg.xof(s).expect("register");
                self.cx.st_imm(W::B1, fr(s, 0), i32::from(LUA_VNUMFLT));
                self.cx.op(Op::StF {
                    m: fr(s, 8),
                    src: x,
                });
            }
        }
    }

    // -- float operand helpers (T4b)

    /// The double register holding the value of `o`: the slot's own register (loaded first if it is not
    /// register-resident), else `scratch` filled from memory / converted / materialised.
    fn fx(&mut self, o: FOpd, scratch: X) -> X {
        match o {
            FOpd::Flt(s) => match self.asg.xof(s) {
                Some(x) => {
                    if !self.rr.contains(s) {
                        self.cx.op(Op::LdF {
                            dst: x,
                            m: fr(s, 8),
                        });
                        self.rr.insert(s);
                    }
                    x
                }
                None => {
                    debug_assert!(
                        !self.rr.contains(s),
                        "float slot {s} resident in an integer register"
                    );
                    self.cx.op(Op::LdF {
                        dst: scratch,
                        m: fr(s, 8),
                    });
                    scratch
                }
            },
            FOpd::Int(s) => {
                let r = self.src_reg(s, S0);
                if super::mutants::active("i2f_missing") {
                    self.cx.op(Op::GtoF(scratch, r));
                } else {
                    self.cx.op(Op::I2Fx(scratch, r));
                }
                scratch
            }
            FOpd::Kf(kx) => {
                self.cx.op(Op::LdF {
                    dst: scratch,
                    m: M {
                        base: Base::K,
                        off: (kx as i32) * 16 + 8,
                    },
                });
                scratch
            }
            FOpd::C(bits) => {
                self.cx.op(Op::Mov(S0, bits));
                self.cx.op(Op::GtoF(scratch, S0));
                scratch
            }
        }
    }

    /// The second operand of a double operation: a register, or a memory operand where one is free.
    fn fy(&mut self, o: FOpd, scratch: X) -> Result<X, M> {
        match o {
            FOpd::Flt(s) if self.asg.xof(s).is_none() => Err(fr(s, 8)),
            FOpd::Kf(kx) => Err(M {
                base: Base::K,
                off: (kx as i32) * 16 + 8,
            }),
            _ => Ok(self.fx(o, scratch)),
        }
    }

    /// Register that receives a new double value of slot `a` (`X0` when the slot has none).
    fn def_x(&self, a: u32) -> X {
        self.asg.xof(a).unwrap_or(0)
    }

    /// The double value of slot `a` is in `x`: keep it (register-resident) or write tag + bits to the slot.
    fn commit_flt(&mut self, a: u32, x: X) {
        if self.asg.xof(a).is_some() {
            debug_assert_eq!(self.asg.xof(a), Some(x));
            self.rr.insert(a);
        } else {
            self.cx.st_imm(W::B1, fr(a, 0), i32::from(LUA_VNUMFLT));
            self.cx.op(Op::StF {
                m: fr(a, 8),
                src: x,
            });
            // (the slot may have an integer register: its value there is dead now)
            self.rr.remove(a);
        }
    }

    fn end_straight(&mut self, pc: u32) {
        let f = self.follow(pc);
        let l = self.edge(self.rr, f, false);
        self.cx.op(Op::Jmp(l));
    }

    fn emit_act(&mut self, pc: u32, act: Act) {
        match act {
            Act::Dead => {}
            Act::Fall => self.emit_fall(pc),
            Act::Intr { .. } | Act::IntrH { .. } => self.emit_intr(pc, act),
            Act::GetUp { a, b, res } => self.emit_get_up(pc, a, b, res),
            Act::GetArr { .. } | Act::SetArr { .. } | Act::GetFld { .. } | Act::SetFld { .. } => {
                self.emit_table_act(pc, act)
            }
            Act::LoadI(a, k) => {
                let d = self.def_reg(a);
                self.cx.op(Op::Mov(d, k as u64));
                self.commit_int(a, d);
                self.end_straight(pc);
            }
            Act::MoveI(a, b) => {
                let s = self.src_reg(b, S0);
                let d = self.def_reg(a);
                if d != s {
                    self.cx.op(Op::MovR(d, s));
                }
                self.commit_int(a, d);
                self.end_straight(pc);
            }
            Act::Bin { a, op, x, y } => {
                self.emit_bin(a, op, x, y);
                self.end_straight(pc);
            }
            Act::Neg(a, b) | Act::Not(a, b) => {
                let s = self.src_reg(b, S0);
                let d = self.def_reg(a);
                if d != s {
                    self.cx.op(Op::MovR(d, s));
                }
                self.cx.op(if matches!(act, Act::Neg(..)) {
                    Op::Neg(d)
                } else {
                    Op::Not(d)
                });
                self.commit_int(a, d);
                self.end_straight(pc);
            }
            Act::DivK { a, b, c, is_mod } => {
                // SDivRem needs its operands in V0..V6 and clobbers rax/rdx: dividend in V6, divisor in V4 (V4 is
                // not assignable in regions with divisions, see `emit_region`); floor correction in V7/V8.
                let s = self.src_reg(b, S2);
                if s != S2 {
                    self.cx.op(Op::MovR(S2, s));
                }
                self.cx.op(Op::Mov(4, c as u64));
                self.cx.op(Op::SDivRem {
                    q: S2,
                    r: 4,
                    a: S2,
                    b: 4,
                });
                let lfix = self.cx.p.label();
                self.cx.br(Cc::Eq, 4, Src::Imm(0), lfix);
                self.cx.op(Op::Mov(S1, c as u64));
                self.cx.op(Op::MovR(S0, 4));
                self.cx.op(Op::Xor(S0, Src::V(S1)));
                self.cx.br(Cc::Ge, S0, Src::Imm(0), lfix);
                if is_mod {
                    self.cx.op(Op::Add(4, Src::V(S1)));
                } else {
                    self.cx.op(Op::Sub(S2, Src::Imm(1)));
                }
                self.cx.op(Op::Bind(lfix));
                let d = self.def_reg(a);
                self.cx.op(Op::MovR(d, if is_mod { 4 } else { S2 }));
                self.commit_int(a, d);
                self.end_straight(pc);
            }
            Act::Shift { a, b, n } => {
                let s = self.src_reg(b, S0);
                let d = self.def_reg(a);
                if (1..=63).contains(&n) {
                    if d != s {
                        self.cx.op(Op::MovR(d, s));
                    }
                    self.cx.op(Op::ShlI(d, n as u8));
                } else if (-63..=-1).contains(&n) {
                    if d != s {
                        self.cx.op(Op::MovR(d, s));
                    }
                    self.cx.op(Op::ShrI(d, (-n) as u8));
                } else if n == 0 {
                    if d != s {
                        self.cx.op(Op::MovR(d, s));
                    }
                } else {
                    self.cx.op(Op::Mov(d, 0));
                }
                self.commit_int(a, d);
                self.end_straight(pc);
            }
            Act::MemConst(a, tag) => {
                self.cx.st_imm(W::B1, fr(a, 0), i32::from(tag));
                self.cx.st_imm(W::B8, fr(a, 8), 0);
                self.mem_def(a);
                self.end_straight(pc);
            }
            Act::LoadNil(a, n) => {
                for s in a..a + n {
                    self.cx.st_imm(W::B1, fr(s, 0), i32::from(LUA_VNIL));
                    self.cx.st_imm(W::B8, fr(s, 8), 0);
                    self.mem_def(s);
                }
                self.end_straight(pc);
            }
            Act::LoadFc(a, bits) => {
                self.cx.op(Op::Mov(S0, bits));
                if self.asg.xof(a).is_some() {
                    let d = self.def_x(a);
                    self.cx.op(Op::GtoF(d, S0));
                    self.commit_flt(a, d);
                } else {
                    self.cx.st_imm(W::B1, fr(a, 0), i32::from(LUA_VNUMFLT));
                    self.cx.st(W::B8, fr(a, 8), S0);
                    self.mem_def(a);
                }
                self.end_straight(pc);
            }
            Act::MoveF(a, b) => {
                let d = self.def_x(a);
                let s = self.fx(FOpd::Flt(b), d);
                if s != d {
                    self.cx.op(Op::MovF(d, s));
                }
                self.commit_flt(a, d);
                self.end_straight(pc);
            }
            Act::FNeg(a, b) => {
                let d = self.def_x(a);
                let s = self.fx(FOpd::Flt(b), d);
                if s != d {
                    self.cx.op(Op::MovF(d, s));
                }
                if !super::mutants::active("fneg_missing") {
                    self.cx.op(Op::FNeg(d));
                }
                self.commit_flt(a, d);
                self.end_straight(pc);
            }
            Act::FBin { a, op, x, y } => {
                let d = self.def_x(a);
                let xr = self.fx(x, 0);
                let yo = self.fy(y, 1);
                // the operation is done in `d`, unless `d` is also the second operand (the order of the operands
                // decides which NaN wins: no commuting): then in X0
                let alias = yo == Ok(d) && d != xr;
                if alias && super::mutants::active("fbin_alias_commute") {
                    // BUG: d = d op x
                    self.cx.op(Op::FBin {
                        op,
                        dst: d,
                        src: xr,
                    });
                    self.commit_flt(a, d);
                    self.end_straight(pc);
                    return;
                }
                let w = if alias { 0 } else { d };
                if xr != w {
                    self.cx.op(Op::MovF(w, xr));
                }
                match yo {
                    Ok(yr) => self.cx.op(Op::FBin {
                        op,
                        dst: w,
                        src: yr,
                    }),
                    Err(m) => self.cx.op(Op::FBinM { op, dst: w, m }),
                }
                if w != d {
                    self.cx.op(Op::MovF(d, w));
                }
                self.commit_flt(a, d);
                self.end_straight(pc);
            }
            Act::FCmp {
                cc,
                x,
                y,
                k,
                jt,
                skip,
                poll,
            } => {
                let xr = self.fx(x, 0);
                let yr = self.fx(y, 1);
                let (tt, tf) = if k { (jt, skip) } else { (skip, jt) };
                let lt = self.edge(self.rr, tt, poll && tt == jt);
                let lf = self.edge(self.rr, tf, poll && tf == jt);
                // (never a negated condition: a NaN makes every ordered comparison and `==` false)
                if super::mutants::active("fcmp_negated") {
                    // BUG: branch on the negated condition to the false target
                    let neg = match cc {
                        Cc::Lt => Cc::Ge,
                        Cc::Le => Cc::Gt,
                        Cc::Gt => Cc::Le,
                        Cc::Ge => Cc::Lt,
                        _ => Cc::Ne,
                    };
                    self.cx.op(Op::FBrX {
                        cc: neg,
                        a: xr,
                        b: yr,
                        to: lf,
                    });
                    self.cx.op(Op::Jmp(lt));
                } else {
                    self.cx.op(Op::FBrX {
                        cc,
                        a: xr,
                        b: yr,
                        to: lt,
                    });
                    self.cx.op(Op::Jmp(lf));
                }
            }
            Act::FForLoop { a, t, fall } => {
                // idx' = idx + step into X0 (the slot keeps the old value on the exit path); the step's sign is
                // tested exactly on its bits (0 < step <=> 0 < bits <= bits(+inf), as signed integers)
                let xi = self.fx(FOpd::Flt(a + 2), 0);
                if xi != 0 {
                    self.cx.op(Op::MovF(0, xi));
                }
                match self.fy(FOpd::Flt(a + 1), 1) {
                    Ok(yr) => {
                        self.cx.op(Op::FBin {
                            op: FOp::Add,
                            dst: 0,
                            src: yr,
                        });
                        self.cx.op(Op::FtoG(S0, yr));
                    }
                    Err(m) => {
                        self.cx.op(Op::FBinM {
                            op: FOp::Add,
                            dst: 0,
                            m,
                        });
                        self.cx.ld(S0, W::B8, m);
                    }
                }
                let xl = self.fx(FOpd::Flt(a), 1);
                let (lneg, lcont) = (self.cx.p.label(), self.cx.p.label());
                let lf = self.edge(self.rr, fall, false);
                if super::mutants::active("fforloop_clobber_idx") && xi != 0 {
                    // BUG: the index register is updated whether or not the loop continues
                    self.cx.op(Op::MovF(xi, 0));
                }
                if !super::mutants::active("fforloop_always_pos") {
                    self.cx.br(Cc::Le, S0, Src::Imm(0), lneg);
                    self.cx.op(Op::Mov(S1, 0x7FF0_0000_0000_0000));
                    self.cx.br(Cc::Gt, S0, Src::V(S1), lneg);
                }
                // 0 < step: continue while idx <= limit
                self.cx.op(Op::FBrX {
                    cc: Cc::Le,
                    a: 0,
                    b: xl,
                    to: lcont,
                });
                self.cx.op(Op::Jmp(lf));
                // not 0 < step: continue while limit <= idx
                self.cx.op(Op::Bind(lneg));
                self.cx.op(Op::FBrX {
                    cc: Cc::Le,
                    a: xl,
                    b: 0,
                    to: lcont,
                });
                self.cx.op(Op::Jmp(lf));
                self.cx.op(Op::Bind(lcont));
                match self.asg.xof(a + 2) {
                    Some(x) => self.cx.op(Op::MovF(x, 0)),
                    None => self.cx.op(Op::StF {
                        m: fr(a + 2, 8),
                        src: 0,
                    }),
                }
                let lb = self.edge(self.rr, t, true);
                self.cx.op(Op::Jmp(lb));
            }
            Act::LoadK(a, kx) => {
                let km = M {
                    base: Base::K,
                    off: (kx as i32) * 16,
                };
                self.cx.ld(S0, W::B1, km);
                self.cx.ld(
                    S1,
                    W::B8,
                    M {
                        off: km.off + 8,
                        ..km
                    },
                );
                self.cx.st(W::B1, fr(a, 0), S0);
                self.cx.st(W::B8, fr(a, 8), S1);
                self.mem_def(a);
                self.end_straight(pc);
            }
            Act::MoveM(a, b) => {
                self.spill(b);
                self.cx.ld(S0, W::B1, fr(b, 0));
                self.cx.ld(S1, W::B8, fr(b, 8));
                self.cx.st(W::B1, fr(a, 0), S0);
                self.cx.st(W::B8, fr(a, 8), S1);
                self.mem_def(a);
                self.end_straight(pc);
            }
            Act::Jmp { t, poll } => {
                let l = self.edge(self.rr, t, poll);
                self.cx.op(Op::Jmp(l));
            }
            Act::Cmp {
                cc,
                x,
                y,
                k,
                jt,
                skip,
                poll,
            } => {
                let xr = self.src_reg(x, S0);
                let ys = self.opd_src(y, S1);
                let (tt, tf) = if k { (jt, skip) } else { (skip, jt) };
                let lt = self.edge(self.rr, tt, poll && tt == jt);
                let lf = self.edge(self.rr, tf, poll && tf == jt);
                self.cx.br(cc, xr, ys, lt);
                self.cx.op(Op::Jmp(lf));
            }
            Act::Test {
                a,
                k,
                jt,
                skip,
                poll,
                st,
            } => {
                // truthy -> `jt` when k != 0, else `skip` (tier 1's `cond_labels`)
                let (tt, tf) = if k { (jt, skip) } else { (skip, jt) };
                let lt = self.edge(self.rr, tt, poll && tt == jt);
                let lf = self.edge(self.rr, tf, poll && tf == jt);
                match st {
                    Some(true) => self.cx.op(Op::Jmp(lt)),
                    Some(false) => self.cx.op(Op::Jmp(lf)),
                    None => {
                        self.spill(a);
                        self.cx.ld(S0, W::B1, fr(a, 0));
                        self.cx.br(Cc::Eq, S0, Src::Imm(TAG_FALSE), lf);
                        self.cx.op(Op::And(S0, Src::Imm(0x0F)));
                        self.cx.br(Cc::Eq, S0, Src::Imm(0), lf);
                        self.cx.op(Op::Jmp(lt));
                    }
                }
            }
            Act::ForLoop { a, t, fall } => {
                let c = self.src_reg(a, S0);
                let s = self.src_reg(a + 1, S1);
                let l = self.src_reg(a + 2, S2);
                let lf = self.edge(self.rr, fall, false);
                self.cx.br(Cc::Eq, c, Src::Imm(0), lf);
                self.cx.op(Op::Sub(c, Src::Imm(1)));
                if self.asg.of(a).is_none() {
                    self.cx.st(W::B8, fr(a, 8), c);
                }
                self.cx.op(Op::Add(l, Src::V(s)));
                if self.asg.of(a + 2).is_none() {
                    self.cx.st(W::B8, fr(a + 2, 8), l);
                }
                let lb = self.edge(self.rr, t, true);
                self.cx.op(Op::Jmp(lb));
            }
        }
    }

    fn emit_bin(&mut self, a: u32, op: AOp, x: u32, y: Opd) {
        let d = self.def_reg(a);
        let xr = self.src_reg(x, S0);
        let ys = self.opd_src(y, S1);
        let mk = |v: V, s: Src| match op {
            AOp::Add => Op::Add(v, s),
            AOp::Sub => Op::Sub(v, s),
            AOp::Mul => Op::Mul(v, s),
            AOp::And => Op::And(v, s),
            AOp::Or => Op::Or(v, s),
            AOp::Xor => Op::Xor(v, s),
        };
        if xr == d {
            self.cx.op(mk(d, ys));
        } else if ys == Src::V(d) {
            if op == AOp::Sub {
                self.cx.op(Op::MovR(S0, xr));
                self.cx.op(Op::Sub(S0, ys));
                self.cx.op(Op::MovR(d, S0));
            } else {
                // commutative: d = y; d op= x
                self.cx.op(mk(d, Src::V(xr)));
            }
        } else {
            self.cx.op(Op::MovR(d, xr));
            self.cx.op(mk(d, ys));
        }
        self.commit_int(a, d);
    }

    /// The fallback protocol (docs 4.8): flush everything, the tier-1 template with the successors' `pcl` entries
    /// remapped to the stubs, the continuation through `stub(next)`.
    fn emit_fall(&mut self, pc: u32) {
        let (h, e) = (self.plan.head, self.plan.end);
        self.flush_stores(self.rr);
        if !super::mutants::active("fall_no_budget") {
            self.store_budget();
        }
        self.rr = SlotSet::EMPTY;
        if self.env.kinds[pc as usize] == Kind::Stub {
            // tier 1's exit stub for this pc ("flush, then leave at the instruction")
            let t = self.cx.pcl[pc as usize];
            self.cx.op(Op::Jmp(t));
            return;
        }
        let op = self.op_at(pc).expect("opcode");
        let ins = self.ins(pc);
        let saved: Vec<L> = self.cx.pcl[h as usize..=e as usize].to_vec();
        for q in h..=e {
            self.cx.pcl[q as usize] = self.stub_l[(q - h) as usize];
        }
        self.cx.ty = [crate::lower::Ty::Unk; 256];
        self.cx.capture_rejoin = true;
        let (ops0, cold0) = (self.cx.p.ops.len(), self.cx.cold.len());
        self.cx.template(pc, ins, op);
        self.cx.capture_rejoin = false;
        self.cx.pcl[h as usize..=e as usize].copy_from_slice(&saved);
        let f = self.follow(pc);
        let tgt = if self.in_region(f) {
            self.stub_l[(f - h) as usize]
        } else {
            self.tier1_label(f)
        };
        self.cx.op(Op::Jmp(tgt));
        // which stubs does the template (and its out-of-line slow paths) refer to?
        let stub_pc: HashMap<L, u32> = (h..=e)
            .map(|q| (self.stub_l[(q - h) as usize], q))
            .collect();
        let mut refs: Vec<L> = Vec::new();
        for o in &self.cx.p.ops[ops0..] {
            match *o {
                Op::Jmp(l)
                | Op::Br { to: l, .. }
                | Op::SubJl { to: l, .. }
                | Op::FBr { to: l, .. }
                | Op::FBrX { to: l, .. }
                | Op::BrM { to: l, .. }
                | Op::BrMi { to: l, .. }
                | Op::CountSub { exit: l, .. }
                | Op::F2IFloor { to: l, .. }
                | Op::F2IExact { to: l, .. } => refs.push(l),
                _ => {}
            }
        }
        for c in &self.cx.cold[cold0..] {
            if let crate::templates_alloc::Cold::Slow { .. } = c {
                refs.extend(c.rejoin_label());
            }
            if let crate::templates_alloc::Cold::Cmp { lt, lf, .. } = *c {
                refs.extend([lt, lf]);
            }
        }
        for l in refs {
            if let Some(&q) = stub_pc.get(&l) {
                // the register dataflow assumed exactly these stub entries (soundness: a stub loads `RR(q)`, which is
                // a subset of what it guards only for pcs the dataflow knew to be stub-entered)
                if !(self.fall_targets(pc).contains(&q) || q == self.follow(pc)) {
                    self.refuse(format!(
                        "the tier-1 template at pc {pc} refers to the stub of pc {q}, unknown to the register dataflow"
                    ));
                    return;
                }
                self.need_stub.insert(q);
            }
        }
    }
}
