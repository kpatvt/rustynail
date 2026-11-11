//! The interpreter entry point (`luaV_execute`) and the VM-side hook machinery
//! (`luaG_tracecall`, `luaG_traceexec`).
//!
//! The hot loop itself (raw `pc`/`base`/`k` pointers, opcode dispatch) lives in
//! `crate::vm_unsafe`; the out-of-line slow paths (`luaV_finishget`, concatenation, comparison
//! with metamethods, `for` preparation...) are safe code in [`crate::arith`], [`crate::meta`] and
//! [`crate::call`].
//!
//! **Frame protocol:** a Lua -> Lua call (`OP_CALL`, `OP_TAILCALL`) or return switches `ci` inside
//! the same invocation of [`execute`]; `CIST_FRESH` marks the frame at which `execute` returns.
//! The same interpreter serves Lua 5.4 (`--cfg rustynail_lua54`): the opcodes whose form differs
//! are `cfg` arms in `vm_unsafe`.
use crate::state::*;
use crate::status::LuaResult;

/// Run the Lua function whose (fresh) frame is `ci` until it returns.
pub fn execute(st: &mut State, ci: Ci) -> LuaResult<()> {
    crate::vm_unsafe::run(st, ci)
}

mod hooks {
    use crate::debuginfo::changed_line;
    use crate::instruction::Instruction;
    use crate::state::*;
    use crate::status::{LuaResult, Status};
    #[cfg(not(rustynail_lua54))]
    use crate::ver::active::OpCode;
    use crate::ver::active::OP_PROPS;

    /// `luaP_isIT`: does the instruction use `top` as an implicit operand (so `top` must not be
    /// reset while tracing it)?
    pub(crate) fn is_it(i: Instruction) -> bool {
        match i.opcode() {
            #[cfg(not(rustynail_lua54))]
            Some(OpCode::Setlist) => i.vb() == 0,
            #[cfg(not(rustynail_lua54))]
            Some(OpCode::Varargprep) => true,
            Some(op) => OP_PROPS[op as usize].it && i.b() == 0,
            None => false,
        }
    }

    impl State {
        /// `luaG_tracecall`: called when a Lua function starts (or resumes) running with hooks on.
        /// Returns the new value of `trap`.
        pub fn trace_call(&mut self) -> LuaResult<bool> {
            let ci = self.ci();
            ci.w().trap = 1; // ensure hooks will be checked
                             // J5 recorder (TR23.2): no call hook, no `oldpc` reset and no vararg shortcut while recording
            #[cfg(feature = "jit_trace")]
            if self.hook_mask() == 0 && crate::jit_hooks::rec_active_for(self) {
                return Ok(true);
            }
            let Some(cl) = self.ci_lclosure(ci) else {
                return Ok(true);
            };
            let p = cl.proto();
            if ci.r().savedpc == p.code_ptr() {
                // first instruction (not resuming)?
                if p.is_vararg() {
                    // hooks will start at the VARARGPREP instruction (which sets `trap` again)
                    ci.w().trap = 0;
                    return Ok(false);
                } else if ci.r().callstatus & CIST_HOOKYIELD == 0 {
                    self.hook_call(ci)?; // check 'call' hook
                }
            }
            Ok(true)
        }

        /// `luaG_traceexec`: called before each instruction when `trap` is set; `pc` points to the
        /// instruction about to run. Returns the new value of `trap`.
        pub(crate) fn trace_exec(&mut self, pc: *const u32) -> LuaResult<bool> {
            let ci = self.ci();
            if crate::state::INTERRUPT.swap(false, std::sync::atomic::Ordering::Relaxed) {
                // SAFETY: `pc` points to the instruction about to run (for the traceback).
                ci.w().savedpc = unsafe { pc.add(1) };
                return Err(self.l_error(b"interrupted!")); // like `lstop` in lua.c
            }
            let mask = self.hook_mask();
            let Some(cl) = self.ci_lclosure(ci) else {
                return Ok(false);
            };
            let p = cl.proto();
            if mask & (LUA_MASKLINE | LUA_MASKCOUNT) == 0 {
                // no hooks?
                // J5 recorder (TR23.3): keep `trap` set while recording
                #[cfg(feature = "jit_trace")]
                if crate::jit_hooks::rec_active_for(self) {
                    return Ok(true);
                }
                ci.w().trap = 0; // don't need to stop again
                return Ok(false); // turn off 'trap'
            }
            // SAFETY: `pc` points to an instruction of this function; the reference always uses
            // the next one.
            let pc = unsafe { pc.add(1) };
            ci.w().savedpc = pc; // save 'pc'
            let counthook = mask & LUA_MASKCOUNT != 0 && {
                let t = self.tm();
                t.hookcount -= 1;
                t.hookcount == 0
            };
            if counthook {
                let t = self.tm();
                t.hookcount = t.basehookcount; // reset count
            } else if mask & LUA_MASKLINE == 0 {
                return Ok(true); // no line hook and count != 0; nothing to be done now
            }
            if ci.r().callstatus & CIST_HOOKYIELD != 0 {
                // hook yielded last time?
                ci.w().callstatus &= !CIST_HOOKYIELD; // erase mark
                return Ok(true); // do not call hook again
            }
            // SAFETY: `savedpc - 1` is the instruction about to run.
            let cur = Instruction(unsafe { *ci.r().savedpc.sub(1) });
            if !is_it(cur) {
                // top not being used?
                let t = ci.r().top;
                self.set_top(t); // correct top
            }
            if counthook {
                self.hook(LUA_HOOKCOUNT, -1, 0, 0)?; // call count hook
            }
            if mask & LUA_MASKLINE != 0 {
                // 'oldpc' may be invalid; use zero in this case
                let sizecode = p.code().len() as i32;
                let oldpc = {
                    let o = self.t().oldpc;
                    if o < sizecode {
                        o
                    } else {
                        0
                    }
                };
                // SAFETY: both pointers are inside the same code array.
                let npci = unsafe { pc.offset_from(p.code_ptr()) } as i32 - 1;
                if npci <= oldpc || changed_line(&*p, oldpc.max(0) as usize, npci.max(0) as usize) {
                    // jump back (loop), or entered a new line
                    let newline = p.line_at(npci);
                    self.hook(LUA_HOOKLINE, newline, 0, 0)?; // call line hook
                }
                self.tm().oldpc = npci; // 'pc' of last call to line hook
            }
            if self.status() == Status::Yield {
                // hook yielded?
                if counthook {
                    self.tm().hookcount = 1; // undo decrement to zero
                }
                // SAFETY: undo the increment (resume will increment it again).
                ci.w().savedpc = unsafe { ci.r().savedpc.sub(1) };
                ci.w().callstatus |= CIST_HOOKYIELD; // mark that it yielded
                return Err(Status::Yield);
            }
            Ok(true)
        }
    }
}
