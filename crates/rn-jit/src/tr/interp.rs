//! The reference IR interpreter (docs/JIT_TRACE.md 5.7, TR28): every op's semantics in plain Rust on a model frame and
//! a model heap. It is the single reference semantics of the IR: the optimisation passes are tested against it
//! (before / after must give the same exits and final states), and the register allocator and the assembler will be
//! tested against it. Pure Rust, no pointers: Miri-runnable.
//!
//! Float operations copy the x86 SSE rules of `emu.rs` (the emulator's explicit NaN rules): a NaN first operand wins,
//! else a NaN second operand (either quieted); an invalid operation gives the default NaN; `Fmin` / `Fmax` return the
//! second operand unless the first is strictly smaller / larger. Integer operations are `rn_core::number`'s.
//!
//! Value encoding: a value is a `u64`: integers and `I64` as two's complement, floats as bits, `U32` zero extended,
//! `nil` / `false` / `true` as 0; tables and closures are model handles (index + 1); an array address ([`TOp::Aref`])
//! or an upvalue address ([`TOp::Uref`]) is `handle << 32 | index`.
//!
//! The model: a frame of tagged slots, tables with an array part, closures with upvalues, the budget word and a poll
//! script. A guard failure ends the run with [`Outcome::Exit`], carrying the snapshot index and the snapshot's slots
//! resolved to values (what the exit would write to the stack, 5.5). The loop (`LOOP` / `PHI`) runs the body up to
//! [`Model::max_iters`] times.

use super::fold::eval_fcmp;
use super::ir::*;
use super::mutants;
use rn_core::number::{float_to_int, fmod_lua, idiv, imod, pow_lua, shiftl, shiftr, F2I};

/// A tagged model value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Val {
    /// Type (tag).
    pub t: Ty,
    /// Payload.
    pub bits: u64,
}

impl Val {
    /// Integer value.
    pub fn int(i: i64) -> Val {
        Val {
            t: Ty::Int,
            bits: i as u64,
        }
    }
    /// Float value.
    pub fn num(f: f64) -> Val {
        Val {
            t: Ty::Num,
            bits: f.to_bits(),
        }
    }
    /// `nil`.
    pub const NIL: Val = Val {
        t: Ty::Nil,
        bits: 0,
    };
}

/// A model table: the array part and the header words `Fload` reads.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MTable {
    /// Array part.
    pub array: Vec<Val>,
    /// Metatable word.
    pub metatable: u64,
    /// Metamethod absence flags.
    pub flags: u8,
    /// log2 of the node vector size.
    pub lsizenode: u8,
    /// 5.4 (TR30): the array limit `alimit`, kept in the `u32` hint slot at `array + 0`; [`TOp::Atouch`] updates it
    /// when `flags & BITRAS` (bit 7 of `flags`) is set. Unused (0) in the 5.5 build.
    pub lenhint: u32,
    /// F1: the hash nodes the trace has looked at, by node index (sparse: the recorder copies a node when the IR
    /// first names it, tests fill the ones they use). A node that is not here has no key: [`TOp::Hrefk`] on it exits.
    pub nodes: std::collections::BTreeMap<u32, MNode>,
}

/// A model hash node (F1): the identity of its short-string key (the string's address; `0` = no such key) and its
/// value (`nil` for a key whose value was removed).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MNode {
    /// Key identity (address of the interned string).
    pub key: u64,
    /// Value.
    pub val: Val,
    /// Chain offset (`Node::next`, in nodes). [`NEXT_UNKNOWN`] after an insertion into the table (relocation moves chain links
    /// the model does not track): an [`TOp::Hmiss`] on such a node exits.
    pub next: i32,
    /// H1: the key is an integer (`key` = its bits, `0` is a real key), not a short string's address.
    pub int: bool,
}

/// [`MNode::next`] of a node whose chain link the model lost track of.
pub const NEXT_UNKNOWN: i32 = i32::MIN;

/// The 5.4 `BITRAS` bit of [`MTable::flags`] (`rn_core::table::BITRAS`; the array limit is a hint, not the size).
pub const MODEL_BITRAS: u8 = 1 << 7;

/// The [`TOp::Uvrel`] of an upvalue the model has no entry for: a closed upvalue (outside every frame).
pub const UV_REL_CLOSED: u64 = 1 << 62;

/// The model state a trace runs on (and leaves behind).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Model {
    /// The frame (`Sload` reads it; traces never write it, only exits do).
    pub slots: Vec<Val>,
    /// Tables; handle `h` is `tables[h - 1]`.
    pub tables: Vec<MTable>,
    /// Closures, as their upvalue values; handle `h` is `closures[h - 1]`.
    pub closures: Vec<Vec<Val>>,
    /// [`TOp::Uvrel`]: per closure and upvalue, the address of the upvalue's value relative to the root base
    /// (`uv.v - rbx`, wrapping). A missing entry means "closed, far away from the frame" ([`UV_REL_CLOSED`]).
    pub uv_rel: Vec<Vec<u64>>,
    /// Shared upvalue cells (P3d): `uv_cell[h - 1][i]` is `c + 1` when upvalue `i` of closure `h` is the real `UpVal` that
    /// the model keeps in `cells[c]` (two closures that share one `UpVal` share one cell); 0 or a missing entry means the
    /// pair is its own cell, `closures[h - 1][i]`. Read and written only through [`Model::upval`] / the `ULOAD` / `USTORE`
    /// ops. Default empty = the old one-cell-per-pair model.
    pub uv_cell: Vec<Vec<u32>>,
    /// The shared upvalue cells (see `uv_cell`).
    pub cells: Vec<Val>,
    /// Prototype address of each closure (handle `h` is `cl_proto[h - 1]`): when non-empty, `EQREF closure, KPTR(proto)`
    /// compares this address with the constant (the callee-identity guard of the recorder); empty = the closure handle
    /// is compared with the constant itself (the old behaviour).
    pub cl_proto: Vec<u64>,
    /// The budget word (`vb`).
    pub budget: i64,
    /// `Some(n)`: the n-th `Poll` (1-based) exits.
    pub poll_exit: Option<u32>,
    /// Back-edges to take before the run ends with [`Outcome::Done`].
    pub max_iters: u32,
    /// F3 class-A helpers: when set, [`TOp::Tnew`] and [`TOp::Newref`] "do nothing" (return 0), so the guard that
    /// follows them exits (a GC step would be due / the allocation could fail).
    pub alloc_fails: bool,
    /// F3: the node index the next [`TOp::Newref`] inserts at (the real hash placement is the VM's, the recorder
    /// reads it from the table after the instruction ran); `None`: the smallest index with no model node.
    pub newref_ix: Option<u32>,
    /// F3: the result of the next [`TOp::Tlen`] (the real border depends on hidden state: the length hint; the
    /// recorder reads it from the VM); `None`: the number of leading non-nil elements of the model array.
    pub len_result: Option<u64>,
    /// F3b: the `(key, value)` the next [`TOp::Tnext`] finds (the traversal order is the table's; the recorder computes it
    /// from the VM); `None`: the first non-nil element of the model array at or after the control key's position.
    pub next_result: Option<(Val, Val)>,
    /// F3b: the pseudo node the last [`TOp::Tnext`] filled (read by [`TOp::Hload`] of the address [`NEXT_CELL`]).
    pub next_cell: Option<(Val, Val)>,
    /// F3b: the result of the next [`TOp::Snew`] / [`TOp::Concat`] / P-B [`TOp::Lib`] (a string address: the real one is the VM's, the
    /// recorder reads it from the VM); `None`: an arbitrary non-zero address.
    pub str_result: Option<u64>,
    /// F3b: the integer the next [`TOp::Strto`] yields (the model holds no string contents; the recorder converts the
    /// VM's string itself); `None`: the helper "did nothing" (returns 0).
    pub strto_result: Option<Val>,
    /// N2b: the values of the caller's registers `0..A+1` (below the returning frame's base) that the next [`TOp::Retf`] prepends to
    /// `slots` (its `b` operand is `A + 1`); a length that differs from `b` is bad IR.
    pub retf_below: Vec<Val>,
    /// N2b: the guards of the next [`TOp::Retf`] fail (the caller identity does not match): it exits through its snapshot.
    pub retf_fail: bool,
    /// N2b: slots between the current root frame and the entry root (the sum of the `b` operands of the [`TOp::Retf`]s executed):
    /// [`TOp::Uvrel`] is `uv_rel + 16 * shift` (`uv_rel` stays relative to the entry base).
    pub shift: u16,
}

/// F3b: the address [`TOp::Tnext`] returns in the model (the real one is a word of the exit area).
pub const NEXT_CELL: u64 = u64::MAX - 7;
/// DYNMASK: the address a [`HREF_NIL`] [`TOp::Hrefv`] yields in the model when no known node holds the key (the real one is [`NIL_NODE`]).
pub const NIL_CELL: u64 = u64::MAX - 15;

impl Model {
    /// An empty model with a large budget.
    pub fn new() -> Model {
        Model {
            slots: Vec::new(),
            tables: Vec::new(),
            closures: Vec::new(),
            uv_rel: Vec::new(),
            uv_cell: Vec::new(),
            cells: Vec::new(),
            cl_proto: Vec::new(),
            budget: 1 << 20,
            poll_exit: None,
            max_iters: 0,
            alloc_fails: false,
            newref_ix: None,
            len_result: None,
            next_result: None,
            next_cell: None,
            str_result: None,
            strto_result: None,
            retf_below: Vec::new(),
            retf_fail: false,
            shift: 0,
        }
    }
}

impl Model {
    fn cell_of(&self, h: usize, i: usize) -> Option<usize> {
        match self.uv_cell.get(h).and_then(|c| c.get(i)) {
            Some(&c) if c != 0 => Some(c as usize - 1),
            _ => None,
        }
    }

    /// The identity of the upvalue named by a `UREF` value (`handle << 32 | index`): its shared cell (bit 63 set) or the pair itself.
    pub fn uv_identity(&self, uref: u64) -> u64 {
        let (h, ix) = (uref >> 32, (uref & 0xFFFF_FFFF) as usize);
        match h.checked_sub(1).and_then(|hx| self.cell_of(hx as usize, ix)) {
            Some(c) => 1 << 63 | c as u64,
            None => uref,
        }
    }

    /// The value of upvalue `i` of closure handle `h` (1-based), through its shared cell when it has one.
    pub fn upval(&self, h: u64, i: usize) -> Option<Val> {
        let hx = (h.checked_sub(1)?) as usize;
        match self.cell_of(hx, i) {
            Some(c) => self.cells.get(c).copied(),
            None => self.closures.get(hx)?.get(i).copied(),
        }
    }

    fn upval_mut(&mut self, h: u64, i: usize) -> Option<&mut Val> {
        let hx = (h.checked_sub(1)?) as usize;
        match self.cell_of(hx, i) {
            Some(c) => self.cells.get_mut(c),
            None => self.closures.get_mut(hx)?.get_mut(i),
        }
    }
}

impl Default for Model {
    fn default() -> Model {
        Model::new()
    }
}

/// How a run ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// The end of the trace (or `max_iters` back-edges) was reached; `iters` is the number of back-edges taken.
    Done {
        /// Back-edges taken.
        iters: u32,
    },
    /// A guard failed: its snapshot and the resolved `(slot, flags, value)` entries of that snapshot.
    Exit {
        /// Snapshot index.
        snap: u32,
        /// Snapshot slots, resolved.
        slots: Vec<(u16, u8, Val)>,
    },
}

/// The trace is not valid IR (the interpreter's input contract).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BadIr(pub String);

fn bad<T>(s: impl Into<String>) -> Result<T, BadIr> {
    Err(BadIr(s.into()))
}

/// `Calln` helper numbers (`aux`).
pub const CALLN_POW: u32 = 0;
/// `fmod_lua(a, b)`.
pub const CALLN_FMOD: u32 = 1;
/// `floor(a)`.
pub const CALLN_FLOOR: u32 = 2;
/// P-B: C `fmod(a, b)` (`math.fmod` of two floats; Rust's `%`).
pub const CALLN_CFMOD: u32 = 3;
/// F1: `math.sin(a)` (`f64::sin`).
pub const CALLN_SIN: u32 = 4;
/// F1: `math.cos(a)`.
pub const CALLN_COS: u32 = 5;
/// F1: `math.exp(a)`.
pub const CALLN_EXP: u32 = 6;
/// F1: `math.log(a)` (one argument: `f64::ln`).
pub const CALLN_LOG: u32 = 7;

const SIGN: u64 = 1 << 63;
const QNAN_BIT: u64 = 0x0008_0000_0000_0000;
const DEFAULT_NAN: u64 = 0xFFF8_0000_0000_0000;

/// One double op with the x86 NaN rules of `emu.rs::fbin`.
pub(crate) fn fbin(op: TOp, a: u64, b: u64) -> u64 {
    let (x, y) = (f64::from_bits(a), f64::from_bits(b));
    match op {
        TOp::Fmin => return if x < y { a } else { b },
        TOp::Fmax => return if x > y { a } else { b },
        _ => {}
    }
    if x.is_nan() {
        return a | QNAN_BIT;
    }
    if y.is_nan() {
        return b | QNAN_BIT;
    }
    let r = match op {
        TOp::Fadd => x + y,
        TOp::Fsub => x - y,
        TOp::Fmul => x * y,
        TOp::Fdiv => x / y,
        _ => unreachable!(),
    };
    canon(r)
}

fn canon(r: f64) -> u64 {
    if r.is_nan() {
        DEFAULT_NAN
    } else {
        r.to_bits()
    }
}

/// F1: `roundsd` mode 9 on a bit pattern (a NaN is quieted).
fn ffloor(a: u64) -> u64 {
    let x = f64::from_bits(a);
    if x.is_nan() {
        a | QNAN_BIT
    } else {
        x.floor().to_bits()
    }
}

fn fsqrt(a: u64) -> u64 {
    let x = f64::from_bits(a);
    if x.is_nan() {
        a | QNAN_BIT
    } else if x < 0.0 {
        DEFAULT_NAN
    } else {
        x.sqrt().to_bits()
    }
}

/// The payload bits of a constant.
pub fn kbits(k: KVal) -> u64 {
    match k {
        KVal::Int(i) => i as u64,
        KVal::Num(b) => b,
        KVal::Pri(_) => 0,
        KVal::Ptr(a, _) => a,
    }
}

/// Result of [`run`].
pub type RunResult = Result<Outcome, BadIr>;

/// Runs `ir` on `m`. See the module documentation for the semantics.
pub fn run(ir: &TraceIr, m: &mut Model) -> RunResult {
    let mut v = Vec::new();
    run_buf(ir, m, &mut v)
}

/// BUNDLE-C: [`run`] with a caller-owned value buffer (the recorder's lockstep evaluates one instruction at a time and reuses it).
pub fn run_buf(ir: &TraceIr, m: &mut Model, v: &mut Vec<u64>) -> RunResult {
    let n = ir.ins.len();
    v.clear();
    v.resize(n, 0);
    let mut carried: Option<Vec<u64>> = None;
    let loop_at = ir.loop_at.map(|r| (r - REF_BIAS) as usize);
    let mut pc = 0usize;
    let mut iters = 0u32;
    let mut polls = 0u32;

    macro_rules! val {
        ($r:expr) => {{
            let r: Ref = $r;
            if r == NOREF {
                return bad("NOREF operand");
            } else if r < REF_BIAS {
                kbits(ir.k[usize::from(r) - 1])
            } else {
                v[(r - REF_BIAS) as usize]
            }
        }};
    }
    macro_rules! exit {
        ($snap:expr) => {{
            let snap = $snap;
            if snap as usize >= ir.snaps.len() {
                return bad(format!("snapshot {snap} out of range"));
            }
            let slots = ir
                .snap_entries(snap)
                .iter()
                .map(|e| {
                    let bits = if e.r == NOREF {
                        0
                    } else if e.r < REF_BIAS {
                        kbits(ir.k[usize::from(e.r) - 1])
                    } else {
                        v[(e.r - REF_BIAS) as usize]
                    };
                    let t = if e.r == NOREF { Ty::Nil } else { ir.ty_of(e.r) };
                    (e.slot, e.flags, Val { t, bits })
                })
                .collect();
            return Ok(Outcome::Exit { snap, slots });
        }};
    }

    loop {
        if pc == n {
            match loop_at {
                Some(l) if iters < m.max_iters => {
                    let mut c = vec![0u64; n];
                    for (p, i) in ir.ins.iter().enumerate().skip(l + 1) {
                        if i.op == TOp::Phi {
                            c[p] = val!(i.b);
                        }
                    }
                    carried = Some(c);
                    iters += 1;
                    pc = l + 1;
                    continue;
                }
                _ => return Ok(Outcome::Done { iters }),
            }
        }
        let i = ir.ins[pc];
        let snap = aux_snap(i.aux);
        let bits = aux_bits(i.aux);
        let r: u64 = match i.op {
            TOp::Kint | TOp::Knum | TOp::Kpri | TOp::Kptr => val!(i.a),
            TOp::Sload => {
                let Some(s) = m.slots.get(usize::from(i.a)) else {
                    return bad(format!("slot {} out of range", i.a));
                };
                if bits & 1 != 0 && s.t != i.t {
                    exit!(snap);
                }
                s.bits
            }
            TOp::Add => (val!(i.a) as i64).wrapping_add(val!(i.b) as i64) as u64,
            TOp::Sub => (val!(i.a) as i64).wrapping_sub(val!(i.b) as i64) as u64,
            TOp::Mul => (val!(i.a) as i64).wrapping_mul(val!(i.b) as i64) as u64,
            TOp::Neg => (val!(i.a) as i64).wrapping_neg() as u64,
            TOp::Band => val!(i.a) & val!(i.b),
            TOp::Bor => val!(i.a) | val!(i.b),
            TOp::Bxor => val!(i.a) ^ val!(i.b),
            TOp::Bnot => !val!(i.a),
            TOp::Shl => shiftl(val!(i.a) as i64, val!(i.b) as i64) as u64,
            TOp::Shr => shiftr(val!(i.a) as i64, val!(i.b) as i64) as u64,
            TOp::Idiv | TOp::Mod => {
                let (x, y) = (val!(i.a) as i64, val!(i.b) as i64);
                let q = if i.op == TOp::Idiv {
                    idiv(x, y)
                } else {
                    imod(x, y)
                };
                match q {
                    Some(q) => q as u64,
                    None if i.flags & F_GUARD != 0 => exit!(snap),
                    None => return bad("division by zero without guard"),
                }
            }
            TOp::Fadd | TOp::Fsub | TOp::Fmul | TOp::Fdiv | TOp::Fmin | TOp::Fmax => {
                fbin(i.op, val!(i.a), val!(i.b))
            }
            TOp::Fneg => val!(i.a) ^ SIGN,
            TOp::Fabs => val!(i.a) & !SIGN,
            TOp::Fsqrt => fsqrt(val!(i.a)),
            TOp::Ffloor => ffloor(val!(i.a)),
            TOp::ConvNumInt => ((val!(i.a) as i64) as f64).to_bits(),
            TOp::F2iExact => match float_to_int(f64::from_bits(val!(i.a)), F2I::Eq) {
                Some(x) => x as u64,
                None if i.flags & F_GUARD != 0 => exit!(snap),
                None => return bad("inexact F2I without guard"),
            },
            TOp::Eq | TOp::Ne | TOp::Lt | TOp::Le | TOp::Gt | TOp::Ge | TOp::Ult | TOp::Uge => {
                let (mut a, mut b) = (val!(i.a), val!(i.b));
                // P2 bundle 2 (`uvsep`): two `UREF`s are compared as the real `UpVal`s they name (the shared model cell), not as `(closure, index)` pairs
                if matches!(i.op, TOp::Eq | TOp::Ne)
                    && TraceIr::is_ins(i.a)
                    && TraceIr::is_ins(i.b)
                    && ir.at(i.a).op == TOp::Uref
                    && ir.at(i.b).op == TOp::Uref
                {
                    a = m.uv_identity(a);
                    b = m.uv_identity(b);
                }
                let ok = match i.op {
                    TOp::Eq => a == b,
                    TOp::Ne => a != b,
                    TOp::Lt => (a as i64) < (b as i64),
                    TOp::Le => (a as i64) <= (b as i64),
                    TOp::Gt => (a as i64) > (b as i64),
                    TOp::Ge => (a as i64) >= (b as i64),
                    TOp::Ult => a < b,
                    _ => a >= b,
                };
                if !ok {
                    exit!(snap);
                }
                0
            }
            TOp::Fcmp => {
                let (a, b) = (f64::from_bits(val!(i.a)), f64::from_bits(val!(i.b)));
                if eval_fcmp(bits & 15, a, b) != (bits >> 4 & 1 != 0) {
                    exit!(snap);
                }
                0
            }
            TOp::Eqref => {
                let (x, y) = (val!(i.a), val!(i.b));
                let x = if !m.cl_proto.is_empty()
                    && i.b != NOREF
                    && i.b < REF_BIAS
                    && matches!(ir.k[usize::from(i.b) - 1], KVal::Ptr(_, PtrKind::Proto))
                {
                    match x.checked_sub(1).and_then(|h| m.cl_proto.get(h as usize)) {
                        Some(&p) => p,
                        None => return bad("EQREF proto of an unknown closure"),
                    }
                } else {
                    x
                };
                if x != y {
                    exit!(snap);
                }
                0
            }
            TOp::Fload => {
                let h = val!(i.a);
                let Some(t) = h.checked_sub(1).and_then(|x| m.tables.get(x as usize)) else {
                    return bad(format!("bad table handle {h}"));
                };
                match i.b {
                    0 => t.array.len() as u64,
                    1 | 5 => h,
                    2 => t.metatable,
                    3 => u64::from(t.flags),
                    4 => u64::from(t.lsizenode),
                    f => return bad(format!("bad table field {f}")),
                }
            }
            TOp::Aref | TOp::Uref => val!(i.a) << 32 | (val!(i.b) & 0xFFFF_FFFF),
            TOp::Atouch => {
                // 5.4 `alimit_touch(t, u)` (`table.rs`): `u` (0-based, already known `< asize`) at or past the limit,
                // and `u & ~(lim - 1) < lim`, makes `u + 1` the new limit; only while `BITRAS` is set.
                let (h, u) = (val!(i.a), val!(i.b));
                let Some(t) = h.checked_sub(1).and_then(|x| m.tables.get_mut(x as usize)) else {
                    return bad(format!("bad table handle {h}"));
                };
                if t.flags & MODEL_BITRAS != 0 {
                    let lim = u64::from(t.lenhint);
                    if u >= lim && (u & !lim.wrapping_sub(1)) < lim {
                        t.lenhint = (u + 1) as u32;
                    }
                }
                0
            }
            TOp::Aload => {
                let (h, ix) = (val!(i.a) >> 32, (val!(i.a) & 0xFFFF_FFFF) as usize);
                let Some(e) = elem(m, h, ix) else {
                    return bad("array load out of range");
                };
                if bits & 1 != 0 && e.t != i.t && !(i.flags & F_ANYSTR != 0 && is_str_ty(e.t)) {
                    exit!(snap);
                }
                e.bits
            }
            TOp::Astore => {
                let (h, ix) = (val!(i.a) >> 32, (val!(i.a) & 0xFFFF_FFFF) as usize);
                let nv = Val {
                    t: ir.ty_of(i.b),
                    bits: val!(i.b),
                };
                match h
                    .checked_sub(1)
                    .and_then(|x| m.tables.get_mut(x as usize))
                    .and_then(|t| t.array.get_mut(ix))
                {
                    Some(e) => *e = nv,
                    None => return bad("array store out of range"),
                }
                0
            }
            TOp::Uload => {
                let (h, ix) = (val!(i.a) >> 32, (val!(i.a) & 0xFFFF_FFFF) as usize);
                match m.upval(h, ix) {
                    Some(e) => {
                        if bits & 1 != 0 && e.t != i.t {
                            exit!(snap);
                        }
                        e.bits
                    }
                    None => return bad("upvalue load out of range"),
                }
            }
            TOp::Ustore => {
                let (h, ix) = (val!(i.a) >> 32, (val!(i.a) & 0xFFFF_FFFF) as usize);
                let nv = Val {
                    t: ir.ty_of(i.b),
                    bits: val!(i.b),
                };
                match m.upval_mut(h, ix) {
                    Some(e) => *e = nv,
                    None => return bad("upvalue store out of range"),
                }
                0
            }
            TOp::Uvrel => {
                let (h, ix) = (val!(i.a) >> 32, (val!(i.a) & 0xFFFF_FFFF) as usize);
                h.checked_sub(1)
                    .and_then(|x| m.uv_rel.get(x as usize))
                    .and_then(|c| c.get(ix))
                    .copied()
                    .unwrap_or(UV_REL_CLOSED)
                    .wrapping_add(if mutants::active("retf_model_uvrel_unshifted") { 0 } else { 16 * u64::from(m.shift) })
            }
            TOp::Retf => {
                // N2b: the caller identity guards (not modelled: `retf_fail` makes them fail), then the pop: the root becomes the caller,
                // whose registers `0..b` lie below the old root
                if m.retf_fail {
                    exit!(snap);
                }
                let n = usize::from(i.b);
                if m.retf_below.len() != n {
                    return bad(format!("RETF b = {n} but {} caller registers given", m.retf_below.len()));
                }
                let mut s = std::mem::take(&mut m.retf_below);
                s.append(&mut m.slots);
                m.slots = s;
                m.shift += i.b;
                0
            }
            TOp::Hrefk => {
                // the node of constant key `b` at index `bits` of the node vector of table (node handle) `a`
                let (h, key, ix) = (val!(i.a), val!(i.b), bits);
                let Some(t) = h.checked_sub(1).and_then(|x| m.tables.get(x as usize)) else {
                    return bad(format!("bad table handle {h}"));
                };
                match t.nodes.get(&ix) {
                    Some(n) if n.key == key && !n.int => {}
                    _ => exit!(snap),
                }
                h << 32 | u64::from(ix)
            }
            TOp::Hrefv => {
                // N6: the node that holds the dynamic key `b` in the node vector (node handle) `a`: the model has no chain, so
                // it is the (only) known node with this key address; none known = the walk would not find it (exit)
                let (h, key) = (val!(i.a), val!(i.b));
                let Some(t) = h.checked_sub(1).and_then(|x| m.tables.get(x as usize)) else {
                    return bad(format!("bad table handle {h}"));
                };
                let int = bits & HREF_INT != 0;
                let found = t.nodes.iter().find(|(_, n)| n.key == key && (n.int == int) && (int || key != 0));
                if bits & HREF_DYN != 0 && bits & HREF_NIL != 0 {
                    // DYNMASK: no exit; a walk that finds nothing yields the nil node
                    match found {
                        Some((ix, _)) => h << 32 | u64::from(*ix),
                        None => NIL_CELL,
                    }
                } else
                if bits & HREF_ABS != 0 {
                    // BUNDLE-L D1b: the absence proof of a dynamic key: exits when a known node holds the key (the model has no chain:
                    // an unknown node is taken as absent, the recorder only emits it for a key it saw absent)
                    if found.is_some() {
                        exit!(snap);
                    }
                    0
                } else {
                    match found {
                        Some((ix, _)) => h << 32 | u64::from(*ix),
                        None => exit!(snap),
                    }
                }
            }
            TOp::Hmiss => {
                // node `idx` of the node vector (node handle) `a` does not hold constant key `b` and links on by `delta`
                let (h, key) = (val!(i.a), val!(i.b));
                let (ix, delta) = hmiss_unpack(bits);
                let Some(t) = h.checked_sub(1).and_then(|x| m.tables.get(x as usize)) else {
                    return bad(format!("bad table handle {h}"));
                };
                match t.nodes.get(&(ix as u32)) {
                    Some(n) if (n.key != key || n.int) && n.next == delta => {}
                    _ => exit!(snap),
                }
                0
            }
            TOp::Hload if val!(i.a) == NEXT_CELL => {
                let Some((k, v)) = m.next_cell else {
                    return bad("HLOAD of the pseudo node before a TNEXT");
                };
                let e = if bits & 2 != 0 { k } else { v };
                if bits & 1 != 0 && e.t != i.t {
                    exit!(snap);
                }
                e.bits
            }
            TOp::Strto => {
                let _ = val!(i.a);
                match m.strto_result.take() {
                    Some(n) if !m.alloc_fails => {
                        m.next_cell = Some((Val::NIL, n));
                        NEXT_CELL
                    }
                    _ => 0,
                }
            }
            TOp::Snew | TOp::Concat | TOp::Lib => {
                let (va, vb) = (val!(i.a), if matches!(i.op, TOp::Concat | TOp::Lib) { val!(i.b) } else { 0 });
                if let Some(v) = i.third() {
                    let _ = val!(v);
                }
                if m.alloc_fails {
                    m.str_result = None;
                    0
                } else {
                    // N3b: `SETMT` stores the metatable word of the table (handles are the model's table words)
                    if i.op == TOp::Lib && i.aux & 0xFF == lib::SETMT {
                        match va.checked_sub(1).and_then(|x| m.tables.get_mut(x as usize)) {
                            Some(t) => {
                                if !super::mutants::active("setmt_model_skip") {
                                    t.metatable = vb;
                                }
                            }
                            None => return bad(format!("SETMT of a bad table handle {va}")),
                        }
                    }
                    m.str_result.take().unwrap_or(0x1000)
                }
            }
            TOp::Tnext => {
                let (h, c) = (val!(i.a), val!(i.b));
                if m.alloc_fails {
                    m.next_cell = None;
                    0
                } else {
                    let r = match m.next_result.take() {
                        Some(r) => r,
                        None => {
                            let Some(t) = h.checked_sub(1).and_then(|x| m.tables.get(x as usize)) else {
                                return bad(format!("bad table handle {h}"));
                            };
                            if ir.ty_of(i.b) != Ty::Int {
                                return bad("TNEXT needs Model::next_result for a non-integer control key");
                            }
                            let from = (c as i64).max(0) as usize;
                            match t.array.iter().enumerate().skip(from).find(|(_, v)| v.t != Ty::Nil) {
                                Some((j, v)) => (Val::int(j as i64 + 1), *v),
                                None => return bad("TNEXT leaves the model array: needs Model::next_result"),
                            }
                        }
                    };
                    m.next_cell = Some(r);
                    NEXT_CELL
                }
            }
            TOp::Hload if val!(i.a) == NIL_CELL => {
                if bits & 1 != 0 && i.t != Ty::Nil {
                    exit!(snap);
                }
                0
            }
            TOp::Hload => {
                let (h, ix) = (val!(i.a) >> 32, (val!(i.a) & 0xFFFF_FFFF) as u32);
                let Some(e) = node(m, h, ix) else {
                    return bad("hash load of an unknown node");
                };
                if bits & 1 != 0 && e.val.t != i.t && !(i.flags & F_ANYSTR != 0 && is_str_ty(e.val.t)) {
                    exit!(snap);
                }
                e.val.bits
            }
            TOp::Hstore => {
                let (h, ix) = (val!(i.a) >> 32, (val!(i.a) & 0xFFFF_FFFF) as u32);
                let nv = Val {
                    t: ir.ty_of(i.b),
                    bits: val!(i.b),
                };
                match h
                    .checked_sub(1)
                    .and_then(|x| m.tables.get_mut(x as usize))
                    .and_then(|t| t.nodes.get_mut(&ix))
                {
                    Some(e) => {
                        // bit 0: the old value must not be nil (an insertion / `__newindex` is the interpreter's)
                        if i.flags & F_GUARD != 0 && bits & 1 != 0 && e.val.t == Ty::Nil {
                            exit!(snap);
                        }
                        e.val = nv;
                    }
                    None => return bad("hash store to an unknown node"),
                }
                0
            }
            TOp::Tbar => 0,
            TOp::Tnew => {
                if m.alloc_fails {
                    0
                } else {
                    // `a` = NEWTABLE's B (hash size 2^(B-1) when B > 0), `aux` = the array size C
                    let (c, hb) = (i.aux as usize, i.a);
                    m.tables.push(MTable {
                        array: vec![Val::NIL; c],
                        metatable: 0,
                        flags: 0,
                        lsizenode: if hb > 0 { (hb - 1) as u8 } else { 0 },
                        lenhint: 0,
                        nodes: Default::default(),
                    });
                    m.tables.len() as u64
                }
            }
            TOp::Newref => {
                let (h, key) = (val!(i.a), val!(i.b));
                let Some(t) = h.checked_sub(1).and_then(|x| m.tables.get_mut(x as usize)) else {
                    return bad(format!("bad table handle {h}"));
                };
                if m.alloc_fails || t.nodes.values().any(|n| n.key == key && !n.int) {
                    0
                } else {
                    let ix = match m.newref_ix.take() {
                        Some(ix) => ix,
                        None => (0u32..).find(|ix| !t.nodes.contains_key(ix)).unwrap_or(0),
                    };
                    for n in t.nodes.values_mut() {
                        n.next = NEXT_UNKNOWN; // an insertion may relocate a colliding node and relink the chain
                    }
                    t.nodes.insert(ix, MNode { key, val: Val::NIL, next: NEXT_UNKNOWN, int: false });
                    h << 32 | u64::from(ix)
                }
            }
            TOp::Tset => {
                // the value (`aux`) is read for its operand check only: the recorder rebuilt the model of the table from
                // the VM before this runs (where the key landed is the table's business), see `Rec::sync_table`
                let _ = (val!(i.a), val!(i.b), val!(i.aux as Ref));
                u64::from(!m.alloc_fails)
            }
            TOp::Tlen => {
                let h = val!(i.a);
                match m.len_result.take() {
                    Some(n) => n,
                    None => {
                        if i.aux != 0 {
                            return bad("TLEN of a string needs Model::len_result");
                        }
                        let Some(t) = h.checked_sub(1).and_then(|x| m.tables.get(x as usize)) else {
                            return bad(format!("bad table handle {h}"));
                        };
                        t.array.iter().take_while(|v| v.t != Ty::Nil).count() as u64
                    }
                }
            }
            TOp::Calln => {
                let x = f64::from_bits(val!(i.a));
                let y = if i.b == NOREF {
                    0.0
                } else {
                    f64::from_bits(val!(i.b))
                };
                match i.aux {
                    CALLN_POW => pow_lua(x, y).to_bits(),
                    CALLN_FMOD => fmod_lua(x, y).to_bits(),
                    CALLN_FLOOR => x.floor().to_bits(),
                    CALLN_CFMOD => (x % y).to_bits(),
                    CALLN_SIN => x.sin().to_bits(),
                    CALLN_COS => x.cos().to_bits(),
                    CALLN_EXP => x.exp().to_bits(),
                    CALLN_LOG => x.ln().to_bits(),
                    f => return bad(format!("unknown pure helper {f}")),
                }
            }
            TOp::Loop | TOp::Tvirt | TOp::Sstore => 0,
            TOp::Phi => match &carried {
                Some(c) => c[pc],
                None => val!(i.a),
            },
            TOp::Poll => {
                polls += 1;
                if m.poll_exit == Some(polls) {
                    exit!(snap);
                }
                0
            }
            TOp::Budget => {
                if m.budget < i64::from(i.a) {
                    exit!(snap);
                }
                m.budget -= i64::from(i.a);
                0
            }
        };
        v[pc] = r;
        pc += 1;
    }
}

fn node(m: &Model, h: u64, ix: u32) -> Option<MNode> {
    h.checked_sub(1)
        .and_then(|x| m.tables.get(x as usize))
        .and_then(|t| t.nodes.get(&ix))
        .copied()
}

fn elem(m: &Model, h: u64, ix: usize) -> Option<Val> {
    h.checked_sub(1)
        .and_then(|x| m.tables.get(x as usize))
        .and_then(|t| t.array.get(ix))
        .copied()
}

/// The random IR generator for the property tests: valid, well-typed traces (integer / float / array / upvalue ops,
/// guards with snapshots, optionally a loop with PHIs) together with models to run them on.
pub mod gen {
    use super::*;

    /// xorshift64* (deterministic, dependency free).
    #[derive(Clone, Debug)]
    pub struct Rng(pub u64);

    impl Rng {
        /// Seeded generator (the seed is mixed so that small seeds differ).
        pub fn new(seed: u64) -> Rng {
            let mut r = Rng(seed ^ 0x9E37_79B9_7F4A_7C15);
            r.next_u64();
            r.next_u64();
            r
        }
        /// Next 64 random bits.
        pub fn next_u64(&mut self) -> u64 {
            let mut x = self.0;
            if x == 0 {
                x = 0x2545_F491_4F6C_DD1D;
            }
            x ^= x >> 12;
            x ^= x << 25;
            x ^= x >> 27;
            self.0 = x;
            x.wrapping_mul(0x2545_F491_4F6C_DD1D)
        }
        /// Uniform in `0..n` (`n > 0`).
        pub fn below(&mut self, n: u64) -> u64 {
            self.next_u64() % n
        }
        /// True with probability `num / den`.
        pub fn chance(&mut self, num: u64, den: u64) -> bool {
            self.below(den) < num
        }
        /// A random element.
        pub fn pick<T: Copy>(&mut self, s: &[T]) -> T {
            s[self.below(s.len() as u64) as usize]
        }
    }

    /// Integers that hit every fold rule and every wrap / shift corner.
    pub const INTS: &[i64] = &[
        0,
        1,
        -1,
        2,
        3,
        7,
        -7,
        8,
        63,
        64,
        65,
        -63,
        -64,
        -65,
        100,
        0xFFFF,
        i64::MAX,
        i64::MIN,
        i64::MIN + 1,
        1 << 53,
        (1 << 53) + 1,
    ];

    /// Floats: zeros, infinities, NaNs (default and signalling payload), huge, subnormal, integral and not.
    pub const NUMS: &[u64] = &[
        0x0000_0000_0000_0000, // +0.0
        0x8000_0000_0000_0000, // -0.0
        0x3FF0_0000_0000_0000, // 1.0
        0xBFF0_0000_0000_0000, // -1.0
        0x3FE0_0000_0000_0000, // 0.5
        0x4000_0000_0000_0000, // 2.0
        0x400C_0000_0000_0000, // 3.5
        0x4008_0000_0000_0000, // 3.0
        0x7E37_E43C_8800_759C, // 1e300
        0xFE37_E43C_8800_759C, // -1e300
        0x7FF0_0000_0000_0000, // inf
        0xFFF0_0000_0000_0000, // -inf
        0xFFF8_0000_0000_0000, // default NaN
        0x7FF0_0000_0000_0001, // signalling NaN with a payload
        0x4340_0000_0000_0000, // 2^53
        0x43E0_0000_0000_0000, // 2^63
        0xC3E0_0000_0000_0000, // -2^63
        0x0000_0000_0000_0400, // subnormal
        0x4059_0000_0000_0000, // 100.0
    ];

    /// Generator configuration.
    #[derive(Clone, Copy, Debug)]
    pub struct GenCfg {
        /// Approximate number of instructions.
        pub size: usize,
        /// May emit a `LOOP` with PHIs.
        pub loops: bool,
    }

    /// A generated trace and the models to run it on (the first one passes every guard of the first iteration).
    pub struct Case {
        /// The trace, unoptimised.
        pub ir: TraceIr,
        /// Models: slots, tables and closures differ.
        pub models: Vec<Model>,
    }

    const NSLOT: usize = 8;
    const ASIZE: usize = 8;
    const NUPV: usize = 6;
    /// F1: the generator's tables have four hash nodes (`lsizenode` 2); node `j` holds the key `HKEYS[j]` (floats at
    /// even nodes, integers at odd ones).
    pub const HKEYS: [u64; 4] = [0x1000, 0x1018, 0x1030, 0x1048];
    const HLSIZE: u8 = 2;

    fn base_model(rng: &mut Rng) -> Model {
        let mut m = Model::new();
        let int = |rng: &mut Rng| Val::int(rng.pick(INTS));
        let num = |rng: &mut Rng| Val {
            t: Ty::Num,
            bits: rng.pick(NUMS),
        };
        for _ in 0..3 {
            let v = int(rng);
            m.slots.push(v);
        }
        for _ in 0..2 {
            let v = num(rng);
            m.slots.push(v);
        }
        for h in 1..=2u64 {
            m.slots.push(Val {
                t: Ty::Tab,
                bits: h,
            });
        }
        m.slots.push(Val::int(rng.pick(INTS)));
        debug_assert_eq!(m.slots.len(), NSLOT);
        let t1 = (0..ASIZE).map(|_| num(rng)).collect();
        let t2 = (0..ASIZE).map(|_| int(rng)).collect();
        for array in [t1, t2] {
            let mut nodes = std::collections::BTreeMap::new();
            for (j, &key) in HKEYS.iter().enumerate() {
                let val = if j % 2 == 0 { num(rng) } else { int(rng) };
                nodes.insert(j as u32, MNode { key, val, next: i32::from(j + 1 < HKEYS.len()), int: false });
            }
            m.tables.push(MTable {
                array,
                metatable: 0,
                flags: 0,
                lsizenode: HLSIZE,
                lenhint: 0,
                nodes,
            });
        }
        let upv = (0..NUPV)
            .map(|j| if j % 2 == 0 { num(rng) } else { int(rng) })
            .collect();
        m.closures.push(upv);
        m.budget = 200;
        m.max_iters = 3;
        m
    }

    fn vary(rng: &mut Rng, base: &Model) -> Model {
        let mut m = base.clone();
        for s in &mut m.slots {
            if rng.chance(1, 2) {
                match s.t {
                    Ty::Int => s.bits = rng.pick(INTS) as u64,
                    Ty::Num => s.bits = rng.pick(NUMS),
                    _ => {}
                }
            }
        }
        if rng.chance(1, 8) {
            let k = rng.below(3) as usize;
            m.slots[k] = Val::num(1.5); // the entry typecheck fails
        }
        for t in &mut m.tables {
            for e in &mut t.array {
                if rng.chance(1, 3) {
                    match e.t {
                        Ty::Int => e.bits = rng.pick(INTS) as u64,
                        Ty::Num => e.bits = rng.pick(NUMS),
                        _ => {}
                    }
                }
                if rng.chance(1, 40) {
                    *e = Val::NIL;
                }
            }
        }
        for t in &mut m.tables {
            for n in t.nodes.values_mut() {
                if rng.chance(1, 3) {
                    match n.val.t {
                        Ty::Int => n.val.bits = rng.pick(INTS) as u64,
                        Ty::Num => n.val.bits = rng.pick(NUMS),
                        _ => {}
                    }
                }
                if rng.chance(1, 24) {
                    n.val = Val::NIL; // a removed key: the typecheck and the old-value check of a store exit
                }
                if rng.chance(1, 40) {
                    n.key = 0; // another layout: the HREFK key check exits
                } else if rng.chance(1, 30) {
                    n.key = HKEYS[rng.below(4) as usize]; // another layout: a key moved here (the HMISS key check exits)
                }
                if rng.chance(1, 30) {
                    n.next = rng.below(3) as i32; // another chain (the HMISS link check exits)
                }
            }
        }
        if cfg!(rustynail_lua54) {
            // 5.4: some tables have a hinted array limit (BITRAS) somewhere inside the array
            for t in &mut m.tables {
                if rng.chance(2, 3) {
                    t.flags |= MODEL_BITRAS;
                    t.lenhint = 1 + rng.below(ASIZE as u64) as u32;
                }
            }
        }
        if rng.chance(1, 4) {
            m.budget = rng.below(12) as i64;
        }
        if rng.chance(1, 4) {
            m.poll_exit = Some(1 + rng.below(3) as u32);
        }
        m
    }

    struct G<'a> {
        rng: &'a mut Rng,
        b: Builder,
        ints: Vec<Ref>,
        nums: Vec<Ref>,
        tabs: Vec<(Ref, Ty, u64)>,
        base: Model,
    }

    impl G<'_> {
        fn ins(&mut self, op: TOp, t: Ty, flags: u8, a: Ref, b: Ref, aux: u32) -> Ref {
            let ins = Ins {
                op,
                t,
                flags: if op.always_guard() {
                    flags | F_GUARD
                } else {
                    flags
                },
                a,
                b,
                prev: NOREF,
                aux,
            };
            self.b.ir.emit(ins)
        }

        fn kint(&mut self) -> Ref {
            let v = self.rng.pick(INTS);
            self.b.ir.kint(v)
        }

        fn knum(&mut self) -> Ref {
            let v = self.rng.pick(NUMS);
            self.b.ir.konst(KVal::Num(v))
        }

        fn int(&mut self) -> Ref {
            if self.rng.chance(2, 5) {
                self.kint()
            } else {
                let n = self.ints.len() as u64;
                let i = self.rng.below(n) as usize;
                self.ints[i]
            }
        }

        fn num(&mut self) -> Ref {
            if self.rng.chance(1, 3) {
                self.knum()
            } else {
                let n = self.nums.len() as u64;
                let i = self.rng.below(n) as usize;
                self.nums[i]
            }
        }

        fn snap(&mut self) -> u32 {
            let mut entries = Vec::new();
            let k = self.rng.below(5);
            for s in 0..k {
                let r = if self.rng.chance(1, 5) {
                    self.kint()
                } else if self.rng.chance(1, 2) {
                    self.int()
                } else {
                    self.num()
                };
                entries.push(SnapEntry {
                    slot: s as u16 * 2 + self.rng.below(2) as u16,
                    r,
                    flags: 0,
                });
            }
            let s = Snap {
                pc: self.rng.below(40) as u32,
                ..Snap::default()
            };
            self.b.ir.snapshot(s, &entries)
        }

        /// Runs the trace so far on the base model for one iteration: the snapshot of the exit, if any.
        fn base_exit(&self) -> Option<u32> {
            let mut m = self.base.clone();
            m.max_iters = 0;
            match run(&self.b.ir, &mut m) {
                Ok(Outcome::Exit { snap, .. }) => Some(snap),
                Ok(_) => None,
                Err(e) => panic!("generator produced invalid IR: {e:?}"),
            }
        }

        /// Emits a guard instruction and makes it hold on the base model by flipping or, failing that, removing it.
        fn guard(&mut self, op: TOp, t: Ty, flags: u8, a: Ref, b: Ref, bits: u32) -> Option<Ref> {
            let snap = self.snap();
            let mark = self.b.ir.ins.len();
            let r = self.ins(op, t, flags, a, b, guard_aux(snap, bits));
            if self.base_exit() == Some(snap) {
                let flipped = match op {
                    TOp::Eq => Some((TOp::Ne, bits)),
                    TOp::Ne => Some((TOp::Eq, bits)),
                    TOp::Lt => Some((TOp::Ge, bits)),
                    TOp::Ge => Some((TOp::Lt, bits)),
                    TOp::Le => Some((TOp::Gt, bits)),
                    TOp::Gt => Some((TOp::Le, bits)),
                    TOp::Ult => Some((TOp::Uge, bits)),
                    TOp::Uge => Some((TOp::Ult, bits)),
                    TOp::Fcmp => Some((TOp::Fcmp, bits ^ 16)),
                    _ => None,
                };
                match flipped {
                    Some((nop, nbits)) => {
                        let i = &mut self.b.ir.ins[mark];
                        i.op = nop;
                        i.aux = guard_aux(snap, nbits);
                        if self.base_exit() == Some(snap) {
                            self.b.ir.ins.truncate(mark);
                            return None;
                        }
                    }
                    None => {
                        self.b.ir.ins.truncate(mark);
                        return None;
                    }
                }
            }
            Some(r)
        }

        fn step(&mut self) {
            match self.rng.below(26) {
                0..=7 => {
                    let op = [
                        TOp::Add,
                        TOp::Sub,
                        TOp::Mul,
                        TOp::Band,
                        TOp::Bor,
                        TOp::Bxor,
                        TOp::Shl,
                        TOp::Shr,
                    ][self.rng.below(8) as usize];
                    let (a, b) = (self.int(), self.int());
                    let r = self.ins(op, Ty::Int, 0, a, b, 0);
                    self.ints.push(r);
                }
                8 => {
                    let op = if self.rng.chance(1, 2) {
                        TOp::Neg
                    } else {
                        TOp::Bnot
                    };
                    let a = self.int();
                    let r = self.ins(op, Ty::Int, 0, a, NOREF, 0);
                    self.ints.push(r);
                }
                9 => {
                    // reassociation shape: (x + c1) + c2 or (x - c1) + c2
                    let x = self.int();
                    let (c1, c2) = (self.kint(), self.kint());
                    let op = if self.rng.chance(1, 2) {
                        TOp::Add
                    } else {
                        TOp::Sub
                    };
                    let t = self.ins(op, Ty::Int, 0, x, c1, 0);
                    let r = self.ins(TOp::Add, Ty::Int, 0, t, c2, 0);
                    self.ints.push(t);
                    self.ints.push(r);
                }
                10 | 11 => {
                    let op = if self.rng.chance(1, 2) {
                        TOp::Idiv
                    } else {
                        TOp::Mod
                    };
                    let a = self.int();
                    // B4-4: half of the divisors are constants of the magic-number division (and the ones that are not)
                    let b = if self.rng.chance(1, 2) {
                        let v = self.rng.pick(&[3i64, 5, 6, 7, 9, 10, 11, 12, 100, 255, 1000, 65537, 1_000_003, 4_294_967_297, (1 << 62) - 1, 1 << 62, i64::MAX, -3, -10, 2, 1 << 20]);
                        self.b.ir.kint(v)
                    } else {
                        self.int()
                    };
                    if let Some(r) = self.guard(op, Ty::Int, F_GUARD, a, b, 0) {
                        self.ints.push(r);
                    }
                }
                12..=15 => {
                    let op = [
                        TOp::Fadd,
                        TOp::Fsub,
                        TOp::Fmul,
                        TOp::Fdiv,
                        TOp::Fmin,
                        TOp::Fmax,
                    ][self.rng.below(6) as usize];
                    let (a, b) = (self.num(), self.num());
                    let r = self.ins(op, Ty::Num, 0, a, b, 0);
                    self.nums.push(r);
                }
                16 => {
                    let op = [TOp::Fneg, TOp::Fabs, TOp::Fsqrt][self.rng.below(3) as usize];
                    let a = self.num();
                    let r = self.ins(op, Ty::Num, 0, a, NOREF, 0);
                    self.nums.push(r);
                }
                17 => {
                    let a = self.int();
                    let r = self.ins(TOp::ConvNumInt, Ty::Num, 0, a, NOREF, 0);
                    self.nums.push(r);
                }
                18 => {
                    let a = self.num();
                    if let Some(r) = self.guard(TOp::F2iExact, Ty::Int, F_GUARD, a, NOREF, 0) {
                        self.ints.push(r);
                    }
                }
                19 => {
                    let op = [
                        TOp::Eq,
                        TOp::Ne,
                        TOp::Lt,
                        TOp::Le,
                        TOp::Gt,
                        TOp::Ge,
                        TOp::Ult,
                        TOp::Uge,
                    ][self.rng.below(8) as usize];
                    let (a, b) = (self.int(), self.int());
                    self.guard(op, Ty::Int, 0, a, b, 0);
                }
                20 => {
                    let (a, b) = (self.num(), self.num());
                    let bits = self.rng.below(6) as u32 | (self.rng.below(2) as u32) << 4;
                    self.guard(TOp::Fcmp, Ty::Num, 0, a, b, bits);
                }
                21 | 22 => {
                    if self.rng.chance(2, 5) {
                        self.hash_access()
                    } else {
                        self.table_access()
                    }
                }
                23 => {
                    // upvalue load / store: even upvalues hold floats, odd ones integers
                    let j = self.rng.below(NUPV as u64);
                    let cl = self.b.ir.konst(KVal::Ptr(1, PtrKind::LClosure));
                    let jk = self.b.ir.kint(j as i64);
                    let uref = self.ins(TOp::Uref, Ty::P64, 0, cl, jk, 0);
                    if self.rng.chance(1, 2) {
                        let f = j & 1 == 0;
                        let r = self.ins(
                            TOp::Uload,
                            if f { Ty::Num } else { Ty::Int },
                            0,
                            uref,
                            NOREF,
                            0,
                        );
                        if f {
                            self.nums.push(r)
                        } else {
                            self.ints.push(r)
                        }
                    } else {
                        let v = if j & 1 == 0 { self.num() } else { self.int() };
                        self.ins(TOp::Ustore, Ty::Nil, 0, uref, v, 0);
                    }
                }
                24 => {
                    let (a, b) = (self.num(), self.num());
                    let f = self.rng.below(3) as u32;
                    let r = self.ins(TOp::Calln, Ty::Num, 0, a, if f == 2 { NOREF } else { b }, f);
                    self.nums.push(r);
                }
                _ => match self.rng.below(4) {
                    0 => {
                        let snap = self.snap();
                        self.ins(TOp::Poll, Ty::Nil, 0, NOREF, NOREF, guard_aux(snap, 0));
                    }
                    1 => {
                        let snap = self.snap();
                        let c = 1 + self.rng.below(3) as Ref;
                        self.ins(TOp::Budget, Ty::I64, 0, c, NOREF, guard_aux(snap, 0));
                    }
                    2 => {
                        let (t, _, h) = self.tabs[self.rng.below(self.tabs.len() as u64) as usize];
                        let other = if self.rng.chance(4, 5) { h } else { 3 - h };
                        let k = self.b.ir.konst(KVal::Ptr(other, PtrKind::Raw));
                        self.guard(TOp::Eqref, Ty::Tab, 0, t, k, 0);
                    }
                    _ => {
                        let (a, b) = (self.int(), self.int());
                        let x = self.ins(TOp::Add, Ty::Int, 0, a, b, 0);
                        self.ints.push(x);
                        let y = self.ins(TOp::Add, Ty::Int, 0, b, a, 0);
                        self.ints.push(y);
                    }
                },
            }
        }

        /// F1: `FLOAD lsizenode; EQ; FLOAD node; HREFK; HLOAD | HSTORE` on one of the generator's tables.
        fn hash_access(&mut self) {
            let (t, _, _) = self.tabs[self.rng.below(self.tabs.len() as u64) as usize];
            let ls = self.ins(TOp::Fload, Ty::U32, 0, t, Field::Lsizenode as Ref, 0);
            let two = self.b.ir.kint(i64::from(HLSIZE));
            self.guard(TOp::Eq, Ty::I64, 0, ls, two, 0);
            let node = self.ins(TOp::Fload, Ty::P64, 0, t, Field::Node as Ref, 0);
            let j = self.rng.below(4) as usize;
            if self.rng.chance(1, 5) {
                // P-A: one step of an absent-key proof: node `j` is not this key and links on by `delta`
                let key = match self.rng.below(4) {
                    0 => HKEYS[j], // the node holds this key: exits on the base model (the step is dropped)
                    1 | 2 => HKEYS[(j + 1) % 4], // another key of the table: passes unless a model moved a key here
                    _ => 0x2000,
                };
                let right = i32::from(j < 3);
                let delta = if self.rng.chance(1, 8) { 1 - right } else { right };
                let kc = self.b.ir.konst(KVal::Ptr(key, PtrKind::Raw));
                let snap = self.snap();
                let mark = self.b.ir.ins.len();
                let bits = hmiss_bits(j, delta).expect("small index");
                self.ins(TOp::Hmiss, Ty::I64, F_GUARD, node, kc, guard_aux(snap, bits));
                if self.base_exit() == Some(snap) {
                    self.b.ir.ins.truncate(mark);
                }
                return;
            }
            let key = if self.rng.chance(1, 8) {
                HKEYS[(j + 1) % 4] // the node holds another key: the guard fails on the base model
            } else {
                HKEYS[j]
            };
            let kc = self.b.ir.konst(KVal::Ptr(key, PtrKind::Raw));
            let snap = self.snap();
            let mark = self.b.ir.ins.len();
            let hr = self.ins(TOp::Hrefk, Ty::P64, F_GUARD, node, kc, guard_aux(snap, j as u32));
            if self.base_exit() == Some(snap) {
                self.b.ir.ins.truncate(mark);
                return;
            }
            let et = if j.is_multiple_of(2) { Ty::Num } else { Ty::Int };
            if self.rng.chance(1, 2) {
                let lt = if self.rng.chance(1, 10) { Ty::Nil } else { et };
                let snap = self.snap();
                let mark = self.b.ir.ins.len();
                let r = self.ins(TOp::Hload, lt, F_GUARD, hr, NOREF, guard_aux(snap, 1));
                if self.base_exit() == Some(snap) {
                    self.b.ir.ins.truncate(mark);
                } else if lt == Ty::Num {
                    self.nums.push(r);
                } else if lt == Ty::Int {
                    self.ints.push(r);
                }
            } else {
                let v = if et == Ty::Num { self.num() } else { self.int() };
                let (flags, aux, snap) = if self.rng.chance(2, 3) {
                    let snap = self.snap();
                    (F_GUARD, guard_aux(snap, 1), Some(snap))
                } else {
                    (0, 0, None)
                };
                let mark = self.b.ir.ins.len();
                self.ins(TOp::Hstore, Ty::Nil, flags, hr, v, aux);
                if snap.is_some() && self.base_exit() == snap {
                    self.b.ir.ins.truncate(mark);
                }
            }
        }

        fn table_access(&mut self) {
            let (t, et, _) = self.tabs[self.rng.below(self.tabs.len() as u64) as usize];
            let x = self.int();
            let mask = self.b.ir.kint(ASIZE as i64 - 1);
            let idx = self.ins(TOp::Band, Ty::I64, 0, x, mask, 0);
            let asize = self.ins(TOp::Fload, Ty::U32, 0, t, Field::Asize as Ref, 0);
            self.guard(TOp::Ult, Ty::I64, 0, idx, asize, 0);
            if cfg!(rustynail_lua54) {
                // TR30: the recorder's `ATOUCH t idx` right after the bounds guard
                self.ins(TOp::Atouch, Ty::Nil, 0, t, idx, 0);
            }
            let arr = self.ins(TOp::Fload, Ty::P64, 0, t, Field::Array as Ref, 0);
            let ix = if self.rng.chance(1, 2) {
                idx
            } else {
                // (idx + c) - c: folds back to idx (the index normalisation of TR24e)
                let k = self.kint();
                let s = self.ins(TOp::Add, Ty::I64, 0, idx, k, 0);
                self.ins(TOp::Sub, Ty::I64, 0, s, k, 0)
            };
            let aref = self.ins(TOp::Aref, Ty::P64, 0, arr, ix, 0);
            if self.rng.chance(1, 2) {
                let snap = self.snap();
                let mark = self.b.ir.ins.len();
                let r = self.ins(TOp::Aload, et, F_GUARD, aref, NOREF, guard_aux(snap, 1));
                if self.base_exit() == Some(snap) {
                    self.b.ir.ins.truncate(mark);
                } else if et == Ty::Num {
                    self.nums.push(r);
                } else {
                    self.ints.push(r);
                }
            } else {
                let v = if et == Ty::Num {
                    self.num()
                } else {
                    self.int()
                };
                self.ins(TOp::Astore, Ty::Nil, 0, aref, v, 0);
                if self.rng.chance(1, 2) {
                    self.ins(TOp::Tbar, Ty::Nil, 0, t, v, 0);
                }
            }
        }
    }

    /// Generates a random valid trace and models for it.
    pub fn random_case(rng: &mut Rng, cfg: GenCfg) -> Case {
        let base = base_model(rng);
        let mut g = G {
            rng,
            b: Builder::new(false, false),
            ints: Vec::new(),
            nums: Vec::new(),
            tabs: Vec::new(),
            base: base.clone(),
        };
        let snap0 = g.b.ir.snapshot(Snap::default(), &[]);
        // entry loads: guarded (typechecked) like a real trace's
        for s in [0u16, 1, 2, 3, 4] {
            let int = s < 3;
            let t = if int { Ty::Int } else { Ty::Num };
            let r = g.ins(TOp::Sload, t, F_GUARD, s, NOREF, guard_aux(snap0, 1));
            if int {
                g.ints.push(r)
            } else {
                g.nums.push(r)
            }
        }
        for (s, et, h) in [(5u16, Ty::Num, 1u64), (6, Ty::Int, 2)] {
            let r = g.ins(TOp::Sload, Ty::Tab, F_GUARD, s, NOREF, guard_aux(snap0, 1));
            g.tabs.push((r, et, h));
            let k = g.b.ir.konst(KVal::Ptr(h, PtrKind::Raw));
            g.tabs.push((k, et, h));
        }
        if g.rng.chance(1, 2) {
            // an unchecked load of an integer slot
            let r = g.ins(TOp::Sload, Ty::Int, 0, 7, NOREF, 0);
            g.ints.push(r);
        }
        let snap = g.snap();
        g.ins(TOp::Budget, Ty::I64, 0, 2, NOREF, guard_aux(snap, 0));
        let n_pre = g.rng.below(cfg.size as u64 / 2 + 1) as usize + 4;
        for _ in 0..n_pre {
            g.step();
        }
        let mut phis: Vec<(Ref, Ty)> = Vec::new();
        let mut loop_ref = NOREF;
        if cfg.loops && g.rng.chance(2, 3) {
            loop_ref = g.b.loop_marker().expect("short");
            let k = 1 + g.rng.below(3);
            for _ in 0..k {
                let (t, a) = if g.rng.chance(1, 2) {
                    (Ty::Int, g.int())
                } else {
                    (Ty::Num, g.num())
                };
                let p = g.b.phi(t, a).expect("short");
                // provisional back-edge value: the pre-roll value (valid for the prefix checks)
                g.b.set_phi(p, a);
                if t == Ty::Int {
                    g.ints.push(p)
                } else {
                    g.nums.push(p)
                }
                phis.push((p, t));
            }
            let snap = g.snap();
            g.ins(TOp::Poll, Ty::Nil, 0, NOREF, NOREF, guard_aux(snap, 0));
            let snap = g.snap();
            g.ins(TOp::Budget, Ty::I64, 0, 2, NOREF, guard_aux(snap, 0));
            for _ in 0..cfg.size / 2 {
                g.step();
            }
        }
        // make the results observable through upvalues
        let cl = g.b.ir.konst(KVal::Ptr(1, PtrKind::LClosure));
        for j in 0..NUPV as i64 {
            if g.rng.chance(2, 3) {
                let jk = g.b.ir.kint(j);
                let uref = g.ins(TOp::Uref, Ty::P64, 0, cl, jk, 0);
                let v = if j % 2 == 0 {
                    g.nums[g.rng.below(g.nums.len() as u64) as usize]
                } else {
                    g.ints[g.rng.below(g.ints.len() as u64) as usize]
                };
                g.ins(TOp::Ustore, Ty::Nil, 0, uref, v, 0);
            }
        }
        // back-edge values: a random body definition of the right type (or the phi itself)
        for (p, t) in phis {
            let pool = if t == Ty::Int { &g.ints } else { &g.nums };
            let cands: Vec<Ref> = pool.iter().copied().filter(|&r| r > loop_ref).collect();
            let body = cands[g.rng.below(cands.len() as u64) as usize];
            g.b.set_phi(p, body);
        }
        let mut models = vec![base.clone()];
        for _ in 0..5 {
            let m = vary(g.rng, &base);
            models.push(m);
        }
        Case {
            ir: g.b.finish(),
            models,
        }
    }
}

/// N5: a string of either variant.
fn is_str_ty(t: Ty) -> bool {
    matches!(t, Ty::ShrStr | Ty::LngStr)
}
