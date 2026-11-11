//! x86-64 backend of the micro-op IR and the `jit_enter` thunk.
//!
//! Register assignment (docs/JIT_DESIGN.md section 1): `rbx` = frame base, `r12` = thread, `r13` = `CallInfo`,
//! `r14` = constants, `r15` = instruction budget word; virtual registers V0..V8 = `rsi rdi r8 r9 r10 r11
//! rcx rax rdx` (tier-1 templates use V0..V6 only; `rax`/`rdx` are also the implicit operands of division and the
//! exit-code register, see the clobber list in `ir`), `V9` = `rbp` and `V10` = `r14` (trace compiler only, see `ir::NV`), `X0..X15` = `xmm0..xmm15`.
//! `rbp` is the thunk's frame-pointer chain (nothing in generated code reads it; the thunk restores it).

use crate::arch::x86_64::*;
use crate::ir::{Base, Cc, FOp, Mi, Op, Program, Src, L, M, W, X};
use rn_core::jit_layout::callinfo;

const VREG: [Gpr; crate::ir::NV] = [RSI, RDI, R8, R9, R10, R11, RCX, RAX, RDX, RBP, R14];

/// Stack bytes the thunk reserves after its six pushes (alignment).
const THUNK_PAD: i32 = 8;

fn vr(v: u8) -> Gpr {
    VREG[v as usize]
}

fn mem(m: &M) -> Mem {
    let b = match m.base {
        Base::Frame => RBX,
        Base::Ci => R13,
        Base::Thread => R12,
        Base::K => R14,
        Base::Cnt => R15,
        Base::Reg(v) => vr(v),
    };
    Mem::base_disp(b, m.off)
}

fn memi(m: &Mi) -> Mem {
    Mem::sib(vr(m.base), vr(m.idx), m.scale, m.off)
}

/// The scratch X register of ops that need one: `X0`, or `X1` when the operand itself is `X0`.
fn scratch(operand: X) -> X {
    if operand == 0 {
        1
    } else {
        0
    }
}

fn xr(x: X) -> Xmm {
    Xmm::new(x)
}

fn sse_op(op: FOp) -> SseOp {
    match op {
        FOp::Add => SseOp::Add,
        FOp::Sub => SseOp::Sub,
        FOp::Mul => SseOp::Mul,
        FOp::Div => SseOp::Div,
        FOp::Min => SseOp::Min,
        FOp::Max => SseOp::Max,
    }
}

fn cond(cc: Cc) -> Cond {
    match cc {
        Cc::Eq => Cond::E,
        Cc::Ne => Cond::Ne,
        Cc::Lt => Cond::L,
        Cc::Le => Cond::Le,
        Cc::Gt => Cond::G,
        Cc::Ge => Cond::Ge,
        Cc::Ult => Cond::B,
        Cc::Uge => Cond::Ae,
    }
}

/// Machine code of a program.
pub struct Emitted {
    /// The bytes (position independent except for nothing: all branches are relative).
    pub code: Vec<u8>,
    /// Offset of every label (`None` if the label was never bound).
    pub label_off: Vec<Option<usize>>,
}

/// Assemble `prog`.
pub fn emit(prog: &Program) -> Result<Emitted, AsmError> {
    let mut a = Assembler::new();
    let labels: Vec<Label> = (0..prog.nlabels).map(|_| a.new_label()).collect();
    let lab = |l: L| labels[l as usize];
    for op in &prog.ops {
        match *op {
            Op::Bind(l) => a.bind(lab(l)),
            Op::Align(n) => a.align(usize::from(n)),
            Op::Pad(n) => a.pad(usize::from(n)),
            Op::Mov(v, i) => a.mov_ri(vr(v), i as i64),
            Op::MovR(d, s) => a.mov_rr(Size::S64, vr(d), vr(s)),
            Op::Ld { dst, w, m } => match w {
                W::B1 => a.movzx_rm8(vr(dst), mem(&m)),
                W::B4 => a.mov_rm(Size::S32, vr(dst), mem(&m)),
                W::B8 => a.mov_rm(Size::S64, vr(dst), mem(&m)),
            },
            Op::St { w, m, src } => match (w, src) {
                (W::B1, Src::V(v)) => a.mov_m8_r(mem(&m), vr(v)),
                (W::B4, Src::V(v)) => a.mov_mr(Size::S32, mem(&m), vr(v)),
                (W::B8, Src::V(v)) => a.mov_mr(Size::S64, mem(&m), vr(v)),
                (W::B1, Src::Imm(i)) => a.mov_m8_i(mem(&m), i as i8),
                (W::B4, Src::Imm(i)) => a.mov_mi(Size::S32, mem(&m), i),
                (W::B8, Src::Imm(i)) => a.mov_mi(Size::S64, mem(&m), i),
            },
            Op::Lea(d, m) => a.lea(Size::S64, vr(d), mem(&m)),
            Op::Add(d, s) => alu(&mut a, Alu::Add, d, s),
            Op::Sub(d, s) => alu(&mut a, Alu::Sub, d, s),
            Op::And(d, s) => alu(&mut a, Alu::And, d, s),
            Op::Mul(d, s) => match s {
                Src::V(v) => a.imul_rr(Size::S64, vr(d), vr(v)),
                Src::Imm(i) => a.imul_rri(Size::S64, vr(d), vr(d), i),
            },
            Op::Or(d, s) => alu(&mut a, Alu::Or, d, s),
            Op::Xor(d, s) => alu(&mut a, Alu::Xor, d, s),
            Op::Neg(d) => a.neg(Size::S64, vr(d)),
            Op::Not(d) => a.not(Size::S64, vr(d)),
            Op::ShlI(d, n) => a.shift_ri(Shift::Shl, Size::S64, vr(d), n),
            Op::ShrI(d, n) => a.shift_ri(Shift::Shr, Size::S64, vr(d), n),
            Op::SarI(d, n) => a.shift_ri(Shift::Sar, Size::S64, vr(d), n),
            Op::ShlV(d) => {
                assert_ne!(d, 6, "shift destination must not be the count register");
                a.shift_cl(Shift::Shl, Size::S64, vr(d));
            }
            Op::ShrV(d) => {
                assert_ne!(d, 6, "shift destination must not be the count register");
                a.shift_cl(Shift::Shr, Size::S64, vr(d));
            }
            Op::SDivRem { q, r, a: x, b } => {
                assert!(
                    [q, r, x, b].iter().all(|&v| v < 7 || v >= 9),
                    "SDivRem operands must be V0..V6"
                );
                a.mov_rr(Size::S64, RAX, vr(x));
                a.cqo();
                a.idiv(Size::S64, vr(b));
                a.mov_rr(Size::S64, vr(q), RAX);
                a.mov_rr(Size::S64, vr(r), RDX);
            }
            Op::MulHiU { dst, a: x, k } => {
                assert!(
                    [dst, x].iter().all(|&v| v < 7 || v >= 8),
                    "MulHiU operands must be V0..V6 or V8"
                );
                a.mov_ri(RAX, k as i64);
                a.mul1(Size::S64, vr(x));
                if dst != 8 {
                    a.mov_rr(Size::S64, vr(dst), RDX);
                }
            }
            Op::FAdd(d, s) => fop(&mut a, SseOp::Add, d, s),
            Op::FSub(d, s) => fop(&mut a, SseOp::Sub, d, s),
            Op::FMul(d, s) => fop(&mut a, SseOp::Mul, d, s),
            Op::FDiv(d, s) => fop(&mut a, SseOp::Div, d, s),
            Op::I2F(d) => {
                a.cvtsi2sd_rr(Size::S64, XMM0, vr(d));
                a.movq_rx(vr(d), XMM0);
            }
            Op::FBr { cc, a: x, b, to } => {
                a.movq_xr(XMM0, vr(x));
                a.movq_xr(XMM1, vr(b));
                fbr(&mut a, cc, XMM0, XMM1, lab(to));
            }
            Op::UDiv { dst, a: x, b } => {
                assert!(
                    [dst, x, b].iter().all(|&v| v < 7 || v >= 9),
                    "UDiv operands must be V0..V6"
                );
                a.mov_rr(Size::S64, RAX, vr(x));
                a.alu_rr(Alu::Xor, Size::S32, RDX, RDX);
                a.div(Size::S64, vr(b));
                a.mov_rr(Size::S64, vr(dst), RAX);
            }
            Op::Br { cc, a: x, b, to } => {
                match b {
                    Src::V(v) => a.alu_rr(Alu::Cmp, Size::S64, vr(x), vr(v)),
                    Src::Imm(i) => a.alu_ri(Alu::Cmp, Size::S64, vr(x), i),
                }
                a.jcc_label(cond(cc), lab(to));
            }
            Op::SubJl { d, imm, to } => {
                a.alu_ri(Alu::Sub, Size::S64, vr(d), imm);
                a.jcc_label(cond(Cc::Lt), lab(to));
            }
            Op::Jmp(l) => a.jmp_label(lab(l)),
            Op::CountSub { n, exit } => {
                a.alu_mi(Alu::Sub, Size::S32, Mem::base(R15), n as i32);
                a.jcc_label(Cond::S, lab(exit));
            }
            Op::Ret(c) => {
                a.mov_ri(RAX, i64::from(c));
                epilogue(&mut a);
            }
            Op::LdF { dst, m } => a.movsd_rm(xr(dst), mem(&m)),
            Op::StF { m, src } => a.movsd_mr(mem(&m), xr(src)),
            Op::MovF(d, s) => a.movaps_rr(xr(d), xr(s)),
            Op::FBin { op, dst, src } => a.sse_rr(sse_op(op), xr(dst), xr(src)),
            Op::FBinM { op, dst, m } => a.sse_rm(sse_op(op), xr(dst), mem(&m)),
            Op::FSqrt(d, s) => a.sse_rr(SseOp::Sqrt, xr(d), xr(s)),
            Op::FFloor(d, s) => a.roundsd_rr(xr(d), xr(s), 9),
            Op::FNeg(x) => {
                let sc = xr(scratch(x));
                a.mov_ri64(RAX, 0x8000_0000_0000_0000);
                a.movq_xr(sc, RAX);
                a.logic_rr(SseLogic::Xor, xr(x), sc);
            }
            Op::FAbs(x) => {
                let sc = xr(scratch(x));
                a.mov_ri64(RAX, 0x7FFF_FFFF_FFFF_FFFF);
                a.movq_xr(sc, RAX);
                a.logic_rr(SseLogic::And, xr(x), sc);
            }
            Op::GtoF(x, v) => a.movq_xr(xr(x), vr(v)),
            Op::FtoG(v, x) => a.movq_rx(vr(v), xr(x)),
            Op::I2Fx(x, v) => {
                a.xorps_rr(xr(x), xr(x));
                a.cvtsi2sd_rr(Size::S64, xr(x), vr(v));
            }
            Op::F2IFloor { dst, src, to } => {
                a.cvttsd2si(Size::S64, vr(dst), xr(src));
                a.alu_ri(Alu::Cmp, Size::S64, vr(dst), 1);
                a.jcc_label(Cond::O, lab(to));
            }
            Op::F2IExact { dst, src, to } => {
                // cvttsd2si gives 0x8000.. for NaN / out of range; converting back and comparing exactly (the
                // unordered case is the parity flag) accepts exactly the integral values in [-2^63, 2^63)
                let sc = xr(scratch(src));
                a.cvttsd2si(Size::S64, vr(dst), xr(src));
                a.xorps_rr(sc, sc);
                a.cvtsi2sd_rr(Size::S64, sc, vr(dst));
                a.ucomisd_rr(sc, xr(src));
                a.jcc_label(Cond::P, lab(to));
                a.jcc_label(Cond::Ne, lab(to));
            }
            Op::FBrX { cc, a: x, b, to } => fbr(&mut a, cc, xr(x), xr(b), lab(to)),
            Op::BrM { cc, w, m, imm, to } => {
                cmp_mem_imm(&mut a, w, mem(&m), imm);
                a.jcc_label(cond(cc), lab(to));
            }
            Op::BrM16 { cc, m, imm, to } => {
                debug_assert!(matches!(cc, Cc::Eq | Cc::Ne));
                a.cmp_m16_i(mem(&m), imm);
                a.jcc_label(cond(cc), lab(to));
            }
            Op::BrMR { cc, m, src, to } => {
                debug_assert!(matches!(cc, Cc::Eq | Cc::Ne));
                a.alu_mr(Alu::Cmp, Size::S64, mem(&m), vr(src));
                a.jcc_label(cond(cc), lab(to));
            }
            Op::AddM { w, m, imm } => {
                debug_assert!(matches!(w, W::B4 | W::B8));
                a.alu_mi(Alu::Add, if w == W::B4 { Size::S32 } else { Size::S64 }, mem(&m), imm)
            }
            Op::BrMi { cc, w, m, imm, to } => {
                cmp_mem_imm(&mut a, w, memi(&m), imm);
                a.jcc_label(cond(cc), lab(to));
            }
            Op::LdI { dst, w, m } => match w {
                W::B1 => a.movzx_rm8(vr(dst), memi(&m)),
                W::B4 => a.mov_rm(Size::S32, vr(dst), memi(&m)),
                W::B8 => a.mov_rm(Size::S64, vr(dst), memi(&m)),
            },
            Op::StI { w, m, src } => match (w, src) {
                (W::B1, Src::V(v)) => a.mov_m8_r(memi(&m), vr(v)),
                (W::B4, Src::V(v)) => a.mov_mr(Size::S32, memi(&m), vr(v)),
                (W::B8, Src::V(v)) => a.mov_mr(Size::S64, memi(&m), vr(v)),
                (W::B1, Src::Imm(i)) => a.mov_m8_i(memi(&m), i as i8),
                (W::B4, Src::Imm(i)) => a.mov_mi(Size::S32, memi(&m), i),
                (W::B8, Src::Imm(i)) => a.mov_mi(Size::S64, memi(&m), i),
            },
            Op::LdFi { dst, m } => a.movsd_rm(xr(dst), memi(&m)),
            Op::StFi { m, src } => a.movsd_mr(memi(&m), xr(src)),
            Op::Shl(d, n) => a.shift_ri(Shift::Shl, Size::S64, vr(d), n),
            Op::SetFrame(v) => a.mov_rr(Size::S64, RBX, vr(v)),
            Op::SetCi(v) => a.mov_rr(Size::S64, R13, vr(v)),
            Op::SetK(v) => a.mov_rr(Size::S64, R14, vr(v)),
            Op::ReloadK => a.mov_rm(Size::S64, R14, Mem::base_disp(R13, callinfo::KCACHE as i32)),
            Op::JmpR(v) => a.jmp_r(vr(v)),
            Op::ReloadFrame { stack_off } => {
                a.mov_rm(Size::S64, RAX, Mem::base_disp(R12, stack_off));
                a.mov_rm(Size::S64, RDX, Mem::base_disp(R13, callinfo::FUNC as i32));
                a.shift_ri(Shift::Shl, Size::S64, RDX, 4);
                a.alu_rr(Alu::Add, Size::S64, RAX, RDX);
                a.lea(Size::S64, RBX, Mem::base_disp(RAX, 16));
            }
            Op::CallH { f, a: x, b } => {
                a.mov_rr(Size::S64, RDI, R12);
                a.mov_rr(Size::S64, RSI, R13);
                a.mov_ri(RDX, i64::from(x));
                a.mov_ri(RCX, i64::from(b));
                a.mov_ri64(RAX, f);
                a.call_r(RAX);
                a.mov_rr(Size::S32, RSI, RAX); // V0 = eax (zero-extended)
            }
        }
    }
    let label_off = labels.iter().map(|&l| a.label_offset(l)).collect();
    let code = a.finish()?;
    Ok(Emitted { code, label_off })
}

/// `ucomisd` + jump with the NaN rules of [`Op::FBr`] (never by negating a condition code).
fn fbr(a: &mut Assembler, cc: Cc, x: Xmm, y: Xmm, t: Label) {
    match cc {
        // ucomisd sets CF/ZF/PF = 111 when unordered: `ja`/`jae` after the right operand order are false
        Cc::Lt => {
            a.ucomisd_rr(y, x);
            a.jcc_label(Cond::A, t);
        }
        Cc::Le => {
            a.ucomisd_rr(y, x);
            a.jcc_label(Cond::Ae, t);
        }
        Cc::Gt => {
            a.ucomisd_rr(x, y);
            a.jcc_label(Cond::A, t);
        }
        Cc::Ge => {
            a.ucomisd_rr(x, y);
            a.jcc_label(Cond::Ae, t);
        }
        Cc::Eq => {
            let skip = a.new_label();
            a.ucomisd_rr(x, y);
            a.jcc_label(Cond::P, skip);
            a.jcc_label(Cond::E, t);
            a.bind(skip);
        }
        Cc::Ne => {
            a.ucomisd_rr(x, y);
            a.jcc_label(Cond::P, t);
            a.jcc_label(Cond::Ne, t);
        }
        Cc::Ult | Cc::Uge => unreachable!("unsigned condition on a float compare"),
    }
}

fn cmp_mem_imm(a: &mut Assembler, w: W, m: Mem, imm: i32) {
    match w {
        W::B1 => a.cmp_m8_i(m, imm as u8),
        W::B4 => a.alu_mi(Alu::Cmp, Size::S32, m, imm),
        W::B8 => a.alu_mi(Alu::Cmp, Size::S64, m, imm),
    }
}

fn fop(a: &mut Assembler, op: SseOp, d: u8, s: u8) {
    a.movq_xr(XMM0, vr(d));
    a.movq_xr(XMM1, vr(s));
    a.sse_rr(op, XMM0, XMM1);
    a.movq_rx(vr(d), XMM0);
}

fn alu(a: &mut Assembler, op: Alu, d: u8, s: Src) {
    match s {
        Src::V(v) => a.alu_rr(op, Size::S64, vr(d), vr(v)),
        Src::Imm(i) => a.alu_ri(op, Size::S64, vr(d), i),
    }
}

/// Pops what the thunk pushed and returns (`eax` holds the exit code).
fn epilogue(a: &mut Assembler) {
    a.mov_rr(Size::S64, RDX, R13); // second return value: the frame compiled code leaves in
    a.alu_ri(Alu::Add, Size::S64, RSP, THUNK_PAD);
    for r in [R15, R14, R13, R12, RBX, RBP] {
        a.pop(r);
    }
    a.ret();
}

/// What the thunk returns in `rax:rdx`: the exit code and the `CallInfo` that was current (`r13`) when compiled
/// code left, so the interpreter can tell "same frame" from "flat call / return changed it" even when the
/// thread's `ci` was reset behind the frame's back (closing a coroutine from its own `__close`).
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ThunkRet {
    /// The exit code.
    pub code: u64,
    /// The running `CallInfo` at exit.
    pub ci: usize,
}

/// [`Thunk`] as `rt` calls it (the same machine code; tests that only read `eax` use [`Thunk`]).
pub type ThunkEx = unsafe extern "C" fn(*mut u8, *mut u8, *mut u8, *mut i32, *const u8) -> ThunkRet;

/// Signature of the thunk: `(thread, ci, frame_base, cnt, target) -> exit code` (in `eax`).
pub type Thunk = unsafe extern "C" fn(*mut u8, *mut u8, *mut u8, *mut i32, *const u8) -> u32;

/// The `jit_enter` thunk: saves the callee-saved registers, loads the pinned registers
/// (`r12 = thread`, `r13 = ci`, `rbx = frame base`, `r15 = budget word`, `r14 = ci.kcache`) and jumps to
/// the target address; compiled code returns through `epilogue`.
pub fn thunk_code() -> Vec<u8> {
    let mut a = Assembler::new();
    // standard frame-pointer prologue first, so `rbp` chains through the thunk for unwinders
    a.push(RBP);
    a.mov_rr(Size::S64, RBP, RSP);
    for r in [RBX, R12, R13, R14, R15] {
        a.push(r);
    }
    a.alu_ri(Alu::Sub, Size::S64, RSP, THUNK_PAD);
    a.mov_rr(Size::S64, R12, RDI);
    a.mov_rr(Size::S64, R13, RSI);
    a.mov_rr(Size::S64, RBX, RDX);
    a.mov_rr(Size::S64, R15, RCX);
    a.mov_rm(Size::S64, R14, Mem::base_disp(R13, callinfo::KCACHE as i32));
    a.jmp_r(R8);
    a.finish().expect("thunk assembles")
}
