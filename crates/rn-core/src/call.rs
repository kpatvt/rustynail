//! Calls, returns and protected calls (port of `ldo.c`, minus coroutine resume/yield which comes
//! with the coroutine task, and the stack half that lives in `state.rs`).
//!
//! * Lua -> Lua calls never recurse here: [`State::pre_call`] pushes a [`CallInfo`] and returns it,
//!   the interpreter (B4) switches to it; [`State::post_call`] pops it. Only native -> Lua and
//!   metamethod calls recurse through [`State::call`] (depth-limited by `LUAI_MAXCCALLS`).
//! * Errors are `Err(Status)`; [`State::pcall_with`] / [`State::raw_run_protected`] are the
//!   `luaD_pcall` / `luaD_rawrunprotected` equivalents and restore `ci`, `allowhook`, `nCcalls`,
//!   close pending to-be-closed variables and shrink the stack.
//! * Continuations: native frames carry `k`/`ctx`/`old_errfunc` ([`State::call_k`],
//!   [`State::pcall_k`] are `lua_callk`/`lua_pcallk`); the resume side (`finishCcall`, `unroll`)
//!   belongs to the coroutine task.
use crate::error::Opnd;
use crate::func::{status_from_u8, CLOSEKTOP};
use crate::meta::Tm;
use crate::state::*;
use crate::status::{LuaResult, Status};
use crate::value::*;

/// JIT hooks at call time (callee side), for the placements (d) and (e) and the per-CallInfo loop
/// counter (variant C); see `jit_hooks`. Expands to nothing without the `jit` feature or for the
/// `'frame`-head placement (c) (whose hook lives in the interpreter loop).
macro_rules! jit_call_hook {
    ($st:expr, $ci:expr, $p:expr) => {{
        #[cfg(feature = "jit_counter_c")]
        {
            $ci.w().hot = crate::jit_hooks::HOT_THRESHOLD;
        }
        #[cfg(all(feature = "jit_check_d", not(feature = "jit_no_entry")))]
        {
            // (d) count the entry; flag compiled callees so that `do_ret!` finds them on return
            #[cfg(not(feature = "jit_counter_b"))]
            {
                let h = $p.hot.get() - crate::jit_hooks::HOT_ENTRY_WEIGHT;
                $p.hot.set(h);
                if h <= 0 {
                    crate::jit_hooks::jit_hot($st, Some(&*$p));
                }
            }
            #[cfg(feature = "jit_counter_b")]
            if $st.jit_tick_dec() {
                crate::jit_hooks::jit_hot($st, None);
            }
            if !$p.jit.get().is_null() {
                $ci.w().callstatus |= CIST_JIT;
                crate::jit_hooks::jit_try_enter($st, $ci);
            }
        }
        #[cfg(all(feature = "jit_check_e", not(feature = "jit_no_entry")))]
        {
            // (e) one RMW + branch: counts down, and stays <= 0 for compiled protos
            let h = $p.hot.get() - crate::jit_hooks::HOT_ENTRY_WEIGHT;
            $p.hot.set(h);
            if h <= 0 {
                crate::jit_hooks::jit_hot_call($st, $ci, &*$p);
            }
        }
    }};
}

/// The function that runs light C functions (`LUA_VLCFC`): it reads the C pointer from the running
/// frame's function slot and calls it. Installed once by `rn-capi`.
static C_TRAMPOLINE: std::sync::OnceLock<NativeFn> = std::sync::OnceLock::new();

/// Install the foreign-call trampoline (done by the C API layer; idempotent).
pub fn set_c_trampoline(f: NativeFn) {
    let _ = C_TRAMPOLINE.set(f);
}

fn c_trampoline() -> NativeFn {
    C_TRAMPOLINE
        .get()
        .copied()
        .unwrap_or(no_c_trampoline as NativeFn)
}

fn no_c_trampoline(st: &mut State) -> LuaResult<usize> {
    Err(st.run_error(b"C functions are not supported (no C API installed)"))
}

impl State {
    // ---- hooks --------------------------------------------------------------------------------

    /// `luaD_hook`: call the debug hook for `event` (no-op without a hook or inside a hook).
    pub fn hook(&mut self, event: i32, line: i32, ftransfer: i32, ntransfer: i32) -> LuaResult<()> {
        let hook = match self.t().hook {
            Some(h) if self.t().allowhook => h,
            _ => return Ok(()),
        };
        let ci = self.ci();
        let top = self.top();
        let ci_top = ci.r().top;
        {
            let t = self.tm();
            t.ftransfer = ftransfer;
            t.ntransfer = ntransfer;
        }
        if ci.r().is_lua() && top < ci.r().top {
            self.set_top(ci.r().top); // protect entire activation register
        }
        self.check_stack(LUA_MINSTACK)?;
        let top2 = self.top();
        if ci.r().top < top2 + LUA_MINSTACK {
            ci.w().top = top2 + LUA_MINSTACK;
        }
        self.set_allow_hook(false);
        ci.w().callstatus |= CIST_HOOKED;
        // on error nothing is restored: the error object is on top of the stack and the protected
        // call that catches the error restores `allowhook`, the frame and `top` (as `luaD_hook`)
        hook(self, event, line)?;
        self.set_allow_hook(true);
        ci.w().top = ci_top;
        self.set_top(top);
        ci.w().callstatus &= !CIST_HOOKED;
        Ok(())
    }

    /// `luaD_hookcall`: call hook for a Lua function entry.
    pub fn hook_call(&mut self, ci: Ci) -> LuaResult<()> {
        self.tm().oldpc = 0;
        if self.hook_mask() & LUA_MASKCALL != 0 {
            let event = if ci.r().callstatus & CIST_TAIL != 0 {
                LUA_HOOKTAILCALL
            } else {
                LUA_HOOKCALL
            };
            let np = self
                .ci_lclosure(ci)
                .map_or(0, |c| c.proto().numparams as i32);
            // SAFETY: hooks assume 'pc' is already incremented.
            ci.w().savedpc = unsafe { ci.r().savedpc.add(1) };
            let r = self.hook(event, -1, 1, np);
            // SAFETY: undo the increment above.
            ci.w().savedpc = unsafe { ci.r().savedpc.sub(1) };
            r?;
        }
        Ok(())
    }

    /// `rethook`: return hook; also sets `oldpc` (needed by the line hook).
    fn rethook(&mut self, ci: Ci, nres: usize) -> LuaResult<()> {
        if self.hook_mask() & LUA_MASKRET != 0 {
            let firstres = self.top() - nres;
            let mut delta = 0usize;
            if let Some(cl) = self.ci_lclosure(ci) {
                let p = cl.proto();
                if p.flag & crate::protodata::PF_VAHID != 0 {
                    delta = ci.r().nextraargs as usize + p.numparams as usize + 1;
                }
            }
            ci.w().func += delta; // if vararg, back to virtual 'func'
            let ftransfer = (firstres - ci.r().func) as i32;
            let r = self.hook(LUA_HOOKRET, -1, ftransfer, nres as i32);
            ci.w().func -= delta;
            r?;
        }
        if let Some(prev) = ci.previous() {
            if prev.r().is_lua() {
                let pc = self.current_pc(prev);
                self.tm().oldpc = pc;
            }
        }
        Ok(())
    }

    // ---- __call -------------------------------------------------------------------------------

    /// `tryfuncTM`: replace the non-function at `func` by its `__call` metamethod (shifting the
    /// arguments up). `status` counts the `__call` chain length in `CIST_CCMT`.
    fn try_func_tm(&mut self, func: StkId, status: u32) -> LuaResult<u32> {
        // 5.4: `checkstackGCp(L, 1, func)` (no limit on the chain length)
        #[cfg(rustynail_lua54)]
        {
            if (self.stack_size() as isize) - (self.top().0 as isize) <= 1 {
                self.check_gc(); // stack grow uses memory
                self.check_stack(1)?;
            }
        }
        let fv = self.get(func);
        let tm = self.get_tm_by_obj(&fv, Tm::Call);
        if tm.is_nil() {
            return Err(self.call_error(Opnd::from(fv)));
        }
        let top = self.top();
        let mut p = top;
        while p > func {
            let v = self.get(p - 1);
            self.set(p, v);
            p -= 1;
        }
        self.set_top(top + 1); // stack space pre-allocated by the caller
        self.set(func, tm);
        #[cfg(rustynail_lua54)]
        {
            Ok(status)
        }
        #[cfg(not(rustynail_lua54))]
        {
            if status & MAX_CCMT == MAX_CCMT {
                return Err(self.run_error(b"'__call' chain too long"));
            }
            Ok(status + (1 << CIST_CCMT))
        }
    }

    // ---- results ------------------------------------------------------------------------------

    /// `genmoveresults`.
    fn gen_move_results(&mut self, res: StkId, nres: usize, wanted: usize) {
        let first = self.top() - nres;
        let n = nres.min(wanted);
        for i in 0..n {
            let v = self.get(first + i);
            self.set(res + i, v);
        }
        for i in n..wanted {
            self.set(res + i, Value::NIL);
        }
        self.set_top(res + wanted);
    }

    /// `moveresults`: `fwanted` = `callstatus & (CIST_TBC | CIST_NRESULTS)`.
    fn move_results(&mut self, mut res: StkId, nres: usize, fwanted: u32) -> LuaResult<()> {
        match fwanted {
            1 => {
                // no values needed
                self.set_top(res);
            }
            2 => {
                // one value needed
                if nres == 0 {
                    self.set(res, Value::NIL);
                } else {
                    let v = self.get(self.top() - nres);
                    self.set(res, v);
                }
                self.set_top(res + 1);
            }
            0 => self.gen_move_results(res, nres, nres), // LUA_MULTRET + 1: all results
            _ => {
                let mut wanted = (fwanted & CIST_NRESULTS) as i32 - 1;
                if fwanted & CIST_TBC != 0 {
                    // to-be-closed variables
                    let ci = self.ci();
                    ci.w().u2 = nres as i32;
                    ci.w().callstatus |= CIST_CLSRET; // in case of yields
                    res = self.close(res, CLOSEKTOP, true)?;
                    ci.w().callstatus &= !CIST_CLSRET;
                    if self.hook_mask() != 0 {
                        self.rethook(ci, nres)?;
                    }
                    if wanted == LUA_MULTRET {
                        wanted = nres as i32;
                    }
                }
                self.gen_move_results(res, nres, wanted as usize);
            }
        }
        Ok(())
    }

    /// `luaD_poscall`: finish a call: hook, move `nres` results (on top) to the function slot, pop
    /// the frame.
    pub fn post_call(&mut self, ci: Ci, nres: usize) -> LuaResult<()> {
        let fwanted = ci.r().callstatus & (CIST_TBC | CIST_NRESULTS);
        if self.hook_mask() != 0 && fwanted & CIST_TBC == 0 {
            self.rethook(ci, nres)?;
        }
        self.move_results(ci.r().func, nres, fwanted)?;
        debug_assert!(
            ci.r().callstatus & (CIST_HOOKED | CIST_YPCALL | CIST_FIN | CIST_CLSRET) == 0
        );
        // back to caller (after closing variables)
        self.set_ci(ci.previous().expect("returning from the base frame"));
        Ok(())
    }

    /// `adjustresults`.
    pub fn adjust_results(&mut self, nres: i32) {
        let ci = self.ci();
        if nres <= LUA_MULTRET && ci.r().top < self.top() {
            ci.w().top = self.top();
        }
    }

    // ---- precall ------------------------------------------------------------------------------

    /// `checkstackp` with a signed count (negative = nothing to check).
    #[inline]
    fn check_stack_signed(&mut self, n: isize) -> LuaResult<()> {
        if n > 0 {
            self.check_stack(n as usize)
        } else {
            Ok(())
        }
    }

    /// `prepCallInfo`.
    #[inline]
    fn prep_call_info(&mut self, func: StkId, status: u32, top: StkId) -> LuaResult<Ci> {
        let ci = self.next_ci()?;
        self.set_ci(ci);
        let c = ci.w();
        c.func = func;
        c.callstatus = status;
        c.top = top;
        Ok(ci)
    }

    /// `precallC`: run a native function (frame, hook, call, `post_call`). Returns its result count.
    fn precall_c(&mut self, func: StkId, status: u32, f: NativeFn) -> LuaResult<usize> {
        self.check_stack(LUA_MINSTACK)?;
        let top = self.top();
        let ci = self.prep_call_info(func, status | CIST_C, top + LUA_MINSTACK)?;
        ci.w().k = None;
        debug_assert!(ci.r().top.0 <= self.stack_size());
        if self.hook_mask() & LUA_MASKCALL != 0 {
            let narg = (self.top() - func) as i32 - 1;
            self.hook(LUA_HOOKCALL, -1, 1, narg)?;
        }
        let n = f(self)?;
        debug_assert!(n <= self.top().0);
        self.post_call(ci, n)?;
        Ok(n)
    }

    /// Inline fast path of [`State::pre_call`] for a native function (light native or native
    /// closure) when no hook is set, the stack has `LUA_MINSTACK` free slots and a spare
    /// `CallInfo` exists: runs the function and moves its results like `post_call` (inline for
    /// 0 or 1 wanted results). `Ok(false)` means "use the general path" (nothing was modified).
    #[inline(always)]
    pub(crate) fn pre_call_native_fast(&mut self, func: StkId, nresults: i32) -> LuaResult<bool> {
        let fv = self.get(func);
        let f = if let Some(f) = fv.as_native() {
            f
        } else if let Some(cl) = fv.as_cclosure() {
            cl.f
        } else {
            return Ok(false);
        };
        self.native_fast_with(func, f, nresults)
    }

    /// The body of [`State::pre_call_native_fast`] for a known native function `f` (BUNDLE-C `t1ccall`: the lean tier-1 helper
    /// `jit_ccall` calls it with a constant `nresults`, so the result move below specialises).
    #[inline(always)]
    pub(crate) fn native_fast_with(&mut self, func: StkId, f: NativeFn, nresults: i32) -> LuaResult<bool> {
        let top = self.top();
        let caller = self.ci();
        let next = caller.r().next;
        if (self.stack_size() as isize) - (top.0 as isize) <= LUA_MINSTACK as isize
            || next.is_null()
            || self.hook_mask() != 0
        {
            return Ok(false);
        }
        // SAFETY: list invariant (a non-null `next` is a valid spare node).
        let ci = unsafe { Ci::from_raw(next) };
        self.set_ci(ci);
        {
            let c = ci.w();
            c.func = func;
            c.callstatus = (nresults + 1) as u32 | CIST_C;
            c.top = top + LUA_MINSTACK;
            c.k = None;
        }
        let n = f(self)?;
        debug_assert!(n <= self.top().0);
        // `post_call` (the function may have set hooks or marked to-be-closed slots)
        // (`fwanted` = wanted results + 1, or a value with `CIST_TBC` set)
        let fwanted = ci.r().callstatus & (CIST_TBC | CIST_NRESULTS);
        if self.hook_mask() != 0 || fwanted == 0 || fwanted > 4 {
            self.post_call(ci, n)?;
            return Ok(true);
        }
        if fwanted == 2 {
            let v = if n == 0 {
                Value::NIL
            } else {
                self.get(self.top() - n)
            };
            self.set(func, v);
            self.set_top(func + 1);
        } else {
            // `moveresults` for 0, 2 or 3 wanted results
            let wanted = fwanted as usize - 1;
            let first = self.top() - n;
            for i in 0..wanted {
                let v = if i < n {
                    self.get(first + i)
                } else {
                    Value::NIL
                };
                self.set(func + i, v);
            }
            self.set_top(func + wanted);
        }
        self.set_ci(caller);
        Ok(true)
    }

    /// `luaD_pretailcall`: prepare a tail call reusing the frame `ci`. `narg1` = arguments + 1.
    /// Returns the number of results if the callee was native (already finished), or -1 for a Lua
    /// function (frame rewritten in place; the interpreter restarts it).
    pub fn pre_tail_call(
        &mut self,
        ci: Ci,
        func: StkId,
        mut narg1: usize,
        delta: usize,
    ) -> LuaResult<i32> {
        let mut status = (LUA_MULTRET + 1) as u32;
        loop {
            let fv = self.get(func);
            if let Some(cl) = fv.as_lclosure() {
                // (Lua callees first: the common case)
                let p = cl.proto();
                let fsize = p.maxstacksize as usize;
                let nfixparams = p.numparams as usize;
                self.check_stack_signed(fsize as isize - delta as isize)?;
                let nfunc = ci.r().func - delta; // restore 'func' (if vararg)
                debug_assert!(nfunc < func && func.0 + narg1 <= self.top().0);
                // SAFETY: `func..func + narg1` (function and arguments) and the frame from `nfunc`
                // (`fsize + 1` slots, checked just above) are inside the stack block; `nfunc <
                // func`, so a forward copy moves the values down correctly.
                unsafe {
                    let src = self.slot_ptr(func);
                    let dst = self.slot_ptr(nfunc);
                    for i in 0..narg1 {
                        Value::store(dst.add(i), Value::load(src.add(i)));
                    }
                    while narg1 <= nfixparams {
                        Value::store(dst.add(narg1), Value::NIL); // complete missing arguments
                        narg1 += 1;
                    }
                }
                let c = ci.w();
                c.func = nfunc;
                c.top = nfunc + 1 + fsize; // top for new function
                c.savedpc = p.code_ptr();
                c.kcache = p.k_ptr();
                c.callstatus |= CIST_TAIL;
                debug_assert!(ci.r().top.0 <= self.stack_size());
                self.set_top(nfunc + narg1);
                // BUNDLE-1 (2): a callee reached only by tail calls counts and compiles like a called one (`CIST_TAIL` tells the
                // compiler's hook, which ignores it unless `tailhook` is on)
                jit_call_hook!(self, ci, p);
                return Ok(-1);
            } else if let Some(cl) = fv.as_cclosure() {
                return Ok(self.precall_c(func, status, cl.f)? as i32);
            } else if let Some(f) = fv.as_native() {
                return Ok(self.precall_c(func, status, f)? as i32);
            } else if fv.as_light_cfn().is_some() {
                return Ok(self.precall_c(func, status, c_trampoline())? as i32);
            } else {
                // not a function
                self.check_stack(1)?; // space for metamethod
                status = self.try_func_tm(func, status)?;
                narg1 += 1;
            }
        }
    }

    /// Fast path of [`State::pre_call`] for a Lua closure with enough stack and a spare
    /// `CallInfo` node; `None` means "use the general path" (nothing has been modified).
    #[inline(always)]
    pub(crate) fn pre_call_lua_fast(&mut self, func: StkId, nresults: i32) -> Option<Ci> {
        let cl = self.get(func).as_lclosure()?;
        let p = cl.proto();
        let fsize = p.maxstacksize as usize;
        let nfix = p.numparams as usize;
        let top = self.top();
        if (self.stack_size() as isize) - (top.0 as isize) <= fsize as isize {
            return None;
        }
        let next = self.ci().r().next;
        if next.is_null() {
            return None;
        }
        // SAFETY: list invariant.
        let ci = unsafe { Ci::from_raw(next) };
        self.set_ci(ci);
        let c = ci.w();
        c.func = func;
        c.callstatus = (nresults + 1) as u32;
        c.top = func + 1 + fsize;
        c.savedpc = p.code_ptr();
        c.kcache = p.k_ptr();
        jit_call_hook!(self, ci, p);
        let mut t = top;
        let want = func + 1 + nfix;
        while t.0 < want.0 {
            self.set(t, Value::NIL); // complete missing arguments
            t += 1;
        }
        self.set_top(t);
        Some(ci)
    }

    /// `luaD_precall`: prepare the call of the function at `func` (arguments up to `top`).
    /// For a native function, also performs the call and returns `None` (results at `func`).
    /// For a Lua function returns its new frame, to be executed by the interpreter.
    #[cold]
    pub fn pre_call(&mut self, func: StkId, nresults: i32) -> LuaResult<Option<Ci>> {
        let mut status = (nresults + 1) as u32;
        debug_assert!(status <= MAXRESULTS as u32 + 1);
        loop {
            let fv = self.get(func);
            if let Some(cl) = fv.as_lclosure() {
                let p = cl.proto();
                let mut narg = self.top() - func - 1;
                let nfixparams = p.numparams as usize;
                let fsize = p.maxstacksize as usize;
                self.check_stack(fsize)?;
                let ci = self.prep_call_info(func, status, func + 1 + fsize)?;
                ci.w().savedpc = p.code_ptr();
                ci.w().kcache = p.k_ptr();
                jit_call_hook!(self, ci, p);
                while narg < nfixparams {
                    let t = self.top();
                    self.set(t, Value::NIL); // complete missing arguments
                    self.set_top(t + 1);
                    narg += 1;
                }
                debug_assert!(ci.r().top.0 <= self.stack_size());
                return Ok(Some(ci));
            } else if let Some(cl) = fv.as_cclosure() {
                self.precall_c(func, status, cl.f)?;
                return Ok(None);
            } else if let Some(f) = fv.as_native() {
                self.precall_c(func, status, f)?;
                return Ok(None);
            } else if fv.as_light_cfn().is_some() {
                self.precall_c(func, status, c_trampoline())?;
                return Ok(None);
            } else {
                // not a function: stack indices are stable, so retry at the same slot
                self.check_stack(1)?; // space for metamethod
                status = self.try_func_tm(func, status)?;
            }
        }
    }

    // ---- calls --------------------------------------------------------------------------------

    /// `ccall`.
    #[inline]
    pub(crate) fn ccall(&mut self, func: StkId, nresults: i32, inc: u32) -> LuaResult<()> {
        self.tm().ncalls += inc;
        if self.ccalls() >= LUAI_MAXCCALLS {
            self.check_stack(0)?; // free any use of EXTRA_STACK
            self.check_cstack()?;
        }
        let fast = self.pre_call_lua_fast(func, nresults);
        let pc = match fast {
            Some(ci) => Some(ci),
            // native callee (TFORCALL iterators, metamethods): done inline
            None if self.pre_call_native_fast(func, nresults)? => None,
            None => self.pre_call(func, nresults)?,
        };
        if let Some(ci) = pc {
            // a Lua function: run it in a fresh interpreter frame
            ci.w().callstatus |= CIST_FRESH;
            let ex = self.g().executor;
            ex(self, ci)?;
        }
        self.tm().ncalls -= inc;
        Ok(())
    }

    /// `luaD_call`: call the function at `func` with the arguments above it; results are moved to
    /// `func` (adjusted to `nresults`, or all of them for `LUA_MULTRET`). Yieldable.
    pub fn call(&mut self, func: StkId, nresults: i32) -> LuaResult<()> {
        // T2: the generic `for` of the interpreter calls `next` / the `ipairs` iterator through here; they run
        // without a frame (only while no compiler is registered, see `jit_hooks::interp_builtins`). The
        // interpreter loop itself is untouched: any change of it shifts the code layout of every handler.
        if nresults >= 0 && crate::frameless::call_frameless_iter(self, func, nresults as usize) {
            return Ok(());
        }
        self.ccall(func, nresults, 1)
    }

    /// `luaD_callnoyield`.
    pub fn call_no_yield(&mut self, func: StkId, nresults: i32) -> LuaResult<()> {
        self.ccall(func, nresults, NYCI)
    }

    /// `lua_callk`: like [`State::call`] but with a continuation for yields across the call.
    pub fn call_k(
        &mut self,
        func: StkId,
        nresults: i32,
        ctx: isize,
        k: Option<KFn>,
    ) -> LuaResult<()> {
        if k.is_some() && self.yieldable() {
            let ci = self.ci();
            ci.w().k = k;
            ci.w().ctx = ctx;
            self.call(func, nresults)?;
        } else {
            self.call_no_yield(func, nresults)?;
        }
        self.adjust_results(nresults);
        Ok(())
    }

    // ---- protected calls ----------------------------------------------------------------------

    /// `luaD_rawrunprotected`: run `f`, catching its error status (`Status::Ok` on success).
    /// Restores `nCcalls`. Does not touch the stack or the frames: use [`State::pcall_with`] for that.
    pub fn raw_run_protected<F>(&mut self, f: F) -> Status
    where
        F: FnOnce(&mut State) -> LuaResult<()>,
    {
        let old = self.ncalls_raw();
        self.tm().prot_depth += 1;
        let r = f(self);
        self.tm().prot_depth -= 1;
        self.set_ncalls_raw(old);
        match r {
            Ok(()) => Status::Ok,
            Err(s) => {
                if self.t().throw_base && self.t().prot_depth == 0 {
                    self.tm().throw_base = false; // caught at the base level
                }
                s
            }
        }
    }

    /// `luaD_throwbaselevel`: raise `status` so that only the outermost protected call of this
    /// thread catches it. The caller returns the result as `Err(..)`.
    pub fn throw_base_level(&mut self, status: Status) -> Status {
        if self.t().prot_depth > 0 {
            self.tm().throw_base = true;
        }
        status
    }

    /// `luaD_closeprotected`: close variables down to `level` keeping going through errors in
    /// `__close` methods. Returns the final status.
    pub fn close_protected(&mut self, level: StkId, mut status: Status) -> Status {
        let old_ci = self.ci();
        let old_allow = self.allow_hook();
        loop {
            let st = status;
            let r = self.raw_run_protected(|s| s.close(level, st as u8, false).map(|_| ()));
            if r == Status::Ok {
                return status;
            }
            // an error occurred; restore saved state and repeat with the new status
            self.set_ci(old_ci);
            self.set_allow_hook(old_allow);
            status = r;
        }
    }

    /// `luaD_pcall`: run `f` protected. On error: restore frame/hook state, close pending
    /// variables, put the error object at `old_top` and shrink the stack. `ef` is the stack index
    /// of the message handler (0 = none). Returns the final status.
    pub fn pcall_with<F>(&mut self, f: F, old_top: StkId, ef: usize) -> Status
    where
        F: FnOnce(&mut State) -> LuaResult<()>,
    {
        let old_ci = self.ci();
        let old_allow = self.allow_hook();
        let old_errfunc = self.errfunc();
        self.set_errfunc(ef);
        let mut status = self.raw_run_protected(f);
        if self.t().throw_base {
            return status; // throw to the base level: not ours to catch
        }
        if status != Status::Ok {
            self.set_ci(old_ci);
            self.set_allow_hook(old_allow);
            status = self.close_protected(old_top, status);
            self.set_error_obj(status, old_top);
            self.shrink_stack(); // restore stack size in case of overflow
        }
        self.set_errfunc(old_errfunc);
        status
    }

    /// `lua_pcall` (no continuation): call the function at `func` in protected mode. On error the
    /// error object is left at `func`. `errfunc` is the stack index of a message handler or 0.
    pub fn pcall(&mut self, func: StkId, nresults: i32, errfunc: usize) -> Status {
        let status = self.pcall_with(|s| s.call_no_yield(func, nresults), func, errfunc);
        self.adjust_results(nresults);
        status
    }

    /// `lua_pcallk`. With a continuation (and a yieldable thread) the call is made in a way that a
    /// later `resume` can recover from errors (`CIST_YPCALL`); errors then propagate as `Err`
    /// from this function to the recovery machinery instead of being returned as a status.
    pub fn pcall_k(
        &mut self,
        func: StkId,
        nresults: i32,
        errfunc: usize,
        ctx: isize,
        k: Option<KFn>,
    ) -> LuaResult<Status> {
        let status = if k.is_none() || !self.yieldable() {
            let st = self.pcall_with(|s| s.call_no_yield(func, nresults), func, errfunc);
            if self.t().throw_base {
                return Err(st); // keep unwinding to the base level
            }
            st
        } else {
            let ci = self.ci();
            {
                let c = ci.w();
                c.k = k;
                c.ctx = ctx;
                c.u2 = func.0 as i32;
                c.old_errfunc = self.t().errfunc;
            }
            self.set_errfunc(errfunc);
            let oah = self.allow_hook();
            if oah {
                ci.w().callstatus |= CIST_OAH;
            } else {
                ci.w().callstatus &= !CIST_OAH;
            }
            ci.w().callstatus |= CIST_YPCALL;
            self.call(func, nresults)?;
            ci.w().callstatus &= !CIST_YPCALL;
            let oe = ci.r().old_errfunc;
            self.set_errfunc(oe);
            Status::Ok
        };
        self.adjust_results(nresults);
        Ok(status)
    }

    /// Convert a numeric status (as stored in frames) back to [`Status`].
    pub fn status_from_code(code: u8) -> Status {
        status_from_u8(code)
    }
}
