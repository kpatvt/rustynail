//! Micro-op IR of the template compiler (docs/JIT_DESIGN.md section 8).
//!
//! A template is a straight-line function from one bytecode to [`Op`]s; labels and branches connect
//! them. Two backends run the *same* [`Program`]: the x86-64 emitter (`x86`) and the Rust emulator
//! (`emu`, which Miri can execute). Machine state is a handful of base pointers (the frame's registers,
//! the `CallInfo`, the thread, the constants, the block counter), nine virtual 64-bit registers and sixteen
//! virtual doubles (`X0..X15`, tier 1.5 only).
//!
//! # Register file and clobbers (x86-64 mapping in `x86`)
//!
//! `V0..V8` = `rsi rdi r8 r9 r10 r11 rcx rax rdx`; `X0..X15` = `xmm0..xmm15`. Tier-1 templates use only
//! `V0..V6` and `X0/X1` (as hidden scratch); `V7`/`V8` (`rax`/`rdx`) and `X2..X15` are for region code.
//! Ops that clobber registers they do not name:
//!
//! * `SDivRem`, `UDiv`: `rax` and `rdx` (= `V7`, `V8`); every operand must be `V0..V6` (asserted by the emitter).
//!   The emulator poisons `V7`/`V8` (`emu::POISON`) afterwards, so nothing may read them.
//! * `MulHiU`: `rax` and `rdx` (`V7`, `V8`), operands `V0..V6` or `V8`.
//! * `ReloadFrame`: `V7`, `V8`. `Ret`: `V7`, `V8` (leaves compiled code anyway).
//! * `CallH`: every `V` and every `X` (SysV: all of them are caller-saved), result in `V0`.
//! * `FAdd FSub FMul FDiv I2F FBr`: `X0` and `X1`. `F2IExact`, `FNeg`, `FAbs`: `X0` (`X1` when the operand is `X0`); `FNeg`/`FAbs` also `V7`.
//! * `ShlV`/`ShrV`: the count is `V6`.
//! * `BrM`/`BrMi` with `W::B1` compare signed 8-bit values for every `Cc` (tested for all six).
//! * Every other op touches only the registers it names (flags are never live across ops). In particular
//!   `Mov MovR Ld St Lea Add.. Br Jmp CountSub LdF StF MovF FBin FBinM FSqrt GtoF FtoG I2Fx FBrX BrM BrMi LdI StI
//!   LdFi StFi` write only their destination (or memory), `SetFrame/SetCi/SetK` write the pinned register.
//!   (The tier-1 emitter helpers `copy_value`, `poll`, `set_const`, `ld_f` are compositions of these ops and
//!   clobber `V0..V2,V5,V6`; region code does not call them.)

/// Label id (index into the program's label table).
pub type L = u32;
/// Virtual register `0..NV`.
pub type V = u8;
/// Number of virtual registers. `V7` = `rax` and `V8` = `rdx` are allocatable by region code only (tier-1
/// templates never use them), see the module docs for the ops that clobber them. P2 bundle 1: `V9` = `rbp` and `V10` = `r14`
/// (callee-saved: [`Op::CallH`] keeps them) are allocatable by the trace compiler only (`RaOpts::xregs`); `V10` is the pinned
/// constants register of tier 1, so code that allocates it ends every `JmpR` with [`Op::ReloadK`].
pub const NV: usize = 11;
/// Virtual double register `0..NX` (`xmm0..xmm15`). The emulator keeps the bit pattern of the low lane.
pub type X = u8;
/// Number of virtual double registers. `X0`/`X1` are the hidden scratch of the tier-1 float ops.
pub const NX: usize = 16;

/// What a memory operand is relative to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Base {
    /// Register 0 of the running frame (`rbx`).
    Frame,
    /// The running `CallInfo` (`r13`).
    Ci,
    /// The `Thread` (`r12`).
    Thread,
    /// The constants array of the running closure (`r14`).
    K,
    /// The instruction-count budget word (`r15`).
    Cnt,
    /// A virtual register used as a pointer.
    Reg(V),
}

/// Memory operand `[base + off]`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct M {
    /// Base.
    pub base: Base,
    /// Byte offset.
    pub off: i32,
}

/// Indexed memory operand `[base + idx * scale + off]` (`scale` is 1, 2, 4 or 8; `base` and `idx` are virtual
/// registers, address arithmetic wraps).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Mi {
    /// Base register.
    pub base: V,
    /// Index register.
    pub idx: V,
    /// Scale factor.
    pub scale: u8,
    /// Byte offset.
    pub off: i32,
}

/// Binary double operation of [`Op::FBin`] / [`Op::FBinM`]: `dst = dst <op> src`. `Min`/`Max` have the exact
/// `minsd`/`maxsd` semantics: `min(a, b) = if a < b { a } else { b }` (the *second* operand is returned when
/// either is a NaN or both are equal, so `min(+0, -0) = -0` and `min(-0, +0) = +0`), `max` likewise with `>`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FOp {
    /// `addsd`
    Add,
    /// `subsd`
    Sub,
    /// `mulsd`
    Mul,
    /// `divsd`
    Div,
    /// `minsd`
    Min,
    /// `maxsd`
    Max,
}

/// Access width.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum W {
    /// 1 byte (loads zero-extend).
    B1,
    /// 4 bytes (loads zero-extend).
    B4,
    /// 8 bytes.
    B8,
}

/// Second operand: a register or a 32-bit immediate (sign-extended to 64 bits).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Src {
    /// Virtual register.
    V(V),
    /// Immediate.
    Imm(i32),
}

/// Branch conditions (comparing two 64-bit values).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cc {
    /// `==`
    Eq,
    /// `!=`
    Ne,
    /// signed `<`
    Lt,
    /// signed `<=`
    Le,
    /// signed `>`
    Gt,
    /// signed `>=`
    Ge,
    /// unsigned `<` (below; integer compares only: `FBr`/`FBrX` reject it)
    Ult,
    /// unsigned `>=` (above or equal; integer compares only)
    Uge,
}

/// One micro-op.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Op {
    /// Define a label here.
    Bind(L),
    /// Pad with multi-byte NOPs to a multiple of `n` bytes (power of two; only the trace assembler emits it; a
    /// no-op for the emulator). Absolute alignment holds when the block is placed `n`-aligned.
    Align(u8),
    /// Emit `n` bytes of multi-byte NOPs.
    Pad(u8),
    /// `dst = imm` (64 bits).
    Mov(V, u64),
    /// `dst = src`.
    MovR(V, V),
    /// `dst = zero_extend(load w [m])`.
    Ld {
        /// Destination.
        dst: V,
        /// Width.
        w: W,
        /// Address.
        m: M,
    },
    /// `store w [m] = src` (immediates are sign-extended to the width; `B1` stores the low byte).
    St {
        /// Width.
        w: W,
        /// Address.
        m: M,
        /// Value.
        src: Src,
    },
    /// `dst = address of m`.
    Lea(V, M),
    /// `dst += src` (wrapping).
    Add(V, Src),
    /// `dst -= src` (wrapping).
    Sub(V, Src),
    /// `dst &= src`.
    And(V, Src),
    /// `dst *= src` (wrapping, low 64 bits).
    Mul(V, Src),
    /// `dst |= src`.
    Or(V, Src),
    /// `dst ^= src`.
    Xor(V, Src),
    /// `dst = -dst` (wrapping).
    Neg(V),
    /// `dst = !dst`.
    Not(V),
    /// `dst <<= n` (`n < 64`).
    ShlI(V, u8),
    /// `dst >>= n` logical (`n < 64`).
    ShrI(V, u8),
    /// `dst >>= n` arithmetic (`n < 64`; P-N: floor division by a power of two).
    SarI(V, u8),
    /// `dst <<= (V6 & 63)` (the count lives in `V6` = `rcx`; `dst` must not be `V6`).
    ShlV(V),
    /// `dst >>= (V6 & 63)` logical (same contract as `ShlV`).
    ShrV(V),
    /// Signed truncating division: `q = a / b`, `r = a % b`; the template guarantees `b != 0` and
    /// `b != -1` (so no overflow trap). `q != r`.
    SDivRem {
        /// Quotient destination.
        q: V,
        /// Remainder destination.
        r: V,
        /// Dividend.
        a: V,
        /// Divisor.
        b: V,
    },
    /// B4-4: `dst = high 64 bits of (a * k)` (unsigned, 128-bit product); `a` and `dst` are `V0..V6` or `V8`. Clobbers `V7` and `V8`
    /// (`rax`, `rdx`) like `SDivRem`; the emulator poisons them (after writing `dst`). Magic-number division by a constant.
    MulHiU {
        /// Destination.
        dst: V,
        /// Multiplicand.
        a: V,
        /// The 64-bit constant multiplier.
        k: u64,
    },
    /// Float ops on the bit patterns held in virtual registers: `dst = dst <op> src` (IEEE double).
    FAdd(V, V),
    /// `dst = dst - src` (double).
    FSub(V, V),
    /// `dst = dst * src` (double).
    FMul(V, V),
    /// `dst = dst / src` (double).
    FDiv(V, V),
    /// `dst = (dst as i64) as f64` (bit pattern of the double).
    I2F(V),
    /// `if a <cc> b goto to` on doubles (bit patterns); every comparison with a NaN is false, including `Ne`
    /// being *true* for NaN as in IEEE (`!=`).
    FBr {
        /// Condition (`Lt`, `Le`, `Gt`, `Ge`, `Eq`, `Ne`).
        cc: Cc,
        /// Left operand.
        a: V,
        /// Right operand.
        b: V,
        /// Target.
        to: L,
    },
    /// `dst = a / b` (unsigned; `b != 0` is the template's obligation).
    UDiv {
        /// Destination.
        dst: V,
        /// Dividend.
        a: V,
        /// Divisor.
        b: V,
    },
    /// `if a <cc> b goto to`.
    Br {
        /// Condition.
        cc: Cc,
        /// Left operand.
        a: V,
        /// Right operand.
        b: Src,
        /// Target.
        to: L,
    },
    /// `d -= imm` (64-bit, wrapping); `goto to` if the result is negative as a signed 64-bit value (`sub d, imm; jl to`: one fused
    /// compare-and-branch). Used by the trace budget (P2-3a); the exit stub refunds `imm` before it reaches the real exit.
    SubJl {
        /// Register (updated).
        d: V,
        /// Immediate subtracted.
        imm: i32,
        /// Taken when the result is negative.
        to: L,
    },
    /// `goto l`.
    Jmp(L),
    /// `*cnt -= n` (32-bit, wrapping); `goto exit` if the result is negative (C1 block counting).
    CountSub {
        /// Bytecodes of the block.
        n: u32,
        /// Taken when the budget is exhausted.
        exit: L,
    },
    /// Leave compiled code with this exit code (`JitExit as u32`).
    Ret(u32),
    // ---- tier 1.5 (docs/JIT_TIER15.md 4.2): doubles in X registers, memory-operand compares, indexed addressing ----
    /// `dst = load f64 [m]` (`movsd`; the bits are copied unchanged).
    LdF {
        /// Destination.
        dst: X,
        /// Address.
        m: M,
    },
    /// `store f64 [m] = src`.
    StF {
        /// Address.
        m: M,
        /// Source.
        src: X,
    },
    /// `dst = src` (`movaps`).
    MovF(X, X),
    /// `dst = dst <op> src` (IEEE double; see [`FOp`] for `Min`/`Max`). NaN results follow x86: a NaN first
    /// operand wins (quieted), else a NaN second operand (quieted), an invalid operation gives the default NaN
    /// `0xFFF8_0000_0000_0000`.
    FBin {
        /// Operation.
        op: FOp,
        /// Destination and first operand.
        dst: X,
        /// Second operand.
        src: X,
    },
    /// `dst = dst <op> load f64 [m]`.
    FBinM {
        /// Operation.
        op: FOp,
        /// Destination and first operand.
        dst: X,
        /// Second operand.
        m: M,
    },
    /// `dst = sqrt(src)` (`sqrtsd`; `sqrt(-0) = -0`, negative gives the default NaN).
    FSqrt(X, X),
    /// F1: `dst = floor(src)` (`roundsd` mode 9, SSE4.1: the caller checks the CPU; NaN stays NaN, `floor(-0) = -0`).
    FFloor(X, X),
    /// `x = -x` (sign bit flipped, also for NaN and zero). Clobbers `X0` (`X1` if `x` is `X0`) and `V7`.
    FNeg(X),
    /// `x = |x|` (sign bit cleared). Clobbers `X0` (`X1` if `x` is `X0`) and `V7`.
    FAbs(X),
    /// `x = f64::from_bits(v)` (`movq xmm, r64`).
    GtoF(X, V),
    /// `v = x.to_bits()` (`movq r64, xmm`).
    FtoG(V, X),
    /// `x = (v as i64) as f64` with a dependency-breaking `xorps` first.
    I2Fx(X, V),
    /// `dst = (src as i64)` if `src` is an integral value in `[-2^63, 2^63)` (so `-0.0` gives 0), else
    /// `goto to`. On the failing path `dst` holds what `cvttsd2si` left there (the truncation, or
    /// `i64::MIN` for NaN/out of range). Clobbers `X0` (`X1` if `src` is `X0`).
    /// BUNDLE-P P2: [`Op::F2IExact`] for a source that is already integral or NaN / inf / out of range (a `FFLOOR` result): `dst = cvttsd2si(src)`, `goto to` when that is
    /// `i64::MIN` (NaN, +-inf, out of range, and also an exact `-2^63`: conservative). `cmp dst, 1; jo` (`i64::MIN - 1` overflows), no scratch, no `X0`.
    F2IFloor { dst: V, src: X, to: L },
    F2IExact {
        /// Integer destination.
        dst: V,
        /// Float source.
        src: X,
        /// Taken if `src` is not exactly an `i64`.
        to: L,
    },
    /// `if a <cc> b goto to` on doubles held in X registers; NaN rules exactly as [`Op::FBr`]. No scratch.
    FBrX {
        /// Condition (`Lt`, `Le`, `Gt`, `Ge`, `Eq`, `Ne`).
        cc: Cc,
        /// Left operand.
        a: X,
        /// Right operand.
        b: X,
        /// Target.
        to: L,
    },
    /// `if load_w[m] <cc> imm goto to` (`cmp byte/dword/qword [m], imm` + `jcc`). Both sides are compared as
    /// *signed* `w`-bit integers; `imm` is truncated to the width (`B1`: low byte, `B4`: low 32 bits, `B8`:
    /// sign-extended 32-bit immediate).
    BrM {
        /// Condition.
        cc: Cc,
        /// Width of the memory operand.
        w: W,
        /// Address.
        m: M,
        /// Immediate.
        imm: i32,
        /// Target.
        to: L,
    },
    /// B4-3: `if load16[m] <cc> imm goto to` (`cmp word [m], imm16` + `jcc`), `cc` is `Eq` or `Ne`: two adjacent tag bytes at once.
    BrM16 {
        /// Condition (`Eq` / `Ne`).
        cc: Cc,
        /// Address.
        m: M,
        /// Immediate (compared as a 16-bit pattern).
        imm: u16,
        /// Target.
        to: L,
    },
    /// H3-F: `if load64[m] <cc> V[src] goto to` (`cmp [m], r64` + `jcc`), `cc` is `Eq` or `Ne`.
    BrMR {
        /// Condition (`Eq` / `Ne`).
        cc: Cc,
        /// Address.
        m: M,
        /// The register compared with the quadword.
        src: V,
        /// Target.
        to: L,
    },
    /// `*(uw*)m += imm` (`w` = `B4` or `B8`, wrapping; `imm` sign-extended to the width). J5 allocation sinking: the
    /// virtual-allocation counter (`B8`); P-L: the exit counters (`B4`, memory form of the inline exit stub).
    AddM {
        /// Width of the memory operand (`B4` or `B8`).
        w: W,
        /// Address.
        m: M,
        /// Immediate.
        imm: i32,
    },
    /// [`Op::BrM`] with an indexed address.
    BrMi {
        /// Condition.
        cc: Cc,
        /// Width of the memory operand.
        w: W,
        /// Address.
        m: Mi,
        /// Immediate.
        imm: i32,
        /// Target.
        to: L,
    },
    /// `dst = zero_extend(load w [m])` with an indexed address.
    LdI {
        /// Destination.
        dst: V,
        /// Width.
        w: W,
        /// Address.
        m: Mi,
    },
    /// `store w [m] = src` with an indexed address (same immediate rules as [`Op::St`]).
    StI {
        /// Width.
        w: W,
        /// Address.
        m: Mi,
        /// Value.
        src: Src,
    },
    /// `dst = load f64 [m]` with an indexed address.
    LdFi {
        /// Destination.
        dst: X,
        /// Address.
        m: Mi,
    },
    /// `store f64 [m] = src` with an indexed address.
    StFi {
        /// Address.
        m: Mi,
        /// Source.
        src: X,
    },
    // ---- J1c: calls and returns (`SetFrame` is native only: the emulator reports `Stop::Unsupported`) ----
    /// `dst <<= n`.
    Shl(V, u8),
    /// Pinned `rbx` (frame base) `= V`.
    SetFrame(V),
    /// Pinned `r13` (the `CallInfo`) `= V`.
    SetCi(V),
    /// Pinned `r14` (constants) `= V`.
    SetK(V),
    /// P2 bundle 1: pinned `r14 = *(r13 + CallInfo::kcache)` (the constants of the frame in `r13`). The emulator does nothing (its `k`
    /// is the one of the `Env`; trace code never reads it).
    ReloadK,
    /// `goto *V` (an absolute native address).
    JmpR(V),
    /// Pinned `rbx = *(thread + stack_off) + (ci.func << 4) + 16`: the frame base re-derived from the
    /// thread's stack pointer after anything that can move the stack. Clobbers `V7` and `V8` (`rax`, `rdx`).
    ReloadFrame {
        /// Offset of the stack data pointer inside the `Thread`.
        stack_off: i32,
    },
    /// `V0 = f(thread, ci, a, b)` (C ABI, 32-bit result zero-extended). Clobbers every virtual register except the callee-saved
    /// `V9` / `V10` (`rbp`, `r14`), including `V7`/`V8` and every `X`.
    CallH {
        /// Address of the `extern "C"` helper.
        f: u64,
        /// Third argument.
        a: u32,
        /// Fourth argument.
        b: u32,
    },
}

/// A lowered function: ops plus the number of labels.
#[derive(Clone, Debug, Default)]
pub struct Program {
    /// The ops in program order.
    pub ops: Vec<Op>,
    /// Number of labels allocated.
    pub nlabels: u32,
}

impl Program {
    /// A fresh label.
    pub fn label(&mut self) -> L {
        self.nlabels += 1;
        self.nlabels - 1
    }
    /// Append an op.
    pub fn push(&mut self, op: Op) {
        self.ops.push(op);
    }
}
