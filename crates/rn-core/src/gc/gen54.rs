//! Lua 5.4 collector control (`lgc.c` 5.4.9, built only under `cfg(rustynail_lua54)`): the 5.4
//! generational mode (`genstep` with `genmajormul`, major collections as `fullgen`, the "bad
//! collection" fallback `stepgenfull` that stays incremental until a collection traverses less
//! than 9/8 of the previous one, `youngcollection`, `atomic2gen`, `entergen`/`enterinc`,
//! `setminordebt`), the 5.4 incremental step (`incstep`: debt and step size in work units,
//! `GCFINMAX` finalizers per step at `GCFINALIZECOST`), `setpause` on `GCestimate`, `fullinc`,
//! `luaC_fullgc` and `luaC_changemode`.
//!
//! The object-level machinery (marking, `atomic`, sweeps, `sweep2old`/`sweepgen`/
//! `correctgraylists`, the age invariants) is shared with the 5.5 port in `mod.rs` and
//! `generational.rs`; only the control layer differs between the versions.
//!
//! The control functions are generic over a [`Host54`], which runs the collector with the root
//! set and calls finalizers (`GCTM`). The interpreter (`State`, `collect.rs`) is the real host:
//! finalizers run exactly where the reference calls them (inside `finishgencycle`, in the
//! `GCScallfin` state of a step, before `setminordebt`/`setpause`). [`HeapHost`] serves the
//! heap-only API (`Heap::check_gc`, `Heap::collect`, ...) used by heap tests, with the heap's
//! finalizer hook.
//!
//! Debt: the reference keeps `GCdebt` (a step is due when it is `> 0`); the heap keeps a byte
//! threshold (due when `total >= threshold`), so `debt = total - threshold + 1`.
use super::*;

/// `GCFINMAX`: finalizers called per incremental step.
const GCFINMAX: usize = 10;
/// `GCFINALIZECOST`: work units charged per finalizer.
const GCFINALIZECOST: isize = 50;
/// `WORK2MEM` = `sizeof(TValue)`.
const WORK2MEM: isize = 16;
/// `PAUSEADJ`.
const PAUSEADJ: usize = 100;
/// GC stress 2 in generational mode: one step in this many is a major collection.
const STRESS_MAJOR: u32 = 64;

/// What the 5.4 control layer needs from its caller.
pub trait Host54 {
    fn gc_heap(&mut self) -> &mut Heap;
    /// Run `f` with the heap and the root-marking callback.
    fn gc_with_roots<R>(
        &mut self,
        f: impl FnOnce(&mut Heap, &mut dyn FnMut(&mut Marker)) -> R,
    ) -> R;
    /// `GCTM`: call the finalizer of the next object of `tobefnz` (which is not empty).
    fn gc_call_finalizer(&mut self);
}

/// Host for the heap-only API: finalizers run through the heap's finalizer hook (without a hook,
/// or when the caller asked to defer them, they stay queued).
pub(crate) struct HeapHost<'a, 'b> {
    pub(crate) heap: &'a mut Heap,
    pub(crate) roots: &'a mut (dyn FnMut(&mut Marker) + 'b),
}

impl Host54 for HeapHost<'_, '_> {
    fn gc_heap(&mut self) -> &mut Heap {
        self.heap
    }
    fn gc_with_roots<R>(
        &mut self,
        f: impl FnOnce(&mut Heap, &mut dyn FnMut(&mut Marker)) -> R,
    ) -> R {
        f(self.heap, self.roots)
    }
    fn gc_call_finalizer(&mut self) {
        self.heap.call_one_pending_finalizer();
    }
}

/// 5.4 work units of traversing the gray object `o` (`propagatemark`'s return value).
///
/// # Safety
/// `o` is a live object of a kind with a gray list (G1).
pub(super) unsafe fn work54(o: *mut GcHeader) -> usize {
    // SAFETY: G1, F1/F4, U1, ST2.
    unsafe {
        match (*o).tt {
            LUA_VTABLE => ga::work54(o.cast()),
            LUA_VUSERDATA => 1 + (*o.cast::<Udata>()).nuvalue as usize,
            LUA_VLCL => 1 + (*o.cast::<LClosure>()).nupvalues as usize,
            LUA_VCCL => 1 + (*o.cast::<CClosure>()).nupvalues as usize,
            LUA_VPROTO => {
                let f = o.cast::<Proto>();
                1 + (*f).k.cap() + (*f).upvalues.cap() + (*f).p.cap() + (*f).locvars.cap()
            }
            LUA_VTHREAD => {
                let th = o.cast::<Thread>();
                if (*th).stack.cap() == 0 {
                    1 // stack not completely built yet
                } else {
                    1 + (*th).stack_size
                }
            }
            _ => 0,
        }
    }
}

impl Heap {
    /// `GCdebt` (bytes allocated past the point where a step is due; a step is due when `> 0`).
    pub fn debt54(&self) -> isize {
        (self.mem.total_bytes() as isize)
            .saturating_sub(self.gc.threshold as isize)
            .saturating_add(1)
    }

    /// `luaE_setdebt`.
    pub fn set_debt54(&mut self, debt: isize) {
        let t = (self.mem.total_bytes() as isize)
            .saturating_sub(debt)
            .saturating_add(1);
        self.gc.threshold = t.max(0) as usize;
    }

    /// `isdecGCmodegen`: generational mode, or incremental after a bad collection (the mode
    /// `lua_gc` reports and the one `luaC_step` follows).
    pub fn is_decgcmodegen(&self) -> bool {
        self.gc.kind == GcKind::GenMinor || self.gc.lastatomic != 0
    }

    /// 5.4 `setpause`: the next cycle starts when memory reaches `estimate * pause%`.
    pub(super) fn set_pause54(&mut self) {
        // J5 sinking (TR29.4): every end of an incremental cycle credits the virtual bytes charged before it started
        // (the 5.4 step code calls this directly, not through `Heap::set_pause`; a no-op when already credited)
        self.jit_uncharge_cycle();
        let pause = GcParams::get54(self.gc.params.v54.pause) as usize;
        let estimate = self.gc.estimate / PAUSEADJ;
        let threshold = estimate.saturating_mul(pause).min(isize::MAX as usize);
        let debt = (self.mem.total_bytes() as isize - threshold as isize).min(0);
        self.set_debt54(debt);
    }

    /// `setminordebt`: the next minor collection happens when memory grows `genminormul`%.
    fn set_minor_debt54(&mut self) {
        self.jit_uncharge_all();
        let mul = self.gc.params.v54.genminormul as isize;
        let total = self.mem.total_bytes();
        self.set_debt54(-((total / 100) as isize).saturating_mul(mul));
    }

    /// `whitelist`: make every object of a list white with age `G_NEW`.
    fn whitelist(&mut self, mut p: *mut GcHeader) {
        let white = self.current_white & WHITEBITS;
        while !p.is_null() {
            // SAFETY: G1.
            unsafe {
                (*p).marked = ((*p).marked & !MASKGCBITS) | white;
                p = (*p).next;
            }
        }
    }

    /// `enterinc`: turn all objects white, clear the generational boundaries and go to the pause
    /// state of incremental mode.
    pub(super) fn enterinc54(&mut self) {
        self.whitelist(self.allgc);
        self.whitelist(self.gc.finobj);
        self.whitelist(self.gc.tobefnz);
        let null = std::ptr::null_mut();
        self.gc.reallyold = null;
        self.gc.old1 = null;
        self.gc.survival = null;
        self.gc.firstold1 = null;
        self.gc.finobjrold = null;
        self.gc.finobjold1 = null;
        self.gc.finobjsur = null;
        // (the reference leaves the stale gray lists to `restartcollection`)
        self.gc.gray = null;
        self.gc.grayagain = null;
        self.gc.weak = null;
        self.gc.allweak = null;
        self.gc.ephemeron = null;
        self.gc.phase = GcPhase::Pause;
        self.gc.kind = GcKind::Incremental;
        self.gc.lastatomic = 0;
    }

    /// `atomic`, returning its work (an estimate of the number of slots traversed).
    fn atomic54(&mut self, roots: &mut dyn FnMut(&mut Marker)) -> usize {
        debug_assert!(!self.gc.in_collect, "collector is not reentrant");
        self.gc.in_collect = true;
        self.gc.work54 = 0;
        self.atomic(roots);
        self.gc.in_collect = false;
        self.gc.work54
    }

    /// `atomic2gen` up to the finalizer calls of `finishgencycle` (done by the host): sweep
    /// everything to old and prepare the sub-lists of generational mode.
    fn atomic2gen54(&mut self) {
        self.gc.in_collect = true;
        let null = std::ptr::null_mut();
        self.gc.gray = null;
        self.gc.grayagain = null;
        self.gc.weak = null;
        self.gc.allweak = null;
        self.gc.ephemeron = null;
        self.gc.phase = GcPhase::SweepAllGc;
        self.sweep2old(ObjList::AllGc);
        // everything alive now is old
        self.gc.reallyold = self.allgc;
        self.gc.old1 = self.allgc;
        self.gc.survival = self.allgc;
        self.gc.firstold1 = null; // there are no OLD1 objects anywhere
        self.sweep2old(ObjList::FinObj);
        self.gc.finobjrold = self.gc.finobj;
        self.gc.finobjold1 = self.gc.finobj;
        self.gc.finobjsur = self.gc.finobj;
        self.sweep2old(ObjList::ToBeFnz);
        self.gc.kind = GcKind::GenMinor;
        self.gc.lastatomic = 0;
        self.gc.estimate = self.real_total(); // base for memory control
        self.finishgencycle();
        self.gc.in_collect = false;
    }

    /// `youngcollection` up to the finalizer calls of `finishgencycle` (done by the host).
    fn young_collection54(&mut self, roots: &mut dyn FnMut(&mut Marker)) {
        debug_assert_eq!(self.gc.phase, GcPhase::Propagate);
        debug_assert!(!self.gc.in_collect, "collector is not reentrant");
        self.gc.in_collect = true;
        let null: *mut GcHeader = std::ptr::null_mut();
        let mut unused = 0usize; // (5.5's count of bytes turned old)
        if !self.gc.firstold1.is_null() {
            self.markold(self.gc.firstold1, self.gc.reallyold);
            self.gc.firstold1 = null;
        }
        self.markold(self.gc.finobj, self.gc.finobjrold);
        self.markold(self.gc.tobefnz, null);

        self.atomic(roots);

        // SAFETY: G1; the boundary pointers are null or live objects of the respective lists.
        unsafe {
            // sweep the nursery and get the last live element of it
            self.gc.phase = GcPhase::SweepAllGc;
            let mut first = self.gc.firstold1;
            let psurvival = self.sweepgen(
                ObjList::AllGc,
                null,
                self.gc.survival,
                &mut first,
                &mut unused,
            );
            // sweep 'survival'
            self.sweepgen(
                ObjList::AllGc,
                psurvival,
                self.gc.old1,
                &mut first,
                &mut unused,
            );
            self.gc.firstold1 = first;
            self.gc.reallyold = self.gc.old1;
            self.gc.old1 = self.list_next(ObjList::AllGc, psurvival); // survivals are old now
            self.gc.survival = self.allgc; // all news are survivals

            // repeat for the 'finobj' lists (no 'firstold1' optimisation there)
            let mut dummy = null;
            let psurvival = self.sweepgen(
                ObjList::FinObj,
                null,
                self.gc.finobjsur,
                &mut dummy,
                &mut unused,
            );
            self.sweepgen(
                ObjList::FinObj,
                psurvival,
                self.gc.finobjold1,
                &mut dummy,
                &mut unused,
            );
            self.gc.finobjrold = self.gc.finobjold1;
            self.gc.finobjold1 = self.list_next(ObjList::FinObj, psurvival);
            self.gc.finobjsur = self.gc.finobj;

            self.sweepgen(ObjList::ToBeFnz, null, null, &mut dummy, &mut unused);
        }
        self.finishgencycle();
        self.gc.in_collect = false;
    }

    /// One incremental `singlestep`: its work, or `None` in the call-finalizers state with
    /// finalizers pending (the host runs them).
    fn singlestep54(&mut self, roots: &mut dyn FnMut(&mut Marker)) -> Option<usize> {
        match self.singlestep(roots, false) {
            Step::Work(w) => Some(w),
            Step::NeedFin => None,
            Step::Paused | Step::Atomic | Step::Minor => Some(0),
        }
    }
}

/// `runafewfinalizers` (`max` = `usize::MAX`: `callallpendingfinalizers`). Returns how many ran.
fn run_finalizers<H: Host54>(h: &mut H, max: usize) -> usize {
    let mut n = 0;
    while n < max {
        let heap = h.gc_heap();
        if !heap.has_pending_finalizers() || heap.gc.defer_fin {
            break;
        }
        h.gc_call_finalizer();
        n += 1;
    }
    n
}

/// The finalizer part of `finishgencycle`.
fn finishgencycle_fin<H: Host54>(h: &mut H) {
    if !h.gc_heap().gc.emergency {
        run_finalizers(h, usize::MAX);
    }
}

/// `luaC_runtilstate`: finalizers met in the call-finalizers state run on the way.
fn runtilstate<H: Host54>(h: &mut H, target: GcPhase) {
    while h.gc_with_roots(|heap, roots| heap.run_until(target, roots)) == StepOutcome::NeedFin {
        run_finalizers(h, GCFINMAX);
    }
}

/// `entergen`: run a whole atomic cycle, turn everything old. Returns the atomic work.
fn entergen54<H: Host54>(h: &mut H) -> usize {
    runtilstate(h, GcPhase::Pause); // prepare to start a new cycle
    runtilstate(h, GcPhase::Propagate); // start the new cycle
    let numobjs = h.gc_with_roots(|heap, roots| heap.atomic54(roots));
    h.gc_heap().atomic2gen54();
    finishgencycle_fin(h);
    h.gc_heap().set_minor_debt54(); // assume the next cycle will be minor
    numobjs
}

/// `fullgen`: a full collection in generational mode.
fn fullgen54<H: Host54>(h: &mut H) -> usize {
    h.gc_heap().enterinc54();
    entergen54(h)
}

/// `stepgenfull`: a major collection after a bad one; go back to generational mode only if this
/// collection traverses less than 9/8 of what the last one did.
fn stepgenfull54<H: Host54>(h: &mut H) {
    let heap = h.gc_heap();
    let lastatomic = heap.gc.lastatomic;
    if heap.gc.kind == GcKind::GenMinor {
        heap.enterinc54();
    }
    runtilstate(h, GcPhase::Propagate); // start a new cycle
    let newatomic = h.gc_with_roots(|heap, roots| heap.atomic54(roots)); // mark everybody
    if newatomic < lastatomic + (lastatomic >> 3) {
        // good collection: return to generational mode
        h.gc_heap().atomic2gen54();
        finishgencycle_fin(h);
        h.gc_heap().set_minor_debt54();
    } else {
        // another bad collection: stay in incremental mode
        let heap = h.gc_heap();
        heap.gc.estimate = heap.real_total(); // first estimate
        heap.entersweep();
        runtilstate(h, GcPhase::Pause); // finish the collection
        let heap = h.gc_heap();
        heap.set_pause54();
        heap.gc.lastatomic = newatomic;
    }
}

/// `genstep`: usually a minor collection; a major one (`fullgen`) when memory grew `genmajormul`%
/// since the last major collection, and `stepgenfull` after a bad one.
fn genstep54<H: Host54>(h: &mut H) {
    let heap = h.gc_heap();
    if heap.gc.lastatomic != 0 {
        stepgenfull54(h); // the last collection was a bad one
        return;
    }
    let majorbase = heap.gc.estimate; // memory after the last major collection
    let majorinc =
        (majorbase / 100).saturating_mul(GcParams::get54(heap.gc.params.v54.genmajormul) as usize);
    if heap.debt54() > 0 && heap.mem.total_bytes() > majorbase.saturating_add(majorinc) {
        let numobjs = fullgen54(h); // a major collection
        let heap = h.gc_heap();
        if heap.mem.total_bytes() < majorbase + majorinc / 2 {
            // collected at least half of the growth: keep doing minor collections
            debug_assert_eq!(heap.gc.lastatomic, 0);
        } else {
            heap.gc.lastatomic = numobjs; // bad collection
            heap.set_pause54(); // do a long wait for the next (major) collection
        }
    } else {
        // the regular case: a minor collection
        h.gc_with_roots(|heap, roots| heap.young_collection54(roots));
        finishgencycle_fin(h);
        let heap = h.gc_heap();
        heap.set_minor_debt54();
        heap.gc.estimate = majorbase; // preserve the base value
    }
}

/// `incstep`: pay the debt plus one step size, both converted to work units.
fn incstep54<H: Host54>(h: &mut H) {
    let heap = h.gc_heap();
    let p = heap.gc.params.v54;
    let stepmul = (GcParams::get54(p.stepmul) | 1) as isize; // avoid division by 0
    let mut debt = (heap.debt54() / WORK2MEM).saturating_mul(stepmul);
    let stepsize = if p.stepsize <= 62 {
        ((1isize << p.stepsize) / WORK2MEM).saturating_mul(stepmul)
    } else {
        isize::MAX
    };
    let one_step = heap.gc.stress_steps; // GC stress 2: a single step at every safepoint
    loop {
        let work = match h.gc_with_roots(|heap, roots| heap.singlestep54(roots)) {
            Some(w) => w as isize,
            None => run_finalizers(h, GCFINMAX) as isize * GCFINALIZECOST,
        };
        debt = debt.saturating_sub(work);
        if one_step || debt <= -stepsize || h.gc_heap().gc.phase == GcPhase::Pause {
            break;
        }
    }
    let heap = h.gc_heap();
    if heap.gc.phase == GcPhase::Pause {
        heap.set_pause54(); // pause until the next cycle
    } else {
        heap.set_debt54((debt / stepmul).saturating_mul(WORK2MEM));
    }
}

/// `luaC_step`.
pub(crate) fn luac_step54<H: Host54>(h: &mut H) {
    let heap = h.gc_heap();
    if !heap.gc_is_running() {
        heap.set_debt54(-2000);
        return;
    }
    if heap.gc.stress_steps && heap.gc.kind == GcKind::GenMinor {
        // GC stress 2: one young collection per safepoint, and a major collection every
        // `STRESS_MAJOR` steps. (Objects age to old within a few safepoints, so without majors
        // a script waiting for old garbage to be collected would never finish; `genstep` itself
        // would turn every step into a major once memory doubled, as stress ignores the debt.)
        heap.gc.stress_count = heap.gc.stress_count.wrapping_add(1);
        if heap.gc.stress_count.is_multiple_of(STRESS_MAJOR) {
            fullgen54(h);
        } else {
            h.gc_with_roots(|heap, roots| heap.young_collection54(roots));
            finishgencycle_fin(h);
            h.gc_heap().set_minor_debt54();
        }
    } else if heap.is_decgcmodegen() {
        genstep54(h);
    } else {
        incstep54(h);
    }
    let heap = h.gc_heap();
    if heap.gc.verify_steps {
        heap.verify();
    }
}

/// `fullinc`: a full collection in incremental mode.
fn fullinc54<H: Host54>(h: &mut H) {
    let heap = h.gc_heap();
    if heap.keepinvariant() {
        heap.entersweep(); // sweep everything to turn black objects back to white
    }
    runtilstate(h, GcPhase::Pause); // finish any pending sweep phase
    runtilstate(h, GcPhase::Propagate); // start a new cycle
    h.gc_heap().gc.phase = GcPhase::Atomic; // go straight to the atomic phase
    runtilstate(h, GcPhase::CallFin); // run up to the finalizers
    runtilstate(h, GcPhase::Pause); // finish the collection
    h.gc_heap().set_pause54();
}

/// `luaC_fullgc`.
pub(crate) fn fullgc54<H: Host54>(h: &mut H, emergency: bool) {
    let heap = h.gc_heap();
    if heap.gc.in_collect {
        return;
    }
    heap.jit_uncharge_all();
    heap.gc.emergency = emergency;
    if heap.gc.kind == GcKind::Incremental {
        fullinc54(h);
    } else {
        fullgen54(h);
    }
    h.gc_heap().gc.emergency = false;
}

/// `luaC_changemode`.
pub(crate) fn changemode54<H: Host54>(h: &mut H, new: GcKind) {
    let heap = h.gc_heap();
    let gen = new.is_generational();
    if gen != (heap.gc.kind == GcKind::GenMinor) {
        if gen {
            entergen54(h);
        } else {
            heap.enterinc54();
        }
    }
    h.gc_heap().gc.lastatomic = 0;
}
