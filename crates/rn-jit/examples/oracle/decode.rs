//! Source -> `Func` (decoded subset of Lua 5.5 bytecode) for the template oracle.

use rn_compiler::testhost::TestHost;
use rn_compiler::{load, LoadMode};
use rn_core::instruction::Instruction;
use rn_core::protodata::Const;
use rn_core::ver::active::OpCode;

/// A constant of the proto.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kv {
    Int(i64),
    Flt(f64),
    /// Boolean constant (SETTABLE/SETFIELD values such as `false`).
    Bool(bool),
    /// String constant (GETFIELD/SETFIELD keys; must be a short string).
    Str,
    Other,
}

/// An instruction operand.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Src {
    /// Register.
    R(u32),
    /// Constant table entry.
    K(u32),
    /// Small signed immediate (usable as int or float).
    I(i64),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ar {
    Add,
    Sub,
    Mul,
    Div,
    IDiv,
    Mod,
    BAnd,
    BOr,
    BXor,
    Shl,
    Shr,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rel {
    Eq,
    Lt,
    Le,
    Gt,
    Ge,
}

/// Decoded instruction (targets are absolute pcs).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Op {
    Move {
        a: u32,
        b: u32,
    },
    LoadI {
        a: u32,
        i: i64,
    },
    LoadF {
        a: u32,
        f: f64,
    },
    LoadK {
        a: u32,
        k: u32,
    },
    Arith {
        op: Ar,
        a: u32,
        b: Src,
        c: Src,
    },
    Unm {
        a: u32,
        b: u32,
    },
    /// MMBIN/MMBINI/MMBINK: skipped (fast path of the preceding arithmetic op already stepped over it).
    Mmbin,
    Jmp {
        target: usize,
    },
    /// Comparison immediately followed by a JMP (consumed together). `k`: the jump is taken when the
    /// comparison result equals `k`.
    Cmp {
        rel: Rel,
        a: Src,
        b: Src,
        k: bool,
    },
    ForPrep {
        a: u32,
        skip: usize,
    },
    ForLoop {
        a: u32,
        head: usize,
    },
    Return1 {
        a: u32,
    },
    /// RETURN0 / RETURN (unreachable in the oracle programs).
    Unreach,
    /// `R[A] := R[B][R[C]]`
    GetTable {
        a: u32,
        b: u32,
        c: u32,
    },
    /// `R[A] := R[B][C]` (C is an unsigned immediate index)
    GetI {
        a: u32,
        b: u32,
        i: u32,
    },
    /// `R[A] := R[B][K[C]:string]`
    GetField {
        a: u32,
        b: u32,
        k: u32,
    },
    /// `R[A][R[B]] := RK(C)`
    SetTable {
        a: u32,
        b: u32,
        c: Src,
    },
    /// `R[A][B] := RK(C)` (B is an unsigned immediate index)
    SetI {
        a: u32,
        i: u32,
        c: Src,
    },
    /// `R[A][K[B]:string] := RK(C)`
    SetField {
        a: u32,
        k: u32,
        c: Src,
    },
    /// TEST followed by its JMP (consumed together): the jump is taken when `truthy(R[A]) == k`.
    Test {
        a: u32,
        k: bool,
    },
    /// LOADFALSE / LOADTRUE
    LoadBool {
        a: u32,
        v: bool,
    },
    /// `R[A], ... := R[A](R[A+1], ..., R[A+B-1])` with `B-1` fixed arguments and `C-1` results.
    Call {
        a: u32,
        b: u32,
        c: u32,
    },
}

pub struct Func {
    pub ops: Vec<Op>,
    pub k: Vec<Kv>,
    pub max_stack: usize,
    /// Address of the first instruction of the real `Proto` (0 for the register-only modes, which compile
    /// from a separate `TestHost` compilation); CALL stores `code_ptr + 4*(pc+1)` into `ci.savedpc`.
    pub code_ptr: usize,
    pub num_params: usize,
}

/// The RK operand C of the SET* instructions.
fn rk(i: Instruction) -> Src {
    if i.k() != 0 {
        Src::K(i.c())
    } else {
        Src::R(i.c())
    }
}

fn arith(op: Ar, i: Instruction, pc: usize, rr: bool, k: bool) -> Op {
    let _ = pc;
    let c = if rr {
        Src::R(i.c())
    } else if k {
        Src::K(i.c())
    } else {
        Src::I(i.sc() as i64)
    };
    Op::Arith {
        op,
        a: i.a(),
        b: Src::R(i.b()),
        c,
    }
}

pub fn decode(code: &[u32], k: &[Const<u32>], max_stack: usize) -> Result<Func, String> {
    let mut ops = Vec::with_capacity(code.len());
    for (pc, w) in code.iter().enumerate() {
        let i = Instruction(*w);
        let oc = i.opcode().ok_or("bad opcode")?;
        let rel = |rel, a: Src, b: Src| Op::Cmp {
            rel,
            a,
            b,
            k: i.k() != 0,
        };
        let op = match oc {
            OpCode::Move => Op::Move { a: i.a(), b: i.b() },
            OpCode::Loadi => Op::LoadI {
                a: i.a(),
                i: i.sbx() as i64,
            },
            OpCode::Loadf => Op::LoadF {
                a: i.a(),
                f: i.sbx() as f64,
            },
            OpCode::Loadk => Op::LoadK {
                a: i.a(),
                k: i.bx(),
            },
            OpCode::Add => arith(Ar::Add, i, pc, true, false),
            OpCode::Sub => arith(Ar::Sub, i, pc, true, false),
            OpCode::Mul => arith(Ar::Mul, i, pc, true, false),
            OpCode::Div => arith(Ar::Div, i, pc, true, false),
            OpCode::Idiv => arith(Ar::IDiv, i, pc, true, false),
            OpCode::Mod => arith(Ar::Mod, i, pc, true, false),
            OpCode::Band => arith(Ar::BAnd, i, pc, true, false),
            OpCode::Bor => arith(Ar::BOr, i, pc, true, false),
            OpCode::Bxor => arith(Ar::BXor, i, pc, true, false),
            OpCode::Shl => arith(Ar::Shl, i, pc, true, false),
            OpCode::Shr => arith(Ar::Shr, i, pc, true, false),
            OpCode::Addi => arith(Ar::Add, i, pc, false, false),
            OpCode::Addk => arith(Ar::Add, i, pc, false, true),
            OpCode::Subk => arith(Ar::Sub, i, pc, false, true),
            OpCode::Mulk => arith(Ar::Mul, i, pc, false, true),
            OpCode::Modk => arith(Ar::Mod, i, pc, false, true),
            OpCode::Divk => arith(Ar::Div, i, pc, false, true),
            OpCode::Idivk => arith(Ar::IDiv, i, pc, false, true),
            OpCode::Bandk => arith(Ar::BAnd, i, pc, false, true),
            OpCode::Bork => arith(Ar::BOr, i, pc, false, true),
            OpCode::Bxork => arith(Ar::BXor, i, pc, false, true),
            // R[A] := R[B] >> sC
            OpCode::Shri => arith(Ar::Shr, i, pc, false, false),
            // R[A] := sC << R[B]
            OpCode::Shli => Op::Arith {
                op: Ar::Shl,
                a: i.a(),
                b: Src::I(i.sc() as i64),
                c: Src::R(i.b()),
            },
            OpCode::Unm => Op::Unm { a: i.a(), b: i.b() },
            OpCode::Mmbin | OpCode::Mmbini | OpCode::Mmbink => Op::Mmbin,
            OpCode::Jmp => Op::Jmp {
                target: (pc as i64 + 1 + i.sj() as i64) as usize,
            },
            OpCode::Eq => rel(Rel::Eq, Src::R(i.a()), Src::R(i.b())),
            OpCode::Lt => rel(Rel::Lt, Src::R(i.a()), Src::R(i.b())),
            OpCode::Le => rel(Rel::Le, Src::R(i.a()), Src::R(i.b())),
            OpCode::Eqk => rel(Rel::Eq, Src::R(i.a()), Src::K(i.b())),
            OpCode::Eqi => rel(Rel::Eq, Src::R(i.a()), Src::I(i.sb() as i64)),
            OpCode::Lti => rel(Rel::Lt, Src::R(i.a()), Src::I(i.sb() as i64)),
            OpCode::Lei => rel(Rel::Le, Src::R(i.a()), Src::I(i.sb() as i64)),
            OpCode::Gti => rel(Rel::Gt, Src::R(i.a()), Src::I(i.sb() as i64)),
            OpCode::Gei => rel(Rel::Ge, Src::R(i.a()), Src::I(i.sb() as i64)),
            OpCode::Forprep => Op::ForPrep {
                a: i.a(),
                skip: pc + i.bx() as usize + 2,
            },
            OpCode::Forloop => Op::ForLoop {
                a: i.a(),
                head: pc + 1 - i.bx() as usize,
            },
            OpCode::Return1 => Op::Return1 { a: i.a() },
            OpCode::Gettable => Op::GetTable {
                a: i.a(),
                b: i.b(),
                c: i.c(),
            },
            OpCode::Geti => Op::GetI {
                a: i.a(),
                b: i.b(),
                i: i.c(),
            },
            OpCode::Getfield => Op::GetField {
                a: i.a(),
                b: i.b(),
                k: i.c(),
            },
            OpCode::Settable => Op::SetTable {
                a: i.a(),
                b: i.b(),
                c: rk(i),
            },
            OpCode::Seti => Op::SetI {
                a: i.a(),
                i: i.b(),
                c: rk(i),
            },
            OpCode::Setfield => Op::SetField {
                a: i.a(),
                k: i.b(),
                c: rk(i),
            },
            OpCode::Test => Op::Test {
                a: i.a(),
                k: i.k() != 0,
            },
            OpCode::Loadfalse => Op::LoadBool { a: i.a(), v: false },
            OpCode::Loadtrue => Op::LoadBool { a: i.a(), v: true },
            OpCode::Call => Op::Call {
                a: i.a(),
                b: i.b(),
                c: i.c(),
            },
            // CLOSE after `break` (5.5 emits it behind the break's JMP; it is dead code here)
            OpCode::Return0 | OpCode::Return | OpCode::Close => Op::Unreach,
            other => return Err(format!("pc {pc}: unsupported opcode {other:?}")),
        };
        ops.push(op);
    }
    let k = k
        .iter()
        .map(|c| match c {
            Const::Int(i) => Kv::Int(*i),
            Const::Float(f) => Kv::Flt(*f),
            _ => Kv::Other,
        })
        .collect();
    Ok(Func {
        ops,
        k,
        max_stack,
        code_ptr: 0,
        num_params: 0,
    })
}

/// Decodes a real `Proto` of a `State` (the State-based programs: tables and calls).
pub fn decode_proto(p: &rn_core::func::Proto) -> Result<Func, String> {
    let k: Vec<Const<u32>> = p
        .k()
        .iter()
        .map(|v| {
            if let Some(i) = v.as_int() {
                Const::Int(i)
            } else if let Some(f) = v.as_float() {
                Const::Float(f)
            } else if let Some(b) = v.as_bool() {
                if b {
                    Const::True
                } else {
                    Const::False
                }
            } else if v.is_string() {
                Const::Str(0)
            } else {
                Const::Nil
            }
        })
        .collect();
    let mut f = decode(p.code(), &k, p.maxstacksize as usize)?;
    for (kv, c) in f.k.iter_mut().zip(&k) {
        match c {
            Const::True => *kv = Kv::Bool(true),
            Const::False => *kv = Kv::Bool(false),
            Const::Str(_) => *kv = Kv::Str,
            _ => {}
        }
    }
    f.code_ptr = p.code_ptr() as usize;
    f.num_params = p.numparams as usize;
    Ok(f)
}

/// Compiles Lua source and decodes its first nested function (`local function f(N) ... end`).
pub fn compile_source(src: &[u8]) -> Result<Func, String> {
    let mut host = TestHost::default();
    let main = load(&mut host, src, b"=oracle", LoadMode::BT).map_err(|e| format!("{e:?}"))?;
    let p = main.protos.first().ok_or("no nested function")?;
    decode(&p.code, &p.k, p.max_stack_size as usize)
}

impl Func {
    /// True if the instruction at `pc` is a JMP consumed by the comparison before it.
    pub fn consumed_jmp(&self, pc: usize) -> bool {
        pc > 0 && matches!(self.ops[pc - 1], Op::Cmp { .. } | Op::Test { .. })
    }

    /// Basic block leaders.
    pub fn leaders(&self) -> Vec<bool> {
        let n = self.ops.len();
        let mut l = vec![false; n + 1];
        l[0] = true;
        for (pc, op) in self.ops.iter().enumerate() {
            match *op {
                Op::Jmp { target } => {
                    if !self.consumed_jmp(pc) {
                        l[target] = true;
                        l[pc + 1] = true;
                    }
                }
                Op::Cmp { .. } | Op::Test { .. } => {
                    if let Op::Jmp { target } = self.ops[pc + 1] {
                        l[target] = true;
                    }
                    l[pc + 2] = true;
                }
                Op::ForPrep { skip, .. } => {
                    l[skip] = true;
                    l[pc + 1] = true;
                }
                Op::ForLoop { head, .. } => {
                    l[head] = true;
                    l[pc + 1] = true;
                }
                Op::Return1 { .. } | Op::Unreach | Op::Call { .. } => l[pc + 1] = true,
                _ => {}
            }
        }
        l
    }
}
