//! J2a: helper-based templates (docs/JIT_DESIGN.md section 15): NEWTABLE, SETLIST, LEN, CONCAT, CLOSURE, POW/POWK
//! and the slow paths of the table accessors (GETTABLE/GETI/GETFIELD/GETTABUP/SELF and SETTABLE/SETI/SETFIELD/
//! SETTABUP).
//!
//! Every template calls an `extern "C"` helper of `rn_core::jit_helpers` that runs the interpreter's own code for
//! the instruction, with the J1c protocol: `ci.savedpc` = what the interpreter's `savepc!()` would store (the
//! address after the instruction and its `EXTRAARG`), `top` is set by the helper like the interpreter's
//! `Protect`/`check_gc!`, the status in `eax` is checked (`H_DONE` continue, `H_EXIT` leave at the instruction
//! before any side effect so the interpreter handles metamethods/errors/yields (E3), anything else = error exit),
//! and `rbx` is reloaded after every helper that can allocate (GC shrinks the stack, `check_stack` grows it).
//!
//! * The table accessors keep their inline probes (`templates_table`); their former "exit at the instruction"
//!   label is now an out-of-line stub that calls `jit_get`/`jit_set` and rejoins at the next instruction.
//! * After a helper that can run a GC step (finalizers are Lua code) `ci`-visible state may have changed, so the
//!   block-local type knowledge is dropped and a hook installed meanwhile is noticed (`exit at the next
//!   instruction with trap = 1`, refunding the pre-paid count of the rest of the block).

use crate::ir::{Base, Cc, Op, Src, L, M, W};
use crate::lower::{ciw, fr, thw, Cx, Kind, Ty};
use rn_core::jit_layout::table;
fn rm(v: u8, off: i32) -> M {
    M { base: Base::Reg(v), off }
}
const TAG_TABLE: i32 = rn_core::value::ctb(rn_core::value::LUA_VTABLE) as i32;
use rn_core::instruction::Instruction;
use rn_core::jit_helpers as h;
use rn_core::jit_hooks::{thread as thr, JitExit};
use rn_core::jit_layout::callinfo as ci;
use rn_core::value::{Value, LUA_VNUMFLT, LUA_VNUMINT};
use rn_core::ver::active::OpCode;

/// One helper invocation.
#[derive(Clone, Copy, Debug)]
pub(crate) struct HCall {
    /// Helper address.
    f: u64,
    /// Instruction word (third argument).
    instr: u32,
    /// `EXTRAARG` operand (fourth argument).
    extra: u32,
    /// Store `ci.savedpc = &code[saved]` before the call (`None`: the helper can neither error nor collect).
    saved: Option<u32>,
    /// The instruction after the whole template (`pc + 1`, or `pc + 2` with an `EXTRAARG`).
    next: u32,
    /// The helper can allocate: reload the frame, check hooks afterwards, forget register types.
    gc: bool,
    /// Where the out-of-line slow path continues at `next`, captured when the template is emitted (tier 1.5, B2:
    /// a region fallback remaps `pcl` only while the template runs). `None` = read `pcl[next]` when the cold
    /// code is emitted (plain tier 1, byte-identical to the code before tier 1.5).
    rejoin: Option<L>,
    /// BUNDLE-S S1: the helper may push the frame of a Lua `__index` / `__newindex` function (`H_LUA`): compiled code enters it and continues at `next` when it returns.
    mm: bool,
}

/// Out-of-line code of the helper templates, emitted after the main body of the function.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Cold {
    /// Slow path of a table accessor: call, dispatch, rejoin at `next`.
    Slow { l: L, pc: u32, call: HCall },
    /// T3: the first `__index` level of `GETFIELD` / `SELF` / `GETTABUP` (receiver table in `V0`, constant key `kidx`): the metatable's
    /// `__index` field, if a table, is probed for the key; a non-nil value rejoins at `back` with `V2` = its node, everything else
    /// (no metatable, `__index` absent or not a table) goes to the helper at `x`. A key that is absent (or nil) in an `__index` table
    /// without a metatable of its own is a final nil: `R[a] = nil` (and `R[a+1] = R[selfb]` for `SELF`, `selfb != u32::MAX`), continue at `fin`.
    Chain { l: L, back: L, x: L, fin: L, kidx: u32, a: u32, selfb: u32 },
    /// BUNDLE-K item B: `SELF` whose receiver register `b` is not a table: if it is a string, probe the string metatable's `__index` table
    /// (`CallEnv::str_mt`) for the constant key `kidx` inline, store `R[a+1] = R[b]`, `R[a] = method` and rejoin at `done`; anything else
    /// (not a string, no metatable, `__index` not a table, key absent or nil) goes to the helper at `x`.
    StrSelf { l: L, x: L, done: L, kidx: u32, a: u32, b: u32 },
    /// BUNDLE-A item 2: store to a node whose key exists but whose value is nil (`V0` = table, `V2` = node), rejoining at `back` (the store).
    SetNil { l: L, back: L, x: L, v: crate::templates_table::Vop },
    /// C2: out-of-line comparison of two non-numbers (`jit_cmp`): `H_TRUE` jumps to `lt`, `H_FALSE` to `lf`, `H_EXIT` leaves at `pc`.
    Cmp { l: L, pc: u32, instr: u32, lt: L, lf: L },
    /// BUNDLE-B: `RETURN0` / `RETURN1` of a metamethod frame (`CIST_MMRET`): `jit_mm_ret` finishes the caller's comparison when it is
    /// one that tier 1 started (else the instruction is left to the interpreter at `pc`).
    MmRet { l: L, pc: u32, instr: u32 },
    /// BUNDLE-D (`armm`): an arithmetic instruction at `pc` whose operands are not numbers (`instr` = its `MMBIN*` word, or the `UNM` / `BNOT` itself): the
    /// helper pushes the Lua metamethod frame (`savedpc` = `next`), compiled code enters it, and its return ([`Cold::MmRet`]) lands at `next` with the result
    /// stored in the destination register. `extra` = 1 for the `MMBIN*` the interpreter would count, `r` = the pre-paid instructions of the block after `next`
    /// (refunded while the metamethod runs, paid again at the continuation), `x` = the exit at `pc` refunding the pre-paid count plus `extra`.
    ArithMm { l: L, pc: u32, instr: u32, next: u32, extra: u32, r: u32, x: L },
    /// Non-`H_DONE` result in `V0`: `H_EXIT` leaves at the instruction, everything else is an error exit.
    Fail { l: L, pc: u32 },
    /// A hook appeared: leave at `next` with `trap = 1`, refunding `refund` pre-paid instructions.
    Hook { l: L, next: u32, refund: u32 },
}

impl Cold {
    /// The label a slow path rejoins at, when it was captured at template time (tier 1.5 region fallbacks).
    pub(crate) fn rejoin_label(&self) -> Option<L> {
        match self {
            Cold::Slow { call, .. } => call.rejoin,
            _ => None,
        }
    }
}

/// Classification of the opcodes this file implements (`None` = not ours).
pub(crate) fn kind(op: OpCode, i: Instruction, k: &[Value]) -> Option<Kind> {
    use OpCode as O;
    match op {
        O::Newtable | O::Setlist | O::Len | O::Concat | O::Closure | O::Pow => Some(Kind::Guard),
        O::Powk => k
            .get(i.c() as usize)
            .is_some_and(|v| matches!(v.tag(), LUA_VNUMINT | LUA_VNUMFLT))
            .then_some(Kind::Guard),
        _ => None,
    }
}

/// Does `op` (with operands `i`) own the following `EXTRAARG` word?
pub(crate) fn has_extra(op: OpCode, i: Instruction) -> bool {
    match op {
        OpCode::Newtable => true,
        OpCode::Setlist => i.k() != 0,
        _ => false,
    }
}

impl Cx<'_> {
    /// Out-of-line slow path of a table accessor at `pc`: returns the label to jump to instead of
    /// "exit at the instruction".
    pub(crate) fn slow_label(&mut self, pc: u32, i: Instruction, op: OpCode) -> L {
        let (f, gc) = match op {
            OpCode::Gettable
            | OpCode::Geti
            | OpCode::Getfield
            | OpCode::Gettabup
            | OpCode::SelfOp => (h::jit_get as *const () as usize as u64, false),
            OpCode::Setupval => (h::jit_setupval as *const () as usize as u64, false),
            _ => (h::jit_set as *const () as usize as u64, true),
        };
        let l = self.p.label();
        // BUNDLE-S S1 (`t1q` bits 2 / 3): reads and stores whose `__index` / `__newindex` is a Lua function continue in compiled code (not in the region fallbacks of
        // tier 1.5, whose slow paths rejoin through captured labels)
        let (mm_bits, mutants) = self.s1_bits();
        let mm = !self.capture_rejoin && if gc { mm_bits & 8 != 0 } else { mm_bits & 4 != 0 };
        let extra = if mm { (if gc { 2 } else { 1 }) | mutants } else { 0 };
        let saved = if mm && mutants & 0x400 != 0 { None } else if gc || mm { Some(pc + 1) } else { None };
        let call = HCall {
            f,
            instr: i.0,
            extra,
            saved,
            next: pc + 1,
            gc,
            rejoin: self.capture_rejoin.then(|| self.pcl[pc as usize + 1]),
            mm,
        };
        self.cold.push(Cold::Slow { l, pc, call });
        l
    }

    /// The `x` word of `jit_mm_ret` (and of `jit_tailcall` for the tail callee of a metamethod frame): bit 0 `armm`, bits 1 / 2 S1 reads / stores,
    /// bit 3 the general `RETURN` (`t1q` bit 6), bits 8 / 9 mutants.
    pub(crate) fn mm_ret_flags(&self) -> u32 {
        let (bits, _) = self.s1_bits();
        let w = self.cfg.calls.map_or(0, |e| e.t1q);
        u32::from(self.cfg.calls.is_some_and(|e| e.armm)) | bits >> 1 | (w >> 6 & 1) << 3 | (w >> 15 & 1) << 8 | (w >> 21 & 1) << 9 | (w >> 22 & 1) << 10 | (w >> 23 & 1) << 11
    }

    /// BUNDLE-S S1: `(t1q & 12, helper mutant bits)` of this lowering: 0x100 `s1_self_skip`, 0x200 `s1_obj_orig`, 0x400 `s1_savedpc_skip`.
    fn s1_bits(&self) -> (u32, u32) {
        let Some(env) = self.cfg.calls else {
            return (0, 0);
        };
        let w = env.t1q;
        (w & 12, (w >> 14 & 1) << 8 | (w >> 20 & 1) << 9 | (w >> 16 & 1) << 10)
    }

    /// C2: the label a register-register comparison (`LT` / `LE` / `EQ`) at `pc` jumps to for operands the inline number paths do
    /// not take: a call of `jit_cmp` that continues at `lt` / `lf` (the instruction's two targets) instead of leaving tier 1.
    pub(crate) fn cmp_slow_label(&mut self, pc: u32, i: Instruction, lt: L, lf: L) -> L {
        let l = self.p.label();
        self.cold.push(Cold::Cmp { l, pc, instr: i.0, lt, lf });
        l
    }

    /// BUNDLE-B: the label a `RETURN0` / `RETURN1` at `pc` jumps to when its frame is a metamethod frame (`x` = leave at the instruction).
    pub(crate) fn mm_ret_label(&mut self, pc: u32, x: L) -> L {
        let instr = self.code[pc as usize];
        let op = Instruction(instr).opcode();
        // (BUNDLE-S S1: the general `RETURN` too, when the frame may be the handler of a table read / store)
        let s1 = (self.s1_bits().0 != 0 || self.cfg.calls.is_some_and(|e| e.t1q & 64 != 0)) && op == Some(OpCode::Return);
        if !self.calls_on() || !(matches!(op, Some(OpCode::Return0 | OpCode::Return1)) || s1) {
            return x;
        }
        let l = self.p.label();
        self.cold.push(Cold::MmRet { l, pc, instr });
        l
    }

    /// BUNDLE-D (`armm`): the label an arithmetic instruction at `pc` jumps to for operands that are not numbers instead of `x` (the exit at `pc`): the
    /// metamethod of a table operand is pushed as an in-place frame and the instruction finishes when it returns (`x` still when the instruction has no
    /// `MMBIN*` follower, the code is not lowered with calls or the limit is off).
    pub(crate) fn arith_mm_label(&mut self, pc: u32, x: L) -> L {
        let Some(env) = self.cfg.calls else {
            return x;
        };
        #[cfg(feature = "trace")]
        let never = crate::tr::armm_mut() & 8 != 0;
        #[cfg(not(feature = "trace"))]
        let never = false;
        if !env.armm || never {
            return x;
        }
        let op = Instruction(self.code[pc as usize]).opcode();
        let (instr, next, extra) = match op {
            Some(OpCode::Unm | OpCode::Bnot) => (self.code[pc as usize], pc + 1, 0),
            Some(_) if self.code.get(pc as usize + 1).is_some_and(|&w| matches!(Instruction(w).opcode(), Some(OpCode::Mmbin | OpCode::Mmbini | OpCode::Mmbink))) => {
                (self.code[pc as usize + 1], pc + 2, 1)
            }
            _ => return x,
        };
        if next as usize >= self.code.len() || !self.armm_next[next as usize] {
            return x;
        }
        let r = if self.is_head[next as usize] { 0 } else { self.rem[next as usize] };
        let l = self.p.label();
        let xe = self.exit_rx(pc, extra);
        self.cold.push(Cold::ArithMm { l, pc, instr, next, extra, r, x: xe });
        l
    }

    /// The templates of NEWTABLE, SETLIST, LEN, CONCAT, CLOSURE and POW/POWK.
    pub(crate) fn t_alloc(&mut self, pc: u32, i: Instruction, op: OpCode) {
        use OpCode as O;
        let a = i.a() as usize;
        let extra = if has_extra(op, i) {
            Instruction(self.code[pc as usize + 1]).ax()
        } else {
            0
        };
        let next = pc + 1 + u32::from(has_extra(op, i));
        let (f, gc, save) = match op {
            O::Newtable => (h::jit_newtable as *const () as usize as u64, true, true),
            O::Setlist => (h::jit_setlist as *const () as usize as u64, true, true),
            O::Len => (h::jit_len as *const () as usize as u64, false, false),
            O::Concat => (h::jit_concat as *const () as usize as u64, true, true),
            O::Closure => (h::jit_closure as *const () as usize as u64, true, true),
            _ => (h::jit_pow as *const () as usize as u64, false, false),
        };
        let call = HCall {
            f,
            instr: i.0,
            extra,
            saved: save.then_some(next),
            next,
            gc,
            rejoin: None,
            mm: false,
        };
        // `#t` of a table: the length hint is the answer in the common states (see `len_inline`); everything else is the helper
        let len_done = (op == O::Len && !rn_core::jit_layout::LUA54).then(|| self.p.label());
        if let Some(done) = len_done {
            self.len_inline(i, done);
        }
        self.emit_hcall(pc, &call, None);
        if let Some(done) = len_done {
            self.op(Op::Bind(done));
        }
        match op {
            O::Setlist => {}
            O::Len => self.ty[a] = Ty::Int,
            O::Pow | O::Powk => self.ty[a] = Ty::Flt,
            _ => self.ty[a] = Ty::Unk,
        }
        if gc {
            // finalizers (Lua code) may have run: nothing is known about the registers any more
            self.ty = [Ty::Unk; 256];
        }
    }

    /// The inline form of `LEN` of a table without a metatable (`TableRef::len`, 5.5): the length hint `h` (`u32` at `array + 0`) is the
    /// border when `h >= 1`, `arr[h-1]` is non-nil and either `h < asize` and `arr[h]` is nil, or `h == asize` and the hash part is the
    /// dummy node; a table with `asize == 0` and a dummy hash part has length 0. The helper's one-step hint moves for a push or a pop are
    /// replicated, store included: `arr[h]` non-nil, `h + 1 < asize`, `arr[h+1]` nil: hint and answer `h + 1`; `arr[h-1]` nil, `h >= 2`,
    /// `arr[h-2]` non-nil: hint and answer `h - 1`. Every other state (not a table, a metatable, hint 0, any further search, a populated hash
    /// part) falls through to the `jit_len` call, so the other hint updates and `__len` stay the helper's. Jumps to `done` with `R[A]`
    /// written. Clobbers V0..V4.
    fn len_inline(&mut self, i: Instruction, done: L) {
        #[cfg(feature = "trace")]
        let (m_prev, m_next, m_dummy, m_mt, m_hint, m_zero) = crate::tr::tlen_mut();
        #[cfg(not(feature = "trace"))]
        let (m_prev, m_next, m_dummy, m_mt, m_hint, m_zero) = (false, false, false, false, false, false);
        let (a, b) = (i.a(), i.b());
        let slow = self.p.label();
        let (full, empty, store, pop, push) = (self.p.label(), self.p.label(), self.p.label(), self.p.label(), self.p.label());
        let (zero, pscan, pend, pnil) = (self.p.label(), self.p.label(), self.p.label(), self.p.label());
        // V4 = type nibble of array slot `V3 + k` (0 = nil)
        let tag = |c: &mut Self, k: i32| {
            c.op(Op::MovR(4, 2));
            c.op(Op::Add(4, Src::V(3)));
            c.ld(4, W::B1, rm(4, table::ARRAY_TAG_BASE as i32 + k));
            c.op(Op::And(4, Src::Imm(0x0F)));
        };
        self.ld(0, W::B1, fr(b, 0));
        self.br(Cc::Ne, 0, Src::Imm(TAG_TABLE), slow);
        self.ld(0, W::B8, fr(b, 8));
        if !m_mt {
            self.ld(4, W::B8, rm(0, table::METATABLE as i32));
            self.br(Cc::Ne, 4, Src::Imm(0), slow);
        }
        self.ld(1, W::B4, rm(0, table::ASIZE as i32));
        self.br(Cc::Eq, 1, Src::Imm(0), empty);
        self.ld(2, W::B8, rm(0, table::ARRAY as i32));
        self.ld(3, W::B4, rm(2, table::LENHINT as i32));
        self.br(Cc::Eq, 3, Src::Imm(0), zero);
        self.br(Cc::Gt, 3, Src::V(1), slow);
        if !m_prev {
            tag(self, -1);
            self.br(Cc::Eq, 4, Src::Imm(0), pop);
        }
        self.br(Cc::Eq, 3, Src::V(1), full);
        if !m_next {
            tag(self, 0);
            self.br(Cc::Ne, 4, Src::Imm(0), push);
        }
        self.op(Op::Jmp(store));
        for (l, zero) in [(full, false), (empty, true)] {
            self.op(Op::Bind(l));
            if !m_dummy {
                self.ld(4, W::B1, rm(0, table::FLAGS as i32));
                self.op(Op::And(4, Src::Imm(i32::from(table::BITDUMMY))));
                self.br(Cc::Eq, 4, Src::Imm(0), slow);
            }
            if zero {
                self.op(Op::Mov(3, 0));
            }
            self.op(Op::Jmp(store));
        }
        // hint 0 (constructor tables: `asize / 2` is the initial hint, 0 for one slot): slot 0 nil is a binary search (helper); otherwise
        // the upward scan below, which finishes inside the helper's four steps when `asize <= 5`
        self.op(Op::Bind(zero));
        if !m_zero {
            self.br(Cc::Gt, 1, Src::Imm(5), slow);
        }
        self.op(Op::Mov(3, 1));
        tag(self, -1);
        self.br(Cc::Eq, 4, Src::Imm(0), slow);
        self.op(Op::Jmp(pscan));
        // pop: arr[h-1] nil, arr[h-2] non-nil -> hint = answer = h - 1
        self.op(Op::Bind(pop));
        self.br(Cc::Lt, 3, Src::Imm(2), slow);
        tag(self, -2);
        self.br(Cc::Eq, 4, Src::Imm(0), slow);
        self.op(Op::Sub(3, Src::Imm(1)));
        if !m_hint {
            self.st(W::B4, rm(2, table::LENHINT as i32), 3);
        }
        self.op(Op::Jmp(store));
        // push (h < asize, arr[h] non-nil): the helper's upward scan, one slot per step, at most four steps. Its second step (slot h+1) is
        // the common case (`t[#t + 1] = v` leaves the hint one behind): a nil there, or the array's end, is answered at once; a longer
        // scan is exact when `asize - h <= 4`; a nil slot `r` ends it with hint and answer `r`, the end of the array with hint `asize`
        // (the answer for a dummy hash part)
        self.op(Op::Bind(push));
        self.op(Op::Add(3, Src::Imm(1)));
        self.br(Cc::Eq, 3, Src::V(1), pend);
        tag(self, 0);
        self.br(Cc::Eq, 4, Src::Imm(0), pnil);
        if !m_zero {
            self.op(Op::MovR(4, 1));
            self.op(Op::Sub(4, Src::V(3)));
            self.br(Cc::Gt, 4, Src::Imm(3), slow);
        }
        self.op(Op::Add(3, Src::Imm(1)));
        self.op(Op::Bind(pscan));
        self.br(Cc::Eq, 3, Src::V(1), pend);
        tag(self, 0);
        self.br(Cc::Eq, 4, Src::Imm(0), pnil);
        self.op(Op::Add(3, Src::Imm(1)));
        self.op(Op::Jmp(pscan));
        self.op(Op::Bind(pend));
        self.ld(4, W::B1, rm(0, table::FLAGS as i32));
        self.op(Op::And(4, Src::Imm(i32::from(table::BITDUMMY))));
        self.br(Cc::Eq, 4, Src::Imm(0), slow);
        self.op(Op::Bind(pnil));
        if !m_hint {
            self.st(W::B4, rm(2, table::LENHINT as i32), 3);
        }
        self.op(Op::Bind(store));
        self.st_imm(W::B1, fr(a, 0), LUA_VNUMINT as i32);
        self.st(W::B8, fr(a, 8), 3);
        self.op(Op::Jmp(done));
        self.op(Op::Bind(slow));
    }

    /// Call sequence: `savedpc`, `CallH`, status dispatch, frame reload and hook poll.
    fn emit_hcall(&mut self, pc: u32, c: &HCall, lua: Option<L>) {
        let env = self.cfg.calls.expect("calls enabled");
        if let Some(s) = c.saved {
            let addr = self.cfg.code_addr as u64 + 4 * u64::from(s);
            self.op(Op::Mov(0, addr));
            self.st(W::B8, ciw(ci::SAVEDPC), 0);
        }
        self.op(Op::CallH {
            f: c.f,
            a: c.instr,
            b: c.extra,
        });
        if let Some(pushed) = lua {
            self.br(Cc::Eq, 0, Src::Imm(h::H_LUA as i32), pushed);
        }
        let fail = self.p.label();
        self.cold.push(Cold::Fail { l: fail, pc });
        self.br(Cc::Ne, 0, Src::Imm(h::H_DONE as i32), fail);
        if c.gc {
            self.op(Op::ReloadFrame {
                stack_off: env.stack_off,
            });
            // a finalizer may have installed a hook: the interpreter takes over at the next instruction
            let hook = self.p.label();
            let refund = self.rem[pc as usize].saturating_sub(1);
            self.cold.push(Cold::Hook {
                l: hook,
                next: c.next,
                refund,
            });
            self.ld(0, W::B1, thw(thr::HOOKMASK));
            self.br(Cc::Ne, 0, Src::Imm(0), hook);
        }
    }

    /// BUNDLE-S S1: the tail of a table read / store whose Lua metamethod frame the helper pushed (`pushed`, `next` = the instruction after it): the rest of the
    /// block was pre-paid and is given back while the metamethod runs, compiled code enters the frame, and the return (`Cold::MmRet`, `jit_mm_pop` stored the result in
    /// `R[A]`) continues at the resume entry of `next`, which pays the rest of the block again and lands on the tag-checking label of `next` (as `Cold::ArithMm`).
    fn emit_s1_push(&mut self, next: u32, pushed: L) {
        let m = self.cfg.calls.map_or(0, |e| e.t1q);
        let r = if self.is_head[next as usize] { 0 } else { self.rem[next as usize] };
        let enter = self.l_enter();
        self.op(Op::Bind(pushed));
        if r > 0 && m & (1 << 17) == 0 {
            let cm = M { base: Base::Cnt, off: 0 };
            self.ld(1, W::B4, cm);
            self.op(Op::Add(1, Src::Imm(r as i32)));
            self.st(W::B4, cm, 1);
        }
        self.op(Op::Jmp(enter));
        let cont = self.p.label();
        self.op(Op::Bind(cont));
        if r > 0 && m & (1 << 18) == 0 {
            let xr = self.exit_ref_pub(next, r);
            self.op(Op::CountSub { n: r, exit: xr });
        }
        let target = if m & (1 << 19) != 0 { self.pcl[next as usize] } else { self.land_of.get(&next).copied().unwrap_or(self.pcl[next as usize]) };
        self.op(Op::Jmp(target));
        self.s1_resume.push((next, cont));
    }

    /// Emits the out-of-line code collected in `self.cold` (before the lazily created exit sequences).
    pub(crate) fn emit_cold(&mut self) {
        let mut idx = 0;
        // (emitting a Slow stub pushes its own Fail/Hook entries)
        while idx < self.cold.len() {
            let c = self.cold[idx];
            idx += 1;
            match c {
                Cold::Slow { l, pc, call } => {
                    self.op(Op::Bind(l));
                    let pushed = call.mm.then(|| self.p.label());
                    self.emit_hcall(pc, &call, pushed);
                    let t = call.rejoin.unwrap_or(self.pcl[call.next as usize]);
                    self.op(Op::Jmp(t));
                    if let Some(pushed) = pushed {
                        self.emit_s1_push(call.next, pushed);
                    }
                }
                Cold::Chain { l, back, x, fin, kidx, a, selfb } => self.emit_chain(l, back, x, fin, kidx, a, selfb),
                Cold::StrSelf { l, x, done, kidx, a, b } => self.emit_str_self(l, x, done, kidx, a, b),
                Cold::SetNil { l, back, x, v } => self.emit_set_nil(l, back, x, v),
                Cold::Cmp { l, pc, instr, lt, lf } => {
                    self.op(Op::Bind(l));
                    // (mutant `c2cmp_swap`: the two outcomes are exchanged; `c2cmp_exit_skip`: an instruction the helper leaves to the
                    // interpreter counts as "false")
                    #[cfg(feature = "trace")]
                    let (swap, skip) = crate::tr::c2cmp_mut();
                    #[cfg(not(feature = "trace"))]
                    let (swap, skip) = (false, false);
                    // BUNDLE-B: the helper answers strings, raw-equal values and tables without `__eq` itself, and pushes the frame of a
                    // Lua `__lt` / `__le` / `__eq` (`savedpc` = the instruction after the comparison, as the interpreter has it); compiled
                    // code enters that frame and its `RETURN` continues at `cont` below. Everything else leaves at the instruction.
                    let x = self.exit_r(pc);
                    let not_str = if skip { lf } else { x };
                    #[cfg(feature = "trace")]
                    let no_save = crate::tr::mmcmp_savedpc_skip();
                    #[cfg(not(feature = "trace"))]
                    let no_save = false;
                    if !no_save {
                        self.op(Op::Mov(0, self.pc_addr(pc + 1)));
                        self.st(W::B8, ciw(ci::SAVEDPC), 0);
                    }
                    self.op(Op::CallH {
                        f: h::jit_cmp as *const () as usize as u64,
                        a: instr,
                        b: u32::from(self.env().t1q),
                    });
                    let (lt, lf) = if swap { (lf, lt) } else { (lt, lf) };
                    self.br(Cc::Eq, 0, Src::Imm(h::H_TRUE as i32), lt);
                    self.br(Cc::Eq, 0, Src::Imm(h::H_FALSE as i32), lf);
                    let enter = self.l_enter();
                    self.br(Cc::Eq, 0, Src::Imm(h::H_LUA as i32), enter);
                    let err = self.l_exit(1);
                    self.br(Cc::Eq, 0, Src::Imm(h::H_EXIT as i32), not_str);
                    self.op(Op::Jmp(err));
                    // the continuation after the metamethod returned: the result is the boolean `jit_mm_ret` stored at `ci.top`
                    // (r13 = the caller again)
                    let cont = self.p.label();
                    self.op(Op::Bind(cont));
                    self.ld(0, W::B8, ciw(ci::TOP));
                    self.op(Op::Shl(0, 4));
                    self.ld(1, W::B8, thw(self.env().stack_off as usize));
                    self.op(Op::Add(1, Src::V(0)));
                    self.ld(2, W::B1, M { base: Base::Reg(1), off: 0 });
                    // (the interpreter fetches the `JMP` as an instruction of its own after a metamethod comparison, whose condition holds:
                    // count it here, and leave at the `JMP` when the budget is out; the inline `donextjump` is not counted)
                    let k = Instruction(instr).k() != 0;
                    let lt_true = self.p.label();
                    self.br(Cc::Eq, 2, Src::Imm(i32::from(rn_core::value::LUA_VTRUE)), lt_true);
                    if !k {
                        let xj = self.exit_cnt(pc + 1, 1);
                        self.op(Op::CountSub { n: 1, exit: xj });
                    }
                    self.op(Op::Jmp(lf));
                    self.op(Op::Bind(lt_true));
                    if k {
                        let xj = self.exit_cnt(pc + 1, 1);
                        self.op(Op::CountSub { n: 1, exit: xj });
                    }
                    self.op(Op::Jmp(lt));
                    self.mm_resume.push((pc + 1, cont));
                }
                Cold::ArithMm { l, pc, instr, next, extra, r, x } => {
                    #[cfg(feature = "trace")]
                    let mm = crate::tr::armm_mut();
                    #[cfg(not(feature = "trace"))]
                    let mm = 0u8;
                    self.op(Op::Bind(l));
                    // the `MMBIN*` the interpreter counts (nothing of the instruction has been done yet: the exit at `pc` redoes it)
                    if extra != 0 && mm & 1 == 0 {
                        let xc = self.exit_ref_pub(pc, self.rem[pc as usize] + extra);
                        self.op(Op::CountSub { n: extra, exit: xc });
                    }
                    self.op(Op::Mov(0, self.pc_addr(next)));
                    self.st(W::B8, ciw(ci::SAVEDPC), 0);
                    self.op(Op::CallH {
                        f: h::jit_arith_mm as *const () as usize as u64,
                        a: instr,
                        b: 0,
                    });
                    let enter = self.l_enter();
                    let pushed = self.p.label();
                    self.br(Cc::Eq, 0, Src::Imm(h::H_LUA as i32), pushed);
                    self.op(Op::Jmp(x));
                    self.op(Op::Bind(pushed));
                    // the rest of the block was pre-paid: give it back while the metamethod runs (the continuation pays it again,
                    // whoever pushed the frame: the interpreter counts what it executed)
                    if r > 0 && mm & 4 == 0 {
                        let m = M { base: Base::Cnt, off: 0 };
                        self.ld(1, W::B4, m);
                        self.op(Op::Add(1, Src::Imm(r as i32)));
                        self.st(W::B4, m, 1);
                    }
                    self.op(Op::Jmp(enter));
                    // the continuation after the metamethod returned (`jit_mm_ret` stored the result in the destination register; r13 = the caller)
                    let cont = self.p.label();
                    self.op(Op::Bind(cont));
                    if r > 0 && mm & 4 == 0 {
                        let xr = self.exit_ref_pub(next, r);
                        self.op(Op::CountSub { n: r, exit: xr });
                    }
                    let target = if mm & 2 != 0 { self.pcl[next as usize] } else { self.land_of.get(&next).copied().unwrap_or(self.pcl[next as usize]) };
                    self.op(Op::Jmp(target));
                    // (BUNDLE-V: with `t1q` bit 4 the continuation is an entry only `ret_mm` uses, as for the table reads / stores of BUNDLE-S)
                    if self.cfg.calls.is_some_and(|e| e.t1q & 16 != 0) {
                        self.s1_resume.push((next, cont));
                    } else {
                        self.mm_resume.push((next, cont));
                    }
                }
                Cold::MmRet { l, pc, instr } => {
                    self.op(Op::Bind(l));
                    let xc = self.exit_cnt(pc, 1);
                    self.op(Op::CountSub { n: 1, exit: xc });
                    self.op(Op::CallH {
                        f: h::jit_mm_ret as *const () as usize as u64,
                        a: instr,
                        b: self.mm_ret_flags(),
                    });
                    let done = self.l_ret_mm();
                    self.br(Cc::Eq, 0, Src::Imm(h::H_DONE as i32), done);
                    self.op(Op::Jmp(xc));
                }
                Cold::Fail { l, pc } => {
                    let err = self.l_exit(1);
                    self.op(Op::Bind(l));
                    let x = self.exit_r(pc);
                    self.br(Cc::Eq, 0, Src::Imm(h::H_EXIT as i32), x);
                    self.op(Op::Jmp(err));
                }
                Cold::Hook { l, next, refund } => {
                    self.op(Op::Bind(l));
                    let addr = self.cfg.code_addr as u64 + 4 * u64::from(next);
                    self.op(Op::Mov(0, addr));
                    self.st(W::B8, ciw(ci::SAVEDPC), 0);
                    self.st_imm(W::B4, ciw(ci::TRAP), 1);
                    if refund > 0 {
                        let m = M {
                            base: Base::Cnt,
                            off: 0,
                        };
                        self.ld(1, W::B4, m);
                        self.op(Op::Add(1, Src::Imm(refund as i32)));
                        self.st(W::B4, m, 1);
                    }
                    self.op(Op::Ret(JitExit::Continue as u32));
                }
            }
        }
        self.cold.clear();
    }
}
