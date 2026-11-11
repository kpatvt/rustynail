//! Generational mode (G3): the generational part of `lgc.c` (Lua 5.5): object ages, young
//! collections, the switches between minor and major collections and between the two modes.
//!
//! In generational mode the collector sits in the `Propagate` phase between collections (as the
//! reference sits in `GCSpropagate`), so `keepinvariant` is true and the barriers are active. New
//! objects are pushed on the head of `allgc`; the list is kept ordered by age through the boundary
//! pointers `survival`, `old1` and `reallyold` (and `finobjsur`, `finobjold1`, `finobjrold` for
//! `finobj`), so a young collection only sweeps the young prefix.
//!
//! List positions are tracked as *predecessor object pointers* (null = list head), never as
//! pointers into `self` (see `sweep_some`). Finalizers are not called from here: `State` runs the
//! pending ones after a young collection (`finishgencycle`).
use super::*;

/// `nextage` of `sweepgen`: the age an object has after surviving a young collection.
const NEXTAGE: [u8; 7] = [
    G_SURVIVAL, // from G_NEW
    G_OLD1,     // from G_SURVIVAL
    G_OLD1,     // from G_OLD0
    G_OLD,      // from G_OLD1
    G_OLD,      // from G_OLD (do not change)
    G_TOUCHED1, // from G_TOUCHED1 (do not change)
    G_TOUCHED2, // from G_TOUCHED2 (do not change)
];

/// See [`Heap::minor_debt_bytes`].
#[cfg(not(rustynail_lua54))]
const NURSERY_CAP: usize = 4 << 20;

impl Heap {
    /// The object after `prev` in `list` (`prev == null`: the list head).
    pub(super) unsafe fn list_next(&mut self, list: ObjList, prev: *mut GcHeader) -> *mut GcHeader {
        // SAFETY: G1.
        unsafe {
            if prev.is_null() {
                *self.list_head(list)
            } else {
                (*prev).next
            }
        }
    }

    /// Unlink (and free) the object `curr` that follows `prev` in `list`.
    unsafe fn list_remove_after(
        &mut self,
        list: ObjList,
        prev: *mut GcHeader,
        curr: *mut GcHeader,
    ) {
        // SAFETY: G1; `curr` follows `prev`.
        unsafe {
            let next = (*curr).next;
            if prev.is_null() {
                *self.list_head(list) = next;
            } else {
                (*prev).next = next;
            }
            self.free_obj(curr);
        }
    }

    /// `sweep2old`: sweep a list to enter generational mode: delete dead objects and turn all
    /// survivors old. Threads go to `grayagain`; open upvalues stay gray; everything else is black.
    pub(super) fn sweep2old(&mut self, list: ObjList) {
        let mut prev: *mut GcHeader = std::ptr::null_mut();
        // SAFETY: G1.
        unsafe {
            loop {
                let curr = self.list_next(list, prev);
                if curr.is_null() {
                    break;
                }
                if iswhite(curr) {
                    debug_assert!((*curr).marked & (self.current_white ^ WHITEBITS) != 0);
                    self.list_remove_after(list, prev, curr);
                } else {
                    setage(curr, G_OLD);
                    if (*curr).tt == LUA_VTHREAD {
                        self.linkgclist(curr, List::GrayAgain); // threads must be watched
                    } else if (*curr).tt == LUA_VUPVAL
                        && Gc::from_raw(curr.cast::<UpVal>()).is_open()
                    {
                        set2gray(curr); // open upvalues are always gray
                    } else {
                        nw2black(curr); // everything else is black
                    }
                    prev = curr;
                }
            }
        }
    }

    /// `sweepgen`: sweep `list` from after `prev` up to (not including) `limit`. Dead (white)
    /// objects are freed; survivors advance their age. Returns the last surviving object visited
    /// (or `prev`).
    pub(super) unsafe fn sweepgen(
        &mut self,
        list: ObjList,
        mut prev: *mut GcHeader,
        limit: *mut GcHeader,
        firstold1: &mut *mut GcHeader,
        addedold: &mut usize,
    ) -> *mut GcHeader {
        let white = self.current_white & WHITEBITS;
        // SAFETY: G1.
        unsafe {
            loop {
                let curr = self.list_next(list, prev);
                if curr == limit {
                    break;
                }
                if iswhite(curr) {
                    debug_assert!(!isold(curr));
                    self.list_remove_after(list, prev, curr);
                } else {
                    let age = getage(curr);
                    if age == G_NEW {
                        // new objects go back to white
                        (*curr).marked = ((*curr).marked & !MASKGCBITS) | G_SURVIVAL | white;
                    } else {
                        // all other objects will be old, and so keep their colour
                        debug_assert_ne!(age, G_OLD1, "advanced in markold");
                        setage(curr, NEXTAGE[age as usize]);
                        if getage(curr) == G_OLD1 {
                            *addedold += objsize(curr);
                            if firstold1.is_null() {
                                *firstold1 = curr;
                            }
                        }
                    }
                    prev = curr;
                }
            }
        }
        prev
    }

    /// `correctgraylist`: filter the gray list starting at `curr` and append the survivors to the
    /// list being built (`first`/`last`).
    unsafe fn correctgraylist(
        &mut self,
        mut curr: *mut GcHeader,
        first: &mut *mut GcHeader,
        last: &mut *mut GcHeader,
    ) {
        // SAFETY: G1; list members have a `gclist`.
        unsafe {
            while !curr.is_null() {
                let next = *getgclist(curr);
                let keep = if iswhite(curr) {
                    false // remove all white objects
                } else if getage(curr) == G_TOUCHED1 {
                    // touched in this cycle: black for the next barrier, advance, stay in the list
                    nw2black(curr);
                    setage(curr, G_TOUCHED2);
                    true
                } else if (*curr).tt == LUA_VTHREAD {
                    true // keep non-white threads on the list
                } else {
                    // everything else is removed
                    debug_assert!(isold(curr));
                    if getage(curr) == G_TOUCHED2 {
                        setage(curr, G_OLD);
                    }
                    nw2black(curr);
                    false
                };
                if keep {
                    if last.is_null() {
                        *first = curr;
                    } else {
                        *getgclist(*last) = curr;
                    }
                    *last = curr;
                }
                curr = next;
            }
        }
    }

    /// `correctgraylists`: correct all gray lists, coalescing them into `grayagain`.
    fn correctgraylists(&mut self) {
        let mut first: *mut GcHeader = std::ptr::null_mut();
        let mut last: *mut GcHeader = std::ptr::null_mut();
        let lists = [
            std::mem::replace(&mut self.gc.grayagain, std::ptr::null_mut()),
            std::mem::replace(&mut self.gc.weak, std::ptr::null_mut()),
            std::mem::replace(&mut self.gc.allweak, std::ptr::null_mut()),
            std::mem::replace(&mut self.gc.ephemeron, std::ptr::null_mut()),
        ];
        // SAFETY: G1.
        unsafe {
            for l in lists {
                self.correctgraylist(l, &mut first, &mut last);
            }
            if !last.is_null() {
                *getgclist(last) = std::ptr::null_mut();
            }
        }
        self.gc.grayagain = first;
    }

    /// `markold`: mark black `OLD1` objects when starting a young collection (`from` up to `to`).
    pub(super) fn markold(&mut self, from: *mut GcHeader, to: *mut GcHeader) {
        let mut p = from;
        // SAFETY: G1.
        unsafe {
            while p != to {
                if getage(p) == G_OLD1 {
                    debug_assert!(!iswhite(p));
                    setage(p, G_OLD); // now they are old
                    if (*p).marked & BLACKBIT != 0 {
                        self.reallymark(p);
                    }
                }
                p = (*p).next;
            }
        }
    }

    /// `finishgencycle` (without calling finalizers; `State` does).
    pub(super) fn finishgencycle(&mut self) {
        self.correctgraylists();
        self.check_sizes();
        self.gc.phase = GcPhase::Propagate; // skip restart
    }

    /// `minor2inc`: shift from generational minor collections to an incremental cycle (a major
    /// collection, or plain incremental mode). Starts in the "sweep all" state.
    pub(super) fn minor2inc(&mut self, kind: GcKind) {
        self.gc.majorminor = self.gc.marked;
        self.gc.kind = kind;
        let null = std::ptr::null_mut();
        self.gc.reallyold = null;
        self.gc.old1 = null;
        self.gc.survival = null;
        self.gc.finobjrold = null;
        self.gc.finobjold1 = null;
        self.gc.finobjsur = null;
        self.entersweep();
        self.gc.threshold = self.mem.total_bytes() + self.step_size_bytes();
    }

    /// `checkminormajor`: shift to major mode if the bytes that became old since the last major
    /// collection exceed `minormajor`% of the bytes alive after it.
    fn check_minor_major(&self) -> bool {
        let limit = self.gc.params.apply(GCP_MINORMAJOR, self.gc.majorminor);
        if limit == 0 {
            return false; // special case: 'minormajor' 0 stops major collections
        }
        self.gc.marked >= limit
    }

    /// `youngcollection`.
    fn young_collection(&mut self, roots: &mut dyn FnMut(&mut Marker)) {
        debug_assert_eq!(self.gc.phase, GcPhase::Propagate);
        self.gc.in_collect = true;
        let mut addedold1 = 0usize;
        let marked = self.gc.marked; // preserve it: atomic loses it
        let null: *mut GcHeader = std::ptr::null_mut();
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
                &mut addedold1,
            );
            // sweep 'survival'
            self.sweepgen(
                ObjList::AllGc,
                psurvival,
                self.gc.old1,
                &mut first,
                &mut addedold1,
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
                &mut addedold1,
            );
            self.sweepgen(
                ObjList::FinObj,
                psurvival,
                self.gc.finobjold1,
                &mut dummy,
                &mut addedold1,
            );
            self.gc.finobjrold = self.gc.finobjold1;
            self.gc.finobjold1 = self.list_next(ObjList::FinObj, psurvival);
            self.gc.finobjsur = self.gc.finobj;

            self.sweepgen(ObjList::ToBeFnz, null, null, &mut dummy, &mut addedold1);
        }

        // keep the total number of added old1 bytes
        self.gc.marked = marked + addedold1;
        if self.check_minor_major() {
            self.minor2inc(GcKind::GenMajor); // go to major mode
            self.gc.marked = 0; // avoid pause in the first major cycle (see `set_pause`)
        } else {
            self.finishgencycle();
        }
        self.gc.in_collect = false;
    }

    #[cfg(not(rustynail_lua54))]
    /// `atomic2gen`: clear the gray lists, sweep everything to old and prepare the sub-lists to
    /// enter generational mode.
    pub(super) fn atomic2gen(&mut self) {
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
        self.gc.majorminor = self.gc.marked; // "base" for the number of bytes
        self.gc.marked = 0; // to count the number of added old1 bytes
        self.finishgencycle();
    }

    /// `setminordebt`: the next minor collection happens after `minormul`% more bytes than were
    /// alive after the last major collection.
    fn set_minor_debt(&mut self) {
        self.jit_uncharge_all();
        self.gc.threshold =
            self.mem.total_bytes() + self.minor_debt_bytes();
    }

    /// Bytes allocated between two young collections: `minormul`% of the bytes alive after the last major collection (`d`).
    ///
    /// BUNDLE-A item 1: while the program has not set `minormul` itself the nursery is larger on small and medium heaps: up to 20 times
    /// `d` (400% of the live bytes), but not above `max(5 * d, NURSERY_CAP)`, i.e. never less than 100% of the live bytes and never more
    /// than 4 MiB unless that is below 100%. A young collection has a fixed cost (roots, gray lists, boundary pointers) and re-traverses
    /// the survivors of the previous ones, and every few of them on a small heap end in a major collection; the reference's 20% is too
    /// eager for that (profile study: 4.8k cycles per minor on a 40 KB heap, a minor every 44 tables). `collectgarbage("param",
    /// "minormul")` still reports the reference's 20 and an explicit value is used as given.
    #[inline]
    pub(crate) fn minor_debt_bytes(&self) -> usize {
        let d = self.gc.params.apply(GCP_MINORMUL, self.gc.majorminor);
        // (the Lua 5.4 collector, `gen54.rs`, has its own pacing and keeps the reference's)
        #[cfg(rustynail_lua54)]
        return d;
        #[cfg(not(rustynail_lua54))]
        if self.gc.params.minormul_user {
            d
        } else {
            d.saturating_mul(20).min(d.saturating_mul(5).max(NURSERY_CAP))
        }
    }

    #[cfg(not(rustynail_lua54))]
    /// `entergen`: run a whole atomic cycle (so every object is correctly marked and weak tables
    /// are cleared), then turn everything old.
    fn entergen(&mut self, roots: &mut dyn FnMut(&mut Marker)) {
        let olddefer = std::mem::replace(&mut self.gc.defer_fin, true); // the caller runs finalizers
        let _ = self.run_until(GcPhase::Pause, roots); // prepare to start a new cycle
        let _ = self.run_until(GcPhase::Propagate, roots); // start the new cycle
        self.gc.in_collect = true;
        self.atomic(roots); // propagates everything and then does the atomic stuff
        self.atomic2gen();
        self.gc.in_collect = false;
        self.set_minor_debt();
        self.gc.defer_fin = olddefer;
    }

    /// `luaC_changemode`.
    pub fn change_mode(&mut self, roots: &mut dyn FnMut(&mut Marker), new: GcKind) {
        #[cfg(rustynail_lua54)]
        {
            super::gen54::changemode54(&mut super::gen54::HeapHost { heap: self, roots }, new);
            self.gc.defer_fin = false;
        }
        #[cfg(not(rustynail_lua54))]
        self.change_mode55(roots, new);
    }

    #[cfg(not(rustynail_lua54))]
    #[inline(always)]
    fn change_mode55(&mut self, roots: &mut dyn FnMut(&mut Marker), new: GcKind) {
        if self.gc.kind == GcKind::GenMajor {
            self.gc.kind = GcKind::Incremental; // already incremental, but in name
        }
        if new.is_generational() == self.gc.kind.is_generational() {
            return;
        }
        if new == GcKind::Incremental {
            self.minor2inc(GcKind::Incremental);
        } else {
            self.entergen(roots);
        }
    }

    /// Leave generational mode without running a cycle (`luaC_changemode(L, KGC_INC)` as done by
    /// `luaC_freeallobjects`): needed before every finalizable object is queued at close, because
    /// `separatetobefnz` only looks at the young part of `finobj` in generational mode.
    pub(super) fn leave_generational(&mut self) {
        // 5.4 `luaC_changemode(L, KGC_INC)`: `enterinc`
        #[cfg(rustynail_lua54)]
        {
            if self.gc.kind.is_generational() {
                self.enterinc54();
            }
            self.gc.lastatomic = 0;
        }
        #[cfg(not(rustynail_lua54))]
        if self.gc.kind.is_generational() {
            self.gc.kind = GcKind::Incremental;
            self.minor2inc(GcKind::Incremental);
        }
    }

    #[cfg(not(rustynail_lua54))]
    /// `fullgen`: a full collection in generational mode.
    pub(super) fn fullgen(&mut self, roots: &mut dyn FnMut(&mut Marker)) {
        self.minor2inc(GcKind::Incremental);
        self.entergen(roots);
    }

    #[cfg(not(rustynail_lua54))]
    /// `checkmajorminor`: after the atomic step of a major collection, check whether the collector
    /// can return to minor collections. Returns true if it did.
    pub(super) fn check_major_minor(&mut self) -> bool {
        if self.gc.kind == GcKind::GenMajor {
            let numbytes = self.mem.total_bytes();
            let addedbytes = numbytes.saturating_sub(self.gc.majorminor);
            let limit = self.gc.params.apply(GCP_MAJORMINOR, addedbytes);
            let tobecollected = numbytes.saturating_sub(self.gc.marked);
            if tobecollected > limit {
                self.atomic2gen(); // return to generational mode
                self.set_minor_debt();
                return true; // exit the incremental collection
            }
        }
        self.gc.majorminor = self.gc.marked; // prepare for the next collection
        false
    }

    /// One generational step (`luaC_step` in `KGC_GENMINOR`): a young collection.
    pub fn gen_step(&mut self, roots: &mut dyn FnMut(&mut Marker)) {
        debug_assert_eq!(self.gc.kind, GcKind::GenMinor);
        self.young_collection(roots);
        self.set_minor_debt();
        if self.gc.verify_steps {
            self.verify();
        }
    }
}
