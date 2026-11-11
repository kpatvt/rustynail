//! J5 P0 vertical slice (docs/JIT_TRACE_REF.md): one hand-written IR trace for `for i = 1, n do s = s + i end`, lowered by
//! a minimal hand-written lowering to micro-ops (no recorder, no runtime, no register allocator: the register map is
//! written by hand), run by the Rust emulator and natively from a test-only entry, and compared with tier 1 on random
//! frames. The count-model API of `Lowered` is exercised on the way.
//!
//! Test-only simplification: the trace's micro-ops are appended to the tier-1 `Program` of the function, so an exit is
//! a plain `Jmp` to a tier-1 label. A production trace is a separate code block and leaves with `Mov V7, label
//! address; JmpR V7` (5.3, TR7b), which needs P0b's emulator support for `JmpR`.
#![cfg(feature = "trace")]

use rn_core::instruction::Instruction;
use rn_core::jit_layout::callinfo as ci;
use rn_core::value::{LUA_VNIL, LUA_VNUMFLT, LUA_VNUMINT};
use rn_core::ver::active::OpCode;
use rn_jit::emu::{self, Env, Stop};
use rn_jit::ir::{Base, Cc, Op, Src, L, M, V, W};
use rn_jit::lower::{lower, LabelKind, LowerCfg, Lowered};
use rn_jit::tr::ir::*;
use std::collections::HashMap;

const FAKE_CODE: usize = 0x10_0000;
const NSLOT: usize = 40;
const PRE: usize = 2;

/// The loop head (the `ADD`) and the pc of the `FORLOOP`.
const H: u32 = 1;
const FORLOOP_PC: u32 = 3;
/// Instructions fetched by one iteration (`ADD`, `FORLOOP`; the `MMBIN` follower is subsumed).
const N_ITER: u32 = 2;

/// `for i = ... do s = s + i end return s`: `r0` iteration count, `r1` step, `r2` control variable `i`, `r4` is `s`.
fn bytecode() -> Vec<Instruction> {
    vec![
        Instruction::abx(OpCode::Forprep, 0, 2),      // 0: skips to 4
        Instruction::abck(OpCode::Add, 4, 4, CTL, 0), // 1: H
        Instruction::abck(OpCode::Mmbin, 4, 2, 6, 0), // 2
        Instruction::abx(OpCode::Forloop, 0, 3),      // 3: back to 1
        Instruction::abck(OpCode::Return1, 4, 2, 0, 0), // 4
        Instruction::abck(OpCode::Return0, 0, 1, 0, 0), // 5
    ]
}

// ---- the hand-written IR (docs/JIT_TRACE_REF.md section 2) --------------------------------------------------------

/// The numeric `for` registers are written with their 5.5 numbers in this file (0 = count / limit, 1 = step, 2 = index /
/// control variable); `ph` maps them to the version's physical slots (5.4 TR30: index `R[A]`, count `R[A+1]`, step
/// `R[A+2]`, and the visible control variable copy `R[A+3]`).
const fn ph(l: u16) -> u16 {
    if cfg!(rustynail_lua54) {
        match l {
            0 => 1,
            1 => 2,
            2 => 0,
            o => o,
        }
    } else {
        l
    }
}

/// The register the loop body reads as the control variable (`ADD s, s, i`).
const CTL: u32 = if cfg!(rustynail_lua54) { 3 } else { 2 };

/// The snapshot entries of the loop state: count, index (+ the 5.4 copy) and the accumulator.
fn loop_state(cnt: Ref, idx: Ref, acc: Ref) -> Vec<SnapEntry> {
    let e = |slot, r| SnapEntry { slot, r, flags: 0 };
    let mut v = vec![e(ph(0), cnt), e(ph(2), idx), e(4, acc)];
    if cfg!(rustynail_lua54) {
        v.push(e(3, idx));
    }
    v.sort_by_key(|x| x.slot);
    v
}

const fn r(n: u16) -> Ref {
    REF_BIAS + n
}

/// Snapshot indices of the slice.
const S0: u32 = 0; // budget check of the pre-roll: state at H, nothing charged
const S0R: u32 = 1; // entry typechecks: state at H, the pre-roll charge taken
const S1: u32 = 2; // FORLOOP guard of the pre-roll
const SH: u32 = 3; // top of the loop (poll, budget): state at H after an iteration
const S2: u32 = 4; // FORLOOP guard of the body

fn ins(op: TOp, t: Ty, flags: u8, a: Ref, b: Ref, aux: u32) -> Ins {
    Ins {
        op,
        t,
        flags,
        a,
        b,
        prev: NOREF,
        aux,
    }
}

/// The trace IR and the hand-made register map (the job of the allocator, P5a). `low` supplies the count model.
fn slice_ir(low: &Lowered) -> (TraceIr, HashMap<Ref, V>) {
    let mut t = TraceIr::default();
    let k0 = t.konst(KVal::Int(0));
    let k1 = t.konst(KVal::Int(1));
    t.head = (ProtoKey(FAKE_CODE), H);
    t.extent = 5;
    t.depth = 0;
    t.counts = vec![N_ITER, N_ITER];
    let g = F_GUARD;
    let list = [
        // pre-roll: entry checks, then one iteration
        ins(
            TOp::Budget,
            Ty::I64,
            g,
            N_ITER as Ref,
            NOREF,
            guard_aux(S0, 0),
        ), // 0
        ins(TOp::Sload, Ty::Int, g, 4, NOREF, guard_aux(S0R, 1)), // 1  s
        ins(TOp::Sload, Ty::Int, g, ph(2), NOREF, guard_aux(S0R, 1)), // 2  i
        ins(TOp::Sload, Ty::Int, g, ph(0), NOREF, guard_aux(S0R, 1)), // 3  remaining count
        ins(
            TOp::Sload,
            Ty::Int,
            g | F_LOOPINV,
            ph(1),
            NOREF,
            guard_aux(S0R, 1),
        ), // 4  step
        ins(TOp::Add, Ty::Int, 0, r(1), r(2), 0),                 // 5  s1 = s + i       (ADD)
        ins(TOp::Ne, Ty::Int, g, r(3), k0, guard_aux(S1, 0)),     // 6  count != 0       (FORLOOP)
        ins(TOp::Sub, Ty::Int, 0, r(3), k1, 0),                   // 7  count1 = count - 1
        ins(TOp::Add, Ty::Int, 0, r(2), r(4), 0),                 // 8  i1 = i + step
        // loop
        ins(TOp::Loop, Ty::Nil, 0, NOREF, NOREF, 0), // 9
        ins(TOp::Phi, Ty::Int, F_PHI, r(5), r(15), 0), // 10 s
        ins(TOp::Phi, Ty::Int, F_PHI, r(8), r(18), 0), // 11 i
        ins(TOp::Phi, Ty::Int, F_PHI, r(7), r(17), 0), // 12 count
        ins(TOp::Poll, Ty::Nil, g, NOREF, NOREF, guard_aux(SH, 0)), // 13
        ins(
            TOp::Budget,
            Ty::I64,
            g,
            N_ITER as Ref,
            NOREF,
            guard_aux(SH, 0),
        ), // 14
        ins(TOp::Add, Ty::Int, 0, r(10), r(11), 0),  // 15 s2 = s + i
        ins(TOp::Ne, Ty::Int, g, r(12), k0, guard_aux(S2, 0)), // 16
        ins(TOp::Sub, Ty::Int, 0, r(12), k1, 0),     // 17
        ins(TOp::Add, Ty::Int, 0, r(11), r(4), 0),   // 18
    ];
    for (n, i) in list.into_iter().enumerate() {
        assert_eq!(t.emit(i), r(n as u16));
    }
    t.loop_at = Some(r(9));
    // The count model (TR5.4): an exit at P stores `vb + n_charged - fetches(P) - land(P)`.
    let fetched_before_forloop = low.fetches(&[H]); // the ADD
    assert_eq!(low.label_kind(FORLOOP_PC), LabelKind::Mid);
    let mid_refund = N_ITER as i32 - fetched_before_forloop as i32 - low.land(FORLOOP_PC) as i32;
    assert_eq!(low.land(H), 0);
    let e = |slot, r| SnapEntry { slot, r, flags: 0 };
    let snap = |pc, land, refund| Snap {
        pc,
        land,
        refund,
        ..Snap::default()
    };
    assert_eq!(t.snapshot(snap(H, Land::Pre, 0), &[]), S0);
    assert_eq!(t.snapshot(snap(H, Land::Pre, N_ITER as i32), &[]), S0R);
    assert_eq!(
        t.snapshot(snap(FORLOOP_PC, Land::Mid, mid_refund), &[e(4, r(5))]),
        S1
    );
    assert_eq!(
        t.snapshot(snap(H, Land::Pre, 0), &loop_state(r(12), r(11), r(10))),
        SH
    );
    assert_eq!(
        t.snapshot(
            snap(FORLOOP_PC, Land::Mid, mid_refund),
            &loop_state(r(12), r(11), r(15))
        ),
        S2
    );
    // Registers: each PHI shares its register with its pre-roll value and its body value (no back-edge moves).
    let regs: HashMap<Ref, V> = [
        (r(1), 1),
        (r(2), 2),
        (r(3), 3),
        (r(4), 4),
        (r(5), 1),
        (r(7), 3),
        (r(8), 2),
        (r(10), 1),
        (r(11), 2),
        (r(12), 3),
        (r(15), 1),
        (r(17), 3),
        (r(18), 2),
    ]
    .into_iter()
    .collect();
    (t, regs)
}

// ---- the hand-written lowering ------------------------------------------------------------------------------------

/// V5 is the budget register (as in region code), V0/V6 scratch.
const VB: V = 5;

fn frame(slot: u16, off: i32) -> M {
    M {
        base: Base::Frame,
        off: i32::from(slot) * 16 + off,
    }
}

const BUDGET_WORD: M = M {
    base: Base::Cnt,
    off: 0,
};

/// Appends the micro-ops of `t` to `low.prog`; returns the label of the trace entry.
fn lower_slice(low: &mut Lowered, t: &TraceIr, regs: &HashMap<Ref, V>, irq: usize) -> L {
    let entry = low.prog.label();
    let loop_l = low.prog.label();
    let reg = |x: Ref| regs[&x];
    let src = |x: Ref| -> Src {
        if x != NOREF && x < REF_BIAS {
            match t.kval(x) {
                KVal::Int(v) => Src::Imm(i32::try_from(v).unwrap()),
                k => panic!("constant {k:?}"),
            }
        } else {
            Src::V(reg(x))
        }
    };
    let mut exits: Vec<(u32, L)> = Vec::new();
    let mut exit_for = |p: &mut rn_jit::ir::Program, snap: u32| -> L {
        if let Some(&(_, l)) = exits.iter().find(|&&(s, _)| s == snap) {
            return l;
        }
        let l = p.label();
        exits.push((snap, l));
        l
    };
    let landing: Vec<(Option<L>, LabelKind)> = (0..low.total as u32)
        .map(|pc| (low.pc_labels[pc as usize], low.label_kind(pc)))
        .collect();
    let p = &mut low.prog;
    p.push(Op::Bind(entry));
    p.push(Op::Ld {
        dst: VB,
        w: W::B4,
        m: BUDGET_WORD,
    });
    let mut phis: Vec<Ins> = Vec::new();
    for (n, i) in t.ins.iter().enumerate() {
        let me = r(n as u16);
        match i.op {
            TOp::Budget => {
                let x = exit_for(p, aux_snap(i.aux));
                p.push(Op::Br {
                    cc: Cc::Lt,
                    a: VB,
                    b: Src::Imm(i32::from(i.a)),
                    to: x,
                });
                p.push(Op::Sub(VB, Src::Imm(i32::from(i.a))));
            }
            TOp::Sload => {
                assert_eq!(i.t, Ty::Int);
                if aux_bits(i.aux) & 1 != 0 {
                    let x = exit_for(p, aux_snap(i.aux));
                    p.push(Op::Ld {
                        dst: 0,
                        w: W::B1,
                        m: frame(i.a, 0),
                    });
                    p.push(Op::Br {
                        cc: Cc::Ne,
                        a: 0,
                        b: Src::Imm(i32::from(LUA_VNUMINT)),
                        to: x,
                    });
                }
                p.push(Op::Ld {
                    dst: reg(me),
                    w: W::B8,
                    m: frame(i.a, 8),
                });
            }
            TOp::Add | TOp::Sub => {
                let (d, a) = (reg(me), reg(i.a));
                if d != a {
                    p.push(Op::MovR(d, a));
                }
                let b = src(i.b);
                assert_ne!(b, Src::V(d));
                p.push(if i.op == TOp::Add {
                    Op::Add(d, b)
                } else {
                    Op::Sub(d, b)
                });
            }
            TOp::Ne => {
                // the guard asserts a != b: leave when equal
                let x = exit_for(p, aux_snap(i.aux));
                p.push(Op::Br {
                    cc: Cc::Eq,
                    a: reg(i.a),
                    b: src(i.b),
                    to: x,
                });
            }
            TOp::Poll => {
                let x = exit_for(p, aux_snap(i.aux));
                p.push(Op::Ld {
                    dst: 0,
                    w: W::B4,
                    m: M {
                        base: Base::Ci,
                        off: ci::TRAP as i32,
                    },
                });
                p.push(Op::Br {
                    cc: Cc::Ne,
                    a: 0,
                    b: Src::Imm(0),
                    to: x,
                });
                p.push(Op::Mov(6, irq as u64));
                p.push(Op::Ld {
                    dst: 0,
                    w: W::B1,
                    m: M {
                        base: Base::Reg(6),
                        off: 0,
                    },
                });
                p.push(Op::Br {
                    cc: Cc::Ne,
                    a: 0,
                    b: Src::Imm(0),
                    to: x,
                });
            }
            TOp::Loop => {
                // PHIs follow the marker: move the pre-roll values into the PHI registers, then bind the loop label
                phis = t.ins[n + 1..]
                    .iter()
                    .take_while(|i| i.op == TOp::Phi)
                    .copied()
                    .collect();
                for (k, ph) in phis.iter().enumerate() {
                    let (dst, from) = (reg(r((n + 1 + k) as u16)), reg(ph.a));
                    if dst != from {
                        p.push(Op::MovR(dst, from));
                    }
                }
                p.push(Op::Bind(loop_l));
            }
            TOp::Phi => {} // handled at LOOP
            op => panic!("slice lowering: {op:?}"),
        }
    }
    // back-edge: body values into the PHI registers (coalesced by the hand allocation: no moves)
    for (k, ph) in phis.iter().enumerate() {
        let dst = reg(r(t.loop_at.unwrap() - REF_BIAS + 1 + k as u16));
        assert_eq!(dst, reg(ph.b), "the hand allocation coalesces PHIs");
    }
    p.push(Op::Jmp(loop_l));
    // exit stubs: restore the snapshot into the stack, store the budget, continue in tier 1 (TR3, TR5.4)
    for (snap, l) in exits {
        let s = t.snaps[snap as usize];
        p.push(Op::Bind(l));
        for e in t.snap_entries(snap) {
            let ty = t.at(e.r).t;
            assert_eq!(ty, Ty::Int);
            p.push(Op::St {
                w: W::B1,
                m: frame(e.slot, 0),
                src: Src::Imm(i32::from(LUA_VNUMINT)),
            });
            p.push(Op::St {
                w: W::B8,
                m: frame(e.slot, 8),
                src: Src::V(reg(e.r)),
            });
        }
        p.push(Op::Add(VB, Src::Imm(s.refund)));
        p.push(Op::St {
            w: W::B4,
            m: BUDGET_WORD,
            src: Src::V(VB),
        });
        let (target, kind) = landing[s.pc as usize];
        // the landing kind recorded in the snapshot is the one the lowering reports
        assert_eq!(
            kind,
            if s.land == Land::Pre {
                LabelKind::Head
            } else {
                LabelKind::Mid
            }
        );
        p.push(Op::Jmp(target.expect("landing label")));
    }
    entry
}

// ---- harness -------------------------------------------------------------------------------------------------------

#[derive(Clone)]
struct Mem {
    frame: Vec<u64>,
    ci: Vec<u64>,
    thread: Vec<u64>,
    k: Vec<u64>,
    cnt: i32,
    irq: Vec<u8>,
}

impl Mem {
    fn new() -> Mem {
        Mem {
            frame: vec![0; (NSLOT + PRE) * 2],
            ci: vec![0; ci::SIZE.div_ceil(8)],
            thread: vec![0; 64],
            k: vec![0; 8],
            cnt: i32::MAX,
            irq: vec![0],
        }
    }
    fn set_reg(&mut self, r: usize, tag: u8, payload: u64) {
        // logical (5.5) numbers of the `for` registers, see `ph`; the 5.4 visible control variable is a copy
        let phys = ph(r as u16) as usize;
        self.frame[(PRE + phys) * 2] = u64::from(tag);
        self.frame[(PRE + phys) * 2 + 1] = payload;
        if cfg!(rustynail_lua54) && r == 2 {
            self.frame[(PRE + 3) * 2] = u64::from(tag);
            self.frame[(PRE + 3) * 2 + 1] = payload;
        }
    }
    fn put<T: Copy>(buf: &mut [u64], off: usize, v: T) {
        assert!(off + std::mem::size_of::<T>() <= buf.len() * 8);
        // SAFETY: bounds checked above; unaligned write inside the buffer.
        unsafe {
            buf.as_mut_ptr()
                .cast::<u8>()
                .add(off)
                .cast::<T>()
                .write_unaligned(v)
        }
    }
    fn setup_ci(&mut self, callstatus: u32, trap: i32) {
        Self::put(&mut self.ci, ci::CALLSTATUS, callstatus);
        Self::put(&mut self.ci, ci::TRAP, trap);
        Self::put(&mut self.ci, ci::PREVIOUS, 0x7000usize);
        Self::put(&mut self.ci, ci::FUNC, 5usize);
        Self::put(&mut self.ci, ci::KCACHE, self.k.as_ptr() as usize);
    }
    fn savedpc(&self) -> u64 {
        // SAFETY: reads inside the ci buffer.
        unsafe {
            self.ci
                .as_ptr()
                .cast::<u8>()
                .add(ci::SAVEDPC)
                .cast::<u64>()
                .read_unaligned()
        }
    }
    fn env(&mut self) -> Env {
        Env {
            frame: self.frame.as_mut_ptr().cast::<u8>().wrapping_add(PRE * 16),
            ci: self.ci.as_mut_ptr().cast(),
            thread: self.thread.as_mut_ptr().cast(),
            k: self.k.as_ptr().cast(),
            cnt: &raw mut self.cnt,
        }
    }
}

fn lowered(irq: *const u8) -> Lowered {
    let words: Vec<u32> = bytecode().iter().map(|i| i.0).collect();
    lower(
        &words,
        LowerCfg {
            calls: None,
            code_addr: FAKE_CODE,
            interrupt_addr: irq as usize,
            consts: &[],
            t15_warmup: None,
        },
    )
    .expect("lowers")
}

/// A fresh `Lowered` for `irq` with the trace appended; returns it with the trace entry label.
fn with_trace(irq: *const u8) -> (Lowered, L) {
    let mut low = lowered(irq);
    let (t, regs) = slice_ir(&low);
    let entry = lower_slice(&mut low, &t, &regs, irq as usize);
    (low, entry)
}

fn tier1_label(low: &Lowered) -> L {
    low.resume
        .iter()
        .find(|(pc, _)| *pc == H)
        .map(|&(_, l)| l)
        .unwrap_or_else(|| low.pc_labels[H as usize].unwrap())
}

fn run_emu(low: &Lowered, m: &mut Mem, start: L) -> u32 {
    let env = m.env();
    // SAFETY: the buffers in `m` are large enough for every offset the templates and the trace use.
    match unsafe { emu::run(&low.prog, start, &env, 10_000_000) } {
        Stop::Ret(c) => c,
        s => panic!("emulator: {s:?}"),
    }
}

#[cfg(not(miri))]
fn run_native(low: &Lowered, m: &mut Mem, start: L) -> u32 {
    use rn_jit::mem::CodeArena;
    use rn_jit::x86::{emit, thunk_code, Thunk};
    let em = emit(&low.prog).expect("assembles");
    let off = em.label_off[start as usize].expect("bound");
    let mut arena = CodeArena::new();
    let tc = thunk_code();
    let mut tb = arena.alloc(tc.len(), 16).unwrap();
    arena.writer(&mut tb).unwrap().write_at(0, &tc);
    let mut cb = arena.alloc(em.code.len(), 16).unwrap();
    arena.writer(&mut cb).unwrap().write_at(0, &em.code);
    arena.commit().unwrap();
    // SAFETY: `tb` holds the thunk (committed); `Thunk` is its signature.
    let thunk: Thunk = unsafe { arena.entry(&tb) }.unwrap();
    let env = m.env();
    // SAFETY: the buffers in `m` are large enough; the target is a bound label of the committed block.
    let code = unsafe {
        thunk(
            env.thread,
            env.ci,
            env.frame,
            env.cnt,
            (cb.addr() + off) as *const u8,
        )
    };
    drop(arena);
    code
}

fn same(a: &Mem, b: &Mem, what: &str) {
    assert_eq!(a.frame, b.frame, "frame, {what}");
    assert_eq!(a.thread, b.thread, "thread, {what}");
    assert_eq!(a.ci, b.ci, "ci, {what}");
    assert_eq!(a.cnt, b.cnt, "budget, {what}");
}

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
}

const INTERESTING: [i64; 10] = [
    0,
    1,
    -1,
    2,
    7,
    -100,
    i64::MAX,
    i64::MAX - 1,
    i64::MIN,
    1 << 40,
];

fn random_frame(rng: &mut Rng) -> Mem {
    let mut m = Mem::new();
    let pick = |rng: &mut Rng| -> i64 {
        match rng.next() % 3 {
            0 => INTERESTING[(rng.next() % INTERESTING.len() as u64) as usize],
            1 => (rng.next() % 41) as i64 - 20,
            _ => rng.next() as i64,
        }
    };
    let fresh = if rng.next() & 1 == 0 {
        ci::CIST_FRESH
    } else {
        0
    };
    m.setup_ci(2 | fresh, 0);
    if rng.next().is_multiple_of(12) {
        // a float loop: tier 1 takes its float path, the trace must leave at its entry checks
        m.set_reg(0, LUA_VNUMFLT, ((rng.next() % 20) as f64).to_bits());
        m.set_reg(1, LUA_VNUMFLT, 1.0f64.to_bits());
        m.set_reg(2, LUA_VNUMFLT, 0.5f64.to_bits());
    } else {
        // `r0` holds the remaining iteration count (kept small)
        m.set_reg(0, LUA_VNUMINT, rng.next() % 30);
        m.set_reg(1, LUA_VNUMINT, pick(rng) as u64);
        m.set_reg(2, LUA_VNUMINT, pick(rng) as u64);
    }
    match rng.next() % 12 {
        0 => m.set_reg(4, LUA_VNUMFLT, 2.5f64.to_bits()),
        1 => m.set_reg(4, LUA_VNIL, 0),
        _ => m.set_reg(4, LUA_VNUMINT, pick(rng) as u64),
    }
    m.cnt = if rng.next() % 5 < 2 {
        (rng.next() % 16) as i32
    } else {
        i32::MAX
    };
    m
}

// ---- tests ---------------------------------------------------------------------------------------------------------

#[test]
fn count_model_of_the_slice() {
    let m = Mem::new();
    let low = lowered(m.irq.as_ptr());
    assert_eq!(low.label_kind(H), LabelKind::Head);
    assert_eq!(low.label_kind(2), LabelKind::Dead, "MMBIN follower");
    assert_eq!(low.label_kind(FORLOOP_PC), LabelKind::Mid);
    assert_eq!((low.cnt_at(H), low.rem(H), low.rem(FORLOOP_PC)), (2, 2, 1));
    assert_eq!((low.land(H), low.land(FORLOOP_PC)), (0, 1));
    assert_eq!(
        low.fetches(&[H, 2, FORLOOP_PC]),
        2,
        "followers are not fetched"
    );
}

#[test]
fn ir_is_well_formed() {
    let m = Mem::new();
    let low = lowered(m.irq.as_ptr());
    let (t, regs) = slice_ir(&low);
    assert_eq!(t.loop_at, Some(r(9)));
    assert_eq!(t.snaps.len(), 5);
    for i in &t.ins {
        for x in [i.a, i.b] {
            // every instruction operand that is a ref has a register (raw numbers of Sload/Budget excluded)
            if x >= REF_BIAS && !matches!(i.op, TOp::Sload | TOp::Budget) {
                assert!(regs.contains_key(&x) || t.at(x).op == TOp::Phi);
            }
        }
    }
    // the PHI refs and the registers agree (coalescing)
    for (phi, pre, body) in [(10, 5, 15), (11, 8, 18), (12, 7, 17)] {
        assert_eq!(regs[&r(phi)], regs[&r(pre)]);
        assert_eq!(regs[&r(phi)], regs[&r(body)]);
    }
}

/// Emulator: trace + tier-1 continuation equals tier 1 alone on 100 random frames; native equals the emulator.
#[test]
fn slice_matches_tier1_on_random_frames() {
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
    let (mut entered, mut left_early) = (0, 0);
    for case in 0..100 {
        let m = random_frame(&mut rng);
        // tier 1 alone, in the emulator
        let mut m_t1 = m.clone();
        let (low_t1, _) = with_trace(m_t1.irq.as_ptr());
        let l1 = tier1_label(&low_t1);
        let c1 = run_emu(&low_t1, &mut m_t1, l1);
        // the trace entry, in the emulator
        let mut m_tr = m.clone();
        let (low_tr, entry) = with_trace(m_tr.irq.as_ptr());
        let c_tr = run_emu(&low_tr, &mut m_tr, entry);
        assert_eq!(c1, c_tr, "exit code, case {case}");
        same(&m_t1, &m_tr, &format!("trace vs tier 1, case {case}"));
        assert_eq!(m_t1.savedpc(), m_tr.savedpc(), "savedpc, case {case}");
        let loops = m.frame[(PRE + ph(1) as usize) * 2] == u64::from(LUA_VNUMINT);
        if loops {
            entered += 1;
        } else {
            left_early += 1;
        }
        // native trace equals the emulated trace
        #[cfg(not(miri))]
        {
            let mut m_nat = m.clone();
            let (low_n, entry_n) = with_trace(m_nat.irq.as_ptr());
            let c_n = run_native(&low_n, &mut m_nat, entry_n);
            assert_eq!(c_tr, c_n, "native exit code, case {case}");
            same(&m_tr, &m_nat, &format!("native vs emulator, case {case}"));
            assert_eq!(
                m_tr.savedpc(),
                m_nat.savedpc(),
                "native savedpc, case {case}"
            );
        }
    }
    assert!(entered > 40 && left_early > 3, "{entered} {left_early}");
}

/// A pending trap makes the loop-top poll leave: the state is that of tier 1 after exactly two iterations (the
/// pre-roll's and the one tier 1 runs from the poll's exit before its own back-edge poll), with the budget accounted.
#[test]
fn poll_exit_lands_in_tier1_with_exact_budget() {
    for trap in [1, 0] {
        for irq in [0u8, 1] {
            if trap == 0 && irq == 0 {
                continue;
            }
            let mut base = Mem::new();
            base.setup_ci(2, 0);
            base.set_reg(0, LUA_VNUMINT, 10);
            base.set_reg(1, LUA_VNUMINT, 3);
            base.set_reg(2, LUA_VNUMINT, 1);
            base.set_reg(4, LUA_VNUMINT, 100);
            base.cnt = 4 * N_ITER as i32;
            // trace with the pending trap / interrupt
            let mut m_tr = base.clone();
            Mem::put(&mut m_tr.ci, ci::TRAP, trap);
            m_tr.irq[0] = irq;
            let (low, entry) = with_trace(m_tr.irq.as_ptr());
            let c_tr = run_emu(&low, &mut m_tr, entry);
            // tier 1 alone with a budget of two iterations stops after the same two iterations
            let mut m_t1 = base.clone();
            m_t1.cnt = 2 * N_ITER as i32;
            let (low1, _) = with_trace(m_t1.irq.as_ptr());
            let c1 = run_emu(&low1, &mut m_t1, tier1_label(&low1));
            assert_eq!(c_tr, c1, "trap {trap} irq {irq}");
            assert_eq!(m_tr.frame, m_t1.frame, "trap {trap} irq {irq}");
            assert_eq!(m_tr.savedpc(), m_t1.savedpc());
            // the trace was entered with 4 iterations worth of budget and two ran: two remain. Tier 1 alone with a
            // budget of two iterations stops at the next head with exactly nothing left (P0y: a block-count exit
            // refunds the pre-paid block, so the counter holds the exact budget left).
            assert_eq!(m_tr.cnt, 2 * N_ITER as i32);
            assert_eq!(m_t1.cnt, 0);
            // native agrees
            #[cfg(not(miri))]
            {
                let mut m_nat = base.clone();
                Mem::put(&mut m_nat.ci, ci::TRAP, trap);
                m_nat.irq[0] = irq;
                let (low_n, entry_n) = with_trace(m_nat.irq.as_ptr());
                assert_eq!(run_native(&low_n, &mut m_nat, entry_n), c_tr);
                same(&m_tr, &m_nat, "poll exit, native vs emulator");
            }
        }
    }
}

/// The sum loop run to completion: `s + sum(i)` with wrapping arithmetic, through the trace.
#[test]
fn slice_computes_the_sum() {
    let mut m = Mem::new();
    m.setup_ci(2, 0);
    // for i = 1, 1000: r0 = 999 remaining after the first iteration, r2 = 1
    m.set_reg(0, LUA_VNUMINT, 999);
    m.set_reg(1, LUA_VNUMINT, 1);
    m.set_reg(2, LUA_VNUMINT, 1);
    m.set_reg(4, LUA_VNUMINT, 0);
    let (low, entry) = with_trace(m.irq.as_ptr());
    let c = run_emu(&low, &mut m, entry);
    assert_eq!(c, 1, "RETURN1 pops the frame");
    // RETURN1 moved r4 into the function slot (the slot below the base)
    assert_eq!(m.frame[(PRE - 1) * 2 + 1], 500_500);
    assert_eq!(
        m.frame[(PRE + CTL as usize) * 2 + 1],
        1000,
        "control variable"
    );
}
