//! J2b templates: generic `for` (TFORPREP, TFORCALL, TFORLOOP), vararg functions (VARARGPREP, VARARG) and
//! closing (CLOSE, TBC; RETURN with the `k` bit lives in `calls.rs`).
//!
//! Everything that is not trivially inline goes through `extern "C"` helpers in `rn_core::jit_helpers` that
//! are the interpreter's own code for the opcode (same functions, same order of effects):
//!
//! * `TFORPREP` swaps the control and closing slots inline when the closing value is `nil`/`false` (nothing to
//!   mark); anything else exits at the instruction (the interpreter registers the tbc variable).
//! * `TFORCALL` calls `jit_tforcall`: the base library's `next` (recognised by address) and the `ipairs`
//!   iterator over a table run without a call frame and produce the builtins' exact results (same traversal
//!   order and key validation: an invalid key makes the helper fall back to the real call, which raises the
//!   error); any other iterator is the interpreter's nested `call`.
//! * `TFORLOOP` is the inline `nil` test and back-edge (with the usual trap/interrupt poll).
//! * `VARARGPREP`/`VARARG` call `adjust_varargs`/`get_varargs` (the frame is re-based: `ReloadFrame`).
//! * `CLOSE`/`TBC` call `State::close`/`new_tbc_upval`. A `__close` metamethod runs as a nested call exactly
//!   like in the interpreter; yields come back as error exits (the interpreter's `finish_op` repeats the
//!   instruction, the helpers leave `savedpc` at the instruction after it).
//!
//! The helper-based templates are kind `Guarded`: they test for hooks, count themselves after their guards and
//! end their block (the instruction after them is a block head).

use crate::ir::{Base, Cc, Op, Src, M, W};
use crate::lower::{ciw, fr, thw, Cx, Kind};
use rn_core::instruction::Instruction;
use rn_core::jit_helpers as h;
use rn_core::jit_hooks::thread as thr;
use rn_core::jit_layout::{table, LUA54};
use rn_core::value::{ctb, LUA_VFALSE, LUA_VLCF, LUA_VNIL, LUA_VNUMINT, LUA_VTABLE};

use rn_core::ver::active::OpCode;

/// Classification of the opcodes this file implements (`None` = not ours).
pub(crate) fn kind(op: OpCode, _i: Instruction) -> Option<Kind> {
    Some(match op {
        OpCode::Tforprep => Kind::Guard,
        OpCode::Tforloop => Kind::Branch,
        OpCode::Tforcall | OpCode::Varargprep | OpCode::Vararg | OpCode::Close | OpCode::Tbc => {
            Kind::Guarded
        }
        _ => return None,
    })
}

/// Opcodes of this file whose templates call helpers (stubs when calls are disabled).
pub(crate) fn needs_calls(op: Option<OpCode>) -> bool {
    matches!(
        op,
        Some(OpCode::Tforcall | OpCode::Varargprep | OpCode::Vararg | OpCode::Close | OpCode::Tbc)
    )
}

fn addr(
    f: unsafe extern "C" fn(
        *mut rn_core::state::Thread,
        *mut rn_core::state::CallInfo,
        u64,
        u64,
    ) -> u32,
) -> u64 {
    f as usize as u64
}

impl Cx<'_> {
    pub(crate) fn t_loops(&mut self, pc: u32, i: Instruction, op: OpCode) {
        match op {
            OpCode::Tforprep => self.t_tforprep(pc, i),
            OpCode::Tforloop => self.t_tforloop(pc, i),
            OpCode::Tforcall => self.helper_template(pc, i, addr(h::jit_tforcall), true, true),
            OpCode::Varargprep => self.helper_template(pc, i, addr(h::jit_varargprep), true, false),
            OpCode::Vararg => self.helper_template(pc, i, addr(h::jit_vararg), false, false),
            OpCode::Close => self.helper_template(pc, i, addr(h::jit_close), false, false),
            OpCode::Tbc => self.helper_template(pc, i, addr(h::jit_tbc), false, false),
            _ => unreachable!("not a loops opcode"),
        }
    }

    /// The shape shared by all helper templates: (hook guard), block count, `savedpc` = the instruction after
    /// this one, helper call, error exit, frame reload, late hook test.
    fn helper_template(&mut self, pc: u32, i: Instruction, f: u64, hook_guard: bool, tfor: bool) {
        let env = self.cfg.calls.expect("calls enabled");
        let x = self.exit_at(pc);
        if hook_guard {
            // active hooks: the interpreter runs the instruction (call hooks, `hook_call` of VARARGPREP)
            self.ld(0, W::B1, thw(thr::HOOKMASK));
            self.br(Cc::Ne, 0, Src::Imm(0), x);
        }
        let xc = self.exit_cnt(pc, 1);
        self.op(Op::CountSub { n: 1, exit: xc });
        if tfor {
            self.ipairs_fast(pc, i);
        }
        self.op(Op::Mov(0, self.pc_addr(pc + 1)));
        self.st(W::B8, ciw(rn_core::jit_layout::callinfo::SAVEDPC), 0);
        self.op(Op::CallH { f, a: i.0, b: 0 });
        let ok = self.p.label();
        let err = self.l_exit(1);
        self.br(Cc::Eq, 0, Src::Imm(h::H_DONE as i32), ok);
        self.op(Op::Jmp(err));
        self.op(Op::Bind(ok));
        // the stack may have moved and `func` may have changed (VARARGPREP); a hook may have been installed
        self.op(Op::ReloadFrame {
            stack_off: env.stack_off,
        });
        let xh = self.exit_trap(pc + 1);
        self.ld(0, W::B1, thw(thr::HOOKMASK));
        self.br(Cc::Ne, 0, Src::Imm(0), xh);
        let next = self.pcl[pc as usize + 1];
        self.op(Op::Jmp(next));
    }

    /// Inline `ipairs` step for `TFORCALL` (falls through to the helper call when it does not apply): the
    /// iterator is the registered `ipairs_aux` (compared by address), the state is a table, the control
    /// variable an integer `c` with `0 <= c < asize` and array slot `c` (0-based, i.e. index `c + 1`) holds
    /// a non-nil value. Then `R[A+3] = c + 1`, `R[A+4] = value`, the remaining loop variables are nil: exactly
    /// the builtin's result (a non-nil hit never consults `__index`). Everything else - the end of the array,
    /// the hash part, metatables, other types - is the helper's.
    fn ipairs_fast(&mut self, pc: u32, i: Instruction) {
        let ia = rn_core::jit_hooks::builtin_ipairs_aux();
        let (a, c) = (i.a(), i.c());
        if ia == 0 || c < 1 {
            return;
        }
        let slow = self.p.label();
        let next = self.pcl[pc as usize + 1];
        let m = |v: u8, off: usize| M {
            base: Base::Reg(v),
            off: off as i32,
        };
        // near the C-stack limit the real call (and its "C stack overflow") is the helper's
        self.ld(0, W::B4, thw(thr::NCALLS));
        self.op(Op::And(0, Src::Imm(0xffff)));
        self.br(
            Cc::Ge,
            0,
            Src::Imm(rn_core::state::LUAI_MAXCCALLS as i32 - 1),
            slow,
        );
        self.ld(0, W::B1, fr(a, 0));
        self.br(Cc::Ne, 0, Src::Imm(i32::from(LUA_VLCF)), slow);
        self.ld(1, W::B8, fr(a, 8));
        self.op(Op::Mov(2, ia as u64));
        self.br(Cc::Ne, 1, Src::V(2), slow);
        self.ld(0, W::B1, fr(a + 1, 0));
        self.br(Cc::Ne, 0, Src::Imm(i32::from(ctb(LUA_VTABLE))), slow);
        // control variable: `R[A+3]` (5.5) / `R[A+2]` (5.4); the results start at `R[A+3]` / `R[A+4]`
        let (ctl, res) = if LUA54 {
            (a + 2, a + 4)
        } else {
            (a + 3, a + 3)
        };
        self.ld(0, W::B1, fr(ctl, 0));
        self.br(Cc::Ne, 0, Src::Imm(i32::from(LUA_VNUMINT)), slow);
        self.ld(3, W::B8, fr(a + 1, 8)); // the table
        self.ld(2, W::B8, fr(ctl, 8)); // previous index = 0-based slot of the next element
        self.br(Cc::Lt, 2, Src::Imm(0), slow);
        self.ld(4, W::B4, m(3, table::ASIZE));
        self.br(Cc::Ge, 2, Src::V(4), slow);
        self.ld(1, W::B8, m(3, table::ARRAY));
        self.alimit_touch(3, 1, 2, 4, 5); // 5.4: `luaH_getint` updates the array limit
        self.op(Op::MovR(4, 1));
        self.op(Op::Add(4, Src::V(2)));
        self.ld(5, W::B1, m(4, table::ARRAY_TAG_BASE)); // slot tag
        self.op(Op::MovR(6, 5));
        self.op(Op::And(6, Src::Imm(0x0F)));
        self.br(Cc::Eq, 6, Src::Imm(0), slow); // nil: end of the array / hash part / `__index`
        self.op(Op::MovR(0, 2));
        self.op(Op::ShlI(0, 3));
        self.op(Op::Sub(1, Src::V(0)));
        self.ld(
            6,
            W::B8,
            M {
                base: Base::Reg(1),
                off: -(table::ARRAY_PAYLOAD_STRIDE as i32),
            },
        );
        self.op(Op::Add(2, Src::Imm(1)));
        self.st_imm(W::B1, fr(res, 0), i32::from(LUA_VNUMINT));
        self.st(W::B8, fr(res, 8), 2);
        if c >= 2 {
            self.st(W::B1, fr(res + 1, 0), 5);
            self.st(W::B8, fr(res + 1, 8), 6);
        }
        for j in 2..c {
            self.set_const(res + j, LUA_VNIL, 0);
        }
        self.op(Op::Jmp(next));
        self.op(Op::Bind(slow));
    }

    /// `TFORPREP A Bx`: swap the control and closing slots (the closing value must be nil/false: nothing to
    /// mark as to-be-closed), then go to the `TFORCALL` at the end of the loop.
    fn t_tforprep(&mut self, pc: u32, i: Instruction) {
        let a = i.a();
        let x = self.exit_r(pc);
        let t = (i64::from(pc) + 1 + i64::from(i.bx())) as usize;
        let ok = self.p.label();
        self.ld(0, W::B1, fr(a + 3, 0));
        self.op(Op::MovR(1, 0));
        self.op(Op::And(1, Src::Imm(0x0F)));
        self.br(Cc::Eq, 1, Src::Imm(0), ok); // nil
        self.br(Cc::Ne, 0, Src::Imm(i32::from(LUA_VFALSE)), x);
        self.op(Op::Bind(ok));
        if LUA54 {
            // 5.4: the closing value stays in `R[A+3]` (nothing to mark as to-be-closed: nil/false); no swap
            let l = self.pcl[t];
            self.op(Op::Jmp(l));
            return;
        }
        // swap R[a+2] <-> R[a+3] (tag byte + payload)
        self.ld(0, W::B1, fr(a + 2, 0));
        self.ld(1, W::B8, fr(a + 2, 8));
        self.ld(2, W::B1, fr(a + 3, 0));
        self.ld(3, W::B8, fr(a + 3, 8));
        self.st(W::B1, fr(a + 2, 0), 2);
        self.st(W::B8, fr(a + 2, 8), 3);
        self.st(W::B1, fr(a + 3, 0), 0);
        self.st(W::B8, fr(a + 3, 8), 1);
        let l = self.pcl[t];
        self.op(Op::Jmp(l));
    }

    /// `TFORLOOP A Bx`: continue (jump back) while the first loop variable `R[A+3]` (5.4: `R[A+4]`, which is
    /// then copied to the control variable `R[A+2]`) is not nil.
    fn t_tforloop(&mut self, pc: u32, i: Instruction) {
        let a = i.a();
        let t = (i64::from(pc) + 1 - i64::from(i.bx())) as u32;
        let fall = self.pcl[pc as usize + 1];
        let first = if LUA54 { a + 4 } else { a + 3 };
        self.ld(0, W::B1, fr(first, 0));
        self.op(Op::And(0, Src::Imm(0x0F)));
        self.br(Cc::Eq, 0, Src::Imm(0), fall);
        if LUA54 {
            self.copy_value(fr(a + 2, 0), fr(a + 4, 0)); // save the control variable
        }
        self.poll(t);
        let l = self.pcl[t as usize];
        self.op(Op::Jmp(l));
    }
}
