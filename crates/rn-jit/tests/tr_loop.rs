//! J5 P7 (docs/JIT_TRACE.md 6.4, 6.6, 9.1 item 2, 9.7): loop peeling, invariant hoisting, alias analysis and
//! forwarding (`tr::opt_loop`, `tr::opt_mem`), property-tested against the reference IR interpreter.
//!
//! * `rolled_iterations_equal_peeled_traces`: random *rolled* iterations in the shape the recorder closes (`POLL`,
//!   `BUDGET`, entry checks, body with array / upvalue / header accesses, writeback) are optimised (peeled, rolled with
//!   forwarding, ablations) and run for several iterations on random models (tables that are one object, upvalues that
//!   are one cell, exhausted budgets, pending polls, failing guards at every position): the optimised trace must end in
//!   the same state as the original rolled loop, or leave through the same exit with the same restored frame, heap and
//!   budget;
//! * hand-written recorder shapes of the gate programs (`array_rec`, `int_rec2`, `upval_acc`, `licm_expr`, an aliasing
//!   pair): forwarding and hoisting happen where TR13 / TR24 say, and do not where the alias rules forbid it;
//! * `mutants_are_caught` (feature `tr_mutants`): `alias_may_as_no`, `alias_maytable_as_must`, `alias_upval_by_index`,
//!   `fwd_across_store`, `cse_across_store`, `peel_subst_stale`, `nonslot_phi_skip` each fail the test that guards them
//!   (child processes, `TR_LOOP_CHILD`).
#![cfg(feature = "trace")]

use rn_jit::tr::interp::gen::{Rng, INTS, NUMS};
use rn_jit::tr::interp::{run, Model, Outcome, Val};
use rn_jit::tr::ir::*;
use rn_jit::tr::opt_loop;
use rn_jit::tr::print::dump;
use rn_jit::tr::record::Recorded;
use rn_jit::tr::TraceOpts;
#[cfg(feature = "tr_mutants")]
use std::collections::HashMap;

// ---- a recorder in miniature: builds rolled iterations the way `tr::record` closes them ------------------------------------

const NSLOT: usize = 11;
/// Slot numbers: 0 = `i`, 1 = count, 2 = step, 3..=4 ints, 5..=6 floats, 7..=8 float tables, 9 = int table, 10 = int temp.
const SLOT_TY: [Ty; NSLOT] = [
    Ty::Int,
    Ty::Int,
    Ty::Int,
    Ty::Int,
    Ty::Int,
    Ty::Num,
    Ty::Num,
    Ty::Tab,
    Ty::Tab,
    Ty::Tab,
    Ty::Int,
];
const ASIZE: usize = 12;
const NUPV: usize = 4;
const N_CHARGE: u32 = 6;

struct Rb {
    b: Builder,
    cur: Vec<Ref>,
    sload: Vec<Ref>,
    written: Vec<bool>,
    snap: Option<u32>,
    /// A guard folded to constant false: the generated iteration is not a valid recording.
    dead: bool,
    next_pc: u32,
    entry_snap: u32,
}

impl Rb {
    fn new(fold: bool, cse: bool) -> Rb {
        let mut b = Builder::new(fold, cse);
        b.ir.head = (ProtoKey(1), 4);
        b.ir.extent = 16;
        let s0 = b.ir.snapshot(
            Snap {
                pc: 4,
                ..Snap::default()
            },
            &[],
        );
        let entry_snap = b.ir.snapshot(
            Snap {
                pc: 4,
                refund: N_CHARGE as i32,
                ..Snap::default()
            },
            &[],
        );
        b.emit(TOp::Poll, Ty::I64, NOREF, NOREF, guard_aux(s0, 0))
            .unwrap();
        b.emit_raw(Ins {
            op: TOp::Budget,
            t: Ty::I64,
            flags: F_GUARD,
            a: N_CHARGE as Ref,
            b: NOREF,
            prev: NOREF,
            aux: guard_aux(s0, 0),
        })
        .unwrap();
        let mut rb = Rb {
            b,
            cur: vec![NOREF; NSLOT],
            sload: vec![NOREF; NSLOT],
            written: vec![false; NSLOT],
            snap: None,
            dead: false,
            next_pc: 5,
            entry_snap,
        };
        for (s, &t) in SLOT_TY.iter().enumerate() {
            let r =
                rb.b.emit_raw(Ins {
                    op: TOp::Sload,
                    t,
                    flags: F_GUARD,
                    a: s as Ref,
                    b: NOREF,
                    prev: NOREF,
                    aux: guard_aux(entry_snap, 1),
                })
                .unwrap();
            rb.cur[s] = r;
            rb.sload[s] = r;
        }
        rb
    }

    /// Starts a new instruction (TR3a: its guards share one snapshot of the state before it).
    fn insn(&mut self) {
        self.snap = None;
    }

    fn snapshot(&mut self) -> u32 {
        if let Some(s) = self.snap {
            return s;
        }
        let entries: Vec<SnapEntry> = (0..NSLOT)
            .filter(|&s| self.written[s] && self.cur[s] != self.sload[s])
            .map(|s| SnapEntry {
                slot: s as u16,
                r: self.cur[s],
                flags: 0,
            })
            .collect();
        let n = self.b.ir.snapshot(
            Snap {
                pc: self.next_pc,
                refund: N_CHARGE as i32 - 1,
                land: Land::Mid,
                ..Snap::default()
            },
            &entries,
        );
        self.next_pc += 1;
        self.snap = Some(n);
        n
    }

    fn guard(&mut self, op: TOp, t: Ty, flags: u8, a: Ref, b: Ref, bits: u32) -> Ref {
        // like the recorder: the snapshot exists only if the guard is really emitted
        let tentative = self.snap.unwrap_or(self.b.ir.snaps.len() as u32);
        let before = self.b.ir.ins.len();
        let Ok(r) = self
            .b
            .emit_flags(op, t, F_GUARD | flags, a, b, guard_aux(tentative, bits))
        else {
            self.dead = true;
            return NOREF;
        };
        if self.b.ir.ins.len() > before
            && self.b.ir.ins[before].flags & F_GUARD != 0
            && self.snap.is_none()
        {
            // build the snapshot now (state before the instruction); its index must equal `tentative`
            let s = self.snapshot();
            assert_eq!(s, tentative);
        }
        r
    }

    fn ins(&mut self, op: TOp, t: Ty, a: Ref, b: Ref) -> Ref {
        self.b.emit(op, t, a, b, 0).expect("ins")
    }

    fn def(&mut self, slot: usize, r: Ref) {
        self.cur[slot] = r;
        self.written[slot] = true;
    }

    fn kint(&mut self, v: i64) -> Ref {
        self.b.ir.kint(v)
    }

    /// `t[idx]` guards of the recorder (metatable, bounds) and the array address.
    fn aref(&mut self, tab: Ref, idx: Ref) -> Ref {
        let mt = self
            .b
            .emit(TOp::Fload, Ty::P64, tab, Field::Metatable as Ref, 0)
            .unwrap();
        let z = self.kint(0);
        self.guard(TOp::Eq, Ty::I64, 0, mt, z, 0);
        let asz = self
            .b
            .emit(TOp::Fload, Ty::U32, tab, Field::Asize as Ref, 0)
            .unwrap();
        self.guard(TOp::Ult, Ty::I64, 0, idx, asz, 0);
        let arr = self
            .b
            .emit(TOp::Fload, Ty::P64, tab, Field::Array as Ref, 0)
            .unwrap();
        self.ins(TOp::Aref, Ty::P64, arr, idx)
    }

    fn aload(&mut self, tab: Ref, idx: Ref, t: Ty) -> Ref {
        self.insn();
        let a = self.aref(tab, idx);
        self.guard(TOp::Aload, t, 0, a, NOREF, 1)
    }

    fn astore(&mut self, tab: Ref, idx: Ref, v: Ref, barrier: bool) {
        self.insn();
        let a = self.aref(tab, idx);
        self.ins(TOp::Astore, Ty::Nil, a, v);
        if barrier {
            self.ins(TOp::Tbar, Ty::Nil, tab, v);
        }
    }

    /// F1: the recorder's guards for an existing key of the hash part (node-vector size, `HREFK` at node `key`).
    fn hnode(&mut self, tab: Ref, key: usize) -> Ref {
        let ls = self
            .b
            .emit(TOp::Fload, Ty::U32, tab, Field::Lsizenode as Ref, 0)
            .unwrap();
        let k2 = self.kint(2);
        self.guard(TOp::Eq, Ty::I64, 0, ls, k2, 0);
        let node = self
            .b
            .emit(TOp::Fload, Ty::P64, tab, Field::Node as Ref, 0)
            .unwrap();
        let kc = self.b.ir.konst(KVal::Ptr(HK[key], PtrKind::Raw));
        self.guard(TOp::Hrefk, Ty::P64, 0, node, kc, key as u32)
    }

    fn hload(&mut self, tab: Ref, key: usize, t: Ty) -> Ref {
        self.insn();
        let h = self.hnode(tab, key);
        self.guard(TOp::Hload, t, 0, h, NOREF, 1)
    }

    fn hstore(&mut self, tab: Ref, key: usize, v: Ref) {
        self.insn();
        let h = self.hnode(tab, key);
        self.guard(TOp::Hstore, Ty::Nil, 0, h, v, 1);
    }

    fn uref(&mut self, j: i64) -> Ref {
        let cl = self.b.ir.konst(KVal::Ptr(1, PtrKind::LClosure));
        let jk = self.kint(j);
        // P2 bundle 2 (`uvsep`): the recorder names the real `UpVal` in `aux`; here every index claims to be its own upvalue
        let id = if UVSEP.with(|u| u.get()) & 1 != 0 {
            j as u32 + 1
        } else {
            0
        };
        self.b.emit(TOp::Uref, Ty::P64, cl, jk, id).unwrap()
    }

    /// TR15: the range guard of the recorder, then the load.
    fn uload(&mut self, j: i64, t: Ty) -> Ref {
        self.insn();
        let u = self.uref(j);
        let rel = self.ins(TOp::Uvrel, Ty::I64, u, NOREF);
        let lim = self.kint(16 * 12);
        self.guard(TOp::Uge, Ty::I64, 0, rel, lim, 0);
        self.guard(TOp::Uload, t, 0, u, NOREF, 1)
    }

    fn ustore(&mut self, j: i64, v: Ref) {
        self.insn();
        let u = self.uref(j);
        let rel = self.ins(TOp::Uvrel, Ty::I64, u, NOREF);
        let lim = self.kint(16 * 12);
        self.guard(TOp::Uge, Ty::I64, 0, rel, lim, 0);
        self.ins(TOp::Ustore, Ty::Nil, u, v);
    }

    /// The numeric `for` control of the recorder (5.5 layout): count, step and control in slots 1, 2, 0.
    fn forloop(&mut self, kstep: i64) {
        self.insn();
        let (cnt, step, i) = (self.cur[1], self.cur[2], self.cur[0]);
        let ks = self.kint(kstep);
        let z = self.kint(0);
        self.guard(TOp::Ne, Ty::I64, 0, cnt, z, 0);
        self.guard(TOp::Eq, Ty::I64, 0, step, ks, 0);
        let one = self.kint(1);
        let c1 = self.ins(TOp::Sub, Ty::Int, cnt, one);
        let i1 = self.ins(TOp::Add, Ty::Int, i, ks);
        self.def(1, c1);
        self.def(0, i1);
    }

    fn finish(self) -> Recorded {
        let wb: Vec<SnapEntry> = (0..NSLOT)
            .filter(|&s| self.written[s] && self.cur[s] != self.sload[s])
            .map(|s| SnapEntry {
                slot: s as u16,
                r: self.cur[s],
                flags: 0,
            })
            .collect();
        let mut ir = self.b.finish();
        ir.counts = vec![N_CHARGE];
        let nsn = ir.snaps.len();
        Recorded {
            ir,
            writeback: wb,
            path: vec![4],
            cost: vec![N_CHARGE],
            snap_step: vec![0; nsn],
            path_proto: vec![ProtoKey(1)],
            path_depth: vec![0],
            room: 0,
            entry_snap: Some(self.entry_snap),
            n: N_CHARGE,
            rerec: 0,
            side: None,
            stitched: false,
        }
    }
}

/// F1: the keys of the models' hash nodes (node `j` of every table holds `HK[j]`; floats at 0 and 1, an integer at 2).
const HK: [u64; 3] = [0x2000, 0x2018, 0x2030];

// ---- the random iteration generator ----------------------------------------------------------------------------------------

struct Gen<'a> {
    rng: &'a mut Rng,
    rb: Rb,
}

impl Gen<'_> {
    fn int(&mut self) -> Ref {
        match self.rng.below(5) {
            0 => {
                let v = self.rng.pick(&[0i64, 1, 2, 3, 7, -1]);
                self.rb.kint(v)
            }
            1 => self.rb.cur[0],
            _ => {
                let s = self.rng.pick(&[3usize, 4, 10, 3, 4]);
                self.rb.cur[s]
            }
        }
    }

    fn num(&mut self) -> Ref {
        if self.rng.chance(1, 5) {
            self.rb.b.ir.konst(KVal::Num(
                self.rng
                    .pick(&[0x3FE0_0000_0000_0000, 0x4000_0000_0000_0000]),
            ))
        } else {
            let s = self.rng.pick(&[5usize, 6]);
            self.rb.cur[s]
        }
    }

    /// An index: `i + c` (the recurrence shapes), a loaded int masked into range, or a constant.
    fn index(&mut self) -> Ref {
        match self.rng.below(7) {
            0 => {
                let k = self.rng.below(ASIZE as u64) as i64;
                self.rb.kint(k)
            }
            1 => {
                let x = self.int();
                let m = self.rb.kint(7);
                self.rb.ins(TOp::Band, Ty::I64, x, m)
            }
            _ => {
                let c = self.rng.below(5) as i64 - 2;
                let base = self.rb.cur[0];
                let k = self.rb.kint(c);
                self.rb.ins(TOp::Add, Ty::I64, base, k)
            }
        }
    }

    fn ftab(&mut self) -> Ref {
        self.rb.cur[self.rng.pick(&[7usize, 8])]
    }

    fn step(&mut self) {
        match self.rng.below(21) {
            16 | 17 => {
                // F1: float field load
                let (t, k) = (self.ftab(), self.rng.below(2) as usize);
                let r = self.rb.hload(t, k, Ty::Num);
                let d = self.rng.pick(&[5usize, 6]);
                self.rb.def(d, r);
            }
            18 | 19 => {
                // F1: float field store (a forwarding source, a may-alias store for the other table)
                let (t, k, v) = (self.ftab(), self.rng.below(2) as usize, self.num());
                self.rb.hstore(t, k, v);
            }
            20 => {
                // F1: the integer field of the third table
                let t = self.rb.cur[9];
                if self.rng.chance(1, 2) {
                    let r = self.rb.hload(t, 2, Ty::Int);
                    let d = self.rng.pick(&[3usize, 4, 10]);
                    self.rb.def(d, r);
                } else {
                    let v = self.int();
                    self.rb.hstore(t, 2, v);
                }
            }
            0 | 1 => {
                let (a, b) = (self.int(), self.int());
                let op = self
                    .rng
                    .pick(&[TOp::Add, TOp::Sub, TOp::Mul, TOp::Bxor, TOp::Band]);
                let r = self.rb.ins(op, Ty::Int, a, b);
                let d = self.rng.pick(&[3usize, 4, 10]);
                self.rb.def(d, r);
            }
            2 | 3 => {
                let (a, b) = (self.num(), self.num());
                let op = self.rng.pick(&[TOp::Fadd, TOp::Fmul, TOp::Fsub]);
                let r = self.rb.ins(op, Ty::Num, a, b);
                let d = self.rng.pick(&[5usize, 6]);
                self.rb.def(d, r);
            }
            4..=6 => {
                // float element load (recurrence shapes: indices `i + c`)
                let (t, i) = (self.ftab(), self.index());
                let r = self.rb.aload(t, i, Ty::Num);
                let d = self.rng.pick(&[5usize, 6]);
                self.rb.def(d, r);
            }
            7 | 8 => {
                let (t, i, v) = (self.ftab(), self.index(), self.num());
                let bar = self.rng.chance(1, 2);
                self.rb.astore(t, i, v, bar);
            }
            9 => {
                let (t, i) = (self.rb.cur[9], self.index());
                let r = self.rb.aload(t, i, Ty::Int);
                let d = self.rng.pick(&[3usize, 4, 10]);
                self.rb.def(d, r);
            }
            10 => {
                let (t, i, v) = (self.rb.cur[9], self.index(), self.int());
                self.rb.astore(t, i, v, false);
            }
            11 => {
                let j = self.rng.below(NUPV as u64) as i64;
                let (t, d) = if j < 2 {
                    (Ty::Int, 3usize)
                } else {
                    (Ty::Num, 5usize)
                };
                let r = self.rb.uload(j, t);
                self.rb.def(d, r);
            }
            12 => {
                let j = self.rng.below(NUPV as u64) as i64;
                let v = if j < 2 { self.int() } else { self.num() };
                self.rb.ustore(j, v);
            }
            13 => {
                // a data guard on two ints / floats
                self.rb.insn();
                if self.rng.chance(1, 2) {
                    let (a, b) = (self.int(), self.int());
                    let op = self.rng.pick(&[TOp::Lt, TOp::Ge, TOp::Ne]);
                    self.rb.guard(op, Ty::I64, 0, a, b, 0);
                } else {
                    let (a, b) = (self.num(), self.num());
                    let bits = self.rng.below(6) as u32 | 16;
                    self.rb.guard(TOp::Fcmp, Ty::I64, 0, a, b, bits);
                }
            }
            14 => {
                // swap the two float tables (the table refs become loop carried)
                self.rb.insn();
                let (a, b) = (self.rb.cur[7], self.rb.cur[8]);
                self.rb.def(7, b);
                self.rb.def(8, a);
            }
            _ => {
                let (a, b) = (self.int(), self.int());
                let r = self.rb.ins(TOp::Add, Ty::Int, a, b);
                self.rb.def(10, r);
            }
        }
    }
}

/// A random rolled iteration; `recurrence` forces the `t[i] = f(t[i - k])` shapes that make non-slot PHIs.
fn gen_rolled(seed: u64, deep: bool) -> Option<Recorded> {
    let mut rng = Rng::new(seed);
    let fold = !rng.chance(1, 12);
    let mut g = Gen {
        rng: &mut rng,
        rb: Rb::new(fold, true),
    };
    let k = 3 + g.rng.below(10);
    if deep {
        // a delay line `t[i + c] = t[i + c - 1] + t[i + c - 3]` (or through another float table): values live three
        // iterations in memory, so the PHI chains are deeper than 2
        let c = g.rng.below(2) as i64;
        let t = g.ftab();
        let t3 = if g.rng.chance(1, 4) { g.ftab() } else { t };
        let base = g.rb.cur[0];
        let (k1, k3, ks) = (g.rb.kint(c - 1), g.rb.kint(c - 3), g.rb.kint(c));
        let (i1, i3, is) = (
            g.rb.ins(TOp::Add, Ty::I64, base, k1),
            g.rb.ins(TOp::Add, Ty::I64, base, k3),
            g.rb.ins(TOp::Add, Ty::I64, base, ks),
        );
        let x = g.rb.aload(t, i1, Ty::Num);
        let y = g.rb.aload(t3, i3, Ty::Num);
        let z = g.rb.ins(TOp::Fadd, Ty::Num, x, y);
        let kc = g.rb.b.ir.konst(KVal::Num(0x3FE0_0000_0000_0000));
        let w = g.rb.ins(TOp::Fmul, Ty::Num, z, kc);
        g.rb.astore(t, is, w, true);
        for _ in 0..g.rng.below(3) {
            g.step();
        }
        g.rb.forloop(1);
        return (!g.rb.dead).then(|| g.rb.finish());
    }
    if g.rng.chance(1, 2) {
        // a recurrence: load at `i + c - d`, store at `i + c`, one value passes through memory
        let c = g.rng.below(3) as i64 - 1;
        let d = 1 + g.rng.below(2) as i64;
        let t = g.ftab();
        let base = g.rb.cur[0];
        let (kl, ks) = (g.rb.kint(c - d), g.rb.kint(c));
        let il = g.rb.ins(TOp::Add, Ty::I64, base, kl);
        let is = g.rb.ins(TOp::Add, Ty::I64, base, ks);
        let x = g.rb.aload(t, il, Ty::Num);
        let kc = g.rb.b.ir.konst(KVal::Num(0x3FE0_0000_0000_0000));
        let y = g.rb.ins(TOp::Fmul, Ty::Num, x, kc);
        let t2 = if g.rng.chance(1, 3) { g.ftab() } else { t };
        g.rb.astore(t2, is, y, true);
    }
    for _ in 0..k {
        g.step();
    }
    let ks = g.rng.pick(&[1i64, 1, 1, 2]);
    g.rb.forloop(ks);
    (!g.rb.dead).then(|| g.rb.finish())
}

// ---- models ------------------------------------------------------------------------------------------------------------------

fn ival(rng: &mut Rng) -> Val {
    // small integers, so that `i + c` stays an index and the occasional big one makes other guards fail
    if rng.chance(1, 10) {
        Val::int(rng.pick(INTS))
    } else {
        Val::int(rng.below(6) as i64)
    }
}

fn fval(rng: &mut Rng) -> Val {
    if rng.chance(1, 6) {
        Val {
            t: Ty::Num,
            bits: rng.pick(NUMS),
        }
    } else {
        Val::num((rng.below(9) as f64 - 3.0) * 0.5)
    }
}

/// A model for [`gen_rolled`] traces. `alias`: the two float tables are one object; `join`: upvalues 0 and 1 share a cell.
/// `sizes`: the array lengths of the three tables (G1: they differ, so a bounds check proven for one table must not cover
/// another); `long`: counts that run past `ITERS_DEEP` iterations.
fn model(rng: &mut Rng, step: i64, alias: bool, join: bool) -> Model {
    model_sized(rng, step, alias, join, [ASIZE; 3], false)
}

fn model_sized(
    rng: &mut Rng,
    step: i64,
    alias: bool,
    join: bool,
    sizes: [usize; 3],
    long: bool,
) -> Model {
    let mut m = Model::new();
    let cnt = if long {
        8 + rng.below(8) as i64
    } else {
        1 + rng.below(9) as i64
    };
    m.slots = vec![
        Val::int(2 + rng.below(4) as i64),
        Val::int(cnt),
        Val::int(if rng.chance(1, 10) { 3 } else { step }),
        ival(rng),
        ival(rng),
        fval(rng),
        fval(rng),
        Val {
            t: Ty::Tab,
            bits: 1,
        },
        Val {
            t: Ty::Tab,
            bits: if alias { 1 } else { 2 },
        },
        Val {
            t: Ty::Tab,
            bits: 3,
        },
        ival(rng),
    ];
    let mk = |rng: &mut Rng, f: bool, n: usize| {
        (0..n)
            .map(|_| {
                if rng.chance(1, 60) {
                    Val::NIL
                } else if f {
                    fval(rng)
                } else {
                    ival(rng)
                }
            })
            .collect::<Vec<_>>()
    };
    for (k, f) in [true, true, false].into_iter().enumerate() {
        let array = mk(rng, f, sizes[k]);
        let mut nodes = std::collections::BTreeMap::new();
        for (j, &key) in HK.iter().enumerate() {
            let val = if rng.chance(1, 40) {
                Val::NIL // a removed key: the typecheck and the old-value check of a store exit
            } else if j < 2 {
                fval(rng)
            } else {
                ival(rng)
            };
            let key = if rng.chance(1, 70) { 0 } else { key }; // another layout: HREFK exits
            nodes.insert(
                j as u32,
                rn_jit::tr::interp::MNode {
                    key,
                    val,
                    next: 0,
                    int: false,
                },
            );
        }
        m.tables.push(rn_jit::tr::interp::MTable {
            array,
            metatable: 0,
            flags: 0,
            lsizenode: 2,
            lenhint: 0,
            nodes,
        });
    }
    m.closures = vec![vec![ival(rng), ival(rng), fval(rng), fval(rng)]];
    if join {
        m.uv_cell = vec![vec![1, 1, 0, 0]];
        m.cells = vec![ival(rng)];
    }
    m.budget = if rng.chance(1, 3) {
        rng.below(5 * u64::from(N_CHARGE) + 4) as i64
    } else {
        1000
    };
    m
}

// ---- running and comparing -------------------------------------------------------------------------------------------------

const ITERS: u32 = 5;
/// The deep variant: PHI chains longer than 2 and values that travel three iterations through memory (G4).
const ITERS_DEEP: u32 = 10;

#[derive(Debug, PartialEq)]
enum End {
    Done,
    Exit {
        pc: u32,
        frame: u8,
        refund: i32,
        land: Land,
        nframes: u8,
        slots: Vec<Val>,
    },
}

#[derive(Debug, PartialEq)]
struct Result_ {
    end: End,
    heap: Model,
}

fn heap_only(m: &Model) -> Model {
    let mut h = m.clone();
    h.slots = Vec::new();
    h.max_iters = 0;
    h.poll_exit = None;
    h
}

fn overlay(base: &[Val], slots: &[(u16, u8, Val)]) -> Vec<Val> {
    let mut v = base.to_vec();
    for &(s, _, x) in slots {
        v[usize::from(s)] = x;
    }
    v
}

fn exit_end(ir: &TraceIr, snap: u32, slots: &[(u16, u8, Val)], base: &[Val]) -> End {
    let s = ir.snaps[snap as usize];
    End::Exit {
        pc: s.pc,
        frame: s.frame,
        refund: s.refund,
        land: s.land,
        nframes: s.nframes,
        slots: overlay(base, slots),
    }
}

/// `ir` plus a probe: a snapshot of `entries` and an always-failing guard (the iteration's writeback comes out as the
/// resolved slots).
fn with_probe(ir: &TraceIr, entries: &[SnapEntry]) -> (TraceIr, u32) {
    let mut ir = ir.clone();
    let s = ir.snapshot(Snap::default(), entries);
    let (k0, k1) = (ir.kint(0), ir.kint(1));
    ir.ins.push(Ins {
        op: TOp::Eq,
        t: Ty::I64,
        flags: F_GUARD,
        a: k0,
        b: k1,
        prev: NOREF,
        aux: guard_aux(s, 0),
    });
    (ir, s)
}

/// The rolled loop: every iteration runs the IR from the top and its writeback becomes the stack (7.8). `poll_at`: the
/// iteration whose `POLL` fires.
fn run_rolled(
    ir: &TraceIr,
    wb: &[SnapEntry],
    m0: &Model,
    poll_at: Option<u32>,
    iters: u32,
) -> Result_ {
    let (probe, ps) = with_probe(ir, wb);
    let mut m = m0.clone();
    m.max_iters = 0;
    for it in 1..=iters {
        m.poll_exit = (poll_at == Some(it)).then_some(1);
        match run(&probe, &mut m).expect("valid IR") {
            Outcome::Exit { snap, slots } if snap == ps => {
                for (s, _, v) in slots {
                    m.slots[usize::from(s)] = v;
                }
            }
            Outcome::Exit { snap, slots } => {
                let end = exit_end(ir, snap, &slots, &m.slots);
                return Result_ {
                    end,
                    heap: heap_only(&m),
                };
            }
            Outcome::Done { .. } => unreachable!("the probe always fails"),
        }
    }
    Result_ {
        end: End::Done,
        heap: heap_only(&m),
    }
}

/// The peeled trace: the pre-roll and the first body run straight through, then `iters - 2` back-edges (`iters`
/// iterations in all). The stack is never written; exits restore from snapshots.
fn run_peeled(ir: &TraceIr, m0: &Model, poll_at: Option<u32>, iters: u32) -> Result_ {
    let mut m = m0.clone();
    m.max_iters = iters - 2;
    m.poll_exit = poll_at;
    let base = m.slots.clone();
    match run(ir, &mut m).expect("valid IR") {
        Outcome::Done { iters: n } => {
            assert_eq!(n, iters - 2);
            Result_ {
                end: End::Done,
                heap: heap_only(&m),
            }
        }
        Outcome::Exit { snap, slots } => {
            let end = exit_end(ir, snap, &slots, &base);
            Result_ {
                end,
                heap: heap_only(&m),
            }
        }
    }
}

thread_local! {
    /// P2 bundle 2: `TraceOpts::uvsep` of [`opts`] and whether [`Rb::uref`] names upvalue identities (bit 0) on this test thread.
    static UVSEP: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

fn opts(loop_opt: bool, fwd: bool, cse: bool) -> TraceOpts {
    TraceOpts {
        loop_opt,
        fwd,
        cse,
        uvsep: UVSEP.with(|u| u.get()),
        ..TraceOpts::default()
    }
}

#[derive(Default)]
struct Counts {
    cases: u32,
    peeled: u32,
    gave_up: u32,
    done: u32,
    exits: u32,
    phis: usize,
    forwarded: u32,
    /// P2 bundle 2: runs on which an upvalue-separation guard exited where the recorded loop went on (cells that were shared at run time).
    guarded: u32,
}

fn compare_case(seed: u64, st: &mut Counts, configs: &[(bool, bool, bool)], deep: bool) {
    let Some(rec0) = gen_rolled(seed, deep) else {
        return;
    };
    let mut rng = Rng::new(seed ^ 0xABCD);
    let kstep = {
        // the step the iteration was recorded with
        let i = rec0
            .ir
            .ins
            .iter()
            .rev()
            .find(|i| i.op == TOp::Eq && rec0.ir.as_int(i.b).is_some())
            .map(|i| rec0.ir.as_int(i.b).unwrap());
        i.unwrap_or(1)
    };
    for &(l, f, c) in configs {
        st.cases += 1;
        let mut rec = rec0.clone();
        opt_loop::run(&mut rec, &opts(l, f, c)).unwrap();
        opt_loop::check(&rec.ir).unwrap_or_else(|e| {
            panic!("seed {seed} loop={l} fwd={f}: {e}\n{}", dump(&rec.ir));
        });
        let peeled = rec.ir.loop_at.is_some();
        if l && peeled {
            st.peeled += 1;
            st.phis += opt_loop::shape(&rec.ir).unwrap().1;
        }
        if l && !peeled {
            st.gave_up += 1;
        }
        for mi in 0..8u32 {
            let alias = mi & 1 == 1;
            let join = mi & 2 != 0;
            // models 4..8: tables of different lengths (G1); the deep run: more iterations, longer tables (G4)
            let ragged = mi >= 4;
            let sizes = if deep {
                [32, 20, 26]
            } else if ragged {
                let mut v = [12usize, 8, 5];
                for k in (1..3).rev() {
                    v.swap(k, rng.below(k as u64 + 1) as usize);
                }
                v
            } else {
                [ASIZE; 3]
            };
            let m = model_sized(&mut rng, kstep, alias, join, sizes, deep);
            let iters = if deep { ITERS_DEEP } else { ITERS };
            let poll_at = (mi % 4 == 3).then(|| 1 + rng.below(u64::from(iters)) as u32);
            let base = run_rolled(&rec0.ir, &rec0.writeback, &m, poll_at, iters);
            let got = if peeled {
                run_peeled(&rec.ir, &m, poll_at, iters)
            } else {
                run_rolled(&rec.ir, &rec.writeback, &m, poll_at, iters)
            };
            if base != got && join && UVSEP.with(|u| u.get()) & 1 != 0 {
                // an upvalue-separation guard (`NE uref, uref`) fired: the optimised trace leaves at the snapshot of the forwarded load (the recording named two
                // upvalues as different `UpVal`s; this model makes them one cell)
                let pcs: Vec<u32> = rec
                    .ir
                    .ins
                    .iter()
                    .filter(|i| {
                        i.op == TOp::Ne
                            && i.flags & F_GUARD != 0
                            && TraceIr::is_ins(i.a)
                            && TraceIr::is_ins(i.b)
                            && rec.ir.at(i.a).op == TOp::Uref
                            && rec.ir.at(i.b).op == TOp::Uref
                    })
                    .map(|i| rec.ir.snaps[aux_snap(i.aux) as usize].pc)
                    .collect();
                if let End::Exit { pc, .. } = &got.end {
                    if pcs.contains(pc) {
                        st.guarded += 1;
                        continue;
                    }
                }
            }
            assert_eq!(
                base,
                got,
                "seed {seed} loop={l} fwd={f} cse={c} model {mi} (alias={alias} join={join} poll_at={poll_at:?})\n--- recorded\n{}\n--- optimised\n{}",
                dump(&rec0.ir),
                dump(&rec.ir)
            );
            match base.end {
                End::Done => st.done += 1,
                End::Exit { .. } => st.exits += 1,
            }
        }
        st.forwarded += rec
            .ir
            .ins
            .iter()
            .filter(|i| i.flags & F_PHI != 0 && i.op != TOp::Phi)
            .count() as u32;
    }
}

const CASES: u64 = if cfg!(miri) { 4 } else { 400 };

/// The main differential (see the module documentation). Every optimiser configuration (`-loop`, `-fwd`, `-cse`
/// ablations) is compared with the original rolled loop on random models, including tables that are one object and
/// upvalues that are one cell.
#[test]
fn rolled_iterations_equal_peeled_traces() {
    let mut st = Counts::default();
    let configs = [
        (true, true, true),
        (true, false, true),
        (false, true, true),
        (true, true, false),
    ];
    for seed in 1..=CASES {
        compare_case(seed, &mut st, &configs, false);
    }
    eprintln!(
        "cases {} peeled {} gave_up {} done {} exits {} phis {} marked {}",
        st.cases, st.peeled, st.gave_up, st.done, st.exits, st.phis, st.forwarded
    );
    if !cfg!(miri) {
        assert!(
            st.peeled > st.cases / 4,
            "the generator produces peelable loops"
        );
        assert!(st.gave_up * 10 < st.peeled, "giving up is rare");
        assert!(
            st.done > 100 && st.exits > 100,
            "ends and exits are reached"
        );
        assert!(st.phis > 100, "loops carry values");
    }
}

/// Aliasing-focused seeds (other base): longer iterations, more array traffic.
#[test]
#[cfg_attr(miri, ignore)]
fn rolled_iterations_equal_peeled_traces_more_seeds() {
    let mut st = Counts::default();
    for seed in 50_000..50_000 + CASES * 2 {
        compare_case(seed, &mut st, &[(true, true, true)], false);
    }
    assert!(st.peeled > 100);
}

/// Delay-line seeds run `ITERS_DEEP` iterations (PHI chains deeper than two, values that live three iterations in
/// memory), on tables of different lengths.
#[test]
#[cfg_attr(miri, ignore)]
fn deep_delay_lines_equal_peeled_traces() {
    let mut st = Counts::default();
    for seed in 90_000..90_000 + CASES {
        compare_case(
            seed,
            &mut st,
            &[(true, true, true), (true, false, true)],
            true,
        );
    }
    assert!(st.peeled > 100, "{}", st.peeled);
    assert!(st.done > 50 && st.exits > 50, "{} {}", st.done, st.exits);
}

// ---- hand-written shapes of the gate programs ---------------------------------------------------------------------------------

fn count(ir: &TraceIr, op: TOp, after_loop: bool) -> usize {
    let l = ir.loop_at.map_or(0, |r| usize::from(r - REF_BIAS));
    ir.ins
        .iter()
        .enumerate()
        .filter(|(n, i)| i.op == op && (*n > l) == after_loop)
        .count()
}

fn peel(rec: &Recorded) -> Recorded {
    let mut r = rec.clone();
    opt_loop::run(&mut r, &TraceOpts::default()).unwrap();
    opt_loop::check(&r.ir).unwrap();
    r
}

/// `for i = 2, n do t[i] = t[i - 1] * c + x end`: the load of the body is the previous iteration's store.
fn rec_array_rec(table_of_load: usize) -> Recorded {
    let mut rb = Rb::new(true, true);
    let t = rb.cur[7];
    let tl = rb.cur[table_of_load];
    let i = rb.cur[0];
    let (km1, km2) = (rb.kint(-1), rb.kint(-2));
    let il = rb.ins(TOp::Add, Ty::I64, i, km2);
    let is = rb.ins(TOp::Add, Ty::I64, i, km1);
    let x = rb.aload(tl, il, Ty::Num);
    let c = rb.cur[5];
    let y = rb.ins(TOp::Fmul, Ty::Num, x, c);
    let xx = rb.cur[6];
    let z = rb.ins(TOp::Fadd, Ty::Num, y, xx);
    rb.astore(t, is, z, true);
    rb.forloop(1);
    rb.finish()
}

#[test]
fn array_recurrence_forwards_across_the_backedge() {
    let rec = rec_array_rec(7);
    let p = peel(&rec);
    let ir = &p.ir;
    assert!(ir.loop_at.is_some(), "{}", dump(ir));
    // the load of the body is gone (forwarded from the pre-roll store through a PHI), the pre-roll load stays
    assert_eq!(count(ir, TOp::Aload, false), 1, "{}", dump(ir));
    assert_eq!(count(ir, TOp::Aload, true), 0, "{}", dump(ir));
    assert_eq!(count(ir, TOp::Astore, true), 1, "{}", dump(ir));
    // hoisted: the body has neither header loads nor the metatable guard
    assert_eq!(count(ir, TOp::Fload, true), 0, "{}", dump(ir));
    assert_eq!(count(ir, TOp::Eq, true), 0, "{}", dump(ir));
    // the stored float is a non-slot PHI
    let phis: Vec<&Ins> = ir.ins.iter().filter(|i| i.op == TOp::Phi).collect();
    assert!(
        phis.iter().any(|p| p.t == Ty::Num),
        "a float PHI carries the stored value\n{}",
        dump(ir)
    );
    // the same recurrence through the *other* float table ref (a[i] = b[i - 1] * c): may alias, so no forwarding
    let rec2 = rec_array_rec(8);
    let p2 = peel(&rec2);
    assert_eq!(count(&p2.ir, TOp::Aload, true), 1, "{}", dump(&p2.ir));
}

#[test]
fn two_forwarded_values_and_a_proven_distinct_store() {
    // t[i] = (t[i - 1] + t[i - 2]) & 0xFFFFFF over an int table
    let mut rb = Rb::new(true, true);
    let t = rb.cur[9];
    let i = rb.cur[0];
    let (k1, k2, k3) = (rb.kint(-1), rb.kint(-2), rb.kint(-3));
    let (i1, i2, is) = (
        rb.ins(TOp::Add, Ty::I64, i, k2),
        rb.ins(TOp::Add, Ty::I64, i, k3),
        rb.ins(TOp::Add, Ty::I64, i, k1),
    );
    let a = rb.aload(t, i1, Ty::Int);
    let b = rb.aload(t, i2, Ty::Int);
    let s = rb.ins(TOp::Add, Ty::Int, a, b);
    let m = rb.kint(0xFF_FFFF);
    let v = rb.ins(TOp::Band, Ty::Int, s, m);
    rb.astore(t, is, v, false);
    rb.forloop(1);
    let rec = rb.finish();
    let p = peel(&rec);
    let ir = &p.ir;
    assert_eq!(
        count(ir, TOp::Aload, true),
        0,
        "both loads forwarded\n{}",
        dump(ir)
    );
    assert_eq!(count(ir, TOp::Aload, false), 2, "{}", dump(ir));
    // the body computes the new value only (add, and) and stores it
    assert_eq!(count(ir, TOp::Astore, true), 1);
    assert_eq!(count(ir, TOp::Band, true), 1, "{}", dump(ir));
    assert!(
        ir.ins
            .iter()
            .filter(|i| i.op == TOp::Phi && i.t == Ty::Int)
            .count()
            >= 2,
        "two values go through memory\n{}",
        dump(ir)
    );
}

#[test]
fn upvalue_accumulator_stays_in_a_register() {
    // acc = acc + t[i], acc an upvalue
    let mut rb = Rb::new(true, true);
    let t = rb.cur[9];
    let i = rb.cur[0];
    let k1 = rb.kint(-1);
    let ix = rb.ins(TOp::Add, Ty::I64, i, k1);
    let e = rb.aload(t, ix, Ty::Int);
    let a = rb.uload(0, Ty::Int);
    let s = rb.ins(TOp::Add, Ty::Int, a, e);
    rb.ustore(0, s);
    rb.forloop(1);
    let rec = rb.finish();
    let p = peel(&rec);
    let ir = &p.ir;
    assert_eq!(count(ir, TOp::Uload, false), 1, "{}", dump(ir));
    assert_eq!(count(ir, TOp::Uload, true), 0, "{}", dump(ir));
    assert_eq!(count(ir, TOp::Ustore, true), 1, "{}", dump(ir));
    // the upvalue reference and its range guard are hoisted
    assert_eq!(count(ir, TOp::Uref, true), 0, "{}", dump(ir));
    assert_eq!(count(ir, TOp::Uvrel, true), 0, "{}", dump(ir));
    assert_eq!(count(ir, TOp::Uge, true), 0, "{}", dump(ir));
}

#[test]
fn distinct_upvalue_indices_are_not_disjoint() {
    // debug.upvaluejoin: uv0 = acc, uv1 = acc2; `acc = acc + 1; x = acc2` must reload acc2 after the store to acc
    let mut rb = Rb::new(true, true);
    let a = rb.uload(0, Ty::Int);
    let one = rb.kint(1);
    let s = rb.ins(TOp::Add, Ty::Int, a, one);
    rb.ustore(0, s);
    let b = rb.uload(1, Ty::Int);
    rb.def(3, b);
    rb.forloop(1);
    let rec = rb.finish();
    let p = peel(&rec);
    assert!(
        count(&p.ir, TOp::Uload, true) >= 1,
        "the load of upvalue 1 after a store to upvalue 0 stays\n{}",
        dump(&p.ir)
    );
}

/// P2 bundle 2 (`uvsep`): `a = uv0; uv1 = v; b = uv0`: recorded on two different `UpVal`s the second load is forwarded over the store of upvalue 1 under
/// a run-time `NE` guard (without identities, or with `uvsep=0`, it stays); when the two are one cell at run time the guard exits. Mutants `uvsep_no_guard`,
/// `uvsep_first_guard_only` (two stores: only the first got its guard).
#[test]
fn uvsep_forwards_over_distinct_upvalue_stores() {
    let build = |two_stores: bool| {
        let mut rb = Rb::new(true, true);
        let a = rb.uload(0, Ty::Int);
        let one = rb.kint(1);
        let s = rb.ins(TOp::Add, Ty::Int, a, one);
        rb.ustore(1, s);
        if two_stores {
            rb.ustore(2, s);
        }
        let b = rb.uload(0, Ty::Int);
        rb.def(3, b);
        rb.forloop(1);
        rb.finish()
    };
    for two in [false, true] {
        for (uv, ids, want_loads, want_ne) in [
            (1u32, 1u32, 1usize, 1usize),
            (0, 1, 2, 0),
            (1, 0, 2, 0),
            (1, 1, 1, 1),
        ] {
            UVSEP.with(|u| u.set(ids));
            let rec = build(two);
            let mut r = rec.clone();
            let mut o = opts(false, true, true);
            o.uvsep = uv;
            opt_loop::run(&mut r, &o).unwrap();
            opt_loop::check(&r.ir).unwrap();
            let loads = r
                .ir
                .ins
                .iter()
                .filter(|i| {
                    i.op == TOp::Uload && i.t == Ty::Int && r.ir.as_int(r.ir.at(i.a).b) == Some(0)
                })
                .count();
            let nes =
                r.ir.ins
                    .iter()
                    .filter(|i| {
                        i.op == TOp::Ne
                            && i.flags & F_GUARD != 0
                            && TraceIr::is_ins(i.a)
                            && r.ir.at(i.a).op == TOp::Uref
                    })
                    .count();
            assert_eq!(
                (loads, nes),
                (want_loads, want_ne * if two && want_ne > 0 { 2 } else { 1 }),
                "two={two} uvsep={uv} ids={ids}\n{}",
                dump(&r.ir)
            );
            // run on models where upvalue 0 and 1 (2) are, or are not, one cell: the optimised trace ends as the recorded one
            for join in [0u32, 1, 2] {
                let mut m = Model::new();
                m.slots = vec![
                    Val::int(2),
                    Val::int(6),
                    Val::int(1),
                    Val::int(0),
                    Val::int(0),
                    Val::num(0.5),
                    Val::num(1.5),
                    Val {
                        t: Ty::Tab,
                        bits: 1,
                    },
                    Val {
                        t: Ty::Tab,
                        bits: 1,
                    },
                    Val {
                        t: Ty::Tab,
                        bits: 1,
                    },
                    Val::int(0),
                ];
                m.closures = vec![vec![Val::int(10), Val::int(20), Val::int(30), Val::int(40)]];
                m.tables = vec![rn_jit::tr::interp::MTable {
                    array: vec![Val::int(1); ASIZE],
                    metatable: 0,
                    flags: 0,
                    lsizenode: 2,
                    lenhint: 0,
                    nodes: Default::default(),
                }];
                if join == 1 {
                    m.uv_cell = vec![vec![1, 1, 0, 0]];
                    m.cells = vec![Val::int(7)];
                } else if join == 2 {
                    m.uv_cell = vec![vec![1, 0, 1, 0]];
                    m.cells = vec![Val::int(7)];
                }
                let base = run_rolled(&rec.ir, &rec.writeback, &m, None, ITERS);
                let got = run_rolled(&r.ir, &r.writeback, &m, None, ITERS);
                // the load was forwarded over a store to a cell it shares at run time (`join`): the guard exits, at the load, instead of the wrong value
                let aliased = uv & 1 != 0 && ids == 1 && (join == 1 || (join == 2 && two));
                if aliased {
                    assert!(
                        matches!(got.end, End::Exit { .. }),
                        "two={two} uvsep={uv} join={join}: the guard must fire\n{}",
                        dump(&r.ir)
                    );
                    assert_ne!(base, got);
                } else {
                    assert_eq!(
                        base,
                        got,
                        "two={two} uvsep={uv} ids={ids} join={join}\n{}",
                        dump(&r.ir)
                    );
                }
            }
        }
    }
    UVSEP.with(|u| u.set(0));
}

/// P2 bundle 2 (a latent bug of the loop optimiser's clean-up, found with the removed `uvroll`, which let `TSET` / `CONCAT` / `LIB` pass the optimiser): the third operand of
/// `LIB` (see `Ins::third`; `SUB` is header-safe from `hdrlib` 2, the optimiser gives up at `TSET` and `CONCAT` since K1) keeps its
/// producer alive and is renumbered when dead instructions before it are dropped. Mutants `cleanup_third_dead`, `cleanup_third_stale`.
#[test]
fn cleanup_renumbers_third_operands() {
    use rn_jit::tr::ir::lib;
    for kind in 0..1 {
        let mut rb = Rb::new(true, false);
        let (a, b, t) = (rb.cur[3], rb.cur[4], rb.cur[7]);
        // two dead instructions in front of the producer of the third operand (so that the clean-up shifts its number)
        rb.ins(TOp::Add, Ty::Int, a, b);
        rb.ins(TOp::Add, Ty::Int, b, a);
        let j = rb.ins(TOp::Sub, Ty::Int, a, b);
        let (op, ty, aux) = match kind {
            0 => (TOp::Lib, Ty::ShrStr, lib::SUB | (j as u32) << 8),
            1 => (TOp::Tset, Ty::Nil, j as u32),
            _ => (TOp::Concat, Ty::ShrStr, j as u32),
        };
        rb.b.emit_raw(Ins {
            op,
            t: ty,
            flags: 0,
            a: t,
            b: a,
            prev: NOREF,
            aux,
        })
        .unwrap();
        rb.forloop(1);
        let rec = rb.finish();
        let mut r = rec.clone();
        let mut o = opts(false, true, true);
        o.uvsep = 1;
        o.hdrlib = 2;
        opt_loop::run(&mut r, &o).unwrap();
        opt_loop::check(&r.ir).unwrap();
        let ins =
            r.ir.ins
                .iter()
                .find(|i| i.op == op)
                .copied()
                .expect("the op survives");
        assert!(
            r.ir.ins.len() < rec.ir.ins.len(),
            "{op:?}: dead instructions were dropped\n{}",
            dump(&r.ir)
        );
        let third = ins.third().expect("third operand");
        assert!(
            TraceIr::is_ins(third) && r.ir.at(third).op == TOp::Sub,
            "{op:?}: the third operand names its producer\n{}",
            dump(&r.ir)
        );
    }
}

/// P2 bundle 2: the random rolled iterations (upvalue loads and stores over the four upvalues, models where upvalues 0 and 1 are one cell although the recording
/// named them as different `UpVal`s) optimised with `uvsep=1` end like the recorded loop. Mutants `uvsep_no_guard`, `uvsep_first_guard_only`.
#[test]
#[cfg_attr(miri, ignore)]
fn rolled_iterations_with_uvsep_equal_the_recorded_loop() {
    for bits in [1u32] {
        UVSEP.with(|u| u.set(bits));
        let mut st = Counts::default();
        let configs = [(false, true, true), (true, true, true)];
        for seed in 1..=CASES * 2 {
            compare_case(seed, &mut st, &configs, false);
        }
        assert!(
            st.cases > 100 && st.done > 100 && st.exits > 100,
            "uvsep={bits}: {} {} {}",
            st.cases,
            st.done,
            st.exits
        );
        assert!(
            st.guarded > 3,
            "uvsep={bits}: the separation guards fire on models with shared cells: {}",
            st.guarded
        );
    }
    UVSEP.with(|u| u.set(0));
}

#[test]
fn invariant_expression_is_hoisted() {
    // s = s + t[i] * (sqrt(a * a + b * b) / c): a a float local, b and c upvalues
    let mut rb = Rb::new(true, true);
    let a = rb.cur[5];
    let b = rb.uload(3, Ty::Num);
    let aa = rb.ins(TOp::Fmul, Ty::Num, a, a);
    let bb = rb.ins(TOp::Fmul, Ty::Num, b, b);
    let sum = rb.ins(TOp::Fadd, Ty::Num, aa, bb);
    let sq = rb.ins(TOp::Fsqrt, Ty::Num, sum, NOREF);
    let c = rb.uload(2, Ty::Num);
    let d = rb.ins(TOp::Fdiv, Ty::Num, sq, c);
    let t = rb.cur[7];
    let i = rb.cur[0];
    let k1 = rb.kint(-1);
    let ix = rb.ins(TOp::Add, Ty::I64, i, k1);
    let e = rb.aload(t, ix, Ty::Num);
    let m = rb.ins(TOp::Fmul, Ty::Num, e, d);
    let s0 = rb.cur[6];
    let s1 = rb.ins(TOp::Fadd, Ty::Num, s0, m);
    rb.def(6, s1);
    rb.forloop(1);
    let rec = rb.finish();
    let p = peel(&rec);
    let ir = &p.ir;
    for op in [TOp::Fsqrt, TOp::Fdiv, TOp::Uload] {
        assert_eq!(count(ir, op, true), 0, "{op:?} hoisted\n{}", dump(ir));
        assert!(count(ir, op, false) >= 1);
    }
    assert_eq!(
        count(ir, TOp::Aload, true),
        1,
        "the element varies\n{}",
        dump(ir)
    );
    assert_eq!(
        count(ir, TOp::Fmul, true),
        1,
        "only e * d stays\n{}",
        dump(ir)
    );
    assert!(ir.ins.iter().any(|i| i.flags & F_LOOPINV != 0));
}

#[test]
fn peeled_layout_has_the_loop_top_shape() {
    let rec = rec_array_rec(7);
    let p = peel(&rec);
    let ir = &p.ir;
    let l = usize::from(ir.loop_at.unwrap() - REF_BIAS);
    assert_eq!(ir.ins[l].op, TOp::Loop);
    let phis = opt_loop::shape(ir).unwrap().1;
    // TR27b: POLL, BUDGET first after the PHIs, on the S_H snapshot (pc = H, uncharged)
    assert_eq!(ir.ins[l + 1 + phis].op, TOp::Poll);
    assert_eq!(ir.ins[l + 2 + phis].op, TOp::Budget);
    let sh = aux_snap(ir.ins[l + 1 + phis].aux);
    assert_eq!(ir.snaps[sh as usize].pc, 4);
    assert_eq!(ir.snaps[sh as usize].refund, 0);
    // the pre-roll keeps the entry segment: POLL, BUDGET on snapshot 0, checked loads on the entry snapshot
    assert_eq!(ir.ins[0].op, TOp::Poll);
    assert_eq!(ir.ins[1].op, TOp::Budget);
    assert_eq!(aux_snap(ir.ins[0].aux), 0);
    let es = p.entry_snap.expect("entry snapshot");
    assert_eq!(ir.snaps[es as usize].map.1, 0);
    assert!(ir.ins[2..l]
        .iter()
        .filter(|i| i.op == TOp::Sload && i.flags & F_GUARD != 0)
        .all(|i| aux_snap(i.aux) == es));
    // no guard after the loop uses a snapshot of the pre-roll that has a map; counts per segment
    assert_eq!(ir.counts, vec![N_CHARGE, N_CHARGE]);
    // ablations
    let mut r = rec.clone();
    opt_loop::run(&mut r, &opts(false, false, true)).unwrap();
    assert!(
        r.ir.loop_at.is_none(),
        "-loop -fwd leaves the rolled trace alone"
    );
    assert_eq!(r.ir.ins.len(), rec.ir.ins.len());
    let mut r = rec.clone();
    opt_loop::run(&mut r, &opts(true, false, true)).unwrap();
    assert!(r.ir.loop_at.is_some());
    assert_eq!(count(&r.ir, TOp::Aload, true), 1, "-fwd keeps the load");
    let mut r = rec.clone();
    opt_loop::run(&mut r, &opts(false, true, true)).unwrap();
    assert!(r.ir.loop_at.is_none());
}

/// A table written through one ref and read through another ref in the next iteration must not be forwarded, whatever
/// the indices look like (they can be one table).
#[test]
fn may_alias_tables_do_not_forward() {
    let mut rb = Rb::new(true, true);
    let (ta, tb) = (rb.cur[7], rb.cur[8]);
    let i = rb.cur[0];
    let k1 = rb.kint(-1);
    let ix = rb.ins(TOp::Add, Ty::I64, i, k1);
    let v = rb.cur[5];
    rb.astore(ta, ix, v, false);
    let x = rb.aload(tb, ix, Ty::Num);
    rb.def(6, x);
    rb.forloop(1);
    let rec = rb.finish();
    let p = peel(&rec);
    assert_eq!(count(&p.ir, TOp::Aload, true), 1, "{}", dump(&p.ir));
    assert_eq!(count(&p.ir, TOp::Aload, false), 1);
    // the same table ref: forwarded
    let mut rb = Rb::new(true, true);
    let ta = rb.cur[7];
    let i = rb.cur[0];
    let k1 = rb.kint(-1);
    let ix = rb.ins(TOp::Add, Ty::I64, i, k1);
    let v = rb.cur[5];
    rb.astore(ta, ix, v, false);
    let x = rb.aload(ta, ix, Ty::Num);
    rb.def(6, x);
    rb.forloop(1);
    let p = peel(&rb.finish());
    assert_eq!(
        count(&p.ir, TOp::Aload, true) + count(&p.ir, TOp::Aload, false),
        0,
        "{}",
        dump(&p.ir)
    );
}

/// A store that may alias lies between two loads of one location: the second load is not the first (`cse_across_store`,
/// `alias_may_as_no`), and a later may-alias store blocks the forwarding of an older store (`fwd_across_store`).
#[test]
fn stores_between_accesses_block_load_cse_and_forwarding() {
    let mut rb = Rb::new(true, true);
    let (ta, tb) = (rb.cur[7], rb.cur[8]);
    let ix = rb.cur[0];
    let l1 = rb.aload(tb, ix, Ty::Num);
    let v = rb.cur[5];
    rb.astore(ta, ix, v, false);
    let l2 = rb.aload(tb, ix, Ty::Num);
    let s = rb.ins(TOp::Fadd, Ty::Num, l1, l2);
    rb.def(6, s);
    rb.forloop(1);
    let p = peel(&rb.finish());
    assert_eq!(count(&p.ir, TOp::Aload, false), 2, "{}", dump(&p.ir));
    // store ta[x] = v1; store tb[x] = v2; load ta[x]
    let mut rb = Rb::new(true, true);
    let (ta, tb) = (rb.cur[7], rb.cur[8]);
    let ix = rb.cur[0];
    let (v1, v2) = (rb.cur[5], rb.cur[6]);
    rb.astore(ta, ix, v1, false);
    rb.astore(tb, ix, v2, false);
    let l = rb.aload(ta, ix, Ty::Num);
    rb.def(5, l);
    rb.forloop(1);
    let p = peel(&rb.finish());
    assert_eq!(count(&p.ir, TOp::Aload, false), 1, "{}", dump(&p.ir));
}

#[test]
fn passes_are_a_noop_on_a_peeled_or_foreign_trace() {
    let rec = rec_array_rec(7);
    let p = peel(&rec);
    let mut again = p.clone();
    opt_loop::run(&mut again, &TraceOpts::default()).unwrap();
    assert_eq!(again.ir.ins, p.ir.ins);
    // a trace that does not start with POLL, BUDGET is rolled with forwarding at most
    let mut r = rec.clone();
    r.ir.ins.swap(0, 1);
    opt_loop::run(&mut r, &TraceOpts::default()).unwrap();
    assert!(r.ir.loop_at.is_none());
}

/// P7b H2: an op that can allocate, insert a key, store into the hash part, touch the array limit or build a string
/// (a finaliser may run) can change a table header: the `FLOAD` CSE and the bounds dedup are unsound across it, so
/// the optimiser leaves the recorded trace as it is.
#[test]
fn header_mutating_ops_stop_the_optimiser() {
    let build = |op: Option<TOp>| {
        let mut rb = Rb::new(true, true);
        let (t, i) = (rb.cur[7], rb.cur[0]);
        let x = rb.aload(t, i, Ty::Num);
        if let Some(op) = op {
            let ty = match op {
                TOp::Tnew => Ty::Tab,
                TOp::Snew | TOp::Concat => Ty::ShrStr,
                TOp::Newref => Ty::P64,
                _ => Ty::Nil,
            };
            rb.b.emit_raw(Ins {
                op,
                t: ty,
                flags: 0,
                a: t,
                b: NOREF,
                prev: NOREF,
                aux: 0,
            })
            .unwrap();
        }
        let y = rb.aload(t, i, Ty::Num);
        let z = rb.ins(TOp::Fadd, Ty::Num, x, y);
        rb.def(5, z);
        rb.forloop(1);
        rb.finish()
    };
    // control: the same trace without the op is peeled; its body has no header load and one bounds check (the index
    // advances)
    let ok = peel(&build(None));
    assert!(ok.ir.loop_at.is_some(), "{}", dump(&ok.ir));
    assert_eq!(count(&ok.ir, TOp::Fload, true), 0, "{}", dump(&ok.ir));
    assert_eq!(count(&ok.ir, TOp::Ult, true), 1, "{}", dump(&ok.ir));
    for op in [
        TOp::Newref,
        TOp::Tnew,
        TOp::Snew,
        TOp::Concat,
        TOp::Tlen,
        TOp::Tset,
    ] {
        let rec = build(Some(op));
        for (l, f) in [(true, true), (false, true), (true, false)] {
            let mut r = rec.clone();
            opt_loop::run(&mut r, &opts(l, f, true)).unwrap();
            assert!(r.ir.loop_at.is_none(), "{op:?} loop={l} fwd={f}: peeled");
            assert_eq!(r.ir.ins, rec.ir.ins, "{op:?}: the trace is unchanged");
            let note = opt_loop::last_note();
            assert!(note.contains("header-mutating"), "{op:?}: {note}");
        }
    }
}

/// BUNDLE-1 (4): with `hdrlib` the string `TLEN` and the read-only `LIB` kinds are header-safe (the loop is peeled, the header load and the
/// bounds check of the pre-roll cover the body); table `TLEN`, `SETMT`, `NEWREF` and the allocating kinds still stop the optimiser; the
/// third operand of a `LIB` is renumbered in the body copy. Mutants `hdrlib_never`, `hdrlib_table_tlen`, `hdrlib_setmt`, `hdrlib_newref`,
/// `hdrlib_third_stale`, `hdrlib_sub_always`.
#[test]
fn hdrlib_classifies_lib_ops() {
    use rn_jit::tr::ir::lib;
    let build = |op: TOp, aux: u32, ty: Ty| {
        let mut rb = Rb::new(true, true);
        let (t, i) = (rb.cur[7], rb.cur[0]);
        let x = rb.aload(t, i, Ty::Num);
        rb.b.emit_raw(Ins {
            op,
            t: ty,
            flags: 0,
            a: t,
            b: NOREF,
            prev: NOREF,
            aux,
        })
        .unwrap();
        let y = rb.aload(t, i, Ty::Num);
        let z = rb.ins(TOp::Fadd, Ty::Num, x, y);
        rb.def(5, z);
        rb.forloop(1);
        rb.finish()
    };
    let run = |rec: &Recorded, level: u32| {
        let mut r = rec.clone();
        let mut o = opts(true, true, true);
        o.hdrlib = level;
        opt_loop::run(&mut r, &o).unwrap();
        r
    };
    // peeled with the knob (the header load is not repeated in the body), rolled without it
    let cases_ok = [
        (TOp::Tlen, 1, Ty::Int),
        (TOp::Tlen, 2, Ty::Int),
        (TOp::Lib, lib::BYTE, Ty::Int),
        (TOp::Lib, lib::STREQ, Ty::Int),
        (TOp::Lib, lib::TYPENAME, Ty::ShrStr),
    ];
    for (op, aux, ty) in cases_ok {
        let rec = build(op, aux, ty);
        let on = run(&rec, 1);
        assert!(
            on.ir.loop_at.is_some(),
            "{op:?} {aux}: not peeled with hdrlib=1\n{}",
            dump(&on.ir)
        );
        assert_eq!(
            count(&on.ir, TOp::Fload, true),
            0,
            "{op:?} {aux}\n{}",
            dump(&on.ir)
        );
        let off = run(&rec, 0);
        assert!(
            off.ir.loop_at.is_none(),
            "{op:?} {aux}: peeled without hdrlib"
        );
    }
    // still barriers
    let cases_bad = [
        (TOp::Tlen, 0, Ty::Int),
        (TOp::Lib, lib::SETMT, Ty::Int),
        (TOp::Lib, lib::SUB, Ty::ShrStr),
        (TOp::Lib, lib::CHAR, Ty::ShrStr),
        (TOp::Lib, lib::TOSTR, Ty::ShrStr),
        (TOp::Newref, 0, Ty::P64),
        (TOp::Tnew, 0, Ty::Tab),
        (TOp::Snew, 0, Ty::ShrStr),
        (TOp::Concat, 0, Ty::ShrStr),
    ];
    for (op, aux, ty) in cases_bad {
        let rec = build(op, aux, ty);
        let on = run(&rec, 1);
        assert!(
            on.ir.loop_at.is_none(),
            "{op:?} {aux}: peeled with hdrlib=1"
        );
        assert_eq!(
            on.ir.ins, rec.ir.ins,
            "{op:?} {aux}: the trace is unchanged"
        );
    }
    // HDRSUB: the string makers are header-safe from their level on (`sub` 2, `char` 3, `tostring` 4), barriers below it
    for (aux, level) in [(lib::SUB, 2), (lib::CHAR, 3), (lib::TOSTR, 4)] {
        let rec = build(TOp::Lib, aux, Ty::ShrStr);
        for lv in 1..=4 {
            let on = run(&rec, lv);
            if lv >= level {
                assert!(
                    on.ir.loop_at.is_some(),
                    "lib {aux}: not peeled with hdrlib={lv}\n{}",
                    dump(&on.ir)
                );
                assert_eq!(
                    count(&on.ir, TOp::Fload, true),
                    0,
                    "lib {aux} hdrlib={lv}\n{}",
                    dump(&on.ir)
                );
            } else {
                assert!(
                    on.ir.loop_at.is_none(),
                    "lib {aux}: peeled with hdrlib={lv}"
                );
                assert_eq!(
                    on.ir.ins, rec.ir.ins,
                    "lib {aux} hdrlib={lv}: the trace is unchanged"
                );
            }
        }
    }
    // the third operand of `LIB random` follows the renumbering: operands `b` and third are the same value, in the body too
    let mut rb = Rb::new(true, true);
    let (t, i) = (rb.cur[7], rb.cur[0]);
    let x = rb.aload(t, i, Ty::Num);
    let r =
        rb.b.emit_raw(Ins {
            op: TOp::Lib,
            t: Ty::Int,
            flags: 0,
            a: i,
            b: i,
            prev: NOREF,
            aux: lib::RANDOM | u32::from(i) << 8,
        })
        .unwrap();
    let z = rb.ins(TOp::Fadd, Ty::Num, x, x);
    rb.def(5, z);
    rb.def(4, r);
    rb.forloop(1);
    let rec = rb.finish();
    let on = run(&rec, 1);
    assert!(on.ir.loop_at.is_some(), "{}", dump(&on.ir));
    let libs: Vec<_> = on.ir.ins.iter().filter(|n| n.op == TOp::Lib).collect();
    assert_eq!(libs.len(), 2, "{}", dump(&on.ir));
    for l in libs {
        assert_eq!(
            l.b,
            (l.aux >> 8) as Ref,
            "third operand differs from b\n{}",
            dump(&on.ir)
        );
    }
}

/// P7b G1: `t1[i]` and `t2[i]` have different bounds (the array sizes of two tables): the bounds check of one does not
/// cover the other, in the pre-roll or in the body.
#[test]
fn bounds_checks_of_different_tables_are_kept() {
    let mut rb = Rb::new(true, true);
    let (t1, t2, i) = (rb.cur[7], rb.cur[8], rb.cur[0]);
    let x = rb.aload(t1, i, Ty::Num);
    let y = rb.aload(t2, i, Ty::Num);
    let z = rb.ins(TOp::Fadd, Ty::Num, x, y);
    rb.def(5, z);
    rb.forloop(1);
    let rec = rb.finish();
    assert_eq!(count(&rec.ir, TOp::Ult, true), 2); // (rolled: everything counts as body)
    let p = peel(&rec);
    assert!(p.ir.loop_at.is_some());
    // one check per table in the pre-roll and, the index having advanced, one per table in the body
    assert_eq!(count(&p.ir, TOp::Ult, false), 2, "{}", dump(&p.ir));
    assert_eq!(count(&p.ir, TOp::Ult, true), 2, "{}", dump(&p.ir));
    // and the same table twice is deduplicated
    let mut rb = Rb::new(true, true);
    let (t1, i) = (rb.cur[7], rb.cur[0]);
    let x = rb.aload(t1, i, Ty::Num);
    let k1 = rb.kint(1);
    let i1 = rb.ins(TOp::Add, Ty::I64, i, k1);
    let _ = i1;
    let y = rb.aload(t1, i, Ty::Num);
    let z = rb.ins(TOp::Fadd, Ty::Num, x, y);
    rb.def(5, z);
    rb.forloop(1);
    let p = peel(&rb.finish());
    assert_eq!(count(&p.ir, TOp::Ult, false), 1, "{}", dump(&p.ir));
}

// ---- BUNDLE-L ------------------------------------------------------------------------------------------------------------------------

fn peel_bl(rec: &Recorded, aidx: u32, bhoist: u32) -> Recorded {
    let mut r = rec.clone();
    opt_loop::run(
        &mut r,
        &TraceOpts {
            aidx,
            bhoist,
            ..TraceOpts::default()
        },
    )
    .unwrap();
    opt_loop::check(&r.ir).unwrap();
    r
}

/// `sum += t1[i + c] + t2[i + c]` over a counted loop (`kstep` = the recorded step): the body bounds checks of the induction index.
fn rec_bl(c: i64, kstep: i64, replace_table: bool) -> Recorded {
    let mut rb = Rb::new(true, true);
    let (t1, t2, i) = (rb.cur[7], rb.cur[8], rb.cur[0]);
    let kc = rb.kint(c);
    let idx = if c == 0 {
        i
    } else {
        rb.ins(TOp::Add, Ty::Int, i, kc)
    };
    let x = rb.aload(t1, idx, Ty::Num);
    let y = rb.aload(t2, idx, Ty::Num);
    let z = rb.ins(TOp::Fadd, Ty::Num, x, y);
    rb.def(5, z);
    if replace_table {
        // the first table is another one in every iteration (loaded from the second): its size is not loop invariant
        let t9 = rb.cur[9];
        let t = rb.aload(t9, idx, Ty::Tab);
        rb.def(7, t);
    }
    rb.forloop(kstep);
    rb.finish()
}

/// `bhoist`: the body bounds checks of the induction index are replaced by the three guards per table at the end of the pre-roll.
#[test]
fn bhoist_replaces_the_body_bounds_checks_by_pre_roll_guards() {
    for c in [0i64, 2, -1] {
        let rec = rec_bl(c, 1, false);
        let off = peel_bl(&rec, 0, 0);
        assert_eq!(count(&off.ir, TOp::Ult, false), 2, "{}", dump(&off.ir));
        assert_eq!(count(&off.ir, TOp::Ult, true), 2, "{}", dump(&off.ir));
        let on = peel_bl(&rec, 0, 1);
        assert!(on.ir.hoisted);
        // pre-roll: the two bounds checks of the first iteration + (count, first, last) for each of the two tables; body: none
        assert_eq!(
            count(&on.ir, TOp::Ult, false),
            2 + 6,
            "c={c}\n{}",
            dump(&on.ir)
        );
        assert_eq!(count(&on.ir, TOp::Ult, true), 0, "c={c}\n{}", dump(&on.ir));
        // the last index is `(k0 + count0) + c`
        let l = usize::from(on.ir.loop_at.unwrap() - REF_BIAS);
        let adds_c = on.ir.ins[..l]
            .iter()
            .filter(|i| {
                i.op == TOp::Add
                    && on.ir.as_int(i.b) == Some(c)
                    && TraceIr::is_ins(i.a)
                    && on.ir.at(i.a).op == TOp::Add
            })
            .count();
        if c != 0 {
            assert!(
                adds_c >= 1,
                "c={c}: no `(k0 + count0) + c` in\n{}",
                dump(&on.ir)
            );
        }
        // the six guards leave through one snapshot that is used before the loop only
        let hs: Vec<u32> = on.ir.ins[..l]
            .iter()
            .rev()
            .filter(|i| i.op == TOp::Ult)
            .take(6)
            .map(|i| aux_snap(i.aux))
            .collect();
        assert!(
            hs.len() == 6 && hs.windows(2).all(|w| w[0] == w[1]),
            "{hs:?}"
        );
    }
}

/// `bhoist` keeps the body checks when the bound is not loop invariant or the step is not 1.
#[test]
fn bhoist_keeps_the_checks_it_cannot_cover() {
    // a table that changes in the loop
    let on = peel_bl(&rec_bl(0, 1, true), 0, 1);
    assert!(count(&on.ir, TOp::Ult, true) >= 1, "{}", dump(&on.ir));
    // step 2: the induction value advances by 2 (the recorded `EQ step, 2` guard): not hoisted
    let on = peel_bl(&rec_bl(0, 2, false), 0, 1);
    assert_eq!(count(&on.ir, TOp::Ult, true), 2, "{}", dump(&on.ir));
    assert_eq!(count(&on.ir, TOp::Ult, false), 2, "{}", dump(&on.ir));
}

/// `aidx`: the loads of one index share one complemented-index value, `k + c` is `NEG k` with a displacement.
#[test]
fn aidx_makes_the_complemented_index_an_ir_value() {
    let rec = rec_bl(-1, 1, false);
    let on = peel_bl(&rec, 1, 0);
    let arefs: Vec<&Ins> = on.ir.ins.iter().filter(|i| i.op == TOp::Aref).collect();
    assert_eq!(arefs.len(), 4, "{}", dump(&on.ir));
    for a in &arefs {
        let n = a
            .third()
            .unwrap_or_else(|| panic!("an AREF without a complemented index\n{}", dump(&on.ir)));
        let ni = on.ir.at(n);
        // `~(k - 1) == -k + 0`
        assert_eq!((ni.op, aref_disp(a.aux)), (TOp::Neg, 0), "{}", dump(&on.ir));
    }
    // the two loads of one iteration share the value
    let body: Vec<&&Ins> = arefs.iter().skip(2).collect();
    assert_eq!(body[0].third(), body[1].third(), "{}", dump(&on.ir));
    // `+2`: `~(k + 2) == -k - 3`
    let on2 = peel_bl(&rec_bl(2, 1, false), 1, 0);
    assert!(
        on2.ir
            .ins
            .iter()
            .filter(|i| i.op == TOp::Aref)
            .all(|a| aref_disp(a.aux) == -3),
        "{}",
        dump(&on2.ir)
    );
}

// ---- mutants --------------------------------------------------------------------------------------------------------------------

/// Every mutant of this task is selected in a child process (`RUSTYNAIL_JIT_MUT`), which must fail the tests that guard
/// it; the baseline child must pass.
#[cfg(feature = "tr_mutants")]
#[test]
fn mutants_are_caught() {
    if std::env::var("TR_LOOP_CHILD").is_ok() {
        return;
    }
    let exe = std::env::current_exe().unwrap();
    let main = "rolled_iterations_equal_peeled_traces";
    let table: &[(&str, &[&str])] = &[
        (
            "alias_may_as_no",
            &[
                main,
                "stores_between_accesses_block_load_cse_and_forwarding",
            ],
        ),
        (
            "alias_maytable_as_must",
            &[main, "may_alias_tables_do_not_forward"],
        ),
        (
            "alias_upval_by_index",
            &[main, "distinct_upvalue_indices_are_not_disjoint"],
        ),
        (
            "fwd_across_store",
            &[
                main,
                "stores_between_accesses_block_load_cse_and_forwarding",
            ],
        ),
        (
            "cse_across_store",
            &[
                main,
                "stores_between_accesses_block_load_cse_and_forwarding",
            ],
        ),
        ("peel_subst_stale", &[main]),
        (
            "bounds_key_no_bound",
            &[
                main,
                "bounds_checks_of_different_tables_are_kept",
                "deep_delay_lines_equal_peeled_traces",
            ],
        ),
        (
            "forloop_step_guard_skip",
            &["recorded::forloop_records_a_step_guard"],
        ),
        (
            "forloop_step_add_wrong",
            &["recorded::forloop_records_a_step_guard"],
        ),
        (
            "hdr_op_no_barrier",
            &["header_mutating_ops_stop_the_optimiser"],
        ),
        (
            "nonslot_phi_skip",
            &[main, "array_recurrence_forwards_across_the_backedge"],
        ),
    ];
    let child = |mutant: &str, test: &str| {
        let out = std::process::Command::new(&exe)
            .args(["--exact", test, "--test-threads=1"])
            .env("TR_LOOP_CHILD", "1")
            .env("RUSTYNAIL_JIT_MUT", mutant)
            .stdin(std::process::Stdio::null())
            .output()
            .unwrap();
        (
            out.status.success(),
            format!(
                "{}{}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            ),
        )
    };
    let mut seen: HashMap<&str, bool> = HashMap::new();
    for (mutant, tests) in table {
        for test in *tests {
            if !seen.contains_key(test) {
                let (ok, text) = child("", test);
                assert!(ok, "baseline child of {test} failed\n{text}");
                seen.insert(test, true);
            }
        }
        // a mutant must fail at least one of its tests (each test is a guard of the mutant)
        let caught = tests.iter().any(|t| !child(mutant, t).0);
        assert!(caught, "mutant {mutant} was NOT caught by any of {tests:?}");
    }
}

// ---- recorded traces: the real recorder on Lua loops, optimised vs original under the IR interpreter ------------------------

mod recorded {
    use super::*;
    use rn_core::jit_hooks::{self, Abort, RecCtx, RecStep, RecVTable};
    use rn_core::state::{Ci, LuaState, State, StkId, LUA_MULTRET};
    use rn_core::status::{LuaResult, Status};
    use rn_core::table::TableRef;
    use rn_core::value::Value;
    use rn_jit::lower::Lowered;
    use rn_jit::tr::interp::MTable;
    use rn_jit::tr::record::{self, RecCfg, RecOutcome, FUNC_SLOT};
    use std::cell::{Cell, RefCell};
    use std::collections::HashMap;
    use std::rc::Rc;
    use std::sync::Once;

    // The harness of `tests/tr_record.rs` (dispatching recorder, logging recorder, natives), reduced to what a replay of
    // the first iteration state needs.
    /// What the dispatcher does with a recorder callback of this thread.
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum Mode {
        Record,
        Log,
    }

    /// One logged state of the real interpreter, before an instruction of the root frame.
    #[derive(Clone, Debug, PartialEq)]
    struct Step {
        pc: usize,
        slots: Vec<Val>,
        tabs: Vec<Vec<Val>>,
        ups: Vec<Val>,
    }

    #[derive(Default)]
    struct LogState {
        token: u64,
        head: usize,
        started: bool,
        limit: usize,
        steps: Vec<Step>,
        map: HashMap<usize, u64>,
        tabs: Vec<TableRef>,
    }

    #[derive(Default)]
    struct Last {
        low: Option<Rc<Lowered>>,
        head: u32,
        nslots: usize,
        token: u64,
    }

    thread_local! {
        static MODE: Cell<Mode> = const { Cell::new(Mode::Record) };
        static CFG: RefCell<RecCfg> = RefCell::new(RecCfg::default());
        static LAST: RefCell<Last> = RefCell::new(Last::default());
        static LOG: RefCell<LogState> = RefCell::new(LogState::default());
        static STEPNO: Cell<usize> = const { Cell::new(0) };
    }

    fn d_step(st: &mut State, ci: Ci, pc: *const u32, token: u64) -> RecStep {
        if MODE.get() == Mode::Log {
            return log_step(st, ci, pc, token);
        }
        record::step(st, ci, pc, token)
    }

    fn d_abort(st: &mut State, why: Abort, token: u64) {
        if MODE.get() == Mode::Record {
            record::abort(st, why, token);
        }
        let _ = why;
    }

    fn pc_index(st: &mut State, ci: Ci, pc: *const u32) -> usize {
        let cl = st.ci_lclosure(ci).expect("Lua frame");
        (pc as usize - cl.proto().code_ptr() as usize) / 4
    }

    fn vval(reg: &mut LogState, v: &Value) -> Val {
        match record::ty_of_value(v) {
            None => Val {
                t: Ty::Udata,
                bits: 0xDEAD,
            },
            Some(Ty::Tab) => {
                let t = v.as_table().unwrap();
                let key = t.as_ptr() as usize;
                let h = *reg.map.entry(key).or_insert_with(|| {
                    reg.tabs.push(t);
                    reg.tabs.len() as u64
                });
                Val {
                    t: Ty::Tab,
                    bits: h,
                }
            }
            Some(t) => Val {
                t,
                bits: match t {
                    Ty::Int => v.as_int().unwrap() as u64,
                    Ty::Num => v.as_float().unwrap().to_bits(),
                    Ty::Nil | Ty::False | Ty::True => 0,
                    Ty::LUdata => v.as_light_userdata().map_or(0, |p| p as usize as u64),
                    Ty::Lfn => v.as_light_cfn().map_or(0, |p| p as usize as u64),
                    _ => v.as_gc().map_or(0, |p| p as usize as u64),
                },
            },
        }
    }

    fn log_step(st: &mut State, ci: Ci, pc: *const u32, token: u64) -> RecStep {
        let idx = pc_index(st, ci, pc);
        let cl = st.ci_lclosure(ci).unwrap();
        let p = cl.proto();
        let raw = rn_core::instruction::Instruction(p.code()[idx]).opcode_raw();
        use rn_core::ver::active::OpCode as O;
        let is_ret = [O::Return as u8, O::Return0 as u8, O::Return1 as u8].contains(&raw);
        let nslots = usize::from(p.maxstacksize);
        let base = ci.r().func.0 + 1;
        LOG.with(|l| {
            let mut l = l.borrow_mut();
            if l.token != token {
                return RecStep::Leave;
            }
            if !l.started {
                if idx != l.head {
                    return RecStep::Keep;
                }
                l.started = true;
                // register everything reachable from the frame and the upvalues (breadth first)
                let slots: Vec<Value> = (0..nslots).map(|n| st.get(StkId(base + n))).collect();
                for v in &slots {
                    vval(&mut l, v);
                }
                for u in 0..cl.nupvalues() {
                    vval(&mut l, &cl.upval(u).get());
                }
                let mut i = 0;
                while i < l.tabs.len() {
                    let t = l.tabs[i];
                    for k in 1..=t.array_size() as i64 {
                        if let Some(e) = t.get_int_arr(k) {
                            vval(&mut l, &e);
                        }
                    }
                    i += 1;
                }
            }
            if l.steps.len() >= l.limit || is_ret {
                return RecStep::Leave;
            }
            let slots: Vec<Val> = (0..nslots)
                .map(|n| vval(&mut l, &st.get(StkId(base + n))))
                .collect();
            let ups: Vec<Val> = (0..cl.nupvalues())
                .map(|u| vval(&mut l, &cl.upval(u).get()))
                .collect();
            let tabs: Vec<Vec<Val>> = l
                .tabs
                .clone()
                .iter()
                .map(|t| {
                    (1..=t.array_size() as i64)
                        .map(|k| match t.get_int_arr(k) {
                            Some(e) => vval(&mut l, &e),
                            None => Val::NIL,
                        })
                        .collect()
                })
                .collect();
            l.steps.push(Step {
                pc: idx,
                slots,
                tabs,
                ups,
            });
            RecStep::Keep
        })
    }

    /// `arm()`: record the loop that follows. `armlog()`: log the real interpreter over that loop.
    fn arm_common(st: &mut State, log: bool) -> LuaResult<usize> {
        let caller = st.ci().previous().expect("caller");
        let cl = st.ci_lclosure(caller).unwrap();
        let p = cl.proto();
        let low = Rc::new(record::lower_for_landing(&p).expect("lowers"));
        let pc = pc_index(st, caller, caller.r().savedpc);
        let head = record::first_head_after(&low, pc as u32).expect("a loop follows arm()");
        LAST.with(|l| {
            let mut l = l.borrow_mut();
            l.low = Some(low.clone());
            l.head = head;
            l.nslots = usize::from(p.maxstacksize);
        });
        STEPNO.set(usize::MAX);
        if log {
            let tok = jit_hooks::rec_arm(st, caller).expect("armed");
            LOG.with(|l| {
                let mut l = l.borrow_mut();
                let limit = l.limit;
                *l = LogState {
                    token: tok,
                    head: head as usize,
                    limit,
                    ..LogState::default()
                };
            });
        } else {
            let mut cfg = CFG.with(|c| c.borrow().clone());
            cfg.landing = Some(low);
            cfg.wait_head = true;
            let tok = record::arm(st, caller, head, cfg).expect("recorder armed");
            LAST.with(|l| l.borrow_mut().token = tok);
        }
        Ok(0)
    }

    fn n_arm(st: &mut State) -> LuaResult<usize> {
        arm_common(st, false)
    }

    fn n_armlog(st: &mut State) -> LuaResult<usize> {
        arm_common(st, true)
    }

    fn init() {
        static ONCE: Once = Once::new();
        ONCE.call_once(|| {
            assert!(jit_hooks::install_rec(RecVTable {
                step: d_step,
                abort: d_abort
            }));
            jit_hooks::set_intrinsics(rn_std::lib_math::jit_builtins());
        });
        #[cfg(feature = "tr_mutants")]
        rn_jit::tr::mutants::apply_vm_mutants();
    }

    fn reset(cfg: RecCfg) {
        init();
        record::discard();
        jit_hooks::rec_set_ctx(RecCtx::default());
        MODE.set(Mode::Record);
        CFG.with(|c| *c.borrow_mut() = cfg);
        STEPNO.set(usize::MAX);
    }

    fn new_state() -> LuaState {
        init();
        // the libraries without io / os / package (they need libc, which Miri lacks), a fixed hash seed
        let mut st = rn_std::new_state(0x5eed).expect("state");
        for (name, open) in rn_std::STD_LIBS {
            if matches!(*name, b"package" | b"io" | b"os") {
                continue;
            }
            st.requiref(name, *open, true).expect("open library");
            st.pop();
        }
        let g = st.globals();
        st.set_funcs(g, &[("arm", Some(n_arm)), ("armlog", Some(n_armlog))])
            .unwrap();
        st
    }

    /// Runs `src` as a chunk under `pcall`; returns `"<status>:<result as string>"`.
    fn run(st: &mut LuaState, src: &str) -> String {
        let base = st.top();
        assert_eq!(
            st.load_buffer(src.as_bytes(), b"=snippet", None),
            Status::Ok
        );
        let func = st.top() - 1;
        let status = st.pcall(func, LUA_MULTRET, 0);
        let v = st.get(func);
        let s = v.as_string().map_or_else(
            || format!("{v:?}"),
            |s| String::from_utf8_lossy(s.as_bytes()).into_owned(),
        );
        st.set_top(base);
        format!("{status:?}:{s}")
    }

    /// A finished recording with what the test needs to check it.
    struct Rec {
        out: RecOutcome,
        result: String,
    }

    /// Records the loop that follows `arm()` in `src` (the state is dropped before returning, so a recording that is still
    /// armed gets `Abort::Close`).
    fn record_src(src: &str, cfg: RecCfg) -> Rec {
        reset(cfg);
        let mut st = new_state();
        let result = run(&mut st, src);
        drop(st);
        let (tok, out) = record::take_outcome().expect("the recording ended with an outcome");
        assert_eq!(tok, LAST.with(|l| l.borrow().token));
        assert!(!record::is_recording());
        Rec { out, result }
    }

    /// The real interpreter's states over the loop that follows `armlog()` in `src` (at most `limit` steps).
    fn log_src(src: &str, limit: usize) -> Vec<Step> {
        reset(RecCfg::default());
        MODE.set(Mode::Log);
        LOG.with(|l| l.borrow_mut().limit = limit);
        let mut st = new_state();
        let _ = run(&mut st, src);
        drop(st);
        MODE.set(Mode::Record);
        LOG.with(|l| std::mem::take(&mut l.borrow_mut().steps))
    }

    /// `SLOAD FUNC_SLOT` becomes the model closure (handle 1).
    fn rewrite_func(ir: &TraceIr) -> TraceIr {
        let mut ir = ir.clone();
        let kcl = ir.konst(KVal::Ptr(1, PtrKind::LClosure));
        for i in &mut ir.ins {
            if i.op == TOp::Sload && i.a == FUNC_SLOT {
                i.op = TOp::Kptr;
                i.a = kcl;
                i.flags = 0;
                i.aux = 0;
            }
        }
        ir
    }

    fn model_from(s: &Step) -> Model {
        Model {
            slots: s.slots.clone(),
            tables: s
                .tabs
                .iter()
                .map(|a| MTable {
                    array: a.clone(),
                    metatable: 0,
                    flags: 0,
                    lsizenode: 0,
                    lenhint: 0,
                    nodes: Default::default(),
                })
                .collect(),
            closures: vec![s.ups.clone()],
            ..Model::new()
        }
    }

    /// Random variation of the recorded state: values, `nil` holes, budget.
    fn vary(rng: &mut Rng, base: &Model) -> Model {
        let mut m = base.clone();
        for s in &mut m.slots {
            if rng.chance(1, 3) {
                match s.t {
                    Ty::Int => {
                        s.bits = (s.bits as i64).wrapping_add(rng.below(5) as i64 - 2) as u64
                    }
                    Ty::Num => s.bits = rng.pick(NUMS),
                    _ => {}
                }
            }
        }
        for t in &mut m.tables {
            // G1: other lengths (a table can shrink, so a bounds check of one table never covers another)
            if rng.chance(1, 3) && t.array.len() > 3 {
                let n = 3 + rng.below(t.array.len() as u64 - 3) as usize;
                t.array.truncate(n);
            }
            for e in &mut t.array {
                if rng.chance(1, 8) {
                    match e.t {
                        Ty::Int => e.bits = rng.below(9),
                        Ty::Num => e.bits = rng.pick(NUMS),
                        _ => {}
                    }
                }
                if rng.chance(1, 50) {
                    *e = Val::NIL;
                }
            }
        }
        for u in &mut m.closures[0] {
            if rng.chance(1, 4) {
                match u.t {
                    Ty::Int => u.bits = rng.below(9),
                    Ty::Num => u.bits = rng.pick(NUMS),
                    _ => {}
                }
            }
        }
        if rng.chance(1, 4) {
            m.budget = rng.below(40) as i64;
        }
        m
    }

    const SRCS: &[(&str, &str)] = &[
        (
            "array_rec",
            "local function run(t, n, c, x) ARM() for i = 2, n do t[i] = t[i - 1] * c + x end return t[n] end
             local t = {} for i = 1, 40 do t[i] = 0.5 end
             return run(t, 40, 0.999, 0.5)",
        ),
        (
            "int_rec2",
            "local function run(t, n) ARM() for i = 3, n do t[i] = (t[i - 1] + t[i - 2]) & 0xFFFFFF end return t[n] end
             local t = {} for i = 1, 40 do t[i] = i end
             return run(t, 40)",
        ),
        (
            "upval_acc",
            "local acc = 0
             local function run(t, n) ARM() for i = 1, n do acc = acc + t[i] end end
             local t = {} for i = 1, 40 do t[i] = i % 7 end
             run(t, 40) return acc",
        ),
        (
            "licm_expr",
            "local sqrt = math.sqrt
             local function run(t, n, a, b, c)
               local s = 0.0
               ARM()
               for i = 1, n do s = s + t[i] * (sqrt(a * a + b * b) / c) end
               return s
             end
             local t = {} for i = 1, 40 do t[i] = i * 0.5 end
             return run(t, 40, 3.0, 4.0, 2.0)",
        ),
        (
            "dot",
            "local function dot(a, b, n) local s = 0.0 ARM() for i = 1, n do s = s + a[i] * b[i] end return s end
             local a, b = {}, {}
             for i = 1, 40 do a[i] = i * 0.25; b[i] = 2.0 - i * 0.125 end
             return dot(a, b, 40)",
        ),
        (
            "dot_ragged",
            "local function dot(a, b, n) local s = 0.0 ARM() for i = 1, n do s = s + a[i] * b[i] end return s end
             local a, b = {}, {}
             for i = 1, 40 do a[i] = i * 0.25 end
             for i = 1, 25 do b[i] = 2.0 - i * 0.125 end
             return dot(a, b, 25)",
        ),
        (
            "delay_line",
            "local function f(t, n) ARM() for i = 4, n do t[i] = t[i - 1] * 0.5 + t[i - 3] end return t[n] end
             local t = {} for i = 1, 40 do t[i] = i * 0.5 end
             return f(t, 40)",
        ),
        (
            "upval_inv",
            "local t = {}
             for i = 1, 40 do t[i] = i * 1.5 end
             local function run(n) local s = 0.0 ARM() for i = 1, n do s = s + t[i] end return s end
             return run(40)",
        ),
        (
            "alias_same_table",
            "local function f(a, b, n) ARM() for i = 2, n do a[i] = b[i - 1] + 1 end return a[n] end
             local a = {} for i = 1, 40 do a[i] = i end
             return f(a, a, 40)",
        ),
        (
            "alias_two_tables",
            "local function f(a, b, n) ARM() for i = 2, n do a[i] = b[i - 1] + 1; b[i] = a[i] * 2 end return a[n] end
             local a, b = {}, {}
             for i = 1, 40 do a[i] = i; b[i] = i + 3 end
             return f(a, b, 40)",
        ),
        (
            "swap_tables",
            "local function f(a, b, n)
               ARM()
               for i = 2, n do a[i] = b[i - 1] * 0.5; a, b = b, a end
               return a[n]
             end
             local a, b = {}, {}
             for i = 1, 40 do a[i] = i + 0.5; b[i] = i * 0.25 end
             return f(a, b, 39)",
        ),
        (
            "float_for",
            "local function f(n) local s = 0.0 ARM() for x = 0.5, n, 0.5 do s = s + x * x end return s end
             return f(20)",
        ),
        (
            "while_branch",
            "local function f(t, n)
               local s, i = 0, 1
               ARM()
               while i <= n do
                 if t[i] > 3 then s = s + t[i] else s = s - 1 end
                 i = i + 1
               end
               return s
             end
             local t = {} for i = 1, 40 do t[i] = i % 7 end
             return f(t, 40)",
        ),
        (
            "two_upvalues",
            "local u, v = 0, 100
             local function run(t, n) ARM() for i = 1, n do u = u + t[i]; v = v - u end end
             local t = {} for i = 1, 40 do t[i] = i end
             run(t, 40) return u, v",
        ),
    ];

    const PER_PROG: u64 = if cfg!(miri) { 2 } else { 40 };

    pub fn check_program(name: &str, src: &str, st: &mut super::Counts) {
        let rc = record_src(
            &src.replace("ARM()", "arm()"),
            RecCfg {
                verify: true,
                ..RecCfg::default()
            },
        );
        let rec = match &rc.out {
            RecOutcome::Closed(r) => (**r).clone(),
            RecOutcome::Aborted(a) => panic!("{name}: aborted {a:?} ({})", rc.result),
        };
        let log = log_src(&src.replace("ARM()", "armlog()"), 2);
        assert!(!log.is_empty(), "{name}: nothing logged");
        let base_model = model_from(&log[0]);
        let mut rec = rec;
        rec.ir = rewrite_func(&rec.ir);
        let mut rng = Rng::new(name.len() as u64 * 7919 + 5);
        for (l, f) in [(true, true), (true, false), (false, true)] {
            let mut opt = rec.clone();
            opt_loop::run(&mut opt, &opts(l, f, true)).unwrap();
            opt_loop::check(&opt.ir).unwrap_or_else(|e| panic!("{name}: {e}\n{}", dump(&opt.ir)));
            let peeled = opt.ir.loop_at.is_some();
            assert_eq!(
                peeled,
                l,
                "{name}: peeling expected={l}: {}",
                opt_loop::last_note()
            );
            st.cases += 1;
            for k in 0..PER_PROG {
                let m = if k == 0 {
                    base_model.clone()
                } else {
                    vary(&mut rng, &base_model)
                };
                let poll_at = (k % 5 == 4).then(|| 1 + rng.below(u64::from(ITERS)) as u32);
                let a = run_rolled(&rec.ir, &rec.writeback, &m, poll_at, ITERS);
                let b = if peeled {
                    run_peeled(&opt.ir, &m, poll_at, ITERS)
                } else {
                    run_rolled(&opt.ir, &opt.writeback, &m, poll_at, ITERS)
                };
                assert_eq!(
                    a,
                    b,
                    "{name} loop={l} fwd={f} variant {k} poll_at={poll_at:?}\n--- recorded\n{}\n--- optimised\n{}",
                    dump(&rec.ir),
                    dump(&opt.ir)
                );
                match a.end {
                    End::Done => st.done += 1,
                    End::Exit { .. } => st.exits += 1,
                }
            }
            if peeled {
                st.peeled += 1;
            }
        }
    }

    /// The real recorder on the gate loops and hand-written ones (arrays, aliasing tables, upvalues, intrinsics, float
    /// loops, `while` with a branch): every optimiser configuration equals the original trace on the recorded state and
    /// on random variations of it (every iteration compared, exits and budget included).
    #[test]
    #[cfg_attr(miri, ignore)]
    fn recorded_loops_optimise_equal() {
        let mut st = super::Counts::default();
        for (name, src) in SRCS {
            check_program(name, src, &mut st);
        }
        eprintln!(
            "recorded: cases {} peeled {} done {} exits {}",
            st.cases, st.peeled, st.done, st.exits
        );
        assert!(st.done > 20 && st.exits > 20);
    }

    /// P7b G2: the recorder specialises a numeric `for` on its step: an `EQ step, #k` guard (loop invariant after
    /// peeling) and an index advanced by `k`, for several steps. The recording is validated (`verify`), so a wrong
    /// constant aborts it.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn forloop_records_a_step_guard() {
        for ks in [1i64, 2, 3, -1] {
            let (a, b) = if ks > 0 { (1, 40) } else { (40, 1) };
            let src = format!(
                "local function run(t, n) local s = 0 arm() for i = {a}, {b}, {ks} do s = s + t[i] end return s end
                 local t = {{}} for i = 1, 40 do t[i] = i end
                 return run(t, 40)"
            );
            let rc = record_src(
                &src,
                RecCfg {
                    verify: true,
                    ..RecCfg::default()
                },
            );
            let rec = match &rc.out {
                RecOutcome::Closed(r) => (**r).clone(),
                RecOutcome::Aborted(a) => panic!("step {ks}: aborted {a:?} ({})", rc.result),
            };
            let ir = &rec.ir;
            let guarded = ir
                .ins
                .iter()
                .any(|i| i.op == TOp::Eq && i.flags & F_GUARD != 0 && ir.as_int(i.b) == Some(ks));
            assert!(guarded, "step {ks}: no EQ step guard\n{}", dump(ir));
            let adv = ir
                .ins
                .iter()
                .any(|i| i.op == TOp::Add && ir.as_int(i.b) == Some(ks));
            assert!(
                adv,
                "step {ks}: the index does not advance by the step\n{}",
                dump(ir)
            );
            // peeled: the guard is in the pre-roll only
            let mut p = rec.clone();
            opt_loop::run(&mut p, &TraceOpts::default()).unwrap();
            assert!(p.ir.loop_at.is_some());
            let l = usize::from(p.ir.loop_at.unwrap() - REF_BIAS);
            let after = p.ir.ins[l..]
                .iter()
                .filter(|i| {
                    i.op == TOp::Eq && i.flags & F_GUARD != 0 && p.ir.as_int(i.b) == Some(ks)
                })
                .count();
            assert_eq!(
                after,
                0,
                "step {ks}: the step guard is not hoisted\n{}",
                dump(&p.ir)
            );
        }
    }

    /// P-F: a step that is not a literal of the prototype (a parameter) is an invariant `SLOAD`, not a trace constant: no
    /// `EQ step, #k` guard, and the index advances by the step operand itself.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn forloop_variable_step_is_not_a_constant() {
        let src = "local function run(t, st) local s = 0 arm() for i = 1, 40, st do s = s + t[i] end return s end
                 local t = {} for i = 1, 40 do t[i] = i end
                 return run(t, 3)";
        let rc = record_src(
            src,
            RecCfg {
                verify: true,
                ..RecCfg::default()
            },
        );
        let rec = match &rc.out {
            RecOutcome::Closed(r) => (**r).clone(),
            RecOutcome::Aborted(a) => panic!("aborted {a:?} ({})", rc.result),
        };
        let ir = &rec.ir;
        let guarded = ir
            .ins
            .iter()
            .any(|i| i.op == TOp::Eq && i.flags & F_GUARD != 0 && ir.as_int(i.b) == Some(3));
        assert!(
            !guarded,
            "a constant step guard on a run-time step\n{}",
            dump(ir)
        );
        let adv = ir.ins.iter().any(|i| {
            i.op == TOp::Add
                && ir.as_int(i.b).is_none()
                && i.b >= REF_BIAS
                && ir.ins[usize::from(i.b - REF_BIAS)].op == TOp::Sload
        });
        assert!(
            adv,
            "the index does not advance by the step slot\n{}",
            dump(ir)
        );
    }

    /// P-F: `FORPREP` in a nest root (the recorder reaches an inner loop that has a root trace and links into it): a literal
    /// step divides by a constant (guarded to its value when the step is an `SLOAD`); a run-time step guards only its sign (`step > 0`) and divides
    /// by the step operand. Mutants `forprep_step_guard_skip`, `forprep_step_always_lit`, `forprep_var_sign_skip`.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn forprep_literal_and_variable_steps() {
        for (step_src, literal) in [("2", true), ("st", false)] {
            let src = format!(
                "local function run(n, st) local s = 0 arm() for i = 1, n do for j = i, n, {step_src} do s = s + j end end return s end
                 return run(40, 2)"
            );
            // the nest query says "loop head with a root trace" for exactly one pc: the inner loop's head, found by trying
            // the pcs behind the outer head (the layout differs between the versions)
            let mut found = None;
            for cand in 8u32..24 {
                let rc = record_src(
                    &src,
                    RecCfg {
                        verify: true,
                        nest: Some(Rc::new(move |_, pc| (pc == cand).then_some(true))),
                        ..RecCfg::default()
                    },
                );
                if let RecOutcome::Closed(r) = &rc.out {
                    if r.ir.ins.iter().any(|i| i.op == TOp::Idiv) {
                        found = Some((**r).clone());
                        break;
                    }
                }
            }
            let rec = found
                .unwrap_or_else(|| panic!("step {step_src}: no nest recording with a FORPREP"));
            let ir = &rec.ir;
            let eq2 = ir
                .ins
                .iter()
                .any(|i| i.op == TOp::Eq && i.flags & F_GUARD != 0 && ir.as_int(i.b) == Some(2));
            let sign = ir
                .ins
                .iter()
                .any(|i| i.op == TOp::Gt && i.flags & F_GUARD != 0 && ir.as_int(i.b) == Some(0));
            let vdiv = ir
                .ins
                .iter()
                .any(|i| i.op == TOp::Idiv && ir.as_int(i.b).is_none());
            let cdiv = ir
                .ins
                .iter()
                .any(|i| i.op == TOp::Idiv && ir.as_int(i.b) == Some(2));
            if literal {
                // (the literal step is a trace constant here: `LOADI` is in the trace, so its `EQ` guard folds away)
                assert!(
                    !sign && cdiv && !vdiv,
                    "literal step: constant divisor, no sign guard expected\n{}",
                    dump(ir)
                );
            } else {
                assert!(
                    !eq2 && sign && vdiv && !cdiv,
                    "run-time step: sign guard + variable divisor expected\n{}",
                    dump(ir)
                );
            }
        }
    }

    /// The Miri-sized run: two loops, two variations each.
    #[test]
    fn miri_recorded_small() {
        let mut st = super::Counts::default();
        for (name, src) in SRCS
            .iter()
            .filter(|(n, _)| ["array_rec", "upval_acc"].contains(n))
        {
            check_program(name, src, &mut st);
        }
        assert!(st.cases >= 6);
    }
}

/// Debugging aid: `TR_LOOP_SEED=n cargo test ... debug_seed -- --ignored --nocapture` prints both runs of one case.
#[test]
#[ignore]
fn debug_seed() {
    let seed: u64 = std::env::var("TR_LOOP_SEED")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1);
    let rec0 = gen_rolled(seed, false).expect("valid recording");
    let mut rec = rec0.clone();
    opt_loop::run(&mut rec, &opts(true, true, true)).unwrap();
    let mut rng = Rng::new(seed ^ 0xABCD);
    let m = model(&mut rng, 1, false, false);
    eprintln!("slots {:?}", m.slots);
    let a = run_rolled(&rec0.ir, &rec0.writeback, &m, None, ITERS);
    let b = run_peeled(&rec.ir, &m, None, ITERS);
    eprintln!("base end {:?}\nopt end {:?}", a.end, b.end);
    eprintln!("{}", dump(&rec.ir));
}

/// BUNDLE-H1: forwarding and load CSE on a stitched (loop-free) trace: the load that follows a store of the same element is the stored
/// value, and the always-false guard that ends the recording (`EQ #0 #1`) is copied, not folded (a guard proven false is `ConstGuard`, which
/// made the pass give up on every side and stitched trace). Mutants `sidefwd_off`, `end_exit_folded`.
#[test]
fn h1_forwarding_runs_on_a_stitched_trace() {
    let mut rb = Rb::new(true, true);
    let (t, i, c) = (rb.cur[7], rb.cur[0], rb.cur[5]);
    rb.astore(t, i, c, false);
    let x = rb.aload(t, i, Ty::Num);
    let y = rb.ins(TOp::Fadd, Ty::Num, x, c);
    rb.def(6, y);
    rb.insn();
    let s = rb.snapshot();
    let (k0, k1) = (rb.kint(0), rb.kint(1));
    rb.b.emit_raw(Ins {
        op: TOp::Eq,
        t: Ty::I64,
        flags: F_GUARD,
        a: k0,
        b: k1,
        prev: NOREF,
        aux: guard_aux(s, 0),
    })
    .unwrap();
    let mut rec = rb.finish();
    rec.stitched = true;
    let n = |ir: &TraceIr, op: TOp| ir.ins.iter().filter(|i| i.op == op).count();
    assert_eq!(n(&rec.ir, TOp::Aload), 1, "{}", dump(&rec.ir));
    opt_loop::run_fwd(&mut rec, &TraceOpts::default()).unwrap();
    let ir = &rec.ir;
    assert_eq!(
        n(ir, TOp::Aload),
        0,
        "the load is the stored value\n{}",
        dump(ir)
    );
    assert_eq!(n(ir, TOp::Astore), 1, "{}", dump(ir));
    let last = ir.ins.last().unwrap();
    assert!(
        last.op == TOp::Eq && last.flags & F_GUARD != 0,
        "the ending exit stays last\n{}",
        dump(ir)
    );
}
