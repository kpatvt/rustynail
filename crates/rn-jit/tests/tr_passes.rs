//! J5 P2 (docs/JIT_TRACE.md 9.1 item 2): constant folding (TR11, TR12, TR24e index normalisation) and CSE (6.2),
//! property-tested against the reference IR interpreter and the VM's own number helpers. Miri-runnable: the case
//! counts shrink under `cfg(miri)` (the repo's `scripts/miri.sh` `jit-trace` suite runs this binary).
//!
//! The mutants `fold_nan` and `fold_x_plus_zero` (feature `tr_mutants`, `RUSTYNAIL_JIT_MUT=name`) are caught by
//! `float_folds_never_decide_nan_or_signed_zero` (both) and `float_identities_differential` (`fold_x_plus_zero`).
#![cfg(feature = "trace")]

use rn_core::number::{float_to_int, idiv, imod, shiftl, shiftr, F2I};
use rn_jit::tr::fold::{fold, optimize, Fold};
use rn_jit::tr::interp::gen::{random_case, GenCfg, Rng, INTS, NUMS};
use rn_jit::tr::interp::{run, Model, Outcome, Val};
use rn_jit::tr::ir::*;

const CASES: u64 = if cfg!(miri) { 3 } else { 400 };
const SIZE: usize = if cfg!(miri) { 14 } else { 40 };

fn is_k(r: Ref) -> bool {
    TraceIr::is_const(r)
}

// ---- helpers: tiny traces with one observable result --------------------------------------------------------------

/// `x = Sload slot0 (typechecked)`; `r = f(builder, x)`; `Ustore r` into upvalue 0. `Err` if folding aborted.
fn build(
    fold: bool,
    cse: bool,
    xt: Ty,
    f: &dyn Fn(&mut Builder, Ref) -> Result<Ref, IrAbort>,
) -> Result<TraceIr, IrAbort> {
    let mut b = Builder::new(fold, cse);
    let s0 = b.ir.snapshot(Snap::default(), &[]);
    let x = b.emit_flags(TOp::Sload, xt, F_GUARD, 0, NOREF, guard_aux(s0, 1))?;
    let r = f(&mut b, x)?;
    let cl = b.ir.konst(KVal::Ptr(1, PtrKind::LClosure));
    let z = b.ir.kint(0);
    let u = b.emit(TOp::Uref, Ty::P64, cl, z, 0)?;
    // the stored value is observable whether it is a constant, `x`, or an instruction
    let r = if r == NOREF { x } else { r };
    b.emit(TOp::Ustore, Ty::Nil, u, r, 0)?;
    Ok(b.finish())
}

fn model_with(x: Val) -> Model {
    let mut m = Model::new();
    m.slots = vec![x];
    m.closures = vec![vec![Val::NIL]];
    m
}

fn go(ir: &TraceIr, x: Val) -> (Outcome, Model) {
    let mut m = model_with(x);
    let o = run(ir, &mut m).expect("valid IR");
    (o, m)
}

fn rand_ints(n: usize) -> Vec<i64> {
    let mut r = Rng::new(7);
    let mut v = INTS.to_vec();
    for _ in 0..n {
        v.push(r.next_u64() as i64);
    }
    v
}

fn rand_nums(n: usize) -> Vec<u64> {
    let mut r = Rng::new(9);
    let mut v = NUMS.to_vec();
    for _ in 0..n {
        v.push(r.next_u64());
    }
    v
}

// ---- fold == the VM's arithmetic -----------------------------------------------------------------------------------

fn folded_int(op: TOp, flags: u8, x: i64, y: i64) -> Result<Option<i64>, IrAbort> {
    let mut b = Builder::new(true, false);
    let s = b.ir.snapshot(Snap::default(), &[]);
    let (a, c) = (b.ir.kint(x), b.ir.kint(y));
    let r = b.emit_flags(op, Ty::Int, flags, a, c, guard_aux(s, 0))?;
    Ok(b.ir.as_int(r))
}

#[test]
fn int_const_folds_equal_the_vm_helpers() {
    let vals = rand_ints(if cfg!(miri) { 2 } else { 12 });
    for &x in &vals {
        for &y in &vals {
            let want: [(TOp, u8, Option<i64>); 10] = [
                (TOp::Add, 0, Some(x.wrapping_add(y))),
                (TOp::Sub, 0, Some(x.wrapping_sub(y))),
                (TOp::Mul, 0, Some(x.wrapping_mul(y))),
                (TOp::Band, 0, Some(x & y)),
                (TOp::Bor, 0, Some(x | y)),
                (TOp::Bxor, 0, Some(x ^ y)),
                (TOp::Shl, 0, Some(shiftl(x, y))),
                (TOp::Shr, 0, Some(shiftr(x, y))),
                (TOp::Idiv, F_GUARD, idiv(x, y)),
                (TOp::Mod, F_GUARD, imod(x, y)),
            ];
            for (op, flags, w) in want {
                match (folded_int(op, flags, x, y), w) {
                    (Ok(got), Some(w)) => assert_eq!(got, Some(w), "{op:?} {x} {y}"),
                    (Err(e), None) => assert_eq!(e, IrAbort::ConstGuard, "{op:?} {x} {y}"),
                    (g, w) => panic!("{op:?} {x} {y}: folded {g:?}, VM {w:?}"),
                }
            }
        }
        let mut b = Builder::new(true, false);
        let a = b.ir.kint(x);
        for (op, want) in [(TOp::Neg, x.wrapping_neg()), (TOp::Bnot, !x)] {
            let r = b.emit(op, Ty::Int, a, NOREF, 0).unwrap();
            assert_eq!(b.ir.as_int(r), Some(want), "{op:?} {x}");
        }
        let r = b.emit(TOp::ConvNumInt, Ty::Num, a, NOREF, 0).unwrap();
        assert_eq!(b.ir.as_num(r).map(f64::to_bits), Some((x as f64).to_bits()));
    }
}

#[test]
fn float_const_folds_equal_the_vm_arithmetic_or_stay() {
    let vals = rand_nums(if cfg!(miri) { 2 } else { 12 });
    for &xa in &vals {
        for &ya in &vals {
            let (x, y) = (f64::from_bits(xa), f64::from_bits(ya));
            for (op, f) in [
                (TOp::Fadd, (|x, y| x + y) as fn(f64, f64) -> f64),
                (TOp::Fsub, |x, y| x - y),
                (TOp::Fmul, |x, y| x * y),
                (TOp::Fdiv, |x, y| x / y),
            ] {
                let mut b = Builder::new(true, false);
                let (a, c) = (b.ir.konst(KVal::Num(xa)), b.ir.konst(KVal::Num(ya)));
                let r = b.emit(op, Ty::Num, a, c, 0).unwrap();
                let want = f(x, y);
                if x.is_nan() || y.is_nan() || want.is_nan() {
                    assert!(
                        !is_k(r),
                        "{op:?} {xa:#x} {ya:#x}: NaN decided at compile time"
                    );
                } else {
                    assert_eq!(b.ir.as_num(r).map(f64::to_bits), Some(want.to_bits()));
                }
            }
            for (op, first) in [(TOp::Fmin, x < y), (TOp::Fmax, x > y)] {
                let mut b = Builder::new(true, false);
                let (a, c) = (b.ir.konst(KVal::Num(xa)), b.ir.konst(KVal::Num(ya)));
                let r = b.emit(op, Ty::Num, a, c, 0).unwrap();
                if x.is_nan() || y.is_nan() {
                    assert!(!is_k(r) || xa == ya);
                } else {
                    assert_eq!(r, if first || xa == ya { a } else { c }, "{op:?}");
                }
            }
        }
        let x = f64::from_bits(xa);
        let mut b = Builder::new(true, false);
        let a = b.ir.konst(KVal::Num(xa));
        let neg = b.emit(TOp::Fneg, Ty::Num, a, NOREF, 0).unwrap();
        let abs = b.emit(TOp::Fabs, Ty::Num, a, NOREF, 0).unwrap();
        let sq = b.emit(TOp::Fsqrt, Ty::Num, a, NOREF, 0).unwrap();
        if x.is_nan() {
            assert!(!is_k(neg) && !is_k(abs) && !is_k(sq));
        } else {
            assert_eq!(b.ir.as_num(neg).map(f64::to_bits), Some(xa ^ (1 << 63)));
            assert_eq!(b.ir.as_num(abs).map(f64::to_bits), Some(xa & !(1 << 63)));
            if x.sqrt().is_nan() {
                assert!(!is_k(sq));
            } else {
                assert_eq!(b.ir.as_num(sq).map(f64::to_bits), Some(x.sqrt().to_bits()));
            }
        }
        // F2iExact: exact conversions fold, inexact ones are a constant false guard
        let mut b = Builder::new(true, false);
        let a = b.ir.konst(KVal::Num(xa));
        let s = b.ir.snapshot(Snap::default(), &[]);
        let r = b.emit_flags(TOp::F2iExact, Ty::Int, F_GUARD, a, NOREF, guard_aux(s, 0));
        match (x.is_nan(), float_to_int(x, F2I::Eq)) {
            (true, _) => assert!(!is_k(r.unwrap())),
            (false, Some(i)) => assert_eq!(b.ir.as_int(r.unwrap()), Some(i)),
            (false, None) => assert_eq!(r, Err(IrAbort::ConstGuard)),
        }
    }
}

/// TR11, and the catch for both mutants: nothing NaN-related or sign-of-zero-related is decided at compile time.
#[test]
fn float_folds_never_decide_nan_or_signed_zero() {
    let mut b = Builder::new(true, true);
    let z = b.ir.knum(0.0);
    let nan = b.ir.konst(KVal::Num(0x7FF0_0000_0000_0001));
    let one = b.ir.knum(1.0);
    let inf = b.ir.knum(f64::INFINITY);
    // NaN results
    for (op, a, c) in [
        (TOp::Fdiv, z, z),
        (TOp::Fsub, inf, inf),
        (TOp::Fmul, inf, z),
        (TOp::Fadd, nan, one),
        (TOp::Fadd, one, nan),
        (TOp::Fmul, nan, nan),
    ] {
        let r = b.emit(op, Ty::Num, a, c, 0).unwrap();
        assert!(TraceIr::is_ins(r), "{op:?} must not fold: NaN involved");
    }
    // x + 0.0 is not x (x = -0.0), x * 1.0 quiets a signalling NaN: no identity folds
    let s0 = b.ir.snapshot(Snap::default(), &[]);
    let x = b
        .emit_flags(TOp::Sload, Ty::Num, F_GUARD, 0, NOREF, guard_aux(s0, 1))
        .unwrap();
    let nz = b.ir.knum(-0.0);
    for (op, c) in [
        (TOp::Fadd, z),
        (TOp::Fadd, nz),
        (TOp::Fsub, z),
        (TOp::Fsub, nz),
        (TOp::Fmul, one),
        (TOp::Fdiv, one),
    ] {
        let r = b.emit(op, Ty::Num, x, c, 0).unwrap();
        assert!(
            TraceIr::is_ins(r) && r != x,
            "{op:?} x {c}: no float identity"
        );
    }
}

#[test]
fn float_identities_differential() {
    // every float op with x on either side and a constant from the NaN / zero / infinity table: folded == unfolded
    for &ca in NUMS {
        for op in [
            TOp::Fadd,
            TOp::Fsub,
            TOp::Fmul,
            TOp::Fdiv,
            TOp::Fmin,
            TOp::Fmax,
        ] {
            for const_left in [false, true] {
                let mk = |fold: bool| {
                    build(fold, true, Ty::Num, &|b, x| {
                        let k = b.ir.konst(KVal::Num(ca));
                        if const_left {
                            b.emit(op, Ty::Num, k, x, 0)
                        } else {
                            b.emit(op, Ty::Num, x, k, 0)
                        }
                    })
                    .unwrap()
                };
                let (raw, opt) = (mk(false), mk(true));
                for &xa in NUMS.iter() {
                    let x = Val {
                        t: Ty::Num,
                        bits: xa,
                    };
                    assert_eq!(
                        go(&raw, x),
                        go(&opt, x),
                        "{op:?} const_left={const_left} const={ca:#x} x={xa:#x}"
                    );
                }
            }
        }
    }
}

// ---- integer identities and index normalisation (TR12, TR24e) ---------------------------------------------------------

#[test]
fn int_identities_differential() {
    let xs = rand_ints(if cfg!(miri) { 1 } else { 6 });
    for &c in INTS {
        for op in [
            TOp::Add,
            TOp::Sub,
            TOp::Mul,
            TOp::Band,
            TOp::Bor,
            TOp::Bxor,
            TOp::Shl,
            TOp::Shr,
            TOp::Idiv,
            TOp::Mod,
        ] {
            for const_left in [false, true] {
                let guard = if matches!(op, TOp::Idiv | TOp::Mod) {
                    F_GUARD
                } else {
                    0
                };
                let mk = |fold: bool| {
                    build(fold, true, Ty::Int, &|b, x| {
                        let k = b.ir.kint(c);
                        let (p, q) = if const_left { (k, x) } else { (x, k) };
                        b.emit_flags(op, Ty::Int, guard, p, q, guard_aux(0, 0))
                    })
                };
                let raw = mk(false).unwrap();
                let Ok(opt) = mk(true) else {
                    // only a guard that can never hold may abort: x // 0, x % 0
                    assert!(guard != 0 && c == 0 && !const_left, "{op:?} {c}");
                    continue;
                };
                for &x in &xs {
                    assert_eq!(
                        go(&raw, Val::int(x)),
                        go(&opt, Val::int(x)),
                        "{op:?} const_left={const_left} c={c} x={x}"
                    );
                }
            }
        }
    }
}

#[test]
fn identities_simplify() {
    let mut b = Builder::new(true, true);
    let s0 = b.ir.snapshot(Snap::default(), &[]);
    let x = b
        .emit_flags(TOp::Sload, Ty::Int, F_GUARD, 0, NOREF, guard_aux(s0, 1))
        .unwrap();
    let (k0, k1, km1) = (b.ir.kint(0), b.ir.kint(1), b.ir.kint(-1));
    let e = |b: &mut Builder, op, a, c| b.emit(op, Ty::Int, a, c, 0).unwrap();
    assert_eq!(e(&mut b, TOp::Add, x, k0), x);
    assert_eq!(e(&mut b, TOp::Add, k0, x), x);
    assert_eq!(e(&mut b, TOp::Sub, x, k0), x);
    assert_eq!(e(&mut b, TOp::Mul, x, k1), x);
    assert_eq!(e(&mut b, TOp::Mul, k1, x), x);
    assert_eq!(e(&mut b, TOp::Mul, x, k0), k0);
    assert_eq!(e(&mut b, TOp::Band, x, km1), x);
    assert_eq!(e(&mut b, TOp::Bxor, x, k0), x);
    assert_eq!(e(&mut b, TOp::Bor, x, k0), x);
    assert_eq!(e(&mut b, TOp::Sub, x, x), k0);
    assert_eq!(e(&mut b, TOp::Shl, x, k0), x);
    let k64 = b.ir.kint(64);
    assert_eq!(e(&mut b, TOp::Shr, x, k64), k0);
    let n = e(&mut b, TOp::Neg, x, NOREF);
    assert_eq!(e(&mut b, TOp::Neg, n, NOREF), x);
    // x * -1 is a NEG (and then shares it)
    assert_eq!(e(&mut b, TOp::Mul, x, km1), n);
    // x // 1 = x, x % 1 = 0, x // -1 = -x with the guard gone
    let s = b.ir.snapshot(Snap::default(), &[]);
    let g = guard_aux(s, 0);
    assert_eq!(b.emit_flags(TOp::Idiv, Ty::Int, F_GUARD, x, k1, g), Ok(x));
    assert_eq!(b.emit_flags(TOp::Mod, Ty::Int, F_GUARD, x, km1, g), Ok(k0));
    assert_eq!(b.emit_flags(TOp::Idiv, Ty::Int, F_GUARD, x, km1, g), Ok(n));
    // a nonzero constant divisor needs no guard
    let k7 = b.ir.kint(7);
    let d = b.emit_flags(TOp::Idiv, Ty::Int, F_GUARD, x, k7, g).unwrap();
    assert_eq!(b.ir.at(d).flags & F_GUARD, 0);
    // a zero divisor: the guard can never hold
    assert_eq!(
        b.emit_flags(TOp::Mod, Ty::Int, F_GUARD, x, k0, g),
        Err(IrAbort::ConstGuard)
    );
}

#[test]
fn index_normalisation() {
    let mut b = Builder::new(true, true);
    let s0 = b.ir.snapshot(Snap::default(), &[]);
    let x = b
        .emit_flags(TOp::Sload, Ty::Int, F_GUARD, 0, NOREF, guard_aux(s0, 1))
        .unwrap();
    let e = |b: &mut Builder, op, a, c: i64| {
        let k = b.ir.kint(c);
        b.emit(op, Ty::Int, a, k, 0).unwrap()
    };
    // (i + 1) - 1 is i; (i - 1) + 1 is i
    let t = e(&mut b, TOp::Add, x, 1);
    assert_eq!(e(&mut b, TOp::Sub, t, 1), x);
    let u = e(&mut b, TOp::Sub, x, 1);
    assert_eq!(e(&mut b, TOp::Add, u, 1), x);
    // x - c is x + (-c), and constants combine: all spellings of x + 3 are one instruction
    let a3 = e(&mut b, TOp::Add, x, 3);
    let t = e(&mut b, TOp::Add, x, 10);
    assert_eq!(e(&mut b, TOp::Sub, t, 7), a3);
    let t = e(&mut b, TOp::Sub, x, 5);
    assert_eq!(e(&mut b, TOp::Add, t, 8), a3);
    let t = e(&mut b, TOp::Add, x, 1);
    let t = e(&mut b, TOp::Add, t, 2);
    assert_eq!(t, a3);
    let k3 = b.ir.kint(3);
    assert_eq!(b.emit(TOp::Add, Ty::Int, k3, x, 0), Ok(a3));
    // wrapping: x - MIN == x + MIN
    let m = e(&mut b, TOp::Sub, x, i64::MIN);
    assert_eq!(b.ir.at(m).op, TOp::Add);
    assert_eq!(b.ir.as_int(b.ir.at(m).b), Some(i64::MIN));
    // loop-carried shape: the body's index from a PHI-like operand normalises the same way
    assert_eq!(e(&mut b, TOp::Add, m, i64::MIN), x);
}

// ---- guards ----------------------------------------------------------------------------------------------------------

#[test]
fn guard_folds() {
    let mut b = Builder::new(true, true);
    let s = b.ir.snapshot(Snap::default(), &[]);
    let g = guard_aux(s, 0);
    let (k1, k2) = (b.ir.kint(1), b.ir.kint(2));
    // constants: true guards vanish, false ones are `ConstGuard`
    assert_eq!(b.emit(TOp::Lt, Ty::Int, k1, k2, g), Ok(NOREF));
    assert_eq!(
        b.emit(TOp::Lt, Ty::Int, k2, k1, g),
        Err(IrAbort::ConstGuard)
    );
    let m1 = b.ir.kint(-1);
    assert_eq!(
        b.emit(TOp::Ult, Ty::Int, k1, m1, g),
        Ok(NOREF),
        "-1 is huge unsigned"
    );
    assert_eq!(
        b.emit(TOp::Uge, Ty::Int, k1, m1, g),
        Err(IrAbort::ConstGuard)
    );
    assert_eq!(b.ir.ins.len(), 0);
    let x = b
        .emit_flags(TOp::Sload, Ty::Int, F_GUARD, 0, NOREF, guard_aux(s, 1))
        .unwrap();
    assert_eq!(b.emit(TOp::Eq, Ty::Int, x, x, g), Ok(NOREF));
    assert_eq!(b.emit(TOp::Ne, Ty::Int, x, x, g), Err(IrAbort::ConstGuard));
    assert_eq!(b.emit(TOp::Le, Ty::Int, x, x, g), Ok(NOREF));
    // constant on the left is mirrored so the two spellings CSE
    let a = b.emit(TOp::Lt, Ty::Int, x, k2, g).unwrap();
    let c = b.emit(TOp::Gt, Ty::Int, k2, x, g).unwrap();
    assert_eq!(a, c);
    assert_eq!(b.ir.at(a).op, TOp::Lt);
    // floats: constants fold unless NaN is involved
    let (f1, f2) = (b.ir.knum(1.0), b.ir.knum(2.0));
    assert_eq!(
        b.emit(TOp::Fcmp, Ty::Num, f1, f2, guard_aux(s, 2 | 16)),
        Ok(NOREF)
    );
    assert_eq!(
        b.emit(TOp::Fcmp, Ty::Num, f1, f2, guard_aux(s, 2)),
        Err(IrAbort::ConstGuard)
    );
    let nan = b.ir.konst(KVal::Num(0x7FF8_0000_0000_0000));
    let r = b
        .emit(TOp::Fcmp, Ty::Num, nan, f2, guard_aux(s, 2))
        .unwrap();
    assert!(TraceIr::is_ins(r), "a NaN compare is decided at run time");
    // Eqref
    let p = b.ir.konst(KVal::Ptr(16, PtrKind::Builtin));
    let q = b.ir.konst(KVal::Ptr(24, PtrKind::Builtin));
    assert_eq!(b.emit(TOp::Eqref, Ty::P64, p, p, g), Ok(NOREF));
    assert_eq!(
        b.emit(TOp::Eqref, Ty::P64, p, q, g),
        Err(IrAbort::ConstGuard)
    );
}

#[test]
fn raw_fold_api() {
    let mut ir = TraceIr::default();
    let (a, c) = (ir.kint(i64::MAX), ir.kint(1));
    let ins = Ins {
        op: TOp::Add,
        t: Ty::Int,
        flags: 0,
        a,
        b: c,
        prev: NOREF,
        aux: 0,
    };
    let Ok(Fold::Ref(r)) = fold(&mut ir, ins) else {
        panic!("const add folds")
    };
    assert_eq!(ir.as_int(r), Some(i64::MIN), "wraps");
}

// ---- optimize(): the differential property tests --------------------------------------------------------------------

/// Operands are defined before their use (a `PHI`'s back-edge operand excepted), constants exist, snapshots reference
/// defined refs, and guards carry the flag.
fn assert_well_formed(ir: &TraceIr) {
    for (n, i) in ir.ins.iter().enumerate() {
        let me = REF_BIAS + n as Ref;
        let (ra, rb) = i.op.ref_operands();
        for (is_ref, r) in [(ra, i.a), (rb && i.op != TOp::Phi, i.b)] {
            if is_ref && r != NOREF {
                if r >= REF_BIAS {
                    assert!(r < me, "{n}: operand {r:#x} not defined before");
                } else {
                    assert!(usize::from(r) <= ir.k.len(), "{n}: bad constant ref");
                }
            }
        }
        if i.op == TOp::Phi {
            assert!(i.b == NOREF || i.b >= REF_BIAS || usize::from(i.b) <= ir.k.len());
            assert!(i.a < me);
        }
        if i.op.always_guard() {
            assert!(i.flags & F_GUARD != 0);
        }
    }
    for e in &ir.snapmap {
        assert!(
            e.r == NOREF
                || (e.r >= REF_BIAS && usize::from(e.r - REF_BIAS) < ir.ins.len())
                || usize::from(e.r) <= ir.k.len()
        );
    }
}

#[derive(Default)]
struct Stats {
    cases: u32,
    const_guard: u32,
    removed: usize,
    exits: u32,
    done: u32,
    iterated: u32,
}

fn differential(seed0: u64, cases: u64, size: usize, st: &mut Stats) {
    for seed in seed0..seed0 + cases {
        let mut rng = Rng::new(seed);
        let case = random_case(&mut rng, GenCfg { size, loops: true });
        assert_well_formed(&case.ir);
        for (fold, cse) in [(true, false), (false, true), (true, true), (false, false)] {
            st.cases += 1;
            let opt = match optimize(&case.ir, fold, cse) {
                Ok(o) => o,
                Err(IrAbort::ConstGuard) => {
                    st.const_guard += 1;
                    continue;
                }
                Err(e) => panic!("seed {seed}: {e:?}"),
            };
            assert_well_formed(&opt);
            assert!(opt.ins.len() <= case.ir.ins.len(), "seed {seed}: grew");
            if fold || cse {
                st.removed += case.ir.ins.len() - opt.ins.len();
            } else {
                assert_eq!(opt.ins.len(), case.ir.ins.len());
            }
            for (mi, m0) in case.models.iter().enumerate() {
                let (mut ma, mut mb) = (m0.clone(), m0.clone());
                let ra = run(&case.ir, &mut ma).unwrap_or_else(|e| panic!("seed {seed}: {e:?}"));
                let rb = run(&opt, &mut mb).unwrap_or_else(|e| panic!("seed {seed}: {e:?}"));
                assert_eq!(
                    ra,
                    rb,
                    "seed {seed} fold={fold} cse={cse} model {mi}\n{}\n=>\n{}",
                    rn_jit::tr::print::dump(&case.ir),
                    rn_jit::tr::print::dump(&opt)
                );
                assert_eq!(
                    ma, mb,
                    "seed {seed} fold={fold} cse={cse} model {mi}: final state"
                );
                match ra {
                    Outcome::Exit { .. } => st.exits += 1,
                    Outcome::Done { iters } => {
                        st.done += 1;
                        st.iterated += u32::from(iters > 0);
                    }
                }
            }
        }
    }
}

/// Random well-typed IR (integer / float / array / upvalue ops, guards, a loop with PHIs): the optimised trace gives the
/// same exits (snapshot and resolved slots), final heap state and iteration count as the original, for every
/// `-fold` / `-cse` combination.
#[test]
fn optimize_preserves_the_interpreter_result() {
    let mut st = Stats::default();
    differential(1, CASES, SIZE, &mut st);
    eprintln!(
        "cases {} const_guard {} removed {} exits {} done {} iterated {}",
        st.cases, st.const_guard, st.removed, st.exits, st.done, st.iterated
    );
    if !cfg!(miri) {
        assert!(
            st.exits > 100 && st.done > 100,
            "generator reaches exits and ends"
        );
        assert!(st.iterated > 10, "loops iterate");
        assert!(st.removed > 100, "the passes do something");
        assert!(st.const_guard * 20 < st.cases, "ConstGuard is rare");
    }
}

/// Larger traces, other seeds (not in the Miri-sized run).
#[test]
#[cfg_attr(miri, ignore)]
fn optimize_preserves_the_interpreter_result_large() {
    let mut st = Stats::default();
    differential(100_000, 60, 150, &mut st);
    assert!(st.exits > 10 && st.done > 10);
}

#[test]
fn optimize_remaps_snapshots_and_phis() {
    // x + 0 folds to x; the snapshot entry that referenced the ADD must now reference the load
    let mut b = Builder::new(false, false);
    let s0 = b.ir.snapshot(Snap::default(), &[]);
    let x = b
        .emit_flags(TOp::Sload, Ty::Int, F_GUARD, 0, NOREF, guard_aux(s0, 1))
        .unwrap();
    let k0 = b.ir.kint(0);
    let y = b.emit(TOp::Add, Ty::Int, x, k0, 0).unwrap();
    let k9 = b.ir.kint(9);
    let sn = b.ir.snapshot(
        Snap::default(),
        &[SnapEntry {
            slot: 4,
            r: y,
            flags: 0,
        }],
    );
    let l = b.loop_marker().unwrap();
    let _ = l;
    let p = b.phi(Ty::Int, y).unwrap();
    let z = b.emit(TOp::Add, Ty::Int, p, k9, 0).unwrap();
    b.set_phi(p, z);
    b.emit(TOp::Lt, Ty::Int, z, k9, guard_aux(sn, 0)).unwrap();
    let ir = b.finish();
    let opt = optimize(&ir, true, true).unwrap();
    assert_eq!(opt.ins.len(), ir.ins.len() - 1);
    let sx = REF_BIAS;
    assert_eq!(opt.snap_entries(1)[0].r, sx);
    let phi = opt.ins.iter().find(|i| i.op == TOp::Phi).unwrap();
    assert_eq!(phi.a, sx);
    assert!(phi.b >= REF_BIAS && opt.at(phi.b).op == TOp::Add);
    for x in [-5i64, 0, 3, 100] {
        for n in 0..4 {
            let mk = || {
                let mut m = model_with(Val::int(x));
                m.max_iters = n;
                m
            };
            let (mut a, mut c) = (mk(), mk());
            assert_eq!(run(&ir, &mut a), run(&opt, &mut c));
        }
    }
}

#[test]
fn const_guard_aborts_optimize() {
    let mut b = Builder::new(false, false);
    let s0 = b.ir.snapshot(Snap::default(), &[]);
    let (k1, k2) = (b.ir.kint(1), b.ir.kint(2));
    b.emit(TOp::Gt, Ty::Int, k1, k2, guard_aux(s0, 0)).unwrap();
    let ir = b.finish();
    assert!(matches!(
        optimize(&ir, true, true),
        Err(IrAbort::ConstGuard)
    ));
    // without -fold the guard is kept and exits at run time
    let kept = optimize(&ir, false, true).unwrap();
    assert_eq!(
        run(&kept, &mut Model::new()),
        Ok(Outcome::Exit {
            snap: 0,
            slots: vec![]
        })
    );
}

// ---- P7: memory forwarding (tr::opt_loop / tr::opt_mem) on the P2 generator -------------------------------------------------

/// The P2 random IR with its loop removed (the P7 pass takes the rolled, closed iteration): forwarding and redundant-load
/// elimination (`-loop`, `+fwd`) must not change any exit (snapshot indices may differ, so the restored state and the
/// snapshot's `pc` are compared), the final heap or the budget. The generator's tables are distinct objects and its
/// upvalue cells are separate, so this checks the *must* / *no* answers of TR13 (the `(idx + c) - c` shapes, repeated
/// accesses, stores in between); the may-alias adversaries are in `tr_loop.rs`.
#[test]
fn forwarding_preserves_the_interpreter_result() {
    use rn_jit::tr::opt_loop;
    use rn_jit::tr::record::Recorded;
    let mut removed = 0usize;
    let mut compared = 0u32;
    for seed in 1..=CASES * 2 {
        let mut rng = Rng::new(seed);
        let case = random_case(
            &mut rng,
            GenCfg {
                size: SIZE,
                loops: false,
            },
        );
        // make every snapshot's `pc` unique so that exits can be told apart after the pass renumbers snapshots
        let mut ir = case.ir.clone();
        for (i, s) in ir.snaps.iter_mut().enumerate() {
            s.pc = i as u32;
        }
        let nsn = ir.snaps.len();
        let rec = Recorded {
            ir,
            writeback: Vec::new(),
            path: vec![0],
            cost: vec![1],
            snap_step: vec![0; nsn],
            path_proto: vec![ProtoKey(0)],
            path_depth: vec![0],
            room: 0,
            entry_snap: None,
            n: 1,
            rerec: 0,
            side: None,
            stitched: false,
        };
        let mut opt = rec.clone();
        let o = rn_jit::tr::TraceOpts {
            loop_opt: false,
            fwd: true,
            ..rn_jit::tr::TraceOpts::default()
        };
        opt_loop::run(&mut opt, &o).unwrap();
        opt_loop::check(&opt.ir)
            .unwrap_or_else(|e| panic!("seed {seed}: {e}\n{}", rn_jit::tr::print::dump(&opt.ir)));
        removed += rec.ir.ins.len().saturating_sub(opt.ir.ins.len());
        for (mi, m0) in case.models.iter().enumerate() {
            let (mut ma, mut mb) = (m0.clone(), m0.clone());
            let ra = run(&rec.ir, &mut ma).unwrap();
            let rb = run(&opt.ir, &mut mb).unwrap();
            let key = |ir: &TraceIr, o: &Outcome| match o {
                Outcome::Done { iters } => (u32::MAX, vec![], *iters),
                Outcome::Exit { snap, slots } => (ir.snaps[*snap as usize].pc, slots.clone(), 0),
            };
            assert_eq!(
                key(&rec.ir, &ra),
                key(&opt.ir, &rb),
                "seed {seed} model {mi}\n{}\n=>\n{}",
                rn_jit::tr::print::dump(&rec.ir),
                rn_jit::tr::print::dump(&opt.ir)
            );
            assert_eq!(ma, mb, "seed {seed} model {mi}: final state");
            compared += 1;
        }
    }
    if !cfg!(miri) {
        assert!(compared > 1000);
        assert!(removed > 50, "forwarding removes something: {removed}");
    }
}

/// N2b: `optimize` keeps a `RETF` (a guard: never dead, never merged), its segment table and the segment numbers of the snapshots.
#[test]
fn optimize_keeps_retf_and_segments() {
    let mut b = Builder::new(false, false);
    let s0 = b.ir.snapshot(Snap::default(), &[]);
    let x = b.emit_flags(TOp::Sload, Ty::Int, F_GUARD, 0, NOREF, guard_aux(s0, 1)).unwrap();
    let k = b.ir.kint(3);
    let _dead = b.emit(TOp::Add, Ty::Int, x, k, 0).unwrap();
    let site = b.ir.konst(KVal::Ptr(0x1000, PtrKind::Raw));
    b.emit_retf(site, 2, guard_aux(s0, 2)).unwrap();
    let s1 = b.ir.snapshot(Snap { seg: 1, ..Snap::default() }, &[]);
    let y = b.emit_flags(TOp::Sload, Ty::Int, F_GUARD, 0, NOREF, guard_aux(s1, 1)).unwrap();
    let cl = b.ir.konst(KVal::Ptr(1, PtrKind::LClosure));
    let z = b.ir.kint(0);
    let u = b.emit(TOp::Uref, Ty::P64, cl, z, 0).unwrap();
    b.emit(TOp::Ustore, Ty::Nil, u, y, 0).unwrap();
    let mut ir = b.finish();
    ir.segs = vec![
        Seg { proto: ProtoKey(1), code: 0, shift: 0, pops: 0, call_pc: 0, a: 0, retf_ins: NOREF },
        Seg { proto: ProtoKey(2), code: 0, shift: 2, pops: 1, call_pc: 5, a: 1, retf_ins: NOREF },
    ];
    let opt = optimize(&ir, true, true).unwrap();
    assert_eq!(opt.ins.iter().filter(|i| i.op == TOp::Retf).count(), 1, "RETF survives");
    assert_eq!(opt.segs, ir.segs, "segment table survives");
    assert_eq!(opt.snaps[1].seg, 1, "snapshot segment survives");
    let mk = || {
        let mut m = model_with(Val::int(9));
        m.retf_below = vec![Val::int(1), Val::int(2)];
        m
    };
    let (mut a, mut c) = (mk(), mk());
    assert_eq!(run(&ir, &mut a), run(&opt, &mut c));
    assert_eq!(a.closures, c.closures);
    assert_eq!(c.closures[0][0], Val::int(1), "the SLOAD after the pop reads the caller's register 0");
}
