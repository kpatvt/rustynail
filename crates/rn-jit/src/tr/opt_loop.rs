//! Loop optimisation by peeling (docs/JIT_TRACE.md 6.4, 7.8; TR24, TR24e) with memory forwarding (6.6, TR13, see
//! [`super::opt_mem`]).
//!
//! # What the pass does
//!
//! The recorder closes one *rolled* iteration (`loop_at = None`, 7.8): `POLL; BUDGET n; [entry checks]; body`, with
//! the slots the iteration wrote listed in [`Recorded::writeback`]. This pass re-emits that iteration twice through the
//! folding / CSE [`Builder`] and the memory log:
//!
//! * the **pre-roll** copy, in which every live-in slot is its own `SLOAD` (the stack at trace entry); then `LOOP`;
//! * the **body** copy, in which a live-in slot is *the pre-roll's final value of that slot* (`subst`, LuaJIT's
//!   copy-substitution): the body is exactly "one more iteration" (TR24a). Everything the body computes from
//!   invariant values CSEs to the pre-roll copy and disappears (hoisting, also of the guards that depend only on
//!   invariants); loads that no store between the pre-roll copy and the body copy may alias become the pre-roll load
//!   (the memory log sees the pre-roll's stores after the load and the body's stores before it, TR24b); loads that a
//!   must-alias store feeds are forwarded (store-to-load, also across the back-edge: the pre-roll store feeds the body
//!   load).
//!
//! A pre-roll ref `e` that the body uses stands for *`e` in the previous iteration*. If the body has its own copy of
//! `e` (`b(e) != e`) the value is loop carried and `e` gets a `PHI(e, b(e))` (slot PHIs and non-slot PHIs alike,
//! TR24e); every body use of `e` then names the PHI. The recorded `S_H` map is emitted for `POLL` / `BUDGET` after
//! `LOOP` and for the body copies of the entry guards (state at `H` of the next iteration, TR27b).
//!
//! The pass also leaves the **rolled** layout with intra-iteration forwarding when only `-fwd` is on (`-loop`).
//!
//! Anything the pass cannot do (a constant-false guard in the body, more than [`MAX_PHI`] PHIs, too many
//! instructions or snapshots, a PHI whose operand types differ) leaves the recorded rolled trace unchanged.

use super::ir::*;
use super::mutants;
use super::opt_mem::{self, Found, MemLog};
use super::ra::MAX_PHI;
use super::record::Recorded;
use super::snap;
use super::TraceOpts;
use crate::fx::{HashMap, HashSet};
use std::collections::BTreeMap;

/// The pass-list entry (`loop`, `fwd`): see the module documentation.
pub fn run(rec: &mut Recorded, opts: &TraceOpts) -> Result<(), String> {
    if rec.ir.loop_at.is_some() || !(opts.loop_opt || opts.fwd) || !rec.ir.segs.is_empty() {
        // (N2b: a trace with `RETF`s is a rolled side trace: slots, `UVREL` and the frame change under it; the passes below assume one root frame)
        return Ok(());
    }
    let peel = opts.loop_opt && shaped_for_peeling(&rec.ir);
    match transform(rec, opts, peel, false) {
        Ok(()) => Ok(()),
        // everything the optimiser gives up on keeps the recorded rolled trace
        Err(Gave::Up(why)) => {
            NOTE.with(|n| *n.borrow_mut() = format!("loop: gave up: {why}"));
            Ok(())
        }
    }
}

/// H1: forwarding and load CSE on a side or stitched trace (a rolled, loop-free segment): the rolled layout of [`run`] without the
/// peel. A trace that ends at a stitching exit has the always-false guard `EQ #0 #1` at its end, which [`Em::copy_one`] copies as is.
pub fn run_fwd(rec: &mut Recorded, opts: &TraceOpts) -> Result<(), String> {
    // N3a: a trace with `RETF`s is rolled and loop-free, so forwarding / load CSE apply (a `RETF` touches no heap memory; it cuts the `UVREL` chain)
    if rec.ir.loop_at.is_some() || !opts.fwd || mutants::active("sidefwd_off") {
        return Ok(());
    }
    match transform(rec, opts, false, true) {
        Ok(()) => Ok(()),
        Err(Gave::Up(why)) => {
            NOTE.with(|n| *n.borrow_mut() = format!("fwd: gave up: {why}"));
            Ok(())
        }
    }
}

thread_local! {
    /// The summary of the last [`run`] on this thread (tests and `VERBOSE=2`).
    static NOTE: std::cell::RefCell<String> = const { std::cell::RefCell::new(String::new()) };
}

/// The one-line summary of the last [`run`] of this thread (`peeled: 4 phis, 12 hoisted, 3 forwarded ...`).
pub fn last_note() -> String {
    NOTE.with(|n| n.borrow().clone())
}

/// A recorded iteration starts with `POLL, BUDGET` on snapshot 0 (the loop-top charge); anything else is not peeled.
fn shaped_for_peeling(ir: &TraceIr) -> bool {
    ir.ins.len() >= 2 && ir.ins[0].op == TOp::Poll && ir.ins[1].op == TOp::Budget
}

enum Gave {
    Up(String),
}

impl From<IrAbort> for Gave {
    fn from(a: IrAbort) -> Gave {
        Gave::Up(format!("{a:?}"))
    }
}

fn up<T>(s: impl Into<String>) -> Result<T, Gave> {
    Err(Gave::Up(s.into()))
}

/// Statistics of one run.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Stats {
    /// PHIs emitted (slot and non-slot).
    pub phis: usize,
    /// Pre-roll instructions the body reuses unchanged (loop invariant).
    pub hoisted: usize,
    /// Loads replaced by a stored value.
    pub forwarded: usize,
    /// Loads replaced by an earlier load.
    pub load_cse: usize,
    /// P2 bundle 2: `NE` upvalue-separation guards emitted (`uvsep`).
    pub uvsep_guards: usize,
}

struct Em<'a> {
    src: &'a TraceIr,
    rec: &'a Recorded,
    b: Builder,
    log: MemLog,
    fwd: bool,
    /// H1: `snap_of` is the identity (the snapshots are copied at the end, in order): [`run_fwd`].
    keep_snaps: bool,
    body: bool,
    /// `src` instruction index -> output ref of the copy being emitted.
    sub: Vec<Ref>,
    /// The pre-roll copy's `sub`, kept for the body.
    pre_sub: Vec<Ref>,
    kmap: HashMap<Ref, Ref>,
    snap_memo: HashMap<u32, u32>,
    snap_step: Vec<u32>,
    entry_snap_out: Option<u32>,
    /// Snapshots created while emitting the body (their maps hold previous-iteration refs).
    body_snaps: Vec<u32>,
    s_h: Option<u32>,
    s_h_charged: Option<u32>,
    /// Final value of every slot the iteration writes: `(slot, pre-roll ref, flags)`, by slot.
    loop_state: BTreeMap<u16, (Ref, u8)>,
    /// Pre-roll output ref -> the `src` instruction that created it.
    creator: HashMap<Ref, usize>,
    /// `ULT idx, bound` guards emitted so far, as `(canon(idx), bound)`: a later guard of the same value is redundant.
    bounds: Vec<((Ref, i64), Ref)>,
    /// BUNDLE-1: `TrLimits::hdrlib` (see `opt_mem::header_safe_ins`).
    hdrlib: u32,
    /// P2 bundle 2: `TrLimits::uvsep` (see `opt_mem::MemLog::find_sep`).
    uvsep: bool,
    /// The `LOOP` marker (body copy only).
    loop_ref: Ref,
    stats: Stats,
    /// BUNDLE-L A (`aidx`): peeled layout only; `aref_loaded[n]`: the `src` instruction `n` is an `AREF` with an `ALOAD` user.
    aidx: bool,
    peeling: bool,
    aref_loaded: Vec<bool>,
    /// BUNDLE-L B (`bhoist`): the `src` bounds guards (`ULT k + c, asize`) whose check the pre-roll guards of `plan_bhoist` cover, as
    /// `src index -> (bound src ref, k0, c)`.
    bhoist: bool,
    hoisted_ult: HashMap<usize, (Ref, Ref, i64)>,
    aidx_memo: HashMap<usize, u32>,
}

impl<'a> Em<'a> {
    fn new(rec: &'a Recorded, opts: &TraceOpts) -> Em<'a> {
        let mut b = Builder::new(opts.fold, opts.cse);
        // DYNMASK: every op that can rehash a table makes `copy_one` give up, so a dynamic-mask `HREFV` is stable across the trace
        b.cse_dyn_href = true;
        b.ir.head = rec.ir.head;
        b.ir.extent = rec.ir.extent;
        b.ir.depth = rec.ir.depth;
        b.ir.deps = rec.ir.deps.clone();
        b.ir.segs = rec.ir.segs.clone();
        // H1: the flags of a function-entry / recursion root (`run_fwd` is the first pass that sees them)
        b.ir.fn_root = rec.ir.fn_root;
        b.ir.rec_call = rec.ir.rec_call;
        b.ir.rec_ret = rec.ir.rec_ret;
        Em {
            src: &rec.ir,
            rec,
            b,
            log: MemLog::default(),
            fwd: opts.fwd,
            keep_snaps: false,
            hdrlib: opts.hdrlib,
            uvsep: opts.uvsep & 1 != 0,
            body: false,
            sub: vec![NOREF; rec.ir.ins.len()],
            pre_sub: Vec::new(),
            kmap: HashMap::default(),
            snap_memo: HashMap::default(),
            snap_step: Vec::new(),
            entry_snap_out: None,
            body_snaps: Vec::new(),
            s_h: None,
            s_h_charged: None,
            loop_state: BTreeMap::new(),
            creator: HashMap::default(),
            bounds: Vec::new(),
            loop_ref: NOREF,
            stats: Stats::default(),
            aidx: opts.aidx != 0,
            bhoist: opts.bhoist != 0,
            hoisted_ult: HashMap::default(),
            aidx_memo: HashMap::default(),
            peeling: false,
            aref_loaded: if opts.aidx != 0 {
                let mut v = vec![false; rec.ir.ins.len()];
                for i in &rec.ir.ins {
                    if i.op == TOp::Aload && TraceIr::is_ins(i.a) {
                        v[usize::from(i.a - REF_BIAS)] = true;
                    }
                }
                v
            } else {
                Vec::new()
            },
        }
    }

    /// BUNDLE-L A: the `aux` of an `AREF` at the (output) index `u`: the complemented index as an IR value. `u = k + c` (or `k - c`) gives `NEG k`
    /// with the displacement `-c - 1` (`~(k + c) == -k + (-c - 1)`, wrapping), so the loads of one `k` with different `c` share one `NEG`; any other
    /// index gives `BNOT u`. 0 when the index is a constant (the assembler addresses a constant index directly).
    fn aidx_aux(&mut self, u: Ref, n_src: usize) -> Result<u32, Gave> {
        if !TraceIr::is_ins(u) {
            return Ok(0);
        }
        let ui = *self.b.ir.at(u);
        let split = match ui.op {
            // (a body copy may split only an index made in the body: a pre-roll ref used by the body is renamed to its PHI as a whole, its operands are the
            // pre-roll iteration's values, not the previous iteration's)
            TOp::Add | TOp::Sub if ui.t == Ty::Int && TraceIr::is_ins(ui.a) && (!self.body || u > self.loop_ref) => self.b.ir.as_int(ui.b).and_then(|c| {
                let d = if ui.op == TOp::Add { c.wrapping_neg().wrapping_sub(1) } else { c.wrapping_sub(1) };
                i16::try_from(d).ok().map(|d| (ui.a, d))
            }),
            _ => None,
        };
        if mutants::active("aidx_body_reuse") && self.body {
            if let Some(&x) = self.aidx_memo.get(&n_src) {
                return Ok(x);
            }
        }
        let (n, d) = match split {
            Some((k, d)) => (self.b.emit_flags(TOp::Neg, Ty::Int, 0, k, NOREF, 0)?, d),
            _ => (self.b.emit_flags(TOp::Bnot, Ty::Int, 0, u, NOREF, 0)?, 0),
        };
        if !TraceIr::is_ins(n) {
            return Ok(0);
        }
        let x = aref_aux(n, d);
        if mutants::active("aidx_body_reuse") {
            // MUTANT: the complemented index of the first copy (the pre-roll's) serves every later copy of the same source index
            if !self.body {
                self.aidx_memo.insert(n_src, x);
            }
        }
        Ok(x)
    }


    /// BUNDLE-L B (`bhoist`): after the pre-roll copy of a counted `for` loop, emits the guards that make the body's bounds checks of the
    /// induction index redundant, and remembers which `src` checks they cover (`hoisted_ult`).
    ///
    /// A loop qualifies when the iteration has the guard `NE count, 0` of slot `cs` (the loop continues) and the pre-roll left `count - 1` as the
    /// final value of `cs`, and a bounds guard `ULT k + c, bound` of a slot `s` whose final value is `k + 1`. Body iteration `i` (0-based) then
    /// reads index `k0 + i + c` and runs only if `i <= count0` (`count0` = the final value of `cs`; the guard of iteration `j` saw
    /// `count0 - j != 0` for every `j < i`), so `count0 ULT bound` (no wrap: `bound < 2^32`), `k0 + c ULT bound` and
    /// `k0 + count0 + c ULT bound` cover every index of every body iteration. The bound must be the same SSA value in the body (checked at the copy).
    /// The three guards exit on a snapshot of the state at the loop top (the `S_H` shape, not renamed to the PHIs).
    fn plan_bhoist(&mut self) -> Result<(), Gave> {
        let src = self.src;
        // the loop-continue guard
        let mut cnt0: Option<Ref> = None;
        for i in src.ins.iter() {
            if i.op != TOp::Ne || i.flags & F_GUARD == 0 || src.as_int(i.b) != Some(0) || !TraceIr::is_ins(i.a) {
                continue;
            }
            let sl = src.ins[usize::from(i.a - REF_BIAS)];
            if sl.op != TOp::Sload || sl.t != Ty::Int {
                continue;
            }
            let Some(&(r0, _)) = self.loop_state.get(&sl.a) else { continue };
            if !TraceIr::is_ins(r0) {
                continue;
            }
            let f = *self.b.ir.at(r0);
            let pre = self.sub[usize::from(i.a - REF_BIAS)];
            if f.op == TOp::Add && f.a == pre && self.b.ir.as_int(f.b) == Some(-1) {
                cnt0 = Some(r0);
                break;
            }
        }
        let Some(cnt0) = cnt0 else { return Ok(()) };
        // the bounds guards
        let mut found: Vec<(usize, Ref, Ref, i64)> = Vec::new();
        for (n, i) in src.ins.iter().enumerate() {
            if i.op != TOp::Ult || i.flags & F_GUARD == 0 || !TraceIr::is_ins(i.a) || !TraceIr::is_ins(i.b) {
                continue;
            }
            let bd = src.ins[usize::from(i.b - REF_BIAS)];
            if bd.op != TOp::Fload || bd.b != Field::Asize as Ref {
                continue;
            }
            // `a = Sload s` or `Add(Sload s, #c)`
            let ai = src.ins[usize::from(i.a - REF_BIAS)];
            let (sref, c) = match ai.op {
                TOp::Sload => (i.a, 0),
                TOp::Add if ai.t == Ty::Int && TraceIr::is_ins(ai.a) => match src.as_int(ai.b) {
                    Some(c) => (ai.a, c),
                    None => continue,
                },
                _ => continue,
            };
            let sl = src.ins[usize::from(sref - REF_BIAS)];
            if sl.op != TOp::Sload || sl.t != Ty::Int {
                continue;
            }
            let Some(&(k0, _)) = self.loop_state.get(&sl.a) else { continue };
            if !TraceIr::is_ins(k0) {
                continue;
            }
            let f = *self.b.ir.at(k0);
            if f.op == TOp::Add && f.a == self.sub[usize::from(sref - REF_BIAS)] && (self.b.ir.as_int(f.b) == Some(1) || (mutants::active("bhoist_step_any") && self.b.ir.as_int(f.b).is_some_and(|x| x > 1))) && i16::try_from(c).is_ok() {
                found.push((n, i.b, k0, c));
            }
        }
        if found.is_empty() {
            return Ok(());
        }
        // one snapshot at the loop top for all the guards of this loop
        let sn = Snap {
            pc: self.src.head.1,
            frame: 0,
            nframes: 0,
            refund: 0,
            land: Land::Pre,
            ..Snap::default()
        };
        let ents = self.state_entries();
        let hs = self.new_snap(sn, &ents, &[], 0)?;
        let aux = guard_aux(hs, 0);
        let mut done_cnt: Vec<Ref> = Vec::new();
        let mut done_ult: Vec<(Ref, Ref, i64)> = Vec::new();
        for &(n, bsrc, k0, c) in &found {
            let bound = self.sub[usize::from(bsrc - REF_BIAS)];
            if bound == NOREF || !TraceIr::is_ins(bound) {
                continue;
            }
            if !done_cnt.contains(&bound) {
                done_cnt.push(bound);
                if !mutants::active("bhoist_no_count_guard") {
                    self.b.emit_flags(TOp::Ult, Ty::I64, F_GUARD, cnt0, bound, aux)?;
                }
            }
            if !done_ult.contains(&(bound, k0, c)) {
                done_ult.push((bound, k0, c));
                let kc = self.b.ir.kint(c);
                let first = self.b.emit_flags(TOp::Add, Ty::Int, 0, k0, kc, 0)?;
                if !mutants::active("bhoist_no_first_guard") {
                    self.b.emit_flags(TOp::Ult, Ty::I64, F_GUARD, first, bound, aux)?;
                }
                let sum = self.b.emit_flags(TOp::Add, Ty::Int, 0, k0, cnt0, 0)?;
                let c2 = if mutants::active("bhoist_last_c_off") { c - 1 } else { c };
                let kc2 = self.b.ir.kint(c2);
                let last = self.b.emit_flags(TOp::Add, Ty::Int, 0, sum, kc2, 0)?;
                if !mutants::active("bhoist_no_last_guard") {
                    self.b.emit_flags(TOp::Ult, Ty::I64, F_GUARD, last, bound, aux)?;
                }
            }
            self.hoisted_ult.insert(n, (bsrc, k0, c));
        }
        Ok(())
    }

    /// BUNDLE-L B: is the body copy of the `src` bounds guard `n` (`ULT a, bb` in the output) covered by the guards of [`Em::plan_bhoist`]? The index must be
    /// `k0 + c` as planned (`k0` the pre-roll's final value of the induction slot, the same ref the body's `SLOAD` stands for) and the bound the very
    /// SSA value of the pre-roll (loop invariant).
    fn bhoist_covers(&self, n: usize, a: Ref, bb: Ref) -> bool {
        let Some(&(bsrc, k0, c)) = self.hoisted_ult.get(&n) else { return false };
        if !self.body || !TraceIr::is_ins(bb) {
            return false;
        }
        // MUTANT `bhoist_bound_variant`: the bound need not be the pre-roll's SSA value (a table loaded in the loop is covered by guards on another table)
        if mutants::active("bhoist_bound_variant") {
            return true;
        }
        if bb != self.pre_sub[usize::from(bsrc - REF_BIAS)] || bb >= self.loop_ref {
            return false;
        }
        if c == 0 {
            return a == k0;
        }
        TraceIr::is_ins(a) && {
            let x = self.b.ir.at(a);
            x.op == TOp::Add && x.a == k0 && self.b.ir.as_int(x.b) == Some(c)
        }
    }

    /// A `src` operand as an output ref (constants are re-interned).
    fn operand(&mut self, r: Ref) -> Ref {
        if r == NOREF {
            NOREF
        } else if r < REF_BIAS {
            if let Some(&k) = self.kmap.get(&r) {
                return k;
            }
            let k = self.b.ir.konst(self.src.k[usize::from(r) - 1]);
            self.kmap.insert(r, k);
            k
        } else {
            self.sub[usize::from(r - REF_BIAS)]
        }
    }

    fn new_snap(
        &mut self,
        s: Snap,
        entries: &[SnapEntry],
        links: &[FrameLink],
        step: u32,
    ) -> Result<u32, Gave> {
        if self.b.ir.snaps.len() >= MAX_SNAPS - 2 {
            return up("too many snapshots");
        }
        let n = self.b.ir.snapshot(s, entries);
        self.b.ir.frames.extend_from_slice(links);
        self.snap_step.push(step);
        Ok(n)
    }

    /// The output snapshot for the `src` snapshot `old` in the current copy (memoised per copy).
    fn snap_of(&mut self, old: u32) -> Result<u32, Gave> {
        if self.keep_snaps {
            return Ok(old);
        }
        if let Some(&s) = self.snap_memo.get(&old) {
            return Ok(s);
        }
        let s = if self.body {
            self.body_snap(old)?
        } else {
            let sn = self.src.snaps[old as usize];
            let ents: Vec<SnapEntry> = self
                .src
                .snap_entries(old)
                .to_vec()
                .into_iter()
                .map(|e| SnapEntry {
                    r: self.operand(e.r),
                    ..e
                })
                .collect();
            let links = snap::frame_links(self.src, old).to_vec();
            let n = self.new_snap(sn, &ents, &links, self.rec.snap_step[old as usize])?;
            if Some(old) == self.rec.entry_snap {
                self.entry_snap_out = Some(n);
            }
            n
        };
        self.snap_memo.insert(old, s);
        Ok(s)
    }

    /// The state at `H` at the top of the next iteration: every slot the iteration writes, from its pre-roll value
    /// (which names the PHI after the relayout).
    fn state_entries(&self) -> Vec<SnapEntry> {
        self.loop_state
            .iter()
            .map(|(&slot, &(r, flags))| SnapEntry { slot, r, flags })
            .collect()
    }

    /// `S_H`: `POLL` / `BUDGET` of the loop top (uncharged), or with `charged` the body copy of an entry guard
    /// (the charge `n` of this iteration taken, `refund = n`).
    fn s_h(&mut self, charged: bool) -> Result<u32, Gave> {
        let cached = if charged { self.s_h_charged } else { self.s_h };
        if let Some(s) = cached {
            return Ok(s);
        }
        let sn = Snap {
            pc: self.src.head.1,
            frame: 0,
            nframes: 0,
            refund: if charged { self.rec.n as i32 } else { 0 },
            land: Land::Pre,
            ..Snap::default()
        };
        let ents = self.state_entries();
        let n = self.new_snap(sn, &ents, &[], 0)?;
        self.body_snaps.push(n);
        if charged {
            self.s_h_charged = Some(n);
        } else {
            self.s_h = Some(n);
        }
        Ok(n)
    }

    /// The body copy of snapshot `old`: the slots of the pre-roll map with substituted values, plus every slot the
    /// previous iterations wrote and this one has not written yet (the stack is stale for them, no writeback stores).
    fn body_snap(&mut self, old: u32) -> Result<u32, Gave> {
        if Some(old) == self.rec.entry_snap {
            return self.s_h(true);
        }
        let sn = self.src.snaps[old as usize];
        let mut seen: HashSet<u16> = HashSet::default();
        let mut ents: Vec<SnapEntry> = Vec::new();
        for e in self.src.snap_entries(old).to_vec() {
            seen.insert(e.slot);
            ents.push(SnapEntry {
                r: self.operand(e.r),
                ..e
            });
        }
        for e in self.state_entries() {
            if !seen.contains(&e.slot) {
                ents.push(e);
            }
        }
        let links = snap::frame_links(self.src, old).to_vec();
        let n = self.new_snap(sn, &ents, &links, self.rec.snap_step[old as usize])?;
        self.body_snaps.push(n);
        Ok(n)
    }

    fn copy_guard_aux(&mut self, i: &Ins) -> Result<u32, Gave> {
        let s = self.snap_of(aux_snap(i.aux))?;
        Ok(guard_aux(s, aux_bits(i.aux)))
    }

    /// In the body, `(pre-roll ADD x c1) + c2` is **not** reassociated to `x + (c1 + c2)`: that would make the body use
    /// `x` (an older value) next to the pre-roll's `x + c1` and double every induction variable's PHIs and moves. The
    /// alias analysis and the bounds-check elimination see through the chain by their own normal form
    /// ([`opt_mem::canon`]), which is all the folding was needed for.
    fn no_reassoc(&mut self, op: TOp, t: Ty, a: Ref, b: Ref) -> Result<Option<Ref>, Gave> {
        if !self.body || self.loop_ref == NOREF || !matches!(op, TOp::Add | TOp::Sub) {
            return Ok(None);
        }
        let (Some(c), true) = (self.b.ir.as_int(b), TraceIr::is_ins(a) && a < self.loop_ref) else {
            return Ok(None);
        };
        if !matches!(self.b.ir.at(a).op, TOp::Add | TOp::Sub) {
            return Ok(None);
        }
        let c = if op == TOp::Sub { c.wrapping_neg() } else { c };
        if c == 0 {
            return Ok(Some(a));
        }
        let k = self.b.ir.kint(c);
        let f = self.b.fold;
        self.b.fold = false;
        let r = self.b.emit_flags(TOp::Add, t, 0, a, k, 0);
        self.b.fold = f;
        Ok(Some(r?))
    }

    fn bounds_key(&self, a: Ref, b: Ref) -> ((Ref, i64), Ref) {
        // the bound (the array size of one table) is part of the key: `t1[i]` does not cover `t2[i]`
        let b = if mutants::active("bounds_key_no_bound") {
            NOREF // MUTANT: dedup against any bound
        } else {
            b
        };
        (opt_mem::canon(&self.b.ir, a), b)
    }

    fn bounds_known(&self, a: Ref, b: Ref) -> bool {
        self.b.cse && self.bounds.contains(&self.bounds_key(a, b))
    }

    fn note_bounds(&mut self, a: Ref, b: Ref) {
        let k = self.bounds_key(a, b);
        self.bounds.push(k);
    }

    /// Emits (or reuses) the output of `src` instruction `n` and records it in `sub`.
    fn copy_one(&mut self, n: usize) -> Result<(), Gave> {
        let i = self.src.ins[n];
        let before = self.b.ir.ins.len();
        // TR13.4 (header fields never change on a trace) and the bounds dedup hold only without these ops: give up
        if !opt_mem::header_safe_ins(&i, self.hdrlib) && !mutants::active("hdr_op_no_barrier") {
            return up(format!("header-mutating op {:?}", i.op));
        }
        let r: Ref = match i.op {
            TOp::Poll | TOp::Budget => {
                if self.body {
                    return Ok(()); // the loop-top pair is emitted by `emit_body_prefix`
                }
                let aux = self.copy_guard_aux(&i)?;
                self.b.emit_raw(Ins {
                    aux,
                    prev: NOREF,
                    ..i
                })?
            }
            TOp::Sload => {
                if self.body {
                    // the live-in value of the next iteration
                    let slot = i.a;
                    let pre = self.pre_sub[n];
                    let stale = mutants::active("peel_subst_stale");
                    let r = match self.loop_state.get(&slot) {
                        Some(&(e, _)) if !stale => e,
                        _ => pre,
                    };
                    self.sub[n] = r;
                    return Ok(());
                }
                let aux = if i.flags & F_GUARD != 0 {
                    self.copy_guard_aux(&i)?
                } else {
                    i.aux
                };
                self.b.emit_raw(Ins {
                    aux,
                    prev: NOREF,
                    ..i
                })?
            }
            TOp::Kint | TOp::Knum | TOp::Kpri | TOp::Kptr => {
                if self.body {
                    self.sub[n] = self.pre_sub[n];
                    return Ok(());
                }
                let a = self.operand(i.a);
                self.b.emit_raw(Ins {
                    a,
                    prev: NOREF,
                    ..i
                })?
            }
            TOp::Fload => {
                let a = self.operand(i.a);
                let ins = Ins {
                    a,
                    prev: NOREF,
                    ..i
                };
                match self.fwd.then(|| self.b.find(&ins)).flatten() {
                    Some(r) => r,
                    None => self.b.emit_raw(ins)?,
                }
            }
            TOp::Retf => {
                let a = self.operand(i.a);
                let aux = self.copy_guard_aux(&i)?;
                self.b.emit_retf(a, i.b, aux)?
            }
            TOp::Aload | TOp::Uload | TOp::Hload => {
                let a = self.operand(i.a);
                let probe = Ins {
                    a,
                    prev: NOREF,
                    ..i
                };
                let mut hit = None;
                let mut sep: Vec<(Ref, Ref)> = Vec::new();
                if self.fwd {
                    if let Some(key) = opt_mem::locate(&self.b.ir, &probe) {
                        // P2 bundle 2 (`uvsep`): a typed (snapshot-carrying) upvalue load may move over stores to upvalues that were different
                        // `UpVal`s when recorded, under run-time `NE` guards on the load's snapshot
                        let found = if self.uvsep && i.op == TOp::Uload && i.flags & F_GUARD != 0 {
                            self.log.find_sep(&self.b.ir, &key, &mut sep)
                        } else {
                            self.log.find(&self.b.ir, &key)
                        };
                        match found {
                            Some(Found::Store(v)) => {
                                if opt_mem::forwardable(self.b.ir.ty_of(v), i.t) {
                                    self.stats.forwarded += 1;
                                    if i.flags & F_OBS != 0 && TraceIr::is_ins(v) {
                                        self.b.ir.ins[usize::from(v - REF_BIAS)].flags |= F_OBS;
                                    }
                                    hit = Some(v);
                                }
                            }
                            Some(Found::Load(l)) => {
                                let li = *self.b.ir.at(l);
                                let typed = li.flags & F_GUARD != 0 && aux_bits(li.aux) & 1 != 0;
                                if typed && li.t == i.t {
                                    self.stats.load_cse += 1;
                                    self.b.ir.ins[usize::from(l - REF_BIAS)].flags |= i.flags & F_OBS;
                                    hit = Some(l);
                                }
                            }
                            None => {}
                        }
                    }
                }
                if hit.is_some() && !sep.is_empty() {
                    for (k, (us, ul)) in sep.into_iter().enumerate() {
                        if mutants::active("uvsep_no_guard") || (k > 0 && mutants::active("uvsep_first_guard_only")) {
                            break;
                        }
                        let aux = self.copy_guard_aux(&i)?;
                        self.b.emit_flags(TOp::Ne, Ty::I64, F_GUARD, us, ul, aux)?;
                        self.stats.uvsep_guards += 1;
                    }
                }
                match hit {
                    Some(v) => v,
                    None => {
                        let aux = if i.flags & F_GUARD != 0 {
                            self.copy_guard_aux(&i)?
                        } else {
                            i.aux
                        };
                        let ins = Ins { aux, ..probe };
                        let r = self.b.emit_raw(ins)?;
                        if let Some(mut key) = opt_mem::locate(&self.b.ir, &ins) {
                            key.val = r;
                            self.log.push(key);
                        }
                        r
                    }
                }
            }
            TOp::Astore | TOp::Ustore => {
                let a = self.operand(i.a);
                let bb = self.operand(i.b);
                let ins = Ins {
                    a,
                    b: bb,
                    prev: NOREF,
                    ..i
                };
                let r = self.b.emit_raw(ins)?;
                // an unlocatable store empties the log (fails closed)
                self.log.record_store(&self.b.ir, &ins, bb);
                r
            }
            TOp::Hstore => {
                let a = self.operand(i.a);
                let bb = self.operand(i.b);
                let mut ins = Ins {
                    a,
                    b: bb,
                    prev: NOREF,
                    ..i
                };
                if ins.flags & F_GUARD != 0 {
                    ins.aux = self.copy_guard_aux(&i)?;
                    // the old-value check (bit 0) is redundant when the nearest must-alias access of the same node is a
                    // typed non-nil load or a store of a non-nil value (F1; same conditions as a forwarded load)
                    if self.fwd && aux_bits(ins.aux) & 1 != 0 {
                        if let Some(key) = opt_mem::locate(&self.b.ir, &ins) {
                            let known = match self.log.find(&self.b.ir, &key) {
                                Some(Found::Store(v)) => self.b.ir.ty_of(v) != Ty::Nil,
                                Some(Found::Load(l)) => {
                                    let li = *self.b.ir.at(l);
                                    li.flags & F_GUARD != 0
                                        && aux_bits(li.aux) & 1 != 0
                                        && li.t != Ty::Nil
                                }
                                None => false,
                            };
                            if known || mutants::active("hstore_oldcheck_dropped") {
                                self.stats.load_cse += 1;
                                ins.flags &= !F_GUARD;
                                ins.aux = 0;
                            }
                        }
                    }
                }
                let r = self.b.emit_raw(ins)?;
                self.log.record_store(&self.b.ir, &ins, bb);
                r
            }
            TOp::Tbar => {
                let a = self.operand(i.a);
                let bb = self.operand(i.b);
                self.b.emit_raw(Ins {
                    a,
                    b: bb,
                    prev: NOREF,
                    ..i
                })?
            }
            TOp::Atouch if self.body && mutants::active("atouch_hoisted") => {
                // MUTANT (5.4): the body copy of an `ATOUCH` is dropped as if the pre-roll copy covered it (a hoist):
                // the touches of iterations 2.. are lost
                self.sub[n] = NOREF;
                return Ok(());
            }
            TOp::Loop | TOp::Phi => return up("stray loop op in a rolled trace"),
            // BUNDLE-G2: the C-stack guard of an inlined `pcall` (`LIB CCALLS k`) reads `nCcalls`, which nothing on a trace changes (the
            // class-A helpers and the stitching calls leave it as they found it, and a new entry re-runs the pre-roll copy): the body
            // copy is the pre-roll copy (and its `NE` guard, a CSE of the hoisted one, disappears with it).
            TOp::Lib if self.body && i.aux & 0xFF == lib::CCALLS && !mutants::active("g2_ccalls_no_hoist") => {
                let a = self.operand(i.a);
                let bb = self.operand(i.b);
                let probe = Ins { a, b: bb, prev: NOREF, ..i };
                match self.b.find(&probe) {
                    Some(r) => r,
                    None => self.b.emit_flags(i.op, i.t, i.flags & F_GUARD, a, bb, i.aux)?,
                }
            }
            TOp::Aref if self.aidx && self.peeling && self.aref_loaded[n] && !TraceIr::is_const(self.operand(i.b)) => {
                let a = self.operand(i.a);
                let u = self.operand(i.b);
                let aux = self.aidx_aux(u, n)?;
                self.b.emit_flags(TOp::Aref, i.t, 0, a, u, aux)?
            }
            op if op.is_cse() || i.flags & F_GUARD != 0 || op.always_guard() => {
                let (ra, rb) = op.ref_operands();
                let a = if ra { self.operand(i.a) } else { i.a };
                let bb = if rb { self.operand(i.b) } else { i.b };
                let guard = i.flags & F_GUARD != 0 || op.always_guard();
                if op == TOp::Eq && guard && TraceIr::is_const(a) && TraceIr::is_const(bb) && a != bb && !mutants::active("end_exit_folded") {
                    // H1: the always-taken exit that ends a side / stitched recording (`EQ #0 #1`): copied, not folded (a guard proven
                    // false is `ConstGuard`)
                    let aux = self.copy_guard_aux(&i)?;
                    self.b.emit_raw(Ins {
                        a,
                        b: bb,
                        aux,
                        prev: NOREF,
                        ..i
                    })?
                } else if let Some(r) = self.no_reassoc(op, i.t, a, bb)? {
                    r
                } else if op == TOp::Ult && guard && self.bounds_known(a, bb) {
                    // the same bounds check (of the same value) already dominates this one
                    NOREF
                } else if op == TOp::Ult && guard && self.bhoist && self.bhoist_covers(n, a, bb) {
                    // BUNDLE-L B: covered by the guards at the end of the pre-roll
                    NOREF
                } else {
                    let aux = if guard {
                        self.copy_guard_aux(&i)?
                    } else {
                        i.aux
                    };
                    let r = self.b.emit_flags(op, i.t, i.flags & F_GUARD, a, bb, aux)?;
                    if op == TOp::Ult && guard {
                        self.note_bounds(a, bb);
                    }
                    r
                }
            }
            _ => {
                let (ra, rb) = i.op.ref_operands();
                let a = if ra { self.operand(i.a) } else { i.a };
                let bb = if rb { self.operand(i.b) } else { i.b };
                // BUNDLE-1: a third operand (`CONCAT` of three, `LIB` random) is renumbered like the others
                let aux = match i.third() {
                    Some(r) if !mutants::active("hdrlib_third_stale") => i.aux_with_third(self.operand(r)),
                    _ => i.aux,
                };
                self.b
                    .emit_flags(i.op, i.t, i.flags & F_GUARD, a, bb, aux)?
            }
        };
        if !self.body
            && self.b.ir.ins.len() > before
            && TraceIr::is_ins(r)
            && r == REF_BIAS + (self.b.ir.ins.len() - 1) as Ref
        {
            self.creator.insert(r, n);
        }
        self.sub[n] = r;
        Ok(())
    }
}

/// The generic result of both layouts.
struct Out {
    ir: TraceIr,
    snap_step: Vec<u32>,
    entry_snap: Option<u32>,
    writeback: Vec<SnapEntry>,
    stats: Stats,
    /// H1: the snapshots keep their numbers ([`run_fwd`]): [`cleanup`] drops none of them.
    keep_all: bool,
}

fn transform(rec: &mut Recorded, opts: &TraceOpts, peel: bool, keep_snaps: bool) -> Result<(), Gave> {
    let out = if peel {
        peel_loop(rec, opts)?
    } else {
        roll(rec, opts, keep_snaps)?
    };
    let note = format!(
        "{}: {} phis, {} hoisted, {} forwarded, {} loads cse'd",
        if peel { "peeled" } else { "rolled+fwd" },
        out.stats.phis,
        out.stats.hoisted,
        out.stats.forwarded,
        out.stats.load_cse
    );
    NOTE.with(|n| *n.borrow_mut() = note);
    if peel {
        super::sink::PEELED.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        super::sink::PHIS.fetch_add(out.stats.phis as u64, std::sync::atomic::Ordering::Relaxed);
    }
    rec.ir = out.ir;
    rec.snap_step = out.snap_step;
    rec.entry_snap = out.entry_snap;
    rec.writeback = out.writeback;
    Ok(())
}

/// Emits one iteration through the pre-roll machinery (shared by both layouts).
fn emit_preroll(em: &mut Em<'_>) -> Result<(), Gave> {
    // snapshot 0 first: POLL and BUDGET use it
    for n in 0..em.src.ins.len() {
        em.copy_one(n)?;
    }
    // the loop state: the final value of every slot the iteration wrote
    for e in &em.rec.writeback {
        let r = em.operand(e.r);
        em.loop_state.insert(e.slot, (r, e.flags));
    }
    Ok(())
}

/// Layout 1: the rolled trace with intra-iteration forwarding (`-loop` with `-fwd`).
fn roll(rec: &Recorded, opts: &TraceOpts, keep_snaps: bool) -> Result<Out, Gave> {
    let mut em = Em::new(rec, opts);
    em.keep_snaps = keep_snaps;
    emit_preroll(&mut em)?;
    if keep_snaps {
        // H1: a side / stitched trace keeps its snapshots as they are, in the same order (the entry snapshot of `entry_checks`, the
        // link snapshot that no instruction names, the stubs' exit numbers): they are copied now that every value has its new ref
        for old in 0..rec.ir.snaps.len() as u32 {
            let sn = rec.ir.snaps[old as usize];
            let ents: Vec<SnapEntry> = rec
                .ir
                .snap_entries(old)
                .to_vec()
                .into_iter()
                .map(|e| SnapEntry { r: em.operand(e.r), ..e })
                .collect();
            let links = snap::frame_links(&rec.ir, old).to_vec();
            let n = em.new_snap(sn, &ents, &links, rec.snap_step[old as usize])?;
            if n != old {
                return up("snapshot numbering moved");
            }
        }
        em.entry_snap_out = rec.entry_snap;
    }
    let wb: Vec<SnapEntry> = em
        .loop_state
        .iter()
        .map(|(&slot, &(r, flags))| SnapEntry { slot, r, flags })
        .collect();
    let mut ir = std::mem::take(&mut em.b.ir);
    ir.counts = rec.ir.counts.clone();
    let mut o = Out {
        ir,
        snap_step: std::mem::take(&mut em.snap_step),
        entry_snap: em.entry_snap_out,
        writeback: wb,
        stats: em.stats,
        keep_all: keep_snaps,
    };
    cleanup(&mut o)?;
    Ok(o)
}

/// Layout 2: pre-roll, `LOOP`, PHIs, body.
fn peel_loop(rec: &Recorded, opts: &TraceOpts) -> Result<Out, Gave> {
    let mut em = Em::new(rec, opts);
    em.peeling = true;
    emit_preroll(&mut em)?;
    // TR3b (`+wb`): a slot the iteration writes and the next one never reads (no `SLOAD` of it) is stored at the end of the
    // pre-roll and of the body instead of being carried: no PHI, no entry in `S_H` or in the body's snapshots (the stack
    // holds the value, as in the interpreter)
    let mut stored: Vec<(u16, Ref)> = Vec::new();
    if opts.wb {
        let loaded: HashSet<u16> = rec
            .ir
            .ins
            .iter()
            .filter(|i| i.op == TOp::Sload)
            .map(|i| i.a)
            .collect();
        for e in &rec.writeback {
            if !loaded.contains(&e.slot) || mutants::active("wb_loaded_slot_stored") {
                let r = em.operand(e.r);
                em.b.emit_raw(Ins {
                    op: TOp::Sstore,
                    t: Ty::Nil,
                    flags: 0,
                    a: r,
                    b: NOREF,
                    prev: NOREF,
                    aux: u32::from(e.slot),
                })?;
                em.loop_state.remove(&e.slot);
                stored.push((e.slot, e.r));
            }
        }
        super::sink::WB_STORES.fetch_add(stored.len() as u64, std::sync::atomic::Ordering::Relaxed);
    }
    if em.bhoist {
        em.plan_bhoist()?;
    }
    let pre_end = em.b.ir.ins.len();
    em.pre_sub = em.sub.clone();
    let loop_ref = em.b.loop_marker()?;
    em.loop_ref = loop_ref;
    // the body: POLL and BUDGET at the loop top on S_H, then the copy
    em.body = true;
    em.snap_memo.clear();
    em.sub = vec![NOREF; rec.ir.ins.len()];
    let sh = em.s_h(false)?;
    for k in [0usize, 1] {
        let i = rec.ir.ins[k];
        em.b.emit_raw(Ins {
            aux: guard_aux(sh, aux_bits(i.aux)),
            prev: NOREF,
            ..i
        })?;
    }
    for n in 0..rec.ir.ins.len() {
        em.copy_one(n)?;
    }
    for &(slot, r) in &stored {
        let a = em.operand(r);
        em.b.emit_raw(Ins {
            op: TOp::Sstore,
            t: Ty::Nil,
            flags: 0,
            a,
            b: NOREF,
            prev: NOREF,
            aux: u32::from(slot),
        })?;
    }
    let loop_idx = usize::from(loop_ref - REF_BIAS);
    debug_assert_eq!(loop_idx, pre_end);

    // ---- PHIs: every pre-roll ref the body uses whose body copy differs from it ----
    //
    // `bval(e)` is the body copy of the *first creator* of the pre-roll ref `e`, not of the `src` ref the body use came
    // from. They can differ only when several `src` refs share one pre-roll ref (fold, CSE, load CSE, forwarding), and
    // then the body copy of each of them is that same body ref: every such merge in the pre-roll is derived
    // symbolically from the opaque live-in `SLOAD`s (and the memory log), so it holds for *any* values of the live-ins,
    // in particular for the body copy's, where the same derivation runs again on the substituted operands and merges
    // the same refs (or gives a value that is equal by the same argument). A merge that depended on a pre-roll fact
    // that is not true of every iteration (a constant observed at record time, the identity of two live-ins) would
    // break this; the recorder turns such observations into guards on the trace (`EQ step, k`), and a guard is an
    // instruction, not a fold input. See docs/JIT_TRACE_REF.md "P7b decisions".
    let bval = |em: &Em<'_>, e: Ref| -> Ref {
        match em.creator.get(&e) {
            Some(&n) => em.sub[n],
            None => e,
        }
    };
    let is_pre = |r: Ref| TraceIr::is_ins(r) && r < loop_ref;
    let mut used: Vec<Ref> = Vec::new();
    {
        let ir = &em.b.ir;
        for ins in &ir.ins[loop_idx + 1..] {
            let (ra, rb) = ins.op.ref_operands();
            if ra && is_pre(ins.a) {
                used.push(ins.a);
            }
            if rb && is_pre(ins.b) {
                used.push(ins.b);
            }
            // BUNDLE-1: the third operand (`LIB random`, a three-way `CONCAT`) is a use like the others
            if let Some(t) = ins.third() {
                if is_pre(t) && !mutants::active("hdrlib_third_stale") {
                    used.push(t);
                }
            }
        }
        for &s in &em.body_snaps {
            for e in ir.snap_entries(s) {
                if is_pre(e.r) {
                    used.push(e.r);
                }
            }
        }
    }
    let mut phi_ix: HashMap<Ref, usize> = HashMap::default();
    let mut phis: Vec<Ref> = Vec::new();
    let mut invariant_used: HashSet<Ref> = HashSet::default();
    let skip_nonslot = mutants::active("nonslot_phi_skip");
    let slot_vals: HashSet<Ref> = em.loop_state.values().map(|&(r, _)| r).collect();
    let mut work = used;
    while let Some(e) = work.pop() {
        if phi_ix.contains_key(&e) || invariant_used.contains(&e) {
            continue;
        }
        let be = bval(&em, e);
        if be == e {
            invariant_used.insert(e);
            continue;
        }
        if skip_nonslot
            && !slot_vals.contains(&e)
            && em
                .creator
                .get(&e)
                .is_some_and(|&n| em.src.ins[n].op != TOp::Sload)
        {
            // MUTANT: no PHI for a forwarded / CSE'd value that is not a slot
            invariant_used.insert(e);
            continue;
        }
        phi_ix.insert(e, phis.len());
        phis.push(e);
        if is_pre(be) {
            work.push(be);
        }
    }
    if phis.len() > MAX_PHI {
        return up("more than 64 phis");
    }
    // deterministic order: by pre-roll ref
    let mut order: Vec<Ref> = phis.clone();
    order.sort_unstable();
    let phi_ix: HashMap<Ref, usize> = order.iter().enumerate().map(|(i, &e)| (e, i)).collect();
    let nphi = order.len();

    // ---- relayout: pre-roll, LOOP, PHIs, body (refs above LOOP shift by nphi; body uses of PHI'd refs name the PHI) ----
    let rename = |r: Ref| -> Ref {
        if r < REF_BIAS {
            return r;
        }
        if r < loop_ref {
            match phi_ix.get(&r) {
                Some(&p) => loop_ref + 1 + p as Ref,
                None => r,
            }
        } else if r == loop_ref {
            r
        } else {
            r + nphi as Ref
        }
    };
    let src_ins = std::mem::take(&mut em.b.ir.ins);
    let mut ins: Vec<Ins> = Vec::with_capacity(src_ins.len() + nphi);
    ins.extend_from_slice(&src_ins[..=loop_idx]);
    for &e in &order {
        let be = bval(&em, e);
        let t = em.b.ir.ty_of_with(&src_ins, e);
        let tb = ty_with(&em.b.ir, &src_ins, be);
        if t != tb {
            return up(format!("phi type mismatch {t:?} / {tb:?}"));
        }
        // MUTANT `hdrlib_sub_phi_stale`: the PHI of a string made by an allocating `LIB` takes the pre-loop value (not the body's)
        let stale = mutants::active("hdrlib_sub_phi_stale") && src_ins[usize::from(e - REF_BIAS)].op == TOp::Lib;
        ins.push(Ins {
            op: TOp::Phi,
            t,
            flags: F_PHI,
            a: e,
            b: if stale { e } else { rename(be) },
            prev: NOREF,
            aux: 0,
        });
    }
    for x in &src_ins[loop_idx + 1..] {
        let mut y = *x;
        let (ra, rb) = y.op.ref_operands();
        if ra {
            y.a = rename(y.a);
        }
        if rb {
            y.b = rename(y.b);
        }
        if let Some(t) = y.third() {
            if !mutants::active("hdrlib_third_stale") {
                y.aux = y.aux_with_third(rename(t));
            }
        }
        ins.push(y);
    }
    if ins.len() > MAX_INS {
        return up("too long");
    }
    // flags for the dump: invariant pre-roll refs the body uses, and the refs that carry PHIs
    let mut hoisted = 0;
    for &e in &invariant_used {
        let ix = usize::from(e - REF_BIAS);
        if !matches!(
            ins[ix].op,
            TOp::Sload | TOp::Kint | TOp::Knum | TOp::Kpri | TOp::Kptr
        ) {
            ins[ix].flags |= F_LOOPINV;
            hoisted += 1;
        }
    }
    for &e in &order {
        ins[usize::from(e - REF_BIAS)].flags |= F_PHI;
    }
    for &s in &em.body_snaps {
        let (st, n) = em.b.ir.snaps[s as usize].map;
        for k in st as usize..st as usize + usize::from(n) {
            let r = em.b.ir.snapmap[k].r;
            em.b.ir.snapmap[k].r = rename(r);
        }
    }
    let mut ir = std::mem::take(&mut em.b.ir);
    ir.ins = ins;
    ir.loop_at = Some(loop_ref);
    ir.hoisted = !em.hoisted_ult.is_empty();
    ir.counts = vec![rec.n, rec.n];
    let wb: Vec<SnapEntry> = em
        .loop_state
        .iter()
        .map(|(&slot, &(r, flags))| SnapEntry { slot, r, flags })
        .collect();
    let mut stats = em.stats;
    stats.phis = nphi;
    stats.hoisted = hoisted;
    let mut o = Out {
        ir,
        snap_step: std::mem::take(&mut em.snap_step),
        entry_snap: em.entry_snap_out,
        writeback: wb,
        stats,
        keep_all: false,
    };
    cleanup(&mut o)?;
    check(&o.ir).map_err(|e| Gave::Up(format!("internal: {e}")))?;
    Ok(o)
}

fn ty_with(ir: &TraceIr, ins: &[Ins], r: Ref) -> Ty {
    if TraceIr::is_ins(r) {
        ins[usize::from(r - REF_BIAS)].t
    } else {
        ir.kval(r).ty()
    }
}

trait TyWith {
    fn ty_of_with(&self, ins: &[Ins], r: Ref) -> Ty;
}

impl TyWith for TraceIr {
    fn ty_of_with(&self, ins: &[Ins], r: Ref) -> Ty {
        ty_with(self, ins, r)
    }
}

// ---- cleanup: dead snapshots and dead pure instructions ---------------------------------------------------------------

fn is_root(i: &Ins) -> bool {
    i.flags & F_GUARD != 0
        || matches!(
            i.op,
            TOp::Astore
                | TOp::Ustore
                | TOp::Hstore
                | TOp::Tbar
                | TOp::Atouch
                | TOp::Newref
                | TOp::Tnew
                | TOp::Tlen
                | TOp::Tset
                | TOp::Tvirt
                | TOp::Sstore
                | TOp::Tnext
                | TOp::Snew
                | TOp::Strto
                | TOp::Concat
                | TOp::Lib
                | TOp::Loop
                | TOp::Poll
                | TOp::Budget
        )
}

/// Drops the snapshots no guard uses and the instructions nothing live uses (an unfused `AREF` is unsupported by the
/// assembler, and a dead load is a wasted load), and renumbers everything.
fn cleanup(o: &mut Out) -> Result<(), Gave> {
    let ir = &mut o.ir;
    // ---- snapshots ----
    let nsn = ir.snaps.len();

    let mut used = vec![false; nsn];
    if o.keep_all {
        used.iter_mut().for_each(|u| *u = true);
    }
    for i in &ir.ins {
        if i.flags & F_GUARD != 0 {
            let s = aux_snap(i.aux) as usize;
            if s >= nsn {
                return up("guard snapshot out of range");
            }
            used[s] = true;
        }
    }
    let mut new_of = vec![u32::MAX; nsn];
    let mut snaps: Vec<Snap> = Vec::new();
    let mut map: Vec<SnapEntry> = Vec::new();
    let mut frames: Vec<FrameLink> = Vec::new();
    let mut step: Vec<u32> = Vec::new();
    let mut foff = 0usize;
    for s in 0..nsn {
        let nf = usize::from(ir.snaps[s].nframes);
        if used[s] {
            new_of[s] = snaps.len() as u32;
            let (st, n) = ir.snaps[s].map;
            let mut sn = ir.snaps[s];
            sn.map = (map.len() as u32, n);
            map.extend_from_slice(&ir.snapmap[st as usize..st as usize + usize::from(n)]);
            frames.extend_from_slice(&ir.frames[foff..foff + nf]);
            snaps.push(sn);
            step.push(o.snap_step[s]);
        }
        foff += nf;
    }
    for i in &mut ir.ins {
        if i.flags & F_GUARD != 0 {
            let s = new_of[aux_snap(i.aux) as usize];
            i.aux = guard_aux(s, aux_bits(i.aux));
        }
    }
    o.entry_snap = o.entry_snap.and_then(|e| {
        let n = new_of[e as usize];
        (n != u32::MAX).then_some(n)
    });
    ir.snaps = snaps;
    ir.snapmap = map;
    ir.frames = frames;
    o.snap_step = step;

    // ---- dead instructions ----
    let n = ir.ins.len();
    let mut live = vec![false; n];
    let mut work: Vec<usize> = Vec::new();
    let mark = |r: Ref, live: &mut Vec<bool>, work: &mut Vec<usize>| {
        if TraceIr::is_ins(r) {
            let x = usize::from(r - REF_BIAS);
            if x < live.len() && !live[x] {
                live[x] = true;
                work.push(x);
            }
        }
    };
    for (x, i) in ir.ins.iter().enumerate() {
        if is_root(i) && !live[x] {
            live[x] = true;
            work.push(x);
        }
    }
    for e in &o.writeback {
        mark(e.r, &mut live, &mut work);
    }
    while let Some(x) = work.pop() {
        let i = ir.ins[x];
        let (ra, rb) = i.op.ref_operands();
        if matches!(i.op, TOp::Kint | TOp::Knum | TOp::Kpri | TOp::Kptr) {
            continue;
        }
        if ra {
            mark(i.a, &mut live, &mut work);
        }
        if rb {
            mark(i.b, &mut live, &mut work);
        }
        // the third operand (`TSET` value, `CONCAT` / `LIB` third) keeps its producer alive (and is renumbered below)
        if let Some(t) = i.third() {
            // BUNDLE-L A: the complemented index of an `AREF` is used by its `ALOAD` users only (an `ASTORE` addresses the element itself)
            if i.op != TOp::Aref && !mutants::active("cleanup_third_dead") {
                mark(t, &mut live, &mut work);
            }
        }
        if i.op == TOp::Aload && TraceIr::is_ins(i.a) {
            if let Some(t) = ir.at(i.a).third() {
                mark(t, &mut live, &mut work);
            }
        }
        if i.flags & F_GUARD != 0 {
            let (st, nn) = ir.snaps[aux_snap(i.aux) as usize].map;
            for k in st as usize..st as usize + usize::from(nn) {
                let r = ir.snapmap[k].r;
                mark(r, &mut live, &mut work);
            }
        }
    }
    // BUNDLE-L A: an `AREF` whose complemented index nobody reads any more (its `ALOAD` was forwarded away) drops it
    let mut dropped = false;
    for x in 0..n {
        if live[x] && ir.ins[x].op == TOp::Aref {
            if let Some(t) = ir.ins[x].third() {
                if TraceIr::is_ins(t) && !live[usize::from(t - REF_BIAS)] {
                    ir.ins[x].aux = 0;
                    dropped = true;
                }
            }
        }
    }
    let _ = dropped;
    if live.iter().all(|&l| l) {
        return Ok(());
    }
    let mut newidx = vec![0usize; n];
    let mut k = 0;
    for x in 0..n {
        newidx[x] = k;
        if live[x] {
            k += 1;
        }
    }
    let map_ref = |r: Ref| -> Ref {
        if r >= REF_BIAS {
            REF_BIAS + newidx[usize::from(r - REF_BIAS)] as Ref
        } else {
            r
        }
    };
    let old = std::mem::take(&mut ir.ins);
    let mut heads = [NOREF; TOp::COUNT];
    for (x, mut i) in old.into_iter().enumerate() {
        if !live[x] {
            continue;
        }
        let (ra, rb) = i.op.ref_operands();
        if ra {
            i.a = map_ref(i.a);
        }
        if rb {
            i.b = map_ref(i.b);
        }
        if let Some(t) = i.third() {
            if !mutants::active("cleanup_third_stale") {
                i.aux = i.aux_with_third(map_ref(t));
            }
        }
        let r = REF_BIAS + ir.ins.len() as Ref;
        i.prev = heads[i.op as usize];
        heads[i.op as usize] = r;
        ir.ins.push(i);
    }
    for e in &mut ir.snapmap {
        e.r = map_ref(e.r);
    }
    for e in &mut o.writeback {
        e.r = map_ref(e.r);
    }
    if let Some(l) = ir.loop_at.as_mut() {
        *l = map_ref(*l);
    }
    Ok(())
}

// ---- well-formedness ---------------------------------------------------------------------------------------------------

/// Checks the structural invariants of a (peeled or rolled) trace the later stages rely on: operands defined before
/// use (a `PHI`'s body operand may be a forward ref), `PHI`s directly after `LOOP`, snapshots in range with frame
/// links in snapshot order, snapshot refs defined before every guard that uses them, no snapshot shared between the
/// pre-roll and the body unless its map is empty, no `SLOAD` after `LOOP`, no unfused `AREF` user.
pub fn check(ir: &TraceIr) -> Result<(), String> {
    let n = ir.ins.len();
    let l = ir.loop_at.map(|r| usize::from(r - REF_BIAS));
    if let Some(l) = l {
        if l >= n || ir.ins[l].op != TOp::Loop {
            return Err("loop_at is not a LOOP".into());
        }
    }
    let nf: usize = ir.snaps.iter().map(|s| usize::from(s.nframes)).sum();
    if nf != ir.frames.len() {
        return Err(format!(
            "frame links {} != sum of nframes {nf}",
            ir.frames.len()
        ));
    }
    let mut seen_phis = 0usize;
    let mut first_user: HashMap<u32, usize> = HashMap::default();
    let mut straddle: HashMap<u32, (bool, bool)> = HashMap::default();
    for (x, i) in ir.ins.iter().enumerate() {
        let (ra, rb) = i.op.ref_operands();
        let chk = |r: Ref, fwd: bool| -> Result<(), String> {
            if r == NOREF {
                return Err(format!("NOREF operand at {x}"));
            }
            if r < REF_BIAS {
                if usize::from(r) > ir.k.len() {
                    return Err(format!("constant {r} out of range at {x}"));
                }
            } else {
                let o = usize::from(r - REF_BIAS);
                if o >= n || (o >= x && !fwd) {
                    return Err(format!("operand %{o} not defined before %{x}"));
                }
            }
            Ok(())
        };
        match i.op {
            TOp::Phi => {
                let Some(l) = l else {
                    return Err("PHI without LOOP".into());
                };
                if x != l + 1 + seen_phis {
                    return Err("PHIs must directly follow LOOP".into());
                }
                seen_phis += 1;
                chk(i.a, false)?;
                chk(i.b, true)?;
                if usize::from(i.a - REF_BIAS) >= l {
                    return Err("PHI pre-roll operand is not in the pre-roll".into());
                }
            }
            TOp::Loop | TOp::Poll => {}
            TOp::Sload | TOp::Budget => {
                if let Some(l) = l {
                    if x > l && i.op == TOp::Sload {
                        return Err("SLOAD in the body".into());
                    }
                }
            }
            TOp::Calln => {
                chk(i.a, false)?;
                if i.b != NOREF {
                    chk(i.b, false)?;
                }
            }
            TOp::Kint | TOp::Knum | TOp::Kpri | TOp::Kptr => {}
            _ => {
                if ra {
                    chk(i.a, false)?;
                }
                if rb && i.b != NOREF {
                    chk(i.b, false)?;
                }
            }
        }
        if let Some(t) = i.third() {
            chk(t, false)?;
        }
        if i.flags & F_GUARD != 0 {
            let s = aux_snap(i.aux);
            if s as usize >= ir.snaps.len() {
                return Err(format!("snapshot {s} out of range at {x}"));
            }
            first_user.entry(s).or_insert(x);
            let e = straddle.entry(s).or_default();
            if l.is_some_and(|l| x > l) {
                e.1 = true;
            } else {
                e.0 = true;
            }
            for en in ir.snap_entries(s) {
                if TraceIr::is_ins(en.r) {
                    let o = usize::from(en.r - REF_BIAS);
                    if o >= x {
                        return Err(format!(
                            "snapshot {s} ref %{o} not defined before guard %{x}"
                        ));
                    }
                    if o < n && matches!(
                            ir.ins[o].op,
                            TOp::Aref | TOp::Uref | TOp::Hrefk | TOp::Hrefv | TOp::Loop
                        ) {
                        return Err(format!("snapshot {s} holds an address ref"));
                    }
                }
            }
        }
        if matches!(
            i.op,
            TOp::Aload | TOp::Astore | TOp::Uload | TOp::Ustore | TOp::Hload | TOp::Hstore
        ) && !TraceIr::is_ins(i.a)
        {
            return Err(format!("memory op %{x} without an address"));
        }
    }
    for (s, (pre, body)) in straddle {
        if pre && body && ir.snaps[s as usize].map.1 != 0 {
            return Err(format!(
                "snapshot {s} with a map is used on both sides of LOOP"
            ));
        }
    }
    Ok(())
}

/// Counts of the loop shape, for tests and the dump: `(pre-roll length, phis, body length)`; `None` for a rolled trace.
pub fn shape(ir: &TraceIr) -> Option<(usize, usize, usize)> {
    let l = usize::from(ir.loop_at? - REF_BIAS);
    let phis = ir.ins[l + 1..]
        .iter()
        .take_while(|i| i.op == TOp::Phi)
        .count();
    Some((l, phis, ir.ins.len() - l - 1 - phis))
}
