//! Arithmetic, bitwise, comparison, test, upvalue and numeric-`for` templates (J1b).
//!
//! Every template mirrors the fast path of the interpreter (`vm_unsafe.rs`) and *exits at its own
//! instruction before any side effect* for everything else (non-numbers, strings, metamethods, errors such as
//! division by zero, floats where an exact integer conversion would be needed, ...): the interpreter then
//! redoes the instruction. On the fast path an arithmetic instruction skips its MMBIN* follower exactly like
//! the interpreter does, and a conditional jumps itself instead of executing the JMP that follows it.
//!
//! Register-type knowledge (`Cx::ty`) lives inside a block: after a template stores an integer or a float
//! the next instructions of the block skip the tag load and test. It is reset at every block head.

use crate::ir::{Base, Cc, Op, Src, L, M, V, W};
use crate::lower::{fr, Cx, Kind, Ty};
use rn_core::instruction::Instruction;
use rn_core::jit_layout::{lclosure, upval};
use rn_core::value::{
    ctb, Value, BIT_ISCOLLECTABLE, LUA_VFALSE, LUA_VNIL, LUA_VNUMFLT, LUA_VNUMINT, LUA_VSHRSTR,
    LUA_VTABLE, LUA_VTRUE,
};
use rn_core::ver::active::OpCode;

/// What follows a fast-path template in the bytecode and is subsumed by it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Follower {
    /// Nothing.
    None,
    /// An `MMBIN`/`MMBINI`/`MMBINK` (skipped on the fast path).
    Mmbin,
    /// A `JMP` (executed by the conditional itself).
    Jmp,
    /// The `EXTRAARG` operand word of NEWTABLE / SETLIST (read at compile time).
    Extra,
}

pub(crate) fn follower(op: OpCode) -> Follower {
    use OpCode as O;
    match op {
        O::Add
        | O::Sub
        | O::Mul
        | O::Mod
        | O::Div
        | O::Idiv
        | O::Band
        | O::Bor
        | O::Bxor
        | O::Shl
        | O::Shr
        | O::Addi
        | O::Addk
        | O::Subk
        | O::Mulk
        | O::Modk
        | O::Divk
        | O::Idivk
        | O::Bandk
        | O::Bork
        | O::Bxork
        | O::Shli
        | O::Shri => Follower::Mmbin,
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
        | O::Testset => Follower::Jmp,
        _ => Follower::None,
    }
}

/// Classification of the opcodes this file implements (`None` = not ours / exit stub).
pub(crate) fn kind(op: OpCode, i: Instruction, k: &[Value]) -> Option<Kind> {
    use OpCode as O;
    let tag = |idx: u32| k.get(idx as usize).map(Value::tag);
    let knum = |idx: u32| matches!(tag(idx), Some(LUA_VNUMINT | LUA_VNUMFLT));
    let kint = |idx: u32| tag(idx) == Some(LUA_VNUMINT);
    Some(match op {
        O::Add
        | O::Sub
        | O::Mul
        | O::Div
        | O::Mod
        | O::Idiv
        | O::Band
        | O::Bor
        | O::Bxor
        | O::Shl
        | O::Shr
        | O::Addi
        | O::Shli
        | O::Shri
        | O::Unm
        | O::Bnot
        | O::Setupval => Kind::Guard,
        O::Addk | O::Subk | O::Mulk | O::Divk | O::Modk | O::Idivk if knum(i.c()) => Kind::Guard,
        O::Bandk | O::Bork | O::Bxork if kint(i.c()) => Kind::Guard,
        O::Not | O::Getupval => Kind::Plain,
        O::Eq
        | O::Lt
        | O::Le
        | O::Eqi
        | O::Lti
        | O::Lei
        | O::Gti
        | O::Gei
        | O::Test
        | O::Testset
        | O::Forloop
        | O::Forprep => Kind::Branch,
        O::Eqk
            if knum(i.b())
                || matches!(tag(i.b()), Some(LUA_VNIL | LUA_VFALSE | LUA_VTRUE))
                || tag(i.b()) == Some(ctb(LUA_VSHRSTR)) =>
        {
            Kind::Branch
        }
        _ => return None,
    })
}

/// An operand: a register or a known constant.
#[derive(Clone, Copy, Debug)]
enum Opd {
    R(u32),
    I(i64),
    F(f64),
}

/// Numeric type of an operand at run time.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Nt {
    I,
    F,
}

/// Arithmetic operation of an instruction.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Ar {
    Add,
    Sub,
    Mul,
    Div,
    Idiv,
    Mod,
    And,
    Or,
    Xor,
    /// `shiftl(o0, o1)`
    Shl,
    /// `shiftr(o0, o1)` = `shiftl(o0, -o1)`
    Shr,
}

const TAG_FALSE: i32 = LUA_VFALSE as i32;
const TAG_INT: i32 = LUA_VNUMINT as i32;
const TAG_FLT: i32 = LUA_VNUMFLT as i32;
/// Integers within `±2^53` convert to double exactly (mixed int/float comparison).
const EXACT: i64 = 1 << 53;

fn fits32(x: i64) -> bool {
    i32::try_from(x).is_ok()
}

impl Cx<'_> {
    // ---- small helpers -------------------------------------------------------------------------

    fn kopd(&self, idx: u32) -> Opd {
        let v = self.cfg.consts[idx as usize];
        match v.as_int() {
            Some(x) => Opd::I(x),
            None => Opd::F(v.as_float().expect("numeric constant")),
        }
    }

    fn sty(&self, o: Opd) -> Option<Nt> {
        match o {
            Opd::I(_) => Some(Nt::I),
            Opd::F(_) => Some(Nt::F),
            Opd::R(r) => match self.ty[r as usize] {
                Ty::Int => Some(Nt::I),
                Ty::Flt => Some(Nt::F),
                Ty::Unk => None,
            },
        }
    }

    fn st_int(&mut self, a: u32, v: V) {
        if self.ty[a as usize] != Ty::Int {
            self.st_imm(W::B1, fr(a, 0), TAG_INT); // (no tag store when the slot is known to hold an int)
        }
        self.st(W::B8, fr(a, 8), v);
    }

    fn st_flt(&mut self, a: u32, v: V) {
        if self.ty[a as usize] != Ty::Flt {
            self.st_imm(W::B1, fr(a, 0), TAG_FLT);
        }
        self.st(W::B8, fr(a, 8), v);
    }

    /// Integer payload of `o` into `dst`.
    fn ld_i(&mut self, dst: V, o: Opd) {
        match o {
            Opd::R(r) => self.ld(dst, W::B8, fr(r, 8)),
            Opd::I(k) => self.op(Op::Mov(dst, k as u64)),
            Opd::F(_) => unreachable!("float operand in an integer leaf"),
        }
    }

    /// `o` (whose run-time type is `t`) as double bits into `dst`.
    fn ld_f(&mut self, dst: V, o: Opd, t: Nt) {
        match o {
            Opd::R(r) => {
                self.ld(dst, W::B8, fr(r, 8));
                if t == Nt::I {
                    self.op(Op::I2F(dst));
                }
            }
            Opd::I(k) => self.op(Op::Mov(dst, (k as f64).to_bits())),
            Opd::F(f) => self.op(Op::Mov(dst, f.to_bits())),
        }
    }

    /// Second operand of an integer ALU op: an immediate when possible, else loaded into `tmp`.
    fn src_i(&mut self, o: Opd, tmp: V) -> Src {
        match o {
            Opd::I(k) if fits32(k) => Src::Imm(k as i32),
            _ => {
                self.ld_i(tmp, o);
                Src::V(tmp)
            }
        }
    }

    // ---- type dispatch -------------------------------------------------------------------------

    /// Emits the type dispatch over `ops` (at most two): for each combination of numeric types that can
    /// occur, `leaf` emits the code of that case (it ends with a jump to the common end, added here).
    /// A register operand that is neither int nor float jumps to `bad[j]`. Tags of unknown register operands
    /// are loaded into `V<j>`; leaves must only use `V2..V6`. Returns the merged result type.
    fn dispatch(
        &mut self,
        ops: &[Opd],
        bad: &[L],
        leaf: &mut dyn FnMut(&mut Self, &[Nt]) -> Option<Ty>,
    ) -> Ty {
        let done = self.p.label();
        let tys: Vec<Option<Nt>> = ops.iter().map(|o| self.sty(*o)).collect();
        for (j, o) in ops.iter().enumerate() {
            if let (None, Opd::R(r)) = (tys[j], o) {
                self.ld(j as V, W::B1, fr(*r, 0));
            }
        }
        let mut acc = Vec::new();
        let mut res = None;
        self.dispatch_rec(0, &tys, bad, done, &mut acc, leaf, &mut res);
        self.op(Op::Bind(done));
        res.unwrap_or(Ty::Unk)
    }

    #[allow(clippy::too_many_arguments)]
    fn dispatch_rec(
        &mut self,
        j: usize,
        tys: &[Option<Nt>],
        bad: &[L],
        done: L,
        acc: &mut Vec<Nt>,
        leaf: &mut dyn FnMut(&mut Self, &[Nt]) -> Option<Ty>,
        res: &mut Option<Ty>,
    ) {
        if j == tys.len() {
            if let Some(t) = leaf(self, acc) {
                *res = match *res {
                    None => Some(t),
                    Some(o) if o == t => Some(o),
                    Some(_) => Some(Ty::Unk),
                };
            }
            self.op(Op::Jmp(done));
            return;
        }
        match tys[j] {
            Some(t) => {
                acc.push(t);
                self.dispatch_rec(j + 1, tys, bad, done, acc, leaf, res);
                acc.pop();
            }
            None => {
                let notint = self.p.label();
                self.br(Cc::Ne, j as V, Src::Imm(TAG_INT), notint);
                acc.push(Nt::I);
                self.dispatch_rec(j + 1, tys, bad, done, acc, leaf, res);
                acc.pop();
                self.op(Op::Bind(notint));
                self.br(Cc::Ne, j as V, Src::Imm(TAG_FLT), bad[j]);
                acc.push(Nt::F);
                self.dispatch_rec(j + 1, tys, bad, done, acc, leaf, res);
                acc.pop();
            }
        }
    }

    // ---- arithmetic ----------------------------------------------------------------------------

    pub(crate) fn t_arith(&mut self, pc: u32, i: Instruction, op: OpCode) {
        use OpCode as O;
        let a = i.a();
        let (b, c) = (i.b(), i.c());
        match op {
            O::Add => self.arith(pc, a, Ar::Add, Opd::R(b), Opd::R(c)),
            O::Sub => self.arith(pc, a, Ar::Sub, Opd::R(b), Opd::R(c)),
            O::Mul => self.arith(pc, a, Ar::Mul, Opd::R(b), Opd::R(c)),
            O::Div => self.arith(pc, a, Ar::Div, Opd::R(b), Opd::R(c)),
            O::Mod => self.arith(pc, a, Ar::Mod, Opd::R(b), Opd::R(c)),
            O::Idiv => self.arith(pc, a, Ar::Idiv, Opd::R(b), Opd::R(c)),
            O::Band => self.arith(pc, a, Ar::And, Opd::R(b), Opd::R(c)),
            O::Bor => self.arith(pc, a, Ar::Or, Opd::R(b), Opd::R(c)),
            O::Bxor => self.arith(pc, a, Ar::Xor, Opd::R(b), Opd::R(c)),
            O::Shl => self.arith(pc, a, Ar::Shl, Opd::R(b), Opd::R(c)),
            O::Shr => self.arith(pc, a, Ar::Shr, Opd::R(b), Opd::R(c)),
            O::Addi => self.arith(pc, a, Ar::Add, Opd::R(b), Opd::I(i64::from(i.sc()))),
            O::Addk => self.arith(pc, a, Ar::Add, Opd::R(b), self.kopd(c)),
            O::Subk => self.arith(pc, a, Ar::Sub, Opd::R(b), self.kopd(c)),
            O::Mulk => self.arith(pc, a, Ar::Mul, Opd::R(b), self.kopd(c)),
            O::Divk => self.arith(pc, a, Ar::Div, Opd::R(b), self.kopd(c)),
            O::Modk => self.arith(pc, a, Ar::Mod, Opd::R(b), self.kopd(c)),
            O::Idivk => self.arith(pc, a, Ar::Idiv, Opd::R(b), self.kopd(c)),
            O::Bandk => self.arith(pc, a, Ar::And, Opd::R(b), self.kopd(c)),
            O::Bork => self.arith(pc, a, Ar::Or, Opd::R(b), self.kopd(c)),
            O::Bxork => self.arith(pc, a, Ar::Xor, Opd::R(b), self.kopd(c)),
            // `sC << R[B]`
            O::Shli => self.arith(pc, a, Ar::Shl, Opd::I(i64::from(i.sc())), Opd::R(b)),
            // `R[B] >> sC` = shiftl(R[B], -sC)
            O::Shri => self.arith(pc, a, Ar::Shl, Opd::R(b), Opd::I(-i64::from(i.sc()))),
            O::Unm => self.t_unm(pc, a, b),
            O::Bnot => self.t_bnot(pc, a, b),
            O::Not => self.t_not(a, b),
            O::Getupval => self.t_getupval(a, b),
            O::Setupval => self.t_setupval(pc, a, b),
            O::Eq | O::Lt | O::Le | O::Eqk | O::Eqi | O::Lti | O::Lei | O::Gti | O::Gei => {
                self.t_cmp(pc, i, op)
            }
            O::Test => self.t_test(pc, i),
            O::Testset => self.t_testset(pc, i),
            O::Forloop => self.t_forloop(pc, i),
            O::Forprep => self.t_forprep(pc, i),
            _ => unreachable!("not an arithmetic template: {op:?}"),
        }
    }

    fn arith(&mut self, pc: u32, a: u32, ar: Ar, o0: Opd, o1: Opd) {
        let x = self.exit_r(pc);
        let ops = [o0, o1];
        // (BUNDLE-D `armm`: operands that are not numbers go to the metamethod stub; the number cases that leave stay exits)
        let xm = self.arith_mm_label(pc, x);
        let instr = self.code[pc as usize];
        let ty = self.dispatch(&ops, &[xm, xm], &mut |s, t| {
            s.arith_leaf(a, ar, ops, t, x, instr)
        });
        self.ty[a as usize] = ty;
    }

    fn arith_leaf(
        &mut self,
        a: u32,
        ar: Ar,
        ops: [Opd; 2],
        t: &[Nt],
        x: L,
        instr: u32,
    ) -> Option<Ty> {
        let ii = t[0] == Nt::I && t[1] == Nt::I;
        match ar {
            Ar::Add | Ar::Sub | Ar::Mul | Ar::Div => {
                if ii && ar != Ar::Div {
                    self.ld_i(2, ops[0]);
                    let s = self.src_i(ops[1], 3);
                    self.op(match ar {
                        Ar::Add => Op::Add(2, s),
                        Ar::Sub => Op::Sub(2, s),
                        _ => Op::Mul(2, s),
                    });
                    self.st_int(a, 2);
                    Some(Ty::Int)
                } else {
                    self.ld_f(2, ops[0], t[0]);
                    self.ld_f(3, ops[1], t[1]);
                    self.op(match ar {
                        Ar::Add => Op::FAdd(2, 3),
                        Ar::Sub => Op::FSub(2, 3),
                        Ar::Mul => Op::FMul(2, 3),
                        _ => Op::FDiv(2, 3),
                    });
                    self.st_flt(a, 2);
                    Some(Ty::Flt)
                }
            }
            // BUNDLE-Q (`t1q` bit 1): float modulo through `jit_fmod` (`luai_nummod`, stores the float result itself)
            Ar::Mod if !ii && self.calls_on() && self.env().t1q & 2 != 0 => {
                let w = u32::from(self.env().t1q);
                self.op(Op::CallH {
                    f: rn_core::jit_helpers::jit_fmod as *const () as usize as u64,
                    a: instr,
                    b: w,
                });
                self.br(Cc::Ne, 0, Src::Imm(rn_core::jit_helpers::H_DONE as i32), x);
                Some(Ty::Flt)
            }
            // float floor division / modulo: the interpreter's `floor` / `fmod` paths
            Ar::Idiv | Ar::Mod if !ii => {
                self.op(Op::Jmp(x));
                None
            }
            Ar::Idiv | Ar::Mod => {
                self.divmod_leaf(a, ar == Ar::Mod, ops, x);
                Some(Ty::Int)
            }
            _ if !ii => {
                // bitwise operations on floats convert exactly: the interpreter does that
                self.op(Op::Jmp(x));
                None
            }
            Ar::And | Ar::Or | Ar::Xor => {
                self.ld_i(2, ops[0]);
                let s = self.src_i(ops[1], 3);
                self.op(match ar {
                    Ar::And => Op::And(2, s),
                    Ar::Or => Op::Or(2, s),
                    _ => Op::Xor(2, s),
                });
                self.st_int(a, 2);
                Some(Ty::Int)
            }
            Ar::Shl | Ar::Shr => {
                self.shift_leaf(a, ar == Ar::Shr, ops);
                Some(Ty::Int)
            }
        }
    }

    /// `//` and `%` on integers (`number::idiv` / `imod`): zero divisor exits, `-1` is special-cased.
    fn divmod_leaf(&mut self, a: u32, is_mod: bool, ops: [Opd; 2], x: L) {
        self.ld_i(2, ops[0]);
        self.ld_i(3, ops[1]);
        self.br(Cc::Eq, 3, Src::Imm(0), x);
        let (lm1, lend, lfix) = (self.p.label(), self.p.label(), self.p.label());
        self.br(Cc::Eq, 3, Src::Imm(-1), lm1);
        self.op(Op::SDivRem {
            q: 4,
            r: 5,
            a: 2,
            b: 3,
        });
        // floor correction when the remainder is non-zero and has the sign opposite to the divisor
        self.br(Cc::Eq, 5, Src::Imm(0), lfix);
        self.op(Op::MovR(6, 5));
        self.op(Op::Xor(6, Src::V(3)));
        self.br(Cc::Ge, 6, Src::Imm(0), lfix);
        if is_mod {
            self.op(Op::Add(5, Src::V(3)));
        } else {
            self.op(Op::Sub(4, Src::Imm(1)));
        }
        self.op(Op::Bind(lfix));
        self.st_int(a, if is_mod { 5 } else { 4 });
        self.op(Op::Jmp(lend));
        self.op(Op::Bind(lm1));
        self.op(Op::Mov(4, 0));
        if !is_mod {
            self.op(Op::Sub(4, Src::V(2)));
        }
        self.st_int(a, 4);
        self.op(Op::Bind(lend));
    }

    /// `shiftl(o0, o1)` (or with `-o1` when `neg`) into register `a`.
    fn shift_leaf(&mut self, a: u32, neg: bool, ops: [Opd; 2]) {
        self.ld_i(2, ops[0]);
        match ops[1] {
            Opd::I(y) => {
                let y = if neg { y.wrapping_neg() } else { y };
                if (1..=63).contains(&y) {
                    self.op(Op::ShlI(2, y as u8));
                } else if (-63..=-1).contains(&y) {
                    self.op(Op::ShrI(2, (-y) as u8));
                } else if y != 0 {
                    self.op(Op::Mov(2, 0));
                }
            }
            o => {
                self.ld_i(6, o);
                if neg {
                    self.op(Op::Neg(6));
                }
                let (lneg, lzero, lend) = (self.p.label(), self.p.label(), self.p.label());
                self.br(Cc::Lt, 6, Src::Imm(0), lneg);
                self.br(Cc::Ge, 6, Src::Imm(64), lzero);
                self.op(Op::ShlV(2));
                self.op(Op::Jmp(lend));
                self.op(Op::Bind(lneg));
                self.op(Op::Neg(6));
                self.br(Cc::Lt, 6, Src::Imm(0), lzero); // y == i64::MIN
                self.br(Cc::Ge, 6, Src::Imm(64), lzero);
                self.op(Op::ShrV(2));
                self.op(Op::Jmp(lend));
                self.op(Op::Bind(lzero));
                self.op(Op::Mov(2, 0));
                self.op(Op::Bind(lend));
            }
        }
        self.st_int(a, 2);
    }

    fn t_unm(&mut self, pc: u32, a: u32, b: u32) {
        let x = self.exit_r(pc);
        let ops = [Opd::R(b)];
        let xm = self.arith_mm_label(pc, x);
        let ty = self.dispatch(&ops, &[xm], &mut |s, t| {
            s.ld_i(2, ops[0]);
            if t[0] == Nt::I {
                s.op(Op::Neg(2));
                s.st_int(a, 2);
                Some(Ty::Int)
            } else {
                s.op(Op::Mov(3, 1 << 63));
                s.op(Op::Xor(2, Src::V(3)));
                s.st_flt(a, 2);
                Some(Ty::Flt)
            }
        });
        self.ty[a as usize] = ty;
    }

    fn t_bnot(&mut self, pc: u32, a: u32, b: u32) {
        let x = self.exit_r(pc);
        let xm = self.arith_mm_label(pc, x);
        if self.ty[b as usize] != Ty::Int {
            self.ld(0, W::B1, fr(b, 0));
            self.br(Cc::Ne, 0, Src::Imm(TAG_INT), xm);
        }
        self.ld(2, W::B8, fr(b, 8));
        self.op(Op::Not(2));
        self.st_int(a, 2);
        self.ty[a as usize] = Ty::Int;
    }

    /// Branches to `lfalsy` if the value whose tag is in `V0` is nil or false (clobbers V0).
    fn falsy_branch(&mut self, lfalsy: L) {
        self.br(Cc::Eq, 0, Src::Imm(TAG_FALSE), lfalsy);
        self.op(Op::And(0, Src::Imm(0x0F)));
        self.br(Cc::Eq, 0, Src::Imm(0), lfalsy);
    }

    fn t_not(&mut self, a: u32, b: u32) {
        let (lfalsy, lend) = (self.p.label(), self.p.label());
        self.ld(0, W::B1, fr(b, 0));
        self.falsy_branch(lfalsy);
        self.st_imm(W::B1, fr(a, 0), i32::from(LUA_VFALSE));
        self.st_imm(W::B8, fr(a, 8), 0);
        self.op(Op::Jmp(lend));
        self.op(Op::Bind(lfalsy));
        self.st_imm(W::B1, fr(a, 0), i32::from(LUA_VTRUE));
        self.st_imm(W::B8, fr(a, 8), 0);
        self.op(Op::Bind(lend));
        self.ty[a as usize] = Ty::Unk;
    }

    // ---- constants and upvalues ---------------------------------------------------------------

    pub(crate) fn load_k(&mut self, a: u32, kx: u32) {
        let k = self.cfg.consts.get(kx as usize).copied();
        match k {
            Some(v) if v.tag() == LUA_VNUMINT => {
                let x = v.as_int().expect("int constant");
                self.st_imm(W::B1, fr(a, 0), TAG_INT);
                if fits32(x) {
                    self.st_imm(W::B8, fr(a, 8), x as i32);
                } else {
                    self.op(Op::Mov(0, x as u64));
                    self.st(W::B8, fr(a, 8), 0);
                }
                self.ty[a as usize] = Ty::Int;
            }
            Some(v) if v.tag() == LUA_VNUMFLT => {
                let f = v.as_float().expect("float constant");
                self.op(Op::Mov(0, f.to_bits()));
                self.st_imm(W::B1, fr(a, 0), TAG_FLT);
                self.st(W::B8, fr(a, 8), 0);
                self.ty[a as usize] = Ty::Flt;
            }
            _ => {
                let km = M {
                    base: Base::K,
                    off: (kx as i32) * 16,
                };
                self.copy_value(fr(a, 0), km);
                self.ty[a as usize] = Ty::Unk;
            }
        }
    }

    /// Pointer to the value of upvalue `b` into `V0` (closure from the function slot below the frame).
    pub(crate) fn upval_ptr(&mut self, b: u32) {
        self.ld(0, W::B8, fr(0, -16 + 8));
        let r0 = |off: usize| M {
            base: Base::Reg(0),
            off: off as i32,
        };
        self.ld(0, W::B8, r0(lclosure::UPVALS + 8 * b as usize));
        self.ld(0, W::B8, r0(upval::V));
    }

    fn t_getupval(&mut self, a: u32, b: u32) {
        self.upval_ptr(b);
        self.copy_value(
            fr(a, 0),
            M {
                base: Base::Reg(0),
                off: 0,
            },
        );
        self.ty[a as usize] = Ty::Unk;
    }

    /// SETUPVAL of a non-collectable value needs no barrier; collectable values exit.
    fn t_setupval(&mut self, pc: u32, a: u32, b: u32) {
        // C2: a collectable value goes to `jit_setupval` (store + barrier) and continues in tier 1
        let x = if self.cfg.calls.is_some() {
            let i = Instruction(self.code[pc as usize]);
            self.slow_label(pc, i, OpCode::Setupval)
        } else {
            self.exit_r(pc)
        };
        self.ld(1, W::B1, fr(a, 0));
        self.op(Op::And(1, Src::Imm(i32::from(BIT_ISCOLLECTABLE))));
        self.br(Cc::Ne, 1, Src::Imm(0), x);
        self.upval_ptr(b);
        self.copy_value(
            M {
                base: Base::Reg(0),
                off: 0,
            },
            fr(a, 0),
        );
        // `debug.upvaluejoin(f, k, g, 1)` can make upvalue `k` alias a register of this very frame (the open
        // upvalue of a closure created here): the store may have changed any captured register, so the
        // block-local type knowledge is dropped (found by the T2b region-analysis review, finding 4).
        self.ty = [Ty::Unk; 256];
    }

    // ---- conditionals -------------------------------------------------------------------------

    /// Labels (cond true, cond false) of the conditional at `pc`, plus the label of its jump block.
    fn cond_labels(&mut self, pc: u32, k: u32) -> (L, L, L) {
        let lj = self.p.label();
        let skip = self.pcl[pc as usize + 2];
        if k != 0 {
            (lj, skip, lj)
        } else {
            (skip, lj, lj)
        }
    }

    /// The jump block of a conditional: what the interpreter's `donextjump` does.
    fn finish_cond(&mut self, pc: u32, lj: L) {
        let sj = Instruction(self.code[pc as usize + 1]).sj();
        let t = (i64::from(pc) + 2 + i64::from(sj)) as u32;
        self.op(Op::Bind(lj));
        if sj < 0 {
            self.poll(t);
        }
        let l = self.pcl[t as usize];
        self.op(Op::Jmp(l));
    }

    fn t_test(&mut self, pc: u32, i: Instruction) {
        let (lt, lf, lj) = self.cond_labels(pc, i.k());
        match self.ty[i.a() as usize] {
            Ty::Int | Ty::Flt => self.op(Op::Jmp(lt)),
            Ty::Unk => {
                self.ld(0, W::B1, fr(i.a(), 0));
                self.falsy_branch(lf);
                self.op(Op::Jmp(lt));
            }
        }
        self.finish_cond(pc, lj);
    }

    fn t_testset(&mut self, pc: u32, i: Instruction) {
        let (a, b, k) = (i.a(), i.b(), i.k());
        let (lj, skip) = (self.p.label(), self.pcl[pc as usize + 2]);
        let (lcopy, lfalsy) = (self.p.label(), self.p.label());
        let (truthy, falsy) = if k != 0 { (lcopy, skip) } else { (skip, lcopy) };
        match self.ty[b as usize] {
            Ty::Int | Ty::Flt => self.op(Op::Jmp(truthy)),
            Ty::Unk => {
                self.ld(0, W::B1, fr(b, 0));
                self.falsy_branch(lfalsy);
                self.op(Op::Jmp(truthy));
                self.op(Op::Bind(lfalsy));
                self.op(Op::Jmp(falsy));
            }
        }
        self.op(Op::Bind(lcopy));
        self.copy_value(fr(a, 0), fr(b, 0));
        self.ty[a as usize] = self.ty[b as usize];
        self.op(Op::Jmp(lj));
        self.finish_cond(pc, lj);
    }

    fn t_cmp(&mut self, pc: u32, i: Instruction, op: OpCode) {
        use OpCode as O;
        let (lt, lf, lj) = self.cond_labels(pc, i.k());
        let mut x = self.exit_r(pc);
        let a = i.a();
        // C2: operands that are not both known to be numbers (strings, tables, ...) go to `jit_cmp` and stay in tier 1
        if matches!(op, O::Lt | O::Le | O::Eq)
            && self.cfg.calls.is_some()
            && !(self.sty(Opd::R(a)).is_some() && self.sty(Opd::R(i.b())).is_some())
        {
            x = self.cmp_slow_label(pc, i, lt, lf);
        }
        match op {
            O::Lt => self.order(Cc::Lt, [Opd::R(a), Opd::R(i.b())], x, lt, lf),
            O::Le => self.order(Cc::Le, [Opd::R(a), Opd::R(i.b())], x, lt, lf),
            O::Lti => self.order(Cc::Lt, [Opd::R(a), Opd::I(i64::from(i.sb()))], x, lt, lf),
            O::Lei => self.order(Cc::Le, [Opd::R(a), Opd::I(i64::from(i.sb()))], x, lt, lf),
            O::Gti => self.order(Cc::Gt, [Opd::R(a), Opd::I(i64::from(i.sb()))], x, lt, lf),
            O::Gei => self.order(Cc::Ge, [Opd::R(a), Opd::I(i64::from(i.sb()))], x, lt, lf),
            O::Eqi => self.equal(Opd::R(a), Opd::I(i64::from(i.sb())), x, lt, lf),
            O::Eqk => {
                let tag = self.cfg.consts[i.b() as usize].tag();
                match tag {
                    LUA_VNIL | LUA_VFALSE | LUA_VTRUE => {
                        // raw equality of two values of different tags is false; same tag nil/false/true is true
                        self.ld(0, W::B1, fr(a, 0));
                        self.br(Cc::Eq, 0, Src::Imm(i32::from(tag)), lt);
                        self.op(Op::Jmp(lf));
                    }
                    t if t == ctb(LUA_VSHRSTR) => {
                        // short strings are interned: equal iff same variant and same object; every other
                        // value (long strings, numbers, ...) is unequal
                        let kp = self.cfg.consts[i.b() as usize]
                            .as_gc()
                            .expect("string constant") as usize
                            as u64;
                        self.ld(0, W::B1, fr(a, 0));
                        self.br(Cc::Ne, 0, Src::Imm(i32::from(t)), lf);
                        self.ld(1, W::B8, fr(a, 8));
                        self.op(Op::Mov(2, kp));
                        self.br(Cc::Eq, 1, Src::V(2), lt);
                        self.op(Op::Jmp(lf));
                    }
                    _ => {
                        let k = self.kopd(i.b());
                        self.equal(Opd::R(a), k, x, lt, lf);
                    }
                }
            }
            O::Eq => {
                // two registers: numbers compare by value; same-tag nil/false/true are equal; other
                // same-tag values (strings, tables with `__eq`, ...) exit; different tags are unequal
                let ops = [Opd::R(a), Opd::R(i.b())];
                let lnn0 = self.p.label();
                let tys = [self.sty(ops[0]), self.sty(ops[1])];
                self.dispatch(&ops, &[lnn0, lf], &mut |s, t| {
                    s.cmp_leaf(Cc::Eq, ops, t, x, lt, lf);
                    None
                });
                if tys[0].is_none() {
                    // operand 0 is not a number (V0 = its tag; V1 = tag of operand 1 when not known numeric)
                    self.op(Op::Bind(lnn0));
                    if tys[1].is_some() {
                        self.op(Op::Jmp(lf));
                    } else {
                        self.br(Cc::Eq, 1, Src::Imm(TAG_INT), lf);
                        self.br(Cc::Eq, 1, Src::Imm(TAG_FLT), lf);
                        self.br(Cc::Ne, 0, Src::V(1), lf);
                        if self.calls_on() && self.env().t1q & 1 != 0 {
                            self.eq_objects_inline(i, lt, lf);
                        }
                        self.op(Op::And(0, Src::Imm(0x4F)));
                        self.br(Cc::Le, 0, Src::Imm(1), lt);
                        self.op(Op::Jmp(x));
                    }
                }
            }
            _ => unreachable!(),
        }
        self.finish_cond(pc, lj);
    }

    /// BUNDLE-Q (`t1q` bit 0): `EQ` of two registers of the same non-number tag (`V0`): the same payload is the same value (equal); two tables whose metatables are
    /// both absent or flagged as having no `__eq` (`flags` bit `Tm::Eq` set once the absence was looked up, as the interpreter's `fasttm`) are unequal. Everything
    /// else (an unflagged metatable, userdata, a different long string ...) falls through to the generic test below (`jit_cmp`). Clobbers `V2`-`V6`, keeps `V0` / `V1`.
    fn eq_objects_inline(&mut self, i: Instruction, lt: L, lf: L) {
        let w = self.env().t1q;
        let (ra, rb) = (i.a(), i.b());
        let lgen = self.p.label();
        self.ld(2, W::B8, fr(ra, 8));
        self.ld(3, W::B8, fr(rb, 8));
        if w & 0x100 == 0 {
            // (mutant `t1q_eq_same_false`: no shortcut, `jit_cmp` answers "unequal" for the same object)
            self.br(Cc::Eq, 2, Src::V(3), lt);
        }
        self.br(Cc::Ne, 0, Src::Imm(ctb(LUA_VTABLE) as i32), lgen);
        self.ld(
            4,
            W::B8,
            M {
                base: Base::Reg(2),
                off: rn_core::jit_layout::table::METATABLE as i32,
            },
        );
        self.ld(
            5,
            W::B8,
            M {
                base: Base::Reg(3),
                off: rn_core::jit_layout::table::METATABLE as i32,
            },
        );
        for (r, skip) in [(4u8, false), (5u8, w & 0x400 != 0)] {
            if skip {
                continue; // (mutant `t1q_eq_second_mm_skip`: the second metatable is not asked)
            }
            let next = self.p.label();
            self.br(Cc::Eq, r, Src::Imm(0), next);
            self.ld(
                6,
                W::B1,
                M {
                    base: Base::Reg(r),
                    off: rn_core::jit_layout::table::FLAGS as i32,
                },
            );
            self.op(Op::And(6, Src::Imm(1 << rn_core::meta::Tm::Eq as i32)));
            self.br(Cc::Eq, 6, Src::Imm(0), lgen);
            self.op(Op::Bind(next));
        }
        self.op(Op::Jmp(if w & 0x200 != 0 { lt } else { lf })); // (mutant `t1q_eq_nomm_true`)
        self.op(Op::Bind(lgen));
    }

    /// `<`/`<=`/`>`/`>=` on two numbers; everything else exits.
    fn order(&mut self, cc: Cc, ops: [Opd; 2], x: L, lt: L, lf: L) {
        self.dispatch(&ops, &[x, x], &mut |s, t| {
            s.cmp_leaf(cc, ops, t, x, lt, lf);
            None
        });
    }

    /// Raw equality of a register and a numeric constant (non-numbers are unequal).
    fn equal(&mut self, o0: Opd, o1: Opd, x: L, lt: L, lf: L) {
        let ops = [o0, o1];
        self.dispatch(&ops, &[lf, lf], &mut |s, t| {
            s.cmp_leaf(Cc::Eq, ops, t, x, lt, lf);
            None
        });
    }

    /// Loads `o` as double bits into `dst`; an integer register beyond `±2^53` (not exactly convertible)
    /// exits to `x`.
    fn ld_f_exact(&mut self, dst: V, o: Opd, t: Nt, x: L) -> bool {
        match (o, t) {
            (Opd::R(r), Nt::I) => {
                self.ld(dst, W::B8, fr(r, 8));
                self.op(Op::Mov(6, EXACT as u64));
                self.br(Cc::Gt, dst, Src::V(6), x);
                self.op(Op::Mov(6, (-EXACT) as u64));
                self.br(Cc::Lt, dst, Src::V(6), x);
                self.op(Op::I2F(dst));
                true
            }
            (Opd::I(k), _) if k.unsigned_abs() > EXACT as u64 => {
                self.op(Op::Jmp(x));
                false
            }
            _ => {
                self.ld_f(dst, o, t);
                true
            }
        }
    }

    /// One comparison leaf: branches to `lt`/`lf` according to `ops[0] <cc> ops[1]` with Lua's exact
    /// semantics for int/float mixes (NaN compares false).
    fn cmp_leaf(&mut self, cc: Cc, ops: [Opd; 2], t: &[Nt], x: L, lt: L, lf: L) {
        if t[0] == Nt::I && t[1] == Nt::I {
            self.ld_i(2, ops[0]);
            let s = self.src_i(ops[1], 3);
            self.br(cc, 2, s, lt);
        } else {
            if !self.ld_f_exact(2, ops[0], t[0], x) || !self.ld_f_exact(3, ops[1], t[1], x) {
                return;
            }
            self.op(Op::FBr {
                cc,
                a: 2,
                b: 3,
                to: lt,
            });
        }
        self.op(Op::Jmp(lf));
    }

    // ---- numeric for ---------------------------------------------------------------------------

    /// 5.4 `FORLOOP`: `R[A]` internal index, `R[A+1]` remaining iteration count (integer loop; the limit
    /// otherwise), `R[A+2]` step, `R[A+3]` control variable (a copy the body may overwrite: always stored whole).
    fn t_forloop54(&mut self, pc: u32, i: Instruction) {
        let a = i.a();
        let t = (i64::from(pc) + 1 - i64::from(i.bx())) as u32;
        let fall = self.pcl[pc as usize + 1];
        let (lfloat, lcont, lpos) = (self.p.label(), self.p.label(), self.p.label());
        self.ld(0, W::B1, fr(a + 2, 0));
        self.br(Cc::Ne, 0, Src::Imm(TAG_INT), lfloat);
        self.ld(1, W::B8, fr(a + 1, 8)); // remaining iterations
        self.br(Cc::Eq, 1, Src::Imm(0), fall);
        self.ld(2, W::B8, fr(a + 2, 8)); // step
        self.ld(3, W::B8, fr(a, 8)); // internal index
        self.op(Op::Sub(1, Src::Imm(1)));
        self.st(W::B8, fr(a + 1, 8), 1);
        self.op(Op::Add(3, Src::V(2)));
        self.st(W::B8, fr(a, 8), 3);
        self.st_imm(W::B1, fr(a + 3, 0), TAG_INT);
        self.st(W::B8, fr(a + 3, 8), 3);
        self.poll(t);
        let l = self.pcl[t as usize];
        self.op(Op::Jmp(l));
        // float loop: idx += step; continue while (0 < step ? idx <= limit : limit <= idx)
        self.op(Op::Bind(lfloat));
        self.ld(1, W::B8, fr(a + 2, 8)); // step
        self.ld(2, W::B8, fr(a + 1, 8)); // limit
        self.ld(3, W::B8, fr(a, 8)); // idx
        self.op(Op::FAdd(3, 1));
        self.op(Op::Mov(4, 0));
        self.op(Op::FBr {
            cc: Cc::Lt,
            a: 4,
            b: 1,
            to: lpos,
        });
        self.op(Op::FBr {
            cc: Cc::Le,
            a: 2,
            b: 3,
            to: lcont,
        });
        self.op(Op::Jmp(fall));
        self.op(Op::Bind(lpos));
        self.op(Op::FBr {
            cc: Cc::Le,
            a: 3,
            b: 2,
            to: lcont,
        });
        self.op(Op::Jmp(fall));
        self.op(Op::Bind(lcont));
        self.st(W::B8, fr(a, 8), 3); // internal index (a float already)
        self.st_imm(W::B1, fr(a + 3, 0), TAG_FLT);
        self.st(W::B8, fr(a + 3, 8), 3);
        self.poll(t);
        self.op(Op::Jmp(l));
    }

    fn t_forloop(&mut self, pc: u32, i: Instruction) {
        if rn_core::jit_layout::LUA54 {
            return self.t_forloop54(pc, i);
        }
        let a = i.a();
        let t = (i64::from(pc) + 1 - i64::from(i.bx())) as u32;
        let fall = self.pcl[pc as usize + 1];
        let (lfloat, lcont, lpos) = (self.p.label(), self.p.label(), self.p.label());
        self.ld(0, W::B1, fr(a + 1, 0));
        self.br(Cc::Ne, 0, Src::Imm(TAG_INT), lfloat);
        self.ld(1, W::B8, fr(a, 8)); // remaining iterations
        self.br(Cc::Eq, 1, Src::Imm(0), fall);
        self.ld(2, W::B8, fr(a + 1, 8)); // step
        self.ld(3, W::B8, fr(a + 2, 8)); // control variable
        self.op(Op::Sub(1, Src::Imm(1)));
        self.st(W::B8, fr(a, 8), 1);
        self.op(Op::Add(3, Src::V(2)));
        self.st(W::B8, fr(a + 2, 8), 3);
        self.poll(t);
        let l = self.pcl[t as usize];
        self.op(Op::Jmp(l));
        // float loop: idx += step; continue while (0 < step ? idx <= limit : limit <= idx)
        self.op(Op::Bind(lfloat));
        self.ld(1, W::B8, fr(a + 1, 8)); // step
        self.ld(2, W::B8, fr(a, 8)); // limit
        self.ld(3, W::B8, fr(a + 2, 8)); // idx
        self.op(Op::FAdd(3, 1));
        self.op(Op::Mov(4, 0));
        self.op(Op::FBr {
            cc: Cc::Lt,
            a: 4,
            b: 1,
            to: lpos,
        });
        self.op(Op::FBr {
            cc: Cc::Le,
            a: 2,
            b: 3,
            to: lcont,
        });
        self.op(Op::Jmp(fall));
        self.op(Op::Bind(lpos));
        self.op(Op::FBr {
            cc: Cc::Le,
            a: 3,
            b: 2,
            to: lcont,
        });
        self.op(Op::Jmp(fall));
        self.op(Op::Bind(lcont));
        self.st(W::B8, fr(a + 2, 8), 3);
        self.poll(t);
        self.op(Op::Jmp(l));
    }

    /// Loads slot `r` (int or float) as double bits into `dst`; other tags exit to `x`. Clobbers V0.
    fn ld_slot_f(&mut self, dst: V, r: u32, x: L) {
        let ok = self.p.label();
        self.ld(0, W::B1, fr(r, 0));
        self.ld(dst, W::B8, fr(r, 8));
        self.br(Cc::Eq, 0, Src::Imm(TAG_FLT), ok);
        self.br(Cc::Ne, 0, Src::Imm(TAG_INT), x);
        self.op(Op::I2F(dst));
        self.op(Op::Bind(ok));
    }

    /// 5.4 `FORPREP` (`forprep_int54` / `State::for_prep`): integer loop (init and step integers, limit an
    /// integer, else the interpreter's `forlimit`): `R[A+1]` becomes the iteration count and `R[A+3]` the
    /// control variable (set before the skip test, like the interpreter); float loop: `R[A]`, `R[A+3]` = init,
    /// `R[A+1]` = limit, `R[A+2]` = step.
    fn t_forprep54(&mut self, pc: u32, i: Instruction) {
        let a = i.a();
        let x = self.exit_r(pc);
        let skip = self.pcl[(pc + i.bx() + 2) as usize];
        let fall = self.pcl[pc as usize + 1];
        let lfloat = self.p.label();
        self.ld(0, W::B1, fr(a, 0));
        self.br(Cc::Ne, 0, Src::Imm(TAG_INT), lfloat);
        self.ld(0, W::B1, fr(a + 2, 0));
        self.br(Cc::Ne, 0, Src::Imm(TAG_INT), lfloat);
        self.ld(0, W::B1, fr(a + 1, 0));
        self.br(Cc::Ne, 0, Src::Imm(TAG_INT), x);
        self.ld(1, W::B8, fr(a, 8)); // init
        self.ld(2, W::B8, fr(a + 1, 8)); // limit
        self.ld(3, W::B8, fr(a + 2, 8)); // step
        self.br(Cc::Eq, 3, Src::Imm(0), x); // 'for' step is zero: the interpreter raises it
        self.st_imm(W::B1, fr(a + 3, 0), TAG_INT); // control variable
        self.st(W::B8, fr(a + 3, 8), 1);
        let (lpos, lstore) = (self.p.label(), self.p.label());
        self.br(Cc::Gt, 3, Src::Imm(0), lpos);
        self.br(Cc::Lt, 1, Src::V(2), skip);
        self.op(Op::MovR(4, 1));
        self.op(Op::Sub(4, Src::V(2)));
        self.op(Op::Mov(5, 0));
        self.op(Op::Sub(5, Src::V(3)));
        self.op(Op::UDiv { dst: 4, a: 4, b: 5 });
        self.op(Op::Jmp(lstore));
        self.op(Op::Bind(lpos));
        self.br(Cc::Gt, 1, Src::V(2), skip);
        self.op(Op::MovR(4, 2));
        self.op(Op::Sub(4, Src::V(1)));
        self.op(Op::UDiv { dst: 4, a: 4, b: 3 });
        self.op(Op::Bind(lstore));
        self.st(W::B8, fr(a + 1, 8), 4); // iteration count in place of the limit
        self.op(Op::Jmp(fall));
        // float loop
        self.op(Op::Bind(lfloat));
        self.ld_slot_f(1, a, x); // init
        self.ld_slot_f(2, a + 1, x); // limit
        self.ld_slot_f(3, a + 2, x); // step
        self.op(Op::Mov(4, 0));
        self.op(Op::FBr {
            cc: Cc::Eq,
            a: 3,
            b: 4,
            to: x,
        });
        let (lfpos, lfstore) = (self.p.label(), self.p.label());
        self.op(Op::FBr {
            cc: Cc::Lt,
            a: 4,
            b: 3,
            to: lfpos,
        });
        self.op(Op::FBr {
            cc: Cc::Lt,
            a: 1,
            b: 2,
            to: skip,
        });
        self.op(Op::Jmp(lfstore));
        self.op(Op::Bind(lfpos));
        self.op(Op::FBr {
            cc: Cc::Lt,
            a: 2,
            b: 1,
            to: skip,
        });
        self.op(Op::Bind(lfstore));
        for (r, v) in [(a, 1), (a + 1, 2), (a + 2, 3), (a + 3, 1)] {
            self.st_imm(W::B1, fr(r, 0), TAG_FLT);
            self.st(W::B8, fr(r, 8), v);
        }
        self.op(Op::Jmp(fall));
    }

    fn t_forprep(&mut self, pc: u32, i: Instruction) {
        if rn_core::jit_layout::LUA54 {
            return self.t_forprep54(pc, i);
        }
        let a = i.a();
        let x = self.exit_r(pc);
        let skip = self.pcl[(pc + i.bx() + 2) as usize];
        let fall = self.pcl[pc as usize + 1];
        let lfloat = self.p.label();
        // integer loop: init and step are integers (the limit then must be one too, else the interpreter's
        // `for_limit` converts it)
        self.ld(0, W::B1, fr(a, 0));
        self.br(Cc::Ne, 0, Src::Imm(TAG_INT), lfloat);
        self.ld(0, W::B1, fr(a + 2, 0));
        self.br(Cc::Ne, 0, Src::Imm(TAG_INT), lfloat);
        self.ld(0, W::B1, fr(a + 1, 0));
        self.br(Cc::Ne, 0, Src::Imm(TAG_INT), x);
        self.ld(1, W::B8, fr(a, 8)); // init
        self.ld(2, W::B8, fr(a + 1, 8)); // limit
        self.ld(3, W::B8, fr(a + 2, 8)); // step
        self.br(Cc::Eq, 3, Src::Imm(0), x); // 'for' step is zero: the interpreter raises it
        let (lpos, lstore) = (self.p.label(), self.p.label());
        self.br(Cc::Gt, 3, Src::Imm(0), lpos);
        self.br(Cc::Lt, 1, Src::V(2), skip);
        self.op(Op::MovR(4, 1));
        self.op(Op::Sub(4, Src::V(2)));
        self.op(Op::Mov(5, 0));
        self.op(Op::Sub(5, Src::V(3)));
        self.op(Op::UDiv { dst: 4, a: 4, b: 5 });
        self.op(Op::Jmp(lstore));
        self.op(Op::Bind(lpos));
        self.br(Cc::Gt, 1, Src::V(2), skip);
        self.op(Op::MovR(4, 2));
        self.op(Op::Sub(4, Src::V(1)));
        self.op(Op::UDiv { dst: 4, a: 4, b: 3 });
        self.op(Op::Bind(lstore));
        self.st(W::B8, fr(a, 8), 4);
        self.st(W::B8, fr(a + 1, 8), 3);
        self.st(W::B8, fr(a + 2, 8), 1);
        self.op(Op::Jmp(fall));
        // float loop: all three operands are numbers (else the interpreter reports the error)
        self.op(Op::Bind(lfloat));
        self.ld_slot_f(1, a, x); // init
        self.ld_slot_f(2, a + 1, x); // limit
        self.ld_slot_f(3, a + 2, x); // step
        self.op(Op::Mov(4, 0));
        self.op(Op::FBr {
            cc: Cc::Eq,
            a: 3,
            b: 4,
            to: x,
        });
        let (lfpos, lfstore) = (self.p.label(), self.p.label());
        self.op(Op::FBr {
            cc: Cc::Lt,
            a: 4,
            b: 3,
            to: lfpos,
        });
        self.op(Op::FBr {
            cc: Cc::Lt,
            a: 1,
            b: 2,
            to: skip,
        });
        self.op(Op::Jmp(lfstore));
        self.op(Op::Bind(lfpos));
        self.op(Op::FBr {
            cc: Cc::Lt,
            a: 2,
            b: 1,
            to: skip,
        });
        self.op(Op::Bind(lfstore));
        for (r, v) in [(a, 2), (a + 1, 3), (a + 2, 1)] {
            self.st_imm(W::B1, fr(r, 0), TAG_FLT);
            self.st(W::B8, fr(r, 8), v);
        }
        self.op(Op::Jmp(fall));
    }
}
