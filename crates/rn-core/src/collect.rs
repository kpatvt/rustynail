//! Glue between the collector (`gc/`) and the interpreter state: safepoints, root marking,
//! finalizer calls (`GCTM`), emergency collections and state closing support.
//!
//! The collector itself ([`crate::heap::Heap`] in `gc/`) never runs Lua code: `State::collect`
//! runs a complete mark & sweep with roots {main thread, registry, per-type metatables} and then,
//! outside of any heap borrow, calls the pending `__gc` finalizers one by one (`callallpendingfinalizers`)
//! with the reference's protections (`allowhook = false`, collections suppressed, errors become
//! warnings).
//!
//! **Safepoint contract:** `State::check_gc` may be called only when every live value is reachable from
//! the stack *below `top`*, the registry or the metatables. A Lua function's frame is only scanned up
//! to `top`, so the interpreter must raise `top` to `ci.top` first (the reference's
//! `luaC_condGC(L, (savepc, L->top = c), ...)`).
use crate::gc::{GcKind, Marker, GCSTP_GC};
#[cfg(not(rustynail_lua54))]
use crate::gc::{StepOutcome, CWUFIN};
use crate::heap::Heap;
use crate::meta::Tm;
use crate::state::*;
use crate::status::Status;
use crate::value::*;

impl State {
    /// `luaC_checkGC`: collect if the allocation threshold was crossed (or always under
    /// `RUSTYNAIL_GC_STRESS`). Finalizers run on return of the collection.
    #[inline]
    pub fn check_gc(&mut self) {
        if self.g().heap.gc_due() {
            self.gc_step();
        }
    }

    /// Run `f` with the heap and the root-marking callback (main thread, running thread, registry,
    /// per-type metatables).
    pub(crate) fn with_roots<R>(
        &mut self,
        f: impl FnOnce(&mut Heap, &mut dyn FnMut(&mut Marker)) -> R,
    ) -> R {
        let cur = self.th_ptr();
        let g = self.g_mut();
        let GlobalState {
            heap,
            registry,
            mt,
            mainthread,
            ..
        } = g;
        let mainthread = *mainthread;
        let mut roots = |m: &mut Marker| {
            // SAFETY: the main thread and the running thread are live objects of this heap.
            unsafe {
                m.mark_object(mainthread.cast());
                m.mark_object(cur.cast());
            }
            m.mark_value(registry);
            for t in mt.iter().flatten() {
                m.mark_table(*t);
            }
        };
        f(heap, &mut roots)
    }

    /// `luaC_step`: one collector step (stress mode 1: a full collection). Finalizers are called
    /// from here, interleaved with the collector's work, outside of any heap borrow.
    pub fn gc_step(&mut self) {
        if self.g().heap.gc_stress() {
            self.collect(false);
            return;
        }
        #[cfg(rustynail_lua54)]
        crate::gc::gen54_step(self);
        #[cfg(not(rustynail_lua54))]
        self.gc_step55();
        // P-B: the room the collector left before the next step (the allocation storm guard of the J5 recorder)
        self.heap().note_step_gap();
    }

    #[cfg(not(rustynail_lua54))]
    #[inline(always)]
    fn gc_step55(&mut self) {
        if self.g().heap.gc_kind() == GcKind::GenMinor {
            // a young collection; finishing it calls the pending finalizers
            self.with_roots(|heap, roots| heap.gen_step(roots));
            self.call_all_pending_finalizers();
            return;
        }
        let mut budget = self.g().heap.step_budget();
        loop {
            let outcome = self.with_roots(|heap, roots| heap.inc_step(roots, &mut budget));
            if outcome == StepOutcome::Done {
                return;
            }
            self.gctm(); // call one finalizer
            budget -= CWUFIN;
            if budget <= 0 {
                self.heap().finish_step();
                return;
            }
        }
    }

    /// `luaC_changemode`: switch between incremental and generational mode; returns the previous
    /// mode.
    pub fn gc_change_mode(&mut self, new: GcKind) -> GcKind {
        #[cfg(rustynail_lua54)]
        {
            // the mode `lua_gc` reports: `isdecGCmodegen`
            let old = if self.g().heap.is_decgcmodegen() {
                GcKind::GenMinor
            } else {
                GcKind::Incremental
            };
            crate::gc::gen54_changemode(self, new);
            old
        }
        #[cfg(not(rustynail_lua54))]
        {
            let old = self.g().heap.gc_kind();
            self.with_roots(|heap, roots| heap.change_mode(roots, new));
            self.call_all_pending_finalizers();
            old
        }
    }

    /// 5.4 `lua_gc(LUA_GCSTEP, data)`: `data == 0` does one basic step, otherwise `data` Kbytes are
    /// added to the debt. Returns whether a cycle ended.
    #[cfg(rustynail_lua54)]
    pub fn gc_api_step54(&mut self, data: i32) -> bool {
        let oldstp = self.g().heap.gc_stp();
        self.heap().gc_set_stp(0); // allow the collector to run
        let debt = if data == 0 {
            self.heap().set_debt54(0); // do a basic step
            self.gc_step();
            1 // to signal that it did an actual step
        } else {
            let debt = (data as isize * 1024).saturating_add(self.g().heap.debt54());
            self.heap().set_debt54(debt);
            self.check_gc();
            debt
        };
        self.heap().gc_set_stp(oldstp);
        debt > 0 && self.g().heap.gc_phase() == crate::gc::GcPhase::Pause
    }

    /// `lua_gc(LUA_GCSTEP, n)`: `n == 0` forces one basic step, otherwise behave as if `n` more
    /// bytes had been allocated. Returns whether a cycle finished.
    pub fn gc_api_step(&mut self, n: usize) -> bool {
        let oldstp = self.g().heap.gc_stp();
        self.heap().gc_set_stp(0); // allow the collector to run
        let total = self.g().heap.total_bytes();
        let thr = if n == 0 {
            total
        } else {
            self.g().heap.gc_threshold().saturating_sub(n)
        };
        self.heap().gc_set_threshold(thr);
        let mut work = false;
        if self.g().heap.gc_due() {
            self.gc_step();
            work = true;
        }
        let ended = work && self.g().heap.gc_phase() == crate::gc::GcPhase::Pause;
        self.heap().gc_set_stp(oldstp);
        ended
    }

    /// `luaC_fullgc`: a full collection now. `emergency` collections skip finalizers and stack
    /// shrinking (they run in the middle of allocations).
    pub fn collect(&mut self, emergency: bool) {
        #[cfg(rustynail_lua54)]
        crate::gc::gen54_fullgc(self, emergency);
        #[cfg(not(rustynail_lua54))]
        self.collect55(emergency);
    }

    #[cfg(not(rustynail_lua54))]
    #[inline(always)]
    fn collect55(&mut self, emergency: bool) {
        self.with_roots(|heap, roots| {
            if emergency {
                heap.collect_emergency(roots)
            } else {
                heap.collect_no_finalizers(roots)
            }
        });
        if !emergency {
            self.call_all_pending_finalizers();
        }
    }

    /// Can an allocation failure be retried after an emergency collection (`cantryagain`)?
    pub fn can_emergency_gc(&self) -> bool {
        self.g().complete && !self.g().heap.gc_in_collect()
    }

    /// `GCTM`: call the finalizer of the next pending object.
    pub(crate) fn gctm(&mut self) {
        let o = self.heap().next_finalizable();
        // SAFETY: `o` is a live table/userdata (only those get finalizers) just returned to `allgc`.
        let v = unsafe { Value::from_gc((*o).tt, o) };
        let tm = self.get_tm_by_obj(&v, Tm::Gc);
        if tm.is_nil() {
            return;
        }
        let oldah = self.allow_hook();
        let oldstp = self.g().heap.gc_stp();
        self.heap().gc_set_stp(oldstp | GCSTP_GC); // avoid GC steps
        self.set_allow_hook(false); // stop debug hooks during GC metamethod
        if self.check_stack(2).is_err() {
            self.set_allow_hook(oldah);
            self.heap().gc_set_stp(oldstp);
            return;
        }
        let func = self.top();
        self.push(tm); // push finalizer...
        self.push(v); // ... and its argument
        let ci = self.ci();
        ci.w().callstatus |= CIST_FIN; // will run a finalizer
        let status = self.pcall_with(|s| s.call_no_yield(func, 0), func, 0);
        ci.w().callstatus &= !CIST_FIN;
        self.set_allow_hook(oldah);
        self.heap().gc_set_stp(oldstp);
        if status != Status::Ok {
            // error while running __gc: warn and pop the error object
            self.warn_error(b"__gc");
            let t = self.top();
            self.set_top(t - 1);
        }
    }

    /// `callallpendingfinalizers`.
    pub fn call_all_pending_finalizers(&mut self) {
        while self.g().heap.has_pending_finalizers() {
            self.gctm();
        }
    }

    /// Number of `__gc` finalizers waiting to run.
    pub fn pending_finalizers(&self) -> usize {
        self.g().heap.pending_finalizers()
    }
}

/// The interpreter is the host of the 5.4 collector control (finalizers run where the reference
/// calls them).
#[cfg(rustynail_lua54)]
impl crate::gc::Host54 for State {
    fn gc_heap(&mut self) -> &mut Heap {
        self.heap()
    }
    fn gc_with_roots<R>(
        &mut self,
        f: impl FnOnce(&mut Heap, &mut dyn FnMut(&mut Marker)) -> R,
    ) -> R {
        self.with_roots(f)
    }
    fn gc_call_finalizer(&mut self) {
        self.gctm();
    }
}
