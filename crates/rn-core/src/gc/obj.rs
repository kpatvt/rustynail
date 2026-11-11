//! Traversal and freeing of the object kinds added by task B3: userdata, closures, protos,
//! upvalues and threads (`traverseudata`, `traverseproto`, `traverseCclosure`, `traverseLclosure`,
//! `traversethread`, `remarkupvals`, `freeupval`, `luaF_freeproto`).
//!
//! These are `impl Heap` blocks living in a child module of `gc` so they can use the collector's
//! private helpers (`markobject`, `linkgclist`, colour predicates). Invariant G1 applies.
use super::*;
use crate::func::{CClosure, LClosure, Proto, UpVal};
use crate::state::{shrink_stack, Thread, EXTRA_STACK};
use crate::udata::Udata;

impl Heap {
    pub(super) unsafe fn traverse_udata(&mut self, u: *mut Udata) {
        // SAFETY: G1/U1.
        unsafe {
            self.markobject_n((*u).metatable.cast());
            let uvs = Gc::from_raw(u);
            for i in 0..(*u).nuvalue as usize {
                if let Some(v) = uvs.user_value(i) {
                    self.markvalue(&v);
                }
            }
            self.genlink(u.cast());
        }
    }

    pub(super) unsafe fn traverse_proto(&mut self, f: *mut Proto) {
        // SAFETY: G1/F4 — arrays hold `cap` initialised slots; pointer slots may be NULL.
        unsafe {
            self.markobject_n((*f).source.cast());
            for i in 0..(*f).k.cap() {
                let v = *(*f).k.as_ptr().add(i);
                self.markvalue(&v);
            }
            for i in 0..(*f).upvalues.cap() {
                let name = (*(*f).upvalues.as_ptr().add(i)).name;
                self.markobject_n(name.cast());
            }
            for i in 0..(*f).p.cap() {
                let sub = *(*f).p.as_ptr().add(i);
                self.markobject_n(sub.cast());
            }
            for i in 0..(*f).locvars.cap() {
                let name = (*(*f).locvars.as_ptr().add(i)).varname;
                self.markobject_n(name.cast());
            }
        }
    }

    pub(super) unsafe fn traverse_cclosure(&mut self, cl: *mut CClosure) {
        // SAFETY: G1/F1.
        unsafe {
            let c = Gc::from_raw(cl);
            for i in 0..c.nupvalues() {
                let v = c.upvalue(i);
                self.markvalue(&v);
            }
        }
    }

    pub(super) unsafe fn traverse_lclosure(&mut self, cl: *mut LClosure) {
        // SAFETY: G1/F1 — upvalue slots may be NULL while a closure is being built.
        unsafe {
            self.markobject_n((*cl).p.cast());
            let c = Gc::from_raw(cl);
            let ups = c.upvals_ptr();
            for i in 0..c.nupvalues() {
                self.markobject_n((*ups.add(i)).cast());
            }
        }
    }

    /// `traversethread`: mark the live part of the stack and the open upvalues; in the atomic phase
    /// also shrink the stack (not in emergency cycles) and clear the dead part.
    pub(super) unsafe fn traverse_thread(&mut self, th: *mut Thread) {
        // SAFETY: G1/ST1-ST3.
        unsafe {
            if isold(th.cast()) || self.gc.phase == GcPhase::Propagate {
                // threads have no barriers: old ones are revisited by every young collection,
                // and in the incremental propagate phase they are revisited in the atomic phase
                self.linkgclist(th.cast(), List::GrayAgain);
            }
            if (*th).stack.cap() == 0 {
                return; // stack not completely built yet
            }
            let base = (*th).stack.as_ptr();
            for i in 0..(*th).top.0 {
                let v = *base.add(i);
                self.markvalue(&v);
            }
            let mut uv = (*th).openupval;
            while !uv.is_null() {
                self.markobject(uv.cast()); // open upvalues cannot be collected
                uv = (*uv).u.open.next;
            }
            if self.gc.phase == GcPhase::Atomic {
                if !self.gc.emergency {
                    shrink_stack(th, &mut self.mem); // do not change stack in emergency cycle
                }
                let base = (*th).stack.as_ptr();
                for i in (*th).top.0..(*th).stack_size + EXTRA_STACK {
                    base.add(i).write(Value::NIL); // clear dead stack slice
                }
                // `remarkupvals` may have removed the thread from the `twups` list
                if (*th).twups == th && !(*th).openupval.is_null() {
                    (*th).twups = self.gc.twups;
                    self.gc.twups = th;
                }
            }
        }
    }

    /// `remarkupvals`: for each non-marked thread with open upvalues, simulate a barrier between
    /// each marked open upvalue and its value, and drop the thread from the `twups` list.
    pub(super) fn remarkupvals(&mut self) {
        // SAFETY: G1 — `twups` members are live threads; the predecessor is tracked as an object
        // pointer (null = list head), never as a pointer into `self`.
        unsafe {
            let mut prev: *mut Thread = std::ptr::null_mut();
            loop {
                let th = if prev.is_null() {
                    self.gc.twups
                } else {
                    (*prev).twups
                };
                if th.is_null() {
                    break;
                }
                #[cfg(rustynail_lua54)]
                {
                    self.gc.work54 += 1;
                }
                if !iswhite(th.cast()) && !(*th).openupval.is_null() {
                    prev = th; // keep marked thread with upvalues in the list
                } else {
                    let next = (*th).twups;
                    if prev.is_null() {
                        self.gc.twups = next;
                    } else {
                        (*prev).twups = next;
                    }
                    (*th).twups = th; // mark that it is out of the list
                    let mut uv = (*th).openupval;
                    while !uv.is_null() {
                        #[cfg(rustynail_lua54)]
                        {
                            self.gc.work54 += 1;
                        }
                        if !iswhite(uv.cast()) {
                            let v = *(*uv).v;
                            self.markvalue(&v);
                        }
                        uv = (*uv).u.open.next;
                    }
                }
            }
        }
    }

    /// `freeupval`.
    pub(super) unsafe fn free_upval(&mut self, uv: *mut UpVal) {
        // SAFETY: G1/F2 — an open upvalue is linked into its thread's list.
        unsafe {
            if Gc::from_raw(uv).is_open() {
                let next = (*uv).u.open.next;
                let prev = (*uv).u.open.previous;
                *prev = next;
                if !next.is_null() {
                    (*next).u.open.previous = prev;
                }
            }
            self.mem.free(uv.cast(), std::mem::size_of::<UpVal>());
        }
    }

    /// `luaF_freeproto`.
    pub(super) unsafe fn free_proto(&mut self, f: *mut Proto) {
        // SAFETY: G1/F4 — buffers were allocated from `self.mem`.
        unsafe {
            // compiled code dies with its prototype (a collected prototype has no live frame)
            #[cfg(feature = "jit")]
            crate::jit_hooks::free_proto_code(&mut self.mem, &*f);
            (*f).code.free(&mut self.mem);
            (*f).lineinfo.free(&mut self.mem);
            (*f).abslineinfo.free(&mut self.mem);
            (*f).p.free(&mut self.mem);
            (*f).k.free(&mut self.mem);
            (*f).locvars.free(&mut self.mem);
            (*f).upvalues.free(&mut self.mem);
            self.mem.free(f.cast(), std::mem::size_of::<Proto>());
        }
    }
}
