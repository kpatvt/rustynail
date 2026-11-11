//! Port of `lcode.c` (Lua 5.4.9).
use super::*;
use rn_core::instruction::{
    opcode_from_u8, Instruction, MAXARG_AX, MAXARG_BX, MAXARG_C, OFFSET_SBX, OFFSET_SC, OFFSET_SJ,
};
use rn_core::number::{self, Number, F2I};
use rn_core::ver::active::{OpCode, OP_PROPS};

type E<H> = ExpDesc<<H as CompilerHost>::Str>;

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(clippy::enum_variant_names)]
pub(crate) enum BinOpr {
    Add,
    Sub,
    Mul,
    Mod,
    Pow,
    Div,
    IDiv,
    BAnd,
    BOr,
    BXor,
    Shl,
    Shr,
    Concat,
    Eq,
    Lt,
    Le,
    Ne,
    Gt,
    Ge,
    And,
    Or,
    NoBinOpr,
}

impl BinOpr {
    pub(crate) fn from_i32(v: i32) -> BinOpr {
        use BinOpr::*;
        const ALL: [BinOpr; 22] = [
            Add, Sub, Mul, Mod, Pow, Div, IDiv, BAnd, BOr, BXor, Shl, Shr, Concat, Eq, Lt, Le, Ne,
            Gt, Ge, And, Or, NoBinOpr,
        ];
        ALL.get(v as usize).copied().unwrap_or(NoBinOpr)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(clippy::enum_variant_names)]
pub(crate) enum UnOpr {
    Minus,
    BNot,
    Not,
    Len,
    NoUnOpr,
}

const ABSLINEINFO: i8 = -0x80;
const LIMLINEDIFF: i32 = 0x80;
const TM_ADD: i32 = 6;
const TM_SHL: i32 = 16;
const TM_SHR: i32 = 17;
const TM_SUB: i32 = 7;
const LUAI_MAXSHORTLEN: usize = 40;

fn op_of(v: i32) -> OpCode {
    opcode_from_u8(v as u8).unwrap_or(OpCode::Move)
}

fn fits_c(i: i64) -> bool {
    (i as u64).wrapping_add(OFFSET_SC as u64) <= MAXARG_C as u64
}

fn fits_bx(i: i64) -> bool {
    -(OFFSET_SBX as i64) <= i && i <= (MAXARG_BX as i64) - (OFFSET_SBX as i64)
}

fn nvalue(n: Number) -> f64 {
    match n {
        Number::Int(i) => i as f64,
        Number::Float(f) => f,
    }
}

fn tointegerns(n: Number) -> Option<i64> {
    match n {
        Number::Int(i) => Some(i),
        Number::Float(f) => number::float_to_int(f, F2I::Eq),
    }
}

fn ceil_log2(x: u32) -> u32 {
    // luaO_ceillog2
    if x <= 1 {
        0
    } else {
        32 - (x - 1).leading_zeros()
    }
}

fn valid_op(op: i32, v1: Number, v2: Number) -> bool {
    match op {
        7..=11 | 13 => tointegerns(v1).is_some() && tointegerns(v2).is_some(),
        3 | 5 | 6 => nvalue(v2) != 0.0,
        _ => true,
    }
}

fn intarith(op: i32, a: i64, b: i64) -> i64 {
    match op {
        0 => a.wrapping_add(b),
        1 => a.wrapping_sub(b),
        2 => a.wrapping_mul(b),
        3 => number::imod(a, b).unwrap_or(0),
        6 => number::idiv(a, b).unwrap_or(0),
        7 => a & b,
        8 => a | b,
        9 => a ^ b,
        10 => number::shiftl(a, b),
        11 => number::shiftr(a, b),
        12 => 0i64.wrapping_sub(a),
        13 => !a,
        _ => 0,
    }
}

fn numarith(op: i32, a: f64, b: f64) -> f64 {
    match op {
        0 => a + b,
        1 => a - b,
        2 => a * b,
        3 => number::fmod_lua(a, b),
        4 => number::pow_lua(a, b),
        5 => a / b,
        6 => number::fidiv(a, b),
        12 => -a,
        _ => 0.0,
    }
}

/// `luaO_rawarith` restricted to the cases constant folding can reach.
fn raw_arith(op: i32, v1: Number, v2: Number) -> Number {
    match op {
        7..=11 | 13 => {
            let a = tointegerns(v1).unwrap_or(0);
            let b = tointegerns(v2).unwrap_or(0);
            Number::Int(intarith(op, a, b))
        }
        4 | 5 => Number::Float(numarith(op, nvalue(v1), nvalue(v2))),
        _ => match (v1, v2) {
            (Number::Int(a), Number::Int(b)) => Number::Int(intarith(op, a, b)),
            _ => Number::Float(numarith(op, nvalue(v1), nvalue(v2))),
        },
    }
}

impl<H: CompilerHost> Parser<'_, H> {
    // ---- small accessors -------------------------------------------------------------------

    pub(crate) fn fs(&mut self) -> &mut FuncState<H::Str> {
        let n = self.fstack.len() - 1;
        &mut self.fstack[n]
    }

    pub(crate) fn fsr(&self) -> &FuncState<H::Str> {
        &self.fstack[self.fstack.len() - 1]
    }

    fn inst(&self, pc: i32) -> Instruction {
        Instruction(self.fsr().f.code[pc as usize])
    }

    fn set_inst(&mut self, pc: i32, i: Instruction) {
        self.fs().f.code[pc as usize] = i.0;
    }

    fn modify<F: FnOnce(&mut Instruction)>(&mut self, pc: i32, f: F) {
        let mut i = self.inst(pc);
        f(&mut i);
        self.set_inst(pc, i);
    }

    fn op_at(&self, pc: i32) -> Option<OpCode> {
        self.inst(pc).opcode()
    }

    fn pc(&self) -> i32 {
        self.fsr().pc()
    }

    pub(crate) fn error_limit(&mut self, fi: usize, limit: i32, what: &str) -> CompileError {
        let line = self.fstack[fi].f.line_defined;
        let wh = if line == 0 {
            "main function".to_string()
        } else {
            format!("function at line {line}")
        };
        let msg = format!("too many {what} (limit is {limit}) in {wh}");
        self.ls.syntax_error(&msg)
    }

    pub(crate) fn check_limit(&mut self, fi: usize, v: i32, l: i32, what: &str) -> R<()> {
        if v > l {
            return perr(self.error_limit(fi, l, what));
        }
        Ok(())
    }

    fn sem_error(&mut self, msg: &str) -> CompileError {
        self.ls.sem_error(msg.as_bytes())
    }

    // ---- constants -----------------------------------------------------------------------------

    fn tonumeral(e: &E<H>) -> Option<Number> {
        if e.has_jumps() {
            return None;
        }
        match e.k {
            ExpKind::KInt => Some(Number::Int(e.ival)),
            ExpKind::KFlt => Some(Number::Float(e.nval)),
            _ => None,
        }
    }

    fn is_numeral(e: &E<H>) -> bool {
        Self::tonumeral(e).is_some()
    }

    fn const2val(&self, e: &E<H>) -> Const<H::Str> {
        self.actvar[e.info as usize].k.clone()
    }

    /// `luaK_exp2const`
    pub(crate) fn k_exp2const(&self, e: &E<H>) -> Option<Const<H::Str>> {
        if e.has_jumps() {
            return None;
        }
        match e.k {
            ExpKind::False => Some(Const::False),
            ExpKind::True => Some(Const::True),
            ExpKind::Nil => Some(Const::Nil),
            ExpKind::KStr => e.strval.map(Const::Str),
            ExpKind::Const => Some(self.const2val(e)),
            _ => Self::tonumeral(e).map(|n| match n {
                Number::Int(i) => Const::Int(i),
                Number::Float(f) => Const::Float(f),
            }),
        }
    }

    fn const2exp(v: &Const<H::Str>, e: &mut E<H>) {
        match *v {
            Const::Int(i) => {
                e.k = ExpKind::KInt;
                e.ival = i;
            }
            Const::Float(f) => {
                e.k = ExpKind::KFlt;
                e.nval = f;
            }
            Const::False => e.k = ExpKind::False,
            Const::True => e.k = ExpKind::True,
            Const::Nil => e.k = ExpKind::Nil,
            Const::Str(s) => {
                e.k = ExpKind::KStr;
                e.strval = Some(s);
            }
        }
    }

    /// `addk`: the cache is the scanner table, shared by all functions (so an entry may refer to a
    /// constant of another function); a hit is only used if it is a valid index of the current
    /// function holding an equal value of the same type.
    fn addk(&mut self, key: Option<KKey>, v: Const<H::Str>) -> usize {
        let n = self.fstack.len() - 1;
        let f = &mut self.fstack[n].f;
        let k = f.k.len();
        if let Some(key) = key {
            // one hash lookup: validate a hit, or (re)point the entry at the new constant
            match self.kcache.entry(key) {
                std::collections::hash_map::Entry::Occupied(mut e) => {
                    let c = *e.get();
                    if c < k && f.k[c] == v {
                        return c;
                    }
                    e.insert(k);
                }
                std::collections::hash_map::Entry::Vacant(e) => {
                    e.insert(k);
                }
            }
        }
        // luaM_growvector(f->k, k, ..., MAXARG_Ax, "constants"); deferred to the next `k_code`
        // (reaching it needs ~33.5M distinct constants, i.e. gigabytes of source)
        if k >= MAXARG_AX as usize && self.limit_error.is_none() {
            self.limit_error =
                Some(format!("too many constants (limit is {MAXARG_AX})").into_bytes());
        }
        f.k.push(v);
        k
    }

    fn string_k(&mut self, s: H::Str) -> usize {
        let key = KKey::Str(H::str_id(s));
        self.addk(Some(key), Const::Str(s))
    }

    fn int_k(&mut self, n: i64) -> usize {
        self.addk(Some(KKey::Int(n)), Const::Int(n))
    }

    /// Table key for a float (`luaH_get` turns floats with integral values into integer keys).
    fn float_key(f: f64) -> Option<KKey> {
        if f.is_nan() {
            return None;
        }
        Some(match number::float_to_int(f, F2I::Eq) {
            Some(i) => KKey::Int(i),
            None => KKey::Flt(f.to_bits()),
        })
    }

    fn number_k(&mut self, r: f64) -> usize {
        let o = Const::Float(r);
        match number::float_to_int(r, F2I::Eq) {
            None => self.addk(Self::float_key(r), o),
            Some(ik) => {
                // ldexp(1.0, -MANT_DIG + 1)
                let q = 2f64.powi(-52);
                let k = if ik == 0 { q } else { r + r * q };
                self.addk(Self::float_key(k), o)
            }
        }
    }

    fn bool_f(&mut self) -> usize {
        self.addk(Some(KKey::False), Const::False)
    }
    fn bool_t(&mut self) -> usize {
        self.addk(Some(KKey::True), Const::True)
    }
    fn nil_k(&mut self) -> usize {
        self.addk(Some(KKey::Nil), Const::Nil)
    }

    // ---- line info and raw code emission ----------------------------------------------------

    fn save_line_info(&mut self, line: i32) {
        let fs = self.fs();
        let mut linedif = line - fs.previousline;
        let pc = fs.f.code.len() as i32 - 1;
        let need_abs = linedif.abs() >= LIMLINEDIFF || {
            let c = fs.iwthabs;
            fs.iwthabs = fs.iwthabs.wrapping_add(1);
            c >= MAXIWTHABS
        };
        if need_abs {
            fs.f.abs_line_info.push(rn_core::protodata::AbsLineInfo {
                pc: pc as u32,
                line,
            });
            linedif = ABSLINEINFO as i32;
            fs.iwthabs = 1;
        }
        fs.f.line_info.push(linedif as i8);
        fs.previousline = line;
    }

    fn remove_last_line_info(&mut self) {
        let fs = self.fs();
        let d = fs.f.line_info.pop().unwrap_or(0);
        if d != ABSLINEINFO {
            fs.previousline -= i32::from(d);
            fs.iwthabs = fs.iwthabs.wrapping_sub(1);
        } else {
            fs.f.abs_line_info.pop();
            fs.iwthabs = MAXIWTHABS + 1;
        }
    }

    fn remove_last_instruction(&mut self) {
        self.remove_last_line_info();
        self.fs().f.code.pop();
    }

    /// `luaK_code`
    pub(crate) fn k_code(&mut self, i: Instruction) -> R<i32> {
        if let Some(message) = self.limit_error.take() {
            return perr(CompileError { message });
        }
        if self.fsr().f.code.len() >= i32::MAX as usize {
            // luaM_growvector(f->code, pc, ..., INT_MAX, "opcodes")
            return perr(CompileError {
                message: format!("too many opcodes (limit is {})", i32::MAX).into_bytes(),
            });
        }
        self.fs().f.code.push(i.0);
        let line = self.ls.last_line;
        self.save_line_info(line);
        Ok(self.pc() - 1)
    }

    pub(crate) fn k_code_abck(&mut self, o: OpCode, a: i32, b: i32, c: i32, k: i32) -> R<i32> {
        self.k_code(Instruction::abck(o, a as u32, b as u32, c as u32, k as u32))
    }

    pub(crate) fn k_code_abc(&mut self, o: OpCode, a: i32, b: i32, c: i32) -> R<i32> {
        self.k_code_abck(o, a, b, c, 0)
    }

    pub(crate) fn k_code_abx(&mut self, o: OpCode, a: i32, bc: i32) -> R<i32> {
        self.k_code(Instruction::abx(o, a as u32, bc as u32))
    }

    fn code_asbx(&mut self, o: OpCode, a: i32, bc: i32) -> R<i32> {
        self.k_code(Instruction::asbx(o, a as u32, bc))
    }

    fn codes_j(&mut self, o: OpCode, sj: i32, k: i32) -> R<i32> {
        self.k_code(Instruction::sj_(o, sj, k as u32))
    }

    fn code_extra_arg(&mut self, a: i32) -> R<i32> {
        self.k_code(Instruction::ax_(OpCode::Extraarg, a as u32))
    }

    fn k_codek(&mut self, reg: i32, k: i32) -> R<i32> {
        if k <= MAXARG_BX as i32 {
            self.k_code_abx(OpCode::Loadk, reg, k)
        } else {
            let p = self.k_code_abx(OpCode::Loadkx, reg, 0)?;
            self.code_extra_arg(k)?;
            Ok(p)
        }
    }

    fn previous_instruction(&self) -> Option<i32> {
        let fs = self.fsr();
        if fs.pc() > fs.lasttarget {
            Some(fs.pc() - 1)
        } else {
            None
        }
    }

    // ---- nil / jumps -----------------------------------------------------------------------------

    /// `luaK_nil`
    pub(crate) fn k_nil(&mut self, mut from: i32, n: i32) -> R<()> {
        let mut l = from + n - 1;
        if let Some(prev) = self.previous_instruction() {
            let pi = self.inst(prev);
            if pi.opcode() == Some(OpCode::Loadnil) {
                let pfrom = pi.a() as i32;
                let pl = pfrom + pi.b() as i32;
                if (pfrom <= from && from <= pl + 1) || (from <= pfrom && pfrom <= l + 1) {
                    if pfrom < from {
                        from = pfrom;
                    }
                    if pl > l {
                        l = pl;
                    }
                    self.modify(prev, |i| {
                        i.set_a(from as u32);
                        i.set_b((l - from) as u32);
                    });
                    return Ok(());
                }
            }
        }
        self.k_code_abc(OpCode::Loadnil, from, n - 1, 0)?;
        Ok(())
    }

    fn get_jump(&self, pc: i32) -> i32 {
        let offset = self.inst(pc).sj();
        if offset == NO_JUMP {
            NO_JUMP
        } else {
            pc + 1 + offset
        }
    }

    fn fix_jump(&mut self, pc: i32, dest: i32) -> R<()> {
        let offset = dest - (pc + 1);
        if !(-OFFSET_SJ <= offset && offset <= (rn_core::instruction::MAXARG_SJ as i32) - OFFSET_SJ)
        {
            return perr(self.ls.syntax_error("control structure too long"));
        }
        self.modify(pc, |i| i.set_sj(offset));
        Ok(())
    }

    /// `luaK_concat`
    pub(crate) fn k_concat(&mut self, l1: &mut i32, l2: i32) -> R<()> {
        if l2 == NO_JUMP {
            return Ok(());
        }
        if *l1 == NO_JUMP {
            *l1 = l2;
        } else {
            let mut list = *l1;
            loop {
                let next = self.get_jump(list);
                if next == NO_JUMP {
                    break;
                }
                list = next;
            }
            self.fix_jump(list, l2)?;
        }
        Ok(())
    }

    pub(crate) fn k_jump(&mut self) -> R<i32> {
        self.codes_j(OpCode::Jmp, NO_JUMP, 0)
    }

    pub(crate) fn k_ret(&mut self, first: i32, nret: i32) -> R<()> {
        let op = match nret {
            0 => OpCode::Return0,
            1 => OpCode::Return1,
            _ => OpCode::Return,
        };
        self.k_code_abc(op, first, nret + 1, 0)?;
        Ok(())
    }

    fn cond_jump(&mut self, op: OpCode, a: i32, b: i32, c: i32, k: i32) -> R<i32> {
        self.k_code_abck(op, a, b, c, k)?;
        self.k_jump()
    }

    pub(crate) fn k_get_label(&mut self) -> i32 {
        let fs = self.fs();
        fs.lasttarget = fs.pc();
        fs.pc()
    }

    fn get_jump_control(&self, pc: i32) -> i32 {
        if pc >= 1 {
            if let Some(op) = self.op_at(pc - 1) {
                if OP_PROPS[op as usize].t {
                    return pc - 1;
                }
            }
        }
        pc
    }

    fn patch_test_reg(&mut self, node: i32, reg: i32) -> bool {
        let ip = self.get_jump_control(node);
        let i = self.inst(ip);
        if i.opcode() != Some(OpCode::Testset) {
            return false;
        }
        if reg != NO_REG && reg != i.b() as i32 {
            self.modify(ip, |x| x.set_a(reg as u32));
        } else {
            self.set_inst(ip, Instruction::abck(OpCode::Test, i.b(), 0, 0, i.k()));
        }
        true
    }

    fn remove_values(&mut self, mut list: i32) {
        while list != NO_JUMP {
            self.patch_test_reg(list, NO_REG);
            list = self.get_jump(list);
        }
    }

    fn patch_list_aux(&mut self, mut list: i32, vtarget: i32, reg: i32, dtarget: i32) -> R<()> {
        while list != NO_JUMP {
            let next = self.get_jump(list);
            if self.patch_test_reg(list, reg) {
                self.fix_jump(list, vtarget)?;
            } else {
                self.fix_jump(list, dtarget)?;
            }
            list = next;
        }
        Ok(())
    }

    pub(crate) fn k_patch_list(&mut self, list: i32, target: i32) -> R<()> {
        self.patch_list_aux(list, target, NO_REG, target)
    }

    pub(crate) fn k_patch_to_here(&mut self, list: i32) -> R<()> {
        let hr = self.k_get_label();
        self.k_patch_list(list, hr)
    }

    // ---- registers ---------------------------------------------------------------------------------

    pub(crate) fn k_check_stack(&mut self, n: i32) -> R<()> {
        let newstack = self.fsr().freereg + n;
        if newstack > i32::from(self.fsr().f.max_stack_size) {
            if newstack >= MAXREGS {
                return perr(
                    self.ls
                        .syntax_error("function or expression needs too many registers"),
                );
            }
            self.fs().f.max_stack_size = newstack as u8;
        }
        Ok(())
    }

    pub(crate) fn k_reserve_regs(&mut self, n: i32) -> R<()> {
        self.k_check_stack(n)?;
        self.fs().freereg += n;
        Ok(())
    }

    fn free_reg(&mut self, reg: i32) {
        if reg >= self.nvarstack_cur() {
            self.fs().freereg -= 1;
        }
    }

    fn free_regs(&mut self, r1: i32, r2: i32) {
        if r1 > r2 {
            self.free_reg(r1);
            self.free_reg(r2);
        } else {
            self.free_reg(r2);
            self.free_reg(r1);
        }
    }

    fn free_exp(&mut self, e: &E<H>) {
        if e.k == ExpKind::NonReloc {
            self.free_reg(e.info);
        }
    }

    fn free_exps(&mut self, e1: &E<H>, e2: &E<H>) {
        let r1 = if e1.k == ExpKind::NonReloc {
            e1.info
        } else {
            -1
        };
        let r2 = if e2.k == ExpKind::NonReloc {
            e2.info
        } else {
            -1
        };
        self.free_regs(r1, r2);
    }

    // ---- loading constants -------------------------------------------------------------------------

    pub(crate) fn k_int(&mut self, reg: i32, i: i64) -> R<()> {
        if fits_bx(i) {
            self.code_asbx(OpCode::Loadi, reg, i as i32)?;
        } else {
            let k = self.int_k(i) as i32;
            self.k_codek(reg, k)?;
        }
        Ok(())
    }

    fn k_float(&mut self, reg: i32, f: f64) -> R<()> {
        match number::float_to_int(f, F2I::Eq) {
            Some(fi) if fits_bx(fi) => {
                self.code_asbx(OpCode::Loadf, reg, fi as i32)?;
            }
            _ => {
                let k = self.number_k(f) as i32;
                self.k_codek(reg, k)?;
            }
        }
        Ok(())
    }

    // ---- result adjustment --------------------------------------------------------------------------

    pub(crate) fn k_set_returns(&mut self, e: &mut E<H>, nresults: i32) -> R<()> {
        let pc = e.info;
        if e.k == ExpKind::Call {
            self.modify(pc, |i| i.set_c((nresults + 1) as u32));
        } else {
            let fr = self.fsr().freereg;
            self.modify(pc, |i| {
                i.set_c((nresults + 1) as u32);
                i.set_a(fr as u32);
            });
            self.k_reserve_regs(1)?;
        }
        Ok(())
    }

    pub(crate) fn k_set_multret(&mut self, e: &mut E<H>) -> R<()> {
        self.k_set_returns(e, LUA_MULTRET)
    }

    fn str2k(&mut self, e: &mut E<H>) -> i32 {
        let s = e.strval.unwrap_or(self.ls.envn);
        e.info = self.string_k(s) as i32;
        e.k = ExpKind::K;
        e.info
    }

    pub(crate) fn k_set_one_ret(&mut self, e: &mut E<H>) {
        if e.k == ExpKind::Call {
            e.k = ExpKind::NonReloc;
            e.info = self.inst(e.info).a() as i32;
        } else if e.k == ExpKind::Vararg {
            self.modify(e.info, |i| i.set_c(2));
            e.k = ExpKind::Reloc;
        }
    }

    // ---- discharging expressions --------------------------------------------------------------------

    pub(crate) fn k_discharge_vars(&mut self, e: &mut E<H>) -> R<()> {
        match e.k {
            ExpKind::Const => {
                let v = self.const2val(e);
                Self::const2exp(&v, e);
            }
            ExpKind::Local => {
                e.info = e.var_ridx;
                e.k = ExpKind::NonReloc;
            }
            ExpKind::Upval => {
                e.info = self.k_code_abc(OpCode::Getupval, 0, e.info, 0)?;
                e.k = ExpKind::Reloc;
            }
            ExpKind::IndexUp => {
                e.info = self.k_code_abc(OpCode::Gettabup, 0, e.ind_t, e.ind_idx)?;
                e.k = ExpKind::Reloc;
            }
            ExpKind::IndexI => {
                self.free_reg(e.ind_t);
                e.info = self.k_code_abc(OpCode::Geti, 0, e.ind_t, e.ind_idx)?;
                e.k = ExpKind::Reloc;
            }
            ExpKind::IndexStr => {
                self.free_reg(e.ind_t);
                e.info = self.k_code_abc(OpCode::Getfield, 0, e.ind_t, e.ind_idx)?;
                e.k = ExpKind::Reloc;
            }
            ExpKind::Indexed => {
                self.free_regs(e.ind_t, e.ind_idx);
                e.info = self.k_code_abc(OpCode::Gettable, 0, e.ind_t, e.ind_idx)?;
                e.k = ExpKind::Reloc;
            }
            ExpKind::Vararg | ExpKind::Call => self.k_set_one_ret(e),
            _ => {}
        }
        Ok(())
    }

    fn discharge2reg(&mut self, e: &mut E<H>, reg: i32) -> R<()> {
        self.k_discharge_vars(e)?;
        match e.k {
            ExpKind::Nil => self.k_nil(reg, 1)?,
            ExpKind::False => {
                self.k_code_abc(OpCode::Loadfalse, reg, 0, 0)?;
            }
            ExpKind::True => {
                self.k_code_abc(OpCode::Loadtrue, reg, 0, 0)?;
            }
            ExpKind::KStr | ExpKind::K => {
                if e.k == ExpKind::KStr {
                    self.str2k(e);
                }
                self.k_codek(reg, e.info)?;
            }
            ExpKind::KFlt => self.k_float(reg, e.nval)?,
            ExpKind::KInt => self.k_int(reg, e.ival)?,
            ExpKind::Reloc => {
                self.modify(e.info, |i| i.set_a(reg as u32));
            }
            ExpKind::NonReloc => {
                if reg != e.info {
                    self.k_code_abc(OpCode::Move, reg, e.info, 0)?;
                }
            }
            _ => return Ok(()), // VJMP: nothing to do
        }
        e.info = reg;
        e.k = ExpKind::NonReloc;
        Ok(())
    }

    fn discharge2anyreg(&mut self, e: &mut E<H>) -> R<()> {
        if e.k != ExpKind::NonReloc {
            self.k_reserve_regs(1)?;
            let r = self.fsr().freereg - 1;
            self.discharge2reg(e, r)?;
        }
        Ok(())
    }

    fn code_loadbool(&mut self, a: i32, op: OpCode) -> R<i32> {
        self.k_get_label();
        self.k_code_abc(op, a, 0, 0)
    }

    fn need_value(&self, mut list: i32) -> bool {
        while list != NO_JUMP {
            let i = self.inst(self.get_jump_control(list));
            if i.opcode() != Some(OpCode::Testset) {
                return true;
            }
            list = self.get_jump(list);
        }
        false
    }

    fn exp2reg(&mut self, e: &mut E<H>, reg: i32) -> R<()> {
        self.discharge2reg(e, reg)?;
        if e.k == ExpKind::Jmp {
            let mut t = e.t;
            self.k_concat(&mut t, e.info)?;
            e.t = t;
        }
        if e.has_jumps() {
            let mut p_f = NO_JUMP;
            let mut p_t = NO_JUMP;
            if self.need_value(e.t) || self.need_value(e.f) {
                let fj = if e.k == ExpKind::Jmp {
                    NO_JUMP
                } else {
                    self.k_jump()?
                };
                p_f = self.code_loadbool(reg, OpCode::Lfalseskip)?;
                p_t = self.code_loadbool(reg, OpCode::Loadtrue)?;
                self.k_patch_to_here(fj)?;
            }
            let fin = self.k_get_label();
            self.patch_list_aux(e.f, fin, reg, p_f)?;
            self.patch_list_aux(e.t, fin, reg, p_t)?;
        }
        e.f = NO_JUMP;
        e.t = NO_JUMP;
        e.info = reg;
        e.k = ExpKind::NonReloc;
        Ok(())
    }

    pub(crate) fn k_exp2nextreg(&mut self, e: &mut E<H>) -> R<()> {
        self.k_discharge_vars(e)?;
        self.free_exp(e);
        self.k_reserve_regs(1)?;
        let r = self.fsr().freereg - 1;
        self.exp2reg(e, r)
    }

    pub(crate) fn k_exp2anyreg(&mut self, e: &mut E<H>) -> R<i32> {
        self.k_discharge_vars(e)?;
        if e.k == ExpKind::NonReloc {
            if !e.has_jumps() {
                return Ok(e.info);
            }
            if e.info >= self.nvarstack_cur() {
                let r = e.info;
                self.exp2reg(e, r)?;
                return Ok(e.info);
            }
        }
        self.k_exp2nextreg(e)?;
        Ok(e.info)
    }

    pub(crate) fn k_exp2anyregup(&mut self, e: &mut E<H>) -> R<()> {
        if e.k != ExpKind::Upval || e.has_jumps() {
            self.k_exp2anyreg(e)?;
        }
        Ok(())
    }

    pub(crate) fn k_exp2val(&mut self, e: &mut E<H>) -> R<()> {
        if e.k == ExpKind::Jmp || e.has_jumps() {
            self.k_exp2anyreg(e)?;
            Ok(())
        } else {
            self.k_discharge_vars(e)
        }
    }

    fn k_exp2k(&mut self, e: &mut E<H>) -> bool {
        if !e.has_jumps() {
            let info = match e.k {
                ExpKind::True => self.bool_t(),
                ExpKind::False => self.bool_f(),
                ExpKind::Nil => self.nil_k(),
                ExpKind::KInt => self.int_k(e.ival),
                ExpKind::KFlt => self.number_k(e.nval),
                ExpKind::KStr => {
                    let s = e.strval.unwrap_or(self.ls.envn);
                    self.string_k(s)
                }
                ExpKind::K => e.info as usize,
                _ => return false,
            };
            if info as i32 <= MAXINDEXRK {
                e.k = ExpKind::K;
                e.info = info as i32;
                return true;
            }
        }
        false
    }

    fn exp2rk(&mut self, e: &mut E<H>) -> R<bool> {
        if self.k_exp2k(e) {
            Ok(true)
        } else {
            self.k_exp2anyreg(e)?;
            Ok(false)
        }
    }

    fn code_abrk(&mut self, o: OpCode, a: i32, b: i32, ec: &mut E<H>) -> R<()> {
        let k = self.exp2rk(ec)?;
        self.k_code_abck(o, a, b, ec.info, i32::from(k))?;
        Ok(())
    }

    pub(crate) fn k_storevar(&mut self, var: &mut E<H>, ex: &mut E<H>) -> R<()> {
        match var.k {
            ExpKind::Local => {
                self.free_exp(ex);
                self.exp2reg(ex, var.var_ridx)?;
                return Ok(());
            }
            ExpKind::Upval => {
                let e = self.k_exp2anyreg(ex)?;
                self.k_code_abc(OpCode::Setupval, e, var.info, 0)?;
            }
            ExpKind::IndexUp => self.code_abrk(OpCode::Settabup, var.ind_t, var.ind_idx, ex)?,
            ExpKind::IndexI => self.code_abrk(OpCode::Seti, var.ind_t, var.ind_idx, ex)?,
            ExpKind::IndexStr => self.code_abrk(OpCode::Setfield, var.ind_t, var.ind_idx, ex)?,
            ExpKind::Indexed => self.code_abrk(OpCode::Settable, var.ind_t, var.ind_idx, ex)?,
            _ => {}
        }
        self.free_exp(ex);
        Ok(())
    }

    fn negate_condition(&mut self, e: &E<H>) {
        let pc = self.get_jump_control(e.info);
        self.modify(pc, |i| i.set_k(i.k() ^ 1));
    }

    fn jump_on_cond(&mut self, e: &mut E<H>, cond: i32) -> R<i32> {
        if e.k == ExpKind::Reloc {
            let ie = self.inst(e.info);
            if ie.opcode() == Some(OpCode::Not) {
                self.remove_last_instruction();
                return self.cond_jump(OpCode::Test, ie.b() as i32, 0, 0, i32::from(cond == 0));
            }
        }
        self.discharge2anyreg(e)?;
        self.free_exp(e);
        self.cond_jump(OpCode::Testset, NO_REG, e.info, 0, cond)
    }

    pub(crate) fn k_go_if_true(&mut self, e: &mut E<H>) -> R<()> {
        self.k_discharge_vars(e)?;
        let pc = match e.k {
            ExpKind::Jmp => {
                self.negate_condition(e);
                e.info
            }
            ExpKind::K | ExpKind::KFlt | ExpKind::KInt | ExpKind::KStr | ExpKind::True => NO_JUMP,
            _ => self.jump_on_cond(e, 0)?,
        };
        let mut f = e.f;
        self.k_concat(&mut f, pc)?;
        e.f = f;
        self.k_patch_to_here(e.t)?;
        e.t = NO_JUMP;
        Ok(())
    }

    pub(crate) fn k_go_if_false(&mut self, e: &mut E<H>) -> R<()> {
        self.k_discharge_vars(e)?;
        let pc = match e.k {
            ExpKind::Jmp => e.info,
            ExpKind::Nil | ExpKind::False => NO_JUMP,
            _ => self.jump_on_cond(e, 1)?,
        };
        let mut t = e.t;
        self.k_concat(&mut t, pc)?;
        e.t = t;
        self.k_patch_to_here(e.f)?;
        e.f = NO_JUMP;
        Ok(())
    }

    fn code_not(&mut self, e: &mut E<H>) -> R<()> {
        match e.k {
            ExpKind::Nil | ExpKind::False => e.k = ExpKind::True,
            ExpKind::K | ExpKind::KFlt | ExpKind::KInt | ExpKind::KStr | ExpKind::True => {
                e.k = ExpKind::False
            }
            ExpKind::Jmp => self.negate_condition(e),
            ExpKind::Reloc | ExpKind::NonReloc => {
                self.discharge2anyreg(e)?;
                self.free_exp(e);
                e.info = self.k_code_abc(OpCode::Not, 0, e.info, 0)?;
                e.k = ExpKind::Reloc;
            }
            _ => {}
        }
        std::mem::swap(&mut e.f, &mut e.t);
        self.remove_values(e.f);
        self.remove_values(e.t);
        Ok(())
    }

    // ---- indexing, self -------------------------------------------------------------------------------

    fn is_kstr(&self, e: &E<H>) -> bool {
        e.k == ExpKind::K
            && !e.has_jumps()
            && e.info <= MAXINDEXRK
            && matches!(
                self.fsr().f.k.get(e.info as usize),
                Some(Const::Str(s)) if self.ls.host.bytes(*s).len() <= LUAI_MAXSHORTLEN
            )
    }

    fn is_kint(e: &E<H>) -> bool {
        e.k == ExpKind::KInt && !e.has_jumps()
    }

    fn is_cint(e: &E<H>) -> bool {
        Self::is_kint(e) && (e.ival as u64) <= MAXARG_C as u64
    }

    fn is_scint(e: &E<H>) -> bool {
        Self::is_kint(e) && fits_c(e.ival)
    }

    /// `isSCnumber`: returns the encoded immediate if `e` is a small numeral. Like the C code it sets
    /// `*isfloat` as soon as it sees an integral float, even if the value then fails to fit.
    fn is_sc_number(e: &E<H>, isfloat: &mut bool) -> Option<i32> {
        let i = if e.k == ExpKind::KInt {
            e.ival
        } else if e.k == ExpKind::KFlt {
            {
                let i = number::float_to_int(e.nval, F2I::Eq)?;
                *isfloat = true;
                i
            }
        } else {
            return None;
        };
        if !e.has_jumps() && fits_c(i) {
            Some(i as i32 + OFFSET_SC)
        } else {
            None
        }
    }

    pub(crate) fn k_self(&mut self, e: &mut E<H>, key: &mut E<H>) -> R<()> {
        self.k_exp2anyreg(e)?;
        let ereg = e.info;
        self.free_exp(e);
        e.info = self.fsr().freereg;
        e.k = ExpKind::NonReloc;
        self.k_reserve_regs(2)?;
        self.code_abrk(OpCode::SelfOp, e.info, ereg, key)?;
        self.free_exp(key);
        Ok(())
    }

    pub(crate) fn k_indexed(&mut self, t: &mut E<H>, k: &mut E<H>) -> R<()> {
        if k.k == ExpKind::KStr {
            self.str2k(k);
        }
        if t.k == ExpKind::Upval && !self.is_kstr(k) {
            self.k_exp2anyreg(t)?;
        }
        if t.k == ExpKind::Upval {
            t.ind_t = t.info;
            t.ind_idx = k.info & 0xff;
            t.k = ExpKind::IndexUp;
        } else {
            t.ind_t = if t.k == ExpKind::Local {
                t.var_ridx
            } else {
                t.info
            };
            if self.is_kstr(k) {
                t.ind_idx = k.info & 0xff;
                t.k = ExpKind::IndexStr;
            } else if Self::is_cint(k) {
                t.ind_idx = k.ival as i32;
                t.k = ExpKind::IndexI;
            } else {
                let r = self.k_exp2anyreg(k)?;
                t.ind_idx = r & 0xff;
                t.k = ExpKind::Indexed;
            }
        }
        Ok(())
    }

    // ---- operators ------------------------------------------------------------------------------------------

    fn const_folding(&mut self, op: i32, e1: &mut E<H>, e2: &E<H>) -> bool {
        let (Some(v1), Some(v2)) = (Self::tonumeral(e1), Self::tonumeral(e2)) else {
            return false;
        };
        if !valid_op(op, v1, v2) {
            return false;
        }
        match raw_arith(op, v1, v2) {
            Number::Int(i) => {
                e1.k = ExpKind::KInt;
                e1.ival = i;
            }
            Number::Float(n) => {
                if n.is_nan() || n == 0.0 {
                    return false;
                }
                e1.k = ExpKind::KFlt;
                e1.nval = n;
            }
        }
        true
    }

    fn code_unexpval(&mut self, op: OpCode, e: &mut E<H>, line: i32) -> R<()> {
        let r = self.k_exp2anyreg(e)?;
        self.free_exp(e);
        e.info = self.k_code_abc(op, 0, r, 0)?;
        e.k = ExpKind::Reloc;
        self.k_fix_line(line)
    }

    #[allow(clippy::too_many_arguments)]
    fn finish_binexpval(
        &mut self,
        e1: &mut E<H>,
        e2: &mut E<H>,
        op: OpCode,
        v2: i32,
        flip: i32,
        line: i32,
        mmop: OpCode,
        event: i32,
    ) -> R<()> {
        let v1 = self.k_exp2anyreg(e1)?;
        let pc = self.k_code_abck(op, 0, v1, v2, 0)?;
        self.free_exps(e1, e2);
        e1.info = pc;
        e1.k = ExpKind::Reloc;
        self.k_fix_line(line)?;
        self.k_code_abck(mmop, v1, v2, event, flip)?;
        self.k_fix_line(line)
    }

    fn code_binexpval(&mut self, opr: BinOpr, e1: &mut E<H>, e2: &mut E<H>, line: i32) -> R<()> {
        let op = op_of(opr as i32 - BinOpr::Add as i32 + OpCode::Add as i32);
        let v2 = self.k_exp2anyreg(e2)?;
        self.finish_binexpval(
            e1,
            e2,
            op,
            v2,
            0,
            line,
            OpCode::Mmbin,
            opr as i32 - BinOpr::Add as i32 + TM_ADD,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn code_bini(
        &mut self,
        op: OpCode,
        e1: &mut E<H>,
        e2: &mut E<H>,
        flip: i32,
        line: i32,
        event: i32,
    ) -> R<()> {
        let v2 = e2.ival as i32 + OFFSET_SC;
        self.finish_binexpval(e1, e2, op, v2, flip, line, OpCode::Mmbini, event)
    }

    fn code_bink(
        &mut self,
        opr: BinOpr,
        e1: &mut E<H>,
        e2: &mut E<H>,
        flip: i32,
        line: i32,
    ) -> R<()> {
        let event = opr as i32 - BinOpr::Add as i32 + TM_ADD;
        let v2 = e2.info;
        let op = op_of(opr as i32 - BinOpr::Add as i32 + OpCode::Addk as i32);
        self.finish_binexpval(e1, e2, op, v2, flip, line, OpCode::Mmbink, event)
    }

    fn finish_binexpneg(
        &mut self,
        e1: &mut E<H>,
        e2: &mut E<H>,
        op: OpCode,
        line: i32,
        event: i32,
    ) -> R<bool> {
        if !Self::is_kint(e2) {
            return Ok(false);
        }
        let i2 = e2.ival;
        if !(fits_c(i2) && fits_c(i2.wrapping_neg())) {
            return Ok(false);
        }
        let v2 = i2 as i32;
        self.finish_binexpval(e1, e2, op, -v2 + OFFSET_SC, 0, line, OpCode::Mmbini, event)?;
        let pc = self.pc() - 1;
        self.modify(pc, |i| i.set_b((v2 + OFFSET_SC) as u32));
        Ok(true)
    }

    fn code_bin_nok(
        &mut self,
        opr: BinOpr,
        e1: &mut E<H>,
        e2: &mut E<H>,
        flip: i32,
        line: i32,
    ) -> R<()> {
        if flip != 0 {
            std::mem::swap(e1, e2);
        }
        self.code_binexpval(opr, e1, e2, line)
    }

    fn code_arith(
        &mut self,
        opr: BinOpr,
        e1: &mut E<H>,
        e2: &mut E<H>,
        flip: i32,
        line: i32,
    ) -> R<()> {
        if Self::is_numeral(e2) && self.k_exp2k(e2) {
            self.code_bink(opr, e1, e2, flip, line)
        } else {
            self.code_bin_nok(opr, e1, e2, flip, line)
        }
    }

    fn code_commutative(&mut self, op: BinOpr, e1: &mut E<H>, e2: &mut E<H>, line: i32) -> R<()> {
        let mut flip = 0;
        if Self::is_numeral(e1) {
            std::mem::swap(e1, e2);
            flip = 1;
        }
        if op == BinOpr::Add && Self::is_scint(e2) {
            self.code_bini(OpCode::Addi, e1, e2, flip, line, TM_ADD)
        } else {
            self.code_arith(op, e1, e2, flip, line)
        }
    }

    fn code_bitwise(&mut self, opr: BinOpr, e1: &mut E<H>, e2: &mut E<H>, line: i32) -> R<()> {
        let mut flip = 0;
        if e1.k == ExpKind::KInt {
            std::mem::swap(e1, e2);
            flip = 1;
        }
        if e2.k == ExpKind::KInt && self.k_exp2k(e2) {
            self.code_bink(opr, e1, e2, flip, line)
        } else {
            self.code_bin_nok(opr, e1, e2, flip, line)
        }
    }

    fn code_order(&mut self, opr: BinOpr, e1: &mut E<H>, e2: &mut E<H>) -> R<()> {
        let (r1, r2, op);
        let mut isfloat = false;
        if let Some(im) = Self::is_sc_number(e2, &mut isfloat) {
            r1 = self.k_exp2anyreg(e1)?;
            r2 = im;
            op = op_of(opr as i32 - BinOpr::Lt as i32 + OpCode::Lti as i32);
        } else if let Some(im) = Self::is_sc_number(e1, &mut isfloat) {
            r1 = self.k_exp2anyreg(e2)?;
            r2 = im;
            op = op_of(opr as i32 - BinOpr::Lt as i32 + OpCode::Gti as i32);
        } else {
            r1 = self.k_exp2anyreg(e1)?;
            r2 = self.k_exp2anyreg(e2)?;
            op = op_of(opr as i32 - BinOpr::Lt as i32 + OpCode::Lt as i32);
        }
        self.free_exps(e1, e2);
        e1.info = self.cond_jump(op, r1, r2, i32::from(isfloat), 1)?;
        e1.k = ExpKind::Jmp;
        Ok(())
    }

    fn code_eq(&mut self, opr: BinOpr, e1: &mut E<H>, e2: &mut E<H>) -> R<()> {
        if e1.k != ExpKind::NonReloc {
            std::mem::swap(e1, e2);
        }
        let r1 = self.k_exp2anyreg(e1)?;
        let (op, r2);
        let mut isfloat = false;
        if let Some(im) = Self::is_sc_number(e2, &mut isfloat) {
            op = OpCode::Eqi;
            r2 = im;
        } else if self.exp2rk(e2)? {
            op = OpCode::Eqk;
            r2 = e2.info;
        } else {
            op = OpCode::Eq;
            r2 = self.k_exp2anyreg(e2)?;
        }
        self.free_exps(e1, e2);
        e1.info = self.cond_jump(op, r1, r2, i32::from(isfloat), i32::from(opr == BinOpr::Eq))?;
        e1.k = ExpKind::Jmp;
        Ok(())
    }

    pub(crate) fn k_prefix(&mut self, opr: UnOpr, e: &mut E<H>, line: i32) -> R<()> {
        let ef = {
            let mut x = E::<H>::new(ExpKind::KInt, 0);
            x.ival = 0;
            x
        };
        self.k_discharge_vars(e)?;
        match opr {
            UnOpr::Minus | UnOpr::BNot => {
                let luaop = opr as i32 + 12;
                if self.const_folding(luaop, e, &ef) {
                    return Ok(());
                }
                let op = op_of(opr as i32 + OpCode::Unm as i32);
                self.code_unexpval(op, e, line)
            }
            UnOpr::Len => self.code_unexpval(OpCode::Len, e, line),
            UnOpr::Not => self.code_not(e),
            UnOpr::NoUnOpr => Ok(()),
        }
    }

    pub(crate) fn k_infix(&mut self, op: BinOpr, v: &mut E<H>) -> R<()> {
        self.k_discharge_vars(v)?;
        match op {
            BinOpr::And => self.k_go_if_true(v)?,
            BinOpr::Or => self.k_go_if_false(v)?,
            BinOpr::Concat => self.k_exp2nextreg(v)?,
            BinOpr::Add
            | BinOpr::Sub
            | BinOpr::Mul
            | BinOpr::Div
            | BinOpr::IDiv
            | BinOpr::Mod
            | BinOpr::Pow
            | BinOpr::BAnd
            | BinOpr::BOr
            | BinOpr::BXor
            | BinOpr::Shl
            | BinOpr::Shr => {
                if !Self::is_numeral(v) {
                    self.k_exp2anyreg(v)?;
                }
            }
            BinOpr::Eq | BinOpr::Ne => {
                if !Self::is_numeral(v) {
                    self.exp2rk(v)?;
                }
            }
            BinOpr::Lt | BinOpr::Le | BinOpr::Gt | BinOpr::Ge => {
                let mut dummy = false;
                if Self::is_sc_number(v, &mut dummy).is_none() {
                    self.k_exp2anyreg(v)?;
                }
            }
            BinOpr::NoBinOpr => {}
        }
        Ok(())
    }

    fn code_concat(&mut self, e1: &mut E<H>, e2: &mut E<H>, line: i32) -> R<()> {
        if let Some(prev) = self.previous_instruction() {
            let ie2 = self.inst(prev);
            if ie2.opcode() == Some(OpCode::Concat) {
                let n = ie2.b();
                self.free_exp(e2);
                let a = e1.info as u32;
                self.modify(prev, |i| {
                    i.set_a(a);
                    i.set_b(n + 1);
                });
                return Ok(());
            }
        }
        self.k_code_abc(OpCode::Concat, e1.info, 2, 0)?;
        self.free_exp(e2);
        self.k_fix_line(line)
    }

    pub(crate) fn k_posfix(
        &mut self,
        mut opr: BinOpr,
        e1: &mut E<H>,
        e2: &mut E<H>,
        line: i32,
    ) -> R<()> {
        self.k_discharge_vars(e2)?;
        if (opr as i32) <= BinOpr::Shr as i32 && self.const_folding(opr as i32, e1, e2) {
            return Ok(());
        }
        match opr {
            BinOpr::And => {
                let mut f = e2.f;
                self.k_concat(&mut f, e1.f)?;
                e2.f = f;
                *e1 = *e2;
            }
            BinOpr::Or => {
                let mut t = e2.t;
                self.k_concat(&mut t, e1.t)?;
                e2.t = t;
                *e1 = *e2;
            }
            BinOpr::Concat => {
                self.k_exp2nextreg(e2)?;
                self.code_concat(e1, e2, line)?;
            }
            BinOpr::Add | BinOpr::Mul => self.code_commutative(opr, e1, e2, line)?,
            BinOpr::Sub => {
                if !self.finish_binexpneg(e1, e2, OpCode::Addi, line, TM_SUB)? {
                    self.code_arith(opr, e1, e2, 0, line)?;
                }
            }
            BinOpr::Div | BinOpr::IDiv | BinOpr::Mod | BinOpr::Pow => {
                self.code_arith(opr, e1, e2, 0, line)?
            }
            BinOpr::BAnd | BinOpr::BOr | BinOpr::BXor => self.code_bitwise(opr, e1, e2, line)?,
            BinOpr::Shl => {
                if Self::is_scint(e1) {
                    std::mem::swap(e1, e2);
                    self.code_bini(OpCode::Shli, e1, e2, 1, line, TM_SHL)?;
                } else if !self.finish_binexpneg(e1, e2, OpCode::Shri, line, TM_SHL)? {
                    self.code_binexpval(opr, e1, e2, line)?;
                }
            }
            BinOpr::Shr => {
                if Self::is_scint(e2) {
                    self.code_bini(OpCode::Shri, e1, e2, 0, line, TM_SHR)?;
                } else {
                    self.code_binexpval(opr, e1, e2, line)?;
                }
            }
            BinOpr::Eq | BinOpr::Ne => self.code_eq(opr, e1, e2)?,
            BinOpr::Gt | BinOpr::Ge => {
                std::mem::swap(e1, e2);
                opr = BinOpr::from_i32(opr as i32 - BinOpr::Gt as i32 + BinOpr::Lt as i32);
                self.code_order(opr, e1, e2)?;
            }
            BinOpr::Lt | BinOpr::Le => self.code_order(opr, e1, e2)?,
            BinOpr::NoBinOpr => {}
        }
        Ok(())
    }

    pub(crate) fn k_fix_line(&mut self, line: i32) -> R<()> {
        self.remove_last_line_info();
        self.save_line_info(line);
        Ok(())
    }

    pub(crate) fn k_set_table_size(&mut self, pc: i32, ra: i32, asize: i32, hsize: i32) {
        let rb = if hsize != 0 {
            ceil_log2(hsize as u32) as i32 + 1
        } else {
            0
        };
        let extra = asize / (MAXARG_C as i32 + 1);
        let rc = asize % (MAXARG_C as i32 + 1);
        let k = i32::from(extra > 0);
        self.set_inst(
            pc,
            Instruction::abck(OpCode::Newtable, ra as u32, rb as u32, rc as u32, k as u32),
        );
        self.set_inst(pc + 1, Instruction::ax_(OpCode::Extraarg, extra as u32));
    }

    pub(crate) fn k_set_list(&mut self, base: i32, mut nelems: i32, mut tostore: i32) -> R<()> {
        if tostore == LUA_MULTRET {
            tostore = 0;
        }
        if nelems <= MAXARG_C as i32 {
            self.k_code_abc(OpCode::Setlist, base, tostore, nelems)?;
        } else {
            let extra = nelems / (MAXARG_C as i32 + 1);
            nelems %= MAXARG_C as i32 + 1;
            self.k_code_abck(OpCode::Setlist, base, tostore, nelems, 1)?;
            self.code_extra_arg(extra)?;
        }
        self.fs().freereg = base + 1;
        Ok(())
    }

    fn final_target(&self, mut i: i32) -> i32 {
        for _ in 0..100 {
            let pc = self.inst(i);
            if pc.opcode() != Some(OpCode::Jmp) {
                break;
            }
            i += pc.sj() + 1;
        }
        i
    }

    /// `luaK_finish`
    pub(crate) fn k_finish(&mut self) -> R<()> {
        use rn_core::protodata::PF_VAHID;
        let n = self.pc();
        for i in 0..n {
            let ins = self.inst(i);
            let is_vararg = self.fsr().f.flags & PF_VAHID != 0;
            let needclose = self.fsr().needclose;
            let np = self.fsr().f.num_params;
            match ins.opcode() {
                Some(
                    op @ (OpCode::Return0 | OpCode::Return1 | OpCode::Return | OpCode::Tailcall),
                ) => {
                    if matches!(op, OpCode::Return0 | OpCode::Return1) {
                        if !(needclose || is_vararg) {
                            continue;
                        }
                        self.modify(i, |x| x.set_opcode(OpCode::Return));
                    }
                    if needclose {
                        self.modify(i, |x| x.set_k(1));
                    }
                    if is_vararg {
                        self.modify(i, |x| x.set_c(u32::from(np) + 1));
                    }
                }
                Some(OpCode::Jmp) => {
                    let target = self.final_target(i);
                    self.fix_jump(i, target)?;
                }
                _ => {}
            }
        }
        Ok(())
    }

    // ---- hooks shared with the parser ---------------------------------------------------------------------------

    pub(crate) fn sem_err(&mut self, msg: &str) -> CompileError {
        self.sem_error(msg)
    }
}
