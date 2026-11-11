//! Typed register and condition-code definitions for the x86-64 encoder.

/// A 64-bit general purpose register (`rax` = 0 ... `r15` = 15).
///
/// The same type names the 32-bit and 8-bit views (`eax`, `al`, ...); the operand size is chosen by the
/// instruction method, not by the register.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Gpr(u8);

impl Gpr {
    /// All sixteen registers in encoding order.
    pub const ALL: [Gpr; 16] = [
        RAX, RCX, RDX, RBX, RSP, RBP, RSI, RDI, R8, R9, R10, R11, R12, R13, R14, R15,
    ];

    /// Register from its hardware number `0..16`. Panics if `n >= 16`.
    pub const fn new(n: u8) -> Gpr {
        assert!(n < 16, "general purpose register number out of range");
        Gpr(n)
    }

    /// The hardware register number `0..16`.
    pub const fn index(self) -> u8 {
        self.0
    }
}

/// An SSE register `xmm0..xmm15`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Xmm(u8);

impl Xmm {
    /// All sixteen registers in encoding order.
    pub const ALL: [Xmm; 16] = [
        XMM0, XMM1, XMM2, XMM3, XMM4, XMM5, XMM6, XMM7, XMM8, XMM9, XMM10, XMM11, XMM12, XMM13,
        XMM14, XMM15,
    ];

    /// Register from its number `0..16`. Panics if `n >= 16`.
    pub const fn new(n: u8) -> Xmm {
        assert!(n < 16, "xmm register number out of range");
        Xmm(n)
    }

    /// The hardware register number `0..16`.
    pub const fn index(self) -> u8 {
        self.0
    }
}

macro_rules! gprs {
    ($($name:ident = $n:expr),* $(,)?) => {
        $(
            #[doc = concat!("General purpose register number ", stringify!($n), ".")]
            pub const $name: Gpr = Gpr($n);
        )*
    };
}
gprs! {
    RAX = 0, RCX = 1, RDX = 2, RBX = 3, RSP = 4, RBP = 5, RSI = 6, RDI = 7,
    R8 = 8, R9 = 9, R10 = 10, R11 = 11, R12 = 12, R13 = 13, R14 = 14, R15 = 15,
}

macro_rules! xmms {
    ($($name:ident = $n:expr),* $(,)?) => {
        $(
            #[doc = concat!("SSE register number ", stringify!($n), ".")]
            pub const $name: Xmm = Xmm($n);
        )*
    };
}
xmms! {
    XMM0 = 0, XMM1 = 1, XMM2 = 2, XMM3 = 3, XMM4 = 4, XMM5 = 5, XMM6 = 6, XMM7 = 7,
    XMM8 = 8, XMM9 = 9, XMM10 = 10, XMM11 = 11, XMM12 = 12, XMM13 = 13, XMM14 = 14, XMM15 = 15,
}

/// Operand size of an integer instruction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Size {
    /// 32-bit operation (the result is zero-extended into the 64-bit register).
    S32,
    /// 64-bit operation (`REX.W`).
    S64,
}

/// Condition codes (the low nibble of `jcc` / `setcc` / `cmovcc` opcodes).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Cond {
    /// Overflow (`OF=1`).
    O = 0,
    /// No overflow.
    No = 1,
    /// Below / carry (unsigned `<`).
    B = 2,
    /// Above or equal / no carry (unsigned `>=`).
    Ae = 3,
    /// Equal / zero.
    E = 4,
    /// Not equal / not zero.
    Ne = 5,
    /// Below or equal (unsigned `<=`).
    Be = 6,
    /// Above (unsigned `>`).
    A = 7,
    /// Sign (negative).
    S = 8,
    /// No sign.
    Ns = 9,
    /// Parity even (also "unordered" after `ucomisd`).
    P = 10,
    /// Parity odd.
    Np = 11,
    /// Less (signed `<`).
    L = 12,
    /// Greater or equal (signed `>=`).
    Ge = 13,
    /// Less or equal (signed `<=`).
    Le = 14,
    /// Greater (signed `>`).
    G = 15,
}

impl Cond {
    /// All sixteen condition codes in encoding order.
    pub const ALL: [Cond; 16] = [
        Cond::O,
        Cond::No,
        Cond::B,
        Cond::Ae,
        Cond::E,
        Cond::Ne,
        Cond::Be,
        Cond::A,
        Cond::S,
        Cond::Ns,
        Cond::P,
        Cond::Np,
        Cond::L,
        Cond::Ge,
        Cond::Le,
        Cond::G,
    ];

    /// The logically inverted condition (flips the low bit of the code).
    pub const fn invert(self) -> Cond {
        Cond::ALL[(self as u8 ^ 1) as usize]
    }
}

/// Two-operand integer ALU operations sharing the classic `00..3F` opcode layout.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Alu {
    /// `add`
    Add = 0,
    /// `or`
    Or = 1,
    /// `adc`
    Adc = 2,
    /// `sbb`
    Sbb = 3,
    /// `and`
    And = 4,
    /// `sub`
    Sub = 5,
    /// `xor`
    Xor = 6,
    /// `cmp`
    Cmp = 7,
}

impl Alu {
    /// All eight operations in encoding order.
    pub const ALL: [Alu; 8] = [
        Alu::Add,
        Alu::Or,
        Alu::Adc,
        Alu::Sbb,
        Alu::And,
        Alu::Sub,
        Alu::Xor,
        Alu::Cmp,
    ];
}

/// Shift operations (the `/r` extension of the `C1`/`D3` opcode group).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Shift {
    /// `shl` (logical left).
    Shl = 4,
    /// `shr` (logical right).
    Shr = 5,
    /// `sar` (arithmetic right).
    Sar = 7,
}

/// Scalar double SSE2 arithmetic operations (`F2 0F xx`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum SseOp {
    /// `sqrtsd`
    Sqrt = 0x51,
    /// `addsd`
    Add = 0x58,
    /// `mulsd`
    Mul = 0x59,
    /// `subsd`
    Sub = 0x5C,
    /// `minsd`
    Min = 0x5D,
    /// `divsd`
    Div = 0x5E,
    /// `maxsd`
    Max = 0x5F,
}

/// Packed-double bitwise SSE2 operations (`66 0F xx`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum SseLogic {
    /// `andpd`
    And = 0x54,
    /// `andnpd`
    Andn = 0x55,
    /// `xorpd`
    Xor = 0x57,
}
