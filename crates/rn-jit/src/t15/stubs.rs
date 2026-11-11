//! Stubs, exits and edges of region code (docs/JIT_TIER15.md 4.4-4.6, 4.9; as built: section 16).
//!
//! * [`Rg::flush_stores`] / [`Rg::store_budget`]: write the register-resident slots (tag byte + payload) and the
//!   budget register `r11` back to the VM stack and to `[r15]` (invariant T-EXACT; B1).
//! * [`Rg::edge`]: the label to jump to for a control-flow edge `(state, target)`: the region label itself when
//!   nothing has to happen on the edge, else a *pad* (extra register-resident slots are flushed, the back-edge poll
//!   is done, or the region is left for the tier-1 label of the target).
//! * [`Rg::cexit`]: the budget-exhausted exit of a block head (flush, then tier-1's `exit_at`).
//! * [`Rg::emit_stub`]: `stub(P)`: guards from `RegionPlan::stub_guards`, loads of the slots in `RR(P)`, the
//!   budget register, then the region code of `P`; a failed guard continues in tier 1 at `P` (nothing was
//!   modified, nothing counted).
//! * [`Rg::finish`]: emits everything that was collected out of line.

use super::codegen::Rg;
use super::SlotSet;
use crate::ir::{Base, Cc, Op, L, W};
use crate::lower::{ciw, fr};
use rn_core::value::LUA_VNUMINT;

const TAG_INT: i32 = LUA_VNUMINT as i32;
const TAG_FLT: i32 = rn_core::value::LUA_VNUMFLT as i32;

/// `+verify` / budget-invariant failure (never returns): a check emitted by `+verify` found that the frame does not
/// have the tags tier 1's `Cx::ty` assumed at an exit target (`b` = the slot), or that `[r15]` is negative when a
/// stub runs (`b = 0xFFFF`, the budget invariant of docs 16.1 item 7). Aborts the process with a message: it is a
/// bug in the generator, not something to recover from.
pub extern "C" fn jit_t15_verify_fail(_th: *mut u8, _ci: *mut u8, pc: u32, b: u32) -> u32 {
    if b == 0xFFFF {
        eprintln!("[t15] internal error: negative budget word when entering the stub of pc {pc}");
    } else {
        eprintln!(
            "[t15] internal error (+verify): slot {b} does not have the type tier 1 assumed at pc {pc}"
        );
    }
    std::process::abort()
}

/// An out-of-line edge sequence.
#[derive(Clone, Copy, Debug)]
pub(super) struct Pad {
    pub l: L,
    pub from: SlotSet,
    pub to: u32,
    pub poll: bool,
    /// The target is outside the region (or unreached): flush everything and continue in tier 1.
    pub leave: bool,
}

impl Rg<'_, '_> {
    /// Stores the tag byte and payload of every slot in `rr` (their registers hold the current values).
    pub(super) fn flush_stores(&mut self, rr: SlotSet) {
        self.flush_impl(rr, true);
    }

    /// [`Rg::flush_stores`] without the test mutants (the shadow verification brackets must not share a bug with
    /// the exits they are meant to be independent of).
    pub(super) fn flush_stores_clean(&mut self, rr: SlotSet) {
        self.flush_impl(rr, false);
    }

    fn flush_impl(&mut self, rr: SlotSet, mutate: bool) {
        let n = rr.iter().count();
        let skip_last = mutate && super::mutants::active("flush_skip_last") && n >= 2;
        for (idx, s) in rr.iter().enumerate() {
            if skip_last && idx + 1 == n {
                continue;
            }
            let skip_tag = mutate && idx == 0 && super::mutants::active("flush_skip_tag");
            if let Some(v) = self.asg.of(s) {
                if !skip_tag {
                    self.cx.st_imm(W::B1, fr(s, 0), TAG_INT);
                }
                // (T8 mutant) a wrong entry of the write-back map: the first slot is written from the register of the second
                let v = if mutate && idx == 0 && super::mutants::active("exitmap_wrong_reg") {
                    rr.iter().nth(1).and_then(|s2| self.asg.of(s2)).unwrap_or(v)
                } else {
                    v
                };
                self.cx.st(W::B8, fr(s, 8), v);
            } else if let Some(x) = self.asg.xof(s) {
                if !skip_tag {
                    let tag = if mutate && super::mutants::active("flt_flush_int_tag") {
                        TAG_INT
                    } else {
                        TAG_FLT
                    };
                    self.cx.st_imm(W::B1, fr(s, 0), tag);
                }
                // (`movsd` copies the bits unchanged: -0.0 and NaN payloads survive)
                self.cx.op(Op::StF {
                    m: fr(s, 8),
                    src: x,
                });
            } else {
                self.refuse(format!("slot {s} is register-resident but has no register"));
            }
        }
    }

    /// `[r15] = r11` (the budget word; B1: every fallback and every exit stores it).
    pub(super) fn store_budget(&mut self) {
        self.cx.st(
            W::B4,
            crate::ir::M {
                base: Base::Cnt,
                off: 0,
            },
            5,
        );
    }

    /// `r11 = [r15]`.
    pub(super) fn load_budget(&mut self) {
        self.cx.ld(
            5,
            W::B4,
            crate::ir::M {
                base: Base::Cnt,
                off: 0,
            },
        );
    }

    /// `RR` at the start of a reached pc (a missing entry refuses the region).
    pub(super) fn rr_at(&mut self, pc: u32) -> SlotSet {
        match self
            .rr_in
            .get(pc.wrapping_sub(self.plan.head) as usize)
            .copied()
            .flatten()
        {
            Some(r) => r,
            None => {
                self.refuse(format!("no register state for pc {pc}"));
                SlotSet::EMPTY
            }
        }
    }

    /// Is `pc` code that region code was emitted for (a label `reg_l[pc]` is bound)?
    pub(super) fn reached(&self, pc: u32) -> bool {
        pc >= self.plan.head
            && pc <= self.plan.end
            && !matches!(
                self.acts[(pc - self.plan.head) as usize],
                super::codegen::Act::Dead
            )
            && self.rr_in[(pc - self.plan.head) as usize].is_some()
    }

    /// The label for the edge from a state with register-resident set `from` to `to`. `poll`: the edge is a
    /// back-edge of the source language (a `JMP`/`FORLOOP`/conditional jump backwards): do tier 1's interrupt
    /// poll first (5-instruction form, docs 4.9).
    pub(super) fn edge(&mut self, from: SlotSet, to: u32, poll: bool) -> L {
        let l = self.edge_plain(from, to, poll);
        self.verify_edge(from, to, l)
    }

    fn edge_plain(&mut self, from: SlotSet, to: u32, poll: bool) -> L {
        let leave = !self.reached(to);
        if !leave && !poll {
            let inn = self.rr_at(to);
            if from.minus(&inn).is_empty() {
                return self.reg_l[(to - self.plan.head) as usize];
            }
        }
        let key = (to, from.0, poll);
        if let Some(&l) = self.pad_map.get(&key) {
            return l;
        }
        let l = self.cx.p.label();
        self.pad_map.insert(key, l);
        self.pads.push(Pad {
            l,
            from,
            to,
            poll,
            leave,
        });
        l
    }

    /// The label of the budget exit of the block head `pc`.
    pub(super) fn cexit(&mut self, pc: u32, n: u32) -> L {
        if let Some(&(l, m)) = self.cexit_map.get(&pc) {
            debug_assert_eq!(m, n, "one budget exit per pc has one count");
            return l;
        }
        let l = self.cx.p.label();
        self.cexit_map.insert(pc, (l, n));
        l
    }

    /// Emits the collected pads and budget exits.
    pub(super) fn finish(&mut self) {
        self.vcur = None;
        let mut i = 0;
        while i < self.pads.len() {
            let p = self.pads[i];
            i += 1;
            self.cx.op(Op::Bind(p.l));
            let trap = p.poll.then(|| self.cx.p.label());
            if let Some(t) = trap {
                let ic = self.cx.cfg.interrupt_addr as u64;
                self.cx.op(Op::BrM {
                    cc: Cc::Ne,
                    w: W::B4,
                    m: ciw(rn_core::jit_layout::callinfo::TRAP),
                    imm: 0,
                    to: t,
                });
                if !super::mutants::active("pad_no_irq_poll") {
                    self.cx.op(Op::Mov(7, ic));
                    self.cx.op(Op::BrM {
                        cc: Cc::Ne,
                        w: W::B1,
                        m: crate::ir::M {
                            base: Base::Reg(7),
                            off: 0,
                        },
                        imm: 0,
                        to: t,
                    });
                }
            }
            if p.leave {
                let mut from = p.from;
                if super::mutants::active("leave_drop_entry") {
                    // (T8 mutant) an entry of the exit map of the pads that leave the region is dropped
                    let first = from.iter().next();
                    if let Some(first) = first {
                        from.remove(first);
                    }
                }
                self.flush_stores(from);
                if !super::mutants::active("leave_no_budget") {
                    self.store_budget();
                }
                let t = self.tier1_label(p.to);
                self.cx.op(Op::Jmp(t));
            } else {
                let inn = self.rr_at(p.to);
                self.flush_stores(p.from.minus(&inn));
                let t = self.reg_l[(p.to - self.plan.head) as usize];
                self.cx.op(Op::Jmp(t));
            }
            if let Some(n) = self.opt.exitstorm {
                // pseudo-guards of the pad: a pending interrupt (the trap exit) and leaving the region for tier 1
                if let Some(t) = trap {
                    self.storm_test(n, t);
                }
                if !p.leave {
                    let f = self.storm_leave(p.from, p.to);
                    self.storm_test(n, f);
                }
            }
            if let Some(t) = trap {
                // interrupt or hook pending: leave exactly as tier 1's poll does (savedpc = target, trap = 1)
                self.cx.op(Op::Bind(t));
                self.flush_stores(p.from);
                self.store_budget();
                let x = self.cx.exit_trap(p.to);
                self.cx.op(Op::Jmp(x));
            }
        }
        // out-of-line slow paths of inline table accesses (they register stubs, so before the stub sweep)
        self.emit_slow_paths();
        let mut ce: Vec<(u32, (L, u32))> = self.cexit_map.iter().map(|(&a, &b)| (a, b)).collect();
        ce.sort_unstable_by_key(|&(pc, _)| pc);
        for (pc, (l, n)) in ce {
            self.cx.op(Op::Bind(l));
            let rr = self.rr_in[(pc - self.plan.head) as usize].unwrap_or(SlotSet::EMPTY);
            self.flush_stores(rr);
            self.store_budget();
            // tier 1's own exit of the failed count: savedpc = pc, `Continue`, the `n` just subtracted is given back
            let x = self.cx.exit_cnt(pc, n);
            self.cx.op(Op::Jmp(x));
        }
        let mut st: Vec<u32> = self.need_stub.iter().copied().collect();
        st.sort_unstable();
        for pc in st {
            self.emit_stub(pc);
        }
        self.emit_vpads();
        self.emit_storm_pads();
        self.emit_checks();
    }

    /// `+verify`: the type-knowledge checks in front of tier 1's code (see [`Rg::tier1_label`]).
    fn emit_checks(&mut self) {
        let mut pcs: Vec<u32> = self.chk_map.keys().copied().collect();
        pcs.sort_unstable();
        for pc in pcs {
            let l = self.chk_map[&pc];
            let real = self.cx.pcl[pc as usize];
            let fail = self.cx.p.label();
            let known = self.env.ty_at[pc as usize].clone();
            self.cx.op(Op::Bind(l));
            let mut first = None;
            for (slot, ty) in known {
                let tag = match ty {
                    crate::lower::Ty::Int => LUA_VNUMINT,
                    crate::lower::Ty::Flt => rn_core::value::LUA_VNUMFLT,
                    crate::lower::Ty::Unk => continue,
                };
                first.get_or_insert(u32::from(slot));
                self.cx.op(Op::BrM {
                    cc: Cc::Ne,
                    w: W::B1,
                    m: fr(u32::from(slot), 0),
                    imm: i32::from(tag),
                    to: fail,
                });
            }
            self.cx.op(Op::Jmp(real));
            self.cx.op(Op::Bind(fail));
            // (one failure label per pc; the message names the first checked slot)
            self.cx.op(Op::CallH {
                f: jit_t15_verify_fail as *const () as usize as u64,
                a: pc,
                b: first.unwrap_or(0),
            });
            self.cx.op(Op::Ret(0));
        }
    }

    /// The pc that precedes `pc` in straight-line code (MMBIN / EXTRAARG followers skipped backwards).
    fn follow_prev(&self, pc: u32) -> u32 {
        let mut q = pc - 1;
        while q > 0 && self.env.follower[q as usize] {
            q -= 1;
        }
        q
    }

    /// `stub(pc)` (docs 4.6). Entered with every value in its VM stack slot, `rbx`/`r13`/`r14` valid.
    fn emit_stub(&mut self, pc: u32) {
        let sl = self.stub_l[(pc - self.plan.head) as usize];
        self.cx.op(Op::Bind(sl));
        let real = self.tier1_label(pc);
        if !self.reached(pc) {
            self.cx.op(Op::Jmp(real));
            return;
        }
        let fail = self.cx.p.label();
        if let Some(n) = self.opt.exitstorm {
            // pseudo-guard of the stub entry (period at least 2, so that the region is reached at all)
            self.storm_test(n.max(2), fail);
        }
        // failure accounting (tier 1.5 policy): non-peel guards fail into a counting pad, peel guards (the by-design
        // first-entry failure) and stubs without accounting (`-respec`) go straight to tier 1
        let counted = self.fb.map(|_| self.cx.p.label());
        let mut site_guards: Vec<super::runtime::StubGuard> = Vec::new();
        let guards = self.plan.stub_guards(pc);
        let ng = guards.len();
        for (gi, (s, t)) in guards.into_iter().enumerate() {
            if ng >= 2 && gi == 0 && super::mutants::active("drop_guard_first") {
                continue;
            }
            if ng >= 2 && gi + 1 == ng && super::mutants::active("drop_guard_last") {
                continue;
            }
            if gi == 0
                && self.fall_succ.contains(&pc)
                && super::mutants::active("drop_guard_after_fall")
            {
                continue;
            }
            if self.fall_succ.contains(&pc) && super::mutants::active("fall_succ_no_guards") {
                continue; // (T8 mutant) no re-guard at all after a fallback
            }
            let Some(tag) = t.tag_byte() else {
                self.refuse(format!("guard of slot {s} at pc {pc} is not a single tag"));
                continue;
            };
            let peel = pc == self.plan.head
                && self.plan.entry.iter().any(|e| e.slot == s && e.peel)
                && !super::mutants::active("pol_no_peel_exempt");
            let to = match counted {
                Some(c) if !peel => c,
                _ => fail,
            };
            if counted.is_some() {
                let prev = if pc > self.plan.head {
                    self.follow_prev(pc)
                } else {
                    pc
                };
                // the load whose `prof` byte produced this fact: only the table / upvalue loads (and the method of
                // SELF, not its receiver) have a speculation that retarget / widen can change; LEN / POW have a fixed
                // guard (`fixed`: no feedback can change it)
                let defining = pc > self.plan.head
                    && prev >= self.plan.head
                    && self.follow(prev) == pc
                    && self
                        .plan
                        .insn(prev)
                        .is_some_and(|i| i.guard_def.is_some() && i.defs.contains(s));
                let (mut def_pc, mut fixed) = (None, false);
                if defining {
                    use rn_core::ver::active::OpCode as O;
                    let a = self.env.ins[prev as usize].a();
                    match self.env.ops[prev as usize] {
                        Some(
                            O::Gettable
                            | O::Geti
                            | O::Getfield
                            | O::Gettabup
                            | O::Getupval
                            | O::Call,
                        ) => def_pc = Some(prev),
                        Some(O::SelfOp) if s == a => def_pc = Some(prev),
                        Some(O::SelfOp) => {}
                        _ => fixed = true,
                    }
                }
                site_guards.push(super::runtime::StubGuard {
                    slot: s as u8,
                    tag,
                    peel,
                    def_pc,
                    fixed,
                });
            }
            self.cx.op(Op::BrM {
                cc: Cc::Ne,
                w: W::B1,
                m: fr(s, 0),
                imm: i32::from(tag),
                to,
            });
        }
        if let Some(fb) = self.fb.filter(|_| pc == self.plan.head) {
            // a successful entry through the head stub (the denominator of the failure rate)
            self.cx.op(Op::Mov(7, fb.ok_addr as u64));
            let m = crate::ir::M {
                base: Base::Reg(7),
                off: 0,
            };
            self.cx.ld(6, W::B4, m);
            self.cx.op(Op::Add(6, crate::ir::Src::Imm(1)));
            self.cx.st(W::B4, m, 6);
        }
        let inn = self.rr_at(pc);
        let nin = inn.iter().count();
        for (idx, s) in inn.iter().enumerate() {
            if idx + 1 == nin && super::mutants::active("stub_no_load_last") {
                continue;
            }
            if let Some(v) = self.asg.of(s) {
                self.cx.ld(v, W::B8, fr(s, 8));
            } else if let Some(x) = self.asg.xof(s) {
                self.cx.op(Op::LdF {
                    dst: x,
                    m: fr(s, 8),
                });
            } else {
                self.refuse(format!(
                    "slot {s} is loaded by the stub of pc {pc} but has no register"
                ));
            }
        }
        let after_call = pc > 0
            && matches!(
                self.env.ops[self.follow_prev(pc) as usize],
                Some(rn_core::ver::active::OpCode::Call)
            );
        if !(after_call && super::mutants::active("no_budget_reload_after_call")) {
            self.load_budget();
        }
        if cfg!(debug_assertions) || self.opt.verify {
            // budget invariant (docs 16.1 item 7): compiled code only runs with a non-negative budget word, which is
            // what makes the 64-bit compare of `r11` equal to tier 1's 32-bit one
            let bad = self.cx.p.label();
            let ok = self.cx.p.label();
            self.cx.op(Op::BrM {
                cc: Cc::Lt,
                w: W::B4,
                m: crate::ir::M {
                    base: Base::Cnt,
                    off: 0,
                },
                imm: 0,
                to: bad,
            });
            self.cx.op(Op::Jmp(ok));
            self.cx.op(Op::Bind(bad));
            self.cx.op(Op::CallH {
                f: jit_t15_verify_fail as *const () as usize as u64,
                a: pc,
                b: 0xFFFF,
            });
            self.cx.op(Op::Ret(0));
            self.cx.op(Op::Bind(ok));
        }
        let r = self
            .storm_entry
            .get(&pc)
            .copied()
            .unwrap_or(self.reg_l[(pc - self.plan.head) as usize]);
        self.cx.op(Op::Jmp(r));
        self.cx.op(Op::Bind(fail));
        // a guard failed: tier 1 re-executes from `pc`; it pays the block count itself (nothing was counted)
        self.cx.op(Op::Jmp(real));
        if let (Some(fb), Some(c)) = (self.fb, counted) {
            // the counting pad: `fail -= 1`; when it goes negative the policy helper decides (it may recompile and
            // return the continuation in the new code). Nothing is live here: every value is in its VM slot.
            let (fire, stay) = (self.cx.p.label(), self.cx.p.label());
            let ctr = crate::ir::M {
                base: Base::Reg(7),
                off: 0,
            };
            self.cx.op(Op::Bind(c));
            self.cx.op(Op::Mov(7, fb.fail_addr as u64));
            self.cx.ld(6, W::B4, ctr);
            if !super::mutants::active("pol_no_fail_count") {
                self.cx.op(Op::Sub(6, crate::ir::Src::Imm(1)));
                self.cx.st(W::B4, ctr, 6);
            }
            // (a 32-bit signed compare of the word in memory: the countdown is an `i32`; a 64-bit compare of the
            // zero-extended register fires only on the exact 0 -> -1 step and leaves 0xFFFFFFFF behind)
            if super::mutants::active("pol_countdown_64bit") {
                self.cx.br(Cc::Lt, 6, crate::ir::Src::Imm(0), fire);
            } else {
                self.cx.op(Op::BrM {
                    cc: Cc::Lt,
                    w: W::B4,
                    m: ctr,
                    imm: 0,
                    to: fire,
                });
            }
            self.cx.op(Op::Jmp(real));
            self.cx.op(Op::Bind(fire));
            self.cx.op(Op::CallH {
                f: fb.event_fn,
                a: fb.nest | (u32::from(fb.ver) << 24),
                b: (super::runtime::EventKind::StubFail as u32) << 16 | pc,
            });
            if let Some(env) = self.cx.cfg.calls {
                self.cx.op(Op::ReloadFrame {
                    stack_off: env.stack_off,
                });
            }
            self.cx.br(Cc::Eq, 0, crate::ir::Src::Imm(0), stay);
            self.cx.op(Op::Mov(1, fb.cont_addr as u64));
            self.cx.ld(
                1,
                W::B8,
                crate::ir::M {
                    base: Base::Reg(1),
                    off: 0,
                },
            );
            self.cx.op(Op::JmpR(1));
            self.cx.op(Op::Bind(stay));
            self.cx.op(Op::Jmp(real));
            self.sites.push(super::runtime::StubSite {
                pc,
                guards: site_guards,
            });
        }
    }
}
