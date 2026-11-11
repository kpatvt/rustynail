//! Inline table access in regions (task T5; docs/JIT_TIER15.md section 19).
//!
//! `GETI GETTABLE GETFIELD` / `SETI SETTABLE SETFIELD` whose table slot has the fact `TABLE` (and whose key slot has
//! the fact `INT` for `GETTABLE`/`SETTABLE`) get an inline fast path: array part (bounds check against `asize`,
//! element tag check) or hash part (constant short-string key, the chain walk of tier 1's `node_lookup` rewritten
//! on the region's scratch registers). **Everything uncommon leaves through one out-of-line slow path** per
//! instruction: flush the register-resident slots, store the budget, run the *tier-1 template of the same
//! instruction* (so the miss semantics - nil result, `__index`/`__newindex`, insertion, rehash, barrier work, errors -
//! stay in one place) and continue in `stub(pc+1)` (the fallback protocol of section 16, `Rg::emit_fall`).
//!
//! # Invariants (the review checklist of section 19.2)
//!
//! * **Pure until committed (E3).** Every check precedes the first store; the slow path is entered with the
//!   register-resident set `rr` of the moment all operand registers were loaded, and none of the fast path's
//!   instructions changes registers that the slow path flush relies on.
//! * **No derived pointer outlives the instruction (R-DERIVED / G1).** The table pointer (loaded from its VM stack
//!   slot, which keeps it alive), the array base and the node address live in scratch registers `S0..S3` only until
//!   the instruction ends; nothing caches them across a barrier or a helper. There is no GC point inside the fast
//!   path (no helper call, no allocation), so objects cannot move or die while the pointers are held.
//! * **Table slots are never register-resident** (collectables are write-through, R-G1): the pointer is read from
//!   the slot. A table slot found in `RR` refuses the region.
//! * **Stores mirror tier 1 exactly** (`templates_table.rs`): an array slot that is non-empty, or any slot of a table
//!   without a metatable; a hash node that already holds a non-nil value. The GC barrier is evaluated *before* the
//!   store and any case where `barrier_back` would do work (table black, value collectable and white) takes the
//!   slow path, so no barrier code exists in region code. Values that are statically non-collectable (`INT`, `FLT`,
//!   `nil`, booleans) skip the test: the interpreter's `barrier_back` is a no-op for them.
//! * **Loaded results are typed by the speculation of the analysis** (`InsnPlan::guard_def`): the element's tag byte is
//!   compared with the speculated tag (one compare that also rejects nil); a mismatch takes the slow path, whose
//!   `stub(pc+1)` then rejects the unexpected type and continues in tier 1 (def guard without an exit stub).
//! * Scratch: `S0 = rax`, `S1 = rdx`, `S2 = rcx`, `S3 = r10 (V4)`. Regions with table accesses assign only `V0..V3`
//!   (`Rg` limits the allocation like it does for `DivK`).

use super::{Act, FOpd, Rg, S0, S1, S2, TAG_INT};
use crate::ir::{Base, Cc, Mi, Op, Src, L, M, V, W, X};
use crate::lower::fr;
use crate::t15::TagSet;
use rn_core::gc::{BLACKBIT, WHITEBITS};
use rn_core::instruction::Instruction;
use rn_core::jit_layout::{node, table};
use rn_core::value::{ctb, BIT_ISCOLLECTABLE, LUA_VNUMFLT, LUA_VSHRSTR, LUA_VTABLE};
use rn_core::ver::active::OpCode;

/// Fourth scratch register of regions with table accesses (`V4 = r10`).
pub(super) const S3: V = 4;
const TAG_FLT: i32 = LUA_VNUMFLT as i32;
const TAG_TABLE: i32 = ctb(LUA_VTABLE) as i32;
const TAG_SHRSTR: i32 = ctb(LUA_VSHRSTR) as i32;
const NODE_SIZE: i32 = node::SIZE as i32;

/// Key of an inline array access.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::t15) enum TKey {
    /// Constant 0-based index (`GETI`/`SETI`).
    Imm(u32),
    /// A slot with the fact `INT`.
    Reg(u32),
    /// A slot whose fact does not exclude `INT` (not proven): the tag is checked at run time.
    RegU(u32),
}

/// How the loaded value of a `GET` is committed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::t15) enum TRes {
    /// Integer element (tag compared), into the slot's integer register or its slot.
    Int,
    /// Float element (tag compared), into the slot's double register or its slot.
    Flt,
    /// Any other value copied to the slot: `Some(tag)` compares the tag byte, `None` only rejects nil.
    Mem(Option<u8>),
}

/// Can the stored value be a collectable object?
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::t15) enum Coll {
    No,
    Yes,
    Maybe,
}

/// The value operand of a `SET`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::t15) enum TVal {
    /// A slot with the fact `INT`.
    Int(u32),
    /// A slot with the fact `FLT`.
    Flt(u32),
    /// Any other slot: copied from memory.
    Slot(u32, Coll),
    /// A constant of the constants array.
    K(u32, Coll),
}

#[derive(Clone, Copy)]
enum Ad {
    M(M),
    I(Mi),
}

/// Where the element of an access lives (tag and payload addresses); `neg` is an index register that must be
/// negated once before the payload address is used.
struct Elem {
    tag: Ad,
    pay: Ad,
    neg: Option<V>,
}

fn rm(base: V, off: i32) -> M {
    M {
        base: Base::Reg(base),
        off,
    }
}

fn kcoll(tag: u8) -> Coll {
    if tag & BIT_ISCOLLECTABLE != 0 {
        Coll::Yes
    } else {
        Coll::No
    }
}

impl Rg<'_, '_> {
    /// Is the act an inline table access?
    pub(super) fn is_table_act(a: &Act) -> bool {
        matches!(
            a,
            Act::GetArr { .. } | Act::SetArr { .. } | Act::GetFld { .. } | Act::SetFld { .. }
        )
    }

    /// The inline form of the table access at `pc`, if the facts allow it.
    pub(super) fn classify_table(&self, pc: u32, op: OpCode, i: Instruction) -> Option<Act> {
        use OpCode as O;
        let insn = self.plan.insn(pc)?;
        if insn.disp == crate::t15::Disp::Dead || self.opt.no_tables {
            return None;
        }
        let (a, b, c) = (i.a(), i.b(), i.c());
        let f = |s: u32| self.plan.fact(pc, s);
        // `proven`: the fact says it (the stub guard / a typed def established it); otherwise the tag is checked at
        // run time unless the fact excludes the class altogether
        let tab = |s: u32| !f(s).meet(TagSet::TABLE).is_empty();
        let tp = |s: u32| f(s) == TagSet::TABLE;
        let int = |s: u32| !f(s).meet(TagSet::INT).is_empty();
        let key = |s: u32| {
            if f(s) == TagSet::INT {
                TKey::Reg(s)
            } else {
                TKey::RegU(s)
            }
        };
        // The analysis assumes the speculated `guard_def` for the slot after the load (and downstream code relies on
        // it: e.g. a NIL fact makes a later store skip the GC barrier). The inline form checks one tag byte only, so it
        // is generated for no speculation (`Mem(None)`: no fact) or a non-nil singleton; a widened (`INT|FLT`) or nil
        // speculation keeps the fallback, whose stub re-checks the fact (docs 21.2).
        let res = || match insn.guard_def {
            None => Some(TRes::Mem(None)),
            Some(TagSet::INT) => Some(TRes::Int),
            Some(TagSet::FLT) => Some(TRes::Flt),
            Some(t) if t.is_singleton() && t != TagSet::NIL => Some(TRes::Mem(t.tag_byte())),
            // (mutant `tbl_widen_unchecked` restores the pre-T7 behaviour: an unchecked commit under a nil/widened fact)
            Some(_) if crate::t15::mutants::active("tbl_widen_unchecked") => Some(TRes::Mem(None)),
            Some(_) => None,
        };
        let tval = |c: u32, k: bool| -> Option<TVal> {
            if k {
                let t = self.cx.cfg.consts.get(c as usize)?.tag();
                Some(TVal::K(c, kcoll(t)))
            } else {
                let fv = f(c);
                Some(if fv == TagSet::INT {
                    TVal::Int(c)
                } else if fv == TagSet::FLT {
                    TVal::Flt(c)
                } else {
                    // (only a non-collectable class lets the barrier test be skipped; a collectable fact still
                    // tests the collectable bit at run time, so memory safety never rests on the fact)
                    let coll = match fv.tag_byte().filter(|_| fv.is_singleton()) {
                        Some(t) if kcoll(t) == Coll::No => Coll::No,
                        _ => Coll::Maybe,
                    };
                    TVal::Slot(c, coll)
                })
            }
        };
        let kstr = |kx: u32| {
            self.cx
                .cfg
                .consts
                .get(kx as usize)
                .is_some_and(|v| v.tag() == ctb(LUA_VSHRSTR))
        };
        match op {
            O::Geti if tab(b) && (1..=1 << 20).contains(&c) => Some(Act::GetArr {
                a,
                t: b,
                tp: tp(b),
                key: TKey::Imm(c - 1),
                res: res()?,
            }),
            O::Gettable if tab(b) && int(c) => Some(Act::GetArr {
                a,
                t: b,
                tp: tp(b),
                key: key(c),
                res: res()?,
            }),
            O::Getfield if tab(b) && kstr(c) => Some(Act::GetFld {
                a,
                t: b,
                tp: tp(b),
                kx: c,
                res: res()?,
            }),
            O::Seti if tab(a) && (1..=1 << 20).contains(&b) => Some(Act::SetArr {
                t: a,
                tp: tp(a),
                key: TKey::Imm(b - 1),
                val: tval(c, i.k() != 0)?,
            }),
            O::Settable if tab(a) && int(b) => Some(Act::SetArr {
                t: a,
                tp: tp(a),
                key: key(b),
                val: tval(c, i.k() != 0)?,
            }),
            O::Setfield if tab(a) && kstr(b) => Some(Act::SetFld {
                t: a,
                tp: tp(a),
                kx: b,
                val: tval(c, i.k() != 0)?,
            }),
            _ => None,
        }
    }

    // ---- small emitters ------------------------------------------------------------------------

    fn ad_br(&mut self, cc: Cc, w: W, a: Ad, imm: i32, to: L) {
        match a {
            Ad::M(m) => self.cx.op(Op::BrM { cc, w, m, imm, to }),
            Ad::I(m) => self.cx.op(Op::BrMi { cc, w, m, imm, to }),
        }
    }
    fn ad_ld(&mut self, dst: V, w: W, a: Ad) {
        match a {
            Ad::M(m) => self.cx.op(Op::Ld { dst, w, m }),
            Ad::I(m) => self.cx.op(Op::LdI { dst, w, m }),
        }
    }
    fn ad_ldf(&mut self, dst: X, a: Ad) {
        match a {
            Ad::M(m) => self.cx.op(Op::LdF { dst, m }),
            Ad::I(m) => self.cx.op(Op::LdFi { dst, m }),
        }
    }
    fn ad_st(&mut self, w: W, a: Ad, src: Src) {
        match a {
            Ad::M(m) => self.cx.op(Op::St { w, m, src }),
            Ad::I(m) => self.cx.op(Op::StI { w, m, src }),
        }
    }
    fn ad_stf(&mut self, a: Ad, src: X) {
        match a {
            Ad::M(m) => self.cx.op(Op::StF { m, src }),
            Ad::I(m) => self.cx.op(Op::StFi { m, src }),
        }
    }

    /// The payload address of the element; negates the index register on the first call.
    fn elem_pay(&mut self, e: &mut Elem) -> Ad {
        if let Some(v) = e.neg.take() {
            self.cx.op(Op::Neg(v));
        }
        e.pay
    }

    /// Out-of-line slow path of the instruction at `pc` with the current register-resident set.
    pub(super) fn slow_path(&mut self, pc: u32) -> L {
        let l = self.cx.p.label();
        self.slows.push((l, pc, self.rr));
        l
    }

    /// `S0 = table pointer` of slot `t`; without a proven fact the tag byte is checked first (`slow` otherwise).
    fn tbl_ptr(&mut self, t: u32, tp: bool, slow: L) {
        if self.rr.contains(t) {
            self.refuse(format!("table slot {t} is register-resident"));
        }
        if !tp && !crate::t15::mutants::active("tbl_no_table_tag") {
            self.cx.op(Op::BrM {
                cc: Cc::Ne,
                w: W::B1,
                m: fr(t, 0),
                imm: TAG_TABLE,
                to: slow,
            });
        }
        self.cx.ld(S0, W::B8, fr(t, 8));
    }

    /// `S1 = key - 1` (the 0-based index) for a register key; the register operand is read (and loaded into its
    /// register if it is not register-resident yet) by the caller beforehand.
    fn index_to_s1(&mut self, key: TKey, kreg: Option<V>, slow: L) {
        match (key, kreg) {
            (TKey::Reg(_), Some(kr)) => {
                if kr == S1 {
                    self.cx.op(Op::Sub(S1, Src::Imm(1)));
                } else {
                    self.cx.op(Op::Lea(S1, rm(kr, -1)));
                }
            }
            (TKey::RegU(k), kr) => {
                match kr {
                    // register-resident: an integer by the class invariant of `RR`
                    Some(r) => self.cx.op(Op::MovR(S1, r)),
                    None => {
                        if !crate::t15::mutants::active("tbl_no_key_tag") {
                            self.cx.op(Op::BrM {
                                cc: Cc::Ne,
                                w: W::B1,
                                m: fr(k, 0),
                                imm: TAG_INT,
                                to: slow,
                            });
                        }
                        self.cx.ld(S1, W::B8, fr(k, 8));
                    }
                }
                self.cx.op(Op::Sub(S1, Src::Imm(1)));
            }
            _ => {}
        }
    }

    /// Operand registers that region code needs *before* the slow path is created (so that its flush set is
    /// final). Returns the register of a register key.
    fn prefetch_key(&mut self, key: TKey) -> Option<V> {
        match key {
            TKey::Reg(k) => Some(self.src_reg(k, S1)),
            TKey::RegU(k) => {
                if self.rr.contains(k) {
                    match self.asg.of(k) {
                        Some(r) => Some(r),
                        None => {
                            self.refuse(format!("key slot {k} holds a float in a register"));
                            None
                        }
                    }
                } else {
                    None
                }
            }
            TKey::Imm(_) => None,
        }
    }

    /// Locates the array element: bounds check, leaves `S2 = array base`, `S0 = table`, `S1 = index` (register
    /// key) and returns the element addresses. Exits to `slow` when the key is out of range.
    fn locate_arr(&mut self, t: u32, tp: bool, key: TKey, slow: L) -> Elem {
        match key {
            TKey::Reg(_) | TKey::RegU(_) => {
                if !crate::t15::mutants::active("tbl_no_lower_bound") {
                    self.cx.br(Cc::Lt, S1, Src::Imm(0), slow);
                }
                self.tbl_ptr(t, tp, slow);
                self.cx.ld(S2, W::B4, rm(S0, table::ASIZE as i32));
                if !crate::t15::mutants::active("tbl_no_upper_bound") {
                    self.cx.br(Cc::Ge, S1, Src::V(S2), slow);
                }
                self.cx.ld(S2, W::B8, rm(S0, table::ARRAY as i32));
                Elem {
                    tag: Ad::I(Mi {
                        base: S2,
                        idx: S1,
                        scale: 1,
                        off: table::ARRAY_TAG_BASE as i32,
                    }),
                    pay: Ad::I(Mi {
                        base: S2,
                        idx: S1,
                        scale: 8,
                        off: -(table::ARRAY_PAYLOAD_STRIDE as i32),
                    }),
                    neg: Some(S1),
                }
            }
            TKey::Imm(u) => {
                self.tbl_ptr(t, tp, slow);
                self.cx.ld(S2, W::B4, rm(S0, table::ASIZE as i32));
                if !crate::t15::mutants::active("tbl_no_upper_bound") {
                    self.cx.br(Cc::Le, S2, Src::Imm(u as i32), slow);
                }
                self.cx.ld(S2, W::B8, rm(S0, table::ARRAY as i32));
                Elem {
                    tag: Ad::M(rm(S2, table::ARRAY_TAG_BASE as i32 + u as i32)),
                    pay: Ad::M(rm(
                        S2,
                        -((table::ARRAY_PAYLOAD_STRIDE as i32)
                            * (u as i32
                                + i32::from(!crate::t15::mutants::active("tbl_const_off")))),
                    )),
                    neg: None,
                }
            }
        }
    }

    /// Looks the constant short string `K[kx]` up in the table of slot `t`: leaves `S1 = node`, exits to `slow`
    /// when the key is absent. Clobbers `S0 S2 S3`. The walk of tier 1's `node_lookup`.
    fn locate_fld(&mut self, t: u32, tp: bool, kx: u32, slow: L) -> Elem {
        let hash = self.cx.cfg.consts[kx as usize]
            .as_string()
            .expect("short string constant")
            .hash();
        let (lp, nx, found) = (self.cx.p.label(), self.cx.p.label(), self.cx.p.label());
        self.tbl_ptr(t, tp, slow);
        self.cx.ld(S2, W::B1, rm(S0, table::LSIZENODE as i32));
        self.cx.op(Op::Mov(S1, 1));
        self.cx.op(Op::ShlV(S1)); // count in V6 = S2
        self.cx.op(Op::Sub(S1, Src::Imm(1)));
        self.cx.op(Op::Mov(S3, u64::from(hash)));
        self.cx.op(Op::And(S1, Src::V(S3)));
        self.cx.op(Op::Mul(S1, Src::Imm(NODE_SIZE)));
        self.cx.ld(S2, W::B8, rm(S0, table::NODE as i32));
        self.cx.op(Op::Add(S1, Src::V(S2)));
        self.cx.ld(
            S3,
            W::B8,
            M {
                base: Base::K,
                off: kx as i32 * 16 + 8,
            },
        );
        self.cx.op(Op::Bind(lp));
        self.cx.op(Op::BrM {
            cc: Cc::Ne,
            w: W::B1,
            m: rm(S1, node::KEY_TT as i32),
            imm: TAG_SHRSTR,
            to: nx,
        });
        self.cx.ld(S2, W::B8, rm(S1, node::KEY as i32));
        self.cx.br(Cc::Eq, S2, Src::V(S3), found);
        self.cx.op(Op::Bind(nx));
        self.cx.ld(S2, W::B4, rm(S1, node::NEXT as i32));
        self.cx.br(Cc::Eq, S2, Src::Imm(0), slow);
        // sign-extend the 32-bit chain offset (in nodes)
        if !crate::t15::mutants::active("tbl_next_zext") {
            self.cx.op(Op::Mov(S0, 0x8000_0000));
            self.cx.op(Op::Xor(S2, Src::V(S0)));
            self.cx.op(Op::Sub(S2, Src::V(S0)));
        }
        self.cx.op(Op::Mul(S2, Src::Imm(NODE_SIZE)));
        self.cx.op(Op::Add(S1, Src::V(S2)));
        self.cx.op(Op::Jmp(lp));
        self.cx.op(Op::Bind(found));
        Elem {
            tag: Ad::M(rm(S1, node::VAL_TT as i32)),
            pay: Ad::M(rm(S1, node::VAL as i32)),
            neg: None,
        }
    }

    // ---- GET -----------------------------------------------------------------------------------

    /// Commits the element `e` to slot `a` (checks first, stores last).
    fn commit_get(&mut self, a: u32, mut e: Elem, res: TRes, slow: L) {
        match res {
            TRes::Int => {
                if !crate::t15::mutants::active("tbl_no_tag_check") {
                    self.ad_br(Cc::Ne, W::B1, e.tag, TAG_INT, slow);
                }
                let d = self.def_reg(a);
                let p = self.elem_pay(&mut e);
                self.ad_ld(d, W::B8, p);
                self.commit_int(a, d);
            }
            TRes::Flt => {
                if !crate::t15::mutants::active("tbl_no_tag_check") {
                    self.ad_br(Cc::Ne, W::B1, e.tag, TAG_FLT, slow);
                }
                let d = self.def_x(a);
                let p = self.elem_pay(&mut e);
                self.ad_ldf(d, p);
                self.commit_flt(a, d);
            }
            TRes::Mem(Some(tag)) => {
                self.ad_br(Cc::Ne, W::B1, e.tag, i32::from(tag), slow);
                let p = self.elem_pay(&mut e);
                self.ad_ld(S3, W::B8, p);
                self.cx.st_imm(W::B1, fr(a, 0), i32::from(tag));
                self.cx.st(W::B8, fr(a, 8), S3);
                self.mem_def(a);
            }
            TRes::Mem(None) => {
                self.ad_ld(S0, W::B1, e.tag);
                self.cx.op(Op::MovR(S3, S0));
                self.cx.op(Op::And(S3, Src::Imm(0x0F)));
                if !crate::t15::mutants::active("tbl_no_nil_check") {
                    self.cx.br(Cc::Eq, S3, Src::Imm(0), slow);
                }
                let p = self.elem_pay(&mut e);
                self.ad_ld(S3, W::B8, p);
                self.cx.st(W::B1, fr(a, 0), S0);
                self.cx.st(W::B8, fr(a, 8), S3);
                self.mem_def(a);
            }
        }
    }

    pub(super) fn emit_get_arr(&mut self, pc: u32, a: u32, t: (u32, bool), key: TKey, res: TRes) {
        let kreg = self.prefetch_key(key);
        let slow = self.slow_path(pc);
        self.index_to_s1(key, kreg, slow);
        let e = self.locate_arr(t.0, t.1, key, slow);
        self.commit_get(a, e, res, slow);
        self.end_straight(pc);
    }

    pub(super) fn emit_get_fld(&mut self, pc: u32, a: u32, t: (u32, bool), kx: u32, res: TRes) {
        let slow = self.slow_path(pc);
        let e = self.locate_fld(t.0, t.1, kx, slow);
        self.commit_get(a, e, res, slow);
        self.end_straight(pc);
    }

    // ---- SET -----------------------------------------------------------------------------------

    /// The value operand registers, read before the slow path is created.
    fn prefetch_val(&mut self, val: TVal) -> (Option<V>, Option<X>) {
        match val {
            TVal::Int(v) if self.asg.of(v).is_some() => (Some(self.src_reg(v, S3)), None),
            TVal::Flt(v) if self.asg.xof(v).is_some() => (None, Some(self.fx(FOpd::Flt(v), 0))),
            TVal::Slot(v, _) => {
                self.spill(v);
                (None, None)
            }
            _ => (None, None),
        }
    }

    /// Memory operands (tag, payload) and collectability of a value that is copied from memory.
    fn val_mem(&self, val: TVal) -> Option<(M, M, Coll)> {
        match val {
            TVal::Slot(v, c) => Some((fr(v, 0), fr(v, 8), c)),
            TVal::K(k, c) => Some((
                M {
                    base: Base::K,
                    off: k as i32 * 16,
                },
                M {
                    base: Base::K,
                    off: k as i32 * 16 + 8,
                },
                c,
            )),
            _ => None,
        }
    }

    /// `if barrier_back(t, v) would do anything -> slow` (tier 1's `barrier_exit`): the table (`S0`) is black and
    /// the value is collectable and white. Clobbers `S3`.
    fn barrier_check(&mut self, val: TVal, slow: L) {
        let Some((tag, pay, coll)) = self.val_mem(val) else {
            return; // INT / FLT: not collectable
        };
        if coll == Coll::No || crate::t15::mutants::active("tbl_no_barrier") {
            return;
        }
        let ok = self.cx.p.label();
        self.cx.ld(S3, W::B1, rm(S0, table::MARKED as i32));
        self.cx.op(Op::And(S3, Src::Imm(i32::from(BLACKBIT))));
        self.cx.br(Cc::Eq, S3, Src::Imm(0), ok);
        if coll == Coll::Maybe && !crate::t15::mutants::active("tbl_no_coll_test") {
            self.cx.ld(S3, W::B1, tag);
            self.cx
                .op(Op::And(S3, Src::Imm(i32::from(BIT_ISCOLLECTABLE))));
            self.cx.br(Cc::Eq, S3, Src::Imm(0), ok);
        }
        self.cx.ld(S3, W::B8, pay);
        self.cx.ld(S3, W::B1, rm(S3, table::MARKED as i32)); // `GcHeader::marked` (same offset in every object)
        self.cx.op(Op::And(S3, Src::Imm(i32::from(WHITEBITS))));
        self.cx.br(Cc::Ne, S3, Src::Imm(0), slow);
        self.cx.op(Op::Bind(ok));
    }

    /// Stores the value into the element (tag byte first, then the payload).
    fn store_val(&mut self, e: &mut Elem, val: TVal, regs: (Option<V>, Option<X>)) {
        match val {
            TVal::Int(v) => {
                self.ad_st(W::B1, e.tag, Src::Imm(TAG_INT));
                let p = self.elem_pay(e);
                match regs.0 {
                    Some(r) => self.ad_st(W::B8, p, Src::V(r)),
                    None => {
                        self.cx.ld(S3, W::B8, fr(v, 8));
                        self.ad_st(W::B8, p, Src::V(S3));
                    }
                }
            }
            TVal::Flt(v) => {
                self.ad_st(W::B1, e.tag, Src::Imm(TAG_FLT));
                let p = self.elem_pay(e);
                match regs.1 {
                    Some(x) => self.ad_stf(p, x),
                    None => {
                        self.cx.ld(S3, W::B8, fr(v, 8));
                        self.ad_st(W::B8, p, Src::V(S3));
                    }
                }
            }
            TVal::Slot(..) | TVal::K(..) => {
                let (tag, pay, _) = self.val_mem(val).expect("memory value");
                self.cx.ld(S3, W::B1, tag);
                self.ad_st(W::B1, e.tag, Src::V(S3));
                self.cx.ld(S3, W::B8, pay);
                let p = self.elem_pay(e);
                self.ad_st(W::B8, p, Src::V(S3));
            }
        }
    }

    pub(super) fn emit_set_arr(&mut self, pc: u32, t: (u32, bool), key: TKey, val: TVal) {
        let kreg = self.prefetch_key(key);
        let regs = self.prefetch_val(val);
        let slow = self.slow_path(pc);
        self.index_to_s1(key, kreg, slow);
        let mut e = self.locate_arr(t.0, t.1, key, slow);
        // an empty slot of a table with a metatable may have a `__newindex`: the slow path
        let ok = self.cx.p.label();
        self.ad_ld(S3, W::B1, e.tag);
        self.cx.op(Op::And(S3, Src::Imm(0x0F)));
        self.cx.br(Cc::Ne, S3, Src::Imm(0), ok);
        if !crate::t15::mutants::active("tbl_no_meta_check") {
            self.cx.ld(S3, W::B8, rm(S0, table::METATABLE as i32));
            self.cx.br(Cc::Ne, S3, Src::Imm(0), slow);
        }
        self.cx.op(Op::Bind(ok));
        self.barrier_check(val, slow);
        self.store_val(&mut e, val, regs);
        self.end_straight(pc);
    }

    pub(super) fn emit_set_fld(&mut self, pc: u32, t: (u32, bool), kx: u32, val: TVal) {
        let regs = self.prefetch_val(val);
        let slow = self.slow_path(pc);
        let mut e = self.locate_fld(t.0, t.1, kx, slow);
        // only an existing non-nil value: no `__newindex`, no insertion
        self.ad_ld(S3, W::B1, e.tag);
        self.cx.op(Op::And(S3, Src::Imm(0x0F)));
        if !crate::t15::mutants::active("tbl_no_nonnil_check") {
            self.cx.br(Cc::Eq, S3, Src::Imm(0), slow);
        }
        if self.val_mem(val).is_some_and(|(_, _, c)| c != Coll::No) {
            if !crate::t15::mutants::active("tbl_no_s0_reload") {
                self.tbl_ptr(t.0, true, slow); // the chain walk used S0 as scratch (checked above)
            }
            self.barrier_check(val, slow);
        }
        self.store_val(&mut e, val, regs);
        self.end_straight(pc);
    }

    /// Dispatch from `emit_act`.
    pub(super) fn emit_table_act(&mut self, pc: u32, act: Act) {
        match act {
            Act::GetArr { a, t, tp, key, res } => self.emit_get_arr(pc, a, (t, tp), key, res),
            Act::GetFld { a, t, tp, kx, res } => self.emit_get_fld(pc, a, (t, tp), kx, res),
            Act::SetArr { t, tp, key, val } => self.emit_set_arr(pc, (t, tp), key, val),
            Act::SetFld { t, tp, kx, val } => self.emit_set_fld(pc, (t, tp), kx, val),
            _ => unreachable!("not a table act"),
        }
    }

    /// Emits the collected slow paths (called by `finish`).
    pub(in crate::t15) fn emit_slow_paths(&mut self) {
        let mut i = 0;
        while i < self.slows.len() {
            let (l, pc, rr) = self.slows[i];
            i += 1;
            self.cx.op(Op::Bind(l));
            self.rr = rr;
            if self.fb.is_some()
                && (self.slow_counted.contains(&l)
                    || super::super::mutants::active("pol_count_all_slow"))
                && matches!(
                    self.acts[(pc - self.plan.head) as usize],
                    Act::Intr { .. } | Act::IntrH { .. }
                )
            {
                self.emit_intr_slow_count(pc);
            }
            self.emit_fall(pc);
        }
    }
}
