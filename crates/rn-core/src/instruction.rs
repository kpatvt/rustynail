//! Instruction encoding helpers (port of the macros in `lopcodes.h`).
//!
//! Layout (low bit first): `Op(7) | A(8) | k(1) | B(8) | C(8)` for `iABC`; `vB(6) | vC(10)` replace
//! `B|C` for `ivABC` (Lua 5.5); `Bx(17)`/`sBx` for `iABx`/`iAsBx`; `Ax(25)`/`sJ(25)` for `iAx`/`isJ`.
//! All getters are total (never panic) on any 32-bit word. Unlike the C macros they do not assert the
//! instruction's mode; callers decide which accessor is meaningful.
use crate::ver::active::{OpCode, OpMode, NUM_OPCODES, OP_PROPS};

pub const SIZE_OP: u32 = 7;
pub const SIZE_A: u32 = 8;
pub const SIZE_B: u32 = 8;
pub const SIZE_C: u32 = 8;
pub const SIZE_VB: u32 = 6;
pub const SIZE_VC: u32 = 10;
pub const SIZE_BX: u32 = SIZE_C + SIZE_B + 1;
pub const SIZE_AX: u32 = SIZE_BX + SIZE_A;
pub const SIZE_SJ: u32 = SIZE_BX + SIZE_A;

pub const POS_OP: u32 = 0;
pub const POS_A: u32 = POS_OP + SIZE_OP;
pub const POS_K: u32 = POS_A + SIZE_A;
pub const POS_B: u32 = POS_K + 1;
pub const POS_VB: u32 = POS_K + 1;
pub const POS_C: u32 = POS_B + SIZE_B;
pub const POS_VC: u32 = POS_VB + SIZE_VB;
pub const POS_BX: u32 = POS_K;
pub const POS_AX: u32 = POS_A;
pub const POS_SJ: u32 = POS_A;

pub const MAXARG_BX: u32 = (1 << SIZE_BX) - 1;
pub const OFFSET_SBX: i32 = (MAXARG_BX >> 1) as i32;
pub const MAXARG_AX: u32 = (1 << SIZE_AX) - 1;
pub const MAXARG_SJ: u32 = (1 << SIZE_SJ) - 1;
pub const OFFSET_SJ: i32 = (MAXARG_SJ >> 1) as i32;
pub const MAXARG_A: u32 = (1 << SIZE_A) - 1;
pub const MAXARG_B: u32 = (1 << SIZE_B) - 1;
pub const MAXARG_VB: u32 = (1 << SIZE_VB) - 1;
pub const MAXARG_C: u32 = (1 << SIZE_C) - 1;
pub const MAXARG_VC: u32 = (1 << SIZE_VC) - 1;
pub const OFFSET_SC: i32 = (MAXARG_C >> 1) as i32;

/// `int2sC`: encode a signed value as an unsigned `C`/`B` argument.
#[inline]
pub const fn int2sc(i: i32) -> i32 {
    i + OFFSET_SC
}

/// `sC2int`: decode a signed `C`/`B` argument.
#[inline]
pub const fn sc2int(i: i32) -> i32 {
    i - OFFSET_SC
}

#[inline]
const fn mask1(n: u32, p: u32) -> u32 {
    (!((!0u32) << n)) << p
}

/// A VM instruction word.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Instruction(pub u32);

impl Instruction {
    #[inline]
    fn get(self, pos: u32, size: u32) -> u32 {
        (self.0 >> pos) & mask1(size, 0)
    }

    #[inline]
    fn set(&mut self, v: u32, pos: u32, size: u32) {
        let m = mask1(size, pos);
        self.0 = (self.0 & !m) | ((v << pos) & m);
    }

    /// Raw 7-bit opcode value.
    #[inline]
    pub fn opcode_raw(self) -> u8 {
        self.get(POS_OP, SIZE_OP) as u8
    }

    /// Decoded opcode; `None` when the 7-bit value is not a valid opcode (corrupt/untrusted code).
    #[inline]
    pub fn opcode(self) -> Option<OpCode> {
        opcode_from_u8(self.opcode_raw())
    }

    /// Instruction format of this word's opcode, if valid.
    pub fn mode(self) -> Option<OpMode> {
        self.opcode().map(|o| OP_PROPS[o as usize].mode)
    }

    pub fn set_opcode(&mut self, op: OpCode) {
        self.set(op as u32, POS_OP, SIZE_OP);
    }

    #[inline]
    pub fn a(self) -> u32 {
        self.get(POS_A, SIZE_A)
    }
    #[inline]
    pub fn b(self) -> u32 {
        self.get(POS_B, SIZE_B)
    }
    #[inline]
    pub fn vb(self) -> u32 {
        self.get(POS_VB, SIZE_VB)
    }
    #[inline]
    pub fn sb(self) -> i32 {
        sc2int(self.b() as i32)
    }
    #[inline]
    pub fn c(self) -> u32 {
        self.get(POS_C, SIZE_C)
    }
    #[inline]
    pub fn vc(self) -> u32 {
        self.get(POS_VC, SIZE_VC)
    }
    #[inline]
    pub fn sc(self) -> i32 {
        sc2int(self.c() as i32)
    }
    #[inline]
    pub fn k(self) -> u32 {
        self.get(POS_K, 1)
    }
    /// `TESTARG_k`: the k bit left in place (0 or `1 << POS_K`).
    #[inline]
    pub fn test_k(self) -> u32 {
        self.0 & (1 << POS_K)
    }
    #[inline]
    pub fn bx(self) -> u32 {
        self.get(POS_BX, SIZE_BX)
    }
    #[inline]
    pub fn sbx(self) -> i32 {
        self.bx() as i32 - OFFSET_SBX
    }
    #[inline]
    pub fn ax(self) -> u32 {
        self.get(POS_AX, SIZE_AX)
    }
    #[inline]
    pub fn sj(self) -> i32 {
        self.get(POS_SJ, SIZE_SJ) as i32 - OFFSET_SJ
    }

    pub fn set_a(&mut self, v: u32) {
        self.set(v, POS_A, SIZE_A);
    }
    pub fn set_b(&mut self, v: u32) {
        self.set(v, POS_B, SIZE_B);
    }
    pub fn set_vb(&mut self, v: u32) {
        self.set(v, POS_VB, SIZE_VB);
    }
    pub fn set_c(&mut self, v: u32) {
        self.set(v, POS_C, SIZE_C);
    }
    pub fn set_vc(&mut self, v: u32) {
        self.set(v, POS_VC, SIZE_VC);
    }
    pub fn set_k(&mut self, v: u32) {
        self.set(v, POS_K, 1);
    }
    pub fn set_bx(&mut self, v: u32) {
        self.set(v, POS_BX, SIZE_BX);
    }
    pub fn set_sbx(&mut self, v: i32) {
        self.set_bx((v.wrapping_add(OFFSET_SBX)) as u32);
    }
    pub fn set_ax(&mut self, v: u32) {
        self.set(v, POS_AX, SIZE_AX);
    }
    pub fn set_sj(&mut self, v: i32) {
        self.set((v.wrapping_add(OFFSET_SJ)) as u32, POS_SJ, SIZE_SJ);
    }

    /// `CREATE_ABCk`
    #[inline]
    pub fn abck(op: OpCode, a: u32, b: u32, c: u32, k: u32) -> Self {
        Instruction(
            ((op as u32) << POS_OP) | (a << POS_A) | (b << POS_B) | (c << POS_C) | (k << POS_K),
        )
    }
    /// `CREATE_vABCk`
    #[inline]
    pub fn vabck(op: OpCode, a: u32, vb: u32, vc: u32, k: u32) -> Self {
        Instruction(
            ((op as u32) << POS_OP) | (a << POS_A) | (vb << POS_VB) | (vc << POS_VC) | (k << POS_K),
        )
    }
    /// `CREATE_ABx`
    #[inline]
    pub fn abx(op: OpCode, a: u32, bx: u32) -> Self {
        Instruction(((op as u32) << POS_OP) | (a << POS_A) | (bx << POS_BX))
    }
    /// `iAsBx` builder (`bx` biased by `OFFSET_sBx`).
    #[inline]
    pub fn asbx(op: OpCode, a: u32, sbx: i32) -> Self {
        Self::abx(op, a, sbx.wrapping_add(OFFSET_SBX) as u32)
    }
    /// `CREATE_Ax`
    #[inline]
    pub fn ax_(op: OpCode, ax: u32) -> Self {
        Instruction(((op as u32) << POS_OP) | (ax << POS_AX))
    }
    /// `CREATE_sJ` with a signed jump offset (biased by `OFFSET_sJ`).
    #[inline]
    pub fn sj_(op: OpCode, sj: i32, k: u32) -> Self {
        Instruction(
            ((op as u32) << POS_OP)
                | ((sj.wrapping_add(OFFSET_SJ) as u32) << POS_SJ)
                | (k << POS_K),
        )
    }
}

/// Total conversion from a raw 7-bit (or any 8-bit) value to an opcode.
#[inline]
pub fn opcode_from_u8(v: u8) -> Option<OpCode> {
    if (v as usize) < NUM_OPCODES {
        // Table lookup (no unsafe transmute); order verified in tests.
        Some(ALL_OPCODES[v as usize])
    } else {
        None
    }
}

macro_rules! all_ops {
    ($($n:ident),* $(,)?) => { [$(OpCode::$n),*] };
}

#[cfg(not(rustynail_lua54))]
const ALL_OPCODES: [OpCode; NUM_OPCODES] = all_ops![
    Move, Loadi, Loadf, Loadk, Loadkx, Loadfalse, Lfalseskip, Loadtrue, Loadnil, Getupval,
    Setupval, Gettabup, Gettable, Geti, Getfield, Settabup, Settable, Seti, Setfield, Newtable,
    SelfOp, Addi, Addk, Subk, Mulk, Modk, Powk, Divk, Idivk, Bandk, Bork, Bxork, Shli, Shri, Add,
    Sub, Mul, Mod, Pow, Div, Idiv, Band, Bor, Bxor, Shl, Shr, Mmbin, Mmbini, Mmbink, Unm, Bnot,
    Not, Len, Concat, Close, Tbc, Jmp, Eq, Lt, Le, Eqk, Eqi, Lti, Lei, Gti, Gei, Test, Testset,
    Call, Tailcall, Return, Return0, Return1, Forloop, Forprep, Tforprep, Tforcall, Tforloop,
    Setlist, Closure, Vararg, Getvarg, Errnnil, Varargprep, Extraarg,
];

#[cfg(rustynail_lua54)]
const ALL_OPCODES: [OpCode; NUM_OPCODES] = all_ops![
    Move, Loadi, Loadf, Loadk, Loadkx, Loadfalse, Lfalseskip, Loadtrue, Loadnil, Getupval,
    Setupval, Gettabup, Gettable, Geti, Getfield, Settabup, Settable, Seti, Setfield, Newtable,
    SelfOp, Addi, Addk, Subk, Mulk, Modk, Powk, Divk, Idivk, Bandk, Bork, Bxork, Shri, Shli, Add,
    Sub, Mul, Mod, Pow, Div, Idiv, Band, Bor, Bxor, Shl, Shr, Mmbin, Mmbini, Mmbink, Unm, Bnot,
    Not, Len, Concat, Close, Tbc, Jmp, Eq, Lt, Le, Eqk, Eqi, Lti, Lei, Gti, Gei, Test, Testset,
    Call, Tailcall, Return, Return0, Return1, Forloop, Forprep, Tforprep, Tforcall, Tforloop,
    Setlist, Closure, Vararg, Varargprep, Extraarg,
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ver::active::OP_NAMES;

    #[test]
    fn opcode_table_matches_discriminants() {
        for (i, op) in ALL_OPCODES.iter().enumerate() {
            assert_eq!(*op as usize, i, "{}", OP_NAMES[i]);
            assert_eq!(opcode_from_u8(i as u8), Some(*op));
        }
    }

    #[test]
    fn opcode_total_on_all_7bit_values() {
        for v in 0..128u32 {
            let i = Instruction(v | 0xffff_ff80);
            assert_eq!(i.opcode().is_some(), (v as usize) < NUM_OPCODES);
        }
        assert_eq!(Instruction(u32::MAX).opcode(), None);
    }

    #[test]
    fn constants() {
        assert_eq!(MAXARG_BX, 131071);
        assert_eq!(OFFSET_SBX, 65535);
        assert_eq!(MAXARG_AX, 33554431);
        assert_eq!(OFFSET_SJ, 16777215);
        assert_eq!(OFFSET_SC, 127);
        assert_eq!(MAXARG_VC, 1023);
        assert_eq!(MAXARG_VB, 63);
        assert_eq!(int2sc(-1), 126);
        assert_eq!(sc2int(127), 0);
    }

    #[test]
    fn abck_roundtrip_exhaustive_edges() {
        for &a in &[0, 1, 127, 255] {
            for b in 0..=255u32 {
                for &c in &[0, 1, 100, 255] {
                    for k in 0..2 {
                        let i = Instruction::abck(OpCode::Add, a, b, c, k);
                        assert_eq!(
                            (i.opcode(), i.a(), i.b(), i.c(), i.k()),
                            (Some(OpCode::Add), a, b, c, k)
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn vabck_roundtrip() {
        for vb in 0..=MAXARG_VB {
            for &vc in &[0, 1, 511, MAXARG_VC] {
                let i = Instruction::vabck(OpCode::Newtable, 200, vb, vc, 1);
                assert_eq!((i.a(), i.vb(), i.vc(), i.k()), (200, vb, vc, 1));
                assert_eq!(i.opcode(), Some(OpCode::Newtable));
            }
        }
    }

    #[test]
    fn bx_sbx_ax_sj_roundtrip() {
        for &bx in &[0, 1, 65535, 65536, MAXARG_BX] {
            let i = Instruction::abx(OpCode::Loadk, 9, bx);
            assert_eq!((i.a(), i.bx()), (9, bx));
        }
        for &s in &[-OFFSET_SBX, -1, 0, 1, OFFSET_SBX] {
            let i = Instruction::asbx(OpCode::Loadi, 3, s);
            assert_eq!((i.a(), i.sbx()), (3, s));
            let mut j = Instruction::abx(OpCode::Loadi, 3, 0);
            j.set_sbx(s);
            assert_eq!(i, j);
        }
        for &x in &[0, 1, MAXARG_AX] {
            let i = Instruction::ax_(OpCode::Extraarg, x);
            assert_eq!(i.ax(), x);
            assert_eq!(i.opcode(), Some(OpCode::Extraarg));
        }
        for &j in &[-OFFSET_SJ, -5, 0, 5, OFFSET_SJ] {
            let i = Instruction::sj_(OpCode::Jmp, j, 0);
            assert_eq!(i.sj(), j);
            let mut m = Instruction::sj_(OpCode::Jmp, 0, 0);
            m.set_sj(j);
            assert_eq!(i, m);
        }
    }

    #[test]
    fn setters_do_not_disturb_other_fields() {
        let mut i = Instruction::abck(OpCode::Move, 1, 2, 3, 1);
        i.set_a(255);
        assert_eq!((i.a(), i.b(), i.c(), i.k()), (255, 2, 3, 1));
        i.set_b(77);
        i.set_c(88);
        i.set_k(0);
        assert_eq!(
            (i.opcode(), i.a(), i.b(), i.c(), i.k()),
            (Some(OpCode::Move), 255, 77, 88, 0)
        );
        i.set_opcode(OpCode::Call);
        assert_eq!((i.opcode(), i.a(), i.b()), (Some(OpCode::Call), 255, 77));
        // oversize values are masked, not spilled
        i.set_a(0x1ff);
        assert_eq!((i.a(), i.k()), (255, 0));
    }

    #[test]
    fn hand_computed_words() {
        // lopcodes.h layout: MOVE A=1 B=2 -> op 0 | 1<<7 | 2<<16
        assert_eq!(Instruction::abck(OpCode::Move, 1, 2, 0, 0).0, 0x0002_0080);
        // RETURN (op 70) A=0 B=1 C=1 k=0
        assert_eq!(
            Instruction::abck(OpCode::Return, 0, 1, 1, 0).0,
            70 | (1 << 16) | (1 << 24)
        );
        // LOADI A=0 sBx=0 -> Bx = 65535 at pos 15
        assert_eq!(Instruction::asbx(OpCode::Loadi, 0, 0).0, 1 | (65535 << 15));
    }

    #[test]
    fn signed_c() {
        let i = Instruction::abck(OpCode::Addi, 0, 0, int2sc(-3) as u32, 0);
        assert_eq!(i.sc(), -3);
        assert_eq!(i.test_k(), 0);
        assert_eq!(
            Instruction::abck(OpCode::Test, 0, 0, 0, 1).test_k(),
            1 << POS_K
        );
    }
}
