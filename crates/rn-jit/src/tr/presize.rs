//! P1 (`presize`, docs/JIT_TRACE.md section 32): a group of short-string key insertions into a table the trace allocated, with nothing in
//! between that can exit or look at the table, inserts the first key through a helper that gives the empty table the hash part the
//! interpreter's growth reaches after the whole group, and the other keys as inline plans (A1) into its free nodes: one node-vector
//! allocation per object instead of one per growth step, and no helper call for the other keys.
//!
//! Exactness (TR29.1: content, `#t` and `next` order of every table identical to the interpreter's):
//! * **End state.** The recorder noted, at every helper `NEWREF` of a constant key into a table of this trace, whether the table was
//!   empty before and the VM's hash image and `flags` right after the insertion (`PresizeNote`). The pass simulates the presized
//!   group with the table's own `insertkey` (`rn_core::table::presize_sim`) and applies it only when the simulated final image equals
//!   the image the interpreter's own growth produced for the same key objects (two keys always match; three or more match when the
//!   growth steps' reinsertion order places colliding keys as insertion in order does). The image (node keys, chain links, empty
//!   values, `lastfree`) is a function of the key sequence and the string hashes alone, which are the same at every run of the trace
//!   (the keys are constants, the table is new and empty at the first insertion: the helper checks it at run time and does nothing
//!   otherwise, so the trace exits before the instruction).
//! * **No intermediate state is ever visible.** Between the first insertion and the last one the group holds only the group's
//!   `NEWREF`s, their `NE` guards (the first one's fails only when its helper did nothing: the table is still empty; the others are inline
//!   and assembled to nothing), the `HSTORE`s into the new nodes, `TBAR`s of the table and pure instructions that do not read the table
//!   (no other guard, no call, no allocation, no collector step, no snapshot use): no exit, side trace or observer sees the table
//!   between the insertions. The recorder's `__newindex` memo (`Rec::p1_memo_holds`) removes the repeated metatable guards that
//!   would otherwise sit there.
//! * **Accounting and allocator.** `TableRef::newref_presize` replays the interpreter's growth steps on the allocator (the same
//!   `alloc_counted` / `free_counted` calls, sizes and order): the accounted bytes, every memory-limit check and the free lists end as the
//!   interpreter leaves them, so later objects get the same addresses.
use super::ir::*;
use super::mutants;

/// Presized groups installed by this process (all states; statistics only, `RUSTYNAIL_JIT_VERBOSE`).
pub static GROUPS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Instructions without effects that the group may contain (operands computed between the insertions): arithmetic, conversions,
/// comparisons as values, field loads of other objects. No guard (the caller also checks `F_GUARD`), no call, no allocation, no store.
pub fn pure_op(op: TOp) -> bool {
    matches!(
        op,
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
            | TOp::Fload
            | TOp::Sload
    )
}

/// The short string of the constant key `k` (a `KPTR` raw address), if it is one.
fn key_of(ir: &TraceIr, k: Ref) -> Option<rn_core::string::StrRef> {
    if TraceIr::is_ins(k) {
        return None;
    }
    let KVal::Ptr(bits, PtrKind::Raw) = ir.kval(k) else {
        return None;
    };
    // SAFETY: a `NEWREF`'s key constant is a live interned short string the trace keeps alive (TR32 constants); `Value` is
    // `repr(C)` `{ tag word, payload }`, as `asm::newref_core` builds it.
    let v: rn_core::value::Value = unsafe {
        std::mem::transmute::<[u64; 2], rn_core::value::Value>([
            u64::from(rn_core::value::ctb(rn_core::value::LUA_VSHRSTR)),
            bits,
        ])
    };
    v.as_strref()
}

/// Runs the pass on `ir`: rewrites every group that qualifies and clears the remaining [`nrplan::NOTE`] tags. Returns the number of groups.
pub fn run(ir: &mut TraceIr) -> usize {
    if ir.presize.is_empty() {
        return 0;
    }
    let n = ir.ins.len();
    let mut groups = 0;
    let mut i = 0;
    while i < n {
        let first = ir.ins[i];
        if first.op != TOp::Newref
            || first.aux & nrplan::INLINE != 0
            || first.aux & nrplan::NOTE == 0
        {
            i += 1;
            continue;
        }
        let t = first.a;
        let mut members = vec![i];
        let mut j = i + 1;
        while j < n {
            let x = ir.ins[j];
            let in_group =
                |r: Ref| TraceIr::is_ins(r) && members.contains(&((r - REF_BIAS) as usize));
            let member = x.op == TOp::Newref
                && x.a == t
                && x.aux & nrplan::INLINE == 0
                && x.aux & nrplan::NOTE != 0;
            let ok = match x.op {
                TOp::Newref => member,
                TOp::Ne => in_group(x.a) && ir.kval(x.b) == KVal::Int(0) && !TraceIr::is_ins(x.b),
                TOp::Hstore => in_group(x.a) && x.flags & F_GUARD == 0,
                TOp::Tbar => x.a == t,
                // (a field load of the table itself would see the presized header)
                TOp::Fload => x.a != t,
                op => x.flags & F_GUARD == 0 && pure_op(op),
            } || mutants::active("p1_window_any");
            if !ok {
                break;
            }
            if member {
                members.push(j);
            }
            j += 1;
        }
        i = members.last().copied().unwrap_or(i) + 1;
        // every member's `NE` guard follows it directly (the recorder's shape)
        if members.len() < 2
            || members.iter().any(|&m| {
                let g = ir.ins.get(m + 1);
                !g.is_some_and(|g| {
                    g.op == TOp::Ne && g.a == REF_BIAS + m as Ref && g.flags & F_GUARD != 0
                })
            })
        {
            continue;
        }
        members.truncate(8);
        // (a group whose image differs may still have a prefix that matches: the rest then grows through the helper)
        for len in (2..=members.len()).rev() {
            if let Some(rewrite) = plan_group(ir, &members[..len]) {
                for (m, aux) in members.iter().zip(rewrite) {
                    ir.ins[*m].aux = aux;
                }
                groups += 1;
                break;
            }
        }
    }
    for ins in ir.ins.iter_mut() {
        if ins.op == TOp::Newref && ins.aux & nrplan::INLINE == 0 && ins.aux & nrplan::NOTE != 0 {
            ins.aux = 0;
        }
    }
    groups
}

/// The new `aux` of each member, or `None` when the group does not qualify (see the module documentation).
fn plan_group(ir: &TraceIr, members: &[usize]) -> Option<Vec<u32>> {
    let notes: Vec<&PresizeNote> = members
        .iter()
        .map(|&m| ir.presize.get((ir.ins[m].aux & nrplan::NOTE_MASK) as usize))
        .collect::<Option<_>>()?;
    if !notes[0].empty_before || notes[1..].iter().any(|n| n.empty_before) {
        return None;
    }
    let keys: Vec<rn_core::string::StrRef> = members
        .iter()
        .map(|&m| key_of(ir, ir.ins[m].b))
        .collect::<Option<_>>()?;
    for (a, k) in keys.iter().enumerate() {
        if keys[..a].iter().any(|o| o.as_ptr() == k.as_ptr()) {
            return None;
        }
    }
    let afters: Vec<&(u8, rn_core::table::HashImage)> = notes
        .iter()
        .map(|n| n.after.as_ref())
        .collect::<Option<_>>()?;
    let flags = afters[0].0;
    if afters.iter().any(|a| a.0 != flags) {
        return None;
    }
    let (lsize, plans, img) = rn_core::table::presize_sim(&keys, flags)?;
    if img != afters[afters.len() - 1].1 && !mutants::active("p1_image_check_skip") {
        return None;
    }
    let mut out = vec![nrplan::PRESIZE | u32::from(lsize)];
    for p in &plans {
        if p.flags != flags {
            return None;
        }
        out.push(nrplan::encode(p)?);
    }
    Some(out)
}
