//! Table templates (J1d): GETI, GETTABLE, GETFIELD, GETTABUP, SELF and SETI, SETTABLE, SETFIELD, SETTABUP.
//!
//! Only the pure inline fast paths exist; every miss exits at the instruction *before any side effect*, so
//! the interpreter redoes it with metamethods, insertion, rehash, allocation and errors:
//!
//! * reads hit only when the value is **non-nil** (a nil result may need `__index`): an integer key inside
//!   the array part (`u = k - 1 < asize`, 5.5 has no `alimit` hint to maintain), or a short-string key found
//!   by pointer equality while walking the main-position chain (the constant's hash is read at compile time
//!   from the constant table; the pointer itself comes from `kcache` at run time, G1).
//! * writes mirror `pset_existing_int_arr` (array slot that is non-empty, or any slot of a table without
//!   metatable) and `pset_existing_shortstr_slot` (existing node with a non-nil value: no `__newindex`
//!   lookup, no insertion, no `invalidate_tm_cache`).
//! * the GC barrier: the interpreter calls `barrier_back(t, &value)` after the store, which does something only
//!   if the value is collectable, the table is black and the value is white (`barrier_back_slow`, with its
//!   gray-again / touched ageing logic, is never needed otherwise). The templates evaluate exactly that
//!   condition *before* the store and exit when it holds, so the generational and incremental barrier work
//!   stays entirely in the interpreter and nothing is duplicated in machine code.

use crate::ir::{Base, Cc, Op, Src, L, M, V, W};
use crate::lower::{fr, Cx, Kind, Ty};
use crate::templates_alloc::Cold;
use rn_core::gc::{BLACKBIT, WHITEBITS};
use rn_core::instruction::Instruction;
use rn_core::jit_layout::{node, string, table};
use rn_core::meta::{Tm, MASKFLAGS};
use rn_core::value::{ctb, Value, BIT_ISCOLLECTABLE, LUA_VNIL, LUA_VNUMINT, LUA_VSHRSTR, LUA_VTABLE};
use rn_core::ver::active::OpCode;

const TAG_INT: i32 = LUA_VNUMINT as i32;
const TAG_TABLE: i32 = ctb(LUA_VTABLE) as i32;
const TAG_SHRSTR: i32 = ctb(LUA_VSHRSTR) as i32;
const NODE_SIZE: i32 = node::SIZE as i32;

/// Bits of `tr::nilset_mutants()` (test-only code mutations of the `nilset` stores).
pub(crate) const NS_FLAGS_SKIP: u8 = 1;
pub(crate) const NS_MT_SKIP: u8 = 2;
pub(crate) const NS_BARRIER_SKIP: u8 = 4;
pub(crate) const NS_ARR_MT_SKIP: u8 = 16;
pub(crate) const NS_STR_TAG_SKIP: u8 = 8;
pub(crate) const NS_INT_KEY_SKIP: u8 = 32;
/// BUNDLE-G1 mutants: the inline integer-key read treats a nil-valued node as a hit / sends keys of any type to the hash probe.
pub(crate) const NS_GET_NIL_HIT: u8 = 64;
pub(crate) const NS_GET_TAG_SKIP: u8 = 128;

/// Classification of the opcodes this file implements (`None` = not ours).
pub(crate) fn kind(op: OpCode, i: Instruction, k: &[Value]) -> Option<Kind> {
    use OpCode as O;
    let kshr = |idx: u32| {
        k.get(idx as usize)
            .is_some_and(|v| v.tag() == ctb(LUA_VSHRSTR))
    };
    // the value operand of a store: a register, or a constant that must exist
    let kval = |i: Instruction| i.k() == 0 || (i.c() as usize) < k.len();
    let ok = match op {
        O::Geti => true,
        O::Gettable => true,
        O::Getfield | O::Gettabup => kshr(i.c()),
        // 5.4: `SELF` has an `RKC` key (a register when `k` is clear): only constant short strings are compiled
        O::SelfOp => (!rn_core::jit_layout::LUA54 || i.k() != 0) && kshr(i.c()),
        O::Seti => kval(i),
        O::Settable => kval(i),
        O::Setfield | O::Settabup => kshr(i.b()) && kval(i),
        _ => return None,
    };
    ok.then_some(Kind::Guard)
}

/// Where the key of an array access comes from.
#[derive(Clone, Copy)]
enum Idx {
    /// Constant 0-based index (`GETI`/`SETI` with a constant `>= 1`).
    Const(i32),
    /// Register holding the key.
    Reg(u32),
}

/// Whether the stored value can be collectable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Coll {
    No,
    Yes,
    Maybe,
}

/// A value operand of a store: memory of its tag byte and payload.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Vop {
    tag: M,
    pay: M,
    coll: Coll,
}

fn r0(off: usize) -> M {
    M {
        base: Base::Reg(0),
        off: off as i32,
    }
}
fn rm(v: u8, off: i32) -> M {
    M {
        base: Base::Reg(v),
        off,
    }
}

impl Cx<'_> {
    pub(crate) fn t_table(&mut self, pc: u32, i: Instruction, op: OpCode) {
        use OpCode as O;
        let (a, b, c) = (i.a(), i.b(), i.c());
        // a miss of the inline probe: out-of-line helper (raw miss -> nil / `__index` tables / insertion);
        // without calls (J1a protocol) it is an exit at the instruction
        let x = if self.calls_on() {
            self.slow_label(pc, i, op)
        } else {
            self.exit_r(pc)
        };
        match op {
            O::Geti if c == 0 => {
                // constant key 0 is never in the array part: straight to the helper (was: left tier 1 at the instruction)
                self.op(Op::Jmp(x));
                self.ty[a as usize] = Ty::Unk;
            }
            O::Geti => {
                self.tbl_reg(b, x);
                self.arr_slot(Idx::Const(c as i32 - 1), x);
                self.get_arr_result(a, x);
            }
            O::Gettable => self.t_gettable(a, b, c, x),
            O::Getfield => {
                self.tbl_reg(b, x);
                let (m, back, fin) = self.chain_labels(c, x, a, u32::MAX);
                self.str_lookup_const(c, m);
                self.get_node_result(a, m, back);
                if let Some(fin) = fin {
                    self.op(Op::Bind(fin));
                }
            }
            O::Gettabup => {
                self.tbl_upval(b, x);
                let (m, back, fin) = self.chain_labels(c, x, a, u32::MAX);
                self.str_lookup_const(c, m);
                self.get_node_result(a, m, back);
                if let Some(fin) = fin {
                    self.op(Op::Bind(fin));
                }
            }
            O::SelfOp => {
                // BUNDLE-K item B: a string receiver is probed out of line
                let sdone = (self.calls_on() && self.env().str_mt != 0).then(|| (self.p.label(), self.p.label()));
                let sx = match sdone {
                    Some((l, done)) => {
                        self.cold.push(Cold::StrSelf { l, x, done, kidx: c, a, b });
                        l
                    }
                    None => x,
                };
                self.tbl_reg(b, sx);
                let (m, back, fin) = self.chain_labels(c, x, a, b);
                self.str_lookup_const(c, m);
                // pure so far: V2 = node. Check it holds a value, load it, then do the two stores
                self.ld(4, W::B1, rm(2, node::VAL_TT as i32));
                self.op(Op::MovR(5, 4));
                self.op(Op::And(5, Src::Imm(0x0F)));
                self.br(Cc::Eq, 5, Src::Imm(0), m);
                if let Some(back) = back {
                    self.op(Op::Bind(back));
                }
                self.ld(5, W::B8, rm(2, node::VAL as i32));
                // R[A+1] = R[B] first (B may equal A), then R[A] = method
                self.ld(0, W::B1, fr(b, 0));
                self.ld(1, W::B8, fr(b, 8));
                self.st(W::B1, fr(a + 1, 0), 0);
                self.st(W::B8, fr(a + 1, 8), 1);
                self.st(W::B1, fr(a, 0), 4);
                self.st(W::B8, fr(a, 8), 5);
                self.ty[a as usize] = Ty::Unk;
                self.ty[a as usize + 1] = Ty::Unk;
                if let Some(fin) = fin {
                    self.op(Op::Bind(fin));
                }
                if let Some((_, done)) = sdone {
                    self.op(Op::Bind(done));
                }
            }
            O::Seti if b == 0 => self.op(Op::Jmp(x)),
            O::Seti => {
                let v = self.vop(i);
                self.tbl_reg(a, x);
                self.arr_slot(Idx::Const(b as i32 - 1), x);
                self.set_arr(v, x);
            }
            O::Settable => {
                let v = self.vop(i);
                self.tbl_reg(a, x);
                let tset = if self.calls_on() { self.env().tset } else { 0 };
                let (strk, hashk) = (tset & 1 != 0, tset & 2 != 0);
                if self.ty[b as usize] == Ty::Int && !hashk {
                    self.arr_slot(Idx::Reg(b), x);
                    self.set_arr(v, x);
                } else if !strk && !hashk {
                    self.arr_slot(Idx::Reg(b), x);
                    self.set_arr(v, x);
                } else {
                    // BUNDLE-B item 2: a short-string key or an integer key in the hash part stores into an existing non-nil node like SETFIELD
                    let (lstr, lhash, done) = (self.p.label(), self.p.label(), self.p.label());
                    if self.ty[b as usize] != Ty::Int {
                        self.ld(2, W::B1, fr(b, 0));
                        self.br(Cc::Ne, 2, Src::Imm(TAG_INT), if strk { lstr } else { x });
                    }
                    self.ld(2, W::B8, fr(b, 8));
                    self.arr_slot_reg2(if hashk { lhash } else { x });
                    self.set_arr(v, x);
                    self.op(Op::Jmp(done));
                    if hashk {
                        self.op(Op::Bind(lhash));
                        self.int_node_lookup(b, x);
                        self.set_node(v, x);
                        self.op(Op::Jmp(done));
                    }
                    if strk && self.ty[b as usize] != Ty::Int {
                        self.op(Op::Bind(lstr));
                        if !self.nilset_mut(NS_STR_TAG_SKIP) {
                            self.br(Cc::Ne, 2, Src::Imm(TAG_SHRSTR), x);
                        }
                        self.ld(1, W::B8, fr(b, 8));
                        self.ld(4, W::B1, rm(1, string::EXTRA as i32));
                        self.br(Cc::Eq, 4, Src::Imm(0), x); // (hash-D: a string without its PUC hash is not a key anywhere: the slow path answers)
                        self.ld(3, W::B4, rm(1, string::SHORT_HASH as i32));
                        self.node_lookup(x);
                        self.set_node(v, x);
                    }
                    self.op(Op::Bind(done));
                }
            }
            O::Setfield => {
                let v = self.vop(i);
                self.tbl_reg(a, x);
                self.str_lookup_const(b, x);
                self.set_node(v, x);
            }
            O::Settabup => {
                let v = self.vop(i);
                self.tbl_upval(a, x);
                self.str_lookup_const(b, x);
                self.set_node(v, x);
            }
            _ => unreachable!("not a table template: {op:?}"),
        }
        if self.calls_on() && matches!(op, O::Seti | O::Settable | O::Setfield | O::Settabup) {
            // the slow path can run a GC step (finalizers are Lua code): forget the register types
            self.ty = [Ty::Unk; 256];
        }
    }

    // ---- operands ------------------------------------------------------------------------------

    /// The value operand `RK(C)` of a store.
    fn vop(&self, i: Instruction) -> Vop {
        let c = i.c();
        if i.k() != 0 {
            let t = self.cfg.consts[c as usize].tag();
            Vop {
                tag: M {
                    base: Base::K,
                    off: c as i32 * 16,
                },
                pay: M {
                    base: Base::K,
                    off: c as i32 * 16 + 8,
                },
                coll: if t & BIT_ISCOLLECTABLE != 0 {
                    Coll::Yes
                } else {
                    Coll::No
                },
            }
        } else {
            Vop {
                tag: fr(c, 0),
                pay: fr(c, 8),
                coll: match self.ty[c as usize] {
                    Ty::Int | Ty::Flt => Coll::No,
                    Ty::Unk => Coll::Maybe,
                },
            }
        }
    }

    /// `V0 = table pointer of register r`, exit `x` if it is not a table.
    fn tbl_reg(&mut self, r: u32, x: L) {
        self.ld(0, W::B1, fr(r, 0));
        self.br(Cc::Ne, 0, Src::Imm(TAG_TABLE), x);
        self.ld(0, W::B8, fr(r, 8));
    }

    /// `V0 = table pointer held by upvalue b`, exit `x` if it is not a table. Clobbers V1.
    fn tbl_upval(&mut self, b: u32, x: L) {
        self.upval_ptr(b);
        self.ld(1, W::B1, r0(0));
        self.br(Cc::Ne, 1, Src::Imm(TAG_TABLE), x);
        self.ld(0, W::B8, r0(8));
    }

    // ---- array part ----------------------------------------------------------------------------

    /// Bounds check of an array access on the table in `V0`; leaves `V1` = tag base (slot tag at `[V1 + 4]`)
    /// and `V3` = payload base (slot payload at `[V3 - 8]`). Exits `x` when the index is outside `[1, asize]`
    /// or (register key) not an integer. Clobbers V2.
    fn arr_slot(&mut self, idx: Idx, x: L) {
        match idx {
            Idx::Const(u) => {
                self.ld(3, W::B4, rm(0, table::ASIZE as i32));
                self.br(Cc::Le, 3, Src::Imm(u), x);
                self.ld(1, W::B8, rm(0, table::ARRAY as i32));
                if rn_core::jit_layout::ARRAY_ALIMIT_TOUCH {
                    self.op(Op::Mov(2, u as u64));
                    self.alimit_touch(0, 1, 2, 3, 4);
                }
                self.op(Op::MovR(3, 1));
                self.op(Op::Add(1, Src::Imm(u)));
                self.op(Op::Sub(3, Src::Imm(8 * u)));
            }
            Idx::Reg(r) => {
                if self.ty[r as usize] != Ty::Int {
                    self.ld(2, W::B1, fr(r, 0));
                    self.br(Cc::Ne, 2, Src::Imm(TAG_INT), x);
                }
                self.ld(2, W::B8, fr(r, 8));
                self.arr_slot_reg2(x);
            }
        }
    }

    /// Integer key payload in `V2`: the shared tail of the register-key array access.
    fn arr_slot_reg2(&mut self, x: L) {
        self.op(Op::Sub(2, Src::Imm(1)));
        self.br(Cc::Lt, 2, Src::Imm(0), x); // k < 1 (also k - 1 wrapping for i64::MIN)
        self.ld(3, W::B4, rm(0, table::ASIZE as i32));
        self.br(Cc::Ge, 2, Src::V(3), x);
        self.ld(1, W::B8, rm(0, table::ARRAY as i32));
        if rn_core::jit_layout::ARRAY_ALIMIT_TOUCH {
            self.alimit_touch(0, 1, 2, 3, 4);
        }
        self.op(Op::MovR(3, 1));
        self.op(Op::Add(1, Src::V(2)));
        self.op(Op::ShlI(2, 3));
        self.op(Op::Sub(3, Src::V(2)));
    }

    /// 5.4 `alimit_touch` (`table.rs`), emitted for every integer access to array index `u` (0-based register
    /// `u`, already known to be `< asize`) of the table in register `tbl`, whose array pointer is in `arr`:
    /// if the table has `BITRAS` set (the array limit is a hint in the `u32` at `array + 0`) and
    /// `u >= lim && (u & !(lim - 1)) < lim`, the new limit is `u + 1`. Exactly the interpreter's update, so a
    /// later `#t` gives the same answer. Clobbers `s1`, `s2`; preserves `tbl`, `arr`, `u`.
    pub(crate) fn alimit_touch(&mut self, tbl: V, arr: V, u: V, s1: V, s2: V) {
        #[cfg(rustynail_lua54)]
        {
            let ok = self.p.label();
            self.ld(s1, W::B1, rm(tbl, table::FLAGS as i32));
            self.op(Op::And(s1, Src::Imm(i32::from(table::BITRAS))));
            self.br(Cc::Eq, s1, Src::Imm(0), ok);
            self.ld(s1, W::B4, rm(arr, table::LENHINT as i32));
            self.br(Cc::Gt, s1, Src::V(u), ok); // u < lim: nothing to update
            self.op(Op::MovR(s2, s1));
            self.op(Op::Sub(s2, Src::Imm(1)));
            self.op(Op::Not(s2));
            self.op(Op::And(s2, Src::V(u)));
            self.br(Cc::Ge, s2, Src::V(s1), ok);
            self.op(Op::MovR(s2, u));
            self.op(Op::Add(s2, Src::Imm(1)));
            self.st(W::B4, rm(arr, table::LENHINT as i32), s2);
            self.op(Op::Bind(ok));
        }
        #[cfg(not(rustynail_lua54))]
        let _ = (tbl, arr, u, s1, s2);
    }

    /// Reads the array slot at (`V1`, `V3`): exit `x` if nil, else store it to `R[a]`.
    fn get_arr_result(&mut self, a: u32, x: L) {
        self.ld(4, W::B1, rm(1, table::ARRAY_TAG_BASE as i32));
        self.op(Op::MovR(5, 4));
        self.op(Op::And(5, Src::Imm(0x0F)));
        self.br(Cc::Eq, 5, Src::Imm(0), x);
        self.ld(5, W::B8, rm(3, -(table::ARRAY_PAYLOAD_STRIDE as i32)));
        self.st(W::B1, fr(a, 0), 4);
        self.st(W::B8, fr(a, 8), 5);
        self.ty[a as usize] = Ty::Unk;
    }

    /// Store to the array slot at (`V1`, `V3`) with the interpreter's conditions.
    fn set_arr(&mut self, v: Vop, x: L) {
        let ok = self.p.label();
        self.ld(4, W::B1, rm(1, table::ARRAY_TAG_BASE as i32));
        self.op(Op::And(4, Src::Imm(0x0F)));
        self.br(Cc::Ne, 4, Src::Imm(0), ok); // non-empty slot: no __newindex
        self.ld(4, W::B8, rm(0, table::METATABLE as i32));
        if self.nilset_on() {
            // BUNDLE-A item 2: an empty slot of a table whose metatable is known to have no `__newindex` (`checknoTM`: the flag bit
            // is set once the absence was looked up) is a plain store, like `psetint`
            self.br(Cc::Eq, 4, Src::Imm(0), ok);
            if !self.nilset_mut(NS_ARR_MT_SKIP) {
                self.ld(4, W::B1, rm(4, table::FLAGS as i32));
                self.op(Op::And(4, Src::Imm(1 << Tm::NewIndex as i32)));
                self.br(Cc::Eq, 4, Src::Imm(0), x);
            }
        } else {
            self.br(Cc::Ne, 4, Src::Imm(0), x); // empty slot of a table with a metatable
        }
        self.op(Op::Bind(ok));
        self.barrier_exit(v, x);
        self.ld(4, W::B1, v.tag);
        self.ld(5, W::B8, v.pay);
        self.st(W::B1, rm(1, table::ARRAY_TAG_BASE as i32), 4);
        self.st(W::B8, rm(3, -(table::ARRAY_PAYLOAD_STRIDE as i32)), 5);
    }

    /// BUNDLE-A item 2 (`nilset`): do the tier-1 stores handle an existing key with a nil value and an empty array slot of a table with a
    /// metatable inline (needs the out-of-line calls environment: the knob lives in [`CallEnv`])?
    fn nilset_on(&self) -> bool {
        self.calls_on() && self.env().tset & 4 != 0
    }

    /// The active `nilset` mutant `bit` (test-only; always false without the `tr_mutants` feature).
    fn nilset_mut(&self, bit: u8) -> bool {
        #[cfg(feature = "trace")]
        {
            crate::tr::nilset_mutants() & bit != 0
        }
        #[cfg(not(feature = "trace"))]
        {
            let _ = bit;
            false
        }
    }

    /// `if barrier_back(t, v) would do anything -> exit x`: table (`V0`) black, value collectable and white.
    /// Clobbers V4, V5.
    fn barrier_exit(&mut self, v: Vop, x: L) {
        if v.coll == Coll::No {
            return;
        }
        let ok = self.p.label();
        self.ld(5, W::B1, rm(0, table::MARKED as i32));
        self.op(Op::And(5, Src::Imm(i32::from(BLACKBIT))));
        self.br(Cc::Eq, 5, Src::Imm(0), ok);
        if v.coll == Coll::Maybe {
            self.ld(4, W::B1, v.tag);
            self.op(Op::And(4, Src::Imm(i32::from(BIT_ISCOLLECTABLE))));
            self.br(Cc::Eq, 4, Src::Imm(0), ok);
        }
        self.ld(4, W::B8, v.pay);
        self.ld(4, W::B1, rm(4, table::MARKED as i32)); // `GcHeader::marked` (same offset in every object)
        self.op(Op::And(4, Src::Imm(i32::from(WHITEBITS))));
        self.br(Cc::Ne, 4, Src::Imm(0), x);
        self.op(Op::Bind(ok));
    }

    // ---- hash part -----------------------------------------------------------------------------

    /// Looks the constant short string `K[kidx]` up in the table in `V0`: falls through with `V2` = the node
    /// holding the key, exits `x` if the key is absent. Clobbers V1, V3, V4, V5, V6.
    fn str_lookup_const(&mut self, kidx: u32, x: L) {
        let hash = self.cfg.consts[kidx as usize]
            .as_string()
            .expect("short string constant")
            .hash();
        self.ld(
            1,
            W::B8,
            M {
                base: Base::K,
                off: kidx as i32 * 16 + 8,
            },
        );
        self.op(Op::Mov(3, u64::from(hash)));
        self.node_lookup(x);
    }

    /// `V1` = key pointer (interned short string), `V3` = its hash, `V0` = table. Walk of `luaH_getshortstr`.
    fn node_lookup(&mut self, miss: L) {
        let (lp, nx, found) = (self.p.label(), self.p.label(), self.p.label());
        self.ld(6, W::B1, rm(0, table::LSIZENODE as i32));
        self.op(Op::Mov(2, 1));
        self.op(Op::ShlV(2));
        self.op(Op::Sub(2, Src::Imm(1)));
        self.op(Op::And(2, Src::V(3)));
        self.op(Op::Mul(2, Src::Imm(NODE_SIZE)));
        self.ld(4, W::B8, rm(0, table::NODE as i32));
        self.op(Op::Add(2, Src::V(4)));
        self.op(Op::Bind(lp));
        self.ld(4, W::B1, rm(2, node::KEY_TT as i32));
        self.br(Cc::Ne, 4, Src::Imm(TAG_SHRSTR), nx);
        self.ld(4, W::B8, rm(2, node::KEY as i32));
        self.br(Cc::Eq, 4, Src::V(1), found);
        self.op(Op::Bind(nx));
        self.ld(4, W::B4, rm(2, node::NEXT as i32));
        self.br(Cc::Eq, 4, Src::Imm(0), miss);
        // sign-extend the 32-bit chain offset
        self.op(Op::Mov(5, 0x8000_0000));
        self.op(Op::Xor(4, Src::V(5)));
        self.op(Op::Sub(4, Src::V(5)));
        self.op(Op::Mul(4, Src::Imm(NODE_SIZE)));
        self.op(Op::Add(2, Src::V(4)));
        self.op(Op::Jmp(lp));
        self.op(Op::Bind(found));
    }

    /// BUNDLE-B item 2: `getintfromhash` for the integer key in register `r` on the table in `V0`: falls through with `V2` = the node holding the key,
    /// exits `miss` if it is absent. The main position is `key % ((sizenode - 1) | 1)` (unsigned, as `hashint`). Clobbers V1..V6 (and V7, V8 through `UDiv`).
    fn int_node_lookup(&mut self, r: u32, miss: L) {
        let (lp, nx, found) = (self.p.label(), self.p.label(), self.p.label());
        self.ld(1, W::B8, fr(r, 8));
        self.ld(6, W::B1, rm(0, table::LSIZENODE as i32));
        self.op(Op::Mov(3, 1));
        self.op(Op::ShlV(3));
        self.op(Op::Sub(3, Src::Imm(1)));
        self.op(Op::Or(3, Src::Imm(1)));
        self.op(Op::MovR(4, 1));
        self.op(Op::UDiv { dst: 4, a: 4, b: 3 });
        self.op(Op::Mul(4, Src::V(3)));
        self.op(Op::MovR(2, 1));
        self.op(Op::Sub(2, Src::V(4)));
        self.op(Op::Mul(2, Src::Imm(NODE_SIZE)));
        self.ld(4, W::B8, rm(0, table::NODE as i32));
        self.op(Op::Add(2, Src::V(4)));
        self.op(Op::Bind(lp));
        self.ld(4, W::B1, rm(2, node::KEY_TT as i32));
        self.br(Cc::Ne, 4, Src::Imm(TAG_INT), nx);
        if self.nilset_mut(NS_INT_KEY_SKIP) {
            self.op(Op::Jmp(found));
        }
        self.ld(4, W::B8, rm(2, node::KEY as i32));
        self.br(Cc::Eq, 4, Src::V(1), found);
        self.op(Op::Bind(nx));
        self.ld(4, W::B4, rm(2, node::NEXT as i32));
        self.br(Cc::Eq, 4, Src::Imm(0), miss);
        // sign-extend the 32-bit chain offset
        self.op(Op::Mov(5, 0x8000_0000));
        self.op(Op::Xor(4, Src::V(5)));
        self.op(Op::Sub(4, Src::V(5)));
        self.op(Op::Mul(4, Src::Imm(NODE_SIZE)));
        self.op(Op::Add(2, Src::V(4)));
        self.op(Op::Jmp(lp));
        self.op(Op::Bind(found));
    }

    /// T3: the labels of the receiver miss of a constant-key read: `(miss, back, fin)`. Without the inline `__index` level (`CallEnv::idx_name
    /// == 0`, or no calls) `miss` is the helper label `x` and there is no `back`; with it `miss` is the out-of-line probe of the first
    /// `__index` table (emitted with the cold code), which rejoins at `back` with `V2` = the node of the key and `V4` = its tag byte.
    /// `fin` is bound after the template (the probe continues there with a final nil result).
    fn chain_labels(&mut self, kidx: u32, x: L, a: u32, selfb: u32) -> (L, Option<L>, Option<L>) {
        if !self.calls_on() || self.env().idx_levels == 0 {
            return (x, None, None);
        }
        let (l, back, fin) = (self.p.label(), self.p.label(), self.p.label());
        self.cold.push(Cold::Chain { l, back, x, fin, kidx, a, selfb });
        (l, Some(back), Some(fin))
    }

    /// BUNDLE-K item B: the out-of-line string receiver of `SELF` (see [`Cold::StrSelf`]). Clobbers V0..V6.
    pub(crate) fn emit_str_self(&mut self, l: L, x: L, done: L, kidx: u32, a: u32, b: u32) {
        let env = self.env();
        #[cfg(feature = "trace")]
        let (skip_tag, skip_mt, nil_hit) = crate::tr::strself_mut();
        #[cfg(not(feature = "trace"))]
        let (skip_tag, skip_mt, nil_hit) = (false, false, false);
        let hash = self.cfg.consts[kidx as usize]
            .as_string()
            .expect("short string constant")
            .hash();
        self.op(Op::Bind(l));
        // a string (short or long) has the string metatable
        self.ld(0, W::B1, fr(b, 0));
        self.op(Op::And(0, Src::Imm(0x0F)));
        if !skip_tag {
            self.br(Cc::Ne, 0, Src::Imm(i32::from(rn_core::value::LUA_TSTRING)), x);
        }
        // the metatable of strings, read now (the slot is the live `g.mt[LUA_TSTRING]`)
        self.op(Op::Mov(0, env.str_mt));
        self.ld(0, W::B8, r0(0));
        if !skip_mt {
            self.br(Cc::Eq, 0, Src::Imm(0), x);
        }
        self.op(Op::Mov(1, env.idx_name));
        self.op(Op::Mov(3, u64::from(env.idx_hash)));
        self.node_lookup(x);
        // `__index` must be a table (a function or anything else is the helper's business)
        self.ld(4, W::B1, rm(2, node::VAL_TT as i32));
        self.br(Cc::Ne, 4, Src::Imm(TAG_TABLE), x);
        self.ld(0, W::B8, rm(2, node::VAL as i32));
        self.ld(1, W::B8, M { base: Base::K, off: kidx as i32 * 16 + 8 });
        self.op(Op::Mov(3, u64::from(hash)));
        self.node_lookup(x);
        self.ld(4, W::B1, rm(2, node::VAL_TT as i32));
        if !nil_hit {
            self.op(Op::MovR(5, 4));
            self.op(Op::And(5, Src::Imm(0x0F)));
            self.br(Cc::Eq, 5, Src::Imm(0), x);
        }
        self.ld(5, W::B8, rm(2, node::VAL as i32));
        // R[A+1] = R[B] first (B may equal A), then R[A] = method
        self.ld(0, W::B1, fr(b, 0));
        self.ld(1, W::B8, fr(b, 8));
        self.st(W::B1, fr(a + 1, 0), 0);
        self.st(W::B8, fr(a + 1, 8), 1);
        self.st(W::B1, fr(a, 0), 4);
        self.st(W::B8, fr(a, 8), 5);
        self.op(Op::Jmp(done));
    }

    /// The out-of-line `__index` levels (see [`Cold::Chain`], `CallEnv::idx_levels` of them). `V0` = the receiver table. Clobbers V1..V6.
    pub(crate) fn emit_chain(&mut self, l: L, back: L, x: L, fin: L, kidx: u32, a: u32, selfb: u32) {
        let env = self.env();
        #[cfg(feature = "trace")]
        let (nil_hit, use_mt) = crate::tr::t3idx_mut();
        #[cfg(not(feature = "trace"))]
        let (nil_hit, use_mt) = (false, false);
        let hash = self.cfg.consts[kidx as usize]
            .as_string()
            .expect("short string constant")
            .hash();
        let (nilres, tail) = (self.p.label(), self.p.label());
        self.op(Op::Bind(l));
        for lvl in 0..env.idx_levels {
            // V0 = the table that missed: no metatable ends the chain with nil
            self.ld(4, W::B8, rm(0, table::METATABLE as i32));
            self.br(Cc::Eq, 4, Src::Imm(0), nilres);
            self.op(Op::MovR(0, 4));
            self.op(Op::Mov(1, env.idx_name));
            self.op(Op::Mov(3, u64::from(env.idx_hash)));
            self.node_lookup(x);
            // `__index` must be a table (a function, a string, nil or `false` is the helper's business)
            self.ld(4, W::B1, rm(2, node::VAL_TT as i32));
            self.br(Cc::Ne, 4, Src::Imm(TAG_TABLE), x);
            if !use_mt {
                self.ld(0, W::B8, rm(2, node::VAL as i32));
            }
            self.ld(
                1,
                W::B8,
                M {
                    base: Base::K,
                    off: kidx as i32 * 16 + 8,
                },
            );
            self.op(Op::Mov(3, u64::from(hash)));
            // absent or nil there: the next level (after the last one: nil if that table has no metatable, else the helper)
            let next = if lvl + 1 == env.idx_levels { tail } else { self.p.label() };
            self.node_lookup(next);
            self.ld(4, W::B1, rm(2, node::VAL_TT as i32));
            if !nil_hit {
                self.op(Op::MovR(5, 4));
                self.op(Op::And(5, Src::Imm(0x0F)));
                self.br(Cc::Eq, 5, Src::Imm(0), next);
            }
            self.op(Op::Jmp(back));
            if next != tail {
                self.op(Op::Bind(next));
            }
        }
        self.op(Op::Bind(tail));
        self.ld(4, W::B8, rm(0, table::METATABLE as i32));
        self.br(Cc::Ne, 4, Src::Imm(0), x);
        // nil: no metatable at the end of the chain
        self.op(Op::Bind(nilres));
        if selfb != u32::MAX {
            // R[A+1] = R[B] first (B may equal A), then R[A] = nil
            self.ld(4, W::B1, fr(selfb, 0));
            self.ld(5, W::B8, fr(selfb, 8));
            self.st(W::B1, fr(a + 1, 0), 4);
            self.st(W::B8, fr(a + 1, 8), 5);
        }
        self.st_imm(W::B1, fr(a, 0), i32::from(LUA_VNIL));
        self.op(Op::Jmp(fin));
    }

    /// Node in `V2`: exit `x` (T3: the `__index` probe) if its value is nil, else store it to `R[a]`. `back` is bound after the nil
    /// test, where the out-of-line `__index` probe rejoins with `V4` = the value's tag byte.
    fn get_node_result(&mut self, a: u32, x: L, back: Option<L>) {
        self.get_node_result_m(a, x, back, false)
    }

    fn get_node_result_m(&mut self, a: u32, x: L, back: Option<L>, nil_hit_mut: bool) {
        self.ld(4, W::B1, rm(2, node::VAL_TT as i32));
        self.op(Op::MovR(5, 4));
        self.op(Op::And(5, Src::Imm(0x0F)));
        if !nil_hit_mut {
            self.br(Cc::Eq, 5, Src::Imm(0), x);
        }
        if let Some(back) = back {
            self.op(Op::Bind(back));
        }
        self.ld(5, W::B8, rm(2, node::VAL as i32));
        self.st(W::B1, fr(a, 0), 4);
        self.st(W::B8, fr(a, 8), 5);
        self.ty[a as usize] = Ty::Unk;
    }

    /// Store to the node in `V2`. A node that holds a non-nil value is overwritten; with `nilset` a node of an existing key whose value is
    /// nil goes to the out-of-line [`Cold::SetNil`] (otherwise it exits).
    fn set_node(&mut self, v: Vop, x: L) {
        self.ld(4, W::B1, rm(2, node::VAL_TT as i32));
        self.op(Op::And(4, Src::Imm(0x0F)));
        let back = self.p.label();
        if self.nilset_on() {
            let l = self.p.label();
            self.cold.push(Cold::SetNil { l, back, x, v });
            self.br(Cc::Eq, 4, Src::Imm(0), l);
        } else {
            self.br(Cc::Eq, 4, Src::Imm(0), x);
        }
        self.barrier_exit(v, x);
        self.op(Op::Bind(back));
        self.ld(4, W::B1, v.tag);
        self.ld(5, W::B8, v.pay);
        self.st(W::B1, rm(2, node::VAL_TT as i32), 4);
        self.st(W::B8, rm(2, node::VAL as i32), 5);
    }

    /// The out-of-line nil-valued-node store of [`Cold::SetNil`] (`V0` = table, `V2` = the node of the existing key, whose value is nil):
    /// exactly what `psetshortstr_miss` + `finish_set` do for such a slot when the table has no `__newindex`: the value must not be nil
    /// (the helper keeps the dead-slot variants), the metatable must be known to lack `__newindex`, the barrier is the one of the plain
    /// overwrite, and the store invalidates the table's metamethod-absence flags (the key may be a metamethod name). Rejoins at `back`,
    /// where the store itself is. Every exit comes before the flags are touched.
    pub(crate) fn emit_set_nil(&mut self, l: L, back: L, x: L, v: Vop) {
        self.op(Op::Bind(l));
        self.ld(4, W::B1, v.tag);
        self.op(Op::And(4, Src::Imm(0x0F)));
        self.br(Cc::Eq, 4, Src::Imm(0), x);
        if !self.nilset_mut(NS_MT_SKIP) {
            let ok = self.p.label();
            self.ld(4, W::B8, rm(0, table::METATABLE as i32));
            self.br(Cc::Eq, 4, Src::Imm(0), ok);
            self.ld(4, W::B1, rm(4, table::FLAGS as i32));
            self.op(Op::And(4, Src::Imm(1 << Tm::NewIndex as i32)));
            self.br(Cc::Eq, 4, Src::Imm(0), x);
            self.op(Op::Bind(ok));
        }
        if !self.nilset_mut(NS_BARRIER_SKIP) {
            self.barrier_exit(v, x);
        }
        if !self.nilset_mut(NS_FLAGS_SKIP) {
            self.ld(4, W::B1, rm(0, table::FLAGS as i32));
            self.op(Op::And(4, Src::Imm(i32::from(!MASKFLAGS))));
            self.st(W::B1, rm(0, table::FLAGS as i32), 4);
        }
        self.op(Op::Jmp(back));
    }

    // ---- GETTABLE ------------------------------------------------------------------------------

    /// `R[A] = R[B][R[C]]` for an integer key in the array part or a short-string key in the hash part.
    fn t_gettable(&mut self, a: u32, b: u32, c: u32, x: L) {
        self.tbl_reg(b, x);
        // BUNDLE-G1 item 1 (`tset` bit 3): an integer key outside the array part probes the hash part inline (`getintfromhash`)
        let geth = self.calls_on() && self.env().tset & 8 != 0;
        if self.ty[c as usize] == Ty::Int {
            if geth {
                let (lhash, done) = (self.p.label(), self.p.label());
                self.ld(2, W::B8, fr(c, 8));
                self.arr_slot_reg2(lhash);
                self.get_arr_result(a, x);
                self.op(Op::Jmp(done));
                self.op(Op::Bind(lhash));
                self.int_node_lookup(c, x);
                let nh = self.nilset_mut(NS_GET_NIL_HIT);
                self.get_node_result_m(a, x, None, nh);
                self.op(Op::Bind(done));
                self.ty[a as usize] = Ty::Unk;
                return;
            }
            self.arr_slot(Idx::Reg(c), x);
            self.get_arr_result(a, x);
            return;
        }
        let (lstr, done) = (self.p.label(), self.p.label());
        let lhash = self.p.label();
        self.ld(2, W::B1, fr(c, 0));
        // (mutant `tget_tag_skip`: a key of any type goes to the integer probe)
        let tagskip = geth && self.nilset_mut(NS_GET_TAG_SKIP);
        self.br(Cc::Ne, 2, Src::Imm(TAG_INT), if tagskip { lhash } else { lstr });
        self.ld(2, W::B8, fr(c, 8));
        self.arr_slot_reg2(if geth { lhash } else { x });
        self.get_arr_result(a, x);
        self.op(Op::Jmp(done));
        if geth {
            self.op(Op::Bind(lhash));
            self.int_node_lookup(c, x);
            let nh = self.nilset_mut(NS_GET_NIL_HIT);
            self.get_node_result_m(a, x, None, nh);
            self.op(Op::Jmp(done));
        }
        self.op(Op::Bind(lstr));
        self.br(Cc::Ne, 2, Src::Imm(TAG_SHRSTR), x);
        self.ld(1, W::B8, fr(c, 8));
        self.ld(4, W::B1, rm(1, string::EXTRA as i32));
        self.br(Cc::Eq, 4, Src::Imm(0), x); // (hash-D: see Settable)
        self.ld(3, W::B4, rm(1, string::SHORT_HASH as i32));
        self.node_lookup(x);
        self.get_node_result(a, x, None);
        self.op(Op::Bind(done));
        self.ty[a as usize] = Ty::Unk;
    }
}
