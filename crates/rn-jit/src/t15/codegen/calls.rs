//! Calls in regions (task T6; docs/JIT_TIER15.md section 20): inline `math` builtins and inline upvalue loads.
//!
//! # Intrinsic calls (`Act::Intr`, `Act::IntrH`)
//!
//! A `CALL` of a registered `math` function ([`crate::t15::intrinsics`]) whose argument facts are `INT`/`FLT`
//! singletons is executed inline. The inline form must be indistinguishable from what tier 1 does around a light C
//! function call, so *every* condition under which the interpreter or tier 1 does something else is a guard that
//! leaves through the **slow path** = the tier-1 `CALL` template of the same instruction (`Rg::emit_fall`, entered with
//! the register-resident set of the moment all operand registers were loaded; it continues in `stub(pc+1)`):
//!
//! * no hook (`thread.hookmask == 0`: call hooks, count hooks, line hooks - the interpreter must run the call) and no
//!   pending interrupt (tier 1's `CALL` guards);
//! * the callee slot holds the light C function `==` the registered address (tag `LCF` and payload; the slot is
//!   never register-resident, so its memory is exact);
//! * the conditions of `pre_call_native_fast` / `precall_c`: `stack_size - top > LUA_MINSTACK` (else the stack grows,
//!   which moves it and is observable through memory accounting) and a spare `CallInfo` (`ci.next != 0`, else
//!   `extend_ci` allocates);
//! * the result is representable (`floor`/`ceil` of a float whose result is not an `i64` call the real function:
//!   it returns a float).
//!
//! After the guards: the instruction count (`CountSub{1}` of tier 1's `CALL`, M4; the exit is the budget exit of the
//! instruction, no refund, like tier 1's), then `ci.savedpc = &code[pc+1]`, the result, and `thread.top = func + a + 2`
//! (what `pre_call_native_fast` leaves for one wanted result). The deliberate divergences (docs section 20.3): the
//! interpreter's native frame is not pushed (no observer exists while no Lua code and no hook runs), and the stale
//! copy that the C function's `push` leaves in slot `A+B` is not written (it is a dead temporary).
//!
//! `sin cos exp log` call libm through [`jit_t15_math`] (the same `f64::sin` ... the interpreter uses, so results are
//! bit-identical). They cannot run in registers (the call clobbers all of them): the guards are the same, then the
//! register-resident set is flushed, the helper reads the argument from its slot and writes the result slot, and
//! control continues in `stub(pc+1)` exactly like a fallback (`Act::IntrH` is a `Fall` for the register dataflow).
//!
//! # Upvalue loads (`Act::GetUp`)
//!
//! `GETUPVAL` copies `*closure.upvals[b].v` into the slot without a flush. The value is read from memory, so the
//! stack slots that an open upvalue of this very frame may alias (`debug.upvaluejoin`; `RegionPlan::escaped`) are
//! written back first (`spill`, the registers stay valid). A speculated result type is compared (mismatch: slow
//! path = the tier-1 template).

use super::{Act, Rg, S0, S1, S2, TAG_INT};
use crate::ir::{Base, Cc, FOp, Op, Src, L, M, W, X};
use crate::lower::{ciw, fr, thw};
use crate::t15::intrinsics::{self, Intr};
use crate::t15::{Disp, InsClass, TagSet};
use rn_core::instruction::Instruction;
use rn_core::jit_hooks::{self, intr as iid, thread as thr};
use rn_core::jit_layout::{callinfo as ci, lclosure, upval};
use rn_core::state::{CallInfo, Ci, StkId, Thread};
use rn_core::value::{Value, LUA_VLCF, LUA_VNUMFLT};
use rn_core::ver::active::OpCode;

const TAG_FLT: i32 = LUA_VNUMFLT as i32;
const TAG_LCF: i32 = LUA_VLCF as i32;
/// `LUA_MINSTACK`: free slots `precall_c` requires.
const MINSTACK: i32 = rn_core::state::LUA_MINSTACK as i32;

/// Class of an intrinsic's argument / result value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::t15) enum IK {
    Int,
    Flt,
}

impl IK {
    fn of(t: TagSet) -> IK {
        if t == TagSet::INT {
            IK::Int
        } else {
            debug_assert!(t == TagSet::FLT);
            IK::Flt
        }
    }
}

/// How the loaded value of a `GETUPVAL` is committed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::t15) enum URes {
    Int,
    Flt,
    /// Copied with its tag; `Some(tag)` compares the tag byte first.
    Mem(Option<u8>),
}

/// `jit_t15_math(thread, ci, slot | kind << 8, _)`: `R[slot] = f(R[slot+1])` for the libm intrinsics, `top = R[slot]
/// + 1`. Called after a flush with the guards of the inline form satisfied (a number argument, no hook).
///
/// # Safety
/// Called only from compiled code with the running thread and its current Lua frame.
pub unsafe extern "C" fn jit_t15_math(th: *mut Thread, ci: *mut CallInfo, a: u32, _b: u32) -> u32 {
    // SAFETY: `th` is the live thread pinned in `r12`, `ci` its current frame (`r13`).
    let (mut st, ci) = unsafe { (jit_hooks::state_from_thread(th.cast()), Ci::from_raw(ci)) };
    let (slot, kind) = ((a & 0xFF) as usize, ((a >> 8) & 0xFF) as u8);
    let base = StkId(ci.r().func.0 + 1);
    let v = st.get(base + (slot + 1));
    let x = match v.as_int() {
        Some(i) => i as f64,
        None => v.as_float().unwrap_or(f64::NAN),
    };
    let r = match kind {
        iid::SIN => x.sin(),
        iid::COS => x.cos(),
        iid::EXP => x.exp(),
        _ => x.ln(),
    };
    st.set(base + slot, Value::float(r));
    st.set_top(base + (slot + 1));
    0
}

impl Rg<'_, '_> {
    /// Is the act one of T6's (the register dataflow treats them like the inline table accesses: the slow path
    /// continues in the stub of the next instruction)?
    pub(super) fn is_call_act(a: &Act) -> bool {
        matches!(a, Act::Intr { .. } | Act::GetUp { .. })
    }

    /// The inline form of the `CALL` / `GETUPVAL` at `pc`, if the facts allow it.
    pub(super) fn classify_call(&self, pc: u32, op: OpCode, i: Instruction) -> Option<Act> {
        let insn = self.plan.insn(pc)?;
        if insn.disp == Disp::Dead {
            return None;
        }
        let a = i.a();
        match op {
            OpCode::Call
                if insn.class == InsClass::Intr
                    && insn.disp == Disp::Specialised
                    && !self.opt.no_intr =>
            {
                let kind = self.plan.intr.iter().find(|&&(p, _)| p == pc)?.1;
                if i.c() != 2 || i.b() != kind.nargs() + 1 {
                    return None;
                }
                let args: Vec<TagSet> = (1..=kind.nargs())
                    .map(|k| self.plan.fact(pc, a + k))
                    .collect();
                let res =
                    intrinsics::result_set(kind, &args, insn.guard_def.unwrap_or(TagSet::ANY))?;
                let addr = jit_hooks::intrinsic_addr(kind.index());
                if addr == 0 {
                    return None;
                }
                let (ac, rc) = (IK::of(args[0]), IK::of(res));
                if kind.is_libm() {
                    Some(Act::IntrH { kind, a })
                } else {
                    Some(Act::Intr {
                        kind,
                        a,
                        ac,
                        rc,
                        addr,
                    })
                }
            }
            OpCode::Getupval if !self.opt.no_hoist => {
                let res = match insn.guard_def {
                    Some(TagSet::INT) => URes::Int,
                    Some(TagSet::FLT) => URes::Flt,
                    Some(t) if t.is_singleton() => URes::Mem(t.tag_byte()),
                    _ => URes::Mem(None),
                };
                Some(Act::GetUp { a, b: i.b(), res })
            }
            _ => None,
        }
    }

    /// The guards common to every intrinsic call (see the module docs); every failure jumps to `slow`.
    fn intr_guards(&mut self, a: u32, nargs: u32, addr: usize, slow: L, slow_c: L) {
        if !crate::t15::mutants::active("intr_no_hook_check") {
            self.cx.op(Op::BrM {
                cc: Cc::Ne,
                w: W::B1,
                m: thw(thr::HOOKMASK),
                imm: 0,
                to: slow,
            });
        }
        self.cx.op(Op::Mov(S0, self.cx.cfg.interrupt_addr as u64));
        self.cx.op(Op::BrM {
            cc: Cc::Ne,
            w: W::B1,
            m: M {
                base: Base::Reg(S0),
                off: 0,
            },
            imm: 0,
            to: slow,
        });
        // the callee is the registered light C function (tag, then payload)
        if !crate::t15::mutants::active("intr_no_identity") {
            self.cx.op(Op::BrM {
                cc: Cc::Ne,
                w: W::B1,
                m: fr(a, 0),
                imm: TAG_LCF,
                to: slow_c,
            });
            let want = if crate::t15::mutants::active("intr_wrong_identity") {
                addr + 1
            } else {
                addr
            };
            self.cx.op(Op::Mov(S0, want as u64));
            self.cx.ld(S1, W::B8, fr(a, 8));
            self.cx.br(Cc::Ne, S1, Src::V(S0), slow_c);
        }
        // `precall_c`: LUA_MINSTACK free slots above `top = func + a + b + 1`, and a spare CallInfo
        self.cx.ld(S0, W::B8, ciw(ci::FUNC));
        self.cx.ld(S1, W::B8, thw(thr::STACK_SIZE));
        self.cx.op(Op::Sub(S1, Src::V(S0)));
        let need = (a + nargs + 1 + 1) as i32 + MINSTACK;
        if !crate::t15::mutants::active("intr_no_stack_check") {
            self.cx.br(Cc::Le, S1, Src::Imm(need), slow_c);
        }
        if !crate::t15::mutants::active("intr_no_ci_check") {
            self.cx.op(Op::BrM {
                cc: Cc::Eq,
                w: W::B8,
                m: ciw(ci::NEXT),
                imm: 0,
                to: slow_c,
            });
        }
    }

    /// The instruction count of `CALL` (`CountSub{1}` after the guards, M4); the exit is the instruction's budget
    /// exit (flush, then tier 1's `exit_at(pc)`: no refund).
    fn intr_count(&mut self, pc: u32) {
        if crate::t15::mutants::active("intr_no_count") {
            return;
        }
        let x = self.cexit(pc, 1);
        self.cx.op(Op::Sub(5, Src::Imm(1)));
        self.cx.br(Cc::Lt, 5, Src::Imm(0), x);
    }

    /// `ci.savedpc = &code[pc + 1]` (tier 1's `CALL` stores it before the call and leaves it there).
    fn intr_savedpc(&mut self, pc: u32) {
        self.cx.op(Op::Mov(S0, self.cx.pc_addr(pc + 1)));
        self.cx.st(W::B8, ciw(ci::SAVEDPC), S0);
    }

    /// `ci.savedpc`, then `thread.top = func + a + 2` (the state `pre_call_native_fast` leaves for one wanted result).
    fn intr_post(&mut self, pc: u32, a: u32) {
        self.intr_savedpc(pc);
        self.cx.ld(S0, W::B8, ciw(ci::FUNC));
        self.cx.op(Op::Add(S0, Src::Imm(a as i32 + 2)));
        self.cx.st(W::B8, thw(thr::TOP), S0);
    }

    /// Loads the register of an integer / float argument (registers only; memory operands are read after the guards,
    /// which use the scratch registers).
    fn intr_prefetch(&mut self, s: u32, k: IK) -> Option<u8> {
        match k {
            IK::Int => self.asg.of(s).map(|_| self.src_reg(s, S0)),
            IK::Flt => self.asg.xof(s).map(|_| self.fx(super::FOpd::Flt(s), 0)),
        }
    }

    /// Demotion feedback (T7): the slow path of an inline intrinsic (callee identity mismatch, stack or `CallInfo`
    /// shortage) is an ordinary `CALL` plus the failed guards. It decrements the nest's failure countdown; when that
    /// fires, the policy helper demotes the site to an ordinary `CALL` (it recompiles without the intrinsic) and the
    /// activation continues in the tier-1 `CALL` of the new code. State is flushed first (exactly what `emit_fall`
    /// would do) so the helper sees the exact frame; nothing of the instruction has been executed or counted yet.
    pub(super) fn slow_path_counted(&mut self, pc: u32) -> L {
        let l = self.slow_path(pc);
        self.slow_counted.push(l);
        l
    }

    pub(super) fn emit_intr_slow_count(&mut self, pc: u32) {
        let Some(fb) = self.fb else { return };
        self.flush_stores(self.rr);
        if !crate::t15::mutants::active("fall_no_budget") {
            self.store_budget();
        }
        self.rr = crate::t15::SlotSet::EMPTY;
        let go = self.cx.p.label();
        let ctr = M {
            base: Base::Reg(7),
            off: 0,
        };
        self.cx.op(Op::Mov(7, fb.fail_addr as u64));
        self.cx.ld(6, W::B4, ctr);
        if !crate::t15::mutants::active("pol_no_fail_count") {
            self.cx.op(Op::Sub(6, Src::Imm(1)));
            self.cx.st(W::B4, ctr, 6);
        }
        self.cx.op(Op::BrM {
            cc: Cc::Ge,
            w: W::B4,
            m: ctr,
            imm: 0,
            to: go,
        });
        self.cx.op(Op::CallH {
            f: fb.event_fn,
            a: fb.nest | (u32::from(fb.ver) << 24),
            b: (crate::t15::runtime::EventKind::StubFail as u32) << 16 | pc,
        });
        if let Some(env) = self.cx.cfg.calls {
            self.cx.op(Op::ReloadFrame {
                stack_off: env.stack_off,
            });
        }
        self.cx.br(Cc::Eq, 0, Src::Imm(0), go);
        self.cx.op(Op::Mov(1, fb.cont_addr as u64));
        self.cx.ld(
            1,
            W::B8,
            M {
                base: Base::Reg(1),
                off: 0,
            },
        );
        self.cx.op(Op::JmpR(1));
        self.cx.op(Op::Bind(go));
        // (the helper clobbered `r11`: on the stay path the budget is whatever the pad stored to `[r15]`)
        if !crate::t15::mutants::active("pol_no_budget_reload_stay") {
            self.load_budget();
        }
    }

    pub(super) fn emit_intr(&mut self, pc: u32, act: Act) {
        match act {
            Act::Intr {
                kind,
                a,
                ac,
                rc,
                addr,
            } => self.emit_intr_inline(pc, kind, a, ac, rc, addr),
            Act::IntrH { kind, a } => self.emit_intr_helper(pc, kind, a),
            _ => unreachable!("not an intrinsic act"),
        }
    }

    fn emit_intr_inline(&mut self, pc: u32, kind: Intr, a: u32, ac: IK, rc: IK, addr: usize) {
        if self.rr.contains(a) {
            // (cannot happen for a function value; a number in a register would make the memory check lie)
            self.refuse(format!(
                "callee slot {a} of the intrinsic at pc {pc} is register-resident"
            ));
            return;
        }
        let (x, y) = (a + 1, a + 2);
        let two = kind.nargs() == 2;
        // operand registers first: the slow path is entered with the register-resident set after these loads
        let (rx, ry) = (
            self.intr_prefetch(x, ac),
            if two { self.intr_prefetch(y, ac) } else { None },
        );
        // a float result of an integer-argument `sqrt` etc. converts through X0: nothing to prefetch
        let slow = self.slow_path(pc);
        let slow_c = self.slow_path_counted(pc);
        self.intr_guards(a, kind.nargs(), addr, slow, slow_c);
        // compute (registers only; every remaining jump to `slow` precedes the count), then count, then commit
        let done = match (kind, ac) {
            (Intr::Sqrt, _) => {
                let src = self.intr_flt_operand(x, ac, rx);
                let d = self.def_x(a);
                self.cx.op(Op::FSqrt(d, src));
                (IK::Flt, d)
            }
            (Intr::Abs, IK::Int) => {
                let r = rx.unwrap_or_else(|| {
                    self.cx.ld(S0, W::B8, fr(x, 8));
                    S0
                });
                let d = self.def_reg(a);
                if d != r {
                    self.cx.op(Op::MovR(d, r));
                }
                let l = self.cx.p.label();
                self.cx.br(Cc::Ge, d, Src::Imm(0), l);
                self.cx.op(Op::Neg(d));
                self.cx.op(Op::Bind(l));
                (IK::Int, d)
            }
            (Intr::Abs, IK::Flt) => {
                let d = self.def_x(a);
                match rx {
                    Some(r) if r != d => self.cx.op(Op::MovF(d, r)),
                    Some(_) => {}
                    None => self.cx.op(Op::LdF {
                        dst: d,
                        m: fr(x, 8),
                    }),
                }
                if !crate::t15::mutants::active("intr_abs_no_clear") {
                    self.cx.op(Op::FAbs(d));
                }
                (IK::Flt, d)
            }
            (Intr::Floor | Intr::Ceil, IK::Int) => {
                let r = rx.unwrap_or_else(|| {
                    self.cx.ld(S0, W::B8, fr(x, 8));
                    S0
                });
                let d = self.def_reg(a);
                if d != r {
                    self.cx.op(Op::MovR(d, r));
                }
                (IK::Int, d)
            }
            (Intr::Floor | Intr::Ceil, IK::Flt) => {
                debug_assert!(rc == IK::Int);
                (
                    IK::Int,
                    self.emit_floor_ceil(a, x, rx, kind == Intr::Floor, slow),
                )
            }
            (Intr::Min | Intr::Max, IK::Int) => {
                let ra = rx.unwrap_or_else(|| {
                    self.cx.ld(S0, W::B8, fr(x, 8));
                    S0
                });
                let rb = ry.unwrap_or_else(|| {
                    self.cx.ld(S1, W::B8, fr(y, 8));
                    S1
                });
                // min: r = a; if b < a then r = b.  max: r = a; if a < b then r = b   (lmathlib order)
                self.cx.op(Op::MovR(S2, ra));
                let l = self.cx.p.label();
                if kind == Intr::Min {
                    self.cx.br(Cc::Ge, rb, Src::V(ra), l);
                } else {
                    self.cx.br(Cc::Ge, ra, Src::V(rb), l);
                }
                self.cx.op(Op::MovR(S2, rb));
                self.cx.op(Op::Bind(l));
                let d = self.def_reg(a);
                self.cx.op(Op::MovR(d, S2));
                (IK::Int, d)
            }
            (Intr::Min | Intr::Max, IK::Flt) => {
                // Lua's `min(a, b)` is `b < a ? b : a`, `max(a, b)` is `a < b ? b : a`: exactly `minsd`/`maxsd` with
                // the *second* argument as destination and the first as source (NaN and equal operands give the
                // source = the first argument)
                let (first, second) = if crate::t15::mutants::active("intr_minmax_order") {
                    (x, y)
                } else {
                    (y, x)
                };
                let (rfirst, rsecond) = if first == y { (ry, rx) } else { (rx, ry) };
                match rfirst {
                    Some(r) => self.cx.op(Op::MovF(0, r)),
                    None => self.cx.op(Op::LdF {
                        dst: 0,
                        m: fr(first, 8),
                    }),
                }
                let op = match (kind, crate::t15::mutants::active("intr_minmax_swap")) {
                    (Intr::Min, false) | (Intr::Max, true) => FOp::Min,
                    _ => FOp::Max,
                };
                match rsecond {
                    Some(r) => self.cx.op(Op::FBin { op, dst: 0, src: r }),
                    None => self.cx.op(Op::FBinM {
                        op,
                        dst: 0,
                        m: fr(second, 8),
                    }),
                }
                let d = self.def_x(a);
                if d != 0 {
                    self.cx.op(Op::MovF(d, 0));
                }
                (IK::Flt, d)
            }
            _ => unreachable!("intrinsic {kind:?} is not an inline form"),
        };
        self.intr_count(pc);
        match done.0 {
            IK::Int => self.commit_int(a, done.1),
            IK::Flt => self.commit_flt(a, done.1),
        }
        self.intr_post(pc, a);
        self.end_straight(pc);
    }

    /// The double operand of a float function: the argument's register, or `X0` converted / loaded from memory.
    fn intr_flt_operand(&mut self, s: u32, ac: IK, reg: Option<u8>) -> X {
        match (ac, reg) {
            (IK::Flt, Some(r)) => r,
            (IK::Flt, None) => {
                self.cx.op(Op::LdF {
                    dst: 0,
                    m: fr(s, 8),
                });
                0
            }
            (IK::Int, r) => {
                let g = r.unwrap_or_else(|| {
                    self.cx.ld(S0, W::B8, fr(s, 8));
                    S0
                });
                self.cx.op(Op::I2Fx(0, g));
                0
            }
        }
    }

    /// `floor` / `ceil` of the float slot `x` into the integer slot `a`; a result that is not an `i64` (NaN, inf,
    /// beyond 2^63) takes the slow path (the real function returns a float).
    fn emit_floor_ceil(&mut self, a: u32, x: u32, rx: Option<u8>, floor: bool, slow: L) -> u8 {
        let xr = match rx {
            Some(r) => r,
            None => {
                self.cx.op(Op::LdF {
                    dst: 0,
                    m: fr(x, 8),
                });
                0
            }
        };
        let d = self.def_reg(a);
        let (inexact, done) = (self.cx.p.label(), self.cx.p.label());
        // exact integral value in [-2^63, 2^63): floor = ceil = the value
        self.cx.op(Op::F2IExact {
            dst: d,
            src: xr,
            to: inexact,
        });
        self.cx.op(Op::Jmp(done));
        self.cx.op(Op::Bind(inexact));
        // `d` = truncation toward zero, or i64::MIN for NaN / infinities / out of range
        self.cx.op(Op::Mov(S1, i64::MIN as u64));
        if !crate::t15::mutants::active("intr_floor_no_range") {
            self.cx.br(Cc::Eq, d, Src::V(S1), slow);
        }
        self.cx.op(Op::FtoG(S1, xr));
        let neg = self.cx.p.label();
        self.cx.br(Cc::Lt, S1, Src::Imm(0), neg);
        // positive, non-integral: floor = trunc, ceil = trunc + 1
        if !floor || crate::t15::mutants::active("intr_floor_ceil_swap") {
            self.cx.op(Op::Add(d, Src::Imm(1)));
        }
        self.cx.op(Op::Jmp(done));
        self.cx.op(Op::Bind(neg));
        // negative, non-integral: floor = trunc - 1, ceil = trunc
        if floor && !crate::t15::mutants::active("intr_floor_ceil_swap") {
            self.cx.op(Op::Sub(d, Src::Imm(1)));
        }
        self.cx.op(Op::Bind(done));
        d
    }

    fn emit_intr_helper(&mut self, pc: u32, kind: Intr, a: u32) {
        if self.rr.contains(a) {
            self.refuse(format!(
                "callee slot {a} of the intrinsic at pc {pc} is register-resident"
            ));
            return;
        }
        let addr = jit_hooks::intrinsic_addr(kind.index());
        let slow = self.slow_path(pc);
        let slow_c = self.slow_path_counted(pc);
        self.intr_guards(a, 1, addr, slow, slow_c);
        self.intr_count(pc);
        self.intr_savedpc(pc);
        // the helper reads the argument slot and writes the result slot: both must be exact
        self.flush_stores(self.rr);
        self.store_budget();
        self.rr = crate::t15::SlotSet::EMPTY;
        self.cx.op(Op::CallH {
            f: jit_t15_math as *const () as usize as u64,
            a: a | u32::from(kind.index()) << 8,
            b: 0,
        });
        let f = self.follow(pc);
        let tgt = if self.in_region(f) {
            self.need_stub.insert(f);
            self.stub_l[(f - self.plan.head) as usize]
        } else {
            self.tier1_label(f)
        };
        self.cx.op(Op::Jmp(tgt));
    }

    // ---- GETUPVAL ----------------------------------------------------------------------------------------

    pub(super) fn emit_get_up(&mut self, pc: u32, a: u32, b: u32, res: URes) {
        // an open upvalue of this frame can alias a stack slot (`debug.upvaluejoin`): make those slots exact first
        if !crate::t15::mutants::active("up_no_spill") {
            for s in self.rr.iter().collect::<Vec<_>>() {
                if self.plan.escaped.contains(s) {
                    self.spill(s);
                }
            }
        }
        let slow = match res {
            URes::Mem(None) => None,
            _ => Some(self.slow_path(pc)),
        };
        // S0 = &upvalue value (closure in the function slot below the frame)
        self.cx.ld(S0, W::B8, fr(0, -16 + 8));
        self.cx.ld(
            S0,
            W::B8,
            M {
                base: Base::Reg(S0),
                off: (lclosure::UPVALS + 8 * b as usize) as i32,
            },
        );
        self.cx.ld(
            S0,
            W::B8,
            M {
                base: Base::Reg(S0),
                off: upval::V as i32,
            },
        );
        let at = |off: i32| M {
            base: Base::Reg(S0),
            off,
        };
        match res {
            URes::Int | URes::Flt => {
                let (tag, is_int) = (
                    if res == URes::Int { TAG_INT } else { TAG_FLT },
                    res == URes::Int,
                );
                self.cx.op(Op::BrM {
                    cc: Cc::Ne,
                    w: W::B1,
                    m: at(0),
                    imm: tag,
                    to: slow.expect("typed"),
                });
                if is_int {
                    let d = self.def_reg(a);
                    self.cx.ld(d, W::B8, at(8));
                    self.commit_int(a, d);
                } else {
                    let d = self.def_x(a);
                    self.cx.op(Op::LdF { dst: d, m: at(8) });
                    self.commit_flt(a, d);
                }
            }
            URes::Mem(Some(tag)) => {
                self.cx.op(Op::BrM {
                    cc: Cc::Ne,
                    w: W::B1,
                    m: at(0),
                    imm: i32::from(tag),
                    to: slow.expect("typed"),
                });
                self.cx.ld(S1, W::B8, at(8));
                self.cx.st_imm(W::B1, fr(a, 0), i32::from(tag));
                self.cx.st(W::B8, fr(a, 8), S1);
                self.mem_def(a);
            }
            URes::Mem(None) => {
                self.cx.ld(S1, W::B1, at(0));
                self.cx.ld(S2, W::B8, at(8));
                self.cx.st(W::B1, fr(a, 0), S1);
                self.cx.st(W::B8, fr(a, 8), S2);
                self.mem_def(a);
            }
        }
        self.end_straight(pc);
    }
}
