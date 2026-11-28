//! Section 87 (`asmfuse`, docs/JIT_TRACE.md section 87): the fused back end. One backward pass over the IR of an eligible trace allocates each
//! instruction (its result register is the one its later uses chose, its operands get registers at the use, an evicted value gets its spill store at its
//! definition and its reload right behind the evicting instruction, a guard's exit map is the register state at that guard) and selects its micro-ops at
//! once (the per-instruction emitters of [`Cx`], fed with the allocation record just made). The micro-ops of each instruction are appended in reverse to
//! a buffer that grows backwards; there is no `Allocation` materialised before the emission, no snapshot pin pass and no forward re-walk of the IR.
//! The entry sequence, the writeback / link tail and the exit stubs are the old code ([`finish`](FuseBody::finish)), placed around the body once the
//! exits are built from the maps the pass recorded.
//!
//! Eligible: side traces without loop, carry, segments, native frames, lazy region, prelude or body room checks, whose every instruction is in
//! [`converted`]; anything else (and any situation the pass does not handle: only pinned eviction victims, more than [`FUSE_EXT`] spill slots)
//! falls back to the old pipeline, which is unchanged.

use super::*;
use crate::tr::ra::{self, RaOpts, Reload};
use crate::tr::TraceOpts;

/// The spill slots a fused trace may use: its extension slots start here (the pass does not know its slot count before the instructions that
/// use the extension slots are emitted).
pub const FUSE_EXT: u16 = 24;
/// Section 91 (`fusecov` bit 0): the extension slots of a fused trace start here (high in the exit area, so that the pass does not run out of spill
/// slots on large traces: every spill slot is a disp32 access either way).
pub const FUSE_EXT_COV: u16 = asm_ext_base();

const fn asm_ext_base() -> u16 {
    super::ITER_SLOT - 16
}

const NOREG: u8 = 255;
const NONE: u32 = u32::MAX;
const NOSLOT: u16 = u16::MAX;
/// [`ra`]'s general register pool as a mask (`V7` never, `V10` never in a fused trace).
const GPR_POOL_MASK: u16 = 0x37F;

/// Section 91 (`fusecov` bit 0): side traces with body room checks (R2).
pub const COV_ROOM: u32 = 1;
/// Section 91 (`fusecov` bit 1): native-recursion traces (`rec_call` / `rec_ret`: the placeholder guard emits nothing, the guards and tails are
/// [`assemble_dl_in`]'s).
pub const COV_NATIVE: u32 = 2;
/// Section 91 (`fusecov` bit 2): rolled root traces (entry bookkeeping, first-iteration flag, second / inner entries: [`assemble_dl_in`]'s).
pub const COV_ROOT: u32 = 4;

fn idx(r: Ref) -> usize {
    usize::from(r - REF_BIAS)
}

/// Section 87: the instruction classes the fused pass converts (stage 1).
pub fn converted(ir: &TraceIr, i: usize) -> bool {
    let ins = &ir.ins[i];
    match ins.op {
        TOp::Kint | TOp::Knum | TOp::Kpri | TOp::Kptr => true,
        TOp::Sload => aux_bits(ins.aux) & SLOAD_CARRIED == 0,
        TOp::Add
        | TOp::Sub
        | TOp::Mul
        | TOp::Band
        | TOp::Bor
        | TOp::Bxor
        | TOp::Neg
        | TOp::Bnot => true,
        TOp::Idiv | TOp::Mod | TOp::Shl | TOp::Shr | TOp::F2iExact => true,
        TOp::Fadd | TOp::Fsub | TOp::Fmul | TOp::Fdiv | TOp::Fmin | TOp::Fmax => true,
        TOp::Fneg | TOp::Fabs | TOp::Fsqrt | TOp::Ffloor | TOp::ConvNumInt => true,
        TOp::Eq | TOp::Lt | TOp::Le | TOp::Gt | TOp::Ge | TOp::Ult | TOp::Uge => true,
        // (the guard behind an inline `NEWREF` reads the IR around it: not converted while `NEWREF` is not)
        TOp::Ne => true,
        TOp::Fcmp | TOp::Eqref | TOp::Fload => true,
        TOp::Budget | TOp::Poll | TOp::Sstore | TOp::Tvirt => true,
        // stage 2: the table and upvalue memory ops (no helper calls)
        TOp::Hrefk | TOp::Hload | TOp::Hmiss | TOp::Hshape | TOp::Hstore | TOp::Hrefv => true,
        TOp::Uref | TOp::Uload | TOp::Ustore | TOp::Uvrel => true,
        TOp::Aref | TOp::Aload | TOp::Astore | TOp::Xload => true,
        // stage 3: helper calls (the call site's saved registers are the pass's live registers at the call)
        TOp::Tlen
        | TOp::Tbar
        | TOp::Tnew
        | TOp::Newref
        | TOp::Tset
        | TOp::Lib
        | TOp::Calln
        | TOp::Snew
        | TOp::Concat
        | TOp::Strto => true,
        _ => false,
    }
}

static STAT: [std::sync::atomic::AtomicU64; 6] =
    [const { std::sync::atomic::AtomicU64::new(0) }; 6];

/// Section 87: `(traces fused, traces refused by the eligibility scan, traces that fell back during the pass, instructions fused, spill slots of
/// the fused traces)`.
pub fn stats() -> [u64; 6] {
    STAT.each_ref()
        .map(|a| a.load(std::sync::atomic::Ordering::Relaxed))
}

fn bump(k: usize, n: u64) {
    STAT[k].fetch_add(n, std::sync::atomic::Ordering::Relaxed);
}

/// What the body pass hands on: the assembler state with the body's micro-ops in program order ([`FuseBody::into_body`], for
/// [`assemble_dl_in`]), plus the allocation records ([`FuseBody::allocation`]).
pub struct FuseBody {
    body: FusedBody,
    fal: Vec<InsAlloc>,
    snap_locs: Vec<Vec<Loc>>,
    fused: Vec<bool>,
    end_locs: Vec<Loc>,
    used: [u32; 2],
    calls: Vec<CallSite>,
    ext: u16,
}

/// Section 87 (`RUSTYNAIL_JIT_OPSTAT=1`) / 91: why a trace is not eligible (the first reason), `None` when it is.
fn refusal(
    ir: &TraceIr,
    env: &AsmEnv,
    opts: &TraceOpts,
    ropts: &RaOpts,
    cov: u32,
) -> Option<&'static str> {
    let root = env.side.is_none();
    let native = ir.rec_call.is_some() || ir.rec_ret.is_some();
    let checks: [(&str, bool); 12] = [
        ("root", root && (cov & COV_ROOT == 0 || ir.loop_at.is_some())),
        ("loop", ir.loop_at.is_some()),
        (
            "carry",
            !ir.carry.is_empty()
                || !ir.carry_in.is_empty()
                || !ir.carry_store.is_empty()
                || ir.carry_link,
        ),
        ("segs", !ir.segs.is_empty()),
        ("native", native && cov & COV_NATIVE == 0),
        ("prelude", env.prelude),
        ("room", env.side_room && cov & COV_ROOM == 0),
        // (a root's entry bookkeeping is `head`; a side trace with a head is a link root)
        ("lazy", env.lazy || (env.head != 0 && !root)),
        (
            "budget",
            env.budget_reg != Some(ra::BUDGET_GPR) || ropts.reserve_gpr != Some(ra::BUDGET_GPR),
        ),
        ("randra", opts.randra.is_some()),
        (
            "homes",
            ropts.carry_at != 0 || ropts.homes || ropts.homes_snap,
        ),
        ("op", !(0..ir.ins.len()).all(|i| converted(ir, i))),
    ];
    checks.iter().find(|c| c.1).map(|c| c.0)
}

fn refusal_text(ir: &TraceIr, why: &str) -> String {
    if why == "op" {
        let i = (0..ir.ins.len()).find(|&i| !converted(ir, i)).unwrap_or(0);
        format!("op {}", ir.ins[i].op.name())
    } else {
        why.to_string()
    }
}

/// Section 87: the fused pass over an eligible trace (`None`: not eligible or not handled; the old pipeline runs).
#[allow(clippy::too_many_arguments)]
pub fn body(
    ir: &TraceIr,
    writeback: &[SnapEntry],
    end: &[Ref],
    end_hints: &[Loc],
    ropts: &RaOpts,
    env: &AsmEnv,
    opts: &TraceOpts,
    lim: u32,
    cov: u32,
) -> Option<FuseBody> {
    if lim == 0 {
        return None;
    }
    if let Some(why) = refusal(ir, env, opts, ropts, cov) {
        if super::super::opstat_on() {
            eprintln!("[fuse] refused: {}", refusal_text(ir, why));
        }
        bump(1, 1);
        return None;
    }
    match run(ir, writeback, end, end_hints, ropts, env, lim, cov) {
        Ok(b) => {
            bump(0, 1);
            bump(3, ir.ins.len() as u64);
            Some(b)
        }
        Err(why) => {
            if super::super::opstat_on() {
                eprintln!("[fuse] fallback: {why}");
            }
            bump(2, 1);
            None
        }
    }
}

/// The allocator state of the pass (one backward scan, as [`ra`]'s, without loops, homes, calls or randomisation).
struct Fa<'a> {
    ir: &'a TraceIr,
    fused: &'a [bool],
    defines: Vec<bool>,
    is_x: Vec<bool>,
    reg: Vec<u8>,
    slot: Vec<u16>,
    pins: Vec<u16>,
    weak_only: Vec<bool>,
    hint: Vec<u8>,
    gpr: [u32; 16],
    xmm: [u32; 16],
    held_g: u16,
    held_x: u16,
    /// Every register the pass handed out (the allocation's `used` masks).
    used_g: u16,
    used_x: u16,
    avail_g: u16,
    avail_x: u16,
    cur_g: u16,
    cur_x: u16,
    cur_i: usize,
    /// Registers evicted at the current instruction (reloaded behind it: a helper call there need not save them).
    ev_g: u16,
    ev_x: u16,
    /// `ra::RAPOL_CALLSPILL`: values live across an always-taken helper call are evicted there.
    callspill: bool,
    /// The call sites found so far (backwards).
    calls: Vec<CallSite>,
    next_slot: u16,
    snap_first: Vec<u32>,
    snap_last: Vec<u32>,
    /// `(snapshot, value)` pins of the open snapshots.
    open: Vec<(u32, u32)>,
    /// `fusecov` bit 0: a pinned value may be evicted (its open exit maps patched to its spill slot).
    patch_pins: bool,
    /// The first extension slot ([`FUSE_EXT`] or [`FUSE_EXT_COV`]).
    ext: u16,
}

type R<T> = Result<T, &'static str>;

impl Fa<'_> {
    fn holder(&self, x: bool, r: u8) -> u32 {
        if x {
            self.xmm[usize::from(r)]
        } else {
            self.gpr[usize::from(r)]
        }
    }

    fn take_reg(&mut self, v: usize, r: u8) {
        if self.is_x[v] {
            self.used_x |= 1 << r;
            self.xmm[usize::from(r)] = v as u32;
            self.held_x |= 1 << r;
        } else {
            self.used_g |= 1 << r;
            self.gpr[usize::from(r)] = v as u32;
            self.held_g |= 1 << r;
        }
        self.reg[v] = r;
    }

    fn free_reg(&mut self, v: usize) {
        let r = self.reg[v];
        if r == NOREG {
            return;
        }
        if self.is_x[v] {
            self.xmm[usize::from(r)] = NONE;
            self.held_x &= !(1 << r);
        } else {
            self.gpr[usize::from(r)] = NONE;
            self.held_g &= !(1 << r);
        }
        self.reg[v] = NOREG;
    }

    fn rloc(x: bool, r: u8) -> Loc {
        if x {
            Loc::X(r)
        } else {
            Loc::G(r)
        }
    }

    fn set_cur(&mut self, x: bool, r: u8) {
        if x {
            self.cur_x |= 1 << r;
        } else {
            self.cur_g |= 1 << r;
        }
    }

    fn ensure_slot(&mut self, v: usize) -> R<u16> {
        if self.slot[v] != NOSLOT {
            return Ok(self.slot[v]);
        }
        if self.next_slot >= self.ext {
            return Err("spill slots");
        }
        self.slot[v] = self.next_slot;
        self.next_slot += 1;
        Ok(self.slot[v])
    }

    fn pick_free(&self, x: bool, excl: u16, hint: u8) -> Option<u8> {
        let m = if x {
            self.avail_x & !self.held_x & !excl & !self.cur_x
        } else {
            self.avail_g & !self.held_g & !excl & !self.cur_g & GPR_POOL_MASK
        };
        if m == 0 {
            return None;
        }
        if hint < 16 && m >> hint & 1 != 0 {
            return Some(hint);
        }
        Some(m.trailing_zeros() as u8)
    }

    /// Evicts the value with the lowest defining ref (weak snapshot-only values first) of the class; its reload follows the current instruction.
    /// When every candidate is pinned by an open snapshot (`fusecov` bit 0), the lowest pinned value is evicted and the open exit maps that name its
    /// register are patched to its spill slot (stored at its definition, so it holds the value at every guard of those snapshots).
    fn try_evict(
        &mut self,
        x: bool,
        excl: u16,
        out: &mut [InsAlloc],
        snaps: &mut [Vec<Loc>],
    ) -> R<u8> {
        let (avail, cur) = if x {
            (self.avail_x, self.cur_x)
        } else {
            (self.avail_g & GPR_POOL_MASK, self.cur_g)
        };
        let mut cand = avail & !excl & !cur;
        let mut best: Option<(i64, u8)> = None;
        let mut best_pinned: Option<(i64, u8)> = None;
        while cand != 0 {
            let r = cand.trailing_zeros() as u8;
            cand &= cand - 1;
            let h = self.holder(x, r);
            if h == NONE {
                continue;
            }
            let v = h as usize;
            let mut sc = v as i64;
            if self.weak_only[v] {
                sc -= 1_000_000;
            }
            if self.pins[v] > 0 {
                bump(5, 1);
                if best_pinned.is_none_or(|(b, _)| sc < b) {
                    best_pinned = Some((sc, r));
                }
                continue;
            }
            if best.is_none_or(|(b, _)| sc < b) {
                best = Some((sc, r));
            }
        }
        let r = match (best, best_pinned) {
            (Some((_, r)), _) => r,
            (None, Some((_, r))) if self.patch_pins => {
                let v = self.holder(x, r) as usize;
                let slot = self.ensure_slot(v)?;
                let reg = Self::rloc(x, r);
                let vr = (v + usize::from(REF_BIAS)) as Ref;
                let mut k = 0;
                while k < self.open.len() {
                    let (s, pv) = self.open[k];
                    if pv as usize != v {
                        k += 1;
                        continue;
                    }
                    // (MUTANT `fusecov_patch_skip`: the open exit maps keep the evicted register)
                    if !mutants::active("fusecov_patch_skip") {
                        for (e, l) in self
                            .ir
                            .snap_entries(s)
                            .iter()
                            .zip(snaps[s as usize].iter_mut())
                        {
                            if e.r == vr && *l == reg {
                                *l = Loc::Spill(slot);
                            }
                        }
                    }
                    self.open.swap_remove(k);
                }
                self.pins[v] = 0;
                r
            }
            _ => return Err("no victim"),
        };
        let v = self.holder(x, r) as usize;
        let slot = self.ensure_slot(v)?;
        if !mutants::active("asmfuse_no_reload") {
            out[self.cur_i].post.push(Reload {
                reg: Self::rloc(x, r),
                slot,
                r: (v + usize::from(REF_BIAS)) as Ref,
            });
        }
        self.free_reg(v);
        if x {
            self.ev_x |= 1 << r;
        } else {
            self.ev_g |= 1 << r;
        }
        Ok(r)
    }

    /// `RAPOL_CALLSPILL`: every unpinned value in a register the helper call destroys is evicted at the call.
    fn spill_across_call(&mut self, out: &mut [InsAlloc]) -> R<()> {
        for x in [false, true] {
            let mut m = if x { self.held_x } else { self.held_g & 0x1FF };
            while m != 0 {
                let r = m.trailing_zeros() as u8;
                m &= m - 1;
                let v = self.holder(x, r) as usize;
                if self.pins[v] > 0 {
                    continue;
                }
                let slot = self.ensure_slot(v)?;
                out[self.cur_i].post.push(Reload {
                    reg: Self::rloc(x, r),
                    slot,
                    r: (v + usize::from(REF_BIAS)) as Ref,
                });
                self.free_reg(v);
                if x {
                    self.ev_x |= 1 << r;
                } else {
                    self.ev_g |= 1 << r;
                }
            }
        }
        Ok(())
    }

    fn alloc_temp(
        &mut self,
        x: bool,
        excl: u16,
        hint: u8,
        must: bool,
        out: &mut [InsAlloc],
        snaps: &mut [Vec<Loc>],
    ) -> R<Option<u8>> {
        if let Some(r) = self.pick_free(x, excl, hint) {
            return Ok(Some(r));
        }
        if must {
            return self.try_evict(x, excl, out, snaps).map(Some);
        }
        Ok(None)
    }

    fn end_loc(
        &mut self,
        r: Ref,
        hint: Loc,
        ends: &[Loc],
        refs: &[Ref],
        out: &mut [InsAlloc],
    ) -> R<Loc> {
        if r == NOREF {
            return Err("NOREF at the end");
        }
        if r < REF_BIAS {
            return Ok(Loc::Const(r));
        }
        if let Some(k) = refs.iter().position(|&q| q == r) {
            return Ok(ends[k]);
        }
        let v = idx(r);
        if v >= self.defines.len() || !self.defines[v] {
            return Err("end use of a non-value");
        }
        let x = self.is_x[v];
        if self.reg[v] != NOREG {
            return Ok(Self::rloc(x, self.reg[v]));
        }
        let h = match hint {
            Loc::G(n) if !x => n,
            Loc::X(n) if x => n,
            _ => self.hint[v],
        };
        Ok(match self.alloc_temp(x, 0, h, false, out, &mut [])? {
            Some(reg) => {
                self.take_reg(v, reg);
                Self::rloc(x, reg)
            }
            None => Loc::Spill(self.ensure_slot(v)?),
        })
    }

    /// One instruction, as `ra::Ra::process`: (a) the result, (b) the operands, (c) the exit map at the last guard of a snapshot, (e) the unpin at
    /// its first guard.
    #[inline(never)]
    fn process(&mut self, i: usize, out: &mut [InsAlloc], snap_locs: &mut [Vec<Loc>]) -> R<()> {
        let ir = self.ir;
        let ins = ir.ins[i];
        self.cur_i = i;
        self.cur_g = 0;
        self.cur_x = 0;
        self.ev_g = 0;
        self.ev_x = 0;
        let (mut dmask_g, mut dmask_x) = (0u16, 0u16);
        let mut dst_loc = Loc::None;
        if self.defines[i] {
            let x = self.is_x[i];
            let r = if self.reg[i] != NOREG {
                let r = self.reg[i];
                self.free_reg(i);
                r
            } else {
                self.alloc_temp(x, 0, NOREG, true, out, snap_locs)?
                    .ok_or("pressure")?
            };
            dst_loc = Self::rloc(x, r);
            if x {
                dmask_x = 1 << r;
                self.used_x |= 1 << r;
            } else {
                dmask_g = 1 << r;
                self.used_g |= 1 << r;
            }
            out[i].dst = dst_loc;
            if self.slot[i] != NOSLOT && !mutants::active("asmfuse_no_store") {
                out[i].stores.push(self.slot[i]);
            }
        }
        let live_after = if ra::is_call(ir, &ins) {
            if self.callspill && ra::always_calls(&ins) {
                self.spill_across_call(out)?;
            }
            // (`V9` / `V10` are callee-saved)
            Some((self.held_g & 0x1FF, self.held_x))
        } else {
            None
        };
        let refs = ra::operand_refs(ir, self.fused, i);
        let mut locs = [Loc::None; 3];
        for k in 0..3 {
            let r = refs[k];
            if r == NOREF {
                continue;
            }
            if r < REF_BIAS {
                locs[k] = Loc::Const(r);
                continue;
            }
            if let Some(j) = (0..k).find(|&j| refs[j] == r) {
                locs[k] = locs[j];
                continue;
            }
            let v = idx(r);
            if v >= i || !self.defines[v] {
                return Err("operand not a value defined before its use");
            }
            let x = self.is_x[v];
            if self.reg[v] != NOREG {
                let reg = self.reg[v];
                self.weak_only[v] = false;
                self.set_cur(x, reg);
                locs[k] = Self::rloc(x, reg);
                continue;
            }
            let mut excl = 0u16;
            let mut hint = self.hint[v];
            match dst_loc {
                // (MUTANT `asmfuse_two_addr`: the second operand may get the destination's register)
                Loc::G(d) if !x && k == 1 && !mutants::active("asmfuse_two_addr") => excl = 1 << d,
                Loc::X(d) if x && k == 1 && !mutants::active("asmfuse_two_addr") => excl = 1 << d,
                Loc::G(d) if !x && k == 0 => hint = d,
                Loc::X(d) if x && k == 0 => hint = d,
                _ => {}
            }
            let reg = self
                .alloc_temp(x, excl, hint, true, out, snap_locs)?
                .ok_or("pressure")?;
            self.take_reg(v, reg);
            self.weak_only[v] = false;
            self.set_cur(x, reg);
            locs[k] = Self::rloc(x, reg);
        }
        out[i].ops = locs;
        if let Some((g, x)) = live_after {
            self.calls.push(CallSite {
                at: i,
                live: ra::CallClobber {
                    // (MUTANT `asmfuse_call_live`: the general registers live across the call are not saved)
                    gpr: if mutants::active("asmfuse_call_live") {
                        0
                    } else {
                        g & !self.ev_g
                    },
                    xmm: x & !self.ev_x,
                },
            });
        }
        if ins.flags & F_GUARD != 0 {
            let s = aux_snap(ins.aux);
            let su = s as usize;
            if self.snap_last[su] == i as u32 {
                let multi = self.snap_first[su] != self.snap_last[su];
                let ents = ir.snap_entries(s);
                let mut locs = Vec::with_capacity(ents.len());
                for e in ents {
                    let loc = if e.r == NOREF {
                        Loc::None
                    } else if e.r < REF_BIAS {
                        Loc::Const(e.r)
                    } else {
                        let v = idx(e.r);
                        if v >= i || !self.defines[v] {
                            return Err("snapshot ref unusable");
                        }
                        let x = self.is_x[v];
                        if self.reg[v] != NOREG {
                            if multi {
                                self.pins[v] += 1;
                                self.open.push((s, v as u32));
                            }
                            Self::rloc(x, self.reg[v])
                        } else if self.slot[v] != NOSLOT {
                            Loc::Spill(self.slot[v])
                        } else {
                            let excl = if x {
                                dmask_x | self.cur_x
                            } else {
                                dmask_g | self.cur_g
                            };
                            match self.alloc_temp(x, excl, self.hint[v], false, out, snap_locs)? {
                                Some(r) => {
                                    self.take_reg(v, r);
                                    self.weak_only[v] = true;
                                    if multi {
                                        self.pins[v] += 1;
                                        self.open.push((s, v as u32));
                                    }
                                    Self::rloc(x, r)
                                }
                                None => Loc::Spill(self.ensure_slot(v)?),
                            }
                        }
                    };
                    locs.push(loc);
                }
                snap_locs[su] = locs;
            }
            if cfg!(debug_assertions) && self.snap_last[su] != i as u32 {
                // the exit map was fixed at the snapshot's last guard: every register it names still holds its value at this guard (the pins)
                for (e, l) in ir.snap_entries(s).iter().zip(&snap_locs[su]) {
                    if let (Loc::G(r) | Loc::X(r), true) = (*l, TraceIr::is_ins(e.r)) {
                        assert_eq!(
                            self.reg[idx(e.r)],
                            r,
                            "fused exit map of snapshot {s} moved at guard {i}"
                        );
                    }
                }
            }
            if self.snap_first[su] == i as u32 && self.snap_last[su] != i as u32 {
                let mut k = 0;
                while k < self.open.len() {
                    if self.open[k].0 == s {
                        let v = self.open[k].1 as usize;
                        self.pins[v] -= 1;
                        self.open.swap_remove(k);
                    } else {
                        k += 1;
                    }
                }
            }
        }
        Ok(())
    }

    /// A double register holding no live value here and not used by the current instruction (the highest, as the old whole-trace scan).
    fn free_x_here(&self) -> Option<u8> {
        (0..16u8)
            .rev()
            .find(|&x| (self.held_x | self.cur_x) >> x & 1 == 0)
    }
}

#[inline(never)]
fn run(
    ir: &TraceIr,
    writeback: &[SnapEntry],
    end: &[Ref],
    end_hints: &[Loc],
    ropts: &RaOpts,
    env: &AsmEnv,
    lim: u32,
    cov: u32,
) -> R<FuseBody> {
    let n = ir.ins.len();
    let ns = ir.snaps.len();
    let fused = ra::fusion(ir, end, ropts);
    let mut defines = Vec::with_capacity(n);
    let mut is_x = Vec::with_capacity(n);
    let rbp = ropts.xregs & 1 != 0 || ropts.pol & ra::RAPOL_RBP != 0;
    let mut avail_g: u16 = 0x17F | (u16::from(rbp) << 9);
    if let Some(r) = ropts.reserve_gpr {
        avail_g &= !(1u16 << r);
    }
    let mut avail_x: u16 = 0xFFFF;
    let mut snap_first = vec![u32::MAX; ns];
    let mut snap_last = vec![u32::MAX; ns];
    for (i, ins) in ir.ins.iter().enumerate() {
        defines.push(ra::defines(ins.op, fused[i]));
        is_x.push(ins.t == Ty::Num);
        let c = ra::op_clobbers(ins, ir);
        avail_g &= !c.gpr;
        avail_x &= !c.xmm;
        if ins.flags & F_GUARD != 0 {
            let s = aux_snap(ins.aux) as usize;
            if s >= ns {
                return Err("snapshot out of range");
            }
            if snap_first[s] == u32::MAX {
                snap_first[s] = i as u32;
            }
            snap_last[s] = i as u32;
        }
    }
    if lim >= 2 {
        // `asmfuse=2` (tests): three general and four double registers, so that evictions, spills, reloads and pinned exit maps are common
        let mut k = 0;
        let mut m = 0u16;
        for r in 0..16 {
            if avail_g >> r & 1 != 0 && k < 3 {
                m |= 1 << r;
                k += 1;
            }
        }
        avail_g = m;
        avail_x &= 0x00F0;
    }
    let ext = if cov & COV_ROOM != 0 {
        FUSE_EXT_COV
    } else {
        FUSE_EXT
    };
    let mut fa = Fa {
        ir,
        fused: &fused,
        defines,
        is_x,
        reg: vec![NOREG; n],
        slot: vec![NOSLOT; n],
        pins: vec![0; n],
        weak_only: vec![false; n],
        hint: vec![NOREG; n],
        gpr: [NONE; 16],
        xmm: [NONE; 16],
        held_g: 0,
        held_x: 0,
        used_g: 0,
        used_x: 0,
        avail_g,
        avail_x,
        cur_g: 0,
        cur_x: 0,
        cur_i: n,
        ev_g: 0,
        ev_x: 0,
        callspill: ropts.pol & ra::RAPOL_CALLSPILL != 0,
        calls: Vec::new(),
        next_slot: 0,
        snap_first,
        snap_last,
        open: Vec::new(),
        patch_pins: cov & COV_ROOM != 0,
        ext,
    };
    let al0 = Allocation {
        fused: fused.clone(),
        nspill: ext,
        ..Allocation::default()
    };
    let mut cx = Cx::new(ir, &al0, &[], env);
    cx.fbody = true;
    // (as `assemble_dl_in`: a rolled root with entry bookkeeping whose guards exit to the head's dispatch)
    cx.first_flag =
        env.head != 0 && (0..n).any(|i| cx.entry_kind(i) == EntryKind::FirstIter);
    cx.p.ops = Vec::with_capacity(32);
    cx.fal.resize_with(n, InsAlloc::default);
    cx.fsnap = vec![Vec::new(); ns];
    // R1 (`regcarry`): a link that hands values over in registers reads the allocation's end locations in its own way: not converted
    if let Some(sd) = env.side.filter(|s| s.link_cell != 0) {
        if cx.carry_plan(sd, writeback).is_some() {
            return Err("carrying link");
        }
    }
    // the end of the trace: the writeback values, first (the scan is backwards)
    let mut end_locs: Vec<Loc> = Vec::with_capacity(end.len());
    for (k, &r) in end.iter().enumerate() {
        let h = end_hints.get(k).copied().unwrap_or(Loc::None);
        let l = fa.end_loc(r, h, &end_locs, &end[..k], &mut cx.fal)?;
        end_locs.push(l);
    }
    let loop_l = cx.label();
    cx.shape_dropped = shape_dropped(ir, env.shape_min);
    // B2 (`b2tag`): the known node tags are a forward fact of the straight-line code; `HSTORE` reads them: taken in a forward scan of the IR
    let mut ktags: Vec<(usize, Vec<(Ref, u8)>)> = Vec::new();
    if env.b2_tag && ir.ins.iter().any(|x| x.op == TOp::Hstore) {
        for i in 0..n {
            if ir.ins[i].op == TOp::Hstore {
                ktags.push((i, cx.ktag.clone()));
            }
            cx.track_tags(i);
        }
        cx.ktag.clear();
    }
    let mut rev: Vec<Op> = Vec::with_capacity(8 * n + 16);
    // per instruction: (length of `rev` behind its group, offset of its code inside the group behind the R2 room checks)
    let mut at_rev: Vec<(u32, u32)> = vec![(0, 0); n];
    // the placeholder guard of a native self-call / return assembles to nothing (`assemble_dl_in` emits the push / the return's guards)
    let placeholder = (ir.rec_call.is_some() || ir.rec_ret.is_some()).then_some(n.wrapping_sub(1));
    // an instruction whose emission waits for the one in front of it (B4-3: the `HLOAD` behind its `HREFK`: the pair is emitted forwards, the
    // `HREFK` decides the fusion with both allocated)
    let mut held: Option<usize> = None;
    for i in (0..n).rev() {
        fa.process(i, &mut cx.fal, &mut cx.fsnap)?;
        let pair = i > 0
            && ir.ins[i].op == TOp::Hload
            && ir.ins[i - 1].op == TOp::Hrefk
            && ir.ins[i].a == REF_BIAS + (i - 1) as Ref;
        if pair {
            held = Some(i);
            continue;
        }
        cx.p.ops.clear();
        let last = held.take().unwrap_or(i);
        for j in i..=last {
            if placeholder == Some(j) {
                if !(ir.ins[j].op == TOp::Eq && ir.ins[j].flags & F_GUARD != 0) {
                    return Err("native trace without its placeholder guard");
                }
                at_rev[j].1 = cx.p.ops.len() as u32;
                continue;
            }
            cx.ffree_x = (ir.ins[j].op == TOp::Fcmp).then(|| fa.free_x_here());
            cx.fcall = fa.calls.last().copied().filter(|c| c.at == j);
            if ir.ins[j].op == TOp::Hstore {
                if let Some(k) = ktags.iter().position(|e| e.0 == j) {
                    cx.ktag = std::mem::take(&mut ktags[k].1);
                }
            }
            cx.side_room_block(j).map_err(|_| "room checks")?;
            at_rev[j].1 = cx.p.ops.len() as u32;
            cx.pre(j);
            cx.ins(j, loop_l).map_err(|_| "emitter")?;
            cx.post(j);
        }
        rev.extend(cx.p.ops.drain(..).rev());
        for e in &mut at_rev[i..=last] {
            e.0 = rev.len() as u32;
        }
    }
    if held.is_some() {
        return Err("held instruction");
    }
    if !fa.open.is_empty() {
        return Err("snapshot left open");
    }
    bump(4, u64::from(fa.next_slot));
    let total = rev.len() as u32;
    let at: Vec<u32> = at_rev.iter().map(|&(e, o)| total - e + o).collect();
    rev.reverse();
    cx.p.ops = rev;
    let snap_locs = std::mem::take(&mut cx.fsnap);
    // snapshots no guard uses keep an empty map in the old allocation too: their entries are `None` there
    let snap_locs: Vec<Vec<Loc>> = snap_locs
        .into_iter()
        .enumerate()
        .map(|(s, v)| {
            if v.is_empty() {
                vec![Loc::None; ir.snap_entries(s as u32).len()]
            } else {
                v
            }
        })
        .collect();
    let used = [u32::from(fa.used_g), u32::from(fa.used_x)];
    let mut calls = std::mem::take(&mut fa.calls);
    calls.reverse();
    drop(fa);
    let st = cx.take_state();
    Ok(FuseBody {
        body: FusedBody { st, at },
        fal: std::mem::take(&mut cx.fal),
        snap_locs,
        fused,
        end_locs,
        used,
        calls,
        ext,
    })
}

impl FuseBody {
    /// The allocation of the pass in the old form, for the stages that read one (`build_exits_ex`, the shape hash, dumps). Call once.
    pub fn allocation(&mut self) -> Allocation {
        Allocation {
            ins: std::mem::take(&mut self.fal),
            fused: std::mem::take(&mut self.fused),
            snap_locs: std::mem::take(&mut self.snap_locs),
            end_locs: std::mem::take(&mut self.end_locs),
            nspill: self.ext,
            backedge_after: 0,
            used: std::cell::OnceCell::from(self.used),
            calls: std::mem::take(&mut self.calls),
            ..Allocation::default()
        }
    }

    /// The body for [`assemble_dl_in`] (call after [`FuseBody::allocation`]).
    pub(crate) fn into_body(self) -> FusedBody {
        self.body
    }
}
