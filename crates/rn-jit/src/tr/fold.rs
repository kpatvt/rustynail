//! Constant folding and algebraic identities (docs/JIT_TRACE.md 6.1: TR11 floats, TR12 integers, and the index
//! normalisation of TR24e), plus [`optimize`], which re-emits a finished trace through the folding / CSE
//! [`Builder`](super::ir::Builder) (the property tests run it against [`super::interp`]).
//!
//! The integer semantics are `rn_core::number`'s (the VM's own helpers); nothing is re-implemented here.
//!
//! Float rule (TR11): an operation is folded only when its inputs and its result are not NaN, so no NaN payload and no
//! sign-of-zero identity (`x + 0.0`, `x * 1.0`) is ever decided at compile time. The only float identities are bit
//! exact ones (`--x`, `|-x|`, `min(x, x)`).

use super::ir::*;
use super::mutants;
use rn_core::number::{float_to_int, idiv, imod, shiftl, shiftr, F2I};

/// The outcome of folding one instruction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fold {
    /// The instruction is equivalent to this existing ref (a constant or an earlier instruction).
    Ref(Ref),
    /// A guard proven true: nothing is emitted.
    Drop,
    /// Emit this (possibly rewritten) instruction.
    Keep(Ins),
}

fn kint(ir: &mut TraceIr, v: i64) -> Result<Fold, IrAbort> {
    Ok(Fold::Ref(ir.kint(v)))
}

fn knum(ir: &mut TraceIr, v: f64) -> Result<Fold, IrAbort> {
    Ok(Fold::Ref(ir.knum(v)))
}

/// A guard whose condition is statically known: true drops it, false is `ConstGuard`.
fn known(holds: bool) -> Result<Fold, IrAbort> {
    if holds {
        Ok(Fold::Drop)
    } else {
        Err(IrAbort::ConstGuard)
    }
}

/// Constants to the right; for commutative ops otherwise the smaller ref first (so CSE finds both spellings).
fn canon(ins: &mut Ins, commutative: bool) {
    let (ca, cb) = (TraceIr::is_const(ins.a), TraceIr::is_const(ins.b));
    if (ca && !cb) || (commutative && ca == cb && ins.a > ins.b) {
        std::mem::swap(&mut ins.a, &mut ins.b);
    }
}

/// Mirror of an integer relation (`a op b` == `b mirror(op) a`).
fn mirror(op: TOp) -> Option<TOp> {
    match op {
        TOp::Lt => Some(TOp::Gt),
        TOp::Le => Some(TOp::Ge),
        TOp::Gt => Some(TOp::Lt),
        TOp::Ge => Some(TOp::Le),
        _ => None,
    }
}

fn eval_icmp(op: TOp, x: i64, y: i64) -> bool {
    match op {
        TOp::Eq => x == y,
        TOp::Ne => x != y,
        TOp::Lt => x < y,
        TOp::Le => x <= y,
        TOp::Gt => x > y,
        TOp::Ge => x >= y,
        TOp::Ult => (x as u64) < (y as u64),
        TOp::Uge => (x as u64) >= (y as u64),
        _ => unreachable!(),
    }
}

/// The `Fcmp` condition `cc` (`0 Eq, 1 Ne, 2 Lt, 3 Le, 4 Gt, 5 Ge`) on two doubles (IEEE: false on NaN except `Ne`).
pub fn eval_fcmp(cc: u32, x: f64, y: f64) -> bool {
    match cc {
        0 => x == y,
        1 => x != y,
        2 => x < y,
        3 => x <= y,
        4 => x > y,
        5 => x >= y,
        _ => panic!("bad Fcmp condition {cc}"),
    }
}

/// Folds one instruction against the trace so far (6.1). Only `ir.k` is touched (constants are interned).
pub fn fold(ir: &mut TraceIr, ins: Ins) -> Result<Fold, IrAbort> {
    let mut ins = ins;
    let guarded = ins.flags & F_GUARD != 0;
    match ins.op {
        // ---- integer arithmetic (TR12): wrapping, exact identities only ----
        TOp::Add => {
            if let (Some(x), Some(y)) = (ir.as_int(ins.a), ir.as_int(ins.b)) {
                return kint(ir, x.wrapping_add(y));
            }
            canon(&mut ins, true);
            if let Some(c2) = ir.as_int(ins.b) {
                if c2 == 0 {
                    return Ok(Fold::Ref(ins.a));
                }
                // TR24e: (x + c1) + c2 -> x + (c1 + c2)
                if TraceIr::is_ins(ins.a) {
                    let inner = *ir.at(ins.a);
                    if inner.op == TOp::Add {
                        if let Some(c1) = ir.as_int(inner.b) {
                            let k = ir.kint(c1.wrapping_add(c2));
                            return fold(
                                ir,
                                Ins {
                                    a: inner.a,
                                    b: k,
                                    ..ins
                                },
                            );
                        }
                    }
                }
            }
            Ok(Fold::Keep(ins))
        }
        TOp::Sub => {
            if let (Some(x), Some(y)) = (ir.as_int(ins.a), ir.as_int(ins.b)) {
                return kint(ir, x.wrapping_sub(y));
            }
            if ins.a == ins.b {
                return kint(ir, 0);
            }
            if let Some(c) = ir.as_int(ins.b) {
                // TR24e: x - c -> x + (-c)
                let k = ir.kint(c.wrapping_neg());
                return fold(
                    ir,
                    Ins {
                        op: TOp::Add,
                        b: k,
                        ..ins
                    },
                );
            }
            Ok(Fold::Keep(ins))
        }
        TOp::Mul => {
            if let (Some(x), Some(y)) = (ir.as_int(ins.a), ir.as_int(ins.b)) {
                return kint(ir, x.wrapping_mul(y));
            }
            canon(&mut ins, true);
            match ir.as_int(ins.b) {
                Some(0) => kint(ir, 0),
                Some(1) => Ok(Fold::Ref(ins.a)),
                Some(-1) => fold(
                    ir,
                    Ins {
                        op: TOp::Neg,
                        b: NOREF,
                        ..ins
                    },
                ),
                _ => Ok(Fold::Keep(ins)),
            }
        }
        TOp::Neg => {
            if let Some(x) = ir.as_int(ins.a) {
                return kint(ir, x.wrapping_neg());
            }
            if TraceIr::is_ins(ins.a) && ir.at(ins.a).op == TOp::Neg {
                return Ok(Fold::Ref(ir.at(ins.a).a));
            }
            Ok(Fold::Keep(ins))
        }
        TOp::Band => {
            if let (Some(x), Some(y)) = (ir.as_int(ins.a), ir.as_int(ins.b)) {
                return kint(ir, x & y);
            }
            canon(&mut ins, true);
            match ir.as_int(ins.b) {
                Some(0) => kint(ir, 0),
                Some(-1) => Ok(Fold::Ref(ins.a)),
                _ if ins.a == ins.b => Ok(Fold::Ref(ins.a)),
                _ => Ok(Fold::Keep(ins)),
            }
        }
        TOp::Bor => {
            if let (Some(x), Some(y)) = (ir.as_int(ins.a), ir.as_int(ins.b)) {
                return kint(ir, x | y);
            }
            canon(&mut ins, true);
            match ir.as_int(ins.b) {
                Some(0) => Ok(Fold::Ref(ins.a)),
                Some(-1) => kint(ir, -1),
                _ if ins.a == ins.b => Ok(Fold::Ref(ins.a)),
                _ => Ok(Fold::Keep(ins)),
            }
        }
        TOp::Bxor => {
            if let (Some(x), Some(y)) = (ir.as_int(ins.a), ir.as_int(ins.b)) {
                return kint(ir, x ^ y);
            }
            canon(&mut ins, true);
            match ir.as_int(ins.b) {
                Some(0) => Ok(Fold::Ref(ins.a)),
                _ if ins.a == ins.b => kint(ir, 0),
                _ => Ok(Fold::Keep(ins)),
            }
        }
        TOp::Bnot => {
            if let Some(x) = ir.as_int(ins.a) {
                return kint(ir, !x);
            }
            if TraceIr::is_ins(ins.a) && ir.at(ins.a).op == TOp::Bnot {
                return Ok(Fold::Ref(ir.at(ins.a).a));
            }
            Ok(Fold::Keep(ins))
        }
        TOp::Shl | TOp::Shr => {
            let f = if ins.op == TOp::Shl { shiftl } else { shiftr };
            let (ia, ib) = (ir.as_int(ins.a), ir.as_int(ins.b));
            if let (Some(x), Some(y)) = (ia, ib) {
                return kint(ir, f(x, y));
            }
            if ia == Some(0) {
                return kint(ir, 0);
            }
            match ib {
                Some(0) => Ok(Fold::Ref(ins.a)),
                Some(c) if c >= 64 || c <= -64 => kint(ir, 0),
                _ => Ok(Fold::Keep(ins)),
            }
        }
        TOp::Idiv | TOp::Mod => {
            let is_div = ins.op == TOp::Idiv;
            let Some(c) = ir.as_int(ins.b) else {
                return Ok(Fold::Keep(ins));
            };
            if c == 0 {
                // Division by a constant zero: the guard can never hold. (An unguarded one is not valid IR; leave it.)
                return if guarded {
                    Err(IrAbort::ConstGuard)
                } else {
                    Ok(Fold::Keep(ins))
                };
            }
            if let Some(x) = ir.as_int(ins.a) {
                let r = if is_div { idiv(x, c) } else { imod(x, c) };
                return kint(ir, r.expect("divisor is not zero"));
            }
            // The divisor is a nonzero constant: the guard is redundant.
            if guarded {
                ins.flags &= !F_GUARD;
                ins.aux = 0;
            }
            if is_div {
                match c {
                    1 => Ok(Fold::Ref(ins.a)),
                    -1 => fold(
                        ir,
                        Ins {
                            op: TOp::Neg,
                            b: NOREF,
                            ..ins
                        },
                    ),
                    _ => Ok(Fold::Keep(ins)),
                }
            } else if c == 1 || c == -1 {
                kint(ir, 0)
            } else {
                Ok(Fold::Keep(ins))
            }
        }
        // ---- floats (TR11) ----
        TOp::Fadd | TOp::Fsub | TOp::Fmul | TOp::Fdiv => {
            if let (Some(x), Some(y)) = (ir.as_num(ins.a), ir.as_num(ins.b)) {
                let r = match ins.op {
                    TOp::Fadd => x + y,
                    TOp::Fsub => x - y,
                    TOp::Fmul => x * y,
                    _ => x / y,
                };
                // Mutant fold_nan: fold NaN inputs / results too.
                if mutants::active("fold_nan") || (!x.is_nan() && !y.is_nan() && !r.is_nan()) {
                    return knum(ir, r);
                }
            }
            // Mutant fold_x_plus_zero: `x + 0.0 -> x` (wrong for x = -0.0).
            if mutants::active("fold_x_plus_zero")
                && ins.op == TOp::Fadd
                && ir.as_num(ins.b).is_some_and(|f| f.to_bits() == 0)
            {
                return Ok(Fold::Ref(ins.a));
            }
            Ok(Fold::Keep(ins))
        }
        TOp::Fneg => {
            if let Some(x) = ir.as_num(ins.a) {
                if !x.is_nan() {
                    return knum(ir, f64::from_bits(x.to_bits() ^ (1 << 63)));
                }
            }
            if TraceIr::is_ins(ins.a) && ir.at(ins.a).op == TOp::Fneg {
                return Ok(Fold::Ref(ir.at(ins.a).a));
            }
            Ok(Fold::Keep(ins))
        }
        TOp::Fabs => {
            if let Some(x) = ir.as_num(ins.a) {
                if !x.is_nan() {
                    return knum(ir, f64::from_bits(x.to_bits() & !(1 << 63)));
                }
            }
            if TraceIr::is_ins(ins.a) {
                let inner = *ir.at(ins.a);
                match inner.op {
                    TOp::Fabs => return Ok(Fold::Ref(ins.a)),
                    TOp::Fneg => return fold(ir, Ins { a: inner.a, ..ins }),
                    _ => {}
                }
            }
            Ok(Fold::Keep(ins))
        }
        TOp::Fsqrt => {
            if let Some(x) = ir.as_num(ins.a) {
                if !x.is_nan() && !x.sqrt().is_nan() {
                    return knum(ir, x.sqrt());
                }
            }
            Ok(Fold::Keep(ins))
        }
        TOp::Ffloor => {
            if let Some(x) = ir.as_num(ins.a) {
                if !x.is_nan() {
                    return knum(ir, x.floor());
                }
            }
            Ok(Fold::Keep(ins))
        }
        TOp::Fmin | TOp::Fmax => {
            if ins.a == ins.b {
                return Ok(Fold::Ref(ins.a));
            }
            if let (Some(x), Some(y)) = (ir.as_num(ins.a), ir.as_num(ins.b)) {
                if !x.is_nan() && !y.is_nan() {
                    // minsd / maxsd: the second operand unless the first is strictly smaller / larger.
                    let first = if ins.op == TOp::Fmin { x < y } else { x > y };
                    return Ok(Fold::Ref(if first { ins.a } else { ins.b }));
                }
            }
            Ok(Fold::Keep(ins))
        }
        TOp::ConvNumInt => {
            if let Some(x) = ir.as_int(ins.a) {
                return knum(ir, x as f64);
            }
            Ok(Fold::Keep(ins))
        }
        TOp::F2iExact => {
            if let Some(x) = ir.as_num(ins.a) {
                if !x.is_nan() {
                    return match float_to_int(x, F2I::Eq) {
                        Some(i) => kint(ir, i),
                        None if guarded => Err(IrAbort::ConstGuard),
                        None => Ok(Fold::Keep(ins)),
                    };
                }
            }
            Ok(Fold::Keep(ins))
        }
        // ---- guards ----
        TOp::Eq | TOp::Ne | TOp::Lt | TOp::Le | TOp::Gt | TOp::Ge | TOp::Ult | TOp::Uge => {
            if let (Some(x), Some(y)) = (ir.as_int(ins.a), ir.as_int(ins.b)) {
                return known(eval_icmp(ins.op, x, y));
            }
            if ins.a == ins.b {
                return known(matches!(ins.op, TOp::Eq | TOp::Le | TOp::Ge | TOp::Uge));
            }
            if matches!(ins.op, TOp::Eq | TOp::Ne) {
                canon(&mut ins, true);
            } else if TraceIr::is_const(ins.a) && !TraceIr::is_const(ins.b) {
                if let Some(m) = mirror(ins.op) {
                    std::mem::swap(&mut ins.a, &mut ins.b);
                    ins.op = m;
                }
            }
            Ok(Fold::Keep(ins))
        }
        TOp::Fcmp => {
            if let (Some(x), Some(y)) = (ir.as_num(ins.a), ir.as_num(ins.b)) {
                if !x.is_nan() && !y.is_nan() {
                    let bits = aux_bits(ins.aux);
                    return known(eval_fcmp(bits & 15, x, y) == (bits >> 4 & 1 != 0));
                }
            }
            Ok(Fold::Keep(ins))
        }
        TOp::Eqref => {
            if ins.a == ins.b {
                return Ok(Fold::Drop);
            }
            if TraceIr::is_const(ins.a) && TraceIr::is_const(ins.b) {
                return known(ir.kval(ins.a) == ir.kval(ins.b));
            }
            Ok(Fold::Keep(ins))
        }
        _ => Ok(Fold::Keep(ins)),
    }
}

/// Re-emits `src` through a fold / CSE [`Builder`] and returns the optimised trace. Snapshot maps are remapped to the
/// substituted refs; snapshot indices are unchanged, so an exit that is reached reports the same index. `Err` if a
/// guard folded to constant false or the result is too long.
pub fn optimize(src: &TraceIr, fold: bool, cse: bool) -> Result<TraceIr, IrAbort> {
    let mut b = Builder::new(fold, cse);
    let mut subst: Vec<Ref> = vec![NOREF; src.ins.len()];
    let mut phis: Vec<(Ref, Ref)> = Vec::new();
    // Constants are re-interned in order of first use.
    let map = |b: &mut Builder, subst: &[Ref], r: Ref| -> Ref {
        if r == NOREF {
            NOREF
        } else if r < REF_BIAS {
            b.ir.konst(src.k[usize::from(r) - 1])
        } else {
            subst[(r - REF_BIAS) as usize]
        }
    };
    for (i, ins) in src.ins.iter().enumerate() {
        let (ra, rb) = ins.op.ref_operands();
        let new = match ins.op {
            TOp::Loop => b.loop_marker()?,
            TOp::Phi => {
                let a = map(&mut b, &subst, ins.a);
                let r = b.emit_raw(Ins {
                    a,
                    prev: NOREF,
                    ..*ins
                })?;
                phis.push((r, ins.b));
                r
            }
            TOp::Kint | TOp::Knum | TOp::Kpri | TOp::Kptr if fold && TraceIr::is_const(ins.a) => {
                map(&mut b, &subst, ins.a)
            }
            TOp::Retf => {
                let a = map(&mut b, &subst, ins.a);
                b.emit_retf(a, ins.b, ins.aux)?
            }
            _ => {
                let a = if ra {
                    map(&mut b, &subst, ins.a)
                } else {
                    ins.a
                };
                let bb = if rb {
                    map(&mut b, &subst, ins.b)
                } else {
                    ins.b
                };
                let aux = match ins.third() {
                    Some(v) => ins.aux_with_third(map(&mut b, &subst, v)),
                    None => ins.aux,
                };
                b.emit_flags(ins.op, ins.t, ins.flags, a, bb, aux)?
            }
        };
        subst[i] = new;
    }
    for (phi, old_b) in phis {
        let nb = map(&mut b, &subst, old_b);
        b.set_phi(phi, nb);
    }
    let mut out = b.finish();
    out.snaps = src.snaps.clone();
    out.frames = src.frames.clone();
    out.segs = src.segs.clone();
    out.deps = src.deps.clone();
    out.head = src.head;
    out.extent = src.extent;
    out.depth = src.depth;
    out.counts = src.counts.clone();
    out.snapmap = Vec::with_capacity(src.snapmap.len());
    for e in &src.snapmap {
        let r = if e.r == NOREF {
            NOREF
        } else if e.r < REF_BIAS {
            out.konst(src.k[usize::from(e.r) - 1])
        } else {
            subst[(e.r - REF_BIAS) as usize]
        };
        out.snapmap.push(SnapEntry { r, ..*e });
    }
    Ok(out)
}
