//! J5 P2 (docs/JIT_TRACE.md 9.1 item 1): the IR builder, constant interning, CSE chains, the printer and the reference
//! IR interpreter's per-op semantics (against `rn_core::number`, the VM's own helpers). Miri-runnable (pure Rust).
#![cfg(feature = "trace")]

use rn_core::number::{fmod_lua, idiv, imod, pow_lua, shiftl, shiftr};
use rn_jit::tr::interp::gen::{INTS, NUMS};
use rn_jit::tr::interp::{run, Model, Outcome, Val, CALLN_FLOOR, CALLN_FMOD, CALLN_POW};
use rn_jit::tr::ir::*;
use rn_jit::tr::print::dump;

const DEFAULT_NAN: u64 = 0xFFF8_0000_0000_0000;

/// `Sload slot0 (+ slot1)` -> `op` -> `Ustore` into upvalue 0; returns the run's outcome and the stored value.
fn eval(op: TOp, t: Ty, flags: u8, x: Val, y: Option<Val>, aux: u32) -> (Outcome, Val) {
    let mut b = Builder::new(false, false);
    let s0 = b.ir.snapshot(Snap::default(), &[]);
    let a = b
        .emit_flags(TOp::Sload, x.t, F_GUARD, 0, NOREF, guard_aux(s0, 1))
        .unwrap();
    let bb = match y {
        Some(y) => b
            .emit_flags(TOp::Sload, y.t, F_GUARD, 1, NOREF, guard_aux(s0, 1))
            .unwrap(),
        None => NOREF,
    };
    let r = b.emit_flags(op, t, flags, a, bb, aux).unwrap();
    let cl = b.ir.konst(KVal::Ptr(1, PtrKind::LClosure));
    let z = b.ir.kint(0);
    let u = b.emit(TOp::Uref, Ty::P64, cl, z, 0).unwrap();
    b.emit(TOp::Ustore, Ty::Nil, u, r, 0).unwrap();
    let ir = b.finish();
    let mut m = Model::new();
    m.slots = vec![x, y.unwrap_or(Val::NIL)];
    m.closures = vec![vec![Val::NIL]];
    let out = run(&ir, &mut m).expect("valid IR");
    (out, m.closures[0][0])
}

fn ints() -> Vec<i64> {
    let mut v = INTS.to_vec();
    if cfg!(miri) {
        v.truncate(10);
    }
    v
}

fn nums() -> Vec<u64> {
    NUMS.to_vec()
}

#[test]
fn constants_are_interned() {
    let mut t = TraceIr::default();
    let a = t.kint(5);
    assert_eq!(a, t.kint(5));
    assert_ne!(a, t.kint(6));
    let z = t.knum(0.0);
    let nz = t.knum(-0.0);
    assert_ne!(z, nz, "interning is by bits");
    let n1 = t.konst(KVal::Num(DEFAULT_NAN));
    let n2 = t.konst(KVal::Num(0x7FF0_0000_0000_0001));
    assert_ne!(n1, n2, "NaN payloads stay distinct");
    assert_eq!(n1, t.konst(KVal::Num(DEFAULT_NAN)));
    assert_ne!(
        t.konst(KVal::Ptr(8, PtrKind::Raw)),
        t.konst(KVal::Ptr(8, PtrKind::Proto))
    );
    assert_ne!(t.konst(KVal::Pri(Ty::Nil)), t.konst(KVal::Pri(Ty::True)));
    assert_eq!(t.k.len(), 10);
    assert_eq!(t.as_int(a), Some(5));
    assert_eq!(t.as_int(z), None);
    assert_eq!(t.as_num(nz).map(f64::to_bits), Some((-0.0f64).to_bits()));
    assert_eq!(t.ty_of(z), Ty::Num);
}

fn two_loads(b: &mut Builder) -> (Ref, Ref) {
    let s = b.ir.snapshot(Snap::default(), &[]);
    let x = b
        .emit_flags(TOp::Sload, Ty::Int, F_GUARD, 0, NOREF, guard_aux(s, 1))
        .unwrap();
    let y = b
        .emit_flags(TOp::Sload, Ty::Int, F_GUARD, 1, NOREF, guard_aux(s, 1))
        .unwrap();
    (x, y)
}

#[test]
fn cse_chains_and_limits() {
    let mut b = Builder::new(false, true);
    let (x, y) = two_loads(&mut b);
    let a1 = b.emit(TOp::Add, Ty::Int, x, y, 0).unwrap();
    let m1 = b.emit(TOp::Mul, Ty::Int, x, y, 0).unwrap();
    let a2 = b.emit(TOp::Add, Ty::Int, x, y, 0).unwrap();
    assert_eq!(a1, a2, "identical pure op is shared");
    assert_ne!(a1, m1);
    // fold is off: the swapped spelling is a different instruction, linked into the chain
    let a3 = b.emit(TOp::Add, Ty::Int, y, x, 0).unwrap();
    assert_ne!(a3, a1);
    assert_eq!(b.ir.at(a3).prev, a1, "per-op chain");
    assert_eq!(b.ir.at(a1).prev, NOREF);
    assert_eq!(b.ir.at(m1).prev, NOREF);
    // different type or aux: not shared
    let a4 = b.emit(TOp::Add, Ty::I64, x, y, 0).unwrap();
    assert_ne!(a4, a1);
    let c1 = b.emit(TOp::Calln, Ty::Num, x, y, CALLN_POW).unwrap();
    let c2 = b.emit(TOp::Calln, Ty::Num, x, y, CALLN_FMOD).unwrap();
    assert_ne!(c1, c2);
    // floats keep operand order (TR11): never swapped, so with fold on the two spellings stay apart
    let mut bf = Builder::new(true, true);
    let (x, y) = two_loads(&mut bf);
    let f1 = bf.emit(TOp::Fadd, Ty::Num, x, y, 0).unwrap();
    let f2 = bf.emit(TOp::Fadd, Ty::Num, y, x, 0).unwrap();
    assert_ne!(f1, f2);
    // integer commutative ops are canonicalised by fold, so both spellings share
    let i1 = bf.emit(TOp::Add, Ty::Int, x, y, 0).unwrap();
    let i2 = bf.emit(TOp::Add, Ty::Int, y, x, 0).unwrap();
    assert_eq!(i1, i2);
}

#[test]
fn cse_off_and_non_cse_ops() {
    let mut b = Builder::new(true, false);
    let (x, y) = two_loads(&mut b);
    let a1 = b.emit(TOp::Add, Ty::Int, x, y, 0).unwrap();
    let a2 = b.emit(TOp::Add, Ty::Int, x, y, 0).unwrap();
    assert_ne!(a1, a2, "-cse");
    let mut b = Builder::new(true, true);
    let (x, _) = two_loads(&mut b);
    // loads, polls, budgets and checked slot loads are never merged by this pass (6.2: loads go through 6.6)
    let s = b.ir.snapshot(Snap::default(), &[]);
    let l1 = b
        .emit(TOp::Fload, Ty::U32, x, Field::Asize as Ref, 0)
        .unwrap();
    let l2 = b
        .emit(TOp::Fload, Ty::U32, x, Field::Asize as Ref, 0)
        .unwrap();
    assert_ne!(l1, l2);
    let p1 = b
        .emit(TOp::Poll, Ty::Nil, NOREF, NOREF, guard_aux(s, 0))
        .unwrap();
    let p2 = b
        .emit(TOp::Poll, Ty::Nil, NOREF, NOREF, guard_aux(s, 0))
        .unwrap();
    assert_ne!(p1, p2);
    let b1 = b
        .emit(TOp::Budget, Ty::I64, 2, NOREF, guard_aux(s, 0))
        .unwrap();
    let b2 = b
        .emit(TOp::Budget, Ty::I64, 2, NOREF, guard_aux(s, 0))
        .unwrap();
    assert_ne!(b1, b2);
    let s1 = b
        .emit_flags(TOp::Sload, Ty::Int, F_GUARD, 0, NOREF, guard_aux(s, 1))
        .unwrap();
    assert_ne!(s1, x);
}

#[test]
fn guard_cse_ignores_the_snapshot_but_not_the_condition() {
    let mut b = Builder::new(false, true);
    let (x, y) = two_loads(&mut b);
    let (s1, s2) = (
        b.ir.snapshot(Snap::default(), &[]),
        b.ir.snapshot(Snap::default(), &[]),
    );
    let g1 = b.emit(TOp::Lt, Ty::Int, x, y, guard_aux(s1, 0)).unwrap();
    let n = b.ir.ins.len();
    let g2 = b.emit(TOp::Lt, Ty::Int, x, y, guard_aux(s2, 0)).unwrap();
    assert_eq!(g1, g2);
    assert_eq!(b.ir.ins.len(), n, "the dominated guard is removed");
    assert!(b.ir.at(g1).flags & F_GUARD != 0);
    let g3 = b.emit(TOp::Le, Ty::Int, x, y, guard_aux(s2, 0)).unwrap();
    assert_ne!(g3, g1);
    // Fcmp: same operands but another condition or expectation is another guard (never a negated cc, TR11)
    let (f, g) = {
        let a = b.ir.knum(1.0);
        let c = b.emit(TOp::Sload, Ty::Num, 2, NOREF, 0).unwrap();
        (a, c)
    };
    let c1 = b
        .emit(TOp::Fcmp, Ty::Num, g, f, guard_aux(s1, 2 | 16))
        .unwrap();
    let c2 = b
        .emit(TOp::Fcmp, Ty::Num, g, f, guard_aux(s2, 2 | 16))
        .unwrap();
    let c3 = b.emit(TOp::Fcmp, Ty::Num, g, f, guard_aux(s2, 2)).unwrap();
    let c4 = b
        .emit(TOp::Fcmp, Ty::Num, g, f, guard_aux(s2, 3 | 16))
        .unwrap();
    assert_eq!(c1, c2);
    assert!(c3 != c1 && c4 != c1 && c3 != c4);
}

#[test]
fn too_long() {
    let mut b = Builder::new(false, false);
    for i in 0..MAX_INS {
        b.emit(TOp::Sload, Ty::Int, (i % 100) as Ref, NOREF, 0)
            .unwrap();
    }
    assert_eq!(
        b.emit(TOp::Sload, Ty::Int, 0, NOREF, 0),
        Err(IrAbort::TooLong)
    );
    assert_eq!(b.ir.ins.len(), MAX_INS);
}

#[test]
fn printer_golden() {
    let mut b = Builder::new(true, true);
    let s0 = b.ir.snapshot(Snap::default(), &[]);
    let x = b
        .emit_flags(TOp::Sload, Ty::Int, F_GUARD, 3, NOREF, guard_aux(s0, 1))
        .unwrap();
    let k1 = b.ir.kint(1);
    let a = b.emit(TOp::Add, Ty::Int, x, k1, 0).unwrap();
    let f = b.ir.knum(2.5);
    let n = b.ir.knum(-0.0);
    let c = b.emit(TOp::ConvNumInt, Ty::Num, a, NOREF, 0).unwrap();
    let m = b.emit(TOp::Fmul, Ty::Num, c, f, 0).unwrap();
    let s1 = b.ir.snapshot(
        Snap {
            pc: 7,
            frame: 0,
            ..Snap::default()
        },
        &[
            SnapEntry {
                slot: 3,
                r: a,
                flags: 0,
            },
            SnapEntry {
                slot: 4,
                r: n,
                flags: 0,
            },
        ],
    );
    b.emit(TOp::Fcmp, Ty::Num, m, f, guard_aux(s1, 2 | 16))
        .unwrap();
    b.emit(TOp::Budget, Ty::I64, 2, NOREF, guard_aux(s0, 0))
        .unwrap();
    b.loop_marker().unwrap();
    let p = b.phi(Ty::Int, a).unwrap();
    b.set_phi(p, a);
    let text = dump(&b.finish());
    let want = "\
0000 >   SLOAD  int   slot3  S0 bits=0x1
0001     ADD    int   %0 #1
0002     CONV   num   %1 -
0003     FMUL   num   %2 #2.5
0004 >   FCMP   num   %3 #2.5  S1 bits=0x12
0005 >   BUDGET i64   2  S0
0006     LOOP   i64
0007  +  PHI    int   %1 %1
S0 pc=0 frame=0 []
S1 pc=7 frame=0 [3=%1 4=#-0.0]
";
    assert_eq!(text, want);
}

#[test]
fn int_ops_match_the_vm_helpers() {
    let vals = ints();
    for &x in &vals {
        for &y in &vals {
            let (vx, vy) = (Val::int(x), Some(Val::int(y)));
            let check = |op: TOp, flags: u8, want: Option<i64>| {
                let (out, got) = eval(op, Ty::Int, flags, vx, vy, guard_aux(0, 0));
                match want {
                    Some(w) => {
                        assert_eq!(out, Outcome::Done { iters: 0 }, "{op:?} {x} {y}");
                        assert_eq!(got, Val::int(w), "{op:?} {x} {y}");
                    }
                    None => assert!(
                        matches!(out, Outcome::Exit { snap: 0, .. }),
                        "{op:?} {x} {y}"
                    ),
                }
            };
            check(TOp::Add, 0, Some(x.wrapping_add(y)));
            check(TOp::Sub, 0, Some(x.wrapping_sub(y)));
            check(TOp::Mul, 0, Some(x.wrapping_mul(y)));
            check(TOp::Band, 0, Some(x & y));
            check(TOp::Bor, 0, Some(x | y));
            check(TOp::Bxor, 0, Some(x ^ y));
            check(TOp::Shl, 0, Some(shiftl(x, y)));
            check(TOp::Shr, 0, Some(shiftr(x, y)));
            check(TOp::Idiv, F_GUARD, idiv(x, y));
            check(TOp::Mod, F_GUARD, imod(x, y));
        }
        let (out, got) = eval(TOp::Neg, Ty::Int, 0, Val::int(x), None, 0);
        assert_eq!(
            (out, got),
            (Outcome::Done { iters: 0 }, Val::int(x.wrapping_neg()))
        );
        let (_, got) = eval(TOp::Bnot, Ty::Int, 0, Val::int(x), None, 0);
        assert_eq!(got, Val::int(!x));
        let (_, got) = eval(TOp::ConvNumInt, Ty::Num, 0, Val::int(x), None, 0);
        assert_eq!(got, Val::num(x as f64));
    }
}

#[test]
fn float_ops_follow_the_emulator_nan_rules() {
    let vals = nums();
    for &a in &vals {
        for &b in &vals {
            let (x, y) = (f64::from_bits(a), f64::from_bits(b));
            let flt = |bits| Val { t: Ty::Num, bits };
            for (op, f) in [
                (TOp::Fadd, (|x, y| x + y) as fn(f64, f64) -> f64),
                (TOp::Fsub, |x, y| x - y),
                (TOp::Fmul, |x, y| x * y),
                (TOp::Fdiv, |x, y| x / y),
            ] {
                let (_, got) = eval(op, Ty::Num, 0, flt(a), Some(flt(b)), 0);
                let want = if x.is_nan() {
                    a | 0x0008_0000_0000_0000
                } else if y.is_nan() {
                    b | 0x0008_0000_0000_0000
                } else if f(x, y).is_nan() {
                    DEFAULT_NAN
                } else {
                    f(x, y).to_bits()
                };
                assert_eq!(got.bits, want, "{op:?} {a:#x} {b:#x}");
            }
            let (_, got) = eval(TOp::Fmin, Ty::Num, 0, flt(a), Some(flt(b)), 0);
            assert_eq!(got.bits, if x < y { a } else { b });
            let (_, got) = eval(TOp::Fmax, Ty::Num, 0, flt(a), Some(flt(b)), 0);
            assert_eq!(got.bits, if x > y { a } else { b });
            for cc in 0..6u32 {
                for expect in [false, true] {
                    let aux = guard_aux(0, cc | u32::from(expect) << 4);
                    let (out, _) = eval(TOp::Fcmp, Ty::Num, F_GUARD, flt(a), Some(flt(b)), aux);
                    let holds = match cc {
                        0 => x == y,
                        1 => x != y,
                        2 => x < y,
                        3 => x <= y,
                        4 => x > y,
                        _ => x >= y,
                    };
                    assert_eq!(
                        matches!(out, Outcome::Done { .. }),
                        holds == expect,
                        "fcmp {cc} {expect}"
                    );
                }
            }
        }
        let flt = Val {
            t: Ty::Num,
            bits: a,
        };
        let x = f64::from_bits(a);
        let (_, got) = eval(TOp::Fneg, Ty::Num, 0, flt, None, 0);
        assert_eq!(got.bits, a ^ (1 << 63));
        let (_, got) = eval(TOp::Fabs, Ty::Num, 0, flt, None, 0);
        assert_eq!(got.bits, a & !(1 << 63));
        let (_, got) = eval(TOp::Fsqrt, Ty::Num, 0, flt, None, 0);
        let want = if x.is_nan() {
            a | 0x0008_0000_0000_0000
        } else if x < 0.0 {
            DEFAULT_NAN
        } else {
            x.sqrt().to_bits()
        };
        assert_eq!(got.bits, want);
        let (out, got) = eval(TOp::F2iExact, Ty::Int, F_GUARD, flt, None, guard_aux(0, 0));
        match rn_core::number::float_to_int(x, rn_core::number::F2I::Eq) {
            Some(i) => assert_eq!((out, got), (Outcome::Done { iters: 0 }, Val::int(i))),
            None => assert!(matches!(out, Outcome::Exit { snap: 0, .. })),
        }
        let (_, got) = eval(TOp::Calln, Ty::Num, 0, flt, Some(flt), CALLN_POW);
        let want = pow_lua(x, x);
        // the pure helpers pass a NaN's sign / payload through bit for bit, like the VM's `pow_lua` / `fmod_lua`
        assert_eq!(got.bits, want.to_bits());
        let (_, got) = eval(TOp::Calln, Ty::Num, 0, flt, Some(flt), CALLN_FMOD);
        let want = fmod_lua(x, x);
        // the pure helpers pass a NaN's sign / payload through bit for bit, like the VM's `pow_lua` / `fmod_lua`
        assert_eq!(got.bits, want.to_bits());
        let (_, got) = eval(TOp::Calln, Ty::Num, 0, flt, None, CALLN_FLOOR);
        let want = x.floor();
        // the pure helpers pass a NaN's sign / payload through bit for bit, like the VM's `pow_lua` / `fmod_lua`
        assert_eq!(got.bits, want.to_bits());
    }
}

#[test]
fn integer_compare_guards_signed_and_unsigned() {
    let vals = ints();
    for &x in &vals {
        for &y in &vals {
            for (op, holds) in [
                (TOp::Eq, x == y),
                (TOp::Ne, x != y),
                (TOp::Lt, x < y),
                (TOp::Le, x <= y),
                (TOp::Gt, x > y),
                (TOp::Ge, x >= y),
                (TOp::Ult, (x as u64) < (y as u64)),
                (TOp::Uge, (x as u64) >= (y as u64)),
            ] {
                let (out, _) = eval(
                    op,
                    Ty::Int,
                    0,
                    Val::int(x),
                    Some(Val::int(y)),
                    guard_aux(0, 0),
                );
                assert_eq!(matches!(out, Outcome::Done { .. }), holds, "{op:?} {x} {y}");
            }
        }
    }
}

#[test]
fn typecheck_exit_and_snapshot_resolution() {
    let mut b = Builder::new(false, false);
    let s0 = b.ir.snapshot(Snap::default(), &[]);
    let x = b
        .emit_flags(TOp::Sload, Ty::Int, F_GUARD, 0, NOREF, guard_aux(s0, 1))
        .unwrap();
    let k = b.ir.knum(2.5);
    let s1 = b.ir.snapshot(
        Snap::default(),
        &[
            SnapEntry {
                slot: 9,
                r: x,
                flags: 0,
            },
            SnapEntry {
                slot: 2,
                r: k,
                flags: SNAP_FUNC,
            },
        ],
    );
    let k5 = b.ir.kint(5);
    b.emit(TOp::Lt, Ty::Int, x, k5, guard_aux(s1, 0)).unwrap();
    let ir = b.finish();
    let mut m = Model::new();
    m.slots = vec![Val::num(1.0)];
    assert_eq!(
        run(&ir, &mut m),
        Ok(Outcome::Exit {
            snap: 0,
            slots: vec![]
        }),
        "wrong type at entry"
    );
    m.slots = vec![Val::int(9)];
    assert_eq!(
        run(&ir, &mut m),
        Ok(Outcome::Exit {
            snap: 1,
            slots: vec![(9, 0, Val::int(9)), (2, SNAP_FUNC, Val::num(2.5)),]
        })
    );
    m.slots = vec![Val::int(4)];
    assert_eq!(run(&ir, &mut m), Ok(Outcome::Done { iters: 0 }));
}

#[test]
fn arrays_upvalues_budget_and_poll() {
    let mut b = Builder::new(false, false);
    let s0 = b.ir.snapshot(Snap::default(), &[]);
    let t = b.ir.konst(KVal::Ptr(1, PtrKind::Raw));
    let asize = b
        .emit(TOp::Fload, Ty::U32, t, Field::Asize as Ref, 0)
        .unwrap();
    let k2 = b.ir.kint(2);
    b.emit(TOp::Ult, Ty::I64, k2, asize, guard_aux(s0, 0))
        .unwrap();
    let arr = b
        .emit(TOp::Fload, Ty::P64, t, Field::Array as Ref, 0)
        .unwrap();
    let aref = b.emit(TOp::Aref, Ty::P64, arr, k2, 0).unwrap();
    let v = b
        .emit_flags(TOp::Aload, Ty::Num, F_GUARD, aref, NOREF, guard_aux(s0, 1))
        .unwrap();
    let one = b.ir.knum(1.0);
    let w = b.emit(TOp::Fadd, Ty::Num, v, one, 0).unwrap();
    b.emit(TOp::Astore, Ty::Nil, aref, w, 0).unwrap();
    let cl = b.ir.konst(KVal::Ptr(1, PtrKind::LClosure));
    let k0 = b.ir.kint(0);
    let u = b.emit(TOp::Uref, Ty::P64, cl, k0, 0).unwrap();
    b.emit(TOp::Ustore, Ty::Nil, u, w, 0).unwrap();
    let ul = b.emit(TOp::Uload, Ty::Num, u, NOREF, 0).unwrap();
    b.emit(TOp::Budget, Ty::I64, 3, NOREF, guard_aux(s0, 0))
        .unwrap();
    b.emit(TOp::Poll, Ty::Nil, NOREF, NOREF, guard_aux(s0, 0))
        .unwrap();
    let k1 = b.ir.kint(1);
    let u1 = b.emit(TOp::Uref, Ty::P64, cl, k1, 0).unwrap();
    b.emit(TOp::Ustore, Ty::Nil, u1, ul, 0).unwrap();
    let ir = b.finish();
    let mk = || {
        let mut m = Model::new();
        m.tables.push(rn_jit::tr::interp::MTable {
            array: vec![Val::num(0.0), Val::num(0.0), Val::num(4.5)],
            metatable: 0,
            flags: 0,
            lsizenode: 0,
            lenhint: 0,
            nodes: Default::default(),
        });
        m.closures = vec![vec![Val::NIL, Val::NIL]];
        m.budget = 10;
        m
    };
    let mut m = mk();
    assert_eq!(run(&ir, &mut m), Ok(Outcome::Done { iters: 0 }));
    assert_eq!(m.tables[0].array[2], Val::num(5.5));
    assert_eq!(m.closures[0], vec![Val::num(5.5), Val::num(5.5)]);
    assert_eq!(m.budget, 7);
    let mut m = mk();
    m.budget = 2;
    assert!(matches!(
        run(&ir, &mut m),
        Ok(Outcome::Exit { snap: 0, .. })
    ));
    assert_eq!(m.budget, 2, "a failed budget check charges nothing");
    let mut m = mk();
    m.poll_exit = Some(1);
    assert!(matches!(
        run(&ir, &mut m),
        Ok(Outcome::Exit { snap: 0, .. })
    ));
    let mut m = mk();
    m.tables[0].array[2] = Val::int(1);
    assert!(
        matches!(run(&ir, &mut m), Ok(Outcome::Exit { snap: 0, .. })),
        "element typecheck"
    );
    let mut m = mk();
    m.tables[0].array.truncate(2);
    assert!(
        matches!(run(&ir, &mut m), Ok(Outcome::Exit { snap: 0, .. })),
        "bounds guard"
    );
}

/// The loop of the P0 slice shape: `s = s + cnt; cnt = cnt - 1` until `cnt == 0` (a guard in the body).
#[test]
fn loop_with_phis() {
    let mut b = Builder::new(false, false);
    let s0 = b.ir.snapshot(Snap::default(), &[]);
    let x = b
        .emit_flags(TOp::Sload, Ty::Int, F_GUARD, 0, NOREF, guard_aux(s0, 1))
        .unwrap();
    let c = b
        .emit_flags(TOp::Sload, Ty::Int, F_GUARD, 1, NOREF, guard_aux(s0, 1))
        .unwrap();
    let (k0, k1) = (b.ir.kint(0), b.ir.kint(1));
    let s1 = b.emit(TOp::Add, Ty::Int, x, c, 0).unwrap();
    let c1 = b.emit(TOp::Sub, Ty::Int, c, k1, 0).unwrap();
    let snap = b.ir.snapshot(
        Snap::default(),
        &[
            SnapEntry {
                slot: 0,
                r: s1,
                flags: 0,
            },
            SnapEntry {
                slot: 1,
                r: c1,
                flags: 0,
            },
        ],
    );
    b.emit(TOp::Ne, Ty::Int, c1, k0, guard_aux(snap, 0))
        .unwrap();
    let l = b.loop_marker().unwrap();
    let ps = b.phi(Ty::Int, s1).unwrap();
    let pc = b.phi(Ty::Int, c1).unwrap();
    let s2 = b.emit(TOp::Add, Ty::Int, ps, pc, 0).unwrap();
    let c2 = b.emit(TOp::Sub, Ty::Int, pc, k1, 0).unwrap();
    let snap2 = b.ir.snapshot(
        Snap::default(),
        &[
            SnapEntry {
                slot: 0,
                r: s2,
                flags: 0,
            },
            SnapEntry {
                slot: 1,
                r: c2,
                flags: 0,
            },
        ],
    );
    b.emit(TOp::Ne, Ty::Int, c2, k0, guard_aux(snap2, 0))
        .unwrap();
    b.set_phi(ps, s2);
    b.set_phi(pc, c2);
    let ir = b.finish();
    assert_eq!(ir.loop_at, Some(l));
    for n in [2i64, 5, 9] {
        let mut m = Model::new();
        m.slots = vec![Val::int(100), Val::int(n)];
        m.max_iters = 20;
        let mut want = 100;
        for c in (1..=n).rev() {
            want += c;
        }
        let out = run(&ir, &mut m).unwrap();
        // exits at the guard of the body (snap2) once the counter reaches 0; with n == 1 in the pre-roll's guard
        assert_eq!(
            out,
            Outcome::Exit {
                snap: 2,
                slots: vec![(0, 0, Val::int(want)), (1, 0, Val::int(0))]
            },
            "n = {n}"
        );
    }
    // the iteration cap ends the run without an exit
    let mut m = Model::new();
    m.slots = vec![Val::int(0), Val::int(50)];
    m.max_iters = 3;
    assert_eq!(run(&ir, &mut m), Ok(Outcome::Done { iters: 3 }));
}

#[test]
fn invalid_ir_is_reported() {
    let mut b = Builder::new(false, false);
    let x = b.emit(TOp::Sload, Ty::Int, 5, NOREF, 0).unwrap();
    let _ = x;
    let ir = b.finish();
    let mut m = Model::new();
    assert!(run(&ir, &mut m).is_err(), "slot out of range");
}

// ---- F3: the class-A ops in the model ---------------------------------------------------------------------------------

/// `TNEW` / `NEWREF` / `TSET` / `TLEN` in the reference interpreter: the shapes, the "did nothing" results and the guards
/// that turn them into exits.
#[test]
fn class_a_ops_in_the_model() {
    let build = |dup_key: bool| {
        let mut b = Builder::new(false, false);
        let s0 = b.ir.snapshot(Snap::default(), &[]);
        let z = b.ir.kint(0);
        let t = b.emit_flags(TOp::Tnew, Ty::Tab, 0, 2, NOREF, 3).unwrap();
        b.emit_flags(TOp::Ne, Ty::I64, F_GUARD, t, z, guard_aux(s0, 0)).unwrap();
        let k = b.ir.konst(KVal::Ptr(0x1000, PtrKind::Raw));
        let n = b.emit_flags(TOp::Newref, Ty::P64, 0, t, k, 0).unwrap();
        b.emit_flags(TOp::Ne, Ty::I64, F_GUARD, n, z, guard_aux(s0, 0)).unwrap();
        let v = b.ir.kint(5);
        b.emit_flags(TOp::Hstore, Ty::I64, 0, n, v, 0).unwrap();
        if dup_key {
            // the key is there now: the helper does nothing and the guard exits
            let n2 = b.emit_flags(TOp::Newref, Ty::P64, 0, t, k, 0).unwrap();
            b.emit_flags(TOp::Ne, Ty::I64, F_GUARD, n2, z, guard_aux(s0, 0)).unwrap();
        }
        let key = b.ir.kint(9);
        let r = b.emit_flags(TOp::Tset, Ty::I64, 0, t, key, u32::from(v)).unwrap();
        b.emit_flags(TOp::Ne, Ty::I64, F_GUARD, r, z, guard_aux(s0, 0)).unwrap();
        b.emit_flags(TOp::Tlen, Ty::Int, 0, t, NOREF, 0).unwrap();
        b.finish()
    };
    let ir = build(false);
    let mut m = Model::new();
    m.len_result = Some(0);
    assert_eq!(run(&ir, &mut m).unwrap(), Outcome::Done { iters: 0 });
    assert_eq!(m.tables.len(), 1);
    assert_eq!(m.tables[0].array.len(), 3);
    assert_eq!(m.tables[0].lsizenode, 1, "B = 2: two nodes");
    assert_eq!(m.tables[0].nodes.len(), 1);
    assert_eq!(m.tables[0].nodes[&0].key, 0x1000);
    assert_eq!(m.tables[0].nodes[&0].val, Val::int(5));
    // a duplicate key
    let mut m = Model::new();
    assert!(matches!(run(&build(true), &mut m).unwrap(), Outcome::Exit { .. }));
    // allocation that "does nothing"
    for failing in [true, false] {
        let mut m = Model::new();
        m.alloc_fails = failing;
        let out = run(&ir, &mut m).unwrap();
        assert_eq!(matches!(out, Outcome::Exit { .. }), failing, "{out:?}");
        assert_eq!(m.tables.len(), usize::from(!failing));
    }
    // the dump names the third operand of TSET
    let txt = dump(&ir);
    assert!(txt.contains("TSET") && txt.contains("TNEW") && txt.contains("NEWREF") && txt.contains("TLEN"), "{txt}");
}

/// BUNDLE-2: constants are interned by value also beyond the size where `konst` switches from the linear scan to the hash index; the refs are
/// stable (first occurrence), NaN payloads and `-0.0` stay distinct, and a clone of the table keeps interning consistently.
#[test]
fn b2_konst_index_interns_like_the_scan() {
    let mut t = TraceIr::default();
    let mut first = Vec::new();
    for i in 0..200i64 {
        first.push((i, t.konst(KVal::Int(i * 7 - 50))));
    }
    assert_eq!(t.k.len(), 200);
    // the same values again: the same refs, no growth
    for &(i, r) in &first {
        assert_eq!(t.konst(KVal::Int(i * 7 - 50)), r);
    }
    assert_eq!(t.k.len(), 200);
    // other kinds with the same payload bits are other constants
    let f = t.konst(KVal::Num(7));
    assert_ne!(f, first[1].1);
    assert_eq!(t.konst(KVal::Num(7)), f);
    let (z, nz) = (t.konst(KVal::Num(0)), t.konst(KVal::Num(1u64 << 63)));
    assert_ne!(z, nz);
    let p = t.konst(KVal::Ptr(7, PtrKind::Raw));
    assert_ne!(p, f);
    assert_eq!(t.konst(KVal::Ptr(7, PtrKind::Raw)), p);
    assert_ne!(t.konst(KVal::Ptr(7, PtrKind::Proto)), p);
    // a clone interns consistently with the original (the index is part of the clone)
    let mut u = t.clone();
    assert_eq!(u.konst(KVal::Int(10 * 7 - 50)), first[10].1);
    let n = u.k.len();
    let fresh = u.konst(KVal::Int(1 << 40));
    assert_eq!(usize::from(fresh), n + 1);
    assert_eq!(u.konst(KVal::Int(1 << 40)), fresh);
    // a table whose constants were appended behind the index's back is re-indexed
    let mut w = TraceIr::default();
    for i in 0..40i64 {
        w.konst(KVal::Int(i));
    }
    w.k.push(KVal::Int(1000));
    assert_eq!(usize::from(w.konst(KVal::Int(1000))), 41);
    w.k.truncate(10);
    assert_eq!(usize::from(w.konst(KVal::Int(5))), 6);
    assert_eq!(usize::from(w.konst(KVal::Int(1000))), 11);
}

/// BUNDLE-C: `TraceIr::reset` (the recorder's scratch IR for the one-instruction evaluation) leaves exactly what `TraceIr::default()` is, also
/// after the constant index was built, and `run_buf` with a reused buffer gives the same outcome as `run`.
#[test]
fn bc_reset_equals_default_and_run_buf_matches_run() {
    let mut t = TraceIr::default();
    for i in 0..60i64 {
        t.konst(KVal::Int(i));
    }
    let s0 = t.snapshot(Snap::default(), &[]);
    t.emit(Ins { op: TOp::Sload, t: Ty::Int, flags: F_GUARD, a: 0, b: NOREF, prev: NOREF, aux: guard_aux(s0, 1) });
    t.deps.push(ProtoKey(5));
    t.counts.push(3);
    t.extent = 9;
    t.depth = 2;
    t.fn_root = true;
    t.reset();
    assert_eq!(format!("{t:?}"), format!("{:?}", TraceIr::default()));
    // the index of a reset IR interns from scratch
    assert_eq!(t.konst(KVal::Int(3)), 1);
    assert_eq!(t.konst(KVal::Int(4)), 2);
    // run vs run_buf on a tiny program, the buffer reused across runs of different length
    let mut buf = vec![7u64; 100];
    for n in [3usize, 1, 5] {
        let mut ir = TraceIr::default();
        let s = ir.snapshot(Snap::default(), &[]);
        let mut last = ir.emit(Ins { op: TOp::Sload, t: Ty::Int, flags: F_GUARD, a: 0, b: NOREF, prev: NOREF, aux: guard_aux(s, 1) });
        for k in 1..n {
            let c = ir.kint(k as i64);
            last = ir.emit(Ins { op: TOp::Add, t: Ty::Int, flags: 0, a: last, b: c, prev: NOREF, aux: 0 });
        }
        let _ = last;
        let mut m1 = Model::new();
        m1.slots = vec![Val { t: Ty::Int, bits: 40 }];
        let mut m2 = Model::new();
        m2.slots = m1.slots.clone();
        let a = run(&ir, &mut m1);
        let b = rn_jit::tr::interp::run_buf(&ir, &mut m2, &mut buf);
        assert_eq!(format!("{a:?}"), format!("{b:?}"));
    }
}

// ---- N2b: RETF in the builder and in the reference model ---------------------------------------------------------

/// `RETF` is never CSE'd, and it cuts the `UVREL` chain: `UVREL` is relative to the root base, which `RETF` moves, so the same operands
/// before and after it are different values. Detector of `retf_uvrel_cse`.
#[test]
fn retf_cuts_the_uvrel_cse_chain() {
    let mut b = Builder::new(true, true);
    let s0 = b.ir.snapshot(Snap::default(), &[]);
    let cl = b.ir.konst(KVal::Ptr(1, PtrKind::LClosure));
    let z = b.ir.kint(0);
    let u = b.emit(TOp::Uref, Ty::P64, cl, z, 0).unwrap();
    let r1 = b.emit(TOp::Uvrel, Ty::I64, u, NOREF, 0).unwrap();
    let r2 = b.emit(TOp::Uvrel, Ty::I64, u, NOREF, 0).unwrap();
    assert_eq!(r1, r2, "two UVRELs of one upvalue before the RETF merge");
    let site = b.ir.konst(KVal::Ptr(0x1000, PtrKind::Raw));
    let f1 = b.emit_retf(site, 2, guard_aux(s0, 2)).unwrap();
    let f2 = b.emit_retf(site, 2, guard_aux(s0, 2)).unwrap();
    assert_ne!(f1, f2, "RETF is never merged");
    let r3 = b.emit(TOp::Uvrel, Ty::I64, u, NOREF, 0).unwrap();
    assert_ne!(r3, r1, "UVREL after the RETF is a new value (retf_uvrel_cse)");
    let r4 = b.emit(TOp::Uvrel, Ty::I64, u, NOREF, 0).unwrap();
    assert_eq!(r3, r4, "and merges with later ones");
    assert!(b.ir.at(f1).flags & F_GUARD != 0);
    assert!(dump(&b.ir).contains("RETF"));
}

/// The model: a failing `RETF` exits through its snapshot with nothing changed; a passing one prepends the caller's registers to the
/// frame (slots are relative to the caller afterwards) and `UVREL` grows by `16 * (A + 1)` (stored relative to the entry base). Detector
/// of `retf_model_uvrel_unshifted`.
#[test]
fn retf_in_the_model_rebases_slots_and_uvrel() {
    let build = || {
        let mut b = Builder::new(false, false);
        let s0 = b.ir.snapshot(Snap::default(), &[]);
        let cl = b.ir.konst(KVal::Ptr(1, PtrKind::LClosure));
        let z = b.ir.kint(0);
        let u = b.emit(TOp::Uref, Ty::P64, cl, z, 0).unwrap();
        let before = b.emit(TOp::Uvrel, Ty::I64, u, NOREF, 0).unwrap();
        let site = b.ir.konst(KVal::Ptr(0x1000, PtrKind::Raw));
        b.emit_retf(site, 2, guard_aux(s0, 2)).unwrap();
        let s1 = b.ir.snapshot(Snap { seg: 1, ..Snap::default() }, &[]);
        let after = b.emit(TOp::Uvrel, Ty::I64, u, NOREF, 0).unwrap();
        // slot 0 after the pop is the caller's register 0
        let c0 = b.emit_flags(TOp::Sload, Ty::Int, F_GUARD, 0, NOREF, guard_aux(s1, 1)).unwrap();
        // slot 2 after the pop is the old slot 0 (the popped frame keeps its registers)
        let c2 = b.emit_flags(TOp::Sload, Ty::Int, F_GUARD, 2, NOREF, guard_aux(s1, 1)).unwrap();
        // store the three values into upvalue cells 0..2 to read them back
        for (i, r) in [(0u32, before), (1, after), (2, c0), (3, c2)] {
            let ix = b.ir.kint(i64::from(i));
            let ur = b.emit(TOp::Uref, Ty::P64, cl, ix, 0).unwrap();
            b.emit(TOp::Ustore, Ty::Nil, ur, r, 0).unwrap();
        }
        b.finish()
    };
    let ir = build();
    let mk = || {
        let mut m = Model::new();
        m.slots = vec![Val::int(5), Val::int(6)];
        m.closures = vec![vec![Val::NIL; 4]];
        m.uv_rel = vec![vec![64, 0, 0, 0]];
        m.retf_below = vec![Val::int(70), Val::int(71)];
        m
    };
    let mut m = mk();
    let out = run(&ir, &mut m).expect("valid IR");
    assert!(matches!(out, Outcome::Done { .. }), "{out:?}");
    assert_eq!(m.shift, 2);
    assert_eq!(m.slots, vec![Val::int(70), Val::int(71), Val::int(5), Val::int(6)]);
    let c = &m.closures[0];
    assert_eq!((c[0].bits, c[1].bits), (64, 64 + 32), "UVREL after = before + 16 * (A + 1) (retf_model_uvrel_unshifted)");
    assert_eq!((c[2].bits, c[3].bits), (70, 5), "slots are caller-relative after the pop");
    // the guards fail: exit through the snapshot of the RETURN, nothing popped
    let mut m = mk();
    m.retf_fail = true;
    match run(&ir, &mut m).expect("valid IR") {
        Outcome::Exit { snap, .. } => assert_eq!(snap, 0),
        o => panic!("{o:?}"),
    }
    assert_eq!((m.shift, m.slots.len()), (0, 2));
    // a wrong number of caller registers is invalid IR
    let mut m = mk();
    m.retf_below.pop();
    assert!(run(&ir, &mut m).is_err());
}
