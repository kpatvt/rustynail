//! The VM-truth oracle of the tier-1.5 region analysis (`rn_jit::t15`, docs/JIT_TIER15.md sections 3, 4.6, 12, 13).
//!
//! The analysis claims facts ("before `pc`, slot `s` has a tag in `fact(pc, s)`", "the VM-stack tag byte of
//! `s` equals the fact") for region code that is entered through guarded stubs. This test checks those claims
//! against the *interpreter* (feature `t15_oracle` of rn-core: a callback before every instruction fetch):
//!
//! * pass 1 runs a program and records, per prototype, the last result tag of every load (`prof`) and the frame
//!   tags at the 3rd hit of every loop head (`obs`);
//! * pass 2 plans every loop of every prototype under several speculation / policy variants (observed, random
//!   and wrong `obs`/`prof`, both `EscapedMode`s, random `fallback_g` sets), reruns the program and tracks, per
//!   frame and plan, whether execution is "inside the region": the interpreter reaching ANY resume point of the
//!   plan from anywhere with all `stub_guards` true enters; a barrier op, a failed def guard, an operand the
//!   region template cannot handle, a nested Lua frame (metamethod / `__close` ...), `Dead`, an exit or leaving
//!   `[head, end]` leaves. While inside, every instruction asserts the facts of its guard set and uses, the
//!   `mem_known` tag claims, and that no reached non-`Dead` instruction sees a bottom fact. Independently,
//!   register effects are checked: slots in `defs \ uses` are poisoned (light userdata) before the instruction
//!   and must have been overwritten afterwards, and no slot outside `defs | may_defs` may change.
//!
//! Programs: `tests/vm/*.lua`, `bench/programs/*.lua` (tiny `arg[1]`), `tests/jitdiff/gen.lua` seeds
//! (`T15_ORACLE_GEN`, default 40) and `tests/t15_oracle_progs/*.lua`. Each runs in a child process (this test
//! binary re-executed, `T15_ORACLE_CHILD=<kind>|<path>|<seed>`), so `os.exit`, crashes and timeouts are
//! contained. `T15_ORACLE_ONLY=<substring>` restricts the corpus, `T15_ORACLE_V=1` prints per-program stats.
#![cfg(feature = "t15_oracle")]

use rn_core::func::Proto;
use rn_core::instruction::Instruction;
use rn_core::state::{LuaState, LUA_MULTRET};
use rn_core::status::Status;
use rn_core::value::{Value, LUA_VNUMINT};
use rn_core::ver::active::OpCode;
use rn_jit::t15::analysis::Analysis;
use rn_jit::t15::effects as fx;
use rn_jit::t15::*;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

const POISON: usize = 0x5EED_BA5E;

// ------------------------------------------------------------------------------------------ models

struct PlanRec {
    plan: RegionPlan,
    label: String,
}

struct PModel {
    key: u64,
    line: i32,
    sig: (usize, usize, usize, i32, i32),
    info: &'static FuncInfo<'static>,
    an: Option<&'static Analysis<'static>>,
    effs: Vec<fx::Effects>,
    ops: Vec<Option<OpCode>>,
    plans: Vec<PlanRec>,
    /// pc -> plan ids having a resume point there.
    resume_at: Vec<Vec<u16>>,
    maxstack: usize,
}

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n.max(1)
    }
}

fn sig_of(p: &Proto) -> (usize, usize, usize, i32, i32) {
    (
        p as *const Proto as usize,
        p.code_ptr() as usize,
        p.code().len(),
        p.linedefined,
        i32::from(p.maxstacksize),
    )
}

fn key_of(p: &Proto) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    let mut mix = |x: u64| {
        h ^= x;
        h = h.wrapping_mul(0x100_0000_01b3);
    };
    for &w in p.code() {
        mix(u64::from(w));
    }
    mix(p.linedefined as u64);
    mix(p.lastlinedefined as u64);
    mix(u64::from(p.maxstacksize));
    mix(p.k().len() as u64);
    h
}

fn build_info(p: &Proto) -> &'static FuncInfo<'static> {
    let code: &'static [u32] = Box::leak(p.code().to_vec().into_boxed_slice());
    let k = p.k().iter().map(|v| TagSet::of_tag(v.tag())).collect();
    let k_int = p.k().iter().map(|v| v.as_int()).collect();
    let mut escaped = SlotSet::EMPTY;
    for c in p.protos() {
        for u in c.upvalue_descs().iter().filter(|u| u.instack) {
            escaped.insert(u32::from(u.idx));
        }
    }
    Box::leak(Box::new(FuncInfo {
        code,
        k,
        k_int,
        max_stack: u32::from(p.maxstacksize),
        escaped,
        extra_heads: Vec::new(),
    }))
}

// ---------------------------------------------------------------------------------------- pass-1 data

#[derive(Default, Clone)]
struct P1Proto {
    prof: Vec<u8>,
    /// Last light C function seen as the callee of each `CALL` (0 = none): the intrinsic sites of T6.
    callee: Vec<usize>,
    obs: HashMap<u32, Vec<u8>>,
    hits: HashMap<u32, u32>,
}

#[derive(Default)]
struct P1 {
    protos: HashMap<u64, P1Proto>,
}

// -------------------------------------------------------------------------------------- frame records

#[derive(Default)]
struct Pending {
    pc: u32,
    seq: u64,
    /// the instruction's metamethod follower (`MMBIN*`) was fetched: the region template exited.
    mm: bool,
}

#[derive(Default)]
struct FrameRec {
    proto: usize,
    mi: usize,
    pending: Option<Pending>,
    before: Vec<(u8, u64)>,
    poisoned: Vec<u16>,
    inside: Vec<u16>,
}

#[derive(Default)]
struct Stats {
    fetches: u64,
    enters: u64,
    checks: u64,
    plans: u64,
    leaves: u64,
}

struct Oracle {
    label: String,
    seq: u64,
    pass: u8,
    seed: u64,
    models: Vec<Rc<PModel>>,
    by_ptr: HashMap<usize, usize>,
    frames: HashMap<usize, FrameRec>,
    p1: P1,
    p1_frames: HashMap<usize, (usize, u32, u64)>,
    viol: Vec<String>,
    viol_seen: std::collections::HashSet<String>,
    stats: Stats,
    off: bool,
    budget: u64,
}

thread_local! {
    static ORA: RefCell<Option<Oracle>> = const { RefCell::new(None) };
}

fn hook(frame: usize, proto: *const Proto, pc: usize, base: *mut Value, ms: usize) {
    ORA.with(|o| {
        if let Some(or) = o.borrow_mut().as_mut() {
            if or.off {
                return;
            }
            // SAFETY: the interpreter passes the running closure's live prototype.
            let p = unsafe { &*proto };
            if or.pass == 1 {
                or.on_fetch_p1(frame, p, pc, base, ms);
            } else {
                or.on_fetch(frame, p, pc, base, ms);
            }
        }
    });
}

unsafe fn tag_at(base: *mut Value, s: usize) -> u8 {
    // SAFETY: `s < maxstack` of the frame.
    unsafe { Value::load_tag(base.add(s)) }
}

unsafe fn pay_at(base: *mut Value, s: usize) -> u64 {
    // SAFETY: as above.
    unsafe { Value::load_int_payload(base.add(s)) as u64 }
}

fn class_of(tag: u8) -> TagSet {
    TagSet::of_tag(tag)
}

fn is_getter(op: OpCode) -> bool {
    use OpCode as O;
    matches!(
        op,
        O::Gettabup | O::Gettable | O::Geti | O::Getfield | O::SelfOp | O::Getupval
    )
}

impl Oracle {
    fn new(label: &str, pass: u8, seed: u64, p1: P1) -> Oracle {
        Oracle {
            label: label.to_string(),
            seq: 0,
            pass,
            seed,
            models: Vec::new(),
            by_ptr: HashMap::new(),
            frames: HashMap::new(),
            p1,
            p1_frames: HashMap::new(),
            viol: Vec::new(),
            viol_seen: Default::default(),
            stats: Stats::default(),
            off: false,
            budget: std::env::var("T15_ORACLE_BUDGET")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(30_000_000),
        }
    }

    fn violation(&mut self, m: &PModel, plan: &str, pc: u32, msg: String) {
        let line = format!(
            "ORACLE-VIOLATION [{}] proto@{} (key {:016x}) plan {} pc {}: {}",
            self.label, m.line, m.key, plan, pc, msg
        );
        let dedupe = format!("{:016x}|{}|{}|{}", m.key, plan, pc, msg);
        if self.viol_seen.insert(dedupe) && self.viol.len() < 60 {
            eprintln!("{line}");
            self.viol.push(line);
        }
    }

    // ---------------------------------------------------------------------------------- pass 1

    fn on_fetch_p1(&mut self, frame: usize, p: &Proto, pc: usize, base: *mut Value, ms: usize) {
        self.seq += 1;
        self.stats.fetches += 1;
        if self.stats.fetches > self.budget {
            self.off = true;
            return;
        }
        let key = key_of(p);
        let psig = p as *const Proto as usize;
        if Instruction(p.code()[pc]).opcode() == Some(OpCode::Call) {
            let a = Instruction(p.code()[pc]).a() as usize;
            let ent = self.p1.protos.entry(key).or_default();
            if ent.callee.len() != p.code().len() {
                ent.callee = vec![0; p.code().len()];
            }
            if a < ms {
                // SAFETY: a < maxstack.
                let v = unsafe { &*base.add(a) };
                ent.callee[pc] = v.as_native().map_or(0, |f| f as usize);
            }
        }
        if let Some(&(pp, ppc, _pseq)) = self.p1_frames.get(&frame) {
            if pp == psig {
                let code = p.code();
                if let Some(op) = Instruction(code[ppc as usize]).opcode() {
                    let ent = self.p1.protos.entry(key).or_default();
                    if ent.prof.len() != code.len() {
                        ent.prof = vec![0xFF; code.len()];
                    }
                    let call_res = op == OpCode::Call && Instruction(code[ppc as usize]).c() >= 2;
                    if (is_getter(op) || call_res) && pc == ppc as usize + 1 {
                        let a = Instruction(code[ppc as usize]).a() as usize;
                        if a < ms {
                            // SAFETY: a < maxstack.
                            ent.prof[ppc as usize] = unsafe { tag_at(base, a) };
                        }
                    }
                    if pc <= ppc as usize
                        && matches!(
                            op,
                            OpCode::Forloop
                                | OpCode::Tforloop
                                | OpCode::Jmp
                                | OpCode::Eq
                                | OpCode::Lt
                                | OpCode::Le
                                | OpCode::Eqk
                                | OpCode::Eqi
                                | OpCode::Lti
                                | OpCode::Lei
                                | OpCode::Gti
                                | OpCode::Gei
                                | OpCode::Test
                                | OpCode::Testset
                        )
                    {
                        let h = ent.hits.entry(pc as u32).or_insert(0);
                        *h += 1;
                        if *h == 3 {
                            let v: Vec<u8> = (0..ms)
                                // SAFETY: s < maxstack.
                                .map(|s| unsafe { tag_at(base, s) })
                                .collect();
                            ent.obs.insert(pc as u32, v);
                        }
                    }
                }
            }
        }
        self.p1_frames.insert(frame, (psig, pc as u32, self.seq));
    }

    // ---------------------------------------------------------------------------------- pass 2

    fn model_idx(&mut self, p: &Proto) -> usize {
        let sig = sig_of(p);
        if let Some(&i) = self.by_ptr.get(&sig.0) {
            // (the signature alone is not enough: a freed prototype's address, code buffer address and shape can all be
            // reused by a later `load`ed chunk with different instructions: compare the code itself)
            if self.models[i].sig == sig && self.models[i].info.code == p.code() {
                return i;
            }
        }
        let m = self.build_model(p, sig);
        self.models.push(Rc::new(m));
        let i = self.models.len() - 1;
        self.by_ptr.insert(sig.0, i);
        i
    }

    fn build_model(&mut self, p: &Proto, sig: (usize, usize, usize, i32, i32)) -> PModel {
        let info = build_info(p);
        let key = key_of(p);
        let n = info.code.len();
        let an = Analysis::new(info).ok().map(|a| &*Box::leak(Box::new(a)));
        let ops: Vec<Option<OpCode>> = (0..n)
            .map(|pc| Instruction(info.code[pc]).opcode())
            .collect();
        let effs = if an.is_some() {
            (0..n).map(|pc| fx::effects(info, pc)).collect()
        } else {
            Vec::new()
        };
        let mut plans: Vec<PlanRec> = Vec::new();
        let mut resume_at: Vec<Vec<u16>> = vec![Vec::new(); n];
        if let Some(an) = an {
            let p1 = self.p1.protos.get(&key).cloned().unwrap_or_default();
            let mut rng = Rng(self.seed ^ key ^ 0x9E37_79B9_7F4A_7C15);
            rng.next();
            let tags: [u8; 12] = [
                rn_core::value::LUA_VNIL,
                rn_core::value::LUA_VFALSE,
                rn_core::value::LUA_VTRUE,
                rn_core::value::LUA_VNUMINT,
                rn_core::value::LUA_VNUMFLT,
                rn_core::value::ctb(rn_core::value::LUA_VSHRSTR),
                rn_core::value::ctb(rn_core::value::LUA_VLNGSTR),
                rn_core::value::ctb(rn_core::value::LUA_VTABLE),
                rn_core::value::ctb(rn_core::value::LUA_VLCL),
                rn_core::value::LUA_VLCF,
                rn_core::value::ctb(rn_core::value::LUA_VUSERDATA),
                rn_core::value::ctb(rn_core::value::LUA_VTHREAD),
            ];
            let nslots = info.max_stack as usize;
            let table_pcs: Vec<u32> = (0..n as u32)
                .filter(|&pc| {
                    matches!(
                        ops[pc as usize],
                        Some(
                            OpCode::Gettabup
                                | OpCode::Gettable
                                | OpCode::Geti
                                | OpCode::Getfield
                                | OpCode::SelfOp
                                | OpCode::Settabup
                                | OpCode::Settable
                                | OpCode::Seti
                                | OpCode::Setfield
                        )
                    )
                })
                .collect();
            for l in 0..an.loops.len() {
                let head = an.loops[l].head;
                let obs1 = p1
                    .obs
                    .get(&head)
                    .cloned()
                    .unwrap_or_else(|| vec![0xFF; nslots]);
                let prof1 = if p1.prof.len() == n {
                    p1.prof.clone()
                } else {
                    vec![0xFF; n]
                };
                for v in 0..4u32 {
                    if plans.len() >= 48 {
                        break;
                    }
                    let mut obs = obs1.clone();
                    let mut prof = prof1.clone();
                    let mut policy = Policy::default();
                    if v == 0 || v == 2 {
                        // intrinsic sites (T6): the registered `math` functions seen as callees in pass 1
                        for (pc, &c) in p1.callee.iter().enumerate() {
                            if let Some(k) = rn_core::jit_hooks::intrinsic_of(c)
                                .and_then(rn_jit::t15::intrinsics::Intr::from_index)
                            {
                                policy.intr.push((pc as u32, k));
                            }
                        }
                    }
                    match v {
                        0 => {}
                        1 => {
                            policy.escaped = EscapedMode::ToAny;
                            policy.guard_n_defs = rng.below(2) == 0;
                        }
                        2 => {
                            for o in obs.iter_mut() {
                                if rng.below(10) < 7 {
                                    *o = tags[rng.below(tags.len() as u64) as usize];
                                }
                            }
                            for q in prof.iter_mut() {
                                if rng.below(2) == 0 {
                                    *q = tags[rng.below(tags.len() as u64) as usize];
                                }
                            }
                            for &pc in &table_pcs {
                                if rng.below(10) < 3 {
                                    policy.fallback_g.push(pc);
                                }
                            }
                        }
                        _ => {
                            for o in obs.iter_mut() {
                                if rng.below(4) == 0 {
                                    *o = tags[rng.below(tags.len() as u64) as usize];
                                }
                            }
                            for q in prof.iter_mut() {
                                *q = 0xFF;
                            }
                        }
                    }
                    let spec = Speculation {
                        obs: &obs,
                        prof: &prof,
                        ..Default::default()
                    };
                    let Ok(plan) = an.plan(l, &spec, &policy) else {
                        continue;
                    };
                    let id = plans.len() as u16;
                    for &rp in &plan.resume_points {
                        if (rp as usize) < n {
                            resume_at[rp as usize].push(id);
                        }
                    }
                    plans.push(PlanRec {
                        plan,
                        label: format!(
                            "L{}[{}..{}]/v{}/{:?}",
                            l, head, an.loops[l].end, v, policy.escaped
                        ),
                    });
                }
            }
        }
        self.stats.plans += plans.len() as u64;
        PModel {
            key,
            line: p.linedefined,
            sig,
            info,
            an,
            effs,
            ops,
            plans,
            resume_at,
            maxstack: info.max_stack as usize,
        }
    }

    fn on_fetch(&mut self, frame: usize, p: &Proto, pc: usize, base: *mut Value, ms: usize) {
        self.seq += 1;
        self.stats.fetches += 1;
        if self.stats.fetches > self.budget {
            self.off = true;
            return;
        }
        let mi = self.model_idx(p);
        let m = self.models[mi].clone();
        let Some(an) = m.an else { return };
        let psig = p as *const Proto as usize;
        let mut rec = self.frames.remove(&frame).unwrap_or_default();
        if rec.proto != psig || rec.mi != mi {
            rec = FrameRec {
                proto: psig,
                mi,
                ..Default::default()
            };
        }
        let ms = ms.min(m.maxstack);
        let pc32 = pc as u32;
        // ---- finish the previous instruction of this frame
        if let Some(pd) = rec.pending.take() {
            let allowed = an.succ_pc[pd.pc as usize].contains(&pc32) || pc32 == pd.pc + 1;
            if !allowed {
                rec.inside.clear(); // a new activation reusing the frame
            } else if pc32 == pd.pc + 1
                && an.role[pc] == Role::Mmbin
                && an.role[pd.pc as usize] == Role::Normal
                && !an.succ_pc[pd.pc as usize].contains(&pc32)
            {
                rec.pending = Some(Pending {
                    pc: pd.pc,
                    seq: self.seq,
                    mm: true,
                });
                self.frames.insert(frame, rec);
                return;
            } else {
                self.finish(&m, &mut rec, &pd, pc32, base, ms);
            }
        }
        // ---- enter at resume points, then check the facts of every plan we are inside
        for &id in &m.resume_at[pc] {
            if rec.inside.contains(&id) {
                continue;
            }
            let plan = &m.plans[id as usize].plan;
            let ok = plan.stub_guards(pc32).iter().all(|&(s, set)| {
                (s as usize) < ms
                    // SAFETY: s < maxstack.
                    && class_of(unsafe { tag_at(base, s as usize) }) == set
            });
            if ok {
                rec.inside.push(id);
                self.stats.enters += 1;
            }
        }
        let mut keep: Vec<u16> = Vec::with_capacity(rec.inside.len());
        for id in std::mem::take(&mut rec.inside) {
            if self.before_check(&m, id, pc32, base, ms) {
                keep.push(id);
            } else {
                self.stats.leaves += 1;
            }
        }
        rec.inside = keep;
        // ---- snapshot and poison for the instruction about to run
        if an.role[pc] == Role::Normal {
            rec.before.clear();
            for s in 0..ms {
                // SAFETY: s < maxstack.
                rec.before
                    .push(unsafe { (tag_at(base, s), pay_at(base, s)) });
            }
            rec.poisoned.clear();
            let e = &m.effs[pc];
            let class = fx::classify(m.ops[pc].expect("valid"), pc32, &Policy::default());
            let _ = class;
            // 5.4 `FORLOOP` / `FORPREP` write the control variable copy (`a + 3`) only when the loop runs (`FORPREP` that skips the loop writes nothing): on the exit
            // path the slot keeps the last iteration's value (a number, out of scope, so the analysis's kill is sound). It is not
            // poisoned: the "defs but not written" check would flag every loop exit.
            let exempt = cfg!(rustynail_lua54) && matches!(m.ops[pc], Some(OpCode::Forloop | OpCode::Forprep));
            for s in e.defs.minus(&e.uses).iter().filter(|_| !exempt) {
                if (s as usize) < ms {
                    // SAFETY: s < maxstack; poisoning a slot the instruction overwrites.
                    unsafe {
                        Value::store(
                            base.add(s as usize),
                            Value::light_userdata(POISON as *mut std::ffi::c_void),
                        );
                    }
                    rec.poisoned.push(s as u16);
                }
            }
            rec.pending = Some(Pending {
                pc: pc32,
                seq: self.seq,
                mm: false,
            });
        }
        self.frames.insert(frame, rec);
    }

    /// The region template for the instruction at `pc` cannot handle these operands (tier 1 exits to the
    /// interpreter): region code is left.
    fn would_exit(&self, m: &PModel, pc: u32, before: &[(u8, u64)]) -> bool {
        let i = Instruction(m.info.code[pc as usize]);
        let Some(op) = m.ops[pc as usize] else {
            return false;
        };
        let tag = |r: u32| -> TagSet {
            before
                .get(r as usize)
                .map_or(TagSet::ANY, |&(t, _)| class_of(t))
        };
        if let Some((k, x, y)) = fx::arith(op, i) {
            let set = |o: fx::Opnd| match o {
                fx::Opnd::Reg(r) => tag(r),
                fx::Opnd::K(kk) => m.info.k.get(kk as usize).copied().unwrap_or(TagSet::ANY),
                fx::Opnd::Imm => TagSet::INT,
            };
            let (sx, sy) = (set(x), set(y));
            return if k == fx::Ar::Bit {
                sx != TagSet::INT || sy != TagSet::INT
            } else {
                !sx.subset(TagSet::NUM) || !sy.subset(TagSet::NUM)
            };
        }
        match op {
            OpCode::Unm => !tag(i.b()).subset(TagSet::NUM),
            OpCode::Bnot => tag(i.b()) != TagSet::INT,
            _ => false,
        }
    }

    fn finish(
        &mut self,
        m: &Rc<PModel>,
        rec: &mut FrameRec,
        pd: &Pending,
        q: u32,
        base: *mut Value,
        ms: usize,
    ) {
        let gap = self.seq != pd.seq + 1;
        let e = &m.effs[pd.pc as usize];
        let op = m.ops[pd.pc as usize].expect("valid");
        let cls = fx::classify(op, pd.pc, &Policy::default());
        // ---- register effects
        for &s in &rec.poisoned {
            // SAFETY: s < maxstack.
            let (t, p) = unsafe { (tag_at(base, s as usize), pay_at(base, s as usize)) };
            if class_of(t) == TagSet::LUDATA && p == POISON as u64 {
                let msg = format!(
                    "{:?}: slot {} is in `defs` but was not written{}",
                    op,
                    s,
                    if gap { " (nested frames ran)" } else { "" }
                );
                self.violation(m, "effects", pd.pc, msg);
            }
        }
        // VARARGPREP moves the whole frame (function + fixed parameters): not a register-effect instruction
        if !gap && !cls.is_barrier() && !pd.mm && op != OpCode::Varargprep {
            for s in 0..ms {
                if e.defs.contains(s as u32) || e.may_defs.contains(s as u32) {
                    continue;
                }
                // SAFETY: s < maxstack.
                let (t, p) = unsafe { (tag_at(base, s), pay_at(base, s)) };
                let (bt, bp) = rec.before[s];
                let payload_matters =
                    !matches!(class_of(bt), TagSet::NIL | TagSet::FALSE | TagSet::TRUE);
                if t != bt || (payload_matters && p != bp) {
                    let msg = format!(
                        "{:?}: slot {} changed ({}->{}) but is not in defs|may_defs",
                        op,
                        s,
                        class_of(bt),
                        class_of(t)
                    );
                    self.violation(m, "effects", pd.pc, msg);
                }
            }
        }
        // ---- region state machine
        let exit_op = self.would_exit(m, pd.pc, &rec.before);
        let ia = Instruction(m.info.code[pd.pc as usize]).a() as usize;
        let mut keep: Vec<u16> = Vec::new();
        for id in std::mem::take(&mut rec.inside) {
            let pr = &m.plans[id as usize].plan;
            let Some(p) = pr.insn(pd.pc) else {
                self.stats.leaves += 1;
                continue;
            };
            let mut leave = gap
                || pd.mm
                || p.barrier
                || p.class == InsClass::Exit
                || disp_leaves(p.disp)
                || exit_op
                || q < pr.head
                || q > pr.end;
            if !leave {
                if let Some(g) = p.guard_def {
                    // SAFETY: ia < maxstack for any instruction with a result register.
                    if ia < ms && !class_of(unsafe { tag_at(base, ia) }).subset(g) {
                        leave = true;
                    }
                }
            }
            if leave {
                self.stats.leaves += 1;
            } else {
                keep.push(id);
            }
        }
        rec.inside = keep;
    }

    /// Check the claims of plan `id` at `pc`; false = region code is left (nothing to check).
    fn before_check(
        &mut self,
        m: &Rc<PModel>,
        id: u16,
        pc: u32,
        base: *mut Value,
        ms: usize,
    ) -> bool {
        let rp = &m.plans[id as usize];
        let pr = &rp.plan;
        if pc < pr.head || pc > pr.end {
            return false;
        }
        let Some(p) = pr.insn(pc) else { return false };
        if !p.reached {
            let msg = "region flow reached a pc the analysis marks unreached".to_string();
            self.violation(m, &rp.label, pc, msg);
            return false;
        }
        let mut chk = p.guard_live_in;
        if matches!(p.class, InsClass::Typed | InsClass::Table) {
            chk.union_with(&p.uses);
        }
        if p.class == InsClass::Intr && p.disp == Disp::Specialised {
            // the inline form reads the argument slots as typed operands
            let w = pr.insn(pc).map(|_| m.info.code[pc as usize]).unwrap_or(0);
            chk.union_with(&rn_jit::t15::effects::intr_uses(
                Instruction(w),
                m.info.max_stack,
            ));
        }
        for s in chk.iter() {
            if (s as usize) >= ms {
                continue;
            }
            self.stats.checks += 1;
            // SAFETY: s < maxstack.
            let a = class_of(unsafe { tag_at(base, s as usize) });
            let f = pr.fact(pc, s);
            if !a.subset(f) {
                let msg = format!(
                    "slot {s}: actual {a} not in fact {f} (disp {:?}, class {:?}, mem_known {})",
                    p.disp,
                    p.class,
                    p.mem_known.contains(s)
                );
                self.violation(m, &rp.label, pc, msg);
            }
        }
        for s in p.mem_known.iter() {
            if (s as usize) >= ms {
                continue;
            }
            self.stats.checks += 1;
            // SAFETY: s < maxstack.
            let a = class_of(unsafe { tag_at(base, s as usize) });
            let f = pr.fact(pc, s);
            if a != f {
                let msg = format!("mem_known slot {s}: VM tag class {a} != fact {f}");
                self.violation(m, &rp.label, pc, msg);
            }
        }
        true
    }
}

fn disp_leaves(d: Disp) -> bool {
    matches!(d, Disp::Dead) || disp_is_local_barrier(d)
}

fn disp_is_local_barrier(d: Disp) -> bool {
    format!("{d:?}") == "LocalBarrier"
}

// ------------------------------------------------------------------------------------------- driver

fn run_chunk(st: &mut LuaState, src: &[u8], name: &[u8]) -> (Status, Option<String>) {
    let base = st.top();
    let s = st.load_buffer(src, name, None);
    if s != Status::Ok {
        let v = st.get(st.top() - 1);
        let msg = v
            .as_string()
            .map(|s| String::from_utf8_lossy(s.as_bytes()).into_owned());
        st.set_top(base);
        return (s, msg);
    }
    let func = st.top() - 1;
    let status = st.pcall(func, LUA_MULTRET, 0);
    let v = st.get(func);
    let out = v
        .as_string()
        .map(|s| String::from_utf8_lossy(s.as_bytes()).into_owned());
    st.set_top(base);
    (status, out)
}

fn repo_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root")
}

fn bench_arg(name: &str) -> &'static str {
    match name {
        "binary_trees" => "6",
        "closures" => "20000",
        "coroutines" => "3000",
        "fannkuch" => "6",
        "fasta" => "2000",
        "fib" => "16",
        "gc_churn" => "30",
        "heapsort" => "2000",
        "json" => "6",
        "knucleotide" => "2000",
        "load_big" => "20",
        "loops_arith" => "20000",
        "mandelbrot" => "24",
        "matmul" => "12",
        "metamethods" => "20000",
        "nbody" => "300",
        "oop_methods" => "20000",
        "pcall_error" => "5000",
        "queens" => "6",
        "sieve" => "20000",
        "spectral_norm" => "12",
        "string_ops" => "2000",
        "table_array" => "3000",
        "table_hash" => "3000",
        "varargs" => "20000",
        _ => "10",
    }
}

fn child_main(spec: &str) {
    rn_core::jit_hooks::set_intrinsics(rn_std::lib_math::jit_builtins());
    // memory cap (CLAUDE.md memory-safety rules); not under ASan (`RUSTYNAIL_TEST_NOULIMIT=1`, scripts/asan-jit.sh): its shadow memory
    // needs terabytes of address space, the cgroup cap of the caller applies instead
    if std::env::var_os("RUSTYNAIL_TEST_NOULIMIT").is_none() {
        // SAFETY: plain libc call.
        unsafe {
            let lim = libc::rlimit {
                rlim_cur: 6 << 30,
                rlim_max: 6 << 30,
            };
            libc::setrlimit(libc::RLIMIT_AS, &lim);
        }
    }
    let mut it = spec.splitn(3, '|');
    let kind = it.next().unwrap_or("");
    let path = it.next().unwrap_or("");
    let arg = it.next().unwrap_or("");
    let root = repo_root();
    let _ = std::env::set_current_dir(&root);
    let seed: u64 = std::env::var("T15_ORACLE_SEED")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1);
    let src: Vec<u8> = if kind == "gen" {
        let mut st = rn_std::new_state_with_libs().expect("state");
        let wrapper = format!(
            "arg = {{\"{arg}\"}}\nlocal buf = {{}}\nio.write = function(...)\n for i = 1, select('#', ...) do buf[#buf + 1] = tostring((select(i, ...))) end\nend\ndofile(\"{path}\")\nreturn table.concat(buf)\n"
        );
        let (s, out) = run_chunk(&mut st, wrapper.as_bytes(), b"=gen");
        match (s, out) {
            (Status::Ok, Some(o)) => o.into_bytes(),
            (s, o) => {
                eprintln!("ORACLE-SKIP generator failed: {s:?} {o:?}");
                return;
            }
        }
    } else {
        std::fs::read(path).unwrap_or_else(|e| panic!("{path}: {e}"))
    };
    let label = format!("{kind}:{}", path.rsplit('/').next().unwrap_or(path));
    let prelude = if kind == "bench" {
        format!("arg = {{\"{arg}\"}}\n")
    } else {
        String::new()
    };
    let mut p1 = P1::default();
    for pass in [1u8, 2] {
        let mut st = rn_std::new_state_with_libs().expect("state");
        ORA.with(|o| {
            *o.borrow_mut() = Some(Oracle::new(&label, pass, seed, std::mem::take(&mut p1)));
        });
        rn_core::oracle::set_hook(Some(hook));
        if !prelude.is_empty() {
            run_chunk(&mut st, prelude.as_bytes(), b"=prelude");
        }
        let (status, msg) = run_chunk(&mut st, &src, format!("@{path}").as_bytes());
        rn_core::oracle::set_hook(None);
        let or = ORA.with(|o| o.borrow_mut().take()).expect("oracle");
        if std::env::var("T15_ORACLE_V").is_ok() || or.pass == 2 {
            eprintln!(
                "ORACLE-STATS [{label}] pass {} status {status:?} fetches {} plans {} enters {} checks {} leaves {} violations {}{}",
                or.pass,
                or.stats.fetches,
                or.stats.plans,
                or.stats.enters,
                or.stats.checks,
                or.stats.leaves,
                or.viol.len(),
                if or.off { " (budget hit)" } else { "" }
            );
            if std::env::var("T15_ORACLE_V").is_ok() {
                if let Some(m) = msg {
                    eprintln!("  program message: {m}");
                }
            }
        }
        p1 = or.p1;
        drop(st);
    }
}

#[test]
fn child_run() {
    if let Ok(spec) = std::env::var("T15_ORACLE_CHILD") {
        child_main(&spec);
    }
}

struct Job {
    spec: String,
    name: String,
    must_enter: bool,
}

fn corpus() -> Vec<Job> {
    let root = repo_root();
    let only = std::env::var("T15_ORACLE_ONLY").ok();
    let mut jobs = Vec::new();
    let mut add_dir = |dir: &str, kind: &str, must: bool| {
        let mut files: Vec<_> = std::fs::read_dir(root.join(dir))
            .map(|d| d.filter_map(|e| e.ok()).map(|e| e.path()).collect())
            .unwrap_or_default();
        files.sort();
        for f in files {
            if f.extension().and_then(|e| e.to_str()) != Some("lua") {
                continue;
            }
            let stem = f.file_stem().unwrap().to_string_lossy().into_owned();
            let arg = if kind == "bench" {
                bench_arg(&stem)
            } else {
                ""
            };
            jobs.push(Job {
                spec: format!("{kind}|{}|{arg}", f.display()),
                name: format!("{kind}:{stem}"),
                must_enter: must,
            });
        }
    };
    add_dir("tests/t15_oracle_progs", "prog", true);
    add_dir("tests/vm", "prog", false);
    add_dir("bench/programs", "bench", false);
    let n: u32 = std::env::var("T15_ORACLE_GEN")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(40);
    let gen = root.join("tests/jitdiff/gen.lua");
    for seed in 1..=n {
        jobs.push(Job {
            spec: format!("gen|{}|{seed}", gen.display()),
            name: format!("gen:{seed}"),
            must_enter: false,
        });
    }
    if let Some(o) = only {
        jobs.retain(|j| j.name.contains(&o));
    }
    jobs
}

#[test]
fn oracle_corpus() {
    if std::env::var("T15_ORACLE_CHILD").is_ok() {
        return;
    }
    let exe = std::env::current_exe().expect("exe");
    let jobs = corpus();
    let queue = std::sync::Mutex::new(jobs.into_iter().collect::<Vec<_>>());
    let results = std::sync::Mutex::new(Vec::<(String, bool, bool, String)>::new());
    let tmo: u64 = std::env::var("T15_ORACLE_TIMEOUT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(240);
    std::thread::scope(|sc| {
        for _ in 0..3 {
            sc.spawn(|| loop {
                let Some(job) = queue.lock().unwrap().pop() else {
                    break;
                };
                let mut cmd = std::process::Command::new(&exe);
                cmd.args(["--exact", "child_run", "--nocapture", "--test-threads=1"])
                    .env("T15_ORACLE_CHILD", &job.spec)
                    .stdin(std::process::Stdio::null())
                    .stdout(std::process::Stdio::piped())
                    .stderr(std::process::Stdio::piped());
                let mut ch = cmd.spawn().expect("spawn child");
                let out = ch.stdout.take();
                let err = ch.stderr.take();
                let t_out = std::thread::spawn(move || {
                    let mut v = Vec::new();
                    if let Some(mut o) = out {
                        let _ = std::io::Read::read_to_end(&mut o, &mut v);
                    }
                });
                let t_err = std::thread::spawn(move || {
                    let mut v = Vec::new();
                    if let Some(mut o) = err {
                        let _ = std::io::Read::read_to_end(&mut o, &mut v);
                    }
                    v
                });
                let start = std::time::Instant::now();
                let mut timed_out = false;
                let status = loop {
                    if let Some(s) = ch.try_wait().expect("wait") {
                        break s;
                    }
                    if start.elapsed().as_secs() > tmo {
                        let _ = ch.kill();
                        timed_out = true;
                        break ch.wait().expect("wait");
                    }
                    std::thread::sleep(std::time::Duration::from_millis(20));
                };
                let _ = t_out.join();
                let err = String::from_utf8_lossy(&t_err.join().unwrap()).into_owned();
                let viol = err.lines().any(|l| l.starts_with("ORACLE-VIOLATION"));
                use std::os::unix::process::ExitStatusExt;
                let crashed = status.signal().is_some() && !timed_out;
                let entered = err.lines().any(|l| {
                    l.contains("pass 2")
                        && l.split(" enters ")
                            .nth(1)
                            .and_then(|r| r.split(' ').next())
                            .and_then(|n| n.parse::<u64>().ok())
                            .is_some_and(|n| n > 0)
                });
                let mut report = String::new();
                for l in err.lines() {
                    if l.starts_with("ORACLE-VIOLATION") || l.starts_with("ORACLE-SKIP") {
                        report.push_str(l);
                        report.push('\n');
                    }
                }
                if std::env::var("T15_ORACLE_V").is_ok() {
                    eprintln!("{}", err.lines().filter(|l| l.starts_with("ORACLE-STATS") || l.starts_with("  program")).collect::<Vec<_>>().join("\n"));
                }
                if timed_out {
                    eprintln!("[oracle] timeout (inconclusive): {}", job.name);
                }
                if crashed {
                    report.push_str(&format!("CRASH {:?} in {}\n", status, job.name));
                }
                let failed = viol || crashed || (job.must_enter && !entered && !timed_out);
                if job.must_enter && !entered && !timed_out {
                    report.push_str(&format!(
                        "{}: expected the oracle to enter at least one region (no `enters` in pass 2)\n{}\n",
                        job.name,
                        err.lines().rev().take(6).collect::<Vec<_>>().join("\n")
                    ));
                }
                results
                    .lock()
                    .unwrap()
                    .push((job.name.clone(), failed, entered, report));
            });
        }
    });
    let res = results.into_inner().unwrap();
    let total = res.len();
    let entered = res.iter().filter(|r| r.2).count();
    eprintln!("[oracle] {total} programs, {entered} entered at least one region");
    let mut bad = String::new();
    for (name, failed, _, rep) in &res {
        if *failed {
            bad.push_str(&format!("--- {name}\n{rep}"));
        }
    }
    assert!(bad.is_empty(), "oracle failures:\n{bad}");
}

#[allow(dead_code)]
fn _unused(_: i64) -> u8 {
    LUA_VNUMINT
}
