//! The SSA IR of the trace compiler (docs/JIT_TRACE.md section 5): frozen types shared by the recorder, the
//! optimiser, the register allocator and the assembler. P0 fixes the shapes; the builder, folding, CSE, the
//! reference interpreter and the printer are P2's (they add methods, never fields).
//!
//! Instructions are appended in order (straight line); an operand is a [`Ref`]: `0` is [`NOREF`], `1..REF_BIAS` is the
//! constant `k[r - 1]` (constants are interned, one ref per value), from [`REF_BIAS`] on an instruction
//! (`ins[r - REF_BIAS]`). "Defined before" is therefore an integer compare.

/// An operand: none (`0`), constant (`0 < r < REF_BIAS`, `k[r - 1]`) or instruction (`ins[r - REF_BIAS]`).
pub type Ref = u16;
/// First instruction ref. Refs below it are constants (or `NOREF`).
pub const REF_BIAS: Ref = 0x8000;
/// "No operand".
pub const NOREF: Ref = 0;

/// Most IR instructions of one trace (`TooLong`, 4.6).
pub const MAX_INS: usize = 4000;
/// Most snapshots of one trace (`TooLong`).
pub const MAX_SNAPS: usize = 500;

/// H1: bit of the op bits (`aux_bits`) of a [`TOp::Hrefv`] that selects the integer-key form (`hashint` main position, `LUA_VNUMINT` key
/// tag); the low byte is the node count's log, as for the string form.
pub const HREF_INT: u32 = 0x100;
/// BUNDLE-L D1b: bit of the op bits of a [`TOp::Hrefv`]: the absence proof of a dynamic key (exits when the walk FINDS the key; no result).
pub const HREF_ABS: u32 = 0x200;
/// DYNMASK: bit of the op bits of a [`TOp::Hrefv`] (string key): operand `a` is the TABLE (not its node vector); the node count and the node vector are read
/// from it when the instruction runs (no `lsizenode` guard, no node-vector `FLOAD`). Such an instruction depends on the table's hash layout at its
/// own execution point: the builder never merges two of them (`Builder::cse_dyn_href`, only the loop optimiser, which gives up on every trace that can
/// rehash, sets it).
pub const HREF_DYN: u32 = 0x400;
/// DYNMASK: bit of the op bits of a [`TOp::Hrefv`] with [`HREF_DYN`]: a walk that does not find the key yields the address of [`NIL_NODE`] (a node with a nil
/// value) and the instruction is NOT a guard (no exit). Loads only: a store through the result would write into the static node (the recorder never emits one).
pub const HREF_NIL: u32 = 0x800;
/// DYNMASK: the node a missing key resolves to (all zero: value tag 0 = nil, key tag 0). Never written.
pub static NIL_NODE: [u64; 8] = [0; 8];
/// DYNMASK: `MASKS[l] = (1 << l) - 1` for the node-count logs `l <= 31` (the assembler indexes it by the table's `lsizenode`).
pub static DYN_MASKS: [u32; 32] = {
    let mut m = [0u32; 32];
    let mut i = 0;
    while i < 31 {
        m[i] = (1u32 << i) - 1;
        i += 1;
    }
    m[31] = u32::MAX >> 1;
    m
};
/// DYNMASK: is this the op / aux of a dynamic-mask `HREFV` (never merged by the builder's CSE, see [`HREF_DYN`])?
pub const fn is_dyn_href(op: TOp, aux: u32) -> bool {
    matches!(op, TOp::Hrefv) && aux_bits(aux) & HREF_DYN != 0
}
/// [`Ins::flags`]: the instruction is a guard. Its `aux` is `snapshot index | op bits << 16` ([`guard_aux`]).
pub const F_GUARD: u8 = 1;
/// [`Ins::flags`]: loop-carried value (`PHI`) or an operand of one.
pub const F_PHI: u8 = 2;
/// [`Ins::flags`]: loop invariant (hoisted into the pre-roll by peeling).
pub const F_LOOPINV: u8 = 4;
/// [`Ins::flags`]: scratch mark for passes (always clear between passes).
pub const F_MARK: u8 = 8;
/// [`Ins::flags`] (N5): a string-typed value whose short / long variant is not known statically: a checked load
/// (`Aload`, `Hload`) of this flag guards "some string" (`tag & 0xEF == ctb(LUA_VSHRSTR)`: the variant bit masked) instead of its
/// exact tag, a `Phi` of it joins such loads. Only set by the `relaxstr` pass on values whose every use is a snapshot slot
/// or another flagged `Phi`; a snapshot restore reads the variant from the object header (`exit::TAG_DYN_STR`).
pub const F_ANYSTR: u8 = 16;
/// [`Ins::flags`] (N5): the recorder read this string-typed value as an operand (`rd_slot`): a decision it made may depend on its short /
/// long variant without an IR instruction saying so (`s == t` of different variants is folded to false), so the `relaxstr` pass keeps the
/// exact guard. Merges of loads (`opt_loop`) must OR it.
pub const F_OBS: u8 = 32;

/// BUNDLE-L A: the displacement `d` (in elements) of an `Aref` with a complemented-index operand: `~index == n + d`.
pub const fn aref_disp(aux: u32) -> i32 {
    (aux >> 16) as u16 as i16 as i32
}

/// BUNDLE-L A: the `aux` of an `Aref` that carries the complemented index `n` and the displacement `d`.
pub const fn aref_aux(n: Ref, d: i16) -> u32 {
    n as u32 | (d as u16 as u32) << 16
}

/// `aux` of a guard: snapshot index in the low 16 bits, op-specific bits (`Sload`: bit 0 = typecheck; `Fcmp`:
/// condition in bits 0..4 as `0 Eq, 1 Ne, 2 Lt, 3 Le, 4 Gt, 5 Ge` and the expected result in bit 4) above.
pub const fn guard_aux(snap: u32, bits: u32) -> u32 {
    snap | bits << 16
}

/// The op bits of an [`TOp::Hmiss`]: node index (8 bits) and the expected chain offset (signed, 8 bits). `None` when either does
/// not fit.
pub fn hmiss_bits(idx: usize, delta: i32) -> Option<u32> {
    if idx < 256 && (-128..128).contains(&delta) {
        Some(idx as u32 | ((delta as u32) & 0xFF) << 8)
    } else {
        None
    }
}

/// The `(idx, delta)` of [`hmiss_bits`].
pub fn hmiss_unpack(bits: u32) -> (usize, i32) {
    ((bits & 0xFF) as usize, i32::from((bits >> 8) as u8 as i8))
}

/// The snapshot index of a guard's `aux`.
pub const fn aux_snap(aux: u32) -> u32 {
    aux & 0xFFFF
}

/// The op-specific bits of a guard's `aux`.
pub const fn aux_bits(aux: u32) -> u32 {
    aux >> 16
}

/// IR operations (5.3). The classes: N pure, G guard (has a snapshot), L load, S store, C helper call.
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TOp {
    // ---- constants (the value is in `TraceIr::k`; these exist for printing and for refs that must be instructions) ----
    /// Integer constant.
    Kint,
    /// Float constant (bits).
    Knum,
    /// `nil` / `false` / `true`.
    Kpri,
    /// Pointer constant (proto, closure, builtin, ...).
    Kptr,
    // ---- stack ----
    /// `a` = root-relative slot (a raw number, not a ref), `t` = the speculated type; `aux` = [`guard_aux`] with bit 0
    /// = typecheck (a checked load is a guard).
    Sload,
    // ---- integer arithmetic (wrapping; Lua shift semantics) ----
    /// `a + b`.
    Add,
    /// `a - b`.
    Sub,
    /// `a * b`.
    Mul,
    /// `-a`.
    Neg,
    /// `a & b`.
    Band,
    /// `a | b`.
    Bor,
    /// `a ^ b`.
    Bxor,
    /// `~a`.
    Bnot,
    /// `a << b` (Lua semantics: counts >= 64 give 0, negative counts shift right).
    Shl,
    /// `a >> b` (logical).
    Shr,
    /// Floor division, guard `b != 0` (`aux` = [`guard_aux`]).
    Idiv,
    /// Floor modulo, guard `b != 0` (`aux` = [`guard_aux`]).
    Mod,
    // ---- float arithmetic (IEEE, operand order preserved, TR11) ----
    /// `a + b`.
    Fadd,
    /// `a - b`.
    Fsub,
    /// `a * b`.
    Fmul,
    /// `a / b`.
    Fdiv,
    /// `-a`.
    Fneg,
    /// `|a|`.
    Fabs,
    /// `sqrt(a)`.
    Fsqrt,
    /// F1: `floor(a)` (`roundsd` mode 9).
    Ffloor,
    /// `math.min` operand order (`minsd` semantics).
    Fmin,
    /// `math.max`.
    Fmax,
    /// int -> float.
    ConvNumInt,
    /// float -> int, exact only; guard (`aux` = [`guard_aux`]).
    F2iExact,
    // ---- comparisons: guards that assert the relation (`aux` = [`guard_aux`]) ----
    /// `a == b` (integers).
    Eq,
    /// `a != b`.
    Ne,
    /// `a < b` (signed).
    Lt,
    /// `a <= b`.
    Le,
    /// `a > b`.
    Gt,
    /// `a >= b`.
    Ge,
    /// `a < b` unsigned (needs the P0b condition codes).
    Ult,
    /// `a >= b` unsigned.
    Uge,
    /// Float compare guard: asserts `(a cc b) == expect` (never a negated `cc`, TR11); `aux` = [`guard_aux`] with
    /// `cc | expect << 4` as the op bits.
    Fcmp,
    /// Identity guard of a pointer / Lua reference (callee proto, builtin); `aux` = [`guard_aux`].
    Eqref,
    // ---- memory ----
    /// Table header field: `a` = table, `b` = [`Field`] number.
    Fload,
    /// Address of array element `b` of array base `a`. BUNDLE-L A (`aidx`): `aux` low half = the ref of an integer `n` with
    /// `~b == n + d` (wrapping), `d` = the signed high half ([`aref_disp`]); `0` = none. The assembler addresses the
    /// payload of the element as `[base + 8 * n + 8 * d]` instead of recomputing `~b` (an `Aload` only; `Astore` ignores it).
    Aref,
    /// Element load + tag check; `a` = aref.
    Aload,
    /// Element store; `a` = aref, `b` = value.
    Astore,
    /// Address of upvalue `b` of closure `a`.
    Uref,
    /// Upvalue load.
    Uload,
    /// Upvalue store.
    Ustore,
    /// Table write barrier (`barrier_back`).
    Tbar,
    /// Call of a pure helper (`pow_lua`, `fmod`, `floor` of floats), class N.
    Calln,
    // ---- control ----
    /// Loop marker: pre-roll below, body above.
    Loop,
    /// Loop-carried value: `a` = pre-roll ref, `b` = body ref.
    Phi,
    /// Trap / interrupt poll (guard; `aux` = [`guard_aux`], snapshot `S_H`).
    Poll,
    /// Instruction budget check and charge of `a` instructions (a raw number, TR5.2): exit when `vb < a`, else
    /// `vb -= a`; `aux` = [`guard_aux`] (the snapshot is taken before the charge).
    Budget,
    // ---- full build only (reserved so the numbering is frozen) ----
    /// Hash node address with constant key.
    Hrefk,
    /// Hash load.
    Hload,
    /// Hash store.
    Hstore,
    /// Insertion of a new short-string key `b` (a constant) into the table `a` (F3, class A): `t` = `P64` node
    /// address (`handle << 32 | index` in the model) of the new node whose value is a placeholder the following
    /// `HSTORE` overwrites, or **0 when the helper did nothing** (key present, memory limit): the next instruction is
    /// then the guard `NE %newref, #0` on the instruction's snapshot. Not a guard itself.
    Newref,
    /// Table allocation (F3, class A): `a` = the `NEWTABLE` hash-size operand `B` (raw), `aux` = the array size `C` (raw);
    /// result a table, or **0 when the helper did nothing** (a GC step would be due after the allocation, or it could
    /// fail): the next instruction is the guard `NE %tnew, #0` on the snapshot of the `NEWTABLE`. Not a guard itself.
    Tnew,
    /// String -> integer coercion of a `for` loop limit (F3b, class A; `a` = a `ShrStr` / `LngStr`, `aux` = the rounding mode
    /// `0` floor / `1` ceil): `rn_core::arith::to_integer`, the interpreter's `forlimit`. Result `P64`: the address of
    /// a **pseudo node** (the helper's argument area, as for [`TOp::Tnext`]) whose value fields hold the integer, or **0
    /// when the helper did nothing** (the string is no numeral or its value is no integer in range: the interpreter
    /// raises or clamps): the next instruction is the guard `NE %strto, #0`, the value is the `HLOAD` that follows. No
    /// allocation, no GC step, nothing changes.
    Strto,
    /// Number -> string conversion (F3b, class A; `a` = an `Int` or `Num` value, `b` unused): the interned short string
    /// `luaO_tostring` makes of it, result `ShrStr`, or **0 when the helper did nothing** (a GC step would be due after the
    /// allocation, it could fail, or creating the string would grow the string table): the next instruction is the guard
    /// `NE %snew, #0` on the snapshot of the instruction. Not a guard itself.
    Snew,
    /// Concatenation of two (`a`, `b`) or three (`a`, `b`, `aux` = the third ref; `aux == 0` for two) **non-empty strings**
    /// (F3b, class A; `ShrStr` / `LngStr` operands): result a string of the type `t` (`ShrStr` iff the total length is at
    /// most 40), or **0 when the helper did nothing** (an operand is empty, the length overflows or has the other kind than
    /// `t`, a GC step would be due, the allocation could fail): the next instruction is the guard `NE %concat, #0`.''
    Concat,
    /// 5.4 only: `alimit_touch` (TR30).
    Atouch,
    // ---- P3c additions (appended: the numbering above is frozen) ----
    /// TR15 range operand: the address of the value of upvalue `uref` (`a` = a [`TOp::Uref`]) **relative to the root
    /// base** (`uv.v - rbx`, a wrapping 64-bit difference; `b` unused), class L. The recorder guards
    /// `UGE uvrel, 16 * nslots` (unsigned: outside the frame, or below its base). `interp` reads it from
    /// `Model::uv_rel`.
    Uvrel,
    // ---- F3 additions (appended) ----
    /// Length (`#`) of a table without `__len` (aux 0, `a` = table) or of a string (aux 1, `a` = string): class A helper
    /// that cannot fail, result `Int`. Not a guard.
    Tlen,
    /// `t[key] = value` for an integer key (`a` = table, `b` = key, **`aux` = the value's ref**, a third operand that no
    /// generic `a` / `b` code sees; see [`Ins::third`]) as a raw set without metamethods (F3, class A, the table's
    /// `set_int`: array growth, rehash, hash insertion); `t` = `I64`, result 1 when done and **0 when the helper did
    /// nothing** (memory limit): the next instruction is the guard `NE %tset, #0` on the instruction's snapshot. The
    /// barrier is a separate `TBAR`.
    Tset,
    // ---- F3b additions (appended) ----
    /// `next(a, b)` (F3b, class A; `a` = table, `b` = the control value, any non-nil type): the table's `luaH_next`
    /// (a read: nothing is allocated, nothing changes). Result `P64`: the address of a **pseudo node** (the four-word
    /// argument area of the helper; `HLOAD` reads its value fields, `HLOAD` with bit 1 its key fields) that holds the
    /// next key and value, or **0 when the helper did nothing** (the control key is not in the table: the interpreter
    /// raises): the next instruction is the guard `NE %tnext, #0` on the instruction's snapshot. At the end of the
    /// traversal the key is `nil`. Not a guard itself; the loads of the pseudo node must directly follow.
    Tnext,
    // ---- S0 additions (appended) ----
    /// Allocation sinking (6.8, S0): a sunk `NEWTABLE` was *virtually* allocated here: `aux` = the bytes the interpreter's
    /// `new_table` would have charged. Adds them to the virtual-byte counter (the
    /// cell of `AsmEnv::vcell`), as `Heap::charge_virtual` does. No operands, no result, not a guard; never moved.
    Tvirt,
    /// TR3b writeback (S0, option `+wb`): stores the value `a` (tag and payload) to the root-relative stack slot `aux`. Emitted
    /// at the end of the pre-roll and of the body for the slots the next iteration does not read; no result.
    Sstore,
    // ---- P-A additions (appended) ----
    /// One step of the absent-key proof of a short-string key `b` (a constant `KPTR`) in the node vector `a` (an `FLOAD node`):
    /// guard (`aux` = [`guard_aux`], op bits `idx | delta << 8`, see [`hmiss_bits`]) that node `idx` does not hold the key (`key tag
    /// != SHRSTR` or another address) and that its chain offset `next` equals `delta`. The recorder emits one per node of the chain
    /// the interpreter's `luaH_getshortstr` walks (main position first, the last has `delta == 0`) after the `lsizenode` guard, so the
    /// conjunction proves that the walk finds nothing. No result.
    Hmiss,
    // ---- P-B additions (appended) ----
    /// Library-call helper (P-B, class A; a generic `Lib` op): `aux & 0xFF` = the [`lib`] kind, `aux >> 8` = the ref of an optional third
    /// operand (0 = none; see [`Ins::third`]), `a` / `b` the first two operands (a constant `0` when the kind has fewer). Result type `t`
    /// (`ShrStr` / `LngStr` / `Lfn`: an object, or **0 when the helper did nothing**; `Int`: the value **plus one** for `Byte`, 0 when
    /// the helper did nothing). The next instruction is the guard `NE %lib, #0` on the snapshot of the library call: the interpreter
    /// re-executes the call (errors, GC step due, memory limit, unusual argument, a user-replaced metatable ...). Not a guard itself.
    Lib,
    // ---- N6 additions (appended) ----
    /// N6: hash node address of a **dynamic** short-string key: `a` = the node vector (`FLOAD t NODE`), `b` = a ref holding the
    /// address of a live short string (never a long string: the recorder guards the type), op bits (`aux` = [`guard_aux`]) = the
    /// `lsizenode` the recorder guarded (`FLOAD lsizenode; EQ #bits` precedes it, as for `HREFK`). A guard: walks the chain
    /// `luaH_getshortstr` walks (main position `hash & (2^bits - 1)` with `hash` the string's header hash, `next` links, key tag
    /// `SHRSTR` and key address equal to `b`) and exits when the key is not found; the result is the node address like `HREFK`'s.
    /// `HLOAD` / `HSTORE` use it as they use `HREFK`. The key's identity is the SSA value `b` (no constant is embedded).
    Hrefv,
    // ---- N2b additions (appended; RETF, docs/JIT_PARKED.md C38) ----
    /// N2b: pop the root frame for real and continue in its Lua caller (the `RETURN` of the root frame in a side recording). A guard
    /// whose snapshot is the state before the `RETURN` (segment k-1): the returning frame's `callstatus & RETF_CS_MASK` equals the low
    /// op bits (`nresults + 1`; the mask excludes `CIST_TAIL`, `CIST_JIT` and the C-call bits), the caller `C = ci.previous` is no C
    /// frame, and `C.savedpc` equals the constant `a` (a `KPTR` raw address: the code address after the caller's `CALL`); with op
    /// bit 8 (`RETF_K`, a `RETURN` with `k`) there is also no registered to-be-closed variable and no open upvalue at or above the
    /// frame base. Effect (after the guards pass): `thread.ci = r13 = C`, `rbx -= 16 * b` (`b` = raw `A + 1`: the returning frame's
    /// function slot is C's register `A`), `r14 = C.kcache`. No result; slots of the following instructions and snapshots are
    /// relative to C's base (`Snap::seg`). Never CSE'd, never moved, a barrier for every pass.
    Retf,
}

/// N2b: bit 8 of the op bits of a [`TOp::Retf`]: the `RETURN` has `k` (closes upvalues / to-be-closed variables): the guards add "no tbc
/// variable registered and no open upvalue at or above the frame base".
pub const RETF_K: u32 = 1 << 8;

/// N2b: the `callstatus` bits [`TOp::Retf`] compares (5.5 layout; everything but `CIST_TAIL`, `CIST_JIT`, the C-call counter and the
/// recover status): `nresults + 1`, `CIST_C`, `CIST_FRESH`, `CIST_CLSRET`, `CIST_TBC`, `CIST_OAH`, `CIST_HOOKED`, `CIST_YPCALL`,
/// `CIST_FIN`, `CIST_MMRET`. The expected value is `nresults + 1` alone (every other masked bit must be clear).
pub const RETF_CS_MASK: u32 = rn_core::state::CIST_NRESULTS
    | rn_core::state::CIST_C
    | rn_core::state::CIST_FRESH
    | rn_core::state::CIST_CLSRET
    | rn_core::state::CIST_TBC
    | rn_core::state::CIST_OAH
    | rn_core::state::CIST_HOOKED
    | rn_core::state::CIST_YPCALL
    | rn_core::state::CIST_FIN
    | rn_core::state::CIST_MMRET;

/// A1: the `aux` of an inline [`TOp::Newref`] (a key inserted into a table allocated by the same trace, whose hash part is in the state
/// the recorder tracked): the bits of a `rn_core::table::InsertPlan`. Bit 31 marks the inline form (a helper `NEWREF` has `aux == 0`).
pub mod nrplan {
    use rn_core::table::InsertPlan;
    /// The marker bit.
    pub const INLINE: u32 = 1 << 31;
    /// `aux` of the plan (nodes below 8, chain links within +-15).
    pub fn encode(p: &InsertPlan) -> Option<u32> {
        let fit = |v: i32| (-16..16).contains(&v);
        if p.node >= 8 || p.main >= 8 || !fit(p.main_next) || !fit(p.node_next) || p.lastfree.is_some_and(|l| l >= 8) {
            return None;
        }
        Some(
            INLINE
                | u32::from(p.node)
                | u32::from(p.main) << 3
                | ((p.main_next + 16) as u32) << 6
                | ((p.node_next + 16) as u32) << 11
                | p.lastfree.map_or(0, |l| u32::from(l) + 1) << 16
                | u32::from(p.flags) << 21,
        )
    }
    /// The plan `aux` encodes.
    pub fn decode(aux: u32) -> InsertPlan {
        InsertPlan {
            node: (aux & 7) as u8,
            main: (aux >> 3 & 7) as u8,
            main_next: (aux >> 6 & 31) as i32 - 16,
            node_next: (aux >> 11 & 31) as i32 - 16,
            lastfree: match aux >> 16 & 31 {
                0 => None,
                l => Some((l - 1) as u8),
            },
            flags: (aux >> 21 & 255) as u8,
        }
    }
}

/// Kinds of [`TOp::Lib`] (the low byte of its `aux`).
pub mod lib {
    /// `string.sub(s, i, j)` with integer `i`, `j`: operands `s`, `i`, `j`; result a string (type `t`).
    pub const SUB: u32 = 0;
    /// `string.byte(s, i)` (one result): operands `s`, `i`; result `byte + 1` or 0.
    pub const BYTE: u32 = 1;
    /// `string.char(n)` (one argument): operand `n`; result a one-byte string.
    pub const CHAR: u32 = 2;
    /// The method `name` of a string receiver (`s:name(...)`): `a` = the interned name (a `KPTR`), result `Lfn` (the light C function
    /// stored under `name` in the string metatable's `__index` table), 0 if the lookup finds anything else.
    pub const STRMETH: u32 = 3;
    /// `type(v)` / `math.type(v)`: `a` = the type-name index (`TYPENAMES`) as an integer constant; result a short string.
    pub const TYPENAME: u32 = 4;
    /// `tostring(n)` for a number `n` (`a`): the string (as `SNEW`, plus the check that the number metatable has no `__tostring` / `__name`).
    pub const TOSTR: u32 = 5;
    /// `type(v)`: `a` = the callee (the C closure `type`), `b` = the type index (`LuaType`) as an integer constant; the helper checks the
    /// closure's function and returns the matching upvalue string, 0 if the closure is another one.
    pub const TYPE: u32 = 6;
    /// P-O: `ccalls_ok(k)`: `a` = the integer constant `k`, the number of C-stack levels the inlined protected calls on the path need;
    /// result 1 when `ccalls + k + MARGIN < LUAI_MAXCCALLS`, 0 otherwise (the guard then exits and the interpreter runs the real
    /// `pcall`, which raises "C stack overflow" at the right depth).
    pub const CCALLS: u32 = 7;
    /// N7: `ipairs_end(t, k)`: `a` = a table, `b` = an integer key; result 1 when the raw lookup `t[k]` (array part or hash) is nil, 0
    /// otherwise (the guard then exits and the interpreter runs the iteration).
    pub const IPAIRS_END: u32 = 8;
    /// N3b: `setmetatable(t, mt)`: `a` = a table, `b` = a table or the integer 0 (no metatable); the helper does what `base_setmetatable`
    /// does (the old metatable of `t` has no `__metatable`, `mt` has no `__gc`, then the store with the collector barrier) and returns 1,
    /// or does nothing and returns 0 (the guard then exits and the interpreter runs the call, which raises its errors and registers
    /// finalizers).
    pub const SETMT: u32 = 9;
    /// F2: `a == b` of two long strings (content compare): operands the two strings; result 2 when equal, 1 when not (0 never: the guard that
    /// follows the helper is an "alloc" guard, the recorder then guards the result against the recorded outcome).
    pub const STREQ: u32 = 10;
    /// F2: `a < b` of two strings (any lengths; flag bits 0 / 3 = `a` / `b` long): 2 when true, 1 when false (bytewise, as `less_than`).
    pub const STRLT: u32 = 11;
    /// F2: `a <= b` of two strings, as [`STRLT`].
    pub const STRLE: u32 = 12;
    /// F2: `math.random(low, up)` of integers: `a` = the C closure `math.random`, `b` = low, third = up; the result is the drawn integer plus
    /// one (0 = "did nothing": an empty interval, the value -1, another closure; the guard then exits and the interpreter runs the call).
    pub const RANDOM: u32 = 13;
    /// BUNDLE-GM: one step of a `gmatch` iterator (`TFORCALL` of `gmatch_aux`): `a` = the iterator closure (a C closure); the helper does the
    /// match step the C function does for a match yielding exactly one string (whole match or one string capture), advances the closure's
    /// position / `lastmatch` upvalues, and returns the string (flag bit 1: a long one); 1 for the end of the iteration (the position is advanced as the C
    /// function does: the guard that follows exits, the interpreter's call then ends the loop), 0 (and no state change) for several or position captures, a pattern error, another closure, a result of the other length class, a GC step due.
    pub const GMATCH: u32 = 14;
    /// BUNDLE-A (`libfind`): plain `string.find(s, p, init, true)`: operands `s`, `p`, third = `init` (an integer); flag bit 0 / 3 = `s` / `p`
    /// is long. The helper searches like the library (`find_plain_pos`) and answers 1 for "not found", `pos + 2` (`pos` = the 0-based start)
    /// when found (never 0); the recorder guards the outcome it saw. Read-only: header-safe.
    pub const FIND: u32 = 15;
    /// BUNDLE-A: `string.find(s, p, init)` whose `plain` argument was false or absent while the pattern had no specials when recorded: as
    /// [`FIND`], but the helper first checks that `p` has no pattern specials *now* and answers 0 (the guard then exits and the interpreter
    /// runs the call) when it has.
    pub const FINDS: u32 = 16;
    /// BUNDLE-C (`tfmt`): `string.format` of a constant format of `%d %x %X %s %c` items (and `-` / `0` / width digits) and one to five
    /// arguments. The kind byte is `FMT_BASE + id` (`id` < 64 = the format's index in `rn_std::lib_string::jit_fmt_register`) for one to three
    /// arguments (operands = the arguments, a constant `nil` fills the absent ones), `FMT_BASE + 64 + id` for four or five: operands = the first
    /// two arguments and the result of the [`FMTX`] op that stashed the rest. The flags carry the argument types (2 bits each from bit 4: 0 integer,
    /// 1 short string, 2 long string, 3 none) and the result kind (bit 1 = long). The helper runs the reference format engine on the values and
    /// answers 0 for anything but a plain success (the interpreter then runs the call and raises its errors); allocating, header-safe like `SUB`.
    pub const FMT_BASE: u32 = 64;
    /// Is `k` a [`FMT_BASE`] kind (either form)?
    pub const fn is_fmt(k: u32) -> bool {
        k >= FMT_BASE && k < FMT_BASE + 128
    }
    /// Is `k` the form of [`FMT_BASE`] kinds that takes its trailing arguments from the stash of a [`FMTX`] op?
    pub const fn is_fmt_ext(k: u32) -> bool {
        k >= FMT_BASE + 64 && k < FMT_BASE + 128
    }
    /// BUNDLE-C: the stash op of the extended `FMT` form: operands = up to three arguments (a constant nil fills the absent ones; types in the flags as for
    /// `FMT`); the helper keeps their values (and their number) in a thread-local cell for the very next `FMT` op, which takes its third operand from
    /// this op's result (so they stay together through every pass) and clears the cell; result 1. Header-safe (touches nothing in the heap).
    pub const FMTX: u32 = 17;
    /// BUNDLE-H (`libcall`): `string.char(a, b)` of two integers: operands `a`, `b`; result a two-byte string, 0 for a code above 255.
    pub const CHAR2: u32 = 18;
    /// BUNDLE-H: `string.char(a, b, c)` of three integers (operands `a`, `b`, third); a three-byte string, 0 for a code above 255.
    pub const CHAR3: u32 = 19;
    /// BUNDLE-H: `math.random()` (the float form): `a` = the C closure `math.random`; the result is the top 53 bits of the draw plus one (never 0
    /// for a success; 0 = another closure, which the guard exits on). The recorder converts: `(raw - 1) * 2^-53`, exactly `Rand::to_float`.
    pub const RANDF: u32 = 20;
    /// BUNDLE-H: `string.lower(s)`: operand `s` (flag bit 0: long); the result has the length of `s` (flag bit 1: long). Allocating, header-safe.
    pub const LOWER: u32 = 21;
    /// BUNDLE-H: `string.upper(s)`, as [`LOWER`].
    pub const UPPER: u32 = 22;
    /// BUNDLE-H: `string.reverse(s)`, as [`LOWER`].
    pub const REVERSE: u32 = 23;
    /// BUNDLE-H: `string.rep(s, n)`: operands `s`, `n` (an integer); the result (flag bit 1: long) is `n` copies, the empty string for `n <= 0`;
    /// 0 when it would exceed [`REP_MAX`] bytes (the guard exits and the interpreter runs the call, which allocates it or raises).
    pub const REP: u32 = 24;
    /// BUNDLE-H: `string.rep(s, n, sep)`: operands `s`, `n`, third = `sep` (flag bit 4: `sep` is long), as [`REP`].
    pub const REPS: u32 = 25;
    /// BUNDLE-H: the longest result of `REP` / `REPS` the helper builds (bytes).
    pub const REP_MAX: usize = 1 << 22;
    /// Number of kinds (below [`FMT_BASE`]).
    pub const COUNT: u32 = 26;
    /// The names `TYPENAME` produces, by index.
    pub const TYPENAMES: [&str; 10] =
        ["nil", "boolean", "number", "string", "table", "function", "thread", "userdata", "integer", "float"];
}

/// Types of IR values (5.2): one Lua tag each (the mapping to `rn_core::value` tags is generated by the P2 builder,
/// as `t15::TagSet` does), plus raw machine types.
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Ty {
    /// `nil`.
    Nil,
    /// `false`.
    False,
    /// `true`.
    True,
    /// Integer.
    Int,
    /// Float.
    Num,
    /// Short string.
    ShrStr,
    /// Long string.
    LngStr,
    /// Table.
    Tab,
    /// Light C function.
    Lfn,
    /// Lua closure.
    Lcf,
    /// C closure.
    Ccl,
    /// Full userdata.
    Udata,
    /// Light userdata.
    LUdata,
    /// Coroutine.
    Thread,
    /// Raw 64-bit integer (counts, indices).
    I64,
    /// Untagged pointer (array base, node, `UpVal`, `Proto`).
    P64,
    /// Raw 32-bit (array size).
    U32,
}

/// Table header fields of [`TOp::Fload`] (offsets come from `jit_layout::table`).
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Field {
    /// Array part size.
    Asize,
    /// Array part base.
    Array,
    /// Metatable pointer.
    Metatable,
    /// Metamethod absence flags.
    Flags,
    /// log2 of the node vector size.
    Lsizenode,
    /// Node vector.
    Node,
}

/// What a pointer constant points at.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PtrKind {
    /// `Proto`.
    Proto,
    /// Lua closure.
    LClosure,
    /// A registered builtin (`math.sqrt` ...), compared by address.
    Builtin,
    /// `UpVal`.
    UpVal,
    /// Other (a table, a stack address ...).
    Raw,
}

/// A constant of the trace.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum KVal {
    /// Integer.
    Int(i64),
    /// Float, as bits (interning is by bits, so `-0.0` and NaNs stay distinct).
    Num(u64),
    /// `nil` / `false` / `true`: the type says which.
    Pri(Ty),
    /// Pointer with its kind.
    Ptr(u64, PtrKind),
}

/// Identity of a prototype for dependency edges (TR32): its address. P6 adds the generation-checked `TrId` API.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct ProtoKey(pub usize);

/// One IR instruction. 16 bytes.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ins {
    /// Operation.
    pub op: TOp,
    /// Result type.
    pub t: Ty,
    /// `F_GUARD | F_PHI | F_LOOPINV | F_MARK`.
    pub flags: u8,
    /// First operand.
    pub a: Ref,
    /// Second operand.
    pub b: Ref,
    /// Previous instruction of the same op (CSE chain), `NOREF` at the end.
    pub prev: Ref,
    /// Op-specific word: snapshot index of a guard, slot flags, immediate.
    pub aux: u32,
}

impl Ins {
    /// The `aux` word of this op with its third operand replaced by `r` (renumbering): `Lib` keeps its kind in the low byte.
    pub const fn aux_with_third(&self, r: Ref) -> u32 {
        match self.op {
            TOp::Lib => (self.aux & 0xFF) | (r as u32) << 8,
            // BUNDLE-L A (`aidx`): the low half is the ref of the complemented index, the high half the displacement (`aidx_disp`)
            TOp::Aref => (self.aux & 0xFFFF_0000) | r as u32,
            _ => r as u32,
        }
    }

    /// The third value operand of the ops that have one (`Tset`: the value, kept in `aux`), `None` for the others. Code
    /// that renumbers or visits refs must handle it besides `a` / `b` (`ref_operands`).
    pub const fn third(&self) -> Option<Ref> {
        match self.op {
            TOp::Tset => Some(self.aux as Ref),
            TOp::Concat if self.aux != 0 => Some(self.aux as Ref),
            TOp::Lib if self.aux >> 8 != 0 => Some((self.aux >> 8) as Ref),
            TOp::Aref if self.aux & 0xFFFF != 0 => Some((self.aux & 0xFFFF) as Ref),
            _ => None,
        }
    }
}

/// Which landing label of tier 1 an exit continues at (TR5.4): the label before a block's count (a head) or after it.
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum Land {
    /// The pre-count label of a block head: `land = 0`.
    #[default]
    Pre,
    /// A label in the middle of a block: `land = rem[pc]` instructions were prepaid by the head.
    Mid,
}

/// A snapshot: the interpreter state before instruction `pc` (TR3a). The map is a window of
/// [`TraceIr::snapmap`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Snap {
    /// The instruction (of the frame at depth `frame`) the exit re-executes in tier 1.
    pub pc: u32,
    /// Depth of `pc`'s frame (0 = root).
    pub frame: u8,
    /// `(start, len)` into `TraceIr::snapmap`.
    pub map: (u32, u16),
    /// Number of inlined frames to materialise (`0`: the inline stub restores it).
    pub nframes: u8,
    /// TR5.4: an exit stores `vb + refund` to the budget word, where `vb` is the budget register at the guard and
    /// `refund = n_charged - fetches(pc) - land(pc)` (`n_charged` is the charge already taken in this segment).
    pub refund: i32,
    /// Landing label kind of `pc` in tier 1.
    pub land: Land,
    /// Exit-count slot, owned by P6 (7.9); `0` in P0.
    pub count: u32,
    /// F4 (TR22b relaxed): `0` = the exit sets `thread.top = ci.top` of the innermost frame; otherwise `top_slot + 1`
    /// with `top_slot` the root-relative stack slot the interpreter's `thread.top` holds before `pc` (the instruction is
    /// a consumer of a multiple-result sequence: `CALL B=0`, `RETURN B=0`, `TAILCALL B=0`, `SETLIST B=0`).
    pub top: u16,
    /// N2a (RETF): the segment the snapshot's guard executes in: `0` = the entry segment (slots relative to the root frame of
    /// the trace entry), `k >= 1` = after the k-th `RETF` (slots relative to the base of the frame that RETF returned into;
    /// `TraceIr::segs[k]` describes it). Always 0 unless `TrLimits::retf` is set.
    pub seg: u8,
}

/// [`SnapEntry::flags`]: the slot is a function slot of an inlined frame (restored from the closure).
pub const SNAP_FUNC: u8 = 1;

/// [`SnapEntry::flags`] (allocation sinking, S0): the entry describes a *sunk table* held by slot `slot`. `r` is the constant
/// `Int(b | c << 16)` (the `NEWTABLE` operands `B`, `C`); the `SNAP_SFIELD` entries that follow it are its fields in insertion
/// order. The exit restore allocates the table and stores its fields (TR26c).
pub const SNAP_SUNK: u8 = 2;
/// [`SnapEntry::flags`]: first half of a field of the preceding `SNAP_SUNK` entry: `slot` = [`SNAP_PSEUDO`], `r` = the key
/// (a `KVal::Ptr` constant: the interned short string); the next entry is its `SNAP_SFVAL`.
pub const SNAP_SFKEY: u8 = 4;
/// [`SnapEntry::flags`]: second half of a sunk field: `slot` = [`SNAP_PSEUDO`], `r` = the value.
pub const SNAP_SFVAL: u8 = 16;
/// [`SnapEntry::flags`]: slot `slot` holds the *same* sunk table as the slot whose number is the `Int` constant `r` (the
/// restore stores that slot's table here too: identity).
pub const SNAP_SALIAS: u8 = 8;
/// [`SnapEntry::flags`] (BUNDLE-2, `sinkmt`): `slot` = [`SNAP_PSEUDO`], `r` = the metatable of the preceding `SNAP_SUNK` table (set after its fields).
pub const SNAP_SMT: u8 = 32;
/// Any of the sunk-table entry kinds.
pub const SNAP_SUNK_ANY: u8 = SNAP_SUNK | SNAP_SFKEY | SNAP_SFVAL | SNAP_SALIAS | SNAP_SMT;
/// `slot` values at or above this are not stack slots (the field entries of a sunk table).
pub const SNAP_PSEUDO: u16 = 0x8000;

/// One slot of a snapshot map.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SnapEntry {
    /// Root-relative stack slot.
    pub slot: u16,
    /// The value (a constant or an instruction).
    pub r: Ref,
    /// `SNAP_FUNC` or 0.
    pub flags: u8,
}

/// An inlined frame (5.5).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FrameLink {
    /// The callee.
    pub proto: ProtoKey,
    /// Root-relative slot of the callee's function slot.
    pub func_slot: u16,
    /// The caller's `CALL` instruction.
    pub call_pc: u32,
    /// Results wanted by the call.
    pub nresults: i16,
    /// F4: the frame was entered by a `TAILCALL` (`CIST_TAIL` in its `callstatus`).
    pub tail: bool,
    /// P-E: a vararg callee: the number of extra arguments (`CallInfo::nextraargs`, written by the restore);
    /// `func_slot` is the *shifted* function slot (`luaT_adjustvarargs`). `-1`: not a vararg frame.
    pub nextra: i32,
    /// N3: a metamethod frame run in place (`CIST_MMRET`; its `ctx` / the thread's `nCcalls` are rebuilt by the restore).
    pub mm: bool,
}

/// N2a (RETF): one segment of a trace that has executed `RETF`s. `segs[0]` describes the entry root; `segs[k]` (k >= 1) the
/// frame the k-th `RETF` returned into. A slot `s` of segment k is the stack index `base_k + s`, with
/// `base_k = base_0 - shift_k`, so it is the entry-relative slot `s - shift_k` (negative for the caller region).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Seg {
    /// The prototype that runs in this segment's root frame (landing labels, frame restore keys).
    pub proto: ProtoKey,
    /// `Proto.code` of `proto` (the root frame's `savedpc` base for exits in this segment).
    pub code: u64,
    /// Cumulative number of slots between this segment's base and the entry base.
    pub shift: u16,
    /// Number of frames popped since the entry (`RETF`s executed).
    pub pops: u8,
    /// The caller's `CALL` pc this segment continues after, and that CALL's `A`.
    pub call_pc: u32,
    /// `A` of the CALL.
    pub a: u16,
    /// The `RETF` instruction that opens the segment.
    pub retf_ins: Ref,
}

/// A recorded trace (5.1).
#[derive(Clone, Debug, Default)]
pub struct TraceIr {
    /// N2a: the segments of a trace with `RETF`s (`segs[0]` = the entry root); empty when the feature is unused.
    pub segs: Vec<Seg>,
    /// Instructions, in order.
    pub ins: Vec<Ins>,
    /// Constants.
    pub k: Vec<KVal>,
    /// BUNDLE-2: index of `k` for [`TraceIr::konst`] (value -> ref); covers the first `kidx_n` constants (rebuilt when `k` changed behind its back).
    #[doc(hidden)]
    pub kidx: crate::fx::HashMap<KVal, Ref>,
    #[doc(hidden)]
    pub kidx_n: usize,
    /// Snapshots.
    pub snaps: Vec<Snap>,
    /// Snapshot maps (windows of `Snap::map`).
    pub snapmap: Vec<SnapEntry>,
    /// Inlined frames of the snapshots.
    pub frames: Vec<FrameLink>,
    /// The `LOOP` marker (pre-roll below, body above); `None`: rolled layout (7.8).
    pub loop_at: Option<Ref>,
    /// Prototypes the trace depends on (TR32).
    pub deps: Vec<ProtoKey>,
    /// The loop head: prototype and pc.
    pub head: (ProtoKey, u32),
    /// Stack extent in slots above the root base the trace touches (TR6a).
    pub extent: u16,
    /// Maximum inline depth (TR6b).
    pub depth: u8,
    /// Per segment (pre-roll, body): total instructions fetched in one iteration (TR5.1).
    pub counts: Vec<u32>,
    /// P-E2: a function-entry root: every exit except the final one (before the root frame's `RETURN`, the last guard)
    /// leaves the trace early and counts for the ineffective-trace rule like a pre-roll exit.
    pub fn_root: bool,
    /// P-M3: the trace ends with a native self-call: after the writeback, the frame of the callee (this very prototype) is
    /// pushed as tier 1's flat `CALL` pushes it, and control continues at the loop top of the new frame. The last IR
    /// instruction is the always-failing placeholder guard of a stitching exit (assembled to nothing).
    pub rec_call: Option<RecCall>,
    /// P-M3: the trace ends with a native `RETURN1` of the root frame (see [`RecRet`]); the only writeback entry is the
    /// returned value, stored into the function slot. Like `rec_call`, the last IR instruction is the placeholder guard.
    pub rec_ret: Option<RecRet>,
    /// BUNDLE-L B: the pre-roll holds the hoisted bounds guards of a counted loop (`opt_loop::Em::plan_bhoist`): a persistent failure of them
    /// re-records the head without the hoist (`HeadData::nohoist`).
    pub hoisted: bool,
}

/// P-M3: the native `RETURN1` that ends a continuation or return trace: the frame must have been called for exactly one
/// result and by a frame suspended after one of the continuation heads (otherwise the placeholder guard exits to tier 1
/// before the instruction); the result is stored into the function slot, the frame popped as tier 1's `RETURN1` pops it,
/// and control continues at the dispatch cell of the continuation head.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RecRet {
    /// Required `callstatus` of the returning frame (`nresults + 1` = 2).
    pub c: u32,
    /// R2: number of results returned (0: `RETURN0`, 1: `RETURN1`); the caller's `top` is `func + n` after the return.
    pub n: u8,
}

/// P-M3: the native self-call that ends a function-entry trace (see [`TraceIr::rec_call`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RecCall {
    /// Register `A` of the `CALL`: the callee's function slot (root-relative).
    pub a: u16,
    /// Number of arguments (`B - 1`).
    pub nargs: u16,
    /// `callstatus` of the new frame (`C`: wanted results + 1).
    pub c: u32,
    /// `maxstacksize` of the callee.
    pub maxstack: u16,
    /// `Proto.code` base address (the new frame's `savedpc`).
    pub code: u64,
    /// Address of the instruction after the `CALL` (the caller's `savedpc`).
    pub after: u64,
}

impl TraceIr {
    /// BUNDLE-C: empties the trace but keeps the buffers' capacity (a reusable scratch IR; the result equals `TraceIr::default()`).
    pub fn reset(&mut self) {
        self.ins.clear();
        self.k.clear();
        self.kidx.clear();
        self.kidx_n = 0;
        self.snaps.clear();
        self.snapmap.clear();
        self.frames.clear();
        self.segs.clear();
        self.loop_at = None;
        self.deps.clear();
        self.head = Default::default();
        self.extent = 0;
        self.depth = 0;
        self.counts.clear();
        self.fn_root = false;
        self.rec_call = None;
        self.rec_ret = None;
    }

    /// Is `r` an instruction ref?
    pub fn is_ins(r: Ref) -> bool {
        r >= REF_BIAS
    }

    /// Appends an instruction and returns its ref.
    pub fn emit(&mut self, ins: Ins) -> Ref {
        assert!(self.ins.len() < MAX_INS, "TooLong");
        self.ins.push(ins);
        REF_BIAS + (self.ins.len() - 1) as Ref
    }

    /// The instruction a ref denotes (panics for a constant).
    pub fn at(&self, r: Ref) -> &Ins {
        assert!(Self::is_ins(r), "constant ref {r}");
        &self.ins[(r - REF_BIAS) as usize]
    }

    /// Interns a constant: one ref per value.
    pub fn konst(&mut self, v: KVal) -> Ref {
        if self.k.len() <= 24 {
            if let Some(i) = self.k.iter().position(|&x| x == v) {
                return i as Ref + 1;
            }
        } else {
            // BUNDLE-2: a hash index (the linear search was 0.7-1.6% of the compile-heavy programs); the first occurrence wins, as in the scan
            if self.kidx_n > self.k.len() {
                self.kidx.clear();
                self.kidx_n = 0;
            }
            while self.kidx_n < self.k.len() {
                self.kidx.entry(self.k[self.kidx_n]).or_insert(self.kidx_n as Ref + 1);
                self.kidx_n += 1;
            }
            if let Some(&r) = self.kidx.get(&v) {
                return r;
            }
        }
        assert!(
            self.k.len() + 1 < usize::from(REF_BIAS),
            "too many constants"
        );
        self.k.push(v);
        self.k.len() as Ref
    }

    /// The value of a constant ref (panics for `NOREF` and instructions).
    pub fn kval(&self, r: Ref) -> KVal {
        assert!(r != NOREF && r < REF_BIAS, "not a constant ref {r}");
        self.k[usize::from(r) - 1]
    }

    /// Appends a snapshot with the given map entries and returns its index.
    pub fn snapshot(&mut self, mut s: Snap, entries: &[SnapEntry]) -> u32 {
        assert!(self.snaps.len() < MAX_SNAPS, "TooLong");
        s.map = (self.snapmap.len() as u32, entries.len() as u16);
        self.snapmap.extend_from_slice(entries);
        self.snaps.push(s);
        (self.snaps.len() - 1) as u32
    }

    /// The map entries of snapshot `i`.
    pub fn snap_entries(&self, i: u32) -> &[SnapEntry] {
        let (s, n) = self.snaps[i as usize].map;
        &self.snapmap[s as usize..s as usize + usize::from(n)]
    }
}

// ---- P2: classification, builder, constants ---------------------------------------------------------------------

/// Why building / optimising a trace stopped (docs/JIT_TRACE.md 4.6 and 6.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IrAbort {
    /// More than [`MAX_INS`] instructions.
    TooLong,
    /// A guard folded to constant false (`Compile(const_guard)`: the recorder disagreed with itself).
    ConstGuard,
}

impl TOp {
    /// Number of operations (size of per-op tables).
    pub const COUNT: usize = TOp::Retf as usize + 1;

    /// Ops that are guards by definition (their `F_GUARD` flag is always set by the builder).
    pub const fn always_guard(self) -> bool {
        matches!(
            self,
            TOp::Eq
                | TOp::Ne
                | TOp::Lt
                | TOp::Le
                | TOp::Gt
                | TOp::Ge
                | TOp::Ult
                | TOp::Uge
                | TOp::Fcmp
                | TOp::Eqref
                | TOp::Poll
                | TOp::Budget
                | TOp::Retf
        )
    }

    /// Integer compare guards (`Eq..Uge`).
    pub const fn is_icmp(self) -> bool {
        matches!(
            self,
            TOp::Eq | TOp::Ne | TOp::Lt | TOp::Le | TOp::Gt | TOp::Ge | TOp::Ult | TOp::Uge
        )
    }

    /// Ops the CSE pass (6.2) may merge: class N (pure, possibly with a guard that depends only on the operands) and
    /// the pure guards. Loads, stores, calls with effects, `Sload`, `Poll`, `Budget` and control ops never.
    pub const fn is_cse(self) -> bool {
        matches!(
            self,
            TOp::Add
                | TOp::Sub
                | TOp::Mul
                | TOp::Neg
                | TOp::Band
                | TOp::Bor
                | TOp::Bxor
                | TOp::Bnot
                | TOp::Shl
                | TOp::Shr
                | TOp::Idiv
                | TOp::Mod
                | TOp::Fadd
                | TOp::Fsub
                | TOp::Fmul
                | TOp::Fdiv
                | TOp::Fneg
                | TOp::Fabs
                | TOp::Fsqrt
                | TOp::Ffloor
                | TOp::Fmin
                | TOp::Fmax
                | TOp::ConvNumInt
                | TOp::F2iExact
                | TOp::Eq
                | TOp::Ne
                | TOp::Lt
                | TOp::Le
                | TOp::Gt
                | TOp::Ge
                | TOp::Ult
                | TOp::Uge
                | TOp::Fcmp
                | TOp::Eqref
                | TOp::Aref
                | TOp::Hrefk
                | TOp::Hrefv
                | TOp::Hmiss
                | TOp::Uref
                | TOp::Uvrel
                | TOp::Calln
        )
    }

    /// Integer ops whose operands commute exactly (floats keep their operand order, TR11).
    pub const fn is_commutative(self) -> bool {
        matches!(
            self,
            TOp::Add | TOp::Mul | TOp::Band | TOp::Bor | TOp::Bxor | TOp::Eq | TOp::Ne
        )
    }

    /// Which of `a`, `b` are refs (the others are raw numbers: `Sload` slot, `Budget` charge, `Fload` field).
    pub const fn ref_operands(self) -> (bool, bool) {
        match self {
            TOp::Sload | TOp::Budget | TOp::Tvirt => (false, false),
            TOp::Retf => (true, false),
            TOp::Fload | TOp::Uvrel | TOp::Hload | TOp::Tlen | TOp::Snew | TOp::Strto | TOp::Sstore => (true, false),
            TOp::Tnew => (false, false),
            _ => (true, true),
        }
    }

    /// Display name.
    pub const fn name(self) -> &'static str {
        match self {
            TOp::Kint => "KINT",
            TOp::Knum => "KNUM",
            TOp::Kpri => "KPRI",
            TOp::Kptr => "KPTR",
            TOp::Sload => "SLOAD",
            TOp::Add => "ADD",
            TOp::Sub => "SUB",
            TOp::Mul => "MUL",
            TOp::Neg => "NEG",
            TOp::Band => "BAND",
            TOp::Bor => "BOR",
            TOp::Bxor => "BXOR",
            TOp::Bnot => "BNOT",
            TOp::Shl => "SHL",
            TOp::Shr => "SHR",
            TOp::Idiv => "IDIV",
            TOp::Mod => "MOD",
            TOp::Fadd => "FADD",
            TOp::Fsub => "FSUB",
            TOp::Fmul => "FMUL",
            TOp::Fdiv => "FDIV",
            TOp::Fneg => "FNEG",
            TOp::Fabs => "FABS",
            TOp::Fsqrt => "FSQRT",
            TOp::Ffloor => "FFLOOR",
            TOp::Fmin => "FMIN",
            TOp::Fmax => "FMAX",
            TOp::ConvNumInt => "CONV",
            TOp::F2iExact => "F2I",
            TOp::Eq => "EQ",
            TOp::Ne => "NE",
            TOp::Lt => "LT",
            TOp::Le => "LE",
            TOp::Gt => "GT",
            TOp::Ge => "GE",
            TOp::Ult => "ULT",
            TOp::Uge => "UGE",
            TOp::Fcmp => "FCMP",
            TOp::Eqref => "EQREF",
            TOp::Fload => "FLOAD",
            TOp::Aref => "AREF",
            TOp::Aload => "ALOAD",
            TOp::Astore => "ASTORE",
            TOp::Uref => "UREF",
            TOp::Uload => "ULOAD",
            TOp::Ustore => "USTORE",
            TOp::Tbar => "TBAR",
            TOp::Calln => "CALLN",
            TOp::Loop => "LOOP",
            TOp::Phi => "PHI",
            TOp::Poll => "POLL",
            TOp::Budget => "BUDGET",
            TOp::Hrefk => "HREFK",
            TOp::Hrefv => "HREFV",
            TOp::Hmiss => "HMISS",
            TOp::Lib => "LIB",
            TOp::Hload => "HLOAD",
            TOp::Hstore => "HSTORE",
            TOp::Newref => "NEWREF",
            TOp::Tnew => "TNEW",
            TOp::Strto => "STRTO",
            TOp::Snew => "SNEW",
            TOp::Concat => "CONCAT",
            TOp::Atouch => "ATOUCH",
            TOp::Uvrel => "UVREL",
            TOp::Tlen => "TLEN",
            TOp::Tset => "TSET",
            TOp::Tvirt => "TVIRT",
            TOp::Sstore => "SSTORE",
            TOp::Tnext => "TNEXT",
            TOp::Retf => "RETF",
        }
    }
}

impl Ty {
    /// Display name.
    pub const fn name(self) -> &'static str {
        match self {
            Ty::Nil => "nil",
            Ty::False => "false",
            Ty::True => "true",
            Ty::Int => "int",
            Ty::Num => "num",
            Ty::ShrStr => "str",
            Ty::LngStr => "lstr",
            Ty::Tab => "tab",
            Ty::Lfn => "lfn",
            Ty::Lcf => "lcf",
            Ty::Ccl => "ccl",
            Ty::Udata => "udata",
            Ty::LUdata => "ludata",
            Ty::Thread => "thread",
            Ty::I64 => "i64",
            Ty::P64 => "p64",
            Ty::U32 => "u32",
        }
    }
}

impl KVal {
    /// The type of a constant.
    pub const fn ty(self) -> Ty {
        match self {
            KVal::Int(_) => Ty::Int,
            KVal::Num(_) => Ty::Num,
            KVal::Pri(t) => t,
            // N7: a registered builtin is a light C function value (it can be stored into a slot: `ipairs(t)`'s iterator)
            KVal::Ptr(_, PtrKind::Builtin) => Ty::Lfn,
            KVal::Ptr(..) => Ty::P64,
        }
    }
}

impl TraceIr {
    /// Is `r` a constant ref (not `NOREF`, not an instruction)?
    pub fn is_const(r: Ref) -> bool {
        r != NOREF && r < REF_BIAS
    }

    /// The type of the value `r` denotes.
    pub fn ty_of(&self, r: Ref) -> Ty {
        if Self::is_ins(r) {
            self.at(r).t
        } else {
            self.kval(r).ty()
        }
    }

    /// Interns an integer constant.
    pub fn kint(&mut self, i: i64) -> Ref {
        self.konst(KVal::Int(i))
    }

    /// Interns a float constant (by bits).
    pub fn knum(&mut self, f: f64) -> Ref {
        self.konst(KVal::Num(f.to_bits()))
    }

    /// The integer constant `r` denotes, if it is one.
    pub fn as_int(&self, r: Ref) -> Option<i64> {
        match Self::is_const(r).then(|| self.kval(r)) {
            Some(KVal::Int(i)) => Some(i),
            _ => None,
        }
    }

    /// The float constant `r` denotes, if it is one.
    pub fn as_num(&self, r: Ref) -> Option<f64> {
        match Self::is_const(r).then(|| self.kval(r)) {
            Some(KVal::Num(b)) => Some(f64::from_bits(b)),
            _ => None,
        }
    }
}

/// Incremental trace construction with the optimisations of 6.1 / 6.2 applied at emission time (LuaJIT's design: an
/// instruction is folded, then looked up in the per-op chain of [`Ins::prev`], and only then appended). `fold` /
/// `cse` mirror the `-fold` / `-cse` switches of `RUSTYNAIL_JIT_TRACE_OPT`.
#[derive(Clone, Debug)]
pub struct Builder {
    /// The trace under construction.
    pub ir: TraceIr,
    heads: [Ref; TOp::COUNT],
    /// Constant folding on.
    pub fold: bool,
    /// CSE on.
    pub cse: bool,
    /// DYNMASK: may two [`HREF_DYN`] `HREFV`s of the same operands be merged? Only for a builder whose input has no op that can rehash a table
    /// (the loop optimiser's: it gives up on `NEWREF` and friends, `opt_mem::header_safe_ins`).
    pub cse_dyn_href: bool,
}

impl Builder {
    /// A builder with the given pass switches.
    pub fn new(fold: bool, cse: bool) -> Builder {
        Builder {
            ir: TraceIr::default(),
            heads: [NOREF; TOp::COUNT],
            fold,
            cse,
            cse_dyn_href: false,
        }
    }

    /// A builder with the switches of `o`.
    pub fn with_opts(o: &super::TraceOpts) -> Builder {
        Builder::new(o.fold, o.cse)
    }

    /// Emits `op` with the usual flags (`F_GUARD` for the always-guard ops). A guard proven true is removed and
    /// yields [`NOREF`]; a guard proven false is `Err(ConstGuard)`.
    pub fn emit(&mut self, op: TOp, t: Ty, a: Ref, b: Ref, aux: u32) -> Result<Ref, IrAbort> {
        self.emit_flags(op, t, 0, a, b, aux)
    }

    /// [`Builder::emit`] with explicit flags (`F_GUARD` for `Idiv`, `Mod`, `F2iExact`, checked `Sload` and `Aload`).
    pub fn emit_flags(
        &mut self,
        op: TOp,
        t: Ty,
        flags: u8,
        a: Ref,
        b: Ref,
        aux: u32,
    ) -> Result<Ref, IrAbort> {
        let mut ins = Ins {
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
        if self.fold {
            match super::fold::fold(&mut self.ir, ins)? {
                super::fold::Fold::Ref(r) => return Ok(r),
                super::fold::Fold::Drop => return Ok(NOREF),
                super::fold::Fold::Keep(i) => ins = i,
            }
        }
        if self.cse
            && op.is_cse()
            && (self.cse_dyn_href || !is_dyn_href(op, aux) || super::mutants::active("dynmask_cse_merge")) {
            if let Some(r) = self.find(&ins) {
                return Ok(r);
            }
        }
        self.emit_raw(ins)
    }

    /// N2b: emits a [`TOp::Retf`] (`a` = the site constant, `b` = raw `A + 1`, `aux` = `guard_aux(snapshot, bits)`) and cuts the `UVREL` chain:
    /// `UVREL` is relative to the root base, which `RETF` moves, so nothing before it may be merged with anything after it. The cut makes the
    /// `RETF` itself the head of the `Uvrel` chain (it never equals a `Uvrel`, and its `prev` links older `RETF`s only), so `find` stops there:
    /// no per-instruction test in `emit_raw`. Every pass that rebuilds IR through a `Builder` emits a `RETF` through this function.
    pub fn emit_retf(&mut self, a: Ref, b: Ref, aux: u32) -> Result<Ref, IrAbort> {
        let r = self.emit_raw(Ins { op: TOp::Retf, t: Ty::Nil, flags: F_GUARD, a, b, prev: NOREF, aux })?;
        if !super::mutants::active("retf_uvrel_cse") {
            self.heads[TOp::Uvrel as usize] = r;
        }
        Ok(r)
    }

    /// Appends `ins` as is (links the CSE chain).
    pub fn emit_raw(&mut self, mut ins: Ins) -> Result<Ref, IrAbort> {
        if self.ir.ins.len() >= MAX_INS {
            return Err(IrAbort::TooLong);
        }
        ins.prev = self.heads[ins.op as usize];
        let r = self.ir.emit(ins);
        self.heads[ins.op as usize] = r;
        Ok(r)
    }

    /// The `LOOP` marker (records `loop_at`).
    pub fn loop_marker(&mut self) -> Result<Ref, IrAbort> {
        let r = self.emit_raw(Ins {
            op: TOp::Loop,
            t: Ty::I64,
            flags: 0,
            a: NOREF,
            b: NOREF,
            prev: NOREF,
            aux: 0,
        })?;
        self.ir.loop_at = Some(r);
        Ok(r)
    }

    /// A `PHI` with pre-roll value `a`; the body value is set later by [`Builder::set_phi`].
    pub fn phi(&mut self, t: Ty, a: Ref) -> Result<Ref, IrAbort> {
        self.emit_raw(Ins {
            op: TOp::Phi,
            t,
            flags: F_PHI,
            a,
            b: NOREF,
            prev: NOREF,
            aux: 0,
        })
    }

    /// Sets the body value of `phi`.
    pub fn set_phi(&mut self, phi: Ref, body: Ref) {
        let i = (phi - REF_BIAS) as usize;
        assert_eq!(self.ir.ins[i].op, TOp::Phi);
        self.ir.ins[i].b = body;
    }

    /// The CSE lookup (6.2): the latest identical instruction of the op chain. The search stops at the larger operand
    /// ref (an identical instruction cannot precede its operands). Guards compare their op bits, not the snapshot
    /// (the earlier guard dominates on a straight line).
    pub fn find(&self, ins: &Ins) -> Option<Ref> {
        let lim = ins.a.max(ins.b);
        let guard = ins.flags & F_GUARD != 0;
        let mut c = self.heads[ins.op as usize];
        while c != NOREF && c >= lim {
            let o = self.ir.at(c);
            let same_aux = if guard {
                aux_bits(o.aux) == aux_bits(ins.aux)
            } else {
                o.aux == ins.aux
            };
            if o.a == ins.a
                && o.b == ins.b
                && o.t == ins.t
                && same_aux
                && (o.flags & F_GUARD != 0) == guard
            {
                return Some(c);
            }
            c = o.prev;
        }
        None
    }

    /// The finished trace.
    pub fn finish(self) -> TraceIr {
        self.ir
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_is_frozen() {
        assert_eq!(std::mem::size_of::<Ins>(), 16);
        assert_eq!(std::mem::size_of::<TOp>(), 1);
        assert_eq!(std::mem::size_of::<Ty>(), 1);
    }

    /// DYNMASK: two `HREFV` with `HREF_DYN` of the same table and key are never merged by the recorder's builder (a `NEWREF` between them can
    /// rehash the table: the second walk must read the table again); the loop optimiser's builder (`cse_dyn_href`) merges them, and the other
    /// `HREFV` forms still merge. Detector of `dynmask_cse_merge`.
    #[test]
    fn dynamic_mask_hrefv_is_never_merged() {
        let build = |cse_dyn: bool, bits: u32| {
            let mut b = Builder::new(true, true);
            b.cse_dyn_href = cse_dyn;
            let t = b.emit_raw(Ins { op: TOp::Sload, t: Ty::Tab, flags: 0, a: 1, b: 0, prev: NOREF, aux: 0 }).unwrap();
            let k = b.emit_raw(Ins { op: TOp::Sload, t: Ty::ShrStr, flags: 0, a: 2, b: 0, prev: NOREF, aux: 0 }).unwrap();
            let aux = guard_aux(0, bits);
            let h1 = b.emit_flags(TOp::Hrefv, Ty::P64, 0, t, k, aux).unwrap();
            let h2 = b.emit_flags(TOp::Hrefv, Ty::P64, 0, t, k, aux).unwrap();
            (h1, h2)
        };
        let (a, b) = build(false, HREF_DYN | HREF_NIL);
        assert_ne!(a, b, "two dynamic-mask HREFV merged");
        let (a, b) = build(true, HREF_DYN | HREF_NIL);
        assert_eq!(a, b, "the loop optimiser's builder does not merge");
        let (a, b) = build(false, 5);
        assert_eq!(a, b, "the node-count form merges as before");
        assert!(is_dyn_href(TOp::Hrefv, guard_aux(3, HREF_DYN)) && !is_dyn_href(TOp::Hrefv, guard_aux(3, 7)) && !is_dyn_href(TOp::Aref, guard_aux(3, HREF_DYN)));
    }

    #[test]
    fn constants_are_interned_and_refs_ordered() {
        let mut t = TraceIr::default();
        let a = t.konst(KVal::Int(7));
        let b = t.konst(KVal::Num(0.0f64.to_bits()));
        let c = t.konst(KVal::Num((-0.0f64).to_bits()));
        assert_eq!(a, t.konst(KVal::Int(7)));
        assert_ne!(b, c, "-0.0 and 0.0 stay distinct");
        let i = t.emit(Ins {
            op: TOp::Add,
            t: Ty::Int,
            flags: 0,
            a,
            b: a,
            prev: NOREF,
            aux: 0,
        });
        assert!(a != NOREF && a < REF_BIAS && i >= REF_BIAS && TraceIr::is_ins(i));
        assert_eq!(t.kval(a), KVal::Int(7));
        assert_eq!(t.at(i).op, TOp::Add);
    }
}
