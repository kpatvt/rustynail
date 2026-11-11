//! Code emitted by the verification options `+exitstorm` and `+shadow` (task T8; docs/JIT_TIER15.md section 22).
//! Neither option changes anything when it is off: every function here is reached only behind an `OptFlags` test.

use super::{Act, Rg, VPad, S0, S2};
use crate::ir::{Base, Cc, Op, Src, L, M, W};
use crate::lower::fr;
use crate::t15::SlotSet;

/// Is a region template of this kind free of side effects on anything but the frame slots and the budget, and free
/// of helper calls and exits in the middle of the instruction (the blocks `+shadow` can verify)?
fn pure_act(a: &Act) -> bool {
    matches!(
        a,
        Act::LoadI(..)
            | Act::MoveI(..)
            | Act::Bin { .. }
            | Act::Neg(..)
            | Act::DivK { .. }
            | Act::Not(..)
            | Act::Shift { .. }
            | Act::MemConst(..)
            | Act::LoadNil(..)
            | Act::LoadK(..)
            | Act::MoveM(..)
            | Act::Jmp { .. }
            | Act::Cmp { .. }
            | Act::Test { .. }
            | Act::ForLoop { .. }
            | Act::LoadFc(..)
            | Act::MoveF(..)
            | Act::FBin { .. }
            | Act::FNeg(..)
            | Act::FCmp { .. }
            | Act::FForLoop { .. }
    )
}

impl Rg<'_, '_> {
    // ---- exit storm ----

    /// A pseudo-guard: increments a private counter and jumps to `fire` (resetting it) at every `n`-th execution.
    /// Clobbers only the scratch registers `rax` and `rcx`.
    pub(in crate::t15) fn storm_test(&mut self, n: u32, fire: L) {
        let addr = crate::t15::verify::storm_counter();
        let m = M {
            base: Base::Reg(S0),
            off: 0,
        };
        let nf = self.cx.p.label();
        self.cx.op(Op::Mov(S0, addr as u64));
        self.cx.ld(S2, W::B4, m);
        self.cx.op(Op::Add(S2, Src::Imm(1)));
        self.cx.br(Cc::Ne, S2, Src::Imm(n as i32), nf);
        self.cx.st_imm(W::B4, m, 0);
        self.cx.op(Op::Jmp(fire));
        self.cx.op(Op::Bind(nf));
        self.cx.st(W::B4, m, S2);
    }

    /// The out-of-line exit of a pseudo-guard: write back `rr`, store the budget, continue in tier 1 at `pc`.
    pub(in crate::t15) fn storm_leave(&mut self, rr: SlotSet, pc: u32) -> L {
        if let Some(&l) = self.storm_map.get(&(rr.0, pc)) {
            return l;
        }
        let l = self.cx.p.label();
        self.storm_map.insert((rr.0, pc), l);
        self.storm_pads.push((l, rr, pc));
        l
    }

    pub(in crate::t15) fn emit_storm_pads(&mut self) {
        let mut i = 0;
        while i < self.storm_pads.len() {
            let (l, rr, pc) = self.storm_pads[i];
            i += 1;
            self.cx.op(Op::Bind(l));
            self.flush_stores(rr);
            self.store_budget();
            let t = self.tier1_label(pc);
            self.cx.op(Op::Jmp(t));
        }
    }

    // ---- shadow verification ----

    /// Finds the blocks that `+shadow` verifies (every non-follower instruction a pure template) and registers them.
    pub(in crate::t15) fn plan_shadow(&mut self) {
        if !self.opt.shadow {
            return;
        }
        let (h, e) = (self.plan.head, self.plan.end);
        let env = self.env;
        let pcl = &self.cx.pcl;
        crate::t15::verify::add_heads(
            (0..env.head.len())
                .filter(|&p| env.head[p])
                .map(|p| (pcl[p], p as u32)),
        );
        let starts: Vec<u32> = (h..=e)
            .filter(|&pc| pc == h || env.head[pc as usize])
            .collect();
        for (i, &s) in starts.iter().enumerate() {
            let end = starts.get(i + 1).copied().unwrap_or(e + 1);
            let (mut ok, mut any) = (true, false);
            for pc in s..end {
                if env.follower[pc as usize] {
                    continue;
                }
                let k = (pc - h) as usize;
                if self.rr_in[k].is_none() || !pure_act(&self.acts[k]) {
                    ok = false;
                    break;
                }
                any = true;
            }
            if !(ok && any) {
                continue;
            }
            let id = crate::t15::verify::register_block(
                s,
                end - 1,
                self.cx.pcl[s as usize],
                self.plan.nslots,
            );
            for pc in s..end {
                self.vblock_of[(pc - h) as usize] = Some(id);
            }
            self.vstart.insert(s);
        }
    }

    /// Reloads the registers of the slots in `rr` from the (flushed) frame.
    fn reload_rr(&mut self, rr: SlotSet) {
        for s in rr.iter() {
            if let Some(v) = self.asg.of(s) {
                self.cx.ld(v, W::B8, fr(s, 8));
            } else if let Some(x) = self.asg.xof(s) {
                self.cx.op(Op::LdF {
                    dst: x,
                    m: fr(s, 8),
                });
            } else {
                self.refuse(format!("slot {s} is register-resident but has no register"));
            }
        }
    }

    /// The start of a verified block: flush, record the frame and the budget, reload.
    pub(in crate::t15) fn emit_shadow_pre(&mut self, id: u32) {
        let rr = self.rr;
        self.flush_stores_clean(rr);
        self.store_budget();
        self.cx.op(Op::CallH {
            f: crate::t15::verify::jit_t15_shadow_pre as *const () as usize as u64,
            a: id,
            b: 0,
        });
        self.reload_rr(rr);
        self.load_budget();
    }

    /// The label for a successor edge of the verified block being emitted: verification first, then `normal`.
    pub(in crate::t15) fn verify_edge(&mut self, from: SlotSet, to: u32, normal: L) -> L {
        let Some(id) = self.vcur else { return normal };
        if !self.env.head[to as usize] {
            return normal; // an edge inside the block
        }
        let key = (to, from.0, normal, id);
        if let Some(&l) = self.vpad_map.get(&key) {
            return l;
        }
        let l = self.cx.p.label();
        self.vpad_map.insert(key, l);
        self.vpads.push(VPad {
            l,
            from,
            to,
            id,
            normal,
        });
        l
    }

    pub(in crate::t15) fn emit_vpads(&mut self) {
        let pads = std::mem::take(&mut self.vpads);
        for p in pads {
            self.cx.op(Op::Bind(p.l));
            self.flush_stores_clean(p.from);
            self.store_budget();
            self.cx.op(Op::CallH {
                f: crate::t15::verify::jit_t15_shadow_post as *const () as usize as u64,
                a: p.id,
                b: p.to,
            });
            self.reload_rr(p.from);
            self.load_budget();
            self.cx.op(Op::Jmp(p.normal));
        }
    }
}
