//! V1: plain template compiler. Every bytecode loads its operands from, and stores its result to, the
//! Value stack in memory (rbx = base, r14 = kcache); type tags are guarded per bytecode.
//!
//! Tag stores are full 8-byte tag-word stores (`mov qword [slot], tag`), like `Value::store`; tag guards are
//! `cmp byte [slot], tag` (hand-encoded: the encoder has no `cmp m8, imm8`).

use crate::decode::{Ar, Func, Kv, Op, Rel, Src};
use rn_core::jit_layout::{self as lay, ProtoLayout};
use rn_core::value::{
    ctb, LUA_VFALSE, LUA_VLCL, LUA_VNUMFLT, LUA_VNUMINT, LUA_VSHRSTR, LUA_VTABLE,
};
use rn_jit::arch::x86_64::*;

pub const INT: i32 = LUA_VNUMINT as i32;
pub const FLT: i32 = LUA_VNUMFLT as i32;

/// Layout of the oracle's "state" block pointed to by r13 (stand-in for `State`/`CallInfo`).
#[repr(C)]
pub struct St {
    /// Trap flag word (back-edge poll: `cmp dword [r13], 0; jne`).
    pub trap: u32,
    pub _pad: u32,
    /// Instruction budget counter (block head: `sub qword [r13+8], n; js`).
    pub count: i64,
    /// Written by abort stubs: pc+1 of the failing bytecode.
    pub abort_pc: u32,
    /// Written by abort stubs: index of the function that aborted.
    pub abort_fn: u32,
    /// Current `CallInfo` (stand-in for `State::ci`); only used by the call programs.
    pub ci: *mut u8,
    /// Stack top as an index (stand-in for `State::top`).
    pub top: usize,
    /// Stack size in slots (stand-in for `State::stack_size`).
    pub stack_size: usize,
    /// Address of stack slot 0 (stand-in for the thread's stack pointer).
    pub stack_base: *mut u8,
}
pub const ST_COUNT: i32 = 8;
pub const ST_ABORT: i32 = 16;
pub const ST_ABORT_FN: i32 = 20;
pub const ST_CI: i32 = 24;
pub const ST_TOP: i32 = 32;
pub const ST_STACK_SIZE: i32 = 40;
pub const ST_STACK_BASE: i32 = 48;

/// Tag bytes of collectable object values (`ctb`).
pub const TAG_TABLE: u8 = ctb(LUA_VTABLE);
pub const TAG_SHRSTR: u8 = ctb(LUA_VSHRSTR);
pub const TAG_LCL: u8 = ctb(LUA_VLCL);
pub const TAG_FALSE: u8 = LUA_VFALSE;
/// Offset of the resume table inside a `JitProto` (`[entry: u64][resume addresses, one per pc]`).
pub const JP_RESUME: i32 = 8;

#[derive(Clone, Copy, Default)]
pub struct Opts {
    /// Emit the back-edge trap poll (and, in call mode, the per-call `ci.trap` checks).
    pub poll: bool,
    /// Emit the per-basic-block instruction counter.
    pub count: bool,
}

/// Compile-time configuration of the State-based programs.
#[derive(Clone, Copy)]
pub struct Cfg {
    /// Flat-call protocol (E2): CALL pushes a `CallInfo` and jumps to the callee, RETURN pops and jumps back
    /// through the resume table. Off: RETURN1 leaves the oracle (register-only programs).
    pub call_mode: bool,
    /// Offsets of the proto's code/k data pointers.
    pub pl: ProtoLayout,
}

/// Result of compiling several functions into one code block.
pub struct Multi {
    pub code: Vec<u8>,
    /// Code offset of each function's entry.
    pub entries: Vec<usize>,
    /// Per function: (pc, code offset) of every call-return site.
    pub resumes: Vec<Vec<(usize, usize)>>,
}

pub struct Compiled {
    pub code: Vec<u8>,
    /// Register named by RETURN1; V1 leaves its tag in frame slot `ret_reg`.
    pub ret_reg: u32,
    /// Result tag when known statically (V2 keeps no tags in memory).
    pub ret_ty: Option<i32>,
}

#[derive(Clone, Copy, PartialEq)]
enum T {
    Dyn,
    Int,
    Flt,
    Any,
}

impl T {
    fn int_ok(self) -> bool {
        self != T::Flt
    }
    fn flt_ok(self) -> bool {
        self != T::Int
    }
}

pub fn slot(r: u32) -> Mem {
    Mem::base_disp(RBX, 16 * r as i32)
}
pub fn pay(r: u32) -> Mem {
    Mem::base_disp(RBX, 16 * r as i32 + 8)
}
pub fn kslot(k: u32) -> Mem {
    Mem::base_disp(R14, 16 * k as i32)
}
pub fn kpay(k: u32) -> Mem {
    Mem::base_disp(R14, 16 * k as i32 + 8)
}

/// `cmp byte [rbx + off], tag`
fn cmp_tag(a: &mut Assembler, off: i32, tag: i32) {
    if (-128..128).contains(&off) {
        a.emit_bytes(&[0x80, 0x7B, off as i8 as u8, tag as u8]);
    } else {
        a.emit_bytes(&[0x80, 0xBB]);
        a.emit_bytes(&off.to_le_bytes());
        a.emit_bytes(&[tag as u8]);
    }
}

pub fn invert(cc: Cond) -> Cond {
    Cond::ALL[(cc as usize) ^ 1]
}

pub fn int_cc(rel: Rel) -> Cond {
    match rel {
        Rel::Eq => Cond::E,
        Rel::Lt => Cond::L,
        Rel::Le => Cond::Le,
        Rel::Gt => Cond::G,
        Rel::Ge => Cond::Ge,
    }
}

/// Emits `ucomisd` + jumps so that control goes to `tgt` iff `(xa rel xb) == jump_if_true`; otherwise falls
/// through. NaN operands make every relation except `~=` false (as in Lua).
pub fn fjump(
    a: &mut Assembler,
    rel: Rel,
    jump_if_true: bool,
    xa: Xmm,
    xb: Xmm,
    tgt: Label,
    skip: Label,
) {
    // For `a < b` / `a <= b` compare (b, a) so that "above" means "b > a" and unordered (CF=1) fails.
    let (l, r) = match rel {
        Rel::Lt | Rel::Le => (xb, xa),
        _ => (xa, xb),
    };
    a.ucomisd_rr(l, r);
    match (rel, jump_if_true) {
        (Rel::Lt | Rel::Gt, true) => a.jcc_label(Cond::A, tgt),
        (Rel::Le | Rel::Ge, true) => a.jcc_label(Cond::Ae, tgt),
        (Rel::Lt | Rel::Gt, false) => a.jcc_label(Cond::Be, tgt),
        (Rel::Le | Rel::Ge, false) => a.jcc_label(Cond::B, tgt),
        (Rel::Eq, true) => {
            a.jcc_label(Cond::P, skip);
            a.jcc_label(Cond::E, tgt);
        }
        (Rel::Eq, false) => {
            a.jcc_label(Cond::P, tgt);
            a.jcc_label(Cond::Ne, tgt);
        }
    }
}

pub fn prologue(a: &mut Assembler) {
    for r in [RBP, RBX, R12, R13, R14, R15] {
        a.push(r);
    }
}

pub fn epilogue(a: &mut Assembler) {
    for r in [R15, R14, R13, R12, RBX, RBP] {
        a.pop(r);
    }
    a.ret();
}

/// `rax = rax // rcx` (or `rax % rcx`) with Lua semantics (floor, `-1` divisor special case); `rcx` holds the
/// divisor, `const_div` is its value when known at compile time. Clobbers rdx. Zero divisors jump to `ab`.
pub fn idiv_core(a: &mut Assembler, is_idiv: bool, const_div: Option<i64>, ab: Label) {
    let lneg = a.new_label();
    let done = a.new_label();
    let mut need_neg = true;
    match const_div {
        Some(0) => {
            a.jmp_label(ab);
            return;
        }
        Some(-1) => {}
        Some(_) => need_neg = false,
        None => {
            a.test_rr(Size::S64, RCX, RCX);
            a.jcc_label(Cond::E, ab);
        }
    }
    if need_neg {
        a.alu_ri(Alu::Cmp, Size::S64, RCX, -1);
        a.jcc_label(Cond::E, lneg);
    }
    a.cqo();
    a.idiv(Size::S64, RCX);
    a.test_rr(Size::S64, RDX, RDX);
    a.jcc_label(Cond::E, done);
    if is_idiv {
        a.alu_rr(Alu::Xor, Size::S64, RDX, RCX);
        a.jcc_label(Cond::Ns, done);
        a.dec(Size::S64, RAX);
    } else {
        a.mov_rr(Size::S64, RAX, RDX);
        a.alu_rr(Alu::Xor, Size::S64, RAX, RCX);
        a.jcc_label(Cond::Ns, done);
        a.alu_rr(Alu::Add, Size::S64, RDX, RCX);
    }
    a.bind(done);
    if !is_idiv {
        a.mov_rr(Size::S64, RAX, RDX);
    }
    if need_neg {
        let fin = a.new_label();
        a.jmp_label(fin);
        a.bind(lneg);
        if is_idiv {
            a.neg(Size::S64, RAX);
        } else {
            a.alu_rr(Alu::Xor, Size::S32, RAX, RAX);
        }
        a.bind(fin);
    } else {
        a.bind(lneg);
    }
}

/// `rax = shiftl(rax, y)` (`left`) or `shiftl(rax, -y)` where `y` is `const_y` or `rcx`: Lua shift semantics
/// (logical, counts of 64 or more give 0, negative counts reverse the direction). Clobbers rcx.
pub fn shift_core(a: &mut Assembler, left: bool, const_y: Option<i64>) {
    if let Some(y) = const_y {
        let y = if left { y } else { y.wrapping_neg() };
        if y.unsigned_abs() >= 64 {
            a.alu_rr(Alu::Xor, Size::S32, RAX, RAX);
        } else if y >= 0 {
            a.shift_ri(Shift::Shl, Size::S64, RAX, y as u8);
        } else {
            a.shift_ri(Shift::Shr, Size::S64, RAX, (-y) as u8);
        }
        return;
    }
    if !left {
        a.neg(Size::S64, RCX);
    }
    let lshl = a.new_label();
    let lzero = a.new_label();
    let done = a.new_label();
    a.alu_ri(Alu::Cmp, Size::S64, RCX, 63);
    a.jcc_label(Cond::Be, lshl);
    a.neg(Size::S64, RCX);
    a.alu_ri(Alu::Cmp, Size::S64, RCX, 63);
    a.jcc_label(Cond::A, lzero);
    a.shift_cl(Shift::Shr, Size::S64, RAX);
    a.jmp_label(done);
    a.bind(lshl);
    a.shift_cl(Shift::Shl, Size::S64, RAX);
    a.jmp_label(done);
    a.bind(lzero);
    a.alu_rr(Alu::Xor, Size::S32, RAX, RAX);
    a.bind(done);
}

#[derive(Clone, Copy)]
pub enum KeyOp {
    /// Register holding an integer key.
    R(u32),
    /// 1-based constant index (GETI/SETI).
    Imm(u32),
}
use KeyOp as Key;

struct G<'a> {
    f: &'a Func,
    a: &'a mut Assembler,
    lab: Vec<Label>,
    aborts: Vec<Option<Label>>,
    epi: Label,
    o: Opts,
    cfg: Cfg,
    fi: usize,
}

pub fn compile(f: &Func, o: Opts) -> Compiled {
    let cfg = Cfg {
        call_mode: false,
        pl: ProtoLayout {
            code_ptr: 0,
            k_ptr: 0,
        },
    };
    let (m, ret_reg) = compile_inner(&[f], o, cfg);
    Compiled {
        code: m.code,
        ret_reg,
        ret_ty: None,
    }
}

/// Compiles several functions (the State-based programs) into one block; the entry thunk jumps into
/// function 0.
pub fn compile_multi(funcs: &[&Func], o: Opts, cfg: Cfg) -> Multi {
    compile_inner(funcs, o, cfg).0
}

fn compile_inner(funcs: &[&Func], o: Opts, cfg: Cfg) -> (Multi, u32) {
    let mut a = Assembler::new();
    let epi = a.new_label();
    prologue(&mut a);
    a.mov_rr(Size::S64, RBX, RDI);
    a.mov_rr(Size::S64, R14, RSI);
    a.mov_rr(Size::S64, R13, RDX);
    if cfg.call_mode {
        a.mov_rm(Size::S64, R12, Mem::base_disp(R13, ST_CI));
    }
    let mut ret_reg = 0;
    let mut entries = Vec::new();
    let mut resumes = Vec::new();
    for (fi, f) in funcs.iter().enumerate() {
        let n = f.ops.len();
        let lab: Vec<Label> = (0..=n).map(|_| a.new_label()).collect();
        let mut g = G {
            f,
            a: &mut a,
            lab,
            aborts: vec![None; n + 1],
            epi,
            o,
            cfg,
            fi,
        };
        let leaders = f.leaders();
        let mut res = Vec::new();
        for pc in 0..n {
            g.a.bind(g.lab[pc]);
            if pc == 0 {
                entries.push(g.a.offset());
                if cfg.call_mode && o.poll {
                    // per-call trap check at function entry (hooks, call hook)
                    let ab = g.abort(0);
                    g.a.alu_mi(
                        Alu::Cmp,
                        Size::S32,
                        Mem::base_disp(R12, lay::callinfo::TRAP as i32),
                        0,
                    );
                    g.a.jcc_label(Cond::Ne, ab);
                }
            }
            if pc > 0 && matches!(f.ops[pc - 1], Op::Call { .. }) {
                res.push((pc, g.a.offset()));
            }
            if o.count && leaders[pc] {
                let mut cnt = 0;
                let mut q = pc;
                loop {
                    if !matches!(f.ops[q], Op::Mmbin) {
                        cnt += 1;
                    }
                    q += 1;
                    if q >= n || leaders[q] {
                        break;
                    }
                }
                let ab = g.abort(pc);
                g.a.alu_mi(Alu::Sub, Size::S64, Mem::base_disp(R13, ST_COUNT), cnt);
                g.a.jcc_label(Cond::S, ab);
            }
            if let Op::Return1 { a } = f.ops[pc] {
                ret_reg = a;
            }
            g.op(pc);
        }
        g.a.bind(g.lab[n]);
        let ab = g.abort(n);
        g.a.jmp_label(ab);
        let aborts = std::mem::take(&mut g.aborts);
        for (pc, l) in aborts.into_iter().enumerate() {
            if let Some(l) = l {
                a.bind(l);
                a.mov_mi(Size::S32, Mem::base_disp(R13, ST_ABORT), pc as i32 + 1);
                a.mov_mi(Size::S32, Mem::base_disp(R13, ST_ABORT_FN), fi as i32);
                a.alu_rr(Alu::Xor, Size::S32, RAX, RAX);
                a.jmp_label(epi);
            }
        }
        resumes.push(res);
    }
    a.bind(epi);
    epilogue(&mut a);
    (
        Multi {
            code: a.finish().expect("assemble"),
            entries,
            resumes,
        },
        ret_reg,
    )
}

impl G<'_> {
    fn abort(&mut self, pc: usize) -> Label {
        if let Some(l) = self.aborts[pc] {
            return l;
        }
        let l = self.a.new_label();
        self.aborts[pc] = Some(l);
        l
    }

    fn ty(&self, s: Src) -> T {
        match s {
            Src::R(_) => T::Dyn,
            Src::I(_) => T::Any,
            Src::K(k) => match self.f.k[k as usize] {
                Kv::Int(_) => T::Int,
                Kv::Flt(_) => T::Flt,
                _ => panic!("non-numeric constant operand"),
            },
        }
    }

    fn tag_of(&mut self, s: Src, tag: i32, target: Label) {
        if let Src::R(r) = s {
            cmp_tag(self.a, 16 * r as i32, tag);
            self.a.jcc_label(Cond::Ne, target);
        }
    }

    fn poll(&mut self, pc: usize) {
        if self.o.poll {
            let ab = self.abort(pc);
            self.a.alu_mi(Alu::Cmp, Size::S32, Mem::base(R13), 0);
            self.a.jcc_label(Cond::Ne, ab);
        }
    }

    fn mem_pay(&self, s: Src) -> Option<Mem> {
        match s {
            Src::R(r) => Some(pay(r)),
            Src::K(k) => Some(kpay(k)),
            Src::I(_) => None,
        }
    }

    /// Integer value of `s` into `dst`.
    fn load_i(&mut self, dst: Gpr, s: Src) {
        match s {
            Src::I(i) => self.a.mov_ri(dst, i),
            _ => {
                let m = self.mem_pay(s).unwrap();
                self.a.mov_rm(Size::S64, dst, m);
            }
        }
    }

    /// Float bits of `s` (a register/constant payload or a small immediate converted to f64) into `x`.
    fn load_f(&mut self, x: Xmm, s: Src) {
        match s {
            Src::I(i) => {
                self.a.mov_ri(RAX, (i as f64).to_bits() as i64);
                self.a.movq_xr(x, RAX);
            }
            _ => {
                let m = self.mem_pay(s).unwrap();
                self.a.movsd_rm(x, m);
            }
        }
    }

    fn store_i(&mut self, a: u32) {
        self.a.mov_mr(Size::S64, pay(a), RAX);
        self.a.mov_mi(Size::S64, slot(a), INT);
    }

    fn store_f(&mut self, a: u32, x: Xmm) {
        self.a.movsd_mr(pay(a), x);
        self.a.mov_mi(Size::S64, slot(a), FLT);
    }

    /// Dual-path emission for `b op c`. `int` and `flt` emit the path bodies (operands already guarded).
    fn dual(
        &mut self,
        pc: usize,
        b: Src,
        c: Src,
        int: impl FnOnce(&mut Self),
        flt: impl FnOnce(&mut Self),
    ) {
        let (tb, tc) = (self.ty(b), self.ty(c));
        let int_ok = tb.int_ok() && tc.int_ok();
        let flt_ok = tb.flt_ok() && tc.flt_ok();
        let ab = self.abort(pc);
        let dyns: Vec<Src> = [b, c]
            .into_iter()
            .filter(|s| matches!(s, Src::R(_)))
            .collect();
        if int_ok && flt_ok {
            assert!(!dyns.is_empty());
            let lf = self.a.new_label();
            let done = self.a.new_label();
            for (i, s) in dyns.iter().enumerate() {
                self.tag_of(*s, INT, if i == 0 { lf } else { ab });
            }
            int(self);
            self.a.jmp_label(done);
            self.a.bind(lf);
            for s in &dyns {
                self.tag_of(*s, FLT, ab);
            }
            flt(self);
            self.a.bind(done);
        } else if int_ok {
            for s in &dyns {
                self.tag_of(*s, INT, ab);
            }
            int(self);
        } else if flt_ok {
            for s in &dyns {
                self.tag_of(*s, FLT, ab);
            }
            flt(self);
        } else {
            panic!("pc {pc}: statically mixed int/float operands");
        }
    }

    fn op(&mut self, pc: usize) {
        let ab = self.abort(pc);
        match self.f.ops[pc] {
            Op::Move { a, b } => {
                self.a.mov_rm(Size::S64, RAX, slot(b));
                self.a.mov_rm(Size::S64, RCX, pay(b));
                self.a.mov_mr(Size::S64, slot(a), RAX);
                self.a.mov_mr(Size::S64, pay(a), RCX);
            }
            Op::LoadI { a, i } => {
                self.a.mov_ri(RAX, i);
                self.store_i(a);
            }
            Op::LoadF { a, f } => {
                self.a.mov_ri(RAX, f.to_bits() as i64);
                self.a.mov_mr(Size::S64, pay(a), RAX);
                self.a.mov_mi(Size::S64, slot(a), FLT);
            }
            Op::LoadK { a, k } => {
                self.a.mov_rm(Size::S64, RAX, kslot(k));
                self.a.mov_rm(Size::S64, RCX, kpay(k));
                self.a.mov_mr(Size::S64, slot(a), RAX);
                self.a.mov_mr(Size::S64, pay(a), RCX);
            }
            Op::Mmbin => {}
            Op::Jmp { target } => {
                if self.f.consumed_jmp(pc) {
                    return;
                }
                if target <= pc {
                    self.poll(pc);
                }
                self.a.jmp_label(self.lab[target]);
            }
            Op::Unm { a, b } => {
                let lf = self.a.new_label();
                let done = self.a.new_label();
                cmp_tag(self.a, 16 * b as i32, INT);
                self.a.jcc_label(Cond::Ne, lf);
                self.a.mov_rm(Size::S64, RAX, pay(b));
                self.a.neg(Size::S64, RAX);
                self.store_i(a);
                self.a.jmp_label(done);
                self.a.bind(lf);
                cmp_tag(self.a, 16 * b as i32, FLT);
                self.a.jcc_label(Cond::Ne, ab);
                self.a.mov_rm(Size::S64, RAX, pay(b));
                self.a.mov_ri(RCX, i64::MIN);
                self.a.alu_rr(Alu::Xor, Size::S64, RAX, RCX);
                self.a.mov_mr(Size::S64, pay(a), RAX);
                self.a.mov_mi(Size::S64, slot(a), FLT);
                self.a.bind(done);
            }
            Op::Arith { op, a, b, c } => self.arith(pc, op, a, b, c),
            Op::Cmp { rel, a, b, k } => self.cmp(pc, rel, a, b, k),
            Op::ForPrep { a, skip } => self.forprep(pc, a, skip),
            Op::ForLoop { a, head } => self.forloop(pc, a, head),
            Op::Return1 { a } if self.cfg.call_mode => self.return1_call(pc, a),
            Op::Return1 { a } => {
                self.a.mov_rm(Size::S64, RAX, pay(a));
                self.a.jmp_label(self.epi);
            }
            Op::Unreach => self.a.jmp_label(ab),
            Op::GetTable { a, b, c } => self.gettable(pc, a, b, Key::R(c)),
            Op::GetI { a, b, i } => self.gettable(pc, a, b, Key::Imm(i)),
            Op::GetField { a, b, k } => self.getfield(pc, a, b, k),
            Op::SetTable { a, b, c } => self.settable(pc, a, Key::R(b), c),
            Op::SetI { a, i, c } => self.settable(pc, a, Key::Imm(i), c),
            Op::SetField { a, k, c } => self.setfield(pc, a, k, c),
            Op::Test { a, k } => self.test(pc, a, k),
            Op::LoadBool { a, v } => {
                let tag = if v { TAG_FALSE | 0x10 } else { TAG_FALSE };
                self.a.mov_mi(Size::S64, slot(a), i32::from(tag));
            }
            Op::Call { a, b, c } => self.call(pc, a, b, c),
        }
    }

    // ---------------------------------------------------------------- tables

    /// Guards `R[t]` is a table and the key is an integer inside the array part; leaves rdx = table, rsi =
    /// array pointer (`array`, i.e. the address of the length hint); for a register key also rax = key and
    /// rcx = key - 1. Aborts (= exit to the interpreter before any side effect) otherwise.
    fn array_access(&mut self, ab: Label, t: u32, key: Key) {
        self.a.cmp_m8_i(slot(t), TAG_TABLE);
        self.a.jcc_label(Cond::Ne, ab);
        self.a.mov_rm(Size::S64, RDX, pay(t));
        match key {
            Key::R(r) => {
                self.a.cmp_m8_i(slot(r), INT as u8);
                self.a.jcc_label(Cond::Ne, ab);
                self.a.mov_rm(Size::S64, RAX, pay(r));
                self.a.lea(Size::S64, RCX, Mem::base_disp(RAX, -1));
                self.a.mov_rm(
                    Size::S32,
                    RSI,
                    Mem::base_disp(RDX, lay::table::ASIZE as i32),
                );
                self.a.alu_rr(Alu::Cmp, Size::S64, RCX, RSI);
                self.a.jcc_label(Cond::Ae, ab);
            }
            Key::Imm(i) => {
                if i == 0 {
                    self.a.jmp_label(ab);
                    return;
                }
                self.a.alu_mi(
                    Alu::Cmp,
                    Size::S32,
                    Mem::base_disp(RDX, lay::table::ASIZE as i32),
                    i as i32 - 1,
                );
                self.a.jcc_label(Cond::Be, ab);
            }
        }
        self.a.mov_rm(
            Size::S64,
            RSI,
            Mem::base_disp(RDX, lay::table::ARRAY as i32),
        );
    }

    /// Tag-byte operand of the array slot selected by `array_access`.
    fn elem_tag(key: Key) -> Mem {
        match key {
            Key::R(_) => Mem::sib(RSI, RCX, 1, lay::table::ARRAY_TAG_BASE as i32),
            Key::Imm(i) => Mem::base_disp(RSI, lay::table::ARRAY_TAG_BASE as i32 + i as i32 - 1),
        }
    }

    /// Payload operand of the array slot (payloads are stored below the array pointer; for a register key
    /// rax must hold `-key` ([`G::neg_key`])).
    fn elem_pay(key: Key) -> Mem {
        match key {
            Key::R(_) => Mem::sib(RSI, RAX, 8, 0),
            Key::Imm(i) => {
                Mem::base_disp(RSI, -(lay::table::ARRAY_PAYLOAD_STRIDE as i32) * i as i32)
            }
        }
    }

    fn neg_key(&mut self, key: Key) {
        if let Key::R(_) = key {
            self.a.neg(Size::S64, RAX);
        }
    }

    /// GETTABLE / GETI on the array part: in-bounds, non-nil element, otherwise exit (the interpreter's
    /// `get_int_arr` fast path; nil needs the `__index` check).
    fn gettable(&mut self, pc: usize, a: u32, b: u32, key: Key) {
        let ab = self.abort(pc);
        self.array_access(ab, b, key);
        self.a.movzx_rm8(R8, Self::elem_tag(key));
        self.a.test_ri(Size::S32, R8, 0x0F);
        self.a.jcc_label(Cond::E, ab);
        self.neg_key(key);
        self.a.mov_rm(Size::S64, R9, Self::elem_pay(key));
        self.a.mov_mr(Size::S64, pay(a), R9);
        self.a.mov_mr(Size::S64, slot(a), R8);
    }

    /// Loads the RK value into (`tag`, `pay`) and exits if it is collectable (the barrier is not inlined
    /// here: the interpreter's `barrier_back` is a no-op for non-collectable values, and the oracle
    /// programs store only numbers and booleans).
    fn load_rk(&mut self, c: Src, ab: Label, tag: Gpr, pay_r: Gpr) {
        match c {
            Src::R(r) => {
                self.a.movzx_rm8(tag, slot(r));
                self.a.mov_rm(Size::S64, pay_r, pay(r));
                self.a.test_ri(Size::S32, tag, 0x40);
                self.a.jcc_label(Cond::Ne, ab);
            }
            Src::K(k) => {
                self.a.movzx_rm8(tag, kslot(k));
                self.a.mov_rm(Size::S64, pay_r, kpay(k));
                if !matches!(self.f.k[k as usize], Kv::Int(_) | Kv::Flt(_) | Kv::Bool(_)) {
                    self.a.test_ri(Size::S32, tag, 0x40);
                    self.a.jcc_label(Cond::Ne, ab);
                }
            }
            Src::I(_) => unreachable!("immediate stored into a table"),
        }
    }

    /// SETTABLE / SETI: store over a non-empty array slot, or over an empty one of a table without a
    /// metatable (`pset_existing_int_arr`).
    fn settable(&mut self, pc: usize, a: u32, key: Key, c: Src) {
        let ab = self.abort(pc);
        self.array_access(ab, a, key);
        let ok = self.a.new_label();
        self.a.test_m8_i(Self::elem_tag(key), 0x0F);
        self.a.jcc_label(Cond::Ne, ok);
        self.a.alu_mi(
            Alu::Cmp,
            Size::S64,
            Mem::base_disp(RDX, lay::table::METATABLE as i32),
            0,
        );
        self.a.jcc_label(Cond::Ne, ab);
        self.a.bind(ok);
        self.load_rk(c, ab, R8, R9);
        self.a.mov_m8_r(Self::elem_tag(key), R8);
        self.neg_key(key);
        self.a.mov_mr(Size::S64, Self::elem_pay(key), R9);
    }

    /// Short-string lookup in the node part (`hgetshortstr`): main position, then the collision chain.
    /// Leaves r9 = the node; aborts if the key is absent. Clobbers rax, rcx, rdx, rsi, r8.
    fn node_lookup(&mut self, ab: Label, t: u32, k: u32) {
        use lay::node;
        self.a.cmp_m8_i(slot(t), TAG_TABLE);
        self.a.jcc_label(Cond::Ne, ab);
        self.a.mov_rm(Size::S64, RDX, pay(t));
        self.a.mov_rm(Size::S64, RSI, kpay(k)); // the key string
        self.a.mov_rm(
            Size::S32,
            RAX,
            Mem::base_disp(RSI, lay::string::SHORT_HASH as i32),
        );
        self.a
            .movzx_rm8(RCX, Mem::base_disp(RDX, lay::table::LSIZENODE as i32));
        self.a.mov_ri(R8, 1);
        self.a.shift_cl(Shift::Shl, Size::S32, R8);
        self.a.dec(Size::S32, R8);
        self.a.alu_rr(Alu::And, Size::S32, RAX, R8);
        self.a.lea(Size::S64, RAX, Mem::sib(RAX, RAX, 2, 0)); // 3 * index (nodes are 24 bytes)
        self.a
            .mov_rm(Size::S64, R9, Mem::base_disp(RDX, lay::table::NODE as i32));
        self.a.lea(Size::S64, R9, Mem::sib(R9, RAX, 8, 0));
        let lp = self.a.new_label();
        let nx = self.a.new_label();
        let found = self.a.new_label();
        self.a.bind(lp);
        self.a
            .cmp_m8_i(Mem::base_disp(R9, node::KEY_TT as i32), TAG_SHRSTR);
        self.a.jcc_label(Cond::Ne, nx);
        self.a.alu_mr(
            Alu::Cmp,
            Size::S64,
            Mem::base_disp(R9, node::KEY as i32),
            RSI,
        );
        self.a.jcc_label(Cond::E, found);
        self.a.bind(nx);
        // movsxd rax, dword [r9 + NEXT]   (the encoder has no movsxd)
        const { assert!(node::NEXT < 128) };
        self.a.emit_bytes(&[0x49, 0x63, 0x41, node::NEXT as u8]);
        self.a.test_rr(Size::S32, RAX, RAX);
        self.a.jcc_label(Cond::E, ab);
        self.a.lea(Size::S64, RAX, Mem::sib(RAX, RAX, 2, 0));
        self.a.lea(Size::S64, R9, Mem::sib(R9, RAX, 8, 0));
        self.a.jmp_label(lp);
        self.a.bind(found);
    }

    fn getfield(&mut self, pc: usize, a: u32, b: u32, k: u32) {
        let ab = self.abort(pc);
        self.node_lookup(ab, b, k);
        self.a
            .movzx_rm8(R8, Mem::base_disp(R9, lay::node::VAL_TT as i32));
        self.a.test_ri(Size::S32, R8, 0x0F);
        self.a.jcc_label(Cond::E, ab);
        self.a
            .mov_rm(Size::S64, R10, Mem::base_disp(R9, lay::node::VAL as i32));
        self.a.mov_mr(Size::S64, pay(a), R10);
        self.a.mov_mr(Size::S64, slot(a), R8);
    }

    /// SETFIELD over an existing non-empty node (`pset_existing_shortstr`).
    fn setfield(&mut self, pc: usize, a: u32, k: u32, c: Src) {
        let ab = self.abort(pc);
        self.node_lookup(ab, a, k);
        self.a
            .test_m8_i(Mem::base_disp(R9, lay::node::VAL_TT as i32), 0x0F);
        self.a.jcc_label(Cond::E, ab);
        self.load_rk(c, ab, R10, R11);
        self.a
            .mov_m8_r(Mem::base_disp(R9, lay::node::VAL_TT as i32), R10);
        self.a
            .mov_mr(Size::S64, Mem::base_disp(R9, lay::node::VAL as i32), R11);
    }

    /// TEST + JMP: the jump is taken when `truthy(R[A]) == k`.
    fn test(&mut self, pc: usize, a: u32, k: bool) {
        let Op::Jmp { target } = self.f.ops[pc + 1] else {
            panic!("TEST not followed by JMP")
        };
        let tgt = self.lab[target];
        let fall = self.lab[pc + 2];
        self.a.movzx_rm8(RAX, slot(a));
        self.a
            .alu_ri(Alu::Cmp, Size::S32, RAX, i32::from(TAG_FALSE));
        if k {
            // jump iff truthy
            self.a.jcc_label(Cond::E, fall);
            self.a.test_ri(Size::S32, RAX, 0x0F);
            self.a.jcc_label(Cond::Ne, tgt);
        } else {
            // jump iff falsy
            self.a.jcc_label(Cond::E, tgt);
            self.a.test_ri(Size::S32, RAX, 0x0F);
            self.a.jcc_label(Cond::E, tgt);
        }
    }

    // ---------------------------------------------------------------- calls (E2)

    /// CALL of a Lua closure: `pre_call_lua_fast` inlined (stack check, next `CallInfo`, frame fields), then a
    /// jump to the callee's compiled entry (no native recursion).
    fn call(&mut self, pc: usize, ra: u32, b: u32, c: u32) {
        use lay::{callinfo as ci, lclosure, proto as pr};
        let ab = self.abort(pc);
        assert!(b != 0 && c == 2, "oracle CALL: fixed arguments, one result");
        let nargs = b - 1;
        self.a.cmp_m8_i(slot(ra), TAG_LCL);
        self.a.jcc_label(Cond::Ne, ab);
        self.a.mov_rm(Size::S64, RDX, pay(ra)); // closure
        self.a
            .mov_rm(Size::S64, RSI, Mem::base_disp(RDX, lclosure::PROTO as i32)); // proto
                                                                                  // st.top = ci.func + 1 + a + b   (index of the slot after the last argument)
        self.a
            .mov_rm(Size::S64, RAX, Mem::base_disp(R12, ci::FUNC as i32));
        self.a
            .lea(Size::S64, R8, Mem::base_disp(RAX, 1 + ra as i32 + b as i32));
        self.a.mov_mr(Size::S64, Mem::base_disp(R13, ST_TOP), R8);
        // stack_size - top <= fsize  -> general path
        self.a
            .mov_rm(Size::S64, R9, Mem::base_disp(R13, ST_STACK_SIZE));
        self.a.alu_rr(Alu::Sub, Size::S64, R9, R8);
        self.a
            .movzx_rm8(RCX, Mem::base_disp(RSI, pr::MAXSTACKSIZE as i32));
        self.a.alu_rr(Alu::Cmp, Size::S64, R9, RCX);
        self.a.jcc_label(Cond::Le, ab);
        // a spare CallInfo node?
        self.a
            .mov_rm(Size::S64, R10, Mem::base_disp(R12, ci::NEXT as i32));
        self.a.test_rr(Size::S64, R10, R10);
        self.a.jcc_label(Cond::E, ab);
        // fewer arguments than parameters: the nil-fill loop is not emulated
        self.a
            .cmp_m8_i(Mem::base_disp(RSI, pr::NUMPARAMS as i32), nargs as u8);
        self.a.jcc_label(Cond::A, ab);
        // the callee must be compiled (Proto.jit emulation)
        self.a
            .mov_rm(Size::S64, R11, Mem::base_disp(RSI, pr::GCLIST as i32));
        self.a.test_rr(Size::S64, R11, R11);
        self.a.jcc_label(Cond::E, ab);
        // savepc for the caller
        let ret_pc = self.f.code_ptr as u64 + 4 * (pc as u64 + 1);
        self.a.mov_ri64(RDI, ret_pc);
        self.a
            .mov_mr(Size::S64, Mem::base_disp(R12, ci::SAVEDPC as i32), RDI);
        // set_ci + frame fields
        self.a.mov_mr(Size::S64, Mem::base_disp(R13, ST_CI), R10);
        self.a
            .lea(Size::S64, R8, Mem::base_disp(RAX, 1 + ra as i32)); // callee func index
        self.a
            .mov_mr(Size::S64, Mem::base_disp(R10, ci::FUNC as i32), R8);
        self.a.mov_mi(
            Size::S32,
            Mem::base_disp(R10, ci::CALLSTATUS as i32),
            c as i32,
        );
        self.a.lea(Size::S64, R9, Mem::sib(R8, RCX, 1, 1)); // func + 1 + fsize
        self.a
            .mov_mr(Size::S64, Mem::base_disp(R10, ci::TOP as i32), R9);
        self.a.mov_rm(
            Size::S64,
            RDI,
            Mem::base_disp(RSI, self.cfg.pl.code_ptr as i32),
        );
        self.a
            .mov_mr(Size::S64, Mem::base_disp(R10, ci::SAVEDPC as i32), RDI);
        self.a.mov_rm(
            Size::S64,
            RDI,
            Mem::base_disp(RSI, self.cfg.pl.k_ptr as i32),
        );
        self.a
            .mov_mr(Size::S64, Mem::base_disp(R10, ci::KCACHE as i32), RDI);
        // enter the callee: r12 = ci, rbx = new base, r14 = kcache, jump to its entry
        self.a.mov_rr(Size::S64, R12, R10);
        self.a
            .lea(Size::S64, RBX, Mem::base_disp(RBX, 16 * (ra as i32 + 1)));
        self.a.mov_rr(Size::S64, R14, RDI);
        self.a.mov_rm(Size::S64, RAX, Mem::base(R11));
        self.a.jmp_r(RAX);
    }

    /// RETURN1 inside the flat-call protocol (`Return1` + `do_ret!`): move the result to the function slot,
    /// pop the CallInfo, and jump to the caller's resume address (looked up through the caller's proto).
    fn return1_call(&mut self, pc: usize, ra: u32) {
        use lay::{callinfo as ci, lclosure, proto as pr};
        let ab = self.abort(pc);
        if self.o.poll {
            self.a
                .alu_mi(Alu::Cmp, Size::S32, Mem::base_disp(R12, ci::TRAP as i32), 0);
            self.a.jcc_label(Cond::Ne, ab);
        }
        self.a
            .mov_rm(Size::S32, RAX, Mem::base_disp(R12, ci::CALLSTATUS as i32));
        self.a.mov_rr(Size::S32, RCX, RAX);
        self.a
            .alu_ri(Alu::And, Size::S32, RCX, ci::NRESULTS_MASK as i32);
        self.a.alu_ri(Alu::Cmp, Size::S32, RCX, 2); // exactly one result wanted
        self.a.jcc_label(Cond::Ne, ab);
        // *(base - 1) = R[A]
        self.a.mov_rm(Size::S64, RDX, slot(ra));
        self.a.mov_rm(Size::S64, RSI, pay(ra));
        self.a.mov_mr(Size::S64, Mem::base_disp(RBX, -16), RDX);
        self.a.mov_mr(Size::S64, Mem::base_disp(RBX, -8), RSI);
        // st.ci = ci.previous ; st.top = ci.func + 1
        self.a
            .mov_rm(Size::S64, R9, Mem::base_disp(R12, ci::PREVIOUS as i32));
        self.a.mov_mr(Size::S64, Mem::base_disp(R13, ST_CI), R9);
        self.a
            .mov_rm(Size::S64, RDX, Mem::base_disp(R12, ci::FUNC as i32));
        self.a.inc(Size::S64, RDX);
        self.a.mov_mr(Size::S64, Mem::base_disp(R13, ST_TOP), RDX);
        let fresh = self.a.new_label();
        self.a
            .test_ri(Size::S32, RAX, (ci::CIST_FRESH | ci::CIST_MMRET) as i32);
        self.a.jcc_label(Cond::Ne, fresh);
        // continue the caller: ci = prev; base = stack_base + (func + 1) * 16; kcache = ci.kcache
        self.a.mov_rr(Size::S64, R12, R9);
        self.a
            .mov_rm(Size::S64, RAX, Mem::base_disp(R12, ci::FUNC as i32));
        self.a.shift_ri(Shift::Shl, Size::S64, RAX, 4);
        self.a
            .alu_rm(Alu::Add, Size::S64, RAX, Mem::base_disp(R13, ST_STACK_BASE));
        self.a.lea(Size::S64, RBX, Mem::base_disp(RAX, 16));
        self.a
            .mov_rm(Size::S64, R14, Mem::base_disp(R12, ci::KCACHE as i32));
        // resume address: closure -> proto -> JitProto -> resume[(savedpc - code) / 4]
        self.a.mov_rm(Size::S64, RDX, Mem::base_disp(RBX, -8));
        self.a
            .mov_rm(Size::S64, RDX, Mem::base_disp(RDX, lclosure::PROTO as i32));
        self.a
            .mov_rm(Size::S64, RCX, Mem::base_disp(RDX, pr::GCLIST as i32));
        self.a.test_rr(Size::S64, RCX, RCX);
        self.a.jcc_label(Cond::E, ab);
        self.a
            .mov_rm(Size::S64, RAX, Mem::base_disp(R12, ci::SAVEDPC as i32));
        self.a.alu_rm(
            Alu::Sub,
            Size::S64,
            RAX,
            Mem::base_disp(RDX, self.cfg.pl.code_ptr as i32),
        );
        self.a
            .mov_rm(Size::S64, RAX, Mem::sib(RCX, RAX, 2, JP_RESUME));
        self.a.jmp_r(RAX);
        // the CIST_FRESH frame returned: leave compiled code with the payload in rax
        self.a.bind(fresh);
        self.a.mov_rr(Size::S64, RAX, RSI);
        self.a.jmp_label(self.epi);
    }

    fn arith(&mut self, pc: usize, op: Ar, a: u32, b: Src, c: Src) {
        let ab = self.abort(pc);
        match op {
            Ar::Add | Ar::Sub | Ar::Mul => {
                let (alu, sse) = match op {
                    Ar::Add => (Some(Alu::Add), SseOp::Add),
                    Ar::Sub => (Some(Alu::Sub), SseOp::Sub),
                    _ => (None, SseOp::Mul),
                };
                self.dual(
                    pc,
                    b,
                    c,
                    |g| {
                        g.load_i(RAX, b);
                        match (c, alu) {
                            (Src::I(i), Some(al)) => g.a.alu_ri(al, Size::S64, RAX, i as i32),
                            (Src::I(i), None) => g.a.imul_rri(Size::S64, RAX, RAX, i as i32),
                            (_, Some(al)) => {
                                let m = g.mem_pay(c).unwrap();
                                g.a.alu_rm(al, Size::S64, RAX, m)
                            }
                            (_, None) => {
                                let m = g.mem_pay(c).unwrap();
                                g.a.imul_rm(Size::S64, RAX, m)
                            }
                        }
                        g.store_i(a);
                    },
                    |g| {
                        g.load_f(XMM0, b);
                        match c {
                            Src::I(_) => {
                                g.load_f(XMM1, c);
                                g.a.sse_rr(sse, XMM0, XMM1)
                            }
                            _ => {
                                let m = g.mem_pay(c).unwrap();
                                g.a.sse_rm(sse, XMM0, m)
                            }
                        }
                        g.store_f(a, XMM0);
                    },
                );
            }
            Ar::Div => {
                // always float: int operands are converted
                self.load_conv_f(XMM0, b, pc);
                self.load_conv_f(XMM1, c, pc);
                self.a.sse_rr(SseOp::Div, XMM0, XMM1);
                self.store_f(a, XMM0);
            }
            Ar::IDiv | Ar::Mod => {
                for s in [b, c] {
                    assert!(self.ty(s).int_ok());
                    self.tag_of(s, INT, ab);
                }
                self.load_i(RAX, b);
                self.load_i(RCX, c);
                let const_div = match c {
                    Src::I(i) => Some(i),
                    Src::K(k) => match self.f.k[k as usize] {
                        Kv::Int(i) => Some(i),
                        _ => None,
                    },
                    Src::R(_) => None,
                };
                idiv_core(self.a, op == Ar::IDiv, const_div, ab);
                self.store_i(a);
            }
            Ar::BAnd | Ar::BOr | Ar::BXor => {
                for s in [b, c] {
                    assert!(self.ty(s).int_ok());
                    self.tag_of(s, INT, ab);
                }
                let al = match op {
                    Ar::BAnd => Alu::And,
                    Ar::BOr => Alu::Or,
                    _ => Alu::Xor,
                };
                self.load_i(RAX, b);
                match c {
                    Src::I(i) => self.a.alu_ri(al, Size::S64, RAX, i as i32),
                    _ => {
                        let m = self.mem_pay(c).unwrap();
                        self.a.alu_rm(al, Size::S64, RAX, m)
                    }
                }
                self.store_i(a);
            }
            Ar::Shl | Ar::Shr => {
                for s in [b, c] {
                    assert!(self.ty(s).int_ok());
                    self.tag_of(s, INT, ab);
                }
                self.load_i(RAX, b);
                let cconst = if let Src::I(sc) = c { Some(sc) } else { None };
                if cconst.is_none() {
                    self.load_i(RCX, c);
                }
                shift_core(self.a, op == Ar::Shl, cconst);
                self.store_i(a);
            }
        }
    }

    /// Loads `s` converted to a double into `x` (int operands via `cvtsi2sd`), guarding tags.
    fn load_conv_f(&mut self, x: Xmm, s: Src, pc: usize) {
        let ab = self.abort(pc);
        match s {
            Src::I(_) => self.load_f(x, s),
            Src::K(k) => match self.f.k[k as usize] {
                Kv::Int(_) => {
                    self.a.logic_rr(SseLogic::Xor, x, x);
                    self.a.cvtsi2sd_rm(Size::S64, x, kpay(k));
                }
                _ => self.a.movsd_rm(x, kpay(k)),
            },
            Src::R(r) => {
                let lf = self.a.new_label();
                let done = self.a.new_label();
                cmp_tag(self.a, 16 * r as i32, INT);
                self.a.jcc_label(Cond::Ne, lf);
                self.a.logic_rr(SseLogic::Xor, x, x);
                self.a.cvtsi2sd_rm(Size::S64, x, pay(r));
                self.a.jmp_label(done);
                self.a.bind(lf);
                cmp_tag(self.a, 16 * r as i32, FLT);
                self.a.jcc_label(Cond::Ne, ab);
                self.a.movsd_rm(x, pay(r));
                self.a.bind(done);
            }
        }
    }

    fn cmp(&mut self, pc: usize, rel: Rel, a: Src, b: Src, k: bool) {
        let Op::Jmp { target } = self.f.ops[pc + 1] else {
            panic!("comparison not followed by JMP")
        };
        let tgt = self.lab[target];
        let fall = self.lab[pc + 2];
        let (ta, tb) = (self.ty(a), self.ty(b));
        let int_ok = ta.int_ok() && tb.int_ok();
        let flt_ok = ta.flt_ok() && tb.flt_ok();
        let ab = self.abort(pc);
        let dyns: Vec<Src> = [a, b]
            .into_iter()
            .filter(|s| matches!(s, Src::R(_)))
            .collect();
        let lf = self.a.new_label();
        let skip = self.a.new_label();
        if int_ok {
            for (i, s) in dyns.iter().enumerate() {
                self.tag_of(*s, INT, if i == 0 && flt_ok { lf } else { ab });
            }
            self.load_i(RAX, a);
            match b {
                Src::I(i) => self.a.alu_ri(Alu::Cmp, Size::S64, RAX, i as i32),
                _ => {
                    let m = self.mem_pay(b).unwrap();
                    self.a.alu_rm(Alu::Cmp, Size::S64, RAX, m)
                }
            }
            let cc = if k { int_cc(rel) } else { invert(int_cc(rel)) };
            self.a.jcc_label(cc, tgt);
            if flt_ok {
                self.a.jmp_label(fall);
            }
        }
        if flt_ok {
            if int_ok {
                self.a.bind(lf);
            }
            for s in &dyns {
                self.tag_of(*s, FLT, ab);
            }
            self.load_f(XMM0, a);
            self.load_f(XMM1, b);
            fjump(self.a, rel, k, XMM0, XMM1, tgt, skip);
        }
        self.a.bind(skip);
    }

    fn forprep(&mut self, pc: usize, a: u32, skip: usize) {
        let ab = self.abort(pc);
        let lskip = self.lab[skip];
        let lflt = self.a.new_label();
        let lneg = self.a.new_label();
        let lstore = self.a.new_label();
        let done = self.a.new_label();
        // ra = init, ra+1 = limit, ra+2 = step
        cmp_tag(self.a, 16 * a as i32, INT);
        self.a.jcc_label(Cond::Ne, lflt);
        cmp_tag(self.a, 16 * (a + 1) as i32, INT);
        self.a.jcc_label(Cond::Ne, ab);
        cmp_tag(self.a, 16 * (a + 2) as i32, INT);
        self.a.jcc_label(Cond::Ne, ab);
        self.a.mov_rm(Size::S64, R8, pay(a));
        self.a.mov_rm(Size::S64, R9, pay(a + 1));
        self.a.mov_rm(Size::S64, R10, pay(a + 2));
        self.a.test_rr(Size::S64, R10, R10);
        self.a.jcc_label(Cond::E, ab);
        self.a.jcc_label(Cond::S, lneg);
        self.a.alu_rr(Alu::Cmp, Size::S64, R8, R9);
        self.a.jcc_label(Cond::G, lskip);
        self.a.mov_rr(Size::S64, RAX, R9);
        self.a.alu_rr(Alu::Sub, Size::S64, RAX, R8);
        self.a.alu_ri(Alu::Cmp, Size::S64, R10, 1);
        self.a.jcc_label(Cond::E, lstore);
        self.a.alu_rr(Alu::Xor, Size::S32, RDX, RDX);
        self.a.div(Size::S64, R10);
        self.a.jmp_label(lstore);
        self.a.bind(lneg);
        self.a.alu_rr(Alu::Cmp, Size::S64, R8, R9);
        self.a.jcc_label(Cond::L, lskip);
        self.a.mov_rr(Size::S64, RAX, R8);
        self.a.alu_rr(Alu::Sub, Size::S64, RAX, R9);
        self.a.mov_rr(Size::S64, RCX, R10);
        self.a.inc(Size::S64, RCX);
        self.a.neg(Size::S64, RCX);
        self.a.inc(Size::S64, RCX);
        self.a.alu_rr(Alu::Xor, Size::S32, RDX, RDX);
        self.a.div(Size::S64, RCX);
        self.a.bind(lstore);
        self.a.mov_mr(Size::S64, pay(a), RAX);
        self.a.mov_mi(Size::S64, slot(a), INT);
        self.a.mov_mr(Size::S64, pay(a + 1), R10);
        self.a.mov_mi(Size::S64, slot(a + 1), INT);
        self.a.mov_mr(Size::S64, pay(a + 2), R8);
        self.a.mov_mi(Size::S64, slot(a + 2), INT);
        self.a.jmp_label(done);
        // float loop
        self.a.bind(lflt);
        for r in 0..3 {
            cmp_tag(self.a, 16 * (a + r) as i32, FLT);
            self.a.jcc_label(Cond::Ne, ab);
        }
        self.a.movsd_rm(XMM0, pay(a)); // init
        self.a.movsd_rm(XMM1, pay(a + 1)); // limit
        self.a.movsd_rm(XMM2, pay(a + 2)); // step
        self.a.logic_rr(SseLogic::Xor, XMM3, XMM3);
        let nz = self.a.new_label();
        let lposf = self.a.new_label();
        self.a.ucomisd_rr(XMM2, XMM3);
        self.a.jcc_label(Cond::Ne, nz);
        self.a.jcc_label(Cond::Np, ab); // step == 0.0
        self.a.bind(nz);
        self.a.jcc_label(Cond::A, lposf); // 0 < step
        self.a.ucomisd_rr(XMM1, XMM0); // limit > init ?
        self.a.jcc_label(Cond::A, lskip); // init < limit: skip
        let fstore = self.a.new_label();
        self.a.jmp_label(fstore);
        self.a.bind(lposf);
        self.a.ucomisd_rr(XMM0, XMM1); // init > limit ?
        self.a.jcc_label(Cond::A, lskip);
        self.a.bind(fstore);
        self.a.movsd_mr(pay(a), XMM1);
        self.a.movsd_mr(pay(a + 1), XMM2);
        self.a.movsd_mr(pay(a + 2), XMM0);
        self.a.bind(done);
    }

    fn forloop(&mut self, pc: usize, a: u32, head: usize) {
        let lhead = self.lab[head];
        let next = self.lab[pc + 1];
        let lflt = self.a.new_label();
        let lcont = self.a.new_label();
        cmp_tag(self.a, 16 * (a + 1) as i32, INT);
        self.a.jcc_label(Cond::Ne, lflt);
        self.a.mov_rm(Size::S64, RAX, pay(a));
        self.a.test_rr(Size::S64, RAX, RAX);
        self.a.jcc_label(Cond::E, next);
        self.a.dec(Size::S64, RAX);
        self.a.mov_mr(Size::S64, pay(a), RAX);
        self.a.mov_rm(Size::S64, RCX, pay(a + 2));
        self.a.alu_rm(Alu::Add, Size::S64, RCX, pay(a + 1));
        self.a.mov_mr(Size::S64, pay(a + 2), RCX);
        self.poll(pc);
        self.a.jmp_label(lhead);
        // float loop: ra = limit, ra+1 = step, ra+2 = idx
        self.a.bind(lflt);
        self.a.movsd_rm(XMM0, pay(a + 2));
        self.a.sse_rm(SseOp::Add, XMM0, pay(a + 1)); // idx + step
        self.a.movsd_rm(XMM1, pay(a + 1)); // step
        self.a.movsd_rm(XMM2, pay(a)); // limit
        self.a.logic_rr(SseLogic::Xor, XMM3, XMM3);
        let lneg = self.a.new_label();
        self.a.ucomisd_rr(XMM1, XMM3);
        self.a.jcc_label(Cond::Be, lneg); // not (0 < step)
        self.a.ucomisd_rr(XMM2, XMM0); // limit >= idx
        self.a.jcc_label(Cond::Ae, lcont);
        self.a.jmp_label(next);
        self.a.bind(lneg);
        self.a.ucomisd_rr(XMM0, XMM2); // idx >= limit
        self.a.jcc_label(Cond::Ae, lcont);
        self.a.jmp_label(next);
        self.a.bind(lcont);
        self.a.movsd_mr(pay(a + 2), XMM0);
        self.poll(pc);
        self.a.jmp_label(lhead);
    }
}
