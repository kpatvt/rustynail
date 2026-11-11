//! The [`Assembler`]: a safe x86-64 encoder writing into a `Vec<u8>`.

use super::regs::*;
use std::sync::atomic::{AtomicU32, Ordering};

/// A code position that can be referenced before it is bound (see [`Assembler::new_label`]).
///
/// A label remembers which assembler created it; using it with another assembler is reported as
/// [`AsmError::ForeignLabel`] by [`Assembler::finish`] (it never panics or indexes out of range).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Label {
    asm: u32,
    idx: u32,
}

static NEXT_ASM_ID: AtomicU32 = AtomicU32::new(1);

/// Errors reported by [`Assembler::finish`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AsmError {
    /// A label was referenced but never bound.
    UnboundLabel(Label),
    /// A label was bound more than once.
    LabelRebound(Label),
    /// A displacement does not fit in 32 bits.
    OutOfRange,
    /// A label created by a different [`Assembler`] was bound or referenced.
    ForeignLabel,
}

impl std::fmt::Display for AsmError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AsmError::UnboundLabel(l) => write!(f, "label {} referenced but never bound", l.idx),
            AsmError::LabelRebound(l) => write!(f, "label {} bound more than once", l.idx),
            AsmError::OutOfRange => write!(f, "branch displacement out of 32-bit range"),
            AsmError::ForeignLabel => write!(f, "label belongs to a different assembler"),
        }
    }
}

impl std::error::Error for AsmError {}

/// A memory operand `[base + index*scale + disp]`, or a RIP-relative label reference
/// `[rip + label + disp]`.
///
/// Every ModRM/SIB special case is handled by the encoder: `rsp`/`r12` bases get a SIB byte, `rbp`/`r13`
/// bases always get a displacement, and the shortest displacement size is chosen. `rsp` cannot be an index.
/// RIP-relative operands have no base or index (impossible to construct); their displacement is added to the
/// label's address (`rel32 = label + disp - end_of_instruction`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Mem {
    base: Gpr,
    index: Option<(Gpr, u8)>,
    disp: i32,
    rip: Option<Label>,
}

impl Mem {
    /// `[base]`
    pub const fn base(base: Gpr) -> Mem {
        Mem {
            base,
            index: None,
            disp: 0,
            rip: None,
        }
    }

    /// `[base + disp]`
    pub const fn base_disp(base: Gpr, disp: i32) -> Mem {
        Mem {
            base,
            index: None,
            disp,
            rip: None,
        }
    }

    /// `[base + disp]` (alias of [`Mem::base_disp`]).
    pub const fn new(base: Gpr, disp: i32) -> Mem {
        Mem::base_disp(base, disp)
    }

    /// `[base + index*scale + disp]`. `scale` must be 1, 2, 4 or 8 and `index` must not be `rsp`
    /// (checked when the operand is encoded).
    pub const fn sib(base: Gpr, index: Gpr, scale: u8, disp: i32) -> Mem {
        Mem {
            base,
            index: Some((index, scale)),
            disp,
            rip: None,
        }
    }

    /// `[base + index*scale + disp]` (alias of [`Mem::sib`]).
    pub const fn indexed(base: Gpr, index: Gpr, scale: u8, disp: i32) -> Mem {
        Mem::sib(base, index, scale, disp)
    }

    /// `[rip + label]`: the 32-bit displacement is patched when the label is bound.
    pub const fn rip(label: Label) -> Mem {
        Mem::rip_disp(label, 0)
    }

    /// `[rip + label + disp]`: like [`Mem::rip`] with a constant added to the label's address.
    pub const fn rip_disp(label: Label, disp: i32) -> Mem {
        Mem {
            base: RAX,
            index: None,
            disp,
            rip: Some(label),
        }
    }

    /// The same operand with displacement `disp` (for RIP operands: the offset added to the label).
    pub const fn with_disp(self, disp: i32) -> Mem {
        Mem { disp, ..self }
    }

    /// Base register, `None` for RIP-relative operands.
    pub const fn base_reg(&self) -> Option<Gpr> {
        if self.rip.is_some() {
            None
        } else {
            Some(self.base)
        }
    }

    /// Index register and scale, if any (always `None` for RIP-relative operands).
    pub const fn index(&self) -> Option<(Gpr, u8)> {
        self.index
    }

    /// The displacement (for RIP operands: the constant added to the label's address).
    pub const fn disp(&self) -> i32 {
        self.disp
    }

    /// The label of a RIP-relative operand.
    pub const fn rip_label(&self) -> Option<Label> {
        self.rip
    }
}

#[derive(Clone, Copy)]
enum Rm {
    Reg(u8),
    Mem(Mem),
}

#[derive(Clone, Copy)]
struct Fixup {
    /// Offset of the 4-byte displacement field.
    pos: usize,
    label: Label,
    /// Bytes of the instruction that follow the displacement field (immediates).
    trail: usize,
    /// Constant added to the label's address.
    disp: i32,
}

/// `target + disp - end` as a 32-bit displacement, or [`AsmError::OutOfRange`].
pub(super) fn rel32_value(target: usize, disp: i32, end: usize) -> Result<i32, AsmError> {
    let t = i64::try_from(target).map_err(|_| AsmError::OutOfRange)?;
    let e = i64::try_from(end).map_err(|_| AsmError::OutOfRange)?;
    let rel = t
        .checked_add(i64::from(disp))
        .and_then(|v| v.checked_sub(e))
        .ok_or(AsmError::OutOfRange)?;
    i32::try_from(rel).map_err(|_| AsmError::OutOfRange)
}

/// x86-64 instruction encoder with labels. Entirely safe code; the result is a plain `Vec<u8>`.
///
/// Conventions: methods taking a [`Size`] encode a 32- or 64-bit operation; `dst` comes first (Intel
/// order). Invalid operands (a bad scale, `rsp` as an index) panic: they are programming errors in the
/// code generator. Unbound, doubly bound and foreign labels are reported by [`Assembler::finish`].
pub struct Assembler {
    id: u32,
    buf: Vec<u8>,
    labels: Vec<Option<usize>>,
    fixups: Vec<Fixup>,
    error: Option<AsmError>,
}

fn fits_i8(v: i32) -> bool {
    i8::try_from(v).is_ok()
}

impl Default for Assembler {
    fn default() -> Self {
        Self::new()
    }
}

impl Assembler {
    /// An empty assembler.
    pub fn new() -> Assembler {
        Assembler {
            id: NEXT_ASM_ID.fetch_add(1, Ordering::Relaxed),
            buf: Vec::new(),
            labels: Vec::new(),
            fixups: Vec::new(),
            error: None,
        }
    }

    /// Current code offset (the offset of the next emitted byte).
    pub fn offset(&self) -> usize {
        self.buf.len()
    }

    /// The bytes emitted so far (label references are not patched until [`Assembler::finish`]).
    pub fn code(&self) -> &[u8] {
        &self.buf
    }

    /// Offset a label was bound to, if bound.
    pub fn label_offset(&self, l: Label) -> Option<usize> {
        if l.asm != self.id {
            return None;
        }
        self.labels.get(l.idx as usize).copied().flatten()
    }

    /// Creates a fresh unbound label.
    pub fn new_label(&mut self) -> Label {
        self.labels.push(None);
        Label {
            asm: self.id,
            idx: u32::try_from(self.labels.len() - 1).expect("too many labels"),
        }
    }

    /// Binds `l` to the current offset. Never panics: binding twice is reported by [`Assembler::finish`]
    /// as [`AsmError::LabelRebound`], binding a label of another assembler as [`AsmError::ForeignLabel`]
    /// (the first such error is the one `finish` returns).
    pub fn bind(&mut self, l: Label) {
        let off = self.buf.len();
        let Some(slot) = (l.asm == self.id)
            .then(|| self.labels.get_mut(l.idx as usize))
            .flatten()
        else {
            self.error.get_or_insert(AsmError::ForeignLabel);
            return;
        };
        if slot.is_some() {
            self.error.get_or_insert(AsmError::LabelRebound(l));
        } else {
            *slot = Some(off);
        }
    }

    /// Resolves every label reference and returns the machine code, or the first error.
    pub fn finish(mut self) -> Result<Vec<u8>, AsmError> {
        if let Some(e) = self.error {
            return Err(e);
        }
        for f in std::mem::take(&mut self.fixups) {
            if f.label.asm != self.id {
                return Err(AsmError::ForeignLabel);
            }
            let target = self
                .labels
                .get(f.label.idx as usize)
                .copied()
                .flatten()
                .ok_or(AsmError::UnboundLabel(f.label))?;
            let end = f.pos + 4 + f.trail;
            let rel = rel32_value(target, f.disp, end)?;
            self.buf[f.pos..f.pos + 4].copy_from_slice(&rel.to_le_bytes());
        }
        Ok(self.buf)
    }

    // ---------------------------------------------------------------- raw emission

    fn b(&mut self, byte: u8) {
        self.buf.push(byte);
    }

    fn imm8(&mut self, v: i8) {
        self.buf.push(v as u8);
    }

    fn imm32(&mut self, v: i32) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }

    /// Emits a label-relative 32-bit field (`trail` = instruction bytes following it).
    fn rel32(&mut self, label: Label, trail: usize, disp: i32) {
        if label.asm != self.id {
            self.error.get_or_insert(AsmError::ForeignLabel);
        }
        self.fixups.push(Fixup {
            pos: self.buf.len(),
            label,
            trail,
            disp,
        });
        self.buf.extend_from_slice(&[0; 4]);
    }

    /// Emits `[prefix] [REX] opcode... modrm [sib] [disp]` for `reg` (register number or `/digit`)
    /// and `rm`. `trail` is the number of immediate bytes the caller will append afterwards.
    #[allow(clippy::too_many_arguments)]
    fn rm_op(
        &mut self,
        prefix: Option<u8>,
        w: bool,
        opc: &[u8],
        reg: u8,
        rm: Rm,
        force_rex: bool,
        trail: usize,
    ) {
        let (x, b) = match rm {
            Rm::Reg(n) => (0, n >> 3),
            Rm::Mem(m) if m.rip.is_some() => (0, 0),
            Rm::Mem(m) => (
                m.index.map_or(0, |(i, _)| i.index() >> 3),
                m.base.index() >> 3,
            ),
        };
        let rex = 0x40 | (u8::from(w) << 3) | ((reg >> 3) << 2) | (x << 1) | b;
        if let Some(p) = prefix {
            self.b(p);
        }
        if rex != 0x40 || force_rex {
            self.b(rex);
        }
        self.buf.extend_from_slice(opc);
        let reg3 = (reg & 7) << 3;
        match rm {
            Rm::Reg(n) => self.b(0xC0 | reg3 | (n & 7)),
            Rm::Mem(m) => self.modrm_mem(reg3, &m, trail),
        }
    }

    fn modrm_mem(&mut self, reg3: u8, m: &Mem, trail: usize) {
        if let Some(label) = m.rip {
            self.b(reg3 | 0b101);
            self.rel32(label, trail, m.disp);
            return;
        }
        let base3 = m.base.index() & 7;
        let (md, disp_len) = if m.disp == 0 && base3 != 5 {
            (0u8, 0)
        } else if fits_i8(m.disp) {
            (1, 1)
        } else {
            (2, 4)
        };
        match m.index {
            None if base3 != 4 => self.b((md << 6) | reg3 | base3),
            _ => {
                self.b((md << 6) | reg3 | 0b100);
                let (ss, idx3) = match m.index {
                    None => (0, 4),
                    Some((i, scale)) => {
                        assert!(i != RSP, "rsp cannot be an index register");
                        let ss = match scale {
                            1 => 0,
                            2 => 1,
                            4 => 2,
                            8 => 3,
                            _ => panic!("invalid scale {scale}"),
                        };
                        (ss, i.index() & 7)
                    }
                };
                self.b((ss << 6) | (idx3 << 3) | base3);
            }
        }
        match disp_len {
            1 => self.imm8(m.disp as i8),
            4 => self.imm32(m.disp),
            _ => {}
        }
    }

    /// Integer `op r/m, reg`-style helper with no legacy prefix.
    fn int_op(&mut self, size: Size, opc: &[u8], reg: u8, rm: Rm, trail: usize) {
        self.rm_op(None, size == Size::S64, opc, reg, rm, false, trail);
    }

    /// Opcode with the register number added to the last opcode byte (`push`, `pop`, `mov r, imm`).
    fn opreg(&mut self, w: bool, opc: u8, r: Gpr) {
        let rex = 0x40 | (u8::from(w) << 3) | (r.index() >> 3);
        if rex != 0x40 {
            self.b(rex);
        }
        self.b(opc + (r.index() & 7));
    }

    // ---------------------------------------------------------------- misc

    /// Appends raw bytes (inline data such as constants or jump tables).
    pub fn emit_bytes(&mut self, bytes: &[u8]) {
        self.buf.extend_from_slice(bytes);
    }

    /// `int3`
    pub fn int3(&mut self) {
        self.b(0xCC);
    }

    /// `ret`
    pub fn ret(&mut self) {
        self.b(0xC3);
    }

    /// Pads with Intel's recommended multi-byte NOP sequences (at most 9 bytes each) until the offset is
    /// a multiple of `n` (a power of two). The absolute alignment holds only if the buffer is placed at
    /// an `n`-aligned address.
    pub fn align(&mut self, n: usize) {
        assert!(n.is_power_of_two(), "alignment must be a power of two");
        let pad = (n - (self.buf.len() & (n - 1))) & (n - 1);
        self.pad(pad);
    }

    /// Emits exactly `pad` bytes of Intel's recommended multi-byte NOPs (at most 9 bytes each).
    pub fn pad(&mut self, mut pad: usize) {
        const NOPS: [&[u8]; 9] = [
            &[0x90],
            &[0x66, 0x90],
            &[0x0F, 0x1F, 0x00],
            &[0x0F, 0x1F, 0x40, 0x00],
            &[0x0F, 0x1F, 0x44, 0x00, 0x00],
            &[0x66, 0x0F, 0x1F, 0x44, 0x00, 0x00],
            &[0x0F, 0x1F, 0x80, 0x00, 0x00, 0x00, 0x00],
            &[0x0F, 0x1F, 0x84, 0x00, 0x00, 0x00, 0x00, 0x00],
            &[0x66, 0x0F, 0x1F, 0x84, 0x00, 0x00, 0x00, 0x00, 0x00],
        ];
        while pad > 0 {
            let k = pad.min(9);
            self.buf.extend_from_slice(NOPS[k - 1]);
            pad -= k;
        }
    }

    // ---------------------------------------------------------------- mov family

    /// `mov dst, src` (register to register).
    pub fn mov_rr(&mut self, size: Size, dst: Gpr, src: Gpr) {
        self.int_op(size, &[0x89], src.index(), Rm::Reg(dst.index()), 0);
    }

    /// Loads an immediate using the shortest encoding: `mov r32, imm32` (zero-extending) for
    /// `0..=u32::MAX`, `mov r/m64, imm32` (sign-extending) for negative `i32` values, else `movabs`.
    pub fn mov_ri(&mut self, dst: Gpr, imm: i64) {
        if let Ok(v) = u32::try_from(imm) {
            self.opreg(false, 0xB8, dst);
            self.buf.extend_from_slice(&v.to_le_bytes());
        } else if let Ok(v) = i32::try_from(imm) {
            self.int_op(Size::S64, &[0xC7], 0, Rm::Reg(dst.index()), 4);
            self.imm32(v);
        } else {
            self.opreg(true, 0xB8, dst);
            self.buf.extend_from_slice(&imm.to_le_bytes());
        }
    }

    /// `movabs dst, imm64`: always the 10-byte form (`REX.W B8+r imm64`), e.g. to load an absolute address.
    /// x86-64 has no absolute memory operand without a register: load the address with this (or
    /// [`Assembler::mov_ri`]) and use `[reg]`.
    pub fn mov_ri64(&mut self, dst: Gpr, imm: u64) {
        self.opreg(true, 0xB8, dst);
        self.buf.extend_from_slice(&imm.to_le_bytes());
    }

    /// `mov dst, [m]`
    pub fn mov_rm(&mut self, size: Size, dst: Gpr, m: Mem) {
        self.int_op(size, &[0x8B], dst.index(), Rm::Mem(m), 0);
    }

    /// `mov [m], src`
    pub fn mov_mr(&mut self, size: Size, m: Mem, src: Gpr) {
        self.int_op(size, &[0x89], src.index(), Rm::Mem(m), 0);
    }

    /// `mov qword/dword [m], imm32` (the 64-bit form sign-extends the immediate).
    pub fn mov_mi(&mut self, size: Size, m: Mem, imm: i32) {
        self.int_op(size, &[0xC7], 0, Rm::Mem(m), 4);
        self.imm32(imm);
    }

    /// `mov byte [m], imm8`
    pub fn mov_m8_i(&mut self, m: Mem, imm: i8) {
        self.int_op(Size::S32, &[0xC6], 0, Rm::Mem(m), 1);
        self.imm8(imm);
    }

    /// `mov byte [m], src8` (`spl`/`bpl`/`sil`/`dil` get the REX prefix they need).
    pub fn mov_m8_r(&mut self, m: Mem, src: Gpr) {
        let force = (4..8).contains(&src.index());
        self.rm_op(None, false, &[0x88], src.index(), Rm::Mem(m), force, 0);
    }

    /// `movzx dst32, byte [m]`
    pub fn movzx_rm8(&mut self, dst: Gpr, m: Mem) {
        self.int_op(Size::S32, &[0x0F, 0xB6], dst.index(), Rm::Mem(m), 0);
    }

    /// `movzx dst32, src8`
    pub fn movzx_rr8(&mut self, dst: Gpr, src: Gpr) {
        let force = (4..8).contains(&src.index());
        self.rm_op(
            None,
            false,
            &[0x0F, 0xB6],
            dst.index(),
            Rm::Reg(src.index()),
            force,
            0,
        );
    }

    /// `lea dst, [m]`
    pub fn lea(&mut self, size: Size, dst: Gpr, m: Mem) {
        self.int_op(size, &[0x8D], dst.index(), Rm::Mem(m), 0);
    }

    // ---------------------------------------------------------------- ALU

    /// `op dst, src` for `add/or/adc/sbb/and/sub/xor/cmp`.
    pub fn alu_rr(&mut self, op: Alu, size: Size, dst: Gpr, src: Gpr) {
        self.int_op(
            size,
            &[((op as u8) << 3) | 1],
            src.index(),
            Rm::Reg(dst.index()),
            0,
        );
    }

    /// `op dst, imm` (`imm8` form when the value fits, else `imm32`).
    pub fn alu_ri(&mut self, op: Alu, size: Size, dst: Gpr, imm: i32) {
        self.alu_imm(op, size, Rm::Reg(dst.index()), imm);
    }

    /// `op dst, [m]`
    pub fn alu_rm(&mut self, op: Alu, size: Size, dst: Gpr, m: Mem) {
        self.int_op(size, &[((op as u8) << 3) | 3], dst.index(), Rm::Mem(m), 0);
    }

    /// `op [m], src`
    pub fn alu_mr(&mut self, op: Alu, size: Size, m: Mem, src: Gpr) {
        self.int_op(size, &[((op as u8) << 3) | 1], src.index(), Rm::Mem(m), 0);
    }

    /// `op qword/dword [m], imm`
    pub fn alu_mi(&mut self, op: Alu, size: Size, m: Mem, imm: i32) {
        self.alu_imm(op, size, Rm::Mem(m), imm);
    }

    fn alu_imm(&mut self, op: Alu, size: Size, rm: Rm, imm: i32) {
        if fits_i8(imm) {
            self.int_op(size, &[0x83], op as u8, rm, 1);
            self.imm8(imm as i8);
        } else {
            self.int_op(size, &[0x81], op as u8, rm, 4);
            self.imm32(imm);
        }
    }

    /// `test a, b` (also covers `test r, [m]`, which is the same operation: use [`Assembler::test_mr`]).
    pub fn test_rr(&mut self, size: Size, a: Gpr, b: Gpr) {
        self.int_op(size, &[0x85], b.index(), Rm::Reg(a.index()), 0);
    }

    /// `test r, imm32`
    pub fn test_ri(&mut self, size: Size, r: Gpr, imm: i32) {
        self.int_op(size, &[0xF7], 0, Rm::Reg(r.index()), 4);
        self.imm32(imm);
    }

    /// `test [m], r` (equivalently `test r, [m]`).
    pub fn test_mr(&mut self, size: Size, m: Mem, r: Gpr) {
        self.int_op(size, &[0x85], r.index(), Rm::Mem(m), 0);
    }

    /// `test qword/dword [m], imm32`
    pub fn test_mi(&mut self, size: Size, m: Mem, imm: i32) {
        self.int_op(size, &[0xF7], 0, Rm::Mem(m), 4);
        self.imm32(imm);
    }

    /// `cmp byte [m], imm8` (`80 /7 ib`; the immediate is compared as an unsigned byte pattern).
    pub fn cmp_m8_i(&mut self, m: Mem, imm: u8) {
        self.int_op(Size::S32, &[0x80], Alu::Cmp as u8, Rm::Mem(m), 1);
        self.b(imm);
    }

    /// `cmp word [m], imm16` (`66 81 /7 iw`).
    pub fn cmp_m16_i(&mut self, m: Mem, imm: u16) {
        self.rm_op(Some(0x66), false, &[0x81], Alu::Cmp as u8, Rm::Mem(m), false, 2);
        self.b(imm as u8);
        self.b((imm >> 8) as u8);
    }

    /// `test byte [m], imm8` (`F6 /0 ib`).
    pub fn test_m8_i(&mut self, m: Mem, imm: u8) {
        self.int_op(Size::S32, &[0xF6], 0, Rm::Mem(m), 1);
        self.b(imm);
    }

    /// `imul dst, src`
    pub fn imul_rr(&mut self, size: Size, dst: Gpr, src: Gpr) {
        self.int_op(size, &[0x0F, 0xAF], dst.index(), Rm::Reg(src.index()), 0);
    }

    /// `imul dst, [m]`
    pub fn imul_rm(&mut self, size: Size, dst: Gpr, m: Mem) {
        self.int_op(size, &[0x0F, 0xAF], dst.index(), Rm::Mem(m), 0);
    }

    /// `imul dst, src, imm` (`imm8` form when the value fits).
    pub fn imul_rri(&mut self, size: Size, dst: Gpr, src: Gpr, imm: i32) {
        if fits_i8(imm) {
            self.int_op(size, &[0x6B], dst.index(), Rm::Reg(src.index()), 1);
            self.imm8(imm as i8);
        } else {
            self.int_op(size, &[0x69], dst.index(), Rm::Reg(src.index()), 4);
            self.imm32(imm);
        }
    }

    /// `neg r`
    pub fn neg(&mut self, size: Size, r: Gpr) {
        self.int_op(size, &[0xF7], 3, Rm::Reg(r.index()), 0);
    }

    /// `not r`
    pub fn not(&mut self, size: Size, r: Gpr) {
        self.int_op(size, &[0xF7], 2, Rm::Reg(r.index()), 0);
    }

    /// `inc r`
    pub fn inc(&mut self, size: Size, r: Gpr) {
        self.int_op(size, &[0xFF], 0, Rm::Reg(r.index()), 0);
    }

    /// `dec r`
    pub fn dec(&mut self, size: Size, r: Gpr) {
        self.int_op(size, &[0xFF], 1, Rm::Reg(r.index()), 0);
    }

    /// `shl/shr/sar r, imm8`
    pub fn shift_ri(&mut self, op: Shift, size: Size, r: Gpr, imm: u8) {
        self.int_op(size, &[0xC1], op as u8, Rm::Reg(r.index()), 1);
        self.b(imm);
    }

    /// `shl/shr/sar r, cl`
    pub fn shift_cl(&mut self, op: Shift, size: Size, r: Gpr) {
        self.int_op(size, &[0xD3], op as u8, Rm::Reg(r.index()), 0);
    }

    /// `cqo` (sign-extend `rax` into `rdx:rax`).
    pub fn cqo(&mut self) {
        self.b(0x48);
        self.b(0x99);
    }

    /// `cdq` (sign-extend `eax` into `edx:eax`).
    pub fn cdq(&mut self) {
        self.b(0x99);
    }

    /// `idiv r` (signed divide of `rdx:rax` / `edx:eax`).
    pub fn idiv(&mut self, size: Size, r: Gpr) {
        self.int_op(size, &[0xF7], 7, Rm::Reg(r.index()), 0);
    }

    /// `mul r` (unsigned multiply `rdx:rax = rax * r`).
    pub fn mul1(&mut self, size: Size, r: Gpr) {
        self.int_op(size, &[0xF7], 4, Rm::Reg(r.index()), 0);
    }

    /// `div r` (unsigned divide of `rdx:rax` / `edx:eax`).
    pub fn div(&mut self, size: Size, r: Gpr) {
        self.int_op(size, &[0xF7], 6, Rm::Reg(r.index()), 0);
    }

    /// `bt r, bit_reg`
    pub fn bt_rr(&mut self, size: Size, r: Gpr, bit: Gpr) {
        self.int_op(size, &[0x0F, 0xA3], bit.index(), Rm::Reg(r.index()), 0);
    }

    /// `bt r, imm8`
    pub fn bt_ri(&mut self, size: Size, r: Gpr, imm: u8) {
        self.int_op(size, &[0x0F, 0xBA], 4, Rm::Reg(r.index()), 1);
        self.b(imm);
    }

    /// `setcc dst8` (`spl`/`bpl`/`sil`/`dil` get the REX prefix they need).
    pub fn setcc(&mut self, cc: Cond, dst: Gpr) {
        let force = (4..8).contains(&dst.index());
        self.rm_op(
            None,
            false,
            &[0x0F, 0x90 | cc as u8],
            0,
            Rm::Reg(dst.index()),
            force,
            0,
        );
    }

    /// `cmovcc dst, src`
    pub fn cmovcc(&mut self, cc: Cond, size: Size, dst: Gpr, src: Gpr) {
        self.int_op(
            size,
            &[0x0F, 0x40 | cc as u8],
            dst.index(),
            Rm::Reg(src.index()),
            0,
        );
    }

    // ---------------------------------------------------------------- stack and control flow

    /// `push r64`
    pub fn push(&mut self, r: Gpr) {
        self.opreg(false, 0x50, r);
    }

    /// `pop r64`
    pub fn pop(&mut self, r: Gpr) {
        self.opreg(false, 0x58, r);
    }

    /// `call r64`
    pub fn call_r(&mut self, r: Gpr) {
        self.int_op(Size::S32, &[0xFF], 2, Rm::Reg(r.index()), 0);
    }

    /// `call qword [m]`
    pub fn call_m(&mut self, m: Mem) {
        self.int_op(Size::S32, &[0xFF], 2, Rm::Mem(m), 0);
    }

    /// `call rel32` to a label.
    pub fn call_label(&mut self, l: Label) {
        self.b(0xE8);
        self.rel32(l, 0, 0);
    }

    /// Calls an absolute address: `movabs r11, addr; call r11` (13 bytes, clobbers `r11`, which is
    /// caller-saved and not an argument register in the System V ABI).
    pub fn call_abs(&mut self, addr: u64) {
        self.mov_ri64(R11, addr);
        self.call_r(R11);
    }

    /// Jumps to an absolute address: `movabs r11, addr; jmp r11` (13 bytes, clobbers `r11`).
    pub fn jmp_abs(&mut self, addr: u64) {
        self.mov_ri64(R11, addr);
        self.jmp_r(R11);
    }

    /// `jmp r64`
    pub fn jmp_r(&mut self, r: Gpr) {
        self.int_op(Size::S32, &[0xFF], 4, Rm::Reg(r.index()), 0);
    }

    /// `jmp qword [m]`
    pub fn jmp_m(&mut self, m: Mem) {
        self.int_op(Size::S32, &[0xFF], 4, Rm::Mem(m), 0);
    }

    /// `jmp rel32` to a label (always the 32-bit form).
    pub fn jmp_label(&mut self, l: Label) {
        self.b(0xE9);
        self.rel32(l, 0, 0);
    }

    /// `jcc rel32` to a label (always the 32-bit form).
    pub fn jcc_label(&mut self, cc: Cond, l: Label) {
        self.b(0x0F);
        self.b(0x80 | cc as u8);
        self.rel32(l, 0, 0);
    }

    // ---------------------------------------------------------------- SSE2 scalar double

    fn sse(&mut self, prefix: Option<u8>, w: bool, opc: &[u8], reg: u8, rm: Rm, trail: usize) {
        self.rm_op(prefix, w, opc, reg, rm, false, trail);
    }

    /// `movsd dst, src`
    pub fn movsd_rr(&mut self, dst: Xmm, src: Xmm) {
        self.sse(
            Some(0xF2),
            false,
            &[0x0F, 0x10],
            dst.index(),
            Rm::Reg(src.index()),
            0,
        );
    }

    /// `movsd dst, qword [m]`
    pub fn movsd_rm(&mut self, dst: Xmm, m: Mem) {
        self.sse(Some(0xF2), false, &[0x0F, 0x10], dst.index(), Rm::Mem(m), 0);
    }

    /// `movsd qword [m], src`
    pub fn movsd_mr(&mut self, m: Mem, src: Xmm) {
        self.sse(Some(0xF2), false, &[0x0F, 0x11], src.index(), Rm::Mem(m), 0);
    }

    /// `movaps dst, src` (whole-register copy, the preferred register-to-register move).
    pub fn movaps_rr(&mut self, dst: Xmm, src: Xmm) {
        self.sse(
            None,
            false,
            &[0x0F, 0x28],
            dst.index(),
            Rm::Reg(src.index()),
            0,
        );
    }

    /// `xorps dst, src` (`xorps x, x` is the zeroing idiom that breaks register dependencies).
    pub fn xorps_rr(&mut self, dst: Xmm, src: Xmm) {
        self.sse(
            None,
            false,
            &[0x0F, 0x57],
            dst.index(),
            Rm::Reg(src.index()),
            0,
        );
    }

    /// `movq dst_xmm, src_r64`
    pub fn movq_xr(&mut self, dst: Xmm, src: Gpr) {
        self.sse(
            Some(0x66),
            true,
            &[0x0F, 0x6E],
            dst.index(),
            Rm::Reg(src.index()),
            0,
        );
    }

    /// `movq dst_r64, src_xmm`
    pub fn movq_rx(&mut self, dst: Gpr, src: Xmm) {
        self.sse(
            Some(0x66),
            true,
            &[0x0F, 0x7E],
            src.index(),
            Rm::Reg(dst.index()),
            0,
        );
    }

    /// `addsd/subsd/mulsd/divsd/sqrtsd/minsd/maxsd dst, src`
    pub fn sse_rr(&mut self, op: SseOp, dst: Xmm, src: Xmm) {
        self.sse(
            Some(0xF2),
            false,
            &[0x0F, op as u8],
            dst.index(),
            Rm::Reg(src.index()),
            0,
        );
    }

    /// `addsd/subsd/mulsd/divsd/sqrtsd/minsd/maxsd dst, qword [m]`
    pub fn sse_rm(&mut self, op: SseOp, dst: Xmm, m: Mem) {
        self.sse(
            Some(0xF2),
            false,
            &[0x0F, op as u8],
            dst.index(),
            Rm::Mem(m),
            0,
        );
    }

    /// `ucomisd a, b`
    pub fn ucomisd_rr(&mut self, a: Xmm, b: Xmm) {
        self.sse(
            Some(0x66),
            false,
            &[0x0F, 0x2E],
            a.index(),
            Rm::Reg(b.index()),
            0,
        );
    }

    /// `ucomisd a, qword [m]`
    pub fn ucomisd_rm(&mut self, a: Xmm, m: Mem) {
        self.sse(Some(0x66), false, &[0x0F, 0x2E], a.index(), Rm::Mem(m), 0);
    }

    /// `cvtsi2sd dst, src` (32- or 64-bit integer source).
    pub fn cvtsi2sd_rr(&mut self, size: Size, dst: Xmm, src: Gpr) {
        self.sse(
            Some(0xF2),
            size == Size::S64,
            &[0x0F, 0x2A],
            dst.index(),
            Rm::Reg(src.index()),
            0,
        );
    }

    /// `cvtsi2sd dst, dword/qword [m]`
    pub fn cvtsi2sd_rm(&mut self, size: Size, dst: Xmm, m: Mem) {
        self.sse(
            Some(0xF2),
            size == Size::S64,
            &[0x0F, 0x2A],
            dst.index(),
            Rm::Mem(m),
            0,
        );
    }

    /// `cvttsd2si dst, src` (truncating conversion to a 32- or 64-bit integer).
    pub fn cvttsd2si(&mut self, size: Size, dst: Gpr, src: Xmm) {
        self.sse(
            Some(0xF2),
            size == Size::S64,
            &[0x0F, 0x2C],
            dst.index(),
            Rm::Reg(src.index()),
            0,
        );
    }

    /// `andpd/andnpd/xorpd dst, src`
    pub fn logic_rr(&mut self, op: SseLogic, dst: Xmm, src: Xmm) {
        self.sse(
            Some(0x66),
            false,
            &[0x0F, op as u8],
            dst.index(),
            Rm::Reg(src.index()),
            0,
        );
    }

    /// `andpd/andnpd/xorpd dst, xmmword [m]` (the memory operand must be 16-byte aligned at run time).
    pub fn logic_rm(&mut self, op: SseLogic, dst: Xmm, m: Mem) {
        self.sse(
            Some(0x66),
            false,
            &[0x0F, op as u8],
            dst.index(),
            Rm::Mem(m),
            0,
        );
    }

    /// `roundsd dst, src, mode`. Requires SSE4.1: the caller must runtime-detect support
    /// (`is_x86_feature_detected!("sse4.1")`) before executing code containing this instruction.
    pub fn roundsd_rr(&mut self, dst: Xmm, src: Xmm, mode: u8) {
        self.sse(
            Some(0x66),
            false,
            &[0x0F, 0x3A, 0x0B],
            dst.index(),
            Rm::Reg(src.index()),
            1,
        );
        self.b(mode);
    }

    /// `roundsd dst, qword [m], mode`. Requires SSE4.1, see [`Assembler::roundsd_rr`].
    pub fn roundsd_rm(&mut self, dst: Xmm, m: Mem, mode: u8) {
        self.sse(
            Some(0x66),
            false,
            &[0x0F, 0x3A, 0x0B],
            dst.index(),
            Rm::Mem(m),
            1,
        );
        self.b(mode);
    }
}
