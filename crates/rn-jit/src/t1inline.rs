//! BUNDLE-T1I: inline fast paths for the cheapest, very hot built-ins in the tier-1 `CALL` template.
//!
//! A native call through `jit_call` costs about 185 instructions (the template, the helper's native path, the library
//! body); the body of `type`, `math.floor`, `string.byte` ... is a handful of instructions. When the callee of a `CALL` is
//! not a Lua closure, the template tests its *identity* (the light C function pointer, or for `type` a C closure whose
//! function pointer is `type`'s) against the addresses registered by the library (`jit_hooks`), and for a match runs the
//! built-in's fast path inline: argument guards, the result stored exactly where `post_call` would leave it (`R[A]`),
//! `top` = the end of the result, and the result also stored in the slot above the arguments where the native would have
//! pushed it (a dead copy, so that the stack above `top` is what the generic path leaves, TR3). Anything else (a user replaced the
//! function, another argument count or type, a value the fast path does not cover, an error) takes the generic `jit_call`
//! path *with nothing changed*, so the library raises the identical error. The entry guards (hooks, Ctrl-C, instruction
//! count, `top`, `savedpc`) are those of the generic path and run before.
//!
//! The table [`BUILTINS`] is the whole design: one row per built-in with its limit bit, call shape, identity and emitter.
//! The limit `t1inline` (`CallEnv::inl`) is a bit mask of the rows (ablation); mutants flip single guards (`CallEnv::inl_mut`).

use super::{ciw, fr, thw, Cx};
use crate::ir::{Base, Cc, Mi, Op, Src, L, M, V, W, X};
use rn_core::jit_hooks::{self, libfn, thread as thr};
use rn_core::jit_layout::{callinfo as ci, cclosure, string as lstr, table};
use rn_core::func::Proto;
use rn_core::instruction::Instruction;
use rn_core::ver::active::OpCode;
use rn_core::value::{ctb, LUA_VCCL, LUA_VLCF, LUA_VLNGSTR, LUA_VNUMFLT, LUA_VNUMINT, LUA_VSHRSTR, LUA_VTABLE};

/// Limit bit of [`CallEnv::inl`](super::calls::CallEnv::inl): `type`.
pub const B_TYPE: u16 = 1;
/// `math.floor`.
pub const B_FLOOR: u16 = 2;
/// `math.abs`.
pub const B_ABS: u16 = 4;
/// `math.max`.
pub const B_MAX: u16 = 8;
/// `math.min`.
pub const B_MIN: u16 = 16;
/// `math.sqrt`.
pub const B_SQRT: u16 = 32;
/// `string.byte`.
pub const B_BYTE: u16 = 64;
/// `string.len`.
pub const B_LEN: u16 = 128;
/// `select('#', ...)`.
pub const B_SELECT: u16 = 256;
/// `rawlen`.
pub const B_RAWLEN: u16 = 512;
/// All rows.
pub const B_ALL: u16 = 1023;

/// Mutant bit of [`CallEnv::inl_mut`](super::calls::CallEnv::inl_mut) (test-only; always 0 without the `tr_mutants` feature): the `type` identity test is skipped.
pub const M_TYPE_IDENT: u16 = 1;
/// The light-C-function identity test is skipped.
pub const M_LCF_IDENT: u16 = 2;
/// `math.floor` of a float uses the truncation when the value is not exactly an integer (NaN, infinities, huge).
pub const M_FLOOR_EXACT: u16 = 4;
/// `math.max` / `math.min` of floats compares with `<=` (picks the second operand for equal values: -0.0 / 0.0).
pub const M_MM_LE: u16 = 8;
/// `string.byte` accepts an index below 1.
pub const M_BYTE_LO: u16 = 16;
/// `math.abs` of a float keeps the sign.
pub const M_ABS_FLT: u16 = 32;
/// `select('#', ...)` answers the argument count instead of count - 1.
pub const M_SELECT_N: u16 = 64;
/// `rawlen` of a table does not check that the element after the length hint is empty.
pub const M_RAWLEN_NEXT: u16 = 128;
/// `math.sqrt` of an integer reads the integer's bits as a float.
pub const M_SQRT_INT: u16 = 256;
/// `type` answers the upvalue of the *next* basic type.
pub const M_TYPE_IDX: u16 = 512;

/// The call site being compiled.
#[derive(Clone, Copy)]
pub(crate) struct Site {
    pub a: u32,
    pub b: u32,
    pub next: L,
    pub slow: L,
    pub mu: u16,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    /// A light C function: `R[A]` has tag `LUA_VLCF` and the function pointer as payload.
    Light,
    /// A C closure whose function pointer (`CClosure::f`) is the registered address.
    Closure,
}

/// Per-prototype call-site masks: key = address of the prototype's code, value = one limit mask per instruction.
pub type SiteHints = std::collections::HashMap<usize, Box<[u16]>>;

type Emit = fn(&mut Cx<'_>, &Site);

struct Builtin {
    bit: u16,
    /// Lower-case fragments of the callee names that make the row worth emitting at a site (see [`site_hints`]).
    names: &'static [&'static str],
    kind: Kind,
    /// The registered address (0 = unknown: the row is skipped).
    addr: fn() -> usize,
    /// Call shapes (`B` of the `CALL`) the emitter handles.
    shape: fn(u32) -> bool,
    emit: Emit,
}

fn a_type() -> usize {
    jit_hooks::type_fn_addr()
}
fn a_floor() -> usize {
    jit_hooks::intrinsic_addr(jit_hooks::intr::FLOOR)
}
fn a_abs() -> usize {
    jit_hooks::intrinsic_addr(jit_hooks::intr::ABS)
}
fn a_max() -> usize {
    jit_hooks::intrinsic_addr(jit_hooks::intr::MAX)
}
fn a_min() -> usize {
    jit_hooks::intrinsic_addr(jit_hooks::intr::MIN)
}
fn a_sqrt() -> usize {
    jit_hooks::intrinsic_addr(jit_hooks::intr::SQRT)
}
fn a_byte() -> usize {
    jit_hooks::libfn_addr(libfn::STR_BYTE)
}
fn a_len() -> usize {
    jit_hooks::strlen_addr()
}
fn a_select() -> usize {
    jit_hooks::select_addr()
}
fn a_rawlen() -> usize {
    jit_hooks::libfn_addr(libfn::RAWLEN)
}

fn one_arg(b: u32) -> bool {
    b == 2
}
fn two_args(b: u32) -> bool {
    b == 3
}
fn byte_shape(b: u32) -> bool {
    b == 2 || b == 3
}
fn select_shape(b: u32) -> bool {
    b == 0 || b >= 2
}

/// The rows, most frequent first (the identity tests are tried in this order).
static BUILTINS: &[Builtin] = &[
    Builtin { bit: B_TYPE, names: &["type"], kind: Kind::Closure, addr: a_type, shape: one_arg, emit: |c, s| c.t1i_type(s) },
    Builtin { bit: B_FLOOR, names: &["floor"], kind: Kind::Light, addr: a_floor, shape: one_arg, emit: |c, s| c.t1i_floor(s) },
    Builtin { bit: B_BYTE, names: &["byte"], kind: Kind::Light, addr: a_byte, shape: byte_shape, emit: |c, s| c.t1i_byte(s) },
    Builtin { bit: B_MAX, names: &["max"], kind: Kind::Light, addr: a_max, shape: two_args, emit: |c, s| c.t1i_max(s) },
    Builtin { bit: B_MIN, names: &["min"], kind: Kind::Light, addr: a_min, shape: two_args, emit: |c, s| c.t1i_min(s) },
    Builtin { bit: B_ABS, names: &["abs"], kind: Kind::Light, addr: a_abs, shape: one_arg, emit: |c, s| c.t1i_abs(s) },
    Builtin { bit: B_SQRT, names: &["sqrt"], kind: Kind::Light, addr: a_sqrt, shape: one_arg, emit: |c, s| c.t1i_sqrt(s) },
    Builtin { bit: B_LEN, names: &["len"], kind: Kind::Light, addr: a_len, shape: one_arg, emit: |c, s| c.t1i_len(s) },
    Builtin { bit: B_SELECT, names: &["select"], kind: Kind::Light, addr: a_select, shape: select_shape, emit: |c, s| c.t1i_select(s) },
    Builtin { bit: B_RAWLEN, names: &["rawlen"], kind: Kind::Light, addr: a_rawlen, shape: one_arg, emit: |c, s| c.t1i_rawlen(s) },
];

/// The limit bits whose rows exist for this process (addresses registered, CPU features present).
pub fn available() -> u16 {
    let mut m = 0;
    for r in BUILTINS {
        if usable(r) {
            m |= r.bit;
        }
    }
    m
}

fn usable(r: &Builtin) -> bool {
    (r.addr)() != 0 && (r.bit != B_FLOOR || std::is_x86_feature_detected!("sse4.1"))
}

/// The rows whose names occur in `name` (lower-cased).
fn name_mask(name: &[u8]) -> u16 {
    let lower = name.to_ascii_lowercase();
    let mut m = 0;
    for r in BUILTINS {
        if r.names.iter().any(|n| lower.windows(n.len()).any(|w| w == n.as_bytes())) {
            m |= r.bit;
        }
    }
    m
}

/// The name the code gives the callee of the `CALL` at `pc` whose function register is `reg`: the constant key of the `GETTABUP` /
/// `GETFIELD` / `SELF` that loaded it, the name of the upvalue (`GETUPVAL`) or local (`MOVE`) it was copied from. 0 = no known name
/// (or not a built-in's): the site gets no inline code.
fn callee_mask(p: &Proto, pc: usize, reg: u32, depth: u32) -> u16 {
    let code = p.code();
    let k = p.k();
    let key = |idx: u32| k.get(idx as usize).and_then(|v| v.as_string()).map_or(0, |s| name_mask(s.as_bytes()));
    for q in (pc.saturating_sub(256)..pc).rev() {
        let i = Instruction(code[q]);
        let Some(op) = i.opcode() else { continue };
        if i.a() != reg {
            continue;
        }
        return match op {
            OpCode::Gettabup | OpCode::Getfield => key(i.c()),
            OpCode::SelfOp => {
                if i.k() != 0 {
                    key(i.c())
                } else {
                    0
                }
            }
            OpCode::Getupval => p
                .upvalue_descs()
                .get(i.b() as usize)
                .and_then(|u| u.name())
                .map_or(0, |n| name_mask(n.get().as_bytes())),
            OpCode::Move => match p.local_name(i.b() as i32 + 1, pc as i32) {
                Some(n) => name_mask(n.get().as_bytes()),
                None if depth < 3 => callee_mask(p, q, i.b(), depth + 1),
                None => 0,
            },
            OpCode::Gettable
            | OpCode::Geti
            | OpCode::Loadk
            | OpCode::Loadkx
            | OpCode::Loadi
            | OpCode::Loadf
            | OpCode::Loadfalse
            | OpCode::Loadtrue
            | OpCode::Closure
            | OpCode::Newtable
            | OpCode::Call
            | OpCode::Concat
            | OpCode::Vararg => 0,
            _ => continue,
        };
    }
    0
}

/// The per-instruction masks of `p`: at a `CALL` with one or no wanted result, the rows whose names the callee carries (a stripped chunk
/// has no names: every row). A site whose callee has no recognisable name gets no inline code: the identity tests of the rows
/// cost instructions on every call of another native, so only the call sites that look like the built-in are given them.
pub fn site_hints(p: &Proto) -> Box<[u16]> {
    let code = p.code();
    let stripped = p.source().is_none();
    let mut out = vec![0u16; code.len()];
    for pc in 0..code.len() {
        let i = Instruction(code[pc]);
        if i.opcode() == Some(OpCode::Call) && (i.c() == 0 || i.c() == 2) {
            out[pc] = if stripped { B_ALL } else { callee_mask(p, pc, i.a(), 0) };
        }
    }
    out.into_boxed_slice()
}

fn m_reg(v: V, off: i32) -> M {
    M { base: Base::Reg(v), off }
}

impl Cx<'_> {
    /// The rows enabled for a call shape.
    fn t1i_rows(inl: u16, b: u32) -> Vec<&'static Builtin> {
        BUILTINS.iter().filter(|r| inl & r.bit != 0 && (r.shape)(b) && usable(r)).collect()
    }

    /// The limit mask of the rows worth emitting at the `CALL` at `pc` (the prototype's [`site_hints`]; every row without them).
    fn t1i_site_mask(&self, pc: u32) -> u16 {
        let env = self.env();
        if env.hints.is_null() {
            return B_ALL;
        }
        // SAFETY: the map lives in the state's `JitData`, which outlives every lowering (see `CallEnv::hints`).
        let m = unsafe { &*env.hints };
        m.get(&self.cfg.code_addr).map_or(B_ALL, |v| v.get(pc as usize).copied().unwrap_or(0))
    }

    /// Whether any row applies to a call with `B` = `b`, `C` = `c` (then the caller branches non-Lua-closures to the label it
    /// passes to [`Cx::t1i_emit`]).
    pub(crate) fn t1i_applies(&self, pc: u32, b: u32, c: u32) -> bool {
        let inl = self.env().inl & self.t1i_site_mask(pc);
        inl != 0 && (c == 0 || c == 2) && !Self::t1i_rows(inl, b).is_empty()
    }

    /// Emits the inline built-ins at `notlcl` (`V0` = the callee's tag byte, `V5` = `top`). Control leaves to `next` (done) or
    /// `slow` (nothing has been changed: the generic call).
    pub(crate) fn t1i_emit(&mut self, pc: u32, a: u32, b: u32, next: L, slow: L, notlcl: L) {
        let env = self.env();
        let rows = Self::t1i_rows(env.inl & self.t1i_site_mask(pc), b);
        let mu = env.inl_mut;
        let site = Site { a, b, next, slow, mu };
        self.op(Op::Bind(notlcl));
        let lights: Vec<&Builtin> = rows.iter().copied().filter(|r| r.kind == Kind::Light).collect();
        let closures: Vec<&Builtin> = rows.iter().copied().filter(|r| r.kind == Kind::Closure).collect();
        if !lights.is_empty() {
            let not_lcf = self.p.label();
            self.br(Cc::Ne, 0, Src::Imm(i32::from(LUA_VLCF)), not_lcf);
            self.ld(1, W::B8, fr(a, 8));
            for r in lights {
                let fail = self.p.label();
                if mu & M_LCF_IDENT == 0 {
                    self.op(Op::Mov(2, (r.addr)() as u64));
                    self.br(Cc::Ne, 1, Src::V(2), fail);
                }
                (r.emit)(self, &site);
                self.op(Op::Bind(fail));
            }
            self.op(Op::Jmp(slow));
            self.op(Op::Bind(not_lcf));
        }
        if !closures.is_empty() {
            self.br(Cc::Ne, 0, Src::Imm(i32::from(ctb(LUA_VCCL))), slow);
            self.ld(1, W::B8, fr(a, 8));
            for r in closures {
                let fail = self.p.label();
                if mu & M_TYPE_IDENT == 0 {
                    self.ld(2, W::B8, m_reg(1, cclosure::F as i32));
                    self.op(Op::Mov(3, (r.addr)() as u64));
                    self.br(Cc::Ne, 2, Src::V(3), fail);
                }
                (r.emit)(self, &site);
                self.op(Op::Bind(fail));
            }
        }
        self.op(Op::Jmp(slow));
    }

    /// Stores the result `(tag, payload)` in `R[A]` and the dead copy above the arguments, sets `top` after the result and
    /// leaves to the continuation.
    fn t1i_store(&mut self, s: &Site, tag: Src, pay: V, dead: bool) {
        self.op(Op::St { w: W::B8, m: fr(s.a, 0), src: tag });
        self.st(W::B8, fr(s.a, 8), pay);
        if dead {
            let d = s.a + s.b + 1;
            self.op(Op::St { w: W::B8, m: fr(d, 0), src: tag });
            self.st(W::B8, fr(d, 8), pay);
        }
        self.t1i_done(s);
    }

    fn t1i_store_int(&mut self, s: &Site, pay: V) {
        self.t1i_store(s, Src::Imm(i32::from(LUA_VNUMINT)), pay, true);
    }

    /// `top` = `R[A + 1]` (one result), continue.
    fn t1i_done(&mut self, s: &Site) {
        self.ld(5, W::B8, ciw(ci::FUNC));
        self.op(Op::Add(5, Src::Imm((s.a + 2) as i32)));
        self.st(W::B8, thw(thr::TOP), 5);
        self.op(Op::Jmp(s.next));
    }

    /// `type(x)`: the closure's upvalue of `x`'s basic type (`V1` = the closure).
    pub(crate) fn t1i_type(&mut self, s: &Site) {
        self.ld(2, W::B1, fr(s.a + 1, 0));
        self.op(Op::And(2, Src::Imm(0x0f)));
        if s.mu & M_TYPE_IDX != 0 {
            self.op(Op::Add(2, Src::Imm(1)));
        }
        self.ld(3, W::B1, m_reg(1, cclosure::NUPVALUES as i32));
        self.br(Cc::Uge, 2, Src::V(3), s.slow);
        self.op(Op::ShlI(2, 4));
        self.op(Op::Add(2, Src::V(1)));
        self.ld(3, W::B8, m_reg(2, cclosure::UPVALS as i32));
        self.ld(4, W::B8, m_reg(2, cclosure::UPVALS as i32 + 8));
        self.t1i_store(s, Src::V(3), 4, true);
    }

    /// `math.floor(x)` (`x` an integer: itself; a float whose floor is exactly an integer: that integer).
    pub(crate) fn t1i_floor(&mut self, s: &Site) {
        let flt = self.p.label();
        self.ld(2, W::B1, fr(s.a + 1, 0));
        self.ld(3, W::B8, fr(s.a + 1, 8));
        self.br(Cc::Ne, 2, Src::Imm(i32::from(LUA_VNUMINT)), flt);
        self.t1i_store_int(s, 3);
        self.op(Op::Bind(flt));
        self.br(Cc::Ne, 2, Src::Imm(i32::from(LUA_VNUMFLT)), s.slow);
        const X2: X = 2;
        self.op(Op::GtoF(X2, 3));
        self.op(Op::FFloor(X2, X2));
        if s.mu & M_FLOOR_EXACT != 0 {
            let ok = self.p.label();
            self.op(Op::F2IExact { dst: 4, src: X2, to: ok });
            self.op(Op::Bind(ok));
        } else {
            self.op(Op::F2IExact { dst: 4, src: X2, to: s.slow });
        }
        self.t1i_store_int(s, 4);
    }

    /// `math.abs(x)`.
    pub(crate) fn t1i_abs(&mut self, s: &Site) {
        let flt = self.p.label();
        self.ld(2, W::B1, fr(s.a + 1, 0));
        self.ld(3, W::B8, fr(s.a + 1, 8));
        self.br(Cc::Ne, 2, Src::Imm(i32::from(LUA_VNUMINT)), flt);
        self.op(Op::MovR(4, 3));
        self.op(Op::SarI(4, 63));
        self.op(Op::Xor(3, Src::V(4)));
        self.op(Op::Sub(3, Src::V(4)));
        self.t1i_store_int(s, 3);
        self.op(Op::Bind(flt));
        self.br(Cc::Ne, 2, Src::Imm(i32::from(LUA_VNUMFLT)), s.slow);
        if s.mu & M_ABS_FLT == 0 {
            self.op(Op::Mov(4, 0x7fff_ffff_ffff_ffff));
            self.op(Op::And(3, Src::V(4)));
        }
        self.t1i_store(s, Src::Imm(i32::from(LUA_VNUMFLT)), 3, true);
    }

    pub(crate) fn t1i_max(&mut self, s: &Site) {
        self.t1i_minmax(s, true);
    }
    pub(crate) fn t1i_min(&mut self, s: &Site) {
        self.t1i_minmax(s, false);
    }

    /// `math.max(a, b)` / `math.min(a, b)` of two integers or two floats (`imax = a; if imax < b then imax = b`,
    /// `imin = a; if b < imin then imin = b`: the result is one of the operands, copied with its tag).
    fn t1i_minmax(&mut self, s: &Site, max: bool) {
        let (ints, p1, p2) = (self.p.label(), self.p.label(), self.p.label());
        self.ld(2, W::B1, fr(s.a + 1, 0));
        self.ld(3, W::B1, fr(s.a + 2, 0));
        self.ld(4, W::B8, fr(s.a + 1, 8));
        self.ld(6, W::B8, fr(s.a + 2, 8));
        self.br(Cc::Ne, 2, Src::V(3), s.slow);
        self.br(Cc::Eq, 2, Src::Imm(i32::from(LUA_VNUMINT)), ints);
        self.br(Cc::Ne, 2, Src::Imm(i32::from(LUA_VNUMFLT)), s.slow);
        let cc = if s.mu & M_MM_LE != 0 { Cc::Le } else { Cc::Lt };
        let (x, y) = if max { (4, 6) } else { (6, 4) };
        self.op(Op::FBr { cc, a: x, b: y, to: p2 });
        self.op(Op::Jmp(p1));
        self.op(Op::Bind(ints));
        self.br(Cc::Lt, x, Src::V(y), p2);
        self.op(Op::Bind(p1));
        self.t1i_store(s, Src::V(2), 4, true);
        self.op(Op::Bind(p2));
        self.t1i_store(s, Src::V(3), 6, true);
    }

    /// `math.sqrt(x)` of a number.
    pub(crate) fn t1i_sqrt(&mut self, s: &Site) {
        let (int, go) = (self.p.label(), self.p.label());
        const X2: X = 2;
        self.ld(2, W::B1, fr(s.a + 1, 0));
        self.ld(3, W::B8, fr(s.a + 1, 8));
        self.br(Cc::Eq, 2, Src::Imm(i32::from(LUA_VNUMINT)), int);
        self.br(Cc::Ne, 2, Src::Imm(i32::from(LUA_VNUMFLT)), s.slow);
        self.op(Op::GtoF(X2, 3));
        self.op(Op::Jmp(go));
        self.op(Op::Bind(int));
        if s.mu & M_SQRT_INT != 0 {
            self.op(Op::GtoF(X2, 3));
        } else {
            self.op(Op::I2Fx(X2, 3));
        }
        self.op(Op::Bind(go));
        self.op(Op::FSqrt(X2, X2));
        self.op(Op::FtoG(4, X2));
        self.t1i_store(s, Src::Imm(i32::from(LUA_VNUMFLT)), 4, true);
    }

    /// `string.byte(s)` / `string.byte(s, i)` of a short string with `1 <= i <= #s`: one result.
    pub(crate) fn t1i_byte(&mut self, s: &Site) {
        self.ld(2, W::B1, fr(s.a + 1, 0));
        self.br(Cc::Ne, 2, Src::Imm(i32::from(ctb(LUA_VSHRSTR))), s.slow);
        self.ld(1, W::B8, fr(s.a + 1, 8));
        self.ld(3, W::B1, m_reg(1, lstr::SHRLEN as i32));
        if s.b == 3 {
            self.ld(2, W::B1, fr(s.a + 2, 0));
            self.br(Cc::Ne, 2, Src::Imm(i32::from(LUA_VNUMINT)), s.slow);
            self.ld(2, W::B8, fr(s.a + 2, 8));
        } else {
            self.op(Op::Mov(2, 1));
        }
        if s.mu & M_BYTE_LO == 0 {
            self.br(Cc::Lt, 2, Src::Imm(1), s.slow);
        }
        self.br(Cc::Gt, 2, Src::V(3), s.slow);
        self.op(Op::Add(1, Src::V(2)));
        self.ld(4, W::B1, m_reg(1, lstr::CONTENTS as i32 - 1));
        self.t1i_store_int(s, 4);
    }

    /// `string.len(s)` of a string.
    pub(crate) fn t1i_len(&mut self, s: &Site) {
        let lng = self.p.label();
        self.ld(2, W::B1, fr(s.a + 1, 0));
        self.ld(1, W::B8, fr(s.a + 1, 8));
        self.br(Cc::Ne, 2, Src::Imm(i32::from(ctb(LUA_VSHRSTR))), lng);
        self.ld(3, W::B1, m_reg(1, lstr::SHRLEN as i32));
        self.t1i_store_int(s, 3);
        self.op(Op::Bind(lng));
        self.br(Cc::Ne, 2, Src::Imm(i32::from(ctb(LUA_VLNGSTR))), s.slow);
        self.ld(3, W::B8, m_reg(1, lstr::LNGLEN as i32));
        self.t1i_store_int(s, 3);
    }

    /// `select('#', ...)`: the number of arguments minus one (`B == 0`: from `top`).
    pub(crate) fn t1i_select(&mut self, s: &Site) {
        let dyn_top = s.b == 0;
        if dyn_top {
            // V5 = top (slot index); V6 = top - func; V3 = nargs - 1 = top - func - (A + 3), must be >= 0 (an argument exists)
            self.ld(2, W::B8, ciw(ci::FUNC));
            self.op(Op::MovR(6, 5));
            self.op(Op::Sub(6, Src::V(2)));
            self.op(Op::MovR(3, 6));
            self.op(Op::Sub(3, Src::Imm((s.a + 3) as i32)));
            self.br(Cc::Lt, 3, Src::Imm(0), s.slow);
            // the generic path wants `LUA_MINSTACK` free slots above `top`
            self.ld(4, W::B8, thw(thr::STACK_SIZE));
            self.op(Op::Sub(4, Src::V(5)));
            self.br(Cc::Le, 4, Src::Imm(rn_core::state::LUA_MINSTACK as i32), s.slow);
        } else {
            self.op(Op::Mov(3, u64::from(s.b - 2)));
        }
        self.ld(2, W::B1, fr(s.a + 1, 0));
        self.br(Cc::Ne, 2, Src::Imm(i32::from(ctb(LUA_VSHRSTR))), s.slow);
        self.ld(1, W::B8, fr(s.a + 1, 8));
        self.ld(2, W::B1, m_reg(1, lstr::SHRLEN as i32));
        self.br(Cc::Lt, 2, Src::Imm(1), s.slow);
        self.ld(2, W::B1, m_reg(1, lstr::CONTENTS as i32));
        self.br(Cc::Ne, 2, Src::Imm(i32::from(b'#')), s.slow);
        if s.mu & M_SELECT_N != 0 {
            self.op(Op::Add(3, Src::Imm(1)));
        }
        if dyn_top {
            // the dead copy goes to the slot `top` (the native pushed there): frame base + 16 * (top - func - 1)
            self.op(Op::ShlI(6, 4));
            self.op(Op::Lea(4, fr(0, -16)));
            self.op(Op::Add(4, Src::V(6)));
            self.st_imm(W::B8, m_reg(4, 0), i32::from(LUA_VNUMINT));
            self.st(W::B8, m_reg(4, 8), 3);
            self.t1i_store(s, Src::Imm(i32::from(LUA_VNUMINT)), 3, false);
        } else {
            self.t1i_store_int(s, 3);
        }
    }

    /// `rawlen(v)` of a string, or (5.5) of a table whose length hint is a border (`t[h] ~= nil`, `t[h + 1] == nil`, `h < asize`): the
    /// library's `luaH_getn` then returns `h` and leaves the hint as it is.
    pub(crate) fn t1i_rawlen(&mut self, s: &Site) {
        let (shr, lng, tbl) = (self.p.label(), self.p.label(), self.p.label());
        self.ld(2, W::B1, fr(s.a + 1, 0));
        self.ld(1, W::B8, fr(s.a + 1, 8));
        self.br(Cc::Eq, 2, Src::Imm(i32::from(ctb(LUA_VSHRSTR))), shr);
        self.br(Cc::Eq, 2, Src::Imm(i32::from(ctb(LUA_VLNGSTR))), lng);
        if !rn_core::jit_layout::LUA54 {
            self.br(Cc::Eq, 2, Src::Imm(i32::from(ctb(LUA_VTABLE))), tbl);
        }
        self.op(Op::Jmp(s.slow));
        self.op(Op::Bind(shr));
        self.ld(3, W::B1, m_reg(1, lstr::SHRLEN as i32));
        self.t1i_store_int(s, 3);
        self.op(Op::Bind(lng));
        self.ld(3, W::B8, m_reg(1, lstr::LNGLEN as i32));
        self.t1i_store_int(s, 3);
        self.op(Op::Bind(tbl));
        if rn_core::jit_layout::LUA54 {
            self.op(Op::Jmp(s.slow));
            return;
        }
        self.ld(2, W::B4, m_reg(1, table::ASIZE as i32));
        self.br(Cc::Eq, 2, Src::Imm(0), s.slow);
        self.ld(3, W::B8, m_reg(1, table::ARRAY as i32));
        self.ld(4, W::B4, m_reg(3, table::LENHINT as i32));
        self.br(Cc::Eq, 4, Src::Imm(0), s.slow);
        let full = self.p.label();
        self.br(Cc::Eq, 4, Src::V(2), full);
        self.br(Cc::Uge, 4, Src::V(2), s.slow);
        let tag = |off: i32| Mi { base: 3, idx: 4, scale: 1, off };
        self.op(Op::LdI { dst: 6, w: W::B1, m: tag(table::ARRAY_TAG_BASE as i32 - 1) });
        self.op(Op::And(6, Src::Imm(0x0f)));
        self.br(Cc::Eq, 6, Src::Imm(0), s.slow);
        if s.mu & M_RAWLEN_NEXT == 0 {
            self.op(Op::LdI { dst: 6, w: W::B1, m: tag(table::ARRAY_TAG_BASE as i32) });
            self.op(Op::And(6, Src::Imm(0x0f)));
            self.br(Cc::Ne, 6, Src::Imm(0), s.slow);
        }
        self.t1i_store_int(s, 4);
        // the hint is the array size: the last element is present and the hash part is the dummy node (no `asize + 1` key): the length
        // is `asize` and the hint stays
        self.op(Op::Bind(full));
        self.op(Op::LdI { dst: 6, w: W::B1, m: tag(table::ARRAY_TAG_BASE as i32 - 1) });
        self.op(Op::And(6, Src::Imm(0x0f)));
        self.br(Cc::Eq, 6, Src::Imm(0), s.slow);
        self.ld(6, W::B1, m_reg(1, table::FLAGS as i32));
        self.op(Op::And(6, Src::Imm(i32::from(table::BITDUMMY))));
        self.br(Cc::Eq, 6, Src::Imm(0), s.slow);
        self.t1i_store_int(s, 4);
    }
}
