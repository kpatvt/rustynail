//! Coroutines: `lua_resume`, `lua_yieldk`, the unrolling of an interrupted coroutine
//! (`unroll`, `finishCcall`, `finishpcallk`, `precover`) and `lua_closethread` (port of the
//! second half of `ldo.c`).
//!
//! A yield is `Err(Status::Yield)` travelling up the Rust frames to [`State::resume`]; the
//! `CallInfo` chain of the coroutine is left untouched, so [`State::resume`] can later finish
//! every interrupted native frame (through its continuation) and Lua frame (`finish_op`).
use crate::instruction::Instruction;
use crate::state::*;
use crate::status::{LuaResult, Status};
use crate::value::*;
use crate::ver::active::OpCode;

/// BUNDLE-T: the fused resume path ([`State::resume_fused`]). Off with the cargo feature `coro_nofuse` (A/B builds).
pub const CORO_FUSED: bool = !cfg!(feature = "coro_nofuse");

/// Test-only mutants of the fused resume (feature `jit_trace_mut`, set by `rn_jit::tr::mutants::apply_vm_mutants`): a bit mask,
/// `1` `cofuse_room_skip`, `2` `cofuse_hook_ignored`, `4` `cofuse_ncalls_skip`, `8` `cofuse_finish_op_skip`, `16` `cofuse_wanted_nil_skip`,
/// `32` `cofuse_results_room_skip`, `64` `cofuse_prev_c_ignored`.
#[cfg(feature = "jit_trace_mut")]
static MUT_COFUSE: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

/// Select the fused-resume mutants (feature `jit_trace_mut`); see [`MUT_COFUSE`] for the bits.
#[cfg(feature = "jit_trace_mut")]
pub fn set_mut_cofuse(mask: u32) {
    MUT_COFUSE.store(mask, std::sync::atomic::Ordering::Relaxed);
}

#[inline(always)]
fn mut_cofuse(bit: u32) -> bool {
    #[cfg(feature = "jit_trace_mut")]
    return MUT_COFUSE.load(std::sync::atomic::Ordering::Relaxed) & bit != 0;
    #[cfg(not(feature = "jit_trace_mut"))]
    {
        let _ = bit;
        false
    }
}

/// Result of [`State::resume_fused`].
pub enum ResumeOut {
    /// Not applicable; nothing was modified.
    Slow,
    /// Resumed; `n` results (or yielded values) were moved to the resumer's stack.
    Ok(usize),
    /// Resumed, but the resumer has no room for the results.
    TooMany,
    /// The coroutine raised an error; the error object was moved to the resumer's stack.
    Err,
}

fn is_error(s: Status) -> bool {
    !matches!(s, Status::Ok | Status::Yield)
}

impl State {
    /// `lua_yieldk`: yield `nresults` values from the top of the stack. A native function must
    /// `return st.yield_k(..)`. From a hook (`Ok(0)`) the caller (`hook`) notices the status.
    pub fn yield_k(&mut self, nresults: usize, ctx: isize, k: Option<KFn>) -> LuaResult<usize> {
        let ci = self.ci();
        if !self.yieldable() {
            return Err(if self.is_main_thread() {
                self.run_error(b"attempt to yield from outside a coroutine")
            } else {
                self.run_error(b"attempt to yield across a C-call boundary")
            });
        }
        self.tm().status = Status::Yield;
        ci.w().u2 = nresults as i32; // save number of results
        if ci.r().is_lua() {
            // inside a hook
            debug_assert!(nresults == 0 && k.is_none());
            return Ok(0);
        }
        ci.w().k = k;
        if k.is_some() {
            ci.w().ctx = ctx;
        }
        Err(Status::Yield)
    }

    /// `finishpcallk`: finish the job of `pcall_k` after it was interrupted by a yield.
    fn finish_pcallk(&mut self, ci: Ci) -> LuaResult<Status> {
        let mut status = Self::status_from_code(ci.r().recst());
        if status == Status::Ok {
            status = Status::Yield; // was interrupted by a yield
        } else {
            // error
            let func = StkId(ci.r().u2 as usize);
            self.set_allow_hook(ci.r().callstatus & CIST_OAH != 0);
            let func = self.close(func, status as u8, true)?; // can yield or raise an error
            self.set_error_obj(status, func);
            self.shrink_stack(); // restore stack size in case of overflow
            ci.w().set_recst(0); // clear original status
        }
        ci.w().callstatus &= !CIST_YPCALL;
        let oe = ci.r().old_errfunc;
        self.set_errfunc(oe);
        // if it is here, there were errors or yields; unlike `pcall_k`, do not change status
        Ok(status)
    }

    /// `finishCcall`: complete the execution of a native function interrupted by a yield.
    fn finish_ccall(&mut self, ci: Ci) -> LuaResult<()> {
        let n = if ci.r().callstatus & CIST_CLSRET != 0 {
            // was closing a tbc variable: just redo `post_call`
            ci.r().u2 as usize
        } else {
            let mut status = Status::Yield; // default if there were no errors
            let Some(kf) = ci.r().k else {
                return Err(self.run_error(b"attempt to yield across a C-call boundary"));
            };
            if ci.r().callstatus & CIST_YPCALL != 0 {
                status = self.finish_pcallk(ci)?; // was inside a `pcall_k`: finish it
            }
            self.adjust_results(LUA_MULTRET); // finish `call_k`
            kf(self, status, ci.r().ctx)? // call continuation
        };
        self.post_call(ci, n) // finish `call`
    }

    /// `unroll`: execute the "full continuation" (everything on the frame list) of an interrupted
    /// coroutine until the list is empty (or another interruption leaves through `Err`).
    fn unroll(&mut self) -> LuaResult<()> {
        let base = self.base_ci();
        loop {
            let ci = self.ci();
            if ci.as_ptr() == base.as_ptr() {
                return Ok(());
            }
            if !ci.r().is_lua() {
                self.finish_ccall(ci)?; // complete its execution
            } else {
                crate::vm_unsafe::finish_op(self)?; // finish interrupted instruction
                let ex = self.g().executor;
                ex(self, ci)?; // execute down to higher C boundary
            }
        }
    }

    /// `findpcall`: a suspended protected call (a "recover point") of this thread.
    fn find_pcall(&self) -> Option<Ci> {
        let mut ci = Some(self.ci());
        while let Some(c) = ci {
            if c.r().callstatus & CIST_YPCALL != 0 {
                return Some(c);
            }
            ci = c.previous();
        }
        None
    }

    /// `resume_error`: an error in the call to `resume`, not in the coroutine itself.
    fn resume_error(&mut self, msg: &[u8], narg: usize) -> (Status, usize) {
        let t = self.top() - narg;
        self.set_top(t); // remove args from the stack
        match self.new_str_value(msg) {
            Ok(v) => self.push(v),
            Err(_) => {
                let m = Value::string(self.g().memerrmsg);
                self.push(m);
            }
        }
        (Status::ErrRun, 1)
    }

    /// The protected part of `lua_resume`.
    fn resume_body(&mut self, mut n: usize) -> LuaResult<()> {
        let first_arg = self.top() - n;
        let ci = self.ci();
        if self.status() == Status::Ok {
            // starting a coroutine: just call its body
            self.ccall(first_arg - 1, LUA_MULTRET, 0)
        } else {
            debug_assert!(self.status() == Status::Yield);
            self.tm().status = Status::Ok; // mark that it is running (again)
            if ci.r().is_lua() {
                // yielded inside a hook: the instruction was not executed yet
                // SAFETY: undo the increment made by `trace_exec`.
                ci.w().savedpc = unsafe { ci.r().savedpc.sub(1) };
                self.set_top(first_arg); // discard arguments
                let ex = self.g().executor;
                ex(self, ci)?; // just continue running Lua code
            } else {
                // 'common' yield
                if let Some(k) = ci.r().k {
                    n = k(self, Status::Yield, ci.r().ctx)?; // call continuation
                }
                self.post_call(ci, n)?; // finish `call`
            }
            self.unroll() // run continuation
        }
    }

    /// `precover`: unroll while there are recoverable errors (inside a protected call).
    fn precover(&mut self, mut status: Status) -> Status {
        while is_error(status) {
            let Some(ci) = self.find_pcall() else { break };
            self.set_ci(ci); // go down to the recovery functions
            ci.w().set_recst(status as u8); // status to finish `pcall`
            status = self.raw_run_protected(|s| s.unroll());
        }
        status
    }

    /// `lua_resume`: start or continue the coroutine `self` with `nargs` arguments on its stack
    /// (above the function when starting). Returns the status (`Ok` finished, `Yield`, or an error
    /// with the error object on top) and the number of values to collect from the top of its
    /// stack.
    pub fn resume(&mut self, from: Option<&State>, nargs: usize) -> (Status, usize) {
        if self.status() == Status::Ok {
            // may be starting a coroutine
            let ci = self.ci();
            if ci.as_ptr() != self.base_ci().as_ptr() {
                return self.resume_error(b"cannot resume non-suspended coroutine", nargs);
            } else if self.top() - (ci.r().func + 1) == nargs {
                return self.resume_error(b"cannot resume dead coroutine", nargs);
            }
        } else if self.status() != Status::Yield {
            return self.resume_error(b"cannot resume dead coroutine", nargs);
        }
        let nc = from.map_or(0, |f| get_ccalls(f.ncalls_raw()));
        self.set_ncalls_raw(nc);
        if get_ccalls(self.ncalls_raw()) >= LUAI_MAXCCALLS {
            return self.resume_error(b"C stack overflow", nargs);
        }
        self.tm().ncalls += 1;
        let status = self.raw_run_protected(|s| s.resume_body(nargs));
        self.resume_finish(status)
    }

    /// The part of `lua_resume` after the protected run: recoverable errors, the dead-thread mark, the result count.
    #[inline]
    fn resume_finish(&mut self, mut status: Status) -> (Status, usize) {
        // continue running after recoverable errors
        status = self.precover(status);
        if is_error(status) {
            // unrecoverable error: mark thread as 'dead'
            self.tm().status = status;
            let t = self.top();
            self.set_error_obj(status, t); // push error message
            let t = self.top();
            self.ci().w().top = t;
        } else {
            debug_assert!(status == self.status());
        }
        let ci = self.ci();
        let nres = if status == Status::Yield {
            ci.r().u2 as usize
        } else {
            self.top() - (ci.r().func + 1)
        };
        (status, nres)
    }

    /// BUNDLE-T (`coro_fused`): the whole of `auxresume` (argument move, `lua_resume`, result move) for the common case: `self`
    /// is suspended in a plain `yield` (a native frame without continuation) called by a Lua frame, nobody has a hook, there
    /// is room for the arguments and the C stack limit is not near. Nothing is modified unless the case applies
    /// ([`ResumeOut::Slow`]: use the general `auxresume`). Same results as the general path: `Ok(n)` = `n` values moved to
    /// `from`'s top (success or yield), `Err` = the error object moved to `from`'s top, `TooMany` = not enough room in
    /// `from` for the results (they are dropped; the caller pushes the message).
    pub fn resume_fused(&mut self, from: &mut State, nargs: usize) -> ResumeOut {
        if !CORO_FUSED {
            return ResumeOut::Slow;
        }
        let yci = self.ci();
        let nc = get_ccalls(from.ncalls_raw());
        let top = self.top();
        if self.status() != Status::Yield
            || (self.hook_mask() != 0 && !mut_cofuse(2))
            || yci.r().callstatus & CIST_C == 0
            || yci.r().k.is_some()
            || nc >= LUAI_MAXCCALLS
            || ((self.stack_size() as isize) - (top.0 as isize) <= nargs as isize && !mut_cofuse(1))
        {
            return ResumeOut::Slow;
        }
        let Some(prev) = yci.previous() else {
            return ResumeOut::Slow;
        };
        if !prev.r().is_lua() && !mut_cofuse(64) {
            return ResumeOut::Slow;
        }
        // ---- committed: `check_stack_api(nargs)` + `xmove(from -> self, nargs)`
        let ftop = from.top();
        let first = ftop - nargs;
        // (mutant `cofuse_room_skip`: no room check, so the values that do not fit are silently dropped)
        let room = (self.stack_size() as isize) - (top.0 as isize) - 1;
        let nargs = if mut_cofuse(1) && (nargs as isize) > room {
            room.max(0) as usize
        } else {
            nargs
        };
        // SAFETY: both ranges are inside the stack blocks (`nargs` values below `from`'s top; room checked above).
        unsafe {
            let src = from.slot_ptr(first);
            let dst = self.slot_ptr(top);
            for i in 0..nargs {
                Value::store(dst.add(i), Value::load(src.add(i)));
            }
        }
        from.set_top(first);
        self.set_top(top + nargs);
        if yci.r().top < top + nargs {
            yci.w().top = top + nargs;
        }
        if !mut_cofuse(4) {
            self.set_ncalls_raw(nc + 1);
        }
        let status = self.raw_run_protected(|s| {
            s.tm().status = Status::Ok; // mark that it is running (again)
                                        // `post_call(yci, nargs)` (no hook, a plain native frame): the common wanted-result counts inline
            let fw = yci.r().callstatus & (CIST_TBC | CIST_NRESULTS);
            if fw == 1 {
                s.set_top(yci.r().func); // no value wanted
                s.set_ci(prev);
            } else if fw == 2 {
                let v = if nargs == 0 {
                    Value::NIL
                } else {
                    s.get(s.top() - nargs)
                };
                let f = yci.r().func;
                if !(nargs == 0 && mut_cofuse(16)) {
                    s.set(f, v);
                }
                s.set_top(f + 1);
                s.set_ci(prev);
            } else {
                s.post_call(yci, nargs)?; // finish the `yield` call
            }
            // `finish_op` of an `OP_CALL` is empty
            // SAFETY: a Lua frame interrupted by a call has `savedpc - 1` inside its code.
            let inst = Instruction(unsafe { *prev.r().savedpc.sub(1) });
            if inst.opcode() != Some(OpCode::Call) && !mut_cofuse(8) {
                crate::vm_unsafe::finish_op(s)?;
            }
            let ex = s.g().executor;
            ex(s, prev)?; // execute down to the higher C boundary
            s.unroll() // run the rest of the continuation
        });
        let (status, nres) = self.resume_finish(status);
        if status == Status::Ok || status == Status::Yield {
            if (from.stack_size() as isize) - (from.top().0 as isize) <= (nres + 1) as isize
                && !mut_cofuse(32)
                && !from.check_stack_api(nres + 1)
            {
                let t = self.top() - nres;
                self.set_top(t); // remove results anyway
                return ResumeOut::TooMany;
            }
            let t = self.top();
            let first = t - nres;
            let ftop = from.top();
            // (mutant `cofuse_results_room_skip`: no room check, so the results that do not fit are silently dropped)
            let room = (from.stack_size() as isize) - (ftop.0 as isize) - 1;
            let ncopy = if mut_cofuse(32) && (nres as isize) > room {
                room.max(0) as usize
            } else {
                nres
            };
            let fci = from.ci(); // (`check_stack_api` raises the frame's top too)
            if fci.r().top < ftop + nres + 1 {
                fci.w().top = ftop + nres + 1;
            }
            // SAFETY: `nres` values below `self`'s top; room in `from` checked above.
            unsafe {
                let src = self.slot_ptr(first);
                let dst = from.slot_ptr(ftop);
                for i in 0..ncopy {
                    Value::store(dst.add(i), Value::load(src.add(i)));
                }
            }
            self.set_top(first);
            from.set_top(ftop + ncopy);
            ResumeOut::Ok(ncopy)
        } else {
            self.xmove(from, 1); // move error message
            ResumeOut::Err
        }
    }

    /// `lua_closethread`: reset the thread, closing its pending to-be-closed variables. Returns
    /// the final status (with the error object at stack index 1 on error).
    pub fn close_thread(&mut self, from: Option<&State>) -> Status {
        let nc = from.map_or(0, |f| get_ccalls(f.ncalls_raw()));
        self.set_ncalls_raw(nc);
        let st = self.status();
        self.reset_thread(st)
    }

    /// `lua_xmove`: move the top `n` values of this stack to the top of `to`'s stack.
    pub fn xmove(&mut self, to: &mut State, n: usize) {
        if self.th_ptr() == to.th_ptr() {
            return;
        }
        let first = self.top() - n;
        for i in 0..n {
            let v = self.get(first + i);
            to.push(v);
        }
        self.set_top(first);
    }
}

impl State {
    /// `lua_checkstack`: ensure `n` free slots, `false` (no error raised) if impossible.
    pub fn check_stack_api(&mut self, n: usize) -> bool {
        let ok = if (self.stack_size() as isize) - (self.top().0 as isize) > n as isize {
            true
        } else {
            matches!(self.grow_stack(n, false), Ok(true))
        };
        if ok {
            let need = self.top() + n;
            let ci = self.ci();
            if ci.r().top < need {
                ci.w().top = need;
            }
        }
        ok
    }

    /// Are `self` and `other` the same thread?
    pub fn same_thread(&self, other: &State) -> bool {
        self.th_ptr() == other.th_ptr()
    }

    /// `lua_gettop` of this thread's base frame or current frame: number of values above the
    /// current function.
    pub fn frame_top(&self) -> usize {
        self.top() - (self.ci().r().func + 1)
    }
}
