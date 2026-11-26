//! Snapshots of the recorder (docs/JIT_TRACE.md 5.4, TR3, TR3a, TR5, TR22b): the slot map the recorder keeps while it
//! records one iteration, the construction of snapshot maps from it, and the count-model fix-up (`refund`, `land`) that
//! runs when the iteration is closed and its total instruction count `n` is known.
//!
//! The model (P3a: the root frame; P3b: inlined frames, slots are root-relative so a callee's registers are just
//! slots above its caller's):
//!
//! * [`SlotMap`] tracks, per root-relative slot, the *current* ref (the SSA value the interpreter's slot holds at this
//!   point of the recorded path), the unmodified `SLOAD` ref of the slot (what the stack holds from the start of the
//!   iteration), the type at its first read (the entry type, TR24c) and whether an instruction of the path wrote it.
//! * A snapshot is taken lazily at the first guard that is actually emitted for an instruction `P` (guards that fold
//!   away or hit the CSE chain do not create one) and describes the state *before* `P` (TR3a): every slot written on
//!   the path so far, temporaries included, dead or not (TR3: no pruning). A slot is omitted only when the stack
//!   already holds its value (never written, or its current ref is its own unmodified `SLOAD`).
//! * `refund` and `land` depend on the total count `n` of the segment, so they are patched at close ([`fixup`]):
//!   `refund = n_charged - fetches(pc) - land(pc)` with `n_charged = n` for every guard after the segment's budget
//!   charge and `0` for the two guards before it (the `POLL` and `BUDGET` at the loop top use the *uncharged*
//!   snapshot 0, which lands on `pcl[H]`).

use super::ir::*;
use super::mutants;
use crate::lower::{LabelKind, Lowered};
use rn_core::instruction::Instruction;
use rn_core::ver::active::OpCode;
use std::rc::Rc;

/// Per-slot state of the recorder (root frame).
#[derive(Debug, Default)]
pub struct SlotMap {
    /// The slot's current ref; [`NOREF`] = never touched on the path (the stack holds the value).
    pub cur: Vec<Ref>,
    /// The unmodified `SLOAD` of the slot ([`NOREF`] = not loaded).
    pub sload: Vec<Ref>,
    /// The type at the first read of the slot (the type its `SLOAD` checks).
    pub entry_ty: Vec<Option<Ty>>,
    /// Was the slot written by an instruction of the path? (Set through [`SlotMap::def`] / [`SlotMap::set_written`], which keep `wmask` in step.)
    pub written: Vec<bool>,
    /// T2 (docs/JIT_TRACE.md section 39.3): `written` as a bit set (bit `s % 64` of word `s / 64`), so that [`SlotMap::entries`] visits the written
    /// slots only (it ran over every slot of the frame for every snapshot: 2.7 % of `apps/raytrace`'s instructions).
    pub wmask: Vec<u64>,
    /// Path step of the last instruction that read the slot (`-1`: never); only the `snap_prune_dead` mutant uses it.
    pub last_read: Vec<i32>,
}

// T2 (docs/JIT_TRACE.md section 39.3): `clone_from` reuses the vectors (the derived one is `*self = source.clone()`: six allocations and six frees
// for the recorder's per-step copy of the map, `Rec::st_sm`).
impl Clone for SlotMap {
    fn clone(&self) -> SlotMap {
        SlotMap {
            cur: self.cur.clone(),
            sload: self.sload.clone(),
            entry_ty: self.entry_ty.clone(),
            written: self.written.clone(),
            wmask: self.wmask.clone(),
            last_read: self.last_read.clone(),
        }
    }

    fn clone_from(&mut self, src: &SlotMap) {
        self.cur.clone_from(&src.cur);
        self.sload.clone_from(&src.sload);
        self.entry_ty.clone_from(&src.entry_ty);
        self.written.clone_from(&src.written);
        self.wmask.clone_from(&src.wmask);
        self.last_read.clone_from(&src.last_read);
    }
}

impl SlotMap {
    /// A map for a frame of `n` slots.
    pub fn new(n: usize) -> SlotMap {
        SlotMap {
            cur: vec![NOREF; n],
            sload: vec![NOREF; n],
            entry_ty: vec![None; n],
            written: vec![false; n],
            wmask: vec![0; n.div_ceil(64)],
            last_read: vec![-1; n],
        }
    }

    /// Number of slots.
    pub fn len(&self) -> usize {
        self.cur.len()
    }

    /// Grows the map to `n` slots (a callee frame above the current extent); never shrinks.
    pub fn grow(&mut self, n: usize) {
        if n > self.cur.len() {
            self.cur.resize(n, NOREF);
            self.sload.resize(n, NOREF);
            self.entry_ty.resize(n, None);
            self.written.resize(n, false);
            self.wmask.resize(n.div_ceil(64), 0);
            self.last_read.resize(n, -1);
        }
    }

    /// Is the map empty (a frame without slots)?
    pub fn is_empty(&self) -> bool {
        self.cur.is_empty()
    }

    /// The slot was loaded from the stack by `r` (an `SLOAD` of type `ty`): its value is the stack's.
    pub fn loaded(&mut self, slot: usize, r: Ref, ty: Ty) {
        self.cur[slot] = r;
        self.sload[slot] = r;
        self.entry_ty[slot] = Some(ty);
    }

    /// An instruction of the path defined `slot` as `r`.
    pub fn def(&mut self, slot: usize, r: Ref) {
        self.cur[slot] = r;
        self.set_written(slot, true);
    }

    /// Sets the `written` flag of `slot` (and its bit in `wmask`).
    pub fn set_written(&mut self, slot: usize, w: bool) {
        self.written[slot] = w;
        let (i, b) = (slot / 64, 1u64 << (slot % 64));
        if w {
            self.wmask[i] |= b;
        } else {
            self.wmask[i] &= !b;
        }
    }

    /// The slots a snapshot must restore: written on the path and not holding their stack value already. In slot order.
    /// (T2: the set bits of `wmask` in ascending order are exactly the slots with `written`, so the result is the one of the scan over all slots.)
    pub fn entries(&self) -> Vec<SnapEntry> {
        // (sized once: the written slots bound the result; growing it one entry at a time was the larger cost of this function)
        let n: u32 = self.wmask.iter().map(|w| w.count_ones()).sum();
        let mut out = Vec::with_capacity(n as usize + 8);
        self.entries_into(&mut out);
        out
    }

    /// [`SlotMap::entries`] appended to `out` (section 43.2, T2b: [`take`] writes straight into the snapshot map of the trace instead of through a
    /// vector of its own that is allocated, filled, copied and freed again for every snapshot).
    pub fn entries_into(&self, out: &mut Vec<SnapEntry>) {
        for (i, &w) in self.wmask.iter().enumerate() {
            let mut w = w;
            while w != 0 {
                let s = i * 64 + w.trailing_zeros() as usize;
                w &= w - 1;
                if self.cur[s] != NOREF && self.cur[s] != self.sload[s] {
                    out.push(SnapEntry {
                        slot: s as u16,
                        r: self.cur[s],
                        flags: 0,
                    });
                }
            }
        }
    }

    /// The first slot (lowest number) whose type at the end of the iteration differs from the type its first read
    /// checked (`Unstable`, TR24c): `(slot, entry type, final type)`.
    pub fn unstable(&self, ir: &TraceIr) -> Option<(u16, Ty, Ty)> {
        (0..self.cur.len()).find_map(|s| {
            let t0 = self.entry_ty[s]?;
            let t1 = ir.ty_of(self.cur[s]);
            (t0 != t1).then_some((s as u16, t0, t1))
        })
    }
}

/// Recorder-side facts about one snapshot, needed for the count-model fix-up.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SnapMeta {
    /// Index into the recorded path of the instruction the snapshot belongs to (TR3a); `0` for snapshot 0.
    pub step: u32,
    /// The pc of that instruction.
    pub pc: u32,
    /// Snapshot 0: taken before the segment's budget charge (`n_charged = 0`, `fetches = 0`).
    pub uncharged: bool,
    /// Index of the `Lowered` of the frame `pc` belongs to (`0`: the root proto), see [`fixup`].
    pub low: u32,
}

/// TR5.4 `land`: the landing label kind of `pc` and the instructions its label has prepaid (`0` for a pre-count head
/// label, `rem(pc)` for a mid-block label). Mutant `land_pre_mid_swap` exchanges the two kinds.
pub fn land_info(low: &Lowered, pc: u32) -> (Land, u32) {
    let kind = low.label_kind(pc);
    let swap = mutants::active("land_pre_mid_swap");
    match (kind, swap) {
        (LabelKind::Head, false) | (LabelKind::Mid, true) => (Land::Pre, 0),
        (LabelKind::Mid, false) | (LabelKind::Head, true) => (Land::Mid, low.rem(pc)),
        (LabelKind::Dead, _) => panic!("no tier-1 label at pc {pc}"),
    }
}

/// P-X: mutant `land_type_check_skip` (the landing labels of typed mid-block pcs omit the tag checks, `lower`).
pub fn land_check_skipped() -> bool {
    mutants::active("land_type_check_skip")
}

/// S73: mutant `landfix_live_rewrite` (with `landfix`, a typed landing label rewrites the tag of every known register, live ones included).
pub fn land_live_rewrite() -> bool {
    mutants::active("landfix_live_rewrite")
}

/// The instructions tier 1 counts for the interpreter step at `pc` (TR5.1 `fetches`, docs/JIT_TRACE.md 9.5): the
/// instruction itself (`Lowered::fetches`: none for a `Dead` follower) plus the instructions the interpreter executes
/// *without fetching* them: `TFORPREP` runs the `TFORCALL` and that the `TFORLOOP` (+2), `TFORCALL` runs the
/// `TFORLOOP` (+1) (`do_tforcall!` / `do_tforloop!` in vm_unsafe.rs; tier 1 counts all of them, gap G2 of
/// `tr_identity_pre.rs`). The prototype records no generic `for`, so the chain is 0 there; P3b / the full build rely on it.
pub fn step_cost(low: &Lowered, code: &[u32], pc: u32) -> u32 {
    let own = low.fetches(&[pc]);
    let chained = match Instruction(code[pc as usize]).opcode() {
        Some(OpCode::Tforprep) => 2,
        Some(OpCode::Tforcall) => 1,
        _ => 0,
    };
    own + chained
}

/// Patches `refund` and `land` of every snapshot once the segment total `n` is known (TR5.4, TR5.5):
/// `refund = n_charged - fetches(pc) - land(pc)`; `cost[i]` is [`step_cost`] of step `i` of the recorded path
/// (across frames: callee instructions and the `RETURN` are ordinary steps) and `land` is read from the `Lowered` of
/// the frame the snapshot's pc belongs to (`lows[meta.low]`).
pub fn fixup(ir: &mut TraceIr, meta: &[SnapMeta], lows: &[Rc<Lowered>], cost: &[u32], n: u32) {
    debug_assert_eq!(ir.snaps.len(), meta.len());
    for (s, m) in ir.snaps.iter_mut().zip(meta) {
        let (land, amt) = land_info(&lows[m.low as usize], m.pc);
        let (charged, fetched) = if m.uncharged {
            (0, 0)
        } else {
            (n, cost[..m.step as usize].iter().sum::<u32>())
        };
        let mut refund = charged as i32 - fetched as i32 - amt as i32;
        if mutants::active("refund_off_by_one") && !m.uncharged {
            refund += 1;
        }
        s.refund = refund;
        s.land = land;
    }
}

/// Mutant `snap_prune_dead`: drops from every snapshot the slots that no later instruction of the path reads (a
/// "liveness" optimisation TR3 forbids: `debug.getlocal` reads temporaries). Rebuilds the snapshot maps.
pub fn prune_dead(ir: &mut TraceIr, meta: &[SnapMeta], last_read: &[i32]) {
    let old = std::mem::take(&mut ir.snapmap);
    for (s, m) in ir.snaps.iter_mut().zip(meta) {
        let (start, len) = s.map;
        let at = ir.snapmap.len() as u32;
        for e in &old[start as usize..start as usize + usize::from(len)] {
            if e.flags & SNAP_FUNC != 0 || last_read[usize::from(e.slot)] >= m.step as i32 {
                ir.snapmap.push(*e);
            }
        }
        s.map = (at, (ir.snapmap.len() as u32 - at) as u16);
    }
}

/// F4 (`snapprune`): the pruning data of the root prototype `p` (`None`: nothing may be pruned for it). The mutants
/// `prune_live_temp`, `prune_local` and `prune_loop_carried` / `prune_after_call` build a deliberately wrong analysis.
/// Is one of the mutants that build a wrong pruning analysis active (the result is then never cached)?
pub fn prune_faulty() -> bool {
    mutants::active("prune_loop_carried")
        || mutants::active("prune_after_call")
        || mutants::active("prune_live_temp")
        || mutants::active("prune_local")
        || mutants::active("opencall_defs_low")
}

pub fn prune_info(
    p: &rn_core::func::Proto,
    locals: bool,
    opencall: bool,
) -> Option<std::sync::Arc<PruneInfo>> {
    let f = crate::live::Faults {
        open_call_defs: opencall,
        open_defs_low: mutants::active("opencall_defs_low"),
        no_backedges: mutants::active("prune_loop_carried"),
        open_empty: mutants::active("prune_after_call"),
        ignore_live: mutants::active("prune_live_temp"),
        ignore_locals: locals
            || mutants::active("prune_local")
            || mutants::active("prune_loop_carried"),
    };
    crate::live::prunable_with(p, f).map(|dead| std::sync::Arc::new(PruneInfo { dead }))
}

/// May the entry `e` (a value of type `ty`) be left out of snapshot `s`? [`PruneInfo::drops`], with mutant
/// `prune_collectable` (a table or string temporary is dropped too).
pub fn may_drop(p: &PruneInfo, s: &Snap, e: &SnapEntry, ty: Ty) -> bool {
    if mutants::active("prune_collectable") && matches!(ty, Ty::Tab | Ty::ShrStr | Ty::LngStr) {
        return s.nframes == 0
            && s.frame == 0
            && s.top == 0
            && e.flags == 0
            && p.dead
                .get(s.pc as usize)
                .is_some_and(|d| crate::live::has(d, u32::from(e.slot)));
    }
    p.drops(s, e, ty)
}

/// F4: leaves the dead numeric temporaries out of the maps of the snapshots of `ir` (see [`TraceIr::prune`]). The slots
/// an exit does not restore keep whatever the stack holds, which the liveness proof says nothing reads.
pub fn prune_snaps(ir: &mut TraceIr) {
    let Some(p) = ir.prune.clone() else {
        return;
    };
    let mut new_map: Vec<SnapEntry> = Vec::with_capacity(ir.snapmap.len());
    let mut wins: Vec<(u32, u16)> = Vec::with_capacity(ir.snaps.len());
    for s in &ir.snaps {
        let (start, len) = s.map;
        let at = new_map.len() as u32;
        for e in &ir.snapmap[start as usize..start as usize + usize::from(len)] {
            if !may_drop(&p, s, e, ir.ty_of(e.r)) {
                new_map.push(*e);
            }
        }
        wins.push((at, (new_map.len() as u32 - at) as u16));
    }
    ir.snapmap = new_map;
    for (s, w) in ir.snaps.iter_mut().zip(wins) {
        s.map = w;
    }
}

/// An inlined frame as the recorder knows it when it takes a snapshot (5.5): what [`FrameLink`] stores plus the ref of
/// the closure in the function slot (the `FUNC` entry of the map).
#[derive(Clone, Copy, Debug)]
pub struct FrameRec {
    /// The callee's prototype.
    pub proto: ProtoKey,
    /// Root-relative slot of the callee's function slot.
    pub func_slot: u16,
    /// The caller's `CALL` pc (the caller resumes at `call_pc + 1`).
    pub call_pc: u32,
    /// Results the call wants (`C - 1`).
    pub nresults: i16,
    /// The closure value in the function slot.
    pub closure: Ref,
    /// F4: the frame was entered by a `TAILCALL`.
    pub tail: bool,
    /// P-E: see [`FrameLink::nextra`].
    pub nextra: i32,
    /// N3: see [`FrameLink::mm`].
    pub mm: bool,
}

/// Appends a snapshot of the current state (before the instruction at `pc`, in the frame at depth `frames.len()`)
/// and returns its index. `frames` are the inlined frames `1..=d`, outermost first: the snapshot gets `frame =
/// nframes = d`, a `SNAP_FUNC` entry for every function slot (always present, even when the stack still holds the
/// closure: the restore needs the frame's `func`), and its `FrameLink`s are **appended** to `ir.frames` (see
/// [`frame_links`]). Mutant `snap_drop_last` omits the last map entry of the slot map (before the `FUNC` entries
/// are merged). `top` is [`Snap::top`] (`0`: none).
pub fn take(
    ir: &mut TraceIr,
    sm: &SlotMap,
    pc: u32,
    land: Land,
    frames: &[FrameRec],
    top: u16,
) -> u32 {
    // (section 43.2, T2b: the map is built in place at the end of `ir.snapmap`, which is what `TraceIr::snapshot` appends it to)
    assert!(ir.snaps.len() < MAX_SNAPS, "TooLong");
    let start = ir.snapmap.len();
    sm.entries_into(&mut ir.snapmap);
    if mutants::active("snap_drop_last") && ir.snapmap.len() > start {
        ir.snapmap.pop();
    }
    for f in frames {
        let e = SnapEntry {
            slot: f.func_slot,
            r: f.closure,
            flags: SNAP_FUNC,
        };
        match ir.snapmap[start..].binary_search_by_key(&f.func_slot, |x| x.slot) {
            Ok(i) => ir.snapmap[start + i] = e,
            Err(i) => ir.snapmap.insert(start + i, e),
        }
    }
    ir.frames.extend(frames.iter().map(|f| FrameLink {
        proto: f.proto,
        func_slot: f.func_slot,
        call_pc: f.call_pc,
        nresults: f.nresults,
        tail: f.tail,
        nextra: f.nextra,
        mm: f.mm,
    }));
    ir.snapshot_built(
        Snap {
            pc,
            frame: frames.len() as u8,
            map: (0, 0),
            nframes: frames.len() as u8,
            refund: 0,
            land,
            count: 0,
            top,
            seg: 0,
        },
        start,
    )
}

/// The inlined frames of snapshot `s`, outermost first (link `k` is the frame at depth `k + 1`). Convention: the
/// snapshots store their links back to back in snapshot order, so the links of `s` start after those of all earlier
/// snapshots (`Snap` has no offset field).
pub fn frame_links(ir: &TraceIr, s: u32) -> &[FrameLink] {
    let off: usize = ir.snaps[..s as usize]
        .iter()
        .map(|x| usize::from(x.nframes))
        .sum();
    &ir.frames[off..off + usize::from(ir.snaps[s as usize].nframes)]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lower::{lower, LowerCfg};

    /// `for k, v in next, t do end` followed by a return: `TFORPREP` costs itself + `TFORCALL` + `TFORLOOP`, a stepped
    /// `TFORCALL` itself + `TFORLOOP`, everything else 1 (and 0 for a `Dead` pc).
    #[test]
    fn generic_for_chains_are_counted() {
        let code: Vec<u32> = [
            Instruction::abx(OpCode::Tforprep, 0, 1).0, // 0: -> pc 2
            Instruction::abck(OpCode::Loadnil, 5, 0, 0, 0).0, // 1: body
            Instruction::abck(OpCode::Tforcall, 0, 0, 1, 0).0, // 2
            Instruction::abx(OpCode::Tforloop, 0, 3).0, // 3: -> pc 1
            Instruction::abck(OpCode::Return0, 0, 1, 0, 0).0, // 4
        ]
        .to_vec();
        let low = lower(
            &code,
            LowerCfg {
                code_addr: 0,
                interrupt_addr: 0,
                calls: None,
                consts: &[],
                t15_warmup: None,
            },
        )
        .expect("lowers");
        assert_eq!(step_cost(&low, &code, 0), 3);
        assert_eq!(step_cost(&low, &code, 1), 1);
        assert_eq!(step_cost(&low, &code, 2), 2);
    }

    #[test]
    fn entries_are_sparse_and_ordered() {
        let mut sm = SlotMap::new(4);
        let mut ir = TraceIr::default();
        let k = ir.kint(1);
        sm.loaded(1, REF_BIAS, Ty::Int);
        sm.def(1, REF_BIAS); // redefined to its own SLOAD: the stack already holds it
        sm.def(3, k);
        sm.def(0, k);
        let e = sm.entries();
        assert_eq!(e.iter().map(|e| e.slot).collect::<Vec<_>>(), vec![0, 3]);
        sm.def(1, k);
        assert_eq!(sm.entries().len(), 3);
    }

    #[test]
    fn unstable_reports_the_type_change() {
        let mut sm = SlotMap::new(2);
        let mut ir = TraceIr::default();
        let kf = ir.knum(1.0);
        sm.loaded(0, REF_BIAS, Ty::Int);
        ir.ins.push(Ins {
            op: TOp::Sload,
            t: Ty::Int,
            flags: 0,
            a: 0,
            b: 0,
            prev: NOREF,
            aux: 0,
        });
        assert_eq!(sm.unstable(&ir), None);
        sm.def(0, kf);
        assert_eq!(sm.unstable(&ir), Some((0, Ty::Int, Ty::Num)));
    }

    /// T2 (docs/JIT_TRACE.md section 39.3): `entries` over the `wmask` bit set equals the scan over every slot it replaced, through definitions,
    /// redefinitions to the slot's own `SLOAD`, cleared flags, growth across word boundaries and `clone_from` into maps of other sizes.
    #[test]
    fn entries_by_mask_equal_the_full_scan() {
        let scan = |sm: &SlotMap| -> Vec<SnapEntry> {
            (0..sm.cur.len())
                .filter(|&s| sm.written[s] && sm.cur[s] != NOREF && sm.cur[s] != sm.sload[s])
                .map(|s| SnapEntry {
                    slot: s as u16,
                    r: sm.cur[s],
                    flags: 0,
                })
                .collect()
        };
        let mut sm = SlotMap::new(3);
        let mut other = SlotMap::new(200);
        let mut x: u64 = 0x9e37_79b9_7f4a_7c15;
        for step in 0..4000u32 {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            let n = sm.len();
            let slot = (x % (n as u64 + 3)) as usize;
            match (x >> 20) % 7 {
                0 => sm.grow(n + 1 + (x >> 30) as usize % 70),
                1 if slot < n => sm.loaded(slot, REF_BIAS + slot as Ref, Ty::Int),
                2 if slot < n => sm.def(slot, REF_BIAS + slot as Ref), // its own SLOAD's ref
                3 if slot < n => sm.set_written(slot, false),
                _ if slot < n => sm.def(slot, 1 + (x >> 40) as Ref % 50),
                _ => {}
            }
            assert_eq!(sm.entries(), scan(&sm), "step {step}");
            if step % 97 == 0 {
                other.clone_from(&sm);
                assert_eq!(other.entries(), scan(&sm));
                assert_eq!(other.wmask, sm.wmask);
                assert_eq!(other.cur, sm.cur);
            }
        }
        assert!(sm.len() > 128, "the map grew across several mask words");
    }

    /// Section 43.2 (T2b): [`take`] builds the map in place at the end of `ir.snapmap`; the result is the one of the earlier composition
    /// (`entries()`, the function-slot entries merged in slot order, [`TraceIr::snapshot`]), with frames, a replaced and an inserted function slot,
    /// and a map that starts after earlier snapshots.
    #[test]
    fn take_builds_the_map_in_place() {
        let mut sm = SlotMap::new(12);
        let mut ir = TraceIr::default();
        for s in [1usize, 4, 5, 9] {
            let k = ir.knum(s as f64);
            sm.def(s, k);
        }
        let frames = [
            FrameRec {
                proto: ProtoKey(7),
                func_slot: 4, // replaces the written slot 4
                call_pc: 3,
                nresults: 1,
                closure: ir.knum(100.0),
                tail: false,
                nextra: 0,
                mm: false,
            },
            FrameRec {
                proto: ProtoKey(8),
                func_slot: 7, // inserted between 5 and 9
                call_pc: 6,
                nresults: -1,
                closure: ir.knum(101.0),
                tail: true,
                nextra: 2,
                mm: false,
            },
        ];
        for (n, fr) in [(0usize, &frames[..0]), (1, &frames[..]), (2, &frames[..1])] {
            let before = ir.snapmap.len();
            let mut want = sm.entries();
            for f in fr {
                let e = SnapEntry {
                    slot: f.func_slot,
                    r: f.closure,
                    flags: SNAP_FUNC,
                };
                match want.binary_search_by_key(&f.func_slot, |x| x.slot) {
                    Ok(i) => want[i] = e,
                    Err(i) => want.insert(i, e),
                }
            }
            let s = take(&mut ir, &sm, 11 + n as u32, Land::Pre, fr, 3);
            assert_eq!(ir.snap_entries(s), &want[..], "case {n}");
            assert_eq!(ir.snaps[s as usize].map.0 as usize, before);
            assert_eq!(ir.snapmap.len(), before + want.len());
            assert_eq!(usize::from(ir.snaps[s as usize].nframes), fr.len());
        }
    }
}
