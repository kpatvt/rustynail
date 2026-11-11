//! V2: whole-function register allocation (upper bound for the "tier 1.5" estimate).
//!
//! A forward type-inference pass (int / float per register, joined at merges) replaces the "pre-run type
//! profile": every (Lua register, type) pair that is ever defined gets its own machine register (GPR for
//! ints, xmm for floats), with no liveness analysis (programs are small enough that they all fit). There
//! are no tag guards, no tag stores, no loads/stores to the Value stack except the parameter load at entry,
//! and no back-edge poll / block counter. Constants come from the kcache (`r14`) as memory operands.

use crate::decode::{Ar, Func, Kv, Op, Rel, Src};
use crate::v1::{
    epilogue, fjump, idiv_core, int_cc, invert, kpay, prologue, shift_core, Compiled, FLT, INT,
};
use rn_core::jit_layout as lay;
use rn_jit::arch::x86_64::*;
use std::collections::HashMap;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Ty {
    Bot,
    Int,
    Flt,
    /// Boolean loaded from a table: the machine register holds the tag byte (`false` = 1, `true` = 0x11).
    Bool,
    /// Table whose array pointer and size are hoisted into two registers (parameters only).
    Tab,
    Top,
}

/// Type of a parameter of the compiled function.
#[derive(Clone, Copy, Debug)]
pub enum PTy {
    Int,
    Flt,
    /// A table parameter whose array part holds elements of the given type (`Int`/`Flt`/`Bool`); it must have no
    /// metatable (checked once at entry: the hoisted array pointer and size are loop invariants because the
    /// compiled code performs no calls, no resizes and only in-bounds stores).
    Tab(Ty),
}

fn join(a: Ty, b: Ty) -> Ty {
    match (a, b) {
        (Ty::Bot, x) | (x, Ty::Bot) => x,
        (x, y) if x == y => x,
        _ => Ty::Top,
    }
}

type R<T> = Result<T, String>;

fn reg_ty(env: &[Ty], r: u32, pc: usize) -> R<Ty> {
    match env[r as usize] {
        t @ (Ty::Int | Ty::Flt) => Ok(t),
        t => Err(format!("pc {pc}: register {r} has type {t:?}")),
    }
}

/// Type of an operand; `None` for immediates (adapt to the other operand).
fn src_ty(f: &Func, env: &[Ty], s: Src, pc: usize) -> R<Option<Ty>> {
    Ok(match s {
        Src::R(r) => Some(reg_ty(env, r, pc)?),
        Src::K(k) => match f.k[k as usize] {
            Kv::Int(_) => Some(Ty::Int),
            Kv::Flt(_) => Some(Ty::Flt),
            _ => return Err(format!("pc {pc}: non-numeric constant")),
        },
        Src::I(_) => None,
    })
}

fn unify(a: Option<Ty>, b: Option<Ty>, pc: usize) -> R<Ty> {
    match (a, b) {
        (Some(x), Some(y)) if x == y => Ok(x),
        (Some(x), None) | (None, Some(x)) => Ok(x),
        _ => Err(format!("pc {pc}: mixed or untyped operands {a:?} {b:?}")),
    }
}

/// Result type of an arithmetic op.
fn arith_ty(f: &Func, env: &[Ty], op: Ar, b: Src, c: Src, pc: usize) -> R<Ty> {
    let (tb, tc) = (src_ty(f, env, b, pc)?, src_ty(f, env, c, pc)?);
    match op {
        Ar::Add | Ar::Sub | Ar::Mul => unify(tb, tc, pc),
        Ar::Div => Ok(Ty::Flt),
        _ => {
            if unify(tb, tc, pc)? != Ty::Int {
                return Err(format!("pc {pc}: integer-only op on floats"));
            }
            Ok(Ty::Int)
        }
    }
}

struct Info {
    env_in: Vec<Option<Vec<Ty>>>,
    defs: Vec<(u32, Ty)>,
    /// Element type of each table parameter register.
    elem: HashMap<u32, Ty>,
}

/// Abstract transfer function; returns the successor pcs.
fn step(
    f: &Func,
    pc: usize,
    env: &mut [Ty],
    defs: &mut Vec<(u32, Ty)>,
    elem: &HashMap<u32, Ty>,
) -> R<Vec<usize>> {
    let mut def = |env: &mut [Ty], r: u32, t: Ty| {
        env[r as usize] = t;
        defs.push((r, t));
    };
    Ok(match f.ops[pc] {
        Op::Move { a, b } => {
            let t = reg_ty(env, b, pc)?;
            def(env, a, t);
            vec![pc + 1]
        }
        Op::LoadI { a, .. } => {
            def(env, a, Ty::Int);
            vec![pc + 1]
        }
        Op::LoadF { a, .. } => {
            def(env, a, Ty::Flt);
            vec![pc + 1]
        }
        Op::LoadK { a, k } => {
            let t = match f.k[k as usize] {
                Kv::Int(_) => Ty::Int,
                Kv::Flt(_) => Ty::Flt,
                _ => return Err(format!("pc {pc}: non-numeric LOADK")),
            };
            def(env, a, t);
            vec![pc + 1]
        }
        Op::Arith { op, a, b, c } => {
            let t = arith_ty(f, env, op, b, c, pc)?;
            def(env, a, t);
            vec![pc + 1]
        }
        Op::Unm { a, b } => {
            let t = reg_ty(env, b, pc)?;
            def(env, a, t);
            vec![pc + 1]
        }
        Op::Mmbin => vec![pc + 1],
        Op::Jmp { target } => vec![target],
        Op::Cmp { a, b, .. } => {
            unify(src_ty(f, env, a, pc)?, src_ty(f, env, b, pc)?, pc)?;
            vec![pc + 1, pc + 2]
        }
        Op::ForPrep { a, skip } => {
            let (i, l, s) = (
                reg_ty(env, a, pc)?,
                reg_ty(env, a + 1, pc)?,
                reg_ty(env, a + 2, pc)?,
            );
            if !(i == l && l == s) {
                return Err(format!(
                    "pc {pc}: for loop with mixed types {i:?} {l:?} {s:?}"
                ));
            }
            for r in a..a + 3 {
                def(env, r, i);
            }
            vec![pc + 1, skip]
        }
        Op::ForLoop { head, .. } => vec![head, pc + 1],
        Op::Return1 { .. } | Op::Unreach => vec![],
        Op::GetTable { a, b, c } => {
            let e = tab_elem(env, elem, b, pc)?;
            if reg_ty(env, c, pc)? != Ty::Int {
                return Err(format!("pc {pc}: non-integer table key"));
            }
            def(env, a, e);
            vec![pc + 1]
        }
        Op::GetI { a, b, .. } => {
            let e = tab_elem(env, elem, b, pc)?;
            def(env, a, e);
            vec![pc + 1]
        }
        Op::SetTable { a, b, c } => {
            tab_elem(env, elem, a, pc)?;
            if reg_ty(env, b, pc)? != Ty::Int {
                return Err(format!("pc {pc}: non-integer table key"));
            }
            store_ty(f, env, c, pc)?;
            vec![pc + 1]
        }
        Op::SetI { a, c, .. } => {
            tab_elem(env, elem, a, pc)?;
            store_ty(f, env, c, pc)?;
            vec![pc + 1]
        }
        Op::LoadBool { a, .. } => {
            def(env, a, Ty::Bool);
            vec![pc + 1]
        }
        Op::Test { a, .. } => {
            if env[a as usize] != Ty::Bool {
                return Err(format!("pc {pc}: TEST on a non-boolean register"));
            }
            vec![pc + 1, pc + 2]
        }
        _ => return Err(format!("pc {pc}: op not supported by V2")),
    })
}

fn tab_elem(env: &[Ty], elem: &HashMap<u32, Ty>, r: u32, pc: usize) -> R<Ty> {
    match (env[r as usize], elem.get(&r)) {
        (Ty::Tab, Some(e)) => Ok(*e),
        (t, _) => Err(format!(
            "pc {pc}: register {r} is {t:?}, not a table parameter"
        )),
    }
}

/// Type of a value stored into a table (`Int`, `Flt`, `Bool`).
fn store_ty(f: &Func, env: &[Ty], c: Src, pc: usize) -> R<Ty> {
    match c {
        Src::R(r) => match env[r as usize] {
            t @ (Ty::Int | Ty::Flt | Ty::Bool) => Ok(t),
            t => Err(format!("pc {pc}: cannot store a {t:?} register")),
        },
        Src::K(k) => match f.k[k as usize] {
            Kv::Int(_) => Ok(Ty::Int),
            Kv::Flt(_) => Ok(Ty::Flt),
            Kv::Bool(_) => Ok(Ty::Bool),
            _ => Err(format!("pc {pc}: unsupported stored constant")),
        },
        Src::I(_) => Err(format!("pc {pc}: immediate stored into a table")),
    }
}

fn analyze(f: &Func, params: &[PTy]) -> R<Info> {
    let n = f.ops.len();
    let mut env_in: Vec<Option<Vec<Ty>>> = vec![None; n + 1];
    let mut start = vec![Ty::Bot; f.max_stack + 8];
    let mut elem = HashMap::new();
    for (i, p) in params.iter().enumerate() {
        start[i] = match *p {
            PTy::Int => Ty::Int,
            PTy::Flt => Ty::Flt,
            PTy::Tab(e) => {
                elem.insert(i as u32, e);
                Ty::Tab
            }
        };
    }
    env_in[0] = Some(start);
    let mut work = vec![0usize];
    let mut scratch = Vec::new();
    while let Some(pc) = work.pop() {
        if pc >= n {
            continue;
        }
        let mut env = env_in[pc].clone().unwrap();
        for s in step(f, pc, &mut env, &mut scratch, &elem)? {
            match &mut env_in[s] {
                None => {
                    env_in[s] = Some(env.clone());
                    work.push(s);
                }
                Some(old) => {
                    let mut ch = false;
                    for (o, e) in old.iter_mut().zip(&env) {
                        let j = join(*o, *e);
                        ch |= j != *o;
                        *o = j;
                    }
                    if ch {
                        work.push(s);
                    }
                }
            }
        }
    }
    // defs in pc order, from the final environments
    let mut defs = Vec::new();
    #[allow(clippy::needless_range_loop)]
    for pc in 0..n {
        if let Some(e) = &env_in[pc] {
            let mut e = e.clone();
            step(f, pc, &mut e, &mut defs, &elem)?;
        }
    }
    Ok(Info { env_in, defs, elem })
}

#[derive(Clone, Copy, Debug)]
enum Loc {
    G(Gpr),
    X(Xmm),
}

/// Integer operand.
#[derive(Clone, Copy)]
enum IO {
    R(Gpr),
    Imm(i64),
    M(Mem),
}

/// Float operand.
#[derive(Clone, Copy)]
enum FO {
    X(Xmm),
    M(Mem),
}

fn movaps(a: &mut Assembler, d: Xmm, s: Xmm) {
    if d == s {
        return;
    }
    let (d, s) = (d.index(), s.index());
    let rex = 0x40 | ((d >> 3) << 2) | (s >> 3);
    if rex != 0x40 {
        a.emit_bytes(&[rex]);
    }
    a.emit_bytes(&[0x0F, 0x28, 0xC0 | ((d & 7) << 3) | (s & 7)]);
}

#[derive(Clone, Copy, PartialEq)]
enum IB {
    Alu(Alu),
    Mul,
}

struct E<'a> {
    f: &'a Func,
    a: Assembler,
    info: Info,
    /// Table parameter register -> (array pointer register, array size register).
    tabs: HashMap<u32, (Gpr, Gpr)>,
    map: HashMap<(u32, Ty), Loc>,
    lab: Vec<Label>,
    epi: Label,
    ud: Label,
    ret_ty: Option<i32>,
}

pub fn compile(f: &Func) -> Compiled {
    compile_t(f, &[PTy::Int])
}

/// V2 with explicit parameter types (tables hoisted, see [`PTy`]).
pub fn compile_t(f: &Func, params: &[PTy]) -> Compiled {
    let info = analyze(f, params).expect("type inference");
    // register assignment
    let gpool = [RBX, RBP, RSI, RDI, R8, R9, R12, R13, R15];
    let xpool: Vec<Xmm> = (4..16).map(Xmm::new).collect();
    let (mut gi, mut xi) = (0, 0);
    let mut map = HashMap::new();
    let mut want: Vec<(u32, Ty)> = params
        .iter()
        .enumerate()
        .map(|(i, p)| {
            (
                i as u32,
                match p {
                    PTy::Int => Ty::Int,
                    PTy::Flt => Ty::Flt,
                    PTy::Tab(_) => Ty::Tab,
                },
            )
        })
        .collect();
    want.extend(info.defs.iter().copied());
    let mut tabs: HashMap<u32, (Gpr, Gpr)> = HashMap::new();
    for (r, t) in want {
        if t == Ty::Tab {
            let a1 = *gpool.get(gi).expect("out of GPRs for V2 allocation");
            let a2 = *gpool.get(gi + 1).expect("out of GPRs for V2 allocation");
            gi += 2;
            tabs.insert(r, (a1, a2));
            continue;
        }
        map.entry((r, t)).or_insert_with(|| match t {
            Ty::Int | Ty::Bool => {
                let g = *gpool.get(gi).expect("out of GPRs for V2 allocation");
                gi += 1;
                Loc::G(g)
            }
            Ty::Flt => {
                let x = *xpool
                    .get(xi)
                    .expect("out of xmm registers for V2 allocation");
                xi += 1;
                Loc::X(x)
            }
            _ => unreachable!(),
        });
    }
    eprintln!("[oracle] v2 allocation: {gi} GPRs, {xi} xmm registers");
    let mut a = Assembler::new();
    let n = f.ops.len();
    let lab: Vec<Label> = (0..=n).map(|_| a.new_label()).collect();
    let epi = a.new_label();
    let ud = a.new_label();
    let mut e = E {
        f,
        a,
        info,
        tabs,
        map,
        lab,
        epi,
        ud,
        ret_ty: None,
    };
    prologue(&mut e.a);
    e.a.mov_rr(Size::S64, R14, RSI);
    e.a.mov_rr(Size::S64, RAX, RDI);
    for (i, p) in params.iter().enumerate() {
        let src = Mem::base_disp(RAX, 16 * i as i32 + 8);
        match p {
            PTy::Int => {
                let g = e.g(i as u32);
                e.a.mov_rm(Size::S64, g, src);
            }
            PTy::Flt => {
                let x = e.x(i as u32);
                e.a.movsd_rm(x, src);
            }
            PTy::Tab(_) => {
                let (arr, asz) = e.tabs[&(i as u32)];
                e.a.mov_rm(Size::S64, RCX, src);
                // no metatable (empty array slots may then be stored over without a __newindex check)
                e.a.alu_mi(
                    Alu::Cmp,
                    Size::S64,
                    Mem::base_disp(RCX, lay::table::METATABLE as i32),
                    0,
                );
                e.a.jcc_label(Cond::Ne, e.ud);
                e.a.mov_rm(
                    Size::S32,
                    asz,
                    Mem::base_disp(RCX, lay::table::ASIZE as i32),
                );
                e.a.mov_rm(
                    Size::S64,
                    arr,
                    Mem::base_disp(RCX, lay::table::ARRAY as i32),
                );
            }
        }
    }
    for pc in 0..n {
        e.a.bind(e.lab[pc]);
        if e.info.env_in[pc].is_some() {
            e.op(pc);
        }
    }
    e.a.bind(e.lab[n]);
    e.a.bind(e.ud);
    e.a.emit_bytes(&[0x0F, 0x0B]); // ud2
    e.a.bind(e.epi);
    epilogue(&mut e.a);
    let ret_ty = e.ret_ty;
    Compiled {
        code: e.a.finish().expect("assemble"),
        ret_reg: 0,
        ret_ty,
    }
}

impl E<'_> {
    fn g(&self, r: u32) -> Gpr {
        match self.map.get(&(r, Ty::Int)) {
            Some(Loc::G(g)) => *g,
            _ => panic!("no int location for register {r}"),
        }
    }

    fn gb(&self, r: u32) -> Gpr {
        match self.map.get(&(r, Ty::Bool)) {
            Some(Loc::G(g)) => *g,
            _ => panic!("no bool location for register {r}"),
        }
    }

    /// Element slot operands for table register `t` and the key in `key` (`Key::R` = register holding the
    /// integer key, which is bounds-checked against the hoisted size). Leaves rcx = key - 1 and rax = -key.
    fn arr_index(&mut self, t: u32, key: crate::v1::KeyOp) {
        let (_, asz) = self.tabs[&t];
        match key {
            crate::v1::KeyOp::R(kr) => {
                let k = self.g(kr);
                self.a.lea(Size::S64, RCX, Mem::base_disp(k, -1));
                self.a.alu_rr(Alu::Cmp, Size::S64, RCX, asz);
                self.a.jcc_label(Cond::Ae, self.ud);
                self.a.mov_rr(Size::S64, RAX, k);
                self.a.neg(Size::S64, RAX);
            }
            crate::v1::KeyOp::Imm(i) => {
                if i == 0 {
                    self.a.jmp_label(self.ud);
                    return;
                }
                self.a.alu_ri(Alu::Cmp, Size::S32, asz, i as i32 - 1);
                self.a.jcc_label(Cond::Be, self.ud);
            }
        }
    }

    fn elem_tag(&self, t: u32, key: crate::v1::KeyOp) -> Mem {
        let (arr, _) = self.tabs[&t];
        match key {
            crate::v1::KeyOp::R(_) => Mem::sib(arr, RCX, 1, lay::table::ARRAY_TAG_BASE as i32),
            crate::v1::KeyOp::Imm(i) => {
                Mem::base_disp(arr, lay::table::ARRAY_TAG_BASE as i32 + i as i32 - 1)
            }
        }
    }

    fn elem_pay(&self, t: u32, key: crate::v1::KeyOp) -> Mem {
        let (arr, _) = self.tabs[&t];
        match key {
            crate::v1::KeyOp::R(_) => Mem::sib(arr, RAX, 8, 0),
            crate::v1::KeyOp::Imm(i) => {
                Mem::base_disp(arr, -(lay::table::ARRAY_PAYLOAD_STRIDE as i32) * i as i32)
            }
        }
    }

    /// `R[a] = R[t][key]` with the element type known from the table's contents; a mismatching tag leaves
    /// compiled code (ud2 here).
    fn gettable(&mut self, pc: usize, a: u32, t: u32, key: crate::v1::KeyOp) {
        let ty = self.env(pc + 1)[a as usize];
        self.arr_index(t, key);
        let tag = self.elem_tag(t, key);
        match ty {
            Ty::Int | Ty::Flt => {
                let want = if ty == Ty::Int { INT } else { FLT };
                self.a.cmp_m8_i(tag, want as u8);
                self.a.jcc_label(Cond::Ne, self.ud);
                let pay = self.elem_pay(t, key);
                if ty == Ty::Int {
                    let d = self.g(a);
                    self.a.mov_rm(Size::S64, d, pay);
                } else {
                    let d = self.x(a);
                    self.a.movsd_rm(d, pay);
                }
            }
            Ty::Bool => {
                // tag - 1 must be 0 (false) or 0x10 (true)
                let d = self.gb(a);
                self.a.movzx_rm8(d, tag);
                self.a.lea(Size::S32, RDX, Mem::base_disp(d, -1));
                self.a.test_ri(Size::S32, RDX, 0xEF);
                self.a.jcc_label(Cond::Ne, self.ud);
            }
            _ => unreachable!(),
        }
    }

    /// `R[t][key] = value` over an in-bounds slot (table has no metatable: checked at entry).
    fn settable(&mut self, pc: usize, t: u32, key: crate::v1::KeyOp, c: Src) {
        let vt = store_ty(self.f, self.env(pc), c, pc).expect("store type");
        self.arr_index(t, key);
        let tag = self.elem_tag(t, key);
        let pay = self.elem_pay(t, key);
        match (vt, c) {
            (Ty::Int, Src::R(r)) => {
                self.a.mov_m8_i(tag, INT as i8);
                let g = self.g(r);
                self.a.mov_mr(Size::S64, pay, g);
            }
            (Ty::Flt, Src::R(r)) => {
                self.a.mov_m8_i(tag, FLT as i8);
                let x = self.x(r);
                self.a.movsd_mr(pay, x);
            }
            (Ty::Bool, Src::R(r)) => {
                let g = self.gb(r);
                self.a.mov_m8_r(tag, g);
                self.a.mov_mi(Size::S64, pay, 0);
            }
            (_, Src::K(k)) => {
                // tag known statically; payload copied from the constant
                let tagv = match self.f.k[k as usize] {
                    Kv::Int(_) => INT as i8,
                    Kv::Flt(_) => FLT as i8,
                    Kv::Bool(b) => (rn_core::value::LUA_VFALSE | if b { 0x10 } else { 0 }) as i8,
                    _ => unreachable!(),
                };
                self.a.mov_m8_i(tag, tagv);
                self.a.mov_rm(Size::S64, RDX, kpay(k));
                self.a.mov_mr(Size::S64, pay, RDX);
            }
            _ => unreachable!(),
        }
    }

    fn x(&self, r: u32) -> Xmm {
        match self.map.get(&(r, Ty::Flt)) {
            Some(Loc::X(x)) => *x,
            _ => panic!("no float location for register {r}"),
        }
    }

    fn env(&self, pc: usize) -> &[Ty] {
        self.info.env_in[pc].as_ref().unwrap()
    }

    fn iop(&self, s: Src) -> IO {
        match s {
            Src::R(r) => IO::R(self.g(r)),
            Src::I(i) => IO::Imm(i),
            Src::K(k) => match self.f.k[k as usize] {
                Kv::Int(v) if i32::try_from(v).is_ok() => IO::Imm(v),
                Kv::Int(_) => IO::M(kpay(k)),
                _ => panic!("float constant in int context"),
            },
        }
    }

    fn fop(&self, s: Src) -> FO {
        match s {
            Src::R(r) => FO::X(self.x(r)),
            Src::K(k) => FO::M(kpay(k)),
            Src::I(_) => panic!("immediate in float context is not supported by V2"),
        }
    }

    fn load_g(&mut self, d: Gpr, s: IO) {
        match s {
            IO::R(r) => {
                if r != d {
                    self.a.mov_rr(Size::S64, d, r)
                }
            }
            IO::Imm(i) => self.a.mov_ri(d, i),
            IO::M(m) => self.a.mov_rm(Size::S64, d, m),
        }
    }

    fn load_x(&mut self, d: Xmm, s: FO) {
        match s {
            FO::X(x) => movaps(&mut self.a, d, x),
            FO::M(m) => self.a.movsd_rm(d, m),
        }
    }

    fn ib_emit(&mut self, op: IB, d: Gpr, s: IO) {
        match (op, s) {
            (IB::Alu(al), IO::R(r)) => self.a.alu_rr(al, Size::S64, d, r),
            (IB::Alu(al), IO::Imm(i)) => self.a.alu_ri(al, Size::S64, d, i as i32),
            (IB::Alu(al), IO::M(m)) => self.a.alu_rm(al, Size::S64, d, m),
            (IB::Mul, IO::R(r)) => self.a.imul_rr(Size::S64, d, r),
            (IB::Mul, IO::Imm(i)) => self.a.imul_rri(Size::S64, d, d, i as i32),
            (IB::Mul, IO::M(m)) => self.a.imul_rm(Size::S64, d, m),
        }
    }

    /// `d = b op c` for a commutative-or-not integer operation.
    fn ibin(&mut self, op: IB, commutes: bool, d: Gpr, b: IO, c: IO) {
        let is_d = |o: IO| matches!(o, IO::R(r) if r == d);
        if is_d(c) && !is_d(b) {
            if commutes {
                self.ib_emit(op, d, b);
            } else {
                self.load_g(RAX, b);
                self.ib_emit(op, RAX, c);
                self.a.mov_rr(Size::S64, d, RAX);
            }
        } else {
            self.load_g(d, b);
            self.ib_emit(op, d, c);
        }
    }

    fn fb_emit(&mut self, op: SseOp, d: Xmm, s: FO) {
        match s {
            FO::X(x) => self.a.sse_rr(op, d, x),
            FO::M(m) => self.a.sse_rm(op, d, m),
        }
    }

    fn fbin(&mut self, op: SseOp, commutes: bool, d: Xmm, b: FO, c: FO) {
        let is_d = |o: FO| matches!(o, FO::X(x) if x == d);
        if is_d(c) && !is_d(b) {
            if commutes {
                self.fb_emit(op, d, b);
            } else {
                self.load_x(XMM0, b);
                self.fb_emit(op, XMM0, c);
                movaps(&mut self.a, d, XMM0);
            }
        } else {
            self.load_x(d, b);
            self.fb_emit(op, d, c);
        }
    }

    /// Operand converted to double in `x` (int registers via cvtsi2sd).
    fn load_conv_x(&mut self, pc: usize, x: Xmm, s: Src) {
        match s {
            Src::R(r) => {
                if self.env(pc)[r as usize] == Ty::Int {
                    self.a.logic_rr(SseLogic::Xor, x, x);
                    let g = self.g(r);
                    self.a.cvtsi2sd_rr(Size::S64, x, g);
                } else {
                    let s = self.x(r);
                    movaps(&mut self.a, x, s);
                }
            }
            Src::K(k) => match self.f.k[k as usize] {
                Kv::Int(_) => {
                    self.a.logic_rr(SseLogic::Xor, x, x);
                    self.a.cvtsi2sd_rm(Size::S64, x, kpay(k));
                }
                _ => self.a.movsd_rm(x, kpay(k)),
            },
            Src::I(i) => {
                self.a.mov_ri(RAX, (i as f64).to_bits() as i64);
                self.a.movq_xr(x, RAX);
            }
        }
    }

    fn op(&mut self, pc: usize) {
        match self.f.ops[pc] {
            Op::Move { a, b } => match self.env(pc)[b as usize] {
                Ty::Int => {
                    let (d, s) = (self.g(a), self.g(b));
                    self.load_g(d, IO::R(s));
                }
                Ty::Bool => {
                    let (d, s) = (self.gb(a), self.gb(b));
                    self.load_g(d, IO::R(s));
                }
                _ => {
                    let (d, s) = (self.x(a), self.x(b));
                    movaps(&mut self.a, d, s);
                }
            },
            Op::LoadI { a, i } => {
                let d = self.g(a);
                self.a.mov_ri(d, i);
            }
            Op::LoadF { a, f } => {
                let d = self.x(a);
                if f.to_bits() == 0 {
                    self.a.logic_rr(SseLogic::Xor, d, d);
                } else {
                    self.a.mov_ri(RAX, f.to_bits() as i64);
                    self.a.movq_xr(d, RAX);
                }
            }
            Op::LoadK { a, k } => match self.f.k[k as usize] {
                Kv::Int(v) => {
                    let d = self.g(a);
                    self.a.mov_ri(d, v);
                }
                _ => {
                    let d = self.x(a);
                    self.a.movsd_rm(d, kpay(k));
                }
            },
            Op::Mmbin => {}
            Op::Jmp { target } => {
                if !self.f.consumed_jmp(pc) {
                    self.a.jmp_label(self.lab[target]);
                }
            }
            Op::Unm { a, b } => {
                if self.env(pc)[b as usize] == Ty::Int {
                    let (d, s) = (self.g(a), self.g(b));
                    self.load_g(d, IO::R(s));
                    self.a.neg(Size::S64, d);
                } else {
                    let (d, s) = (self.x(a), self.x(b));
                    self.a.mov_ri(RAX, i64::MIN);
                    self.a.movq_xr(XMM0, RAX);
                    movaps(&mut self.a, d, s);
                    self.a.logic_rr(SseLogic::Xor, d, XMM0);
                }
            }
            Op::Arith { op, a, b, c } => self.arith(pc, op, a, b, c),
            Op::Cmp { rel, a, b, k } => self.cmp(pc, rel, a, b, k),
            Op::ForPrep { a, skip } => self.forprep(pc, a, skip),
            Op::ForLoop { a, head } => self.forloop(pc, a, head),
            Op::Return1 { a } => {
                if self.env(pc)[a as usize] == Ty::Int {
                    let g = self.g(a);
                    self.a.mov_rr(Size::S64, RAX, g);
                    self.ret_ty = Some(INT);
                } else {
                    let x = self.x(a);
                    self.a.movq_rx(RAX, x);
                    self.ret_ty = Some(FLT);
                }
                self.a.jmp_label(self.epi);
            }
            Op::Unreach => self.a.jmp_label(self.ud),
            Op::GetTable { a, b, c } => self.gettable(pc, a, b, crate::v1::KeyOp::R(c)),
            Op::GetI { a, b, i } => self.gettable(pc, a, b, crate::v1::KeyOp::Imm(i)),
            Op::SetTable { a, b, c } => self.settable(pc, a, crate::v1::KeyOp::R(b), c),
            Op::SetI { a, i, c } => self.settable(pc, a, crate::v1::KeyOp::Imm(i), c),
            Op::LoadBool { a, v } => {
                let d = self.gb(a);
                self.a.mov_ri(
                    d,
                    i64::from(rn_core::value::LUA_VFALSE | if v { 0x10 } else { 0 }),
                );
            }
            Op::Test { a, k } => {
                let Op::Jmp { target } = self.f.ops[pc + 1] else {
                    panic!("TEST not followed by JMP")
                };
                let g = self.gb(a);
                self.a.alu_ri(Alu::Cmp, Size::S32, g, 0x11); // 0x11 = true
                self.a
                    .jcc_label(if k { Cond::E } else { Cond::Ne }, self.lab[target]);
            }
            _ => unreachable!("rejected by the analysis"),
        }
    }

    fn arith(&mut self, pc: usize, op: Ar, a: u32, b: Src, c: Src) {
        let rt = arith_ty(self.f, self.env(pc), op, b, c, pc).unwrap();
        match op {
            Ar::Add | Ar::Sub | Ar::Mul => {
                if rt == Ty::Int {
                    let d = self.g(a);
                    let ibop = match op {
                        Ar::Add => IB::Alu(Alu::Add),
                        Ar::Sub => IB::Alu(Alu::Sub),
                        _ => IB::Mul,
                    };
                    let (ib, ic) = (self.iop(b), self.iop(c));
                    self.ibin(ibop, op != Ar::Sub, d, ib, ic);
                } else {
                    let d = self.x(a);
                    let sse = match op {
                        Ar::Add => SseOp::Add,
                        Ar::Sub => SseOp::Sub,
                        _ => SseOp::Mul,
                    };
                    let (fb, fc) = (self.fop(b), self.fop(c));
                    self.fbin(sse, op != Ar::Sub, d, fb, fc);
                }
            }
            Ar::Div => {
                let d = self.x(a);
                self.load_conv_x(pc, XMM0, b);
                let direct = match c {
                    Src::R(r) if self.env(pc)[r as usize] == Ty::Flt => Some(FO::X(self.x(r))),
                    Src::K(k) if matches!(self.f.k[k as usize], Kv::Flt(_)) => Some(FO::M(kpay(k))),
                    _ => None,
                };
                match direct {
                    Some(fo) => self.fb_emit(SseOp::Div, XMM0, fo),
                    None => {
                        self.load_conv_x(pc, XMM1, c);
                        self.a.sse_rr(SseOp::Div, XMM0, XMM1);
                    }
                }
                movaps(&mut self.a, d, XMM0);
            }
            Ar::IDiv | Ar::Mod => {
                let d = self.g(a);
                let (ib, ic) = (self.iop(b), self.iop(c));
                self.load_g(RAX, ib);
                self.load_g(RCX, ic);
                let cd = match ic {
                    IO::Imm(i) => Some(i),
                    _ => None,
                };
                idiv_core(&mut self.a, op == Ar::IDiv, cd, self.ud);
                self.a.mov_rr(Size::S64, d, RAX);
            }
            Ar::BAnd | Ar::BOr | Ar::BXor => {
                let d = self.g(a);
                let al = match op {
                    Ar::BAnd => Alu::And,
                    Ar::BOr => Alu::Or,
                    _ => Alu::Xor,
                };
                let (ib, ic) = (self.iop(b), self.iop(c));
                self.ibin(IB::Alu(al), true, d, ib, ic);
            }
            Ar::Shl | Ar::Shr => {
                let d = self.g(a);
                let (ib, ic) = (self.iop(b), self.iop(c));
                self.load_g(RAX, ib);
                let cd = if let (Src::I(sc), Ar::Shr) = (c, op) {
                    Some(sc)
                } else {
                    self.load_g(RCX, ic);
                    None
                };
                shift_core(&mut self.a, op == Ar::Shl, cd);
                self.a.mov_rr(Size::S64, d, RAX);
            }
        }
    }

    fn cmp(&mut self, pc: usize, rel: Rel, a: Src, b: Src, k: bool) {
        let Op::Jmp { target } = self.f.ops[pc + 1] else {
            panic!("comparison not followed by JMP")
        };
        let tgt = self.lab[target];
        let t = unify(
            src_ty(self.f, self.env(pc), a, pc).unwrap(),
            src_ty(self.f, self.env(pc), b, pc).unwrap(),
            pc,
        )
        .unwrap();
        let Src::R(ra) = a else { panic!("cmp lhs") };
        if t == Ty::Int {
            let ga = self.g(ra);
            let ib = self.iop(b);
            match ib {
                IO::R(r) => self.a.alu_rr(Alu::Cmp, Size::S64, ga, r),
                IO::Imm(i) => self.a.alu_ri(Alu::Cmp, Size::S64, ga, i as i32),
                IO::M(m) => self.a.alu_rm(Alu::Cmp, Size::S64, ga, m),
            }
            let cc = if k { int_cc(rel) } else { invert(int_cc(rel)) };
            self.a.jcc_label(cc, tgt);
        } else {
            let xa = self.x(ra);
            let xb = match b {
                Src::R(r) => self.x(r),
                Src::K(kk) => {
                    self.a.movsd_rm(XMM1, kpay(kk));
                    XMM1
                }
                Src::I(i) => {
                    self.a.mov_ri(RAX, (i as f64).to_bits() as i64);
                    self.a.movq_xr(XMM1, RAX);
                    XMM1
                }
            };
            let skip = self.a.new_label();
            fjump(&mut self.a, rel, k, xa, xb, tgt, skip);
            self.a.bind(skip);
        }
    }

    fn forprep(&mut self, pc: usize, a: u32, skip: usize) {
        let lskip = self.lab[skip];
        let ud = self.ud;
        let lneg = self.a.new_label();
        let lstore = self.a.new_label();
        if self.env(pc)[a as usize] == Ty::Int {
            // ra = init, ra+1 = limit, ra+2 = step  ->  ra = count, ra+1 = step, ra+2 = init
            let (ri, rl, rs) = (self.g(a), self.g(a + 1), self.g(a + 2));
            self.a.mov_rr(Size::S64, R10, ri);
            self.a.mov_rr(Size::S64, R11, rl);
            self.a.mov_rr(Size::S64, RCX, rs);
            self.a.test_rr(Size::S64, RCX, RCX);
            self.a.jcc_label(Cond::E, ud);
            self.a.jcc_label(Cond::S, lneg);
            self.a.alu_rr(Alu::Cmp, Size::S64, R10, R11);
            self.a.jcc_label(Cond::G, lskip);
            self.a.mov_rr(Size::S64, RAX, R11);
            self.a.alu_rr(Alu::Sub, Size::S64, RAX, R10);
            self.a.alu_ri(Alu::Cmp, Size::S64, RCX, 1);
            self.a.jcc_label(Cond::E, lstore);
            self.a.alu_rr(Alu::Xor, Size::S32, RDX, RDX);
            self.a.div(Size::S64, RCX);
            self.a.jmp_label(lstore);
            self.a.bind(lneg);
            self.a.alu_rr(Alu::Cmp, Size::S64, R10, R11);
            self.a.jcc_label(Cond::L, lskip);
            self.a.mov_rr(Size::S64, RAX, R10);
            self.a.alu_rr(Alu::Sub, Size::S64, RAX, R11);
            self.a.mov_rr(Size::S64, R11, RCX);
            self.a.inc(Size::S64, R11);
            self.a.neg(Size::S64, R11);
            self.a.inc(Size::S64, R11);
            self.a.alu_rr(Alu::Xor, Size::S32, RDX, RDX);
            self.a.div(Size::S64, R11);
            self.a.bind(lstore);
            self.a.mov_rr(Size::S64, ri, RAX);
            self.a.mov_rr(Size::S64, rl, RCX);
            self.a.mov_rr(Size::S64, rs, R10);
        } else {
            let (xi, xl, xs) = (self.x(a), self.x(a + 1), self.x(a + 2));
            movaps(&mut self.a, XMM0, xi);
            movaps(&mut self.a, XMM1, xl);
            movaps(&mut self.a, XMM2, xs);
            self.a.logic_rr(SseLogic::Xor, XMM3, XMM3);
            let nz = self.a.new_label();
            let lposf = self.a.new_label();
            self.a.ucomisd_rr(XMM2, XMM3);
            self.a.jcc_label(Cond::Ne, nz);
            self.a.jcc_label(Cond::Np, ud);
            self.a.bind(nz);
            self.a.jcc_label(Cond::A, lposf);
            self.a.ucomisd_rr(XMM1, XMM0);
            self.a.jcc_label(Cond::A, lskip);
            self.a.jmp_label(lstore);
            self.a.bind(lposf);
            self.a.ucomisd_rr(XMM0, XMM1);
            self.a.jcc_label(Cond::A, lskip);
            self.a.bind(lstore);
            movaps(&mut self.a, xi, XMM1);
            movaps(&mut self.a, xl, XMM2);
            movaps(&mut self.a, xs, XMM0);
        }
        let _ = lneg;
    }

    fn forloop(&mut self, pc: usize, a: u32, head: usize) {
        let lhead = self.lab[head];
        let next = self.lab[pc + 1];
        if self.env(pc)[a as usize] == Ty::Int {
            let (cnt, step, idx) = (self.g(a), self.g(a + 1), self.g(a + 2));
            // the count is dead after the loop, so `sub; jb` replaces `test; jz; dec`
            self.a.alu_ri(Alu::Sub, Size::S64, cnt, 1);
            self.a.jcc_label(Cond::B, next);
            self.a.alu_rr(Alu::Add, Size::S64, idx, step);
            self.a.jmp_label(lhead);
        } else {
            let (lim, step, idx) = (self.x(a), self.x(a + 1), self.x(a + 2));
            let lneg = self.a.new_label();
            let lcont = self.a.new_label();
            movaps(&mut self.a, XMM0, idx);
            self.a.sse_rr(SseOp::Add, XMM0, step);
            self.a.logic_rr(SseLogic::Xor, XMM1, XMM1);
            self.a.ucomisd_rr(step, XMM1);
            self.a.jcc_label(Cond::Be, lneg);
            self.a.ucomisd_rr(lim, XMM0);
            self.a.jcc_label(Cond::Ae, lcont);
            self.a.jmp_label(next);
            self.a.bind(lneg);
            self.a.ucomisd_rr(XMM0, lim);
            self.a.jcc_label(Cond::Ae, lcont);
            self.a.jmp_label(next);
            self.a.bind(lcont);
            movaps(&mut self.a, idx, XMM0);
            self.a.jmp_label(lhead);
        }
    }
}
