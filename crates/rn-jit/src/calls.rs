//! J1c: calls and returns (docs/JIT_DESIGN.md section 4, "As built: J1c").
//!
//! This file is a child module of `lower` (`#[path = "calls.rs"] mod calls;`), so it uses the lowering
//! context `Cx` directly. Templates:
//!
//! * `CALL`: hook / interrupt guards, block count, `top`, `savedpc = &code[pc + 1]`, then the **flat call**
//!   (E2): when the callee is a Lua closure whose proto is compiled, has enough arguments and stack and a
//!   spare `CallInfo`, the `CallInfo` is filled exactly like `pre_call_lua_fast` and control jumps to the
//!   callee's entry (`JitProto.entry0`) with `rbx/r13/r14` reloaded; no native recursion. Everything else
//!   (natives, uncompiled callees, `__call`, stack growth, missing arguments) goes through the `jit_call`
//!   helper, which is the interpreter's `OP_CALL` slow path; its result decides between "native done"
//!   (reload `rbx`, continue at `pc + 1`), "Lua frame pushed" (enter it from `thread.ci`) and "error".
//! * `RETURN0/1/RETURN`: move the results, pop the frame, then `ret_tail`: `CIST_FRESH` -> `Returned`;
//!   otherwise, if the caller's proto is compiled and has a resume entry for `caller.savedpc` (re-entry
//!   points are exactly the instructions after a `CALL`), jump to it with the caller's `rbx/r13/r14`;
//!   anything else exits `FrameChanged`.
//! * `TAILCALL`: the `jit_tailcall` helper (the interpreter's `OP_TAILCALL`), then like `CALL`.
//!
//! Valid re-entry points (the resume table, `Lowered::resume`): pc 0, back-edge targets of JMP/FORLOOP/
//! TFORLOOP, and the instruction after every CALL. Never "the instruction after any exit stub": the
//! instruction after an arithmetic op is its MMBIN, after NEWTABLE/SETLIST an EXTRAARG, after a compare a
//! JMP that the compare consumes; executing those standalone is wrong. [`valid_reentry`] states the rule and
//! `lower` asserts it in debug builds.

use super::{ciw, fr, thw, Cx};
use crate::ir::{Base, Cc, Op, Src, L, M, W};
use rn_core::instruction::Instruction;
use rn_core::jit_hooks::{thread as thr, JitExit};
use rn_core::jit_layout::{callinfo as ci, lclosure, proto};
use rn_core::value::{ctb, LUA_VLCF, LUA_VLCL};
use rn_core::ver::active::OpCode;

/// Header of the compiled-code descriptor behind `Proto.jit` (the first fields of `rt::JitProto`).
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct JitProtoHdr {
    /// Absolute address of the entry for pc 0.
    pub entry0: usize,
    /// `block.addr() - 1`: the resume table stores `native offset + 1` (0 = no entry).
    pub base_m1: usize,
    /// Resume table: one `u32` per instruction, indexed by the *byte* offset of the instruction.
    pub resume: *const u32,
}

/// Byte offset of `entry0` in the descriptor.
pub const HDR_ENTRY0: i32 = std::mem::offset_of!(JitProtoHdr, entry0) as i32;
/// Byte offset of `base_m1` in the descriptor.
pub const HDR_BASE_M1: i32 = std::mem::offset_of!(JitProtoHdr, base_m1) as i32;
/// Byte offset of `resume` in the descriptor.
/// BUNDLE-S S1: the flag (bit 31) of a resume-table entry that only `ret_mm` uses (the continuation after a table read / store whose Lua
/// metamethod frame tier 1 pushed); the interpreter's re-entry ignores such entries.
pub const RESUME_MM_ONLY: u32 = 0x8000_0000;
pub const HDR_RESUME: i32 = std::mem::offset_of!(JitProtoHdr, resume) as i32;

/// Layout and helper addresses that depend on the running process (resolved by `rt`).
#[derive(Clone, Copy, Debug)]
pub struct CallEnv {
    /// Offset of the stack data pointer inside `Thread`.
    pub stack_off: i32,
    /// Offset of the code data pointer inside `Proto`.
    pub proto_code: i32,
    /// Offset of the constants data pointer inside `Proto`.
    pub proto_k: i32,
    /// `jit_call` helper address.
    pub call_helper: u64,
    /// BUNDLE-C (`t1ccall`): `jit_ccall` helper address for callees that are light C functions (0 = every non-Lua callee goes through `call_helper`).
    pub ccall_helper: u64,
    /// `jit_tailcall` helper address.
    pub tail_helper: u64,
    /// R1: lower `TAILCALL` of a Lua closure with a fixed argument count inline (0 = always the helper, as in J4).
    pub tail_fast: bool,
    /// BUNDLE-D (`armm`): arithmetic instructions on operands that are not numbers push the Lua metamethod frame and continue in compiled code when it returns.
    pub armm: bool,
    /// R1: mutants of the inline tail call (`TM_*`; always 0 without the `tr_mutants` feature).
    pub tail_mut: u8,
    /// T3: address of the interned `"__index"` name string (0 = no inline `__index` level in the table templates).
    pub idx_name: u64,
    /// T3: its hash.
    pub idx_hash: u32,
    /// T3: number of `__index` levels the table templates probe inline (0 = none).
    pub idx_levels: u8,
    /// BUNDLE-K item B: address of `g.mt[LUA_TSTRING]` (the string metatable slot of this global state): tier-1 `SELF` on a string probes it inline (0 = the helper).
    pub str_mt: u64,
    /// BUNDLE-B item 2: bit 0 = `SETTABLE` with a short-string key stores inline into an existing node, bit 1 = integer key
    /// in the hash part, bit 2 = nil-slot stores (`tset` limit; 0 = as before).
    pub tset: u8,
    /// BUNDLE-T1I: bit mask of the built-ins the tier-1 `CALL` template runs inline (`t1inline::B_*`, limit `t1inline`; 0 = none).
    pub inl: u16,
    /// BUNDLE-T1I: test-only mutants of the inline built-ins (`t1inline::M_*`; always 0 without the `tr_mutants` feature).
    pub inl_mut: u16,
    /// BUNDLE-Q (`t1q`): bits 0-3 = the enabled pieces (1 = `EQ` of objects answered in tier 1, 2 = float `MOD` inline helper, 4 / 8 = BUNDLE-S Lua `__index` / `__newindex` frames), bits 8.. = the test-only mutants
    /// (`tr::t1q_mut`; always 0 without the `tr_mutants` feature).
    pub t1q: u32,
    /// BUNDLE-T1I: the per-prototype masks of the rows worth emitting at each `CALL` (`t1inline::site_hints`, keyed by the address of
    /// the prototype's code; lives in the state's `JitData`, which outlives every lowering); null or no entry = every enabled row.
    pub hints: *const super::t1inline::SiteHints,
}

/// R1: bits of [`CallEnv::tail_mut`], the test-only code mutations of the inline tail call (`tr::mutants`).
const TM_ARGS: u8 = 1;
const TM_COPY: u8 = 2;
const TM_STATUS: u8 = 4;
/// M1 mutant: the open-upvalue test of the `k` fast paths is skipped (closes nothing, wrong when an upvalue is open).
const TM_CLOSE: u8 = 8;

/// Shared stubs of one function, created on demand.
#[derive(Default)]
pub struct Shared {
    enter: Option<L>,
    exit_fc: Option<L>,
    exit_err: Option<L>,
    exit_ret: Option<L>,
    ret_tail: Option<L>,
    ret_mm: Option<L>,
}

const H_DONE: u32 = rn_core::jit_helpers::H_DONE;
const H_LUA: u32 = rn_core::jit_helpers::H_LUA;
const H_RETURNED: u32 = rn_core::jit_helpers::H_RETURNED;

/// May compiled code be (re)entered at instruction `pc` of `code`? (The rule `lower` records resume
/// entries by.) `pc` must be an instruction start in the sense of the interpreter's dispatch: not the
/// operand word of the previous instruction (`EXTRAARG`), not an `MMBIN*` that only runs when the
/// arithmetic fast path of its predecessor failed, not the `JMP` that a test/compare consumes.
pub fn valid_reentry(code: &[u32], pc: usize) -> bool {
    use OpCode::*;
    let Some(i) = code.get(pc).map(|&w| Instruction(w)) else {
        return false;
    };
    let Some(op) = i.opcode() else {
        return false;
    };
    if matches!(op, Mmbin | Mmbini | Mmbink | Extraarg) {
        return false;
    }
    if pc > 0 {
        if let Some(prev) = Instruction(code[pc - 1]).opcode() {
            // the instruction after a test/compare is the JMP it consumes; after a LOADKX / (5.5)
            // NEWTABLE / SETLIST-with-extra the next word is an EXTRAARG operand (handled above)
            if matches!(
                prev,
                Eq | Lt | Le | Eqk | Eqi | Lti | Lei | Gti | Gei | Test | Testset
            ) && op == Jmp
            {
                return false;
            }
        }
    }
    true
}

impl Cx<'_> {
    pub(crate) fn env(&self) -> CallEnv {
        self.cfg.calls.expect("calls enabled")
    }
    pub(crate) fn calls_on(&self) -> bool {
        self.cfg.calls.is_some()
    }

    pub(crate) fn l_exit(&mut self, which: u8) -> L {
        let slot = match which {
            0 => &mut self.sh.exit_fc,
            1 => &mut self.sh.exit_err,
            _ => &mut self.sh.exit_ret,
        };
        if let Some(l) = *slot {
            return l;
        }
        let l = self.p.label();
        match which {
            0 => self.sh.exit_fc = Some(l),
            1 => self.sh.exit_err = Some(l),
            _ => self.sh.exit_ret = Some(l),
        }
        l
    }
    pub(crate) fn l_enter(&mut self) -> L {
        if let Some(l) = self.sh.enter {
            return l;
        }
        let l = self.p.label();
        self.sh.enter = Some(l);
        l
    }
    /// BUNDLE-B: the shared "a metamethod frame of a tier-1 comparison returned" sequence (the frame is already popped).
    pub(crate) fn l_ret_mm(&mut self) -> L {
        if let Some(l) = self.sh.ret_mm {
            return l;
        }
        let l = self.p.label();
        self.sh.ret_mm = Some(l);
        l
    }
    fn l_ret_tail(&mut self) -> L {
        if let Some(l) = self.sh.ret_tail {
            return l;
        }
        let l = self.p.label();
        self.sh.ret_tail = Some(l);
        l
    }

    /// Jump to the shared "return to the caller" sequence (the frame is already popped).
    pub(super) fn jmp_ret_tail(&mut self) {
        let l = self.l_ret_tail();
        self.op(Op::Jmp(l));
    }

    /// Interrupt poll for a return: leave at the instruction (with `trap` set) when Ctrl-C is pending.
    pub(super) fn return_poll(&mut self, pc: u32) {
        let x = self.exit_trap(pc);
        // (V0 = callstatus and V2 = caller are live in `return_prologue`: use V1 and V4)
        self.op(Op::Mov(1, self.cfg.interrupt_addr as u64));
        self.ld(
            4,
            W::B1,
            M {
                base: Base::Reg(1),
                off: 0,
            },
        );
        self.br(Cc::Ne, 4, Src::Imm(0), x);
    }

    fn m_reg(v: u8, off: i32) -> M {
        M {
            base: Base::Reg(v),
            off,
        }
    }

    /// `CALL A B C`.
    pub(super) fn tpl_call(&mut self, pc: u32, i: Instruction) {
        let env = self.env();
        let (a, b, c) = (i.a(), i.b(), i.c());
        let x = self.exit_at(pc);
        let xt = self.exit_trap(pc);
        let slow = self.p.label();
        let next = self.pcl[pc as usize + 1];
        // hooks (installed asynchronously) and Ctrl-C: the interpreter performs the call
        self.ld(0, W::B1, thw(thr::HOOKMASK));
        self.br(Cc::Ne, 0, Src::Imm(0), x);
        self.op(Op::Mov(1, self.cfg.interrupt_addr as u64));
        self.ld(2, W::B1, Self::m_reg(1, 0));
        self.br(Cc::Ne, 2, Src::Imm(0), xt);
        let xc = self.exit_cnt(pc, 1);
        self.op(Op::CountSub { n: 1, exit: xc });
        // top = end of the arguments (like `st.set_top(sidx!(a + b))`); B == 0: already set
        if b != 0 {
            self.ld(5, W::B8, ciw(ci::FUNC));
            self.op(Op::Add(5, Src::Imm((a + b + 1) as i32)));
            self.st(W::B8, thw(thr::TOP), 5);
        } else {
            self.ld(5, W::B8, thw(thr::TOP));
        }
        // savepc: the instruction after the CALL
        self.op(Op::Mov(0, self.pc_addr(pc + 1)));
        self.st(W::B8, ciw(ci::SAVEDPC), 0);
        // ---- flat call guards (any failure: the helper does the general call)
        self.ld(0, W::B1, fr(a, 0));
        // BUNDLE-T1I: a callee that is not a Lua closure may be a built-in with an inline fast path (`t1inline`)
        let inl = self.t1i_applies(pc, b, c);
        let notlcl = if inl { self.p.label() } else { slow };
        self.br(Cc::Ne, 0, Src::Imm(i32::from(ctb(LUA_VLCL))), notlcl);
        self.ld(1, W::B8, fr(a, 8)); // closure
        self.ld(2, W::B8, Self::m_reg(1, lclosure::PROTO as i32));
        self.ld(3, W::B8, Self::m_reg(2, proto::JIT as i32));
        self.br(Cc::Le, 3, Src::Imm(1), slow); // not compiled (0) or never compiled (1)
        self.ld(4, W::B8, Self::m_reg(3, HDR_ENTRY0));
        self.br(Cc::Eq, 4, Src::Imm(0), slow); // compiled for its loops only: no function entry
        self.ld(4, W::B1, Self::m_reg(2, proto::NUMPARAMS as i32));
        if b != 0 {
            self.br(Cc::Gt, 4, Src::Imm(b as i32 - 1), slow); // too few arguments: nil-fill in the helper
        } else {
            self.ld(6, W::B8, ciw(ci::FUNC));
            self.op(Op::Add(6, Src::Imm(a as i32 + 1)));
            self.op(Op::MovR(0, 5));
            self.op(Op::Sub(0, Src::V(6)));
            self.op(Op::Sub(0, Src::Imm(1))); // nargs = top - (callee func + 1)
            self.br(Cc::Gt, 4, Src::V(0), slow);
        }
        self.ld(0, W::B1, Self::m_reg(2, proto::MAXSTACKSIZE as i32));
        self.ld(6, W::B8, thw(thr::STACK_SIZE));
        self.op(Op::Sub(6, Src::V(5)));
        self.br(Cc::Le, 6, Src::V(0), slow); // stack_size - top <= fsize
        self.ld(6, W::B8, ciw(ci::NEXT));
        self.br(Cc::Eq, 6, Src::Imm(0), slow);
        // ---- the flat call: V6 = new CallInfo, V0 = fsize, V2 = proto, V3 = descriptor
        self.ld(1, W::B8, ciw(ci::FUNC));
        self.op(Op::Add(1, Src::Imm(a as i32 + 1))); // callee func index
        self.st(W::B8, Self::m_reg(6, ci::FUNC as i32), 1);
        self.op(Op::Add(0, Src::V(1)));
        self.op(Op::Add(0, Src::Imm(1)));
        self.st(W::B8, Self::m_reg(6, ci::TOP as i32), 0); // func + 1 + fsize
        self.st_imm(W::B4, Self::m_reg(6, ci::CALLSTATUS as i32), c as i32); // nresults + 1
        self.st_imm(W::B4, Self::m_reg(6, ci::TRAP as i32), 0);
        self.ld(0, W::B8, Self::m_reg(2, env.proto_code));
        self.st(W::B8, Self::m_reg(6, ci::SAVEDPC as i32), 0);
        self.ld(0, W::B8, Self::m_reg(2, env.proto_k));
        self.st(W::B8, Self::m_reg(6, ci::KCACHE as i32), 0);
        self.st(W::B8, thw(thr::CI), 6);
        self.op(Op::Lea(1, fr(a + 1, 0)));
        self.ld(3, W::B8, Self::m_reg(3, HDR_ENTRY0));
        self.op(Op::SetCi(6));
        self.op(Op::SetFrame(1));
        self.op(Op::SetK(0));
        self.op(Op::JmpR(3));
        if inl {
            self.t1i_emit(pc, a, b, next, slow, notlcl);
        }
        // ---- the general call
        self.op(Op::Bind(slow));
        let after = self.p.label();
        if env.ccall_helper != 0 {
            // BUNDLE-C: a light C function callee takes the lean helper
            let gen = self.p.label();
            self.ld(0, W::B1, fr(a, 0));
            self.br(Cc::Ne, 0, Src::Imm(i32::from(LUA_VLCF)), gen);
            self.op(Op::CallH {
                f: env.ccall_helper,
                a: i.0,
                b: 0,
            });
            self.op(Op::Jmp(after));
            self.op(Op::Bind(gen));
        }
        self.op(Op::CallH {
            f: env.call_helper,
            a: i.0,
            b: 0,
        });
        self.op(Op::Bind(after));
        let enter = self.l_enter();
        let err = self.l_exit(1);
        let native = self.p.label();
        self.br(Cc::Eq, 0, Src::Imm(H_LUA as i32), enter);
        self.br(Cc::Eq, 0, Src::Imm(H_DONE as i32), native);
        self.op(Op::Jmp(err));
        self.op(Op::Bind(native));
        // a native function ran: the stack may have moved; hooks may have been installed
        self.op(Op::ReloadFrame {
            stack_off: env.stack_off,
        });
        let xh = self.exit_trap(pc + 1);
        self.ld(0, W::B1, thw(thr::HOOKMASK));
        self.br(Cc::Ne, 0, Src::Imm(0), xh);
        self.op(Op::Jmp(next));
    }

    /// `TAILCALL A B C k`.
    pub(super) fn tpl_tailcall(&mut self, pc: u32, i: Instruction) {
        let env = self.env();
        let x = self.exit_at(pc);
        let xt = self.exit_trap(pc);
        self.ld(0, W::B1, thw(thr::HOOKMASK));
        self.br(Cc::Ne, 0, Src::Imm(0), x);
        self.op(Op::Mov(1, self.cfg.interrupt_addr as u64));
        self.ld(2, W::B1, Self::m_reg(1, 0));
        self.br(Cc::Ne, 2, Src::Imm(0), xt);
        if env.t1q & 32 == 0 {
            self.ld(0, W::B4, ciw(ci::CALLSTATUS));
            self.op(Op::And(0, Src::Imm(ci::CIST_MMRET as i32)));
            self.br(Cc::Ne, 0, Src::Imm(0), x); // metamethod frame: the interpreter finishes it
        } // (BUNDLE-V `t1q` bit 5: the frame stays the metamethod frame of its caller's instruction, its callee returns through `Cold::MmRet`; `jit_tailcall` refuses the callees it does not take: H_EXIT)
        let xc = self.exit_cnt(pc, 1);
        self.op(Op::CountSub { n: 1, exit: xc });
        self.op(Op::Mov(0, self.pc_addr(pc + 1)));
        self.st(W::B8, ciw(ci::SAVEDPC), 0);
        let fast = env.tail_fast && i.c() == 0 && i.b() != 0 && i.b() <= 16;
        let slow = if fast { self.p.label() } else { 0 };
        // R1: the interpreter's `OP_TAILCALL` of a Lua closure, inline: the callee's function and arguments move down over
        // the frame's function slot (`pre_tail_call`), the frame (the same `CallInfo`) becomes the callee's, and control
        // continues at the callee's entry. Only the common shape: no upvalues to close (`k`), a non-vararg caller (`C`), a
        // fixed argument count (`B`), a compiled callee with at least as many arguments as parameters and the stack room
        // `pre_tail_call` checks; anything else (and every failing guard, nothing has been changed by then) takes the helper.
        let (a, b) = (i.a(), i.b());
        if fast {
            // V5 = top = end of the arguments
            self.ld(5, W::B8, ciw(ci::FUNC));
            self.op(Op::Add(5, Src::Imm((a + b + 1) as i32)));
            self.ld(0, W::B1, fr(a, 0));
            self.br(Cc::Ne, 0, Src::Imm(i32::from(ctb(LUA_VLCL))), slow);
            self.ld(1, W::B8, fr(a, 8)); // closure
            self.ld(2, W::B8, Self::m_reg(1, lclosure::PROTO as i32));
            self.ld(3, W::B8, Self::m_reg(2, proto::JIT as i32));
            self.br(Cc::Le, 3, Src::Imm(1), slow); // not compiled (0) or never compiled (1)
            self.ld(4, W::B8, Self::m_reg(3, HDR_ENTRY0));
            self.br(Cc::Eq, 4, Src::Imm(0), slow); // compiled for its loops only: no function entry
            self.ld(4, W::B1, Self::m_reg(2, proto::NUMPARAMS as i32));
            if env.tail_mut & TM_ARGS == 0 {
                self.br(Cc::Gt, 4, Src::Imm(b as i32 - 1), slow); // too few arguments: nil-fill in the helper
            }
            self.ld(0, W::B1, Self::m_reg(2, proto::MAXSTACKSIZE as i32));
            self.ld(6, W::B8, thw(thr::STACK_SIZE));
            self.op(Op::Sub(6, Src::V(5)));
            self.br(Cc::Le, 6, Src::V(0), slow); // stack_size - top <= fsize
            if i.k() != 0 && env.tail_mut & TM_CLOSE == 0 {
                // M1: `k` (the function has closures or a generic `for`): `luaF_closeupval(base)` is a no-op when no open
                // upvalue lies at or above the frame base (the open list is sorted, its head is the highest); an open one
                // takes the helper
                let nc = self.p.label();
                self.ld(4, W::B8, thw(thr::OPENUPVAL));
                self.br(Cc::Eq, 4, Src::Imm(0), nc);
                self.ld(
                    4,
                    W::B8,
                    Self::m_reg(4, rn_core::jit_layout::upval::V as i32),
                );
                self.op(Op::Lea(6, fr(0, 0)));
                self.br(Cc::Ge, 4, Src::V(6), slow);
                self.op(Op::Bind(nc));
            }
            // ---- commit: V0 = fsize, V2 = proto, V3 = descriptor
            let n = if env.tail_mut & TM_COPY != 0 {
                b - 1
            } else {
                b
            };
            for j in 0..n {
                self.copy_value(fr(0, -16 + 16 * j as i32), fr(a + j, 0));
            }
            self.ld(1, W::B8, ciw(ci::FUNC));
            self.op(Op::Add(0, Src::V(1)));
            self.op(Op::Add(0, Src::Imm(1)));
            self.st(W::B8, ciw(ci::TOP), 0); // func + 1 + fsize
            self.op(Op::Add(1, Src::Imm(b as i32)));
            self.st(W::B8, thw(thr::TOP), 1); // func + narg1
            if env.tail_mut & TM_STATUS == 0 {
                self.ld(0, W::B4, ciw(ci::CALLSTATUS));
                self.op(Op::Or(0, Src::Imm(ci::CIST_TAIL as i32)));
                self.st(W::B4, ciw(ci::CALLSTATUS), 0);
            }
            self.ld(0, W::B8, Self::m_reg(2, env.proto_code));
            self.st(W::B8, ciw(ci::SAVEDPC), 0);
            self.ld(0, W::B8, Self::m_reg(2, env.proto_k));
            self.st(W::B8, ciw(ci::KCACHE), 0);
            self.ld(3, W::B8, Self::m_reg(3, HDR_ENTRY0));
            self.op(Op::SetK(0));
            self.op(Op::JmpR(3));
            self.op(Op::Bind(slow));
        }
        self.op(Op::CallH {
            f: env.tail_helper,
            a: i.0,
            b: if env.t1q & 32 != 0 {
                self.mm_ret_flags()
            } else {
                0
            },
        });
        let enter = self.l_enter();
        let err = self.l_exit(1);
        let fc = self.l_exit(0);
        let done = self.p.label();
        self.br(Cc::Eq, 0, Src::Imm(H_LUA as i32), enter);
        self.br(Cc::Eq, 0, Src::Imm(H_RETURNED as i32), done);
        if env.t1q & 32 != 0 {
            // (BUNDLE-V: the helper refused (nothing changed: leave at the instruction, giving back its count) or a native tail callee of a metamethod frame; `thread.ci` = the caller, whose instruction is finished like after `jit_mm_ret`)
            let mmret = self.l_ret_mm();
            self.br(
                Cc::Eq,
                0,
                Src::Imm(rn_core::jit_helpers::H_MMPOP as i32),
                mmret,
            );
            self.br(Cc::Eq, 0, Src::Imm(rn_core::jit_helpers::H_MMFC as i32), fc);
            self.br(Cc::Eq, 0, Src::Imm(rn_core::jit_helpers::H_EXIT as i32), xc);
        }
        self.op(Op::Jmp(err));
        self.op(Op::Bind(done));
        // a native tail callee finished and popped the frame: return like RETURN. A `CIST_FRESH` frame returns to
        // its C caller (`Returned`) whatever the hooks are (`FrameChanged` would resume the wrong frame); only
        // then is the hook test (a native may have installed one) allowed to leave to the interpreter.
        let ret = self.l_exit(2);
        self.ld(0, W::B4, ciw(ci::CALLSTATUS));
        self.op(Op::And(0, Src::Imm(ci::CIST_FRESH as i32)));
        self.br(Cc::Ne, 0, Src::Imm(0), ret);
        self.ld(0, W::B1, thw(thr::HOOKMASK));
        self.br(Cc::Ne, 0, Src::Imm(0), fc);
        self.jmp_ret_tail();
    }

    /// Generic `RETURN A B C k`: `B - 1` results, or up to `top` when `B == 0`. `k`: close upvalues first (the
    /// `jit_return_close` helper; tbc variables included); `C != 0`: vararg frame (`func` is moved back).
    pub(super) fn tpl_return(&mut self, pc: u32, i: Instruction) {
        let a = i.a();
        let x = self.exit_at(pc);
        // TBC variables are closed by the interpreter's `post_call`
        self.ld(0, W::B4, ciw(ci::CALLSTATUS));
        self.op(Op::And(0, Src::Imm(rn_core::state::CIST_TBC as i32)));
        self.br(Cc::Ne, 0, Src::Imm(0), x);
        if i.k() != 0 {
            // hooks / metamethod frames / base frame: the interpreter performs the whole instruction
            // (before the helper has side effects)
            self.ld(0, W::B4, ciw(ci::TRAP));
            self.br(Cc::Ne, 0, Src::Imm(0), x);
            self.ld(0, W::B4, ciw(ci::CALLSTATUS));
            self.op(Op::And(0, Src::Imm(ci::CIST_MMRET as i32)));
            // (BUNDLE-S S1: the handler of a table read / store returns through `Cold::MmRet`, which closes the upvalues itself)
            let xm = self.mm_ret_label(pc, x);
            self.br(Cc::Ne, 0, Src::Imm(0), xm);
            self.ld(2, W::B8, ciw(ci::PREVIOUS));
            self.br(Cc::Eq, 2, Src::Imm(0), x);
            // M1: nothing to close (no open upvalue at or above the frame base; tbc variables exited above): the plain return
            let skip = self.p.label();
            let noskip = self.p.label();
            let fast = self.env().tail_fast;
            if fast && self.env().tail_mut & TM_CLOSE != 0 {
                self.op(Op::Jmp(skip)); // (mutant: nothing is ever closed)
            } else if fast {
                // (`OP_TBC` registers the variable in the thread's list without marking the frame: any registered variable takes the helper)
                self.ld(4, W::B8, thw(thr::TBC_LEN));
                self.br(Cc::Ne, 4, Src::Imm(0), noskip);
                self.ld(4, W::B8, thw(thr::OPENUPVAL));
                self.br(Cc::Eq, 4, Src::Imm(0), skip);
                self.ld(
                    4,
                    W::B8,
                    Self::m_reg(4, rn_core::jit_layout::upval::V as i32),
                );
                self.op(Op::Lea(6, fr(0, 0)));
                self.br(Cc::Lt, 4, Src::V(6), skip);
            }
            self.op(Op::Bind(noskip));
            self.op(Op::Mov(0, self.pc_addr(pc + 1)));
            self.st(W::B8, ciw(ci::SAVEDPC), 0);
            self.op(Op::CallH {
                f: rn_core::jit_helpers::jit_return_close as *const () as usize as u64,
                a: i.0,
                b: 0,
            });
            let ok = self.p.label();
            let err = self.l_exit(1);
            self.br(Cc::Eq, 0, Src::Imm(H_DONE as i32), ok);
            // a `__close` handler raised: the RETURN was fetched (the success path counts it in `return_prologue`)
            let m = M {
                base: Base::Cnt,
                off: 0,
            };
            self.ld(1, W::B4, m);
            self.op(Op::Sub(1, Src::Imm(1)));
            self.st(W::B4, m, 1);
            self.op(Op::Jmp(err));
            self.op(Op::Bind(ok));
            self.op(Op::ReloadFrame {
                stack_off: self.env().stack_off,
            });
            self.op(Op::Bind(skip));
        }
        self.return_prologue(pc); // V0 = wanted (-1 = all), V3 = func index
        if i.b() == 0 {
            self.ld(1, W::B8, thw(thr::TOP));
            self.op(Op::Sub(1, Src::V(3)));
            self.op(Op::Sub(1, Src::Imm(a as i32 + 1)));
        } else {
            self.op(Op::Mov(1, u64::from(i.b() - 1)));
        }
        if i.c() != 0 {
            // vararg frame: results go to the original `func` (`func -= nextraargs + C`)
            self.ld(6, W::B4, ciw(ci::NEXTRAARGS));
            self.op(Op::Add(6, Src::Imm(i.c() as i32)));
            self.op(Op::Sub(3, Src::V(6)));
        }
        let (have_w, cnt_ok, copy, fill, done) = (
            self.p.label(),
            self.p.label(),
            self.p.label(),
            self.p.label(),
            self.p.label(),
        );
        self.br(Cc::Ge, 0, Src::Imm(0), have_w);
        self.op(Op::MovR(0, 1));
        self.op(Op::Bind(have_w));
        self.op(Op::MovR(2, 1));
        self.br(Cc::Le, 2, Src::V(0), cnt_ok);
        self.op(Op::MovR(2, 0));
        self.op(Op::Bind(cnt_ok));
        self.op(Op::MovR(1, 3));
        self.op(Op::Add(1, Src::V(0))); // new top = func + wanted
        self.op(Op::Lea(4, fr(0, -16)));
        self.op(Op::Lea(5, fr(a, 0)));
        if i.c() != 0 {
            self.op(Op::Shl(6, 4));
            self.op(Op::Sub(4, Src::V(6)));
        }
        self.op(Op::Bind(copy));
        self.br(Cc::Le, 2, Src::Imm(0), fill);
        self.ld(6, W::B8, Self::m_reg(5, 0));
        self.st(W::B8, Self::m_reg(4, 0), 6);
        self.ld(6, W::B8, Self::m_reg(5, 8));
        self.st(W::B8, Self::m_reg(4, 8), 6);
        self.op(Op::Add(4, Src::Imm(16)));
        self.op(Op::Add(5, Src::Imm(16)));
        self.op(Op::Sub(2, Src::Imm(1)));
        self.op(Op::Sub(0, Src::Imm(1)));
        self.op(Op::Jmp(copy));
        self.op(Op::Bind(fill));
        self.br(Cc::Le, 0, Src::Imm(0), done);
        self.st_imm(
            W::B8,
            Self::m_reg(4, 0),
            i32::from(rn_core::value::LUA_VNIL),
        );
        self.st_imm(W::B8, Self::m_reg(4, 8), 0);
        self.op(Op::Add(4, Src::Imm(16)));
        self.op(Op::Sub(0, Src::Imm(1)));
        self.op(Op::Jmp(fill));
        self.op(Op::Bind(done));
        self.st(W::B8, thw(thr::TOP), 1);
        self.jmp_ret_tail();
    }

    /// Emits the shared stubs that were referenced.
    pub(super) fn emit_shared(&mut self) {
        if !self.calls_on() {
            return;
        }
        let env = self.env();
        // referenced stubs may reference each other: create the labels first
        if self.sh.enter.is_some() {
            self.l_exit(0);
        }
        if self.sh.ret_tail.is_some() {
            self.l_exit(0);
            self.l_exit(2);
        }
        if self.sh.ret_mm.is_some() {
            self.l_exit(0);
        }
        if let Some(l) = self.sh.exit_fc {
            self.op(Op::Bind(l));
            self.op(Op::Ret(JitExit::FrameChanged as u32));
        }
        if let Some(l) = self.sh.exit_err {
            self.op(Op::Bind(l));
            self.op(Op::Ret(JitExit::Error as u32));
        }
        if let Some(l) = self.sh.exit_ret {
            self.op(Op::Bind(l));
            self.op(Op::Ret(JitExit::Returned as u32));
        }
        if let Some(l) = self.sh.enter {
            // A helper pushed (or rewrote in place) the Lua frame `thread.ci`: continue in compiled code
            // when its proto is compiled, else let the interpreter run it.
            let fc = self.l_exit(0);
            self.op(Op::Bind(l));
            self.ld(0, W::B1, thw(thr::HOOKMASK));
            self.br(Cc::Ne, 0, Src::Imm(0), fc);
            self.ld(0, W::B8, thw(thr::CI));
            self.op(Op::SetCi(0));
            self.op(Op::ReloadFrame {
                stack_off: env.stack_off,
            });
            self.ld(2, W::B8, fr(0, -8)); // closure
            self.ld(2, W::B8, Self::m_reg(2, lclosure::PROTO as i32));
            self.ld(3, W::B8, Self::m_reg(2, proto::JIT as i32));
            self.br(Cc::Le, 3, Src::Imm(1), fc);
            self.ld(3, W::B8, Self::m_reg(3, HDR_ENTRY0));
            self.br(Cc::Eq, 3, Src::Imm(0), fc);
            self.ld(1, W::B8, ciw(ci::KCACHE));
            self.op(Op::SetK(1));
            self.st_imm(W::B4, ciw(ci::TRAP), 0); // consume the `TRAP_JIT` flag of the entry hook
            self.op(Op::JmpR(3));
        }
        if let Some(l) = self.sh.ret_mm {
            // BUNDLE-B: the caller's comparison is not finished yet (`savedpc` = its `JMP`); the continuation stub of that `JMP`
            // (`Cold::Cmp`) reads the result `jit_mm_ret` left at the caller's `top`. A caller without such an entry (not compiled,
            // interpreter frame) gets its comparison finished by `jit_mm_fin` and continues in the interpreter.
            let fc = self.l_exit(0);
            let fin = self.p.label();
            self.op(Op::Bind(l));
            self.ld(2, W::B8, thw(thr::CI)); // the caller (popped to by the return)
            self.ld(3, W::B8, Self::m_reg(2, ci::FUNC as i32));
            self.op(Op::Shl(3, 4));
            self.ld(4, W::B8, thw(env.stack_off as usize));
            self.op(Op::Add(4, Src::V(3))); // &stack[caller.func]
            self.ld(5, W::B8, Self::m_reg(4, 8)); // its closure
            self.ld(5, W::B8, Self::m_reg(5, lclosure::PROTO as i32));
            self.ld(6, W::B8, Self::m_reg(5, proto::JIT as i32));
            self.br(Cc::Le, 6, Src::Imm(1), fin);
            self.ld(0, W::B8, Self::m_reg(2, ci::SAVEDPC as i32));
            self.ld(1, W::B8, Self::m_reg(5, env.proto_code));
            self.op(Op::Sub(0, Src::V(1)));
            self.ld(1, W::B8, Self::m_reg(6, HDR_RESUME));
            self.op(Op::Add(1, Src::V(0)));
            self.ld(1, W::B4, Self::m_reg(1, 0));
            self.br(Cc::Eq, 1, Src::Imm(0), fin);
            self.op(Op::And(1, Src::Imm(!RESUME_MM_ONLY as i32))); // (BUNDLE-S: the flag of the entries only this return uses)
            self.ld(0, W::B8, Self::m_reg(6, HDR_BASE_M1));
            self.op(Op::Add(0, Src::V(1)));
            self.ld(3, W::B8, Self::m_reg(2, ci::KCACHE as i32));
            self.op(Op::Lea(4, Self::m_reg(4, 16)));
            self.op(Op::SetCi(2));
            self.op(Op::SetFrame(4));
            self.op(Op::SetK(3));
            self.op(Op::JmpR(0));
            self.op(Op::Bind(fin));
            self.op(Op::CallH {
                f: rn_core::jit_helpers::jit_mm_fin as *const () as usize as u64,
                a: 0,
                b: 0,
            });
            self.op(Op::Jmp(fc));
        }
        if let Some(l) = self.sh.ret_tail {
            let fc = self.l_exit(0);
            let ret = self.l_exit(2);
            self.op(Op::Bind(l));
            self.ld(0, W::B4, ciw(ci::CALLSTATUS));
            self.op(Op::And(0, Src::Imm(ci::CIST_FRESH as i32)));
            self.br(Cc::Ne, 0, Src::Imm(0), ret);
            self.ld(2, W::B8, thw(thr::CI)); // the caller (popped to by the return)
            self.ld(3, W::B8, Self::m_reg(2, ci::FUNC as i32));
            self.op(Op::Shl(3, 4));
            self.ld(4, W::B8, thw(env.stack_off as usize));
            self.op(Op::Add(4, Src::V(3))); // &stack[caller.func]
            self.ld(5, W::B8, Self::m_reg(4, 8)); // its closure
            self.ld(5, W::B8, Self::m_reg(5, lclosure::PROTO as i32));
            self.ld(6, W::B8, Self::m_reg(5, proto::JIT as i32));
            self.br(Cc::Le, 6, Src::Imm(1), fc);
            self.ld(0, W::B8, Self::m_reg(2, ci::SAVEDPC as i32));
            self.ld(1, W::B8, Self::m_reg(5, env.proto_code));
            self.op(Op::Sub(0, Src::V(1))); // byte offset of the resume pc
            self.ld(1, W::B8, Self::m_reg(6, HDR_RESUME));
            self.op(Op::Add(1, Src::V(0)));
            self.ld(1, W::B4, Self::m_reg(1, 0)); // native offset + 1 (0 = not a re-entry point)
            self.br(Cc::Eq, 1, Src::Imm(0), fc);
            self.ld(0, W::B8, Self::m_reg(6, HDR_BASE_M1));
            self.op(Op::Add(0, Src::V(1)));
            self.ld(3, W::B8, Self::m_reg(2, ci::KCACHE as i32));
            self.op(Op::Lea(4, Self::m_reg(4, 16)));
            self.op(Op::SetCi(2));
            self.op(Op::SetFrame(4));
            self.op(Op::SetK(3));
            self.op(Op::JmpR(0));
        }
    }
}
