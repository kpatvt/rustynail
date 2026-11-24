//! T6 (`t6` limit bit 0, docs/JIT_TRACE.md section 44.1): the fused table constructor of tier 1.
//!
//! `NEWTABLE` with a hash size, followed by `SETFIELD`s of distinct constant short-string keys into the new register (`{x = a, y = b, z = c}`, the
//! constructor of every small record): the keys' main positions in the presized node vector are known when the function is compiled (the PUC hash of a
//! constant is fixed, the node count is `2^(B-1)`), so when no two of them share a main position every key lands in a free node of a fresh table and the
//! whole group is one block of stores, no probe, no collision handling and no per-key checks. A key whose main position is taken by an earlier key of the
//! group ends the group (the instructions from there on are the ordinary templates, which find the earlier keys in their nodes exactly as the interpreter's
//! insertions would).
//!
//! The fused code is emitted behind the helper call of the `NEWTABLE` template and jumps over the generic templates of the group, which stay in place as
//! the fall-through (the guards below jump into them with nothing changed). Guards (all decided before the first store): the table is not black (a collection
//! step inside the helper may have marked it: the generic path leaves the barrier work to the helper) and no stored value is nil (a nil store is a no-op that
//! must not create the key). The table is the one the helper just made: no metatable (no `__newindex`), a hash part of `2^(B-1)` nodes (not the dummy node),
//! empty nodes. Stores, per key: key tag + key pointer + value tag + payload into the node at its main position (`next` stays 0, `lastfree` is not touched:
//! what `insertkey` does for a free main position); once per group: `flags &= ~MASKFLAGS` (the keys may be metamethod names) and, for parts of 8 nodes or
//! more, the layout id slot at `node - 16` is zeroed (docs/JIT_TRACE.md section 15 rule 1). No barrier: a stored value is never needed to be remembered by a
//! table that is not black (the collector marks the table after the store, or has not reached it yet).

use crate::ir::{Base, Cc, Op, Src, M, W};
use crate::lower::{fr, Cx};
use rn_core::gc::BLACKBIT;
use rn_core::instruction::Instruction;
use rn_core::jit_layout::{node, table};
use rn_core::meta::MASKFLAGS;
use rn_core::value::{ctb, LUA_VSHRSTR};
use rn_core::ver::active::OpCode;

const TAG_SHRSTR: i32 = ctb(LUA_VSHRSTR) as i32;
const NODE_SIZE: i32 = node::SIZE as i32;

/// Mutants of the fused constructor (`tr::ctor_mutants()`; always 0 without the `tr_mutants` feature).
pub(crate) const CT_NIL_VALUE: u8 = 2;
pub(crate) const CT_FLAGS_SKIP: u8 = 4;
pub(crate) const CT_LAYOUT_SKIP: u8 = 8;
pub(crate) const CT_MAINPOS: u8 = 16;
pub(crate) const CT_COLLIDE: u8 = 32;
pub(crate) const CT_ANY_REG: u8 = 64;

/// One key of a fused group.
#[derive(Clone, Copy, Debug)]
pub(crate) struct CtorKey {
    /// Constant index of the key (a short string).
    pub kidx: u32,
    /// Node index of its main position.
    pub pos: u32,
    /// The `SETFIELD` instruction (its value operand).
    pub ins: Instruction,
}

fn rm(v: u8, off: i32) -> M {
    M {
        base: Base::Reg(v),
        off,
    }
}

impl Cx<'_> {
    fn ctor_mut(&self, bit: u8) -> bool {
        #[cfg(feature = "trace")]
        {
            crate::tr::ctor_mutants() & bit != 0
        }
        #[cfg(not(feature = "trace"))]
        {
            let _ = bit;
            false
        }
    }

    /// The group of `SETFIELD`s after the `NEWTABLE` at `pc` (register `a`, node count `size`, power of two): `(keys, pc after the group)`. Empty when
    /// the first instruction after the constructor header is not an eligible store.
    pub(crate) fn ctor_group(&self, pc: u32, a: u32, size: u32) -> (Vec<CtorKey>, u32) {
        let mut keys: Vec<CtorKey> = Vec::new();
        let mut q = pc + 2;
        while (q as usize) < self.code.len() {
            let ins = Instruction(self.code[q as usize]);
            if self.is_head[q as usize] || ins.opcode() != Some(OpCode::Setfield) {
                break;
            }
            if ins.a() != a && !self.ctor_mut(CT_ANY_REG) {
                break;
            }
            let kidx = ins.b();
            let Some(kv) = self.cfg.consts.get(kidx as usize) else {
                break;
            };
            if kv.tag() != ctb(LUA_VSHRSTR) {
                break;
            }
            // the value operand: a constant must exist and not be nil; a register other than the table itself
            if ins.k() != 0 {
                match self.cfg.consts.get(ins.c() as usize) {
                    Some(v) if v.tag() & 0x0F != 0 => {}
                    _ => break,
                }
            } else if ins.c() == a {
                break;
            }
            let Some(s) = kv.as_string() else { break };
            let pos = s.hash() & (size - 1);
            if keys.iter().any(|k| k.pos == pos) && !self.ctor_mut(CT_COLLIDE) {
                break;
            }
            keys.push(CtorKey { kidx, pos, ins });
            q += 1;
        }
        (keys, q)
    }

    /// Emits the fused constructor behind the `NEWTABLE` template of `pc` (its helper call already emitted: `R[A]` is the new table).
    pub(crate) fn emit_ctor(&mut self, pc: u32, i: Instruction) {
        if !self.calls_on() || self.env().tset & 0x20 == 0 || self.capture_rejoin {
            return;
        }
        let a = i.a();
        #[cfg(not(rustynail_lua54))]
        let b = i.vb();
        #[cfg(rustynail_lua54)]
        let b = i.b();
        if b == 0 || b > 20 {
            return;
        }
        let size = 1u32 << (b - 1);
        let (keys, after) = self.ctor_group(pc, a, size);
        if keys.is_empty() || after as usize >= self.code.len() {
            return;
        }
        let generic = self.p.label();
        // V0 = the table (just made by the helper)
        self.ld(0, W::B8, fr(a, 8));
        // (a black table can only be one that the collection step inside the helper marked from the stack: there is no way to build one on demand, so this
        // guard has no mutant; it is the guard of `Cold::NewKey`, whose mutant `newkey_black_skip` is caught on old tables)
        self.ld(4, W::B1, rm(0, table::MARKED as i32));
        self.op(Op::And(4, Src::Imm(i32::from(BLACKBIT))));
        self.br(Cc::Ne, 4, Src::Imm(0), generic);
        // a value that is nil makes the store a no-op: the generic templates answer it
        if !self.ctor_mut(CT_NIL_VALUE) {
            for k in &keys {
                if k.ins.k() == 0 {
                    self.ld(4, W::B1, fr(k.ins.c(), 0));
                    self.op(Op::And(4, Src::Imm(0x0F)));
                    self.br(Cc::Eq, 4, Src::Imm(0), generic);
                }
            }
        }
        // V2 = node vector
        self.ld(2, W::B8, rm(0, table::NODE as i32));
        for k in &keys {
            let off = if self.ctor_mut(CT_MAINPOS) {
                (k.pos as i32 + 1) * NODE_SIZE
            } else {
                k.pos as i32 * NODE_SIZE
            };
            self.st_imm(W::B1, rm(2, off + node::KEY_TT as i32), TAG_SHRSTR);
            self.ld(
                1,
                W::B8,
                M {
                    base: Base::K,
                    off: k.kidx as i32 * 16 + 8,
                },
            );
            self.st(W::B8, rm(2, off + node::KEY as i32), 1);
            let (vt, vp) = if k.ins.k() != 0 {
                let c = k.ins.c() as i32;
                (
                    M {
                        base: Base::K,
                        off: c * 16,
                    },
                    M {
                        base: Base::K,
                        off: c * 16 + 8,
                    },
                )
            } else {
                (fr(k.ins.c(), 0), fr(k.ins.c(), 8))
            };
            self.ld(4, W::B1, vt);
            self.ld(5, W::B8, vp);
            self.st(W::B1, rm(2, off + node::VAL_TT as i32), 4);
            self.st(W::B8, rm(2, off + node::VAL as i32), 5);
        }
        if !self.ctor_mut(CT_FLAGS_SKIP) {
            self.ld(4, W::B1, rm(0, table::FLAGS as i32));
            self.op(Op::And(4, Src::Imm(i32::from(!MASKFLAGS))));
            self.st(W::B1, rm(0, table::FLAGS as i32), 4);
        }
        if size >= 8 && !self.ctor_mut(CT_LAYOUT_SKIP) {
            self.st_imm(
                W::B8,
                rm(2, -(rn_core::jit_layout::node::LAYOUT_ID_BYTES as i32)),
                0,
            );
        }
        let t = self.pcl[after as usize];
        self.op(Op::Jmp(t));
        self.op(Op::Bind(generic));
    }
}
