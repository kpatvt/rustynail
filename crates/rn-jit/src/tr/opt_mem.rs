//! Memory optimisation of the trace compiler (docs/JIT_TRACE.md 6.6, TR13): alias analysis over the memory classes
//! of the prototype (array elements, upvalue cells, table headers), store-to-load forwarding and redundant-load
//! elimination. The rules (TR13) are:
//!
//! 1. different classes never alias (the stack has no class: slots are SSA values on a trace);
//! 2. **array refs (TR13.3):** *must* only if the table refs are the same ref **and** the indices are the same value
//!    (same ref after folding, or the same `(base, offset)` normal form); *no* if the indices are provably different
//!    whatever the tables are (two different constants, or `x + c1` against `x + c2` with `c1 != c2`); *may* otherwise.
//!    Two different table refs may be one table (`a[i] = ..; ..= b[i]` with `a == b`);
//! 3. **upvalues (TR13.5):** the same `UREF` ref is *must*; every other pair *may* alias, including two indices of one
//!    closure (`debug.upvaluejoin`) and the same index of two closures;
//!    (P2 bundle 2, `uvsep`: two `UREF`s recorded on different real `UpVal`s may be moved over each other by forwarding / load CSE under a run-time
//!    `NE uref, uref` guard on the load's snapshot, [`MemLog::find_sep`]; the rule above stays the semantics of [`alias`]);
//! 4. table-header fields never change on a trace **of F1** (nothing can resize a table), so an `FLOAD` is
//!    the same value everywhere the table ref is the same ref. This holds only because F1 emits none of the
//!    ops of [`header_safe`]'s `false` arm (allocation, key insertion, concatenation: each can resize / rehash a table,
//!    change its flags or metatable, or run a finaliser); `opt_loop` refuses to optimise a trace that contains one
//!    (P7b), so the full build cannot inherit the rule silently. `HSTORE` is safe: it only overwrites the value of an
//!    existing key whose old value is not nil (its own guard), which moves no node and clears no flag bit;
//!    4b. **hash refs (F1, `Class::Hash`):** *must* for the same `HREFK` (same table ref and constant key), *no* for two
//!    different constant keys whatever the tables, *may* otherwise (two table refs can be one table);
//! 5. forwarding: a load returns the value of the nearest earlier *must* store if no *may* store lies between; else the
//!    nearest earlier *must* load under the same condition; else it stays a load. A forwarded load keeps its type (the
//!    store's value type) and loses its typecheck; a `nil` value is never forwarded (rule 8: a real `nil` load would
//!    need the metatable guard) and a value of another type than the load speculated is not forwarded either (the
//!    load stays and exits at run time, as the original did);
//! 6. dead stores are **not** eliminated (every exit observes the heap, TR13.9); `TBAR`s, calls and guards do not
//!    change any location this module tracks.
//!
//! The log ([`MemLog`]) is kept in emission order by the loop optimiser (`opt_loop`), which feeds it the instructions
//! of the pre-roll and of the peeled body, so a body load sees the pre-roll's stores and loads exactly like the earlier
//! ones of the body (docs/JIT_TRACE.md 6.4, TR24b).

use super::ir::*;
use super::mutants;

/// Result of an alias query.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Alias {
    /// The locations are provably different.
    No,
    /// Unknown.
    May,
    /// The same location.
    Must,
}

/// Memory class (TR13.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Class {
    /// Array-part elements (`ALOAD` / `ASTORE`).
    Array,
    /// Upvalue cells (`ULOAD` / `USTORE`).
    Upval,
    /// Table header fields (`FLOAD`).
    Hdr,
    /// Hash-part values of existing keys (`HLOAD` / `HSTORE` through `HREFK`, F1).
    Hash,
}

/// One logged access. Location fields by class: `Array`: `a` = table ref (or the base ref if it is not an
/// `FLOAD array`), `b` = index ref; `Upval`: `a` = the `UREF`; `Hdr`: `a` = table ref, `b` = field number (raw);
/// `Hash`: `a` = the `HREFK`.
#[derive(Clone, Copy, Debug)]
pub struct MemOp {
    /// The class.
    pub class: Class,
    /// Store (`true`) or load.
    pub store: bool,
    /// Location operand 1.
    pub a: Ref,
    /// Location operand 2.
    pub b: Ref,
    /// The stored value (store) or the load instruction's ref (load).
    pub val: Ref,
}

/// P2 bundle 2: the most guards [`MemLog::find_sep`] asks for per load.
pub const SEP_MAX: usize = 4;

/// P2 bundle 2: were the two `UREF`s recorded on different real `UpVal`s (their `aux` identities, see `Rec::uref`)?
pub fn uv_distinct(ir: &TraceIr, x: Ref, y: Ref) -> bool {
    if !TraceIr::is_ins(x) || !TraceIr::is_ins(y) {
        return false;
    }
    let (ix, iy) = (ir.at(x), ir.at(y));
    ix.op == TOp::Uref && iy.op == TOp::Uref && ix.aux != 0 && iy.aux != 0 && ix.aux != iy.aux
}

/// What a lookup found.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Found {
    /// The value of an earlier must-alias store.
    Store(Ref),
    /// An earlier must-alias load.
    Load(Ref),
}

/// The access log (emission order).
#[derive(Clone, Debug, Default)]
pub struct MemLog {
    /// Accesses, oldest first.
    pub ops: Vec<MemOp>,
}

/// `(base, offset)` normal form of an index: constants have base [`NOREF`]; `x + c` / `x - c` chains are flattened.
pub fn canon(ir: &TraceIr, mut r: Ref) -> (Ref, i64) {
    let mut off = 0i64;
    loop {
        if TraceIr::is_const(r) {
            return match ir.kval(r) {
                KVal::Int(i) => (NOREF, off.wrapping_add(i)),
                _ => (r, off),
            };
        }
        if r == NOREF || !TraceIr::is_ins(r) {
            return (r, off);
        }
        let i = ir.at(r);
        match (i.op, ir.as_int(i.b)) {
            (TOp::Add, Some(c)) => {
                off = off.wrapping_add(c);
                r = i.a;
            }
            (TOp::Sub, Some(c)) => {
                off = off.wrapping_sub(c);
                r = i.a;
            }
            _ => return (r, off),
        }
    }
}

/// The location of a memory instruction of `ir` (`Aload`, `Astore`, `Uload`, `Ustore`, `Fload`) as a log entry
/// without value; `None` when the address operand is not the expected `AREF` / `UREF`. A load that cannot be located
/// is emitted untracked; a **store** that cannot be located must go through [`MemLog::record_store`], which fails
/// closed (it may write anywhere, so it empties the log).
pub fn locate(ir: &TraceIr, i: &Ins) -> Option<MemOp> {
    let none = |class, store, a, b| MemOp {
        class,
        store,
        a,
        b,
        val: NOREF,
    };
    match i.op {
        TOp::Aload | TOp::Astore => {
            if !TraceIr::is_ins(i.a) {
                return None;
            }
            let ar = ir.at(i.a);
            if ar.op != TOp::Aref {
                return None;
            }
            let arr = ar.a;
            let tab = if TraceIr::is_ins(arr) {
                let f = ir.at(arr);
                if f.op == TOp::Fload && f.b == Field::Array as Ref {
                    f.a
                } else {
                    arr
                }
            } else {
                arr
            };
            Some(none(Class::Array, i.op == TOp::Astore, tab, ar.b))
        }
        TOp::Uload | TOp::Ustore => {
            if !TraceIr::is_ins(i.a) || ir.at(i.a).op != TOp::Uref {
                return None;
            }
            Some(none(Class::Upval, i.op == TOp::Ustore, i.a, NOREF))
        }
        TOp::Fload => Some(none(Class::Hdr, false, i.a, i.b)),
        TOp::Hload | TOp::Hstore => {
            if !TraceIr::is_ins(i.a) || !matches!(ir.at(i.a).op, TOp::Hrefk | TOp::Hrefv) {
                return None;
            }
            Some(none(Class::Hash, i.op == TOp::Hstore, i.a, NOREF))
        }
        _ => None,
    }
}

/// Alias relation of two accesses (TR13).
pub fn alias(ir: &TraceIr, x: &MemOp, y: &MemOp) -> Alias {
    let r = alias_raw(ir, x, y);
    if r == Alias::May && mutants::active("alias_may_as_no") {
        return Alias::No;
    }
    r
}

fn alias_raw(ir: &TraceIr, x: &MemOp, y: &MemOp) -> Alias {
    if x.class != y.class {
        return Alias::No;
    }
    match x.class {
        Class::Array => {
            let (bx, ox) = canon(ir, x.b);
            let (by, oy) = canon(ir, y.b);
            // provably different indices: whatever the tables are
            if bx == by && ox != oy {
                return Alias::No;
            }
            if bx == NOREF && by == NOREF && ox != oy {
                return Alias::No;
            }
            let same_index = bx == by && ox == oy;
            if mutants::active("alias_maytable_as_must") && same_index {
                return Alias::Must; // MUTANT: the same index on possibly different tables
            }
            if same_index && x.a == y.a {
                Alias::Must
            } else {
                Alias::May
            }
        }
        Class::Upval => {
            if mutants::active("alias_upval_by_index") {
                // MUTANT (LuaJIT's rule, unsound since `debug.upvaluejoin`): distinct upvalue indices are disjoint
                let (ux, uy) = (ir.at(x.a), ir.at(y.a));
                if let (Some(i), Some(j)) = (ir.as_int(ux.b), ir.as_int(uy.b)) {
                    if i != j {
                        return Alias::No;
                    }
                }
            }
            if x.a == y.a {
                Alias::Must
            } else {
                Alias::May
            }
        }
        Class::Hdr => {
            if x.b != y.b {
                Alias::No
            } else if x.a == y.a {
                Alias::Must
            } else {
                Alias::May
            }
        }
        Class::Hash => {
            // TR13.4: same table (the `FLOAD node` ref) and same constant key: must; different constant keys: no,
            // whatever the tables (a key lives at one node of one table; constants are interned, so two different
            // refs are two different strings); else may (two table refs can be one table)
            let (hx, hy) = (ir.at(x.a), ir.at(y.a));
            // N6: a `HREFV` names its key by an SSA value, which may hold any string: two different refs can be the same
            // string, so only two *constant* keys are provably different; a constant against a variable, or two different
            // variables, may alias (a store through a variable-key `HREFV` therefore kills the forwarding of every hash
            // load of every table, constant keys included). The same variable ref on
            // the same node vector is the same location.
            let dynamic = hx.op == TOp::Hrefv || hy.op == TOp::Hrefv;
            if dynamic && !mutants::active("alias_hrefv_as_no") {
                let same_key = hx.b == hy.b;
                if hx.b != hy.b && TraceIr::is_const(hx.b) && TraceIr::is_const(hy.b) {
                    return Alias::No;
                }
                if mutants::active("alias_hrefv_key_as_must") && (x.a == y.a || hx.a == hy.a) {
                    return Alias::Must; // MUTANT: different key refs of one table as the same location
                }
                return if same_key && hx.op == hy.op && (x.a == y.a || hx.a == hy.a) {
                    Alias::Must
                } else {
                    Alias::May
                };
            }
            if hx.b != hy.b {
                return Alias::No;
            }
            if mutants::active("alias_hashtab_as_must") {
                return Alias::Must; // MUTANT: the same key on possibly different tables
            }
            if x.a == y.a || hx.a == hy.a {
                Alias::Must
            } else {
                Alias::May
            }
        }
    }
}

/// May the instruction of this op leave table headers (array size / base, metatable, flags) and the array parts
/// of the heap as they are, and run no code? Exhaustive on purpose: a new op must be classified here. `false` ops
/// are barriers for every header-derived fact (the `FLOAD` CSE of rule 4, the bounds dedup); the loop optimiser
/// gives up on a trace that has one.
pub fn header_safe(op: TOp) -> bool {
    match op {
        TOp::Kint
        | TOp::Knum
        | TOp::Kpri
        | TOp::Kptr
        | TOp::Sload
        | TOp::Add
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
        | TOp::Fload
        | TOp::Aref
        | TOp::Aload
        | TOp::Astore
        | TOp::Uref
        | TOp::Uload
        | TOp::Ustore
        | TOp::Tbar
        // 5.4 `alimit` touch (F7a, TR30): moves only the array-limit hint in the array's first word, which no header
        // field of `Field` reads (`Asize` is the real size); a future `LEN` / hint load must be killed by it
        | TOp::Atouch
        | TOp::Calln
        | TOp::Loop
        | TOp::Phi
        | TOp::Poll
        | TOp::Budget
        | TOp::Hrefk
        | TOp::Hrefv
        | TOp::Hmiss
        | TOp::Hload
        // F1: a store to an EXISTING key whose old value is not nil (the op's own guard): the node vector, the array,
        // the metatable and the flag-cache of every table stay as they are (`invalidate_tm_cache` belongs to
        // insertions, which are `NEWREF`s)
        | TOp::Hstore
        // S0: adds to the collector's byte counters only / stack stores of the writeback
        | TOp::Tvirt
        | TOp::Sstore
        | TOp::Uvrel
        // N2b: the pop touches the frame registers and `thread.ci` only
        | TOp::Retf => true,
        // allocation (may run a GC step and so a finaliser), key insertion (rehash), string building: any of them
        // can change a header or run arbitrary code
        // F3: `Tlen` reads the array part and moves the length hint (5.4 `alimit`), so it is not a header-safe
        // reference point either; the optimiser keeps the recorded rolled trace around it
        // MUTANT `newref_header_safe`: the insertion (rehash: node vector and array move) keeps header facts alive
        TOp::Newref if mutants::active("newref_header_safe") => true,
        TOp::Newref | TOp::Tnew | TOp::Snew | TOp::Concat | TOp::Tlen | TOp::Tset | TOp::Tnext | TOp::Strto | TOp::Lib => false,
    }
}

/// BUNDLE-H: bit of the `level` argument of [`header_safe_ins`] that makes `TNEXT` header-safe (`TrLimits::tnextinl` bit 0).
pub const HDR_TNEXT: u32 = 0x100;

/// BUNDLE-1 (4): [`header_safe`] of one instruction, with the kind of a `LIB` call and the variant of a `TLEN` known. `level`
/// (`TrLimits::hdrlib`): 0 = as [`header_safe`]; 2 = also `LIB sub`, 3 = also `LIB char`, 4 = also `LIB tostring` (HDRSUB); 1 = also the string `TLEN` and the `LIB` kinds that only read (byte, string compares,
/// `type`, string-method lookup, ccalls check, ipairs end, `math.random`: they touch no table header and no array part).
/// Left as barriers: `NEWREF` / `TSET` / `TNEXT` (rehash, key insertion, traversal state), table `TLEN` (length hint), `SETMT` (metatable)
/// and the allocating ops `TNEW`, `SNEW`, `CONCAT`, `STRTO`. The class-A string makers `LIB sub` (level 2), `char` (3) and `tostring` (4)
/// (HDRSUB) are header-safe: their helpers allocate a string without a collector step (when one is due they do nothing and the trace
/// exits), so no finaliser runs inside a trace and no existing table changes. The old B8 miscompile of peeled `string.sub` loops was the
/// `cleanup` third-operand bug (fixed in 4ce16b7a, docs/JIT_PARKED.md B8); `tr_core::hs_sub_ring_matrix` is its regression test.
pub fn header_safe_ins(i: &Ins, level: u32) -> bool {
    if header_safe(i.op) {
        return true;
    }
    // BUNDLE-H (`tnextinl` bit 0, passed as bit 8 of `level`): `TNEXT` only reads the table and moves the heap's traversal hint, so it keeps the header facts
    // of every table alive (the loops of `pairs` / `next` peel)
    let (tnext, level) = (level & HDR_TNEXT != 0, level & 0xFF);
    if tnext && i.op == TOp::Tnext && !mutants::active("tnext_hdr_never") {
        return true;
    }
    if level == 0 || mutants::active("hdrlib_never") {
        return false;
    }
    match i.op {
        // BUNDLE-A (`libnum`): `STRTO` in tonumber mode parses a string and writes the exit area only (no allocation, no table)
        TOp::Strto if i.aux & 2 != 0 => true,
        TOp::Tlen => i.aux == 1 || i.aux == 2 || mutants::active("hdrlib_table_tlen"),
        TOp::Lib => match i.aux & 0xFF {
            lib::SETMT if mutants::active("hdrlib_setmt") => true,
            lib::BYTE | lib::STRMETH | lib::TYPENAME | lib::TYPE | lib::CCALLS | lib::IPAIRS_END | lib::STREQ | lib::STRLT | lib::STRLE | lib::RANDOM | lib::FIND | lib::FINDS | lib::FMTX => true,
            // HDRSUB: the class-A string makers (no collector step, no table touched: they only intern / allocate a string) from level 2
            lib::SUB if level >= 2 || mutants::active("hdrlib_sub_always") => true,
            lib::CHAR | lib::CHAR2 | lib::CHAR3 | lib::LOWER | lib::UPPER | lib::REVERSE | lib::REP | lib::REPS if level >= 3 => true,
            // BUNDLE-H: the generator step of `math.random()` touches no table and no array part
            lib::RANDF => true,
            // BUNDLE-C (`tfmt`): a string maker like `SUB`
            k if lib::is_fmt(k) && (level >= 2 || mutants::active("hdrlib_sub_always")) => true,
            lib::TOSTR if level >= 4 => true,
            _ => false,
        },
        TOp::Newref if mutants::active("hdrlib_newref") => true,
        _ => false,
    }
}

impl MemLog {
    /// Appends an access.
    pub fn push(&mut self, op: MemOp) {
        self.ops.push(op);
    }

    /// Logs the store `i` (an `Astore` / `Ustore` of `ir` whose value is `val`). A store whose location cannot be
    /// determined may write any location: fail closed, forget everything (later loads are not forwarded from, nor
    /// merged with, anything that precedes it).
    pub fn record_store(&mut self, ir: &TraceIr, i: &Ins, val: Ref) {
        match locate(ir, i) {
            Some(mut key) => {
                key.val = val;
                self.ops.push(key);
            }
            None => self.ops.clear(),
        }
    }

    /// P2 bundle 2 (`uvsep`): [`MemLog::find`] where a may-alias **upvalue** store whose `UREF` was recorded on a different real `UpVal` than the
    /// load's (different non-zero `aux` identities) does not block: the pair of `UREF` refs is appended to `sep`, and the caller must emit the guard
    /// `NE uref_store, uref_load` (on the load's snapshot) if it uses the result. At most [`SEP_MAX`] pairs; more give `None`.
    pub fn find_sep(&self, ir: &TraceIr, key: &MemOp, sep: &mut Vec<(Ref, Ref)>) -> Option<Found> {
        let mut blocked = false;
        for o in self.ops.iter().rev() {
            match alias(ir, o, key) {
                Alias::No => {}
                Alias::Must => {
                    if blocked {
                        return None;
                    }
                    return Some(if o.store { Found::Store(o.val) } else { Found::Load(o.val) });
                }
                Alias::May => {
                    if !o.store {
                        continue;
                    }
                    if o.class == Class::Upval && key.class == Class::Upval && uv_distinct(ir, o.a, key.a) {
                        if !sep.contains(&(o.a, key.a)) {
                            if sep.len() >= SEP_MAX {
                                return None;
                            }
                            sep.push((o.a, key.a));
                        }
                    } else {
                        blocked = true;
                    }
                }
            }
        }
        None
    }

    /// Looks up the value of a load of `key` (a [`locate`] result): the nearest must-alias access with no may-alias
    /// store between it and the end of the log (TR13.7).
    pub fn find(&self, ir: &TraceIr, key: &MemOp) -> Option<Found> {
        let mut blocked = false;
        for o in self.ops.iter().rev() {
            match alias(ir, o, key) {
                Alias::No => {}
                Alias::Must => {
                    let ignore = if o.store {
                        mutants::active("fwd_across_store")
                    } else {
                        mutants::active("cse_across_store")
                    };
                    if blocked && !ignore {
                        return None;
                    }
                    return Some(if o.store {
                        Found::Store(o.val)
                    } else {
                        Found::Load(o.val)
                    });
                }
                Alias::May => {
                    if o.store {
                        blocked = true;
                    }
                }
            }
        }
        None
    }
}

/// May a value of type `t` be forwarded to a load that speculated `want`? (Same type, not `nil`: TR13.8.)
pub fn forwardable(have: Ty, want: Ty) -> bool {
    have == want && have != Ty::Nil
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ir_with_idx() -> (TraceIr, Ref, Ref, Ref) {
        let mut ir = TraceIr::default();
        let x = ir.emit(Ins {
            op: TOp::Sload,
            t: Ty::Int,
            flags: 0,
            a: 1,
            b: NOREF,
            prev: NOREF,
            aux: 0,
        });
        let c1 = ir.kint(-1);
        let c2 = ir.kint(-2);
        let a = ir.emit(Ins {
            op: TOp::Add,
            t: Ty::Int,
            flags: 0,
            a: x,
            b: c1,
            prev: NOREF,
            aux: 0,
        });
        let b = ir.emit(Ins {
            op: TOp::Add,
            t: Ty::Int,
            flags: 0,
            a: x,
            b: c2,
            prev: NOREF,
            aux: 0,
        });
        (ir, x, a, b)
    }

    fn arr(t: Ref, i: Ref, store: bool) -> MemOp {
        MemOp {
            class: Class::Array,
            store,
            a: t,
            b: i,
            val: NOREF,
        }
    }

    #[test]
    fn array_alias_rules() {
        let (ir, x, a, b) = ir_with_idx();
        let (t1, t2) = (REF_BIAS + 100, REF_BIAS + 101);
        // same table, same index: must
        assert_eq!(
            alias(&ir, &arr(t1, a, true), &arr(t1, a, false)),
            Alias::Must
        );
        // different indices x-1 / x-2: no, whatever the tables
        assert_eq!(alias(&ir, &arr(t1, a, true), &arr(t1, b, false)), Alias::No);
        assert_eq!(alias(&ir, &arr(t1, a, true), &arr(t2, b, false)), Alias::No);
        // same index on different table refs: may (they can be one table)
        assert_eq!(
            alias(&ir, &arr(t1, a, true), &arr(t2, a, false)),
            Alias::May
        );
        // unrelated index refs: may
        assert_eq!(alias(&ir, &arr(t1, a, true), &arr(t1, x, false)), Alias::No);
        let k1 = 1u16;
        assert_eq!(
            alias(&ir, &arr(t1, k1, true), &arr(t1, x, false)),
            Alias::May
        );
        // different constants
        let mut ir2 = ir.clone();
        let (c5, c6) = (ir2.kint(5), ir2.kint(6));
        assert_eq!(
            alias(&ir2, &arr(t1, c5, true), &arr(t2, c6, false)),
            Alias::No
        );
        assert_eq!(
            alias(&ir2, &arr(t1, c5, true), &arr(t1, c5, false)),
            Alias::Must
        );
    }

    #[test]
    fn upvalue_alias_rules() {
        let mut ir = TraceIr::default();
        let cl = ir.konst(KVal::Ptr(1, PtrKind::LClosure));
        let (j0, j1) = (ir.kint(0), ir.kint(1));
        let mk = |ir: &mut TraceIr, j: Ref| {
            ir.emit(Ins {
                op: TOp::Uref,
                t: Ty::P64,
                flags: 0,
                a: cl,
                b: j,
                prev: NOREF,
                aux: 0,
            })
        };
        let (u0, u1) = (mk(&mut ir, j0), mk(&mut ir, j1));
        let up = |u: Ref, store: bool| MemOp {
            class: Class::Upval,
            store,
            a: u,
            b: NOREF,
            val: NOREF,
        };
        assert_eq!(alias(&ir, &up(u0, true), &up(u0, false)), Alias::Must);
        // distinct indices of one closure may be one upvalue (debug.upvaluejoin)
        assert_eq!(alias(&ir, &up(u0, true), &up(u1, false)), Alias::May);
        // classes never alias
        assert_eq!(
            alias(&ir, &up(u0, true), &arr(REF_BIAS + 9, j0, false)),
            Alias::No
        );
    }

    #[test]
    fn unlocatable_store_is_a_full_barrier() {
        let (mut ir, _x, a, _b) = ir_with_idx();
        let t = REF_BIAS + 100;
        let mut log = MemLog::default();
        let mut st = arr(t, a, true);
        st.val = 77;
        log.push(st);
        assert_eq!(log.find(&ir, &arr(t, a, false)), Some(Found::Store(77)));
        // an ASTORE whose address is a constant (not an AREF): no location
        let k = ir.kint(5);
        let bad = Ins {
            op: TOp::Astore,
            t: Ty::Nil,
            flags: 0,
            a: k,
            b: k,
            prev: NOREF,
            aux: 0,
        };
        assert!(locate(&ir, &bad).is_none());
        log.record_store(&ir, &bad, k);
        assert_eq!(log.find(&ir, &arr(t, a, false)), None);
        // same for a USTORE that does not address a UREF
        let mut log = MemLog::default();
        log.push(st);
        let bad_u = Ins {
            op: TOp::Ustore,
            ..bad
        };
        log.record_store(&ir, &bad_u, k);
        assert!(log.ops.is_empty());
    }

    #[test]
    fn every_op_is_classified() {
        for op in [TOp::Newref, TOp::Tnew, TOp::Snew, TOp::Concat, TOp::Tlen, TOp::Tset, TOp::Tnext, TOp::Strto, TOp::Lib] {
            assert!(!header_safe(op), "{op:?}");
        }
        for op in [
            TOp::Fload,
            TOp::Astore,
            TOp::Ustore,
            TOp::Tbar,
            TOp::Calln,
            TOp::Hload,
            TOp::Hstore,
            TOp::Atouch,
        ] {
            assert!(header_safe(op), "{op:?}");
        }
    }

    #[test]
    fn forwarding_scan() {
        let (ir, _x, a, b) = ir_with_idx();
        let t = REF_BIAS + 100;
        let mut log = MemLog::default();
        let mut st = arr(t, a, true);
        st.val = 77;
        log.push(st);
        // must-alias store forwards
        assert_eq!(log.find(&ir, &arr(t, a, false)), Some(Found::Store(77)));
        // a different index is no alias: the store is skipped, nothing found
        assert_eq!(log.find(&ir, &arr(t, b, false)), None);
        // a may-alias store in between blocks forwarding from an older store
        let mut st2 = arr(REF_BIAS + 101, a, true);
        st2.val = 88;
        log.push(st2);
        assert_eq!(log.find(&ir, &arr(t, a, false)), None);
        // ... and the newer one (a different table ref, same index) is only a may
        assert!(!forwardable(Ty::Nil, Ty::Nil) && forwardable(Ty::Num, Ty::Num));
        assert!(!forwardable(Ty::Int, Ty::Num));
    }
}
