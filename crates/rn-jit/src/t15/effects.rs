//! Per-opcode tables: register def/use sets, instruction classes and type transfer functions.
//!
//! Everything that depends on an opcode lives here so that a rule change touches one place:
//! [`effects`] (exact register reads/writes, incl. implicit windows), [`classify`] (doc 3.6 classes) and
//! [`step`] (the transfer function over [`State`]). Register conventions follow the VM in
//! `rn-core/src/vm_unsafe.rs`, not the (partly stale) comments of `lopcodes.h`.

use super::intrinsics::{self, Intr};
use super::{Disp, EscapedMode, FuncInfo, InsClass, Policy, SlotSet, Speculation, TagSet};
use rn_core::instruction::Instruction;
use rn_core::ver::active::OpCode;

/// Registers read, written for sure, and possibly written / left as garbage.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Effects {
    /// Read (open ranges extend to `max_stack`).
    pub uses: SlotSet,
    /// Written on every path through the instruction (kills liveness).
    pub defs: SlotSet,
    /// Written on some path or clobbered (callee frames, `CONCAT` temporaries): no kill, fact becomes the
    /// instruction's result or `Any`.
    pub may_defs: SlotSet,
}

/// Operand of an arithmetic instruction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Opnd {
    /// A register.
    Reg(u32),
    /// A constant (index).
    K(u32),
    /// A small integer immediate.
    Imm,
}

/// Arithmetic result rules (see [`pair`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ar {
    /// `+ - *`: int op int -> int, otherwise float.
    AddSubMul,
    /// `/`: float.
    Div,
    /// `^`: float (via a helper in tier 1).
    Pow,
    /// `// %`: int op int -> int, otherwise float.
    IdivMod,
    /// `& | ~ << >>`: int.
    Bit,
}

/// Decode an arithmetic instruction: `(rule, first operand, second operand)`.
pub fn arith(op: OpCode, i: Instruction) -> Option<(Ar, Opnd, Opnd)> {
    use OpCode as O;
    let (b, c) = (Opnd::Reg(i.b()), Opnd::Reg(i.c()));
    let ck = Opnd::K(i.c());
    Some(match op {
        O::Add | O::Sub | O::Mul => (Ar::AddSubMul, b, c),
        O::Addi => (Ar::AddSubMul, b, Opnd::Imm),
        O::Addk | O::Subk | O::Mulk => (Ar::AddSubMul, b, ck),
        O::Mod | O::Idiv => (Ar::IdivMod, b, c),
        O::Modk | O::Idivk => (Ar::IdivMod, b, ck),
        O::Div => (Ar::Div, b, c),
        O::Divk => (Ar::Div, b, ck),
        O::Pow => (Ar::Pow, b, c),
        O::Powk => (Ar::Pow, b, ck),
        O::Band | O::Bor | O::Bxor | O::Shl | O::Shr => (Ar::Bit, b, c),
        O::Bandk | O::Bork | O::Bxork => (Ar::Bit, b, ck),
        O::Shli => (Ar::Bit, Opnd::Imm, b),
        O::Shri => (Ar::Bit, b, Opnd::Imm),
        _ => return None,
    })
}

/// Result of one operand pair of single classes. Non-numbers give bottom: every region template exits on
/// them (the metamethod / string-coercion path belongs to tier 1, doc 3.4).
pub fn pair(k: Ar, a: TagSet, b: TagSet) -> TagSet {
    if !a.subset(TagSet::NUM) || !b.subset(TagSet::NUM) {
        return TagSet::EMPTY;
    }
    let both_int = a == TagSet::INT && b == TagSet::INT;
    match k {
        Ar::AddSubMul | Ar::IdivMod if both_int => TagSet::INT,
        Ar::AddSubMul | Ar::IdivMod | Ar::Div | Ar::Pow => TagSet::FLT,
        Ar::Bit => TagSet::INT,
    }
}

/// Result set of an arithmetic rule over operand sets (union over all class pairs: monotone).
pub fn arith_result(k: Ar, a: TagSet, b: TagSet) -> TagSet {
    let mut r = TagSet::EMPTY;
    for x in a.bits() {
        for y in b.bits() {
            r = r.join(pair(k, x, y));
        }
    }
    r
}

fn rng(s: &mut SlotSet, lo: u32, hi: u32, max: u32) {
    s.insert_range(lo, hi.min(max));
}

/// Register effects of the (normal-role) instruction at `pc`. Open windows (`B == 0`, `C == 0`) extend to
/// `max_stack` (sound over-approximation of `top`).
pub fn effects(info: &FuncInfo<'_>, pc: usize) -> Effects {
    use OpCode as O;
    let i = Instruction(info.code[pc]);
    let max = info.max_stack;
    let (a, b, c) = (i.a(), i.b(), i.c());
    let mut e = Effects::default();
    let Some(op) = i.opcode() else {
        e.uses = SlotSet::first(max);
        return e;
    };
    if let Some((_, x, y)) = arith(op, i) {
        for o in [x, y] {
            if let Opnd::Reg(r) = o {
                e.uses.insert(r);
            }
        }
        e.defs.insert(a);
        return e;
    }
    let (u, d, m) = (&mut e.uses, &mut e.defs, &mut e.may_defs);
    match op {
        O::Move => {
            u.insert(b);
            d.insert(a);
        }
        O::Loadi | O::Loadf | O::Loadk | O::Loadkx | O::Loadfalse | O::Lfalseskip | O::Loadtrue => {
            d.insert(a)
        }
        O::Loadnil => rng(d, a, a + b + 1, max),
        O::Getupval | O::Gettabup | O::Newtable | O::Closure => d.insert(a),
        O::Setupval => {
            u.insert(a);
            // `debug.upvaluejoin(f, k, g, 1)` can make upvalue `k` alias a slot of this very frame (an open
            // upvalue of a closure created here): SETUPVAL may write any escaped slot (review finding 4).
            m.union_with(&info.escaped);
        }
        O::Gettable => {
            u.insert(b);
            u.insert(c);
            d.insert(a);
        }
        O::Geti | O::Getfield => {
            u.insert(b);
            d.insert(a);
        }
        O::Settabup => {
            if i.k() == 0 {
                u.insert(c);
            }
        }
        O::Settable => {
            u.insert(a);
            u.insert(b);
            if i.k() == 0 {
                u.insert(c);
            }
        }
        O::Seti | O::Setfield => {
            u.insert(a);
            if i.k() == 0 {
                u.insert(c);
            }
        }
        O::SelfOp => {
            u.insert(b);
            #[cfg(rustynail_lua54)]
            if i.k() == 0 {
                u.insert(c);
            }
            d.insert(a);
            d.insert(a + 1);
        }
        O::Mmbin => {
            u.insert(a);
            u.insert(b);
            if pc > 0 {
                d.insert(Instruction(info.code[pc - 1]).a());
            }
        }
        O::Mmbini | O::Mmbink => {
            u.insert(a);
            if pc > 0 {
                d.insert(Instruction(info.code[pc - 1]).a());
            }
        }
        O::Unm | O::Bnot | O::Not | O::Len => {
            u.insert(b);
            d.insert(a);
        }
        O::Concat => {
            rng(u, a, a + b, max);
            d.insert(a);
            rng(m, a + 1, a + b, max);
        }
        O::Close => rng(u, a, max, max),
        O::Tbc => u.insert(a),
        O::Jmp | O::Extraarg | O::Varargprep => {}
        O::Eq | O::Lt | O::Le => {
            u.insert(a);
            u.insert(b);
        }
        O::Eqk | O::Eqi | O::Lti | O::Lei | O::Gti | O::Gei | O::Test => u.insert(a),
        O::Testset => {
            u.insert(b);
            m.insert(a);
        }
        O::Call => {
            if b == 0 {
                rng(u, a, max, max);
            } else {
                rng(u, a, a + b, max);
            }
            if c >= 2 {
                rng(d, a, a + c - 1, max);
            }
            rng(m, a, max, max);
        }
        O::Tailcall => {
            if b == 0 {
                rng(u, a, max, max);
            } else {
                rng(u, a, a + b, max);
            }
        }
        O::Return => match b {
            0 => rng(u, a, max, max),
            1 => {}
            _ => rng(u, a, a + b - 1, max),
        },
        O::Return0 => {}
        O::Return1 => u.insert(a),
        O::Setlist => {
            u.insert(a);
            #[cfg(not(rustynail_lua54))]
            let n = i.vb();
            #[cfg(rustynail_lua54)]
            let n = b;
            if n == 0 {
                rng(u, a, max, max);
            } else {
                rng(u, a, a + n + 1, max);
            }
        }
        O::Vararg => {
            #[cfg(not(rustynail_lua54))]
            if i.k() != 0 {
                u.insert(b);
            }
            if c >= 2 {
                rng(d, a, a + c - 1, max);
            } else if c == 0 {
                rng(m, a, max, max);
            }
        }
        _ => {
            // for loops, generic for and the 5.5 named-vararg ops: layout depends on the version
            effects_loops(op, i, max, &mut e);
        }
    }
    e
}

#[cfg(not(rustynail_lua54))]
fn effects_loops(op: OpCode, i: Instruction, max: u32, e: &mut Effects) {
    use OpCode as O;
    let (a, b, c) = (i.a(), i.b(), i.c());
    match op {
        O::Forprep => {
            rng(&mut e.uses, a, a + 3, max);
            rng(&mut e.defs, a, a + 3, max);
        }
        O::Forloop => {
            rng(&mut e.uses, a, a + 3, max);
            e.defs.insert(a + 2);
            e.may_defs.insert(a);
        }
        O::Tforprep => {
            e.uses.insert(a + 2);
            e.uses.insert(a + 3);
            e.defs.insert(a + 2);
            e.defs.insert(a + 3);
        }
        O::Tforcall => {
            for s in [a, a + 1, a + 3] {
                e.uses.insert(s);
            }
            rng(&mut e.defs, a + 3, a + 3 + c, max);
            rng(&mut e.may_defs, a + 3, max, max);
        }
        O::Tforloop => e.uses.insert(a + 3),
        O::Getvarg => {
            e.uses.insert(b);
            e.uses.insert(c);
            e.defs.insert(a);
        }
        O::Errnnil => e.uses.insert(a),
        _ => e.uses = SlotSet::first(max),
    }
}

#[cfg(rustynail_lua54)]
fn effects_loops(op: OpCode, i: Instruction, max: u32, e: &mut Effects) {
    use OpCode as O;
    let (a, c) = (i.a(), i.c());
    match op {
        O::Forprep => {
            rng(&mut e.uses, a, a + 3, max);
            rng(&mut e.defs, a, a + 4, max);
        }
        O::Forloop => {
            rng(&mut e.uses, a, a + 3, max);
            e.may_defs.insert(a);
            e.may_defs.insert(a + 1);
            e.defs.insert(a + 3);
        }
        O::Tforprep => e.uses.insert(a + 3),
        O::Tforcall => {
            for s in [a, a + 1, a + 2] {
                e.uses.insert(s);
            }
            rng(&mut e.defs, a + 4, a + 4 + c, max);
            rng(&mut e.may_defs, a + 4, max, max);
        }
        O::Tforloop => {
            e.uses.insert(a + 4);
            e.may_defs.insert(a + 2);
        }
        _ => e.uses = SlotSet::first(max),
    }
}

/// Static class of the instruction (doc 3.6), before facts. `fallback_g` sites turn table ops into class G.
pub fn classify(op: OpCode, pc: u32, policy: &Policy) -> InsClass {
    use OpCode as O;
    match op {
        O::Gettabup
        | O::Gettable
        | O::Geti
        | O::Getfield
        | O::SelfOp
        | O::Settabup
        | O::Settable
        | O::Seti
        | O::Setfield => {
            if policy.fallback_g.contains(&pc) {
                InsClass::FallbackG
            } else {
                InsClass::Table
            }
        }
        O::Len | O::Pow | O::Powk => InsClass::FallbackN,
        O::Newtable
        | O::Setlist
        | O::Concat
        | O::Closure
        | O::Vararg
        | O::Close
        | O::Tbc
        | O::Tforprep => InsClass::FallbackG,
        O::Call if policy.intr.iter().any(|&(p, _)| p == pc) => InsClass::Intr,
        O::Call | O::Tailcall | O::Return | O::Return0 | O::Return1 | O::Tforcall => InsClass::Call,
        O::Loadkx | O::Varargprep => InsClass::Exit,
        O::Mmbin | O::Mmbini | O::Mmbink | O::Extraarg => InsClass::Follower,
        #[cfg(not(rustynail_lua54))]
        O::Getvarg | O::Errnnil => InsClass::Exit,
        _ => InsClass::Typed,
    }
}

/// Is the opcode an arithmetic-like op that is followed by `MMBIN*` in the bytecode?
pub fn has_mmbin_follower(op: OpCode) -> bool {
    arith(op, Instruction(0)).is_some() && op != OpCode::Mmbin
}

/// Compare / test ops that are followed by a `JMP` and skip it or take it.
pub fn is_cond(op: OpCode) -> bool {
    use OpCode as O;
    matches!(
        op,
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
            | O::Testset
    )
}

/// Ops followed by an `EXTRAARG` (`SETLIST` only with the `k` bit).
pub fn has_extra_follower(op: OpCode, i: Instruction) -> bool {
    match op {
        OpCode::Loadkx | OpCode::Newtable => true,
        OpCode::Setlist => i.k() != 0,
        _ => false,
    }
}

/// Dataflow state: a type fact per slot, and the set of slots whose VM-stack tag byte is known to equal it.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct State {
    /// Fact per slot (`max_stack` entries).
    pub t: Vec<TagSet>,
    /// Slots whose memory tag byte equals the fact (M1).
    pub mem: SlotSet,
}

impl State {
    /// All slots `Any`, nothing known about memory.
    pub fn any(n: usize) -> State {
        State {
            t: vec![TagSet::ANY; n],
            mem: SlotSet::EMPTY,
        }
    }
    /// Join `o` into `self` (facts OR, memory knowledge AND); true if `self` changed.
    pub fn join_from(&mut self, o: &State) -> bool {
        let mut ch = false;
        for (x, y) in self.t.iter_mut().zip(&o.t) {
            let j = x.join(*y);
            ch |= j != *x;
            *x = j;
        }
        let mut m = self.mem.meet(&o.mem);
        for s in m.iter().collect::<Vec<_>>() {
            if !self.t.get(s as usize).is_some_and(|t| t.is_singleton()) {
                m.remove(s); // memory knowledge only describes singleton facts
            }
        }
        ch |= m != self.mem;
        self.mem = m;
        ch
    }
    /// The state at a resume point `P` (docs/JIT_TIER15.md section 13, T2b): `stub(P)` can be entered from
    /// outside the region and checks only the singleton facts of the guard set `g`; every other fact may be
    /// false there. Guard-set slots with a non-singleton or bottom fact become `Any` (nothing is checked for
    /// them), slots outside the set keep their (unreliable, never consumed before a later re-guard) fact.
    /// Returns the guard set with singleton facts: what the stub checks and hence what memory is known for.
    pub fn clamp_to_guards(&mut self, g: &SlotSet) -> SlotSet {
        let mut checked = SlotSet::EMPTY;
        for s in g.iter() {
            let Some(t) = self.t.get_mut(s as usize) else {
                continue;
            };
            if t.is_singleton() {
                checked.insert(s);
            } else {
                *t = TagSet::ANY;
            }
        }
        checked
    }
    fn set(&mut self, s: u32, t: TagSet, in_memory: bool) {
        let s = s as usize;
        if s >= self.t.len() {
            return;
        }
        let keep = self.mem.contains(s as u32) && self.t[s] == t && t.is_singleton();
        let known = t.is_singleton() && (in_memory || t.is_collectable() || keep);
        self.t[s] = t;
        if known {
            self.mem.insert(s as u32);
        } else {
            self.mem.remove(s as u32);
        }
    }
    fn get(&self, s: u32) -> TagSet {
        self.t.get(s as usize).copied().unwrap_or(TagSet::ANY)
    }
}

/// Inputs of [`step`].
pub struct StepCtx<'a> {
    /// The function.
    pub info: &'a FuncInfo<'a>,
    /// Speculation inputs.
    pub spec: &'a Speculation<'a>,
    /// Tunables.
    pub policy: &'a Policy,
    /// Upward-exposed typed uses at a pc (what the stub there guards).
    pub guard_live: &'a dyn Fn(u32) -> SlotSet,
}

/// What [`step`] reports besides the new state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StepOut {
    /// Compilation mode.
    pub disp: Disp,
    /// Mandatory def guard (see [`super::InsnPlan::guard_def`]).
    pub guard_def: Option<TagSet>,
}

fn opnd_set(ctx: &StepCtx<'_>, st: &State, o: Opnd) -> TagSet {
    match o {
        Opnd::Reg(r) => st.get(r),
        Opnd::K(k) => ctx.info.k.get(k as usize).copied().unwrap_or(TagSet::ANY),
        Opnd::Imm => TagSet::INT,
    }
}

/// Speculated result of a load at `pc` (profile byte joined with the respecialisation widening).
fn load_spec(ctx: &StepCtx<'_>, pc: u32) -> TagSet {
    let mut t = match ctx.spec.prof.get(pc as usize) {
        Some(&b) if b != 0xFF => TagSet::of_tag(b),
        _ => TagSet::ANY,
    };
    for &(p, w) in ctx.spec.def_widen {
        if p == pc {
            t = t.join(w);
        }
    }
    t
}

/// Disp from operand sets: any bottom -> Dead; all singleton -> Specialised; all within num -> Dual; else Local.
fn disp_of(sets: &[TagSet]) -> Disp {
    if sets.iter().any(|s| s.is_empty()) {
        Disp::Dead
    } else if sets.iter().all(|s| s.is_singleton()) {
        Disp::Specialised
    } else if sets.iter().all(|s| s.subset(TagSet::NUM)) {
        Disp::Dual
    } else {
        Disp::Local
    }
}

/// The argument slots of an intrinsic call (`A+1 .. A+B-1`): the typed uses of the inline form. The callee slot is
/// compared at run time (identity), never typed.
pub fn intr_uses(i: Instruction, max: u32) -> SlotSet {
    let mut s = SlotSet::EMPTY;
    let (a, b) = (i.a(), i.b());
    if b >= 2 {
        rng(&mut s, a + 1, a + b, max);
    }
    s
}

/// The pc control continues at after a barrier-like instruction falls through (`P+1`; the `EXTRAARG` of
/// `NEWTABLE` / `SETLIST k` is skipped, `TFORPREP` jumps to its `TFORCALL`).
pub fn continuation(info: &FuncInfo<'_>, pc: u32) -> u32 {
    let i = Instruction(info.code[pc as usize]);
    match i.opcode() {
        Some(OpCode::Tforprep) => pc + 1 + i.bx(),
        Some(op) if has_extra_follower(op, i) => pc + 2,
        _ => pc + 1,
    }
}

/// The transfer function of the instruction at `pc` (a normal-role pc), applied to `st`.
/// `class` is the already-classified static class.
///
/// Rules added by the T2b soundness review (docs/JIT_TIER15.md section 13): a typed instruction with a
/// bottom operand is `Dead`; a table access whose disposition is [`Disp::LocalBarrier`] is a barrier from
/// here on (facts are clamped to its continuation's guard set, memory knowledge reset).
pub fn step(ctx: &StepCtx<'_>, pc: u32, class: InsClass, st: &mut State) -> StepOut {
    let eff = effects(ctx.info, pc as usize);
    let bot_use = match class {
        InsClass::Typed | InsClass::Table => eff.uses.iter().any(|s| st.get(s).is_empty()),
        InsClass::Intr => intr_uses(Instruction(ctx.info.code[pc as usize]), ctx.info.max_stack)
            .iter()
            .any(|s| st.get(s).is_empty()),
        _ => false,
    };
    let mut out = step_core(ctx, pc, class, st, &eff);
    if bot_use {
        out.disp = Disp::Dead;
    }
    let mut cls = class;
    if out.disp == Disp::LocalBarrier {
        cls = InsClass::FallbackG;
        if ctx.policy.escaped == EscapedMode::ToAny {
            for s in ctx.info.escaped.iter() {
                st.set(s, TagSet::ANY, false);
            }
        }
    }
    finish(ctx, pc, cls, st, out)
}

fn step_core(
    ctx: &StepCtx<'_>,
    pc: u32,
    class: InsClass,
    st: &mut State,
    eff: &Effects,
) -> StepOut {
    use OpCode as O;
    let i = Instruction(ctx.info.code[pc as usize]);
    let (a, b) = (i.a(), i.b());
    let op = i.opcode().expect("validated by the CFG builder");
    let mut out = StepOut {
        disp: Disp::Specialised,
        guard_def: None,
    };

    // (read before the clobber of `may_defs` below overwrites the argument facts)
    let intr_args: Option<(Intr, [TagSet; 2])> = (op == O::Call && class == InsClass::Intr)
        .then(|| ctx.policy.intr.iter().find(|&&(p, _)| p == pc))
        .flatten()
        .map(|&(_, k)| (k, [st.get(a + 1), st.get(a + 2)]));
    // Barrier ops: facts of escaped slots are dropped in ToAny mode (Reguard keeps them: the stub re-checks).
    if (class.is_barrier() || class == InsClass::Intr) && ctx.policy.escaped == EscapedMode::ToAny {
        for s in ctx.info.escaped.iter() {
            st.set(s, TagSet::ANY, false);
        }
    }
    let in_mem = matches!(
        class,
        InsClass::FallbackN | InsClass::FallbackG | InsClass::Call
    );

    // 1. clobbers first (garbage above call results etc.), then the precise defs overwrite them
    // (the numeric-for counter is only re-written by FORLOOP, its memory tag byte stays valid: skip it)
    if op != O::Forloop {
        for s in eff.may_defs.iter() {
            st.set(s, TagSet::ANY, false);
        }
    }

    if let Some((k, x, y)) = arith(op, i) {
        let (sx, sy) = (opnd_set(ctx, st, x), opnd_set(ctx, st, y));
        if class == InsClass::FallbackN {
            // `jit_pow`: the helper coerces / converts its operands itself (they are not typed uses, so their
            // facts are not guarded) and writes a float or does not return `H_DONE`: the result is `FLT`
            // whatever the operand facts say (review finding 3, POW variant). Never `Dead`.
            out.disp = Disp::Opaque;
            if ctx.policy.guard_n_defs {
                out.guard_def = Some(TagSet::FLT);
                st.set(a, TagSet::FLT, true);
            } else {
                st.set(a, TagSet::ANY, true);
            }
            return out;
        }
        let r = arith_result(k, sx, sy);
        out.disp = if r.is_empty() || sx.is_empty() || sy.is_empty() {
            Disp::Dead
        } else {
            disp_of(&[sx, sy])
        };
        st.set(a, r, false);
        return out;
    }

    match op {
        O::Move => {
            let v = st.get(b);
            st.set(a, v, false);
        }
        O::Loadi => st.set(a, TagSet::INT, false),
        O::Loadf => st.set(a, TagSet::FLT, false),
        O::Loadk => {
            let t = ctx
                .info
                .k
                .get(i.bx() as usize)
                .copied()
                .unwrap_or(TagSet::ANY);
            st.set(a, t, false);
        }
        O::Loadfalse | O::Lfalseskip => st.set(a, TagSet::FALSE, false),
        O::Loadtrue => st.set(a, TagSet::TRUE, false),
        O::Loadnil => {
            for s in a..a + b + 1 {
                st.set(s, TagSet::NIL, false);
            }
        }
        O::Getupval | O::Gettabup | O::Gettable | O::Geti | O::Getfield => {
            let t = load_spec(ctx, pc);
            if op == O::Gettable || op == O::Geti || op == O::Getfield {
                out.disp = table_disp(ctx, st, op, i, class);
            } else if op == O::Gettabup {
                out.disp = if class == InsClass::Table {
                    Disp::Specialised
                } else {
                    Disp::Opaque
                };
            }
            if !t.is_any() {
                out.guard_def = Some(t);
            }
            st.set(a, t, class == InsClass::FallbackG);
        }
        O::SelfOp => {
            let t = load_spec(ctx, pc);
            let recv = st.get(b);
            out.disp = table_disp(ctx, st, op, i, class);
            if !t.is_any() {
                out.guard_def = Some(t);
            }
            // the receiver fact is only reliable when the typed form guards it (class Table)
            st.set(
                a + 1,
                if class == InsClass::Table {
                    recv
                } else {
                    TagSet::ANY
                },
                false,
            );
            st.set(a, t, class == InsClass::FallbackG);
        }
        O::Settabup | O::Settable | O::Seti | O::Setfield => {
            out.disp = table_disp(ctx, st, op, i, class);
        }
        O::Newtable => st.set(a, TagSet::TABLE, in_mem),
        O::Closure => st.set(a, TagSet::LCL, in_mem),
        O::Unm => {
            let s = st.get(b);
            let r = s.meet(TagSet::NUM);
            out.disp = disp_of(&[s]);
            if r.is_empty() {
                out.disp = Disp::Dead;
            }
            st.set(a, r, false);
        }
        O::Bnot => {
            let s = st.get(b);
            let r = if s.meet(TagSet::NUM).is_empty() {
                TagSet::EMPTY
            } else {
                TagSet::INT
            };
            out.disp = if r.is_empty() {
                Disp::Dead
            } else {
                disp_of(&[s])
            };
            st.set(a, r, false);
        }
        O::Not => st.set(a, TagSet::BOOL, false),
        O::Len => {
            // `jit_len` (class N): the operand fact is not a guarded use, so nothing is concluded from it; the
            // result is behind a mandatory guard (a `__len` may return anything).
            out.disp = Disp::Opaque;
            if ctx.policy.guard_n_defs {
                out.guard_def = Some(TagSet::INT);
                st.set(a, TagSet::INT, true);
            } else {
                st.set(a, TagSet::ANY, true);
            }
        }
        O::Concat => st.set(a, TagSet::ANY, true), // `__concat` may return anything
        O::Eq | O::Lt | O::Le => out.disp = disp_of(&[st.get(a), st.get(b)]),
        O::Eqk => out.disp = disp_of(&[st.get(a), opnd_set(ctx, st, Opnd::K(b))]),
        O::Eqi | O::Lti | O::Lei | O::Gti | O::Gei => out.disp = disp_of(&[st.get(a)]),
        O::Test => out.disp = test_disp(st.get(a)),
        O::Jmp => {}
        O::Testset => {
            out.disp = test_disp(st.get(b));
            let v = st.get(a).join(st.get(b)); // only the taken edge assigns; join both
            st.set(a, v, false);
        }
        O::Call | O::Tforcall | O::Vararg | O::Setlist | O::Close | O::Tbc => {
            // results (if any) are Any: already set by the may_defs clobber, now the must-defs
            for s in eff.defs.iter() {
                st.set(s, TagSet::ANY, true);
            }
            out.disp = Disp::Opaque;
            if op == O::Call && i.c() >= 2 {
                // the first result is speculated from the profile (the continuation's stub guards it if it is read
                // as a typed operand later); an intrinsic call with typed operands computes it inline
                let spec = load_spec(ctx, pc);
                let mut res = None;
                if let Some((kind, args)) = intr_args {
                    let n = kind.nargs() as usize;
                    res = (i.b() == kind.nargs() + 1 && i.c() == 2)
                        .then(|| intrinsics::result_set(kind, &args[..n], spec))
                        .flatten();
                    out.disp = if res.is_some() {
                        Disp::Specialised
                    } else {
                        Disp::LocalBarrier
                    };
                }
                if let Some(r) = res {
                    out.guard_def = Some(r);
                    st.set(a, r, false);
                } else if spec.is_singleton() {
                    out.guard_def = Some(spec);
                    st.set(a, spec, true);
                }
            } else if intr_args.is_some() {
                out.disp = Disp::LocalBarrier;
            }
        }
        _ => step_loops(op, i, st, eff, &mut out),
    }
    out
}

/// `TEST` / `TESTSET`: a singleton fact folds to one compare, a Bool fact is a two-way flag test, anything else
/// goes through the tier-1 template (falsy = nil or false).
fn test_disp(s: TagSet) -> Disp {
    if s.is_empty() {
        Disp::Dead
    } else if s.is_singleton() {
        Disp::Specialised
    } else if s.subset(TagSet::BOOL) {
        Disp::Dual
    } else {
        Disp::Local
    }
}

fn table_disp(ctx: &StepCtx<'_>, st: &State, op: OpCode, i: Instruction, class: InsClass) -> Disp {
    use OpCode as O;
    if class != InsClass::Table {
        return Disp::Opaque;
    }
    let tbl = match op {
        O::Gettable | O::Geti | O::Getfield | O::SelfOp => st.get(i.b()),
        O::Settable | O::Seti | O::Setfield => st.get(i.a()),
        _ => TagSet::TABLE, // GETTABUP/SETTABUP: the upvalue is guarded at the def / hoist
    };
    let mut sets = vec![tbl];
    match op {
        O::Gettable => sets.push(st.get(i.c())),
        O::Settable => {
            sets.push(st.get(i.b()));
        }
        _ => {}
    }
    if matches!(op, O::Settabup | O::Settable | O::Seti | O::Setfield) && i.k() == 0 {
        sets.push(st.get(i.c()));
    }
    let _ = ctx;
    if sets.iter().any(|s| s.is_empty()) {
        Disp::Dead
    } else if tbl == TagSet::TABLE && sets.iter().all(|s| s.is_singleton()) {
        Disp::Specialised
    } else {
        // the tier-1 template calls `jit_get` / `jit_set` on its cold paths (insertion, rehash, GC step,
        // finalizers): not a pure local fallback (B1), a barrier op
        Disp::LocalBarrier
    }
}

#[cfg(not(rustynail_lua54))]
fn step_loops(op: OpCode, i: Instruction, st: &mut State, eff: &Effects, out: &mut StepOut) {
    use OpCode as O;
    let a = i.a();
    match op {
        O::Forprep => {
            let (init, limit, step) = (st.get(a), st.get(a + 1), st.get(a + 2));
            let r = for_ctl(init, limit, step);
            out.disp = forprep_disp(r, init, limit, step);
            for s in 0..3 {
                st.set(a + s, r, false);
            }
        }
        O::Forloop => {
            // the step slot keeps its type; count / limit and the control variable follow it (A6: the control
            // slot is read by the VM whatever its tag, it is in `uses` and hence in the stub guard set)
            let r = st.get(a + 1).meet(TagSet::NUM);
            out.disp = if r.is_empty() {
                Disp::Dead
            } else {
                disp_of(&[st.get(a + 1)])
            };
            st.set(a, r, false);
            st.set(a + 2, r, false);
        }
        O::Tforprep => {
            // class G: its operand facts are not guarded uses, so the swapped registers become `Any`
            st.set(a + 2, TagSet::ANY, true);
            st.set(a + 3, TagSet::ANY, true);
            out.disp = Disp::Opaque;
        }
        O::Tforloop => {}
        _ => {
            for s in eff.defs.iter().chain(eff.may_defs.iter()) {
                st.set(s, TagSet::ANY, false);
            }
            out.disp = Disp::Opaque;
        }
    }
}

#[cfg(rustynail_lua54)]
fn step_loops(op: OpCode, i: Instruction, st: &mut State, eff: &Effects, out: &mut StepOut) {
    use OpCode as O;
    let a = i.a();
    match op {
        O::Forprep => {
            let (init, limit, step) = (st.get(a), st.get(a + 1), st.get(a + 2));
            let r = for_ctl(init, limit, step);
            out.disp = forprep_disp(r, init, limit, step);
            for s in 0..4 {
                st.set(a + s, r, false);
            }
        }
        O::Forloop => {
            let r = st.get(a + 2).meet(TagSet::NUM);
            out.disp = if r.is_empty() {
                Disp::Dead
            } else {
                disp_of(&[st.get(a + 2)])
            };
            st.set(a, r, false);
            st.set(a + 3, r, false);
        }
        O::Tforloop => {}
        _ => {
            for s in eff.defs.iter().chain(eff.may_defs.iter()) {
                st.set(s, TagSet::ANY, false);
            }
            out.disp = Disp::Opaque;
        }
    }
}

/// `FORPREP` disposition: init and step decide the loop kind; a limit that is not provably a number needs a
/// runtime check (exit) in the typed template, hence dual path.
fn forprep_disp(r: TagSet, init: TagSet, limit: TagSet, step: TagSet) -> Disp {
    if r.is_empty() {
        return Disp::Dead;
    }
    match disp_of(&[init, step]) {
        Disp::Specialised if !limit.subset(TagSet::NUM) => Disp::Dual,
        d => d,
    }
}

/// Type shared by the three internal numeric-`for` registers after `FORPREP`: integer loop when `init` and
/// `step` can both be integers, float loop when either can be anything but an integer (numbers convert, the
/// rest raises). Bottom operands give bottom.
pub fn for_ctl(init: TagSet, limit: TagSet, step: TagSet) -> TagSet {
    if init.is_empty() || limit.is_empty() || step.is_empty() {
        return TagSet::EMPTY;
    }
    let mut r = TagSet::EMPTY;
    if init.meet(TagSet::INT) == TagSet::INT && step.meet(TagSet::INT) == TagSet::INT {
        r = r.join(TagSet::INT);
    }
    if !init.minus(TagSet::INT).is_empty() || !step.minus(TagSet::INT).is_empty() {
        r = r.join(TagSet::FLT);
    }
    r
}

/// Common tail of [`step`]: after a barrier the memory-tag knowledge shrinks to what the next stub re-guards
/// (M1); after a class-N helper to the guarded result only.
fn finish(ctx: &StepCtx<'_>, pc: u32, class: InsClass, st: &mut State, out: StepOut) -> StepOut {
    match class {
        InsClass::FallbackG | InsClass::Call | InsClass::Intr => {
            // the continuation is re-entered through `stub(P+1)` (also from outside the region: the resume
            // table): only its guard set is known there, memory tags equal the facts of exactly that set
            let g = (ctx.guard_live)(continuation(ctx.info, pc));
            st.mem = st.clamp_to_guards(&g);
        }
        InsClass::FallbackN => {
            let i = Instruction(ctx.info.code[pc as usize]);
            let mut m = SlotSet::EMPTY;
            if out.guard_def.is_some_and(TagSet::is_singleton) {
                m.insert(i.a());
            }
            st.mem = m;
        }
        _ => {}
    }
    out
}
