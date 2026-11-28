//! Allocation sinking (docs/JIT_TRACE.md 6.8, TR26, TR29; task S0, the prototype). Switch `-sink`.
//!
//! # What the pass does
//!
//! It runs on the recorded rolled iteration (before `entry_checks` and the loop optimiser) and removes the `NEWTABLE`s
//! whose table never leaves the trace: a `TNEW` (array size 0) that is only
//!
//! * guarded non-null (`NE %t, 0`), asked for its metatable / node-vector size (guards that are trivially true for a
//!   table created on the trace) and its node vector,
//! * given string keys with `NEWREF` + `HSTORE` (+ `TBAR`), and read and overwritten through `HREFK` + `HLOAD` /
//!   `HSTORE` (+ `TBAR`),
//! * and named by snapshot entries and, at the loop end, by the writeback entry of a **dead** slot.
//!
//! Everything else that mentions the table (a store into another table, a call, an identity compare, an `ALOAD`, a
//! field that is stored `nil`, a writeback entry of a slot the next iteration or the program can still see) unsinks it:
//! the instructions stay as recorded (class-A helper allocation, TR1).
//!
//! The sunk table's instructions disappear; `TNEW` itself becomes `TVIRT` (the virtual allocation: the bytes the
//! interpreter's `new_table` would have charged are added to the collector's total, TR29.4); loads from it are
//! forwarded from its stores. Every snapshot that held the table gets a **sunk entry group** ([`SNAP_SUNK`]): the
//! exit restore (`exit::materialise`) allocates the table with the `NEWTABLE`'s sizes, stores the fields in insertion
//! order (same sizes and order give the same node layout, TR26c) and moves its bytes from virtual to real.
//!
//! # Writeback (TR3b, S0 decision)
//!
//! A sunk table still held at the loop end by a slot the next iteration does not read and that is not an active local at
//! the loop head (`Proto::locvars` at the head pc; every slot of an inlined callee) is **dead** (decision D7): its
//! writeback entry is dropped and no PHI carries it. A table held by a live local (or in a prototype without debug
//! information) is not sunk. Materialising at the writeback would allocate once per iteration (no gain); sunk PHIs
//! would need their own review.

use super::ir::*;
use super::mutants;
use super::record::Recorded;
use super::TraceOpts;
use crate::fx::{HashMap, HashSet};
use std::cell::RefCell;
use std::sync::atomic::{AtomicU64, Ordering};

/// Tables sunk, traces with a sunk table, peeled loops and their PHIs (process-wide, for `RUSTYNAIL_JIT_VERBOSE` and the
/// X11 measurement).
pub static SUNK_TABLES: AtomicU64 = AtomicU64::new(0);
/// Traces (compiles) in which at least one table was sunk.
pub static SUNK_TRACES: AtomicU64 = AtomicU64::new(0);
/// Peeled loops produced by the loop optimiser.
pub static PEELED: AtomicU64 = AtomicU64::new(0);
/// PHIs of those loops.
pub static PHIS: AtomicU64 = AtomicU64::new(0);
/// Sunk tables materialised by exits (process-wide).
pub static MATERIALISED: AtomicU64 = AtomicU64::new(0);
/// J1-2: calls of the light materialising helper by inline exit stubs (a subset of the exits that materialise).
pub static MAT_STUB_CALLS: AtomicU64 = AtomicU64::new(0);
/// J1-2: of those, the calls of frameful stubs.
pub static MAT_STUB_FRAMES: AtomicU64 = AtomicU64::new(0);
/// J1-2: tables created by those calls.
pub static MAT_STUB_TABLES: AtomicU64 = AtomicU64::new(0);
/// `SSTORE`s emitted by the `+wb` writeback.
pub static WB_STORES: AtomicU64 = AtomicU64::new(0);

/// N2c: installed traces that contain a `RETF` (printed only when non-zero: the statistics of `retf=0` runs stay identical).
pub static RETF_TRACES: AtomicU64 = AtomicU64::new(0);

/// The counters as a `[trace] sink: ...` line.
pub fn report() -> String {
    let mut s = report0();
    let r = RETF_TRACES.load(Ordering::Relaxed);
    if r != 0 {
        s.push_str(&format!(" retf {r}"));
    }
    s
}

fn report0() -> String {
    format!(
        "[trace] sink: tables {} traces {}; materialised {}; loops peeled {} phis {} wb-stores {} stub-calls {} stub-frames {} stub-tables {}",
        SUNK_TABLES.load(Ordering::Relaxed),
        SUNK_TRACES.load(Ordering::Relaxed),
        MATERIALISED.load(Ordering::Relaxed),
        PEELED.load(Ordering::Relaxed),
        PHIS.load(Ordering::Relaxed),
        WB_STORES.load(Ordering::Relaxed),
        MAT_STUB_CALLS.load(Ordering::Relaxed),
        MAT_STUB_FRAMES.load(Ordering::Relaxed),
        MAT_STUB_TABLES.load(Ordering::Relaxed)
    )
}

thread_local! {
    static NOTE: RefCell<String> = const { RefCell::new(String::new()) };
    /// H3-G: shapes whose exits materialise so often that sinking them loses; the recorder keeps their allocations.
    static DEMOTED: RefCell<HashSet<u64>> = RefCell::new(HashSet::default());
}

/// H3-G: the identity of a sunk table for the per-site policy: the `NEWTABLE` sizes and the address of its first field key (the
/// constructor idiom `{x = .., y = ..}` names its fields in a fixed order; a snapshot taken before the first store has no key).
pub fn shape_of(b: u32, c: u32, first_key: u64) -> u64 {
    let mut h = (u64::from(b) << 32 | u64::from(c)) ^ first_key.wrapping_mul(0x9E37_79B9_7F4A_7C15);
    h ^= h >> 29;
    h.wrapping_mul(0xBF58_476D_1CE4_E5B9)
}

/// SINK-R: a shape key scoped to one root head (prototype address, pc): the ineffective-root rule demotes a shape for the root that failed with it,
/// not for every loop that builds a table of that shape (`sinkroot=2`).
pub fn scoped_shape(shape: u64, proto: u64, pc: u32) -> u64 {
    let mut h = shape
        ^ proto.wrapping_mul(0x9E37_79B9_7F4A_7C15)
        ^ u64::from(pc).wrapping_mul(0xD6E8_FEB8_6659_FD93);
    h ^= h >> 31;
    h.wrapping_mul(0xBF58_476D_1CE4_E5B9) | 1
}

/// SINK-R: demotes `shape` at once (no count, no pending flush: the caller flushes). `false` when it was demoted already.
pub fn demote_shape(shape: u64) -> bool {
    DEMOTED.with(|d| d.borrow_mut().insert(shape))
}

/// H3-G: is `shape` demoted (the recorder must not sink it)?
pub fn is_demoted(shape: u64) -> bool {
    !mutants::active("sink_demote_ignored")
        && DEMOTED.with(|d| !d.borrow().is_empty() && d.borrow().contains(&shape))
}

/// H3-G (tests): forget every demotion of this thread.
pub fn reset_policy() {
    DEMOTED.with(|d| d.borrow_mut().clear());
}

/// J1-2: adds to a statistics counter of the exit path without a locked read-modify-write (a `lock`ed instruction waits for the store
/// buffer to drain: ~100 cycles per exit after the stub's slot stores, 5% of `cd`); two states on two threads may lose an update, which a
/// statistic tolerates.
#[inline(always)]
pub fn bump(c: &AtomicU64, n: u64) {
    c.store(c.load(Ordering::Relaxed).wrapping_add(n), Ordering::Relaxed);
}

/// The one-line summary of the last [`run`] of this thread.
pub fn last_note() -> String {
    NOTE.with(|n| n.borrow().clone())
}

fn collectable(t: Ty) -> bool {
    matches!(
        t,
        Ty::ShrStr | Ty::LngStr | Ty::Tab | Ty::Lcf | Ty::Ccl | Ty::Udata | Ty::Thread
    )
}

/// One event on a sunk table, in program order.
#[derive(Clone, Copy, Debug)]
struct Ev {
    /// Index of the `HSTORE`.
    at: usize,
    key: Ref,
    val: Ref,
    insert: bool,
}

/// A table that can be sunk.
#[derive(Clone, Debug, Default)]
struct Cand {
    t: usize,
    b: u32,
    c: u32,
    vbytes: u32,
    remove: Vec<usize>,
    events: Vec<Ev>,
    /// `HLOAD` index -> the old ref of the value it reads.
    alias: Vec<(usize, Ref)>,
    /// Slots holding the table in a writeback entry (dropped).
    dead_wb: Vec<u16>,
    /// BUNDLE-2 (`sinkmt`): the `setmetatable` of the table: index of its `SETMT` `LIB` and the metatable ref (the guard `NE %lib, 0` behind it,
    /// at the next index, is replaced by the `__gc`-absence flag guard of the metatable: the same snapshot, the same position).
    setmt: Option<(usize, Ref)>,
    /// The removed `NE %lib, 0` guard (its snapshot and flags are the new guard's).
    setmt_ne: Option<Ins>,
}

fn ref_of(i: usize) -> Ref {
    REF_BIAS + i as Ref
}

fn idx(r: Ref) -> usize {
    usize::from(r - REF_BIAS)
}

/// The operands of `i` that are refs (`a`, `b`, the third of `Tset`).
fn operands(i: &Ins) -> [Ref; 3] {
    let (ra, rb) = i.op.ref_operands();
    [
        if ra { i.a } else { NOREF },
        if rb { i.b } else { NOREF },
        i.third().unwrap_or(NOREF),
    ]
}

/// Can slot `slot` (root-relative) of the loop at the head `(proto, pc)` be overwritten without anyone noticing? True for
/// a slot that no instruction of the iteration reads from the stack (`loaded`), and that is either a slot of an inlined
/// callee or a root slot above the active locals of the head.
fn slot_is_dead(head: (ProtoKey, u32), slot: u16, loaded: &HashSet<u16>) -> bool {
    if loaded.contains(&slot) {
        return false;
    }
    slot_is_dead_at(head, slot)
}

/// Which slots may a trace leave stale at its end (BUNDLE-5 `sinkside`)?
#[derive(Clone, Copy, Debug)]
enum EndKind {
    /// A loop trace: the next iteration; the head's active locals and the `loaded` slots decide.
    Loop,
    /// A side trace that ends with a link in the root frame: the target's head `(proto, pc)` decides (its active locals).
    Link((ProtoKey, u32)),
    /// The trace ends with its always-taken exit (stitched, `no_link`): the writeback stores behind the exit never run.
    Exit,
    /// Nothing is dead (a linking exit that materialises inlined frames).
    Nothing,
}

fn slot_is_dead_at(head: (ProtoKey, u32), slot: u16) -> bool {
    // SAFETY: the head's prototype is the running closure's (the recording is in progress / just closed on this thread).
    let p = unsafe { &*(head.0 .0 as *const rn_core::func::Proto) };
    if usize::from(slot) >= usize::from(p.maxstacksize) {
        return true; // a slot of an inlined callee: popped at the loop end
    }
    if p.lineinfo().is_empty() {
        return false; // stripped: the active locals are unknown
    }
    let pc = head.1 as i32;
    let mut nact = 0usize;
    for lv in p.locvars() {
        if lv.startpc as i32 > pc {
            break;
        }
        if pc < lv.endpc as i32 {
            nact += 1;
        }
    }
    usize::from(slot) >= nact
}

const M_REMOVED: u8 = 1;
const M_META: u8 = 2;
const M_LS: u8 = 4;
const M_NODE: u8 = 8;
const M_NEWREF: u8 = 16;
const M_MTF: u8 = 32;

/// The instruction sets of [`analyse`]. Section 85.1 (`cspeed` lever `SINKD`): one byte of `M_*` flags per instruction and a table of the `HREFK`
/// keys (the analysis looked every operand of every later instruction up in five hash sets, per candidate: 30-40 k instructions per `TNEW` on
/// `awfy/cd` and `apps/raytrace`); without the lever the hash sets of the old path.
struct Marks {
    dense: bool,
    flags: Vec<u8>,
    keys: Vec<Ref>,
    sets: [HashSet<usize>; 6],
    hrefks: HashMap<usize, Ref>,
}

impl Marks {
    fn new(n: usize, dense: bool) -> Marks {
        Marks {
            dense,
            flags: if dense { vec![0; n + 4] } else { Vec::new() },
            keys: if dense {
                vec![NOREF; n + 4]
            } else {
                Vec::new()
            },
            sets: Default::default(),
            hrefks: HashMap::default(),
        }
    }

    fn has(&self, m: u8, j: usize) -> bool {
        if self.dense {
            // (MUTANT `sinkd_ls_miss`: the `lsizenode` loads are not found: their shape guards end the candidate)
            let m = if m == M_LS && mutants::active("sinkd_ls_miss") {
                0
            } else {
                m
            };
            self.flags.get(j).is_some_and(|&f| f & m != 0)
        } else {
            self.sets[m.trailing_zeros() as usize].contains(&j)
        }
    }

    fn ins(&mut self, m: u8, j: usize) {
        if self.dense {
            if j >= self.flags.len() {
                self.flags.resize(j + 1, 0);
            }
            self.flags[j] |= m;
        } else {
            self.sets[m.trailing_zeros() as usize].insert(j);
        }
    }

    /// Is `j` one of the derived values of the table (a field load, an `HREFK`, a `NEWREF`)?
    fn fld(&self, j: usize) -> bool {
        if self.dense {
            self.flags
                .get(j)
                .is_some_and(|&f| f & (M_META | M_LS | M_NODE | M_NEWREF) != 0)
                || self.keys.get(j).is_some_and(|&k| k != NOREF)
        } else {
            self.has(M_META, j)
                || self.has(M_LS, j)
                || self.has(M_NODE, j)
                || self.hrefks.contains_key(&j)
                || self.has(M_NEWREF, j)
        }
    }

    fn hrefk(&self, j: usize) -> Option<Ref> {
        if self.dense {
            self.keys.get(j).copied().filter(|&k| k != NOREF)
        } else {
            self.hrefks.get(&j).copied()
        }
    }

    fn set_hrefk(&mut self, j: usize, k: Ref) {
        if self.dense {
            if j >= self.keys.len() {
                self.keys.resize(j + 1, NOREF);
            }
            self.keys[j] = k;
        } else {
            self.hrefks.insert(j, k);
        }
    }
}

fn analyse(
    rec: &Recorded,
    t: usize,
    loaded: &HashSet<u16>,
    sinkmt: bool,
    end: EndKind,
) -> Option<Cand> {
    let ir = &rec.ir;
    let n = ir.ins.len();
    let ti = ir.ins[t];
    let (b, c) = (u32::from(ti.a), ti.aux);
    if c != 0 || b > 12 {
        return None;
    }
    let hsize = if b > 0 { 1u32 << (b - 1) } else { 0 };
    let vbytes = rn_core::table::new_table_bytes(0, hsize)?;
    let tr = ref_of(t);
    let mut cand = Cand {
        t,
        b,
        c,
        vbytes: u32::try_from(vbytes).ok()?,
        ..Cand::default()
    };
    // (the removed instructions, the `FLOAD`s of the metatable / `lsizenode` / node fields, the `HREFK`s with their keys, the `NEWREF`s, and
    // BUNDLE-2's `FLOAD metatable` after the `SETMT` (aliased to the metatable ref); section 85.1: dense with the lever `SINKD`)
    let mut mk = Marks::new(n, crate::cspeed::on(crate::cspeed::SINKD));
    let mut setmt: Option<(usize, Ref)> = None;
    let mut store: Vec<(Ref, Ref)> = Vec::new(); // key -> value, insertion order
    let is_const_int = |r: Ref, v: i64| ir.as_int(r) == Some(v);
    let mut j = t + 1;
    while j < n {
        let i = ir.ins[j];
        if mk.has(M_REMOVED, j) {
            j += 1;
            continue;
        }
        let ops = operands(&i);
        let uses_t = ops.contains(&tr);
        let uses_fld = ops.iter().any(|&r| TraceIr::is_ins(r) && mk.fld(idx(r)));
        // BUNDLE-2: `metatable != 0` of a sunk table that was given one: true (any other user of that load is renamed to the metatable ref)
        if sinkmt
            && i.op == TOp::Ne
            && TraceIr::is_ins(i.a)
            && mk.has(M_MTF, idx(i.a))
            && is_const_int(i.b, 0)
        {
            cand.remove.push(j);
            j += 1;
            continue;
        }
        if !uses_t && !uses_fld {
            j += 1;
            continue;
        }
        match i.op {
            TOp::Ne if i.a == tr && is_const_int(i.b, 0) => {
                cand.remove.push(j);
            }
            TOp::Fload if i.a == tr => {
                let f = i.b;
                if f == Field::Metatable as Ref {
                    if let Some((_, mt)) = setmt {
                        // BUNDLE-2: after the `SETMT` the metatable of the sunk table is the one it was given
                        mk.ins(M_MTF, j);
                        cand.alias.push((j, mt));
                    } else {
                        mk.ins(M_META, j);
                    }
                } else if f == Field::Lsizenode as Ref {
                    mk.ins(M_LS, j);
                } else if f == Field::Node as Ref {
                    mk.ins(M_NODE, j);
                } else {
                    return None;
                }
                cand.remove.push(j);
            }
            TOp::Eq if TraceIr::is_ins(i.a) && mk.has(M_META, idx(i.a)) && is_const_int(i.b, 0) => {
                cand.remove.push(j);
            }
            // BUNDLE-2: `setmetatable(t, mt)` of the sunk table with a table `mt` (an instruction's value): the helper and its guard `NE %lib, 0` are
            // replaced (see `rebuild`) by the guard that `mt` has no `__gc` (the helper's only other failure, a protected old metatable, cannot
            // happen: the table is new)
            TOp::Lib
                if sinkmt
                    && i.a == tr
                    && (i.aux & 0xFF) == lib::SETMT
                    && setmt.is_none()
                    && !mutants::active("sinkmt_never") =>
            {
                let mt = i.b;
                if !TraceIr::is_ins(mt)
                    || ir.ty_of(mt) != Ty::Tab
                    || ir.ins[idx(mt)].op == TOp::Tnew
                    || idx(mt) >= j
                {
                    return None;
                }
                let ne = *ir.ins.get(j + 1)?;
                if !(ne.op == TOp::Ne
                    && ne.a == ref_of(j)
                    && is_const_int(ne.b, 0)
                    && ne.flags & F_GUARD != 0)
                {
                    return None;
                }
                setmt = Some((j, mt));
                cand.setmt = Some((j, mt));
                cand.setmt_ne = Some(ne);
                mk.ins(M_REMOVED, j);
                mk.ins(M_REMOVED, j + 1);
                cand.remove.push(j);
                cand.remove.push(j + 1);
            }
            // BUNDLE-2: the absent-key proof of a method lookup on a sunk table: true when the key was never stored
            TOp::Hmiss
                if sinkmt
                    && TraceIr::is_ins(i.a)
                    && mk.has(M_NODE, idx(i.a))
                    && TraceIr::is_const(i.b) =>
            {
                if store.iter().any(|&(k, _)| k == i.b) {
                    return None;
                }
                cand.remove.push(j);
            }
            TOp::Eq if TraceIr::is_ins(i.a) && mk.has(M_LS, idx(i.a)) && TraceIr::is_const(i.b) => {
                cand.remove.push(j);
            }
            TOp::Newref if i.a == tr => {
                if !(TraceIr::is_const(i.b) && matches!(ir.kval(i.b), KVal::Ptr(_, PtrKind::Raw))) {
                    return None;
                }
                if store.iter().any(|&(k, _)| k == i.b) {
                    return None;
                }
                if j + 2 >= n {
                    return None;
                }
                let (ne, hs) = (ir.ins[j + 1], ir.ins[j + 2]);
                if !(ne.op == TOp::Ne && ne.a == ref_of(j) && is_const_int(ne.b, 0))
                    || !(hs.op == TOp::Hstore && hs.a == ref_of(j) && hs.flags & F_GUARD == 0)
                {
                    return None;
                }
                let val = hs.b;
                let vt = ir.ty_of(val);
                if vt == Ty::Nil || matches!(vt, Ty::I64 | Ty::P64 | Ty::U32) {
                    return None;
                }
                let mut upto = j + 2;
                if collectable(vt) {
                    let tb = *ir.ins.get(j + 3)?;
                    if !(tb.op == TOp::Tbar && tb.a == tr && tb.b == val) {
                        return None;
                    }
                    upto = j + 3;
                }
                for k in j..=upto {
                    mk.ins(M_REMOVED, k);
                    cand.remove.push(k);
                }
                mk.ins(M_NEWREF, j);
                store.push((i.b, val));
                cand.events.push(Ev {
                    at: j + 2,
                    key: i.b,
                    val,
                    insert: true,
                });
            }
            TOp::Hrefk if TraceIr::is_ins(i.a) && mk.has(M_NODE, idx(i.a)) => {
                if !TraceIr::is_const(i.b) || !store.iter().any(|&(k, _)| k == i.b) {
                    return None;
                }
                mk.set_hrefk(j, i.b);
                cand.remove.push(j);
            }
            TOp::Hload if TraceIr::is_ins(i.a) && mk.hrefk(idx(i.a)).is_some() => {
                let key = mk.hrefk(idx(i.a))?;
                let (_, v) = *store.iter().find(|&&(k, _)| k == key)?;
                if ir.ty_of(v) != i.t {
                    return None;
                }
                cand.alias.push((j, v));
                cand.remove.push(j);
            }
            TOp::Hstore if TraceIr::is_ins(i.a) && mk.hrefk(idx(i.a)).is_some() && i.b != tr => {
                let key = mk.hrefk(idx(i.a))?;
                let val = i.b;
                let vt = ir.ty_of(val);
                if vt == Ty::Nil || matches!(vt, Ty::I64 | Ty::P64 | Ty::U32) {
                    return None;
                }
                let slot = store.iter_mut().find(|(k, _)| *k == key)?;
                slot.1 = val;
                cand.events.push(Ev {
                    at: j,
                    key,
                    val,
                    insert: false,
                });
                cand.remove.push(j);
                mk.ins(M_REMOVED, j);
                if collectable(vt) {
                    let tb = *ir.ins.get(j + 1)?;
                    if !(tb.op == TOp::Tbar && tb.a == tr && tb.b == val) {
                        return None;
                    }
                    mk.ins(M_REMOVED, j + 1);
                    cand.remove.push(j + 1);
                }
            }
            _ => return None,
        }
        j += 1;
    }
    // a later user of a removed HLOAD is rewritten by the alias; users of other removed values were rejected above
    // snapshots: the table may be named by entries; their contents are decided by position (see `rebuild`)
    // writeback: only dead slots
    for e in &rec.writeback {
        if e.r == tr {
            let dead = match end {
                EndKind::Loop => slot_is_dead(ir.head, e.slot, loaded),
                EndKind::Link(h) => {
                    mutants::active("sinkside_dead_all") || slot_is_dead_at(h, e.slot)
                }
                EndKind::Exit => true,
                EndKind::Nothing => false,
            };
            if mutants::active("sink_escape_ignored") || dead {
                cand.dead_wb.push(e.slot);
            } else {
                return None;
            }
        }
    }
    // H3-G: a shape whose exits materialise it too often is kept as a real allocation
    let first_key = store.first().and_then(|&(k, _)| match ir.kval(k) {
        KVal::Ptr(a, _) => Some(a),
        _ => None,
    });
    let shape = shape_of(b, c, first_key.unwrap_or(0));
    if is_demoted(shape) || is_demoted(scoped_shape(shape, rec.ir.head.0 .0 as u64, rec.ir.head.1))
    {
        return None;
    }
    Some(cand)
}

/// The slots some instruction of the iteration reads from the stack.
fn loaded_slots(ir: &TraceIr) -> HashSet<u16> {
    ir.ins
        .iter()
        .filter(|i| i.op == TOp::Sload)
        .map(|i| i.a)
        .collect()
}

/// Section 52 (`resink`): side traces that kept every table they replayed sunk.
pub static RESUNK: AtomicU64 = AtomicU64::new(0);
/// Section 52: side traces whose replay was reverted (a replayed table did not stay sunk).
pub static REVERTED: AtomicU64 = AtomicU64::new(0);

/// The pass-list entry (`sink`): see the module documentation.
///
/// Section 52 (`resink`): a side trace that replays its parent's sunk tables (`TraceIr::replay`) keeps the replay only when the pass sinks every
/// replayed table again (the trial runs on a copy); otherwise the whole replay is reverted first (each replayed `TNEW` becomes the unchecked `SLOAD` of
/// the slot that holds the table the parent's exit materialises, the other replay instructions go) and the pass runs on that.
pub fn run(rec: &mut Recorded, opts: &TraceOpts) -> Result<(), String> {
    if rec.ir.replay.is_empty() {
        return run0(rec, opts).map(|_| ());
    }
    let mut trial = rec.clone();
    let all = match run0(&mut trial, opts) {
        Ok(sunk) => rec.ir.replay.iter().all(|&(t, _)| sunk.contains(&idx(t))),
        Err(_) => false,
    };
    if all {
        *rec = trial;
        rec.ir.replay.clear();
        rec.ir.replay_range = (0, 0);
        rec.ir.resink = true;
        bump(&RESUNK, 1);
        return Ok(());
    }
    bump(&REVERTED, 1);
    revert_replay(rec)?;
    run0(rec, opts).map(|_| ())
}

/// Section 52: undoes the replay of a side trace (see [`run`]).
fn revert_replay(rec: &mut Recorded) -> Result<(), String> {
    let n = rec.ir.ins.len();
    let (lo, hi) = (
        rec.ir.replay_range.0 as usize,
        (rec.ir.replay_range.1 as usize).min(n),
    );
    let slot_of: HashMap<usize, u16> = rec.ir.replay.iter().map(|&(t, s)| (idx(t), s)).collect();
    let mut removed = vec![false; n];
    for (i, r) in removed.iter_mut().enumerate().take(hi).skip(lo) {
        *r = !slot_of.contains_key(&i);
    }
    let mut newidx: Vec<Option<usize>> = vec![None; n];
    let mut k = 0usize;
    for i in 0..n {
        if !removed[i] {
            newidx[i] = Some(k);
            k += 1;
        }
    }
    let res = |r: Ref| -> Result<Ref, String> {
        if r < REF_BIAS {
            return Ok(r);
        }
        newidx
            .get(idx(r))
            .copied()
            .flatten()
            .map(ref_of)
            .ok_or_else(|| "resink: a reference to a reverted replay instruction".to_string())
    };
    let mut heads = [NOREF; TOp::COUNT];
    let mut out: Vec<Ins> = Vec::with_capacity(k);
    for i in 0..n {
        if removed[i] {
            continue;
        }
        let mut ins = rec.ir.ins[i];
        if let Some(&slot) = slot_of.get(&i) {
            // (MUTANT `resink_revert_slot_off`: the reverted table is loaded from the next slot)
            ins = Ins {
                op: TOp::Sload,
                t: Ty::Tab,
                flags: 0,
                a: slot + u16::from(mutants::active("resink_revert_slot_off")),
                b: 0,
                prev: NOREF,
                aux: 0,
            };
        } else {
            let (ra, rb) = ins.op.ref_operands();
            if ra {
                ins.a = res(ins.a)?;
            }
            if rb {
                ins.b = res(ins.b)?;
            }
            if let Some(v) = ins.third() {
                ins.aux = ins.aux_with_third(res(v)?);
            }
        }
        ins.prev = heads[ins.op as usize];
        heads[ins.op as usize] = ref_of(out.len());
        out.push(ins);
    }
    for e in rec.ir.snapmap.iter_mut() {
        e.r = res(e.r)?;
    }
    for e in rec.writeback.iter_mut() {
        e.r = res(e.r)?;
    }
    rec.ir.ins = out;
    rec.ir.replay.clear();
    rec.ir.replay_range = (0, 0);
    Ok(())
}

/// [`run`] without the replay decision: returns the indices of the `TNEW`s it sank.
fn run0(rec: &mut Recorded, opts: &TraceOpts) -> Result<Vec<usize>, String> {
    if !opts.sink
        || rec.ir.loop_at.is_some()
        || (!rec.ir.segs.is_empty() && (opts.sinkseg == 0 || rec.side.is_none()))
    {
        // (N2b: slots of different segments are different stack cells: the dead-slot analysis of a loop is per root frame. Section 86.4, `sinkseg`: a
        // side trace's end is decided in the last segment's coordinates, the writeback's and the link's, and each snapshot names slots of its own
        // segment, so the analysis holds there)
        return Ok(Vec::new());
    }
    let end = match (rec.side, rec.stitched) {
        // a native self-call / return continues in the frame that the writeback describes: nothing in it is dead
        _ if rec.ir.rec_call.is_some() || rec.ir.rec_ret.is_some() => EndKind::Nothing,
        (None, false) => EndKind::Loop,
        _ if opts.sinkside == 0 => return Ok(Vec::new()),
        (Some(sd), _) if sd.no_link => EndKind::Exit,
        (Some(sd), _) if sd.frame_link => EndKind::Nothing,
        (Some(sd), _) => EndKind::Link((sd.link_proto, sd.link_pc)),
        (None, true) => EndKind::Exit,
    };
    // `sinkside=2`: only the side traces that link back to a root frame (their stale slots are decided by the link target's active locals). The stitched /
    // no-link traces and the frame-link traces gain nothing measurable and are the losers of `sinkside=1` (`awfy/json`, `stream_closures`: their
    // materialisation stubs run more often than the allocations they remove), docs/JIT_PARKED.md A8.
    if opts.sinkside == 2 && !matches!(end, EndKind::Loop | EndKind::Link(_)) {
        return Ok(Vec::new());
    }
    // Section 53 (`cspeed` lever `SINKSKIP`): no `TNEW`, no candidate
    if crate::cspeed::on(crate::cspeed::SINKSKIP) && !rec.ir.ins.iter().any(|i| i.op == TOp::Tnew) {
        NOTE.with(|n| *n.borrow_mut() = "sink: no candidates".into());
        return Ok(Vec::new());
    }
    let loaded = loaded_slots(&rec.ir);
    let mut cands: Vec<Cand> = Vec::new();
    for t in 0..rec.ir.ins.len() {
        if rec.ir.ins[t].op == TOp::Tnew {
            if let Some(c) = analyse(rec, t, &loaded, opts.sinkmt != 0, end) {
                cands.push(c);
            }
        }
    }
    if cands.is_empty() {
        NOTE.with(|n| *n.borrow_mut() = "sink: no candidates".into());
        return Ok(Vec::new());
    }
    let sunk = rebuild(rec, &cands)?;
    SUNK_TABLES.fetch_add(sunk.len() as u64, Ordering::Relaxed);
    SUNK_TRACES.fetch_add(1, Ordering::Relaxed);
    NOTE.with(|n| *n.borrow_mut() = format!("sink: {} tables sunk", sunk.len()));
    Ok(sunk)
}

fn rebuild(rec: &mut Recorded, cands: &[Cand]) -> Result<Vec<usize>, String> {
    let n = rec.ir.ins.len();
    let mut removed = vec![false; n];
    let mut alias: HashMap<usize, Ref> = HashMap::default();
    let mut sunk_of: HashMap<Ref, usize> = HashMap::default(); // table ref -> candidate
    for (ci, c) in cands.iter().enumerate() {
        for &r in &c.remove {
            removed[r] = true;
        }
        for &(h, v) in &c.alias {
            alias.insert(h, v);
        }
        sunk_of.insert(ref_of(c.t), ci);
    }
    // Section 80.2 (`cspeed` lever `M1`): the alias and candidate maps as tables indexed by instruction (empty with the lever off)
    let m1 = crate::cspeed::on(crate::cspeed::M1);
    let mut alias_d: Vec<Ref> = Vec::new();
    let mut sunk_d: Vec<u32> = Vec::new();
    if m1 {
        alias_d = vec![NOREF; n];
        sunk_d = vec![u32::MAX; n];
        for (&h, &v) in &alias {
            if h < n {
                alias_d[h] = v;
            }
        }
        for (ci, c) in cands.iter().enumerate() {
            if c.t < n {
                sunk_d[c.t] = ci as u32;
            }
        }
    }
    let sunk_get = |r: Ref| -> Option<usize> {
        if m1 {
            if r < REF_BIAS || idx(r) >= n {
                return None;
            }
            let c = sunk_d[idx(r)];
            (c != u32::MAX).then_some(c as usize)
        } else {
            sunk_of.get(&r).copied()
        }
    };
    // first guard index and last guard index of every snapshot
    let nsn = rec.ir.snaps.len();
    let mut first_g = vec![usize::MAX; nsn];
    let mut last_g = vec![0usize; nsn];
    for (i, ins) in rec.ir.ins.iter().enumerate() {
        if ins.flags & F_GUARD != 0 && !removed[i] {
            let s = aux_snap(ins.aux) as usize;
            first_g[s] = first_g[s].min(i);
            last_g[s] = last_g[s].max(i);
        }
    }
    // BUNDLE-2: the replacement guard of a `SETMT` sits at the position of the removed `LIB` with the snapshot of the removed `NE`
    let mut mt_consts: Option<(Ref, Ref)> = None;
    // (section 52: a replayed `setmetatable` gets no `__gc` guard: the parent's guard passed in the run that left through the exit the side trace
    // starts at, and no Lua code runs between that exit and the side trace's start)
    // (only the replay's own `setmetatable`: one the side trace records itself on a replayed table keeps its guard. MUTANT `resink_setmt_guard_any`:
    // every `setmetatable` of a replayed table loses it)
    let (rlo, rhi) = (
        rec.ir.replay_range.0 as usize,
        rec.ir.replay_range.1 as usize,
    );
    let replay_t: Vec<usize> = rec.ir.replay.iter().map(|&(t, _)| idx(t)).collect();
    let replayed = |c: &Cand| {
        c.setmt.is_some_and(|(at, _)| {
            (rlo <= at && at < rhi)
                || (mutants::active("resink_setmt_guard_any") && replay_t.contains(&c.t))
        })
    };
    for c in cands.iter().filter(|c| !replayed(c)) {
        if let (Some((at, _)), Some(ne)) = (c.setmt, c.setmt_ne) {
            let s = aux_snap(ne.aux) as usize;
            first_g[s] = first_g[s].min(at);
            last_g[s] = last_g[s].max(at);
            if mt_consts.is_none() {
                let bit = rec
                    .ir
                    .konst(KVal::Int(1i64 << (rn_core::meta::Tm::Gc as u8)));
                let z = rec.ir.konst(KVal::Int(0));
                mt_consts = Some((bit, z));
            }
        }
    }
    // a store between the first and the last guard of one snapshot would make "the contents before the snapshot"
    // ambiguous (TR3a says it cannot happen): unsink every table that has one
    let mut bad: HashSet<usize> = HashSet::default();
    if crate::cspeed::on(crate::cspeed::PASSES) {
        // Section 49 (`cspeed` lever `PASSES`): the snapshots that name each candidate, from one scan of the snapshot maps (the check below
        // scanned every map once per store of every candidate); a snapshot without a guard never decides (`first_g` is `MAX`)
        let mut named: Vec<Vec<usize>> = vec![Vec::new(); cands.len()];
        for s in 0..nsn {
            if first_g[s] == usize::MAX {
                continue;
            }
            let (st, len) = rec.ir.snaps[s].map;
            for e in &rec.ir.snapmap[st as usize..st as usize + usize::from(len)] {
                if let Some(ci) = sunk_get(e.r) {
                    if named[ci].last() != Some(&s) {
                        named[ci].push(s);
                    }
                }
            }
        }
        for (ci, c) in cands.iter().enumerate() {
            if c.events.iter().any(|ev| {
                named[ci]
                    .iter()
                    .any(|&s| first_g[s] < ev.at && ev.at < last_g[s])
            }) {
                bad.insert(ci);
            }
        }
    }
    for (ci, c) in cands
        .iter()
        .enumerate()
        .filter(|_| !crate::cspeed::on(crate::cspeed::PASSES))
    {
        for ev in &c.events {
            for s in 0..nsn {
                let (st, len) = rec.ir.snaps[s].map;
                let names = rec.ir.snapmap[st as usize..st as usize + usize::from(len)]
                    .iter()
                    .any(|e| e.r == ref_of(c.t));
                if names && first_g[s] != usize::MAX && first_g[s] < ev.at && ev.at < last_g[s] {
                    bad.insert(ci);
                }
            }
        }
    }
    if !bad.is_empty() {
        let keep: Vec<Cand> = cands
            .iter()
            .enumerate()
            .filter(|(i, _)| !bad.contains(i))
            .map(|(_, c)| c.clone())
            .collect();
        if keep.is_empty() {
            return Ok(Vec::new());
        }
        return rebuild(rec, &keep);
    }
    // ---- new instruction list ----
    // BUNDLE-2: the replacement guards of the `SETMT`s (3 instructions each) are emitted before the first kept instruction behind the removed `NE`
    let mut guards: Vec<(usize, usize)> = cands
        .iter()
        .enumerate()
        .filter(|&(_, c)| !replayed(c))
        .filter_map(|(ci, c)| c.setmt.map(|(at, _)| (at + 1, ci)))
        .collect();
    guards.sort_unstable();
    let mut newidx: Vec<Option<usize>> = vec![None; n];
    let mut order: Vec<usize> = Vec::new();
    {
        let (mut extra, mut gi) = (0usize, 0usize);
        for i in 0..n {
            if !removed[i] {
                while gi < guards.len() && guards[gi].0 < i {
                    extra += 3;
                    gi += 1;
                }
                newidx[i] = Some(order.len() + extra);
                order.push(i);
            }
        }
    }
    fn resolve(
        r: Ref,
        alias: &HashMap<usize, Ref>,
        alias_d: &[Ref],
        newidx: &[Option<usize>],
    ) -> Result<Ref, String> {
        if r < REF_BIAS {
            return Ok(r);
        }
        let mut r = r;
        for _ in 0..64 {
            let i = idx(r);
            let hit = if alias_d.is_empty() {
                alias.get(&i).copied()
            } else {
                alias_d.get(i).copied().filter(|&a| a != NOREF)
            };
            if let Some(a) = hit {
                r = a;
                if r < REF_BIAS {
                    return Ok(r);
                }
                continue;
            }
            return match newidx[i] {
                Some(k) => Ok(ref_of(k)),
                None => Err(format!("sink: reference to the removed instruction {i}")),
            };
        }
        Err("sink: alias chain".into())
    }
    let mut heads = [NOREF; TOp::COUNT];
    let mut out: Vec<Ins> = Vec::with_capacity(order.len() + 3 * cands.len());
    // BUNDLE-2: the replacement guards, emitted before the first kept instruction behind the removed `NE`
    let mut next_guard = 0usize;
    for &o in order.iter().chain(std::iter::once(&usize::MAX)) {
        while next_guard < guards.len() && guards[next_guard].0 < o {
            let c = &cands[guards[next_guard].1];
            next_guard += 1;
            let (Some((_, mt)), Some(ne), Some((bit, z))) = (c.setmt, c.setmt_ne, mt_consts) else {
                return Err("sink: setmt without constants".into());
            };
            let mtr = resolve(mt, &alias, &alias_d, &newidx)?;
            let flags_ref = ref_of(out.len());
            let band_ref = ref_of(out.len() + 1);
            let seq = [
                Ins {
                    op: TOp::Fload,
                    t: Ty::U32,
                    flags: 0,
                    a: mtr,
                    b: Field::Flags as Ref,
                    prev: NOREF,
                    aux: 0,
                },
                // (MUTANT `sinkmt_gc_guard_skip`: the guard tests a bit that is always set)
                Ins {
                    op: TOp::Band,
                    t: Ty::I64,
                    flags: 0,
                    a: flags_ref,
                    b: if mutants::active("sinkmt_gc_guard_skip") {
                        z
                    } else {
                        bit
                    },
                    prev: NOREF,
                    aux: 0,
                },
                Ins {
                    op: TOp::Ne,
                    t: Ty::I64,
                    flags: ne.flags,
                    a: band_ref,
                    b: z,
                    prev: NOREF,
                    aux: ne.aux,
                },
            ];
            for mut g in seq {
                if mutants::active("sinkmt_gc_guard_skip") && g.op == TOp::Ne {
                    g.op = TOp::Eq; // (a guard that always holds: `0 == 0`)
                }
                g.prev = heads[g.op as usize];
                heads[g.op as usize] = ref_of(out.len());
                out.push(g);
            }
        }
        if o == usize::MAX {
            break;
        }
        let mut i = rec.ir.ins[o];
        let cand = if crate::cspeed::on(crate::cspeed::PASSES) {
            // (the candidate allocated at `o`: `sunk_of` holds every candidate's table ref)
            sunk_get(ref_of(o))
                .filter(|_| !mutants::active("cspeed_sink_cand_miss"))
                .map(|ci| &cands[ci])
        } else {
            cands.iter().find(|c| c.t == o)
        };
        if let Some(c) = cand {
            i = Ins {
                op: TOp::Tvirt,
                t: Ty::Nil,
                flags: 0,
                a: NOREF,
                b: NOREF,
                prev: NOREF,
                // (section 52: a replayed table was charged by the parent's `TVIRT` already)
                aux: if mutants::active("sink_vbytes_zero")
                    || rec.ir.replay.iter().any(|&(t, _)| idx(t) == c.t)
                {
                    0
                } else {
                    c.vbytes
                },
            };
        } else {
            let (ra, rb) = i.op.ref_operands();
            if ra {
                i.a = resolve(i.a, &alias, &alias_d, &newidx)?;
            }
            if rb {
                i.b = resolve(i.b, &alias, &alias_d, &newidx)?;
            }
            if let Some(v) = i.third() {
                i.aux = i.aux_with_third(resolve(v, &alias, &alias_d, &newidx)?);
            }
        }
        i.prev = heads[i.op as usize];
        heads[i.op as usize] = ref_of(out.len());
        out.push(i);
    }
    // ---- snapshots ----
    let old_map = std::mem::take(&mut rec.ir.snapmap);
    let mut new_map: Vec<SnapEntry> = Vec::with_capacity(old_map.len());
    // Section 85.1 (`cspeed` lever `SINKD`): per candidate, its header constant and the last field group it wrote (the number of its stores
    // before the snapshot's first guard, whether the metatable entry was written, and where the group's entries are in `new_map`): a
    // snapshot with the same store prefix copies the group (the group is a function of that prefix: `events` are in program order); the
    // candidate -> group slot map of a snapshot is a short list instead of a hash map
    let sinkd = crate::cspeed::on(crate::cspeed::SINKD);
    let mut hk_m: Vec<Ref> = vec![NOREF; if sinkd { cands.len() } else { 0 }];
    let mut grp_m: Vec<Option<(usize, bool, usize, usize)>> = vec![None; hk_m.len()];
    let sorted: Vec<bool> = cands
        .iter()
        .map(|c| sinkd && c.events.windows(2).all(|w| w[0].at <= w[1].at))
        .collect();
    let mut first_v: Vec<(usize, u16)> = Vec::new();
    #[allow(clippy::needless_range_loop)]
    // `s` also indexes the snapshot table, `last_g` and the output
    for s in 0..nsn {
        let (st, len) = rec.ir.snaps[s].map;
        let at = new_map.len();
        let mut first_slot: HashMap<usize, u16> = HashMap::default(); // candidate -> slot of its group
        first_v.clear();
        for e in &old_map[st as usize..st as usize + usize::from(len)] {
            if let Some(ci) = sunk_get(e.r) {
                let seen = if sinkd {
                    first_v.iter().find(|x| x.0 == ci).map(|x| x.1)
                } else {
                    first_slot.get(&ci).copied()
                };
                if let Some(fs) = seen {
                    let k = rec.ir.konst(KVal::Int(i64::from(fs)));
                    new_map.push(SnapEntry {
                        slot: e.slot,
                        r: k,
                        flags: SNAP_SALIAS,
                    });
                    continue;
                }
                if sinkd {
                    first_v.push((ci, e.slot));
                } else {
                    first_slot.insert(ci, e.slot);
                }
                let c = &cands[ci];
                let g = first_g[s];
                if g != usize::MAX && g <= c.t {
                    return Err(format!(
                        "sink: snapshot {s} names a table before its allocation"
                    ));
                }
                let hk = match hk_m.get(ci) {
                    Some(&k) if k != NOREF => k,
                    _ => {
                        let k = rec
                            .ir
                            .konst(KVal::Int(i64::from(c.b) | i64::from(c.c) << 16));
                        if let Some(m) = hk_m.get_mut(ci) {
                            *m = k;
                        }
                        k
                    }
                };
                new_map.push(SnapEntry {
                    slot: e.slot,
                    r: hk,
                    flags: SNAP_SUNK,
                });
                let with_mt = c.setmt.is_some_and(|(at, _)| g != usize::MAX && g > at);
                let pfx = if sorted[ci] {
                    // (MUTANT `sinkd_group_any`: the prefix length is not part of the key)
                    if mutants::active("sinkd_group_any") {
                        0
                    } else {
                        c.events.partition_point(|ev| ev.at < g)
                    }
                } else {
                    usize::MAX
                };
                if let Some((_, _, gs, gl)) = grp_m
                    .get(ci)
                    .copied()
                    .flatten()
                    .filter(|&(p, m, _, _)| pfx != usize::MAX && p == pfx && m == with_mt)
                {
                    new_map.extend_from_within(gs..gs + gl);
                    continue;
                }
                let gstart = new_map.len();
                // the contents: inserted keys in insertion order with their latest value before the snapshot
                let mut keys: Vec<(Ref, Ref)> = Vec::new();
                for ev in c.events.iter().filter(|ev| ev.at < g) {
                    if ev.insert {
                        keys.push((ev.key, ev.val));
                    } else if let Some(k) = keys.iter_mut().find(|(k, _)| *k == ev.key) {
                        if !mutants::active("sink_store_order") {
                            k.1 = ev.val;
                        }
                    }
                }
                for (k, v) in keys {
                    let v = resolve(v, &alias, &alias_d, &newidx)?;
                    new_map.push(SnapEntry {
                        slot: SNAP_PSEUDO,
                        r: k,
                        flags: SNAP_SFKEY,
                    });
                    new_map.push(SnapEntry {
                        slot: SNAP_PSEUDO,
                        r: v,
                        flags: SNAP_SFVAL,
                    });
                }
                // BUNDLE-2: the metatable, when the snapshot is taken after the `SETMT` (a guard of this snapshot behind it)
                if let Some((at, mt)) = c.setmt {
                    if g != usize::MAX && g > at {
                        new_map.push(SnapEntry {
                            slot: SNAP_PSEUDO,
                            r: resolve(mt, &alias, &alias_d, &newidx)?,
                            flags: SNAP_SMT,
                        });
                    }
                }
                if pfx != usize::MAX {
                    if let Some(m) = grp_m.get_mut(ci) {
                        *m = Some((pfx, with_mt, gstart, new_map.len() - gstart));
                    }
                }
            } else {
                new_map.push(SnapEntry {
                    r: resolve(e.r, &alias, &alias_d, &newidx)?,
                    ..*e
                });
            }
        }
        rec.ir.snaps[s].map = (at as u32, (new_map.len() - at) as u16);
    }
    rec.ir.snapmap = new_map;
    rec.ir.ins = out;
    // ---- writeback ----
    let wb = std::mem::take(&mut rec.writeback);
    for e in wb {
        if sunk_get(e.r).is_some() {
            continue; // a dead slot (checked by `analyse`)
        }
        rec.writeback.push(SnapEntry {
            r: resolve(e.r, &alias, &alias_d, &newidx)?,
            ..e
        });
    }
    Ok(cands.iter().map(|c| c.t).collect())
}
