//! Encoder tests: every instruction form is encoded over a systematic operand sweep and decoded with the
//! independent `iced-x86` decoder. Expectations are built from the typed operands (never from text).

use super::asm::rel32_value;
use super::*;
use iced_x86::{Decoder, DecoderOptions, Instruction, Mnemonic as M, OpKind, Register as IR};
use std::cell::Cell;

// ------------------------------------------------------------------------------------------------
// Expectation model

#[derive(Clone, Copy, Debug)]
enum Op {
    /// Register operand (general purpose of any width, or xmm).
    R(IR),
    /// Memory operand with the expected access width in bytes (0 = don't check, e.g. `lea`).
    Mem(super::Mem, u32),
    /// Immediate: value, operand size in bits (comparison mask), encoded size in bytes.
    I(i64, u32, u32),
    /// Branch target (absolute offset in the code buffer).
    T(u64),
}

thread_local! {
    static CASES: Cell<usize> = const { Cell::new(0) };
}

fn mask(bits: u32) -> u64 {
    if bits >= 64 {
        u64::MAX
    } else {
        (1u64 << bits) - 1
    }
}

fn disp_size(m: &super::Mem) -> u32 {
    if m.disp() == 0 && m.base_reg().unwrap().index() & 7 != 5 {
        0
    } else if i8::try_from(m.disp()).is_ok() {
        1
    } else {
        4
    }
}

fn check_ins(d: &Decoder, ins: &Instruction, bytes: &[u8], mn: M, ops: &[Op]) {
    let ctx = || format!("bytes {bytes:02X?}, expected {mn:?} {ops:?}, decoded {ins:?}");
    assert!(!ins.is_invalid(), "invalid: {}", ctx());
    assert_eq!(ins.len(), bytes.len(), "length: {}", ctx());
    assert_eq!(ins.mnemonic(), mn, "mnemonic: {}", ctx());
    assert_eq!(ins.op_count() as usize, ops.len(), "op count: {}", ctx());
    let co = d.get_constant_offsets(ins);
    let (mut exp_disp, mut exp_imm) = (0, 0);
    for (i, op) in ops.iter().enumerate() {
        let i32_ = i as u32;
        match *op {
            Op::R(r) => {
                assert_eq!(ins.op_kind(i32_), OpKind::Register, "kind: {}", ctx());
                assert_eq!(ins.op_register(i32_), r, "register: {}", ctx());
            }
            Op::Mem(m, width) => {
                assert_eq!(ins.op_kind(i32_), OpKind::Memory, "kind: {}", ctx());
                exp_disp = disp_size(&m);
                assert!(!ins.is_ip_rel_memory_operand(), "not rip: {}", ctx());
                assert_eq!(
                    ins.memory_base(),
                    r64(m.base_reg().unwrap()),
                    "base: {}",
                    ctx()
                );
                let (idx, scale) = match m.index() {
                    Some((i, s)) => (r64(i), u32::from(s)),
                    None => (IR::None, 1),
                };
                assert_eq!(ins.memory_index(), idx, "index: {}", ctx());
                assert_eq!(ins.memory_index_scale(), scale, "scale: {}", ctx());
                assert_eq!(
                    ins.memory_displacement64() as i64,
                    i64::from(m.disp()),
                    "disp: {}",
                    ctx()
                );
                if width != 0 {
                    assert_eq!(ins.memory_size().size() as u32, width, "width: {}", ctx());
                }
            }
            Op::I(v, bits, enc) => {
                exp_imm = enc;
                assert_eq!(
                    ins.immediate(i32_) & mask(bits),
                    (v as u64) & mask(bits),
                    "imm: {}",
                    ctx()
                );
            }
            Op::T(t) => {
                exp_imm = 4; // iced reports the rel32 field as the immediate
                assert_eq!(ins.op_kind(i32_), OpKind::NearBranch64, "kind: {}", ctx());
                assert_eq!(ins.near_branch_target(), t, "target: {}", ctx());
            }
        }
    }
    assert_eq!(
        co.displacement_size() as u32,
        exp_disp,
        "disp size: {}",
        ctx()
    );
    assert_eq!(co.immediate_size() as u32, exp_imm, "imm size: {}", ctx());
    reencode_identical(ins, bytes);
    CASES.with(|c| c.set(c.get() + 1));
}

/// Exact-bytes oracle: iced's *encoder* re-encodes the decoded instruction (same `Code`, i.e. same opcode
/// form) and must produce byte-for-byte our sequence. This catches what the decode checks cannot: redundant
/// prefixes, wrong-but-ignored REX bits, non-minimal displacement or immediate forms that decode identically.
/// Because the decoded `Code` pins the opcode direction (`89` vs `8B`, `01` vs `03`) iced does not substitute
/// an equally-valid alternative encoding, so no form needs an exemption: any difference is a failure.
/// (If one ever appears, list it here and compare length plus decoded semantics for that form only.)
fn reencode_identical(ins: &Instruction, bytes: &[u8]) {
    let mut e = iced_x86::Encoder::new(64);
    let n = e
        .encode(ins, ins.ip())
        .unwrap_or_else(|err| panic!("iced cannot encode {ins:?} ({bytes:02X?}): {err}"));
    assert_eq!(n, bytes.len(), "iced length for {bytes:02X?}");
    assert_eq!(e.take_buffer(), bytes, "iced bytes differ for {ins:?}");
}

fn check(bytes: &[u8], mn: M, ops: &[Op]) {
    let mut d = Decoder::with_ip(64, bytes, 0, DecoderOptions::NONE);
    let ins = d.decode();
    check_ins(&d, &ins, bytes, mn, ops);
}

fn enc(f: impl FnOnce(&mut Assembler)) -> Vec<u8> {
    let mut a = Assembler::new();
    f(&mut a);
    a.finish().unwrap()
}

fn report(name: &str) {
    let n = CASES.with(Cell::get);
    println!("CASES {name}: {n}");
    assert!(n > 0);
}

// ------------------------------------------------------------------------------------------------
// Register tables

const TAB64: [IR; 16] = [
    IR::RAX,
    IR::RCX,
    IR::RDX,
    IR::RBX,
    IR::RSP,
    IR::RBP,
    IR::RSI,
    IR::RDI,
    IR::R8,
    IR::R9,
    IR::R10,
    IR::R11,
    IR::R12,
    IR::R13,
    IR::R14,
    IR::R15,
];
const TAB32: [IR; 16] = [
    IR::EAX,
    IR::ECX,
    IR::EDX,
    IR::EBX,
    IR::ESP,
    IR::EBP,
    IR::ESI,
    IR::EDI,
    IR::R8D,
    IR::R9D,
    IR::R10D,
    IR::R11D,
    IR::R12D,
    IR::R13D,
    IR::R14D,
    IR::R15D,
];
const TAB8: [IR; 16] = [
    IR::AL,
    IR::CL,
    IR::DL,
    IR::BL,
    IR::SPL,
    IR::BPL,
    IR::SIL,
    IR::DIL,
    IR::R8L,
    IR::R9L,
    IR::R10L,
    IR::R11L,
    IR::R12L,
    IR::R13L,
    IR::R14L,
    IR::R15L,
];
const XR: [IR; 16] = [
    IR::XMM0,
    IR::XMM1,
    IR::XMM2,
    IR::XMM3,
    IR::XMM4,
    IR::XMM5,
    IR::XMM6,
    IR::XMM7,
    IR::XMM8,
    IR::XMM9,
    IR::XMM10,
    IR::XMM11,
    IR::XMM12,
    IR::XMM13,
    IR::XMM14,
    IR::XMM15,
];

fn r64(g: Gpr) -> IR {
    TAB64[g.index() as usize]
}
fn r32(g: Gpr) -> IR {
    TAB32[g.index() as usize]
}
fn r8(g: Gpr) -> IR {
    TAB8[g.index() as usize]
}
fn xr(x: Xmm) -> IR {
    XR[x.index() as usize]
}
fn gp(s: Size, g: Gpr) -> IR {
    if s == Size::S64 {
        r64(g)
    } else {
        r32(g)
    }
}
fn bits(s: Size) -> u32 {
    if s == Size::S64 {
        64
    } else {
        32
    }
}
fn bytes_of(s: Size) -> u32 {
    bits(s) / 8
}

const SIZES: [Size; 2] = [Size::S32, Size::S64];

fn imm_enc(v: i32) -> u32 {
    if i8::try_from(v).is_ok() {
        1
    } else {
        4
    }
}

// ------------------------------------------------------------------------------------------------
// Memory operand sweeps

const DISPS: [i32; 11] = [
    0,
    1,
    -1,
    127,
    -127,
    128,
    -128,
    129,
    -129,
    0x1234_5678,
    -0x1234_5678,
];

fn full_mems() -> Vec<super::Mem> {
    let mut v = Vec::new();
    for base in Gpr::ALL {
        for d in DISPS {
            v.push(super::Mem::base_disp(base, d));
            for idx in Gpr::ALL {
                if idx == RSP {
                    continue;
                }
                for s in [1u8, 2, 4, 8] {
                    v.push(super::Mem::sib(base, idx, s, d));
                }
            }
        }
    }
    v
}

fn rep_mems() -> Vec<super::Mem> {
    use super::Mem as Me;
    vec![
        Me::base(RAX),
        Me::base(RSP),
        Me::base(RBP),
        Me::base(R12),
        Me::base(R13),
        Me::base(R15),
        Me::base_disp(RBX, -129),
        Me::sib(R8, R9, 8, 0x1234),
        Me::sib(RSP, R12, 2, 0),
        Me::sib(RBP, RSI, 4, 0),
        Me::sib(R13, RAX, 1, 127),
        Me::sib(R12, R13, 8, -128),
    ]
}

/// Full memory sweep with a rotating register, plus all registers over the representative operands.
fn mem_reg(mut f: impl FnMut(super::Mem, Gpr)) {
    for (i, m) in full_mems().into_iter().enumerate() {
        f(m, Gpr::ALL[i % 16]);
    }
    for m in rep_mems() {
        for g in Gpr::ALL {
            f(m, g);
        }
    }
}

fn mem_xmm(mut f: impl FnMut(super::Mem, Xmm)) {
    for (i, m) in full_mems().into_iter().enumerate() {
        f(m, Xmm::ALL[i % 16]);
    }
    for m in rep_mems() {
        for x in Xmm::ALL {
            f(m, x);
        }
    }
}

// ------------------------------------------------------------------------------------------------
// Mnemonic tables

fn alu_mn(op: Alu) -> M {
    match op {
        Alu::Add => M::Add,
        Alu::Or => M::Or,
        Alu::Adc => M::Adc,
        Alu::Sbb => M::Sbb,
        Alu::And => M::And,
        Alu::Sub => M::Sub,
        Alu::Xor => M::Xor,
        Alu::Cmp => M::Cmp,
    }
}

const JCC: [M; 16] = [
    M::Jo,
    M::Jno,
    M::Jb,
    M::Jae,
    M::Je,
    M::Jne,
    M::Jbe,
    M::Ja,
    M::Js,
    M::Jns,
    M::Jp,
    M::Jnp,
    M::Jl,
    M::Jge,
    M::Jle,
    M::Jg,
];
const SETCC: [M; 16] = [
    M::Seto,
    M::Setno,
    M::Setb,
    M::Setae,
    M::Sete,
    M::Setne,
    M::Setbe,
    M::Seta,
    M::Sets,
    M::Setns,
    M::Setp,
    M::Setnp,
    M::Setl,
    M::Setge,
    M::Setle,
    M::Setg,
];
const CMOVCC: [M; 16] = [
    M::Cmovo,
    M::Cmovno,
    M::Cmovb,
    M::Cmovae,
    M::Cmove,
    M::Cmovne,
    M::Cmovbe,
    M::Cmova,
    M::Cmovs,
    M::Cmovns,
    M::Cmovp,
    M::Cmovnp,
    M::Cmovl,
    M::Cmovge,
    M::Cmovle,
    M::Cmovg,
];

// ------------------------------------------------------------------------------------------------
// Integer forms

const MOV_IMMS: [i64; 12] = [
    0,
    1,
    0x7fff_ffff,
    0x8000_0000,
    0xffff_ffff,
    -1,
    -0x8000_0000,
    -0x8000_0001,
    0x1_0000_0000,
    0x1122_3344_5566_7788,
    i64::MIN,
    i64::MAX,
];
const IMM32S: [i32; 10] = [
    0,
    1,
    -1,
    127,
    -128,
    128,
    -129,
    0x7fff_ffff,
    i32::MIN,
    0x1234_5678,
];

#[test]
fn mov_forms() {
    for s in SIZES {
        for d in Gpr::ALL {
            for sr in Gpr::ALL {
                check(
                    &enc(|a| a.mov_rr(s, d, sr)),
                    M::Mov,
                    &[Op::R(gp(s, d)), Op::R(gp(s, sr))],
                );
            }
        }
    }
    for d in Gpr::ALL {
        for v in MOV_IMMS {
            let b = enc(|a| a.mov_ri(d, v));
            if (0..=i64::from(u32::MAX)).contains(&v) {
                check(&b, M::Mov, &[Op::R(r32(d)), Op::I(v, 32, 4)]);
                assert_eq!(b.len(), 5 + usize::from(d.index() >= 8));
            } else if i32::try_from(v).is_ok() {
                check(&b, M::Mov, &[Op::R(r64(d)), Op::I(v, 64, 4)]);
            } else {
                check(&b, M::Mov, &[Op::R(r64(d)), Op::I(v, 64, 8)]);
                assert_eq!(b.len(), 10);
            }
        }
    }
    for s in SIZES {
        mem_reg(|m, g| {
            check(
                &enc(|a| a.mov_rm(s, g, m)),
                M::Mov,
                &[Op::R(gp(s, g)), Op::Mem(m, bytes_of(s))],
            );
            check(
                &enc(|a| a.mov_mr(s, m, g)),
                M::Mov,
                &[Op::Mem(m, bytes_of(s)), Op::R(gp(s, g))],
            );
        });
        for (i, m) in full_mems().into_iter().chain(rep_mems()).enumerate() {
            let v = IMM32S[i % IMM32S.len()];
            check(
                &enc(|a| a.mov_mi(s, m, v)),
                M::Mov,
                &[Op::Mem(m, bytes_of(s)), Op::I(i64::from(v), bits(s), 4)],
            );
        }
    }
    report("mov");
}

#[test]
fn mov_byte_forms() {
    mem_reg(|m, g| {
        check(
            &enc(|a| a.mov_m8_r(m, g)),
            M::Mov,
            &[Op::Mem(m, 1), Op::R(r8(g))],
        );
        check(
            &enc(|a| a.movzx_rm8(g, m)),
            M::Movzx,
            &[Op::R(r32(g)), Op::Mem(m, 1)],
        );
    });
    for (i, m) in full_mems().into_iter().chain(rep_mems()).enumerate() {
        let v = [0i8, 1, -1, 127, -128, 0x55][i % 6];
        check(
            &enc(|a| a.mov_m8_i(m, v)),
            M::Mov,
            &[Op::Mem(m, 1), Op::I(i64::from(v), 8, 1)],
        );
    }
    for (i, m) in full_mems().into_iter().chain(rep_mems()).enumerate() {
        let v = [0u8, 1, 0x7f, 0x80, 0xff, 0x55][i % 6];
        check(
            &enc(|a| a.cmp_m8_i(m, v)),
            M::Cmp,
            &[Op::Mem(m, 1), Op::I(i64::from(v as i8), 8, 1)],
        );
        check(
            &enc(|a| a.test_m8_i(m, v)),
            M::Test,
            &[Op::Mem(m, 1), Op::I(i64::from(v as i8), 8, 1)],
        );
    }
    for d in Gpr::ALL {
        for s in Gpr::ALL {
            check(
                &enc(|a| a.movzx_rr8(d, s)),
                M::Movzx,
                &[Op::R(r32(d)), Op::R(r8(s))],
            );
        }
        check(&enc(|a| a.setcc(Cond::E, d)), M::Sete, &[Op::R(r8(d))]);
    }
    report("mov_byte");
}

#[test]
fn lea_forms() {
    for s in SIZES {
        mem_reg(|m, g| {
            check(
                &enc(|a| a.lea(s, g, m)),
                M::Lea,
                &[Op::R(gp(s, g)), Op::Mem(m, 0)],
            );
        });
    }
    report("lea");
}

#[test]
fn alu_forms() {
    for op in Alu::ALL {
        let mn = alu_mn(op);
        for s in SIZES {
            for d in Gpr::ALL {
                for sr in Gpr::ALL {
                    check(
                        &enc(|a| a.alu_rr(op, s, d, sr)),
                        mn,
                        &[Op::R(gp(s, d)), Op::R(gp(s, sr))],
                    );
                }
                for v in IMM32S {
                    check(
                        &enc(|a| a.alu_ri(op, s, d, v)),
                        mn,
                        &[Op::R(gp(s, d)), Op::I(i64::from(v), bits(s), imm_enc(v))],
                    );
                }
            }
            let mems: Vec<_> = if s == Size::S64 {
                full_mems()
            } else {
                Vec::new()
            };
            let sweep = |m: super::Mem, g: Gpr| {
                let w = bytes_of(s);
                check(
                    &enc(|a| a.alu_rm(op, s, g, m)),
                    mn,
                    &[Op::R(gp(s, g)), Op::Mem(m, w)],
                );
                check(
                    &enc(|a| a.alu_mr(op, s, m, g)),
                    mn,
                    &[Op::Mem(m, w), Op::R(gp(s, g))],
                );
                let v = IMM32S[(g.index() as usize).wrapping_add(m.disp() as usize) % IMM32S.len()];
                check(
                    &enc(|a| a.alu_mi(op, s, m, v)),
                    mn,
                    &[Op::Mem(m, w), Op::I(i64::from(v), bits(s), imm_enc(v))],
                );
            };
            for (i, m) in mems.into_iter().enumerate() {
                sweep(m, Gpr::ALL[i % 16]);
            }
            for m in rep_mems() {
                for g in Gpr::ALL {
                    sweep(m, g);
                }
            }
        }
    }
    report("alu");
}

#[test]
fn test_forms() {
    for s in SIZES {
        for a_ in Gpr::ALL {
            for b_ in Gpr::ALL {
                check(
                    &enc(|a| a.test_rr(s, a_, b_)),
                    M::Test,
                    &[Op::R(gp(s, a_)), Op::R(gp(s, b_))],
                );
            }
            for v in IMM32S {
                check(
                    &enc(|a| a.test_ri(s, a_, v)),
                    M::Test,
                    &[Op::R(gp(s, a_)), Op::I(i64::from(v), bits(s), 4)],
                );
            }
        }
        mem_reg(|m, g| {
            check(
                &enc(|a| a.test_mr(s, m, g)),
                M::Test,
                &[Op::Mem(m, bytes_of(s)), Op::R(gp(s, g))],
            );
            let v = IMM32S[g.index() as usize % IMM32S.len()];
            check(
                &enc(|a| a.test_mi(s, m, v)),
                M::Test,
                &[Op::Mem(m, bytes_of(s)), Op::I(i64::from(v), bits(s), 4)],
            );
        });
    }
    report("test");
}

#[test]
fn imul_forms() {
    for s in SIZES {
        for d in Gpr::ALL {
            for sr in Gpr::ALL {
                check(
                    &enc(|a| a.imul_rr(s, d, sr)),
                    M::Imul,
                    &[Op::R(gp(s, d)), Op::R(gp(s, sr))],
                );
                for v in [3, -1, 127, -128, 128, 0x1234_5678] {
                    check(
                        &enc(|a| a.imul_rri(s, d, sr, v)),
                        M::Imul,
                        &[
                            Op::R(gp(s, d)),
                            Op::R(gp(s, sr)),
                            Op::I(i64::from(v), bits(s), imm_enc(v)),
                        ],
                    );
                }
            }
        }
        mem_reg(|m, g| {
            check(
                &enc(|a| a.imul_rm(s, g, m)),
                M::Imul,
                &[Op::R(gp(s, g)), Op::Mem(m, bytes_of(s))],
            );
        });
    }
    report("imul");
}

#[test]
fn unary_shift_div_forms() {
    for s in SIZES {
        for r in Gpr::ALL {
            let one = [Op::R(gp(s, r))];
            check(&enc(|a| a.neg(s, r)), M::Neg, &one);
            check(&enc(|a| a.not(s, r)), M::Not, &one);
            check(&enc(|a| a.inc(s, r)), M::Inc, &one);
            check(&enc(|a| a.dec(s, r)), M::Dec, &one);
            check(&enc(|a| a.idiv(s, r)), M::Idiv, &one);
            check(&enc(|a| a.div(s, r)), M::Div, &one);
            for (sh, mn) in [
                (Shift::Shl, M::Shl),
                (Shift::Shr, M::Shr),
                (Shift::Sar, M::Sar),
            ] {
                for n in [0u8, 1, 3, 31, 63, 255] {
                    check(
                        &enc(|a| a.shift_ri(sh, s, r, n)),
                        mn,
                        &[Op::R(gp(s, r)), Op::I(i64::from(n), 8, 1)],
                    );
                }
                check(
                    &enc(|a| a.shift_cl(sh, s, r)),
                    mn,
                    &[Op::R(gp(s, r)), Op::R(IR::CL)],
                );
            }
            for b_ in Gpr::ALL {
                check(
                    &enc(|a| a.bt_rr(s, r, b_)),
                    M::Bt,
                    &[Op::R(gp(s, r)), Op::R(gp(s, b_))],
                );
            }
            for n in [0u8, 5, 31] {
                check(
                    &enc(|a| a.bt_ri(s, r, n)),
                    M::Bt,
                    &[Op::R(gp(s, r)), Op::I(i64::from(n), 8, 1)],
                );
            }
        }
    }
    check(&enc(Assembler::cqo), M::Cqo, &[]);
    check(&enc(Assembler::cdq), M::Cdq, &[]);
    check(&enc(Assembler::ret), M::Ret, &[]);
    check(&enc(Assembler::int3), M::Int3, &[]);
    report("unary_shift_div");
}

#[test]
fn stack_call_jmp_setcc_cmov_forms() {
    for r in Gpr::ALL {
        check(&enc(|a| a.push(r)), M::Push, &[Op::R(r64(r))]);
        check(&enc(|a| a.pop(r)), M::Pop, &[Op::R(r64(r))]);
        check(&enc(|a| a.call_r(r)), M::Call, &[Op::R(r64(r))]);
        check(&enc(|a| a.jmp_r(r)), M::Jmp, &[Op::R(r64(r))]);
        for cc in Cond::ALL {
            check(
                &enc(|a| a.setcc(cc, r)),
                SETCC[cc as usize],
                &[Op::R(r8(r))],
            );
            for s in SIZES {
                for sr in Gpr::ALL {
                    check(
                        &enc(|a| a.cmovcc(cc, s, r, sr)),
                        CMOVCC[cc as usize],
                        &[Op::R(gp(s, r)), Op::R(gp(s, sr))],
                    );
                }
            }
        }
    }
    mem_reg(|m, _| {
        check(&enc(|a| a.call_m(m)), M::Call, &[Op::Mem(m, 8)]);
        check(&enc(|a| a.jmp_m(m)), M::Jmp, &[Op::Mem(m, 8)]);
    });
    report("stack_call_jmp");
}

// ------------------------------------------------------------------------------------------------
// SSE2

#[test]
fn sse_forms() {
    let ops = [
        SseOp::Sqrt,
        SseOp::Add,
        SseOp::Mul,
        SseOp::Sub,
        SseOp::Min,
        SseOp::Div,
        SseOp::Max,
    ];
    let mns = [
        M::Sqrtsd,
        M::Addsd,
        M::Mulsd,
        M::Subsd,
        M::Minsd,
        M::Divsd,
        M::Maxsd,
    ];
    for d in Xmm::ALL {
        for s in Xmm::ALL {
            check(
                &enc(|a| a.movsd_rr(d, s)),
                M::Movsd,
                &[Op::R(xr(d)), Op::R(xr(s))],
            );
            check(
                &enc(|a| a.movaps_rr(d, s)),
                M::Movaps,
                &[Op::R(xr(d)), Op::R(xr(s))],
            );
            check(
                &enc(|a| a.xorps_rr(d, s)),
                M::Xorps,
                &[Op::R(xr(d)), Op::R(xr(s))],
            );
            check(
                &enc(|a| a.ucomisd_rr(d, s)),
                M::Ucomisd,
                &[Op::R(xr(d)), Op::R(xr(s))],
            );
            for (op, mn) in ops.iter().zip(mns) {
                check(
                    &enc(|a| a.sse_rr(*op, d, s)),
                    mn,
                    &[Op::R(xr(d)), Op::R(xr(s))],
                );
            }
            for (op, mn) in [
                (SseLogic::And, M::Andpd),
                (SseLogic::Andn, M::Andnpd),
                (SseLogic::Xor, M::Xorpd),
            ] {
                check(
                    &enc(|a| a.logic_rr(op, d, s)),
                    mn,
                    &[Op::R(xr(d)), Op::R(xr(s))],
                );
            }
            for mode in [0u8, 1, 9, 0xB] {
                check(
                    &enc(|a| a.roundsd_rr(d, s, mode)),
                    M::Roundsd,
                    &[Op::R(xr(d)), Op::R(xr(s)), Op::I(i64::from(mode), 8, 1)],
                );
            }
        }
        for g in Gpr::ALL {
            check(
                &enc(|a| a.movq_xr(d, g)),
                M::Movq,
                &[Op::R(xr(d)), Op::R(r64(g))],
            );
            check(
                &enc(|a| a.movq_rx(g, d)),
                M::Movq,
                &[Op::R(r64(g)), Op::R(xr(d))],
            );
            for s in SIZES {
                check(
                    &enc(|a| a.cvtsi2sd_rr(s, d, g)),
                    M::Cvtsi2sd,
                    &[Op::R(xr(d)), Op::R(gp(s, g))],
                );
                check(
                    &enc(|a| a.cvttsd2si(s, g, d)),
                    M::Cvttsd2si,
                    &[Op::R(gp(s, g)), Op::R(xr(d))],
                );
            }
        }
    }
    mem_xmm(|m, x| {
        check(
            &enc(|a| a.movsd_rm(x, m)),
            M::Movsd,
            &[Op::R(xr(x)), Op::Mem(m, 8)],
        );
        check(
            &enc(|a| a.movsd_mr(m, x)),
            M::Movsd,
            &[Op::Mem(m, 8), Op::R(xr(x))],
        );
        check(
            &enc(|a| a.ucomisd_rm(x, m)),
            M::Ucomisd,
            &[Op::R(xr(x)), Op::Mem(m, 8)],
        );
        for (op, mn) in ops.iter().zip(mns) {
            check(
                &enc(|a| a.sse_rm(*op, x, m)),
                mn,
                &[Op::R(xr(x)), Op::Mem(m, 8)],
            );
        }
        for (op, mn) in [
            (SseLogic::And, M::Andpd),
            (SseLogic::Andn, M::Andnpd),
            (SseLogic::Xor, M::Xorpd),
        ] {
            check(
                &enc(|a| a.logic_rm(op, x, m)),
                mn,
                &[Op::R(xr(x)), Op::Mem(m, 16)],
            );
        }
        for s in SIZES {
            check(
                &enc(|a| a.cvtsi2sd_rm(s, x, m)),
                M::Cvtsi2sd,
                &[Op::R(xr(x)), Op::Mem(m, bytes_of(s))],
            );
        }
        check(
            &enc(|a| a.roundsd_rm(x, m, 0xA)),
            M::Roundsd,
            &[Op::R(xr(x)), Op::Mem(m, 8), Op::I(0xA, 8, 1)],
        );
    });
    report("sse");
}

// ------------------------------------------------------------------------------------------------
// Fixed known-good byte sequences (Intel SDM / what GNU as emits)

#[test]
fn known_good_sequences() {
    let cases: Vec<(Vec<u8>, Vec<u8>)> = vec![
        (enc(|a| a.push(RBP)), vec![0x55]),
        (enc(|a| a.push(R12)), vec![0x41, 0x54]),
        (enc(|a| a.pop(R15)), vec![0x41, 0x5F]),
        (
            enc(|a| a.mov_rr(Size::S64, RBP, RSP)),
            vec![0x48, 0x89, 0xE5],
        ),
        (enc(|a| a.ret()), vec![0xC3]),
        (enc(|a| a.int3()), vec![0xCC]),
        (
            enc(|a| a.mov_rm(Size::S64, RAX, Mem::base_disp(RBX, 8))),
            vec![0x48, 0x8B, 0x43, 0x08],
        ),
        (
            enc(|a| a.alu_ri(Alu::Add, Size::S64, RAX, 1)),
            vec![0x48, 0x83, 0xC0, 0x01],
        ),
        (
            enc(|a| a.alu_ri(Alu::Sub, Size::S64, RSP, 0x28)),
            vec![0x48, 0x83, 0xEC, 0x28],
        ),
        (
            enc(|a| a.alu_ri(Alu::Sub, Size::S64, RSP, 0x128)),
            vec![0x48, 0x81, 0xEC, 0x28, 0x01, 0, 0],
        ),
        (
            enc(|a| a.alu_rr(Alu::Xor, Size::S32, RAX, RAX)),
            vec![0x31, 0xC0],
        ),
        (
            enc(|a| a.lea(Size::S64, RAX, Mem::sib(RDI, RDI, 2, 0))),
            vec![0x48, 0x8D, 0x04, 0x7F],
        ),
        (
            enc(|a| a.mov_mr(Size::S64, Mem::base_disp(RSP, 8), RDI)),
            vec![0x48, 0x89, 0x7C, 0x24, 0x08],
        ),
        (
            enc(|a| a.mov_mr(Size::S64, Mem::base(RBP), RAX)),
            vec![0x48, 0x89, 0x45, 0x00],
        ),
        (
            enc(|a| a.mov_rm(Size::S64, RAX, Mem::base(R13))),
            vec![0x49, 0x8B, 0x45, 0x00],
        ),
        (
            enc(|a| a.mov_rm(Size::S64, RAX, Mem::base(R12))),
            vec![0x49, 0x8B, 0x04, 0x24],
        ),
        (enc(|a| a.mov_ri(R8, 1)), vec![0x41, 0xB8, 1, 0, 0, 0]),
        (
            enc(|a| a.mov_ri(RAX, -1)),
            vec![0x48, 0xC7, 0xC0, 0xFF, 0xFF, 0xFF, 0xFF],
        ),
        (
            enc(|a| a.mov_ri(RAX, 0x1122_3344_5566_7788)),
            vec![0x48, 0xB8, 0x88, 0x77, 0x66, 0x55, 0x44, 0x33, 0x22, 0x11],
        ),
        (
            enc(|a| a.mov_m8_r(Mem::base(RSP), RSI)),
            vec![0x40, 0x88, 0x34, 0x24],
        ),
        (enc(|a| a.movzx_rr8(RAX, RDI)), vec![0x40, 0x0F, 0xB6, 0xC7]),
        (enc(|a| a.setcc(Cond::L, RAX)), vec![0x0F, 0x9C, 0xC0]),
        (
            enc(|a| a.cmovcc(Cond::Ne, Size::S64, RAX, RCX)),
            vec![0x48, 0x0F, 0x45, 0xC1],
        ),
        (
            enc(|a| a.imul_rr(Size::S64, RAX, RBX)),
            vec![0x48, 0x0F, 0xAF, 0xC3],
        ),
        (
            enc(|a| a.shift_ri(Shift::Shl, Size::S64, RAX, 3)),
            vec![0x48, 0xC1, 0xE0, 0x03],
        ),
        (enc(|a| a.cqo()), vec![0x48, 0x99]),
        (enc(|a| a.idiv(Size::S64, RCX)), vec![0x48, 0xF7, 0xF9]),
        (enc(|a| a.call_r(RAX)), vec![0xFF, 0xD0]),
        (enc(|a| a.jmp_r(RAX)), vec![0xFF, 0xE0]),
        (
            enc(|a| a.movsd_rm(XMM0, Mem::base(RDI))),
            vec![0xF2, 0x0F, 0x10, 0x07],
        ),
        (
            enc(|a| a.sse_rr(SseOp::Add, XMM0, XMM1)),
            vec![0xF2, 0x0F, 0x58, 0xC1],
        ),
        (
            enc(|a| a.movq_xr(XMM0, RAX)),
            vec![0x66, 0x48, 0x0F, 0x6E, 0xC0],
        ),
        (
            enc(|a| a.cvtsi2sd_rr(Size::S64, XMM0, RAX)),
            vec![0xF2, 0x48, 0x0F, 0x2A, 0xC0],
        ),
        (
            enc(|a| a.ucomisd_rr(XMM0, XMM1)),
            vec![0x66, 0x0F, 0x2E, 0xC1],
        ),
        (
            enc(|a| a.cvttsd2si(Size::S64, RAX, XMM0)),
            vec![0xF2, 0x48, 0x0F, 0x2C, 0xC0],
        ),
        (
            enc(|a| a.logic_rr(SseLogic::Xor, XMM8, XMM9)),
            vec![0x66, 0x45, 0x0F, 0x57, 0xC1],
        ),
        (
            enc(|a| a.roundsd_rr(XMM0, XMM1, 9)),
            vec![0x66, 0x0F, 0x3A, 0x0B, 0xC1, 0x09],
        ),
    ];
    for (got, want) in &cases {
        assert_eq!(got, want);
    }
}

// ------------------------------------------------------------------------------------------------
// Labels, jumps, alignment

fn decode_all(code: &[u8]) -> Vec<Instruction> {
    let mut d = Decoder::with_ip(64, code, 0, DecoderOptions::NONE);
    let mut v = Vec::new();
    while d.can_decode() {
        let i = d.decode();
        assert!(!i.is_invalid(), "invalid instruction in {code:02X?}");
        v.push(i);
    }
    v
}

#[test]
fn forward_and_backward_branches() {
    for cc in Cond::ALL {
        // backward: L: int3 int3 jcc L ; forward: jcc F ; int3 x3 ; F:
        let mut a = Assembler::new();
        let back = a.new_label();
        let fwd = a.new_label();
        a.int3();
        a.bind(back);
        a.int3();
        a.jcc_label(cc, back);
        a.jmp_label(back);
        a.call_label(back);
        a.jcc_label(cc, fwd);
        a.jmp_label(fwd);
        a.call_label(fwd);
        a.int3();
        a.int3();
        a.bind(fwd);
        a.ret();
        let (back_off, fwd_off) = (
            a.label_offset(back).unwrap() as u64,
            a.label_offset(fwd).unwrap() as u64,
        );
        assert_eq!((back_off, fwd_off), (1, 1 + 1 + 6 + 5 + 5 + 6 + 5 + 5 + 2));
        let code = a.finish().unwrap();
        let ins = decode_all(&code);
        let d = Decoder::with_ip(64, &code, 0, DecoderOptions::NONE);
        let want: [(M, u64); 6] = [
            (JCC[cc as usize], back_off),
            (M::Jmp, back_off),
            (M::Call, back_off),
            (JCC[cc as usize], fwd_off),
            (M::Jmp, fwd_off),
            (M::Call, fwd_off),
        ];
        let mut pos = 2; // skip the two int3 instructions
        for (k, (mn, target)) in want.iter().enumerate() {
            let i = &ins[pos + k];
            let len = if k % 3 == 0 { 6 } else { 5 };
            let bytes = &code[i.ip() as usize..i.ip() as usize + len];
            check_ins(&d, i, bytes, *mn, &[Op::T(*target)]);
        }
        pos += want.len();
        assert_eq!(ins[pos].mnemonic(), M::Int3);
    }
    report("branches");
}

#[test]
fn rip_relative_with_trailing_immediates() {
    let mut a = Assembler::new();
    let l = a.new_label();
    a.lea(Size::S64, RAX, Mem::rip(l));
    a.mov_rm(Size::S64, R9, Mem::rip(l));
    a.alu_mi(Alu::Add, Size::S32, Mem::rip(l), 5); // imm8 trail
    a.alu_mi(Alu::Cmp, Size::S64, Mem::rip(l), 0x1234_5678); // imm32 trail
    a.mov_mi(Size::S32, Mem::rip(l), 7);
    a.mov_m8_i(Mem::rip(l), 9);
    a.roundsd_rm(XMM3, Mem::rip(l), 1);
    a.movsd_mr(Mem::rip(l), XMM10);
    a.call_m(Mem::rip(l));
    a.align(16);
    a.bind(l);
    a.emit_bytes(&[0; 8]);
    let target = a.label_offset(l).unwrap() as u64;
    let code = a.finish().unwrap();
    let ins: Vec<_> = decode_all(&code)
        .into_iter()
        .filter(|i| i.mnemonic() != M::Nop)
        .collect();
    // 9 instructions + trailing data bytes decoded as `add [rax], al`.
    let rip_ins: Vec<_> = ins
        .iter()
        .filter(|i| i.is_ip_rel_memory_operand())
        .collect();
    assert_eq!(rip_ins.len(), 9);
    for i in rip_ins {
        assert_eq!(i.ip_rel_memory_address(), target, "{i:?}");
    }
}

#[test]
fn label_errors() {
    let mut a = Assembler::new();
    let l = a.new_label();
    a.jmp_label(l);
    assert_eq!(a.finish(), Err(AsmError::UnboundLabel(l)));

    let mut a = Assembler::new();
    let l = a.new_label();
    a.lea(Size::S64, RAX, Mem::rip(l));
    assert_eq!(a.finish(), Err(AsmError::UnboundLabel(l)));

    let mut a = Assembler::new();
    let l = a.new_label();
    a.bind(l);
    a.bind(l);
    assert_eq!(a.finish(), Err(AsmError::LabelRebound(l)));

    // an unreferenced, never-bound label is fine; so is a bound label with no references
    let mut a = Assembler::new();
    let _unused = a.new_label();
    let b = a.new_label();
    a.bind(b);
    a.ret();
    assert_eq!(a.finish(), Ok(vec![0xC3]));
}

#[test]
fn align_emits_decodable_nops() {
    for start in 0..24usize {
        for n in [1usize, 2, 4, 8, 16, 32, 64] {
            let mut a = Assembler::new();
            for _ in 0..start {
                a.int3();
            }
            a.align(n);
            let len = a.offset();
            assert_eq!(len % n, 0);
            assert!(len - start < n.max(1));
            a.int3();
            let code = a.finish().unwrap();
            let ins = decode_all(&code[start..]);
            let (last, pad) = ins.split_last().unwrap();
            assert_eq!(last.mnemonic(), M::Int3);
            assert!(pad.iter().all(|i| i.mnemonic() == M::Nop), "{code:02X?}");
            assert_eq!(pad.iter().map(Instruction::len).sum::<usize>(), len - start);
            CASES.with(|c| c.set(c.get() + 1));
        }
    }
    report("align");
}

#[test]
#[should_panic(expected = "rsp cannot be an index")]
fn rsp_index_panics() {
    enc(|a| a.mov_rm(Size::S64, RAX, Mem::sib(RAX, RSP, 1, 0)));
}

#[test]
#[should_panic(expected = "invalid scale")]
fn bad_scale_panics() {
    enc(|a| a.mov_rm(Size::S64, RAX, Mem::sib(RAX, RCX, 3, 0)));
}

// ------------------------------------------------------------------------------------------------
// RIP-relative operands, labels, absolute helpers, rel32 math

/// RIP operands: mnemonic, immediate value and the resolved address (label + disp), not just addresses.
#[test]
fn rip_forms_mnemonic_operands_immediates() {
    let mut a = Assembler::new();
    let l = a.new_label();
    // (mnemonic, displacement added to the label, immediate (value, bits))
    type Want = (M, i32, Option<(i64, u32)>);
    let mut want: Vec<Want> = Vec::new();
    for (k, &d) in [0i32, 8, -4, 0x100].iter().enumerate() {
        let m = Mem::rip_disp(l, d);
        a.lea(Size::S64, RAX, m);
        want.push((M::Lea, d, None));
        a.mov_rm(Size::S32, R9, m);
        want.push((M::Mov, d, None));
        a.alu_mi(Alu::Add, Size::S32, m, 5);
        want.push((M::Add, d, Some((5, 32))));
        a.alu_mi(Alu::Cmp, Size::S64, m, 0x1234_5678);
        want.push((M::Cmp, d, Some((0x1234_5678, 64))));
        a.mov_mi(Size::S32, m, 7 + k as i32);
        want.push((M::Mov, d, Some((7 + k as i64, 32))));
        a.mov_m8_i(m, 9);
        want.push((M::Mov, d, Some((9, 8))));
        a.cmp_m8_i(m, 0xFE);
        want.push((M::Cmp, d, Some((-2, 8))));
        a.test_m8_i(m, 0x40);
        want.push((M::Test, d, Some((0x40, 8))));
        a.test_mi(Size::S32, m, 0x0100_0000);
        want.push((M::Test, d, Some((0x0100_0000, 32))));
        a.roundsd_rm(XMM3, m, 1);
        want.push((M::Roundsd, d, Some((1, 8))));
        a.movsd_mr(m, XMM10);
        want.push((M::Movsd, d, None));
        a.call_m(m);
        want.push((M::Call, d, None));
    }
    a.align(16);
    a.bind(l);
    a.emit_bytes(&[0; 0x200]);
    let base = a.label_offset(l).unwrap() as i64;
    let code = a.finish().unwrap();
    let ins: Vec<_> = decode_all(&code[..base as usize])
        .into_iter()
        .filter(|i| i.mnemonic() != M::Nop)
        .collect();
    assert_eq!(ins.len(), want.len());
    for (i, &(mn, disp, imm)) in ins.iter().zip(want.iter()) {
        assert_eq!(i.mnemonic(), mn, "{i:?}");
        assert!(i.is_ip_rel_memory_operand(), "{i:?}");
        assert_eq!(
            i.ip_rel_memory_address() as i64,
            base + i64::from(disp),
            "{i:?}"
        );
        if let Some((v, bits)) = imm {
            let k = (0..i.op_count())
                .rev()
                .find(|&k| !matches!(i.op_kind(k), OpKind::Memory | OpKind::Register))
                .expect("immediate operand");
            assert_eq!(
                i.immediate(k) & mask(bits),
                (v as u64) & mask(bits),
                "{i:?}"
            );
        }
        let bytes = &code[i.ip() as usize..i.ip() as usize + i.len()];
        reencode_identical(i, bytes);
    }
}

#[test]
fn rip_operand_accessors() {
    let mut a = Assembler::new();
    let l = a.new_label();
    let m = Mem::rip_disp(l, -8);
    assert_eq!(
        (m.base_reg(), m.index(), m.disp(), m.rip_label()),
        (None, None, -8, Some(l))
    );
    assert_eq!(Mem::rip(l).disp(), 0);
    assert_eq!(m.with_disp(12).disp(), 12);
    let n = Mem::new(RBX, 4);
    assert_eq!(n, Mem::base_disp(RBX, 4));
    assert_eq!(Mem::indexed(RAX, RCX, 4, 8), Mem::sib(RAX, RCX, 4, 8));
    assert_eq!(n.base_reg(), Some(RBX));
    assert_eq!(n.rip_label(), None);
    a.bind(l);
}

#[test]
fn foreign_labels_are_errors_not_panics() {
    // bind a foreign label whose index is in range for `a`
    let mut a = Assembler::new();
    let mut b = Assembler::new();
    let _pad = a.new_label();
    let lb = b.new_label();
    a.bind(lb);
    a.ret();
    assert_eq!(a.finish(), Err(AsmError::ForeignLabel));
    // bind a foreign label whose index is out of range for `a`
    let mut a = Assembler::new();
    let mut b = Assembler::new();
    for _ in 0..10 {
        b.new_label();
    }
    let far = b.new_label();
    a.bind(far);
    assert_eq!(a.label_offset(far), None);
    assert_eq!(a.finish(), Err(AsmError::ForeignLabel));
    // referencing a foreign label from jumps and rip operands
    let mut a = Assembler::new();
    let lb = b.new_label();
    a.jmp_label(lb);
    assert_eq!(a.finish(), Err(AsmError::ForeignLabel));
    let mut a = Assembler::new();
    a.lea(Size::S64, RAX, Mem::rip(lb));
    assert_eq!(a.finish(), Err(AsmError::ForeignLabel));
}

#[test]
fn rel32_overflow_math() {
    assert_eq!(rel32_value(100, 0, 10), Ok(90));
    assert_eq!(rel32_value(10, 0, 100), Ok(-90));
    assert_eq!(rel32_value(10, 6, 20), Ok(-4));
    assert_eq!(rel32_value(0, 0, i32::MAX as usize), Ok(-i32::MAX));
    assert_eq!(rel32_value(0, 0, 1usize << 31), Ok(i32::MIN));
    assert_eq!(
        rel32_value(0, 0, (1usize << 31) + 1),
        Err(AsmError::OutOfRange)
    );
    assert_eq!(
        rel32_value(i32::MAX as usize + 5, 0, 4),
        Err(AsmError::OutOfRange)
    );
    assert_eq!(rel32_value(i32::MAX as usize, 0, 0), Ok(i32::MAX));
    assert_eq!(
        rel32_value(i32::MAX as usize, 1, 0),
        Err(AsmError::OutOfRange)
    );
    assert_eq!(rel32_value(1 << 40, i32::MIN, 1 << 40), Ok(i32::MIN));
    assert_eq!(rel32_value(usize::MAX, 0, 0), Err(AsmError::OutOfRange));
    assert_eq!(rel32_value(0, 0, usize::MAX), Err(AsmError::OutOfRange));
}

#[test]
fn absolute_helpers_and_movabs() {
    let addr = 0x1122_3344_5566_7788u64;
    for r in Gpr::ALL {
        let b = enc(|a| a.mov_ri64(r, 5));
        assert_eq!(b.len(), 10);
        check(&b, M::Mov, &[Op::R(r64(r)), Op::I(5, 64, 8)]);
    }
    let b = enc(|a| a.call_abs(addr));
    assert_eq!(b.len(), 13);
    let ins = decode_all(&b);
    assert_eq!(
        (ins[0].mnemonic(), ins[0].op0_register()),
        (M::Mov, IR::R11)
    );
    assert_eq!(ins[0].immediate(1), addr);
    assert_eq!(
        (ins[1].mnemonic(), ins[1].op0_register()),
        (M::Call, IR::R11)
    );
    let b = enc(|a| a.jmp_abs(addr));
    let ins = decode_all(&b);
    assert_eq!(
        (ins[1].mnemonic(), ins[1].op0_register()),
        (M::Jmp, IR::R11)
    );
    assert_eq!(ins[0].immediate(1), addr);
}

/// Direct use of iced's `CodeAssembler` (independent of decoding) for a few representative forms.
#[test]
fn iced_code_asm_direct_oracle() {
    use iced_x86::code_asm::*;
    let mut c = CodeAssembler::new(64).unwrap();
    c.mov(rax, qword_ptr(rbx + rcx * 4 + 0x10)).unwrap();
    c.mov(qword_ptr(r13), r12).unwrap();
    c.add(dword_ptr(rsp + 8), 5).unwrap();
    c.cmp(byte_ptr(r12 + r13 * 8 - 128), 0x7f).unwrap();
    c.lea(r9, qword_ptr(rbp + rsi * 2)).unwrap();
    c.addsd(xmm9, qword_ptr(rdi + 0x1234)).unwrap();
    c.ret().unwrap();
    let want = c.assemble(0).unwrap();
    let got = enc(|a| {
        a.mov_rm(Size::S64, RAX, Mem::sib(RBX, RCX, 4, 0x10));
        a.mov_mr(Size::S64, Mem::base(R13), R12);
        a.alu_mi(Alu::Add, Size::S32, Mem::base_disp(RSP, 8), 5);
        a.cmp_m8_i(Mem::sib(R12, R13, 8, -128), 0x7f);
        a.lea(Size::S64, R9, Mem::sib(RBP, RSI, 2, 0));
        a.sse_rm(SseOp::Add, XMM9, Mem::base_disp(RDI, 0x1234));
        a.ret();
    });
    assert_eq!(got, want);
}
